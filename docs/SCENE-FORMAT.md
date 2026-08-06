# SceneFormat v1 — spécification

> Format de scène de PSX Studio. Deux représentations :
> - **JSON** (`.scene.json`) : éditable, lisible, diffable — la source de vérité côté projet.
> - **Binaire** (`.psc`, PlayStation SCene) : packé par `psxpipe scene`, consommé
>   par le runtime player. **Une scène = un seul fichier contigu sur le CD**
>   (une seule lecture CD + fixup, conformément au design §4.3 de la doc projet).
>
> Le pipeline Rust est la seule source de vérité de la conversion JSON→binaire ;
> l'éditeur ne fabrique jamais de binaire lui-même.

## 1. JSON éditable

```json
{
  "name": "village",
  "settings": {
    "background":  [16, 16, 48],
    "ambient":     [64, 64, 64],
    "light_dir":   [-1.0, -1.0, -1.0],
    "light_color": [255, 255, 255]
  },
  "assets": {
    "textures": [
      { "id": "house_tex", "tim": "house.tim" }
    ],
    "models": [
      { "id": "house", "pmd": "house.pmd", "texture": "house_tex" }
    ]
  },
  "entities": [
    {
      "name": "maison1",
      "position": [0, 0, 0],
      "rotation": [0, 45, 0],
      "scale":    [1, 1, 1],
      "model":    "house"
    },
    {
      "name": "cheminee",
      "parent": "maison1",
      "position": [40, -90, 0],
      "model": "cube"
    },
    {
      "name": "perso",
      "position": [0, 0, -60],
      "model": "guy",
      "script": "player"
    }
  ]
}
```

Conventions :
- `position` : unités monde GTE (mêmes unités que les sommets PMD), axes PS1
  (+Y vers le bas). Doit tenir en i16.
- `rotation` : degrés (convertis en unités d'angle PS1, 4096 = 360°).
- `scale` : flottant, converti en 4.12. Défauts : pos/rot 0, scale 1.
- `parent` : nom d'une autre entité (absent/null = racine). Les cycles sont
  rejetés ; le sérialiseur trie parents avant enfants.
- `model` : absent = entité vide (nœud de hiérarchie pur).
- `light_dir` : direction de **propagation** de la lumière en espace monde
  (le pipeline la normalise et stocke le vecteur *vers* la source, prêt pour
  la matrice lumière GTE).
- Chemins `pmd`/`tim` relatifs au fichier JSON. Les assets sont **embarqués**
  dans le `.psc`.
- `script` (v1.1, optionnel) : nom d'un script runtime. Le binaire ne stocke
  que son **hash FNV-1a 32 bits** (nom passé en minuscules) ; le runtime le
  résout au chargement contre sa table `g_scripts` compilée dans le jeu.

## 2. Binaire `.psc` (little-endian, sections alignées sur 4)

### En-tête (64 octets)

| Offset | Taille | Champ |
|---|---|---|
| 0x00 | 4 | magic `"PSC1"` |
| 0x04 | u16 | version = 1 |
| 0x06 | u16 | flags (réservé) |
| 0x08 | u32 | taille totale du fichier |
| 0x0C | u16×4 | `model_count`, `texture_count`, `entity_count`, pad |
| 0x14 | u32×3 | offsets : table modèles, table textures, table entités |
| 0x20 | u8×4 | couleur de fond RGB + pad |
| 0x24 | u8×4 | ambiante RGB + pad |
| 0x28 | u8×4 | couleur lumière RGB + pad |
| 0x2C | i16×3 | vecteur **vers** la source lumière, 4.12, espace monde |
| 0x32 | u16 | `script_count` (v1.1 ; 0 = pas de table de scripts) |
| 0x34 | u32 | `scripts_offset` (v1.1 ; 0 si `script_count` = 0) |
| 0x38 | u32 | `lights_offset` (v1.2 ; 0 = pas de table de lumières) |
| 0x3C | u16 | `light_count` (v1.2 ; 2 max — le GTE offre 3 lignes, le soleil des settings occupe la ligne 0) |
| 0x3E | 2 | réservé (0) |

Les extensions v1.1/v1.2 vivent dans les octets réservés de la v1 : un
lecteur ancien ignore ces champs (ils valaient 0), un lecteur récent lit
une scène ancienne comme « sans scripts / sans lumières ». `version`
reste 1.

### Table des modèles (`model_count` × 12 octets)
| u32 `offset` | u32 `size` | u16 `texture` | u16 pad |

`offset` pointe sur un blob **PMD** complet (voir `PMD-FORMAT.md`).
`texture` = indice dans la table des textures, `0xFFFF` = non texturé.

### Table des textures (`texture_count` × 8 octets)
| u32 `offset` | u32 `size` |

`offset` pointe sur un blob **TIM** complet. Les coordonnées VRAM sont celles
du TIM ; le sérialiseur **vérifie les collisions** entre textures de la scène
et avec les framebuffers (erreur si chevauchement).

### Table des entités (`entity_count` × 32 octets)

| Offset | Type | Champ |
|---|---|---|
| 0x00 | i16×3 + `cam_fov` u16 | position locale ; le pad porte le FOV vertical caméra en degrés (v1.2, 0 = défaut ~74°) |
| 0x08 | i16×3 + `cam_draw` u16 | rotation locale (unités 4096 = 360°) ; le pad porte la distance d'affichage caméra (v1.2, 0 = illimitée) |
| 0x10 | i16×3 + pad | échelle locale (4.12, 4096 = 1.0) |
| 0x18 | u16 | parent (indice, `0xFFFF` = racine) |
| 0x1A | u16 | modèle (indice, `0xFFFF` = aucun) |
| 0x1C | u16 | flags (v1.2) : bit 0 = lumière, bit 1 = caméra |
| 0x1E | u16 | `script_ref` (v1.1) : 0 = aucun, sinon **indice + 1** dans la table des scripts |

Invariant : **`parent < index`** pour toute entité non racine (le sérialiseur
trie topologiquement) → le runtime calcule les matrices monde en un seul
passage avant.

### Table des scripts (v1.1, `script_count` × 4 octets)

| u32 `hash` |

Hash **FNV-1a 32 bits** du nom du script en minuscules (implémentation
identique dans `scene.rs::script_hash` et `scene.c::Script_Hash`). Les noms
apparaissent dans l'ordre de première utilisation par les entités. Le runtime
résout chaque hash contre `g_scripts[]` au chargement ; un hash inconnu vaut
« pas de script » (+ warning debug), la scène reste jouable.

### Table des lumières (v1.2, `light_count` × 6 octets)

| u16 `entity` | u8×3 `color` | u8 `intensity` (pourcent, 0 = 100) |

L'intensité multiplie la couleur dans la matrice GTE (4.12 : une lumière
peut dépasser 100 %, jusqu'à 250 %).

Deux types, distingués par les flags de l'entité : **directionnelle**
(défaut, lignes GTE natives, 2 max en plus du soleil) et **ponctuelle**
(bit 2, « torche », 4 max) — JSON : `"light": { "type": "point",
"color": [...], "radius": 520 }`. Le rayon vit dans le pad du vecteur
échelle de l'entité (u16, unités monde) et reste **mutable au runtime**
(le faire osciller = vacillement). Une ponctuelle est appliquée **par
objet** dans `Scene_Draw` : direction torche→objet + atténuation
linéaire par la distance (approchée en octogonal, sans racine carrée)
injectées dans les lignes GTE restantes — l'approximation des jeux
d'époque — plus un **halo additif** (losange semi-transparent, mode B+F
du GPU) projeté à sa position.

`entity` = indice d'entité (ordre du fichier) : sa **rotation donne la
direction** — convention unique du studio, une entité « regarde » et
« éclaire » le long de son axe **-Z local** (comme les modèles). Le
runtime re-dérive la direction chaque frame (vers-la-source = +Z monde,
3e colonne de la rotation monde) : tourner l'entité — script, gizmo ou
live tweaking — change l'éclairage en direct. Les couleurs remplissent
les colonnes 1-2 de la matrice couleur GTE.

Côté JSON : `"light": { "color": [r, g, b], "intensity": 1.5 }` sur
l'entité (intensité optionnelle, 0.1–2.5), et `"camera": true` ou
`"camera": { "fov": 74, "draw_distance": 1500 }` pour le composant caméra
(la première entité caméra donne la vue initiale de la scène ; les
scripts peuvent reprendre la main). Le FOV vertical est converti par le
runtime en distance de projection GTE (`gte_SetGeomScreen`,
h = 120/tan(fov/2) ; le défaut console h = 160 vaut ~74°).

### Blobs
Après les tables : blobs modèles puis textures, chacun aligné sur 4.

## 3. Chargement côté runtime

1. `CdSearchFile` + `CdRead` du fichier entier dans l'**arène de scène**
   (taille arrondie au secteur de 2048 octets).
2. Validation magic/version.
3. Textures : `GetTimInfo` + `LoadImage` (+ CLUT) pour chaque TIM.
4. Modèles : `Pmd_Load` avec le tpage/CLUT de leur texture (fixup in-place).
5. Entités : matrices monde en un passage (parents d'abord) :
   `W = W_parent ∘ (R(rot) · S(scale), pos)` en virgule fixe.
6. Rendu : par entité, composition caméra ∘ monde → GTE, `Pmd_Draw`.

Changement de scène = reset de l'arène + retour à l'étape 1. Aucune
allocation générique : tout vit dans l'arène, libérée d'un bloc.

Depuis la Phase 5, les entités sont copiées dans un tableau **mutable**
(`Entity[]`) : les scripts et l'éditeur peuvent modifier pos/rot/scale à
chaque frame, `Scene_UpdateWorld` recalcule les matrices monde.

### Balise éditeur (live tweaking)

Le runtime publie en RAM une structure de 28 octets commençant par le magic
`"PSXSTUDIOBCN"` (écrit en dernier), suivie de : `version` u16 (=1),
`entity_size` u16, `entities_addr` u32 (adresse KSEG de `entities[0]`),
`entity_count` u16, puis les offsets u16 de `pos`, `rot`, `scale` dans
`Entity`. L'éditeur la localise en scannant un dump RAM de PCSX-Redux
(`GET /api/v1/cpu/ram/raw`) puis écrit les transforms directement en RAM
console (`POST …?offset=&size=`) pendant que le jeu tourne — l'indice
d'entité est l'indice du fichier `.psc` (ordre topologique).

## 4. Limites v1 & évolution
- Pas de noms dans le binaire (debug uniquement côté JSON/éditeur).
- Composants futurs (Camera, Light multiples, Collider, AudioSource)
  viendront comme nouvelles tables adressées par de nouveaux offsets
  d'en-tête — sans rupture tant que le layout existant est stable
  (le composant Script est arrivé exactement comme ça en v1.1).
- Le placement VRAM reste celui des TIMs (packing assisté en Phase 3).
- Toute rupture incrémente `version` ; le runtime rejette l'inconnu.

## Extension v1.3 — table UI + polices (Lot E jalon 1)

Le 4e compteur de l'en-tête (0x12) devient `ui_count`, et 0x3E devient
`font_count` (nuls avant : rétrocompatible). Les tables se dérivent :
`ui = align4(lights + light_count*6)`, puis `font_count` entrées de
8 octets (offset u32 + taille u32 d'un blob .fnt embarqué), puis la
table de chaînes (C, terminées par 0). Un enregistrement UI fait
40 octets : entité u16, composants u8 (canvas/image/text/button/layout/
actif), flags u8 (type d'image, fill vertical, semi-trans, axe layout),
ancres min/max + pivot en 4.12 (6×u16), position/taille i16×4, teinte
RGB + asset u8, data u16 (offset chaîne ou amount 4.12), extra u16
(alignement texte, spacing+padding layout), sprite uv u8×4, border
u8×4. Les entités UI portent `ENTITY_FLAG_UI` (bit 3). Un .fnt :
"FNT1", cell_w/h u8, first u8, count u8, chasses u8×count, pad(4),
puis un TIM 4bpp (packé en VRAM avec les textures par le builder).
Spec de conception : docs/UI-SYSTEM.md.
