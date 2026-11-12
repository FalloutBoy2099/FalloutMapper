//! Archives `.dat` : Fallout 2 (DAT2 : index en fin de fichier, données zlib ou brutes) et Fallout 1 (DAT1 : index en
//! tête, tout en gros-boutiste, données LZSS ou brutes).

use crate::error::{Error, Result};
use flate2::read::ZlibDecoder;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Taille maximale acceptée pour une entrée décompressée (protège d'une archive corrompue).
const MAX_ENTRY_SIZE: u32 = 256 << 20;

#[derive(Debug, Clone)]
pub struct DatEntry {
    /// Chemin dans l'archive, séparateur `/`, casse d'origine.
    pub path: String,
    pub compressed: bool,
    pub size: u32,
    pub packed_size: u32,
    pub offset: u32,
}

/// Variante d'archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatFormat {
    Dat1,
    Dat2,
}

#[derive(Debug)]
pub struct DatArchive {
    path: PathBuf,
    format: DatFormat,
    file: Mutex<File>,
    entries: Vec<DatEntry>,
    lookup: HashMap<String, usize>,
}

impl DatArchive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = File::open(&path).map_err(|e| Error::io(&path, e))?;
        let len = file.metadata().map_err(|e| Error::io(&path, e))?.len();
        if len < 16 {
            return Err(Error::format(format!(
                "{} : archive DAT trop courte",
                path.display()
            )));
        }

        let io = |e| Error::io(&path, e);
        file.seek(SeekFrom::Start(len - 8)).map_err(io)?;
        let mut foot = [0u8; 8];
        file.read_exact(&mut foot).map_err(io)?;
        let tree_size = u32::from_le_bytes(foot[0..4].try_into().unwrap()) as u64;
        let data_size = u32::from_le_bytes(foot[4..8].try_into().unwrap()) as u64;
        let (format, entries) = if data_size == len && tree_size >= 4 && tree_size + 8 <= len {
            file.seek(SeekFrom::Start(len - 8 - tree_size))
                .map_err(io)?;
            let mut tree = vec![0u8; tree_size as usize];
            file.read_exact(&mut tree).map_err(io)?;
            (DatFormat::Dat2, parse_tree(&tree, len)?)
        } else {
            // Fallout 1 : l'index est au début du fichier. On lit au plus 16 Mio, bien assez pour les 40 000 entrées.
            file.seek(SeekFrom::Start(0)).map_err(io)?;
            let mut head = Vec::new();
            (&mut file)
                .take(16 << 20)
                .read_to_end(&mut head)
                .map_err(io)?;
            let entries = parse_dat1(&head, len).map_err(|e| {
                Error::format(format!("{} : ni DAT2 ni DAT1 ({e})", path.display()))
            })?;
            (DatFormat::Dat1, entries)
        };

        let lookup = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.path.to_ascii_lowercase(), i))
            .collect();
        Ok(Self {
            path,
            format,
            file: Mutex::new(file),
            entries,
            lookup,
        })
    }

    pub fn format(&self) -> DatFormat {
        self.format
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn entries(&self) -> &[DatEntry] {
        &self.entries
    }

    /// Cherche une entrée ; `path` est déjà normalisé (minuscules, `/`).
    pub fn find(&self, path: &str) -> Option<&DatEntry> {
        self.lookup.get(path).map(|&i| &self.entries[i])
    }

    pub fn read(&self, entry: &DatEntry) -> Result<Vec<u8>> {
        let packed_len = if entry.compressed {
            entry.packed_size
        } else {
            entry.size
        } as usize;
        let mut packed = vec![0u8; packed_len];
        {
            let mut f = self.file.lock().unwrap_or_else(|e| e.into_inner());
            f.seek(SeekFrom::Start(entry.offset as u64))
                .map_err(|e| Error::io(&self.path, e))?;
            f.read_exact(&mut packed)
                .map_err(|e| Error::io(&self.path, e))?;
        }
        if !entry.compressed {
            return Ok(packed);
        }
        if self.format == DatFormat::Dat1 {
            return lzss_decode(&packed, entry.size as usize).map_err(|e| {
                Error::format(format!(
                    "{} : {} : décompression impossible ({e})",
                    self.path.display(),
                    entry.path
                ))
            });
        }
        let mut out = Vec::with_capacity(entry.size as usize);
        ZlibDecoder::new(packed.as_slice())
            .read_to_end(&mut out)
            .map_err(|e| {
                Error::format(format!(
                    "{} : {} : décompression impossible ({e})",
                    self.path.display(),
                    entry.path
                ))
            })?;
        if out.len() != entry.size as usize {
            return Err(Error::format(format!(
                "{} : taille décompressée inattendue",
                entry.path
            )));
        }
        Ok(out)
    }
}

fn parse_tree(tree: &[u8], file_len: u64) -> Result<Vec<DatEntry>> {
    let u32_at = |p: usize| -> Result<u32> {
        tree.get(p..p + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .ok_or(Error::Truncated {
                what: "index DAT",
                at: p,
            })
    };
    let count = u32_at(0)? as usize;
    // Une entrée occupe au moins 17 octets : refuse un nombre aberrant avant d'allouer.
    if count > tree.len() / 17 {
        return Err(Error::format("index DAT : nombre d'entrées invalide"));
    }
    let mut entries = Vec::with_capacity(count);
    let mut p = 4;
    for _ in 0..count {
        let name_len = u32_at(p)? as usize;
        p += 4;
        let name = tree.get(p..p + name_len).ok_or(Error::Truncated {
            what: "nom DAT",
            at: p,
        })?;
        p += name_len;
        let compressed = *tree.get(p).ok_or(Error::Truncated {
            what: "index DAT",
            at: p,
        })? != 0;
        let size = u32_at(p + 1)?;
        let packed_size = u32_at(p + 5)?;
        let offset = u32_at(p + 9)?;
        p += 13;
        let stored = if compressed { packed_size } else { size };
        if size > MAX_ENTRY_SIZE || offset as u64 + stored as u64 > file_len {
            return Err(Error::format(format!(
                "index DAT : entrée hors du fichier ({})",
                String::from_utf8_lossy(name)
            )));
        }
        let path = String::from_utf8_lossy(name).replace('\\', "/");
        entries.push(DatEntry {
            path,
            compressed,
            size,
            packed_size,
            offset,
        });
    }
    Ok(entries)
}

/// Index d'une archive DAT1 : en-tête puis noms de dossiers, puis les fichiers de chaque dossier (tout en gros-boutiste).
fn parse_dat1(buf: &[u8], file_len: u64) -> std::result::Result<Vec<DatEntry>, String> {
    let mut p = 0usize;
    let u32_be = |p: &mut usize| -> std::result::Result<u32, String> {
        let b = buf.get(*p..*p + 4).ok_or("index tronqué")?;
        *p += 4;
        Ok(u32::from_be_bytes(b.try_into().unwrap()))
    };
    let dir_count = u32_be(&mut p)? as usize;
    if dir_count == 0 || dir_count > 20_000 {
        return Err("nombre de dossiers invalide".into());
    }
    p += 12;
    let name = |p: &mut usize| -> std::result::Result<String, String> {
        let n = *buf.get(*p).ok_or("index tronqué")? as usize;
        let s = buf.get(*p + 1..*p + 1 + n).ok_or("index tronqué")?;
        *p += 1 + n;
        Ok(String::from_utf8_lossy(s).replace('\\', "/"))
    };
    let mut dirs = Vec::with_capacity(dir_count);
    for _ in 0..dir_count {
        dirs.push(name(&mut p)?);
    }
    let mut entries = Vec::new();
    for dir in &dirs {
        let count = u32_be(&mut p)? as usize;
        p += 12;
        if count > buf.len() / 13 {
            return Err("nombre de fichiers invalide".into());
        }
        for _ in 0..count {
            let file = name(&mut p)?;
            let attr = u32_be(&mut p)?;
            let offset = u32_be(&mut p)?;
            let size = u32_be(&mut p)?;
            let packed_size = u32_be(&mut p)?;
            let compressed = attr & 0x40 != 0;
            let stored = if compressed { packed_size } else { size };
            if size > MAX_ENTRY_SIZE || offset as u64 + stored as u64 > file_len {
                return Err(format!("entrée hors du fichier ({dir}/{file})"));
            }
            let path = if dir == "." {
                file
            } else {
                format!("{dir}/{file}")
            };
            entries.push(DatEntry {
                path,
                compressed,
                size,
                packed_size,
                offset,
            });
        }
    }
    Ok(entries)
}

/// Décompression LZSS de Fallout 1 : suite de blocs précédés d'un entier gros-boutiste sur 16 bits. Bit 15 levé : bloc
/// brut de `n & 0x7FFF` octets ; sinon `n` octets compressés (octet d'indicateurs, bit à 1 = littéral, à 0 = référence
/// sur deux octets : décalage 12 bits, longueur 4 bits + 3) dans un dictionnaire circulaire de 4096 octets.
fn lzss_decode(src: &[u8], size: usize) -> std::result::Result<Vec<u8>, String> {
    const N: usize = 4096;
    let mut out = Vec::with_capacity(size);
    let mut ring = [0x20u8; N];
    let mut r = N - 18;
    let mut p = 0usize;
    while out.len() < size {
        let head = src.get(p..p + 2).ok_or("données tronquées")?;
        p += 2;
        let n = u16::from_be_bytes([head[0], head[1]]);
        if n == 0 {
            return Err("bloc vide".into());
        }
        if n & 0x8000 != 0 {
            let len = (n & 0x7FFF) as usize;
            out.extend_from_slice(src.get(p..p + len).ok_or("données tronquées")?);
            p += len;
            continue;
        }
        let end = p + n as usize;
        let block = src.get(p..end).ok_or("données tronquées")?;
        p = end;
        let mut i = 0;
        while i < block.len() && out.len() < size {
            let flags = block[i];
            i += 1;
            for bit in 0..8 {
                if i >= block.len() || out.len() >= size {
                    break;
                }
                if flags & (1 << bit) != 0 {
                    let b = block[i];
                    i += 1;
                    out.push(b);
                    ring[r] = b;
                    r = (r + 1) % N;
                } else {
                    let (b1, b2) = (
                        *block.get(i).ok_or("données tronquées")?,
                        *block.get(i + 1).ok_or("données tronquées")?,
                    );
                    i += 2;
                    let off = b1 as usize | ((b2 as usize & 0xF0) << 4);
                    for k in 0..(b2 as usize & 0x0F) + 3 {
                        let b = ring[(off + k) % N];
                        out.push(b);
                        ring[r] = b;
                        r = (r + 1) % N;
                    }
                }
            }
        }
    }
    out.truncate(size);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lzss_literals_and_raw_blocks() {
        // Bloc compressé : indicateurs 0b0000_0111 → trois littéraux « abc » ; puis bloc brut « de ».
        let src = [
            0x00,
            0x04,
            0b0000_0111,
            b'a',
            b'b',
            b'c',
            0x80,
            0x02,
            b'd',
            b'e',
        ];
        assert_eq!(lzss_decode(&src, 5).unwrap(), b"abcde");
    }

    #[test]
    fn lzss_back_reference_into_initial_spaces() {
        // Référence au décalage 4078 (zone initialisée d'espaces), longueur 3.
        let off = 4078usize;
        let src = [
            0x00,
            0x03,
            0x00,
            (off & 0xFF) as u8,
            (((off >> 4) & 0xF0) as u8),
        ];
        assert_eq!(lzss_decode(&src, 3).unwrap(), b"   ");
    }

    #[test]
    fn rejects_truncated() {
        assert!(lzss_decode(&[0x00, 0x09, 0x00], 4).is_err());
    }
}
