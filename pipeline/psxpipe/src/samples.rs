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

/// A triangle mesh ready to be serialized as glTF (positions/normals/uvs
/// interleaved by index, u16 indices).
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u16>,
}

impl MeshData {
    fn quad(&mut self, corners: [[f32; 3]; 4], normal: [f32; 3], uv: [[f32; 2]; 4]) {
        let base = self.positions.len() as u16;
        for k in 0..4 {
            self.positions.push(corners[k]);
            self.normals.push(normal);
            self.uvs.push(uv[k]);
        }
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn tri(&mut self, corners: [[f32; 3]; 3], normal: [f32; 3], uv: [[f32; 2]; 3]) {
        let base = self.positions.len() as u16;
        for k in 0..3 {
            self.positions.push(corners[k]);
            self.normals.push(normal);
            self.uvs.push(uv[k]);
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    /// Binary buffer: positions | normals | uvs | indices.
    pub fn to_bin(&self) -> Vec<u8> {
        let mut bin = Vec::new();
        for p in self.positions.iter().chain(&self.normals) {
            for c in p {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        for uv in &self.uvs {
            for c in uv {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        for i in &self.indices {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        bin
    }

    /// glTF document referencing `bin_name` as an external buffer.
    pub fn to_gltf(&self, name: &str, bin_name: &str) -> String {
        let vcount = self.positions.len();
        let pos_len = vcount * 12;
        let nrm_len = vcount * 12;
        let uv_len = vcount * 8;
        let idx_len = self.indices.len() * 2;
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for p in &self.positions {
            for c in 0..3 {
                min[c] = min[c].min(p[c]);
                max[c] = max[c].max(p[c]);
            }
        }
        format!(
            r#"{{
  "asset": {{ "version": "2.0", "generator": "psxpipe sample generator" }},
  "scene": 0,
  "scenes": [ {{ "nodes": [0] }} ],
  "nodes": [ {{ "mesh": 0, "name": "{name}" }} ],
  "meshes": [ {{
    "name": "{name}",
    "primitives": [ {{
      "attributes": {{ "POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2 }},
      "indices": 3,
      "material": 0
    }} ]
  }} ],
  "materials": [ {{
    "name": "{name}Mat",
    "pbrMetallicRoughness": {{ "baseColorFactor": [1.0, 1.0, 1.0, 1.0] }}
  }} ],
  "buffers": [ {{ "uri": "{bin_name}", "byteLength": {total} }} ],
  "bufferViews": [
    {{ "buffer": 0, "byteOffset": 0, "byteLength": {pos_len}, "target": 34962 }},
    {{ "buffer": 0, "byteOffset": {pos_len}, "byteLength": {nrm_len}, "target": 34962 }},
    {{ "buffer": 0, "byteOffset": {uv_off}, "byteLength": {uv_len}, "target": 34962 }},
    {{ "buffer": 0, "byteOffset": {idx_off}, "byteLength": {idx_len}, "target": 34963 }}
  ],
  "accessors": [
    {{ "bufferView": 0, "componentType": 5126, "count": {vcount}, "type": "VEC3",
       "min": [{}, {}, {}], "max": [{}, {}, {}] }},
    {{ "bufferView": 1, "componentType": 5126, "count": {vcount}, "type": "VEC3" }},
    {{ "bufferView": 2, "componentType": 5126, "count": {vcount}, "type": "VEC2" }},
    {{ "bufferView": 3, "componentType": 5123, "count": {icount}, "type": "SCALAR" }}
  ]
}}
"#,
            min[0], min[1], min[2], max[0], max[1], max[2],
            total = pos_len + nrm_len + uv_len + idx_len,
            uv_off = pos_len + nrm_len,
            idx_off = pos_len + nrm_len + uv_len,
            icount = self.indices.len(),
        )
    }
}

/// Low-poly house (16 triangles): walls, pyramid roof with overhang,
/// door and window baked in the texture atlas. A Blender-like test model.
pub fn house_mesh() -> MeshData {
    let mut m = MeshData {
        positions: Vec::new(),
        normals: Vec::new(),
        uvs: Vec::new(),
        indices: Vec::new(),
    };
    let (w, h, o, top) = (1.0f32, 1.5f32, 1.2f32, 2.5f32);

    // Texture atlas regions (glTF UV: v=0 is the top of the image):
    // walls: u 0..0.5, v 0..0.5 | roof: u 0.5..1, v 0..0.5 | under: u 0..0.5, v 0.5..1
    let wall = |left: f32, y: f32| [0.5 * left, 0.5 - y / h * 0.5];
    let wall_uv = [wall(0.0, 0.0), wall(1.0, 0.0), wall(1.0, h), wall(0.0, h)];
    let under_uv = [[0.05, 0.55], [0.45, 0.55], [0.45, 0.95], [0.05, 0.95]];
    let roof_uv = [[0.5, 0.5], [1.0, 0.5], [0.75, 0.0]];

    // Walls (CCW seen from outside).
    m.quad(
        [[-w, 0.0, w], [w, 0.0, w], [w, h, w], [-w, h, w]],
        [0.0, 0.0, 1.0],
        wall_uv,
    );
    m.quad(
        [[w, 0.0, -w], [-w, 0.0, -w], [-w, h, -w], [w, h, -w]],
        [0.0, 0.0, -1.0],
        wall_uv,
    );
    m.quad(
        [[w, 0.0, w], [w, 0.0, -w], [w, h, -w], [w, h, w]],
        [1.0, 0.0, 0.0],
        wall_uv,
    );
    m.quad(
        [[-w, 0.0, -w], [-w, 0.0, w], [-w, h, w], [-w, h, -w]],
        [-1.0, 0.0, 0.0],
        wall_uv,
    );

    // Floor (facing down).
    m.quad(
        [[-w, 0.0, -w], [w, 0.0, -w], [w, 0.0, w], [-w, 0.0, w]],
        [0.0, -1.0, 0.0],
        under_uv,
    );

    // Roof: four slopes to the apex. Slope normals: rise 1.0 over run 1.2.
    let apex = [0.0, top, 0.0];
    let rn = |x: f32, z: f32| {
        let len = (x * x + 1.44 + z * z).sqrt();
        [x / len, 1.2 / len, z / len]
    };
    m.tri([[-o, h, o], [o, h, o], apex], rn(0.0, 1.0), roof_uv);
    m.tri([[o, h, o], [o, h, -o], apex], rn(1.0, 0.0), roof_uv);
    m.tri([[o, h, -o], [-o, h, -o], apex], rn(0.0, -1.0), roof_uv);
    m.tri([[-o, h, -o], [-o, h, o], apex], rn(-1.0, 0.0), roof_uv);

    // Roof underside (overhang, facing down).
    m.quad(
        [[-o, h, -o], [o, h, -o], [o, h, o], [-o, h, o]],
        [0.0, -1.0, 0.0],
        under_uv,
    );

    m
}

/// 256x256 texture atlas for the house: walls with door and window
/// (top-left), roof tiles (top-right), plain underside (bottom-left).
pub fn house_rgba(size: u32) -> Vec<u8> {
    let half = size / 2;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let c: [u8; 3] = if y < half && x < half {
                // Wall region: beige with timber frame, door and window.
                let (u, v) = (x, y); // v=0 is the wall top
                if (52..76).contains(&u) && v >= 60 {
                    [88, 56, 36] // door
                } else if (88..112).contains(&u) && (56..84).contains(&v) {
                    [88, 128, 168] // window
                } else if v < 6 || u < 6 || u >= half - 6 {
                    [124, 92, 60] // timber edges
                } else {
                    [216, 196, 160] // plaster
                }
            } else if y < half {
                // Roof region: red tiles with darker rows.
                let row = y / 12;
                let shift = (row % 2) * 10;
                if y % 12 < 2 {
                    [112, 44, 36]
                } else if (x + shift) % 20 < 2 {
                    [136, 56, 44]
                } else {
                    [176, 80, 60]
                }
            } else if x < half {
                [120, 116, 112] // underside gray
            } else {
                [40, 40, 40] // unused quadrant
            };
            out.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    out
}

/// Write the sample source assets into `dir`:
/// cube.gltf/cube.bin + checker.png, house.gltf/house.bin + house.png.
pub fn write_all(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("cube.bin"), cube_bin()).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("cube.gltf"), cube_gltf("cube.bin")).map_err(|e| e.to_string())?;

    let house = house_mesh();
    std::fs::write(dir.join("house.bin"), house.to_bin()).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("house.gltf"), house.to_gltf("House", "house.bin"))
        .map_err(|e| e.to_string())?;

    let size = 256u32;
    for (name, rgba) in [("checker.png", checker_rgba(size)), ("house.png", house_rgba(size))] {
        let img: image::RgbaImage =
            image::ImageBuffer::from_raw(size, size, rgba).ok_or("buffer size mismatch")?;
        img.save(dir.join(name)).map_err(|e| e.to_string())?;
    }
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
