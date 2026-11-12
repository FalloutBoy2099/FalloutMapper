//! Langues de l'interface (français par défaut, anglais). Les textes sont écrits en français dans le code ; `t` renvoie leur
//! traduction quand la langue courante est l'anglais, et le texte d'origine s'il n'y en a pas.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Lang {
    #[default]
    Fr,
    En,
    Es,
    It,
    De,
    Pt,
    Ru,
}

impl Lang {
    pub(crate) const ALL: [Lang; 7] = [
        Lang::Fr,
        Lang::En,
        Lang::Es,
        Lang::It,
        Lang::De,
        Lang::Pt,
        Lang::Ru,
    ];

    pub(crate) fn code(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
            Lang::Es => "es",
            Lang::It => "it",
            Lang::De => "de",
            Lang::Pt => "pt",
            Lang::Ru => "ru",
        }
    }

    pub(crate) fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.code() == code)
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Lang::Fr => "Français",
            Lang::En => "English",
            Lang::Es => "Español",
            Lang::It => "Italiano",
            Lang::De => "Deutsch",
            Lang::Pt => "Português",
            Lang::Ru => "Русский",
        }
    }

    fn from_index(i: u8) -> Lang {
        Self::ALL.get(i as usize).copied().unwrap_or(Lang::Fr)
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub(crate) fn set_lang(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub(crate) fn lang() -> Lang {
    Lang::from_index(CURRENT.load(Ordering::Relaxed))
}

const EN: &[(&str, &str)] = &[
    ("Construction de murs", "Build walls"),
    ("Modification d'un bâtiment", "Modify building"),
    ("Création d'une liaison retour", "Create return link"),
    ("Style de bâtiment", "Building style"),
    ("Palettes visuelles", "Visual palettes"),
    ("Toit assorti aux murs", "Match roof to walls"),
    (
        "Modifier le bâtiment sélectionné",
        "Modify selected building",
    ),
    ("Appliquer au bâtiment", "Apply to building"),
    ("Liaisons entre cartes", "Map links"),
    ("Liaisons entre cartes…", "Map links…"),
    ("Actualiser les liaisons", "Refresh links"),
    ("Ouvrir la source", "Open source map"),
    ("Ouvrir la destination", "Open destination map"),
    ("Créer le retour", "Create return link"),
    ("Retour présent", "Return link exists"),
    ("Retour absent", "No return link"),
    ("Éditer la source SSL", "Edit SSL source"),
    ("Script compilé", "Compiled script"),
    ("Nouvelle source", "New source"),
    ("Enregistrer SSL", "Save SSL"),
    ("Restaurer le brouillon", "Restore draft"),
    ("Compiler et installer INT", "Compile and install INT"),
    (
        "Compilateur et inclusions",
        "Compiler and include directories",
    ),
    ("Nom du script compilé", "Compiled script name"),
    ("Ouvrir une carte du lieu", "Open a location map"),
    (
        "Étirer la sélection (pièce)",
        "Stretch the selection (room)",
    ),
    (
        "Répète la bande centrale du groupe sur chaque axe de l'écran ; les bords gardent leur forme.",
        "Repeats the middle band of the group along each screen axis; the edges keep their shape.",
    ),
    ("Étirer", "Stretch"),
    (
        "Étirement impossible : un objet de la sélection n'est pas sur la carte.",
        "Cannot stretch: an object of the selection is not on the map.",
    ),
    ("objets après étirement", "objects after stretching"),
    ("Analyse des tuiles…", "Analysing tiles…"),
    ("Analyse terminée", "Analysis done"),
    ("cartes", "maps"),
    ("sols différents", "distinct floors"),
    ("Analyse et génération", "Analysis and generation"),
    (
        "Relancer l'analyse de toutes les cartes",
        "Re-run the analysis of all maps",
    ),
    (
        "Analyser les tuiles de toutes les cartes",
        "Analyse the tiles of all maps",
    ),
    (
        "Suggérer les voisines de la tuile choisie en premier",
        "Suggest neighbours of the chosen tile first",
    ),
    ("Famille", "Family"),
    ("Toutes", "All"),
    ("Celle du pinceau", "Brush's"),
    ("Cases sélectionnées", "Selected squares"),
    (
        "Remplir la sélection de façon plausible",
        "Fill the selection plausibly",
    ),
    (
        "Pose des tuiles qui vont avec leurs voisines, d'après toutes les cartes analysées",
        "Places tiles that fit their neighbours, based on all analysed maps",
    ),
    ("case(s) remplie(s)", "square(s) filled"),
    ("Remplacer la famille", "Replace family"),
    ("… dans la sélection", "… in the selection"),
    ("… dans toute l'élévation", "… in the whole elevation"),
    (
        "Remplace les tuiles de la famille du pinceau par leur équivalent (même rang) dans la famille d'arrivée",
        "Replaces tiles of the brush's family with their equivalent (same rank) in the target family",
    ),
    ("tuile(s) remplacée(s)", "tile(s) replaced"),
    (
        "Afficher à droite (côte à côte)",
        "Show on the right (side by side)",
    ),
    ("Vue unique", "Single view"),
    ("Copier ici", "Copy here"),
    (
        "glisser un objet sélectionné vers l'autre carte pour le copier",
        "drag a selected object onto the other map to copy it",
    ),
    (
        "objet(s) copié(s) depuis l'autre carte",
        "object(s) copied from the other map",
    ),
    ("Fermer l'onglet", "Close tab"),
    ("Noms", "Names"),
    ("Fermer les autres onglets", "Close other tabs"),
    ("Fermer tous les onglets", "Close all tabs"),
    ("Historique…", "History…"),
    (
        "Surligner les objets avec un script",
        "Highlight objects with a script",
    ),
    ("Légende des repères…", "Marker legend…"),
    ("Historique", "History"),
    ("État initial", "Initial state"),
    (
        "Analysez d'abord toutes les cartes et choisissez un style de murs.",
        "First analyse all maps and choose a wall style.",
    ),
    (
        "Choisissez une famille de sol pour la carte.",
        "Choose a ground family for the map.",
    ),
    ("Carte générée", "Map generated"),
    ("bâtiment(s)", "building(s)"),
    ("sortie(s)", "exit(s)"),
    ("décor(s)", "decor item(s)"),
    ("meuble(s)", "furniture"),
    (
        "attention : des portes ou sorties restent inaccessibles",
        "warning: some doors or exits remain unreachable",
    ),
    ("Générer une carte", "Generate a map"),
    ("Générer une carte…", "Generate a map…"),
    (
        "Le style des murs et du mobilier vient de l'analyse de toutes les cartes du jeu.",
        "Wall and furniture styles come from the analysis of all game maps.",
    ),
    ("Analyser les cartes", "Analyse the maps"),
    ("Nom de la carte", "Map name"),
    ("Graine", "Seed"),
    ("Relancer", "Reroll"),
    (
        "Tire une autre graine et régénère la carte",
        "Picks another seed and regenerates the map",
    ),
    ("Taille (hexagones)", "Size (hexes)"),
    ("largeur", "width"),
    ("hauteur", "height"),
    ("Bâtiments", "Buildings"),
    (
        "Pièces par bâtiment (au plus)",
        "Rooms per building (at most)",
    ),
    ("Taille d'une pièce", "Room size"),
    ("longueur", "length"),
    ("profondeur", "depth"),
    ("Espace entre bâtiments", "Space between buildings"),
    ("Sorties vers la carte du monde", "Exits to the world map"),
    ("Ouest", "West"),
    ("Est", "East"),
    ("Nord", "North"),
    ("Sud", "South"),
    ("Largeur des routes", "Road width"),
    ("Végétation", "Vegetation"),
    ("Menus objets", "Small props"),
    ("Bordure", "Border"),
    (
        "Bande de végétation dense au bord",
        "Dense vegetation band along the edge",
    ),
    ("Décor appris de la carte", "Decor learned from map"),
    ("Sol de la carte", "Ground"),
    ("Sol des routes", "Road surface"),
    ("Style des murs", "Wall style"),
    ("Sol des bâtiments", "Building floor"),
    ("Toit", "Roof"),
    ("Fenêtres et mobilier", "Windows and furniture"),
    ("fenêtres", "windows"),
    ("meubles", "furniture"),
    ("Lumières dans les bâtiments", "Lights inside buildings"),
    (
        "porte(s) reliée(s) à la route",
        "door(s) linked to the road",
    ),
    ("bloqueur(s)", "blocker(s)"),
    (
        "Nom de fichier : 1 à 8 caractères a-z, 0-9 ou _.",
        "File name: 1 to 8 characters a-z, 0-9 or _.",
    ),
    ("Générer", "Generate"),
    ("Éléments", "Elements"),
    ("Hexagones", "Hexes"),
    (
        "Mode de sélection : « Hexagones » et « Bloqueurs de défilement » ne désignent que ça.",
        "Selection mode: \"Hexes\" and \"Scroll blockers\" only pick those.",
    ),
    ("hexagone(s) sélectionné(s)", "hex(es) selected"),
    (
        "Supprimer les objets sur ces hexagones",
        "Delete the objects on these hexes",
    ),
    ("bloqueur(s) sélectionné(s)", "blocker(s) selected"),
    ("Minicarte", "Minimap"),
    (
        "Minicarte : cliquez ou glissez pour déplacer la vue",
        "Minimap: click or drag to move the view",
    ),
    ("Bibliothèque de motifs…", "Pattern library…"),
    ("Bibliothèque de motifs", "Pattern library"),
    ("Filtrer par nom ou étiquette…", "Filter by name or tag…"),
    (
        "Aucun motif. Capturez une sélection avec l'outil Tampon, ou gardez une pièce construite.",
        "No patterns. Capture a selection with the Stamp tool, or keep a built room.",
    ),
    (
        "Glissez sur la carte pour poser le motif",
        "Drag onto the map to place the pattern",
    ),
    ("Étiquettes", "Tags"),
    ("nouvelle étiquette", "new tag"),
    (
        "Activez le mode édition (menu Édition) pour poser un motif.",
        "Turn on edit mode (Edit menu) to place a pattern.",
    ),
    ("maison", "house"),
    ("entrepôt", "warehouse"),
    ("bureau", "office"),
    ("mur", "wall"),
    ("décor", "decor"),
    ("végétation", "vegetation"),
    (
        "Accessibilité (hexagones bloqués et atteignables)",
        "Accessibility (blocked and reachable hexes)",
    ),
    (
        "Rouge : bloqué ; orange : créature ; vert : atteignable à pied depuis le départ du joueur ; bleu : libre mais hors d'atteinte.",
        "Red: blocked; orange: creature; green: reachable on foot from the player start; blue: free but out of reach.",
    ),
    (
        "Inaccessible depuis le départ",
        "Unreachable from the start",
    ),
    ("sortie vers la carte n°", "exit to map no."),
    ("porte", "door"),
    ("escalier ou échelle", "stairs or ladder"),
    (
        "Pas de point de départ du joueur sur cette élévation.",
        "No player start on this elevation.",
    ),
    (
        "Rien à exporter : la carte est vide.",
        "Nothing to export: the map is empty.",
    ),
    ("Aucune carte ouverte.", "No map open."),
    ("Image exportée :", "Image exported:"),
    ("Échec de l'export", "Export failed"),
    ("Exporter une image", "Export an image"),
    ("Exporter une image…", "Export an image…"),
    ("Carte entière", "Whole map"),
    (
        "Écran du jeu (640 × 480 autour du point de départ du joueur)",
        "Game screen (640 × 480 around the player start)",
    ),
    ("Appliquer l'éclairage", "Apply lighting"),
    (
        "Les calques affichés sont exportés ; les repères d'édition ne le sont pas.",
        "Visible layers are exported; editing markers are not.",
    ),
    ("Choisir le fichier…", "Choose the file…"),
    ("Cartes à récupérer", "Maps to recover"),
    (
        "L'éditeur s'est arrêté avant d'enregistrer ces cartes. Les copies de récupération ci-dessous existent encore.",
        "The editor stopped before these maps were saved. The recovery copies below still exist.",
    ),
    ("Restaurer", "Restore"),
    ("Tout supprimer", "Delete all"),
    ("à l'instant", "just now"),
    ("il y a", "ago"),
    ("Restauration de", "Restoring"),
    (
        "Carte restaurée depuis la copie de récupération : enregistrez-la pour la garder.",
        "Map restored from the recovery copy: save it to keep it.",
    ),
    (
        "Relancer tous les lieux aléatoires",
        "Re-roll all random places",
    ),
    (
        "Les lieux dont le jeu choisit la position (rencontres spéciales, caravanes…) sont posés au hasard sur le bon terrain ; city.txt n'est pas modifié.",
        "Places whose position the game picks (special encounters, caravans…) are placed at random on the right terrain; city.txt is not modified.",
    ),
    ("Lieu aléatoire", "Random place"),
    ("terrain", "terrain"),
    ("désert", "desert"),
    ("montagne", "mountain"),
    ("côte", "coast"),
    ("terre", "land"),
    ("Position tirée", "Rolled position"),
    ("Relancer l'emplacement", "Re-roll the location"),
    ("Grille", "Grid"),
    (
        "Grille hexagonale (Affichage → Grille hexagonale)",
        "Hexagonal grid (View → Hexagonal grid)",
    ),
    ("Couleur", "Color"),
    ("Opacité", "Opacity"),
    ("Nouvelle carte…", "New map…"),
    ("Cartes en miniatures", "Map thumbnails"),
    ("Enregistrer et jouer", "Save and play"),
    ("Raccourci", "Shortcut"),
    ("Calques", "Layers"),
    ("Aperçu du jeu", "Game preview"),
    (
        "Aperçu du jeu : lumière, toits masqués, sans repères (réglages dans Affichage)",
        "Game preview: lighting, roofs hidden, no markers (settings in View)",
    ),
    ("Jour", "Day"),
    ("Soir", "Evening"),
    ("Nuit", "Night"),
    ("Lumière ambiante", "Ambient light"),
    (
        "Masquer les toits autour du point de vue",
        "Hide roofs around the viewpoint",
    ),
    ("Rayon sans toit (hexagones)", "Roof-free radius (hexes)"),
    (
        "Point de vue : le curseur (sinon le départ du joueur)",
        "Viewpoint: the cursor (otherwise the player start)",
    ),
    (
        "Cadre de l'écran du jeu (640 × 480)",
        "Game screen frame (640 × 480)",
    ),
    ("Tailles de pièce variées", "Varied room sizes"),
    ("Ajouter un étage (escaliers)", "Add a floor (stairs)"),
    (
        "Construit le même bâtiment à l'élévation suivante, relié par des escaliers appris des cartes.",
        "Builds the same building on the next elevation, linked by stairs learned from the maps.",
    ),
    ("escalier(s)", "stair(s)"),
    ("pièce(s) de mur à l'étage", "wall piece(s) upstairs"),
    (
        "pas de place pour des escaliers : étage non créé",
        "no room for stairs: upper floor not created",
    ),
    (
        "Génération d'un bâtiment à étage",
        "Two-floor building generation",
    ),
    ("Textes (.msg)…", "Texts (.msg)…"),
    ("Textes (.msg)", "Texts (.msg)"),
    ("Fichier introuvable :", "File not found:"),
    ("Textes enregistrés", "Texts saved"),
    (
        "Choisissez un fichier de messages.",
        "Choose a message file.",
    ),
    (
        "Chercher un numéro ou un texte…",
        "Search a number or a text…",
    ),
    ("Annuler les modifications", "Discard changes"),
    ("audio", "audio"),
    ("Supprimer l'entrée", "Delete the entry"),
    ("Nouvelle entrée n°", "New entry no."),
    ("Ajouter", "Add"),
    ("Ce numéro existe déjà.", "This number already exists."),
    ("entrée(s)", "entry(ies)"),
    (
        "Enregistrez ou annulez d'abord les modifications de ce fichier.",
        "Save or discard this file's changes first.",
    ),
    ("Tout corriger", "Fix all"),
    (
        "Supprime les objets fautifs, détache les scripts introuvables et replace le départ. Annulable (Ctrl+Z).",
        "Removes faulty objects, detaches missing scripts and moves the start point. Undoable (Ctrl+Z).",
    ),
    ("Supprimer l'objet", "Remove the object"),
    ("Détacher le script", "Detach the script"),
    ("Replacer le départ", "Move the start point"),
    ("Corrigé :", "Fixed:"),
    ("objet(s) supprimé(s)", "object(s) removed"),
    ("script(s) détaché(s)", "script(s) detached"),
    ("départ replacé", "start point moved"),
    ("Correction du vérificateur", "Checker fix"),
    ("Pièces en largeur", "Rooms wide"),
    ("Pièces en profondeur", "Rooms deep"),
    (
        "Plusieurs pièces qui se partagent leurs cloisons, reliées par des portes ; l'entrée est sur la façade.",
        "Several rooms sharing their partitions, linked by doors; the entrance is on the front.",
    ),
    ("nom du motif", "pattern name"),
    (
        "Garder la dernière construction comme motif",
        "Keep the last build as a pattern",
    ),
    ("bâtiment", "building"),
    ("Bâtiment généré :", "Building generated:"),
    ("Génération d'un bâtiment", "Building generation"),
    ("Type de pièce", "Room type"),
    ("Mobilier assorti", "Matching furniture"),
    ("Tous les meubles", "All furniture"),
    ("Comparer avec une version…", "Compare with a version…"),
    ("Comparer avec une version", "Compare with a version"),
    (
        "Pas de copie .bak : la carte n'a jamais été enregistrée par l'éditeur.",
        "No .bak copy: the map was never saved by the editor.",
    ),
    ("Aucune différence.", "No difference."),
    ("différence(s)", "difference(s)"),
    ("version enregistrée", "saved version"),
    ("copie .bak", ".bak copy"),
    ("Version enregistrée", "Saved version"),
    ("Copie .bak", ".bak copy"),
    ("Comparer", "Compare"),
    (
        "La version enregistrée est celle du disque : elle ne contient pas vos modifications en cours.",
        "The saved version is the one on disk: it does not contain your current changes.",
    ),
    ("case(s) de sol ou de toit", "floor or roof square(s)"),
    ("Surligner sur la carte", "Highlight on the map"),
    ("autres", "more"),
    ("Ajouté", "Added"),
    ("Retiré", "Removed"),
    ("Déplacé", "Moved"),
    ("Modifié", "Modified"),
    ("Vérifier la carte…", "Check the map…"),
    ("Vérifier la carte", "Check the map"),
    ("Aucun problème trouvé.", "No problem found."),
    ("problème(s) trouvé(s)", "problem(s) found"),
    ("Vérifier de nouveau", "Check again"),
    ("problème(s)", "problem(s)"),
    ("Tous", "All"),
    ("Objet hors de la grille", "Object outside the grid"),
    ("Objet en double", "Duplicate object"),
    ("Prototype introuvable", "Prototype not found"),
    ("Script introuvable", "Script not found"),
    ("Sortie vers une carte inexistante", "Exit to a missing map"),
    ("Image introuvable", "Image not found"),
    ("Point de départ invalide", "Invalid start point"),
    ("Rechercher un objet…", "Find an object…"),
    ("Rechercher un objet", "Find an object"),
    ("Nom du prototype", "Prototype name"),
    ("nom, fichier ou PID…", "name, file or PID…"),
    ("Utiliser l'objet sélectionné", "Use the selected object"),
    ("Recherché", "Searching for"),
    ("Choisissez un prototype.", "Choose a prototype."),
    ("Toutes les élévations", "All elevations"),
    ("Chercher", "Search"),
    ("trouvé(s)", "found"),
    ("Précédent", "Previous"),
    ("Suivant", "Next"),
    (
        "Remplacer par (palette « Placer »)",
        "Replace with (« Place » palette)",
    ),
    (
        "Pour remplacer, choisissez d'abord un prototype dans l'outil « Placer ».",
        "To replace, first choose a prototype in the « Place » tool.",
    ),
    ("Remplacer tous les résultats", "Replace all results"),
    (
        "Activez le mode édition pour remplacer.",
        "Turn on edit mode to replace.",
    ),
    ("objet(s) trouvé(s)", "object(s) found"),
    ("objet(s) remplacé(s)", "object(s) replaced"),
    ("Remplacement d'objets", "Object replacement"),
    ("Éclairage (sources lumineuses)", "Lighting (light sources)"),
    ("source(s) lumineuse(s)", "light source(s)"),
    ("Murs de devant", "Front walls"),
    ("Fenêtres", "Windows"),
    (
        "Aucune fenêtre observée dans ce style.",
        "No window seen in this style.",
    ),
    ("Poser un toit", "Lay a roof"),
    ("Famille de toit", "Roof family"),
    ("fenêtre(s)", "window(s)"),
    ("Dupliquer la sélection", "Duplicate selection"),
    ("Tout sélectionner", "Select all"),
    ("Fermer la carte", "Close map"),
    ("Miniatures des cartes", "Map thumbnails"),
    (
        "Cliquez sur un raccourci, puis appuyez sur la nouvelle touche (Ctrl s'ajoute tout seul quand il est indiqué).",
        "Click a shortcut, then press the new key (Ctrl is added automatically when shown).",
    ),
    ("Poser une porte", "Place a door"),
    ("Mobilier", "Furniture"),
    (
        "Aucun mobilier observé près des murs de ce style.",
        "No furniture seen near the walls of this style.",
    ),
    ("porte(s)", "door(s)"),
    ("objet(s) de mobilier", "furniture object(s)"),
    (
        "un mur est plus court que demandé : pas assez de données pour ce style",
        "a wall is shorter than requested: not enough data for this style",
    ),
    ("Pièce", "Room"),
    (
        "Cliquez pour construire une pièce : deux murs du fond et un sol.",
        "Click to build a room: two back walls and a floor.",
    ),
    (
        "Cliquez sur la carte : le coin du haut de la pièce se pose sur l'hexagone, les murs du fond partent vers la gauche et vers la droite.",
        "Click on the map: the top corner of the room goes on the hex, the back walls run to the left and to the right.",
    ),
    (
        "Les styles de murs viennent de l'analyse de toutes les cartes.",
        "Wall styles come from the analysis of all maps.",
    ),
    ("Analyser toutes les cartes", "Analyze all maps"),
    ("Style de murs", "Wall style"),
    ("Aucun style de mur utilisable.", "No usable wall style."),
    ("Mur de gauche", "Left wall"),
    ("Mur de droite", "Right wall"),
    ("Poser un sol", "Lay a floor"),
    ("Famille de sol", "Floor family"),
    (
        "Analysez d'abord toutes les cartes (panneau d'édition).",
        "Analyze all maps first (edit panel).",
    ),
    ("Pièce générée :", "Room generated:"),
    ("pièce(s) de mur", "wall piece(s)"),
    ("sans prototype", "without prototype"),
    ("Génération d'une pièce", "Room generation"),
    ("Enregistrer la carte sous", "Save map as"),
    ("Récents", "Recent"),
    ("Aucun dossier récent.", "No recent folder."),
    ("Emplacements", "Places"),
    ("Dossier personnel", "Home folder"),
    ("Dossier parent", "Parent folder"),
    (
        "Ce fichier existe déjà : il sera remplacé.",
        "This file already exists: it will be replaced.",
    ),
    ("Choisir ce dossier", "Choose this folder"),
    (
        "Lancer le jeu n'est pas disponible pour Fallout 1.",
        "Launching the game is not available for Fallout 1.",
    ),
    ("Éclairage (halos des lumières)", "Lighting (light halos)"),
    (
        "Éclairage : halo de chaque objet lumineux",
        "Lighting: halo of each light-emitting object",
    ),
    ("Mémoriser les dimensions", "Remember size"),
    ("Mémoriser la position", "Remember position"),
    ("Raccourcis clavier", "Keyboard shortcuts"),
    ("Ajuster la vue", "Fit view"),
    ("Centrer sur le départ", "Center on start"),
    ("Tourner la sélection", "Rotate selection"),
    ("Pipette", "Eyedropper"),
    ("Appuyez sur une touche…", "Press a key…"),
    ("Touches par défaut", "Default keys"),
    ("Journal…", "Log…"),
    ("Journal", "Log"),
    ("Effacer", "Clear"),
    ("Modification", "Edit"),
    ("Propriétés d'un objet", "Object properties"),
    ("Obscurité de la carte", "Map darkness"),
    ("Peinture de tuiles", "Tile painting"),
    ("Propriétés d'une sortie", "Exit properties"),
    ("Limites de défilement", "Scroll limits"),
    ("Drapeaux d'un objet", "Object flags"),
    ("Inventaire d'un objet", "Object inventory"),
    ("Script spatial", "Spatial script"),
    ("Remplissage plausible", "Plausible fill"),
    (
        "Installations et profils de test",
        "Installations and test profiles",
    ),
    (
        "Un profil associe un dossier de mapping à une installation de test facultative.",
        "A profile pairs a mapping folder with an optional test installation.",
    ),
    ("Profils enregistrés", "Saved profiles"),
    ("Actif", "Active"),
    ("Utiliser", "Use"),
    ("Modifier le profil", "Edit profile"),
    ("Nouveau profil", "New profile"),
    ("Nom du mod", "Mod name"),
    (
        "Dossier de mapping (données à éditer)",
        "Mapping folder (data to edit)",
    ),
    (
        "Dossier de test (facultatif; vide = mapping)",
        "Test folder (optional; blank = mapping)",
    ),
    ("Mettre à jour", "Update"),
    ("Ajouter", "Add"),
    ("Nouveau", "New"),
    ("Supprimer", "Delete"),
    (
        "Enregistrez ou fermez les cartes modifiées avant de changer de profil.",
        "Save or close modified maps before switching profiles.",
    ),
    (
        "Échec de copie vers le dossier de test",
        "Could not copy to the test folder",
    ),
    (
        "La carte enregistrée sera copiée dans le dossier de test avant le lancement.",
        "The saved map will be copied to the test folder before launch.",
    ),
    ("Recharger dans Fallout Fusion", "Reload in Fallout Fusion"),
    (
        "Canal Fusion désactivé : configurez [debug] control_port dans fallout2.cfg du dossier de test.",
        "Fusion control channel is disabled: configure [debug] control_port in the test folder's fallout2.cfg.",
    ),
    (
        "Connexion à Fallout Fusion…",
        "Connecting to Fallout Fusion…",
    ),
    (
        "Carte rechargée dans Fallout Fusion",
        "Map reloaded in Fallout Fusion",
    ),
    (
        "Connexion à Fallout Fusion impossible",
        "Could not connect to Fallout Fusion",
    ),
    ("Remplacement de famille", "Family replacement"),
    ("Point de départ", "Player start"),
    ("Placement d'un objet", "Object placement"),
    ("Script attaché", "Script attached"),
    ("Script détaché", "Script detached"),
    ("Pose de sorties", "Exit placement"),
    ("Collage", "Paste"),
    ("Étirement", "Stretch"),
    ("Ajout à l'inventaire", "Added to inventory"),
    ("Script principal de la carte", "Map main script"),
    ("Script spatial supprimé", "Spatial script removed"),
    ("Script spatial ajouté", "Spatial script added"),
    ("Déplacement", "Move"),
    ("Suppression", "Deletion"),
    ("Rotation", "Rotation"),
    ("Duplication", "Duplication"),
    ("Remplacement de cases", "Tile replacement"),
    ("Pose d'un motif", "Stamp placement"),
    ("objet(s) copié(s)", "object(s) copied"),
    ("Cloner", "Clone"),
    ("Copier (Ctrl+C)", "Copy (Ctrl+C)"),
    ("Coller (Ctrl+V)", "Paste (Ctrl+V)"),
    ("Voir le script", "View script"),
    ("Légende des repères", "Marker legend"),
    ("Point de départ du joueur", "Player start"),
    (
        "Grille de sortie (vers une autre carte)",
        "Exit grid (to another map)",
    ),
    (
        "Script spatial (déclenché au passage)",
        "Spatial script (triggered when walked on)",
    ),
    ("Objet masqué", "Hidden object"),
    (
        "Objet ou créature avec un script (Affichage › Surligner)",
        "Object or creature with a script (View › Highlight)",
    ),
    ("Élément sélectionné", "Selected element"),
    ("Limite de défilement (.edg)", "Scroll limit (.edg)"),
    ("Zone de bloqueurs de défilement", "Scroll blocker area"),
    ("Gabarit", "Template"),
    ("Jeu", "Game"),
    ("Isoler", "Isolate"),
    (
        "N'afficher que la procédure choisie",
        "Show only the chosen procedure",
    ),
    ("Aller à", "Go to"),
    (
        "Afficher tout le script et aller à la procédure",
        "Show the whole script and jump to the procedure",
    ),
    (
        "Réduire la police (Ctrl + molette)",
        "Smaller font (Ctrl + wheel)",
    ),
    (
        "Agrandir la police (Ctrl + molette)",
        "Larger font (Ctrl + wheel)",
    ),
    ("Afficher ou masquer", "Show or hide"),
    ("Lieux", "Places"),
    ("Entrées", "Entrances"),
    ("Ouvrir", "Open"),
    ("Ouvrir la carte locale", "Open the local map"),
    (
        "molette : zoom · clic milieu : 100 % · glisser un marqueur : déplacer",
        "wheel: zoom · middle click: 100 % · drag a marker: move",
    ),
    ("Créatures", "Creatures"),
    ("Décors", "Scenery"),
    ("Murs", "Walls"),
    ("Sorties de carte", "Map exits"),
    ("Bloqueurs de défilement", "Scroll blockers"),
    ("Vide", "Empty"),
    ("Désert", "Desert"),
    ("Côte", "Coast"),
    ("Ville", "Town"),
    ("Intérieur", "Interior"),
    (
        "Aucune tuile : tout est à peindre.",
        "No tiles: everything is yours to paint.",
    ),
    (
        "Sol de terre et de sable sur toute la carte.",
        "Dirt and sand floor over the whole map.",
    ),
    (
        "Mer à gauche, rivage, puis terre.",
        "Sea on the left, shore, then land.",
    ),
    (
        "Sol de terre traversé par une route.",
        "Dirt floor crossed by a road.",
    ),
    (
        "Sol de bâtiment partout, sans toit.",
        "Building floor everywhere, no roof.",
    ),
    ("+ entrée", "+ entry"),
    ("8 caractères max", "8 characters max"),
    ("Affichage", "View"),
    (
        "Carte de référence illisible.",
        "The reference map cannot be read.",
    ),
    (
        "La carte de référence n'a pas de blocs assez grands : réduisez la taille des blocs.",
        "The reference map has no blocks that large: reduce the block size.",
    ),
    ("décor(s) semé(s)", "decor item(s) scattered"),
    (
        "Synthèse d'après une carte du jeu",
        "Synthesis from a game map",
    ),
    (
        "Recopie des blocs de la carte choisie : les transitions viennent de la carte d'origine.",
        "Copies blocks from the chosen map: transitions come from the original map.",
    ),
    ("Modèle", "Model"),
    ("Taille des blocs", "Block size"),
    (
        "Semer aussi des décors de cette carte",
        "Also scatter decor from this map",
    ),
    ("Décors pour 1000 hexagones", "Decor per 1000 hexes"),
    ("Remplir la sélection", "Fill the selection"),
    (
        "Recompose la sélection à partir de blocs de la carte modèle",
        "Rebuilds the selection from blocks of the model map",
    ),
    ("Autre tirage", "Another draw"),
    (
        "Défait la dernière synthèse et en refait une autre",
        "Undoes the last synthesis and makes another",
    ),
    (
        "Sélectionnez des cases (outil Sélection, filtre Sols ou Toits).",
        "Select some squares (Select tool, Floors or Roofs filter).",
    ),
    ("Personnaliser la barre d'outils…", "Customize the toolbar…"),
    ("Rétablir la barre d'outils", "Reset the toolbar"),
    ("Barre d'outils", "Toolbar"),
    (
        "Cochez les outils à montrer ; les flèches changent leur ordre.",
        "Tick the tools to show; the arrows change their order.",
    ),
    (
        "Centrer la vue sur cet hexagone",
        "Center the view on this hex",
    ),
    (
        "Cliquez sur la carte pour poser un script spatial : un cercle montre son rayon.",
        "Click the map to place a spatial script: a circle shows its radius.",
    ),
    ("Rayon (en hexagones)", "Radius (in hexes)"),
    ("Script à poser :", "Script to place:"),
    (
        "Aucun script choisi : cherchez-en un dans la liste.",
        "No script chosen: search for one in the list.",
    ),
    ("Analyse du script", "Script analysis"),
    ("Analyser", "Analyze"),
    (
        "Relit la source et ses inclusions : procédures, appels, variables, messages",
        "Re-reads the source and its includes: procedures, calls, variables, messages",
    ),
    (
        "(source modifiée depuis l'analyse)",
        "(source changed since the analysis)",
    ),
    ("Inclusions introuvables :", "Includes not found:"),
    ("Procédures jamais appelées :", "Procedures never called:"),
    ("Déclarées sans corps :", "Declared without a body:"),
    ("ligne", "line"),
    ("Appelée par", "Called by"),
    ("Appelle", "Calls"),
    ("Variables globales lues", "Global variables read"),
    ("Variables globales écrites", "Global variables written"),
    ("Variables de carte lues", "Map variables read"),
    ("Variables de carte écrites", "Map variables written"),
    ("Variables locales lues", "Local variables read"),
    ("Variables locales écrites", "Local variables written"),
    ("Messages", "Messages"),
    ("Prototypes créés", "Prototypes created"),
    ("Références non littérales", "Non-literal references"),
    ("Variable globale n°", "Global variable no."),
    ("écrit", "writes"),
    ("lit", "reads"),
    ("Aucune procédure ne l'utilise.", "No procedure uses it."),
    ("Dossiers", "Folders"),
    ("Cartes regroupées par lieu", "Maps grouped by place"),
    ("Liste", "List"),
    ("Toutes les cartes à la suite", "All maps in a row"),
    ("A–Z", "A–Z"),
    ("Ordre alphabétique", "Alphabetical order"),
    ("Ordre du jeu", "Game order"),
    (
        "Ordre des cartes dans le jeu (numéro de maps.txt)",
        "Order of the maps in the game (maps.txt number)",
    ),
    ("Décroissant", "Descending"),
    ("Croissant", "Ascending"),
    ("Bâtiments du jeu", "Game buildings"),
    (
        "Les bâtiments viennent de l'analyse de toutes les cartes.",
        "Buildings come from the analysis of all maps.",
    ),
    (
        "Des constructions réelles du jeu : leurs murs sont exactement ceux des cartes.",
        "Real constructions from the game: their walls are exactly those of the maps.",
    ),
    ("murs", "walls"),
    ("Décalage X", "Offset X"),
    ("Décalage Y", "Offset Y"),
    ("Quitter", "Quit"),
    ("Style précédent", "Previous style"),
    ("Style suivant", "Next style"),
    ("Autre variante", "Another layout"),
    ("Précédente", "Previous"),
    (
        "Tire une nouvelle disposition (bouton « suivant » de la souris) ; « précédent » revient à la précédente.",
        "Draws a new layout (mouse \"forward\" button); \"back\" returns to the previous one.",
    ),
    ("Fenêtre", "Window"),
    (
        "Afficher le nom à côté des miniatures",
        "Show names next to thumbnails",
    ),
    (
        "Ajouter la sélection comme variante",
        "Add selection as variant",
    ),
    ("Ajouter un objet :", "Add an object:"),
    (
        "Ajouter un script spatial (au départ du joueur) :",
        "Add a spatial script (at player start):",
    ),
    ("Ajouter une entrée", "Add an entry"),
    ("Ajuster à la fenêtre", "Fit to window"),
    (
        "Animer la palette (eau, feu, écrans…)",
        "Animate palette (water, fire, screens…)",
    ),
    ("Annuler (Ctrl+Z)", "Undo (Ctrl+Z)"),
    ("Attacher le programme :", "Attach program:"),
    ("Aucun jeu ouvert.", "No game open."),
    ("Aucun script ouvert.", "No script open."),
    ("Aucun script.", "No script."),
    ("Aucune carte.", "No map."),
    (
        "Autre côté de la bande (Espace)",
        "Other side of the strip (Space)",
    ),
    ("Capturer", "Capture"),
    ("Carte du monde (city.txt)", "World map (city.txt)"),
    ("Carte du monde", "World map"),
    ("Carte du monde…", "World map…"),
    ("Carte", "Map"),
    ("Cartes du jeu (maps.txt)", "Game maps (maps.txt)"),
    ("Cartes du jeu (maps.txt)…", "Game maps (maps.txt)…"),
    ("Cartes en miniatures (Ctrl+B)", "Map thumbnails (Ctrl+B)"),
    ("Cartes — miniatures", "Maps — thumbnails"),
    ("Cartes", "Maps"),
    ("Chargement des prototypes…", "Loading prototypes…"),
    ("Chargement du catalogue…", "Loading catalog…"),
    (
        "Choisissez un prototype, puis cliquez sur la carte.",
        "Pick a prototype, then click on the map.",
    ),
    (
        "Choisissez un script dans la liste.",
        "Pick a script from the list.",
    ),
    (
        "Clic : ajoute un sommet · Entrée ou double-clic : termine · Échap : annule.",
        "Click: add a vertex · Enter or double-click: finish · Esc: cancel.",
    ),
    (
        "Clic : pose le motif · R : variante suivante · Maj+clic droit : annuler.",
        "Click: place the pattern · R: next variant · Shift+right click: cancel.",
    ),
    ("Clic : pose une sortie.", "Click: place an exit."),
    (
        "Clic : sélectionner (re-clic au même endroit : élément suivant sous le curseur).",
        "Click: select (click again at the same spot: next element under the cursor).",
    ),
    (
        "Cliquez sur la carte pour placer le point de départ du joueur (élévation courante).",
        "Click on the map to place the player start (current elevation).",
    ),
    ("Cliquez sur un marqueur.", "Click on a marker."),
    ("Cliquez sur un objet.", "Click on an object."),
    ("Destination", "Destination"),
    ("Drapeaux et lumière", "Flags and light"),
    ("Dupliquer", "Duplicate"),
    ("Décalage X / Y", "Offset X / Y"),
    ("Démarrage", "Startup"),
    ("En-tête de la carte", "Map header"),
    ("Enregistrer (Ctrl+S)", "Save (Ctrl+S)"),
    ("Enregistrer city.txt", "Save city.txt"),
    ("Enregistrer et jouer (F5)", "Save and play (F5)"),
    ("Enregistrer sous…", "Save as…"),
    ("Enregistrer", "Save"),
    ("Fermer la carte (Ctrl+W)", "Close map (Ctrl+W)"),
    ("Fermer", "Close"),
    ("Fichier", "File"),
    ("Filtrer…", "Filter…"),
    (
        "Glisser sur le vide : sélection par zone ; Alt ajoute, Ctrl retire, Maj étend une plage de cases.",
        "Drag on empty space: box selection; Alt adds, Ctrl removes, Shift extends a range of tiles.",
    ),
    (
        "Glisser un objet : le déplacer (avec tout le groupe sélectionné).",
        "Drag an object: move it (with the whole selected group).",
    ),
    (
        "Glissez un marqueur pour déplacer une zone.",
        "Drag a marker to move a zone.",
    ),
    (
        "Glissez un rectangle sur la carte : des bloqueurs de défilement sont posés sur son contour.",
        "Drag a rectangle on the map: scroll blockers are placed along its outline.",
    ),
    ("Gomme (tuile vide)", "Eraser (empty tile)"),
    ("Grille hexagonale", "Hex grid"),
    ("Hexagone d'arrivée", "Arrival hex"),
    ("Hexagone", "Hex"),
    ("Image", "Image"),
    ("Intensité (0–65536)", "Intensity (0–65536)"),
    ("Interface", "Interface"),
    ("Jouer cette carte", "Play this map"),
    (
        "La carte sera enregistrée dans data/maps/, puis le jeu démarrera dessus.",
        "The map will be saved in data/maps/, then the game will start on it.",
    ),
    (
        "La destination vaut pour les sorties posées ensuite ; pour une sortie existante, sélectionnez-la.",
        "The destination applies to exits placed afterwards; to edit an existing exit, select it.",
    ),
    (
        "Les autres lignes (points de départ aléatoires, commentaires…) sont conservées telles quelles.",
        "Other lines (random start points, comments…) are kept as they are.",
    ),
    (
        "Les changements non enregistrés seront perdus si vous les abandonnez.",
        "Unsaved changes will be lost if you discard them.",
    ),
    ("Limites de défilement (.edg)", "Scroll limits (.edg)"),
    ("Listing des opcodes", "Opcode listing"),
    ("Masqué", "Hidden"),
    ("Mode édition", "Edit mode"),
    ("Modifications non enregistrées", "Unsaved changes"),
    ("Ne bloque pas le passage", "Does not block movement"),
    ("Nom du fichier", "File name"),
    ("Nom", "Name"),
    (
        "Nouveau motif depuis la sélection",
        "New pattern from selection",
    ),
    ("Nouvelle carte", "New map"),
    ("Nouvelle carte… (Ctrl+N)", "New map… (Ctrl+N)"),
    ("Objets", "Objects"),
    ("Obscurité", "Darkness"),
    ("Options", "Options"),
    ("Options…", "Options…"),
    ("Orientation", "Orientation"),
    ("Ouvrir le dossier du jeu…", "Open game folder…"),
    (
        "Palettes (sols, toits, objets)",
        "Palettes (floors, roofs, objects)",
    ),
    ("Par défaut", "Default"),
    ("Plan de ville", "Town map"),
    ("Position", "Position"),
    (
        "Pour cela, la ligne StartingMap de ddraw.ini est modifiée (copie dans ddraw.ini.fomapper.bak,\nrestaurable depuis le menu Fichier).",
        "For this, the StartingMap line of ddraw.ini is modified (backup in ddraw.ini.fomapper.bak,\nrestorable from the File menu).",
    ),
    ("Pseudo-source", "Pseudo-source"),
    ("Rayon de lumière", "Light radius"),
    ("Rechercher…", "Search…"),
    ("Repères (sorties, scripts…)", "Markers (exits, scripts…)"),
    ("Restaurer ddraw.ini", "Restore ddraw.ini"),
    (
        "Rouvrir le dernier dossier de Fallout 2 (et la dernière carte)",
        "Reopen the last Fallout 2 folder (and last map)",
    ),
    ("Rétablir (Ctrl+Y)", "Redo (Ctrl+Y)"),
    ("Script principal", "Main script"),
    ("Scripts de la carte", "Map scripts"),
    ("Scripts", "Scripts"),
    ("Sols", "Floors"),
    ("Sortie de carte", "Map exit"),
    (
        "Suppr · R tourner · Ctrl+J dupliquer · Ctrl+A tout · Ctrl+D / Échap désélectionner.",
        "Del · R rotate · Ctrl+J duplicate · Ctrl+A all · Ctrl+D / Esc deselect.",
    ),
    (
        "Supprimer ce motif de la bibliothèque",
        "Delete this pattern from the library",
    ),
    ("Supprimer la dernière", "Delete the last one"),
    ("Supprimer les cases", "Delete tiles"),
    ("Supprimer", "Delete"),
    ("Sélection", "Selection"),
    ("Sélectionner :", "Select:"),
    ("Sélectionnez une carte.", "Select a map."),
    ("Taille des miniatures", "Thumbnail size"),
    ("Taille", "Size"),
    ("Terminer le tracé", "Finish outline"),
    ("Toits", "Roofs"),
    ("Tourner", "Rotate"),
    ("Tout désélectionner", "Deselect all"),
    ("Tout le script", "Whole script"),
    ("Tracer un bord", "Trace an edge"),
    ("Un hexagone", "One hex"),
    ("Vider", "Clear"),
    ("Visible au départ", "Visible at start"),
    (
        "Zones en numéros d'hexagones (gauche, haut, droite, bas). Sans zone, pas de fichier.",
        "Zones as hex numbers (left, top, right, bottom). No zone, no file.",
    ),
    ("Zoom", "Zoom"),
    (
        "À plat (dessiné sous les autres)",
        "Flat (drawn under the others)",
    ),
    ("Échelle", "Scale"),
    ("Édition", "Edit"),
    ("Élévation", "Elevation"),
    ("Élévations", "Elevations"),
    ("↶  Annuler", "↶  Undo"),
    ("↷  Rétablir", "↷  Redo"),
    ("Annuler", "Cancel"),
    ("Rétablir", "Redo"),
    ("Abandonner", "Discard"),
    ("Créer", "Create"),
    ("Enregistrer et lancer", "Save and launch"),
    ("Langue", "Language"),
    ("Thème", "Theme"),
    ("Sombre", "Dark"),
    ("Clair", "Light"),
    ("Système", "System"),
    ("Départ", "Start"),
    ("Sortie", "Exit"),
    ("Bloqueurs", "Blockers"),
    ("Tampon", "Stamp"),
    ("Placer", "Place"),
    (
        "Enregistrer et jouer cette carte (F5)",
        "Save and play this map (F5)",
    ),
    (
        "Mode édition : active les outils et la modification de la carte",
        "Edit mode: enables the tools and map modification",
    ),
    (
        "Calques affichés : sols, objets, toits…",
        "Visible layers: floors, objects, roofs…",
    ),
    (
        "Repères : point de départ, sorties, scripts spatiaux",
        "Markers: player start, exits, spatial scripts",
    ),
    (
        "Grille hexagonale (visible à partir de 60 % de zoom)",
        "Hex grid (visible from 60 % zoom)",
    ),
    (
        "Animation des couleurs de palette (eau, feu, écrans…)",
        "Palette color animation (water, fire, screens…)",
    ),
    ("Ajuster la carte à la fenêtre", "Fit the map to the window"),
    (
        "Zoom (molette de la souris sur la carte)",
        "Zoom (mouse wheel over the map)",
    ),
    (
        "Clic ou rectangle pour sélectionner (Alt ajoute, Ctrl retire). Glisser un objet le déplace.",
        "Click or drag a box to select (Alt adds, Ctrl removes). Dragging an object moves it.",
    ),
    (
        "Glissez sur la carte pour peindre la tuile de sol choisie.",
        "Drag on the map to paint the chosen floor tile.",
    ),
    (
        "Glissez sur la carte pour peindre la tuile de toit choisie.",
        "Drag on the map to paint the chosen roof tile.",
    ),
    (
        "Choisissez un prototype dans la palette, puis cliquez sur la carte (R fait tourner).",
        "Pick a prototype in the palette, then click on the map (R rotates).",
    ),
    (
        "Cliquez pour placer le point d'arrivée du joueur.",
        "Click to place the player's arrival point.",
    ),
    (
        "Posez des grilles de sortie vers une autre carte.",
        "Place exit grids leading to another map.",
    ),
    (
        "Glissez un rectangle : des bloqueurs de défilement bordent son contour.",
        "Drag a rectangle: scroll blockers line its outline.",
    ),
    (
        "Copiez la sélection, enregistrez-la comme motif, puis collez-la.",
        "Copy the selection, save it as a pattern, then paste it.",
    ),
    ("Panneau des cartes", "Maps panel"),
    ("Panneau d'édition et d'infos", "Edit and info panel"),
    (
        "Survolez un bouton pour afficher son aide.",
        "Hover a button to see its help.",
    ),
    ("carte", "map"),
    ("sortie(s) posée(s)", "exit(s) placed"),
    (
        "Prototype de bloqueur de défilement introuvable.",
        "Scroll blocker prototype not found.",
    ),
    (
        "bloqueur(s) de défilement posé(s)",
        "scroll blocker(s) placed",
    ),
    ("zone(s)", "zone(s)"),
    ("+ zone", "+ zone"),
    (
        "L'élévation 0 doit avoir une zone pour que le fichier soit valide.",
        "Elevation 0 must have a zone for the file to be valid.",
    ),
    ("limites de défilement", "scroll limits"),
    ("aucune carte", "no map"),
    (
        "sélectionnez d'abord des objets ou des cases",
        "select objects or tiles first",
    ),
    ("pas d'ancre", "no anchor"),
    ("Motif enregistré", "Pattern saved"),
    (
        "Échec de l'enregistrement du motif",
        "Failed to save the pattern",
    ),
    ("Motif posé", "Pattern placed"),
    ("objet(s)", "object(s)"),
    ("case(s)", "square(s)"),
    (
        "objet(s) non créables (créatures ?)",
        "object(s) cannot be created (creatures?)",
    ),
    ("élément(s) hors grille", "element(s) off the grid"),
    ("Capture impossible", "Capture failed"),
    ("Bibliothèque", "Library"),
    ("variante(s)", "variant(s)"),
    ("toit n°", "roof no."),
    ("sol n°", "floor no."),
    (
        "cet objet ne peut pas être recréé depuis son prototype (dupliquez-le).",
        "this object cannot be recreated from its prototype (duplicate it).",
    ),
    ("Valeur brute", "Raw value"),
    ("Script (liste « Créature »)", "Script (list “Critter”)"),
    ("Script (liste « Objet »)", "Script (list “Object”)"),
    ("Voir", "View"),
    ("Détacher", "Detach"),
    ("Retirer", "Remove"),
    ("aucun", "none"),
    (
        "Script attaché dont le programme est introuvable.",
        "Attached script whose program cannot be found.",
    ),
    ("Inventaire", "Inventory"),
    ("Scripts spatiaux", "Spatial scripts"),
    ("hex", "hex"),
    ("élév.", "elev."),
    ("rayon", "radius"),
    ("Éclairage", "Lighting"),
    ("Ne peut pas être retiré", "Cannot be removed"),
    ("Plusieurs hexagones", "Multi-hex"),
    ("Sans surbrillance", "No highlight"),
    ("En file d'attente", "Queued"),
    ("Transparence rouge", "Red transparency"),
    ("Sans transparence", "No transparency"),
    ("Transparence mur", "Wall transparency"),
    ("Transparence verre", "Glass transparency"),
    ("Transparence vapeur", "Steam transparency"),
    ("Transparence énergie", "Energy transparency"),
    ("Tenu en main gauche", "Held in left hand"),
    ("Tenu en main droite", "Held in right hand"),
    ("Armure portée", "Armor worn"),
    ("Fin de transparence de mur", "End of wall transparency"),
    ("La lumière traverse", "Light passes through"),
    ("Vu", "Seen"),
    ("Les tirs traversent", "Shots pass through"),
    ("non enregistrée", "unsaved"),
    ("Carte fermée", "Map closed"),
    (
        "Le nom doit faire 1 à 8 caractères : a–z, 0–9, _",
        "The name must be 1 to 8 characters: a–z, 0–9, _",
    ),
    (
        "Une carte porte déjà ce nom.",
        "A map already has this name.",
    ),
    ("miniatures", "thumbnails"),
    ("Ouvrez d'abord une carte.", "Open a map first."),
    ("Lancement impossible", "Cannot launch"),
    ("ddraw.ini restauré.", "ddraw.ini restored."),
    (
        "Aucune copie de secours de ddraw.ini.",
        "No backup copy of ddraw.ini.",
    ),
    ("Échec", "Failed"),
    ("Annulé", "Undone"),
    ("Rétabli", "Redone"),
    ("étapes restantes", "steps left"),
    ("Enregistrée", "Saved"),
    ("mais", "but"),
    ("Échec de l'enregistrement", "Save failed"),
    ("Carte Fallout", "Fallout map"),
    ("Enregistrée sous", "Saved as"),
    (
        "La carte a été modifiée. Que faire avant de quitter ?",
        "The map has been modified. What to do before quitting?",
    ),
    (
        "La carte a été modifiée. Que faire avant de fermer la carte ?",
        "The map has been modified. What to do before closing the map?",
    ),
    (
        "Ce type d'objet ne peut pas être créé depuis un prototype (dupliquez un objet existant).",
        "This type of object cannot be created from a prototype (duplicate an existing object).",
    ),
    ("case(s) sélectionnée(s)", "square(s) selected"),
    ("Remplir", "Fill"),
    ("tuile n°", "tile no."),
    ("objets sélectionnés", "objects selected"),
    (
        "Peinture des toits : glissez sur la carte. Tuile choisie : n°",
        "Roof painting: drag on the map. Chosen tile: no.",
    ),
    (
        "Peinture des sols : glissez sur la carte. Tuile choisie : n°",
        "Floor painting: drag on the map. Chosen tile: no.",
    ),
    ("Choisi", "Chosen"),
    ("Objet", "Object"),
    ("Créature", "Critter"),
    ("Décor", "Scenery"),
    ("Mur", "Wall"),
    ("Tuile", "Tile"),
    ("Divers", "Misc"),
    (
        "Choisissez le dossier de Fallout 2 (menu Fichier).",
        "Choose the Fallout 2 folder (File menu).",
    ),
    ("Impossible d'ouvrir", "Cannot open"),
    ("Chargement de", "Loading"),
    ("chargée en", "loaded in"),
    ("Erreur sur", "Error on"),
    ("Dossier de Fallout 2", "Fallout 2 folder"),
    ("Version", "Version"),
    ("Numéro", "Number"),
    ("Script de carte", "Map script"),
    ("Variables", "Variables"),
    ("glob.", "glob."),
    ("loc.", "loc."),
    ("avec inventaires", "with inventories"),
    ("Minuterie", "Timer"),
    ("Art introuvable", "Art not found"),
    ("objet(s) et", "object(s) and"),
    (
        "case(s) sélectionnés : voir le panneau d'édition.",
        "square(s) selected: see the edit panel.",
    ),
    ("Type", "Type"),
    ("Drapeaux", "Flags"),
    ("Script", "Script"),
    ("Sous-type", "Subtype"),
    ("Art", "Art"),
    ("Chargement…", "Loading…"),
    (
        "Choisissez une carte dans la liste",
        "Pick a map from the list",
    ),
    ("● modifiée", "● modified"),
    ("zoom", "zoom"),
    ("Gauche", "Left"),
    ("Droite", "Right"),
    ("Bas", "Bottom"),
    ("Haut", "Top"),
    ("Diagonale / (A)", "Diagonal / (A)"),
    ("Diagonale / (B)", "Diagonal / (B)"),
    ("Diagonale \\ (A)", "Diagonal \\ (A)"),
    ("Diagonale \\ (B)", "Diagonal \\ (B)"),
    ("procédures", "procedures"),
    ("noms", "names"),
    ("chaînes", "strings"),
    ("octets", "bytes"),
    ("data/maps.txt introuvable.", "data/maps.txt not found."),
    ("● modifié", "● modified"),
    ("maps.txt enregistré", "maps.txt saved"),
    (
        "Échec de l'enregistrement de maps.txt",
        "Failed to save maps.txt",
    ),
    ("data/city.txt introuvable.", "data/city.txt not found."),
    ("city.txt enregistré", "city.txt saved"),
    (
        "Échec de l'enregistrement de city.txt",
        "Failed to save city.txt",
    ),
    ("numéro attendu", "expected number"),
    (
        "la liste ne doit avoir aucun trou",
        "the list must have no gaps",
    ),
    ("manquant", "missing"),
    ("déjà utilisé par la carte", "already used by map"),
    ("é", "el"),
    ("objet(s) sélectionné(s)", "object(s) selected"),
    ("case(s) sélectionné(s)", "square(s) selected"),
];

type Table = HashMap<&'static str, &'static str>;

/// Table du français vers la langue `l` (anglais compris) ; vide pour le français.
fn table_of(l: Lang) -> &'static Table {
    static T: OnceLock<Vec<Table>> = OnceLock::new();
    let all = T.get_or_init(|| {
        Lang::ALL
            .iter()
            .map(|l| match l {
                Lang::Fr => HashMap::new(),
                Lang::En => EN.iter().copied().collect(),
                Lang::Es => crate::i18n_es::TABLE.iter().copied().collect(),
                Lang::It => crate::i18n_it::TABLE.iter().copied().collect(),
                Lang::De => crate::i18n_de::TABLE.iter().copied().collect(),
                Lang::Pt => crate::i18n_pt::TABLE.iter().copied().collect(),
                Lang::Ru => crate::i18n_ru::TABLE.iter().copied().collect(),
            })
            .collect()
    });
    &all[l as usize]
}

/// Traduction d'un texte français : la langue courante, sinon l'anglais, sinon le français.
fn lookup(fr: &'static str, l: Lang) -> &'static str {
    match l {
        Lang::Fr => fr,
        Lang::En => table_of(Lang::En).get(fr).copied().unwrap_or(fr),
        _ => table_of(l)
            .get(fr)
            .or_else(|| table_of(Lang::En).get(fr))
            .copied()
            .unwrap_or(fr),
    }
}

/// Comme `t`, pour un texte français qui n'est pas un littéral (nom venu d'une table de `fo-core`, par exemple).
pub(crate) fn tr(fr: &str) -> String {
    let l = lang();
    if l == Lang::Fr {
        return fr.to_string();
    }
    table_of(l)
        .get(fr)
        .or_else(|| table_of(Lang::En).get(fr))
        .copied()
        .unwrap_or(fr)
        .to_string()
}

/// Met un texte entre guillemets de la langue courante (« texte » en français, “texte” en anglais, „Text“ en allemand).
pub(crate) fn quote(s: &str) -> String {
    match lang() {
        Lang::Fr | Lang::Es | Lang::It | Lang::Pt | Lang::Ru => format!("« {s} »"),
        Lang::En => format!("\u{201c}{s}\u{201d}"),
        Lang::De => format!("\u{201e}{s}\u{201c}"),
    }
}

/// Texte de l'interface dans la langue courante (`fr` : le texte français du code).
pub(crate) fn t(fr: &'static str) -> &'static str {
    lookup(fr, lang())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_table_has_duplicate_keys() {
        assert_eq!(table_of(Lang::En).len(), EN.len());
        for (l, t) in [
            (Lang::Es, crate::i18n_es::TABLE),
            (Lang::It, crate::i18n_it::TABLE),
            (Lang::De, crate::i18n_de::TABLE),
            (Lang::Pt, crate::i18n_pt::TABLE),
            (Lang::Ru, crate::i18n_ru::TABLE),
        ] {
            assert_eq!(table_of(l).len(), t.len(), "clés en double en {}", l.code());
        }
    }

    #[test]
    fn every_language_covers_the_english_keys_and_has_no_stale_ones() {
        let en = table_of(Lang::En);
        for l in [Lang::Es, Lang::It, Lang::De, Lang::Pt, Lang::Ru] {
            let t = table_of(l);
            let missing: Vec<&&str> = en.keys().filter(|k| !t.contains_key(*k)).take(5).collect();
            assert!(
                missing.is_empty(),
                "{} : textes sans traduction, p. ex. {missing:?}",
                l.code()
            );
            let stale: Vec<&&str> = t.keys().filter(|k| !en.contains_key(*k)).take(5).collect();
            assert!(stale.is_empty(), "{} : clés inconnues {stale:?}", l.code());
            assert!(
                t.values().all(|v| !v.trim().is_empty()),
                "{} : traduction vide",
                l.code()
            );
        }
    }

    #[test]
    fn translations_keep_symbols_and_placeholders() {
        let en = table_of(Lang::En);
        for l in [Lang::Es, Lang::It, Lang::De, Lang::Pt, Lang::Ru] {
            for (fr, tr) in table_of(l) {
                for sym in ["…", "●", "✕", "%", "{}"] {
                    assert_eq!(
                        fr.contains(sym),
                        tr.contains(sym),
                        "{} : « {fr} » → « {tr} » : symbole {sym}",
                        l.code()
                    );
                }
            }
        }
        assert!(!en.is_empty());
    }

    #[test]
    fn falls_back_to_french_text_and_english_for_untranslated() {
        set_lang(Lang::En);
        assert_eq!(t("Fichier"), "File");
        assert_eq!(t("texte inconnu"), "texte inconnu");
        set_lang(Lang::Fr);
        assert_eq!(t("Fichier"), "Fichier");
        set_lang(Lang::Ru);
        assert_ne!(t("Fichier"), "Fichier");
        assert_eq!(t("texte inconnu"), "texte inconnu");
        set_lang(Lang::Fr);
    }

    #[test]
    fn language_codes_round_trip() {
        for l in Lang::ALL {
            assert_eq!(Lang::from_code(l.code()), Some(l));
            assert_eq!(Lang::from_index(l as u8), l);
        }
    }
}
