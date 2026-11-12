//! Fenêtre « Comparer avec… » : met la carte ouverte en regard de sa version enregistrée (ou de sa copie `.bak`), liste ce qui
//! a changé et le surligne sur la carte (vert : ajouté, rouge : retiré, orange : déplacé, bleu : modifié).

use crate::app::MapperApp;
use crate::i18n::t;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use fo_core::diff::{ChangeKind, MapDiff, ObjectChange, diff_maps};
use fo_core::{Map, geom, read_map};

/// Contre quoi on compare.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Against {
    /// Version enregistrée dans les correctifs ou les archives.
    #[default]
    Saved,
    /// Copie `.map.bak` faite au premier enregistrement.
    Backup,
}

#[derive(Default)]
pub(crate) struct DiffState {
    pub open: bool,
    pub(crate) against: Against,
    diff: Option<MapDiff>,
    /// Carte comparée et ce contre quoi.
    label: String,
    error: Option<String>,
    /// Surligner les changements sur la carte.
    pub highlight: bool,
}

fn color(kind: ChangeKind) -> Color32 {
    match kind {
        ChangeKind::Added => Color32::from_rgb(80, 220, 110),
        ChangeKind::Removed => Color32::from_rgb(240, 80, 80),
        ChangeKind::Moved => Color32::from_rgb(255, 170, 60),
        ChangeKind::Modified => Color32::from_rgb(90, 170, 255),
    }
}

fn kind_label(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Added => "Ajouté",
        ChangeKind::Removed => "Retiré",
        ChangeKind::Moved => "Déplacé",
        ChangeKind::Modified => "Modifié",
    }
}

impl DiffState {
    /// Résumé chiffré de la dernière comparaison (ajoutés, retirés, déplacés, modifiés, cases).
    pub(crate) fn diff_summary(&self) -> String {
        self.diff.as_ref().map_or_else(String::new, |d| {
            format!(
                "+{} -{} déplacés {} modifiés {} cases {}",
                d.count(ChangeKind::Added),
                d.count(ChangeKind::Removed),
                d.count(ChangeKind::Moved),
                d.count(ChangeKind::Modified),
                d.tiles.len()
            )
        })
    }
}

impl MapperApp {
    /// Version de la carte avec laquelle comparer.
    fn load_reference(&self, against: Against) -> Result<Map, String> {
        let (Some(cur), Some(g)) = (self.current.as_ref(), self.game.as_ref()) else {
            return Err(t("Aucune carte.").into());
        };
        let bytes = match against {
            Against::Saved => g
                .data
                .read_map_bytes(&cur.file)
                .map_err(|e| e.to_string())?,
            Against::Backup => {
                let patches = g.data.root.join(&g.data.config.master_patches);
                let base = cur.file.to_ascii_lowercase();
                let found = ["maps", "MAPS", "Maps"]
                    .iter()
                    .map(|d| patches.join(d).join(format!("{base}.map.bak")))
                    .find(|p| p.is_file());
                let path = found.ok_or_else(|| {
                    t("Pas de copie .bak : la carte n'a jamais été enregistrée par l'éditeur.")
                        .to_string()
                })?;
                std::fs::read(&path).map_err(|e| format!("{} : {e}", path.display()))?
            }
        };
        read_map(&bytes, &*g.data).map_err(|e| e.to_string())
    }

    pub(crate) fn run_diff(&mut self) {
        match self.load_reference(self.diff.against) {
            Ok(reference) => {
                let Some(cur) = self.current.as_ref() else {
                    return;
                };
                let d = diff_maps(&reference, &cur.map);
                self.status = if d.is_empty() {
                    t("Aucune différence.").into()
                } else {
                    format!("{} {}", d.objects.len() + d.tiles.len(), t("différence(s)"))
                };
                self.diff.label = format!(
                    "{} ({})",
                    cur.file,
                    t(if self.diff.against == Against::Saved {
                        "version enregistrée"
                    } else {
                        "copie .bak"
                    })
                );
                self.diff.diff = Some(d);
                self.diff.error = None;
                self.diff.highlight = true;
            }
            Err(e) => {
                self.diff.diff = None;
                self.diff.error = Some(e);
            }
        }
    }

    fn goto_change(&mut self, c: &ObjectChange) {
        let Some(hex) =
            c.to.or(c.from)
                .filter(|h| (0..geom::HEX_COUNT as i32).contains(h))
        else {
            return;
        };
        if c.elevation != self.elevation
            && self
                .current
                .as_ref()
                .is_some_and(|cur| cur.scenes.get(c.elevation).is_some_and(Option::is_some))
        {
            self.elevation = c.elevation;
        }
        if c.kind != ChangeKind::Removed {
            self.selection.set_object(Some(fo_scene::ObjectRef {
                elevation: c.elevation as u8,
                index: c.index as u32,
            }));
        }
        let (x, y) = geom::hex_to_screen(hex as u32);
        self.camera.center = Pos2::new(x as f32, y as f32);
    }

    /// Surligne les changements de l'élévation affichée : un losange sur chaque objet, un trait de l'ancienne à la nouvelle position.
    pub(crate) fn draw_diff(&self, painter: &egui::Painter, view: Rect) {
        let Some(d) = self.diff.diff.as_ref().filter(|_| self.diff.highlight) else {
            return;
        };
        let cam = &self.camera;
        let to_screen = |hex: i32| {
            let (x, y) = geom::hex_to_screen(hex.max(0) as u32);
            cam.world_to_screen(view, Pos2::new(x as f32, y as f32))
        };
        let r = 9.0 * cam.zoom.clamp(0.6, 1.6);
        for c in d.objects.iter().filter(|c| c.elevation == self.elevation) {
            let col = color(c.kind);
            if let (ChangeKind::Moved, Some(a), Some(b)) = (c.kind, c.from, c.to) {
                painter.line_segment(
                    [to_screen(a), to_screen(b)],
                    Stroke::new(1.2, col.gamma_multiply(0.7)),
                );
                painter.circle_stroke(
                    to_screen(a),
                    r * 0.5,
                    Stroke::new(1.2, col.gamma_multiply(0.7)),
                );
            }
            let Some(h) =
                c.to.or(c.from)
                    .filter(|h| (0..geom::HEX_COUNT as i32).contains(h))
            else {
                continue;
            };
            let p = to_screen(h);
            if !view.expand(r).contains(p) {
                continue;
            }
            painter.add(egui::Shape::closed_line(
                vec![
                    p + Vec2::new(0.0, -r),
                    p + Vec2::new(r, 0.0),
                    p + Vec2::new(0.0, r),
                    p + Vec2::new(-r, 0.0),
                ],
                Stroke::new(2.0, col),
            ));
        }
        // Cases de sol et de toit modifiées : un petit point au centre de la case.
        for tc in d.tiles.iter().filter(|c| c.elevation == self.elevation) {
            let (x, y) = geom::square_to_screen(tc.square as u32);
            let dy = if tc.layer == fo_core::map::TileLayer::Roof {
                geom::ROOF_OFFSET_Y
            } else {
                0
            };
            let p = cam.world_to_screen(view, Pos2::new(x as f32 + 40.0, (y + dy) as f32 + 18.0));
            if view.contains(p) {
                painter.circle_filled(p, 2.5, color(ChangeKind::Modified));
            }
        }
    }

    pub(crate) fn diff_window(&mut self, ctx: &egui::Context) {
        if !self.diff.open {
            return;
        }
        let mut open = true;
        let mut run = false;
        let mut goto: Option<ObjectChange> = None;
        egui::Window::new(t("Comparer avec une version")).open(&mut open).order(egui::Order::Middle).collapsible(false).default_size([520.0, 420.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.diff.against, Against::Saved, t("Version enregistrée"));
                ui.selectable_value(&mut self.diff.against, Against::Backup, t("Copie .bak"));
                if ui.button(t("Comparer")).clicked() {
                    run = true;
                }
            });
            ui.weak(t("La version enregistrée est celle du disque : elle ne contient pas vos modifications en cours."));
            if let Some(e) = &self.diff.error {
                ui.colored_label(Color32::LIGHT_RED, e);
            }
            if let Some(d) = &self.diff.diff {
                ui.separator();
                ui.label(egui::RichText::new(&self.diff.label).strong());
                if d.is_empty() {
                    ui.label(t("Aucune différence."));
                } else {
                    ui.horizontal_wrapped(|ui| {
                        for k in [ChangeKind::Added, ChangeKind::Removed, ChangeKind::Moved, ChangeKind::Modified] {
                            ui.colored_label(color(k), format!("{} : {}", t(kind_label(k)), d.count(k)));
                        }
                        ui.label(format!("{} {}", d.tiles.len(), t("case(s) de sol ou de toit")));
                    });
                    ui.checkbox(&mut self.diff.highlight, t("Surligner sur la carte"));
                    ui.separator();
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        for c in d.objects.iter().take(2000) {
                            let name = self.game.as_ref().and_then(|g| g.data.proto_entry(c.pid)).map(|p| p.name).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("{:08X}", c.pid));
                            let place = match (c.from, c.to) {
                                (Some(a), Some(b)) if a != b => format!("{a} → {b}"),
                                (_, Some(b)) => b.to_string(),
                                (Some(a), None) => a.to_string(),
                                _ => String::new(),
                            };
                            let label = format!("{} — {name} (élév. {}, hex {place})", t(kind_label(c.kind)), c.elevation);
                            if ui.add(egui::Button::new(egui::RichText::new(label).color(color(c.kind))).frame(false)).clicked() {
                                goto = Some(c.clone());
                            }
                        }
                        if d.objects.len() > 2000 {
                            ui.weak(format!("… {} {}", d.objects.len() - 2000, t("autres")));
                        }
                    });
                }
            }
        });
        if run {
            self.run_diff();
        }
        if let Some(c) = goto {
            self.goto_change(&c);
        }
        self.diff.open = open;
    }
}
