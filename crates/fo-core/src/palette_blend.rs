//! Tables de transparence de Fallout : niveaux de gris du sprite, teinte de l'effet,
//! mélange RGB555 puis table d'intensité. Référence : fallout2-ce/src/{color,object}.cc.
use crate::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Effect {
    Red,
    Wall,
    Glass,
    Steam,
    Energy,
}

#[derive(Debug, Clone)]
pub struct PaletteTables {
    inverse: Vec<u8>,
    intensity: Vec<u8>,
    gray: [[u8; 256]; 2],
    blend: Vec<u8>,
}

fn rgb5(c: [u8; 4]) -> [usize; 3] {
    // Palette::parse expands 6 bits to 8; recover the original six-bit value first.
    [c[0], c[1], c[2]].map(|v| ((v as usize * 63 + 127) / 255) >> 1)
}
fn packed([r, g, b]: [usize; 3]) -> usize {
    (r << 10) | (g << 5) | b
}

impl PaletteTables {
    pub fn new(pal: &Palette, data: &[u8]) -> Self {
        let rgb: Vec<_> = pal
            .colors
            .iter()
            .copied()
            .enumerate()
            .map(|(i, c)| match data.get(i * 3..i * 3 + 3) {
                Some(raw) if raw.iter().any(|&v| v > 63) => [0; 3],
                Some(raw) => [
                    raw[0] as usize >> 1,
                    raw[1] as usize >> 1,
                    raw[2] as usize >> 1,
                ],
                None => rgb5(c),
            })
            .collect();
        let inverse = if data.len() >= 768 + 32768 {
            data[768..768 + 32768].to_vec()
        } else {
            (0..32768)
                .map(|v| {
                    let q = [v >> 10, (v >> 5) & 31, v & 31];
                    (0..256)
                        .min_by_key(|&i| {
                            (0..3)
                                .map(|c| (q[c] - rgb[i][c] as i32).pow(2))
                                .sum::<i32>()
                        })
                        .unwrap() as u8
                })
                .collect()
        };
        let offset = 768 + 32768;
        let intensity =
            if data.get(offset..offset + 4) == Some(b"CWEN") && data.len() >= offset + 4 + 65536 {
                data[offset + 4..offset + 4 + 65536].to_vec()
            } else {
                (0..65536)
                    .map(|v| {
                        let c = v / 256;
                        // The engine leaves unmapped (animated) colors unchanged.
                        if data
                            .get(c * 3..c * 3 + 3)
                            .is_some_and(|s| s.iter().any(|&v| v > 63))
                        {
                            return c as u8;
                        }
                        let step = v % 128;
                        let q = rgb[c].map(|x| {
                            if v % 256 < 128 {
                                x * step / 128
                            } else {
                                x + (31 - x) * step / 128
                            }
                        });
                        inverse[packed(q)]
                    })
                    .collect()
            };
        let mut gray = [[0; 256]; 2];
        for (i, &[r, g, b]) in rgb.iter().enumerate().skip(1) {
            gray[0][i] = (((b + 3 * r + 6 * g) / 10) >> 2) as u8;
            gray[1][i] = (((r + 5 * g + 4 * b) / 10) >> 2) as u8;
        }
        let mut blend = vec![0; 5 * 8 * 256];
        for (effect, tint) in [
            [255, 0, 0],
            [197, 214, 255],
            [74, 255, 255],
            [255, 255, 255],
            [239, 255, 8],
        ]
        .into_iter()
        .enumerate()
        {
            let tint_index = inverse[packed(tint.map(|v| v >> 3))] as usize;
            for level in 0..8 {
                for (dest, dst) in rgb.iter().enumerate() {
                    blend[(effect * 8 + level) * 256 + dest] = if level == 0 {
                        dest as u8
                    } else {
                        inverse[packed(std::array::from_fn(|c| {
                            (rgb[tint_index][c] * level + dst[c] * (7 - level)) / 7
                        }))]
                    };
                }
            }
        }
        Self {
            inverse,
            intensity,
            gray,
            blend,
        }
    }

    pub fn nearest(&self, rgba: [u8; 4]) -> u8 {
        self.inverse[packed([rgba[0], rgba[1], rgba[2]].map(|v| v as usize >> 3))]
    }
    pub fn shade(&self, index: u8, light: f32) -> u8 {
        let step = (light.clamp(0.0, 1.0) * 128.0) as usize;
        self.intensity[index as usize * 256 + step]
    }
    pub fn mix(&self, effect: Effect, source: u8, dest: u8, light: f32) -> u8 {
        if source == 0 {
            return dest;
        }
        let level = self.gray[usize::from(effect == Effect::Glass)][source as usize] as usize;
        let mixed = self.blend[(effect as usize * 8 + level) * 256 + dest as usize];
        self.shade(mixed, if effect == Effect::Wall { 1.0 } else { light })
    }
}
