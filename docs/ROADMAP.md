# Feuille de route — après la Phase 6

> Les phases 0→6 du plan initial sont livrées (voir les guides de phase).
> Ce document planifie les chantiers suivants, discutés et validés dans
> leur principe. Ordre indicatif ; chaque lot est livrable indépendamment,
> avec la même exigence : format rétrocompatible, testé des deux côtés,
> vérifiable sans console.

## Lot A — Lumières & caméras (composants d'entité)

Philosophie Unity : une entité *devient* lumière ou caméra, se manipule
au gizmo, s'anime par script, se règle en live tweaking.

1. ~~**Lumières directionnelles ×3**~~ — **livré** (format v1.2 : flags
   d'entité + table dans les octets réservés ; direction = rotation de
   l'entité, re-dérivée chaque frame ; viewport et previewer à 3
   lumières ; UI inspecteur avec couleur ; lune bleutée dans le village
   de démo)
2. ~~**Lumières ponctuelles (torches)**~~ — **livré** (type point +
   rayon dans le format, approximation par objet dans Scene_Draw avec
   distance octogonale, halo additif B+F, script `torche` qui fait
   osciller le rayon, torche orange dans le village, éditeur avec
   type/rayon + sphère filaire + shader par sommet, previewer en
   parité par objet)
3. ~~**Caméras**~~ — **livré** (bit `camera`, frustum dans le calque net,
   vue initiale du runtime — `Scene_ApplyCamera()` — dans le game et le
   player ; convention unique : une entité regarde vers son -Z local).
   Reste : `Camera_Activate()` par script pour les cinématiques.

## Lot B — Streaming (niveau 1 : préchargement)

~~Objectif : plus aucun écran de chargement entre scènes.~~ — **livré** :

- deux arènes de 512 Ko en bascule ; `Scene_Preload()` lit la scène
  suivante en asynchrone pendant le gameplay (`CdRead` + poll
  `CdReadSync(1)` par frame), `Scene_ActivatePreloaded()` bascule
  instantanément (parse local, zéro lecture CD)
- garde-fou mono-laser : un chargement bloquant attend la fin d'un
  préchargement en vol ; la musique CD-DA reste à relancer par
  l'appelant après toute lecture
- démo : franchir le bord nord du village bascule sans coupure vers le
  champ de cubes (portail via `g_scene_switch_request` posé par le
  script player), et réciproquement — la scène suivante se re-précharge
  aussitôt
- **Niveau 2** (monde sans couture, zones adjacentes, style Soul
  Reaver) : projet ultérieur ; prérequis = packer VRAM contraint à des
  pages disjointes par zone + design de frontières

## Lot C — VRAM : compaction outillée

Par rapport gain/effort :

1. ~~**Auto-4bpp**~~ — **livré** : sans `"bpp"` explicite dans
   `project.json`, le pipeline compte les couleurs et émet du 4bpp dès
   que l'image tient en 16 couleurs (transparence comprise) — moitié de
   la place, zéro effort. Partout : build projet, import PNG/glTF,
   `psxpipe png2tim`, assets de démo (les 3 textures de la démo passent
   en 4bpp, scènes −44 % sans un pixel de différence au rendu)
2. ~~**Stats dans le VRAM Viewer**~~ — **livré** : % d'occupation,
   poids des textures (et par texture dans la légende), pages libres
   et plus grand bloc contigu de pages de 64×256 mots
3. **Palettes partagées** : quantization conjointe de textures aux
   couleurs proches → une CLUT pour N textures (+ ouvre les palette
   swaps)
4. **Atlas assisté à l'import** : regrouper les petites textures
5. **Texture window** (répétition hardware d'une tuile) : le plus gros
   gain pour les décors répétitifs — touche au format (UV + mode de
   répétition par modèle), à faire en dernier

## Lot D — Panneau « Project » façon Unity

Demandé : un panneau en bas de l'éditeur montrant l'arborescence du
projet (scènes, modèles 3D, textures, scripts, sons) comme la vue
Project/Assets d'Unity — navigation par dossiers, vignettes, et à
terme drag & drop vers la hiérarchie/le viewport.

1. **Explorateur** : lister `assets/` + `scenes/` du projet ouvert
   (Tauri fs), arbre de dossiers à gauche, grille de vignettes à droite
2. **Vignettes** : icône par type (.gltf/.glb, .png, .vag, .json de
   scène) puis aperçus rendus pour les modèles/textures
3. **Actions** : double-clic = ouvrir la scène / importer l'asset,
   menu contextuel (réimporter, subdivision, supprimer)
4. **Drag & drop** vers la hiérarchie (instancier un modèle comme
   entité) — dépend du 1

## Divers (good first issues, voir CONTRIBUTING.md)

Grille au sol du viewport, sélecteur de piste CD-DA par scène, LOD par
distance, icônes SVG des gizmos, `psxpipe info` pour les .vag, undo en
mode visionneuse, presets de subdivision au drop.

## Règles de livraison (rappel)

- Tout changement de format passe par les octets réservés ou une
  version ; writers Rust, parser TS, C runtime, spec et tests bougent
  ensemble.
- Chaque lot se termine par : suite de tests verte, preuve visuelle
  (previewer/screenshot), guide mis à jour, test utilisateur sur
  émulateur réel.
