//! Génération d'une carte extérieure complète à partir du plan de `procgen` : sol, bâtiments (murs d'un style appris des cartes du jeu,
//! portes, fenêtres, mobilier, lumières, toit), routes qui relient chaque porte au réseau, sorties vers la carte du monde, bloqueurs de
//! défilement, point de départ et décor semé (arbres, rochers… appris d'une carte du jeu choisie comme modèle d'ambiance).

use crate::MapObject;
use crate::build::{Counts, fill_tiles, floor_squares, place_plan};
use crate::formats::{ObjectType, scenery_kind};
use crate::game::GameData;
use crate::geom;
use crate::map::exit::{self, SCROLL_BLOCKER_PID, WORLD_MAP_EXIT};
use crate::map::{ExitDestination, Map, TileLayer, flags, word};
use crate::procgen::{Params, Rng, Side, plan_sites, route, scatter};
use crate::room::{DecorOptions, StyleModel, plan_building};
use crate::tile_stats::TileStats;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Un décor du semis : prototype, image, orientation, poids de tirage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteItem {
    pub pid: u32,
    pub fid: u32,
    pub orient: u8,
    pub weight: u32,
}

/// Les décors les plus fréquents d'une carte du jeu (sans portes, escaliers, lumières, objets masqués ni scriptés) : l'ambiance
/// d'un village, d'un désert ou d'une forêt se lit dans ce qu'on y sème.
pub fn learn_palette(map: &Map, game: &GameData, max_items: usize) -> Vec<PaletteItem> {
    let mut counts: HashMap<(u32, u32, u8), u32> = HashMap::new();
    for list in map.objects.iter() {
        for o in list {
            if o.object_type() != Some(ObjectType::Scenery)
                || o.flags() & flags::HIDDEN != 0
                || o.script_id() != -1
                || o.words[word::LIGHT_DISTANCE] > 0
            {
                continue;
            }
            if game.proto_subtype(o.pid()) != Some(scenery_kind::GENERIC) {
                continue;
            }
            *counts
                .entry((o.pid(), o.fid(), o.orientation().max(0) as u8))
                .or_default() += 1;
        }
    }
    let mut items: Vec<PaletteItem> = counts
        .into_iter()
        .map(|((pid, fid, orient), weight)| PaletteItem {
            pid,
            fid,
            orient,
            weight,
        })
        .collect();
    items.sort_by(|a, b| {
        b.weight
            .cmp(&a.weight)
            .then(a.pid.cmp(&b.pid))
            .then(a.fid.cmp(&b.fid))
            .then(a.orient.cmp(&b.orient))
    });
    items.truncate(max_items);
    items
}

pub fn pick_weighted<'a>(items: &'a [PaletteItem], rng: &mut Rng) -> Option<&'a PaletteItem> {
    let total: u64 = items.iter().map(|i| u64::from(i.weight.max(1))).sum();
    if total == 0 {
        return None;
    }
    let mut roll = rng.roll() % total;
    items.iter().find(|i| {
        let w = u64::from(i.weight.max(1));
        let hit = roll < w;
        roll = roll.saturating_sub(w);
        hit
    })
}

#[derive(Debug, Clone)]
pub struct GenOptions {
    pub params: Params,
    /// Familles de tuiles : sol de la carte, route, sol des bâtiments, toit (vide : pas de toit).
    pub ground: Vec<u16>,
    pub road: Vec<u16>,
    pub floor: Vec<u16>,
    pub roof: Vec<u16>,
    /// Largeur de la route en cases carrées (1 à 3).
    pub road_width: u32,
    pub palette: Vec<PaletteItem>,
    pub windows: u32,
    pub furniture: u32,
    pub lights: bool,
    pub floor_stats: Option<Arc<TileStats>>,
    pub roof_stats: Option<Arc<TileStats>>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GenReport {
    pub buildings: usize,
    pub doors_linked: usize,
    pub doors_unlinked: usize,
    pub exits: usize,
    pub blockers: usize,
    pub decor: usize,
    pub missing: usize,
    pub wall_pieces: usize,
    pub furniture: usize,
    pub lights: usize,
    /// Portes ou sorties qu'on ne peut pas atteindre à pied depuis le départ (doit rester nul).
    pub unreachable: usize,
}

/// Cases carrées voisines : élargit un ensemble de cases de `grow` rangs de chaque côté.
fn grow_squares(squares: &[usize], grow: u32) -> Vec<usize> {
    if grow == 0 {
        return squares.to_vec();
    }
    let mut set: HashSet<usize> = HashSet::new();
    for &s in squares {
        let (r, c) = ((s / 100) as i64, (s % 100) as i64);
        let g = grow as i64;
        for dr in -g..=g {
            for dc in -g..=g {
                let (nr, nc) = (r + dr, c + dc);
                if (0..100).contains(&nr) && (0..100).contains(&nc) {
                    set.insert((nr * 100 + nc) as usize);
                }
            }
        }
    }
    let mut v: Vec<usize> = set.into_iter().collect();
    v.sort_unstable();
    v
}

/// Voisinage élargi : les hexagones à au plus `radius` pas d'un des hexagones donnés.
fn dilate(hexes: &HashSet<u32>, radius: u32) -> HashSet<u32> {
    let mut out = hexes.clone();
    let mut frontier: Vec<u32> = hexes.iter().copied().collect();
    for _ in 0..radius {
        let mut next = Vec::new();
        for h in frontier {
            for n in geom::hex_neighbors(h) {
                if out.insert(n) {
                    next.push(n);
                }
            }
        }
        frontier = next;
    }
    out
}

/// Pose un décor du semis sur l'élévation `elevation` ; `false` si son prototype est introuvable.
pub fn place_scenery(
    m: &mut Map,
    game: &GameData,
    item: &PaletteItem,
    hex: u32,
    elevation: usize,
) -> bool {
    let id = m.max_object_id() + 1;
    let Some(mut obj) = game
        .proto_info(item.pid)
        .and_then(|info| MapObject::from_proto(&info, hex, id))
    else {
        return false;
    };
    obj.words[word::FID] = item.fid as i32;
    obj.words[word::ORIENTATION] = i32::from(item.orient);
    m.add_object(elevation, obj);
    true
}

fn add_scenery(m: &mut Map, game: &GameData, item: &PaletteItem, hex: u32) -> bool {
    let id = m.max_object_id() + 1;
    let Some(mut obj) = game
        .proto_info(item.pid)
        .and_then(|info| MapObject::from_proto(&info, hex, id))
    else {
        return false;
    };
    obj.words[word::FID] = item.fid as i32;
    obj.words[word::ORIENTATION] = i32::from(item.orient);
    m.add_object(0, obj);
    true
}

/// Comble les trous d'une route : une case libre entourée de route sur au moins trois côtés en fait partie (deux passes), hors `except`.
fn close_gaps(squares: &[usize], except: &HashSet<usize>) -> Vec<usize> {
    let mut set: HashSet<usize> = squares.iter().copied().collect();
    for _ in 0..2 {
        let mut add = Vec::new();
        let candidates: HashSet<usize> = set
            .iter()
            .flat_map(|&s| [s + 1, s.wrapping_sub(1), s + 100, s.wrapping_sub(100)])
            .filter(|&c| c < 10_000 && !set.contains(&c) && !except.contains(&c))
            .collect();
        for c in candidates {
            let n = [c + 1, c.wrapping_sub(1), c + 100, c.wrapping_sub(100)]
                .iter()
                .filter(|s| set.contains(s))
                .count();
            if n >= 3 {
                add.push(c);
            }
        }
        if add.is_empty() {
            break;
        }
        set.extend(add);
    }
    let mut v: Vec<usize> = set.into_iter().collect();
    v.sort_unstable();
    v
}

/// Remplit `map` (neuve, une élévation) d'un village complet. La carte reste cohérente : bâtiments sans chevauchement, chaque porte
/// reliée à la route, sorties et départ sur la route, décor hors des routes et des portes.
pub fn generate(map: &mut Map, game: &GameData, model: &StyleModel, o: &GenOptions) -> GenReport {
    let mut report = GenReport::default();
    let params = &o.params;
    let sites = plan_sites(params);
    let mut rng = Rng::new(params.seed ^ 0xC0FF_EE11);
    let all_squares: Vec<usize> = (0..10_000).collect();
    fill_tiles(
        map,
        0,
        TileLayer::Floor,
        &all_squares,
        &o.ground,
        o.floor_stats.as_deref(),
        params.seed,
    );

    // Bâtiments.
    struct Built {
        footprint: HashSet<u32>,
        door: Option<u32>,
    }
    let mut built: Vec<Built> = Vec::new();
    let mut counts = Counts::default();
    for (i, b) in sites.buildings.iter().enumerate() {
        let seed = rng.roll() | 1;
        let opts = DecorOptions {
            door: true,
            lights: o.lights,
            windows: o.windows,
            furniture: o.furniture,
            seed,
            vary_sizes: false,
            ..DecorOptions::default()
        };
        let mut plan = plan_building(model, b.corner, b.nx, b.ny, b.rows, b.cols, &opts);
        if plan.walls.is_empty() {
            continue;
        }
        // Un style sans porte connue : l'entrée est un passage ouvert dans le mur de devant, au milieu de la façade.
        let mut opening = None;
        if plan.doors.is_empty() {
            let r_max = plan
                .hexes
                .iter()
                .map(|&h| geom::hex_col_row(h).1)
                .max()
                .unwrap_or(0);
            let mut front: Vec<usize> = (0..plan.walls.len())
                .filter(|&k| geom::hex_col_row(plan.walls[k].0).1 == r_max)
                .collect();
            front.sort_by_key(|&k| geom::hex_col_row(plan.walls[k].0).0);
            if let Some(&k) = front.get(front.len() / 2) {
                opening = Some(plan.walls.remove(k).0);
            }
        }
        place_plan(map, game, model, 0, &plan, &mut counts);
        let squares = floor_squares(&plan.hexes);
        fill_tiles(
            map,
            0,
            TileLayer::Floor,
            &squares,
            &o.floor,
            o.floor_stats.as_deref(),
            seed ^ 0x51,
        );
        fill_tiles(
            map,
            0,
            TileLayer::Roof,
            &squares,
            &o.roof,
            o.roof_stats.as_deref(),
            seed ^ 0x9E37 ^ i as u64,
        );
        let footprint: HashSet<u32> = plan
            .hexes
            .iter()
            .copied()
            .chain(plan.walls.iter().map(|w| w.0))
            .collect();
        // La porte d'entrée : la plus proche du coin avant du bâtiment.
        let door = opening.or_else(|| {
            plan.doors.iter().map(|d| d.hex).max_by_key(|&h| {
                let (c, r) = geom::hex_col_row(h);
                c + r
            })
        });
        built.push(Built { footprint, door });
    }
    report.buildings = built.len();
    report.wall_pieces = counts.placed;
    report.furniture = counts.furniture;
    report.lights = counts.lights;
    report.missing = counts.missing;

    // Routes : le réseau principal, puis un chemin de chaque porte vers le réseau.
    let mut roads: HashSet<u32> = sites.roads.iter().copied().collect();
    let all_footprints: HashSet<u32> = built
        .iter()
        .flat_map(|b| b.footprint.iter().copied())
        .collect();
    for b in &built {
        let Some(door) = b.door else {
            report.doors_unlinked += 1;
            continue;
        };
        let starts: Vec<u32> = geom::hex_neighbors(door)
            .into_iter()
            .filter(|n| !all_footprints.contains(n))
            .collect();
        let best = starts
            .iter()
            .filter_map(|&s| route(s, &roads, &all_footprints, &sites.area))
            .min_by_key(Vec::len);
        match best {
            Some(path) => {
                roads.extend(path);
                roads.insert(door);
                report.doors_linked += 1;
            }
            None => report.doors_unlinked += 1,
        }
    }
    let building_squares: HashSet<usize> =
        floor_squares(&all_footprints.iter().copied().collect::<Vec<_>>())
            .into_iter()
            .collect();
    let road_hexes: Vec<u32> = roads
        .iter()
        .copied()
        .filter(|h| !all_footprints.contains(h))
        .collect();
    let road_squares: Vec<usize> =
        grow_squares(&floor_squares(&road_hexes), o.road_width.clamp(1, 3) - 1)
            .into_iter()
            .filter(|s| !building_squares.contains(s))
            .collect();
    let road_squares = close_gaps(&road_squares, &building_squares);
    fill_tiles(
        map,
        0,
        TileLayer::Floor,
        &road_squares,
        &o.road,
        o.floor_stats.as_deref(),
        params.seed ^ 0xA5,
    );

    // Sorties vers la carte du monde et bloqueurs de défilement.
    for e in &sites.exits {
        let (a, z) = (
            geom::hex_to_screen(e.hexes[0]),
            geom::hex_to_screen(e.hexes[e.hexes.len() - 1]),
        );
        let flipped = matches!(e.side, Side::East | Side::South);
        let dir = exit::direction_for_segment(z.0 - a.0, z.1 - a.1, flipped);
        for &h in &e.hexes {
            let id = map.max_object_id() + 1;
            map.add_object(
                0,
                MapObject::exit_grid(
                    dir,
                    ExitDestination {
                        map: WORLD_MAP_EXIT,
                        tile: -1,
                        elevation: 0,
                        orientation: 0,
                    },
                    h,
                    id,
                ),
            );
            report.exits += 1;
        }
    }
    if let Some(info) = game.proto_info(SCROLL_BLOCKER_PID) {
        for &h in &sites.blockers {
            let id = map.max_object_id() + 1;
            if let Some(obj) = MapObject::from_proto(&info, h, id) {
                map.add_object(0, obj);
                report.blockers += 1;
            }
        }
    }
    map.set_player_start(sites.start, 0);

    // Décor : semé hors des bâtiments, des routes (et d'un hexagone autour) et des sorties.
    if !o.palette.is_empty() {
        let mut keep_clear: HashSet<u32> = dilate(&all_footprints, 2);
        keep_clear.extend(dilate(&roads, 1));
        keep_clear.extend(dilate(
            &sites
                .exits
                .iter()
                .flat_map(|e| e.hexes.iter().copied())
                .collect(),
            4,
        ));
        keep_clear.extend(dilate(&HashSet::from([sites.start]), 3));
        let area_hexes = (sites.area.c1 - sites.area.c0 + 1) as usize
            * (sites.area.r1 - sites.area.r0 + 1) as usize;
        let veg_count = (params.vegetation.clamp(0.0, 1.0) * area_hexes as f32 / 22.0) as usize;
        let mut chosen = scatter(
            &sites.area,
            &keep_clear,
            veg_count,
            3,
            params.seed ^ 0x77,
            false,
        );
        if params.border_trees {
            let taken: HashSet<u32> = keep_clear
                .iter()
                .copied()
                .chain(chosen.iter().copied())
                .collect();
            let band =
                ((sites.area.c1 - sites.area.c0 + sites.area.r1 - sites.area.r0) as usize).max(10);
            chosen.extend(
                scatter(&sites.area, &taken, band, 2, params.seed ^ 0x99, true)
                    .into_iter()
                    .filter(|h| {
                        let (c, r) = geom::hex_col_row(*h);
                        c.min(r).min(sites.area.c1 - c).min(sites.area.r1 - r) < 9
                    }),
            );
        }
        // Menus objets : autour des routes et des bâtiments, jamais dessus.
        let near: HashSet<u32> = dilate(
            &roads.iter().chain(all_footprints.iter()).copied().collect(),
            6,
        );
        let taken: HashSet<u32> = keep_clear
            .iter()
            .copied()
            .chain(chosen.iter().copied())
            .collect();
        let prop_count = (params.props.clamp(0.0, 1.0) * area_hexes as f32 / 60.0) as usize;
        let outside_near: HashSet<u32> = (sites.area.r0..=sites.area.r1)
            .flat_map(|r| {
                (sites.area.c0..=sites.area.c1).map(move |c| r * geom::HEX_GRID_WIDTH + c)
            })
            .filter(|h| !near.contains(h))
            .collect();
        let props_blocked: HashSet<u32> = taken.union(&outside_near).copied().collect();
        chosen.extend(scatter(
            &sites.area,
            &props_blocked,
            prop_count,
            4,
            params.seed ^ 0x55,
            false,
        ));
        for h in chosen {
            if let Some(item) = pick_weighted(&o.palette, &mut rng).cloned()
                && add_scenery(map, game, &item, h)
            {
                report.decor += 1;
            }
        }
    }

    // Contrôle : portes et sorties atteignables depuis le départ.
    let walk = crate::reach::walk_map(map, 0, game);
    let seen = crate::reach::reachable(&walk.blocked, sites.start);
    for b in &built {
        if let Some(d) = b.door
            && !crate::reach::touched(&seen, d)
        {
            report.unreachable += 1;
        }
    }
    for e in &sites.exits {
        if !crate::reach::touched(&seen, e.inner) {
            report.unreachable += 1;
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_pick_follows_the_weights_and_handles_empty() {
        let items = vec![
            PaletteItem {
                pid: 1,
                fid: 1,
                orient: 0,
                weight: 9,
            },
            PaletteItem {
                pid: 2,
                fid: 2,
                orient: 0,
                weight: 1,
            },
        ];
        let mut rng = Rng::new(3);
        let heavy = (0..1000)
            .filter(|_| pick_weighted(&items, &mut rng).unwrap().pid == 1)
            .count();
        assert!((800..980).contains(&heavy), "{heavy}");
        assert!(pick_weighted(&[], &mut rng).is_none());
    }

    #[test]
    fn squares_grow_and_stay_on_the_grid() {
        assert_eq!(grow_squares(&[0], 1), vec![0, 1, 100, 101]);
        assert_eq!(grow_squares(&[505], 0), vec![505]);
        assert_eq!(grow_squares(&[9999], 1).len(), 4);
        assert_eq!(grow_squares(&[5050], 1).len(), 9);
    }

    #[test]
    fn dilation_adds_rings_of_neighbours() {
        let start = 100 * 200 + 100;
        let one = dilate(&HashSet::from([start]), 1);
        assert_eq!(one.len(), 7);
        assert_eq!(dilate(&HashSet::from([start]), 2).len(), 19);
        assert_eq!(dilate(&HashSet::new(), 3).len(), 0);
    }
}
