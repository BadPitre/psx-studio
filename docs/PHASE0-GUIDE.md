# Phase 0 — Guide d'installation & premier build (Windows)

Objectif de sortie : le triangle 3D en rotation (`hello.bin/.cue`) tourne sur PCSX-Redux **et** DuckStation, buildé en une commande.

> Leçons PVSnesLib appliquées d'office : chemins **sans espaces ni accents**, releases précompilées plutôt que compilation du SDK, variables d'environnement vérifiées dans un **nouveau** terminal.

---

## 1. Prérequis

| Outil | Où | Notes |
|---|---|---|
| CMake ≥ 3.21 | cmake.org | cocher "Add to PATH" à l'installation |
| Ninja | github.com/ninja-build/ninja/releases | dézipper `ninja.exe` dans un dossier du PATH (ex. `C:\tools\ninja\`) |
| PCSX-Redux | github.com/grumpycoders/pcsx-redux (releases/nightly) | émulateur + débogueur, indispensable |
| DuckStation | github.com/stenzek/duckstation | validation visuelle |

Vérification dans un terminal PowerShell **fraîchement ouvert** :

```powershell
cmake --version   # >= 3.21
ninja --version
```

## 2. Installer PSn00bSDK (release précompilée)

1. Télécharger la dernière release **Windows** : `github.com/Lameguy64/PSn00bSDK/releases` (zip contenant libs + toolchain GCC MIPS + outils + exemples).
2. Extraire vers **`C:\psn00bsdk`** (pas dans `Program Files`, pas d'espaces).
3. Variables d'environnement (Paramètres → "variables d'environnement") :
   - Ajouter `C:\psn00bsdk\bin` au **PATH**.
   - Créer **`PSN00BSDK_LIBS`** = `C:\psn00bsdk\lib\libpsn00b` — c'est cette variable que lit le `CMakePresets.json` du projet.
4. Vérifier dans un **nouveau** terminal :

```powershell
mipsel-none-elf-gcc --version
mkpsxiso -h
echo $env:PSN00BSDK_LIBS
```

Si une commande n'est pas trouvée : le PATH n'est pas pris en compte → rouvrir le terminal, sinon revérifier l'étape 3.

## 3. Sanity check : compiler un exemple officiel

```powershell
cd C:\psn00bsdk\share\psn00bsdk\template   # (chemin selon la release)
cmake --preset default
cmake --build .\build
```

Attendu : `template.exe`, `template.bin`, `template.cue` dans `build\`. Ouvrir le `.cue` dans DuckStation → carré jaune qui rebondit sur fond violet. Si ça passe, l'installation est bonne.

## 4. Builder le hello-triangle PSX Studio

```powershell
cd <repo>\runtime\hello-triangle
cmake --preset default
cmake --build .\build
```

Sortie : `build\hello.bin` + `build\hello.cue`.

- **DuckStation** : ouvrir `hello.cue` (BIOS PS1 requis dans les réglages — à dumper de ta propre console).
- **PCSX-Redux** : File → Open Disk Image, ou glisser le `.cue`. Profites-en pour ouvrir *Debug → Show VRAM* : tu verras les deux framebuffers empilés et la police de debug en (960,0) — première intuition de la gestion VRAM.

Attendu : triangle Gouraud rouge/vert/bleu en rotation sur fond bleu nuit, texte "PSX STUDIO - PHASE 0".

## 5. Ce que le code montre déjà (à lire dans `main.c`)

- **Double buffering** : deux couples DISPENV/DRAWENV empilés verticalement en VRAM, swap à chaque VSync.
- **Ordering Table** : `ClearOTagR` + `addPrim` à l'index `otz` — le mécanisme de tri par profondeur qu'on utilisera pour TOUT (y compris les tuiles de décors précalculés en Phase 5 bis).
- **GTE** : `RotMatrix`/`TransMatrix` construisent la matrice monde en virgule fixe (4096 = un tour complet), `gte_rtpt` transforme et projette 3 sommets d'un coup, `gte_avsz3` donne le Z moyen pour l'OT.
- **Budget primitives** : `BUFFER_LENGTH` + l'`assert` de dépassement — l'ancêtre du compteur de l'éditeur.

## 6. Critères de sortie de la Phase 0

- [ ] `cmake --preset default && cmake --build ./build` fonctionne sans erreur dans `runtime/hello-triangle`
- [ ] Le `.cue` boote sur PCSX-Redux **et** DuckStation
- [ ] Le triangle tourne fluide (30/60 fps), texte visible
- [ ] Repo Git initialisé avec l'arborescence `runtime/ pipeline/ editor/ docs/`
- [ ] (Bonus) VRAM inspectée dans PCSX-Redux, compréhension du layout

## 7. Dépannage

| Symptôme | Cause probable | Fix |
|---|---|---|
| `psn00bsdk_add_executable` inconnu | `PSN00BSDK_LIBS` absent/faux | revérifier la variable, nouveau terminal |
| `No CMAKE_C_COMPILER` / gcc introuvable | `bin` pas dans le PATH | ajouter `C:\psn00bsdk\bin` au PATH |
| Generator Ninja introuvable | ninja.exe pas dans le PATH | étape 1 |
| Écran noir dans l'ému | `.exe` lancé au lieu du `.cue`, ou BIOS manquant (DuckStation) | ouvrir le `.cue`, configurer le BIOS |
| Erreurs de chemins bizarres | espaces/accents dans le chemin | déplacer le repo (ex. `C:\dev\psx-studio`) |

## 8. Prochaine étape → Phase 1

POC rendu & pipeline minimal : outil Rust `gltf2pmd` (un cube Blender vers notre format), chargement du mesh dans le runtime, caméra orbitale au pad, éclairage GTE. La décision PSn00bSDK vs PSYQo sera figée à la fin de cette phase.
