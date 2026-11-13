//! Édition de la carte : outils, annulation / rétablissement, palettes, enregistrement.

use crate::app::MapperApp;
use eframe::egui::{self, Pos2};
use fo_core::map::{TileLayer, word};
use fo_core::{MapObject, ProtoEntry, geom};
use fo_scene::Scene;
use std::sync::Arc;

use crate::edit_ui::*;

impl MapperApp {
    /// Navigation : F ajuste la vue, Début centre sur le point de départ, Ctrl+1/2/3 changent d'élévation.
    pub(crate) fn navigation_shortcuts(&mut self, ctx: &egui::Context) {
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

    /// Verrouille ou déverrouille la sélection (bouton de la barre d'outils, panneau de sélection, raccourci).
    pub(crate) fn toggle_selection_lock(&mut self) {
        if self.edit.selection_locked {
            self.edit.selection_locked = false;
            self.status = crate::i18n::t("Sélection déverrouillée.").into();
        } else if self.selection.is_empty() {
            self.status = crate::i18n::t("Rien à verrouiller : sélectionnez d'abord un élément.").into();
        } else {
            self.edit.selection_locked = true;
            self.status = crate::i18n::t("Sélection verrouillée : un clic ou un rectangle ne la change plus.").into();
        }
    }

    /// Flèches du clavier : déplacent l'objet (ou le groupe) sélectionné d'un hexagone à l'écran (Maj : trois). Gauche et droite avancent
    /// de 32 pixels, haut et bas de 24 : ce sont les voisins en ligne droite de la grille d'hexagones.
    pub(crate) fn nudge_with_arrows(&mut self, ctx: &egui::Context) {
        if self.edit.tool != Tool::Select || self.preview.on {
            return;
        }
        let step = ctx.input(|i| {
            if i.modifiers.command || i.modifiers.alt {
                return None;
            }
            let n = if i.modifiers.shift { 3 } else { 1 };
            [
                (egui::Key::ArrowLeft, (-32, 0)),
                (egui::Key::ArrowRight, (32, 0)),
                (egui::Key::ArrowUp, (0, -24)),
                (egui::Key::ArrowDown, (0, 24)),
            ]
            .iter()
            .find(|(k, _)| i.key_pressed(*k))
            .map(|&(_, (dx, dy))| (dx * n, dy * n))
        });
        let Some((dx, dy)) = step else { return };
        let e = self.elevation;
        let indices = self.selection.object_indices(e);
        let from = self
            .selection
            .primary()
            .and_then(|r| self.object(r))
            .or_else(|| indices.first().and_then(|&i| self.current.as_ref()?.map.objects.get(e)?.get(i)))
            .map(|o| o.tile())
            .filter(|t| (0..geom::HEX_COUNT as i32).contains(t));
        let Some(from) = from else { return };
        let (x, y) = geom::hex_to_screen(from as u32);
        if let Some(to) = geom::screen_to_hex(x + dx, y + dy).filter(|&t| t != from as u32) {
            self.move_selection_tagged(from as u32, to, Some("nudge"));
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
        let lock_key = k(K::LockSelection);
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
        let lock = ctx.input(|i| !i.modifiers.command && !i.modifiers.alt && i.key_pressed(lock_key));
        self.nudge_with_arrows(ctx);
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
        if lock {
            self.toggle_selection_lock();
        }
        let locked = self.edit.selection_locked;
        if select_all && !locked {
            self.select_all();
        }
        if deselect || escape {
            if !locked {
                self.deselect_all();
            }
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
        // Une sélection vide n'a rien à garder : le verrou tombe (suppression, annulation…).
        if self.edit.selection_locked && self.selection.is_empty() {
            self.edit.selection_locked = false;
        }
        let locked = self.edit.selection_locked;
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
                            // Verrouillée, la sélection ne change pas : on ne peut saisir qu'un de ses objets.
                            Some((r, tile)) => {
                                let inside = self.selection.contains_object(r);
                                if !inside && !locked {
                                    self.selection.set_object(Some(r));
                                }
                                if inside || !locked {
                                    self.edit.drag = Some(ObjectDrag { from_tile: tile });
                                }
                            }
                            // Sinon : rectangle de sélection (pas quand elle est verrouillée).
                            None if !locked => self.edit.band = ui.input(|i| i.pointer.press_origin()),
                            None => {}
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
                if response.clicked_by(primary) && !locked {
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

    pub(crate) fn place_object(&mut self, proto: &ProtoEntry, tile: u32) {
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

    pub(crate) fn paint_at(&mut self, world: Pos2, layer: TileLayer) {
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

    pub(crate) fn end_stroke(&mut self) {
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
}
