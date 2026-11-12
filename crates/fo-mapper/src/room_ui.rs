//! Outil « Pièce » : construit deux murs du fond en L, d'après les enchaînements de pièces observés dans toutes les cartes,
//! et un sol d'une famille de tuiles choisie. Les statistiques viennent de l'analyse de toutes les cartes.

use crate::app::MapperApp;
use crate::i18n::t;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke};
use fo_core::Fid;
use fo_core::build::{Counts, fill_tiles, floor_squares, interior_squares, place_plan};
use fo_core::map::{TileLayer, word};
use fo_core::pattern::{CapturedObject, CapturedTile, Pattern, PatternVariant};
use fo_core::room::{
    DecorOptions, RoomPlan, StyleModel, WallSet, plan_building, plan_room, plan_stairs,
    stair_destination,
};
use fo_core::tile_stats::{families, family_of};
use fo_core::{MapObject, geom};
use std::sync::Arc;

/// Ce qu'une construction a posé, pour l'enregistrer comme motif de la bibliothèque.
#[derive(Clone)]
struct LastBuild {
    anchor: u32,
    objects: Vec<CapturedObject>,
    floor: Vec<CapturedTile>,
    roof: Vec<CapturedTile>,
}

pub(crate) struct RoomState {
    pub style: String,
    /// Longueur du mur de droite (le long des lignes) et du mur de gauche (le long des colonnes), en hexagones.
    pub rows: i32,
    pub cols: i32,
    pub floor: bool,
    pub family: String,
    pub auto_floor: bool,
    pub auto_roof: bool,
    pub edit_shell: Option<fo_core::construction::Shell>,
    pub door_target: Option<u32>,
    pub picking_door: bool,
    pub palette_thumbs: std::collections::HashMap<String, egui::TextureHandle>,
    pub palette_open: bool,
    /// Poser des sources lumineuses.
    pub lights: bool,
    /// Tailles de pièce variées dans un bâtiment.
    pub vary: bool,
    /// Ajouter un étage au-dessus (avec des escaliers).
    pub second_floor: bool,
    /// Graine des constructions : l'aperçu et la pose utilisent la même tant qu'on n'en demande pas une autre
    /// (bouton du panneau, boutons précédent / suivant de la souris).
    pub seed: u64,
    /// Graines déjà essayées et position de la graine courante, pour revenir en arrière.
    seeds: Vec<u64>,
    seed_at: usize,
    /// Nombre de pièces du bâtiment en largeur et en profondeur (1 × 1 : une seule pièce).
    pub nx: i32,
    pub ny: i32,
    /// Nom proposé pour enregistrer la dernière construction comme motif.
    pub pattern_name: String,
    /// Dernière construction : de quoi l'enregistrer comme motif.
    last: Option<LastBuild>,
    /// Construire aussi les murs de devant.
    pub front: bool,
    /// Poser un toit.
    pub roof: bool,
    pub roof_family: String,
    /// Nombre de fenêtres à poser dans les murs.
    pub windows: i32,
    /// Poser une porte dans un mur.
    pub door: bool,
    /// Nombre d'objets de mobilier à poser contre les murs.
    pub furniture: i32,
    /// Type de pièce (indice dans `StyleModel::kinds`) ; `None` : ensemble cohérent choisi selon la graine.
    pub kind: Option<usize>,
    styles: Vec<WallSet>,
    /// Nombre de cartes de l'analyse pour laquelle `styles` a été calculé (0 : pas encore).
    styles_for: usize,
    model: Option<(String, Arc<StyleModel>)>,
}

impl RoomState {
    /// Graine précédente de l'historique (sans effet sur la première).
    pub(crate) fn previous_seed(&mut self) {
        if self.seed_at > 0 {
            self.seed_at -= 1;
            self.seed = self.seeds[self.seed_at];
        }
    }

    /// Graine suivante de l'historique, ou une nouvelle si on est au bout.
    pub(crate) fn next_seed(&mut self) {
        if self.seed_at + 1 < self.seeds.len() {
            self.seed_at += 1;
        } else {
            let fresh = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(7, |d| d.as_nanos() as u64)
                | 1;
            self.seeds.push(fresh);
            self.seed_at = self.seeds.len() - 1;
        }
        self.seed = self.seeds[self.seed_at];
    }

    /// Styles de murs utilisables (après l'analyse de toutes les cartes).
    pub(crate) fn styles(&self) -> &[WallSet] {
        &self.styles
    }
}

impl Default for RoomState {
    fn default() -> Self {
        Self {
            style: String::new(),
            rows: 12,
            cols: 12,
            vary: false,
            second_floor: false,
            seed: 1,
            seeds: vec![1],
            seed_at: 0,
            nx: 1,
            ny: 1,
            pattern_name: String::new(),
            last: None,
            floor: true,
            family: String::new(),
            auto_floor: true,
            auto_roof: true,
            edit_shell: None,
            door_target: None,
            picking_door: false,
            palette_thumbs: Default::default(),
            palette_open: false,
            door: true,
            furniture: 6,
            kind: None,
            lights: true,
            front: false,
            roof: false,
            roof_family: String::new(),
            windows: 2,
            styles: Vec::new(),
            styles_for: 0,
            model: None,
        }
    }
}

/// Largeur visible d'une pièce de mur : pixels opaques divisés par le nombre de lignes qui en contiennent. Un dessin de 32 px de large
/// mais surtout transparent compte pour peu, ce que la largeur du fichier ne dit pas.
pub(crate) fn visible_width(art: &fo_core::ArtLibrary, fid: u32) -> Option<u32> {
    let (_, sprite) = art.object_sprite(Fid(fid), 0, 0)?;
    let opaque = sprite.rgba.chunks_exact(4).filter(|p| p[3] > 0).count() as u32;
    let rows = (0..sprite.height as usize)
        .filter(|&y| {
            sprite.rgba[y * sprite.width as usize * 4..(y + 1) * sprite.width as usize * 4]
                .chunks_exact(4)
                .any(|p| p[3] > 0)
        })
        .count() as u32;
    (rows > 0).then(|| opaque / rows)
}

/// Hexagone de colonne et de ligne paires le plus proche en arrière : les pièces s'alignent ainsi sur les cases de sol (2 × 2 hexagones),
/// et la disposition (murs, mobilier) ne change pas quand on déplace la souris.
pub(crate) fn snap_corner(hex: u32) -> u32 {
    let (c, r) = geom::hex_col_row(hex);
    (r & !1) * geom::HEX_GRID_WIDTH + (c & !1)
}

impl MapperApp {
    fn wall_names(&self) -> Option<Arc<Vec<String>>> {
        self.game
            .as_ref()
            .map(|g| g.data.list("art/walls/walls.lst"))
    }

    /// Met à jour la liste des styles et le modèle du style choisi.
    pub(crate) fn refresh_room_models(&mut self) {
        let Some(res) = self.analysis.result.as_ref() else {
            return;
        };
        let Some(names) = self.wall_names() else {
            return;
        };
        let analysis_id = Arc::as_ptr(&res.walls) as usize;
        if self.edit.room.styles_for != analysis_id {
            // Seuls les ensembles qui permettent une pièce complète de taille moyenne (enchaînements confirmés par les
            // cartes) sont proposés ; les pièces invisibles de blocage (« block ») sont écartées.
            let walls = res.walls.clone();
            let art = self.game.as_ref().map(|g| g.art.clone());
            self.edit.room.styles = walls
                .sets(&names)
                .into_iter()
                .filter(|s| !s.label.starts_with("block"))
                .filter(|s| {
                    let members = s.members.clone();
                    let mut model = walls.model(&|f| members.contains(&f)).strong_only();
                    if let Some(art) = &art {
                        model.set_widths(&|f| visible_width(art, f));
                    }
                    // Les quatre parités possibles du coin (colonne et ligne paires ou impaires).
                    [(100, 50), (100, 51), (101, 50), (101, 51)]
                        .iter()
                        .all(|&(row, col)| {
                            let plan = plan_room(&model, row * 200 + col, 8, 8);
                            !plan.short && !plan.walls.is_empty()
                        })
                })
                .collect();
            self.edit.room.styles_for = analysis_id;
            self.edit.room.model = None;
            self.edit.room.palette_thumbs.clear();
            if !self
                .edit
                .room
                .styles
                .iter()
                .any(|s| s.label == self.edit.room.style)
            {
                // Une ancienne sélection VB/VCN appartient désormais au style
                // complet d'Abri ; conserver ce matériau après le regroupement.
                let fid = names
                    .iter()
                    .position(|name| {
                        name.trim()
                            .trim_end_matches(".frm")
                            .trim_end_matches(".FRM")
                            .eq_ignore_ascii_case(&self.edit.room.style)
                    })
                    .map(|id| 0x0300_0000 | id as u32);
                self.edit.room.style = self
                    .edit
                    .room
                    .styles
                    .iter()
                    .find(|s| fid.is_some_and(|f| s.members.contains(&f)))
                    .or_else(|| self.edit.room.styles.first())
                    .map(|s| s.label.clone())
                    .unwrap_or_default();
            }
        }
        let wanted = self.edit.room.style.clone();
        if self
            .edit
            .room
            .model
            .as_ref()
            .is_none_or(|(s, _)| *s != wanted)
            && !wanted.is_empty()
        {
            if let Some(set) = self.edit.room.styles.iter().find(|s| s.label == wanted) {
                let members = set.members.clone();
                let mut model = res.walls.model(&|fid| members.contains(&fid)).strong_only();
                if let Some(g) = &self.game {
                    model.set_widths(&|fid| visible_width(&g.art, fid));
                }
                self.edit.room.model = Some((wanted, Arc::new(model)));
            }
        }
        if self.edit.room.auto_floor {
            self.room_floor_ids();
        }
        if self.edit.room.auto_roof {
            self.room_roof_ids();
        }
    }

    /// Capture d'écran : pose une pièce près du point de départ du joueur et recentre la vue dessus.
    pub(crate) fn shot_place_room(&mut self, style: &str) {
        self.refresh_room_models();
        // `#n` : le n-ième style de la liste ; sinon le nom du style ; vide : le style par défaut.
        if let Some(i) = style
            .strip_prefix('#')
            .and_then(|n| n.parse::<usize>().ok())
        {
            if let Some(s) = self.edit.room.styles.get(i) {
                self.edit.room.style = s.label.clone();
            }
        } else if !style.is_empty() {
            self.edit.room.style = style.to_string();
        }
        self.refresh_room_models();
        self.edit.room.roof = std::env::var_os("FO_SHOT_NOROOF").is_none();
        self.edit.room.front = true;
        // Essai d'enregistrement du motif (FO_SHOT_KEEP=<nom>, avec FO_SELFTEST_WRITE_OK=1 sur un dossier jetable).
        let keep = std::env::var("FO_SHOT_KEEP")
            .ok()
            .filter(|_| std::env::var_os("FO_SELFTEST_WRITE_OK").is_some());
        self.edit.room.vary = std::env::var_os("FO_SHOT_VARY").is_some();
        self.edit.room.second_floor = std::env::var_os("FO_SHOT_FLOORS").is_some();
        if let Some((a, b)) = std::env::var("FO_SHOT_BUILD").ok().and_then(|v| {
            v.split_once('x')
                .map(|(a, b)| (a.to_string(), b.to_string()))
        }) {
            self.edit.room.nx = a.parse().unwrap_or(1);
            self.edit.room.ny = b.parse().unwrap_or(1);
            self.edit.room.cols = 7;
            self.edit.room.rows = 6;
        }
        // FO_SHOT_ROOM_SIZE=<colonnes>x<lignes> : taille de la pièce (mur de gauche x mur de droite).
        if let Some((a, b)) = std::env::var("FO_SHOT_ROOM_SIZE").ok().and_then(|v| v.split_once('x').map(|(a, b)| (a.to_string(), b.to_string()))) {
            self.edit.room.cols = a.parse().unwrap_or(8);
            self.edit.room.rows = b.parse().unwrap_or(8);
        }
        if let Some(k) = std::env::var("FO_SHOT_KIND")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
        {
            self.edit.room.kind = Some(k);
        }
        let Some(cur) = &self.current else { return };
        let start = cur.map.header.player_tile.max(0) as u32;
        let (c, r) = geom::hex_col_row(start);
        let corner = (r.saturating_sub(10)) * geom::HEX_GRID_WIDTH + c.saturating_sub(5);
        let corner = snap_corner(corner);
        self.place_room(corner);
        eprintln!("SHOT {}", self.status);
        if std::env::var_os("FO_SHOT_UPPER").is_some() {
            if let Some(cur) = self.current.as_ref() {
                eprintln!(
                    "SHOT objets étage 1: {}, escaliers: {:?}",
                    cur.map.objects[1].len(),
                    cur.map
                        .objects
                        .iter()
                        .enumerate()
                        .flat_map(|(e, l)| l
                            .iter()
                            .filter(|o| o.subtype == fo_core::formats::scenery_kind::STAIRS
                                && o.tile() >= 0)
                            .map(move |o| (e, o.tile(), o.extra.clone())))
                        .collect::<Vec<_>>()
                );
            }
            self.elevation = 1;
            self.rebuild_scene(1);
        }
        if let Some(name) = keep {
            self.edit.room.pattern_name = name;
            self.keep_last_build_as_pattern();
            eprintln!("KEEP {}", self.status);
        }
        let span = (self.edit.room.ny.max(1) as u32) * (self.edit.room.rows as u32 + 1) / 2;
        let (x, y) = geom::hex_to_screen(
            corner
                + (6 + span) * geom::HEX_GRID_WIDTH
                + self.edit.room.nx.max(1) as u32 * self.edit.room.cols as u32 / 2,
        );
        self.camera.center = Pos2::new(x as f32, y as f32);
        self.camera.zoom = 1.0;
        self.needs_fit = false;
    }

    /// Nom d'un type de pièce : les trois meubles les plus courants (noms du jeu, sans doublon).
    fn kind_label(&self, model: &StyleModel, kind: &fo_core::room::RoomKind) -> String {
        let mut members: Vec<(u32, u32)> = kind
            .members
            .iter()
            .map(|&f| (f, model.furniture_weight(f)))
            .collect();
        members.sort_by_key(|&(f, w)| (std::cmp::Reverse(w), f));
        let mut names: Vec<String> = Vec::new();
        for (f, _) in members {
            let name = model
                .pid_of(f)
                .and_then(|pid| self.proto_name(pid))
                .unwrap_or_default();
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
            if names.len() == 3 {
                break;
            }
        }
        if names.is_empty() {
            format!("{:08X}", kind.seed)
        } else {
            names.join(", ")
        }
    }

    /// Vérification des nouveaux outils (voir `FO_SHOT_VERIFY`) : les résultats vont sur la sortie d'erreur.
    pub(crate) fn verify_new_tools(&mut self) {
        let count = |a: &MapperApp| a.current.as_ref().map_or(0, |c| c.map.objects[0].len());
        self.edit.enabled = true;
        self.refresh_room_models();
        let base = count(self);
        // 0. Correction du vérificateur : on injecte des défauts, on corrige, puis on annule.
        self.mutate_as("Essai", |m, _| {
            if let Some(mut o) = m.objects[0].iter().find(|o| o.tile() >= 0).cloned() {
                o.words[fo_core::map::word::TILE] = -5;
                o.words[fo_core::map::word::ID] = m.max_object_id() + 1;
                m.add_object(0, o);
            }
            m.header.player_tile = 99_999;
            true
        });
        self.check.open = true;
        self.run_check();
        let found = self.check.issues.len();
        let n0 = count(self);
        self.fix_check_issues(None);
        eprintln!(
            "VERIFY correction: {found} problème(s) -> {} après ; objets {n0} -> {} ; {}",
            self.check.issues.len(),
            count(self),
            self.status
        );
        self.undo();
        self.run_check();
        eprintln!(
            "VERIFY correction annulée: objets {} ; problèmes revenus {}",
            count(self),
            self.check.issues.len()
        );
        self.check.open = false;
        // 0 bis. Un bâtiment à étage et tailles variées, puis Ctrl+Z : l'élévation créée disparaît aussi.
        {
            self.edit.room.nx = 3;
            self.edit.room.ny = 2;
            self.edit.room.vary = true;
            self.edit.room.second_floor = true;
            let had = self
                .current
                .as_ref()
                .is_some_and(|c| c.map.header.has_elevation(1));
            self.place_room(90 * 200 + 95);
            let (has, upper_objects) = self.current.as_ref().map_or((false, 0), |c| {
                (c.map.header.has_elevation(1), c.map.objects[1].len())
            });
            self.undo();
            let after = self.current.as_ref().map_or((false, 0), |c| {
                (c.map.header.has_elevation(1), c.map.objects[1].len())
            });
            eprintln!(
                "VERIFY étage: avant {had}, après pose {has} ({upper_objects} objets à l'étage), après annulation {after:?}"
            );
            self.edit.room.vary = false;
            self.edit.room.second_floor = false;
        }
        // 1. Un bâtiment 3x2 puis Ctrl+Z.
        self.edit.room.nx = 3;
        self.edit.room.ny = 2;
        self.edit.room.roof = true;
        let tiles_before = self.current.as_ref().map(|c| c.map.tiles[0].clone());
        self.place_room(90 * 200 + 95);
        let built = count(self);
        self.undo();
        let tiles_back = self.current.as_ref().map(|c| c.map.tiles[0].clone()) == tiles_before;
        eprintln!(
            "VERIFY bâtiment: {base} -> {built} objets, après annulation {} (sol et toit restitués : {tiles_back})",
            count(self)
        );
        // 2. Enregistrement (crée la copie .bak), modification, comparaison.
        let saved = self.save_current();
        self.edit.room.nx = 1;
        self.edit.room.ny = 1;
        self.place_room(90 * 200 + 95);
        let saved_again = self.save_current();
        self.diff.open = true;
        self.run_diff();
        eprintln!(
            "VERIFY enregistrements: {saved} {saved_again} ; diff vs enregistrée: {}",
            self.status
        );
        self.diff.against = crate::diff_ui::Against::Backup;
        self.run_diff();
        eprintln!(
            "VERIFY diff vs .bak: {} ({})",
            self.status,
            self.diff.diff_summary()
        );
        self.diff.open = false;
        // 3. Remplacement en masse : tous les objets du prototype le plus répandu → un autre prototype, puis Ctrl+Z.
        let target = self.current.as_ref().and_then(|c| {
            let mut n: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
            for o in &c.map.objects[0] {
                if o.object_type() == Some(fo_core::ObjectType::Scenery) && o.tile() >= 0 {
                    *n.entry(o.pid()).or_default() += 1;
                }
            }
            n.into_iter().max_by_key(|(p, k)| (*k, *p))
        });
        if let (Some((pid, k)), Some(g)) = (target, self.game.as_ref()) {
            let other = g
                .data
                .proto_catalog(fo_core::ObjectType::Scenery)
                .into_iter()
                .find(|e| e.pid != pid && !e.name.is_empty());
            self.edit.place = other.clone();
            self.find.chosen = Some((pid, String::new()));
            self.find.all_elevations = false;
            self.run_find();
            let hits = self.find.hits.len();
            let before = count(self);
            self.replace_hits();
            let left = self
                .current
                .as_ref()
                .map_or(0, |c| c.map.find_objects(pid, Some(0)).len());
            let now = other
                .as_ref()
                .and_then(|o| {
                    self.current
                        .as_ref()
                        .map(|c| c.map.find_objects(o.pid, Some(0)).len())
                })
                .unwrap_or(0);
            self.undo();
            let back = self
                .current
                .as_ref()
                .map_or(0, |c| c.map.find_objects(pid, Some(0)).len());
            eprintln!(
                "VERIFY remplacement: {k} attendus, {hits} trouvés, objets {before} -> {} ; restant de l'ancien {left}, du nouveau {now} ; après annulation ancien {back}",
                count(self)
            );
        }
    }

    pub(crate) fn room_model(&self) -> Option<Arc<StyleModel>> {
        self.edit
            .room
            .model
            .as_ref()
            .map(|(_, m)| m.clone())
            .filter(|m| m.is_usable())
    }

    pub(crate) fn room_panel(&mut self, ui: &mut egui::Ui) {
        if self.edit.room.edit_shell.is_some() {
            ui.label(t(
                "Modification du bâtiment : le mobilier et ses propriétés seront conservés.",
            ));
            ui.weak(t(
                "Le contour et les cloisons sont reconstruits selon les réglages ci-dessous.",
            ));
            if ui.button(t("Appliquer au bâtiment")).clicked() {
                self.rebuild_selected_building();
            }
            if ui
                .button(t("Choisir la position de la porte sur la carte"))
                .clicked()
            {
                self.edit.room.picking_door = true;
            }
            if self.edit.room.picking_door {
                ui.label(t("Cliquez sur le nouvel emplacement de la porte."));
            }
            if ui
                .button(t("Terminer la modification du bâtiment"))
                .clicked()
            {
                self.edit.room.edit_shell = None;
                self.edit.room.door_target = None;
            }
            ui.separator();
        }
        ui.label(t("Cliquez sur la carte : le coin du haut de la pièce se pose sur l'hexagone, les murs du fond partent vers la gauche et vers la droite."));
        if self.analysis.running() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(&self.status);
            });
            return;
        }
        if self.analysis.result.is_none() {
            ui.label(t(
                "Les styles de murs viennent de l'analyse de toutes les cartes.",
            ));
            if ui.button(t("Analyser toutes les cartes")).clicked() {
                let ctx = ui.ctx().clone();
                self.start_tile_analysis(&ctx);
            }
            return;
        }
        self.refresh_room_models();
        self.construction_style_ui(ui);
        if self.room_model().is_none() {
            ui.colored_label(Color32::LIGHT_RED, t("Aucun style de mur utilisable."));
            return;
        }
        ui.horizontal(|ui| {
            ui.label(t("Mur de gauche"));
            ui.add(crate::stepper::Stepper::new(&mut self.edit.room.cols).range(2..=60));
        });
        ui.horizontal(|ui| {
            ui.label(t("Mur de droite"));
            ui.add(crate::stepper::Stepper::new(&mut self.edit.room.rows).range(2..=60));
        });
        let hint = t(
            "Plusieurs pièces qui se partagent leurs cloisons, reliées par des portes ; l'entrée est sur la façade.",
        );
        ui.horizontal(|ui| {
            ui.label(t("Pièces en largeur"));
            ui.add(crate::stepper::Stepper::new(&mut self.edit.room.nx).range(1..=8));
        })
        .response
        .on_hover_text(hint);
        ui.horizontal(|ui| {
            ui.label(t("Pièces en profondeur"));
            ui.add(crate::stepper::Stepper::new(&mut self.edit.room.ny).range(1..=8));
        })
        .response
        .on_hover_text(hint);
        ui.add_enabled(
            self.edit.room.nx > 1 || self.edit.room.ny > 1,
            egui::Checkbox::new(&mut self.edit.room.vary, t("Tailles de pièce variées")),
        );
        let can_stack =
            self.elevation < 2 && self.room_model().is_some_and(|m| m.stairs().is_some());
        ui.add_enabled(can_stack, egui::Checkbox::new(&mut self.edit.room.second_floor, t("Ajouter un étage (escaliers)")))
            .on_hover_text(t("Construit le même bâtiment à l'élévation suivante, relié par des escaliers appris des cartes."));
        ui.horizontal(|ui| {
            if ui.button(t("Autre variante")).on_hover_text(t("Tire une nouvelle disposition (bouton « suivant » de la souris) ; « précédent » revient à la précédente.")).clicked() {
                self.edit.room.next_seed();
            }
            if ui.add_enabled(self.edit.room.seed_at > 0, egui::Button::new(t("Précédente"))).clicked() {
                self.edit.room.previous_seed();
            }
        });
        ui.checkbox(&mut self.edit.room.front, t("Murs de devant"));
        let has_light = self.room_model().is_some_and(|m| m.light().is_some());
        ui.add_enabled(
            has_light,
            egui::Checkbox::new(
                &mut self.edit.room.lights,
                t("Éclairage (sources lumineuses)"),
            ),
        );
        let (doors, furniture) = self.room_model().map_or((0, 0), |m| m.decor_counts());
        let windows = self.room_model().map_or(0, |m| m.window_count());
        ui.horizontal(|ui| {
            ui.label(t("Fenêtres"));
            ui.add_enabled(
                windows > 0,
                crate::stepper::Stepper::new(&mut self.edit.room.windows).range(0..=20),
            );
        });
        if windows == 0 {
            ui.weak(t("Aucune fenêtre observée dans ce style."));
        }
        ui.add_enabled(
            doors > 0,
            egui::Checkbox::new(&mut self.edit.room.door, t("Poser une porte")),
        );
        if let Some(model) = self.room_model()
            && !model.kinds().is_empty()
        {
            let names: Vec<String> = model
                .kinds()
                .iter()
                .map(|k| self.kind_label(&model, k))
                .collect();
            let current = self.edit.room.kind.filter(|k| *k < names.len());
            self.edit.room.kind = current;
            ui.horizontal(|ui| {
                ui.label(t("Type de pièce"));
                egui::ComboBox::from_id_salt("room_kind")
                    .width((ui.available_width() - 4.0).max(120.0))
                    .selected_text(
                        current.map_or_else(
                            || t("Mobilier assorti").to_string(),
                            |k| names[k].clone(),
                        ),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.edit.room.kind, None, t("Mobilier assorti"));
                        for (i, n) in names.iter().enumerate() {
                            ui.selectable_value(&mut self.edit.room.kind, Some(i), n);
                        }
                    });
            });
        }
        ui.horizontal(|ui| {
            ui.label(t("Mobilier"));
            ui.add_enabled(
                furniture > 0,
                crate::stepper::Stepper::new(&mut self.edit.room.furniture).range(0..=40),
            );
        });
        if furniture == 0 {
            ui.weak(t("Aucun mobilier observé près des murs de ce style."));
        }
        ui.checkbox(&mut self.edit.room.floor, t("Poser un sol"));
        if self.edit.room.floor {
            ui.checkbox(&mut self.edit.room.auto_floor, t("Sol assorti aux murs"));
            self.room_floor_ids();
            if self.edit.room.auto_floor && self.edit.room.family.is_empty() {
                ui.weak(t(
                    "Aucun sol associé à ces murs : choisissez une famille de sol.",
                ));
            }
            let names = self.floor_names();
            let fams = families(&names);
            if !self.edit.room.auto_floor && !fams.contains_key(&self.edit.room.family) {
                let from_brush = names
                    .get(self.edit.brush_tile as usize)
                    .map(|n| family_of(n))
                    .filter(|f| fams.contains_key(f));
                self.edit.room.family = from_brush
                    .or_else(|| fams.keys().next().cloned())
                    .unwrap_or_default();
            }
            ui.horizontal(|ui| {
                ui.label(t("Famille de sol"));
                egui::ComboBox::from_id_salt("room_family")
                    .selected_text(self.edit.room.family.clone())
                    .show_ui(ui, |ui| {
                        for f in fams.keys() {
                            if ui
                                .selectable_value(&mut self.edit.room.family, f.clone(), f)
                                .clicked()
                            {
                                self.edit.room.auto_floor = false;
                            }
                        }
                    });
                if ui.small_button(t("Celle du pinceau")).clicked()
                    && let Some(f) = names
                        .get(self.edit.brush_tile as usize)
                        .map(|n| family_of(n))
                    && fams.contains_key(&f)
                {
                    self.edit.room.family = f;
                    self.edit.room.auto_floor = false;
                }
            });
        }
        ui.checkbox(&mut self.edit.room.roof, t("Poser un toit"));
        if self.edit.room.roof {
            self.roof_family_ui(ui);
        }
        if self.edit.room.last.is_some() {
            ui.separator();
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.edit.room.pattern_name)
                        .hint_text(t("nom du motif"))
                        .desired_width(f32::INFINITY),
                );
            });
            if ui
                .button(t("Garder la dernière construction comme motif"))
                .clicked()
            {
                self.keep_last_build_as_pattern();
            }
        }
    }

    /// Range la dernière construction dans la bibliothèque de motifs (outil « Tampon »), au même format que celui de Gecko.
    fn keep_last_build_as_pattern(&mut self) {
        let Some(last) = self.edit.room.last.clone() else {
            return;
        };
        let variant = PatternVariant::capture(
            "défaut",
            last.anchor,
            &last.objects,
            &last.floor,
            &last.roof,
        );
        let name = match self.edit.room.pattern_name.trim() {
            "" => format!("{} {}", t("bâtiment"), self.edit.stamp.patterns.len() + 1),
            n => n.to_string(),
        };
        let mut pattern = Pattern::new(name.clone(), variant);
        pattern.tags = vec![t("bâtiment").to_string()];
        match self.add_pattern(pattern) {
            Ok(path) => self.status = format!("{} : {}", t("Motif enregistré"), path),
            Err(e) => self.status = format!("{} : {e}", t("Échec de l'enregistrement du motif")),
        }
    }

    /// Choix de la famille de tuiles d'un toit (liste déroulante, défaut : la famille du toit le plus courant).
    fn roof_family_ui(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.edit.room.auto_roof, t("Toit assorti aux murs"));
        self.room_roof_ids();
        let names = self.floor_names();
        let fams = families(&names);
        if !self.edit.room.auto_roof && !fams.contains_key(&self.edit.room.roof_family) {
            let common = self
                .analysis
                .stats(TileLayer::Roof)
                .and_then(|s| s.pick(None, None, 0))
                .and_then(|id| names.get(id as usize))
                .map(|n| family_of(n))
                .filter(|f| fams.contains_key(f));
            self.edit.room.roof_family = common
                .or_else(|| fams.keys().next().cloned())
                .unwrap_or_default();
        }
        ui.horizontal(|ui| {
            ui.label(t("Famille de toit"));
            egui::ComboBox::from_id_salt("room_roof_family")
                .selected_text(self.edit.room.roof_family.clone())
                .show_ui(ui, |ui| {
                    for f in fams.keys() {
                        if ui
                            .selectable_value(&mut self.edit.room.roof_family, f.clone(), f)
                            .clicked()
                        {
                            self.edit.room.auto_roof = false;
                        }
                    }
                });
        });
    }

    pub(crate) fn floor_names(&mut self) -> Arc<Vec<String>> {
        if self.edit.tile_names.is_none()
            && let Some(g) = &self.game
        {
            self.edit.tile_names = Some(g.data.list("art/tiles/tiles.lst"));
        }
        self.edit.tile_names.clone().unwrap_or_default()
    }

    pub(crate) fn room_floor_ids(&mut self) -> Vec<u16> {
        let names = self.floor_names();
        if self.edit.room.auto_floor {
            if let Some((family, ids)) = self.room_model().and_then(|m| m.floor_choice(&names)) {
                self.edit.room.family = family;
                ids
            } else {
                self.edit.room.family.clear();
                Vec::new()
            }
        } else {
            families(&names)
                .get(&self.edit.room.family)
                .cloned()
                .unwrap_or_default()
        }
    }

    pub(crate) fn room_roof_ids(&mut self) -> Vec<u16> {
        let names = self.floor_names();
        if self.edit.room.auto_roof {
            if let Some((family, ids)) = self.room_model().and_then(|m| m.roof_choice(&names)) {
                self.edit.room.roof_family = family;
                ids
            } else {
                self.edit.room.roof_family.clear();
                Vec::new()
            }
        } else {
            families(&names)
                .get(&self.edit.room.roof_family)
                .cloned()
                .unwrap_or_default()
        }
    }

    pub(crate) fn room_input(&mut self, _ui: &egui::Ui, response: &egui::Response) {
        self.refresh_room_models();
        if self.edit.room.edit_shell.is_some() {
            if self.edit.room.picking_door && response.clicked_by(egui::PointerButton::Primary) {
                self.edit.room.door_target = self.hover_hex;
                self.edit.room.picking_door = false;
            }
            return;
        }
        if response.hovered() {
            let (back, forward) = response.ctx.input(|i| {
                (
                    i.pointer.button_pressed(egui::PointerButton::Extra1),
                    i.pointer.button_pressed(egui::PointerButton::Extra2),
                )
            });
            if back {
                self.edit.room.previous_seed();
            }
            if forward {
                self.edit.room.next_seed();
            }
        }
        if response.clicked_by(egui::PointerButton::Primary)
            && let Some(h) = self.hover_hex
        {
            self.place_room(snap_corner(h));
        }
    }

    fn decor_options(&self, seed: u64) -> DecorOptions {
        let r = &self.edit.room;
        DecorOptions {
            kind: r.kind,
            lights: r.lights,
            front: r.front,
            door: r.door,
            windows: r.windows.max(0) as u32,
            furniture: r.furniture.max(0) as u32,
            vary_sizes: r.vary,
            seed,
            ..DecorOptions::default()
        }
    }

    fn place_room(&mut self, corner: u32) {
        let Some(model) = self.room_model() else {
            self.status = t("Analysez d'abord toutes les cartes (panneau d'édition).").into();
            return;
        };
        let seed = self.edit.room.seed;
        let opts = self.decor_options(seed);
        let (nx, ny) = (
            self.edit.room.nx.max(1) as u32,
            self.edit.room.ny.max(1) as u32,
        );
        let (rows, cols) = (self.edit.room.rows as u32, self.edit.room.cols as u32);
        let plan = plan_building(&model, corner, nx, ny, rows, cols, &opts);
        if plan.walls.is_empty() {
            return;
        }
        let e = self.elevation;
        // Deuxième étage : même empreinte (mêmes tailles), autre mobilier, escaliers entre les deux.
        let upper: Option<(RoomPlan, fo_core::room::StairPlan)> = (self.edit.room.second_floor
            && e < 2)
            .then(|| {
                let upper = plan_building(
                    &model,
                    corner,
                    nx,
                    ny,
                    rows,
                    cols,
                    &DecorOptions {
                        seed: seed ^ 0x5DEE_CE66,
                        size_seed: Some(seed),
                        door: false,
                        ..opts.clone()
                    },
                );
                plan_stairs(&model, &plan, &upper, seed).map(|st| (upper, st))
            })
            .flatten();
        let two_floors = upper.is_some();
        let floor_ids: Vec<u16> = if self.edit.room.floor {
            self.room_floor_ids()
        } else {
            Vec::new()
        };
        let roof_ids = if self.edit.room.roof {
            self.room_roof_ids()
        } else {
            Vec::new()
        };
        let squares = floor_squares(&plan.hexes);
        let floor_sq = interior_squares(&plan.hexes);
        let floor_stats = self.analysis.stats(TileLayer::Floor).cloned();
        let roof_stats = self.analysis.stats(TileLayer::Roof).cloned();
        let mut counts = Counts::default();
        let objects_before = self.current.as_ref().map_or(0, |c| c.map.objects[e].len());
        let label = if two_floors {
            "Génération d'un bâtiment à étage"
        } else if nx * ny > 1 {
            "Génération d'un bâtiment"
        } else {
            "Génération d'une pièce"
        };
        self.mutate_as(label, |m, game| {
            place_plan(m, game, &model, e, &plan, &mut counts);
            // Sol (et toit sur le dernier étage seulement).
            fill_tiles(
                m,
                e,
                TileLayer::Floor,
                &floor_sq,
                &floor_ids,
                floor_stats.as_deref(),
                seed,
            );
            if upper.is_none() {
                fill_tiles(
                    m,
                    e,
                    TileLayer::Roof,
                    &squares,
                    &roof_ids,
                    roof_stats.as_deref(),
                    seed ^ 0x9E37_79B9,
                );
            }
            if let Some((upper_plan, st)) = &upper
                && m.ensure_elevation(e + 1)
            {
                let before = counts.placed;
                place_plan(m, game, &model, e + 1, upper_plan, &mut counts);
                counts.upper = counts.placed - before;
                fill_tiles(
                    m,
                    e + 1,
                    TileLayer::Floor,
                    &floor_sq,
                    &floor_ids,
                    floor_stats.as_deref(),
                    seed ^ 0x1234_5678,
                );
                fill_tiles(
                    m,
                    e + 1,
                    TileLayer::Roof,
                    &squares,
                    &roof_ids,
                    roof_stats.as_deref(),
                    seed ^ 0x9E37_79B9,
                );
                // Escaliers : celui du bas monte à l'étage, celui du haut redescend ; on arrive à côté de l'escalier d'en face.
                for (level, go_up) in [(e, true), (e + 1, false)] {
                    let (fid, pid) = if go_up {
                        (st.kinds.up_fid, st.kinds.up_pid)
                    } else {
                        (st.kinds.down_fid, st.kinds.down_pid)
                    };
                    let target = if go_up { e + 1 } else { e };
                    let arrive = if go_up { st.arrive_up } else { st.arrive_down };
                    let id = m.max_object_id() + 1;
                    if let Some(mut obj) = game
                        .proto_info(pid)
                        .and_then(|info| MapObject::from_proto(&info, st.hex, id))
                    {
                        obj.words[word::FID] = fid as i32;
                        if obj.extra.len() >= 2 {
                            obj.extra[0] = stair_destination(target as u32, arrive);
                            obj.extra[1] = -1;
                        }
                        m.add_object(level, obj);
                        counts.stairs += 1;
                    }
                }
            }
            counts.placed > 0
        });
        // De quoi enregistrer cette construction (étage du bas) comme motif : les objets posés et les cases couvertes.
        if let Some(cur) = self.current.as_ref() {
            let objects: Vec<CapturedObject> = cur.map.objects[e]
                .iter()
                .skip(objects_before)
                .filter(|o| o.tile() >= 0 && o.subtype != fo_core::formats::scenery_kind::STAIRS)
                .map(|o| CapturedObject {
                    hex: o.tile() as u32,
                    pro_pid: o.pid(),
                    frm_pid: o.fid(),
                    direction: o.orientation().max(0) as u32,
                    flags: o.flags(),
                })
                .collect();
            let tiles = |roof: bool, ids: &[u16]| -> Vec<CapturedTile> {
                if ids.is_empty() {
                    return Vec::new();
                }
                (if roof { &squares } else { &floor_sq })
                    .iter()
                    .filter_map(|&sq| {
                        cur.map.tile(e, sq).map(|t| CapturedTile {
                            square: sq as u32,
                            tile_id: if roof { t.roof } else { t.floor },
                        })
                    })
                    .collect()
            };
            self.edit.room.last = Some(LastBuild {
                anchor: corner,
                floor: tiles(false, &floor_ids),
                roof: tiles(true, &roof_ids),
                objects,
            });
        }
        let what = if nx * ny > 1 {
            t("Bâtiment généré :")
        } else {
            t("Pièce générée :")
        };
        self.status = format!(
            "{} {} {}, {} {}, {} {}, {} {}",
            what,
            counts.placed,
            t("pièce(s) de mur"),
            plan.windows,
            t("fenêtre(s)"),
            counts.doors,
            t("porte(s)"),
            counts.furniture,
            t("objet(s) de mobilier")
        );
        if counts.lights > 0 {
            self.status = format!(
                "{}, {} {}",
                self.status,
                counts.lights,
                t("source(s) lumineuse(s)")
            );
        }
        if counts.stairs > 0 {
            self.status = format!(
                "{} — {} {} ({} {})",
                self.status,
                counts.stairs,
                t("escalier(s)"),
                counts.upper,
                t("pièce(s) de mur à l'étage")
            );
        } else if self.edit.room.second_floor && !two_floors {
            self.status = format!(
                "{} — {}",
                self.status,
                t("pas de place pour des escaliers : étage non créé")
            );
        }
        if plan.short {
            self.status = format!(
                "{} — {}",
                self.status,
                t("un mur est plus court que demandé : pas assez de données pour ce style")
            );
        }
        if counts.missing > 0 {
            self.status = format!(
                "{} ({} {})",
                self.status,
                counts.missing,
                t("sans prototype")
            );
        }
    }

    /// Aperçu sous le curseur : un losange sur chaque pièce de mur, et le contour du rectangle.
    pub(crate) fn draw_room_overlay(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        view: Rect,
        hex: u32,
    ) {
        let Some(model) = self.room_model() else {
            return;
        };
        // Le coin se pose sur le quadrillage des cases de sol (colonne et ligne paires) : l'aperçu garde la même disposition partout.
        let hex = if self.edit.room.edit_shell.is_some() { hex } else { snap_corner(hex) };
        let (rows, cols) = (self.edit.room.rows as u32, self.edit.room.cols as u32);
        let (nx, ny) = (
            self.edit.room.nx.max(1) as u32,
            self.edit.room.ny.max(1) as u32,
        );
        let plan = if self.edit.room.edit_shell.is_some() {
            self.edited_building_plan(&model)
        } else {
            plan_building(
                &model,
                hex,
                nx,
                ny,
                rows,
                cols,
                &self.decor_options(self.edit.room.seed),
            )
        };
        let cam = self.camera;
        let to_screen = |h: u32| {
            let (x, y) = geom::hex_to_screen(h);
            cam.world_to_screen(view, Pos2::new(x as f32, y as f32))
        };
        for &h in &plan.hexes {
            painter.circle_filled(
                to_screen(h),
                2.0 * cam.zoom.clamp(0.6, 1.5),
                Color32::from_white_alpha(60),
            );
        }
        // Aperçu en transparence : les murs, portes et meubles tels qu'ils seront posés.
        let pieces: Vec<_> = plan
            .walls
            .iter()
            .map(|&(h, fid)| (h, fid, 0u8, (0, 0)))
            .chain(
                plan.doors
                    .iter()
                    .chain(plan.furniture.iter())
                    .map(|p| (p.hex, p.fid, p.orient, p.offset)),
            )
            .collect();
        let mut sorted = pieces;
        sorted.sort_by_key(|&(h, _, _, _)| (h / 200, h % 200));
        for (h, fid, orient, offset) in sorted {
            let sprite = self
                .game
                .as_ref()
                .and_then(|g| g.art.object_sprite(Fid(fid), orient as u32, 0))
                .map(|(_, s)| s);
            let Some(sprite) = sprite else {
                let c = to_screen(h);
                let r = 8.0 * cam.zoom.clamp(0.6, 1.5);
                painter.add(egui::Shape::closed_line(
                    vec![
                        c + egui::vec2(0.0, -r),
                        c + egui::vec2(r, 0.0),
                        c + egui::vec2(0.0, r),
                        c + egui::vec2(-r, 0.0),
                    ],
                    Stroke::new(1.5, Color32::from_rgb(255, 200, 80)),
                ));
                continue;
            };
            let (hx, hy) = geom::hex_to_screen(h);
            let (hx, hy) = (hx + offset.0, hy + offset.1);
            let min = cam.world_to_screen(
                view,
                Pos2::new(
                    hx as f32 + sprite.origin_x as f32,
                    hy as f32 + sprite.origin_y as f32,
                ),
            );
            let max = cam.world_to_screen(
                view,
                Pos2::new(
                    hx as f32 + sprite.origin_x as f32 + sprite.width as f32,
                    hy as f32 + sprite.origin_y as f32 + sprite.height as f32,
                ),
            );
            let tex = self.texture_for(ctx, &sprite);
            painter.image(
                tex,
                Rect::from_min_max(min, max),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::from_white_alpha(150),
            );
        }
    }
}
