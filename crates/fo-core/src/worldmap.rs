//! Carte du monde : les zones (villes, lieux) de `data/city.txt` et les tuiles de fond de `data/worldmap.txt`.
//! Les fichiers sont lus par sections (voir [`crate::ini_doc`]) : on ne réécrit que les clés modifiées.

use crate::ini_doc::{IniDoc, IniSection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entrance {
    /// Clé de la ligne : `entrance_0`.
    pub key: String,
    pub on: bool,
    /// Position sur la carte de la ville (-1 : aucune).
    pub x: i32,
    pub y: i32,
    /// Nom de la carte (`lookup_name` de maps.txt).
    pub map: String,
    pub elevation: i32,
    pub tile: i32,
    pub orientation: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Area {
    /// Nom de la section : `Area 00`.
    pub section: String,
    pub name: String,
    pub world_pos: (i32, i32),
    pub start_on: bool,
    /// `Small`, `Medium` ou `Large`.
    pub size: String,
    pub entrances: Vec<Entrance>,
}

pub const AREA_SIZES: [&str; 3] = ["Small", "Medium", "Large"];

fn pair(v: &str) -> Option<(i32, i32)> {
    let (a, b) = v.split_once(',')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

fn parse_entrance(key: &str, value: &str) -> Option<Entrance> {
    let p: Vec<&str> = value.split(',').map(str::trim).collect();
    if p.len() < 7 {
        return None;
    }
    Some(Entrance {
        key: key.to_string(),
        on: p[0].eq_ignore_ascii_case("on"),
        x: p[1].parse().ok()?,
        y: p[2].parse().ok()?,
        map: p[3].to_string(),
        elevation: p[4].parse().ok()?,
        tile: p[5].parse().ok()?,
        orientation: p[6].parse().ok()?,
    })
}

fn format_entrance(e: &Entrance) -> String {
    format!(
        "{},{},{},{},{},{},{}",
        if e.on { "On" } else { "Off" },
        e.x,
        e.y,
        e.map,
        e.elevation,
        e.tile,
        e.orientation
    )
}

impl Area {
    pub fn from_section(s: &IniSection) -> Option<Self> {
        let number = s.name.strip_prefix("Area ")?;
        let _: u32 = number.trim().parse().ok()?;
        let mut entrances: Vec<Entrance> = s
            .keys()
            .into_iter()
            .filter(|k| k.to_ascii_lowercase().starts_with("entrance_"))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter_map(|k| parse_entrance(&k, &s.get(&k)?))
            .collect();
        entrances.sort_by_key(|e| {
            e.key
                .trim_start_matches("entrance_")
                .parse::<u32>()
                .unwrap_or(0)
        });
        Some(Area {
            section: s.name.clone(),
            name: s.get("area_name").unwrap_or_default(),
            world_pos: s.get("world_pos").and_then(|v| pair(&v)).unwrap_or((0, 0)),
            start_on: s
                .get("start_state")
                .is_some_and(|v| v.eq_ignore_ascii_case("on")),
            size: s.get("size").unwrap_or_else(|| "Medium".into()),
            entrances,
        })
    }

    /// Reporte les valeurs dans la section en conservant les commentaires des lignes existantes.
    pub fn apply_to(&self, s: &mut IniSection) {
        s.set("area_name", &self.name);
        s.set(
            "world_pos",
            &format!("{},{}", self.world_pos.0, self.world_pos.1),
        );
        s.set("start_state", if self.start_on { "On" } else { "Off" });
        s.set("size", &self.size);
        // Les entrées supprimées disparaissent du fichier ; les autres sont mises à jour (ou ajoutées).
        let gone: Vec<String> = s
            .keys()
            .into_iter()
            .filter(|k| {
                k.to_ascii_lowercase().starts_with("entrance_")
                    && !self.entrances.iter().any(|e| e.key.eq_ignore_ascii_case(k))
            })
            .collect();
        for k in gone {
            s.remove(&k);
        }
        for e in &self.entrances {
            s.set(&e.key, &format_entrance(e));
        }
    }
}

/// Toutes les zones d'un `city.txt`, dans l'ordre du fichier.
pub fn parse_areas(doc: &IniDoc) -> Vec<Area> {
    doc.sections.iter().filter_map(Area::from_section).collect()
}

/// Reporte les zones modifiées dans le document (les zones sont retrouvées par leur nom de section).
pub fn apply_areas(doc: &mut IniDoc, areas: &[Area]) {
    for a in areas {
        if let Some(s) = doc.section_mut(&a.section) {
            a.apply_to(s);
        }
    }
}

/// Tuiles de fond de la carte du monde : numéro d'image (`art_idx`) de chaque `[Tile N]`, et nombre de tuiles par ligne.
pub fn parse_tiles(worldmap: &IniDoc) -> (usize, Vec<i32>) {
    let per_row = worldmap
        .section("Tile Data")
        .and_then(|s| s.get("num_horizontal_tiles"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let mut tiles: Vec<(u32, i32)> = worldmap
        .sections
        .iter()
        .filter_map(|s| {
            let n: u32 = s.name.strip_prefix("Tile ")?.trim().parse().ok()?;
            Some((n, s.get("art_idx")?.parse().ok()?))
        })
        .collect();
    tiles.sort_by_key(|t| t.0);
    (per_row, tiles.into_iter().map(|t| t.1).collect())
}

/// Dimensions d'une tuile de la carte du monde et de l'une de ses cases de terrain (7 × 6 cases par tuile).
pub const TILE_WIDTH: i32 = 350;
pub const TILE_HEIGHT: i32 = 300;
pub const CELL: i32 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terrain {
    Desert,
    Mountain,
    City,
    Ocean,
}

/// Terrain de la carte du monde, case par case (cases de 50 × 50 pixels du monde), lu dans les tuiles de `worldmap.txt`.
#[derive(Debug, Clone, Default)]
pub struct TerrainMap {
    /// Nombre de cases en largeur et en hauteur.
    pub cols: usize,
    pub rows: usize,
    cells: Vec<Option<Terrain>>,
}

impl TerrainMap {
    pub fn at(&self, x: i32, y: i32) -> Option<Terrain> {
        if x < 0 || y < 0 {
            return None;
        }
        let (c, r) = ((x / CELL) as usize, (y / CELL) as usize);
        (c < self.cols && r < self.rows)
            .then(|| self.cells[r * self.cols + c])
            .flatten()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.iter().all(Option::is_none)
    }

    fn cell(&self, c: i64, r: i64) -> Option<Terrain> {
        (c >= 0 && r >= 0 && (c as usize) < self.cols && (r as usize) < self.rows)
            .then(|| self.cells[r as usize * self.cols + c as usize])
            .flatten()
    }
}

/// Terrain de chaque case : une tuile `[Tile n]` pose ses cases `x_y=Terrain,…` (x de 0 à 6, y de 0 à 5) à la place `n`.
pub fn parse_terrain(worldmap: &IniDoc) -> TerrainMap {
    let per_row = worldmap
        .section("Tile Data")
        .and_then(|s| s.get("num_horizontal_tiles"))
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(4)
        .max(1);
    let per_tile_cols = (TILE_WIDTH / CELL) as usize;
    let per_tile_rows = (TILE_HEIGHT / CELL) as usize;
    let tiles: Vec<(usize, &crate::ini_doc::IniSection)> = worldmap
        .sections
        .iter()
        .filter_map(|s| {
            Some((
                s.name.strip_prefix("Tile ")?.trim().parse::<usize>().ok()?,
                s,
            ))
        })
        .collect();
    let tile_count = tiles.iter().map(|t| t.0 + 1).max().unwrap_or(0);
    let tile_rows = tile_count.div_ceil(per_row);
    let (cols, rows) = (per_row * per_tile_cols, tile_rows * per_tile_rows);
    let mut cells = vec![None; cols * rows];
    for (n, section) in tiles {
        let (tc, tr) = (n % per_row, n / per_row);
        for x in 0..per_tile_cols {
            for y in 0..per_tile_rows {
                let Some(v) = section.get(&format!("{x}_{y}")) else {
                    continue;
                };
                let terrain = match v
                    .split(',')
                    .next()
                    .map(|t| t.trim().to_ascii_lowercase())
                    .as_deref()
                {
                    Some("desert") => Terrain::Desert,
                    Some("mountain") => Terrain::Mountain,
                    Some("city") => Terrain::City,
                    Some("ocean") => Terrain::Ocean,
                    _ => continue,
                };
                cells[(tr * per_tile_rows + y) * cols + tc * per_tile_cols + x] = Some(terrain);
            }
        }
    }
    TerrainMap { cols, rows, cells }
}

/// Une zone dont le jeu choisit la position au hasard : désactivée au départ, avec une position de remplissage que d'autres zones
/// partagent exactement (le fichier le dit : « some locations have random-by-game positions »).
pub fn is_random_area(areas: &[Area], index: usize) -> bool {
    let Some(a) = areas.get(index) else {
        return false;
    };
    !a.start_on
        && areas
            .iter()
            .enumerate()
            .any(|(j, b)| j != index && b.world_pos == a.world_pos)
}

/// Où une zone aléatoire peut se trouver, d'après les cartes locales de ses entrées.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementRule {
    Desert,
    Mountain,
    /// Sur la terre, au bord de l'océan.
    Coast,
    /// N'importe où sur la terre (désert ou montagne).
    Land,
}

/// Règle de placement d'après le nom de la première carte de la zone (« Desert Encounter 3 », « Coast Encounter 1 »…) ou son nom.
pub fn placement_rule(area: &Area) -> PlacementRule {
    let text = area
        .entrances
        .first()
        .map_or(String::new(), |e| e.map.to_ascii_lowercase())
        + " "
        + &area.name.to_ascii_lowercase();
    if text.contains("mountain") {
        PlacementRule::Mountain
    } else if text.contains("coast") || text.contains("whale") {
        PlacementRule::Coast
    } else if text.contains("desert") {
        PlacementRule::Desert
    } else {
        PlacementRule::Land
    }
}

/// Distance minimale (en pixels du monde) entre une zone aléatoire et une autre zone placée : les cercles ne doivent pas se recouvrir.
pub const MIN_DISTANCE: i32 = 45;

/// Tire une position pour la zone `index` : une case du bon terrain, éloignée des autres zones déjà placées (celles dont la position
/// est le remplissage commun des zones aléatoires ne comptent pas). `seed` rend le tirage reproductible ; un autre `seed` donne un autre
/// lieu. `None` si aucune case ne convient (ou si le terrain n'est pas lu).
pub fn roll_position(
    terrain: &TerrainMap,
    areas: &[Area],
    index: usize,
    seed: u64,
) -> Option<(i32, i32)> {
    let rule = placement_rule(areas.get(index)?);
    let others: Vec<(i32, i32)> = areas
        .iter()
        .enumerate()
        .filter(|(j, _)| *j != index && !is_random_area(areas, *j))
        .map(|(_, a)| a.world_pos)
        .collect();
    let land = |t: Option<Terrain>| matches!(t, Some(Terrain::Desert | Terrain::Mountain));
    let mut candidates: Vec<(i32, i32)> = Vec::new();
    for r in 0..terrain.rows as i64 {
        for c in 0..terrain.cols as i64 {
            let cell = terrain.cell(c, r);
            let ok = match rule {
                PlacementRule::Desert => cell == Some(Terrain::Desert),
                PlacementRule::Mountain => cell == Some(Terrain::Mountain),
                PlacementRule::Land => land(cell),
                PlacementRule::Coast => {
                    land(cell)
                        && [
                            (-1, 0),
                            (1, 0),
                            (0, -1),
                            (0, 1),
                            (-1, -1),
                            (1, 1),
                            (-1, 1),
                            (1, -1),
                        ]
                        .iter()
                        .any(|(dc, dr)| terrain.cell(c + dc, r + dr) == Some(Terrain::Ocean))
                }
            };
            if !ok {
                continue;
            }
            let center = (c as i32 * CELL + CELL / 2, r as i32 * CELL + CELL / 2);
            if others.iter().all(|o| {
                (o.0 - center.0).pow(2) + (o.1 - center.1).pow(2) >= MIN_DISTANCE * MIN_DISTANCE
            }) {
                candidates.push(center);
            }
        }
    }
    if candidates.is_empty() {
        return None;
    }
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let (cx, cy) = candidates[(next() % candidates.len() as u64) as usize];
    // Un peu de dispersion dans la case.
    let jitter = |v: u64| (v % 31) as i32 - 15;
    Some((cx + jitter(next()), cy + jitter(next())))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CITY: &str = "[Area 00]   ; Arroyo\narea_name=Arroyo\nworld_pos=184,133        ; Absolute\nstart_state=On\nsize=Medium\nentrance_0=On,350,275,Arroyo Bridge,-1,-1,3  ; Etc.\nentrance_1=Off,-1,-1,Arroyo Caves,-1,-1,0\n\n[Area 01]\narea_name=Den\nworld_pos=473,272\n";

    #[test]
    fn reads_areas_with_entrances() {
        let doc = IniDoc::parse(CITY);
        let areas = parse_areas(&doc);
        assert_eq!(areas.len(), 2);
        let a = &areas[0];
        assert_eq!(
            (a.name.as_str(), a.world_pos, a.start_on, a.size.as_str()),
            ("Arroyo", (184, 133), true, "Medium")
        );
        assert_eq!(a.entrances.len(), 2);
        assert_eq!(
            (
                a.entrances[0].map.as_str(),
                a.entrances[0].x,
                a.entrances[0].orientation
            ),
            ("Arroyo Bridge", 350, 3)
        );
        assert!(!a.entrances[1].on);
    }

    #[test]
    fn editing_an_area_only_touches_its_own_lines_and_keeps_comments() {
        let mut doc = IniDoc::parse(CITY);
        let mut areas = parse_areas(&doc);
        areas[0].world_pos = (200, 150);
        areas[0].entrances[0].x = 360;
        apply_areas(&mut doc, &areas);
        let t = doc.to_text();
        assert!(t.contains("world_pos=200,150  ; Absolute"));
        assert!(t.contains("entrance_0=On,360,275,Arroyo Bridge,-1,-1,3  ; Etc."));
        assert!(
            t.contains("[Area 01]\narea_name=Den\nworld_pos=473,272\n"),
            "la zone suivante est intacte"
        );
    }

    #[test]
    fn removed_and_added_entrances_are_written() {
        let mut doc = IniDoc::parse(CITY);
        let mut areas = parse_areas(&doc);
        areas[0].entrances.remove(1);
        let mut e = areas[0].entrances[0].clone();
        e.key = "entrance_2".into();
        e.map = "Arroyo Temple".into();
        areas[0].entrances.push(e);
        apply_areas(&mut doc, &areas);
        let t = doc.to_text();
        assert!(!t.contains("entrance_1="));
        assert!(t.contains("entrance_2=On,350,275,Arroyo Temple,-1,-1,3"));
        assert_eq!(parse_areas(&doc)[0].entrances.len(), 2);
    }

    #[test]
    fn reads_background_tiles() {
        let doc = IniDoc::parse(
            "[Tile Data]\nnum_horizontal_tiles=4\n\n[Tile 1]\nart_idx=340\n[Tile 0]\nart_idx=339\n",
        );
        assert_eq!(parse_tiles(&doc), (4, vec![339, 340]));
    }

    fn terrain_doc() -> IniDoc {
        // Deux tuiles côte à côte : désert partout sauf une colonne d'océan à l'ouest de la première et une montagne à l'est de la seconde.
        let mut t = String::from("[Tile Data]\nnum_horizontal_tiles=2\n");
        for n in 0..2 {
            t.push_str(&format!("[Tile {n}]\nart_idx=1\n"));
            for x in 0..7 {
                for y in 0..6 {
                    let terrain = if n == 0 && x == 0 {
                        "Ocean"
                    } else if n == 1 && x >= 5 {
                        "Mountain"
                    } else {
                        "Desert"
                    };
                    t.push_str(&format!("{x}_{y}={terrain},Fill_W,None,None,None,X\n"));
                }
            }
        }
        IniDoc::parse(&t)
    }

    fn area(name: &str, pos: (i32, i32), on: bool, map: &str) -> Area {
        Area {
            section: "Area 00".into(),
            name: name.into(),
            world_pos: pos,
            start_on: on,
            size: "Small".into(),
            entrances: vec![Entrance {
                key: "entrance_0".into(),
                on: true,
                x: 0,
                y: 0,
                map: map.into(),
                elevation: -1,
                tile: -1,
                orientation: 0,
            }],
        }
    }

    #[test]
    fn terrain_is_read_cell_by_cell() {
        let t = parse_terrain(&terrain_doc());
        assert_eq!((t.cols, t.rows), (14, 6));
        assert_eq!(t.at(10, 10), Some(Terrain::Ocean));
        assert_eq!(t.at(120, 10), Some(Terrain::Desert));
        assert_eq!(t.at(350 + 300, 100), Some(Terrain::Mountain));
        assert_eq!(t.at(-5, 0), None);
        assert_eq!(t.at(5000, 0), None);
    }

    #[test]
    fn random_areas_are_the_off_ones_sharing_a_filler_position() {
        let areas = vec![
            area("Town", (100, 100), true, "Town"),
            area("A", (120, 350), false, "Desert Encounter 1"),
            area("B", (120, 350), false, "Mountain Encounter 1"),
            area("Off but alone", (400, 100), false, "X"),
        ];
        assert!(!is_random_area(&areas, 0));
        assert!(is_random_area(&areas, 1) && is_random_area(&areas, 2));
        assert!(
            !is_random_area(&areas, 3),
            "une zone éteinte mais seule à sa place n'est pas aléatoire"
        );
        assert_eq!(placement_rule(&areas[1]), PlacementRule::Desert);
        assert_eq!(placement_rule(&areas[2]), PlacementRule::Mountain);
        assert_eq!(
            placement_rule(&area(
                "Special Whale",
                (1, 1),
                false,
                "Special Whale Encounter"
            )),
            PlacementRule::Coast
        );
        assert_eq!(
            placement_rule(&area("Random", (1, 1), false, "Cavern Encounter 6")),
            PlacementRule::Land
        );
    }

    #[test]
    fn rolls_land_on_the_right_terrain_away_from_other_areas_and_can_be_rerolled() {
        let t = parse_terrain(&terrain_doc());
        let areas = vec![
            area("Town", (200, 100), true, "Town"),
            area("D", (120, 350), false, "Desert Encounter 1"),
            area("M", (120, 350), false, "Mountain Encounter 1"),
            area("C", (120, 350), false, "Coast Encounter 1"),
        ];
        let mut seen = std::collections::HashSet::new();
        for seed in 1..40 {
            let d = roll_position(&t, &areas, 1, seed).unwrap();
            assert_eq!(t.at(d.0, d.1), Some(Terrain::Desert), "{d:?}");
            assert!(
                (d.0 - 200).pow(2) + (d.1 - 100).pow(2) >= MIN_DISTANCE * MIN_DISTANCE - 450,
                "pas sur la ville : {d:?}"
            );
            let m = roll_position(&t, &areas, 2, seed).unwrap();
            assert_eq!(t.at(m.0, m.1), Some(Terrain::Mountain));
            // Côte : sur la terre, collée à l'océan (colonne 1, juste à l'est de l'océan de la première colonne).
            let c = roll_position(&t, &areas, 3, seed).unwrap();
            assert_eq!(t.at(c.0, c.1).map(|x| x != Terrain::Ocean), Some(true));
            assert!(c.0 < 100 + 15, "au bord de l'océan : {c:?}");
            seen.insert(d);
        }
        assert!(
            seen.len() > 5,
            "relancer donne d'autres lieux : {}",
            seen.len()
        );
        assert_eq!(
            roll_position(&t, &areas, 1, 7),
            roll_position(&t, &areas, 1, 7),
            "reproductible"
        );
        assert_eq!(
            roll_position(&TerrainMap::default(), &areas, 1, 1),
            None,
            "sans terrain lu : rien"
        );
    }
}
