//! Cartes `.map` de Fallout 1 et 2 : modèle, lecture et écriture sans perte.

mod edit;
pub mod exit;
mod find;
mod model;
mod reader;
mod script_edit;
mod writer;

pub use edit::TileLayer;
pub use model::*;
pub use reader::{ProtoResolver, is_map, read_map};
pub use script_edit::{
    SCRIPT_TYPE_CRITTER, SCRIPT_TYPE_ITEM, SCRIPT_TYPE_SPATIAL, script_section_for,
};
pub use writer::write_map;
