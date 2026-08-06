# Système UI — conception (Lot E)

> Objectif : composer des interfaces (HUD, menus, dialogues) comme dans
> UMG d'Unreal Engine — un **Canvas** sur lequel on pose des **widgets**
> (images, textes, jauges, boutons), ancrés et hiérarchisés — éditées
> visuellement dans PSX Studio et rendues par le runtime console.
> Ce document est la spec de référence ; rien n'est encore implémenté.

## 1. Ce que la PS1 sait faire (et pas faire)

Le rendu 2D console est fait de primitives GPU insérées en **tête
d'ordering table** (OT index 0 = dessiné en dernier, donc au-dessus de
la 3D) :

| Besoin | Primitive | Notes |
|---|---|---|
| Image / icône | `SPRT` (+ `DR_TPAGE`) | le plus rapide, pas de GTE ; pas de rotation ni d'échelle |
| Image tournée/étirée | `POLY_FT4` | quad texturé affine |
| Aplat / fond | `TILE`, `POLY_F4` | couleur unie |
| Texte | 1 `SPRT` par glyphe | police = atlas TIM + table de chasse |
| Transparence | bit semi-trans + mode ABR (0 : moyenne, 1 : additif, 2 : soustractif) | **pas d'alpha 0-255** : 4 modes fixes, c'est tout |
| Teinte | couleur de primitive (128 = neutre, ×2 max) | assombrir ou pousser une couleur |

Contraintes structurantes :

- **Écran fixe 320×240** (NTSC ; on garde 320×240 partout comme le
  reste du projet). Les ancres restent utiles pour centrer/coller aux
  bords, pas pour du responsive.
- **Pas de souris** : la navigation est au **D-pad + boutons** — le
  focus est un concept de premier ordre, pas une option d'accessibilité.
- Les textures UI vivent dans la **même VRAM** que le reste : l'atlas
  UI et la police passent par le packer existant et comptent dans les
  stats du VRAM Viewer.
- Budget : chaque glyphe est une primitive. Un HUD raisonnable (~100
  primitives) est négligeable ; un pavé de texte de 500 caractères ne
  l'est pas — le dialogue actuel (`Dialog_Draw`) le montre déjà.

## 2. Modèle de données : le widget tree

Un fichier UI (« layout ») est un **arbre de widgets**, comme une scène
est un arbre d'entités — mêmes invariants (parent avant enfant, ordre
du fichier = ordre de dessin, donc le dernier est au-dessus).

### Types de widgets (v1)

| Type | Rôle | Propriétés spécifiques |
|---|---|---|
| `canvas` | racine (unique) | taille de référence 320×240 |
| `panel` | groupe/conteneur, aplat optionnel | couleur, semi-trans |
| `image` | sprite depuis un atlas | texture, rect UV, teinte, semi-trans |
| `text` | texte en police bitmap | police, chaîne, teinte, alignement |
| `gauge` | barre de progression | valeur 0-4096, direction, couleurs fond/remplissage |
| `button` | zone focusable | états normal/focus (teinte ou UV décalées), voisins de navigation |

Volontairement pas en v1 : 9-slice, scroll, masques, animations de
timeline. Le format les permettra (octets réservés), on les ajoutera
quand un vrai besoin les tire.

### Ancres et géométrie (le cœur « UMG »)

Chaque widget se place par rapport au **rectangle de son parent** :

- `anchor` : préréglage parmi 9 positions (coins, milieux de bords,
  centre) + 3 modes étirés (horizontal, vertical, plein) — comme la
  grille de presets d'UMG/Unity ;
- `offset` : `[x, y]` en pixels depuis l'ancre (ou marges
  `[gauche, haut, droite, bas]` pour les modes étirés) ;
- `size` : `[l, h]` en pixels (ignoré sur les axes étirés) ;
- `pivot` : point du widget posé sur l'ancre (mêmes 9 presets).

Tout est **entier, en pixels d'écran 320×240**. Pas de pourcentages ni
de flottants : l'écran est fixe, et le runtime doit résoudre un rect en
quelques additions.

### JSON (édité par l'éditeur, versionné dans `ui/`)

```json
{
  "name": "hud",
  "assets": {
    "textures": [ { "id": "hud_atlas", "tim": "hud.tim" } ],
    "fonts":    [ { "id": "main", "fnt": "main.fnt" } ]
  },
  "widgets": [
    { "name": "racine", "type": "canvas" },
    { "name": "vie_fond", "type": "image", "parent": "racine",
      "anchor": "top-left", "offset": [8, 8], "size": [64, 12],
      "texture": "hud_atlas", "uv": [0, 0] },
    { "name": "vie", "type": "gauge", "parent": "vie_fond",
      "anchor": "stretch", "margins": [2, 2, 2, 2],
      "value": 4096, "fill": [200, 40, 40], "back": [20, 20, 20] },
    { "name": "score", "type": "text", "parent": "racine",
      "anchor": "top-right", "offset": [-8, 8], "pivot": "top-right",
      "font": "main", "text": "SCORE 0" }
  ]
}
```

## 3. Format binaire `.PUI` (SceneFormat, même philosophie)

Un fichier par layout, embarquant ses blobs comme le `.psc` : en-tête,
table des textures (TIM), table des polices, table de chaînes, table
des widgets. Écrit par psxpipe, parsé par le runtime C **et** par
l'éditeur TS (mêmes tests croisés que `formats.test.ts`).

Enregistrement de widget à **taille fixe 24 octets** :

```
0x00 u8   type            0x01 u8   flags (semi-trans mode, visible, focusable)
0x02 u16  parent          (0xFFFF = racine)
0x04 u8   anchor          0x05 u8   pivot
0x06 i16  offset_x        0x08 i16  offset_y
0x0A i16  size_w          0x0C i16  size_h
0x0E u8   r, 0x0F u8 g, 0x10 u8 b   (teinte, 128 = neutre)
0x11 u8   texture/police  (index, selon le type)
0x12 u16  uv/valeur/chaîne (rect UV packé, valeur de jauge, offset chaîne)
0x14 u16  nav             (voisins de focus packés : 4 × 4 bits d'index relatif)
0x16 u16  réservé
```

Les marges des modes étirés réutilisent offset+size. Le détail exact
sera fixé dans `docs/UI-FORMAT.md` au moment de l'implémentation, avec
les `static_assert` C et les tests Rust/TS habituels.

### Polices `.fnt`

Une police = un TIM (atlas de glyphes, 4bpp presque toujours) + une
table : premier caractère, nombre, largeur/hauteur de cellule, chasse
par glyphe (police proportionnelle). psxpipe gagne `font2fnt` : un PNG
en grille régulière → `.fnt` + TIM (chasses mesurées sur les pixels
non vides, forçables). Charset v1 : ASCII 32-126 + accents français
(é è ê à ç ù ô î ï û … via une seconde rangée).

## 4. Runtime C (`engine/ui.c`)

```c
int      Ui_Load(UiLayout* ui, const char* cd_path);   /* arène, comme Scene_LoadFromCd */
uint8_t* Ui_Draw(UiLayout* ui, uint32_t* ot, uint8_t* packet);  /* primitives en OT[0] */

UiWidget* Ui_Find(UiLayout* ui, const char* name);      /* hash, comme les scripts */
void      Ui_SetText(UiWidget* w, const char* text);    /* chaîne dynamique (buffer par widget) */
void      Ui_SetValue(UiWidget* w, int value_412);      /* jauges */
void      Ui_SetVisible(UiWidget* w, int visible);
void      Ui_SetTint(UiWidget* w, uint8_t r, uint8_t g, uint8_t b);
```

- **Résolution des rects** : une passe descendante par frame (parents
  d'abord — l'ordre du fichier le garantit), quelques additions par
  widget, pas de GTE.
- **Focus** : `Ui_FocusInit/Ui_FocusMove(dir)/Ui_Focused()` — la
  navigation D-pad suit les voisins du format (calculés par l'éditeur
  géométriquement, forçables à la main). Le jeu décide quoi faire de
  X/O : le moteur ne capture pas l'input, il expose l'état.
- **Scripts** : les scripts d'entité existants pilotent l'UI par l'API
  (`Ui_Find` + setters) — pas de nouveau système d'événements en v1.
  Le dialogue actuel (`Dialog_Show`) migrera vers un layout UI à terme.
- Les textes dynamiques vivent dans un petit buffer par widget flagué
  « dynamique » dans l'éditeur (sinon la chaîne du fichier est utilisée
  telle quelle, zéro RAM).

## 5. Éditeur : le mode Canvas

Un troisième mode de vue à côté de Scène/VRAM : **UI**.

- **Canvas 320×240** rendu au pixel (même zoom entier que le viewport),
  fond au choix : damier, couleur, ou **capture de la scène courante**
  derrière le HUD.
- **Palette de widgets** : les types ci-dessus, glissés sur le canvas —
  même geste que le panneau Project vers le viewport.
- **Hiérarchie de widgets** (le panneau existant, réutilisé) et
  **inspecteur à cartes** : carte Transform (ancre visuelle en grille
  3×3 comme UMG, offsets, taille, pivot), carte spécifique au type
  (texture + rect UV choisi visuellement dans l'atlas, texte + police,
  valeur de jauge…).
- **Manipulation** : déplacement à la souris (snap 1px, Ctrl = 8px),
  poignées de redimensionnement, flèches clavier = 1px. Ctrl+Z/Y,
  copier/coller, dupliquer — la mécanique undo/historique existante.
- **Panneau Project** : les `.json` d'`ui/` apparaissent (section UI),
  double-clic = ouvrir dans le mode Canvas, « Créer ▸ UI » rejoint le
  sous-menu Créer.
- **Aperçu focus** : une liste déroulante « état » (normal / focus sur
  tel bouton) pour prévisualiser les états sans lancer le jeu.

## 6. Pipeline

- `project.json` gagne `"ui": ["ui/hud.json"]` et `"fonts"` ;
- `psxpipe build` : `ui/*.json` → `HUD.PUI` sur l'ISO (cache
  incrémental comme le reste) ; les TIM d'UI passent par le packer
  VRAM (l'auto-4bpp fait déjà le bon choix pour des atlas d'icônes) ;
- `psxpipe font2fnt police.png -o main.fnt --cell 8x12` ;
- `gen_project` ajoute une police de démo et un HUD d'exemple au
  village (vie + score + prompt « X PARLER » au-dessus du PNJ).

## 7. Jalons de livraison

1. **Fondations** : format `.PUI` + `.fnt` (spec `UI-FORMAT.md`,
   writer Rust, parsers TS/C, tests croisés), `font2fnt`, runtime
   `Ui_Load/Ui_Draw` avec canvas/panel/image/text, HUD statique dans la
   démo. *Vérifiable au previewer + émulateur.*
2. **Mode Canvas éditeur** : rendu du layout, hiérarchie/inspecteur,
   manipulation souris + ancres, sauvegarde JSON, panneau Project.
3. **Interactif** : jauges, boutons + focus D-pad, API scripts
   (`Ui_Find`/setters), le dialogue de démo migré en layout UI.
4. **Confort** (au besoin) : 9-slice, teintes animées par script,
   live tweaking des offsets comme les transforms d'entités.

## 8. Questions ouvertes (à trancher en implémentant)

- Chaînes accentuées : UTF-8 translittéré vers le charset de la police
  à la conversion (probable), ou charset 8 bits fixe ?
- Un layout par scène (chargé avec elle) **et/ou** un layout global
  (HUD persistant à travers le streaming) — le format le permet, le
  runtime devra choisir où vivre (arène scène vs buffer dédié).
- Rotation d'images (POLY_FT4) : utile pour aiguilles/compas, mais
  casse la simplicité SPRT — probablement flag v2.

## 9. Règles (rappel projet)

Comme tout changement de format : spec + writer Rust + parser TS +
runtime C + tests bougent ensemble ; chaque jalon se conclut par une
preuve visuelle (previewer/Playwright/émulateur) et la mise à jour de
GETTING-STARTED.
