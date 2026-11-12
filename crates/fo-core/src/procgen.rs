//! Génération procédurale d'une carte extérieure (village, campement) : à partir de quelques paramètres et d'une graine, on dispose
//! des bâtiments, des routes, des sorties, des bordures et du semis de décor de façon cohérente (rien ne se chevauche, chaque
//! porte est reliée à la route, chaque sortie est atteignable depuis le départ du joueur).
//!
//! Ce module ne fait que de la géométrie sur la grille d'hexagones ; la pose des objets et des tuiles (murs d'un style, décor appris
//! d'une carte du jeu) se fait côté éditeur à partir de ce plan. Même graine et mêmes paramètres : même plan.

use crate::geom;
use std::collections::{HashSet, VecDeque};

/// Côté de la zone de jeu, selon les axes de la grille d'hexagones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// Colonnes les plus faibles.
    West,
    East,
    /// Lignes les plus faibles.
    North,
    South,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::West, Side::East, Side::North, Side::South];

    pub fn label(self) -> &'static str {
        match self {
            Side::West => "Ouest",
            Side::East => "Est",
            Side::North => "Nord",
            Side::South => "Sud",
        }
    }
}

/// Zone de jeu : rectangle de colonnes et de lignes d'hexagones (bornes comprises).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    pub c0: u32,
    pub r0: u32,
    pub c1: u32,
    pub r1: u32,
}

impl Area {
    pub fn contains(&self, hex: u32) -> bool {
        let (c, r) = geom::hex_col_row(hex);
        (self.c0..=self.c1).contains(&c) && (self.r0..=self.r1).contains(&r)
    }

    pub fn center(&self) -> u32 {
        hex_at((self.c0 + self.c1) / 2, (self.r0 + self.r1) / 2)
    }

    /// Hexagones du contour (là où se posent les bloqueurs de défilement).
    pub fn border(&self) -> Vec<u32> {
        let mut v = Vec::new();
        for c in self.c0..=self.c1 {
            v.push(hex_at(c, self.r0));
            v.push(hex_at(c, self.r1));
        }
        for r in self.r0 + 1..self.r1 {
            v.push(hex_at(self.c0, r));
            v.push(hex_at(self.c1, r));
        }
        v
    }
}

pub fn hex_at(col: u32, row: u32) -> u32 {
    row * geom::HEX_GRID_WIDTH + col
}

#[derive(Debug, Clone)]
pub struct Params {
    pub seed: u64,
    /// Taille de la zone de jeu en hexagones (colonnes × lignes), au plus 190.
    pub cols: u32,
    pub rows: u32,
    pub buildings: u32,
    /// Au plus ce nombre de pièces en largeur et en profondeur pour un bâtiment.
    pub max_rooms: u32,
    /// Taille d'une pièce en hexagones (longueur le long des colonnes, le long des lignes).
    pub room_cols: u32,
    pub room_rows: u32,
    /// Distance minimale entre deux bâtiments, en hexagones.
    pub spacing: u32,
    /// Côtés où la carte a une sortie vers la carte du monde.
    pub exits: Vec<Side>,
    /// Largeur d'une sortie, en hexagones.
    pub exit_width: u32,
    /// Densité de végétation (0 à 1) et de menus objets près des routes et des bâtiments (0 à 1).
    pub vegetation: f32,
    pub props: f32,
    /// Bande de végétation dense le long du bord de la carte.
    pub border_trees: bool,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            seed: 1,
            cols: 120,
            rows: 120,
            buildings: 6,
            max_rooms: 2,
            room_cols: 12,
            room_rows: 12,
            spacing: 6,
            exits: vec![Side::South, Side::North],
            exit_width: 6,
            vegetation: 0.4,
            props: 0.4,
            border_trees: true,
        }
    }
}

/// Emplacement d'un bâtiment : coin du haut, nombre de pièces et taille d'une pièce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingSite {
    pub corner: u32,
    pub nx: u32,
    pub ny: u32,
    pub rows: u32,
    pub cols: u32,
    /// Rectangle réservé (murs et marge d'un hexagone) : rien d'autre n'y est posé.
    pub reserved: Area,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitSite {
    pub side: Side,
    /// Hexagones des grilles de sortie (une ligne le long du bord).
    pub hexes: Vec<u32>,
    /// Hexagone juste à l'intérieur, au milieu de la sortie : là où arrive la route.
    pub inner: u32,
}

#[derive(Debug, Clone)]
pub struct Sites {
    pub area: Area,
    pub buildings: Vec<BuildingSite>,
    /// Hexagones de la route principale (des sorties jusqu'à la place centrale).
    pub roads: Vec<u32>,
    pub exits: Vec<ExitSite>,
    /// Point de départ du joueur.
    pub start: u32,
    /// Bloqueurs de défilement : le contour de la zone, sauf devant les sorties.
    pub blockers: Vec<u32>,
}

/// Petit générateur déterministe (xorshift).
#[derive(Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn roll(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// Entier dans `lo..=hi`.
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            lo
        } else {
            lo + (self.roll() % u64::from(hi - lo + 1)) as u32
        }
    }

    pub fn chance(&mut self, p: f32) -> bool {
        (self.roll() % 10_000) as f32 / 10_000.0 < p
    }
}

/// Plus court chemin (par les six voisins) de `from` à l'un des `targets`, sans traverser `blocked` ni sortir de `area`.
/// Le chemin va de `from` à la cible, extrémités comprises.
pub fn route(
    from: u32,
    targets: &HashSet<u32>,
    blocked: &HashSet<u32>,
    area: &Area,
) -> Option<Vec<u32>> {
    if targets.contains(&from) {
        return Some(vec![from]);
    }
    let mut prev: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut queue = VecDeque::from([from]);
    prev.insert(from, from);
    while let Some(h) = queue.pop_front() {
        for n in geom::hex_neighbors(h) {
            if prev.contains_key(&n)
                || !area.contains(n)
                || (blocked.contains(&n) && !targets.contains(&n))
            {
                continue;
            }
            prev.insert(n, h);
            if targets.contains(&n) {
                let mut path = vec![n];
                let mut cur = n;
                while cur != from {
                    cur = prev[&cur];
                    path.push(cur);
                }
                path.reverse();
                return Some(path);
            }
            queue.push_back(n);
        }
    }
    None
}

/// Hexagones d'un rectangle, agrandi de `grow` de chaque côté.
fn rect_hexes(a: &Area, grow: u32) -> impl Iterator<Item = u32> + use<> {
    let (c0, r0) = (a.c0.saturating_sub(grow), a.r0.saturating_sub(grow));
    let (c1, r1) = (
        (a.c1 + grow).min(geom::HEX_GRID_WIDTH - 1),
        (a.r1 + grow).min(geom::HEX_GRID_WIDTH - 1),
    );
    (r0..=r1).flat_map(move |r| (c0..=c1).map(move |c| hex_at(c, r)))
}

fn overlaps(a: &Area, b: &Area, gap: u32) -> bool {
    a.c0 <= b.c1 + gap && b.c0 <= a.c1 + gap && a.r0 <= b.r1 + gap && b.r0 <= a.r1 + gap
}

/// Plan des sites : zone, sorties, route principale, bâtiments, départ et contour. Les bâtiments évitent les routes et se tiennent à
/// `spacing` hexagones les uns des autres ; si la zone est trop petite pour tous, il y en a moins (jamais de chevauchement).
pub fn plan_sites(p: &Params) -> Sites {
    let mut rng = Rng::new(p.seed);
    let (cols, rows) = (p.cols.clamp(40, 190), p.rows.clamp(40, 190));
    let (c0, r0) = (
        (geom::HEX_GRID_WIDTH - cols) / 2,
        (geom::HEX_GRID_WIDTH - rows) / 2,
    );
    let area = Area {
        c0,
        r0,
        c1: c0 + cols - 1,
        r1: r0 + rows - 1,
    };
    let center = area.center();

    // Sorties : une ligne d'hexagones au milieu de chaque côté demandé, sur le contour.
    let mut exits = Vec::new();
    for &side in &p.exits {
        if exits.iter().any(|e: &ExitSite| e.side == side) {
            continue;
        }
        let w = p.exit_width.clamp(2, 20);
        let (mid_c, mid_r) = ((area.c0 + area.c1) / 2, (area.r0 + area.r1) / 2);
        let hexes: Vec<u32> = match side {
            Side::West => (0..w).map(|i| hex_at(area.c0, mid_r + i - w / 2)).collect(),
            Side::East => (0..w).map(|i| hex_at(area.c1, mid_r + i - w / 2)).collect(),
            Side::North => (0..w).map(|i| hex_at(mid_c + i - w / 2, area.r0)).collect(),
            Side::South => (0..w).map(|i| hex_at(mid_c + i - w / 2, area.r1)).collect(),
        };
        let mid = hexes[hexes.len() / 2];
        let (mc, mr) = geom::hex_col_row(mid);
        let inner = match side {
            Side::West => hex_at(mc + 2, mr),
            Side::East => hex_at(mc - 2, mr),
            Side::North => hex_at(mc, mr + 2),
            Side::South => hex_at(mc, mr - 2),
        };
        exits.push(ExitSite { side, hexes, inner });
    }

    // Contour sans les sorties : des bloqueurs de défilement.
    let exit_hexes: HashSet<u32> = exits.iter().flat_map(|e| e.hexes.iter().copied()).collect();
    let blockers: Vec<u32> = area
        .border()
        .into_iter()
        .filter(|h| !exit_hexes.contains(h))
        .collect();

    // Route principale : de chaque sortie à la place centrale (ou, sans sortie, un croisement au centre).
    let mut roads: HashSet<u32> = HashSet::new();
    let plaza = Area {
        c0: (area.c0 + area.c1) / 2 - 2,
        r0: (area.r0 + area.r1) / 2 - 2,
        c1: (area.c0 + area.c1) / 2 + 2,
        r1: (area.r0 + area.r1) / 2 + 2,
    };
    roads.extend(rect_hexes(&plaza, 0));
    let hub: HashSet<u32> = HashSet::from([center]);
    let mut start = center;
    for (i, e) in exits.iter().enumerate() {
        let none = HashSet::new();
        if let Some(path) = route(e.inner, &hub, &none, &area) {
            // Le départ du joueur : sur la route de la première sortie, à quelques hexagones de son arrivée.
            if i == 0 {
                start = path[4.min(path.len() - 1)];
            }
            roads.extend(path);
        }
        // Les deux hexagones entre le bord et l'arrivée de la route sont aussi de la route.
        roads.extend(geom::hex_line(e.hexes[e.hexes.len() / 2], e.inner));
    }

    // Bâtiments : essais au hasard, sans chevauchement ni route.
    let mut buildings: Vec<BuildingSite> = Vec::new();
    let road_zone: HashSet<u32> = roads
        .iter()
        .flat_map(|&h| std::iter::once(h).chain(geom::hex_neighbors(h)))
        .collect();
    let mut attempts = 0;
    while (buildings.len() as u32) < p.buildings && attempts < 600 {
        attempts += 1;
        let (nx, ny) = (
            rng.range(1, p.max_rooms.max(1)),
            rng.range(1, p.max_rooms.max(1)),
        );
        let (rc, rr) = (p.room_cols.max(5), p.room_rows.max(4));
        // Emprise : nx pièces de `rc` hexagones en largeur, ny pièces de `rr + 1` en profondeur, plus une marge d'un hexagone.
        let (w, d) = (nx * rc + 2, ny * (rr + 1) + 2);
        if w + 8 >= cols || d + 8 >= rows {
            continue;
        }
        let c = rng.range(area.c0 + 4, area.c1 - w - 4);
        let r = rng.range(area.r0 + 4, area.r1 - d - 4);
        let reserved = Area {
            c0: c,
            r0: r,
            c1: c + w - 1,
            r1: r + d - 1,
        };
        if overlaps(&reserved, &plaza, 2)
            || buildings
                .iter()
                .any(|b| overlaps(&reserved, &b.reserved, p.spacing))
            || rect_hexes(&reserved, 0).any(|h| road_zone.contains(&h))
        {
            continue;
        }
        buildings.push(BuildingSite {
            // Coin sur le quadrillage des cases de sol (colonne et ligne paires) : le sol suit les murs.
            corner: hex_at((c + 1) & !1, (r + 1) & !1),
            nx,
            ny,
            rows: rr,
            cols: rc,
            reserved,
        });
    }

    let mut roads: Vec<u32> = roads.into_iter().collect();
    roads.sort_unstable();
    Sites {
        area,
        buildings,
        roads,
        exits,
        start,
        blockers,
    }
}

/// Hexagones où semer du décor : choisis au hasard parmi les hexagones libres de la zone, séparés d'au moins `gap` hexagones, au plus
/// `count`. `blocked` : tout ce qui est déjà occupé (bâtiments, routes et leur marge). Le résultat est trié (reproductible).
pub fn scatter(
    area: &Area,
    blocked: &HashSet<u32>,
    count: usize,
    gap: i32,
    seed: u64,
    prefer_border: bool,
) -> Vec<u32> {
    let mut rng = Rng::new(seed);
    let mut candidates: Vec<u32> = (area.r0 + 1..area.r1)
        .flat_map(|r| (area.c0 + 1..area.c1).map(move |c| hex_at(c, r)))
        .filter(|h| !blocked.contains(h))
        .collect();
    // Mélange (Fisher-Yates) ; au bord, on trie ensuite pour que les hexagones proches du contour passent en premier.
    for i in (1..candidates.len()).rev() {
        let j = (rng.roll() % (i as u64 + 1)) as usize;
        candidates.swap(i, j);
    }
    if prefer_border {
        let edge = |h: u32| {
            let (c, r) = geom::hex_col_row(h);
            c.min(r).min(area.c1 - c).min(area.r1 - r)
        };
        candidates.sort_by_key(|&h| edge(h) / 3);
    }
    let mut chosen: Vec<u32> = Vec::new();
    for h in candidates {
        if chosen.len() >= count {
            break;
        }
        if chosen.iter().all(|&o| geom::hex_distance(o, h) >= gap) {
            chosen.push(h);
        }
    }
    chosen.sort_unstable();
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> Params {
        Params {
            seed: 42,
            ..Params::default()
        }
    }

    #[test]
    fn same_seed_same_plan_and_other_seed_other_plan() {
        let a = plan_sites(&params());
        let b = plan_sites(&params());
        assert_eq!(a.buildings, b.buildings);
        assert_eq!(a.roads, b.roads);
        let c = plan_sites(&Params {
            seed: 43,
            ..params()
        });
        assert_ne!(a.buildings, c.buildings);
    }

    #[test]
    fn buildings_stay_inside_apart_and_off_the_roads() {
        for seed in 1..30 {
            let p = Params {
                seed,
                buildings: 8,
                ..params()
            };
            let s = plan_sites(&p);
            assert!(!s.buildings.is_empty(), "seed {seed}");
            for (i, b) in s.buildings.iter().enumerate() {
                assert_eq!(
                    (b.cols, b.rows),
                    (p.room_cols, p.room_rows),
                    "dimensions demandées, seed {seed}"
                );
                assert!(
                    s.area.contains(hex_at(b.reserved.c0, b.reserved.r0))
                        && s.area.contains(hex_at(b.reserved.c1, b.reserved.r1))
                );
                for o in &s.buildings[i + 1..] {
                    assert!(
                        !overlaps(&b.reserved, &o.reserved, p.spacing),
                        "chevauchement seed {seed}"
                    );
                }
                assert!(
                    rect_hexes(&b.reserved, 0).all(|h| !s.roads.contains(&h)),
                    "route dans un bâtiment seed {seed}"
                );
            }
        }
    }

    #[test]
    fn exits_sit_on_the_border_the_road_reaches_them_and_the_start_is_on_the_road() {
        let s = plan_sites(&Params {
            exits: vec![Side::West, Side::South, Side::West],
            ..params()
        });
        assert_eq!(s.exits.len(), 2, "pas de doublon de côté");
        let border: HashSet<u32> = s.area.border().into_iter().collect();
        for e in &s.exits {
            assert!(e.hexes.iter().all(|h| border.contains(h)));
            assert!(s.roads.contains(&e.inner), "la route arrive à la sortie");
            // Les hexagones de sortie n'ont pas de bloqueur.
            assert!(e.hexes.iter().all(|h| !s.blockers.contains(h)));
        }
        assert!(s.roads.contains(&s.start));
        // Tout le contour hors sorties est bloqué.
        let exit_hexes: HashSet<u32> = s
            .exits
            .iter()
            .flat_map(|e| e.hexes.iter().copied())
            .collect();
        assert_eq!(s.blockers.len() + exit_hexes.len(), border.len());
    }

    #[test]
    fn roads_form_one_connected_network() {
        let s = plan_sites(&Params {
            exits: vec![Side::West, Side::East, Side::North, Side::South],
            ..params()
        });
        let roads: HashSet<u32> = s.roads.iter().copied().collect();
        let blocked: HashSet<u32> = s
            .area
            .border()
            .into_iter()
            .filter(|h| !roads.contains(h))
            .collect();
        for e in &s.exits {
            let path = route(s.start, &HashSet::from([e.inner]), &HashSet::new(), &s.area).unwrap();
            assert!(
                path.iter()
                    .all(|h| roads.contains(h) || s.area.contains(*h))
            );
            let _ = &blocked;
        }
        // Chaque hexagone de route est atteignable depuis le départ en ne marchant que sur la route.
        let outside: HashSet<u32> = (0..geom::HEX_COUNT)
            .filter(|h| !roads.contains(h))
            .collect();
        for &r in &s.roads {
            assert!(
                route(s.start, &HashSet::from([r]), &outside, &s.area).is_some(),
                "route isolée en {r}"
            );
        }
    }

    #[test]
    fn route_goes_around_obstacles_and_reports_blocked_goals() {
        let area = Area {
            c0: 10,
            r0: 10,
            c1: 40,
            r1: 40,
        };
        let from = hex_at(12, 25);
        let goal = hex_at(30, 25);
        let wall: HashSet<u32> = (15..=35).map(|r| hex_at(20, r)).collect();
        let path = route(from, &HashSet::from([goal]), &wall, &area).unwrap();
        assert_eq!((path[0], *path.last().unwrap()), (from, goal));
        assert!(path.iter().all(|h| !wall.contains(h)));
        assert!(path.windows(2).all(|w| geom::hex_distance(w[0], w[1]) == 1));
        let full: HashSet<u32> = (10..=40).map(|r| hex_at(20, r)).collect();
        assert!(route(from, &HashSet::from([goal]), &full, &area).is_none());
        assert_eq!(
            route(from, &HashSet::from([from]), &HashSet::new(), &area),
            Some(vec![from])
        );
    }

    #[test]
    fn scatter_respects_gap_blocked_and_count() {
        let area = Area {
            c0: 50,
            r0: 50,
            c1: 120,
            r1: 120,
        };
        let blocked: HashSet<u32> = (60..90)
            .flat_map(|r| (60..90).map(move |c| hex_at(c, r)))
            .collect();
        let v = scatter(&area, &blocked, 80, 3, 5, false);
        assert!(v.len() > 40 && v.len() <= 80, "{}", v.len());
        assert!(v.iter().all(|h| !blocked.contains(h) && area.contains(*h)));
        for (i, a) in v.iter().enumerate() {
            assert!(v[i + 1..].iter().all(|b| geom::hex_distance(*a, *b) >= 3));
        }
        assert_eq!(
            v,
            scatter(&area, &blocked, 80, 3, 5, false),
            "reproductible"
        );
        // Au bord : les premiers choisis sont près du contour.
        let edge = scatter(&area, &HashSet::new(), 30, 3, 5, true);
        let near = edge.iter().filter(|&&h| {
            let (c, r) = geom::hex_col_row(h);
            c.min(r).min(area.c1 - c).min(area.r1 - r) < 6
        });
        assert!(near.count() >= 25);
    }
}
