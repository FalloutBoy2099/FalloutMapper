//! Sélection et opérations de groupe : clic (avec parcours des éléments superposés), sélection par zone, tout
//! sélectionner, déplacement / suppression / rotation / duplication de plusieurs objets, remplissage de cases.

use crate::app::MapperApp;
use crate::edit_ui::SelectMode;
use crate::selection::{Combine, Pick, TileRef};
use eframe::egui::{Modifiers, Pos2};
use fo_core::geom;
use fo_core::map::{TileLayer, word};
use fo_scene::{Layer, ObjectRef, Scene, object_layer};
use std::sync::Arc;

/// Distance (en pixels du monde) en deçà de laquelle deux clics visent « le même endroit ».
const SAME_SPOT: f32 = 4.0;

impl MapperApp {
    // ---------------------------------------------------------------- désignation

    /// Éléments sous un point du monde, du dessus vers le dessous : toit, objets (le plus en avant d'abord), sol.
    /// Seuls les types activés dans le filtre de sélection et visibles sont proposés.
    pub(crate) fn pick_candidates(&self, scene: &Scene, w: Pos2) -> Vec<Pick> {
        let (x, y) = (w.x as i32, w.y as i32);
        let e = self.elevation;
        let Some(cur) = self.current.as_ref() else {
            return Vec::new();
        };
        match self.edit.select_mode {
            SelectMode::Hexes => {
                return geom::screen_to_hex(x, y)
                    .map(Pick::Hex)
                    .into_iter()
                    .collect();
            }
            SelectMode::Blockers => {
                let blockers = scene
                    .items
                    .iter()
                    .rev()
                    .filter_map(|i| i.object.map(|r| (i, r)))
                    .filter(|(i, r)| {
                        cur.map.objects[e]
                            .get(r.index as usize)
                            .is_some_and(|o| o.is_scroll_blocker())
                            && i.sprite.alpha_at(x - i.x, y - i.y) > 16
                    });
                let mut out: Vec<Pick> = blockers.map(|(_, r)| Pick::Object(r)).collect();
                // Les bloqueurs n'ont souvent pas d'image : repli sur l'hexagone visé.
                if out.is_empty()
                    && let Some(h) = geom::screen_to_hex(x, y)
                    && let Some(i) = cur.map.objects[e]
                        .iter()
                        .position(|o| o.is_scroll_blocker() && o.tile() == h as i32)
                {
                    out.push(Pick::Object(ObjectRef {
                        elevation: e as u8,
                        index: i as u32,
                    }));
                }
                return out;
            }
            SelectMode::Normal => {}
        }
        let mut out = Vec::new();
        if self.edit.pick_roof
            && self.layers.contains(Layer::Roof)
            && let Some(sq) = geom::screen_to_square(x, y - geom::ROOF_OFFSET_Y)
            && cur
                .map
                .tile(e, sq as usize)
                .is_some_and(|t| t.roof != fo_core::map::EMPTY_TILE)
        {
            out.push(Pick::Tile(TileRef {
                layer: TileLayer::Roof,
                square: sq,
            }));
        }
        if self.edit.pick_objects {
            for item in scene.items.iter().rev() {
                let Some(r) = item.object else { continue };
                if !self.layers.contains(item.layer)
                    || item.sprite.alpha_at(x - item.x, y - item.y) <= 16
                {
                    continue;
                }
                if !out.contains(&Pick::Object(r)) {
                    out.push(Pick::Object(r));
                }
            }
        }
        if self.edit.pick_floor
            && self.layers.contains(Layer::Floor)
            && let Some(sq) = geom::screen_to_square(x, y)
            && cur
                .map
                .tile(e, sq as usize)
                .is_some_and(|t| t.floor != fo_core::map::EMPTY_TILE)
        {
            out.push(Pick::Tile(TileRef {
                layer: TileLayer::Floor,
                square: sq,
            }));
        }
        out
    }

    /// Clic gauche simple en mode sélection.
    pub(crate) fn click_select(
        &mut self,
        scene: &Arc<Scene>,
        world: Option<Pos2>,
        mods: Modifiers,
    ) {
        let Some(w) = world else { return };
        let candidates = self.pick_candidates(scene, w);
        let combine = Combine::from_modifiers(mods.alt, mods.command);
        if candidates.is_empty() {
            if combine == Combine::Replace {
                self.selection.clear();
            }
            self.edit.last_click = None;
            return;
        }
        // Maj + clic sur une case : sélectionne le rectangle entre la première case sélectionnée et celle-ci.
        if mods.shift
            && let Some(Pick::Tile(to)) = candidates
                .iter()
                .copied()
                .find(|p| matches!(p, Pick::Tile(_)))
            && let Some(from) = self
                .selection
                .tiles()
                .first()
                .copied()
                .filter(|t| t.layer == to.layer)
        {
            let picks = tile_rectangle(from, to);
            self.selection.apply(Combine::Add, picks);
            return;
        }
        // Maj + clic sur un objet : ajoute les objets du rectangle isométrique entre l'objet principal et celui-ci.
        if mods.shift
            && let Some(Pick::Object(to)) = candidates
                .iter()
                .copied()
                .find(|p| matches!(p, Pick::Object(_)))
            && let Some(from) = self.selection.primary()
            && let Some(cur) = self.current.as_ref()
            && let (Some(a), Some(b)) = (
                cur.map.objects[self.elevation]
                    .get(from.index as usize)
                    .map(|o| o.tile()),
                cur.map.objects[self.elevation]
                    .get(to.index as usize)
                    .map(|o| o.tile()),
            )
            && (0..geom::HEX_COUNT as i32).contains(&a)
            && (0..geom::HEX_COUNT as i32).contains(&b)
        {
            let (ax, ay) = geom::hex_to_screen(a as u32);
            let (bx, by) = geom::hex_to_screen(b as u32);
            let rect = IsoRect::new(
                Pos2::new(ax as f32, ay as f32),
                Pos2::new(bx as f32, by as f32),
            );
            let mut picks = self.objects_in_rect(&rect);
            picks.push(Pick::Object(to));
            self.edit.last_click = None;
            self.selection.apply(Combine::Add, picks);
            self.status = format!(
                "{} {}",
                self.selection.objects().len(),
                crate::i18n::t("objet(s) sélectionné(s)")
            );
            return;
        }
        // Un nouveau clic au même endroit passe à l'élément suivant de la pile.
        let index = match self.edit.last_click {
            Some((p, i)) if (p - w).length() < SAME_SPOT => (i + 1) % candidates.len(),
            _ => 0,
        };
        self.edit.last_click = Some((w, index));
        self.selection.apply(combine, [candidates[index]]);
    }

    /// Objets de l'élévation courante dont l'hexagone tombe dans le rectangle (selon le filtre et les calques visibles).
    fn objects_in_rect(&self, rect: &IsoRect) -> Vec<Pick> {
        let e = self.elevation;
        let Some(cur) = self.current.as_ref() else {
            return Vec::new();
        };
        if !self.edit.pick_objects {
            return Vec::new();
        }
        let mut picks = Vec::new();
        for (index, o) in cur.map.objects[e].iter().enumerate() {
            let Some(layer) = object_layer(o) else {
                continue;
            };
            let t = o.tile();
            if !self.layers.contains(layer) || !(0..geom::HEX_COUNT as i32).contains(&t) {
                continue;
            }
            let (cx, cy) = geom::hex_to_screen(t as u32);
            if rect.contains(Pos2::new(cx as f32, cy as f32)) {
                picks.push(Pick::Object(ObjectRef {
                    elevation: e as u8,
                    index: index as u32,
                }));
            }
        }
        picks
    }

    /// Fin d'une sélection par zone (rectangle en coordonnées écran).
    pub(crate) fn band_select(&mut self, a: Pos2, b: Pos2, mods: Modifiers) {
        let (wa, wb) = (
            self.camera.screen_to_world(self.canvas_rect, a),
            self.camera.screen_to_world(self.canvas_rect, b),
        );
        let rect = IsoRect::new(wa, wb);
        let e = self.elevation;
        let Some(cur) = self.current.as_ref() else {
            return;
        };
        match self.edit.select_mode {
            SelectMode::Hexes => {
                let picks: Vec<Pick> = (0..geom::HEX_COUNT)
                    .filter(|&h| {
                        let (x, y) = geom::hex_to_screen(h);
                        rect.contains(Pos2::new(x as f32, y as f32))
                    })
                    .map(Pick::Hex)
                    .collect();
                self.selection
                    .apply(Combine::from_modifiers(mods.alt, mods.command), picks);
                self.edit.last_click = None;
                self.status = format!(
                    "{} {}",
                    self.selection.hexes().len(),
                    crate::i18n::t("hexagone(s) sélectionné(s)")
                );
                return;
            }
            SelectMode::Blockers => {
                let picks: Vec<Pick> = cur.map.objects[e]
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| {
                        o.is_scroll_blocker() && (0..geom::HEX_COUNT as i32).contains(&o.tile())
                    })
                    .filter(|(_, o)| {
                        let (x, y) = geom::hex_to_screen(o.tile() as u32);
                        rect.contains(Pos2::new(x as f32, y as f32))
                    })
                    .map(|(i, _)| {
                        Pick::Object(ObjectRef {
                            elevation: e as u8,
                            index: i as u32,
                        })
                    })
                    .collect();
                self.selection
                    .apply(Combine::from_modifiers(mods.alt, mods.command), picks);
                self.edit.last_click = None;
                self.status = format!(
                    "{} {}",
                    self.selection.objects().len(),
                    crate::i18n::t("bloqueur(s) sélectionné(s)")
                );
                return;
            }
            SelectMode::Normal => {}
        }
        let mut picks = self.objects_in_rect(&rect);
        if let Some(tiles) = &cur.map.tiles[e] {
            for (sq, t) in tiles.iter().enumerate() {
                let (x, y) = geom::square_to_screen(sq as u32);
                let center = Pos2::new(x as f32 + 40.0, y as f32 + 18.0);
                if self.edit.pick_floor
                    && self.layers.contains(Layer::Floor)
                    && t.floor != fo_core::map::EMPTY_TILE
                    && rect.contains(center)
                {
                    picks.push(Pick::Tile(TileRef {
                        layer: TileLayer::Floor,
                        square: sq as u32,
                    }));
                }
                let roof_center = Pos2::new(center.x, center.y + geom::ROOF_OFFSET_Y as f32);
                if self.edit.pick_roof
                    && self.layers.contains(Layer::Roof)
                    && t.roof != fo_core::map::EMPTY_TILE
                    && rect.contains(roof_center)
                {
                    picks.push(Pick::Tile(TileRef {
                        layer: TileLayer::Roof,
                        square: sq as u32,
                    }));
                }
            }
        }
        self.edit.last_click = None;
        self.selection
            .apply(Combine::from_modifiers(mods.alt, mods.command), picks);
        self.status = format!(
            "{} {}, {} {}",
            self.selection.objects().len(),
            crate::i18n::t("objet(s)"),
            self.selection.tiles().len(),
            crate::i18n::t("case(s) sélectionné(s)")
        );
    }

    /// Ctrl+A : tout ce que le filtre de sélection permet, dans l'élévation courante.
    pub(crate) fn select_all(&mut self) {
        let Some(cur) = self.current.as_ref() else {
            return;
        };
        let e = self.elevation;
        let mut picks = Vec::new();
        if self.edit.pick_objects {
            for (index, o) in cur.map.objects[e].iter().enumerate() {
                if object_layer(o).is_some_and(|l| self.layers.contains(l)) {
                    picks.push(Pick::Object(ObjectRef {
                        elevation: e as u8,
                        index: index as u32,
                    }));
                }
            }
        }
        if let Some(tiles) = &cur.map.tiles[e] {
            for (sq, t) in tiles.iter().enumerate() {
                if self.edit.pick_floor && t.floor != fo_core::map::EMPTY_TILE {
                    picks.push(Pick::Tile(TileRef {
                        layer: TileLayer::Floor,
                        square: sq as u32,
                    }));
                }
                if self.edit.pick_roof && t.roof != fo_core::map::EMPTY_TILE {
                    picks.push(Pick::Tile(TileRef {
                        layer: TileLayer::Roof,
                        square: sq as u32,
                    }));
                }
            }
        }
        self.selection.apply(Combine::Replace, picks);
        self.edit.last_click = None;
        self.status = format!(
            "{} {}, {} {}",
            self.selection.objects().len(),
            crate::i18n::t("objet(s)"),
            self.selection.tiles().len(),
            crate::i18n::t("case(s) sélectionné(s)")
        );
    }

    /// Supprime tous les objets de l'élévation courante posés sur les hexagones sélectionnés.
    pub(crate) fn delete_objects_on_selected_hexes(&mut self) {
        let e = self.elevation;
        let hexes: std::collections::HashSet<i32> =
            self.selection.hexes().iter().map(|&h| h as i32).collect();
        let mut doomed: Vec<usize> = self
            .current
            .as_ref()
            .map(|c| {
                c.map.objects[e]
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| hexes.contains(&o.tile()))
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default();
        doomed.sort_unstable_by(|a, b| b.cmp(a));
        let n = doomed.len();
        if self.mutate_as("Suppression sur des hexagones", |m, _| {
            let mut any = false;
            for i in doomed {
                any |= m.remove_object(e, i).is_some();
            }
            any
        }) {
            self.status = format!("{n} {}", crate::i18n::t("objet(s) supprimé(s)"));
        }
    }

    pub(crate) fn deselect_all(&mut self) {
        self.selection.clear();
        self.edit.last_click = None;
    }

    // ---------------------------------------------------------------- opérations de groupe

    /// Déplace tous les objets sélectionnés comme l'objet saisi : il va de `from` à `to`, les autres gardent leur
    /// position relative (calculée en coordonnées cube, donc sans déformation selon la parité des colonnes).
    pub(crate) fn move_selection(&mut self, from: u32, to: u32) {
        let e = self.elevation;
        let indices = self.selection.object_indices(e);
        self.mutate_as("Déplacement", |m, _| {
            let mut any = false;
            for i in indices {
                let Some(t) = m.objects[e]
                    .get(i)
                    .map(|o| o.tile())
                    .filter(|t| (0..geom::HEX_COUNT as i32).contains(t))
                else {
                    continue;
                };
                if let Some(nt) = geom::hex_translate(t as u32, from, to) {
                    any |= m.move_object(e, i, nt);
                }
            }
            any
        });
    }

    pub(crate) fn delete_selected(&mut self) {
        let e = self.elevation;
        let mut indices = self.selection.object_indices(e);
        let tiles: Vec<TileRef> = self.selection.tiles().to_vec();
        if indices.is_empty() && tiles.is_empty() {
            return;
        }
        indices.reverse();
        let changed = self.mutate_as("Suppression", |m, _| {
            let mut any = false;
            for i in indices {
                any |= m.remove_object(e, i).is_some();
            }
            for t in tiles {
                any |= m.set_tile(e, t.square as usize, t.layer, fo_core::map::EMPTY_TILE);
            }
            any
        });
        if changed {
            self.selection.clear();
        }
    }

    pub(crate) fn rotate_selected(&mut self) {
        let e = self.elevation;
        let indices = self.selection.object_indices(e);
        if indices.is_empty() {
            return;
        }
        self.mutate_as("Rotation", |m, _| {
            let mut any = false;
            for i in indices {
                if let Some(o) = m.objects[e].get_mut(i) {
                    o.words[word::ORIENTATION] = (o.words[word::ORIENTATION] + 1).rem_euclid(6);
                    any = true;
                }
            }
            any
        });
    }

    /// Copie la sélection : le groupe est posé de façon que son objet principal arrive sous le curseur (ou un hexagone
    /// plus loin sans curseur). Les copies deviennent la nouvelle sélection.
    pub(crate) fn duplicate_selected(&mut self) {
        let e = self.elevation;
        let indices = self.selection.object_indices(e);
        let Some(anchor) = self
            .selection
            .primary()
            .and_then(|r| self.object(r))
            .map(|o| o.tile())
            .filter(|t| *t >= 0)
            .map(|t| t as u32)
        else {
            return;
        };
        let target = self
            .hover_hex
            .unwrap_or_else(|| (anchor + 1).min(geom::HEX_COUNT - 1));
        let mut created = Vec::new();
        self.mutate_as("Duplication", |m, _| {
            for i in indices {
                let Some(o) = m.objects[e].get(i) else {
                    continue;
                };
                let tile = (o.tile() >= 0)
                    .then(|| geom::hex_translate(o.tile() as u32, anchor, target))
                    .flatten()
                    .unwrap_or(target);
                let copy = o.duplicate_at(tile, m.max_object_id() + 1);
                created.push(m.add_object(e, copy));
            }
            !created.is_empty()
        });
        if !created.is_empty() {
            self.selection.set_objects(
                created
                    .into_iter()
                    .map(|i| ObjectRef {
                        elevation: e as u8,
                        index: i as u32,
                    })
                    .collect(),
            );
        }
    }

    /// Remplace les cases sélectionnées par la tuile du pinceau (ou la tuile vide avec `None`).
    pub(crate) fn fill_selected_tiles(&mut self, id: Option<u16>) {
        let e = self.elevation;
        let tiles: Vec<TileRef> = self.selection.tiles().to_vec();
        if tiles.is_empty() {
            return;
        }
        let id = id.unwrap_or(fo_core::map::EMPTY_TILE);
        self.mutate_as("Remplacement de cases", |m, _| {
            let mut any = false;
            for t in tiles {
                any |= m.set_tile(e, t.square as usize, t.layer, id);
            }
            any
        });
    }
}

/// Toutes les cases du rectangle (en lignes et colonnes) compris entre deux cases de la même couche.
fn tile_rectangle(a: TileRef, b: TileRef) -> Vec<Pick> {
    let (ac, ar) = (
        a.square % geom::SQUARE_GRID_WIDTH,
        a.square / geom::SQUARE_GRID_WIDTH,
    );
    let (bc, br) = (
        b.square % geom::SQUARE_GRID_WIDTH,
        b.square / geom::SQUARE_GRID_WIDTH,
    );
    let mut v = Vec::new();
    for r in ar.min(br)..=ar.max(br) {
        for c in ac.min(bc)..=ac.max(bc) {
            v.push(Pick::Tile(TileRef {
                layer: a.layer,
                square: r * geom::SQUARE_GRID_WIDTH + c,
            }));
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_covers_the_block_between_two_squares() {
        let a = TileRef {
            layer: TileLayer::Floor,
            square: 105,
        };
        let b = TileRef {
            layer: TileLayer::Floor,
            square: 307,
        };
        let r = tile_rectangle(a, b);
        assert_eq!(r.len(), 3 * 3);
        assert!(r.contains(&Pick::Tile(TileRef {
            layer: TileLayer::Floor,
            square: 206
        })));
    }
}

/// Rectangle isométrique : un rectangle dans les axes de la grille hexagonale (les côtés suivent les directions
/// (16, 12) et (−16, 12) de l'écran), défini par deux coins opposés.
#[derive(Clone, Copy, Debug)]
pub(crate) struct IsoRect {
    a: (f32, f32),
    b: (f32, f32),
}

fn to_iso(p: Pos2) -> (f32, f32) {
    (
        (p.x / 16.0 + p.y / 12.0) / 2.0,
        (p.y / 12.0 - p.x / 16.0) / 2.0,
    )
}

fn from_iso(u: f32, v: f32) -> Pos2 {
    Pos2::new(16.0 * (u - v), 12.0 * (u + v))
}

impl IsoRect {
    pub(crate) fn new(p: Pos2, q: Pos2) -> Self {
        Self {
            a: to_iso(p),
            b: to_iso(q),
        }
    }

    pub(crate) fn contains(&self, p: Pos2) -> bool {
        let (u, v) = to_iso(p);
        (u >= self.a.0.min(self.b.0) && u <= self.a.0.max(self.b.0))
            && (v >= self.a.1.min(self.b.1) && v <= self.a.1.max(self.b.1))
    }

    /// Les quatre coins, en coordonnées du monde.
    pub(crate) fn corners(&self) -> [Pos2; 4] {
        [
            from_iso(self.a.0, self.a.1),
            from_iso(self.b.0, self.a.1),
            from_iso(self.b.0, self.b.1),
            from_iso(self.a.0, self.b.1),
        ]
    }
}

#[cfg(test)]
mod iso_tests {
    use super::*;

    #[test]
    fn iso_rectangle_is_a_diamond_on_screen() {
        let r = IsoRect::new(Pos2::new(0.0, 0.0), Pos2::new(160.0, 120.0));
        assert!(
            r.contains(Pos2::new(80.0, 60.0)),
            "le centre de la diagonale est dedans"
        );
        assert!(
            !r.contains(Pos2::new(150.0, 10.0)),
            "un coin du rectangle écran est hors du losange"
        );
        let c = r.corners();
        assert!(
            (c[0] - Pos2::new(0.0, 0.0)).length() < 0.01
                && (c[2] - Pos2::new(160.0, 120.0)).length() < 0.01
        );
    }
}
