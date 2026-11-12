//! Éclairage : halo lumineux de chaque objet qui émet de la lumière (rayon et intensité des mots 13 et 14 de l'objet).

use crate::app::MapperApp;
use eframe::egui::{Color32, Mesh, Painter, Pos2, Rect, Shape, Vec2};
use fo_core::geom;
use fo_core::map::word;

/// Largeur d'un hexagone en pixels du monde : un rayon de 1 éclaire à peu près cette distance.
const HEX_PIXELS: f32 = 32.0;
/// Nombre d'anneaux du dégradé.
const RINGS: usize = 6;
/// Coins d'un hexagone de rayon 1 (en hexagones), en pixels du monde : la distance de 1 hexagone fait 32 en largeur et (16, 12) en diagonale.
const HEX_CORNERS: [(f32, f32); 6] = [
    (32.0, 0.0),
    (16.0, 12.0),
    (-16.0, 12.0),
    (-32.0, 0.0),
    (-16.0, -12.0),
    (16.0, -12.0),
];

/// Courbe douce de 0 à 1 (nulle en 0, 1 en 1, pente nulle aux deux bouts).
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}
const MAX_INTENSITY: f32 = 65_536.0;
/// Opacité maximale du halo au centre.
const PEAK: f32 = 0.30;

/// Halo : rayon en pixels du monde et opacité du centre (0 à 1), ou `None` si l'objet n'éclaire pas.
pub(crate) fn halo(distance: i32, intensity: i32) -> Option<(f32, f32)> {
    if distance <= 0 || intensity <= 0 {
        return None;
    }
    let strength = (intensity as f32 / MAX_INTENSITY).clamp(0.0, 1.0);
    Some((distance as f32 * HEX_PIXELS, PEAK * strength.sqrt()))
}

impl MapperApp {
    /// Dessine les halos dans la vue. Les couleurs sont pré-multipliées : alpha nul ajoute la lumière à l'image.
    pub(crate) fn draw_lights(&self, painter: &Painter, view: Rect, world_view: Rect) {
        let Some(cur) = self.current.as_ref() else {
            return;
        };
        let cam = &self.camera;
        let mut mesh = Mesh::default();
        for o in &cur.map.objects[self.elevation] {
            let t = o.tile();
            if !(0..geom::HEX_COUNT as i32).contains(&t) {
                continue;
            }
            let Some((radius, peak)) = halo(
                o.words[word::LIGHT_DISTANCE],
                o.words[word::LIGHT_INTENSITY],
            ) else {
                continue;
            };
            let (cx, cy) = geom::hex_to_screen(t as u32);
            let c = Pos2::new(cx as f32, cy as f32);
            if !world_view.expand(radius).contains(c) {
                continue;
            }
            let center = cam.world_to_screen(view, c);
            let warm = |a: f32| {
                Color32::from_rgba_premultiplied(
                    (255.0 * a) as u8,
                    (214.0 * a) as u8,
                    (140.0 * a) as u8,
                    0,
                )
            };
            // La portée est une distance en hexagones : son contour est un hexagone (pas 32 en largeur, (16, 12) en diagonale).
            // Le dégradé se fait sur plusieurs anneaux, avec une courbe douce (lissée aux deux bouts : ni disque brillant au
            // centre, ni bord net à la limite).
            let reach = radius / HEX_PIXELS + 1.0;
            let first = mesh.vertices.len() as u32;
            mesh.colored_vertex(center, warm(peak));
            for ring in 1..=RINGS {
                let t = ring as f32 / RINGS as f32;
                let level = peak * smooth(1.0 - t);
                for (dx, dy) in HEX_CORNERS {
                    mesh.colored_vertex(
                        center + Vec2::new(dx, dy) * (reach * t * cam.zoom),
                        warm(level),
                    );
                }
            }
            for k in 0..6u32 {
                mesh.add_triangle(first, first + 1 + k, first + 1 + (k + 1) % 6);
            }
            for ring in 1..RINGS as u32 {
                let (inner, outer) = (first + 1 + 6 * (ring - 1), first + 1 + 6 * ring);
                for k in 0..6u32 {
                    let k2 = (k + 1) % 6;
                    mesh.add_triangle(inner + k, outer + k, outer + k2);
                    mesh.add_triangle(inner + k, outer + k2, inner + k2);
                }
            }
        }
        painter.add(Shape::mesh(mesh));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_falloff_is_smooth_and_bounded() {
        assert_eq!((smooth(0.0), smooth(1.0)), (0.0, 1.0));
        assert!((smooth(0.5) - 0.5).abs() < 1e-6);
        assert!(smooth(0.05) < 0.05, "départ doux");
        assert!(smooth(-3.0) == 0.0 && smooth(7.0) == 1.0);
    }

    #[test]
    fn no_light_without_radius_or_intensity() {
        assert!(halo(0, 65_536).is_none());
        assert!(halo(4, 0).is_none());
        let (r, a) = halo(4, 65_536).unwrap();
        assert_eq!((r, a), (128.0, PEAK));
        assert!(halo(4, 1_000_000).unwrap().1 <= PEAK, "intensité bornée");
    }
}
