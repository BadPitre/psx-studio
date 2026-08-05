//! Automatic VRAM packing for scene textures.
//!
//! VRAM is 1024x512 16-bit words. The double framebuffer occupies
//! (0,0)-(320,480) and the debug font (960,0)-(1024,64). Textures are
//! placed at texture-page-aligned origins (x multiple of 64 words, y = 0
//! or 256) so the UVs baked by gltf2pmd need no offset; CLUTs go to a
//! reserved strip at the bottom-right. Packing failures are hard errors —
//! the report tells the user what didn't fit.

pub const VRAM_W: u16 = 1024;
pub const VRAM_H: u16 = 512;

/// Vertical bands where textures may live (page-aligned y).
const BAND_Y: [u16; 2] = [0, 256];
/// CLUT strip: bottom rows, safe from band 2 textures (height cap below).
const CLUT_Y0: u16 = 504;
/// Band 2 textures must not reach into the CLUT strip.
const BAND2_MAX_H: u16 = CLUT_Y0 - 256;

#[derive(Debug, Clone)]
pub struct TexRequest {
    pub label: String,
    /// Pixel-data width in VRAM words.
    pub words: u16,
    pub height: u16,
    /// CLUT entries (0 = no CLUT).
    pub clut_entries: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub x: u16,
    pub y: u16,
    pub clut_x: u16,
    pub clut_y: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct Reserved {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

/// Fixed reservations: double 320x240 framebuffer and the debug font.
pub fn default_reserved() -> Vec<(String, Reserved)> {
    vec![
        (
            "framebuffers".into(),
            Reserved { x: 0, y: 0, w: 320, h: 480 },
        ),
        (
            "debug font".into(),
            Reserved { x: 960, y: 0, w: 64, h: 64 },
        ),
    ]
}

fn collides(x: u16, y: u16, w: u16, h: u16, r: &Reserved) -> bool {
    x < r.x + r.w && r.x < x + w && y < r.y + r.h && r.y < y + h
}

/// Pack textures and CLUTs. Deterministic: requests are placed in the
/// given order, textures left-to-right per band, CLUTs left-to-right in
/// the bottom strip.
pub fn pack(requests: &[TexRequest]) -> Result<Vec<Placement>, String> {
    let reserved = default_reserved();
    // Next free page-aligned x per band.
    let mut band_x = [0u16; 2];
    let mut clut_cursor = (0u16, CLUT_Y0);
    let mut out = Vec::with_capacity(requests.len());

    for req in requests {
        if req.height > 256 {
            return Err(format!(
                "texture '{}' is {} rows tall (max 256)",
                req.label, req.height
            ));
        }
        if req.words > 256 {
            return Err(format!(
                "texture '{}' is {} words wide (max 256 = one 16bpp page)",
                req.label, req.words
            ));
        }

        /* Texture: first band that fits. */
        let span = req.words.div_ceil(64) * 64;
        let mut place: Option<(u16, u16)> = None;
        for (band, &y) in BAND_Y.iter().enumerate() {
            if band == 1 && req.height > BAND2_MAX_H {
                continue;
            }
            // Skip past reserved areas.
            let mut x = band_x[band].max(if y < 480 { 320 } else { 0 });
            loop {
                if x + span > VRAM_W {
                    break;
                }
                if let Some((_, r)) = reserved
                    .iter()
                    .find(|(_, r)| collides(x, y, span, req.height, r))
                {
                    x = ((r.x + r.w).div_ceil(64)) * 64;
                    continue;
                }
                place = Some((x, y));
                band_x[band] = x + span;
                break;
            }
            if place.is_some() {
                break;
            }
        }
        let Some((x, y)) = place else {
            return Err(format!(
                "VRAM full: no page-aligned slot for texture '{}' ({} words x {})",
                req.label, req.words, req.height
            ));
        };

        /* CLUT: bottom strip, x multiple of 16. */
        let (mut cx, mut cy) = (0, 0);
        if req.clut_entries > 0 {
            let (mut px, mut py) = clut_cursor;
            if px + req.clut_entries > VRAM_W {
                px = 0;
                py += 1;
            }
            if py >= VRAM_H {
                return Err(format!(
                    "VRAM full: no CLUT slot left for texture '{}'",
                    req.label
                ));
            }
            cx = px;
            cy = py;
            let next = px + req.clut_entries.div_ceil(16) * 16;
            clut_cursor = if next >= VRAM_W { (0, py + 1) } else { (next, py) };
        }

        out.push(Placement {
            x,
            y,
            clut_x: cx,
            clut_y: cy,
        });
    }
    Ok(out)
}

/// Render the VRAM layout to an RGBA image (1024x512), for the map export.
pub fn render_map(
    requests: &[TexRequest],
    placements: &[Placement],
) -> image::RgbaImage {
    let mut img = image::RgbaImage::from_pixel(
        VRAM_W as u32,
        VRAM_H as u32,
        image::Rgba([24, 24, 28, 255]),
    );
    let mut fill = |x: u16, y: u16, w: u16, h: u16, c: [u8; 4]| {
        for py in y..(y + h).min(VRAM_H) {
            for px in x..(x + w).min(VRAM_W) {
                img.put_pixel(px as u32, py as u32, image::Rgba(c));
            }
        }
    };
    for (i, (_, r)) in default_reserved().iter().enumerate() {
        let c = if i == 0 {
            [40, 60, 110, 255]
        } else {
            [90, 90, 90, 255]
        };
        fill(r.x, r.y, r.w, r.h, c);
    }
    let colors: [[u8; 4]; 6] = [
        [200, 120, 60, 255],
        [90, 170, 90, 255],
        [170, 90, 170, 255],
        [90, 160, 180, 255],
        [200, 180, 80, 255],
        [160, 100, 100, 255],
    ];
    for (i, (req, p)) in requests.iter().zip(placements).enumerate() {
        let c = colors[i % colors.len()];
        fill(p.x, p.y, req.words, req.height, c);
        if req.clut_entries > 0 {
            fill(p.clut_x, p.clut_y, req.clut_entries, 1, [255, 255, 255, 255]);
        }
    }
    // Page grid (64-word columns, 256-row bands).
    for gx in (0..VRAM_W).step_by(64) {
        for gy in 0..VRAM_H {
            let px = img.get_pixel_mut(gx as u32, gy as u32);
            px.0 = [px.0[0] / 2 + 40, px.0[1] / 2 + 40, px.0[2] / 2 + 40, 255];
        }
    }
    for gy in [0u16, 256] {
        for gx in 0..VRAM_W {
            let px = img.get_pixel_mut(gx as u32, gy as u32);
            px.0 = [px.0[0] / 2 + 40, px.0[1] / 2 + 40, px.0[2] / 2 + 40, 255];
        }
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tex(label: &str, words: u16, height: u16, clut: u16) -> TexRequest {
        TexRequest {
            label: label.into(),
            words,
            height,
            clut_entries: clut,
        }
    }

    #[test]
    fn packs_two_8bpp_textures_beside_framebuffers() {
        let reqs = vec![tex("a", 128, 256, 256), tex("b", 128, 256, 256)];
        let p = pack(&reqs).unwrap();
        assert_eq!((p[0].x, p[0].y), (320, 0));
        assert_eq!((p[1].x, p[1].y), (448, 0));
        // CLUTs in the bottom strip, non-overlapping.
        assert_eq!(p[0].clut_y, 504);
        assert_ne!(p[0].clut_x, p[1].clut_x);
        // Page alignment.
        for pl in &p {
            assert_eq!(pl.x % 64, 0);
            assert!(pl.y == 0 || pl.y == 256);
            assert_eq!(pl.clut_x % 16, 0);
        }
    }

    #[test]
    fn small_textures_get_their_own_pages() {
        let reqs = vec![tex("a", 32, 64, 16), tex("b", 32, 64, 16)];
        let p = pack(&reqs).unwrap();
        assert_eq!((p[0].x, p[0].y), (320, 0));
        assert_eq!((p[1].x, p[1].y), (384, 0));
    }

    #[test]
    fn band2_used_when_band1_full() {
        // Five 256-wide 16bpp textures: band 1 fits (1024-320)/256 = 2,
        // then two more go to band 2 (x from 0 allowed: below framebuffers?
        // no - band 2 y=256 is still inside fb rows, so x starts at 320).
        let reqs = vec![
            tex("a", 256, 256, 0),
            tex("b", 256, 256, 0),
            tex("c", 256, 200, 0),
        ];
        let p = pack(&reqs).unwrap();
        assert_eq!((p[0].x, p[0].y), (320, 0));
        assert_eq!((p[1].x, p[1].y), (576, 0));
        // Third doesn't fit band 1 (832+256 > 1024 would hit the font at
        // 960 anyway); goes to band 2.
        assert_eq!((p[2].x, p[2].y), (320, 256));
    }

    #[test]
    fn oversized_texture_is_rejected() {
        assert!(pack(&[tex("big", 300, 100, 0)]).is_err());
        assert!(pack(&[tex("tall", 100, 300, 0)]).is_err());
    }

    #[test]
    fn full_band2_texture_rejected_from_clut_strip() {
        // 256-tall texture cannot go in band 2 (would cover the CLUT strip);
        // with band 1 full it must fail rather than overlap.
        let reqs = vec![
            tex("a", 256, 256, 0),
            tex("b", 256, 256, 0),
            tex("c", 256, 256, 0),
        ];
        assert!(pack(&reqs).is_err());
    }
}
