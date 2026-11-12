use fo_core::{ArtLibrary, GameData, read_map};
use fo_scene::build_scene;
use std::sync::Arc;

#[test]
fn some_maps_use_animated_palette_colors() {
    let Ok(game) = GameData::open(
        std::env::var("FO2_GAME_DIR").unwrap_or_else(|_| r"C:\Fallout2\F2_RPU_2.4.34_old".into()),
    ) else {
        return;
    };
    let game = Arc::new(game);
    let art = ArtLibrary::new(game.clone());
    let mut total = 0;
    for name in ["gstcav2", "arvillag", "vctyctyd", "cave1", "mbase12"] {
        let Ok(bytes) = game.read_map_bytes(name) else {
            continue;
        };
        let map = read_map(&bytes, &*game).unwrap();
        for e in (0..3).filter(|&e| map.header.has_elevation(e)) {
            let scene = build_scene(&map, e, &art);
            let n = scene
                .items
                .iter()
                .filter(|i| i.sprite.anim_mask != 0)
                .count();
            eprintln!(
                "{name} élév. {e} : {n} sprites animés / {}",
                scene.items.len()
            );
            total += n;
        }
    }
    assert!(
        total > 0,
        "aucun sprite animé trouvé : la détection des indices de palette est suspecte"
    );
}
