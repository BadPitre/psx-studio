# PSX Studio

Éditeur de jeux 3D pour PlayStation 1, philosophie Unity, 100 % open source (MIT).

- `runtime/`  — moteur console (C, PSn00bSDK) — Phase 0 : `hello-triangle/`, Phase 1 : `poc-renderer/`
- `pipeline/` — outils d'assets et de build (Rust) — Phase 1 : CLI `psxpipe` (`gltf2pmd`, `png2tim`)
- `editor/`   — éditeur desktop (Tauri + React + TypeScript) — à partir de la Phase 4
- `docs/`     — doc projet (`psx-studio-doc-projet.md`), guides de phase, spec `PMD-FORMAT.md`

Démarrage : `docs/PHASE0-GUIDE.md` (installation) puis `docs/PHASE1-GUIDE.md` (pipeline + rendu).

Licence : MIT. Certaines portions du runtime sont adaptées des exemples PSn00bSDK (MPL, © Lameguy64/spicyjpeg) — attribution conservée dans les fichiers concernés.
