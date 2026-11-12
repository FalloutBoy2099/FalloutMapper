//! Scène d'une élévation de carte : liste ordonnée de sprites positionnés dans le monde, repères (sorties,
//! scripts spatiaux…) et rendu logiciel. Indépendante de toute bibliothèque graphique.

mod render;
mod scene;

pub use render::{AnimAt, RgbaImage, render, render_shaded};
pub use scene::{
    Blend, DrawItem, Layer, LayerSet, Marker, MarkerKind, ObjectRef, Scene, build_scene,
    object_layer,
};
