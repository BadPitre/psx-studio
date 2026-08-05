//! Software preview of a PMD + TIM pair, simulating the poc-renderer:
//! same projection (H=160, 320x240), same culling, same lighting model
//! (one white directional + ambient 64), affine texture mapping like the
//! PS1 GPU. Lets you check an asset before booting an emulator.
//!
//! Usage: cargo run --example preview -- <model.pmd> [texture.tim] [out.png]
//!        [--yaw N] [--pitch N] [--dist N]   (PS1 angle units, 4096 = 360°)

use std::path::PathBuf;

const W: usize = 320;
const H: usize = 240;

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
        let count = (len - 12) / 2;
        for i in 0..count {
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
        let word = |x: usize| self.pixels[v * self.words_per_row + x];
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

struct Tri {
    sxy: [[f32; 2]; 3],
    z: f32,
    // Per-vertex lighting factor (GPU modulation: 1.0 = neutral 128).
    light: [f32; 3],
    color: [f32; 3],
    uv: Option<[[u8; 2]; 3]>,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut files: Vec<PathBuf> = Vec::new();
    let (mut yaw, mut pitch, mut dist) = (500.0f32, -256.0f32, 600.0f32);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--yaw" => yaw = it.next().unwrap().parse().unwrap(),
            "--pitch" => pitch = it.next().unwrap().parse().unwrap(),
            "--dist" => dist = it.next().unwrap().parse().unwrap(),
            _ => files.push(PathBuf::from(a)),
        }
    }
    if files.is_empty() {
        eprintln!("usage: preview <model.pmd> [texture.tim] [out.png] [--yaw N] [--pitch N] [--dist N]");
        std::process::exit(1);
    }
    let pmd_path = files[0].clone();
    let tim_path = files.iter().find(|f| f.extension().is_some_and(|e| e == "tim"));
    let out_path = files
        .iter()
        .find(|f| f.extension().is_some_and(|e| e == "png"))
        .cloned()
        .unwrap_or_else(|| pmd_path.with_extension("preview.png"));

    let data = std::fs::read(&pmd_path).expect("cannot read PMD");
    let header = psxpipe::pmd::parse_header(&data).expect("bad PMD");
    let tim = tim_path.map(|p| parse_tim(&std::fs::read(p).expect("cannot read TIM")).unwrap());

    let u16at = |o: usize| u16::from_le_bytes([data[o], data[o + 1]]) as usize;
    let i16at = |o: usize| i16::from_le_bytes([data[o], data[o + 1]]) as f32;
    let vert = |i: usize| -> [f32; 3] {
        let o = header.offsets[0] as usize + i * 8;
        [i16at(o), i16at(o + 2), i16at(o + 4)]
    };
    let normal = |i: usize| -> [f32; 3] {
        let o = header.offsets[1] as usize + i * 8;
        [i16at(o) / 4096.0, i16at(o + 2) / 4096.0, i16at(o + 4) / 4096.0]
    };

    // Rotation like RotMatrix (mX * mY, no Z) and camera light like main.c.
    let (sp, cp) = (pitch / 4096.0 * std::f32::consts::TAU).sin_cos();
    let (sy, cy) = (yaw / 4096.0 * std::f32::consts::TAU).sin_cos();
    let rot = |v: [f32; 3]| -> [f32; 3] {
        // mY then mX (row-vector form of mX * mY column convention).
        let x1 = cy * v[0] + sy * v[2];
        let z1 = -sy * v[0] + cy * v[2];
        let y2 = cp * v[1] - sp * z1;
        let z2 = sp * v[1] + cp * z1;
        [x1, y2, z2]
    };
    let to_light = {
        let l = [-1.0f32, -1.0, -1.0];
        let n = (3.0f32).sqrt();
        [l[0] / n, l[1] / n, l[2] / n]
    };
    let lit = |n_idx: usize| -> f32 {
        let n = rot(normal(n_idx));
        let d = (to_light[0] * n[0] + to_light[1] * n[1] + to_light[2] * n[2]).max(0.0);
        // GTE: CC = BK(64) + LCM(ONE) * intensity(0..255) ; GPU: texel * CC/128
        (64.0 + 255.0 * d) / 128.0
    };

    let mut tris: Vec<Tri> = Vec::new();
    let kinds = [(0usize, 28usize, false, true), (1, 40, true, true), (2, 40, false, false), (3, 52, true, false)];
    // (kind index, record size, gouraud, untextured?) — untextured flag says "no uv"
    for (k, rec_size, gouraud, untextured) in kinds {
        let count = header.prim_counts[k] as usize;
        let base = header.offsets[2 + k] as usize;
        let idx_size = if gouraud { 12 } else { 8 };
        for p in 0..count {
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
            let uv = if untextured {
                None
            } else if gouraud {
                // GT3: uv at packet offsets 12, 24, 36
                Some([
                    [data[pkt + 12], data[pkt + 13]],
                    [data[pkt + 24], data[pkt + 25]],
                    [data[pkt + 36], data[pkt + 37]],
                ])
            } else {
                // FT3: uv at packet offsets 12, 20, 28
                Some([
                    [data[pkt + 12], data[pkt + 13]],
                    [data[pkt + 20], data[pkt + 21]],
                    [data[pkt + 28], data[pkt + 29]],
                ])
            };

            let mut sxy = [[0.0f32; 2]; 3];
            let mut zsum = 0.0;
            let mut behind = false;
            for k in 0..3 {
                let v = rot(vert(vidx[k]));
                let z = v[2] + dist;
                if z < 16.0 {
                    behind = true;
                    break;
                }
                sxy[k] = [160.0 + v[0] * 160.0 / z, 120.0 + v[1] * 160.0 / z];
                zsum += z;
            }
            if behind {
                continue;
            }
            // Same winding test as the runtime nclip (keep > 0).
            let nclip = sxy[0][0] * (sxy[1][1] - sxy[2][1])
                + sxy[1][0] * (sxy[2][1] - sxy[0][1])
                + sxy[2][0] * (sxy[0][1] - sxy[1][1]);
            if nclip <= 0.0 {
                continue;
            }
            tris.push(Tri {
                sxy,
                z: zsum / 3.0,
                light: [lit(nidx[0]), lit(nidx[1]), lit(nidx[2])],
                color,
                uv,
            });
        }
    }

    // Painter's algorithm: far to near, like the OT.
    tris.sort_by(|a, b| b.z.partial_cmp(&a.z).unwrap());

    let mut frame = vec![[16.0 / 255.0, 16.0 / 255.0, 48.0 / 255.0]; W * H];
    for t in &tris {
        let xs = t.sxy.iter().map(|p| p[0]).collect::<Vec<_>>();
        let ys = t.sxy.iter().map(|p| p[1]).collect::<Vec<_>>();
        let x0 = xs.iter().cloned().fold(f32::MAX, f32::min).floor().max(0.0) as usize;
        let x1 = xs.iter().cloned().fold(f32::MIN, f32::max).ceil().min(W as f32 - 1.0) as usize;
        let y0 = ys.iter().cloned().fold(f32::MAX, f32::min).floor().max(0.0) as usize;
        let y1 = ys.iter().cloned().fold(f32::MIN, f32::max).ceil().min(H as f32 - 1.0) as usize;
        let [a, b, c] = t.sxy;
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
        if area.abs() < 1e-6 {
            continue;
        }
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
                let light = w0 * t.light[0] + w1 * t.light[1] + w2 * t.light[2];
                let tex = match (&t.uv, &tim) {
                    (Some(uv), Some(tim)) => {
                        let u = w0 * uv[0][0] as f32 + w1 * uv[1][0] as f32 + w2 * uv[2][0] as f32;
                        let v = w0 * uv[0][1] as f32 + w1 * uv[1][1] as f32 + w2 * uv[2][1] as f32;
                        tim.sample(u.round().clamp(0.0, 255.0) as u8, v.round().clamp(0.0, 255.0) as u8)
                    }
                    _ => [1.0, 1.0, 1.0],
                };
                let out = &mut frame[y * W + x];
                for ch in 0..3 {
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
    img.save(&out_path).expect("cannot write PNG");
    println!(
        "{} tris drawn -> {} (yaw={yaw} pitch={pitch} dist={dist})",
        tris.len(),
        out_path.display()
    );
}
