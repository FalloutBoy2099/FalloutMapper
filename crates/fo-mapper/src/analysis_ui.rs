//! Analyse des tuiles de toutes les cartes : suggestions de voisines, remplissage plausible d'une zone, familles de tuiles
//! (préfixe du nom) et remplacement d'une famille par une autre (changer un bâtiment en un autre).

use crate::app::MapperApp;
use crate::i18n::t;
use eframe::egui;
use fo_core::map::{TileLayer, read_map};
use fo_core::room::WallStats;
use fo_core::tile_stats::{TileStats, equivalent_in, families, family_of, generate_fill};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, channel};

/// Résultat d'une analyse : statistiques des sols et des toits, et nombre de cartes lues.
pub(crate) struct Analysis {
    pub floor: Arc<TileStats>,
    pub roof: Arc<TileStats>,
    pub walls: Arc<WallStats>,
    /// Constructions repérées dans les cartes du jeu (murs, décors, sol, toit), prêtes à être reposées.
    pub buildings: Arc<Vec<fo_core::buildings::Building>>,
    pub maps: usize,
}

#[derive(Default)]
pub(crate) struct AnalysisState {
    pub result: Option<Analysis>,
    rx: Option<Receiver<Analysis>>,
    progress: Arc<AtomicUsize>,
    total: usize,
    /// Trier la palette selon les voisines de la tuile choisie.
    pub suggest: bool,
    /// Famille retenue pour filtrer la palette (vide : toutes).
    pub family: String,
    /// Famille d'arrivée du remplacement.
    pub replace_to: String,
}

impl AnalysisState {
    pub(crate) fn running(&self) -> bool {
        self.rx.is_some()
    }

    pub(crate) fn stats(&self, layer: TileLayer) -> Option<&Arc<TileStats>> {
        self.result.as_ref().map(|r| {
            if layer == TileLayer::Floor {
                &r.floor
            } else {
                &r.roof
            }
        })
    }
}

impl MapperApp {
    /// Lit toutes les cartes du jeu en arrière-plan et compte les voisinages de tuiles.
    pub(crate) fn start_tile_analysis(&mut self, ctx: &egui::Context) {
        let Some(g) = &self.game else { return };
        if self.analysis.running() {
            return;
        }
        let data = g.data.clone();
        let files: Vec<String> = g.maps.iter().map(|m| m.file.clone()).collect();
        let (tx, rx) = channel();
        let progress = Arc::new(AtomicUsize::new(0));
        self.analysis.progress = progress.clone();
        self.analysis.total = files.len();
        self.analysis.rx = Some(rx);
        self.status = format!("{} 0 / {}", t("Analyse des tuiles…"), files.len());
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let (mut floor, mut roof) = (TileStats::default(), TileStats::default());
            let mut walls = WallStats::default();
            let mut buildings = Vec::new();
            let mut read = 0;
            for f in &files {
                if let Ok(bytes) = data.read_map_bytes(f)
                    && let Ok(map) = read_map(&bytes, &*data)
                {
                    floor.add_map(&map, TileLayer::Floor);
                    roof.add_map(&map, TileLayer::Roof);
                    walls.add_map(&map);
                    buildings.extend(fo_core::buildings::detect(f, &map));
                    read += 1;
                }
                progress.fetch_add(1, Ordering::Relaxed);
                ctx.request_repaint();
            }
            let _ = tx.send(Analysis {
                floor: Arc::new(floor),
                roof: Arc::new(roof),
                walls: Arc::new(walls),
                buildings: Arc::new(buildings),
                maps: read,
            });
            ctx.request_repaint();
        });
    }

    pub(crate) fn poll_tile_analysis(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.analysis.rx {
            if let Ok(done) = rx.try_recv() {
                self.status = format!(
                    "{}: {} {}, {} {}",
                    t("Analyse terminée"),
                    done.maps,
                    t("cartes"),
                    done.floor.distinct(),
                    t("sols différents")
                );
                self.analysis.result = Some(done);
                self.analysis.rx = None;
            } else {
                let n = self.analysis.progress.load(Ordering::Relaxed);
                self.status = format!("{} {n} / {}", t("Analyse des tuiles…"), self.analysis.total);
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
    }

    /// Outils de la palette de tuiles : analyse, suggestions, familles, génération et remplacement.
    pub(crate) fn tile_tools(&mut self, ui: &mut egui::Ui, names: &[String]) {
        let layer = if self.edit.tool == crate::edit_ui::Tool::Roof {
            TileLayer::Roof
        } else {
            TileLayer::Floor
        };
        ui.collapsing(t("Analyse et génération"), |ui| {
            if self.analysis.running() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(&self.status);
                });
            } else if ui.button(if self.analysis.result.is_some() { t("Relancer l'analyse de toutes les cartes") } else { t("Analyser les tuiles de toutes les cartes") }).clicked() {
                let ctx = ui.ctx().clone();
                self.start_tile_analysis(&ctx);
            }
            let stats = self.analysis.stats(layer).cloned();
            ui.add_enabled(stats.is_some(), egui::Checkbox::new(&mut self.analysis.suggest, t("Suggérer les voisines de la tuile choisie en premier")));

            let fams = families(names);
            let brush_family = names.get(self.edit.brush_tile as usize).map(|n| family_of(n)).unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label(t("Famille"));
                egui::ComboBox::from_id_salt("tile_family").selected_text(if self.analysis.family.is_empty() { t("Toutes").to_string() } else { self.analysis.family.clone() }).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.analysis.family, String::new(), t("Toutes"));
                    for f in fams.keys() {
                        ui.selectable_value(&mut self.analysis.family, f.clone(), f);
                    }
                });
                if !brush_family.is_empty() && ui.small_button(t("Celle du pinceau")).clicked() {
                    self.analysis.family = brush_family.clone();
                }
            });

            let squares: Vec<usize> = self.selection.tiles().iter().filter(|r| r.layer == layer).map(|r| r.square as usize).collect();
            ui.separator();
            ui.label(format!("{} : {}", t("Cases sélectionnées"), squares.len()));
            let can_generate = stats.is_some() && !squares.is_empty();
            if ui.add_enabled(can_generate, egui::Button::new(t("Remplir la sélection de façon plausible"))).on_hover_text(t("Pose des tuiles qui vont avec leurs voisines, d'après toutes les cartes analysées")).clicked()
                && let Some(stats) = stats.clone()
            {
                let e = self.elevation;
                let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.as_nanos() as u64);
                let mut n = 0;
                self.mutate_as("Remplissage plausible", |m, _| {
                    n = generate_fill(m, e, layer, &squares, &stats, seed);
                    n > 0
                });
                self.status = format!("{n} {}", t("case(s) remplie(s)"));
            }

            self.fill_section(ui, layer, &squares);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(t("Remplacer la famille"));
                ui.label(egui::RichText::new(if brush_family.is_empty() { "—".to_string() } else { brush_family.clone() }).strong());
                ui.label("→");
                egui::ComboBox::from_id_salt("tile_replace_to").selected_text(self.analysis.replace_to.clone()).show_ui(ui, |ui| {
                    for f in fams.keys() {
                        ui.selectable_value(&mut self.analysis.replace_to, f.clone(), f);
                    }
                });
            });
            let (from, to) = (brush_family, self.analysis.replace_to.clone());
            let can_replace = !from.is_empty() && !to.is_empty() && from != to;
            let hint = t("Remplace les tuiles de la famille du pinceau par leur équivalent (même rang) dans la famille d'arrivée");
            ui.horizontal(|ui| {
                let on_selection = ui.add_enabled(can_replace && !squares.is_empty(), egui::Button::new(t("… dans la sélection"))).on_hover_text(hint).clicked();
                let on_all = ui.add_enabled(can_replace, egui::Button::new(t("… dans toute l'élévation"))).on_hover_text(hint).clicked();
                if on_selection || on_all {
                    let e = self.elevation;
                    let targets: Option<Vec<usize>> = on_selection.then(|| squares.clone());
                    let mut n = 0;
                    self.mutate_as("Remplacement de famille", |m, _| {
                        let all: Vec<usize> = (0..10_000).collect();
                        for &sq in targets.as_ref().unwrap_or(&all) {
                            let Some(tile) = m.tile(e, sq) else { continue };
                            let id = if layer == TileLayer::Floor { tile.floor } else { tile.roof };
                            let Some(name) = names.get(id as usize) else { continue };
                            if family_of(name) != from {
                                continue;
                            }
                            if let Some(new) = equivalent_in(&fams, &from, &to, id)
                                && m.set_tile(e, sq, layer, new)
                            {
                                n += 1;
                            }
                        }
                        n > 0
                    });
                    self.status = format!("{n} {}", t("tuile(s) remplacée(s)"));
                }
            });
        });
    }
}
