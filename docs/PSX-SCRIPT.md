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

- Un objet peut porter **plusieurs scripts** (autant de composants
  dans l'inspecteur) : chacun a ses propres champs et son propre cycle
  de vie, exécutés dans l'ordre d'ajout.
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

## 4. Champs `public` (réglables dans l'inspecteur)

Préfixe une déclaration par `public` et elle apparaît dans la carte du
composant, **réglable par objet** — comme un champ sérialisé d'un
MonoBehaviour Unity :

```
public var speed = 24          # nombre (défaut 24)
public var actif : bool = 1    # case à cocher
public var target : entity     # sélecteur d'objet de la scène
```

- Types : `int` (défaut), `bool`, `entity`. Un champ `entity` liste les
  objets de la scène (GameObject, **caméra**, lumière…) et donne au
  script l'entité choisie, utilisable directement :
  `distance(self, target)`, `pos_x(target)`, `move(target, …)`,
  `camera(target)`. Non réglé, il vaut `nil` (`if target != nil then`).
- La valeur réglée dans l'inspecteur **écrase le défaut du script**
  avant `on start` ; sans réglage, le défaut s'applique.
- Chaque objet a ses propres valeurs (deux tourelles, deux vitesses).
- Côté scène, ça s'écrit
  `"scripts": [{ "name": "turret", "values": { "speed": 42, "target": "cible" } }]` ;
  les références sont résolues en index d'entité **au build** (la
  console ne manipule que des nombres), et un objet introuvable est une
  erreur claire.

Exemple — une tourelle qui vise un objet réglé dans l'inspecteur :

```
public var speed = 8
public var target : entity

every frame
    if target != nil and distance(self, target) < 300 then
        rotate_y(self, speed)
    end
end
```

## 5. Fonctions du moteur

| Fonction | Effet |
|---|---|
| `self` | l'entité qui porte le script |
| `nil` | entité absente (retour de `find` sans résultat) |
| `find("nom")` | première entité portant le script « nom » (C ou PSX Script) |
| `pos_x(e)` `pos_y(e)` `pos_z(e)` | position (unités monde, +Y vers le bas) |
| `set_x(e, v)` `set_y` `set_z` | téléporte sur un axe |
| `move(e, dx, dz)` | déplace **avec collisions** (glisse sur les murs) |
| `rot_y(e)` / `set_rot_y(e, v)` / `rotate_y(e, delta)` | cap (4096 = un tour) |
| `rot_x(e)` / `set_rot_x(e, v)` | tangage (positif = regard vers le haut) |
| `camera(e)` | l'entité **devient la vue** (elle garde son FOV et sa distance de rendu) |
| `camera(x, y, z, cap, tangage)` | vue libre, sans entité caméra |
| `sin(a)` / `cos(a)` | trigonométrie en 4.12 (`4096` = 1.0), angle en unités projet |
| `held(B)` / `pressed(B)` | bouton tenu / vient d'être pressé — B : `CROSS CIRCLE SQUARE TRIANGLE UP DOWN LEFT RIGHT L1 R1 L2 R2 START SELECT` |
| `lstick_x()` `lstick_y()` `rstick_x()` `rstick_y()` | sticks analogiques, **-128..127** (0 au repos, Y négatif = vers le haut) |
| `analog()` | 1 si la manette est en mode analogique (sticks lisibles) |
| `distance(a, b)` | distance XZ approchée (rapide, ~4 %) en unités monde |
| `dialog("L1 / L2 / L3")` | boîte de dialogue (3 lignes, `/` = retour) |
| `dialog_open()` / `close_dialog()` | état / fermeture |
| `show(e, 0/1)` | cache/montre (rendu **et** collisions) |
| `switch_scene()` | demande la bascule vers la scène préchargée |
| `random(n)` | entier 0..n-1 |

Les textes sont translittérés vers le charset des polices `.fnt`
(majuscules, accents aplatis).

**Repères** : cap 0 = regard vers **-Z** (comme les modèles), le cap
tourne vers la droite de l'écran quand il augmente ; +Y va vers le
**bas**. Un champ `entity` non réglé vaut `nil` — teste-le avant usage.

## 5 bis. Caméra à la première personne

Le projet démo contient `scripts/fps.psxs`, un contrôleur FPS complet
(~60 lignes, aucune ligne de C) : regard, marche relative au regard,
pas de côté, course, collisions, butée du regard.

1. Sélectionne l'objet joueur → **＋ Ajouter un composant → fps**.
2. Ajoute une caméra (clic droit dans la hiérarchie → **Créer un
   enfant → Caméra**, ou le menu ＋), puis glisse-la dans le champ
   **cam** de la carte du script.
3. Règle si besoin `speed`, `turn_speed`, `look_speed`, `eye_height`,
   `run_factor`, `look_limit`, `deadzone` — par objet, sans recompiler
   le jeu.

**Manette analogique** (DualShock, LED rouge) : stick **gauche** =
déplacement (dosé : à mi-course on marche, à fond on file), stick
**droit** = regard. **Manette numérique** — ou en plus : D-pad haut/bas
avance/recule, gauche/droite tourne, L1/R1 pas de côté, L2/R2 lève et
baisse le regard. CROIX court dans les deux cas.

Les sticks ne demandent aucun réglage côté moteur : `lstick_x()` et
compagnie rendent 0 sur une manette numérique, le même script marche
donc dans les deux cas. Sous émulateur, pense à choisir une manette
analogique dans la configuration des contrôleurs.

Le regard tient en deux lignes (zone morte comprise, `dead()` est une
`function` du script) :

```
yaw = yaw + dead(rstick_x()) * turn_speed / 128
pitch = pitch - dead(rstick_y()) * look_speed / 128
```

Le cœur de la caméra tient en trois lignes — c'est une entité comme une
autre, le script la place :

```
set_x(cam, pos_x(self))
set_rot_y(cam, yaw)
camera(cam)              # cette entité devient la vue
```

Sans caméra réglée, `camera(x, y, z, cap, tangage)` pose une vue libre
au même endroit : le script marche même « nu ».

## 6. Sous le capot (format)

- psxpipe compile chaque `scripts/<nom>.psxs` référencé par la scène en
  blob **PSB1** : en-tête 16 octets (magic, nb constantes, taille code,
  points d'entrée start/frame, taille chaînes), constantes i32, code
  (u32 par instruction : op/a/b/c), chaînes.
- Champs publics (v1.7, bit 2 des flags) : le bytecode passe en
  « PSB2 » (en-tête 20 o + table des registres publics) et la scène
  porte une table `(entité, script, champ, valeur)` ; l'opcode
  `INITPUB` applique la valeur au démarrage du script.
- Plusieurs scripts sur une entité (v1.6, bit 1 des flags) : une table
  de composants `(entité, script)` suit la table des scripts ; le JSON
  s'écrit `"scripts": ["a", "b"]` (`"script": "a"` reste valide pour un
  seul). Une entité mono-script ne change rien au binaire.
- Le `.psc` gagne (v1.5, bit 0 des flags d'en-tête) une **table
  d'offsets** juste après la table de hashes de scripts : un u32 par
  script, 0 = script C du registre, sinon l'offset du blob. Un runtime
  ancien ignore flag et table — rétrocompatible.
- `camera(entité)` ne pose pas la vue tout de suite : elle est
  **réclamée**, puis appliquée par la boucle de jeu après la mise à jour
  des matrices monde (la caméra suit donc son parent au pixel près), et
  la demande ne dure qu'une frame — un script qui cesse de la poser rend
  la main à la caméra précédente.
- `sin`/`cos` tapent directement dans `isin`/`icos` du SDK : même unité
  d'angle que le projet (4096 = un tour), résultat en 4.12.
- Au chargement, un blob présent **prime sur le registre C** pour ce
  nom. La VM (`engine/vm.c`, liée par le jeu seulement) instancie un
  jeu de registres par entité scriptée (24 instances max) et exécute
  `on start` puis `every frame`.

## 7. Limites v1 (assumées)

- Pas de tableaux ni de chaînes manipulables ; 12 champs/script,
  12 paramètres + locales par function.
- Pas encore d'API UI (`gauge`, `text`) ni audio — prochain jalon,
  avec le rechargement à chaud pendant que l'émulateur tourne.
- `distance` est approchée ; les angles sont des entiers 4.12.
