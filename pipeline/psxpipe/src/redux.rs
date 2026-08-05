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
}
