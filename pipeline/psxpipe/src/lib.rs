//! psxpipe — PSX Studio asset pipeline.
//!
//! Converts source assets (glTF, PNG) into PS1 console formats:
//! - PMD: custom mesh format with pre-encoded GPU packets (see docs/PMD-FORMAT.md)
//! - TIM: standard PS1 texture format (CLUT + pixel data)
//!
//! MIT License — Copyright (c) 2026 Bertrand

pub mod gltf_import;
pub mod pmd;
pub mod project;
pub mod quant;
pub mod samples;
pub mod scene;
pub mod tim;
pub mod vag;
pub mod vram;

/// 4.12 fixed-point one (GTE convention: 4096 = 1.0).
pub const ONE_4_12: i32 = 4096;
