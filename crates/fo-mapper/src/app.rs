use crate::edit_ui::EditState;
use crate::file_ui::FileUi;
use crate::loader::{LoadResult, LoadedMap, spawn_load};
use crate::prefs::Prefs;
use crate::scripts::ScriptsState;
use crate::selection::Selection;
use crate::view::Camera;
use crate::world_ui::WorldUi;
use eframe::egui::{self, Color32, Rect, TextureHandle};
use fo_core::geom;
use fo_core::map::MapObject;
use fo_core::{ArtLibrary, GameData, MapEntry, PaletteAnim};
use fo_scene::{LayerSet, ObjectRef, Scene};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

pub(crate) const KEY_GAME_DIR: &str = "game_dir";
pub(crate) const KEY_LAST_MAP: &str = "last_map";
pub(crate) const KEY_LAYERS: &str = "layers2";
pub(crate) const KEY_VIEW: &str = "view_flags";
pub(crate) const KEY_MAPLIST: &str = "maplist_mode";
pub(crate) const KEY_TOOLBAR: &str = "toolbar_tools";
pub(crate) const KEY_RECENT_GAMES: &str = "recent_games";
pub(crate) const KEY_RECENT_SAVE: &str = "recent_save_dirs";
/// Nombre maximal de textures créées par image affichée (évite un blocage au premier affichage).
pub(crate) const TEXTURE_BUDGET: usize = 400;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Map,
    Scripts,
    World,
}

pub(crate) struct Game {
    pub data: Arc<GameData>,
    pub art: Arc<ArtLibrary>,
    pub maps: Vec<MapEntry>,
}

/// Action en attente de la réponse à « enregistrer les modifications ? ».
#[derive(Clone)]
pub(crate) enum Confirm {
    Quit,
    Close,
}

pub struct MapperApp {
    pub(crate) game: Option<Game>,
    pub(crate) status: String,
    pub(crate) filter: String,
    pub(crate) loading: Option<String>,
    pub(crate) current: Option<LoadedMap>,
    pub(crate) elevation: usize,
    pub(crate) camera: Camera,
    pub(crate) needs_fit: bool,
    /// Vue de départ d'une carte qui vient de charger : 50 % de zoom, centrée sur le point de départ du joueur.
    pub(crate) start_view: bool,
    pub(crate) clip: Option<crate::extras::Clipboard>,
    /// Onglets de cartes : l'entrée `active_tab` est un simple emplacement (la carte active est dans les champs ci-dessus).
    pub(crate) docs: Vec<crate::tabs::Doc>,
    /// Fermeture groupée d'onglets en cours (« Fermer les autres » / « Fermer tous »).
    pub(crate) close_batch: Option<crate::tabs::CloseBatch>,
    /// Liste des cartes : à plat ou par dossiers, tri alphabétique ou dans l'ordre du jeu, croissant ou décroissant.
    /// Boutons d'outils de la barre du haut : ordre et visibilité (modifiables par clic droit sur la barre).
    pub(crate) toolbar_tools: Vec<(crate::edit_ui::Tool, bool)>,
    pub(crate) toolbar_dialog: bool,
    pub(crate) fill: crate::fill_ui::FillState,
    pub(crate) map_list_flat: bool,
    pub(crate) map_sort_alpha: bool,
    pub(crate) map_sort_desc: bool,
    pub(crate) active_tab: usize,
    /// Onglet affiché à droite de la carte active (vue côte à côte).
    pub(crate) split: Option<usize>,
    pub(crate) analysis: crate::analysis_ui::AnalysisState,
    pub(crate) cross_drag: bool,
    pub(crate) history_open: bool,
    pub(crate) legend_open: bool,
    pub(crate) log: crate::log_ui::LogState,
    pub(crate) window: crate::window_state::WindowState,
    pub(crate) dialog: Option<crate::file_dialog::DialogState>,
    pub(crate) find: crate::find_ui::FindState,
    pub(crate) check: crate::check_ui::CheckState,
    pub(crate) diff: crate::diff_ui::DiffState,
    pub(crate) msg: crate::msg_ui::MsgEditor,
    pub(crate) preview: crate::preview::PreviewState,
    pub(crate) recovery: crate::recovery::RecoveryState,
    pub(crate) export: crate::export_ui::ExportState,
    pub(crate) walk: crate::walk_ui::WalkState,
    pub(crate) library: crate::library_ui::LibraryState,
    pub(crate) minimap: crate::minimap::MinimapState,
    pub(crate) procgen: crate::procgen_ui::GenState,
    pub(crate) links: crate::links_ui::LinksState,
    pub(crate) recent_games: Vec<PathBuf>,
    pub(crate) recent_save_dirs: Vec<PathBuf>,
    pub(crate) profiles: crate::profiles::ProfileState,
    /// Action dont la touche attend d'être choisie dans les options.
    pub(crate) rebinding: Option<crate::prefs::KeyAction>,
    /// Onglet affiché dans la fenêtre Options.
    pub(crate) prefs_tab: usize,
    pub(crate) show_scripted: bool,
    pub(crate) show_left: bool,
    pub(crate) show_right: bool,
    pub(crate) layers: LayerSet,
    pub(crate) show_markers: bool,
    pub(crate) show_grid: bool,
    pub(crate) show_edges: bool,
    pub(crate) show_lights: bool,
    pub(crate) selection: Selection,
    pub(crate) hover_hex: Option<u32>,
    /// Zone du canevas de la carte à l'image précédente (pour poser un motif glissé depuis la bibliothèque).
    pub(crate) canvas_rect: Rect,
    pub(crate) textures: HashMap<u64, TextureHandle>,
    pub(crate) effects: crate::effects::EffectsCache,
    pub(crate) tx: Sender<LoadResult>,
    pub(crate) rx: Receiver<LoadResult>,
    pub(crate) fusion_tx: Sender<Result<String, String>>,
    pub(crate) fusion_rx: Receiver<Result<String, String>>,
    pub(crate) fusion_pending: bool,
    pub(crate) pending_map: Option<String>,
    pub(crate) mode: Mode,
    pub(crate) scripts: ScriptsState,
    pub(crate) edit: EditState,
    pub(crate) confirm: Option<Confirm>,
    pub(crate) file: FileUi,
    pub(crate) prefs: Prefs,
    pub(crate) prefs_open: bool,
    /// Texte d'aide de l'élément survolé à l'image précédente (zone d'information du bas).
    pub(crate) hint: String,
    pub(crate) hint_next: String,
    /// Contexte egui conservé pour les essais automatiques.
    pub(crate) ctx_handle: Option<egui::Context>,
    pub(crate) world: WorldUi,
    pub(crate) title: String,
    /// Script à ouvrir dans l'onglet Scripts (demandé depuis un panneau d'édition).
    pub(crate) open_script_request: Option<String>,
    pub(crate) anim: PaletteAnim,
    pub(crate) anim_on: bool,
    pub(crate) anim_start: Instant,
    pub(crate) anim_keys: HashMap<u64, u64>,
    /// Décors à plusieurs images (FID, orientation) → (nombre d'images, images par seconde) ; `None` : une seule image.
    pub(crate) frame_info: HashMap<(u32, u32), Option<(usize, u16)>>,
    pub(crate) selftest: Option<crate::selftest::SelfTest>,
}

impl MapperApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        game_dir: Option<PathBuf>,
        map: Option<String>,
        script: Option<String>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let (fusion_tx, fusion_rx) = mpsc::channel();
        let storage = cc.storage;
        // Les réglages vivent dans la base de registre ; à défaut (première ouverture après le changement), on lit l'ancien stockage.
        let saved = |k: &str| crate::settings::get(k).or_else(|| storage.and_then(|s| s.get_string(k)));
        let prefs = Prefs::load(|k| saved(k));
        let startup_game_dir = game_dir.clone().or_else(|| {
            saved(KEY_GAME_DIR)
                .filter(|_| prefs.open_last_game)
                .map(PathBuf::from)
        });
        let profiles = crate::profiles::ProfileState::load(|k| saved(k), startup_game_dir.clone());
        crate::i18n::set_lang(prefs.lang);
        cc.egui_ctx.set_theme(prefs.theme.preference());
        crate::ui_kit::apply_theme(&cc.egui_ctx);
        cc.egui_ctx.set_zoom_factor(prefs.ui_scale);
        // Réglages d'affichage mémorisés (ignorés pour les captures et auto-tests, qui partent toujours de l'état par défaut).
        let view = saved(KEY_VIEW)
            .and_then(|s| s.parse::<u16>().ok())
            .filter(|_| {
                std::env::var_os("FO_SHOT").is_none() && std::env::var_os("FO_SELFTEST").is_none()
            });
        let flag = |bit: u16, default: bool| view.map_or(default, |v| v & (1 << bit) != 0);
        let layers = saved(KEY_LAYERS)
            .and_then(|s| s.parse::<u8>().ok())
            .map_or(LayerSet::all(), LayerSet::from_bits);
        let mut app = Self {
            game: None,
            status: crate::i18n::t("Choisissez le dossier de Fallout 2 (menu Fichier).").into(),
            filter: String::new(),
            loading: None,
            current: None,
            elevation: 0,
            camera: Camera::default(),
            needs_fit: true,
            start_view: false,
            clip: None,
            docs: Vec::new(),
            close_batch: None,
            toolbar_tools: saved(KEY_TOOLBAR).map_or_else(crate::toolbar::default_tools, |s| {
                crate::toolbar::tools_from_string(&s)
            }),
            toolbar_dialog: false,
            fill: Default::default(),
            map_list_flat: saved(KEY_MAPLIST)
                .and_then(|s| s.parse::<u8>().ok())
                .is_some_and(|v| v & 1 != 0),
            map_sort_alpha: saved(KEY_MAPLIST)
                .and_then(|s| s.parse::<u8>().ok())
                .is_some_and(|v| v & 2 != 0),
            map_sort_desc: saved(KEY_MAPLIST)
                .and_then(|s| s.parse::<u8>().ok())
                .is_some_and(|v| v & 4 != 0),
            active_tab: 0,
            split: None,
            analysis: Default::default(),
            cross_drag: false,
            history_open: false,
            legend_open: false,
            log: Default::default(),
            window: crate::window_state::WindowState::load(),
            dialog: None,
            find: Default::default(),
            check: Default::default(),
            diff: Default::default(),
            msg: Default::default(),
            preview: Default::default(),
            recovery: Default::default(),
            export: Default::default(),
            walk: {
                let mut w = crate::walk_ui::WalkState::default();
                w.on = flag(8, w.on);
                w
            },
            library: Default::default(),
            minimap: {
                let mut m = crate::minimap::MinimapState::new();
                m.on = flag(7, m.on);
                m
            },
            procgen: Default::default(),
            links: Default::default(),
            recent_games: saved(KEY_RECENT_GAMES)
                .map(|s| crate::file_dialog::recent_from_string(&s))
                .unwrap_or_default(),
            recent_save_dirs: saved(KEY_RECENT_SAVE)
                .map(|s| crate::file_dialog::recent_from_string(&s))
                .unwrap_or_default(),
            profiles,
            rebinding: None,
            prefs_tab: 0,
            show_scripted: flag(0, false),
            show_left: flag(1, true),
            show_right: flag(2, true),
            layers,
            show_markers: flag(3, true),
            show_grid: flag(4, false),
            show_edges: flag(5, true),
            show_lights: flag(6, false),
            selection: Selection::default(),
            hover_hex: None,
            canvas_rect: Rect::NOTHING,
            textures: HashMap::new(),
            effects: crate::effects::EffectsCache::default(),
            tx,
            rx,
            fusion_tx,
            fusion_rx,
            fusion_pending: false,
            pending_map: map.or_else(|| saved(KEY_LAST_MAP).filter(|_| prefs.open_last_game)),
            mode: Mode::Map,
            scripts: ScriptsState::default(),
            edit: EditState::default(),
            confirm: None,
            file: FileUi::default(),
            prefs: prefs.clone(),
            prefs_open: false,
            hint: String::new(),
            hint_next: String::new(),
            ctx_handle: None,
            world: WorldUi::default(),
            title: String::new(),
            open_script_request: None,
            anim: PaletteAnim::default(),
            anim_on: true,
            anim_start: Instant::now(),
            anim_keys: HashMap::new(),
            frame_info: HashMap::new(),
            selftest: crate::selftest::SelfTest::from_env(),
        };
        if let Some(dir) = game_dir {
            if let Some(index) = app.profiles.items.iter().position(|p| p.map_dir == dir) {
                app.profiles.active = index;
            } else {
                app.profiles.items.push(crate::profiles::GameProfile {
                    name: dir
                        .file_name()
                        .map_or_else(|| "Fallout".into(), |n| n.to_string_lossy().into_owned()),
                    map_dir: dir.clone(),
                    test_dir: PathBuf::new(),
                });
                app.profiles.active = app.profiles.items.len() - 1;
            }
            app.open_game(&dir, &cc.egui_ctx);
        } else if let Some(dir) = app
            .profiles
            .active()
            .map(|p| p.map_dir.clone())
            .or(startup_game_dir)
        {
            app.open_game(&dir, &cc.egui_ctx);
        }
        // Pratique pour les essais : FO_EDIT=1 démarre en mode édition, FO_TOOL choisit l'outil.
        if std::env::var_os("FO_EDIT").is_some() || app.selftest.is_some() {
            app.edit.enabled = true;
            app.edit.tool = match std::env::var("FO_TOOL").as_deref() {
                Ok("floor") => crate::edit_ui::Tool::Floor,
                Ok("roof") => crate::edit_ui::Tool::Roof,
                Ok("place") => crate::edit_ui::Tool::Place,
                Ok("spatial") => crate::edit_ui::Tool::Spatial,
                _ => crate::edit_ui::Tool::Select,
            };
        }
        if let (Some(file), Some(g)) = (script, &app.game) {
            app.scripts.open_file(&g.data, &file);
            app.mode = Mode::Scripts;
        }
        app
    }

    pub(crate) fn open_game(&mut self, dir: &std::path::Path, ctx: &egui::Context) {
        match GameData::open(dir) {
            Ok(data) => {
                let data = Arc::new(data);
                crate::file_dialog::push_recent(&mut self.recent_games, dir);
                if let Some(profile) = self.profiles.items.get_mut(self.profiles.active) {
                    profile.map_dir = dir.to_path_buf();
                } else {
                    self.profiles.items.push(crate::profiles::GameProfile {
                        name: dir
                            .file_name()
                            .map_or_else(|| "Fallout".into(), |n| n.to_string_lossy().into_owned()),
                        map_dir: dir.to_path_buf(),
                        test_dir: PathBuf::new(),
                    });
                    self.profiles.active = self.profiles.items.len() - 1;
                }
                let maps = data.list_maps();
                self.status = format!(
                    "{} : {} {}",
                    dir.display(),
                    maps.len(),
                    crate::i18n::t("cartes")
                );
                let art = Arc::new(ArtLibrary::new(data.clone()));
                self.scripts.set_game(data.clone());
                self.game = Some(Game { data, art, maps });
                self.anim = PaletteAnim {
                    speed_factor: self
                        .game
                        .as_ref()
                        .map_or(1.0, |g| g.data.config.cycle_speed_factor),
                };
                self.anim_on = self
                    .game
                    .as_ref()
                    .is_none_or(|g| g.data.config.color_cycling);
                self.anim_keys.clear();
                self.current = None;
                self.docs.clear();
                self.active_tab = 0;
                self.split = None;
                self.textures.clear();
                self.effects = crate::effects::EffectsCache::default();
                if let Some(name) = self.pending_map.take()
                    && self
                        .game
                        .as_ref()
                        .is_some_and(|g| g.maps.iter().any(|m| m.file == name))
                {
                    self.load_map(&name, ctx);
                }
            }
            Err(e) => {
                self.status = format!(
                    "{} {} : {e}",
                    crate::i18n::t("Impossible d'ouvrir"),
                    dir.display()
                )
            }
        }
    }

    pub(crate) fn load_map(&mut self, file: &str, ctx: &egui::Context) {
        if self.game.is_none() {
            return;
        }
        // Déjà ouverte : on affiche son onglet ; sinon elle s'ouvre dans un nouvel onglet (sauf s'il n'y a pas de carte).
        if let Some(i) = (0..self.docs.len()).find(|&i| self.tab_file(i) == Some(file)) {
            self.switch_tab(i);
            return;
        }
        if self.current.is_some() {
            self.open_new_tab();
        }
        let Some(g) = &self.game else { return };
        self.loading = Some(file.to_string());
        self.status = format!("{} {file}…", crate::i18n::t("Chargement de"));
        spawn_load(
            g.data.clone(),
            g.art.clone(),
            file.to_string(),
            self.tx.clone(),
            ctx.clone(),
        );
    }

    fn poll_loader(&mut self) {
        while let Ok(result) = self.rx.try_recv() {
            match result {
                Ok(loaded) => {
                    self.status = format!(
                        "{} {} {} ms",
                        loaded.file,
                        crate::i18n::t("chargée en"),
                        loaded.millis
                    );
                    self.elevation = (0..3).find(|&e| loaded.scenes[e].is_some()).unwrap_or(0);
                    self.selection.clear();
                    self.start_view = true;
                    self.loading = None;
                    self.ensure_tab();
                    self.reset_edit_for_new_map();
                    if let Some(g) = &self.game {
                        self.scripts.set_map(&g.data, &loaded.map);
                    }
                    let restored = self
                        .recovery
                        .restoring
                        .take_if(|f| *f == loaded.file)
                        .is_some();
                    self.current = Some(loaded);
                    if restored {
                        // Copie de récupération : la carte diffère du fichier du jeu tant qu'elle n'est pas enregistrée.
                        self.edit.dirty = true;
                        self.status = crate::i18n::t("Carte restaurée depuis la copie de récupération : enregistrez-la pour la garder.").into();
                    }
                }
                Err((file, e)) => {
                    self.loading = None;
                    self.status = format!("{} {file} : {e}", crate::i18n::t("Erreur sur"));
                    if self.current.is_none() && self.docs.len() > 1 {
                        self.drop_active_tab();
                    }
                }
            }
        }
    }

    pub(crate) fn scene(&self) -> Option<&Arc<Scene>> {
        self.current.as_ref()?.scenes[self.elevation].as_ref()
    }

    pub(crate) fn object(&self, r: ObjectRef) -> Option<&MapObject> {
        self.current
            .as_ref()?
            .map
            .objects
            .get(r.elevation as usize)?
            .get(r.index as usize)
    }

    /// Retient la taille et la position de la fenêtre (hors réduite ou agrandie) pour la prochaine ouverture.
    fn track_window(&mut self, ctx: &egui::Context) {
        let (inner, outer, minimized, maximized) = ctx.input(|i| {
            let v = i.viewport();
            (
                v.inner_rect,
                v.outer_rect,
                v.minimized.unwrap_or(false),
                v.maximized.unwrap_or(false),
            )
        });
        if minimized || maximized {
            return;
        }
        if let Some(r) = inner {
            self.window.size = Some([r.width().round(), r.height().round()]);
        }
        if let Some(r) = outer {
            self.window.pos = Some([r.min.x.round(), r.min.y.round()]);
        }
    }

    /// Échap ferme la fenêtre ouverte la plus récente (et ne désélectionne pas).
    fn escape_closes_window(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            return;
        }
        let closed = if self.confirm.is_some() {
            self.confirm = None;
            true
        } else if self.file.new_map.is_some() {
            self.file.new_map = None;
            true
        } else if self.file.play_dialog {
            self.file.play_dialog = false;
            true
        } else if self.log.open {
            self.log.open = false;
            true
        } else if self.prefs_open {
            self.prefs_open = false;
            true
        } else if self.file.thumbs.open {
            self.toggle_thumbnails(ctx);
            true
        } else if self.world.maps_txt.is_some() {
            self.world.maps_txt = None;
            true
        } else {
            false
        };
        if closed {
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        }
    }

    /// Texte de la zone d'information du bas : aide de l'élément survolé, sinon objet sous le curseur ou aide de l'outil.
    fn info_text(&self) -> String {
        if !self.hint.is_empty() {
            return self.hint.clone();
        }
        if self.mode == Mode::Map && self.edit.enabled && self.hover_hex.is_none() {
            return crate::i18n::t(self.edit.tool.help()).to_string();
        }
        if let Some(h) = self.hover_hex {
            let (c, r) = geom::hex_col_row(h);
            let mut text = format!("{} {h} ({c}, {r})", crate::i18n::t("Hexagone"));
            if let Some(cur) = &self.current {
                let n = cur
                    .map
                    .objects
                    .get(self.elevation)
                    .map_or(0, |v| v.iter().filter(|o| o.tile() == h as i32).count());
                if n > 0 {
                    text += &format!(" — {n} {}", crate::i18n::t("objet(s)"));
                }
            }
            if self.edit.enabled {
                text += &format!(" — {}", crate::i18n::t(self.edit.tool.help()));
            }
            return text;
        }
        crate::i18n::t("Survolez un bouton pour afficher son aide.").to_string()
    }
}

impl eframe::App for MapperApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        crate::selftest::drive(self, ctx, raw_input);
        crate::shot::drive(self, ctx, raw_input);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.ctx_handle.get_or_insert_with(|| ctx.clone());
        if (ctx.zoom_factor() - self.prefs.ui_scale).abs() > 0.001 {
            ctx.set_zoom_factor(self.prefs.ui_scale);
        }
        crate::i18n::set_lang(self.prefs.lang);
        if ctx.options(|o| o.theme_preference) != self.prefs.theme.preference() {
            ctx.set_theme(self.prefs.theme.preference());
        }
        self.hint = std::mem::take(&mut self.hint_next);
        self.prefs_window(&ctx);
        self.profiles_window(&ctx);
        self.toolbar_window(&ctx);
        self.poll_loader();
        self.poll_fusion_link();
        self.poll_tile_analysis(&ctx);
        self.escape_closes_window(&ctx);
        self.handle_shortcuts(&ctx);
        if ctx.input(|i| i.viewport().close_requested())
            && self.first_dirty_tab().is_some()
            && self.confirm.is_none()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.quit_requested();
        }
        self.confirm_dialog(&ctx);
        self.history_window(&ctx);
        self.legend_window(&ctx);
        self.log_window(&ctx);
        self.dialog_window(&ctx);
        self.recovery_tick(&ctx);
        self.recovery_window(&ctx);
        self.export_window(&ctx);
        self.library_window(&ctx);
        self.generator_window(&ctx);
        self.find_window(&ctx);
        self.check_window(&ctx);
        self.diff_window(&ctx);
        self.msg_window(&ctx);
        self.track_window(&ctx);
        self.file_windows(&ctx);
        self.world_windows(&ctx);
        self.links_window(&ctx);
        let title = match &self.current {
            Some(c) => format!(
                "FalloutMapper — {}{}",
                c.file,
                if self.edit.dirty { " *" } else { "" }
            ),
            None => "FalloutMapper".to_string(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui, &ctx));
        egui::Panel::bottom("status").show(ui, |ui| {
            let info = self.info_text();
            ui.add_space(2.0);
            // Une seule ligne, hauteur fixe : la zone de la carte ne doit pas bouger quand le texte change.
            ui.add_sized(
                [ui.available_width(), 22.0],
                egui::Label::new(egui::RichText::new(info).color(ui.visuals().text_color()))
                    .truncate(),
            );
            ui.separator();
            ui.horizontal(|ui| {
                if self.edit.dirty {
                    ui.colored_label(
                        Color32::from_rgb(255, 190, 80),
                        crate::i18n::t("● modifiée"),
                    );
                }
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(h) = self.hover_hex {
                        let (c, r) = geom::hex_col_row(h);
                        ui.label(format!("{} {h} ({c}, {r})", crate::i18n::t("hex")));
                    }
                    ui.label(format!(
                        "{} {:.0} %",
                        crate::i18n::t("zoom"),
                        self.camera.zoom * 100.0
                    ));
                });
            });
        });
        if self.show_left && self.mode == Mode::Map {
            egui::Panel::left("maps")
                .default_size(260.0)
                .show(ui, |ui| self.map_list(ui, &ctx));
        }
        if let (Some(file), Some(g)) = (self.open_script_request.take(), &self.game) {
            self.scripts.open_file(&g.data, &file);
            self.mode = Mode::Scripts;
        }
        self.tab_bar(ui);
        match self.mode {
            Mode::Map => {
                let request = if !self.show_right {
                    None
                } else {
                    egui::Panel::right("inspector")
                        .resizable(true)
                        .default_size(340.0)
                        .min_size(240.0)
                        .show(ui, |ui| {
                            // Le contenu s'adapte à la largeur du panneau : il ne change pas de largeur selon la sélection.
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    if self.edit.enabled {
                                        self.edit_panel(ui);
                                        ui.separator();
                                    }
                                    let request = self.inspector(ui);
                                    // Marge sous le contenu : le dernier élément ne colle pas au bord du panneau.
                                    ui.add_space(24.0);
                                    request
                                })
                                .inner
                        })
                        .inner
                };
                egui::CentralPanel::default().show(ui, |ui| self.map_area(ui));
                if let (Some(file), Some(g)) = (request, &self.game) {
                    self.scripts.open_file(&g.data, &file);
                    self.mode = Mode::Scripts;
                }
            }
            Mode::World => self.world_tab(ui),
            Mode::Scripts => {
                if !self.scripts.editing_source() {
                    egui::Panel::right("procedures")
                        .default_size(300.0)
                        .resizable(true)
                        .min_size(140.0)
                        .max_size((ui.available_width() * 0.30).max(140.0))
                        .show(ui, |ui| self.scripts.procedures_ui(ui));
                }
                if let Some(g) = &self.game {
                    egui::Panel::left("script_list")
                        .default_size(320.0)
                        .resizable(true)
                        .min_size(140.0)
                        .max_size((ui.available_width() * 0.40).max(140.0))
                        .show(ui, |ui| self.scripts.list_ui(ui, &g.data));
                }
                egui::CentralPanel::default().show(ui, |ui| self.scripts.code_ui(ui));
            }
        }
    }

    /// La mémoire d'egui (ouverture des sections…) n'est pas conservée : les réglages sont dans la base de registre.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        // Captures et essais automatiques : on n'enregistre ni la position de la fenêtre ni les préférences de l'utilisateur.
        if std::env::var_os("FO_SHOT").is_some()
            || std::env::var_os("FO_SELFTEST").is_some()
            || std::env::var_os("FO_HIDDEN").is_some()
        {
            return;
        }
        self.window.save();
        if let Some(g) = &self.game {
            crate::settings::set_string(KEY_GAME_DIR, g.data.root.display().to_string());
        }
        if let Some(c) = &self.current {
            crate::settings::set_string(KEY_LAST_MAP, c.file.clone());
        }
        self.profiles.save(&mut |k, v| crate::settings::set_string(k, v));
        crate::settings::set_string(
            KEY_RECENT_GAMES,
            crate::file_dialog::recent_to_string(&self.recent_games),
        );
        crate::settings::set_string(
            KEY_RECENT_SAVE,
            crate::file_dialog::recent_to_string(&self.recent_save_dirs),
        );
        crate::settings::set_string(KEY_LAYERS, self.layers.bits().to_string());
        crate::settings::set_string(
            KEY_TOOLBAR,
            crate::toolbar::tools_to_string(&self.toolbar_tools),
        );
        crate::settings::set_string(
            KEY_MAPLIST,
            (u8::from(self.map_list_flat)
                | u8::from(self.map_sort_alpha) << 1
                | u8::from(self.map_sort_desc) << 2)
                .to_string(),
        );
        let bits = [
            self.show_scripted,
            self.show_left,
            self.show_right,
            self.show_markers,
            self.show_grid,
            self.show_edges,
            self.show_lights,
            self.minimap.on,
            self.walk.on,
        ];
        crate::settings::set_string(
            KEY_VIEW,
            bits.iter()
                .enumerate()
                .fold(0u16, |acc, (i, &b)| acc | (u16::from(b) << i))
                .to_string(),
        );
        self.prefs.save(&mut |k, v| crate::settings::set_string(k, v));
    }
}
