//! Outil « Script spatial » : on choisit un script et un rayon, puis un clic sur la carte le pose ; sous le curseur, le rayon est dessiné
//! (les hexagones du disque et son contour), et les scripts spatiaux déjà posés montrent le leur.

use crate::app::MapperApp;
use crate::i18n::t;
use eframe::egui::{self, Color32, Pos2, Stroke, Vec2};
use fo_core::geom;
use fo_core::map::SCRIPT_TYPE_SPATIAL;

impl MapperApp {
    pub(crate) fn spatial_input(&mut self, response: &egui::Response) {
        if response.clicked_by(egui::PointerButton::Primary)
            && let (Some(hex), Some(prog)) = (self.hover_hex, self.edit.spatial_prog)
        {
            let (elevation, radius) = (self.elevation as u32, self.edit.spatial_radius.max(0));
            self.mutate_as("Script spatial ajouté", |m, _| {
                m.add_spatial_script(prog as u32, hex, elevation, radius);
                true
            });
        }
    }

    /// Les hexagones à `radius` pas au plus de `center` (disque) ; `ring` ne garde que ceux à exactement `radius` pas.
    fn hexes_around(center: u32, radius: i32, ring: bool) -> Vec<u32> {
        let (c, r) = geom::hex_col_row(center);
        let (c, r, rad) = (c as i32, r as i32, radius.max(0));
        let mut out = Vec::new();
        for row in (r - rad).max(0)..=(r + rad).min(geom::HEX_GRID_WIDTH as i32 - 1) {
            for col in (c - 2 * rad).max(0)..=(c + 2 * rad).min(geom::HEX_GRID_WIDTH as i32 - 1) {
                let h = row as u32 * geom::HEX_GRID_WIDTH + col as u32;
                let d = geom::hex_distance(center, h);
                if d <= rad && (!ring || d == rad) {
                    out.push(h);
                }
            }
        }
        out
    }

    pub(crate) fn draw_spatial_overlay(&self, painter: &egui::Painter, view: egui::Rect, hex: u32) {
        let cam = self.camera;
        let polygon = |h: u32, fill: Color32, stroke: Stroke| {
            let (x, y) = geom::hex_to_screen(h);
            let c = Pos2::new(x as f32, y as f32);
            let pts: Vec<Pos2> = crate::edit_ui::HEX_OUTLINE
                .iter()
                .map(|(dx, dy)| cam.world_to_screen(view, c + Vec2::new(*dx, *dy)))
                .collect();
            if pts.iter().any(|p| view.expand(30.0).contains(*p)) {
                painter.add(egui::Shape::convex_polygon(pts, fill, stroke));
            }
        };
        // Scripts déjà posés sur cette élévation : leur contour.
        if let Some(cur) = &self.current {
            for e in cur.map.scripts[SCRIPT_TYPE_SPATIAL].iter() {
                if let Some(s) = e
                    .spatial()
                    .filter(|s| s.elevation as usize == self.elevation && s.tile < geom::HEX_COUNT)
                {
                    for h in Self::hexes_around(s.tile, s.radius, true) {
                        polygon(
                            h,
                            Color32::TRANSPARENT,
                            Stroke::new(1.0, Color32::from_rgba_unmultiplied(90, 170, 255, 110)),
                        );
                    }
                }
            }
        }
        // Le script à poser : le disque sous le curseur, puis son contour plus marqué.
        let radius = self.edit.spatial_radius.max(0);
        for h in Self::hexes_around(hex, radius, false) {
            polygon(
                h,
                Color32::from_rgba_unmultiplied(90, 170, 255, 34),
                Stroke::NONE,
            );
        }
        for h in Self::hexes_around(hex, radius, true) {
            polygon(
                h,
                Color32::TRANSPARENT,
                Stroke::new(1.5, Color32::from_rgb(120, 200, 255)),
            );
        }
        polygon(
            hex,
            Color32::TRANSPARENT,
            Stroke::new(2.0, Color32::from_rgb(255, 210, 60)),
        );
    }

    pub(crate) fn spatial_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(t(
            "Cliquez sur la carte pour poser un script spatial : un cercle montre son rayon.",
        ));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(t("Rayon (en hexagones)"));
            ui.add(crate::stepper::Stepper::new(&mut self.edit.spatial_radius).range(0..=50));
        });
        ui.add_space(4.0);
        let chosen = self.edit.spatial_prog.map(|p| {
            self.game
                .as_ref()
                .and_then(|g| g.data.script_file(p))
                .unwrap_or_else(|| format!("#{p}"))
        });
        match chosen {
            Some(name) => {
                ui.label(egui::RichText::new(format!("{} {name}", t("Script à poser :"))).strong());
            }
            None => {
                ui.colored_label(
                    Color32::YELLOW,
                    t("Aucun script choisi : cherchez-en un dans la liste."),
                );
            }
        }
        let names = self.script_names();
        if let Some(k) = crate::object_ui::picker(ui, &mut self.edit.spatial_filter, &names) {
            self.edit.spatial_prog = Some(k);
            self.edit.spatial_filter.clear();
        }
    }
}
