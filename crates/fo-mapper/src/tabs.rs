//! Cartes en onglets. L'état de la carte active (carte, historique d'édition, sélection, vue…) vit dans les champs de
//! `MapperApp` ; les autres onglets sont « rangés » dans des `Doc`. Changer d'onglet échange les deux.

use crate::app::MapperApp;
use crate::app::{Confirm, Mode};
use crate::edit_ui::EditState;
use crate::loader::LoadedMap;
use crate::selection::Selection;
use crate::view::Camera;
use eframe::egui::{self, Vec2};

#[derive(Default)]
pub(crate) struct Doc {
    pub current: Option<LoadedMap>,
    pub edit: EditState,
    pub selection: Selection,
    pub camera: Camera,
    pub elevation: usize,
    pub needs_fit: bool,
    pub start_view: bool,
}

/// Fermeture de plusieurs onglets d'affilée : `keep` est l'onglet à conserver (aucun pour « Fermer tous »).
/// `asked` retient l'onglet pour lequel une confirmation est affichée : si elle disparaît et que l'onglet est
/// toujours là, l'utilisateur a annulé et la suite est abandonnée.
pub(crate) struct CloseBatch {
    keep: Option<String>,
    all: bool,
    asked: Option<Option<String>>,
}

impl MapperApp {
    /// Avance la fermeture groupée d'une étape ; à appeler à chaque image.
    fn step_close_batch(&mut self, ctx: &egui::Context) {
        let Some(batch) = &self.close_batch else {
            return;
        };
        if self.confirm.is_some() {
            return;
        }
        if let Some(asked) = &batch.asked {
            if asked.is_some()
                && (0..self.docs.len()).any(|i| self.tab_file(i).map(str::to_string) == *asked)
            {
                self.close_batch = None;
                return;
            }
        }
        let (keep, all) = (batch.keep.clone(), batch.all);
        let target =
            (0..self.docs.len()).find(|&i| all || self.tab_file(i).map(str::to_string) != keep);
        let Some(i) = target else {
            self.close_batch = None;
            return;
        };
        ctx.request_repaint();
        if i != self.active_tab && !self.tab_dirty(i) {
            self.docs.remove(i);
            if i < self.active_tab {
                self.active_tab -= 1;
            }
            self.split = None;
            if let Some(b) = &mut self.close_batch {
                b.asked = None;
            }
            return;
        }
        let name = self.tab_file(i).map(str::to_string);
        self.switch_tab(i);
        self.request_close();
        if let Some(b) = &mut self.close_batch {
            b.asked = Some(name);
        }
    }

    /// Range l'état de la carte active dans un `Doc` (les champs reviennent à leurs valeurs par défaut).
    fn take_doc(&mut self) -> Doc {
        Doc {
            current: self.current.take(),
            edit: std::mem::take(&mut self.edit),
            selection: std::mem::take(&mut self.selection),
            camera: std::mem::take(&mut self.camera),
            elevation: std::mem::take(&mut self.elevation),
            needs_fit: std::mem::take(&mut self.needs_fit),
            start_view: std::mem::take(&mut self.start_view),
        }
    }

    fn put_doc(&mut self, doc: Doc) {
        self.current = doc.current;
        self.edit = doc.edit;
        self.selection = doc.selection;
        self.camera = doc.camera;
        self.elevation = doc.elevation;
        self.needs_fit = doc.needs_fit;
        self.start_view = doc.start_view;
        self.hover_hex = None;
        if let (Some(cur), Some(g)) = (&self.current, &self.game) {
            self.scripts.set_map(&g.data, &cur.map);
        }
    }

    /// Nom de fichier de la carte de l'onglet `i`.
    pub(crate) fn tab_file(&self, i: usize) -> Option<&str> {
        if i == self.active_tab {
            self.current.as_ref()
        } else {
            self.docs.get(i)?.current.as_ref()
        }
        .map(|c| c.file.as_str())
    }

    fn tab_dirty(&self, i: usize) -> bool {
        if i == self.active_tab {
            self.edit.dirty
        } else {
            self.docs.get(i).is_some_and(|d| d.edit.dirty)
        }
    }

    /// S'assure qu'il y a une entrée d'onglet pour la carte active.
    pub(crate) fn ensure_tab(&mut self) {
        if self.docs.is_empty() {
            self.docs.push(Doc::default());
            self.active_tab = 0;
        }
    }

    /// Ouvre un onglet vide (la carte active est rangée) : la carte chargée ensuite s'y affichera.
    pub(crate) fn open_new_tab(&mut self) {
        self.ensure_tab();
        let parked = self.take_doc();
        let (enabled, tool) = (parked.edit.enabled, parked.edit.tool);
        let at = self.active_tab;
        self.docs[at] = parked;
        self.docs.push(Doc::default());
        // Le mode d'édition et l'outil suivent d'un onglet à l'autre.
        self.edit.enabled = enabled;
        self.edit.tool = tool;
        self.active_tab = self.docs.len() - 1;
        self.split = None;
    }

    pub(crate) fn switch_tab(&mut self, i: usize) {
        if i == self.active_tab || i >= self.docs.len() {
            return;
        }
        let parked = self.take_doc();
        let at = self.active_tab;
        self.docs[at] = parked;
        let enabled = self.edit.enabled;
        let next = std::mem::take(&mut self.docs[i]);
        self.active_tab = i;
        self.split = None;
        self.put_doc(next);
        self.edit.enabled = enabled;
        self.status = self
            .current
            .as_ref()
            .map(|c| c.file.clone())
            .unwrap_or_default();
    }

    /// Retire l'onglet actif (sa carte est déjà vidée) et affiche le voisin, s'il y en a un.
    pub(crate) fn drop_active_tab(&mut self) {
        self.split = None;
        if self.docs.len() <= 1 {
            self.docs.clear();
            self.active_tab = 0;
            return;
        }
        self.docs.remove(self.active_tab);
        self.active_tab = self.active_tab.min(self.docs.len() - 1);
        let next = std::mem::take(&mut self.docs[self.active_tab]);
        self.put_doc(next);
    }

    /// Premier onglet dont la carte a des modifications non enregistrées (l'actif d'abord).
    pub(crate) fn first_dirty_tab(&self) -> Option<usize> {
        if self.edit.dirty {
            return Some(self.active_tab);
        }
        (0..self.docs.len()).find(|&i| self.tab_dirty(i))
    }

    /// Bande d'onglets au-dessus de la carte.
    pub(crate) fn tab_bar(&mut self, ui: &mut egui::Ui) {
        if self.mode == Mode::Map {
            self.step_close_batch(ui.ctx());
        }
        if self.mode != Mode::Map || self.docs.is_empty() {
            return;
        }
        let mut switch = None;
        let mut close = None;
        let mut batch = None;
        let mut split = None;
        let split_now = self.split;
        egui::Panel::top("doc_tabs").show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .id_salt("doc_tabs_scroll")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // Onglets serrés : à peine quelques pixels entre deux.
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for i in 0..self.docs.len() {
                            let name = self
                                .tab_file(i)
                                .map_or_else(|| "…".to_string(), |f| f.to_string());
                            let dirty = self.tab_dirty(i);
                            let label = name;
                            let selected = i == self.active_tab;
                            let (r, closed) = tab_widget(ui, &label, selected, dirty);
                            if r.clicked() && !closed {
                                switch = Some(i);
                            }
                            if closed || r.middle_clicked() {
                                close = Some(i);
                            }
                            r.context_menu(|ui| {
                                if i != self.active_tab
                                    && ui
                                        .button(crate::i18n::t("Afficher à droite (côte à côte)"))
                                        .clicked()
                                {
                                    split = Some(Some(i));
                                    ui.close();
                                }
                                if ui.button(crate::i18n::t("Fermer l'onglet")).clicked() {
                                    close = Some(i);
                                    ui.close();
                                }
                                if self.docs.len() > 1
                                    && ui
                                        .button(crate::i18n::t("Fermer les autres onglets"))
                                        .clicked()
                                {
                                    batch = Some(CloseBatch {
                                        keep: self.tab_file(i).map(str::to_string),
                                        all: false,
                                        asked: None,
                                    });
                                    ui.close();
                                }
                                if ui
                                    .button(crate::i18n::t("Fermer tous les onglets"))
                                    .clicked()
                                {
                                    batch = Some(CloseBatch {
                                        keep: None,
                                        all: true,
                                        asked: None,
                                    });
                                    ui.close();
                                }
                            });
                        }
                        if split_now.is_some() && ui.button(crate::i18n::t("Vue unique")).clicked()
                        {
                            split = Some(None);
                        }
                    });
                });
        });
        if batch.is_some() {
            self.close_batch = batch;
        }
        if let Some(i) = switch {
            self.switch_tab(i);
        }
        if let Some(s) = split {
            self.set_split(s);
        }
        if let Some(i) = close {
            self.switch_tab(i);
            self.request_close();
        }
    }

    /// Fermeture de la fenêtre : si un onglet a des modifications, on l'affiche et on demande quoi en faire.
    pub(crate) fn quit_requested(&mut self) {
        if let Some(i) = self.first_dirty_tab() {
            self.switch_tab(i);
            self.confirm = Some(Confirm::Quit);
        }
    }
}

/// Un onglet : fond, nom et croix de fermeture dans la même forme. Renvoie la réponse de l'onglet et `true` si la croix a été cliquée.
fn tab_widget(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    dirty: bool,
) -> (egui::Response, bool) {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, egui::Color32::PLACEHOLDER);
    let close_w = 22.0;
    // Carte modifiée : un point dessiné (centré en hauteur) entre le nom et la croix, pas un caractère du texte.
    let dot_w = if dirty { 14.0 } else { 0.0 };
    let size = Vec2::new(galley.size().x + 24.0 + dot_w + close_w, 26.0);
    let (rect, r) = ui.allocate_exact_size(size, egui::Sense::click());
    let close_rect = egui::Rect::from_min_size(
        egui::pos2(rect.max.x - close_w - 2.0, rect.min.y + 3.0),
        Vec2::new(close_w, rect.height() - 6.0),
    );
    let close_resp = ui
        .interact(close_rect, r.id.with("close"), egui::Sense::click())
        .on_hover_text(crate::i18n::t("Fermer l'onglet"));
    let visuals = ui.style().visuals.clone();
    let fill = if selected {
        visuals.selection.bg_fill
    } else if r.hovered() {
        visuals.widgets.hovered.weak_bg_fill
    } else {
        visuals.widgets.inactive.weak_bg_fill
    };
    let text_color = if selected {
        visuals.selection.stroke.color
    } else {
        visuals.widgets.inactive.fg_stroke.color
    };
    ui.painter().rect_filled(rect, 6.0, fill);
    let text_w = galley.size().x;
    ui.painter().galley(
        egui::pos2(rect.min.x + 12.0, rect.center().y - galley.size().y / 2.0),
        galley,
        text_color,
    );
    if dirty {
        ui.painter().circle_filled(
            egui::pos2(rect.min.x + 12.0 + text_w + 9.0, rect.center().y),
            3.5,
            text_color,
        );
    }
    if close_resp.hovered() {
        ui.painter()
            .rect_filled(close_rect, 4.0, text_color.gamma_multiply(0.2));
    }
    let c = close_rect.center();
    let stroke = egui::Stroke::new(
        1.4,
        if close_resp.hovered() {
            text_color
        } else {
            text_color.gamma_multiply(0.7)
        },
    );
    ui.painter()
        .line_segment([c + Vec2::new(-4.0, -4.0), c + Vec2::new(4.0, 4.0)], stroke);
    ui.painter()
        .line_segment([c + Vec2::new(-4.0, 4.0), c + Vec2::new(4.0, -4.0)], stroke);
    (r, close_resp.clicked())
}
