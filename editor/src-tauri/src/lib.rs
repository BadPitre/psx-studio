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
pub struct BuiltScene {
    /// Le .psc packé, prêt pour les parsers du viewport.
    pub psc: Vec<u8>,
    /// Noms d'entités dans l'ordre du fichier (tri topologique) : fait le
    /// lien entre les indices du viewport et les entités du JSON.
    pub entity_names: Vec<String>,
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

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            open_project,
            load_scene,
            save_scene,
            build_scene,
            build_project,
            play,
            redux_status,
            redux_pause,
            redux_resume,
            redux_reset,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de PSX Studio");
}
