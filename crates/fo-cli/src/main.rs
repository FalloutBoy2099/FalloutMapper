use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use fo_core::map::{SCRIPT_TYPE_NAMES, read_map};
use fo_core::{ArtLibrary, GameData};
use fo_scene::{LayerSet, build_scene, render};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;

mod tools;

#[derive(Parser)]
#[command(name = "fomap", version, about = "Outils pour les cartes de Fallout 2")]
struct Cli {
    /// Dossier d'installation du jeu (celui qui contient master.dat / fallout2.cfg).
    #[arg(short, long, env = "FO2_GAME_DIR", global = true)]
    game: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Liste les cartes de l'installation.
    List,
    /// Affiche l'en-tête et le contenu d'une carte.
    Info { map: String },
    /// Affiche le pseudo-source (ou le listing avec --asm) d'un script compilé.
    Script {
        /// Nom du script, avec ou sans `.int`.
        name: String,
        /// Procédure à afficher (toutes par défaut).
        #[arg(short, long)]
        procedure: Option<String>,
        /// Listing brut des opcodes.
        #[arg(long)]
        asm: bool,
    },
    /// Analyse une source SSL (sans la compiler) : procédures, appels, variables, messages, objets créés.
    Ssl {
        /// Fichier `.ssl`.
        file: PathBuf,
        /// Dossier d'inclusion (répétable) ; celui du fichier est toujours cherché.
        #[arg(short = 'I', long = "include")]
        include: Vec<PathBuf>,
        /// Affiche seulement cette procédure, avec ses appelants.
        #[arg(short, long)]
        procedure: Option<String>,
        /// Cherche qui lit ou écrit cette variable globale.
        #[arg(long)]
        gvar: Option<i64>,
    },
    /// Statistiques d'une carte : objets par type, décors et sols fréquents, sorties, scripts.
    Analyze {
        map: String,
        /// Sortie JSON.
        #[arg(long)]
        json: bool,
    },
    /// Graphe des liens entre cartes (sorties) : texte, JSON ou DOT (Graphviz).
    Graph {
        #[arg(long)]
        dot: bool,
        #[arg(long)]
        json: bool,
    },
    /// Sorties et portes atteignables à pied depuis le point de départ.
    Reach {
        map: String,
        #[arg(short, long, default_value_t = 0)]
        elevation: usize,
        #[arg(long)]
        json: bool,
    },
    /// Contrôle d'une carte (objets hors grille, prototypes inconnus, sorties cassées…).
    Check {
        map: String,
        #[arg(long)]
        json: bool,
    },
    /// Inspecte un FRM : cadence, directions, images (`scenery/fan.frm` ou `art/scenery/fan.frm`).
    Frm {
        path: String,
        #[arg(long)]
        json: bool,
    },
    /// Envoie une commande au jeu en cours (canal de contrôle) : ping, state, load_map <carte>, goto <hex>, run_script <nom>, get_gvar <n>, set_gvar <n> <v>.
    Game {
        /// Port du canal ; par défaut `control_port` de `fallout2.cfg` du dossier du jeu.
        #[arg(long)]
        port: Option<u16>,
        /// La commande et ses arguments.
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Variables globales du jeu (vault13.gam), avec filtre sur le nom ou le commentaire.
    Gvars {
        filter: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Rend une élévation d'une carte en PNG.
    Render {
        map: String,
        /// Fichier PNG de sortie.
        #[arg(short, long, default_value = "map.png")]
        output: PathBuf,
        #[arg(short, long, default_value_t = 0)]
        elevation: usize,
        /// Réduction de l'image (2 = moitié de la taille).
        #[arg(short, long, default_value_t = 2)]
        downscale: u32,
        /// Zone du monde à rendre : x,y,largeur,hauteur (par défaut tout le contenu).
        #[arg(long, value_delimiter = ',')]
        crop: Option<Vec<i32>>,
        /// Instant (ms) de l'animation de palette à figer ; sans cette option, palette au repos.
        #[arg(long)]
        anim_ms: Option<u64>,
        /// Ne dessine que cette couche : floor, roof, item, critter, scenery, wall.
        #[arg(long)]
        only: Option<String>,
        /// Ne dessine pas les toits.
        #[arg(long)]
        no_roofs: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Command::Ssl {
        file,
        include,
        procedure,
        gvar,
    } = &cli.command
    {
        return ssl(file, include, procedure.as_deref(), *gvar);
    }
    let dir = cli
        .game
        .context("indiquez l'installation du jeu avec --game ou FO2_GAME_DIR")?;
    let game =
        Arc::new(GameData::open(&dir).with_context(|| format!("ouverture de {}", dir.display()))?);

    match cli.command {
        Command::List => {
            for m in game.list_maps() {
                println!(
                    "{:>4}  {:<10} {}",
                    m.number.map_or("-".to_string(), |n| n.to_string()),
                    m.file,
                    m.display.unwrap_or_default()
                );
            }
        }
        Command::Ssl { .. } => unreachable!("traité avant l'ouverture du jeu"),
        Command::Analyze { map, json } => tools::analyze(&game, &map, json)?,
        Command::Graph { dot, json } => tools::graph(&game, dot, json)?,
        Command::Reach {
            map,
            elevation,
            json,
        } => tools::reach(&game, &map, elevation, json)?,
        Command::Check { map, json } => tools::check(&game, &map, json)?,
        Command::Frm { path, json } => tools::frm(&game, &path, json)?,
        Command::Game { port, command } => {
            let settings = fo_core::game_link::link_settings(&game.root);
            let port = port
                .or(settings.as_ref().map(|s| s.0))
                .context("aucun port : donnez --port ou réglez control_port dans fallout2.cfg")?;
            let token = settings.and_then(|s| s.1);
            let mut link = fo_core::game_link::GameLink::connect(port, token.as_deref())?;
            println!("{}", link.command(&command.join(" "))?);
        }
        Command::Gvars { filter, json } => tools::gvars(&game, filter.as_deref(), json)?,
        Command::Info { map } => info(&game, &map)?,
        Command::Script {
            name,
            procedure,
            asm,
        } => {
            let file = format!("scripts/{}.int", name.trim_end_matches(".int"));
            let program = fo_script::Program::parse(&game.read(&file)?)?;
            let msgs = fo_script::GameMessages::new(&game, file.trim_start_matches("scripts/"));
            match &procedure {
                None if !asm => print!("{}", program.pseudo_source_with(&msgs)),
                _ => {
                    let indexes: Vec<usize> = match &procedure {
                        Some(p) => vec![
                            program
                                .procedure_by_name(p)
                                .with_context(|| format!("procédure {p} introuvable"))?,
                        ],
                        None => (0..program.procedures.len()).collect(),
                    };
                    for i in indexes {
                        if asm {
                            println!(
                                "; {}
{}",
                                program.procedures[i].name,
                                program.disassemble(i)
                            );
                        } else {
                            println!("{}", program.procedure_source_with(i, &msgs));
                        }
                    }
                }
            }
        }
        Command::Render {
            map,
            output,
            elevation,
            downscale,
            crop,
            only,
            anim_ms,
            no_roofs,
        } => {
            let art = ArtLibrary::new(game.clone());
            let bytes = game.read_map_bytes(&map)?;
            let parsed = read_map(&bytes, &*game)?;
            if elevation >= 3 || !parsed.header.has_elevation(elevation) {
                bail!("la carte n'a pas d'élévation {elevation}");
            }
            let scene = build_scene(&parsed, elevation, &art);
            let mut layers = LayerSet::all();
            if let Some(name) = &only {
                let layer = match name.as_str() {
                    "floor" => fo_scene::Layer::Floor,
                    "roof" => fo_scene::Layer::Roof,
                    "item" => fo_scene::Layer::Item,
                    "critter" => fo_scene::Layer::Critter,
                    "scenery" => fo_scene::Layer::Scenery,
                    "wall" => fo_scene::Layer::Wall,
                    other => bail!("couche inconnue : {other}"),
                };
                layers = LayerSet::from_bits(0);
                layers.set(layer, true);
            }
            if no_roofs {
                layers.set(fo_scene::Layer::Roof, false);
            }
            let region = match crop.as_deref() {
                Some(&[x, y, w, h]) => (x, y, w, h),
                _ => scene.bounds,
            };
            let cycle = fo_core::PaletteAnim {
                speed_factor: game.config.cycle_speed_factor,
            };
            let palette = game.palette();
            let img = render(
                &scene,
                layers,
                region,
                downscale,
                anim_ms.map(|t| (&cycle, &*palette, t)),
            );
            let file = File::create(&output).with_context(|| output.display().to_string())?;
            let mut enc = png::Encoder::new(BufWriter::new(file), img.width, img.height);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header()?.write_image_data(&img.data)?;
            println!(
                "{} : {}×{} px, {} sprites, {} repères",
                output.display(),
                img.width,
                img.height,
                scene.items.len(),
                scene.markers.len()
            );
            for (fid, n) in scene.missing_art.iter().take(10) {
                println!("art introuvable : fid {fid:08X} (×{n})");
            }
        }
    }
    Ok(())
}

fn info(game: &GameData, name: &str) -> Result<()> {
    let bytes = game.read_map_bytes(name)?;
    let map = read_map(&bytes, game)?;
    let h = &map.header;
    println!("Carte {} (fichier {})", h.name(), name);
    println!(
        "  version {}  numéro {}  obscurité {}  script {}",
        h.version, h.map_number, h.darkness, h.script_id
    );
    println!(
        "  départ : hex {} élév. {} orientation {}",
        h.player_tile, h.player_elevation, h.player_orientation
    );
    println!(
        "  variables : {} globales, {} locales",
        map.globals.len(),
        map.locals.len()
    );
    for e in 0..3 {
        match &map.tiles[e] {
            Some(t) => {
                let floors = t.iter().filter(|t| t.floor != 1).count();
                let roofs = t.iter().filter(|t| t.roof != 1).count();
                println!(
                    "  élévation {e} : {floors} sols, {roofs} toits, {} objets",
                    map.objects[e].len()
                );
            }
            None => println!("  élévation {e} : absente"),
        }
    }
    for (name, list) in SCRIPT_TYPE_NAMES.iter().zip(&map.scripts) {
        println!("  scripts {name} : {}", list.count);
    }
    let exits = map
        .objects
        .iter()
        .flatten()
        .filter_map(|o| o.exit_destination())
        .count();
    println!(
        "  sorties de carte : {exits}  objets (inventaires inclus) : {}",
        map.total_object_count()
    );
    Ok(())
}

fn ssl(
    file: &std::path::Path,
    include: &[PathBuf],
    procedure: Option<&str>,
    gvar: Option<i64>,
) -> Result<()> {
    use fo_script::ssl::{DirProvider, analyze};
    let bytes = std::fs::read(file).with_context(|| format!("lecture de {}", file.display()))?;
    let text: String = bytes.iter().map(|&b| b as char).collect();
    let mut dirs: Vec<PathBuf> = file.parent().map(|p| p.to_path_buf()).into_iter().collect();
    dirs.extend(include.iter().cloned());
    let facts = analyze(&text, &DirProvider { dirs });
    let list = |v: &std::collections::BTreeSet<i64>| {
        v.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
    };
    if !facts.missing_includes.is_empty() {
        eprintln!(
            "inclusions introuvables : {}",
            facts.missing_includes.join(", ")
        );
    }
    if let Some(n) = gvar {
        for (name, write) in facts.users_of_gvar(n) {
            println!("{name}  {}", if write { "écrit" } else { "lit" });
        }
        return Ok(());
    }
    for p in facts
        .procedures
        .iter()
        .filter(|p| procedure.is_none_or(|n| n == p.name))
    {
        println!(
            "{} ({} arg) lignes {}-{}",
            p.name, p.arg_count, p.line, p.end_line
        );
        let callers = facts.callers_of(&p.name);
        if !callers.is_empty() {
            println!("  appelée par : {}", callers.join(", "));
        }
        if !p.calls.is_empty() {
            println!(
                "  appelle     : {}",
                p.calls.iter().cloned().collect::<Vec<_>>().join(", ")
            );
        }
        for (label, set) in [
            ("gvar lit", &p.gvar_read),
            ("gvar écrit", &p.gvar_write),
            ("mvar lit", &p.mvar_read),
            ("mvar écrit", &p.mvar_write),
            ("lvar lit", &p.lvar_read),
            ("lvar écrit", &p.lvar_write),
            ("messages", &p.messages),
            ("crée (pid)", &p.created),
        ] {
            if !set.is_empty() {
                println!("  {label:<11} : {}", list(set));
            }
        }
        if p.dynamic > 0 {
            println!("  références non littérales : {}", p.dynamic);
        }
    }
    if procedure.is_none() {
        let unused = facts.unused();
        if !unused.is_empty() {
            println!(
                "
procédures jamais appelées : {}",
                unused.join(", ")
            );
        }
        if !facts.undefined.is_empty() {
            println!("déclarées sans corps : {}", facts.undefined.join(", "));
        }
    }
    Ok(())
}
