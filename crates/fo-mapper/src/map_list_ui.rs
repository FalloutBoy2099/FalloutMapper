use eframe::egui::{self};

use crate::app::*;

impl MapperApp {
    pub(crate) fn map_list(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let t = crate::i18n::t;
        ui.heading(t("Cartes"));
        ui.add(
            egui::TextEdit::singleline(&mut self.filter)
                .hint_text(t("Filtrer…"))
                .desired_width(f32::INFINITY),
        );
        ui.separator();
        // Barre du bas : vue (dossiers / liste), critère de tri (alphabétique / ordre du jeu) et sens.
        egui::Panel::bottom("maplist_bar")
            .show_separator_line(true)
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.selectable_value(&mut self.map_list_flat, false, t("Dossiers"))
                        .on_hover_text(t("Cartes regroupées par lieu"));
                    ui.selectable_value(&mut self.map_list_flat, true, t("Liste"))
                        .on_hover_text(t("Toutes les cartes à la suite"));
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.selectable_value(&mut self.map_sort_alpha, true, t("A–Z"))
                        .on_hover_text(t("Ordre alphabétique"));
                    ui.selectable_value(&mut self.map_sort_alpha, false, t("Ordre du jeu"))
                        .on_hover_text(t("Ordre des cartes dans le jeu (numéro de maps.txt)"));
                    let (arrow, tip) = if self.map_sort_desc {
                        ("↓", t("Décroissant"))
                    } else {
                        ("↑", t("Croissant"))
                    };
                    if ui.button(arrow).on_hover_text(tip).clicked() {
                        self.map_sort_desc = !self.map_sort_desc;
                    }
                });
                ui.add_space(2.0);
            });
        let Some(game) = &self.game else {
            ui.label(t("Aucun jeu ouvert."));
            return;
        };
        let needle = self.filter.to_lowercase();
        let mut to_load = None;
        let current = self.current.as_ref().map(|c| c.file.clone());
        let loading = self.loading.clone();
        let (flat, alpha, desc) = (self.map_list_flat, self.map_sort_alpha, self.map_sort_desc);
        let matches = |m: &fo_core::MapEntry| {
            needle.is_empty()
                || m.file.contains(&needle)
                || m.display
                    .as_deref()
                    .is_some_and(|d| d.to_lowercase().contains(&needle))
                || m.group_name
                    .as_deref()
                    .is_some_and(|d| d.to_lowercase().contains(&needle))
        };
        // Nom affiché d'une carte : « Lieu — carte » quand elle est dans un groupe, sinon son nom du jeu, sinon son fichier.
        let full_label = |m: &fo_core::MapEntry| match (&m.group_name, &m.place) {
            (Some(g), Some(p)) => format!("{g} — {p}"),
            _ => m
                .display
                .clone()
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| m.file.clone()),
        };
        // Clé de tri d'une carte : son nom en minuscules, ou son numéro dans le jeu (puis son fichier).
        let key = |m: &fo_core::MapEntry, label: &str| -> (u32, String) {
            if alpha {
                (0, label.to_lowercase())
            } else {
                (m.number.unwrap_or(u32::MAX), m.file.clone())
            }
        };
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let row = |ui: &mut egui::Ui,
                           m: &fo_core::MapEntry,
                           label: String,
                           to_load: &mut Option<String>| {
                    let selected =
                        Some(&m.file) == current.as_ref() || Some(&m.file) == loading.as_ref();
                    if ui
                        .selectable_label(selected, label)
                        .on_hover_text(&m.file)
                        .clicked()
                    {
                        *to_load = Some(m.file.clone());
                    }
                };
                if flat {
                    let mut all: Vec<(&fo_core::MapEntry, String)> = game
                        .maps
                        .iter()
                        .filter(|m| matches(m))
                        .map(|m| (m, full_label(m)))
                        .collect();
                    all.sort_by_key(|(m, l)| key(m, l));
                    if desc {
                        all.reverse();
                    }
                    for (m, label) in all {
                        row(ui, m, label, &mut to_load);
                    }
                    return;
                }
                // Vue par dossiers : un groupe par lieu, puis les cartes sans groupe ; le tri s'applique aux dossiers et à leur contenu.
                struct Item<'a> {
                    title: String,
                    group: Option<u32>,
                    members: Vec<&'a fo_core::MapEntry>,
                }
                let mut items: Vec<Item> = Vec::new();
                let mut seen: std::collections::HashMap<u32, usize> =
                    std::collections::HashMap::new();
                for m in game.maps.iter().filter(|m| matches(m)) {
                    match m.group {
                        Some(g) => {
                            let at = *seen.entry(g).or_insert_with(|| {
                                items.push(Item {
                                    title: m.group_name.clone().unwrap_or_default(),
                                    group: Some(g),
                                    members: Vec::new(),
                                });
                                items.len() - 1
                            });
                            items[at].members.push(m);
                        }
                        None => items.push(Item {
                            title: full_label(m),
                            group: None,
                            members: vec![m],
                        }),
                    }
                }
                for it in &mut items {
                    it.members
                        .sort_by_key(|m| key(m, m.place.as_deref().unwrap_or(&m.file)));
                    if desc {
                        it.members.reverse();
                    }
                }
                items.sort_by_key(|it| {
                    if alpha {
                        (0, it.title.to_lowercase())
                    } else {
                        it.members
                            .iter()
                            .map(|m| (m.number.unwrap_or(u32::MAX), m.file.clone()))
                            .min()
                            .unwrap_or((u32::MAX, String::new()))
                    }
                });
                if desc {
                    items.reverse();
                }
                for it in items {
                    match it.group {
                        Some(g) => {
                            let has_current = it.members.iter().any(|x| {
                                Some(&x.file) == current.as_ref()
                                    || Some(&x.file) == loading.as_ref()
                            });
                            let title = format!("{} ({})", it.title, it.members.len());
                            egui::CollapsingHeader::new(title)
                                .id_salt(("map_group", g))
                                .default_open(has_current || !needle.is_empty())
                                .show(ui, |ui| {
                                    for x in &it.members {
                                        row(
                                            ui,
                                            x,
                                            x.place.clone().unwrap_or_else(|| x.file.clone()),
                                            &mut to_load,
                                        );
                                    }
                                });
                        }
                        None => {
                            if let Some(m) = it.members.first() {
                                let text = m
                                    .display
                                    .clone()
                                    .filter(|d| !d.is_empty())
                                    .unwrap_or_else(|| m.file.clone());
                                row(ui, m, text, &mut to_load);
                            }
                        }
                    }
                }
            });
        if let Some(f) = to_load {
            self.request_load(&f, ctx);
        }
    }
}
