#![allow(deprecated)]
//! Barres d'outils flottantes au-dessus de la carte : en haut les actions et les outils d'édition, en bas l'affichage, le
//! zoom et l'élévation. Chaque bouton a une info-bulle ; la même description alimente la zone d'aide du bas.

use crate::app::MapperApp;
use crate::edit_ui::Tool;
use crate::icons::{Icon, icon_button as icon_btn};
use eframe::egui::{self, Align2, Frame, Id, LayerId, Rect, Response, Vec2};
use fo_scene::Layer;

const BUTTON: Vec2 = Vec2::new(36.0, 32.0);

impl Tool {
    /// Nom stable de l'outil (pour mémoriser la barre d'outils).
    fn code(self) -> &'static str {
        match self {
            Tool::Select => "select",
            Tool::Floor => "floor",
            Tool::Roof => "roof",
            Tool::Place => "place",
            Tool::PlayerStart => "start",
            Tool::Exit => "exit",
            Tool::Blocker => "blocker",
            Tool::Stamp => "stamp",
            Tool::Room => "room",
            Tool::Wall => "wall",
            Tool::Spatial => "spatial",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Tool::Select => Icon::Select,
            Tool::Floor => Icon::Floor,
            Tool::Roof => Icon::Roof,
            Tool::Place => Icon::Place,
            Tool::PlayerStart => Icon::Start,
            Tool::Exit => Icon::Exit,
            Tool::Blocker => Icon::Blocker,
            Tool::Stamp => Icon::Stamp,
            Tool::Room => Icon::Room,
            Tool::Wall => Icon::Wall,
            Tool::Spatial => Icon::Spatial,
        }
    }

    pub(crate) fn help(self) -> &'static str {
        match self {
            Tool::Select => {
                "Clic ou rectangle pour sélectionner (Alt ajoute, Ctrl retire). Glisser un objet le déplace."
            }
            Tool::Floor => "Glissez sur la carte pour peindre la tuile de sol choisie.",
            Tool::Roof => "Glissez sur la carte pour peindre la tuile de toit choisie.",
            Tool::Place => {
                "Choisissez un prototype dans la palette, puis cliquez sur la carte (R fait tourner)."
            }
            Tool::PlayerStart => "Cliquez pour placer le point d'arrivée du joueur.",
            Tool::Exit => "Posez des grilles de sortie vers une autre carte.",
            Tool::Blocker => {
                "Glissez un rectangle : des bloqueurs de défilement bordent son contour."
            }
            Tool::Stamp => "Copiez la sélection, enregistrez-la comme motif, puis collez-la.",
            Tool::Room => "Cliquez pour construire une pièce : deux murs du fond et un sol.",
            Tool::Wall => "Tracez des murs et des rectangles, ou insérez une porte ou une fenêtre.",
            Tool::Spatial => {
                "Cliquez sur la carte pour poser un script spatial : un cercle montre son rayon."
            }
        }
    }
}

/// Outils de la barre, dans l'ordre voulu, chacun montré ou masqué : tous, dans l'ordre d'origine, par défaut.
pub(crate) fn default_tools() -> Vec<(Tool, bool)> {
    Tool::ALL.iter().map(|(t, _)| (*t, true)).collect()
}

/// « select,floor,-roof » : l'ordre, un tiret devant les outils masqués.
pub(crate) fn tools_to_string(tools: &[(Tool, bool)]) -> String {
    tools
        .iter()
        .map(|(t, on)| format!("{}{}", if *on { "" } else { "-" }, t.code()))
        .collect::<Vec<_>>()
        .join(",")
}

/// Relit `tools_to_string` ; les outils inconnus sont ignorés et ceux qui manquent (outil ajouté depuis) sont ajoutés à la fin.
/// L'outil de sélection reste toujours visible.
pub(crate) fn tools_from_string(text: &str) -> Vec<(Tool, bool)> {
    let mut out: Vec<(Tool, bool)> = Vec::new();
    for part in text.split(',') {
        let (code, on) = part.strip_prefix('-').map_or((part, true), |c| (c, false));
        if let Some((tool, _)) = Tool::ALL.iter().find(|(t, _)| t.code() == code.trim())
            && !out.iter().any(|(t, _)| t == tool)
        {
            out.push((*tool, on));
        }
    }
    for (tool, _) in Tool::ALL {
        if !out.iter().any(|(t, _)| *t == tool) {
            out.push((tool, true));
        }
    }
    if let Some(s) = out.iter_mut().find(|(t, _)| *t == Tool::Select) {
        s.1 = true;
    }
    out
}

fn icon_button(ui: &mut egui::Ui, icon: Icon, selected: bool, enabled: bool) -> Response {
    icon_btn(ui, icon, BUTTON, selected, enabled)
}

fn bar_frame(v: &egui::Visuals) -> Frame {
    Frame::NONE
        .fill(v.window_fill.gamma_multiply(0.92))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .stroke(v.window_stroke)
        .shadow(v.window_shadow)
}

impl MapperApp {
    pub(crate) fn map_toolbars(&mut self, ctx: &egui::Context, parent: LayerId, view: Rect) {
        if view.width() < 200.0
            || self.confirm.is_some()
            || self.file.new_map.is_some()
            || self.file.play_dialog
        {
            return;
        }
        let mut hint: Option<String> = None;
        let mut note = |r: &Response, text: &str| {
            if r.hovered() {
                hint = Some(text.to_string());
            }
            r.clone().on_hover_text(text);
        };
        let has_map = self.current.is_some();

        crate::ui_kit::canvas_overlay(ctx, parent, Id::new("toolbar_top"))
            .pivot(Align2::CENTER_TOP)
            .fixed_pos(view.center_top() + Vec2::new(0.0, 10.0))
            .show(ctx, |ui| {
                let bar = bar_frame(&ui.visuals().clone()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let r = icon_button(ui, Icon::Save, false, has_map && self.edit.dirty);
                        note(&r, crate::i18n::t("Enregistrer (Ctrl+S)"));
                        if r.clicked() {
                            self.save_current();
                        }
                        let r = icon_button(ui, Icon::Play, false, has_map);
                        note(&r, crate::i18n::t("Enregistrer et jouer cette carte (F5)"));
                        if r.clicked() {
                            self.request_play();
                        }
                        ui.separator();
                        let r = icon_button(ui, Icon::Edit, self.edit.enabled, has_map);
                        note(
                            &r,
                            crate::i18n::t(
                                "Mode édition : active les outils et la modification de la carte",
                            ),
                        );
                        if r.clicked() {
                            self.edit.enabled = !self.edit.enabled;
                        }
                        if self.edit.enabled {
                            ui.separator();
                            let r = icon_button(ui, Icon::Undo, false, self.edit.can_undo());
                            note(&r, crate::i18n::t("Annuler (Ctrl+Z)"));
                            if r.clicked() {
                                self.undo();
                            }
                            let r = icon_button(ui, Icon::Redo, false, self.edit.can_redo());
                            note(&r, crate::i18n::t("Rétablir (Ctrl+Y)"));
                            if r.clicked() {
                                self.redo();
                            }
                            ui.separator();
                            for (tool, _) in
                                self.toolbar_tools.clone().into_iter().filter(|(_, on)| *on)
                            {
                                let name = Tool::ALL
                                    .iter()
                                    .find(|(t, _)| *t == tool)
                                    .map_or("", |(_, n)| *n);
                                let r = icon_button(ui, tool.icon(), self.edit.tool == tool, true);
                                note(
                                    &r,
                                    &format!(
                                        "{} — {}",
                                        crate::i18n::t(name),
                                        crate::i18n::t(tool.help())
                                    ),
                                );
                                if r.clicked() {
                                    self.edit.set_tool(tool);
                                }
                            }
                        }
                    });
                });
                // Clic droit sur la barre : choisir les boutons d'outils et leur ordre.
                bar.response.context_menu(|ui| {
                    if ui
                        .button(crate::i18n::t("Personnaliser la barre d'outils…"))
                        .clicked()
                    {
                        self.toolbar_dialog = true;
                        ui.close();
                    }
                    if ui
                        .button(crate::i18n::t("Rétablir la barre d'outils"))
                        .clicked()
                    {
                        self.toolbar_tools = default_tools();
                        ui.close();
                    }
                });
            });

        crate::ui_kit::canvas_overlay(ctx, parent, Id::new("toolbar_bottom")).pivot(Align2::CENTER_BOTTOM).fixed_pos(view.center_bottom() - Vec2::new(0.0, 10.0)).show(ctx, |ui| {
            bar_frame(&ui.visuals().clone()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let r = icon_button(ui, Icon::Layers, false, true);
                    egui::Popup::menu(&r).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                        for layer in Layer::ALL {
                            let mut on = self.layers.contains(layer);
                            if ui.checkbox(&mut on, crate::i18n::t(layer.label())).changed() {
                                self.layers.set(layer, on);
                            }
                        }
                    });
                    note(&r, crate::i18n::t("Calques affichés : sols, objets, toits…"));
                    let r = icon_button(ui, Icon::Markers, self.show_markers, true);
                    note(&r, crate::i18n::t("Repères : point de départ, sorties, scripts spatiaux"));
                    if r.clicked() {
                        self.show_markers = !self.show_markers;
                    }
                    let r = icon_button(ui, Icon::Grid, self.show_grid, true);
                    note(&r, crate::i18n::t("Grille hexagonale (visible à partir de 60 % de zoom)"));
                    if r.clicked() {
                        self.show_grid = !self.show_grid;
                    }
                    let r = icon_button(ui, Icon::Edges, self.show_edges, true);
                    note(&r, crate::i18n::t("Limites de défilement (.edg)"));
                    if r.clicked() {
                        self.show_edges = !self.show_edges;
                    }
                    let r = icon_button(ui, Icon::Preview, self.preview.on, has_map);
                    note(&r, crate::i18n::t("Aperçu du jeu : lumière, toits masqués, sans repères (réglages dans Affichage)"));
                    if r.clicked() {
                        self.preview.on = !self.preview.on;
                    }
                    let r = icon_button(ui, Icon::Light, self.show_lights, true);
                    note(&r, crate::i18n::t("Éclairage : halo de chaque objet lumineux"));
                    if r.clicked() {
                        self.show_lights = !self.show_lights;
                    }
                    let r = icon_button(ui, Icon::Anim, self.anim_on, true);
                    note(&r, crate::i18n::t("Animation des couleurs de palette (eau, feu, écrans…)"));
                    if r.clicked() {
                        self.anim_on = !self.anim_on;
                    }
                    ui.separator();
                    let r = icon_button(ui, Icon::Fit, false, has_map);
                    note(&r, crate::i18n::t("Ajuster la carte à la fenêtre"));
                    if r.clicked() {
                        self.needs_fit = true;
                    }
                    let mut log_zoom = self.camera.zoom.ln();
                    let (lo, hi) = (crate::view::Camera::MIN_ZOOM.ln(), crate::view::Camera::MAX_ZOOM.ln());
                    let r = ui.add_sized([130.0, BUTTON.y], egui::Slider::new(&mut log_zoom, lo..=hi).show_value(false));
                    note(&r, crate::i18n::t("Zoom (molette de la souris sur la carte)"));
                    if r.changed() {
                        self.camera.zoom = log_zoom.exp();
                    }
                    ui.label(format!("{:.0} %", self.camera.zoom * 100.0));
                    let existing: Vec<usize> = self.current.as_ref().map_or(Vec::new(), |c| (0..3).filter(|&e| c.scenes[e].is_some()).collect());
                    if existing.len() > 1 {
                        ui.separator();
                        for e in existing {
                            let r = ui.add(egui::Button::new(egui::RichText::new(e.to_string()).size(16.0)).selected(self.elevation == e).min_size(BUTTON));
                            note(&r, &format!("{} {e}", crate::i18n::t("Élévation")));
                            if r.clicked() {
                                self.elevation = e;
                                self.selection.clear();
                                self.needs_fit = true;
                            }
                        }
                    }
                });
            });
        });
        crate::ui_kit::canvas_overlay(ctx, parent, Id::new("toolbar_left"))
            .pivot(Align2::LEFT_CENTER)
            .fixed_pos(view.left_center() + Vec2::new(10.0, 0.0))
            .show(ctx, |ui| {
                bar_frame(&ui.visuals().clone()).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        for (layer, icon) in [
                            (Layer::Floor, Icon::Floor),
                            (Layer::Roof, Icon::Roof),
                            (Layer::Wall, Icon::Wall),
                            (Layer::Scenery, Icon::Scenery),
                            (Layer::Item, Icon::Item),
                            (Layer::Critter, Icon::Critter),
                        ] {
                            let on = self.layers.contains(layer);
                            let r = icon_button(ui, icon, on, true);
                            note(
                                &r,
                                &format!(
                                    "{} : {}",
                                    crate::i18n::t("Afficher ou masquer"),
                                    crate::i18n::t(layer.label())
                                ),
                            );
                            if r.clicked() {
                                self.layers.set(layer, !on);
                            }
                        }
                        ui.separator();
                        let r = icon_button(ui, Icon::Scripted, self.show_scripted, true);
                        note(&r, crate::i18n::t("Surligner les objets avec un script"));
                        if r.clicked() {
                            self.show_scripted = !self.show_scripted;
                        }
                    });
                });
            });
        if let Some(h) = hint {
            self.hint_next = h;
        }
    }
}

impl MapperApp {
    /// Fenêtre « Barre d'outils » : montrer ou masquer chaque outil et le monter ou le descendre.
    pub(crate) fn toolbar_window(&mut self, ctx: &egui::Context) {
        if !self.toolbar_dialog {
            return;
        }
        let mut open = true;
        let mut tools = self.toolbar_tools.clone();
        egui::Window::new(crate::i18n::t("Barre d'outils"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_width(300.0);
                ui.label(crate::i18n::t(
                    "Cochez les outils à montrer ; les flèches changent leur ordre.",
                ));
                ui.add_space(4.0);
                let n = tools.len();
                let mut swap: Option<(usize, usize)> = None;
                for i in 0..n {
                    let tool = tools[i].0;
                    let name = Tool::ALL
                        .iter()
                        .find(|(t, _)| *t == tool)
                        .map_or("", |(_, n)| *n);
                    ui.horizontal(|ui| {
                        // L'outil de sélection reste toujours là.
                        ui.add_enabled(
                            tool != Tool::Select,
                            egui::Checkbox::without_text(&mut tools[i].1),
                        );
                        let (rect, _) = ui.allocate_exact_size(BUTTON * 0.7, egui::Sense::hover());
                        crate::icons::paint(
                            ui.painter(),
                            rect,
                            tool.icon(),
                            ui.visuals().text_color(),
                        );
                        ui.label(crate::i18n::t(name));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add_enabled(i + 1 < n, egui::Button::new("↓")).clicked() {
                                swap = Some((i, i + 1));
                            }
                            if ui.add_enabled(i > 0, egui::Button::new("↑")).clicked() {
                                swap = Some((i, i - 1));
                            }
                        });
                    });
                }
                if let Some((a, b)) = swap {
                    tools.swap(a, b);
                }
                ui.add_space(6.0);
                if ui
                    .button(crate::i18n::t("Rétablir la barre d'outils"))
                    .clicked()
                {
                    tools = default_tools();
                }
            });
        self.toolbar_tools = tools;
        self.toolbar_dialog = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_layout_round_trips_and_repairs_itself() {
        let mut tools = default_tools();
        tools.swap(1, 4);
        tools[2].1 = false;
        assert_eq!(tools_from_string(&tools_to_string(&tools)), tools);
        // Texte abîmé : inconnus ignorés, doublons écartés, manquants ajoutés, la sélection reste visible.
        let fixed = tools_from_string("-select,bogus,floor,floor,-wall");
        assert_eq!(fixed.len(), Tool::ALL.len());
        assert!(fixed[0] == (Tool::Select, true) && fixed[1] == (Tool::Floor, true));
        assert!(fixed.contains(&(Tool::Wall, false)));
    }
}
