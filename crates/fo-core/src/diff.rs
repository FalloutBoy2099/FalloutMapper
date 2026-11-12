//! Comparaison de deux versions d'une carte : objets ajoutés, retirés, déplacés ou modifiés, et cases de sol et de toit.

use crate::map::{Map, MapObject, TileLayer, word};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Removed,
    Moved,
    /// Même objet, mêmes positions, autre contenu (orientation, drapeaux, script, lumière, inventaire…).
    Modified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectChange {
    pub kind: ChangeKind,
    pub elevation: usize,
    /// Indice de l'objet dans la version « après » (ou « avant » pour un objet retiré).
    pub index: usize,
    pub pid: u32,
    /// Hexagone avant et après (`None` si l'objet n'existait pas / n'existe plus).
    pub from: Option<i32>,
    pub to: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileChange {
    pub elevation: usize,
    pub layer: TileLayer,
    pub square: usize,
    pub before: u16,
    pub after: u16,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MapDiff {
    pub objects: Vec<ObjectChange>,
    pub tiles: Vec<TileChange>,
    /// L'en-tête ou les variables ont changé.
    pub header_changed: bool,
}

impl MapDiff {
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty() && self.tiles.is_empty() && !self.header_changed
    }

    pub fn count(&self, kind: ChangeKind) -> usize {
        self.objects.iter().filter(|c| c.kind == kind).count()
    }
}

/// Clé d'appariement : l'identifiant d'objet ; deux objets qui partagent le même identifiant sont distingués par leur rang.
fn keyed(list: &[MapObject]) -> HashMap<(i32, usize), usize> {
    let mut seen: HashMap<i32, usize> = HashMap::new();
    let mut out = HashMap::new();
    for (i, o) in list.iter().enumerate() {
        let n = seen.entry(o.words[word::ID]).or_default();
        out.insert((o.words[word::ID], *n), i);
        *n += 1;
    }
    out
}

/// Compare `before` à `after`.
pub fn diff_maps(before: &Map, after: &Map) -> MapDiff {
    let mut d = MapDiff {
        header_changed: before.header != after.header
            || before.globals != after.globals
            || before.locals != after.locals,
        ..MapDiff::default()
    };
    for e in 0..before.objects.len().max(after.objects.len()) {
        let (a, b) = (
            before.objects.get(e).map_or(&[][..], |v| v),
            after.objects.get(e).map_or(&[][..], |v| v),
        );
        let (ka, kb) = (keyed(a), keyed(b));
        for (key, &ib) in &kb {
            match ka.get(key) {
                None => d.objects.push(ObjectChange {
                    kind: ChangeKind::Added,
                    elevation: e,
                    index: ib,
                    pid: b[ib].pid(),
                    from: None,
                    to: Some(b[ib].tile()),
                }),
                Some(&ia) if a[ia] == b[ib] => {}
                Some(&ia) => {
                    let kind = if a[ia].tile() != b[ib].tile() {
                        ChangeKind::Moved
                    } else {
                        ChangeKind::Modified
                    };
                    d.objects.push(ObjectChange {
                        kind,
                        elevation: e,
                        index: ib,
                        pid: b[ib].pid(),
                        from: Some(a[ia].tile()),
                        to: Some(b[ib].tile()),
                    });
                }
            }
        }
        for (key, &ia) in &ka {
            if !kb.contains_key(key) {
                d.objects.push(ObjectChange {
                    kind: ChangeKind::Removed,
                    elevation: e,
                    index: ia,
                    pid: a[ia].pid(),
                    from: Some(a[ia].tile()),
                    to: None,
                });
            }
        }
        let (ta, tb) = (
            before.tiles.get(e).and_then(|t| t.as_ref()),
            after.tiles.get(e).and_then(|t| t.as_ref()),
        );
        if let (Some(ta), Some(tb)) = (ta, tb) {
            for (sq, (x, y)) in ta.iter().zip(tb.iter()).enumerate() {
                if x.floor != y.floor {
                    d.tiles.push(TileChange {
                        elevation: e,
                        layer: TileLayer::Floor,
                        square: sq,
                        before: x.floor,
                        after: y.floor,
                    });
                }
                if x.roof != y.roof {
                    d.tiles.push(TileChange {
                        elevation: e,
                        layer: TileLayer::Roof,
                        square: sq,
                        before: x.roof,
                        after: y.roof,
                    });
                }
            }
        }
    }
    d.objects
        .sort_by_key(|c| (c.elevation, c.index, c.kind as u8));
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::ProtoHeader;

    fn proto(n: u32) -> ProtoHeader {
        ProtoHeader {
            pid: 0x0300_0000 | n,
            msg_id: 0,
            fid: 0x0300_0000 | n,
            light_distance: 0,
            light_intensity: 0,
            flags: 0,
            subtype: -1,
            script_id: -1,
            critter: None,
        }
    }

    fn base() -> Map {
        let mut m = Map::new_empty("t", 2);
        for tile in [100, 101, 102] {
            let id = m.max_object_id() + 1;
            m.add_object(0, MapObject::from_proto(&proto(1), tile, id).unwrap());
        }
        m
    }

    #[test]
    fn identical_maps_have_no_difference() {
        let m = base();
        assert!(diff_maps(&m, &m.clone()).is_empty());
    }

    #[test]
    fn added_removed_moved_and_modified_objects_are_told_apart() {
        let before = base();
        let mut after = before.clone();
        after.objects[0][0].words[word::TILE] = 500; // déplacé
        after.objects[0][1].words[word::ORIENTATION] = 4; // modifié
        after.remove_object(0, 2); // retiré
        let id = after.max_object_id() + 1;
        after.add_object(1, MapObject::from_proto(&proto(7), 900, id).unwrap()); // ajouté à l'élévation 1
        after.set_tile(0, 5, TileLayer::Floor, 12);
        let d = diff_maps(&before, &after);
        assert_eq!(
            (
                d.count(ChangeKind::Moved),
                d.count(ChangeKind::Modified),
                d.count(ChangeKind::Removed),
                d.count(ChangeKind::Added)
            ),
            (1, 1, 1, 1)
        );
        let moved = d
            .objects
            .iter()
            .find(|c| c.kind == ChangeKind::Moved)
            .unwrap();
        assert_eq!(
            (moved.from, moved.to, moved.elevation),
            (Some(100), Some(500), 0)
        );
        let added = d
            .objects
            .iter()
            .find(|c| c.kind == ChangeKind::Added)
            .unwrap();
        assert_eq!(
            (added.elevation, added.pid, added.to),
            (1, 0x0300_0007, Some(900))
        );
        assert_eq!(d.tiles.len(), 1);
        assert_eq!((d.tiles[0].square, d.tiles[0].after), (5, 12));
        assert!(!d.header_changed);
    }

    #[test]
    fn deleting_an_object_in_the_middle_does_not_shift_the_others_into_changes() {
        let before = base();
        let mut after = before.clone();
        after.remove_object(0, 0);
        let d = diff_maps(&before, &after);
        assert_eq!(
            d.objects.len(),
            1,
            "seul l'objet retiré est signalé : {:?}",
            d.objects
        );
        assert_eq!(d.objects[0].kind, ChangeKind::Removed);
    }
}
