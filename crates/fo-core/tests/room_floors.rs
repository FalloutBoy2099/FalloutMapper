use fo_core::room::WallStats;
use fo_core::{GameData, read_map};

#[test]
fn original_buildings_recommend_their_own_interior_floors() {
    let dir =
        std::env::var("FO2_GAME_DIR").unwrap_or_else(|_| r"C:\Fallout2\F2_RPU_2.4.34_old".into());
    if !std::path::Path::new(&dir).is_dir() {
        return;
    }
    let game = GameData::open(dir).unwrap();
    let mut stats = WallStats::default();
    for entry in game.list_maps() {
        stats.add_map(&read_map(&game.read_map_bytes(&entry.file).unwrap(), &game).unwrap());
    }
    let walls = game.list("art/walls/walls.lst");
    let tiles = game.list("art/tiles/tiles.lst");
    let sets = stats.sets(&walls);
    for (wall, expected) in [
        ("va5001.frm", "rst"),
        ("dv1004.frm", "brn"),
        ("ENCLAV06.FRM", "encl"),
        ("adb1027.frm", "towr"),
    ] {
        let fid = 0x0300_0000
            | walls
                .iter()
                .position(|n| n.eq_ignore_ascii_case(wall))
                .unwrap() as u32;
        let set = sets.iter().find(|s| s.members.contains(&fid)).unwrap();
        let model = stats.model(&|f| set.members.contains(&f)).strong_only();
        let (family, ids) = model
            .floor_choice(&tiles)
            .expect("un sol intérieur observé");
        assert_eq!(family, expected, "sol associé à {wall}");
        assert!(!ids.is_empty());
        assert!(
            ids.iter()
                .all(|&id| fo_core::tile_stats::family_of(&tiles[id as usize]) == family)
        );
        let plan = fo_core::room::plan_room(&model, 20_050, 12, 12);
        let squares = fo_core::build::interior_squares(&plan.hexes);
        assert!(!squares.is_empty());
        let mut map = fo_core::Map::new_empty("floor12", 1);
        fo_core::build::fill_tiles(
            &mut map,
            0,
            fo_core::map::TileLayer::Floor,
            &squares,
            &ids,
            None,
            1,
        );
        assert!(
            squares
                .iter()
                .all(|&sq| ids.contains(&map.tile(0, sq).unwrap().floor))
        );
    }
}
