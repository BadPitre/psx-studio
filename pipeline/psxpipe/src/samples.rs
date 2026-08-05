//! Sample asset generation: a textured unit cube (glTF + bin) and a
//! checkerboard texture. Used by `cargo run --example gen_samples` and by
//! the integration tests.

use std::path::Path;

/// Cube faces: (normal, u axis, v axis) with u x v = normal so that the
/// vertex order is CCW seen from outside (glTF front-face convention).
const FACES: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
    ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),   // +X
    ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),  // -X
    ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),   // +Y
    ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),  // -Y
    ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),   // +Z
    ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),  // -Z
];

/// Binary buffer for the cube: positions | normals | uvs | indices.
pub fn cube_bin() -> Vec<u8> {
    let mut positions: Vec<f32> = Vec::new();
    let mut normals: Vec<f32> = Vec::new();
    let mut uvs: Vec<f32> = Vec::new();
    let mut indices: Vec<u16> = Vec::new();

    for (fi, (n, u, v)) in FACES.iter().enumerate() {
        let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
        for (ci, (su, sv)) in corners.iter().enumerate() {
            for ch in 0..3 {
                positions.push(n[ch] + u[ch] * su + v[ch] * sv);
                normals.push(n[ch]);
            }
            let uv = [(ci == 1 || ci == 2) as u32 as f32, (ci >= 2) as u32 as f32];
            uvs.extend_from_slice(&uv);
        }
        let base = (fi * 4) as u16;
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    let mut bin = Vec::new();
    for f in positions.iter().chain(&normals).chain(&uvs) {
        bin.extend_from_slice(&f.to_le_bytes());
    }
    for i in &indices {
        bin.extend_from_slice(&i.to_le_bytes());
    }
    bin
}

/// glTF document referencing `bin_name` as an external buffer.
pub fn cube_gltf(bin_name: &str) -> String {
    const POS_LEN: usize = 24 * 12;
    const NRM_LEN: usize = 24 * 12;
    const UV_LEN: usize = 24 * 8;
    const IDX_LEN: usize = 36 * 2;
    let total = POS_LEN + NRM_LEN + UV_LEN + IDX_LEN;
    format!(
        r#"{{
  "asset": {{ "version": "2.0", "generator": "psxpipe sample generator" }},
  "scene": 0,
  "scenes": [ {{ "nodes": [0] }} ],
  "nodes": [ {{ "mesh": 0, "name": "Cube" }} ],
  "meshes": [ {{
    "name": "Cube",
    "primitives": [ {{
      "attributes": {{ "POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2 }},
      "indices": 3,
      "material": 0
    }} ]
  }} ],
  "materials": [ {{
    "name": "CubeMat",
    "pbrMetallicRoughness": {{ "baseColorFactor": [1.0, 1.0, 1.0, 1.0] }}
  }} ],
  "buffers": [ {{ "uri": "{bin_name}", "byteLength": {total} }} ],
  "bufferViews": [
    {{ "buffer": 0, "byteOffset": 0, "byteLength": {POS_LEN}, "target": 34962 }},
    {{ "buffer": 0, "byteOffset": {POS_LEN}, "byteLength": {NRM_LEN}, "target": 34962 }},
    {{ "buffer": 0, "byteOffset": {uv_off}, "byteLength": {UV_LEN}, "target": 34962 }},
    {{ "buffer": 0, "byteOffset": {idx_off}, "byteLength": {IDX_LEN}, "target": 34963 }}
  ],
  "accessors": [
    {{ "bufferView": 0, "componentType": 5126, "count": 24, "type": "VEC3",
       "min": [-1.0, -1.0, -1.0], "max": [1.0, 1.0, 1.0] }},
    {{ "bufferView": 1, "componentType": 5126, "count": 24, "type": "VEC3" }},
    {{ "bufferView": 2, "componentType": 5126, "count": 24, "type": "VEC2" }},
    {{ "bufferView": 3, "componentType": 5123, "count": 36, "type": "SCALAR" }}
  ]
}}
"#,
        uv_off = POS_LEN + NRM_LEN,
        idx_off = POS_LEN + NRM_LEN + UV_LEN,
    )
}

/// 256x256 four-color checkerboard with a thin grid, PS1-friendly.
pub fn checker_rgba(size: u32) -> Vec<u8> {
    let colors: [[u8; 3]; 4] = [
        [224, 96, 64],   // brick orange
        [64, 128, 224],  // blue
        [240, 224, 96],  // yellow
        [96, 192, 112],  // green
    ];
    let cell = size / 8;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let cx = x / cell;
            let cy = y / cell;
            let mut c = colors[((cx + cy * 3) % 4) as usize];
            // Darkened cell borders make the affine warping readable.
            if x % cell < 2 || y % cell < 2 {
                c = [c[0] / 2, c[1] / 2, c[2] / 2];
            }
            out.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    out
}

/// Write cube.gltf, cube.bin and checker.png into `dir`.
pub fn write_all(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("cube.bin"), cube_bin()).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("cube.gltf"), cube_gltf("cube.bin")).map_err(|e| e.to_string())?;

    let size = 256u32;
    let rgba = checker_rgba(size);
    let img: image::RgbaImage =
        image::ImageBuffer::from_raw(size, size, rgba).ok_or("checker buffer size mismatch")?;
    img.save(dir.join("checker.png")).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_buffer_layout() {
        let bin = cube_bin();
        assert_eq!(bin.len(), 24 * 12 + 24 * 12 + 24 * 8 + 36 * 2);
    }

    #[test]
    fn cube_positions_are_unit_cube() {
        let bin = cube_bin();
        for i in 0..24 * 3 {
            let f = f32::from_le_bytes(bin[i * 4..i * 4 + 4].try_into().unwrap());
            assert!(f == 1.0 || f == -1.0, "position component {f} not on cube");
        }
    }
}
