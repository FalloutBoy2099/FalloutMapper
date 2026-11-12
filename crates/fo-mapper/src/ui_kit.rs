//! Éléments d'interface partagés : thème, pied de fenêtre (séparateur + boutons alignés à droite), boutons principaux.

use eframe::egui::{self, Color32, RichText, Vec2};

/// Commandes flottantes de la carte : au-dessus du canevas, sous les fenêtres et les menus.
pub(crate) fn canvas_overlay(
    ctx: &egui::Context,
    parent: egui::LayerId,
    id: egui::Id,
) -> egui::Area {
    ctx.set_sublayer(parent, egui::LayerId::new(parent.order, id));
    egui::Area::new(id).order(parent.order)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_controls_stay_below_windows_and_their_popups() {
        let ctx = egui::Context::default();
        let control_id = egui::Id::unique("test_canvas_controls");
        let mut control = None;
        let mut window = None;
        let mut popup = None;
        for show_window in [false, true, false] {
            for _ in 0..4 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        ..Default::default()
                    },
                    |root| {
                        if show_window {
                            let shown = egui::Window::new("Test window")
                                .fixed_pos(egui::pos2(60.0, 60.0))
                                .fixed_size(egui::vec2(300.0, 200.0))
                                .show(&ctx, |ui| {
                                    let button = ui.button("Menu");
                                    popup = egui::Popup::menu(&button)
                                        .open(true)
                                        .show(|ui| {
                                            ui.set_min_size(egui::vec2(180.0, 100.0));
                                            ui.label("Menu content");
                                        })
                                        .map(|r| r.response);
                                })
                                .unwrap();
                            window = Some(shown.response);
                        }
                        egui::CentralPanel::default().show(root, |ui| {
                            let parent = ui.layer_id();
                            let shown = canvas_overlay(&ctx, parent, control_id)
                                .fixed_pos(egui::pos2(50.0, 50.0))
                                .show(&ctx, |ui| {
                                    ui.allocate_exact_size(
                                        egui::vec2(350.0, 260.0),
                                        egui::Sense::click(),
                                    );
                                });
                            // Une interaction ne doit pas faire remonter les commandes au-dessus d'une fenêtre.
                            ctx.move_to_top(shown.response.layer_id);
                            control = Some(shown.response);
                        });
                    },
                );
                output.textures_delta.clear();
            }
            let control = control.as_ref().unwrap();
            if show_window {
                let window = window.as_ref().unwrap();
                let header = window.rect.min + egui::vec2(80.0, 10.0);
                assert!(control.rect.contains(header));
                assert_eq!(ctx.layer_id_at(header), Some(window.layer_id));
                let popup = popup.as_ref().expect("menu ouvert");
                assert_eq!(ctx.layer_id_at(popup.rect.center()), Some(popup.layer_id));
            } else {
                assert_eq!(
                    ctx.layer_id_at(control.rect.center()),
                    Some(control.layer_id)
                );
            }
        }
    }
}

/// Rôle d'un bouton de pied de fenêtre.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Role {
    /// Action recommandée : mise en avant.
    Primary,
    Normal,
    /// Action destructrice.
    Danger,
}

pub(crate) struct FooterButton<'a> {
    pub label: &'a str,
    pub role: Role,
    pub enabled: bool,
}

impl<'a> FooterButton<'a> {
    pub(crate) fn primary(label: &'a str) -> Self {
        Self {
            label,
            role: Role::Primary,
            enabled: true,
        }
    }
    pub(crate) fn normal(label: &'a str) -> Self {
        Self {
            label,
            role: Role::Normal,
            enabled: true,
        }
    }
    pub(crate) fn danger(label: &'a str) -> Self {
        Self {
            label,
            role: Role::Danger,
            enabled: true,
        }
    }
    pub(crate) fn enabled(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }
}

/// Séparateur puis rangée de boutons qui se partagent toute la largeur, hauteur confortable. Les boutons sont donnés
/// dans l'ordre de lecture (de gauche à droite : le bouton principal en dernier). Renvoie l'indice du bouton cliqué.
pub(crate) fn footer(ui: &mut egui::Ui, buttons: &[FooterButton]) -> Option<usize> {
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);
    let mut clicked = None;
    let n = buttons.len().max(1) as f32;
    let gap = ui.spacing().item_spacing.x;
    let width = ((ui.available_width() - gap * (n - 1.0)) / n).max(80.0);
    ui.horizontal(|ui| {
        for (i, b) in buttons.iter().enumerate() {
            let text = RichText::new(b.label);
            let button = match b.role {
                Role::Primary => egui::Button::new(text.strong().color(Color32::WHITE))
                    .fill(ui.visuals().selection.bg_fill),
                Role::Danger => egui::Button::new(text.color(Color32::WHITE))
                    .fill(Color32::from_rgb(140, 50, 50)),
                Role::Normal => egui::Button::new(text),
            };
            if ui
                .add_enabled(b.enabled, button.min_size(Vec2::new(width, 32.0)))
                .clicked()
            {
                clicked = Some(i);
            }
        }
    });
    ui.add_space(2.0);
    clicked
}

/// Thème : coins arrondis, ombres, boutons plus hauts et mieux espacés.
pub(crate) fn apply_theme(ctx: &egui::Context) {
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(theme, tweak);
    }
}

fn tweak(s: &mut egui::Style) {
    {
        s.spacing.interact_size.y = 24.0;
        s.spacing.scroll.handle_min_length = 48.0;
        s.spacing.scroll.bar_width = 12.0;
        s.spacing.button_padding = Vec2::new(8.0, 4.0);
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.window_margin = egui::Margin::same(12);
        s.spacing.menu_margin = egui::Margin::same(8);
        s.visuals.window_corner_radius = egui::CornerRadius::same(10);
        s.visuals.menu_corner_radius = egui::CornerRadius::same(8);
        let r = egui::CornerRadius::same(6);
        s.visuals.widgets.noninteractive.corner_radius = r;
        s.visuals.widgets.inactive.corner_radius = r;
        s.visuals.widgets.hovered.corner_radius = r;
        s.visuals.widgets.active.corner_radius = r;
        s.visuals.widgets.open.corner_radius = r;
        s.visuals.window_shadow = egui::Shadow {
            offset: [0, 6],
            blur: 24,
            spread: 2,
            color: Color32::from_black_alpha(140),
        };
        s.visuals.window_stroke = egui::Stroke::new(1.0, Color32::from_gray(70));
    }
}
