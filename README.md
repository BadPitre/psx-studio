# PSX Studio

**Un éditeur de jeux 3D pour PlayStation 1, philosophie Unity, 100 % open
source (MIT).** Scène, hiérarchie, inspecteur, import glTF depuis Blender,
Play sur émulateur en un bouton — et le résultat tourne sur une vraie
console : formats binaires natifs, GTE en virgule fixe, Ordering Table,
2 Mo de RAM.

## Ce que ça fait aujourd'hui

- **Éditeur desktop** (Tauri + React + Three.js) : viewport 320×240
  authentique (vertex snapping, textures affines, Gouraud GTE), gizmos
  déplacement/rotation/échelle, hiérarchie parent/enfant, inspecteur,
  annuler/rétablir, VRAM viewer, import glTF/GLB/PNG par glisser-déposer
  (texture extraite et quantifiée automatiquement).
- **Pipeline d'assets** (Rust) : glTF → PMD (packets GPU pré-encodés),
  PNG → TIM (quantization median-cut, CLUT), WAV → VAG (SPU-ADPCM),
  subdivision anti-warping, packer VRAM automatique, scènes JSON → PSC
  (un fichier contigu = une lecture CD), build ISO incrémental avec cache.
- **Moteur console** (C, PSn00bSDK) : chargement de scène par arène,
  hiérarchie de transforms, scripts C enregistrés par nom (résolution par
  hash, zéro interpréteur), entrées manette, physique AABB, dialogues,
  SFX SPU et musique CD-DA.
- **Play Mode** : build + lancement PCSX-Redux pilotés depuis l'éditeur
  (statut, pause, reset) et **live tweaking** : déplacer une entité dans
  l'éditeur pendant que le jeu tourne l'écrit directement dans la RAM
  console.
- **Démo jouable** : un village, un perso qui marche à la manette, des
  collisions, un PNJ qui parle.

## Démarrage

Suis **[docs/GETTING-STARTED.md](docs/GETTING-STARTED.md)** — prérequis,
build, premier Play, workflow Blender, écriture de gameplay. Résumé :

```bat
cd runtime\game && cmake --preset default && cmake --build build
cd pipeline\psxpipe && cargo run --release --example gen_project -- ..\..\examples\demo
cd editor && npm install && cargo tauri dev
```

puis « Ouvrir un projet… » → `examples/demo` → **▶ Play**.

## Arborescence

- `runtime/` — moteur console C : `engine/` (partagé), `game/` (démo
  jouable Phase 5), `player/` (visionneuse), `hello-triangle/`,
  `poc-renderer/`
- `pipeline/` — CLI `psxpipe` (build, gltf2pmd, png2tim, scene, wav2vag,
  info) + previewer logiciel + client PCSX-Redux
- `editor/` — l'éditeur desktop (mode navigateur = visionneuse .psc)
- `examples/demo/` — projet exemple complet
- `docs/` — [mode d'emploi](docs/GETTING-STARTED.md), specs
  ([PMD](docs/PMD-FORMAT.md), [Scene](docs/SCENE-FORMAT.md)), doc
  d'architecture (`psx-studio-doc-projet.md`) et guides de phase 0→6
  (le journal de construction du studio)

## Contribuer

Le projet est construit pour être repris : formats spécifiés et testés
des deux côtés, validations sans console (previewer logiciel, simulateur
GTE, faux serveur émulateur), et une liste d'améliorations autonomes dans
**[CONTRIBUTING.md](CONTRIBUTING.md)**.

## Licence

MIT (voir [LICENSE](LICENSE)). Certaines portions du runtime sont
adaptées des exemples PSn00bSDK (MPL, © Lameguy64/spicyjpeg) —
attribution conservée dans les fichiers concernés.
