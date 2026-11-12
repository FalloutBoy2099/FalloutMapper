use fo_core::GameData;
use fo_script::Program;

fn game() -> Option<GameData> {
    let p =
        std::env::var("FO2_GAME_DIR").unwrap_or_else(|_| r"C:\Fallout2\F2_RPU_2.4.34_old".into());
    GameData::open(p).ok()
}

#[test]
fn every_script_parses() {
    let Some(game) = game() else { return };
    let list = game.list("scripts/scripts.lst");
    assert!(list.len() > 1000);
    let mut failures = Vec::new();
    let mut checked = 0;
    for name in list.iter() {
        let Ok(Some(data)) = game.vfs().try_read(&format!("scripts/{name}")) else {
            continue;
        };
        checked += 1;
        match Program::parse(&data) {
            Ok(p) => {
                let _ = p.pseudo_source();
            }
            Err(e) => failures.push(format!("{name} : {e}")),
        }
    }
    assert!(checked > 1000, "{checked} scripts lus");
    assert!(
        failures.len() * 100 < checked,
        "{} échecs sur {checked} :\n{}",
        failures.len(),
        failures
            .iter()
            .take(15)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn village_script_has_standard_procedures() {
    let Some(game) = game() else { return };
    let data = game.read("scripts/abel.int").unwrap();
    let p = Program::parse(&data).unwrap();
    assert!(p.procedure_by_name("start").is_some());
    assert!(p.procedure_by_name("talk_p_proc").is_some());
    let src = p.pseudo_source();
    assert!(src.contains("procedure talk_p_proc"));
}

#[test]
fn control_flow_is_mostly_structured() {
    let Some(game) = game() else { return };
    let (mut gotos, mut structured, mut loops, mut scripts_with_goto) =
        (0usize, 0usize, 0usize, 0usize);
    for name in game.list("scripts/scripts.lst").iter() {
        let Ok(Some(data)) = game.vfs().try_read(&format!("scripts/{name}")) else {
            continue;
        };
        let Ok(p) = Program::parse(&data) else {
            continue;
        };
        let src = p.pseudo_source();
        let g = src.matches("goto L_").count();
        gotos += g;
        scripts_with_goto += usize::from(g > 0);
        structured += src.matches("if (").count();
        loops += src.matches("while (").count();
    }
    eprintln!("gotos={gotos} if={structured} while={loops} scripts_avec_goto={scripts_with_goto}");
    assert!(loops > 0, "aucune boucle reconnue");
    assert!(
        gotos * 20 < structured + loops,
        "trop de goto restants : {gotos} pour {} constructions",
        structured + loops
    );
}

#[test]
fn variables_are_named() {
    let Some(game) = game() else { return };
    let (mut globals, mut named, mut exported, mut with_locals, mut anonymous_refs) =
        (0, 0, 0, 0, 0);
    for name in game.list("scripts/scripts.lst").iter() {
        let Ok(Some(data)) = game.vfs().try_read(&format!("scripts/{name}")) else {
            continue;
        };
        let Ok(p) = Program::parse(&data) else {
            continue;
        };
        globals += p.globals.len();
        named += p
            .globals
            .iter()
            .filter(|g| !g.name.starts_with("GVar"))
            .count();
        exported += p.exported_vars.len();
        let src = p.pseudo_source();
        with_locals += usize::from(src.contains("variable LVar"));
        anonymous_refs += src.matches("gvar[").count();
    }
    eprintln!(
        "globales={globals} nommées={named} exportées={exported} scripts_avec_locales={with_locals} gvar[]={anonymous_refs}"
    );
    assert!(globals > 0 && with_locals > 0);
    assert!(
        anonymous_refs * 50 < globals,
        "trop d'accès globaux non résolus"
    );
}

#[test]
fn message_texts_are_inlined() {
    let Some(game) = game() else { return };
    let p = Program::parse(&game.read("scripts/arvillag.int").unwrap()).unwrap();
    let plain = p.pseudo_source();
    let with = p.pseudo_source_with(&fo_script::GameMessages::new(&game, "arvillag.int"));
    assert!(!plain.contains('«'));
    assert!(with.contains("message_str(27, 100 /* « Te voilà"), "{with}");
}

/// Scripts de Fallout 1 (`FO1_GAME_DIR`) : tous les `.int` de l'archive se lisent et se désassemblent.
#[test]
fn fallout1_scripts_parse() {
    let root = std::env::var("FO1_GAME_DIR")
        .unwrap_or_else(|_| r"D:\Torrent\Down\Fallout Setup\Fallout1-extract".into());
    let Ok(game) = GameData::open(root) else {
        return;
    };
    let mut failures = Vec::new();
    let mut checked = 0;
    let mut unknown = std::collections::BTreeMap::<String, usize>::new();
    for e in game
        .vfs()
        .list("scripts")
        .into_iter()
        .filter(|e| e.name.ends_with(".int"))
    {
        let Ok(Some(data)) = game.vfs().try_read(&format!("scripts/{}", e.name)) else {
            continue;
        };
        checked += 1;
        match Program::parse(&data) {
            Ok(p) => {
                for part in p.pseudo_source().split("/* opcode inconnu ").skip(1) {
                    *unknown.entry(part.chars().take(4).collect()).or_default() += 1;
                }
            }
            Err(err) => failures.push(format!("{} : {err}", e.name)),
        }
    }
    eprintln!(
        "{checked} scripts, {} échecs, opcodes inconnus : {unknown:?}",
        failures.len()
    );
    assert!(
        unknown.is_empty(),
        "opcodes inconnus dans les scripts de Fallout 1 : {unknown:?}"
    );
    assert!(checked > 100, "{checked} scripts lus");
    assert!(
        failures.is_empty(),
        "{} échecs sur {checked} :\n{}",
        failures.len(),
        failures
            .iter()
            .take(15)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
