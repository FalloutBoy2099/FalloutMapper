//! Sols proches des murs dans des espaces entourés sur les quatre côtés.
use crate::map::{EMPTY_TILE, flags};
use crate::{Map, ObjectType, geom};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Default, Clone)]
pub(crate) struct RoomFloors {
    counts: HashMap<(u32, u16), u32>,
    roofs: HashMap<(u32, u16), u32>,
}

impl RoomFloors {
    pub fn add_map(&mut self, map: &Map) {
        for (e, objects) in map.objects.iter().enumerate() {
            let mut columns: HashMap<i32, Vec<(f64, u32)>> = HashMap::new();
            let mut rows: HashMap<i32, Vec<(f64, u32)>> = HashMap::new();
            let mut candidates = HashSet::new();
            let mut positions = HashSet::new();
            for o in objects {
                if o.object_type() != Some(ObjectType::Wall)
                    || o.tile() < 0
                    || o.flags() & (flags::HIDDEN | flags::FLAT) != 0
                {
                    continue;
                }
                let h = o.tile() as u32;
                if h >= geom::HEX_COUNT {
                    continue;
                }
                let (c, r) = geom::hex_col_row(h);
                positions.insert((c as i32, r as i32));
                // Coordonnées affines : les colonnes impaires sont décalées d'une demi-ligne.
                let v2 = 2 * r as i32 - (c % 2) as i32;
                columns
                    .entry(c as i32)
                    .or_default()
                    .push((v2 as f64 / 2.0, o.fid()));
                rows.entry(v2).or_default().push((c as f64, o.fid()));
                let (x, y) = geom::hex_to_screen(h);
                if let Some(sq) = geom::screen_to_square(x, y) {
                    for dr in -3..=3 {
                        for dc in -3..=3 {
                            let (sc, sr) = (sq as i32 % 100 + dc, sq as i32 / 100 + dr);
                            if (0..100).contains(&sc) && (0..100).contains(&sr) {
                                candidates.insert((sr * 100 + sc) as usize);
                            }
                        }
                    }
                }
            }
            // Les quatre murs doivent appartenir au même bâtiment. Sans cela,
            // une rue entre plusieurs bâtiments passerait pour une pièce.
            let mut components = HashMap::new();
            for &start in &positions {
                if components.contains_key(&start) {
                    continue;
                }
                let component = components.len();
                components.insert(start, component);
                let mut pending = vec![start];
                while let Some((c, r)) = pending.pop() {
                    for step in 1..=3 {
                        for next in [(c - step, r), (c + step, r), (c, r - step), (c, r + step)] {
                            if positions.contains(&next) && !components.contains_key(&next) {
                                components.insert(next, component);
                                pending.push(next);
                            }
                        }
                    }
                }
            }
            for sq in candidates {
                let Some(tile) = map
                    .tile(e, sq)
                    .map(|t| t.floor)
                    .filter(|&t| t > 1 && t != EMPTY_TILE)
                else {
                    continue;
                };
                let (x, y) = geom::square_to_screen(sq as u32);
                let (dx, dy) = ((x + 40 - 4816) as f64, (y + 18 - 11) as f64);
                let (u, v) = (
                    (12.0 * dx - 16.0 * dy) / -384.0,
                    (-6.0 * dx - 24.0 * dy) / -384.0,
                );
                let mut hits = [None::<(f64, u32, usize)>; 4];
                let mut hit = |axis: usize, delta: f64, fid: u32, component: usize| {
                    let d = delta.abs();
                    if !(0.5..=32.0).contains(&d) {
                        return;
                    }
                    let side = axis * 2 + usize::from(delta > 0.0);
                    if hits[side].is_none_or(|old| (d, fid) < (old.0, old.1)) {
                        hits[side] = Some((d, fid, component));
                    }
                };
                for k in u.floor() as i32..=u.ceil() as i32 {
                    if (k as f64 - u).abs() <= 0.75 {
                        if let Some(walls) = columns.get(&k) {
                            for &(pos, fid) in walls {
                                let r = ((2.0 * pos) as i32 + k % 2) / 2;
                                hit(1, pos - v, fid, components[&(k, r)]);
                            }
                        }
                    }
                }
                for k in (2.0 * v - 1.5).ceil() as i32..=(2.0 * v + 1.5).floor() as i32 {
                    if let Some(walls) = rows.get(&k) {
                        for &(pos, fid) in walls {
                            let c = pos as i32;
                            hit(0, pos - u, fid, components[&(c, (k + c % 2) / 2)]);
                        }
                    }
                }
                if hits.iter().any(Option::is_none) {
                    continue;
                }
                if hits.iter().flatten().any(|h| h.2 != hits[0].unwrap().2) {
                    continue;
                }
                // Une tuile ne vote qu'une fois par FID, même près d'un angle.
                let nearby: HashSet<u32> = hits
                    .iter()
                    .flatten()
                    .filter(|(d, _, _)| (2.0..=6.0).contains(d))
                    .map(|(_, fid, _)| *fid)
                    .collect();
                for fid in nearby {
                    *self.counts.entry((fid, tile)).or_default() += 1;
                    if let Some(roof) = map
                        .tile(e, sq)
                        .map(|t| t.roof)
                        .filter(|&id| id > 1 && id != EMPTY_TILE)
                    {
                        *self.roofs.entry((fid, roof)).or_default() += 1;
                    }
                }
            }
        }
    }

    pub fn for_style(&self, contains: &dyn Fn(u32) -> bool) -> BTreeMap<u16, u32> {
        let mut counts = BTreeMap::new();
        for (&(fid, tile), &n) in &self.counts {
            if contains(fid) {
                *counts.entry(tile).or_default() += n;
            }
        }
        counts
    }

    pub fn roofs_for_style(&self, contains: &dyn Fn(u32) -> bool) -> BTreeMap<u16, u32> {
        let mut counts = BTreeMap::new();
        for (&(fid, tile), &n) in &self.roofs {
            if contains(fid) {
                *counts.entry(tile).or_default() += n;
            }
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{MapObject, OBJECT_WORDS, TileLayer, word};

    fn wall(c: u32, r: u32, fid: u32) -> MapObject {
        let mut o = MapObject {
            words: [0; OBJECT_WORDS],
            extra: Vec::new(),
            inventory: Vec::new(),
            subtype: -1,
        };
        o.words[word::PID] = 0x0300_0001;
        o.words[word::FID] = fid as i32;
        o.words[word::TILE] = (r * 200 + c) as i32;
        o
    }

    #[test]
    fn enclosed_floors_stay_associated_with_their_own_walls() {
        let mut map = Map::new_empty("floors", 1);
        for sq in 0..10_000 {
            map.set_tile(0, sq, TileLayer::Floor, 20);
        }
        for (c0, fid, floor) in [(40, 0x0300_0001, 21), (70, 0x0300_0002, 22)] {
            for i in 0..=12 {
                for (c, r) in [(c0 + i, 40), (c0 + i, 52), (c0, 40 + i), (c0 + 12, 40 + i)] {
                    map.objects[0].push(wall(c, r, fid));
                }
            }
            let hexes: Vec<_> = (40..=52)
                .flat_map(|r| (c0..=c0 + 12).map(move |c| r * 200 + c))
                .collect();
            for sq in crate::build::interior_squares(&hexes) {
                map.set_tile(0, sq, TileLayer::Floor, floor);
            }
        }
        let mut stats = RoomFloors::default();
        stats.add_map(&map);
        for (fid, floor) in [(0x0300_0001, 21), (0x0300_0002, 22)] {
            let observed = stats.for_style(&|f| f == fid);
            assert_eq!(
                observed.keys().copied().collect::<Vec<_>>(),
                vec![floor],
                "pas de terre extérieure ni de sol de l'autre pièce : {observed:?}"
            );
        }
    }

    #[test]
    fn an_outdoor_wall_does_not_teach_a_room_floor() {
        let mut map = Map::new_empty("outside", 1);
        for sq in 0..10_000 {
            map.set_tile(0, sq, TileLayer::Floor, 20);
        }
        for i in 0..=12 {
            map.objects[0].push(wall(40 + i, 40, 0x0300_0001));
        }
        let mut stats = RoomFloors::default();
        stats.add_map(&map);
        assert!(stats.for_style(&|_| true).is_empty());
    }

    #[test]
    fn separate_buildings_around_an_outdoor_space_are_not_a_room() {
        let mut map = Map::new_empty("street", 1);
        for sq in 0..10_000 {
            map.set_tile(0, sq, TileLayer::Floor, 20);
        }
        for i in 44..=48 {
            for (c, r) in [(i, 40), (i, 60), (40, i), (60, i)] {
                map.objects[0].push(wall(c, r, 0x0300_0001));
            }
        }
        let mut stats = RoomFloors::default();
        stats.add_map(&map);
        assert!(stats.for_style(&|_| true).is_empty());
    }
}
