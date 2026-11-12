//! Édition de la carte : outils, annulation / rétablissement, palettes, enregistrement.

use crate::app::{Confirm, MapperApp};
use crate::exit_ui::ExitTool;
use crate::stamp_ui::StampState;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, TextureOptions, Vec2};
use fo_core::map::{Map, TileLayer, flags, word};
use fo_core::{Fid, GameData, MapObject, ObjectType, ProtoEntry, Sprite, geom, write_map};
use fo_scene::{DrawItem, Layer, Scene, build_scene};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const MAX_UNDO: usize = 50;
/// Délai pendant lequel des modifications successives d'un même champ ne forment qu'une étape d'annulation.
const COALESCE: Duration = Duration::from_millis(1200);
const REBUILD_INTERVAL: Duration = Duration::from_millis(120);

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

struct ObjectDrag {
    from_tile: u32,
}

pub(crate) struct EditState {
    pub enabled: bool,
    pub tool: Tool,
    undo: Vec<Map>,
    redo: Vec<Map>,
    /// Libellés de l'historique, parallèles à `undo` et `redo`.
    pub(crate) undo_labels: Vec<&'static str>,
    redo_labels: Vec<&'static str>,
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
    drag: Option<ObjectDrag>,
    stroke: bool,
    stroke_changed: bool,
    last_rebuild: Instant,
    last_tag: Option<(&'static str, Instant)>,
}

impl Default for EditState {
    fn default() -> Self {
        Self {
            enabled: false,
            tool: Tool::Select,
            undo: Vec::new(),
            redo: Vec::new(),
            undo_labels: Vec::new(),
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

    fn push_undo(&mut self, snapshot: Map, label: &'static str) {
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

    /// Applique une modification à la carte. `f` renvoie `true` si quelque chose a changé.
    /// Avec une `tag`, les modifications répétées du même champ se fondent en une seule étape d'annulation.
    pub(crate) fn mutate(
        &mut self,
        tag: Option<&'static str>,
        f: impl FnOnce(&mut Map, &GameData) -> bool,
    ) -> bool {
        self.mutate_labeled(tag, None, f)
    }

    /// Comme `mutate` sans fusion, avec le libellé de l'étape d'historique.
    pub(crate) fn mutate_as(
        &mut self,
        label: &'static str,
        f: impl FnOnce(&mut Map, &GameData) -> bool,
    ) -> bool {
        self.mutate_labeled(None, Some(label), f)
    }

    fn mutate_labeled(
        &mut self,
        tag: Option<&'static str>,
        label: Option<&'static str>,
        f: impl FnOnce(&mut Map, &GameData) -> bool,
    ) -> bool {
        let (Some(cur), Some(g)) = (self.current.as_mut(), self.game.as_ref()) else {
            return false;
        };
        let coalesce = tag.is_some()
            && self
                .edit
                .last_tag
                .is_some_and(|(t, at)| Some(t) == tag && at.elapsed() < COALESCE);
        let snapshot = (!coalesce).then(|| (*cur.map).clone());
        if !f(Arc::make_mut(&mut cur.map), &g.data) {
            return false;
        }
        if let Some(s) = snapshot {
            let what = label.or(tag.map(history_label)).unwrap_or("Modification");
            // Le journal garde chaque étape d'édition, avec l'hexagone sous le curseur quand l'action vient d'un clic sur la carte.
            self.log
                .push(crate::i18n::t(what).to_string(), self.hover_hex);
            self.edit.push_undo(s, what);
        } else {
            self.edit.redo.clear();
            self.edit.redo_labels.clear();
        }
        self.edit.dirty = true;
        self.edit.last_tag = tag.map(|t| (t, Instant::now()));
        self.rebuild_scene(self.elevation);
        true
    }

    pub(crate) fn rebuild_scene(&mut self, elevation: usize) {
        let (Some(cur), Some(g)) = (self.current.as_mut(), self.game.as_ref()) else {
            return;
        };
        cur.scenes[elevation] = Some(Arc::new(build_scene(&cur.map, elevation, &g.art)));
        self.edit.last_rebuild = Instant::now();
    }

    fn rebuild_all(&mut self) {
        for e in 0..3 {
            if self
                .current
                .as_ref()
                .is_some_and(|c| c.scenes[e].is_some() || c.map.header.has_elevation(e))
            {
                self.rebuild_scene(e);
            }
        }
    }

    pub(crate) fn undo(&mut self) {
        let Some(cur) = self.current.as_mut() else {
            return;
        };
        if let Some(prev) = self.edit.undo.pop() {
            let now = std::mem::replace(&mut *Arc::make_mut(&mut cur.map), prev);
            self.edit.redo.push(now);
            let label = self.edit.undo_labels.pop().unwrap_or("Modification");
            self.edit.redo_labels.push(label);
            self.after_history_step(crate::i18n::t("Annulé"));
        }
    }

    /// Fenêtre d'historique : revenir (ou avancer) jusqu'à une étape donnée.
    pub(crate) fn history_window(&mut self, ctx: &egui::Context) {
        if !self.history_open {
            return;
        }
        let mut open = true;
        let mut target = None;
        let (done, undone) = (self.edit.undo_labels.clone(), self.edit.redo_labels.clone());
        egui::Window::new(crate::i18n::t("Historique"))
            .open(&mut open)
            .default_size([320.0, 420.0])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if ui
                            .selectable_label(
                                done.is_empty() && undone.is_empty() || false,
                                crate::i18n::t("État initial"),
                            )
                            .clicked()
                        {
                            target = Some(0);
                        }
                        for (i, label) in done.iter().enumerate() {
                            let current = i + 1 == done.len();
                            if ui
                                .selectable_label(
                                    current,
                                    format!("{}. {}", i + 1, crate::i18n::t(label)),
                                )
                                .clicked()
                            {
                                target = Some(i + 1);
                            }
                        }
                        // Étapes annulées : la plus proche de l'état courant est la dernière de `redo_labels`.
                        for (k, label) in undone.iter().rev().enumerate() {
                            let n = done.len() + k + 1;
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(format!(
                                            "{n}. {}",
                                            crate::i18n::t(label)
                                        ))
                                        .weak(),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                target = Some(n);
                            }
                        }
                    });
            });
        if let Some(t) = target {
            let now = self.edit.undo.len();
            for _ in t..now {
                self.undo();
            }
            for _ in now..t {
                self.redo();
            }
        }
        self.history_open = open;
    }

    pub(crate) fn redo(&mut self) {
        let Some(cur) = self.current.as_mut() else {
            return;
        };
        if let Some(next) = self.edit.redo.pop() {
            let now = std::mem::replace(&mut *Arc::make_mut(&mut cur.map), next);
            self.edit.undo.push(now);
            let label = self.edit.redo_labels.pop().unwrap_or("Modification");
            self.edit.undo_labels.push(label);
            self.after_history_step(crate::i18n::t("Rétabli"));
        }
    }

    fn after_history_step(&mut self, what: &str) {
        self.edit.dirty = true;
        self.edit.last_tag = None;
        self.selection.clear();
        self.rebuild_all();
        self.status = format!(
            "{what} ({} {})",
            self.edit.undo.len(),
            crate::i18n::t("étapes restantes")
        );
    }

    // ---------------------------------------------------------------- enregistrement

    pub(crate) fn save_current(&mut self) -> bool {
        let (Some(cur), Some(g)) = (&self.current, &self.game) else {
            return false;
        };
        let result = write_map(&cur.map).and_then(|bytes| g.data.save_map(&cur.file, &bytes));
        let edge_error = result.as_ref().ok().and_then(|_| {
            crate::exit_ui::save_edge_beside(&g.data, &cur.file, cur.map.edge.as_ref())
        });
        match result {
            Ok(path) => {
                self.status = match edge_error {
                    Some(e) => format!(
                        "{} : {} ({} {e})",
                        crate::i18n::t("Enregistrée"),
                        path.display(),
                        crate::i18n::t("mais")
                    ),
                    None => format!("{} : {}", crate::i18n::t("Enregistrée"), path.display()),
                };
                self.edit.dirty = false;
                if let Some(c) = &self.current {
                    self.forget_recovery(&c.file);
                }
                true
            }
            Err(e) => {
                self.status = format!("{} : {e}", crate::i18n::t("Échec de l'enregistrement"));
                false
            }
        }
    }

    pub(crate) fn save_current_as(&mut self) {
        if self.current.is_some() {
            self.open_dialog(crate::file_dialog::Purpose::SaveMapAs);
        }
    }

    /// Écrit une copie de la carte courante à l'emplacement choisi dans le dialogue « Enregistrer sous ».
    pub(crate) fn write_map_copy(&mut self, path: &std::path::Path) {
        let Some(cur) = &self.current else { return };
        let Ok(bytes) = write_map(&cur.map) else {
            return;
        };
        self.status = match std::fs::write(path, bytes) {
            Ok(()) => format!("{} {}", crate::i18n::t("Enregistrée sous"), path.display()),
            Err(e) => format!("{} : {e}", crate::i18n::t("Échec")),
        };
    }

    /// Demande de charger une autre carte : confirmation si des modifications ne sont pas enregistrées.
    /// Charge une carte : elle s'ouvre dans un onglet (les autres cartes gardent leurs modifications).
    pub(crate) fn request_load(&mut self, file: &str, ctx: &egui::Context) {
        self.load_map(file, ctx);
    }

    pub(crate) fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(confirm) = self.confirm.clone() else {
            return;
        };
        let what = match &confirm {
            Confirm::Quit => {
                crate::i18n::t("La carte a été modifiée. Que faire avant de quitter ?")
            }
            Confirm::Close => {
                crate::i18n::t("La carte a été modifiée. Que faire avant de fermer la carte ?")
            }
        };
        let mut choice = None;
        egui::Window::new(crate::i18n::t("Modifications non enregistrées"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_min_width(420.0);
                ui.label(egui::RichText::new(what).size(15.0));
                ui.label(crate::i18n::t(
                    "Les changements non enregistrés seront perdus si vous les abandonnez.",
                ));
                use crate::ui_kit::FooterButton as B;
                match crate::ui_kit::footer(
                    ui,
                    &[
                        B::normal(crate::i18n::t("Annuler")),
                        B::danger(crate::i18n::t("Abandonner")),
                        B::primary(crate::i18n::t("Enregistrer")),
                    ],
                ) {
                    Some(0) => self.confirm = None,
                    Some(1) => choice = Some(false),
                    Some(2) => choice = Some(true),
                    _ => {}
                }
            });
        let Some(save) = choice else { return };
        if save && !self.save_current() {
            return;
        }
        // Abandonner : la copie de récupération n'a plus de raison d'être.
        if let Some(c) = &self.current {
            self.forget_recovery(&c.file);
        }
        self.edit.dirty = false;
        self.confirm = None;
        match confirm {
            Confirm::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Confirm::Close => self.close_map(),
        }
    }

    // ---------------------------------------------------------------- raccourcis

    /// Navigation : F ajuste la vue, Début centre sur le point de départ, Ctrl+1/2/3 changent d'élévation.
    fn navigation_shortcuts(&mut self, ctx: &egui::Context) {
        use crate::prefs::KeyAction;
        use egui::Key;
        let (fit_key, home_key) = (
            self.prefs.key(KeyAction::Fit),
            self.prefs.key(KeyAction::Home),
        );
        let (fit, home, elev) = ctx.input(|i| {
            let cmd = i.modifiers.command;
            let elev = [Key::Num1, Key::Num2, Key::Num3]
                .iter()
                .position(|&k| cmd && i.key_pressed(k));
            (
                !cmd && i.key_pressed(fit_key),
                i.key_pressed(home_key),
                elev,
            )
        });
        let Some(cur) = &self.current else { return };
        if fit {
            self.needs_fit = true;
        }
        if home {
            let h = &cur.map.header;
            if (0..geom::HEX_COUNT as i32).contains(&h.player_tile) {
                if h.player_elevation as usize != self.elevation
                    && cur
                        .scenes
                        .get(h.player_elevation as usize)
                        .is_some_and(Option::is_some)
                {
                    self.elevation = h.player_elevation as usize;
                }
                let (x, y) = geom::hex_to_screen(h.player_tile as u32);
                self.camera.center = Pos2::new(x as f32, y as f32);
            }
        }
        if let Some(e) = elev
            && cur.scenes[e].is_some()
        {
            self.elevation = e;
            self.selection.clear();
            self.needs_fit = true;
        }
    }

    pub(crate) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.egui_wants_keyboard_input();
        // Les raccourcis d'une carte locale ne font rien dans l'onglet Scripts ni dans la carte du monde.
        let local = self.mode == crate::app::Mode::Map;
        use crate::prefs::KeyAction as K;
        let k = |a: K| self.prefs.key(a);
        let (new_key, close_key, thumbs_key) = (k(K::NewMap), k(K::CloseMap), k(K::Thumbnails));
        let (find_key, check_key, preview_key, history_key, options_key) = (
            k(K::Find),
            k(K::Check),
            k(K::Preview),
            k(K::History),
            k(K::Options),
        );
        let (rotate_key, pipette_key, undo_key, redo_key, save_key, dup_key, all_key, desel_key) = (
            k(K::Rotate),
            k(K::Pipette),
            k(K::Undo),
            k(K::Redo),
            k(K::Save),
            k(K::Duplicate),
            k(K::SelectAll),
            k(K::Deselect),
        );
        let (undo, redo, save, delete, rotate, duplicate, select_all, deselect, escape, eyedrop) =
            ctx.input(|i| {
                let cmd = i.modifiers.command;
                (
                    cmd && !i.modifiers.shift && i.key_pressed(undo_key),
                    (cmd && i.key_pressed(redo_key))
                        || (cmd && i.modifiers.shift && i.key_pressed(undo_key)),
                    cmd && i.key_pressed(save_key),
                    i.key_pressed(egui::Key::Delete),
                    !cmd && i.key_pressed(rotate_key),
                    cmd && i.key_pressed(dup_key),
                    cmd && i.key_pressed(all_key),
                    cmd && i.key_pressed(desel_key),
                    i.key_pressed(egui::Key::Escape),
                    !i.modifiers.command && i.key_pressed(pipette_key),
                )
            });
        if save && local {
            self.save_current();
        }
        if !typing && self.edit.enabled && local {
            let (copy, cut, paste) = ctx.input(|i| {
                (
                    i.events.iter().any(|e| matches!(e, egui::Event::Copy)),
                    i.events.iter().any(|e| matches!(e, egui::Event::Cut)),
                    i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))),
                )
            });
            if copy {
                self.copy_selected();
            }
            if cut {
                self.cut_selected();
            }
            if paste {
                self.paste_clipboard();
            }
        }
        if !typing {
            if local {
                self.navigation_shortcuts(ctx);
            }
            if self.edit.enabled && local {
                let keys = [
                    egui::Key::Num1,
                    egui::Key::Num2,
                    egui::Key::Num3,
                    egui::Key::Num4,
                    egui::Key::Num5,
                    egui::Key::Num6,
                    egui::Key::Num7,
                    egui::Key::Num8,
                ];
                let pressed = ctx.input(|i| {
                    if i.modifiers.command || i.modifiers.alt {
                        None
                    } else {
                        keys.iter().position(|&k| i.key_pressed(k))
                    }
                });
                if let Some(n) = pressed {
                    self.edit.set_tool(Tool::ALL[n].0);
                }
            }
            let (new_map, close, thumbs, play) = ctx.input(|i| {
                let cmd = i.modifiers.command;
                (
                    cmd && i.key_pressed(new_key),
                    cmd && i.key_pressed(close_key),
                    cmd && i.key_pressed(thumbs_key),
                    i.key_pressed(egui::Key::F5),
                )
            });
            let (find, check, preview, history, options) = ctx.input(|i| {
                let cmd = i.modifiers.command;
                (
                    cmd && i.key_pressed(find_key),
                    cmd && i.key_pressed(check_key),
                    cmd && i.key_pressed(preview_key),
                    cmd && i.key_pressed(history_key),
                    cmd && i.key_pressed(options_key),
                )
            });
            if find && local && self.current.is_some() {
                self.find.open = true;
            }
            if check && local && self.current.is_some() {
                self.check.open = true;
                self.run_check();
            }
            if preview && local && self.current.is_some() {
                self.preview.on = !self.preview.on;
            }
            if history && local {
                self.history_open = true;
            }
            if options {
                self.prefs_open = true;
            }
            if new_map {
                self.open_new_map_dialog();
            }
            if close && local {
                self.request_close();
            }
            if thumbs && self.game.is_some() {
                self.toggle_thumbnails(ctx);
            }
            if play && local {
                self.request_play();
            }
        }
        if typing || !self.edit.enabled || !local {
            return;
        }
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        if delete {
            self.delete_selected();
        }
        if rotate {
            match self.edit.tool {
                Tool::Stamp => self.cycle_stamp_variant(),
                Tool::Place => self.edit.place_orientation = (self.edit.place_orientation + 1) % 6,
                _ => self.rotate_selected(),
            }
        }
        if eyedrop
            && let Some(p) = ctx
                .input(|i| i.pointer.hover_pos())
                .filter(|p| self.canvas_rect.contains(*p))
        {
            let w = self.camera.screen_to_world(self.canvas_rect, p);
            self.eyedrop(w);
        }
        if duplicate {
            self.duplicate_selected();
        }
        if select_all {
            self.select_all();
        }
        if deselect || escape {
            self.deselect_all();
            self.edit.drag = None;
            self.edit.band = None;
            self.edit.exit.points.clear();
            self.edit.blocker_start = None;
        }
    }

    // ---------------------------------------------------------------- interaction avec le canevas

    /// Gère les gestes d'édition sur le canevas. Renvoie `true` si le bouton gauche est pris par un geste d'édition
    /// (le glisser ne doit alors pas déplacer la vue).
    pub(crate) fn canvas_edit_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        world: Option<Pos2>,
        scene: &Arc<Scene>,
    ) -> bool {
        let primary = egui::PointerButton::Primary;
        // Le glisser ne démarre qu'après quelques pixels de déplacement : c'est le point d'appui qui désigne l'objet.
        let press_world = ui
            .input(|i| i.pointer.press_origin())
            .map(|p| self.camera.screen_to_world(response.rect, p));
        // Clic droit sans glisser : annule l'outil en cours (le glisser droit déplace la vue).
        if response.clicked_by(egui::PointerButton::Secondary)
            && self.edit.tool == Tool::Exit
            && !self.edit.exit.points.is_empty()
        {
            self.edit.exit.points.clear();
            return false;
        }
        if response.clicked_by(egui::PointerButton::Secondary) && self.edit.tool != Tool::Select {
            self.edit.tool = Tool::Select;
            self.edit.drag = None;
            return false;
        }
        match self.edit.tool {
            Tool::Select => {
                let mods = ui.input(|i| i.modifiers);
                if response.drag_started_by(primary) {
                    self.edit.drag = None;
                    self.edit.band = None;
                    if let Some(w) = press_world {
                        match scene
                            .pick(self.layers, w.x as i32, w.y as i32)
                            .and_then(|i| i.object.map(|r| (r, i.tile)))
                        {
                            // Saisir un objet : s'il fait déjà partie de la sélection, tout le groupe suit ; sinon il devient la sélection.
                            Some((r, tile)) => {
                                if !self.selection.contains_object(r) {
                                    self.selection.set_object(Some(r));
                                }
                                self.edit.drag = Some(ObjectDrag { from_tile: tile });
                            }
                            // Sinon : rectangle de sélection.
                            None => self.edit.band = ui.input(|i| i.pointer.press_origin()),
                        }
                    }
                }
                if response.drag_stopped_by(primary) {
                    if let Some(d) = self.edit.drag.take()
                        && let Some(tile) = self.hover_hex
                        && tile != d.from_tile
                    {
                        self.move_selection(d.from_tile, tile);
                    }
                    if let Some(start) = self.edit.band.take()
                        && let Some(end) = ui.input(|i| i.pointer.latest_pos())
                    {
                        self.band_select(start, end, mods);
                    }
                }
                if response.clicked_by(primary) {
                    self.click_select(scene, world, mods);
                }
                self.edit.drag.is_some() || self.edit.band.is_some()
            }
            Tool::Floor | Tool::Roof => {
                // Maj pendant la peinture : on peint sur le toit (comme Gecko).
                let shift = ui.input(|i| i.modifiers.shift);
                let layer = if self.edit.tool == Tool::Roof || shift {
                    TileLayer::Roof
                } else {
                    TileLayer::Floor
                };
                let down =
                    ui.input(|i| i.pointer.primary_down()) && response.is_pointer_button_down_on();
                if down && let Some(w) = world {
                    self.paint_at(w, layer);
                }
                if !down {
                    self.end_stroke();
                }
                down
            }
            Tool::Exit => {
                self.exit_input(ui, response);
                false
            }
            Tool::Blocker => self.blocker_input(ui, response),
            Tool::Stamp => {
                self.stamp_input(ui, response);
                false
            }
            Tool::Room => {
                self.room_input(ui, response);
                false
            }
            Tool::Wall => {
                self.wall_input(response);
                false
            }
            Tool::Spatial => {
                self.spatial_input(response);
                false
            }
            Tool::Place => {
                if response.clicked_by(primary)
                    && let (Some(tile), Some(proto)) = (self.hover_hex, self.edit.place.clone())
                {
                    self.place_object(&proto, tile);
                }
                false
            }
            Tool::PlayerStart => {
                if response.clicked_by(primary)
                    && let Some(tile) = self.hover_hex
                {
                    let e = self.elevation;
                    self.mutate_as("Point de départ", |m, _| {
                        m.set_player_start(tile, e);
                        true
                    });
                }
                false
            }
        }
    }

    fn place_object(&mut self, proto: &ProtoEntry, tile: u32) {
        let e = self.elevation;
        let orientation = self.edit.place_orientation;
        let mut index = None;
        let ok = self.mutate_as("Placement d'un objet", |m, _| {
            let id = m.max_object_id() + 1;
            let Some(mut obj) = MapObject::from_proto(&proto.info, tile, id) else {
                return false;
            };
            obj.words[word::ORIENTATION] = orientation as i32;
            index = Some(m.add_object(e, obj));
            true
        });
        if ok && let Some(i) = index {
            // Un prototype qui porte un script le garde : l'objet reçoit une entrée dans la liste de son type.
            if proto.info.script_id != -1 {
                let program = (proto.info.script_id as u32) & 0x00FF_FFFF;
                self.mutate_as("Script attaché", |m, _| {
                    m.attach_object_script(e, i, program)
                });
            }
            self.selection.set_object(Some(fo_scene::ObjectRef {
                elevation: e as u8,
                index: i as u32,
            }));
        } else if !ok {
            self.status = crate::i18n::t("Ce type d'objet ne peut pas être créé depuis un prototype (dupliquez un objet existant).").into();
        }
    }

    fn paint_at(&mut self, world: Pos2, layer: TileLayer) {
        let dy = if layer == TileLayer::Roof {
            -geom::ROOF_OFFSET_Y
        } else {
            0
        };
        let Some(square) = geom::screen_to_square(world.x as i32, world.y as i32 + dy) else {
            return;
        };
        let (e, id) = (self.elevation, self.edit.brush_tile);
        let Some(cur) = self.current.as_mut() else {
            return;
        };
        if cur.map.tile(e, square as usize).is_none() {
            return;
        }
        let existing = cur.map.tile(e, square as usize).map(|t| {
            if layer == TileLayer::Roof {
                t.roof
            } else {
                t.floor
            }
        });
        if existing == Some(id) {
            return;
        }
        if !self.edit.stroke {
            let snapshot = (*cur.map).clone();
            self.edit.push_undo(snapshot, "Peinture de tuiles");
            self.edit.stroke = true;
            self.edit.stroke_changed = false;
        }
        Arc::make_mut(&mut cur.map).set_tile(e, square as usize, layer, id);
        self.edit.stroke_changed = true;
        self.edit.dirty = true;
        self.edit.last_tag = None;
        if self.edit.last_rebuild.elapsed() > REBUILD_INTERVAL {
            self.rebuild_scene(e);
        }
    }

    fn end_stroke(&mut self) {
        if !self.edit.stroke {
            return;
        }
        self.edit.stroke = false;
        if self.edit.stroke_changed {
            self.rebuild_scene(self.elevation);
        } else {
            self.edit.undo.pop();
        }
    }

    /// Aperçus et repères d'édition par-dessus la carte.
    pub(crate) fn draw_edit_overlay(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        view: Rect,
        scene: &Arc<Scene>,
    ) {
        let cam = self.camera;
        let to_screen = |p: Pos2| cam.world_to_screen(view, p);
        let outline = |painter: &egui::Painter, pts: &[Pos2], color: Color32| {
            painter.add(egui::Shape::closed_line(
                pts.iter().map(|&p| to_screen(p)).collect(),
                Stroke::new(1.5, color),
            ));
        };
        if let Some(start) = self.edit.band
            && let Some(end) = ctx.input(|i| i.pointer.latest_pos())
        {
            let iso = crate::select_ops::IsoRect::new(
                cam.screen_to_world(view, start),
                cam.screen_to_world(view, end),
            );
            let pts: Vec<Pos2> = iso.corners().iter().map(|&p| to_screen(p)).collect();
            painter.add(egui::Shape::convex_polygon(
                pts.clone(),
                Color32::from_rgba_unmultiplied(120, 180, 255, 36),
                Stroke::new(1.0, Color32::from_rgb(120, 180, 255)),
            ));
        }
        self.draw_exit_overlay(painter, view, scene);
        let Some(hex) = self.hover_hex else {
            return;
        };
        let (hx, hy) = geom::hex_to_screen(hex);
        let hc = Pos2::new(hx as f32, hy as f32);
        match self.edit.tool {
            Tool::Select => {
                if let Some(d) = &self.edit.drag {
                    let (fx, fy) = geom::hex_to_screen(d.from_tile);
                    let shift = Vec2::new(hx as f32 - fx as f32, hy as f32 - fy as f32);
                    let selected: std::collections::HashSet<fo_scene::ObjectRef> =
                        self.selection.objects().iter().copied().collect();
                    for item in scene
                        .items
                        .iter()
                        .filter(|i| i.object.is_some_and(|o| selected.contains(&o)))
                    {
                        // Chaque objet du groupe se déplace de la même translation d'écran que l'objet saisi.
                        self.draw_item_shifted(ctx, painter, view, item, shift);
                    }
                    outline(painter, &hex_diamond(hc), Color32::from_rgb(255, 200, 80));
                }
            }
            Tool::Place => {
                outline(painter, &hex_diamond(hc), Color32::from_rgb(120, 220, 255));
                if let Some(p) = self.edit.place.clone()
                    && let Some(g) = &self.game
                    && let Some((_, sprite)) =
                        g.art
                            .object_sprite(Fid(p.info.fid), self.edit.place_orientation, 0)
                {
                    let rect = Rect::from_min_size(
                        Pos2::new(hc.x + sprite.origin_x as f32, hc.y + sprite.origin_y as f32),
                        Vec2::new(sprite.width as f32, sprite.height as f32),
                    );
                    let tex = self.texture_for(ctx, &sprite);
                    painter.image(
                        tex,
                        Rect::from_min_max(to_screen(rect.min), to_screen(rect.max)),
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::from_white_alpha(170),
                    );
                }
            }
            Tool::PlayerStart => {
                outline(painter, &hex_diamond(hc), Color32::from_rgb(80, 255, 120))
            }
            Tool::Stamp => self.draw_stamp_overlay(ctx, painter, view, hex),
            Tool::Room => self.draw_room_overlay(ctx, painter, view, hex),
            Tool::Wall => self.draw_wall_overlay(painter, view, hex),
            Tool::Spatial => self.draw_spatial_overlay(painter, view, hex),
            Tool::Floor | Tool::Roof | Tool::Exit | Tool::Blocker => {}
        }
        if matches!(self.edit.tool, Tool::Floor | Tool::Roof)
            && let Some(w) = response_world(ctx, &cam, view)
        {
            let dy = if self.edit.tool == Tool::Roof {
                -geom::ROOF_OFFSET_Y
            } else {
                0
            };
            if let Some(sq) = geom::screen_to_square(w.x as i32, w.y as i32 + dy) {
                let (x, y) = geom::square_to_screen(sq);
                let y = y as f32 - dy as f32;
                let x = x as f32;
                outline(
                    painter,
                    &[
                        Pos2::new(x + 40.0, y),
                        Pos2::new(x + 80.0, y + 18.0),
                        Pos2::new(x + 40.0, y + 36.0),
                        Pos2::new(x, y + 18.0),
                    ],
                    Color32::from_rgb(255, 200, 80),
                );
            }
        }
    }

    fn draw_item_shifted(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        view: Rect,
        item: &DrawItem,
        shift: Vec2,
    ) {
        let min = Pos2::new(item.x as f32, item.y as f32) + shift;
        let max = min + Vec2::new(item.sprite.width as f32, item.sprite.height as f32);
        let tex = self.texture_for(ctx, &item.sprite);
        painter.image(
            tex,
            Rect::from_min_max(
                self.camera.world_to_screen(view, min),
                self.camera.world_to_screen(view, max),
            ),
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::from_white_alpha(150),
        );
    }

    /// Texture d'un sprite (créée à la demande et partagée avec le rendu de la carte).
    pub(crate) fn texture_for(&mut self, ctx: &egui::Context, sprite: &Sprite) -> egui::TextureId {
        self.textures
            .entry(sprite.id)
            .or_insert_with(|| {
                let img = egui::ColorImage::from_rgba_unmultiplied(
                    [sprite.width as usize, sprite.height as usize],
                    &sprite.rgba,
                );
                ctx.load_texture(format!("sprite{}", sprite.id), img, TextureOptions::NEAREST)
            })
            .id()
    }

    // ---------------------------------------------------------------- panneau d'édition

    pub(crate) fn edit_panel(&mut self, ui: &mut egui::Ui) {
        let tool_label = Tool::ALL
            .iter()
            .find(|(t, _)| *t == self.edit.tool)
            .map_or("", |(_, l)| *l);
        ui.heading(format!(
            "{} — {}",
            crate::i18n::t("Édition"),
            crate::i18n::t(tool_label)
        ));
        // Les outils se choisissent dans la barre flottante de la carte ; annuler / rétablir à parts égales.
        let gap = ui.spacing().item_spacing.x;
        let height = ui.spacing().interact_size.y.max(28.0);
        ui.separator();
        let half = ((ui.available_width() - gap) / 2.0).floor();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.edit.can_undo(),
                    egui::Button::new(crate::i18n::t("↶  Annuler"))
                        .min_size(egui::Vec2::new(half, height)),
                )
                .clicked()
            {
                self.undo();
            }
            if ui
                .add_enabled(
                    self.edit.can_redo(),
                    egui::Button::new(crate::i18n::t("↷  Rétablir"))
                        .min_size(egui::Vec2::new(half, height)),
                )
                .clicked()
            {
                self.redo();
            }
        });
        ui.separator();
        match self.edit.tool {
            Tool::Select => self.selection_editor(ui),
            Tool::Floor | Tool::Roof => self.tile_palette(ui),
            Tool::Place => self.place_palette(ui),
            Tool::Exit => self.exit_panel(ui),
            Tool::Stamp => self.stamp_panel(ui),
            Tool::Room => self.room_panel(ui),
            Tool::Wall => self.wall_panel(ui),
            Tool::Spatial => self.spatial_panel(ui),
            Tool::Blocker => {
                ui.label(crate::i18n::t("Glissez un rectangle sur la carte : des bloqueurs de défilement sont posés sur son contour."));
            }
            Tool::PlayerStart => {
                ui.label(crate::i18n::t("Cliquez sur la carte pour placer le point de départ du joueur (élévation courante)."));
            }
        }
        ui.separator();
        self.header_editor(ui);
    }

    fn header_editor(&mut self, ui: &mut egui::Ui) {
        let Some(cur) = &self.current else { return };
        let mut darkness = cur.map.header.darkness;
        ui.collapsing(crate::i18n::t("En-tête de la carte"), |ui| {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("Obscurité"));
                ui.add(crate::stepper::Stepper::new(&mut darkness).range(0..=100_000));
            });
        });
        self.edge_editor(ui);
        self.map_scripts_editor(ui);
        let Some(cur) = &self.current else { return };
        if darkness != cur.map.header.darkness {
            self.mutate(Some("darkness"), |m, _| {
                m.header.darkness = darkness;
                true
            });
        }
    }

    /// Filtre de sélection : quels éléments un clic ou un rectangle peuvent désigner.
    fn pick_filter(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (mode, label) in [(SelectMode::Normal, "Éléments"), (SelectMode::Hexes, "Hexagones"), (SelectMode::Blockers, "Bloqueurs de défilement")] {
                if ui.selectable_label(self.edit.select_mode == mode, crate::i18n::t(label)).on_hover_text(crate::i18n::t("Mode de sélection : « Hexagones » et « Bloqueurs de défilement » ne désignent que ça.")).clicked() {
                    self.edit.select_mode = mode;
                    self.selection.clear();
                    self.edit.last_click = None;
                }
            }
        });
        if self.edit.select_mode != SelectMode::Normal {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(crate::i18n::t("Sélectionner :"));
            ui.checkbox(&mut self.edit.pick_objects, crate::i18n::t("Objets"));
            ui.checkbox(&mut self.edit.pick_floor, crate::i18n::t("Sols"));
            ui.checkbox(&mut self.edit.pick_roof, crate::i18n::t("Toits"));
        });
    }

    fn selection_editor(&mut self, ui: &mut egui::Ui) {
        if self.selection.primary().is_some()
            && ui
                .button(crate::i18n::t("Modifier le bâtiment sélectionné"))
                .clicked()
        {
            self.begin_edit_building();
        }
        self.pick_filter(ui);
        self.stretch_section(ui);
        let (n_objects, n_tiles) = (self.selection.objects().len(), self.selection.tiles().len());
        let n_hexes = self.selection.hexes().len();
        if n_hexes > 0 {
            ui.separator();
            ui.label(format!(
                "{n_hexes} {}",
                crate::i18n::t("hexagone(s) sélectionné(s)")
            ));
            if ui
                .button(crate::i18n::t("Supprimer les objets sur ces hexagones"))
                .clicked()
            {
                self.delete_objects_on_selected_hexes();
            }
        }
        if n_objects == 0 && n_tiles == 0 && n_hexes == 0 {
            ui.label(crate::i18n::t(
                "Clic : sélectionner (re-clic au même endroit : élément suivant sous le curseur).",
            ));
            ui.label(crate::i18n::t("Glisser sur le vide : sélection par zone ; Alt ajoute, Ctrl retire, Maj étend une plage de cases."));
            ui.label(crate::i18n::t(
                "Glisser un objet : le déplacer (avec tout le groupe sélectionné).",
            ));
            ui.label(crate::i18n::t("Suppr · R tourner · Ctrl+J dupliquer · Ctrl+A tout · Ctrl+D / Échap désélectionner."));
            return;
        }
        if n_tiles > 0 {
            ui.separator();
            ui.label(format!(
                "{n_tiles} {}",
                crate::i18n::t("case(s) sélectionnée(s)")
            ));
            ui.horizontal(|ui| {
                let brush = self.edit.brush_tile;
                if ui
                    .button(format!(
                        "{} ({} {brush})",
                        crate::i18n::t("Remplir"),
                        crate::i18n::t("tuile n°")
                    ))
                    .clicked()
                {
                    self.fill_selected_tiles(Some(brush));
                }
                if ui.button(crate::i18n::t("Vider")).clicked() {
                    self.fill_selected_tiles(None);
                }
            });
        }
        if n_objects > 1 {
            ui.separator();
            ui.label(format!(
                "{n_objects} {}",
                crate::i18n::t("objets sélectionnés")
            ));
            ui.horizontal(|ui| {
                if ui.button(crate::i18n::t("Tourner")).clicked() {
                    self.rotate_selected();
                }
                if ui.button(crate::i18n::t("Dupliquer")).clicked() {
                    self.duplicate_selected();
                }
                if ui.button(crate::i18n::t("Supprimer")).clicked() {
                    self.delete_selected();
                }
            });
            if ui.button(crate::i18n::t("Tout désélectionner")).clicked() {
                self.deselect_all();
            }
            return;
        }
        if n_objects == 0 {
            if ui.button(crate::i18n::t("Supprimer les cases")).clicked() {
                self.delete_selected();
            }
            return;
        }
        ui.separator();
        let Some(r) = self.selection.primary() else {
            return;
        };
        self.exit_properties(ui, r);
        let Some(o) = self.object(r).cloned() else {
            return;
        };
        let (e, i) = (r.elevation as usize, r.index as usize);
        let (mut orientation, mut frame, mut tile) = (o.orientation(), o.frame(), o.tile());
        let (mut ox, mut oy) = o.offset();
        let (mut hidden, mut flat, mut no_block) = (
            o.has_flag(flags::HIDDEN),
            o.has_flag(flags::FLAT),
            o.has_flag(flags::NO_BLOCK),
        );

        let kind = o.object_type().map_or("?", |t| crate::i18n::t(t.label()));
        match self.proto_name(o.pid()) {
            Some(name) => ui
                .label(egui::RichText::new(name).strong())
                .on_hover_text(format!("{kind} — PID {:08X}", o.pid())),
            None => ui.label(format!("{kind} — PID {:08X}", o.pid())),
        };
        let mut changed = false;
        egui::Grid::new("edit_object")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label(crate::i18n::t("Hexagone"));
                changed |= ui
                    .add(
                        crate::stepper::Stepper::new(&mut tile)
                            .range(0..=(geom::HEX_COUNT as i32 - 1)),
                    )
                    .changed();
                ui.end_row();
                ui.label(crate::i18n::t("Orientation"));
                changed |= ui
                    .add(crate::stepper::Stepper::new(&mut orientation).range(0..=5))
                    .changed();
                ui.end_row();
                ui.label(crate::i18n::t("Image"));
                changed |= ui
                    .add(crate::stepper::Stepper::new(&mut frame).range(0..=63))
                    .changed();
                ui.end_row();
                // Un champ par ligne : deux champs côte à côte dépassent la largeur du panneau et le décalent.
                ui.label(crate::i18n::t("Décalage X"));
                changed |= ui.add(crate::stepper::Stepper::new(&mut ox)).changed();
                ui.end_row();
                ui.label(crate::i18n::t("Décalage Y"));
                changed |= ui.add(crate::stepper::Stepper::new(&mut oy)).changed();
                ui.end_row();
            });
        changed |= ui.checkbox(&mut hidden, crate::i18n::t("Masqué")).changed();
        changed |= ui
            .checkbox(
                &mut flat,
                crate::i18n::t("À plat (dessiné sous les autres)"),
            )
            .changed();
        changed |= ui
            .checkbox(&mut no_block, crate::i18n::t("Ne bloque pas le passage"))
            .changed();

        if changed {
            self.mutate(Some("object"), |m, _| {
                let Some(o) = m.objects.get_mut(e).and_then(|v| v.get_mut(i)) else {
                    return false;
                };
                o.words[word::TILE] = tile;
                o.words[word::ORIENTATION] = orientation;
                o.words[word::FRAME] = frame;
                o.words[word::OFFSET_X] = ox;
                o.words[word::OFFSET_Y] = oy;
                o.set_flag(flags::HIDDEN, hidden);
                o.set_flag(flags::FLAT, flat);
                o.set_flag(flags::NO_BLOCK, no_block);
                true
            });
        }
        self.flags_editor(ui, r);
        self.object_script_section(ui, r);
        self.inventory_editor(ui, r);
        ui.horizontal(|ui| {
            if ui.button(crate::i18n::t("Supprimer")).clicked() {
                self.delete_selected();
            }
            if ui.button(crate::i18n::t("Dupliquer")).clicked() {
                self.duplicate_selected();
            }
        });
    }

    /// Accès rapide à la taille des miniatures et à l'affichage des noms (mêmes réglages que les Options).
    fn thumb_options(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("thumb_size_quick")
                .selected_text(format!("{0}×{0}", self.prefs.thumb_size))
                .width(70.0)
                .show_ui(ui, |ui| {
                    for s in crate::prefs::THUMB_SIZES {
                        ui.selectable_value(&mut self.prefs.thumb_size, s, format!("{s}×{s}"));
                    }
                });
            ui.checkbox(&mut self.prefs.show_names, crate::i18n::t("Noms"));
        });
    }

    fn tile_palette(&mut self, ui: &mut egui::Ui) {
        let Some(g) = &self.game else { return };
        if self.edit.tile_names.is_none() {
            self.edit.tile_names = Some(g.data.list("art/tiles/tiles.lst"));
        }
        let names = self.edit.tile_names.clone().unwrap_or_default();
        let art = g.art.clone();
        let ctx = ui.ctx().clone();
        let help = if self.edit.tool == Tool::Roof {
            crate::i18n::t("Peinture des toits : glissez sur la carte. Tuile choisie : n°")
        } else {
            crate::i18n::t("Peinture des sols : glissez sur la carte. Tuile choisie : n°")
        };
        ui.label(format!("{help} {}", self.edit.brush_tile));
        if ui.button(crate::i18n::t("Gomme (tuile vide)")).clicked() {
            self.edit.brush_tile = 1;
        }
        ui.add(
            egui::TextEdit::singleline(&mut self.edit.tile_filter)
                .hint_text(crate::i18n::t("Filtrer…"))
                .desired_width(f32::INFINITY),
        );
        self.thumb_options(ui);
        self.tile_tools(ui, &names);
        let needle = self.edit.tile_filter.to_lowercase();
        let family = self.analysis.family.clone();
        let mut rows: Vec<usize> = (2..names.len())
            .filter(|&i| {
                (needle.is_empty() || names[i].to_lowercase().contains(&needle))
                    && (family.is_empty() || fo_core::tile_stats::family_of(&names[i]) == family)
            })
            .collect();
        // Les tuiles déjà utilisées sur la carte (à l'élévation courante) passent en premier.
        let used: std::collections::HashSet<usize> = self
            .current
            .as_ref()
            .map(|c| {
                (0..10_000)
                    .filter_map(|s| c.map.tile(self.elevation, s))
                    .flat_map(|t| [t.floor as usize, t.roof as usize])
                    .collect()
            })
            .unwrap_or_default();
        let layer = if self.edit.tool == Tool::Roof {
            fo_core::map::TileLayer::Roof
        } else {
            fo_core::map::TileLayer::Floor
        };
        let suggested: std::collections::HashMap<usize, u32> = match self.analysis.stats(layer) {
            Some(stats) if self.analysis.suggest => stats
                .neighbors_of(self.edit.brush_tile)
                .into_iter()
                .map(|(id, n)| (id as usize, n))
                .collect(),
            _ => Default::default(),
        };
        // Voisines probables de la tuile choisie d'abord (les plus fréquentes en tête), puis les tuiles déjà sur la carte.
        rows.sort_by_key(|id| {
            (
                std::cmp::Reverse(suggested.get(id).copied().unwrap_or(0)),
                !used.contains(id),
            )
        });
        let (size, names_on) = (self.prefs.thumb_size as f32, self.prefs.show_names);
        let col_w = if names_on { size + 110.0 } else { size + 8.0 };
        let per_row = ((ui.available_width() / col_w).floor() as usize).max(1);
        let row_h = size + 8.0;
        let line_count = rows.len().div_ceil(per_row);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row_h, line_count, |ui, range| {
                for line in range {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for &id in rows.iter().skip(line * per_row).take(per_row) {
                            let Some(sprite) = art.tile_sprite(id) else {
                                continue;
                            };
                            let tex = self.texture_for(&ctx, &sprite);
                            let selected = self.edit.brush_tile as usize == id;
                            let label = format!("{id} {}", names[id]);
                            let clicked = thumb_cell(
                                ui,
                                tex,
                                (sprite.width as f32, sprite.height as f32),
                                size,
                                selected,
                                names_on.then_some(label.as_str()),
                                &label,
                                col_w - 6.0,
                            );
                            if clicked {
                                self.edit.brush_tile = id as u16;
                            }
                        }
                    });
                }
            });
    }

    fn place_palette(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for ty in [
                ObjectType::Scenery,
                ObjectType::Wall,
                ObjectType::Item,
                ObjectType::Critter,
                ObjectType::Misc,
            ] {
                if ui
                    .selectable_label(self.edit.place_type == ty, crate::i18n::t(ty.label()))
                    .clicked()
                {
                    self.edit.place_type = ty;
                }
            }
        });
        let ty = self.edit.place_type;
        self.ensure_catalog(ty, ui.ctx());
        let Some(CatalogState::Ready(catalog)) = self.edit.catalogs.get(&ty) else {
            ui.label(crate::i18n::t("Chargement des prototypes…"));
            return;
        };
        let catalog = catalog.clone();
        let Some(g) = &self.game else { return };
        let art = g.art.clone();
        let ctx = ui.ctx().clone();
        match &self.edit.place {
            Some(p) => ui.label(format!(
                "{} : {} ({})",
                crate::i18n::t("Choisi"),
                if p.name.is_empty() { &p.file } else { &p.name },
                p.file
            )),
            None => ui.label(crate::i18n::t(
                "Choisissez un prototype, puis cliquez sur la carte.",
            )),
        };
        ui.add(
            egui::TextEdit::singleline(&mut self.edit.proto_filter)
                .hint_text(crate::i18n::t("Filtrer…"))
                .desired_width(f32::INFINITY),
        );
        self.thumb_options(ui);
        let needle = self.edit.proto_filter.to_lowercase();
        let rows: Vec<&ProtoEntry> = catalog
            .iter()
            .filter(|e| {
                needle.is_empty()
                    || e.name.to_lowercase().contains(&needle)
                    || e.file.to_lowercase().contains(&needle)
            })
            .collect();
        let (size, names_on) = (self.prefs.thumb_size as f32, self.prefs.show_names);
        let col_w = if names_on { size + 110.0 } else { size + 8.0 };
        let per_row = ((ui.available_width() / col_w).floor() as usize).max(1);
        let row_h = size + 8.0;
        let line_count = rows.len().div_ceil(per_row);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row_h, line_count, |ui, range| {
                for line in range {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for entry in rows.iter().skip(line * per_row).take(per_row) {
                            let label = if entry.name.is_empty() {
                                entry.file.clone()
                            } else {
                                format!("{}  ({})", entry.name, entry.file)
                            };
                            let selected =
                                self.edit.place.as_ref().is_some_and(|p| p.pid == entry.pid);
                            let clicked = match art.object_sprite(Fid(entry.info.fid), 0, 0) {
                                Some((_, sprite)) => {
                                    let tex = self.texture_for(&ctx, &sprite);
                                    thumb_cell(
                                        ui,
                                        tex,
                                        (sprite.width as f32, sprite.height as f32),
                                        size,
                                        selected,
                                        names_on.then_some(label.as_str()),
                                        &label,
                                        col_w - 6.0,
                                    )
                                }
                                None => ui.selectable_label(selected, &label).clicked(),
                            };
                            if clicked {
                                self.edit.place = Some((*entry).clone());
                            }
                        }
                    });
                }
            });
    }

    pub(crate) fn ensure_catalog(&mut self, ty: ObjectType, ctx: &egui::Context) {
        let Some(g) = &self.game else { return };
        match self.edit.catalogs.get(&ty) {
            Some(CatalogState::Ready(_)) => {}
            Some(CatalogState::Loading(rx)) => {
                if let Ok(list) = rx.try_recv() {
                    self.edit
                        .catalogs
                        .insert(ty, CatalogState::Ready(Arc::new(list)));
                } else {
                    ctx.request_repaint_after(Duration::from_millis(100));
                }
            }
            None => {
                let (tx, rx) = mpsc::channel();
                let data = g.data.clone();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(data.proto_catalog(ty));
                    ctx.request_repaint();
                });
                self.edit.catalogs.insert(ty, CatalogState::Loading(rx));
            }
        }
    }

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

/// Une vignette carrée (l'image tient dans `size`×`size`), avec ou sans nom à droite. Renvoie vrai si on a cliqué dessus.
fn thumb_cell(
    ui: &mut egui::Ui,
    tex: egui::TextureId,
    dims: (f32, f32),
    size: f32,
    selected: bool,
    name: Option<&str>,
    tooltip: &str,
    col_w: f32,
) -> bool {
    let (w, h) = dims;
    let scale = (size / w.max(1.0))
        .min(size / h.max(1.0))
        .min(1.0_f32.max(size / 64.0));
    let img = Vec2::new(w * scale, h * scale);
    let cell = Vec2::new(size + 6.0, size + 6.0);
    let total = match name {
        Some(_) => Vec2::new(col_w.max(cell.x), cell.y),
        None => cell,
    };
    let (rect, response) = ui.allocate_exact_size(total, egui::Sense::click());
    let visuals = ui.visuals();
    if selected {
        ui.painter()
            .rect_filled(rect, 4.0, visuals.selection.bg_fill);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, 4.0, visuals.widgets.hovered.weak_bg_fill);
    }
    let image_rect =
        Rect::from_center_size(Pos2::new(rect.min.x + cell.x / 2.0, rect.center().y), img);
    ui.painter().image(
        tex,
        image_rect,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
    if let Some(n) = name {
        let color = if selected {
            visuals.selection.stroke.color
        } else {
            visuals.text_color()
        };
        ui.painter().with_clip_rect(rect).text(
            Pos2::new(rect.min.x + cell.x + 4.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            n,
            egui::TextStyle::Body.resolve(ui.style()),
            color,
        );
    }
    response.on_hover_text(tooltip).clicked()
}

/// Libellé lisible d'une étape de l'historique d'après l'étiquette de la modification.
fn history_label(tag: &'static str) -> &'static str {
    match tag {
        "object" => "Propriétés d'un objet",
        "darkness" => "Obscurité de la carte",
        "paint" => "Peinture de tuiles",
        "exit" => "Propriétés d'une sortie",
        "edge" => "Limites de défilement",
        "flags" => "Drapeaux d'un objet",
        "inventory" => "Inventaire d'un objet",
        "spatial" => "Script spatial",
        other => other,
    }
}

fn hex_diamond(c: Pos2) -> [Pos2; 6] {
    HEX_OUTLINE.map(|(dx, dy)| Pos2::new(c.x + dx, c.y + dy))
}

/// Position du pointeur dans le monde, si elle est dans la vue.
fn response_world(ctx: &egui::Context, cam: &crate::view::Camera, view: Rect) -> Option<Pos2> {
    let p = ctx.input(|i| i.pointer.hover_pos())?;
    view.contains(p).then(|| cam.screen_to_world(view, p))
}
