//! Median-cut color quantization for PS1 palettes (16 or 256 colors).

/// Convert an 8-bit RGB color to a PS1 15-bit VRAM word.
/// Bit 15 is the STP (semi-transparency) flag; the value 0x0000 is fully
/// transparent when used as a texture texel.
pub fn rgb888_to_ps1(r: u8, g: u8, b: u8, stp: bool) -> u16 {
    let r5 = (r >> 3) as u16;
    let g5 = (g >> 3) as u16;
    let b5 = (b >> 3) as u16;
    r5 | (g5 << 5) | (b5 << 10) | ((stp as u16) << 15)
}

/// Encode an opaque color, avoiding the reserved transparent value 0x0000:
/// opaque black becomes 0x8000 (STP set on black = opaque black on PS1).
pub fn opaque_to_ps1(r: u8, g: u8, b: u8) -> u16 {
    let c = rgb888_to_ps1(r, g, b, false);
    if c == 0 {
        0x8000
    } else {
        c
    }
}

/// Median-cut quantization: reduce `pixels` to at most `max_colors` colors.
/// Returns the palette (averaged box colors). Pixels should be opaque RGB.
pub fn median_cut(pixels: &[[u8; 3]], max_colors: usize) -> Vec<[u8; 3]> {
    assert!(max_colors >= 1);

    // Start from the unique colors: no point splitting further than that.
    let mut unique = pixels.to_vec();
    unique.sort_unstable();
    unique.dedup();
    if unique.len() <= max_colors {
        return unique;
    }

    let mut boxes: Vec<Vec<[u8; 3]>> = vec![pixels.to_vec()];
    while boxes.len() < max_colors {
        // Pick the box with the widest channel range that can still split.
        let mut best: Option<(usize, usize, u8)> = None; // (box, channel, range)
        for (bi, b) in boxes.iter().enumerate() {
            for ch in 0..3 {
                let min = b.iter().map(|p| p[ch]).min().unwrap();
                let max = b.iter().map(|p| p[ch]).max().unwrap();
                let range = max - min;
                if range > 0 && best.is_none_or(|(_, _, r)| range > r) {
                    best = Some((bi, ch, range));
                }
            }
        }
        let Some((bi, ch, _)) = best else { break };

        let mut b = boxes.swap_remove(bi);
        b.sort_unstable_by_key(|p| p[ch]);
        let half = b.len() / 2;
        let right = b.split_off(half);
        boxes.push(b);
        boxes.push(right);
    }

    boxes
        .iter()
        .map(|b| {
            let n = b.len() as u32;
            let sum = b.iter().fold([0u32; 3], |mut acc, p| {
                for ch in 0..3 {
                    acc[ch] += p[ch] as u32;
                }
                acc
            });
            [
                (sum[0] / n) as u8,
                (sum[1] / n) as u8,
                (sum[2] / n) as u8,
            ]
        })
        .collect()
}

/// Index of the palette color closest to `px` (squared RGB distance).
pub fn nearest(palette: &[[u8; 3]], px: [u8; 3]) -> usize {
    let mut best = 0;
    let mut best_d = u32::MAX;
    for (i, p) in palette.iter().enumerate() {
        let d: u32 = (0..3)
            .map(|ch| {
                let diff = p[ch] as i32 - px[ch] as i32;
                (diff * diff) as u32
            })
            .sum();
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ps1_color_encoding() {
        assert_eq!(rgb888_to_ps1(255, 255, 255, false), 0x7FFF);
        assert_eq!(rgb888_to_ps1(0, 0, 0, false), 0x0000);
        assert_eq!(rgb888_to_ps1(255, 0, 0, false), 0x001F);
        assert_eq!(rgb888_to_ps1(0, 0, 255, false), 0x7C00);
        assert_eq!(opaque_to_ps1(0, 0, 0), 0x8000);
        // Channel values below 8 collapse to 0 in 5-bit precision.
        assert_eq!(opaque_to_ps1(7, 7, 7), 0x8000);
    }

    #[test]
    fn median_cut_few_colors_passthrough() {
        let px = vec![[255, 0, 0], [0, 255, 0], [255, 0, 0]];
        let pal = median_cut(&px, 16);
        assert_eq!(pal.len(), 2);
    }

    #[test]
    fn median_cut_reduces_to_target() {
        // 64 distinct grays must reduce to 16 palette entries.
        let px: Vec<[u8; 3]> = (0..64).map(|i| [(i * 4) as u8; 3]).collect();
        let pal = median_cut(&px, 16);
        assert_eq!(pal.len(), 16);
        // Every pixel maps to some palette entry within quantization error.
        for p in &px {
            let n = nearest(&pal, *p);
            let d = (pal[n][0] as i32 - p[0] as i32).abs();
            assert!(d <= 16, "quantization error too large: {d}");
        }
    }
}
