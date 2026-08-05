# pipeline — outils d'assets (Rust)

CLI `psxpipe` : conversion des assets sources vers les formats console PS1.

```
psxpipe build    [projet]  [--force]                 # projet complet -> .bin/.cue (mkpsxiso)
psxpipe gltf2pmd <in.gltf> [-o out.pmd] [--size 128] [--flat] [--untextured] [--tex-w 256] [--tex-h 256]
psxpipe png2tim  <in.png>  [-o out.tim] [--bpp 4|8|16] [--org-x 320] [--org-y 0] [--clut-x 320] [--clut-y 256]
psxpipe scene    <in.json> [-o out.psc] [--keep-vram] [--vram-map carte.png]
psxpipe wav2vag  <in.wav>  [-o out.vag]
psxpipe info     <file.pmd|file.tim|file.psc>
```

- `build` : dossier projet (`project.json`) → ISO bootable. Conversion
  incrémentale (cache par hash dans `Library/`), packing VRAM automatique,
  cartes VRAM PNG, `iso.xml` + pistes CD-DA, `mkpsxiso`. Projet exemple :
  `examples/demo/` (sources via `cargo run --example gen_project`).
- `wav2vag` : encodeur SPU-ADPCM (SFX console), en-tête VAG standard.

- `gltf2pmd` : glTF/GLB → **PMD** (spec : `docs/PMD-FORMAT.md`). Quantization
  des positions en i16, normales 4.12, UV 8 bits, déduplication, paquets GPU
  pré-encodés, rapport d'import avec avertissements.
- `png2tim` : PNG → **TIM**. Quantization median-cut (16 ou 256 couleurs),
  CLUT, transparence (alpha < 128 → texel 0x0000), placement VRAM avec
  détection de collision framebuffer.
- `scene` : scène JSON éditable → **PSC** packé (spec : `docs/SCENE-FORMAT.md`).
  Blobs PMD/TIM embarqués pour une lecture CD contiguë, tri topologique de la
  hiérarchie, validation des collisions VRAM. Scènes de démo :
  `cargo run --example gen_scenes -- samples ../runtime/player/assets`.
- `samples/` : assets de démonstration régénérables via
  `cargo run --example gen_samples -- samples` : cube + damier, et une
  **maison low-poly** (`house.gltf` + `house.png`, 16 tris, porte/fenêtre,
  toit débordant) qui sert de modèle de test "façon Blender".
- **Préview PC** : `cargo run --example preview -- model.pmd texture.tim out.png
  [--yaw N --pitch N --dist N]` — rendu logiciel 320×240 simulant le runtime
  (même projection, culling, éclairage, mapping affine) pour vérifier un asset
  sans booter l'émulateur.

Développement : `cargo test` (tests unitaires + import du cube de bout en
bout), `cargo clippy`.
