//! Vue côte à côte : une seconde carte (un autre onglet) s'affiche à droite de la carte active. On peut y naviguer,
//! y sélectionner des objets et les glisser sur la carte active pour les y copier.

use crate::app::MapperApp;
use crate::extras::Clipboard;
use crate::i18n::t;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use fo_core::geom;
use fo_scene::ObjectRef;

impl MapperApp {
    /// Affiche (ou non) l'onglet `i` à droite de la carte active.
    pub(crate) fn set_split(&mut self, i: Option<usize>) {
        self.split = i.filter(|&i| i != self.active_tab && i < self.docs.len());
        self.cross_drag = false;
    }

    /// Dispose la carte active à gauche et la seconde à droite (ou la carte active seule).
    pub(crate) fn map_area(&mut self, ui: &mut egui::Ui) {
        let Some(j) = self
            .split
            .filter(|&j| j != self.active_tab && j < self.docs.len())
        else {
            self.split = None;
            self.canvas(ui);
            return;
        };
        let full = ui.available_rect_before_wrap();
        let gap = 6.0;
        let half = ((full.width() - gap) / 2.0).floor();
        let left = Rect::from_min_size(full.min, Vec2::new(half, full.height()));
        let right = Rect::from_min_size(
            Pos2::new(full.min.x + half + gap, full.min.y),
            Vec2::new(half, full.height()),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| self.canvas(ui));
        ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| {
            self.secondary_pane(ui, j)
        });
    }

    fn secondary_pane(&mut self, ui: &mut egui::Ui, j: usize) {
        let ctx = ui.ctx().clone();
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let view = resp.rect;
        painter.rect_filled(view, 0.0, Color32::from_rgb(18, 18, 22));
        painter.rect_stroke(
            view,
            0.0,
            Stroke::new(1.0, Color32::from_gray(70)),
            egui::StrokeKind::Inside,
        );
        let Some(doc) = self.docs.get(j) else { return };
        let (Some(cur), elevation, mut camera) = (doc.current.as_ref(), doc.elevation, doc.camera)
        else {
            return;
        };
        let name = cur.file.clone();
        let Some(scene) = cur.scenes.get(elevation).and_then(|s| s.clone()) else {
            return;
        };

        // Navigation : molette (zoom), glisser (déplacement), clic du milieu (100 %).
        if let Some(p) = resp.hover_pos() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                camera.zoom_at(view, p, (scroll * 0.0025).exp());
            }
            if resp.clicked_by(egui::PointerButton::Middle) {
                camera.zoom_at(view, p, 1.0 / camera.zoom);
            }
        }
        let world = resp.hover_pos().map(|p| camera.screen_to_world(view, p));
        let press_world = ui
            .input(|i| i.pointer.press_origin())
            .map(|p| camera.screen_to_world(view, p));

        // Un glisser qui part d'un objet sélectionné copie la sélection sur la carte active ; sinon il déplace la vue.
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(w) = press_world
        {
            let picked = scene
                .pick(self.layers, w.x as i32, w.y as i32)
                .and_then(|i| i.object);
            if let Some(r) = picked {
                if !self.docs[j].selection.contains_object(r) {
                    self.docs[j].selection.set_object(Some(r));
                }
                self.cross_drag = true;
            } else {
                self.cross_drag = false;
            }
        }
        let panning = resp.dragged_by(egui::PointerButton::Middle)
            || resp.dragged_by(egui::PointerButton::Secondary)
            || (resp.dragged_by(egui::PointerButton::Primary) && !self.cross_drag);
        if panning {
            camera.pan_by_screen(resp.drag_delta());
        }
        if resp.clicked_by(egui::PointerButton::Primary)
            && let Some(w) = world
        {
            let picked = scene
                .pick(self.layers, w.x as i32, w.y as i32)
                .and_then(|i| i.object);
            self.docs[j].selection.set_object(picked);
        }

        // Dessin.
        let world_view = Rect::from_min_max(
            camera.screen_to_world(view, view.min),
            camera.screen_to_world(view, view.max),
        );
        let layers = self.layers;
        self.effects.prune(&ctx);
        let mut effects = crate::effects::EffectsFrame::new(&scene, [18, 18, 22, 255]);
        for item in scene.items.iter().filter(|i| layers.contains(i.layer)) {
            if let Some(effects) = &mut effects {
                effects.push(item.clone(), 1.0, 0);
            }
            let wr = Rect::from_min_size(
                Pos2::new(item.x as f32, item.y as f32),
                Vec2::new(item.sprite.width as f32, item.sprite.height as f32),
            );
            if !wr.intersects(world_view) {
                continue;
            }
            let tex = if matches!(item.blend, fo_scene::Blend::Palette(_))
                && let Some(effects) = &effects
            {
                effects.patch(&mut self.effects, &ctx, None)
            } else {
                self.texture_for(&ctx, &item.sprite)
            };
            let sr = Rect::from_min_max(
                camera.world_to_screen(view, wr.min),
                camera.world_to_screen(view, wr.max),
            );
            painter.image(
                tex,
                sr,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::from_white_alpha(item.alpha),
            );
        }
        let selected: Vec<ObjectRef> = self.docs[j].selection.objects().to_vec();
        for item in scene
            .items
            .iter()
            .filter(|i| i.object.is_some_and(|o| selected.contains(&o)))
        {
            let wr = Rect::from_min_size(
                Pos2::new(item.x as f32, item.y as f32),
                Vec2::new(item.sprite.width as f32, item.sprite.height as f32),
            );
            let sr = Rect::from_min_max(
                camera.world_to_screen(view, wr.min),
                camera.world_to_screen(view, wr.max),
            );
            painter.rect_stroke(
                sr,
                0.0,
                Stroke::new(1.5, Color32::from_rgb(255, 80, 80)),
                egui::StrokeKind::Outside,
            );
        }
        painter.text(
            view.left_top() + Vec2::new(10.0, 8.0),
            egui::Align2::LEFT_TOP,
            format!(
                "{name}  ·  {}",
                t("glisser un objet sélectionné vers l'autre carte pour le copier")
            ),
            egui::FontId::proportional(13.0),
            Color32::from_gray(210),
        );

        // Fin du glisser : si le pointeur est au-dessus de la carte active, on y copie la sélection.
        if resp.drag_stopped() && self.cross_drag {
            self.cross_drag = false;
            let drop = ui.input(|i| i.pointer.latest_pos());
            if let Some(p) = drop.filter(|p| self.canvas_rect.contains(*p)) {
                let w = self.camera.screen_to_world(self.canvas_rect, p);
                if let Some(target) = geom::screen_to_hex(w.x as i32, w.y as i32) {
                    self.copy_from_tab(j, target);
                }
            }
        } else if self.cross_drag
            && let Some(p) = ui.input(|i| i.pointer.latest_pos())
        {
            ui.painter().text(
                p + Vec2::new(14.0, 14.0),
                egui::Align2::LEFT_TOP,
                t("Copier ici"),
                egui::FontId::proportional(13.0),
                Color32::from_rgb(255, 210, 90),
            );
        }
        self.docs[j].camera = camera;
    }

    /// Copie la sélection de l'onglet `j` sur la carte active, l'objet principal arrivant sur `target`.
    fn copy_from_tab(&mut self, j: usize, target: u32) {
        let Some(doc) = self.docs.get(j) else { return };
        let Some(cur) = &doc.current else { return };
        let indices = doc.selection.object_indices(doc.elevation);
        let objects: Vec<_> = indices
            .iter()
            .filter_map(|&i| cur.map.objects[doc.elevation].get(i).cloned())
            .collect();
        let Some(anchor) = doc
            .selection
            .primary()
            .and_then(|r| cur.map.objects[doc.elevation].get(r.index as usize))
            .map(|o| o.tile())
            .filter(|t| *t >= 0)
            .map(|t| t as u32)
        else {
            return;
        };
        if objects.is_empty() {
            return;
        }
        let n = objects.len();
        self.paste_objects(&Clipboard { objects, anchor }, target);
        self.status = format!("{n} {}", t("objet(s) copié(s) depuis l'autre carte"));
    }
}
