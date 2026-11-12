//! Résolution des FID en fichiers d'art, chargement des FRM et conversion en sprites RGBA.

use crate::formats::{ObjectType, Palette};
use crate::frm::Frm;
use crate::game::GameData;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

/// Identifiant d'art décomposé (FID des objets de carte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fid(pub u32);

impl Fid {
    pub fn object_type(self) -> Option<ObjectType> {
        ObjectType::from_index((self.0 >> 24) & 0xF)
    }
    /// Numéro dans la liste d'art du type.
    pub fn id(self) -> usize {
        (self.0 & 0xFFF) as usize
    }
    /// Code d'animation (créatures).
    pub fn anim(self) -> u32 {
        (self.0 >> 16) & 0xFF
    }
    /// Code d'arme (créatures).
    pub fn weapon(self) -> u32 {
        (self.0 >> 12) & 0xF
    }
    /// Direction encodée dans le FID.
    pub fn rotation(self) -> u32 {
        (self.0 >> 28) & 0x7
    }
}

/// Image prête à afficher.
#[derive(Debug)]
pub struct Sprite {
    /// Identifiant unique (sert de clé de texture côté interface).
    pub id: u64,
    pub width: u32,
    pub height: u32,
    /// Position du coin haut-gauche par rapport à l'ancrage (centre bas de l'image sur son hexagone).
    pub origin_x: i32,
    pub origin_y: i32,
    /// RGBA non prémultiplié, `width * height * 4` octets (palette au repos).
    pub rgba: Vec<u8>,
    /// Animations de palette utilisées par ce sprite (un bit par entrée de `palette_anim::CYCLES`), 0 si fixe.
    pub anim_mask: u8,
    /// Indices d'origine, nécessaires aux tables de transparence et à l'animation de palette.
    pub indices: Vec<u8>,
}

impl Sprite {
    pub fn alpha_at(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return 0;
        }
        self.rgba[(y as usize * self.width as usize + x as usize) * 4 + 3]
    }
}

type SpriteKey = (Arc<str>, u8, u16);

/// Bibliothèque d'art : listes `.lst`, FRM et sprites, avec cache partagé entre threads.
#[derive(Debug)]
pub struct ArtLibrary {
    game: Arc<GameData>,
    palette: Arc<Palette>,
    frms: RwLock<HashMap<String, Option<Arc<Frm>>>>,
    sprites: RwLock<HashMap<SpriteKey, Option<Arc<Sprite>>>>,
    next_id: AtomicU64,
}

impl ArtLibrary {
    pub fn new(game: Arc<GameData>) -> Self {
        let palette = game.palette();
        Self {
            game,
            palette,
            frms: RwLock::default(),
            sprites: RwLock::default(),
            next_id: AtomicU64::new(1),
        }
    }

    pub fn palette(&self) -> &Arc<Palette> {
        &self.palette
    }

    pub fn game(&self) -> &Arc<GameData> {
        &self.game
    }

    /// Nom de base (sans extension) d'une entrée de la liste d'art du type.
    fn list_name(&self, ty: ObjectType, id: usize) -> Option<String> {
        let dir = ty.dir_name();
        let list = self.game.list(&format!("art/{dir}/{dir}.lst"));
        list.get(id).cloned()
    }

    /// Chemins candidats (du plus précis au repli) pour un FID.
    pub fn fid_paths(&self, fid: Fid, orientation: u32) -> Vec<String> {
        let Some(ty) = fid.object_type() else {
            return Vec::new();
        };
        let Some(name) = self.list_name(ty, fid.id()) else {
            return Vec::new();
        };
        let dir = ty.dir_name();
        if ty != ObjectType::Critter {
            return vec![format!("art/{dir}/{name}")];
        }
        let base = name.trim_end_matches(".frm");
        let letters = |weapon: u32, anim: u32| {
            let w = (b'a' + (weapon as u8).min(25)) as char;
            let a = (b'a' + (anim as u8).min(25)) as char;
            format!("{w}{a}")
        };
        let mut v = vec![
            format!("art/{dir}/{base}{}.frm", letters(fid.weapon(), fid.anim())),
            format!(
                "art/{dir}/{base}{}.fr{}",
                letters(fid.weapon(), fid.anim()),
                orientation.min(5)
            ),
            // Repli : posture debout sans arme.
            format!("art/{dir}/{base}aa.frm"),
            format!("art/{dir}/{base}aa.fr{}", orientation.min(5)),
        ];
        v.dedup();
        v
    }

    pub fn frm(&self, path: &str) -> Option<Arc<Frm>> {
        let key = crate::vfs::normalize(path);
        if let Some(v) = self
            .frms
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
        {
            return v.clone();
        }
        let loaded = self
            .game
            .vfs()
            .try_read(&key)
            .ok()
            .flatten()
            .and_then(|d| Frm::parse(&d).ok())
            .map(Arc::new);
        self.frms
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key, loaded.clone());
        loaded
    }

    /// Sprite d'une image donnée d'un FRM (cache par chemin, direction et image).
    pub fn sprite(&self, path: &str, dir: usize, frame: usize) -> Option<Arc<Sprite>> {
        let key: SpriteKey = (
            Arc::from(crate::vfs::normalize(path)),
            dir as u8,
            frame as u16,
        );
        if let Some(v) = self
            .sprites
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
        {
            return v.clone();
        }
        let sprite = self
            .frm(&key.0)
            .and_then(|frm| self.make_sprite(&frm, dir, frame));
        self.sprites
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key, sprite.clone());
        sprite
    }

    fn make_sprite(&self, frm: &Frm, dir: usize, frame: usize) -> Option<Arc<Sprite>> {
        let d = frm.direction(dir);
        let f = d.frames.get(frame).or_else(|| d.frames.first())?;
        let (origin_x, origin_y) = frm.frame_origin(dir, frame.min(d.frames.len() - 1));
        let anim_mask = f
            .pixels
            .iter()
            .fold(0u8, |m, &p| m | crate::palette_anim::animation_mask(p));
        let mut rgba = Vec::with_capacity(f.pixels.len() * 4);
        for &p in &f.pixels {
            rgba.extend_from_slice(&self.palette.colors[p as usize]);
        }
        Some(Arc::new(Sprite {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            width: f.width as u32,
            height: f.height as u32,
            origin_x,
            origin_y,
            rgba,
            anim_mask,
            indices: f.pixels.clone(),
        }))
    }

    /// Sprite d'un objet de carte : essaie les chemins candidats du FID dans l'ordre.
    /// Renvoie aussi le chemin retenu.
    pub fn object_sprite(
        &self,
        fid: Fid,
        orientation: u32,
        frame: usize,
    ) -> Option<(String, Arc<Sprite>)> {
        for path in self.fid_paths(fid, orientation) {
            let Some(frm) = self.frm(&path) else { continue };
            // Les FRM à une seule direction (décors, tuiles) ignorent l'orientation.
            let dir = if frm.directions.len() > 1 {
                orientation as usize
            } else {
                0
            };
            if let Some(s) = self.sprite(&path, dir, frame) {
                return Some((path, s));
            }
        }
        None
    }

    /// Sprite d'une tuile de sol ou de toit (numéro dans `tiles.lst`).
    pub fn tile_sprite(&self, tile_id: usize) -> Option<Arc<Sprite>> {
        let name = self.list_name(ObjectType::Tile, tile_id)?;
        self.sprite(&format!("art/tiles/{name}"), 0, 0)
    }
}
