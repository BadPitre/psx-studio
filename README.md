# PSX Studio

Éditeur de jeux 3D pour PlayStation 1, philosophie Unity, 100 % open source (MIT).

- `runtime/`  — moteur console (C, PSn00bSDK) — `engine/` (moteur partagé : scène, scripts, entrées, physique, SFX), `hello-triangle/` (Ph. 0), `poc-renderer/` (Ph. 1), `player/` (Ph. 2-3), `game/` (Ph. 5 : démo jouable avec scripts)
- `pipeline/` — outils d'assets et de build (Rust) — CLI `psxpipe` (`build`, `gltf2pmd`, `png2tim`, `scene`, `wav2vag`) + préview PC + client PCSX-Redux (Play Mode, live tweaking)
- `examples/` — `demo/` : projet exemple buildable en une commande (`psxpipe build`)
- `editor/`   — éditeur desktop (Tauri + React + TypeScript + Three.js) — Phase 4 : projet, inspecteur, import d'assets, VRAM viewer, Play Mode ; Phase 5 : champ Script, live tweaking
- `docs/`     — doc projet (`psx-studio-doc-projet.md`), guides de phase, specs `PMD-FORMAT.md` / `SCENE-FORMAT.md`

Démarrage : `docs/PHASE0-GUIDE.md` (installation), puis les guides `PHASE1` (pipeline + rendu), `PHASE2` (scènes + player), `PHASE3` (build one-command + audio), `PHASE4` (éditeur), `PHASE5` (gameplay : scripts, physique, live tweaking).

Licence : MIT. Certaines portions du runtime sont adaptées des exemples PSn00bSDK (MPL, © Lameguy64/spicyjpeg) — attribution conservée dans les fichiers concernés.
