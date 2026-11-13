//! Options de l'application : échelle de l'interface, miniatures des palettes, ouverture du dernier dossier de jeu.

use crate::app::MapperApp;
use crate::i18n::{self, Lang};
use eframe::egui;

const KEY_SCALE: &str = "pref_ui_scale_v2";
const KEY_THUMB: &str = "pref_thumb_size";
const KEY_NAMES: &str = "pref_show_names";
const KEY_LAST_GAME: &str = "pref_open_last_game";
const KEY_LANG: &str = "pref_lang";
const KEY_THEME: &str = "pref_theme";
const KEY_GRID_COLOR: &str = "pref_grid_color";
const KEY_GRID_OPACITY: &str = "pref_grid_opacity";

/// Grille hexagonale : blanc à 11 % d'opacité par défaut.
pub(crate) const DEFAULT_GRID_COLOR: [u8; 3] = [255, 255, 255];
pub(crate) const DEFAULT_GRID_OPACITY: f32 = 0.11;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum ThemeChoice {
    #[default]
    Dark,
    Light,
    System,
}

impl ThemeChoice {
    const ALL: [ThemeChoice; 3] = [ThemeChoice::Dark, ThemeChoice::Light, ThemeChoice::System];

    fn code(self) -> &'static str {
        match self {
            ThemeChoice::Dark => "dark",
            ThemeChoice::Light => "light",
            ThemeChoice::System => "system",
        }
    }

    fn label(self) -> &'static str {
        match self {
            ThemeChoice::Dark => "Sombre",
            ThemeChoice::Light => "Clair",
            ThemeChoice::System => "Système",
        }
    }

    pub(crate) fn preference(self) -> egui::ThemePreference {
        match self {
            ThemeChoice::Dark => egui::ThemePreference::Dark,
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::System => egui::ThemePreference::System,
        }
    }
}

/// Actions dont la touche est modifiable : les quatre premières se font touche seule, les autres avec Ctrl.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum KeyAction {
    Fit,
    Home,
    Rotate,
    Pipette,
    Undo,
    Redo,
    Save,
    Duplicate,
    SelectAll,
    Deselect,
    NewMap,
    CloseMap,
    Thumbnails,
    Find,
    Check,
    Preview,
    History,
    Options,
    LockSelection,
}

impl KeyAction {
    pub(crate) const COUNT: usize = 19;
    pub(crate) const ALL: [KeyAction; Self::COUNT] = [
        KeyAction::Fit,
        KeyAction::Home,
        KeyAction::Rotate,
        KeyAction::Pipette,
        KeyAction::Undo,
        KeyAction::Redo,
        KeyAction::Save,
        KeyAction::Duplicate,
        KeyAction::SelectAll,
        KeyAction::Deselect,
        KeyAction::NewMap,
        KeyAction::CloseMap,
        KeyAction::Thumbnails,
        KeyAction::Find,
        KeyAction::Check,
        KeyAction::Preview,
        KeyAction::History,
        KeyAction::Options,
        KeyAction::LockSelection,
    ];

    fn index(self) -> usize {
        self as usize
    }

    /// Se fait avec Ctrl (Commande sur Mac).
    pub(crate) fn with_ctrl(self) -> bool {
        self.index() >= 4 && self != KeyAction::LockSelection
    }

    fn pref_key(self) -> &'static str {
        [
            "pref_key_fit",
            "pref_key_home",
            "pref_key_rotate",
            "pref_key_pipette",
            "pref_key_undo",
            "pref_key_redo",
            "pref_key_save",
            "pref_key_duplicate",
            "pref_key_select_all",
            "pref_key_deselect",
            "pref_key_new_map",
            "pref_key_close_map",
            "pref_key_thumbnails",
            "pref_key_find",
            "pref_key_check",
            "pref_key_preview",
            "pref_key_history",
            "pref_key_options",
            "pref_key_lock_selection",
        ][self.index()]
    }

    fn label(self) -> &'static str {
        [
            "Ajuster la vue",
            "Centrer sur le départ",
            "Tourner la sélection",
            "Pipette",
            "Annuler",
            "Rétablir",
            "Enregistrer",
            "Dupliquer la sélection",
            "Tout sélectionner",
            "Tout désélectionner",
            "Nouvelle carte",
            "Fermer la carte",
            "Miniatures des cartes",
            "Rechercher un objet",
            "Vérifier la carte",
            "Aperçu du jeu",
            "Historique",
            "Options",
            "Verrouiller la sélection",
        ][self.index()]
    }

    fn default_key(self) -> egui::Key {
        use egui::Key::*;
        [F, Home, R, P, Z, Y, S, J, A, D, N, W, B, E, K, G, H, Comma, L][self.index()]
    }
}

/// Onglets de la fenêtre Options.
const OPTION_TABS: [&str; 4] = ["Interface", "Palettes", "Grille", "Raccourcis clavier"];

/// Tailles de miniatures proposées pour les palettes (sols, toits, objets).
pub(crate) const THUMB_SIZES: [u32; 5] = [16, 24, 32, 48, 64];
/// Interface agrandie de 15 % par défaut.
pub(crate) const DEFAULT_SCALE: f32 = 1.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Prefs {
    pub ui_scale: f32,
    pub thumb_size: u32,
    pub show_names: bool,
    /// Au démarrage : rouvrir le dernier dossier de Fallout 2 (et la dernière carte).
    pub open_last_game: bool,
    pub lang: Lang,
    pub theme: ThemeChoice,
    /// Touche de chaque `KeyAction` (dans l'ordre de `KeyAction::ALL`).
    pub keys: [egui::Key; KeyAction::COUNT],
    /// Couleur (RVB) et opacité (0 à 1) de la grille hexagonale.
    pub grid_color: [u8; 3],
    pub grid_opacity: f32,
}

/// « RRGGBB » (avec ou sans #) → couleur.
fn parse_hex_color(v: &str) -> Option<[u8; 3]> {
    let v = v.trim().trim_start_matches('#');
    (v.len() == 6)
        .then(|| u32::from_str_radix(v, 16).ok())
        .flatten()
        .map(|n| [(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

impl Prefs {
    /// Couleur de trait de la grille hexagonale.
    pub(crate) fn grid_stroke_color(&self) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(
            self.grid_color[0],
            self.grid_color[1],
            self.grid_color[2],
            (self.grid_opacity.clamp(0.0, 1.0) * 255.0) as u8,
        )
    }

    /// Raccourci tel qu'on l'affiche dans un menu (« Ctrl+N », « F »).
    pub(crate) fn shortcut_text(&self, a: KeyAction) -> String {
        let key = self.key(a).symbol_or_name();
        if a.with_ctrl() {
            format!("Ctrl+{key}")
        } else {
            key.to_string()
        }
    }

    pub(crate) fn key(&self, a: KeyAction) -> egui::Key {
        self.keys[a.index()]
    }
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            ui_scale: DEFAULT_SCALE,
            thumb_size: 32,
            show_names: true,
            open_last_game: true,
            lang: Lang::Fr,
            theme: ThemeChoice::Dark,
            keys: KeyAction::ALL.map(KeyAction::default_key),
            grid_color: DEFAULT_GRID_COLOR,
            grid_opacity: DEFAULT_GRID_OPACITY,
        }
    }
}

impl Prefs {
    pub(crate) fn load(get: impl Fn(&str) -> Option<String>) -> Self {
        let d = Self::default();
        let thumb = get(KEY_THUMB)
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|v| THUMB_SIZES.contains(v));
        Self {
            ui_scale: get(KEY_SCALE)
                .and_then(|v| v.parse::<f32>().ok())
                .map_or(d.ui_scale, |v| v.clamp(0.8, 2.0)),
            thumb_size: thumb.unwrap_or(d.thumb_size),
            show_names: get(KEY_NAMES).map_or(d.show_names, |v| v != "0"),
            open_last_game: get(KEY_LAST_GAME).map_or(d.open_last_game, |v| v != "0"),
            lang: get(KEY_LANG)
                .and_then(|v| Lang::from_code(&v))
                .unwrap_or(d.lang),
            grid_color: get(KEY_GRID_COLOR)
                .and_then(|v| parse_hex_color(&v))
                .unwrap_or(d.grid_color),
            grid_opacity: get(KEY_GRID_OPACITY)
                .and_then(|v| v.parse::<f32>().ok())
                .map_or(d.grid_opacity, |v| v.clamp(0.02, 1.0)),
            theme: get(KEY_THEME)
                .and_then(|v| ThemeChoice::ALL.into_iter().find(|t| t.code() == v))
                .unwrap_or(d.theme),
            keys: KeyAction::ALL.map(|a| {
                get(a.pref_key())
                    .and_then(|v| egui::Key::from_name(&v))
                    .unwrap_or(a.default_key())
            }),
        }
    }

    pub(crate) fn save(&self, set: &mut dyn FnMut(&str, String)) {
        set(KEY_SCALE, self.ui_scale.to_string());
        set(KEY_THUMB, self.thumb_size.to_string());
        set(
            KEY_NAMES,
            if self.show_names { "1" } else { "0" }.to_string(),
        );
        set(
            KEY_LAST_GAME,
            if self.open_last_game { "1" } else { "0" }.to_string(),
        );
        set(KEY_LANG, self.lang.code().to_string());
        set(KEY_THEME, self.theme.code().to_string());
        set(
            KEY_GRID_COLOR,
            format!(
                "{:02X}{:02X}{:02X}",
                self.grid_color[0], self.grid_color[1], self.grid_color[2]
            ),
        );
        set(KEY_GRID_OPACITY, self.grid_opacity.to_string());
        for a in KeyAction::ALL {
            set(a.pref_key(), self.key(a).name().to_string());
        }
    }
}

impl MapperApp {
    pub(crate) fn prefs_window(&mut self, ctx: &egui::Context) {
        if !self.prefs_open {
            return;
        }
        let mut open = true;
        let mut close = false;
        let mut prefs = self.prefs.clone();
        let mut tab = self.prefs_tab;
        // Fenêtre modale : le reste de l'application est grisé et ne réagit pas tant qu'elle est ouverte (Échap ou un clic à côté la ferme).
        let modal = egui::Modal::new(egui::Id::unique("prefs_modal")).show(ctx, |ui| {
            ui.set_width(720.0);
            ui.vertical_centered(|ui| {
                ui.heading(crate::i18n::t("Options"));
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                for (i, name) in OPTION_TABS.iter().enumerate() {
                    ui.selectable_value(&mut tab, i, crate::i18n::t(name));
                }
            });
            ui.separator();
            // Hauteur fixe : la fenêtre ne change pas de taille d'un onglet à l'autre.
            ui.allocate_ui(egui::vec2(ui.available_width(), 380.0), |ui| {
                ui.set_min_height(380.0);
                match tab {
                    0 => {
                        ui.spacing_mut().item_spacing.y = 12.0;
                        ui.horizontal_wrapped(|ui| {
                            ui.label(crate::i18n::t("Langue"));
                            for l in Lang::ALL {
                                ui.selectable_value(&mut prefs.lang, l, l.name());
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(crate::i18n::t("Thème"));
                            for th in ThemeChoice::ALL {
                                ui.selectable_value(&mut prefs.theme, th, i18n::t(th.label()));
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(crate::i18n::t("Échelle"));
                            ui.add(egui::Slider::new(&mut prefs.ui_scale, 0.8..=2.0).step_by(0.05).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));
                            if ui.button(crate::i18n::t("Par défaut")).clicked() {
                                prefs.ui_scale = DEFAULT_SCALE;
                            }
                        });
                        ui.separator();
                        ui.heading(crate::i18n::t("Démarrage"));
                        ui.checkbox(&mut prefs.open_last_game, crate::i18n::t("Rouvrir le dernier dossier de Fallout 2 (et la dernière carte)"));
                    }
                    1 => {
                        ui.horizontal(|ui| {
                            ui.label(crate::i18n::t("Taille des miniatures"));
                            for s in THUMB_SIZES {
                                ui.selectable_value(&mut prefs.thumb_size, s, format!("{s}×{s}"));
                            }
                        });
                        ui.checkbox(&mut prefs.show_names, crate::i18n::t("Afficher le nom à côté des miniatures"));
                    }
                    2 => {
                        ui.label(crate::i18n::t("Grille hexagonale (Affichage → Grille hexagonale)"));
                        ui.horizontal(|ui| {
                            ui.label(crate::i18n::t("Couleur"));
                            ui.color_edit_button_srgb(&mut prefs.grid_color);
                        });
                        ui.horizontal(|ui| {
                            ui.label(crate::i18n::t("Opacité"));
                            ui.add(egui::Slider::new(&mut prefs.grid_opacity, 0.02..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));
                        });
                        if ui.button(crate::i18n::t("Par défaut")).clicked() {
                            prefs.grid_color = DEFAULT_GRID_COLOR;
                            prefs.grid_opacity = DEFAULT_GRID_OPACITY;
                        }
                        // Aperçu : quelques hexagones de la grille sur fond sombre.
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(360.0), 90.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_gray(30));
                        let stroke = egui::Stroke::new(1.0, prefs.grid_stroke_color());
                        for row in 0..4 {
                            for col in 0..9 {
                                let c = rect.min + egui::vec2(24.0 + col as f32 * 38.0 + if row % 2 == 1 { 19.0 } else { 0.0 }, 14.0 + row as f32 * 20.0);
                                let pts: Vec<egui::Pos2> = crate::edit_ui::HEX_OUTLINE.iter().map(|(dx, dy)| c + egui::vec2(*dx * 0.6, *dy * 0.9)).collect();
                                ui.painter().add(egui::Shape::closed_line(pts, stroke));
                            }
                        }
                    }
                    _ => {
                        ui.weak(crate::i18n::t("Cliquez sur un raccourci, puis appuyez sur la nouvelle touche (Ctrl s'ajoute tout seul quand il est indiqué)."));
                        let capturing = self.rebinding;
                        let pressed = ui.input(|i| {
                            if i.modifiers.command || i.modifiers.alt {
                                return None;
                            }
                            i.events.iter().find_map(|e| match e {
                                egui::Event::Key { key, pressed: true, repeat: false, .. } => Some(*key),
                                _ => None,
                            })
                        });
                        egui::Grid::new("shortcut_grid").num_columns(4).spacing([16.0, 6.0]).show(ui, |ui| {
                            for pair in KeyAction::ALL.chunks(2) {
                                for &a in pair {
                                    ui.label(crate::i18n::t(a.label()));
                                    if capturing == Some(a) {
                                        if let Some(k) = pressed.filter(|k| *k != egui::Key::Escape) {
                                            // Une touche déjà prise par une autre action du même genre (avec ou sans Ctrl) est échangée.
                                            let old = prefs.keys[a.index()];
                                            if let Some(other) = KeyAction::ALL.iter().find(|o| **o != a && o.with_ctrl() == a.with_ctrl() && prefs.key(**o) == k) {
                                                prefs.keys[other.index()] = old;
                                            }
                                            prefs.keys[a.index()] = k;
                                        }
                                        if pressed.is_some() {
                                            self.rebinding = None;
                                        } else {
                                            ui.add_enabled(false, egui::Button::new(crate::i18n::t("Appuyez sur une touche…")));
                                        }
                                    } else {
                                        let key_name = prefs.key(a).name();
                                        let text = if a.with_ctrl() { format!("Ctrl + {key_name}") } else { key_name.to_string() };
                                        if ui.button(text).clicked() {
                                            self.rebinding = Some(a);
                                        }
                                    }
                                }
                                ui.end_row();
                            }
                        });
                        ui.add_space(6.0);
                        if ui.button(crate::i18n::t("Touches par défaut")).clicked() {
                            prefs.keys = KeyAction::ALL.map(KeyAction::default_key);
                        }
                    }
                }
            });
            if crate::ui_kit::footer(ui, &[crate::ui_kit::FooterButton::primary(crate::i18n::t("Fermer"))]).is_some() {
                close = true;
            }
        });
        if modal.should_close() {
            open = false;
        }
        self.prefs_tab = tab;
        self.prefs = prefs;
        self.prefs_open = open && !close;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn defaults_are_15_percent_bigger_and_32px_thumbnails() {
        let p = Prefs::default();
        assert_eq!(
            (p.ui_scale, p.thumb_size, p.show_names, p.open_last_game),
            (1.0, 32, true, true)
        );
    }

    #[test]
    fn round_trips_and_rejects_bad_values() {
        let p = Prefs {
            ui_scale: 1.4,
            thumb_size: 48,
            show_names: false,
            open_last_game: false,
            lang: Lang::En,
            theme: ThemeChoice::Light,
            keys: KeyAction::ALL.map(|a| {
                if a == KeyAction::Fit {
                    egui::Key::G
                } else {
                    a.default_key()
                }
            }),
            grid_color: [10, 200, 90],
            grid_opacity: 0.4,
        };
        let mut store: HashMap<String, String> = HashMap::new();
        p.save(&mut |k, v| {
            store.insert(k.to_string(), v);
        });
        assert_eq!(Prefs::load(|k| store.get(k).cloned()), p);
        store.insert(KEY_THUMB.into(), "20".into());
        store.insert(KEY_SCALE.into(), "9".into());
        let q = Prefs::load(|k| store.get(k).cloned());
        assert_eq!(q.thumb_size, 32, "taille inconnue : valeur par défaut");
        assert_eq!(q.ui_scale, 2.0, "échelle bornée");
    }
}
