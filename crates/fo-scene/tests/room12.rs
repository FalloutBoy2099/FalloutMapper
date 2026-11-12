//! Contrôle des pixels du cas 12×12 : les murs d'Abri ne doivent pas devenir des piliers espacés.
use fo_core::room::{DecorOptions, WallStats, plan_room_with};
use fo_core::{ArtLibrary, GameData, Map, geom, read_map};
use std::sync::Arc;

#[test]
fn vault_room_12_by_12_has_four_continuous_walls() {
    let dir =
        std::env::var("FO2_GAME_DIR").unwrap_or_else(|_| r"C:\Fallout2\F2_RPU_2.4.34_old".into());
    if !std::path::Path::new(&dir).is_dir() {
        return;
    }
    let game = Arc::new(GameData::open(dir).unwrap());
    let art = ArtLibrary::new(game.clone());
    let mut stats = WallStats::default();
    for entry in game.list_maps() {
        stats.add_map(&read_map(&game.read_map_bytes(&entry.file).unwrap(), &*game).unwrap());
    }
    let names = game.list("art/walls/walls.lst");
    let fid = |name: &str| {
        0x0300_0000
            | names
                .iter()
                .position(|n| n.trim().eq_ignore_ascii_case(name))
                .unwrap() as u32
    };
    let sets = stats.sets(&names);
    let set = sets
        .iter()
        .find(|s| s.members.contains(&fid("vb4000.frm")))
        .unwrap();
    for name in ["va1000.frm", "vcn1000.frm", "vd1000.frm", "vt1000.frm"] {
        assert!(
            set.members.contains(&fid(name)),
            "{name} est nécessaire à la famille complète"
        );
    }
    let model = stats.model(&|f| set.members.contains(&f)).strong_only();
    for anchor in [20_050, 20_051, 20_250, 20_251] {
        let plan = plan_room_with(
            &model,
            anchor,
            12,
            12,
            &DecorOptions {
                front: true,
                ..Default::default()
            },
        );
        assert!(
            !plan.short,
            "la pièce 12×12 doit être complètement raccordée"
        );
        let mut map = Map::new_empty("room12", 1);
        let mut counts = Default::default();
        fo_core::build::place_plan(&mut map, &game, &model, 0, &plan, &mut counts);
        assert_eq!(counts.missing, 0);
        let scene = fo_scene::build_scene(&map, 0, &art);
        let corners = [20_050, 20_062, 22_462, 22_450]
            .map(|h| geom::hex_translate(h, 20_050, anchor).unwrap());
        for side in 0..4 {
            let (ax, ay) = geom::hex_to_screen(corners[side]);
            let (bx, by) = geom::hex_to_screen(corners[(side + 1) % 4]);
            for step in 2..98 {
                let x = ax + (bx - ax) * step / 100;
                let y = ay + (by - ay) * step / 100 - 40;
                assert!(
                    scene
                        .items
                        .iter()
                        .any(|item| item.sprite.alpha_at(x - item.x, y - item.y) > 0),
                    "trou dans le mur {side} à ({x},{y}), ancrage {anchor}"
                );
            }
        }
    }
}
