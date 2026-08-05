# Phase 1 — POC rendu & pipeline minimal

Objectif de sortie : un modèle texturé et éclairé (cube de démonstration,
puis n'importe quel export Blender) qui tourne à 30 fps sur émulateur, avec
caméra orbitale au pad. Décision SDK figée en fin de phase.

Prérequis : Phase 0 terminée (PSn00bSDK + toolchain installés, voir
`PHASE0-GUIDE.md`) + **Rust** (rustup.rs, toolchain stable).

---

## 1. Ce qui a été construit

### Pipeline Rust (`pipeline/psxpipe`)
CLI unique `psxpipe` avec trois sous-commandes :

| Commande | Rôle |
|---|---|
| `gltf2pmd` | glTF/GLB → mesh PMD (quantization 16 bits, normales 4.12, paquets GPU pré-encodés — voir `PMD-FORMAT.md`) |
| `png2tim` | PNG → TIM (quantization median-cut 16/256 couleurs ou 16 bpp direct, CLUT, placement VRAM) |
| `info` | affiche l'en-tête d'un `.pmd` / `.tim` |

Chaque conversion imprime un **rapport d'import** : nombre de triangles par
type, sommets/normales après déduplication, avertissements (UV hors [0,1],
mesh trop dense, chevauchement framebuffer…). C'est l'embryon des rapports
de l'éditeur (Phase 4).

### Runtime (`runtime/poc-renderer`)
- Chargeur PMD zéro-copie : pose des pointeurs sur le blob, patche
  `tpage`/`clut` une fois, dessine en copiant les templates de paquets.
- Caméra orbitale au pad, éclairage GTE 1 directionnelle blanche + ambiante
  (matrices lumière/couleur, `nccs`/`ncct`), backface culling `nclip`,
  tri par OT 1024 entrées.
- Compteur de triangles à l'écran — l'ancêtre du compteur de l'éditeur.
- Assets embarqués dans l'exécutable via `psn00bsdk_target_incbin`
  (le streaming CD arrive en Phase 2 avec le format de scène).

Contrôles : **D-pad** orbite, **Croix/Triangle** zoom, **Select** stoppe la
rotation auto, **Start** reset caméra.

## 2. Builder le pipeline

```powershell
cd <repo>\pipeline
cargo test          # 13 tests (formats, quantization, import cube de bout en bout)
cargo build --release
```

Le binaire est dans `pipeline\target\release\psxpipe.exe`.

## 3. Régénérer les assets d'exemple (optionnel — ils sont commités)

```powershell
cd <repo>\pipeline
cargo run --example gen_samples -- samples          # cube.gltf + checker.png
cargo run -- gltf2pmd samples\cube.gltf -o ..\runtime\poc-renderer\assets\cube.pmd
cargo run -- png2tim  samples\checker.png -o ..\runtime\poc-renderer\assets\checker.tim
cargo run -- info ..\runtime\poc-renderer\assets\cube.pmd
```

## 4. Builder et lancer le runtime

```powershell
cd <repo>\runtime\poc-renderer
cmake --preset default
cmake --build .\build
```

Ouvrir `build\poc.cue` dans PCSX-Redux ou DuckStation.

Attendu : cube damier multicolore, éclairé (une face vers la lumière claire,
faces opposées en ambiante sombre), rotation auto, orbite au pad, texte
"PSX STUDIO - PHASE 1" + compteur `TRIS 12`.

Dans PCSX-Redux, *Debug → Show VRAM* : les deux framebuffers à gauche, la
texture damier 8 bpp en (320,0), sa CLUT en (320,256) — premier aperçu du
packing VRAM que l'éditeur devra gérer (Phase 3).

## 5. Tester avec un vrai modèle Blender

Un modèle de test "façon Blender" est fourni : `pipeline/samples/house.gltf`
+ `house.png` (maison low-poly, 16 tris, porte/fenêtre, toit débordant).
Déroule la moulinette dessus pour te faire la main :

```powershell
cd <repo>\pipeline
cargo run -- gltf2pmd samples\house.gltf -o ..\runtime\poc-renderer\assets\cube.pmd
cargo run -- png2tim  samples\house.png  -o ..\runtime\poc-renderer\assets\checker.tim
# préview PC sans émulateur (mêmes règles de rendu que le runtime) :
cargo run --example preview -- samples\house.pmd samples\house.tim house.png.preview.png --dist 500
```

Puis rebuilder `runtime/poc-renderer` : les assets sont embarqués au build.

Pour tes propres modèles :

1. Dans Blender : modèle low-poly (< 800 tris), **UV dans [0,1]** (pas de
   tiling), une seule texture, export **glTF Separate ou Binary**.
2. Convertir :

```powershell
cargo run -- gltf2pmd monmodele.gltf -o ..\runtime\poc-renderer\assets\cube.pmd --size 128
cargo run -- png2tim  matexture.png  -o ..\runtime\poc-renderer\assets\checker.tim --bpp 8
```

3. Rebuilder le runtime (les assets sont embarqués au build). Lire le
   rapport d'import : les avertissements y signalent tout ce qui ne
   passera pas bien sur console.

Notes :
- `--size` contrôle la taille du modèle en unités PMD (la caméra est réglée
  pour ~128 ; augmenter `CAM_DIST_*` dans `main.c` pour un modèle plus grand).
- Modèle sans texture : ajouter `--untextured` (utilise la couleur de base
  du matériau) ; `--flat` force une normale par face.

## 5 bis. Dépannage

| Symptôme | Cause probable | Fix |
|---|---|---|
| Texte affiché mais **aucun modèle** | ordre des appels GTE : `MulMatrix0` écrase les registres de la matrice de rotation (il tourne sur le GTE) | toujours appeler `MulMatrix0` **avant** `gte_SetRotMatrix` (cf. commentaire dans `main.c`) |
| Modèle noir/sombre | lumière mal orientée (les lignes de la matrice lumière pointent **vers** la source) ou ambiante à 0 | vérifier `light_mtx` et `gte_SetBackColor` |
| Texture absente (polys invisibles) | texels à 0x0000 = transparent (TIM pas uploadé, tpage/clut faux) | vérifier le VRAM Viewer de PCSX-Redux : texture en (320,0), CLUT en (320,256) |
| Modèle qui "explose" | coordonnées hors 16 bits ou caméra dans le mesh | augmenter `--size` côté pipeline ou `CAM_DIST_MIN` |
| Rendu bizarre vs attendu | — | comparer avec `cargo run --example preview` qui simule le runtime sur PC |

## 6. Critères de sortie de la Phase 1

- [ ] `cargo test` vert dans `pipeline/`
- [ ] `gltf2pmd` + `png2tim` convertissent le cube d'exemple avec rapport
- [ ] `poc.cue` boote sur PCSX-Redux **et** DuckStation
- [ ] Cube texturé, éclairé, orbite au pad fluide (60 fps ici — le budget
      30 fps se jugera sur un mesh Blender de 500+ tris)
- [ ] Un export glTF de Blender passe dans la même moulinette sans édition
      manuelle
- [ ] Décision SDK figée (voir ci-dessous)

## 7. Décision SDK — recommandation

**Rester sur PSn00bSDK.** Constats de cette phase :
- Tout ce dont la Phase 1 avait besoin existe et fonctionne : GTE complet
  (macros `inline_c.h`), primitives GPU, incbin CMake, mkpsxiso intégré.
- L'API calque le SDK officiel : la doc historique et les tutoriels
  s'appliquent, et le layout `POLY_*` a pu être pré-encodé dans le pipeline
  sans surprise.
- PSYQo (C++) reste élégant mais son écosystème 3D/exemples est plus mince,
  et le pré-encodage des paquets nous fait déjà éviter la majorité du
  boilerplate C.

À valider une dernière fois au moment de figer : la licence MPL des
portions adaptées (attribution conservée dans les fichiers concernés) est
compatible avec la publication MIT de l'outil.

## 8. Prochaine étape → Phase 2

Format de scène binaire `SceneFormat v1` + runtime « player » générique :
hiérarchie de transforms, plusieurs entités, arènes mémoire, chargement
depuis le CD (fin de l'incbin), changement de scène.
