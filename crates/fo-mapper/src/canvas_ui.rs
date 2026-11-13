use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, TextureOptions, Vec2};
use fo_core::geom;
use fo_scene::{MarkerKind, ObjectRef};
use std::sync::Arc;
use std::time::Duration;

use crate::app::*;

impl MapperApp {
    /// Image courante d'un décor animé dans l'aperçu du jeu : le sprite de l'image voulue et la position où le dessiner.
    /// Seuls les décors ordinaires tournent ; les portes, escaliers, murs et créatures gardent l'image de la carte.
    pub(crate) fn object_frame(
        &mut self,
        item: &fo_scene::DrawItem,
        ms: u64,
    ) -> Option<(Arc<fo_core::Sprite>, i32, i32)> {
        let r = item.object?;
        let o = self.object(r)?;
        if o.object_type() != Some(fo_core::ObjectType::Scenery)
            || o.subtype != fo_core::formats::scenery_kind::GENERIC
        {
            return None;
        }
        let (fid, orient) = (o.fid(), o.orientation().max(0) as u32);
        let art = self.game.as_ref()?.art.clone();
        let info = *self.frame_info.entry((fid, orient)).or_insert_with(|| {
            let (path, _) = art.object_sprite(fo_core::Fid(fid), orient, 0)?;
            let frm = art.frm(&path)?;
            let dir = if frm.directions.len() > 1 {
                orient as usize
            } else {
                0
            };
            let frames = frm.directions.get(dir)?.frames.len();
            (frames > 1).then_some((frames, frm.fps))
        });
        let (frames, fps) = info?;
        let frame = (ms * u64::from(fps.max(1)) / 1000) as usize % frames;
        let (_, sprite) = art.object_sprite(fo_core::Fid(fid), orient, frame)?;
        // Même point d'ancrage que l'image de la carte : on ne déplace que de la différence d'origine des deux sprites.
        let (x, y) = (
            item.x + sprite.origin_x - item.sprite.origin_x,
            item.y + sprite.origin_y - item.sprite.origin_y,
        );
        Some((sprite, x, y))
    }

    pub(crate) fn canvas(&mut self, ui: &mut egui::Ui) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let view = response.rect;
        self.canvas_rect = view;
        let toolbar_ctx = ui.ctx().clone();
        self.map_toolbars(&toolbar_ctx, ui.layer_id(), view);
        self.minimap_overlay(&toolbar_ctx, ui.layer_id(), view);
        painter.rect_filled(view, 0.0, Color32::from_rgb(14, 14, 16));

        if self.loading.is_some() {
            painter.text(
                view.center(),
                egui::Align2::CENTER_CENTER,
                crate::i18n::t("Chargement…"),
                egui::FontId::proportional(22.0),
                Color32::GRAY,
            );
        }
        let Some(scene) = self.scene().cloned() else {
            if self.loading.is_none() {
                painter.text(
                    view.center(),
                    egui::Align2::CENTER_CENTER,
                    crate::i18n::t("Choisissez une carte dans la liste"),
                    egui::FontId::proportional(18.0),
                    Color32::GRAY,
                );
            }
            return;
        };

        if self.start_view && view.width() > 10.0 {
            self.camera.zoom = 0.5;
            let start = self
                .current
                .as_ref()
                .map(|c| c.map.header.player_tile)
                .filter(|&t| (0..geom::HEX_COUNT as i32).contains(&t));
            self.camera.center = match start {
                Some(t) => {
                    let (x, y) = geom::hex_to_screen(t as u32);
                    Pos2::new(x as f32, y as f32)
                }
                None => Pos2::new(
                    (scene.bounds.0 + scene.bounds.2 / 2) as f32,
                    (scene.bounds.1 + scene.bounds.3 / 2) as f32,
                ),
            };
            self.start_view = false;
            self.needs_fit = false;
        }
        if self.needs_fit && view.width() > 10.0 {
            self.camera.fit(view, scene.bounds);
            self.needs_fit = false;
        }
        let world_pos = response
            .hover_pos()
            .map(|p| self.camera.screen_to_world(view, p));
        self.hover_hex = world_pos.and_then(|w| geom::screen_to_hex(w.x as i32, w.y as i32));
        let editing = self.edit.enabled;
        let left_taken = editing && self.canvas_edit_input(ui, &response, world_pos, &scene);
        if response.clicked_by(egui::PointerButton::Middle)
            && let Some(p) = response.hover_pos()
        {
            // Bouton du milieu : retour au zoom 100 %, sous le curseur.
            let factor = 1.0 / self.camera.zoom;
            self.camera.zoom_at(view, p, factor);
        }
        let pan = response.dragged_by(egui::PointerButton::Middle)
            || response.dragged_by(egui::PointerButton::Secondary)
            || (response.dragged_by(egui::PointerButton::Primary) && !left_taken);
        if pan {
            self.camera.pan_by_screen(response.drag_delta());
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0
                && let Some(p) = response.hover_pos()
            {
                self.camera.zoom_at(view, p, (scroll * 0.0025).exp());
            }
        }

        let cam = self.camera;
        let world_view = Rect::from_min_max(
            cam.screen_to_world(view, view.min),
            cam.screen_to_world(view, view.max),
        );

        let mut created = 0;
        let mut pending = false;
        let mut any_animated = false;
        let ctx = ui.ctx().clone();
        // Aperçu du jeu : lumière par hexagone, toits masqués autour du point de vue, sans repères d'édition.
        let preview = self.preview_prepare();
        let previewing = preview.is_some();
        // Dans l'aperçu du jeu, les décors à plusieurs images tournent en boucle (ventilateurs, enseignes…).
        let anim_ms = self.anim_start.elapsed().as_millis() as u64;
        let layers = self.layers;
        self.effects.prune(&ctx);
        let mut effects = crate::effects::EffectsFrame::new(&scene, [14, 14, 16, 255]);
        for item in scene.items.iter().filter(|i| {
            layers.contains(i.layer)
                && !(previewing
                    && matches!(i.layer, fo_scene::Layer::Exit | fo_scene::Layer::Blocker))
        }) {
            if preview.as_ref().is_some_and(|p| p.hides(item)) {
                continue;
            }
            let moving = if previewing {
                self.object_frame(item, anim_ms)
            } else {
                None
            };
            if moving.is_some() {
                any_animated = true;
            }
            let (sprite, ix, iy) = moving.unwrap_or_else(|| (item.sprite.clone(), item.x, item.y));
            if let Some(effects) = &mut effects {
                let mut posed = item.clone();
                posed.sprite = sprite.clone();
                posed.x = ix;
                posed.y = iy;
                effects.push(
                    posed,
                    if previewing {
                        self.preview_light(item)
                    } else {
                        1.0
                    },
                    if self.anim_on {
                        self.anim.key(sprite.anim_mask, anim_ms)
                    } else {
                        0
                    },
                );
            }
            let wr = Rect::from_min_size(
                Pos2::new(ix as f32, iy as f32),
                Vec2::new(sprite.width as f32, sprite.height as f32),
            );
            if !wr.intersects(world_view) {
                continue;
            }
            if matches!(item.blend, fo_scene::Blend::Palette(_))
                && let Some(effects) = &effects
            {
                let anim = self
                    .game
                    .as_ref()
                    .filter(|_| self.anim_on)
                    .map(|g| (&self.anim, g.art.palette().as_ref(), anim_ms));
                let tex = effects.patch(&mut self.effects, &ctx, anim);
                let sr = Rect::from_min_max(
                    cam.world_to_screen(view, wr.min),
                    cam.world_to_screen(view, wr.max),
                );
                let alpha = preview.as_ref().map_or(255, |p| p.wall_alpha(item));
                painter.image(
                    tex,
                    sr,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::from_white_alpha(alpha),
                );
                if self.anim_on && sprite.anim_mask != 0 {
                    any_animated = true;
                }
                continue;
            }
            let id = sprite.id;
            if !self.textures.contains_key(&id) {
                if created >= TEXTURE_BUDGET {
                    pending = true;
                    continue;
                }
                created += 1;
                let img = egui::ColorImage::from_rgba_unmultiplied(
                    [sprite.width as usize, sprite.height as usize],
                    &sprite.rgba,
                );
                self.textures.insert(
                    id,
                    ctx.load_texture(format!("sprite{id}"), img, TextureOptions::NEAREST),
                );
            }
            if self.anim_on && sprite.anim_mask != 0 {
                any_animated = true;
                let t = self.anim_start.elapsed().as_millis() as u64;
                let key = self.anim.key(sprite.anim_mask, t);
                if self.anim_keys.get(&id) != Some(&key)
                    && let (Some(g), Some(tex)) = (&self.game, self.textures.get_mut(&id))
                {
                    let rgba = self.anim.render(&sprite.indices, g.art.palette(), t);
                    let img = egui::ColorImage::from_rgba_unmultiplied(
                        [sprite.width as usize, sprite.height as usize],
                        &rgba,
                    );
                    tex.set(img, TextureOptions::NEAREST);
                    self.anim_keys.insert(id, key);
                }
            }
            let sr = Rect::from_min_max(
                cam.world_to_screen(view, wr.min),
                cam.world_to_screen(view, wr.max),
            );
            let tint = if previewing {
                crate::preview::lit_tint(item, self.preview_light(item))
            } else {
                match item.blend {
                    fo_scene::Blend::Normal => Color32::from_white_alpha(item.alpha),
                    fo_scene::Blend::Palette(_) => Color32::WHITE,
                }
            };
            let tint = preview.as_ref().map_or(tint, |p| {
                tint.gamma_multiply(p.wall_alpha(item) as f32 / 255.0)
            });
            painter.image(
                self.textures[&id].id(),
                sr,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                tint,
            );
        }
        if pending {
            ctx.request_repaint();
        } else if any_animated {
            ctx.request_repaint_after(Duration::from_millis(33));
        }

        if let Some(p) = &preview {
            // Aperçu : ni halos, ni grille, ni repères ; le cadre de l'écran du jeu si demandé.
            self.draw_game_frame(&painter, view, p);
        }
        if self.walk.on && !previewing {
            self.draw_walk(&painter, view, world_view);
        }
        if self.show_lights && !previewing {
            self.draw_lights(&painter, view, world_view);
        }
        if !previewing {
            self.draw_diff(&painter, view);
        }
        if self.show_grid && cam.zoom >= 0.6 && !previewing {
            self.draw_grid(&painter, view, world_view);
        }
        if self.show_markers && !previewing {
            for m in &scene.markers {
                let (cx, cy) = geom::hex_to_screen(m.hex);
                let p = cam.world_to_screen(view, Pos2::new(cx as f32, cy as f32));
                if !view.expand(20.0).contains(p) {
                    continue;
                }
                let (color, r) = match m.kind {
                    MarkerKind::PlayerStart => (Color32::from_rgb(80, 255, 120), 9.0),
                    MarkerKind::ExitGrid => (Color32::from_rgb(255, 210, 60), 6.0),
                    MarkerKind::SpatialScript => (Color32::from_rgb(90, 170, 255), 6.0),
                    MarkerKind::Hidden => (Color32::from_rgba_unmultiplied(200, 200, 200, 90), 3.0),
                };
                painter.circle_stroke(p, r * cam.zoom.clamp(0.6, 1.5), Stroke::new(1.5, color));
            }
        }
        // Éléments sélectionnés : cadre rouge autour des objets, losange autour des cases.
        if !self.selection.is_empty() {
            let hex_fill = Color32::from_rgba_unmultiplied(80, 160, 255, 70);
            for &h in self.selection.hexes() {
                let (cx, cy) = geom::hex_to_screen(h);
                let c = Pos2::new(cx as f32, cy as f32);
                let pts: Vec<Pos2> = crate::edit_ui::HEX_OUTLINE
                    .iter()
                    .map(|(dx, dy)| cam.world_to_screen(view, c + Vec2::new(*dx, *dy)))
                    .collect();
                painter.add(egui::Shape::convex_polygon(
                    pts,
                    hex_fill,
                    Stroke::new(1.0, Color32::from_rgb(80, 160, 255)),
                ));
            }
            let selected: std::collections::HashSet<ObjectRef> =
                self.selection.objects().iter().copied().collect();
            let stroke = Stroke::new(if self.edit.selection_locked { 2.5 } else { 1.5 }, if self.edit.selection_locked { Color32::from_rgb(255, 190, 50) } else { Color32::from_rgb(255, 80, 80) });
            for item in scene
                .items
                .iter()
                .filter(|i| i.object.is_some_and(|o| selected.contains(&o)))
            {
                let r = Rect::from_min_size(
                    Pos2::new(item.x as f32, item.y as f32),
                    Vec2::new(item.sprite.width as f32, item.sprite.height as f32),
                );
                painter.rect_stroke(
                    Rect::from_min_max(
                        cam.world_to_screen(view, r.min),
                        cam.world_to_screen(view, r.max),
                    ),
                    0.0,
                    stroke,
                    egui::StrokeKind::Outside,
                );
            }
            for t in self.selection.tiles() {
                let (x, y) = geom::square_to_screen(t.square);
                let dy = if t.layer == fo_core::map::TileLayer::Roof {
                    geom::ROOF_OFFSET_Y as f32
                } else {
                    0.0
                };
                let (x, y) = (x as f32, y as f32 + dy);
                let pts = [
                    Pos2::new(x + 40.0, y),
                    Pos2::new(x + 80.0, y + 18.0),
                    Pos2::new(x + 40.0, y + 36.0),
                    Pos2::new(x, y + 18.0),
                ];
                painter.add(egui::Shape::closed_line(
                    pts.iter().map(|&p| cam.world_to_screen(view, p)).collect(),
                    stroke,
                ));
            }
        }

        if self.show_scripted && !previewing {
            self.draw_scripted(&painter, view, &scene);
        }
        if editing {
            self.draw_edit_overlay(&ctx, &painter, view, &scene);
            self.canvas_context_menu(&response);
        } else if response.clicked()
            && let Some(w) = world_pos
        {
            self.selection.set_object(
                scene
                    .pick(self.layers, w.x as i32, w.y as i32)
                    .and_then(|i| i.object),
            );
        }
    }

    pub(crate) fn draw_grid(&self, painter: &egui::Painter, view: Rect, world_view: Rect) {
        let stroke = Stroke::new(1.0, self.prefs.grid_stroke_color());
        for tile in 0..geom::HEX_COUNT {
            let (cx, cy) = geom::hex_to_screen(tile);
            let c = Pos2::new(cx as f32, cy as f32);
            if !world_view.expand(32.0).contains(c) {
                continue;
            }
            let pts: Vec<Pos2> = crate::edit_ui::HEX_OUTLINE
                .iter()
                .map(|(dx, dy)| self.camera.world_to_screen(view, c + Vec2::new(*dx, *dy)))
                .collect();
            painter.add(egui::Shape::closed_line(pts, stroke));
        }
    }
}
