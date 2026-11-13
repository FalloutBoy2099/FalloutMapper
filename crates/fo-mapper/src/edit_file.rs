//! Édition de la carte : outils, annulation / rétablissement, palettes, enregistrement.

use crate::app::{Confirm, MapperApp};
use eframe::egui::{self, Vec2};
use fo_core::write_map;


impl MapperApp {
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
}
