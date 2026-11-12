//! Aperçu « comme dans le jeu » : éclairage de la carte (lumière ambiante + lumières des objets, hexagone par hexagone), toits
//! masqués autour du point de vue, sans les repères d'édition, et cadre de l'écran du jeu (640 × 480).
//!
//! La lumière ambiante vient de l'heure du jeu, pas du fichier de la carte : l'aperçu propose donc jour, soir et nuit.
//! Chaque lumière éclaire les hexagones jusqu'à son rayon, de moins en moins (décroissance linéaire avec la distance).

use crate::app::MapperApp;
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use fo_core::geom;
use fo_core::map::word;
use fo_scene::{DrawItem, Layer};

/// Luminosité ambiante des trois préréglages.
pub(crate) const DAY: f32 = 1.0;
pub(crate) const EVENING: f32 = 0.55;
pub(crate) const NIGHT: f32 = 0.2;
/// Luminosité minimale : même en pleine nuit, on devine les formes.
const MIN_LIGHT: f32 = 0.08;
/// Écran du jeu d'origine, en pixels du monde.
const GAME_VIEW: Vec2 = Vec2::new(640.0, 480.0);

#[derive(Debug, Clone)]
pub(crate) struct PreviewState {
    pub on: bool,
    pub ambient: f32,
    /// Masquer les toits autour du point de vue (comme le jeu quand on entre dans un bâtiment).
    pub hide_roofs: bool,
    /// Rayon, en hexagones, de la zone sans toit.
    pub roof_radius: i32,
    /// Le point de vue suit le curseur ; sinon, le point de départ du joueur.
    pub follow_cursor: bool,
    /// Cadre de l'écran du jeu autour du point de vue.
    pub frame: bool,
    /// Lumières de la carte à l'origine du champ calculé : (hexagone, rayon, intensité) et élévation.
    lights: Vec<(u32, i32, i32)>,
    elevation: usize,
    last_ambient: f32,
    field: Vec<f32>,
}

impl Default for PreviewState {
    fn default() -> Self {
        Self {
            on: false,
            ambient: EVENING,
            hide_roofs: true,
            roof_radius: 12,
            follow_cursor: false,
            frame: false,
            lights: Vec::new(),
            elevation: usize::MAX,
            last_ambient: -1.0,
            field: Vec::new(),
        }
    }
}

/// Lumière reçue par chaque hexagone (0 à 1), ambiante comprise.
pub(crate) fn light_field(lights: &[(u32, i32, i32)], ambient: f32) -> Vec<f32> {
    let mut field = vec![ambient.clamp(MIN_LIGHT, 1.0); geom::HEX_COUNT as usize];
    for &(hex, radius, intensity) in lights {
        if radius <= 0 || intensity <= 0 {
            continue;
        }
        let strength = (intensity as f32 / 65_536.0).clamp(0.0, 1.0);
        let (c0, r0) = geom::hex_col_row(hex);
        let reach = radius as i64;
        for r in
            (r0 as i64 - reach).max(0)..=(r0 as i64 + reach).min(geom::HEX_GRID_WIDTH as i64 - 1)
        {
            for c in (c0 as i64 - reach).max(0)
                ..=(c0 as i64 + reach).min(geom::HEX_GRID_WIDTH as i64 - 1)
            {
                let h = r as u32 * geom::HEX_GRID_WIDTH + c as u32;
                let d = geom::hex_distance(hex, h);
                if d <= radius {
                    let cell = &mut field[h as usize];
                    *cell = (*cell + strength * (1.0 - d as f32 / (radius as f32 + 1.0))).min(1.0);
                }
            }
        }
    }
    field
}

/// Lumières d'une élévation : (hexagone, rayon, intensité) de chaque objet qui éclaire.
pub(crate) fn map_lights(map: &fo_core::map::Map, elevation: usize) -> Vec<(u32, i32, i32)> {
    map.objects
        .get(elevation)
        .into_iter()
        .flatten()
        .filter(|o| {
            (0..geom::HEX_COUNT as i32).contains(&o.tile())
                && o.words[word::LIGHT_DISTANCE] > 0
                && o.words[word::LIGHT_INTENSITY] > 0
        })
        .map(|o| {
            (
                o.tile() as u32,
                o.words[word::LIGHT_DISTANCE],
                o.words[word::LIGHT_INTENSITY],
            )
        })
        .collect()
}

/// Teinte des sprites ordinaires ; les transparences sont composées et éclairées par les tables de palette.
pub(crate) fn lit_tint(item: &DrawItem, light: f32) -> Color32 {
    let a = item.alpha as f32;
    let k = light.clamp(0.0, 1.0);
    match item.blend {
        fo_scene::Blend::Normal => Color32::from_rgba_premultiplied(
            (a * k) as u8,
            (a * k) as u8,
            (a * k) as u8,
            item.alpha,
        ),
        fo_scene::Blend::Palette(_) => Color32::WHITE, // Already lit by the palette compositor.
    }
}

/// Hexagone d'un élément dessiné : celui de l'objet, ou celui du centre de sa case de sol ou de toit.
pub(crate) fn item_hex(item: &DrawItem) -> Option<u32> {
    if item.object.is_some() {
        return (item.tile < geom::HEX_COUNT).then_some(item.tile);
    }
    let (x, y) = geom::square_to_screen(item.tile);
    geom::screen_to_hex(x + 40, y + 18)
}

/// Ce que le canevas a besoin de savoir pour dessiner un aperçu.
pub(crate) struct PreviewView {
    pub viewpoint: Option<Pos2>,
    pub roof_radius_px: Vec2,
    pub hide_roofs: bool,
}

impl PreviewView {
    /// Un toit est caché quand son centre est dans l'ellipse (les hexagones sont deux fois plus larges que hauts à l'écran).
    pub(crate) fn hides(&self, item: &DrawItem) -> bool {
        if !self.hide_roofs || item.layer != Layer::Roof {
            return false;
        }
        let Some(vp) = self.viewpoint else {
            return false;
        };
        // Centre de la case de sol sous le toit.
        let (x, y) = geom::square_to_screen(item.tile);
        let (dx, dy) = (
            (x as f32 + 40.0 - vp.x) / self.roof_radius_px.x,
            (y as f32 + 18.0 - vp.y) / self.roof_radius_px.y,
        );
        dx * dx + dy * dy < 1.0
    }

    /// Adoucit les murs au premier plan près du point de vue, inspiré de l'effet
    /// local du jeu. L'effet ne s'applique qu'à l'aperçu, jamais à la carte éditée.
    pub(crate) fn wall_alpha(&self, item: &DrawItem) -> u8 {
        if item.layer != Layer::Wall {
            return 255;
        }
        let Some(vp) = self.viewpoint else { return 255 };
        let (x, y) = geom::hex_to_screen(item.tile);
        let (dx, dy) = (x as f32 - vp.x, y as f32 - vp.y);
        // Le mur doit être devant le point de vue et dans un œuf de quelques hexagones.
        if dy < 0.0 {
            return 255;
        }
        let d2 = (dx / 64.0).powi(2) + (dy / 32.0).powi(2);
        if d2 >= 1.0 {
            255
        } else {
            // Bord progressif : évite une découpe nette des murs autour du joueur.
            (120.0 + 135.0 * d2.sqrt()).round() as u8
        }
    }
}

impl MapperApp {
    /// Prépare l'aperçu de cette image : recalcule la lumière si les lumières ont changé, renvoie le point de vue.
    pub(crate) fn preview_prepare(&mut self) -> Option<PreviewView> {
        if !self.preview.on {
            return None;
        }
        let cur = self.current.as_ref()?;
        let e = self.elevation;
        let lights = map_lights(&cur.map, e);
        let ambient = self.preview.ambient;
        if self.preview.elevation != e
            || self.preview.lights != lights
            || self.preview.field.is_empty()
            || self.preview.last_ambient != ambient
        {
            self.preview.field = light_field(&lights, ambient);
            self.preview.lights = lights;
            self.preview.elevation = e;
            self.preview.last_ambient = ambient;
        }
        let start = (cur.map.header.player_elevation as usize == e)
            .then_some(cur.map.header.player_tile)
            .filter(|t| (0..geom::HEX_COUNT as i32).contains(t))
            .map(|t| t as u32);
        let hex = if self.preview.follow_cursor {
            self.hover_hex.or(start)
        } else {
            start
        };
        let viewpoint = hex.map(|h| {
            let (x, y) = geom::hex_to_screen(h);
            Pos2::new(x as f32, y as f32)
        });
        let r = self.preview.roof_radius.max(1) as f32;
        Some(PreviewView {
            viewpoint,
            roof_radius_px: Vec2::new(r * 32.0, r * 16.0),
            hide_roofs: self.preview.hide_roofs,
        })
    }

    /// Lumière reçue par un élément (1 si l'aperçu est éteint).
    pub(crate) fn preview_light(&self, item: &DrawItem) -> f32 {
        item_hex(item)
            .and_then(|h| self.preview.field.get(h as usize).copied())
            .unwrap_or(self.preview.ambient)
    }

    /// Cadre de l'écran du jeu autour du point de vue : tout le reste est assombri.
    pub(crate) fn draw_game_frame(&self, painter: &egui::Painter, view: Rect, v: &PreviewView) {
        if !self.preview.frame {
            return;
        }
        let Some(vp) = v.viewpoint else { return };
        let cam = &self.camera;
        let half = GAME_VIEW / 2.0;
        let frame = Rect::from_min_max(
            cam.world_to_screen(view, Pos2::new(vp.x - half.x, vp.y - half.y)),
            cam.world_to_screen(view, Pos2::new(vp.x + half.x, vp.y + half.y)),
        );
        let shade = Color32::from_black_alpha(170);
        let outside = [
            Rect::from_min_max(
                view.min,
                Pos2::new(view.max.x, frame.min.y.clamp(view.min.y, view.max.y)),
            ),
            Rect::from_min_max(
                Pos2::new(view.min.x, frame.max.y.clamp(view.min.y, view.max.y)),
                view.max,
            ),
            Rect::from_min_max(
                Pos2::new(view.min.x, frame.min.y.clamp(view.min.y, view.max.y)),
                Pos2::new(
                    frame.min.x.clamp(view.min.x, view.max.x),
                    frame.max.y.clamp(view.min.y, view.max.y),
                ),
            ),
            Rect::from_min_max(
                Pos2::new(
                    frame.max.x.clamp(view.min.x, view.max.x),
                    frame.min.y.clamp(view.min.y, view.max.y),
                ),
                Pos2::new(view.max.x, frame.max.y.clamp(view.min.y, view.max.y)),
            ),
        ];
        for r in outside {
            if r.width() > 0.0 && r.height() > 0.0 {
                painter.rect_filled(r, 0.0, shade);
            }
        }
        painter.rect_stroke(
            frame,
            0.0,
            egui::Stroke::new(1.5, Color32::from_rgb(255, 200, 80)),
            egui::StrokeKind::Outside,
        );
    }

    /// Réglages de l'aperçu (menu Affichage).
    pub(crate) fn preview_menu(&mut self, ui: &mut egui::Ui) {
        use crate::i18n::t;
        ui.add(egui::Checkbox::new(
            &mut self.preview.on,
            t("Aperçu du jeu"),
        ));
        ui.weak(format!(
            "{} : {}",
            t("Raccourci"),
            self.prefs.shortcut_text(crate::prefs::KeyAction::Preview)
        ));
        ui.add_enabled_ui(self.preview.on, |ui| {
            ui.horizontal(|ui| {
                for (label, v) in [("Jour", DAY), ("Soir", EVENING), ("Nuit", NIGHT)] {
                    if ui
                        .selectable_label((self.preview.ambient - v).abs() < 0.01, t(label))
                        .clicked()
                    {
                        self.preview.ambient = v;
                    }
                }
            });
            ui.add(
                egui::Slider::new(&mut self.preview.ambient, MIN_LIGHT..=1.0)
                    .text(t("Lumière ambiante"))
                    .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
            );
            ui.checkbox(
                &mut self.preview.hide_roofs,
                t("Masquer les toits autour du point de vue"),
            );
            ui.add(
                egui::Slider::new(&mut self.preview.roof_radius, 4..=30)
                    .text(t("Rayon sans toit (hexagones)")),
            );
            ui.checkbox(
                &mut self.preview.follow_cursor,
                t("Point de vue : le curseur (sinon le départ du joueur)"),
            );
            ui.checkbox(
                &mut self.preview.frame,
                t("Cadre de l'écran du jeu (640 × 480)"),
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambient_sets_the_floor_and_a_light_brightens_around_itself() {
        let hex = 100 * 200 + 100;
        let f = light_field(&[(hex, 4, 65_536)], 0.2);
        assert!(
            (f[0] - 0.2).abs() < 1e-6,
            "loin de toute lumière : l'ambiante"
        );
        let near = f[hex as usize];
        let far = f[(hex + 3) as usize];
        assert!(
            near > far && far > 0.2,
            "plus clair au centre : {near} {far}"
        );
        assert!(
            (near - 1.0).abs() < 1e-3 || near > 0.9,
            "intensité maximale au centre : {near}"
        );
        assert!(
            (f[(hex + 5) as usize] - 0.2).abs() < 1e-6,
            "au-delà du rayon"
        );
        // Trop sombre : le plancher de luminosité.
        assert!((light_field(&[], 0.0)[7] - MIN_LIGHT).abs() < 1e-6);
        // Des lumières qui se chevauchent s'additionnent sans dépasser 1.
        let two = light_field(
            &[(hex, 6, 65_536), (hex + 1, 6, 65_536), (hex + 2, 6, 65_536)],
            0.5,
        );
        assert!(two.iter().all(|v| *v <= 1.0));
        // Une demi-intensité éclaire moitié moins.
        let half = light_field(&[(hex, 4, 32_768)], 0.0)[hex as usize];
        let full = light_field(&[(hex, 4, 65_536)], 0.0)[hex as usize];
        assert!(half < full);
    }
}
