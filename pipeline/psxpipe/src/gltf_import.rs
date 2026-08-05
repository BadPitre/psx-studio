//! glTF → PMD conversion.
//!
//! Coordinate conventions: glTF is right-handed, +Y up. The PS1/GTE screen
//! space is +Y down, +Z into the screen. We map (x, y, z) → (x, -y, z);
//! the mirroring also converts glTF's CCW front faces into the clockwise
//! winding expected by the GTE nclip test.

use std::collections::HashMap;
use std::path::Path;

use crate::pmd::{Pmd, PrimF3, PrimFt3, PrimG3, PrimGt3};
use crate::ONE_4_12;

pub struct ImportOptions {
    /// Target half-extent in PMD units: the largest |coordinate| of the
    /// model maps to this value (keeps meshes GTE-friendly).
    pub target_size: i32,
    /// Force flat shading (one normal per face).
    pub flat: bool,
    /// Ignore UVs and export untextured primitives.
    pub untextured: bool,
    /// Texture dimensions used to map UVs to texel coordinates.
    pub tex_w: u32,
    pub tex_h: u32,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions {
            target_size: 128,
            flat: false,
            untextured: false,
            tex_w: 256,
            tex_h: 256,
        }
    }
}

#[derive(Debug, Default)]
pub struct ImportReport {
    pub meshes: usize,
    pub triangles_in: usize,
    pub degenerate_dropped: usize,
    pub vertex_count: usize,
    pub normal_count: usize,
    pub counts: [usize; 4], // F3, G3, FT3, GT3
    pub scale_4_12: i32,
    pub warnings: Vec<String>,
}

/// One triangle of the intermediate soup (already in PS1 axis convention).
struct Tri {
    pos: [[f32; 3]; 3],
    /// Per-vertex normals, or a single face normal when flat.
    normals: [[f32; 3]; 3],
    flat: bool,
    uv: Option<[[f32; 2]; 3]>,
    color: [u8; 3],
}

fn mat_mul(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0f32; 4]; 4];
    // Column-major (glTF convention): out = a * b.
    for col in 0..4 {
        for row in 0..4 {
            out[col][row] = (0..4).map(|k| a[k][row] * b[col][k]).sum();
        }
    }
    out
}

fn transform_point(m: &[[f32; 4]; 4], p: [f32; 3]) -> [f32; 3] {
    let mut out = [0.0f32; 3];
    for row in 0..3 {
        out[row] = m[0][row] * p[0] + m[1][row] * p[1] + m[2][row] * p[2] + m[3][row];
    }
    out
}

fn transform_dir(m: &[[f32; 4]; 4], p: [f32; 3]) -> [f32; 3] {
    let mut out = [0.0f32; 3];
    for row in 0..3 {
        out[row] = m[0][row] * p[0] + m[1][row] * p[1] + m[2][row] * p[2];
    }
    out
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-12 {
        [0.0, 0.0, -1.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

fn face_normal(p: &[[f32; 3]; 3]) -> [f32; 3] {
    let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
    let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
    // Cross product; with our mirrored (CW) winding this points outward
    // when computed as e2 x e1.
    normalize([
        e2[1] * e1[2] - e2[2] * e1[1],
        e2[2] * e1[0] - e2[0] * e1[2],
        e2[0] * e1[1] - e2[1] * e1[0],
    ])
}

/// glTF → PS1 axis convention.
fn to_ps1(p: [f32; 3]) -> [f32; 3] {
    [p[0], -p[1], p[2]]
}

pub fn import(path: &Path, opts: &ImportOptions) -> Result<(Pmd, ImportReport), String> {
    let (doc, buffers, _images) =
        gltf::import(path).map_err(|e| format!("glTF import failed: {e}"))?;

    let mut report = ImportReport::default();
    let mut tris: Vec<Tri> = Vec::new();
    let mut uv_out_of_range = 0usize;

    // Walk the default scene (or the first one) applying node transforms.
    let scene = doc
        .default_scene()
        .or_else(|| doc.scenes().next())
        .ok_or("glTF file contains no scene")?;

    let identity = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let mut stack: Vec<(gltf::Node, [[f32; 4]; 4])> = scene
        .nodes()
        .map(|n| (n, identity))
        .collect();

    while let Some((node, parent)) = stack.pop() {
        let world = mat_mul(&parent, &node.transform().matrix());
        for child in node.children() {
            stack.push((child, world));
        }
        let Some(mesh) = node.mesh() else { continue };
        report.meshes += 1;

        for prim in mesh.primitives() {
            if prim.mode() != gltf::mesh::Mode::Triangles {
                report
                    .warnings
                    .push(format!("primitive mode {:?} skipped (triangles only)", prim.mode()));
                continue;
            }
            let reader = prim.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
            let Some(positions) = reader.read_positions() else {
                report.warnings.push("primitive without positions skipped".into());
                continue;
            };
            let positions: Vec<[f32; 3]> = positions.collect();
            let normals: Option<Vec<[f32; 3]>> =
                reader.read_normals().map(|n| n.collect());
            let uvs: Option<Vec<[f32; 2]>> = if opts.untextured {
                None
            } else {
                reader.read_tex_coords(0).map(|t| t.into_f32().collect())
            };
            let indices: Vec<u32> = match reader.read_indices() {
                Some(i) => i.into_u32().collect(),
                None => (0..positions.len() as u32).collect(),
            };

            let base = prim
                .material()
                .pbr_metallic_roughness()
                .base_color_factor();
            let color = if uvs.is_some() {
                // Neutral modulation color for textured polys (128 = 1.0).
                [128, 128, 128]
            } else {
                [
                    (base[0].clamp(0.0, 1.0) * 255.0) as u8,
                    (base[1].clamp(0.0, 1.0) * 255.0) as u8,
                    (base[2].clamp(0.0, 1.0) * 255.0) as u8,
                ]
            };

            let missing_normals = normals.is_none();
            if missing_normals {
                report
                    .warnings
                    .push("primitive without normals: face normals generated (flat shading)".into());
            }
            let flat = opts.flat || missing_normals;

            for tri_idx in indices.chunks_exact(3) {
                let i = [tri_idx[0] as usize, tri_idx[1] as usize, tri_idx[2] as usize];
                let pos = [
                    to_ps1(transform_point(&world, positions[i[0]])),
                    to_ps1(transform_point(&world, positions[i[1]])),
                    to_ps1(transform_point(&world, positions[i[2]])),
                ];
                let tri_normals = if flat {
                    let n = match &normals {
                        // Average the source normals so flat shading still
                        // follows the authored orientation.
                        Some(ns) => normalize(to_ps1(transform_dir(
                            &world,
                            [
                                ns[i[0]][0] + ns[i[1]][0] + ns[i[2]][0],
                                ns[i[0]][1] + ns[i[1]][1] + ns[i[2]][1],
                                ns[i[0]][2] + ns[i[1]][2] + ns[i[2]][2],
                            ],
                        ))),
                        None => face_normal(&pos),
                    };
                    [n, n, n]
                } else {
                    let ns = normals.as_ref().unwrap();
                    [
                        normalize(to_ps1(transform_dir(&world, ns[i[0]]))),
                        normalize(to_ps1(transform_dir(&world, ns[i[1]]))),
                        normalize(to_ps1(transform_dir(&world, ns[i[2]]))),
                    ]
                };
                let uv = uvs.as_ref().map(|u| {
                    let mut out = [[0.0f32; 2]; 3];
                    for k in 0..3 {
                        let c = u[i[k]];
                        if !(0.0..=1.0).contains(&c[0]) || !(0.0..=1.0).contains(&c[1]) {
                            uv_out_of_range += 1;
                        }
                        out[k] = c;
                    }
                    out
                });
                tris.push(Tri {
                    pos,
                    normals: tri_normals,
                    flat,
                    uv,
                    color,
                });
                report.triangles_in += 1;
            }
        }
    }

    if tris.is_empty() {
        return Err("no triangles found in the glTF scene".into());
    }
    if uv_out_of_range > 0 {
        report.warnings.push(format!(
            "{uv_out_of_range} UV coordinates outside [0,1] were clamped (tiling is not supported on a single texture page)"
        ));
    }

    // Global quantization scale: largest |coordinate| maps to target_size.
    let max_abs = tris
        .iter()
        .flat_map(|t| t.pos.iter())
        .flat_map(|p| p.iter())
        .fold(0.0f32, |m, c| m.max(c.abs()));
    if max_abs <= 0.0 {
        return Err("degenerate model: all coordinates are zero".into());
    }
    let quant = opts.target_size as f32 / max_abs;
    // Header metadata: source units per PMD unit, in 4.12.
    let scale_4_12 = ((max_abs / opts.target_size as f32) * ONE_4_12 as f32).round() as i32;
    report.scale_4_12 = scale_4_12.max(1);

    let mut verts: Vec<[i16; 3]> = Vec::new();
    let mut normals_out: Vec<[i16; 3]> = Vec::new();
    let mut vert_map: HashMap<[i16; 3], u16> = HashMap::new();
    let mut normal_map: HashMap<[i16; 3], u16> = HashMap::new();
    let mut pmd = Pmd {
        scale_4_12: report.scale_4_12,
        ..Default::default()
    };

    let mut vert_index = |v: [i16; 3]| -> u16 {
        *vert_map.entry(v).or_insert_with(|| {
            verts.push(v);
            (verts.len() - 1) as u16
        })
    };
    let mut normal_index = |n: [i16; 3]| -> u16 {
        *normal_map.entry(n).or_insert_with(|| {
            normals_out.push(n);
            (normals_out.len() - 1) as u16
        })
    };

    let quantize_pos = |p: [f32; 3]| -> [i16; 3] {
        [
            (p[0] * quant).round().clamp(-32768.0, 32767.0) as i16,
            (p[1] * quant).round().clamp(-32768.0, 32767.0) as i16,
            (p[2] * quant).round().clamp(-32768.0, 32767.0) as i16,
        ]
    };
    let quantize_normal = |n: [f32; 3]| -> [i16; 3] {
        [
            ((n[0] * ONE_4_12 as f32).round() as i32).clamp(-32768, 32767) as i16,
            ((n[1] * ONE_4_12 as f32).round() as i32).clamp(-32768, 32767) as i16,
            ((n[2] * ONE_4_12 as f32).round() as i32).clamp(-32768, 32767) as i16,
        ]
    };
    let quantize_uv = |uv: [f32; 2]| -> [u8; 2] {
        [
            (uv[0].clamp(0.0, 1.0) * (opts.tex_w - 1) as f32)
                .round()
                .min(255.0) as u8,
            (uv[1].clamp(0.0, 1.0) * (opts.tex_h - 1) as f32)
                .round()
                .min(255.0) as u8,
        ]
    };

    for tri in &tris {
        let vidx = [
            vert_index(quantize_pos(tri.pos[0])),
            vert_index(quantize_pos(tri.pos[1])),
            vert_index(quantize_pos(tri.pos[2])),
        ];
        if vidx[0] == vidx[1] || vidx[1] == vidx[2] || vidx[0] == vidx[2] {
            report.degenerate_dropped += 1;
            continue;
        }
        match (&tri.uv, tri.flat) {
            (Some(uv), true) => pmd.ft3.push(PrimFt3 {
                vidx,
                nidx: normal_index(quantize_normal(tri.normals[0])),
                color: tri.color,
                uv: [quantize_uv(uv[0]), quantize_uv(uv[1]), quantize_uv(uv[2])],
            }),
            (Some(uv), false) => pmd.gt3.push(PrimGt3 {
                vidx,
                nidx: [
                    normal_index(quantize_normal(tri.normals[0])),
                    normal_index(quantize_normal(tri.normals[1])),
                    normal_index(quantize_normal(tri.normals[2])),
                ],
                color: tri.color,
                uv: [quantize_uv(uv[0]), quantize_uv(uv[1]), quantize_uv(uv[2])],
            }),
            (None, true) => pmd.f3.push(PrimF3 {
                vidx,
                nidx: normal_index(quantize_normal(tri.normals[0])),
                color: tri.color,
            }),
            (None, false) => pmd.g3.push(PrimG3 {
                vidx,
                nidx: [
                    normal_index(quantize_normal(tri.normals[0])),
                    normal_index(quantize_normal(tri.normals[1])),
                    normal_index(quantize_normal(tri.normals[2])),
                ],
                color: tri.color,
            }),
        }
    }

    pmd.verts = verts;
    pmd.normals = normals_out;
    pmd.textured = !pmd.ft3.is_empty() || !pmd.gt3.is_empty();
    report.vertex_count = pmd.verts.len();
    report.normal_count = pmd.normals.len();
    report.counts = [pmd.f3.len(), pmd.g3.len(), pmd.ft3.len(), pmd.gt3.len()];

    if pmd.verts.len() > u16::MAX as usize {
        return Err(format!(
            "mesh too dense: {} vertices after quantization (max 65535)",
            pmd.verts.len()
        ));
    }
    let total = pmd.prim_count();
    if total > 4000 {
        report.warnings.push(format!(
            "{total} triangles is far above the PS1 per-frame budget (~1500-4000 for a whole scene)"
        ));
    }

    Ok((pmd, report))
}
