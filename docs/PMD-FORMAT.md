# Format PMD v1 — spécification

> PSX Studio Mesh Data. Format binaire de mesh produit par `psxpipe gltf2pmd`
> et consommé par le runtime (`runtime/poc-renderer/pmd.c`).
> Inspiré du TMD Sony, mais avec des **paquets GPU pré-encodés** : à
> l'exécution le moteur copie et complète, il ne construit jamais de primitive.

Tout est **little-endian**. Toutes les sections sont alignées sur 4 octets.
Le fichier est conçu pour être chargé tel quel en RAM : le runtime pose des
pointeurs dessus (aucune désérialisation), et patche uniquement `tpage`/`clut`
dans les templates texturés au chargement.

## En-tête (48 octets)

| Offset | Taille | Champ | Description |
|---|---|---|---|
| 0x00 | 4 | `magic` | `"PMD1"` |
| 0x04 | u16 | `version` | 1 |
| 0x06 | u16 | `flags` | bit 0 : le mesh référence une texture |
| 0x08 | i32 | `scale` | 4.12 : unités source par unité PMD (métadonnée éditeur, ignorée par le runtime) |
| 0x0C | u16 | `vertex_count` | nombre de sommets |
| 0x0E | u16 | `normal_count` | nombre de normales |
| 0x10 | u16×4 | `prim_counts` | nombre de primitives par type : F3, G3, FT3, GT3 |
| 0x18 | u32 | `verts_offset` | offset de la section sommets (depuis le début du fichier) |
| 0x1C | u32 | `normals_offset` | offset de la section normales |
| 0x20 | u32×4 | `prim_offsets` | offset de chaque section de primitives (F3, G3, FT3, GT3) |

## Sections de données

### Sommets (`verts_offset`)
`vertex_count` × 8 octets, layout `SVECTOR` : `i16 x, y, z, pad(=0)`.
Coordonnées en espace modèle, quantifiées en 16 bits signés
(par défaut la plus grande |coordonnée| vaut 128, option `--size`).

Convention d'axes : **+Y vers le bas, +Z vers l'écran** (espace GTE).
`gltf2pmd` applique (x, y, z) → (x, −y, z) depuis le glTF ; le miroir
transforme aussi le winding CCW du glTF en horaire, ce qu'attend le test
`nclip` du GTE.

### Normales (`normals_offset`)
`normal_count` × 8 octets, même layout, normales unitaires en **4.12**
(4096 = 1.0). Dédupliquées, indexées par les primitives.

### Primitives (`prim_offsets[k]`)
Chaque enregistrement = indices + template de paquet GPU (tag de 4 octets
inclus, champ longueur pré-rempli). Tailles fixes par type :

| Type | Indices | Paquet | Total | Contenu du template |
|---|---|---|---|---|
| F3 | `u16 vidx[3], nidx` (8 o) | `POLY_F3` (20 o) | 28 o | couleur matériau, code 0x20 |
| G3 | `u16 vidx[3], nidx[3]` (12 o) | `POLY_G3` (28 o) | 40 o | couleur ×3, code 0x30 |
| FT3 | `u16 vidx[3], nidx` (8 o) | `POLY_FT3` (32 o) | 40 o | couleur (128 = neutre), UV, code 0x24 |
| GT3 | `u16 vidx[3], nidx[3]` (12 o) | `POLY_GT3` (40 o) | 52 o | couleur ×3, UV ×3, code 0x34 |

Champs remplis par qui :

| Champ du paquet | Pipeline | Runtime |
|---|---|---|
| tag (longueur) | ✔ | link (via `addPrim`) |
| code, UV | ✔ | — |
| `tpage`, `clut` | 0 | patchés une fois au `Pmd_Load` |
| `x0..x2, y0..y2` | 0 | à chaque frame (GTE `rtpt`) |
| couleurs | couleur de base | éclairées à chaque frame (GTE `nccs`/`ncct`) |

Les UV sont en texels 8 bits relatifs à la page de texture (0–255),
mappés depuis [0,1] × (`--tex-w`/`--tex-h` − 1).

## Limites v1
- Triangles uniquement (le glTF ne produit que des triangles).
- Une seule texture (page + CLUT) par mesh.
- 65 535 sommets / normales / primitives par type max.
- Pas de quads (POLY_x4) : prévu pour une v2 si le gain le justifie.
- Pas de subdivision anti-warping (Phase 6).

## Évolution
Toute rupture de layout incrémente `version` ; le runtime rejette les
versions inconnues. Les ajouts compatibles (nouveaux flags, sections en fin
de fichier adressées par de nouveaux offsets) ne changent pas la version.
