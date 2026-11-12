//! Petites fonctions d'édition et d'aide : copier/coller, menu contextuel, légende des repères.

use crate::app::MapperApp;
use crate::i18n::t;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use fo_core::geom;
use fo_core::map::MapObject;
use fo_scene::ObjectRef;

/// Presse-papiers d'objets : les copies et l'hexagone de l'objet principal (point d'ancrage du collage).
#[derive(Clone, Default)]
pub(crate) struct Clipboard {
    pub objects: Vec<MapObject>,
    pub anchor: u32,
}

impl MapperApp {
    pub(crate) fn copy_selected(&mut self) {
        let e = self.elevation;
        let indices = self.selection.object_indices(e);
        let Some(anchor) = self
            .selection
            .primary()
            .and_then(|r| self.object(r))
            .map(|o| o.tile())
            .filter(|t| *t >= 0)
            .map(|t| t as u32)
        else {
            return;
        };
        let Some(cur) = &self.current else { return };
        let objects: Vec<MapObject> = indices
            .iter()
            .filter_map(|&i| cur.map.objects[e].get(i).cloned())
            .collect();
        if objects.is_empty() {
            return;
        }
        self.status = format!("{} {}", objects.len(), t("objet(s) copié(s)"));
        self.clip = Some(Clipboard { objects, anchor });
    }

    pub(crate) fn cut_selected(&mut self) {
        self.copy_selected();
        self.delete_selected();
    }

    /// Colle le presse-papiers : l'objet principal arrive sous le curseur, les autres gardent leur position relative.
    pub(crate) fn paste_clipboard(&mut self) {
        let Some(clip) = self.clip.clone() else {
            return;
        };
        let target = self.hover_hex.unwrap_or(clip.anchor);
        self.paste_objects(&clip, target);
    }

    /// Colle des objets sur la carte active : l'objet principal arrive sur `target`, les autres gardent leur position relative.
    pub(crate) fn paste_objects(&mut self, clip: &Clipboard, target: u32) {
        let e = self.elevation;
        let mut created = Vec::new();
        self.mutate_as("Collage", |m, _| {
            for o in &clip.objects {
                let tile = (o.tile() >= 0)
                    .then(|| geom::hex_translate(o.tile() as u32, clip.anchor, target))
                    .flatten()
                    .unwrap_or(target);
                let copy = o.duplicate_at(tile, m.max_object_id() + 1);
                created.push(m.add_object(e, copy));
            }
            !created.is_empty()
        });
        if !created.is_empty() {
            self.selection.set_objects(
                created
                    .into_iter()
                    .map(|i| ObjectRef {
                        elevation: e as u8,
                        index: i as u32,
                    })
                    .collect(),
            );
        }
    }

    /// Menu contextuel (clic droit) sur la carte : cloner, supprimer, ouvrir le script de l'élément sélectionné.
    pub(crate) fn canvas_context_menu(&mut self, response: &egui::Response) {
        if !self.edit.enabled {
            return;
        }
        let script = self
            .selection
            .primary()
            .and_then(|r| self.object(r))
            .and_then(|o| self.current.as_ref()?.map.find_script(o.script_id()))
            .and_then(|entry| {
                self.game
                    .as_ref()?
                    .data
                    .script_file(entry.list_index().max(0) as usize)
            });
        let has_selection = !self.selection.is_empty();
        let has_clip = self.clip.is_some();
        let mut action = None;
        response.context_menu(|ui| {
            ui.set_min_width(180.0);
            if ui
                .add_enabled(has_selection, egui::Button::new(t("Cloner")))
                .clicked()
            {
                action = Some(0);
            }
            if ui
                .add_enabled(has_selection, egui::Button::new(t("Copier (Ctrl+C)")))
                .clicked()
            {
                action = Some(1);
            }
            if ui
                .add_enabled(has_clip, egui::Button::new(t("Coller (Ctrl+V)")))
                .clicked()
            {
                action = Some(2);
            }
            if ui
                .add_enabled(has_selection, egui::Button::new(t("Supprimer")))
                .clicked()
            {
                action = Some(3);
            }
            ui.separator();
            if ui
                .add_enabled(script.is_some(), egui::Button::new(t("Voir le script")))
                .clicked()
            {
                action = Some(4);
            }
            if action.is_some() {
                ui.close();
            }
        });
        match action {
            Some(0) => self.duplicate_selected(),
            Some(1) => self.copy_selected(),
            Some(2) => self.paste_clipboard(),
            Some(3) => self.delete_selected(),
            Some(4) => self.open_script_request = script,
            _ => {}
        }
    }

    /// Étire la sélection d'objets (une pièce, un bâtiment) : les murs et le mobilier de la bande du milieu sont répétés.
    pub(crate) fn stretch_section(&mut self, ui: &mut egui::Ui) {
        let n = self.selection.objects().len();
        if n < 2 {
            return;
        }
        ui.collapsing(t("Étirer la sélection (pièce)"), |ui| {
            ui.label(t("Répète la bande centrale du groupe sur chaque axe de l'écran ; les bords gardent leur forme."));
            ui.horizontal(|ui| {
                ui.label("↘");
                ui.add(crate::stepper::Stepper::new(&mut self.edit.stretch.0).range(0..=60));
                ui.label("↙");
                ui.add(crate::stepper::Stepper::new(&mut self.edit.stretch.1).range(0..=60));
            });
            let (du, dv) = (self.edit.stretch.0.max(0) as u32, self.edit.stretch.1.max(0) as u32);
            if ui.add_enabled(du + dv > 0, egui::Button::new(t("Étirer"))).clicked() {
                self.stretch_selection(du, dv);
            }
        });
    }

    pub(crate) fn stretch_for_test(&mut self, du: u32, dv: u32) {
        self.stretch_selection(du, dv);
    }

    fn stretch_selection(&mut self, du: u32, dv: u32) {
        let e = self.elevation;
        let indices = self.selection.object_indices(e);
        let Some(cur) = &self.current else { return };
        let originals: Vec<MapObject> = indices
            .iter()
            .filter_map(|&i| cur.map.objects[e].get(i).cloned())
            .collect();
        let hexes: Vec<u32> = originals
            .iter()
            .filter_map(|o| (o.tile() >= 0).then(|| o.tile() as u32))
            .collect();
        if hexes.len() != originals.len() || hexes.is_empty() {
            self.status =
                t("Étirement impossible : un objet de la sélection n'est pas sur la carte.")
                    .to_string();
            return;
        }
        let placed = fo_core::stretch::stretch(&hexes, du, dv);
        let before = placed.len();
        let mut created = Vec::new();
        let mut removed = indices.clone();
        removed.sort_unstable();
        self.mutate_as("Étirement", |m, _| {
            for &i in removed.iter().rev() {
                m.remove_object(e, i);
            }
            for p in &placed {
                let copy = originals[p.source].duplicate_at(p.hex, m.max_object_id() + 1);
                created.push(m.add_object(e, copy));
            }
            !created.is_empty()
        });
        if !created.is_empty() {
            self.selection.set_objects(
                created
                    .into_iter()
                    .map(|i| ObjectRef {
                        elevation: e as u8,
                        index: i as u32,
                    })
                    .collect(),
            );
            self.status = format!(
                "{} → {before} {}",
                originals.len(),
                t("objets après étirement")
            );
        }
    }

    /// Entoure les objets qui portent un script.
    pub(crate) fn draw_scripted(
        &self,
        painter: &egui::Painter,
        view: Rect,
        scene: &fo_scene::Scene,
    ) {
        let stroke = Stroke::new(1.5, Color32::from_rgb(255, 80, 220));
        for item in scene.items.iter() {
            let Some(r) = item.object else { continue };
            if self.object(r).is_none_or(|o| o.script_id() < 0) {
                continue;
            }
            let rect = Rect::from_min_size(
                Pos2::new(item.x as f32, item.y as f32),
                Vec2::new(item.sprite.width as f32, item.sprite.height as f32),
            );
            let sr = Rect::from_min_max(
                self.camera.world_to_screen(view, rect.min),
                self.camera.world_to_screen(view, rect.max),
            );
            if view.intersects(sr) {
                painter.rect_stroke(sr, 2.0, stroke, egui::StrokeKind::Outside);
            }
        }
    }

    pub(crate) fn legend_window(&mut self, ctx: &egui::Context) {
        if !self.legend_open {
            return;
        }
        let mut open = true;
        egui::Window::new(t("Légende des repères"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let row = |ui: &mut egui::Ui, draw: &dyn Fn(&egui::Painter, Rect), text: &str| {
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(30.0, 22.0), egui::Sense::hover());
                        draw(ui.painter(), rect);
                        ui.label(text);
                    });
                };
                let circle = |color: Color32, r: f32| {
                    move |p: &egui::Painter, rect: Rect| {
                        p.circle_stroke(rect.center(), r, Stroke::new(1.5, color));
                    }
                };
                row(
                    ui,
                    &circle(Color32::from_rgb(80, 255, 120), 9.0),
                    t("Point de départ du joueur"),
                );
                row(
                    ui,
                    &circle(Color32::from_rgb(255, 210, 60), 6.0),
                    t("Grille de sortie (vers une autre carte)"),
                );
                row(
                    ui,
                    &circle(Color32::from_rgb(90, 170, 255), 6.0),
                    t("Script spatial (déclenché au passage)"),
                );
                row(
                    ui,
                    &circle(Color32::from_rgba_unmultiplied(200, 200, 200, 90), 3.0),
                    t("Objet masqué"),
                );
                row(
                    ui,
                    &|p, rect| {
                        p.rect_stroke(
                            rect.shrink(4.0),
                            2.0,
                            Stroke::new(1.5, Color32::from_rgb(255, 80, 220)),
                            egui::StrokeKind::Outside,
                        );
                    },
                    t("Objet ou créature avec un script (Affichage › Surligner)"),
                );
                row(
                    ui,
                    &|p, rect| {
                        p.rect_stroke(
                            rect.shrink(4.0),
                            0.0,
                            Stroke::new(1.5, Color32::from_rgb(255, 80, 80)),
                            egui::StrokeKind::Outside,
                        );
                    },
                    t("Élément sélectionné"),
                );
                row(
                    ui,
                    &|p, rect| {
                        p.line_segment(
                            [rect.left_center(), rect.right_center()],
                            Stroke::new(2.0, Color32::from_rgb(180, 100, 255)),
                        );
                    },
                    t("Limite de défilement (.edg)"),
                );
                row(
                    ui,
                    &|p, rect| {
                        p.rect_filled(
                            rect.shrink(4.0),
                            0.0,
                            Color32::from_rgba_unmultiplied(255, 190, 40, 90),
                        );
                    },
                    t("Zone de bloqueurs de défilement"),
                );
                row(
                    ui,
                    &|p, rect| {
                        let c = rect.center();
                        let pts: Vec<Pos2> = crate::edit_ui::HEX_OUTLINE
                            .iter()
                            .map(|&(dx, dy)| Pos2::new(c.x + dx * 0.7, c.y + dy * 0.7 - 1.0))
                            .collect();
                        p.add(egui::Shape::closed_line(
                            pts,
                            Stroke::new(1.0, Color32::from_gray(150)),
                        ));
                    },
                    t("Grille hexagonale"),
                );
            });
        self.legend_open = open;
    }
}
