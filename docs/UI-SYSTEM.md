# Système UI — conception (Lot E)

> Objectif : composer des interfaces (HUD, menus, dialogues) **comme
> avec l'UI de Unity (uGUI)** — un **Canvas**, des widgets portés par un
> **RectTransform** (ancres min/max, pivot), des **Layout Groups**
> verticaux/horizontaux qui rangent leurs enfants tout seuls — éditées
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

- **Écran fixe 320×240** (NTSC, comme le reste du projet). Les ancres
  servent à composer (centrer, coller aux bords, étirer), pas à faire
  du responsive multi-résolutions.
- **Pas de souris** : la navigation est au **D-pad + boutons** — le
  focus est un concept de premier ordre, pas une option d'accessibilité.
- Les textures UI vivent dans la **même VRAM** que le reste : atlas et
  polices passent par le packer existant et comptent dans les stats du
  VRAM Viewer.
- Budget : chaque glyphe est une primitive. Un HUD raisonnable
  (~100 primitives) est négligeable ; un pavé de 500 caractères ne
  l'est pas — le dialogue actuel (`Dialog_Draw`) le montre déjà.

## 2. Le RectTransform (fidèle à Unity)

Chaque widget porte un RectTransform résolu contre le **rectangle de
son parent**, avec exactement la sémantique uGUI :

- `anchor_min`, `anchor_max` : fractions du rect parent, `[0..1]` sur
  chaque axe. Min == max sur un axe → le widget est **posé** (sa taille
  est la sienne) ; min != max → il est **étiré** entre les deux ancres
  (sa « taille » devient des marges). Les 9 presets + modes étirés de
  la grille Unity ne sont que des raccourcis vers ces valeurs — elles
  restent librement éditables.
- `pivot` : point du widget (fractions `[0..1]`) aligné sur l'ancre et
  centre de ses futurs effets (fill, échelle éventuelle).
- `position` : décalage en pixels du pivot par rapport à l'ancre
  (l'`anchoredPosition` de Unity).
- `size` : taille en pixels sur les axes posés ; sur un axe étiré,
  `position`/`size` deviennent les marges gauche/droite (ou haut/bas),
  comme le `sizeDelta` Unity.

Côté fichier et runtime, les fractions sont stockées en **4.12**
(0..4096) : la résolution d'un rect est une poignée de multiplications
entières et de décalages — pas de flottants sur PS1. L'éditeur affiche
des valeurs 0..1 à la Unity.

**Ordre de dessin** = ordre du fichier (parent avant enfant, le dernier
au-dessus), comme la hiérarchie Unity.

## 3. Les widgets (v1)

| Type | Équivalent Unity | Propriétés spécifiques |
|---|---|---|
| `canvas` | Canvas | racine unique, rect de référence 320×240 |
| `panel` | Panel / empty + Image | aplat couleur optionnel, semi-trans |
| `image` | Image | texture, rect UV dans l'atlas, teinte, semi-trans, **`fill`** : `none` / `horizontal` / `vertical` + `amount` (0..1) — les jauges à la Unity (barre de vie = Image *Filled*) |
| `text` | Text | police, chaîne, teinte, alignement |
| `button` | Button | états normal/focus (teinte ou UV alternatives), voisins de navigation D-pad |
| `vlist` | **Vertical Layout Group** | `padding` [g, h, d, b], `spacing`, `child_align` (9 positions), `expand_w`/`expand_h` |
| `hlist` | **Horizontal Layout Group** | idem, axe horizontal |

Un `vlist`/`hlist` est un conteneur : il **range ses enfants
lui-même** — leurs ancres/positions sont ignorées et recalculées chaque
frame (empilés dans l'ordre de la hiérarchie, avec espacement et
alignement), exactement comme un Layout Group Unity prend le contrôle
des RectTransforms enfants. La taille de chaque enfant reste la sienne,
sauf si `expand_w`/`expand_h` la force à remplir l'axe croisé.

Volontairement pas en v1 (le format les permet via flags/réservés) :
Grid Layout Group, Content Size Fitter, 9-slice, scroll, masques,
animations. Ajoutés quand un vrai besoin les tire.

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
      "anchor_min": [0, 0], "anchor_max": [0, 0], "pivot": [0, 0],
      "position": [8, 8], "size": [64, 12],
      "texture": "hud_atlas", "uv": [0, 0] },
    { "name": "vie", "type": "image", "parent": "vie_fond",
      "anchor_min": [0, 0], "anchor_max": [1, 1],
      "position": [2, 2], "size": [2, 2],
      "color": [200, 40, 40], "fill": "horizontal", "amount": 1.0 },

    { "name": "menu", "type": "vlist", "parent": "racine",
      "anchor_min": [0.5, 0.5], "anchor_max": [0.5, 0.5],
      "pivot": [0.5, 0.5], "position": [0, 0], "size": [120, 90],
      "padding": [6, 6, 6, 6], "spacing": 4, "child_align": "top-center",
      "expand_w": true },
    { "name": "btn_jouer", "type": "button", "parent": "menu",
      "size": [0, 18], "font": "main", "text": "JOUER" },
    { "name": "btn_options", "type": "button", "parent": "menu",
      "size": [0, 18], "font": "main", "text": "OPTIONS" },
    { "name": "btn_quitter", "type": "button", "parent": "menu",
      "size": [0, 18], "font": "main", "text": "QUITTER" }
  ]
}
```

(Les enfants du `vlist` n'ont ni ancre ni position : le groupe les
range. `size` [0, 18] + `expand_w` : hauteur fixe, largeur remplie.)

## 4. Format binaire `.PUI` (SceneFormat, même philosophie)

Un fichier par layout, embarquant ses blobs comme le `.psc` : en-tête,
table des textures (TIM), table des polices, table de chaînes, table
des widgets. Écrit par psxpipe, parsé par le runtime C **et** par
l'éditeur TS (mêmes tests croisés que `formats.test.ts`).

Enregistrement de widget à **taille fixe 32 octets** :

```
0x00 u8   type            0x01 u8   flags (semi-trans, visible, focusable, fill h/v)
0x02 u16  parent          (0xFFFF = racine)
0x04 u16  anchor_min_x    0x06 u16  anchor_min_y     (4.12, 0..4096)
0x08 u16  anchor_max_x    0x0A u16  anchor_max_y
0x0C u16  pivot_x         0x0E u16  pivot_y
0x10 i16  pos_x           0x12 i16  pos_y            (anchoredPosition / marges)
0x14 i16  size_w          0x16 i16  size_h           (taille / marges)
0x18 u8   r, g, b         0x1B u8   asset            (texture ou police)
0x1C u16  data            (rect UV packé, offset chaîne, amount 4.12)
0x1E u16  extra           (nav focus packée / padding+spacing+align des listes)
```

Le packing exact de `data`/`extra` par type (et les
`static_assert`/tests qui le verrouillent) sera fixé dans
`docs/UI-FORMAT.md` au moment de l'implémentation.

### Polices `.fnt`

Une police = un TIM (atlas de glyphes, 4bpp presque toujours) + une
table : premier caractère, nombre, largeur/hauteur de cellule, chasse
par glyphe (police proportionnelle). psxpipe gagne `font2fnt` : un PNG
en grille régulière → `.fnt` + TIM (chasses mesurées sur les pixels
non vides, forçables). Charset v1 : ASCII 32-126 + accents français
(é è ê à ç ù ô î ï û … via une seconde rangée).

## 5. Runtime C (`engine/ui.c`)

```c
int      Ui_Load(UiLayout* ui, const char* cd_path);   /* arène, comme Scene_LoadFromCd */
uint8_t* Ui_Draw(UiLayout* ui, uint32_t* ot, uint8_t* packet);  /* primitives en OT[0] */

UiWidget* Ui_Find(UiLayout* ui, const char* name);      /* hash, comme les scripts */
void      Ui_SetText(UiWidget* w, const char* text);    /* chaîne dynamique (buffer par widget) */
void      Ui_SetFill(UiWidget* w, int amount_412);      /* images Filled (jauges) */
void      Ui_SetVisible(UiWidget* w, int visible);
void      Ui_SetTint(UiWidget* w, uint8_t r, uint8_t g, uint8_t b);
```

- **Résolution des rects** : une passe descendante par frame, parents
  d'abord (l'ordre du fichier le garantit) : ancres 4.12 × rect parent
  (multiplications + `>> 12`), puis les Layout Groups **écrasent** les
  rects de leurs enfants (empilement + espacement + alignement) —
  mêmes règles que l'éditeur, au bit près.
- **Focus** : `Ui_FocusInit/Ui_FocusMove(dir)/Ui_Focused()` — la
  navigation D-pad suit les voisins du format (calculés par l'éditeur
  géométriquement — et automatiques dans une liste —, forçables à la
  main). Le jeu décide quoi faire de X/O : le moteur ne capture pas
  l'input, il expose l'état.
- **Scripts** : les scripts d'entité existants pilotent l'UI par l'API
  (`Ui_Find` + setters) — pas de nouveau système d'événements en v1.
  Le dialogue actuel (`Dialog_Show`) migrera vers un layout UI à terme.
- Les textes dynamiques vivent dans un petit buffer par widget flagué
  « dynamique » dans l'éditeur (sinon la chaîne du fichier est utilisée
  telle quelle, zéro RAM).

## 6. Éditeur : le mode Canvas

Un troisième mode de vue à côté de Scène/VRAM : **UI**.

- **Canvas 320×240** rendu au pixel (même zoom entier que le viewport),
  fond au choix : damier, couleur, ou **capture de la scène courante**
  derrière le HUD.
- **Palette de widgets** : les types ci-dessus, glissés sur le canvas —
  même geste que le panneau Project vers le viewport. Glisser un widget
  **dans un `vlist`/`hlist`** l'insère dans la liste (avec l'aperçu de
  la position d'insertion, comme Unity).
- **Hiérarchie de widgets** (le panneau existant, réutilisé) et
  **inspecteur à cartes** : carte **RectTransform** avec la grille de
  presets d'ancres 3×3 + étirés **et** les champs min/max/pivot
  éditables (comme Unity : le preset n'est qu'un raccourci), carte du
  type (texture + rect UV choisi visuellement dans l'atlas, texte +
  police, fill + amount, padding/spacing/alignement des listes).
- **Manipulation** : déplacement à la souris (snap 1px, Ctrl = 8px),
  poignées de redimensionnement, flèches clavier = 1px ; les enfants
  d'une liste ne se déplacent pas à la main (la liste les range), on
  les **réordonne** par drag dans la hiérarchie. Ctrl+Z/Y,
  copier/coller, dupliquer — la mécanique undo existante.
- **Panneau Project** : les `.json` d'`ui/` apparaissent (section UI),
  double-clic = ouvrir dans le mode Canvas, « Créer ▸ UI » rejoint le
  sous-menu Créer.
- **Aperçu focus** : une liste déroulante « état » (normal / focus sur
  tel bouton) pour prévisualiser les états sans lancer le jeu.

## 7. Pipeline

- `project.json` gagne `"ui": ["ui/hud.json"]` et `"fonts"` ;
- `psxpipe build` : `ui/*.json` → `HUD.PUI` sur l'ISO (cache
  incrémental comme le reste) ; les TIM d'UI passent par le packer
  VRAM (l'auto-4bpp fait déjà le bon choix pour des atlas d'icônes) ;
- `psxpipe font2fnt police.png -o main.fnt --cell 8x12` ;
- `gen_project` ajoute une police de démo et un HUD d'exemple au
  village (vie en Image *Filled* + score + prompt « X PARLER »
  au-dessus du PNJ) et un petit menu en `vlist` (pause).

## 8. Jalons de livraison

1. **Fondations** : format `.PUI` + `.fnt` (spec `UI-FORMAT.md`,
   writer Rust, parsers TS/C, tests croisés), `font2fnt`, runtime
   `Ui_Load/Ui_Draw` avec RectTransform complet (ancres/pivot/étirement)
   + canvas/panel/image (fill compris)/text, HUD statique dans la démo.
   *Vérifiable au previewer + émulateur.*
2. **Layout Groups + mode Canvas éditeur** : `vlist`/`hlist` résolus
   dans le runtime et l'éditeur, rendu du layout, hiérarchie/inspecteur
   (carte RectTransform à la Unity), manipulation souris, sauvegarde
   JSON, panneau Project.
3. **Interactif** : boutons + focus D-pad (navigation automatique dans
   les listes), API scripts (`Ui_Find`/setters), le dialogue de démo
   migré en layout UI.
4. **Confort** (au besoin) : Grid Layout, Content Size Fitter, 9-slice,
   teintes animées par script, live tweaking des RectTransforms.

## 9. Questions ouvertes (à trancher en implémentant)

- Chaînes accentuées : UTF-8 translittéré vers le charset de la police
  à la conversion (probable), ou charset 8 bits fixe ?
- Un layout par scène (chargé avec elle) **et/ou** un layout global
  (HUD persistant à travers le streaming) — le format le permet, le
  runtime devra choisir où vivre (arène scène vs buffer dédié).
- Rotation d'images (POLY_FT4) : utile pour aiguilles/compas, mais
  casse la simplicité SPRT — probablement flag v2.
- `expand` des listes : faut-il aussi « control child size » complet
  (la liste impose la taille sur l'axe principal) comme Unity, ou
  seulement l'axe croisé en v1 ?

## 10. Règles (rappel projet)

Comme tout changement de format : spec + writer Rust + parser TS +
runtime C + tests bougent ensemble ; chaque jalon se conclut par une
preuve visuelle (previewer/Playwright/émulateur) et la mise à jour de
GETTING-STARTED.
