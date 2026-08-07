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

1. ~~**Explorateur**~~ — **livré** : bande repliable sous le viewport,
   **arborescence de dossiers libre** à gauche (l'utilisateur crée ses
   dossiers — « Créer ▸ Dossier » — et range par **drag & drop** d'une
   tuile vers un dossier : le fichier bouge sur disque, project.json est
   réécrit, le .bin d'un .gltf suit, la scène ouverte est rouverte à son
   nouveau chemin ; l'import d'un fichier déjà dans le projet respecte
   son emplacement), grille du dossier + recherche à droite. Le contenu
   croise le disque et `project.json` : badge « non importé » (fichier
   présent, pas enregistré) et « manquant » (enregistré, disparu).
   Onglet **Console** à côté : journal horodaté des imports, builds et
   erreurs (⚠ sur l'onglet si erreur), bouton Effacer
2. ~~**Vignettes**~~ — **livré** : aperçus rendus depuis les fichiers
   convertis de Library/ (les octets que la console verra) — modèles en
   rendu three.js offscreen cadré sur la sphère englobante, texture
   appariée par nom de sortie (guy.pmd → guy.tim), textures = TIM
   décodé (palette quantifiée réelle) en nearest ; cache par
   (chemin, taille), repli sur l'icône par type
3. ~~**Actions**~~ — **livré** : double-clic = ouvrir la scène /
   importer un asset non enregistré ; menu contextuel Ouvrir /
   (Ré)importer / Rafraîchir. Restent : subdivision et retrait
4. ~~**Menu « Créer ▸ »**~~ — **livré** au clic droit dans le vide,
   comme Unity : Scène (nom → `scenes/<slug>.json` enregistré et
   ouvert), sous-menu pensé extensible (prefabs, bases de données,
   etc. viendront s'y ajouter)
5. ~~**Drag & drop** vers la scène~~ — **livré** : glisser une tuile de
   modèle vers le viewport instancie une entité à l'endroit visé au sol
   (raycast sur le plan y=0), vers la hiérarchie = à l'origine. Le
   modèle (et sa texture appariée) est ajouté aux assets de la scène si
   besoin, l'entité est nommée et sélectionnée, annulable au Ctrl+Z

## Lot E — Système UI (dans la scène, façon uGUI Unity)

Composer HUD, menus et dialogues **comme dans Unity, pas comme
Unreal** : l'UI vit dans la scène — le Canvas est une entité de la
hiérarchie, les éléments UI sont des entités enfants à **composants**
(RectTransform fidèle : ancres min/max + presets, pivot, étirement ;
Image avec les **4 Image Types de Unity** — Simple, Sliced 9-slice,
Tiled, Filled pour les jauges ; Text ; Button ; **Layout Groups**
verticaux/horizontaux qui rangent leurs enfants). Édition dans les
panneaux existants (hiérarchie, inspecteur à cartes, viewport en
surimpression 2D), sérialisation dans le `.psc` (SceneFormat v1.3,
table UI dans les réservés) + polices bitmap `.fnt`, runtime `Ui_Draw`
en tête d'OT, navigation au D-pad (pas de souris sur PS1 : le focus
est un concept de premier ordre).

Un canvas se sauvegarde aussi en **Prefab** réutilisable entre les
scènes (même schéma d'entités, inliné dans le `.psc` au build — la
console ne connaît pas les prefabs), édité isolément en **Prefab
Mode** avec fil d'Ariane, instancié en bleu dans la hiérarchie ou par
drag depuis le panneau Project. Mécanisme générique : les prefabs 3D
l'utiliseront plus tard.

**Spec complète : [`UI-SYSTEM.md`](UI-SYSTEM.md)** — 5 jalons :
~~fondations (format v1.3 + RectTransform + runtime + HUD de démo)~~
— **livré** (table UI 40 o/widget + polices .fnt dans le .psc, offsets
dérivés des réservés ; font2fnt + police de démo 5×7 ; Ui_Draw en
OT[0] : rects 4.12, aplats, Image Filled, texte aligné ; HUD village
et champ ; parsers TS/C + previewer en parité, testés),
~~jalon 2 (édition dans l'éditeur)~~ — **livré** (calque HUD du
viewport, mode Canvas dédié — vue séparée, fond damier, sélection +
déplacement souris —, cartes RectTransform/Canvas/Image/Text, menu ＋
UI, conversion auto de Library/, resize du panneau Project, **Layout
Groups** verticaux/horizontaux et **Sliced/Tiled** dans toute la
pile),
~~jalon 3 (prefabs vivants + Prefab Mode)~~ — **livré** (référence
`{ "prefab": "prefabs/x.json" }` dans la scène, inlinée au build par
psxpipe — assets fusionnés, enfants `instance.enfant`, racine qui
prend transform/nom/parent de l'instance, prefabs imbriqués rejetés ;
instances en **bleu** 🧩 dans la hiérarchie, enfants en lecture seule,
carte Prefab dans l'inspecteur ; **Prefab Mode** : ouverture isolée
d'un prefab depuis le panneau Project ou l'inspecteur, fil d'Ariane
`‹ scène 🧩 prefab`, sauvegarde = toutes les scènes suivent),
~~jalon 4 (interactif)~~ — **livré** (composant `button` bit 3 dans
toute la pile ; focus D-pad au runtime : navigation géométrique entre
boutons visibles + surlignage du focalisé ; `Ui_SetText` et API
scripts sur la scène courante ; démo branchée : jauge de vie pilotée
par script, menu pause à boutons START/D-pad/X, dialogue migré en
canvas Sliced),
reste : confort (Grid Layout, fill radial, live tweaking des
RectTransforms, overrides d'instance avancés).

## Lot F — PSX Script (le gameplay sans C)

Un langage de script maison compilé en **bytecode** par psxpipe et
interprété par une petite **VM à registres** du runtime — l'architecture
des jeux de l'époque (FF7), pas un interpréteur générique : entiers
seulement, zéro allocation/GC, opcodes qui appellent le C natif, bac à
sable (bornes, division par zéro, garde-fou de boucle). Spec :
[`PSX-SCRIPT.md`](PSX-SCRIPT.md).

~~Jalon 1 (fondations)~~ — **livré** : compilateur complet
(`psxpipe::psxs`, erreurs françaises avec lignes), format PSB1 embarqué
dans le `.psc` (v1.5 : bit 0 des flags + table d'offsets après les
hashes — un blob prime sur le registre C, rétrocompatible), VM
`engine/vm.c` (16 registres/entité, 24 instances, liée par le jeu
seulement), langage à mots-clés anglais : var / on start / every frame /
if-else / while / and-or-not + 17 fonctions moteur (transforms, move
avec collisions, boutons, distance, find, dialog, show, switch_scene,
random), girouette de démo (spinner.psxs) dans le village, banc d'essai
natif hôte (le vrai blob exécuté par la vraie VM : 100 000 frames en
7 ms).

Suite : API UI/audio (jauge, texte, sons), rechargement à chaud pendant
que l'émulateur tourne, éditeur de texte intégré avec coloration,
« Créer ▸ Script » dans le panneau Project, visual scripting par-dessus
le même bytecode.

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
