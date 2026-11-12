//! Fenêtre « Rechercher un objet » : retrouver tous les objets d'un prototype dans la carte, les sélectionner, aller de l'un à
//! l'autre, ou les remplacer par le prototype choisi dans la palette « Placer ».

use crate::app::MapperApp;
use crate::i18n::t;
use eframe::egui::{self, Pos2};
use fo_core::{ObjectType, ProtoEntry, geom};
use fo_scene::ObjectRef;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

/// Nombre maximal de prototypes proposés dans la liste des résultats de la recherche par nom.
const MAX_SUGGESTIONS: usize = 30;

#[derive(Default)]
pub(crate) struct FindState {
    pub open: bool,
    query: String,
    /// Prototype recherché : (PID, nom affiché).
    pub(crate) chosen: Option<(u32, String)>,
    /// Toutes les élévations (sinon l'élévation affichée).
    pub(crate) all_elevations: bool,
    pub(crate) hits: Vec<(usize, usize)>,
    /// Résultat actuellement visé par « Suivant ».
    cursor: usize,
    catalog: Option<Arc<Vec<ProtoEntry>>>,
    loading: Option<Receiver<Vec<ProtoEntry>>>,
}

/// Prototypes dont le nom, le fichier ou le PID (en hexadécimal) contient `query`, dans l'ordre du catalogue.
pub(crate) fn suggestions<'a>(catalog: &'a [ProtoEntry], query: &str) -> Vec<&'a ProtoEntry> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    catalog
        .iter()
        .filter(|p| {
            p.name.to_lowercase().contains(&q)
                || p.file.to_lowercase().contains(&q)
                || format!("{:08x}", p.pid).contains(&q)
        })
        .take(MAX_SUGGESTIONS)
        .collect()
}

impl MapperApp {
    fn load_find_catalog(&mut self) {
        if self.find.catalog.is_some() || self.find.loading.is_some() {
            return;
        }
        let Some(g) = &self.game else { return };
        let data = g.data.clone();
        let (tx, rx) = channel();
        self.find.loading = Some(rx);
        std::thread::spawn(move || {
            let mut all = Vec::new();
            for ty in [
                ObjectType::Item,
                ObjectType::Critter,
                ObjectType::Scenery,
                ObjectType::Wall,
                ObjectType::Misc,
            ] {
                all.extend(data.proto_catalog(ty));
            }
            let _ = tx.send(all);
        });
    }

    fn proto_label(&self, pid: u32) -> String {
        let name = self.proto_name(pid).unwrap_or_default();
        if name.is_empty() {
            format!("{pid:08X}")
        } else {
            format!("{name} ({pid:08X})")
        }
    }

    pub(crate) fn run_find(&mut self) {
        let (Some((pid, _)), Some(cur)) = (self.find.chosen.clone(), self.current.as_ref()) else {
            return;
        };
        let only = (!self.find.all_elevations).then_some(self.elevation);
        self.find.hits = cur.map.find_objects(pid, only);
        self.find.cursor = 0;
        self.status = format!("{} {}", self.find.hits.len(), t("objet(s) trouvé(s)"));
    }

    /// Sélectionne le résultat `n` : change d'élévation si besoin et centre la vue dessus.
    fn goto_hit(&mut self, n: usize) {
        let Some(&(e, i)) = self.find.hits.get(n) else {
            return;
        };
        let Some(tile) = self
            .current
            .as_ref()
            .and_then(|c| c.map.objects.get(e))
            .and_then(|l| l.get(i))
            .map(|o| o.tile())
            .filter(|t| *t >= 0)
        else {
            return;
        };
        if e != self.elevation
            && self
                .current
                .as_ref()
                .is_some_and(|c| c.scenes.get(e).is_some_and(Option::is_some))
        {
            self.elevation = e;
        }
        self.selection.set_object(Some(ObjectRef {
            elevation: e as u8,
            index: i as u32,
        }));
        let (x, y) = geom::hex_to_screen(tile as u32);
        self.camera.center = Pos2::new(x as f32, y as f32);
        self.find.cursor = n;
    }

    pub(crate) fn replace_hits(&mut self) {
        let (Some(target), Some(g)) = (self.edit.place.clone(), self.game.as_ref()) else {
            return;
        };
        let hits = self.find.hits.clone();
        let info = target.info;
        let _ = g;
        let mut n = 0;
        self.mutate_as("Remplacement d'objets", |m, _| {
            for &(e, i) in &hits {
                n += usize::from(m.replace_object(e, i, &info));
            }
            n > 0
        });
        self.status = format!("{n} {}", t("objet(s) remplacé(s)"));
        self.selection.clear();
        self.find.hits.clear();
    }

    /// Capture d'écran (`FO_SHOT_FIND=<texte>`) : ouvre la fenêtre, choisit le premier prototype correspondant et cherche.
    pub(crate) fn shot_find(&mut self, query: &str) -> bool {
        self.find.open = true;
        self.find.all_elevations = true;
        if self.find.query.is_empty() {
            self.find.query = query.to_string();
        }
        if self.find.chosen.is_none()
            && let Some(cat) = self.find.catalog.clone()
            && let Some(p) = suggestions(&cat, query).first()
        {
            self.find.chosen = Some((
                p.pid,
                if p.name.is_empty() {
                    p.file.clone()
                } else {
                    p.name.clone()
                },
            ));
            self.run_find();
            self.goto_hit(0);
        }
        !self.find.hits.is_empty()
    }

    pub(crate) fn find_window(&mut self, ctx: &egui::Context) {
        if !self.find.open {
            return;
        }
        self.load_find_catalog();
        if let Some(rx) = &self.find.loading
            && let Ok(all) = rx.try_recv()
        {
            self.find.catalog = Some(Arc::new(all));
            self.find.loading = None;
        } else if self.find.loading.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        let mut open = true;
        let mut run = false;
        let mut pick: Option<(u32, String)> = None;
        let mut use_selected = false;
        let mut select_all = false;
        let mut go: Option<usize> = None;
        let mut replace = false;
        let selected_pid = self
            .selection
            .primary()
            .and_then(|r| self.object(r))
            .map(|o| o.pid());
        let place = self.edit.place.clone();
        let hits_len = self.find.hits.len();
        egui::Window::new(t("Rechercher un objet"))
            .open(&mut open)
            .order(egui::Order::Middle)
            .collapsible(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(t("Nom du prototype"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.find.query)
                            .desired_width(f32::INFINITY)
                            .hint_text(t("nom, fichier ou PID…")),
                    );
                });
                if self.find.catalog.is_none() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(t("Chargement des prototypes…"));
                    });
                } else if let Some(cat) = self.find.catalog.clone() {
                    for p in suggestions(&cat, &self.find.query) {
                        let label = format!(
                            "{} — {} ({:08X})",
                            if p.name.is_empty() { &p.file } else { &p.name },
                            p.file,
                            p.pid
                        );
                        if ui
                            .selectable_label(
                                self.find
                                    .chosen
                                    .as_ref()
                                    .is_some_and(|(pid, _)| *pid == p.pid),
                                label,
                            )
                            .clicked()
                        {
                            pick = Some((
                                p.pid,
                                if p.name.is_empty() {
                                    p.file.clone()
                                } else {
                                    p.name.clone()
                                },
                            ));
                        }
                    }
                }
                if ui
                    .add_enabled(
                        selected_pid.is_some(),
                        egui::Button::new(t("Utiliser l'objet sélectionné")),
                    )
                    .clicked()
                {
                    use_selected = true;
                }
                ui.separator();
                match &self.find.chosen {
                    Some((pid, name)) => ui.label(
                        egui::RichText::new(format!("{} : {name} ({pid:08X})", t("Recherché")))
                            .strong(),
                    ),
                    None => ui.weak(t("Choisissez un prototype.")),
                };
                ui.checkbox(&mut self.find.all_elevations, t("Toutes les élévations"));
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.find.chosen.is_some() && self.current.is_some(),
                            egui::Button::new(t("Chercher")),
                        )
                        .clicked()
                    {
                        run = true;
                    }
                    ui.label(format!("{hits_len} {}", t("trouvé(s)")));
                });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(hits_len > 0, egui::Button::new(t("Précédent")))
                        .clicked()
                    {
                        go = Some((self.find.cursor + hits_len - 1) % hits_len.max(1));
                    }
                    if ui
                        .add_enabled(hits_len > 0, egui::Button::new(t("Suivant")))
                        .clicked()
                    {
                        go = Some((self.find.cursor + 1) % hits_len.max(1));
                    }
                    if ui
                        .add_enabled(hits_len > 0, egui::Button::new(t("Tout sélectionner")))
                        .clicked()
                    {
                        select_all = true;
                    }
                });
                ui.separator();
                match &place {
                    Some(p) => ui.label(format!(
                        "{} : {}",
                        t("Remplacer par (palette « Placer »)"),
                        if p.name.is_empty() { &p.file } else { &p.name }
                    )),
                    None => ui.weak(t(
                        "Pour remplacer, choisissez d'abord un prototype dans l'outil « Placer ».",
                    )),
                };
                if ui
                    .add_enabled(
                        hits_len > 0 && place.is_some() && self.edit.enabled,
                        egui::Button::new(format!(
                            "{} ({hits_len})",
                            t("Remplacer tous les résultats")
                        )),
                    )
                    .clicked()
                {
                    replace = true;
                }
                if !self.edit.enabled {
                    ui.weak(t("Activez le mode édition pour remplacer."));
                }
            });
        if let Some(p) = pick {
            self.find.chosen = Some(p);
            self.find.hits.clear();
        }
        if use_selected && let Some(pid) = selected_pid {
            self.find.chosen = Some((pid, self.proto_label(pid)));
            self.find.hits.clear();
        }
        if run {
            self.run_find();
            if !self.find.hits.is_empty() {
                self.goto_hit(0);
            }
        }
        if let Some(n) = go {
            self.goto_hit(n);
        }
        if select_all {
            let e = self.elevation;
            let refs: Vec<ObjectRef> = self
                .find
                .hits
                .iter()
                .filter(|(he, _)| *he == e)
                .map(|&(he, i)| ObjectRef {
                    elevation: he as u8,
                    index: i as u32,
                })
                .collect();
            self.status = format!("{} {}", refs.len(), t("objet(s) sélectionné(s)"));
            self.selection.set_objects(refs);
        }
        if replace {
            self.replace_hits();
        }
        self.find.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fo_core::formats::ProtoHeader;

    fn entry(pid: u32, name: &str, file: &str) -> ProtoEntry {
        ProtoEntry {
            pid,
            file: file.into(),
            name: name.into(),
            info: ProtoHeader {
                pid,
                msg_id: 0,
                fid: pid,
                light_distance: 0,
                light_intensity: 0,
                flags: 0,
                subtype: -1,
                script_id: -1,
                critter: None,
            },
        }
    }

    #[test]
    fn suggestions_match_name_file_or_pid_and_are_bounded() {
        let cat = vec![
            entry(0x0200_0015, "Source lumineuse", "light.pro"),
            entry(0x0300_00AB, "Mur de briques", "brick.pro"),
            entry(0x0200_0016, "Lampe", "lamp.pro"),
        ];
        assert_eq!(suggestions(&cat, "lumin").len(), 1);
        assert_eq!(suggestions(&cat, "BRICK.PRO")[0].pid, 0x0300_00AB);
        assert_eq!(suggestions(&cat, "000015")[0].pid, 0x0200_0015);
        assert!(suggestions(&cat, "  ").is_empty());
        let many: Vec<ProtoEntry> = (0..100).map(|i| entry(i, "Mur", "m.pro")).collect();
        assert_eq!(suggestions(&many, "mur").len(), MAX_SUGGESTIONS);
    }
}
