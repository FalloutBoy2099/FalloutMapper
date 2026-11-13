//! Édition de la carte : outils, annulation / rétablissement, palettes, enregistrement.

use crate::app::MapperApp;
use crate::exit_ui::ExitTool;
use crate::stamp_ui::StampState;
use eframe::egui::Pos2;
use fo_core::map::Map;
use fo_core::{ObjectType, ProtoEntry};
use fo_scene::Layer;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

pub(crate) const MAX_UNDO: usize = 50;
/// Délai pendant lequel des modifications successives d'un même champ ne forment qu'une étape d'annulation.
pub(crate) const COALESCE: Duration = Duration::from_millis(1200);
pub(crate) const REBUILD_INTERVAL: Duration = Duration::from_millis(120);

/// Ce que l'outil Sélection désigne : tout selon le filtre, ou seulement des hexagones, ou seulement des bloqueurs de défilement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectMode {
    Normal,
    Hexes,
    Blockers,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tool {
    Select,
    Floor,
    Roof,
    Place,
    PlayerStart,
    Exit,
    Blocker,
    Stamp,
    Room,
    Wall,
    Spatial,
}

impl Tool {
    pub(crate) const ALL: [(Tool, &'static str); 11] = [
        (Tool::Select, "Sélection"),
        (Tool::Floor, "Sols"),
        (Tool::Roof, "Toits"),
        (Tool::Place, "Placer"),
        (Tool::PlayerStart, "Départ"),
        (Tool::Exit, "Sortie"),
        (Tool::Blocker, "Bloqueurs"),
        (Tool::Stamp, "Tampon"),
        (Tool::Room, "Pièce"),
        (Tool::Wall, "Murs"),
        (Tool::Spatial, "Script spatial"),
    ];
}

pub(crate) enum CatalogState {
    Loading(Receiver<Vec<ProtoEntry>>),
    Ready(Arc<Vec<ProtoEntry>>),
}

pub(crate) struct ObjectDrag {
    pub(crate) from_tile: u32,
}

pub(crate) struct EditState {
    pub enabled: bool,
    pub tool: Tool,
    pub(crate) undo: Vec<Map>,
    pub(crate) redo: Vec<Map>,
    /// Libellés de l'historique, parallèles à `undo` et `redo`.
    pub(crate) undo_labels: Vec<&'static str>,
    /// Sélection verrouillée : un clic, un rectangle ou Échap ne la change plus (seuls le déplacement et les propriétés restent possibles).
    pub(crate) selection_locked: bool,
    pub(crate) redo_labels: Vec<&'static str>,
    pub dirty: bool,
    pub brush_tile: u16,
    /// Types d'éléments que le clic et la zone peuvent sélectionner.
    pub pick_objects: bool,
    /// Mode de sélection : normal, ou exclusif (hexagones, bloqueurs de défilement).
    pub select_mode: SelectMode,
    pub pick_floor: bool,
    pub pick_roof: bool,
    /// Début (écran) d'un rectangle de sélection en cours.
    pub band: Option<Pos2>,
    pub exit: ExitTool,
    pub script_filter: String,
    pub inventory_filter: String,
    pub map_script_filter: String,
    pub spatial_filter: String,
    /// Outil « Script spatial » : script choisi (indice de `scripts.lst`) et rayon à poser.
    pub spatial_prog: Option<usize>,
    pub spatial_radius: i32,
    pub stamp: StampState,
    pub room: crate::room_ui::RoomState,
    pub wall: crate::wall_ui::WallTool,
    /// Orientation donnée aux objets posés par l'outil « Placer » (`R` la change).
    pub place_orientation: u32,
    /// Début (écran) du rectangle de bloqueurs de défilement en cours.
    pub blocker_start: Option<Pos2>,
    /// Dernier clic de sélection (point du monde, rang dans la pile) pour parcourir les éléments superposés.
    pub last_click: Option<(Pos2, usize)>,
    pub place_type: ObjectType,
    pub place: Option<ProtoEntry>,
    pub catalogs: HashMap<ObjectType, CatalogState>,
    pub proto_filter: String,
    pub tile_filter: String,
    /// Agrandissement demandé pour « Étirer la sélection » (pas le long de u et de v).
    pub stretch: (i32, i32),
    pub(crate) tile_names: Option<Arc<Vec<String>>>,
    pub(crate) drag: Option<ObjectDrag>,
    pub(crate) stroke: bool,
    pub(crate) stroke_changed: bool,
    pub(crate) last_rebuild: Instant,
    pub(crate) last_tag: Option<(&'static str, Instant)>,
}

impl Default for EditState {
    fn default() -> Self {
        Self {
            enabled: false,
            tool: Tool::Select,
            undo: Vec::new(),
            redo: Vec::new(),
            undo_labels: Vec::new(),
            selection_locked: false,
            redo_labels: Vec::new(),
            dirty: false,
            brush_tile: 2,
            pick_objects: true,
            select_mode: SelectMode::Normal,
            pick_floor: false,
            pick_roof: false,
            band: None,
            exit: ExitTool::default(),
            script_filter: String::new(),
            inventory_filter: String::new(),
            map_script_filter: String::new(),
            spatial_filter: String::new(),
            spatial_prog: None,
            spatial_radius: 3,
            stamp: StampState::default(),
            room: Default::default(),
            wall: Default::default(),
            place_orientation: 0,
            blocker_start: None,
            last_click: None,
            place_type: ObjectType::Scenery,
            place: None,
            catalogs: HashMap::new(),
            proto_filter: String::new(),
            tile_filter: String::new(),
            stretch: (4, 0),
            tile_names: None,
            drag: None,
            stroke: false,
            stroke_changed: false,
            last_rebuild: Instant::now(),
            last_tag: None,
        }
    }
}

impl EditState {
    pub(crate) fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.wall.start = None;
        self.drag = None;
    }

    /// Nouvelle carte chargée : l'historique ne vaut plus.
    pub(crate) fn reset(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.undo_labels.clear();
        self.redo_labels.clear();
        self.dirty = false;
        self.drag = None;
        self.band = None;
        self.exit.points.clear();
        self.blocker_start = None;
        self.last_click = None;
        self.stroke = false;
        self.last_tag = None;
    }

    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(crate) fn push_undo(&mut self, snapshot: Map, label: &'static str) {
        self.undo.push(snapshot);
        self.undo_labels.push(label);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
            self.undo_labels.remove(0);
        }
        self.redo.clear();
        self.redo_labels.clear();
    }
}

impl MapperApp {
    // ---------------------------------------------------------------- historique et mutations

    pub(crate) fn reset_edit_for_new_map(&mut self) {
        self.edit.reset();
    }

    #[allow(dead_code)]
    pub(crate) fn layer_visible(&self, layer: Layer) -> bool {
        self.layers.contains(layer)
    }
}

/// Contour d'un hexagone Fallout (sommet en haut, 32 de large, 16 de haut) autour du centre de la case, comme Gecko.
pub(crate) const HEX_OUTLINE: [(f32, f32); 6] = [
    (0.0, -7.0),
    (16.0, -3.0),
    (16.0, 5.0),
    (0.0, 9.0),
    (-16.0, 5.0),
    (-16.0, -3.0),
];

/// Libellé lisible d'une étape de l'historique d'après l'étiquette de la modification.
pub(crate) fn history_label(tag: &'static str) -> &'static str {
    match tag {
        "object" => "Propriétés d'un objet",
        "darkness" => "Obscurité de la carte",
        "paint" => "Peinture de tuiles",
        "exit" => "Propriétés d'une sortie",
        "edge" => "Limites de défilement",
        "flags" => "Drapeaux d'un objet",
        "inventory" => "Inventaire d'un objet",
        "spatial" => "Script spatial",
        "nudge" => "Déplacement",
        other => other,
    }
}
