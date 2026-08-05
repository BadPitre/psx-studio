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
            // Bordures légèrement assombries : lisibles de près, sans
            // créer de lignes noires d'aliasing au loin (pas de mipmaps
            // sur PS1, le nearest-sampling ressort les rangées sombres).
            if x % cell < 2 || y % cell < 2 {
                c = [
                    c[0] - c[0] / 4,
                    c[1] - c[1] / 4,
                    c[2] - c[2] / 4,
                ];
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

    /// glTF document referencing `bin_name`, with a baseColor texture
    /// pointing at `image_uri` (like a Blender export with texture).
    pub fn to_gltf_textured(&self, name: &str, bin_name: &str, image_uri: &str) -> String {
        let base = self.to_gltf(name, bin_name);
        // Injecte image/sampler/texture et référence la texture dans le
        // matériau (le JSON généré par to_gltf est stable).
        base.replace(
            "\"pbrMetallicRoughness\": { \"baseColorFactor\": [1.0, 1.0, 1.0, 1.0] }",
            "\"pbrMetallicRoughness\": { \"baseColorTexture\": { \"index\": 0 } }",
        )
        .replace(
            "\"buffers\":",
            &format!(
                "\"images\": [ {{ \"uri\": \"{image_uri}\" }} ],\n  \"samplers\": [ {{}} ],\n  \"textures\": [ {{ \"source\": 0, \"sampler\": 0 }} ],\n  \"buffers\":"
            ),
        )
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

    let ground = plane_mesh(12);
    std::fs::write(dir.join("ground.bin"), ground.to_bin()).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("ground.gltf"), ground.to_gltf("Ground", "ground.bin"))
        .map_err(|e| e.to_string())?;

    let guy = guy_mesh();
    std::fs::write(dir.join("guy.bin"), guy.to_bin()).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("guy.gltf"), guy.to_gltf("Guy", "guy.bin"))
        .map_err(|e| e.to_string())?;

    let size = 256u32;
    for (name, rgba) in [("checker.png", checker_rgba(size)), ("house.png", house_rgba(size))] {
        let img: image::RgbaImage =
            image::ImageBuffer::from_raw(size, size, rgba).ok_or("buffer size mismatch")?;
        img.save(dir.join(name)).map_err(|e| e.to_string())?;
    }
    let img: image::RgbaImage =
        image::ImageBuffer::from_raw(64, 64, guy_rgba(64)).ok_or("buffer size mismatch")?;
    img.save(dir.join("guy.png")).map_err(|e| e.to_string())?;
    Ok(())
}

impl MeshData {
    /// Boîte axis-alignée : 6 quads CCW vers l'extérieur, tous mappés sur
    /// le même rectangle UV (zones de couleur d'un atlas).
    fn box_at(&mut self, center: [f32; 3], half: [f32; 3], uv: [[f32; 2]; 2]) {
        let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
            ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
            ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
        ];
        let ([u0, v0], [u1, v1]) = (uv[0], uv[1]);
        for (n, u, v) in faces {
            let corner = |su: f32, sv: f32| {
                [
                    center[0] + (n[0] + u[0] * su + v[0] * sv) * half[0],
                    center[1] + (n[1] + u[1] * su + v[1] * sv) * half[1],
                    center[2] + (n[2] + u[2] * su + v[2] * sv) * half[2],
                ]
            };
            self.quad(
                [corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)],
                n,
                [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
            );
        }
    }
}

/// Personnage low-poly (4 boîtes, 48 tris) : jambes, torse, tête.
/// Atlas 64×64 : peau (quart haut-gauche), chemise (haut-droit),
/// pantalon (bas-gauche).
pub fn guy_mesh() -> MeshData {
    let mut m = MeshData {
        positions: Vec::new(),
        normals: Vec::new(),
        uvs: Vec::new(),
        indices: Vec::new(),
    };
    let skin = [[0.02, 0.02], [0.48, 0.48]];
    let shirt = [[0.52, 0.02], [0.98, 0.48]];
    let pants = [[0.02, 0.52], [0.48, 0.98]];

    m.box_at([-0.14, 0.22, 0.0], [0.11, 0.22, 0.13], pants); // jambe G
    m.box_at([0.14, 0.22, 0.0], [0.11, 0.22, 0.13], pants); //  jambe D
    m.box_at([0.0, 0.7, 0.0], [0.3, 0.27, 0.17], shirt); //    torse
    m.box_at([0.0, 1.12, 0.0], [0.17, 0.19, 0.17], skin); //   tête
    m
}

/// Atlas 64×64 du personnage : peau + cheveux, chemise, pantalon.
pub fn guy_rgba(size: u32) -> Vec<u8> {
    let half = size / 2;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let c: [u8; 3] = if y < half && x < half {
                // Peau, avec une bande cheveux en haut du quart.
                if y < half / 4 { [72, 48, 32] } else { [222, 178, 138] }
            } else if y < half {
                // Chemise à liseré.
                if y % 12 < 2 { [40, 84, 140] } else { [58, 112, 180] }
            } else if x < half {
                [52, 48, 66] // pantalon
            } else {
                [40, 40, 40]
            };
            out.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    out
}

/// Flat ground plane subdivided into an NxN grid, one full texture repeat
/// per cell. Small cells are how PS1 games did large grounds: each cell
/// stays under the GPU primitive size limit (1023x511), depth-sorts
/// locally in the OT, and tiles the texture instead of stretching it.
pub fn plane_mesh(cells: u32) -> MeshData {
    let mut m = MeshData {
        positions: Vec::new(),
        normals: Vec::new(),
        uvs: Vec::new(),
        indices: Vec::new(),
    };
    let step = 2.0 / cells as f32;
    for cz in 0..cells {
        for cx in 0..cells {
            let x0 = -1.0 + cx as f32 * step;
            let z0 = -1.0 + cz as f32 * step;
            let (x1, z1) = (x0 + step, z0 + step);
            // CCW seen from +Y (up in glTF space).
            m.quad(
                [[x0, 0.0, z0], [x0, 0.0, z1], [x1, 0.0, z1], [x1, 0.0, z0]],
                [0.0, 1.0, 0.0],
                [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
            );
        }
    }
    m
}

/// Demo scene "village": textured ground slab, three houses, a chimney
/// parented to the first house (exercises hierarchy + per-axis scale).
pub fn scene_village_json() -> &'static str {
    r#"{
  "name": "village",
  "settings": {
    "background":  [24, 32, 56],
    "ambient":     [72, 72, 80],
    "light_dir":   [0.5, 1.0, 0.35],
    "light_color": [255, 245, 225]
  },
  "assets": {
    "textures": [
      { "id": "checker_tex", "tim": "checker.tim" },
      { "id": "house_tex",   "tim": "house.tim" },
      { "id": "guy_tex",     "tim": "guy.tim" }
    ],
    "models": [
      { "id": "cube",   "pmd": "cube.pmd",   "texture": "checker_tex" },
      { "id": "ground", "pmd": "ground.pmd", "texture": "checker_tex" },
      { "id": "house",  "pmd": "house.pmd",  "texture": "house_tex" },
      { "id": "guy",    "pmd": "guy.pmd",    "texture": "guy_tex" }
    ]
  },
  "entities": [
    { "name": "sol",     "position": [0, 0, 150],    "scale": [4.0, 1.0, 4.0], "model": "ground" },
    { "name": "maison1", "position": [-160, 0, 120], "rotation": [0, 35, 0],  "model": "house" },
    { "name": "cheminee", "parent": "maison1", "position": [40, -105, 25], "scale": [0.14, 0.3, 0.14], "model": "cube" },
    { "name": "maison2", "position": [170, 0, 190],  "rotation": [0, -30, 0], "model": "house" },
    { "name": "maison3", "position": [10, 0, 330],   "rotation": [0, 180, 0], "scale": [1.3, 1.3, 1.3], "model": "house" },
    { "name": "perso",   "position": [0, 0, -60],    "scale": [0.5, 0.5, 0.5], "model": "guy", "script": "player" },
    { "name": "pnj",     "position": [60, 0, 290],   "scale": [0.5, 0.5, 0.5], "model": "guy", "script": "npc" },
    { "name": "lune",    "position": [300, -260, 500], "rotation": [35, -120, 0], "light": { "color": [70, 90, 160] } },
    { "name": "torche",  "position": [265, -36, 150], "scale": [0.05, 0.28, 0.05], "model": "cube",
      "light": { "type": "point", "color": [255, 150, 60], "intensity": 1.6, "radius": 520 }, "script": "torche" },
    { "name": "camera",  "position": [0, -200, -420], "rotation": [-21, 180, 0], "camera": true }
  ]
}
"#
}

/// Demo scene "champ de cubes": a grid of spinningly-arranged cubes on a
/// dark slab. Different lighting/background to make the switch obvious.
pub fn scene_field_json() -> &'static str {
    r#"{
  "name": "champ-de-cubes",
  "settings": {
    "background":  [8, 8, 20],
    "ambient":     [40, 40, 64],
    "light_dir":   [-0.6, 1.0, 0.2],
    "light_color": [200, 210, 255]
  },
  "assets": {
    "textures": [
      { "id": "checker_tex", "tim": "checker.tim" },
      { "id": "house_tex",   "tim": "house.tim" },
      { "id": "guy_tex",     "tim": "guy.tim" }
    ],
    "models": [
      { "id": "cube",   "pmd": "cube.pmd",   "texture": "checker_tex" },
      { "id": "ground", "pmd": "ground.pmd", "texture": "checker_tex" },
      { "id": "house",  "pmd": "house.pmd",  "texture": "house_tex" },
      { "id": "guy",    "pmd": "guy.pmd",    "texture": "guy_tex" }
    ]
  },
  "entities": [
    { "name": "sol",    "position": [0, 0, 200],     "scale": [4.5, 1.0, 4.5], "model": "ground" },
    { "name": "perso",  "position": [0, 0, -100],    "scale": [0.5, 0.5, 0.5], "model": "guy", "script": "player" },
    { "name": "camera", "position": [0, -200, -460], "rotation": [-21, 180, 0], "camera": true },
    { "name": "cube11", "position": [-200, -40, 80],  "rotation": [0, 15, 0], "scale": [0.3, 0.3, 0.3], "model": "cube" },
    { "name": "cube12", "position": [0, -40, 80],     "rotation": [0, 30, 0], "scale": [0.3, 0.3, 0.3], "model": "cube" },
    { "name": "cube13", "position": [200, -40, 80],   "rotation": [0, 45, 0], "scale": [0.3, 0.3, 0.3], "model": "cube" },
    { "name": "cube21", "position": [-200, -40, 280], "rotation": [0, 60, 0], "scale": [0.3, 0.3, 0.3], "model": "cube" },
    { "name": "phare",  "position": [0, 0, 280],      "rotation": [0, 90, 0], "scale": [0.8, 1.6, 0.8], "model": "house" },
    { "name": "cube23", "position": [200, -40, 280],  "rotation": [0, 75, 0], "scale": [0.3, 0.3, 0.3], "model": "cube" },
    { "name": "cube31", "position": [-200, -40, 480], "rotation": [0, 10, 0], "scale": [0.3, 0.3, 0.3], "model": "cube" },
    { "name": "cube32", "position": [0, -40, 480],    "rotation": [0, 25, 0], "scale": [0.3, 0.3, 0.3], "model": "cube" },
    { "name": "cube33", "position": [200, -40, 480],  "rotation": [0, 40, 0], "scale": [0.3, 0.3, 0.3], "model": "cube" }
  ]
}
"#
}

/// Demo SFX: a short "coin" blip (mono, 22050 Hz).
pub fn sfx_pcm() -> Vec<i16> {
    let rate = 22050.0f32;
    let len = (rate * 0.18) as usize;
    (0..len)
        .map(|i| {
            let t = i as f32 / rate;
            // Rising sweep with an exponential decay.
            let freq = 660.0 + 1400.0 * (t / 0.18);
            let env = (1.0 - t / 0.18).powf(1.5);
            ((t * freq * std::f32::consts::TAU).sin() * env * 14000.0) as i16
        })
        .collect()
}

/// Demo music: an 8-second chiptune loop (stereo, 44100 Hz) — square bass
/// plus a triangle-wave arpeggio, CD-DA friendly.
pub fn music_pcm() -> Vec<(i16, i16)> {
    let rate = 44100.0f32;
    let chords: [(f32, [f32; 3]); 4] = [
        (110.00, [220.00, 261.63, 329.63]), // Am
        (87.31, [174.61, 220.00, 261.63]),  // F
        (130.81, [261.63, 329.63, 392.00]), // C
        (98.00, [196.00, 246.94, 293.66]),  // G
    ];
    let chord_len = (rate * 2.0) as usize;
    let arp_len = chord_len / 8;
    let mut out = Vec::with_capacity(chord_len * 4);
    for (bass, arp) in &chords {
        for i in 0..chord_len {
            let t = i as f32 / rate;
            // Square bass.
            let b = if (t * bass).fract() < 0.5 { 1.0 } else { -1.0 };
            // Triangle arpeggio, 8 notes per chord, soft attack/decay.
            let step = i / arp_len;
            let note = arp[step % 3] * if step % 8 >= 6 { 2.0 } else { 1.0 };
            let phase = (t * note).fract();
            let tri = 4.0 * (phase - 0.5).abs() - 1.0;
            let pos = (i % arp_len) as f32 / arp_len as f32;
            let env = (pos * 8.0).min(1.0) * (1.0 - pos * 0.6);
            let bass_s = b * 4500.0;
            let mel_s = tri * env * 9000.0;
            // Slight stereo spread: bass left-ish, melody right-ish.
            let l = (bass_s * 0.9 + mel_s * 0.6) as i16;
            let r = (bass_s * 0.6 + mel_s * 0.9) as i16;
            out.push((l, r));
        }
    }
    out
}

/// Write sfx.wav (mono 22050) and music.wav (stereo 44100) into `dir`.
pub fn write_audio(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 22050,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w =
        hound::WavWriter::create(dir.join("sfx.wav"), spec).map_err(|e| e.to_string())?;
    for s in sfx_pcm() {
        w.write_sample(s).map_err(|e| e.to_string())?;
    }
    w.finalize().map_err(|e| e.to_string())?;

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w =
        hound::WavWriter::create(dir.join("music.wav"), spec).map_err(|e| e.to_string())?;
    for (l, r) in music_pcm() {
        w.write_sample(l).map_err(|e| e.to_string())?;
        w.write_sample(r).map_err(|e| e.to_string())?;
    }
    w.finalize().map_err(|e| e.to_string())?;
    Ok(())
}

/// Convert the sample sources into console assets (PMD + TIM) inside `dir`,
/// with a VRAM layout where both textures coexist:
/// checker at (320,0) CLUT (320,256), house at (448,0) CLUT (320,257).
pub fn build_demo_assets(dir: &Path) -> Result<(), String> {
    use crate::{gltf_import, tim};

    write_all(dir)?;

    for (gltf_name, pmd_name) in [
        ("cube.gltf", "cube.pmd"),
        ("house.gltf", "house.pmd"),
        ("ground.gltf", "ground.pmd"),
    ] {
        let (pmd, _) = gltf_import::import(&dir.join(gltf_name), &Default::default())?;
        std::fs::write(dir.join(pmd_name), pmd.write()?).map_err(|e| e.to_string())?;
    }
    // Le personnage : UVs mappées sur son atlas 64×64.
    let guy_opts = gltf_import::ImportOptions {
        tex_w: 64,
        tex_h: 64,
        ..Default::default()
    };
    let (guy_pmd, _) = gltf_import::import(&dir.join("guy.gltf"), &guy_opts)?;
    std::fs::write(dir.join("guy.pmd"), guy_pmd.write()?).map_err(|e| e.to_string())?;

    let layouts = [
        ("checker.png", "checker.tim", 320u16, 0u16, 320u16, 256u16),
        ("house.png", "house.tim", 448, 0, 320, 257),
        ("guy.png", "guy.tim", 576, 0, 320, 258),
    ];
    for (png, tim_name, org_x, org_y, clut_x, clut_y) in layouts {
        let img = image::open(dir.join(png))
            .map_err(|e| format!("{png}: {e}"))?
            .to_rgba8();
        let (w, h) = img.dimensions();
        let opts = tim::TimOptions {
            bpp: tim::Bpp::Eight,
            org_x,
            org_y,
            clut_x,
            clut_y,
        };
        let (timg, _) = tim::encode(img.as_raw(), w, h, &opts)?;
        std::fs::write(dir.join(tim_name), timg.write()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Build the two demo .psc scenes into `out_dir`, using (and generating)
/// assets in `samples_dir`. Returns the scene reports.
pub fn build_demo_scenes(
    samples_dir: &Path,
    out_dir: &Path,
) -> Result<Vec<crate::scene::SceneReport>, String> {
    build_demo_assets(samples_dir)?;
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;

    let mut reports = Vec::new();
    for (json, json_name, psc_name) in [
        (scene_village_json(), "scene0.json", "scene0.psc"),
        (scene_field_json(), "scene1.json", "scene1.psc"),
    ] {
        let json_path = samples_dir.join(json_name);
        std::fs::write(&json_path, json).map_err(|e| e.to_string())?;
        let (bytes, report) = crate::scene::build_file(&json_path)?;
        std::fs::write(out_dir.join(psc_name), &bytes).map_err(|e| e.to_string())?;
        reports.push(report);
    }

    /* Demo SFX for the player (Square button). */
    write_audio(samples_dir)?;
    let (vag, _) = crate::vag::wav_to_vag(&samples_dir.join("sfx.wav"), "blip")?;
    std::fs::write(out_dir.join("BLIP.VAG"), &vag).map_err(|e| e.to_string())?;

    Ok(reports)
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
