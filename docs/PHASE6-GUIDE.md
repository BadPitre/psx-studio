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
- **Dithering Bayer + sortie 15 bits** dans le shader du viewport (le
  dernier écart visuel avec la console).
- **Toggle culling console** dans le viewport (l'éditeur rend
  double-face pour l'ergonomie).
- **Subdiv à l'import éditeur** : exposer le champ dans l'UI de drop.
- Voir la liste complète dans `CONTRIBUTING.md`.
