//! Fichiers `.edg` (limites de défilement) de Fallout 2 CE / sfall : « EDGE » v1 ou v2, gros-boutiste.
//!
//! Par élévation : des zones (rectangles en numéros d'hexagones aux coins) et, en v2, un rectangle de cases carrées et
//! des drapeaux de découpe par côté. Le modèle est sans perte : relire puis réécrire redonne les mêmes octets.
//! Disposition vérifiée sur Gecko (`MapEdgeReader` / `MapEdgeWriter`) et fallout2-ce (`map_edge.cc`).

use crate::bytes::{PutBe, Reader};
use crate::error::{Error, Result};

pub const EDGE_MAGIC: u32 = 0x4544_4745; // « EDGE »
pub const EDGE_ELEVATIONS: usize = 3;

/// Rectangle dans l'ordre du disque : gauche, haut, droite, bas (valeurs conservées telles quelles : le moteur stocke
/// parfois des X inversés).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EdgeRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClipSides {
    pub bottom: bool,
    pub right: bool,
    pub top: bool,
    pub left: bool,
}

impl ClipSides {
    fn unpack(raw: i32) -> Self {
        Self {
            bottom: raw & 1 != 0,
            right: (raw >> 8) & 1 != 0,
            top: (raw >> 16) & 1 != 0,
            left: (raw >> 24) & 1 != 0,
        }
    }

    fn pack(self) -> i32 {
        i32::from(self.bottom)
            | i32::from(self.right) << 8
            | i32::from(self.top) << 16
            | i32::from(self.left) << 24
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeElevation {
    pub zones: Vec<EdgeRect>,
    /// Rectangle de cases carrées (v2 seulement ; valeur par défaut du moteur sinon).
    pub square_rect: EdgeRect,
    pub clip_sides: ClipSides,
}

impl Default for EdgeElevation {
    fn default() -> Self {
        Self {
            zones: Vec::new(),
            square_rect: EdgeRect {
                left: 99,
                top: 0,
                right: 0,
                bottom: 99,
            },
            clip_sides: ClipSides::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapEdge {
    /// 1 ou 2.
    pub version: u32,
    pub elevations: [EdgeElevation; EDGE_ELEVATIONS],
}

impl Default for MapEdge {
    fn default() -> Self {
        Self {
            version: 1,
            elevations: Default::default(),
        }
    }
}

impl MapEdge {
    pub fn total_zones(&self) -> usize {
        self.elevations.iter().map(|e| e.zones.len()).sum()
    }

    /// Une carte sans aucune zone n'a pas de fichier : le moteur n'en écrit pas.
    pub fn is_empty(&self) -> bool {
        self.total_zones() == 0
    }

    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        if r.u32()? != EDGE_MAGIC {
            return Err(Error::format("pas un fichier EDGE (signature incorrecte)"));
        }
        let version = r.u32()?;
        if version != 1 && version != 2 {
            return Err(Error::format(format!("version EDGE non gérée : {version}")));
        }
        if r.i32()? != 0 {
            return Err(Error::format("EDGE : champ réservé non nul"));
        }
        let rect = |r: &mut Reader| -> Result<Option<EdgeRect>> {
            if r.remaining() < 16 {
                return Ok(None);
            }
            Ok(Some(EdgeRect {
                left: r.i32()?,
                top: r.i32()?,
                right: r.i32()?,
                bottom: r.i32()?,
            }))
        };

        let mut edge = MapEdge {
            version,
            ..Default::default()
        };
        // Le dernier entier lu nomme l'élévation de la zone suivante.
        let mut level = 0i32;
        'elevations: for e in 0..EDGE_ELEVATIONS {
            if version == 2 {
                let (Some(square), true) = (rect(&mut r)?, r.remaining() >= 4) else {
                    return Err(Error::format("EDGE v2 tronqué (rectangle de cases)"));
                };
                edge.elevations[e].square_rect = square;
                edge.elevations[e].clip_sides = ClipSides::unpack(r.i32()?);
            }
            if level != e as i32 {
                continue;
            }
            loop {
                let Some(zone) = rect(&mut r)? else {
                    // Fin de fichier : naturelle sur la dernière élévation, corruption ailleurs.
                    if e == EDGE_ELEVATIONS - 1 {
                        break 'elevations;
                    }
                    return Err(Error::format("EDGE tronqué (zone)"));
                };
                edge.elevations[e].zones.push(zone);
                if r.remaining() < 4 {
                    if e == EDGE_ELEVATIONS - 1 {
                        break 'elevations;
                    }
                    return Err(Error::format("EDGE tronqué (indicateur d'élévation)"));
                }
                level = r.i32()?;
                if level != e as i32 {
                    break;
                }
            }
        }
        Ok(edge)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        if self.is_empty() {
            return Err(Error::format("EDGE sans zone : aucun fichier à écrire"));
        }
        // Le format lit la première zone comme étant celle de l'élévation 0 : sans zone à l'élévation 0, le fichier
        // serait mal relu (par le moteur aussi).
        if self.elevations[0].zones.is_empty() {
            return Err(Error::format(
                "EDGE : l'élévation 0 doit avoir au moins une zone",
            ));
        }
        let put_rect = |out: &mut Vec<u8>, r: EdgeRect| {
            out.put_i32(r.left);
            out.put_i32(r.top);
            out.put_i32(r.right);
            out.put_i32(r.bottom);
        };
        let mut out = Vec::new();
        out.put_u32(EDGE_MAGIC);
        out.put_u32(if self.version == 2 { 2 } else { 1 });
        out.put_u32(0);
        let next_with_zones = |from: usize| {
            (from..EDGE_ELEVATIONS)
                .find(|&e| !self.elevations[e].zones.is_empty())
                .unwrap_or(EDGE_ELEVATIONS)
        };
        for (e, data) in self.elevations.iter().enumerate() {
            if self.version == 2 {
                put_rect(&mut out, data.square_rect);
                out.put_i32(data.clip_sides.pack());
            }
            for (i, &zone) in data.zones.iter().enumerate() {
                put_rect(&mut out, zone);
                // La toute dernière zone du fichier, sur la dernière élévation, n'a pas d'indicateur (fin de fichier).
                if e == EDGE_ELEVATIONS - 1 && i + 1 == data.zones.len() {
                    continue;
                }
                let indicator = if i + 1 < data.zones.len() {
                    e
                } else {
                    next_with_zones(e + 1)
                };
                out.put_i32(indicator as i32);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(a: i32) -> EdgeRect {
        EdgeRect {
            left: a,
            top: a + 1,
            right: a + 2,
            bottom: a + 3,
        }
    }

    #[test]
    fn round_trips_with_zones_on_several_elevations() {
        let mut edge = MapEdge::default();
        edge.elevations[0].zones = vec![zone(10), zone(20)];
        edge.elevations[2].zones = vec![zone(30)];
        let bytes = edge.to_bytes().unwrap();
        let back = MapEdge::parse(&bytes).unwrap();
        assert_eq!(back, edge);
        assert_eq!(back.to_bytes().unwrap(), bytes, "octets identiques");
    }

    #[test]
    fn version_two_keeps_square_rect_and_clip_sides() {
        let mut edge = MapEdge {
            version: 2,
            ..Default::default()
        };
        edge.elevations[0].zones = vec![zone(4)];
        edge.elevations[1].zones = vec![zone(5)];
        edge.elevations[1].square_rect = EdgeRect {
            left: 90,
            top: 1,
            right: 2,
            bottom: 80,
        };
        edge.elevations[1].clip_sides = ClipSides {
            bottom: true,
            right: false,
            top: true,
            left: true,
        };
        let bytes = edge.to_bytes().unwrap();
        let back = MapEdge::parse(&bytes).unwrap();
        assert_eq!(back, edge);
        assert_eq!(back.to_bytes().unwrap(), bytes);
    }

    #[test]
    fn empty_edge_has_no_file_and_garbage_is_rejected() {
        assert!(MapEdge::default().to_bytes().is_err());
        let mut upper_only = MapEdge::default();
        upper_only.elevations[1].zones = vec![zone(1)];
        assert!(
            upper_only.to_bytes().is_err(),
            "non représentable : la première zone est lue comme celle de l'élévation 0"
        );
        assert!(MapEdge::parse(b"nope").is_err());
        let mut bad = Vec::new();
        bad.put_u32(EDGE_MAGIC);
        bad.put_u32(7);
        bad.put_u32(0);
        assert!(MapEdge::parse(&bad).is_err());
    }

    #[test]
    fn truncated_file_is_an_error_unless_on_the_last_elevation() {
        let mut edge = MapEdge::default();
        edge.elevations[0].zones = vec![zone(1), zone(2)];
        let bytes = edge.to_bytes().unwrap();
        assert!(MapEdge::parse(&bytes[..bytes.len() - 6]).is_err());
    }
}
