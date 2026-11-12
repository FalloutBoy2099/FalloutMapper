use crate::{app::MapperApp, i18n::t};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke};
use fo_core::room::{
    Axis, DecorOptions, RoomPlan, join_wall_corners, opening_at, plan_room_with, plan_wall_run,
};
use fo_core::{ObjectType, geom};

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WallMode {
    #[default]
    Line,
    Rectangle,
    Door,
    Window,
}
#[derive(Default)]
pub(crate) struct WallTool {
    pub mode: WallMode,
    pub start: Option<u32>,
}

impl MapperApp {
    pub(crate) fn wall_panel(&mut self, ui: &mut egui::Ui) {
        if self.analysis.result.is_none() {
            if self.analysis.running() {
                ui.spinner();
            } else if ui.button(t("Analyser toutes les cartes")).clicked() {
                self.start_tile_analysis(ui.ctx());
            }
            return;
        }
        self.refresh_room_models();
        self.construction_style_ui(ui);
        for (mode, label) in [
            (WallMode::Line, "Ligne"),
            (WallMode::Rectangle, "Rectangle"),
            (WallMode::Door, "Porte"),
            (WallMode::Window, "Fenêtre"),
        ] {
            if ui
                .selectable_value(&mut self.edit.wall.mode, mode, t(label))
                .changed()
            {
                self.edit.wall.start = None;
            }
        }
        ui.label(t(
            "Deux clics pour tracer. Pour une ouverture, cliquez sur le mur à modifier.",
        ));
        if ui.button(t("Annuler le tracé")).clicked() {
            self.edit.wall.start = None;
        }
    }

    fn wall_gesture_plan(&self, from: u32, to: u32) -> Option<RoomPlan> {
        let model = self.room_model()?;
        let ((ac, ar), (bc, br)) = (geom::hex_col_row(from), geom::hex_col_row(to));
        if self.edit.wall.mode == WallMode::Rectangle {
            return Some(plan_room_with(
                &model,
                ar.min(br) * 200 + ac.min(bc),
                ar.abs_diff(br).max(3),
                ac.abs_diff(bc).max(3),
                &DecorOptions {
                    front: true,
                    ..Default::default()
                },
            ));
        }
        let axis = if ac.abs_diff(bc) >= ar.abs_diff(br) {
            Axis::Col
        } else {
            Axis::Row
        };
        let (start, len) = if axis == Axis::Col {
            (ar * 200 + ac.min(bc), ac.abs_diff(bc) + 1)
        } else {
            (ar.min(br) * 200 + ac, ar.abs_diff(br) + 1)
        };
        Some(plan_wall_run(&model, start, axis, len))
    }

    pub(crate) fn wall_input(&mut self, response: &egui::Response) {
        self.refresh_room_models();
        if !response.clicked_by(egui::PointerButton::Primary) {
            return;
        }
        let Some(hex) = self.hover_hex else { return };
        let Some(model) = self.room_model() else {
            self.status = t("Analysez d'abord toutes les cartes.").into();
            return;
        };
        let e = self.elevation;
        let old: Vec<_> = self
            .current
            .as_ref()
            .map(|c| {
                c.map.objects[e]
                    .iter()
                    .filter(|o| o.object_type() == Some(ObjectType::Wall) && o.tile() >= 0)
                    .map(|o| (o.tile() as u32, o.fid()))
                    .collect()
            })
            .unwrap_or_default();
        let plan = if matches!(self.edit.wall.mode, WallMode::Door | WallMode::Window) {
            let Some(plan) = opening_at(&model, &old, hex, self.edit.wall.mode == WallMode::Window)
            else {
                self.status = t("Aucune ouverture compatible avec les raccords de ce mur.").into();
                return;
            };
            plan
        } else {
            let Some(start) = self.edit.wall.start.take() else {
                self.edit.wall.start = Some(hex);
                return;
            };
            let Some(mut plan) = self.wall_gesture_plan(start, hex) else {
                return;
            };
            let changed: Vec<_> = plan.walls.iter().map(|p| p.0).collect();
            let mut joined = old.clone();
            joined.retain(|p| !changed.contains(&p.0));
            joined.extend(plan.walls);
            join_wall_corners(&model, &mut joined, &changed);
            plan.walls = joined;
            plan
        };
        if plan.short {
            self.status = t("Tracé impossible : raccords incomplets pour ce style.").into();
            return;
        }
        if self.current.as_ref().is_some_and(|c| {
            plan.doors.iter().any(|p| {
                c.map.objects[e].iter().any(|o| {
                    o.tile() == p.hex as i32
                        && o.object_type() == Some(ObjectType::Scenery)
                        && o.subtype == fo_core::formats::scenery_kind::DOOR
                })
            })
        }) {
            self.status = t("Une porte occupe déjà cet emplacement.").into();
            return;
        }
        let mut additions = plan.clone();
        additions.walls.retain(|p| !old.contains(p));
        if self.game.as_ref().is_none_or(|g| {
            additions
                .walls
                .iter()
                .map(|p| p.1)
                .chain(additions.doors.iter().map(|p| p.fid))
                .any(|fid| {
                    model
                        .pid_of(fid)
                        .and_then(|pid| g.data.proto_info(pid))
                        .is_none()
                })
        }) {
            self.status = t("Tracé annulé : prototype introuvable.").into();
            return;
        }
        let removed: Vec<_> = old
            .iter()
            .filter(|p| !plan.walls.contains(p))
            .copied()
            .collect();
        if self.current.as_ref().is_some_and(|c| {
            c.map.objects[e]
                .iter()
                .any(|o| removed.contains(&(o.tile() as u32, o.fid())) && o.script_id() >= 0)
        }) {
            self.status = t("Le tracé toucherait un mur scripté : opération annulée.").into();
            return;
        }
        let mut counts = Default::default();
        self.mutate_as("Construction de murs", |map, game| {
            map.objects[e].retain(|o| {
                o.object_type() != Some(ObjectType::Wall)
                    || !removed.contains(&(o.tile() as u32, o.fid()))
            });
            fo_core::build::place_plan(map, game, &model, e, &additions, &mut counts);
            !removed.is_empty() || counts.placed + counts.doors > 0
        });
        self.status = t("Murs et raccords mis à jour.").into();
    }

    pub(crate) fn draw_wall_overlay(&self, painter: &egui::Painter, view: Rect, hex: u32) {
        let Some(start) = self.edit.wall.start else {
            return;
        };
        let Some(plan) = self.wall_gesture_plan(start, hex) else {
            return;
        };
        let color = if plan.short {
            Color32::LIGHT_RED
        } else {
            Color32::from_rgb(100, 220, 255)
        };
        let mut previous = None;
        for &(h, _) in &plan.walls {
            let (x, y) = geom::hex_to_screen(h);
            let p = self
                .camera
                .world_to_screen(view, Pos2::new(x as f32, y as f32));
            painter.circle_filled(p, 4.0, color);
            if let Some((old, a)) = previous
                && geom::hex_distance(old, h) <= 2
            {
                painter.line_segment([a, p], Stroke::new(1.5, color));
            }
            previous = Some((h, p));
        }
    }
}
