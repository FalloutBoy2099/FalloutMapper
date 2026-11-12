use fo_core::geom::{self, ROOF_OFFSET_Y, WORLD_HEIGHT, WORLD_WIDTH};
use fo_core::map::{EMPTY_TILE, Map, MapObject, flags};
use fo_core::{ArtLibrary, Fid, ObjectType, Sprite};
use rayon::prelude::*;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    Floor,
    Roof,
    Item,
    Critter,
    Scenery,
    Wall,
    /// Sorties de carte (barres vertes ou brunes).
    Exit,
    /// Bloqueurs de défilement.
    Blocker,
}

impl Layer {
    pub const ALL: [Layer; 8] = [
        Layer::Floor,
        Layer::Roof,
        Layer::Item,
        Layer::Critter,
        Layer::Scenery,
        Layer::Wall,
        Layer::Exit,
        Layer::Blocker,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Layer::Floor => "Sols",
            Layer::Roof => "Toits",
            Layer::Item => "Objets",
            Layer::Critter => "Créatures",
            Layer::Scenery => "Décors",
            Layer::Wall => "Murs",
            Layer::Exit => "Sorties de carte",
            Layer::Blocker => "Bloqueurs de défilement",
        }
    }

    fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// Ensemble de couches visibles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerSet(u8);

impl LayerSet {
    pub fn all() -> Self {
        Self(Layer::ALL.iter().fold(0, |m, l| m | l.bit()))
    }
    pub fn contains(self, l: Layer) -> bool {
        self.0 & l.bit() != 0
    }
    pub fn set(&mut self, l: Layer, on: bool) {
        if on {
            self.0 |= l.bit();
        } else {
            self.0 &= !l.bit();
        }
    }
    pub fn bits(self) -> u8 {
        self.0
    }
    pub fn from_bits(bits: u8) -> Self {
        Self(bits & Self::all().0)
    }
}

/// Référence stable vers un objet de la carte (élévation + position dans la liste).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectRef {
    pub elevation: u8,
    pub index: u32,
}

/// Façon de combiner un sprite avec ce qu'il y a derrière (drapeaux de transparence des objets).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Blend {
    /// Fondu ordinaire, pondéré par l'opacité.
    Normal,
    /// Mélange indexé par les tables de palette du moteur.
    Palette(fo_core::palette_blend::Effect),
}

#[derive(Debug, Clone)]
pub struct DrawItem {
    pub layer: Layer,
    pub sprite: Arc<Sprite>,
    /// Coin haut-gauche dans le repère du monde.
    pub x: i32,
    pub y: i32,
    pub alpha: u8,
    pub blend: Blend,
    pub object: Option<ObjectRef>,
    /// Hexagone (objets) ou case carrée (sols et toits).
    pub tile: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    PlayerStart,
    ExitGrid,
    SpatialScript,
    /// Objet masqué ou sans art (blocs de défilement, repères…).
    Hidden,
}

#[derive(Debug, Clone)]
pub struct Marker {
    pub kind: MarkerKind,
    pub hex: u32,
    pub label: String,
}

#[derive(Debug, Default)]
pub struct Scene {
    pub elevation: u8,
    pub palette: Option<Arc<fo_core::Palette>>,
    /// Dans l'ordre de dessin (du fond vers l'avant).
    pub items: Vec<DrawItem>,
    pub markers: Vec<Marker>,
    /// Objets dont l'art n'a pas été trouvé : (fid, nombre).
    pub missing_art: Vec<(u32, usize)>,
    /// Emprise du contenu : x, y, largeur, hauteur.
    pub bounds: (i32, i32, i32, i32),
}

impl Scene {
    /// Objet visible le plus en avant sous le point `(x, y)` du monde (pixel non transparent).
    pub fn pick(&self, layers: LayerSet, x: i32, y: i32) -> Option<&DrawItem> {
        self.items
            .iter()
            .rev()
            .filter(|i| i.object.is_some() && layers.contains(i.layer))
            .find(|i| {
                let (lx, ly) = (x - i.x, y - i.y);
                i.sprite.alpha_at(lx, ly) > 16
            })
    }
}

/// Couche d'un objet de la carte, `None` pour ce qui ne se dessine pas comme un objet (tuiles, autres objets divers).
pub fn object_layer(o: &MapObject) -> Option<Layer> {
    match o.object_type()? {
        ObjectType::Item => Some(Layer::Item),
        ObjectType::Critter => Some(Layer::Critter),
        ObjectType::Scenery => Some(Layer::Scenery),
        ObjectType::Wall => Some(Layer::Wall),
        ObjectType::Misc if o.is_exit_grid() => Some(Layer::Exit),
        ObjectType::Misc if o.is_scroll_blocker() => Some(Layer::Blocker),
        ObjectType::Tile | ObjectType::Misc => None,
    }
}

/// Le moteur compare le masque entier : une combinaison de plusieurs effets est opaque.
fn blend_for(f: u32) -> (Blend, u8) {
    use fo_core::palette_blend::Effect;
    let effect = match f & 0x000f_c000 {
        flags::TRANS_RED => Some(Effect::Red),
        flags::TRANS_WALL => Some(Effect::Wall),
        flags::TRANS_GLASS => Some(Effect::Glass),
        flags::TRANS_STEAM => Some(Effect::Steam),
        flags::TRANS_ENERGY => Some(Effect::Energy),
        _ => None,
    };
    (effect.map_or(Blend::Normal, Blend::Palette), 255)
}

/// Construit la scène d'une élévation. Charge (en parallèle) tous les sprites nécessaires.
pub fn build_scene(map: &Map, elevation: usize, art: &ArtLibrary) -> Scene {
    let mut scene = Scene {
        elevation: elevation as u8,
        palette: Some(art.palette().clone()),
        ..Scene::default()
    };

    let (floors, roofs) = build_tiles(map, elevation, art);
    let (objects, markers, missing) = build_objects(map, elevation, art);

    // Ordre de dessin : sols, objets, puis toits.
    scene
        .items
        .reserve(floors.len() + objects.len() + roofs.len());
    scene.items.extend(floors);
    scene.items.extend(objects);
    scene.items.extend(roofs);
    scene.markers = markers;
    scene.missing_art = missing;

    // Repères : départ du joueur et scripts spatiaux.
    let h = &map.header;
    if h.player_elevation as usize == elevation
        && (0..geom::HEX_COUNT as i32).contains(&h.player_tile)
    {
        scene.markers.push(Marker {
            kind: MarkerKind::PlayerStart,
            hex: h.player_tile as u32,
            label: "Départ du joueur".into(),
        });
    }
    for entry in map.scripts[1].iter() {
        if let Some(t) = entry.spatial()
            && t.elevation as usize == elevation
            && t.tile < geom::HEX_COUNT
        {
            scene.markers.push(Marker {
                kind: MarkerKind::SpatialScript,
                hex: t.tile,
                label: format!(
                    "Script spatial #{} (rayon {})",
                    entry.list_index(),
                    t.radius
                ),
            });
        }
    }

    scene.bounds = bounds(&scene.items);
    scene
}

fn build_tiles(map: &Map, elevation: usize, art: &ArtLibrary) -> (Vec<DrawItem>, Vec<DrawItem>) {
    let Some(tiles) = &map.tiles[elevation] else {
        return (Vec::new(), Vec::new());
    };
    let item = |layer: Layer, tile: u32, id: u16, dy: i32| {
        let sprite = art.tile_sprite(id as usize)?;
        let (x, y) = geom::square_to_screen(tile);
        Some(DrawItem {
            layer,
            sprite,
            x,
            y: y + dy,
            alpha: 255,
            blend: Blend::Normal,
            object: None,
            tile,
        })
    };
    let floors = tiles
        .par_iter()
        .enumerate()
        .filter(|(_, t)| t.floor != EMPTY_TILE)
        .filter_map(|(i, t)| item(Layer::Floor, i as u32, t.floor, 0))
        .collect();
    let roofs = tiles
        .par_iter()
        .enumerate()
        .filter(|(_, t)| t.roof != EMPTY_TILE)
        .filter_map(|(i, t)| item(Layer::Roof, i as u32, t.roof, ROOF_OFFSET_Y))
        .collect();
    (floors, roofs)
}

type ObjectBuild = (Vec<DrawItem>, Vec<Marker>, Vec<(u32, usize)>);

fn build_objects(map: &Map, elevation: usize, art: &ArtLibrary) -> ObjectBuild {
    enum Built {
        Drawn(DrawItem, u32),
        /// Dessiné et signalé par un repère (sorties de carte).
        DrawnWithMarker(DrawItem, u32, Marker),
        Marker(Marker),
        Missing(u32),
    }

    let built: Vec<Built> = map.objects[elevation]
        .par_iter()
        .enumerate()
        .filter_map(|(index, o)| {
            let ty = o.object_type()?;
            let tile = o.tile();
            if !(0..geom::HEX_COUNT as i32).contains(&tile) {
                // Objets dans l'inventaire d'un autre ou placés hors grille : rien à dessiner.
                return None;
            }
            let tile = tile as u32;
            let layer = object_layer(o);
            // Sorties et bloqueurs sont masqués dans le jeu mais visibles dans l'éditeur.
            let always_shown = matches!(layer, Some(Layer::Exit | Layer::Blocker));
            if o.has_flag(flags::HIDDEN) && !always_shown {
                return Some(Built::Marker(Marker {
                    kind: MarkerKind::Hidden,
                    hex: tile,
                    label: format!("Masqué ({})", ty.label()),
                }));
            }
            let Some(layer) = layer else {
                // Autres objets « divers » : seulement un repère.
                return (ty == ObjectType::Misc).then(|| {
                    Built::Marker(Marker {
                        kind: MarkerKind::Hidden,
                        hex: tile,
                        label: format!("Divers {:08X}", o.pid()),
                    })
                });
            };
            let flat = if o.has_flag(flags::FLAT) { 0 } else { 1 };
            let exit_marker = o.is_exit_grid().then(|| {
                let label = o
                    .exit_destination()
                    .map_or("Sortie".to_string(), |d| match d.map {
                        -2 => "Sortie → carte du monde".to_string(),
                        -1 => format!("Sortie → hex {} (même carte)", d.tile),
                        n => format!("Sortie → carte {n}, hex {}", d.tile),
                    });
                Marker {
                    kind: MarkerKind::ExitGrid,
                    hex: tile,
                    label,
                }
            });
            Some(
                match (
                    draw_object(o, index, elevation, tile, layer, art),
                    exit_marker,
                ) {
                    (Some(item), Some(m)) => Built::DrawnWithMarker(item, flat, m),
                    (Some(item), None) => Built::Drawn(item, flat),
                    (None, Some(m)) => Built::Marker(m),
                    (None, None) => Built::Missing(o.fid()),
                },
            )
        })
        .collect();

    let mut drawn: Vec<(u32, DrawItem)> = Vec::new();
    let mut markers = Vec::new();
    let mut missing: std::collections::BTreeMap<u32, usize> = Default::default();
    for b in built {
        match b {
            Built::Drawn(item, flat) => drawn.push((flat, item)),
            Built::DrawnWithMarker(item, flat, m) => {
                drawn.push((flat, item));
                markers.push(m);
            }
            Built::Marker(m) => markers.push(m),
            Built::Missing(fid) => *missing.entry(fid).or_default() += 1,
        }
    }
    // Objets « plats » d'abord, puis par hexagone ; l'ordre du fichier départage les égalités.
    drawn.sort_by_key(|(flat, item)| (*flat, item.tile));
    (
        drawn.into_iter().map(|(_, i)| i).collect(),
        markers,
        missing.into_iter().collect(),
    )
}

fn draw_object(
    o: &MapObject,
    index: usize,
    elevation: usize,
    tile: u32,
    layer: Layer,
    art: &ArtLibrary,
) -> Option<DrawItem> {
    let fid = Fid(o.fid());
    let orientation = o.orientation().clamp(0, 5) as u32;
    let (_, sprite) = art.object_sprite(fid, orientation, o.frame().max(0) as usize)?;
    let (cx, cy) = geom::hex_to_screen(tile);
    let (ox, oy) = o.offset();
    Some(DrawItem {
        layer,
        x: cx + ox + sprite.origin_x,
        y: cy + oy + sprite.origin_y,
        sprite,
        alpha: blend_for(o.flags()).1,
        blend: blend_for(o.flags()).0,
        object: Some(ObjectRef {
            elevation: elevation as u8,
            index: index as u32,
        }),
        tile,
    })
}

fn bounds(items: &[DrawItem]) -> (i32, i32, i32, i32) {
    if items.is_empty() {
        return (0, 0, WORLD_WIDTH, WORLD_HEIGHT);
    }
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for i in items {
        x0 = x0.min(i.x);
        y0 = y0.min(i.y);
        x1 = x1.max(i.x + i.sprite.width as i32);
        y1 = y1.max(i.y + i.sprite.height as i32);
    }
    (x0, y0, x1 - x0, y1 - y0)
}

#[cfg(test)]
mod blend_tests {
    use super::*;

    #[test]
    fn flags_pick_a_blend() {
        assert_eq!(blend_for(0), (Blend::Normal, 255));
        use fo_core::palette_blend::Effect;
        for (flag, effect) in [
            (flags::TRANS_GLASS, Effect::Glass),
            (flags::TRANS_STEAM, Effect::Steam),
            (flags::TRANS_ENERGY, Effect::Energy),
            (flags::TRANS_RED, Effect::Red),
            (flags::TRANS_WALL, Effect::Wall),
        ] {
            assert_eq!(blend_for(flag), (Blend::Palette(effect), 255));
        }
        assert_eq!(
            blend_for(flags::TRANS_ENERGY | flags::TRANS_GLASS),
            (Blend::Normal, 255)
        );
        assert_eq!(blend_for(flags::TRANS_NONE), (Blend::Normal, 255));
    }
}
