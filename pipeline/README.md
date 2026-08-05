# pipeline — outils d'assets (Rust)

CLI `psxpipe` : conversion des assets sources vers les formats console PS1.

```
psxpipe gltf2pmd <in.gltf> [-o out.pmd] [--size 128] [--flat] [--untextured] [--tex-w 256] [--tex-h 256]
psxpipe png2tim  <in.png>  [-o out.tim] [--bpp 4|8|16] [--org-x 320] [--org-y 0] [--clut-x 320] [--clut-y 256]
psxpipe info     <file.pmd|file.tim>
```

- `gltf2pmd` : glTF/GLB → **PMD** (spec : `docs/PMD-FORMAT.md`). Quantization
  des positions en i16, normales 4.12, UV 8 bits, déduplication, paquets GPU
  pré-encodés, rapport d'import avec avertissements.
- `png2tim` : PNG → **TIM**. Quantization median-cut (16 ou 256 couleurs),
  CLUT, transparence (alpha < 128 → texel 0x0000), placement VRAM avec
  détection de collision framebuffer.
- `samples/` : cube glTF + texture damier de démonstration, régénérables via
  `cargo run --example gen_samples -- samples`.

Développement : `cargo test` (tests unitaires + import du cube de bout en
bout), `cargo clippy`.
