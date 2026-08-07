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
pub const LIGHT_ENTRY_SIZE: usize = 6;
/// v1.3 : enregistrement de composants UI (voir docs/UI-SYSTEM.md).
pub const UI_ENTRY_SIZE: usize = 40;
pub const FONT_ENTRY_SIZE: usize = 8;
pub const NO_INDEX: u16 = 0xFFFF;

/* ---------------------------------------------------------------- JSON -- */

fn default_scale() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct SceneJson {
    pub name: String,
    #[serde(default)]
    pub settings: Settings,
    pub assets: Assets,
    pub entities: Vec<EntityJson>,
}

#[derive(Deserialize, Clone)]
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

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Assets {
    #[serde(default)]
    pub textures: Vec<TextureJson>,
    pub models: Vec<ModelJson>,
    /// Polices bitmap .fnt (v1.3, UI).
    #[serde(default)]
    pub fonts: Vec<FontJson>,
}

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct TextureJson {
    pub id: String,
    pub tim: String,
}

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ModelJson {
    pub id: String,
    pub pmd: String,
    #[serde(default)]
    pub texture: Option<String>,
}

#[derive(Deserialize, Clone)]
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
    /// Nom du script C attaché (OnStart/OnUpdate), résolu par hash côté
    /// runtime dans le registre compilé avec le jeu.
    #[serde(default)]
    pub script: Option<String>,
    /// Composant lumière directionnelle (v1.2) : la rotation de l'entité
    /// donne la direction (la lumière éclaire le long de son axe -Z
    /// local, comme les modèles font face à -Z). 2 max par scène : la
    /// lumière des settings occupe la ligne 0 du GTE, celles-ci les
    /// lignes 1 et 2.
    #[serde(default)]
    pub light: Option<LightJson>,
    /// Composant caméra (v1.2) : la première entité caméra donne la vue
    /// initiale de la scène. Convention unique du studio : une entité
    /// « regarde » le long de son axe -Z local (modèles, lumières,
    /// caméras). Accepte `true` ou `{ "fov": 74 }` (FOV vertical en
    /// degrés ; défaut 74 ≈ la projection PS1 native, h = 160).
    #[serde(default)]
    pub camera: CameraJson,
    /// Référence de prefab : l'entité est remplacée au build par le
    /// sous-arbre de `prefabs/<nom>.json` (assets fusionnés, enfants
    /// nommés `<instance>.<enfant>`) — la console ne voit que des
    /// entités ordinaires. La transform de l'instance s'applique à la
    /// racine du prefab.
    #[serde(default)]
    pub prefab: Option<String>,
    /// Composant Collider AABB : participe aux collisions
    /// Physics_MoveAndSlide (boîte du modèle × échelle, axes X/Z).
    /// Absent = défaut console : solide si l'entité a un modèle.
    #[serde(default)]
    pub solid: Option<bool>,
    /// Composant Character Controller (v1.4) : perso jouable sans code.
    #[serde(default)]
    pub controller: ControllerJson,
    /* Composants UI (v1.3, docs/UI-SYSTEM.md) — philosophie uGUI : le
     * canvas est une entité, ses enfants portent RectTransform + Image/
     * Text/Button/Layout. */
    #[serde(default)]
    pub canvas: Option<bool>,
    /// Inactif au chargement (canvas de menu pause, etc.).
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default)]
    pub rect: Option<RectJson>,
    #[serde(default)]
    pub image: Option<UiImageJson>,
    #[serde(default)]
    pub text: Option<UiTextJson>,
    #[serde(default)]
    pub button: Option<bool>,
    #[serde(default)]
    pub layout: Option<UiLayoutJson>,
}

/// RectTransform (sémantique Unity) : ancres/pivot en fractions 0..1 du
/// parent, position/taille en pixels écran (marges sur un axe étiré).
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct RectJson {
    #[serde(default = "half2")]
    pub anchor_min: [f32; 2],
    #[serde(default = "half2")]
    pub anchor_max: [f32; 2],
    #[serde(default = "half2")]
    pub pivot: [f32; 2],
    #[serde(default)]
    pub position: [f32; 2],
    #[serde(default)]
    pub size: [f32; 2],
}

fn half2() -> [f32; 2] {
    [0.5, 0.5]
}

impl Default for RectJson {
    fn default() -> Self {
        RectJson {
            anchor_min: half2(),
            anchor_max: half2(),
            pivot: half2(),
            position: [0.0, 0.0],
            size: [0.0, 0.0],
        }
    }
}

/// Composant Image, avec les 4 Image Types de Unity.
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct UiImageJson {
    #[serde(default)]
    pub texture: Option<String>,
    /// Teinte (défaut 128 = neutre pour les texturées, blanc en aplat).
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// Sprite [x, y, l, h] en texels dans l'atlas (absent = texture entière).
    #[serde(default)]
    pub uv: Option<[u16; 4]>,
    /// "simple" (défaut) | "sliced" | "tiled" | "filled".
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    /// Marges 9-slice [g, h, d, b] en texels (mode sliced).
    #[serde(default)]
    pub border: Option<[u8; 4]>,
    /// "horizontal" (défaut) ou "vertical" (mode filled).
    #[serde(default)]
    pub fill: Option<String>,
    /// Remplissage 0..1 (mode filled).
    #[serde(default)]
    pub amount: Option<f32>,
    #[serde(default)]
    pub semi_transparent: Option<bool>,
}

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct UiTextJson {
    pub font: String,
    pub text: String,
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// "left" (défaut) | "center" | "right".
    #[serde(default)]
    pub align: Option<String>,
}

/// Layout Group vertical/horizontal (jalon 2 côté rendu ; le format le
/// porte dès la v1.3).
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct UiLayoutJson {
    pub axis: String,
    #[serde(default)]
    pub padding: [i16; 4],
    #[serde(default)]
    pub spacing: i16,
    #[serde(default)]
    pub child_align: Option<String>,
    #[serde(default)]
    pub expand_w: bool,
    #[serde(default)]
    pub expand_h: bool,
}

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct FontJson {
    pub id: String,
    pub fnt: String,
}

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct LightJson {
    pub color: [u8; 3],
    /// Multiplicateur d'intensité (0.1–2.5, défaut 1.0). Les matrices
    /// couleur du GTE sont en 4.12 : une lumière peut dépasser 100 %.
    #[serde(default)]
    pub intensity: Option<f32>,
    /// "directional" (défaut) ou "point" (torche : atténuation par la
    /// distance, appliquée par objet — l'approximation d'époque).
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    /// Rayon d'action d'une lumière ponctuelle en unités monde
    /// (défaut 600).
    #[serde(default)]
    pub radius: Option<f32>,
}

impl LightJson {
    pub fn is_point(&self) -> bool {
        self.kind.as_deref() == Some("point")
    }
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
pub enum CameraJson {
    Enabled(bool),
    Props {
        #[serde(default)]
        fov: Option<f32>,
        /// Distance d'affichage en unités monde (0/absent = illimitée) :
        /// les entités au-delà ne sont pas dessinées — le « pop » maîtrisé
        /// des jeux PS1, et du budget GPU récupéré.
        #[serde(default)]
        draw_distance: Option<f32>,
    },
}

impl Default for CameraJson {
    fn default() -> Self {
        CameraJson::Enabled(false)
    }
}

impl CameraJson {
    pub fn enabled(&self) -> bool {
        !matches!(self, CameraJson::Enabled(false))
    }
    pub fn fov(&self) -> Option<f32> {
        match self {
            CameraJson::Props { fov, .. } => *fov,
            CameraJson::Enabled(_) => None,
        }
    }
    pub fn draw_distance(&self) -> Option<f32> {
        match self {
            CameraJson::Props { draw_distance, .. } => *draw_distance,
            CameraJson::Enabled(_) => None,
        }
    }
}

/// Character Controller (v1.4) : déplacement au D-pad avec collisions,
/// orientation 8 directions, caméra suiveuse optionnelle — le tout sans
/// écrire de C (exécuté par Controller_Tick du runtime). Accepte `true`
/// (défauts) ou `{ "speed": 5, "camera": true, "camera_back": 340,
/// "camera_up": 200 }`. Les paramètres logent dans les pads d'entité :
/// une entité contrôleur ne peut être ni caméra ni lumière.
#[derive(Deserialize, Clone)]
#[serde(untagged)]
pub enum ControllerJson {
    Enabled(bool),
    Props {
        /// Vitesse de marche en unités monde par frame (défaut 5).
        #[serde(default)]
        speed: Option<f32>,
        /// Caméra suiveuse (défaut true) : false = le jeu gère sa caméra.
        #[serde(default)]
        camera: Option<bool>,
        /// Recul de la caméra derrière le perso (unités, défaut 340).
        #[serde(default)]
        camera_back: Option<f32>,
        /// Hauteur de la caméra au-dessus du perso (unités, défaut 200).
        #[serde(default)]
        camera_up: Option<f32>,
    },
}

impl Default for ControllerJson {
    fn default() -> Self {
        ControllerJson::Enabled(false)
    }
}

impl ControllerJson {
    pub fn enabled(&self) -> bool {
        !matches!(self, ControllerJson::Enabled(false))
    }
    fn prop(&self, pick: fn(&Self) -> Option<f32>) -> Option<f32> {
        pick(self)
    }
    pub fn speed(&self) -> f32 {
        self.prop(|c| match c {
            ControllerJson::Props { speed, .. } => *speed,
            ControllerJson::Enabled(_) => None,
        })
        .unwrap_or(5.0)
    }
    pub fn camera(&self) -> bool {
        match self {
            ControllerJson::Props { camera, .. } => camera.unwrap_or(true),
            ControllerJson::Enabled(_) => true,
        }
    }
    pub fn camera_back(&self) -> f32 {
        self.prop(|c| match c {
            ControllerJson::Props { camera_back, .. } => *camera_back,
            ControllerJson::Enabled(_) => None,
        })
        .unwrap_or(340.0)
    }
    pub fn camera_up(&self) -> f32 {
        self.prop(|c| match c {
            ControllerJson::Props { camera_up, .. } => *camera_up,
            ControllerJson::Enabled(_) => None,
        })
        .unwrap_or(200.0)
    }
}

/// Flags d'entité (champ réservé depuis la v1).
pub const ENTITY_FLAG_LIGHT: u16 = 1 << 0;
pub const ENTITY_FLAG_CAMERA: u16 = 1 << 1;
/// Modificateur du bit lumière : ponctuelle (torche) au lieu de
/// directionnelle. Le rayon vit dans le pad du vecteur échelle.
pub const ENTITY_FLAG_LIGHT_POINT: u16 = 1 << 2;
/// Collider AABB forcé (v1.4) : solide même sans modèle... (bit 4) — le
/// défaut runtime reste « solide si modèle » quand aucun bit n'est posé.
pub const ENTITY_FLAG_SOLID: u16 = 1 << 4;
/// ...ou traversable malgré un modèle (bit 5) : décor purement visuel.
pub const ENTITY_FLAG_NOT_SOLID: u16 = 1 << 5;
/// Character Controller (v1.4, bit 6) : ses paramètres logent dans les
/// pads d'entité (vitesse dans pos.pad, recul caméra dans rot.pad,
/// hauteur caméra dans scale.pad) — exclusif avec caméra et lumière.
pub const ENTITY_FLAG_CONTROLLER: u16 = 1 << 6;
/// v1.3 : l'entité porte des composants UI (voir la table UI).
pub const ENTITY_FLAG_UI: u16 = 1 << 3;

/// Hash FNV-1a 32 bits d'un nom de script (le runtime fait le même calcul).
pub fn script_hash(name: &str) -> u32 {
    let mut h = 0x811c9dc5u32;
    for b in name.bytes() {
        h ^= b.to_ascii_lowercase() as u32;
        h = h.wrapping_mul(0x01000193);
    }
    h
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

#[derive(Debug, Clone)]
pub struct BuildOptions {
    /// Repack textures into VRAM automatically (default). When false, the
    /// placements baked into the TIM files are kept as-is.
    pub pack_vram: bool,
    /// Dossier de résolution des références de prefab (le dossier projet,
    /// en général). Absent : résolues relativement à `base_dir`.
    pub prefab_dir: Option<std::path::PathBuf>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        BuildOptions { pack_vram: true, prefab_dir: None }
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

/// Résout les références de prefab d'une scène : chaque entité portant
/// `"prefab"` est remplacée par le sous-arbre du fichier (même schéma
/// qu'une scène). Les assets sont fusionnés (dédupliqués par id), la
/// racine du prefab prend le nom, le parent et la transform de
/// l'instance, les enfants sont nommés `<instance>.<enfant>` — la
/// console ne connaît pas les prefabs, tout est inliné.
pub fn resolve_prefabs(
    scene: &SceneJson,
    base_dir: &Path,
    options: &BuildOptions,
) -> Result<SceneJson, String> {
    let mut out = scene.clone();
    out.entities.clear();
    for e in &scene.entities {
        let Some(rel) = &e.prefab else {
            out.entities.push(e.clone());
            continue;
        };
        let dir = options.prefab_dir.as_deref().unwrap_or(base_dir);
        let path = dir.join(rel);
        let text = std::fs::read_to_string(&path)
            .map_err(|err| format!("prefab '{}': cannot read {}: {err}", e.name, path.display()))?;
        let prefab: SceneJson =
            serde_json::from_str(&text).map_err(|err| format!("prefab {rel}: {err}"))?;
        if prefab.entities.iter().any(|x| x.prefab.is_some()) {
            return Err(format!("prefab {rel}: prefabs imbriqués non supportés (v1)"));
        }
        let roots: Vec<usize> = prefab
            .entities
            .iter()
            .enumerate()
            .filter(|(_, x)| x.parent.is_none())
            .map(|(i, _)| i)
            .collect();
        if roots.len() != 1 {
            return Err(format!(
                "prefab {rel}: une seule entité racine attendue ({} trouvées)",
                roots.len()
            ));
        }
        for m in prefab.assets.models {
            if !out.assets.models.iter().any(|x| x.id == m.id) {
                out.assets.models.push(m);
            }
        }
        for t in prefab.assets.textures {
            if !out.assets.textures.iter().any(|x| x.id == t.id) {
                out.assets.textures.push(t);
            }
        }
        for f in prefab.assets.fonts {
            if !out.assets.fonts.iter().any(|x| x.id == f.id) {
                out.assets.fonts.push(f);
            }
        }
        // Renommage : racine -> nom de l'instance, enfants préfixés.
        let rename: HashMap<String, String> = prefab
            .entities
            .iter()
            .enumerate()
            .map(|(i, x)| {
                let new = if i == roots[0] {
                    e.name.clone()
                } else {
                    format!("{}.{}", e.name, x.name)
                };
                (x.name.clone(), new)
            })
            .collect();
        for (i, mut pe) in prefab.entities.into_iter().enumerate() {
            pe.name = rename[&pe.name].clone();
            if i == roots[0] {
                pe.parent = e.parent.clone();
                pe.position = e.position;
                pe.rotation = e.rotation;
                pe.scale = e.scale;
                if e.active == Some(false) {
                    pe.active = Some(false);
                }
            } else if let Some(p) = &pe.parent {
                pe.parent = Some(
                    rename
                        .get(p)
                        .cloned()
                        .ok_or_else(|| format!("prefab {rel}: parent '{p}' inconnu"))?,
                );
            }
            out.entities.push(pe);
        }
    }
    Ok(out)
}

pub fn build_with_options(
    scene: &SceneJson,
    base_dir: &Path,
    options: &BuildOptions,
) -> Result<(Vec<u8>, SceneReport), String> {
    // Références de prefab : expansion avant tout (une passe, sans
    // récursion — les prefabs imbriqués sont refusés en v1).
    if scene.entities.iter().any(|e| e.prefab.is_some()) {
        let expanded = resolve_prefabs(scene, base_dir, options)?;
        return build_with_options(&expanded, base_dir, options);
    }
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

    /* Polices UI (v1.3) : blobs .fnt, leur TIM interne est packé en VRAM
     * avec les textures. */
    let mut font_index: HashMap<&str, u8> = HashMap::new();
    let mut font_blobs: Vec<Vec<u8>> = Vec::new();
    for (i, font) in scene.assets.fonts.iter().enumerate() {
        if i >= 4 {
            return Err("4 polices max par scène".into());
        }
        if font_index.insert(&font.id, i as u8).is_some() {
            return Err(format!("duplicate font id '{}'", font.id));
        }
        let path = base_dir.join(&font.fnt);
        let data = std::fs::read(&path)
            .map_err(|e| format!("police '{}': cannot read {}: {e}", font.id, path.display()))?;
        crate::fnt::parse(&data).map_err(|e| format!("police '{}': {e}", font.id))?;
        font_blobs.push(data);
    }

    /* Automatic VRAM packing: place every texture page-aligned and rewrite
     * the coordinates baked in the TIM blobs (font atlases included). */
    if options.pack_vram {
        let mut requests: Vec<vram::TexRequest> = scene
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
        for (font, blob) in scene.assets.fonts.iter().zip(&font_blobs) {
            let tim_off = crate::fnt::parse(blob).unwrap().tim_offset;
            let (words, height, clut_entries) = tim_geometry(&font.id, &blob[tim_off..])?;
            requests.push(vram::TexRequest {
                label: format!("font:{}", font.id),
                words,
                height,
                clut_entries,
            });
        }
        let placements = vram::pack(&requests)?;
        let ntex = texture_blobs.len();
        for (blob, placement) in texture_blobs.iter_mut().zip(&placements[..ntex]) {
            tim_set_position(blob, placement);
        }
        for (blob, placement) in font_blobs.iter_mut().zip(&placements[ntex..]) {
            let tim_off = crate::fnt::parse(blob).unwrap().tim_offset;
            tim_set_position(&mut blob[tim_off..], placement);
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

    /* Scripts référencés (indices stables, dans l'ordre de première
     * apparition) — la table de hashes permet au runtime de résoudre les
     * fonctions par nom, indépendamment de l'ordre du registre compilé. */
    let mut script_names: Vec<String> = Vec::new();
    for &i in &order {
        if let Some(s) = &scene.entities[i].script {
            if !script_names.iter().any(|n| n == s) {
                script_names.push(s.clone());
            }
        }
    }
    if script_names.len() >= u16::MAX as usize {
        return Err("too many scripts".into());
    }

    /* PSX Script (v1.5) : un `scripts/<nom>.psxs` dans le projet prime
     * sur le registre C — compilé en bytecode PSB1 embarqué dans le
     * .psc, avec une table d'offsets (0 = script C) juste après la
     * table de hashes, signalée par le bit 0 des flags d'en-tête. */
    let script_dir = options
        .prefab_dir
        .as_deref()
        .unwrap_or(base_dir)
        .join("scripts");
    let mut vm_blobs: Vec<Option<Vec<u8>>> = Vec::with_capacity(script_names.len());
    for name in &script_names {
        let path = script_dir.join(format!("{name}.psxs"));
        if path.is_file() {
            let src = std::fs::read_to_string(&path)
                .map_err(|e| format!("{} : {e}", path.display()))?;
            let compiled = crate::psxs::compile(&src)
                .map_err(|e| format!("script '{name}' ({}) : {e}", path.display()))?;
            vm_blobs.push(Some(compiled.bytecode));
        } else {
            vm_blobs.push(None);
        }
    }
    let has_vm = vm_blobs.iter().any(Option::is_some);

    /* Serialize entities in sorted order. */
    let mut entities = Vec::with_capacity(n * ENTITY_SIZE);
    for &i in &order {
        let e = &scene.entities[i];
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
        // Les pads des trois vecteurs dépendent du rôle de l'entité — un
        // contrôleur y loge ses paramètres, d'où l'exclusivité avec
        // caméra et lumière (validée plus bas, erreur claire).
        let is_controller = e.controller.enabled();
        if is_controller && (e.camera.enabled() || e.light.is_some()) {
            return Err(format!(
                "entité '{}': un Character Controller ne peut pas aussi être \
                 caméra ou lumière (les pads d'entité portent ses paramètres)",
                e.name
            ));
        }
        // FOV caméra (v1.2) : logé dans le pad du vecteur position
        // (u16, degrés verticaux, 0 = défaut — pad nul sur les anciens
        // fichiers, donc rétrocompatible). Contrôleur : vitesse de marche.
        let pos_pad = if is_controller {
            let s = e.controller.speed();
            if !(1.0..=64.0).contains(&s) {
                return Err(format!(
                    "entité '{}': speed {s} hors plage (1-64 unités/frame)",
                    e.name
                ));
            }
            s.round() as u16
        } else {
            match e.camera.fov() {
                Some(f) => {
                    if !(10.0..=170.0).contains(&f) {
                        return Err(format!(
                            "entity '{}': fov {f} hors plage (10-170 degrés)",
                            e.name
                        ));
                    }
                    f.round() as u16
                }
                None => 0,
            }
        };
        for c in pos {
            entities.extend_from_slice(&c.to_le_bytes());
        }
        entities.extend_from_slice(&pos_pad.to_le_bytes());
        // Distance d'affichage caméra : pad du vecteur rotation (0 = infini).
        // Contrôleur : recul de la caméra suiveuse (0 = pas de suivi).
        let rot_pad = if is_controller {
            let b = if e.controller.camera() { e.controller.camera_back() } else { 0.0 };
            if !(0.0..=8192.0).contains(&b) {
                return Err(format!(
                    "entité '{}': camera_back {b} hors plage (0-8192 unités)",
                    e.name
                ));
            }
            b.round() as u16
        } else {
            match e.camera.draw_distance() {
                Some(d) => {
                    if !(100.0..=32767.0).contains(&d) {
                        return Err(format!(
                            "entity '{}': draw_distance {d} hors plage (100-32767 unités)",
                            e.name
                        ));
                    }
                    d.round() as u16
                }
                None => 0,
            }
        };
        for c in rot {
            entities.extend_from_slice(&c.to_le_bytes());
        }
        entities.extend_from_slice(&rot_pad.to_le_bytes());
        // Rayon de lumière ponctuelle : pad du vecteur échelle (0 sinon).
        // Contrôleur : hauteur de la caméra suiveuse.
        let scale_pad = if is_controller {
            let u = if e.controller.camera() { e.controller.camera_up() } else { 0.0 };
            if !(0.0..=8192.0).contains(&u) {
                return Err(format!(
                    "entité '{}': camera_up {u} hors plage (0-8192 unités)",
                    e.name
                ));
            }
            u.round() as u16
        } else {
            match &e.light {
                Some(l) if l.is_point() => {
                    let r = l.radius.unwrap_or(600.0);
                    if !(64.0..=8192.0).contains(&r) {
                        return Err(format!(
                            "entity '{}': radius {r} hors plage (64-8192 unités)",
                            e.name
                        ));
                    }
                    r.round() as u16
                }
                Some(l) => {
                    if l.radius.is_some() {
                        report.warnings.push(format!(
                            "entité '{}' : radius ignoré (lumière directionnelle)",
                            e.name
                        ));
                    }
                    0
                }
                None => 0,
            }
        };
        for c in scale {
            entities.extend_from_slice(&c.to_le_bytes());
        }
        entities.extend_from_slice(&scale_pad.to_le_bytes());
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
        let mut flags = 0u16;
        if let Some(light) = &e.light {
            flags |= ENTITY_FLAG_LIGHT;
            match light.kind.as_deref() {
                None | Some("directional") => {}
                Some("point") => flags |= ENTITY_FLAG_LIGHT_POINT,
                Some(other) => {
                    return Err(format!(
                        "entity '{}': type de lumière inconnu '{other}' (directional|point)",
                        e.name
                    ))
                }
            }
        }
        if e.camera.enabled() {
            flags |= ENTITY_FLAG_CAMERA;
        }
        if e.canvas.unwrap_or(false)
            || e.rect.is_some()
            || e.image.is_some()
            || e.text.is_some()
            || e.button.unwrap_or(false)
            || e.layout.is_some()
        {
            flags |= ENTITY_FLAG_UI;
        }
        match e.solid {
            Some(true) => flags |= ENTITY_FLAG_SOLID,
            Some(false) => flags |= ENTITY_FLAG_NOT_SOLID,
            None => {}
        }
        if is_controller {
            flags |= ENTITY_FLAG_CONTROLLER;
        }
        if e.light.is_some() && e.camera.enabled() {
            report.warnings.push(format!(
                "entité '{}' : lumière ET caméra sur la même entité — choisis un rôle (l'éditeur les rend exclusifs)",
                e.name
            ));
        }
        entities.extend_from_slice(&flags.to_le_bytes());
        // Script : indice+1 dans la table (0 = aucun) — les fichiers
        // antérieurs ont 0 ici, donc restent valides.
        let script_ref = match &e.script {
            Some(s) => (script_names.iter().position(|n| n == s).unwrap() + 1) as u16,
            None => 0,
        };
        entities.extend_from_slice(&script_ref.to_le_bytes());
    }

    /* Table des lumières (v1.2) : entités-lumières dans l'ordre du
     * fichier. Le GTE offre 3 directionnelles ; la lumière des settings
     * occupe la ligne 0, donc 2 entités max — le surplus est ignoré. */
    let mut lights: Vec<(u16, [u8; 3], u8)> = Vec::new();
    let mut dir_count = 0usize;
    let mut point_count = 0usize;
    for &i in &order {
        let e = &scene.entities[i];
        if let Some(light) = &e.light {
            if light.is_point() {
                if point_count >= 4 {
                    report.warnings.push(format!(
                        "torche '{}' ignorée : 4 lumières ponctuelles max par scène",
                        e.name
                    ));
                    continue;
                }
                point_count += 1;
            } else {
                if dir_count >= 2 {
                    report.warnings.push(format!(
                        "lumière '{}' ignorée : 3 directionnelles max sur le GTE (settings + 2 entités)",
                        e.name
                    ));
                    continue;
                }
                dir_count += 1;
            }
            // Intensité en pourcent dans l'octet de pad (0 = 100 %).
            let intensity = match light.intensity {
                Some(v) => {
                    if !(0.1..=2.5).contains(&v) {
                        return Err(format!(
                            "entité '{}': intensity {v} hors plage (0.1-2.5)",
                            e.name
                        ));
                    }
                    (v * 100.0).round() as u8
                }
                None => 0,
            };
            lights.push((pos_to_sorted[i], light.color, intensity));
        }
    }

    /* Table UI (v1.3) : un enregistrement de 40 octets par entité UI,
     * dans l'ordre du fichier (parents d'abord), + table de chaînes. */
    let mut ui_recs: Vec<u8> = Vec::new();
    let mut ui_strings: Vec<u8> = Vec::new();
    let mut ui_count = 0usize;
    for &i in &order {
        let e = &scene.entities[i];
        let is_ui = e.canvas.unwrap_or(false)
            || e.rect.is_some()
            || e.image.is_some()
            || e.text.is_some()
            || e.button.unwrap_or(false)
            || e.layout.is_some();
        if !is_ui {
            continue;
        }
        ui_count += 1;
        let mut components = 0u8;
        let mut uflags = 0u8;
        let mut asset = 0xFFu8;
        let mut data = 0u16;
        let mut extra = 0u16;
        let mut uv = [0u8; 4];
        let mut border = [0u8; 4];
        let mut color = [128u8, 128, 128];

        if e.canvas.unwrap_or(false) {
            components |= 1 << 0;
        }
        if e.active.unwrap_or(true) {
            components |= 1 << 5;
        }
        if let Some(img) = &e.image {
            components |= 1 << 1;
            match img.kind.as_deref() {
                None | Some("simple") => {}
                Some("sliced") => uflags |= 1,
                Some("tiled") => uflags |= 2,
                Some("filled") => uflags |= 3,
                Some(other) => {
                    return Err(format!(
                        "entité '{}': image type '{other}' inconnu (simple|sliced|tiled|filled)",
                        e.name
                    ))
                }
            }
            match img.fill.as_deref() {
                None | Some("horizontal") => {}
                Some("vertical") => uflags |= 1 << 2,
                Some(other) => {
                    return Err(format!(
                        "entité '{}': fill '{other}' inconnu (horizontal|vertical)",
                        e.name
                    ))
                }
            }
            if img.semi_transparent.unwrap_or(false) {
                uflags |= 1 << 3;
            }
            if let Some(id) = &img.texture {
                asset = *texture_index
                    .get(id.as_str())
                    .ok_or(format!("entité '{}': texture UI inconnue '{id}'", e.name))?
                    as u8;
            } else {
                color = [255, 255, 255];
            }
            if let Some(c) = img.color {
                color = c;
            }
            if let Some(r) = img.uv {
                for (k, v) in r.iter().enumerate() {
                    if *v > 255 {
                        return Err(format!(
                            "entité '{}': uv {v} hors page (0-255 texels)",
                            e.name
                        ));
                    }
                    uv[k] = *v as u8;
                }
            }
            if let Some(b) = img.border {
                border = b;
            }
            let amount = img.amount.unwrap_or(1.0).clamp(0.0, 1.0);
            data = (amount * ONE_4_12 as f32).round() as u16;
        }
        if let Some(text) = &e.text {
            components |= 1 << 2;
            asset = *font_index
                .get(text.font.as_str())
                .ok_or(format!("entité '{}': police inconnue '{}'", e.name, text.font))?;
            if ui_strings.len() > u16::MAX as usize {
                return Err("table de chaînes UI pleine (64 Ko)".into());
            }
            data = ui_strings.len() as u16;
            ui_strings.extend_from_slice(text.text.as_bytes());
            ui_strings.push(0);
            extra = match text.align.as_deref() {
                None | Some("left") => 0,
                Some("center") => 1,
                Some("right") => 2,
                Some(other) => {
                    return Err(format!(
                        "entité '{}': align '{other}' inconnu (left|center|right)",
                        e.name
                    ))
                }
            };
            if let Some(c) = text.color {
                color = c;
            } else {
                color = [255, 255, 255];
            }
        }
        if e.button.unwrap_or(false) {
            components |= 1 << 3;
        }
        if let Some(layout) = &e.layout {
            components |= 1 << 4;
            match layout.axis.as_str() {
                "vertical" => {}
                "horizontal" => uflags |= 1 << 4,
                other => {
                    return Err(format!(
                        "entité '{}': layout axis '{other}' inconnu (vertical|horizontal)",
                        e.name
                    ))
                }
            }
            if layout.expand_w {
                uflags |= 1 << 5;
            }
            if layout.expand_h {
                uflags |= 1 << 6;
            }
            // Un layout n'a ni sprite ni 9-slice : ses champs uv/border
            // portent padding [g, h, d, b] et alignement des enfants.
            extra = layout.spacing.clamp(0, 255) as u16;
            for (k, p) in layout.padding.iter().enumerate() {
                uv[k] = (*p).clamp(0, 255) as u8;
            }
            border[0] = match layout.child_align.as_deref() {
                None | Some("top-left") => 0,
                Some("top-center") => 1,
                Some("top-right") => 2,
                Some("middle-left") => 3,
                Some("middle-center") => 4,
                Some("middle-right") => 5,
                Some("bottom-left") => 6,
                Some("bottom-center") => 7,
                Some("bottom-right") => 8,
                Some(other) => {
                    return Err(format!(
                        "entité '{}': child_align '{other}' inconnu",
                        e.name
                    ))
                }
            };
        }

        // Un canvas sans rect explicite couvre tout l'écran (ancres
        // étirées) — le défaut « posé au centre » n'a de sens que pour
        // les widgets enfants.
        let rect = e.rect.clone().unwrap_or_else(|| {
            if e.canvas.unwrap_or(false) {
                RectJson {
                    anchor_min: [0.0, 0.0],
                    anchor_max: [1.0, 1.0],
                    pivot: [0.0, 0.0],
                    position: [0.0, 0.0],
                    size: [0.0, 0.0],
                }
            } else {
                RectJson::default()
            }
        });
        let frac = |v: f32| (v.clamp(0.0, 1.0) * ONE_4_12 as f32).round() as u16;
        ui_recs.extend_from_slice(&pos_to_sorted[i].to_le_bytes());
        ui_recs.push(components);
        ui_recs.push(uflags);
        for v in [
            rect.anchor_min[0],
            rect.anchor_min[1],
            rect.anchor_max[0],
            rect.anchor_max[1],
            rect.pivot[0],
            rect.pivot[1],
        ] {
            ui_recs.extend_from_slice(&frac(v).to_le_bytes());
        }
        for v in [rect.position[0], rect.position[1], rect.size[0], rect.size[1]] {
            ui_recs.extend_from_slice(&quantize_i16(v, "rect", &e.name)?.to_le_bytes());
        }
        ui_recs.extend_from_slice(&color);
        ui_recs.push(asset);
        ui_recs.extend_from_slice(&data.to_le_bytes());
        ui_recs.extend_from_slice(&extra.to_le_bytes());
        ui_recs.extend_from_slice(&uv);
        ui_recs.extend_from_slice(&border);
    }
    debug_assert_eq!(ui_recs.len(), ui_count * UI_ENTRY_SIZE);

    /* Layout: header | model table | texture table | entities | scripts
     * (table de hashes) | lights | table UI | table polices | chaînes UI
     * | blobs. Les offsets UI se dérivent des précédents (pas de place
     * dans l'en-tête) : ui = align4(fin des lumières). */
    let models_offset = HEADER_SIZE;
    let textures_offset = models_offset + model_blobs.len() * MODEL_ENTRY_SIZE;
    let entities_offset = textures_offset + texture_blobs.len() * TEXTURE_ENTRY_SIZE;
    let scripts_offset = entities_offset + entities.len();
    // v1.5 : la table d'offsets bytecode double la table de hashes.
    let scripts_table_len = script_names.len() * 4 * if has_vm { 2 } else { 1 };
    let lights_offset = scripts_offset + scripts_table_len;
    let align4 = |v: usize| (v + 3) & !3;
    let ui_offset = align4(lights_offset + lights.len() * LIGHT_ENTRY_SIZE);
    let fonts_offset = ui_offset + ui_recs.len();
    let strings_offset = fonts_offset + font_blobs.len() * FONT_ENTRY_SIZE;
    let mut blob_cursor = strings_offset + ui_strings.len();
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
    let mut font_entries = Vec::new();
    let mut font_offsets = Vec::new();
    for blob in &font_blobs {
        blob_cursor = align4(blob_cursor);
        font_offsets.push(blob_cursor);
        font_entries.extend_from_slice(&(blob_cursor as u32).to_le_bytes());
        font_entries.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        blob_cursor += blob.len();
    }
    // Blobs bytecode PSX Script, places comme les autres blobs.
    let mut vm_offsets: Vec<u32> = Vec::with_capacity(vm_blobs.len());
    for blob in &vm_blobs {
        match blob {
            Some(b) => {
                blob_cursor = align4(blob_cursor);
                vm_offsets.push(blob_cursor as u32);
                blob_cursor += b.len();
            }
            None => vm_offsets.push(0),
        }
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
    // Flags d'en-tête : bit 0 = table d'offsets bytecode (v1.5).
    out.extend_from_slice(&(if has_vm { 1u16 } else { 0 }).to_le_bytes());
    out.extend_from_slice(&(total_size as u32).to_le_bytes());
    // Le 4e compteur (réservé jusqu'à la v1.2) devient le nombre de
    // widgets UI — nul sur les anciens fichiers, donc rétrocompatible.
    for count in [model_blobs.len(), texture_blobs.len(), n, ui_count] {
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
    // Extension v1.1 dans les octets réservés : table des scripts.
    out.extend_from_slice(&(script_names.len() as u16).to_le_bytes());
    out.extend_from_slice(&(scripts_offset as u32).to_le_bytes());
    // Extension v1.2 : table des lumières (0x38 offset, 0x3C count).
    out.extend_from_slice(&(lights_offset as u32).to_le_bytes());
    out.extend_from_slice(&(lights.len() as u16).to_le_bytes());
    // Extension v1.3 : nombre de polices (0x3E) ; les offsets des tables
    // UI/polices/chaînes se dérivent de la fin des lumières.
    out.extend_from_slice(&(font_blobs.len() as u16).to_le_bytes());
    out.resize(HEADER_SIZE, 0); // reserved
    out.extend_from_slice(&model_entries);
    out.extend_from_slice(&texture_entries);
    out.extend_from_slice(&entities);
    for name in &script_names {
        out.extend_from_slice(&script_hash(name).to_le_bytes());
    }
    if has_vm {
        for off in &vm_offsets {
            out.extend_from_slice(&off.to_le_bytes());
        }
    }
    for (entity, color, intensity) in &lights {
        out.extend_from_slice(&entity.to_le_bytes());
        out.extend_from_slice(&[color[0], color[1], color[2], *intensity]);
    }
    out.resize(ui_offset, 0);
    out.extend_from_slice(&ui_recs);
    out.extend_from_slice(&font_entries);
    out.extend_from_slice(&ui_strings);
    for (offset, (blob, _)) in model_offsets.iter().zip(&model_blobs) {
        out.resize(*offset, 0);
        out.extend_from_slice(blob);
    }
    for (offset, blob) in texture_offsets.iter().zip(&texture_blobs) {
        out.resize(*offset, 0);
        out.extend_from_slice(blob);
    }
    for (offset, blob) in font_offsets.iter().zip(&font_blobs) {
        out.resize(*offset, 0);
        out.extend_from_slice(blob);
    }
    for (offset, blob) in vm_offsets.iter().zip(&vm_blobs) {
        if let Some(b) = blob {
            out.resize(*offset as usize, 0);
            out.extend_from_slice(b);
        }
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
    /// Bit 0 (v1.5) : table d'offsets bytecode PSX Script présente.
    pub flags: u16,
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
    pub script_count: u16,
    pub scripts_offset: u32,
    pub lights_offset: u32,
    pub light_count: u16,
    /// v1.3 : widgets UI (4e compteur) et polices (0x3E).
    pub ui_count: u16,
    pub font_count: u16,
}

/// Table d'offsets bytecode PSX Script (v1.5, flag bit 0) : un u32 par
/// script, 0 = script C du registre, sinon offset du blob PSB1.
pub fn parse_vm_offsets(data: &[u8], h: &PscHeader) -> Vec<u32> {
    if h.flags & 1 == 0 {
        return vec![0; h.script_count as usize];
    }
    let base = h.scripts_offset as usize + h.script_count as usize * 4;
    (0..h.script_count as usize)
        .map(|i| u32::from_le_bytes(data[base + i * 4..base + i * 4 + 4].try_into().unwrap()))
        .collect()
}

impl PscHeader {
    /// Offsets dérivés des tables v1.3 : (ui, polices, chaînes).
    pub fn ui_offsets(&self) -> (usize, usize, usize) {
        let ui = (self.lights_offset as usize + self.light_count as usize * LIGHT_ENTRY_SIZE + 3)
            & !3;
        let fonts = ui + self.ui_count as usize * UI_ENTRY_SIZE;
        let strings = fonts + self.font_count as usize * FONT_ENTRY_SIZE;
        (ui, fonts, strings)
    }
}

/// Un enregistrement de la table UI (v1.3), champs bruts du fichier.
#[derive(Debug, Clone)]
pub struct PscUiRec {
    pub entity: u16,
    pub components: u8,
    pub flags: u8,
    /// anchor_min, anchor_max, pivot en 4.12.
    pub anchors: [u16; 6],
    pub pos: [i16; 2],
    pub size: [i16; 2],
    pub color: [u8; 3],
    pub asset: u8,
    pub data: u16,
    pub extra: u16,
    pub uv: [u8; 4],
    pub border: [u8; 4],
}

pub fn parse_ui(data: &[u8], header: &PscHeader) -> Vec<PscUiRec> {
    let (base, _, _) = header.ui_offsets();
    let mut out = Vec::new();
    for i in 0..header.ui_count as usize {
        let o = base + i * UI_ENTRY_SIZE;
        let u16at = |k: usize| u16::from_le_bytes([data[o + k], data[o + k + 1]]);
        let i16at = |k: usize| i16::from_le_bytes([data[o + k], data[o + k + 1]]);
        out.push(PscUiRec {
            entity: u16at(0),
            components: data[o + 2],
            flags: data[o + 3],
            anchors: [u16at(4), u16at(6), u16at(8), u16at(10), u16at(12), u16at(14)],
            pos: [i16at(16), i16at(18)],
            size: [i16at(20), i16at(22)],
            color: [data[o + 24], data[o + 25], data[o + 26]],
            asset: data[o + 27],
            data: u16at(28),
            extra: u16at(30),
            uv: [data[o + 32], data[o + 33], data[o + 34], data[o + 35]],
            border: [data[o + 36], data[o + 37], data[o + 38], data[o + 39]],
        });
    }
    out
}

/// Blobs .fnt embarqués : (offset, taille) par police.
pub fn parse_fonts(data: &[u8], header: &PscHeader) -> Vec<(usize, usize)> {
    let (_, base, _) = header.ui_offsets();
    (0..header.font_count as usize)
        .map(|i| {
            let o = base + i * FONT_ENTRY_SIZE;
            let u32at = |k: usize| u32::from_le_bytes(data[o + k..o + k + 4].try_into().unwrap());
            (u32at(0) as usize, u32at(4) as usize)
        })
        .collect()
}

/// Une entrée de la table des lumières (v1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PscLight {
    /// Indice d'entité (ordre du fichier) : sa rotation donne la direction.
    pub entity: u16,
    pub color: [u8; 3],
    /// Intensité en pourcent (0 = 100).
    pub intensity_percent: u8,
}

/// Parse la table des lumières d'un .psc (vide pour les fichiers < v1.2).
pub fn parse_lights(data: &[u8], header: &PscHeader) -> Vec<PscLight> {
    let mut lights = Vec::new();
    let base = header.lights_offset as usize;
    for i in 0..header.light_count as usize {
        let o = base + i * LIGHT_ENTRY_SIZE;
        if o + LIGHT_ENTRY_SIZE > data.len() {
            break;
        }
        lights.push(PscLight {
            entity: u16::from_le_bytes([data[o], data[o + 1]]),
            color: [data[o + 2], data[o + 3], data[o + 4]],
            intensity_percent: data[o + 5],
        });
    }
    lights
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
        flags: u16at(6),
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
        script_count: u16at(50),
        scripts_offset: u32at(52),
        lights_offset: u32at(56),
        light_count: u16at(60),
        ui_count: u16at(18),
        font_count: u16at(62),
    };
    if h.version != VERSION {
        return Err(format!("unsupported PSC version {}", h.version));
    }
    Ok(h)
}
