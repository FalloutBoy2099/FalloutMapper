//! Recherche et remplacement d'objets par prototype.

use super::{Map, MapObject, word};
use crate::formats::ProtoHeader;

impl Map {
    /// Objets (élévation, indice) dont le prototype est `pid`, dans une élévation ou dans toutes (`None`).
    pub fn find_objects(&self, pid: u32, elevation: Option<usize>) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (e, list) in self.objects.iter().enumerate() {
            if elevation.is_some_and(|only| only != e) {
                continue;
            }
            out.extend(
                list.iter()
                    .enumerate()
                    .filter(|(_, o)| o.pid() == pid)
                    .map(|(i, _)| (e, i)),
            );
        }
        out
    }

    /// Remplace l'objet par un objet du prototype `proto`, sur le même hexagone, avec la même orientation, le même décalage
    /// et le même identifiant. Le script est celui du nouveau prototype ; l'inventaire est gardé quand le type ne change pas.
    /// Renvoie `false` si l'objet n'existe pas, n'a pas d'hexagone ou si le prototype ne peut pas devenir un objet de carte.
    pub fn replace_object(&mut self, elevation: usize, index: usize, proto: &ProtoHeader) -> bool {
        let Some(old) = self.objects.get(elevation).and_then(|l| l.get(index)) else {
            return false;
        };
        if old.tile() < 0 {
            return false;
        }
        let Some(mut new) = MapObject::from_proto(proto, old.tile() as u32, old.words[word::ID])
        else {
            return false;
        };
        new.words[word::ELEVATION] = old.words[word::ELEVATION];
        new.words[word::ORIENTATION] = old.words[word::ORIENTATION];
        new.words[word::OFFSET_X] = old.words[word::OFFSET_X];
        new.words[word::OFFSET_Y] = old.words[word::OFFSET_Y];
        if old.pid() >> 24 == new.pid() >> 24 && !old.inventory.is_empty() {
            new.inventory = old.inventory.clone();
            new.words[word::INVENTORY_COUNT] = new.inventory.len() as i32;
        }
        self.objects[elevation][index] = new;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall_proto(n: u32) -> ProtoHeader {
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

    fn map_with_walls() -> Map {
        let mut m = Map::new_empty("t", 2);
        for (e, tile, pid) in [(0, 100, 1), (0, 101, 1), (1, 102, 1), (0, 103, 2)] {
            let mut o =
                MapObject::from_proto(&wall_proto(pid), tile, m.max_object_id() + 1).unwrap();
            o.words[word::ORIENTATION] = 3;
            o.words[word::OFFSET_X] = 7;
            m.add_object(e, o);
        }
        m
    }

    #[test]
    fn finds_by_prototype_in_one_or_all_elevations() {
        let m = map_with_walls();
        assert_eq!(
            m.find_objects(0x0300_0001, None),
            vec![(0, 0), (0, 1), (1, 0)]
        );
        assert_eq!(m.find_objects(0x0300_0001, Some(0)), vec![(0, 0), (0, 1)]);
        assert!(m.find_objects(0x0300_0009, None).is_empty());
    }

    #[test]
    fn replacement_keeps_position_orientation_offset_and_id() {
        let mut m = map_with_walls();
        let (id, count) = (m.objects[0][0].words[word::ID], m.object_count());
        assert!(m.replace_object(0, 0, &wall_proto(5)));
        let o = &m.objects[0][0];
        assert_eq!(
            (
                o.pid(),
                o.fid(),
                o.tile(),
                o.orientation(),
                o.offset(),
                o.words[word::ID]
            ),
            (0x0300_0005, 0x0300_0005, 100, 3, (7, 0), id)
        );
        assert_eq!(m.object_count(), count);
        assert_eq!(m.find_objects(0x0300_0001, None).len(), 2);
        assert_eq!(m.find_objects(0x0300_0005, None), vec![(0, 0)]);
        assert!(
            !m.replace_object(0, 99, &wall_proto(5)),
            "indice inexistant"
        );
        // Un prototype de tuile ne peut pas devenir un objet.
        let tile = ProtoHeader {
            pid: 0x0400_0001,
            ..wall_proto(1)
        };
        assert!(!m.replace_object(0, 1, &tile));
        assert_eq!(m.objects[0][1].pid(), 0x0300_0001, "l'objet est inchangé");
    }
}
