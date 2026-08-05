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
    assert_eq!(h.model_count, 4);
    assert_eq!(h.texture_count, 3);
    assert_eq!(h.entity_count, 9);
    assert_eq!(h.total_size as usize, bytes.len());
    assert_eq!(h.background, [24, 32, 56]);

    // Light vector points toward the source (negated, normalized 4.12).
    assert!(h.light_toward[1] < -2000, "light Y should point up (negative)");

    // Tables are contiguous and 4-aligned.
    assert_eq!(h.models_offset, 64);
    assert_eq!(h.textures_offset, 64 + 4 * 12);
    assert_eq!(h.entities_offset, 64 + 4 * 12 + 3 * 8);
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
fn vram_overlap_rejected_without_packing() {
    let dir = tempfile::tempdir().unwrap();
    samples::build_demo_assets(dir.path()).unwrap();
    // Both TIMs baked at the SAME placement: duplicate checker.tim as
    // house.tim so they collide when auto-packing is disabled.
    std::fs::copy(dir.path().join("checker.tim"), dir.path().join("house.tim")).unwrap();
    let json_path = dir.path().join("scene0.json");
    std::fs::write(&json_path, samples::scene_village_json()).unwrap();
    let opts = scene::BuildOptions { pack_vram: false };
    let err = scene::build_file_with_options(&json_path, &opts).unwrap_err();
    assert!(err.contains("VRAM overlap"), "unexpected error: {err}");
}

#[test]
fn auto_packing_repairs_colliding_placements() {
    let dir = tempfile::tempdir().unwrap();
    samples::build_demo_assets(dir.path()).unwrap();
    // Same colliding input as above, but the default build repacks: the
    // scene builds fine and the two textures get distinct placements.
    std::fs::copy(dir.path().join("checker.tim"), dir.path().join("house.tim")).unwrap();
    let json_path = dir.path().join("scene0.json");
    std::fs::write(&json_path, samples::scene_village_json()).unwrap();
    let (_, report) = scene::build_file(&json_path).unwrap();
    assert_eq!(report.vram.len(), 3);
    let (a, b) = (&report.vram[0].1, &report.vram[1].1);
    assert_ne!((a.x, a.y), (b.x, b.y));
    assert_eq!(a.x % 64, 0);
    assert_eq!(b.x % 64, 0);
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

#[test]
fn scripts_table_and_entity_refs() {
    let dir = tempfile::tempdir().unwrap();
    samples::build_demo_assets(dir.path()).unwrap();
    let json = r#"{
      "name": "scripted",
      "assets": { "textures": [], "models": [{ "id": "cube", "pmd": "cube.pmd" }] },
      "entities": [
        { "name": "perso", "model": "cube", "script": "player" },
        { "name": "decor", "model": "cube" },
        { "name": "pnj",   "model": "cube", "script": "npc" },
        { "name": "pnj2",  "model": "cube", "script": "npc" }
      ]
    }"#;
    let json_path = dir.path().join("s.json");
    std::fs::write(&json_path, json).unwrap();
    let (bytes, _) = scene::build_file(&json_path).unwrap();
    let h = scene::parse_header(&bytes).unwrap();

    // Table: 2 scripts uniques (player, npc), hashes attendus.
    assert_eq!(h.script_count, 2);
    let hash_at = |i: usize| {
        u32::from_le_bytes(
            bytes[h.scripts_offset as usize + i * 4..h.scripts_offset as usize + i * 4 + 4]
                .try_into()
                .unwrap(),
        )
    };
    assert_eq!(hash_at(0), scene::script_hash("player"));
    assert_eq!(hash_at(1), scene::script_hash("npc"));

    // Champ script par entité : indice+1, 0 = aucun.
    let script_ref = |i: usize| {
        let rec = h.entities_offset as usize + i * scene::ENTITY_SIZE;
        u16::from_le_bytes([bytes[rec + 0x1E], bytes[rec + 0x1F]])
    };
    assert_eq!(script_ref(0), 1); // player
    assert_eq!(script_ref(1), 0); // aucun
    assert_eq!(script_ref(2), 2); // npc
    assert_eq!(script_ref(3), 2); // npc (partage)

    // Le village de démo embarque les scripts player et npc.
    let (bytes2, _) = scene::build_file(&{
        let p = dir.path().join("s2.json");
        std::fs::write(&p, samples::scene_village_json()).unwrap();
        p
    })
    .unwrap();
    assert_eq!(scene::parse_header(&bytes2).unwrap().script_count, 2);
}

#[test]
fn lights_table_and_camera_flags() {
    let bytes = build_village();
    let h = scene::parse_header(&bytes).unwrap();

    // La lune est la seule entité-lumière (la lumière des settings ne
    // compte pas dans la table), la caméra est flaguée.
    assert_eq!(h.light_count, 1);
    let lights = scene::parse_lights(&bytes, &h);
    assert_eq!(lights.len(), 1);
    assert_eq!(lights[0].color, [70, 90, 160]);

    let flags = |i: usize| {
        let rec = h.entities_offset as usize + i * scene::ENTITY_SIZE;
        u16::from_le_bytes([bytes[rec + 0x1C], bytes[rec + 0x1D]])
    };
    // Ordre topo stable : lune = 7, camera = 8.
    assert_eq!(lights[0].entity, 7);
    assert_eq!(flags(7), scene::ENTITY_FLAG_LIGHT);
    assert_eq!(flags(8), scene::ENTITY_FLAG_CAMERA);
    for i in 0..7 {
        assert_eq!(flags(i), 0, "entité {i} sans composant");
    }

    // Plus de 2 lumières : les extras sont ignorés avec un warning.
    let dir = tempfile::tempdir().unwrap();
    samples::build_demo_assets(dir.path()).unwrap();
    let json = r#"{
      "name": "trop",
      "assets": { "textures": [], "models": [] },
      "entities": [
        { "name": "l1", "light": { "color": [255, 0, 0] } },
        { "name": "l2", "light": { "color": [0, 255, 0] } },
        { "name": "l3", "light": { "color": [0, 0, 255] } }
      ]
    }"#;
    let p = dir.path().join("trop.json");
    std::fs::write(&p, json).unwrap();
    let (bytes3, report) = scene::build_file(&p).unwrap();
    let h3 = scene::parse_header(&bytes3).unwrap();
    assert_eq!(h3.light_count, 2);
    assert!(report.warnings.iter().any(|w| w.contains("l3")), "{:?}", report.warnings);
}

#[test]
fn camera_fov_in_entity_pad() {
    let dir = tempfile::tempdir().unwrap();
    samples::build_demo_assets(dir.path()).unwrap();
    let json = r#"{
      "name": "cams",
      "assets": { "textures": [], "models": [] },
      "entities": [
        { "name": "large", "camera": { "fov": 96 } },
        { "name": "simple", "camera": true },
        { "name": "rien" }
      ]
    }"#;
    let p = dir.path().join("cams.json");
    std::fs::write(&p, json).unwrap();
    let (bytes, _) = scene::build_file(&p).unwrap();
    let h = scene::parse_header(&bytes).unwrap();

    let fov_at = |i: usize| {
        let rec = h.entities_offset as usize + i * scene::ENTITY_SIZE;
        u16::from_le_bytes([bytes[rec + 6], bytes[rec + 7]])
    };
    let flags_at = |i: usize| {
        let rec = h.entities_offset as usize + i * scene::ENTITY_SIZE;
        u16::from_le_bytes([bytes[rec + 0x1C], bytes[rec + 0x1D]])
    };
    assert_eq!(fov_at(0), 96);
    assert_eq!(flags_at(0), scene::ENTITY_FLAG_CAMERA);
    assert_eq!(fov_at(1), 0); // défaut (h = 160, ~74°)
    assert_eq!(flags_at(1), scene::ENTITY_FLAG_CAMERA);
    assert_eq!(fov_at(2), 0);
    assert_eq!(flags_at(2), 0);

    // FOV hors plage : erreur claire.
    let bad = json.replace("96", "500");
    std::fs::write(&p, bad).unwrap();
    assert!(scene::build_file(&p).unwrap_err().contains("fov"));
}

#[test]
fn light_intensity_and_camera_draw_distance() {
    let dir = tempfile::tempdir().unwrap();
    samples::build_demo_assets(dir.path()).unwrap();
    let json = r#"{
      "name": "params",
      "assets": { "textures": [], "models": [] },
      "entities": [
        { "name": "forte", "light": { "color": [255, 0, 0], "intensity": 1.8 } },
        { "name": "cam",   "camera": { "fov": 74, "draw_distance": 1500 } }
      ]
    }"#;
    let p = dir.path().join("params.json");
    std::fs::write(&p, json).unwrap();
    let (bytes, _) = scene::build_file(&p).unwrap();
    let h = scene::parse_header(&bytes).unwrap();

    let lights = scene::parse_lights(&bytes, &h);
    assert_eq!(lights[0].intensity_percent, 180);

    // draw_distance dans le pad du vecteur rotation (offset 0x0E).
    let rec = h.entities_offset as usize + scene::ENTITY_SIZE; // entité 1
    assert_eq!(u16::from_le_bytes([bytes[rec + 0x0E], bytes[rec + 0x0F]]), 1500);
    assert_eq!(u16::from_le_bytes([bytes[rec + 6], bytes[rec + 7]]), 74);

    // Hors plage : erreurs claires.
    std::fs::write(&p, json.replace("1.8", "9.0")).unwrap();
    assert!(scene::build_file(&p).unwrap_err().contains("intensity"));
    std::fs::write(&p, json.replace("1500", "50")).unwrap();
    assert!(scene::build_file(&p).unwrap_err().contains("draw_distance"));
}
