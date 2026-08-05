# PSX Studio — Documentation de projet

> Éditeur de jeux 3D pour PlayStation 1, philosophie Unity, 100 % open source.
> Document de référence v0.1 — Août 2026

---

## 1. Vision

Créer un outil de création de jeux **3D** pour PS1 qui reprend la philosophie de Unity : on ouvre un projet, on assemble des **scènes** faites d'**entités** portant des **composants**, on glisse des **assets** importés automatiquement, on appuie sur **Play**, et on exporte en un clic une image **`.bin/.cue` bootable** sur émulateur ou console réelle (via chip/unlock).

Principes directeurs :

- **Philosophie Unity, réalité PS1.** L'éditeur expose les concepts familiers (Scene, GameObject, Transform, Inspector, Prefab) mais rend les contraintes de la console **visibles en permanence** : budget polygones, occupation VRAM, virgule fixe.
- **100 % libre.** Aucune dépendance au SDK officiel Sony (PsyQ). Licence MIT pour l'outil, SDK open source côté runtime.
- **Le runtime est un "player".** Comme Unity, le moteur C embarqué sur le CD est générique : il charge des données de scène sérialisées. L'éditeur ne génère pas de code moteur, il génère des **données**.
- **Continuité avec snes-studio.** Même triptyque : moteur C sur console, outils de pipeline en Rust, éditeur Tauri + React + TypeScript. Les compétences et une partie de l'outillage sont mutualisables.

---

## 2. Comprendre la cible : contraintes matérielles PS1

Tout le design de l'outil découle de ces limites. À afficher dans l'éditeur, pas à cacher.

### CPU & géométrie
- **MIPS R3000A à ~33,8 MHz**, pas de FPU → **tout est en virgule fixe** (format GTE : 4.12 pour les fractions, coordonnées 16 bits signées).
- **GTE (Geometry Transformation Engine)** : coprocesseur qui fait les transformations, projections, éclairage et le test de backface culling (`nclip`). C'est lui qui rend la 3D viable — le runtime doit s'appuyer dessus massivement.
- **Scratchpad de 1 Ko** (D-cache détourné) : mémoire ultra-rapide pour les calculs chauds.

### Mémoire
- **2 Mo de RAM principale** (tout : code, données de scène, buffers audio, pile).
- **1 Mo de VRAM** : framebuffers + toutes les textures + palettes (CLUT). C'est la ressource la plus disputée du projet.
- **512 Ko de SPU RAM** pour les samples audio.

### GPU (rendu)
- **Pas de z-buffer** → tri par profondeur via **Ordering Table (OT)** : chaque primitive est insérée dans une liste chaînée indexée par profondeur.
- **Texture mapping affine** (pas de correction de perspective) → distorsion sur les grands polygones proches de la caméra. Mitigation : **subdivision dynamique** des polys.
- Coordonnées de sommets **entières à l'écran** → le fameux "vertex jitter" PS1 (que tu as déjà recréé en post-process dans UE5 — ici c'est gratuit 😄).
- Textures organisées en **pages de 256×256**, formats 4/8/16 bpp avec palettes (CLUT).
- Résolution typique **320×240**, double buffering.

### Budgets réalistes (à 30 fps)
| Ressource | Ordre de grandeur |
|---|---|
| Polygones affichés / frame | ~1 500 à 4 000 (textures + éclairage) |
| Vertices par personnage | 300–800 |
| Texture par personnage | 1 page 8 bpp ou moins |
| RAM dispo après moteur | ~1,5 Mo pour la scène |

### Stockage
- CD-ROM 2x : **~300 Ko/s**, seek très lent → l'agencement des fichiers sur l'ISO compte. Le pipeline doit packer les données d'une scène de façon contiguë.

---

## 3. Stack technique (100 % libre)

### Runtime console (C)
- **PSn00bSDK** (Lameguy64) — le SDK open source le plus complet : support GPU intégral (toutes primitives, DMA, ordering tables), **GTE**, CD-ROM, SPU, pads. API calquée sur le SDK officiel Sony, donc quasi toute la documentation et les tutoriels historiques s'appliquent. Toolchain GCC MIPS + **CMake**, outil **mkpsxiso** inclus pour générer le `.bin/.cue`. Utilisable pour projets freeware et commerciaux d'après son README (vérifier la licence exacte au moment de figer le projet).
- Alternative à évaluer : **PSYQo** (projet pcsx-redux/nugget, MIT) — bibliothèque C++ moderne. Plus élégante, mais l'écosystème et les exemples 3D sont plus riches côté PSn00bSDK. **Recommandation : PSn00bSDK** pour le POC ; décision définitive fin de Phase 1.

### Pipeline d'assets (Rust)
- CLI `psxpipe` : import glTF/OBJ/PNG/WAV → formats console. Crates : `gltf`, `image`, quantization de palettes (`color_quant` ou k-means maison), écriture binaire via `byteorder`/`binrw`.
- Réutilisation directe des patterns de tes outils Rust de snes-studio (watch mode, manifest d'assets, hashing incrémental).

### Éditeur (desktop)
- **Tauri + React + TypeScript** (même stack que snes-studio).
- **Viewport 3D : Three.js** avec un pipeline de shaders "PS1 authentique" (vertex snapping, affine mapping simulé, dithering Bayer, éclairage par sommet) pour que le WYSIWYG soit honnête — tu as déjà écrit exactement ces shaders pour UE5, la logique se transpose.
- Vues spécialisées : **VRAM Viewer** (packing des pages de textures), **compteur de polygones/OT**, inspecteur de virgule fixe.

### Émulation & debug
- **PCSX-Redux** : le compagnon idéal — débogueur intégré, serveur GDB, **API Lua et interface web/HTTP** permettant de piloter l'émulateur et lire la mémoire depuis l'éditeur → c'est la clé du "Play Mode".
- **DuckStation** pour la validation visuelle/compatibilité.
- Test final sur console réelle recommandé à partir de la Phase 5.

---

## 4. Architecture "Unity-like"

### 4.1 Structure d'un projet utilisateur
```
MonJeu/
├── project.json          # métadonnées, config build
├── Assets/
│   ├── Models/           # .gltf, .obj (sources)
│   ├── Textures/         # .png (sources)
│   ├── Audio/            # .wav (sources)
│   └── Scripts/          # .c (gameplay)
├── Scenes/
│   └── Main.scene.json   # scènes éditables (JSON lisible, diffable)
├── Library/              # cache des assets convertis (généré, ignoré en VCS)
└── Build/                # jeu.bin / jeu.cue (généré)
```

### 4.2 Modèle Entité-Composant
Une scène = hiérarchie d'entités. Chaque entité porte des composants. Set initial :

| Composant | Rôle | Notes PS1 |
|---|---|---|
| `Transform` | position/rotation/échelle, hiérarchie | stocké en fixed 4.12, l'éditeur affiche en flottant |
| `MeshRenderer` | référence mesh + material | material = texture(s) + mode (flat/gouraud, textured) |
| `Camera` | caméra active | FOV via distance de projection GTE |
| `Light` | directionnelle (×3 max) + ambiante | limite du GTE : 3 lumières dir. par scène |
| `Collider` | AABB / sphère | physique volontairement simple |
| `Script` | comportement C attaché | voir §7 |
| `AudioSource` | SFX (VAG) ou musique (XA) | budget SPU affiché |

**Prefabs** : une entité (et ses enfants) sauvegardée comme asset réutilisable, avec overrides par instance — indispensable pour peupler des niveaux.

### 4.3 Le runtime "player"
Le moteur C embarqué ne connaît pas ton jeu : il lit un **format binaire de scène** produit par le pipeline :

```
[SceneHeader] → [table des assets] → [entités: id, parent, composants packés]
```

- Chargement d'une scène = une lecture CD contiguë + fixup des pointeurs.
- Boucle : input → scripts → animation transforms → GTE transform + éclairage → insertion OT → DrawSync/VSync → swap.
- Gestion mémoire par **arènes par scène** (pas de malloc générique) : tout est libéré au changement de scène.

### 4.4 Sérialisation
- **Éditeur ↔ disque** : JSON versionné (lisible, mergeable en VCS — leçon Perforce).
- **Pipeline → console** : binaire packé little-endian, aligné, avec numéro de version de format. Le pipeline Rust est **la seule** source de vérité de la conversion JSON→binaire (l'éditeur ne fabrique jamais de binaire lui-même).

---

## 5. Pipeline d'assets

### Modèles 3D — glTF → format `PMD` (custom, inspiré du TMD Sony)
1. Import glTF/OBJ (export Blender direct).
2. Quantization des positions en **16 bits signés** avec facteur d'échelle par mesh.
3. Normales en 4.12 pour l'éclairage GTE.
4. UV remappés vers la page de texture assignée (coordonnées 8 bits).
5. Primitives **pré-encodées au format des paquets GPU** (POLY_FT3/GT3/FT4/GT4…) → à l'exécution, le moteur copie et complète, il ne construit pas.
6. Rapport d'import : nombre de polys, avertissements (mesh trop dense, UV hors page, échelle perdue).

### Textures — PNG → TIM
- Quantization **16 ou 256 couleurs** (choix par asset, préview du résultat dans l'éditeur avant validation).
- Génération des CLUT, **packing VRAM assisté** : l'éditeur montre la carte VRAM 1024×512 et laisse arranger/verrouiller les pages ; le pipeline vérifie les collisions avec les framebuffers.

### Audio
- SFX : WAV → **VAG** (ADPCM SPU), budget 512 Ko affiché.
- Musique : **CD-XA** streamé (simple, peu de RAM) en v1 ; séquencé plus tard si besoin.

### Build final
`psxpipe build` : convertit l'incrémental, packe les scènes, génère le layout ISO (fichiers d'une même scène contigus), appelle **mkpsxiso** → `.bin/.cue`.

---

## 6. Rendu 3D côté runtime — points clés

- **Double buffer** d'ordering tables + primitive buffers (on remplit la frame N+1 pendant que la N s'affiche).
- **Frustum culling** par AABB d'entité avant tout envoi au GTE ; **backface** via `nclip` du GTE.
- **Subdivision** des quads/tris trop grands à l'écran (seuil configurable par material) pour limiter le warping affine — activable par mesh dans l'inspecteur.
- **Éclairage** : vecteurs lumière chargés dans les registres GTE, couleur calculée par sommet (Gouraud).
- **LOD manuel** (v2) : deux meshes par MeshRenderer avec distance de bascule.

---

## 7. Scripting

Deux voies, dans l'ordre :

1. **Phase MVP — C natif.** Un `Script` = un fichier `.c` dans `Assets/Scripts/` exposant `OnStart(Entity*)` / `OnUpdate(Entity*)`, compilé avec le runtime au build. API moteur simple (`Entity_Find`, `Transform_Translate`, `Input_Held`, `Scene_Load`…). Puissant, zéro overhead, mais nécessite le toolchain → acceptable car il est installé avec l'outil.
2. **Phase ultérieure — VM bytecode**, réutilisant l'architecture de la VM snes-studio (Phase 1 de ce projet-là). Permet le hot-reload en Play Mode et un futur langage visuel de scripting. Décision après le MVP — ne pas construire les deux en parallèle.

---

## 8. Play Mode

Le "Play in Editor" est **simulé via PCSX-Redux** :

1. Build incrémental (secondes, grâce au cache `Library/`).
2. Lancement de PCSX-Redux avec l'exécutable + image.
3. L'éditeur se connecte à l'**API de PCSX-Redux** (Lua/web) pour : lire les positions des entités (adresse de la table d'entités connue via le format), afficher les stats (frame time, taille OT), mettre en pause, et à terme **modifier des valeurs en live** (transform tweaking).

C'est la fonctionnalité différenciante de l'outil — à prototyper tôt (Phase 4) pour valider la faisabilité.

---

## 8 bis. Décors précalculés (mode "Resident Evil / FF7")

Second mode de rendu de premier ordre, à côté de la 3D temps réel : **décor 2D pré-rendu + personnages 3D**, caméras fixes.

### Principe technique sur PS1
- Le décor est une image 320×240 (rendue dans Blender ou peinte), **découpée en tuiles** par le pipeline et dessinée en sprites via le GPU.
- **Occlusion sans z-buffer** : chaque tuile porte une **profondeur** et est insérée dans l'Ordering Table à la valeur correspondante. Les tuiles de premier plan (colonne, meuble, embrasure) passent *devant* les personnages 3D — c'est exactement la technique de FF7/RE.
- La **caméra 3D du runtime doit reproduire exactement la caméra du rendu** (position, rotation, focale) pour que la projection des persos colle au décor.
- Déplacement des personnages sur un **walkmesh** (mesh de navigation invisible, vraie 3D) ; le sol du décor n'est qu'une image.
- **Zones de bascule** : trigger volumes qui changent de plan (nouveau décor + nouvelle caméra), avec conservation de la position monde du joueur.
- VRAM/RAM : un décor 16 bpp plein écran est lourd → décors streamés depuis le CD au changement de plan (chargement masqué par une transition), tuiles statiques uploadées une fois.

### Nouveaux éléments dans l'outil
| Élément | Rôle |
|---|---|
| Asset `Backdrop` | image + découpe en tuiles + carte de profondeur par tuile |
| Composant `FixedCamera` | caméra verrouillée, importée depuis la caméra Blender (glTF) |
| Composant `CameraZone` | volume de déclenchement → active un couple Backdrop/FixedCamera |
| Asset `Walkmesh` | mesh de navigation importé (glTF, calque dédié) |
| Éditeur de profondeur | peinture des masques/depth par tuile sur le décor, avec préview de l'occlusion en direct dans le viewport |

### Workflow utilisateur cible
1. Modéliser la pièce dans Blender, placer la caméra, faire le rendu (l'image) et exporter la scène simplifiée (caméra + walkmesh + volumes) en glTF.
2. Importer le tout dans PSX Studio : l'image devient un `Backdrop`, la caméra un `FixedCamera`, le walkmesh est reconnu par convention de nommage.
3. Peindre/ajuster les profondeurs des tuiles de premier plan dans l'éditeur de profondeur (aidé, si dispo, par un rendu depth exporté de Blender).
4. Poser les `CameraZone` et tester en Play Mode.

Ce mode est aussi le plus économe pour la console (peu de polys à l'écran → persos plus détaillés), et il rend l'outil unique : aucun moteur moderne ne propose ce workflow nativement.

---

## 9. Plan de développement par phases

> Même logique que snes-studio : phases courtes, livrable démontrable, critère de sortie explicite. Chaque phase produit une entrée dans `ClaudeHub`.

### Phase 0 — Fondations (1–2 semaines)
- Installer PSn00bSDK + toolchain GCC MIPS + CMake sur Windows (les leçons de l'install PVSnesLib s'appliquent : chemins sans espaces, PATH, préférer les releases précompilées).
- Compiler les exemples du SDK, lancer sur PCSX-Redux et DuckStation.
- Repo Git mono-repo : `runtime/` (C), `pipeline/` (Rust), `editor/` (Tauri), `docs/`.
- **Sortie :** "hello triangle 3D" tournant en rotation, buildé par script, sur les deux émulateurs.

### Phase 1 — POC rendu & pipeline minimal (2–4 semaines)
- Outil Rust `gltf2pmd` : un cube puis un mesh Blender → format PMD.
- Outil `png2tim` avec quantization.
- Runtime : chargement PMD, caméra orbitale au pad, éclairage 1 directionnelle + ambiante, OT, fixed-point propre.
- **Sortie :** un modèle Blender texturé et éclairé qui tourne à 30 fps. **Décision SDK figée ici.**

### Phase 2 — Format de scène & runtime player (3–4 semaines)
- Spécification binaire `SceneFormat v1` (documentée dans `docs/`).
- Sérialiseur Rust JSON→binaire ; runtime générique qui charge la scène, la hiérarchie de transforms, plusieurs entités.
- Arènes mémoire, changement de scène.
- **Sortie :** deux scènes JSON écrites à la main, chargées et naviguables, transition entre les deux.

### Phase 3 — Pipeline complet & VRAM (2–3 semaines)
- Packer VRAM avec export de la carte, gestion des CLUT, atlas.
- Audio VAG + lecture XA.
- `psxpipe build` de bout en bout → `.bin/.cue` via mkpsxiso, cache incrémental.
- **Sortie :** build one-command d'un projet exemple avec son et musique.

### Phase 4 — Éditeur MVP (4–6 semaines)
- Tauri + React : arborescence projet, hiérarchie de scène, inspecteur de composants, viewport Three.js avec shaders PS1.
- Import d'assets par drag & drop (appelle le pipeline), VRAM Viewer, compteur de polys.
- Bouton **Play** → build + lancement PCSX-Redux ; connexion API pour les stats de base.
- **Sortie :** créer une scène entièrement dans l'éditeur, sans toucher au JSON, et la lancer.

### Phase 5 — Gameplay (3–4 semaines)
- Composants Collider (AABB), API de scripts C, input mappé dans l'éditeur.
- Caméra contrôlable par script, spawn de prefabs.
- Premier test sur **console réelle**.
- **Sortie :** mini-démo jouable "marcher dans un village et parler à un PNJ" (Croûton-sur-Mie en 3D ? 😉).

### Phase 5 bis — Décors précalculés (3–4 semaines)
- Pipeline `Backdrop` : découpe en tuiles, profondeur par tuile, streaming CD au changement de plan.
- Import caméra + walkmesh depuis glTF (conventions de nommage Blender).
- Éditeur de profondeur avec préview d'occlusion, composants `FixedCamera` / `CameraZone`.
- **Sortie :** démo "deux pièces façon Resident Evil" — persos 3D occlus par le décor, bascule de caméra fonctionnelle.

### Phase 6 — Consolidation & ouverture (continu)
- Streaming CD par scène, LOD, subdivision anti-warping.
- Documentation utilisateur, projet template, exemples.
- Publication MIT, README, premières issues "good first issue".

---

## 10. Risques & mitigations

| Risque | Impact | Mitigation |
|---|---|---|
| VRAM ingérable à la main | bloquant UX | VRAM Viewer dès la Phase 3, packing assisté |
| Warping affine décevant | qualité visuelle | subdivision configurable + budgets recommandés dans l'import |
| API PCSX-Redux insuffisante pour le Play Mode | perte de la killer feature | prototype de connexion dès la Phase 4, fallback = launch simple + logs série |
| Scope creep (le syndrome DialogueMap 😅) | délais | MVP strict = "une scène créée dans l'éditeur tourne sur émulateur" ; toute feature hors MVP va dans un backlog v2 |
| Deux projets studio en parallèle (snes-studio) | dispersion | mutualiser le pipeline Rust et l'ossature Tauri ; ne pas mener deux Phase 1 en même temps |

---

## 11. Références

- PSn00bSDK : `github.com/Lameguy64/PSn00bSDK` (SDK, exemples, mkpsxiso)
- PSYQo : `github.com/pcsx-redux/nugget` (alternative C++ MIT)
- PCSX-Redux : émulateur/débogueur avec API
- Spécifications matérielles : document *nocash PSX specs* (psx-spx)
- Tutoriels : Lameguy's PlayStation Programming Series ; annuaire `ps1.consoledev.net`
