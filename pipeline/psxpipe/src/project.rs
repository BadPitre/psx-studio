//! `psxpipe build`: whole-project build, from sources to a bootable ISO.
//!
//! A project directory contains a `project.json` describing sources
//! (glTF, PNG, WAV), scenes and the PS-EXE built by CMake. The build:
//! 1. converts changed assets into `Library/` (content-hash cache),
//! 2. packs every scene into `Build/SCENEn.PSC` (VRAM auto-packed),
//! 3. generates `Build/iso.xml` + `SYSTEM.CNF`,
//! 4. invokes `mkpsxiso` to produce the final `.bin/.cue`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{gltf_import, scene, tim, vag};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectJson {
    pub name: String,
    /// Path (relative to project.json) of the PS-EXE built by CMake.
    pub exe: String,
    #[serde(default)]
    pub models: Vec<ModelSrc>,
    #[serde(default)]
    pub textures: Vec<TextureSrc>,
    /// Scene JSONs; their asset paths resolve in Library/ first.
    pub scenes: Vec<String>,
    #[serde(default)]
    pub sfx: Vec<SfxSrc>,
    /// WAV files (stereo 44.1 kHz) appended as CD-DA audio tracks.
    #[serde(default)]
    pub music: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelSrc {
    pub gltf: String,
    /// Output name in Library/ (e.g. "house.pmd").
    pub out: String,
    #[serde(default)]
    pub size: Option<i32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextureSrc {
    pub png: String,
    pub out: String,
    #[serde(default = "default_bpp")]
    pub bpp: u32,
}

fn default_bpp() -> u32 {
    8
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SfxSrc {
    pub wav: String,
    /// ISO file name, 8.3 uppercase (e.g. "BLIP.VAG").
    pub out: String,
}

#[derive(Debug, Default)]
pub struct BuildReport {
    pub converted: usize,
    pub cached: usize,
    pub scenes: Vec<scene::SceneReport>,
    pub iso_xml: PathBuf,
    pub mkpsxiso_ran: bool,
    pub warnings: Vec<String>,
}

/* ---------------------------------------------------------------- cache -- */

fn fnv1a64(data: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

struct Cache {
    path: PathBuf,
    entries: HashMap<String, String>,
    dirty: bool,
}

impl Cache {
    fn load(library: &Path) -> Cache {
        let path = library.join("manifest.json");
        let entries = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Cache {
            path,
            entries,
            dirty: false,
        }
    }

    /// True when `out` is up to date for the given source contents.
    fn is_fresh(&self, key: &str, hash: &str, out: &Path) -> bool {
        out.exists() && self.entries.get(key).is_some_and(|h| h == hash)
    }

    fn record(&mut self, key: String, hash: String) {
        self.entries.insert(key, hash);
        self.dirty = true;
    }

    fn save(&self) -> Result<(), String> {
        if !self.dirty {
            return Ok(());
        }
        let json = serde_json::to_string_pretty(&self.entries).map_err(|e| e.to_string())?;
        std::fs::write(&self.path, json).map_err(|e| e.to_string())
    }
}

fn hash_file(path: &Path) -> Result<String, String> {
    let data =
        std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    Ok(format!("{:016x}", fnv1a64(&data)))
}

/// Hash a glTF file together with any external .bin buffer next to it.
fn hash_gltf(path: &Path) -> Result<String, String> {
    let mut h = hash_file(path)?;
    if let Some(stem) = path.file_stem() {
        let bin = path.with_file_name(format!("{}.bin", stem.to_string_lossy()));
        if bin.exists() {
            h.push_str(&hash_file(&bin)?);
        }
    }
    Ok(h)
}

/* ---------------------------------------------------------------- build -- */

fn check_iso_name(name: &str) -> Result<(), String> {
    let valid = name.len() <= 12
        && name.chars().filter(|&c| c == '.').count() == 1
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '.' || c == '_');
    if !valid {
        return Err(format!(
            "'{name}' is not a valid ISO file name (8.3, uppercase, e.g. BLIP.VAG)"
        ));
    }
    Ok(())
}

pub fn build(project_dir: &Path, force: bool) -> Result<BuildReport, String> {
    let json_path = project_dir.join("project.json");
    let text = std::fs::read_to_string(&json_path)
        .map_err(|e| format!("cannot read {}: {e}", json_path.display()))?;
    let project: ProjectJson =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", json_path.display()))?;

    let library = project_dir.join("Library");
    let build_dir = project_dir.join("Build");
    std::fs::create_dir_all(&library).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&build_dir).map_err(|e| e.to_string())?;
    let mut cache = Cache::load(&library);
    let mut report = BuildReport::default();

    let exe_path = project_dir.join(&project.exe);
    if !exe_path.exists() {
        return Err(format!(
            "executable {} not found — build the runtime first (cmake --build)",
            exe_path.display()
        ));
    }

    /* 1. Convert assets into Library/ (skip fresh ones). */
    for model in &project.models {
        let src = project_dir.join(&model.gltf);
        let out = library.join(&model.out);
        let hash = format!("{}:{}", hash_gltf(&src)?, model.size.unwrap_or(128));
        if !force && cache.is_fresh(&model.gltf, &hash, &out) {
            report.cached += 1;
            continue;
        }
        let opts = gltf_import::ImportOptions {
            target_size: model.size.unwrap_or(128),
            ..Default::default()
        };
        let (pmd, import_report) = gltf_import::import(&src, &opts)?;
        std::fs::write(&out, pmd.write()?).map_err(|e| e.to_string())?;
        for w in import_report.warnings {
            report.warnings.push(format!("{}: {w}", model.gltf));
        }
        cache.record(model.gltf.clone(), hash);
        report.converted += 1;
    }

    for texture in &project.textures {
        let src = project_dir.join(&texture.png);
        let out = library.join(&texture.out);
        let hash = format!("{}:{}", hash_file(&src)?, texture.bpp);
        if !force && cache.is_fresh(&texture.png, &hash, &out) {
            report.cached += 1;
            continue;
        }
        let bpp = match texture.bpp {
            4 => tim::Bpp::Four,
            8 => tim::Bpp::Eight,
            16 => tim::Bpp::Sixteen,
            other => return Err(format!("{}: unsupported bpp {other}", texture.png)),
        };
        let img = image::open(&src)
            .map_err(|e| format!("{}: {e}", texture.png))?
            .to_rgba8();
        let (w, h) = img.dimensions();
        let opts = tim::TimOptions {
            bpp,
            ..Default::default()
        };
        let (timg, tim_report) = tim::encode(img.as_raw(), w, h, &opts)?;
        std::fs::write(&out, timg.write()).map_err(|e| e.to_string())?;
        for w in tim_report.warnings {
            // Placement warnings are moot: the scene packer re-places TIMs.
            if !w.contains("framebuffer") {
                report.warnings.push(format!("{}: {w}", texture.png));
            }
        }
        cache.record(texture.png.clone(), hash);
        report.converted += 1;
    }

    let mut sfx_files = Vec::new();
    for sfx in &project.sfx {
        check_iso_name(&sfx.out)?;
        let src = project_dir.join(&sfx.wav);
        let out = build_dir.join(&sfx.out);
        let hash = hash_file(&src)?;
        sfx_files.push(sfx.out.clone());
        if !force && cache.is_fresh(&sfx.wav, &hash, &out) {
            report.cached += 1;
            continue;
        }
        let name = sfx.out.split('.').next().unwrap_or("sample").to_lowercase();
        let (vag_bytes, vag_report) = vag::wav_to_vag(&src, &name)?;
        std::fs::write(&out, &vag_bytes).map_err(|e| e.to_string())?;
        for w in vag_report.warnings {
            report.warnings.push(format!("{}: {w}", sfx.wav));
        }
        cache.record(sfx.wav.clone(), hash);
        report.converted += 1;
    }

    /* 2. Pack scenes (always rebuilt: they aggregate many inputs and the
     * packing itself is fast). Asset paths resolve in Library/. */
    for (i, scene_rel) in project.scenes.iter().enumerate() {
        let scene_path = project_dir.join(scene_rel);
        let scene_text = std::fs::read_to_string(&scene_path)
            .map_err(|e| format!("cannot read {}: {e}", scene_path.display()))?;
        let scene_json: scene::SceneJson =
            serde_json::from_str(&scene_text).map_err(|e| format!("{scene_rel}: {e}"))?;
        let (bytes, scene_report) =
            scene::build_with_options(&scene_json, &library, &scene::BuildOptions::default())?;
        std::fs::write(build_dir.join(format!("SCENE{i}.PSC")), &bytes)
            .map_err(|e| e.to_string())?;

        let (reqs, places): (Vec<_>, Vec<_>) = scene_report.vram.iter().cloned().unzip();
        crate::vram::render_map(&reqs, &places)
            .save(build_dir.join(format!("vram-scene{i}.png")))
            .map_err(|e| e.to_string())?;
        report.scenes.push(scene_report);
    }

    /* 3. SYSTEM.CNF + iso.xml. */
    let exe_iso_name = exe_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_uppercase();
    check_iso_name(&exe_iso_name)?;
    std::fs::write(
        build_dir.join("SYSTEM.CNF"),
        format!("BOOT=cdrom:\\{exe_iso_name};1\r\nTCB=4\r\nEVENT=10\r\nSTACK=801FFFF0\r\n"),
    )
    .map_err(|e| e.to_string())?;
    std::fs::copy(&exe_path, build_dir.join(&exe_iso_name)).map_err(|e| e.to_string())?;

    let volume = project
        .name
        .to_uppercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .take(16)
        .collect::<String>();
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<iso_project>\n");
    xml.push_str("\t<track type=\"data\">\n");
    xml.push_str(&format!(
        "\t\t<identifiers system=\"PLAYSTATION\" volume=\"{volume}\" volume_set=\"{volume}\" publisher=\"PSXSTUDIO\" application=\"PLAYSTATION\" />\n"
    ));
    xml.push_str("\t\t<directory_tree>\n");
    xml.push_str("\t\t\t<file name=\"SYSTEM.CNF\" type=\"data\" source=\"SYSTEM.CNF\" />\n");
    xml.push_str(&format!(
        "\t\t\t<file name=\"{exe_iso_name}\" type=\"data\" source=\"{exe_iso_name}\" />\n"
    ));
    for i in 0..project.scenes.len() {
        xml.push_str(&format!(
            "\t\t\t<file name=\"SCENE{i}.PSC\" type=\"data\" source=\"SCENE{i}.PSC\" />\n"
        ));
    }
    for name in &sfx_files {
        xml.push_str(&format!(
            "\t\t\t<file name=\"{name}\" type=\"data\" source=\"{name}\" />\n"
        ));
    }
    xml.push_str("\t\t\t<dummy sectors=\"1024\"/>\n");
    xml.push_str("\t\t</directory_tree>\n\t</track>\n");
    for wav in &project.music {
        let src = project_dir.join(wav);
        if !src.exists() {
            return Err(format!("music file {} not found", src.display()));
        }
        // mkpsxiso resolves audio sources relative to the XML file.
        let rel = pathdiff_simple(&build_dir, &src);
        xml.push_str(&format!("\t<track type=\"audio\" source=\"{rel}\" />\n"));
    }
    xml.push_str("</iso_project>\n");
    let iso_xml = build_dir.join("iso.xml");
    std::fs::write(&iso_xml, xml).map_err(|e| e.to_string())?;
    report.iso_xml = iso_xml.clone();

    cache.save()?;

    /* 4. mkpsxiso. */
    let bin = format!("{}.bin", project.name);
    let cue = format!("{}.cue", project.name);
    let status = std::process::Command::new("mkpsxiso")
        .current_dir(&build_dir)
        .args(["-y", "-o", &bin, "-c", &cue, "iso.xml"])
        .status();
    match status {
        Ok(s) if s.success() => report.mkpsxiso_ran = true,
        Ok(s) => return Err(format!("mkpsxiso failed with status {s}")),
        Err(_) => report.warnings.push(format!(
            "mkpsxiso not found in PATH — run manually: cd {} && mkpsxiso -y -o {bin} -c {cue} iso.xml",
            build_dir.display()
        )),
    }

    Ok(report)
}

/// Best-effort relative path from `from` dir to `to` (falls back to
/// absolute), for the generated iso.xml.
fn pathdiff_simple(from: &Path, to: &Path) -> String {
    let from = from.canonicalize().unwrap_or_else(|_| from.to_path_buf());
    let to = to.canonicalize().unwrap_or_else(|_| to.to_path_buf());
    let from_parts: Vec<_> = from.components().collect();
    let to_parts: Vec<_> = to.components().collect();
    let common = from_parts
        .iter()
        .zip(&to_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let mut rel = PathBuf::new();
    for _ in common..from_parts.len() {
        rel.push("..");
    }
    for part in &to_parts[common..] {
        rel.push(part);
    }
    rel.to_string_lossy().replace('\\', "/")
}
