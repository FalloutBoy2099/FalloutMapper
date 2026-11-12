//! `data/maps.txt` : la liste ordonnée des cartes du jeu (`[Map 000]`…). Les numéros se suivent sans trou ; chaque entrée
//! donne le nom affiché (`lookup_name`) et le fichier (`map_name`).

use crate::ini_doc::{IniDoc, IniSection};
use std::collections::HashMap;

/// Nom de section d'une carte : `Map 004`.
pub fn section_name(number: u32) -> String {
    format!("Map {number:03}")
}

/// Numéro d'une section `Map NNN`.
pub fn section_number(s: &IniSection) -> Option<u32> {
    s.name
        .strip_prefix("Map ")
        .or_else(|| s.name.strip_prefix("map "))
        .and_then(|n| n.trim().parse().ok())
}

/// Nouvelle entrée en fin de liste, avec les réglages habituels d'une carte ordinaire. Renvoie son numéro.
pub fn add_entry(doc: &mut IniDoc, map_name: &str, lookup_name: &str) -> u32 {
    let number = doc
        .sections
        .iter()
        .filter_map(section_number)
        .max()
        .map_or(0, |n| n + 1);
    let eol = doc.eol.clone();
    // Une ligne vide sépare les entrées, comme dans le fichier d'origine.
    if let Some(last) = doc.sections.last_mut()
        && last.lines.last().is_none_or(|l| !l.trim().is_empty())
    {
        last.lines.push(String::new());
    }
    let _ = eol;
    doc.sections.push(IniSection {
        header: format!("[{}]", section_name(number)),
        name: section_name(number),
        lines: vec![
            format!("lookup_name={lookup_name}"),
            format!("map_name={}", map_name.to_ascii_lowercase()),
            "music=".to_string(),
            "ambient_sfx=".to_string(),
            "saved=Yes".to_string(),
            "dead_bodies_age=Yes".to_string(),
            "can_rest_here=Yes,Yes,Yes".to_string(),
            "random_start_point_0=elev:0, tile_num:19896".to_string(),
        ],
    });
    number
}

/// Retire la dernière entrée (les numéros ne doivent pas avoir de trou). Renvoie son numéro.
pub fn remove_last(doc: &mut IniDoc) -> Option<u32> {
    let idx = doc
        .sections
        .iter()
        .rposition(|s| section_number(s).is_some())?;
    let number = section_number(&doc.sections[idx]);
    doc.sections.remove(idx);
    number
}

/// Problèmes qui empêcheraient le jeu de lire la liste : numéros qui ne se suivent pas, noms manquants ou en double.
pub fn validate(doc: &IniDoc) -> Vec<String> {
    let mut problems = Vec::new();
    let mut seen: HashMap<String, u32> = HashMap::new();
    let mut expected = 0u32;
    for s in &doc.sections {
        let Some(n) = section_number(s) else { continue };
        if n != expected {
            problems.push(format!(
                "[{}] : numéro attendu {expected} (la liste ne doit avoir aucun trou)",
                s.name
            ));
            expected = n;
        }
        expected += 1;
        match s.get("map_name").filter(|v| !v.is_empty()) {
            None => problems.push(format!("[{}] : map_name manquant", s.name)),
            Some(name) => {
                if let Some(first) = seen.insert(name.to_ascii_lowercase(), n) {
                    problems.push(format!(
                        "[{}] : map_name « {name} » déjà utilisé par la carte {first}",
                        s.name
                    ));
                }
            }
        }
        if s.get("lookup_name").is_none_or(|v| v.is_empty()) {
            problems.push(format!("[{}] : lookup_name manquant", s.name));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str =
        "; liste\n[Map 000]\nlookup_name=A\nmap_name=a\n\n[Map 001]\nlookup_name=B\nmap_name=b\n";

    #[test]
    fn add_numbers_after_the_last_and_keeps_the_rest() {
        let mut d = IniDoc::parse(SAMPLE);
        assert!(validate(&d).is_empty());
        let n = add_entry(&mut d, "NewOne", "Ma carte");
        assert_eq!(n, 2);
        let t = d.to_text();
        assert!(t.starts_with(SAMPLE.trim_end()), "l'existant est intact");
        assert!(t.contains("[Map 002]") && t.contains("map_name=newone"));
        assert!(validate(&d).is_empty());
        assert_eq!(remove_last(&mut d), Some(2));
        assert!(!d.to_text().contains("Map 002"));
    }

    #[test]
    fn validation_reports_gaps_duplicates_and_missing_names() {
        let d = IniDoc::parse(
            "[Map 000]\nlookup_name=A\nmap_name=a\n[Map 002]\nlookup_name=B\nmap_name=A\n[Map 003]\nmap_name=\n",
        );
        let p = validate(&d);
        assert!(p.iter().any(|m| m.contains("numéro attendu 1")), "{p:?}");
        assert!(p.iter().any(|m| m.contains("déjà utilisé")), "{p:?}");
        assert!(p.iter().any(|m| m.contains("map_name manquant")), "{p:?}");
        assert!(
            p.iter().any(|m| m.contains("lookup_name manquant")),
            "{p:?}"
        );
    }
}
