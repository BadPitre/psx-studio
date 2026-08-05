//! psxpipe CLI — PSX Studio asset pipeline (Phase 1).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use psxpipe::{gltf_import, pmd, scene, tim};

#[derive(Parser)]
#[command(name = "psxpipe", version, about = "PSX Studio asset pipeline: glTF/PNG -> PS1 formats")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Convert a glTF/GLB model to the PMD console mesh format
    Gltf2pmd {
        /// Input .gltf or .glb file
        input: PathBuf,
        /// Output .pmd file (default: input with .pmd extension)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Target half-extent in PMD units (largest |coordinate| maps to this)
        #[arg(long, default_value_t = 128)]
        size: i32,
        /// Force flat shading (one normal per face)
        #[arg(long)]
        flat: bool,
        /// Ignore UVs and export untextured primitives
        #[arg(long)]
        untextured: bool,
        /// Texture width in pixels, used to map UVs (max 256)
        #[arg(long, default_value_t = 256)]
        tex_w: u32,
        /// Texture height in pixels, used to map UVs (max 256)
        #[arg(long, default_value_t = 256)]
        tex_h: u32,
    },
    /// Convert a PNG image to the TIM texture format
    Png2tim {
        /// Input .png file
        input: PathBuf,
        /// Output .tim file (default: input with .tim extension)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Bits per pixel: 4 (16 colors), 8 (256 colors) or 16 (direct)
        #[arg(long, default_value_t = 8)]
        bpp: u32,
        /// VRAM X of the pixel data (in 16-bit words)
        #[arg(long, default_value_t = 320)]
        org_x: u16,
        /// VRAM Y of the pixel data
        #[arg(long, default_value_t = 0)]
        org_y: u16,
        /// VRAM X of the CLUT
        #[arg(long, default_value_t = 320)]
        clut_x: u16,
        /// VRAM Y of the CLUT
        #[arg(long, default_value_t = 256)]
        clut_y: u16,
    },
    /// Build a packed .psc scene from an editable scene JSON
    Scene {
        /// Input .scene.json file (asset paths resolved relative to it)
        input: PathBuf,
        /// Output .psc file (default: input with .psc extension)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Keep the VRAM placements baked in the TIM files instead of
        /// packing textures automatically
        #[arg(long)]
        keep_vram: bool,
        /// Write a PNG map of the packed VRAM layout
        #[arg(long)]
        vram_map: Option<PathBuf>,
    },
    /// Build a whole project into a bootable .bin/.cue (via mkpsxiso)
    Build {
        /// Project directory containing project.json (default: current dir)
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Reconvert every asset, ignoring the Library/ cache
        #[arg(long)]
        force: bool,
    },
    /// Convert a WAV file to the VAG sound format (SPU-ADPCM)
    Wav2vag {
        /// Input .wav file (mono/stereo, 16-bit or float PCM)
        input: PathBuf,
        /// Output .vag file (default: input with .vag extension)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Print header information of a .pmd, .tim or .psc file
    Info {
        input: PathBuf,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.cmd {
        Cmd::Gltf2pmd {
            input,
            output,
            size,
            flat,
            untextured,
            tex_w,
            tex_h,
        } => {
            if !(1..=32767).contains(&size) {
                return Err("--size must be between 1 and 32767".into());
            }
            if tex_w > 256 || tex_h > 256 || tex_w == 0 || tex_h == 0 {
                return Err("--tex-w/--tex-h must be between 1 and 256 (one texture page)".into());
            }
            let opts = gltf_import::ImportOptions {
                target_size: size,
                flat,
                untextured,
                tex_w,
                tex_h,
            };
            let (pmd, report) = gltf_import::import(&input, &opts)?;
            let out = output.unwrap_or_else(|| input.with_extension("pmd"));
            let bytes = pmd.write()?;
            std::fs::write(&out, &bytes).map_err(|e| format!("cannot write {}: {e}", out.display()))?;

            println!("{} -> {} ({} bytes)", input.display(), out.display(), bytes.len());
            println!(
                "  meshes: {}   triangles: {} ({} degenerate dropped)",
                report.meshes,
                report.triangles_in - report.degenerate_dropped,
                report.degenerate_dropped
            );
            println!(
                "  vertices: {}   normals: {}   scale: {} (4.12)",
                report.vertex_count, report.normal_count, report.scale_4_12
            );
            println!(
                "  prims: F3={} G3={} FT3={} GT3={}",
                report.counts[0], report.counts[1], report.counts[2], report.counts[3]
            );
            for w in &report.warnings {
                println!("  warning: {w}");
            }
            Ok(())
        }
        Cmd::Png2tim {
            input,
            output,
            bpp,
            org_x,
            org_y,
            clut_x,
            clut_y,
        } => {
            let bpp = match bpp {
                4 => tim::Bpp::Four,
                8 => tim::Bpp::Eight,
                16 => tim::Bpp::Sixteen,
                other => return Err(format!("unsupported bpp {other} (use 4, 8 or 16)")),
            };
            let img = image::open(&input)
                .map_err(|e| format!("cannot open {}: {e}", input.display()))?
                .to_rgba8();
            let (w, h) = img.dimensions();
            if w > 256 || h > 256 {
                return Err(format!(
                    "image is {w}x{h}: textures are limited to 256x256 (one texture page)"
                ));
            }
            let opts = tim::TimOptions {
                bpp,
                org_x,
                org_y,
                clut_x,
                clut_y,
            };
            let (timg, report) = tim::encode(img.as_raw(), w, h, &opts)?;
            let out = output.unwrap_or_else(|| input.with_extension("tim"));
            let bytes = timg.write();
            std::fs::write(&out, &bytes).map_err(|e| format!("cannot write {}: {e}", out.display()))?;

            println!("{} -> {} ({} bytes)", input.display(), out.display(), bytes.len());
            println!(
                "  {}x{} px, {} source colors -> {} palette colors{}",
                w,
                h,
                report.source_colors,
                report.palette_colors,
                if report.has_transparency { ", transparency" } else { "" }
            );
            println!(
                "  VRAM: pixels at ({}, {}) {}x{} words{}",
                timg.pixels.x,
                timg.pixels.y,
                timg.pixels.w,
                timg.pixels.h,
                match &timg.clut {
                    Some(c) => format!(", CLUT at ({}, {})", c.x, c.y),
                    None => String::new(),
                }
            );
            for w in &report.warnings {
                println!("  warning: {w}");
            }
            Ok(())
        }
        Cmd::Scene {
            input,
            output,
            keep_vram,
            vram_map,
        } => {
            let options = scene::BuildOptions {
                pack_vram: !keep_vram,
            };
            let (bytes, report) = scene::build_file_with_options(&input, &options)?;
            let out = output.unwrap_or_else(|| input.with_extension("psc"));
            std::fs::write(&out, &bytes).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
            println!(
                "{} -> {} ({} bytes)",
                input.display(),
                out.display(),
                report.total_size
            );
            println!(
                "  scene '{}': {} entities, {} models, {} textures",
                report.name, report.entities, report.models, report.textures
            );
            for (req, p) in &report.vram {
                println!(
                    "  vram: '{}' -> ({}, {}) {}x{}{}",
                    req.label,
                    p.x,
                    p.y,
                    req.words,
                    req.height,
                    if req.clut_entries > 0 {
                        format!(", CLUT ({}, {})", p.clut_x, p.clut_y)
                    } else {
                        String::new()
                    }
                );
            }
            if let Some(map_path) = vram_map {
                let (reqs, places): (Vec<_>, Vec<_>) = report.vram.iter().cloned().unzip();
                psxpipe::vram::render_map(&reqs, &places)
                    .save(&map_path)
                    .map_err(|e| format!("cannot write {}: {e}", map_path.display()))?;
                println!("  vram map -> {}", map_path.display());
            }
            for w in &report.warnings {
                println!("  warning: {w}");
            }
            Ok(())
        }
        Cmd::Build { project, force } => {
            let report = psxpipe::project::build(&project, force)?;
            println!(
                "assets: {} converted, {} cached",
                report.converted, report.cached
            );
            for (i, s) in report.scenes.iter().enumerate() {
                println!(
                    "scene {i} '{}': {} entities, {} bytes (VRAM map: Build/vram-scene{i}.png)",
                    s.name, s.entities, s.total_size
                );
            }
            if report.mkpsxiso_ran {
                println!("ISO built in Build/ — open the .cue in an emulator");
            }
            for w in &report.warnings {
                println!("warning: {w}");
            }
            Ok(())
        }
        Cmd::Wav2vag { input, output } => {
            let name = input
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let (vag, report) = psxpipe::vag::wav_to_vag(&input, &name)?;
            let out = output.unwrap_or_else(|| input.with_extension("vag"));
            std::fs::write(&out, &vag).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
            println!("{} -> {} ({} bytes)", input.display(), out.display(), vag.len());
            println!(
                "  {} samples at {} Hz -> {} bytes of SPU RAM",
                report.input_samples, report.sample_rate, report.spu_bytes
            );
            for w in &report.warnings {
                println!("  warning: {w}");
            }
            Ok(())
        }
        Cmd::Info { input } => {
            let data =
                std::fs::read(&input).map_err(|e| format!("cannot read {}: {e}", input.display()))?;
            if data.starts_with(scene::MAGIC) {
                let h = scene::parse_header(&data)?;
                println!("PSC v{} ({} bytes)", h.version, data.len());
                println!(
                    "  models: {}   textures: {}   entities: {}",
                    h.model_count, h.texture_count, h.entity_count
                );
                println!(
                    "  bg: {:?}   ambient: {:?}   light color: {:?}   toward light: {:?}",
                    h.background, h.ambient, h.light_color, h.light_toward
                );
            } else if data.starts_with(pmd::MAGIC) {
                let h = pmd::parse_header(&data)?;
                println!("PMD v{} ({} bytes)", h.version, data.len());
                println!(
                    "  textured: {}   scale: {} (4.12)",
                    h.flags & pmd::FLAG_TEXTURED != 0,
                    h.scale_4_12
                );
                println!("  vertices: {}   normals: {}", h.vertex_count, h.normal_count);
                println!(
                    "  prims: F3={} G3={} FT3={} GT3={}",
                    h.prim_counts[0], h.prim_counts[1], h.prim_counts[2], h.prim_counts[3]
                );
            } else if data.len() >= 8 && data[0] == 0x10 && data[1..4] == [0, 0, 0] {
                let flags = u32::from_le_bytes(data[4..8].try_into().unwrap());
                let bpp = match flags & 3 {
                    0 => "4bpp",
                    1 => "8bpp",
                    2 => "16bpp",
                    _ => "24bpp",
                };
                println!("TIM {} ({} bytes), CLUT: {}", bpp, data.len(), flags & 8 != 0);
            } else {
                return Err("unknown file format (expected PMD or TIM)".into());
            }
            Ok(())
        }
    }
}
