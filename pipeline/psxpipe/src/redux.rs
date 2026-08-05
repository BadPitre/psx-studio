//! Client minimal de l'API web de PCSX-Redux, pour le Play Mode de
//! l'éditeur. HTTP/1.1 sur TcpStream, zéro dépendance réseau.
//!
//! Routes vérifiées dans les sources de PCSX-Redux (src/core/web-server.cc) :
//! - GET  /api/v1/execution-flow            → JSON { running, ... }
//! - POST /api/v1/execution-flow?function=pause|resume|start
//!   (reset : function=reset&type=hard|soft)
//! - GET  /api/v1/cpu/ram/raw               → dump RAM complet (2 Mo)
//! - POST /api/v1/cpu/ram/raw?offset=N&size=N (+ corps) → écriture RAM
//!
//! Lancement de l'émulateur : `pcsx-redux -run -loadiso <cue> -webserver
//! -webserver-port <port>` (port par défaut : 8080).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

pub const DEFAULT_PORT: u16 = 8080;

pub struct ReduxClient {
    pub host: String,
    pub port: u16,
    pub timeout: Duration,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ExecutionStatus {
    pub running: bool,
}

/// Arguments de lancement de PCSX-Redux pour le Play Mode.
pub fn launch_args(cue_path: &str, port: u16) -> Vec<String> {
    vec![
        "-run".into(),
        "-stdout".into(),
        "-loadiso".into(),
        cue_path.into(),
        "-webserver".into(),
        "-webserver-port".into(),
        port.to_string(),
    ]
}

impl ReduxClient {
    pub fn new(port: u16) -> ReduxClient {
        ReduxClient {
            host: "127.0.0.1".into(),
            port,
            timeout: Duration::from_secs(3),
        }
    }

    fn request(&self, method: &str, path: &str, body: &[u8]) -> Result<(u16, Vec<u8>), String> {
        let addr = format!("{}:{}", self.host, self.port);
        let mut stream = TcpStream::connect(&addr)
            .map_err(|e| format!("PCSX-Redux injoignable sur {addr}: {e}"))?;
        stream.set_read_timeout(Some(self.timeout)).ok();
        stream.set_write_timeout(Some(self.timeout)).ok();

        let head = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
            self.host,
            body.len()
        );
        stream.write_all(head.as_bytes()).map_err(|e| e.to_string())?;
        stream.write_all(body).map_err(|e| e.to_string())?;

        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).map_err(|e| e.to_string())?;

        let header_end = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or("réponse HTTP invalide")?;
        let head = std::str::from_utf8(&raw[..header_end]).map_err(|e| e.to_string())?;
        let status: u16 = head
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .ok_or("ligne de statut HTTP invalide")?;
        Ok((status, raw[header_end + 4..].to_vec()))
    }

    fn expect_ok(&self, method: &str, path: &str, body: &[u8]) -> Result<Vec<u8>, String> {
        let (status, response) = self.request(method, path, body)?;
        if status != 200 {
            return Err(format!("{method} {path}: HTTP {status}"));
        }
        Ok(response)
    }

    pub fn status(&self) -> Result<ExecutionStatus, String> {
        let body = self.expect_ok("GET", "/api/v1/execution-flow", &[])?;
        let json: serde_json::Value =
            serde_json::from_slice(&body).map_err(|e| format!("JSON invalide: {e}"))?;
        Ok(ExecutionStatus {
            running: json["running"].as_bool().unwrap_or(false),
        })
    }

    pub fn pause(&self) -> Result<(), String> {
        self.expect_ok("POST", "/api/v1/execution-flow?function=pause", &[])?;
        Ok(())
    }

    pub fn resume(&self) -> Result<(), String> {
        self.expect_ok("POST", "/api/v1/execution-flow?function=resume", &[])?;
        Ok(())
    }

    pub fn reset(&self) -> Result<(), String> {
        self.expect_ok("POST", "/api/v1/execution-flow?function=reset&type=hard", &[])?;
        Ok(())
    }

    /// Dump complet de la RAM console (2 Mo, 8 Mo si le devkit est activé).
    pub fn read_ram(&self) -> Result<Vec<u8>, String> {
        self.expect_ok("GET", "/api/v1/cpu/ram/raw", &[])
    }

    /// Écriture directe en RAM console (base du live tweaking).
    pub fn write_ram(&self, offset: u32, data: &[u8]) -> Result<(), String> {
        let path = format!("/api/v1/cpu/ram/raw?offset={offset}&size={}", data.len());
        self.expect_ok("POST", &path, data)?;
        Ok(())
    }
}

/* --------------------------------------------------------- live tweak -- */

/// Balise écrite par le runtime (engine/scene.c) : localisable dans un
/// dump RAM par son magic, elle décrit la table d'entités pour piloter
/// les transforms depuis l'éditeur pendant que le jeu tourne.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Beacon {
    pub entity_size: u16,
    /// Adresse console (KSEG) de entities[0].
    pub entities_addr: u32,
    pub entity_count: u16,
    pub pos_offset: u16,
    pub rot_offset: u16,
    pub scale_offset: u16,
}

const BEACON_MAGIC: &[u8; 12] = b"PSXSTUDIOBCN";

/// Cherche la balise dans un dump RAM complet.
pub fn find_beacon(ram: &[u8]) -> Option<Beacon> {
    let pos = ram
        .windows(BEACON_MAGIC.len())
        .position(|w| w == BEACON_MAGIC)?;
    let b = &ram[pos..];
    if b.len() < 28 {
        return None;
    }
    let u16at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
    let version = u16at(12);
    if version != 1 {
        return None;
    }
    Some(Beacon {
        entity_size: u16at(14),
        entities_addr: u32::from_le_bytes(b[16..20].try_into().unwrap()),
        entity_count: u16at(20),
        pos_offset: u16at(22),
        rot_offset: u16at(24),
        scale_offset: u16at(26),
    })
}

impl ReduxClient {
    /// Dump la RAM et localise la balise du runtime.
    pub fn locate_beacon(&self) -> Result<Beacon, String> {
        let ram = self.read_ram()?;
        find_beacon(&ram).ok_or_else(|| {
            "balise PSX Studio introuvable en RAM (le jeu tourne-t-il ?)".into()
        })
    }

    /// Écrit la transform locale d'une entité (live tweaking). La rotation
    /// est en unités PS1 (4096 = tour), l'échelle en 4.12.
    pub fn write_entity_transform(
        &self,
        beacon: &Beacon,
        index: u16,
        pos: [i32; 3],
        rot: [i16; 3],
        scale: [i16; 3],
    ) -> Result<(), String> {
        if index >= beacon.entity_count {
            return Err(format!(
                "entité {index} hors limite ({} dans la scène)",
                beacon.entity_count
            ));
        }
        // Adresse KSEG -> offset dans la RAM physique (2 Mo miroités).
        let base = (beacon.entities_addr & 0x001f_ffff)
            + index as u32 * beacon.entity_size as u32;

        let mut pos_bytes = Vec::with_capacity(12);
        for c in pos {
            pos_bytes.extend_from_slice(&c.to_le_bytes());
        }
        self.write_ram(base + beacon.pos_offset as u32, &pos_bytes)?;

        let mut rot_bytes = Vec::with_capacity(6);
        for c in rot {
            rot_bytes.extend_from_slice(&c.to_le_bytes());
        }
        self.write_ram(base + beacon.rot_offset as u32, &rot_bytes)?;

        let mut scale_bytes = Vec::with_capacity(6);
        for c in scale {
            scale_bytes.extend_from_slice(&c.to_le_bytes());
        }
        self.write_ram(base + beacon.scale_offset as u32, &scale_bytes)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    /// Faux PCSX-Redux : répond aux routes execution-flow comme le vrai.
    fn fake_redux() -> (u16, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for _ in 0..3 {
                let (stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                seen.push(line.trim().to_string());
                // Consommer les en-têtes.
                loop {
                    let mut h = String::new();
                    reader.read_line(&mut h).unwrap();
                    if h == "\r\n" || h.is_empty() {
                        break;
                    }
                }
                let mut stream = reader.into_inner();
                if line.starts_with("GET /api/v1/execution-flow") {
                    let body = r#"{"running":true,"isDynarec":false}"#;
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                        body.len()
                    )
                    .unwrap();
                } else {
                    write!(stream, "HTTP/1.1 200 OK\r\n\r\n").unwrap();
                }
            }
            seen
        });
        (port, handle)
    }

    #[test]
    fn client_talks_to_execution_flow() {
        let (port, server) = fake_redux();
        let client = ReduxClient::new(port);
        assert_eq!(client.status().unwrap(), ExecutionStatus { running: true });
        client.pause().unwrap();
        client.resume().unwrap();
        let seen = server.join().unwrap();
        assert_eq!(seen[0], "GET /api/v1/execution-flow HTTP/1.1");
        assert_eq!(seen[1], "POST /api/v1/execution-flow?function=pause HTTP/1.1");
        assert_eq!(seen[2], "POST /api/v1/execution-flow?function=resume HTTP/1.1");
    }

    #[test]
    fn launch_args_match_redux_cli() {
        let args = launch_args("Build/demo.cue", 8080);
        assert!(args.contains(&"-loadiso".to_string()));
        assert!(args.contains(&"-webserver".to_string()));
        assert_eq!(args.last().unwrap(), "8080");
    }

    #[test]
    fn unreachable_emulator_is_a_clean_error() {
        // Port 1 : rien n'écoute.
        let client = ReduxClient::new(1);
        let err = client.status().unwrap_err();
        assert!(err.contains("injoignable"), "{err}");
    }

    /// Construit un faux dump RAM avec une balise à l'offset donné.
    fn ram_with_beacon(offset: usize, version: u16) -> Vec<u8> {
        let mut ram = vec![0u8; 64 * 1024];
        let b = &mut ram[offset..];
        b[..12].copy_from_slice(BEACON_MAGIC);
        b[12..14].copy_from_slice(&version.to_le_bytes());
        b[14..16].copy_from_slice(&112u16.to_le_bytes()); // entity_size
        b[16..20].copy_from_slice(&0x8003_4a20u32.to_le_bytes()); // entities_addr
        b[20..22].copy_from_slice(&7u16.to_le_bytes()); // entity_count
        b[22..24].copy_from_slice(&0u16.to_le_bytes()); // pos_offset
        b[24..26].copy_from_slice(&12u16.to_le_bytes()); // rot_offset
        b[26..28].copy_from_slice(&20u16.to_le_bytes()); // scale_offset
        ram
    }

    #[test]
    fn beacon_found_in_ram_dump() {
        let ram = ram_with_beacon(0x1234, 1);
        let b = find_beacon(&ram).expect("balise non trouvée");
        assert_eq!(
            b,
            Beacon {
                entity_size: 112,
                entities_addr: 0x8003_4a20,
                entity_count: 7,
                pos_offset: 0,
                rot_offset: 12,
                scale_offset: 20,
            }
        );
    }

    #[test]
    fn beacon_absent_or_wrong_version_is_none() {
        assert_eq!(find_beacon(&vec![0u8; 4096]), None);
        assert_eq!(find_beacon(&ram_with_beacon(64, 2)), None);
    }

    #[test]
    fn write_entity_transform_targets_physical_ram() {
        let ram = ram_with_beacon(0, 1);
        let beacon = find_beacon(&ram).unwrap();
        // Adresse KSEG 0x80034a20 -> physique 0x34a20 ; entité 2.
        let base = 0x34a20 + 2 * 112;

        let (port, server) = fake_redux();
        let client = ReduxClient::new(port);
        client
            .write_entity_transform(&beacon, 2, [10, -20, 30], [0, 1024, 0], [4096, 4096, 4096])
            .unwrap();
        let seen = server.join().unwrap();
        assert_eq!(
            seen[0],
            format!("POST /api/v1/cpu/ram/raw?offset={base}&size=12 HTTP/1.1")
        );
        assert_eq!(
            seen[1],
            format!("POST /api/v1/cpu/ram/raw?offset={}&size=6 HTTP/1.1", base + 12)
        );
        assert_eq!(
            seen[2],
            format!("POST /api/v1/cpu/ram/raw?offset={}&size=6 HTTP/1.1", base + 20)
        );

        // Index hors limites : erreur claire, aucune requête.
        let err = client
            .write_entity_transform(&beacon, 7, [0; 3], [0; 3], [0; 3])
            .unwrap_err();
        assert!(err.contains("hors limite"), "{err}");
    }
}
