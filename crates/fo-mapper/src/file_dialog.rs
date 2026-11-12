//! Dialogue d'ouverture de dossier et d'enregistrement de fichier maison : liste des dossiers récents, lecteurs,
//! saisie du chemin. Remplace les boîtes du système.

use crate::app::MapperApp;
use crate::i18n::t;
use crate::ui_kit::FooterButton as B;
use eframe::egui;
use std::path::{Path, PathBuf};

const MAX_RECENT: usize = 10;

/// À quoi sert le dialogue ouvert.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Purpose {
    OpenGame,
    ProfileMapDir,
    ProfileTestDir,
    SaveMapAs,
    /// Image PNG de la carte ou de l'aperçu du jeu.
    ExportImage,
}

pub(crate) struct DialogState {
    pub purpose: Purpose,
    title: &'static str,
    dir: PathBuf,
    path_text: String,
    /// Nom du fichier (enregistrement seulement).
    name: String,
    /// Extension des fichiers proposés (enregistrement) ; `None` : dossiers seulement.
    ext: Option<&'static str>,
    folders: Vec<String>,
    files: Vec<String>,
    error: Option<String>,
}

/// Place `p` en tête de la liste des récents (sans doublon, au plus `MAX_RECENT`).
pub(crate) fn push_recent(list: &mut Vec<PathBuf>, p: &Path) {
    list.retain(|q| q != p);
    list.insert(0, p.to_path_buf());
    list.truncate(MAX_RECENT);
}

pub(crate) fn recent_to_string(list: &[PathBuf]) -> String {
    list.iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn recent_from_string(s: &str) -> Vec<PathBuf> {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .take(MAX_RECENT)
        .collect()
}

/// Dossiers (triés, sans les cachés) et fichiers portant l'extension demandée.
fn read_dir_sorted(dir: &Path, ext: Option<&str>) -> std::io::Result<(Vec<String>, Vec<String>)> {
    let (mut folders, mut files) = (Vec::new(), Vec::new());
    for e in std::fs::read_dir(dir)?.filter_map(|e| e.ok()) {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(ty) = e.file_type() else { continue };
        if ty.is_dir() {
            folders.push(name);
        } else if ext.is_some_and(|x| {
            Path::new(&name)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(x))
        }) {
            files.push(name);
        }
    }
    folders.sort_by_key(|n| n.to_lowercase());
    files.sort_by_key(|n| n.to_lowercase());
    Ok((folders, files))
}

fn drives() -> Vec<PathBuf> {
    ('A'..='Z')
        .map(|c| PathBuf::from(format!("{c}:\\")))
        .filter(|p| p.is_dir())
        .collect()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}

impl DialogState {
    pub(crate) fn new(purpose: Purpose, start: &Path, name: &str) -> Self {
        let (title, ext) = match purpose {
            Purpose::OpenGame => ("Dossier de Fallout 2", None),
            Purpose::ProfileMapDir => ("Dossier de mapping", None),
            Purpose::ProfileTestDir => ("Dossier de test", None),
            Purpose::SaveMapAs => ("Enregistrer la carte sous", Some("map")),
            Purpose::ExportImage => ("Exporter une image", Some("png")),
        };
        let dir = std::iter::successors(Some(start.to_path_buf()), |p| {
            p.parent().map(Path::to_path_buf)
        })
        .find(|p| p.is_dir())
        .or_else(home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
        let mut s = Self {
            purpose,
            title,
            dir: dir.clone(),
            path_text: String::new(),
            name: name.to_string(),
            ext,
            folders: Vec::new(),
            files: Vec::new(),
            error: None,
        };
        s.go(&dir);
        s
    }

    fn go(&mut self, dir: &Path) {
        match read_dir_sorted(dir, self.ext) {
            Ok((folders, files)) => {
                self.dir = dir.to_path_buf();
                self.path_text = dir.display().to_string();
                self.folders = folders;
                self.files = files;
                self.error = None;
            }
            Err(e) => self.error = Some(format!("{} : {e}", dir.display())),
        }
    }

    fn target(&self) -> PathBuf {
        match (self.purpose, self.ext) {
            (Purpose::SaveMapAs | Purpose::ExportImage, Some(x)) => {
                let mut n = self.name.trim().to_string();
                if !n.to_lowercase().ends_with(&format!(".{x}")) {
                    n.push('.');
                    n.push_str(x);
                }
                self.dir.join(n)
            }
            _ => self.dir.clone(),
        }
    }
}

enum Outcome {
    Cancel,
    Accept(PathBuf),
}

impl MapperApp {
    pub(crate) fn open_dialog(&mut self, purpose: Purpose) {
        let root = self.game.as_ref().map(|g| g.data.root.clone());
        let (start, name) = match purpose {
            Purpose::OpenGame | Purpose::ProfileMapDir => (
                self.recent_games
                    .first()
                    .cloned()
                    .or(root)
                    .unwrap_or_default(),
                String::new(),
            ),
            Purpose::ProfileTestDir => (
                if self.profiles.draft.test_dir.as_os_str().is_empty() {
                    root.unwrap_or_default()
                } else {
                    self.profiles.draft.test_dir.clone()
                },
                String::new(),
            ),
            Purpose::SaveMapAs => (
                self.recent_save_dirs
                    .first()
                    .cloned()
                    .or_else(|| root.map(|r| r.join("data").join("maps")))
                    .unwrap_or_default(),
                format!(
                    "{}.map",
                    self.current.as_ref().map_or("", |c| c.file.as_str())
                ),
            ),
            Purpose::ExportImage => (
                self.recent_save_dirs.first().cloned().unwrap_or_default(),
                format!(
                    "{}.png",
                    self.current.as_ref().map_or("", |c| c.file.as_str())
                ),
            ),
        };
        self.dialog = Some(DialogState::new(purpose, &start, &name));
    }

    pub(crate) fn dialog_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.dialog.take() else {
            return;
        };
        let folder_purpose = matches!(
            d.purpose,
            Purpose::OpenGame | Purpose::ProfileMapDir | Purpose::ProfileTestDir
        );
        let recent = if folder_purpose {
            self.recent_games.clone()
        } else {
            self.recent_save_dirs.clone()
        };
        let mut navigate: Option<PathBuf> = None;
        let mut outcome = None;
        egui::Window::new(t(d.title))
            .collapsible(false)
            .resizable(false)
            .order(egui::Order::Middle)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .fixed_size([720.0, 400.0])
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(200.0);
                        ui.strong(t("Récents"));
                        if recent.is_empty() {
                            ui.weak(t("Aucun dossier récent."));
                        }
                        for p in &recent {
                            let label = p.file_name().map_or_else(
                                || p.display().to_string(),
                                |n| n.to_string_lossy().into_owned(),
                            );
                            if ui
                                .add(egui::Button::new(label).frame(false))
                                .on_hover_text(p.display().to_string())
                                .clicked()
                            {
                                navigate = Some(p.clone());
                            }
                        }
                        ui.separator();
                        ui.strong(t("Emplacements"));
                        if let Some(h) = home_dir()
                            && ui
                                .add(egui::Button::new(t("Dossier personnel")).frame(false))
                                .clicked()
                        {
                            navigate = Some(h);
                        }
                        for dr in drives() {
                            if ui
                                .add(egui::Button::new(dr.display().to_string()).frame(false))
                                .clicked()
                            {
                                navigate = Some(dr);
                            }
                        }
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            if ui.button("↑").on_hover_text(t("Dossier parent")).clicked()
                                && let Some(p) = d.dir.parent()
                            {
                                navigate = Some(p.to_path_buf());
                            }
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut d.path_text)
                                    .desired_width(f32::INFINITY),
                            );
                            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                navigate = Some(PathBuf::from(d.path_text.trim()));
                            }
                        });
                        if let Some(e) = &d.error {
                            ui.colored_label(egui::Color32::LIGHT_RED, e);
                        }
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .min_scrolled_height(300.0)
                            .max_height(300.0)
                            .show(ui, |ui| {
                                for f in &d.folders {
                                    if ui
                                        .add(egui::Button::new(format!("▸ {f}")).frame(false))
                                        .clicked()
                                    {
                                        navigate = Some(d.dir.join(f));
                                    }
                                }
                                for f in &d.files {
                                    if ui.selectable_label(d.name == *f, f).clicked() {
                                        d.name = f.clone();
                                    }
                                }
                            });
                        if !folder_purpose {
                            ui.horizontal(|ui| {
                                ui.label(t("Nom du fichier"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut d.name)
                                        .desired_width(f32::INFINITY),
                                );
                            });
                            if d.target().exists() {
                                ui.colored_label(
                                    egui::Color32::LIGHT_RED,
                                    t("Ce fichier existe déjà : il sera remplacé."),
                                );
                            }
                        }
                    });
                });
                let ok = match d.purpose {
                    Purpose::OpenGame | Purpose::ProfileMapDir | Purpose::ProfileTestDir => {
                        t("Choisir ce dossier")
                    }
                    Purpose::SaveMapAs | Purpose::ExportImage => t("Enregistrer"),
                };
                let enabled = folder_purpose || !d.name.trim().is_empty();
                match crate::ui_kit::footer(
                    ui,
                    &[B::normal(t("Annuler")), B::primary(ok).enabled(enabled)],
                ) {
                    Some(0) => outcome = Some(Outcome::Cancel),
                    Some(_) => outcome = Some(Outcome::Accept(d.target())),
                    None => {}
                }
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            outcome = Some(Outcome::Cancel);
        }
        if let Some(p) = navigate {
            d.go(&p);
        }
        match outcome {
            None => self.dialog = Some(d),
            Some(Outcome::Cancel) => {}
            Some(Outcome::Accept(path)) => self.dialog_accepted(d.purpose, path, ctx),
        }
    }

    fn dialog_accepted(&mut self, purpose: Purpose, path: PathBuf, ctx: &egui::Context) {
        match purpose {
            Purpose::OpenGame => self.open_game(&path, ctx),
            Purpose::ProfileMapDir => self.profiles.draft.map_dir = path,
            Purpose::ProfileTestDir => self.profiles.draft.test_dir = path,
            Purpose::SaveMapAs => {
                if let Some(dir) = path.parent() {
                    push_recent(&mut self.recent_save_dirs, dir);
                }
                self.write_map_copy(&path);
            }
            Purpose::ExportImage => {
                if let Some(dir) = path.parent() {
                    push_recent(&mut self.recent_save_dirs, dir);
                }
                self.export_to(&path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_list_is_deduplicated_bounded_and_round_trips() {
        let mut l = Vec::new();
        for i in 0..15 {
            push_recent(&mut l, Path::new(&format!("C:/jeu{i}")));
        }
        push_recent(&mut l, Path::new("C:/jeu10"));
        assert_eq!(l.len(), MAX_RECENT);
        assert_eq!(l[0], Path::new("C:/jeu10"));
        assert_eq!(l.iter().filter(|p| *p == Path::new("C:/jeu10")).count(), 1);
        assert_eq!(recent_from_string(&recent_to_string(&l)), l);
    }

    #[test]
    fn save_target_gets_the_extension() {
        let mut d = DialogState::new(Purpose::SaveMapAs, Path::new("."), "carte");
        d.dir = PathBuf::from("X");
        assert_eq!(d.target(), PathBuf::from("X").join("carte.map"));
        d.name = "autre.MAP".into();
        assert_eq!(d.target(), PathBuf::from("X").join("autre.MAP"));
    }
}
