//! Vérificateur de carte : repère ce qui fera planter ou mal fonctionner le jeu avant de l'essayer.

use crate::art::{ArtLibrary, Fid};
use crate::formats::ObjectType;
use crate::game::GameData;
use crate::geom;
use crate::map::{Map, MapObject, word};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IssueKind {
    /// Objet sans hexagone valide.
    OutOfGrid,
    /// Deux objets identiques sur le même hexagone.
    Duplicate,
    /// Le prototype de l'objet n'existe pas.
    UnknownPrototype,
    /// Un script de la carte ou d'un objet n'existe pas.
    ScriptMissing,
    /// Une sortie mène à une carte qui n'existe pas.
    ExitMissingMap,
    /// L'image de l'objet est introuvable.
    MissingArt,
    /// Le point de départ du joueur est invalide ou tombe sur un mur.
    BadStart,
    /// Une sortie, une porte ou un escalier qu'on ne peut pas atteindre à pied depuis le point de départ du joueur.
    Unreachable,
}

impl IssueKind {
    pub const ALL: [IssueKind; 8] = [
        IssueKind::OutOfGrid,
        IssueKind::Duplicate,
        IssueKind::UnknownPrototype,
        IssueKind::ScriptMissing,
        IssueKind::ExitMissingMap,
        IssueKind::MissingArt,
        IssueKind::BadStart,
        IssueKind::Unreachable,
    ];

    /// Libellé (français ; traduit par l'interface).
    pub fn label(self) -> &'static str {
        match self {
            IssueKind::OutOfGrid => "Objet hors de la grille",
            IssueKind::Duplicate => "Objet en double",
            IssueKind::UnknownPrototype => "Prototype introuvable",
            IssueKind::ScriptMissing => "Script introuvable",
            IssueKind::ExitMissingMap => "Sortie vers une carte inexistante",
            IssueKind::MissingArt => "Image introuvable",
            IssueKind::BadStart => "Point de départ invalide",
            IssueKind::Unreachable => "Inaccessible depuis le départ",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub kind: IssueKind,
    pub elevation: Option<usize>,
    /// Indice de l'objet dans la liste de son élévation.
    pub index: Option<usize>,
    pub hex: Option<u32>,
    /// Précision : nom du script, numéro de carte, PID…
    pub detail: String,
    /// Identifiant de l'entrée de script concernée (scripts spatiaux et de minuterie).
    pub script: Option<u32>,
}

fn issue(
    kind: IssueKind,
    elevation: Option<usize>,
    index: Option<usize>,
    hex: Option<u32>,
    detail: impl Into<String>,
) -> Issue {
    Issue {
        kind,
        elevation,
        index,
        hex,
        detail: detail.into(),
        script: None,
    }
}

fn hex_of(o: &MapObject) -> Option<u32> {
    (0..geom::HEX_COUNT as i32)
        .contains(&o.tile())
        .then(|| o.tile() as u32)
}

/// Examine la carte. Les problèmes sont rendus dans l'ordre : élévation, puis nature, puis objet.
pub fn check_map(map: &Map, game: &GameData, art: &ArtLibrary) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut proto_ok: HashMap<u32, bool> = HashMap::new();
    let mut art_ok: HashMap<u32, bool> = HashMap::new();
    let map_numbers: HashSet<u32> = game.list_maps().iter().filter_map(|m| m.number).collect();
    let script_exists = |index: usize| {
        game.script_file(index)
            .is_some_and(|f| game.vfs().exists(&format!("scripts/{f}")))
    };

    for (e, list) in map.objects.iter().enumerate() {
        let mut seen: HashSet<Vec<i32>> = HashSet::new();
        for (i, o) in list.iter().enumerate() {
            let hex = hex_of(o);
            if hex.is_none() {
                out.push(issue(
                    IssueKind::OutOfGrid,
                    Some(e),
                    Some(i),
                    None,
                    format!("{} (PID {:08X})", o.tile(), o.pid()),
                ));
            }
            // Doublon : tous les mots identiques, sauf l'identifiant.
            let key: Vec<i32> = o
                .words
                .iter()
                .enumerate()
                .filter(|(w, _)| *w != word::ID)
                .map(|(_, v)| *v)
                .chain(o.extra.iter().copied())
                .collect();
            // Les objets « divers » (bloqueurs de défilement, marqueurs) sont volontairement répétés dans les cartes d'origine.
            if !seen.insert(key) && hex.is_some() && o.object_type() != Some(ObjectType::Misc) {
                out.push(issue(
                    IssueKind::Duplicate,
                    Some(e),
                    Some(i),
                    hex,
                    format!("PID {:08X}", o.pid()),
                ));
            }
            if !*proto_ok
                .entry(o.pid())
                .or_insert_with(|| game.proto_info(o.pid()).is_some())
            {
                out.push(issue(
                    IssueKind::UnknownPrototype,
                    Some(e),
                    Some(i),
                    hex,
                    format!("PID {:08X}", o.pid()),
                ));
            }
            if o.fid() != u32::MAX
                && !*art_ok.entry(o.fid()).or_insert_with(|| {
                    art.fid_paths(Fid(o.fid()), 0)
                        .iter()
                        .any(|p| game.vfs().exists(p))
                })
            {
                out.push(issue(
                    IssueKind::MissingArt,
                    Some(e),
                    Some(i),
                    hex,
                    format!("FID {:08X}", o.fid()),
                ));
            }
            // Script attaché à l'objet : l'emplacement doit exister dans la carte et pointer vers un fichier.
            if o.script_id() != -1 && o.script_id() != 0 {
                match map.find_script(o.script_id()) {
                    None => out.push(issue(
                        IssueKind::ScriptMissing,
                        Some(e),
                        Some(i),
                        hex,
                        format!("emplacement {:08X} absent de la carte", o.script_id()),
                    )),
                    Some(s) if !script_exists(s.list_index().max(0) as usize) => {
                        let name = game
                            .script_file(s.list_index().max(0) as usize)
                            .unwrap_or_else(|| format!("n° {}", s.list_index()));
                        out.push(issue(IssueKind::ScriptMissing, Some(e), Some(i), hex, name));
                    }
                    Some(_) => {}
                }
            }
            if let Some(dest) = o.exit_destination()
                && dest.map >= 0
                && !map_numbers.contains(&(dest.map as u32))
            {
                out.push(issue(
                    IssueKind::ExitMissingMap,
                    Some(e),
                    Some(i),
                    hex,
                    format!("carte n° {}", dest.map),
                ));
            }
        }
    }

    // Script de la carte et scripts spatiaux ou de minuterie.
    if let Some(index) = map.map_script_index()
        && !script_exists(index)
    {
        out.push(issue(
            IssueKind::ScriptMissing,
            None,
            None,
            None,
            game.script_file(index)
                .unwrap_or_else(|| format!("script de la carte n° {index}")),
        ));
    }
    for entry in map.scripts.iter().flat_map(|l| l.iter()) {
        if entry.script_type() == 1 && !script_exists(entry.list_index().max(0) as usize) {
            let spatial = entry.spatial();
            let mut i = issue(
                IssueKind::ScriptMissing,
                spatial.map(|s| s.elevation as usize),
                None,
                spatial.map(|s| s.tile).filter(|t| *t < geom::HEX_COUNT),
                game.script_file(entry.list_index().max(0) as usize)
                    .unwrap_or_else(|| format!("n° {}", entry.list_index())),
            );
            i.script = Some(entry.pid);
            out.push(i);
        }
    }

    // Point de départ.
    let (tile, elevation) = (map.header.player_tile, map.header.player_elevation);
    let start_hex = (0..geom::HEX_COUNT as i32)
        .contains(&tile)
        .then_some(tile as u32);
    if start_hex.is_none()
        || !(0..3).contains(&elevation)
        || !map.header.has_elevation(elevation as usize)
    {
        out.push(issue(
            IssueKind::BadStart,
            None,
            None,
            start_hex,
            format!("hexagone {tile}, élévation {elevation}"),
        ));
    } else if let Some(list) = map.objects.get(elevation as usize)
        && let Some(i) = list
            .iter()
            .position(|o| o.object_type() == Some(ObjectType::Wall) && o.tile() == tile)
    {
        out.push(issue(
            IssueKind::BadStart,
            Some(elevation as usize),
            Some(i),
            start_hex,
            format!("hexagone {tile} : un mur"),
        ));
    }

    // Accessibilité : sorties, portes et escaliers de l'élévation du départ qu'on ne peut pas toucher en marchant.
    if let Some(start) = start_hex
        && let Some(list) = map.objects.get(elevation as usize)
        && (0..3).contains(&elevation)
    {
        let walk = crate::reach::walk_map(map, elevation as usize, game);
        let seen = crate::reach::reachable(&walk.blocked, start);
        if seen.iter().any(|b| *b) {
            for (i, o) in list.iter().enumerate() {
                let Some(hex) = hex_of(o) else { continue };
                if o.flags() & crate::map::flags::HIDDEN != 0 || crate::reach::touched(&seen, hex) {
                    continue;
                }
                let what = if let Some(dest) = o.exit_destination() {
                    format!("sortie vers la carte n° {}", dest.map)
                } else if o.object_type() == Some(ObjectType::Scenery) {
                    match game.proto_subtype(o.pid()) {
                        Some(crate::formats::scenery_kind::DOOR) => "porte".to_string(),
                        Some(
                            crate::formats::scenery_kind::STAIRS
                            | crate::formats::scenery_kind::ELEVATOR
                            | crate::formats::scenery_kind::LADDER_BOTTOM
                            | crate::formats::scenery_kind::LADDER_TOP,
                        ) => "escalier ou échelle".to_string(),
                        _ => continue,
                    }
                } else {
                    continue;
                };
                out.push(issue(
                    IssueKind::Unreachable,
                    Some(elevation as usize),
                    Some(i),
                    Some(hex),
                    what,
                ));
            }
        }
    }

    out.sort_by_key(|i| {
        (
            i.elevation,
            IssueKind::ALL.iter().position(|k| *k == i.kind),
            i.index,
        )
    });
    out
}

impl IssueKind {
    /// Le vérificateur sait corriger ce genre de problème sans décision de l'utilisateur.
    pub fn fixable(self) -> bool {
        !matches!(
            self,
            IssueKind::ExitMissingMap | IssueKind::MissingArt | IssueKind::Unreachable
        )
    }

    /// Ce que fait la correction (français ; traduit par l'interface).
    pub fn fix_label(self) -> &'static str {
        match self {
            IssueKind::OutOfGrid | IssueKind::Duplicate | IssueKind::UnknownPrototype => {
                "Supprimer l'objet"
            }
            IssueKind::ScriptMissing => "Détacher le script",
            IssueKind::BadStart => "Replacer le départ",
            IssueKind::ExitMissingMap | IssueKind::MissingArt | IssueKind::Unreachable => "",
        }
    }
}

/// Bilan d'une correction.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FixReport {
    pub removed: usize,
    pub scripts_detached: usize,
    pub start_moved: bool,
}

impl FixReport {
    pub fn total(&self) -> usize {
        self.removed + self.scripts_detached + usize::from(self.start_moved)
    }
}

/// Premier hexagone libre (sans aucun objet) en partant du centre de la carte, de plus en plus loin : sert à replacer le départ.
fn free_hex(map: &Map, elevation: usize) -> u32 {
    let occupied: HashSet<i32> = map
        .objects
        .get(elevation)
        .into_iter()
        .flatten()
        .map(MapObject::tile)
        .collect();
    let center = (geom::HEX_GRID_WIDTH / 2) * geom::HEX_GRID_WIDTH + geom::HEX_GRID_WIDTH / 2;
    (0..geom::HEX_COUNT)
        .flat_map(|d| {
            [
                center.saturating_add(d).min(geom::HEX_COUNT - 1),
                center.saturating_sub(d),
            ]
        })
        .find(|h| !occupied.contains(&(*h as i32)))
        .unwrap_or(center)
}

/// Corrige les problèmes donnés : supprime les objets fautifs (et leurs scripts), détache les scripts introuvables, replace le
/// départ du joueur sur un hexagone libre. Les problèmes sans correction automatique (sortie, image) sont ignorés. Les indices
/// des problèmes doivent venir de `check_map` sur cette même carte, non modifiée entre-temps.
pub fn fix_issues(map: &mut Map, issues: &[Issue]) -> FixReport {
    let mut report = FixReport::default();
    // 1. Scripts : objets d'abord (les indices ne bougent pas), puis scripts spatiaux et script de la carte.
    for i in issues.iter().filter(|i| i.kind == IssueKind::ScriptMissing) {
        match (i.elevation, i.index, i.script) {
            (Some(e), Some(idx), _) => {
                let Some(o) = map.objects.get(e).and_then(|l| l.get(idx)) else {
                    continue;
                };
                if o.words[word::SCRIPT_ID] != -1 {
                    let had_entry = map.find_script(o.words[word::SCRIPT_ID]).is_some();
                    if map.detach_object_script(e, idx) || had_entry {
                        report.scripts_detached += 1;
                    }
                }
            }
            (_, None, Some(pid)) => report.scripts_detached += usize::from(map.remove_script(pid)),
            (None, None, None) if map.map_script_index().is_some() => {
                map.set_map_script(None);
                report.scripts_detached += 1;
            }
            _ => {}
        }
    }
    // 2. Objets à supprimer, du dernier au premier dans chaque élévation ; leur script part avec eux.
    let mut doomed: Vec<(usize, usize)> = issues
        .iter()
        .filter(|i| {
            matches!(
                i.kind,
                IssueKind::OutOfGrid | IssueKind::Duplicate | IssueKind::UnknownPrototype
            )
        })
        .filter_map(|i| Some((i.elevation?, i.index?)))
        .collect();
    doomed.sort_unstable_by(|a, b| b.cmp(a));
    doomed.dedup();
    for (e, idx) in doomed {
        let Some(sid) = map
            .objects
            .get(e)
            .and_then(|l| l.get(idx))
            .map(|o| o.words[word::SCRIPT_ID])
        else {
            continue;
        };
        if sid != -1 && sid != 0 {
            map.remove_script(sid as u32);
        }
        report.removed += usize::from(map.remove_object(e, idx).is_some());
    }
    // 3. Point de départ.
    if issues.iter().any(|i| i.kind == IssueKind::BadStart) {
        let current = map.header.player_elevation;
        let elevation = if (0..3).contains(&current) && map.header.has_elevation(current as usize) {
            current as usize
        } else {
            0
        };
        let hex = free_hex(map, elevation);
        map.set_player_start(hex, elevation);
        report.start_moved = true;
    }
    report
}

/// Nombre de problèmes de chaque nature.
pub fn count_by_kind(issues: &[Issue]) -> Vec<(IssueKind, usize)> {
    IssueKind::ALL
        .iter()
        .map(|&k| (k, issues.iter().filter(|i| i.kind == k).count()))
        .filter(|(_, n)| *n > 0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall_proto(n: u32) -> crate::formats::ProtoHeader {
        crate::formats::ProtoHeader {
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

    #[test]
    fn fixing_removes_bad_objects_detaches_scripts_and_moves_the_start() {
        let mut m = Map::new_empty("t", 1);
        for tile in [50, 51, 52, 53] {
            let id = m.max_object_id() + 1;
            m.add_object(0, MapObject::from_proto(&wall_proto(1), tile, id).unwrap());
        }
        m.objects[0][1].words[word::TILE] = -9; // hors grille
        m.header.player_tile = 52; // sur un mur
        assert!(m.attach_object_script(0, 3, 5));
        let scripts_before = m.scripts.iter().map(|l| l.count).sum::<u32>();
        let issues = vec![
            issue(IssueKind::OutOfGrid, Some(0), Some(1), None, ""),
            issue(
                IssueKind::ScriptMissing,
                Some(0),
                Some(3),
                Some(53),
                "x.int",
            ),
            issue(IssueKind::BadStart, Some(0), Some(2), Some(52), ""),
            issue(
                IssueKind::ExitMissingMap,
                Some(0),
                Some(0),
                Some(50),
                "carte n° 99",
            ),
        ];
        let r = fix_issues(&mut m, &issues);
        assert_eq!((r.removed, r.scripts_detached, r.start_moved), (1, 1, true));
        assert_eq!(
            m.objects[0].len(),
            3,
            "l'objet hors grille est supprimé, la sortie ignorée"
        );
        assert_eq!(
            m.objects[0][2].words[word::SCRIPT_ID],
            -1,
            "script détaché (l'objet a changé d'indice)"
        );
        assert!(m.scripts.iter().map(|l| l.count).sum::<u32>() < scripts_before);
        assert!(
            m.objects[0]
                .iter()
                .all(|o| o.tile() != m.header.player_tile),
            "le départ est sur un hexagone libre"
        );
        assert_eq!(r.total(), 3);
    }

    #[test]
    fn removing_several_objects_uses_the_original_indices() {
        let mut m = Map::new_empty("t", 1);
        for tile in [10, 11, 12, 13, 14] {
            let id = m.max_object_id() + 1;
            m.add_object(0, MapObject::from_proto(&wall_proto(1), tile, id).unwrap());
        }
        let issues = vec![
            issue(IssueKind::Duplicate, Some(0), Some(1), Some(11), ""),
            issue(IssueKind::Duplicate, Some(0), Some(3), Some(13), ""),
            issue(IssueKind::Duplicate, Some(0), Some(3), Some(13), ""),
        ];
        let r = fix_issues(&mut m, &issues);
        assert_eq!(
            r.removed, 2,
            "un objet signalé deux fois n'est supprimé qu'une fois"
        );
        assert_eq!(
            m.objects[0].iter().map(|o| o.tile()).collect::<Vec<_>>(),
            vec![10, 12, 14]
        );
        assert!(
            IssueKind::ALL
                .iter()
                .filter(|k| k.fixable())
                .all(|k| !k.fix_label().is_empty())
        );
    }

    #[test]
    fn counting_groups_by_kind_in_a_stable_order() {
        let issues = vec![
            issue(IssueKind::Duplicate, Some(0), Some(1), Some(5), ""),
            issue(IssueKind::OutOfGrid, Some(0), Some(0), None, ""),
            issue(IssueKind::Duplicate, Some(0), Some(2), Some(5), ""),
        ];
        assert_eq!(
            count_by_kind(&issues),
            vec![(IssueKind::OutOfGrid, 1), (IssueKind::Duplicate, 2)]
        );
        assert!(IssueKind::ALL.iter().all(|k| !k.label().is_empty()));
    }
}
