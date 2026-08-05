//! Generate the sample source assets (cube.gltf/cube.bin + checker.png).
//!
//! Usage: cargo run --example gen_samples -- <output-dir>

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "samples".to_string());
    let path = std::path::PathBuf::from(dir);
    psxpipe::samples::write_all(&path).expect("failed to write samples");
    println!("samples written to {}", path.display());
}
