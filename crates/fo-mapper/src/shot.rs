//! Capture d'écran pour les essais visuels (`FO_SHOT=<fichier.png>`) : une fois la carte affichée et la mise en page
//! stabilisée, l'image de la fenêtre est enregistrée, puis l'application se ferme. `FO_SHOT_MODE=scripts` bascule d'abord
//! sur l'onglet des scripts, `FO_SHOT_DIALOG=prefs|confirm|new|open|save` ouvre une fenêtre.

use crate::app::{Confirm, MapperApp, Mode};
use eframe::egui;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static FRAMES: AtomicU32 = AtomicU32::new(0);
static ASKED: AtomicBool = AtomicBool::new(false);
static DONE: AtomicBool = AtomicBool::new(false);

pub(crate) fn drive(app: &mut MapperApp, ctx: &egui::Context, raw: &mut egui::RawInput) {
    let Some(path) = std::env::var_os("FO_SHOT") else {
        return;
    };
    ctx.request_repaint();
    if DONE.load(Ordering::Relaxed) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        return;
    }
    let n = FRAMES.fetch_add(1, Ordering::Relaxed);
    if n == 5 {
        if let Some(l) = std::env::var("FO_SHOT_LANG")
            .ok()
            .and_then(|c| crate::i18n::Lang::from_code(&c))
        {
            app.prefs.lang = l;
        }
        if std::env::var("FO_SHOT_THEME").as_deref() == Ok("light") {
            app.prefs.theme = crate::prefs::ThemeChoice::Light;
        }
        match std::env::var("FO_SHOT_MODE").as_deref() {
            Ok("scripts") => {
                app.mode = Mode::Scripts;
                if let Some(g) = &app.game {
                    app.scripts.open_file(&g.data, "arvillag.int");
                }
            }
            Ok("world") => app.mode = Mode::World,
            _ => {}
        }
        if std::env::var_os("FO_SHOT_SSL").is_some() {
            app.mode = Mode::Scripts;
            app.scripts.show_source_editor();
        }
        if std::env::var_os("FO_SHOT_PALETTE").is_some() {
            app.edit.room.palette_open = true;
        }
        match std::env::var("FO_SHOT_DIALOG").as_deref() {
            Ok("prefs") => {
                app.prefs_open = true;
                app.prefs_tab = std::env::var("FO_SHOT_PREFS_TAB")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
            }
            Ok("confirm") => app.confirm = Some(Confirm::Quit),
            Ok("new") => app.open_new_map_dialog(),
            Ok("maps") => app.open_maps_txt(),
            Ok("open") => app.open_dialog(crate::file_dialog::Purpose::OpenGame),
            Ok("save") => app.open_dialog(crate::file_dialog::Purpose::SaveMapAs),
            _ => {}
        }
    }
    // FO_SHOT_CTRLS=1 (avec FO_SELFTEST_WRITE_OK=1, sur un dossier de jeu jetable) : ajoute un objet puis injecte un vrai
    // Ctrl+S ; le résultat est écrit sur la sortie d'erreur.
    if std::env::var_os("FO_SHOT_CTRLS").is_some() {
        if std::env::var_os("FO_SELFTEST_WRITE_OK").is_none() {
            if n == 140 {
                eprintln!(
                    "CTRLS refusé : l'essai écrit dans le dossier du jeu (FO_SELFTEST_WRITE_OK=1 sur une copie jetable)."
                );
            }
        } else {
            if n == 130 {
                app.edit.enabled = true;
                let changed = app.mutate_as("Essai", |m, _| {
                    let Some(mut o) = m.objects[0].iter().find(|o| o.tile() >= 0).cloned() else {
                        return false;
                    };
                    o.words[fo_core::map::word::TILE] = 20_000;
                    o.words[fo_core::map::word::ID] = m.max_object_id() + 1;
                    m.add_object(0, o);
                    true
                });
                eprintln!("CTRLS modification={changed} dirty={}", app.edit.dirty);
            }
            if n == 135 {
                let mods = egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..egui::Modifiers::NONE
                };
                raw.events.push(egui::Event::ModifiersChanged(mods));
                for pressed in [true, false] {
                    raw.events.push(egui::Event::Key {
                        key: egui::Key::S,
                        physical_key: Some(egui::Key::S),
                        pressed,
                        repeat: false,
                        modifiers: mods,
                    });
                }
            }
            if n == 136 {
                raw.events
                    .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
            }
            if n == 140 {
                eprintln!("CTRLS status={} dirty={}", app.status, app.edit.dirty);
            }
        }
    }
    let room_shot = std::env::var("FO_SHOT_ROOM").ok();
    if n == 5 && (std::env::var_os("FO_SHOT_ANALYZE").is_some() || room_shot.is_some()) {
        app.start_tile_analysis(ctx);
        app.analysis.suggest = true;
    }
    if n == 100
        && let Ok(second) = std::env::var("FO_SHOT_OPEN2")
    {
        app.request_load(&second, ctx);
    }
    if n == 120 && std::env::var_os("FO_SHOT_STRETCH").is_some() {
        let e = app.elevation;
        let walls: Vec<(usize, i32)> = app
            .current
            .as_ref()
            .map(|c| {
                c.map.objects[e]
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| {
                        fo_scene::object_layer(o) == Some(fo_scene::Layer::Wall) && o.tile() >= 0
                    })
                    .map(|(i, o)| (i, o.tile()))
                    .collect()
            })
            .unwrap_or_default();
        let count_in = |t0: i32| {
            let (x0, y0) = fo_core::geom::hex_to_screen(t0 as u32);
            let p0 = egui::Pos2::new(x0 as f32, y0 as f32);
            let iso = crate::select_ops::IsoRect::new(p0, p0 + egui::Vec2::new(0.0, 192.0));
            let picked: Vec<fo_scene::ObjectRef> = walls
                .iter()
                .filter(|(_, t)| {
                    let (x, y) = fo_core::geom::hex_to_screen(*t as u32);
                    iso.contains(egui::Pos2::new(x as f32, y as f32))
                })
                .map(|&(i, _)| fo_scene::ObjectRef {
                    elevation: e as u8,
                    index: i as u32,
                })
                .collect();
            (p0, picked)
        };
        let best = walls
            .iter()
            .take(600)
            .map(|&(_, t)| count_in(t))
            .max_by_key(|(_, p)| p.len());
        if let Some((p0, picked)) = best {
            eprintln!("stretch: {} murs", picked.len());
            app.selection.set_objects(picked);
            app.camera.center = p0 + egui::Vec2::new(0.0, 96.0);
            app.camera.zoom = 1.0;
        }
    }
    if n == 125 && std::env::var_os("FO_SHOT_STRETCH").is_some() {
        app.edit.stretch = (8, 0);
        app.stretch_for_test(8, 0);
    }
    if n == 130 && std::env::var_os("FO_SHOT_SPLIT").is_some() {
        app.set_split(Some(0));
    }
    if n == 140 && std::env::var_os("FO_SHOT_GRID").is_some() {
        app.show_grid = true;
        app.camera.zoom = 1.5;
    }
    if n == 140 && std::env::var_os("FO_SHOT_WALK").is_some() {
        app.walk.on = true;
    }
    if n == 140 && std::env::var_os("FO_SHOT_LIGHTS").is_some() {
        app.show_lights = true;
    }
    // FO_SHOT_ROOM=<style> : après l'analyse, pose une pièce dans la carte (en mémoire seulement) avant la capture.
    static ROOM_DONE: AtomicBool = AtomicBool::new(false);
    if let Some(style) = &room_shot
        && app.analysis.result.is_some()
        && app.scene().is_some()
        && !ROOM_DONE.swap(true, Ordering::Relaxed)
    {
        app.edit.enabled = true;
        app.edit.tool = crate::edit_ui::Tool::Room;
        app.shot_place_room(style);
    }
    if n == 120 && std::env::var_os("FO_SHOT_CHECK").is_some() {
        app.check.open = true;
        app.run_check();
    }
    if std::env::var_os("FO_SHOT_PALETTE").is_some() && ROOM_DONE.load(Ordering::Relaxed) {
        app.edit.tool = crate::edit_ui::Tool::Wall;
    }
    if n == 140 && std::env::var_os("FO_SHOT_LINKS").is_some() {
        app.links.open = true;
    }
    // FO_SHOT_DIFF=1 : modifie quelques objets en mémoire, compare à la version enregistrée et ouvre la fenêtre.
    if n == 120 && std::env::var_os("FO_SHOT_DIFF").is_some() {
        app.mutate_as("Essai", |m, _| {
            let ids: Vec<usize> = (0..m.objects[0].len())
                .filter(|&i| {
                    m.objects[0][i].tile() >= 0
                        && m.objects[0][i].object_type() == Some(fo_core::ObjectType::Scenery)
                })
                .take(6)
                .collect();
            for (k, &i) in ids.iter().enumerate() {
                match k {
                    0 | 1 => m.objects[0][i].words[fo_core::map::word::TILE] += 400,
                    2 => m.objects[0][i].words[fo_core::map::word::ORIENTATION] = 3,
                    3 | 4 => {
                        let mut o = m.objects[0][i].clone();
                        o.words[fo_core::map::word::TILE] += 6;
                        o.words[fo_core::map::word::ID] = m.max_object_id() + 1;
                        m.add_object(0, o);
                    }
                    _ => {
                        m.remove_object(0, i);
                    }
                }
            }
            true
        });
        app.diff.open = true;
        app.run_diff();
    }
    // FO_SHOT_VERIFY=1 (avec FO_SELFTEST_WRITE_OK=1, dossier jetable) : annulation d'un bâtiment, remplacement en masse, comparaison à la copie .bak.
    if n == 130
        && std::env::var_os("FO_SHOT_VERIFY").is_some()
        && std::env::var_os("FO_SELFTEST_WRITE_OK").is_some()
    {
        app.verify_new_tools();
    }
    if n == 130
        && std::env::var_os("FO_SHOT_MSG").is_some()
        && std::env::var_os("FO_SELFTEST_WRITE_OK").is_some()
    {
        app.verify_msg_editor();
    }
    // FO_SHOT_PREVIEW=jour|soir|nuit (avec FO_SHOT_FRAME=1 pour le cadre, FO_SHOT_CURSOR=<hex> pour un point de vue) : aperçu du jeu.
    if n == 120
        && let Ok(mode) = std::env::var("FO_SHOT_PREVIEW")
    {
        app.preview.on = true;
        app.preview.ambient = match mode.as_str() {
            "jour" => crate::preview::DAY,
            "nuit" => crate::preview::NIGHT,
            _ => crate::preview::EVENING,
        };
        app.preview.frame = std::env::var_os("FO_SHOT_FRAME").is_some();
        if let Some(hex) = std::env::var("FO_SHOT_VIEWHEX")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
        {
            let (x, y) = fo_core::geom::hex_to_screen(hex);
            app.camera.center = eframe::egui::Pos2::new(x as f32, y as f32);
            app.camera.zoom = 1.0;
            app.needs_fit = false;
            if let Some(cur) = app.current.as_mut() {
                std::sync::Arc::make_mut(&mut cur.map).header.player_tile = hex as i32;
            }
        }
    }
    if n == 100
        && let Some(a) = std::env::var("FO_SHOT_WORLDAREA")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
    {
        app.shot_select_area(a);
    }
    // FO_SHOT_RECOVERY=write|restore (avec FO_RECOVERY_DIR=<dossier jetable>) : essai de la sauvegarde de récupération.
    // FO_SHOT_LIBRARY=1 : crée deux motifs d'essai autour du départ du joueur et ouvre la bibliothèque.
    if n == 140 && std::env::var_os("FO_SHOT_LIBRARY").is_some() {
        let made = app.current.as_ref().map(|c| {
            let start = c.map.header.player_tile.max(0) as u32;
            let objects: Vec<fo_core::pattern::CapturedObject> = c.map.objects[0]
                .iter()
                .filter(|o| {
                    o.tile() >= 0 && fo_core::geom::hex_distance(o.tile() as u32, start) < 14
                })
                .map(|o| fo_core::pattern::CapturedObject {
                    hex: o.tile() as u32,
                    pro_pid: o.pid(),
                    frm_pid: o.fid(),
                    direction: o.orientation().max(0) as u32,
                    flags: o.flags(),
                })
                .collect();
            let floor: Vec<fo_core::pattern::CapturedTile> = (0..12u32)
                .flat_map(|r| (0..12u32).map(move |q| (r, q)))
                .filter_map(|(r, q)| {
                    let sq = (start / 200 / 2 + r) * 100 + (start % 200 / 2 + q);
                    c.map
                        .tile(0, sq as usize)
                        .map(|t| fo_core::pattern::CapturedTile {
                            square: sq,
                            tile_id: t.floor,
                        })
                })
                .collect();
            (start, objects, floor)
        });
        if let Some((start, objects, floor)) = made {
            for (name, tag) in [
                ("Tente du village", "maison"),
                ("Coin d'entrepôt", "entrepôt"),
            ] {
                let v = fo_core::pattern::PatternVariant::capture(
                    "défaut",
                    start,
                    &objects,
                    &floor,
                    &[],
                );
                let mut p = fo_core::pattern::Pattern::new(name, v);
                p.tags = vec![tag.to_string(), "bâtiment".to_string()];
                let _ = app.add_pattern(p);
            }
        }
        app.library.open = true;
        eprintln!("LIBRARY {} motifs", app.edit.stamp.patterns.len());
    }
    if n == 140 && std::env::var_os("FO_SHOT_HEXSEL").is_some() {
        app.edit.enabled = true;
        app.edit.tool = crate::edit_ui::Tool::Select;
        app.edit.select_mode = crate::edit_ui::SelectMode::Hexes;
        if let Some(c) = app.current.as_ref() {
            let start = c.map.header.player_tile.max(0) as u32;
            let picks: Vec<crate::selection::Pick> = (0..fo_core::geom::HEX_COUNT)
                .filter(|h| fo_core::geom::hex_distance(*h, start) <= 3)
                .map(crate::selection::Pick::Hex)
                .collect();
            app.selection
                .apply(crate::selection::Combine::Replace, picks);
        }
    }
    // FO_SHOT_GEN=map|window : après l'analyse, génère un village (et montre la fenêtre de paramètres si « window »).
    static GEN_DONE: AtomicBool = AtomicBool::new(false);
    if let Ok(mode) = std::env::var("FO_SHOT_GEN")
        && app.analysis.result.is_some()
        && app.scene().is_some()
        && !GEN_DONE.swap(true, Ordering::Relaxed)
    {
        app.procgen.params.seed = std::env::var("FO_SHOT_GEN_SEED")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(11);
        if let Some(n) = std::env::var("FO_SHOT_GEN_BUILDINGS")
            .ok()
            .and_then(|v| v.parse().ok())
        {
            app.procgen.params.buildings = n;
        }
        app.procgen.open = mode == "window";
        app.refresh_room_models();
        if let Ok(style) = std::env::var("FO_SHOT_GEN_STYLE") {
            app.edit.room.style = style;
            app.refresh_room_models();
        }
        app.edit.room.roof = std::env::var_os("FO_SHOT_NOROOF").is_none();
        app.run_generator();
        eprintln!("GEN {:?} / {}", app.procgen.report, app.status);
        if let Some(z) = std::env::var("FO_SHOT_GEN_ZOOM")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
        {
            app.camera.zoom = z;
            app.start_view = false;
            app.needs_fit = false;
            app.camera.center = egui::Pos2::new(
                app.current.as_ref().map_or(0, |c| {
                    fo_core::geom::hex_to_screen(c.map.header.player_tile.max(0) as u32).0
                }) as f32
                    + 300.0,
                app.current.as_ref().map_or(0, |c| {
                    fo_core::geom::hex_to_screen(c.map.header.player_tile.max(0) as u32).1
                }) as f32
                    - 1200.0,
            );
        }
    }
    // FO_SHOT_EXPORT=<fichier.png> + FO_SHOT_EXPORT_MODE=game|whole|whole_lit : export d'image.
    let export_shot = std::env::var("FO_SHOT_EXPORT").ok();
    // Avec FO_SHOT_GEN, l'export attend que la carte soit générée (quelques images plus tard, le temps de la dessiner).
    static GEN_AT: AtomicU32 = AtomicU32::new(0);
    if GEN_DONE.load(Ordering::Relaxed) && GEN_AT.load(Ordering::Relaxed) == 0 {
        GEN_AT.store(n.max(1), Ordering::Relaxed);
    }
    let export_now = if std::env::var_os("FO_SHOT_GEN").is_some() {
        GEN_AT.load(Ordering::Relaxed) != 0 && n == GEN_AT.load(Ordering::Relaxed) + 30
    } else {
        n == 130
    };
    if export_now && let Some(path) = &export_shot {
        let mode = std::env::var("FO_SHOT_EXPORT_MODE").unwrap_or_default();
        app.export.game_view = mode == "game";
        app.export.lighting = mode == "whole_lit";
        app.export.scale = std::env::var("FO_SHOT_EXPORT_SCALE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2);
        app.export.ambient = crate::preview::NIGHT;
        app.export_to(std::path::Path::new(path));
        eprintln!("EXPORT {}", app.status);
    }
    // FO_SHOT_QUILT=<carte modèle> : recompose le sol d'une zone autour du départ d'après cette carte (FO_SHOT_QUILT_DECOR=1 sème aussi des décors).
    if n == 130
        && let Ok(model) = std::env::var("FO_SHOT_QUILT")
    {
        app.fill.map = model;
        app.fill.scatter = std::env::var_os("FO_SHOT_QUILT_DECOR").is_some();
        if let Some(c) = app.current.as_ref() {
            let start = c.map.header.player_tile.max(0) as u32;
            let (hc, hr) = fo_core::geom::hex_col_row(start);
            let (sc, sr) = ((hc / 2) as usize, (hr / 2) as usize);
            let squares: Vec<usize> = (sr.saturating_sub(12)..(sr + 12).min(100))
                .flat_map(|r| (sc.saturating_sub(20)..(sc + 20).min(100)).map(move |c| r * 100 + c))
                .collect();
            app.fill_apply(fo_core::map::TileLayer::Floor, &squares, false);
            eprintln!("QUILT {}", app.status);
        }
    }
    let recovery_shot = std::env::var("FO_SHOT_RECOVERY").ok();
    if n == 130 && recovery_shot.as_deref() == Some("write") {
        app.mutate_as("Essai", |m, _| {
            if let Some(mut o) = m.objects[0].iter().find(|o| o.tile() >= 0).cloned() {
                o.words[fo_core::map::word::TILE] = 20_000;
                o.words[fo_core::map::word::ID] = m.max_object_id() + 1;
                m.add_object(0, o);
            }
            true
        });
        app.write_recovery_copies();
        eprintln!(
            "RECOVERY écrit: {} objets",
            app.current.as_ref().map_or(0, |c| c.map.objects[0].len())
        );
    }
    if n == 130 && recovery_shot.as_deref() == Some("restore") {
        let offered = app.recovery.offered.clone();
        eprintln!(
            "RECOVERY proposées: {:?}",
            offered.iter().map(|e| e.file.clone()).collect::<Vec<_>>()
        );
        if let Some(e) = offered.first() {
            app.restore_recovery(e, ctx);
        }
    }
    if n == 250 && recovery_shot.as_deref() == Some("restore") {
        eprintln!(
            "RECOVERY restaurée: {} objets, modifiée={}, onglets={}",
            app.current.as_ref().map_or(0, |c| c.map.objects[0].len()),
            app.edit.dirty,
            app.docs.len()
        );
    }
    let find_shot = std::env::var("FO_SHOT_FIND").ok();
    let find_ready = find_shot
        .as_deref()
        .is_none_or(|q| n > 120 && app.shot_find(q));
    let analysis_ok = (std::env::var_os("FO_SHOT_ANALYZE").is_none() && room_shot.is_none())
        || (app.analysis.result.is_some()
            && (room_shot.is_none() || ROOM_DONE.load(Ordering::Relaxed)));
    let ready = analysis_ok
        && find_ready
        && app.scene().is_some()
        && (recovery_shot.is_none() || n > 260)
        && (export_shot.is_none()
            || (std::env::var_os("FO_SHOT_GEN").is_none() && n > 140)
            || (GEN_AT.load(Ordering::Relaxed) != 0 && n > GEN_AT.load(Ordering::Relaxed) + 45))
        && (std::env::var_os("FO_SHOT_GEN").is_none() || GEN_DONE.load(Ordering::Relaxed))
        || (app.game.is_some()
            && n > 200
            && room_shot.is_none()
            && find_shot.is_none()
            && recovery_shot.is_none()
            && export_shot.is_none()
            && std::env::var_os("FO_SHOT_GEN").is_none());
    if n > 150 && ready && !ASKED.swap(true, Ordering::Relaxed) {
        let callback = egui::ScreenshotCallback::new(move |image| {
            save(&path, &image);
            DONE.store(true, Ordering::Relaxed);
        });
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(callback));
    }
}

fn save(path: &std::ffi::OsStr, image: &egui::ColorImage) {
    let Ok(file) = std::fs::File::create(path) else {
        return;
    };
    let mut enc = png::Encoder::new(
        std::io::BufWriter::new(file),
        image.width() as u32,
        image.height() as u32,
    );
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let Ok(mut w) = enc.write_header() else {
        return;
    };
    let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    let _ = w.write_image_data(&bytes);
}
