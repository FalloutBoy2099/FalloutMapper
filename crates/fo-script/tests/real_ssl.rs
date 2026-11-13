//! Compare l'analyse des sources SSL du Restoration Project (si présentes) aux procédures des scripts compilés du jeu.

use fo_core::GameData;
use fo_script::Program;
use fo_script::ssl::{DirProvider, analyze};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("ssl")) {
            out.push(p);
        }
    }
}

#[test]
#[ignore = "lent (environ une minute) : cargo test -p fo-script --test real_ssl -- --ignored"]
fn procedure_names_match_the_compiled_scripts() {
    let root =
        PathBuf::from(std::env::var("FO_SSL_SOURCES").unwrap_or_else(|_| {
            r"C:\Fallout2\setup\Fallout2_Restoration_Project\scripts_src".into()
        }));
    let game_dir =
        std::env::var("FO2_GAME_DIR").unwrap_or_else(|_| r"C:\Fallout2\F2_RPU_2.4.34_old".into());
    let (Ok(game), true) = (GameData::open(game_dir), root.is_dir()) else {
        return;
    };
    let mut files = Vec::new();
    collect(&root, &mut files);
    let (mut compared, mut same, mut missing_headers) = (0, 0, 0);
    let mut bad = Vec::new();
    for f in &files {
        let stem = f.file_stem().unwrap().to_string_lossy().to_lowercase();
        let Ok(Some(data)) = game.vfs().try_read(&format!("scripts/{stem}.int")) else {
            continue;
        };
        let Ok(program) = Program::parse(&data) else {
            continue;
        };
        let bytes = std::fs::read(f).unwrap();
        let text: String = bytes.iter().map(|&b| b as char).collect();
        let dirs = vec![
            f.parent().unwrap().to_path_buf(),
            root.join("headers"),
            root.clone(),
        ];
        let facts = analyze(&text, &DirProvider { dirs });
        if !facts.missing_includes.is_empty() {
            missing_headers += 1;
        }
        let from_source: BTreeSet<String> = facts
            .procedures
            .iter()
            .map(|p| p.name.to_lowercase())
            .collect();
        let from_int: BTreeSet<String> = program
            .procedures
            .iter()
            .filter(|p| p.is_implemented() && !p.name.starts_with('.'))
            .map(|p| p.name.to_lowercase())
            .collect();
        compared += 1;
        // Tout ce que le compilateur a gardé doit se retrouver dans la source (et ses en-têtes) ; l'inverse n'est pas exigé : un script peut
        // contenir des procédures que le compilateur n'a pas conservées.
        if from_int.is_subset(&from_source) {
            same += 1;
        } else if bad.len() < 12 {
            bad.push(format!(
                "{stem}: source seule {:?} ; INT seul {:?}",
                from_source
                    .difference(&from_int)
                    .take(4)
                    .collect::<Vec<_>>(),
                from_int
                    .difference(&from_source)
                    .take(4)
                    .collect::<Vec<_>>()
            ));
        }
    }
    eprintln!(
        "SSL comparés {compared} / {} ; dont toutes les procédures du INT retrouvées {same} ; en-têtes manquants {missing_headers}",
        files.len()
    );
    for b in &bad {
        eprintln!("DIFF {b}");
    }
    assert!(
        compared == 0 || same * 100 >= compared * 70,
        "{same} complets sur {compared}"
    );
}
