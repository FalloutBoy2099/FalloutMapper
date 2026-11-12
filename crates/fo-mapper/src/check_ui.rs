//! Fenêtre « Vérifier la carte » : liste ce que le vérificateur de `fo-core` trouve ; un clic sur une ligne sélectionne
//! l'objet concerné et centre la vue dessus.

use crate::app::MapperApp;
use crate::i18n::t;
use eframe::egui::{self, Color32, Pos2};
use fo_core::check::{Issue, IssueKind, check_map, count_by_kind, fix_issues};
use fo_core::geom;
use fo_scene::ObjectRef;

#[derive(Default)]
pub(crate) struct CheckState {
    pub open: bool,
    pub(crate) issues: Vec<Issue>,
    /// Carte examinée (fichier) ; vide : pas encore de vérification.
    checked: String,
    /// Filtre : nature de problème affichée (`None` : toutes).
    pub(crate) only: Option<IssueKind>,
}

impl MapperApp {
    /// Examine la carte courante (elle peut avoir des modifications non enregistrées).
    /// Corrige les problèmes d'une nature (ou tous les corrigeables), en une seule étape d'historique, puis revérifie.
    pub(crate) fn fix_check_issues(&mut self, only: Option<IssueKind>) {
        let issues: Vec<Issue> = self
            .check
            .issues
            .iter()
            .filter(|i| i.kind.fixable() && only.is_none_or(|k| k == i.kind))
            .cloned()
            .collect();
        if issues.is_empty() {
            return;
        }
        let mut report = fo_core::check::FixReport::default();
        self.mutate_as("Correction du vérificateur", |m, _| {
            report = fix_issues(m, &issues);
            report.total() > 0
        });
        self.selection.clear();
        self.run_check();
        self.status = format!(
            "{} {} {}, {} {}{} — {}",
            t("Corrigé :"),
            report.removed,
            t("objet(s) supprimé(s)"),
            report.scripts_detached,
            t("script(s) détaché(s)"),
            if report.start_moved {
                format!(", {}", t("départ replacé"))
            } else {
                String::new()
            },
            self.status,
        );
    }

    pub(crate) fn run_check(&mut self) {
        let (Some(cur), Some(g)) = (self.current.as_ref(), self.game.as_ref()) else {
            return;
        };
        self.check.issues = check_map(&cur.map, &g.data, &g.art);
        self.check.checked = cur.file.clone();
        self.status = if self.check.issues.is_empty() {
            t("Aucun problème trouvé.").into()
        } else {
            format!("{} {}", self.check.issues.len(), t("problème(s) trouvé(s)"))
        };
    }

    fn goto_issue(&mut self, issue: &Issue) {
        if let Some(e) = issue.elevation
            && e != self.elevation
            && self
                .current
                .as_ref()
                .is_some_and(|c| c.scenes.get(e).is_some_and(Option::is_some))
        {
            self.elevation = e;
        }
        if let (Some(e), Some(i)) = (issue.elevation, issue.index)
            && self
                .current
                .as_ref()
                .is_some_and(|c| c.map.objects.get(e).is_some_and(|l| i < l.len()))
        {
            self.selection.set_object(Some(ObjectRef {
                elevation: e as u8,
                index: i as u32,
            }));
        }
        if let Some(h) = issue.hex {
            let (x, y) = geom::hex_to_screen(h);
            self.camera.center = Pos2::new(x as f32, y as f32);
        }
    }

    pub(crate) fn check_window(&mut self, ctx: &egui::Context) {
        if !self.check.open {
            return;
        }
        // Première ouverture, ou autre carte : on vérifie tout de suite.
        if self
            .current
            .as_ref()
            .is_some_and(|c| c.file != self.check.checked)
        {
            self.run_check();
        }
        let mut open = true;
        let mut rerun = false;
        let mut fix: Option<Option<IssueKind>> = None;
        let mut goto: Option<Issue> = None;
        let counts = count_by_kind(&self.check.issues);
        egui::Window::new(t("Vérifier la carte")).open(&mut open).order(egui::Order::Middle).collapsible(false).default_size([520.0, 420.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button(t("Vérifier de nouveau")).clicked() {
                    rerun = true;
                }
                ui.label(if self.check.issues.is_empty() { t("Aucun problème trouvé.").to_string() } else { format!("{} {}", self.check.issues.len(), t("problème(s)")) });
            });
            let fixable_total: usize = counts.iter().filter(|(k, _)| k.fixable()).map(|(_, n)| n).sum();
            if fixable_total > 0 {
                ui.horizontal_wrapped(|ui| {
                    if ui.button(format!("{} ({fixable_total})", t("Tout corriger"))).on_hover_text(t("Supprime les objets fautifs, détache les scripts introuvables et replace le départ. Annulable (Ctrl+Z).")).clicked() {
                        fix = Some(None);
                    }
                    if let Some(kind) = self.check.only.filter(|k| k.fixable()) {
                        let n = counts.iter().find(|(k, _)| *k == kind).map_or(0, |(_, n)| *n);
                        if ui.button(format!("{} ({n})", t(kind.fix_label()))).clicked() {
                            fix = Some(Some(kind));
                        }
                    }
                });
            }
            if !counts.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut self.check.only, None, t("Tous"));
                    for (kind, n) in &counts {
                        ui.selectable_value(&mut self.check.only, Some(*kind), format!("{} ({n})", t(kind.label())));
                    }
                });
            }
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for issue in self.check.issues.iter().filter(|i| self.check.only.is_none_or(|k| k == i.kind)) {
                    let place = match (issue.elevation, issue.hex) {
                        (Some(e), Some(h)) => format!("{} {e}, {} {h}", t("élév."), t("hex")),
                        (None, Some(h)) => format!("{} {h}", t("hex")),
                        (Some(e), None) => format!("{} {e}", t("élév.")),
                        (None, None) => String::new(),
                    };
                    let label = format!("{} — {} {}", t(issue.kind.label()), issue.detail, if place.is_empty() { String::new() } else { format!("({place})") });
                    let color = match issue.kind {
                        IssueKind::MissingArt | IssueKind::Duplicate => ui.visuals().warn_fg_color,
                        _ => Color32::LIGHT_RED,
                    };
                    if ui.add(egui::Button::new(egui::RichText::new(label).color(color)).frame(false)).clicked() {
                        goto = Some(issue.clone());
                    }
                }
            });
        });
        if rerun {
            self.run_check();
        }
        if let Some(only) = fix {
            self.fix_check_issues(only);
        }
        if let Some(i) = goto {
            self.goto_issue(&i);
        }
        self.check.open = open;
    }
}
