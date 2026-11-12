#![allow(deprecated)]
//! Minicarte : vue d'ensemble de l'élévation affichée, en bas à droite du canevas, avec le cadre de la vue courante.
//! Un clic ou un glissé dans la minicarte déplace la vue.

use crate::app::MapperApp;
use eframe::egui::{
    self, Align2, Color32, ColorImage, Id, LayerId, Pos2, Rect, Sense, Stroke, TextureHandle,
    TextureOptions, Vec2,
};
use fo_scene::{Layer, RgbaImage, Scene, render_shaded};

/// Côté le plus long de la minicarte, en pixels de l'écran.
const SIDE: f32 = 220.0;

#[derive(Default)]
pub(crate) struct MinimapState {
    pub on: bool,
    /// Empreinte de la scène dessinée (adresse, nombre d'éléments, calques) : l'image est refaite quand elle change.
    key: (usize, usize, u8),
    tex: Option<TextureHandle>,
    /// Pixels du monde par pixel de la minicarte.
    ds: u32,
}

impl MinimapState {
    pub(crate) fn new() -> Self {
        Self {
            on: true,
            ..Default::default()
        }
    }
}

/// Moyenne l'image par blocs de `k` × `k` pixels ; renvoie aussi les pixels du monde par pixel du résultat.
fn shrink(img: &RgbaImage, k: u32, base: u32) -> (RgbaImage, u32) {
    let k = k.max(1);
    let (w, h) = ((img.width / k).max(1), (img.height / k).max(1));
    let mut out = RgbaImage::new(w, h, [0, 0, 0, 255]);
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 3];
            let mut n = 0;
            for dy in 0..k {
                for dx in 0..k {
                    let (sx, sy) = (x * k + dx, y * k + dy);
                    if sx < img.width && sy < img.height {
                        let i = (sy as usize * img.width as usize + sx as usize) * 4;
                        for (c, acc) in sum.iter_mut().enumerate() {
                            *acc += img.data[i + c] as u32;
                        }
                        n += 1;
                    }
                }
            }
            let o = (y as usize * w as usize + x as usize) * 4;
            for (c, acc) in sum.iter().enumerate() {
                out.data[o + c] = (acc / n.max(1)) as u8;
            }
        }
    }
    (out, base * k)
}

/// Réduction qui fait tenir le côté le plus long de l'emprise dans `SIDE` pixels.
pub(crate) fn reduction(bounds: (i32, i32, i32, i32)) -> u32 {
    ((bounds.2.max(bounds.3) as f32 / SIDE).ceil() as u32).max(1)
}

/// Point du monde sous un point de la minicarte (`rect` : où l'image est dessinée).
pub(crate) fn minimap_to_world(bounds: (i32, i32, i32, i32), ds: u32, rect: Rect, p: Pos2) -> Pos2 {
    Pos2::new(
        bounds.0 as f32 + (p.x - rect.min.x) * ds as f32,
        bounds.1 as f32 + (p.y - rect.min.y) * ds as f32,
    )
}

pub(crate) fn world_to_minimap(bounds: (i32, i32, i32, i32), ds: u32, rect: Rect, w: Pos2) -> Pos2 {
    Pos2::new(
        rect.min.x + (w.x - bounds.0 as f32) / ds as f32,
        rect.min.y + (w.y - bounds.1 as f32) / ds as f32,
    )
}

impl MapperApp {
    fn minimap_refresh(&mut self, ctx: &egui::Context, scene: &Scene) {
        let mut layers = self.layers;
        layers.set(Layer::Exit, false);
        layers.set(Layer::Blocker, false);
        let key = (
            scene as *const Scene as usize,
            scene.items.len(),
            layers.bits(),
        );
        if key == self.minimap.key && self.minimap.tex.is_some() {
            return;
        }
        let ds = reduction(scene.bounds);
        // On dessine à une échelle où chaque sprite touche plusieurs pixels, puis on moyenne par blocs jusqu'à la taille de la minicarte.
        let draw_ds = ds.min(4);
        let img = render_shaded(
            scene,
            layers,
            scene.bounds,
            draw_ds,
            None,
            [12, 12, 14, 255],
            &mut |_| Some(1.0),
        );
        let (img, ds) = shrink(&img, ds / draw_ds, draw_ds);
        let color = ColorImage::from_rgba_unmultiplied(
            [img.width as usize, img.height as usize],
            &img.data,
        );
        self.minimap.tex = Some(ctx.load_texture("minimap", color, TextureOptions::LINEAR));
        self.minimap.ds = ds;
        self.minimap.key = key;
    }

    pub(crate) fn minimap_overlay(&mut self, ctx: &egui::Context, parent: LayerId, view: Rect) {
        if !self.minimap.on
            || view.width() < 500.0
            || view.height() < 300.0
            || self.confirm.is_some()
            || self.file.new_map.is_some()
            || self.file.play_dialog
        {
            return;
        }
        let Some(scene) = self.scene().cloned() else {
            return;
        };
        if scene.bounds.2 <= 0 || scene.bounds.3 <= 0 {
            return;
        }
        self.minimap_refresh(ctx, &scene);
        let (Some(tex), ds) = (self.minimap.tex.clone(), self.minimap.ds) else {
            return;
        };
        let cam = self.camera;
        let mut goto = None;
        crate::ui_kit::canvas_overlay(ctx, parent, Id::new("minimap"))
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(view.right_bottom() + Vec2::new(-10.0, -70.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(4.0)
                    .show(ui, |ui| {
                        let (rect, resp) =
                            ui.allocate_exact_size(tex.size_vec2(), Sense::click_and_drag());
                        ui.painter().image(
                            tex.id(),
                            rect,
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            Color32::WHITE,
                        );
                        // Cadre de la vue courante.
                        let (a, b) = (
                            cam.screen_to_world(view, view.min),
                            cam.screen_to_world(view, view.max),
                        );
                        let frame = Rect::from_min_max(
                            world_to_minimap(scene.bounds, ds, rect, a),
                            world_to_minimap(scene.bounds, ds, rect, b),
                        )
                        .intersect(rect);
                        if frame.is_positive() {
                            ui.painter().rect_stroke(
                                frame,
                                0.0,
                                Stroke::new(1.5, Color32::from_rgb(255, 200, 80)),
                                egui::StrokeKind::Inside,
                            );
                        }
                        // Point de départ du joueur.
                        if let Some(c) = self.current.as_ref()
                            && c.map.header.player_elevation as usize == self.elevation
                            && (0..fo_core::geom::HEX_COUNT as i32)
                                .contains(&c.map.header.player_tile)
                        {
                            let (x, y) =
                                fo_core::geom::hex_to_screen(c.map.header.player_tile as u32);
                            let p = world_to_minimap(
                                scene.bounds,
                                ds,
                                rect,
                                Pos2::new(x as f32, y as f32),
                            );
                            if rect.contains(p) {
                                ui.painter()
                                    .circle_filled(p, 2.5, Color32::from_rgb(80, 255, 120));
                            }
                        }
                        if (resp.clicked() || resp.dragged())
                            && let Some(p) = resp.interact_pointer_pos()
                        {
                            goto = Some(minimap_to_world(
                                scene.bounds,
                                ds,
                                rect,
                                p.clamp(rect.min, rect.max),
                            ));
                        }
                        resp.on_hover_text(crate::i18n::t(
                            "Minicarte : cliquez ou glissez pour déplacer la vue",
                        ));
                    });
            });
        if let Some(w) = goto {
            self.camera.center = w;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduction_fits_the_longest_side() {
        assert_eq!(reduction((0, 0, 3328, 1500)), 16);
        assert_eq!(reduction((0, 0, 100, 100)), 1);
        assert!(3328 / reduction((0, 0, 3328, 1500)) as i32 <= SIDE as i32);
    }

    #[test]
    fn world_and_minimap_points_round_trip() {
        let bounds = (-500, 40, 3000, 1200);
        let rect = Rect::from_min_size(Pos2::new(700.0, 20.0), Vec2::new(188.0, 75.0));
        let w = Pos2::new(1234.0, 600.0);
        let back = minimap_to_world(bounds, 16, rect, world_to_minimap(bounds, 16, rect, w));
        assert!((back - w).length() < 0.01);
        assert_eq!(
            minimap_to_world(bounds, 16, rect, rect.min),
            Pos2::new(-500.0, 40.0)
        );
    }
}

#[cfg(test)]
mod shrink_tests {
    use super::*;

    #[test]
    fn shrink_averages_blocks() {
        let mut img = RgbaImage::new(4, 2, [0, 0, 0, 255]);
        for (i, v) in [10u8, 30, 50, 70, 10, 30, 50, 70].iter().enumerate() {
            img.data[i * 4] = *v;
        }
        let (out, ds) = shrink(&img, 2, 4);
        assert_eq!((out.width, out.height, ds), (2, 1, 8));
        assert_eq!(out.data[0], 20);
        assert_eq!(out.data[4], 60);
    }
}
