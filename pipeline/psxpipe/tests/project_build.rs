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
            { "png": "assets/checker.png", "out": "checker.tim", "bpp": 8 },
            { "png": "assets/house.png",   "out": "house.tim",   "bpp": 8 },
            { "png": "assets/guy.png",     "out": "guy.tim",     "bpp": 8 }
          ],
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
    // 4 models + 3 textures + 1 sfx converted on the first run.
    assert_eq!(report.converted, 8);
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
    assert_eq!(report2.cached, 8);

    // Changing a source (valid but different image) invalidates only it.
    let png = dir.path().join("assets/checker.png");
    let img: image::RgbaImage =
        image::ImageBuffer::from_raw(256, 256, samples::house_rgba(256)).unwrap();
    img.save(&png).unwrap();
    let report3 = project::build(dir.path(), false).unwrap();
    assert_eq!(report3.converted, 1);
    assert_eq!(report3.cached, 7);
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
