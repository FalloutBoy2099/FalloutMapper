//! Fichiers texte à sections (`maps.txt`, `city.txt`…) lus et réécrits *sans perte* : commentaires, espacement et clés
//! inconnues sont conservés ; seules les lignes modifiées changent.
//!
//! Une section commence par `[Nom]` (éventuellement suivi d'un commentaire). Une ligne de clé est `clé=valeur`, avec un
//! commentaire facultatif après `;`. Les lignes qui commencent par `;` sont des commentaires.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IniSection {
    /// Ligne d'en-tête telle qu'écrite, commentaire compris (`[Map 000]   ; Arroyo`).
    pub header: String,
    /// Nom entre crochets (`Map 000`).
    pub name: String,
    /// Lignes de la section, sans l'en-tête.
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IniDoc {
    /// Lignes avant la première section.
    pub preamble: Vec<String>,
    pub sections: Vec<IniSection>,
    /// Fin de ligne d'origine (`\n` ou `\r\n`).
    pub eol: String,
    /// Le fichier se terminait-il par une fin de ligne ?
    pub final_eol: bool,
}

/// `(clé, valeur, commentaire)` d'une ligne `clé=valeur ; commentaire`, `None` pour un commentaire ou une ligne vide.
fn split_line(line: &str) -> Option<(&str, &str, &str)> {
    let t = line.trim_start();
    if t.is_empty() || t.starts_with(';') {
        return None;
    }
    let (key, rest) = t.split_once('=')?;
    let (value, comment) = match rest.find(';') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    Some((key.trim(), value.trim(), comment))
}

impl IniSection {
    /// Valeur d'une clé (insensible à la casse), sans son commentaire. La dernière occurrence gagne, comme dans le jeu.
    pub fn get(&self, key: &str) -> Option<String> {
        self.lines.iter().rev().find_map(|l| {
            split_line(l)
                .filter(|(k, _, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v, _)| v.to_string())
        })
    }

    /// Change la valeur d'une clé en gardant son commentaire ; l'ajoute après la dernière clé si elle manque.
    pub fn set(&mut self, key: &str, value: &str) {
        let found = self
            .lines
            .iter()
            .rposition(|l| split_line(l).is_some_and(|(k, _, _)| k.eq_ignore_ascii_case(key)));
        match found {
            Some(i) => {
                let (k, _, comment) = split_line(&self.lines[i])
                    .map(|(k, v, c)| (k.to_string(), v.to_string(), c.to_string()))
                    .unwrap();
                let tail = if comment.is_empty() {
                    String::new()
                } else {
                    format!("  {comment}")
                };
                self.lines[i] = format!("{k}={value}{tail}");
            }
            None => {
                let at = self
                    .lines
                    .iter()
                    .rposition(|l| split_line(l).is_some())
                    .map_or(0, |i| i + 1);
                self.lines.insert(at, format!("{key}={value}"));
            }
        }
    }

    pub fn remove(&mut self, key: &str) -> bool {
        let before = self.lines.len();
        self.lines
            .retain(|l| !split_line(l).is_some_and(|(k, _, _)| k.eq_ignore_ascii_case(key)));
        self.lines.len() != before
    }

    /// Les clés présentes, dans l'ordre (avec doublons).
    pub fn keys(&self) -> Vec<String> {
        self.lines
            .iter()
            .filter_map(|l| split_line(l).map(|(k, _, _)| k.to_string()))
            .collect()
    }
}

impl IniDoc {
    pub fn parse(text: &str) -> Self {
        let eol = if text.contains("\r\n") { "\r\n" } else { "\n" }.to_string();
        let mut doc = IniDoc {
            eol,
            ..Default::default()
        };
        let stripped = text
            .strip_suffix("\r\n")
            .or_else(|| text.strip_suffix('\n'));
        doc.final_eol = stripped.is_some();
        let text = stripped.unwrap_or(text);
        for line in text.split('\n') {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let t = line.trim_start();
            let section_name = t
                .strip_prefix('[')
                .and_then(|r| r.split_once(']'))
                .filter(|_| !line.trim_start().starts_with(';'))
                .map(|(n, _)| n.trim().to_string());
            match section_name {
                Some(name) => doc.sections.push(IniSection {
                    header: line.to_string(),
                    name,
                    lines: Vec::new(),
                }),
                None => match doc.sections.last_mut() {
                    Some(s) => s.lines.push(line.to_string()),
                    None => doc.preamble.push(line.to_string()),
                },
            }
        }
        doc
    }

    pub fn to_text(&self) -> String {
        let eol = if self.eol.is_empty() { "\n" } else { &self.eol };
        let mut out: Vec<&str> = self.preamble.iter().map(String::as_str).collect();
        for s in &self.sections {
            out.push(&s.header);
            out.extend(s.lines.iter().map(String::as_str));
        }
        let mut text = out.join(eol);
        if self.final_eol {
            text.push_str(eol);
        }
        text
    }

    pub fn section(&self, name: &str) -> Option<&IniSection> {
        self.sections
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    pub fn section_mut(&mut self, name: &str) -> Option<&mut IniSection> {
        self.sections
            .iter_mut()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "; titre\r\n\r\n[Map 000]  ; premier\r\nlookup_name=Desert 1\r\nmap_name=desert1 ; fichier\r\n; note\r\nsaved=No\r\n\r\n[Map 001]\r\nmap_name=desert2\r\n";

    #[test]
    fn round_trips_byte_for_byte() {
        let d = IniDoc::parse(SAMPLE);
        assert_eq!(d.to_text(), SAMPLE);
        assert_eq!(d.sections.len(), 2);
        assert_eq!(d.preamble.len(), 2);
        let lf = SAMPLE.replace("\r\n", "\n");
        assert_eq!(IniDoc::parse(&lf).to_text(), lf);
    }

    #[test]
    fn reads_values_without_comments_and_ignores_commented_keys() {
        let d = IniDoc::parse(SAMPLE);
        let s = d.section("map 000").unwrap();
        assert_eq!(s.get("MAP_NAME").as_deref(), Some("desert1"));
        assert_eq!(s.get("lookup_name").as_deref(), Some("Desert 1"));
        assert_eq!(s.get("note"), None);
        let doc = IniDoc::parse("[A]\n;x=1\nx=2\nx=3\n");
        assert_eq!(
            doc.section("A").unwrap().get("x").as_deref(),
            Some("3"),
            "la dernière gagne"
        );
    }

    #[test]
    fn set_keeps_comments_and_appends_missing_keys_after_the_last_key() {
        let mut d = IniDoc::parse(SAMPLE);
        let s = d.section_mut("Map 000").unwrap();
        s.set("map_name", "desert9");
        assert!(d.to_text().contains("map_name=desert9  ; fichier"));
        let s = d.section_mut("Map 000").unwrap();
        s.set("music", "07desert");
        let t = d.to_text();
        let music = t.find("music=07desert").unwrap();
        assert!(music > t.find("saved=No").unwrap() && music < t.find("[Map 001]").unwrap());
        assert!(d.section_mut("Map 000").unwrap().remove("music"));
        assert_eq!(
            d.to_text()
                .replace("map_name=desert9  ; fichier", "map_name=desert1 ; fichier"),
            SAMPLE
        );
    }
}
