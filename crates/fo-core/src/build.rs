//! Aides de construction partagées par l'outil « Pièce » et le générateur de cartes : tuiles d'une famille posées en suivant les voisines
//! observées, objets d'un plan de pièce, cases de sol couvertes par des hexagones.

use crate::MapObject;
use crate::geom;
use crate::map::{Map, TileLayer, word};
use crate::room::{RoomPlan, StyleModel};
use crate::tile_stats::TileStats;

/// Pose sur `squares` des tuiles de la famille `ids` : chacune suit sa voisine de gauche et celle du dessus quand les cartes
/// observées le permettent (et que la suggestion reste dans la famille), sinon une tuile de la famille au hasard.
pub fn fill_tiles(
    m: &mut Map,
    e: usize,
    layer: TileLayer,
    squares: &[usize],
    ids: &[u16],
    stats: Option<&TileStats>,
    seed: u64,
) {
    if ids.is_empty() {
        return;
    }
    let mut state = seed | 1;
    let mut roll = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut placed: std::collections::HashMap<usize, u16> = std::collections::HashMap::new();
    for &sq in squares {
        let left = (sq % 100 > 0)
            .then(|| placed.get(&(sq - 1)).copied())
            .flatten();
        let up = (sq >= 100)
            .then(|| placed.get(&(sq - 100)).copied())
            .flatten();
        let random = ids[(roll() % ids.len() as u64) as usize];
        let id = stats
            .and_then(|s| s.pick_from(left, up, roll(), ids))
            .unwrap_or(random);
        m.set_tile(e, sq, layer, id);
        placed.insert(sq, id);
    }
}

#[derive(Default, Debug)]
pub struct Counts {
    pub placed: usize,
    pub missing: usize,
    pub doors: usize,
    pub furniture: usize,
    pub lights: usize,
    pub stairs: usize,
    /// Pièces de mur posées à l'étage.
    pub upper: usize,
}

/// Pose les objets d'un plan (murs, portes, mobilier, lumières) sur une élévation.
pub fn place_plan(
    m: &mut Map,
    game: &crate::GameData,
    model: &StyleModel,
    e: usize,
    plan: &RoomPlan,
    counts: &mut Counts,
) {
    for &(hex, fid) in &plan.walls {
        let id = m.max_object_id() + 1;
        let Some(mut obj) = model
            .pid_of(fid)
            .and_then(|pid| game.proto_info(pid))
            .and_then(|info| MapObject::from_proto(&info, hex, id))
        else {
            counts.missing += 1;
            continue;
        };
        obj.words[word::FID] = fid as i32;
        m.add_object(e, obj);
        counts.placed += 1;
    }
    // Portes et mobilier : objets du décor, avec l'orientation observée.
    for (list, which) in [(&plan.doors, 0), (&plan.furniture, 1), (&plan.lights, 2)] {
        for p in list {
            let id = m.max_object_id() + 1;
            let Some(mut obj) = model
                .pid_of(p.fid)
                .and_then(|pid| game.proto_info(pid))
                .and_then(|info| MapObject::from_proto(&info, p.hex, id))
            else {
                counts.missing += 1;
                continue;
            };
            obj.words[word::FID] = p.fid as i32;
            obj.words[word::ORIENTATION] = p.orient as i32;
            obj.words[word::OFFSET_X] = p.offset.0;
            obj.words[word::OFFSET_Y] = p.offset.1;
            if let Some(flags) = p.flags {
                obj.words[word::FLAGS] = flags as i32;
            }
            if let Some(l) = model.light() {
                if l.fid == p.fid {
                    // Rayon et intensité observés dans les cartes (le prototype en prévoit parfois d'autres).
                    obj.words[word::LIGHT_DISTANCE] = l.distance;
                    obj.words[word::LIGHT_INTENSITY] = l.intensity;
                } else if obj.words[word::LIGHT_DISTANCE] > l.distance {
                    // Un meuble qui éclaire (lampe…) ne dépasse pas la portée habituelle d'une pièce.
                    obj.words[word::LIGHT_DISTANCE] = l.distance;
                }
            }
            m.add_object(e, obj);
            match which {
                0 => counts.doors += 1,
                1 => counts.furniture += 1,
                _ => counts.lights += 1,
            }
        }
    }
}

/// Cases de sol couvertes par les hexagones donnés (sans doublon).
pub fn floor_squares(hexes: &[u32]) -> Vec<usize> {
    let mut v: Vec<usize> = hexes
        .iter()
        .filter_map(|&h| {
            let (x, y) = geom::hex_to_screen(h);
            geom::screen_to_square(x, y)
        })
        .map(|s| s as usize)
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Cases de sol dont le losange tient dans le rectangle de hexagones donné : `floor_squares` prend toute case touchée par un
/// hexagone, et son losange (deux hexagones de large) déborde alors des murs. Retombe sur `floor_squares` si rien ne reste.
pub fn interior_squares(hexes: &[u32]) -> Vec<usize> {
    // Une case carrée recouvre les hexagones (2c, 2r) à (2c + 1, 2r + 1) : on prend les cases du rectangle englobant, sans passer par
    // la position à l'écran de chaque hexagone. Avec un coin aligné sur ce quadrillage (colonne et ligne paires), le sol suit exactement
    // les murs, et la même pièce se pose de la même façon partout.
    let (mut c0, mut c1, mut r0, mut r1) = (u32::MAX, 0, u32::MAX, 0);
    for &h in hexes {
        let (c, r) = geom::hex_col_row(h);
        (c0, c1, r0, r1) = (c0.min(c), c1.max(c), r0.min(r), r1.max(r));
    }
    if hexes.is_empty() {
        return Vec::new();
    }
    let side = geom::SQUARE_GRID_WIDTH;
    // Seules les cases entièrement dans le rectangle : une rangée d'hexagones qui ne remplit que la moitié d'une case (la rangée des murs
    // de devant) resterait sans sol, sous le mur, au lieu de faire dépasser le sol dehors.
    let squares = |rows: std::ops::RangeInclusive<u32>, cols: std::ops::RangeInclusive<u32>| -> Vec<usize> {
        rows.flat_map(|sr| cols.clone().map(move |sc| (sr, sc))).filter(|&(sr, sc)| sc < side && sr < side).map(|(sr, sc)| (sr * side + sc) as usize).collect()
    };
    let full = squares(r0.div_ceil(2)..=(r1.saturating_sub(1)) / 2, c0.div_ceil(2)..=(c1.saturating_sub(1)) / 2);
    if r1 >= 1 && c1 >= 1 && !full.is_empty() && r0.div_ceil(2) <= (r1 - 1) / 2 && c0.div_ceil(2) <= (c1 - 1) / 2 {
        return full;
    }
    squares(r0 / 2..=r1 / 2, c0 / 2..=c1 / 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interior_squares_follow_the_even_aligned_rectangle() {
        // Un coin aligné (colonne et ligne paires) : 8 × 9 hexagones → 4 × 4 cases (la neuvième rangée, celle du mur de devant, est sous le mur).
        let corner = 100 * 200 + 50;
        let hexes: Vec<u32> = (0..9).flat_map(|j| (0..8).map(move |i| corner + j * 200 + i)).collect();
        let inner = interior_squares(&hexes);
        assert_eq!(inner.len(), 4 * 4);
        assert!(inner.contains(&(50 * 100 + 25)) && inner.contains(&(53 * 100 + 28)));
        assert!(interior_squares(&[]).is_empty());
    }

    #[test]
    fn floor_squares_are_unique_and_cover_the_area() {
        let corner = 100 * 200 + 50;
        let hexes: Vec<u32> = (0..6)
            .flat_map(|j| (0..6).map(move |i| corner + j * 200 + i))
            .collect();
        let sq = floor_squares(&hexes);
        assert!(!sq.is_empty() && sq.len() <= hexes.len());
        assert!(sq.windows(2).all(|w| w[0] < w[1]));
    }
}
