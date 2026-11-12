use super::model::*;
use crate::bytes::Reader;
use crate::error::{Error, Result};
use crate::formats::{ObjectType, item_kind, scenery_kind};

const MAX_INVENTORY_DEPTH: usize = 32;

/// Fournit le sous-type (item / décor) d'un prototype : nécessaire car une carte ne dit pas elle-même
/// combien de mots suivent un objet.
pub trait ProtoResolver {
    fn subtype(&self, pid: u32) -> Option<i32>;
}

impl ProtoResolver for crate::game::GameData {
    fn subtype(&self, pid: u32) -> Option<i32> {
        self.proto_subtype(pid)
    }
}

/// `true` si les données commencent comme une carte de Fallout 1 (19) ou 2 (20).
pub fn is_map(data: &[u8]) -> bool {
    data.len() >= HEADER_WORDS * 4
        && matches!(
            i32::from_be_bytes(data[..4].try_into().unwrap()),
            VERSION_FALLOUT1 | VERSION_FALLOUT2
        )
}

pub fn read_map(data: &[u8], protos: &dyn ProtoResolver) -> Result<Map> {
    if !is_map(data) {
        return Err(Error::format(
            "ce n'est pas une carte de Fallout (version 19 ou 20 attendue)",
        ));
    }
    let mut r = Reader::new(data);
    let version = r.i32()?;
    let name_raw: [u8; NAME_LEN] = r.bytes(NAME_LEN)?.try_into().unwrap();
    let player_tile = r.i32()?;
    let player_elevation = r.i32()?;
    let player_orientation = r.i32()?;
    let local_count = r.i32()?;
    let script_id = r.i32()?;
    let flags = r.i32()?;
    let darkness = r.i32()?;
    let global_count = r.i32()?;
    let map_number = r.i32()?;
    let reserved = (14..HEADER_WORDS)
        .map(|_| r.i32())
        .collect::<Result<Vec<_>>>()?;
    let header = MapHeader {
        version,
        name_raw,
        player_tile,
        player_elevation,
        player_orientation,
        script_id,
        flags,
        darkness,
        map_number,
        reserved,
    };

    if global_count < 0
        || local_count < 0
        || (global_count as usize + local_count as usize) * 4 > r.remaining()
    {
        return Err(Error::format("nombre de variables invalide"));
    }
    let globals = (0..global_count)
        .map(|_| r.i32())
        .collect::<Result<Vec<_>>>()?;
    let locals = (0..local_count)
        .map(|_| r.i32())
        .collect::<Result<Vec<_>>>()?;

    let mut tiles: [Option<Vec<Tile>>; ELEVATION_COUNT] = Default::default();
    for (e, slot) in tiles.iter_mut().enumerate() {
        if !header.has_elevation(e) {
            continue;
        }
        let raw = r.bytes(TILES_PER_ELEVATION * 4)?;
        *slot = Some(
            raw.chunks_exact(4)
                .map(|c| Tile {
                    roof: u16::from_be_bytes([c[0], c[1]]),
                    floor: u16::from_be_bytes([c[2], c[3]]),
                })
                .collect(),
        );
    }

    let mut scripts: [ScriptList; SCRIPT_TYPES] = Default::default();
    for list in &mut scripts {
        *list = read_script_list(&mut r)?;
    }

    let declared_total = r.i32()?;
    let mut objects: [Vec<MapObject>; ELEVATION_COUNT] = Default::default();
    for list in &mut objects {
        let count = r.u32()? as usize;
        if count
            .checked_mul(OBJECT_WORDS * 4)
            .is_none_or(|n| n > r.remaining())
        {
            return Err(Error::format("nombre d'objets invalide"));
        }
        let mut v = Vec::with_capacity(count);
        for _ in 0..count {
            v.push(read_object(&mut r, header.version, protos, 0)?);
        }
        *list = v;
    }
    let trailing = r.bytes(r.remaining())?.to_vec();

    let map = Map {
        header,
        globals,
        locals,
        tiles,
        scripts,
        objects,
        trailing,
        edge: None,
    };
    if declared_total as usize != map.object_count() {
        // L'écriture recalcule ce total ; une divergence ici signale un format mal compris.
        return Err(Error::format(format!(
            "total d'objets annoncé ({declared_total}) différent du nombre lu ({})",
            map.object_count()
        )));
    }
    Ok(map)
}

fn read_script_list(r: &mut Reader) -> Result<ScriptList> {
    let count = r.u32()?;
    if count == 0 {
        return Ok(ScriptList::default());
    }
    let group_count = (count as usize).div_ceil(SCRIPT_GROUP_SLOTS);
    // Borne basse : un emplacement fait au moins (2 + 14) mots, plus 2 mots par groupe.
    if group_count.saturating_mul(SCRIPT_GROUP_SLOTS * (2 + SCRIPT_TAIL_WORDS) * 4) > r.remaining()
    {
        return Err(Error::format("nombre de scripts invalide"));
    }
    let mut groups = Vec::with_capacity(group_count);
    let mut used_total = 0u64;
    for _ in 0..group_count {
        let mut entries = Vec::with_capacity(SCRIPT_GROUP_SLOTS);
        for _ in 0..SCRIPT_GROUP_SLOTS {
            let pid = r.u32()?;
            let next = r.u32()?;
            let typed_words = match pid >> 24 {
                1 => 2,
                2 => 1,
                _ => 0,
            };
            let typed = (0..typed_words)
                .map(|_| r.u32())
                .collect::<Result<Vec<_>>>()?;
            let mut tail = [0i32; SCRIPT_TAIL_WORDS];
            for w in &mut tail {
                *w = r.i32()?;
            }
            entries.push(ScriptEntry {
                pid,
                next,
                typed,
                tail,
            });
        }
        let used = r.u32()?;
        let reserved = r.u32()?;
        used_total += used as u64;
        groups.push(ScriptGroup {
            entries,
            used,
            reserved,
        });
    }
    if used_total != count as u64 {
        return Err(Error::format(
            "les scripts de la carte ne sont pas stockés comme prévu",
        ));
    }
    Ok(ScriptList { count, groups })
}

fn read_object(
    r: &mut Reader,
    version: i32,
    protos: &dyn ProtoResolver,
    depth: usize,
) -> Result<MapObject> {
    if depth > MAX_INVENTORY_DEPTH {
        return Err(Error::format("objets imbriqués trop profondément"));
    }
    let start = r.position();
    let mut words = [0i32; OBJECT_WORDS];
    for w in &mut words {
        *w = r.i32()?;
    }
    let pid = words[word::PID] as u32;
    let ty = ObjectType::from_index(pid >> 24).ok_or_else(|| {
        Error::format(format!(
            "objet de type inconnu ({pid:08X} à l'octet {start})"
        ))
    })?;
    let index = pid & 0x00FF_FFFF;

    let mut subtype = -1;
    let extra_words = match ty {
        ObjectType::Item => {
            subtype = protos.subtype(pid).ok_or_else(|| {
                Error::format(format!("prototype d'objet introuvable ({pid:08X})"))
            })?;
            match subtype {
                item_kind::AMMO | item_kind::MISC | item_kind::KEY => 1,
                item_kind::WEAPON => 2,
                _ => 0,
            }
        }
        ObjectType::Critter => 10,
        ObjectType::Scenery => {
            subtype = protos.subtype(pid).ok_or_else(|| {
                Error::format(format!("prototype de décor introuvable ({pid:08X})"))
            })?;
            match subtype {
                scenery_kind::DOOR => 1,
                scenery_kind::STAIRS => 2,
                scenery_kind::LADDER_BOTTOM | scenery_kind::LADDER_TOP => {
                    // Fallout 1 ne stocke que la destination sur la même carte.
                    if version == VERSION_FALLOUT1 { 1 } else { 2 }
                }
                scenery_kind::ELEVATOR => 2,
                scenery_kind::GENERIC => 0,
                _ => return Err(Error::format("décor de type inconnu")),
            }
        }
        ObjectType::Misc if (16..=23).contains(&index) => 4,
        _ => 0,
    };
    let extra = (0..extra_words)
        .map(|_| r.i32())
        .collect::<Result<Vec<_>>>()?;

    let carried = words[word::INVENTORY_COUNT];
    if carried < 0 || (carried as usize).saturating_mul(4 + OBJECT_WORDS * 4) > r.remaining() {
        return Err(Error::format("nombre d'objets transportés invalide"));
    }
    let mut inventory = Vec::with_capacity(carried as usize);
    for _ in 0..carried {
        let count = r.u32()?;
        inventory.push(InventoryEntry {
            count,
            object: read_object(r, version, protos, depth + 1)?,
        });
    }
    Ok(MapObject {
        words,
        extra,
        inventory,
        subtype,
    })
}
