//! Motifs (« patterns », tampons) : un ensemble d'objets et de tuiles capturé depuis une sélection, qu'on peut reposer
//! ailleurs. Le format JSON est celui de Gecko (`scripts/stamps/*.json`) : les motifs s'échangent entre les deux éditeurs.
//!
//! Un motif a une ou plusieurs *variantes* (orientations dessinées à la main : l'art de Fallout dépend de la direction,
//! on ne tourne pas un motif par calcul). Chaque variante est posée telle quelle, translatée de son ancre vers la cible.
//! Les décalages d'objets sont des différences (colonne, ligne) d'hexagone ; ceux des tuiles, des différences (colonne,
//! ligne) de case carrée. Pour que les deux restent solidaires, la cible est d'abord alignée sur la parité de colonne et
//! de ligne de l'ancre : la pose est donc au grain de la case (l'ancre réelle peut être à un hexagone de la cible).

use crate::error::{Error, Result};
use crate::geom::{HEX_COUNT, HEX_GRID_WIDTH, SQUARE_GRID_WIDTH};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PATTERN_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternObject {
    pub dx_hex: i32,
    pub dy_hex: i32,
    pub pro_pid: u32,
    pub frm_pid: u32,
    pub direction: u32,
    /// Drapeaux de l'instance ; absents dans un motif écrit à la main : ceux du prototype s'appliquent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternTile {
    pub dx_tile: i32,
    pub dy_tile: i32,
    pub tile_id: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternVariant {
    pub label: String,
    pub anchor_hex: u32,
    #[serde(default)]
    pub objects: Vec<PatternObject>,
    #[serde(default)]
    pub floor: Vec<PatternTile>,
    #[serde(default)]
    pub roof: Vec<PatternTile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pattern {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: u32,
    pub variants: Vec<PatternVariant>,
    /// Étiquettes de classement (maison, entrepôt…) : propres à ce mapper, Gecko les ignore.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

fn default_version() -> u32 {
    PATTERN_VERSION
}

/// Un objet capturé : ce que la carte en dit.
#[derive(Debug, Clone, Copy)]
pub struct CapturedObject {
    pub hex: u32,
    pub pro_pid: u32,
    pub frm_pid: u32,
    pub direction: u32,
    pub flags: u32,
}

/// Une case capturée (numéro de case carrée et tuile).
#[derive(Debug, Clone, Copy)]
pub struct CapturedTile {
    pub square: u32,
    pub tile_id: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectPlacement {
    pub hex: u32,
    pub pro_pid: u32,
    pub frm_pid: u32,
    pub direction: u32,
    pub flags: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TilePlacement {
    pub square: u32,
    pub roof: bool,
    pub tile_id: u16,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub objects: Vec<ObjectPlacement>,
    pub tiles: Vec<TilePlacement>,
    /// Éléments dont la cible tombe hors de la grille.
    pub objects_dropped: usize,
    pub tiles_dropped: usize,
}

fn col_row(hex: u32) -> (i32, i32) {
    ((hex % HEX_GRID_WIDTH) as i32, (hex / HEX_GRID_WIDTH) as i32)
}

impl PatternVariant {
    /// Capture une variante : décalages relatifs à `anchor_hex`.
    pub fn capture(
        label: impl Into<String>,
        anchor_hex: u32,
        objects: &[CapturedObject],
        floor: &[CapturedTile],
        roof: &[CapturedTile],
    ) -> Self {
        let (acol, arow) = col_row(anchor_hex);
        let (atx, aty) = (acol / 2, arow / 2);
        let tiles = |src: &[CapturedTile]| {
            src.iter()
                .map(|t| PatternTile {
                    dx_tile: (t.square % SQUARE_GRID_WIDTH) as i32 - atx,
                    dy_tile: (t.square / SQUARE_GRID_WIDTH) as i32 - aty,
                    tile_id: t.tile_id,
                })
                .collect()
        };
        Self {
            label: label.into(),
            anchor_hex,
            objects: objects
                .iter()
                .map(|o| {
                    let (c, r) = col_row(o.hex);
                    PatternObject {
                        dx_hex: c - acol,
                        dy_hex: r - arow,
                        pro_pid: o.pro_pid,
                        frm_pid: o.frm_pid,
                        direction: o.direction,
                        flags: Some(o.flags),
                    }
                })
                .collect(),
            floor: tiles(floor),
            roof: tiles(roof),
        }
    }

    /// Où tombe chaque élément quand l'ancre est posée près de `target_hex`.
    pub fn plan(&self, target_hex: u32) -> Plan {
        let (acol, arow) = col_row(self.anchor_hex);
        let (mut tcol, mut trow) = col_row(target_hex);
        // Aligne la cible sur la parité de l'ancre (au plus un hexagone d'écart).
        let w = HEX_GRID_WIDTH as i32;
        let snap = |t: i32, a: i32| {
            if (t & 1) == (a & 1) {
                t
            } else if t + 1 < w {
                t + 1
            } else {
                t - 1
            }
        };
        tcol = snap(tcol, acol);
        trow = snap(trow, arow);

        let mut plan = Plan::default();
        for o in &self.objects {
            let (c, r) = (tcol + o.dx_hex, trow + o.dy_hex);
            if (0..w).contains(&c) && (0..w).contains(&r) && ((r * w + c) as u32) < HEX_COUNT {
                plan.objects.push(ObjectPlacement {
                    hex: (r * w + c) as u32,
                    pro_pid: o.pro_pid,
                    frm_pid: o.frm_pid,
                    direction: o.direction,
                    flags: o.flags,
                });
            } else {
                plan.objects_dropped += 1;
            }
        }
        let sw = SQUARE_GRID_WIDTH as i32;
        let (ttx, tty) = (tcol / 2, trow / 2);
        for (list, roof) in [(&self.floor, false), (&self.roof, true)] {
            for t in list {
                let (c, r) = (ttx + t.dx_tile, tty + t.dy_tile);
                if (0..sw).contains(&c) && (0..sw).contains(&r) {
                    plan.tiles.push(TilePlacement {
                        square: (r * sw + c) as u32,
                        roof,
                        tile_id: t.tile_id,
                    });
                } else {
                    plan.tiles_dropped += 1;
                }
            }
        }
        plan
    }
}

impl Pattern {
    pub fn new(name: impl Into<String>, first: PatternVariant) -> Self {
        Self {
            name: name.into(),
            version: PATTERN_VERSION,
            variants: vec![first],
            tags: Vec::new(),
        }
    }

    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| Error::format(format!("motif : {e}")))
    }

    pub fn from_json(text: &str) -> Result<Self> {
        let p: Pattern = serde_json::from_str(text)
            .map_err(|e| Error::format(format!("motif invalide : {e}")))?;
        if p.variants.is_empty() {
            return Err(Error::format("motif sans variante"));
        }
        Ok(p)
    }
}

/// Nom de fichier sûr pour un motif.
fn file_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!(
        "{}.json",
        if clean.is_empty() {
            "motif".to_string()
        } else {
            clean
        }
    )
}

/// Tous les motifs (`*.json`) d'un dossier, triés par nom. Les fichiers illisibles sont ignorés.
pub fn load_patterns(dir: &Path) -> Vec<Pattern> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<Pattern> = rd
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("json"))
        })
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|t| Pattern::from_json(&t).ok())
        .collect();
    v.sort_by_key(|p| p.name.to_lowercase());
    v
}

pub fn save_pattern(dir: &Path, p: &Pattern) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let path = dir.join(file_name(&p.name));
    std::fs::write(&path, p.to_json()?).map_err(|e| Error::io(&path, e))?;
    Ok(path)
}

pub fn delete_pattern(dir: &Path, name: &str) -> Result<()> {
    let path = dir.join(file_name(name));
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| Error::io(&path, e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variant() -> PatternVariant {
        // Ancre au (col 100, ligne 100) ; deux objets et deux tuiles de sol.
        let anchor = 100 * HEX_GRID_WIDTH + 100;
        let hex = |c: u32, r: u32| r * HEX_GRID_WIDTH + c;
        PatternVariant::capture(
            "défaut",
            anchor,
            &[
                CapturedObject {
                    hex: hex(100, 100),
                    pro_pid: 1,
                    frm_pid: 2,
                    direction: 0,
                    flags: 8,
                },
                CapturedObject {
                    hex: hex(103, 98),
                    pro_pid: 3,
                    frm_pid: 4,
                    direction: 2,
                    flags: 0,
                },
            ],
            &[
                CapturedTile {
                    square: 50 * 100 + 50,
                    tile_id: 7,
                },
                CapturedTile {
                    square: 51 * 100 + 52,
                    tile_id: 8,
                },
            ],
            &[CapturedTile {
                square: 49 * 100 + 50,
                tile_id: 9,
            }],
        )
    }

    #[test]
    fn plan_at_the_original_anchor_reproduces_the_capture() {
        let v = variant();
        let plan = v.plan(v.anchor_hex);
        assert_eq!(
            plan.objects.iter().map(|o| o.hex).collect::<Vec<_>>(),
            vec![100 * 200 + 100, 98 * 200 + 103]
        );
        assert_eq!(
            plan.tiles
                .iter()
                .map(|t| (t.square, t.roof))
                .collect::<Vec<_>>(),
            vec![(5050, false), (5152, false), (4950, true)]
        );
        assert_eq!((plan.objects_dropped, plan.tiles_dropped), (0, 0));
    }

    #[test]
    fn target_is_snapped_to_the_anchor_parity_and_shape_is_kept() {
        let v = variant();
        // Cible d'une parité différente (col 151, ligne 71) : l'ancre réelle est décalée d'au plus un hexagone.
        let plan = v.plan(71 * 200 + 151);
        let first = plan.objects[0].hex;
        let (c, r) = (first % 200, first / 200);
        assert_eq!((c & 1, r & 1), (0, 0), "même parité que l'ancre (100, 100)");
        assert!((c as i32 - 151).abs() <= 1 && (r as i32 - 71).abs() <= 1);
        // Les décalages relatifs sont conservés.
        let second = plan.objects[1].hex;
        assert_eq!((second % 200) as i32 - c as i32, 3);
        assert_eq!((second / 200) as i32 - r as i32, -2);
        // Les tuiles suivent le même bloc : colonne/2, ligne/2.
        assert_eq!(plan.tiles[0].square, ((r / 2) * 100 + c / 2) as u32);
    }

    #[test]
    fn off_grid_entries_are_dropped_and_counted() {
        let v = variant();
        let plan = v.plan(0);
        assert!(plan.objects_dropped > 0 || plan.tiles_dropped > 0);
        assert_eq!(plan.objects.len() + plan.objects_dropped, 2);
    }

    #[test]
    fn json_is_gecko_compatible() {
        let p = Pattern::new("tente", variant());
        let json = p.to_json().unwrap();
        assert!(
            json.contains("\"anchorHex\"")
                && json.contains("\"dxHex\"")
                && json.contains("\"tileId\"")
        );
        assert_eq!(Pattern::from_json(&json).unwrap(), p);
        // Un motif de Gecko, avec un objet sans drapeaux.
        let g = r#"{"name":"t","version":1,"variants":[{"label":"default","anchorHex":18901,"objects":[{"dxHex":-3,"dyHex":-4,"proPid":50332577,"frmPid":50332618,"direction":0}],"floor":[],"roof":[{"dxTile":0,"dyTile":-2,"tileId":1632}]}]}"#;
        let q = Pattern::from_json(g).unwrap();
        assert_eq!(q.variants[0].objects[0].flags, None);
        assert_eq!(q.variants[0].roof[0].tile_id, 1632);
        assert!(Pattern::from_json(r#"{"name":"x","variants":[]}"#).is_err());
    }

    #[test]
    fn library_round_trip_on_disk() {
        let dir = std::env::temp_dir().join(format!("fomapper_patterns_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = Pattern::new("Ma tente / test", variant());
        let path = save_pattern(&dir, &p).unwrap();
        assert!(path.ends_with("Ma_tente___test.json"));
        assert_eq!(load_patterns(&dir), vec![p.clone()]);
        delete_pattern(&dir, &p.name).unwrap();
        assert!(load_patterns(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
