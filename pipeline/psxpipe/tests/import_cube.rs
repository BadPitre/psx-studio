//! End-to-end test: generated cube glTF -> PMD.

use psxpipe::{gltf_import, pmd, samples};

fn import_cube(opts: &gltf_import::ImportOptions) -> (pmd::Pmd, gltf_import::ImportReport) {
    let dir = tempfile::tempdir().unwrap();
    samples::write_all(dir.path()).unwrap();
    gltf_import::import(&dir.path().join("cube.gltf"), opts).unwrap()
}

#[test]
fn cube_imports_as_textured_gouraud() {
    let (pmd, report) = import_cube(&gltf_import::ImportOptions::default());

    // 12 triangles, all textured gouraud.
    assert_eq!(report.counts, [0, 0, 0, 12]);
    assert_eq!(report.degenerate_dropped, 0);
    // 24 corner vertices dedup to the 8 cube corners.
    assert_eq!(report.vertex_count, 8);
    // 6 face normals stay distinct.
    assert_eq!(report.normal_count, 6);
    assert!(pmd.textured);

    // Quantized corners must sit at +/- target_size.
    for v in &pmd.verts {
        for c in v {
            assert!(c.abs() == 128, "corner coordinate {c} != +/-128");
        }
    }
    // Normals are unit 4.12 axis vectors.
    for n in &pmd.normals {
        let mag: i32 = n.iter().map(|c| (*c as i32).abs()).sum();
        assert_eq!(mag, 4096);
    }
    // Scale metadata: 1 source unit -> 128 PMD units => scale = 4096/128 = 32.
    assert_eq!(report.scale_4_12, 32);
}

#[test]
fn cube_flat_untextured() {
    let opts = gltf_import::ImportOptions {
        flat: true,
        untextured: true,
        ..Default::default()
    };
    let (pmd, report) = import_cube(&opts);
    assert_eq!(report.counts, [12, 0, 0, 0]);
    assert!(!pmd.textured);
    // White base color from the sample material.
    assert_eq!(pmd.f3[0].color, [255, 255, 255]);
}

#[test]
fn cube_pmd_serializes_and_parses() {
    let (pmd, _) = import_cube(&gltf_import::ImportOptions::default());
    let bytes = pmd.write().unwrap();
    let h = pmd::parse_header(&bytes).unwrap();
    assert_eq!(h.vertex_count, 8);
    assert_eq!(h.prim_counts, [0, 0, 0, 12]);
    // GT3 section fills the rest of the file: 12 records of 52 bytes.
    assert_eq!(bytes.len() as u32, h.offsets[5] + 12 * 52);
    // All offsets 4-byte aligned.
    for off in h.offsets {
        assert_eq!(off % 4, 0);
    }
}

#[test]
fn house_imports_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    samples::write_all(dir.path()).unwrap();
    let (pmd, report) =
        gltf_import::import(&dir.path().join("house.gltf"), &gltf_import::ImportOptions::default())
            .unwrap();
    // 16 textured gouraud triangles, no degenerates, no warnings.
    assert_eq!(report.counts, [0, 0, 0, 16]);
    assert_eq!(report.degenerate_dropped, 0);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert!(pmd.textured);
    // 36 authored corners dedup to the 13 unique house vertices.
    assert_eq!(report.vertex_count, 13);
}
