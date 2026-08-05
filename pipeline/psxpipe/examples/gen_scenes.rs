//! Build the demo scenes (Phase 2): generates the sample assets, converts
//! them to PMD/TIM, and packs scene0.psc + scene1.psc.
//!
//! Usage: cargo run --example gen_scenes -- [samples-dir] [psc-out-dir]

fn main() {
    let mut args = std::env::args().skip(1);
    let samples = std::path::PathBuf::from(args.next().unwrap_or_else(|| "samples".into()));
    let out = std::path::PathBuf::from(
        args.next()
            .unwrap_or_else(|| "../runtime/player/assets".into()),
    );
    let reports = psxpipe::samples::build_demo_scenes(&samples, &out).expect("scene build failed");
    for r in &reports {
        println!(
            "scene '{}': {} entities, {} models, {} textures, {} bytes",
            r.name, r.entities, r.models, r.textures, r.total_size
        );
        for w in &r.warnings {
            println!("  warning: {w}");
        }
    }
    println!("scenes written to {}", out.display());
}
