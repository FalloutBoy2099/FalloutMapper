use eframe::egui::{self};
use fo_scene::Layer;

use crate::app::*;

impl MapperApp {
    /// Entrée de menu avec le raccourci réel (celui des options) affiché à droite.
    pub(crate) fn menu_item(
        &self,
        text: &'static str,
        action: Option<crate::prefs::KeyAction>,
        enabled: bool,
    ) -> egui::Button<'static> {
        let _ = enabled;
        let button = egui::Button::new(crate::i18n::t(text));
        match action {
            Some(a) => button.shortcut_text(self.prefs.shortcut_text(a)),
            None => button,
        }
    }

    pub(crate) fn menu_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Les fonctions d'une carte locale (enregistrer, éditer, vérifier, calques…) n'ont de sens que dans l'onglet Carte.
        let local = self.mode == Mode::Map;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(crate::i18n::t("Fichier"), |ui| {
                if ui.button(crate::i18n::t("Ouvrir le dossier du jeu…")).clicked() {
                    ui.close();
                    self.open_dialog(crate::file_dialog::Purpose::OpenGame);
                }
                if ui.button("Installations et profils de test…").clicked() {
                    ui.close();
                    self.profiles.open = true;
                }
                ui.separator();
                if ui.add_enabled(self.game.is_some(), egui::Button::new(crate::i18n::t("Générer une carte…"))).clicked() {
                    ui.close();
                    self.open_generator();
                }
                if ui.add_enabled(self.game.is_some(), self.menu_item("Nouvelle carte…", Some(crate::prefs::KeyAction::NewMap), true)).clicked() {
                    ui.close();
                    self.open_new_map_dialog();
                }
                if ui.add_enabled(self.game.is_some(), self.menu_item("Cartes en miniatures", Some(crate::prefs::KeyAction::Thumbnails), true)).clicked() {
                    ui.close();
                    self.toggle_thumbnails(ctx);
                }
                if ui.add_enabled(local && self.current.is_some(), self.menu_item("Fermer la carte", Some(crate::prefs::KeyAction::CloseMap), true)).clicked() {
                    ui.close();
                    self.request_close();
                }
                ui.separator();
                let has_map = self.current.is_some() && local;
                if ui.add_enabled(has_map, self.menu_item("Enregistrer", Some(crate::prefs::KeyAction::Save), true)).clicked() {
                    ui.close();
                    self.save_current();
                }
                if ui.add_enabled(has_map, egui::Button::new(crate::i18n::t("Enregistrer sous…"))).clicked() {
                    ui.close();
                    self.save_current_as();
                }
                if ui.add_enabled(self.current.is_some() && self.scene().is_some(), egui::Button::new(crate::i18n::t("Exporter une image…"))).clicked() {
                    ui.close();
                    self.export.open = true;
                }
                ui.separator();
                if ui.add_enabled(has_map, egui::Button::new(crate::i18n::t("Enregistrer et jouer")).shortcut_text("F5")).clicked() {
                    ui.close();
                    self.request_play();
                }
                if ui.add_enabled(has_map && !self.fusion_pending, egui::Button::new(crate::i18n::t("Recharger dans Fallout Fusion"))).clicked() {
                    ui.close();
                    self.request_fusion_reload();
                }
                if ui.add_enabled(self.game.is_some(), egui::Button::new(crate::i18n::t("Restaurer la configuration de lancement"))).clicked() {
                    ui.close();
                    self.restore_ddraw();
                }
                ui.separator();
                if ui.add(self.menu_item("Options…", Some(crate::prefs::KeyAction::Options), true)).clicked() {
                    ui.close();
                    self.prefs_open = true;
                }
                ui.separator();
                if ui.button(crate::i18n::t("Quitter")).clicked() {
                    ui.close();
                    // Les modifications non enregistrées passent d'abord par la confirmation ; sinon on ferme tout de suite.
                    self.quit_requested();
                    if self.confirm.is_none() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            });
            ui.menu_button(crate::i18n::t("Édition"), |ui| {
                ui.add_enabled(local, egui::Checkbox::new(&mut self.edit.enabled, crate::i18n::t("Mode édition")));
                ui.separator();
                if ui.add_enabled(local && self.edit.can_undo(), self.menu_item("Annuler", Some(crate::prefs::KeyAction::Undo), true)).clicked() {
                    ui.close();
                    self.undo();
                }
                if ui.add_enabled(local && self.edit.can_redo(), self.menu_item("Rétablir", Some(crate::prefs::KeyAction::Redo), true)).clicked() {
                    ui.close();
                    self.redo();
                }
                if ui.add_enabled(local, self.menu_item("Historique…", Some(crate::prefs::KeyAction::History), true)).clicked() {
                    ui.close();
                    self.history_open = true;
                }
                if ui.add_enabled(local && self.current.is_some(), self.menu_item("Rechercher un objet…", Some(crate::prefs::KeyAction::Find), true)).clicked() {
                    ui.close();
                    self.find.open = true;
                }
                if ui.add_enabled(self.game.is_some(), egui::Button::new(crate::i18n::t("Bibliothèque de motifs…"))).clicked() {
                    ui.close();
                    self.library.open = true;
                }
                if ui.add_enabled(local && self.current.is_some(), egui::Button::new(crate::i18n::t("Comparer avec une version…"))).clicked() {
                    ui.close();
                    self.diff.open = true;
                }
                if ui.add_enabled(local && self.current.is_some(), self.menu_item("Vérifier la carte…", Some(crate::prefs::KeyAction::Check), true)).clicked() {
                    ui.close();
                    self.check.open = true;
                    self.run_check();
                }
                ui.separator();
                if ui.add_enabled(self.game.is_some() && !self.analysis.running(), egui::Button::new(crate::i18n::t("Analyser les tuiles de toutes les cartes"))).clicked() {
                    ui.close();
                    self.start_tile_analysis(ctx);
                }
                if ui.add_enabled(self.game.is_some(), egui::Button::new(crate::i18n::t("Textes (.msg)…"))).clicked() {
                    ui.close();
                    self.msg.open = true;
                }
                if ui.add_enabled(self.game.is_some(), egui::Button::new(crate::i18n::t("Cartes du jeu (maps.txt)…"))).clicked() {
                    ui.close();
                    self.open_maps_txt();
                }
                if ui.add_enabled(self.game.is_some(), egui::Button::new(crate::i18n::t("Carte du monde…"))).clicked() {
                    ui.close();
                    self.mode = Mode::World;
                }
                if ui.add_enabled(self.game.is_some(), egui::Button::new(crate::i18n::t("Liaisons entre cartes…"))).clicked() {ui.close();self.links.open=true;}
            });
            ui.menu_button(crate::i18n::t("Affichage"), |ui| {
                ui.add_enabled_ui(local, |ui| {
                ui.menu_button(crate::i18n::t("Calques"), |ui| {
                    for layer in Layer::ALL {
                        let mut on = self.layers.contains(layer);
                        if ui.checkbox(&mut on, crate::i18n::t(layer.label())).changed() {
                            self.layers.set(layer, on);
                        }
                    }
                });
                ui.separator();
                ui.checkbox(&mut self.show_markers, crate::i18n::t("Repères (sorties, scripts…)"));
                ui.checkbox(&mut self.minimap.on, crate::i18n::t("Minicarte"));
                ui.checkbox(&mut self.show_grid, crate::i18n::t("Grille hexagonale"));
                ui.checkbox(&mut self.show_edges, crate::i18n::t("Limites de défilement (.edg)"));
                ui.checkbox(&mut self.show_lights, crate::i18n::t("Éclairage (halos des lumières)"));
                ui.checkbox(&mut self.walk.on, crate::i18n::t("Accessibilité (hexagones bloqués et atteignables)")).on_hover_text(crate::i18n::t("Rouge : bloqué ; orange : créature ; vert : atteignable à pied depuis le départ du joueur ; bleu : libre mais hors d'atteinte."));
                ui.separator();
                self.preview_menu(ui);
                ui.separator();
                ui.checkbox(&mut self.anim_on, crate::i18n::t("Animer la palette (eau, feu, écrans…)"));
                ui.separator();
                ui.checkbox(&mut self.show_scripted, crate::i18n::t("Surligner les objets avec un script"));
                if ui.button(crate::i18n::t("Légende des repères…")).clicked() {
                    ui.close();
                    self.legend_open = true;
                }
                ui.separator();
                if ui.add_enabled(local, self.menu_item("Ajuster à la fenêtre", Some(crate::prefs::KeyAction::Fit), true)).clicked() {
                    self.needs_fit = true;
                    ui.close();
                }
                });
            });
            ui.menu_button(crate::i18n::t("Fenêtre"), |ui| {
                if ui.button(crate::i18n::t("Journal…")).clicked() {
                    ui.close();
                    self.log.open = true;
                }
                ui.separator();
                ui.checkbox(&mut self.show_left, crate::i18n::t("Panneau des cartes"));
                ui.checkbox(&mut self.show_right, crate::i18n::t("Panneau d'édition et d'infos"));
                ui.separator();
                ui.checkbox(&mut self.window.remember_size, crate::i18n::t("Mémoriser les dimensions"));
                ui.checkbox(&mut self.window.remember_pos, crate::i18n::t("Mémoriser la position"));
            });
            ui.separator();
            for (mode, icon, tip) in [(Mode::World, crate::icons::Icon::World, "Carte du monde"), (Mode::Map, crate::icons::Icon::Map, "Carte"), (Mode::Scripts, crate::icons::Icon::Script, "Scripts")] {
                let r = crate::icons::icon_button(ui, icon, egui::Vec2::new(36.0, 28.0), self.mode == mode, true);
                if r.on_hover_text(crate::i18n::t(tip)).clicked() {
                    self.mode = mode;
                }
            }
        });
    }
}
