# FalloutMapper

Visionneuse (puis éditeur) de cartes Fallout 2, 100 % Rust.

## Structure

| Crate | Rôle |
|---|---|
| `fo-core` | DAT2, VFS (dossiers + archives), FRM, PAL, LST, PRO, **MAP (lecture/écriture sans perte)**, géométrie hex/iso |
| `fo-scene` | Scène d'une élévation (sprites ordonnés, repères) + rendu logiciel |
| `fo-script` | Scripts `.int` : procédures, tables de chaînes, 641 opcodes (Fallout 2 + sfall), désassemblage, pseudo-source structuré avec textes des `.msg` en commentaire, variables nommées (globales, exportées, `argN`/`LVarN`) (`if`/`else if`/`while`/`do-while`/`break`) |
| `fo-cli` | `fomap` : `list`, `info`, `render` (PNG), `script`, `ssl` (analyse d'une source SSL), `analyze`, `graph`, `reach`, `check`, `frm`, `gvars` (voir « Outils en ligne de commande ») |
| `fo-mapper` | Application egui : onglets **Carte** (pan/zoom, couches, inspecteur d'objet) et **Scripts** (scripts de la carte, tous les scripts, procédures, pseudo-source / listing) |

## Utilisation

```
cargo run --release -p fo-mapper -- C:\Fallout2\F2_RPU_2.4.34 arvillag
cargo run --release -p fo-cli -- -g C:\Fallout2\F2_RPU_2.4.34 render arvillag -o arvillag.png
cargo test -p fo-core   # FO2_GAME_DIR / FO2_GAME_DIR_PACKED pour pointer vers vos installations
```

Fallout 1 est aussi ouvert (archives DAT1, cartes v19) : édition des cartes ; F5 n'y est pas disponible.
Le dossier du jeu peut contenir `master.dat`/`critter.dat` compressés ou des dossiers décompressés du même nom ;
`fallout2.cfg` et le dossier `data` (correctifs) sont pris en compte.

## Édition

Menu **Édition → Mode édition** : sélection/déplacement par glisser (Suppr, R, Ctrl+D), peinture des sols et toits,
placement d'objets depuis les prototypes, point de départ du joueur, annulation/rétablissement (Ctrl+Z / Ctrl+Y).
Sélection (comme Gecko) : clic, re-clic pour parcourir les éléments superposés, glisser sur le vide pour une zone
(Alt ajoute, Ctrl retire, Maj + clic étend une plage : cases, ou objets dans le rectangle isométrique jusqu'à l'objet cliqué), Ctrl+A / Ctrl+D / Échap, filtre objets/sols/toits ;
déplacement, rotation (`R`), duplication (Ctrl+J) et suppression de groupes. Navigation : `F` ajuste, `Début` centre
sur le départ, Ctrl+1/2/3 changent d'élévation, clic droit annule l'outil.
Outil **Sortie** : un hexagone ou le tracé d'un bord (marqueurs sur toute la ligne, seconde rangée pour les diagonales,
Espace pour l'autre côté), destination (carte, carte du monde, plan de ville), propriétés d'une sortie sélectionnée.
Outil **Bloqueurs** : un rectangle pose des bloqueurs de défilement sur son contour. Les limites de défilement (`.edg`,
format EDGE v1/v2 sans perte) se lisent et s'éditent dans « En-tête de la carte » ; le fichier est écrit à côté de la carte.
Pipette : `P` sous le curseur prélève un toit ou un sol (arme la peinture) ou un objet (arme « Placer » avec son
orientation ; `R` tourne le fantôme). Maj pendant la peinture d'un sol : peint le toit. Outil **Pièce** : après l'analyse de toutes les cartes, choisissez un style de murs appris des cartes du jeu, la longueur des deux murs du fond et une famille de sol, puis cliquez : la pièce est posée (Ctrl+Z l'annule). Affichage → **Aperçu du jeu** (ou l'œil de la barre du bas) : la carte comme à l'écran du jeu, avec jour, soir ou nuit, la lumière de chaque objet lumineux, les toits masqués autour du point de départ du joueur (ou du curseur), sans repères d'édition, et un cadre de l'écran du jeu (640 × 480). Menu Édition : **Rechercher un objet** (par nom, fichier ou PID de prototype : parcourir, sélectionner, remplacer par le prototype de « Placer »), **Vérifier la carte** (objets hors grille ou en double, prototypes, scripts, images et sorties introuvables, départ invalide ; un clic va à l'objet, « Tout corriger » répare ce qui peut l'être, annulable), **Textes (.msg)** (éditer les noms et descriptions du jeu ; seules les entrées modifiées sont réécrites, dans `data/text/`) et **Comparer avec une version** (version enregistrée ou copie `.bak` : ajouts, retraits, déplacements surlignés sur la carte). L'outil Pièce construit aussi des bâtiments de plusieurs pièces (tailles variables, étage avec escaliers) et garde le résultat comme motif. Affichage → **Minicarte** : vue d'ensemble de l'élévation en bas à droite du canevas, avec le cadre de la vue et le départ du joueur ; un clic ou un glissé la déplace. Fichier → **Générer une carte** : un village cohérent à partir de paramètres et d'une graine (taille, bâtiments et leurs pièces, espace entre bâtiments, sorties vers la carte du monde par côté, routes, végétation, menus objets, bande d'arbres au bord ; style des murs, sols et toit comme l'outil Pièce ; décor appris d'une carte du jeu au choix) ; chaque porte est reliée à la route, les sorties et le départ sont atteignables à pied, et « Relancer » tire une autre graine. Outil Sélection : trois modes, **Éléments** (filtre objets, sols, toits), **Hexagones** (sélection d'hexagones, avec « Supprimer les objets sur ces hexagones ») et **Bloqueurs de défilement** ; clic répété au même endroit : élément suivant (toit, objet, sol) ; Alt ajoute, Ctrl retire. Langues de l'interface : français, anglais, espagnol, italien, allemand, portugais, russe (Options → Interface). Fichier → **Exporter une image** : PNG de la carte entière (échelle 1:1, 1:2 ou 1:4, éclairage jour/soir/nuit en option) ou de l'écran du jeu (640 × 480 autour du départ du joueur). Affichage → **Accessibilité** : hexagones bloqués (rouge), créatures (orange), atteignables à pied depuis le départ du joueur (vert) et libres mais hors d'atteinte (bleu) ; le vérificateur signale aussi les sorties, portes et escaliers inaccessibles. Édition → **Bibliothèque de motifs** : miniatures de tous les tampons de `data/stamps/`, étiquettes (maison, entrepôt, bureau…, enregistrées dans le motif), filtre, et glisser-déposer d'une miniature sur la carte (mode édition). Récupération : toutes les 2 minutes, chaque carte modifiée est copiée dans le dossier de configuration de l'éditeur ; après un arrêt brutal, une fenêtre propose de les restaurer. Outil **Tampon** : capture la
sélection (objets et cases) comme motif, enregistré en JSON compatible Gecko dans `data/stamps/` ; clic pour poser,
`R` pour changer de variante, aperçu sous le curseur.
Objet sélectionné : tous les drapeaux nommés, rayon et intensité de lumière, script (attacher / détacher / voir, liste
« Objet » ou « Créature » comme le moteur), inventaire (ajout, quantités, retrait). Les créatures se créent depuis leur
prototype (points de vie, points d'action, IA et équipe du prototype). « En-tête de la carte » : script principal et
scripts spatiaux (hexagone, élévation, rayon).
Fichier : **Nouvelle carte** (Ctrl+N), **Fermer** (Ctrl+W), **miniatures** de toutes les cartes (Ctrl+B, générées en
arrière-plan), **Enregistrer et jouer** (F5 : écrit la carte puis lance le jeu dessus via `StartingMap` de `ddraw.ini`,
copie dans `ddraw.ini.fomapper.bak`, menu « Restaurer ddraw.ini »). Édition : **Cartes du jeu (maps.txt)** et
**Carte du monde** (zones et entrées de `city.txt` sur le fond de `worldmap.txt`, marqueurs déplaçables) ; ces fichiers
sont lus et réécrits sans perte, dans `data/` (les archives ne sont jamais modifiées).
**Ctrl+S** écrit dans `data/maps/` (les `.dat` ne sont jamais modifiés ; l'original est copié une fois en `.map.bak`).

Menu **Fichier → Installations et profils de test** : créez un profil par mod (Fallout 2 RPU, Fallout 1 in 2,
Nevada, Sonora, Olympus 2207, etc.), avec un dossier de mapping et, facultativement, un dossier de test séparé.
Le mapping lit et écrit dans son installation. **Enregistrer et jouer** copie la carte courante et son `.edg` vers le
dossier de test avant de lancer le jeu qui s'y trouve ; le dossier de mapping reste séparé. Sans dossier de test,
le jeu se lance dans le dossier de mapping comme auparavant.

Pour Fallout2-ce Fusion, activez son canal de contrôle dans le `fallout2.cfg` du dossier de test (`[debug]`,
`control_port=47720`, et éventuellement `control_token=...`). La commande **Fichier → Recharger dans Fallout Fusion**
enregistre et synchronise la carte, puis la charge dans l'instance déjà lancée sans la redémarrer. Le jeu doit être
sur l'écran de la carte ; sinon Fusion renvoie une erreur. La commande utilise le jeton configuré et affiche l'état
ou l'erreur de connexion dans le mappeur.

Depuis la carte du monde, un double-clic sur un lieu (marqueur ou liste de gauche), ou le bouton **Ouvrir la carte locale** de sa fiche, ouvre sa carte dans un onglet. Si plusieurs cartes sont associées au lieu, une fenêtre permet de choisir laquelle ouvrir. Les entrées cachées au début du jeu restent accessibles dans l'éditeur.

### Construction et scripts

Les transparences rouge, mur, verre, vapeur et énergie utilisent les niveaux de gris, les teintes et les tables RGB555/intensité de `color.pal`, selon le calcul de [Fallout 2 CE](https://github.com/fallout2-ce/fallout2-ce/blob/main/src/object.cc). Le même calcul sert au canevas, à la vue côte à côte et aux exports. L'éclairage ambiant de l'aperçu reste une simulation.

Le choix **Mobilier assorti** sélectionne un ensemble observé dans les cartes selon la graine, également pour une pièce seule. Les associations ne traversent pas les cloisons ; les objets masqués, les objets ramassables hors conteneurs et les luminaires sont exclus du mobilier appris. Le seuil des portes reste libre.

- **Murs** : après l'analyse des cartes, deux clics tracent une ligne isométrique ou un rectangle. Les raccords et angles utilisent les voisinages observés dans le jeu. Les modes **Porte** et **Fenêtre** remplacent un panneau existant uniquement si son encadrement et ses voisins sont compatibles. Chaque opération s'annule avec Ctrl+Z.
- **Palettes visuelles** : dans les outils Murs et Pièce, déplier les palettes pour voir un bâtiment miniature avec son sol et ses ouvertures. Le sol et le toit associés sont appris dans les cartes originales ; sélectionner une famille manuellement reste possible.
- **Modifier un bâtiment** : sélectionner un mur ou un objet à l'intérieur, puis **Modifier le bâtiment sélectionné**. Régler les dimensions, les pièces et le style, ou choisir la position de la porte sur la carte, puis **Appliquer au bâtiment**. Le contour et les cloisons sont reconstruits ; les objets intérieurs gardent leurs propriétés, scripts et inventaires. Une réduction les replace à l'intérieur. Les murs scriptés sont protégés, et les portes scriptées sont conservées lorsqu'il reste assez de portes dans le nouveau plan. L'opération entière est annulable.
- **Édition → Liaisons entre cartes** : sorties et entrées de la carte active, navigation vers les cartes concernées, contrôle des destinations et présence d'un retour. **Créer le retour** permet de choisir les deux hexagones avant d'ouvrir et modifier la destination. Le retour est une modification en mémoire, annulable, à enregistrer avec Ctrl+S. Les numéros utilisés proviennent de `maps.txt` ; les arrivées bloquées ou posées sur une autre sortie sont refusées.
- **Scripts → Éditer la source SSL** : ouvrir une source `.ssl`, en créer une, enregistrer et compiler. La pseudo-source du désassembleur reste une vue distincte. Le compilateur sfall est recherché dans les dossiers `tools/sslc`, `tools/sslc-sfall5`, dans le projet voisin `FO2glscripts`, ou via `FO_SSL_COMPILER` ; son chemin et les dossiers d'inclusion sont modifiables. Le vrai compilateur s'exécute en arrière-plan sans console. Un INT n'est installé dans `data/scripts` que si la compilation et sa relecture réussissent. Les diagnostics pointent vers la ligne source ; des brouillons sont conservés dans `%LOCALAPPDATA%/FalloutMapper/script-drafts`. Un nouveau script peut être ajouté à la fin de `scripts.lst`, sans décaler les indices existants. La compilation et l'enregistrement des sources gardent une copie `.bak` des fichiers remplacés. Les réglages SSL défilent indépendamment du code, qui reste visible en petite fenêtre ; la liste des scripts se redimensionne en largeur.

Référence du compilateur : <https://github.com/sfall-team/sslc>.

- **Script spatial** : outil de la barre d'outils. On choisit un script et un rayon, un cercle suit le curseur, un clic pose le script (annulable) ; les scripts déjà posés montrent leur contour.
- **Remplir d'après une carte du jeu** : dans la palette des sols et des toits, « Synthèse d'après une carte du jeu » recompose la sélection par blocs recopiés d'une carte modèle (`fo-core/src/quilt.rs`), avec en option des décors semés comme dans cette carte. « Autre tirage » défait et refait avec une autre graine.
- **Bâtiments du jeu** : outil Tampon → « Bâtiments du jeu » : les constructions réelles repérées dans toutes les cartes (murs, décors, sol, toit), à poser telles quelles (`fo-core/src/buildings.rs`). Demande l'analyse de toutes les cartes.
- **Analyse d'une source SSL** : dans l'éditeur de source, panneau « Analyse du script » (procédures, appels, variables globales / de carte / locales, messages, prototypes créés, recherche par variable), sans compiler (`fo-script/src/ssl.rs`).
- **Barre d'outils personnalisable** : clic droit sur la barre du haut pour choisir les outils et leur ordre. **Liste des cartes** : vue dossiers ou liste, tri alphabétique ou dans l'ordre du jeu, croissant ou décroissant. **Journal** : chaque étape d'édition y est notée, avec un bouton qui recentre la vue sur son hexagone.
- **Flèches du clavier** : avec l'outil Sélection, les flèches déplacent l'objet ou le groupe sélectionné d'un hexagone à l'écran (Maj : trois) ; une série de pressions ne fait qu'une étape d'historique.
- **Verrouiller la sélection** (bouton cadenas de la barre d'outils, bouton du panneau de sélection, touche `L` modifiable dans Options) : ni un clic, ni un rectangle, ni Échap, ni Ctrl+A ne changent la sélection ; on ne peut saisir et déplacer que ses objets. Le cadre devient orange ; le verrou tombe quand la sélection est vide.
- **Outil Pièce** : le coin se pose sur le quadrillage des cases de sol (colonne et ligne paires), donc le sol suit les murs et l'aperçu ne change pas quand la souris bouge ; « Autre variante » (ou les boutons précédent/suivant de la souris) change la disposition.

### Outils en ligne de commande

`fomap -g <dossier du jeu> <commande>` (ou `FO2_GAME_DIR`). `--json` donne une sortie structurée pour les scripts.

| Commande | Rôle |
|---|---|
| `analyze <carte>` | objets par type, décors et sols les plus fréquents, sorties, scripts |
| `graph [--dot]` | liens entre cartes (sorties), texte, JSON ou Graphviz |
| `reach <carte> [-e N]` | sorties et portes atteignables à pied depuis le départ |
| `check <carte>` | contrôle de la carte (comme « Vérifier la carte ») |
| `frm <chemin>` | cadence, directions et images d'un FRM |
| `gvars [filtre]` | variables globales du jeu (`vault13.gam`) |
| `ssl <fichier.ssl> [-I dossier] [-p proc] [--gvar N]` | analyse statique d'une source SSL (ne demande pas le jeu) |

### Réglages de l'utilisateur

Ils sont dans la base de registre, `HKEY_CURRENT_USER\SOFTWARE\FALLOUT\FalloutMapper` (une valeur texte par réglage : préférences, dossiers récents, profils de jeu, calques, barre d'outils, fenêtre…). À la première ouverture, les anciens fichiers (`app.ron`, `window.txt` du dossier de stockage de l'application) sont lus puis recopiés dans le registre. Ailleurs que sous Windows, un fichier `settings.txt` sert de repli. `FO_SETTINGS_KEY` remplace le chemin de la clé (essais). Les copies de récupération (`recovery\`) et les brouillons SSL (`%LOCALAPPDATA%/FalloutMapper/script-drafts`) restent des fichiers.

### Essai automatique de l'interface

`FO_SELFTEST_WRITE_OK=1 FO_SELFTEST=rapport.txt fallout-mapper <copie-jetable-du-dossier-de-jeu> arvillag` (l'essai écrit des cartes dans le dossier : jamais sur une vraie installation) injecte de vrais événements souris/clavier
(glisser-déposer, Ctrl+Z/Y, R, Ctrl+D, Suppr, peinture, Ctrl+S), écrit un rapport OK/ÉCHEC puis ferme l'application.
Utilisez un dossier de test dont `fallout2.cfg` pointe vers l'installation, pour que l'enregistrement n'y touche pas.

### Animation de palette

Les indices 229–254 de la palette (boue, rivage, feux, écrans, alarme) tournent comme dans le jeu
(<https://fallout.wiki/wiki/Pal_animations>), à la vitesse de `cycle_speed_factor` et seulement si `color_cycling=1`
dans `fallout2.cfg`. Menu Affichage → « Animer la palette ». `fomap render --anim-ms 400` fige un instant.

## Pistes suivantes

`for`/`switch` reconstruits ; éditeur de prototypes (`.pro`) ; son d'ambiance ; serveur MCP et autres outils d'analyse de Gecko (quêtes, fins du jeu, groupe, sauvegardes, rencontres) ; voir `TODO.md`.
