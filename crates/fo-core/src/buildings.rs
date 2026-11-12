//! Bâtiments des cartes du jeu : on repère les groupes de pièces de mur qui forment une construction (deux murs au moins sur le
//! pourtour d'un rectangle) et on en tire des motifs (murs, décors, sol, toit) qu'on peut reposer tels quels avec l'outil Tampon.
//!
//! La synthèse de murs ne sait pas reproduire tous les styles : l'art d'un mur n'est juste que là où les créateurs l'ont posé.
//! Copier une vraie construction garde ces murs exacts.

use crate::build::{floor_squares, interior_squares};
use crate::formats::{ObjectType, scenery_kind};
use crate::geom;
use crate::map::{EMPTY_TILE, Map, flags};
use crate::pattern::{CapturedObject, CapturedTile, Pattern, PatternVariant};
use std::collections::HashMap;

/// Une construction trouvée dans une carte.
#[derive(Debug, Clone)]
pub struct Building {
    pub map: String,
    pub elevation: usize,
    /// Nombre de pièces de mur.
    pub walls: usize,
    /// Dimensions du rectangle englobant, en colonnes et en lignes d'hexagones.
    pub cols: u32,
    pub rows: u32,
    pub pattern: Pattern,
}

/// Au plus ce nombre de constructions par carte (les plus fournies en murs).
const MAX_PER_MAP: usize = 6;

fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

/// Constructions d'une carte, de la plus fournie en murs à la moins fournie.
pub fn detect(map_name: &str, map: &Map) -> Vec<Building> {
    let mut found: Vec<Building> = Vec::new();
    for (e, objects) in map.objects.iter().enumerate() {
        // Pièces de mur visibles (les blocages invisibles n'en font pas partie).
        let walls: Vec<(usize, u32, u32)> = objects
            .iter()
            .enumerate()
            .filter(|(_, o)| {
                o.object_type() == Some(ObjectType::Wall)
                    && o.tile() >= 0
                    && !o.has_flag(flags::HIDDEN)
            })
            .map(|(i, o)| {
                let (c, r) = geom::hex_col_row(o.tile() as u32);
                (i, c, r)
            })
            .collect();
        if walls.len() < 10 {
            continue;
        }
        let at: HashMap<(u32, u32), Vec<usize>> =
            walls
                .iter()
                .enumerate()
                .fold(HashMap::new(), |mut m, (k, &(_, c, r))| {
                    m.entry((c, r)).or_default().push(k);
                    m
                });
        let mut parent: Vec<usize> = (0..walls.len()).collect();
        for (k, &(_, c, r)) in walls.iter().enumerate() {
            for dr in 0..=2u32 {
                for dc in 0..=2u32 {
                    if dc == 0 && dr == 0 {
                        // Plusieurs pièces sur le même hexagone : même construction.
                        for &j in &at[&(c, r)] {
                            let (a, b) = (find(&mut parent, k), find(&mut parent, j));
                            parent[b] = a;
                        }
                        continue;
                    }
                    for (nc, nr) in [(c + dc, r + dr), (c.wrapping_sub(dc), r + dr)] {
                        if let Some(list) = at.get(&(nc, nr)) {
                            for &j in list {
                                let (a, b) = (find(&mut parent, k), find(&mut parent, j));
                                parent[b] = a;
                            }
                        }
                    }
                }
            }
        }
        let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
        for k in 0..walls.len() {
            let root = find(&mut parent, k);
            groups.entry(root).or_default().push(k);
        }
        for members in groups.into_values() {
            if !(10..=120).contains(&members.len()) {
                continue;
            }
            let (mut cmin, mut cmax, mut rmin, mut rmax) = (u32::MAX, 0, u32::MAX, 0);
            for &k in &members {
                let (_, c, r) = walls[k];
                (cmin, cmax, rmin, rmax) = (cmin.min(c), cmax.max(c), rmin.min(r), rmax.max(r));
            }
            let (cols, rows) = (cmax - cmin + 1, rmax - rmin + 1);
            if !(6..=30).contains(&cols) || !(5..=30).contains(&rows) {
                continue;
            }
            // Un pourtour bien garni : au moins un tiers des hexagones du bord d'un rectangle porte un mur.
            let border = members.iter().filter(|&&k| {
                let (_, c, r) = walls[k];
                c == cmin || c == cmax || r == rmin || r == rmax
            });
            let border = border.count() as u32;
            if border * 3 < 2 * (cols + rows) {
                continue;
            }
            let anchor = rmin * geom::HEX_GRID_WIDTH + cmin;
            let inside = |hex: i32| -> bool {
                hex >= 0 && {
                    let (c, r) = geom::hex_col_row(hex as u32);
                    (cmin..=cmax).contains(&c) && (rmin..=rmax).contains(&r)
                }
            };
            let captured: Vec<CapturedObject> = objects
                .iter()
                .filter(|o| match o.object_type() {
                    Some(ObjectType::Wall) => !o.has_flag(flags::HIDDEN),
                    Some(ObjectType::Scenery) => {
                        matches!(o.subtype, scenery_kind::DOOR | scenery_kind::GENERIC)
                    }
                    _ => false,
                })
                .filter(|o| inside(o.tile()))
                .map(|o| CapturedObject {
                    hex: o.tile() as u32,
                    pro_pid: o.pid(),
                    frm_pid: o.fid(),
                    direction: o.orientation().max(0) as u32,
                    flags: o.flags(),
                })
                .collect();
            let hexes: Vec<u32> = (rmin..=rmax)
                .flat_map(|r| (cmin..=cmax).map(move |c| r * geom::HEX_GRID_WIDTH + c))
                .collect();
            let tiles = |roof: bool, squares: &[usize]| -> Vec<CapturedTile> {
                squares
                    .iter()
                    .filter_map(|&sq| {
                        map.tile(e, sq).map(|t| CapturedTile {
                            square: sq as u32,
                            tile_id: if roof { t.roof } else { t.floor },
                        })
                    })
                    .filter(|t| t.tile_id != EMPTY_TILE)
                    .collect()
            };
            let (floor, roof) = (
                tiles(false, &interior_squares(&hexes)),
                tiles(true, &floor_squares(&hexes)),
            );
            let variant = PatternVariant::capture("défaut", anchor, &captured, &floor, &roof);
            found.push(Building {
                map: map_name.to_string(),
                elevation: e,
                walls: members.len(),
                cols,
                rows,
                pattern: Pattern::new(String::new(), variant),
            });
        }
    }
    found.sort_by_key(|b| std::cmp::Reverse(b.walls));
    found.truncate(MAX_PER_MAP);
    for (i, b) in found.iter_mut().enumerate() {
        b.pattern.name = format!("{} {} : {}×{}", map_name, i + 1, b.cols, b.rows);
        b.pattern.tags = vec!["jeu".to_string()];
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_map_has_no_buildings() {
        assert!(detect("vide", &Map::new_empty("vide", 1)).is_empty());
    }
}
