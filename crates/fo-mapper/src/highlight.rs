//! Coloration syntaxique du pseudo-source et du listing des scripts.

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{Color32, FontId};

const KEYWORDS: &[&str] = &[
    "if",
    "else",
    "while",
    "for",
    "foreach",
    "return",
    "procedure",
    "begin",
    "end",
    "variable",
    "import",
    "export",
    "call",
    "then",
    "do",
    "not",
    "and",
    "or",
    "bwand",
    "bwor",
    "bwxor",
    "bwnot",
    "break",
    "continue",
    "in",
    "goto",
    "switch",
    "case",
    "default",
    "true",
    "false",
    "none",
];

struct Palette {
    plain: Color32,
    keyword: Color32,
    number: Color32,
    string: Color32,
    comment: Color32,
    name: Color32,
}

fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            plain: Color32::from_rgb(210, 214, 220),
            keyword: Color32::from_rgb(197, 134, 235),
            number: Color32::from_rgb(240, 170, 100),
            string: Color32::from_rgb(140, 210, 120),
            comment: Color32::from_rgb(120, 130, 140),
            name: Color32::from_rgb(110, 180, 245),
        }
    } else {
        Palette {
            plain: Color32::from_rgb(40, 44, 52),
            keyword: Color32::from_rgb(140, 40, 180),
            number: Color32::from_rgb(190, 100, 20),
            string: Color32::from_rgb(40, 130, 40),
            comment: Color32::from_rgb(120, 130, 140),
            name: Color32::from_rgb(20, 90, 190),
        }
    }
}

/// Découpe `text` en segments colorés : mots-clés, nombres, chaînes, commentaires (`//` et lignes `;`), noms de procédures
/// (mot suivi de `(`) et noms d'opcodes en début de ligne du listing.
pub(crate) fn highlight(text: &str, font: FontId, dark: bool) -> LayoutJob {
    let pal = palette(dark);
    let mut job = LayoutJob::default();
    job.text.reserve(text.len());
    let fmt = |color: Color32| TextFormat {
        font_id: font.clone(),
        color,
        ..Default::default()
    };
    for (n, line) in text.split_inclusive('\n').enumerate() {
        let _ = n;
        let trimmed = line.trim_start();
        if trimmed.starts_with(';') || trimmed.starts_with("//") {
            job.append(line, 0.0, fmt(pal.comment));
            continue;
        }
        let bytes = line.as_bytes();
        let mut i = 0;
        let mut plain_from = 0;
        let flush = |job: &mut LayoutJob, from: usize, to: usize| {
            if to > from {
                job.append(&line[from..to], 0.0, fmt(pal.plain));
            }
        };
        while i < bytes.len() {
            let c = bytes[i];
            if c == b'/' && bytes.get(i + 1) == Some(&b'/') {
                flush(&mut job, plain_from, i);
                job.append(&line[i..], 0.0, fmt(pal.comment));
                plain_from = line.len();
                i = line.len();
            } else if c == b'"' {
                flush(&mut job, plain_from, i);
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != b'"' {
                    if bytes[j] == b'\\' {
                        j += 1;
                    }
                    j += 1;
                }
                let end = (j + 1).min(line.len());
                job.append(&line[i..end], 0.0, fmt(pal.string));
                plain_from = end;
                i = end;
            } else if c.is_ascii_digit() {
                flush(&mut job, plain_from, i);
                let mut j = i;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'.') {
                    j += 1;
                }
                job.append(&line[i..j], 0.0, fmt(pal.number));
                plain_from = j;
                i = j;
            } else if c.is_ascii_alphabetic() || c == b'_' {
                let mut j = i;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                let word = &line[i..j];
                let color = if KEYWORDS.contains(&word) {
                    Some(pal.keyword)
                } else if bytes.get(j) == Some(&b'(') {
                    Some(pal.name)
                } else {
                    None
                };
                if let Some(color) = color {
                    flush(&mut job, plain_from, i);
                    job.append(word, 0.0, fmt(color));
                    plain_from = j;
                }
                i = j;
            } else {
                i += 1;
                while i < bytes.len() && !bytes[i].is_ascii() && (bytes[i] & 0xC0) == 0x80 {
                    i += 1;
                }
            }
        }
        flush(&mut job, plain_from, line.len());
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colors(text: &str) -> Vec<(String, Color32)> {
        highlight(text, FontId::monospace(12.0), true)
            .sections
            .iter()
            .map(|s| {
                (
                    text[s.byte_range.start.0..s.byte_range.end.0].to_string(),
                    s.format.color,
                )
            })
            .collect()
    }

    #[test]
    fn covers_the_whole_text_in_order() {
        let text = "procedure start begin\n  if (x == 12) then call foo(\"a\\\"b\"); // fin\nend\n";
        let joined: String = colors(text).into_iter().map(|(s, _)| s).collect();
        assert_eq!(joined, text);
    }

    #[test]
    fn keywords_numbers_strings_and_comments_get_distinct_colors() {
        let c = colors("if x == 12 then \"s\" // c");
        let color_of = |w: &str| c.iter().find(|(s, _)| s == w).map(|(_, col)| *col);
        let plain = color_of(" x == ").unwrap_or_default();
        assert_ne!(color_of("if").unwrap(), plain);
        assert_ne!(color_of("12").unwrap(), plain);
        assert_ne!(color_of("\"s\"").unwrap(), plain);
        assert_ne!(color_of("// c").unwrap(), plain);
    }

    #[test]
    fn handles_non_ascii_text() {
        let text = "// é à ü\nvariable s = \"Bâtiment\";\n";
        let joined: String = colors(text).into_iter().map(|(s, _)| s).collect();
        assert_eq!(joined, text);
    }
}
