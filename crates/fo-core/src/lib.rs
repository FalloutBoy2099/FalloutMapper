//! Cœur de FalloutMapper : formats de fichiers de Fallout 2, système de fichiers virtuel et accès aux données du jeu.
//!
//! Aucune dépendance graphique : cette bibliothèque sert aussi bien à l'interface qu'aux outils en ligne de commande.

pub mod art;
pub mod build;
pub mod buildings;
pub mod bytes;
pub mod check;
pub mod construction;
pub mod dat;
pub mod diff;
pub mod edge;
pub mod error;
pub mod formats;
pub mod frm;
pub mod game;
pub mod game_link;
pub mod geom;
pub mod ini_doc;
pub mod launch;
pub mod map;
pub mod maps_txt;
pub mod msg;
pub mod palette_anim;
pub mod palette_blend;
pub mod pattern;
pub mod procgen;
pub mod quilt;
pub mod reach;
pub mod room;
mod room_floors;
pub mod stretch;
pub mod tile_stats;
pub mod vfs;
pub mod village;
pub mod worldmap;

pub use art::{ArtLibrary, Fid, Sprite};
pub use error::{Error, Result};
pub use formats::{MapDef, ObjectType, Palette};
pub use game::{GameData, MapEntry, ProtoEntry};
pub use map::{Map, MapObject, read_map, write_map};
pub use palette_anim::PaletteAnim;
