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

## Part 2 — à venir (l'éditeur complet)

1. **Enveloppe Tauri** : accès disque projet, `psxpipe` en dépendance
   Rust directe du backend.
2. **Édition du `scene.json` source** (pas du binaire) + sauvegarde ;
   le `.psc` devient un artefact de build à la volée.
3. **Prototype Play Mode** — le risque n°1 : build incrémental + lancement
   PCSX-Redux + connexion à son API (pause/reprise, lecture mémoire).
4. Import d'assets par drag & drop (rapports psxpipe dans l'UI),
   **VRAM Viewer** interactif (la carte PNG de la Phase 3 devient un
   panneau), compteur de budgets permanent.

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
