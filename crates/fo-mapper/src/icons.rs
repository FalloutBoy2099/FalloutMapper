//! Icônes vectorielles dessinées au pinceau (indépendantes des polices, nettes à toutes les échelles).

use eframe::egui::{self, Color32, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Vec2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Icon {
    Map,
    World,
    Script,
    Save,
    Play,
    Edit,
    Undo,
    Redo,
    Select,
    Floor,
    Roof,
    Place,
    Start,
    Exit,
    Blocker,
    Stamp,
    Layers,
    Markers,
    Grid,
    Edges,
    Anim,
    Fit,
    Wall,
    Scenery,
    Item,
    Critter,
    Scripted,
    Light,
    Room,
    Preview,
    Spatial,
    Lock,
}

/// Dessine `icon` dans `rect` (coordonnées normalisées 0..1 à l'intérieur d'un carré centré).
pub(crate) fn paint(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let side = rect.width().min(rect.height());
    let sq = Rect::from_center_size(rect.center(), Vec2::splat(side));
    let w = (side / 11.0).clamp(1.3, 2.4);
    let st = Stroke::new(w, color);
    let pt = |x: f32, y: f32| Pos2::new(sq.min.x + x * sq.width(), sq.min.y + y * sq.height());
    let line = |pts: &[(f32, f32)]| {
        p.add(Shape::line(
            pts.iter().map(|&(x, y)| pt(x, y)).collect(),
            st,
        ))
    };
    let closed = |pts: &[(f32, f32)]| {
        p.add(Shape::closed_line(
            pts.iter().map(|&(x, y)| pt(x, y)).collect(),
            st,
        ))
    };
    let filled = |pts: &[(f32, f32)]| {
        p.add(Shape::convex_polygon(
            pts.iter().map(|&(x, y)| pt(x, y)).collect(),
            color,
            Stroke::NONE,
        ))
    };
    let r = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(pt(x0, y0), pt(x1, y1));
    use std::f32::consts::PI;
    match icon {
        Icon::Map => {
            closed(&[
                (0.1, 0.22),
                (0.37, 0.12),
                (0.63, 0.22),
                (0.9, 0.12),
                (0.9, 0.78),
                (0.63, 0.88),
                (0.37, 0.78),
                (0.1, 0.88),
            ]);
            line(&[(0.37, 0.12), (0.37, 0.78)]);
            line(&[(0.63, 0.22), (0.63, 0.88)]);
        }
        Icon::World => {
            p.circle_stroke(sq.center(), side * 0.4, st);
            line(&[(0.1, 0.5), (0.9, 0.5)]);
            let meridian: Vec<(f32, f32)> = (0..=16)
                .map(|i| {
                    let a = std::f32::consts::PI * i as f32 / 16.0;
                    (0.5 + 0.2 * a.sin(), 0.1 + 0.8 * (a / std::f32::consts::PI))
                })
                .collect();
            line(&meridian);
            let meridian2: Vec<(f32, f32)> = (0..=16)
                .map(|i| {
                    let a = std::f32::consts::PI * i as f32 / 16.0;
                    (0.5 - 0.2 * a.sin(), 0.1 + 0.8 * (a / std::f32::consts::PI))
                })
                .collect();
            line(&meridian2);
        }
        Icon::Script => {
            closed(&[
                (0.2, 0.08),
                (0.62, 0.08),
                (0.82, 0.28),
                (0.82, 0.92),
                (0.2, 0.92),
            ]);
            line(&[(0.62, 0.08), (0.62, 0.28), (0.82, 0.28)]);
            line(&[(0.32, 0.5), (0.7, 0.5)]);
            line(&[(0.32, 0.65), (0.7, 0.65)]);
            line(&[(0.32, 0.8), (0.58, 0.8)]);
        }
        Icon::Save => {
            closed(&[
                (0.12, 0.12),
                (0.78, 0.12),
                (0.9, 0.24),
                (0.9, 0.9),
                (0.12, 0.9),
            ]);
            p.rect_stroke(r(0.3, 0.12, 0.68, 0.38), 0.0, st, egui::StrokeKind::Middle);
            p.rect_stroke(r(0.26, 0.55, 0.74, 0.9), 0.0, st, egui::StrokeKind::Middle);
        }
        Icon::Play => {
            filled(&[(0.25, 0.12), (0.88, 0.5), (0.25, 0.88)]);
        }
        Icon::Edit => {
            closed(&[
                (0.15, 0.85),
                (0.2, 0.62),
                (0.68, 0.14),
                (0.86, 0.32),
                (0.38, 0.8),
            ]);
            line(&[(0.58, 0.24), (0.76, 0.42)]);
            line(&[(0.2, 0.62), (0.38, 0.8)]);
        }
        Icon::Undo | Icon::Redo => {
            let flip = icon == Icon::Redo;
            let fx = |x: f32| if flip { 1.0 - x } else { x };
            let pts: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let a = PI * 1.05 + (PI * 1.15) * i as f32 / 16.0;
                    pt(fx(0.5 + 0.34 * a.cos()), 0.62 + 0.34 * a.sin())
                })
                .collect();
            p.add(Shape::line(pts, st));
            filled(&[(fx(0.1), 0.2), (fx(0.1), 0.62), (fx(0.5), 0.46)]);
        }
        Icon::Select => {
            filled(&[
                (0.22, 0.1),
                (0.22, 0.84),
                (0.42, 0.66),
                (0.56, 0.92),
                (0.68, 0.86),
                (0.54, 0.6),
                (0.8, 0.58),
            ]);
        }
        Icon::Floor => {
            closed(&[(0.5, 0.15), (0.92, 0.5), (0.5, 0.85), (0.08, 0.5)]);
            line(&[(0.29, 0.325), (0.71, 0.675)]);
            line(&[(0.71, 0.325), (0.29, 0.675)]);
        }
        Icon::Roof => {
            closed(&[(0.08, 0.52), (0.5, 0.14), (0.92, 0.52)]);
            closed(&[(0.22, 0.46), (0.22, 0.86), (0.78, 0.86), (0.78, 0.46)]);
            line(&[(0.42, 0.86), (0.42, 0.62), (0.58, 0.62), (0.58, 0.86)]);
        }
        Icon::Place => {
            closed(&[
                (0.5, 0.1),
                (0.86, 0.28),
                (0.86, 0.68),
                (0.5, 0.86),
                (0.14, 0.68),
                (0.14, 0.28),
            ]);
            line(&[(0.5, 0.1), (0.5, 0.48), (0.86, 0.28)]);
            line(&[(0.5, 0.48), (0.14, 0.28)]);
        }
        Icon::Start => {
            line(&[(0.25, 0.9), (0.25, 0.1)]);
            filled(&[(0.25, 0.12), (0.85, 0.3), (0.25, 0.5)]);
        }
        Icon::Exit => {
            line(&[(0.42, 0.14), (0.14, 0.14), (0.14, 0.86), (0.42, 0.86)]);
            line(&[(0.34, 0.5), (0.84, 0.5)]);
            filled(&[(0.9, 0.5), (0.68, 0.3), (0.68, 0.7)]);
        }
        Icon::Blocker => {
            p.circle_stroke(sq.center(), side * 0.38, st);
            line(&[(0.23, 0.77), (0.77, 0.23)]);
        }
        Icon::Stamp => {
            p.rect_stroke(r(0.1, 0.28, 0.66, 0.9), 2.0, st, egui::StrokeKind::Middle);
            line(&[
                (0.34, 0.28),
                (0.34, 0.1),
                (0.9, 0.1),
                (0.9, 0.72),
                (0.66, 0.72),
            ]);
        }
        Icon::Layers => {
            closed(&[(0.5, 0.12), (0.92, 0.34), (0.5, 0.56), (0.08, 0.34)]);
            line(&[(0.08, 0.52), (0.5, 0.74), (0.92, 0.52)]);
            line(&[(0.08, 0.7), (0.5, 0.92), (0.92, 0.7)]);
        }
        Icon::Markers => {
            p.circle_stroke(sq.center(), side * 0.36, st);
            p.circle_filled(sq.center(), side * 0.1, color);
            line(&[(0.5, 0.02), (0.5, 0.2)]);
            line(&[(0.5, 0.8), (0.5, 0.98)]);
            line(&[(0.02, 0.5), (0.2, 0.5)]);
            line(&[(0.8, 0.5), (0.98, 0.5)]);
        }
        Icon::Grid => {
            closed(&[
                (0.5, 0.06),
                (0.88, 0.28),
                (0.88, 0.72),
                (0.5, 0.94),
                (0.12, 0.72),
                (0.12, 0.28),
            ]);
        }
        Icon::Edges => {
            for (a, b) in [
                ((0.1, 0.12), (0.4, 0.12)),
                ((0.6, 0.12), (0.9, 0.12)),
                ((0.1, 0.88), (0.4, 0.88)),
                ((0.6, 0.88), (0.9, 0.88)),
                ((0.1, 0.12), (0.1, 0.4)),
                ((0.1, 0.6), (0.1, 0.88)),
                ((0.9, 0.12), (0.9, 0.4)),
                ((0.9, 0.6), (0.9, 0.88)),
            ] {
                line(&[a, b]);
            }
        }
        Icon::Anim => {
            p.circle_stroke(sq.center(), side * 0.38, st);
            let half: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let a = -PI / 2.0 + PI * i as f32 / 16.0;
                    pt(0.5 + 0.38 * a.cos(), 0.5 + 0.38 * a.sin())
                })
                .collect();
            p.add(Shape::convex_polygon(half, color, Stroke::NONE));
        }
        Icon::Lock => {
            // Un cadenas : l'anse au-dessus, le corps en dessous.
            closed(&[(0.2, 0.46), (0.8, 0.46), (0.8, 0.9), (0.2, 0.9)]);
            line(&[(0.32, 0.46), (0.32, 0.3), (0.38, 0.14), (0.5, 0.1), (0.62, 0.14), (0.68, 0.3), (0.68, 0.46)]);
            p.circle_filled(pt(0.5, 0.68), side * 0.07, color);
        }
        Icon::Spatial => {
            // Un disque pointillé autour d'un centre : la zone qui déclenche le script.
            p.circle_stroke(pt(0.5, 0.5), side * 0.4, st);
            p.circle_filled(pt(0.5, 0.5), side * 0.09, color);
            line(&[(0.5, 0.5), (0.82, 0.3)]);
        }
        Icon::Wall => {
            closed(&[(0.1, 0.4), (0.5, 0.2), (0.5, 0.8), (0.1, 0.6)]);
            closed(&[(0.5, 0.2), (0.9, 0.4), (0.9, 0.6), (0.5, 0.8)]);
        }
        Icon::Scenery => {
            line(&[(0.5, 0.92), (0.5, 0.5)]);
            p.circle_stroke(pt(0.5, 0.32), side * 0.24, st);
            line(&[(0.5, 0.75), (0.3, 0.6)]);
        }
        Icon::Item => {
            closed(&[(0.5, 0.1), (0.9, 0.3), (0.5, 0.5), (0.1, 0.3)]);
            line(&[(0.1, 0.3), (0.1, 0.7), (0.5, 0.9), (0.9, 0.7), (0.9, 0.3)]);
            line(&[(0.5, 0.5), (0.5, 0.9)]);
        }
        Icon::Critter => {
            p.circle_stroke(pt(0.5, 0.25), side * 0.15, st);
            line(&[(0.5, 0.4), (0.5, 0.7)]);
            line(&[(0.2, 0.5), (0.8, 0.5)]);
            line(&[(0.5, 0.7), (0.3, 0.92)]);
            line(&[(0.5, 0.7), (0.7, 0.92)]);
        }
        Icon::Preview => {
            // Un œil.
            line(&[
                (0.05, 0.5),
                (0.25, 0.3),
                (0.5, 0.22),
                (0.75, 0.3),
                (0.95, 0.5),
                (0.75, 0.7),
                (0.5, 0.78),
                (0.25, 0.7),
                (0.05, 0.5),
            ]);
            p.circle_stroke(sq.center(), side * 0.15, st);
            p.circle_filled(sq.center(), side * 0.06, color);
        }
        Icon::Room => {
            line(&[(0.1, 0.55), (0.5, 0.2), (0.9, 0.55)]);
            line(&[(0.5, 0.2), (0.5, 0.82)]);
            line(&[
                (0.1, 0.55),
                (0.1, 0.82),
                (0.5, 0.82),
                (0.9, 0.82),
                (0.9, 0.55),
            ]);
        }
        Icon::Light => {
            p.circle_stroke(sq.center(), side * 0.2, st);
            for k in 0..8 {
                let a = PI / 4.0 * k as f32;
                line(&[
                    (0.5 + 0.32 * a.cos(), 0.5 + 0.32 * a.sin()),
                    (0.5 + 0.46 * a.cos(), 0.5 + 0.46 * a.sin()),
                ]);
            }
        }
        Icon::Scripted => {
            line(&[(0.35, 0.2), (0.12, 0.5), (0.35, 0.8)]);
            line(&[(0.65, 0.2), (0.88, 0.5), (0.65, 0.8)]);
            line(&[(0.56, 0.12), (0.44, 0.88)]);
        }
        Icon::Fit => {
            for (cx, cy, dx, dy) in [
                (0.12, 0.12, 1.0, 1.0),
                (0.88, 0.12, -1.0, 1.0),
                (0.12, 0.88, 1.0, -1.0),
                (0.88, 0.88, -1.0, -1.0),
            ] {
                line(&[(cx + dx * 0.28, cy), (cx, cy), (cx, cy + dy * 0.28)]);
            }
        }
    }
}

/// Bouton d'icône : fond et survol comme un bouton egui, icône vectorielle au centre.
pub(crate) fn icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    size: Vec2,
    selected: bool,
    enabled: bool,
) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        let v = ui.style().interact_selectable(&resp, selected);
        if selected || resp.hovered() || resp.is_pointer_button_down_on() {
            ui.painter().rect(
                rect,
                v.corner_radius,
                v.weak_bg_fill,
                v.bg_stroke,
                egui::StrokeKind::Inside,
            );
        }
        let mut color = v.fg_stroke.color;
        if !enabled {
            color = color.gamma_multiply(0.35);
        }
        paint(
            ui.painter(),
            Rect::from_center_size(rect.center(), Vec2::splat(size.y * 0.58)),
            icon,
            color,
        );
    }
    resp
}
