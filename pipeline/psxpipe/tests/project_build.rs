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
            { "gltf": "assets/house.gltf",  "out": "house.pmd" }
          ],
          "textures": [
            { "png": "assets/checker.png", "out": "checker.tim", "bpp": 8 },
            { "png": "assets/house.png",   "out": "house.tim",   "bpp": 8 }
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
    // 3 models + 2 textures + 1 sfx converted on the first run.
    assert_eq!(report.converted, 6);
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
    assert_eq!(report2.cached, 6);

    // Changing a source (valid but different image) invalidates only it.
    let png = dir.path().join("assets/checker.png");
    let img: image::RgbaImage =
        image::ImageBuffer::from_raw(256, 256, samples::house_rgba(256)).unwrap();
    img.save(&png).unwrap();
    let report3 = project::build(dir.path(), false).unwrap();
    assert_eq!(report3.converted, 1);
    assert_eq!(report3.cached, 5);
}

#[test]
fn missing_exe_is_a_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    setup_project(dir.path());
    std::fs::remove_file(dir.path().join("PLAYER.EXE")).unwrap();
    let err = project::build(dir.path(), false).unwrap_err();
    assert!(err.contains("build the runtime"), "unexpected: {err}");
}
