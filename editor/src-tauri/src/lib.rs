// Backend Tauri de l'éditeur PSX Studio : accès au projet sur disque,
// build de scènes/projet via psxpipe (dépendance directe), et Play Mode
// (lancement de PCSX-Redux + pilotage par son API web).

use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;

/* ------------------------------------------------------------- projet -- */

#[derive(Serialize)]
pub struct SceneEntry {
    pub name: String,
    /// Chemin relatif au dossier projet (tel que dans project.json).
    pub path: String,
}

#[derive(Serialize)]
pub struct ProjectInfo {
    pub name: String,
    pub dir: String,
    pub scenes: Vec<SceneEntry>,
    pub exe_present: bool,
}

#[tauri::command]
fn open_project(path: String) -> Result<ProjectInfo, String> {
    let dir = PathBuf::from(&path);
    let json_path = dir.join("project.json");
    let text = std::fs::read_to_string(&json_path)
        .map_err(|e| format!("{} : {e}", json_path.display()))?;
    let project: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("project.json : {e}"))?;

    let mut scenes = Vec::new();
    for scene_rel in project["scenes"].as_array().into_iter().flatten() {
        let Some(rel) = scene_rel.as_str() else { continue };
        // Le nom vient du JSON de scène lui-même.
        let name = std::fs::read_to_string(dir.join(rel))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v["name"].as_str().map(String::from))
            .unwrap_or_else(|| rel.to_string());
        scenes.push(SceneEntry {
            name,
            path: rel.to_string(),
        });
    }

    let exe_present = project["exe"]
        .as_str()
        .map(|e| dir.join(e).exists())
        .unwrap_or(false);

    Ok(ProjectInfo {
        name: project["name"].as_str().unwrap_or("projet").to_string(),
        dir: path,
        scenes,
        exe_present,
    })
}

#[tauri::command]
fn load_scene(project_dir: String, scene_path: String) -> Result<String, String> {
    let path = PathBuf::from(project_dir).join(scene_path);
    std::fs::read_to_string(&path).map_err(|e| format!("{} : {e}", path.display()))
}

#[tauri::command]
fn save_scene(project_dir: String, scene_path: String, contents: String) -> Result<(), String> {
    // Validation avant écriture : un JSON invalide ne doit jamais écraser
    // la scène sur disque.
    serde_json::from_str::<psxpipe::scene::SceneJson>(&contents)
        .map_err(|e| format!("scène invalide : {e}"))?;
    let path = PathBuf::from(project_dir).join(scene_path);
    std::fs::write(&path, contents).map_err(|e| format!("{} : {e}", path.display()))
}

/* -------------------------------------------------------------- build -- */

#[derive(Serialize)]
pub struct VramEntry {
    pub label: String,
    pub x: u16,
    pub y: u16,
    pub words: u16,
    pub height: u16,
    pub clut_x: u16,
    pub clut_y: u16,
    pub clut_entries: u16,
}

#[derive(Serialize)]
pub struct BuiltScene {
    /// Le .psc packé, prêt pour les parsers du viewport.
    pub psc: Vec<u8>,
    /// Noms d'entités dans l'ordre du fichier (tri topologique) : fait le
    /// lien entre les indices du viewport et les entités du JSON.
    pub entity_names: Vec<String>,
    pub vram: Vec<VramEntry>,
    pub warnings: Vec<String>,
}

/// Construit une scène depuis son JSON (contenu fourni, pas le fichier :
/// permet la préview des éditions non sauvegardées). Les assets sont
/// résolus dans Library/ puis dans le dossier de la scène.
#[tauri::command]
fn build_scene(
    project_dir: String,
    scene_path: String,
    contents: String,
) -> Result<BuiltScene, String> {
    let scene: psxpipe::scene::SceneJson =
        serde_json::from_str(&contents).map_err(|e| format!("scène invalide : {e}"))?;
    let project = PathBuf::from(&project_dir);
    let library = project.join("Library");
    let base = if library.exists() {
        library
    } else {
        project
            .join(&scene_path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or(project)
    };
    let (psc, report) = psxpipe::scene::build(&scene, &base)?;
    Ok(BuiltScene {
        psc,
        entity_names: report.entity_names,
        vram: report
            .vram
            .iter()
            .map(|(req, p)| VramEntry {
                label: req.label.clone(),
                x: p.x,
                y: p.y,
                words: req.words,
                height: req.height,
                clut_x: p.clut_x,
                clut_y: p.clut_y,
                clut_entries: req.clut_entries,
            })
            .collect(),
        warnings: report.warnings,
    })
}

#[derive(Serialize)]
pub struct BuildSummary {
    pub converted: usize,
    pub cached: usize,
    pub cue_path: String,
    pub mkpsxiso_ran: bool,
    pub warnings: Vec<String>,
}

#[tauri::command]
fn build_project(project_dir: String) -> Result<BuildSummary, String> {
    let dir = PathBuf::from(&project_dir);
    let report = psxpipe::project::build(&dir, false)?;
    let name = std::fs::read_to_string(dir.join("project.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["name"].as_str().map(String::from))
        .unwrap_or_else(|| "projet".into());
    Ok(BuildSummary {
        converted: report.converted,
        cached: report.cached,
        cue_path: dir
            .join("Build")
            .join(format!("{name}.cue"))
            .to_string_lossy()
            .into_owned(),
        mkpsxiso_ran: report.mkpsxiso_ran,
        warnings: report.warnings,
    })
}

/// Import d'un asset source (glTF/GLB/PNG) déposé dans l'éditeur.
#[tauri::command]
fn import_asset(
    project_dir: String,
    src_path: String,
) -> Result<psxpipe::project::ImportedAsset, String> {
    psxpipe::project::import_asset(&PathBuf::from(project_dir), &PathBuf::from(src_path))
}

/* ---------------------------------------------------------- play mode -- */

#[tauri::command]
fn play(
    project_dir: String,
    emulator_path: Option<String>,
    port: Option<u16>,
) -> Result<BuildSummary, String> {
    let summary = build_project(project_dir)?;
    if !summary.mkpsxiso_ran {
        return Err(
            "mkpsxiso introuvable : l'ISO n'a pas été générée (voir PATH)".into(),
        );
    }
    let mut emulator = PathBuf::from(emulator_path.unwrap_or_else(|| "pcsx-redux".into()));
    // Tolérance : si on nous donne le dossier d'installation, chercher
    // l'exécutable dedans (erreur classique sous Windows -> "Accès refusé").
    if emulator.is_dir() {
        for candidate in ["pcsx-redux.exe", "pcsx-redux"] {
            let full = emulator.join(candidate);
            if full.is_file() {
                emulator = full;
                break;
            }
        }
        if emulator.is_dir() {
            return Err(format!(
                "{} est un dossier et ne contient pas pcsx-redux.exe — indique le chemin de l'exécutable",
                emulator.display()
            ));
        }
    }
    let port = port.unwrap_or(psxpipe::redux::DEFAULT_PORT);
    // Nouvelle exécution -> la balise live-tweak devra être relocalisée.
    *BEACON.lock().unwrap() = None;
    let args = psxpipe::redux::launch_args(&summary.cue_path, port);
    Command::new(&emulator)
        .args(&args)
        .spawn()
        .map_err(|e| format!("lancement de {} impossible : {e}", emulator.display()))?;
    Ok(summary)
}

#[tauri::command]
fn redux_status(port: Option<u16>) -> Result<bool, String> {
    let client = psxpipe::redux::ReduxClient::new(port.unwrap_or(psxpipe::redux::DEFAULT_PORT));
    Ok(client.status()?.running)
}

#[tauri::command]
fn redux_pause(port: Option<u16>) -> Result<(), String> {
    psxpipe::redux::ReduxClient::new(port.unwrap_or(psxpipe::redux::DEFAULT_PORT)).pause()
}

#[tauri::command]
fn redux_resume(port: Option<u16>) -> Result<(), String> {
    psxpipe::redux::ReduxClient::new(port.unwrap_or(psxpipe::redux::DEFAULT_PORT)).resume()
}

#[tauri::command]
fn redux_reset(port: Option<u16>) -> Result<(), String> {
    psxpipe::redux::ReduxClient::new(port.unwrap_or(psxpipe::redux::DEFAULT_PORT)).reset()
}

/* --------------------------------------------------------- live tweak -- */

// Balise mémorisée après le premier scan RAM : la localiser coûte un dump
// de 2 Mo, la réutiliser coûte trois petits POST.
static BEACON: std::sync::Mutex<Option<psxpipe::redux::Beacon>> = std::sync::Mutex::new(None);

/// À appeler quand la cible change (nouveau Play, reset, autre scène) :
/// la balise sera relocalisée au prochain sync.
#[tauri::command]
fn redux_clear_beacon() {
    *BEACON.lock().unwrap() = None;
}

/// Écrit la transform d'une entité dans la RAM console pendant que le jeu
/// tourne. `index` est l'indice dans l'ordre du fichier .psc (entity_names).
/// Rotation en unités PS1 (4096 = tour), échelle en 4.12.
#[tauri::command]
fn redux_sync_entity(
    port: Option<u16>,
    index: u16,
    pos: [i32; 3],
    rot: [i16; 3],
    scale: [i16; 3],
) -> Result<(), String> {
    let client =
        psxpipe::redux::ReduxClient::new(port.unwrap_or(psxpipe::redux::DEFAULT_PORT));
    let cached = *BEACON.lock().unwrap();
    let beacon = match cached {
        Some(b) => b,
        None => {
            let b = client.locate_beacon()?;
            *BEACON.lock().unwrap() = Some(b);
            b
        }
    };
    match client.write_entity_transform(&beacon, index, pos, rot, scale) {
        Ok(()) => Ok(()),
        Err(e) => {
            // La balise peut être périmée (reset, autre build) : on
            // invalide pour retenter proprement au prochain appel.
            *BEACON.lock().unwrap() = None;
            Err(e)
        }
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            open_project,
            load_scene,
            save_scene,
            build_scene,
            build_project,
            import_asset,
            play,
            redux_status,
            redux_pause,
            redux_resume,
            redux_reset,
            redux_clear_beacon,
            redux_sync_entity,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de PSX Studio");
}
