//! Propriétés d'un objet : drapeaux et lumière, script, inventaire ; scripts de la carte (script principal et spatiaux).

use crate::app::MapperApp;
use eframe::egui::{self, Color32};
use fo_core::map::{FLAG_LABELS, SCRIPT_TYPE_CRITTER, SCRIPT_TYPE_SPATIAL, word};
use fo_core::{MapObject, ObjectType, geom};
use fo_scene::ObjectRef;

/// Nombre maximal de résultats montrés par un sélecteur filtré.
const PICKER_RESULTS: usize = 10;

/// Champ de recherche suivi des premières correspondances ; renvoie l'indice (dans `names`) choisi.
pub(crate) fn picker(ui: &mut egui::Ui, filter: &mut String, names: &[String]) -> Option<usize> {
    ui.add(
        egui::TextEdit::singleline(filter)
            .hint_text(crate::i18n::t("Rechercher…"))
            .desired_width(f32::INFINITY),
    );
    let needle = filter.to_lowercase();
    if needle.is_empty() {
        return None;
    }
    let mut chosen = None;
    for (i, n) in names
        .iter()
        .enumerate()
        .filter(|(_, n)| n.to_lowercase().contains(&needle))
        .take(PICKER_RESULTS)
    {
        if ui.selectable_label(false, n).clicked() {
            chosen = Some(i);
        }
    }
    chosen
}

impl MapperApp {
    pub(crate) fn script_names(&self) -> Vec<String> {
        self.game
            .as_ref()
            .map(|g| g.data.list("scripts/scripts.lst").iter().cloned().collect())
            .unwrap_or_default()
    }

    // ---------------------------------------------------------------- drapeaux et lumière

    pub(crate) fn flags_editor(&mut self, ui: &mut egui::Ui, r: ObjectRef) {
        let Some(o) = self.object(r).cloned() else {
            return;
        };
        let (e, i) = (r.elevation as usize, r.index as usize);
        let (mut light_distance, mut light_intensity) = (
            o.words[word::LIGHT_DISTANCE],
            o.words[word::LIGHT_INTENSITY],
        );
        let mut flags = o.flags();
        let mut changed = false;
        ui.collapsing(crate::i18n::t("Drapeaux et lumière"), |ui| {
            egui::Grid::new("light").num_columns(2).show(ui, |ui| {
                ui.label(crate::i18n::t("Rayon de lumière"));
                changed |= ui
                    .add(crate::stepper::Stepper::new(&mut light_distance).range(0..=8))
                    .changed();
                ui.end_row();
                ui.label(crate::i18n::t("Intensité (0–65536)"));
                changed |= ui
                    .add(
                        crate::stepper::Stepper::new(&mut light_intensity)
                            .range(0..=65536)
                            .speed(100.0),
                    )
                    .changed();
                ui.end_row();
            });
            ui.separator();
            for &(bit, label) in FLAG_LABELS {
                let mut on = flags & bit != 0;
                if ui.checkbox(&mut on, crate::i18n::t(label)).changed() {
                    flags = if on { flags | bit } else { flags & !bit };
                    changed = true;
                }
            }
            ui.label(format!("{} : {flags:08X}", crate::i18n::t("Valeur brute")));
        });
        if changed {
            self.mutate(Some("flags"), |m, _| {
                let Some(o) = m.objects.get_mut(e).and_then(|v| v.get_mut(i)) else {
                    return false;
                };
                o.words[word::FLAGS] = flags as i32;
                o.words[word::LIGHT_DISTANCE] = light_distance;
                o.words[word::LIGHT_INTENSITY] = light_intensity;
                true
            });
        }
    }

    // ---------------------------------------------------------------- script d'un objet

    pub(crate) fn attach_script_to_selected(&mut self, program: u32) -> bool {
        let Some(r) = self.selection.primary() else {
            return false;
        };
        let (e, i) = (r.elevation as usize, r.index as usize);
        self.mutate_as("Script attaché", |m, _| {
            m.attach_object_script(e, i, program)
        })
    }

    pub(crate) fn detach_script_from_selected(&mut self) -> bool {
        let Some(r) = self.selection.primary() else {
            return false;
        };
        let (e, i) = (r.elevation as usize, r.index as usize);
        self.mutate_as("Script détaché", |m, _| m.detach_object_script(e, i))
    }

    pub(crate) fn object_script_section(&mut self, ui: &mut egui::Ui, r: ObjectRef) {
        let Some(o) = self.object(r).cloned() else {
            return;
        };
        let Some(cur) = self.current.as_ref() else {
            return;
        };
        let entry = cur.map.find_script(o.script_id());
        let program = entry.map(|e| e.list_index().max(0) as usize);
        let file = program.and_then(|p| self.game.as_ref().and_then(|g| g.data.script_file(p)));
        let title = if o.object_type() == Some(ObjectType::Critter) {
            crate::i18n::t("Script (liste « Créature »)")
        } else {
            crate::i18n::t("Script (liste « Objet »)")
        };
        let mut open = None;
        let mut detach = false;
        let mut attach = None;
        let names = self.script_names();
        ui.collapsing(title, |ui| {
            match (&file, entry) {
                (Some(f), _) => {
                    ui.horizontal(|ui| {
                        ui.label(f);
                        if ui.small_button(crate::i18n::t("Voir")).clicked() {
                            open = Some(f.clone());
                        }
                        if ui.small_button(crate::i18n::t("Détacher")).clicked() {
                            detach = true;
                        }
                    });
                }
                (None, Some(_)) => {
                    ui.colored_label(
                        Color32::from_rgb(255, 190, 80),
                        crate::i18n::t("Script attaché dont le programme est introuvable."),
                    );
                    detach |= ui.small_button(crate::i18n::t("Détacher")).clicked();
                }
                (None, None) => {
                    ui.label(crate::i18n::t("Aucun script."));
                }
            }
            ui.label(crate::i18n::t("Attacher le programme :"));
            attach = picker(ui, &mut self.edit.script_filter, &names);
        });
        if let Some(f) = open {
            self.open_script_request = Some(f);
        }
        if detach {
            self.detach_script_from_selected();
        }
        if let Some(p) = attach {
            self.attach_script_to_selected(p as u32);
            self.edit.script_filter.clear();
        }
    }

    // ---------------------------------------------------------------- inventaire

    pub(crate) fn add_inventory_item(&mut self, pid: u32, count: u32) -> bool {
        let Some(r) = self.selection.primary() else {
            return false;
        };
        let (e, i) = (r.elevation as usize, r.index as usize);
        self.mutate_as("Ajout à l'inventaire", |m, game| {
            let Some(info) = game.proto_info(pid) else {
                return false;
            };
            let id = m.max_object_id() + 1;
            let Some(item) = MapObject::for_inventory(&info, id) else {
                return false;
            };
            let Some(holder) = m.objects.get_mut(e).and_then(|v| v.get_mut(i)) else {
                return false;
            };
            holder.push_inventory(count.max(1), item);
            true
        })
    }

    pub(crate) fn inventory_editor(&mut self, ui: &mut egui::Ui, r: ObjectRef) {
        let Some(o) = self.object(r).cloned() else {
            return;
        };
        let (e, i) = (r.elevation as usize, r.index as usize);
        let rows: Vec<(String, u32)> = o
            .inventory
            .iter()
            .map(|it| {
                let name = self
                    .game
                    .as_ref()
                    .and_then(|g| g.data.proto_entry(it.object.pid()))
                    .map_or_else(
                        || format!("{:08X}", it.object.pid()),
                        |p| if p.name.is_empty() { p.file } else { p.name },
                    );
                (name, it.count)
            })
            .collect();
        let mut counts: Vec<i32> = rows.iter().map(|r| r.1 as i32).collect();
        let mut remove = None;
        let mut add = None;
        self.ensure_catalog(ObjectType::Item, ui.ctx());
        let catalog = match self.edit.catalogs.get(&ObjectType::Item) {
            Some(crate::edit_ui::CatalogState::Ready(c)) => Some(c.clone()),
            _ => None,
        };
        ui.collapsing(
            format!("{} ({})", crate::i18n::t("Inventaire"), rows.len()),
            |ui| {
                for (n, (name, _)) in rows.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add(
                            crate::stepper::Stepper::new(&mut counts[n])
                                .range(1..=99_999)
                                .prefix("× "),
                        );
                        ui.label(name);
                        if ui.small_button("✕").clicked() {
                            remove = Some(n);
                        }
                    });
                }
                ui.separator();
                ui.label(crate::i18n::t("Ajouter un objet :"));
                match &catalog {
                    None => {
                        ui.label(crate::i18n::t("Chargement du catalogue…"));
                    }
                    Some(c) => {
                        let names: Vec<String> = c
                            .iter()
                            .map(|p| {
                                if p.name.is_empty() {
                                    p.file.clone()
                                } else {
                                    format!("{} ({})", p.name, p.file)
                                }
                            })
                            .collect();
                        add = picker(ui, &mut self.edit.inventory_filter, &names).map(|k| c[k].pid);
                    }
                }
            },
        );
        let count_changes = counts
            .iter()
            .zip(&rows)
            .any(|(c, (_, old))| *c as u32 != *old);
        if count_changes || remove.is_some() {
            self.mutate(Some("inventory"), |m, _| {
                let Some(holder) = m.objects.get_mut(e).and_then(|v| v.get_mut(i)) else {
                    return false;
                };
                for (item, &c) in holder.inventory.iter_mut().zip(&counts) {
                    item.count = c.max(1) as u32;
                }
                if let Some(n) = remove {
                    holder.remove_inventory(n);
                }
                true
            });
        }
        if let Some(pid) = add {
            self.add_inventory_item(pid, 1);
            self.edit.inventory_filter.clear();
        }
    }

    // ---------------------------------------------------------------- scripts de la carte

    pub(crate) fn map_scripts_editor(&mut self, ui: &mut egui::Ui) {
        let Some(cur) = self.current.as_ref() else {
            return;
        };
        let program = cur.map.map_script_index();
        let file = program.and_then(|p| self.game.as_ref().and_then(|g| g.data.script_file(p)));
        let spatial: Vec<(u32, u32, u32, i32, usize)> = cur.map.scripts[SCRIPT_TYPE_SPATIAL]
            .iter()
            .filter_map(|e| {
                e.spatial().map(|t| {
                    (
                        e.pid,
                        t.tile,
                        t.elevation,
                        t.radius,
                        e.list_index().max(0) as usize,
                    )
                })
            })
            .collect();
        let player = cur.map.header.player_tile.max(0) as u32;
        let elevation = self.elevation as u32;
        let names = self.script_names();
        let mut open = None;
        let mut set_main: Option<Option<u32>> = None;
        let mut edits: Vec<(u32, u32, u32, u32, i32)> = Vec::new();
        let mut remove = None;
        let mut add = None;
        ui.collapsing(crate::i18n::t("Scripts de la carte"), |ui| {
            ui.label(crate::i18n::t("Script principal"));
            ui.horizontal(|ui| {
                ui.label(
                    file.clone()
                        .unwrap_or_else(|| crate::i18n::t("aucun").into()),
                );
                if let Some(f) = &file
                    && ui.small_button(crate::i18n::t("Voir")).clicked()
                {
                    open = Some(f.clone());
                }
                if file.is_some() && ui.small_button(crate::i18n::t("Retirer")).clicked() {
                    set_main = Some(None);
                }
            });
            if let Some(k) = picker(ui, &mut self.edit.map_script_filter, &names) {
                set_main = Some(Some(k as u32));
            }
            ui.separator();
            ui.label(format!(
                "{} ({})",
                crate::i18n::t("Scripts spatiaux"),
                spatial.len()
            ));
            for &(pid, tile, elev, radius, prog) in &spatial {
                let (mut tile_v, mut elev_v, mut radius_v) = (tile as i32, elev as i32, radius);
                ui.horizontal(|ui| {
                    let name = self
                        .game
                        .as_ref()
                        .and_then(|g| g.data.script_file(prog))
                        .unwrap_or_else(|| format!("#{prog}"));
                    ui.label(name);
                    let c = ui
                        .add(
                            crate::stepper::Stepper::new(&mut tile_v)
                                .range(0..=(geom::HEX_COUNT as i32 - 1))
                                .prefix(format!("{} ", crate::i18n::t("hex"))),
                        )
                        .changed()
                        | ui.add(
                            crate::stepper::Stepper::new(&mut elev_v)
                                .range(0..=2)
                                .prefix(format!("{} ", crate::i18n::t("élév."))),
                        )
                        .changed()
                        | ui.add(
                            crate::stepper::Stepper::new(&mut radius_v)
                                .range(0..=50)
                                .prefix(format!("{} ", crate::i18n::t("rayon"))),
                        )
                        .changed();
                    if c {
                        edits.push((pid, prog as u32, tile_v as u32, elev_v as u32, radius_v));
                    }
                    if ui.small_button("✕").clicked() {
                        remove = Some(pid);
                    }
                });
            }
            ui.label(crate::i18n::t(
                "Ajouter un script spatial (au départ du joueur) :",
            ));
            add = picker(ui, &mut self.edit.spatial_filter, &names);
        });
        if let Some(f) = open {
            self.open_script_request = Some(f);
        }
        if let Some(p) = set_main {
            self.mutate_as("Script principal de la carte", |m, _| {
                m.set_map_script(p);
                true
            });
            self.edit.map_script_filter.clear();
        }
        if !edits.is_empty() {
            self.mutate(Some("spatial"), |m, _| {
                edits
                    .iter()
                    .fold(false, |a, &(pid, prog, tile, elev, radius)| {
                        m.edit_spatial_script(pid, prog, tile, elev, radius) | a
                    })
            });
        }
        if let Some(pid) = remove {
            self.mutate_as("Script spatial supprimé", |m, _| m.remove_script(pid));
        }
        if let Some(k) = add {
            self.mutate_as("Script spatial ajouté", |m, _| {
                m.add_spatial_script(k as u32, player, elevation, 3);
                true
            });
            self.edit.spatial_filter.clear();
        }
    }

    #[allow(dead_code)]
    pub(crate) fn critter_section() -> usize {
        SCRIPT_TYPE_CRITTER
    }
}
