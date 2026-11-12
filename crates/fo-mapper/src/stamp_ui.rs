//! Tampons (motifs) et pipette : capturer une sélection, la reposer, prélever une tuile ou un objet sous le curseur.

use crate::app::MapperApp;
use crate::edit_ui::Tool;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use fo_core::map::{TileLayer, word};
use fo_core::pattern::{self, CapturedObject, CapturedTile, Pattern, PatternVariant};
use fo_core::{Fid, MapObject, ObjectType, geom};
use fo_scene::Layer;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub(crate) struct StampState {
    pub patterns: Vec<Pattern>,
    pub loaded: bool,
    pub selected: Option<usize>,
    /// Variante qui sera posée (`R` change de variante).
    pub variant: usize,
    /// Nom d'un nouveau motif ou d'une nouvelle variante.
    pub name: String,
    /// Filtre de la liste des bâtiments du jeu.
    pub building_filter: String,
}

impl MapperApp {
    pub(crate) fn stamps_dir(&self) -> Option<PathBuf> {
        let g = self.game.as_ref()?;
        Some(
            g.data
                .root
                .join(&g.data.config.master_patches)
                .join("stamps"),
        )
    }

    pub(crate) fn ensure_patterns(&mut self) {
        if self.edit.stamp.loaded {
            return;
        }
        if let Some(dir) = self.stamps_dir() {
            self.edit.stamp.patterns = pattern::load_patterns(&dir);
        }
        self.edit.stamp.loaded = true;
        if self.edit.stamp.selected.is_none() && !self.edit.stamp.patterns.is_empty() {
            self.edit.stamp.selected = Some(0);
        }
    }

    fn current_variant(&self) -> Option<&PatternVariant> {
        let p = self.edit.stamp.patterns.get(self.edit.stamp.selected?)?;
        p.variants.get(
            self.edit
                .stamp
                .variant
                .min(p.variants.len().saturating_sub(1)),
        )
    }

    /// `R` en mode tampon : variante suivante.
    pub(crate) fn cycle_stamp_variant(&mut self) {
        let Some(p) = self
            .edit
            .stamp
            .selected
            .and_then(|i| self.edit.stamp.patterns.get(i))
        else {
            return;
        };
        let n = p.variants.len().max(1);
        self.edit.stamp.variant = (self.edit.stamp.variant + 1) % n;
    }

    // ---------------------------------------------------------------- capture

    /// Variante issue de la sélection (objets et cases), ancrée sur l'objet principal.
    fn capture_selection(&self, label: &str) -> Result<PatternVariant, String> {
        let cur = self
            .current
            .as_ref()
            .ok_or(crate::i18n::t("aucune carte"))?;
        let e = self.elevation;
        let mut objects = Vec::new();
        for i in self.selection.object_indices(e) {
            let Some(o) = cur.map.objects[e].get(i) else {
                continue;
            };
            if (0..geom::HEX_COUNT as i32).contains(&o.tile()) {
                objects.push(CapturedObject {
                    hex: o.tile() as u32,
                    pro_pid: o.pid(),
                    frm_pid: o.fid(),
                    direction: o.orientation().max(0) as u32,
                    flags: o.flags(),
                });
            }
        }
        let (mut floor, mut roof) = (Vec::new(), Vec::new());
        for t in self.selection.tiles() {
            let Some(tile) = cur.map.tile(e, t.square as usize) else {
                continue;
            };
            match t.layer {
                TileLayer::Floor => floor.push(CapturedTile {
                    square: t.square,
                    tile_id: tile.floor,
                }),
                TileLayer::Roof => roof.push(CapturedTile {
                    square: t.square,
                    tile_id: tile.roof,
                }),
            }
        }
        if objects.is_empty() && floor.is_empty() && roof.is_empty() {
            return Err(crate::i18n::t("sélectionnez d'abord des objets ou des cases").into());
        }
        let first_tile_hex = self
            .selection
            .tiles()
            .first()
            .map(|t| (t.square / 100) * 2 * geom::HEX_GRID_WIDTH + (t.square % 100) * 2);
        let anchor = self
            .selection
            .primary()
            .and_then(|r| self.object(r))
            .map(|o| o.tile())
            .filter(|t| *t >= 0)
            .map(|t| t as u32)
            .or(first_tile_hex)
            .or(self.hover_hex)
            .ok_or(crate::i18n::t("pas d'ancre"))?;
        Ok(PatternVariant::capture(
            label, anchor, &objects, &floor, &roof,
        ))
    }

    /// Crée (ou remplace) un motif à partir de la sélection et l'enregistre dans la bibliothèque. Un nom vide donne « motif N ».
    pub(crate) fn capture_named(&mut self, name: &str) -> Result<(), String> {
        self.ensure_patterns();
        let name = if name.is_empty() {
            format!("motif {}", self.edit.stamp.patterns.len() + 1)
        } else {
            name.to_string()
        };
        let v = self.capture_selection("défaut")?;
        self.edit.stamp.patterns.retain(|p| p.name != name);
        self.edit.stamp.patterns.push(Pattern::new(name.clone(), v));
        self.edit
            .stamp
            .patterns
            .sort_by_key(|p| p.name.to_lowercase());
        let idx = self.edit.stamp.patterns.iter().position(|p| p.name == name);
        self.edit.stamp.selected = idx;
        self.edit.stamp.variant = 0;
        if let Some(i) = idx {
            self.save_pattern_to_library(i);
        }
        Ok(())
    }

    /// Ajoute (ou remplace) un motif de la bibliothèque et l'enregistre dans `data/stamps/`. Renvoie le chemin du fichier.
    pub(crate) fn add_pattern(&mut self, p: Pattern) -> Result<String, String> {
        self.ensure_patterns();
        let dir = self
            .stamps_dir()
            .ok_or_else(|| crate::i18n::t("aucune carte").to_string())?;
        let path = pattern::save_pattern(&dir, &p).map_err(|e| e.to_string())?;
        let name = p.name.clone();
        self.edit.stamp.patterns.retain(|q| q.name != name);
        self.edit.stamp.patterns.push(p);
        self.edit
            .stamp
            .patterns
            .sort_by_key(|q| q.name.to_lowercase());
        self.edit.stamp.selected = self.edit.stamp.patterns.iter().position(|q| q.name == name);
        self.edit.stamp.variant = 0;
        Ok(path.display().to_string())
    }

    pub(crate) fn save_pattern_to_library(&mut self, index: usize) {
        let (Some(dir), Some(p)) = (self.stamps_dir(), self.edit.stamp.patterns.get(index)) else {
            return;
        };
        self.status = match pattern::save_pattern(&dir, p) {
            Ok(path) => format!(
                "{} : {}",
                crate::i18n::t("Motif enregistré"),
                path.display()
            ),
            Err(e) => format!(
                "{} : {e}",
                crate::i18n::t("Échec de l'enregistrement du motif")
            ),
        };
    }

    // ---------------------------------------------------------------- pose

    pub(crate) fn stamp_input(&mut self, _ui: &egui::Ui, response: &egui::Response) {
        self.ensure_patterns();
        if response.clicked_by(egui::PointerButton::Primary)
            && let Some(h) = self.hover_hex
        {
            self.stamp_at(h);
        }
    }

    pub(crate) fn stamp_at(&mut self, hex: u32) {
        let Some(v) = self.current_variant().cloned() else {
            return;
        };
        let plan = v.plan(hex);
        let e = self.elevation;
        let (mut placed, mut failed, mut painted) = (0usize, 0usize, 0usize);
        self.mutate_as("Pose d'un motif", |m, game| {
            for o in &plan.objects {
                let id = m.max_object_id() + 1;
                let Some(obj) = game
                    .proto_info(o.pro_pid)
                    .and_then(|info| MapObject::from_proto(&info, o.hex, id))
                else {
                    failed += 1;
                    continue;
                };
                let mut obj = obj;
                obj.words[word::FID] = o.frm_pid as i32;
                obj.words[word::ORIENTATION] = o.direction as i32;
                if let Some(f) = o.flags {
                    obj.words[word::FLAGS] = f as i32;
                }
                m.add_object(e, obj);
                placed += 1;
            }
            for t in &plan.tiles {
                let layer = if t.roof {
                    TileLayer::Roof
                } else {
                    TileLayer::Floor
                };
                if m.set_tile(e, t.square as usize, layer, t.tile_id) {
                    painted += 1;
                }
            }
            placed + painted > 0
        });
        self.status = format!(
            "{} : {placed} {}, {painted} {}{}{}",
            crate::i18n::t("Motif posé"),
            crate::i18n::t("objet(s)"),
            crate::i18n::t("case(s)"),
            if failed > 0 {
                format!(
                    ", {failed} {}",
                    crate::i18n::t("objet(s) non créables (créatures ?)")
                )
            } else {
                String::new()
            },
            if plan.objects_dropped + plan.tiles_dropped > 0 {
                format!(
                    ", {} {}",
                    plan.objects_dropped + plan.tiles_dropped,
                    crate::i18n::t("élément(s) hors grille")
                )
            } else {
                String::new()
            }
        );
    }

    // ---------------------------------------------------------------- panneau

    /// Bâtiments repérés dans les cartes du jeu : un clic en met un dans la bibliothèque, prêt à être posé.
    fn game_buildings_ui(&mut self, ui: &mut egui::Ui) {
        let t = crate::i18n::t;
        let buildings = self.analysis.result.as_ref().map(|r| r.buildings.clone());
        let title = match &buildings {
            Some(b) => format!("{} ({})", t("Bâtiments du jeu"), b.len()),
            None => t("Bâtiments du jeu").to_string(),
        };
        ui.collapsing(title, |ui| {
            let Some(buildings) = buildings else {
                if self.analysis.running() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(&self.status);
                    });
                } else {
                    ui.label(t(
                        "Les bâtiments viennent de l'analyse de toutes les cartes.",
                    ));
                    if ui.button(t("Analyser toutes les cartes")).clicked() {
                        let ctx = ui.ctx().clone();
                        self.start_tile_analysis(&ctx);
                    }
                }
                return;
            };
            ui.label(t(
                "Des constructions réelles du jeu : leurs murs sont exactement ceux des cartes.",
            ));
            ui.add(
                egui::TextEdit::singleline(&mut self.edit.stamp.building_filter)
                    .hint_text(t("Filtrer…"))
                    .desired_width(f32::INFINITY),
            );
            let needle = self.edit.stamp.building_filter.to_lowercase();
            let mut chosen = None;
            egui::ScrollArea::vertical()
                .id_salt("game_buildings")
                .max_height(220.0)
                .show(ui, |ui| {
                    for (i, b) in buildings.iter().enumerate().filter(|(_, b)| {
                        needle.is_empty() || b.pattern.name.to_lowercase().contains(&needle)
                    }) {
                        let selected = self
                            .edit
                            .stamp
                            .selected
                            .and_then(|s| self.edit.stamp.patterns.get(s))
                            .is_some_and(|p| p.name == b.pattern.name);
                        if ui
                            .selectable_label(
                                selected,
                                format!("{} — {} {}", b.pattern.name, b.walls, t("murs")),
                            )
                            .clicked()
                        {
                            chosen = Some(i);
                        }
                    }
                });
            if let Some(i) = chosen {
                let pattern = buildings[i].pattern.clone();
                let name = pattern.name.clone();
                if !self.edit.stamp.patterns.iter().any(|p| p.name == name) {
                    self.edit.stamp.patterns.push(pattern);
                    self.edit
                        .stamp
                        .patterns
                        .sort_by_key(|p| p.name.to_lowercase());
                }
                self.edit.stamp.selected =
                    self.edit.stamp.patterns.iter().position(|p| p.name == name);
                self.edit.stamp.variant = 0;
            }
        });
    }

    pub(crate) fn stamp_panel(&mut self, ui: &mut egui::Ui) {
        self.ensure_patterns();
        ui.label(crate::i18n::t(
            "Clic : pose le motif · R : variante suivante · Maj+clic droit : annuler.",
        ));
        ui.separator();
        ui.label(crate::i18n::t("Nouveau motif depuis la sélection"));
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.edit.stamp.name)
                    .hint_text(crate::i18n::t("Nom"))
                    .desired_width(150.0),
            );
            if ui.button(crate::i18n::t("Capturer")).clicked() {
                let name = self.edit.stamp.name.trim().to_string();
                if let Err(e) = self.capture_named(&name) {
                    self.status = format!("{} : {e}", crate::i18n::t("Capture impossible"));
                }
            }
        });
        if let Some(i) = self
            .edit
            .stamp
            .selected
            .filter(|&i| i < self.edit.stamp.patterns.len())
        {
            ui.horizontal(|ui| {
                if ui
                    .button(crate::i18n::t("Ajouter la sélection comme variante"))
                    .clicked()
                {
                    let n = self.edit.stamp.patterns[i].variants.len() + 1;
                    let label = if self.edit.stamp.name.trim().is_empty() {
                        format!("variante {n}")
                    } else {
                        self.edit.stamp.name.trim().to_string()
                    };
                    match self.capture_selection(&label) {
                        Ok(v) => {
                            self.edit.stamp.patterns[i].variants.push(v);
                            self.edit.stamp.variant =
                                self.edit.stamp.patterns[i].variants.len() - 1;
                            self.save_pattern_to_library(i);
                        }
                        Err(e) => {
                            self.status = format!("{} : {e}", crate::i18n::t("Capture impossible"))
                        }
                    }
                }
            });
        }
        ui.separator();
        self.game_buildings_ui(ui);
        ui.separator();
        ui.label(format!(
            "{} ({})",
            crate::i18n::t("Bibliothèque"),
            self.edit.stamp.patterns.len()
        ));
        let mut delete = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, p) in self.edit.stamp.patterns.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let text = format!(
                            "{} ({} {})",
                            p.name,
                            p.variants.len(),
                            crate::i18n::t("variante(s)")
                        );
                        if ui
                            .selectable_label(self.edit.stamp.selected == Some(i), text)
                            .clicked()
                        {
                            self.edit.stamp.selected = Some(i);
                            self.edit.stamp.variant = 0;
                        }
                        if ui
                            .small_button("✕")
                            .on_hover_text(crate::i18n::t("Supprimer ce motif de la bibliothèque"))
                            .clicked()
                        {
                            delete = Some(i);
                        }
                    });
                    if self.edit.stamp.selected == Some(i) && p.variants.len() > 1 {
                        ui.horizontal_wrapped(|ui| {
                            for (n, v) in p.variants.iter().enumerate() {
                                if ui
                                    .selectable_label(self.edit.stamp.variant == n, &v.label)
                                    .clicked()
                                {
                                    self.edit.stamp.variant = n;
                                }
                            }
                        });
                    }
                }
            });
        if let Some(i) = delete {
            let name = self.edit.stamp.patterns[i].name.clone();
            if let Some(dir) = self.stamps_dir() {
                let _ = pattern::delete_pattern(&dir, &name);
            }
            self.edit.stamp.patterns.remove(i);
            self.edit.stamp.selected = (!self.edit.stamp.patterns.is_empty()).then_some(0);
            self.edit.stamp.variant = 0;
        }
    }

    /// Aperçu du motif sous le curseur : objets en transparence et cases en losanges.
    pub(crate) fn draw_stamp_overlay(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        view: Rect,
        hex: u32,
    ) {
        let Some(v) = self.current_variant().cloned() else {
            return;
        };
        let Some(art) = self.game.as_ref().map(|g| g.art.clone()) else {
            return;
        };
        let plan = v.plan(hex);
        let cam = self.camera;
        for o in &plan.objects {
            let Some((_, sprite)) = art.object_sprite(Fid(o.frm_pid), o.direction, 0) else {
                continue;
            };
            let (cx, cy) = geom::hex_to_screen(o.hex);
            let min = Pos2::new((cx + sprite.origin_x) as f32, (cy + sprite.origin_y) as f32);
            let max = min + Vec2::new(sprite.width as f32, sprite.height as f32);
            let tex = self.texture_for(ctx, &sprite);
            painter.image(
                tex,
                Rect::from_min_max(
                    cam.world_to_screen(view, min),
                    cam.world_to_screen(view, max),
                ),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::from_white_alpha(150),
            );
        }
        for t in &plan.tiles {
            let (x, y) = geom::square_to_screen(t.square);
            let dy = if t.roof { geom::ROOF_OFFSET_Y } else { 0 };
            if let Some(sprite) = art.tile_sprite(t.tile_id as usize) {
                let min = Pos2::new(x as f32, (y + dy) as f32);
                let max = min + Vec2::new(sprite.width as f32, sprite.height as f32);
                let tex = self.texture_for(ctx, &sprite);
                painter.image(
                    tex,
                    Rect::from_min_max(
                        cam.world_to_screen(view, min),
                        cam.world_to_screen(view, max),
                    ),
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::from_white_alpha(110),
                );
            }
        }
        if let Some(h) = (!plan.objects.is_empty())
            .then(|| plan.objects[0].hex)
            .or(Some(hex))
        {
            let (x, y) = geom::hex_to_screen(h);
            let c = cam.world_to_screen(view, Pos2::new(x as f32, y as f32));
            painter.circle_stroke(c, 5.0, Stroke::new(1.5, Color32::from_rgb(255, 200, 80)));
        }
    }

    // ---------------------------------------------------------------- pipette

    /// Prélève ce qui est sous le curseur : un toit ou un sol arme la peinture avec cette tuile ; un objet arme le
    /// placement de cet objet (avec son orientation).
    pub(crate) fn eyedrop(&mut self, w: Pos2) {
        let Some(scene) = self.scene().cloned() else {
            return;
        };
        let (x, y) = (w.x as i32, w.y as i32);
        let e = self.elevation;
        let Some(cur) = self.current.as_ref() else {
            return;
        };
        let roof = self
            .layers
            .contains(Layer::Roof)
            .then(|| geom::screen_to_square(x, y - geom::ROOF_OFFSET_Y))
            .flatten()
            .and_then(|sq| cur.map.tile(e, sq as usize))
            .filter(|t| t.roof != fo_core::map::EMPTY_TILE);
        if let Some(t) = roof {
            self.edit.brush_tile = t.roof;
            self.edit.tool = Tool::Roof;
            self.status = format!(
                "{} : {} {}",
                crate::i18n::t("Pipette"),
                crate::i18n::t("toit n°"),
                t.roof
            );
            return;
        }
        let object = scene
            .items
            .iter()
            .rev()
            .find(|i| {
                i.object.is_some()
                    && self.layers.contains(i.layer)
                    && i.sprite.alpha_at(x - i.x, y - i.y) > 16
            })
            .and_then(|i| i.object);
        if let Some(r) = object
            && let Some(o) = self.object(r).cloned()
        {
            let entry = self.game.as_ref().and_then(|g| g.data.proto_entry(o.pid()));
            match (entry, o.object_type()) {
                (Some(entry), Some(ty)) if ty != ObjectType::Critter && ty != ObjectType::Tile => {
                    self.status = format!(
                        "{} : {}",
                        crate::i18n::t("Pipette"),
                        if entry.name.is_empty() {
                            &entry.file
                        } else {
                            &entry.name
                        }
                    );
                    self.edit.place_type = ty;
                    self.edit.place = Some(entry);
                    self.edit.place_orientation = o.orientation().clamp(0, 5) as u32;
                    self.edit.tool = Tool::Place;
                }
                _ => {
                    self.status = format!(
                        "{} : {}",
                        crate::i18n::t("Pipette"),
                        crate::i18n::t(
                            "cet objet ne peut pas être recréé depuis son prototype (dupliquez-le)."
                        )
                    )
                }
            }
            return;
        }
        let floor = geom::screen_to_square(x, y)
            .and_then(|sq| cur.map.tile(e, sq as usize))
            .filter(|t| t.floor != fo_core::map::EMPTY_TILE);
        if let Some(t) = floor {
            self.edit.brush_tile = t.floor;
            self.edit.tool = Tool::Floor;
            self.status = format!(
                "{} : {} {}",
                crate::i18n::t("Pipette"),
                crate::i18n::t("sol n°"),
                t.floor
            );
        }
    }
}
