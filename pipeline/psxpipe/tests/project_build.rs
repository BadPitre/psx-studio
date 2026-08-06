//! End-to-end test of `psxpipe build` on a synthetic demo project.

use psxpipe::{project, samples};

fn setup_project(dir: &std::path::Path) {
    samples::write_all(&dir.join("assets")).unwrap();
    samples::write_audio(&dir.join("audio")).unwrap();
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::write(dir.join("scenes/scene0.json"), samples::scene_village_json()).unwrap();
    std::fs::write(dir.join("scenes/scene1.json"), samples::scene_field_json()).unwrap();
    // Stand-in PS-EXE (the real one comes from the CMake build).
    std::fs::write(dir.join("PLAYER.EXE"), b"PS-X EXE dummy").unwrap();
    std::fs::write(
        dir.join("project.json"),
        r#"{
          "name": "demo",
          "exe": "PLAYER.EXE",
          "models": [
            { "gltf": "assets/cube.gltf",   "out": "cube.pmd" },
            { "gltf": "assets/ground.gltf", "out": "ground.pmd" },
            { "gltf": "assets/house.gltf",  "out": "house.pmd" },
            { "gltf": "assets/guy.gltf",    "out": "guy.pmd", "tex_w": 64, "tex_h": 64 }
          ],
          "textures": [
            { "png": "assets/checker.png", "out": "checker.tim" },
            { "png": "assets/house.png",   "out": "house.tim" },
            { "png": "assets/guy.png",     "out": "guy.tim" }
          ],
          "fonts": [{ "png": "assets/font.png", "out": "main.fnt" }],
          "scenes": ["scenes/scene0.json", "scenes/scene1.json"],
          "sfx": [{ "wav": "audio/sfx.wav", "out": "BLIP.VAG" }],
          "music": ["audio/music.wav"]
        }"#,
    )
    .unwrap();
}

#[test]
fn full_project_build_and_cache() {
    let dir = tempfile::tempdir().unwrap();
    setup_project(dir.path());

    let report = project::build(dir.path(), false).unwrap();
    // 4 models + 3 textures + 1 police + 1 sfx converted on the first run.
    assert_eq!(report.converted, 9);
    assert_eq!(report.cached, 0);
    assert_eq!(report.scenes.len(), 2);

    // Outputs exist.
    for name in ["SCENE0.PSC", "SCENE1.PSC", "BLIP.VAG", "SYSTEM.CNF", "iso.xml", "vram-scene0.png"] {
        assert!(
            dir.path().join("Build").join(name).exists(),
            "missing Build/{name}"
        );
    }
    // Library holds the converted assets + manifest.
    assert!(dir.path().join("Library/house.pmd").exists());
    assert!(dir.path().join("Library/manifest.json").exists());

    // iso.xml references the audio track and the scenes.
    let xml = std::fs::read_to_string(dir.path().join("Build/iso.xml")).unwrap();
    assert!(xml.contains("SCENE1.PSC"));
    assert!(xml.contains("type=\"audio\""));
    assert!(xml.contains("BLIP.VAG"));

    // Second run: everything cached.
    let report2 = project::build(dir.path(), false).unwrap();
    assert_eq!(report2.converted, 0);
    assert_eq!(report2.cached, 9);

    // Changing a source (valid but different image) invalidates only it.
    let png = dir.path().join("assets/checker.png");
    let img: image::RgbaImage =
        image::ImageBuffer::from_raw(256, 256, samples::house_rgba(256)).unwrap();
    img.save(&png).unwrap();
    let report3 = project::build(dir.path(), false).unwrap();
    assert_eq!(report3.converted, 1);
    assert_eq!(report3.cached, 8);
}

#[test]
fn set_model_subdiv_updates_json_and_reconverts() {
    let dir = tempfile::tempdir().unwrap();
    setup_project(dir.path());
    project::build(dir.path(), false).unwrap();
    let plain_len = std::fs::metadata(dir.path().join("Library/house.pmd")).unwrap().len();

    // Activer : le JSON gagne le champ, le .pmd grossit (plus de tris).
    let summary = project::set_model_subdiv(dir.path(), "house.pmd", Some(48.0)).unwrap();
    assert!(summary.contains("par subdivision"), "{summary}");
    assert_eq!(project::model_subdiv(dir.path(), "house.pmd").unwrap(), Some(48.0));
    let text = std::fs::read_to_string(dir.path().join("project.json")).unwrap();
    assert!(text.contains("\"subdiv\": 48"), "{text}");
    let sub_len = std::fs::metadata(dir.path().join("Library/house.pmd")).unwrap().len();
    assert!(sub_len > plain_len);

    // Le cache est à jour : le build suivant ne reconvertit rien.
    let report = project::build(dir.path(), false).unwrap();
    assert_eq!(report.converted, 0);

    // Désactiver : champ retiré, retour au modèle d'origine.
    project::set_model_subdiv(dir.path(), "house.pmd", None).unwrap();
    assert_eq!(project::model_subdiv(dir.path(), "house.pmd").unwrap(), None);
    let back = std::fs::metadata(dir.path().join("Library/house.pmd")).unwrap().len();
    assert_eq!(back, plain_len);

    // Modèle inconnu : erreur claire.
    assert!(project::set_model_subdiv(dir.path(), "nope.pmd", Some(32.0)).is_err());
}

#[test]
fn missing_exe_is_a_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    setup_project(dir.path());
    std::fs::remove_file(dir.path().join("PLAYER.EXE")).unwrap();
    let err = project::build(dir.path(), false).unwrap_err();
    assert!(err.contains("build the runtime"), "unexpected: {err}");
}

#[test]
fn import_asset_registers_and_converts() {
    let dir = tempfile::tempdir().unwrap();
    setup_project(dir.path());

    // Nouveau modèle + texture déposés depuis un dossier externe.
    let ext = tempfile::tempdir().unwrap();
    samples::write_all(ext.path()).unwrap();
    std::fs::rename(ext.path().join("house.gltf"), ext.path().join("Tour Eiffel.gltf")).unwrap();
    std::fs::rename(ext.path().join("house.bin"), ext.path().join("Tour Eiffel.bin")).unwrap();

    let imported = project::import_asset(dir.path(), &ext.path().join("Tour Eiffel.gltf")).unwrap();
    assert_eq!(imported.id, "tour_eiffel");
    assert_eq!(imported.out, "tour_eiffel.pmd");
    assert!(dir.path().join("Library/tour_eiffel.pmd").exists());
    assert!(dir.path().join("assets/tour_eiffel.gltf").exists());
    // Le buffer garde son nom d'origine (référencé tel quel par le glTF).
    assert!(dir.path().join("assets/Tour Eiffel.bin").exists());

    let tex = project::import_asset(dir.path(), &ext.path().join("checker.png"));
    assert!(tex.is_ok());

    // project.json mis à jour et toujours buildable (idempotent).
    let text = std::fs::read_to_string(dir.path().join("project.json")).unwrap();
    assert!(text.contains("tour_eiffel.pmd"));
    project::import_asset(dir.path(), &ext.path().join("Tour Eiffel.gltf")).unwrap();
    let text2 = std::fs::read_to_string(dir.path().join("project.json")).unwrap();
    assert_eq!(
        text2.matches("tour_eiffel.pmd").count(),
        text.matches("tour_eiffel.pmd").count()
    );
    project::build(dir.path(), false).unwrap();
}

#[test]
fn list_files_crosses_disk_and_project_json() {
    let dir = tempfile::tempdir().unwrap();
    setup_project(dir.path());

    // Fichier présent mais pas dans project.json -> « non importé ».
    std::fs::copy(
        dir.path().join("assets/checker.png"),
        dir.path().join("assets/libre.png"),
    )
    .unwrap();
    // Entrée de project.json dont le fichier a disparu -> « manquant ».
    std::fs::remove_file(dir.path().join("assets/guy.png")).unwrap();

    let files = project::list_files(dir.path()).unwrap();
    let find = |p: &str| files.iter().find(|f| f.path == p).unwrap();

    let scene = find("scenes/scene0.json");
    assert_eq!((scene.kind.as_str(), scene.registered, scene.exists), ("scene", true, true));
    let checker = find("assets/checker.png");
    assert_eq!((checker.kind.as_str(), checker.registered), ("texture", true));
    assert!(checker.size > 0);
    let libre = find("assets/libre.png");
    assert!(!libre.registered && libre.exists);
    let guy = find("assets/guy.png");
    assert!(guy.registered && !guy.exists);
    // Le .bin compagnon d'un .gltf ne porte pas de badge.
    let bin = find("assets/house.bin");
    assert_eq!((bin.kind.as_str(), bin.registered), ("buffer", true));
    let wav = find("audio/sfx.wav");
    assert_eq!((wav.kind.as_str(), wav.registered), ("audio", true));
}

#[test]
fn folders_move_and_import_in_place() {
    let dir = tempfile::tempdir().unwrap();
    setup_project(dir.path());

    // Créer un dossier : visible dans la liste, même vide.
    let folder = project::create_folder(dir.path(), "assets", "persos").unwrap();
    assert_eq!(folder, "assets/persos");
    let files = project::list_files(dir.path()).unwrap();
    let node = files.iter().find(|f| f.path == "assets/persos").unwrap();
    assert_eq!(node.kind, "dir");
    // Noms hostiles nettoyés, remontées hors projet refusées.
    assert_eq!(project::create_folder(dir.path(), "assets", "mes persos !").unwrap(), "assets/mes_persos__");
    assert!(project::create_folder(dir.path(), "../ailleurs", "x").is_err());
    assert!(project::move_entry(dir.path(), "../project.json", "assets").is_err());

    // Déplacer un .gltf enregistré : le .bin suit, project.json est réécrit.
    let moved = project::move_entry(dir.path(), "assets/house.gltf", "assets/persos").unwrap();
    assert_eq!(moved, "assets/persos/house.gltf");
    assert!(dir.path().join("assets/persos/house.bin").exists());
    assert!(!dir.path().join("assets/house.gltf").exists());
    let text = std::fs::read_to_string(dir.path().join("project.json")).unwrap();
    assert!(text.contains("assets/persos/house.gltf"), "{text}");
    assert!(!text.contains("\"assets/house.gltf\""), "{text}");

    // La liste suit le rangement et le projet se builde toujours.
    let files = project::list_files(dir.path()).unwrap();
    let gltf = files.iter().find(|f| f.path == "assets/persos/house.gltf").unwrap();
    assert!(gltf.registered && gltf.exists);
    project::build(dir.path(), false).unwrap();

    // Import « sur place » : un fichier déposé dans un sous-dossier du
    // projet est enregistré là où il est, pas recopié à la racine.
    std::fs::copy(
        dir.path().join("assets/checker.png"),
        dir.path().join("assets/persos/peau.png"),
    )
    .unwrap();
    project::import_asset(dir.path(), &dir.path().join("assets/persos/peau.png")).unwrap();
    let text = std::fs::read_to_string(dir.path().join("project.json")).unwrap();
    assert!(text.contains("assets/persos/peau.png"), "{text}");
    assert!(!dir.path().join("assets/peau.png").exists());
    project::build(dir.path(), false).unwrap();
}

#[test]
fn create_scene_registers_and_stays_buildable() {
    let dir = tempfile::tempdir().unwrap();
    setup_project(dir.path());

    let rel = project::create_scene(dir.path(), "Niveau 2 !").unwrap();
    assert_eq!(rel, "scenes/niveau_2__.json");
    assert!(dir.path().join(&rel).exists());
    let text = std::fs::read_to_string(dir.path().join("project.json")).unwrap();
    assert!(text.contains(&rel), "{text}");

    // La scène vide se builde avec le projet (3 scènes désormais).
    let report = project::build(dir.path(), false).unwrap();
    assert_eq!(report.scenes.len(), 3);

    // Doublon et nom vide : erreurs claires.
    assert!(project::create_scene(dir.path(), "Niveau 2 !").is_err());
    assert!(project::create_scene(dir.path(), "   ").is_err());
}

#[test]
fn import_gltf_with_texture_extracts_and_pairs() {
    let dir = tempfile::tempdir().unwrap();
    setup_project(dir.path());

    // Un glTF "façon Blender" avec texture baseColor référencée.
    let ext = tempfile::tempdir().unwrap();
    samples::write_all(ext.path()).unwrap();
    let house = samples::house_mesh();
    std::fs::write(
        ext.path().join("batiment.gltf"),
        house.to_gltf_textured("Batiment", "batiment.bin", "house.png"),
    )
    .unwrap();
    std::fs::write(ext.path().join("batiment.bin"), house.to_bin()).unwrap();

    let imported = project::import_asset(dir.path(), &ext.path().join("batiment.gltf")).unwrap();
    assert_eq!(imported.texture_out.as_deref(), Some("batiment.tim"));
    assert!(imported.summary.contains("texture extraite 256x256"), "{}", imported.summary);
    assert!(dir.path().join("Library/batiment.tim").exists());
    assert!(dir.path().join("Library/batiment.pmd").exists());
    assert!(dir.path().join("assets/batiment.png").exists());

    // project.json référence les deux, et le projet rebuilde.
    let text = std::fs::read_to_string(dir.path().join("project.json")).unwrap();
    assert!(text.contains("batiment.tim"));
    assert!(text.contains("batiment.pmd"));
    project::build(dir.path(), false).unwrap();
}
