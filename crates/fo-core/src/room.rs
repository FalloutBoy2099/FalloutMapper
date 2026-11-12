//! Génération d'une pièce : les enchaînements de pièces de mur observés dans toutes les cartes servent à construire les
//! deux murs du fond (en L, comme dans le jeu : le mur de gauche suit les colonnes, celui de droite suit les lignes).
//!
//! Une pièce de mur couvre un ou deux hexagones : dans les cartes, la pièce suivante est à 1 ou 2 hexagones. Les pièces d'un
//! même mur alternent selon la parité de leur position ; chaque enchaînement apprend donc le pas (1 ou 2), la parité de la
//! pièce de départ et la pièce suivante.
//!
//! Un ensemble (« style ») regroupe les pièces qui se touchent dans les cartes et partagent le même préfixe de nom d'art.

use crate::formats::ObjectType;
use crate::geom;
use crate::map::Map;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

/// Nombre minimal d'observations pour qu'un enchaînement soit « sûr ».
const MIN_COUNT: u32 = 2;
/// Plus grand écart (en hexagones) entre deux pièces consécutives.
const MAX_STEP: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Axis {
    /// Même ligne, colonne suivante : le mur de gauche.
    Col,
    /// Même colonne, ligne suivante : le mur de droite.
    Row,
}

/// Clé d'un voisinage : pas, parité de la position de la pièce, pièce, pièce suivante.
type Key = (u8, u8, u32, u32);
/// Transitions candidates : (axe, parité, départ) → (pas, arrivée, observations).
type EdgeTable = HashMap<(Axis, u8, u32), Vec<(u8, u32, u32)>>;

/// Objet de décor (porte, mobilier) vu près d'un mur : mur voisin, axe du mur (0 colonnes, 1 lignes), parité de la position du
/// mur (colonne × 2 + ligne), écart en lignes puis en colonnes depuis le mur, FID et orientation de l'objet, porte ou non.
type DecorKey = (u32, u8, u8, i8, i8, u32, u8, bool);

/// Clé d'un décor une fois regroupé par style : axe, parité, écarts, FID, orientation, porte.
type MergedDecorKey = (u32, u8, u8, i8, i8, u32, u8, bool);

/// Distance maximale (en hexagones) entre un objet de décor et le mur qui lui sert de repère.
const DECOR_REACH: i32 = 3;

/// Voisinages de pièces de mur observés dans les cartes.
#[derive(Debug, Default, Clone)]
pub struct WallStats {
    floors: crate::room_floors::RoomFloors,
    col: HashMap<Key, u32>,
    row: HashMap<Key, u32>,
    total: HashMap<u32, u32>,
    /// Identifiant de prototype vu pour chaque pièce (FID).
    pids: HashMap<u32, u32>,
    decor: HashMap<DecorKey, u32>,
    decor_pids: HashMap<u32, u32>,
    doorways: HashMap<Doorway, u32>,
    /// Pièces d'angle vues dans les cartes : (sorte, parité de la colonne, FID) → nombre. La sorte dit où sont les deux voisines :
    /// 3 = plus loin en colonne et en ligne (angle du haut), 1 = moins loin en colonne et plus loin en ligne, 2 = l'inverse, 0 = les deux en arrière.
    corners: HashMap<(u8, u8, u32), u32>,
    /// Pièces de mur qui laissent passer la lumière : (FID, axe 0 colonnes / 1 lignes, parité de la position) → nombre.
    see_thru: HashMap<(u32, u8, u8), u32>,
    /// Décors qui émettent de la lumière : FID → (nombre, prototype).
    lights: HashMap<u32, (u32, u32)>,
    /// Meubles vus à moins de 6 hexagones l'un de l'autre près d'un mur : (FID, FID) triés → nombre.
    pairs: HashMap<(u32, u32), u32>,
    /// Nombre de fois où chaque meuble a été vu près d'un mur, et total.
    furniture_count: HashMap<u32, u32>,
    furniture_total: u32,
    /// Escaliers vus : (monte ?, FID, PID) → nombre. Le sens vient de l'élévation d'arrivée codée dans l'objet.
    stairs: HashMap<(bool, u32, u32), u32>,
    /// Écart (en pixels) entre une source lumineuse et la plus proche du même FID dans la même carte, par FID.
    light_gaps: HashMap<u32, Vec<u16>>,
    /// Réglages de lumière vus : (FID, rayon, intensité) → nombre.
    light_params: HashMap<(u32, i32, i32), u32>,
}

/// Nombre maximal d'écarts mémorisés par type de lumière.
const MAX_GAPS: usize = 600;

/// Vérifie le segment entre deux meubles ; les trous d'un hexagone entre deux
/// panneaux sont une cloison aussi, pas un passage vers la pièce voisine.
fn decor_same_room(a: (i32, i32), b: (i32, i32), walls: &HashMap<(u32, u32), Vec<u32>>) -> bool {
    let steps = (a.0 - b.0).abs().max((a.1 - b.1).abs()) * 2;
    let has = |c: i32, r: i32| c >= 0 && r >= 0 && walls.contains_key(&(c as u32, r as u32));
    (1..steps).all(|i| {
        let c = a.0 + ((b.0 - a.0) as f32 * i as f32 / steps as f32).round() as i32;
        let r = a.1 + ((b.1 - a.1) as f32 * i as f32 / steps as f32).round() as i32;
        !has(c, r) && !(has(c - 1, r) && has(c + 1, r)) && !(has(c, r - 1) && has(c, r + 1))
    })
}

impl WallStats {
    pub fn add_map(&mut self, map: &Map) {
        self.floors.add_map(map);
        for (elevation, objects) in map.objects.iter().enumerate() {
            self.add_stairs(elevation, objects);
            let mut at: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
            let mut lit: Vec<((u32, u32), u32)> = Vec::new();
            for o in objects {
                // Les bloqueurs invisibles du jeu sont des murs FLAT (pas HIDDEN).
                // Ils remplissent notamment l'hexagone sous les angles et faussent
                // les voisins, les jonctions et les pas appris pour les vrais murs.
                if o.object_type() != Some(ObjectType::Wall)
                    || o.tile() < 0
                    || o.flags() & (crate::map::flags::HIDDEN | crate::map::flags::FLAT) != 0
                {
                    continue;
                }
                at.entry(geom::hex_col_row(o.tile() as u32))
                    .or_default()
                    .push(o.fid());
                if o.has_flag(crate::map::flags::LIGHT_THRU) {
                    lit.push((geom::hex_col_row(o.tile() as u32), o.fid()));
                }
                *self.total.entry(o.fid()).or_default() += 1;
                self.pids.entry(o.fid()).or_insert(o.pid());
            }
            for (&(c, r), fids) in &at {
                for step in 1..=MAX_STEP {
                    for (axis, next, par) in [
                        (Axis::Col, (c + step, r), (c % 2) as u8),
                        (Axis::Row, (c, r + step), (r % 2) as u8),
                    ] {
                        if next.0 >= geom::HEX_GRID_WIDTH || next.1 >= geom::HEX_GRID_WIDTH {
                            continue;
                        }
                        let Some(others) = at.get(&next) else {
                            continue;
                        };
                        // Ne pas apprendre A→C à travers la pièce B de A,B,C.
                        // Ce faux pas de deux faisait sauter une pièce après les angles.
                        let between = if axis == Axis::Col {
                            (c + 1, r)
                        } else {
                            (c, r + 1)
                        };
                        if step > 1 && at.contains_key(&between) {
                            continue;
                        }
                        let table = if axis == Axis::Col {
                            &mut self.col
                        } else {
                            &mut self.row
                        };
                        for &a in fids {
                            for &b in others {
                                *table.entry((step as u8, par, a, b)).or_default() += 1;
                                // Translater un mur le long des lignes ne change
                                // pas son dessin : seule la parité des colonnes
                                // impose des pièces de largeurs différentes.
                                if axis == Axis::Row {
                                    *table.entry((step as u8, 1 - par, a, b)).or_default() += 1;
                                }
                            }
                        }
                    }
                }
            }
            for &((c, r), fid) in &lit {
                let has = |dc: i32, dr: i32| {
                    let (nc, nr) = (c as i32 + dc, r as i32 + dr);
                    nc >= 0 && nr >= 0 && at.contains_key(&(nc as u32, nr as u32))
                };
                let col = [-2, -1, 1, 2].iter().any(|&d| has(d, 0));
                let row = [-2, -1, 1, 2].iter().any(|&d| has(0, d));
                let (axis, par) = if col {
                    (0u8, (c % 2) as u8)
                } else if row {
                    (1u8, (r % 2) as u8)
                } else {
                    continue;
                };
                *self.see_thru.entry((fid, axis, par)).or_default() += 1;
            }
            for (&(c, r), fids) in &at {
                let has = |dc: i32, dr: i32| {
                    let (nc, nr) = (c as i32 + dc, r as i32 + dr);
                    nc >= 0 && nr >= 0 && at.contains_key(&(nc as u32, nr as u32))
                };
                let (cp, cm) = (
                    [1, 2].iter().any(|&d| has(d, 0)),
                    [1, 2].iter().any(|&d| has(-d, 0)),
                );
                let (rp, rm) = (
                    [1, 2].iter().any(|&d| has(0, d)),
                    [1, 2].iter().any(|&d| has(0, -d)),
                );
                // Un angle (sortes 0 à 3) : une voisine d'un seul côté le long des colonnes et d'un seul côté le long des lignes. Une jonction en T
                // (4 à 7) : deux côtés sur un axe, un seul sur l'autre ; un croisement (8) : les quatre.
                let kind = if cp != cm && rp != rm {
                    Some(u8::from(cp) * 2 + u8::from(rp))
                } else if cp && cm && rp != rm {
                    Some(4 + u8::from(rp))
                } else if rp && rm && cp != cm {
                    Some(6 + u8::from(cp))
                } else if cp && cm && rp && rm {
                    Some(8)
                } else {
                    None
                };
                if let Some(kind) = kind {
                    for &f in fids {
                        *self.corners.entry((kind, (c % 2) as u8, f)).or_default() += 1;
                    }
                }
            }
            self.add_decor(objects, &at);
            self.add_doorways(objects, &at);
            self.add_lights(objects);
        }
    }

    /// Une ouverture est un assemblage de jambages et de panneaux, pas une porte
    /// posée à la place d'une pièce quelconque. Garder aussi les deux extrémités
    /// du segment permet de le raccorder au mur construit sans trou ni recouvrement.
    fn add_doorways(
        &mut self,
        objects: &[crate::map::MapObject],
        at: &HashMap<(u32, u32), Vec<u32>>,
    ) {
        use crate::map::flags;
        for o in objects.iter().filter(|o| {
            o.object_type() == Some(ObjectType::Scenery)
                && o.subtype == crate::formats::scenery_kind::DOOR
                && o.tile() >= 0
                && o.frame() == 0
                && !o.has_flag(flags::HIDDEN)
        }) {
            let (c, r) = geom::hex_col_row(o.tile() as u32);
            for axis in [Axis::Col, Axis::Row] {
                for reach in 2..=4i32 {
                    let mut walls = Vec::new();
                    for off in -reach..=reach {
                        let (nc, nr) = if axis == Axis::Col {
                            (c as i32 + off, r as i32)
                        } else {
                            (c as i32, r as i32 + off)
                        };
                        if nc < 0 || nr < 0 {
                            continue;
                        }
                        if let Some(fids) = at.get(&(nc as u32, nr as u32)) {
                            if fids.len() != 1 {
                                walls.clear();
                                break;
                            }
                            walls.push((off as i8, fids[0]));
                        }
                    }
                    let (Some(first), Some(last)) = (walls.first(), walls.last()) else {
                        continue;
                    };
                    if first.0 >= 0 || last.0 <= 0 || walls.iter().any(|p| p.0 == 0) {
                        continue;
                    }
                    // Ne pas capturer une seconde ouverture sans sa porte.
                    let another = objects.iter().any(|other| {
                        other.tile() != o.tile()
                            && other.tile() >= 0
                            && other.object_type() == Some(ObjectType::Scenery)
                            && other.subtype == crate::formats::scenery_kind::DOOR
                            && {
                                let (oc, or) = geom::hex_col_row(other.tile() as u32);
                                let off = if axis == Axis::Col {
                                    oc as i32 - c as i32
                                } else {
                                    or as i32 - r as i32
                                };
                                (if axis == Axis::Col { or == r } else { oc == c })
                                    && off >= first.0 as i32
                                    && off <= last.0 as i32
                            }
                    });
                    if another {
                        continue;
                    }
                    let mut frame_walls = Vec::new();
                    for dc in -2..=2i32 {
                        for dr in -2..=2i32 {
                            if (axis == Axis::Col && dr == 0)
                                || (axis == Axis::Row && dc == 0)
                                || c as i32 + dc < 0
                                || r as i32 + dr < 0
                            {
                                continue;
                            }
                            if let Some(fids) =
                                at.get(&((c as i32 + dc) as u32, (r as i32 + dr) as u32))
                            {
                                for &fid in fids {
                                    frame_walls.push((dc as i8, dr as i8, fid));
                                }
                            }
                        }
                    }
                    let doorway = Doorway {
                        axis,
                        col_par: (c % 2) as u8,
                        fid: o.fid(),
                        orient: o.orientation().clamp(0, 5) as u8,
                        offset: o.offset(),
                        flags: o.flags() & !0x4000_0000,
                        walls,
                        frame_walls,
                    };
                    *self.doorways.entry(doorway).or_default() += 1;
                    self.decor_pids.entry(o.fid()).or_insert(o.pid());
                }
            }
        }
    }

    /// Escaliers entre élévations : on retient le décor le plus courant pour monter et celui pour descendre.
    fn add_stairs(&mut self, elevation: usize, objects: &[crate::map::MapObject]) {
        for o in objects {
            if o.object_type() != Some(ObjectType::Scenery)
                || o.subtype != crate::formats::scenery_kind::STAIRS
                || o.tile() < 0
            {
                continue;
            }
            let Some(&dest) = o.extra.first().filter(|d| **d != -1) else {
                continue;
            };
            let target = (dest as u32 >> 29) as usize;
            if target == elevation || target > 2 {
                continue;
            }
            *self
                .stairs
                .entry((target > elevation, o.fid(), o.pid()))
                .or_default() += 1;
        }
    }

    /// Sources lumineuses (décors qui éclairent) : combien de chaque sorte, et l'écart habituel entre deux sources voisines.
    fn add_lights(&mut self, objects: &[crate::map::MapObject]) {
        use crate::map::word;
        let mut by_fid: HashMap<u32, Vec<(i32, i32)>> = HashMap::new();
        for o in objects {
            if o.object_type() != Some(ObjectType::Scenery)
                || o.tile() < 0
                || o.words[word::LIGHT_DISTANCE] <= 0
                || o.words[word::LIGHT_INTENSITY] <= 0
            {
                continue;
            }
            let e = self.lights.entry(o.fid()).or_insert((0, o.pid()));
            e.0 += 1;
            *self
                .light_params
                .entry((
                    o.fid(),
                    o.words[word::LIGHT_DISTANCE],
                    o.words[word::LIGHT_INTENSITY],
                ))
                .or_default() += 1;
            by_fid
                .entry(o.fid())
                .or_default()
                .push(geom::hex_to_screen(o.tile() as u32));
        }
        for (fid, pts) in by_fid {
            if pts.len() < 2 {
                continue;
            }
            let gaps = self.light_gaps.entry(fid).or_default();
            for (i, a) in pts.iter().enumerate() {
                let nearest = pts
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, b)| (((a.0 - b.0).pow(2) + (a.1 - b.1).pow(2)) as f64).sqrt())
                    .fold(f64::MAX, f64::min);
                if gaps.len() < MAX_GAPS && nearest < 400.0 {
                    gaps.push(nearest as u16);
                }
            }
        }
    }

    /// Objets de décor (portes, décors, objets) vus près d'un mur : on retient, pour chacun, le mur le plus proche et sa
    /// position relative, côté intérieur de la pièce seulement (les portes peuvent aussi être sur la ligne du mur).
    fn add_decor(&mut self, objects: &[crate::map::MapObject], at: &HashMap<(u32, u32), Vec<u32>>) {
        // Meubles retenus dans cette carte, pour compter ceux qui vont ensemble.
        let mut placed: Vec<(u32, i32, i32)> = Vec::new();
        let has_wall = |c: i32, r: i32| c >= 0 && r >= 0 && at.contains_key(&(c as u32, r as u32));
        for o in objects {
            let ty = o.object_type();
            if o.tile() < 0
                || o.has_flag(crate::map::flags::HIDDEN)
                || !matches!(ty, Some(ObjectType::Scenery) | Some(ObjectType::Item))
            {
                continue;
            }
            // Un objet ramassable n'est pas un meuble (armes, médicaments, clés…).
            if ty == Some(ObjectType::Item) && o.subtype != crate::formats::item_kind::CONTAINER {
                continue;
            }
            // Escaliers, ascenseurs et échelles ne sont pas du mobilier : ils mènent ailleurs (ils sont appris à part).
            if ty == Some(ObjectType::Scenery)
                && !matches!(
                    o.subtype,
                    crate::formats::scenery_kind::DOOR | crate::formats::scenery_kind::GENERIC
                )
            {
                continue;
            }
            let door = ty == Some(ObjectType::Scenery) && o.subtype == 0;
            // Les luminaires ont leur propre apprentissage et option de placement.
            if !door
                && o.words[crate::map::word::LIGHT_DISTANCE] > 0
                && o.words[crate::map::word::LIGHT_INTENSITY] > 0
            {
                continue;
            }
            let (c, r) = geom::hex_col_row(o.tile() as u32);
            let (c, r) = (c as i32, r as i32);
            // Mur le plus proche (distance max, puis plus petit FID).
            let mut near: Option<(i32, u32, i32, i32)> = None;
            for dr in -DECOR_REACH..=DECOR_REACH {
                for dc in -DECOR_REACH..=DECOR_REACH {
                    let Some(fids) = (c + dc >= 0 && r + dr >= 0)
                        .then(|| at.get(&((c + dc) as u32, (r + dr) as u32)))
                        .flatten()
                    else {
                        continue;
                    };
                    let dist = dr.abs().max(dc.abs());
                    for &w in fids {
                        if near.is_none_or(|(d, f, _, _)| (dist, w) < (d, f)) {
                            near = Some((dist, w, c + dc, r + dr));
                        }
                    }
                }
            }
            let Some((_, wall, cw, rw)) = near else {
                continue;
            };
            let (dc, dr) = (c - cw, r - rw);
            let col_wall = [-2, -1, 1, 2].iter().any(|d| has_wall(cw + d, rw));
            let row_wall = [-2, -1, 1, 2].iter().any(|d| has_wall(cw, rw + d));
            let min_inside = if door { 0 } else { 1 };
            let axis = if col_wall && dr >= min_inside && dr <= DECOR_REACH {
                0u8
            } else if row_wall && dc >= min_inside && dc <= DECOR_REACH {
                1u8
            } else {
                continue;
            };
            let par = ((cw % 2) * 2 + (rw % 2)) as u8;
            let key: DecorKey = (
                wall,
                axis,
                par,
                dr as i8,
                dc as i8,
                o.fid(),
                o.orientation().clamp(0, 5) as u8,
                door,
            );
            *self.decor.entry(key).or_default() += 1;
            self.decor_pids.entry(o.fid()).or_insert(o.pid());
            if !door && !o.has_flag(crate::map::flags::HIDDEN) {
                placed.push((o.fid(), c, r));
            }
        }
        // Meubles voisins (6 hexagones au plus dans chaque direction) : rangés par cases de 6 pour éviter les comparaisons inutiles.
        let mut cells: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (i, p) in placed.iter().enumerate() {
            cells.entry((p.1 / 6, p.2 / 6)).or_default().push(i);
            *self.furniture_count.entry(p.0).or_default() += 1;
            self.furniture_total += 1;
        }
        for (i, a) in placed.iter().enumerate() {
            for cx in (a.1 / 6 - 1).max(0)..=a.1 / 6 + 1 {
                for cy in (a.2 / 6 - 1).max(0)..=a.2 / 6 + 1 {
                    for &j in cells.get(&(cx, cy)).into_iter().flatten() {
                        let b = &placed[j];
                        if j > i
                            && a.0 != b.0
                            && (a.1 - b.1).abs() <= 6
                            && (a.2 - b.2).abs() <= 6
                            && decor_same_room((a.1, a.2), (b.1, b.2), at)
                        {
                            *self.pairs.entry((a.0.min(b.0), a.0.max(b.0))).or_default() += 1;
                        }
                    }
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.total.is_empty()
    }

    /// Modèle d'un style : seules les pièces pour lesquelles `in_style` est vrai sont considérées.
    pub fn model(&self, in_style: &dyn Fn(u32) -> bool) -> StyleModel {
        let strict = self.model_with(in_style, true);
        // Un style dont les pièces servent indifféremment sur les deux axes n'a pas d'enchaînement une fois le tri fait : on le garde tel quel.
        if strict.is_usable() {
            strict
        } else {
            self.model_with(in_style, false)
        }
    }

    fn model_with(&self, in_style: &dyn Fn(u32) -> bool, by_axis: bool) -> StyleModel {
        let mut m = StyleModel {
            floors: self.floors.for_style(in_style),
            roofs: self.floors.roofs_for_style(in_style),
            ..Default::default()
        };
        // Une pièce de mur sert surtout sur un axe (son dessin est de face ou de profil) : on ne la laisse pas enchaîner sur l'autre
        // à cause de quelques couples rares (deux murs parallèles, une cloison…), sans quoi un mur se compose de pièces tournées du
        // mauvais côté et laisse des trous.
        let axis_use = |fid: u32| -> (u32, u32) {
            let sum = |t: &HashMap<Key, u32>| {
                t.iter()
                    .filter(|(k, _)| k.2 == fid || k.3 == fid)
                    .map(|(_, n)| *n)
                    .sum::<u32>()
            };
            (sum(&self.col), sum(&self.row))
        };
        let fits = |axis: Axis, fid: u32| {
            let (c, r) = axis_use(fid);
            let (own, other) = if axis == Axis::Col { (c, r) } else { (r, c) };
            !by_axis || own * 3 >= other
        };
        for (axis, table) in [(Axis::Col, &self.col), (Axis::Row, &self.row)] {
            // Pour chaque (parité, pièce, pas) : la pièce suivante la plus fréquente ; puis le pas. Dans un mur continu, les
            // couples à 2 hexagones sont aussi nombreux que ceux à 1 : on garde le pas de 1 dès qu'il atteint la moitié de
            // l'effectif du pas de 2.
            let mut per_step: BTreeMap<(u8, u32, u8), (u32, u32)> = BTreeMap::new();
            for (&(step, par, a, b), &n) in table {
                if !in_style(a) || !in_style(b) || !fits(axis, a) || !fits(axis, b) {
                    continue;
                }
                let e = per_step.entry((par, a, step)).or_insert((n, b));
                if n > e.0 || (n == e.0 && b < e.1) {
                    *e = (n, b);
                }
            }
            let mut best: BTreeMap<(u8, u32), (u32, u8, u32)> = BTreeMap::new();
            for (&(par, a, step), &(n, b)) in &per_step {
                let cand = (n, step, b);
                match best.get(&(par, a)) {
                    None => {
                        best.insert((par, a), cand);
                    }
                    // Le pas plus court est déjà retenu (ordre du BTreeMap) : le grand pas ne le remplace que s'il est plus de deux fois
                    // plus fréquent.
                    Some(&(n0, _, _)) if n > n0 * 2 => {
                        best.insert((par, a), cand);
                    }
                    Some(_) => {}
                }
            }
            for (&(par, a), &(n, step, b)) in &best {
                if n >= MIN_COUNT {
                    m.strong.insert((axis, par, a), (step, b));
                    m.starts.entry((axis, par)).or_default().push((n, a));
                } else {
                    m.weak.insert((axis, par, a), (step, b));
                }
            }
        }
        for list in m.starts.values_mut() {
            list.sort_by_key(|&(n, a)| (std::cmp::Reverse(n), a));
        }
        m.pids = self
            .pids
            .iter()
            .filter(|(f, _)| in_style(**f))
            .map(|(&f, &p)| (f, p))
            .collect();
        // Pièce d'angle la plus courante par sorte et parité de colonne.
        let mut best_corner: HashMap<(u8, u8), (u32, u32)> = HashMap::new();
        for (&(kind, par, fid), &n) in &self.corners {
            // Une vraie pièce d'angle sert surtout d'angle : une pièce courante qui touche parfois un mur perpendiculaire n'en est pas une.
            let seen = self.total.get(&fid).copied().unwrap_or(1).max(1);
            let as_corner: u32 = self
                .corners
                .iter()
                .filter(|(k, _)| k.0 == kind && k.2 == fid)
                .map(|(_, c)| *c)
                .sum();
            if !in_style(fid) || as_corner == 0 {
                continue;
            }
            // La spécificité distingue un angle rare d'un mur droit très courant
            // qui touche accidentellement une cloison. Un angle peut aussi avoir
            // été classé comme jonction dans d'autres bâtiments.
            let score = (u64::from(n) * u64::from(n) * 1024 / u64::from(seen)) as u32;
            m.corner_choices
                .entry((kind, par))
                .or_default()
                .push((score, fid));
            let e = best_corner.entry((kind, par)).or_insert((score, fid));
            if score > e.0 || (score == e.0 && fid < e.1) {
                *e = (score, fid);
            }
        }
        m.corners = best_corner.into_iter().map(|(k, (_, f))| (k, f)).collect();
        for choices in m.corner_choices.values_mut() {
            choices.sort_by_key(|&(score, fid)| (std::cmp::Reverse(score), fid));
            choices.truncate(4);
        }
        // Garder les alternatives : la transition la plus fréquente n'est pas
        // nécessairement celle qui rejoint l'angle à la longueur demandée.
        for (axis, table) in [(Axis::Col, &self.col), (Axis::Row, &self.row)] {
            for (&(step, par, a, b), &count) in table {
                if in_style(a) && in_style(b) {
                    m.edges
                        .entry((axis, par, a))
                        .or_default()
                        .push((step, b, count));
                }
            }
        }
        for edges in m.edges.values_mut() {
            edges.sort_by_key(|&(step, fid, count)| (std::cmp::Reverse(count), step, fid));
        }
        // Voisine d'une pièce d'angle : le couple (pas, pièce) le plus courant, sans préférer le pas de 1 comme pour un mur continu (une pièce d'angle
        // couvre souvent deux hexagones, et la pièce qui la suit est alors à 2).
        let corner_fids: HashSet<u32> = m.corners.values().copied().collect();
        for (axis, table) in [(Axis::Col, &self.col), (Axis::Row, &self.row)] {
            let mut best: HashMap<(u8, u32), (u32, u8, u32)> = HashMap::new();
            for (&(step, par, a, b), &n) in table {
                if !corner_fids.contains(&a) || !in_style(a) || !in_style(b) {
                    continue;
                }
                let e = best.entry((par, a)).or_insert((n, step, b));
                if n > e.0 || (n == e.0 && (step, b) < (e.1, e.2)) {
                    *e = (n, step, b);
                }
            }
            for ((par, a), (_, step, b)) in best {
                m.corner_next.insert((axis, par, a), (step, b));
            }
        }
        let mut merged: BTreeMap<MergedDecorKey, u32> = BTreeMap::new();
        for (&(wall, axis, par, dr, dc, fid, orient, door), &n) in &self.decor {
            if in_style(wall) {
                *merged
                    .entry((wall, axis, par, dr, dc, fid, orient, door))
                    .or_default() += n;
            }
        }
        m.decor = merged
            .into_iter()
            .map(
                |((wall, axis, par, dr, dc, fid, orient, door), count)| Decor {
                    wall,
                    axis: if axis == 0 { Axis::Col } else { Axis::Row },
                    par,
                    dr,
                    dc,
                    fid,
                    orient,
                    door,
                    count,
                },
            )
            .collect();
        // Les plus fréquents d'abord ; on en garde assez pour varier sans alourdir.
        m.decor.sort_by_key(|d| (std::cmp::Reverse(d.count), d.fid));
        m.decor.truncate(600);
        // Certains jambages (ENCDOR…) appartiennent à une autre famille d'art.
        // Ne retenir hors du style que les pièces associées de façon répétée à
        // cette porte, et non les murs ordinaires des pièces voisines.
        let mut associated: HashMap<(u32, u32), u32> = HashMap::new();
        for (d, &count) in &self.doorways {
            let fids: HashSet<_> = d
                .walls
                .iter()
                .map(|p| p.1)
                .chain(d.frame_walls.iter().map(|p| p.2))
                .collect();
            for fid in fids {
                *associated.entry((d.fid, fid)).or_default() += count;
            }
        }
        let frame_for = |door, fid| {
            associated.get(&(door, fid)).is_some_and(|&n| {
                n >= MIN_COUNT && n * 4 >= self.total.get(&fid).copied().unwrap_or(u32::MAX)
            })
        };
        m.doorways = self
            .doorways
            .iter()
            .filter(|(d, _)| {
                d.walls.first().is_some_and(|p| in_style(p.1))
                    && d.walls.last().is_some_and(|p| in_style(p.1))
                    && d.walls
                        .iter()
                        .all(|p| in_style(p.1) || frame_for(d.fid, p.1))
            })
            .map(|(d, &n)| {
                let mut d = d.clone();
                d.frame_walls
                    .retain(|p| !in_style(p.2) && frame_for(d.fid, p.2));
                (d, n)
            })
            .collect();
        m.doorways.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (d, _) in &m.doorways {
            if let Some(&pid) = self.decor_pids.get(&d.fid) {
                m.pids.insert(d.fid, pid);
            }
            for fid in d
                .walls
                .iter()
                .map(|p| p.1)
                .chain(d.frame_walls.iter().map(|p| p.2))
            {
                if let Some(&pid) = self.pids.get(&fid) {
                    m.pids.insert(fid, pid);
                }
            }
        }
        for d in &m.decor {
            if let Some(&p) = self.decor_pids.get(&d.fid) {
                m.pids.insert(d.fid, p);
            }
        }
        // Types de pièce : autour de chaque meuble fréquent, ceux qu'on trouve à côté bien plus souvent que par hasard.
        {
            const MIN_PAIRS: u32 = 8;
            const MIN_LIFT: f64 = 10.0;
            const MIN_MEMBERS: usize = 2;
            let mut furniture: Vec<(u32, u32)> = m
                .decor
                .iter()
                .filter(|d| !d.door)
                .fold(HashMap::new(), |mut acc: HashMap<u32, u32>, d| {
                    *acc.entry(d.fid).or_default() += d.count;
                    acc
                })
                .into_iter()
                .collect();
            furniture.sort_by_key(|&(f, n)| (std::cmp::Reverse(n), f));
            let total = self.furniture_total.max(1) as f64;
            let mut taken: HashSet<u32> = HashSet::new();
            for &(seed, n) in &furniture {
                if taken.contains(&seed) {
                    continue;
                }
                let seed_count =
                    self.furniture_count.get(&seed).copied().unwrap_or(1).max(1) as f64;
                let mut members: Vec<u32> = vec![seed];
                for &(g, _) in &furniture {
                    if g == seed || taken.contains(&g) {
                        continue;
                    }
                    let g_count = self.furniture_count.get(&g).copied().unwrap_or(1).max(1) as f64;
                    // Un meuble « pivot » ne doit pas réunir deux ensembles qui ne
                    // coexistent jamais : chaque membre doit être compatible.
                    let fits = |f: &u32| {
                        let pair = self
                            .pairs
                            .get(&((*f).min(g), (*f).max(g)))
                            .copied()
                            .unwrap_or(0);
                        let count = if *f == seed {
                            seed_count
                        } else {
                            self.furniture_count.get(f).copied().unwrap_or(1).max(1) as f64
                        };
                        pair >= MIN_PAIRS
                            && f64::from(pair) * total / (count * g_count * 2.0) >= MIN_LIFT
                    };
                    if members.iter().all(fits) {
                        members.push(g);
                    }
                }
                if members.len() >= MIN_MEMBERS {
                    let count = members
                        .iter()
                        .map(|f| furniture.iter().find(|x| x.0 == *f).map_or(0, |x| x.1))
                        .sum();
                    taken.extend(members.iter().copied());
                    m.kinds.push(RoomKind {
                        seed,
                        members: Arc::new(members.into_iter().collect()),
                        count,
                    });
                } else {
                    let _ = n;
                }
            }
        }
        // Escaliers : les plus courants pour monter et pour descendre.
        let best = |up: bool| {
            self.stairs
                .iter()
                .filter(|((u, _, _), _)| *u == up)
                .max_by_key(|((_, f, p), n)| (**n, std::cmp::Reverse((*f, *p))))
                .map(|((_, f, p), _)| (*f, *p))
        };
        if let (Some((up_fid, up_pid)), Some((down_fid, down_pid))) = (best(true), best(false)) {
            m.stairs = Some(StairKinds {
                up_fid,
                up_pid,
                down_fid,
                down_pid,
            });
            m.pids.insert(up_fid, up_pid);
            m.pids.insert(down_fid, down_pid);
        }
        // Lumière : le décor lumineux le plus courant des cartes (hors style), et son écart habituel.
        const MIN_LIGHTS: u32 = 20;
        if let Some((&fid, &(n, pid))) = self
            .lights
            .iter()
            .max_by_key(|(f, (n, _))| (*n, std::cmp::Reverse(**f)))
            && n >= MIN_LIGHTS
        {
            let mut gaps = self.light_gaps.get(&fid).cloned().unwrap_or_default();
            gaps.sort_unstable();
            let median_px = gaps.get(gaps.len() / 2).copied().unwrap_or(110) as u32;
            // Un hexagone fait en moyenne une vingtaine de pixels de centre à centre.
            // Réglage le plus courant pour cette source (le prototype peut en prévoir un autre).
            let (distance, intensity) = self
                .light_params
                .iter()
                .filter(|((f, _, _), _)| *f == fid)
                .max_by_key(|((_, d, i), n)| (**n, std::cmp::Reverse((*d, *i))))
                .map_or((3, 65_536), |((_, d, i), _)| (*d, *i));
            m.light = Some(RoomLight {
                fid,
                spacing: (median_px / 22).clamp(3, 8),
                distance,
                intensity,
            });
            m.pids.insert(fid, pid);
        }
        // Fenêtres : pièces du style qui laissent presque toujours passer la lumière, dans un style qui est surtout plein
        // (dans une clôture, tout laisse passer la lumière : rien n'y est une fenêtre).
        let mut lit_total: HashMap<u32, u32> = HashMap::new();
        for (&(fid, _, _), &n) in &self.see_thru {
            *lit_total.entry(fid).or_default() += n;
        }
        let style_total: u32 = self
            .total
            .iter()
            .filter(|(f, _)| in_style(**f))
            .map(|(_, n)| *n)
            .sum();
        let style_lit: u32 = lit_total
            .iter()
            .filter(|(f, _)| in_style(**f))
            .map(|(_, n)| *n)
            .sum();
        if style_total > 0 && style_lit * 100 < style_total * 35 {
            for (&(fid, axis, par), &n) in &self.see_thru {
                let uses = self.total.get(&fid).copied().unwrap_or(0);
                if in_style(fid)
                    && n >= MIN_COUNT
                    && lit_total[&fid] * 10 >= uses * 6
                    && m.pids.contains_key(&fid)
                {
                    m.windows
                        .push((if axis == 0 { Axis::Col } else { Axis::Row }, par, fid, n));
                }
            }
            m.windows
                .sort_by_key(|w| (std::cmp::Reverse(w.3), w.2, w.1));
        }
        m
    }

    /// Ensembles de pièces, du plus employé au moins employé. Deux pièces sont dans le même ensemble quand elles se touchent
    /// dans les cartes (à `MIN_COUNT` reprises au moins) et appartiennent à la même famille d'art ; les préfixes des différentes
    /// fonctions d'un mur d'Abri sont regroupés. Un ensemble n'est conservé que
    /// s'il contient des enchaînements dans les deux sens. `names` est le contenu de `walls.lst`.
    pub fn sets(&self, names: &[String]) -> Vec<WallSet> {
        let prefix = |fid: u32| {
            names
                .get((fid & 0x00FF_FFFF) as usize)
                .map(|n| wall_family(n))
                .unwrap_or_default()
        };
        let ids: Vec<u32> = self
            .total
            .keys()
            .copied()
            .filter(|&f| !prefix(f).is_empty())
            .collect();
        let index: HashMap<u32, usize> = ids.iter().enumerate().map(|(i, &f)| (f, i)).collect();
        let mut parent: Vec<usize> = (0..ids.len()).collect();
        fn find(parent: &mut [usize], mut x: usize) -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        }
        let mut edges: Vec<(usize, usize, bool)> = Vec::new();
        for (is_col, table) in [(true, &self.col), (false, &self.row)] {
            for (&(_, _, a, b), &n) in table {
                let (Some(&ia), Some(&ib)) = (index.get(&a), index.get(&b)) else {
                    continue;
                };
                if n >= MIN_COUNT && prefix(a) == prefix(b) {
                    edges.push((ia, ib, is_col));
                }
            }
        }
        for &(a, b, _) in &edges {
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            if ra != rb {
                parent[rb] = ra;
            }
        }
        let mut axes: HashMap<usize, (bool, bool)> = HashMap::new();
        for &(a, _, is_col) in &edges {
            let e = axes.entry(find(&mut parent, a)).or_default();
            if is_col {
                e.0 = true;
            } else {
                e.1 = true;
            }
        }
        let mut groups: HashMap<usize, Vec<u32>> = HashMap::new();
        for (i, &f) in ids.iter().enumerate() {
            groups.entry(find(&mut parent, i)).or_default().push(f);
        }
        let mut out: Vec<WallSet> = groups
            .into_iter()
            .filter(|(root, _)| axes.get(root) == Some(&(true, true)))
            .map(|(_, members)| {
                let count: u32 = members.iter().map(|f| self.total[f]).sum();
                let rep = members
                    .iter()
                    .copied()
                    .max_by_key(|f| (self.total[f], std::cmp::Reverse(*f)))
                    .unwrap_or(0);
                let label = names
                    .get((rep & 0x00FF_FFFF) as usize)
                    .map(|n| {
                        n.trim_end_matches(".frm")
                            .trim_end_matches(".FRM")
                            .to_string()
                    })
                    .unwrap_or_else(|| format!("{rep:08X}"));
                WallSet {
                    rep,
                    label,
                    count,
                    members: Arc::new(members.into_iter().collect::<HashSet<u32>>()),
                }
            })
            .collect();
        out.sort_by_key(|s| (std::cmp::Reverse(s.count), s.label.clone()));
        out
    }
}

/// Ensemble de pièces de mur qui vont ensemble (même matière).
#[derive(Debug, Clone)]
pub struct WallSet {
    /// Pièce la plus employée.
    pub rep: u32,
    /// Nom d'art de la pièce la plus employée, sans extension.
    pub label: String,
    /// Nombre de pièces posées dans les cartes.
    pub count: u32,
    pub members: Arc<HashSet<u32>>,
}

/// Style d'un mur d'après son nom d'art : les lettres du début, en minuscules (`FEN7000.FRM` → `fen`).
pub fn style_of(name: &str) -> String {
    name.chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Les préfixes des murs classiques d'Abri décrivent leur rôle, pas des matériaux
/// différents : VA/VB sont les deux axes, VCN les angles, VD les encadrements et
/// VT les jonctions. Les séparer produit des colonnes isolées avec les pièces VB.
fn wall_family(name: &str) -> String {
    let prefix = style_of(name);
    match prefix.as_str() {
        "va" | "vaw" | "vb" | "vcn" | "vd" | "ve" | "vt" | "vtx" | "vx" => {
            "classic_vault".to_string()
        }
        _ => prefix,
    }
}

/// Un objet de décor observé près d'un mur du style : position relative à une pièce du mur (`dr` lignes, `dc` colonnes),
/// côté intérieur de la pièce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decor {
    /// Le dessin du mur d'origine fixe aussi la face sur laquelle poser le décor.
    pub wall: u32,
    /// Axe du mur qui sert de repère.
    pub axis: Axis,
    /// Parité de la position de la pièce de mur (colonne × 2 + ligne).
    pub par: u8,
    pub dr: i8,
    pub dc: i8,
    pub fid: u32,
    pub orient: u8,
    pub door: bool,
    /// Nombre d'observations.
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Doorway {
    axis: Axis,
    col_par: u8,
    fid: u32,
    orient: u8,
    offset: (i32, i32),
    flags: u32,
    /// Position le long du mur, relative à la porte, et FID de chaque panneau.
    walls: Vec<(i8, u32)>,
    frame_walls: Vec<(i8, i8, u32)>,
}

/// Une pièce d'un mur : écart en hexagones depuis le début du mur, et FID.
pub type Piece = (u32, u32);

/// Enchaînements d'un style : pour une pièce et la parité de sa position, (pas, pièce suivante).
#[derive(Debug, Default, Clone)]
pub struct StyleModel {
    floors: BTreeMap<u16, u32>,
    roofs: BTreeMap<u16, u32>,
    strong: HashMap<(Axis, u8, u32), (u8, u32)>,
    /// Enchaînements vus une seule fois : second recours quand il n'y en a pas de plus sûr.
    weak: HashMap<(Axis, u8, u32), (u8, u32)>,
    /// Pièces de départ possibles par axe et parité, de la plus observée à la moins observée.
    starts: HashMap<(Axis, u8), Vec<(u32, u32)>>,
    pids: HashMap<u32, u32>,
    /// Largeur en pixels du dessin de chaque pièce (donnée par l'appelant, qui a accès aux images) ; vide : inconnue.
    widths: HashMap<u32, u32>,
    /// Pièce d'angle par sorte (voir `WallStats::corners`) et parité de la colonne.
    corners: HashMap<(u8, u8), u32>,
    /// Voisine d'une pièce d'angle le long d'un axe : (axe, parité, pièce) → (pas, pièce suivante).
    corner_next: HashMap<(Axis, u8, u32), (u8, u32)>,
    /// Alternatives de raccord et d'angle, conservées pour fermer un contour.
    edges: EdgeTable,
    corner_choices: HashMap<(u8, u8), Vec<(u32, u32)>>,
    decor: Vec<Decor>,
    doorways: Vec<(Doorway, u32)>,
    /// Pièces de mur qui servent de fenêtres dans ce style : (axe, parité, FID, observations).
    windows: Vec<(Axis, u8, u32, u32)>,
    /// Source lumineuse la plus courante dans les cartes et écart habituel entre deux sources, en hexagones.
    light: Option<RoomLight>,
    /// Types de pièce : meubles qui vont ensemble dans ce style.
    kinds: Vec<RoomKind>,
    /// Escaliers appris : pour monter et pour descendre (indépendants du style de murs).
    stairs: Option<StairKinds>,
}

/// Escaliers à poser entre deux étages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StairKinds {
    pub up_fid: u32,
    pub up_pid: u32,
    pub down_fid: u32,
    pub down_pid: u32,
}

/// Type de pièce : un meuble de référence et ceux qui l'accompagnent.
#[derive(Debug, Clone)]
pub struct RoomKind {
    /// Meuble de référence du type (le plus courant du style).
    pub seed: u32,
    pub members: Arc<HashSet<u32>>,
    /// Nombre de meubles posés dans les cartes.
    pub count: u32,
}

/// Source lumineuse que l'on pose dans les pièces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomLight {
    pub fid: u32,
    /// Écart habituel entre deux sources voisines, en hexagones (de 3 à 8).
    pub spacing: u32,
    /// Rayon et intensité de lumière à donner à l'objet posé (ceux des cartes, pas ceux du prototype).
    pub distance: i32,
    pub intensity: i32,
}

impl StyleModel {
    /// Famille de sol la plus observée à l'intérieur près de ces murs, et uniquement ses tuiles observées.
    pub fn floor_choice(&self, names: &[String]) -> Option<(String, Vec<u16>)> {
        Self::tile_choice(&self.floors, names)
    }

    pub fn roof_choice(&self, names: &[String]) -> Option<(String, Vec<u16>)> {
        Self::tile_choice(&self.roofs, names)
    }

    fn tile_choice(counts: &BTreeMap<u16, u32>, names: &[String]) -> Option<(String, Vec<u16>)> {
        let mut families: BTreeMap<String, u64> = BTreeMap::new();
        for (&id, &n) in counts {
            if let Some(name) = names.get(id as usize) {
                *families
                    .entry(crate::tile_stats::family_of(name))
                    .or_default() += u64::from(n);
            }
        }
        let family = families
            .into_iter()
            .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))?
            .0;
        let ids = counts
            .keys()
            .copied()
            .filter(|&id| {
                names
                    .get(id as usize)
                    .is_some_and(|n| crate::tile_stats::family_of(n) == family)
            })
            .collect();
        Some((family, ids))
    }
    /// Pièce d'angle de cette sorte pour une colonne de parité `par` (à défaut, celle de l'autre parité).
    pub fn corner(&self, kind: u8, par: u32) -> Option<u32> {
        self.corners
            .get(&(kind, (par % 2) as u8))
            .or_else(|| self.corners.get(&(kind, ((par + 1) % 2) as u8)))
            .copied()
    }

    /// Pièce qui suit une pièce d'angle le long de `axis` : (pas, pièce).
    pub fn after_corner(&self, axis: Axis, par: u32, corner: u32) -> Option<(u32, u32)> {
        self.corner_next
            .get(&(axis, (par % 2) as u8, corner))
            .map(|&(s, b)| (u32::from(s), b))
            .or_else(|| self.next(axis, (par % 2) as u8, corner))
    }

    /// Renseigne la largeur du dessin de chaque pièce connue (`width(fid)`), pour repérer les murs qui laisseraient des trous.
    pub fn set_widths(&mut self, width: &dyn Fn(u32) -> Option<u32>) {
        let fids: Vec<u32> = self.pids.keys().copied().collect();
        self.widths = fids
            .into_iter()
            .filter_map(|f| width(f).map(|w| (f, w)))
            .collect();
    }

    /// Mur dont toutes les pièces sont espacées de 2 hexagones avec des dessins trop étroits pour se toucher : on y ajoute la
    /// suite d'en face (décalée d'un hexagone) pour que le mur soit plein. `start` : position (colonne ou ligne) de l'hexagone
    /// d'origine du mur, `len` : sa longueur, `first_off` : écart de sa première pièce.
    fn fill_gaps(&self, axis: Axis, start: u32, len: u32, chain: Vec<Piece>) -> Vec<Piece> {
        if self.widths.is_empty()
            || chain.len() < 3
            || chain.windows(2).any(|w| w[1].0 - w[0].0 != 2)
        {
            return chain;
        }
        // Un pas de 2 hexagones fait 48 pixels le long des colonnes et 32 le long des lignes.
        let span = if axis == Axis::Col { 48.0 } else { 32.0 };
        let covered: f64 = chain
            .iter()
            .map(|p| f64::from(self.widths.get(&p.1).copied().unwrap_or(0)))
            .sum::<f64>()
            / chain.len() as f64;
        if covered >= span * 0.8 {
            return chain;
        }
        let first = chain[0].0;
        let par = ((start + first + 1) % 2) as u8;
        let (extra, _) = self.chain_from(
            axis,
            par,
            match self.start_candidates(axis, par).next() {
                Some(a) => a,
                None => return chain,
            },
            first + 1,
            len,
        );
        let mut merged = chain;
        merged.extend(extra);
        merged.sort_by_key(|p| p.0);
        merged.dedup_by_key(|p| p.0);
        merged
    }

    /// Écarte les suites ordinaires vues une seule fois. Les raccords de jonction
    /// restent disponibles au solveur : certains croisements du jeu sont uniques.
    pub fn strong_only(&self) -> StyleModel {
        let mut model = self.clone();
        model.weak.clear();
        let junctions: HashSet<u32> = model
            .corner_choices
            .values()
            .flatten()
            .map(|&(_, fid)| fid)
            .collect();
        for (&(_, _, first), edges) in &mut model.edges {
            edges.retain(|&(_, last, count)| {
                count >= MIN_COUNT || junctions.contains(&first) || junctions.contains(&last)
            });
        }
        model
    }

    /// Vrai si les deux murs peuvent être construits.
    pub fn is_usable(&self) -> bool {
        [Axis::Col, Axis::Row]
            .iter()
            .all(|&a| (0..2u8).any(|p| self.starts.contains_key(&(a, p))))
    }

    /// Combien de fois ce meuble a été vu près des murs de ce style.
    pub fn furniture_weight(&self, fid: u32) -> u32 {
        self.decor
            .iter()
            .filter(|d| !d.door && d.fid == fid)
            .map(|d| d.count)
            .sum()
    }

    /// Escaliers appris des cartes (monter, descendre), s'il y en a assez dans les deux sens.
    pub fn stairs(&self) -> Option<StairKinds> {
        self.stairs
    }

    /// Types de pièce appris pour ce style, du plus fourni au moins fourni.
    pub fn kinds(&self) -> &[RoomKind] {
        &self.kinds
    }

    /// Source lumineuse que l'on peut poser, si les cartes en contiennent assez.
    pub fn light(&self) -> Option<RoomLight> {
        self.light
    }

    /// Nombre de variantes de fenêtre connues pour ce style.
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    pub fn door_sample(&self) -> Option<(u32, u8)> {
        self.doorways.first().map(|(d, _)| (d.fid, d.orient))
    }
    pub fn window_sample(&self) -> Option<u32> {
        self.windows.first().map(|w| w.2)
    }

    /// Nombre de portes et d'objets de décor observés près des murs de ce style.
    pub fn decor_counts(&self) -> (usize, usize) {
        let doors = self.decor.iter().filter(|d| d.door).count();
        (doors, self.decor.len() - doors)
    }

    /// Prototype d'une pièce du style (ou d'un objet de décor observé).
    pub fn pid_of(&self, fid: u32) -> Option<u32> {
        self.pids.get(&fid).copied()
    }

    /// Voisin de `piece` le long de `axis` quand elle est sur une position de parité `par` : (pas, pièce).
    pub fn next(&self, axis: Axis, par: u8, piece: u32) -> Option<(u32, u32)> {
        let k = (axis, par % 2, piece);
        self.strong
            .get(&k)
            .or_else(|| self.weak.get(&k))
            .map(|&(s, b)| (s as u32, b))
    }

    /// Pièces de départ possibles le long de `axis`, de la plus observée à la moins observée.
    pub fn start_candidates(&self, axis: Axis, par: u8) -> impl Iterator<Item = u32> + '_ {
        self.starts
            .get(&(axis, par % 2))
            .into_iter()
            .flatten()
            .map(|&(_, a)| a)
    }

    /// Mur de `len` hexagones le long de `axis` dont la première pièce est `first`, posée à l'écart `offset` du début et
    /// sur une position de parité `par0`. Renvoie les pièces et `true` si l'enchaînement s'est arrêté faute de données
    /// avant d'atteindre la longueur.
    pub fn chain_from(
        &self,
        axis: Axis,
        par0: u8,
        first: u32,
        offset: u32,
        len: u32,
    ) -> (Vec<Piece>, bool) {
        let mut out = Vec::new();
        let (mut cur, mut off, mut par) = (first, offset, par0 % 2);
        while off < len {
            out.push((off, cur));
            let Some((step, next)) = self.next(axis, par, cur) else {
                return (out, true);
            };
            off += step;
            par = (par + step as u8) % 2;
            cur = next;
        }
        (out, false)
    }

    /// Mur de `len` hexagones qui commence par la pièce de départ la plus observée pour cette parité.
    pub fn chain(&self, axis: Axis, par0: u8, len: u32) -> (Vec<Piece>, bool) {
        match self.start_candidates(axis, par0).next() {
            Some(first) => self.chain_from(axis, par0, first, 0, len),
            None => (Vec::new(), true),
        }
    }
}

/// Une pièce à poser : les pièces de mur (hexagone, FID), tous les hexagones du rectangle (pour le sol) et `true` si l'un des
/// murs s'arrête avant la longueur demandée faute de données.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RoomPlan {
    pub walls: Vec<(u32, u32)>,
    pub hexes: Vec<u32>,
    pub short: bool,
    /// Portes (posées à la place de pièces de mur) et mobilier.
    pub doors: Vec<Placed>,
    pub furniture: Vec<Placed>,
    /// Sources lumineuses (décors invisibles qui éclairent), en grille dans la pièce.
    pub lights: Vec<Placed>,
    /// Nombre de pièces de mur remplacées par une fenêtre.
    pub windows: usize,
    /// Passages ouverts dans une cloison (pièce de mur retirée) faute de porte connue pour ce mur.
    pub openings: usize,
}

/// Un objet de décor posé : hexagone, FID, orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub hex: u32,
    pub fid: u32,
    pub orient: u8,
    pub offset: (i32, i32),
    pub flags: Option<u32>,
}

impl Placed {
    fn new(hex: u32, fid: u32, orient: u8) -> Self {
        Self {
            hex,
            fid,
            orient,
            offset: (0, 0),
            flags: None,
        }
    }
}

/// Ce qu'on ajoute aux murs.
#[derive(Debug, Clone, Default)]
pub struct DecorOptions {
    pub door: bool,
    /// Poser des sources lumineuses en grille dans la pièce.
    pub lights: bool,
    /// Tailles de pièce variables dans un bâtiment (couloirs étroits, grandes salles) ; sinon toutes les pièces sont identiques.
    pub vary_sizes: bool,
    /// Graine des tailles de pièce ; `None` : celle de `seed`. Deux étages d'un même bâtiment la partagent pour avoir la même empreinte.
    pub size_seed: Option<u64>,
    /// Type de pièce (indice dans `StyleModel::kinds`) ; `None` choisit un ensemble cohérent selon la graine.
    pub kind: Option<usize>,
    /// Construire aussi les deux murs de devant (les deux autres côtés du rectangle).
    pub front: bool,
    /// Seulement le mur de devant le long des colonnes (côté « bas »), ou seulement celui le long des lignes (côté « droit »).
    pub front_bottom: bool,
    pub front_right: bool,
    /// Une porte dans le mur du fond le long des colonnes / le long des lignes (portes entre deux pièces d'un bâtiment).
    pub door_in_col_wall: bool,
    pub door_in_row_wall: bool,
    /// Nombre de fenêtres voulues.
    pub windows: u32,
    /// Nombre d'objets de mobilier voulus.
    pub furniture: u32,
    pub seed: u64,
}

/// Plan d'une pièce dont le coin du haut est `corner`. `cols` : longueur du mur de gauche (le long des colonnes),
/// `rows` : longueur du mur de droite (le long des lignes), en hexagones. Les parties hors de la carte sont ignorées.
pub fn plan_room(model: &StyleModel, corner: u32, rows: u32, cols: u32) -> RoomPlan {
    plan_room_with(model, corner, rows, cols, &DecorOptions::default())
}

/// Mur libre aligné sur un des deux axes isométriques. Les variantes suivent les raccords appris.
pub fn plan_wall_run(model: &StyleModel, start: u32, axis: Axis, length: u32) -> RoomPlan {
    let (c, r) = geom::hex_col_row(start);
    let length = length.clamp(1, 200);
    let pos = if axis == Axis::Col { c } else { r };
    let (pieces, short) = model.chain(axis, (pos % 2) as u8, length);
    let pieces = model.fill_gaps(axis, pos, length, pieces);
    let mut plan = RoomPlan {
        short,
        ..Default::default()
    };
    for off in 0..length {
        let (col, row) = if axis == Axis::Col {
            (c + off, r)
        } else {
            (c, r + off)
        };
        if col < 200 && row < 200 {
            plan.hexes.push(row * 200 + col);
        } else {
            plan.short = true;
        }
    }
    for (off, fid) in pieces {
        let (col, row) = if axis == Axis::Col {
            (c + off, r)
        } else {
            (c, r + off)
        };
        if col < 200 && row < 200 {
            plan.walls.push((row * 200 + col, fid));
        }
    }
    plan
}

/// Recalcule les angles aux jonctions, uniquement si tous les raccords voisins sont connus.
pub fn join_wall_corners(model: &StyleModel, walls: &mut [(u32, u32)], changed: &[u32]) {
    let snapshot = walls.to_vec();
    for (hex, fid) in walls {
        if !changed.contains(hex) {
            continue;
        }
        let (c, r) = geom::hex_col_row(*hex);
        let mut neighbors = Vec::new();
        for (axis, dc, dr) in [
            (Axis::Col, -1, 0),
            (Axis::Col, 1, 0),
            (Axis::Row, 0, -1),
            (Axis::Row, 0, 1),
        ] {
            for step in 1..=2i32 {
                let (nc, nr) = (c as i32 + dc * step, r as i32 + dr * step);
                if nc < 0 || nr < 0 || nc >= 200 || nr >= 200 {
                    break;
                }
                if let Some(&(_, f)) = snapshot.iter().find(|p| p.0 == (nr * 200 + nc) as u32) {
                    neighbors.push((axis, step as u8, dc + dr > 0, f));
                    break;
                }
            }
        }
        if !neighbors.iter().any(|n| n.0 == Axis::Col)
            || !neighbors.iter().any(|n| n.0 == Axis::Row)
        {
            continue;
        }
        let mut candidates: Vec<_> = model
            .corner_choices
            .iter()
            .filter(|(k, _)| k.1 == (c % 2) as u8)
            .flat_map(|(_, v)| v.iter().copied())
            .collect();
        candidates.sort_by_key(|&(n, f)| (std::cmp::Reverse(n), f));
        if let Some((_, corner)) = candidates.into_iter().find(|&(_, candidate)| {
            neighbors.iter().all(|&(axis, step, after, other)| {
                let pos = if axis == Axis::Col { c } else { r };
                let (first, last, par) = if after {
                    (candidate, other, (pos % 2) as u8)
                } else {
                    (other, candidate, ((pos - step as u32) % 2) as u8)
                };
                model
                    .edges
                    .get(&(axis, par, first))
                    .is_some_and(|v| v.iter().any(|&(s, f, _)| s == step && f == last))
            })
        }) {
            *fid = corner;
        }
    }
}

/// Remplace un panneau par une fenêtre compatible ou un encadrement de porte complet.
pub fn opening_at(
    model: &StyleModel,
    walls: &[(u32, u32)],
    target: u32,
    window: bool,
) -> Option<RoomPlan> {
    let mut plan = RoomPlan {
        walls: walls.to_vec(),
        ..Default::default()
    };
    if window {
        let mut candidates = Vec::new();
        for &(hex, _) in walls {
            if geom::hex_distance(hex, target) > 1 {
                continue;
            }
            let (c, r) = geom::hex_col_row(hex);
            for &(axis, par, fid, n) in &model.windows {
                if par == (if axis == Axis::Col { c % 2 } else { r % 2 }) as u8
                    && replacement_fits(&plan, model, axis, c, r, fid)
                {
                    candidates.push((
                        geom::hex_distance(hex, target),
                        std::cmp::Reverse(n),
                        hex,
                        fid,
                    ));
                }
            }
        }
        candidates.sort_unstable();
        let &(_, _, hex, fid) = candidates.first()?;
        plan.walls.iter_mut().find(|p| p.0 == hex)?.1 = fid;
        plan.windows = 1;
        return Some(plan);
    }
    let at: BTreeMap<_, _> = walls.iter().copied().collect();
    let mut candidates = Vec::new();
    for (index, (door, n)) in model.doorways.iter().enumerate() {
        for &(hex, _) in walls {
            if geom::hex_distance(hex, target) > 2 {
                continue;
            }
            let (c, r) = geom::hex_col_row(hex);
            if (c % 2) as u8 != door.col_par {
                continue;
            }
            let shifted = |dc: i32, dr: i32| {
                let (x, y) = (c as i32 + dc, r as i32 + dr);
                (x >= 0 && y >= 0 && x < 200 && y < 200).then_some((y * 200 + x) as u32)
            };
            let line = |off: i8| {
                if door.axis == Axis::Col {
                    shifted(off as i32, 0)
                } else {
                    shifted(0, off as i32)
                }
            };
            let (first, last) = (door.walls.first()?, door.walls.last()?);
            if line(first.0).and_then(|h| at.get(&h)) != Some(&first.1)
                || line(last.0).and_then(|h| at.get(&h)) != Some(&last.1)
            {
                continue;
            }
            if door.frame_walls.iter().any(|&(dc, dr, f)| {
                shifted(dc as i32, dr as i32)
                    .is_none_or(|h| at.get(&h).is_some_and(|&old| old != f))
            }) {
                continue;
            }
            candidates.push((
                geom::hex_distance(hex, target),
                std::cmp::Reverse(*n),
                index,
                hex,
            ));
        }
    }
    candidates.sort_unstable();
    let &(_, _, index, hex) = candidates.first()?;
    let door = &model.doorways[index].0;
    let (c, r) = geom::hex_col_row(hex);
    let (lo, hi) = (door.walls.first()?.0 as i32, door.walls.last()?.0 as i32);
    plan.walls.retain(|&(h, _)| {
        let (hc, hr) = geom::hex_col_row(h);
        let off = if door.axis == Axis::Col {
            hc as i32 - c as i32
        } else {
            hr as i32 - r as i32
        };
        !(off >= lo
            && off <= hi
            && if door.axis == Axis::Col {
                hr == r
            } else {
                hc == c
            })
    });
    for &(off, fid) in &door.walls {
        let h = if door.axis == Axis::Col {
            (r as i32 * 200 + c as i32 + off as i32) as u32
        } else {
            ((r as i32 + off as i32) * 200 + c as i32) as u32
        };
        plan.walls.push((h, fid));
    }
    for &(dc, dr, fid) in &door.frame_walls {
        let h = ((r as i32 + dr as i32) * 200 + c as i32 + dc as i32) as u32;
        plan.walls.retain(|p| p.0 != h);
        plan.walls.push((h, fid));
    }
    plan.doors.push(Placed {
        hex,
        fid: door.fid,
        orient: door.orient,
        offset: door.offset,
        flags: Some(door.flags),
    });
    Some(plan)
}

/// Comme `plan_room`, avec une porte et du mobilier d'après les objets observés près des murs du style.
pub fn plan_room_with(
    model: &StyleModel,
    corner: u32,
    rows: u32,
    cols: u32,
    opts: &DecorOptions,
) -> RoomPlan {
    let closed = opts.front || opts.front_bottom || opts.front_right;
    if rows >= 3 && cols >= 3 && !model.corner_choices.is_empty() {
        if let Some(plan) = plan_closed_room(model, corner, rows, cols, opts) {
            return plan;
        }
        let mut plan = plan_room_aligned(model, corner, rows, cols, opts);
        plan.short |= closed;
        return plan;
    }
    plan_room_aligned(model, corner, rows, cols, opts)
}

fn plan_room_aligned(
    model: &StyleModel,
    corner: u32,
    rows: u32,
    cols: u32,
    opts: &DecorOptions,
) -> RoomPlan {
    if cols < 3 || model.corner_choices.is_empty() {
        return plan_room_legacy(model, corner, rows, cols, opts);
    }
    let par = (0..2u8)
        .filter_map(|p| {
            model
                .corner_choices
                .get(&(3, p))
                .map(|choices| (choices[0].0, p))
        })
        .max_by_key(|&(score, par)| (score, std::cmp::Reverse(par)))
        .map_or(0, |(_, par)| par);
    let (c, r) = geom::hex_col_row(corner);
    let canonical = if c % 2 == u32::from(par) {
        c
    } else if c > 0 {
        c - 1
    } else {
        1
    };
    let source = r * geom::HEX_GRID_WIDTH + canonical;
    let mut plan = plan_room_legacy(model, source, rows, (cols & !1) + 1, opts);
    if source != corner {
        translate_room(&mut plan, source, corner);
    }
    plan
}

/// Recherche un chemin entre deux angles à une distance exacte. Chaque raccord
/// doit avoir été observé ; on ne remplace jamais la dernière pièce après coup.
fn wall_between(
    model: &StyleModel,
    axis: Axis,
    par: u8,
    first: u32,
    last: u32,
    distance: u32,
) -> Option<(u64, Vec<Piece>)> {
    type State = (u64, Vec<Piece>);
    let mut states: Vec<BTreeMap<u32, State>> = vec![BTreeMap::new(); distance as usize + 1];
    states[0].insert(first, (0, vec![(0, first)]));
    for off in 0..distance {
        let current = std::mem::take(&mut states[off as usize]);
        for (fid, (cost, path)) in current {
            let Some(edges) = model.edges.get(&(axis, (par + off as u8) % 2, fid)) else {
                continue;
            };
            let maximum = edges.iter().map(|e| e.2).max().unwrap_or(1).max(1);
            for &(step, next, count) in edges {
                let dest = off + u32::from(step);
                if step == 0 || dest > distance || (dest == distance && next != last) {
                    continue;
                }
                let next_cost = cost
                    + 1
                    + u64::from((maximum - count) * 1024 / maximum)
                    + if count < MIN_COUNT { 4096 } else { 0 };
                let entry = states[dest as usize].get(&next);
                if entry.is_none_or(|(old, _)| next_cost < *old) {
                    let mut next_path = path.clone();
                    next_path.push((dest, next));
                    states[dest as usize].insert(next, (next_cost, next_path));
                }
            }
        }
    }
    states[distance as usize].remove(&last)
}

fn translate_room(plan: &mut RoomPlan, from: u32, to: u32) {
    let translate = |h| geom::hex_translate(h, from, to);
    let mut dropped = false;
    plan.walls.retain_mut(|(hex, _)| match translate(*hex) {
        Some(h) => {
            *hex = h;
            true
        }
        None => {
            dropped = true;
            false
        }
    });
    plan.hexes = plan.hexes.iter().filter_map(|&h| translate(h)).collect();
    for list in [&mut plan.doors, &mut plan.furniture, &mut plan.lights] {
        list.retain_mut(|p| match translate(p.hex) {
            Some(h) => {
                p.hex = h;
                true
            }
            None => {
                dropped = true;
                false
            }
        });
    }
    plan.short |= dropped;
}

/// Résout toutes les jonctions d'un bâtiment ensemble. La frontière de recherche
/// garde plusieurs affectations : un choix local ne doit pas condamner le raccord
/// avec la rangée suivante. Les chemins sont mis en cache par couple de pièces.
fn building_walls(model: &StyleModel, columns: &[u32], rows: &[u32]) -> Option<Vec<(u32, u32)>> {
    type Edge = (Axis, u8, u32, u32, u32);
    type Path = Option<(u64, Vec<Piece>)>;
    let width = columns.len();
    let height = rows.len();
    let mut paths: HashMap<Edge, Path> = HashMap::new();
    let mut beam: Vec<(u64, Vec<u32>)> = vec![(0, Vec::new())];
    for (j, &r) in rows.iter().enumerate() {
        for (i, &c) in columns.iter().enumerate() {
            let kind = match (i == 0, i == width - 1, j == 0, j == height - 1) {
                (true, _, true, _) => 3,
                (_, true, true, _) => 1,
                (true, _, _, true) => 2,
                (_, true, _, true) => 0,
                (_, _, true, _) => 5,
                (_, _, _, true) => 4,
                (true, _, _, _) => 7,
                (_, true, _, _) => 6,
                _ => 8,
            };
            let choices = model.corner_choices.get(&(kind, (c % 2) as u8))?;
            let max_score = choices[0].0.max(1);
            let mut next = Vec::new();
            for (cost, fids) in &beam {
                for &(score, fid) in choices {
                    let mut score = *cost + u64::from((max_score - score) * 1024 / max_score);
                    let mut compatible = true;
                    for edge in [
                        (i > 0).then(|| {
                            (
                                Axis::Col,
                                (columns[i - 1] % 2) as u8,
                                fids[fids.len() - 1],
                                fid,
                                c - columns[i - 1],
                            )
                        }),
                        (j > 0).then(|| {
                            (
                                Axis::Row,
                                (rows[j - 1] % 2) as u8,
                                fids[fids.len() - width],
                                fid,
                                r - rows[j - 1],
                            )
                        }),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        let path = paths.entry(edge).or_insert_with(|| {
                            wall_between(model, edge.0, edge.1, edge.2, edge.3, edge.4)
                        });
                        match path {
                            Some((cost, _)) => score += *cost,
                            None => {
                                compatible = false;
                                break;
                            }
                        }
                    }
                    if compatible {
                        let mut fids = fids.clone();
                        fids.push(fid);
                        next.push((score, fids));
                    }
                }
            }
            next.sort();
            // Conserver la meilleure histoire pour une même frontière active.
            let mut frontiers = HashSet::new();
            next.retain(|(_, fids)| {
                frontiers.insert(fids[fids.len().saturating_sub(width)..].to_vec())
            });
            next.truncate(512);
            if next.is_empty() {
                return None;
            }
            beam = next;
        }
    }
    let (_, fids) = &beam[0];
    let mut walls = BTreeMap::new();
    for (j, &r) in rows.iter().enumerate() {
        for (i, &c) in columns.iter().enumerate() {
            let last = fids[j * width + i];
            for (axis, x, y, first, distance) in [
                (i > 0).then(|| {
                    (
                        Axis::Col,
                        columns[i - 1],
                        r,
                        fids[j * width + i - 1],
                        c - columns[i - 1],
                    )
                }),
                (j > 0).then(|| {
                    (
                        Axis::Row,
                        c,
                        rows[j - 1],
                        fids[(j - 1) * width + i],
                        r - rows[j - 1],
                    )
                }),
            ]
            .into_iter()
            .flatten()
            {
                let par = if axis == Axis::Col { x % 2 } else { y % 2 } as u8;
                let (_, path) = paths.get(&(axis, par, first, last, distance))?.as_ref()?;
                for &(off, fid) in path {
                    let (c, r) = if axis == Axis::Col {
                        (x + off, y)
                    } else {
                        (x, y + off)
                    };
                    walls.insert(r * geom::HEX_GRID_WIDTH + c, fid);
                }
            }
        }
    }
    Some(walls.into_iter().collect())
}

fn plan_grid_building(
    model: &StyleModel,
    corner: u32,
    widths: &[u32],
    depths: &[u32],
    opts: &DecorOptions,
) -> Option<RoomPlan> {
    let (clicked_col, r0) = geom::hex_col_row(corner);
    let par =
        (0..2u8).find(|&p| (0..4).all(|kind| model.corner_choices.contains_key(&(kind, p))))?;
    let c0 = if clicked_col % 2 == u32::from(par) {
        clicked_col
    } else if clicked_col > 0 {
        clicked_col - 1
    } else {
        1
    };
    let mut columns = vec![c0];
    let mut rows = vec![r0];
    for &w in widths {
        columns.push(columns.last()? + (w & !1));
    }
    for &d in depths {
        rows.push(rows.last()? + d);
    }
    if *columns.last()? >= geom::HEX_GRID_WIDTH || *rows.last()? >= geom::HEX_GRID_WIDTH {
        return None;
    }
    let walls = building_walls(model, &columns, &rows)?;
    let mut out = RoomPlan {
        walls,
        ..RoomPlan::default()
    };
    for r in r0..=*rows.last()? {
        for c in c0..=*columns.last()? {
            out.hexes.push(r * geom::HEX_GRID_WIDTH + c);
        }
    }
    let mut rng = Rng(opts.seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut edited: BTreeMap<u32, u32> = out.walls.iter().copied().collect();
    for j in 0..depths.len() {
        for i in 0..widths.len() {
            let (c, r, ce, re) = (columns[i], rows[j], columns[i + 1], rows[j + 1]);
            let mut room = RoomPlan {
                walls: edited
                    .iter()
                    .map(|(&h, &fid)| (h, fid))
                    .filter(|&(h, _)| {
                        let (hc, hr) = geom::hex_col_row(h);
                        (c..=ce).contains(&hc) && (r..=re).contains(&hr)
                    })
                    .collect(),
                ..RoomPlan::default()
            };
            let before: Vec<_> = room.walls.iter().map(|p| p.0).collect();
            let mut anchors = Vec::new();
            let mut front = Vec::new();
            for &(h, _) in &room.walls {
                let (hc, hr) = geom::hex_col_row(h);
                if hr == r {
                    anchors.push((Axis::Col, hc, hr));
                } else if hc == c {
                    anchors.push((Axis::Row, hc, hr));
                } else if j == depths.len() - 1 && hr == re {
                    front.push((Axis::Col, hc, hr));
                } else if i == widths.len() - 1 && hc == ce {
                    front.push((Axis::Row, hc, hr));
                }
            }
            let kind = opts.kind.or_else(|| {
                (!model.kinds.is_empty()).then(|| (rng.next() % model.kinds.len() as u64) as usize)
            });
            let room_opts = DecorOptions {
                door: i == widths.len() - 1 && j == depths.len() - 1,
                door_in_col_wall: j > 0,
                door_in_row_wall: i > 0,
                kind,
                seed: rng.next() | 1,
                windows: if (i + j) % 2 == 0 { opts.windows } else { 0 },
                ..opts.clone()
            };
            add_decor(
                &mut room,
                model,
                &anchors,
                &front,
                (c, r, ce - c + 1, re - r),
                &room_opts,
            );
            for h in before {
                edited.remove(&h);
            }
            edited.extend(room.walls);
            out.doors.extend(room.doors);
            out.furniture.extend(room.furniture);
            out.lights.extend(room.lights);
            out.windows += room.windows;
            out.openings += room.openings;
        }
    }
    for door in &out.doors {
        edited.remove(&door.hex);
    }
    out.walls = edited.into_iter().collect();
    let source = r0 * geom::HEX_GRID_WIDTH + c0;
    if source != corner {
        translate_room(&mut out, source, corner);
    }
    Some(out)
}

/// Les angles d'un style n'existent pas toujours sur les deux parités de colonne.
/// Construire dans son repère appris, puis translater en coordonnées cube garde
/// exactement les raccords à l'écran, même lorsque le clic est sur une colonne impaire.
fn plan_closed_room(
    model: &StyleModel,
    corner: u32,
    rows: u32,
    cols: u32,
    opts: &DecorOptions,
) -> Option<RoomPlan> {
    let (clicked_col, r0) = geom::hex_col_row(corner);
    let nw_kind = match (opts.door_in_row_wall, opts.door_in_col_wall) {
        (false, false) => 3,
        (true, false) => 5,
        (false, true) => 7,
        (true, true) => 8,
    };
    let kinds = [
        nw_kind,
        if opts.door_in_col_wall { 6 } else { 1 },
        if opts.door_in_row_wall { 4 } else { 2 },
        0,
    ];
    let par = (0..2u8).find(|&p| {
        kinds
            .iter()
            .all(|&kind| model.corner_choices.contains_key(&(kind, p)))
    })?;
    let c0 = if clicked_col % 2 == u32::from(par) {
        clicked_col
    } else if clicked_col > 0 {
        clicked_col - 1
    } else {
        1
    };
    // Une largeur entière de tuiles, angles inclus. Ne pas poser un angle pair
    // sur une colonne impaire simplement parce que cette variante manque.
    let span = cols & !1;
    if span < 2 || c0 + span >= geom::HEX_GRID_WIDTH || r0 + rows >= geom::HEX_GRID_WIDTH {
        return None;
    }
    let choices: Vec<_> = kinds
        .iter()
        .map(|&kind| &model.corner_choices[&(kind, par)])
        .collect();
    let corner_cost =
        |i: usize, score: u32| u64::from((choices[i][0].0 - score) * 1024 / choices[i][0].0.max(1));
    let mut best: Option<(u64, [Vec<Piece>; 4])> = None;
    // Calculer chaque côté une seule fois par couple d'angles.
    let sides = [
        (Axis::Col, 0, 1, span, par),
        (Axis::Row, 0, 2, rows, (r0 % 2) as u8),
        (Axis::Col, 2, 3, span, par),
        (Axis::Row, 1, 3, rows, (r0 % 2) as u8),
    ];
    let paths: Vec<_> = sides
        .iter()
        .map(|&(axis, a, b, len, p)| {
            choices[a]
                .iter()
                .map(|&(_, first)| {
                    choices[b]
                        .iter()
                        .map(|&(_, last)| wall_between(model, axis, p, first, last, len))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .collect();
    for (a, &(sa, _)) in choices[0].iter().enumerate() {
        for (b, &(sb, _)) in choices[1].iter().enumerate() {
            let Some(top) = &paths[0][a][b] else { continue };
            for (c, &(sc, _)) in choices[2].iter().enumerate() {
                let Some(left) = &paths[1][a][c] else {
                    continue;
                };
                for (d, &(sd, _)) in choices[3].iter().enumerate() {
                    let (Some(bottom), Some(right)) = (&paths[2][c][d], &paths[3][b][d]) else {
                        continue;
                    };
                    let cost = top.0
                        + left.0
                        + bottom.0
                        + right.0
                        + corner_cost(0, sa)
                        + corner_cost(1, sb)
                        + corner_cost(2, sc)
                        + corner_cost(3, sd);
                    if best.as_ref().is_none_or(|(old, _)| cost < *old) {
                        best = Some((
                            cost,
                            [
                                top.1.clone(),
                                left.1.clone(),
                                bottom.1.clone(),
                                right.1.clone(),
                            ],
                        ));
                    }
                }
            }
        }
    }
    let (_, [top, left, bottom, right]) = best?;
    let mut plan = RoomPlan::default();
    let (mut anchors, mut front) = (Vec::new(), Vec::new());
    let mut add = |axis: Axis, c: u32, r: u32, fid, is_front| {
        plan.walls.push((r * geom::HEX_GRID_WIDTH + c, fid));
        if is_front {
            front.push((axis, c, r))
        } else {
            anchors.push((axis, c, r))
        }
    };
    for (off, fid) in top {
        add(Axis::Col, c0 + off, r0, fid, false);
    }
    for (off, fid) in left.into_iter().filter(|p| p.0 > 0) {
        add(Axis::Row, c0, r0 + off, fid, false);
    }
    if opts.front || opts.front_bottom {
        for (off, fid) in bottom.into_iter().filter(|p| p.0 > 0) {
            add(Axis::Col, c0 + off, r0 + rows, fid, true);
        }
    }
    if opts.front || opts.front_right {
        for (off, fid) in right
            .into_iter()
            .filter(|p| p.0 > 0 && (p.0 < rows || !(opts.front || opts.front_bottom)))
        {
            add(Axis::Row, c0 + span, r0 + off, fid, true);
        }
    }
    for r in r0..=r0 + rows {
        for c in c0..=c0 + span {
            plan.hexes.push(r * geom::HEX_GRID_WIDTH + c);
        }
    }
    add_decor(
        &mut plan,
        model,
        &anchors,
        &front,
        (c0, r0, span + 1, rows),
        opts,
    );
    let source = r0 * geom::HEX_GRID_WIDTH + c0;
    if source != corner {
        translate_room(&mut plan, source, corner);
    }
    Some(plan)
}

fn plan_room_legacy(
    model: &StyleModel,
    corner: u32,
    rows: u32,
    cols: u32,
    opts: &DecorOptions,
) -> RoomPlan {
    let (c0, r0) = geom::hex_col_row(corner);
    let at = |c: u32, r: u32| {
        (c < geom::HEX_GRID_WIDTH && r < geom::HEX_GRID_WIDTH).then(|| r * geom::HEX_GRID_WIDTH + c)
    };
    let (p_col, p_row) = ((c0 % 2) as u8, (r0 % 2) as u8);
    // Le coin appartient au mur de gauche ; celui de droite part de ce qui suit le coin le long des lignes. On retient le
    // coin qui permet les deux murs jusqu'à la longueur demandée (puis le plus de pièces au total), pour que les deux
    // murs soient de la même matière.
    let mut candidates: Vec<((bool, usize), Vec<Piece>, Vec<Piece>, bool)> = model
        .start_candidates(Axis::Col, p_col)
        .filter_map(|a| {
            let (step, b) = model.next(Axis::Row, p_row, a)?;
            let (l, lshort) = model.chain_from(Axis::Col, p_col, a, 0, cols);
            let (r, rshort) =
                model.chain_from(Axis::Row, ((r0 + step) % 2) as u8, b, step, rows + 1);
            Some((
                (!lshort && !rshort, l.len() + r.len()),
                l,
                r,
                lshort || rshort,
            ))
        })
        .collect();
    // Le coin qui permet les deux murs complets avec le plus de pièces ; la graine choisit parmi les coins presque aussi bons
    // (même variante tant que la graine ne change pas).
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    let top = candidates.first().map(|c| c.0);
    let near = candidates
        .iter()
        .take_while(|c| top.is_some_and(|t| c.0.0 == t.0 && c.0.1 * 10 >= t.1 * 8))
        .count()
        .min(4);
    let best = (near > 0).then(|| candidates.swap_remove((opts.seed % near as u64) as usize));
    let (left, right, short) = match best {
        Some((_, l, r, s)) => (l, r, s),
        None => {
            let (l, ls) = model.chain(Axis::Col, p_col, cols);
            let (r, rs) = model.chain(Axis::Row, ((r0 + 1) % 2) as u8, rows);
            // Sans coin commun, le mur de droite part à la ligne suivante.
            (
                l,
                r.into_iter().map(|(o, f)| (o + 1, f)).collect(),
                ls || rs,
            )
        }
    };
    let mut left = model.fill_gaps(Axis::Col, c0, cols, left);
    let mut right = model.fill_gaps(Axis::Row, r0, rows + 1, right);
    // Les angles ont leurs propres pièces dans les cartes du jeu (coin du haut, de droite, de gauche, du bas) et chaque mur part de sa
    // pièce d'angle : le voisin et le pas (une pièce d'angle couvre souvent deux hexagones) sont ceux appris pour elle. Un côté sans mur de
    // devant se termine par la pièce courante.
    let (want_bottom, want_right) = (
        opts.front || opts.front_bottom,
        opts.front || opts.front_right,
    );
    let corners_on = (want_bottom || want_right) && cols >= 3 && rows >= 3;
    let (c_end, r_end) = (c0 + cols - 1, r0 + rows);
    // Dans un bâtiment à plusieurs pièces, le coin du haut d'une pièce qui a une voisine à l'ouest (`door_in_row_wall`) ou au nord
    // (`door_in_col_wall`) est une jonction en T ou un croisement, pas un angle.
    let nw_kind = match (opts.door_in_row_wall, opts.door_in_col_wall) {
        (false, false) => 3,
        (true, false) => 5,
        (false, true) => 7,
        (true, true) => 8,
    };
    let nw = model.corner(nw_kind, c0);
    let ne = (corners_on && want_right)
        .then(|| model.corner(if opts.door_in_col_wall { 6 } else { 1 }, c_end))
        .flatten();
    let sw = (corners_on && want_bottom)
        .then(|| model.corner(if opts.door_in_row_wall { 4 } else { 2 }, c0))
        .flatten();
    let se = (corners_on && want_bottom && want_right)
        .then(|| model.corner(0, c_end))
        .flatten();
    // Pièces d'un mur qui part de `corner` (posée à l'écart 0 de `origin`, colonne ou ligne) : leurs écarts vont jusqu'à `limit` exclu.
    let incomplete = std::cell::Cell::new(false);
    let from_corner = |axis: Axis, origin: u32, corner: u32, limit: u32| -> Option<Vec<Piece>> {
        let Some((step, first)) = model.after_corner(axis, origin, corner) else {
            incomplete.set(true);
            return None;
        };
        let (pieces, short) =
            model.chain_from(axis, ((origin + step) % 2) as u8, first, step, limit);
        incomplete.set(incomplete.get() || short);
        Some(pieces)
    };
    if let Some(nw) = nw {
        let limit = if ne.is_some() { cols - 1 } else { cols };
        match from_corner(Axis::Col, c0, nw, limit) {
            Some(rest) => left = std::iter::once((0, nw)).chain(rest).collect(),
            None => {
                left.retain(|p| p.0 > 0);
                left.push((0, nw));
            }
        }
        let limit = if sw.is_some() { rows } else { rows + 1 };
        if let Some(rest) = from_corner(Axis::Row, r0, nw, limit) {
            right = rest;
        }
    }
    if let Some(ne) = ne {
        left.retain(|p| p.0 < cols - 1);
        left.push((cols - 1, ne));
    }
    if let Some(sw) = sw {
        right.retain(|p| p.0 < rows);
        right.push((rows, sw));
    }
    left.sort_by_key(|p| p.0);
    right.sort_by_key(|p| p.0);
    let mut plan = RoomPlan {
        short: short || incomplete.get(),
        ..RoomPlan::default()
    };
    // Pièces de mur avec leur position (colonne, ligne), pour servir de repères au décor.
    let mut anchors: Vec<(Axis, u32, u32)> = Vec::new();
    for (off, fid) in left {
        if let Some(h) = at(c0 + off, r0) {
            plan.walls.push((h, fid));
            anchors.push((Axis::Col, c0 + off, r0));
        }
    }
    for (off, fid) in right {
        if let Some(h) = at(c0, r0 + off) {
            plan.walls.push((h, fid));
            anchors.push((Axis::Row, c0, r0 + off));
        }
    }
    for j in 0..=rows {
        for i in 0..cols {
            if let Some(h) = at(c0 + i, r0 + j) {
                plan.hexes.push(h);
            }
        }
    }
    // Murs de devant : le long des colonnes sur la dernière ligne, le long des lignes sur la dernière colonne ; les deux coins
    // du fond appartiennent déjà aux murs du fond.
    let mut front_anchors: Vec<(Axis, u32, u32)> = Vec::new();
    if corners_on {
        let mut bottom: Vec<Piece> = Vec::new();
        if want_bottom {
            // Écarts depuis la colonne `c0` ; le coin du bas à gauche est déjà dans le mur de gauche.
            let limit = if se.is_some() { cols - 1 } else { cols };
            bottom = match sw.and_then(|sw| from_corner(Axis::Col, c0, sw, limit)) {
                Some(rest) => rest.into_iter().filter(|p| p.0 > 0).collect(),
                None => {
                    let (v, s) = model.chain(Axis::Col, ((c0 + 1) % 2) as u8, cols - 1);
                    plan.short |= s;
                    model
                        .fill_gaps(Axis::Col, c0 + 1, cols - 1, v)
                        .into_iter()
                        .map(|(o, f)| (o + 1, f))
                        .collect()
                }
            };
            if let Some(se) = se {
                bottom.retain(|p| p.0 < cols - 1);
                bottom.push((cols - 1, se));
            }
        }
        let mut side: Vec<Piece> = Vec::new();
        if want_right {
            // Écarts depuis la ligne `r0` ; le coin du haut à droite est dans le mur du fond, celui du bas dans le mur de devant.
            side = match ne.and_then(|ne| from_corner(Axis::Row, r0, ne, rows)) {
                Some(rest) => rest.into_iter().filter(|p| p.0 > 0).collect(),
                None => {
                    let (v, s) = model.chain(Axis::Row, ((r0 + 1) % 2) as u8, rows - 1);
                    plan.short |= s;
                    model
                        .fill_gaps(Axis::Row, r0 + 1, rows - 1, v)
                        .into_iter()
                        .map(|(o, f)| (o + 1, f))
                        .collect()
                }
            };
        }
        for (off, fid) in bottom {
            if let Some(h) = at(c0 + off, r_end) {
                plan.walls.push((h, fid));
                front_anchors.push((Axis::Col, c0 + off, r_end));
            }
        }
        for (off, fid) in side {
            if let Some(h) = at(c_end, r0 + off) {
                plan.walls.push((h, fid));
                front_anchors.push((Axis::Row, c_end, r0 + off));
            }
        }
    }
    if opts.door
        || opts.door_in_col_wall
        || opts.door_in_row_wall
        || opts.furniture > 0
        || opts.windows > 0
        || opts.lights
    {
        add_decor(
            &mut plan,
            model,
            &anchors,
            &front_anchors,
            (c0, r0, cols, rows),
            opts,
        );
    }
    plan.short |= incomplete.get();
    plan
}

/// Largeurs des colonnes et profondeurs des rangées d'un bâtiment : toutes égales, ou variées (au moins 5 et 4 hexagones).
/// Les pièces d'une même colonne partagent leur largeur et celles d'une même rangée leur profondeur : les cloisons restent alignées.
fn room_sizes(
    rng: &mut Rng,
    nx: u32,
    ny: u32,
    cols: u32,
    rows: u32,
    vary: bool,
) -> (Vec<u32>, Vec<u32>) {
    let pick = |rng: &mut Rng, base: u32, min: u32| -> u32 {
        if !vary {
            return base;
        }
        // Plus souvent proche de la taille demandée que très différent ; un couloir (min) ou une grande salle de temps en temps.
        const DELTA: [i32; 8] = [-3, -2, -1, 0, 0, 1, 2, 4];
        (base as i32 + DELTA[(rng.next() % DELTA.len() as u64) as usize]).max(min as i32) as u32
    };
    (
        (0..nx).map(|_| pick(rng, cols, 5)).collect(),
        (0..ny).map(|_| pick(rng, rows, 4)).collect(),
    )
}

/// Position de départ de chaque pièce le long d'un axe : `extra` hexagones s'ajoutent entre deux pièces pour les rangées (leur
/// dernière ligne de sol et le mur de la rangée suivante).
fn starts(sizes: &[u32], extra: u32) -> Vec<u32> {
    let mut at = 0;
    sizes
        .iter()
        .map(|&w| {
            let s = at;
            at += w + extra;
            s
        })
        .collect()
}

/// Plan d'un bâtiment : une grille de `nx` × `ny` pièces dont le coin du haut de la première est `corner`. Chaque pièce garde ses
/// deux murs du fond (ceux d'une pièce voisine servent de cloison) ; seules les pièces du bord ont des murs de devant, et la
/// dernière pièce a la porte d'entrée sur sa façade. Une porte relie chaque pièce à sa voisine de l'ouest et à celle du nord.
/// Sans type de pièce imposé, chaque pièce reçoit un type de mobilier tiré au hasard parmi ceux du style.
pub fn plan_building(
    model: &StyleModel,
    corner: u32,
    nx: u32,
    ny: u32,
    rows: u32,
    cols: u32,
    opts: &DecorOptions,
) -> RoomPlan {
    let (nx, ny) = (nx.max(1), ny.max(1));
    if nx == 1 && ny == 1 {
        // Une pièce seule est un bâtiment : ses quatre murs sont construits (la porte d'entrée est sur la façade).
        return plan_room_with(
            model,
            corner,
            rows,
            cols,
            &DecorOptions {
                front_bottom: true,
                front_right: true,
                ..opts.clone()
            },
        );
    }
    let (c0, r0) = geom::hex_col_row(corner);
    let mut out = RoomPlan::default();
    let mut rng = Rng(opts.seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut size_rng = Rng(opts
        .size_seed
        .unwrap_or(opts.seed)
        .wrapping_mul(0xA24B_AED4_963E_E407)
        | 1);
    let (widths, depths) = room_sizes(&mut size_rng, nx, ny, cols, rows, opts.vary_sizes);
    if !model.corner_choices.is_empty() {
        if let Some(plan) = plan_grid_building(model, corner, &widths, &depths, opts) {
            return plan;
        }
        // Une disposition que les données ne permettent pas de raccorder doit
        // être signalée, même si l'aperçu de secours reste disponible.
        out.short = true;
    }
    let (col_starts, row_starts) = (starts(&widths, 0), starts(&depths, 1));
    for j in 0..ny {
        for i in 0..nx {
            let (c, r) = (c0 + col_starts[i as usize], r0 + row_starts[j as usize]);
            let (cols, rows) = (widths[i as usize], depths[j as usize]);
            if c >= geom::HEX_GRID_WIDTH || r >= geom::HEX_GRID_WIDTH {
                continue;
            }
            let last_row = j == ny - 1;
            let last_col = i == nx - 1;
            let kind = opts.kind.or_else(|| {
                (!model.kinds.is_empty()).then(|| (rng.next() % model.kinds.len() as u64) as usize)
            });
            let room_opts = DecorOptions {
                front: false,
                front_bottom: last_row,
                front_right: last_col,
                door: last_row && last_col,
                door_in_col_wall: j > 0,
                door_in_row_wall: i > 0,
                kind,
                seed: rng.next() | 1,
                // Les fenêtres : une pièce sur deux au plus, pour garder des pièces aveugles.
                windows: if (i + j) % 2 == 0 { opts.windows } else { 0 },
                ..opts.clone()
            };
            let room = plan_room_with(model, r * geom::HEX_GRID_WIDTH + c, rows, cols, &room_opts);
            out.walls.extend(room.walls);
            out.hexes.extend(room.hexes);
            out.doors.extend(room.doors);
            out.furniture.extend(room.furniture);
            out.lights.extend(room.lights);
            out.windows += room.windows;
            out.openings += room.openings;
            out.short |= room.short;
        }
    }
    out
}

/// Une paire d'escaliers entre deux étages : l'escalier qui monte (étage du bas) et celui qui descend (étage du haut), au même
/// hexagone ; on arrive à côté de l'escalier de l'autre étage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StairPlan {
    pub hex: u32,
    /// Hexagone d'arrivée sur l'étage du haut (en montant) et sur celui du bas (en descendant).
    pub arrive_up: u32,
    pub arrive_down: u32,
    pub kinds: StairKinds,
}

/// Valeur du premier mot d'un escalier : élévation d'arrivée et hexagone, comme dans les cartes du jeu
/// (`1 << 29 | 3 << 26 | hexagone` : élévation 1, hexagone donné).
pub fn stair_destination(elevation: u32, hex: u32) -> i32 {
    ((elevation << 29) | (3 << 26) | hex) as i32
}

/// Place une paire d'escaliers dans une case libre à l'intérieur du bâtiment, sur les deux étages. Il faut une case libre dans les
/// deux plans (ni mur, ni porte, ni meuble, ni lumière) et la case voisine (à droite) libre aussi, pour l'arrivée.
pub fn plan_stairs(
    model: &StyleModel,
    lower: &RoomPlan,
    upper: &RoomPlan,
    seed: u64,
) -> Option<StairPlan> {
    let kinds = model.stairs()?;
    let occupied = |plan: &RoomPlan| -> HashSet<u32> {
        plan.walls
            .iter()
            .map(|w| w.0)
            .chain(plan.doors.iter().map(|d| d.hex))
            .chain(plan.furniture.iter().map(|f| f.hex))
            .chain(plan.lights.iter().map(|l| l.hex))
            .collect()
    };
    let (busy_lower, busy_upper) = (occupied(lower), occupied(upper));
    let in_upper: HashSet<u32> = upper.hexes.iter().copied().collect();
    let free = |h: u32| {
        h < geom::HEX_COUNT
            && in_upper.contains(&h)
            && !busy_lower.contains(&h)
            && !busy_upper.contains(&h)
    };
    // Intérieur seulement : les cases du rectangle dont les quatre voisines directes (gauche, droite, haut, bas) en font partie.
    let hexes: HashSet<u32> = lower.hexes.iter().copied().collect();
    let width = geom::HEX_GRID_WIDTH;
    let mut candidates: Vec<u32> = lower
        .hexes
        .iter()
        .copied()
        .filter(|&h| {
            h % width > 0
                && h >= width
                && [h - 1, h + 1, h - width, h + width]
                    .iter()
                    .all(|n| hexes.contains(n))
        })
        .filter(|&h| free(h) && free(h + 1) && free(h + width))
        .collect();
    candidates.sort_unstable();
    candidates.dedup();
    if candidates.is_empty() {
        return None;
    }
    let mut rng = Rng(seed.wrapping_mul(0xD1B5_4A32_D192_ED03) | 1);
    let hex = candidates[(rng.next() % candidates.len() as u64) as usize];
    Some(StairPlan {
        hex,
        arrive_up: hex + 1,
        arrive_down: hex + 1,
        kinds,
    })
}

/// Petit générateur pseudo-aléatoire (xorshift) pour des résultats reproductibles avec la même graine.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// Indice tiré au hasard, proportionnellement aux poids.
    fn pick(&mut self, weights: &[u32]) -> Option<usize> {
        let total: u64 = weights.iter().map(|&w| w as u64).sum();
        if total == 0 {
            return None;
        }
        let mut roll = self.next() % total;
        weights.iter().position(|&w| {
            let w = w as u64;
            if roll < w {
                true
            } else {
                roll -= w;
                false
            }
        })
    }
}

/// Une fenêtre isolée doit conserver les deux raccords du panneau remplacé.
fn replacement_fits(
    plan: &RoomPlan,
    model: &StyleModel,
    axis: Axis,
    c: u32,
    r: u32,
    fid: u32,
) -> bool {
    let pos = if axis == Axis::Col { c } else { r };
    let mut line: Vec<_> = plan
        .walls
        .iter()
        .filter_map(|&(h, f)| {
            let (hc, hr) = geom::hex_col_row(h);
            (if axis == Axis::Col { hr == r } else { hc == c })
                .then_some((if axis == Axis::Col { hc } else { hr }, f))
        })
        .collect();
    line.sort_unstable();
    let (Some(&(before, a)), Some(&(after, b))) = (
        line.iter().rev().find(|p| p.0 < pos),
        line.iter().find(|p| p.0 > pos),
    ) else {
        return false;
    };
    let connects = |start: u32, a: u32, end: u32, b: u32| {
        model
            .edges
            .get(&(axis, (start % 2) as u8, a))
            .is_some_and(|edges| {
                edges
                    .iter()
                    .any(|&(step, next, _)| start + u32::from(step) == end && next == b)
            })
    };
    connects(before, a, pos, fid) && connects(pos, fid, after, b)
}

/// Pose une porte avec son encadrement puis du mobilier côté intérieur.
fn add_decor(
    plan: &mut RoomPlan,
    model: &StyleModel,
    anchors: &[(Axis, u32, u32)],
    front: &[(Axis, u32, u32)],
    (c0, r0, cols, rows): (u32, u32, u32, u32),
    opts: &DecorOptions,
) {
    let mut rng = Rng(opts.seed | 1);
    let hex_of = |c: i32, r: i32| {
        (c >= 0 && r >= 0 && (c as u32) < geom::HEX_GRID_WIDTH && (r as u32) < geom::HEX_GRID_WIDTH)
            .then(|| r as u32 * geom::HEX_GRID_WIDTH + c as u32)
    };
    let par = |c: u32, r: u32| ((c % 2) * 2 + (r % 2)) as u8;
    // Hexagones occupés : murs, puis portes et mobilier au fur et à mesure.
    let mut used: HashSet<u32> = plan.walls.iter().map(|w| w.0).collect();

    // Portes : sur la ligne d'un mur, loin des extrémités. L'entrée est sur un mur de devant s'il y en a ; les portes entre deux
    // pièces d'un bâtiment sont dans les murs du fond demandés.
    // (axe, colonne, ligne, rang dans son mur, longueur de son mur)
    type DoorSpot = (Axis, u32, u32, usize, usize);
    let describe = |list: &[(Axis, u32, u32)], start_c: u32, start_r: u32| -> Vec<DoorSpot> {
        list.iter()
            .map(|&(axis, c, r)| {
                let len = list.iter().filter(|a| a.0 == axis).count();
                let idx = if axis == Axis::Col {
                    c - start_c
                } else {
                    r - start_r
                } as usize;
                (axis, c, r, idx, len)
            })
            .collect()
    };
    let back_spots = describe(anchors, c0, r0);
    let mut requests: Vec<(Vec<DoorSpot>, Option<Axis>)> = Vec::new();
    if opts.door {
        requests.push((
            if front.is_empty() {
                back_spots.clone()
            } else {
                describe(front, c0 + 1, r0 + 1)
            },
            None,
        ));
    }
    if opts.door_in_col_wall {
        requests.push((back_spots.clone(), Some(Axis::Col)));
    }
    if opts.door_in_row_wall {
        requests.push((back_spots.clone(), Some(Axis::Row)));
    }
    for (spots, only_axis) in requests {
        let before = plan.doors.len();
        let existing: BTreeMap<u32, u32> = plan.walls.iter().copied().collect();
        let mut candidates = Vec::new();
        for (index, (d, count)) in model.doorways.iter().enumerate() {
            if only_axis.is_some_and(|axis| axis != d.axis) {
                continue;
            }
            for a in spots
                .iter()
                .filter(|a| a.0 == d.axis && (a.1 % 2) as u8 == d.col_par)
            {
                let Some(hex) = hex_of(a.1 as i32, a.2 as i32) else {
                    continue;
                };
                let at = |off: i8| {
                    if d.axis == Axis::Col {
                        hex_of(a.1 as i32 + off as i32, a.2 as i32)
                    } else {
                        hex_of(a.1 as i32, a.2 as i32 + off as i32)
                    }
                };
                let first = d.walls.first().unwrap();
                let last = d.walls.last().unwrap();
                let (Some(start), Some(end)) = (at(first.0), at(last.0)) else {
                    continue;
                };
                let (sc, sr) = geom::hex_col_row(start);
                let (ec, er) = geom::hex_col_row(end);
                if (d.axis == Axis::Col && (sc <= c0 || ec >= c0 + cols - 1))
                    || (d.axis == Axis::Row && (sr <= r0 || er >= r0 + rows))
                {
                    continue;
                }
                if existing.get(&start) != Some(&first.1) || existing.get(&end) != Some(&last.1) {
                    continue;
                }
                if d.frame_walls.iter().any(|&(dc, dr, fid)| {
                    hex_of(a.1 as i32 + dc as i32, a.2 as i32 + dr as i32)
                        .is_none_or(|h| existing.get(&h).is_some_and(|&f| f != fid))
                }) {
                    continue;
                }
                if plan.doors.iter().any(|p| {
                    p.hex >= start
                        && p.hex <= end
                        && (if d.axis == Axis::Col {
                            p.hex / 200 == a.2
                        } else {
                            p.hex % 200 == a.1
                        })
                }) {
                    continue;
                }
                candidates.push((*count, index, hex));
            }
        }
        if let Some(i) = rng.pick(&candidates.iter().map(|p| p.0).collect::<Vec<_>>()) {
            let (_, index, hex) = candidates[i];
            let d = &model.doorways[index].0;
            let (c, r) = geom::hex_col_row(hex);
            let start = d.walls.first().unwrap().0 as i32;
            let end = d.walls.last().unwrap().0 as i32;
            plan.walls.retain(|&(h, _)| {
                let (hc, hr) = geom::hex_col_row(h);
                let off = if d.axis == Axis::Col {
                    hc as i32 - c as i32
                } else {
                    hr as i32 - r as i32
                };
                !(off >= start
                    && off <= end
                    && (if d.axis == Axis::Col {
                        hr == r
                    } else {
                        hc == c
                    }))
            });
            for &(off, fid) in &d.walls {
                let h = if d.axis == Axis::Col {
                    hex_of(c as i32 + off as i32, r as i32)
                } else {
                    hex_of(c as i32, r as i32 + off as i32)
                };
                if let Some(h) = h {
                    plan.walls.push((h, fid));
                }
            }
            for &(dc, dr, fid) in &d.frame_walls {
                if let Some(h) = hex_of(c as i32 + dc as i32, r as i32 + dr as i32) {
                    plan.walls.retain(|p| p.0 != h);
                    plan.walls.push((h, fid));
                }
            }
            plan.doors.push(Placed {
                hex,
                fid: d.fid,
                orient: d.orient,
                offset: d.offset,
                flags: Some(d.flags),
            });
            used = plan
                .walls
                .iter()
                .map(|p| p.0)
                .chain(plan.doors.iter().map(|p| p.hex))
                .collect();
        }
        // Pas de porte connue pour ce mur : on ouvre un passage à la place d'une pièce de mur.
        if let (Some(axis), true) = (only_axis, plan.doors.len() == before) {
            let candidates: Vec<&DoorSpot> = spots
                .iter()
                .filter(|a| a.0 == axis && a.3 >= 2 && a.3 + 2 < a.4.max(3))
                .collect();
            if !candidates.is_empty() {
                let a = candidates[(rng.next() % candidates.len() as u64) as usize];
                if let Some(hex) = hex_of(a.1 as i32, a.2 as i32) {
                    plan.walls.retain(|w| w.0 != hex);
                    plan.openings += 1;
                }
            }
        }
    }

    // Fenêtres : une pièce du mur est remplacée par une variante qui laisse passer la lumière, loin des coins et espacées.
    let mut window_hexes: Vec<(Axis, u32, u32)> = Vec::new();
    for _ in 0..opts.windows * 8 {
        if plan.windows as u32 >= opts.windows || model.windows.is_empty() {
            break;
        }
        let Some(i) = rng.pick(&model.windows.iter().map(|w| w.3).collect::<Vec<_>>()) else {
            break;
        };
        let (axis, wpar, fid, _) = model.windows[i];
        let n_axis = anchors.iter().filter(|a| a.0 == axis).count();
        let usable: Vec<&(Axis, u32, u32)> = anchors
            .iter()
            .filter(|a| {
                let idx = if axis == Axis::Col {
                    a.1 - c0
                } else {
                    a.2 - r0
                } as usize;
                let own_par = if axis == Axis::Col { a.1 % 2 } else { a.2 % 2 } as u8;
                a.0 == axis && own_par == wpar && idx >= 2 && idx + 2 < n_axis.max(3)
            })
            .filter(|a| {
                plan.doors.iter().all(|p| {
                    let (dc, dr) = geom::hex_col_row(p.hex);
                    (dc as i32 - a.1 as i32)
                        .abs()
                        .max((dr as i32 - a.2 as i32).abs())
                        > 3
                })
            })
            .filter(|a| replacement_fits(plan, model, axis, a.1, a.2, fid))
            .filter(|a| {
                window_hexes.iter().all(|w| {
                    w.0 != axis
                        || (w.1 as i32 - a.1 as i32).abs() + (w.2 as i32 - a.2 as i32).abs() >= 3
                })
            })
            .collect();
        if usable.is_empty() {
            continue;
        }
        let a = usable[(rng.next() % usable.len() as u64) as usize];
        let Some(hex) = hex_of(a.1 as i32, a.2 as i32) else {
            continue;
        };
        if let Some(w) = plan.walls.iter_mut().find(|w| w.0 == hex) {
            w.1 = fid;
            plan.windows += 1;
            window_hexes.push((axis, a.1, a.2));
        }
    }

    // Mobilier : à l'intérieur, contre les murs.
    let (c_lo, c_hi, r_lo, r_hi) = (
        c0 as i32 + 1,
        (c0 + cols) as i32 - 1,
        r0 as i32 + 1,
        (r0 + rows) as i32 - 1,
    );
    let kind = opts.kind.filter(|&k| k < model.kinds.len()).or_else(|| {
        (!model.kinds.is_empty()).then(|| (rng.next() % model.kinds.len() as u64) as usize)
    });
    let kind_members = kind
        .and_then(|k| model.kinds.get(k))
        .map(|k| k.members.clone());
    // La source lumineuse des pièces se pose avec l'option « éclairage » : elle n'est pas un meuble.
    let light_fid = model.light.map(|l| l.fid);
    let furniture: Vec<&Decor> = model
        .decor
        .iter()
        .filter(|d| {
            !d.door
                && Some(d.fid) != light_fid
                && kind_members.as_ref().is_none_or(|ms| ms.contains(&d.fid))
        })
        .collect();
    let weights: Vec<u32> = furniture.iter().map(|d| d.count).collect();
    let mut tries = 0;
    while (plan.furniture.len() as u32) < opts.furniture && tries < opts.furniture * 30 {
        tries += 1;
        let Some(i) = rng.pick(&weights) else { break };
        let d = furniture[i];
        let compatible = |a: &&(Axis, u32, u32)| {
            a.0 == d.axis
                && plan
                    .walls
                    .iter()
                    .any(|&(hex, fid)| hex == a.2 * geom::HEX_GRID_WIDTH + a.1 && fid == d.wall)
        };
        let mut of_axis: Vec<&(Axis, u32, u32)> = anchors
            .iter()
            .filter(compatible)
            .filter(|a| par(a.1, a.2) == d.par)
            .collect();
        if of_axis.is_empty() {
            // Repli : aucun mur de cette parité (un type de pièce a peu d'exemples) : les écarts en colonnes et en lignes restent valables.
            of_axis = anchors.iter().filter(compatible).collect();
        }
        if of_axis.is_empty() {
            continue;
        }
        let a = of_axis[(rng.next() % of_axis.len() as u64) as usize];
        let (c, r) = (a.1 as i32 + d.dc as i32, a.2 as i32 + d.dr as i32);
        if c < c_lo || c > c_hi || r < r_lo || r > r_hi {
            continue;
        }
        let Some(hex) = hex_of(c, r) else { continue };
        // Laisser libre le seuil et le premier pas après chaque porte.
        if plan
            .doors
            .iter()
            .any(|door| geom::hex_distance(hex, door.hex) <= 1)
        {
            continue;
        }
        if !used.insert(hex) {
            continue;
        }
        plan.furniture.push(Placed::new(hex, d.fid, d.orient));
    }

    // Éclairage : des cellules régulières d'environ `spacing` hexagones, avec une source au centre de chacune.
    if opts.lights
        && let Some(light) = model.light
    {
        let step = light.spacing.max(1);
        let (w, h) = (cols.saturating_sub(2).max(1), rows.saturating_sub(1).max(1));
        let (n_c, n_r) = (
            ((w + step / 2) / step).max(1),
            ((h + step / 2) / step).max(1),
        );
        for j in 0..n_r {
            for i in 0..n_c {
                let c = c0 + 1 + (2 * i + 1) * w / (2 * n_c);
                let r = r0 + 1 + (2 * j + 1) * h / (2 * n_r);
                if let Some(hex) = hex_of(c as i32, r as i32)
                    && used.insert(hex)
                {
                    plan.lights.push(Placed::new(hex, light.fid, 0));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_windows_use_axis_parity_and_keep_both_neighbors() {
        let mut model = StyleModel::default();
        model.windows.push((Axis::Col, 0, 9, 20));
        model.edges.insert((Axis::Col, 1, 1), vec![(1, 9, 20)]);
        model.edges.insert((Axis::Col, 0, 9), vec![(1, 2, 20)]);
        let h = 51 * 200 + 52;
        let old = vec![(h - 1, 1), (h, 3), (h + 1, 2)];
        let plan = opening_at(&model, &old, h, true).unwrap();
        assert_eq!(plan.walls, vec![(h - 1, 1), (h, 9), (h + 1, 2)]);
        assert_eq!(plan.windows, 1);
    }

    #[test]
    fn joining_walls_requires_a_corner_compatible_with_both_axes() {
        let h = 100 * 200 + 50;
        let mut model = StyleModel::default();
        model.corner_choices.insert((3, 0), vec![(10, 9)]);
        model.edges.insert((Axis::Col, 0, 9), vec![(1, 1, 10)]);
        let mut walls = vec![(h, 3), (h + 1, 1), (h + 200, 2)];
        join_wall_corners(&model, &mut walls, &[h]);
        assert_eq!(walls[0].1, 3);
        model.edges.insert((Axis::Row, 0, 9), vec![(1, 2, 10)]);
        join_wall_corners(&model, &mut walls, &[h]);
        assert_eq!(walls[0].1, 9);
    }

    fn put(t: &mut HashMap<Key, u32>, step: u8, par: u8, a: u32, b: u32, n: u32) {
        *t.entry((step, par, a, b)).or_default() += n;
    }

    /// Statistiques synthétiques : le long des colonnes A↔B alternent à 1 hexagone (A sur parité 0) ; le long des lignes
    /// C↔D alternent aussi, mais le coin A est suivi d'un C.
    fn stats() -> WallStats {
        let mut s = WallStats::default();
        put(&mut s.col, 1, 0, 0xA, 0xB, 5);
        put(&mut s.col, 1, 1, 0xB, 0xA, 5);
        // Le long des lignes, le coin A (ligne paire) est suivi de C (ligne impaire), puis C↔D alternent.
        put(&mut s.row, 1, 1, 0xC, 0xD, 5);
        put(&mut s.row, 1, 0, 0xD, 0xC, 5);
        put(&mut s.row, 1, 0, 0xA, 0xC, 5);
        // Bruit isolé, sous le seuil.
        put(&mut s.col, 1, 0, 0xA, 0xE, 1);
        for f in [0xA, 0xB, 0xC, 0xD, 0xE] {
            s.total.insert(f, 10);
            s.pids.insert(f, 0x0300_0000 + f);
        }
        s
    }

    fn fids(p: &[Piece]) -> Vec<u32> {
        p.iter().map(|x| x.1).collect()
    }

    #[test]
    fn style_prefix_is_the_leading_letters() {
        assert_eq!(style_of("FEN7000.FRM"), "fen");
        assert_eq!(style_of("ybk1011.frm"), "ybk");
        assert_eq!(style_of("7up.frm"), "");
    }

    #[test]
    fn vault_axes_corners_and_door_frames_are_one_wall_family() {
        for name in [
            "VA1000.FRM",
            "vb4000.frm",
            "vcn1000.frm",
            "vt1000.frm",
            "vd1000.frm",
        ] {
            assert_eq!(wall_family(name), wall_family("va5001.frm"));
        }
        assert_ne!(wall_family("va5001.frm"), wall_family("VWLA000.FRM"));
        assert_ne!(wall_family("va5001.frm"), wall_family("velvdr01.frm"));
    }

    #[test]
    fn chains_alternate_with_the_parity() {
        let m = stats().model(&|_| true);
        assert!(m.is_usable());
        let (c, short) = m.chain(Axis::Col, 0, 5);
        assert_eq!(fids(&c), vec![0xA, 0xB, 0xA, 0xB, 0xA]);
        assert!(!short);
        assert_eq!(
            c.iter().map(|p| p.0).collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4]
        );
        assert_eq!(fids(&m.chain(Axis::Col, 1, 3).0), vec![0xB, 0xA, 0xB]);
        assert_eq!(m.pid_of(0xA), Some(0x0300_000A));
    }

    #[test]
    fn two_hex_steps_are_followed() {
        let mut s = WallStats::default();
        put(&mut s.col, 2, 0, 0x1, 0x2, 4);
        put(&mut s.col, 2, 0, 0x2, 0x1, 4);
        put(&mut s.row, 1, 0, 0x3, 0x3, 4);
        put(&mut s.row, 1, 1, 0x3, 0x3, 4);
        for f in [0x1, 0x2, 0x3] {
            s.total.insert(f, 9);
            s.pids.insert(f, f);
        }
        let m = s.model(&|_| true);
        let (c, short) = m.chain(Axis::Col, 0, 8);
        assert!(!short);
        assert_eq!(
            c,
            vec![(0, 0x1), (2, 0x2), (4, 0x1), (6, 0x2)],
            "une pièce tous les 2 hexagones"
        );
    }

    #[test]
    fn a_continuous_wall_keeps_the_one_hex_step() {
        let mut s = WallStats::default();
        // Mur continu A,B,A,B… : les couples à 2 hexagones (A→A) sont aussi nombreux que ceux à 1 (A→B).
        put(&mut s.col, 1, 0, 0xA, 0xB, 9);
        put(&mut s.col, 1, 1, 0xB, 0xA, 9);
        put(&mut s.col, 2, 0, 0xA, 0xA, 10);
        put(&mut s.col, 2, 1, 0xB, 0xB, 10);
        put(&mut s.row, 1, 0, 0xA, 0xA, 9);
        put(&mut s.row, 1, 1, 0xA, 0xA, 9);
        for f in [0xA, 0xB] {
            s.total.insert(f, 30);
        }
        let m = s.model(&|_| true);
        assert_eq!(
            m.chain(Axis::Col, 0, 4).0,
            vec![(0, 0xA), (1, 0xB), (2, 0xA), (3, 0xB)]
        );
    }

    #[test]
    fn a_chain_without_data_is_reported_short() {
        let mut s = stats();
        s.col.retain(|k, _| k.2 != 0xB);
        let m = s.model(&|_| true);
        let (c, short) = m.chain(Axis::Col, 0, 6);
        assert!(short);
        assert_eq!(fids(&c), vec![0xA, 0xB]);
    }

    #[test]
    fn style_filter_excludes_other_pieces() {
        let m = stats().model(&|f| f == 0xA || f == 0xB);
        assert!(!m.is_usable(), "aucun enchaînement de lignes dans ce style");
        assert!(m.pid_of(0xC).is_none());
    }

    #[test]
    fn room_plan_makes_an_l_of_walls_from_the_corner_and_a_floor_rectangle() {
        let m = stats().model(&|_| true);
        let corner = 100 * 200 + 50; // ligne 100, colonne 50 (colonne paire, ligne paire)
        let plan = plan_room(&m, corner, 3, 4);
        assert!(!plan.short);
        // 4 pièces à gauche (dont le coin A) puis les pièces du mur de droite, qui part du voisin du coin : C.
        assert_eq!(plan.walls[0], (corner, 0xA));
        assert_eq!(plan.walls[4], (corner + 200, 0xC));
        let mut hexes: Vec<u32> = plan.walls.iter().map(|w| w.0).collect();
        hexes.sort_unstable();
        hexes.dedup();
        assert_eq!(hexes.len(), plan.walls.len(), "pas de doublon");
        assert_eq!(plan.hexes.len(), 4 * 4);
        assert!(plan.walls.iter().all(|(h, _)| plan.hexes.contains(h)));
    }

    #[test]
    fn sets_group_touching_pieces_and_need_both_axes() {
        let names: Vec<String> = (0..0x30).map(|i| format!("nec{i:03}.frm")).collect();
        let mut s = stats();
        // Pièces d'une autre matière avec un seul axe : écartées.
        put(&mut s.col, 1, 0, 0x20, 0x21, 4);
        for f in [0x20, 0x21] {
            s.total.insert(f, 100);
        }
        let sets = s.sets(&names);
        assert!(sets.iter().all(|x| !x.members.contains(&0x20)));
        let set = sets
            .iter()
            .find(|x| x.members.contains(&0xA))
            .expect("A→C relie les colonnes et les lignes");
        assert!(
            set.members.contains(&0xB) && set.members.contains(&0xC) && set.members.contains(&0xD)
        );
        assert!(set.label.starts_with("nec"));
    }

    /// Statistiques avec du décor : un lit (0xB0) contre le mur de gauche (à 1 ligne, 1 colonne du mur) et une porte (0xD0) dans le
    /// mur de gauche (sur la ligne, 3 colonnes plus loin).
    fn stats_with_decor() -> WallStats {
        let mut s = stats();
        // Toutes les parités du mur A (0xA) : clé (mur, axe, parité, dr, dc, fid, orientation, porte).
        for par in 0..4u8 {
            s.decor.insert((0xA, 0, par, 1, 1, 0xB0, 2, false), 8);
            s.decor.insert((0xB, 0, par, 0, 3, 0xD0, 1, true), 8);
            s.decor.insert((0xC, 1, par, 2, 1, 0xB1, 3, false), 4);
        }
        for f in [0xB0, 0xB1, 0xD0] {
            s.decor_pids.insert(f, 0x0200_0000 + f);
        }
        for par in 0..2 {
            let side = if par == 0 { 0xB } else { 0xA };
            s.doorways.insert(
                Doorway {
                    axis: Axis::Col,
                    col_par: par,
                    fid: 0xD0,
                    orient: 1,
                    offset: (0, 0),
                    flags: 0,
                    walls: vec![(-1, side), (1, side)],
                    frame_walls: Vec::new(),
                },
                8,
            );
        }
        s
    }

    #[test]
    fn decor_is_aggregated_per_style_and_counted() {
        let m = stats_with_decor().model(&|_| true);
        assert_eq!(m.decor_counts(), (4, 8));
        assert_eq!(m.pid_of(0xB0), Some(0x0200_00B0));
        let none = stats_with_decor().model(&|f| f == 0xE);
        assert_eq!(
            none.decor_counts(),
            (0, 0),
            "aucun mur de ce style : pas de décor"
        );
    }

    #[test]
    fn furniture_and_a_door_stay_inside_the_room() {
        let m = stats_with_decor().model(&|_| true);
        let corner = 100 * 200 + 50;
        let opts = DecorOptions {
            door: true,
            furniture: 6,
            seed: 7,
            ..DecorOptions::default()
        };
        let plan = plan_room_with(&m, corner, 12, 12, &opts);
        assert!(!plan.furniture.is_empty(), "du mobilier est posé");
        let (c0, r0) = (50u32, 100u32);
        for p in &plan.furniture {
            let (c, r) = geom::hex_col_row(p.hex);
            assert!(
                c > c0 && c < c0 + 12 && r > r0 && r < r0 + 12,
                "{c},{r} est hors de l'intérieur"
            );
        }
        let mut hexes: Vec<u32> = plan
            .walls
            .iter()
            .map(|w| w.0)
            .chain(plan.doors.iter().map(|p| p.hex))
            .chain(plan.furniture.iter().map(|p| p.hex))
            .collect();
        let n = hexes.len();
        hexes.sort_unstable();
        hexes.dedup();
        assert_eq!(hexes.len(), n, "un seul objet par hexagone");
        assert_eq!(plan.doors.len(), 1);
        let door = plan.doors[0];
        assert!(
            plan.walls.iter().all(|w| w.0 != door.hex),
            "la porte remplace la pièce de mur"
        );
        assert_eq!(
            plan_room_with(&m, corner, 12, 12, &opts),
            plan,
            "même graine, même pièce"
        );
    }

    #[test]
    fn windows_replace_wall_pieces_in_a_solid_style_only() {
        let mut s = stats_with_decor();
        // Fenêtre 0xF0 : observée 6 fois sur les murs de gauche (parité 0), et utilisée 6 fois en tout.
        s.see_thru.insert((0xF0, 0, 0), 6);
        s.see_thru.insert((0xF0, 0, 1), 6);
        s.total.insert(0xF0, 12);
        s.pids.insert(0xF0, 0x0300_00F0);
        for (par, neighbor) in [(0, 0xA), (1, 0xB)] {
            put(&mut s.col, 1, par, neighbor, 0xF0, 2);
            put(&mut s.col, 1, 1 - par, 0xF0, neighbor, 2);
        }
        for f in [0xA, 0xB, 0xC, 0xD, 0xE] {
            s.total.insert(f, 200);
        }
        let m = s.model(&|_| true);
        assert!(m.window_count() > 0);
        let opts = DecorOptions {
            windows: 2,
            seed: 3,
            ..DecorOptions::default()
        };
        let plan = plan_room_with(&m, 100 * 200 + 50, 12, 14, &opts);
        assert_eq!(plan.windows, 2);
        assert_eq!(plan.walls.iter().filter(|w| w.1 == 0xF0).count(), 2);
        assert!(
            plan.walls.first().is_some_and(|w| w.1 != 0xF0),
            "pas de fenêtre dans le coin"
        );
        // Dans un style où presque tout laisse passer la lumière (clôture), il n'y a pas de fenêtre.
        let mut fence = s.clone();
        for f in [0xA, 0xB, 0xC, 0xD, 0xE] {
            fence.see_thru.insert((f, 0, 0), 200);
        }
        assert_eq!(fence.model(&|_| true).window_count(), 0);
    }

    #[test]
    fn room_kinds_group_furniture_that_appears_together() {
        let mut s = stats_with_decor();
        // Un lit (0xB0), une table de nuit (0xB1, déjà dans les statistiques) et une armoire (0xB2) vont ensemble ; 0xC0 est ailleurs.
        for par in 0..4u8 {
            s.decor.insert((0xA, 0, par, 1, 2, 0xB2, 1, false), 6);
            s.decor.insert((0xA, 0, par, 1, 3, 0xC0, 1, false), 6);
        }
        for f in [0xB2, 0xC0] {
            s.decor_pids.insert(f, 0x0200_0000 + f);
        }
        for (f, n) in [(0xB0, 30), (0xB1, 30), (0xB2, 30), (0xC0, 30), (0xD1, 400)] {
            s.furniture_count.insert(f, n);
        }
        s.furniture_total = 2000;
        s.pairs.insert((0xB0, 0xB1), 25);
        s.pairs.insert((0xB0, 0xB2), 22);
        s.pairs.insert((0xB1, 0xB2), 20);
        s.pairs.insert((0xB0, 0xC0), 1);
        // La table de nuit existe aussi près d'un meuble d'un autre ensemble,
        // mais ce dernier ne doit pas rejoindre le groupe du lit.
        s.pairs.insert((0xB1, 0xC0), 25);
        let m = s.model(&|_| true);
        let kind = m.kinds().first().expect("un type de pièce");
        assert_eq!(kind.seed, 0xB0);
        assert!(
            kind.members.contains(&0xB1)
                && kind.members.contains(&0xB2)
                && !kind.members.contains(&0xC0)
        );
        let opts = DecorOptions {
            furniture: 12,
            kind: Some(0),
            seed: 11,
            ..DecorOptions::default()
        };
        let plan = plan_room_with(&m, 100 * 200 + 50, 14, 14, &opts);
        assert!(!plan.furniture.is_empty());
        assert!(
            plan.furniture.iter().all(|p| kind.members.contains(&p.fid)),
            "mobilier du type seulement : {:?}",
            plan.furniture
        );
        let any = plan_room_with(
            &m,
            100 * 200 + 50,
            14,
            14,
            &DecorOptions {
                furniture: 30,
                seed: 11,
                ..DecorOptions::default()
            },
        );
        assert!(
            any.furniture.iter().all(|p| kind.members.contains(&p.fid)),
            "le choix automatique garde un ensemble cohérent"
        );
    }

    #[test]
    fn furniture_learning_ignores_hidden_props_loose_items_and_lamps() {
        use crate::map::{MapObject, flags, word};
        let object = |fid, ty, subtype, col, row, flags_value| {
            let mut words = [0; crate::map::OBJECT_WORDS];
            words[word::TILE] = row * 200 + col;
            words[word::FID] = fid;
            words[word::PID] = (ty << 24) | fid;
            words[word::FLAGS] = flags_value;
            MapObject {
                words,
                extra: Vec::new(),
                inventory: Vec::new(),
                subtype,
            }
        };
        let mut lamp = object(3, 2, 5, 14, 11, 0);
        lamp.words[word::LIGHT_DISTANCE] = 4;
        lamp.words[word::LIGHT_INTENSITY] = 65536;
        let props = vec![
            object(1, 2, 5, 11, 11, 0),
            object(2, 2, 5, 12, 11, flags::HIDDEN as i32),
            lamp,
            object(4, 0, crate::formats::item_kind::WEAPON, 13, 11, 0),
            object(5, 0, crate::formats::item_kind::CONTAINER, 15, 11, 0),
        ];
        let walls: HashMap<_, _> = (10..=17).map(|c| ((c, 10), vec![0xA])).collect();
        let mut stats = WallStats::default();
        stats.add_decor(&props, &walls);
        assert_eq!(
            stats.decor.keys().map(|k| k.5).collect::<HashSet<_>>(),
            HashSet::from([1, 5])
        );
    }

    #[test]
    fn furniture_pairs_do_not_cross_partitions_or_depend_on_object_order() {
        use crate::map::{MapObject, word};
        let object = |fid, col| {
            let mut words = [0; crate::map::OBJECT_WORDS];
            words[word::TILE] = 11 * 200 + col;
            words[word::FID] = fid;
            words[word::PID] = 0x02000000 | fid;
            MapObject {
                words,
                extra: Vec::new(),
                inventory: Vec::new(),
                subtype: 5,
            }
        };
        let mut walls: HashMap<_, _> = (8..=18).map(|c| ((c, 10), vec![0xA])).collect();
        let props = vec![object(2, 13), object(1, 11)]; // reverse order, separate spatial bins
        let mut stats = WallStats::default();
        stats.add_decor(&props, &walls);
        assert_eq!(stats.pairs.get(&(1, 2)), Some(&1));
        walls.insert((12, 12), vec![0xC]); // partition has a normal two-hex panel gap
        assert!(!decor_same_room((11, 11), (13, 11), &walls));
        let mut partitioned = WallStats::default();
        partitioned.add_decor(&props, &walls);
        assert!(partitioned.pairs.is_empty());
    }

    #[test]
    fn generated_furniture_leaves_door_thresholds_free() {
        let model = stats_with_decor().model(&|_| true);
        for seed in 1..=24 {
            let plan = plan_room_with(
                &model,
                20050,
                12,
                12,
                &DecorOptions {
                    door: true,
                    furniture: 20,
                    seed,
                    ..DecorOptions::default()
                },
            );
            assert!(!plan.doors.is_empty());
            assert!(plan.furniture.iter().all(|p| {
                plan.doors
                    .iter()
                    .all(|d| geom::hex_distance(p.hex, d.hex) > 1)
            }));
        }
    }

    #[test]
    fn a_building_shares_partitions_and_connects_its_rooms() {
        let m = stats_with_decor().model(&|_| true);
        let corner = 100 * 200 + 50;
        let (rows, cols) = (8, 8);
        let opts = DecorOptions {
            furniture: 3,
            lights: false,
            seed: 9,
            ..DecorOptions::default()
        };
        let b = plan_building(&m, corner, 3, 2, rows, cols, &opts);
        let single = plan_room_with(
            &m,
            corner,
            rows,
            cols,
            &DecorOptions {
                front: true,
                ..opts.clone()
            },
        );
        assert!(
            b.walls.len() > 4 * single.walls.len() / 2,
            "six pièces : bien plus de murs qu'une"
        );
        // Aucun hexagone n'est occupé deux fois (cloisons partagées, pas doublées).
        let mut hexes: Vec<u32> = b
            .walls
            .iter()
            .map(|w| w.0)
            .chain(b.doors.iter().map(|p| p.hex))
            .chain(b.furniture.iter().map(|p| p.hex))
            .collect();
        let n = hexes.len();
        hexes.sort_unstable();
        hexes.dedup();
        assert_eq!(hexes.len(), n, "un seul objet par hexagone");
        // Portes : une par frontière entre pièces (3 × 2 pièces : 2 verticales par rangée × 2 rangées + 3 horizontales = 7) plus l'entrée.
        assert!(
            b.doors.len() + b.openings >= 7,
            "{} portes, {} passages",
            b.doors.len(),
            b.openings
        );
        assert!(
            b.openings > 0,
            "ces statistiques n'ont pas de porte pour le mur des lignes : des passages sont ouverts"
        );
        // L'entrée est sur la façade (dernière ligne ou dernière colonne du bâtiment).
        let (c_end, r_end) = (50 + 3 * cols - 1, 100 + 2 * (rows + 1) - 1);
        assert!(
            b.doors
                .iter()
                .any(|d| d.hex / 200 == r_end || d.hex % 200 == c_end),
            "porte d'entrée sur la façade"
        );
        // Tout reste dans le rectangle du bâtiment.
        for h in b
            .walls
            .iter()
            .map(|w| w.0)
            .chain(b.furniture.iter().map(|p| p.hex))
        {
            let (c, r) = (h % 200, h / 200);
            assert!(
                (50..=c_end).contains(&c) && (100..=r_end + 1).contains(&r),
                "{c},{r} hors du bâtiment"
            );
        }
        assert_eq!(
            plan_building(&m, corner, 3, 2, rows, cols, &opts),
            b,
            "même graine, même bâtiment"
        );
        // Un bâtiment 1×1 est la pièce simple, fermée sur ses quatre côtés.
        assert_eq!(
            plan_building(&m, corner, 1, 1, rows, cols, &opts),
            plan_room_with(
                &m,
                corner,
                rows,
                cols,
                &DecorOptions {
                    front: true,
                    ..opts.clone()
                }
            )
        );
    }

    #[test]
    fn varied_room_sizes_keep_partitions_aligned_and_never_overlap() {
        let m = stats_with_decor().model(&|_| true);
        let corner = 100 * 200 + 50;
        let opts = DecorOptions {
            vary_sizes: true,
            furniture: 2,
            lights: false,
            seed: 4,
            ..DecorOptions::default()
        };
        let b = plan_building(&m, corner, 3, 3, 8, 8, &opts);
        let fixed = plan_building(
            &m,
            corner,
            3,
            3,
            8,
            8,
            &DecorOptions {
                vary_sizes: false,
                ..opts.clone()
            },
        );
        assert_ne!(b.hexes.len(), fixed.hexes.len(), "les tailles changent");
        let mut seen: Vec<u32> = b
            .walls
            .iter()
            .map(|w| w.0)
            .chain(b.doors.iter().map(|p| p.hex))
            .chain(b.furniture.iter().map(|p| p.hex))
            .collect();
        let n = seen.len();
        seen.sort_unstable();
        let dups: Vec<(u32, u32, &str, &str)> = seen
            .windows(2)
            .filter(|w| w[0] == w[1])
            .map(|w| {
                let kind = |h: u32| {
                    if b.walls.iter().any(|x| x.0 == h) {
                        "mur"
                    } else if b.doors.iter().any(|x| x.hex == h) {
                        "porte"
                    } else {
                        "meuble"
                    }
                };
                let n_wall = b.walls.iter().filter(|x| x.0 == w[0]).count();
                (
                    w[0] % 200,
                    w[0] / 200,
                    kind(w[0]),
                    if n_wall > 1 { "mur x2" } else { "-" },
                )
            })
            .collect();
        seen.dedup();
        assert_eq!(seen.len(), n, "un seul objet par hexagone : {dups:?}");
        // Les rangées d'une même colonne ont la même largeur : les murs de droite sont alignés sur quelques colonnes seulement.
        let (mut rng, mut rng2) = (
            Rng(opts.seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1),
            Rng(1),
        );
        let (w, d) = room_sizes(&mut rng, 3, 3, 8, 8, true);
        assert!(w.iter().all(|&x| x >= 5) && d.iter().all(|&x| x >= 4));
        assert_eq!(
            room_sizes(&mut rng2, 3, 2, 8, 6, false),
            (vec![8, 8, 8], vec![6, 6])
        );
        assert_eq!(starts(&[5, 8, 6], 0), vec![0, 5, 13]);
        assert_eq!(starts(&[4, 4], 1), vec![0, 5]);
    }

    #[test]
    fn two_floors_share_their_footprint_with_a_shared_size_seed() {
        let m = stats_with_decor().model(&|_| true);
        let corner = 100 * 200 + 50;
        let lower = DecorOptions {
            vary_sizes: true,
            seed: 11,
            ..DecorOptions::default()
        };
        let upper = DecorOptions {
            vary_sizes: true,
            seed: 99,
            size_seed: Some(11),
            ..DecorOptions::default()
        };
        let (a, b) = (
            plan_building(&m, corner, 3, 2, 8, 8, &lower),
            plan_building(&m, corner, 3, 2, 8, 8, &upper),
        );
        let (mut ha, mut hb) = (a.hexes.clone(), b.hexes.clone());
        ha.sort_unstable();
        hb.sort_unstable();
        assert_eq!(ha, hb, "même empreinte");
        // Sans graine de tailles commune, la même graine de pièces donnerait une autre empreinte.
        let other = plan_building(
            &m,
            corner,
            3,
            2,
            8,
            8,
            &DecorOptions {
                size_seed: None,
                ..upper.clone()
            },
        );
        let mut ho = other.hexes.clone();
        ho.sort_unstable();
        assert_ne!(ho, ha, "l'empreinte dépend bien de la graine des tailles");
    }

    #[test]
    fn stair_destination_matches_the_game_encoding() {
        // denbus1 : l'escalier de l'élévation 0 mène à l'élévation 1, hexagone 17066 ; l'autre revient à l'élévation 0, hexagone 16661.
        assert_eq!(stair_destination(1, 17_066), 738_214_570);
        assert_eq!(stair_destination(0, 16_661), 201_343_253);
        assert_eq!(stair_destination(2, 13_669), 1_275_082_085);
    }

    #[test]
    fn stairs_go_on_a_free_interior_hex_of_both_floors() {
        let mut s = stats_with_decor();
        s.stairs.insert((true, 0x0200_0166, 0x0200_0164), 9);
        s.stairs.insert((false, 0x0200_015E, 0x0200_015C), 7);
        let m = s.model(&|_| true);
        let corner = 100 * 200 + 50;
        let opts = DecorOptions {
            furniture: 6,
            lights: true,
            seed: 3,
            ..DecorOptions::default()
        };
        let lower = plan_building(&m, corner, 2, 1, 8, 8, &opts);
        let upper = plan_building(
            &m,
            corner,
            2,
            1,
            8,
            8,
            &DecorOptions {
                seed: 77,
                ..opts.clone()
            },
        );
        let st = plan_stairs(&m, &lower, &upper, 5).expect("des escaliers");
        let all = |p: &RoomPlan| -> Vec<u32> {
            p.walls
                .iter()
                .map(|w| w.0)
                .chain(p.doors.iter().map(|d| d.hex))
                .chain(p.furniture.iter().map(|f| f.hex))
                .chain(p.lights.iter().map(|l| l.hex))
                .collect()
        };
        for plan in [&lower, &upper] {
            assert!(
                !all(plan).contains(&st.hex) && !all(plan).contains(&st.arrive_up),
                "cases libres sur les deux étages"
            );
        }
        assert!(lower.hexes.contains(&st.hex) && upper.hexes.contains(&st.hex));
        assert_eq!(st.arrive_up, st.hex + 1);
        assert_eq!(
            plan_stairs(&m, &lower, &upper, 5),
            Some(st),
            "reproductible"
        );
        // Sans escaliers appris : rien.
        assert!(plan_stairs(&stats_with_decor().model(&|_| true), &lower, &upper, 5).is_none());
        // Un étage sans case commune libre : rien.
        assert!(plan_stairs(&m, &lower, &RoomPlan::default(), 5).is_none());
    }

    #[test]
    fn stairs_are_learned_by_direction() {
        let mut s = stats();
        s.stairs.insert((true, 0x0200_0166, 0x0200_0164), 9);
        s.stairs.insert((true, 0x0200_0170, 0x0200_0170), 2);
        s.stairs.insert((false, 0x0200_015E, 0x0200_015C), 7);
        let m = s.model(&|_| true);
        assert_eq!(
            m.stairs(),
            Some(StairKinds {
                up_fid: 0x0200_0166,
                up_pid: 0x0200_0164,
                down_fid: 0x0200_015E,
                down_pid: 0x0200_015C
            })
        );
        assert_eq!(m.pid_of(0x0200_0166), Some(0x0200_0164));
        let mut only_up = stats();
        only_up.stairs.insert((true, 1, 1), 5);
        assert_eq!(
            only_up.model(&|_| true).stairs(),
            None,
            "il faut les deux sens"
        );
    }

    #[test]
    fn furniture_still_found_when_the_wall_parity_has_no_example() {
        let mut s = stats();
        // Un seul meuble appris, sur des murs de parité 3 seulement ; la pièce est posée sur un coin de parité 0.
        s.decor.insert((0xA, 0, 3, 1, 1, 0xB0, 2, false), 9);
        s.decor_pids.insert(0xB0, 0x0200_00B0);
        let m = s.model(&|_| true);
        let plan = plan_room_with(
            &m,
            100 * 200 + 50,
            12,
            12,
            &DecorOptions {
                furniture: 3,
                seed: 5,
                ..DecorOptions::default()
            },
        );
        assert!(
            !plan.furniture.is_empty(),
            "le repli sans parité pose quand même du mobilier"
        );
    }

    #[test]
    fn light_sources_are_learned_and_placed_on_a_grid() {
        let mut s = stats_with_decor();
        s.lights.insert(0x15, (500, 0x0200_0015));
        s.lights.insert(0x0C, (30, 0x0200_000C));
        s.light_gaps.insert(0x15, vec![100, 110, 120, 110, 140]); // médiane 110 px → 5 hexagones
        s.light_params.insert((0x15, 3, 65_536), 480);
        s.light_params.insert((0x15, 8, 65_536), 20);
        let m = s.model(&|_| true);
        assert_eq!(
            m.light(),
            Some(RoomLight {
                fid: 0x15,
                spacing: 5,
                distance: 3,
                intensity: 65_536
            })
        );
        assert_eq!(m.pid_of(0x15), Some(0x0200_0015));
        let opts = DecorOptions {
            lights: true,
            ..DecorOptions::default()
        };
        let plan = plan_room_with(&m, 100 * 200 + 50, 14, 14, &opts);
        assert!(
            plan.lights.len() >= 4,
            "plusieurs sources dans une grande pièce : {}",
            plan.lights.len()
        );
        let small = plan_room_with(&m, 100 * 200 + 50, 8, 8, &opts);
        assert!(
            !small.lights.is_empty(),
            "au moins une source dans une pièce de taille moyenne"
        );
        for p in &plan.lights {
            let (c, r) = geom::hex_col_row(p.hex);
            assert!(
                c > 50 && c < 63 && r > 100 && r < 114,
                "à l'intérieur : {c},{r}"
            );
        }
        let few = plan_room_with(&m, 100 * 200 + 50, 4, 4, &opts);
        assert!(
            few.lights.len() <= 1,
            "peu de sources dans une petite pièce"
        );
        // Trop peu de sources observées : pas de lumière automatique.
        let mut rare = stats_with_decor();
        rare.lights.insert(0x15, (3, 0x0200_0015));
        assert_eq!(rare.model(&|_| true).light(), None);
    }

    #[test]
    fn front_walls_close_the_other_two_sides() {
        let m = stats_with_decor().model(&|_| true);
        let corner = 100 * 200 + 50;
        let (c0, r0, cols, rows) = (50u32, 100u32, 10u32, 8u32);
        let back = plan_room_with(&m, corner, rows, cols, &DecorOptions::default());
        let opts = DecorOptions {
            front: true,
            door: true,
            seed: 5,
            ..DecorOptions::default()
        };
        let plan = plan_room_with(&m, corner, rows, cols, &opts);
        assert!(
            plan.walls.len() > back.walls.len(),
            "des pièces de devant s'ajoutent"
        );
        let on_front_right =
            |h: u32| h / 200 == r0 + rows && (c0 + 1..c0 + cols).contains(&(h % 200));
        let on_front_left =
            |h: u32| h % 200 == c0 + cols - 1 && (r0 + 1..r0 + rows).contains(&(h / 200));
        assert!(plan.walls.iter().any(|w| on_front_right(w.0)));
        assert!(plan.walls.iter().any(|w| on_front_left(w.0)));
        let mut hexes: Vec<u32> = plan
            .walls
            .iter()
            .map(|w| w.0)
            .chain(plan.doors.iter().map(|p| p.hex))
            .collect();
        let n = hexes.len();
        hexes.sort_unstable();
        hexes.dedup();
        assert_eq!(hexes.len(), n, "un seul objet par hexagone");
        let door = plan.doors[0];
        assert!(
            on_front_right(door.hex) || on_front_left(door.hex),
            "avec des murs de devant, la porte est sur l'un d'eux : {}",
            door.hex
        );
        assert_eq!(plan_room_with(&m, corner, rows, cols, &opts), plan);
    }

    #[test]
    fn room_at_the_map_edge_is_clipped() {
        let m = stats().model(&|_| true);
        let plan = plan_room(&m, 199, 2, 5);
        assert!(plan.walls.iter().all(|(h, _)| *h < geom::HEX_COUNT));
        assert!(plan.walls.iter().all(|(h, _)| h % 200 == 199 || *h >= 199));
    }

    #[test]
    fn blockers_do_not_turn_a_corner_into_a_junction_or_skip_a_piece() {
        use crate::map::{MapObject, OBJECT_WORDS, flags, word};
        let wall = |c, r, fid, flags| {
            let mut o = MapObject {
                words: [0; OBJECT_WORDS],
                extra: Vec::new(),
                inventory: Vec::new(),
                subtype: -1,
            };
            o.words[word::PID] = 0x0300_0001;
            o.words[word::FID] = fid;
            o.words[word::TILE] = r * 200 + c;
            o.words[word::FLAGS] = flags;
            o
        };
        let mut map = Map::new_empty("walls", 1);
        map.objects[0] = vec![
            wall(50, 50, 1, 0),
            wall(51, 50, 2, 0),
            wall(52, 50, 3, 0),
            wall(50, 52, 4, 0),
            wall(50, 49, 5, flags::FLAT as i32),
            wall(49, 50, 6, flags::HIDDEN as i32),
        ];
        let mut stats = WallStats::default();
        stats.add_map(&map);
        assert_eq!(stats.corners.get(&(3, 0, 1)), Some(&1));
        assert!(!stats.total.contains_key(&5) && !stats.total.contains_key(&6));
        assert!(stats.col.contains_key(&(1, 0, 1, 2)));
        assert!(
            !stats.col.contains_key(&(2, 0, 1, 3)),
            "le voisin à deux cases n'est pas un raccord direct"
        );
        assert_eq!(
            stats.row.get(&(2, 0, 1, 4)),
            stats.row.get(&(2, 1, 1, 4)),
            "une translation en ligne préserve le raccord"
        );
    }

    #[test]
    fn wall_search_keeps_an_alternative_that_reaches_the_end_corner() {
        let mut m = StyleModel::default();
        m.edges
            .insert((Axis::Col, 0, 1), vec![(1, 2, 20), (1, 3, 5)]);
        m.edges.insert((Axis::Col, 1, 2), vec![(1, 5, 20)]);
        m.edges.insert((Axis::Col, 1, 3), vec![(1, 4, 5)]);
        assert_eq!(
            wall_between(&m, Axis::Col, 0, 1, 4, 2).unwrap().1,
            vec![(0, 1), (1, 3), (2, 4)]
        );
        assert!(
            wall_between(&m, Axis::Col, 0, 1, 4, 3).is_none(),
            "pas d'angle imposé à une distance non observée"
        );
    }

    #[test]
    fn a_broken_chain_after_a_corner_is_reported_short() {
        let mut m = stats().model(&|_| true);
        m.corners.insert((3, 0), 0xF);
        m.corner_next.insert((Axis::Col, 0, 0xF), (1, 0xE));
        m.corner_next.insert((Axis::Row, 0, 0xF), (1, 0xE));
        assert!(plan_room(&m, 20_050, 8, 8).short);
    }

    #[test]
    fn light_through_door_jamb_is_not_a_single_panel_window() {
        let mut s = stats();
        s.see_thru.insert((0xF0, 0, 0), 10);
        s.total.insert(0xF0, 10);
        s.pids.insert(0xF0, 0x0300_00F0);
        for fid in [0xA, 0xB, 0xC, 0xD] {
            s.total.insert(fid, 200);
        }
        let m = s.model(&|_| true);
        assert!(m.window_count() > 0);
        let plan = plan_room_with(
            &m,
            20_050,
            12,
            14,
            &DecorOptions {
                windows: 2,
                ..Default::default()
            },
        );
        assert_eq!(
            plan.windows, 0,
            "un jambage sans raccords des deux côtés ne doit pas trouer un mur"
        );
    }

    #[test]
    fn a_door_keeps_its_frame_even_when_the_frame_is_from_another_art_family() {
        let mut s = stats_with_decor();
        s.doorways.clear();
        s.pids.insert(0xF1, 0x0300_00F1);
        s.total.insert(0xF1, 8);
        s.doorways.insert(
            Doorway {
                axis: Axis::Col,
                col_par: 0,
                fid: 0xD0,
                orient: 1,
                offset: (3, -2),
                flags: 0,
                walls: vec![(-1, 0xB), (1, 0xB)],
                frame_walls: vec![(0, 1, 0xF1)],
            },
            8,
        );
        let m = s.model(&|fid| fid != 0xF1);
        assert_eq!(m.pid_of(0xF1), Some(0x0300_00F1));
        let plan = plan_room_with(
            &m,
            20_050,
            12,
            12,
            &DecorOptions {
                front: true,
                door: true,
                ..Default::default()
            },
        );
        let door = plan
            .doors
            .first()
            .expect("porte avec un segment raccordable");
        assert_eq!(door.offset, (3, -2));
        assert!(plan.walls.contains(&(door.hex + 200, 0xF1)));
        assert!(!plan.walls.iter().any(|p| p.0 == door.hex));
    }
}
