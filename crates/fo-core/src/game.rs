//! Accès aux données d'une installation de Fallout 2 (dossier du jeu, archives ou fichiers décompressés).

use crate::error::{Error, Result};
use crate::formats::{MapDef, ObjectType, Palette, ProtoHeader, parse_lst, parse_maps_txt};
use crate::msg::MsgFile;
use crate::vfs::{DatSource, DirSource, Vfs};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Clés utiles de `fallout2.cfg` (section `[system]`).
#[derive(Debug, Clone)]
pub struct GameConfig {
    pub master_dat: String,
    pub critter_dat: String,
    pub master_patches: String,
    pub critter_patches: String,
    /// Langue des textes (`text/<langue>/`).
    pub language: String,
    /// `color_cycling` : animation de la palette activée.
    pub color_cycling: bool,
    /// `cycle_speed_factor` : plus grand = plus lent.
    pub cycle_speed_factor: f32,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            master_dat: "master.dat".into(),
            critter_dat: "critter.dat".into(),
            master_patches: "data".into(),
            critter_patches: "data".into(),
            language: "english".into(),
            color_cycling: true,
            cycle_speed_factor: 1.0,
        }
    }
}

impl GameConfig {
    pub fn parse(text: &str) -> Self {
        let mut cfg = Self::default();
        let mut in_system = false;
        for line in text.lines() {
            let line = line.trim();
            if let Some(sec) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                in_system = sec.eq_ignore_ascii_case("system");
            } else if in_system && let Some((k, v)) = line.split_once('=') {
                let v = v.trim().to_string();
                if v.is_empty() {
                    continue;
                }
                match k.trim().to_ascii_lowercase().as_str() {
                    "master_dat" => cfg.master_dat = v,
                    "critter_dat" => cfg.critter_dat = v,
                    "master_patches" => cfg.master_patches = v,
                    "critter_patches" => cfg.critter_patches = v,
                    "language" => cfg.language = v.to_ascii_lowercase(),
                    "color_cycling" => cfg.color_cycling = v != "0",
                    "cycle_speed_factor" => cfg.cycle_speed_factor = v.parse().unwrap_or(1.0),
                    _ => {}
                }
            }
        }
        cfg
    }
}

/// Un prototype du catalogue.
#[derive(Debug, Clone)]
pub struct ProtoEntry {
    pub pid: u32,
    /// Nom du fichier `.pro`.
    pub file: String,
    /// Nom affiché dans le jeu (peut être vide).
    pub name: String,
    pub info: ProtoHeader,
}

/// Une carte disponible dans l'installation.
#[derive(Debug, Clone)]
pub struct MapEntry {
    /// Nom du fichier sans extension, en minuscules.
    pub file: String,
    pub number: Option<u32>,
    pub display: Option<String>,
    /// Groupe de cartes (une ville, un lieu) : Fallout 2 regroupe par dizaine d'identifiant de `worldmap.msg` (200 + 10 × zone + entrée) ;
    /// Fallout 1 par description de `map.msg`. Les commentaires des fichiers ne sont pas lus : certains mods les laissent faux.
    pub group: Option<u32>,
    pub group_name: Option<String>,
    /// Nom du lieu dans son groupe (« Village » pour Arroyo).
    pub place: Option<String>,
}

/// Groupe et lieu d'une carte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapPlace {
    pub group: u32,
    pub group_name: String,
    pub place: String,
}

#[derive(Debug)]
pub struct GameData {
    pub root: PathBuf,
    pub config: GameConfig,
    vfs: Vfs,
    layers: Vec<String>,
    protos: Mutex<HashMap<u32, Option<ProtoHeader>>>,
    lists: Mutex<HashMap<String, std::sync::Arc<Vec<String>>>>,
    messages: Mutex<HashMap<String, Option<std::sync::Arc<MsgFile>>>>,
    palette: OnceLock<std::sync::Arc<Palette>>,
}

impl GameData {
    /// Ouvre une installation : dossier contenant `fallout2.cfg` ou `master.dat` (fichier ou dossier).
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        if !root.is_dir() {
            return Err(Error::NotFound(root.display().to_string()));
        }
        // Fallout 2 : `fallout2.cfg` ; Fallout 1 : `fallout.cfg`.
        let config = ["fallout2.cfg", "fallout.cfg"]
            .iter()
            .find_map(|name| std::fs::read(root.join(name)).ok())
            .map(|b| GameConfig::parse(&String::from_utf8_lossy(&b)))
            .unwrap_or_default();

        // Priorité décroissante : correctifs, puis critter, puis master.
        let mut candidates = vec![
            config.master_patches.clone(),
            config.critter_patches.clone(),
            config.critter_dat.clone(),
            config.master_dat.clone(),
        ];
        let mut seen = Vec::new();
        candidates.retain(|c| {
            let k = c.to_ascii_lowercase();
            let fresh = !seen.contains(&k);
            seen.push(k);
            fresh
        });

        let mut vfs = Vfs::new();
        let mut layers = Vec::new();
        for c in candidates {
            let path = root.join(c.replace('\\', "/"));
            if path.is_dir() {
                vfs.push(DirSource::new(&path));
            } else if path.is_file() {
                vfs.push(DatSource::open(&path)?);
            } else {
                continue;
            }
            layers.push(path.display().to_string());
        }
        if layers.is_empty() {
            return Err(Error::format(format!(
                "{} : aucune donnée de jeu trouvée (master.dat, critter.dat, data)",
                root.display()
            )));
        }
        Ok(Self {
            root,
            config,
            vfs,
            layers,
            protos: Mutex::default(),
            lists: Mutex::default(),
            messages: Mutex::default(),
            palette: OnceLock::new(),
        })
    }

    pub fn vfs(&self) -> &Vfs {
        &self.vfs
    }

    /// Sources utilisées, de la plus prioritaire à la moins prioritaire.
    pub fn layers(&self) -> &[String] {
        &self.layers
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        self.vfs.read(path)
    }

    pub fn palette(&self) -> std::sync::Arc<Palette> {
        self.palette
            .get_or_init(|| {
                let pal = self
                    .vfs
                    .read("color.pal")
                    .and_then(|d| Palette::parse(&d))
                    .unwrap_or_else(|_| Palette::grayscale());
                std::sync::Arc::new(pal)
            })
            .clone()
    }

    /// Liste `.lst` mise en cache (vide si le fichier n'existe pas).
    pub fn list(&self, path: &str) -> std::sync::Arc<Vec<String>> {
        let key = crate::vfs::normalize(path);
        let mut cache = self.lists.lock().unwrap_or_else(|e| e.into_inner());
        cache
            .entry(key.clone())
            .or_insert_with(|| {
                std::sync::Arc::new(
                    self.vfs
                        .read(&key)
                        .map(|d| parse_lst(&d))
                        .unwrap_or_default(),
                )
            })
            .clone()
    }

    /// En-tête du prototype `pid` (items, créatures, décors, murs, tuiles, divers), ou `None` s'il est introuvable.
    pub fn proto_info(&self, pid: u32) -> Option<ProtoHeader> {
        if let Some(v) = self
            .protos
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&pid)
        {
            return v.clone();
        }
        let v = self.load_proto(pid);
        self.protos
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pid, v.clone());
        v
    }

    /// Sous-type (item ou décor) du prototype `pid`, ou `None` si le prototype est introuvable.
    pub fn proto_subtype(&self, pid: u32) -> Option<i32> {
        self.proto_info(pid).map(|h| h.subtype)
    }

    fn load_proto(&self, pid: u32) -> Option<ProtoHeader> {
        let ty = ObjectType::from_index(pid >> 24)?;
        let index = (pid & 0x00FF_FFFF) as usize;
        let dir = ty.dir_name();
        let list = self.list(&format!("proto/{dir}/{dir}.lst"));
        let name = list.get(index.checked_sub(1)?)?;
        let data = self.vfs.try_read(&format!("proto/{dir}/{name}")).ok()??;
        ProtoHeader::parse(&data).ok()
    }

    /// Fichier de messages qui donne leur nom aux prototypes d'une famille.
    fn proto_msg_file(ty: ObjectType) -> &'static str {
        match ty {
            ObjectType::Item => "game/pro_item.msg",
            ObjectType::Critter => "game/pro_crit.msg",
            ObjectType::Scenery => "game/pro_scen.msg",
            ObjectType::Wall => "game/pro_wall.msg",
            ObjectType::Tile => "game/pro_tile.msg",
            ObjectType::Misc => "game/pro_misc.msg",
        }
    }

    /// Un prototype avec son nom affiché (sans charger tout le catalogue de sa famille).
    pub fn proto_entry(&self, pid: u32) -> Option<ProtoEntry> {
        let ty = ObjectType::from_index(pid >> 24)?;
        let info = self.proto_info(pid)?;
        let dir = ty.dir_name();
        let list = self.list(&format!("proto/{dir}/{dir}.lst"));
        let file = list
            .get(((pid & 0x00FF_FFFF) as usize).checked_sub(1)?)?
            .clone();
        let name = self
            .messages(Self::proto_msg_file(ty))
            .and_then(|m| m.get(info.msg_id).map(|e| e.text.clone()))
            .unwrap_or_default();
        Some(ProtoEntry {
            pid,
            file,
            name,
            info,
        })
    }

    /// Tous les prototypes d'une famille avec leur nom affiché (peut prendre un moment : lit chaque fichier).
    pub fn proto_catalog(&self, ty: ObjectType) -> Vec<ProtoEntry> {
        let dir = ty.dir_name();
        let list = self.list(&format!("proto/{dir}/{dir}.lst"));
        let names = self.messages(Self::proto_msg_file(ty));
        (1..=list.len())
            .filter_map(|n| {
                let pid = ((ty as u32) << 24) | n as u32;
                let info = self.proto_info(pid)?;
                let name = names
                    .as_ref()
                    .and_then(|m| m.get(info.msg_id))
                    .map(|e| e.text.clone())
                    .unwrap_or_default();
                Some(ProtoEntry {
                    pid,
                    file: list[n - 1].clone(),
                    name,
                    info,
                })
            })
            .collect()
    }

    /// Nom du fichier `.int` d'un script (ligne `index` de `scripts/scripts.lst`).
    pub fn script_file(&self, index: usize) -> Option<String> {
        self.list("scripts/scripts.lst").get(index).cloned()
    }

    /// Fichier de messages `text/<langue>/<chemin>` (ex. `dialog/arvillag.msg`), avec repli sur l'anglais.
    pub fn messages(&self, path: &str) -> Option<std::sync::Arc<MsgFile>> {
        let key = crate::vfs::normalize(path);
        if let Some(v) = self
            .messages
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
        {
            return v.clone();
        }
        let data = [self.config.language.as_str(), "english"]
            .iter()
            .find_map(|lang| {
                self.vfs
                    .try_read(&format!("text/{lang}/{key}"))
                    .ok()
                    .flatten()
            });
        let loaded = data.map(|d| std::sync::Arc::new(MsgFile::parse(&d)));
        self.messages
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key, loaded.clone());
        loaded
    }

    /// Messages de dialogue d'un script (`dialog/<nom du script>.msg`).
    pub fn script_messages(&self, script_file: &str) -> Option<std::sync::Arc<MsgFile>> {
        let base = script_file
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(script_file);
        let base = base
            .strip_suffix(".int")
            .or_else(|| base.strip_suffix(".INT"))
            .unwrap_or(base);
        self.messages(&format!("dialog/{base}.msg"))
    }

    /// Installation de Fallout 1 : pas de `data/maps.txt` (propre à Fallout 2), cartes en version 19.
    pub fn is_fallout1(&self) -> bool {
        !self.vfs.exists("data/maps.txt")
    }

    /// Cartes déclarées dans `data/maps.txt`.
    pub fn map_defs(&self) -> Vec<MapDef> {
        self.vfs
            .try_read("data/maps.txt")
            .ok()
            .flatten()
            .map(|d| parse_maps_txt(&d))
            .unwrap_or_default()
    }

    /// Oublie les fichiers de messages déjà lus (après la modification d'un `.msg`).
    pub fn forget_messages(&self) {
        self.messages
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    pub fn forget_lists(&self) {
        self.lists.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// Noms des cartes dans la langue du jeu : `city.txt` dit quelle entrée (zone n, entrée k) mène à quelle carte ; le nom de
    /// l'entrée est `worldmap.msg` n° `200 + 10 × n + k` et celui de la ville `map.msg` n° `1500 + n`. Donne « Ville — Lieu »
    /// (« Arroyo — Village »), ou le seul lieu quand il porte le nom de la ville. Clé : nom de fichier de la carte.
    pub fn map_titles(&self) -> HashMap<String, String> {
        if self.is_fallout1() {
            return self.map_titles_fallout1();
        }
        let defs = self.map_defs();
        let by_lookup: HashMap<String, &MapDef> = defs
            .iter()
            .filter_map(|d| d.display.as_ref().map(|n| (n.to_lowercase(), d)))
            .collect();
        let Some(city) = self.read_data_text("data/city.txt") else {
            return HashMap::new();
        };
        let worldmap = self.messages("game/worldmap.msg");
        let cities = self.messages("game/map.msg");
        let text = |m: &Option<std::sync::Arc<MsgFile>>, id: i32| {
            m.as_ref()
                .and_then(|m| m.get(id))
                .map(|e| e.text.trim().to_string())
                .filter(|t| !t.is_empty() && !t.starts_with("Error:"))
        };
        let mut out = HashMap::new();
        for area in crate::worldmap::parse_areas(&crate::ini_doc::IniDoc::parse(&city)) {
            let Some(n) = area
                .section
                .split_whitespace()
                .last()
                .and_then(|s| s.parse::<i32>().ok())
            else {
                continue;
            };
            let city_name = text(&cities, 1500 + n).unwrap_or_else(|| area.name.clone());
            for e in &area.entrances {
                let Some(k) = e.key.rsplit('_').next().and_then(|s| s.parse::<i32>().ok()) else {
                    continue;
                };
                let Some(def) = by_lookup.get(&e.map.to_lowercase()) else {
                    continue;
                };
                let place = text(&worldmap, 200 + 10 * n + k).unwrap_or_else(|| e.map.clone());
                let title = if place.eq_ignore_ascii_case(&city_name) || city_name.is_empty() {
                    place
                } else {
                    format!("{city_name} — {place}")
                };
                out.entry(def.file.clone()).or_insert(title);
            }
        }
        // Plusieurs cartes portent parfois le même nom (« Désert ») : le fichier les distingue.
        let mut count: HashMap<String, usize> = HashMap::new();
        for t in out.values() {
            *count.entry(t.clone()).or_default() += 1;
        }
        for (file, t) in out.iter_mut() {
            if count[t.as_str()] > 1 {
                *t = format!("{t} ({file})");
            }
        }
        out
    }

    /// Groupe et lieu de chaque carte rattachée à une zone (clé : nom de fichier). Fallout 2 : `city.txt` dit quelle entrée (zone n,
    /// entrée k) mène à quelle carte ; l'identifiant `200 + 10 × n + k` de `worldmap.msg` donne le lieu, et sa dizaine le groupe ;
    /// le nom du groupe est celui de la ville dans `map.msg` (1500 + n), sinon le nom de la zone dans `city.txt`.
    /// Fallout 1 : les cartes qui ont la même description dans `map.msg` forment un groupe.
    pub fn map_places(&self) -> HashMap<String, MapPlace> {
        if self.is_fallout1() {
            return self.map_places_fallout1();
        }
        let defs = self.map_defs();
        let by_lookup: HashMap<String, &MapDef> = defs
            .iter()
            .filter_map(|d| d.display.as_ref().map(|n| (n.to_lowercase(), d)))
            .collect();
        let Some(city) = self.read_data_text("data/city.txt") else {
            return HashMap::new();
        };
        let worldmap = self.messages("game/worldmap.msg");
        let cities = self.messages("game/map.msg");
        let text = |m: &Option<std::sync::Arc<MsgFile>>, id: i32| {
            m.as_ref()
                .and_then(|m| m.get(id))
                .map(|e| e.text.trim().to_string())
                .filter(|t| !t.is_empty() && !t.starts_with("Error:"))
        };
        let mut out = HashMap::new();
        for area in crate::worldmap::parse_areas(&crate::ini_doc::IniDoc::parse(&city)) {
            let Some(n) = area
                .section
                .split_whitespace()
                .last()
                .and_then(|s| s.parse::<i32>().ok())
            else {
                continue;
            };
            let group_name = text(&cities, 1500 + n).unwrap_or_else(|| area.name.clone());
            for e in &area.entrances {
                let Some(k) = e.key.rsplit('_').next().and_then(|s| s.parse::<i32>().ok()) else {
                    continue;
                };
                let Some(def) = by_lookup.get(&e.map.to_lowercase()) else {
                    continue;
                };
                let id = 200 + 10 * n + k;
                let place = text(&worldmap, id).unwrap_or_else(|| e.map.clone());
                out.entry(def.file.clone()).or_insert(MapPlace {
                    group: (id / 10) as u32,
                    group_name: group_name.clone(),
                    place,
                });
            }
        }
        out
    }

    fn map_places_fallout1(&self) -> HashMap<String, MapPlace> {
        let Some(msg) = self.messages("game/map.msg") else {
            return HashMap::new();
        };
        let mut by_desc: HashMap<String, Vec<(u32, String)>> = HashMap::new();
        for (file, n) in self.map_numbers_fallout1() {
            if let Some(desc) = msg
                .get(n as i32 + 100)
                .map(|e| e.text.trim().to_string())
                .filter(|d| !d.is_empty())
            {
                by_desc.entry(desc).or_default().push((n, file));
            }
        }
        let mut out = HashMap::new();
        for (desc, maps) in by_desc {
            // Une carte seule n'a pas besoin de groupe.
            if maps.len() < 2 {
                continue;
            }
            let key = maps.iter().map(|m| m.0).min().unwrap_or(0);
            for (_, file) in maps {
                out.insert(
                    file.clone(),
                    MapPlace {
                        group: key,
                        group_name: desc.clone(),
                        place: file,
                    },
                );
            }
        }
        out
    }

    /// Fallout 1 : pas de `maps.txt` ni de `city.txt`. `map.msg` donne le fichier de chaque numéro de carte ({0}…) et sa
    /// description lisible ({100 + numéro} : « Junktown »). Plusieurs cartes ont la même description : le fichier les distingue.
    fn map_titles_fallout1(&self) -> HashMap<String, String> {
        let Some(msg) = self.messages("game/map.msg") else {
            return HashMap::new();
        };
        let mut out: HashMap<String, String> = HashMap::new();
        for (file, n) in self.map_numbers_fallout1() {
            if let Some(desc) = msg
                .get(n as i32 + 100)
                .map(|e| e.text.trim().to_string())
                .filter(|d| !d.is_empty())
            {
                out.insert(file, desc);
            }
        }
        let mut count: HashMap<String, usize> = HashMap::new();
        for t in out.values() {
            *count.entry(t.clone()).or_default() += 1;
        }
        for (file, t) in out.iter_mut() {
            if count[t.as_str()] > 1 {
                *t = format!("{t} ({file})");
            }
        }
        out
    }

    /// Fallout 1 : numéro de chaque carte d'après `map.msg` ({n} = nom du fichier de la carte n).
    fn map_numbers_fallout1(&self) -> HashMap<String, u32> {
        let Some(msg) = self.messages("game/map.msg") else {
            return HashMap::new();
        };
        (0..100u32)
            .filter_map(|n| {
                let file = msg.get(n as i32)?.text.trim().to_ascii_lowercase();
                Some((file.strip_suffix(".map")?.to_string(), n))
            })
            .collect()
    }

    /// Toutes les cartes présentes (`maps/*.map`), triées par numéro de carte puis par nom.
    pub fn list_maps(&self) -> Vec<MapEntry> {
        let defs: HashMap<String, MapDef> = self
            .map_defs()
            .into_iter()
            .map(|d| (d.file.clone(), d))
            .collect();
        let titles = self.map_titles();
        let places = self.map_places();
        // Sans `maps.txt` (Fallout 1), l'ordre et les numéros viennent de `map.msg`.
        let f1_numbers = if self.is_fallout1() {
            self.map_numbers_fallout1()
        } else {
            HashMap::new()
        };
        let mut maps: Vec<MapEntry> = self
            .vfs
            .list("maps")
            .into_iter()
            .filter(|e| !e.is_dir)
            .filter_map(|e| e.name.strip_suffix(".map").map(str::to_string))
            .map(|file| {
                let def = defs.get(&file);
                let place = places.get(&file);
                MapEntry {
                    number: def
                        .map(|d| d.number)
                        .or_else(|| f1_numbers.get(&file).copied()),
                    display: titles
                        .get(&file)
                        .cloned()
                        .or_else(|| def.and_then(|d| d.display.clone())),
                    group: place.map(|p| p.group),
                    group_name: place.map(|p| p.group_name.clone()),
                    place: place.map(|p| p.place.clone()),
                    file,
                }
            })
            .collect();
        maps.sort_by(|a, b| {
            a.number
                .is_none()
                .cmp(&b.number.is_none())
                .then(a.number.cmp(&b.number))
                .then_with(|| a.file.cmp(&b.file))
        });
        maps
    }

    /// Enregistre une carte dans le dossier de correctifs du jeu (`data/maps/`), qui prime sur les archives :
    /// les `.dat` ne sont jamais modifiés. Une éventuelle version déjà présente est copiée une fois en `.bak`.
    pub fn save_map(&self, file: &str, bytes: &[u8]) -> Result<PathBuf> {
        self.save_in_maps(file, "map", bytes)
    }

    /// Enregistre les limites de défilement (`.edg`) à côté de la carte, avec la même prudence.
    pub fn save_edge(&self, file: &str, bytes: &[u8]) -> Result<PathBuf> {
        self.save_in_maps(file, "edg", bytes)
    }

    /// Supprime le `.edg` du dossier de correctifs (une carte sans zone n'a pas de fichier). Renvoie `true` s'il existait.
    pub fn remove_edge_override(&self, file: &str) -> Result<bool> {
        let base = Self::map_base_name(file)?;
        let patches = self.root.join(&self.config.master_patches);
        let Some(dir) = child_dir_ignore_case(&patches, "maps") else {
            return Ok(false);
        };
        let target = dir.join(format!("{base}.edg"));
        if !target.exists() {
            return Ok(false);
        }
        std::fs::remove_file(&target).map_err(|e| Error::io(&target, e))?;
        Ok(true)
    }

    /// Écrit un fichier de données (`data/maps.txt`, `data/city.txt`…) dans le dossier de correctifs, qui prime sur les
    /// archives ; l'original déjà présent est copié une fois en `.bak`.
    pub fn save_data_file(&self, relative: &str, bytes: &[u8]) -> Result<PathBuf> {
        let parts: Vec<&str> = relative
            .split(['/', '\\'])
            .filter(|p| !p.is_empty() && *p != "." && *p != "..")
            .collect();
        let Some((file, dirs)) = parts.split_last() else {
            return Err(Error::format("chemin vide"));
        };
        let mut dir = self.root.join(&self.config.master_patches);
        for d in dirs {
            dir = child_dir_ignore_case(&dir, d).unwrap_or_else(|| dir.join(d));
        }
        std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        let target = dir.join(file);
        if target.exists() {
            let backup = dir.join(format!("{file}.bak"));
            if !backup.exists() {
                std::fs::copy(&target, &backup).map_err(|e| Error::io(&backup, e))?;
            }
        }
        let tmp = dir.join(format!("{file}.tmp"));
        std::fs::write(&tmp, bytes).map_err(|e| Error::io(&tmp, e))?;
        std::fs::rename(&tmp, &target).map_err(|e| Error::io(&target, e))?;
        Ok(target)
    }

    /// Contenu texte d'un fichier de données (correctifs d'abord), décodé en Latin-1/Windows-1252 sans perte d'octets.
    pub fn read_data_text(&self, path: &str) -> Option<String> {
        let bytes = self.vfs.try_read(path).ok().flatten()?;
        Some(bytes.iter().map(|&b| b as char).collect())
    }

    /// Limites de défilement d'une carte, si un `.edg` existe (correctifs ou archives) et se lit.
    pub fn read_edge(&self, file: &str) -> Option<crate::edge::MapEdge> {
        let base = Self::map_base_name(file).ok()?;
        let bytes = self
            .vfs
            .try_read(&format!("maps/{base}.edg"))
            .ok()
            .flatten()?;
        crate::edge::MapEdge::parse(&bytes).ok()
    }

    fn map_base_name(file: &str) -> Result<String> {
        let base = file.trim_end_matches(".map").to_ascii_lowercase();
        if base.is_empty() || base.contains(['/', '\\', ':']) {
            return Err(Error::format(format!("nom de carte invalide : {file}")));
        }
        Ok(base)
    }

    fn save_in_maps(&self, file: &str, ext: &str, bytes: &[u8]) -> Result<PathBuf> {
        let base = Self::map_base_name(file)?;
        let patches = self.root.join(&self.config.master_patches);
        let dir = child_dir_ignore_case(&patches, "maps").unwrap_or_else(|| patches.join("maps"));
        std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        let target = dir.join(format!("{base}.{ext}"));
        if target.exists() {
            let backup = dir.join(format!("{base}.{ext}.bak"));
            if !backup.exists() {
                std::fs::copy(&target, &backup).map_err(|e| Error::io(&backup, e))?;
            }
        }
        // Écriture dans un fichier temporaire puis renommage : un plantage ne laisse pas un fichier tronqué.
        let tmp = dir.join(format!("{base}.{ext}.tmp"));
        std::fs::write(&tmp, bytes).map_err(|e| Error::io(&tmp, e))?;
        std::fs::rename(&tmp, &target).map_err(|e| Error::io(&target, e))?;
        Ok(target)
    }

    pub fn read_map_bytes(&self, file: &str) -> Result<Vec<u8>> {
        self.vfs
            .read(&format!("maps/{}.map", file.trim_end_matches(".map")))
    }
}

fn child_dir_ignore_case(parent: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(parent)
        .ok()?
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name) && e.path().is_dir())
        .map(|e| e.path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_reads_system_section() {
        let c = GameConfig::parse(
            "[sound]\nmaster_dat=x\n[system]\nmaster_dat = my.dat\ncritter_patches=\n",
        );
        assert_eq!(c.master_dat, "my.dat");
        assert_eq!(c.critter_patches, "data");
    }
}
