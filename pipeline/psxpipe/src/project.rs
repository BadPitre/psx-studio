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
    /// Polices bitmap UI (v1.3).
    #[serde(default)]
    pub fonts: Vec<FontSrc>,
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
    /// Dimensions de la texture associée (mapping des UV, défaut 256).
    #[serde(default)]
    pub tex_w: Option<u32>,
    #[serde(default)]
    pub tex_h: Option<u32>,
    /// Subdivision anti-warping : longueur d'arête max en unités PMD
    /// (absent = désactivée). Borne la distorsion affine des textures.
    #[serde(default)]
    pub subdiv: Option<f32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextureSrc {
    pub png: String,
    pub out: String,
    /// 4, 8 ou 16 bpp ; absent = automatique (4 bpp si l'image tient en
    /// 16 couleurs — moitié de VRAM gagnée sans perte, sinon 8 bpp).
    #[serde(default)]
    pub bpp: Option<u32>,
}

/// Police bitmap UI : PNG en grille régulière -> .fnt (atlas TIM 4bpp +
/// chasses mesurées). Défauts alignés sur la police de démo.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontSrc {
    pub png: String,
    pub out: String,
    /// [largeur, hauteur] de cellule (défaut [6, 8]).
    #[serde(default)]
    pub cell: Option<[u8; 2]>,
    /// Premier caractère ASCII (défaut 32 = espace).
    #[serde(default)]
    pub first: Option<u8>,
    /// Nombre de glyphes (défaut 59 = espace..Z).
    #[serde(default)]
    pub count: Option<u8>,
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
        let hash = format!(
            "{}:{}:{}x{}:s{}",
            hash_gltf(&src)?,
            model.size.unwrap_or(128),
            model.tex_w.unwrap_or(256),
            model.tex_h.unwrap_or(256),
            model.subdiv.unwrap_or(0.0)
        );
        if !force && cache.is_fresh(&model.gltf, &hash, &out) {
            report.cached += 1;
            continue;
        }
        let opts = gltf_import::ImportOptions {
            target_size: model.size.unwrap_or(128),
            tex_w: model.tex_w.unwrap_or(256),
            tex_h: model.tex_h.unwrap_or(256),
            subdiv: model.subdiv,
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
        let hash = format!(
            "{}:{}",
            hash_file(&src)?,
            texture.bpp.map_or("auto".into(), |b| b.to_string())
        );
        if !force && cache.is_fresh(&texture.png, &hash, &out) {
            report.cached += 1;
            continue;
        }
        let img = image::open(&src)
            .map_err(|e| format!("{}: {e}", texture.png))?
            .to_rgba8();
        let bpp = match texture.bpp {
            Some(4) => tim::Bpp::Four,
            Some(8) => tim::Bpp::Eight,
            Some(16) => tim::Bpp::Sixteen,
            None => tim::auto_bpp(img.as_raw()),
            Some(other) => {
                return Err(format!("{}: unsupported bpp {other}", texture.png))
            }
        };
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

    for font in &project.fonts {
        let src = project_dir.join(&font.png);
        let out = library.join(&font.out);
        let cell = font.cell.unwrap_or([6, 8]);
        let (first, count) = (font.first.unwrap_or(32), font.count.unwrap_or(59));
        let hash = format!("{}:{}x{}:{first}:{count}", hash_file(&src)?, cell[0], cell[1]);
        if !force && cache.is_fresh(&font.png, &hash, &out) {
            report.cached += 1;
            continue;
        }
        let img = image::open(&src)
            .map_err(|e| format!("{}: {e}", font.png))?
            .to_rgba8();
        let (w, h) = img.dimensions();
        let fnt = crate::fnt::encode(img.as_raw(), w, h, cell[0], cell[1], first, count)
            .map_err(|e| format!("{}: {e}", font.png))?;
        std::fs::write(&out, fnt).map_err(|e| e.to_string())?;
        cache.record(font.png.clone(), hash);
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

/* --------------------------------------------------------- import UI -- */

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub enum AssetKind {
    Model,
    Texture,
}

#[derive(Debug, serde::Serialize)]
pub struct ImportedAsset {
    pub kind: AssetKind,
    /// Identifiant dérivé du nom de fichier (minuscules, alphanumérique).
    pub id: String,
    /// Nom de sortie dans Library/ (ex. "house.pmd").
    pub out: String,
    /// Texture extraite du glTF/GLB et convertie avec le modèle
    /// (ex. "house.tim"), à appairer par l'éditeur.
    pub texture_out: Option<String>,
    pub summary: String,
    pub warnings: Vec<String>,
}

/// Copie vers assets/ en tolérant un fichier déjà à sa place (import
/// depuis le panneau Project) : se copier sur soi-même tronque le fichier.
fn copy_into_assets(src: &Path, dst: &Path) -> Result<(), String> {
    if let (Ok(a), Ok(b)) = (src.canonicalize(), dst.canonicalize()) {
        if a == b {
            return Ok(());
        }
    }
    std::fs::copy(src, dst)
        .map(|_| ())
        .map_err(|e| format!("copie : {e}"))
}

fn sanitize_id(stem: &str) -> String {
    let id: String = stem
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if id.is_empty() { "asset".into() } else { id }
}

/// Importe un fichier source (glTF/GLB/PNG) dans un projet : copie dans
/// assets/, enregistrement dans project.json (idempotent), conversion
/// immédiate dans Library/. Utilisé par le drag & drop de l'éditeur.
pub fn import_asset(project_dir: &Path, src: &Path) -> Result<ImportedAsset, String> {
    let ext = src
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .ok_or("nom de fichier invalide")?;
    let id = sanitize_id(&stem);

    let assets_dir = project_dir.join("assets");
    std::fs::create_dir_all(&assets_dir).map_err(|e| e.to_string())?;
    let library = project_dir.join("Library");
    std::fs::create_dir_all(&library).map_err(|e| e.to_string())?;

    let json_path = project_dir.join("project.json");
    let text = std::fs::read_to_string(&json_path)
        .map_err(|e| format!("{} : {e}", json_path.display()))?;
    let mut project: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("project.json : {e}"))?;

    // Fichier déjà dans le projet (import depuis le panneau Project) : il
    // reste à sa place et on enregistre son chemin réel — le rangement de
    // l'utilisateur est respecté.
    let project_canon = project_dir.canonicalize().map_err(|e| e.to_string())?;
    let in_project_rel = src.canonicalize().ok().and_then(|s| {
        s.strip_prefix(&project_canon)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
    });

    let register = |list: &mut serde_json::Value, entry: serde_json::Value, out_key: &str| {
        if !list.is_array() {
            *list = serde_json::Value::Array(Vec::new());
        }
        let arr = list.as_array_mut().unwrap();
        if !arr.iter().any(|e| e["out"] == entry["out"]) {
            arr.push(entry);
        } else {
            // Déjà enregistré : la reconversion suffira.
            let _ = out_key;
        }
    };

    let result = match ext.as_str() {
        "gltf" | "glb" => {
            let file_name = format!("{id}.{ext}");
            let (dst, gltf_rel) = match &in_project_rel {
                Some(rel) => (src.to_path_buf(), rel.clone()),
                None => {
                    let dst = assets_dir.join(&file_name);
                    copy_into_assets(src, &dst)?;
                    // Buffer externe d'un .gltf : copié sous son nom
                    // D'ORIGINE (le JSON du glTF le référence par ce nom
                    // exact dans son URI).
                    if ext == "gltf" {
                        let bin = src.with_file_name(format!("{stem}.bin"));
                        if bin.exists() {
                            copy_into_assets(&bin, &assets_dir.join(format!("{stem}.bin")))?;
                        }
                    }
                    (dst, format!("assets/{file_name}"))
                }
            };
            // Dossier où atterrit une éventuelle texture extraite : celui
            // du glTF (l'import sur place respecte le rangement).
            let tex_dir_rel = gltf_rel
                .rsplit_once('/')
                .map(|(d, _)| d.to_string())
                .unwrap_or_else(|| "assets".into());
            let mut warnings = Vec::new();

            /* Texture baseColor embarquée (.glb) ou référencée (.gltf) :
             * extraite, réduite à 256 max, convertie en TIM et
             * enregistrée avec le modèle. */
            let mut texture_out = None;
            let mut tex_dims = (256u32, 256u32);
            if let Some((rgba, w, h)) = gltf_import::extract_base_color_rgba(&dst)? {
                let mut img: image::RgbaImage = image::ImageBuffer::from_raw(w, h, rgba)
                    .ok_or("texture glTF : buffer invalide")?;
                if w > 256 || h > 256 {
                    let scale = 256.0 / w.max(h) as f32;
                    let (nw, nh) = (
                        ((w as f32 * scale) as u32).max(1),
                        ((h as f32 * scale) as u32).max(1),
                    );
                    warnings.push(format!(
                        "texture {w}x{h} réduite à {nw}x{nh} (une page = 256x256 max)"
                    ));
                    img = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Lanczos3);
                }
                tex_dims = img.dimensions();
                img.save(project_dir.join(&tex_dir_rel).join(format!("{id}.png")))
                    .map_err(|e| e.to_string())?;
                let tim_out = format!("{id}.tim");
                let tim_opts = tim::TimOptions {
                    bpp: tim::auto_bpp(img.as_raw()),
                    ..Default::default()
                };
                let (timg, tim_report) =
                    tim::encode(img.as_raw(), tex_dims.0, tex_dims.1, &tim_opts)?;
                std::fs::write(library.join(&tim_out), timg.write()).map_err(|e| e.to_string())?;
                warnings.extend(
                    tim_report
                        .warnings
                        .into_iter()
                        .filter(|w| !w.contains("framebuffer")),
                );
                register(
                    &mut project["textures"],
                    serde_json::json!({ "png": format!("{tex_dir_rel}/{id}.png"), "out": tim_out }),
                    &tim_out,
                );
                texture_out = Some(tim_out);
            }

            let out = format!("{id}.pmd");
            let opts = gltf_import::ImportOptions {
                tex_w: tex_dims.0,
                tex_h: tex_dims.1,
                ..Default::default()
            };
            let (pmd, report) = gltf_import::import(&dst, &opts)?;
            std::fs::write(library.join(&out), pmd.write()?).map_err(|e| e.to_string())?;
            warnings.extend(report.warnings);
            register(
                &mut project["models"],
                serde_json::json!({ "gltf": gltf_rel, "out": out }),
                &out,
            );
            let summary = format!(
                "{} triangles, {} sommets{}",
                report.triangles_in - report.degenerate_dropped,
                report.vertex_count,
                if texture_out.is_some() {
                    format!(", texture extraite {}x{}", tex_dims.0, tex_dims.1)
                } else {
                    String::new()
                }
            );
            ImportedAsset {
                kind: AssetKind::Model,
                id,
                out,
                texture_out,
                summary,
                warnings,
            }
        }
        "png" => {
            let file_name = format!("{id}.png");
            let (dst, png_rel) = match &in_project_rel {
                Some(rel) => (src.to_path_buf(), rel.clone()),
                None => {
                    let dst = assets_dir.join(&file_name);
                    copy_into_assets(src, &dst)?;
                    (dst, format!("assets/{file_name}"))
                }
            };
            let out = format!("{id}.tim");
            let img = image::open(&dst).map_err(|e| e.to_string())?.to_rgba8();
            let (w, h) = img.dimensions();
            if w > 256 || h > 256 {
                return Err(format!("{w}x{h} : les textures sont limitées à 256x256"));
            }
            let bpp = tim::auto_bpp(img.as_raw());
            let opts = tim::TimOptions { bpp, ..Default::default() };
            let (timg, report) = tim::encode(img.as_raw(), w, h, &opts)?;
            std::fs::write(library.join(&out), timg.write()).map_err(|e| e.to_string())?;
            register(
                &mut project["textures"],
                serde_json::json!({ "png": png_rel, "out": out }),
                &out,
            );
            ImportedAsset {
                kind: AssetKind::Texture,
                id,
                out,
                texture_out: None,
                summary: format!(
                    "{w}x{h}, {} couleurs -> {} en palette {}bpp{}",
                    report.source_colors,
                    report.palette_colors,
                    if bpp == tim::Bpp::Four { 4 } else { 8 },
                    if bpp == tim::Bpp::Four { " (moitié de VRAM)" } else { "" },
                ),
                warnings: report
                    .warnings
                    .into_iter()
                    .filter(|w| !w.contains("framebuffer"))
                    .collect(),
            }
        }
        other => return Err(format!("extension .{other} non supportée (gltf, glb, png)")),
    };

    std::fs::write(
        &json_path,
        serde_json::to_string_pretty(&project).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| e.to_string())?;

    Ok(result)
}

/* ------------------------------------------------- réglages de modèle -- */

/// Lit le seuil de subdivision d'un modèle, par son nom de sortie .pmd.
pub fn model_subdiv(project_dir: &Path, pmd_out: &str) -> Result<Option<f32>, String> {
    let json_path = project_dir.join("project.json");
    let text = std::fs::read_to_string(&json_path)
        .map_err(|e| format!("{} : {e}", json_path.display()))?;
    let project: ProjectJson =
        serde_json::from_str(&text).map_err(|e| format!("project.json : {e}"))?;
    let model = project
        .models
        .iter()
        .find(|m| m.out == pmd_out)
        .ok_or_else(|| format!("modèle {pmd_out} introuvable dans project.json"))?;
    Ok(model.subdiv)
}

/// Écrit le seuil de subdivision d'un modèle dans project.json (None ou 0
/// = désactivée) puis le reconvertit immédiatement dans Library/ pour que
/// l'éditeur voie le résultat sans attendre un Play. Retourne un résumé.
pub fn set_model_subdiv(
    project_dir: &Path,
    pmd_out: &str,
    subdiv: Option<f32>,
) -> Result<String, String> {
    let json_path = project_dir.join("project.json");
    let text = std::fs::read_to_string(&json_path)
        .map_err(|e| format!("{} : {e}", json_path.display()))?;
    // Édition en Value : ne réécrit que le champ visé, préserve le reste.
    let mut project: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("project.json : {e}"))?;
    let model = project["models"]
        .as_array_mut()
        .ok_or("project.json : pas de liste models")?
        .iter_mut()
        .find(|m| m["out"] == pmd_out)
        .ok_or_else(|| format!("modèle {pmd_out} introuvable dans project.json"))?;
    match subdiv {
        Some(s) if s > 0.0 => {
            if !(1.0..=32767.0).contains(&s) {
                return Err("subdiv doit être entre 1 et 32767 (unités PMD)".into());
            }
            model["subdiv"] = serde_json::json!(s);
        }
        _ => {
            model.as_object_mut().unwrap().remove("subdiv");
        }
    }
    std::fs::write(
        &json_path,
        serde_json::to_string_pretty(&project).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| e.to_string())?;

    reconvert_model(project_dir, pmd_out)
}

/// Reconvertit un seul modèle du projet dans Library/ (avec ses options
/// actuelles) et met le cache à jour pour que le prochain build le saute.
pub fn reconvert_model(project_dir: &Path, pmd_out: &str) -> Result<String, String> {
    let text = std::fs::read_to_string(project_dir.join("project.json"))
        .map_err(|e| e.to_string())?;
    let project: ProjectJson =
        serde_json::from_str(&text).map_err(|e| format!("project.json : {e}"))?;
    let model = project
        .models
        .iter()
        .find(|m| m.out == pmd_out)
        .ok_or_else(|| format!("modèle {pmd_out} introuvable dans project.json"))?;

    let library = project_dir.join("Library");
    std::fs::create_dir_all(&library).map_err(|e| e.to_string())?;
    let src = project_dir.join(&model.gltf);
    let opts = gltf_import::ImportOptions {
        target_size: model.size.unwrap_or(128),
        tex_w: model.tex_w.unwrap_or(256),
        tex_h: model.tex_h.unwrap_or(256),
        subdiv: model.subdiv,
        ..Default::default()
    };
    let (pmd, report) = gltf_import::import(&src, &opts)?;
    std::fs::write(library.join(&model.out), pmd.write()?).map_err(|e| e.to_string())?;

    let mut cache = Cache::load(&library);
    let hash = format!(
        "{}:{}:{}x{}:s{}",
        hash_gltf(&src)?,
        model.size.unwrap_or(128),
        model.tex_w.unwrap_or(256),
        model.tex_h.unwrap_or(256),
        model.subdiv.unwrap_or(0.0)
    );
    cache.record(model.gltf.clone(), hash);
    cache.save()?;

    let total: usize = report.counts.iter().sum();
    Ok(format!(
        "{} : {total} triangles{}",
        model.out,
        if report.triangles_subdivided > 0 {
            format!(" (dont +{} par subdivision)", report.triangles_subdivided)
        } else {
            String::new()
        }
    ))
}

/* ------------------------------------------ panneau Project (éditeur) -- */

/// Une entrée du panneau Project : fichier sur disque et/ou référencé par
/// project.json — le croisement des deux rend visibles les
/// désynchronisations (« non importé », « manquant »).
#[derive(serde::Serialize, Debug)]
pub struct ProjectFile {
    /// Chemin relatif au dossier projet, séparateurs '/'.
    pub path: String,
    pub name: String,
    /// Dossier de premier niveau : "assets", "audio" ou "scenes".
    pub section: String,
    /// "dir" | "scene" | "model" | "texture" | "audio" | "buffer" | "other".
    pub kind: String,
    pub size: u64,
    /// Présent dans project.json. Les fichiers compagnons (.bin d'un
    /// .gltf) et inconnus sont marqués enregistrés : pas de badge inutile.
    pub registered: bool,
    /// Faux : entrée de project.json dont le fichier a disparu du disque.
    pub exists: bool,
    /// Sortie convertie dans Library/ (ex. "guy.pmd") pour un fichier
    /// enregistré comme modèle ou texture — sert aux vignettes de
    /// l'éditeur et à l'instanciation par drag & drop.
    pub out: Option<String>,
}

fn kind_for(section: &str, ext: &str) -> &'static str {
    match ext {
        "gltf" | "glb" => "model",
        "png" => "texture",
        "wav" | "vag" => "audio",
        "bin" => "buffer",
        "json" if section == "scenes" => "scene",
        _ => "other",
    }
}

/// Liste le contenu du projet pour le panneau Project : les fichiers des
/// dossiers `assets/`, `audio/` et `scenes/`, croisés avec project.json.
pub fn list_files(project_dir: &Path) -> Result<Vec<ProjectFile>, String> {
    let json_path = project_dir.join("project.json");
    let text = std::fs::read_to_string(&json_path)
        .map_err(|e| format!("{} : {e}", json_path.display()))?;
    let project: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("project.json : {e}"))?;

    // Chemins sources enregistrés (normalisés en '/') -> sortie Library/.
    let mut registered: std::collections::BTreeMap<String, Option<String>> =
        std::collections::BTreeMap::new();
    let mut collect = |value: &serde_json::Value, key: Option<&str>| {
        for entry in value.as_array().into_iter().flatten() {
            let path = match key {
                Some(k) => entry[k].as_str(),
                None => entry.as_str(),
            };
            if let Some(p) = path {
                let out = entry["out"].as_str().map(String::from);
                registered.insert(p.replace('\\', "/"), out);
            }
        }
    };
    collect(&project["models"], Some("gltf"));
    collect(&project["textures"], Some("png"));
    collect(&project["sfx"], Some("wav"));
    collect(&project["music"], None);
    collect(&project["scenes"], None);

    // Parcours récursif : les dossiers (même vides) apparaissent, pour que
    // l'utilisateur puisse ranger ses fichiers comme il veut.
    fn walk(
        dir: &Path,
        prefix: &str,
        section: &str,
        registered: &std::collections::BTreeMap<String, Option<String>>,
        files: &mut Vec<ProjectFile>,
        seen: &mut std::collections::BTreeSet<String>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut names: Vec<(String, bool)> = entries
            .flatten()
            .map(|e| {
                (
                    e.file_name().to_string_lossy().into_owned(),
                    e.path().is_dir(),
                )
            })
            .collect();
        names.sort();
        for (name, is_dir) in names {
            // Library/ et fichiers cachés n'ont rien à faire dans le panneau.
            if name.starts_with('.') {
                continue;
            }
            let rel = format!("{prefix}/{name}");
            if is_dir {
                let child = dir.join(&name);
                files.push(ProjectFile {
                    path: rel.clone(),
                    name,
                    section: section.into(),
                    kind: "dir".into(),
                    size: 0,
                    registered: true,
                    exists: true,
                    out: None,
                });
                walk(&child, &rel, section, registered, files, seen);
                continue;
            }
            let ext = std::path::Path::new(&name)
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let kind = kind_for(section, &ext);
            let size = std::fs::metadata(dir.join(&name)).map(|m| m.len()).unwrap_or(0);
            seen.insert(rel.clone());
            files.push(ProjectFile {
                name,
                section: section.into(),
                kind: kind.into(),
                size,
                // Les compagnons/inconnus ne sont pas importables : pas de badge.
                registered: registered.contains_key(&rel)
                    || kind == "buffer"
                    || kind == "other",
                exists: true,
                out: registered.get(&rel).cloned().flatten(),
                path: rel,
            });
        }
    }

    let mut files = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for section in ["scenes", "assets", "audio"] {
        walk(
            &project_dir.join(section),
            section,
            section,
            &registered,
            &mut files,
            &mut seen,
        );
    }

    // Entrées de project.json dont le fichier a disparu (ex. asset supprimé
    // à la main) : montrées avec le badge « manquant ».
    for (rel, out) in &registered {
        if seen.contains(rel) || project_dir.join(rel).exists() {
            continue;
        }
        let section = rel.split('/').next().unwrap_or("");
        let name = rel.rsplit('/').next().unwrap_or(rel.as_str());
        let ext = std::path::Path::new(name)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        files.push(ProjectFile {
            path: rel.clone(),
            name: name.to_string(),
            section: section.to_string(),
            kind: kind_for(section, &ext).into(),
            size: 0,
            registered: true,
            exists: false,
            out: out.clone(),
        });
    }

    Ok(files)
}

/// Garde-fou des chemins venus du panneau Project : relatifs, en avant,
/// sans remonter hors du projet.
fn check_rel_path(rel: &str) -> Result<(), String> {
    if rel.is_empty()
        || rel.starts_with('/')
        || rel.contains('\\')
        || rel.contains("..")
        || rel.split('/').any(|seg| seg.is_empty())
    {
        return Err(format!("chemin invalide : {rel}"));
    }
    Ok(())
}

/// Crée un dossier dans le projet (menu « Créer ▸ Dossier » du panneau).
pub fn create_folder(project_dir: &Path, parent_rel: &str, name: &str) -> Result<String, String> {
    check_rel_path(parent_rel)?;
    let clean: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if clean.is_empty() {
        return Err("nom de dossier vide".into());
    }
    let rel = format!("{parent_rel}/{clean}");
    let path = project_dir.join(&rel);
    if path.exists() {
        return Err(format!("{rel} existe déjà"));
    }
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    Ok(rel)
}

/// Déplace un fichier du projet vers un autre dossier (drag & drop du
/// panneau Project) et réécrit toutes les références de project.json.
/// Le .bin compagnon d'un .gltf suit (le glTF le référence par URI
/// relative au même dossier). Retourne le nouveau chemin relatif.
pub fn move_entry(project_dir: &Path, from_rel: &str, to_dir_rel: &str) -> Result<String, String> {
    check_rel_path(from_rel)?;
    check_rel_path(to_dir_rel)?;
    let src = project_dir.join(from_rel);
    if !src.is_file() {
        return Err(format!("{from_rel} n'est pas un fichier du projet"));
    }
    let dst_dir = project_dir.join(to_dir_rel);
    if !dst_dir.is_dir() {
        return Err(format!("{to_dir_rel} n'est pas un dossier du projet"));
    }
    let name = from_rel.rsplit('/').next().unwrap().to_string();
    let new_rel = format!("{to_dir_rel}/{name}");
    if new_rel == from_rel {
        return Ok(new_rel);
    }
    if project_dir.join(&new_rel).exists() {
        return Err(format!("{new_rel} existe déjà"));
    }

    std::fs::rename(&src, project_dir.join(&new_rel)).map_err(|e| format!("déplacement : {e}"))?;

    // Compagnon .bin d'un .gltf : déplacé avec lui.
    let mut moved = vec![(from_rel.to_string(), new_rel.clone())];
    if name.to_lowercase().ends_with(".gltf") {
        let bin_name = format!("{}.bin", &name[..name.len() - 5]);
        let bin_from = format!("{}/{bin_name}", from_rel.rsplit_once('/').map(|(d, _)| d).unwrap_or(""));
        if project_dir.join(&bin_from).is_file() {
            let bin_to = format!("{to_dir_rel}/{bin_name}");
            std::fs::rename(project_dir.join(&bin_from), project_dir.join(&bin_to))
                .map_err(|e| format!("déplacement du .bin : {e}"))?;
            moved.push((bin_from, bin_to));
        }
    }

    // project.json : réécrit chaque référence au chemin déplacé.
    let json_path = project_dir.join("project.json");
    let text = std::fs::read_to_string(&json_path)
        .map_err(|e| format!("{} : {e}", json_path.display()))?;
    let mut project: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("project.json : {e}"))?;
    let rewrite = |v: &mut serde_json::Value| {
        if let Some(s) = v.as_str() {
            let normalized = s.replace('\\', "/");
            if let Some((_, to)) = moved.iter().find(|(from, _)| *from == normalized) {
                *v = serde_json::Value::String(to.clone());
            }
        }
    };
    for entry in project["models"].as_array_mut().into_iter().flatten() {
        rewrite(&mut entry["gltf"]);
    }
    for entry in project["textures"].as_array_mut().into_iter().flatten() {
        rewrite(&mut entry["png"]);
    }
    for entry in project["sfx"].as_array_mut().into_iter().flatten() {
        rewrite(&mut entry["wav"]);
    }
    for entry in project["music"].as_array_mut().into_iter().flatten() {
        rewrite(entry);
    }
    for entry in project["scenes"].as_array_mut().into_iter().flatten() {
        rewrite(entry);
    }
    std::fs::write(
        &json_path,
        serde_json::to_string_pretty(&project).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| e.to_string())?;

    Ok(new_rel)
}

/// Crée une scène vide `scenes/<slug>.json` et l'enregistre dans
/// project.json (menu « Créer ▸ Scène » du panneau Project).
pub fn create_scene(project_dir: &Path, name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("nom de scène vide".into());
    }
    let rel = format!("scenes/{}.json", sanitize_id(trimmed));
    let path = project_dir.join(&rel);
    if path.exists() {
        return Err(format!("{rel} existe déjà"));
    }

    let doc = serde_json::json!({
        "name": trimmed,
        "assets": { "textures": [], "models": [] },
        "entities": []
    });
    // Garantie : la scène minimale doit rester un SceneJson valide.
    serde_json::from_value::<crate::scene::SceneJson>(doc.clone())
        .map_err(|e| format!("scène minimale invalide : {e}"))?;

    let json_path = project_dir.join("project.json");
    let text = std::fs::read_to_string(&json_path)
        .map_err(|e| format!("{} : {e}", json_path.display()))?;
    let mut project: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("project.json : {e}"))?;
    if !project["scenes"].is_array() {
        project["scenes"] = serde_json::Value::Array(Vec::new());
    }
    project["scenes"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::Value::String(rel.clone()));

    std::fs::create_dir_all(project_dir.join("scenes")).map_err(|e| e.to_string())?;
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        &json_path,
        serde_json::to_string_pretty(&project).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| e.to_string())?;

    Ok(rel)
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
