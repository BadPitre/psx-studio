# Feuille de route — après la Phase 6

> Les phases 0→6 du plan initial sont livrées (voir les guides de phase).
> Ce document planifie les chantiers suivants, discutés et validés dans
> leur principe. Ordre indicatif ; chaque lot est livrable indépendamment,
> avec la même exigence : format rétrocompatible, testé des deux côtés,
> vérifiable sans console.

## Lot A — Lumières & caméras (composants d'entité)

Philosophie Unity : une entité *devient* lumière ou caméra, se manipule
au gizmo, s'anime par script, se règle en live tweaking.

1. **Lumières directionnelles ×3** (support natif GTE, lignes 1-2 des
   matrices lumière/couleur aujourd'hui vides)
   - format : bit `light` dans les flags d'entité (réservés v1) + table
     couleurs dans les octets réservés de l'en-tête — recette v1.1
   - direction = rotation de l'entité ; la lumière des settings reste le
     soleil par défaut (rétrocompatible)
   - viewport + previewer Rust : passer de 1 à 3 lumières
2. **Lumières ponctuelles (torches)** — approximation d'époque :
   - par objet dessiné : direction torche→objet + atténuation par la
     distance injectées dans une ligne GTE libre
   - habillage : billboard additif (halo) + script de vacillement
   - vitrine : une torche dans le village de démo
3. **Caméras** : bit `camera`, frustum dans le calque net de l'éditeur,
   vue initiale du runtime prise sur la première caméra de la scène
   (les scripts gardent la main), puis `Camera_Activate()` par script

## Lot B — Streaming (niveau 1 : préchargement)

Objectif : plus aucun écran de chargement entre scènes.

- deux arènes en bascule ; `CdRead` asynchrone de la scène suivante
  pendant le gameplay (poll par frame, ~0,5 s pour 150 Ko à 2×)
- le format y est prêt depuis la Phase 2 : une scène = un fichier
  contigu = une lecture, sans seek
- contrainte à gérer : un seul laser — pause de la musique CD-DA pendant
  le préchargement (les alternatives XA/SPU sont hors lot)
- démo : village → champ de cubes sans coupure
- **Niveau 2** (monde sans couture, zones adjacentes, style Soul
  Reaver) : projet ultérieur ; prérequis = packer VRAM contraint à des
  pages disjointes par zone + design de frontières

## Lot C — VRAM : compaction outillée

Par rapport gain/effort :

1. **Auto-4bpp** : si la quantization tombe à ≤16 couleurs, émettre du
   4bpp automatiquement (moitié de la place, zéro effort utilisateur)
2. **Stats dans le VRAM Viewer** : % d'occupation, plus gros trou
   libre, poids par texture
3. **Palettes partagées** : quantization conjointe de textures aux
   couleurs proches → une CLUT pour N textures (+ ouvre les palette
   swaps)
4. **Atlas assisté à l'import** : regrouper les petites textures
5. **Texture window** (répétition hardware d'une tuile) : le plus gros
   gain pour les décors répétitifs — touche au format (UV + mode de
   répétition par modèle), à faire en dernier

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
