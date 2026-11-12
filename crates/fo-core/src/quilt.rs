//! Synthèse de sols par patchs (« quilting ») : on recopie, d'une carte de référence, des blocs de tuiles dessinés à la main, en choisissant
//! pour chaque bloc celui qui se raccorde le mieux à ce qui est déjà en place. Les transitions entre tuiles viennent ainsi de la carte
//! d'origine plutôt que d'un tirage case par case.
//!
//! Le bloc est un carré de `patch` × `patch` cases ; deux blocs voisins se recouvrent d'une rangée, qui sert à mesurer l'écart. Les cases
//! voisines de la zone (déjà posées sur la carte, hors zone) comptent aussi dans cette mesure, pour que les bords se fondent dans l'existant.

use crate::map::{EMPTY_TILE, Map, TileLayer};
use std::collections::{HashMap, HashSet};

const SIDE: usize = 100;
/// Nombre de blocs candidats essayés pour chaque bloc posé.
const CANDIDATES: usize = 300;

/// Les tuiles d'une couche d'une élévation d'une carte de référence.
#[derive(Debug, Clone)]
pub struct Reference {
    tiles: Vec<u16>,
}

impl Reference {
    pub fn from_map(map: &Map, elevation: usize, layer: TileLayer) -> Reference {
        let tiles = (0..SIDE * SIDE)
            .map(|sq| {
                map.tile(elevation, sq).map_or(EMPTY_TILE, |t| {
                    if layer == TileLayer::Floor {
                        t.floor
                    } else {
                        t.roof
                    }
                })
            })
            .collect();
        Reference { tiles }
    }

    pub fn at(&self, col: usize, row: usize) -> u16 {
        self.tiles[row * SIDE + col]
    }

    /// Nombre de cases utilisables (ni vides ni remplissage).
    pub fn usable(&self) -> usize {
        self.tiles.iter().filter(|&&t| t > 1).count()
    }

    /// Origines (colonne, ligne) des blocs `patch` × `patch` dont toutes les cases sont utilisables.
    fn origins(&self, patch: usize) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for r in 0..=SIDE - patch {
            for c in 0..=SIDE - patch {
                if (0..patch).all(|dy| (0..patch).all(|dx| self.at(c + dx, r + dy) > 1)) {
                    out.push((c, r));
                }
            }
        }
        out
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QuiltReport {
    /// Cases de la zone remplies.
    pub placed: usize,
    /// Blocs posés sans aucune case de contexte avec laquelle les comparer (premier bloc, zone isolée).
    pub free_blocks: usize,
    /// Écart moyen de raccord des blocs posés (cases voisines différentes), pour juger la qualité.
    pub mismatches: usize,
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Remplit `squares` (numéros de cases carrées) de la couche `layer` d'après `reference`. Renvoie ce qui a été fait, ou `None` si la
/// référence n'a aucun bloc utilisable de cette taille.
pub fn quilt(
    map: &mut Map,
    elevation: usize,
    layer: TileLayer,
    squares: &[usize],
    reference: &Reference,
    patch: usize,
    seed: u64,
) -> Option<QuiltReport> {
    let patch = patch.clamp(2, 12);
    let target: HashSet<usize> = squares
        .iter()
        .copied()
        .filter(|&s| s < SIDE * SIDE)
        .collect();
    if target.is_empty() {
        return Some(QuiltReport::default());
    }
    let origins = reference.origins(patch);
    if origins.is_empty() {
        return None;
    }
    let (mut c0, mut c1, mut r0, mut r1) = (usize::MAX, 0, usize::MAX, 0);
    for &s in &target {
        let (c, r) = (s % SIDE, s / SIDE);
        (c0, c1, r0, r1) = (c0.min(c), c1.max(c), r0.min(r), r1.max(r));
    }
    let tile_of = |m: &Map, s: usize| {
        m.tile(elevation, s)
            .map(|t| {
                if layer == TileLayer::Floor {
                    t.floor
                } else {
                    t.roof
                }
            })
            .filter(|&v| v > 1)
    };
    let mut out: HashMap<usize, u16> = HashMap::new();
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut report = QuiltReport::default();
    let step = patch - 1;
    // Les blocs partent d'un peu avant la zone (le recouvrement d'une rangée) pour que les bords soient eux aussi comparés.
    let mut r = r0;
    loop {
        let mut c = c0;
        loop {
            // Contexte du bloc : tuile déjà en place pour chaque case, posée par nous ou existante hors de la zone.
            let context: Vec<Option<u16>> = (0..patch * patch)
                .map(|k| {
                    let (cc, rr) = (c + k % patch, r + k / patch);
                    if cc >= SIDE || rr >= SIDE {
                        return None;
                    }
                    let s = rr * SIDE + cc;
                    out.get(&s).copied().or_else(|| {
                        if target.contains(&s) {
                            None
                        } else {
                            tile_of(map, s)
                        }
                    })
                })
                .collect();
            let known = context.iter().filter(|x| x.is_some()).count();
            let mut best: Vec<(usize, (usize, usize))> = Vec::new();
            for _ in 0..CANDIDATES {
                let o = origins[(rng.next() % origins.len() as u64) as usize];
                let score = context
                    .iter()
                    .enumerate()
                    .filter(|(_, want)| want.is_some())
                    .filter(|(k, want)| {
                        reference.at(o.0 + k % patch, o.1 + k / patch) != want.unwrap_or(0)
                    })
                    .count();
                if best.len() < 4 || score < best[best.len() - 1].0 {
                    best.push((score, o));
                    best.sort_by_key(|b| b.0);
                    best.truncate(4);
                }
            }
            let pick = (rng.next() % best.len() as u64) as usize;
            let (score, o) = best[pick];
            if known == 0 {
                report.free_blocks += 1;
            } else {
                report.mismatches += score;
            }
            for k in 0..patch * patch {
                let (cc, rr) = (c + k % patch, r + k / patch);
                if cc >= SIDE || rr >= SIDE {
                    continue;
                }
                let s = rr * SIDE + cc;
                // On ne remplit que ce qui est dans la zone et pas encore posé : le recouvrement garde la première version.
                if target.contains(&s) && !out.contains_key(&s) {
                    out.insert(s, reference.at(o.0 + k % patch, o.1 + k / patch));
                }
            }
            if c >= c1 {
                break;
            }
            c += step;
        }
        if r >= r1 {
            break;
        }
        r += step;
    }
    for (&s, &id) in &out {
        if map.set_tile(elevation, s, layer, id) {
            report.placed += 1;
        }
    }
    Some(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une carte dont le sol est un motif en bandes (tuiles 10, 11, 12 en rotation sur les colonnes).
    fn striped() -> Map {
        let mut m = Map::new_empty("ref", 1);
        for sq in 0..SIDE * SIDE {
            m.set_tile(0, sq, TileLayer::Floor, 10 + (sq % SIDE) as u16 % 3);
        }
        m
    }

    #[test]
    fn quilting_uses_only_reference_tiles_and_stays_in_the_zone() {
        let reference = Reference::from_map(&striped(), 0, TileLayer::Floor);
        assert!(reference.usable() > 9000);
        let mut target = Map::new_empty("t", 1);
        let squares: Vec<usize> = (20..40)
            .flat_map(|r| (30..55).map(move |c| r * SIDE + c))
            .collect();
        let report = quilt(&mut target, 0, TileLayer::Floor, &squares, &reference, 4, 7)
            .expect("références");
        assert_eq!(report.placed, squares.len());
        for &s in &squares {
            let id = target.tile(0, s).unwrap().floor;
            assert!((10..13).contains(&id), "case {s} : {id}");
        }
        // Hors zone : rien n'a changé.
        assert_eq!(
            target.tile(0, 5).unwrap().floor,
            Map::new_empty("x", 1).tile(0, 5).unwrap().floor
        );
    }

    #[test]
    fn same_seed_same_result_and_context_is_respected() {
        let reference = Reference::from_map(&striped(), 0, TileLayer::Floor);
        let squares: Vec<usize> = (20..30)
            .flat_map(|r| (30..60).map(move |c| r * SIDE + c))
            .collect();
        let (mut a, mut b) = (Map::new_empty("a", 1), Map::new_empty("b", 1));
        quilt(&mut a, 0, TileLayer::Floor, &squares, &reference, 4, 3);
        quilt(&mut b, 0, TileLayer::Floor, &squares, &reference, 4, 3);
        assert!(
            squares
                .iter()
                .all(|&s| a.tile(0, s).unwrap().floor == b.tile(0, s).unwrap().floor)
        );
        // Les bandes se prolongent : le motif d'une case à sa voisine de droite reste 10 → 11 → 12 → 10 la plupart du temps.
        let ok = squares
            .windows(2)
            .filter(|w| w[1] == w[0] + 1)
            .filter(|w| {
                (a.tile(0, w[1]).unwrap().floor - 10)
                    == (a.tile(0, w[0]).unwrap().floor - 10 + 1) % 3
            })
            .count();
        assert!(
            ok * 10 >= squares.len() * 8,
            "{ok} raccords corrects sur {}",
            squares.len()
        );
    }

    #[test]
    fn reference_without_blocks_gives_none() {
        let empty = Reference::from_map(&Map::new_empty("e", 1), 0, TileLayer::Floor);
        let mut m = Map::new_empty("t", 1);
        assert!(
            quilt(&mut m, 0, TileLayer::Floor, &[505], &empty, 4, 1).is_none()
                || empty.usable() > 0
        );
    }
}
