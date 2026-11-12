//! Lancer une carte avec CE en priorité, en adaptant la configuration au moteur installé.

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};

/// Contenu de `ddraw.ini` avec `StartingMap=<carte>` dans `[Misc]` (section et clé créées si besoin). Le reste du
/// fichier, ses commentaires et ses fins de ligne sont conservés ; une ligne `;StartingMap=` commentée est remplacée.
pub fn apply_starting_map(ini: &str, map: &str) -> String {
    let eol = if ini.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = ini
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .collect();
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    let new_line = format!("StartingMap={map}");
    let is_section = |l: &str| l.trim_start().starts_with('[');
    let misc_header = lines
        .iter()
        .position(|l| l.trim().eq_ignore_ascii_case("[misc]"));
    match misc_header {
        Some(h) => {
            let end = lines
                .iter()
                .enumerate()
                .skip(h + 1)
                .find(|(_, l)| is_section(l))
                .map_or(lines.len(), |(i, _)| i);
            let key_of = |l: &str| {
                l.trim_start()
                    .trim_start_matches(';')
                    .trim_start()
                    .split('=')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase()
            };
            // Une ligne active existante, sinon la ligne commentée du modèle, sinon on ajoute.
            let active = (h + 1..end).find(|&i| {
                !lines[i].trim_start().starts_with(';') && key_of(&lines[i]) == "startingmap"
            });
            let commented = (h + 1..end).find(|&i| {
                lines[i].trim_start().starts_with(';')
                    && lines[i].contains('=')
                    && key_of(&lines[i]) == "startingmap"
            });
            match active.or(commented) {
                Some(i) => lines[i] = new_line,
                None => {
                    // Après la dernière ligne non vide de la section.
                    let at = (h + 1..end)
                        .rev()
                        .find(|&i| !lines[i].trim().is_empty())
                        .map_or(h + 1, |i| i + 1);
                    lines.insert(at, new_line);
                }
            }
        }
        None => {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push("[Misc]".to_string());
            lines.push(new_line);
        }
    }
    let mut out = lines.join(eol);
    out.push_str(eol);
    out
}

/// L'exécutable du jeu dans un dossier d'installation : Community Edition en priorité.
pub fn find_game_executable(root: &Path) -> Option<PathBuf> {
    [
        "fallout2-ce.exe",
        "fallout2ce.exe",
        "fallout2-ce",
        "fallout2.exe",
        "fallout2HR.exe",
        "fallout2.x86_64",
        "fallout2",
    ]
    .iter()
    .map(|n| root.join(n))
    .find(|p| p.is_file())
}

fn uses_ce_content_config(exe: &Path) -> Result<bool> {
    let name = exe
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    if !matches!(
        name.as_str(),
        "fallout2-ce.exe" | "fallout2ce.exe" | "fallout2-ce"
    ) {
        return Ok(false);
    }
    let bytes = std::fs::read(exe).map_err(|e| Error::io(exe, e))?;
    let marker = b"config\\game#patch.cfg";
    Ok(bytes.windows(marker.len()).any(|s| s == marker))
}

fn ce_config_path(root: &Path) -> PathBuf {
    let config = std::fs::read(root.join("fallout2.cfg"))
        .map(|b| crate::game::GameConfig::parse(&String::from_utf8_lossy(&b)))
        .unwrap_or_default();
    root.join(config.master_patches)
        .join("config")
        .join("game#patch.cfg")
}

fn apply_ce_starting_map(text: &str, stem: &str) -> String {
    let mut doc = crate::ini_doc::IniDoc::parse(text);
    if text.is_empty() {
        doc.preamble.clear();
    }
    if doc.section("start").is_none() {
        doc.sections.push(crate::ini_doc::IniSection {
            header: "[start]".into(),
            name: "start".into(),
            lines: Vec::new(),
        });
    }
    doc.section_mut("start")
        .unwrap()
        .set("map", &format!("{stem}.map"));
    doc.final_eol = true;
    doc.to_text()
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn write_launch_config(path: &Path, text: &str) -> Result<()> {
    let backup = sidecar(path, ".fomapper.bak");
    let absent = sidecar(path, ".fomapper.absent");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    if !backup.exists() && !absent.exists() {
        if path.exists() {
            std::fs::copy(path, &backup).map_err(|e| Error::io(&backup, e))?;
        } else {
            std::fs::write(&absent, []).map_err(|e| Error::io(&absent, e))?;
        }
    }
    std::fs::write(path, text).map_err(|e| Error::io(path, e))
}

/// Prépare le lancement du jeu sur `map` (nom sans extension) : adapte sa configuration (copie de secours incluse) puis démarre le
/// jeu dans son dossier. Renvoie un message à montrer. Ne bloque pas : le jeu est détaché.
pub fn launch_on_map(root: &Path, map: &str) -> Result<String> {
    let exe = find_game_executable(root).ok_or_else(|| {
        Error::format(format!(
            "aucun exécutable de Fallout 2 dans {}",
            root.display()
        ))
    })?;
    let map = map.to_ascii_lowercase();
    let stem = map.strip_suffix(".map").unwrap_or(&map);
    if stem.is_empty()
        || !stem
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(Error::format(format!("nom de carte invalide : {map}")));
    }
    // Un jeu déjà lancé avec le canal de contrôle (`control_port`) charge la carte sans redémarrer.
    if let Some((port, token)) = crate::game_link::link_settings(root)
        && let Ok(mut link) = crate::game_link::GameLink::connect(port, token.as_deref())
    {
        let state = link.load_map(stem, None, true)?;
        return Ok(format!("Carte chargée dans le jeu en cours ({state})"));
    }
    let native_ce = uses_ce_content_config(&exe)?;
    let ini_path = if native_ce {
        ce_config_path(root)
    } else {
        root.join("ddraw.ini")
    };
    let previous = match std::fs::read(&ini_path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(Error::io(&ini_path, e)),
    };
    let current = previous
        .as_deref()
        .map(String::from_utf8_lossy)
        .unwrap_or_default();
    let text = if native_ce {
        apply_ce_starting_map(&current, stem)
    } else {
        apply_starting_map(&current, stem)
    };
    write_launch_config(&ini_path, &text)?;
    if let Err(error) = std::process::Command::new(&exe).current_dir(root).spawn() {
        if let Some(bytes) = previous {
            std::fs::write(&ini_path, bytes).map_err(|e| Error::io(&ini_path, e))?;
        } else {
            std::fs::remove_file(&ini_path).map_err(|e| Error::io(&ini_path, e))?;
        }
        return Err(Error::io(&exe, error));
    }
    Ok(format!(
        "Jeu lancé ({}) — choisissez Nouvelle partie pour charger {stem}",
        exe.file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned())
    ))
}

/// Restaure les configurations de lancement sauvegardées, y compris celle de CE.
pub fn restore_launch_config(root: &Path) -> Result<bool> {
    let mut restored = false;
    for path in [root.join("ddraw.ini"), ce_config_path(root)] {
        let backup = sidecar(&path, ".fomapper.bak");
        let absent = sidecar(&path, ".fomapper.absent");
        if backup.exists() {
            std::fs::copy(&backup, &path).map_err(|e| Error::io(&path, e))?;
            restored = true;
        } else if absent.exists() {
            if path.exists() {
                std::fs::remove_file(&path).map_err(|e| Error::io(&path, e))?;
            }
            restored = true;
        }
    }
    Ok(restored)
}

/// Remet `ddraw.ini` tel qu'avant le premier lancement. Renvoie `false` s'il n'y a pas de copie de secours.
pub fn restore_ddraw_ini(root: &Path) -> Result<bool> {
    let backup = root.join("ddraw.ini.fomapper.bak");
    if !backup.exists() {
        return Ok(false);
    }
    let ini = root.join("ddraw.ini");
    std::fs::copy(&backup, &ini).map_err(|e| Error::io(&ini, e))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_the_commented_template_line() {
        let ini = "[Main]\r\nA=1\r\n\r\n[Misc]\r\n;Les cartes\r\n;StartingMap=\r\nB=2\r\n\r\n[Other]\r\nC=3\r\n";
        let out = apply_starting_map(ini, "arvillag");
        assert!(out.contains("\r\nStartingMap=arvillag\r\n"));
        assert!(!out.contains(";StartingMap"));
        assert_eq!(out.matches("StartingMap").count(), 1);
        // Le reste est intact.
        assert!(
            out.starts_with("[Main]\r\nA=1\r\n")
                && out.contains("B=2")
                && out.ends_with("[Other]\r\nC=3\r\n")
        );
    }

    #[test]
    fn replaces_an_active_line_and_creates_missing_parts() {
        assert!(
            apply_starting_map("[Misc]\nStartingMap=old\n", "new").contains("StartingMap=new")
                && !apply_starting_map("[Misc]\nStartingMap=old\n", "new").contains("old")
        );
        let appended = apply_starting_map("[Misc]\nX=1\n\n[Z]\nY=2\n", "m");
        let misc = appended.find("StartingMap=m").unwrap();
        assert!(misc > appended.find("X=1").unwrap() && misc < appended.find("[Z]").unwrap());
        let created = apply_starting_map("[Main]\nA=1\n", "m");
        assert!(created.ends_with("[Misc]\nStartingMap=m\n"));
        assert_eq!(apply_starting_map("", "m"), "[Misc]\nStartingMap=m\n");
    }

    #[test]
    fn rejects_bad_names_before_touching_anything() {
        let dir = std::env::temp_dir().join(format!("fomapper_launch_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(launch_on_map(&dir, "a;b").is_err());
        assert!(
            launch_on_map(&dir, "ok").is_err(),
            "pas d'exécutable dans ce dossier"
        );
        assert!(!dir.join("ddraw.ini").exists());
        assert!(!restore_ddraw_ini(&dir).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn community_edition_is_preferred_when_original_game_is_also_installed() {
        let dir = std::env::temp_dir().join(format!("fomapper_ce_choice_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["fallout2.exe", "fallout2ce.exe", "fallout2-ce.exe"] {
            std::fs::write(dir.join(name), []).unwrap();
        }
        assert_eq!(
            find_game_executable(&dir),
            Some(dir.join("fallout2-ce.exe"))
        );
        std::fs::remove_file(dir.join("fallout2-ce.exe")).unwrap();
        assert_eq!(find_game_executable(&dir), Some(dir.join("fallout2ce.exe")));
        std::fs::remove_file(dir.join("fallout2ce.exe")).unwrap();
        assert_eq!(find_game_executable(&dir), Some(dir.join("fallout2.exe")));
        std::fs::remove_file(dir.join("fallout2.exe")).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn ce_starting_map_keeps_other_settings_and_comments() {
        let original = ";CE\r\n[characters]\r\npremade_paths=combat\r\n[start]\r\nmap=artemple.map ; départ\r\nworldmap_x=100\r\n";
        let changed = apply_ce_starting_map(original, "test12");
        assert!(changed.starts_with(";CE\r\n[characters]\r\npremade_paths=combat\r\n"));
        assert!(changed.contains("map=test12.map  ; départ\r\n"));
        assert!(changed.contains("worldmap_x=100\r\n"));
        assert_eq!(
            apply_ce_starting_map("", "test12"),
            "[start]\nmap=test12.map\n"
        );
    }

    #[test]
    fn ce_config_is_detected_and_backed_up_independently_of_ddraw() {
        let dir = std::env::temp_dir().join(format!("fomapper_ce_config_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("fallout2-ce.exe");
        std::fs::write(&exe, b"MZ\0config\\game#patch.cfg\0").unwrap();
        assert!(uses_ce_content_config(&exe).unwrap());
        std::fs::write(
            dir.join("fallout2.cfg"),
            "[system]\nmaster_patches=custom\n",
        )
        .unwrap();
        let path = ce_config_path(&dir);
        assert_eq!(path, dir.join("custom/config/game#patch.cfg"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let original = "[characters]\npremade_paths=combat\n";
        std::fs::write(&path, original).unwrap();
        std::fs::write(dir.join("ddraw.ini"), "original sfall settings").unwrap();
        write_launch_config(&path, &apply_ce_starting_map(original, "first")).unwrap();
        write_launch_config(&path, &apply_ce_starting_map(original, "second")).unwrap();
        let before_failed_launch = std::fs::read(&path).unwrap();
        assert!(
            launch_on_map(&dir, "spawnfailure").is_err(),
            "l'exécutable factice ne doit pas démarrer"
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before_failed_launch,
            "un échec restaure la configuration courante"
        );
        assert!(launch_on_map(&dir, "bad\nmap").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before_failed_launch);
        assert_eq!(
            std::fs::read_to_string(sidecar(&path, ".fomapper.bak")).unwrap(),
            original
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("ddraw.ini")).unwrap(),
            "original sfall settings"
        );
        assert!(restore_launch_config(&dir).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        for p in [
            path.clone(),
            sidecar(&path, ".fomapper.bak"),
            exe,
            dir.join("ddraw.ini"),
            dir.join("fallout2.cfg"),
        ] {
            std::fs::remove_file(p).unwrap();
        }
        std::fs::remove_dir(path.parent().unwrap()).unwrap();
        std::fs::remove_dir(dir.join("custom")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn restoring_a_previously_absent_config_removes_the_generated_file() {
        let dir = std::env::temp_dir().join(format!("fomapper_ce_absent_{}", std::process::id()));
        let path = ce_config_path(&dir);
        write_launch_config(&path, &apply_ce_starting_map("", "test12")).unwrap();
        assert!(path.is_file());
        assert!(restore_launch_config(&dir).unwrap());
        assert!(!path.exists());
        std::fs::remove_file(sidecar(&path, ".fomapper.absent")).unwrap();
        std::fs::remove_dir(path.parent().unwrap()).unwrap();
        std::fs::remove_dir(dir.join("data")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
