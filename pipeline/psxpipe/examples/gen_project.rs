//! Generate the source assets of the demo project (examples/demo):
//! glTF models + PNG textures into assets/, WAV audio into audio/,
//! and the demo scene JSONs into scenes/ (overwritten: they must stay in
//! sync with the sample assets — e.g. the guy model + scripts of Phase 5).
//! The project then builds with: psxpipe build ../examples/demo
//!
//! Usage: cargo run --example gen_project -- [project-dir]

fn main() {
    let dir = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "../examples/demo".into()),
    );
    psxpipe::samples::write_all_organized(&dir.join("assets"))
        .expect("failed to write assets");
    psxpipe::samples::write_audio(&dir.join("audio")).expect("failed to write audio");
    let scripts = dir.join("scripts");
    std::fs::create_dir_all(&scripts).expect("failed to create scripts/");
    std::fs::write(
        scripts.join("spinner.psxs"),
        psxpipe::samples::demo_spinner_psxs(),
    )
    .expect("failed to write spinner.psxs");
    let scenes = dir.join("scenes");
    std::fs::create_dir_all(&scenes).expect("failed to create scenes/");
    std::fs::write(scenes.join("scene0.json"), psxpipe::samples::scene_village_json())
        .expect("failed to write scene0.json");
    std::fs::write(scenes.join("scene1.json"), psxpipe::samples::scene_field_json())
        .expect("failed to write scene1.json");
    std::fs::write(dir.join("project.json"), psxpipe::samples::demo_project_json())
        .expect("failed to write project.json");
    println!("demo project sources written to {}", dir.display());
}
