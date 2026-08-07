# PSX Script — le gameplay sans C

PSX Script est le langage de script de PSX Studio : un fichier
`scripts/<nom>.psxs` dans ton projet, attaché à une entité par la carte
**Script** de l'inspecteur (le nom du fichier, sans extension). Pas de
compilation du jeu, pas de toolchain : psxpipe compile le script en
**bytecode** embarqué dans la scène (`.psc`), et la petite **VM** du
runtime l'exécute. Édite → Play → c'est là.

C'est l'architecture des jeux de l'époque (les scripts de terrain de
Final Fantasy VII sont un bytecode interprété) — pas un interpréteur
générique embarqué :

- **entiers uniquement** (unités monde, angles où 4096 = un tour) ;
- **zéro allocation, zéro garbage collector** : les variables ont des
  emplacements fixes (16 registres de 32 bits par entité), décidés à la
  compilation ;
- une instruction VM = un mot de 32 bits, la boucle est un `switch` ;
  les opérations lourdes (déplacement + collisions, dialogue…) sont des
  **appels natifs du moteur** — la VM n'orchestre que la logique ;
- erreurs détectées **à la compilation** avec ligne et message clair ;
  à l'exécution la VM est un bac à sable (bornes vérifiées, division
  par zéro = 0, boucle infinie coupée à 4096 pas par frame) : un script
  ne peut pas planter la console.

Mesuré sur le banc d'essai : ~25 instructions VM coûtent quelques
microseconds par frame même rapporté au R3000 à 33 MHz — des dizaines
de scripts tiennent dans une frame. Le C (`runtime/game/scripts/`)
reste disponible pour les cas extrêmes.

## 1. Un script

```
# Girouette : tourne, et salue le joueur proche.
var vitesse = 24
var pnj

quand demarre
    pnj = trouve("npc")
fin

chaque frame
    tourner_y(moi, vitesse)
    si pnj != rien et distance(moi, pnj) < 120 et touche_pressee(CROIX) alors
        dialogue("SALUT ! / JE TOURNE SANS UNE LIGNE DE C.")
    fin
fin
```

- `#` commente jusqu'à la fin de la ligne ; une instruction par ligne.
- `var nom` ou `var nom = expression` — en tête de fichier uniquement,
  12 variables max (les 4 registres restants servent aux calculs). Les
  variables sont **par entité** : deux entités portant le même script
  ont chacune les leurs.
- `quand demarre ... fin` : au chargement de la scène.
- `chaque frame ... fin` : à chaque frame (60 Hz), avant le rendu.
  Gelé pendant le menu pause.

## 2. Contrôle

```
si <condition> alors
    ...
sinon            # optionnel
    ...
fin

tantque <condition> faire
    ...
fin
```

Opérateurs : `+ - * / %`, comparaisons `< <= > >= == !=`, logique
`et ou non`, parenthèses. Vrai = tout sauf 0.

## 3. Fonctions du moteur

| Fonction | Effet |
|---|---|
| `moi` | l'entité qui porte le script |
| `rien` | entité absente (retour de `trouve` sans résultat) |
| `trouve("nom")` | première entité portant le script « nom » (C ou PSX Script) |
| `pos_x(e)` `pos_y(e)` `pos_z(e)` | position (unités monde, +Y vers le bas) |
| `poser_x(e, v)` `poser_y` `poser_z` | téléporte sur un axe |
| `bouger(e, dx, dz)` | déplace **avec collisions** (glisse sur les murs) |
| `rot_y(e)` / `poser_rot_y(e, v)` / `tourner_y(e, delta)` | cap (4096 = un tour) |
| `touche(B)` / `touche_pressee(B)` | bouton tenu / vient d'être pressé — B : `CROIX ROND CARRE TRIANGLE HAUT BAS GAUCHE DROITE START SELECT` |
| `distance(a, b)` | distance XZ approchée (rapide, ~4 %) en unités monde |
| `dialogue("L1 / L2 / L3")` | boîte de dialogue (3 lignes, `/` = retour) |
| `dialogue_ouvert()` / `fermer_dialogue()` | état / fermeture |
| `montrer(e, 0/1)` | cache/montre (rendu **et** collisions) |
| `changer_scene()` | demande la bascule vers la scène préchargée |
| `hasard(n)` | entier 0..n-1 |

Les textes sont translittérés vers le charset des polices `.fnt`
(majuscules, accents aplatis).

## 4. Sous le capot (format)

- psxpipe compile chaque `scripts/<nom>.psxs` référencé par la scène en
  blob **PSB1** : en-tête 16 octets (magic, nb constantes, taille code,
  points d'entrée demarre/frame, taille chaînes), constantes i32, code
  (u32 par instruction : op/a/b/c), chaînes.
- Le `.psc` gagne (v1.5, bit 0 des flags d'en-tête) une **table
  d'offsets** juste après la table de hashes de scripts : un u32 par
  script, 0 = script C du registre, sinon l'offset du blob. Un runtime
  ancien ignore flag et table — rétrocompatible.
- Au chargement, un blob présent **prime sur le registre C** pour ce
  nom. La VM (`engine/vm.c`, liée par le jeu seulement) instancie un
  jeu de registres par entité scriptée (24 instances max) et exécute
  `quand demarre` puis `chaque frame`.

## 5. Limites v1 (assumées)

- Pas de fonctions utilisateur ni de tableaux ; 12 variables/script.
- Pas encore d'API UI (`jauge`, `texte`) ni audio — prochain jalon,
  avec le rechargement à chaud pendant que l'émulateur tourne.
- `distance` est approchée ; les angles sont des entiers 4.12.
