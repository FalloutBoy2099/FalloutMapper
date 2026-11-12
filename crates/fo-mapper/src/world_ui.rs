//! Éditeur de `maps.txt` (liste des cartes du jeu) et carte du monde (zones de `city.txt` sur le fond de `worldmap.txt`).

use crate::app::MapperApp;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2};
use fo_core::ini_doc::IniDoc;
use fo_core::maps_txt;
use fo_core::worldmap::{
    AREA_SIZES, Area, Entrance, TerrainMap, apply_areas, is_random_area, parse_areas,
    parse_terrain, parse_tiles, placement_rule, roll_position,
};
use std::collections::HashMap;

const MAPS_TXT: &str = "data/maps.txt";
const CITY_TXT: &str = "data/city.txt";
const WORLDMAP_TXT: &str = "data/worldmap.txt";
/// Clés modifiables d'une carte dans maps.txt.
const MAP_KEYS: [&str; 7] = [
    "lookup_name",
    "map_name",
    "music",
    "ambient_sfx",
    "saved",
    "dead_bodies_age",
    "can_rest_here",
];

/// Texte → octets Latin-1 (les fichiers du jeu n'ont pas d'autre codage) ; un caractère hors Latin-1 devient `?`.
/// Traduit un problème signalé par `maps_txt::validate` (texte français de `fo-core`).
fn problem_text(p: &str) -> String {
    use crate::i18n::{quote, t, tr};
    let Some((section, msg)) = p.split_once(" : ") else {
        return tr(p);
    };
    if let Some(rest) = msg.strip_prefix("numéro attendu ")
        && let Some((n, _)) = rest.split_once(" (")
    {
        return format!(
            "{section} : {} {n} ({})",
            t("numéro attendu"),
            t("la liste ne doit avoir aucun trou")
        );
    }
    if let Some(field) = msg.strip_suffix(" manquant") {
        return format!("{section} : {field} {}", t("manquant"));
    }
    if let Some(rest) = msg.strip_prefix("map_name « ")
        && let Some((name, n)) = rest.split_once(" » déjà utilisé par la carte ")
    {
        return format!(
            "{section} : map_name {} {} {n}",
            quote(name),
            t("déjà utilisé par la carte")
        );
    }
    tr(p)
}

fn to_latin1(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| u8::try_from(c as u32).unwrap_or(b'?'))
        .collect()
}

#[derive(Debug)]
pub(crate) struct MapsTxtDialog {
    doc: IniDoc,
    selected: Option<usize>,
    dirty: bool,
}

pub(crate) struct WorldDialog {
    city: IniDoc,
    areas: Vec<Area>,
    selected: Option<usize>,
    background: Option<TextureHandle>,
    background_size: Vec2,
    zoom: f32,
    dirty: bool,
    pan: Vec2,
    filter: String,
    focus: Option<usize>,
    dragging: Option<usize>,
    open_area: Option<usize>,
    /// Terrain de la carte du monde (désert, montagne, ville, océan), pour placer les lieux aléatoires.
    terrain: TerrainMap,
    /// Position tirée au hasard des lieux aléatoires (aperçu : `city.txt` garde leur position d'origine).
    rolled: HashMap<usize, (i32, i32)>,
    seed: u64,
}

/// Position affichée d'une zone : celle tirée au hasard pour un lieu aléatoire, sinon celle du fichier.
fn shown_pos(areas: &[Area], rolled: &HashMap<usize, (i32, i32)>, i: usize) -> (i32, i32) {
    rolled
        .get(&i)
        .copied()
        .unwrap_or_else(|| areas.get(i).map_or((0, 0), |a| a.world_pos))
}

fn resolve_entrance_map<'a>(
    name: &str,
    maps: &'a [(String, String)],
) -> Option<&'a (String, String)> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    maps.iter()
        .find(|(lookup, _)| lookup.eq_ignore_ascii_case(name))
        .or_else(|| {
            let stem = name
                .rsplit_once('.')
                .filter(|(_, ext)| ext.eq_ignore_ascii_case("map"))
                .map_or(name, |(stem, _)| stem);
            maps.iter()
                .find(|(_, file)| file.eq_ignore_ascii_case(stem))
        })
}

fn world_map_choices(
    defs: &[fo_core::formats::MapDef],
    maps: &[fo_core::game::MapEntry],
) -> Vec<(String, String)> {
    maps.iter()
        .map(|map| {
            let lookup = defs
                .iter()
                .find(|d| d.file.eq_ignore_ascii_case(&map.file))
                .and_then(|d| d.display.clone())
                .unwrap_or_else(|| map.file.clone());
            (lookup, map.file.clone())
        })
        .collect()
}

/// Une carte peut avoir plusieurs entrées, même cachées au début de la partie.
fn area_maps(area: &Area, maps: &[(String, String)]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for entrance in &area.entrances {
        if let Some(target) = resolve_entrance_map(&entrance.map, maps)
            && !out
                .iter()
                .any(|(_, file)| file.eq_ignore_ascii_case(&target.1))
        {
            out.push(target.clone());
        }
    }
    out
}

/// (Re)tire la position d'un lieu aléatoire, ou de tous (`None`). Un nouveau tirage donne un autre endroit.
fn reroll(dlg: &mut WorldDialog, which: Option<usize>) {
    dlg.seed = dlg.seed.wrapping_add(0x9E37_79B9_7F4A_7C15).max(1);
    let indexes: Vec<usize> = match which {
        Some(i) => vec![i],
        None => (0..dlg.areas.len()).collect(),
    };
    for i in indexes {
        if !is_random_area(&dlg.areas, i) {
            continue;
        }
        // Chaque zone a sa propre graine : deux lieux aléatoires n'arrivent pas au même endroit.
        if let Some(pos) = roll_position(
            &dlg.terrain,
            &dlg.areas,
            i,
            dlg.seed ^ (i as u64 + 1).wrapping_mul(0xD1B5_4A32_D192_ED03),
        ) {
            // Éloigne aussi des lieux aléatoires déjà tirés.
            let too_close = dlg
                .rolled
                .iter()
                .any(|(j, p)| *j != i && (p.0 - pos.0).pow(2) + (p.1 - pos.1).pow(2) < 45 * 45);
            if !too_close {
                dlg.rolled.insert(i, pos);
            }
        }
    }
}

impl std::fmt::Debug for WorldDialog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldDialog")
            .field("areas", &self.areas.len())
            .finish()
    }
}

#[derive(Debug, Default)]
pub(crate) struct WorldUi {
    pub maps_txt: Option<MapsTxtDialog>,
    pub world: Option<WorldDialog>,
}

impl MapperApp {
    // ---------------------------------------------------------------- maps.txt

    pub(crate) fn open_maps_txt(&mut self) {
        let Some(g) = &self.game else { return };
        match g.data.read_data_text(MAPS_TXT) {
            Some(text) => {
                self.world.maps_txt = Some(MapsTxtDialog {
                    doc: IniDoc::parse(&text),
                    selected: None,
                    dirty: false,
                })
            }
            None => self.status = crate::i18n::t("data/maps.txt introuvable.").into(),
        }
    }

    pub(crate) fn maps_txt_window(&mut self, ctx: &egui::Context) {
        let Some(dlg) = &mut self.world.maps_txt else {
            return;
        };
        let current_file = self.current.as_ref().map(|c| c.file.clone());
        let current_name = self.current.as_ref().map(|c| c.map.header.name());
        let mut open = true;
        let mut save = false;
        egui::Window::new(crate::i18n::t("Cartes du jeu (maps.txt)")).open(&mut open).order(egui::Order::Middle).default_size([820.0, 560.0]).show(ctx, |ui| {
            let problems = maps_txt::validate(&dlg.doc);
            ui.horizontal(|ui| {
                if ui.button(crate::i18n::t("Ajouter une entrée")).clicked() {
                    let (file, name) = current_file.clone().map_or(("newmap".to_string(), "Nouvelle carte".to_string()), |f| (f, current_name.clone().unwrap_or_default().trim_end_matches(".MAP").to_string()));
                    let n = maps_txt::add_entry(&mut dlg.doc, &file, &name);
                    dlg.selected = dlg.doc.sections.iter().position(|s| maps_txt::section_number(s) == Some(n));
                    dlg.dirty = true;
                }
                if ui.button(crate::i18n::t("Supprimer la dernière")).clicked() && maps_txt::remove_last(&mut dlg.doc).is_some() {
                    dlg.selected = None;
                    dlg.dirty = true;
                }
                if ui.add_enabled(dlg.dirty && problems.is_empty(), egui::Button::new(crate::i18n::t("Enregistrer"))).clicked() {
                    save = true;
                }
                if dlg.dirty {
                    ui.colored_label(Color32::from_rgb(255, 190, 80), crate::i18n::t("● modifié"));
                }
            });
            for p in &problems {
                ui.colored_label(Color32::LIGHT_RED, problem_text(p));
            }
            ui.separator();
            ui.horizontal_top(|ui| {
                egui::ScrollArea::vertical().id_salt("maps_txt_list").max_width(300.0).auto_shrink([false, false]).show(ui, |ui| {
                    // Une colonne : le conteneur est horizontal, les entrées doivent donc s'empiler dans un sous-conteneur vertical.
                    ui.vertical(|ui| {
                        ui.set_width(280.0);
                        for (i, s) in dlg.doc.sections.iter().enumerate() {
                            let Some(n) = maps_txt::section_number(s) else { continue };
                            let label = format!("{n:03}  {}  —  {}", s.get("map_name").unwrap_or_default(), s.get("lookup_name").unwrap_or_default());
                            if ui.add(egui::Button::selectable(dlg.selected == Some(i), label).truncate()).clicked() {
                                dlg.selected = Some(i);
                            }
                        }
                    });
                });
                ui.separator();
                ui.vertical(|ui| {
                    let Some(i) = dlg.selected.filter(|&i| i < dlg.doc.sections.len()) else {
                        ui.label(crate::i18n::t("Sélectionnez une carte."));
                        return;
                    };
                    let section = &mut dlg.doc.sections[i];
                    ui.heading(section.name.clone());
                    egui::Grid::new("maps_txt_fields").num_columns(2).show(ui, |ui| {
                        for key in MAP_KEYS {
                            let mut value = section.get(key).unwrap_or_default();
                            ui.label(key);
                            if ui.add(egui::TextEdit::singleline(&mut value).desired_width(360.0)).changed() {
                                section.set(key, &value);
                                dlg.dirty = true;
                            }
                            ui.end_row();
                        }
                    });
                    ui.label(crate::i18n::t("Les autres lignes (points de départ aléatoires, commentaires…) sont conservées telles quelles."));
                });
            });
        });
        if save {
            self.save_maps_txt();
        }
        if !open {
            self.world.maps_txt = None;
        }
    }

    /// Pour les essais : ajoute une entrée dans la fenêtre maps.txt ouverte.
    pub(crate) fn maps_txt_add_for_test(&mut self, file: &str, name: &str) {
        if let Some(d) = &mut self.world.maps_txt {
            maps_txt::add_entry(&mut d.doc, file, name);
            d.dirty = true;
        }
    }

    /// Nombre de zones et présence du fond de la carte du monde ouverte.
    pub(crate) fn world_summary(&self) -> (usize, bool) {
        self.world
            .world
            .as_ref()
            .map_or((0, false), |w| (w.areas.len(), w.background.is_some()))
    }

    pub(crate) fn save_maps_txt(&mut self) {
        let (Some(g), Some(dlg)) = (&mut self.game, &mut self.world.maps_txt) else {
            return;
        };
        match g
            .data
            .save_data_file(MAPS_TXT, &to_latin1(&dlg.doc.to_text()))
        {
            Ok(path) => {
                dlg.dirty = false;
                g.maps = g.data.list_maps();
                self.status = format!(
                    "{} : {}",
                    crate::i18n::t("maps.txt enregistré"),
                    path.display()
                );
            }
            Err(e) => {
                self.status = format!(
                    "{} : {e}",
                    crate::i18n::t("Échec de l'enregistrement de maps.txt")
                )
            }
        }
    }

    // ---------------------------------------------------------------- carte du monde

    pub(crate) fn open_world_map(&mut self, ctx: &egui::Context) {
        let Some(g) = &self.game else { return };
        let Some(city_text) = g.data.read_data_text(CITY_TXT) else {
            self.status = crate::i18n::t("data/city.txt introuvable.").into();
            return;
        };
        let city = IniDoc::parse(&city_text);
        let areas = parse_areas(&city);
        // Fond : tuiles de worldmap.txt, posées côte à côte (images de `art/intrface`).
        let worldmap_doc = g
            .data
            .read_data_text(WORLDMAP_TXT)
            .map(|t| IniDoc::parse(&t));
        let (per_row, tiles) = worldmap_doc
            .as_ref()
            .map(parse_tiles)
            .unwrap_or((4, Vec::new()));
        let terrain = worldmap_doc.as_ref().map(parse_terrain).unwrap_or_default();
        let names = g.data.list("art/intrface/intrface.lst");
        let sprites: Vec<_> = tiles
            .iter()
            .map(|&idx| {
                names
                    .get(idx.max(0) as usize)
                    .and_then(|n| g.art.sprite(&format!("art/intrface/{n}"), 0, 0))
            })
            .collect();
        let (tw, th) = sprites
            .iter()
            .flatten()
            .map(|s| (s.width as usize, s.height as usize))
            .next()
            .unwrap_or((350, 300));
        let rows = tiles.len().div_ceil(per_row.max(1));
        let (w, h) = (tw * per_row.max(1), th * rows.max(1));
        let (background, background_size) =
            if sprites.iter().any(Option::is_some) && w * h <= 8192 * 8192 {
                let mut rgba = vec![0u8; w * h * 4];
                for (i, s) in sprites.iter().enumerate() {
                    let Some(s) = s else { continue };
                    let (ox, oy) = ((i % per_row.max(1)) * tw, (i / per_row.max(1)) * th);
                    for y in 0..(s.height as usize).min(th) {
                        let dst = ((oy + y) * w + ox) * 4;
                        let src = y * s.width as usize * 4;
                        let n = (s.width as usize).min(tw) * 4;
                        rgba[dst..dst + n].copy_from_slice(&s.rgba[src..src + n]);
                    }
                }
                (
                    Some(ctx.load_texture(
                        "worldmap_bg",
                        egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba),
                        TextureOptions::LINEAR,
                    )),
                    Vec2::new(w as f32, h as f32),
                )
            } else {
                (None, Vec2::new(1400.0, 1200.0))
            };
        self.world.world = Some(WorldDialog {
            city,
            areas,
            selected: None,
            background,
            background_size,
            zoom: 0.6,
            dirty: false,
            pan: Vec2::ZERO,
            filter: String::new(),
            focus: None,
            dragging: None,
            open_area: None,
            terrain,
            rolled: HashMap::new(),
            seed: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(1, |d| d.as_nanos() as u64)
                | 1,
        });
        if let Some(dlg) = self.world.world.as_mut() {
            // Les lieux aléatoires sont posés au hasard dès l'ouverture.
            reroll(dlg, None);
        }
    }

    /// Capture d'écran (`FO_SHOT_WORLDAREA=<n>`) : sélectionne la zone n et centre la vue dessus.
    pub(crate) fn shot_select_area(&mut self, n: usize) {
        if let Some(d) = self.world.world.as_mut() {
            d.selected = Some(n);
            d.focus = Some(n);
            d.zoom = 1.0;
        }
    }

    /// Onglet « Carte du monde » : fond, lieux de city.txt (molette : zoom, bouton du milieu : 100 %), liste des lieux à gauche,
    /// fiche du lieu à droite avec ses entrées (chacune renvoie vers une carte locale).
    pub(crate) fn world_tab(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.world.world.is_none() {
            self.open_world_map(&ctx);
        }
        let maps = self
            .game
            .as_ref()
            .map(|g| world_map_choices(&g.data.map_defs(), &g.maps))
            .unwrap_or_default();
        let map_labels: HashMap<String, String> = self
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
        let mut open_map: Option<String> = None;
        let mut open_area = None;
        let mut save = false;
        let Some(dlg) = &mut self.world.world else {
            ui.label(crate::i18n::t("Aucun jeu ouvert."));
            return;
        };

        egui::Panel::left("world_areas")
            .default_size(230.0)
            .show(ui, |ui| {
                ui.heading(crate::i18n::t("Lieux"));
                ui.add(
                    egui::TextEdit::singleline(&mut dlg.filter)
                        .hint_text(crate::i18n::t("Rechercher…"))
                        .desired_width(f32::INFINITY),
                );
                ui.separator();
                let needle = dlg.filter.to_lowercase();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (i, a) in dlg.areas.iter().enumerate() {
                            if !needle.is_empty() && !a.name.to_lowercase().contains(&needle) {
                                continue;
                            }
                            let response = ui.selectable_label(dlg.selected == Some(i), &a.name);
                            if response.clicked() {
                                dlg.selected = Some(i);
                                dlg.focus = Some(i);
                            }
                            if response.double_clicked() {
                                open_area = Some(i);
                            }
                        }
                    });
            });

        egui::Panel::right("world_side").resizable(true).default_size(420.0).min_size(280.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.add_enabled(dlg.dirty, egui::Button::new(crate::i18n::t("Enregistrer city.txt"))).clicked() {
                    save = true;
                }
                if dlg.dirty {
                    ui.colored_label(Color32::from_rgb(255, 190, 80), "●");
                }
            });
            if ui.add_enabled(!dlg.terrain.is_empty(), egui::Button::new(crate::i18n::t("Relancer tous les lieux aléatoires"))).on_hover_text(crate::i18n::t("Les lieux dont le jeu choisit la position (rencontres spéciales, caravanes…) sont posés au hasard sur le bon terrain ; city.txt n'est pas modifié.")).clicked() {
                reroll(dlg, None);
            }
            ui.separator();
            let Some(i) = dlg.selected.filter(|&i| i < dlg.areas.len()) else {
                ui.label(crate::i18n::t("Cliquez sur un marqueur."));
                return;
            };
            let mut changed = false;
            let areas_snapshot = dlg.areas.clone();
            let rolled_snapshot = dlg.rolled.clone();
            let mut reroll_one = false;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let a = &mut dlg.areas[i];
                ui.heading(format!("{} ({})", a.name, a.section));
                let targets = area_maps(a, &maps);
                if ui.add_enabled(!targets.is_empty(), egui::Button::new(crate::i18n::t("Ouvrir la carte locale"))).clicked() {
                    open_area = Some(i);
                }
                if targets.is_empty() {
                    ui.weak(crate::i18n::t("Aucune carte locale associée à ce lieu."));
                }
                if is_random_area(&areas_snapshot, i) {
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(Color32::from_rgb(255, 200, 80), crate::i18n::t("Lieu aléatoire"));
                        let rule = match placement_rule(a) {
                            fo_core::worldmap::PlacementRule::Desert => "désert",
                            fo_core::worldmap::PlacementRule::Mountain => "montagne",
                            fo_core::worldmap::PlacementRule::Coast => "côte",
                            fo_core::worldmap::PlacementRule::Land => "terre",
                        };
                        ui.weak(format!("{} : {}", crate::i18n::t("terrain"), crate::i18n::t(rule)));
                    });
                    if let Some(p) = rolled_snapshot.get(&i) {
                        ui.weak(format!("{} : {}, {}", crate::i18n::t("Position tirée"), p.0, p.1));
                    }
                    if ui.button(crate::i18n::t("Relancer l'emplacement")).clicked() {
                        reroll_one = true;
                    }
                }
                egui::Grid::new("area_fields").num_columns(2).show(ui, |ui| {
                    ui.label(crate::i18n::t("Nom"));
                    changed |= ui.add(egui::TextEdit::singleline(&mut a.name).desired_width(200.0)).changed();
                    ui.end_row();
                    ui.label(crate::i18n::t("Position"));
                    ui.vertical(|ui| {
                        changed |= ui.add(crate::stepper::Stepper::new(&mut a.world_pos.0).range(0..=5000).prefix("x ")).changed();
                        changed |= ui.add(crate::stepper::Stepper::new(&mut a.world_pos.1).range(0..=5000).prefix("y ")).changed();
                    });
                    ui.end_row();
                    ui.label(crate::i18n::t("Visible au départ"));
                    changed |= ui.checkbox(&mut a.start_on, "").changed();
                    ui.end_row();
                    ui.label(crate::i18n::t("Taille"));
                    egui::ComboBox::from_id_salt("area_size").selected_text(a.size.clone()).show_ui(ui, |ui| {
                        for s in AREA_SIZES {
                            changed |= ui.selectable_value(&mut a.size, s.to_string(), s).changed();
                        }
                    });
                    ui.end_row();
                });
                ui.separator();
                ui.heading(format!("{} ({})", crate::i18n::t("Entrées"), a.entrances.len()));
                let mut remove = None;
                for (n, e) in a.entrances.iter_mut().enumerate() {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            changed |= ui.checkbox(&mut e.on, e.key.clone()).changed();
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("✕").on_hover_text(crate::i18n::t("Supprimer")).clicked() {
                                    remove = Some(n);
                                }
                            });
                        });
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt(("entrance_map", n)).selected_text(e.map.clone()).width(230.0).show_ui(ui, |ui| {
                                for (name, _) in &maps {
                                    changed |= ui.selectable_value(&mut e.map, name.clone(), name).changed();
                                }
                            });
                            let target = resolve_entrance_map(&e.map, &maps).map(|(_, f)| f.clone());
                            if ui.add_enabled(target.is_some(), egui::Button::new(crate::i18n::t("Ouvrir"))).on_hover_text(crate::i18n::t("Ouvrir la carte locale")).clicked() {
                                open_map = target;
                            }
                        });
                        ui.horizontal(|ui| {
                            changed |= ui.add(crate::stepper::Stepper::new(&mut e.x).prefix("x ")).changed();
                            changed |= ui.add(crate::stepper::Stepper::new(&mut e.y).prefix("y ")).changed();
                        });
                        ui.horizontal(|ui| {
                            changed |= ui.add(crate::stepper::Stepper::new(&mut e.elevation).range(-1..=2).prefix(format!("{} ", crate::i18n::t("é")))).changed();
                            changed |= ui.add(crate::stepper::Stepper::new(&mut e.tile).range(-1..=39_999).prefix(format!("{} ", crate::i18n::t("hex"))).speed(20.0)).changed();
                        });
                    });
                }
                if let Some(n) = remove {
                    a.entrances.remove(n);
                    changed = true;
                }
                if ui.button(crate::i18n::t("+ entrée")).clicked() {
                    let next = a.entrances.iter().filter_map(|e| e.key.trim_start_matches("entrance_").parse::<u32>().ok()).max().map_or(0, |m| m + 1);
                    a.entrances.push(Entrance { key: format!("entrance_{next}"), on: false, x: -1, y: -1, map: maps.first().map(|m| m.0.clone()).unwrap_or_default(), elevation: -1, tile: -1, orientation: 0 });
                    changed = true;
                }
            });
            if reroll_one {
                reroll(dlg, Some(i));
            }
            if changed {
                dlg.dirty = true;
            }
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
            let view = resp.rect;
            painter.rect_filled(view, 0.0, Color32::from_rgb(20, 22, 26));
            // Centrer sur un lieu choisi dans la liste.
            if let Some(i) = dlg.focus.take()
                && let Some(a) = dlg.areas.get(i)
            {
                let shown = shown_pos(&dlg.areas, &dlg.rolled, i);
                let _ = a;
                dlg.pan = view.size() / 2.0 - Vec2::new(shown.0 as f32, shown.1 as f32) * dlg.zoom;
            }
            // Molette : zoom sous le curseur ; bouton du milieu : retour à 100 %.
            let zoom_to = |dlg: &mut WorldDialog, anchor: Pos2, new_zoom: f32| {
                let new_zoom = new_zoom.clamp(0.1, 4.0);
                let world = (anchor - view.min - dlg.pan) / dlg.zoom;
                dlg.zoom = new_zoom;
                dlg.pan = anchor - view.min - world * new_zoom;
            };
            if let Some(p) = resp.hover_pos() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    zoom_to(dlg, p, dlg.zoom * (scroll * 0.0025).exp());
                }
                if resp.clicked_by(egui::PointerButton::Middle) {
                    zoom_to(dlg, p, 1.0);
                }
            }
            let origin = view.min + dlg.pan;
            let to_screen =
                |p: (i32, i32), zoom: f32| origin + Vec2::new(p.0 as f32, p.1 as f32) * zoom;
            let bg_rect = Rect::from_min_size(origin, dlg.background_size * dlg.zoom);
            if let Some(bg) = &dlg.background {
                painter.image(
                    bg.id(),
                    bg_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            let zoom = dlg.zoom;
            let pointer = resp.interact_pointer_pos();
            if resp.drag_started_by(egui::PointerButton::Primary)
                || resp.clicked_by(egui::PointerButton::Primary)
            {
                let press = ui.input(|i| i.pointer.press_origin()).or(pointer);
                let picked = press.and_then(|p| {
                    dlg.areas
                        .iter()
                        .enumerate()
                        .map(|(i, _)| {
                            (
                                i,
                                (to_screen(shown_pos(&dlg.areas, &dlg.rolled, i), zoom) - p)
                                    .length(),
                            )
                        })
                        .filter(|(_, d)| *d < 18.0)
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|(i, _)| i)
                });
                if picked.is_some() {
                    dlg.selected = picked;
                }
                dlg.dragging = picked;
                if resp.double_clicked()
                    && let Some(i) = picked
                {
                    open_area = Some(i);
                }
            }
            if resp.dragged_by(egui::PointerButton::Primary)
                && let (Some(i), Some(p)) = (dlg.dragging, pointer)
            {
                let w = ((p - origin) / zoom).to_pos2();
                let new = (w.x.round().max(0.0) as i32, w.y.round().max(0.0) as i32);
                if let Some(slot) = dlg.rolled.get_mut(&i) {
                    // Un lieu aléatoire se déplace dans l'aperçu seulement : sa position d'origine reste celle du fichier.
                    *slot = new;
                } else {
                    dlg.areas[i].world_pos = new;
                    dlg.dirty = true;
                }
            }
            let pan = resp.dragged_by(egui::PointerButton::Middle)
                || resp.dragged_by(egui::PointerButton::Secondary)
                || (resp.dragged_by(egui::PointerButton::Primary) && dlg.dragging.is_none());
            if pan {
                dlg.pan += resp.drag_delta();
            }
            if resp.drag_stopped() {
                dlg.dragging = None;
            }
            for (i, a) in dlg.areas.iter().enumerate() {
                let c = to_screen(shown_pos(&dlg.areas, &dlg.rolled, i), zoom);
                if !view.expand(40.0).contains(c) {
                    continue;
                }
                let r = match a.size.as_str() {
                    "Small" => 5.0,
                    "Large" => 11.0,
                    _ => 8.0,
                } * zoom.max(0.6);
                let selected = dlg.selected == Some(i);
                let color = if selected {
                    Color32::from_rgb(255, 210, 60)
                } else if a.start_on {
                    Color32::from_rgb(120, 220, 140)
                } else {
                    Color32::from_rgb(200, 120, 120)
                };
                painter.circle_stroke(c, r, Stroke::new(if selected { 3.0 } else { 1.5 }, color));
                let font = egui::FontId::proportional(12.0);
                let galley = painter.layout_no_wrap(a.name.clone(), font, Color32::WHITE);
                let pos = c + Vec2::new(r + 4.0, -6.0);
                painter.rect_filled(
                    Rect::from_min_size(
                        pos - Vec2::new(2.0, 1.0),
                        galley.size() + Vec2::new(4.0, 2.0),
                    ),
                    3.0,
                    Color32::from_black_alpha(150),
                );
                painter.galley(pos, galley, Color32::WHITE);
            }
            painter.text(
                view.left_bottom() + Vec2::new(10.0, -10.0),
                egui::Align2::LEFT_BOTTOM,
                format!(
                    "{:.0} %  ·  {}",
                    zoom * 100.0,
                    crate::i18n::t(
                        "molette : zoom · double-clic : ouvrir · glisser un marqueur : déplacer"
                    )
                ),
                egui::FontId::proportional(12.0),
                Color32::from_gray(190),
            );
        });

        if let Some(i) = open_area
            && let Some(area) = dlg.areas.get(i)
        {
            let targets = area_maps(area, &maps);
            if targets.len() == 1 {
                open_map = Some(targets[0].1.clone());
            } else {
                dlg.open_area = Some(i);
            }
        }
        if let Some(i) = dlg.open_area
            && let Some(area) = dlg.areas.get(i)
        {
            let mut picker_open = true;
            egui::Window::new(crate::i18n::t("Ouvrir une carte du lieu"))
                .id(egui::Id::unique("world_map_picker"))
                .open(&mut picker_open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .default_width(440.0)
                .min_width(440.0)
                .show(&ctx, |ui| {
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new(&area.name).size(20.0).strong());
                    let targets = area_maps(area, &maps);
                    if targets.is_empty() {
                        ui.label(
                            egui::RichText::new(crate::i18n::t(
                                "Aucune carte locale associée à ce lieu.",
                            ))
                            .weak(),
                        );
                    }
                    ui.add_space(6.0);
                    // Au moins cinq cartes visibles sans défiler (5 lignes de 36 px et 4 écarts de 5 px).
                    egui::ScrollArea::vertical()
                        .min_scrolled_height(210.0)
                        .max_height(400.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 5.0;
                            for (label, file) in targets {
                                let label = map_labels.get(&file).unwrap_or(&label);
                                // Une ligne par carte : le nom à gauche, le fichier en discret à droite.
                                let (rect, resp) = ui.allocate_exact_size(
                                    Vec2::new(ui.available_width(), 36.0),
                                    egui::Sense::click(),
                                );
                                let visuals = ui.style().interact(&resp).clone();
                                let fill = if resp.hovered() {
                                    ui.visuals().widgets.hovered.bg_fill
                                } else {
                                    ui.visuals().widgets.inactive.weak_bg_fill
                                };
                                ui.painter().rect_filled(rect, 7.0, fill);
                                let name =
                                    label.split_once(" — ").map_or(label.as_str(), |(_, n)| n);
                                ui.painter().text(
                                    rect.left_center() + Vec2::new(14.0, 0.0),
                                    egui::Align2::LEFT_CENTER,
                                    name,
                                    egui::FontId::proportional(15.0),
                                    visuals.fg_stroke.color,
                                );
                                ui.painter().text(
                                    rect.right_center() - Vec2::new(14.0, 0.0),
                                    egui::Align2::RIGHT_CENTER,
                                    format!("{file}.map"),
                                    egui::FontId::proportional(12.0),
                                    ui.visuals().weak_text_color(),
                                );
                                if resp
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    open_map = Some(file);
                                }
                            }
                        });
                });
            if !picker_open || open_map.is_some() {
                dlg.open_area = None;
            }
        }

        if save {
            self.save_city_txt();
        }
        if let Some(file) = open_map {
            self.mode = crate::app::Mode::Map;
            self.request_load(&file, &ctx);
        }
    }

    fn save_city_txt(&mut self) {
        let (Some(g), Some(dlg)) = (&self.game, &mut self.world.world) else {
            return;
        };
        apply_areas(&mut dlg.city, &dlg.areas);
        match g
            .data
            .save_data_file(CITY_TXT, &to_latin1(&dlg.city.to_text()))
        {
            Ok(path) => {
                dlg.dirty = false;
                self.status = format!(
                    "{} : {}",
                    crate::i18n::t("city.txt enregistré"),
                    path.display()
                );
            }
            Err(e) => {
                self.status = format!(
                    "{} : {e}",
                    crate::i18n::t("Échec de l'enregistrement de city.txt")
                )
            }
        }
    }

    pub(crate) fn world_windows(&mut self, ctx: &egui::Context) {
        self.maps_txt_window(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin1_encoding_keeps_accents_and_replaces_the_rest() {
        assert_eq!(to_latin1("é"), vec![0xE9]);
        assert_eq!(to_latin1("a€"), vec![b'a', b'?']);
    }

    #[test]
    fn world_entrances_resolve_lookup_names_and_map_files() {
        let maps = vec![
            ("Arroyo Village".into(), "arvillag".into()),
            ("Temple".into(), "artemple".into()),
        ];
        for name in [
            "Arroyo Village",
            "ARROYO VILLAGE",
            "arvillag",
            "ARVILLAG.MAP",
            "Arvillag.Map",
        ] {
            assert_eq!(
                resolve_entrance_map(name, &maps).map(|m| m.1.as_str()),
                Some("arvillag")
            );
        }
        assert!(resolve_entrance_map("unknown", &maps).is_none());
    }

    #[test]
    fn world_navigation_uses_lookup_names_instead_of_translated_titles() {
        let defs = vec![fo_core::formats::MapDef {
            number: 0,
            file: "arvillag".into(),
            display: Some("ArroyoVillage".into()),
        }];
        let maps = vec![
            fo_core::game::MapEntry {
                file: "arvillag".into(),
                number: Some(0),
                display: Some("Arroyo — Village".into()),
                group: None,
                group_name: None,
                place: None,
            },
            fo_core::game::MapEntry {
                file: "custom".into(),
                number: None,
                display: None,
                group: None,
                group_name: None,
                place: None,
            },
        ];
        let choices = world_map_choices(&defs, &maps);
        assert_eq!(
            resolve_entrance_map("ArroyoVillage", &choices).unwrap().1,
            "arvillag"
        );
        assert_eq!(
            resolve_entrance_map("custom", &choices).unwrap().1,
            "custom"
        );
        assert!(
            resolve_entrance_map("Arroyo — Village", &choices).is_none(),
            "le titre traduit ne doit pas être enregistré dans city.txt"
        );
    }

    #[test]
    fn a_place_lists_each_map_once_including_hidden_entrances() {
        let maps = vec![
            ("Village".into(), "arvillag".into()),
            ("Temple".into(), "artemple".into()),
        ];
        let entrances = ["Village", "ARVILLAG", "Temple", "missing"]
            .iter()
            .enumerate()
            .map(|(i, name)| Entrance {
                key: format!("entrance_{i}"),
                on: false,
                x: -1,
                y: -1,
                map: (*name).into(),
                elevation: 0,
                tile: 1,
                orientation: 0,
            })
            .collect();
        let area = Area {
            section: "Area 00".into(),
            name: "Arroyo".into(),
            world_pos: (100, 100),
            start_on: true,
            size: "Small".into(),
            entrances,
        };
        assert_eq!(area_maps(&area, &maps), maps);
    }
}
