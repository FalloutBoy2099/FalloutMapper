//! Visualiseur de scripts : liste des scripts de la carte et du jeu, procédures, pseudo-source / listing.

use eframe::egui::{self, Color32, RichText};
use fo_core::GameData;
use fo_core::map::{Map, SCRIPT_TYPE_NAMES};
use fo_script::{GameMessages, Program};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct MapScriptRef {
    pub label: String,
    pub file: String,
    /// Nombre d'objets de la carte qui l'utilisent.
    pub uses: usize,
}

struct OpenScript {
    file: String,
    program: Arc<Program>,
    selected: Option<usize>,
    text: String,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum ListTab {
    #[default]
    Map,
    Game,
}

#[derive(Default)]
pub struct ScriptsState {
    source_editor: crate::script_editor::SourceEditor,
    source_mode: bool,
    game: Option<Arc<GameData>>,
    filter: String,
    all: Vec<String>,
    map_scripts: Vec<MapScriptRef>,
    open: Option<OpenScript>,
    show_asm: bool,
    error: Option<String>,
    /// Onglet de la liste de gauche : scripts de la carte ou du jeu, chacun avec sa recherche.
    list_tab: ListTab,
    map_filter: String,
    /// Taille de la police du code (Ctrl + molette ou boutons A−/A+) ; 0 : valeur par défaut.
    font_size: f32,
    /// Procédure choisie : n'afficher qu'elle (isoler) ou tout le script avec défilement jusqu'à elle (aller à).
    show_all: bool,
    /// Première ligne de chaque procédure dans le texte affiché.
    proc_lines: Vec<usize>,
    goto: Option<usize>,
}

/// Libellé d'un script de la carte (« Spatial hex 123, Objet ») dans la langue courante.
fn label_text(label: &str) -> String {
    label
        .split(", ")
        .map(|part| match part.split_once(" hex ") {
            Some((kind, hex)) => {
                format!("{} {} {hex}", crate::i18n::tr(kind), crate::i18n::t("hex"))
            }
            None => crate::i18n::tr(part),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

impl ScriptsState {
    pub(crate) fn show_source_editor(&mut self) {
        self.source_mode = true;
    }
    pub(crate) fn editing_source(&self) -> bool {
        self.source_mode
    }
    fn font(&self) -> f32 {
        if self.font_size < 8.0 {
            13.0
        } else {
            self.font_size
        }
    }

    pub fn set_game(&mut self, game: Arc<GameData>) {
        self.all = game.list("scripts/scripts.lst").iter().cloned().collect();
        self.game = Some(game);
        self.map_scripts.clear();
        self.open = None;
        self.error = None;
    }

    /// Recense les scripts utilisés par la carte (script de carte, scripts d'objets, spatiaux, minuteries…).
    pub fn set_map(&mut self, game: &GameData, map: &Map) {
        self.map_scripts.clear();
        if let Some(file) = map.map_script_index().and_then(|i| game.script_file(i)) {
            self.map_scripts.push(MapScriptRef {
                label: "Carte".into(),
                file,
                uses: 1,
            });
        }
        // Un même programme sert souvent à de nombreux objets : on regroupe par fichier.
        let mut groups: Vec<(String, Vec<String>, usize)> = Vec::new();
        for (name, list) in SCRIPT_TYPE_NAMES.iter().zip(&map.scripts) {
            for entry in list.iter() {
                let Some(file) = game.script_file(entry.list_index().max(0) as usize) else {
                    continue;
                };
                let uses = map
                    .objects
                    .iter()
                    .flatten()
                    .filter(|o| o.script_id() as u32 == entry.pid)
                    .count();
                let label = match entry.spatial() {
                    Some(t) => format!("{name} hex {}", t.tile),
                    None => (*name).to_string(),
                };
                match groups.iter_mut().find(|g| g.0.eq_ignore_ascii_case(&file)) {
                    Some(g) => {
                        if !g.1.contains(&label) && g.1.len() < 3 {
                            g.1.push(label);
                        }
                        g.2 += uses.max(1);
                    }
                    None => groups.push((file, vec![label], uses.max(1))),
                }
            }
        }
        groups.sort_by_key(|g| g.0.to_lowercase());
        self.map_scripts
            .extend(groups.into_iter().map(|(file, labels, uses)| MapScriptRef {
                label: labels.join(", "),
                file,
                uses,
            }));
    }

    pub fn open_file(&mut self, game: &GameData, file: &str) {
        let bytes = game.read(&format!("scripts/{file}"));
        match bytes
            .map_err(|e| e.to_string())
            .and_then(|b| Program::parse(&b).map_err(|e| e.to_string()))
        {
            Ok(program) => {
                self.error = None;
                self.open = Some(OpenScript {
                    file: file.to_string(),
                    program: Arc::new(program),
                    selected: None,
                    text: String::new(),
                });
                self.refresh_text();
            }
            Err(e) => {
                self.error = Some(format!("{file} : {e}"));
                self.open = None;
            }
        }
    }

    fn refresh_text(&mut self) {
        let asm = self.show_asm;
        let show_all = self.show_all;
        let Some(o) = &mut self.open else { return };
        let p = &o.program;
        let indexes: Vec<usize> = match o.selected {
            Some(i) if !show_all => vec![i],
            _ => (0..p.procedures.len()).collect(),
        };
        let own = o.file.clone();
        let msgs = self.game.as_deref().map(|g| GameMessages::new(g, own));
        let mut text = String::new();
        let mut lines = vec![0usize; p.procedures.len()];
        for (n, i) in indexes.into_iter().enumerate() {
            if n > 0 {
                text.push('\n');
            }
            lines[i] = text.matches('\n').count();
            let chunk = match (&msgs, asm) {
                (_, true) => format!("; {}\n{}", p.procedures[i].name, p.disassemble(i)),
                (Some(m), false) => p.procedure_source_with(i, m),
                (None, false) => p.procedure_source(i),
            };
            text.push_str(&chunk);
        }
        o.text = text;
        self.proc_lines = lines;
    }

    /// Panneau de gauche : onglets « Carte » (scripts de la carte ouverte) et « Jeu » (tous les scripts), chacun avec sa recherche.
    pub fn list_ui(&mut self, ui: &mut egui::Ui, game: &GameData) {
        let mut to_open: Option<String> = None;
        let current = self.open.as_ref().map(|o| o.file.to_ascii_lowercase());
        ui.horizontal(|ui| {
            let w = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
            let map_label = format!("{} ({})", crate::i18n::t("Carte"), self.map_scripts.len());
            let game_label = format!("{} ({})", crate::i18n::t("Jeu"), self.all.len());
            if ui
                .add_sized(
                    [w, 28.0],
                    egui::Button::new(map_label).selected(self.list_tab == ListTab::Map),
                )
                .clicked()
            {
                self.list_tab = ListTab::Map;
            }
            if ui
                .add_sized(
                    [w, 28.0],
                    egui::Button::new(game_label).selected(self.list_tab == ListTab::Game),
                )
                .clicked()
            {
                self.list_tab = ListTab::Game;
            }
        });
        let tab = self.list_tab;
        let filter = if tab == ListTab::Map {
            &mut self.map_filter
        } else {
            &mut self.filter
        };
        ui.add(
            egui::TextEdit::singleline(filter)
                .hint_text(crate::i18n::t("Rechercher…"))
                .desired_width(f32::INFINITY),
        );
        let needle = filter.to_lowercase();
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt(("scripts_list", tab as u8))
            .auto_shrink([false, false])
            .show(ui, |ui| match tab {
                ListTab::Map => {
                    if self.map_scripts.is_empty() {
                        ui.label(crate::i18n::t("Aucun script."));
                    }
                    for s in self.map_scripts.iter().filter(|s| {
                        needle.is_empty()
                            || s.file.to_lowercase().contains(&needle)
                            || s.label.to_lowercase().contains(&needle)
                    }) {
                        let sel = current.as_deref() == Some(&s.file.to_ascii_lowercase());
                        let shown = label_text(&s.label);
                        let text = if s.uses > 1 {
                            format!("{}  ({} ×{})", s.file, shown, s.uses)
                        } else {
                            format!("{}  ({})", s.file, shown)
                        };
                        if ui.selectable_label(sel, text).clicked() {
                            to_open = Some(s.file.clone());
                        }
                    }
                }
                ListTab::Game => {
                    for file in self
                        .all
                        .iter()
                        .filter(|f| needle.is_empty() || f.to_lowercase().contains(&needle))
                    {
                        let sel = current.as_deref() == Some(&file.to_ascii_lowercase());
                        if ui.selectable_label(sel, file).clicked() {
                            to_open = Some(file.clone());
                        }
                    }
                }
            });
        if let Some(f) = to_open {
            self.open_file(game, &f);
        }
    }

    /// Panneau de droite : informations et procédures du script ouvert.
    pub fn procedures_ui(&mut self, ui: &mut egui::Ui) {
        let Some(o) = &mut self.open else {
            ui.label(crate::i18n::t("Aucun script ouvert."));
            return;
        };
        ui.heading(&o.file);
        ui.label(format!(
            "{} {} · {} {} · {} {} · {} {}",
            o.program.procedures.len(),
            crate::i18n::t("procédures"),
            o.program.names.len(),
            crate::i18n::t("noms"),
            o.program.strings.len(),
            crate::i18n::t("chaînes"),
            o.program.file_size,
            crate::i18n::t("octets")
        ));
        ui.separator();
        let mut changed = false;
        let before = self.show_all;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.show_all, false, crate::i18n::t("Isoler"))
                .on_hover_text(crate::i18n::t("N'afficher que la procédure choisie"));
            ui.selectable_value(&mut self.show_all, true, crate::i18n::t("Aller à"))
                .on_hover_text(crate::i18n::t(
                    "Afficher tout le script et aller à la procédure",
                ));
        });
        changed |= before != self.show_all;
        if ui
            .selectable_label(o.selected.is_none(), crate::i18n::t("Tout le script"))
            .clicked()
        {
            o.selected = None;
            changed = true;
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, p) in o.program.procedures.iter().enumerate() {
                    let mut text = RichText::new(format!("{} ({})", p.name, p.arg_count));
                    if !p.is_implemented() {
                        text = text.color(Color32::GRAY);
                    }
                    let resp = ui.selectable_label(o.selected == Some(i), text);
                    let flags = p.flag_labels().join(", ");
                    let resp = if flags.is_empty() {
                        resp
                    } else {
                        resp.on_hover_text(flags)
                    };
                    if resp.clicked() {
                        o.selected = Some(i);
                        self.goto = Some(i);
                        changed = true;
                    }
                }
            });
        if changed {
            self.refresh_text();
        }
    }

    /// Zone centrale : le code, coloré, avec zoom de la police.
    pub fn code_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(
                &mut self.source_mode,
                false,
                crate::i18n::t("Script compilé"),
            );
            ui.selectable_value(
                &mut self.source_mode,
                true,
                crate::i18n::t("Éditer la source SSL"),
            );
        });
        if self.source_mode {
            if let Some(game) = self.game.clone() {
                let name = self.open.as_ref().map(|o| o.file.clone());
                self.source_editor.ui(ui, game.clone(), name.as_deref());
                if let Some(file) = self.source_editor.completed.take() {
                    self.all = game.list("scripts/scripts.lst").iter().cloned().collect();
                    self.open_file(&game, &file);
                }
            }
            return;
        }
        let before = self.show_asm;
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.show_asm, false, crate::i18n::t("Pseudo-source"));
            ui.radio_value(
                &mut self.show_asm,
                true,
                crate::i18n::t("Listing des opcodes"),
            );
            ui.separator();
            let size = self.font();
            if ui
                .button("A−")
                .on_hover_text(crate::i18n::t("Réduire la police (Ctrl + molette)"))
                .clicked()
            {
                self.font_size = (size - 1.0).max(8.0);
            }
            ui.label(format!("{:.0}", self.font()));
            if ui
                .button("A+")
                .on_hover_text(crate::i18n::t("Agrandir la police (Ctrl + molette)"))
                .clicked()
            {
                self.font_size = (size + 1.0).min(40.0);
            }
        });
        if before != self.show_asm {
            self.refresh_text();
        }
        ui.separator();
        if let Some(e) = &self.error {
            ui.colored_label(Color32::LIGHT_RED, e);
        }
        if self.open.is_none() {
            if self.error.is_none() {
                ui.label(crate::i18n::t("Choisissez un script dans la liste."));
            }
            return;
        }
        let zoom = ui.input(|i| {
            if i.modifiers.command {
                i.smooth_scroll_delta.y
            } else {
                0.0
            }
        });
        if zoom != 0.0 && ui.rect_contains_pointer(ui.available_rect_before_wrap()) {
            self.font_size = (self.font() + zoom.signum()).clamp(8.0, 40.0);
        }
        let font_id = egui::FontId::monospace(self.font());
        let dark = ui.visuals().dark_mode;
        let row_height = ui.fonts_mut(|f| f.row_height(&font_id));
        let goto_line = self
            .goto
            .take()
            .and_then(|i| self.proc_lines.get(i).copied())
            .filter(|_| self.show_all);
        let Some(o) = &self.open else { return };
        let mut area = egui::ScrollArea::both().auto_shrink([false, false]);
        if let Some(line) = goto_line {
            area = area.vertical_scroll_offset(line as f32 * row_height);
        }
        area.show(ui, |ui| {
            let mut text: &str = &o.text;
            let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _wrap: f32| {
                let job = crate::highlight::highlight(buf.as_str(), font_id.clone(), dark);
                ui.fonts_mut(|f| f.layout_job(job))
            };
            ui.add(
                egui::TextEdit::multiline(&mut text)
                    .font(font_id.clone())
                    .layouter(&mut layouter)
                    .desired_width(f32::INFINITY)
                    .desired_rows(40),
            );
        });
    }
}
