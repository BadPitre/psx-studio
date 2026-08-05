# Phase 5 — Gameplay : scripts, entrées, physique, live tweaking

Objectif de sortie : une **démo jouable** construite avec le studio — un
personnage qui se déplace à la manette dans le village, entre en collision
avec les maisons, parle à un PNJ — et l'éditeur qui **modifie le jeu pendant
qu'il tourne** dans l'émulateur.

## 1. Composant Script (SceneFormat v1.1)

Une entité du `scene.json` peut porter un script par son nom :

```json
{ "name": "perso", "model": "guy", "script": "player" }
```

Le binaire `.psc` ne stocke que le **hash FNV-1a 32 bits** du nom (table de
scripts + référence par entité, logés dans les octets réservés de la v1 —
format 100 % rétrocompatible, voir `SCENE-FORMAT.md`). Au chargement, le
runtime résout chaque hash contre la table `g_scripts[]` **compilée dans le
jeu** : les données restent des données, le code reste du code — pas
d'interpréteur, zéro coût quand une entité n'a pas de script.

Côté éditeur, l'inspecteur a un champ **Script** : taper `player` ou `npc`
(ou le laisser vide), c'est écrit dans le JSON comme le reste.

## 2. Moteur : l'API de gameplay (`runtime/engine/`)

Le code runtime partagé a été extrait dans `runtime/engine/` (le player
l'utilise aussi). Nouveautés :

- **Entités mutables** : le `.psc` est copié dans un tableau `Entity[]`
  (pos/rot/scale/visible modifiables), `Scene_UpdateWorld()` recalcule les
  matrices monde à chaque frame (parents d'abord, comme au chargement).
- **`ScriptDef`** : `{ nom, on_start(entity), on_update(entity) }` —
  enregistrés dans `g_scripts[]` par le jeu (`scripts/registry.c`).
- **Entrées** : `Input_Held(bouton)` / `Input_Pressed(bouton)` (front
  montant), lecture manette standard PSn00bSDK.
- **Caméra** : `Camera_Set(x, y, z, yaw, pitch)` pilotée par script (la
  démo fait une caméra de suivi type RPG).
- **Physique minimale** : `Physics_MoveAndSlide(entity, dx, dz)` — AABB
  XZ contre les entités `solid` (boîtes déduites des bornes du modèle ×
  échelle), axes séparés pour glisser le long des murs, le sol plat est
  ignoré.
- **Dialogue** : `Dialog_Show(lignes…)` — boîte semi-transparente + texte
  debug, fermée par ✕, `Dialog_IsOpen()` pour geler le joueur pendant la
  lecture.
- **SFX** : `Sfx_UploadVag` / `Sfx_Play` factorisés depuis le player.

## 3. La démo jouable (`runtime/game/`)

Un **second exécutable** construit avec le moteur :

- `scripts/player.c` : déplacement 8 directions à la croix (vitesse fixe),
  collision `MoveAndSlide` contre les maisons, orientation du modèle selon
  la direction, caméra de suivi ; près du PNJ, ✕ ouvre un dialogue (et
  joue le blip SPU).
- `scripts/npc.c` : petit flottement sinusoïdal, se tourne vers le joueur
  quand il s'approche.
- La scène village (`scene0.psc`) contient `perso` (script `player`) et
  `pnj` (script `npc`) — le nouveau modèle `guy` (4 boîtes texturées).

Build (même toolchain que le player) :

```
cd runtime/game
cmake --preset default && cmake --build build
# puis build du projet démo (examples/demo/project.json pointe sur game.exe)
cd ../../pipeline/psxpipe && cargo run -- build ../../examples/demo
```

Boucle frame : `Input_Update → Scene_UpdateScripts → Scene_UpdateWorld →
Camera_GetViewMatrix → Scene_Draw → Dialog_Draw`.

## 4. Live tweaking — éditer le jeu pendant qu'il tourne

Découvert en vérifiant les sources de PCSX-Redux : son API web lit **et
écrit** la RAM console. La chaîne complète :

1. Le runtime publie une **balise** de 28 octets en RAM (magic
   `PSXSTUDIOBCN`, écrit en dernier) décrivant la table d'entités :
   adresse, taille d'entité, offsets de pos/rot/scale.
2. `psxpipe::redux` dump la RAM (`GET /api/v1/cpu/ram/raw`), **scanne le
   magic**, puis écrit les transforms ciblées (`POST …?offset=&size=`).
3. L'éditeur (commandes Tauri `redux_sync_entity`, balise mise en cache) :
   quand le jeu est **en cours** dans l'émulateur et qu'une transform est
   éditée dans l'inspecteur, elle est aussi poussée dans la RAM console —
   l'objet bouge **dans le jeu qui tourne**, sans rebuild ni reset. La
   balise est relocalisée à chaque Play/Reset, et invalidée si une
   écriture échoue.

Limites assumées : la synchro va de l'éditeur vers la console (pas
l'inverse), et un script qui écrit la même transform à chaque frame
(le `player` sur pos) reprend la main — le live tweak est fait pour le
décor et les PNJ statiques.

## 5. Vérification sans console

Comme aux phases précédentes : syntaxe C validée contre les vrais en-têtes
PSn00bSDK, `_Static_assert` sur les layouts (`Entity`, balise, en-têtes),
hash FNV identique testé des deux côtés (Rust ↔ C), client Redux testé
contre un faux serveur HTTP (y compris le scan de balise et les offsets
d'écriture RAM), scènes régénérées et rendues par le previewer logiciel.
Le test final sur émulateur réel reste à faire sur ta machine (voir §3).
