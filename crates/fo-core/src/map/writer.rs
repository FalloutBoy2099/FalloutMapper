use super::model::*;
use crate::bytes::PutBe;
use crate::error::{Error, Result};

/// Sérialise une carte. Les compteurs (variables, objets, scripts utilisés) sont recalculés depuis le modèle.
pub fn write_map(map: &Map) -> Result<Vec<u8>> {
    let h = &map.header;
    let mut out: Vec<u8> = Vec::with_capacity(1 << 20);

    out.put_i32(h.version);
    out.extend_from_slice(&h.name_raw);
    out.put_i32(h.player_tile);
    out.put_i32(h.player_elevation);
    out.put_i32(h.player_orientation);
    out.put_i32(map.locals.len() as i32);
    out.put_i32(h.script_id);
    out.put_i32(h.flags);
    out.put_i32(h.darkness);
    out.put_i32(map.globals.len() as i32);
    out.put_i32(h.map_number);
    if h.reserved.len() != HEADER_WORDS - 14 {
        return Err(Error::format(
            "en-tête : zone réservée de taille incorrecte",
        ));
    }
    for &w in &h.reserved {
        out.put_i32(w);
    }

    for &v in map.globals.iter().chain(&map.locals) {
        out.put_i32(v);
    }

    for (e, tiles) in map.tiles.iter().enumerate() {
        match (h.has_elevation(e), tiles) {
            (true, Some(t)) if t.len() == TILES_PER_ELEVATION => {
                for tile in t {
                    out.put_u16(tile.roof);
                    out.put_u16(tile.floor);
                }
            }
            (false, None) => {}
            _ => {
                return Err(Error::format(format!(
                    "élévation {e} : tuiles incohérentes avec les drapeaux de l'en-tête"
                )));
            }
        }
    }

    for list in &map.scripts {
        write_script_list(&mut out, list)?;
    }

    out.put_i32(map.object_count() as i32);
    for objects in &map.objects {
        out.put_u32(objects.len() as u32);
        for o in objects {
            write_object(&mut out, o);
        }
    }
    out.extend_from_slice(&map.trailing);
    Ok(out)
}

fn write_script_list(out: &mut Vec<u8>, list: &ScriptList) -> Result<()> {
    let used: u32 = list.groups.iter().map(|g| g.used).sum();
    if used != list.count || list.groups.len() != (list.count as usize).div_ceil(SCRIPT_GROUP_SLOTS)
    {
        return Err(Error::format("scripts : compteurs incohérents"));
    }
    out.put_u32(list.count);
    for g in &list.groups {
        if g.entries.len() != SCRIPT_GROUP_SLOTS {
            return Err(Error::format(
                "scripts : un groupe doit contenir 16 emplacements",
            ));
        }
        for e in &g.entries {
            out.put_u32(e.pid);
            out.put_u32(e.next);
            for &w in &e.typed {
                out.put_u32(w);
            }
            for &w in &e.tail {
                out.put_i32(w);
            }
        }
        out.put_u32(g.used);
        out.put_u32(g.reserved);
    }
    Ok(())
}

fn write_object(out: &mut Vec<u8>, o: &MapObject) {
    let mut words = o.words;
    words[word::INVENTORY_COUNT] = o.inventory.len() as i32;
    for w in words {
        out.put_i32(w);
    }
    for &w in &o.extra {
        out.put_i32(w);
    }
    for item in &o.inventory {
        out.put_u32(item.count);
        write_object(out, &item.object);
    }
}
