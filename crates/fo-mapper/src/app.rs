use crate::edit_ui::EditState;
use crate::file_ui::FileUi;
use crate::loader::{LoadResult, LoadedMap, spawn_load};
use crate::prefs::Prefs;
use crate::scripts::ScriptsState;
use crate::selection::Selection;
use crate::view::Camera;
use crate::world_ui::WorldUi;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2};
use fo_core::geom;
use fo_core::map::{MapObject, SCRIPT_TYPE_NAMES};
use fo_core::{ArtLibrary, Fid, GameData, MapEntry, PaletteAnim};
use fo_scene::{Layer, LayerSet, MarkerKind, ObjectRef, Scene};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

const KEY_GAME_DIR: &str = "game_dir";
const KEY_LAST_MAP: &str = "last_map";
const KEY_LAYERS: &str = "layers2";
const KEY_VIEW: &str = "view_flags";
const KEY_MAPLIST: &str = "maplist_mode";
const KEY_TOOLBAR: &str = "toolbar_tools";
const KEY_RECENT_GAMES: &str = "recent_games";
const KEY_RECENT_SAVE: &str = "recent_save_dirs";
/// Nombre maximal de textures créées par image affichée (évite un blocage au premier affichage).
const TEXTURE_BUDGET: usize = 400;

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
    filter: String,
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
    rx: Receiver<LoadResult>,
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
    title: String,
    /// Script à ouvrir dans l'onglet Scripts (demandé depuis un panneau d'édition).
    pub(crate) open_script_request: Option<String>,
    anim: PaletteAnim,
    pub(crate) anim_on: bool,
    anim_start: Instant,
    anim_keys: HashMap<u64, u64>,
    /// Décors à plusieurs images (FID, orientation) → (nombre d'images, images par seconde) ; `None` : une seule image.
    frame_info: HashMap<(u32, u32), Option<(usize, u16)>>,
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

    /// Image courante d'un décor animé dans l'aperçu du jeu : le sprite de l'image voulue et la position où le dessiner.
    /// Seuls les décors ordinaires tournent ; les portes, escaliers, murs et créatures gardent l'image de la carte.
    fn object_frame(
        &mut self,
        item: &fo_scene::DrawItem,
        ms: u64,
    ) -> Option<(Arc<fo_core::Sprite>, i32, i32)> {
        let r = item.object?;
        let o = self.object(r)?;
        if o.object_type() != Some(fo_core::ObjectType::Scenery)
            || o.subtype != fo_core::formats::scenery_kind::GENERIC
        {
            return None;
        }
        let (fid, orient) = (o.fid(), o.orientation().max(0) as u32);
        let art = self.game.as_ref()?.art.clone();
        let info = *self.frame_info.entry((fid, orient)).or_insert_with(|| {
            let (path, _) = art.object_sprite(fo_core::Fid(fid), orient, 0)?;
            let frm = art.frm(&path)?;
            let dir = if frm.directions.len() > 1 {
                orient as usize
            } else {
                0
            };
            let frames = frm.directions.get(dir)?.frames.len();
            (frames > 1).then_some((frames, frm.fps))
        });
        let (frames, fps) = info?;
        let frame = (ms * u64::from(fps.max(1)) / 1000) as usize % frames;
        let (_, sprite) = art.object_sprite(fo_core::Fid(fid), orient, frame)?;
        // Même point d'ancrage que l'image de la carte : on ne déplace que de la différence d'origine des deux sprites.
        let (x, y) = (
            item.x + sprite.origin_x - item.sprite.origin_x,
            item.y + sprite.origin_y - item.sprite.origin_y,
        );
        Some((sprite, x, y))
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

    /// Entrée de menu avec le raccourci réel (celui des options) affiché à droite.
    fn menu_item(
        &self,
        text: &'static str,
        action: Option<crate::prefs::KeyAction>,
        enabled: bool,
    ) -> egui::Button<'static> {
        let _ = enabled;
        let button = egui::Button::new(crate::i18n::t(text));
        match action {
            Some(a) => button.shortcut_text(self.prefs.shortcut_text(a)),
            None => button,
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Les fonctions d'une carte locale (enregistrer, éditer, vérifier, calques…) n'ont de sens que dans l'onglet Carte.
        let local = self.mode == Mode::Map;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(crate::i18n::t("Fichier"), |ui| {
                if ui.button(crate::i18n::t("Ouvrir le dossier du jeu…")).clicked() {
                    ui.close();
                    self.open_dialog(crate::file_dialog::Purpose::OpenGame);
                }
                if ui.button("Installations et profils de test…").clicked() {
                    ui.close();
                    self.profiles.open = true;
                }
                ui.separator();
                if ui.add_enabled(self.game.is_some(), egui::Button::new(crate::i18n::t("Générer une carte…"))).clicked() {
                    ui.close();
                    self.open_generator();
                }
                if ui.add_enabled(self.game.is_some(), self.menu_item("Nouvelle carte…", Some(crate::prefs::KeyAction::NewMap), true)).clicked() {
                    ui.close();
                    self.open_new_map_dialog();
                }
                if ui.add_enabled(self.game.is_some(), self.menu_item("Cartes en miniatures", Some(crate::prefs::KeyAction::Thumbnails), true)).clicked() {
                    ui.close();
                    self.toggle_thumbnails(ctx);
                }
                if ui.add_enabled(local && self.current.is_some(), self.menu_item("Fermer la carte", Some(crate::prefs::KeyAction::CloseMap), true)).clicked() {
                    ui.close();
                    self.request_close();
                }
                ui.separator();
                let has_map = self.current.is_some() && local;
                if ui.add_enabled(has_map, self.menu_item("Enregistrer", Some(crate::prefs::KeyAction::Save), true)).clicked() {
                    ui.close();
                    self.save_current();
                }
                if ui.add_enabled(has_map, egui::Button::new(crate::i18n::t("Enregistrer sous…"))).clicked() {
                    ui.close();
                    self.save_current_as();
                }
                if ui.add_enabled(self.current.is_some() && self.scene().is_some(), egui::Button::new(crate::i18n::t("Exporter une image…"))).clicked() {
                    ui.close();
                    self.export.open = true;
                }
                ui.separator();
                if ui.add_enabled(has_map, egui::Button::new(crate::i18n::t("Enregistrer et jouer")).shortcut_text("F5")).clicked() {
                    ui.close();
                    self.request_play();
                }
                if ui.add_enabled(has_map && !self.fusion_pending, egui::Button::new(crate::i18n::t("Recharger dans Fallout Fusion"))).clicked() {
                    ui.close();
                    self.request_fusion_reload();
                }
                if ui.add_enabled(self.game.is_some(), egui::Button::new(crate::i18n::t("Restaurer la configuration de lancement"))).clicked() {
                    ui.close();
                    self.restore_ddraw();
                }
                ui.separator();
                if ui.add(self.menu_item("Options…", Some(crate::prefs::KeyAction::Options), true)).clicked() {
                    ui.close();
                    self.prefs_open = true;
                }
                ui.separator();
                if ui.button(crate::i18n::t("Quitter")).clicked() {
                    ui.close();
                    // Les modifications non enregistrées passent d'abord par la confirmation ; sinon on ferme tout de suite.
                    self.quit_requested();
                    if self.confirm.is_none() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            });
            ui.menu_button(crate::i18n::t("Édition"), |ui| {
                ui.add_enabled(local, egui::Checkbox::new(&mut self.edit.enabled, crate::i18n::t("Mode édition")));
                ui.separator();
                if ui.add_enabled(local && self.edit.can_undo(), self.menu_item("Annuler", Some(crate::prefs::KeyAction::Undo), true)).clicked() {
                    ui.close();
                    self.undo();
                }
                if ui.add_enabled(local && self.edit.can_redo(), self.menu_item("Rétablir", Some(crate::prefs::KeyAction::Redo), true)).clicked() {
                    ui.close();
                    self.redo();
                }
                if ui.add_enabled(local, self.menu_item("Historique…", Some(crate::prefs::KeyAction::History), true)).clicked() {
                    ui.close();
                    self.history_open = true;
                }
                if ui.add_enabled(local && self.current.is_some(), self.menu_item("Rechercher un objet…", Some(crate::prefs::KeyAction::Find), true)).clicked() {
                    ui.close();
                    self.find.open = true;
                }
                if ui.add_enabled(self.game.is_some(), egui::Button::new(crate::i18n::t("Bibliothèque de motifs…"))).clicked() {
                    ui.close();
                    self.library.open = true;
                }
                if ui.add_enabled(local && self.current.is_some(), egui::Button::new(crate::i18n::t("Comparer avec une version…"))).clicked() {
                    ui.close();
                    self.diff.open = true;
                }
                if ui.add_enabled(local && self.current.is_some(), self.menu_item("Vérifier la carte…", Some(crate::prefs::KeyAction::Check), true)).clicked() {
                    ui.close();
                    self.check.open = true;
                    self.run_check();
                }
                ui.separator();
                if ui.add_enabled(self.game.is_some() && !self.analysis.running(), egui::Button::new(crate::i18n::t("Analyser les tuiles de toutes les cartes"))).clicked() {
                    ui.close();
                    self.start_tile_analysis(ctx);
                }
                if ui.add_enabled(self.game.is_some(), egui::Button::new(crate::i18n::t("Textes (.msg)…"))).clicked() {
                    ui.close();
                    self.msg.open = true;
                }
                if ui.add_enabled(self.game.is_some(), egui::Button::new(crate::i18n::t("Cartes du jeu (maps.txt)…"))).clicked() {
                    ui.close();
                    self.open_maps_txt();
                }
                if ui.add_enabled(self.game.is_some(), egui::Button::new(crate::i18n::t("Carte du monde…"))).clicked() {
                    ui.close();
                    self.mode = Mode::World;
                }
                if ui.add_enabled(self.game.is_some(), egui::Button::new(crate::i18n::t("Liaisons entre cartes…"))).clicked() {ui.close();self.links.open=true;}
            });
            ui.menu_button(crate::i18n::t("Affichage"), |ui| {
                ui.add_enabled_ui(local, |ui| {
                ui.menu_button(crate::i18n::t("Calques"), |ui| {
                    for layer in Layer::ALL {
                        let mut on = self.layers.contains(layer);
                        if ui.checkbox(&mut on, crate::i18n::t(layer.label())).changed() {
                            self.layers.set(layer, on);
                        }
                    }
                });
                ui.separator();
                ui.checkbox(&mut self.show_markers, crate::i18n::t("Repères (sorties, scripts…)"));
                ui.checkbox(&mut self.minimap.on, crate::i18n::t("Minicarte"));
                ui.checkbox(&mut self.show_grid, crate::i18n::t("Grille hexagonale"));
                ui.checkbox(&mut self.show_edges, crate::i18n::t("Limites de défilement (.edg)"));
                ui.checkbox(&mut self.show_lights, crate::i18n::t("Éclairage (halos des lumières)"));
                ui.checkbox(&mut self.walk.on, crate::i18n::t("Accessibilité (hexagones bloqués et atteignables)")).on_hover_text(crate::i18n::t("Rouge : bloqué ; orange : créature ; vert : atteignable à pied depuis le départ du joueur ; bleu : libre mais hors d'atteinte."));
                ui.separator();
                self.preview_menu(ui);
                ui.separator();
                ui.checkbox(&mut self.anim_on, crate::i18n::t("Animer la palette (eau, feu, écrans…)"));
                ui.separator();
                ui.checkbox(&mut self.show_scripted, crate::i18n::t("Surligner les objets avec un script"));
                if ui.button(crate::i18n::t("Légende des repères…")).clicked() {
                    ui.close();
                    self.legend_open = true;
                }
                ui.separator();
                if ui.add_enabled(local, self.menu_item("Ajuster à la fenêtre", Some(crate::prefs::KeyAction::Fit), true)).clicked() {
                    self.needs_fit = true;
                    ui.close();
                }
                });
            });
            ui.menu_button(crate::i18n::t("Fenêtre"), |ui| {
                if ui.button(crate::i18n::t("Journal…")).clicked() {
                    ui.close();
                    self.log.open = true;
                }
                ui.separator();
                ui.checkbox(&mut self.show_left, crate::i18n::t("Panneau des cartes"));
                ui.checkbox(&mut self.show_right, crate::i18n::t("Panneau d'édition et d'infos"));
                ui.separator();
                ui.checkbox(&mut self.window.remember_size, crate::i18n::t("Mémoriser les dimensions"));
                ui.checkbox(&mut self.window.remember_pos, crate::i18n::t("Mémoriser la position"));
            });
            ui.separator();
            for (mode, icon, tip) in [(Mode::World, crate::icons::Icon::World, "Carte du monde"), (Mode::Map, crate::icons::Icon::Map, "Carte"), (Mode::Scripts, crate::icons::Icon::Script, "Scripts")] {
                let r = crate::icons::icon_button(ui, icon, egui::Vec2::new(36.0, 28.0), self.mode == mode, true);
                if r.on_hover_text(crate::i18n::t(tip)).clicked() {
                    self.mode = mode;
                }
            }
        });
    }

    fn map_list(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let t = crate::i18n::t;
        ui.heading(t("Cartes"));
        ui.add(
            egui::TextEdit::singleline(&mut self.filter)
                .hint_text(t("Filtrer…"))
                .desired_width(f32::INFINITY),
        );
        ui.separator();
        // Barre du bas : vue (dossiers / liste), critère de tri (alphabétique / ordre du jeu) et sens.
        egui::Panel::bottom("maplist_bar")
            .show_separator_line(true)
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.selectable_value(&mut self.map_list_flat, false, t("Dossiers"))
                        .on_hover_text(t("Cartes regroupées par lieu"));
                    ui.selectable_value(&mut self.map_list_flat, true, t("Liste"))
                        .on_hover_text(t("Toutes les cartes à la suite"));
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.selectable_value(&mut self.map_sort_alpha, true, t("A–Z"))
                        .on_hover_text(t("Ordre alphabétique"));
                    ui.selectable_value(&mut self.map_sort_alpha, false, t("Ordre du jeu"))
                        .on_hover_text(t("Ordre des cartes dans le jeu (numéro de maps.txt)"));
                    let (arrow, tip) = if self.map_sort_desc {
                        ("↓", t("Décroissant"))
                    } else {
                        ("↑", t("Croissant"))
                    };
                    if ui.button(arrow).on_hover_text(tip).clicked() {
                        self.map_sort_desc = !self.map_sort_desc;
                    }
                });
                ui.add_space(2.0);
            });
        let Some(game) = &self.game else {
            ui.label(t("Aucun jeu ouvert."));
            return;
        };
        let needle = self.filter.to_lowercase();
        let mut to_load = None;
        let current = self.current.as_ref().map(|c| c.file.clone());
        let loading = self.loading.clone();
        let (flat, alpha, desc) = (self.map_list_flat, self.map_sort_alpha, self.map_sort_desc);
        let matches = |m: &fo_core::MapEntry| {
            needle.is_empty()
                || m.file.contains(&needle)
                || m.display
                    .as_deref()
                    .is_some_and(|d| d.to_lowercase().contains(&needle))
                || m.group_name
                    .as_deref()
                    .is_some_and(|d| d.to_lowercase().contains(&needle))
        };
        // Nom affiché d'une carte : « Lieu — carte » quand elle est dans un groupe, sinon son nom du jeu, sinon son fichier.
        let full_label = |m: &fo_core::MapEntry| match (&m.group_name, &m.place) {
            (Some(g), Some(p)) => format!("{g} — {p}"),
            _ => m
                .display
                .clone()
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| m.file.clone()),
        };
        // Clé de tri d'une carte : son nom en minuscules, ou son numéro dans le jeu (puis son fichier).
        let key = |m: &fo_core::MapEntry, label: &str| -> (u32, String) {
            if alpha {
                (0, label.to_lowercase())
            } else {
                (m.number.unwrap_or(u32::MAX), m.file.clone())
            }
        };
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let row = |ui: &mut egui::Ui,
                           m: &fo_core::MapEntry,
                           label: String,
                           to_load: &mut Option<String>| {
                    let selected =
                        Some(&m.file) == current.as_ref() || Some(&m.file) == loading.as_ref();
                    if ui
                        .selectable_label(selected, label)
                        .on_hover_text(&m.file)
                        .clicked()
                    {
                        *to_load = Some(m.file.clone());
                    }
                };
                if flat {
                    let mut all: Vec<(&fo_core::MapEntry, String)> = game
                        .maps
                        .iter()
                        .filter(|m| matches(m))
                        .map(|m| (m, full_label(m)))
                        .collect();
                    all.sort_by_key(|(m, l)| key(m, l));
                    if desc {
                        all.reverse();
                    }
                    for (m, label) in all {
                        row(ui, m, label, &mut to_load);
                    }
                    return;
                }
                // Vue par dossiers : un groupe par lieu, puis les cartes sans groupe ; le tri s'applique aux dossiers et à leur contenu.
                struct Item<'a> {
                    title: String,
                    group: Option<u32>,
                    members: Vec<&'a fo_core::MapEntry>,
                }
                let mut items: Vec<Item> = Vec::new();
                let mut seen: std::collections::HashMap<u32, usize> =
                    std::collections::HashMap::new();
                for m in game.maps.iter().filter(|m| matches(m)) {
                    match m.group {
                        Some(g) => {
                            let at = *seen.entry(g).or_insert_with(|| {
                                items.push(Item {
                                    title: m.group_name.clone().unwrap_or_default(),
                                    group: Some(g),
                                    members: Vec::new(),
                                });
                                items.len() - 1
                            });
                            items[at].members.push(m);
                        }
                        None => items.push(Item {
                            title: full_label(m),
                            group: None,
                            members: vec![m],
                        }),
                    }
                }
                for it in &mut items {
                    it.members
                        .sort_by_key(|m| key(m, m.place.as_deref().unwrap_or(&m.file)));
                    if desc {
                        it.members.reverse();
                    }
                }
                items.sort_by_key(|it| {
                    if alpha {
                        (0, it.title.to_lowercase())
                    } else {
                        it.members
                            .iter()
                            .map(|m| (m.number.unwrap_or(u32::MAX), m.file.clone()))
                            .min()
                            .unwrap_or((u32::MAX, String::new()))
                    }
                });
                if desc {
                    items.reverse();
                }
                for it in items {
                    match it.group {
                        Some(g) => {
                            let has_current = it.members.iter().any(|x| {
                                Some(&x.file) == current.as_ref()
                                    || Some(&x.file) == loading.as_ref()
                            });
                            let title = format!("{} ({})", it.title, it.members.len());
                            egui::CollapsingHeader::new(title)
                                .id_salt(("map_group", g))
                                .default_open(has_current || !needle.is_empty())
                                .show(ui, |ui| {
                                    for x in &it.members {
                                        row(
                                            ui,
                                            x,
                                            x.place.clone().unwrap_or_else(|| x.file.clone()),
                                            &mut to_load,
                                        );
                                    }
                                });
                        }
                        None => {
                            if let Some(m) = it.members.first() {
                                let text = m
                                    .display
                                    .clone()
                                    .filter(|d| !d.is_empty())
                                    .unwrap_or_else(|| m.file.clone());
                                row(ui, m, text, &mut to_load);
                            }
                        }
                    }
                }
            });
        if let Some(f) = to_load {
            self.request_load(&f, ctx);
        }
    }

    /// Panneau de droite en mode carte. Renvoie le script à ouvrir si l'utilisateur le demande.
    fn inspector(&self, ui: &mut egui::Ui) -> Option<String> {
        let Some(cur) = &self.current else {
            ui.label(crate::i18n::t("Aucune carte."));
            return None;
        };
        let h = &cur.map.header;
        ui.heading(h.name());
        egui::Grid::new("header").num_columns(2).show(ui, |ui| {
            let mut row = |k: &'static str, v: String| {
                ui.label(crate::i18n::t(k));
                ui.label(v);
                ui.end_row();
            };
            row("Version", h.version.to_string());
            row("Numéro", h.map_number.to_string());
            row("Obscurité", h.darkness.to_string());
            row("Script de carte", h.script_id.to_string());
            row(
                "Départ",
                format!(
                    "{} {} ({} {})",
                    crate::i18n::t("hex"),
                    h.player_tile,
                    crate::i18n::t("élév."),
                    h.player_elevation
                ),
            );
            row(
                "Variables",
                format!(
                    "{} {} / {} {}",
                    cur.map.globals.len(),
                    crate::i18n::t("glob."),
                    cur.map.locals.len(),
                    crate::i18n::t("loc.")
                ),
            );
            row(
                "Objets",
                format!(
                    "{} ({} {})",
                    cur.map.object_count(),
                    cur.map.total_object_count(),
                    crate::i18n::t("avec inventaires")
                ),
            );
        });
        ui.collapsing(crate::i18n::t("Scripts"), |ui| {
            for (name, list) in SCRIPT_TYPE_NAMES.iter().zip(&cur.map.scripts) {
                ui.label(format!("{} : {}", crate::i18n::t(name), list.count));
            }
        });
        if let Some(scene) = self.scene()
            && !scene.missing_art.is_empty()
        {
            ui.collapsing(
                format!(
                    "{} ({})",
                    crate::i18n::t("Art introuvable"),
                    scene.missing_art.len()
                ),
                |ui| {
                    for (fid, n) in scene.missing_art.iter().take(50) {
                        ui.label(format!("FID {fid:08X} ×{n}"));
                    }
                },
            );
        }
        ui.separator();
        ui.heading(crate::i18n::t("Sélection"));
        let selected_count = self.selection.objects().len()
            + self.selection.tiles().len()
            + self.selection.hexes().len();
        if selected_count > 1 {
            ui.label(format!(
                "{} {} {} {}",
                self.selection.objects().len(),
                crate::i18n::t("objet(s) et"),
                self.selection.tiles().len(),
                crate::i18n::t("case(s) sélectionnés : voir le panneau d'édition.")
            ));
        }
        let Some(obj) = self.selection.primary().and_then(|r| self.object(r)) else {
            ui.label(crate::i18n::t("Cliquez sur un objet."));
            return None;
        };
        self.object_details(ui, obj);
        let game = self.game.as_ref()?;
        let entry = cur.map.find_script(obj.script_id())?;
        let file = game.data.script_file(entry.list_index().max(0) as usize)?;
        ui.separator();
        let open = ui
            .button(format!("{} {file}", crate::i18n::t("Voir le script")))
            .clicked();
        // De la marge sous le dernier bouton : il ne doit pas toucher le bord du panneau.
        ui.add_space(16.0);
        open.then_some(file)
    }

    /// Nom du prototype dans la langue du jeu (`pro_*.msg`), si le prototype existe et porte un nom.
    pub(crate) fn proto_name(&self, pid: u32) -> Option<String> {
        let g = self.game.as_ref()?;
        g.data
            .proto_entry(pid)
            .map(|e| e.name)
            .filter(|n| !n.is_empty())
    }

    fn object_details(&self, ui: &mut egui::Ui, o: &MapObject) {
        let fid = Fid(o.fid());
        egui::Grid::new("object").num_columns(2).show(ui, |ui| {
            let mut row = |k: &'static str, v: String| {
                ui.label(crate::i18n::t(k));
                ui.label(v);
                ui.end_row();
            };
            if let Some(name) = self.proto_name(o.pid()) {
                row("Nom", name);
            }
            row(
                "Type",
                o.object_type()
                    .map_or("?".into(), |t| crate::i18n::t(t.label()).into()),
            );
            row("PID", format!("{:08X}", o.pid()));
            row("FID", format!("{:08X}", o.fid()));
            row(
                "Hexagone",
                format!(
                    "{} ({}, {})",
                    o.tile(),
                    geom::hex_col_row(o.tile().max(0) as u32).0,
                    geom::hex_col_row(o.tile().max(0) as u32).1
                ),
            );
            row("Orientation", o.orientation().to_string());
            row("Drapeaux", format!("{:08X}", o.flags()));
            row("Script", o.script_id().to_string());
            row("Sous-type", o.subtype.to_string());
            if let Some(g) = self.game.as_ref() {
                row(
                    "Art",
                    g.art
                        .fid_paths(fid, o.orientation().clamp(0, 5) as u32)
                        .first()
                        .cloned()
                        .unwrap_or_default(),
                );
            }
            if let Some(d) = o.exit_destination() {
                row(
                    "Destination",
                    format!(
                        "{} {} {} {} {} {}",
                        crate::i18n::t("carte"),
                        d.map,
                        crate::i18n::t("hex"),
                        d.tile,
                        crate::i18n::t("élév."),
                        d.elevation
                    ),
                );
            }
        });
        if !o.inventory.is_empty() {
            ui.collapsing(
                format!("{} ({})", crate::i18n::t("Inventaire"), o.inventory.len()),
                |ui| {
                    for i in &o.inventory {
                        ui.label(format!("{} × {:08X}", i.count, i.object.pid()));
                    }
                },
            );
        }
    }

    pub(crate) fn canvas(&mut self, ui: &mut egui::Ui) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let view = response.rect;
        self.canvas_rect = view;
        let toolbar_ctx = ui.ctx().clone();
        self.map_toolbars(&toolbar_ctx, ui.layer_id(), view);
        self.minimap_overlay(&toolbar_ctx, ui.layer_id(), view);
        painter.rect_filled(view, 0.0, Color32::from_rgb(14, 14, 16));

        if self.loading.is_some() {
            painter.text(
                view.center(),
                egui::Align2::CENTER_CENTER,
                crate::i18n::t("Chargement…"),
                egui::FontId::proportional(22.0),
                Color32::GRAY,
            );
        }
        let Some(scene) = self.scene().cloned() else {
            if self.loading.is_none() {
                painter.text(
                    view.center(),
                    egui::Align2::CENTER_CENTER,
                    crate::i18n::t("Choisissez une carte dans la liste"),
                    egui::FontId::proportional(18.0),
                    Color32::GRAY,
                );
            }
            return;
        };

        if self.start_view && view.width() > 10.0 {
            self.camera.zoom = 0.5;
            let start = self
                .current
                .as_ref()
                .map(|c| c.map.header.player_tile)
                .filter(|&t| (0..geom::HEX_COUNT as i32).contains(&t));
            self.camera.center = match start {
                Some(t) => {
                    let (x, y) = geom::hex_to_screen(t as u32);
                    Pos2::new(x as f32, y as f32)
                }
                None => Pos2::new(
                    (scene.bounds.0 + scene.bounds.2 / 2) as f32,
                    (scene.bounds.1 + scene.bounds.3 / 2) as f32,
                ),
            };
            self.start_view = false;
            self.needs_fit = false;
        }
        if self.needs_fit && view.width() > 10.0 {
            self.camera.fit(view, scene.bounds);
            self.needs_fit = false;
        }
        let world_pos = response
            .hover_pos()
            .map(|p| self.camera.screen_to_world(view, p));
        self.hover_hex = world_pos.and_then(|w| geom::screen_to_hex(w.x as i32, w.y as i32));
        let editing = self.edit.enabled;
        let left_taken = editing && self.canvas_edit_input(ui, &response, world_pos, &scene);
        if response.clicked_by(egui::PointerButton::Middle)
            && let Some(p) = response.hover_pos()
        {
            // Bouton du milieu : retour au zoom 100 %, sous le curseur.
            let factor = 1.0 / self.camera.zoom;
            self.camera.zoom_at(view, p, factor);
        }
        let pan = response.dragged_by(egui::PointerButton::Middle)
            || response.dragged_by(egui::PointerButton::Secondary)
            || (response.dragged_by(egui::PointerButton::Primary) && !left_taken);
        if pan {
            self.camera.pan_by_screen(response.drag_delta());
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0
                && let Some(p) = response.hover_pos()
            {
                self.camera.zoom_at(view, p, (scroll * 0.0025).exp());
            }
        }

        let cam = self.camera;
        let world_view = Rect::from_min_max(
            cam.screen_to_world(view, view.min),
            cam.screen_to_world(view, view.max),
        );

        let mut created = 0;
        let mut pending = false;
        let mut any_animated = false;
        let ctx = ui.ctx().clone();
        // Aperçu du jeu : lumière par hexagone, toits masqués autour du point de vue, sans repères d'édition.
        let preview = self.preview_prepare();
        let previewing = preview.is_some();
        // Dans l'aperçu du jeu, les décors à plusieurs images tournent en boucle (ventilateurs, enseignes…).
        let anim_ms = self.anim_start.elapsed().as_millis() as u64;
        let layers = self.layers;
        self.effects.prune(&ctx);
        let mut effects = crate::effects::EffectsFrame::new(&scene, [14, 14, 16, 255]);
        for item in scene.items.iter().filter(|i| {
            layers.contains(i.layer)
                && !(previewing
                    && matches!(i.layer, fo_scene::Layer::Exit | fo_scene::Layer::Blocker))
        }) {
            if preview.as_ref().is_some_and(|p| p.hides(item)) {
                continue;
            }
            let moving = if previewing {
                self.object_frame(item, anim_ms)
            } else {
                None
            };
            if moving.is_some() {
                any_animated = true;
            }
            let (sprite, ix, iy) = moving.unwrap_or_else(|| (item.sprite.clone(), item.x, item.y));
            if let Some(effects) = &mut effects {
                let mut posed = item.clone();
                posed.sprite = sprite.clone();
                posed.x = ix;
                posed.y = iy;
                effects.push(
                    posed,
                    if previewing {
                        self.preview_light(item)
                    } else {
                        1.0
                    },
                    if self.anim_on {
                        self.anim.key(sprite.anim_mask, anim_ms)
                    } else {
                        0
                    },
                );
            }
            let wr = Rect::from_min_size(
                Pos2::new(ix as f32, iy as f32),
                Vec2::new(sprite.width as f32, sprite.height as f32),
            );
            if !wr.intersects(world_view) {
                continue;
            }
            if matches!(item.blend, fo_scene::Blend::Palette(_))
                && let Some(effects) = &effects
            {
                let anim = self
                    .game
                    .as_ref()
                    .filter(|_| self.anim_on)
                    .map(|g| (&self.anim, g.art.palette().as_ref(), anim_ms));
                let tex = effects.patch(&mut self.effects, &ctx, anim);
                let sr = Rect::from_min_max(
                    cam.world_to_screen(view, wr.min),
                    cam.world_to_screen(view, wr.max),
                );
                let alpha = preview.as_ref().map_or(255, |p| p.wall_alpha(item));
                painter.image(
                    tex,
                    sr,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::from_white_alpha(alpha),
                );
                if self.anim_on && sprite.anim_mask != 0 {
                    any_animated = true;
                }
                continue;
            }
            let id = sprite.id;
            if !self.textures.contains_key(&id) {
                if created >= TEXTURE_BUDGET {
                    pending = true;
                    continue;
                }
                created += 1;
                let img = egui::ColorImage::from_rgba_unmultiplied(
                    [sprite.width as usize, sprite.height as usize],
                    &sprite.rgba,
                );
                self.textures.insert(
                    id,
                    ctx.load_texture(format!("sprite{id}"), img, TextureOptions::NEAREST),
                );
            }
            if self.anim_on && sprite.anim_mask != 0 {
                any_animated = true;
                let t = self.anim_start.elapsed().as_millis() as u64;
                let key = self.anim.key(sprite.anim_mask, t);
                if self.anim_keys.get(&id) != Some(&key)
                    && let (Some(g), Some(tex)) = (&self.game, self.textures.get_mut(&id))
                {
                    let rgba = self.anim.render(&sprite.indices, g.art.palette(), t);
                    let img = egui::ColorImage::from_rgba_unmultiplied(
                        [sprite.width as usize, sprite.height as usize],
                        &rgba,
                    );
                    tex.set(img, TextureOptions::NEAREST);
                    self.anim_keys.insert(id, key);
                }
            }
            let sr = Rect::from_min_max(
                cam.world_to_screen(view, wr.min),
                cam.world_to_screen(view, wr.max),
            );
            let tint = if previewing {
                crate::preview::lit_tint(item, self.preview_light(item))
            } else {
                match item.blend {
                    fo_scene::Blend::Normal => Color32::from_white_alpha(item.alpha),
                    fo_scene::Blend::Palette(_) => Color32::WHITE,
                }
            };
            let tint = preview.as_ref().map_or(tint, |p| {
                tint.gamma_multiply(p.wall_alpha(item) as f32 / 255.0)
            });
            painter.image(
                self.textures[&id].id(),
                sr,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                tint,
            );
        }
        if pending {
            ctx.request_repaint();
        } else if any_animated {
            ctx.request_repaint_after(Duration::from_millis(33));
        }

        if let Some(p) = &preview {
            // Aperçu : ni halos, ni grille, ni repères ; le cadre de l'écran du jeu si demandé.
            self.draw_game_frame(&painter, view, p);
        }
        if self.walk.on && !previewing {
            self.draw_walk(&painter, view, world_view);
        }
        if self.show_lights && !previewing {
            self.draw_lights(&painter, view, world_view);
        }
        if !previewing {
            self.draw_diff(&painter, view);
        }
        if self.show_grid && cam.zoom >= 0.6 && !previewing {
            self.draw_grid(&painter, view, world_view);
        }
        if self.show_markers && !previewing {
            for m in &scene.markers {
                let (cx, cy) = geom::hex_to_screen(m.hex);
                let p = cam.world_to_screen(view, Pos2::new(cx as f32, cy as f32));
                if !view.expand(20.0).contains(p) {
                    continue;
                }
                let (color, r) = match m.kind {
                    MarkerKind::PlayerStart => (Color32::from_rgb(80, 255, 120), 9.0),
                    MarkerKind::ExitGrid => (Color32::from_rgb(255, 210, 60), 6.0),
                    MarkerKind::SpatialScript => (Color32::from_rgb(90, 170, 255), 6.0),
                    MarkerKind::Hidden => (Color32::from_rgba_unmultiplied(200, 200, 200, 90), 3.0),
                };
                painter.circle_stroke(p, r * cam.zoom.clamp(0.6, 1.5), Stroke::new(1.5, color));
            }
        }
        // Éléments sélectionnés : cadre rouge autour des objets, losange autour des cases.
        if !self.selection.is_empty() {
            let hex_fill = Color32::from_rgba_unmultiplied(80, 160, 255, 70);
            for &h in self.selection.hexes() {
                let (cx, cy) = geom::hex_to_screen(h);
                let c = Pos2::new(cx as f32, cy as f32);
                let pts: Vec<Pos2> = crate::edit_ui::HEX_OUTLINE
                    .iter()
                    .map(|(dx, dy)| cam.world_to_screen(view, c + Vec2::new(*dx, *dy)))
                    .collect();
                painter.add(egui::Shape::convex_polygon(
                    pts,
                    hex_fill,
                    Stroke::new(1.0, Color32::from_rgb(80, 160, 255)),
                ));
            }
            let selected: std::collections::HashSet<ObjectRef> =
                self.selection.objects().iter().copied().collect();
            let stroke = Stroke::new(1.5, Color32::from_rgb(255, 80, 80));
            for item in scene
                .items
                .iter()
                .filter(|i| i.object.is_some_and(|o| selected.contains(&o)))
            {
                let r = Rect::from_min_size(
                    Pos2::new(item.x as f32, item.y as f32),
                    Vec2::new(item.sprite.width as f32, item.sprite.height as f32),
                );
                painter.rect_stroke(
                    Rect::from_min_max(
                        cam.world_to_screen(view, r.min),
                        cam.world_to_screen(view, r.max),
                    ),
                    0.0,
                    stroke,
                    egui::StrokeKind::Outside,
                );
            }
            for t in self.selection.tiles() {
                let (x, y) = geom::square_to_screen(t.square);
                let dy = if t.layer == fo_core::map::TileLayer::Roof {
                    geom::ROOF_OFFSET_Y as f32
                } else {
                    0.0
                };
                let (x, y) = (x as f32, y as f32 + dy);
                let pts = [
                    Pos2::new(x + 40.0, y),
                    Pos2::new(x + 80.0, y + 18.0),
                    Pos2::new(x + 40.0, y + 36.0),
                    Pos2::new(x, y + 18.0),
                ];
                painter.add(egui::Shape::closed_line(
                    pts.iter().map(|&p| cam.world_to_screen(view, p)).collect(),
                    stroke,
                ));
            }
        }

        if self.show_scripted && !previewing {
            self.draw_scripted(&painter, view, &scene);
        }
        if editing {
            self.draw_edit_overlay(&ctx, &painter, view, &scene);
            self.canvas_context_menu(&response);
        } else if response.clicked()
            && let Some(w) = world_pos
        {
            self.selection.set_object(
                scene
                    .pick(self.layers, w.x as i32, w.y as i32)
                    .and_then(|i| i.object),
            );
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

    fn draw_grid(&self, painter: &egui::Painter, view: Rect, world_view: Rect) {
        let stroke = Stroke::new(1.0, self.prefs.grid_stroke_color());
        for tile in 0..geom::HEX_COUNT {
            let (cx, cy) = geom::hex_to_screen(tile);
            let c = Pos2::new(cx as f32, cy as f32);
            if !world_view.expand(32.0).contains(c) {
                continue;
            }
            let pts: Vec<Pos2> = crate::edit_ui::HEX_OUTLINE
                .iter()
                .map(|(dx, dy)| self.camera.world_to_screen(view, c + Vec2::new(*dx, *dy)))
                .collect();
            painter.add(egui::Shape::closed_line(pts, stroke));
        }
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
