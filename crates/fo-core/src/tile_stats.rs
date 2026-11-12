//! Statistiques de voisinage des tuiles : sur un ensemble de cartes, combien de fois une tuile en touche une autre.
//! Elles servent à suggérer les tuiles qui vont ensemble, à remplir une zone de façon plausible et à regrouper les tuiles
//! par famille (préfixe du nom dans `tiles.lst`).

use crate::map::{EMPTY_TILE, Map, TileLayer};
use std::collections::{BTreeMap, HashMap};

const SIDE: usize = 100;

/// Voisinages observés : `east[(a, b)]` = nombre de fois où `b` est à droite de `a` (colonne suivante),
/// `south[(a, b)]` = `b` est sous `a` (ligne suivante).
#[derive(Debug, Default, Clone)]
pub struct TileStats {
    east: HashMap<(u16, u16), u32>,
    south: HashMap<(u16, u16), u32>,
    total: HashMap<u16, u32>,
}

impl TileStats {
    /// Ajoute les voisinages de toutes les élévations de `map`, pour la couche `layer`. Les cases vides sont ignorées.
    pub fn add_map(&mut self, map: &Map, layer: TileLayer) {
        for e in 0..crate::map::ELEVATION_COUNT {
            let Some(tiles) = map.tiles[e].as_ref() else {
                continue;
            };
            let id = |sq: usize| -> Option<u16> {
                let t = tiles.get(sq)?;
                let v = if layer == TileLayer::Floor {
                    t.floor
                } else {
                    t.roof
                };
                (v != EMPTY_TILE && v > 1).then_some(v)
            };
            for sq in 0..SIDE * SIDE {
                let Some(a) = id(sq) else { continue };
                *self.total.entry(a).or_default() += 1;
                if sq % SIDE + 1 < SIDE
                    && let Some(b) = id(sq + 1)
                {
                    *self.east.entry((a, b)).or_default() += 1;
                }
                if sq / SIDE + 1 < SIDE
                    && let Some(b) = id(sq + SIDE)
                {
                    *self.south.entry((a, b)).or_default() += 1;
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.total.is_empty()
    }

    /// Nombre de tuiles différentes vues.
    pub fn distinct(&self) -> usize {
        self.total.len()
    }

    /// Tuiles qui touchent `id` (n'importe quel côté), de la plus fréquente à la moins fréquente.
    pub fn neighbors_of(&self, id: u16) -> Vec<(u16, u32)> {
        let mut acc: BTreeMap<u16, u32> = BTreeMap::new();
        for (&(a, b), &n) in self.east.iter().chain(self.south.iter()) {
            if a == id {
                *acc.entry(b).or_default() += n;
            }
            if b == id {
                *acc.entry(a).or_default() += n;
            }
        }
        let mut v: Vec<(u16, u32)> = acc.into_iter().collect();
        v.sort_by(|x, y| y.1.cmp(&x.1).then(x.0.cmp(&y.0)));
        v
    }

    /// Tuile à poser sachant ses voisines de gauche et du dessus : tirage pondéré par les voisinages observés, avec `roll`
    /// (un nombre quelconque, par exemple issu d'un générateur pseudo-aléatoire) comme source de hasard.
    /// Sans voisine connue, on prend la tuile la plus fréquente.
    pub fn pick(&self, left: Option<u16>, up: Option<u16>, roll: u64) -> Option<u16> {
        self.pick_filtered(left, up, roll, &|_| true)
    }

    /// Même tirage, limité aux tuiles autorisées avant de calculer les poids.
    pub fn pick_from(
        &self,
        left: Option<u16>,
        up: Option<u16>,
        roll: u64,
        ids: &[u16],
    ) -> Option<u16> {
        self.pick_filtered(left, up, roll, &|id| ids.contains(&id))
    }

    fn pick_filtered(
        &self,
        left: Option<u16>,
        up: Option<u16>,
        roll: u64,
        allowed: &dyn Fn(u16) -> bool,
    ) -> Option<u16> {
        let mut weights: BTreeMap<u16, u64> = BTreeMap::new();
        if let Some(l) = left {
            for (&(a, b), &n) in &self.east {
                if a == l && allowed(b) {
                    *weights.entry(b).or_default() += u64::from(n);
                }
            }
        }
        if let Some(u) = up {
            for (&(a, b), &n) in &self.south {
                if a == u && allowed(b) {
                    *weights.entry(b).or_default() += u64::from(n);
                }
            }
        }
        if weights.is_empty() {
            return self
                .total
                .iter()
                .filter(|(id, _)| allowed(**id))
                .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
                .map(|(&id, _)| id);
        }
        let sum: u64 = weights.values().sum();
        let mut target = roll % sum.max(1);
        for (id, w) in weights {
            if target < w {
                return Some(id);
            }
            target -= w;
        }
        None
    }
}

/// Famille d'une tuile d'après son nom dans `tiles.lst` : le nom sans chiffres ni extension (`bld012.frm` → `bld`).
pub fn family_of(name: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    stem.trim_end_matches(|c: char| c.is_ascii_digit())
        .to_ascii_lowercase()
}

/// Familles de `names` (contenu de `tiles.lst`) : préfixe → identifiants, dans l'ordre de la liste. Les deux premières
/// entrées (réservé, grille) sont ignorées.
pub fn families(names: &[String]) -> BTreeMap<String, Vec<u16>> {
    let mut map: BTreeMap<String, Vec<u16>> = BTreeMap::new();
    for (i, n) in names.iter().enumerate().skip(2) {
        let f = family_of(n);
        if !f.is_empty() {
            map.entry(f).or_default().push(i as u16);
        }
    }
    map
}

/// Équivalent de la tuile `id` dans une autre famille : même rang dans la famille (modulo sa taille).
pub fn equivalent_in(
    fams: &BTreeMap<String, Vec<u16>>,
    from: &str,
    to: &str,
    id: u16,
) -> Option<u16> {
    let src = fams.get(from)?;
    let dst = fams.get(to)?;
    let rank = src.iter().position(|&x| x == id)?;
    dst.get(rank % dst.len()).copied()
}

/// Remplit les cases `squares` d'une couche avec des tuiles plausibles, case après case (de gauche à droite, de haut en
/// bas), selon les voisines déjà en place. Renvoie le nombre de cases modifiées.
pub fn generate_fill(
    map: &mut Map,
    elevation: usize,
    layer: TileLayer,
    squares: &[usize],
    stats: &TileStats,
    seed: u64,
) -> usize {
    let mut order: Vec<usize> = squares
        .iter()
        .copied()
        .filter(|&s| s < SIDE * SIDE)
        .collect();
    order.sort_unstable();
    order.dedup();
    let mut state = seed | 1;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut changed = 0;
    for sq in order {
        let get = |m: &Map, s: usize| {
            m.tile(elevation, s)
                .map(|t| {
                    if layer == TileLayer::Floor {
                        t.floor
                    } else {
                        t.roof
                    }
                })
                .filter(|&v| v != EMPTY_TILE && v > 1)
        };
        let left = (sq % SIDE > 0).then(|| get(map, sq - 1)).flatten();
        let up = (sq >= SIDE).then(|| get(map, sq - SIDE)).flatten();
        if let Some(id) = stats.pick(left, up, next())
            && map.set_tile(elevation, sq, layer, id)
        {
            changed += 1;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn striped_map() -> Map {
        // Damier de 10 et 11 : chaque tuile n'a que l'autre pour voisine.
        let mut m = Map::new_empty("test", 1);
        for sq in 0..SIDE * SIDE {
            m.set_tile(
                0,
                sq,
                TileLayer::Floor,
                if (sq / SIDE + sq % SIDE) % 2 == 0 {
                    10
                } else {
                    11
                },
            );
        }
        m
    }

    #[test]
    fn counts_neighbors_and_ranks_them() {
        let mut s = TileStats::default();
        s.add_map(&striped_map(), TileLayer::Floor);
        assert_eq!(s.distinct(), 2);
        let n = s.neighbors_of(10);
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].0, 11);
        assert!(n[0].1 > 4000);
    }

    #[test]
    fn pick_follows_the_observed_pattern() {
        let mut s = TileStats::default();
        s.add_map(&striped_map(), TileLayer::Floor);
        for roll in 0..20 {
            assert_eq!(s.pick(Some(10), None, roll), Some(11));
            assert_eq!(s.pick(Some(11), None, roll), Some(10));
        }
        assert!(
            s.pick(None, None, 3).is_some(),
            "sans voisine : la plus fréquente"
        );
    }

    #[test]
    fn constrained_pick_keeps_valid_neighbors_even_when_another_family_is_more_common() {
        let s = TileStats {
            east: HashMap::from([((10, 11), 2), ((10, 99), 200)]),
            south: HashMap::new(),
            total: HashMap::from([(10, 5), (11, 10), (99, 500)]),
        };
        for roll in 0..250 {
            assert_eq!(s.pick_from(Some(10), None, roll, &[10, 11]), Some(11));
            assert_eq!(s.pick_from(None, None, roll, &[10, 11]), Some(11));
        }
        assert_eq!(s.pick_from(None, None, 0, &[]), None);
    }

    #[test]
    fn generation_reproduces_the_pattern_from_a_seed_tile() {
        let mut s = TileStats::default();
        s.add_map(&striped_map(), TileLayer::Floor);
        let mut m = Map::new_empty("gen", 1);
        m.set_tile(0, 0, TileLayer::Floor, 10);
        let squares: Vec<usize> = (1..20).collect();
        let n = generate_fill(&mut m, 0, TileLayer::Floor, &squares, &s, 42);
        assert_eq!(n, 19);
        for sq in 0..20 {
            let want = if sq % 2 == 0 { 10 } else { 11 };
            assert_eq!(m.tile(0, sq).unwrap().floor, want, "case {sq}");
        }
    }

    #[test]
    fn families_group_by_name_prefix_and_map_ranks() {
        let names: Vec<String> = [
            "reserved.frm",
            "grid000.frm",
            "bld001.frm",
            "bld002.frm",
            "hola001.frm",
            "hola002.frm",
            "hola003.frm",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(family_of("BLD012.FRM"), "bld");
        let f = families(&names);
        assert_eq!(f["bld"], vec![2, 3]);
        assert_eq!(f["hola"], vec![4, 5, 6]);
        assert_eq!(equivalent_in(&f, "bld", "hola", 3), Some(5));
        assert_eq!(
            equivalent_in(&f, "hola", "bld", 6),
            Some(2),
            "modulo la taille de la famille d'arrivée"
        );
        assert_eq!(equivalent_in(&f, "bld", "nope", 2), None);
    }
}
