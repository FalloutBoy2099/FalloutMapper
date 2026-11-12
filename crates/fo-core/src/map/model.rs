//! Modèle de données d'une carte (`.map`) de Fallout 2.
//!
//! Le modèle est *sans perte* : chaque champ non interprété est conservé tel quel, de sorte que
//! `write(read(octets)) == octets`. C'est la base de l'édition future des cartes.

use crate::formats::ObjectType;

pub const TILES_PER_ELEVATION: usize = 10_000;
pub const ELEVATION_COUNT: usize = 3;
pub const HEADER_WORDS: usize = 59;
pub const NAME_LEN: usize = 16;
pub const OBJECT_WORDS: usize = 22;
pub const SCRIPT_TYPES: usize = 5;
pub const SCRIPT_GROUP_SLOTS: usize = 16;
/// Mots communs à tous les emplacements de script (après l'éventuel bloc propre au type).
pub const SCRIPT_TAIL_WORDS: usize = 14;
pub const EMPTY_TILE: u16 = 1;

pub const VERSION_FALLOUT1: i32 = 19;
pub const VERSION_FALLOUT2: i32 = 20;

/// Noms des types de scripts, dans l'ordre du fichier.
pub const SCRIPT_TYPE_NAMES: [&str; SCRIPT_TYPES] =
    ["Système", "Spatial", "Minuterie", "Objet", "Créature"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapHeader {
    pub version: i32,
    /// Nom du fichier, 16 octets (terminé par 0, la suite est conservée telle quelle).
    pub name_raw: [u8; NAME_LEN],
    pub player_tile: i32,
    pub player_elevation: i32,
    pub player_orientation: i32,
    /// Identifiant du script de la carte (index + 1 dans `scripts.lst`, 0 ou -1 si aucun).
    pub script_id: i32,
    /// Bit 0 : carte d'une sauvegarde. Bits 1 à 3 : l'élévation 0, 1 ou 2 n'existe pas.
    pub flags: i32,
    pub darkness: i32,
    pub map_number: i32,
    /// Mots 14 à 58 de l'en-tête (horodatage et réservé).
    pub reserved: Vec<i32>,
}

impl MapHeader {
    pub fn name(&self) -> String {
        let end = self
            .name_raw
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(NAME_LEN);
        self.name_raw[..end].iter().map(|&b| b as char).collect()
    }

    pub fn has_elevation(&self, elevation: usize) -> bool {
        elevation < ELEVATION_COUNT && self.flags & (0x2 << elevation) == 0
    }
}

/// Une case de sol / toit (mot de 32 bits : toit dans les 16 bits de poids fort).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    pub roof: u16,
    pub floor: u16,
}

impl Tile {
    pub const EMPTY: Tile = Tile {
        roof: EMPTY_TILE,
        floor: EMPTY_TILE,
    };
}

/// Un emplacement de script. Les mots sont conservés bruts ; les accesseurs donnent les champs connus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptEntry {
    /// Identifiant du script : type dans l'octet de poids fort.
    pub pid: u32,
    pub next: u32,
    /// Mots propres au type : 2 pour un script spatial, 1 pour une minuterie, 0 sinon.
    pub typed: Vec<u32>,
    pub tail: [i32; SCRIPT_TAIL_WORDS],
}

impl ScriptEntry {
    pub fn script_type(&self) -> u32 {
        self.pid >> 24
    }
    /// Numéro de la ligne dans `scripts.lst` (à partir de 0).
    pub fn list_index(&self) -> i32 {
        self.tail[1]
    }
    /// Case déclencheuse d'un script spatial (hexagone, élévation, rayon).
    pub fn spatial(&self) -> Option<SpatialTrigger> {
        (self.script_type() == 1 && self.typed.len() == 2).then(|| SpatialTrigger {
            tile: self.typed[0] & 0x03FF_FFFF,
            elevation: self.typed[0] >> 29,
            radius: self.typed[1] as i32,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpatialTrigger {
    pub tile: u32,
    pub elevation: u32,
    pub radius: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScriptGroup {
    /// Toujours 16 emplacements ; seuls les `used` premiers sont valides.
    pub entries: Vec<ScriptEntry>,
    pub used: u32,
    pub reserved: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScriptList {
    pub count: u32,
    pub groups: Vec<ScriptGroup>,
}

impl ScriptList {
    /// Les scripts réellement utilisés.
    pub fn iter(&self) -> impl Iterator<Item = &ScriptEntry> {
        self.groups
            .iter()
            .flat_map(|g| g.entries.iter().take(g.used as usize))
    }
}

/// Un objet de la carte (22 mots communs + données propres à son type + inventaire).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapObject {
    pub words: [i32; OBJECT_WORDS],
    /// Mots supplémentaires selon le type (munitions, destination d'une sortie, etc.).
    pub extra: Vec<i32>,
    pub inventory: Vec<InventoryEntry>,
    /// Sous-type issu du prototype (item ou décor), -1 si sans objet. Non sérialisé.
    pub subtype: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryEntry {
    pub count: u32,
    pub object: MapObject,
}

/// Indices des mots communs d'un objet.
pub mod word {
    pub const ID: usize = 0;
    pub const TILE: usize = 1;
    /// Décalage en pixels par rapport au centre de l'hexagone.
    pub const OFFSET_X: usize = 2;
    pub const OFFSET_Y: usize = 3;
    /// Position écran au moment de l'enregistrement (sans effet au chargement).
    pub const SCREEN_X: usize = 4;
    pub const SCREEN_Y: usize = 5;
    pub const FRAME: usize = 6;
    pub const ORIENTATION: usize = 7;
    pub const FID: usize = 8;
    pub const FLAGS: usize = 9;
    pub const ELEVATION: usize = 10;
    pub const PID: usize = 11;
    pub const CID: usize = 12;
    pub const LIGHT_DISTANCE: usize = 13;
    pub const LIGHT_INTENSITY: usize = 14;
    pub const OUTLINE: usize = 15;
    pub const SCRIPT_ID: usize = 16;
    pub const SCRIPT_INDEX: usize = 17;
    pub const INVENTORY_COUNT: usize = 18;
    pub const INVENTORY_CAPACITY: usize = 19;
}

/// Drapeaux d'objet.
pub mod flags {
    pub const HIDDEN: u32 = 0x0000_0001;
    pub const FLAT: u32 = 0x0000_0008;
    pub const NO_BLOCK: u32 = 0x0000_0010;
    pub const MULTIHEX: u32 = 0x0000_0800;
    pub const TRANS_RED: u32 = 0x0000_4000;
    pub const TRANS_NONE: u32 = 0x0000_8000;
    pub const TRANS_WALL: u32 = 0x0001_0000;
    pub const TRANS_GLASS: u32 = 0x0002_0000;
    pub const TRANS_STEAM: u32 = 0x0004_0000;
    pub const TRANS_ENERGY: u32 = 0x0008_0000;
    pub const LIGHT_THRU: u32 = 0x2000_0000;
    pub const SHOOT_THRU: u32 = 0x8000_0000;
}

/// Drapeaux d'objet avec leur libellé (pour l'éditeur).
pub const FLAG_LABELS: &[(u32, &str)] = &[
    (flags::HIDDEN, "Masqué"),
    (flags::FLAT, "À plat (dessiné sous les autres)"),
    (flags::NO_BLOCK, "Ne bloque pas le passage"),
    (0x0000_0020, "Éclairage"),
    (0x0000_0400, "Ne peut pas être retiré"),
    (flags::MULTIHEX, "Plusieurs hexagones"),
    (0x0000_1000, "Sans surbrillance"),
    (0x0000_2000, "En file d'attente"),
    (flags::TRANS_RED, "Transparence rouge"),
    (flags::TRANS_NONE, "Sans transparence"),
    (flags::TRANS_WALL, "Transparence mur"),
    (flags::TRANS_GLASS, "Transparence verre"),
    (flags::TRANS_STEAM, "Transparence vapeur"),
    (flags::TRANS_ENERGY, "Transparence énergie"),
    (0x0100_0000, "Tenu en main gauche"),
    (0x0200_0000, "Tenu en main droite"),
    (0x0400_0000, "Armure portée"),
    (0x1000_0000, "Fin de transparence de mur"),
    (flags::LIGHT_THRU, "La lumière traverse"),
    (0x4000_0000, "Vu"),
    (flags::SHOOT_THRU, "Les tirs traversent"),
];

impl MapObject {
    pub fn tile(&self) -> i32 {
        self.words[word::TILE]
    }
    pub fn offset(&self) -> (i32, i32) {
        (self.words[word::OFFSET_X], self.words[word::OFFSET_Y])
    }
    pub fn frame(&self) -> i32 {
        self.words[word::FRAME]
    }
    pub fn orientation(&self) -> i32 {
        self.words[word::ORIENTATION]
    }
    pub fn fid(&self) -> u32 {
        self.words[word::FID] as u32
    }
    pub fn flags(&self) -> u32 {
        self.words[word::FLAGS] as u32
    }
    pub fn elevation(&self) -> i32 {
        self.words[word::ELEVATION]
    }
    pub fn pid(&self) -> u32 {
        self.words[word::PID] as u32
    }
    pub fn script_id(&self) -> i32 {
        self.words[word::SCRIPT_ID]
    }
    pub fn object_type(&self) -> Option<ObjectType> {
        ObjectType::from_index(self.pid() >> 24)
    }
    /// Numéro du prototype dans sa liste (à partir de 1).
    pub fn proto_index(&self) -> u32 {
        self.pid() & 0x00FF_FFFF
    }
    pub fn has_flag(&self, f: u32) -> bool {
        self.flags() & f != 0
    }
    /// Sortie de carte : objet « divers » de numéro 16 à 23.
    pub fn is_exit_grid(&self) -> bool {
        self.object_type() == Some(ObjectType::Misc) && (16..=23).contains(&self.proto_index())
    }
    /// Destination d'une sortie de carte (carte, hexagone, élévation, orientation).
    pub fn exit_destination(&self) -> Option<ExitDestination> {
        (self.is_exit_grid() && self.extra.len() >= 4).then(|| ExitDestination {
            map: self.extra[0],
            tile: self.extra[1],
            elevation: self.extra[2],
            orientation: self.extra[3],
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitDestination {
    /// Numéro de carte, ou -2 pour la carte du monde, -1 pour la même carte.
    pub map: i32,
    pub tile: i32,
    pub elevation: i32,
    pub orientation: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map {
    pub header: MapHeader,
    pub globals: Vec<i32>,
    pub locals: Vec<i32>,
    /// Sols et toits par élévation ; `None` si l'élévation n'existe pas.
    pub tiles: [Option<Vec<Tile>>; ELEVATION_COUNT],
    pub scripts: [ScriptList; SCRIPT_TYPES],
    /// Objets de premier niveau par élévation (même si l'élévation n'a pas de tuiles).
    pub objects: [Vec<MapObject>; ELEVATION_COUNT],
    /// Octets après le dernier objet (normalement aucun).
    pub trailing: Vec<u8>,
    /// Limites de défilement (fichier `.edg` voisin) : chargées à part, jamais écrites dans le `.map`.
    pub edge: Option<crate::edge::MapEdge>,
}

impl Map {
    /// Emplacement de script désigné par l'identifiant de script d'un objet (`words[16]`).
    pub fn find_script(&self, sid: i32) -> Option<&ScriptEntry> {
        if sid == -1 || sid == 0 {
            return None;
        }
        self.scripts
            .iter()
            .flat_map(ScriptList::iter)
            .find(|e| e.pid == sid as u32)
    }

    /// Script de la carte elle-même (`header.script_id` = index dans `scripts.lst` + 1).
    pub fn map_script_index(&self) -> Option<usize> {
        usize::try_from(self.header.script_id)
            .ok()
            .and_then(|i| i.checked_sub(1))
    }

    pub fn object_count(&self) -> usize {
        self.objects.iter().map(Vec::len).sum()
    }

    /// Nombre total d'objets, inventaires compris.
    pub fn total_object_count(&self) -> usize {
        fn count(o: &MapObject) -> usize {
            1 + o.inventory.iter().map(|i| count(&i.object)).sum::<usize>()
        }
        self.objects.iter().flatten().map(count).sum()
    }
}
