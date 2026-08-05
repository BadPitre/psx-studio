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
| 0x32 | 14 | réservé (0) |

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
| 0x00 | i16×3 + pad | position locale |
| 0x08 | i16×3 + pad | rotation locale (unités 4096 = 360°) |
| 0x10 | i16×3 + pad | échelle locale (4.12, 4096 = 1.0) |
| 0x18 | u16 | parent (indice, `0xFFFF` = racine) |
| 0x1A | u16 | modèle (indice, `0xFFFF` = aucun) |
| 0x1C | u16 | flags (réservé) |
| 0x1E | u16 | pad |

Invariant : **`parent < index`** pour toute entité non racine (le sérialiseur
trie topologiquement) → le runtime calcule les matrices monde en un seul
passage avant.

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

## 4. Limites v1 & évolution
- Pas de noms dans le binaire (debug uniquement côté JSON/éditeur).
- Composants futurs (Camera, Light multiples, Collider, Script, AudioSource)
  viendront comme nouvelles tables adressées par de nouveaux offsets
  d'en-tête — sans rupture tant que le layout existant est stable.
- Le placement VRAM reste celui des TIMs (packing assisté en Phase 3).
- Toute rupture incrémente `version` ; le runtime rejette l'inconnu.
