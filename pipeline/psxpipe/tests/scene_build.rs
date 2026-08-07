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
    assert_eq!(h.entity_count, 30);
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
    let opts = scene::BuildOptions {
        pack_vram: false,
        ..Default::default()
    };
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
    // 3 textures + l'atlas de la police UI (v1.3).
    assert_eq!(report.vram.len(), 4);
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

    // Le village de démo embarque player/npc, les scripts UI (hud,
    // dialogue, pause) et le PSX Script spinner.
    let (bytes2, _) = scene::build_file(&{
        let p = dir.path().join("s2.json");
        std::fs::write(&p, samples::scene_village_json()).unwrap();
        p
    })
    .unwrap();
    assert_eq!(scene::parse_header(&bytes2).unwrap().script_count, 7);
}

#[test]
fn lights_table_and_camera_flags() {
    let bytes = build_village();
    let h = scene::parse_header(&bytes).unwrap();

    // Lune (directionnelle) + torche (ponctuelle) dans la table ; la
    // caméra est flaguée.
    assert_eq!(h.light_count, 2);
    let lights = scene::parse_lights(&bytes, &h);
    assert_eq!(lights.len(), 2);
    assert_eq!(lights[0].color, [70, 90, 160]);
    assert_eq!(lights[1].color, [255, 150, 60]);
    assert_eq!(lights[1].intensity_percent, 160);

    let flags = |i: usize| {
        let rec = h.entities_offset as usize + i * scene::ENTITY_SIZE;
        u16::from_le_bytes([bytes[rec + 0x1C], bytes[rec + 0x1D]])
    };
    // Ordre topo stable : lune = 7, torche = 8, camera = 9.
    assert_eq!(lights[0].entity, 7);
    assert_eq!(lights[1].entity, 8);
    assert_eq!(flags(7), scene::ENTITY_FLAG_LIGHT);
    assert_eq!(
        flags(8),
        scene::ENTITY_FLAG_LIGHT | scene::ENTITY_FLAG_LIGHT_POINT
    );
    assert_eq!(flags(9), scene::ENTITY_FLAG_CAMERA);
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

#[test]
fn point_lights_flag_and_radius() {
    let dir = tempfile::tempdir().unwrap();
    samples::build_demo_assets(dir.path()).unwrap();
    let json = r#"{
      "name": "torches",
      "assets": { "textures": [], "models": [] },
      "entities": [
        { "name": "soleil2", "light": { "color": [255, 255, 255] } },
        { "name": "torche",  "light": { "type": "point", "color": [255, 150, 60], "radius": 500 } },
        { "name": "torche2", "light": { "type": "point", "color": [255, 150, 60] } }
      ]
    }"#;
    let p = dir.path().join("t.json");
    std::fs::write(&p, json).unwrap();
    let (bytes, _) = scene::build_file(&p).unwrap();
    let h = scene::parse_header(&bytes).unwrap();

    // Les 3 entrent dans la table (2 directionnelles max ne compte pas
    // les ponctuelles, plafonnées à 4 séparément).
    assert_eq!(h.light_count, 3);

    let flags_at = |i: usize| {
        let rec = h.entities_offset as usize + i * scene::ENTITY_SIZE;
        u16::from_le_bytes([bytes[rec + 0x1C], bytes[rec + 0x1D]])
    };
    let radius_at = |i: usize| {
        let rec = h.entities_offset as usize + i * scene::ENTITY_SIZE;
        u16::from_le_bytes([bytes[rec + 0x16], bytes[rec + 0x17]])
    };
    assert_eq!(flags_at(0), scene::ENTITY_FLAG_LIGHT);
    assert_eq!(
        flags_at(1),
        scene::ENTITY_FLAG_LIGHT | scene::ENTITY_FLAG_LIGHT_POINT
    );
    assert_eq!(radius_at(0), 0);
    assert_eq!(radius_at(1), 500);
    assert_eq!(radius_at(2), 600); // défaut

    // Type inconnu et rayon hors plage : erreurs claires.
    std::fs::write(&p, json.replace("\"point\"", "\"spot\"")).unwrap();
    assert!(scene::build_file(&p).unwrap_err().contains("type de lumière"));
    std::fs::write(&p, json.replace("500", "20")).unwrap();
    assert!(scene::build_file(&p).unwrap_err().contains("radius"));
}

#[test]
fn ui_table_fonts_and_strings() {
    let bytes = build_village();
    let h = scene::parse_header(&bytes).unwrap();
    assert_eq!(h.ui_count, 19);
    assert_eq!(h.font_count, 1);

    let ui = scene::parse_ui(&bytes, &h);
    // hud : canvas actif, sans image ni texte.
    assert_eq!(ui[0].components, (1 << 0) | (1 << 5));
    // vie_fond : image posée en haut-gauche, 70x12 a (8, 8).
    assert_eq!(ui[1].components & (1 << 1), 1 << 1);
    assert_eq!((ui[1].pos, ui[1].size), ([8, 8], [70, 12]));
    assert_eq!(ui[1].anchors[0..2], [0, 0]);
    // vie : image filled horizontale, etiree (min != max), amount 0.75.
    assert_eq!(ui[2].flags & 0x3, 3);
    assert_eq!(ui[2].data, 3072);
    assert_eq!(ui[2].anchors[2..4], [4096, 4096]);
    assert_eq!(ui[2].color, [200, 40, 40]);
    // zone : texte aligne a droite, police 0, chaine dans la table.
    assert_eq!(ui[3].components & (1 << 2), 1 << 2);
    assert_eq!((ui[3].asset, ui[3].extra), (0, 2));
    let (_, _, strings) = h.ui_offsets();
    let start = strings + ui[3].data as usize;
    let end = bytes[start..].iter().position(|&b| b == 0).unwrap() + start;
    assert_eq!(&bytes[start..end], b"VILLAGE");

    // Les entites UI portent le flag, et la police embarquee est un FNT
    // valide dont le TIM a ete place en VRAM par le packer.
    let fonts = scene::parse_fonts(&bytes, &h);
    let (off, size) = fonts[0];
    let fnt = &bytes[off..off + size];
    let info = psxpipe::fnt::parse(fnt).unwrap();
    assert_eq!((info.cell_w, info.cell_h), (6, 8));
    assert_eq!(psxpipe::fnt::advances(fnt)[(b'I' - 32) as usize], 5);

    // Jalon 4 : le canvas dialogue est inactif, ses lignes sont des
    // chaines vides (remplies par Ui_SetText), et les boutons du menu
    // pause portent le composant button (bit 3) en plus du texte.
    assert_eq!(ui[8].components & (1 << 0), 1 << 0); // dialogue canvas
    assert_eq!(ui[8].components & (1 << 5), 0); // inactif
    let l1_start = strings + ui[10].data as usize;
    assert_eq!(bytes[l1_start], 0); // dlg_l1 : chaine vide
    let btn = &ui[17]; // btn_reprendre
    assert_eq!(btn.components & (1 << 3), 1 << 3);
    assert_eq!(btn.components & (1 << 2), 1 << 2);
    assert_eq!(ui[18].components & (1 << 3), 1 << 3); // btn_quitter
}

#[test]
fn prefab_reference_is_inlined() {
    let dir = tempfile::tempdir().unwrap();
    samples::build_demo_assets(dir.path()).unwrap();
    std::fs::create_dir_all(dir.path().join("prefabs")).unwrap();
    // Prefab : un panneau UI avec un texte enfant.
    std::fs::write(
        dir.path().join("prefabs/pancarte.json"),
        r#"{
          "name": "pancarte",
          "assets": { "textures": [], "models": [],
                      "fonts": [{ "id": "main", "fnt": "main.fnt" }] },
          "entities": [
            { "name": "racine", "canvas": true },
            { "name": "libelle", "parent": "racine",
              "rect": { "size": [80, 10] },
              "text": { "font": "main", "text": "SALUT" } }
          ]
        }"#,
    )
    .unwrap();
    let json = r#"{
      "name": "avec-prefab",
      "assets": { "textures": [], "models": [{ "id": "cube", "pmd": "cube.pmd" }] },
      "entities": [
        { "name": "decor", "model": "cube" },
        { "name": "hud", "prefab": "prefabs/pancarte.json" }
      ]
    }"#;
    let scene: scene::SceneJson = serde_json::from_str(json).unwrap();
    let opts = scene::BuildOptions {
        prefab_dir: Some(dir.path().to_path_buf()),
        ..Default::default()
    };
    let (bytes, report) = scene::build_with_options(&scene, dir.path(), &opts).unwrap();
    // L'instance est remplacee : racine renommee "hud", enfant prefixe.
    assert_eq!(report.entity_names, vec!["decor", "hud", "hud.libelle"]);
    let h = scene::parse_header(&bytes).unwrap();
    assert_eq!((h.entity_count, h.ui_count, h.font_count), (3, 2, 1));
    // La police du prefab a bien ete fusionnee et embarquee.
    let ui = scene::parse_ui(&bytes, &h);
    assert_eq!(ui[1].components & (1 << 2), 1 << 2);
    // Prefabs imbriques refuses.
    std::fs::write(
        dir.path().join("prefabs/meta.json"),
        r#"{ "name": "meta", "assets": { "models": [] },
             "entities": [{ "name": "r", "prefab": "prefabs/pancarte.json" }] }"#,
    )
    .unwrap();
    let json2 = json.replace("prefabs/pancarte.json", "prefabs/meta.json");
    let scene2: scene::SceneJson = serde_json::from_str(&json2).unwrap();
    let err = scene::build_with_options(&scene2, dir.path(), &opts).unwrap_err();
    assert!(err.contains("imbriqu"), "{err}");
}

#[test]
fn solid_override_flags() {
    let dir = tempfile::tempdir().unwrap();
    samples::build_demo_assets(dir.path()).unwrap();
    // mur : collider force sans modele (bit 4) ; deco : modele traversable
    // (bit 5) ; defaut : aucun bit (le runtime decide par la presence du
    // modele).
    let json = r#"{
      "name": "colliders",
      "assets": { "textures": [], "models": [{ "id": "cube", "pmd": "cube.pmd" }] },
      "entities": [
        { "name": "mur",    "solid": true,  "scale": [4.0, 1.0, 0.5] },
        { "name": "deco",   "model": "cube", "solid": false },
        { "name": "normal", "model": "cube" }
      ]
    }"#;
    let json_path = dir.path().join("s.json");
    std::fs::write(&json_path, json).unwrap();
    let (bytes, _) = scene::build_file(&json_path).unwrap();
    let h = scene::parse_header(&bytes).unwrap();

    let flags = |i: usize| {
        let rec = h.entities_offset as usize + i * scene::ENTITY_SIZE;
        u16::from_le_bytes([bytes[rec + 0x1C], bytes[rec + 0x1D]])
    };
    assert_eq!(flags(0) & scene::ENTITY_FLAG_SOLID, scene::ENTITY_FLAG_SOLID);
    assert_eq!(flags(1) & scene::ENTITY_FLAG_NOT_SOLID, scene::ENTITY_FLAG_NOT_SOLID);
    assert_eq!(
        flags(2) & (scene::ENTITY_FLAG_SOLID | scene::ENTITY_FLAG_NOT_SOLID),
        0
    );
}

#[test]
fn controller_flag_and_pads() {
    let dir = tempfile::tempdir().unwrap();
    samples::build_demo_assets(dir.path()).unwrap();
    let json = r#"{
      "name": "ctrl",
      "assets": { "textures": [], "models": [{ "id": "cube", "pmd": "cube.pmd" }] },
      "entities": [
        { "name": "hero",  "model": "cube",
          "controller": { "speed": 7, "camera_back": 400, "camera_up": 180 } },
        { "name": "hero2", "model": "cube",
          "controller": { "camera": false } }
      ]
    }"#;
    let json_path = dir.path().join("s.json");
    std::fs::write(&json_path, json).unwrap();
    let (bytes, _) = scene::build_file(&json_path).unwrap();
    let h = scene::parse_header(&bytes).unwrap();

    // Les parametres logent dans les pads : pos.pad (vitesse) a 0x06,
    // rot.pad (recul camera) a 0x0E, scale.pad (hauteur) a 0x16.
    let pad = |i: usize, off: usize| {
        let rec = h.entities_offset as usize + i * scene::ENTITY_SIZE;
        u16::from_le_bytes([bytes[rec + off], bytes[rec + off + 1]])
    };
    let flags = |i: usize| pad(i, 0x1C);
    assert_eq!(
        flags(0) & scene::ENTITY_FLAG_CONTROLLER,
        scene::ENTITY_FLAG_CONTROLLER
    );
    assert_eq!((pad(0, 0x06), pad(0, 0x0E), pad(0, 0x16)), (7, 400, 180));
    // Defauts : vitesse 5, camera suiveuse coupee -> recul/hauteur 0.
    assert_eq!((pad(1, 0x06), pad(1, 0x0E), pad(1, 0x16)), (5, 0, 0));

    // Un controleur ne peut pas aussi etre camera ou lumiere (les pads
    // d'entite portent ses parametres).
    let bad = json.replace(
        r#""controller": { "camera": false }"#,
        r#""controller": true, "camera": true"#,
    );
    std::fs::write(&json_path, bad).unwrap();
    let err = scene::build_file(&json_path).unwrap_err();
    assert!(err.contains("Controller"), "unexpected error: {err}");
}

#[test]
fn psx_script_embedded_in_psc() {
    let dir = tempfile::tempdir().unwrap();
    samples::build_demo_assets(dir.path()).unwrap();
    std::fs::create_dir_all(dir.path().join("scripts")).unwrap();
    std::fs::write(
        dir.path().join("scripts/spinner.psxs"),
        "var speed = 12\nevery frame\n    rotate_y(self, speed)\nend\n",
    )
    .unwrap();
    let json = r#"{
      "name": "vm",
      "assets": { "textures": [], "models": [{ "id": "cube", "pmd": "cube.pmd" }] },
      "entities": [
        { "name": "girouette", "model": "cube", "script": "spinner" },
        { "name": "pnj", "model": "cube", "script": "npc" }
      ]
    }"#;
    let json_path = dir.path().join("s.json");
    std::fs::write(&json_path, json).unwrap();
    let (bytes, _) = scene::build_file(&json_path).unwrap();
    let h = scene::parse_header(&bytes).unwrap();

    // Flag v1.5 pose, table d'offsets : spinner -> blob PSB1, npc -> 0
    // (registre C). Le blob est un PSB1 valide au bon endroit.
    assert_eq!(h.flags & 1, 1);
    assert_eq!(h.script_count, 2);
    let offs = scene::parse_vm_offsets(&bytes, &h);
    assert_eq!(offs.len(), 2);
    assert!(offs[0] > 0 && offs[1] == 0);
    let blob = &bytes[offs[0] as usize..];
    assert_eq!(&blob[0..4], b"PSB2");

    // Sans .psxs : pas de flag, pas de table (retrocompatible).
    std::fs::remove_file(dir.path().join("scripts/spinner.psxs")).unwrap();
    let (bytes2, _) = scene::build_file(&json_path).unwrap();
    let h2 = scene::parse_header(&bytes2).unwrap();
    assert_eq!(h2.flags & 1, 0);

    // Erreur de compilation : nom du script + ligne dans le message.
    std::fs::write(
        dir.path().join("scripts/spinner.psxs"),
        "every frame\n    turn(self)\nend\n",
    )
    .unwrap();
    let err = scene::build_file(&json_path).unwrap_err();
    assert!(
        err.contains("spinner") && err.contains("ligne 2"),
        "unexpected: {err}"
    );
}

#[test]
fn multiple_scripts_per_entity() {
    let dir = tempfile::tempdir().unwrap();
    samples::build_demo_assets(dir.path()).unwrap();
    let json = r#"{
      "name": "multi",
      "assets": { "textures": [], "models": [{ "id": "cube", "pmd": "cube.pmd" }] },
      "entities": [
        { "name": "hero", "model": "cube", "scripts": ["player", "hud", "npc"] },
        { "name": "deco", "model": "cube", "script": "torche" }
      ]
    }"#;
    let json_path = dir.path().join("s.json");
    std::fs::write(&json_path, json).unwrap();
    let (bytes, _) = scene::build_file(&json_path).unwrap();
    let h = scene::parse_header(&bytes).unwrap();

    // 4 scripts distincts, table de composants presente (flag bit 1).
    assert_eq!(h.script_count, 4);
    assert_eq!(h.flags & 2, 2);
    let comps = scene::parse_script_comps(&bytes, &h);
    assert_eq!(comps.len(), 4);
    // hero (entite 0) porte 3 scripts, deco (1) en porte 1.
    assert_eq!(comps.iter().filter(|(e, _)| *e == 0).count(), 3);
    assert_eq!(comps.iter().filter(|(e, _)| *e == 1).count(), 1);
    // Le champ script de l'entite garde le PREMIER (compat runtime ancien).
    let script_ref = |i: usize| {
        let rec = h.entities_offset as usize + i * scene::ENTITY_SIZE;
        u16::from_le_bytes([bytes[rec + 0x1E], bytes[rec + 0x1F]])
    };
    assert_eq!(script_ref(0), 1); // player
    assert_eq!(comps[0], (0, 1));

    // Les tables suivantes restent lisibles (offsets de l'en-tete).
    assert!(h.lights_offset as usize > h.scripts_offset as usize);
    assert_eq!(h.total_size as usize, bytes.len());

    // Mono-script : pas de table, layout historique.
    let mono = json.replace(r#""scripts": ["player", "hud", "npc"]"#, r#""script": "player""#);
    std::fs::write(&json_path, mono).unwrap();
    let (b2, _) = scene::build_file(&json_path).unwrap();
    let h2 = scene::parse_header(&b2).unwrap();
    assert_eq!(h2.flags & 2, 0);
    assert!(scene::parse_script_comps(&b2, &h2).is_empty());
}

#[test]
fn public_script_values_in_scene() {
    let dir = tempfile::tempdir().unwrap();
    samples::build_demo_assets(dir.path()).unwrap();
    std::fs::create_dir_all(dir.path().join("scripts")).unwrap();
    std::fs::write(
        dir.path().join("scripts/turret.psxs"),
        "public var speed = 10\npublic var target : entity\npublic var actif : bool = 1\n\nevery frame\n    rotate_y(self, speed)\nend\n",
    )
    .unwrap();
    let json = r#"{
      "name": "pub",
      "assets": { "textures": [], "models": [{ "id": "cube", "pmd": "cube.pmd" }] },
      "entities": [
        { "name": "cible", "model": "cube" },
        { "name": "tourelle", "model": "cube",
          "scripts": [{ "name": "turret",
                        "values": { "speed": 42, "target": "cible", "actif": false } }] }
      ]
    }"#;
    let json_path = dir.path().join("s.json");
    std::fs::write(&json_path, json).unwrap();
    let (bytes, _) = scene::build_file(&json_path).unwrap();
    let h = scene::parse_header(&bytes).unwrap();

    // Flag v1.7 pose, 3 valeurs pour l'entite 1 (tourelle), script 1.
    assert_eq!(h.flags & 4, 4);
    let values = scene::parse_script_values(&bytes, &h);
    assert_eq!(values.len(), 3);
    assert!(values.iter().all(|(e, s, _, _)| *e == 1 && *s == 1));
    let by_field = |f: u8| values.iter().find(|(_, _, ff, _)| *ff == f).unwrap().3;
    assert_eq!(by_field(0), 42); // speed
    assert_eq!(by_field(1), 0); // target -> index de l'entite "cible"
    assert_eq!(by_field(2), 0); // actif = false

    // Reference d'entite inconnue : erreur claire.
    let bad = json.replace("\"target\": \"cible\"", "\"target\": \"fantome\"");
    std::fs::write(&json_path, bad).unwrap();
    let err = scene::build_file(&json_path).unwrap_err();
    assert!(err.contains("fantome") && err.contains("introuvable"), "{err}");

    // Champ inconnu : simple avertissement, la scene se construit.
    let warn = json.replace("\"speed\": 42", "\"vitesse\": 42");
    std::fs::write(&json_path, warn).unwrap();
    let (_, report) = scene::build_file(&json_path).unwrap();
    assert!(report.warnings.iter().any(|w| w.contains("vitesse")), "{:?}", report.warnings);
}
