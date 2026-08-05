//! psxpipe CLI — PSX Studio asset pipeline (Phase 1).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use psxpipe::{gltf_import, pmd, tim};

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
    /// Print header information of a .pmd or .tim file
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
        Cmd::Info { input } => {
            let data =
                std::fs::read(&input).map_err(|e| format!("cannot read {}: {e}", input.display()))?;
            if data.starts_with(pmd::MAGIC) {
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
