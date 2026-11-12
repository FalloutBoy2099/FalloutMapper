//! Journal : mémorise les messages successifs de la barre d'état, avec l'heure, dans une fenêtre.

use crate::app::MapperApp;
use eframe::egui;

/// Nombre maximal de messages conservés.
const MAX_ENTRIES: usize = 500;

#[derive(Default)]
pub(crate) struct LogState {
    pub(crate) open: bool,
    /// Date et heure locales de chaque message, et l'hexagone concerné quand on le connaît (clic : aller dessus).
    entries: Vec<(chrono::DateTime<chrono::Local>, String, Option<u32>)>,
    last: String,
}

impl LogState {
    /// Enregistre `status` s'il diffère du dernier message vu.
    fn observe(&mut self, status: &str) {
        if status == self.last {
            return;
        }
        self.last = status.to_string();
        if status.is_empty() {
            return;
        }
        self.push(status.to_string(), None);
    }

    /// Ajoute un message, avec l'hexagone où l'action a eu lieu s'il y en a un.
    pub(crate) fn push(&mut self, text: String, at: Option<u32>) {
        self.entries.push((chrono::Local::now(), text, at));
        if self.entries.len() > MAX_ENTRIES {
            self.entries.remove(0);
        }
    }
}

impl MapperApp {
    pub(crate) fn log_window(&mut self, ctx: &egui::Context) {
        self.log.observe(&self.status);
        if !self.log.open {
            return;
        }
        let mut open = true;
        let mut clear = false;
        let mut goto: Option<u32> = None;
        egui::Window::new(crate::i18n::t("Journal"))
            .open(&mut open)
            .default_size([420.0, 300.0])
            .show(ctx, |ui| {
                if ui.button(crate::i18n::t("Effacer")).clicked() {
                    clear = true;
                }
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for (at, text, hex) in &self.log.entries {
                            ui.horizontal_wrapped(|ui| {
                                ui.weak(at.format("%d/%m %H:%M:%S").to_string());
                                ui.label(text);
                                if let Some(h) = hex
                                    && ui
                                        .small_button(format!("{} {h}", crate::i18n::t("hex")))
                                        .on_hover_text(crate::i18n::t(
                                            "Centrer la vue sur cet hexagone",
                                        ))
                                        .clicked()
                                {
                                    goto = Some(*h);
                                }
                            });
                        }
                    });
            });
        if clear {
            self.log.entries.clear();
        }
        if let Some(h) = goto {
            let (x, y) = fo_core::geom::hex_to_screen(h);
            self.camera.center = eframe::egui::Pos2::new(x as f32, y as f32);
            self.needs_fit = false;
            self.start_view = false;
        }
        self.log.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_and_empty_messages_are_skipped() {
        let mut l = LogState::default();
        l.observe("a");
        l.observe("a");
        l.observe("");
        l.observe("a");
        l.observe("b");
        let texts: Vec<_> = l.entries.iter().map(|e| e.1.as_str()).collect();
        assert_eq!(texts, ["a", "a", "b"]);
        l.push("posé".into(), Some(20_100));
        assert_eq!(l.entries.last().and_then(|e| e.2), Some(20_100));
    }
}
