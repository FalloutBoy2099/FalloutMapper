//! Tests sur de vraies données de jeu. Ignorés si aucune installation n'est disponible.
//!
//! Variables d'environnement : `FO2_GAME_DIR` (installation, par défaut le dossier décompressé de développement)
//! et `FO2_GAME_DIR_PACKED` (installation avec de vraies archives .dat).

use fo_core::{GameData, read_map, write_map};
use std::path::PathBuf;

fn dir(var: &str, default: &str) -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var(var).unwrap_or_else(|_| default.to_string()));
    p.is_dir().then_some(p)
}

fn unpacked() -> Option<GameData> {
    GameData::open(dir("FO2_GAME_DIR", r"C:\Fallout2\F2_RPU_2.4.34_old")?).ok()
}

#[test]
fn transparency_matches_ce_reference_palette_samples() {
    use fo_core::palette_blend::Effect;
    let Some(game) = unpacked() else { return };
    let data = game.read("color.pal").unwrap();
    let fnv = |bytes: &[u8]| {
        bytes.iter().fold(0xcbf29ce484222325u64, |h, &v| {
            (h ^ u64::from(v)).wrapping_mul(0x100000001b3)
        })
    };
    // Golden samples evaluated from CE's gray, blend and intensity routines.
    // Custom palettes are valid game data but do not share this reference hash.
    if fnv(&data) != 0x5070763a4a0eef84 {
        return;
    }
    let palette = game.palette();
    let tables = palette.tables();
    let mut samples = Vec::new();
    for effect in [
        Effect::Red,
        Effect::Wall,
        Effect::Glass,
        Effect::Steam,
        Effect::Energy,
    ] {
        for source in (0..=255).step_by(17) {
            for dest in (0..=255).step_by(11) {
                for light in [1.0, 0.5, 0.2] {
                    samples.push(tables.mix(effect, source, dest, light));
                }
            }
        }
    }
    assert_eq!(samples.len(), 5760);
    assert_eq!(fnv(&samples), 0xf6a13888ac357c9b);
}

fn packed() -> Option<GameData> {
    GameData::open(dir("FO2_GAME_DIR_PACKED", r"C:\Fallout2\F2_RPU_2.4.34")?).ok()
}

#[test]
fn lists_maps() {
    let Some(game) = unpacked() else { return };
    let maps = game.list_maps();
    assert!(maps.len() > 100, "{} cartes", maps.len());
    assert!(maps.iter().any(|m| m.file == "arvillag"));
    let first = &maps[0];
    assert!(first.number.is_some(), "tri par numéro de carte");
    // Noms du jeu : zone de city.txt + worldmap.msg + map.msg.
    let title = |f: &str| {
        maps.iter()
            .find(|m| m.file == f)
            .and_then(|m| m.display.clone())
    };
    assert_eq!(title("arvillag").as_deref(), Some("Arroyo — Village"));
    // Le texte suit la langue du jeu (fallout2.cfg) : « Bridge » en anglais, « Pont » en français.
    assert!(title("arbridge").is_some_and(|t| t.starts_with("Arroyo — ")));
    assert!(
        title("klamall").is_some(),
        "une carte hors zone garde le nom de maps.txt"
    );
}

#[test]
fn every_map_reads_and_round_trips() {
    let Some(game) = unpacked() else { return };
    let mut failures = Vec::new();
    for entry in game.list_maps() {
        let bytes = game.read_map_bytes(&entry.file).unwrap();
        match read_map(&bytes, &game) {
            Err(e) => failures.push(format!("{} : lecture : {e}", entry.file)),
            Ok(map) => {
                assert!(map.trailing.is_empty(), "{} : octets en trop", entry.file);
                match write_map(&map) {
                    Ok(out) if out == bytes => {}
                    Ok(out) => failures.push(format!(
                        "{} : écriture différente ({} octets, attendu {})",
                        entry.file,
                        out.len(),
                        bytes.len()
                    )),
                    Err(e) => failures.push(format!("{} : écriture : {e}", entry.file)),
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} échec(s) :\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn packed_install_matches_unpacked_files() {
    let (Some(packed), Some(unpacked)) = (packed(), unpacked()) else {
        return;
    };
    assert!(packed.layers().len() >= 2);
    // color.pal ne change pas entre deux versions d'un même jeu.
    assert_eq!(
        packed.read("color.pal").unwrap(),
        unpacked.read("color.pal").unwrap()
    );
    let maps = packed.list_maps();
    assert!(maps.len() > 100);
    let name = &maps
        .iter()
        .find(|m| m.file == "arvillag")
        .expect("arvillag")
        .file;
    let bytes = packed.read_map_bytes(name).unwrap();
    read_map(&bytes, &packed).expect("lecture depuis un vrai .dat compressé");
}

#[test]
fn object_scripts_resolve_to_script_files() {
    let Some(game) = unpacked() else { return };
    let map = read_map(&game.read_map_bytes("arvillag").unwrap(), &game).unwrap();
    let with_script: Vec<_> = map
        .objects
        .iter()
        .flatten()
        .filter(|o| o.script_id() != -1)
        .collect();
    assert!(!with_script.is_empty());
    for o in &with_script {
        let entry = map
            .find_script(o.script_id())
            .unwrap_or_else(|| panic!("script {:08X} introuvable", o.script_id()));
        let file = game
            .script_file(entry.list_index() as usize)
            .expect("scripts.lst");
        assert!(file.ends_with(".int"), "{file}");
        assert!(
            game.vfs().exists(&format!("scripts/{file}")),
            "{file} absent"
        );
    }
    let main = game.script_file(map.map_script_index().unwrap()).unwrap();
    assert!(main.eq_ignore_ascii_case("arvillag.int"), "{main}");
}

#[test]
fn dialog_messages_load_in_configured_language() {
    let Some(game) = unpacked() else { return };
    assert_eq!(game.config.language, "french");
    let m = game.script_messages("ArVillag.int").expect("arvillag.msg");
    assert!(
        m.get(100).unwrap().text.starts_with("Te voilà"),
        "{:?}",
        m.get(100)
    );
    // Une langue absente retombe sur l'anglais ; un fichier inconnu donne None.
    assert!(game.script_messages("nonexistent.int").is_none());
}

#[test]
fn edits_survive_write_and_read() {
    use fo_core::map::{TileLayer, word};
    use fo_core::{MapObject, ObjectType};
    let Some(game) = unpacked() else { return };
    let original = read_map(&game.read_map_bytes("arvillag").unwrap(), &game).unwrap();
    let mut map = original.clone();

    // Déplacer, supprimer, peindre, ajouter un décor issu d'un prototype.
    assert!(map.move_object(0, 0, 12345));
    let removed = map.remove_object(0, 1).unwrap();
    assert!(map.set_tile(0, 5050, TileLayer::Floor, 7));
    let catalog = game.proto_catalog(ObjectType::Scenery);
    assert!(catalog.len() > 100);
    assert!(
        catalog.iter().any(|e| !e.name.is_empty()),
        "noms des prototypes"
    );
    let door = catalog
        .iter()
        .find(|e| e.info.subtype == 0)
        .expect("une porte");
    let id = map.max_object_id() + 1;
    let obj = MapObject::from_proto(&door.info, 20100, id).unwrap();
    let idx = map.add_object(0, obj);

    let bytes = write_map(&map).unwrap();
    let back = read_map(&bytes, &game).expect("la carte modifiée se relit");
    assert_eq!(back, map);
    assert_eq!(back.objects[0].len(), original.objects[0].len());
    assert_eq!(back.objects[0][0].tile(), 12345);
    assert_eq!(back.tile(0, 5050).unwrap().floor, 7);
    assert_eq!(back.objects[0][idx].words[word::PID] as u32, door.pid);
    assert_eq!(
        back.objects[0][idx].extra.len(),
        1,
        "une porte a un mot d'état"
    );
    assert_ne!(removed.words[word::ID], back.objects[0][1].words[word::ID]);
}

#[test]
fn save_map_writes_to_patches_with_backup() {
    let root = std::env::temp_dir().join(format!("fomapper_save_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("data")).unwrap();
    let game = GameData::open(&root).unwrap();
    let path = game.save_map("Test.map", b"one").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"one");
    let path2 = game.save_map("test", b"two").unwrap();
    assert_eq!(path, path2);
    assert_eq!(
        std::fs::read(path.with_extension("map.bak")).unwrap(),
        b"one"
    );
    assert!(game.save_map("../evil", b"x").is_err());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn edge_files_are_saved_beside_the_map_and_read_back() {
    use fo_core::edge::{EdgeRect, MapEdge};
    let root = std::env::temp_dir().join(format!("fomapper_edge_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("data")).unwrap();
    let game = GameData::open(&root).unwrap();
    let mut edge = MapEdge::default();
    edge.elevations[0].zones = vec![EdgeRect {
        left: 100,
        top: 200,
        right: 300,
        bottom: 400,
    }];
    let bytes = edge.to_bytes().unwrap();
    let path = game.save_edge("Test.map", &bytes).unwrap();
    assert!(path.ends_with("test.edg"));
    assert_eq!(game.read_edge("test"), Some(edge));
    assert!(game.remove_edge_override("test").unwrap());
    assert_eq!(game.read_edge("test"), None);
    assert!(!game.remove_edge_override("test").unwrap());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn object_scripts_attach_detach_and_survive_write_read() {
    use fo_core::map::{SCRIPT_TYPE_CRITTER, SCRIPT_TYPE_ITEM, word};
    let Some(game) = unpacked() else { return };
    let original = read_map(&game.read_map_bytes("arvillag").unwrap(), &game).unwrap();
    let mut map = original.clone();

    // Un décor sans script et une créature sans script.
    let scenery = map.objects[0]
        .iter()
        .position(|o| o.object_type() == Some(fo_core::ObjectType::Scenery) && o.script_id() == -1)
        .expect("décor sans script");
    let critter = map.objects[0]
        .iter()
        .position(|o| o.object_type() == Some(fo_core::ObjectType::Critter))
        .expect("créature");
    let before_item = map.scripts[SCRIPT_TYPE_ITEM].count;
    assert!(map.attach_object_script(0, scenery, 216));
    assert_eq!(map.scripts[SCRIPT_TYPE_ITEM].count, before_item + 1);
    let sid = map.objects[0][scenery].script_id();
    assert_eq!((sid as u32) >> 24, SCRIPT_TYPE_ITEM as u32);
    assert_eq!(map.find_script(sid).unwrap().list_index(), 216);
    // Remplacer le script de la créature : l'ancienne entrée disparaît, la liste garde le même effectif.
    let before_critter = map.scripts[SCRIPT_TYPE_CRITTER].count;
    let had = map.objects[0][critter].script_id() != -1;
    assert!(map.attach_object_script(0, critter, 11));
    assert_eq!(
        map.scripts[SCRIPT_TYPE_CRITTER].count,
        if had {
            before_critter
        } else {
            before_critter + 1
        }
    );
    assert_eq!(map.objects[0][critter].words[word::SCRIPT_INDEX], 11);

    // Écriture puis relecture : mêmes scripts, mêmes liens.
    let back = read_map(&write_map(&map).unwrap(), &game).expect("relecture");
    assert_eq!(back, map);
    let e = back
        .find_script(back.objects[0][scenery].script_id())
        .unwrap();
    assert_eq!(e.list_index(), 216);
    assert_eq!(
        e.tail[3],
        back.objects[0][scenery].words[word::ID],
        "le propriétaire de l'entrée est l'objet"
    );

    // Détacher rend la carte identique à l'original pour cet objet.
    assert!(map.detach_object_script(0, scenery));
    assert_eq!(map.objects[0][scenery].script_id(), -1);
    assert_eq!(map.scripts[SCRIPT_TYPE_ITEM].count, before_item);
    assert!(!map.detach_object_script(0, scenery));

    // Script spatial + script de la carte.
    let pid = map.add_spatial_script(7, 20_000, 0, 3);
    let spatial = map.find_script(pid as i32).unwrap().spatial().unwrap();
    assert_eq!(
        (spatial.tile, spatial.elevation, spatial.radius),
        (20_000, 0, 3)
    );
    assert!(map.edit_spatial_script(pid, 8, 20_100, 0, 5));
    assert_eq!(
        map.find_script(pid as i32)
            .unwrap()
            .spatial()
            .unwrap()
            .radius,
        5
    );
    map.set_map_script(Some(26));
    assert_eq!(map.map_script_index(), Some(26));
    map.set_map_script(None);
    assert_eq!(map.map_script_index(), None);
    let back = read_map(&write_map(&map).unwrap(), &game).expect("relecture avec script spatial");
    assert_eq!(back, map);
}

#[test]
fn creatures_and_inventory_items_come_from_prototypes() {
    use fo_core::{MapObject, ObjectType};
    let Some(game) = unpacked() else { return };
    let crit = game.proto_catalog(ObjectType::Critter);
    assert!(crit.len() > 100, "catalogue de créatures");
    let entry = crit
        .iter()
        .find(|e| e.info.critter.as_ref().is_some_and(|c| c.hit_points > 0))
        .expect("une créature");
    let mut map = read_map(&game.read_map_bytes("arvillag").unwrap(), &game).unwrap();
    let id = map.max_object_id() + 1;
    let obj = MapObject::from_proto(&entry.info, 20_100, id)
        .expect("une créature se crée depuis son prototype");
    assert_eq!(obj.extra.len(), 10);
    assert_eq!(
        obj.extra[7],
        entry.info.critter.as_ref().unwrap().hit_points
    );
    let idx = map.add_object(0, obj);
    // Un objet dans son inventaire.
    let items = game.proto_catalog(ObjectType::Item);
    let weapon = items
        .iter()
        .find(|e| e.info.subtype == 3)
        .expect("une arme");
    let item = MapObject::for_inventory(&weapon.info, map.max_object_id() + 2).unwrap();
    assert_eq!(item.tile(), -1);
    map.objects[0][idx].push_inventory(2, item);
    let back =
        read_map(&write_map(&map).unwrap(), &game).expect("relecture avec créature et inventaire");
    assert_eq!(back, map);
    assert_eq!(back.objects[0][idx].inventory[0].count, 2);
}

#[test]
fn game_text_files_round_trip_through_the_ini_model() {
    use fo_core::ini_doc::IniDoc;
    let Some(game) = unpacked() else { return };
    for path in ["data/maps.txt", "data/city.txt", "data/worldmap.txt"] {
        let raw = game.vfs().read(path).unwrap();
        let doc = IniDoc::parse(&game.read_data_text(path).unwrap());
        // Latin-1 : un caractère par octet, donc la réécriture redonne les mêmes octets.
        let back: Vec<u8> = doc.to_text().chars().map(|c| c as u32 as u8).collect();
        assert_eq!(back.len(), raw.len(), "{path}");
        assert_eq!(back, raw, "{path} : réécriture identique");
    }
    let city = IniDoc::parse(&game.read_data_text("data/city.txt").unwrap());
    let areas = fo_core::worldmap::parse_areas(&city);
    assert!(areas.len() > 30, "{} zones", areas.len());
    assert!(
        areas
            .iter()
            .any(|a| a.name == "Arroyo" && !a.entrances.is_empty())
    );
    let wm = IniDoc::parse(&game.read_data_text("data/worldmap.txt").unwrap());
    let (per_row, tiles) = fo_core::worldmap::parse_tiles(&wm);
    assert!(
        per_row > 0 && tiles.len() >= 20,
        "{per_row} × {}",
        tiles.len()
    );
    let maps = IniDoc::parse(&game.read_data_text("data/maps.txt").unwrap());
    assert!(
        fo_core::maps_txt::validate(&maps).is_empty(),
        "{:?}",
        fo_core::maps_txt::validate(&maps)
    );
}

#[test]
fn new_empty_map_writes_and_reads_back() {
    let Some(game) = unpacked() else { return };
    for n in 1..=3 {
        let map = fo_core::Map::new_empty("brandnew", n);
        let bytes = write_map(&map).unwrap();
        let back = read_map(&bytes, &game).unwrap();
        assert_eq!(back, map);
        assert_eq!((0..3).filter(|&e| back.header.has_elevation(e)).count(), n);
        assert_eq!(back.header.name(), "BRANDNEW.MAP");
    }
}

/// Archives DAT1 de Fallout 1 (`FO1_GAME_DIR`) : l'index se lit et chaque fichier se décompresse à la bonne taille.
#[test]
fn fallout1_dat1_archives_read() {
    let Some(root) = dir(
        "FO1_GAME_DIR",
        r"D:\Torrent\Down\Fallout Setup\Fallout1-extract",
    ) else {
        return;
    };
    let path = root.join("MASTER.DAT");
    if !path.is_file() {
        return;
    }
    let archive = fo_core::dat::DatArchive::open(&path).unwrap();
    assert_eq!(archive.format(), fo_core::dat::DatFormat::Dat1);
    assert!(
        archive.entries().len() > 1000,
        "{} entrées",
        archive.entries().len()
    );
    let mut maps = 0;
    for e in archive
        .entries()
        .iter()
        .filter(|e| e.path.to_ascii_lowercase().ends_with(".map"))
    {
        let data = archive.read(e).unwrap();
        assert_eq!(data.len(), e.size as usize, "{}", e.path);
        maps += 1;
    }
    assert!(maps > 10, "{maps} cartes dans l'archive");
}

/// Toutes les cartes de Fallout 1 (version 19) se lisent et se réécrivent à l'identique.
#[test]
fn fallout1_maps_round_trip() {
    let Some(root) = dir(
        "FO1_GAME_DIR",
        r"D:\Torrent\Down\Fallout Setup\Fallout1-extract",
    ) else {
        return;
    };
    let Ok(game) = GameData::open(root) else {
        return;
    };
    let maps = game.list_maps();
    if maps.is_empty() {
        return;
    }
    let mut failures = Vec::new();
    for entry in &maps {
        let bytes = game.read_map_bytes(&entry.file).unwrap();
        match read_map(&bytes, &game) {
            Err(e) => failures.push(format!("{} : lecture : {e}", entry.file)),
            Ok(map) => {
                if write_map(&map)
                    .as_ref()
                    .map(|w| w != &bytes)
                    .unwrap_or(true)
                {
                    failures.push(format!("{} : écriture différente", entry.file));
                }
            }
        }
    }
    // childrn2 contient un objet dont le prototype (objet n° 255) n'existe pas dans Fallout 1 : défaut des données d'origine.
    failures.retain(|f| {
        !f.starts_with(
            "childrn2 : lecture : données invalides : prototype d'objet introuvable (000000FF)",
        )
    });
    assert!(
        failures.is_empty(),
        "{} échecs sur {} :\n{}",
        failures.len(),
        maps.len(),
        failures.join("\n")
    );
}

/// Les noms de prototypes viennent des `pro_*.msg` (dans la langue du jeu) : au moins un objet et un décor ont un nom.
#[test]
fn prototype_names_come_from_the_msg_files() {
    let Some(game) = unpacked() else { return };
    let items = game.proto_catalog(fo_core::ObjectType::Item);
    assert!(
        items.iter().filter(|e| !e.name.is_empty()).count() > 100,
        "noms d'objets"
    );
    let scenery = game.proto_catalog(fo_core::ObjectType::Scenery);
    assert!(scenery.iter().any(|e| !e.name.is_empty()));
    let first = items.iter().find(|e| !e.name.is_empty()).unwrap();
    assert_eq!(
        game.proto_entry(first.pid).map(|e| e.name),
        Some(first.name.clone())
    );
}

/// Enregistrement d'une carte de Fallout 1 : les archives de l'installation ne sont lues qu'en lecture seule, les écritures
/// vont dans un dossier temporaire dont le `fallout2.cfg` pointe vers elles (aucun fichier de l'installation n'est modifié).
#[test]
fn fallout1_map_saves_and_reloads_from_a_scratch_root() {
    let Some(real) = dir(
        "FO1_GAME_DIR",
        r"D:\Torrent\Down\Fallout Setup\Fallout1-extract",
    ) else {
        return;
    };
    let (master, critter) = (real.join("MASTER.DAT"), real.join("CRITTER.DAT"));
    if !master.is_file() {
        return;
    }
    let scratch = std::env::temp_dir().join(format!("fo1_save_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let cfg = format!(
        "[system]\nmaster_dat={}\ncritter_dat={}\nmaster_patches=data\ncritter_patches=data\nlanguage=english\n",
        master.display(),
        critter.display()
    );
    std::fs::write(scratch.join("fallout2.cfg"), cfg).unwrap();

    let result = std::panic::catch_unwind(|| {
        let game = GameData::open(&scratch).unwrap();
        assert!(game.is_fallout1());
        let original = game.read_map_bytes("junkent").unwrap();
        let mut map = read_map(&original, &game).unwrap();
        assert_eq!(map.header.version, 19);

        // 1. Réécriture sans modification : identique à l'octet près, y compris une fois relue depuis le dossier de correctifs.
        let bytes = write_map(&map).unwrap();
        assert_eq!(bytes, original);
        let path = game.save_map("junkent", &bytes).unwrap();
        assert!(
            path.starts_with(&scratch),
            "l'enregistrement reste dans le dossier temporaire : {}",
            path.display()
        );

        // 2. Avec une modification : un mur de plus.
        let wall = map.objects[0]
            .iter()
            .find(|o| o.object_type() == Some(fo_core::ObjectType::Wall))
            .cloned()
            .unwrap();
        let before = map.objects[0].len();
        let mut extra = wall.clone();
        extra.words[fo_core::map::word::TILE] = 20_000;
        extra.words[fo_core::map::word::ID] = map.max_object_id() + 1;
        map.add_object(0, extra);
        game.save_map("junkent", &write_map(&map).unwrap()).unwrap();
        assert!(
            scratch
                .join("data")
                .join("maps")
                .join("junkent.map.bak")
                .is_file(),
            "la première version est sauvegardée en .bak"
        );

        // 3. Rouvert : la version modifiée (dossier de correctifs) prime sur l'archive.
        let again = GameData::open(&scratch).unwrap();
        let reread = read_map(&again.read_map_bytes("junkent").unwrap(), &again).unwrap();
        assert_eq!(
            reread.header.version, 19,
            "la version de la carte est conservée"
        );
        assert_eq!(reread.objects[0].len(), before + 1);
        // Et l'original de l'installation n'a pas bougé.
        assert!(std::fs::metadata(&master).unwrap().len() > 0);
    });
    let _ = std::fs::remove_dir_all(&scratch);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

/// Les `.edg` du patch haute résolution de Fallout 1 (format EDGE v1) se relisent et se réécrivent à l'identique.
#[test]
fn fallout1_edge_files_round_trip() {
    let Some(root) = dir(
        "FO1_GAME_DIR",
        r"D:\Torrent\Down\Fallout Setup\Fallout1-extract",
    ) else {
        return;
    };
    let Ok(game) = GameData::open(root) else {
        return;
    };
    let (mut n, mut failures) = (0, Vec::new());
    for entry in game.list_maps() {
        let Ok(bytes) = game.read(&format!("maps/{}.edg", entry.file)) else {
            continue;
        };
        n += 1;
        match fo_core::edge::MapEdge::parse(&bytes).and_then(|e| e.to_bytes()) {
            Ok(out) if out == bytes => {}
            Ok(_) => failures.push(format!("{} : écriture différente", entry.file)),
            Err(e) => failures.push(format!("{} : {e}", entry.file)),
        }
    }
    assert!(
        failures.is_empty(),
        "{} échecs sur {n} :\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Prototypes de Fallout 1 : chaque fichier `.pro` se lit, son PID et le type de son FID sont cohérents, la plupart ont un nom
/// (`pro_*.msg`) et l'art de l'objet existe.
#[test]
fn fallout1_prototypes_are_consistent() {
    let Some(root) = dir(
        "FO1_GAME_DIR",
        r"D:\Torrent\Down\Fallout Setup\Fallout1-extract",
    ) else {
        return;
    };
    let Ok(game) = GameData::open(root) else {
        return;
    };
    let game = std::sync::Arc::new(game);
    let art = fo_core::ArtLibrary::new(game.clone());
    let mut report = Vec::new();
    for ty in [
        fo_core::ObjectType::Item,
        fo_core::ObjectType::Scenery,
        fo_core::ObjectType::Wall,
        fo_core::ObjectType::Tile,
        fo_core::ObjectType::Misc,
    ] {
        let listed = game
            .list(&format!("proto/{0}/{0}.lst", ty.dir_name()))
            .len();
        let catalog = game.proto_catalog(ty);
        let (mut bad_pid, mut bad_fid, mut unnamed, mut no_art) = (0, 0, 0, 0);
        for e in &catalog {
            bad_pid += usize::from(e.info.pid != e.pid);
            bad_fid += usize::from(e.info.fid >> 24 != ty as u32 && e.info.fid != u32::MAX);
            unnamed += usize::from(e.name.is_empty());
            let has_art = art
                .fid_paths(fo_core::Fid(e.info.fid), 0)
                .iter()
                .any(|p| game.vfs().exists(p));
            no_art += usize::from(!has_art);
        }
        report.push(format!("{ty:?} : listés {listed}, lus {}, pid≠ {bad_pid}, fid de mauvais type {bad_fid}, sans nom {unnamed}, sans art {no_art}", catalog.len()));
        assert_eq!(
            catalog.len(),
            listed,
            "{ty:?} : des prototypes ne se lisent pas\n{}",
            report.join("\n")
        );
        assert_eq!(bad_pid + bad_fid, 0, "{}", report.join("\n"));
    }
    eprintln!("{}", report.join("\n"));
}

/// Scripts attachés aux cartes de Fallout 1 : le script de la carte et ceux de ses objets pointent vers un `.int` qui existe.
#[test]
fn fallout1_map_scripts_resolve_to_files() {
    let Some(root) = dir(
        "FO1_GAME_DIR",
        r"D:\Torrent\Down\Fallout Setup\Fallout1-extract",
    ) else {
        return;
    };
    let Ok(game) = GameData::open(root) else {
        return;
    };
    let (mut checked, mut missing) = (0usize, Vec::new());
    for entry in game.list_maps() {
        let Ok(map) = read_map(&game.read_map_bytes(&entry.file).unwrap(), &game) else {
            continue;
        };
        let indexes = map.map_script_index().into_iter().chain(
            map.scripts
                .iter()
                .flat_map(|l| l.iter())
                .map(|e| e.list_index().max(0) as usize),
        );
        for i in indexes {
            checked += 1;
            let ok = game
                .script_file(i)
                .is_some_and(|f| game.vfs().exists(&format!("scripts/{f}")));
            if !ok {
                missing.push(format!(
                    "{} : script n° {i} → {:?}",
                    entry.file,
                    game.script_file(i)
                ));
            }
        }
    }
    missing.sort();
    missing.dedup();
    assert!(checked > 500, "{checked} scripts vérifiés");
    assert!(
        missing.len() * 50 < checked,
        "{} introuvables sur {checked} :\n{}",
        missing.len(),
        missing
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Fallout 1 : les noms et l'ordre des cartes viennent de `map.msg` (pas de `maps.txt`).
#[test]
fn fallout1_map_names_come_from_map_msg() {
    let Some(root) = dir(
        "FO1_GAME_DIR",
        r"D:\Torrent\Down\Fallout Setup\Fallout1-extract",
    ) else {
        return;
    };
    let Ok(game) = GameData::open(root) else {
        return;
    };
    let maps = game.list_maps();
    let find = |f: &str| {
        maps.iter()
            .find(|m| m.file == f)
            .unwrap_or_else(|| panic!("{f}"))
    };
    assert_eq!(find("desert1").number, Some(0));
    assert_eq!(find("junkent").number, Some(10));
    assert_eq!(
        maps.first().map(|m| m.file.as_str()),
        Some("desert1"),
        "triées par numéro de carte"
    );
    // Les numéros correspondent à ceux de l'en-tête des cartes.
    for f in ["junkent", "vault13", "shadye"] {
        let map = read_map(&game.read_map_bytes(f).unwrap(), &game).unwrap();
        assert_eq!(find(f).number, Some(map.header.map_number as u32), "{f}");
    }
    // Un nom lisible, avec le fichier quand plusieurs cartes portent le même (« Junktown (junkent) »).
    let title = |f: &str| find(f).display.clone().unwrap_or_default();
    assert!(
        title("junkent").starts_with("Junktown"),
        "{}",
        title("junkent")
    );
    assert!(
        title("junkent").contains("junkent"),
        "doublon distingué : {}",
        title("junkent")
    );
    // Les cartes absentes de map.msg gardent leur nom de fichier.
    assert!(maps.iter().any(|m| m.display.is_none()));
}

/// Le vérificateur repère les problèmes qu'on injecte dans une vraie carte (et ne s'affole pas sur la carte intacte).
#[test]
fn checker_finds_injected_problems() {
    use fo_core::check::{IssueKind, check_map, count_by_kind};
    let Some(game) = unpacked() else { return };
    let game = std::sync::Arc::new(game);
    let art = fo_core::ArtLibrary::new(game.clone());
    let mut map = read_map(&game.read_map_bytes("arvillag").unwrap(), &*game).unwrap();
    let before = count_by_kind(&check_map(&map, &game, &art));
    let n = |counts: &[(IssueKind, usize)], k: IssueKind| {
        counts
            .iter()
            .find(|(kk, _)| *kk == k)
            .map_or(0, |(_, c)| *c)
    };

    // Objet hors grille, doublon exact, prototype inconnu, point de départ hors carte.
    let template = map.objects[0]
        .iter()
        .find(|o| o.tile() >= 0 && o.object_type() == Some(fo_core::ObjectType::Scenery))
        .cloned()
        .unwrap();
    let mut off = template.clone();
    off.words[fo_core::map::word::TILE] = -7;
    let mut twin = template.clone();
    twin.words[fo_core::map::word::ID] = map.max_object_id() + 1;
    let mut unknown = template.clone();
    unknown.words[fo_core::map::word::PID] = 0x0200_7FFF;
    unknown.words[fo_core::map::word::TILE] = 100;
    unknown.words[fo_core::map::word::ID] = map.max_object_id() + 2;
    for o in [off, twin, unknown] {
        map.add_object(0, o);
    }
    map.header.player_tile = 99_999;
    let after = check_map(&map, &game, &art);
    let counts = count_by_kind(&after);
    assert_eq!(
        n(&counts, IssueKind::OutOfGrid),
        n(&before, IssueKind::OutOfGrid) + 1,
        "{counts:?}"
    );
    assert_eq!(
        n(&counts, IssueKind::Duplicate),
        n(&before, IssueKind::Duplicate) + 1,
        "{counts:?}"
    );
    assert_eq!(
        n(&counts, IssueKind::UnknownPrototype),
        n(&before, IssueKind::UnknownPrototype) + 1,
        "{counts:?}"
    );
    assert_eq!(
        n(&counts, IssueKind::BadStart),
        n(&before, IssueKind::BadStart) + 1,
        "{counts:?}"
    );
    // Les problèmes pointent vers l'objet fautif.
    assert!(
        after
            .iter()
            .any(|i| i.kind == IssueKind::Duplicate && i.index == Some(map.objects[0].len() - 2))
    );
    // Une carte du jeu intacte n'a presque rien à signaler.
    assert!(
        before.iter().map(|(_, c)| c).sum::<usize>() < 40,
        "{before:?}"
    );
    eprintln!("arvillag intacte : {before:?}");
}

/// Corriger les problèmes injectés dans une vraie carte les fait disparaître au contrôle suivant.
#[test]
fn checker_fixes_what_it_finds() {
    use fo_core::check::{IssueKind, check_map, fix_issues};
    let Some(game) = unpacked() else { return };
    let game = std::sync::Arc::new(game);
    let art = fo_core::ArtLibrary::new(game.clone());
    let mut map = read_map(&game.read_map_bytes("arvillag").unwrap(), &*game).unwrap();
    let baseline = check_map(&map, &game, &art).len();
    let template = map.objects[0]
        .iter()
        .find(|o| o.tile() >= 0 && o.object_type() == Some(fo_core::ObjectType::Scenery))
        .cloned()
        .unwrap();
    let (mut off, mut twin, mut unknown) = (template.clone(), template.clone(), template.clone());
    off.words[fo_core::map::word::TILE] = -7;
    twin.words[fo_core::map::word::ID] = map.max_object_id() + 1;
    unknown.words[fo_core::map::word::PID] = 0x0200_7FFF;
    unknown.words[fo_core::map::word::TILE] = 100;
    unknown.words[fo_core::map::word::ID] = map.max_object_id() + 2;
    for o in [off, twin, unknown] {
        map.add_object(0, o);
    }
    map.header.player_tile = 99_999;
    let issues = check_map(&map, &game, &art);
    assert!(issues.len() >= baseline + 4);
    let fixable: Vec<_> = issues.into_iter().filter(|i| i.kind.fixable()).collect();
    let report = fix_issues(&mut map, &fixable);
    assert!(report.removed >= 3 && report.start_moved, "{report:?}");
    let after = check_map(&map, &game, &art);
    for k in [
        IssueKind::OutOfGrid,
        IssueKind::UnknownPrototype,
        IssueKind::BadStart,
    ] {
        assert!(
            after.iter().all(|i| i.kind != k),
            "{k:?} persiste : {after:?}"
        );
    }
    assert!(
        after.len() <= baseline,
        "{} problèmes après correction, {baseline} au départ",
        after.len()
    );
}

/// Modifier une entrée d'un vrai `.msg` ne change rien d'autre : toutes les autres entrées, les commentaires et la mise en page
/// restent identiques à l'octet (essai sur tous les fichiers de messages de l'installation).
#[test]
fn msg_patch_is_surgical_on_every_real_file() {
    use fo_core::msg::{MsgEdit, MsgFile, patch};
    let Some(game) = unpacked() else { return };
    let mut files = Vec::new();
    for dir in [
        "text/french/game",
        "text/english/game",
        "text/english/dialog",
    ] {
        for e in game
            .vfs()
            .list(dir)
            .into_iter()
            .filter(|e| e.name.ends_with(".msg"))
        {
            files.push(format!("{dir}/{}", e.name));
        }
    }
    assert!(files.len() > 30, "{} fichiers", files.len());
    let mut failures = Vec::new();
    for f in &files {
        let Ok(data) = game.read(f) else { continue };
        let before = MsgFile::parse(&data);
        let Some((&id, entry)) = before.entries.iter().next() else {
            continue;
        };
        let text = format!("{}€é\nfin", entry.text);
        let out = patch(
            &data,
            &[MsgEdit::Set {
                id,
                audio: entry.audio.clone(),
                text: text.clone(),
            }],
        );
        let after = MsgFile::parse(&out);
        let mut expected = before.clone();
        expected.entries.insert(
            id,
            fo_core::msg::MsgEntry {
                audio: entry.audio.clone(),
                text,
            },
        );
        if after != expected {
            failures.push(format!(
                "{f} : entrées différentes après modification de {id}"
            ));
        }
        // Sans modification, ou en supprimant puis réécrivant : le fichier d'origine est reproduit tel quel.
        if patch(&data, &[]) != data {
            failures.push(format!("{f} : patch vide différent"));
        }
        let back = patch(
            &out,
            &[MsgEdit::Set {
                id,
                audio: entry.audio.clone(),
                text: entry.text.clone(),
            }],
        );
        if MsgFile::parse(&back) != before {
            failures.push(format!("{f} : remise à l'état d'origine"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} échecs sur {} fichiers :\n{}",
        failures.len(),
        files.len(),
        failures
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Les cartes se regroupent par dizaine d'identifiant de `worldmap.msg` (Fallout 2) : Arroyo, La Fosse, Klamath…
#[test]
fn maps_are_grouped_by_worldmap_msg_tens() {
    let Some(game) = unpacked() else { return };
    let maps = game.list_maps();
    let group = |f: &str| maps.iter().find(|m| m.file == f).and_then(|m| m.group);
    let (village, bridge, temple) = (group("arvillag"), group("arbridge"), group("arcaves"));
    assert!(
        village.is_some() && village == bridge && village == temple,
        "Arroyo : {village:?} {bridge:?} {temple:?}"
    );
    assert_ne!(
        group("denbus1"),
        village,
        "une autre ville est un autre groupe"
    );
    assert_eq!(group("denbus1"), group("denbus2"));
    assert_eq!(group("denbus1"), group("denres1"));
    let name = |f: &str| {
        maps.iter()
            .find(|m| m.file == f)
            .and_then(|m| m.group_name.clone())
    };
    assert!(name("arvillag").is_some_and(|n| n.contains("Arroyo")));
    assert!(
        maps.iter()
            .find(|m| m.file == "arvillag")
            .and_then(|m| m.place.clone())
            .is_some()
    );
    // Les cartes d'une même zone du monde (identifiants 460 à 469 : « Désert ») sont groupées entre elles.
    assert!(group("desert1").is_some());
}

/// Fallout 1 : les cartes de même description (Junktown) forment un groupe.
#[test]
fn fallout1_maps_with_the_same_description_are_grouped() {
    let Some(root) = dir(
        "FO1_GAME_DIR",
        r"D:\Torrent\Down\Fallout Setup\Fallout1-extract",
    ) else {
        return;
    };
    let Ok(game) = GameData::open(root) else {
        return;
    };
    let maps = game.list_maps();
    let g = |f: &str| maps.iter().find(|m| m.file == f).and_then(|m| m.group);
    assert!(
        g("junkent").is_some() && g("junkent") == g("junkcsno") && g("junkent") == g("junkkill")
    );
    assert_ne!(g("junkent"), g("hotel"));
}

/// L'accessibilité sur de vraies cartes : les sorties d'un village du jeu s'atteignent à pied depuis le départ du joueur, et le
/// parcours ne déborde pas dans le vide (un enclos fermé reste fermé).
#[test]
fn reach_on_real_maps() {
    use fo_core::check::{IssueKind, check_map};
    use fo_core::reach::{reachable, walk_map};
    let Some(game) = unpacked() else { return };
    let game = std::sync::Arc::new(game);
    let art = fo_core::ArtLibrary::new(game.clone());
    let mut total = 0;
    for name in [
        "arvillag", "klamath", "den1", "modmain", "vault13", "redding", "gecko1",
    ] {
        let Ok(bytes) = game.read_map_bytes(name) else {
            continue;
        };
        let map = read_map(&bytes, &*game).unwrap();
        let e = map.header.player_elevation.max(0) as usize;
        let walk = walk_map(&map, e, &game);
        let seen = reachable(&walk.blocked, map.header.player_tile.max(0) as u32);
        let blocked = walk.blocked.iter().filter(|b| **b).count();
        let reached = seen.iter().filter(|b| **b).count();
        let unreachable = check_map(&map, &game, &art)
            .into_iter()
            .filter(|i| i.kind == IssueKind::Unreachable)
            .collect::<Vec<_>>();
        eprintln!(
            "{name}: {blocked} hex bloqués, {reached} atteints, inaccessibles : {:?}",
            unreachable
                .iter()
                .map(|i| (i.hex, i.detail.clone()))
                .collect::<Vec<_>>()
        );
        total += unreachable.len();
        if name == "arvillag" {
            assert!(reached > 1000, "le village est ouvert : {reached}");
            assert!(blocked > 50 && blocked < 5000, "{blocked}");
        }
    }
    eprintln!("total inaccessibles : {total}");
}

/// Un village généré a des bâtiments, des routes qui relient chaque porte, des sorties atteignables, et le vérificateur n'y trouve
/// rien d'anormal (ni objet hors grille, ni prototype inconnu, ni porte ou sortie inaccessible).
#[test]
fn generated_village_is_coherent() {
    use fo_core::check::{IssueKind, check_map};
    use fo_core::procgen::{Params, Side};
    use fo_core::room::{WallStats, plan_room};
    use fo_core::tile_stats::{TileStats, families};
    use fo_core::village::{GenOptions, generate, learn_palette};
    let Some(game) = unpacked() else { return };
    let game = std::sync::Arc::new(game);
    let art = fo_core::ArtLibrary::new(game.clone());
    let (mut walls, mut floor, mut roof) = (
        WallStats::default(),
        TileStats::default(),
        TileStats::default(),
    );
    for f in game.list_maps().iter().map(|m| m.file.clone()) {
        if let Ok(b) = game.read_map_bytes(&f)
            && let Ok(m) = read_map(&b, &*game)
        {
            walls.add_map(&m);
            floor.add_map(&m, fo_core::map::TileLayer::Floor);
            roof.add_map(&m, fo_core::map::TileLayer::Roof);
        }
    }
    let names = game.list("art/walls/walls.lst");
    let model = walls
        .sets(&names)
        .into_iter()
        .filter(|s| !s.label.starts_with("block"))
        .map(|s| {
            let members = s.members.clone();
            walls.model(&|f| members.contains(&f)).strong_only()
        })
        .find(|m| {
            [(100, 50), (100, 51), (101, 50), (101, 51)]
                .iter()
                .all(|&(r, c)| !plan_room(m, r * 200 + c, 8, 8).short)
        })
        .expect("un style de murs utilisable");
    let tiles = game.list("art/tiles/tiles.lst");
    let fams = families(&tiles);
    let pick = |prefix: &str| {
        fams.iter()
            .find(|(k, _)| k.to_lowercase().starts_with(prefix))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| fams.values().next().unwrap().clone())
    };
    let reference = read_map(&game.read_map_bytes("arvillag").unwrap(), &*game).unwrap();
    let palette = learn_palette(&reference, &game, 14);
    assert!(palette.len() >= 4, "palette : {palette:?}");
    let opts = GenOptions {
        params: Params {
            seed: 7,
            buildings: 6,
            exits: vec![Side::South, Side::North],
            ..Params::default()
        },
        ground: pick("tp"),
        road: pick("road"),
        floor: pick("hola"),
        roof: pick("hola"),
        road_width: 2,
        palette,
        windows: 2,
        furniture: 4,
        lights: true,
        floor_stats: Some(std::sync::Arc::new(floor)),
        roof_stats: Some(std::sync::Arc::new(roof)),
    };
    let mut map = fo_core::Map::new_empty("genvill", 1);
    let report = generate(&mut map, &game, &model, &opts);
    eprintln!("{report:?} ; objets : {}", map.objects[0].len());
    assert!(report.buildings >= 3, "{report:?}");
    assert_eq!(report.doors_unlinked, 0, "{report:?}");
    assert_eq!(report.unreachable, 0, "{report:?}");
    assert!(
        report.exits >= 10 && report.blockers > 100 && report.decor > 30,
        "{report:?}"
    );
    let issues = check_map(&map, &game, &art);
    let bad: Vec<_> = issues
        .iter()
        .filter(|i| {
            matches!(
                i.kind,
                IssueKind::OutOfGrid
                    | IssueKind::UnknownPrototype
                    | IssueKind::BadStart
                    | IssueKind::Unreachable
            )
        })
        .collect();
    assert!(bad.is_empty(), "{bad:?}");
    // Reproductible, et une autre graine donne une autre carte.
    let mut again = fo_core::Map::new_empty("genvill", 1);
    generate(&mut again, &game, &model, &opts);
    assert_eq!(again.objects[0].len(), map.objects[0].len());
    let mut other = fo_core::Map::new_empty("genvill", 1);
    generate(
        &mut other,
        &game,
        &model,
        &GenOptions {
            params: Params {
                seed: 8,
                ..opts.params.clone()
            },
            ..opts.clone()
        },
    );
    assert_ne!(
        other.objects[0]
            .iter()
            .map(|o| o.tile())
            .collect::<Vec<_>>(),
        map.objects[0].iter().map(|o| o.tile()).collect::<Vec<_>>()
    );
}

#[test]
#[ignore]
fn diag_wall_axes() {
    use fo_core::room::WallStats;
    let Some(game) = unpacked() else { return };
    let game = std::sync::Arc::new(game);
    let art = fo_core::ArtLibrary::new(game.clone());
    let mut walls = WallStats::default();
    for f in game.list_maps().iter().map(|m| m.file.clone()) {
        if let Ok(b) = game.read_map_bytes(&f)
            && let Ok(m) = read_map(&b, &*game)
        {
            walls.add_map(&m);
        }
    }
    let names = game.list("art/walls/walls.lst");
    for s in walls.sets(&names) {
        let members = s.members.clone();
        let mut model = walls.model(&|f| members.contains(&f)).strong_only();
        model.set_widths(&|f| {
            art.object_sprite(fo_core::Fid(f), 0, 0)
                .map(|(_, sp)| sp.width as u32)
        });
        for (cc, rr) in [(50u32, 100u32), (51, 100), (50, 101), (51, 101)] {
            let plan = fo_core::room::plan_room_with(
                &model,
                rr * 200 + cc,
                8,
                8,
                &fo_core::room::DecorOptions {
                    front: true,
                    ..Default::default()
                },
            );
            let mut line = format!(
                "{} @({},{}) short={} :",
                s.label,
                cc % 2,
                rr % 2,
                plan.short
            );
            for &(h, fid) in plan.walls.iter() {
                let w = art
                    .object_sprite(fo_core::Fid(fid), 0, 0)
                    .map_or(0, |(_, sp)| sp.width);
                let (c, r) = fo_core::geom::hex_col_row(h);
                line += &format!(
                    " [{},{} {} w{}]",
                    c - cc,
                    r - rr,
                    names
                        .get((fid & 0xFFFFFF) as usize)
                        .map(|n| n.trim_end_matches(".frm"))
                        .unwrap_or("?"),
                    w
                );
            }
            eprintln!("{line}");
        }
    }
}

#[test]
#[ignore]
fn diag_buildings() {
    let Some(game) = unpacked() else { return };
    let mut total = 0;
    for f in game.list_maps().iter().map(|m| m.file.clone()) {
        if let Ok(b) = game.read_map_bytes(&f)
            && let Ok(m) = read_map(&b, &game)
        {
            let found = fo_core::buildings::detect(&f, &m);
            total += found.len();
            if !found.is_empty() {
                eprintln!(
                    "{f}: {}",
                    found
                        .iter()
                        .map(|b| format!(
                            "{}w {}x{} o{} f{} r{}",
                            b.walls,
                            b.cols,
                            b.rows,
                            b.pattern.variants[0].objects.len(),
                            b.pattern.variants[0].floor.len(),
                            b.pattern.variants[0].roof.len()
                        ))
                        .collect::<Vec<_>>()
                        .join(" | ")
                );
            }
        }
    }
    eprintln!("TOTAL {total}");
}

#[test]
#[ignore]
fn diag_real_vs_generated() {
    use fo_core::room::{DecorOptions, WallStats, plan_room_with};
    let Some(game) = unpacked() else { return };
    let names = game.list("art/walls/walls.lst");
    let mut walls = WallStats::default();
    let mut maps = Vec::new();
    for f in game.list_maps().iter().map(|m| m.file.clone()) {
        if let Ok(b) = game.read_map_bytes(&f)
            && let Ok(m) = read_map(&b, &game)
        {
            walls.add_map(&m);
            maps.push((f, m));
        }
    }
    let wanted = std::env::var("DIAG_STYLE").unwrap_or_else(|_| "enclav".into());
    let set = walls
        .sets(&names)
        .into_iter()
        .find(|s| s.label.to_lowercase().starts_with(&wanted))
        .expect("style");
    let model = walls.model(&|f| set.members.contains(&f)).strong_only();
    let nm = |fid: u32| {
        names
            .get((fid & 0xFFFFFF) as usize)
            .map(|n| {
                n.trim_end_matches(".FRM")
                    .trim_end_matches(".frm")
                    .to_string()
            })
            .unwrap_or_default()
    };
    // Réel : première construction dont les murs sont du style.
    'outer: for (f, m) in &maps {
        for b in fo_core::buildings::detect(f, m) {
            let ws: Vec<_> = m.objects[b.elevation]
                .iter()
                .filter(|o| {
                    o.object_type() == Some(fo_core::ObjectType::Wall)
                        && set.members.contains(&o.fid())
                })
                .collect();
            if ws.len() < 20 || b.walls > 60 {
                continue;
            }
            let (mut c0, mut r0) = (u32::MAX, u32::MAX);
            for o in &ws {
                let (c, r) = fo_core::geom::hex_col_row(o.tile() as u32);
                c0 = c0.min(c);
                r0 = r0.min(r);
            }
            let mut v: Vec<(u32, u32, String, i32)> = ws
                .iter()
                .map(|o| {
                    let (c, r) = fo_core::geom::hex_col_row(o.tile() as u32);
                    (r - r0, c - c0, nm(o.fid()), o.orientation())
                })
                .collect();
            v.sort();
            eprintln!(
                "REEL {f} {}x{} : {}",
                b.cols,
                b.rows,
                v.iter()
                    .map(|(r, c, n, o)| format!("({c},{r}){n}/{o}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            break 'outer;
        }
    }
    for (cc, rr) in [(50u32, 100u32), (51, 100)] {
        let plan = plan_room_with(
            &model,
            rr * 200 + cc,
            8,
            8,
            &DecorOptions {
                front: true,
                ..Default::default()
            },
        );
        let mut v: Vec<(u32, u32, String)> = plan
            .walls
            .iter()
            .map(|&(h, fid)| {
                let (c, r) = fo_core::geom::hex_col_row(h);
                (r - rr, c - cc, nm(fid))
            })
            .collect();
        v.sort();
        eprintln!(
            "GEN @{} : {}",
            cc % 2,
            v.iter()
                .map(|(r, c, n)| format!("({c},{r}){n}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
}

#[test]
#[ignore]
fn diag_wall_coverage() {
    use fo_core::room::{DecorOptions, WallStats, plan_room_with};
    let Some(game) = unpacked() else { return };
    let game = std::sync::Arc::new(game);
    let art = fo_core::ArtLibrary::new(game.clone());
    let names = game.list("art/walls/walls.lst");
    let mut walls = WallStats::default();
    for f in game.list_maps().iter().map(|m| m.file.clone()) {
        if let Ok(b) = game.read_map_bytes(&f)
            && let Ok(m) = read_map(&b, &*game)
        {
            walls.add_map(&m);
        }
    }
    let vis = |fid: u32| -> Option<u32> {
        let (_, sp) = art.object_sprite(fo_core::Fid(fid), 0, 0)?;
        let opaque = sp.rgba.chunks_exact(4).filter(|p| p[3] > 0).count() as u32;
        let w = sp.width as usize;
        let rows = (0..sp.height as usize)
            .filter(|&y| {
                sp.rgba[y * w * 4..(y + 1) * w * 4]
                    .chunks_exact(4)
                    .any(|p| p[3] > 0)
            })
            .count() as u32;
        (rows > 0).then(|| opaque / rows)
    };
    for s in walls.sets(&names) {
        if s.label.starts_with("block") {
            continue;
        }
        let members = s.members.clone();
        let mut model = walls.model(&|f| members.contains(&f)).strong_only();
        model.set_widths(&vis);
        let mut report = Vec::new();
        if ![
            "dv1004", "ENCLAV06", "adb1027", "ca059", "nec096", "ware011", "SHIP02", "temple02",
            "adw1007", "black03",
        ]
        .contains(&s.label.as_str())
        {
            continue;
        }
        for (cc, rr, cols, rows) in (6u32..=12).flat_map(|c| {
            (5u32..=11).flat_map(move |r| {
                [
                    (50u32, 100u32, c, r),
                    (51, 100, c, r),
                    (50, 101, c, r),
                    (51, 101, c, r),
                ]
            })
        }) {
            let plan = plan_room_with(
                &model,
                rr * 200 + cc,
                rows,
                cols,
                &DecorOptions {
                    front: true,
                    ..Default::default()
                },
            );
            let (mut top, mut left, mut bottom, mut right) = (0u32, 0u32, 0u32, 0u32);
            for &(h, fid) in &plan.walls {
                let (c, r) = fo_core::geom::hex_col_row(h);
                let w = vis(fid).unwrap_or(0);
                let (dc, dr) = (c - cc, r - rr);
                if dr == 0 {
                    top += w
                } else if dr == rows {
                    bottom += w
                } else if dc == 0 {
                    left += w
                } else if dc == cols - 1 {
                    right += w
                }
            }
            let pct = |v: u32, t: u32| v * 100 / t;
            let (ct, cl) = (cols * 24, rows * 16);
            let worst = [
                pct(top, ct),
                pct(left, cl) * 100 / 76,
                pct(bottom, ct),
                pct(right, cl) * 100 / 76,
            ]
            .into_iter()
            .min()
            .unwrap();
            if worst < 55 {
                report.push(format!(
                    "{cols}x{rows}@{}{}: top{} left{} bottom{} right{}",
                    cc % 2,
                    rr % 2,
                    pct(top, ct),
                    pct(left, cl),
                    pct(bottom, ct),
                    pct(right, cl)
                ));
            }
        }
        eprintln!("COVER {} short={} | {}", s.label, false, report.join(" | "));
    }
}

#[test]
#[ignore]
fn diag_village_walls() {
    use fo_core::procgen::{Params, Rng, Side, plan_sites};
    use fo_core::room::{DecorOptions, WallStats, plan_building};
    let Some(game) = unpacked() else { return };
    let game = std::sync::Arc::new(game);
    let art = fo_core::ArtLibrary::new(game.clone());
    let names = game.list("art/walls/walls.lst");
    let mut walls = WallStats::default();
    for f in game.list_maps().iter().map(|m| m.file.clone()) {
        if let Ok(b) = game.read_map_bytes(&f)
            && let Ok(m) = read_map(&b, &*game)
        {
            walls.add_map(&m);
        }
    }
    let vis = |fid: u32| -> Option<u32> {
        let (_, sp) = art.object_sprite(fo_core::Fid(fid), 0, 0)?;
        let opaque = sp.rgba.chunks_exact(4).filter(|p| p[3] > 0).count() as u32;
        let w = sp.width as usize;
        let rows = (0..sp.height as usize)
            .filter(|&y| {
                sp.rgba[y * w * 4..(y + 1) * w * 4]
                    .chunks_exact(4)
                    .any(|p| p[3] > 0)
            })
            .count() as u32;
        (rows > 0).then(|| opaque / rows)
    };
    let style = std::env::var("DIAG_STYLE").unwrap_or_else(|_| "dv1004".into());
    let set = walls
        .sets(&names)
        .into_iter()
        .find(|s| s.label == style)
        .expect("style");
    let mut model = walls.model(&|f| set.members.contains(&f)).strong_only();
    model.set_widths(&vis);
    let seed: u64 = std::env::var("DIAG_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(21);
    let params = Params {
        seed,
        buildings: 8,
        exits: vec![Side::South, Side::North],
        ..Params::default()
    };
    let sites = plan_sites(&params);
    let mut rng = Rng::new(params.seed ^ 0xC0FF_EE11);
    for (i, b) in sites.buildings.iter().enumerate() {
        let s = rng.roll() | 1;
        let opts = DecorOptions {
            door: true,
            lights: true,
            windows: 2,
            furniture: 6,
            seed: s,
            vary_sizes: false,
            ..DecorOptions::default()
        };
        let plan = plan_building(&model, b.corner, b.nx, b.ny, b.rows, b.cols, &opts);
        let mut by_row: std::collections::BTreeMap<u32, Vec<(u32, u32)>> = Default::default();
        let mut by_col: std::collections::BTreeMap<u32, Vec<(u32, u32)>> = Default::default();
        for &(h, fid) in &plan.walls {
            let (c, r) = fo_core::geom::hex_col_row(h);
            let w = vis(fid).unwrap_or(0);
            by_row.entry(r).or_default().push((c, w));
            by_col.entry(c).or_default().push((r, w));
        }
        let mut bad = Vec::new();
        for (r, v) in &by_row {
            if v.len() >= 4 {
                let (lo, hi) = (
                    v.iter().map(|p| p.0).min().unwrap(),
                    v.iter().map(|p| p.0).max().unwrap(),
                );
                let cov = v.iter().map(|p| p.1).sum::<u32>() * 100 / (24 * (hi - lo + 1));
                if cov < 60 {
                    bad.push(format!(
                        "ligne r{r} cols{lo}-{hi} couv{cov}% pièces{}",
                        v.len()
                    ));
                }
            }
        }
        for (c, v) in &by_col {
            if v.len() >= 4 {
                let (lo, hi) = (
                    v.iter().map(|p| p.0).min().unwrap(),
                    v.iter().map(|p| p.0).max().unwrap(),
                );
                let cov = v.iter().map(|p| p.1).sum::<u32>() * 100 / (16 * (hi - lo + 1));
                if cov < 45 {
                    bad.push(format!(
                        "colonne c{c} rows{lo}-{hi} couv{cov}% pièces{}",
                        v.len()
                    ));
                }
            }
        }
        eprintln!(
            "BAT {i} corner{:?} {}x{} rooms{}x{} : {}",
            fo_core::geom::hex_col_row(b.corner),
            b.cols,
            b.rows,
            b.nx,
            b.ny,
            if bad.is_empty() {
                "ok".into()
            } else {
                bad.join(" ; ")
            }
        );
    }
}

#[test]
#[ignore]
fn diag_village_objects() {
    use fo_core::procgen::{Params, Side, plan_sites};
    use fo_core::room::WallStats;
    use fo_core::tile_stats::{TileStats, families};
    use fo_core::village::{GenOptions, generate, learn_palette};
    let Some(game) = unpacked() else { return };
    let game = std::sync::Arc::new(game);
    let (mut walls, mut floor, mut roof) = (
        WallStats::default(),
        TileStats::default(),
        TileStats::default(),
    );
    for f in game.list_maps().iter().map(|m| m.file.clone()) {
        if let Ok(b) = game.read_map_bytes(&f)
            && let Ok(m) = read_map(&b, &*game)
        {
            walls.add_map(&m);
            floor.add_map(&m, fo_core::map::TileLayer::Floor);
            roof.add_map(&m, fo_core::map::TileLayer::Roof);
        }
    }
    let names = game.list("art/walls/walls.lst");
    let style = std::env::var("DIAG_STYLE").unwrap_or_else(|_| "dv1004".into());
    let set = walls
        .sets(&names)
        .into_iter()
        .find(|s| s.label == style)
        .expect("style");
    let model = walls.model(&|f| set.members.contains(&f)).strong_only();
    let tiles = game.list("art/tiles/tiles.lst");
    let fams = families(&tiles);
    let pick = |prefix: &str| {
        fams.iter()
            .find(|(k, _)| k.to_lowercase().starts_with(prefix))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| fams.values().next().unwrap().clone())
    };
    let reference = read_map(&game.read_map_bytes("arvillag").unwrap(), &*game).unwrap();
    let palette = learn_palette(&reference, &game, 14);
    let seed: u64 = std::env::var("DIAG_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(21);
    let params = Params {
        seed,
        buildings: 8,
        exits: vec![Side::South, Side::North],
        ..Params::default()
    };
    let sites = plan_sites(&params);
    let opts = GenOptions {
        params,
        ground: pick("tp"),
        road: pick("road"),
        floor: pick("hola"),
        roof: Vec::new(),
        road_width: 2,
        palette,
        windows: 2,
        furniture: 6,
        lights: true,
        floor_stats: Some(std::sync::Arc::new(floor)),
        roof_stats: None,
    };
    let mut map = fo_core::Map::new_empty("diag", 1);
    generate(&mut map, &game, &model, &opts);
    for (i, b) in sites.buildings.iter().enumerate() {
        let a = &b.reserved;
        let mut kinds: std::collections::BTreeMap<String, usize> = Default::default();
        for o in &map.objects[0] {
            if o.tile() < 0 {
                continue;
            }
            let (c, r) = fo_core::geom::hex_col_row(o.tile() as u32);
            if c + 2 >= a.c0 && c <= a.c1 + 2 && r + 2 >= a.r0 && r <= a.r1 + 2 {
                *kinds
                    .entry(format!(
                        "{:?} sub{} pid{:08X}",
                        o.object_type(),
                        o.subtype,
                        o.pid()
                    ))
                    .or_default() += 1;
            }
        }
        eprintln!(
            "OBJ {i} {:?}-{:?}: {}",
            (a.c0, a.r0),
            (a.c1, a.r1),
            kinds
                .iter()
                .map(|(k, n)| format!("{k}×{n}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

#[test]
#[ignore]
fn diag_road_tiles() {
    use fo_core::tile_stats::{TileStats, families};
    let Some(game) = unpacked() else { return };
    let tiles = game.list("art/tiles/tiles.lst");
    let fams = families(&tiles);
    eprintln!(
        "FAMILLES {}",
        fams.keys().cloned().collect::<Vec<_>>().join(" ")
    );
    for (k, v) in &fams {
        if k.to_lowercase().starts_with("road") || k.to_lowercase().starts_with("str") {
            eprintln!(
                "FAM {k}: {} tuiles {:?}",
                v.len(),
                v.iter()
                    .map(|&i| tiles[i as usize].clone())
                    .collect::<Vec<_>>()
            );
        }
    }
    let mut floor = TileStats::default();
    let mut count = 0;
    for f in game.list_maps().iter().map(|m| m.file.clone()) {
        if let Ok(b) = game.read_map_bytes(&f)
            && let Ok(m) = read_map(&b, &game)
        {
            floor.add_map(&m, fo_core::map::TileLayer::Floor);
            count += 1;
        }
    }
    eprintln!("cartes {count}");
}

#[test]
#[ignore]
fn diag_road_sheet() {
    use fo_core::tile_stats::families;
    let Some(game) = unpacked() else { return };
    let game = std::sync::Arc::new(game);
    let art = fo_core::ArtLibrary::new(game.clone());
    let tiles = game.list("art/tiles/tiles.lst");
    let fams = families(&tiles);
    let out = std::env::var("DIAG_OUT").unwrap();
    let fam = std::env::var("DIAG_FAM").unwrap_or_else(|_| "road".into());
    for &id in &fams[&fam] {
        if let Some(sp) = art.tile_sprite(id as usize) {
            let mut data = Vec::new();
            data.extend_from_slice(&sp.width.to_le_bytes());
            data.extend_from_slice(&sp.height.to_le_bytes());
            data.extend_from_slice(&sp.rgba);
            std::fs::write(format!("{out}/{fam}_{id}.bin"), data).unwrap();
        }
    }
}

#[test]
#[ignore]
fn diag_road_dirt() {
    use fo_core::tile_stats::families;
    let Some(game) = unpacked() else { return };
    let game = std::sync::Arc::new(game);
    let art = fo_core::ArtLibrary::new(game.clone());
    let tiles = game.list("art/tiles/tiles.lst");
    let fams = families(&tiles);
    for &id in &fams["road"] {
        let Some(sp) = art.tile_sprite(id as usize) else {
            continue;
        };
        let (w, h) = (sp.width as usize, sp.height as usize);
        let mut q = [(0u32, 0u32); 4]; // NW, NE, SW, SE : (terre, total)
        for y in 0..h {
            for x in 0..w {
                let p = &sp.rgba[(y * w + x) * 4..(y * w + x) * 4 + 4];
                if p[3] == 0 {
                    continue;
                }
                let k = usize::from(x >= w / 2) + 2 * usize::from(y >= h / 2);
                q[k].1 += 1;
                if p[0] > 62 && p[0] as i32 - p[2] as i32 > 14 {
                    q[k].0 += 1;
                }
            }
        }
        eprintln!(
            "DIRT {id} {}x{} NW{:3} NE{:3} SW{:3} SE{:3}",
            w,
            h,
            q[0].0 * 100 / q[0].1.max(1),
            q[1].0 * 100 / q[1].1.max(1),
            q[2].0 * 100 / q[2].1.max(1),
            q[3].0 * 100 / q[3].1.max(1)
        );
    }
}

#[test]
#[ignore]
fn diag_animated_scenery() {
    let Some(game) = unpacked() else { return };
    let game = std::sync::Arc::new(game);
    let art = fo_core::ArtLibrary::new(game.clone());
    let mut kinds: std::collections::BTreeMap<String, (usize, u16)> = Default::default();
    for f in [
        "arvillag", "klamall", "denbus1", "newr1", "redding", "modmain", "desert1", "vctyctyd",
    ] {
        let Ok(b) = game.read_map_bytes(f) else {
            continue;
        };
        let Ok(m) = read_map(&b, &*game) else {
            continue;
        };
        for o in &m.objects[0] {
            if o.object_type() != Some(fo_core::ObjectType::Scenery)
                || o.subtype != fo_core::formats::scenery_kind::GENERIC
            {
                continue;
            }
            let orient = o.orientation().max(0) as u32;
            if let Some((path, _)) = art.object_sprite(fo_core::Fid(o.fid()), orient, 0)
                && let Some(frm) = art.frm(&path)
            {
                let dir = if frm.directions.len() > 1 {
                    orient as usize
                } else {
                    0
                };
                let n = frm.directions.get(dir).map_or(0, |d| d.frames.len());
                if n > 1 {
                    kinds.entry(path).or_insert((0, frm.fps)).0 += 1;
                }
            }
        }
    }
    for (p, (n, fps)) in kinds.iter().take(25) {
        eprintln!("ANIM {p} x{n} fps{fps}");
    }
    eprintln!("ANIM kinds {}", kinds.len());
}
