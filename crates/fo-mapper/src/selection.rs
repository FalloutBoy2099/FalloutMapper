//! Sélection d'éléments de la carte : objets (de l'élévation courante) et cases de sol ou de toit.

use fo_core::map::TileLayer;
use fo_scene::ObjectRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TileRef {
    pub layer: TileLayer,
    /// Numéro de case carrée (0 à 9999).
    pub square: u32,
}

/// Un élément désigné par un clic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pick {
    Object(ObjectRef),
    Tile(TileRef),
    /// Un hexagone de la grille (mode de sélection « Hexagones »).
    Hex(u32),
}

/// Comment une opération modifie la sélection existante.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Combine {
    Replace,
    Add,
    Remove,
}

impl Combine {
    /// Alt ajoute, Ctrl retire, sans modificateur remplace (comme Gecko).
    pub(crate) fn from_modifiers(alt: bool, command: bool) -> Self {
        if alt {
            Combine::Add
        } else if command {
            Combine::Remove
        } else {
            Combine::Replace
        }
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct Selection {
    /// Dans l'ordre de sélection ; le dernier est l'objet « principal » (celui que montre l'inspecteur).
    objects: Vec<ObjectRef>,
    tiles: Vec<TileRef>,
    hexes: Vec<u32>,
}

impl Selection {
    pub(crate) fn clear(&mut self) {
        self.objects.clear();
        self.tiles.clear();
        self.hexes.clear();
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.objects.is_empty() && self.tiles.is_empty() && self.hexes.is_empty()
    }

    pub(crate) fn hexes(&self) -> &[u32] {
        &self.hexes
    }

    pub(crate) fn objects(&self) -> &[ObjectRef] {
        &self.objects
    }

    pub(crate) fn tiles(&self) -> &[TileRef] {
        &self.tiles
    }

    /// L'objet affiché dans l'inspecteur : le dernier sélectionné.
    pub(crate) fn primary(&self) -> Option<ObjectRef> {
        self.objects.last().copied()
    }

    pub(crate) fn contains_object(&self, r: ObjectRef) -> bool {
        self.objects.contains(&r)
    }

    /// Remplace tout par un objet (ou vide la sélection).
    pub(crate) fn set_object(&mut self, r: Option<ObjectRef>) {
        self.clear();
        self.objects.extend(r);
    }

    pub(crate) fn apply(&mut self, combine: Combine, picks: impl IntoIterator<Item = Pick>) {
        if combine == Combine::Replace {
            self.clear();
        }
        for p in picks {
            match (combine, p) {
                (Combine::Remove, Pick::Object(r)) => self.objects.retain(|&o| o != r),
                (Combine::Remove, Pick::Tile(t)) => self.tiles.retain(|&o| o != t),
                (Combine::Remove, Pick::Hex(h)) => self.hexes.retain(|&o| o != h),
                (_, Pick::Hex(h)) => {
                    if !self.hexes.contains(&h) {
                        self.hexes.push(h);
                    }
                }
                (_, Pick::Object(r)) => {
                    if !self.objects.contains(&r) {
                        self.objects.push(r);
                    }
                }
                (_, Pick::Tile(t)) => {
                    if !self.tiles.contains(&t) {
                        self.tiles.push(t);
                    }
                }
            }
        }
    }

    pub(crate) fn set_objects(&mut self, objects: Vec<ObjectRef>) {
        self.clear();
        self.objects = objects;
    }

    /// Indices (dans l'élévation donnée) des objets sélectionnés, triés.
    pub(crate) fn object_indices(&self, elevation: usize) -> Vec<usize> {
        let mut v: Vec<usize> = self
            .objects
            .iter()
            .filter(|r| r.elevation as usize == elevation)
            .map(|r| r.index as usize)
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(i: u32) -> Pick {
        Pick::Object(ObjectRef {
            elevation: 0,
            index: i,
        })
    }

    #[test]
    fn replace_add_remove() {
        let mut s = Selection::default();
        s.apply(Combine::Replace, [obj(1), obj(2)]);
        assert_eq!(s.objects().len(), 2);
        s.apply(Combine::Add, [obj(2), obj(3)]);
        assert_eq!(s.objects().len(), 3, "pas de doublon");
        s.apply(Combine::Remove, [obj(1), obj(99)]);
        assert_eq!(s.object_indices(0), vec![2, 3]);
        assert_eq!(s.primary().map(|r| r.index), Some(3));
        s.apply(Combine::Replace, [obj(7)]);
        assert_eq!(s.object_indices(0), vec![7]);
    }

    #[test]
    fn modifiers_map_to_combine() {
        assert_eq!(Combine::from_modifiers(true, true), Combine::Add);
        assert_eq!(Combine::from_modifiers(false, true), Combine::Remove);
        assert_eq!(Combine::from_modifiers(false, false), Combine::Replace);
    }

    #[test]
    fn tiles_and_objects_are_independent() {
        let mut s = Selection::default();
        let t = Pick::Tile(TileRef {
            layer: TileLayer::Floor,
            square: 5,
        });
        s.apply(Combine::Replace, [t, obj(1)]);
        assert_eq!((s.tiles().len(), s.objects().len()), (1, 1));
        s.apply(Combine::Remove, [t]);
        assert!(s.tiles().is_empty() && !s.is_empty());
    }

    #[test]
    fn hexes_are_selected_added_and_removed_independently() {
        let mut s = Selection::default();
        s.apply(
            Combine::Replace,
            [Pick::Hex(10), Pick::Hex(11), Pick::Hex(10)],
        );
        assert_eq!(s.hexes(), [10, 11]);
        s.apply(Combine::Add, [Pick::Hex(12), obj(1)]);
        s.apply(Combine::Remove, [Pick::Hex(11)]);
        assert_eq!(s.hexes(), [10, 12]);
        assert_eq!(s.objects().len(), 1);
        assert!(!s.is_empty());
        s.clear();
        assert!(s.is_empty() && s.hexes().is_empty());
    }
}
