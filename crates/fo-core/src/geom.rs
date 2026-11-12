//! Géométrie isométrique de Fallout : grille hexagonale 200×200 (objets) et grille carrée 100×100 (sols et toits).
//!
//! Le repère « monde » est en pixels, origine en haut à gauche de la zone de la carte (8100 × 3700).
//!
//! * Une case carrée mesure 80×36 ; passer à la colonne suivante décale de (−48, +12), à la ligne suivante de (+32, +24).
//! * Les hexagones sont numérotés `ligne × 200 + colonne` ; la colonne croît vers la gauche à l'écran, en zigzag.
//!   Chaque case carrée recouvre un bloc de 2×2 hexagones, décalé d'environ 16 pixels vers la droite par rapport au
//!   centre de l'image du sol (c'est ainsi dans le jeu).

pub const HEX_GRID_WIDTH: u32 = 200;
pub const HEX_COUNT: u32 = HEX_GRID_WIDTH * HEX_GRID_WIDTH;
pub const SQUARE_GRID_WIDTH: u32 = 100;
pub const SQUARE_COUNT: u32 = SQUARE_GRID_WIDTH * SQUARE_GRID_WIDTH;
pub const SQUARE_TILE_WIDTH: i32 = 80;
pub const SQUARE_TILE_HEIGHT: i32 = 36;
/// Les toits sont dessinés 96 pixels au-dessus des sols.
pub const ROOF_OFFSET_Y: i32 = -96;
pub const WORLD_WIDTH: i32 = 8100;
pub const WORLD_HEIGHT: i32 = 3700;

/// Centre (en pixels) d'un hexagone : formule du Mapper original de Fallout 2 (`GetHexWorldCoord`), confirmée par
/// Gecko. Les colonnes impaires sont décalées d'un demi-hexagone : de la colonne 0 à 1 on avance de (−32, 0), de 1 à 2
/// de (−16, +12) ; d'une ligne à la suivante, de (+16, +12).
pub fn hex_to_screen(tile: u32) -> (i32, i32) {
    let hx = (tile % HEX_GRID_WIDTH) as i32;
    let hy = (tile / HEX_GRID_WIDTH) as i32;
    (
        4816 - 32 * ((hx + 1) / 2) - 16 * (hx / 2) + 16 * hy,
        12 * (hx / 2) + 12 * hy + 11,
    )
}

/// Hexagone le plus proche d'un point du monde, s'il est dans la grille.
pub fn screen_to_hex(x: i32, y: i32) -> Option<u32> {
    let mut best: Option<(i64, u32)> = None;
    for hy in 0..HEX_GRID_WIDTH as i32 {
        // Sur cette ligne, y = 12*(hx/2) + 12*hy + 11 : on en tire la paire de colonnes voisine.
        let k = ((y - 11 - 12 * hy) as f64 / 12.0).round() as i32;
        for hx in (2 * k - 2)..=(2 * k + 3) {
            if !(0..HEX_GRID_WIDTH as i32).contains(&hx) {
                continue;
            }
            let tile = (hy as u32) * HEX_GRID_WIDTH + hx as u32;
            let (cx, cy) = hex_to_screen(tile);
            let (dx, dy) = ((cx - x) as i64, (cy - y) as i64);
            let d = dx * dx + dy * dy;
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, tile));
            }
        }
    }
    best.filter(|(d, _)| *d < 48 * 48).map(|(_, t)| t)
}

/// Coin haut-gauche de l'image d'une case carrée (sol).
pub fn square_to_screen(tile: u32) -> (i32, i32) {
    let col = (tile % SQUARE_GRID_WIDTH) as i32;
    let row = (tile / SQUARE_GRID_WIDTH) as i32;
    (
        48 * (SQUARE_GRID_WIDTH as i32 - 1 - col) + 32 * row,
        12 * col + 24 * row,
    )
}

/// Case carrée dont le losange contient (ou est le plus proche de) un point du monde.
pub fn screen_to_square(x: i32, y: i32) -> Option<u32> {
    // Centre d'une case : (48u + 32r + 40, 1206 - 12u + 24r) avec u = 99 - colonne.
    let (px, py) = ((x - 40) as f64, (y - 1206) as f64);
    let r = (px + 4.0 * py) / 128.0;
    let u = (px - 32.0 * r) / 48.0;
    let mut best: Option<(f64, u32)> = None;
    for dr in -1..=1 {
        for du in -1..=1 {
            let (rr, uu) = (r.round() as i32 + dr, u.round() as i32 + du);
            if !(0..SQUARE_GRID_WIDTH as i32).contains(&rr)
                || !(0..SQUARE_GRID_WIDTH as i32).contains(&uu)
            {
                continue;
            }
            let (cx, cy) = (48 * uu + 32 * rr + 40, 1206 - 12 * uu + 24 * rr);
            let d = ((x - cx).abs() as f64) / 40.0 + ((y - cy).abs() as f64) / 18.0;
            let tile = rr as u32 * SQUARE_GRID_WIDTH + (SQUARE_GRID_WIDTH as i32 - 1 - uu) as u32;
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, tile));
            }
        }
    }
    best.filter(|(d, _)| *d <= 1.0).map(|(_, t)| t)
}

/// Coordonnées « cube » d'un hexagone : elles suppriment la dépendance à la parité des colonnes, donc translater une
/// forme d'un hexagone à un autre en préserve la disposition (voisins, distances, copies de groupes d'objets).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cube {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl std::ops::Add for Cube {
    type Output = Cube;
    fn add(self, o: Cube) -> Cube {
        Cube {
            x: self.x + o.x,
            y: self.y + o.y,
            z: self.z + o.z,
        }
    }
}

impl std::ops::Sub for Cube {
    type Output = Cube;
    fn sub(self, o: Cube) -> Cube {
        Cube {
            x: self.x - o.x,
            y: self.y - o.y,
            z: self.z - o.z,
        }
    }
}

/// Les six directions voisines, dans l'ordre canonique (voir Gecko `hexgrid::DIRECTIONS`).
pub const CUBE_DIRECTIONS: [Cube; 6] = [
    Cube { x: 1, y: -1, z: 0 },
    Cube { x: 1, y: 0, z: -1 },
    Cube { x: 0, y: 1, z: -1 },
    Cube { x: -1, y: 1, z: 0 },
    Cube { x: -1, y: 0, z: 1 },
    Cube { x: 0, y: -1, z: 1 },
];

pub fn hex_cube(tile: u32) -> Cube {
    let col = (tile % HEX_GRID_WIDTH) as i32;
    let row = (tile / HEX_GRID_WIDTH) as i32;
    let z = row - (col + (col & 1)) / 2;
    Cube {
        x: col,
        y: -col - z,
        z,
    }
}

/// Hexagone d'un cube, `None` s'il est hors de la grille.
pub fn cube_hex(c: Cube) -> Option<u32> {
    let col = c.x;
    let row = c.z + (c.x + (c.x & 1)) / 2;
    let n = HEX_GRID_WIDTH as i32;
    ((0..n).contains(&col) && (0..n).contains(&row)).then(|| (row * n + col) as u32)
}

/// Nombre de pas d'hexagone entre deux hexagones.
pub fn hex_distance(a: u32, b: u32) -> i32 {
    let d = hex_cube(a) - hex_cube(b);
    (d.x.abs() + d.y.abs() + d.z.abs()) / 2
}

/// Voisins d'un hexagone (jusqu'à six, dans la grille).
pub fn hex_neighbors(tile: u32) -> Vec<u32> {
    let c = hex_cube(tile);
    CUBE_DIRECTIONS
        .iter()
        .filter_map(|&d| cube_hex(c + d))
        .collect()
}

/// Hexagones traversés par la ligne droite de `a` à `b` (extrémités comprises), calculés en coordonnées cube.
pub fn hex_line(a: u32, b: u32) -> Vec<u32> {
    let (ca, cb) = (hex_cube(a), hex_cube(b));
    let n = hex_distance(a, b);
    if n == 0 {
        return vec![a];
    }
    (0..=n)
        .filter_map(|i| {
            let t = i as f64 / n as f64;
            let lerp = |p: i32, q: i32| p as f64 + (q - p) as f64 * t;
            cube_hex(cube_round(
                lerp(ca.x, cb.x),
                lerp(ca.y, cb.y),
                lerp(ca.z, cb.z),
            ))
        })
        .collect()
}

fn cube_round(x: f64, y: f64, z: f64) -> Cube {
    let (mut rx, mut ry, mut rz) = (x.round(), y.round(), z.round());
    let (dx, dy, dz) = ((rx - x).abs(), (ry - y).abs(), (rz - z).abs());
    if dx > dy && dx > dz {
        rx = -ry - rz;
    } else if dy > dz {
        ry = -rx - rz;
    } else {
        rz = -rx - ry;
    }
    Cube {
        x: rx as i32,
        y: ry as i32,
        z: rz as i32,
    }
}

/// Hexagones dont le centre est dans le rectangle du monde et qui ont au moins un voisin dehors (ou hors grille) :
/// le contour d'un rectangle, pour poser des bloqueurs de défilement.
pub fn hexes_on_rect_border(left: f32, top: f32, right: f32, bottom: f32) -> Vec<u32> {
    let inside = |t: u32| {
        let (x, y) = hex_to_screen(t);
        (x as f32) >= left && (x as f32) <= right && (y as f32) >= top && (y as f32) <= bottom
    };
    (0..HEX_COUNT)
        .filter(|&t| inside(t))
        .filter(|&t| {
            let c = hex_cube(t);
            CUBE_DIRECTIONS
                .iter()
                .any(|&d| cube_hex(c + d).is_none_or(|n| !inside(n)))
        })
        .collect()
}

/// Où tombe `tile` (écrit par rapport à `from`) quand la forme est posée sur `to`. `None` si hors de la grille.
pub fn hex_translate(tile: u32, from: u32, to: u32) -> Option<u32> {
    cube_hex(hex_cube(to) + (hex_cube(tile) - hex_cube(from)))
}

/// Colonne et ligne d'un hexagone.
pub fn hex_col_row(tile: u32) -> (u32, u32) {
    (tile % HEX_GRID_WIDTH, tile / HEX_GRID_WIDTH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        for tile in (0..HEX_COUNT).step_by(7) {
            let (x, y) = hex_to_screen(tile);
            assert_eq!(screen_to_hex(x, y), Some(tile), "tile {tile} at ({x},{y})");
        }
    }

    #[test]
    fn square_round_trips() {
        for tile in (0..SQUARE_COUNT).step_by(3) {
            let (x, y) = square_to_screen(tile);
            assert_eq!(screen_to_square(x + 40, y + 18), Some(tile), "case {tile}");
        }
        assert_eq!(screen_to_square(-500, -500), None);
    }

    #[test]
    fn cube_model_agrees_with_screen_lattice() {
        // Si la parité des colonnes du modèle cube différait de celle de la formule d'écran, certains voisins seraient loin.
        for tile in [20_100u32, 20_101, 20_300, 20_301, 12_345, 12_346] {
            let (cx, cy) = hex_to_screen(tile);
            let n = hex_neighbors(tile);
            assert_eq!(n.len(), 6);
            for m in n {
                let (x, y) = hex_to_screen(m);
                let d = (((x - cx).pow(2) + (y - cy).pow(2)) as f64).sqrt();
                assert!(d < 34.0, "voisin {m} de {tile} à {d:.0} px");
                assert_eq!(hex_distance(tile, m), 1);
            }
        }
    }

    #[test]
    fn translate_preserves_shape_whatever_the_parity() {
        for (a, b) in [(20_100u32, 20_101u32), (20_100, 20_301), (20_101, 20_300)] {
            for dir in CUBE_DIRECTIONS {
                let n = cube_hex(hex_cube(a) + dir).unwrap();
                let moved = hex_translate(n, a, b).unwrap();
                assert_eq!(hex_distance(moved, b), 1);
                assert_eq!(cube_hex(hex_cube(b) + dir), Some(moved));
            }
        }
        assert_eq!(hex_translate(5, 0, 39_999), None);
    }

    #[test]
    fn hex_line_is_contiguous() {
        let line = hex_line(20_100, 20_460);
        assert_eq!(line.first(), Some(&20_100));
        assert_eq!(line.last(), Some(&20_460));
        assert_eq!(line.len() as i32, hex_distance(20_100, 20_460) + 1);
        assert!(line.windows(2).all(|w| hex_distance(w[0], w[1]) == 1));
        assert_eq!(hex_line(7, 7), vec![7]);
    }

    #[test]
    fn rect_border_is_a_ring() {
        let (cx, cy) = hex_to_screen(20_100);
        let border = hexes_on_rect_border(
            cx as f32 - 150.0,
            cy as f32 - 80.0,
            cx as f32 + 150.0,
            cy as f32 + 80.0,
        );
        assert!(border.len() > 20);
        let inside_count = (0..HEX_COUNT)
            .filter(|&t| {
                let (x, y) = hex_to_screen(t);
                (x as f32 - cx as f32).abs() <= 150.0 && (y as f32 - cy as f32).abs() <= 80.0
            })
            .count();
        assert!(
            border.len() < inside_count,
            "le contour est plus petit que la surface"
        );
        assert!(
            !border.contains(&20_100),
            "le centre n'est pas sur le contour"
        );
    }

    #[test]
    fn hex_positions_match_the_original_mapper() {
        // Valeurs de `GetHexWorldCoord` du Mapper de Fallout 2.
        assert_eq!(hex_to_screen(0), (4816, 11));
        assert_eq!(hex_to_screen(1), (4784, 11));
        assert_eq!(hex_to_screen(2), (4768, 23));
        assert_eq!(hex_to_screen(3), (4736, 23));
        assert_eq!(hex_to_screen(200), (4832, 23));
    }

    #[test]
    fn hex_positions_stay_in_world() {
        for tile in [0, 199, 39_800, 39_999] {
            let (x, y) = hex_to_screen(tile);
            assert!(
                (0..WORLD_WIDTH).contains(&x) && (0..WORLD_HEIGHT).contains(&y),
                "{tile}: ({x},{y})"
            );
        }
    }

    #[test]
    fn neighbours_are_one_hex_apart() {
        // Dans la grille, tile+200 est le voisin direct vers (+16,+12).
        let (x0, y0) = hex_to_screen(1000);
        let (x1, y1) = hex_to_screen(1200);
        assert_eq!((x1 - x0, y1 - y0), (16, 12));
    }
}
