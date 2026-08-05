# Contribuer à PSX Studio

Merci de ton intérêt ! PSX Studio est un éditeur de jeux PS1 open source
(MIT) ; toute contribution est bienvenue — code, doc, assets d'exemple,
rapports de bug avec captures.

## Vue d'ensemble du code

| Dossier | Langage | Rôle |
|---|---|---|
| `runtime/engine/` | C (PSn00bSDK) | moteur console : scène, scripts, entrées, physique, SFX |
| `runtime/game/` | C | la démo jouable (et la base de code d'un jeu utilisateur) |
| `runtime/player/` | C | visionneuse de scènes (phases 2-3) |
| `pipeline/psxpipe/` | Rust | conversions glTF/PNG/WAV, packer de scènes/VRAM, build ISO, client PCSX-Redux |
| `editor/` | TypeScript + React + Three.js | frontend de l'éditeur |
| `editor/src-tauri/` | Rust | backend desktop (psxpipe en dépendance directe) |

Principes à respecter (détail dans `docs/psx-studio-doc-projet.md`) :
le pipeline Rust est la **seule** source de vérité des formats binaires ;
le runtime ne parse pas, il *fixe up* ; pas d'interpréteur de scripts ;
tout changement de format passe par les octets réservés ou une version.

## Lancer les validations

```bash
# Pipeline (obligatoire pour tout changement Rust ou de format)
cd pipeline/psxpipe && cargo test && cargo clippy --all-targets

# Éditeur
cd editor && npx tsc --noEmit && npm run build && npx vitest run

# Backend Tauri
cd editor/src-tauri && cargo check

# Runtime C sans toolchain MIPS : vérification de syntaxe contre les
# vrais en-têtes PSn00bSDK (clonés où tu veux)
gcc -fsyntax-only -std=c99 -I<psn00bsdk>/libpsn00b/include \
    -Iruntime/engine runtime/engine/*.c

# Préview logicielle d'une scène (rendu affine fidèle, sans émulateur)
cargo run --example preview -- runtime/player/assets/scene0.psc out.png
```

Un changement de format binaire doit mettre à jour : le writer Rust, le
parser TS de l'éditeur, le C du runtime, la spec dans `docs/`, et les
tests des trois côtés.

## Style

- Rust : `cargo fmt`, clippy sans warning.
- C : style PSn00bSDK (tabs, `_Static_assert` sur tout layout binaire).
- Commentaires : français ou anglais, du moment que c'est clair —
  expliquer les contraintes console (GTE, VRAM, OT) plutôt que le code.
- Commits : message descriptif, une fonctionnalité par commit.

## Idées balisées « good first issue »

Chacune est autonome et bien délimitée — parfaites pour découvrir le
projet. Ouvre une issue GitHub pour dire que tu la prends.

1. **Dithering Bayer dans le viewport** — le shader
   (`editor/src/viewport/ps1material.ts`) sort des couleurs lisses ; la
   console dithère en motif 4×4 avant le 15 bits. Ajouter le motif + une
   quantization RGB555 au fragment shader.
2. **Toggle « culling console »** — l'éditeur rend en `DoubleSide` pour
   l'ergonomie ; ajouter un bouton pour afficher le back-face culling
   réel (ce que le nclip GTE éliminera).
3. **Champ subdiv à l'import éditeur** — l'import par drag & drop
   convertit sans subdivision ; proposer le seuil dans une petite boîte
   de dialogue et le stocker dans `project.json` (le pipeline le gère
   déjà).
4. **Grille au sol dans le viewport** — une grille repère à Y=0 (dans le
   calque net, voir `viewport-overlay`).
5. **Sélecteur de piste musique** — `Music_Play(track)` existe côté
   runtime ; exposer la piste CD-DA à jouer par scène dans le JSON +
   l'éditeur.
6. **Undo/redo en mode visionneuse** — l'historique ne couvre que le
   mode projet ; en faire autant pour les overrides locaux.
7. **LOD par distance** — table de niveaux dans le PMD (format : via
   octets réservés), choix dans `Scene_Draw` selon l'avg-Z.
8. **Préchargement de scène (streaming CD)** — `CdRead` asynchrone de la
   scène suivante dans une seconde arène pendant le jeu.
9. **Icônes de la barre de gizmos** — remplacer ✥/⟳/⤢ par de vraies
   icônes SVG cohérentes avec le thème.
10. **`psxpipe info` pour les .vag** — la commande décode PMD/TIM/PSC
    mais pas les VAG (en-tête, durée, taille SPU).

## Licence

En contribuant, tu acceptes que ta contribution soit publiée sous licence
MIT (voir `LICENSE`). Les portions adaptées de PSn00bSDK restent sous MPL
avec leur attribution.
