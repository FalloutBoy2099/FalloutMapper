//! Étirement d'un groupe d'objets (une pièce, un bâtiment) le long des deux axes isométriques de l'écran.
//!
//! Les hexagones forment un réseau dont les pas élémentaires sont (32, 0), (16, 12) et (−16, 12). On travaille avec les
//! deux axes en diagonale : `u` = (+16, +12) et `v` = (−16, +12). Le groupe est découpé en trois bandes sur chaque axe (début,
//! milieu, fin) ; la bande du milieu est répétée pour agrandir, et la fin est décalée d'autant. Chaque objet reste ainsi sur
//! un hexagone du réseau et les murs des bords gardent leur forme.

use crate::geom;

/// Un élément du résultat : l'objet d'origine (indice dans la liste donnée) et son nouvel hexagone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub source: usize,
    pub hex: u32,
}

#[derive(Clone, Copy)]
struct Item {
    source: usize,
    u: i32,
    v: i32,
}

fn centre(hex: u32) -> (i32, i32) {
    geom::hex_to_screen(hex)
}

/// Étire le groupe de `hexes` de `du` pas le long de `u` et de `dv` pas le long de `v` (arrondis au multiple de la largeur de la
/// bande répétée). Les résultats hors de la carte sont ignorés. L'ordre : d'abord u, puis v.
pub fn stretch(hexes: &[u32], du: u32, dv: u32) -> Vec<Placed> {
    let Some(&first) = hexes.first() else {
        return Vec::new();
    };
    let (fx, fy) = centre(first);
    let mut items: Vec<Item> = hexes
        .iter()
        .enumerate()
        .map(|(source, &h)| {
            let (x, y) = centre(h);
            // Décalage en pixels → coordonnées (u, v) : x = 16 (u − v), y = 12 (u + v).
            let (a, b) = ((y - fy) / 12, (x - fx) / 16);
            Item {
                source,
                u: (a + b) / 2,
                v: (a - b) / 2,
            }
        })
        .collect();
    items = stretch_axis(items, du, |i| i.u, |i, d| i.u += d);
    items = stretch_axis(items, dv, |i| i.v, |i, d| i.v += d);
    items
        .into_iter()
        .filter_map(|i| {
            let (x, y) = (fx + 16 * (i.u - i.v), fy + 12 * (i.u + i.v));
            geom::screen_to_hex(x, y).map(|hex| Placed {
                source: i.source,
                hex,
            })
        })
        .collect()
}

fn stretch_axis(
    items: Vec<Item>,
    extra: u32,
    get: impl Fn(&Item) -> i32,
    shift: impl Fn(&mut Item, i32),
) -> Vec<Item> {
    if extra == 0 || items.is_empty() {
        return items;
    }
    let (lo, hi) = items.iter().fold((i32::MAX, i32::MIN), |(l, h), i| {
        (l.min(get(i)), h.max(get(i)))
    });
    let span = hi - lo;
    // Bande du milieu : au moins un pas, environ un quart de la longueur.
    let width = (span / 4).max(1);
    let start = lo + (span - width) / 2 + 1;
    let end = start + width; // exclusive
    let repeats = (extra as i32 + width - 1) / width;
    let mut out = Vec::with_capacity(items.len() + 8);
    for it in &items {
        let p = get(it);
        let mut base = *it;
        if p >= end {
            shift(&mut base, repeats * width);
        }
        out.push(base);
        if (start..end).contains(&p) {
            for k in 1..=repeats {
                let mut copy = *it;
                shift(&mut copy, k * width);
                out.push(copy);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hexagone situé à `u` pas le long de (16, 12) et `v` pas le long de (−16, 12) depuis `from`.
    fn step(from: u32, u: i32, v: i32) -> u32 {
        let (x, y) = geom::hex_to_screen(from);
        geom::screen_to_hex(x + 16 * (u - v), y + 12 * (u + v)).expect("dans la carte")
    }

    #[test]
    fn no_stretch_returns_the_same_hexes() {
        let base = 10_000;
        let hexes: Vec<u32> = (0..5).map(|i| step(base, i, 0)).collect();
        let out = stretch(&hexes, 0, 0);
        assert_eq!(out.iter().map(|p| p.hex).collect::<Vec<_>>(), hexes);
    }

    #[test]
    fn a_wall_line_gets_longer_and_keeps_its_ends() {
        let base = 10_000;
        // Un mur de 9 éléments le long de u.
        let hexes: Vec<u32> = (0..9).map(|i| step(base, i, 0)).collect();
        let out = stretch(&hexes, 4, 0);
        assert!(out.len() > hexes.len(), "des éléments sont ajoutés");
        // Le premier élément reste en place ; le dernier est décalé de la largeur ajoutée.
        assert_eq!(out.iter().find(|p| p.source == 0).unwrap().hex, hexes[0]);
        let last = out
            .iter()
            .filter(|p| p.source == 8)
            .map(|p| p.hex)
            .next()
            .unwrap();
        assert_ne!(last, hexes[8]);
        let added = (out.len() - hexes.len()) as i32;
        assert_eq!(
            last,
            step(base, 8 + added, 0),
            "la fin est décalée d'autant que les éléments ajoutés"
        );
        // Les positions sont toutes différentes : une ligne continue, sans doublon.
        let mut seen: Vec<u32> = out.iter().map(|p| p.hex).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), out.len());
    }

    #[test]
    fn stretching_the_other_axis_leaves_the_first_axis_alone() {
        let base = 10_000;
        let hexes: Vec<u32> = (0..6).map(|i| step(base, 0, i)).collect();
        let out = stretch(&hexes, 5, 0);
        assert_eq!(
            out.iter().map(|p| p.hex).collect::<Vec<_>>(),
            hexes,
            "tout le groupe est sur une seule valeur de u : rien à répéter"
        );
    }
}
