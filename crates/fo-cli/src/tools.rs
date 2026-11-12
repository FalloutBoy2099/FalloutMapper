//! Outils d'analyse en ligne de commande : statistiques d'une carte, graphe des liens entre cartes, accessibilité, contrôle, inspection
//! d'un FRM, variables globales du jeu. Chaque outil a une sortie texte lisible et, avec `--json`, une sortie structurée pour les scripts.

use anyhow::{Context, Result, bail};
use fo_core::map::{Map, read_map};
use fo_core::{ArtLibrary, GameData, ObjectType};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

fn load(game: &GameData, name: &str) -> Result<Map> {
    let bytes = game
        .read_map_bytes(name)
        .with_context(|| format!("carte {name}"))?;
    Ok(read_map(&bytes, game)?)
}

fn type_name(t: Option<ObjectType>) -> &'static str {
    match t {
        Some(ObjectType::Item) => "objets",
        Some(ObjectType::Critter) => "créatures",
        Some(ObjectType::Scenery) => "décors",
        Some(ObjectType::Wall) => "murs",
        Some(ObjectType::Tile) => "tuiles",
        Some(ObjectType::Misc) => "divers",
        None => "inconnu",
    }
}

/// Numéro de carte → nom de fichier.
fn numbers(game: &GameData) -> HashMap<i32, String> {
    game.list_maps()
        .into_iter()
        .filter_map(|m| m.number.map(|n| (n as i32, m.file)))
        .collect()
}

fn print(value: &Value, json: bool, text: impl FnOnce()) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        text();
    }
    Ok(())
}

/// Statistiques d'une carte : objets par type, décors les plus fréquents, sols les plus utilisés, sorties, scripts.
pub fn analyze(game: &Arc<GameData>, name: &str, as_json: bool) -> Result<()> {
    let map = load(game, name)?;
    let by_number = numbers(game);
    let tile_names = game.list("art/tiles/tiles.lst");
    let mut types: BTreeMap<&str, usize> = BTreeMap::new();
    let mut scenery: HashMap<u32, usize> = HashMap::new();
    let mut exits: Vec<Value> = Vec::new();
    for e in 0..3 {
        for o in &map.objects[e] {
            *types.entry(type_name(o.object_type())).or_default() += 1;
            if o.object_type() == Some(ObjectType::Scenery) {
                *scenery.entry(o.pid()).or_default() += 1;
            }
            if let Some(d) = o.exit_destination() {
                let dest = match d.map {
                    -2 => "carte du monde".to_string(),
                    -1 => "même carte".to_string(),
                    n => by_number
                        .get(&n)
                        .cloned()
                        .unwrap_or_else(|| format!("#{n}")),
                };
                exits.push(json!({"élévation": e, "hex": o.tile(), "destination": dest, "hex_arrivée": d.tile, "élévation_arrivée": d.elevation}));
            }
        }
    }
    let mut top: Vec<(u32, usize)> = scenery.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let top: Vec<Value> = top.into_iter().take(15).map(|(pid, n)| json!({"pid": format!("{pid:08X}"), "nom": game.proto_entry(pid).map(|p| if p.name.is_empty() { p.file } else { p.name }).unwrap_or_default(), "nombre": n})).collect();
    let mut floors: HashMap<u16, usize> = HashMap::new();
    let mut roofs = 0usize;
    for t in map.tiles.iter().flatten().flatten() {
        if t.floor > 1 {
            *floors.entry(t.floor).or_default() += 1;
        }
        if t.roof > 1 {
            roofs += 1;
        }
    }
    let mut fl: Vec<(u16, usize)> = floors.into_iter().collect();
    fl.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let fl: Vec<Value> = fl.into_iter().take(10).map(|(id, n)| json!({"tuile": id, "nom": tile_names.get(id as usize).cloned().unwrap_or_default(), "cases": n})).collect();
    let scripts: Vec<Value> = fo_core::map::SCRIPT_TYPE_NAMES
        .iter()
        .zip(&map.scripts)
        .map(|(n, l)| json!({"type": n, "nombre": l.count}))
        .collect();
    let v = json!({
        "carte": name,
        "nom": map.header.name(),
        "numéro": map.header.map_number,
        "départ": {"hex": map.header.player_tile, "élévation": map.header.player_elevation},
        "objets_par_type": types,
        "décors_fréquents": top,
        "sols_fréquents": fl,
        "cases_de_toit": roofs,
        "sorties": exits,
        "scripts": scripts,
    });
    print(&v, as_json, || {
        println!(
            "Carte {name} — {} (n° {})",
            map.header.name(),
            map.header.map_number
        );
        println!(
            "départ : hex {} élévation {}",
            map.header.player_tile, map.header.player_elevation
        );
        println!(
            "objets : {}",
            types
                .iter()
                .map(|(k, n)| format!("{n} {k}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        println!("décors les plus fréquents :");
        for t in v["décors_fréquents"].as_array().into_iter().flatten() {
            println!(
                "  {:>4} × {} ({})",
                t["nombre"],
                t["nom"].as_str().unwrap_or(""),
                t["pid"].as_str().unwrap_or("")
            );
        }
        println!("sols les plus utilisés (+ {roofs} cases de toit) :");
        for t in v["sols_fréquents"].as_array().into_iter().flatten() {
            println!(
                "  {:>5} cases  {}",
                t["cases"],
                t["nom"].as_str().unwrap_or("")
            );
        }
        println!("sorties : {}", exits.len());
        for e in &exits {
            println!(
                "  hex {} → {}",
                e["hex"],
                e["destination"].as_str().unwrap_or("")
            );
        }
    })
}

/// Graphe des liens entre cartes (une arête par destination distincte) : texte, JSON ou DOT.
pub fn graph(game: &Arc<GameData>, dot: bool, as_json: bool) -> Result<()> {
    let by_number = numbers(game);
    let mut edges: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entry in game.list_maps() {
        let Ok(map) = load(game, &entry.file) else {
            continue;
        };
        let mut to: Vec<String> = Vec::new();
        for o in map.objects.iter().flatten() {
            if let Some(d) = o.exit_destination() {
                let dest = match d.map {
                    -2 => "carte du monde".to_string(),
                    -1 => continue,
                    n => by_number
                        .get(&n)
                        .cloned()
                        .unwrap_or_else(|| format!("#{n}")),
                };
                if !to.contains(&dest) {
                    to.push(dest);
                }
            }
        }
        to.sort();
        edges.insert(entry.file, to);
    }
    if dot {
        println!("digraph cartes {{");
        for (from, tos) in &edges {
            for t in tos {
                println!("  \"{from}\" -> \"{t}\";");
            }
        }
        println!("}}");
        return Ok(());
    }
    let v = json!(edges);
    print(&v, as_json, || {
        for (from, tos) in &edges {
            println!(
                "{from:<12} → {}",
                if tos.is_empty() {
                    "—".to_string()
                } else {
                    tos.join(", ")
                }
            );
        }
    })
}

/// Qui peut atteindre quoi à pied depuis le point de départ : sorties et portes d'une élévation.
pub fn reach(game: &Arc<GameData>, name: &str, elevation: usize, as_json: bool) -> Result<()> {
    let map = load(game, name)?;
    let start = map.header.player_tile;
    if start < 0 {
        bail!("la carte n'a pas de point de départ");
    }
    let walk = fo_core::reach::walk_map(&map, elevation, game);
    let seen = fo_core::reach::reachable(&walk.blocked, start as u32);
    let reachable_hexes = seen.iter().filter(|&&b| b).count();
    let mut exits = Vec::new();
    let mut doors = Vec::new();
    for o in &map.objects[elevation] {
        if o.tile() < 0 {
            continue;
        }
        let ok = fo_core::reach::touched(&seen, o.tile() as u32);
        if o.exit_destination().is_some() {
            exits.push(json!({"hex": o.tile(), "atteignable": ok}));
        } else if o.object_type() == Some(ObjectType::Scenery)
            && game.proto_subtype(o.pid()) == Some(fo_core::formats::scenery_kind::DOOR)
        {
            doors.push(json!({"hex": o.tile(), "atteignable": ok}));
        }
    }
    let count = |l: &[Value]| l.iter().filter(|v| v["atteignable"] == true).count();
    let v = json!({"carte": name, "élévation": elevation, "hexagones_atteignables": reachable_hexes, "sorties": exits, "portes": doors});
    print(&v, as_json, || {
        println!(
            "{name} élévation {elevation} : {reachable_hexes} hexagones atteignables depuis le départ (hex {start})"
        );
        println!(
            "sorties : {} sur {} atteignables",
            count(&exits),
            exits.len()
        );
        println!(
            "portes  : {} sur {} atteignables",
            count(&doors),
            doors.len()
        );
        for e in exits
            .iter()
            .chain(&doors)
            .filter(|v| v["atteignable"] == false)
        {
            println!("  inatteignable : hex {}", e["hex"]);
        }
    })
}

/// Contrôle d'une carte (le même que « Vérifier la carte » de l'application).
pub fn check(game: &Arc<GameData>, name: &str, as_json: bool) -> Result<()> {
    let map = load(game, name)?;
    let art = ArtLibrary::new(game.clone());
    let issues = fo_core::check::check_map(&map, game, &art);
    let list: Vec<Value> = issues.iter().map(|i| json!({"type": i.kind.label(), "élévation": i.elevation, "hex": i.hex, "détail": i.detail})).collect();
    print(&json!({"carte": name, "problèmes": list}), as_json, || {
        if issues.is_empty() {
            println!("{name} : aucun problème.");
        }
        for (kind, n) in fo_core::check::count_by_kind(&issues) {
            println!("{n:>5}  {}", kind.label());
        }
        for i in issues.iter().take(30) {
            println!(
                "  {} — hex {} {}",
                i.kind.label(),
                i.hex.map_or("-".to_string(), |h| h.to_string()),
                i.detail
            );
        }
        if issues.len() > 30 {
            println!(
                "  … et {} autres (utilisez --json pour tout voir)",
                issues.len() - 30
            );
        }
    })
}

/// Inspection d'un fichier FRM : cadence, directions, images et leur taille.
pub fn frm(game: &Arc<GameData>, path: &str, as_json: bool) -> Result<()> {
    let full = if path.starts_with("art/") {
        path.to_string()
    } else {
        format!("art/{path}")
    };
    let art = ArtLibrary::new(game.clone());
    let frm = art
        .frm(&full)
        .with_context(|| format!("FRM introuvable : {full}"))?;
    let dirs: Vec<Value> = frm.directions.iter().enumerate().map(|(i, d)| json!({"direction": i, "décalage": [d.shift_x, d.shift_y], "images": d.frames.iter().map(|f| json!({"largeur": f.width, "hauteur": f.height, "dx": f.dx, "dy": f.dy})).collect::<Vec<_>>()})).collect();
    let v = json!({"fichier": full, "version": frm.version, "images_par_seconde": frm.fps, "image_d_action": frm.action_frame, "directions": dirs});
    print(&v, as_json, || {
        println!(
            "{full} : version {}, {} images/s, image d'action {}, {} direction(s)",
            frm.version,
            frm.fps,
            frm.action_frame,
            frm.directions.len()
        );
        for (i, d) in frm.directions.iter().enumerate() {
            let sizes: Vec<String> = d
                .frames
                .iter()
                .map(|f| format!("{}×{}", f.width, f.height))
                .collect();
            println!(
                "  direction {i} : {} image(s) [{}] décalage {},{}",
                d.frames.len(),
                sizes.join(" "),
                d.shift_x,
                d.shift_y
            );
        }
    })
}

/// Variables globales du jeu (`data/vault13.gam`) : numéro, nom, valeur initiale.
pub fn gvars(game: &Arc<GameData>, filter: Option<&str>, as_json: bool) -> Result<()> {
    let bytes = game
        .read("data/vault13.gam")
        .or_else(|_| game.read("data/VAULT13.GAM"))
        .context("vault13.gam introuvable")?;
    let text: String = bytes.iter().map(|&b| b as char).collect();
    let mut vars: Vec<Value> = Vec::new();
    let mut in_globals = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with("GAME_GLOBAL_VARS") {
            in_globals = true;
            continue;
        }
        if l.starts_with("MAP_GLOBAL_VARS") {
            in_globals = false;
        }
        if !in_globals {
            continue;
        }
        if let Some((name, rest)) = l.split_once(":=") {
            let value = rest.split([';', '/']).next().unwrap_or("").trim();
            let comment = rest.split("//").nth(1).map(str::trim).unwrap_or("");
            vars.push(json!({"numéro": vars.len(), "nom": name.trim(), "valeur": value, "commentaire": comment}));
        }
    }
    let needle = filter.map(str::to_lowercase);
    vars.retain(|v| {
        needle.as_ref().is_none_or(|n| {
            v["nom"]
                .as_str()
                .unwrap_or("")
                .to_lowercase()
                .contains(n.as_str())
                || v["commentaire"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(n.as_str())
        })
    });
    print(&json!(vars), as_json, || {
        for v in &vars {
            println!(
                "{:>4}  {:<28} = {:<6} {}",
                v["numéro"],
                v["nom"].as_str().unwrap_or(""),
                v["valeur"].as_str().unwrap_or(""),
                v["commentaire"].as_str().unwrap_or("")
            );
        }
        println!("{} variable(s)", vars.len());
    })
}
