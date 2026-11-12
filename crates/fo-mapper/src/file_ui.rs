//! Fichier : nouvelle carte, fermer, miniatures de toutes les cartes, lancer le jeu sur la carte courante.

use crate::app::{Confirm, MapperApp};
use crate::loader::LoadedMap;
use eframe::egui::{self, Color32, TextureHandle, TextureOptions, Vec2};
use fo_core::map::ELEVATION_COUNT;
use fo_core::{GameData, Map, launch, read_map};
use fo_scene::{Layer, LayerSet, Scene, build_scene, render};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};

/// Largeur visée (pixels) d'une miniature.
const THUMB_WIDTH: i32 = 360;

#[derive(Debug)]
pub(crate) struct NewMapDialog {
    pub name: String,
    pub elevations: usize,
    pub template: Template,
    pub error: Option<String>,
}

/// Gabarit d'une nouvelle carte : remplissage initial des sols.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Template {
    #[default]
    Empty,
    Desert,
    Coast,
    Town,
    Interior,
}

impl Template {
    pub(crate) const ALL: [Template; 5] = [
        Template::Empty,
        Template::Desert,
        Template::Coast,
        Template::Town,
        Template::Interior,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Template::Empty => "Vide",
            Template::Desert => "Désert",
            Template::Coast => "Côte",
            Template::Town => "Ville",
            Template::Interior => "Intérieur",
        }
    }

    pub(crate) fn help(self) -> &'static str {
        match self {
            Template::Empty => "Aucune tuile : tout est à peindre.",
            Template::Desert => "Sol de terre et de sable sur toute la carte.",
            Template::Coast => "Mer à gauche, rivage, puis terre.",
            Template::Town => "Sol de terre traversé par une route.",
            Template::Interior => "Sol de bâtiment partout, sans toit.",
        }
    }

    /// Préfixes de noms de tuiles (dans `tiles.lst`) utilisés par le gabarit : (terre, accent).
    fn prefixes(self) -> (&'static str, &'static str) {
        match self {
            Template::Empty => ("", ""),
            Template::Desert => ("tp", "tp"),
            Template::Coast => ("seas", "shore"),
            Template::Town => ("tp", "road"),
            Template::Interior => ("hola", "hola"),
        }
    }

    /// Remplit les sols de `map` selon le gabarit, avec les tuiles de `names` (contenu de `tiles.lst`).
    pub(crate) fn apply(self, map: &mut Map, names: &[String]) {
        let (base, accent) = self.prefixes();
        if base.is_empty() {
            return;
        }
        let ids = |prefix: &str| -> Vec<u16> {
            names
                .iter()
                .enumerate()
                .filter(|(i, n)| *i >= 2 && n.to_ascii_lowercase().starts_with(prefix))
                .map(|(i, _)| i as u16)
                .take(6)
                .collect()
        };
        let (base_ids, accent_ids, ground_ids) = (ids(base), ids(accent), ids("tp"));
        if base_ids.is_empty() {
            return;
        }
        for e in 0..ELEVATION_COUNT {
            if !map.header.has_elevation(e) {
                continue;
            }
            for square in 0..10_000usize {
                let (row, col) = (square / 100, square % 100);
                let pick = |ids: &[u16]| ids[(row * 7 + col * 3) % ids.len()];
                let id = match self {
                    Template::Coast if col < 40 => pick(&base_ids),
                    Template::Coast if col < 44 && !accent_ids.is_empty() => pick(&accent_ids),
                    Template::Coast if !ground_ids.is_empty() => pick(&ground_ids),
                    Template::Town if (48..52).contains(&row) && !accent_ids.is_empty() => {
                        pick(&accent_ids)
                    }
                    _ => pick(&base_ids),
                };
                map.set_tile(e, square, fo_core::map::TileLayer::Floor, id);
            }
        }
    }
}

type ThumbMessage = (String, u32, u32, Vec<u8>);

pub(crate) struct ThumbsState {
    pub open: bool,
    pub filter: String,
    textures: HashMap<String, TextureHandle>,
    rx: Option<Receiver<ThumbMessage>>,
    cancel: Arc<AtomicBool>,
    pending: usize,
}

impl Default for ThumbsState {
    fn default() -> Self {
        Self {
            open: false,
            filter: String::new(),
            textures: HashMap::new(),
            rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
            pending: 0,
        }
    }
}

impl std::fmt::Debug for ThumbsState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThumbsState")
            .field("open", &self.open)
            .field("textures", &self.textures.len())
            .finish()
    }
}

#[derive(Debug, Default)]
pub(crate) struct FileUi {
    pub new_map: Option<NewMapDialog>,
    pub thumbs: ThumbsState,
    /// La modification de la configuration de départ a été acceptée pendant cette session.
    pub play_confirmed: bool,
    pub play_dialog: bool,
    /// Gabarit choisi dans la fenêtre « Nouvelle carte », appliqué à la création.
    pub new_map_template: Template,
}

/// Nom de fichier de carte valide : 1 à 8 caractères `a-z`, `0-9`, `_`.
pub(crate) fn valid_map_name(name: &str) -> bool {
    (1..=8).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

impl MapperApp {
    // ---------------------------------------------------------------- nouvelle carte et fermeture

    pub(crate) fn open_new_map_dialog(&mut self) {
        if self.game.is_some() {
            self.file.new_map = Some(NewMapDialog {
                name: String::new(),
                elevations: 1,
                template: Template::Empty,
                error: None,
            });
        }
    }

    /// Demande de création : confirmation si la carte actuelle a des modifications non enregistrées.
    pub(crate) fn request_new_map(&mut self, name: &str, elevations: usize) {
        self.create_new_map(name, elevations);
    }

    pub(crate) fn create_new_map(&mut self, name: &str, elevations: usize) {
        if self.game.is_none() {
            return;
        }
        if self.current.is_some() {
            self.open_new_tab();
        }
        self.ensure_tab();
        let Some(g) = &self.game else { return };
        let mut map = Map::new_empty(name, elevations);
        let names = g.data.list("art/tiles/tiles.lst");
        std::mem::take(&mut self.file.new_map_template).apply(&mut map, &names);
        self.install_new_map(name, map);
    }

    /// Met une carte construite en mémoire dans l'onglet courant (non enregistrée).
    pub(crate) fn install_new_map(&mut self, name: &str, map: Map) {
        let Some(g) = &self.game else { return };
        let mut scenes: [Option<Arc<Scene>>; ELEVATION_COUNT] = Default::default();
        for (e, slot) in scenes.iter_mut().enumerate() {
            if map.header.has_elevation(e) {
                *slot = Some(Arc::new(build_scene(&map, e, &g.art)));
            }
        }
        self.scripts.set_map(&g.data, &map);
        self.current = Some(LoadedMap {
            file: name.to_string(),
            map: Arc::new(map),
            scenes,
            millis: 0,
        });
        self.elevation = 0;
        self.selection.clear();
        self.needs_fit = true;
        self.reset_edit_for_new_map();
        // Une carte neuve n'existe pas encore sur le disque : elle est « modifiée » tant qu'elle n'est pas enregistrée.
        self.edit.dirty = true;
        self.edit.enabled = true;
        self.status = format!(
            "{} {} ({})",
            crate::i18n::t("Nouvelle carte"),
            crate::i18n::quote(&name),
            crate::i18n::t("non enregistrée")
        );
    }

    pub(crate) fn request_close(&mut self) {
        if self.edit.dirty {
            self.confirm = Some(Confirm::Close);
        } else {
            self.close_map();
        }
    }

    pub(crate) fn close_map(&mut self) {
        self.current = None;
        self.selection.clear();
        self.reset_edit_for_new_map();
        self.drop_active_tab();
        self.status = crate::i18n::t("Carte fermée").into();
    }

    pub(crate) fn new_map_window(&mut self, ctx: &egui::Context) {
        let Some(dlg) = &mut self.file.new_map else {
            return;
        };
        let mut create = false;
        let mut cancel = false;
        egui::Window::new(crate::i18n::t("Nouvelle carte"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_min_width(460.0);
                ui.label(egui::RichText::new(crate::i18n::t("Nouvelle carte")).heading());
                ui.add_space(4.0);
                egui::Grid::new("new_map_fields")
                    .num_columns(2)
                    .spacing([14.0, 10.0])
                    .show(ui, |ui| {
                        ui.label(crate::i18n::t("Nom du fichier"));
                        ui.add(
                            egui::TextEdit::singleline(&mut dlg.name)
                                .hint_text(crate::i18n::t("8 caractères max"))
                                .desired_width(180.0),
                        );
                        ui.end_row();
                        ui.label(crate::i18n::t("Élévations"));
                        ui.horizontal(|ui| {
                            for n in 1..=3 {
                                ui.selectable_value(&mut dlg.elevations, n, n.to_string());
                            }
                        });
                        ui.end_row();
                    });
                ui.add_space(6.0);
                ui.label(crate::i18n::t("Gabarit"));
                ui.horizontal_wrapped(|ui| {
                    for t in Template::ALL {
                        if ui
                            .add(
                                egui::Button::new(crate::i18n::t(t.label()))
                                    .selected(dlg.template == t)
                                    .min_size(Vec2::new(84.0, 32.0)),
                            )
                            .on_hover_text(crate::i18n::t(t.help()))
                            .clicked()
                        {
                            dlg.template = t;
                        }
                    }
                });
                ui.label(egui::RichText::new(crate::i18n::t(dlg.template.help())).weak());
                if let Some(e) = &dlg.error {
                    ui.colored_label(Color32::LIGHT_RED, e);
                }
                use crate::ui_kit::FooterButton as B;
                match crate::ui_kit::footer(
                    ui,
                    &[
                        B::normal(crate::i18n::t("Annuler")),
                        B::primary(crate::i18n::t("Créer")),
                    ],
                ) {
                    Some(0) => cancel = true,
                    Some(1) => create = true,
                    _ => {}
                }
            });
        if cancel {
            self.file.new_map = None;
        } else if create {
            let (name, elevations, template) = (
                dlg.name.trim().to_ascii_lowercase(),
                dlg.elevations,
                dlg.template,
            );
            let exists = self
                .game
                .as_ref()
                .is_some_and(|g| g.maps.iter().any(|m| m.file == name));
            if !valid_map_name(&name) {
                dlg.error =
                    Some(crate::i18n::t("Le nom doit faire 1 à 8 caractères : a–z, 0–9, _").into());
            } else if exists {
                dlg.error = Some(crate::i18n::t("Une carte porte déjà ce nom.").into());
            } else {
                self.file.new_map = None;
                self.file.new_map_template = template;
                self.request_new_map(&name, elevations);
            }
        }
    }

    // ---------------------------------------------------------------- miniatures

    pub(crate) fn toggle_thumbnails(&mut self, ctx: &egui::Context) {
        self.file.thumbs.open = !self.file.thumbs.open;
        if self.file.thumbs.open {
            self.start_thumbnails(ctx);
        } else {
            self.file.thumbs.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Lance (ou relance) la génération, en arrière-plan, des miniatures qui manquent.
    fn start_thumbnails(&mut self, ctx: &egui::Context) {
        let Some(g) = &self.game else { return };
        let todo: Vec<String> = g
            .maps
            .iter()
            .map(|m| m.file.clone())
            .filter(|f| !self.file.thumbs.textures.contains_key(f))
            .collect();
        if todo.is_empty()
            || (self.file.thumbs.rx.is_some()
                && self.file.thumbs.pending > 0
                && !self.file.thumbs.cancel.load(Ordering::Relaxed))
        {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.file.thumbs.rx = Some(rx);
        self.file.thumbs.cancel = cancel.clone();
        self.file.thumbs.pending = todo.len();
        let (data, art, ctx) = (g.data.clone(), g.art.clone(), ctx.clone());
        std::thread::spawn(move || {
            let mut layers = LayerSet::all();
            layers.set(Layer::Exit, false);
            layers.set(Layer::Blocker, false);
            for file in todo {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let image = data
                    .read_map_bytes(&file)
                    .ok()
                    .and_then(|b| read_map(&b, &*data).ok())
                    .and_then(|map| {
                        let e = (0..ELEVATION_COUNT).find(|&e| map.header.has_elevation(e))?;
                        let scene = build_scene(&map, e, &art);
                        let ds = (scene.bounds.2 / THUMB_WIDTH).max(2) as u32;
                        Some(render(&scene, layers, scene.bounds, ds, None))
                    });
                // Une carte illisible donne une miniature vide : elle ne sera pas réessayée à chaque ouverture.
                let (w, h, rgba) = image.map_or((1, 1, vec![40, 20, 20, 255]), |i| {
                    (i.width, i.height, i.data)
                });
                if tx.send((file, w, h, rgba)).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
    }

    /// Nombre de miniatures déjà prêtes.
    pub(crate) fn thumbnail_count(&self) -> usize {
        self.file.thumbs.textures.len()
    }

    pub(crate) fn thumbnails_window(&mut self, ctx: &egui::Context) {
        if !self.file.thumbs.open {
            return;
        }
        // Réception des miniatures prêtes.
        if let Some(rx) = &self.file.thumbs.rx {
            while let Ok((file, w, h, rgba)) = rx.try_recv() {
                let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                self.file.thumbs.textures.insert(
                    file.clone(),
                    ctx.load_texture(format!("thumb_{file}"), img, TextureOptions::LINEAR),
                );
                self.file.thumbs.pending = self.file.thumbs.pending.saturating_sub(1);
            }
        }
        let Some(maps) = self.game.as_ref().map(|g| g.maps.clone()) else {
            return;
        };
        let mut open = true;
        let mut load = None;
        egui::Window::new(crate::i18n::t("Cartes — miniatures"))
            .open(&mut open)
            .default_size([900.0, 640.0])
            .min_size([320.0, 240.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.file.thumbs.filter)
                            .hint_text(crate::i18n::t("Filtrer…"))
                            .desired_width(220.0),
                    );
                    let done = self.file.thumbs.textures.len();
                    ui.label(format!(
                        "{done} / {} {}",
                        maps.len(),
                        crate::i18n::t("miniatures")
                    ));
                    if self.file.thumbs.pending > 0 {
                        ui.spinner();
                    }
                });
                ui.separator();
                let needle = self.file.thumbs.filter.to_lowercase();
                let list: Vec<_> = maps
                    .iter()
                    .filter(|m| {
                        needle.is_empty()
                            || m.file.contains(&needle)
                            || m.display
                                .as_deref()
                                .is_some_and(|d| d.to_lowercase().contains(&needle))
                    })
                    .collect();
                let size = Vec2::new(THUMB_WIDTH as f32 * 0.6, THUMB_WIDTH as f32 * 0.3);
                let cell = Vec2::new(size.x + 14.0, size.y + 44.0);
                let columns = ((ui.available_width() / cell.x).floor() as usize).max(1);
                let lines = list.len().div_ceil(columns);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show_rows(ui, cell.y, lines, |ui, range| {
                        for line in range {
                            ui.horizontal(|ui| {
                                for m in list.iter().skip(line * columns).take(columns) {
                                    ui.allocate_ui(Vec2::new(cell.x - 6.0, cell.y - 4.0), |ui| {
                                        ui.vertical(|ui| {
                                            match self.file.thumbs.textures.get(&m.file) {
                                                Some(t) => {
                                                    if ui
                                                        .add(egui::Button::image(
                                                            egui::Image::new((t.id(), size))
                                                                .fit_to_exact_size(size),
                                                        ))
                                                        .clicked()
                                                    {
                                                        load = Some(m.file.clone());
                                                    }
                                                }
                                                None => {
                                                    ui.allocate_ui(size, |ui| {
                                                        ui.centered_and_justified(|ui| ui.spinner())
                                                    });
                                                }
                                            }
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&m.file).strong(),
                                                )
                                                .truncate(),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(
                                                        m.display.as_deref().unwrap_or(""),
                                                    )
                                                    .small(),
                                                )
                                                .truncate(),
                                            );
                                        });
                                    });
                                }
                            });
                        }
                    });
            });
        if !open {
            self.toggle_thumbnails(ctx);
        }
        if let Some(f) = load {
            self.request_load(&f, ctx);
        }
    }

    // ---------------------------------------------------------------- jouer

    /// F5 : enregistre la carte puis lance le moteur, avec cette carte comme départ d'une nouvelle partie.
    pub(crate) fn request_play(&mut self) {
        if self.current.is_none() || self.game.is_none() {
            self.status = crate::i18n::t("Ouvrez d'abord une carte.").into();
            return;
        }
        if self.game.as_ref().is_some_and(|g| g.data.is_fallout1()) {
            self.status =
                crate::i18n::t("Lancer le jeu n'est pas disponible pour Fallout 1.").into();
            return;
        }
        if self.file.play_confirmed {
            self.play_now();
        } else {
            self.file.play_dialog = true;
        }
    }

    fn save_and_sync_test_map(&mut self) -> Result<(std::path::PathBuf, String), String> {
        if !self.save_current() {
            return Err(crate::i18n::t("Échec de l'enregistrement").into());
        }
        let (source_root, file, map) = match (&self.game, &self.current) {
            (Some(g), Some(cur)) => (g.data.root.clone(), cur.file.clone(), cur.map.clone()),
            _ => return Err(crate::i18n::t("Ouvrez d'abord une carte.").into()),
        };
        let test_root = self
            .profiles
            .active()
            .map(|p| p.test_root().to_path_buf())
            .unwrap_or_else(|| source_root.clone());
        let same_root = test_root
            .canonicalize()
            .ok()
            .zip(source_root.canonicalize().ok())
            .is_some_and(|(test, source)| test == source);
        if !same_root {
            let result = (|| -> fo_core::Result<()> {
                let test = GameData::open(&test_root)?;
                let bytes = fo_core::write_map(&map)?;
                test.save_map(&file, &bytes)?;
                crate::exit_ui::save_edge_beside(&test, &file, map.edge.as_ref())
                    .map_or(Ok(()), |e| Err(fo_core::Error::format(e)))?;
                Ok(())
            })();
            result.map_err(|e| {
                format!(
                    "{} : {e}",
                    crate::i18n::t("Échec de copie vers le dossier de test")
                )
            })?;
        }
        Ok((test_root, file))
    }

    fn play_now(&mut self) {
        let (test_root, file) = match self.save_and_sync_test_map() {
            Ok(target) => target,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        self.status = match launch::launch_on_map(&test_root, &file) {
            Ok(msg) => msg,
            Err(e) => format!("{} : {e}", crate::i18n::t("Lancement impossible")),
        };
    }

    pub(crate) fn request_fusion_reload(&mut self) {
        let (root, file) = match self.save_and_sync_test_map() {
            Ok(target) => target,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let Some((port, token)) = fo_core::game_link::link_settings(&root) else {
            self.status = crate::i18n::t("Canal Fusion désactivé : configurez [debug] control_port dans fallout2.cfg du dossier de test.").into();
            return;
        };
        let tx = self.fusion_tx.clone();
        let ctx = self.ctx_handle.clone();
        self.fusion_pending = true;
        self.status = crate::i18n::t("Connexion à Fallout Fusion…").into();
        std::thread::spawn(move || {
            let result = (|| -> fo_core::Result<String> {
                let mut link = fo_core::game_link::GameLink::connect(port, token.as_deref())?;
                link.ping()?;
                link.load_map(&file, None, true)
            })()
            .map_err(|e| e.to_string());
            let _ = tx.send(result);
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });
    }

    pub(crate) fn poll_fusion_link(&mut self) {
        if let Ok(result) = self.fusion_rx.try_recv() {
            self.fusion_pending = false;
            self.status = match result {
                Ok(state) => format!(
                    "{} : {state}",
                    crate::i18n::t("Carte rechargée dans Fallout Fusion")
                ),
                Err(e) => format!(
                    "{} : {e}",
                    crate::i18n::t("Connexion à Fallout Fusion impossible")
                ),
            };
        }
    }

    pub(crate) fn play_dialog_window(&mut self, ctx: &egui::Context) {
        if !self.file.play_dialog {
            return;
        }
        let mut go = false;
        let mut cancel = false;
        let executable = self
            .profiles
            .active()
            .map(|p| p.test_root())
            .or_else(|| self.game.as_ref().map(|g| g.data.root.as_path()))
            .and_then(launch::find_game_executable);
        egui::Window::new(crate::i18n::t("Jouer cette carte")).order(egui::Order::Middle).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            if let Some(exe) = &executable {
                ui.label(exe.display().to_string());
            }
            ui.label(crate::i18n::t("La carte sera enregistrée, puis le jeu sera lancé."));
            if self.profiles.active().is_some_and(|p| p.test_dir != p.map_dir && !p.test_dir.as_os_str().is_empty()) {
                ui.label("La carte enregistrée sera copiée dans le dossier de test avant le lancement.");
            }
            ui.label(crate::i18n::t("La configuration de départ du jeu sera sauvegardée avant modification et pourra être restaurée depuis le menu Fichier."));
            ui.label(crate::i18n::t("Choisissez Nouvelle partie dans le jeu pour charger cette carte."));
            use crate::ui_kit::FooterButton as B;
            match crate::ui_kit::footer(ui, &[B::normal(crate::i18n::t("Annuler")), B::primary(crate::i18n::t("Enregistrer et lancer"))]) {
                Some(0) => cancel = true,
                Some(1) => go = true,
                _ => {}
            }
        });
        if go {
            self.file.play_dialog = false;
            self.file.play_confirmed = true;
            self.play_now();
        } else if cancel {
            self.file.play_dialog = false;
        }
    }

    pub(crate) fn restore_ddraw(&mut self) {
        let Some(g) = &self.game else { return };
        self.status = match launch::restore_launch_config(&g.data.root) {
            Ok(true) => crate::i18n::t("Configuration de lancement restaurée.").into(),
            Ok(false) => {
                crate::i18n::t("Aucune copie de secours de la configuration de lancement.").into()
            }
            Err(e) => format!("{} : {e}", crate::i18n::t("Échec")),
        };
    }

    pub(crate) fn file_windows(&mut self, ctx: &egui::Context) {
        self.new_map_window(ctx);
        self.thumbnails_window(ctx);
        self.play_dialog_window(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_names_follow_the_dos_convention() {
        assert!(valid_map_name("arvillag") && valid_map_name("a1_b"));
        assert!(
            !valid_map_name("")
                && !valid_map_name("toolongname")
                && !valid_map_name("Maj")
                && !valid_map_name("a b")
                && !valid_map_name("a.b")
        );
    }
}
