# editor — éditeur desktop (Phase 4)

Éditeur PSX Studio : app desktop **Tauri** (React + TypeScript + Three.js)
avec `psxpipe` en dépendance Rust directe. Mode projet (édition des
`scene.json`, rebuild à la volée, sauvegarde) + **Play Mode** PCSX-Redux
(build + lancement + pause/reprise/reset via l'API web de l'émulateur).

## Lancer (desktop)

```powershell
cd editor
npm install
cargo install tauri-cli --version "^2"   # une fois
cargo tauri dev
```

Puis « Ouvrir un projet… » → `examples/demo` (après `cargo run --example
gen_project` dans `pipeline/`). Le bouton **▶ Play** attend `pcsx-redux`
dans le PATH (champ à droite de la barre pour un chemin complet).

## Lancer (navigateur — visionneuse seule)

```powershell
npm run dev        # http://localhost:5173
```

La scène de démo se charge automatiquement si `public/scene0.psc` existe :

```powershell
# depuis pipeline/ : régénère les scènes puis copie-les dans public/
cargo run --example gen_scenes -- samples ../runtime/player/assets
cp ../runtime/player/assets/scene*.psc public/
```

Sinon : glisser-déposer n'importe quel `.psc` dans la fenêtre.

## Viewport PS1

Rendu natif **320×240** upscalé en pixels nets, avec un matériau custom qui
reproduit la console :
- **vertex snapping** sur la grille écran (le « jitter » PS1) ;
- **mapping affine** (annulation de la correction de perspective par le
  truc `uv × w`) ;
- **Gouraud par sommet** : mêmes formules que le GTE (CC = ambiante +
  lumière × N·L, clampé à 255, modulation ×base/128) — les valeurs
  d'éclairage viennent de l'en-tête de la scène ;
- sortie couleur linéaire (pas de courbe sRGB) et textures TIM décodées
  CLUT → NearestFilter.

### Contrôles du viewport
- **Clic gauche** : sélection (clic) / orbite (glisser)
- **Clic droit tenu** : caméra FPS — regard à la souris + **ZQSD** pour se
  déplacer (codes physiques : WASD en QWERTY marche aussi), **E/Espace**
  monter, **Q** descendre, **Shift** = rapide
- **Molette** : avancer/reculer · **Clic milieu** : pan

### Raccourcis hiérarchie (mode projet)
- **Ctrl+C / Ctrl+V** copier/coller · **Ctrl+D** dupliquer ·
  **Suppr** supprimer · **F2** renommer · **clic droit** : menu contextuel

Note : le viewport dessine les deux faces des polygones (pratique en
édition) ; la console, elle, cull les faces arrière.

## Tests

```powershell
npm test
```

Les parsers TypeScript (PSC/PMD/TIM) sont validés contre les **vraies
scènes** produites par le pipeline Rust (`runtime/player/assets/*.psc`) —
toute divergence de format entre l'éditeur et `psxpipe` casse la suite.
