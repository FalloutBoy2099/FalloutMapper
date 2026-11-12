use eframe::egui;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const KEY_PROFILES: &str = "game_profiles_v1";
const KEY_ACTIVE_PROFILE: &str = "active_game_profile_v1";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct GameProfile {
    pub name: String,
    pub map_dir: PathBuf,
    pub test_dir: PathBuf,
}

impl GameProfile {
    pub(crate) fn test_root(&self) -> &std::path::Path {
        if self.test_dir.as_os_str().is_empty() {
            &self.map_dir
        } else {
            &self.test_dir
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ProfileState {
    pub items: Vec<GameProfile>,
    pub active: usize,
    pub open: bool,
    pub draft: GameProfile,
    pub editing: Option<usize>,
}

impl ProfileState {
    pub(crate) fn load(
        mut get: impl FnMut(&str) -> Option<String>,
        initial_dir: Option<PathBuf>,
    ) -> Self {
        let items = get(KEY_PROFILES)
            .and_then(|s| serde_json::from_str::<Vec<GameProfile>>(&s).ok())
            .unwrap_or_default();
        let mut items = items;
        if items.is_empty() {
            if let Some(dir) = initial_dir {
                items.push(GameProfile {
                    name: dir
                        .file_name()
                        .map_or_else(|| "Fallout".into(), |n| n.to_string_lossy().into_owned()),
                    map_dir: dir,
                    test_dir: PathBuf::new(),
                });
            }
        }
        let active = get(KEY_ACTIVE_PROFILE)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
            .min(items.len().saturating_sub(1));
        Self {
            items,
            active,
            ..Default::default()
        }
    }

    pub(crate) fn active(&self) -> Option<&GameProfile> {
        self.items.get(self.active)
    }

    pub(crate) fn save(&self, set: &mut dyn FnMut(&str, String)) {
        set(
            KEY_PROFILES,
            serde_json::to_string(&self.items).unwrap_or_default(),
        );
        set(KEY_ACTIVE_PROFILE, self.active.to_string());
    }

    pub(crate) fn ui(&mut self, ctx: &egui::Context) -> Option<(usize, GameProfile)> {
        if !self.open {
            return None;
        }
        let mut apply = None;
        let mut close = false;
        egui::Modal::new(egui::Id::unique("game_profiles")).show(ctx, |ui| {
            ui.set_width(720.0);
            ui.heading(crate::i18n::t("Installations et profils de test"));
            ui.label(crate::i18n::t(
                "Un profil associe un dossier de mapping à une installation de test facultative.",
            ));
            ui.separator();
            ui.columns(2, |cols| {
                cols[0].strong(crate::i18n::t("Profils enregistrés"));
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(&mut cols[0], |ui| {
                        for (i, p) in self.items.iter().enumerate() {
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(self.editing == Some(i), &p.name)
                                    .clicked()
                                {
                                    self.editing = Some(i);
                                    self.draft = p.clone();
                                }
                                if i == self.active {
                                    ui.weak(crate::i18n::t("Actif"));
                                }
                                if ui.small_button(crate::i18n::t("Utiliser")).clicked() {
                                    apply = Some((i, p.clone()));
                                }
                            });
                        }
                    });
                cols[1].strong(crate::i18n::t(if self.editing.is_some() {
                    "Modifier le profil"
                } else {
                    "Nouveau profil"
                }));
                cols[1].label(crate::i18n::t("Nom du mod"));
                cols[1].text_edit_singleline(&mut self.draft.name);
                cols[1].label(crate::i18n::t("Dossier de mapping (données à éditer)"));
                cols[1].horizontal(|ui| {
                    let mut path = self.draft.map_dir.to_string_lossy().to_string();
                    if ui
                        .add(egui::TextEdit::singleline(&mut path).desired_width(280.0))
                        .changed()
                    {
                        self.draft.map_dir = PathBuf::from(path);
                    }
                    if ui.button(crate::i18n::t("Parcourir…")).clicked() {
                        apply = Some((usize::MAX, self.draft.clone()));
                    }
                });
                cols[1].label(crate::i18n::t(
                    "Dossier de test (facultatif; vide = mapping)",
                ));
                cols[1].horizontal(|ui| {
                    let mut path = self.draft.test_dir.to_string_lossy().to_string();
                    if ui
                        .add(egui::TextEdit::singleline(&mut path).desired_width(280.0))
                        .changed()
                    {
                        self.draft.test_dir = PathBuf::from(path);
                    }
                    if ui.button(crate::i18n::t("Parcourir…")).clicked() {
                        apply = Some((usize::MAX - 1, self.draft.clone()));
                    }
                });
                cols[1].horizontal(|ui| {
                    if ui
                        .button(crate::i18n::t(if self.editing.is_some() {
                            "Mettre à jour"
                        } else {
                            "Ajouter"
                        }))
                        .clicked()
                        && !self.draft.name.trim().is_empty()
                        && !self.draft.map_dir.as_os_str().is_empty()
                    {
                        let mut p = self.draft.clone();
                        p.name = p.name.trim().to_string();
                        if let Some(i) = self.editing {
                            if i < self.items.len() {
                                self.items[i] = p;
                            }
                        } else {
                            self.items.push(p);
                            self.editing = Some(self.items.len() - 1);
                        }
                    }
                    if ui.button(crate::i18n::t("Nouveau")).clicked() {
                        self.editing = None;
                        self.draft = GameProfile::default();
                    }
                    if let Some(i) = self.editing
                        && ui.button(crate::i18n::t("Supprimer")).clicked()
                        && i < self.items.len()
                    {
                        self.items.remove(i);
                        self.active = self.active.min(self.items.len().saturating_sub(1));
                        self.editing = None;
                        self.draft = GameProfile::default();
                    }
                });
            });
            ui.separator();
            if ui.button(crate::i18n::t("Fermer")).clicked() {
                close = true;
            }
        });
        if close {
            self.open = false;
        }
        apply
    }
}

impl crate::app::MapperApp {
    pub(crate) fn profiles_window(&mut self, ctx: &egui::Context) {
        let Some((index, draft)) = self.profiles.ui(ctx) else {
            return;
        };
        if index == usize::MAX || index == usize::MAX - 1 {
            let purpose = if index == usize::MAX {
                crate::file_dialog::Purpose::ProfileMapDir
            } else {
                crate::file_dialog::Purpose::ProfileTestDir
            };
            let path = if index == usize::MAX {
                draft.map_dir.clone()
            } else {
                draft.test_dir.clone()
            };
            self.profiles.draft = draft;
            self.dialog = Some(crate::file_dialog::DialogState::new(purpose, &path, ""));
            return;
        }
        let has_unsaved = self.edit.dirty || self.docs.iter().any(|doc| doc.edit.dirty);
        if has_unsaved {
            self.status = crate::i18n::t(
                "Enregistrez ou fermez les cartes modifiées avant de changer de profil.",
            )
            .into();
            return;
        }
        if index < self.profiles.items.len() {
            self.profiles.items[index] = draft.clone();
            self.profiles.active = index;
            self.profiles.open = false;
            self.pending_map = None;
            self.open_game(&draft.map_dir, ctx);
        }
    }
}
