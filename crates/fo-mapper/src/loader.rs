//! Chargement d'une carte en arrière-plan : lecture, puis construction des scènes de chaque élévation.

use eframe::egui;
use fo_core::map::{ELEVATION_COUNT, Map, read_map};
use fo_core::{ArtLibrary, GameData};
use fo_scene::{Scene, build_scene};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Instant;

#[derive(Debug)]
pub struct LoadedMap {
    pub file: String,
    pub map: Arc<Map>,
    pub scenes: [Option<Arc<Scene>>; ELEVATION_COUNT],
    pub millis: u128,
}

pub type LoadResult = Result<LoadedMap, (String, String)>;

pub fn spawn_load(
    game: Arc<GameData>,
    art: Arc<ArtLibrary>,
    file: String,
    tx: Sender<LoadResult>,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let result = game
            .read_map_bytes(&file)
            .map_err(|e| e.to_string())
            .and_then(|bytes| build_loaded(&game, &art, &file, &bytes));
        let _ = tx.send(result.map_err(|e: String| (file, e)));
        ctx.request_repaint();
    });
}

/// Comme `spawn_load`, mais à partir d'octets déjà lus (copie de récupération) au lieu du fichier du jeu.
pub fn spawn_load_bytes(
    game: Arc<GameData>,
    art: Arc<ArtLibrary>,
    file: String,
    bytes: Vec<u8>,
    tx: Sender<LoadResult>,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let result = build_loaded(&game, &art, &file, &bytes);
        let _ = tx.send(result.map_err(|e: String| (file, e)));
        ctx.request_repaint();
    });
}

fn build_loaded(
    game: &Arc<GameData>,
    art: &Arc<ArtLibrary>,
    file: &str,
    bytes: &[u8],
) -> Result<LoadedMap, String> {
    let start = Instant::now();
    let mut map = read_map(bytes, &**game).map_err(|e| e.to_string())?;
    map.edge = game.read_edge(file);
    let mut scenes: [Option<Arc<Scene>>; ELEVATION_COUNT] = Default::default();
    for (e, slot) in scenes.iter_mut().enumerate() {
        if map.header.has_elevation(e) || !map.objects[e].is_empty() {
            *slot = Some(Arc::new(build_scene(&map, e, art)));
        }
    }
    Ok(LoadedMap {
        file: file.to_string(),
        map: Arc::new(map),
        scenes,
        millis: start.elapsed().as_millis(),
    })
}
