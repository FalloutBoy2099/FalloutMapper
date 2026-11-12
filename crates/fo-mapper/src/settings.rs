//! Réglages de l'utilisateur (préférences, dossiers récents, calques, barre d'outils, fenêtre…) : sous Windows, des valeurs texte de la
//! base de registre, dans `HKEY_CURRENT_USER\SOFTWARE\FALLOUT\FalloutMapper` (une valeur par réglage). Ailleurs, un fichier texte
//! `settings.txt` du dossier de stockage de l'application.
//!
//! La variable d'environnement `FO_SETTINGS_KEY` remplace le chemin de la clé (essais) ; sans elle, c'est `SOFTWARE\FALLOUT\FalloutMapper`.

/// Chemin de la clé sous `HKEY_CURRENT_USER`.
pub(crate) const DEFAULT_KEY: &str = r"SOFTWARE\FALLOUT\FalloutMapper";

fn key_path() -> String {
    std::env::var("FO_SETTINGS_KEY").ok().filter(|k| !k.trim().is_empty()).unwrap_or_else(|| DEFAULT_KEY.to_string())
}

/// Valeur d'un réglage, s'il existe.
pub(crate) fn get(name: &str) -> Option<String> {
    imp::get(&key_path(), name)
}

/// Enregistre un réglage (la clé est créée au besoin) ; les échecs sont ignorés : un réglage perdu ne doit pas gêner l'utilisateur.
pub(crate) fn set_string(name: &str, value: String) {
    imp::set(&key_path(), name, &value);
}

#[cfg(windows)]
mod imp {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

    pub(super) fn get(path: &str, name: &str) -> Option<String> {
        RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(path, KEY_READ).ok()?.get_value::<String, _>(name).ok()
    }

    pub(super) fn set(path: &str, name: &str, value: &str) {
        if let Ok((key, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(path) {
            let _ = key.set_value(name, &value);
        }
    }

    #[cfg(test)]
    pub(super) fn delete_tree(path: &str) {
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(path);
    }
}

#[cfg(not(windows))]
mod imp {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn file(path: &str) -> Option<PathBuf> {
        let leaf = path.rsplit('\\').next().unwrap_or("FalloutMapper");
        eframe::storage_dir(leaf).map(|d| d.join("settings.txt"))
    }

    fn load(path: &str) -> BTreeMap<String, String> {
        let text = file(path).and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default();
        text.lines().filter_map(|l| l.split_once('\t')).map(|(k, v)| (k.to_string(), v.replace("\\n", "\n").replace("\\\\", "\\"))).collect()
    }

    pub(super) fn get(path: &str, name: &str) -> Option<String> {
        load(path).get(name).cloned()
    }

    pub(super) fn set(path: &str, name: &str, value: &str) {
        let mut all = load(path);
        all.insert(name.to_string(), value.to_string());
        let Some(f) = file(path) else { return };
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text: String = all.iter().map(|(k, v)| format!("{k}\t{}\n", v.replace('\\', "\\\\").replace('\n', "\\n"))).collect();
        let _ = std::fs::write(f, text);
    }

    #[cfg(test)]
    pub(super) fn delete_tree(path: &str) {
        if let Some(f) = file(path) {
            let _ = std::fs::remove_file(f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_round_trip_in_a_scratch_key() {
        // Clé d'essai, supprimée à la fin : les vrais réglages ne sont jamais touchés.
        let path = r"SOFTWARE\FALLOUT\FalloutMapperTest";
        imp::delete_tree(path);
        assert_eq!(imp::get(path, "a"), None);
        imp::set(path, "a", "1");
        imp::set(path, "liste", "x\ny\\z");
        imp::set(path, "a", "2");
        assert_eq!(imp::get(path, "a").as_deref(), Some("2"));
        assert_eq!(imp::get(path, "liste").as_deref(), Some("x\ny\\z"));
        imp::delete_tree(path);
        assert_eq!(imp::get(path, "a"), None);
    }
}
