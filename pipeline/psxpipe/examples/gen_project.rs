//! Generate the source assets of the demo project (examples/demo):
//! glTF models + PNG textures into assets/, WAV audio into audio/.
//! The project then builds with: psxpipe build ../examples/demo
//!
//! Usage: cargo run --example gen_project -- [project-dir]

fn main() {
    let dir = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "../examples/demo".into()),
    );
    psxpipe::samples::write_all(&dir.join("assets")).expect("failed to write assets");
    psxpipe::samples::write_audio(&dir.join("audio")).expect("failed to write audio");
    println!("demo project sources written to {}", dir.display());
}
