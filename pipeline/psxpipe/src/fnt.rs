//! Polices bitmap .fnt : un atlas TIM (4bpp) + une table de chasses.
//! Layout : "FNT1" | cell_w u8 | cell_h u8 | first u8 | count u8 |
//! advances[count] u8 | pad(4) | TIM. Les glyphes sont rangés en grille
//! de 16 par ligne dans l'atlas, à partir du caractère `first`.

use crate::tim;

pub const MAGIC: &[u8; 4] = b"FNT1";
pub const GLYPHS_PER_ROW: u32 = 16;

pub struct FntInfo {
    pub cell_w: u8,
    pub cell_h: u8,
    pub first: u8,
    pub count: u8,
    /// Offset du blob TIM dans le fichier .fnt.
    pub tim_offset: usize,
}

fn align4(v: usize) -> usize {
    (v + 3) & !3
}

pub fn parse(data: &[u8]) -> Result<FntInfo, String> {
    if data.len() < 8 || &data[0..4] != MAGIC {
        return Err("pas un fichier FNT".into());
    }
    let count = data[7];
    let tim_offset = align4(8 + count as usize);
    if data.len() < tim_offset + 8 {
        return Err("FNT tronqué".into());
    }
    Ok(FntInfo {
        cell_w: data[4],
        cell_h: data[5],
        first: data[6],
        count,
        tim_offset,
    })
}

pub fn advances(data: &[u8]) -> &[u8] {
    &data[8..8 + data[7] as usize]
}

/// Encode une police depuis une image RGBA en grille régulière (16 glyphes
/// par ligne, à partir de `first`). La chasse de chaque glyphe est mesurée
/// sur ses pixels non transparents (+1 d'interlettre) ; une cellule vide
/// (l'espace) reçoit cell_w/2.
pub fn encode(
    rgba: &[u8],
    width: u32,
    height: u32,
    cell_w: u8,
    cell_h: u8,
    first: u8,
    count: u8,
) -> Result<Vec<u8>, String> {
    let rows = (count as u32).div_ceil(GLYPHS_PER_ROW);
    if width < GLYPHS_PER_ROW * cell_w as u32 || height < rows * cell_h as u32 {
        return Err(format!(
            "image {width}x{height} trop petite pour {count} glyphes de {cell_w}x{cell_h}"
        ));
    }

    let mut adv = Vec::with_capacity(count as usize);
    for g in 0..count as u32 {
        let gx = (g % GLYPHS_PER_ROW) * cell_w as u32;
        let gy = (g / GLYPHS_PER_ROW) * cell_h as u32;
        let mut max_col: i32 = -1;
        for y in 0..cell_h as u32 {
            for x in 0..cell_w as u32 {
                let o = (((gy + y) * width + gx + x) * 4 + 3) as usize;
                if rgba[o] >= 128 && x as i32 > max_col {
                    max_col = x as i32;
                }
            }
        }
        adv.push(if max_col < 0 {
            (cell_w / 2).max(2)
        } else {
            (max_col + 2).min(cell_w as i32) as u8
        });
    }

    let opts = tim::TimOptions {
        bpp: tim::Bpp::Four,
        ..Default::default()
    };
    let (timg, _) = tim::encode(rgba, width, height, &opts)?;
    let tim_blob = timg.write();

    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.push(cell_w);
    out.push(cell_h);
    out.push(first);
    out.push(count);
    out.extend_from_slice(&adv);
    out.resize(align4(out.len()), 0);
    out.extend_from_slice(&tim_blob);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_and_parse_roundtrip() {
        // 16x1 glyphes de 4x6 : le glyphe 1 a 2 colonnes de pixels.
        let (w, h) = (64u32, 6u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..6 {
            for x in 4..6 {
                let o = ((y * w + x) * 4) as usize;
                rgba[o..o + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        let fnt = encode(&rgba, w, h, 4, 6, 32, 16).unwrap();
        let info = parse(&fnt).unwrap();
        assert_eq!((info.cell_w, info.cell_h, info.first, info.count), (4, 6, 32, 16));
        let adv = advances(&fnt);
        assert_eq!(adv[0], 2); // cellule vide (espace) -> cell_w/2
        assert_eq!(adv[1], 3); // colonnes 0..=1 -> chasse 1+2 (interlettre)
        // Le TIM embarqué est un vrai TIM 4bpp.
        assert_eq!(&fnt[info.tim_offset..info.tim_offset + 4], &[0x10, 0, 0, 0]);
        assert_eq!(fnt[info.tim_offset + 4] & 3, 0);
    }
}
