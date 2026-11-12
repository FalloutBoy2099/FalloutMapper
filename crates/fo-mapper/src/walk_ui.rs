//! Vue « Accessibilité » : sur la carte, les hexagones que le joueur peut atteindre à pied depuis son point de départ (vert),
//! ceux qui bloquent (rouge : murs, décors), ceux qu'une créature occupe (orange) et ceux qui sont libres mais hors d'atteinte
//! (bleu : pièce fermée, enclos).

use crate::app::MapperApp;
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use fo_core::geom;
use fo_core::reach::{self, Walk};

#[derive(Debug, Default)]
pub(crate) struct WalkState {
    pub on: bool,
    /// Empreinte de ce qui a servi au calcul (élévation, objets) : le calcul est refait quand elle change.
    key: u64,
    walk: Walk,
    seen: Vec<bool>,
}

impl MapperApp {
    fn walk_refresh(&mut self) {
        let (Some(cur), Some(g)) = (self.current.as_ref(), self.game.as_ref()) else {
            return;
        };
        let e = self.elevation;
        let mut key =
            (e as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ cur.map.header.player_tile as u64;
        for o in cur.map.objects.get(e).into_iter().flatten() {
            key = key.rotate_left(5)
                ^ (o.tile() as u64)
                    .wrapping_mul(31)
                    .wrapping_add(o.flags() as u64)
                    .wrapping_add((o.pid() as u64) << 20);
        }
        if key == self.walk.key && !self.walk.seen.is_empty() {
            return;
        }
        self.walk.walk = reach::walk_map(&cur.map, e, &g.data);
        let start = (cur.map.header.player_elevation as usize == e)
            .then_some(cur.map.header.player_tile)
            .filter(|t| (0..geom::HEX_COUNT as i32).contains(t));
        self.walk.seen = start.map_or_else(
            || vec![false; geom::HEX_COUNT as usize],
            |t| reach::reachable(&self.walk.walk.blocked, t as u32),
        );
        self.walk.key = key;
    }

    pub(crate) fn draw_walk(&mut self, painter: &egui::Painter, view: Rect, world_view: Rect) {
        self.walk_refresh();
        let w = &self.walk;
        // Sans point de départ sur cette élévation, on ne sait pas ce qui est atteignable : seulement les blocages.
        let known = w.seen.iter().any(|b| *b);
        let cam = &self.camera;
        for tile in 0..geom::HEX_COUNT {
            let (cx, cy) = geom::hex_to_screen(tile);
            let c = Pos2::new(cx as f32, cy as f32);
            if !world_view.expand(32.0).contains(c) {
                continue;
            }
            let i = tile as usize;
            let color = if w.walk.blocked[i] {
                Color32::from_rgba_unmultiplied(230, 60, 50, 110)
            } else if w.walk.critters[i] {
                Color32::from_rgba_unmultiplied(240, 150, 40, 120)
            } else if !known {
                continue;
            } else if w.seen[i] {
                Color32::from_rgba_unmultiplied(70, 200, 90, 34)
            } else {
                Color32::from_rgba_unmultiplied(70, 130, 255, 90)
            };
            let pts: Vec<Pos2> = crate::edit_ui::HEX_OUTLINE
                .iter()
                .map(|(dx, dy)| cam.world_to_screen(view, c + Vec2::new(*dx, *dy)))
                .collect();
            painter.add(egui::Shape::convex_polygon(pts, color, egui::Stroke::NONE));
        }
    }
}
