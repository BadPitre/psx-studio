//! TIM texture writer (standard PS1 format) with palette quantization.

use crate::quant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bpp {
    Four,
    Eight,
    Sixteen,
}

impl Bpp {
    pub fn mode_bits(self) -> u32 {
        match self {
            Bpp::Four => 0,
            Bpp::Eight => 1,
            Bpp::Sixteen => 2,
        }
    }

    pub fn pixels_per_word(self) -> u32 {
        match self {
            Bpp::Four => 4,
            Bpp::Eight => 2,
            Bpp::Sixteen => 1,
        }
    }
}

/// A VRAM block (CLUT or pixel data). `w` is in 16-bit VRAM words.
#[derive(Debug, Clone)]
pub struct Block {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub data: Vec<u16>,
}

#[derive(Debug, Clone)]
pub struct TimImage {
    pub bpp: Bpp,
    pub clut: Option<Block>,
    pub pixels: Block,
}

#[derive(Debug, Default)]
pub struct TimReport {
    pub source_colors: usize,
    pub palette_colors: usize,
    pub has_transparency: bool,
    pub warnings: Vec<String>,
}

pub struct TimOptions {
    pub bpp: Bpp,
    /// VRAM position of the pixel data (in VRAM words for x).
    pub org_x: u16,
    pub org_y: u16,
    /// VRAM position of the CLUT (ignored for 16 bpp).
    pub clut_x: u16,
    pub clut_y: u16,
}

impl Default for TimOptions {
    fn default() -> Self {
        // Default placement: right of the two stacked 320x240 framebuffers.
        TimOptions {
            bpp: Bpp::Eight,
            org_x: 320,
            org_y: 0,
            clut_x: 320,
            clut_y: 256,
        }
    }
}

/// Encode an RGBA8 image into a TIM.
/// Pixels with alpha < 128 become fully transparent (VRAM value 0x0000).
/// Choix automatique de la profondeur : 4 bpp si l'image tient en 16
/// couleurs (transparence comprise — l'index 0 lui est réservé), sinon
/// 8 bpp. Moitié de VRAM gagnée sans aucune perte quand ça tient.
pub fn auto_bpp(rgba: &[u8]) -> Bpp {
    let mut colors: Vec<[u8; 3]> = Vec::new();
    let mut has_transparency = false;
    for p in rgba.chunks_exact(4) {
        if p[3] < 128 {
            has_transparency = true;
        } else {
            colors.push([p[0], p[1], p[2]]);
        }
        if colors.len() > 4096 {
            break; // largement au-dessus de 16, inutile de tout lire
        }
    }
    colors.sort_unstable();
    colors.dedup();
    if colors.len() + has_transparency as usize <= 16 {
        Bpp::Four
    } else {
        Bpp::Eight
    }
}

pub fn encode(
    rgba: &[u8],
    width: u32,
    height: u32,
    opts: &TimOptions,
) -> Result<(TimImage, TimReport), String> {
    if rgba.len() != (width * height * 4) as usize {
        return Err("RGBA buffer size does not match dimensions".into());
    }
    let mut report = TimReport::default();

    let px_count = (width * height) as usize;
    let mut opaque: Vec<[u8; 3]> = Vec::with_capacity(px_count);
    let mut transparent: Vec<bool> = Vec::with_capacity(px_count);
    for i in 0..px_count {
        let p = &rgba[i * 4..i * 4 + 4];
        let t = p[3] < 128;
        transparent.push(t);
        if t {
            report.has_transparency = true;
        } else {
            opaque.push([p[0], p[1], p[2]]);
        }
    }
    {
        let mut u = opaque.clone();
        u.sort_unstable();
        u.dedup();
        report.source_colors = u.len();
    }

    let (pixels, clut) = match opts.bpp {
        Bpp::Sixteen => {
            let mut data = Vec::with_capacity(px_count);
            for i in 0..px_count {
                let p = &rgba[i * 4..i * 4 + 4];
                data.push(if transparent[i] {
                    0x0000
                } else {
                    quant::opaque_to_ps1(p[0], p[1], p[2])
                });
            }
            let pixels = Block {
                x: opts.org_x,
                y: opts.org_y,
                w: width as u16,
                h: height as u16,
                data,
            };
            (pixels, None)
        }
        bpp => {
            let entries = if bpp == Bpp::Four { 16 } else { 256 };
            // Reserve index 0 for the transparent color when needed.
            let offset = if report.has_transparency { 1 } else { 0 };
            let palette = quant::median_cut(&opaque, entries - offset);
            report.palette_colors = palette.len();

            // Index per pixel (transparent pixels use the reserved index 0).
            let mut indices: Vec<u8> = Vec::with_capacity(px_count);
            for i in 0..px_count {
                if transparent[i] {
                    indices.push(0);
                } else {
                    let p = &rgba[i * 4..i * 4 + 4];
                    let n = quant::nearest(&palette, [p[0], p[1], p[2]]);
                    indices.push((n + offset) as u8);
                }
            }

            // Pack indices into VRAM words, low bits first.
            let ppw = bpp.pixels_per_word();
            if !width.is_multiple_of(ppw) {
                report.warnings.push(format!(
                    "width {width} is not a multiple of {ppw} pixels; rows padded with index 0"
                ));
            }
            let words_per_row = width.div_ceil(ppw);
            let mut data = Vec::with_capacity((words_per_row * height) as usize);
            for y in 0..height {
                for wx in 0..words_per_row {
                    let mut word: u16 = 0;
                    for k in 0..ppw {
                        let x = wx * ppw + k;
                        let idx = if x < width {
                            indices[(y * width + x) as usize] as u16
                        } else {
                            0
                        };
                        let bits = 16 / ppw;
                        word |= idx << (k * bits);
                    }
                    data.push(word);
                }
            }
            let pixels = Block {
                x: opts.org_x,
                y: opts.org_y,
                w: words_per_row as u16,
                h: height as u16,
                data,
            };

            let mut clut_data = vec![0u16; entries];
            for (i, c) in palette.iter().enumerate() {
                clut_data[i + offset] = quant::opaque_to_ps1(c[0], c[1], c[2]);
            }
            let clut = Block {
                x: opts.clut_x,
                y: opts.clut_y,
                w: entries as u16,
                h: 1,
                data: clut_data,
            };
            (pixels, Some(clut))
        }
    };

    // Warn about VRAM collisions with the default 320x240 double framebuffer.
    for (name, b) in [("pixel data", Some(&pixels)), ("CLUT", clut.as_ref())]
        .iter()
        .filter_map(|(n, b)| b.map(|b| (*n, b)))
    {
        if b.x < 320 && b.y < 480 {
            report.warnings.push(format!(
                "{name} at ({}, {}) overlaps the default framebuffer area (x < 320, y < 480)",
                b.x, b.y
            ));
        }
    }

    Ok((
        TimImage {
            bpp: opts.bpp,
            clut,
            pixels,
        },
        report,
    ))
}

impl TimImage {
    /// Serialize to the on-disk / on-CD TIM format (little-endian).
    pub fn write(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&0x0000_0010u32.to_le_bytes()); // magic
        let flags = self.bpp.mode_bits() | if self.clut.is_some() { 8 } else { 0 };
        out.extend_from_slice(&flags.to_le_bytes());

        let mut write_block = |b: &Block| {
            let len = 12u32 + (b.data.len() as u32) * 2;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&b.x.to_le_bytes());
            out.extend_from_slice(&b.y.to_le_bytes());
            out.extend_from_slice(&b.w.to_le_bytes());
            out.extend_from_slice(&b.h.to_le_bytes());
            for w in &b.data {
                out.extend_from_slice(&w.to_le_bytes());
            }
        };
        if let Some(clut) = &self.clut {
            write_block(clut);
        }
        write_block(&self.pixels);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker_rgba(w: u32, h: u32) -> Vec<u8> {
        let mut v = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let on = (x / 2 + y / 2) % 2 == 0;
                v.extend_from_slice(if on {
                    &[255, 255, 255, 255]
                } else {
                    &[255, 0, 0, 255]
                });
            }
        }
        v
    }

    #[test]
    fn tim_16bpp_direct() {
        let rgba = checker_rgba(4, 4);
        let opts = TimOptions {
            bpp: Bpp::Sixteen,
            ..Default::default()
        };
        let (tim, report) = encode(&rgba, 4, 4, &opts).unwrap();
        assert!(tim.clut.is_none());
        assert_eq!(tim.pixels.w, 4);
        assert_eq!(tim.pixels.data[0], 0x7FFF);
        assert_eq!(tim.pixels.data[2], 0x001F);
        assert!(!report.has_transparency);

        let bytes = tim.write();
        assert_eq!(&bytes[0..4], &[0x10, 0, 0, 0]);
        assert_eq!(bytes[4], 2); // 16bpp, no CLUT
        // Pixel block: 12-byte header + 16 words.
        assert_eq!(bytes.len(), 8 + 12 + 32);
    }

    #[test]
    fn tim_8bpp_with_clut() {
        let rgba = checker_rgba(8, 8);
        let opts = TimOptions::default();
        let (tim, report) = encode(&rgba, 8, 8, &opts).unwrap();
        let clut = tim.clut.as_ref().unwrap();
        assert_eq!(clut.w, 256);
        assert_eq!(report.palette_colors, 2);
        // 8bpp: two pixels per word.
        assert_eq!(tim.pixels.w, 4);
        let bytes = tim.write();
        assert_eq!(bytes[4], 1 | 8);
    }

    #[test]
    fn tim_transparency_reserves_index0() {
        let mut rgba = checker_rgba(4, 4);
        rgba[3] = 0; // first pixel transparent
        let opts = TimOptions {
            bpp: Bpp::Four,
            ..Default::default()
        };
        let (tim, report) = encode(&rgba, 4, 4, &opts).unwrap();
        assert!(report.has_transparency);
        let clut = tim.clut.unwrap();
        assert_eq!(clut.data[0], 0x0000);
        // First pixel (low nibble of first word) must be index 0.
        assert_eq!(tim.pixels.data[0] & 0xF, 0);
    }
}
