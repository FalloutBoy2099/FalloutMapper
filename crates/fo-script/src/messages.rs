//! Résolution des numéros de message (`message_str(liste, n)`, `gsay_reply`…) en textes des fichiers `.msg`.

use fo_core::GameData;

/// Fournit le texte d'un message. `list` est le numéro de liste de messages donné au script (`None` si non constant).
pub trait MessageSource {
    fn message(&self, list: Option<i32>, id: i32) -> Option<String>;
}

/// Aucune résolution : le pseudo-source garde les numéros seuls.
#[derive(Debug, Clone, Copy)]
pub struct NoMessages;

impl MessageSource for NoMessages {
    fn message(&self, _list: Option<i32>, _id: i32) -> Option<String> {
        None
    }
}

/// Messages de dialogue d'une installation. Le numéro de liste désigne la ligne `n` (à partir de 1) de `scripts.lst` ;
/// sans numéro connu, c'est le script en cours qui est utilisé.
#[derive(Debug)]
pub struct GameMessages<'a> {
    game: &'a GameData,
    own_script: String,
}

impl<'a> GameMessages<'a> {
    pub fn new(game: &'a GameData, own_script: impl Into<String>) -> Self {
        Self {
            game,
            own_script: own_script.into(),
        }
    }
}

impl MessageSource for GameMessages<'_> {
    fn message(&self, list: Option<i32>, id: i32) -> Option<String> {
        let file = match list {
            Some(n) if n > 0 => self.game.script_file(n as usize - 1)?,
            _ => self.own_script.clone(),
        };
        let entry = self.game.script_messages(&file)?;
        Some(summarize(&entry.get(id)?.text))
    }
}

/// Texte sur une ligne, sans `*/` (il fermerait le commentaire), tronqué.
fn summarize(text: &str) -> String {
    const MAX: usize = 90;
    let one_line = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("*/", "* /");
    if one_line.chars().count() > MAX {
        let cut: String = one_line.chars().take(MAX).collect();
        format!("{cut}…")
    } else {
        one_line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_is_single_line_and_safe() {
        assert_eq!(summarize("a\n  b */ c"), "a b * / c");
        assert!(summarize(&"x".repeat(200)).ends_with('…'));
    }
}
