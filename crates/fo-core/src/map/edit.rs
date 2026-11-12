//! Opérations d'édition sur une carte. Elles maintiennent les invariants du format (élévation des objets,
//! cohérence des compteurs au moment de l'écriture) ; le reste des champs reste modifiable directement.

use super::model::*;
use crate::formats::{ObjectType, ProtoHeader, item_kind, scenery_kind};
use crate::geom::HEX_COUNT;

/// Couche d'une case carrée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileLayer {
    Floor,
    Roof,
}

impl Map {
    /// Carte vide : sols et toits vides, aucun objet ni script. `elevations` (1 à 3) élévations existent.
    pub fn new_empty(name: &str, elevations: usize) -> Map {
        let elevations = elevations.clamp(1, ELEVATION_COUNT);
        let mut name_raw = [0u8; NAME_LEN];
        let upper = format!("{}.MAP", name.to_ascii_uppercase());
        for (d, b) in name_raw.iter_mut().zip(upper.bytes().take(NAME_LEN - 1)) {
            *d = b;
        }
        // Bits 1 à 3 : l'élévation correspondante n'existe pas.
        let flags = (elevations..ELEVATION_COUNT).fold(0, |f, e| f | (0x2 << e));
        let mut tiles: [Option<Vec<Tile>>; ELEVATION_COUNT] = Default::default();
        for t in tiles.iter_mut().take(elevations) {
            *t = Some(vec![Tile::EMPTY; TILES_PER_ELEVATION]);
        }
        Map {
            header: MapHeader {
                version: VERSION_FALLOUT2,
                name_raw,
                player_tile: 19_896,
                player_elevation: 0,
                player_orientation: 0,
                script_id: -1,
                flags,
                darkness: 1,
                map_number: 0,
                reserved: vec![0; HEADER_WORDS - 14],
            },
            globals: Vec::new(),
            locals: Vec::new(),
            tiles,
            scripts: Default::default(),
            objects: Default::default(),
            trailing: Vec::new(),
            edge: None,
        }
    }

    /// Remplace le sol ou le toit d'une case. Renvoie `false` si l'élévation ou la case n'existe pas.
    pub fn set_tile(&mut self, elevation: usize, square: usize, layer: TileLayer, id: u16) -> bool {
        let Some(tile) = self
            .tiles
            .get_mut(elevation)
            .and_then(Option::as_mut)
            .and_then(|t| t.get_mut(square))
        else {
            return false;
        };
        let slot = match layer {
            TileLayer::Floor => &mut tile.floor,
            TileLayer::Roof => &mut tile.roof,
        };
        let changed = *slot != id;
        *slot = id;
        changed
    }

    pub fn tile(&self, elevation: usize, square: usize) -> Option<Tile> {
        self.tiles.get(elevation)?.as_ref()?.get(square).copied()
    }

    /// Déplace un objet sur un autre hexagone (et supprime son décalage en pixels).
    pub fn move_object(&mut self, elevation: usize, index: usize, tile: u32) -> bool {
        if tile >= HEX_COUNT {
            return false;
        }
        let Some(o) = self
            .objects
            .get_mut(elevation)
            .and_then(|v| v.get_mut(index))
        else {
            return false;
        };
        if o.words[word::TILE] == tile as i32 {
            return false;
        }
        o.words[word::TILE] = tile as i32;
        true
    }

    /// Crée l'élévation si elle n'existe pas (sols et toits vides). Renvoie `true` si elle existe ensuite. Les élévations se créent
    /// dans l'ordre : on ne saute pas d'élévation.
    pub fn ensure_elevation(&mut self, elevation: usize) -> bool {
        if elevation >= ELEVATION_COUNT {
            return false;
        }
        if self.header.has_elevation(elevation) {
            return true;
        }
        if elevation > 0 && !self.header.has_elevation(elevation - 1) {
            return false;
        }
        // Bits 1 à 3 des drapeaux : l'élévation correspondante n'existe pas.
        self.header.flags &= !(0x2 << elevation);
        self.tiles[elevation] = Some(vec![Tile::EMPTY; TILES_PER_ELEVATION]);
        true
    }

    pub fn remove_object(&mut self, elevation: usize, index: usize) -> Option<MapObject> {
        let list = self.objects.get_mut(elevation)?;
        (index < list.len()).then(|| list.remove(index))
    }

    /// Ajoute un objet à l'élévation (son mot « élévation » est mis à jour). Renvoie son index.
    pub fn add_object(&mut self, elevation: usize, mut object: MapObject) -> usize {
        object.words[word::ELEVATION] = elevation as i32;
        let list = &mut self.objects[elevation];
        list.push(object);
        list.len() - 1
    }

    /// Plus grand identifiant d'objet utilisé (inventaires compris).
    pub fn max_object_id(&self) -> i32 {
        fn walk(o: &MapObject) -> i32 {
            o.inventory
                .iter()
                .map(|i| walk(&i.object))
                .fold(o.words[word::ID], i32::max)
        }
        self.objects.iter().flatten().map(walk).max().unwrap_or(0)
    }

    /// Définit la position de départ du joueur.
    pub fn set_player_start(&mut self, tile: u32, elevation: usize) {
        self.header.player_tile = tile as i32;
        self.header.player_elevation = elevation as i32;
    }
}

impl MapObject {
    /// Nouvel objet issu d'un prototype (les tuiles ne sont pas des objets). Une créature reçoit les points de vie, les
    /// points d'action, le paquet d'IA et l'équipe de son prototype ; le reste de ses données de combat est à zéro.
    pub fn from_proto(proto: &ProtoHeader, tile: u32, id: i32) -> Option<Self> {
        let ty = ObjectType::from_index(proto.pid >> 24)?;
        let extra: Vec<i32> = match ty {
            ObjectType::Tile => return None,
            // Données de combat : seules celles que le prototype fournit ont un sens à la création.
            ObjectType::Critter => {
                let c = proto.critter.as_ref()?;
                vec![
                    0,
                    0,
                    c.action_points,
                    0,
                    c.ai_packet,
                    c.team,
                    -1,
                    c.hit_points,
                    0,
                    0,
                ]
            }
            ObjectType::Item => match proto.subtype {
                item_kind::AMMO | item_kind::MISC | item_kind::KEY => vec![1],
                item_kind::WEAPON => vec![0, -1],
                _ => Vec::new(),
            },
            ObjectType::Scenery => match proto.subtype {
                scenery_kind::DOOR => vec![0],
                scenery_kind::STAIRS
                | scenery_kind::ELEVATOR
                | scenery_kind::LADDER_BOTTOM
                | scenery_kind::LADDER_TOP => vec![0, 0],
                _ => Vec::new(),
            },
            ObjectType::Wall => Vec::new(),
            ObjectType::Misc => {
                if (16..=23).contains(&(proto.pid & 0x00FF_FFFF)) {
                    vec![-1, 0, 0, 0]
                } else {
                    Vec::new()
                }
            }
        };
        let mut words = [0i32; OBJECT_WORDS];
        words[word::ID] = id;
        words[word::TILE] = tile as i32;
        words[word::FID] = proto.fid as i32;
        words[word::FLAGS] = proto.flags as i32;
        words[word::PID] = proto.pid as i32;
        words[word::CID] = -1;
        words[word::LIGHT_DISTANCE] = proto.light_distance;
        words[word::LIGHT_INTENSITY] = proto.light_intensity;
        words[word::SCRIPT_ID] = -1;
        words[word::SCRIPT_INDEX] = -1;
        Some(MapObject {
            words,
            extra,
            inventory: Vec::new(),
            subtype: proto.subtype,
        })
    }

    /// Range un objet dans l'inventaire (le compteur du mot 18 suit ; la capacité, mot 19, reste celle de l'objet).
    pub fn push_inventory(&mut self, count: u32, object: MapObject) {
        self.inventory.push(InventoryEntry { count, object });
        self.words[word::INVENTORY_COUNT] = self.inventory.len() as i32;
    }

    pub fn remove_inventory(&mut self, index: usize) -> Option<InventoryEntry> {
        (index < self.inventory.len()).then(|| {
            let e = self.inventory.remove(index);
            self.words[word::INVENTORY_COUNT] = self.inventory.len() as i32;
            e
        })
    }

    /// Objet rangé dans un inventaire : pas d'hexagone (-1), élévation 0.
    pub fn for_inventory(proto: &ProtoHeader, id: i32) -> Option<Self> {
        let mut o = Self::from_proto(proto, 0, id)?;
        o.words[word::TILE] = -1;
        o.words[word::ELEVATION] = 0;
        Some(o)
    }

    /// Copie à placer sur un autre hexagone (identifiant neuf, sans script ni décalage).
    pub fn duplicate_at(&self, tile: u32, id: i32) -> MapObject {
        let mut o = self.clone();
        o.words[word::ID] = id;
        o.words[word::TILE] = tile as i32;
        o.words[word::SCRIPT_ID] = -1;
        o.words[word::SCRIPT_INDEX] = -1;
        o.assign_fresh_ids(id + 1);
        o
    }

    fn assign_fresh_ids(&mut self, mut next: i32) -> i32 {
        for item in &mut self.inventory {
            item.object.words[word::ID] = next;
            next = item.object.assign_fresh_ids(next + 1);
        }
        next
    }

    pub fn set_flag(&mut self, flag: u32, on: bool) {
        let mut f = self.words[word::FLAGS] as u32;
        if on {
            f |= flag;
        } else {
            f &= !flag;
        }
        self.words[word::FLAGS] = f as i32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_elevation_is_created_in_order_and_survives_a_write() {
        let mut m = Map::new_empty("t", 1);
        assert!(!m.header.has_elevation(1));
        assert!(!m.ensure_elevation(2), "pas de saut d'élévation");
        assert!(m.ensure_elevation(1));
        assert!(m.header.has_elevation(1) && !m.header.has_elevation(2));
        assert_eq!(m.tiles[1].as_ref().map(Vec::len), Some(TILES_PER_ELEVATION));
        assert!(m.ensure_elevation(1), "déjà là");
        assert!(!m.ensure_elevation(3));
        let bytes = crate::map::write_map(&m).unwrap();
        struct NoProtos;
        impl crate::map::ProtoResolver for NoProtos {
            fn subtype(&self, _pid: u32) -> Option<i32> {
                None
            }
        }
        let back = crate::map::read_map(&bytes, &NoProtos).unwrap();
        assert!(back.header.has_elevation(1) && !back.header.has_elevation(2));
    }
}
