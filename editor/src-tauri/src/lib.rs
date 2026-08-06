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

#[tauri::command(async)]
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

#[tauri::command(async)]
fn load_scene(project_dir: String, scene_path: String) -> Result<String, String> {
    let path = PathBuf::from(project_dir).join(scene_path);
    std::fs::read_to_string(&path).map_err(|e| format!("{} : {e}", path.display()))
}

#[tauri::command(async)]
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
#[tauri::command(async)]
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
    let (psc, report) = psxpipe::scene::build_with_options(
        &scene,
        &base,
        &psxpipe::scene::BuildOptions {
            prefab_dir: Some(PathBuf::from(&project_dir)),
            ..Default::default()
        },
    )?;
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

#[tauri::command(async)]
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

/// Seuil de subdivision anti-warping d'un modèle (par nom de .pmd).
#[tauri::command(async)]
fn get_model_subdiv(project_dir: String, pmd_out: String) -> Result<Option<f32>, String> {
    psxpipe::project::model_subdiv(&PathBuf::from(project_dir), &pmd_out)
}

/// Change le seuil (None = off) : écrit project.json et reconvertit le
/// modèle dans Library/ pour que le viewport voie le résultat.
#[tauri::command(async)]
fn set_model_subdiv(
    project_dir: String,
    pmd_out: String,
    subdiv: Option<f32>,
) -> Result<String, String> {
    psxpipe::project::set_model_subdiv(&PathBuf::from(project_dir), &pmd_out, subdiv)
}

/// Import d'un asset source (glTF/GLB/PNG) déposé dans l'éditeur.
#[tauri::command(async)]
fn import_asset(
    project_dir: String,
    src_path: String,
) -> Result<psxpipe::project::ImportedAsset, String> {
    psxpipe::project::import_asset(&PathBuf::from(project_dir), &PathBuf::from(src_path))
}

/// Import par contenu : le drag & drop HTML5 ne donne pas le chemin du
/// fichier deposé (dragDropEnabled est coupé pour que le drag & drop
/// interne de l'éditeur fonctionne sous Windows) — le frontend envoie
/// les octets, écrits dans assets/ puis importés en place.
#[tauri::command(async)]
fn import_asset_bytes(
    project_dir: String,
    name: String,
    bytes: Vec<u8>,
) -> Result<psxpipe::project::ImportedAsset, String> {
    let dir = PathBuf::from(project_dir);
    // Nom de fichier seul (pas de traversée), déposé dans assets/.
    let file_name = std::path::Path::new(&name)
        .file_name()
        .ok_or_else(|| format!("nom de fichier invalide : {name}"))?;
    let assets = dir.join("assets");
    std::fs::create_dir_all(&assets).map_err(|e| format!("assets/ : {e}"))?;
    let dst = assets.join(file_name);
    std::fs::write(&dst, &bytes).map_err(|e| format!("{} : {e}", dst.display()))?;
    psxpipe::project::import_asset(&dir, &dst)
}

/// Contenu du projet pour le panneau Project (fichiers + project.json).
#[tauri::command(async)]
fn list_project_files(
    project_dir: String,
) -> Result<Vec<psxpipe::project::ProjectFile>, String> {
    psxpipe::project::list_files(&PathBuf::from(project_dir))
}

/// Crée une scène vide et l'enregistre (menu « Créer ▸ Scène »).
/// Retourne son chemin relatif.
#[tauri::command(async)]
fn create_scene(project_dir: String, name: String) -> Result<String, String> {
    psxpipe::project::create_scene(&PathBuf::from(project_dir), &name)
}

/// Sauvegarde un prefab (drag hiérarchie -> panneau Project).
#[tauri::command(async)]
fn save_prefab(project_dir: String, name: String, contents: String) -> Result<String, String> {
    psxpipe::project::save_prefab(&PathBuf::from(project_dir), &name, &contents)
}

/// Crée un dossier (menu « Créer ▸ Dossier ») ; retourne son chemin relatif.
#[tauri::command(async)]
fn create_folder(project_dir: String, parent: String, name: String) -> Result<String, String> {
    psxpipe::project::create_folder(&PathBuf::from(project_dir), &parent, &name)
}

/// Déplace un fichier vers un dossier du projet (drag & drop du panneau)
/// et met à jour project.json ; retourne le nouveau chemin relatif.
#[tauri::command(async)]
fn move_entry(project_dir: String, from: String, to_dir: String) -> Result<String, String> {
    psxpipe::project::move_entry(&PathBuf::from(project_dir), &from, &to_dir)
}

/// Lit un fichier converti de Library/ (vignettes du panneau Project).
/// `name` est un nom de fichier simple, pas un chemin.
#[tauri::command(async)]
fn read_library_file(project_dir: String, name: String) -> Result<Vec<u8>, String> {
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(format!("nom invalide : {name}"));
    }
    let path = PathBuf::from(project_dir).join("Library").join(&name);
    std::fs::read(&path).map_err(|e| format!("{} : {e}", path.display()))
}

/* ---------------------------------------------------------- play mode -- */

#[tauri::command(async)]
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

#[tauri::command(async)]
fn redux_status(port: Option<u16>) -> Result<bool, String> {
    // Timeout court : ce poll tourne toutes les 1,5 s, il doit échouer
    // vite quand l'émulateur a été fermé.
    let client = psxpipe::redux::ReduxClient::new(port.unwrap_or(psxpipe::redux::DEFAULT_PORT))
        .with_timeout(std::time::Duration::from_millis(800));
    Ok(client.status()?.running)
}

#[tauri::command(async)]
fn redux_pause(port: Option<u16>) -> Result<(), String> {
    psxpipe::redux::ReduxClient::new(port.unwrap_or(psxpipe::redux::DEFAULT_PORT)).pause()
}

#[tauri::command(async)]
fn redux_resume(port: Option<u16>) -> Result<(), String> {
    psxpipe::redux::ReduxClient::new(port.unwrap_or(psxpipe::redux::DEFAULT_PORT)).resume()
}

#[tauri::command(async)]
fn redux_reset(port: Option<u16>) -> Result<(), String> {
    psxpipe::redux::ReduxClient::new(port.unwrap_or(psxpipe::redux::DEFAULT_PORT)).reset()
}

/* --------------------------------------------------------- live tweak -- */

// Balise mémorisée après le premier scan RAM : la localiser coûte un dump
// de 2 Mo, la réutiliser coûte trois petits POST.
static BEACON: std::sync::Mutex<Option<psxpipe::redux::Beacon>> = std::sync::Mutex::new(None);

/// À appeler quand la cible change (nouveau Play, reset, autre scène) :
/// la balise sera relocalisée au prochain sync.
#[tauri::command(async)]
fn redux_clear_beacon() {
    *BEACON.lock().unwrap() = None;
}

/// Écrit la transform d'une entité dans la RAM console pendant que le jeu
/// tourne. `index` est l'indice dans l'ordre du fichier .psc (entity_names).
/// Rotation en unités PS1 (4096 = tour), échelle en 4.12.
#[tauri::command(async)]
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
            import_asset_bytes,
            list_project_files,
            create_scene,
            create_folder,
            save_prefab,
            move_entry,
            read_library_file,
            get_model_subdiv,
            set_model_subdiv,
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
