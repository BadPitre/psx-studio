# PSX Studio

Éditeur de jeux 3D pour PlayStation 1, philosophie Unity, 100 % open source (MIT).

- `runtime/`  — moteur console (C, PSn00bSDK) — `engine/` (code partagé), `hello-triangle/` (Ph. 0), `poc-renderer/` (Ph. 1), `player/` (Ph. 2)
- `pipeline/` — outils d'assets et de build (Rust) — CLI `psxpipe` (`gltf2pmd`, `png2tim`, `scene`) + préview PC
- `editor/`   — éditeur desktop (Tauri + React + TypeScript) — à partir de la Phase 4
- `docs/`     — doc projet (`psx-studio-doc-projet.md`), guides de phase, specs `PMD-FORMAT.md` / `SCENE-FORMAT.md`

Démarrage : `docs/PHASE0-GUIDE.md` (installation), `PHASE1-GUIDE.md` (pipeline + rendu), `PHASE2-GUIDE.md` (scènes + player).

Licence : MIT. Certaines portions du runtime sont adaptées des exemples PSn00bSDK (MPL, © Lameguy64/spicyjpeg) — attribution conservée dans les fichiers concernés.
