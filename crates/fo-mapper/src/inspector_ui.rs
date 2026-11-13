use eframe::egui::{self};
use fo_core::geom;
use fo_core::map::{MapObject, SCRIPT_TYPE_NAMES};
use fo_core::Fid;

use crate::app::*;

impl MapperApp {
    /// Panneau de droite en mode carte. Renvoie le script à ouvrir si l'utilisateur le demande.
    pub(crate) fn inspector(&self, ui: &mut egui::Ui) -> Option<String> {
        let Some(cur) = &self.current else {
            ui.label(crate::i18n::t("Aucune carte."));
            return None;
        };
        let h = &cur.map.header;
        ui.heading(h.name());
        egui::Grid::new("header").num_columns(2).show(ui, |ui| {
            let mut row = |k: &'static str, v: String| {
                ui.label(crate::i18n::t(k));
                ui.label(v);
                ui.end_row();
            };
            row("Version", h.version.to_string());
            row("Numéro", h.map_number.to_string());
            row("Obscurité", h.darkness.to_string());
            row("Script de carte", h.script_id.to_string());
            row(
                "Départ",
                format!(
                    "{} {} ({} {})",
                    crate::i18n::t("hex"),
                    h.player_tile,
                    crate::i18n::t("élév."),
                    h.player_elevation
                ),
            );
            row(
                "Variables",
                format!(
                    "{} {} / {} {}",
                    cur.map.globals.len(),
                    crate::i18n::t("glob."),
                    cur.map.locals.len(),
                    crate::i18n::t("loc.")
                ),
            );
            row(
                "Objets",
                format!(
                    "{} ({} {})",
                    cur.map.object_count(),
                    cur.map.total_object_count(),
                    crate::i18n::t("avec inventaires")
                ),
            );
        });
        ui.collapsing(crate::i18n::t("Scripts"), |ui| {
            for (name, list) in SCRIPT_TYPE_NAMES.iter().zip(&cur.map.scripts) {
                ui.label(format!("{} : {}", crate::i18n::t(name), list.count));
            }
        });
        if let Some(scene) = self.scene()
            && !scene.missing_art.is_empty()
        {
            ui.collapsing(
                format!(
                    "{} ({})",
                    crate::i18n::t("Art introuvable"),
                    scene.missing_art.len()
                ),
                |ui| {
                    for (fid, n) in scene.missing_art.iter().take(50) {
                        ui.label(format!("FID {fid:08X} ×{n}"));
                    }
                },
            );
        }
        ui.separator();
        ui.heading(crate::i18n::t("Sélection"));
        let selected_count = self.selection.objects().len()
            + self.selection.tiles().len()
            + self.selection.hexes().len();
        if selected_count > 1 {
            ui.label(format!(
                "{} {} {} {}",
                self.selection.objects().len(),
                crate::i18n::t("objet(s) et"),
                self.selection.tiles().len(),
                crate::i18n::t("case(s) sélectionnés : voir le panneau d'édition.")
            ));
        }
        let Some(obj) = self.selection.primary().and_then(|r| self.object(r)) else {
            ui.label(crate::i18n::t("Cliquez sur un objet."));
            return None;
        };
        self.object_details(ui, obj);
        let game = self.game.as_ref()?;
        let entry = cur.map.find_script(obj.script_id())?;
        let file = game.data.script_file(entry.list_index().max(0) as usize)?;
        ui.separator();
        let open = ui
            .button(format!("{} {file}", crate::i18n::t("Voir le script")))
            .clicked();
        // De la marge sous le dernier bouton : il ne doit pas toucher le bord du panneau.
        ui.add_space(16.0);
        open.then_some(file)
    }

    /// Nom du prototype dans la langue du jeu (`pro_*.msg`), si le prototype existe et porte un nom.
    pub(crate) fn proto_name(&self, pid: u32) -> Option<String> {
        let g = self.game.as_ref()?;
        g.data
            .proto_entry(pid)
            .map(|e| e.name)
            .filter(|n| !n.is_empty())
    }

    pub(crate) fn object_details(&self, ui: &mut egui::Ui, o: &MapObject) {
        let fid = Fid(o.fid());
        egui::Grid::new("object").num_columns(2).show(ui, |ui| {
            let mut row = |k: &'static str, v: String| {
                ui.label(crate::i18n::t(k));
                ui.label(v);
                ui.end_row();
            };
            if let Some(name) = self.proto_name(o.pid()) {
                row("Nom", name);
            }
            row(
                "Type",
                o.object_type()
                    .map_or("?".into(), |t| crate::i18n::t(t.label()).into()),
            );
            row("PID", format!("{:08X}", o.pid()));
            row("FID", format!("{:08X}", o.fid()));
            row(
                "Hexagone",
                format!(
                    "{} ({}, {})",
                    o.tile(),
                    geom::hex_col_row(o.tile().max(0) as u32).0,
                    geom::hex_col_row(o.tile().max(0) as u32).1
                ),
            );
            row("Orientation", o.orientation().to_string());
            row("Drapeaux", format!("{:08X}", o.flags()));
            row("Script", o.script_id().to_string());
            row("Sous-type", o.subtype.to_string());
            if let Some(g) = self.game.as_ref() {
                row(
                    "Art",
                    g.art
                        .fid_paths(fid, o.orientation().clamp(0, 5) as u32)
                        .first()
                        .cloned()
                        .unwrap_or_default(),
                );
            }
            if let Some(d) = o.exit_destination() {
                row(
                    "Destination",
                    format!(
                        "{} {} {} {} {} {}",
                        crate::i18n::t("carte"),
                        d.map,
                        crate::i18n::t("hex"),
                        d.tile,
                        crate::i18n::t("élév."),
                        d.elevation
                    ),
                );
            }
        });
        if !o.inventory.is_empty() {
            ui.collapsing(
                format!("{} ({})", crate::i18n::t("Inventaire"), o.inventory.len()),
                |ui| {
                    for i in &o.inventory {
                        ui.label(format!("{} × {:08X}", i.count, i.object.pid()));
                    }
                },
            );
        }
    }
}
