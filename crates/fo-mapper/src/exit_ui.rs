//! Sorties de carte, bloqueurs de défilement et limites de défilement (`.edg`) : outils, panneaux et repères.

use crate::app::MapperApp;
use crate::edit_ui::Tool;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke};
use fo_core::edge::{EdgeRect, MapEdge};
use fo_core::map::ExitDestination;
use fo_core::map::exit::{
    self, DIRECTION_NAMES, EXIT_DIRECTIONS, SCROLL_BLOCKER_PID, TOWN_MAP_EXIT, WORLD_MAP_EXIT,
};
use fo_core::{MapEntry, MapObject, geom};
use fo_scene::{Layer, ObjectRef, Scene};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ExitMode {
    /// Un clic, un marqueur.
    Single,
    /// Tracé d'une ligne brisée : des marqueurs tout le long.
    Edge,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DestKind {
    Map,
    World,
    Town,
}

/// Réglages de l'outil « Sortie » : destination et direction des marqueurs posés.
#[derive(Debug, Clone)]
pub(crate) struct ExitTool {
    pub mode: ExitMode,
    pub dir: u8,
    /// Mode tracé : choisit l'autre côté de la bande.
    pub flipped: bool,
    pub kind: DestKind,
    pub map: i32,
    pub tile: i32,
    pub elevation: i32,
    pub orientation: i32,
    /// Sommets du tracé en cours.
    pub points: Vec<u32>,
}

impl Default for ExitTool {
    fn default() -> Self {
        Self {
            mode: ExitMode::Single,
            dir: exit::DIR_LEFT,
            flipped: false,
            kind: DestKind::World,
            map: 0,
            tile: 0,
            elevation: 0,
            orientation: 0,
            points: Vec::new(),
        }
    }
}

impl ExitTool {
    pub(crate) fn destination(&self) -> ExitDestination {
        let map = match self.kind {
            DestKind::World => WORLD_MAP_EXIT,
            DestKind::Town => TOWN_MAP_EXIT,
            DestKind::Map => self.map,
        };
        ExitDestination {
            map,
            tile: self.tile,
            elevation: self.elevation,
            orientation: self.orientation,
        }
    }
}

/// Vecteur sortant de chaque bande (écran, y vers le bas) : sert à choisir la seconde rangée d'une diagonale.
fn outward(dir: u8) -> (i32, i32) {
    match dir {
        exit::DIR_LEFT => (-1, 0),
        exit::DIR_RIGHT => (1, 0),
        exit::DIR_BOTTOM => (0, 1),
        exit::DIR_TOP => (0, -1),
        exit::DIR_FWD_A => (1, 6),
        exit::DIR_FWD_B => (-1, -6),
        exit::DIR_BACK_A => (-1, 2),
        exit::DIR_BACK_B => (1, -2),
        _ => (0, 0),
    }
}

/// Marqueurs (hexagone, direction) d'un tracé : pour chaque segment, la direction vient de son orientation à l'écran,
/// un marqueur est posé sur chaque hexagone de la ligne, et les diagonales ont une seconde rangée du côté vers lequel
/// regardent les barres (comme Gecko : les deux rangées sont de vrais marqueurs).
pub(crate) fn exit_edge_plan(points: &[u32], flipped: bool) -> Vec<(u32, u8)> {
    let mut out: Vec<(u32, u8)> = Vec::new();
    let mut push = |h: u32, d: u8| {
        if !out.contains(&(h, d)) {
            out.push((h, d));
        }
    };
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let ((ax, ay), (bx, by)) = (geom::hex_to_screen(a), geom::hex_to_screen(b));
        let dir = exit::direction_for_segment(bx - ax, by - ay, flipped);
        let (ox, oy) = outward(dir);
        for h in geom::hex_line(a, b) {
            push(h, dir);
            if (exit::DIR_FWD_A..=exit::DIR_BACK_B).contains(&dir) {
                let (hx, hy) = geom::hex_to_screen(h);
                let second = geom::hex_neighbors(h)
                    .into_iter()
                    .map(|n| {
                        let (nx, ny) = geom::hex_to_screen(n);
                        (n, (nx - hx) * ox + (ny - hy) * oy)
                    })
                    .filter(|&(_, dot)| dot > 0)
                    .max_by_key(|&(_, dot)| dot);
                if let Some((n, _)) = second {
                    push(n, dir);
                }
            }
        }
    }
    out
}

/// Sélecteur de destination partagé par l'outil et l'éditeur de propriétés. Renvoie `true` si quelque chose a changé.
fn destination_widgets(
    ui: &mut egui::Ui,
    id: &str,
    kind: &mut DestKind,
    map: &mut i32,
    tile: &mut i32,
    elevation: &mut i32,
    orientation: &mut i32,
    maps: &[MapEntry],
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui
            .selectable_value(kind, DestKind::Map, crate::i18n::t("Carte"))
            .changed();
        changed |= ui
            .selectable_value(kind, DestKind::World, crate::i18n::t("Carte du monde"))
            .changed();
        changed |= ui
            .selectable_value(kind, DestKind::Town, crate::i18n::t("Plan de ville"))
            .changed();
    });
    if *kind == DestKind::Map {
        let label = maps
            .iter()
            .find(|m| m.number.is_some_and(|n| n as i32 == *map))
            .map_or(format!("{} {map}", crate::i18n::t("carte")), |m| {
                format!(
                    "{} — {}",
                    m.number.unwrap_or(0),
                    m.display.as_deref().unwrap_or(&m.file)
                )
            });
        egui::ComboBox::from_id_salt(format!("{id}_map"))
            .selected_text(label)
            .width(230.0)
            .show_ui(ui, |ui| {
                for m in maps.iter().filter(|m| m.number.is_some()) {
                    let n = m.number.unwrap_or(0) as i32;
                    let text = format!("{n} — {}", m.display.as_deref().unwrap_or(&m.file));
                    changed |= ui.selectable_value(map, n, text).changed();
                }
            });
    }
    egui::Grid::new(format!("{id}_grid"))
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(crate::i18n::t("Hexagone d'arrivée"));
            changed |= ui
                .add(crate::stepper::Stepper::new(tile).range(0..=(geom::HEX_COUNT as i32 - 1)))
                .changed();
            ui.end_row();
            ui.label(crate::i18n::t("Élévation"));
            changed |= ui
                .add(crate::stepper::Stepper::new(elevation).range(0..=2))
                .changed();
            ui.end_row();
            ui.label(crate::i18n::t("Orientation"));
            changed |= ui
                .add(crate::stepper::Stepper::new(orientation).range(0..=5))
                .changed();
            ui.end_row();
        });
    changed
}

fn kind_of(map: i32) -> DestKind {
    match map {
        WORLD_MAP_EXIT => DestKind::World,
        TOWN_MAP_EXIT => DestKind::Town,
        _ => DestKind::Map,
    }
}

impl MapperApp {
    // ---------------------------------------------------------------- outil « Sortie »

    pub(crate) fn exit_input(&mut self, ui: &egui::Ui, response: &egui::Response) {
        let hover = self.hover_hex;
        let (enter, space) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Space),
            )
        });
        match self.edit.exit.mode {
            ExitMode::Single => {
                if response.clicked_by(egui::PointerButton::Primary)
                    && let Some(h) = hover
                {
                    let dir = self.edit.exit.dir;
                    self.place_exits(&[(h, dir)]);
                }
            }
            ExitMode::Edge => {
                if space {
                    self.edit.exit.flipped = !self.edit.exit.flipped;
                }
                if response.double_clicked_by(egui::PointerButton::Primary) || enter {
                    self.finish_exit_edge();
                } else if response.clicked_by(egui::PointerButton::Primary)
                    && let Some(h) = hover
                    && self.edit.exit.points.last() != Some(&h)
                {
                    self.edit.exit.points.push(h);
                }
            }
        }
    }

    pub(crate) fn finish_exit_edge(&mut self) {
        let points = std::mem::take(&mut self.edit.exit.points);
        let flipped = self.edit.exit.flipped;
        let plan = match points.as_slice() {
            [] => return,
            [one] => vec![(*one, exit::direction_for_segment(1, 0, flipped))],
            _ => exit_edge_plan(&points, flipped),
        };
        self.place_exits(&plan);
    }

    /// Pose des sorties avec la destination courante de l'outil ; une sortie identique déjà présente est ignorée.
    fn place_exits(&mut self, plan: &[(u32, u8)]) {
        let e = self.elevation;
        let dest = self.edit.exit.destination();
        let plan = plan.to_vec();
        let mut added = 0;
        self.mutate_as("Pose de sorties", |m, _| {
            for &(hex, dir) in &plan {
                let exists = m.objects[e]
                    .iter()
                    .any(|o| o.tile() == hex as i32 && o.pid() == exit::exit_pid(dir));
                if exists {
                    continue;
                }
                let id = m.max_object_id() + 1;
                m.add_object(e, MapObject::exit_grid(dir, dest, hex, id));
                added += 1;
            }
            added > 0
        });
        self.status = format!("{added} {}", crate::i18n::t("sortie(s) posée(s)"));
    }

    pub(crate) fn exit_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.edit.exit.mode,
                ExitMode::Single,
                crate::i18n::t("Un hexagone"),
            );
            if ui
                .selectable_value(
                    &mut self.edit.exit.mode,
                    ExitMode::Edge,
                    crate::i18n::t("Tracer un bord"),
                )
                .clicked()
            {
                self.edit.exit.points.clear();
            }
        });
        match self.edit.exit.mode {
            ExitMode::Single => {
                egui::ComboBox::from_id_salt("exit_dir")
                    .selected_text(crate::i18n::t(
                        DIRECTION_NAMES[self.edit.exit.dir as usize % EXIT_DIRECTIONS],
                    ))
                    .show_ui(ui, |ui| {
                        for (i, name) in DIRECTION_NAMES.iter().enumerate() {
                            ui.selectable_value(
                                &mut self.edit.exit.dir,
                                i as u8,
                                crate::i18n::t(name),
                            );
                        }
                    });
                ui.label(crate::i18n::t("Clic : pose une sortie."));
            }
            ExitMode::Edge => {
                ui.checkbox(
                    &mut self.edit.exit.flipped,
                    crate::i18n::t("Autre côté de la bande (Espace)"),
                );
                ui.label(crate::i18n::t(
                    "Clic : ajoute un sommet · Entrée ou double-clic : termine · Échap : annule.",
                ));
                if !self.edit.exit.points.is_empty()
                    && ui.button(crate::i18n::t("Terminer le tracé")).clicked()
                {
                    self.finish_exit_edge();
                }
            }
        }
        ui.separator();
        ui.label(crate::i18n::t("Destination"));
        let maps: Vec<MapEntry> = self
            .game
            .as_ref()
            .map(|g| g.maps.clone())
            .unwrap_or_default();
        let t = &mut self.edit.exit;
        destination_widgets(
            ui,
            "exit_tool",
            &mut t.kind,
            &mut t.map,
            &mut t.tile,
            &mut t.elevation,
            &mut t.orientation,
            &maps,
        );
        ui.label(crate::i18n::t("La destination vaut pour les sorties posées ensuite ; pour une sortie existante, sélectionnez-la."));
        if ui
            .button(crate::i18n::t("Liaisons entre cartes…"))
            .clicked()
        {
            self.links.open = true;
        }
    }

    /// Propriétés d'une sortie sélectionnée : direction et destination.
    pub(crate) fn exit_properties(&mut self, ui: &mut egui::Ui, r: ObjectRef) {
        let Some(o) = self.object(r).cloned() else {
            return;
        };
        let (Some(dest), Some(dir)) = (o.exit_destination(), o.exit_direction()) else {
            return;
        };
        let (e, i) = (r.elevation as usize, r.index as usize);
        ui.label(crate::i18n::t("Sortie de carte"));
        let (mut dir_new, mut kind, mut map, mut tile, mut elev, mut orient) = (
            dir,
            kind_of(dest.map),
            dest.map,
            dest.tile,
            dest.elevation,
            dest.orientation,
        );
        let mut changed = false;
        egui::ComboBox::from_id_salt("exit_sel_dir")
            .selected_text(crate::i18n::t(
                DIRECTION_NAMES[dir as usize % EXIT_DIRECTIONS],
            ))
            .show_ui(ui, |ui| {
                for (n, name) in DIRECTION_NAMES.iter().enumerate() {
                    changed |= ui
                        .selectable_value(&mut dir_new, n as u8, crate::i18n::t(name))
                        .changed();
                }
            });
        let maps: Vec<MapEntry> = self
            .game
            .as_ref()
            .map(|g| g.maps.clone())
            .unwrap_or_default();
        changed |= destination_widgets(
            ui,
            "exit_sel",
            &mut kind,
            &mut map,
            &mut tile,
            &mut elev,
            &mut orient,
            &maps,
        );
        if changed {
            let dest_map = match kind {
                DestKind::World => WORLD_MAP_EXIT,
                DestKind::Town => TOWN_MAP_EXIT,
                DestKind::Map => map,
            };
            self.mutate(Some("exit"), |m, _| {
                let Some(o) = m.objects.get_mut(e).and_then(|v| v.get_mut(i)) else {
                    return false;
                };
                o.set_exit_destination(ExitDestination {
                    map: dest_map,
                    tile,
                    elevation: elev,
                    orientation: orient,
                });
                o.set_exit_direction(dir_new);
                true
            });
        }
    }

    // ---------------------------------------------------------------- bloqueurs de défilement

    pub(crate) fn blocker_input(&mut self, ui: &egui::Ui, response: &egui::Response) -> bool {
        let primary = egui::PointerButton::Primary;
        if response.drag_started_by(primary) {
            self.edit.blocker_start = ui.input(|i| i.pointer.press_origin());
        }
        if response.drag_stopped_by(primary)
            && let Some(start) = self.edit.blocker_start.take()
            && let Some(end) = ui.input(|i| i.pointer.latest_pos())
        {
            let (a, b) = (
                self.camera.screen_to_world(self.canvas_rect, start),
                self.camera.screen_to_world(self.canvas_rect, end),
            );
            self.place_blockers(Rect::from_two_pos(a, b));
        }
        self.edit.blocker_start.is_some()
    }

    /// Pose des bloqueurs de défilement sur le contour du rectangle (en coordonnées du monde).
    fn place_blockers(&mut self, rect: Rect) {
        let e = self.elevation;
        let Some(info) = self
            .game
            .as_ref()
            .and_then(|g| g.data.proto_info(SCROLL_BLOCKER_PID))
        else {
            self.status = crate::i18n::t("Prototype de bloqueur de défilement introuvable.").into();
            return;
        };
        let border = geom::hexes_on_rect_border(rect.min.x, rect.min.y, rect.max.x, rect.max.y);
        let mut added = 0;
        self.mutate_as("Bloqueurs de défilement", |m, _| {
            for h in border {
                if m.objects[e]
                    .iter()
                    .any(|o| o.is_scroll_blocker() && o.tile() == h as i32)
                {
                    continue;
                }
                let id = m.max_object_id() + 1;
                if let Some(o) = MapObject::from_proto(&info, h, id) {
                    m.add_object(e, o);
                    added += 1;
                }
            }
            added > 0
        });
        self.status = format!(
            "{added} {}",
            crate::i18n::t("bloqueur(s) de défilement posé(s)")
        );
    }

    // ---------------------------------------------------------------- limites de défilement (.edg)

    pub(crate) fn edge_editor(&mut self, ui: &mut egui::Ui) {
        let Some(cur) = &self.current else { return };
        let mut edge = cur.map.edge.clone().unwrap_or_default();
        let original = cur.map.edge.clone();
        let player = cur.map.header.player_tile.max(0) as i32;
        let mut changed = false;
        ui.collapsing(crate::i18n::t("Limites de défilement (.edg)"), |ui| {
            ui.label(crate::i18n::t("Zones en numéros d'hexagones (gauche, haut, droite, bas). Sans zone, pas de fichier."));
            for el in 0..3 {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(format!("{} {el} — {} {}", crate::i18n::t("Élévation"), edge.elevations[el].zones.len(), crate::i18n::t("zone(s)")));
                    if ui.small_button(crate::i18n::t("+ zone")).clicked() {
                        edge.elevations[el].zones.push(EdgeRect { left: player, top: player, right: player, bottom: player });
                        changed = true;
                    }
                });
                let mut remove = None;
                for (z, zone) in edge.elevations[el].zones.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        for v in [&mut zone.left, &mut zone.top, &mut zone.right, &mut zone.bottom] {
                            changed |= ui.add(crate::stepper::Stepper::new(v).range(0..=(geom::HEX_COUNT as i32 - 1)).speed(20.0)).changed();
                        }
                        if ui.small_button("✕").clicked() {
                            remove = Some(z);
                        }
                    });
                }
                if let Some(z) = remove {
                    edge.elevations[el].zones.remove(z);
                    changed = true;
                }
            }
            if edge.elevations[0].zones.is_empty() && edge.total_zones() > 0 {
                ui.colored_label(Color32::from_rgb(255, 190, 80), crate::i18n::t("L'élévation 0 doit avoir une zone pour que le fichier soit valide."));
            }
        });
        if changed {
            let value = (!edge.is_empty() || original.is_some()).then_some(edge);
            self.mutate(Some("edge"), |m, _| {
                m.edge = value;
                true
            });
        }
    }

    // ---------------------------------------------------------------- repères

    /// Tracé de sortie en cours, rectangle de bloqueurs et zones de défilement.
    pub(crate) fn draw_exit_overlay(
        &self,
        painter: &egui::Painter,
        view: Rect,
        _scene: &Arc<Scene>,
    ) {
        let cam = self.camera;
        let to_screen = |h: u32| {
            let (x, y) = geom::hex_to_screen(h);
            cam.world_to_screen(view, Pos2::new(x as f32, y as f32))
        };
        if self.edit.tool == Tool::Exit && !self.edit.exit.points.is_empty() {
            let mut pts: Vec<Pos2> = self
                .edit
                .exit
                .points
                .iter()
                .map(|&h| to_screen(h))
                .collect();
            if let Some(h) = self.hover_hex {
                pts.push(to_screen(h));
            }
            painter.add(egui::Shape::line(
                pts.clone(),
                Stroke::new(2.0, Color32::from_rgb(255, 210, 60)),
            ));
            for p in pts {
                painter.circle_filled(p, 3.0, Color32::from_rgb(255, 210, 60));
            }
        }
        if let Some(start) = self.edit.blocker_start
            && let Some(end) = painter.ctx().input(|i| i.pointer.latest_pos())
        {
            let r = Rect::from_two_pos(start, end);
            painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(255, 120, 60, 30));
            painter.rect_stroke(
                r,
                0.0,
                Stroke::new(1.0, Color32::from_rgb(255, 120, 60)),
                egui::StrokeKind::Inside,
            );
        }
        // Zones de défilement de l'élévation courante : polygone passant par les hexagones des quatre coins.
        if self.show_edges
            && let Some(edge) = self.current.as_ref().and_then(|c| c.map.edge.as_ref())
        {
            for z in &edge.elevations[self.elevation].zones {
                let corners = [z.left, z.top, z.right, z.bottom];
                if corners
                    .iter()
                    .all(|&c| (0..geom::HEX_COUNT as i32).contains(&c))
                {
                    let pts: Vec<Pos2> = corners.iter().map(|&c| to_screen(c as u32)).collect();
                    painter.add(egui::Shape::closed_line(
                        pts,
                        Stroke::new(2.0, Color32::from_rgb(180, 120, 255)),
                    ));
                }
            }
        }
    }

    /// La couche donnée est-elle une couche d'éléments de la famille « sorties et bloqueurs » ?
    #[allow(dead_code)]
    pub(crate) fn is_special_layer(layer: Layer) -> bool {
        matches!(layer, Layer::Exit | Layer::Blocker)
    }
}

/// Sauvegarde les limites de défilement à côté de la carte : écrit le `.edg` s'il y a des zones, supprime le
/// fichier de correctifs sinon. Renvoie un message d'erreur éventuel.
pub(crate) fn save_edge_beside(
    game: &fo_core::GameData,
    file: &str,
    edge: Option<&MapEdge>,
) -> Option<String> {
    match edge {
        Some(e) if !e.is_empty() => match e.to_bytes().and_then(|b| game.save_edge(file, &b)) {
            Ok(_) => None,
            Err(err) => Some(format!(
                "{} : {err}",
                crate::i18n::t("limites de défilement")
            )),
        },
        _ => game
            .remove_edge_override(file)
            .err()
            .map(|e| format!("{} : {e}", crate::i18n::t("limites de défilement"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_plan_covers_the_line_with_one_direction_per_segment() {
        let a = 20_100;
        let b = 20_300; // même colonne, 100 lignes plus bas ? non : 1 ligne plus bas.
        let plan = exit_edge_plan(&[a, b], false);
        assert!(plan.len() >= 2);
        assert!(plan.iter().all(|&(_, d)| d == plan[0].1));
        assert_eq!(plan.first().map(|p| p.0), Some(a));
        let flipped = exit_edge_plan(&[a, b], true);
        assert_eq!(flipped[0].1, exit::flip_direction(plan[0].1));
    }

    #[test]
    fn diagonal_edges_get_a_second_row() {
        // Ligne en diagonale : deux rangées de marqueurs.
        let a = 20_100;
        let b = geom::cube_hex(geom::hex_cube(a) + geom::Cube { x: 0, y: -5, z: 5 }).unwrap();
        let plan = exit_edge_plan(&[a, b], false);
        let line_len = geom::hex_line(a, b).len();
        let ((ax, ay), (bx, by)) = (geom::hex_to_screen(a), geom::hex_to_screen(b));
        let dir = exit::direction_for_segment(bx - ax, by - ay, false);
        if (exit::DIR_FWD_A..=exit::DIR_BACK_B).contains(&dir) {
            assert!(
                plan.len() > line_len,
                "{} marqueurs pour {line_len} hexagones",
                plan.len()
            );
        } else {
            assert_eq!(plan.len(), line_len);
        }
    }

    #[test]
    fn destination_kinds_map_to_special_ids() {
        let mut t = ExitTool::default();
        assert_eq!(t.destination().map, WORLD_MAP_EXIT);
        t.kind = DestKind::Town;
        assert_eq!(t.destination().map, TOWN_MAP_EXIT);
        t.kind = DestKind::Map;
        t.map = 12;
        assert_eq!(t.destination().map, 12);
        assert_eq!(kind_of(-2), DestKind::World);
    }
}
