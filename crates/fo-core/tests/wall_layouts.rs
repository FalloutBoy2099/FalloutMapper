//! Régressions des raccords observés dans les trois styles des captures du village.
use fo_core::room::{DecorOptions, RoomPlan, WallStats, plan_building, plan_room_with};
use fo_core::{GameData, geom, read_map};
use std::collections::HashSet;

fn relative_walls(plan: &RoomPlan, anchor: u32) -> Vec<(u32, i32, i32)> {
    let (ax, ay) = geom::hex_to_screen(anchor);
    let mut points: Vec<_> = plan
        .walls
        .iter()
        .map(|&(hex, fid)| {
            let (x, y) = geom::hex_to_screen(hex);
            (fid, x - ax, y - ay)
        })
        .collect();
    points.sort_unstable();
    points
}

#[test]
fn original_wall_styles_close_rooms_and_shared_partitions_on_both_parities() {
    let dir =
        std::env::var("FO2_GAME_DIR").unwrap_or_else(|_| r"C:\Fallout2\F2_RPU_2.4.34_old".into());
    if !std::path::Path::new(&dir).is_dir() {
        return;
    }
    let game = GameData::open(dir).expect("installation de référence");
    let mut stats = WallStats::default();
    for entry in game.list_maps() {
        let map = read_map(&game.read_map_bytes(&entry.file).unwrap(), &game).unwrap();
        stats.add_map(&map);
    }
    let names = game.list("art/walls/walls.lst");
    let sets = stats.sets(&names);
    for name in ["dv1004", "ENCLAV06", "adb1027"] {
        let set = sets
            .iter()
            .find(|s| s.label == name)
            .expect("famille de référence");
        let model = stats.model(&|f| set.members.contains(&f)).strong_only();
        let opts = DecorOptions {
            front: true,
            ..Default::default()
        };
        let base = plan_room_with(&model, 20_050, 8, 8, &opts);
        assert!(!base.short, "{name}: pièce de référence");
        for seed in 0..12 {
            let doorway = plan_room_with(
                &model,
                20_050,
                if name == "adb1027" { 8 } else { 10 },
                if name == "adb1027" { 8 } else { 10 },
                &DecorOptions {
                    door: true,
                    windows: 2,
                    seed,
                    ..opts.clone()
                },
            );
            assert!(
                !doorway.doors.is_empty(),
                "{name}: porte absente, graine {seed}"
            );
            if name == "ENCLAV06" {
                assert!(
                    doorway
                        .walls
                        .iter()
                        .any(|&(_, fid)| names[(fid & 0xFFFFFF) as usize].starts_with("ENCDOR")),
                    "l'encadrement ENCDOR doit accompagner la porte ENCLAV"
                );
            }
        }
        for anchor in [20_050, 20_051, 20_250, 20_251] {
            let plan = plan_room_with(&model, anchor, 8, 8, &opts);
            assert_eq!(
                relative_walls(&plan, anchor),
                relative_walls(&base, 20_050),
                "{name}: translation {anchor}"
            );
            for cols in 5..=12 {
                for rows in 5..=11 {
                    assert!(
                        !plan_room_with(&model, anchor, rows, cols, &opts).short,
                        "{name}: {cols}×{rows} à {anchor}"
                    );
                }
            }
            for (nx, ny) in [(1, 2), (2, 1), (2, 2), (3, 2), (2, 3), (3, 3)] {
                let plan = plan_building(&model, anchor, nx, ny, 8, 8, &opts);
                assert!(!plan.short, "{name}: bâtiment {nx}×{ny} à {anchor}");
                let objects: Vec<_> = plan
                    .walls
                    .iter()
                    .map(|p| p.0)
                    .chain(plan.doors.iter().map(|p| p.hex))
                    .collect();
                assert_eq!(
                    objects.len(),
                    objects.iter().copied().collect::<HashSet<_>>().len(),
                    "{name}: jonction ou porte doublée"
                );
                assert!(objects.iter().all(|&h| h < geom::HEX_COUNT));
            }
            let varied = plan_building(
                &model,
                anchor,
                3,
                2,
                8,
                8,
                &DecorOptions {
                    vary_sizes: true,
                    seed: 7,
                    ..opts.clone()
                },
            );
            assert!(!varied.short, "{name}: tailles variées à {anchor}");
        }
        let options = fo_core::village::GenOptions {
            params: fo_core::procgen::Params {
                seed: 21,
                buildings: 8,
                vegetation: 0.0,
                props: 0.0,
                border_trees: false,
                ..Default::default()
            },
            ground: vec![2],
            road: vec![2],
            floor: vec![2],
            roof: Vec::new(),
            road_width: 2,
            palette: Vec::new(),
            windows: 2,
            furniture: 6,
            lights: false,
            floor_stats: None,
            roof_stats: None,
        };
        let mut village = fo_core::Map::new_empty("walltest", 1);
        let report = fo_core::village::generate(&mut village, &game, &model, &options);
        assert!(report.buildings >= 6, "{name}: {report:?}");
        assert_eq!(report.doors_unlinked, 0, "{name}: {report:?}");
        assert_eq!(report.unreachable, 0, "{name}: {report:?}");
        assert_eq!(report.missing, 0, "{name}: {report:?}");
        let bytes = fo_core::write_map(&village).unwrap();
        assert_eq!(read_map(&bytes, &game).unwrap().objects, village.objects);
    }
    let fence = sets
        .iter()
        .find(|s| s.label == "fence03")
        .expect("grillage de référence");
    let model = stats.model(&|f| fence.members.contains(&f)).strong_only();
    let opts = DecorOptions {
        front: true,
        ..Default::default()
    };
    let base = plan_room_with(&model, 20_050, 8, 8, &opts);
    let top: HashSet<_> = base
        .walls
        .iter()
        .filter(|p| p.0 / 200 == 100)
        .map(|p| p.0 % 200)
        .collect();
    assert!(
        (50..=58).all(|c| top.contains(&c)),
        "le grillage ne doit pas sauter un panneau sur deux"
    );
    for anchor in [20_051, 20_250, 20_251] {
        let plan = plan_room_with(&model, anchor, 8, 8, &opts);
        assert_eq!(
            relative_walls(&plan, anchor),
            relative_walls(&base, 20_050),
            "grillage sur une colonne impaire"
        );
    }
}
