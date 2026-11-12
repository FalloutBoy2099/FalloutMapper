//! Export d'une image PNG : la carte entière ou l'écran du jeu (640 × 480 autour du point de départ du joueur), avec ou sans éclairage.
//! Utilise les calques affichés ; les repères d'édition (sorties, blocs) ne sont jamais dessinés.

use crate::app::MapperApp;
use crate::i18n::t;
use crate::preview::{DAY, EVENING, NIGHT, PreviewView, item_hex, light_field, map_lights};
use crate::ui_kit::FooterButton as B;
use eframe::egui::{self, Pos2, Vec2};
use fo_core::geom;
use fo_scene::{Layer, RgbaImage, render_shaded};
use std::path::Path;

/// Taille maximale (côté le plus long) d'une image exportée : au-delà, l'échelle est réduite d'office.
const MAX_SIDE: i32 = 16_384;
const GAME_VIEW: (i32, i32) = (640, 480);

#[derive(Debug, Clone)]
pub(crate) struct ExportState {
    pub open: bool,
    /// Écran du jeu autour du point de départ ; sinon la carte entière.
    pub game_view: bool,
    /// Réduction de la carte entière : 1, 2 ou 4 (1 pixel de l'image pour n pixels du monde).
    pub scale: u32,
    pub lighting: bool,
    pub ambient: f32,
    pub hide_roofs: bool,
}

impl Default for ExportState {
    fn default() -> Self {
        Self {
            open: false,
            game_view: false,
            scale: 1,
            lighting: false,
            ambient: EVENING,
            hide_roofs: true,
        }
    }
}

/// Zone à rendre et réduction : la carte entière (réduite si elle dépasse `MAX_SIDE`) ou l'écran du jeu centré sur `viewpoint`.
/// Zone du monde : x, y, largeur, hauteur.
type Region = (i32, i32, i32, i32);

fn export_region(
    bounds: Region,
    game_view: bool,
    viewpoint: Option<(i32, i32)>,
    scale: u32,
) -> Result<(Region, u32), String> {
    if game_view {
        let (x, y) = viewpoint.ok_or_else(|| {
            t("Pas de point de départ du joueur sur cette élévation.").to_string()
        })?;
        return Ok((
            (
                x - GAME_VIEW.0 / 2,
                y - GAME_VIEW.1 / 2,
                GAME_VIEW.0,
                GAME_VIEW.1,
            ),
            1,
        ));
    }
    let (_, _, w, h) = bounds;
    if w <= 0 || h <= 0 {
        return Err(t("Rien à exporter : la carte est vide.").to_string());
    }
    let mut ds = scale.max(1);
    while (w.max(h) / ds as i32) > MAX_SIDE {
        ds += 1;
    }
    Ok((bounds, ds))
}

/// Écrit l'image en PNG RGBA 8 bits.
fn write_png(path: &Path, img: &RgbaImage) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), img.width, img.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| w.write_image_data(&img.data))
        .map_err(|e| e.to_string())
}

impl MapperApp {
    /// Dessine l'image selon les réglages d'export.
    pub(crate) fn export_render(&self) -> Result<RgbaImage, String> {
        let (Some(scene), Some(cur)) = (self.scene(), self.current.as_ref()) else {
            return Err(t("Aucune carte ouverte.").into());
        };
        let o = &self.export;
        let e = self.elevation;
        let start = (cur.map.header.player_elevation as usize == e)
            .then_some(cur.map.header.player_tile)
            .filter(|t| (0..geom::HEX_COUNT as i32).contains(t))
            .map(|t| t as u32);
        let viewpoint = start.map(geom::hex_to_screen);
        let (region, ds) = export_region(scene.bounds, o.game_view, viewpoint, o.scale)?;
        let mut layers = self.layers;
        layers.set(Layer::Exit, false);
        layers.set(Layer::Blocker, false);
        let lit = o.lighting || o.game_view;
        let field = lit.then(|| light_field(&map_lights(&cur.map, e), o.ambient));
        let r = self.preview.roof_radius.max(1) as f32;
        let view = PreviewView {
            viewpoint: viewpoint
                .filter(|_| o.game_view)
                .map(|(x, y)| Pos2::new(x as f32, y as f32)),
            roof_radius_px: Vec2::new(r * 32.0, r * 16.0),
            hide_roofs: o.game_view && o.hide_roofs,
        };
        let background = if o.game_view {
            [0, 0, 0, 255]
        } else {
            [12, 12, 14, 255]
        };
        let mut shade = |item: &fo_scene::DrawItem| {
            if view.hides(item) {
                return None;
            }
            Some(
                field
                    .as_ref()
                    .and_then(|f| item_hex(item).and_then(|h| f.get(h as usize).copied()))
                    .unwrap_or(1.0),
            )
        };
        Ok(render_shaded(
            scene, layers, region, ds, None, background, &mut shade,
        ))
    }

    /// Rend et écrit l'image ; le résultat va dans la barre d'état.
    pub(crate) fn export_to(&mut self, path: &Path) {
        let result = self
            .export_render()
            .and_then(|img| write_png(path, &img).map(|()| (img.width, img.height)));
        self.status = match result {
            Ok((w, h)) => format!("{} {} ({w} × {h})", t("Image exportée :"), path.display()),
            Err(e) => format!("{} : {e}", t("Échec de l'export")),
        };
    }

    pub(crate) fn export_window(&mut self, ctx: &egui::Context) {
        if !self.export.open {
            return;
        }
        let mut open = true;
        let mut action = None;
        egui::Window::new(t("Exporter une image"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_width(380.0);
                let o = &mut self.export;
                ui.radio_value(&mut o.game_view, false, t("Carte entière"));
                ui.radio_value(
                    &mut o.game_view,
                    true,
                    t("Écran du jeu (640 × 480 autour du point de départ du joueur)"),
                );
                ui.add_space(4.0);
                if o.game_view {
                    ui.checkbox(
                        &mut o.hide_roofs,
                        t("Masquer les toits autour du point de vue"),
                    );
                } else {
                    ui.horizontal(|ui| {
                        ui.label(t("Échelle"));
                        for (label, s) in [("1:1", 1), ("1:2", 2), ("1:4", 4)] {
                            ui.selectable_value(&mut o.scale, s, label);
                        }
                    });
                    ui.checkbox(&mut o.lighting, t("Appliquer l'éclairage"));
                }
                ui.add_enabled_ui(o.lighting || o.game_view, |ui| {
                    ui.horizontal(|ui| {
                        for (label, v) in [("Jour", DAY), ("Soir", EVENING), ("Nuit", NIGHT)] {
                            if ui
                                .selectable_label((o.ambient - v).abs() < 0.01, t(label))
                                .clicked()
                            {
                                o.ambient = v;
                            }
                        }
                    });
                });
                ui.weak(t(
                    "Les calques affichés sont exportés ; les repères d'édition ne le sont pas.",
                ));
                ui.add_space(6.0);
                match crate::ui_kit::footer(
                    ui,
                    &[
                        B::normal(t("Annuler")),
                        B::primary(t("Choisir le fichier…")),
                    ],
                ) {
                    Some(0) => action = Some(false),
                    Some(_) => action = Some(true),
                    None => {}
                }
            });
        if !open || action == Some(false) || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.export.open = false;
        }
        if action == Some(true) {
            self.export.open = false;
            self.open_dialog(crate::file_dialog::Purpose::ExportImage);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_of_the_game_view_is_640_by_480_around_the_viewpoint() {
        let (r, ds) = export_region((0, 0, 9000, 4000), true, Some((1000, 700)), 4).unwrap();
        assert_eq!((r, ds), ((680, 460, 640, 480), 1));
        assert!(export_region((0, 0, 9000, 4000), true, None, 1).is_err());
    }

    #[test]
    fn whole_map_is_scaled_down_when_too_large_and_empty_maps_are_refused() {
        assert_eq!(
            export_region((5, 6, 3000, 2000), false, None, 2).unwrap(),
            ((5, 6, 3000, 2000), 2)
        );
        let (_, ds) = export_region((0, 0, 40_000, 3000), false, None, 1).unwrap();
        assert!(40_000 / ds as i32 <= MAX_SIDE && ds >= 3);
        assert!(export_region((0, 0, 0, 0), false, None, 1).is_err());
    }

    #[test]
    fn png_is_written_and_readable() {
        let img = RgbaImage::new(3, 2, [10, 20, 30, 255]);
        let path = std::env::temp_dir().join("fo_export_test.png");
        write_png(&path, &img).unwrap();
        let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()));
        let mut reader = dec.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (3, 2));
        assert_eq!(&buf[..4], &[10, 20, 30, 255]);
        std::fs::remove_file(&path).unwrap();
    }
}
