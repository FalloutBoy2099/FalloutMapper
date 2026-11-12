//! Fichiers de messages `.msg` : entrées `{numéro}{audio}{texte}`, lignes de commentaire commençant par `#`.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsgEntry {
    /// Nom du fichier audio associé (souvent vide).
    pub audio: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MsgFile {
    pub entries: BTreeMap<i32, MsgEntry>,
}

impl MsgFile {
    pub fn get(&self, id: i32) -> Option<&MsgEntry> {
        self.entries.get(&id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Lit un fichier `.msg`. Tolérant : une entrée mal formée est ignorée, le reste est conservé.
    pub fn parse(data: &[u8]) -> Self {
        let chars: Vec<char> = decode(data).chars().collect();
        let mut entries = BTreeMap::new();
        for e in scan(&chars) {
            entries.insert(
                e.id,
                MsgEntry {
                    audio: e.audio,
                    text: e.text,
                },
            );
        }
        Self { entries }
    }
}

/// Une entrée repérée dans le texte, avec sa position (indices de caractères, `{` initial compris, `}` final compris).
struct Scanned {
    id: i32,
    audio: String,
    text: String,
    start: usize,
    end: usize,
}

fn scan(chars: &[char]) -> Vec<Scanned> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut at_line_start = true;
    while i < chars.len() {
        match chars[i] {
            '#' if at_line_start => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '{' => {
                let entry_start = i;
                let mut groups: Vec<String> = Vec::with_capacity(3);
                while groups.len() < 3 && i < chars.len() && chars[i] == '{' {
                    let start = i + 1;
                    let Some(len) = chars[start..].iter().position(|&c| c == '}') else {
                        i = chars.len();
                        break;
                    };
                    groups.push(chars[start..start + len].iter().collect());
                    i = start + len + 1;
                    // Espaces et sauts de ligne autorisés entre les groupes d'une entrée.
                    while groups.len() < 3 && i < chars.len() && chars[i].is_whitespace() {
                        i += 1;
                    }
                }
                if let [num, audio, text] = groups.as_slice()
                    && let Ok(id) = num.trim().parse::<i32>()
                {
                    out.push(Scanned {
                        id,
                        audio: audio.trim().to_string(),
                        text: text.replace('\r', ""),
                        start: entry_start,
                        end: i,
                    });
                }
                at_line_start = false;
                continue;
            }
            _ => {}
        }
        at_line_start = chars.get(i) == Some(&'\n');
        i += 1;
    }
    out
}

/// Modification d'un fichier de messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MsgEdit {
    /// Crée l'entrée ou remplace son audio et son texte.
    Set {
        id: i32,
        audio: String,
        text: String,
    },
    /// Supprime l'entrée.
    Delete(i32),
}

/// Applique des modifications au contenu brut d'un `.msg` en ne touchant qu'aux entrées concernées : commentaires, mise en
/// page, ordre et codage des autres entrées restent identiques à l'octet. Les entrées nouvelles sont ajoutées à la fin, par
/// numéro croissant. Les retours à la ligne des textes réécrits suivent ceux du fichier (`\r\n` ou `\n`).
pub fn patch(data: &[u8], edits: &[MsgEdit]) -> Vec<u8> {
    let chars: Vec<char> = decode(data).chars().collect();
    let crlf = data.windows(2).any(|w| w == b"\r\n");
    let eol = if crlf { "\r\n" } else { "\n" };
    let render = |id: i32, audio: &str, text: &str| {
        format!(
            "{{{id}}}{{{audio}}}{{{}}}",
            text.replace("\r\n", "\n").replace('\n', eol)
        )
    };
    // Dernière modification de chaque numéro.
    let mut last: BTreeMap<i32, &MsgEdit> = BTreeMap::new();
    for e in edits {
        match e {
            MsgEdit::Set { id, .. } | MsgEdit::Delete(id) => {
                last.insert(*id, e);
            }
        }
    }
    let scanned = scan(&chars);
    let mut out = String::with_capacity(chars.len() + 64);
    let mut pos = 0;
    let mut written: std::collections::BTreeSet<i32> = std::collections::BTreeSet::new();
    for entry in &scanned {
        out.extend(&chars[pos..entry.start]);
        pos = entry.end;
        match last.get(&entry.id) {
            None => out.extend(&chars[entry.start..entry.end]),
            Some(MsgEdit::Delete(_)) => {
                // On retire aussi la fin de la ligne pour ne pas laisser de ligne vide.
                if chars.get(pos) == Some(&'\r') {
                    pos += 1;
                }
                if chars.get(pos) == Some(&'\n') {
                    pos += 1;
                }
            }
            Some(MsgEdit::Set { audio, text, .. }) => {
                // Une entrée en double dans le fichier n'est réécrite qu'une fois.
                if written.insert(entry.id) {
                    out.push_str(&render(entry.id, audio, text));
                } else if chars.get(pos) == Some(&'\n') {
                    pos += 1;
                }
            }
        }
    }
    out.extend(&chars[pos..]);
    // Entrées nouvelles : à la fin, une par ligne.
    let known: std::collections::BTreeSet<i32> = scanned.iter().map(|e| e.id).collect();
    for (id, e) in &last {
        if let MsgEdit::Set { audio, text, .. } = e
            && !known.contains(id)
        {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push_str(eol);
            }
            out.push_str(&render(*id, audio, text));
            out.push_str(eol);
        }
    }
    encode(&out)
}

/// Encode en Windows-1252 (l'inverse de `decode`) ; un caractère absent de ce codage devient « ? ».
pub fn encode(text: &str) -> Vec<u8> {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž',
        '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}',
        'ž', 'Ÿ',
    ];
    text.chars()
        .map(|c| match c as u32 {
            0..=0x7F | 0xA0..=0xFF => c as u32 as u8,
            _ => HIGH
                .iter()
                .position(|h| *h == c)
                .map_or(b'?', |i| 0x80 + i as u8),
        })
        .collect()
}

/// Windows-1252 : les textes du jeu en français, allemand, etc. n'ont pas d'autre codage.
fn decode(data: &[u8]) -> String {
    const CP1252_HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž',
        '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}',
        'ž', 'Ÿ',
    ];
    data.iter()
        .map(|&b| match b {
            0x80..=0x9F => CP1252_HIGH[(b - 0x80) as usize],
            b => b as char,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_is_the_inverse_of_decoding_for_every_byte() {
        let all: Vec<u8> = (0..=255u8).collect();
        assert_eq!(encode(&decode(&all)), all);
        assert_eq!(encode("€ ’ é Ω"), b"\x80 \x92 \xe9 ?");
    }

    #[test]
    fn patch_only_touches_the_edited_entries() {
        let src = b"# en-t\xeate {1}{}{non}\r\n{100}{}{Bonjour}\r\n\r\n{101}{snd01}{Voil\xe0,\r\n deux lignes}\r\n  {102}{}{Reste}\r\n";
        // Aucune modification : identique.
        assert_eq!(patch(src, &[]), src);
        let out = patch(
            src,
            &[MsgEdit::Set {
                id: 101,
                audio: "snd02".into(),
                text: "Caf\u{e9}\nencore \u{20ac}".into(),
            }],
        );
        let m = MsgFile::parse(&out);
        assert_eq!(m.get(101).unwrap().audio, "snd02");
        assert_eq!(m.get(101).unwrap().text, "Caf\u{e9}\nencore \u{20ac}");
        assert_eq!(m.get(100).unwrap().text, "Bonjour");
        assert_eq!(m.get(102).unwrap().text, "Reste");
        // Le début et la fin du fichier sont intacts à l'octet (commentaire accentué, mise en page, retours à la ligne).
        assert!(out.starts_with(b"# en-t\xeate {1}{}{non}\r\n{100}{}{Bonjour}\r\n\r\n"));
        assert!(out.ends_with(b"\r\n  {102}{}{Reste}\r\n"));
        assert!(
            out.windows(7).any(|w| w == b"Caf\xe9\r\ne"),
            "les nouveaux retours à la ligne suivent le fichier"
        );
    }

    #[test]
    fn patch_adds_and_deletes_entries() {
        let src = b"{1}{}{un}\n{2}{}{deux}\n{3}{}{trois}\n";
        let out = patch(
            src,
            &[
                MsgEdit::Delete(2),
                MsgEdit::Set {
                    id: 9,
                    audio: String::new(),
                    text: "neuf".into(),
                },
                MsgEdit::Set {
                    id: 5,
                    audio: String::new(),
                    text: "cinq".into(),
                },
            ],
        );
        assert_eq!(out, b"{1}{}{un}\n{3}{}{trois}\n{5}{}{cinq}\n{9}{}{neuf}\n");
        let again = patch(&out, &[MsgEdit::Delete(5), MsgEdit::Delete(9)]);
        assert_eq!(again, b"{1}{}{un}\n{3}{}{trois}\n");
    }

    #[test]
    fn parses_entries_comments_and_multiline() {
        let src = b"# commentaire {1}{}{non}\r\n{100}{}{Bonjour,\r\n monde}\r\n\r\n{101}{snd01}{Voil\xe0 \x92}\r\n{bad}{}{x}\r\n";
        let m = MsgFile::parse(src);
        assert_eq!(m.len(), 2);
        assert_eq!(m.get(100).unwrap().text, "Bonjour,\n monde");
        assert_eq!(m.get(101).unwrap().audio, "snd01");
        assert_eq!(m.get(101).unwrap().text, "Voilà ’");
        assert!(m.get(1).is_none());
    }
}
