# PSX Studio

Éditeur de jeux 3D pour PlayStation 1, philosophie Unity, 100 % open source (MIT).

- `runtime/`  — moteur console (C, PSn00bSDK) — `engine/` (code partagé), `hello-triangle/` (Ph. 0), `poc-renderer/` (Ph. 1), `player/` (Ph. 2-3)
- `pipeline/` — outils d'assets et de build (Rust) — CLI `psxpipe` (`build`, `gltf2pmd`, `png2tim`, `scene`, `wav2vag`) + préview PC
- `examples/` — `demo/` : projet exemple buildable en une commande (`psxpipe build`)
- `editor/`   — éditeur desktop (React + TypeScript + Three.js, Tauri à venir) — Phase 4 part 1 : visionneuse/inspecteur `.psc`, viewport PS1
- `docs/`     — doc projet (`psx-studio-doc-projet.md`), guides de phase, specs `PMD-FORMAT.md` / `SCENE-FORMAT.md`

Démarrage : `docs/PHASE0-GUIDE.md` (installation), puis les guides `PHASE1` (pipeline + rendu), `PHASE2` (scènes + player), `PHASE3` (build one-command + audio).

Licence : MIT. Certaines portions du runtime sont adaptées des exemples PSn00bSDK (MPL, © Lameguy64/spicyjpeg) — attribution conservée dans les fichiers concernés.
