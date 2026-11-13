//! Édition de la carte : outils, annulation / rétablissement, palettes, enregistrement.

use crate::app::MapperApp;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, TextureOptions, Vec2};
use fo_core::{Fid, Sprite, geom};
use fo_scene::{DrawItem, Scene};
use std::sync::Arc;

use crate::edit_ui::*;

impl MapperApp {
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

    pub(crate) fn draw_item_shifted(
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
}

pub(crate) fn hex_diamond(c: Pos2) -> [Pos2; 6] {
    HEX_OUTLINE.map(|(dx, dy)| Pos2::new(c.x + dx, c.y + dy))
}

/// Position du pointeur dans le monde, si elle est dans la vue.
pub(crate) fn response_world(ctx: &egui::Context, cam: &crate::view::Camera, view: Rect) -> Option<Pos2> {
    let p = ctx.input(|i| i.pointer.hover_pos())?;
    view.contains(p).then(|| cam.screen_to_world(view, p))
}
