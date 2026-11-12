//! Taille et position de la fenêtre : mémorisées dans la valeur `window` du registre (à défaut, l'ancien `window.txt` du dossier de
//! stockage de l'application est lu une fois), lues avant la création de la fenêtre. Sans position mémorisée, la fenêtre démarre au centre de l'écran.

use std::path::PathBuf;

/// Nom de la valeur du registre qui garde la taille et la position de la fenêtre.
const KEY: &str = "window";

pub(crate) const DEFAULT_SIZE: [f32; 2] = [1500.0, 900.0];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WindowState {
    pub remember_size: bool,
    pub remember_pos: bool,
    pub size: Option<[f32; 2]>,
    pub pos: Option<[f32; 2]>,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            remember_size: true,
            remember_pos: true,
            size: None,
            pos: None,
        }
    }
}

fn pair(v: &str) -> Option<[f32; 2]> {
    let (a, b) = v.split_once(' ')?;
    Some([a.parse().ok()?, b.parse().ok()?])
}

impl WindowState {
    fn path() -> Option<PathBuf> {
        eframe::storage_dir("FalloutMapper").map(|d| d.join("window.txt"))
    }

    pub(crate) fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            match k.trim() {
                "remember_size" => s.remember_size = v.trim() != "0",
                "remember_pos" => s.remember_pos = v.trim() != "0",
                "size" => s.size = pair(v.trim()).filter(|p| p[0] >= 400.0 && p[1] >= 300.0),
                "pos" => s.pos = pair(v.trim()).filter(|p| p[0] > -10_000.0 && p[1] > -10_000.0),
                _ => {}
            }
        }
        s
    }

    pub(crate) fn render(&self) -> String {
        let mut out = format!(
            "remember_size={}
remember_pos={}
",
            u8::from(self.remember_size),
            u8::from(self.remember_pos)
        );
        if let Some([w, h]) = self.size {
            out += &format!(
                "size={w} {h}
"
            );
        }
        if let Some([x, y]) = self.pos {
            out += &format!(
                "pos={x} {y}
"
            );
        }
        out
    }

    pub(crate) fn load() -> Self {
        crate::settings::get(KEY)
            .or_else(|| Self::path().and_then(|p| std::fs::read_to_string(p).ok()))
            .map_or_else(Self::default, |t| Self::parse(&t))
    }

    pub(crate) fn save(&self) {
        crate::settings::set_string(KEY, self.render());
    }

    /// Taille de départ de la fenêtre.
    pub(crate) fn start_size(&self) -> [f32; 2] {
        self.size
            .filter(|_| self.remember_size)
            .unwrap_or(DEFAULT_SIZE)
    }

    /// Position de départ ; `None` : centrer sur l'écran.
    pub(crate) fn start_pos(&self) -> Option<[f32; 2]> {
        self.pos.filter(|_| self.remember_pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let s = WindowState {
            remember_size: true,
            remember_pos: false,
            size: Some([1200.0, 800.0]),
            pos: Some([10.0, 20.0]),
        };
        assert_eq!(WindowState::parse(&s.render()), s);
        assert_eq!(s.start_pos(), None, "position non mémorisée : centré");
        assert_eq!(s.start_size(), [1200.0, 800.0]);
        let d = WindowState::parse("");
        assert_eq!((d.start_size(), d.start_pos()), (DEFAULT_SIZE, None));
    }

    #[test]
    fn rejects_absurd_values() {
        let s = WindowState::parse(
            "size=10 10
pos=-32000 -32000
",
        );
        assert_eq!((s.size, s.pos), (None, None));
    }
}
