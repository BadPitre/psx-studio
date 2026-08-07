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
var speed = 24
var npc

on start
    npc = find("npc")
end

every frame
    rotate_y(self, speed)
    if npc != nil and distance(self, npc) < 120 and pressed(CROSS) then
        dialog("SALUT ! / JE TOURNE SANS UNE LIGNE DE C.")
    end
end
```

- `#` commente jusqu'à la fin de la ligne ; une instruction par ligne.
- `var nom` ou `var nom = expression` — en tête de fichier uniquement,
  12 variables max (les 4 registres restants servent aux calculs). Les
  variables sont **par entité** : deux entités portant le même script
  ont chacune les leurs.
- `on start ... end` : au chargement de la scène.
- `every frame ... end` : à chaque frame (60 Hz), avant le rendu.
  Gelé pendant le menu pause.

## 2. Contrôle

```
if <condition> then
    ...
else             # optionnel
    ...
end

while <condition> do
    ...
end
```

Opérateurs : `+ - * / %`, comparaisons `< <= > >= == !=`, logique
`and or not`, parenthèses. Vrai = tout sauf 0.

## 3. Fonctions et « classes »

**Un fichier `.psxs` est une classe** au sens Unity : ses `var` de tête
de fichier sont les **champs** (une copie par entité qui porte le
script), `on start`/`every frame` le cycle de vie, et tu définis tes
**méthodes** avec `function` :

```
# chest.psxs — un coffre ouvrable (une instance par entité).
var opened = 0
var player

function dist_to_player()
    return distance(self, player)
end

function open_chest()
    if opened == 0 then
        opened = 1
        dialog("UN TRESOR !")
    end
end

on start
    player = find("player")
end

every frame
    if opened == 0 and dist_to_player() < 100 and pressed(CROSS) then
        open_chest()
    end
end
```

- `function nom(p1, p2) ... end` — au niveau du fichier, dans
  n'importe quel ordre (les appels avant définition sont permis).
- `return expression` renvoie une valeur, `return` seul (ou la fin du
  corps) renvoie 0. `return` ne s'utilise que dans une function.
- **Locales** : `var x [= expr]` en **tête de corps** de function —
  paramètres + locales vivent dans la frame d'appel (12 max), les
  champs du fichier restent accessibles en lecture/écriture.
- La **récursion** marche (profondeur 8 max — au-delà, l'appel renvoie
  0 au lieu de faire déborder la console). L'arité est vérifiée à la
  compilation, un nom du moteur ne peut pas être redéfini.
- Sous le capot : pile de frames **statique** dans la VM (zéro
  allocation) — un appel coûte quelques instructions, la récursion de
  `fact(5)` à chaque frame ne se voit pas au chronomètre.

## 4. Fonctions du moteur

| Fonction | Effet |
|---|---|
| `self` | l'entité qui porte le script |
| `nil` | entité absente (retour de `find` sans résultat) |
| `find("nom")` | première entité portant le script « nom » (C ou PSX Script) |
| `pos_x(e)` `pos_y(e)` `pos_z(e)` | position (unités monde, +Y vers le bas) |
| `set_x(e, v)` `set_y` `set_z` | téléporte sur un axe |
| `move(e, dx, dz)` | déplace **avec collisions** (glisse sur les murs) |
| `rot_y(e)` / `set_rot_y(e, v)` / `rotate_y(e, delta)` | cap (4096 = un tour) |
| `held(B)` / `pressed(B)` | bouton tenu / vient d'être pressé — B : `CROSS CIRCLE SQUARE TRIANGLE UP DOWN LEFT RIGHT START SELECT` |
| `distance(a, b)` | distance XZ approchée (rapide, ~4 %) en unités monde |
| `dialog("L1 / L2 / L3")` | boîte de dialogue (3 lignes, `/` = retour) |
| `dialog_open()` / `close_dialog()` | état / fermeture |
| `show(e, 0/1)` | cache/montre (rendu **et** collisions) |
| `switch_scene()` | demande la bascule vers la scène préchargée |
| `random(n)` | entier 0..n-1 |

Les textes sont translittérés vers le charset des polices `.fnt`
(majuscules, accents aplatis).

## 5. Sous le capot (format)

- psxpipe compile chaque `scripts/<nom>.psxs` référencé par la scène en
  blob **PSB1** : en-tête 16 octets (magic, nb constantes, taille code,
  points d'entrée start/frame, taille chaînes), constantes i32, code
  (u32 par instruction : op/a/b/c), chaînes.
- Le `.psc` gagne (v1.5, bit 0 des flags d'en-tête) une **table
  d'offsets** juste après la table de hashes de scripts : un u32 par
  script, 0 = script C du registre, sinon l'offset du blob. Un runtime
  ancien ignore flag et table — rétrocompatible.
- Au chargement, un blob présent **prime sur le registre C** pour ce
  nom. La VM (`engine/vm.c`, liée par le jeu seulement) instancie un
  jeu de registres par entité scriptée (24 instances max) et exécute
  `on start` puis `every frame`.

## 6. Limites v1 (assumées)

- Pas de tableaux ni de chaînes manipulables ; 12 champs/script,
  12 paramètres + locales par function.
- Pas encore d'API UI (`gauge`, `text`) ni audio — prochain jalon,
  avec le rechargement à chaud pendant que l'émulateur tourne.
- `distance` est approchée ; les angles sont des entiers 4.12.
