//! Fenêtre « Textes (.msg) » : éditer les fichiers de messages du jeu (noms de prototypes, descriptions de cartes, dialogues).
//! Les modifications vont dans le dossier de correctifs (`data/text/<langue>/…`), avec une copie `.bak` ; seules les entrées
//! modifiées sont réécrites, le reste du fichier (commentaires, mise en page, accents) reste identique à l'octet.

use crate::app::MapperApp;
use crate::i18n::t;
use eframe::egui::{self, Color32, RichText};
use fo_core::msg::{MsgEdit, MsgFile, patch};
use std::collections::BTreeSet;

#[derive(Clone)]
struct Row {
    id: i32,
    audio: String,
    /// Texte sur une ligne : les retours à la ligne s'écrivent `\n`.
    text: String,
    /// Valeurs d'origine ; `None` pour une entrée ajoutée.
    orig: Option<(String, String)>,
}

impl Row {
    fn changed(&self) -> bool {
        self.orig
            .as_ref()
            .is_none_or(|o| o.0 != self.audio || o.1 != self.text)
    }
}

#[derive(Default)]
pub(crate) struct MsgEditor {
    pub open: bool,
    /// Chemins relatifs à `text/<langue>/` (`game/map.msg`, `dialog/abel.msg`).
    files: Vec<String>,
    file_filter: String,
    entry_filter: String,
    selected: Option<String>,
    /// Fichier réellement lu (langue du jeu, sinon anglais) et ses octets d'origine.
    path: String,
    raw: Vec<u8>,
    rows: Vec<Row>,
    deleted: BTreeSet<i32>,
    new_id: i32,
    notice: Option<String>,
}

pub(crate) fn escape(text: &str) -> String {
    text.replace('\n', "\\n")
}

pub(crate) fn unescape(text: &str) -> String {
    text.replace("\\n", "\n")
}

impl MsgEditor {
    fn dirty(&self) -> bool {
        !self.deleted.is_empty() || self.rows.iter().any(Row::changed)
    }

    /// Modifications à appliquer au fichier d'origine.
    fn edits(&self) -> Vec<MsgEdit> {
        let mut v: Vec<MsgEdit> = self.deleted.iter().map(|id| MsgEdit::Delete(*id)).collect();
        v.extend(
            self.rows
                .iter()
                .filter(|r| r.changed())
                .map(|r| MsgEdit::Set {
                    id: r.id,
                    audio: r.audio.clone(),
                    text: unescape(&r.text),
                }),
        );
        v
    }

    fn load(&mut self, path: String, raw: Vec<u8>) {
        let parsed = MsgFile::parse(&raw);
        self.rows = parsed
            .entries
            .iter()
            .map(|(id, e)| Row {
                id: *id,
                audio: e.audio.clone(),
                text: escape(&e.text),
                orig: Some((e.audio.clone(), escape(&e.text))),
            })
            .collect();
        self.new_id = parsed.entries.keys().next_back().map_or(1, |k| k + 1);
        self.deleted.clear();
        self.path = path;
        self.raw = raw;
    }
}

impl MapperApp {
    fn msg_dirs(&self) -> Vec<String> {
        let lang = self
            .game
            .as_ref()
            .map_or("english".to_string(), |g| g.data.config.language.clone());
        let mut dirs = vec![lang];
        if dirs[0] != "english" {
            dirs.push("english".into());
        }
        dirs
    }

    fn refresh_msg_files(&mut self) {
        let Some(g) = self.game.as_ref() else { return };
        let mut files: BTreeSet<String> = BTreeSet::new();
        for lang in self.msg_dirs() {
            for sub in ["game", "dialog"] {
                for e in g.data.vfs().list(&format!("text/{lang}/{sub}")) {
                    if !e.is_dir && e.name.ends_with(".msg") {
                        files.insert(format!("{sub}/{}", e.name));
                    }
                }
            }
        }
        self.msg.files = files.into_iter().collect();
    }

    /// Ouvre un fichier : la langue du jeu d'abord, sinon l'anglais.
    fn open_msg_file(&mut self, rel: &str) {
        let Some(g) = self.game.as_ref() else { return };
        for lang in self.msg_dirs() {
            let path = format!("text/{lang}/{rel}");
            if let Ok(raw) = g.data.read(&path) {
                self.msg.selected = Some(rel.to_string());
                self.msg.load(path, raw);
                self.msg.notice = None;
                return;
            }
        }
        self.msg.notice = Some(format!("{} {rel}", t("Fichier introuvable :")));
    }

    fn save_msg_file(&mut self) {
        let Some(g) = self.game.as_ref() else { return };
        let edits = self.msg.edits();
        if edits.is_empty() {
            return;
        }
        let out = patch(&self.msg.raw, &edits);
        match g.data.save_data_file(&self.msg.path, &out) {
            Ok(path) => {
                g.data.forget_messages();
                let (path_text, rel, raw) =
                    (path.display().to_string(), self.msg.selected.clone(), out);
                // Les noms qui viennent des messages (cartes, prototypes) sont relus.
                if let Some(game) = self.game.as_mut() {
                    game.maps = game.data.list_maps();
                }
                if let Some(rel) = rel {
                    self.msg.load(self.msg.path.clone(), raw);
                    self.msg.selected = Some(rel);
                }
                self.status = format!("{} : {path_text}", t("Textes enregistrés"));
                self.msg.notice = None;
            }
            Err(e) => self.msg.notice = Some(format!("{} : {e}", t("Échec de l'enregistrement"))),
        }
    }

    /// Essai (`FO_SHOT_MSG=1`, dossier jetable) : renomme la ville d'Arroyo dans `map.msg`, enregistre, relit la liste des cartes.
    pub(crate) fn verify_msg_editor(&mut self) {
        self.msg.open = true;
        self.refresh_msg_files();
        self.open_msg_file("game/map.msg");
        let before = self.game.as_ref().and_then(|g| {
            g.maps
                .iter()
                .find(|m| m.file == "arvillag")
                .and_then(|m| m.display.clone())
        });
        if let Some(r) = self.msg.rows.iter_mut().find(|r| r.id == 1500) {
            r.text = "Arroyo\u{e9} de test \u{20ac}".into();
        }
        let dirty = self.msg.dirty();
        self.save_msg_file();
        let after = self.game.as_ref().and_then(|g| {
            g.maps
                .iter()
                .find(|m| m.file == "arvillag")
                .and_then(|m| m.display.clone())
        });
        eprintln!(
            "VERIFY msg: modifie={dirty} ; carte avant {before:?} apres {after:?} ; encore modifie={}",
            self.msg.dirty()
        );
        eprintln!("VERIFY msg statut: {}", self.status);
    }

    pub(crate) fn msg_window(&mut self, ctx: &egui::Context) {
        if !self.msg.open {
            return;
        }
        if self.msg.files.is_empty() {
            self.refresh_msg_files();
        }
        let mut open = true;
        let mut choose: Option<String> = None;
        let mut save = false;
        let mut revert = false;
        let dirty = self.msg.dirty();
        egui::Window::new(t("Textes (.msg)"))
            .open(&mut open)
            .order(egui::Order::Middle)
            .collapsible(false)
            .default_size([900.0, 560.0])
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(210.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.msg.file_filter)
                                .hint_text(t("Filtrer…"))
                                .desired_width(f32::INFINITY),
                        );
                        let needle = self.msg.file_filter.to_lowercase();
                        egui::ScrollArea::vertical()
                            .id_salt("msg_files")
                            .auto_shrink([false, false])
                            .max_height(470.0)
                            .show(ui, |ui| {
                                for f in self.msg.files.iter().filter(|f| {
                                    needle.is_empty() || f.to_lowercase().contains(&needle)
                                }) {
                                    let on = self.msg.selected.as_deref() == Some(f.as_str());
                                    if ui.selectable_label(on, f).clicked() && !on {
                                        choose = Some(f.clone());
                                    }
                                }
                            });
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        let Some(sel) = self.msg.selected.clone() else {
                            ui.weak(t("Choisissez un fichier de messages."));
                            return;
                        };
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&self.msg.path).strong());
                            if dirty {
                                ui.colored_label(ui.visuals().warn_fg_color, t("● modifié"));
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.msg.entry_filter)
                                    .hint_text(t("Chercher un numéro ou un texte…"))
                                    .desired_width(240.0),
                            );
                            if ui
                                .add_enabled(dirty, egui::Button::new(t("Enregistrer")))
                                .clicked()
                            {
                                save = true;
                            }
                            if ui
                                .add_enabled(
                                    dirty,
                                    egui::Button::new(t("Annuler les modifications")),
                                )
                                .clicked()
                            {
                                revert = true;
                            }
                        });
                        if let Some(n) = &self.msg.notice {
                            ui.colored_label(Color32::LIGHT_RED, n);
                        }
                        let needle = self.msg.entry_filter.to_lowercase();
                        let shown: Vec<usize> = (0..self.msg.rows.len())
                            .filter(|&i| {
                                let r = &self.msg.rows[i];
                                needle.is_empty()
                                    || r.id.to_string() == needle
                                    || r.text.to_lowercase().contains(&needle)
                                    || r.audio.to_lowercase().contains(&needle)
                            })
                            .collect();
                        let mut remove: Option<usize> = None;
                        egui::ScrollArea::vertical()
                            .id_salt("msg_rows")
                            .auto_shrink([false, false])
                            .max_height(380.0)
                            .show_rows(ui, 26.0, shown.len(), |ui, range| {
                                for &i in &shown[range] {
                                    let row = &mut self.msg.rows[i];
                                    ui.horizontal(|ui| {
                                        let color = if row.changed() {
                                            ui.visuals().warn_fg_color
                                        } else {
                                            ui.visuals().text_color()
                                        };
                                        ui.label(
                                            RichText::new(format!("{:>6}", row.id))
                                                .monospace()
                                                .color(color),
                                        );
                                        ui.add(
                                            egui::TextEdit::singleline(&mut row.audio)
                                                .desired_width(80.0)
                                                .hint_text(t("audio")),
                                        );
                                        let w = (ui.available_width() - 34.0).max(120.0);
                                        ui.add(
                                            egui::TextEdit::singleline(&mut row.text)
                                                .desired_width(w),
                                        );
                                        if ui
                                            .small_button("✕")
                                            .on_hover_text(t("Supprimer l'entrée"))
                                            .clicked()
                                        {
                                            remove = Some(i);
                                        }
                                    });
                                }
                            });
                        if let Some(i) = remove {
                            let row = self.msg.rows.remove(i);
                            if row.orig.is_some() {
                                self.msg.deleted.insert(row.id);
                            }
                        }
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(t("Nouvelle entrée n°"));
                            ui.add(
                                crate::stepper::Stepper::new(&mut self.msg.new_id)
                                    .range(0..=i32::MAX),
                            );
                            let taken = self.msg.rows.iter().any(|r| r.id == self.msg.new_id);
                            if ui
                                .add_enabled(!taken, egui::Button::new(t("Ajouter")))
                                .clicked()
                            {
                                let id = self.msg.new_id;
                                self.msg.deleted.remove(&id);
                                self.msg.rows.push(Row {
                                    id,
                                    audio: String::new(),
                                    text: String::new(),
                                    orig: None,
                                });
                                self.msg.rows.sort_by_key(|r| r.id);
                                self.msg.entry_filter = id.to_string();
                                self.msg.new_id = id + 1;
                            }
                            if taken {
                                ui.weak(t("Ce numéro existe déjà."));
                            }
                        });
                        ui.weak(format!(
                            "{sel} — {} {}",
                            self.msg.rows.len(),
                            t("entrée(s)")
                        ));
                    });
                });
            });
        if let Some(f) = choose {
            if dirty {
                self.msg.notice = Some(
                    t("Enregistrez ou annulez d'abord les modifications de ce fichier.").into(),
                );
            } else {
                self.open_msg_file(&f);
            }
        }
        if save {
            self.save_msg_file();
        }
        if revert && let Some(rel) = self.msg.selected.clone() {
            self.open_msg_file(&rel);
        }
        self.msg.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_changed_rows_become_edits_and_newlines_round_trip() {
        let raw = b"{1}{}{un}\r\n{2}{}{deux}\r\n{3}{}{trois}\r\n".to_vec();
        let mut ed = MsgEditor::default();
        ed.load("text/english/game/x.msg".into(), raw.clone());
        assert!(!ed.dirty() && ed.edits().is_empty());
        ed.rows[1].text = escape("deux\nlignes");
        ed.rows.remove(2);
        ed.deleted.insert(3);
        ed.rows.push(Row {
            id: 9,
            audio: String::new(),
            text: "neuf".into(),
            orig: None,
        });
        assert!(ed.dirty());
        let out = patch(&ed.raw, &ed.edits());
        let m = MsgFile::parse(&out);
        assert_eq!(m.get(1).unwrap().text, "un");
        assert_eq!(m.get(2).unwrap().text, "deux\nlignes");
        assert!(m.get(3).is_none());
        assert_eq!(m.get(9).unwrap().text, "neuf");
        assert_eq!(unescape(&escape("a\nb")), "a\nb");
    }
}
