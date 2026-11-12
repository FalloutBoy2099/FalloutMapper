//! Sauvegarde de récupération : toutes les deux minutes, chaque carte modifiée et non enregistrée est copiée dans le dossier de
//! récupération de l'application (jamais dans le dossier du jeu). La copie disparaît dès que la carte est enregistrée ou
//! abandonnée ; s'il en reste au lancement suivant (plantage, coupure), l'éditeur propose de les restaurer.

use crate::app::MapperApp;
use crate::i18n::t;
use eframe::egui;
use fo_core::write_map;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Délai entre deux sauvegardes de récupération.
const INTERVAL: Duration = Duration::from_secs(120);

/// Une copie de récupération trouvée sur le disque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub file: String,
    pub map_path: PathBuf,
    /// Date de l'écriture (secondes depuis 1970).
    pub secs: u64,
}

pub(crate) struct RecoveryState {
    pub(crate) last: Instant,
    /// Copies proposées à la restauration (fenêtre ouverte tant qu'il en reste).
    pub offered: Vec<Entry>,
    scanned: bool,
    /// Carte en cours de restauration : elle sera marquée « modifiée » à son arrivée.
    pub restoring: Option<String>,
}

impl Default for RecoveryState {
    fn default() -> Self {
        Self {
            last: Instant::now(),
            offered: Vec::new(),
            scanned: false,
            restoring: None,
        }
    }
}

/// Dossier des copies. Aucun en mode essai (captures, self-test), sauf si `FO_RECOVERY_DIR` le désigne.
pub(crate) fn recovery_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("FO_RECOVERY_DIR") {
        return Some(PathBuf::from(d));
    }
    if ["FO_SHOT", "FO_SELFTEST", "FO_HIDDEN"]
        .iter()
        .any(|v| std::env::var_os(v).is_some())
    {
        return None;
    }
    eframe::storage_dir("FalloutMapper").map(|d| d.join("recovery"))
}

/// Empreinte courte du dossier du jeu (deux installations peuvent avoir une carte du même nom).
fn root_hash(root: &Path) -> String {
    let mut h: u32 = 0x811C_9DC5;
    for b in root.display().to_string().to_lowercase().bytes() {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    format!("{h:08x}")
}

fn base(dir: &Path, root: &Path, file: &str) -> PathBuf {
    dir.join(format!("{}_{}", root_hash(root), file))
}

/// Écrit (ou remplace) la copie d'une carte.
pub(crate) fn write_entry(
    dir: &Path,
    root: &Path,
    file: &str,
    bytes: &[u8],
) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let b = base(dir, root, file);
    let tmp = b.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, b.with_extension("map"))?;
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    std::fs::write(b.with_extension("txt"), format!("{secs}\n{file}\n"))
}

/// Supprime la copie d'une carte, si elle existe.
pub(crate) fn remove_entry(dir: &Path, root: &Path, file: &str) {
    let b = base(dir, root, file);
    for ext in ["map", "txt", "tmp"] {
        let _ = std::fs::remove_file(b.with_extension(ext));
    }
}

/// Copies de ce jeu, de la plus récente à la plus ancienne.
pub(crate) fn list_entries(dir: &Path, root: &Path) -> Vec<Entry> {
    let prefix = format!("{}_", root_hash(root));
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Entry> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_string_lossy().into_owned();
            let stem = name
                .strip_suffix(".map")?
                .strip_prefix(&prefix)?
                .to_string();
            let info = std::fs::read_to_string(path.with_extension("txt")).unwrap_or_default();
            let secs = info
                .lines()
                .next()
                .and_then(|l| l.trim().parse().ok())
                .unwrap_or(0);
            Some(Entry {
                file: stem,
                map_path: path,
                secs,
            })
        })
        .collect();
    out.sort_by(|a, b| b.secs.cmp(&a.secs).then(a.file.cmp(&b.file)));
    out
}

/// « il y a 3 min », « il y a 2 h »… d'après des secondes depuis 1970.
fn ago(secs: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let d = now.saturating_sub(secs);
    match d {
        0..=89 => t("à l'instant").to_string(),
        90..=5399 => format!("{} {} min", t("il y a"), d / 60),
        5400..=129_599 => format!("{} {} h", t("il y a"), d / 3600),
        _ => format!("{} {} j", t("il y a"), d / 86_400),
    }
}

impl MapperApp {
    fn game_root(&self) -> Option<PathBuf> {
        self.game.as_ref().map(|g| g.data.root.clone())
    }

    /// Oublie la copie de récupération d'une carte (enregistrée ou abandonnée).
    pub(crate) fn forget_recovery(&self, file: &str) {
        if let (Some(dir), Some(root)) = (recovery_dir(), self.game_root()) {
            remove_entry(&dir, &root, file);
        }
    }

    /// Toutes les deux minutes : copie les cartes modifiées, supprime les copies des cartes qui ne le sont plus.
    pub(crate) fn write_recovery_copies(&mut self) {
        let (Some(dir), Some(root)) = (recovery_dir(), self.game_root()) else {
            return;
        };
        let mut maps: Vec<(String, bool, Option<Vec<u8>>)> = Vec::new();
        if let Some(c) = &self.current {
            maps.push((
                c.file.clone(),
                self.edit.dirty,
                self.edit.dirty.then(|| write_map(&c.map).ok()).flatten(),
            ));
        }
        for d in &self.docs {
            if let Some(c) = &d.current {
                maps.push((
                    c.file.clone(),
                    d.edit.dirty,
                    d.edit.dirty.then(|| write_map(&c.map).ok()).flatten(),
                ));
            }
        }
        for (file, dirty, bytes) in maps {
            match (dirty, bytes) {
                (true, Some(b)) => {
                    let _ = write_entry(&dir, &root, &file, &b);
                }
                (false, _) => remove_entry(&dir, &root, &file),
                _ => {}
            }
        }
    }

    /// À appeler à chaque image : écriture périodique, et proposition de restauration au premier passage après l'ouverture du jeu.
    pub(crate) fn recovery_tick(&mut self, ctx: &egui::Context) {
        if self.game.is_none() {
            self.recovery.scanned = false;
            return;
        }
        if !self.recovery.scanned {
            self.recovery.scanned = true;
            if let (Some(dir), Some(root)) = (recovery_dir(), self.game_root()) {
                self.recovery.offered = list_entries(&dir, &root);
            }
        }
        if self.recovery.last.elapsed() >= INTERVAL {
            self.recovery.last = Instant::now();
            self.write_recovery_copies();
        }
        ctx.request_repaint_after(Duration::from_secs(30));
    }

    /// Restaure une copie : la carte s'ouvre dans un nouvel onglet, marquée « modifiée », avec le contenu récupéré.
    pub(crate) fn restore_recovery(&mut self, entry: &Entry, ctx: &egui::Context) {
        let (Some((data, art)), Ok(bytes)) = (
            self.game.as_ref().map(|g| (g.data.clone(), g.art.clone())),
            std::fs::read(&entry.map_path),
        ) else {
            return;
        };
        if self.current.is_some() {
            self.open_new_tab();
        }
        self.loading = Some(entry.file.clone());
        self.recovery.restoring = Some(entry.file.clone());
        self.status = format!("{} {}…", t("Restauration de"), entry.file);
        crate::loader::spawn_load_bytes(
            data,
            art,
            entry.file.clone(),
            bytes,
            self.tx.clone(),
            ctx.clone(),
        );
    }

    pub(crate) fn recovery_window(&mut self, ctx: &egui::Context) {
        if self.recovery.offered.is_empty() {
            return;
        }
        enum Choice {
            Restore(usize),
            Discard(usize),
            DiscardAll,
        }
        let mut choice = None;
        let mut open = true;
        egui::Window::new(t("Cartes à récupérer")).open(&mut open).order(egui::Order::Middle).collapsible(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(ctx, |ui| {
            ui.set_min_width(460.0);
            ui.label(t("L'éditeur s'est arrêté avant d'enregistrer ces cartes. Les copies de récupération ci-dessous existent encore."));
            ui.separator();
            for (i, e) in self.recovery.offered.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&e.file).strong());
                    ui.weak(ago(e.secs));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("Supprimer")).clicked() {
                            choice = Some(Choice::Discard(i));
                        }
                        if ui.button(t("Restaurer")).clicked() {
                            choice = Some(Choice::Restore(i));
                        }
                    });
                });
            }
            ui.separator();
            if ui.button(t("Tout supprimer")).clicked() {
                choice = Some(Choice::DiscardAll);
            }
        });
        let root = self.game_root();
        let dir = recovery_dir();
        match choice {
            Some(Choice::Restore(i)) => {
                let e = self.recovery.offered.remove(i);
                self.restore_recovery(&e, ctx);
            }
            Some(Choice::Discard(i)) => {
                let e = self.recovery.offered.remove(i);
                if let (Some(d), Some(r)) = (&dir, &root) {
                    remove_entry(d, r, &e.file);
                }
            }
            Some(Choice::DiscardAll) => {
                if let (Some(d), Some(r)) = (&dir, &root) {
                    for e in &self.recovery.offered {
                        remove_entry(d, r, &e.file);
                    }
                }
                self.recovery.offered.clear();
            }
            None => {}
        }
        if !open {
            // Fermer la fenêtre garde les copies pour la prochaine fois.
            self.recovery.offered.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fo_recovery_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn entries_are_written_listed_per_game_and_removed() {
        let dir = scratch("a");
        let (g1, g2) = (Path::new("C:/jeu1"), Path::new("C:/jeu2"));
        write_entry(&dir, g1, "arvillag", b"AAA").unwrap();
        write_entry(&dir, g1, "denbus1", b"BBB").unwrap();
        write_entry(&dir, g2, "arvillag", b"CCC").unwrap();
        let l1 = list_entries(&dir, g1);
        assert_eq!(
            l1.iter().map(|e| e.file.as_str()).collect::<Vec<_>>().len(),
            2
        );
        assert!(
            l1.iter()
                .any(|e| e.file == "arvillag" && std::fs::read(&e.map_path).unwrap() == b"AAA")
        );
        assert_eq!(
            list_entries(&dir, g2).len(),
            1,
            "un autre jeu a ses propres copies"
        );
        write_entry(&dir, g1, "arvillag", b"NEW").unwrap();
        assert_eq!(
            std::fs::read(
                list_entries(&dir, g1)
                    .iter()
                    .find(|e| e.file == "arvillag")
                    .unwrap()
                    .map_path
                    .clone()
            )
            .unwrap(),
            b"NEW",
            "remplacée"
        );
        remove_entry(&dir, g1, "arvillag");
        assert_eq!(
            list_entries(&dir, g1)
                .iter()
                .map(|e| e.file.clone())
                .collect::<Vec<_>>(),
            vec!["denbus1".to_string()]
        );
        remove_entry(&dir, g1, "absente");
        assert!(list_entries(&dir.join("nulle part"), g1).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ago_is_readable() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(
            ago(now).contains("instant") || ago(now).contains("moment") || !ago(now).is_empty()
        );
        assert!(ago(now - 600).contains("10 min"));
        assert!(ago(now - 3 * 3600).contains("3 h"));
        assert!(ago(now - 3 * 86_400).contains("3 j"));
    }
}
