//! Software preview of PSX Studio console assets, simulating the runtime:
//! same projection (H=160, 320x240), same culling, same lighting model,
//! affine texture mapping like the PS1 GPU.
//!
//! Model mode:  cargo run --example preview -- model.pmd [texture.tim] [out.png]
//!              [--yaw N] [--pitch N] [--dist N]     (orbit camera, PS1 angle units)
//! Scene mode:  cargo run --example preview -- scene.psc [out.png]
//!              [--yaw N] [--pitch N] [--cam X,Y,Z]  (free camera like the player)

// Index loops mirror the fixed-point matrix code of the runtime.
#![allow(clippy::needless_range_loop)]

use std::path::PathBuf;

const W: usize = 320;
const H: usize = 240;

/* ------------------------------------------------------------------ TIM -- */

struct Tim {
    bpp: u32,
    clut: Vec<u16>,
    pixels: Vec<u16>,
    words_per_row: usize,
}

fn parse_tim(data: &[u8]) -> Result<Tim, String> {
    let u16at = |o: usize| u16::from_le_bytes([data[o], data[o + 1]]);
    let u32at = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
    if u32at(0) != 0x10 {
        return Err("not a TIM file".into());
    }
    let flags = u32at(4);
    let bpp = match flags & 3 {
        0 => 4,
        1 => 8,
        2 => 16,
        _ => return Err("unsupported TIM mode".into()),
    };
    let mut off = 8;
    let mut clut = Vec::new();
    if flags & 8 != 0 {
        let len = u32at(off) as usize;
        for i in 0..(len - 12) / 2 {
            clut.push(u16at(off + 12 + i * 2));
        }
        off += len;
    }
    let words_per_row = u16at(off + 8) as usize;
    let h = u16at(off + 10) as usize;
    let mut pixels = Vec::with_capacity(words_per_row * h);
    for i in 0..words_per_row * h {
        pixels.push(u16at(off + 12 + i * 2));
    }
    Ok(Tim {
        bpp,
        clut,
        pixels,
        words_per_row,
    })
}

fn ps1_to_rgb(c: u16) -> [f32; 3] {
    [
        (c & 31) as f32 / 31.0,
        ((c >> 5) & 31) as f32 / 31.0,
        ((c >> 10) & 31) as f32 / 31.0,
    ]
}

impl Tim {
    fn sample(&self, u: u8, v: u8) -> [f32; 3] {
        let (u, v) = (u as usize, v as usize);
        let word = |x: usize| self.pixels.get(v * self.words_per_row + x).copied().unwrap_or(0);
        let c = match self.bpp {
            16 => word(u),
            8 => {
                let w = word(u / 2);
                let idx = (w >> ((u % 2) * 8)) & 0xFF;
                self.clut.get(idx as usize).copied().unwrap_or(0)
            }
            _ => {
                let w = word(u / 4);
                let idx = (w >> ((u % 4) * 4)) & 0xF;
                self.clut.get(idx as usize).copied().unwrap_or(0)
            }
        };
        ps1_to_rgb(c)
    }
}

/* ------------------------------------------------------------------ PMD -- */

struct Prim {
    vidx: [usize; 3],
    nidx: [usize; 3],
    color: [f32; 3],
    uv: Option<[[u8; 2]; 3]>,
}

struct Model {
    verts: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    prims: Vec<Prim>,
}

fn parse_pmd(data: &[u8]) -> Result<Model, String> {
    let header = psxpipe::pmd::parse_header(data)?;
    let u16at = |o: usize| u16::from_le_bytes([data[o], data[o + 1]]) as usize;
    let i16at = |o: usize| i16::from_le_bytes([data[o], data[o + 1]]) as f32;

    let mut verts = Vec::with_capacity(header.vertex_count as usize);
    for i in 0..header.vertex_count as usize {
        let o = header.offsets[0] as usize + i * 8;
        verts.push([i16at(o), i16at(o + 2), i16at(o + 4)]);
    }
    let mut normals = Vec::with_capacity(header.normal_count as usize);
    for i in 0..header.normal_count as usize {
        let o = header.offsets[1] as usize + i * 8;
        normals.push([i16at(o) / 4096.0, i16at(o + 2) / 4096.0, i16at(o + 4) / 4096.0]);
    }

    // (kind, record size, gouraud, textured)
    let kinds = [(0usize, 28usize, false, false), (1, 40, true, false), (2, 40, false, true), (3, 52, true, true)];
    let mut prims = Vec::new();
    for (k, rec_size, gouraud, textured) in kinds {
        let base = header.offsets[2 + k] as usize;
        let idx_size = if gouraud { 12 } else { 8 };
        for p in 0..header.prim_counts[k] as usize {
            let rec = base + p * rec_size;
            let vidx = [u16at(rec), u16at(rec + 2), u16at(rec + 4)];
            let nidx = if gouraud {
                [u16at(rec + 6), u16at(rec + 8), u16at(rec + 10)]
            } else {
                [u16at(rec + 6); 3]
            };
            let pkt = rec + idx_size;
            let color = [
                data[pkt + 4] as f32 / 128.0,
                data[pkt + 5] as f32 / 128.0,
                data[pkt + 6] as f32 / 128.0,
            ];
            let uv = if !textured {
                None
            } else if gouraud {
                Some([
                    [data[pkt + 12], data[pkt + 13]],
                    [data[pkt + 24], data[pkt + 25]],
                    [data[pkt + 36], data[pkt + 37]],
                ])
            } else {
                Some([
                    [data[pkt + 12], data[pkt + 13]],
                    [data[pkt + 20], data[pkt + 21]],
                    [data[pkt + 28], data[pkt + 29]],
                ])
            };
            prims.push(Prim {
                vidx,
                nidx,
                color,
                uv,
            });
        }
    }
    Ok(Model {
        verts,
        normals,
        prims,
    })
}

/* ----------------------------------------------------------------- math -- */

type Mat3 = [[f32; 3]; 3];

/// Rotation matrix with the RotMatrix composition order (mX * mY * mZ),
/// angles in PS1 units (4096 = full turn).
fn rot_matrix(rx: f32, ry: f32, rz: f32) -> Mat3 {
    let a = |v: f32| v / 4096.0 * std::f32::consts::TAU;
    let (sx, cx) = a(rx).sin_cos();
    let (sy, cy) = a(ry).sin_cos();
    let (sz, cz) = a(rz).sin_cos();
    let mx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let my = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let mz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    mat_mul(&mat_mul(&mx, &my), &mz)
}

fn mat_mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut o = [[0.0f32; 3]; 3];
    for r in 0..3 {
        for c in 0..3 {
            o[r][c] = (0..3).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    o
}

fn mat_vec(m: &Mat3, v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/* ------------------------------------------------------------- renderer -- */

struct Instance<'a> {
    model: &'a Model,
    tim: Option<&'a Tim>,
    /// World transform: rotation*scale and translation.
    rot: Mat3,
    /// Rotation only (unscaled), for lighting.
    light_rot: Mat3,
    t: [f32; 3],
}

/// Jusqu'à 3 lumières directionnelles, comme le GTE (soleil + 2 entités).
struct Lighting {
    /// (vers la source, couleur 0-1) par lumière active.
    lights: Vec<([f32; 3], [f32; 3])>,
    /// Torches : (position monde, couleur 0-1 premultipliée, rayon) —
    /// converties par objet en directionnelles locales, comme le runtime.
    points: Vec<([f32; 3], [f32; 3], f32)>,
    ambient: [f32; 3],
}

struct RasterTri<'a> {
    sxy: [[f32; 2]; 3],
    z: f32,
    light: [[f32; 3]; 3],
    color: [f32; 3],
    uv: Option<[[u8; 2]; 3]>,
    tim: Option<&'a Tim>,
}

fn render(
    instances: &[Instance],
    lighting: &Lighting,
    background: [f32; 3],
    view_rot: &Mat3,
    view_t: [f32; 3],
    psc: Option<&[u8]>,
    out_path: &PathBuf,
) {
    let mut tris: Vec<RasterTri> = Vec::new();

    for inst in instances {
        let comp = mat_mul(view_rot, &inst.rot);
        let ct = {
            let r = mat_vec(view_rot, inst.t);
            [r[0] + view_t[0], r[1] + view_t[1], r[2] + view_t[2]]
        };

        // Torches -> directionnelles locales par objet (parité runtime :
        // distance octogonale, atténuation linéaire, 3 lignes GTE max).
        let mut eff_lights = lighting.lights.clone();
        for (pos, color, radius) in &lighting.points {
            if eff_lights.len() >= 3 {
                break;
            }
            let d = [pos[0] - inst.t[0], pos[1] - inst.t[1], pos[2] - inst.t[2]];
            let (ax, ay, az) = (d[0].abs(), d[1].abs(), d[2].abs());
            let mx = ax.max(ay).max(az);
            let dist = mx + (ax + ay + az - mx) * 0.5;
            if dist <= 0.0 || dist >= *radius {
                continue;
            }
            let falloff = (radius - dist) / radius;
            eff_lights.push((
                [d[0] / dist, d[1] / dist, d[2] / dist],
                [color[0] * falloff, color[1] * falloff, color[2] * falloff],
            ));
        }
        for prim in &inst.model.prims {
            let mut sxy = [[0.0f32; 2]; 3];
            let mut zsum = 0.0;
            let mut behind = false;
            for k in 0..3 {
                let v = mat_vec(&comp, inst.model.verts[prim.vidx[k]]);
                let z = v[2] + ct[2];
                if z < 16.0 {
                    behind = true;
                    break;
                }
                sxy[k] = [
                    160.0 + (v[0] + ct[0]) * 160.0 / z,
                    120.0 + (v[1] + ct[1]) * 160.0 / z,
                ];
                zsum += z;
            }
            if behind {
                continue;
            }
            let nclip = sxy[0][0] * (sxy[1][1] - sxy[2][1])
                + sxy[1][0] * (sxy[2][1] - sxy[0][1])
                + sxy[2][0] * (sxy[0][1] - sxy[1][1]);
            if nclip <= 0.0 {
                continue;
            }
            // Per-vertex GTE-style lighting: CC = ambient + Σ color * (N.L)
            let mut light = [[0.0f32; 3]; 3];
            for k in 0..3 {
                let n = mat_vec(&inst.light_rot, inst.model.normals[prim.nidx[k]]);
                let mut cc = lighting.ambient;
                for (toward, color) in &eff_lights {
                    let d = (toward[0] * n[0] + toward[1] * n[1] + toward[2] * n[2])
                        .max(0.0);
                    for ch in 0..3 {
                        cc[ch] += color[ch] * d * 255.0;
                    }
                }
                for ch in 0..3 {
                    light[k][ch] = cc[ch].min(255.0) / 128.0;
                }
            }
            tris.push(RasterTri {
                sxy,
                z: zsum / 3.0,
                light,
                color: prim.color,
                uv: prim.uv,
                tim: inst.tim,
            });
        }
    }

    // Painter's algorithm, far to near, like the OT.
    tris.sort_by(|a, b| b.z.partial_cmp(&a.z).unwrap());

    let mut frame = vec![background; W * H];
    for t in &tris {
        let [a, b, c] = t.sxy;
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
        if area.abs() < 1e-6 {
            continue;
        }
        let x0 = a[0].min(b[0]).min(c[0]).floor().max(0.0) as usize;
        let x1 = a[0].max(b[0]).max(c[0]).ceil().min(W as f32 - 1.0) as usize;
        let y0 = a[1].min(b[1]).min(c[1]).floor().max(0.0) as usize;
        let y1 = a[1].max(b[1]).max(c[1]).ceil().min(H as f32 - 1.0) as usize;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let w0 = ((b[0] - px) * (c[1] - py) - (c[0] - px) * (b[1] - py)) / area;
                let w1 = ((c[0] - px) * (a[1] - py) - (a[0] - px) * (c[1] - py)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let tex = match (&t.uv, t.tim) {
                    (Some(uv), Some(tim)) => {
                        let u = w0 * uv[0][0] as f32 + w1 * uv[1][0] as f32 + w2 * uv[2][0] as f32;
                        let v = w0 * uv[0][1] as f32 + w1 * uv[1][1] as f32 + w2 * uv[2][1] as f32;
                        tim.sample(
                            u.round().clamp(0.0, 255.0) as u8,
                            v.round().clamp(0.0, 255.0) as u8,
                        )
                    }
                    _ => [1.0, 1.0, 1.0],
                };
                let out = &mut frame[y * W + x];
                for ch in 0..3 {
                    let light =
                        w0 * t.light[0][ch] + w1 * t.light[1][ch] + w2 * t.light[2][ch];
                    out[ch] = (tex[ch] * t.color[ch] * light).min(1.0);
                }
            }
        }
    }

    let mut img = image::RgbaImage::new(W as u32, H as u32);
    for (i, px) in frame.iter().enumerate() {
        img.put_pixel(
            (i % W) as u32,
            (i / W) as u32,
            image::Rgba([
                (px[0] * 255.0) as u8,
                (px[1] * 255.0) as u8,
                (px[2] * 255.0) as u8,
                255,
            ]),
        );
    }
    if let Some(data) = psc {
        draw_ui(&mut img, data);
    }
    img.save(out_path).expect("cannot write PNG");
    println!("{} tris drawn -> {}", tris.len(), out_path.display());
}

/* Overlay UI (v1.3) : parite avec Ui_Draw du runtime — memes resolutions
 * de rects (entier, 4.12), aplats colores, images Filled et texte .fnt. */
fn draw_ui(img: &mut image::RgbaImage, data: &[u8]) {
    use psxpipe::scene as sc;
    let Ok(h) = sc::parse_header(data) else { return };
    if h.ui_count == 0 {
        return;
    }
    let ui = sc::parse_ui(data, &h);
    let (_, _, strings_off) = h.ui_offsets();
    let fonts = sc::parse_fonts(data, &h);
    let parent_of = |e: usize| -> i32 {
        let off = h.entities_offset as usize + e * sc::ENTITY_SIZE + 0x18;
        let p = u16::from_le_bytes([data[off], data[off + 1]]);
        if p == 0xFFFF { -1 } else { p as i32 }
    };

    let axis = |p_start: i32, p_len: i32, amin: u16, amax: u16, pivot: u16, pos: i16, size: i16| {
        let lo = p_start + ((p_len * amin as i32) >> 12);
        let hi = p_start + ((p_len * amax as i32) >> 12);
        if amin == amax {
            (lo + pos as i32 - ((size as i32 * pivot as i32) >> 12), size as i32)
        } else {
            let start = lo + pos as i32;
            (start, hi - size as i32 - start)
        }
    };

    let mut rects = vec![(0i32, 0i32, 0i32, 0i32); ui.len()];
    let mut vis = vec![true; ui.len()];
    let mut cursor = vec![0i32; ui.len()];
    for i in 0..ui.len() {
        let rec = &ui[i];
        let (mut parent, mut pvis) = ((0, 0, W as i32, H as i32), true);
        let mut parent_ui: Option<usize> = None;
        let pe = parent_of(rec.entity as usize);
        if pe >= 0 {
            if let Some(pi) = ui[..i].iter().position(|r| r.entity as i32 == pe) {
                parent = rects[pi];
                pvis = vis[pi];
                parent_ui = Some(pi);
            }
        }
        vis[i] = pvis && rec.components & (1 << 5) != 0;
        // Parent Layout Group : empilement + alignement (parite ui.c).
        if let Some(pi) = parent_ui {
            let pr = &ui[pi];
            if pr.components & (1 << 4) != 0 {
                let (cx, cy) = (parent.0 + pr.uv[0] as i32, parent.1 + pr.uv[1] as i32);
                let cw = parent.2 - pr.uv[0] as i32 - pr.uv[2] as i32;
                let ch = parent.3 - pr.uv[1] as i32 - pr.uv[3] as i32;
                let horizontal = pr.flags & (1 << 4) != 0;
                let w = if pr.flags & (1 << 5) != 0 && !horizontal { cw } else { rec.size[0] as i32 };
                let h = if pr.flags & (1 << 6) != 0 && horizontal { ch } else { rec.size[1] as i32 };
                let align = pr.border[0] as i32;
                if horizontal {
                    rects[i] = (cx + cursor[pi], cy + (align / 3) * (ch - h) / 2, w, h);
                    cursor[pi] += w + pr.extra as i32;
                } else {
                    rects[i] = (cx + (align % 3) * (cw - w) / 2, cy + cursor[pi], w, h);
                    cursor[pi] += h + pr.extra as i32;
                }
                continue;
            }
        }
        let (x, w) = axis(parent.0, parent.2, rec.anchors[0], rec.anchors[2], rec.anchors[4], rec.pos[0], rec.size[0]);
        let (y, hh) = axis(parent.1, parent.3, rec.anchors[1], rec.anchors[3], rec.anchors[5], rec.pos[1], rec.size[1]);
        rects[i] = (x, y, w, hh);
    }

    for i in 0..ui.len() {
        let rec = &ui[i];
        let (x, y, mut w, mut hh) = rects[i];
        if !vis[i] || w <= 0 || hh <= 0 {
            continue;
        }
        if rec.components & (1 << 1) != 0 && rec.asset == 0xFF {
            if rec.flags & 3 == 3 {
                if rec.flags & (1 << 2) != 0 {
                    hh = (hh * rec.data as i32) >> 12;
                } else {
                    w = (w * rec.data as i32) >> 12;
                }
            }
            for py in y.max(0)..(y + hh).min(H as i32) {
                for px in x.max(0)..(x + w).min(W as i32) {
                    img.put_pixel(px as u32, py as u32,
                        image::Rgba([rec.color[0], rec.color[1], rec.color[2], 255]));
                }
            }
        }
        if rec.components & (1 << 2) != 0 {
            let Some(&(foff, fsize)) = fonts.get(rec.asset as usize) else { continue };
            let fdata = &data[foff..foff + fsize];
            let info = psxpipe::fnt::parse(fdata).unwrap();
            let adv = psxpipe::fnt::advances(fdata);
            let tim = parse_tim(&fdata[info.tim_offset..]).unwrap();
            let start = strings_off + rec.data as usize;
            let end = data[start..].iter().position(|&b| b == 0).unwrap() + start;
            let text = &data[start..end];
            let width: i32 = text.iter().map(|&c| {
                let g = c as i32 - info.first as i32;
                if g >= 0 && (g as usize) < info.count as usize { adv[g as usize] as i32 }
                else { info.cell_w as i32 / 2 }
            }).sum();
            let mut cx = match rec.extra {
                1 => x + (w - width) / 2,
                2 => x + w - width,
                _ => x,
            };
            for &c in text {
                let g = c as i32 - info.first as i32;
                if g < 0 || g as usize >= info.count as usize {
                    cx += info.cell_w as i32 / 2;
                    continue;
                }
                let (gu, gv) = ((g % 16) * info.cell_w as i32, (g / 16) * info.cell_h as i32);
                for py in 0..info.cell_h as i32 {
                    for px in 0..info.cell_w as i32 {
                        let rgb = tim.sample((gu + px) as u8, (gv + py) as u8);
                        if rgb == [0.0, 0.0, 0.0] {
                            continue;
                        }
                        let (sx, sy) = (cx + px, y + py);
                        if sx < 0 || sx >= W as i32 || sy < 0 || sy >= H as i32 {
                            continue;
                        }
                        img.put_pixel(sx as u32, sy as u32,
                            image::Rgba([rec.color[0], rec.color[1], rec.color[2], 255]));
                    }
                }
                cx += adv[g as usize] as i32;
            }
        }
    }
}

/* ----------------------------------------------------------------- main -- */

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut files: Vec<PathBuf> = Vec::new();
    let (mut yaw, mut pitch, mut dist) = (500.0f32, -256.0f32, 600.0f32);
    let mut cam_arg: Option<[f32; 3]> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--yaw" => yaw = it.next().unwrap().parse().unwrap(),
            "--pitch" => pitch = it.next().unwrap().parse().unwrap(),
            "--dist" => dist = it.next().unwrap().parse().unwrap(),
            "--cam" => {
                let parts: Vec<f32> = it
                    .next()
                    .unwrap()
                    .split(',')
                    .map(|s| s.trim().parse().unwrap())
                    .collect();
                cam_arg = Some([parts[0], parts[1], parts[2]]);
            }
            _ => files.push(PathBuf::from(a)),
        }
    }
    if files.is_empty() {
        eprintln!("usage: preview <model.pmd|scene.psc> [texture.tim] [out.png] [--yaw N] [--pitch N] [--dist N] [--cam X,Y,Z]");
        std::process::exit(1);
    }
    let input = files[0].clone();
    let out_path = files
        .iter()
        .find(|f| f.extension().is_some_and(|e| e == "png"))
        .cloned()
        .unwrap_or_else(|| input.with_extension("preview.png"));

    let data = std::fs::read(&input).expect("cannot read input");

    if data.starts_with(psxpipe::scene::MAGIC) {
        /* Scene mode: free camera like the player's ResetCamera. */
        let h = psxpipe::scene::parse_header(&data).expect("bad PSC");
        let u16at = |o: usize| u16::from_le_bytes([data[o], data[o + 1]]);
        let u32at = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap()) as usize;
        let i16at = |o: usize| i16::from_le_bytes([data[o], data[o + 1]]) as f32;

        let mut tims = Vec::new();
        for i in 0..h.texture_count as usize {
            let rec = h.textures_offset as usize + i * psxpipe::scene::TEXTURE_ENTRY_SIZE;
            let off = u32at(rec);
            let size = u32at(rec + 4);
            tims.push(parse_tim(&data[off..off + size]).expect("bad TIM in scene"));
        }
        let mut models = Vec::new();
        let mut model_tex = Vec::new();
        for i in 0..h.model_count as usize {
            let rec = h.models_offset as usize + i * psxpipe::scene::MODEL_ENTRY_SIZE;
            let off = u32at(rec);
            let size = u32at(rec + 4);
            models.push(parse_pmd(&data[off..off + size]).expect("bad PMD in scene"));
            model_tex.push(u16at(rec + 8));
        }

        /* Entities: forward pass, parents first (format invariant). */
        struct Ent {
            rot: Mat3,
            light_rot: Mat3,
            t: [f32; 3],
            model: u16,
            flags: u16,
            light_radius: u16,
        }
        let mut ents: Vec<Ent> = Vec::new();
        for i in 0..h.entity_count as usize {
            let rec = h.entities_offset as usize + i * psxpipe::scene::ENTITY_SIZE;
            let pos = [i16at(rec), i16at(rec + 2), i16at(rec + 4)];
            let rot = rot_matrix(i16at(rec + 8), i16at(rec + 10), i16at(rec + 12));
            let scale = [
                i16at(rec + 16) / 4096.0,
                i16at(rec + 18) / 4096.0,
                i16at(rec + 20) / 4096.0,
            ];
            let parent = u16at(rec + 24);
            let model = u16at(rec + 26);
            let mut scaled = rot;
            for r in 0..3 {
                for c in 0..3 {
                    scaled[r][c] *= scale[c];
                }
            }
            let (world_rot, light_rot, t) = if parent == psxpipe::scene::NO_INDEX {
                (scaled, rot, pos)
            } else {
                let p = &ents[parent as usize];
                let wp = mat_vec(&p.rot, pos);
                (
                    mat_mul(&p.rot, &scaled),
                    mat_mul(&p.light_rot, &rot),
                    [wp[0] + p.t[0], wp[1] + p.t[1], wp[2] + p.t[2]],
                )
            };
            ents.push(Ent {
                rot: world_rot,
                light_rot,
                t,
                model,
                flags: u16at(rec + 0x1C),
                light_radius: u16at(rec + 0x16),
            });
        }

        let instances: Vec<Instance> = ents
            .iter()
            .filter(|e| e.model != psxpipe::scene::NO_INDEX)
            .map(|e| {
                let tex = model_tex[e.model as usize];
                Instance {
                    model: &models[e.model as usize],
                    tim: if tex == psxpipe::scene::NO_INDEX {
                        None
                    } else {
                        Some(&tims[tex as usize])
                    },
                    rot: e.rot,
                    light_rot: e.light_rot,
                    t: e.t,
                }
            })
            .collect();

        let mut lights = vec![(
            [
                h.light_toward[0] as f32 / 4096.0,
                h.light_toward[1] as f32 / 4096.0,
                h.light_toward[2] as f32 / 4096.0,
            ],
            [
                h.light_color[0] as f32 / 255.0,
                h.light_color[1] as f32 / 255.0,
                h.light_color[2] as f32 / 255.0,
            ],
        )];
        // Entités-lumières (v1.2) : directionnelles (vers la source = +Z
        // monde, 3e colonne de la rotation) et torches (position + rayon),
        // comme le runtime. Intensité en pourcent dans le pad.
        let mut points = Vec::new();
        for light in psxpipe::scene::parse_lights(&data, &h) {
            let Some(e) = ents.get(light.entity as usize) else { continue };
            let intensity = if light.intensity_percent == 0 {
                1.0
            } else {
                light.intensity_percent as f32 / 100.0
            };
            let color = [
                light.color[0] as f32 / 255.0 * intensity,
                light.color[1] as f32 / 255.0 * intensity,
                light.color[2] as f32 / 255.0 * intensity,
            ];
            if e.flags & 0x4 != 0 {
                points.push((e.t, color, e.light_radius as f32));
            } else if lights.len() < 3 {
                lights.push((
                    [e.light_rot[0][2], e.light_rot[1][2], e.light_rot[2][2]],
                    color,
                ));
            }
        }
        let lighting = Lighting {
            lights,
            points,
            ambient: [
                h.ambient[0] as f32,
                h.ambient[1] as f32,
                h.ambient[2] as f32,
            ],
        };
        let background = [
            h.background[0] as f32 / 255.0,
            h.background[1] as f32 / 255.0,
            h.background[2] as f32 / 255.0,
        ];

        /* Default camera = the player's ResetCamera. */
        let cam_pos = cam_arg.unwrap_or([0.0, -140.0, -420.0]);
        let cam_pitch = if pitch == -256.0 { 170.0 } else { pitch };
        let cam_yaw = if yaw == 500.0 { 0.0 } else { yaw };
        let view_rot = rot_matrix(cam_pitch, cam_yaw, 0.0);
        let view_t = {
            let r = mat_vec(&view_rot, [-cam_pos[0], -cam_pos[1], -cam_pos[2]]);
            [r[0], r[1], r[2]]
        };
        render(&instances, &lighting, background, &view_rot, view_t, Some(&data), &out_path);
    } else {
        /* Model mode: orbit camera like the poc-renderer. */
        let model = parse_pmd(&data).expect("bad PMD");
        let tim_path = files.iter().find(|f| f.extension().is_some_and(|e| e == "tim"));
        let tim = tim_path.map(|p| parse_tim(&std::fs::read(p).expect("cannot read TIM")).unwrap());

        let rot = rot_matrix(pitch, yaw, 0.0);
        let instances = [Instance {
            model: &model,
            tim: tim.as_ref(),
            rot,
            light_rot: rot,
            t: [0.0, 0.0, dist],
        }];
        let lighting = Lighting {
            lights: vec![(
                {
                    let n = (3.0f32).sqrt();
                    [-1.0 / n, -1.0 / n, -1.0 / n]
                },
                [1.0, 1.0, 1.0],
            )],
            points: Vec::new(),
            ambient: [64.0, 64.0, 64.0],
        };
        let identity = rot_matrix(0.0, 0.0, 0.0);
        render(
            &instances,
            &lighting,
            [16.0 / 255.0, 16.0 / 255.0, 48.0 / 255.0],
            &identity,
            [0.0, 0.0, 0.0],
            None,
            &out_path,
        );
    }
}
