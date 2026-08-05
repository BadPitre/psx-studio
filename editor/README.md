# editor — éditeur desktop (Phase 4)

Éditeur PSX Studio. **Part 1 (actuelle)** : visionneuse/inspecteur de
scènes `.psc` dans le navigateur — viewport « PS1 authentique », hiérarchie,
inspecteur de transform en direct. **Part 2 (à venir)** : enveloppe Tauri,
édition/sauvegarde des `scene.json`, import drag & drop via psxpipe,
VRAM Viewer, bouton Play (PCSX-Redux).

## Lancer

```powershell
cd editor
npm install
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

Contrôles : glisser = orbite, clic droit = pan, molette = zoom,
clic = sélection d'entité.

Note : le viewport dessine les deux faces des polygones (pratique en
édition) ; la console, elle, cull les faces arrière.

## Tests

```powershell
npm test
```

Les parsers TypeScript (PSC/PMD/TIM) sont validés contre les **vraies
scènes** produites par le pipeline Rust (`runtime/player/assets/*.psc`) —
toute divergence de format entre l'éditeur et `psxpipe` casse la suite.
