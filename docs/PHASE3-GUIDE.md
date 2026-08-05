# Phase 3 — Pipeline complet & VRAM

Objectif de sortie : build **one-command** d'un projet exemple avec son et
musique — `psxpipe build` transforme un dossier projet en `.bin/.cue`
bootable, avec cache incrémental et packing VRAM automatique.

Prérequis : Phases 1-2. `mkpsxiso` doit être dans le PATH (il est fourni
avec PSn00bSDK).

---

## 1. Ce qui a été construit

### Packer VRAM automatique
- `psxpipe scene` **place les textures tout seul** : origines alignées sur
  les pages (x multiple de 64, y ∈ {0, 256}), CLUTs dans une bande
  réservée en bas de VRAM, les framebuffers et la police de debug sont
  protégés. Les coordonnées sont réécrites dans les TIM embarqués.
- Plus besoin des `--org-x/--clut-x` manuels (ils restent disponibles via
  `--keep-vram` pour un placement contrôlé à la main).
- **Carte VRAM exportée** : `--vram-map carte.png` (ou automatique dans
  `Build/vram-sceneN.png`) — c'est le précurseur du VRAM Viewer de
  l'éditeur (Phase 4).
- Échec = erreur claire (texture > 256×256, VRAM pleine), jamais de
  chevauchement silencieux.

### Audio
- **`psxpipe wav2vag`** : encodeur SPU-ADPCM maison (blocs de 28 samples,
  5 filtres de prédiction, feedback d'état décodé) + en-tête VAG. Testé
  par round-trip encodeur→décodeur (< 5 % d'erreur RMS sur un sinus).
- **SFX dans le player** : `BLIP.VAG` est lu depuis le CD au boot, uploadé
  en SPU RAM, joué sur **Carré**.
- **Musique** : piste **CD-DA** (track 2) déclenchée sur **Select**.
  Choix v1 assumé : la doc projet prévoyait du CD-XA ; le CD-DA donne le
  même résultat (musique streamée sans RAM) pour un dixième de la
  complexité. Le XA (qui permet données+audio entrelacés) viendra quand le
  streaming l'exigera. Limite héritée du CD : un chargement de scène coupe
  la musique, le player la relance après.

### `psxpipe build` — le build de projet
Structure d'un projet (voir `examples/demo/`) :

```
MonJeu/
├── project.json        # sources, scènes, audio, chemin du PS-EXE
├── assets/             # .gltf, .png (sources)
├── audio/              # .wav (sources)
├── scenes/             # .json (scènes éditables)
├── Library/            # cache des conversions (généré, ignoré en VCS)
└── Build/              # SCENEn.PSC, iso.xml, .bin/.cue (généré)
```

Le build : conversion **incrémentale** (hash de contenu dans
`Library/manifest.json` — seuls les sources modifiés sont reconvertis),
packing des scènes avec VRAM auto + cartes PNG, génération `SYSTEM.CNF` +
`iso.xml` (données + pistes audio), puis `mkpsxiso` → `demo.bin/demo.cue`.

## 2. Builder le projet exemple

```powershell
# 1. le runtime (une fois) :
cd <repo>\runtime\player
cmake --preset default && cmake --build .\build

# 2. les sources du projet démo (une fois) :
cd <repo>\pipeline
cargo run --example gen_project

# 3. le build one-command :
cargo run -- build ..\examples\demo
```

Ouvrir `examples\demo\Build\demo.cue`. Mêmes contrôles que la Phase 2,
plus : **Carré** = SFX, **Select** = musique on/off.

Relance `cargo run -- build ..\examples\demo` après une modification :
seuls les assets touchés sont reconvertis (`X converted, Y cached`).

## 3. Le fichier project.json

```json
{
  "name": "demo",
  "exe": "../../runtime/player/build/player.exe",
  "models":   [ { "gltf": "assets/house.gltf", "out": "house.pmd", "size": 128 } ],
  "textures": [ { "png": "assets/house.png", "out": "house.tim", "bpp": 8 } ],
  "scenes":   [ "scenes/scene0.json" ],
  "sfx":      [ { "wav": "audio/sfx.wav", "out": "BLIP.VAG" } ],
  "music":    [ "audio/music.wav" ]
}
```

- Les scènes référencent les sorties de `Library/` (`"pmd": "house.pmd"`).
- `sfx[].out` : nom 8.3 majuscules (contrainte ISO 9660).
- `music` : WAV stéréo 44,1 kHz → pistes CD-DA dans l'ordre (track 2, 3…).
- Le player charge `BLIP.VAG` par nom ; les scènes sont nommées
  `SCENE0.PSC`, `SCENE1.PSC`… dans l'ordre de la liste.

## 4. Dépannage

| Symptôme | Cause probable | Fix |
|---|---|---|
| `executable ... not found` | runtime pas compilé | `cmake --build` dans `runtime/player` |
| `mkpsxiso not found` | PATH | le build s'arrête après `iso.xml` ; lancer la commande affichée |
| `VRAM full` | trop de textures pour le packer v1 (placement par page) | réduire les tailles/bpp ; l'atlas assisté arrive avec l'éditeur |
| Pas de musique sur Select | ISO sans piste 2 (build `runtime/player` seul) | utiliser l'ISO de `psxpipe build` (le projet démo a la piste) |
| Musique coupée au changement de scène | lecture CD de données = arrêt CD-DA (matériel) | comportement attendu, le player relance ; le XA/streaming refera ça proprement |
| SFX muet | `BLIP.VAG` absent de l'ISO | régénérer via `gen_scenes` ou vérifier `sfx` dans project.json |

## 5. Critères de sortie de la Phase 3

- [ ] `cargo test` vert (31 tests : packer, ADPCM round-trip, build projet + cache)
- [ ] `psxpipe build examples/demo` produit un `.cue` bootable en une commande
- [ ] SFX audible (Carré), musique audible (Select), scènes navigables
- [ ] Deuxième build : tout en `cached`, un source modifié → seule sa
      conversion relancée
- [ ] Cartes VRAM générées dans `Build/` et lisibles

## 6. Prochaine étape → Phase 4

L'éditeur MVP (Tauri + React + Three.js) : tout ce que le pipeline sait
faire en CLI passe derrière une UI — hiérarchie, inspecteur, viewport
shaders PS1, VRAM Viewer (la carte PNG devient interactive), et le bouton
**Play** branché sur l'API de PCSX-Redux. À prototyper en premier : la
connexion PCSX-Redux (risque technique n°1 du projet).
