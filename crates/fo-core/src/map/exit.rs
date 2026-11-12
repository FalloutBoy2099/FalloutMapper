//! Sorties de carte (« exit grids ») et bloqueurs de défilement : objets « divers » dont le prototype et l'image portent
//! le sens. Table vérifiée sur Gecko (`util/Constants.h`, `ExitGridDirection.h`) et les cartes d'origine.
//!
//! Le PROTOTYPE (16 à 23) donne la direction ; l'IMAGE donne la famille : verte (vers une autre carte, image = proto + 1) ou
//! brune (vers la carte du monde ou le plan de ville, image = proto + 0x11).

use super::model::*;
use crate::formats::ObjectType;

pub const FIRST_EXIT_PROTO: u32 = 0x0500_0010;
pub const EXIT_DIRECTIONS: usize = 8;
/// Prototype « Scroll Blocker » : le moteur le reconnaît à ce PID exact (`obj->pid == 0x500000C`).
pub const SCROLL_BLOCKER_PID: u32 = 0x0500_000C;
/// Destination spéciale : carte du monde / plan de ville.
pub const WORLD_MAP_EXIT: i32 = -2;
pub const TOWN_MAP_EXIT: i32 = -1;
/// Drapeaux d'une sortie telle que l'enregistrent les cartes d'origine (traverse tout, à plat, sans blocage).
pub const EXIT_GRID_FLAGS: u32 = 0xA000_8018;

pub const DIR_LEFT: u8 = 0;
pub const DIR_RIGHT: u8 = 1;
pub const DIR_BOTTOM: u8 = 2;
pub const DIR_TOP: u8 = 3;
pub const DIR_FWD_A: u8 = 4;
pub const DIR_FWD_B: u8 = 5;
pub const DIR_BACK_A: u8 = 6;
pub const DIR_BACK_B: u8 = 7;

pub const DIRECTION_NAMES: [&str; EXIT_DIRECTIONS] = [
    "Gauche",
    "Droite",
    "Bas",
    "Haut",
    "Diagonale / (A)",
    "Diagonale / (B)",
    "Diagonale \\ (A)",
    "Diagonale \\ (B)",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitFamily {
    /// Vers une carte précise : image verte.
    InterMap,
    /// Vers la carte du monde ou le plan de ville : image brune.
    World,
}

pub fn exit_family(dest_map: i32) -> ExitFamily {
    if dest_map == WORLD_MAP_EXIT || dest_map == TOWN_MAP_EXIT {
        ExitFamily::World
    } else {
        ExitFamily::InterMap
    }
}

pub fn exit_pid(dir: u8) -> u32 {
    FIRST_EXIT_PROTO + (dir as u32).min(7)
}

pub fn exit_fid(dir: u8, family: ExitFamily) -> u32 {
    exit_pid(dir) + if family == ExitFamily::World { 0x11 } else { 1 }
}

/// Direction (0 à 7) portée par un PID de sortie, `None` si ce n'en est pas une.
pub fn exit_dir_of_pid(pid: u32) -> Option<u8> {
    (FIRST_EXIT_PROTO..FIRST_EXIT_PROTO + 8)
        .contains(&pid)
        .then(|| (pid - FIRST_EXIT_PROTO) as u8)
}

/// L'autre côté de la même bande (A ↔ B, gauche ↔ droite…).
pub fn flip_direction(dir: u8) -> u8 {
    dir ^ 1
}

/// Direction qui convient à un segment tracé à l'écran (`dx`, `dy` avec y vers le bas) : proche d'une diagonale →
/// « / » ou « \ » ; sinon horizontal (bas) ou vertical (gauche). `flipped` choisit l'autre côté.
pub fn direction_for_segment(dx: i32, dy: i32, flipped: bool) -> u8 {
    let (ax, ay) = (dx.abs(), dy.abs());
    let dir = if ax.min(ay) * 2 >= ax.max(ay) {
        // En écran (y vers le bas), « / » monte vers la droite.
        if (dx > 0) != (dy > 0) {
            DIR_FWD_A
        } else {
            DIR_BACK_A
        }
    } else if ax > ay {
        DIR_BOTTOM
    } else {
        DIR_LEFT
    };
    if flipped { flip_direction(dir) } else { dir }
}

impl MapObject {
    /// Nouvelle sortie de carte. `flags` : ceux du prototype si on les connaît, sinon [`EXIT_GRID_FLAGS`].
    pub fn exit_grid(dir: u8, dest: ExitDestination, tile: u32, id: i32) -> MapObject {
        let mut words = [0i32; OBJECT_WORDS];
        words[word::ID] = id;
        words[word::TILE] = tile as i32;
        words[word::FID] = exit_fid(dir, exit_family(dest.map)) as i32;
        words[word::FLAGS] = EXIT_GRID_FLAGS as i32;
        words[word::PID] = exit_pid(dir) as i32;
        words[word::CID] = -1;
        words[word::SCRIPT_ID] = -1;
        words[word::SCRIPT_INDEX] = -1;
        MapObject {
            words,
            extra: vec![dest.map, dest.tile, dest.elevation, dest.orientation],
            inventory: Vec::new(),
            subtype: -1,
        }
    }

    pub fn is_scroll_blocker(&self) -> bool {
        self.pid() == SCROLL_BLOCKER_PID
    }

    /// Direction d'une sortie de carte.
    pub fn exit_direction(&self) -> Option<u8> {
        if self.object_type() == Some(ObjectType::Misc) {
            exit_dir_of_pid(self.pid())
        } else {
            None
        }
    }

    /// Change la destination d'une sortie et remet son image dans la bonne famille (verte / brune).
    pub fn set_exit_destination(&mut self, dest: ExitDestination) -> bool {
        let Some(dir) = self.exit_direction() else {
            return false;
        };
        self.extra = vec![dest.map, dest.tile, dest.elevation, dest.orientation];
        self.words[word::FID] = exit_fid(dir, exit_family(dest.map)) as i32;
        true
    }

    /// Change la direction d'une sortie (prototype et image).
    pub fn set_exit_direction(&mut self, dir: u8) -> bool {
        let Some(dest) = self.exit_destination() else {
            return false;
        };
        self.words[word::PID] = exit_pid(dir) as i32;
        self.words[word::FID] = exit_fid(dir, exit_family(dest.map)) as i32;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn art_follows_the_original_maps_convention() {
        // Relevé dans arvillag : proto 0x05000017 (dir 7), image verte 0x05000018.
        assert_eq!(exit_pid(7), 0x0500_0017);
        assert_eq!(exit_fid(7, ExitFamily::InterMap), 0x0500_0018);
        assert_eq!(exit_fid(0, ExitFamily::World), 0x0500_0021);
        assert_eq!(exit_family(-2), ExitFamily::World);
        assert_eq!(exit_family(3), ExitFamily::InterMap);
    }

    #[test]
    fn segment_direction_and_flip() {
        assert_eq!(direction_for_segment(100, -60, false), DIR_FWD_A);
        assert_eq!(direction_for_segment(100, 60, false), DIR_BACK_A);
        assert_eq!(direction_for_segment(200, 10, false), DIR_BOTTOM);
        assert_eq!(direction_for_segment(10, 200, false), DIR_LEFT);
        assert_eq!(direction_for_segment(200, 10, true), DIR_TOP);
        assert_eq!(flip_direction(DIR_FWD_B), DIR_FWD_A);
    }

    #[test]
    fn exit_object_round_trips_its_fields() {
        let dest = ExitDestination {
            map: 3,
            tile: 14128,
            elevation: 2,
            orientation: 0,
        };
        let mut o = MapObject::exit_grid(DIR_BACK_B, dest, 8094, 42);
        assert!(o.is_exit_grid());
        assert_eq!(o.exit_destination(), Some(dest));
        assert_eq!(o.exit_direction(), Some(DIR_BACK_B));
        assert!(o.set_exit_destination(ExitDestination {
            map: WORLD_MAP_EXIT,
            ..dest
        }));
        assert_eq!(o.fid(), exit_fid(DIR_BACK_B, ExitFamily::World));
        assert!(o.set_exit_direction(DIR_TOP));
        assert_eq!(o.pid(), exit_pid(DIR_TOP));
    }
}
