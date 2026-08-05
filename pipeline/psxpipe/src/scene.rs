//! SceneFormat v1: editable JSON scene -> packed .psc binary.
//! Specification: docs/SCENE-FORMAT.md
//!
//! The .psc embeds every referenced PMD/TIM blob so a scene loads with a
//! single contiguous CD read.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::{vram, ONE_4_12};

pub const MAGIC: &[u8; 4] = b"PSC1";
pub const VERSION: u16 = 1;
pub const HEADER_SIZE: usize = 64;
pub const MODEL_ENTRY_SIZE: usize = 12;
pub const TEXTURE_ENTRY_SIZE: usize = 8;
pub const ENTITY_SIZE: usize = 32;
pub const NO_INDEX: u16 = 0xFFFF;

/* ---------------------------------------------------------------- JSON -- */

fn default_scale() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneJson {
    pub name: String,
    #[serde(default)]
    pub settings: Settings,
    pub assets: Assets,
    pub entities: Vec<EntityJson>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(default = "Settings::default_background")]
    pub background: [u8; 3],
    #[serde(default = "Settings::default_ambient")]
    pub ambient: [u8; 3],
    #[serde(default = "Settings::default_light_dir")]
    pub light_dir: [f32; 3],
    #[serde(default = "Settings::default_light_color")]
    pub light_color: [u8; 3],
}

impl Settings {
    fn default_background() -> [u8; 3] {
        [16, 16, 48]
    }
    fn default_ambient() -> [u8; 3] {
        [64, 64, 64]
    }
    fn default_light_dir() -> [f32; 3] {
        [1.0, 1.0, 1.0]
    }
    fn default_light_color() -> [u8; 3] {
        [255, 255, 255]
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            background: Self::default_background(),
            ambient: Self::default_ambient(),
            light_dir: Self::default_light_dir(),
            light_color: Self::default_light_color(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assets {
    #[serde(default)]
    pub textures: Vec<TextureJson>,
    pub models: Vec<ModelJson>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextureJson {
    pub id: String,
    pub tim: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelJson {
    pub id: String,
    pub pmd: String,
    #[serde(default)]
    pub texture: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityJson {
    pub name: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub position: [f32; 3],
    #[serde(default)]
    pub rotation: [f32; 3],
    #[serde(default = "default_scale")]
    pub scale: [f32; 3],
    #[serde(default)]
    pub model: Option<String>,
}

/* -------------------------------------------------------------- report -- */

#[derive(Debug, Default)]
pub struct SceneReport {
    pub name: String,
    pub models: usize,
    pub textures: usize,
    pub entities: usize,
    pub total_size: usize,
    pub warnings: Vec<String>,
    /// VRAM packing result (requests + placements), for the map export.
    pub vram: Vec<(vram::TexRequest, vram::Placement)>,
    /// Entity names in FILE order (after the topological sort) — lets the
    /// editor map .psc entity indices back to JSON entities.
    pub entity_names: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct BuildOptions {
    /// Repack textures into VRAM automatically (default). When false, the
    /// placements baked into the TIM files are kept as-is.
    pub pack_vram: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        BuildOptions { pack_vram: true }
    }
}

/* --------------------------------------------------------------- build -- */

struct VramRect {
    label: String,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
}

/// Collect the VRAM rects (pixel data + CLUT) declared by a TIM blob.
fn tim_rects(label: &str, data: &[u8]) -> Result<Vec<VramRect>, String> {
    if data.len() < 8 || data[0] != 0x10 || data[1..4] != [0, 0, 0] {
        return Err(format!("{label}: not a TIM file"));
    }
    let u16at = |o: usize| u16::from_le_bytes([data[o], data[o + 1]]);
    let u32at = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
    let flags = u32at(4);
    let mut rects = Vec::new();
    let mut off = 8usize;
    let blocks = if flags & 8 != 0 { 2 } else { 1 };
    for b in 0..blocks {
        if off + 12 > data.len() {
            return Err(format!("{label}: truncated TIM"));
        }
        rects.push(VramRect {
            label: format!("{label}{}", if blocks == 2 && b == 0 { " (CLUT)" } else { "" }),
            x: u16at(off + 4),
            y: u16at(off + 6),
            w: u16at(off + 8),
            h: u16at(off + 10),
        });
        off += u32at(off) as usize;
    }
    Ok(rects)
}

fn overlaps(a: &VramRect, b: &VramRect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

fn quantize_i16(v: f32, what: &str, name: &str) -> Result<i16, String> {
    let q = v.round();
    if !(-32768.0..=32767.0).contains(&q) {
        return Err(format!("{name}: {what} {v} out of i16 range"));
    }
    Ok(q as i16)
}

/// Parse the geometry of a TIM blob: (pixel words, height, clut entries).
fn tim_geometry(label: &str, data: &[u8]) -> Result<(u16, u16, u16), String> {
    if data.len() < 8 || data[0] != 0x10 || data[1..4] != [0, 0, 0] {
        return Err(format!("{label}: not a TIM file"));
    }
    let u16at = |o: usize| u16::from_le_bytes([data[o], data[o + 1]]);
    let u32at = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
    let flags = u32at(4);
    let mut off = 8usize;
    let mut clut_entries = 0u16;
    if flags & 8 != 0 {
        clut_entries = u16at(off + 8);
        off += u32at(off) as usize;
    }
    Ok((u16at(off + 8), u16at(off + 10), clut_entries))
}

/// Rewrite the VRAM coordinates baked in a TIM blob.
fn tim_set_position(data: &mut [u8], p: &vram::Placement) {
    let flags = u32::from_le_bytes(data[4..8].try_into().unwrap());
    let mut off = 8usize;
    if flags & 8 != 0 {
        data[off + 4..off + 6].copy_from_slice(&p.clut_x.to_le_bytes());
        data[off + 6..off + 8].copy_from_slice(&p.clut_y.to_le_bytes());
        off += u32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as usize;
    }
    data[off + 4..off + 6].copy_from_slice(&p.x.to_le_bytes());
    data[off + 6..off + 8].copy_from_slice(&p.y.to_le_bytes());
}

/// Build a .psc from a parsed scene. `base_dir` resolves asset paths.
pub fn build(scene: &SceneJson, base_dir: &Path) -> Result<(Vec<u8>, SceneReport), String> {
    build_with_options(scene, base_dir, &BuildOptions::default())
}

pub fn build_with_options(
    scene: &SceneJson,
    base_dir: &Path,
    options: &BuildOptions,
) -> Result<(Vec<u8>, SceneReport), String> {
    let mut report = SceneReport {
        name: scene.name.clone(),
        ..Default::default()
    };

    /* Load asset blobs. */
    let mut texture_index: HashMap<&str, u16> = HashMap::new();
    let mut texture_blobs: Vec<Vec<u8>> = Vec::new();
    for (i, tex) in scene.assets.textures.iter().enumerate() {
        if texture_index.insert(&tex.id, i as u16).is_some() {
            return Err(format!("duplicate texture id '{}'", tex.id));
        }
        let path = base_dir.join(&tex.tim);
        let data = std::fs::read(&path)
            .map_err(|e| format!("texture '{}': cannot read {}: {e}", tex.id, path.display()))?;
        texture_blobs.push(data);
    }

    /* Automatic VRAM packing: place every texture page-aligned and rewrite
     * the coordinates baked in the TIM blobs. */
    if options.pack_vram {
        let requests: Vec<vram::TexRequest> = scene
            .assets
            .textures
            .iter()
            .zip(&texture_blobs)
            .map(|(tex, blob)| {
                let (words, height, clut_entries) = tim_geometry(&tex.id, blob)?;
                Ok(vram::TexRequest {
                    label: tex.id.clone(),
                    words,
                    height,
                    clut_entries,
                })
            })
            .collect::<Result<_, String>>()?;
        let placements = vram::pack(&requests)?;
        for (blob, placement) in texture_blobs.iter_mut().zip(&placements) {
            tim_set_position(blob, placement);
        }
        report.vram = requests.into_iter().zip(placements).collect();
    }

    let mut model_index: HashMap<&str, u16> = HashMap::new();
    let mut model_blobs: Vec<(Vec<u8>, u16)> = Vec::new(); // (pmd, texture index)
    for (i, model) in scene.assets.models.iter().enumerate() {
        if model_index.insert(&model.id, i as u16).is_some() {
            return Err(format!("duplicate model id '{}'", model.id));
        }
        let path = base_dir.join(&model.pmd);
        let data = std::fs::read(&path)
            .map_err(|e| format!("model '{}': cannot read {}: {e}", model.id, path.display()))?;
        let header = crate::pmd::parse_header(&data)
            .map_err(|e| format!("model '{}': {e}", model.id))?;
        let tex = match &model.texture {
            Some(id) => *texture_index
                .get(id.as_str())
                .ok_or(format!("model '{}': unknown texture '{id}'", model.id))?,
            None => NO_INDEX,
        };
        if header.flags & crate::pmd::FLAG_TEXTURED != 0 && tex == NO_INDEX {
            report.warnings.push(format!(
                "model '{}' is textured but has no texture assigned",
                model.id
            ));
        }
        model_blobs.push((data, tex));
    }

    /* VRAM collision checks (textures against each other and the default
     * double framebuffer at (0,0)-(320,480)). */
    let mut rects = vec![VramRect {
        label: "framebuffers".into(),
        x: 0,
        y: 0,
        w: 320,
        h: 480,
    }];
    for (tex, blob) in scene.assets.textures.iter().zip(&texture_blobs) {
        rects.extend(tim_rects(&tex.id, blob)?);
    }
    for i in 0..rects.len() {
        for j in i + 1..rects.len() {
            if overlaps(&rects[i], &rects[j]) {
                return Err(format!(
                    "VRAM overlap between {} at ({},{}) {}x{} and {} at ({},{}) {}x{}",
                    rects[i].label, rects[i].x, rects[i].y, rects[i].w, rects[i].h,
                    rects[j].label, rects[j].x, rects[j].y, rects[j].w, rects[j].h,
                ));
            }
        }
    }

    /* Topological sort of entities (parents before children). */
    let mut name_to_pos: HashMap<&str, usize> = HashMap::new();
    for (i, e) in scene.entities.iter().enumerate() {
        if name_to_pos.insert(&e.name, i).is_some() {
            return Err(format!("duplicate entity name '{}'", e.name));
        }
    }
    let n = scene.entities.len();
    let mut order: Vec<usize> = Vec::with_capacity(n);
    let mut state = vec![0u8; n]; // 0 = unvisited, 1 = visiting, 2 = done
    fn visit(
        i: usize,
        scene: &SceneJson,
        name_to_pos: &HashMap<&str, usize>,
        state: &mut [u8],
        order: &mut Vec<usize>,
    ) -> Result<(), String> {
        match state[i] {
            2 => return Ok(()),
            1 => return Err(format!("parent cycle involving '{}'", scene.entities[i].name)),
            _ => {}
        }
        state[i] = 1;
        if let Some(parent) = &scene.entities[i].parent {
            let p = *name_to_pos
                .get(parent.as_str())
                .ok_or(format!("entity '{}': unknown parent '{parent}'", scene.entities[i].name))?;
            visit(p, scene, name_to_pos, state, order)?;
        }
        state[i] = 2;
        order.push(i);
        Ok(())
    }
    for i in 0..n {
        visit(i, scene, &name_to_pos, &mut state, &mut order)?;
    }
    let mut pos_to_sorted = vec![0u16; n];
    for (sorted, &original) in order.iter().enumerate() {
        pos_to_sorted[original] = sorted as u16;
    }

    /* Serialize entities in sorted order. */
    let mut entities = Vec::with_capacity(n * ENTITY_SIZE);
    for &i in &order {
        let e = &scene.entities[i];
        let push_svec = |out: &mut Vec<u8>, v: [i16; 3]| {
            for c in v {
                out.extend_from_slice(&c.to_le_bytes());
            }
            out.extend_from_slice(&[0, 0]);
        };
        let pos = [
            quantize_i16(e.position[0], "position.x", &e.name)?,
            quantize_i16(e.position[1], "position.y", &e.name)?,
            quantize_i16(e.position[2], "position.z", &e.name)?,
        ];
        let rot_units = |deg: f32| -> i16 { (deg.rem_euclid(360.0) / 360.0 * 4096.0).round() as i16 };
        let rot = [
            rot_units(e.rotation[0]),
            rot_units(e.rotation[1]),
            rot_units(e.rotation[2]),
        ];
        let scale = [
            quantize_i16(e.scale[0] * ONE_4_12 as f32, "scale.x", &e.name)?,
            quantize_i16(e.scale[1] * ONE_4_12 as f32, "scale.y", &e.name)?,
            quantize_i16(e.scale[2] * ONE_4_12 as f32, "scale.z", &e.name)?,
        ];
        push_svec(&mut entities, pos);
        push_svec(&mut entities, rot);
        push_svec(&mut entities, scale);
        let parent = match &e.parent {
            Some(p) => pos_to_sorted[name_to_pos[p.as_str()]],
            None => NO_INDEX,
        };
        let model = match &e.model {
            Some(id) => *model_index
                .get(id.as_str())
                .ok_or(format!("entity '{}': unknown model '{id}'", e.name))?,
            None => NO_INDEX,
        };
        entities.extend_from_slice(&parent.to_le_bytes());
        entities.extend_from_slice(&model.to_le_bytes());
        entities.extend_from_slice(&[0, 0, 0, 0]); // flags + pad
    }

    /* Layout: header | model table | texture table | entities | blobs. */
    let models_offset = HEADER_SIZE;
    let textures_offset = models_offset + model_blobs.len() * MODEL_ENTRY_SIZE;
    let entities_offset = textures_offset + texture_blobs.len() * TEXTURE_ENTRY_SIZE;
    let mut blob_cursor = entities_offset + entities.len();

    let align4 = |v: usize| (v + 3) & !3;
    let mut model_entries = Vec::new();
    let mut model_offsets = Vec::new();
    for (blob, tex) in &model_blobs {
        blob_cursor = align4(blob_cursor);
        model_offsets.push(blob_cursor);
        model_entries.extend_from_slice(&(blob_cursor as u32).to_le_bytes());
        model_entries.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        model_entries.extend_from_slice(&tex.to_le_bytes());
        model_entries.extend_from_slice(&[0, 0]);
        blob_cursor += blob.len();
    }
    let mut texture_entries = Vec::new();
    let mut texture_offsets = Vec::new();
    for blob in &texture_blobs {
        blob_cursor = align4(blob_cursor);
        texture_offsets.push(blob_cursor);
        texture_entries.extend_from_slice(&(blob_cursor as u32).to_le_bytes());
        texture_entries.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        blob_cursor += blob.len();
    }
    let total_size = blob_cursor;

    /* Light vector: normalize, negate (file stores the vector TOWARD the
     * source, ready for the GTE light matrix), convert to PS1 axes (-Y up
     * flip is already the JSON convention: JSON uses PS1 axes). */
    let d = scene.settings.light_dir;
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len < 1e-6 {
        return Err("settings.light_dir must not be zero".into());
    }
    let toward = [
        (-d[0] / len * ONE_4_12 as f32).round() as i16,
        (-d[1] / len * ONE_4_12 as f32).round() as i16,
        (-d[2] / len * ONE_4_12 as f32).round() as i16,
    ];

    let mut out = Vec::with_capacity(total_size);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // flags
    out.extend_from_slice(&(total_size as u32).to_le_bytes());
    for count in [model_blobs.len(), texture_blobs.len(), n, 0] {
        if count > u16::MAX as usize {
            return Err("too many items in scene".into());
        }
        out.extend_from_slice(&(count as u16).to_le_bytes());
    }
    for off in [models_offset, textures_offset, entities_offset] {
        out.extend_from_slice(&(off as u32).to_le_bytes());
    }
    for rgb in [
        scene.settings.background,
        scene.settings.ambient,
        scene.settings.light_color,
    ] {
        out.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 0]);
    }
    for c in toward {
        out.extend_from_slice(&c.to_le_bytes());
    }
    out.resize(HEADER_SIZE, 0); // reserved
    out.extend_from_slice(&model_entries);
    out.extend_from_slice(&texture_entries);
    out.extend_from_slice(&entities);
    for (offset, (blob, _)) in model_offsets.iter().zip(&model_blobs) {
        out.resize(*offset, 0);
        out.extend_from_slice(blob);
    }
    for (offset, blob) in texture_offsets.iter().zip(&texture_blobs) {
        out.resize(*offset, 0);
        out.extend_from_slice(blob);
    }
    debug_assert_eq!(out.len(), total_size);

    report.models = model_blobs.len();
    report.textures = texture_blobs.len();
    report.entities = n;
    report.total_size = total_size;
    report.entity_names = order
        .iter()
        .map(|&i| scene.entities[i].name.clone())
        .collect();
    Ok((out, report))
}

/// Convenience: read a scene JSON file and build the .psc next to it.
pub fn build_file(json_path: &Path) -> Result<(Vec<u8>, SceneReport), String> {
    build_file_with_options(json_path, &BuildOptions::default())
}

pub fn build_file_with_options(
    json_path: &Path,
    options: &BuildOptions,
) -> Result<(Vec<u8>, SceneReport), String> {
    let text = std::fs::read_to_string(json_path)
        .map_err(|e| format!("cannot read {}: {e}", json_path.display()))?;
    let scene: SceneJson =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", json_path.display()))?;
    let base = json_path.parent().unwrap_or(Path::new("."));
    build_with_options(&scene, base, options)
}

/* -------------------------------------------------------------- header -- */

/// Parsed header, for `psxpipe info`, the preview tool and tests.
#[derive(Debug)]
pub struct PscHeader {
    pub version: u16,
    pub total_size: u32,
    pub model_count: u16,
    pub texture_count: u16,
    pub entity_count: u16,
    pub models_offset: u32,
    pub textures_offset: u32,
    pub entities_offset: u32,
    pub background: [u8; 3],
    pub ambient: [u8; 3],
    pub light_color: [u8; 3],
    pub light_toward: [i16; 3],
}

pub fn parse_header(data: &[u8]) -> Result<PscHeader, String> {
    if data.len() < HEADER_SIZE {
        return Err("file too small for a PSC header".into());
    }
    if &data[0..4] != MAGIC {
        return Err("bad magic (not a PSC file)".into());
    }
    let u16at = |o: usize| u16::from_le_bytes([data[o], data[o + 1]]);
    let i16at = |o: usize| i16::from_le_bytes([data[o], data[o + 1]]);
    let u32at = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
    let h = PscHeader {
        version: u16at(4),
        total_size: u32at(8),
        model_count: u16at(12),
        texture_count: u16at(14),
        entity_count: u16at(16),
        models_offset: u32at(20),
        textures_offset: u32at(24),
        entities_offset: u32at(28),
        background: [data[32], data[33], data[34]],
        ambient: [data[36], data[37], data[38]],
        light_color: [data[40], data[41], data[42]],
        light_toward: [i16at(44), i16at(46), i16at(48)],
    };
    if h.version != VERSION {
        return Err(format!("unsupported PSC version {}", h.version));
    }
    Ok(h)
}
