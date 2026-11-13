//! Édition de la carte : outils, annulation / rétablissement, palettes, enregistrement.

use crate::app::MapperApp;
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use fo_core::{Fid, ObjectType, ProtoEntry};
use std::sync::Arc;
use std::sync::mpsc::{self};
use std::time::Duration;

use crate::edit_ui::*;

impl MapperApp {
    /// Accès rapide à la taille des miniatures et à l'affichage des noms (mêmes réglages que les Options).
    pub(crate) fn thumb_options(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("thumb_size_quick")
                .selected_text(format!("{0}×{0}", self.prefs.thumb_size))
                .width(70.0)
                .show_ui(ui, |ui| {
                    for s in crate::prefs::THUMB_SIZES {
                        ui.selectable_value(&mut self.prefs.thumb_size, s, format!("{s}×{s}"));
                    }
                });
            ui.checkbox(&mut self.prefs.show_names, crate::i18n::t("Noms"));
        });
    }

    pub(crate) fn tile_palette(&mut self, ui: &mut egui::Ui) {
        let Some(g) = &self.game else { return };
        if self.edit.tile_names.is_none() {
            self.edit.tile_names = Some(g.data.list("art/tiles/tiles.lst"));
        }
        let names = self.edit.tile_names.clone().unwrap_or_default();
        let art = g.art.clone();
        let ctx = ui.ctx().clone();
        let help = if self.edit.tool == Tool::Roof {
            crate::i18n::t("Peinture des toits : glissez sur la carte. Tuile choisie : n°")
        } else {
            crate::i18n::t("Peinture des sols : glissez sur la carte. Tuile choisie : n°")
        };
        ui.label(format!("{help} {}", self.edit.brush_tile));
        if ui.button(crate::i18n::t("Gomme (tuile vide)")).clicked() {
            self.edit.brush_tile = 1;
        }
        ui.add(
            egui::TextEdit::singleline(&mut self.edit.tile_filter)
                .hint_text(crate::i18n::t("Filtrer…"))
                .desired_width(f32::INFINITY),
        );
        self.thumb_options(ui);
        self.tile_tools(ui, &names);
        let needle = self.edit.tile_filter.to_lowercase();
        let family = self.analysis.family.clone();
        let mut rows: Vec<usize> = (2..names.len())
            .filter(|&i| {
                (needle.is_empty() || names[i].to_lowercase().contains(&needle))
                    && (family.is_empty() || fo_core::tile_stats::family_of(&names[i]) == family)
            })
            .collect();
        // Les tuiles déjà utilisées sur la carte (à l'élévation courante) passent en premier.
        let used: std::collections::HashSet<usize> = self
            .current
            .as_ref()
            .map(|c| {
                (0..10_000)
                    .filter_map(|s| c.map.tile(self.elevation, s))
                    .flat_map(|t| [t.floor as usize, t.roof as usize])
                    .collect()
            })
            .unwrap_or_default();
        let layer = if self.edit.tool == Tool::Roof {
            fo_core::map::TileLayer::Roof
        } else {
            fo_core::map::TileLayer::Floor
        };
        let suggested: std::collections::HashMap<usize, u32> = match self.analysis.stats(layer) {
            Some(stats) if self.analysis.suggest => stats
                .neighbors_of(self.edit.brush_tile)
                .into_iter()
                .map(|(id, n)| (id as usize, n))
                .collect(),
            _ => Default::default(),
        };
        // Voisines probables de la tuile choisie d'abord (les plus fréquentes en tête), puis les tuiles déjà sur la carte.
        rows.sort_by_key(|id| {
            (
                std::cmp::Reverse(suggested.get(id).copied().unwrap_or(0)),
                !used.contains(id),
            )
        });
        let (size, names_on) = (self.prefs.thumb_size as f32, self.prefs.show_names);
        let col_w = if names_on { size + 110.0 } else { size + 8.0 };
        let per_row = ((ui.available_width() / col_w).floor() as usize).max(1);
        let row_h = size + 8.0;
        let line_count = rows.len().div_ceil(per_row);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row_h, line_count, |ui, range| {
                for line in range {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for &id in rows.iter().skip(line * per_row).take(per_row) {
                            let Some(sprite) = art.tile_sprite(id) else {
                                continue;
                            };
                            let tex = self.texture_for(&ctx, &sprite);
                            let selected = self.edit.brush_tile as usize == id;
                            let label = format!("{id} {}", names[id]);
                            let clicked = thumb_cell(
                                ui,
                                tex,
                                (sprite.width as f32, sprite.height as f32),
                                size,
                                selected,
                                names_on.then_some(label.as_str()),
                                &label,
                                col_w - 6.0,
                            );
                            if clicked {
                                self.edit.brush_tile = id as u16;
                            }
                        }
                    });
                }
            });
    }

    pub(crate) fn place_palette(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for ty in [
                ObjectType::Scenery,
                ObjectType::Wall,
                ObjectType::Item,
                ObjectType::Critter,
                ObjectType::Misc,
            ] {
                if ui
                    .selectable_label(self.edit.place_type == ty, crate::i18n::t(ty.label()))
                    .clicked()
                {
                    self.edit.place_type = ty;
                }
            }
        });
        let ty = self.edit.place_type;
        self.ensure_catalog(ty, ui.ctx());
        let Some(CatalogState::Ready(catalog)) = self.edit.catalogs.get(&ty) else {
            ui.label(crate::i18n::t("Chargement des prototypes…"));
            return;
        };
        let catalog = catalog.clone();
        let Some(g) = &self.game else { return };
        let art = g.art.clone();
        let ctx = ui.ctx().clone();
        match &self.edit.place {
            Some(p) => ui.label(format!(
                "{} : {} ({})",
                crate::i18n::t("Choisi"),
                if p.name.is_empty() { &p.file } else { &p.name },
                p.file
            )),
            None => ui.label(crate::i18n::t(
                "Choisissez un prototype, puis cliquez sur la carte.",
            )),
        };
        ui.add(
            egui::TextEdit::singleline(&mut self.edit.proto_filter)
                .hint_text(crate::i18n::t("Filtrer…"))
                .desired_width(f32::INFINITY),
        );
        self.thumb_options(ui);
        let needle = self.edit.proto_filter.to_lowercase();
        let rows: Vec<&ProtoEntry> = catalog
            .iter()
            .filter(|e| {
                needle.is_empty()
                    || e.name.to_lowercase().contains(&needle)
                    || e.file.to_lowercase().contains(&needle)
            })
            .collect();
        let (size, names_on) = (self.prefs.thumb_size as f32, self.prefs.show_names);
        let col_w = if names_on { size + 110.0 } else { size + 8.0 };
        let per_row = ((ui.available_width() / col_w).floor() as usize).max(1);
        let row_h = size + 8.0;
        let line_count = rows.len().div_ceil(per_row);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row_h, line_count, |ui, range| {
                for line in range {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for entry in rows.iter().skip(line * per_row).take(per_row) {
                            let label = if entry.name.is_empty() {
                                entry.file.clone()
                            } else {
                                format!("{}  ({})", entry.name, entry.file)
                            };
                            let selected =
                                self.edit.place.as_ref().is_some_and(|p| p.pid == entry.pid);
                            let clicked = match art.object_sprite(Fid(entry.info.fid), 0, 0) {
                                Some((_, sprite)) => {
                                    let tex = self.texture_for(&ctx, &sprite);
                                    thumb_cell(
                                        ui,
                                        tex,
                                        (sprite.width as f32, sprite.height as f32),
                                        size,
                                        selected,
                                        names_on.then_some(label.as_str()),
                                        &label,
                                        col_w - 6.0,
                                    )
                                }
                                None => ui.selectable_label(selected, &label).clicked(),
                            };
                            if clicked {
                                self.edit.place = Some((*entry).clone());
                            }
                        }
                    });
                }
            });
    }

    pub(crate) fn ensure_catalog(&mut self, ty: ObjectType, ctx: &egui::Context) {
        let Some(g) = &self.game else { return };
        match self.edit.catalogs.get(&ty) {
            Some(CatalogState::Ready(_)) => {}
            Some(CatalogState::Loading(rx)) => {
                if let Ok(list) = rx.try_recv() {
                    self.edit
                        .catalogs
                        .insert(ty, CatalogState::Ready(Arc::new(list)));
                } else {
                    ctx.request_repaint_after(Duration::from_millis(100));
                }
            }
            None => {
                let (tx, rx) = mpsc::channel();
                let data = g.data.clone();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(data.proto_catalog(ty));
                    ctx.request_repaint();
                });
                self.edit.catalogs.insert(ty, CatalogState::Loading(rx));
            }
        }
    }
}

/// Une vignette carrée (l'image tient dans `size`×`size`), avec ou sans nom à droite. Renvoie vrai si on a cliqué dessus.
pub(crate) fn thumb_cell(
    ui: &mut egui::Ui,
    tex: egui::TextureId,
    dims: (f32, f32),
    size: f32,
    selected: bool,
    name: Option<&str>,
    tooltip: &str,
    col_w: f32,
) -> bool {
    let (w, h) = dims;
    let scale = (size / w.max(1.0))
        .min(size / h.max(1.0))
        .min(1.0_f32.max(size / 64.0));
    let img = Vec2::new(w * scale, h * scale);
    let cell = Vec2::new(size + 6.0, size + 6.0);
    let total = match name {
        Some(_) => Vec2::new(col_w.max(cell.x), cell.y),
        None => cell,
    };
    let (rect, response) = ui.allocate_exact_size(total, egui::Sense::click());
    let visuals = ui.visuals();
    if selected {
        ui.painter()
            .rect_filled(rect, 4.0, visuals.selection.bg_fill);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, 4.0, visuals.widgets.hovered.weak_bg_fill);
    }
    let image_rect =
        Rect::from_center_size(Pos2::new(rect.min.x + cell.x / 2.0, rect.center().y), img);
    ui.painter().image(
        tex,
        image_rect,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
    if let Some(n) = name {
        let color = if selected {
            visuals.selection.stroke.color
        } else {
            visuals.text_color()
        };
        ui.painter().with_clip_rect(rect).text(
            Pos2::new(rect.min.x + cell.x + 4.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            n,
            egui::TextStyle::Body.resolve(ui.style()),
            color,
        );
    }
    response.on_hover_text(tooltip).clicked()
}
