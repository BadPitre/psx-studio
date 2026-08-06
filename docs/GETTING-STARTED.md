# Bien démarrer avec PSX Studio

> De zéro à ton propre jeu PlayStation 1 qui tourne dans l'émulateur,
> édité dans un éditeur façon Unity. Ce guide est autonome : il ne suppose
> que Windows (les commandes marchent aussi sous Linux/macOS en adaptant
> les chemins).

## 1. Prérequis

| Outil | Rôle | Installation |
|---|---|---|
| [PSn00bSDK](https://github.com/Lameguy64/PSn00bSDK) + toolchain `mipsel-none-elf` | compiler le code console | releases précompilées, ajouter `bin/` au PATH |
| CMake ≥ 3.21 | build du runtime | cmake.org ou `winget install cmake` |
| [mkpsxiso](https://github.com/Lameguy64/mkpsxiso) | packer l'ISO .bin/.cue | release GitHub, ajouter au PATH |
| Rust (stable) | pipeline d'assets + backend éditeur | rustup.rs |
| Node.js ≥ 20 | frontend de l'éditeur | nodejs.org |
| `cargo install tauri-cli` | lancer l'éditeur desktop | après Rust |
| [PCSX-Redux](https://github.com/grumpycoders/pcsx-redux) | émulateur du Play Mode (API web) | build nightly, retenir le chemin de l'exe |

DuckStation marche aussi pour jouer l'ISO, mais le Play Mode intégré
(statut, pause, live tweaking) pilote PCSX-Redux.

## 2. Construire le studio

```bat
git clone https://github.com/BadPitre/psx-studio
cd psx-studio

:: 1. Le moteur + la démo jouable (exécutable console)
cd runtime\game
cmake --preset default && cmake --build build

:: 2. Les sources du projet démo (assets régénérables, hors git)
cd ..\..\pipeline\psxpipe
cargo run --release --example gen_project -- ..\..\examples\demo

:: 3. L'éditeur desktop
cd ..\..\editor
npm install
cargo tauri dev
```

Dans l'éditeur : **Ouvrir un projet…** → `examples/demo` → renseigne le
chemin de `pcsx-redux.exe` dans la barre Play → **▶ Play**. Le build
incrémental convertit les assets, packe l'ISO et lance l'émulateur.
Croix directionnelle pour marcher, ✕ près du PNJ pour parler.

## 3. Créer ton propre projet

Un projet est un dossier avec un `project.json` :

```json
{
  "name": "monjeu",
  "exe": "../../runtime/game/build/game.exe",
  "models":   [ { "gltf": "assets/heros.gltf", "out": "heros.pmd", "tex_w": 128, "tex_h": 128 } ],
  "textures": [ { "png": "assets/heros.png", "out": "heros.tim" } ],
  "scenes":   [ "scenes/scene0.json" ],
  "sfx":      [ { "wav": "audio/blip.wav", "out": "BLIP.VAG" } ],
  "music":    [ "audio/theme.wav" ]
}
```

- `exe` : l'exécutable console (commence par copier `runtime/game/`,
  c'est ta base de code jeu).
- `textures` : sans `"bpp"`, le pipeline choisit tout seul — **4bpp**
  (16 couleurs, moitié de VRAM) si l'image y tient, **8bpp** (256
  couleurs) sinon. Force `"bpp": 4/8/16` seulement si tu veux imposer
  un mode.
- `scenes` : la première scène listée est `SCENE0.PSC`, chargée au boot.
- `sfx` : WAV → SPU-ADPCM (joués par le moteur) ; `music` : pistes CD-DA.
- Le plus simple reste de **copier `examples/demo`** et de le modifier.

Ensuite tout se passe dans l'éditeur : scènes, entités, import d'assets.

## 4. Le workflow Blender

1. Modélise **low-poly** (l'écran fait 320×240 ; un modèle de héros de
   50 à 300 triangles est dans l'esprit console).
2. Texture **une seule image** par modèle (max **256×256**, la taille
   d'une page VRAM), assignée en *Base Color* du matériau. UV dans
   [0, 1] — le tiling n'existe pas sur une page. **Évite les lignes
   fines très contrastées** (grilles, joints noirs) sur les sols et
   grandes surfaces : la PS1 n'a pas de mipmaps, et au loin
   l'échantillonnage « nearest » les transforme en rayures d'aliasing
   à l'écran. Des transitions douces vieillissent beaucoup mieux.
3. Exporte en **glTF** (.glb ou .gltf+.bin, embarque la texture).
4. **Glisse le fichier dans la fenêtre de l'éditeur** : la texture est
   extraite et quantifiée (16 couleurs → 4bpp automatique, sinon 256
   couleurs en 8bpp), le modèle converti, les deux enregistrés dans le
   projet. Choisis le modèle dans l'inspecteur d'une entité.

### Anti-warping (textures qui « nagent »)

La PS1 mappe les textures **par triangle, sans correction de
perspective** : les grands triangles vus de biais déforment leur texture.
Le remède d'époque : subdiviser. Dans `project.json` :

```json
{ "gltf": "assets/sol.gltf", "out": "sol.pmd", "subdiv": 48 }
```

`subdiv` = longueur d'arête maximale en unités PMD (le modèle est
normalisé à ±128 par défaut). Le pipeline coupe récursivement les grands
triangles en interpolant UV et normales. En CLI :
`psxpipe gltf2pmd sol.gltf --subdiv 48`. Règle de pouce : 32–64 pour un
sol ou de grands murs, inutile sur un modèle déjà dense. (Le coût : plus
de triangles — surveille l'avertissement de budget.)

## 5. L'éditeur en bref

- **Hiérarchie** : ＋ ajoute, clic droit = menu, Ctrl+C/V/D copie/colle/
  duplique, Suppr supprime, F2 renomme, Ctrl+Z/Ctrl+Y annule/rétablit.
- **Viewport** : clic gauche sélectionne/orbite, clic droit tenu = caméra
  FPS (ZQSD, Shift rapide), molette avance. Gizmos **1/2/3**
  (déplacer/rotation/échelle), Ctrl = snap. Rendu 320×240 authentique
  (snapping de sommets, affine, Gouraud GTE).
- **Inspecteur** : philosophie Unity — une entité porte des
  **composants** en cartes retirables (✕) : MeshRenderer (modèle +
  subdivision), Lumière (couleur, intensité), Caméra (FOV, distance
  d'affichage), Script (nom résolu par hash — voir §6). Le bouton
  « ＋ Ajouter un composant » liste ceux disponibles.
- **VRAM** : bouton VRAM = carte des pages texture/CLUT réellement
  packées, avec % d'occupation et pages libres.
- **Panneau Project** (bande du bas, comme Unity) : tout le contenu du
  projet — scènes, modèles, textures, audio — dans une arborescence de
  dossiers **libre** : clic droit → **Créer ▸ Dossier** puis glisse tes
  tuiles dedans pour ranger (le fichier bouge sur disque et
  `project.json` suit tout seul). Double-clic pour ouvrir une scène ou
  importer un fichier marqué « non importé » ; **Créer ▸ Scène** pour
  démarrer une scène vide. Le badge « manquant » signale une entrée de
  `project.json` dont le fichier a disparu. Les modèles et textures
  importés montrent un **aperçu rendu** (depuis les fichiers convertis,
  donc fidèle à la console), et **glisser un modèle dans le viewport
  l'instancie** comme entité à l'endroit visé (dans la hiérarchie : à
  l'origine). L'onglet **Console** à côté garde le journal des imports,
  builds et erreurs.
- **Prefabs** (comme Unity) : glisse une entité de la hiérarchie vers
  le panneau Project → elle devient un `prefabs/<nom>.json`
  réutilisable. Glisse le prefab dans la hiérarchie d'une scène → une
  **instance liée** (en bleu 🧩) : la scène ne stocke qu'une référence,
  le contenu est incorporé au build. Double-clic sur le prefab (ou
  « Ouvrir le prefab » dans l'inspecteur) → **Prefab Mode** : édition
  isolée avec fil d'Ariane « ‹ scène » ; sauvegarde = toutes les scènes
  qui l'utilisent suivent.
- **▶ Play** : build incrémental + PCSX-Redux. Pendant que le jeu tourne,
  déplacer une entité (gizmo ou inspecteur) **l'écrit dans la RAM
  console** : le décor bouge dans le jeu, sans rebuild.
- **UI interactive** (démo village) : la jauge de vie du HUD est
  branchée au gameplay (marcher fatigue, souffler régénère, parler au
  PNJ requinque), **START** ouvre un menu pause à boutons — D-pad pour
  déplacer le focus (surligné), X pour activer, REPRENDRE/QUITTER — et
  le dialogue est un canvas UI de la scène (éditable comme le reste).
  Côté code : composant Button (menu ＋ → Bouton UI), scripts
  `hud`/`pause`/`dialogue` dans `runtime/game/scripts/` comme modèles,
  API `Ui_*` dans `engine.h`.

## 6. Écrire du gameplay

Le gameplay est du **C compilé dans ton exécutable** (pas d'interpréteur :
c'est la philosophie du studio, les données restent des données). Dans ta
copie de `runtime/game/` :

1. Écris un script dans `scripts/` :

```c
#include "engine.h"

void Porte_Start(Entity* self)  { /* init */ }
void Porte_Update(Entity* self) {
    if (Input_Pressed() & PAD_CROSS) self->visible = 0;
}
```

2. Déclare-le dans `scripts/registry.c` :

```c
const ScriptDef g_scripts[] = {
    { "player", Player_Start, Player_Update },
    { "porte",  Porte_Start,  Porte_Update  },
};
```

3. Dans l'éditeur, tape `porte` dans le champ Script de l'entité.

L'API moteur (`engine.h`) : `Input_Held/Pressed`, `Camera_Set`,
`Dialog_Show/IsOpen/Close`, `Physics_MoveAndSlide` (collisions AABB),
`Scene_FindByScript`, `Sfx_Play`, `Entity_Dist2XZ`. Rebuild :
`cmake --build build` puis ▶ Play.

## 7. Aller plus loin

- Specs des formats : `PMD-FORMAT.md`, `SCENE-FORMAT.md`.
- Journal de construction du studio : guides `PHASE0` à `PHASE6`.
- Architecture et philosophie : `psx-studio-doc-projet.md`.
- Contribuer : `CONTRIBUTING.md` à la racine.
