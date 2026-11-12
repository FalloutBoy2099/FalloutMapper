//! Fenêtre « Générer une carte » : des paramètres (taille, bâtiments, routes, sorties, végétation…) et une graine produisent un village
//! cohérent dans un nouvel onglet. « Relancer » tire une autre graine et remplace la carte générée (tant qu'elle n'a pas été renommée ou
//! fermée). Le style des murs, les familles de sol et de toit viennent des mêmes réglages que l'outil Pièce.

use crate::app::MapperApp;
use crate::i18n::t;
use crate::ui_kit::FooterButton as B;
use eframe::egui;
use fo_core::procgen::{Params, Side};
use fo_core::tile_stats::families;
use fo_core::village::{GenOptions, GenReport, PaletteItem, generate, learn_palette};
use fo_core::{Map, read_map};
use std::sync::Arc;

pub(crate) struct GenState {
    pub open: bool,
    pub params: Params,
    pub name: String,
    pub ground: String,
    pub road: String,
    pub road_width: u32,
    /// Carte du jeu dont on apprend les décors à semer.
    pub reference: String,
    palette: Option<(String, Arc<Vec<PaletteItem>>)>,
    /// Nom de la dernière carte générée : « Relancer » la remplace tant qu'elle est ouverte.
    last: Option<String>,
    pub report: Option<GenReport>,
}

impl Default for GenState {
    fn default() -> Self {
        Self {
            open: false,
            params: Params::default(),
            name: "genvill".into(),
            ground: String::new(),
            road: String::new(),
            road_width: 2,
            reference: String::new(),
            palette: None,
            last: None,
            report: None,
        }
    }
}

fn random_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(7, |d| d.as_nanos() as u64)
        | 1
}

impl MapperApp {
    pub(crate) fn open_generator(&mut self) {
        if self.game.is_none() {
            return;
        }
        self.procgen.params.seed = random_seed();
        self.procgen.open = true;
    }

    /// Familles de sol et carte modèle par défaut tant que rien n'est choisi : le sol de la carte modèle est sa famille de tuiles la plus
    /// fréquente, celui des routes une famille « road » (ou la deuxième plus fréquente).
    fn gen_defaults(&mut self, fam_names: &[String]) {
        if self.procgen.reference.is_empty() {
            let maps = self
                .game
                .as_ref()
                .map(|g| g.maps.clone())
                .unwrap_or_default();
            self.procgen.reference = maps
                .iter()
                .find(|m| m.file == "arvillag")
                .or_else(|| maps.first())
                .map(|m| m.file.clone())
                .unwrap_or_default();
        }
        // Sol et toit des bâtiments : ceux de l'outil Pièce, sinon une famille « intérieur », et le toit le plus courant des cartes.
        if !self.edit.room.auto_floor && !fam_names.contains(&self.edit.room.family) {
            self.edit.room.family = fam_names
                .iter()
                .find(|k| k.to_lowercase().starts_with("hola"))
                .or_else(|| fam_names.first())
                .cloned()
                .unwrap_or_default();
        }
        if !self.edit.room.auto_roof && !fam_names.contains(&self.edit.room.roof_family) {
            let names = self
                .game
                .as_ref()
                .map(|g| g.data.list("art/tiles/tiles.lst"))
                .unwrap_or_default();
            let common = self
                .analysis
                .stats(fo_core::map::TileLayer::Roof)
                .and_then(|s| s.pick(None, None, 0))
                .and_then(|id| names.get(id as usize))
                .map(|n| fo_core::tile_stats::family_of(n))
                .filter(|f| fam_names.contains(f));
            self.edit.room.roof_family = common
                .or_else(|| fam_names.first().cloned())
                .unwrap_or_default();
        }
        if fam_names.contains(&self.procgen.ground) && fam_names.contains(&self.procgen.road) {
            return;
        }
        let ranked = self.reference_families();
        let lower = |p: &str| {
            fam_names
                .iter()
                .find(|k| k.to_lowercase().starts_with(p))
                .cloned()
        };
        if !fam_names.contains(&self.procgen.ground) {
            self.procgen.ground = ranked
                .first()
                .cloned()
                .or_else(|| lower("tp"))
                .or_else(|| fam_names.first().cloned())
                .unwrap_or_default();
        }
        if !fam_names.contains(&self.procgen.road) {
            self.procgen.road = lower("road")
                .or_else(|| ranked.get(1).cloned())
                .or_else(|| fam_names.first().cloned())
                .unwrap_or_default();
        }
    }

    /// Familles de tuiles de sol de la carte modèle, de la plus à la moins fréquente.
    fn reference_families(&self) -> Vec<String> {
        let Some(g) = self.game.as_ref() else {
            return Vec::new();
        };
        let Some(map) = g
            .data
            .read_map_bytes(&self.procgen.reference)
            .ok()
            .and_then(|b| read_map(&b, &*g.data).ok())
        else {
            return Vec::new();
        };
        let names = g.data.list("art/tiles/tiles.lst");
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        if let Some(tiles) = &map.tiles[0] {
            for t in tiles.iter() {
                if t.floor != fo_core::map::EMPTY_TILE
                    && let Some(n) = names.get(t.floor as usize)
                {
                    *counts.entry(fo_core::tile_stats::family_of(n)).or_default() += 1;
                }
            }
        }
        let mut v: Vec<(String, usize)> = counts.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v.into_iter().map(|(k, _)| k).collect()
    }

    /// Décors appris de la carte modèle (lue une fois).
    fn gen_palette(&mut self) -> Arc<Vec<PaletteItem>> {
        let wanted = self.procgen.reference.clone();
        if let Some((name, p)) = &self.procgen.palette
            && *name == wanted
        {
            return p.clone();
        }
        let items = self
            .game
            .as_ref()
            .and_then(|g| {
                g.data
                    .read_map_bytes(&wanted)
                    .ok()
                    .and_then(|b| read_map(&b, &*g.data).ok())
                    .map(|m| learn_palette(&m, &g.data, 14))
            })
            .unwrap_or_default();
        let arc = Arc::new(items);
        self.procgen.palette = Some((wanted, arc.clone()));
        arc
    }

    /// Construit la carte et l'installe dans l'onglet (nouveau, ou le même s'il s'agit d'une régénération).
    pub(crate) fn run_generator(&mut self) {
        self.refresh_room_models();
        let Some(model) = self.room_model() else {
            self.status =
                t("Analysez d'abord toutes les cartes et choisissez un style de murs.").into();
            return;
        };
        let names = self.floor_names();
        let fams = families(&names);
        let fam_names: Vec<String> = fams.keys().cloned().collect();
        self.gen_defaults(&fam_names);
        let ids = |name: &str| fams.get(name).cloned().unwrap_or_default();
        let floor = if self.edit.room.floor {
            self.room_floor_ids()
        } else {
            Vec::new()
        };
        let roof = if self.edit.room.roof {
            self.room_roof_ids()
        } else {
            Vec::new()
        };
        let palette = self.gen_palette();
        let opts = GenOptions {
            params: self.procgen.params.clone(),
            ground: ids(&self.procgen.ground),
            road: ids(&self.procgen.road),
            floor,
            roof,
            road_width: self.procgen.road_width,
            palette: (*palette).clone(),
            windows: self.edit.room.windows.max(0) as u32,
            furniture: self.edit.room.furniture.max(0) as u32,
            lights: self.edit.room.lights,
            floor_stats: self.analysis.stats(fo_core::map::TileLayer::Floor).cloned(),
            roof_stats: self.analysis.stats(fo_core::map::TileLayer::Roof).cloned(),
        };
        if opts.ground.is_empty() {
            self.status = t("Choisissez une famille de sol pour la carte.").into();
            return;
        }
        let Some(g) = self.game.as_ref() else { return };
        let name = self.procgen.name.clone();
        let mut map = Map::new_empty(&name, 1);
        let report = generate(&mut map, &g.data, &model, &opts);
        // Chaque onglet a ses propres réglages d'édition : on emporte ceux de l'outil Pièce dans le nouvel onglet.
        let room = {
            let r = &self.edit.room;
            (
                r.style.clone(),
                r.family.clone(),
                r.roof,
                r.roof_family.clone(),
                r.windows,
                r.furniture,
                r.lights,
                r.floor,
                r.auto_floor,
                r.auto_roof,
            )
        };
        let replace = self.procgen.last.as_deref() == Some(name.as_str())
            && self.current.as_ref().is_some_and(|c| c.file == name);
        if !replace {
            if self.current.is_some() {
                self.open_new_tab();
            }
            self.ensure_tab();
        }
        self.install_new_map(&name, map);
        self.start_view = true;
        {
            let r = &mut self.edit.room;
            (
                r.style,
                r.family,
                r.roof,
                r.roof_family,
                r.windows,
                r.furniture,
                r.lights,
                r.floor,
                r.auto_floor,
                r.auto_roof,
            ) = room;
        }
        self.show_lights = report.lights > 0;
        self.procgen.last = Some(name);
        self.status = format!(
            "{} : {} {}, {} {}, {} {}, {} {}{}",
            t("Carte générée"),
            report.buildings,
            t("bâtiment(s)"),
            report.exits,
            t("sortie(s)"),
            report.decor,
            t("décor(s)"),
            report.furniture,
            t("meuble(s)"),
            if report.unreachable > 0 || report.doors_unlinked > 0 {
                format!(
                    " — {}",
                    t("attention : des portes ou sorties restent inaccessibles")
                )
            } else {
                String::new()
            }
        );
        self.procgen.report = Some(report);
    }

    pub(crate) fn generator_window(&mut self, ctx: &egui::Context) {
        if !self.procgen.open {
            return;
        }
        self.refresh_room_models();
        let names = self.floor_names();
        let fams = families(&names);
        let fam_names: Vec<String> = fams.keys().cloned().collect();
        self.gen_defaults(&fam_names);
        let maps: Vec<(String, String)> = self
            .game
            .as_ref()
            .map(|g| {
                g.maps
                    .iter()
                    .map(|m| {
                        (
                            m.file.clone(),
                            m.display.clone().unwrap_or_else(|| m.file.clone()),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let analysed = self.analysis.result.is_some();
        let mut open = true;
        let mut action = None;
        let max_h = (ctx.content_rect().height() - 120.0).max(300.0);
        egui::Window::new(t("Générer une carte")).open(&mut open).collapsible(false).resizable(false).order(egui::Order::Middle).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(ctx, |ui| {
          egui::ScrollArea::vertical().max_height(max_h).auto_shrink([true, true]).show(ui, |ui| {
            ui.set_width(430.0);
            if !analysed {
                ui.colored_label(egui::Color32::from_rgb(255, 190, 80), t("Le style des murs et du mobilier vient de l'analyse de toutes les cartes du jeu."));
                if ui.add_enabled(!self.analysis.running(), egui::Button::new(t("Analyser les cartes"))).clicked() {
                    self.start_tile_analysis(ctx);
                }
            }
            let p = &mut self.procgen.params;
            egui::Grid::new("gen_grid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label(t("Nom de la carte"));
                ui.add(egui::TextEdit::singleline(&mut self.procgen.name).desired_width(120.0).char_limit(8));
                ui.end_row();
                ui.label(t("Graine"));
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut p.seed));
                    if ui.button(t("Relancer")).on_hover_text(t("Tire une autre graine et régénère la carte")).clicked() {
                        p.seed = random_seed();
                        action = Some(true);
                    }
                });
                ui.end_row();
                ui.label(t("Taille (hexagones)"));
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut p.cols, 60..=190).text(t("largeur")));
                    ui.add(egui::Slider::new(&mut p.rows, 60..=190).text(t("hauteur")));
                });
                ui.end_row();
                ui.label(t("Bâtiments"));
                ui.add(egui::Slider::new(&mut p.buildings, 0..=20));
                ui.end_row();
                ui.label(t("Pièces par bâtiment (au plus)"));
                ui.add(egui::Slider::new(&mut p.max_rooms, 1..=3));
                ui.end_row();
                ui.label(t("Taille d'une pièce"));
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut p.room_cols, 5..=14).text(t("longueur")));
                    ui.add(egui::Slider::new(&mut p.room_rows, 4..=14).text(t("profondeur")));
                });
                ui.end_row();
                ui.label(t("Espace entre bâtiments"));
                ui.add(egui::Slider::new(&mut p.spacing, 2..=14));
                ui.end_row();
                ui.label(t("Sorties vers la carte du monde"));
                ui.horizontal(|ui| {
                    for side in Side::ALL {
                        let mut on = p.exits.contains(&side);
                        if ui.checkbox(&mut on, t(side.label())).changed() {
                            if on {
                                p.exits.push(side);
                            } else {
                                p.exits.retain(|s| *s != side);
                            }
                        }
                    }
                });
                ui.end_row();
                ui.label(t("Largeur des routes"));
                ui.add(egui::Slider::new(&mut self.procgen.road_width, 1..=3));
                ui.end_row();
                ui.label(t("Végétation"));
                ui.add(egui::Slider::new(&mut p.vegetation, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));
                ui.end_row();
                ui.label(t("Menus objets"));
                ui.add(egui::Slider::new(&mut p.props, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));
                ui.end_row();
                ui.label(t("Bordure"));
                ui.checkbox(&mut p.border_trees, t("Bande de végétation dense au bord"));
                ui.end_row();
            });
            ui.separator();
            egui::Grid::new("gen_style").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label(t("Décor appris de la carte"));
                let shown = maps.iter().find(|(f, _)| *f == self.procgen.reference).map_or(self.procgen.reference.clone(), |(_, d)| d.clone());
                egui::ComboBox::from_id_salt("gen_ref").selected_text(shown).width(220.0).show_ui(ui, |ui| {
                    for (file, display) in &maps {
                        ui.selectable_value(&mut self.procgen.reference, file.clone(), display);
                    }
                });
                ui.end_row();
                ui.label(t("Sol de la carte"));
                egui::ComboBox::from_id_salt("gen_ground").selected_text(self.procgen.ground.clone()).show_ui(ui, |ui| {
                    for f in &fam_names {
                        ui.selectable_value(&mut self.procgen.ground, f.clone(), f);
                    }
                });
                ui.end_row();
                ui.label(t("Sol des routes"));
                egui::ComboBox::from_id_salt("gen_road").selected_text(self.procgen.road.clone()).show_ui(ui, |ui| {
                    for f in &fam_names {
                        ui.selectable_value(&mut self.procgen.road, f.clone(), f);
                    }
                });
                ui.end_row();
                ui.label(t("Style des murs"));
                let current = self.edit.room.style.clone();
                egui::ComboBox::from_id_salt("gen_style_walls").selected_text(current).show_ui(ui, |ui| {
                    let labels: Vec<String> = self.edit.room.styles().iter().map(|s| s.label.clone()).collect();
                    for l in labels {
                        ui.selectable_value(&mut self.edit.room.style, l.clone(), l);
                    }
                });
                ui.end_row();
                ui.label(t("Sol des bâtiments"));
                ui.checkbox(&mut self.edit.room.auto_floor, t("Sol assorti aux murs"));
                ui.end_row();
                ui.label(t("Famille de sol"));
                egui::ComboBox::from_id_salt("gen_floor").selected_text(self.edit.room.family.clone()).show_ui(ui, |ui| {
                    for f in &fam_names {
                        if ui.selectable_value(&mut self.edit.room.family, f.clone(), f).clicked() {
                            self.edit.room.auto_floor = false;
                        }
                    }
                });
                ui.end_row();
                ui.label(t("Toit"));
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.edit.room.roof, "");
                    ui.add_enabled_ui(self.edit.room.roof, |ui| {
                        egui::ComboBox::from_id_salt("gen_roof").selected_text(self.edit.room.roof_family.clone()).show_ui(ui, |ui| {
                            for f in &fam_names {
                                if ui.selectable_value(&mut self.edit.room.roof_family, f.clone(), f).clicked(){self.edit.room.auto_roof=false;}
                            }
                        });
                    });
                });
                ui.end_row();
                ui.label(t("Fenêtres et mobilier"));
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut self.edit.room.windows, 0..=6).text(t("fenêtres")));
                    ui.add(egui::Slider::new(&mut self.edit.room.furniture, 0..=12).text(t("meubles")));
                });
                ui.end_row();
                ui.label(t("Éclairage"));
                ui.checkbox(&mut self.edit.room.lights, t("Lumières dans les bâtiments"));
                ui.end_row();
            });
            if let Some(r) = &self.procgen.report {
                ui.separator();
                ui.weak(format!("{} {}, {} {}, {} {}, {} {}", r.buildings, t("bâtiment(s)"), r.doors_linked, t("porte(s) reliée(s) à la route"), r.decor, t("décor(s)"), r.blockers, t("bloqueur(s)")));
            }
            let valid = crate::file_ui::valid_map_name(&self.procgen.name);
            if !valid {
                ui.colored_label(egui::Color32::LIGHT_RED, t("Nom de fichier : 1 à 8 caractères a-z, 0-9 ou _."));
            }
            let ready = analysed && valid && !self.edit.room.style.is_empty();
            match crate::ui_kit::footer(ui, &[B::normal(t("Fermer")), B::primary(t("Générer")).enabled(ready)]) {
                Some(0) => action = Some(false),
                Some(_) => action = Some(true),
                None => {}
            }
          });
        });
        if !open || action == Some(false) {
            self.procgen.open = false;
        }
        if action == Some(true) {
            self.run_generator();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_parameters_are_sane() {
        let s = GenState::default();
        assert!(crate::file_ui::valid_map_name(&s.name));
        assert!(!s.params.exits.is_empty() && s.params.buildings > 0);
        assert!((1..=3).contains(&s.road_width));
    }
}
