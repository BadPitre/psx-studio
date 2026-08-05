# Phase 6 — Consolidation & ouverture (continu)

Cette phase n'a pas de « fin » : c'est l'entretien du studio et son
ouverture aux contributions. Premier lot livré :

## 1. Subdivision anti-warping

Le défaut PS1 le plus visible : les textures qui « nagent » sur les
grands triangles (mapping affine, pas de correction de perspective).
Le pipeline sait maintenant subdiviser à l'import :

- `psxpipe gltf2pmd modele.gltf --subdiv 48`
- ou par modèle dans `project.json` : `{ "gltf": "...", "subdiv": 48 }`

Le seuil est la **longueur d'arête maximale en unités PMD**. L'algorithme
coupe récursivement l'arête la plus longue de chaque triangle (positions,
normales et UV interpolées au milieu, en flottant avant quantization —
pas d'erreur cumulative), avec un plafond de sécurité à 20 000 triangles.
L'option entre dans le hash du cache : changer `subdiv` reconvertit le
modèle, rien d'autre.

Vérifié par test (aucune arête au-dessus du seuil après conversion) et
par le previewer logiciel : de biais, une maison non subdivisée étale sa
porte en diagonale sur le mur ; subdivisée à 48, portes et fenêtres
restent en place. Le sol de la démo utilisait déjà une grille modélisée
à la main (Phase 1) — c'est maintenant automatique pour tout modèle.

## 2. Ouverture du projet

- **`docs/GETTING-STARTED.md`** : la doc *utilisateur* (les guides de
  phase sont le journal de construction, celle-ci est le mode d'emploi) :
  prérequis, build, créer son projet, workflow Blender, écrire du
  gameplay.
- **`CONTRIBUTING.md`** : setup dev, les validations à lancer par
  composant, conventions, et une liste d'améliorations balisées
  « good first issue » prêtes à publier sur GitHub.
- **`LICENSE`** : le fichier MIT manquait (seule la mention existait) ;
  l'attribution MPL des portions PSn00bSDK y est rappelée.
- **README** réécrit : pitch, capacités actuelles, démarrage rapide.

## 3. Reste ouvert (candidats aux issues)

- **LOD** : table de niveaux de détail par modèle dans le PMD, choix par
  distance dans `Scene_Draw`.
- **Streaming CD** : précharger `SCENE<n+1>.PSC` pendant le jeu (lecture
  CD asynchrone, l'arène sait déjà tout charger d'un bloc).
- Voir la liste à jour dans `CONTRIBUTING.md`.

## 4. Second lot livré

- **Réglage de subdivision dans l'inspecteur** (menu à presets :
  désactivée/légère/moyenne/forte) — écrit `project.json` et reconvertit
  le modèle immédiatement pour le viewport.
- **Dithering console dans le viewport** : matrice Bayer 4×4 exacte du
  GPU (calculée arithmétiquement dans le fragment shader) + quantization
  RGB555 — le rendu 320×240 natif fait de `gl_FragCoord` le pixel
  console. Toggle ▦ en haut à droite du viewport (défaut : activé).
- **Toggle culling console** ◪ : cache les faces arrière comme le nclip
  GTE (la racine du graphe porte un miroir Y, la face avant console
  correspond à `BackSide` en GL). Défaut : double face, plus ergonomique
  pour éditer.
