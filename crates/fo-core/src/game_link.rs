//! Canal de contrôle du jeu (Fallout 2 CE « Fusion »).
//!
//! Le moteur écoute sur `127.0.0.1:<control_port>` quand `[debug] control_port` de `fallout2.cfg` n'est pas nul
//! (voir `docs/CONTROL.md` du moteur). Une commande par ligne, une réponse par ligne : `ok …`, `busy …` ou `error …`.
//!
//! Les commandes s'exécutent sur la boucle du jeu : le jeu doit être sur l'écran de la carte. Ailleurs (menu,
//! chargement, inventaire, dialogue…) il répond tout de suite `busy <où il est>` au lieu de faire attendre ;
//! `command_wait` et `wait_ready` patientent côté jeu jusqu'à ce qu'il soit prêt. `ping` et `status` répondent toujours.

use crate::error::{Error, Result};
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Une commande déjà prise par le jeu peut durer (le chargement d'une grande carte) : le moteur abandonne à 60 s.
const REPLY_TIMEOUT: Duration = Duration::from_secs(70);

/// État du jeu rendu par `status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameStatus {
    /// Le jeu est sur l'écran de la carte et exécute les commandes.
    pub ready: bool,
    /// `map`, `menu`, `inventory`, `dialog`, `worldmap`, `barter`, `loot`, `pipboy`, `options`…
    pub screen: String,
    /// Une partie est chargée.
    pub loaded: bool,
}

/// Port et jeton du canal de contrôle, lus dans `fallout2.cfg` du dossier du jeu (`None` si le canal est coupé).
pub fn link_settings(root: &Path) -> Option<(u16, Option<String>)> {
    let text = std::fs::read(root.join("fallout2.cfg")).ok()?;
    parse_link_settings(&String::from_utf8_lossy(&text))
}

fn parse_link_settings(text: &str) -> Option<(u16, Option<String>)> {
    let mut in_debug = false;
    let mut port = 0u16;
    let mut token = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_debug = section.eq_ignore_ascii_case("debug");
        } else if in_debug && let Some((key, value)) = line.split_once('=') {
            let value = value.trim();
            match key.trim().to_ascii_lowercase().as_str() {
                "control_port" => port = value.parse().unwrap_or(0),
                "control_token" if !value.is_empty() => token = Some(value.to_string()),
                _ => {}
            }
        }
    }
    (port != 0).then_some((port, token))
}

/// Une connexion au jeu en cours.
#[derive(Debug)]
pub struct GameLink {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
}

impl GameLink {
    /// Se connecte au jeu (ce même ordinateur) et s'authentifie si un jeton est donné.
    pub fn connect(port: u16, token: Option<&str>) -> Result<Self> {
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let name = || PathBuf::from(address.to_string());
        let stream = TcpStream::connect_timeout(&address, Duration::from_secs(2)).map_err(|e| Error::io(name(), e))?;
        stream.set_read_timeout(Some(REPLY_TIMEOUT)).map_err(|e| Error::io(name(), e))?;
        let writer = stream.try_clone().map_err(|e| Error::io(name(), e))?;
        let mut link = GameLink { reader: BufReader::new(stream), writer };
        if let Some(token) = token {
            link.command(&format!("auth {token}"))?;
        }
        Ok(link)
    }

    /// Envoie une commande et rend la suite de `ok …`. Une réponse `busy …` (le jeu n'est pas sur la carte) et une
    /// réponse `error …` deviennent des erreurs ; la première commence par « jeu occupé ».
    pub fn command(&mut self, line: &str) -> Result<String> {
        if line.contains(['\n', '\r']) {
            return Err(Error::format("une commande tient sur une seule ligne"));
        }
        let name = || PathBuf::from("jeu");
        self.writer.write_all(format!("{line}\n").as_bytes()).map_err(|e| Error::io(name(), e))?;
        let mut reply = String::new();
        let read = self.reader.read_line(&mut reply).map_err(|e| Error::io(name(), e))?;
        if read == 0 {
            return Err(Error::format("le jeu a fermé la connexion"));
        }
        let reply = reply.trim_end();
        if let Some(rest) = reply.strip_prefix("ok") {
            Ok(rest.trim_start().to_string())
        } else if let Some(rest) = reply.strip_prefix("busy") {
            Err(Error::format(format!("jeu occupé : {}", rest.trim())))
        } else {
            Err(Error::format(reply.strip_prefix("error").unwrap_or(reply).trim().to_string()))
        }
    }

    /// Comme `command`, mais si le jeu n'est pas sur l'écran de la carte il patiente jusqu'à `seconds` secondes
    /// (côté jeu) avant de répondre « occupé ».
    pub fn command_wait(&mut self, line: &str, seconds: u32) -> Result<String> {
        let timeout = REPLY_TIMEOUT + Duration::from_secs(u64::from(seconds));
        let name = || PathBuf::from("jeu");
        self.reader.get_ref().set_read_timeout(Some(timeout)).map_err(|e| Error::io(name(), e))?;
        let result = self.command(&format!("wait {seconds} {line}"));
        self.reader.get_ref().set_read_timeout(Some(REPLY_TIMEOUT)).map_err(|e| Error::io(name(), e))?;
        result
    }

    /// Attend (au plus `seconds` secondes) que le jeu soit sur l'écran de la carte, par exemple juste après son
    /// lancement.
    pub fn wait_ready(&mut self, seconds: u32) -> Result<String> {
        self.command_wait("state", seconds)
    }

    /// Où en est le jeu, sans attendre.
    pub fn status(&mut self) -> Result<GameStatus> {
        let reply = self.command("status")?;
        let field = |key: &str| reply.split_whitespace().find_map(|part| part.strip_prefix(key)?.strip_prefix('=')).map(str::to_string);
        Ok(GameStatus {
            ready: field("ready").as_deref() == Some("1"),
            screen: field("screen").unwrap_or_default(),
            loaded: field("loaded").as_deref() == Some("1"),
        })
    }

    pub fn ping(&mut self) -> Result<()> {
        self.command("ping").map(|_| ())
    }

    /// Carte, élévation, hexagone et orientation du joueur.
    pub fn state(&mut self) -> Result<String> {
        self.command("state")
    }

    /// Charge une carte (nom sans extension) ; le joueur est placé sur `position` (hexagone, élévation) si donnée.
    /// Avec `fresh`, le fichier `.map` est lu même si le jeu garde un `.sav` de la carte : c'est ce que veut un
    /// éditeur après avoir enregistré.
    pub fn load_map(&mut self, map: &str, position: Option<(i32, u8)>, fresh: bool) -> Result<String> {
        let mut line = match position {
            Some((tile, elevation)) => format!("load_map {map} {tile} {elevation}"),
            None => format!("load_map {map}"),
        };
        if fresh {
            line.push_str(" fresh");
        }
        self.command(&line)
    }

    /// Recharge la carte en cours, le joueur restant où il est.
    pub fn reload_map(&mut self, fresh: bool) -> Result<String> {
        self.command(if fresh { "reload_map fresh" } else { "reload_map" })
    }

    pub fn goto(&mut self, tile: i32, elevation: u8) -> Result<String> {
        self.command(&format!("goto {tile} {elevation}"))
    }

    /// Lance le script compilé `scripts\<nom>.int` des fichiers du jeu.
    pub fn run_script(&mut self, name: &str) -> Result<()> {
        self.command(&format!("run_script {name}")).map(|_| ())
    }

    pub fn get_gvar(&mut self, number: i32) -> Result<i32> {
        let value = self.command(&format!("get_gvar {number}"))?;
        value.trim().parse().map_err(|_| Error::format(format!("réponse inattendue : {value}")))
    }

    pub fn set_gvar(&mut self, number: i32, value: i32) -> Result<()> {
        self.command(&format!("set_gvar {number} {value}")).map(|_| ())
    }

    /// Ajoute un objet à l'inventaire du joueur.
    pub fn give_item(&mut self, pid: i32, quantity: u32) -> Result<()> {
        self.command(&format!("give_item {pid} {quantity}")).map(|_| ())
    }

    /// Crée un objet (identifiant de l'objet créé en retour).
    pub fn spawn(&mut self, pid: i32, tile: i32, elevation: u8) -> Result<String> {
        self.command(&format!("spawn {pid} {tile} {elevation}"))
    }

    /// Les objets autour du joueur : `(pid, hexagone)`, 200 au plus.
    pub fn objects(&mut self, radius: u32) -> Result<Vec<(i32, i32)>> {
        let reply = self.command(&format!("objects {radius}"))?;
        Ok(reply.split_whitespace().skip(1).filter_map(|item| item.split_once(':')).filter_map(|(pid, tile)| Some((pid.parse().ok()?, tile.parse().ok()?))).collect())
    }

    /// L'hexagone sous le curseur de la souris dans le jeu.
    pub fn cursor_tile(&mut self) -> Result<i32> {
        let reply = self.command("cursor")?;
        reply
            .split_whitespace()
            .find_map(|part| part.strip_prefix("tile=")?.parse().ok())
            .ok_or_else(|| Error::format(format!("réponse inattendue : {reply}")))
    }

    /// Affiche une ligne dans la boîte de messages du jeu.
    pub fn message(&mut self, text: &str) -> Result<()> {
        self.command(&format!("message {text}")).map(|_| ())
    }

    /// Enregistre une capture `scrNNNNN.png` dans le dossier du jeu.
    pub fn screenshot(&mut self) -> Result<()> {
        self.command("screenshot").map(|_| ())
    }

    /// Ferme le jeu.
    pub fn quit(&mut self) -> Result<()> {
        self.command("quit").map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// Un faux jeu : répond à quelques commandes, est « occupé » pour `state`, refuse le reste.
    fn fake_game(token: Option<&'static str>) -> u16 {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut writer = stream;
            let mut authorized = token.is_none();
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                let command = line.trim().to_string();
                let reply = if let Some(sent) = command.strip_prefix("auth ") {
                    authorized = Some(sent) == token;
                    if authorized { "ok".to_string() } else { "error bad token".to_string() }
                } else if !authorized {
                    "error authenticate first".to_string()
                } else {
                    match command.as_str() {
                        "ping" => "ok pong fusion".to_string(),
                        "state" => "busy the game is on the inventory screen".to_string(),
                        "status" => "ok ready=0 screen=inventory loaded=1".to_string(),
                        "objects 5" => "ok 2 33554499:17510 16777313:17709".to_string(),
                        "cursor" => "ok tile=9933 elevation=0".to_string(),
                        c if c.starts_with("wait ") => "ok map=ARROYO.MAP elevation=0 tile=19190".to_string(),
                        c if c.starts_with("get_gvar") => "ok 7".to_string(),
                        c if c.starts_with("load_map") => format!("ok {c}"),
                        _ => format!("error unknown command {command}"),
                    }
                };
                writer.write_all(format!("{reply}\n").as_bytes()).unwrap();
                line.clear();
            }
        });
        port
    }

    #[test]
    fn talks_to_a_game() {
        let mut link = GameLink::connect(fake_game(None), None).unwrap();
        link.ping().unwrap();
        assert_eq!(link.get_gvar(1).unwrap(), 7);
        let error = link.command("bogus").unwrap_err().to_string();
        assert!(error.contains("unknown command bogus"), "{error}");
    }

    #[test]
    fn tells_busy_apart_from_errors() {
        let mut link = GameLink::connect(fake_game(None), None).unwrap();
        let busy = link.state().unwrap_err().to_string();
        assert!(busy.contains("jeu occupé") && busy.contains("inventory"), "{busy}");
        assert_eq!(link.status().unwrap(), GameStatus { ready: false, screen: "inventory".into(), loaded: true });
        // `wait_ready` sends the wait prefix and gets the answer of the game once it is ready.
        assert!(link.wait_ready(30).unwrap().contains("map=ARROYO.MAP"));
    }

    #[test]
    fn authenticates_with_the_token() {
        let mut link = GameLink::connect(fake_game(Some("secret")), Some("secret")).unwrap();
        link.ping().unwrap();
        assert!(GameLink::connect(fake_game(Some("secret")), Some("wrong")).is_err());
        assert!(GameLink::connect(fake_game(Some("secret")), None).unwrap().ping().is_err());
    }

    #[test]
    fn reads_the_answers_of_the_new_commands() {
        let mut link = GameLink::connect(fake_game(None), None).unwrap();
        assert_eq!(link.objects(5).unwrap(), vec![(33554499, 17510), (16777313, 17709)]);
        assert_eq!(link.cursor_tile().unwrap(), 9933);
        assert!(link.load_map("arvillag", Some((20517, 0)), true).unwrap().ends_with("arvillag 20517 0 fresh"));
        assert!(link.load_map("arvillag", None, false).unwrap().ends_with("load_map arvillag"));
    }

    #[test]
    fn reads_the_settings() {
        let cfg = "[system]\ncontrol_port=1\n[debug]\nmode=log\ncontrol_port=47720\ncontrol_token=abc\n";
        assert_eq!(parse_link_settings(cfg), Some((47720, Some("abc".to_string()))));
        assert_eq!(parse_link_settings("[debug]\ncontrol_port=0\n"), None);
        assert_eq!(parse_link_settings("[debug]\nmode=log\n"), None);
    }

    #[test]
    fn refuses_a_multi_line_command() {
        let mut link = GameLink::connect(fake_game(None), None).unwrap();
        assert!(link.command("ping\nstate").is_err());
    }
}
