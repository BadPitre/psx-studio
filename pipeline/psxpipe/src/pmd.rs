//! PMD mesh format writer — see docs/PMD-FORMAT.md for the specification.
//!
//! Primitives are stored pre-encoded as PS1 GPU packets (POLY_F3/G3/FT3/GT3
//! layouts from PSn00bSDK, tag included): at runtime the engine copies the
//! packet into the primitive buffer and only fills in screen coordinates,
//! lit colors and (once, at load time) tpage/CLUT.

pub const MAGIC: &[u8; 4] = b"PMD1";
pub const VERSION: u16 = 1;
pub const HEADER_SIZE: usize = 48;

pub const FLAG_TEXTURED: u16 = 1 << 0;

/// GPU packet codes (opaque, texture modulation enabled).
const CODE_F3: u8 = 0x20;
const CODE_FT3: u8 = 0x24;
const CODE_G3: u8 = 0x30;
const CODE_GT3: u8 = 0x34;

/// Packet sizes in bytes, 4-byte tag included (PSn00bSDK struct sizes).
pub const SIZE_F3: usize = 20;
pub const SIZE_G3: usize = 28;
pub const SIZE_FT3: usize = 32;
pub const SIZE_GT3: usize = 40;

/// Kind order used everywhere (header counts, offsets, sections).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimKind {
    F3 = 0,
    G3 = 1,
    FT3 = 2,
    GT3 = 3,
}

/// Flat-shaded untextured triangle: single normal, single color.
#[derive(Debug, Clone)]
pub struct PrimF3 {
    pub vidx: [u16; 3],
    pub nidx: u16,
    pub color: [u8; 3],
}

/// Gouraud-shaded untextured triangle: one normal per vertex.
#[derive(Debug, Clone)]
pub struct PrimG3 {
    pub vidx: [u16; 3],
    pub nidx: [u16; 3],
    pub color: [u8; 3],
}

/// Flat-shaded textured triangle.
#[derive(Debug, Clone)]
pub struct PrimFt3 {
    pub vidx: [u16; 3],
    pub nidx: u16,
    pub color: [u8; 3],
    pub uv: [[u8; 2]; 3],
}

/// Gouraud-shaded textured triangle.
#[derive(Debug, Clone)]
pub struct PrimGt3 {
    pub vidx: [u16; 3],
    pub nidx: [u16; 3],
    pub color: [u8; 3],
    pub uv: [[u8; 2]; 3],
}

#[derive(Debug, Default)]
pub struct Pmd {
    /// 4.12 fixed: source-asset units per PMD unit (editor metadata).
    pub scale_4_12: i32,
    pub textured: bool,
    /// Model-space positions, quantized to i16 (SVECTOR layout on disk).
    pub verts: Vec<[i16; 3]>,
    /// Unit normals in 4.12 fixed point (SVECTOR layout on disk).
    pub normals: Vec<[i16; 3]>,
    pub f3: Vec<PrimF3>,
    pub g3: Vec<PrimG3>,
    pub ft3: Vec<PrimFt3>,
    pub gt3: Vec<PrimGt3>,
}

/// GPU packet tag: link filled by addPrim at runtime, length in words
/// (payload without the tag) pre-set in the top byte.
fn tag(packet_size: usize) -> [u8; 4] {
    [0, 0, 0, ((packet_size - 4) / 4) as u8]
}

fn push_u16s(out: &mut Vec<u8>, vals: &[u16]) {
    for v in vals {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn push_xy_slots(out: &mut Vec<u8>, n: usize) {
    // Screen coordinates are filled by the GTE at runtime.
    out.extend_from_slice(&vec![0u8; 4 * n]);
}

impl Pmd {
    pub fn prim_count(&self) -> usize {
        self.f3.len() + self.g3.len() + self.ft3.len() + self.gt3.len()
    }

    /// Serialize to the on-disk PMD v1 format.
    pub fn write(&self) -> Result<Vec<u8>, String> {
        if self.verts.len() > u16::MAX as usize {
            return Err(format!("too many vertices: {}", self.verts.len()));
        }
        if self.normals.len() > u16::MAX as usize {
            return Err(format!("too many normals: {}", self.normals.len()));
        }
        for count in [self.f3.len(), self.g3.len(), self.ft3.len(), self.gt3.len()] {
            if count > u16::MAX as usize {
                return Err(format!("too many primitives of one kind: {count}"));
            }
        }

        // Data sections, in header order.
        let mut verts = Vec::with_capacity(self.verts.len() * 8);
        for v in &self.verts {
            push_u16s(&mut verts, &[v[0] as u16, v[1] as u16, v[2] as u16, 0]);
        }
        let mut normals = Vec::with_capacity(self.normals.len() * 8);
        for n in &self.normals {
            push_u16s(&mut normals, &[n[0] as u16, n[1] as u16, n[2] as u16, 0]);
        }

        // F3 record: vidx[3], nidx | 20-byte POLY_F3 template.
        let mut f3 = Vec::new();
        for p in &self.f3 {
            push_u16s(&mut f3, &[p.vidx[0], p.vidx[1], p.vidx[2], p.nidx]);
            f3.extend_from_slice(&tag(SIZE_F3));
            f3.extend_from_slice(&[p.color[0], p.color[1], p.color[2], CODE_F3]);
            push_xy_slots(&mut f3, 3);
        }

        // G3 record: vidx[3], nidx[3] | 28-byte POLY_G3 template.
        let mut g3 = Vec::new();
        for p in &self.g3 {
            push_u16s(
                &mut g3,
                &[p.vidx[0], p.vidx[1], p.vidx[2], p.nidx[0], p.nidx[1], p.nidx[2]],
            );
            g3.extend_from_slice(&tag(SIZE_G3));
            g3.extend_from_slice(&[p.color[0], p.color[1], p.color[2], CODE_G3]);
            push_xy_slots(&mut g3, 1); // x0y0
            g3.extend_from_slice(&[p.color[0], p.color[1], p.color[2], 0]); // r1
            push_xy_slots(&mut g3, 1); // x1y1
            g3.extend_from_slice(&[p.color[0], p.color[1], p.color[2], 0]); // r2
            push_xy_slots(&mut g3, 1); // x2y2
        }

        // FT3 record: vidx[3], nidx | 32-byte POLY_FT3 template.
        // clut (after u0v0) and tpage (after u1v1) are patched at load time.
        let mut ft3 = Vec::new();
        for p in &self.ft3 {
            push_u16s(&mut ft3, &[p.vidx[0], p.vidx[1], p.vidx[2], p.nidx]);
            ft3.extend_from_slice(&tag(SIZE_FT3));
            ft3.extend_from_slice(&[p.color[0], p.color[1], p.color[2], CODE_FT3]);
            push_xy_slots(&mut ft3, 1);
            ft3.extend_from_slice(&[p.uv[0][0], p.uv[0][1], 0, 0]); // u0 v0 clut
            push_xy_slots(&mut ft3, 1);
            ft3.extend_from_slice(&[p.uv[1][0], p.uv[1][1], 0, 0]); // u1 v1 tpage
            push_xy_slots(&mut ft3, 1);
            ft3.extend_from_slice(&[p.uv[2][0], p.uv[2][1], 0, 0]); // u2 v2 pad
        }

        // GT3 record: vidx[3], nidx[3] | 40-byte POLY_GT3 template.
        let mut gt3 = Vec::new();
        for p in &self.gt3 {
            push_u16s(
                &mut gt3,
                &[p.vidx[0], p.vidx[1], p.vidx[2], p.nidx[0], p.nidx[1], p.nidx[2]],
            );
            gt3.extend_from_slice(&tag(SIZE_GT3));
            gt3.extend_from_slice(&[p.color[0], p.color[1], p.color[2], CODE_GT3]);
            push_xy_slots(&mut gt3, 1);
            gt3.extend_from_slice(&[p.uv[0][0], p.uv[0][1], 0, 0]); // u0 v0 clut
            gt3.extend_from_slice(&[p.color[0], p.color[1], p.color[2], 0]); // r1
            push_xy_slots(&mut gt3, 1);
            gt3.extend_from_slice(&[p.uv[1][0], p.uv[1][1], 0, 0]); // u1 v1 tpage
            gt3.extend_from_slice(&[p.color[0], p.color[1], p.color[2], 0]); // r2
            push_xy_slots(&mut gt3, 1);
            gt3.extend_from_slice(&[p.uv[2][0], p.uv[2][1], 0, 0]); // u2 v2 pad
        }

        // Header.
        let mut out = Vec::with_capacity(
            HEADER_SIZE + verts.len() + normals.len() + f3.len() + g3.len() + ft3.len() + gt3.len(),
        );
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        let flags = if self.textured { FLAG_TEXTURED } else { 0 };
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&self.scale_4_12.to_le_bytes());
        out.extend_from_slice(&(self.verts.len() as u16).to_le_bytes());
        out.extend_from_slice(&(self.normals.len() as u16).to_le_bytes());
        for count in [self.f3.len(), self.g3.len(), self.ft3.len(), self.gt3.len()] {
            out.extend_from_slice(&(count as u16).to_le_bytes());
        }

        let verts_offset = HEADER_SIZE;
        let normals_offset = verts_offset + verts.len();
        let f3_offset = normals_offset + normals.len();
        let g3_offset = f3_offset + f3.len();
        let ft3_offset = g3_offset + g3.len();
        let gt3_offset = ft3_offset + ft3.len();
        for off in [verts_offset, normals_offset, f3_offset, g3_offset, ft3_offset, gt3_offset] {
            out.extend_from_slice(&(off as u32).to_le_bytes());
        }
        debug_assert_eq!(out.len(), HEADER_SIZE);

        out.extend_from_slice(&verts);
        out.extend_from_slice(&normals);
        out.extend_from_slice(&f3);
        out.extend_from_slice(&g3);
        out.extend_from_slice(&ft3);
        out.extend_from_slice(&gt3);
        Ok(out)
    }
}

/// Parsed header, for `psxpipe info` and tests.
#[derive(Debug)]
pub struct PmdHeader {
    pub version: u16,
    pub flags: u16,
    pub scale_4_12: i32,
    pub vertex_count: u16,
    pub normal_count: u16,
    pub prim_counts: [u16; 4],
    pub offsets: [u32; 6],
}

pub fn parse_header(data: &[u8]) -> Result<PmdHeader, String> {
    if data.len() < HEADER_SIZE {
        return Err("file too small for a PMD header".into());
    }
    if &data[0..4] != MAGIC {
        return Err("bad magic (not a PMD file)".into());
    }
    let u16_at = |o: usize| u16::from_le_bytes([data[o], data[o + 1]]);
    let u32_at = |o: usize| u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
    let header = PmdHeader {
        version: u16_at(4),
        flags: u16_at(6),
        scale_4_12: u32_at(8) as i32,
        vertex_count: u16_at(12),
        normal_count: u16_at(14),
        prim_counts: [u16_at(16), u16_at(18), u16_at(20), u16_at(22)],
        offsets: [
            u32_at(24),
            u32_at(28),
            u32_at(32),
            u32_at(36),
            u32_at(40),
            u32_at(44),
        ],
    };
    if header.version != VERSION {
        return Err(format!("unsupported PMD version {}", header.version));
    }
    Ok(header)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_parse_roundtrip() {
        let pmd = Pmd {
            scale_4_12: 8192,
            textured: true,
            verts: vec![[-100, -100, 0], [100, -100, 0], [0, 100, 0]],
            normals: vec![[0, 0, -4096]],
            gt3: vec![PrimGt3 {
                vidx: [0, 1, 2],
                nidx: [0, 0, 0],
                color: [128, 128, 128],
                uv: [[0, 0], [255, 0], [128, 255]],
            }],
            ..Default::default()
        };
        let bytes = pmd.write().unwrap();
        let h = parse_header(&bytes).unwrap();
        assert_eq!(h.flags & FLAG_TEXTURED, FLAG_TEXTURED);
        assert_eq!(h.scale_4_12, 8192);
        assert_eq!(h.vertex_count, 3);
        assert_eq!(h.normal_count, 1);
        assert_eq!(h.prim_counts, [0, 0, 0, 1]);

        // Section layout: verts | normals | prims.
        assert_eq!(h.offsets[0] as usize, HEADER_SIZE);
        assert_eq!(h.offsets[1] as usize, HEADER_SIZE + 3 * 8);
        let gt3_off = h.offsets[5] as usize;
        // GT3 record = 12 index bytes + 40-byte packet.
        assert_eq!(bytes.len(), gt3_off + 12 + SIZE_GT3);

        // First vertex on disk: -100 as i16 LE.
        let vx = i16::from_le_bytes([bytes[HEADER_SIZE], bytes[HEADER_SIZE + 1]]);
        assert_eq!(vx, -100);

        // Packet template: tag length = 9 words, code = 0x34.
        let packet = &bytes[gt3_off + 12..];
        assert_eq!(packet[3], 9);
        assert_eq!(packet[7], 0x34);
        // u1v1 at 4(tag)+4(rgb0)+4(xy0)+4(uv0/clut)+4(r1)+4(xy1) = offset 24.
        assert_eq!(packet[24], 255);
        assert_eq!(packet[25], 0);
    }

    #[test]
    fn record_sizes_match_spec() {
        let mut pmd = Pmd {
            verts: vec![[0, 0, 0]; 3],
            normals: vec![[0, 0, 4096]; 3],
            ..Default::default()
        };
        pmd.f3.push(PrimF3 { vidx: [0, 1, 2], nidx: 0, color: [255, 0, 0] });
        pmd.g3.push(PrimG3 { vidx: [0, 1, 2], nidx: [0, 1, 2], color: [0, 255, 0] });
        pmd.ft3.push(PrimFt3 { vidx: [0, 1, 2], nidx: 0, color: [128; 3], uv: [[0, 0]; 3] });
        pmd.gt3.push(PrimGt3 { vidx: [0, 1, 2], nidx: [0, 1, 2], color: [128; 3], uv: [[0, 0]; 3] });
        let bytes = pmd.write().unwrap();
        let h = parse_header(&bytes).unwrap();
        assert_eq!(h.offsets[3] - h.offsets[2], (8 + SIZE_F3) as u32);
        assert_eq!(h.offsets[4] - h.offsets[3], (12 + SIZE_G3) as u32);
        assert_eq!(h.offsets[5] - h.offsets[4], (8 + SIZE_FT3) as u32);
        assert_eq!(bytes.len() as u32 - h.offsets[5], (12 + SIZE_GT3) as u32);
    }
}
