//! Édition de la carte : outils, annulation / rétablissement, palettes, enregistrement.

use crate::app::MapperApp;
use eframe::egui::{self};
use fo_core::map::{flags, word};
use fo_core::geom;

use crate::edit_ui::*;

impl MapperApp {
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

    pub(crate) fn header_editor(&mut self, ui: &mut egui::Ui) {
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
    pub(crate) fn pick_filter(&mut self, ui: &mut egui::Ui) {
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

    pub(crate) fn selection_editor(&mut self, ui: &mut egui::Ui) {
        let has_selection = self.selection.primary().is_some() || !self.selection.is_empty();
        if has_selection || self.edit.selection_locked {
            let tip = format!("{} ({})", crate::i18n::t("Empêche de changer de sélection par erreur"), self.prefs.shortcut_text(crate::prefs::KeyAction::LockSelection));
            let mut locked = self.edit.selection_locked;
            if ui.toggle_value(&mut locked, crate::i18n::t("Verrouiller la sélection")).on_hover_text(tip).changed() {
                self.toggle_selection_lock();
            }
        }
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
}
