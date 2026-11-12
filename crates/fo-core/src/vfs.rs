//! Système de fichiers virtuel : plusieurs sources (dossiers, archives `.dat`) empilées par priorité.
//!
//! Tous les chemins sont insensibles à la casse et utilisent `/`. [`normalize`] est appliqué à l'entrée de
//! chaque fonction publique ; les implémentations de [`Source`] reçoivent toujours un chemin normalisé.

use crate::dat::DatArchive;
use crate::error::{Error, Result};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Minuscules, `/` comme séparateur, sans `.`/`..` ni `/` initial.
pub fn normalize(path: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for c in path.split(['/', '\\']) {
        match c {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            c => parts.push(c.to_ascii_lowercase()),
        }
    }
    parts.join("/")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// Nom en minuscules.
    pub name: String,
    pub is_dir: bool,
}

pub trait Source: Send + Sync + std::fmt::Debug {
    fn label(&self) -> String;
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>>;
    fn exists(&self, path: &str) -> bool;
    fn list(&self, dir: &str) -> Vec<DirEntry>;
}

/// Dossier réel du disque.
#[derive(Debug)]
pub struct DirSource {
    root: PathBuf,
}

impl DirSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Résout un chemin normalisé en chemin réel, quelle que soit la casse sur le disque.
    fn resolve(&self, path: &str) -> Option<PathBuf> {
        let mut cur = self.root.clone();
        for comp in path.split('/').filter(|c| !c.is_empty()) {
            let direct = cur.join(comp);
            if direct.exists() {
                cur = direct;
                continue;
            }
            let found = fs::read_dir(&cur)
                .ok()?
                .filter_map(|e| e.ok())
                .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(comp))?;
            cur = found.path();
        }
        Some(cur)
    }
}

impl Source for DirSource {
    fn label(&self) -> String {
        self.root.display().to_string()
    }

    fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        match self.resolve(path) {
            Some(p) if p.is_file() => fs::read(&p).map(Some).map_err(|e| Error::io(p, e)),
            _ => Ok(None),
        }
    }

    fn exists(&self, path: &str) -> bool {
        self.resolve(path).is_some_and(|p| p.is_file())
    }

    fn list(&self, dir: &str) -> Vec<DirEntry> {
        let Some(p) = self.resolve(dir).filter(|p| p.is_dir()) else {
            return Vec::new();
        };
        let Ok(rd) = fs::read_dir(p) else {
            return Vec::new();
        };
        rd.filter_map(|e| e.ok())
            .map(|e| DirEntry {
                name: e.file_name().to_string_lossy().to_ascii_lowercase(),
                is_dir: e.path().is_dir(),
            })
            .collect()
    }
}

/// Archive `.dat` vue comme un dossier.
#[derive(Debug)]
pub struct DatSource {
    archive: DatArchive,
    dirs: HashMap<String, BTreeMap<String, bool>>,
}

impl DatSource {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let archive = DatArchive::open(path)?;
        let mut dirs: HashMap<String, BTreeMap<String, bool>> = HashMap::new();
        for e in archive.entries() {
            let norm = normalize(&e.path);
            let mut parent = String::new();
            let comps: Vec<&str> = norm.split('/').collect();
            for (i, c) in comps.iter().enumerate() {
                let is_dir = i + 1 < comps.len();
                dirs.entry(parent.clone())
                    .or_default()
                    .insert((*c).to_string(), is_dir);
                if is_dir {
                    parent = if parent.is_empty() {
                        (*c).to_string()
                    } else {
                        format!("{parent}/{c}")
                    };
                }
            }
        }
        Ok(Self { archive, dirs })
    }
}

impl Source for DatSource {
    fn label(&self) -> String {
        self.archive.path().display().to_string()
    }

    fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        match self.archive.find(path) {
            Some(e) => self.archive.read(e).map(Some),
            None => Ok(None),
        }
    }

    fn exists(&self, path: &str) -> bool {
        self.archive.find(path).is_some()
    }

    fn list(&self, dir: &str) -> Vec<DirEntry> {
        self.dirs
            .get(dir)
            .map(|m| {
                m.iter()
                    .map(|(name, &is_dir)| DirEntry {
                        name: name.clone(),
                        is_dir,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Empilement de sources : la première qui contient un fichier gagne.
#[derive(Debug, Default, Clone)]
pub struct Vfs {
    sources: Vec<Arc<dyn Source>>,
}

impl Vfs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ajoute une source de priorité inférieure à celles déjà présentes.
    pub fn push(&mut self, source: impl Source + 'static) {
        self.sources.push(Arc::new(source));
    }

    pub fn sources(&self) -> &[Arc<dyn Source>] {
        &self.sources
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let p = normalize(path);
        for s in &self.sources {
            if let Some(data) = s.read(&p)? {
                return Ok(data);
            }
        }
        Err(Error::NotFound(p))
    }

    /// Comme [`Vfs::read`] mais `Ok(None)` si aucune source n'a le fichier.
    pub fn try_read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        match self.read(path) {
            Ok(d) => Ok(Some(d)),
            Err(Error::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn exists(&self, path: &str) -> bool {
        let p = normalize(path);
        self.sources.iter().any(|s| s.exists(&p))
    }

    /// Union (sans doublon, triée) des entrées d'un dossier dans toutes les sources.
    pub fn list(&self, dir: &str) -> Vec<DirEntry> {
        let d = normalize(dir);
        let mut merged: BTreeMap<String, bool> = BTreeMap::new();
        for s in &self.sources {
            for e in s.list(&d) {
                merged
                    .entry(e.name)
                    .and_modify(|v| *v |= e.is_dir)
                    .or_insert(e.is_dir);
            }
        }
        merged
            .into_iter()
            .map(|(name, is_dir)| DirEntry { name, is_dir })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_paths() {
        assert_eq!(
            normalize("\\Art\\Tiles/./GRID000.FRM"),
            "art/tiles/grid000.frm"
        );
        assert_eq!(normalize("/a/b/../c"), "a/c");
        assert_eq!(normalize(""), "");
    }
}
