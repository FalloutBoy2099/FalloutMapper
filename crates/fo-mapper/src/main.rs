#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod analysis_ui;
mod app;
mod check_ui;
mod construction_ui;
mod diff_ui;
mod edit_ui;
mod effects;
mod exit_ui;
mod export_ui;
mod extras;
mod file_dialog;
mod file_ui;
mod fill_ui;
mod find_ui;
mod highlight;
mod i18n;
mod i18n_de;
mod i18n_es;
mod i18n_it;
mod i18n_pt;
mod i18n_ru;
mod icons;
mod library_ui;
mod light_ui;
mod links_ui;
mod loader;
mod log_ui;
mod minimap;
mod msg_ui;
mod object_ui;
mod prefs;
mod preview;
mod procgen_ui;
mod profiles;
mod recovery;
mod room_ui;
mod script_editor;
mod scripts;
mod select_ops;
mod selection;
mod selftest;
mod shot;
mod spatial_ui;
mod split;
mod stamp_ui;
mod stepper;
mod tabs;
mod toolbar;
mod ui_kit;
mod view;
mod walk_ui;
mod wall_ui;
mod settings;
mod window_state;
mod world_ui;

fn main() -> eframe::Result {
    let initial_game = std::env::args().nth(1).map(std::path::PathBuf::from);
    let initial_map = std::env::args().nth(2);
    let initial_script = std::env::args().nth(3);
    let window = window_state::WindowState::load();
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size(window.start_size())
        .with_title("FalloutMapper");
    // Taille reproductible pour les captures, sans modifier les préférences.
    if std::env::var_os("FO_SHOT").is_some()
        && let Some((width, height)) = std::env::var("FO_SHOT_SIZE").ok().and_then(|s| {
            s.split_once('x')
                .and_then(|(w, h)| Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?)))
        })
        && width >= 640.0
        && height >= 480.0
    {
        viewport = viewport.with_inner_size([width, height]);
    }
    let start_pos = window.start_pos();
    if let Some(p) = start_pos {
        viewport = viewport.with_position(p);
    }
    // Essais automatiques (`FO_HIDDEN=1`) : fenêtre hors de l'écran, sans prendre le focus ni apparaître dans la barre des tâches.
    let hidden = std::env::var_os("FO_HIDDEN").is_some();
    if hidden {
        viewport = viewport
            .with_position([-3000.0, -3000.0])
            .with_active(false)
            .with_taskbar(false);
    }
    let options = eframe::NativeOptions {
        viewport,
        centered: start_pos.is_none() && !hidden,
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "FalloutMapper",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::MapperApp::new(
                cc,
                initial_game,
                initial_map,
                initial_script,
            )))
        }),
    )
}
