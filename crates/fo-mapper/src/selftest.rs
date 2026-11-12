//! Essai automatique de l'interface (`FO_SELFTEST=<fichier de rapport>`) : des événements souris et clavier sont injectés
//! dans la boucle d'egui (via `raw_input_hook`), donc les vrais gestes — appui, glissement, relâchement, touches — passent
//! par le même code que pour un utilisateur. Le rapport est écrit dans le fichier, puis l'application se ferme.

use crate::app::MapperApp;
use crate::edit_ui::Tool;
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, Vec2};
use fo_core::geom;
use fo_core::map::word;
use fo_scene::{Layer, ObjectRef};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;

static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
static PRESELECTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static SELECTED_ORIENTATION: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

fn log(line: impl Into<String>) {
    LOG.lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(line.into());
}

/// Consigne un résultat d'essai.
fn check(name: &str, ok: bool, detail: impl std::fmt::Display) {
    log(format!(
        "{} {name} — {detail}",
        if ok { "OK   " } else { "ÉCHEC" }
    ));
}

pub(crate) struct SelfTest {
    steps: VecDeque<Step>,
    out: PathBuf,
}

enum Step {
    Wait(usize),
    Event(Vec<Event>, Modifiers),
    Run(Box<dyn FnOnce(&mut MapperApp) -> Vec<Step>>),
}

impl SelfTest {
    pub(crate) fn from_env() -> Option<Self> {
        let out = PathBuf::from(std::env::var_os("FO_SELFTEST")?);
        // L'essai enregistre des cartes, maps.txt, des motifs… dans le dossier du jeu : jamais sur une vraie installation.
        if std::env::var_os("FO_SELFTEST_WRITE_OK").is_none() {
            let _ = std::fs::write(&out, "Essai refusé : il écrit dans le dossier du jeu. Utilisez une copie jetable et définissez FO_SELFTEST_WRITE_OK=1.
");
            return None;
        }
        Some(Self {
            steps: VecDeque::from([Step::Wait(5), wait_ready()]),
            out,
        })
    }
}

pub(crate) fn drive(app: &mut MapperApp, ctx: &egui::Context, raw: &mut egui::RawInput) {
    let Some(mut st) = app.selftest.take() else {
        return;
    };
    ctx.request_repaint();
    match st.steps.pop_front() {
        Some(Step::Wait(n)) if n > 1 => st.steps.push_front(Step::Wait(n - 1)),
        Some(Step::Event(events, _mods)) => {
            raw.events.extend(events);
        }
        Some(Step::Run(f)) => {
            for s in f(app).into_iter().rev() {
                st.steps.push_front(s);
            }
        }
        _ => {}
    }
    if st.steps.is_empty() {
        let lines = LOG.lock().unwrap_or_else(|e| e.into_inner()).join("\n");
        let _ = std::fs::write(&st.out, format!("{lines}\n"));
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    } else {
        app.selftest = Some(st);
    }
}

// ------------------------------------------------------------------ briques

fn mv(p: Pos2) -> Step {
    Step::Event(vec![Event::PointerMoved(p)], Modifiers::NONE)
}

fn button(p: Pos2, pressed: bool) -> Step {
    Step::Event(
        vec![Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }],
        Modifiers::NONE,
    )
}

fn key(k: Key, ctrl: bool) -> Vec<Step> {
    let m = if ctrl {
        Modifiers {
            ctrl: true,
            command: true,
            ..Modifiers::NONE
        }
    } else {
        Modifiers::NONE
    };
    let ev = |pressed| Event::Key {
        key: k,
        physical_key: Some(k),
        pressed,
        repeat: false,
        modifiers: m,
    };
    // Les modificateurs doivent rester actifs jusqu'à la fin du traitement de l'image : on les relâche à l'image suivante.
    vec![
        Step::Event(vec![Event::ModifiersChanged(m), ev(true), ev(false)], m),
        Step::Event(
            vec![Event::ModifiersChanged(Modifiers::NONE)],
            Modifiers::NONE,
        ),
    ]
}

fn run(f: impl FnOnce(&mut MapperApp) -> Vec<Step> + 'static) -> Step {
    Step::Run(Box::new(f))
}

fn click(p: Pos2) -> Vec<Step> {
    vec![
        mv(p),
        Step::Wait(2),
        button(p, true),
        Step::Wait(2),
        button(p, false),
        Step::Wait(3),
    ]
}

fn drag(from: Pos2, to: Pos2) -> Vec<Step> {
    let mut v = vec![mv(from), Step::Wait(2), button(from, true), Step::Wait(2)];
    for i in 1..=8 {
        v.push(mv(from + (to - from) * (i as f32 / 8.0)));
        v.push(Step::Wait(1));
    }
    v.push(Step::Wait(3));
    v.push(button(to, false));
    v.push(Step::Wait(4));
    v
}

fn wait_ready() -> Step {
    // La mise en page se stabilise sur quelques images (largeur des panneaux, ajustement de la vue) : on attend que
    // la zone et la caméra ne bougent plus avant de calculer des positions d'écran.
    static LAST: Mutex<Option<(egui::Rect, egui::Pos2, f32, usize)>> = Mutex::new(None);
    run(|app| {
        let state = (app.canvas_rect, app.camera.center, app.camera.zoom);
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        let stable = match *last {
            Some((r, c, z, n)) if (r, c, z) == state => n + 1,
            _ => 0,
        };
        *last = Some((state.0, state.1, state.2, stable));
        let ready = app.scene().is_some()
            && app.canvas_rect.width() > 100.0
            && app.edit.enabled
            && stable >= 10;
        drop(last);
        if ready && !PRESELECTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            // Une sélection change la mise en page (panneau de droite) : on la fait d'abord, puis on attend que tout se stabilise.
            let first = app.scene().and_then(|sc| {
                sc.items
                    .iter()
                    .find(|i| i.layer == Layer::Scenery)
                    .and_then(|i| i.object)
            });
            app.selection.set_object(first);
            *LAST.lock().unwrap_or_else(|e| e.into_inner()) = None;
            return vec![Step::Wait(10), wait_ready()];
        }
        if ready {
            setup(app)
        } else {
            vec![Step::Wait(2), wait_ready()]
        }
    })
}

fn object(app: &MapperApp, r: ObjectRef) -> Option<&fo_core::MapObject> {
    app.current
        .as_ref()?
        .map
        .objects
        .get(r.elevation as usize)?
        .get(r.index as usize)
}

fn world_to_screen(app: &MapperApp, x: f32, y: f32) -> Pos2 {
    app.camera.world_to_screen(app.canvas_rect, Pos2::new(x, y))
}

// ------------------------------------------------------------------ scénario

fn setup(app: &mut MapperApp) -> Vec<Step> {
    let Some(scene) = app.scene().cloned() else {
        return Vec::new();
    };
    let rect = app.canvas_rect;
    let e = app.elevation;

    // Un décor bien visible, dont le centre désigne bien lui-même (pas un voisin devant lui).
    let mut found = None;
    for it in scene.items.iter().filter(|i| i.layer == Layer::Scenery) {
        let Some(r) = it.object else { continue };
        let (cx, cy) = (
            it.x + it.sprite.width as i32 / 2,
            it.y + it.sprite.height as i32 / 2,
        );
        let sp = world_to_screen(app, cx as f32, cy as f32);
        if rect.shrink(140.0).contains(sp)
            && scene.pick(app.layers, cx, cy).and_then(|i| i.object) == Some(r)
        {
            found = Some((r, sp));
            break;
        }
    }
    let Some((target, start)) = found else {
        check("trouver un décor visible", false, "aucun");
        return Vec::new();
    };
    let o0 = object(app, target).cloned().unwrap();
    let orig_tile = o0.tile();
    // Vers le centre de la vue, pour rester sur la carte quelle que soit la position de l'objet.
    let dest = start
        + Vec2::new(
            if start.x > rect.center().x {
                -90.0
            } else {
                90.0
            },
            if start.y > rect.center().y {
                -40.0
            } else {
                40.0
            },
        );
    let dest_world = app.camera.screen_to_world(rect, dest);
    let expected_hex = geom::screen_to_hex(dest_world.x as i32, dest_world.y as i32);
    let count0 = app.current.as_ref().unwrap().map.objects[e].len();
    log(format!(
        "cam0 {:?} {} rect {:?} screen {:?}",
        app.camera.center,
        app.camera.zoom,
        rect,
        app.ctx_handle.as_ref().map(|c| c.content_rect())
    ));
    log(format!(
        "cible : objet {} (hex {orig_tile}) en {:.0},{:.0} → {:.0},{:.0} (hex attendu {expected_hex:?})",
        target.index, start.x, start.y, dest.x, dest.y
    ));

    let mut steps = Vec::new();

    // 1. Glisser-déposer : l'objet va sur l'hexagone du relâchement.
    steps.extend(drag(start, dest));
    steps.push(run(move |app| {
        let t = object(app, target).map(|o| o.tile());
        check("glisser-déposer déplace l'objet", t.is_some() && t != Some(orig_tile) && t.map(|t| t as u32) == expected_hex, format!("hex {orig_tile} → {t:?}, attendu {expected_hex:?}; cam {:?} {} rect {:?} hover {:?} screen {:?}", app.camera.center, app.camera.zoom, app.canvas_rect, app.hover_hex, app.ctx_handle.as_ref().map(|c| c.content_rect())));
        check("la carte est marquée modifiée", app.edit.dirty, "");
        check("l'objet reste sélectionné", app.selection.primary() == Some(target), format!("{:?}", app.selection.primary()));
        Vec::new()
    }));

    // 2. Annuler / rétablir.
    steps.extend(key(Key::Z, true));
    steps.push(Step::Wait(3));
    steps.push(run(move |app| {
        let t = object(app, target).map(|o| o.tile());
        check(
            "Ctrl+Z annule le déplacement",
            t == Some(orig_tile),
            format!("{t:?}"),
        );
        Vec::new()
    }));
    steps.extend(key(Key::Y, true));
    steps.push(Step::Wait(3));
    steps.push(run(move |app| {
        let t = object(app, target).map(|o| o.tile());
        check(
            "Ctrl+Y rétablit le déplacement",
            t.map(|t| t as u32) == expected_hex,
            format!("{t:?}"),
        );
        Vec::new()
    }));

    // 3. Rotation, duplication, suppression (l'objet est maintenant sous `dest`).
    // Annuler / rétablir désélectionne : on clique sur un objet bien visible (n'importe lequel) pour en sélectionner un.
    steps.push(run(move |app| {
        let Some(scene) = app.scene().cloned() else {
            return Vec::new();
        };
        let rect = app.canvas_rect;
        for it in scene
            .items
            .iter()
            .filter(|i| i.layer == Layer::Scenery || i.layer == Layer::Wall)
        {
            let Some(r) = it.object else { continue };
            let (cx, cy) = (
                it.x + it.sprite.width as i32 / 2,
                it.y + it.sprite.height as i32 / 2,
            );
            let sp = world_to_screen(app, cx as f32 + 0.5, cy as f32 + 0.5);
            if rect.shrink(100.0).contains(sp)
                && scene.pick(app.layers, cx, cy).and_then(|i| i.object) == Some(r)
            {
                let orientation = object(app, r).map_or(-1, |o| o.orientation());
                SELECTED_ORIENTATION.store(orientation, std::sync::atomic::Ordering::Relaxed);
                log(format!(
                    "clic de re-sélection sur l'objet {} en {:.0},{:.0}",
                    r.index, sp.x, sp.y
                ));
                return click(sp);
            }
        }
        log("aucun objet cliquable trouvé");
        Vec::new()
    }));
    steps.push(run(move |app| {
        check(
            "un clic sélectionne un objet",
            app.selection.primary().is_some(),
            format!("{:?}", app.selection.primary()),
        );
        Vec::new()
    }));
    steps.extend(key(Key::R, false));
    steps.push(Step::Wait(3));
    steps.push(run(move |app| {
        let before = SELECTED_ORIENTATION.load(std::sync::atomic::Ordering::Relaxed);
        let o = app
            .selection
            .primary()
            .and_then(|r| object(app, r))
            .map(|o| o.orientation());
        check(
            "R fait tourner l'objet",
            o == Some((before + 1).rem_euclid(6)),
            format!("{before} → {o:?}"),
        );
        Vec::new()
    }));
    steps.extend(key(Key::J, true));
    steps.push(Step::Wait(3));
    steps.push(run(move |app| {
        let n = app.current.as_ref().unwrap().map.objects[e].len();
        check(
            "Ctrl+J duplique l'objet",
            n == count0 + 1,
            format!("{count0} → {n} objets"),
        );
        Vec::new()
    }));
    steps.extend(key(Key::Delete, false));
    steps.push(Step::Wait(3));
    steps.push(run(move |app| {
        let n = app.current.as_ref().unwrap().map.objects[e].len();
        check(
            "Suppr supprime l'objet sélectionné",
            n == count0,
            format!("{n} objets"),
        );
        check(
            "plus de sélection après suppression",
            app.selection.primary().is_none(),
            format!("{:?}", app.selection.primary()),
        );
        Vec::new()
    }));

    // 3 bis. Sélection multiple.
    steps.extend(key(Key::D, true));
    steps.push(Step::Wait(2));
    steps.push(run(move |app| {
        check(
            "Ctrl+D désélectionne tout",
            app.selection.is_empty(),
            format!("{} objet(s)", app.selection.objects().len()),
        );
        // Un point vide pour commencer le rectangle.
        let rect = app.canvas_rect;
        let Some(scene) = app.scene().cloned() else {
            return Vec::new();
        };
        let mut start = None;
        'search: for gy in 0..12 {
            for gx in 0..12 {
                let p = rect.min
                    + Vec2::new(
                        rect.width() * (0.1 + 0.07 * gx as f32),
                        rect.height() * (0.1 + 0.07 * gy as f32),
                    );
                let w = app.camera.screen_to_world(rect, p);
                if scene.pick(app.layers, w.x as i32, w.y as i32).is_none() {
                    start = Some(p);
                    break 'search;
                }
            }
        }
        let Some(start) = start else {
            check("trouver un point vide pour la zone", false, "aucun");
            return Vec::new();
        };
        let end = start + Vec2::new(300.0, 220.0);
        // Résultat attendu, calculé indépendamment : objets dont l'hexagone tombe dans le rectangle.
        let (w0, w1) = (
            app.camera.screen_to_world(rect, start),
            app.camera.screen_to_world(rect, end),
        );
        let wrect = crate::select_ops::IsoRect::new(w0, w1);
        let expected: Vec<usize> = app.current.as_ref().unwrap().map.objects[e]
            .iter()
            .enumerate()
            .filter(|(_, o)| {
                fo_scene::object_layer(o).is_some()
                    && (0..geom::HEX_COUNT as i32).contains(&o.tile())
                    && wrect.contains({
                        let (x, y) = geom::hex_to_screen(o.tile() as u32);
                        Pos2::new(x as f32, y as f32)
                    })
            })
            .map(|(i, _)| i)
            .collect();
        log(format!(
            "zone {:.0},{:.0} → {:.0},{:.0} : {} objet(s) attendu(s)",
            start.x,
            start.y,
            end.x,
            end.y,
            expected.len()
        ));
        let mut v = drag(start, end);
        v.push(run(move |app| {
            let got = app.selection.object_indices(e);
            check(
                "la zone sélectionne les objets qu'elle couvre",
                !expected.is_empty() && got == expected,
                format!(
                    "{} sélectionné(s), {} attendu(s)",
                    got.len(),
                    expected.len()
                ),
            );
            Vec::new()
        }));
        v
    }));

    // Tout sélectionner, puis supprimer le groupe et annuler.
    steps.extend(key(Key::A, true));
    steps.push(Step::Wait(2));
    steps.push(run(move |app| {
        let layered = app.current.as_ref().unwrap().map.objects[e]
            .iter()
            .filter(|o| fo_scene::object_layer(o).is_some())
            .count();
        let n = app.selection.object_indices(e).len();
        check(
            "Ctrl+A sélectionne tous les objets visibles",
            n == layered && n > 0,
            format!("{n} / {layered}"),
        );
        Vec::new()
    }));
    steps.extend(key(Key::Delete, false));
    steps.push(Step::Wait(3));
    steps.push(run(move |app| {
        let n = app.current.as_ref().unwrap().map.objects[e].len();
        check(
            "Suppr supprime tout le groupe",
            n < count0,
            format!("{n} objets restants sur {count0}"),
        );
        Vec::new()
    }));
    steps.extend(key(Key::Z, true));
    steps.push(Step::Wait(3));
    steps.push(run(move |app| {
        let n = app.current.as_ref().unwrap().map.objects[e].len();
        check(
            "Ctrl+Z restitue le groupe supprimé",
            n == count0,
            format!("{n} objets"),
        );
        Vec::new()
    }));

    // 4. Peinture d'un sol.
    steps.push(run(move |app| {
        let rect = app.canvas_rect;
        // Un point (et son voisin) sur un sol existant : le zoom de la vue varie d'une exécution à l'autre.
        let Some(p0) = find_point(app, Vec2::new(70.0, 25.0)) else {
            check("trouver un sol pour la peinture", false, "aucun");
            return Vec::new();
        };
        let p1 = p0 + Vec2::new(70.0, 25.0);
        let w = app.camera.screen_to_world(rect, p0);
        let square = geom::screen_to_square(w.x as i32, w.y as i32);
        let before = square
            .and_then(|s| app.current.as_ref().unwrap().map.tile(e, s as usize))
            .map(|t| t.floor);
        let brush = before.map_or(5, |f| if f == 5 { 6 } else { 5 });
        app.edit.tool = Tool::Floor;
        app.edit.brush_tile = brush;
        let mut v = drag(p0, p1);
        v.push(run(move |app| {
            let after = square
                .and_then(|s| app.current.as_ref().unwrap().map.tile(e, s as usize))
                .map(|t| t.floor);
            check(
                "peindre un sol",
                after == Some(brush),
                format!("case {square:?} : {before:?} → {after:?} (pinceau {brush})"),
            );
            Vec::new()
        }));
        v
    }));

    // 4 bis. Sorties de carte, bloqueurs de défilement, limites de défilement.
    steps.push(run(move |app| {
        use crate::exit_ui::{DestKind, ExitMode};
        let rect = app.canvas_rect;
        app.edit.tool = Tool::Exit;
        app.edit.exit.mode = ExitMode::Single;
        app.edit.exit.dir = fo_core::map::exit::DIR_BOTTOM;
        app.edit.exit.kind = DestKind::World;
        // Un hexagone de la carte qui ne porte pas déjà de sortie (certaines cartes en ont près du centre).
        let objects = &app.current.as_ref().unwrap().map.objects[e];
        let candidates = [
            (120.0, -60.0),
            (-120.0, 60.0),
            (60.0, 90.0),
            (-90.0, -90.0),
            (200.0, 20.0),
            (-200.0, -40.0),
        ];
        let p = candidates
            .iter()
            .map(|&(dx, dy)| rect.center() + Vec2::new(dx, dy))
            .find(|&p| {
                let w = app.camera.screen_to_world(rect, p);
                geom::screen_to_hex(w.x as i32, w.y as i32).is_some_and(|h| {
                    !objects
                        .iter()
                        .any(|o| o.tile() == h as i32 && o.is_exit_grid())
                })
            })
            .unwrap_or(rect.center());
        let w = app.camera.screen_to_world(rect, p);
        let hex = geom::screen_to_hex(w.x as i32, w.y as i32);
        let before = app.current.as_ref().unwrap().map.objects[e].len();
        let mut v = vec![Step::Wait(6)];
        v.extend(click(p));
        v.push(run(move |app| {
            let m = &app.current.as_ref().unwrap().map;
            let found = m.objects[e].iter().find(|o| {
                o.is_exit_grid()
                    && Some(o.tile() as u32) == hex
                    && o.pid() == fo_core::map::exit::exit_pid(fo_core::map::exit::DIR_BOTTOM)
            });
            check(
                "l'outil Sortie pose un marqueur sur l'hexagone cliqué",
                m.objects[e].len() == before + 1 && found.is_some(),
                format!(
                    "{before} → {} objets, hex {hex:?}, survol {:?}, outil {:?}, statut {}",
                    m.objects[e].len(),
                    app.hover_hex,
                    app.edit.tool,
                    app.status
                ),
            );
            check(
                "la sortie vers la carte du monde utilise l'image brune",
                found.is_some_and(|o| {
                    o.fid()
                        == fo_core::map::exit::exit_fid(
                            fo_core::map::exit::DIR_BOTTOM,
                            fo_core::map::exit::ExitFamily::World,
                        )
                        && o.exit_destination()
                            .is_some_and(|d| d.map == fo_core::map::exit::WORLD_MAP_EXIT)
                }),
                "",
            );
            Vec::new()
        }));
        v
    }));
    steps.push(run(move |app| {
        use crate::exit_ui::ExitMode;

        app.edit.tool = Tool::Exit;
        app.edit.exit.mode = ExitMode::Edge;
        app.edit.exit.points.clear();
        let Some(p1) = find_point(app, Vec2::new(120.0, 120.0)) else {
            check("trouver un point pour le tracé", false, "aucun");
            return Vec::new();
        };
        let p2 = p1 + Vec2::new(120.0, 120.0);
        let before = app.current.as_ref().unwrap().map.objects[e].len();
        let mut v = click(p1);
        v.extend(click(p2));
        v.extend(key(Key::Enter, false));
        v.push(Step::Wait(3));
        v.push(run(move |app| {
            let m = &app.current.as_ref().unwrap().map;
            let added = m.objects[e].len() - before;
            let dirs: std::collections::BTreeSet<u32> =
                m.objects[e][before..].iter().map(|o| o.pid()).collect();
            check(
                "le tracé d'un bord pose une ligne de marqueurs d'une même direction",
                added >= 3 && dirs.len() == 1,
                format!("{added} marqueur(s), {} direction(s)", dirs.len()),
            );
            Vec::new()
        }));
        v
    }));
    steps.push(run(move |app| {
        let rect = app.canvas_rect;
        app.edit.tool = Tool::Blocker;
        let p0 = rect.center() + Vec2::new(-250.0, -200.0);
        let p1 = p0 + Vec2::new(260.0, 150.0);
        let (w0, w1) = (
            app.camera.screen_to_world(rect, p0),
            app.camera.screen_to_world(rect, p1),
        );
        let wr = eframe::egui::Rect::from_two_pos(w0, w1);
        let border = geom::hexes_on_rect_border(wr.min.x, wr.min.y, wr.max.x, wr.max.y);
        let before = app.current.as_ref().unwrap().map.objects[e].len();
        // La carte a déjà des bloqueurs : ceux qui sont sur le contour ne sont pas doublés.
        let already = app.current.as_ref().unwrap().map.objects[e]
            .iter()
            .filter(|o| o.is_scroll_blocker() && border.contains(&(o.tile() as u32)))
            .map(|o| o.tile())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let mut v = drag(p0, p1);
        v.push(run(move |app| {
            let m = &app.current.as_ref().unwrap().map;
            let blockers: Vec<_> = m.objects[e]
                .iter()
                .filter(|o| o.is_scroll_blocker())
                .collect();
            let covered = border
                .iter()
                .filter(|&&h| blockers.iter().any(|o| o.tile() == h as i32))
                .count();
            let added = m.objects[e].len() - before;
            check(
                "le rectangle pose des bloqueurs sur son contour",
                !border.is_empty() && covered == border.len() && added == border.len() - already,
                format!(
                    "{added} posé(s), {already} déjà présent(s), contour de {} hexagone(s)",
                    border.len()
                ),
            );
            check(
                "les bloqueurs utilisent le prototype Scroll Blocker",
                blockers
                    .iter()
                    .all(|o| o.pid() == fo_core::map::exit::SCROLL_BLOCKER_PID),
                "",
            );
            Vec::new()
        }));
        v
    }));
    steps.push(run(move |app| {
        app.edit.tool = Tool::Select;
        let ok = app.mutate(Some("edge"), |m, _| {
            let mut edge = fo_core::edge::MapEdge::default();
            edge.elevations[0].zones = vec![fo_core::edge::EdgeRect {
                left: 100,
                top: 200,
                right: 300,
                bottom: 400,
            }];
            m.edge = Some(edge);
            true
        });
        check(
            "une zone de défilement s'ajoute à la carte",
            ok && app.current.as_ref().unwrap().map.edge.is_some(),
            "",
        );
        Vec::new()
    }));

    // 4 ter. Pipette et tampons.
    steps.push(run(move |app| {
        // La pipette prélève d'abord le toit sous le curseur : on masque les toits pour viser l'objet puis le sol.
        app.layers.set(Layer::Roof, false);
        let Some(scene) = app.scene().cloned() else {
            return Vec::new();
        };
        let rect = app.canvas_rect;
        // Un décor ou un mur bien visible sous le curseur.
        let mut found = None;
        for it in scene
            .items
            .iter()
            .filter(|i| i.layer == Layer::Scenery || i.layer == Layer::Wall)
        {
            let Some(r) = it.object else { continue };
            let (cx, cy) = (
                it.x + it.sprite.width as i32 / 2,
                it.y + it.sprite.height as i32 / 2,
            );
            let sp = world_to_screen(app, cx as f32 + 0.5, cy as f32 + 0.5);
            if rect.shrink(140.0).contains(sp)
                && scene.pick(app.layers, cx, cy).and_then(|i| i.object) == Some(r)
            {
                found = Some((r, sp));
                break;
            }
        }
        let Some((r, sp)) = found else {
            check("trouver un objet pour la pipette", false, "aucun");
            return Vec::new();
        };
        let (pid, orientation) =
            object(app, r).map_or((0, 0), |o| (o.pid(), o.orientation().clamp(0, 5) as u32));
        app.edit.tool = Tool::Select;
        let mut v = vec![mv(sp), Step::Wait(3)];
        v.extend(key(Key::P, false));
        v.push(Step::Wait(3));
        v.push(run(move |app| {
            check(
                "P (pipette) sur un objet arme le placement de cet objet",
                app.edit.tool == Tool::Place
                    && app.edit.place.as_ref().is_some_and(|p| p.pid == pid)
                    && app.edit.place_orientation == orientation,
                format!(
                    "outil {:?}, proto {:?}",
                    app.edit.tool,
                    app.edit.place.as_ref().map(|p| p.pid)
                ),
            );
            Vec::new()
        }));
        v
    }));
    steps.push(run(move |app| {
        // Un point sans objet mais avec un sol, sans toit.
        let Some(scene) = app.scene().cloned() else {
            return Vec::new();
        };
        let rect = app.canvas_rect;
        let mut found = None;
        'search: for gy in 0..14 {
            for gx in 0..14 {
                let p = rect.min
                    + Vec2::new(
                        rect.width() * (0.08 + 0.065 * gx as f32),
                        rect.height() * (0.08 + 0.065 * gy as f32),
                    );
                let w = app.camera.screen_to_world(rect, p);
                let (x, y) = (w.x as i32, w.y as i32);
                let map = &app.current.as_ref().unwrap().map;
                let no_object = !scene
                    .items
                    .iter()
                    .any(|i| i.object.is_some() && i.sprite.alpha_at(x - i.x, y - i.y) > 16);
                let roof_free = geom::screen_to_square(x, y - geom::ROOF_OFFSET_Y)
                    .and_then(|s| map.tile(e, s as usize))
                    .is_none_or(|t| t.roof == fo_core::map::EMPTY_TILE);
                let floor = geom::screen_to_square(x, y)
                    .and_then(|s| map.tile(e, s as usize))
                    .filter(|t| t.floor != fo_core::map::EMPTY_TILE);
                if no_object
                    && roof_free
                    && let Some(t) = floor
                {
                    found = Some((p, t.floor));
                    break 'search;
                }
            }
        }
        let Some((p, floor)) = found else {
            check("trouver un sol nu pour la pipette", false, "aucun");
            return Vec::new();
        };
        app.edit.tool = Tool::Select;
        let mut v = vec![mv(p), Step::Wait(3)];
        v.extend(key(Key::P, false));
        v.push(Step::Wait(3));
        v.push(run(move |app| {
            check(
                "P (pipette) sur un sol arme la peinture avec cette tuile",
                app.edit.tool == Tool::Floor && app.edit.brush_tile == floor,
                format!(
                    "outil {:?}, tuile {} (attendu {floor})",
                    app.edit.tool, app.edit.brush_tile
                ),
            );
            Vec::new()
        }));
        v
    }));
    steps.push(run(move |app| {
        // Capture de décors voisins, puis pose du motif ailleurs.
        let (n_before, group): (usize, Vec<u32>) = {
            let m = &app.current.as_ref().unwrap().map;
            let mut group: Vec<u32> = Vec::new();
            for (i, o) in m.objects[e].iter().enumerate() {
                if o.object_type() == Some(fo_core::ObjectType::Scenery)
                    && (0..geom::HEX_COUNT as i32).contains(&o.tile())
                    && !o.has_flag(fo_core::map::flags::HIDDEN)
                {
                    if group.is_empty()
                        || geom::hex_distance(
                            m.objects[e][group[0] as usize].tile() as u32,
                            o.tile() as u32,
                        ) <= 6
                    {
                        group.push(i as u32);
                    }
                    if group.len() == 3 {
                        break;
                    }
                }
            }
            (m.objects[e].len(), group)
        };
        if group.len() < 2 {
            check(
                "trouver un groupe de décors pour le motif",
                false,
                "trop peu",
            );
            return Vec::new();
        }
        let refs: Vec<fo_scene::ObjectRef> = group
            .iter()
            .map(|&i| fo_scene::ObjectRef {
                elevation: e as u8,
                index: i,
            })
            .collect();
        app.selection.set_objects(refs);
        let captured = app.capture_named("essai auto");
        check(
            "la capture crée un motif et l'enregistre dans la bibliothèque",
            captured.is_ok(),
            format!("{captured:?}"),
        );
        let n = group.len();
        let originals: Vec<i32> = group
            .iter()
            .map(|&i| app.current.as_ref().unwrap().map.objects[e][i as usize].tile())
            .collect();
        let rect = app.canvas_rect;
        // Un point dont l'hexagone existe : le centre de la vue, légèrement décalé.
        let p = rect.center() + Vec2::new(120.0, -60.0);
        app.edit.tool = Tool::Stamp;
        let mut v = click(p);
        v.push(run(move |app| {
            let m = &app.current.as_ref().unwrap().map;
            let added = m.objects[e].len() - n_before;
            check(
                "un clic pose le motif (autant d'objets que capturés)",
                added == n,
                format!(
                    "{added} posé(s) pour {n} — état : {} ; outil {:?} ; survol {:?} ; motifs {}",
                    app.status,
                    app.edit.tool,
                    app.hover_hex,
                    app.edit.stamp.patterns.len()
                ),
            );
            // Mêmes décalages (colonne, ligne) entre les copies et entre les originaux.
            let rel = |a: i32, b: i32| ((b % 200) - (a % 200), (b / 200) - (a / 200));
            let copies: Vec<i32> = m.objects[e][n_before..].iter().map(|o| o.tile()).collect();
            let same_shape = copies.len() == originals.len()
                && (1..originals.len())
                    .all(|k| rel(copies[0], copies[k]) == rel(originals[0], originals[k]));
            check(
                "le motif posé garde la disposition des objets capturés",
                same_shape,
                format!("{originals:?} → {copies:?}"),
            );
            Vec::new()
        }));
        v
    }));

    // 4 quater. Créatures, scripts d'objet, inventaire.
    steps.push(run(move |app| {
        let Some(g) = app.game.as_ref() else {
            return Vec::new();
        };
        let entry = g
            .data
            .proto_catalog(fo_core::ObjectType::Critter)
            .into_iter()
            .find(|p| p.info.critter.as_ref().is_some_and(|c| c.hit_points > 0));
        let Some(entry) = entry else {
            check("trouver un prototype de créature", false, "aucun");
            return Vec::new();
        };
        let rect = app.canvas_rect;
        let p = rect.center() + Vec2::new(120.0, -60.0);
        let (pid, hp) = (
            entry.pid,
            entry.info.critter.as_ref().map_or(0, |c| c.hit_points),
        );
        app.edit.place = Some(entry);
        app.edit.place_orientation = 2;
        app.edit.tool = Tool::Place;
        let before = app.current.as_ref().unwrap().map.objects[e].len();
        let mut v = click(p);
        v.push(run(move |app| {
            let m = &app.current.as_ref().unwrap().map;
            let last = m.objects[e].last();
            check(
                "l'outil Placer crée une créature depuis son prototype",
                m.objects[e].len() > before
                    && last.is_some_and(|o| {
                        o.pid() == pid
                            && o.extra.len() == 10
                            && o.extra[7] == hp
                            && o.orientation() == 2
                    }),
                format!("{} → {} objets", before, m.objects[e].len()),
            );
            Vec::new()
        }));
        v
    }));
    steps.push(run(move |app| {
        // Inventaire de la créature qui vient d'être posée (dernier objet).
        let idx = app.current.as_ref().unwrap().map.objects[e].len() - 1;
        app.selection.set_object(Some(fo_scene::ObjectRef {
            elevation: e as u8,
            index: idx as u32,
        }));
        let weapon = app
            .game
            .as_ref()
            .and_then(|g| {
                g.data
                    .proto_catalog(fo_core::ObjectType::Item)
                    .into_iter()
                    .find(|p| p.info.subtype == 3)
            })
            .map(|p| p.pid);
        let Some(weapon) = weapon else {
            check("trouver une arme pour l'inventaire", false, "aucune");
            return Vec::new();
        };
        let added = app.add_inventory_item(weapon, 2);
        let count = app.current.as_ref().unwrap().map.objects[e][idx]
            .inventory
            .len();
        check(
            "ajouter un objet à l'inventaire",
            added
                && count == 1
                && app.current.as_ref().unwrap().map.objects[e][idx].words[word::INVENTORY_COUNT]
                    == 1,
            format!("{count} objet(s)"),
        );
        let mut v = key(Key::Z, true);
        v.push(Step::Wait(3));
        v.push(run(move |app| {
            let count = app.current.as_ref().unwrap().map.objects[e][idx]
                .inventory
                .len();
            check(
                "Ctrl+Z retire l'objet ajouté",
                count == 0,
                format!("{count} objet(s)"),
            );
            Vec::new()
        }));
        v
    }));
    steps.push(run(move |app| {
        // Script d'un décor : attacher, vérifier le lien, détacher, annuler.
        let idx = app.current.as_ref().unwrap().map.objects[e]
            .iter()
            .position(|o| {
                o.object_type() == Some(fo_core::ObjectType::Scenery) && o.script_id() == -1
            });
        let Some(idx) = idx else {
            check("trouver un décor sans script", false, "aucun");
            return Vec::new();
        };
        app.selection.set_object(Some(fo_scene::ObjectRef {
            elevation: e as u8,
            index: idx as u32,
        }));
        let before =
            app.current.as_ref().unwrap().map.scripts[fo_core::map::SCRIPT_TYPE_ITEM].count;
        let ok = app.attach_script_to_selected(216);
        let m = &app.current.as_ref().unwrap().map;
        let linked = m
            .find_script(m.objects[e][idx].script_id())
            .is_some_and(|s| s.list_index() == 216);
        check(
            "attacher un script à un objet",
            ok && linked && m.scripts[fo_core::map::SCRIPT_TYPE_ITEM].count == before + 1,
            format!(
                "{} → {}",
                before,
                m.scripts[fo_core::map::SCRIPT_TYPE_ITEM].count
            ),
        );
        let detached = app.detach_script_from_selected();
        let m = &app.current.as_ref().unwrap().map;
        check(
            "détacher le script",
            detached
                && m.objects[e][idx].script_id() == -1
                && m.scripts[fo_core::map::SCRIPT_TYPE_ITEM].count == before,
            "",
        );
        let mut v = key(Key::Z, true);
        v.push(Step::Wait(3));
        v.push(run(move |app| {
            let m = &app.current.as_ref().unwrap().map;
            check(
                "Ctrl+Z restitue le script détaché",
                m.objects[e][idx].script_id() != -1
                    && m.scripts[fo_core::map::SCRIPT_TYPE_ITEM].count == before + 1,
                "",
            );
            Vec::new()
        }));
        v
    }));

    // 5. Enregistrement dans data/maps/, puis relecture.
    steps.push(run(|app| {
        let saved = app.save_current();
        check("Ctrl+S / enregistrer", saved, app.status.clone());
        let (Some(cur), Some(g)) = (&app.current, &app.game) else {
            return Vec::new();
        };
        let path = g
            .data
            .root
            .join("data")
            .join("maps")
            .join(format!("{}.map", cur.file));
        match std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|b| fo_core::read_map(&b, &*g.data).map_err(|e| e.to_string()))
        {
            Ok(mut back) => {
                back.edge = cur.map.edge.clone(); // le .edg est un fichier à part
                check(
                    "le fichier enregistré se relit à l'identique",
                    back == *cur.map,
                    path.display(),
                );
            }
            Err(e) => check("le fichier enregistré se relit", false, e),
        }
        check(
            "plus de marque « modifiée » après l'enregistrement",
            !app.edit.dirty,
            "",
        );
        check(
            "le .edg est enregistré à côté de la carte et se relit",
            g.data.read_edge(&cur.file) == cur.map.edge && cur.map.edge.is_some(),
            "",
        );
        let _ = word::TILE;
        Vec::new()
    }));
    // 6. Nouvelle carte, maps.txt, carte du monde, miniatures, fermeture.
    steps.push(run(move |app| {
        let Some(ctx) = app.ctx_handle.clone() else {
            check("contexte egui disponible", false, "absent");
            return Vec::new();
        };
        app.edit.dirty = false;
        app.create_new_map("zznewmap", 2);
        let cur = app.current.as_ref().unwrap();
        check(
            "Nouvelle carte : vide, 2 élévations, non enregistrée",
            cur.file == "zznewmap"
                && cur.map.header.has_elevation(1)
                && !cur.map.header.has_elevation(2)
                && cur.map.object_count() == 0
                && app.edit.dirty
                && cur.scenes[0].is_some()
                && cur.scenes[1].is_some(),
            format!("{}", cur.file),
        );
        // Un objet, puis enregistrement et relecture.
        let proto = app.game.as_ref().and_then(|g| {
            g.data
                .proto_catalog(fo_core::ObjectType::Scenery)
                .into_iter()
                .next()
        });
        if let Some(p) = proto {
            app.mutate(None, |m, _| {
                let id = m.max_object_id() + 1;
                if let Some(o) = fo_core::MapObject::from_proto(&p.info, 20_100, id) {
                    m.add_object(0, o);
                }
                true
            });
        }
        let saved = app.save_current();
        let (Some(cur), Some(g)) = (&app.current, &app.game) else {
            return Vec::new();
        };
        let path = g.data.root.join("data").join("maps").join("zznewmap.map");
        let back = std::fs::read(&path)
            .ok()
            .and_then(|b| fo_core::read_map(&b, &*g.data).ok());
        check(
            "la nouvelle carte s'enregistre et se relit",
            saved && back.is_some_and(|b| b.object_count() == cur.map.object_count()),
            path.display(),
        );

        // maps.txt : ajout d'une entrée pour la nouvelle carte, enregistrement, liste des cartes mise à jour.
        app.open_maps_txt();
        let had = app.world.maps_txt.is_some();
        app.maps_txt_add_for_test("zznewmap", "Carte de test");
        app.save_maps_txt();
        let in_list = app.game.as_ref().is_some_and(|g| {
            g.maps.iter().any(|m| {
                m.file == "zznewmap"
                    && m.number.is_some()
                    && m.display.as_deref() == Some("Carte de test")
            })
        });
        let written = app
            .game
            .as_ref()
            .map(|g| {
                g.data
                    .root
                    .join("data")
                    .join("data")
                    .join("maps.txt")
                    .exists()
            })
            .unwrap_or(false);
        check(
            "maps.txt : l'entrée ajoutée est enregistrée et la carte apparaît dans la liste",
            had && written && in_list,
            format!("fenêtre {had}, fichier {written}, liste {in_list}"),
        );
        app.world.maps_txt = None;

        // Carte du monde.
        app.open_world_map(&ctx);
        let (areas, bg) = app.world_summary();
        check(
            "la carte du monde charge les zones et le fond",
            areas > 30 && bg,
            format!("{areas} zones, fond {bg}"),
        );
        app.world.world = None;

        // Miniatures : attendre quelques-unes.
        app.toggle_thumbnails(&ctx);
        thumbs_wait(0)
    }));
    steps.push(run(|app| {
        check(
            "fermer la carte",
            {
                app.edit.dirty = false;
                // Avec les onglets, fermer la carte affiche la voisine : on les ferme toutes.
                for _ in 0..8 {
                    if app.current.is_some() {
                        app.close_map();
                    }
                }
                app.current.is_none()
            },
            "",
        );
        Vec::new()
    }));

    steps
}

/// Attend que quelques miniatures soient prêtes (jusqu'à ~40 s), puis les vérifie et referme la fenêtre.
fn thumbs_wait(attempt: usize) -> Vec<Step> {
    vec![
        Step::Wait(10),
        run(move |app| {
            let n = app.thumbnail_count();
            if n >= 3 || attempt >= 240 {
                check(
                    "les miniatures des cartes se génèrent en arrière-plan",
                    n >= 3,
                    format!("{n} prête(s)"),
                );
                if let Some(ctx) = app.ctx_handle.clone() {
                    app.toggle_thumbnails(&ctx);
                }
                Vec::new()
            } else {
                thumbs_wait(attempt + 1)
            }
        }),
    ]
}

/// Premier point de la vue dont l'hexagone et la case de sol existent, ainsi que celui de `p + delta`.
fn find_point(app: &MapperApp, delta: Vec2) -> Option<Pos2> {
    let rect = app.canvas_rect;
    let map = &app.current.as_ref()?.map;
    let ok = |p: Pos2| {
        let w = app.camera.screen_to_world(rect, p);
        rect.shrink(20.0).contains(p)
            && geom::screen_to_hex(w.x as i32, w.y as i32).is_some()
            && geom::screen_to_square(w.x as i32, w.y as i32)
                .and_then(|s| map.tile(app.elevation, s as usize))
                .is_some_and(|t| t.floor != fo_core::map::EMPTY_TILE)
    };
    (0..16)
        .flat_map(|gy| (0..16).map(move |gx| (gx, gy)))
        .map(|(gx, gy)| {
            rect.min
                + Vec2::new(
                    rect.width() * (0.1 + 0.05 * gx as f32),
                    rect.height() * (0.1 + 0.05 * gy as f32),
                )
        })
        .find(|&p| ok(p) && ok(p + delta))
}
