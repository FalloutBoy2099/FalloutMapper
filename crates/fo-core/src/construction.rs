//! Reprise d'une construction existante sans perdre les objets et leurs scripts/inventaires.
use crate::map::{flags, word};
use crate::{Map, ObjectType, geom};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct Shell {
    pub elevation: usize,
    pub anchor: u32,
    pub cols: u32,
    pub rows: u32,
    pub wall_positions: HashSet<u32>,
}
impl Shell {
    pub fn owns_wall(&self, o: &crate::MapObject) -> bool {
        o.object_type() == Some(ObjectType::Wall)
            && o.tile() >= 0
            && self.wall_positions.contains(&(o.tile() as u32))
    }
    pub fn contains(&self, h: u32) -> bool {
        let ((c, r), (x, y)) = (geom::hex_col_row(self.anchor), geom::hex_col_row(h));
        x >= c && x <= c + self.cols && y >= r && y <= r + self.rows
    }
}

pub fn shell_at(map: &Map, e: usize, hex: u32) -> Option<Shell> {
    let walls: Vec<_> = map
        .objects
        .get(e)?
        .iter()
        .filter(|o| {
            o.object_type() == Some(ObjectType::Wall)
                && o.tile() >= 0
                && o.flags() & (flags::HIDDEN | flags::FLAT) == 0
        })
        .collect();
    let start = walls
        .iter()
        .min_by_key(|o| geom::hex_distance(o.tile() as u32, hex))?;
    if geom::hex_distance(start.tile() as u32, hex) > 64 {
        return None;
    }
    let mut positions = HashSet::from([start.tile() as u32]);
    let mut pending = vec![start.tile() as u32];
    while let Some(h) = pending.pop() {
        let (c, r) = geom::hex_col_row(h);
        for o in &walls {
            let (x, y) = geom::hex_col_row(o.tile() as u32);
            if ((x == c && y.abs_diff(r) <= 4) || (y == r && x.abs_diff(c) <= 4))
                && positions.insert(o.tile() as u32)
            {
                pending.push(o.tile() as u32);
            }
        }
    }
    if positions.len() < 4 {
        return None;
    }
    let c0 = positions.iter().map(|h| h % 200).min()?;
    let c1 = positions.iter().map(|h| h % 200).max()?;
    let r0 = positions.iter().map(|h| h / 200).min()?;
    let r1 = positions.iter().map(|h| h / 200).max()?;
    if c1 - c0 < 3 || r1 - r0 < 3 {
        return None;
    }
    let (x, y) = geom::hex_col_row(hex);
    if x < c0 || x > c1 || y < r0 || y > r1 {
        return None;
    }
    Some(Shell {
        elevation: e,
        anchor: r0 * 200 + c0,
        cols: c1 - c0,
        rows: r1 - r0,
        wall_positions: positions,
    })
}

/// Retire la coque et réorganise le contenu en gardant les instances originales intactes.
/// Travaille sur une copie ; aucune modification si le nouveau contour ne peut pas accueillir le contenu.
pub fn reshape(map: &Map, shell: &Shell, plan: &crate::room::RoomPlan) -> Result<Map, String> {
    if plan.short || plan.walls.is_empty() {
        return Err("Le nouveau contour est incomplet.".into());
    }
    let e = shell.elevation;
    let mut out = map.clone();
    let objects = &map.objects[e];
    let footprint: HashSet<u32> = plan.hexes.iter().copied().collect();
    if objects.iter().any(|o| {
        o.tile() >= 0
            && !shell.contains(o.tile() as u32)
            && footprint.contains(&(o.tile() as u32))
            && matches!(
                o.object_type(),
                Some(ObjectType::Wall | ObjectType::Scenery)
            )
            && o.flags() & (flags::HIDDEN | flags::NO_BLOCK) == 0
    }) {
        return Err(
            "Le nouveau bâtiment empiète sur une autre construction ou un décor bloquant.".into(),
        );
    }
    let remove_wall = |o: &crate::MapObject| {
        shell.owns_wall(o)
            || (o.object_type() == Some(ObjectType::Wall)
                && o.tile() >= 0
                && shell.contains(o.tile() as u32)
                && o.flags() & (flags::HIDDEN | flags::FLAT) != 0)
    };
    if objects.iter().any(|o| remove_wall(o) && o.script_id() >= 0) {
        return Err("Ce bâtiment contient un mur scripté : déplacez-le manuellement avant de remplacer la coque.".into());
    }
    out.objects[e].retain(|o| {
        !remove_wall(o)
            && !(o.object_type() == Some(ObjectType::Scenery)
                && o.subtype == crate::formats::scenery_kind::DOOR
                && o.tile() >= 0
                && shell.contains(o.tile() as u32))
    });
    let c0 = plan
        .hexes
        .iter()
        .map(|h| h % 200)
        .min()
        .ok_or("Empreinte vide")?;
    let c1 = plan.hexes.iter().map(|h| h % 200).max().unwrap();
    let r0 = plan.hexes.iter().map(|h| h / 200).min().unwrap();
    let r1 = plan.hexes.iter().map(|h| h / 200).max().unwrap();
    if c1 - c0 < 2 || r1 - r0 < 2 || plan.hexes.iter().any(|&h| h >= geom::HEX_COUNT) {
        return Err("Les dimensions du nouveau bâtiment sont invalides.".into());
    }
    let (oldc, oldr) = geom::hex_col_row(shell.anchor);
    let mut occupied: HashSet<u32> = out.objects[e]
        .iter()
        .filter(|o| o.tile() >= 0 && !shell.contains(o.tile() as u32))
        .map(|o| o.tile() as u32)
        .collect();
    occupied.extend(plan.walls.iter().map(|p| p.0));
    occupied.extend(plan.doors.iter().map(|p| p.hex));
    let free: Vec<_> = plan
        .hexes
        .iter()
        .copied()
        .filter(|h| h % 200 > c0 && h % 200 < c1 && h / 200 > r0 && h / 200 < r1)
        .collect();
    for object in &mut out.objects[e] {
        if object.tile() < 0 || !shell.contains(object.tile() as u32) {
            continue;
        }
        let old = object.tile() as u32;
        let (c, r) = geom::hex_col_row(old);
        let x = (c0 + (c - oldc) * (c1 - c0) / shell.cols.max(1)).clamp(c0 + 1, c1 - 1);
        let y = (r0 + (r - oldr) * (r1 - r0) / shell.rows.max(1)).clamp(r0 + 1, r1 - 1);
        let desired = if c0 == oldc
            && r0 == oldr
            && old % 200 > c0
            && old % 200 < c1
            && old / 200 > r0
            && old / 200 < r1
        {
            old
        } else {
            y * 200 + x
        };
        let tile = free
            .iter()
            .copied()
            .filter(|h| !occupied.contains(h))
            .min_by_key(|&h| (geom::hex_distance(h, desired), h))
            .ok_or("Le bâtiment est trop petit pour conserver tout son contenu.")?;
        occupied.insert(tile);
        object.words[word::TILE] = tile as i32;
    }
    // Le sol libéré reprend la tuile extérieure la plus fréquente autour de la construction.
    let oldhex: Vec<_> = (oldr..=oldr + shell.rows)
        .flat_map(|r| (oldc..=oldc + shell.cols).map(move |c| r * 200 + c))
        .collect();
    let oldsq = crate::build::interior_squares(&oldhex);
    let newsq = crate::build::interior_squares(&plan.hexes);
    let mut ground = HashMap::new();
    for &sq in &oldsq {
        for n in [
            sq.checked_sub(1),
            sq.checked_sub(100),
            sq.checked_add(1),
            sq.checked_add(100),
        ]
        .into_iter()
        .flatten()
        .filter(|&n| n < 10000 && !oldsq.contains(&n))
        {
            if let Some(t) = map.tile(e, n) {
                *ground.entry(t.floor).or_insert(0usize) += 1;
            }
        }
    }
    let ground = ground
        .into_iter()
        .max_by_key(|&(id, n)| (n, std::cmp::Reverse(id)))
        .map_or(crate::map::EMPTY_TILE, |p| p.0);
    for sq in oldsq {
        if !newsq.contains(&sq) {
            out.set_tile(e, sq, crate::map::TileLayer::Floor, ground);
            out.set_tile(e, sq, crate::map::TileLayer::Roof, crate::map::EMPTY_TILE);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn object(kind: u32, c: u32, r: u32) -> crate::MapObject {
        let mut o = crate::MapObject {
            words: [0; crate::map::OBJECT_WORDS],
            extra: vec![123, 456],
            inventory: Vec::new(),
            subtype: -1,
        };
        o.words[word::PID] = (kind << 24 | 1) as i32;
        o.words[word::FID] = (kind << 24 | 1) as i32;
        o.words[word::TILE] = (r * 200 + c) as i32;
        o.words[word::SCRIPT_ID] = -1;
        o
    }
    fn fixture() -> Map {
        let mut map = Map::new_empty("shell", 1);
        for i in 0..=12 {
            for (c, r) in [(40 + i, 40), (40 + i, 52), (40, 40 + i), (52, 40 + i)] {
                map.objects[0].push(object(3, c, r));
            }
        }
        let mut furniture = object(2, 50, 50);
        furniture.words[word::SCRIPT_ID] = 42;
        furniture.words[word::LIGHT_DISTANCE] = 5;
        map.objects[0].push(furniture);
        map.objects[0].push(object(2, 80, 80));
        map
    }
    fn smaller() -> crate::room::RoomPlan {
        let hexes = (40..=48)
            .flat_map(|r| (40..=48).map(move |c| r * 200 + c))
            .collect();
        let walls = (40..=48)
            .flat_map(|i| {
                [
                    (40 * 200 + i, 0x3000001),
                    (48 * 200 + i, 0x3000001),
                    (i * 200 + 40, 0x3000001),
                    (i * 200 + 48, 0x3000001),
                ]
            })
            .collect();
        crate::room::RoomPlan {
            walls,
            hexes,
            ..Default::default()
        }
    }
    #[test]
    fn resizing_preserves_furniture_data_and_unrelated_objects_even_with_duplicate_ids() {
        let map = fixture();
        let shell = shell_at(&map, 0, 50 * 200 + 50).unwrap();
        let resized = reshape(&map, &shell, &smaller()).unwrap();
        let furniture = resized.objects[0]
            .iter()
            .find(|o| o.script_id() == 42)
            .unwrap();
        assert_eq!(furniture.extra, vec![123, 456]);
        assert_eq!(furniture.words[word::LIGHT_DISTANCE], 5);
        assert!((41..48).contains(&(furniture.tile() % 200)));
        assert!((41..48).contains(&(furniture.tile() / 200)));
        assert!(resized.objects[0].iter().any(|o| o.tile() == 80 * 200 + 80));
        assert_eq!(
            map.objects[0]
                .iter()
                .find(|o| o.script_id() == 42)
                .unwrap()
                .tile(),
            50 * 200 + 50
        );
    }
    #[test]
    fn invalid_or_scripted_structures_are_not_modified() {
        let mut map = fixture();
        let shell = shell_at(&map, 0, 40 * 200 + 40).unwrap();
        map.objects[0][0].words[word::SCRIPT_ID] = 20;
        assert!(reshape(&map, &shell, &smaller()).is_err());
        assert_eq!(map.objects[0][0].script_id(), 20);
        assert!(shell_at(&map, 0, 10 * 200 + 10).is_none());
    }

    #[test]
    fn enlarging_does_not_overwrite_a_neighboring_building() {
        let mut map = fixture();
        let shell = shell_at(&map, 0, 40 * 200 + 40).unwrap();
        map.objects[0].push(object(3, 57, 57));
        let mut plan = smaller();
        plan.hexes = (40..=60)
            .flat_map(|r| (40..=60).map(move |c| r * 200 + c))
            .collect();
        assert!(reshape(&map, &shell, &plan).is_err());
        assert!(map.objects[0].iter().any(|o| o.tile() == 57 * 200 + 57));
    }
}
