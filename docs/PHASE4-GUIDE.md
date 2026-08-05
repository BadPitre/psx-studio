# Phase 4 — Éditeur MVP (en cours)

Objectif de sortie de la phase complète : créer une scène entièrement dans
l'éditeur, sans toucher au JSON, et la lancer d'un bouton **Play**.

La phase est découpée en deux parties :

## Part 1 — livrée : la visionneuse (le socle rendu + UI)

Application **Vite + React + TypeScript + Three.js** dans `editor/` :

- **Parsers TypeScript** des formats console (PSC/PMD/TIM avec décodage
  CLUT), miroirs des writers Rust et **testés contre les vraies scènes**
  du pipeline (vitest) — le contrat de format est verrouillé des deux
  côtés.
- **Viewport « PS1 authentique »** : rendu natif 320×240 upscalé pixels
  nets, vertex snapping (le jitter), mapping affine (`uv × w`), Gouraud
  par sommet avec les formules du GTE et l'éclairage lu dans la scène,
  sortie couleur linéaire, textures NearestFilter.
- **Hiérarchie** (indentation parent/enfant, compteurs de tris),
  **sélection** au clic (liste ou raycast viewport, surlignage), et
  **inspecteur** de transform éditable **en direct** (position/rotation°/
  échelle — le modèle Unity : Scene/GameObject/Transform/Inspector).
- Ouverture de `.psc` par bouton ou glisser-déposer ; la scène de démo
  se charge automatiquement en dev.

Lancer : voir `editor/README.md` (`npm install && npm run dev`).

## Part 2 — livrée : Tauri + édition + Play Mode

- **Enveloppe Tauri v2** (`editor/src-tauri`) : `psxpipe` est branché en
  **dépendance Rust directe** — l'éditeur convertit et packe avec
  exactement le même code que le CLI. Lancer : `cargo tauri dev` depuis
  `editor/` (prérequis : `cargo install tauri-cli`).
- **Mode projet** : « Ouvrir un projet » (dossier avec `project.json`),
  sélecteur de scènes, hiérarchie avec les **vrais noms** d'entités
  (l'ordre topologique du .psc est corrélé au JSON par le rapport de
  build). L'inspecteur **édite le `scene.json` source** : viewport mis à
  jour immédiatement, `.psc` reconstruit à la volée (~400 ms), bouton
  **Enregistrer** (le backend refuse d'écrire un JSON invalide).
- **Play Mode** : bouton **▶ Play** = build incrémental du projet +
  lancement de PCSX-Redux (`-run -loadiso <cue> -webserver`), puis
  pilotage par son API web : statut en direct (en cours/en pause),
  **Pause / Reprendre / Reset** depuis l'éditeur. Les routes de l'API
  ont été vérifiées dans les sources de PCSX-Redux, et le client HTTP
  Rust (`psxpipe::redux`) est testé contre un faux serveur. L'API
  d'écriture RAM (`POST /api/v1/cpu/ram/raw`) est déjà encapsulée —
  c'est la porte d'entrée du live tweaking à venir.
- Le mode navigateur (visionneuse .psc) reste fonctionnel sans Tauri.

## Part 3 — livrée : création de scène complète dans l'UI

- **Création / duplication / suppression d'entités** (boutons de la
  hiérarchie), **renommage** et **choix du modèle** dans l'inspecteur —
  toutes les éditions passent par le `scene.json`, la suppression refuse
  d'orpheliner des enfants, les noms restent uniques.
- **Import d'assets par drag & drop** : glisser des `.gltf`/`.glb`/`.png`
  dans la fenêtre → copie dans `assets/`, enregistrement dans
  `project.json`, conversion immédiate dans `Library/`, rapport dans la
  barre. Un modèle et une texture du même nom sont appairés
  automatiquement, et l'asset est enregistré dans la scène courante —
  il ne reste qu'à créer une entité et lui assigner le modèle.
- **VRAM Viewer** (bouton « VRAM ») : la carte 1024×512 en panneau, avec
  le **contenu réel** des textures à leur place (largeur en mots 16 bits),
  framebuffers, CLUTs, police et grille des pages. Fonctionne aussi en
  mode navigateur (placements lus dans le .psc).

→ **Critère de sortie atteint** : une scène se crée entièrement dans
l'éditeur (import glTF → entité → transform → Enregistrer → ▶ Play),
sans toucher au JSON.

## Reste (bonus de fin de phase)

1. Live tweaking en Play Mode (écriture RAM sur la table d'entités —
   l'API `write_ram` est déjà encapsulée côté client).
2. Édition des réglages de scène (fond/ambiante/lumière) dans l'UI.
3. Packaging de l'app (`cargo tauri build`, icônes complètes).

## Critères de sortie de la Part 1

- [x] `npm test` vert (parsers validés contre les .psc du pipeline)
- [x] `npm run build` sans erreur TypeScript
- [x] Le village s'affiche fidèlement (vérifié par capture navigateur) ;
      hiérarchie, sélection et édition de transform en direct
- [ ] Vérification sur ta machine : `npm run dev`, orbite fluide, édition
      d'une position dans l'inspecteur visible immédiatement

## Notes de rendu

- Le viewport dessine les polygones **double face** (confort d'édition) ;
  la console cull les faces arrière — un toggle « culling console »
  viendra avec les budgets.
- Le snapping/affine est calculé aux mêmes résolutions que la console ;
  l'écart résiduel avec l'émulateur : le dithering Bayer (à ajouter) et
  la quantization 15 bits de sortie.
