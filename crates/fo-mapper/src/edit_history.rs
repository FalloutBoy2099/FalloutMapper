//! Édition de la carte : outils, annulation / rétablissement, palettes, enregistrement.

use crate::app::MapperApp;
use eframe::egui::{self};
use fo_core::map::Map;
use fo_core::GameData;
use fo_scene::build_scene;
use std::sync::Arc;
use std::time::Instant;

use crate::edit_ui::*;

impl MapperApp {
    /// Applique une modification à la carte. `f` renvoie `true` si quelque chose a changé.
    /// Avec une `tag`, les modifications répétées du même champ se fondent en une seule étape d'annulation.
    pub(crate) fn mutate(
        &mut self,
        tag: Option<&'static str>,
        f: impl FnOnce(&mut Map, &GameData) -> bool,
    ) -> bool {
        self.mutate_labeled(tag, None, f)
    }

    /// Comme `mutate` sans fusion, avec le libellé de l'étape d'historique.
    pub(crate) fn mutate_as(
        &mut self,
        label: &'static str,
        f: impl FnOnce(&mut Map, &GameData) -> bool,
    ) -> bool {
        self.mutate_labeled(None, Some(label), f)
    }

    pub(crate) fn mutate_labeled(
        &mut self,
        tag: Option<&'static str>,
        label: Option<&'static str>,
        f: impl FnOnce(&mut Map, &GameData) -> bool,
    ) -> bool {
        let (Some(cur), Some(g)) = (self.current.as_mut(), self.game.as_ref()) else {
            return false;
        };
        let coalesce = tag.is_some()
            && self
                .edit
                .last_tag
                .is_some_and(|(t, at)| Some(t) == tag && at.elapsed() < COALESCE);
        let snapshot = (!coalesce).then(|| (*cur.map).clone());
        if !f(Arc::make_mut(&mut cur.map), &g.data) {
            return false;
        }
        if let Some(s) = snapshot {
            let what = label.or(tag.map(history_label)).unwrap_or("Modification");
            // Le journal garde chaque étape d'édition, avec l'hexagone sous le curseur quand l'action vient d'un clic sur la carte.
            self.log
                .push(crate::i18n::t(what).to_string(), self.hover_hex);
            self.edit.push_undo(s, what);
        } else {
            self.edit.redo.clear();
            self.edit.redo_labels.clear();
        }
        self.edit.dirty = true;
        self.edit.last_tag = tag.map(|t| (t, Instant::now()));
        self.rebuild_scene(self.elevation);
        true
    }

    pub(crate) fn rebuild_scene(&mut self, elevation: usize) {
        let (Some(cur), Some(g)) = (self.current.as_mut(), self.game.as_ref()) else {
            return;
        };
        cur.scenes[elevation] = Some(Arc::new(build_scene(&cur.map, elevation, &g.art)));
        self.edit.last_rebuild = Instant::now();
    }

    pub(crate) fn rebuild_all(&mut self) {
        for e in 0..3 {
            if self
                .current
                .as_ref()
                .is_some_and(|c| c.scenes[e].is_some() || c.map.header.has_elevation(e))
            {
                self.rebuild_scene(e);
            }
        }
    }

    pub(crate) fn undo(&mut self) {
        let Some(cur) = self.current.as_mut() else {
            return;
        };
        if let Some(prev) = self.edit.undo.pop() {
            let now = std::mem::replace(&mut *Arc::make_mut(&mut cur.map), prev);
            self.edit.redo.push(now);
            let label = self.edit.undo_labels.pop().unwrap_or("Modification");
            self.edit.redo_labels.push(label);
            self.after_history_step(crate::i18n::t("Annulé"));
        }
    }

    /// Fenêtre d'historique : revenir (ou avancer) jusqu'à une étape donnée.
    pub(crate) fn history_window(&mut self, ctx: &egui::Context) {
        if !self.history_open {
            return;
        }
        let mut open = true;
        let mut target = None;
        let (done, undone) = (self.edit.undo_labels.clone(), self.edit.redo_labels.clone());
        egui::Window::new(crate::i18n::t("Historique"))
            .open(&mut open)
            .default_size([320.0, 420.0])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if ui
                            .selectable_label(
                                done.is_empty() && undone.is_empty() || false,
                                crate::i18n::t("État initial"),
                            )
                            .clicked()
                        {
                            target = Some(0);
                        }
                        for (i, label) in done.iter().enumerate() {
                            let current = i + 1 == done.len();
                            if ui
                                .selectable_label(
                                    current,
                                    format!("{}. {}", i + 1, crate::i18n::t(label)),
                                )
                                .clicked()
                            {
                                target = Some(i + 1);
                            }
                        }
                        // Étapes annulées : la plus proche de l'état courant est la dernière de `redo_labels`.
                        for (k, label) in undone.iter().rev().enumerate() {
                            let n = done.len() + k + 1;
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(format!(
                                            "{n}. {}",
                                            crate::i18n::t(label)
                                        ))
                                        .weak(),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                target = Some(n);
                            }
                        }
                    });
            });
        if let Some(t) = target {
            let now = self.edit.undo.len();
            for _ in t..now {
                self.undo();
            }
            for _ in now..t {
                self.redo();
            }
        }
        self.history_open = open;
    }

    pub(crate) fn redo(&mut self) {
        let Some(cur) = self.current.as_mut() else {
            return;
        };
        if let Some(next) = self.edit.redo.pop() {
            let now = std::mem::replace(&mut *Arc::make_mut(&mut cur.map), next);
            self.edit.undo.push(now);
            let label = self.edit.redo_labels.pop().unwrap_or("Modification");
            self.edit.undo_labels.push(label);
            self.after_history_step(crate::i18n::t("Rétabli"));
        }
    }

    pub(crate) fn after_history_step(&mut self, what: &str) {
        self.edit.dirty = true;
        self.edit.last_tag = None;
        self.selection.clear();
        self.rebuild_all();
        self.status = format!(
            "{what} ({} {})",
            self.edit.undo.len(),
            crate::i18n::t("étapes restantes")
        );
    }

    // ---------------------------------------------------------------- enregistrement
}
