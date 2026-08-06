# Système UI — conception (Lot E)

> Objectif : des interfaces (HUD, menus, dialogues) construites **comme
> dans Unity (uGUI)** — et pas comme les Widget Blueprints d'Unreal :
> **l'UI vit dans la scène**. Un **Canvas est une entité** de la
> hiérarchie ; les éléments UI sont des **entités enfants** portant des
> **composants** (RectTransform, Image, Text, Button, Layout Group)
> édités dans la même hiérarchie et le même inspecteur à cartes que le
> reste. Et comme dans Unity, un canvas peut être **sauvegardé en
> Prefab** — réutilisable dans d'autres scènes, édité isolément en
> **Prefab Mode** (son propre viewport).
> Ce document est la spec de référence ; rien n'est encore implémenté.

## 1. Philosophie (ce qui fait « Unity »)

- **Pas de format UI à part** : les widgets sont des entités — dans le
  `scene.json` d'une scène, ou dans un prefab (même schéma d'entités,
  §7) — nommés et hiérarchisés dans le panneau Hiérarchie existant, et
  compilés dans le `.psc` avec le reste.
- **Tout est composant** : une entité sous un Canvas porte un
  `RectTransform` (qui **remplace** sa carte Transform 3D dans
  l'inspecteur, comme dans Unity) plus des composants au choix —
  `Image`, `Text`, `Button`, `Layout Group`. Le bouton
  « ＋ Ajouter un composant » les liste avec les autres.
- **Le Canvas est un composant** d'entité (`canvas`), équivalent du
  Canvas *Screen Space – Overlay* : ses enfants sont dessinés en
  coordonnées écran 320×240, au-dessus de la 3D. Une scène peut en
  avoir plusieurs (HUD, menu pause), activables par script.
- **Édition dans la vue de scène** : sélectionner une entité UI fait
  passer le viewport en surimpression 2D au pixel (le canvas rendu
  par-dessus la 3D, comme Unity affiche l'UI dans la Scene View), avec
  déplacement/redimensionnement à la souris.
- **Prefabs** : un canvas (ou n'importe quel sous-arbre d'entités UI)
  se sauvegarde en **Prefab** réutilisable — instancié dans plusieurs
  scènes, édité isolément en **Prefab Mode** ; voir §7.

## 2. Ce que la PS1 sait faire (et pas faire)

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
- **Pas de souris en jeu** : la navigation est au **D-pad + boutons**
  — le focus est un concept de premier ordre.
- Les textures UI vivent dans la **même VRAM** que le reste : atlas et
  polices passent par le packer existant et comptent dans les stats du
  VRAM Viewer.
- Budget : chaque glyphe est une primitive. Un HUD raisonnable
  (~100 primitives) est négligeable ; un pavé de 500 caractères ne
  l'est pas — le dialogue actuel (`Dialog_Draw`) le montre déjà.

## 3. Le composant RectTransform (fidèle à uGUI)

Sous un Canvas, chaque entité UI est placée par son RectTransform,
résolu contre le **rectangle du parent**, avec la sémantique Unity :

- `anchor_min`, `anchor_max` : fractions du rect parent, `[0..1]` par
  axe. Min == max sur un axe → le widget est **posé** (sa taille est la
  sienne) ; min != max → **étiré** entre les deux ancres (position et
  taille deviennent des marges, comme le `sizeDelta`). La grille de
  presets 3×3 + modes étirés n'est qu'un jeu de raccourcis — les
  valeurs restent librement éditables.
- `pivot` : point du widget (fractions `[0..1]`) aligné sur l'ancre,
  centre des effets (fill…).
- `position` : décalage en pixels du pivot par rapport à l'ancre
  (l'`anchoredPosition`).
- `size` : taille en pixels sur les axes posés ; marges sur les axes
  étirés.

Stockage en **4.12** (0..4096) : la résolution d'un rect est une
poignée de multiplications entières et de décalages — pas de flottants
console. L'éditeur affiche du 0..1 à la Unity.

**Ordre de dessin** = ordre de la hiérarchie (parent avant enfant, le
dernier au-dessus), comme dans Unity.

## 4. Les composants UI (v1)

| Composant | Équivalent Unity | Propriétés |
|---|---|---|
| `canvas` | Canvas (Screen Space – Overlay) | actif/inactif ; rect de référence 320×240 |
| `image` | Image | texture + rect UV dans l'atlas (le « sprite »), teinte, semi-trans, et **`type`** — les quatre Image Types de Unity, voir ci-dessous ; sans texture = aplat coloré (Panel) |
| `text` | Text | police, chaîne, teinte, alignement |
| `button` | Button | focusable, états normal/focus (teinte ou UV alternatives), voisins de navigation D-pad |
| `layout` | Vertical/Horizontal **Layout Group** | `axis` : `vertical`/`horizontal`, `padding` [g, h, d, b], `spacing`, `child_align` (9 positions), `expand_w`/`expand_h` |

### Le composant Image : les quatre types de Unity

Comme dans l'inspecteur Unity, une Image a un **Image Type** :

| `type` | Unity | Rendu console | Propriétés |
|---|---|---|---|
| `simple` | Simple | 1 `SPRT` à la taille du sprite, ou 1 `POLY_FT4` si le rect étire le sprite | — |
| `sliced` | Sliced (9-slice) | 9 primitives : 4 coins `SPRT` intacts, bords et centre étirés (`POLY_FT4`) | `border` [g, h, d, b] en texels — les marges insécables du sprite (cadres de fenêtres, panneaux redimensionnables) |
| `tiled` | Tiled | grille de `SPRT` répétant le sprite (très économe : pas de GTE) | — |
| `filled` | Filled | `SPRT`/`POLY_FT4` tronqué + UV recadrées | `fill` : `horizontal`/`vertical`, `amount` (0..1) — les jauges (barre de vie) ; `Ui_SetFill()` par script |

Le « sprite » est un rect UV dans l'atlas de la texture, choisi
visuellement dans l'éditeur (avec les poignées de `border` pour le mode
`sliced`, comme le Sprite Editor).

Un composant `layout` fait de l'entité un conteneur qui **range ses
enfants lui-même** : leurs ancres/positions sont ignorées et
recalculées chaque frame (empilement dans l'ordre de la hiérarchie,
espacement, alignement), exactement comme un Layout Group Unity prend
le contrôle des RectTransforms enfants. La taille de chaque enfant
reste la sienne, sauf `expand_w`/`expand_h` qui remplit l'axe croisé.

Volontairement pas en v1 (permis par le format via flags/réservés) :
Grid Layout Group, Content Size Fitter, fill radial, scroll, masques,
animations. Ajoutés quand un vrai besoin les tire.

### Dans le `scene.json` (mêmes entités, nouveaux composants)

```json
{
  "entities": [
    { "name": "hud", "canvas": true },

    { "name": "vie_fond", "parent": "hud",
      "rect": { "anchor_min": [0, 0], "anchor_max": [0, 0],
                "pivot": [0, 0], "position": [8, 8], "size": [64, 12] },
      "image": { "texture": "hud_atlas", "uv": [0, 0] } },
    { "name": "vie", "parent": "vie_fond",
      "rect": { "anchor_min": [0, 0], "anchor_max": [1, 1],
                "position": [2, 2], "size": [2, 2] },
      "image": { "color": [200, 40, 40], "type": "filled",
                 "fill": "horizontal", "amount": 1.0 } },

    { "name": "menu_pause", "canvas": true, "active": false },
    { "name": "menu", "parent": "menu_pause",
      "rect": { "anchor_min": [0.5, 0.5], "anchor_max": [0.5, 0.5],
                "pivot": [0.5, 0.5], "position": [0, 0], "size": [120, 90] },
      "image": { "texture": "hud_atlas", "uv": [64, 0, 24, 24],
                 "type": "sliced", "border": [8, 8, 8, 8] },
      "layout": { "axis": "vertical", "padding": [6, 6, 6, 6],
                  "spacing": 4, "child_align": "top-center", "expand_w": true } },
    { "name": "btn_jouer", "parent": "menu",
      "rect": { "size": [0, 18] },
      "text": { "font": "main", "text": "REPRENDRE" }, "button": true },
    { "name": "btn_quitter", "parent": "menu",
      "rect": { "size": [0, 18] },
      "text": { "font": "main", "text": "QUITTER" }, "button": true }
  ]
}
```

Les textures et polices UI sont des assets de scène comme les autres
(`assets.textures` + nouvelle liste `assets.fonts`).

## 5. SceneFormat v1.3 (pas de fichier séparé)

L'UI est **dans le `.psc`**, comme les lumières l'ont été : les entités
UI restent des enregistrements d'entité normaux (nom, parent, ordre
topologique — tout existe déjà) flagués `ENTITY_FLAG_UI` (bit 3), et
une **table UI** dans les octets réservés de l'en-tête porte un
enregistrement de composants par entité UI :

```
0x00 u16  entity          (index dans la table d'entités)
0x02 u8   components      (bits : canvas, image, text, button, layout, actif)
0x03 u8   flags           (semi-trans, type d'image ×4, fill h/v, axe du layout, expand…)
0x04 u16  anchor_min_x    0x06 u16  anchor_min_y     (4.12)
0x08 u16  anchor_max_x    0x0A u16  anchor_max_y
0x0C u16  pivot_x         0x0E u16  pivot_y
0x10 i16  pos_x           0x12 i16  pos_y
0x14 i16  size_w          0x16 i16  size_h
0x18 u8   r, g, b         0x1B u8   asset            (texture ou police)
0x1C u16  data            (offset chaîne / amount 4.12)
0x1E u16  extra           (nav focus / padding+spacing+align du layout)
0x20 u8   uv_x, uv_y      0x22 u8  uv_w, uv_h        (le sprite, en texels)
0x24 u8   border g, h, d, b                          (marges 9-slice, en texels)
```

40 octets par widget, table + table de chaînes + polices embarquées
dans le blob scène. Rétrocompatible : offset/count dans les octets
réservés restants de l'en-tête, un runtime ancien ignore la table (les
entités UI n'ayant pas de modèle, il ne dessine rien). Le packing
exact de `flags`/`data`/`extra` et les `static_assert`/tests croisés
seront fixés dans la spec `SCENE-FORMAT.md` au moment de
l'implémentation.

### Polices `.fnt`

Une police = un TIM (atlas de glyphes, 4bpp presque toujours) + une
table : premier caractère, nombre, largeur/hauteur de cellule, chasse
par glyphe (police proportionnelle). psxpipe gagne `font2fnt` : un PNG
en grille régulière → `.fnt` + TIM (chasses mesurées sur les pixels
non vides, forçables). Charset v1 : ASCII 32-126 + accents français
(é è ê à ç ù ô î ï û … via une seconde rangée).

## 6. Runtime C (`engine/ui.c`, données dans la Scene)

```c
/* Appelé par Scene_Draw après la 3D : primitives des canvas actifs en OT[0]. */
uint8_t* Ui_Draw(Scene* scene, uint32_t* ot, uint8_t* packet, uint8_t* limit);

/* Les widgets SONT des entités de la scène courante (engine.h — les
 * scripts s'attachent au canvas et retrouvent leurs widgets par la
 * hiérarchie, cf. scripts hud/dialogue/pause de la démo). */
uint8_t Ui_Components(const Entity* e);          /* masque UI_COMP_*, 0 = pas un widget */
int     Ui_ImageType(const Entity* e);           /* 0 simple, 1 sliced, 2 tiled, 3 filled */

void Ui_SetText(const Entity* e, const char* s); /* chaîne remplacée (buffer du script) */
void Ui_SetFill(const Entity* e, int amount_412);/* images Filled (jauges) */
void Ui_SetActive(const Entity* e, int active);  /* montrer/cacher (canvas compris) */
void Ui_SetTint(const Entity* e, uint8_t r, uint8_t g, uint8_t b);

/* Focus D-pad : navigation géométrique entre boutons visibles ; le
 * moteur surligne le focalisé, le jeu décide quoi faire de X/O. */
void    Ui_FocusInit(void);
int     Ui_FocusMove(int dx, int dy);
Entity* Ui_Focused(void);
void    Ui_FocusClear(void);
```

- **Résolution des rects** : une passe descendante par frame, parents
  d'abord (l'ordre topologique des entités le garantit) : ancres 4.12 ×
  rect parent (multiplications + `>> 12`), puis les Layout Groups
  **écrasent** les rects de leurs enfants — mêmes règles que
  l'éditeur, au bit près.
- **Focus** : `Ui_FocusInit/Ui_FocusMove(dx, dy)/Ui_Focused()` — la
  navigation D-pad est **géométrique au runtime** (le bouton visible le
  plus proche dans la direction demandée, rects de la dernière frame) ;
  pas de table de voisins dans le format en v1. Le moteur surligne le
  bouton focalisé (TILE semi-transparente derrière) ; le jeu décide
  quoi faire de X/O : le moteur expose l'état, il ne capture pas
  l'input.
- **Scripts** : les scripts d'entité existants pilotent l'UI par l'API
  (barre de vie : `Ui_SetFill` dans le script `hud`, pause :
  `Ui_SetActive` + focus dans `pause`) — pas de nouveau système
  d'événements en v1. Le dialogue de démo est rendu par le script
  `dialogue` (canvas Sliced + `Ui_SetText`) ; le fallback
  `Dialog_Draw` s'efface quand un tel canvas se déclare
  (`Dialog_UiCanvas(1)` dans son Start).
- **Live tweaking** : les widgets étant des entités, la balise RAM
  existante s'étend naturellement aux RectTransforms (jalon 4).

## 7. Prefabs UI (réutiliser entre les scènes) — **livré**

Comme dans Unity, un canvas ne vit **pas forcément dans une scène** :
il peut être sauvegardé en **Prefab** et réutilisé partout.

- **Fichier** : `prefabs/<nom>.json` — le même schéma d'entités que le
  `scene.json` (un sous-arbre avec **exactement une racine**, ici
  typiquement un canvas), plus ses besoins d'assets (modèles,
  textures, polices). Un prefab se crée en glissant une entité de la
  hiérarchie vers le panneau Project (le sous-arbre + ses assets sont
  emportés).
- **Instance dans une scène** : une entité-référence
  `{ "name": "hud", "prefab": "prefabs/hud.json" }` — créée en
  glissant le prefab du panneau Project vers la hiérarchie. La racine
  du prefab prend le nom, le parent, le transform et l'état `actif` de
  l'instance ; les enfants sont nommés `hud.enfant`. Dans la
  hiérarchie, l'instance apparaît **en bleu** 🧩 (convention Unity),
  ses enfants en lecture seule (l'inspecteur renvoie vers le prefab) ;
  modifier le prefab met à jour toutes les scènes qui l'utilisent.
- **Résolution au build** : psxpipe **inline** le sous-arbre du prefab
  dans le `.psc` de chaque scène qui l'instancie
  (`scene::resolve_prefabs` : entités aplaties, assets
  fusionnés/dédupliqués par id). **La console ne connaît pas les
  prefabs** — le runtime voit des entités UI ordinaires, le format
  v1.3 ne change pas. Les **prefabs imbriqués sont rejetés** en v1
  (erreur claire au build).
- **Prefab Mode** (édition isolée) : double-clic sur le prefab dans le
  panneau Project (ou « Ouvrir le prefab » depuis l'inspecteur d'une
  instance) → l'éditeur ouvre le prefab **seul** : la hiérarchie ne
  montre que son sous-arbre, le viewport/mode Canvas l'édite comme une
  scène, un **fil d'Ariane** (`‹ scène 🧩 prefab (Prefab Mode)`)
  ramène à la scène. Sauvegarder écrit le fichier prefab ; toutes les
  instances suivent au rebuild.
- **Overrides d'instance (v1 minimal)** : l'instance surcharge le
  transform, le nom, le parent et l'état `actif` de sa racine, et son
  ordre dans la hiérarchie — pas de surcharge par enfant en v1 (noté
  en question ouverte, comme le « revert/apply » de Unity).

Le mécanisme (fichier d'entités + inline au build + mode isolé) est
volontairement **générique** : les prefabs 3D (une maison + ses
torches) l'utiliseront tel quel plus tard.

## 8. Éditeur (les panneaux existants, pas un éditeur à part)

- **Hiérarchie** : les entités UI y sont, sous leur Canvas — icônes
  dédiées (▦ canvas, 🖼 image, 🅰 text, 🔘 button, ☰ layout). Le menu
  « ＋ » gagne un groupe **UI** : Canvas, Image, Text, Button, Liste
  verticale, Liste horizontale (préréglages prêts à l'emploi).
- **Inspecteur** : sous un Canvas, la carte Transform est **remplacée
  par la carte RectTransform** (grille de presets d'ancres 3×3 + champs
  min/max/pivot éditables, comme Unity) ; cartes Image (texture +
  sprite choisi visuellement dans l'atlas, sélecteur **Image Type**
  Simple/Sliced/Tiled/Filled avec ses champs — poignées de `border`
  sur l'aperçu du sprite en Sliced, fill + amount en Filled), Text
  (police, texte, alignement), Button, Layout Group (axe, padding,
  spacing, alignement) — retirables au ✕, ajoutables par « ＋ Ajouter
  un composant ».
- **Viewport** : les canvas actifs sont rendus en surimpression au
  pixel (320×240) par-dessus la 3D. Sélectionner une entité UI passe
  la manipulation en 2D : déplacement à la souris (snap 1px, Ctrl =
  8px), poignées de redimensionnement, flèches = 1px. Les enfants d'un
  layout ne se déplacent pas à la main — on les **réordonne** dans la
  hiérarchie. Ctrl+Z/Y, copier/coller, dupliquer : la mécanique undo
  existante, rien de neuf.
- **Panneau Project** : les polices apparaissent (section Assets,
  vignette de l'atlas) et les prefabs ont leur section (vignette =
  aperçu du canvas rendu) — double-clic = **Prefab Mode**, glisser un
  prefab dans la hiérarchie = l'instancier. « Créer ▸ Prefab UI »
  rejoint le sous-menu Créer.

## 9. Pipeline

- `project.json` gagne `"fonts": [{ "png": "assets/police.png", "out": "main.fnt" }]` ;
- `psxpipe font2fnt police.png -o main.fnt --cell 8x12` ;
- le build de scène **inline d'abord les prefabs référencés** (assets
  fusionnés et dédupliqués), puis embarque la table UI + chaînes +
  polices dans le `.psc` (cache incrémental : un prefab modifié
  invalide les scènes qui l'instancient) ; les TIM d'UI passent par le
  packer VRAM (l'auto-4bpp fait le bon choix pour des atlas d'icônes) ;
- `gen_project` ajoute une police de démo et, au village : un HUD
  (vie en Image *Filled* + score) **en prefab instancié dans les deux
  scènes** — la preuve du mécanisme — et un menu pause en layout
  vertical sur fond *Sliced*.

## 10. Jalons de livraison

1. **Fondations** : SceneFormat v1.3 (table UI + `.fnt`, writer Rust,
   parsers TS/C, tests croisés — les 4 types d'image dans le format
   dès le départ), `font2fnt`, runtime `Ui_Draw` avec RectTransform
   complet (ancres/pivot/étirement) + canvas/image (*Simple* et
   *Filled*)/text, HUD statique dans la démo. *Vérifiable au previewer
   + émulateur.*
2. **Layout Groups, Sliced/Tiled + édition visuelle** : composant
   `layout` résolu dans le runtime et l'éditeur, rendu *Sliced*
   (9 primitives) et *Tiled* (grille), rendu des canvas dans le
   viewport, cartes RectTransform/Image (sélecteur d'Image Type +
   poignées de border)/Text dans l'inspecteur, manipulation 2D à la
   souris, groupe UI dans le menu « ＋ ».
3. **Prefabs UI + Prefab Mode** : fichier `prefabs/*.json`, inline au
   build (assets dédupliqués, cache invalidé en cascade),
   entité-référence + instance bleue en hiérarchie, édition isolée
   avec fil d'Ariane, « Créer ▸ Prefab UI », le HUD de démo partagé
   entre les deux scènes.
4. ~~**Interactif**~~ — **livré** : composant `button` (bit 3) dans
   toute la pile, focus D-pad au runtime (`Ui_FocusInit/Move/Focused`,
   navigation géométrique entre boutons visibles, surlignage
   semi-transparent du focalisé), `Ui_SetText` (chaîne remplacée par
   widget), API scripts sans `Scene*` (`Ui_Components`, `Ui_ImageType`,
   setters sur la scène courante) ; démo branchée au gameplay : jauge
   de vie pilotée par le script `hud` (marcher fatigue, souffler
   régénère, parler requinque), **menu pause** à boutons
   (START/D-pad/X, scripts `pause`), **dialogue migré en canvas**
   Sliced (script `dialogue`, fallback `Dialog_Draw` effacé).
5. **Confort** (au besoin) : Grid Layout, Content Size Fitter, fill
   radial, live tweaking des RectTransforms via la balise RAM,
   overrides d'instance par enfant (apply/revert à la Unity).

## 11. Questions ouvertes (à trancher en implémentant)

- Chaînes accentuées : UTF-8 translittéré vers le charset de la police
  à la conversion (probable), ou charset 8 bits fixe ?
- HUD persistant à travers le streaming : les canvas vivent dans la
  scène (donc rechargés à la bascule) — un « canvas global » survivant
  au swap d'arène est-il nécessaire, ou le rechargement suffit-il
  (états re-poussés par les scripts au `Start`) ?
- Rotation d'images (POLY_FT4) : utile pour aiguilles/compas, mais
  casse la simplicité SPRT — probablement flag v2.
- `expand` des layouts : faut-il aussi « control child size » complet
  (le layout impose la taille sur l'axe principal) comme Unity, ou
  seulement l'axe croisé en v1 ?
- Overrides d'instance de prefab : v1 se limite à actif/ordre de la
  racine — jusqu'où aller ensuite (par enfant, apply/revert, variants
  de prefab) sans réinventer toute la complexité Unity ?

## 12. Règles (rappel projet)

Comme tout changement de format : spec + writer Rust + parser TS +
runtime C + tests bougent ensemble ; chaque jalon se conclut par une
preuve visuelle (previewer/Playwright/émulateur) et la mise à jour de
GETTING-STARTED.
