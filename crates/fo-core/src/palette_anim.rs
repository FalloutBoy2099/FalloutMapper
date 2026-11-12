//! Animations de palette (« color cycling ») de Fallout : certains indices de la palette tournent sur eux-mêmes ou
//! pulsent, ce qui anime l'eau, la boue, le feu, les écrans et l'alarme sans changer les images.
//!
//! Données : <https://fallout.wiki/wiki/Pal_animations>. Chaque animation fait tourner ses couleurs vers les indices
//! inférieurs à chaque pas ; le moteur écrit les valeurs `>> 2` (palette 6 bits).

use crate::formats::Palette;

#[derive(Debug, Clone, Copy)]
pub struct Cycle {
    pub name: &'static str,
    pub first: u8,
    /// Couleurs (RVB 8 bits) dans l'ordre des indices au pas 0. Vide pour l'alarme (pulsation).
    pub colors: &'static [[u8; 3]],
    pub period_ms: u32,
}

pub const ALARM_INDEX: u8 = 254;
const ALARM_STEPS: u64 = 32;

pub const CYCLES: [Cycle; 6] = [
    Cycle {
        name: "boue",
        first: 229,
        colors: &[[0, 108, 0], [11, 115, 7], [27, 123, 15], [43, 131, 27]],
        period_ms: 200,
    },
    Cycle {
        name: "rivage",
        first: 248,
        colors: &[
            [83, 63, 43],
            [75, 59, 43],
            [67, 55, 39],
            [63, 51, 39],
            [55, 47, 35],
            [51, 43, 35],
        ],
        period_ms: 200,
    },
    Cycle {
        name: "feu lent",
        first: 238,
        colors: &[
            [255, 0, 0],
            [215, 0, 0],
            [147, 43, 11],
            [255, 119, 0],
            [255, 59, 0],
        ],
        period_ms: 200,
    },
    Cycle {
        name: "feu rapide",
        first: 243,
        colors: &[
            [71, 0, 0],
            [123, 0, 0],
            [179, 0, 0],
            [123, 0, 0],
            [71, 0, 0],
        ],
        period_ms: 142,
    },
    Cycle {
        name: "écrans",
        first: 233,
        colors: &[
            [107, 107, 111],
            [99, 103, 127],
            [87, 107, 143],
            [0, 147, 163],
            [107, 187, 255],
        ],
        period_ms: 100,
    },
    Cycle {
        name: "alarme",
        first: ALARM_INDEX,
        colors: &[],
        period_ms: 33,
    },
];

/// Masque (un bit par animation de [`CYCLES`]) de l'animation qui porte cet indice de palette, 0 si aucune.
pub fn animation_mask(index: u8) -> u8 {
    CYCLES
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            index >= c.first && (index as usize) < c.first as usize + c.colors.len().max(1)
        })
        .fold(0, |m, (i, _)| m | 1 << i)
}

/// Valeur 8 bits d'une couleur après passage par la palette 6 bits du moteur.
fn via_six_bits(v: u8) -> u8 {
    (((v >> 2) as u32 * 255 + 31) / 63) as u8
}

#[derive(Debug, Clone, Copy)]
pub struct PaletteAnim {
    /// `cycle_speed_factor` de `fallout2.cfg` : plus grand = plus lent.
    pub speed_factor: f32,
}

impl Default for PaletteAnim {
    fn default() -> Self {
        Self { speed_factor: 1.0 }
    }
}

impl PaletteAnim {
    fn step(&self, cycle: &Cycle, t_ms: u64) -> u64 {
        let period = (cycle.period_ms as f32 * self.speed_factor.max(0.05)).max(1.0) as u64;
        t_ms / period
    }

    /// Clé qui change exactement quand l'aspect d'un sprite utilisant les animations de `mask` change.
    pub fn key(&self, mask: u8, t_ms: u64) -> u64 {
        let mut key = 0u64;
        for (i, c) in CYCLES
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
        {
            let len = if c.colors.is_empty() {
                ALARM_STEPS
            } else {
                c.colors.len() as u64
            };
            key = key * 64 + self.step(c, t_ms) % len + 1 + (i as u64 & 0);
        }
        key
    }

    /// Couleur RVB actuelle d'un indice animé, `None` si l'indice n'est pas animé.
    pub fn color(&self, index: u8, t_ms: u64) -> Option<[u8; 3]> {
        for c in &CYCLES {
            if c.colors.is_empty() {
                if index == c.first {
                    let s = self.step(c, t_ms) % ALARM_STEPS;
                    let v = if s < ALARM_STEPS / 2 {
                        s * 4
                    } else {
                        (ALARM_STEPS - s) * 4
                    }
                    .min(60) as u8;
                    return Some([(v * 255 + 31) / 63, 0, 0]);
                }
            } else if index >= c.first && ((index - c.first) as usize) < c.colors.len() {
                let n = c.colors.len();
                // Chaque couleur passe à l'indice précédent à chaque pas.
                let rgb = c.colors[((index - c.first) as usize + self.step(c, t_ms) as usize) % n];
                return Some(rgb.map(via_six_bits));
            }
        }
        None
    }

    /// Image RGBA d'un sprite indexé à l'instant `t_ms`.
    pub fn render(&self, indices: &[u8], palette: &Palette, t_ms: u64) -> Vec<u8> {
        let mut out = Vec::with_capacity(indices.len() * 4);
        for &i in indices {
            match self.color(i, t_ms) {
                Some([r, g, b]) if i != 0 => out.extend_from_slice(&[r, g, b, 255]),
                _ => out.extend_from_slice(&palette.colors[i as usize]),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_match_the_documented_ranges() {
        assert_eq!(animation_mask(229), 1 << 0);
        assert_eq!(animation_mask(232), 1 << 0);
        assert_eq!(animation_mask(233), 1 << 4);
        assert_eq!(animation_mask(238), 1 << 2);
        assert_eq!(animation_mask(243), 1 << 3);
        assert_eq!(animation_mask(248), 1 << 1);
        assert_eq!(animation_mask(253), 1 << 1);
        assert_eq!(animation_mask(254), 1 << 5);
        assert_eq!(animation_mask(228), 0);
        assert_eq!(animation_mask(255), 0);
        assert_eq!(animation_mask(0), 0);
    }

    #[test]
    fn colors_rotate_toward_lower_indices() {
        let a = PaletteAnim::default();
        let at0 = a.color(229, 0).unwrap();
        // Au pas 1, l'indice 229 prend la couleur qu'avait 230.
        assert_eq!(a.color(229, 200).unwrap(), a.color(230, 0).unwrap());
        // Une boucle complète (4 pas) revient au départ.
        assert_eq!(a.color(229, 800).unwrap(), at0);
        assert!(a.color(100, 0).is_none());
    }

    #[test]
    fn key_changes_only_when_a_step_changes() {
        let a = PaletteAnim::default();
        assert_eq!(a.key(1, 0), a.key(1, 199));
        assert_ne!(a.key(1, 0), a.key(1, 200));
        assert_eq!(a.key(1, 0), a.key(1, 800));
        assert_eq!(a.key(0, 12345), 0);
    }

    #[test]
    fn slower_factor_slows_steps() {
        let slow = PaletteAnim { speed_factor: 2.0 };
        assert_eq!(slow.key(1, 0), slow.key(1, 399));
        assert_ne!(slow.key(1, 0), slow.key(1, 400));
    }

    #[test]
    fn render_replaces_only_animated_indices() {
        let a = PaletteAnim::default();
        let pal = Palette::grayscale();
        let out = a.render(&[0, 100, 229], &pal, 0);
        assert_eq!(&out[0..4], &pal.colors[0]);
        assert_eq!(&out[4..8], &pal.colors[100]);
        assert_eq!(out[11], 255);
        assert_ne!(&out[8..11], &pal.colors[229][..3]);
    }
}
