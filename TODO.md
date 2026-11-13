# FalloutMapper — état du projet et reste à faire

Éditeur de cartes Fallout 2, écrit en Rust à 100 %, interface en français (anglais disponible). Ce fichier sert à reprendre
le travail : où en est le projet, comment le vérifier, ce qu'il reste à faire. Le `README.md` décrit l'utilisation.

## Règles à respecter

- **Ne jamais modifier les archives `.dat`.** Les enregistrements vont dans `<dossier du jeu>/data/…` (cartes, `maps.txt`,
  `city.txt`, `.edg`, motifs) avec une copie `.bak`.
- **Ne jamais lancer de test qui écrit sur une vraie installation.** Le self-test d'interface (`FO_SELFTEST`) enregistre des
  cartes, `maps.txt` et des motifs dans le dossier du jeu : il faut une **copie jetable** du dossier et
  `FO_SELFTEST_WRITE_OK=1` (l'application refuse sinon). Il a déjà fallu nettoyer une installation à cause de cela.
- Ne pas lancer le jeu (F5 / `ddraw.ini`) depuis les essais. `ddraw.ini` n'est modifié qu'après confirmation, avec la copie
  `ddraw.ini.fomapper.bak`.
- Pas de fichiers inutiles dans le projet (scripts temporaires, captures) : tout ce qui sert à un essai va dans `%TEMP%` et
  se supprime après.

## Données de test

- Jeu décompressé (dossiers `master.dat/`, `critter.dat/`) : `C:\Fallout2\F2_RPU_2.4.34_old` — **lecture seule** pour les essais.
- Jeu compressé : `C:\Fallout2\F2_RPU_2.4.34`.
- Fallout 1 (DAT1, cartes v19) : `D:\Torrent\Down\Fallout Setup\Fallout1-extract` — **lecture seule** (`FO1_GAME_DIR` pour les tests).
- Références : `D:\Git\Fallout\FalloutImageBrowser`, `D:\Git\Fallout\DatExplorerIII`, Gecko
  (https://github.com/JanSimek/gecko), Mapper d'origine (https://github.com/FalloutTeamX/F2_Mapper_Dims/tree/master/Mapper).

## Organisation

| Crate | Rôle |
|---|---|
| `fo-core` | `room` (enchaînements de pièces de mur, plan d'une pièce ou d'un bâtiment, types de pièce), `check` (vérificateur de carte et corrections), `msg` (lecture et réécriture chirurgicale des `.msg`), `diff` (comparaison de deux cartes), `map::find` (recherche et remplacement d'objets par prototype). Formats : DAT2, FRM, PAL, MAP v20 (lecture/écriture/édition), PRO, LST, MSG, `maps.txt`, `city.txt`/`worldmap.txt` (modèle INI sans perte), `.edg`, motifs (JSON compatible Gecko), géométrie hexagonale (`geom`), animations de palette, `tile_stats` (voisinage des tuiles, familles), `stretch` (étirement d'un groupe d'objets). Tests d'intégration sur les vraies données : `tests/real_data.rs`. |
| `fo-scene` | Assemble une carte en scène dessinable (ordre de dessin, calques, rendu logiciel). |
| `fo-script` | Lecture des scripts `.int` : désassemblage, pseudo-source structuré, noms de variables, textes `.msg`. |
| `fo-cli` | `fomap list|info|script|render|ssl|analyze|graph|reach|check|frm|gvars` (`src/tools.rs` : les outils d'analyse, texte ou `--json`). |
| `fo-mapper` | L'application (eframe/egui 0.37). |

Modules de `fo-mapper/src` : `app.rs` (structure `MapperApp`, chargement, boucle d'affichage, enregistrement des réglages) et ses
parties : `menu_ui.rs` (barre de menus), `map_list_ui.rs` (liste des cartes : dossiers ou liste, tri), `inspector_ui.rs` (panneau de droite,
détails d'un objet), `canvas_ui.rs` (dessin de la carte, grille, décors animés de l'aperçu) ; `edit_ui.rs` (outils, état d'édition) et ses
parties : `edit_history.rs` (modifier, annuler/rétablir, fenêtre Historique), `edit_file.rs` (enregistrer, confirmations), `edit_input.rs`
(raccourcis, souris sur la carte, peinture), `edit_overlay.rs` (aperçus sous le curseur), `edit_panel_ui.rs` (panneau d'édition, sélection),
`palette_ui.rs` (palettes de tuiles et de prototypes) ; `settings.rs` (réglages dans le registre), `spatial_ui.rs` (outil Script spatial),
`fill_ui.rs` (synthèse de sols d'après une carte du jeu), `tabs.rs` (cartes en onglets), `split.rs` (vue côte à côte), `select_ops.rs` (sélection, rectangle isométrique), `extras.rs` (copier/coller,
menu contextuel, légende, étirement), `analysis_ui.rs` (analyse des tuiles), `exit_ui.rs`, `stamp_ui.rs`, `object_ui.rs`,
`file_ui.rs` (nouvelle carte et gabarits, miniatures, jouer), `room_ui.rs` (outil « Pièce » : pièces et bâtiments), `preview.rs` (aperçu du jeu : lumière par hexagone, toits masqués, cadre 640 × 480), `find_ui.rs` (Rechercher un objet), `check_ui.rs` (Vérifier la carte, avec corrections), `msg_ui.rs` (éditeur de textes `.msg`), carte du monde : lieux aléatoires tirés selon le terrain (`worldmap::roll_position`), `diff_ui.rs` (Comparer avec une version), `recovery.rs` (copies de récupération toutes les 2 min, restauration au lancement suivant), `export_ui.rs` (PNG de la carte ou de l'écran du jeu, `fo_scene::render_shaded`), `fo-core/src/reach.rs` + `walk_ui.rs` (hexagones bloqués et atteignables, problème « Inaccessible depuis le départ » du vérificateur), `library_ui.rs` (bibliothèque de motifs : miniatures, étiquettes, glisser-déposer), `minimap.rs` (minicarte), `fo-core/src/procgen.rs` (plan d'un village : zone, sorties, routes, bâtiments, semis ; graine reproductible), `fo-core/src/village.rs` (génération complète d'après ce plan, palette de décor apprise d'une carte), `fo-core/src/build.rs` (aides de pose partagées avec l'outil Pièce), `procgen_ui.rs` (fenêtre « Générer une carte »), `i18n_es/it/de/pt/ru.rs` (tables de traduction ; absent : anglais puis français), `file_dialog.rs` (dialogues maison d'ouverture du dossier du jeu et d'« Enregistrer sous », dossiers récents ; plus aucune dépendance `rfd`), `world_ui.rs` (`maps.txt` et onglet carte du monde),
`scripts.rs` + `highlight.rs` (onglet scripts), `toolbar.rs`, `icons.rs` (icônes dessinées), `ui_kit.rs` (thème, pied de
fenêtre), `stepper.rs` (champ numérique − / +), `log_ui.rs` (journal des messages d'état), `prefs.rs`, `i18n.rs`, `selftest.rs`, `shot.rs`.

Points d'architecture à connaître :
- **Onglets** : l'état de la carte *active* vit dans les champs de `MapperApp` ; les autres cartes sont rangées dans `docs`
  (`tabs::Doc`). Changer d'onglet échange les deux (`switch_tab`). `docs[active_tab]` n'est qu'un emplacement vide.
- **Géométrie** : le centre d'un hexagone vient de la formule du Mapper d'origine (`geom::hex_to_screen`). Les pas du réseau
  sont (32, 0), (16, 12) et (−16, 12) ; `stretch` et le rectangle de sélection isométrique (`select_ops::IsoRect`) utilisent les
  axes (16, 12) et (−16, 12).
- **Textes** : écrits en français dans le code, traduits par `i18n::t("texte français")`. Un texte sans entrée dans le tableau
  `EN` reste en français.
- **Historique** : `EditState` garde jusqu'à 50 instantanés de la carte (`undo`/`redo`) avec un libellé chacun (`&'static str`, traduit à l'affichage). `mutate_as("libellé", …)` nomme l'étape ; `mutate(Some(tag), …)` fusionne les modifications répétées. Tout nouveau libellé va dans le tableau `EN` de `i18n.rs`.

## Vérifier

```
cargo build -p fo-mapper
cargo test --workspace          # doit passer entièrement si le dossier du jeu n'a pas été pollué
cargo clippy --workspace --all-targets
```

Captures d'écran en lecture seule (aucune écriture dans le jeu) : `FO_SHOT=<fichier.png>` puis le dossier du jeu et le nom
d'une carte. Variantes : `FO_EDIT=1`, `FO_TOOL=floor|roof|place`, `FO_SHOT_MODE=scripts|world`,
`FO_SHOT_DIALOG=prefs|confirm|new`, `FO_SHOT_OPEN2=<carte>` (second onglet), `FO_SHOT_SPLIT=1`, `FO_SHOT_GRID=1`,
`FO_SHOT_ANALYZE=1`, `FO_SHOT_STRETCH=1`, `FO_SHOT_LANG=en`, `FO_SHOT_THEME=light`.
Génération et export : `FO_SHOT_GEN=map` (+ `FO_SHOT_ANALYZE=1`, `FO_SHOT_GEN_SEED`, `FO_SHOT_GEN_BUILDINGS`, `FO_SHOT_GEN_STYLE`, `FO_SHOT_NOROOF=1`),
`FO_SHOT_EXPORT=<png>` (+ `FO_SHOT_EXPORT_MODE`, `FO_SHOT_EXPORT_SCALE=1` pour la taille réelle) ; pièces : `FO_SHOT_ROOM=<style>`,
`FO_SHOT_ROOM_SIZE=12x12`, `FO_SHOT_BUILD=2x2` ; synthèse de sol : `FO_SHOT_QUILT=<carte modèle>` (+ `FO_SHOT_QUILT_DECOR=1`) ; outil : `FO_TOOL=spatial`.
`FO_SETTINGS_KEY` redirige les réglages vers une autre clé du registre.

Self-test d'interface (**sur un dossier jetable uniquement**, jamais sur l'installation) : créer un dossier dont le
`fallout2.cfg` pointe vers l'installation en lecture (`master_dat=` et `critter_dat=` : chemins absolus ; `critter_patches=`
vers le `data` de l'installation ; `master_patches=data` relatif, **et créer ce dossier `data` d'avance**, sinon le moteur ne
le lit pas comme couche de correctifs) puis lancer
`FO_HIDDEN=1 FO_SELFTEST_WRITE_OK=1 FO_SELFTEST=rapport.txt fallout-mapper <dossier-jetable> arvillag`. `FO_HIDDEN=1` place la
fenêtre hors de l'écran, sans focus ni icône de barre des tâches (valable aussi pour `FO_SHOT`). En mode test (`FO_SHOT`,
`FO_SELFTEST`, `FO_HIDDEN`) rien n'est écrit dans le stockage de l'application (position de la fenêtre, préférences, dernier
dossier). Dernier passage : 43 tests sur 43 réussis (15 s) ; le test reste sensible à la charge de la machine (il compte des
images) : sous forte charge, relancer. `FO_SHOT_CTRLS=1` (avec `FO_SELFTEST_WRITE_OK=1`) ajoute un objet puis injecte un vrai
Ctrl+S.

## Fallout 1

Lecture/édition des installations Fallout 1 : archives DAT1 (index gros-boutiste, LZSS) dans `fo-core/src/dat.rs`, cartes v19 (71 cartes sur 72 se réécrivent à l'octet près ; `childrn2` référence un objet inexistant), `GameData::is_fallout1()` (pas de `data/maps.txt`). F5 est désactivé pour Fallout 1. L'enregistrement d'une carte F1 est vérifié par `fallout1_map_saves_and_reloads_from_a_scratch_root` (dossier temporaire dont `fallout2.cfg` pointe vers les archives : réécriture identique, ajout d'un mur, `.bak`, relecture) ; `GameData::open` lit aussi `fallout.cfg`. Ctrl+S depuis l'interface est vérifié (`FO_SHOT_CTRLS=1` + `FO_SELFTEST_WRITE_OK=1` sur un dossier de jeu jetable : ajoute un objet, injecte un vrai Ctrl+S ; les `.edg` de Fallout 1, format EDGE v1, se réécrivent à l'identique). Vérifiés par tests sur `FO1_GAME_DIR` : tous les prototypes (objets, décors, murs, tuiles, divers : lus, PID et type de FID cohérents, art présent), 952 scripts `.int` (lus et désassemblés, aucun opcode inconnu) et les scripts attachés aux cartes (le `scripts.lst` d'origine a des lignes abîmées, « Brohd12.intried ; … » : `parse_lst` coupe le nom après `.int`). Le format des prototypes de créatures est le même (IA, équipe, caractéristiques plausibles) ; les cartes de Fallout 1 donnent souvent à une créature une IA différente de celle de son prototype. Les noms et l'ordre des cartes viennent de `map.msg` ({n} = fichier de la carte n, {100 + n} = description : « Junktown (junkent) »), faute de `maps.txt` ; les noms d'entrée de `worldmap.msg` ne se rattachent pas aux cartes (la table est codée dans le jeu). **Pas encore vérifié** : onglets monde/`maps.txt` (propres à Fallout 2, sans objet pour Fallout 1), création d'une créature à partir d'un prototype F1 dans l'interface, une poignée de scripts de cartes dont le `.int` manque à l'archive.

## Reste à faire

1. **Pièces : finitions.** L'outil « Pièce » (`fo-core/src/room.rs`, `room_ui.rs`) construit une pièce ou un bâtiment de nx × ny pièces (tailles variables en option, cloisons partagées, portes entre pièces, passage ouvert faute de porte connue, entrée sur la façade), avec un étage en option : même empreinte à l'élévation suivante (créée au besoin, `Map::ensure_elevation`), autre mobilier, escaliers appris des cartes (encodage de destination `élévation << 29 | 3 << 26 | hexagone`, vérifié sur les cartes du jeu). Captures sur `desert1` : deux styles, pièce 12 × 12 (45 murs, 0–2 fenêtres, 1 porte, 3–6 meubles), tailles variées ; bâtiment 2 × 2 avec étage (57 murs par niveau, 2 escaliers, mobilier présent). Le sol `rst` apparaît à l'intérieur et suit l'empreinte des murs ; toit posé sur l'étage supérieur. Les passages restent ouverts quand le style ne fournit pas de porte. La paire d'escaliers cible le même hexagone adjacent sur l'autre étage. Reste à vérifier par interaction dans le jeu la traversée des escaliers et l'orientation (0), puis à comparer le placement du mobilier et l'aspect du toit/fenêtres au moteur original.
2. **Plan Gecko, étape 7 (tables de transparence intégrées)** : les cinq transparences utilisent maintenant les tables de palette/intensité et les niveaux de gris de CE (`palette_blend.rs`), vérifiés sur 5 760 échantillons de référence ; le canevas compose les zones des effets avec un cache sensible aux calques, à la lumière et aux animations. L'aperçu du jeu adoucit maintenant progressivement les murs de premier plan dans une petite zone autour du point de vue ; cette transparence locale reste une approximation à comparer visuellement au moteur. L'éclairage est fait (`light_ui.rs`, bouton soleil de la barre du bas, halos d'après rayon et intensité des objets) ; les raccourcis (ajuster, centrer, tourner, pipette, et ceux à Ctrl : annuler, rétablir, enregistrer, dupliquer, tout sélectionner/désélectionner, nouvelle carte, fermer, miniatures) sont modifiables dans Options (`prefs::KeyAction`) ; copier/couper/coller, Ctrl+1/2/3 et F5 restent fixes. Le panneau de journal est fait (`log_ui.rs`, menu Affichage → Journal). Les préférences sont faites (`prefs.rs`).
3. **Outils en ligne de commande de Gecko** : faits : `analyze`, `graph`, `reach`, `check`, `frm`, `gvars`, `ssl` (voir `README.md`). Restent : quêtes, fins du jeu, groupe, inspection de sauvegarde, rencontres de la carte du monde, export des prototypes, références de variables entre scripts, et un serveur MCP qui les expose à un assistant.
4. **Écarts avec Gecko** (d'après son README, son `PLAN.md` et ses sources) : scripts Luau et remplissages à préréglages (remplacés ici par « Remplir d'après une carte du jeu », natif) ; éditeur de prototypes `.pro` ; son d'ambiance (ACM) ; animations de créatures au repos (seuls les décors à plusieurs images tournent, dans l'aperçu du jeu) ; analyse SSL multi-scripts (références croisées sur tout un corpus) ; panneaux ancrables.
5. **Pièces et bâtiments** : les styles dentelés ou faits de bandes (`black…`) ne se synthétisent pas comme dans le jeu (Gecko fait le même constat pour les murs de grottes) ; copier de vraies constructions (outil Tampon → « Bâtiments du jeu ») est la voie fiable. Les routes générées forment encore des escaliers (cases entières) : il faudrait les tuiles de bord de la famille `road` (la terre de chaque tuile a été mesurée : `diag_road_dirt` dans `fo-core/tests/real_data.rs`).

## À améliorer

- Traduction anglaise : terminée pour les textes affichés par l'application (`i18n::t`, `tr`, `quote`). Restent en français : messages venus de `fo-core`/`fo-script`, noms de motifs générés, et un message d'état déjà affiché ne change pas de langue après coup.
- Vue côte à côte : la carte de droite est en lecture seule et ne sert qu'à copier des objets vers la carte active ; la rendre
  éditable, autoriser aussi le glisser dans l'autre sens.
- Étirement : sols et toits ne sont pas étirés.
- Gabarits de nouvelle carte (désert, côte, ville, intérieur) : les familles de tuiles sont choisies par préfixe de nom, sans
  vérification visuelle du rendu.

Éditeur SSL : captures vérifiées avec source chargée, coloration, réglages dépliés et fenêtre de 800 × 600. Les réglages défilent séparément du code ; le chargement d'une autre source efface l'analyse précédente. Deux tests de régression couvrent ces points.

## À vérifier à la main (jamais vu à l'écran)

- Glisser-déposer d'objets entre deux cartes en vue côte à côte.
- Maj + clic sur un objet (ajoute le rectangle isométrique entre l'objet principal et l'objet cliqué ; `select_ops.rs`).
- Étirement sur un vrai bâtiment (essayé seulement sur une ligne de murs).
- Remplissage plausible et remplacement de familles de tuiles.
- Fenêtre Journal (Affichage → Journal…).
- Outil « Pièce » : captures vérifiées sur deux styles, une pièce 12 × 12 avec tailles variées, et un bâtiment 2 × 2 avec étage. Restent à manipuler à la souris : clic/annulation (Ctrl+Z) et génération au bord de carte. La silhouette translucide supplémentaire des captures est l'aperçu de placement sous le curseur. Aperçu du jeu : capture `FO_SHOT_PREVIEW=jour|soir|nuit` (+ `FO_SHOT_FRAME=1`, `FO_SHOT_VIEWHEX=<hex>`). Capture : `FO_SHOT_ROOM=<style|#n|vide>`.
- Génération de carte : le rendu vient d'une carte de 120 × 120 hexagones vue à l'écran (bâtiments, routes, arbres, lumières) ; à juger à l'œil : toit (famille la plus courante des cartes, parfois peu adaptée), dessin des routes, densité du décor, direction des grilles de sortie (posées par côté, destination : carte du monde). Seul un village extérieur est généré (pas d'intérieur ni de caverne). Capture : `FO_SHOT_ANALYZE=1 FO_SHOT_GEN=map|window` (+ `FO_SHOT_GEN_ZOOM`). Langues es/it/de/pt/ru : traduites hors ligne à partir du français et de l'anglais, jamais relues par un locuteur ; les textes nouveaux retombent sur l'anglais tant qu'ils ne sont pas ajoutés aux tables. Sélection : modes Hexagones et Bloqueurs vérifiés par un test et une capture, pas à la souris.
- Glisser-déposer d'une miniature de la bibliothèque sur la carte (non joué en vrai : seul le dessin des miniatures et la pose `stamp_at` sont vérifiés). Accessibilité : portes traitées comme passables, objets multi-hexagones et créatures ignorés pour l'accès ; seule l'élévation du départ est examinée. Captures : `FO_SHOT_EXPORT=<png>` (+ `FO_SHOT_EXPORT_MODE=game|whole|whole_lit`), `FO_SHOT_WALK=1`, `FO_SHOT_LIBRARY=1`, `FO_SHOT_RECOVERY=write|restore` (avec `FO_RECOVERY_DIR`). L'essai automatique tombe parfois en échec sur « le tracé d'un bord » quand la copie jetable garde les fichiers d'un essai précédent.
- Dialogues maison (Fichier → Ouvrir le dossier du jeu…, Fichier → Enregistrer sous) : navigation, récents, saisie du chemin.
- F5 (jouer la carte) et sa fenêtre de confirmation ; dialogues `maps.txt` ; enregistrement de `city.txt` depuis l'onglet carte du monde.
