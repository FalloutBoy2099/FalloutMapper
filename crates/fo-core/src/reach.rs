//! Accessibilité : quels hexagones bloquent le passage (murs, décors, créatures) et lesquels le joueur peut atteindre depuis son
//! point de départ. Sert à la vue « Accessibilité » et au vérificateur (sortie, porte ou escalier hors d'atteinte).

use crate::formats::{ObjectType, scenery_kind};
use crate::game::GameData;
use crate::geom;
use crate::map::{Map, flags};
use std::collections::VecDeque;

/// Ce qui occupe un hexagone, vu de la marche.
#[derive(Debug, Clone, Default)]
pub struct Walk {
    /// Un mur ou un décor fixe interdit le passage.
    pub blocked: Vec<bool>,
    /// Une créature occupe l'hexagone (elle bouge : cela ne compte pas pour l'accès).
    pub critters: Vec<bool>,
}

/// Un objet vu par `walk_from`.
#[derive(Debug, Clone, Copy)]
pub struct Occupant {
    pub hex: u32,
    pub ty: ObjectType,
    pub flags: u32,
    /// Porte : elle s'ouvre, elle ne ferme pas le passage.
    pub door: bool,
}

/// Hexagones occupés d'après la liste d'objets : murs et décors (sauf portes) sans « ne bloque pas » ; créatures à part.
/// Les objets masqués, les objets et les objets divers (sorties, repères) ne bloquent pas.
pub fn walk_from(objects: impl IntoIterator<Item = Occupant>) -> Walk {
    let n = geom::HEX_COUNT as usize;
    let mut w = Walk {
        blocked: vec![false; n],
        critters: vec![false; n],
    };
    for o in objects {
        if o.hex >= geom::HEX_COUNT || o.flags & (flags::HIDDEN | flags::NO_BLOCK) != 0 {
            continue;
        }
        match o.ty {
            ObjectType::Wall => w.blocked[o.hex as usize] = true,
            ObjectType::Scenery if !o.door => w.blocked[o.hex as usize] = true,
            ObjectType::Critter => w.critters[o.hex as usize] = true,
            _ => {}
        }
    }
    w
}

/// Hexagones occupés d'une élévation de la carte.
pub fn walk_map(map: &Map, elevation: usize, game: &GameData) -> Walk {
    walk_from(
        map.objects
            .get(elevation)
            .into_iter()
            .flatten()
            .filter_map(|o| {
                let ty = o.object_type()?;
                let hex = (0..geom::HEX_COUNT as i32)
                    .contains(&o.tile())
                    .then(|| o.tile() as u32)?;
                let door = ty == ObjectType::Scenery
                    && game.proto_subtype(o.pid()) == Some(scenery_kind::DOOR);
                Some(Occupant {
                    hex,
                    ty,
                    flags: o.flags(),
                    door,
                })
            }),
    )
}

/// Hexagones atteignables en marchant depuis `start` (pas de six voisins, sans traverser un hexagone bloqué).
/// Vide si le départ est hors grille ou bloqué.
pub fn reachable(blocked: &[bool], start: u32) -> Vec<bool> {
    let mut seen = vec![false; blocked.len()];
    if start as usize >= blocked.len() || blocked[start as usize] {
        return seen;
    }
    seen[start as usize] = true;
    let mut queue = VecDeque::from([start]);
    while let Some(h) = queue.pop_front() {
        for n in geom::hex_neighbors(h) {
            if !seen[n as usize] && !blocked[n as usize] {
                seen[n as usize] = true;
                queue.push_back(n);
            }
        }
    }
    seen
}

/// Un hexagone ou l'un de ses voisins est atteint : on peut marcher jusqu'à lui (une porte ou un décor bloquant se touche de côté).
pub fn touched(seen: &[bool], hex: u32) -> bool {
    seen.get(hex as usize).copied().unwrap_or(false)
        || geom::hex_neighbors(hex).iter().any(|n| seen[*n as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn occ(hex: u32, ty: ObjectType) -> Occupant {
        Occupant {
            hex,
            ty,
            flags: 0,
            door: false,
        }
    }

    /// Anneau de murs à la distance 3 autour de `center`.
    fn ring(center: u32) -> Vec<Occupant> {
        let (c, r) = geom::hex_col_row(center);
        let mut v = Vec::new();
        for row in r.saturating_sub(5)..=r + 5 {
            for col in c.saturating_sub(5)..=c + 5 {
                let h = row * geom::HEX_GRID_WIDTH + col;
                if geom::hex_distance(center, h) == 3 {
                    v.push(occ(h, ObjectType::Wall));
                }
            }
        }
        v
    }

    #[test]
    fn walls_and_scenery_block_but_doors_items_and_flagged_objects_do_not() {
        let w = walk_from([
            occ(1000, ObjectType::Wall),
            occ(1001, ObjectType::Scenery),
            Occupant {
                door: true,
                ..occ(1002, ObjectType::Scenery)
            },
            occ(1003, ObjectType::Item),
            Occupant {
                flags: flags::NO_BLOCK,
                ..occ(1004, ObjectType::Wall)
            },
            Occupant {
                flags: flags::HIDDEN,
                ..occ(1005, ObjectType::Wall)
            },
            occ(1006, ObjectType::Misc),
            occ(1007, ObjectType::Critter),
            occ(u32::MAX, ObjectType::Wall),
        ]);
        assert!(w.blocked[1000] && w.blocked[1001]);
        assert!(
            !w.blocked[1002]
                && !w.blocked[1003]
                && !w.blocked[1004]
                && !w.blocked[1005]
                && !w.blocked[1006]
        );
        assert!(w.critters[1007] && !w.blocked[1007]);
    }

    #[test]
    fn a_closed_ring_keeps_the_inside_apart_and_a_gap_opens_it() {
        let center = 100 * 200 + 100;
        let mut walls = ring(center);
        let w = walk_from(walls.clone());
        let inside = reachable(&w.blocked, center);
        assert!(
            inside[center as usize] && inside.iter().filter(|b| **b).count() == 1 + 6 + 12,
            "disque de rayon 2 : {}",
            inside.iter().filter(|b| **b).count()
        );
        assert!(
            !inside[100 * 200 + 110] && !touched(&inside, 100 * 200 + 110),
            "dehors : inaccessible"
        );
        assert!(
            touched(&inside, walls[0].hex),
            "un mur du bord se touche depuis l'intérieur"
        );
        // Une brèche dans l'anneau : tout est accessible.
        walls.pop();
        let open = reachable(&walk_from(walls).blocked, center);
        assert!(open[100 * 200 + 110]);
    }

    #[test]
    fn blocked_or_invalid_start_reaches_nothing() {
        let w = walk_from([occ(500, ObjectType::Wall)]);
        assert!(!reachable(&w.blocked, 500).iter().any(|b| *b));
        assert!(!reachable(&w.blocked, u32::MAX).iter().any(|b| *b));
    }
}
