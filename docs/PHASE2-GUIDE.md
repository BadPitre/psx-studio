# Phase 2 — Format de scène & runtime player

Objectif de sortie : deux scènes JSON, packées en binaire, chargées **depuis
le CD** par un runtime générique, naviguables à la manette, avec transition
entre les deux. Le moteur ne connaît plus le contenu : il joue des données.

Prérequis : Phase 1 (toolchain + Rust).

---

## 1. Ce qui a été construit

### Format `SceneFormat v1` (spec complète : `SCENE-FORMAT.md`)
- **JSON éditable** (`scene0.json`) : entités avec transform
  (position/rotation/échelle), hiérarchie parent/enfant, assets référencés,
  éclairage de scène (fond, ambiante, directionnelle).
- **Binaire `.psc`** packé par `psxpipe scene` : en-tête, tables, entités
  triées (parents d'abord), et **blobs PMD/TIM embarqués** → une scène se
  charge en **une seule lecture CD contiguë**, comme prévu dans la doc
  projet (§4.3).
- Validations du sérialiseur : cycles de parenté, ids inconnus, débordements
  i16, **collisions VRAM** entre textures et avec les framebuffers.

### Runtime player (`runtime/player`)
- **Arène mémoire de scène** (1 Mo) : le `.psc` est lu dedans, les modèles
  sont fixés en place, l'array d'entités y est alloué. Changement de scène =
  reset de l'arène. Aucun malloc.
- Lecture CD : `CdSearchFile` + `CdRead` (le code partagé PMD vit
  maintenant dans `runtime/engine/`).
- **Hiérarchie** : matrices monde composées en un passage (l'invariant du
  format garantit parents avant enfants), échelle par axe, matrice de
  rotation séparée pour l'éclairage (une échelle non uniforme ne fausse pas
  les intensités).
- Caméra libre type FPS (maths de l'exemple `fpscam` du SDK).
- **Rond** : passe à la scène suivante (SCENE0 ⇄ SCENE1).

### Scènes de démo (dans `pipeline/samples/`, packées dans
`runtime/player/assets/`)
- `scene0.json` — **village** : sol texturé, 3 maisons (rotations/échelles
  variées), cheminée **parentée** au toit de la première maison.
- `scene1.json` — **champ de cubes** : grille de cubes + « phare », fond et
  éclairage différents pour rendre la transition évidente.

## 2. Builder et lancer

```powershell
# (optionnel) régénérer les scènes :
cd <repo>\pipeline
cargo run --example gen_scenes -- samples ..\runtime\player\assets

# builder le player :
cd <repo>\runtime\player
cmake --preset default
cmake --build .\build
```

Ouvrir `build\player.cue` dans PCSX-Redux ou DuckStation.

Attendu : le village au chargement, texte "PSX STUDIO - PHASE 2",
compteur de triangles. **Rond** bascule sur le champ de cubes (fond
différent) et inversement.

### Contrôles
| Entrée | Action |
|---|---|
| D-pad haut/bas | avancer/reculer (dans la direction du regard) |
| D-pad gauche/droite | tourner (yaw) |
| L1 / R1 | pas latéral |
| Triangle / Croix | regarder haut/bas |
| **Rond** | **scène suivante** |
| Start | reset caméra |

## 3. Préview PC des scènes

```powershell
cd <repo>\pipeline
cargo run --example preview -- ..\runtime\player\assets\scene0.psc village.png
cargo run --example preview -- ..\runtime\player\assets\scene1.psc champ.png --cam 0,-200,-600
```

Mêmes règles de rendu que le runtime (projection, culling, éclairage,
mapping affine). `--cam X,Y,Z`, `--yaw`, `--pitch` pour déplacer la caméra
(rappel : **+Y vers le bas**).

## 4. Écrire ta propre scène

1. Convertis tes assets (`gltf2pmd`, `png2tim`) en choisissant des
   placements VRAM qui ne se chevauchent pas (ex. 2e texture 8bpp :
   `--org-x 448 --clut-x 320 --clut-y 257`).
2. Écris `mascene.json` (copie `samples/scene0.json` comme point de départ).
3. `cargo run -- scene mascene.json -o ..\runtime\player\assets\scene0.psc`
   — lis le rapport (le sérialiseur refuse les collisions VRAM et les
   cycles de parenté).
4. Rebuild du player (les `.psc` partent sur l'ISO, pas dans l'exécutable).

## 5. Dépannage

| Symptôme | Cause probable | Fix |
|---|---|---|
| Écran figé au boot | fichier absent de l'ISO (`CdSearchFile` échoue → assert) | vérifier `iso.xml`, noms 8.3 majuscules (`SCENE0.PSC;1`) |
| Textures corrompues après un switch | collisions VRAM entre scènes | garder les mêmes placements TIM d'une scène à l'autre, ou re-vérifier avec `psxpipe scene` |
| Grands polys qui disparaissent selon l'angle/la distance | le GPU PS1 **rejette** toute primitive > 1023×511 px à l'écran | subdiviser le mesh (cf. `plane_mesh` : le sol des démos est une grille 8×8) |
| Textures très étirées sur de grandes surfaces | 256 texels max par poly + mapping affine | grille avec une répétition de texture par cellule (même fix) |
| Un grand poly coupe/recouvre les objets posés dessus | tri OT par Z **moyen** : un poly géant a une seule profondeur | même fix — des cellules petites se trient localement |
| Sol qui « nage » en mouvement | warping affine — authentique PS1, atténué par la grille | subdivision dynamique prévue Phase 6 |
| Sol/mur qui passe devant un objet proche | granularité du tri OT (buckets de profondeur) | corrigé : buckets de 4 unités (`avsz3` direct) + ordre de dessin inversé — lister le sol en premier dans le JSON |
| Arêtes qui « cassent » en zoom extrême | rasterisation PS1 : coordonnées écran entières + affine | authentique — les jeux d'époque limitaient la proximité caméra ; subdivision Phase 6 |
| Trou dans le sol en bas de l'écran de très près | near-clipping : un triangle passant derrière la caméra est jeté entier | cellules plus petites = trou plus petit ; clipping géométrique prévu Phase 6 |
| Géométrie qui clignote en traversant un objet | pas de near-clipping (v1) | prévu avec la subdivision ; éviter de rentrer dans les meshes |
| Modèle assombri par l'échelle | — corrigé : l'éclairage utilise la rotation non-échelléé | (référence si régression) |

## 6. Critères de sortie de la Phase 2

- [ ] `cargo test` vert (19 tests dont build des deux scènes)
- [ ] `player.cue` boote, le village s'affiche, navigation fluide
- [ ] Rond charge le champ de cubes puis re-village (arène réutilisée,
      pas de fuite ni de corruption après plusieurs cycles)
- [ ] La cheminée suit bien le toit de maison1 (hiérarchie OK)
- [ ] Une scène perso écrite en JSON passe dans `psxpipe scene` et tourne

## 7. Prochaine étape → Phase 3

Pipeline complet & VRAM : packer VRAM automatique avec carte exportée
(fini les `--org-x` à la main), audio VAG + XA, `psxpipe build` de bout en
bout (projet → `.bin/.cue` via mkpsxiso) avec cache incrémental.
