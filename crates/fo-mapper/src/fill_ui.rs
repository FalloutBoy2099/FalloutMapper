//! « Remplir d'après une carte du jeu » : la sélection de cases reçoit un sol recomposé par patchs d'une carte de référence (désert, collines…)
//! et, au choix, des décors semés comme dans cette carte. Un seul pas d'historique ; « Autre tirage » défait et recommence avec une autre graine.

use crate::app::MapperApp;
use crate::i18n::t;
use eframe::egui;
use fo_core::map::{TileLayer, read_map};
use fo_core::procgen::{Area, scatter};
use fo_core::quilt::{Reference, quilt};
use fo_core::village::{PaletteItem, learn_palette, pick_weighted, place_scenery};
use fo_core::{Map, geom};
use std::collections::HashSet;
use std::sync::Arc;

const UNDO_LABEL: &str = "Synthèse de sol";

pub(crate) struct FillState {
    /// Carte du jeu qui sert de modèle (nom de fichier).
    pub map: String,
    pub patch: i32,
    /// Semer aussi des décors, et combien pour mille hexagones de la zone.
    pub scatter: bool,
    pub density: i32,
    pub filter: String,
    seed: u64,
    /// Référence chargée : (carte, sols, toits, décors appris).
    loaded: Option<Loaded>,
}

struct Loaded {
    file: String,
    floor: Reference,
    roof: Reference,
    palette: Arc<Vec<PaletteItem>>,
}

impl Default for FillState {
    fn default() -> Self {
        Self {
            map: "desert1".into(),
            patch: 4,
            scatter: false,
            density: 18,
            filter: String::new(),
            seed: 1,
            loaded: None,
        }
    }
}

/// Hexagones couverts par des cases carrées (une case en couvre quatre).
fn hexes_of(squares: &[usize]) -> HashSet<u32> {
    let mut out = HashSet::new();
    for &sq in squares {
        let (c, r) = ((sq % 100) as u32, (sq / 100) as u32);
        for (dc, dr) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let (hc, hr) = (2 * c + dc, 2 * r + dr);
            if hc < geom::HEX_GRID_WIDTH && hr < geom::HEX_GRID_WIDTH {
                out.insert(hr * geom::HEX_GRID_WIDTH + hc);
            }
        }
    }
    out
}

impl MapperApp {
    /// Charge (une fois) la carte de référence choisie.
    fn fill_reference(&mut self) -> Option<()> {
        let want = self.fill.map.clone();
        if self.fill.loaded.as_ref().is_some_and(|l| l.file == want) {
            return Some(());
        }
        let game = self.game.as_ref()?;
        let bytes = game.data.read_map_bytes(&want).ok()?;
        let map = read_map(&bytes, &*game.data).ok()?;
        let palette = Arc::new(learn_palette(&map, &game.data, 14));
        self.fill.loaded = Some(Loaded {
            file: want,
            floor: Reference::from_map(&map, 0, TileLayer::Floor),
            roof: Reference::from_map(&map, 0, TileLayer::Roof),
            palette,
        });
        Some(())
    }

    /// Applique la synthèse à `squares` (couche `layer`) ; `again` défait d'abord la dernière synthèse.
    pub(crate) fn fill_apply(&mut self, layer: TileLayer, squares: &[usize], again: bool) {
        if again && self.edit.undo_labels.last().copied() == Some(UNDO_LABEL) {
            self.undo();
        }
        if self.fill_reference().is_none() {
            self.status = t("Carte de référence illisible.").into();
            return;
        }
        self.fill.seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(7, |d| d.as_nanos() as u64);
        let Some(loaded) = self.fill.loaded.as_ref() else {
            return;
        };
        let reference = if layer == TileLayer::Floor {
            loaded.floor.clone()
        } else {
            loaded.roof.clone()
        };
        let palette = loaded.palette.clone();
        let (patch, seed, e) = (
            self.fill.patch.max(2) as usize,
            self.fill.seed,
            self.elevation,
        );
        let (do_scatter, density) = (
            self.fill.scatter && layer == TileLayer::Floor,
            self.fill.density.max(0) as usize,
        );
        let squares = squares.to_vec();
        let mut placed = 0;
        let mut decor = 0;
        let mut missing_blocks = false;
        self.mutate_as(UNDO_LABEL, |m, game| {
            match quilt(m, e, layer, &squares, &reference, patch, seed) {
                Some(r) => placed = r.placed,
                None => {
                    missing_blocks = true;
                    return false;
                }
            }
            if do_scatter && !palette.is_empty() {
                decor = scatter_decor(m, game, e, &squares, &palette, density, seed);
            }
            placed + decor > 0
        });
        self.status = if missing_blocks {
            t("La carte de référence n'a pas de blocs assez grands : réduisez la taille des blocs.")
                .to_string()
        } else {
            format!(
                "{placed} {} {}",
                t("case(s) remplie(s)"),
                if decor > 0 {
                    format!("· {decor} {}", t("décor(s) semé(s)"))
                } else {
                    String::new()
                }
            )
        };
    }

    /// Bloc « Synthèse d'après une carte du jeu » de la palette de tuiles.
    pub(crate) fn fill_section(&mut self, ui: &mut egui::Ui, layer: TileLayer, squares: &[usize]) {
        ui.separator();
        ui.label(egui::RichText::new(t("Synthèse d'après une carte du jeu")).strong());
        ui.label(egui::RichText::new(t("Recopie des blocs de la carte choisie : les transitions viennent de la carte d'origine.")).weak());
        let maps: Vec<(String, String)> = self
            .game
            .as_ref()
            .map(|g| {
                g.maps
                    .iter()
                    .map(|m| {
                        (
                            m.file.clone(),
                            m.display
                                .clone()
                                .filter(|d| !d.is_empty())
                                .map_or_else(|| m.file.clone(), |d| format!("{d} ({})", m.file)),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let shown = maps
            .iter()
            .find(|(f, _)| *f == self.fill.map)
            .map_or(self.fill.map.clone(), |(_, l)| l.clone());
        ui.horizontal(|ui| {
            ui.label(t("Modèle"));
            egui::ComboBox::from_id_salt("fill_reference")
                .selected_text(shown)
                .width(220.0)
                .show_ui(ui, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.fill.filter)
                            .hint_text(t("Filtrer…"))
                            .desired_width(f32::INFINITY),
                    );
                    let needle = self.fill.filter.to_lowercase();
                    for (file, label) in maps
                        .iter()
                        .filter(|(f, l)| {
                            needle.is_empty()
                                || f.contains(&needle)
                                || l.to_lowercase().contains(&needle)
                        })
                        .take(200)
                    {
                        ui.selectable_value(&mut self.fill.map, file.clone(), label);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label(t("Taille des blocs"));
            ui.add(crate::stepper::Stepper::new(&mut self.fill.patch).range(2..=10));
        });
        ui.checkbox(
            &mut self.fill.scatter,
            t("Semer aussi des décors de cette carte"),
        );
        if self.fill.scatter {
            ui.horizontal(|ui| {
                ui.label(t("Décors pour 1000 hexagones"));
                ui.add(crate::stepper::Stepper::new(&mut self.fill.density).range(1..=200));
            });
        }
        let can = !squares.is_empty() && self.game.is_some();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(can, egui::Button::new(t("Remplir la sélection")))
                .on_hover_text(t(
                    "Recompose la sélection à partir de blocs de la carte modèle",
                ))
                .clicked()
            {
                self.fill_apply(layer, squares, false);
            }
            let can_again = can && self.edit.undo_labels.last().copied() == Some(UNDO_LABEL);
            if ui
                .add_enabled(can_again, egui::Button::new(t("Autre tirage")))
                .on_hover_text(t("Défait la dernière synthèse et en refait une autre"))
                .clicked()
            {
                self.fill_apply(layer, squares, true);
            }
        });
        if !can {
            ui.weak(t(
                "Sélectionnez des cases (outil Sélection, filtre Sols ou Toits).",
            ));
        }
    }
}

/// Sème des décors de `palette` dans les hexagones libres de la zone ; renvoie leur nombre.
fn scatter_decor(
    m: &mut Map,
    game: &fo_core::GameData,
    e: usize,
    squares: &[usize],
    palette: &[PaletteItem],
    per_thousand: usize,
    seed: u64,
) -> usize {
    let region = hexes_of(squares);
    if region.is_empty() {
        return 0;
    }
    let (mut c0, mut c1, mut r0, mut r1) = (u32::MAX, 0, u32::MAX, 0);
    for &h in &region {
        let (c, r) = geom::hex_col_row(h);
        (c0, c1, r0, r1) = (c0.min(c), c1.max(c), r0.min(r), r1.max(r));
    }
    let area = Area { c0, r0, c1, r1 };
    let occupied: HashSet<u32> = m.objects[e]
        .iter()
        .filter(|o| o.tile() >= 0)
        .flat_map(|o| std::iter::once(o.tile() as u32).chain(geom::hex_neighbors(o.tile() as u32)))
        .collect();
    let mut blocked: HashSet<u32> = occupied;
    for r in r0..=r1 {
        for c in c0..=c1 {
            let h = r * geom::HEX_GRID_WIDTH + c;
            if !region.contains(&h) {
                blocked.insert(h);
            }
        }
    }
    let count = region.len() * per_thousand / 1000;
    let mut rng = fo_core::procgen::Rng::new(seed ^ 0xDEC0);
    let mut placed = 0;
    for hex in scatter(&area, &blocked, count, 3, seed ^ 0x77, false) {
        if let Some(item) = pick_weighted(palette, &mut rng).cloned()
            && place_scenery(m, game, &item, hex, e)
        {
            placed += 1;
        }
    }
    placed
}
