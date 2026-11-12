//! Caméra 2D (déplacement + zoom) sur le repère « monde » de la carte.

use eframe::egui::{Pos2, Rect, Vec2};

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    /// Point du monde au centre de la vue.
    pub center: Pos2,
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            center: Pos2::new(4000.0, 1800.0),
            zoom: 0.5,
        }
    }
}

impl Camera {
    pub const MIN_ZOOM: f32 = 0.05;
    pub const MAX_ZOOM: f32 = 8.0;

    pub fn world_to_screen(&self, view: Rect, p: Pos2) -> Pos2 {
        view.center() + (p - self.center) * self.zoom
    }

    pub fn screen_to_world(&self, view: Rect, p: Pos2) -> Pos2 {
        self.center + (p - view.center()) / self.zoom
    }

    pub fn pan_by_screen(&mut self, delta: Vec2) {
        self.center -= delta / self.zoom;
    }

    /// Zoome en gardant fixe le point du monde situé sous `anchor`.
    pub fn zoom_at(&mut self, view: Rect, anchor: Pos2, factor: f32) {
        let before = self.screen_to_world(view, anchor);
        self.zoom = (self.zoom * factor).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        let after = self.screen_to_world(view, anchor);
        self.center += before - after;
    }

    pub fn fit(&mut self, view: Rect, bounds: (i32, i32, i32, i32)) {
        let (x, y, w, h) = bounds;
        let zx = view.width() / w.max(1) as f32;
        let zy = view.height() / h.max(1) as f32;
        self.zoom = (zx.min(zy) * 0.95).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        self.center = Pos2::new(x as f32 + w as f32 / 2.0, y as f32 + h as f32 / 2.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_anchor_fixed() {
        let view = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let mut cam = Camera::default();
        let anchor = Pos2::new(123.0, 456.0);
        let before = cam.screen_to_world(view, anchor);
        cam.zoom_at(view, anchor, 1.7);
        let after = cam.screen_to_world(view, anchor);
        assert!((before - after).length() < 0.01);
    }
}
