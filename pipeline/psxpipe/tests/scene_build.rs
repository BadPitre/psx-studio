//! End-to-end tests: demo scenes JSON -> .psc.

use psxpipe::{samples, scene};

fn build_village() -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    samples::build_demo_assets(dir.path()).unwrap();
    let json_path = dir.path().join("scene0.json");
    std::fs::write(&json_path, samples::scene_village_json()).unwrap();
    scene::build_file(&json_path).unwrap().0
}

#[test]
fn village_builds_and_parses() {
    let bytes = build_village();
    let h = scene::parse_header(&bytes).unwrap();
    assert_eq!(h.model_count, 2);
    assert_eq!(h.texture_count, 2);
    assert_eq!(h.entity_count, 5);
    assert_eq!(h.total_size as usize, bytes.len());
    assert_eq!(h.background, [24, 32, 56]);

    // Light vector points toward the source (negated, normalized 4.12).
    assert!(h.light_toward[1] < -2000, "light Y should point up (negative)");

    // Tables are contiguous and 4-aligned.
    assert_eq!(h.models_offset, 64);
    assert_eq!(h.textures_offset, 64 + 2 * 12);
    assert_eq!(h.entities_offset, 64 + 2 * 12 + 2 * 8);
    for off in [h.models_offset, h.textures_offset, h.entities_offset] {
        assert_eq!(off % 4, 0);
    }
}

#[test]
fn village_entities_are_topo_sorted() {
    let bytes = build_village();
    let h = scene::parse_header(&bytes).unwrap();
    let base = h.entities_offset as usize;
    for i in 0..h.entity_count as usize {
        let rec = base + i * scene::ENTITY_SIZE;
        let parent = u16::from_le_bytes([bytes[rec + 0x18], bytes[rec + 0x19]]);
        let model = u16::from_le_bytes([bytes[rec + 0x1A], bytes[rec + 0x1B]]);
        if parent != scene::NO_INDEX {
            assert!((parent as usize) < i, "parent {parent} not before child {i}");
        }
        assert!(model == scene::NO_INDEX || model < h.model_count);
    }
}

#[test]
fn village_embeds_valid_blobs() {
    let bytes = build_village();
    let h = scene::parse_header(&bytes).unwrap();
    // Every model blob is a parseable PMD, every texture blob a TIM.
    for i in 0..h.model_count as usize {
        let rec = h.models_offset as usize + i * scene::MODEL_ENTRY_SIZE;
        let off = u32::from_le_bytes(bytes[rec..rec + 4].try_into().unwrap()) as usize;
        let size = u32::from_le_bytes(bytes[rec + 4..rec + 8].try_into().unwrap()) as usize;
        assert_eq!(off % 4, 0);
        psxpipe::pmd::parse_header(&bytes[off..off + size]).unwrap();
        let tex = u16::from_le_bytes([bytes[rec + 8], bytes[rec + 9]]);
        assert!(tex < h.texture_count);
    }
    for i in 0..h.texture_count as usize {
        let rec = h.textures_offset as usize + i * scene::TEXTURE_ENTRY_SIZE;
        let off = u32::from_le_bytes(bytes[rec..rec + 4].try_into().unwrap()) as usize;
        assert_eq!(off % 4, 0);
        assert_eq!(bytes[off], 0x10, "texture blob {i} is not a TIM");
    }
}

#[test]
fn vram_overlap_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    samples::build_demo_assets(dir.path()).unwrap();
    // Both models on the SAME texture placement: duplicate checker.tim as
    // house.tim so the two TIMs collide in VRAM.
    std::fs::copy(dir.path().join("checker.tim"), dir.path().join("house.tim")).unwrap();
    let json_path = dir.path().join("scene0.json");
    std::fs::write(&json_path, samples::scene_village_json()).unwrap();
    let err = scene::build_file(&json_path).unwrap_err();
    assert!(err.contains("VRAM overlap"), "unexpected error: {err}");
}

#[test]
fn parent_cycle_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    samples::build_demo_assets(dir.path()).unwrap();
    let json = r#"{
      "name": "cycle",
      "assets": { "textures": [], "models": [{ "id": "cube", "pmd": "cube.pmd" }] },
      "entities": [
        { "name": "a", "parent": "b" },
        { "name": "b", "parent": "a" }
      ]
    }"#;
    let json_path = dir.path().join("bad.json");
    std::fs::write(&json_path, json).unwrap();
    let err = scene::build_file(&json_path).unwrap_err();
    assert!(err.contains("cycle"), "unexpected error: {err}");
}
