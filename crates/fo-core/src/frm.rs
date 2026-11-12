//! Images FRM de Fallout 1 & 2 : 8 bits indexés, jusqu'à 6 directions, gros-boutiste.

use crate::bytes::Reader;
use crate::error::{Error, Result};

const HEADER_SIZE: usize = 0x3E;

#[derive(Debug, Clone)]
pub struct FrmFrame {
    pub width: u16,
    pub height: u16,
    /// Décalage de cette image par rapport à la précédente.
    pub dx: i16,
    pub dy: i16,
    /// Indices de palette, ligne par ligne. 0 = transparent.
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct FrmDirection {
    pub shift_x: i16,
    pub shift_y: i16,
    pub frames: Vec<FrmFrame>,
}

#[derive(Debug, Clone)]
pub struct Frm {
    pub version: u32,
    pub fps: u16,
    pub action_frame: u16,
    pub directions: Vec<FrmDirection>,
}

impl Frm {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < HEADER_SIZE {
            return Err(Error::format("FRM trop court"));
        }
        let mut r = Reader::new(data);
        let version = r.u32()?;
        let fps = r.u16()?;
        let action_frame = r.u16()?;
        let frames_per_dir = r.u16()? as usize;
        let mut shift_x = [0i16; 6];
        let mut shift_y = [0i16; 6];
        let mut offsets = [0u32; 6];
        for v in &mut shift_x {
            *v = r.i16()?;
        }
        for v in &mut shift_y {
            *v = r.i16()?;
        }
        for v in &mut offsets {
            *v = r.u32()?;
        }
        let _data_size = r.u32()?;
        if frames_per_dir == 0 {
            return Err(Error::format("FRM sans image"));
        }
        // La direction 0 commence toujours à 0 ; les suivantes sont absentes si leur décalage vaut 0.
        let dir_count = (1..6).rev().find(|&d| offsets[d] != 0).map_or(1, |d| d + 1);

        let mut directions = Vec::with_capacity(dir_count);
        for d in 0..dir_count {
            let mut pos = HEADER_SIZE + offsets[d] as usize;
            let mut frames = Vec::with_capacity(frames_per_dir);
            for _ in 0..frames_per_dir {
                let mut fr = Reader::at(data, pos);
                let width = fr.u16()?;
                let height = fr.u16()?;
                let size = fr.u32()? as usize;
                let dx = fr.i16()?;
                let dy = fr.i16()?;
                let n = width as usize * height as usize;
                let pixels = fr.bytes(n)?.to_vec();
                frames.push(FrmFrame {
                    width,
                    height,
                    dx,
                    dy,
                    pixels,
                });
                pos += 12 + size.max(n);
            }
            directions.push(FrmDirection {
                shift_x: shift_x[d],
                shift_y: shift_y[d],
                frames,
            });
        }
        Ok(Self {
            version,
            fps,
            action_frame,
            directions,
        })
    }

    pub fn frames_per_direction(&self) -> usize {
        self.directions.first().map_or(0, |d| d.frames.len())
    }

    /// Direction demandée, ou la 0 si le FRM n'en a qu'une (cas des décors et des tuiles).
    pub fn direction(&self, dir: usize) -> &FrmDirection {
        self.directions.get(dir).unwrap_or(&self.directions[0])
    }

    /// Position du coin haut-gauche d'une image par rapport à son point d'ancrage (bas, centre).
    pub fn frame_origin(&self, dir: usize, frame: usize) -> (i32, i32) {
        // Comme le Mapper original et Gecko : l'image est posée sur le centre de son hexagone, bas et milieu, puis
        // décalée du « shift » de sa direction (`GetDoffX/Y`). Les décalages propres aux images valent 0 pour l'art
        // fixe ; on les ajoute (cumulés) pour les animations.
        let d = self.direction(dir);
        let last = frame.min(d.frames.len().saturating_sub(1));
        let (mut cx, mut cy) = (d.shift_x as i32, d.shift_y as i32);
        for f in &d.frames[..=last] {
            cx += f.dx as i32;
            cy += f.dy as i32;
        }
        let f = &d.frames[last];
        (cx - f.width as i32 / 2, cy - f.height as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(w: u16, h: u16) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend(4u32.to_be_bytes());
        v.extend(10u16.to_be_bytes());
        v.extend(0u16.to_be_bytes());
        v.extend(1u16.to_be_bytes());
        v.extend([0u8; 24]); // décalages x/y
        v.extend([0u8; 24]); // décalages de données
        v.extend((12 + w as u32 * h as u32).to_be_bytes());
        v.extend(w.to_be_bytes());
        v.extend(h.to_be_bytes());
        v.extend((w as u32 * h as u32).to_be_bytes());
        v.extend([0u8; 4]);
        v.extend(vec![7u8; w as usize * h as usize]);
        v
    }

    #[test]
    fn parses_single_frame() {
        let f = Frm::parse(&build(4, 2)).unwrap();
        assert_eq!(f.directions.len(), 1);
        assert_eq!(f.frames_per_direction(), 1);
        assert_eq!(f.directions[0].frames[0].pixels.len(), 8);
        assert_eq!(f.frame_origin(0, 0), (-2, -2));
    }

    #[test]
    fn truncated_is_error() {
        let mut d = build(4, 2);
        d.truncate(d.len() - 3);
        assert!(Frm::parse(&d).is_err());
    }
}
