//! Petits formats : listes `.lst`, palette `.pal`, prototypes `.pro` (en-tête), `maps.txt`.

use crate::bytes::Reader;
use crate::error::{Error, Result};

/// Liste `.lst` : une entrée par ligne, le nom étant le premier mot (le reste est un commentaire). Un nom de script est coupé
/// juste après « .int ».
pub fn parse_lst(data: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(data);
    let mut names: Vec<String> = text
        .split('\n')
        .map(|l| {
            l.trim()
                .split([' ', '\t', ';', ','])
                .next()
                .unwrap_or("")
                .to_string()
        })
        .map(|n| match n.to_ascii_lowercase().find(".int") {
            // Le `scripts.lst` d'origine de Fallout 1 a des lignes abîmées (« Brohd12.intried ; … ») : le nom s'arrête à « .int ».
            Some(pos) if n.len() > pos + 4 => n[..pos + 4].to_string(),
            _ => n,
        })
        .collect();
    // Un fichier qui se termine par un saut de ligne n'a pas de dernière entrée.
    if names.last().is_some_and(|n| n.is_empty()) {
        names.pop();
    }
    names
}

/// Palette 256 couleurs de `color.pal` (valeurs 6 bits converties en 8 bits). L'indice 0 est transparent.
#[derive(Debug, Clone)]
pub struct Palette {
    pub colors: [[u8; 4]; 256],
    source_tables: Vec<u8>,
    tables: std::sync::OnceLock<crate::palette_blend::PaletteTables>,
}

impl Palette {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 768 {
            return Err(Error::format("palette trop courte (768 octets attendus)"));
        }
        let mut colors = [[0u8; 4]; 256];
        for (i, c) in colors.iter_mut().enumerate() {
            let conv = |v: u8| ((v.min(63) as u32 * 255 + 31) / 63) as u8;
            *c = [
                conv(data[i * 3]),
                conv(data[i * 3 + 1]),
                conv(data[i * 3 + 2]),
                if i == 0 { 0 } else { 255 },
            ];
        }
        Ok(Self {
            colors,
            source_tables: data.to_vec(),
            tables: std::sync::OnceLock::new(),
        })
    }

    /// Palette grise de secours quand `color.pal` est absent.
    pub fn grayscale() -> Self {
        let mut colors = [[0u8; 4]; 256];
        for (i, c) in colors.iter_mut().enumerate() {
            *c = [i as u8, i as u8, i as u8, if i == 0 { 0 } else { 255 }];
        }
        Self {
            colors,
            source_tables: Vec::new(),
            tables: std::sync::OnceLock::new(),
        }
    }

    pub fn tables(&self) -> &crate::palette_blend::PaletteTables {
        self.tables
            .get_or_init(|| crate::palette_blend::PaletteTables::new(self, &self.source_tables))
    }
}

/// Famille d'objet (octet de poids fort d'un PID ou d'un FID).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectType {
    Item = 0,
    Critter = 1,
    Scenery = 2,
    Wall = 3,
    Tile = 4,
    Misc = 5,
}

impl ObjectType {
    pub fn from_index(i: u32) -> Option<Self> {
        Some(match i {
            0 => Self::Item,
            1 => Self::Critter,
            2 => Self::Scenery,
            3 => Self::Wall,
            4 => Self::Tile,
            5 => Self::Misc,
            _ => return None,
        })
    }

    /// Nom du dossier dans `proto/` et `art/`.
    pub fn dir_name(self) -> &'static str {
        match self {
            Self::Item => "items",
            Self::Critter => "critters",
            Self::Scenery => "scenery",
            Self::Wall => "walls",
            Self::Tile => "tiles",
            Self::Misc => "misc",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Item => "Objet",
            Self::Critter => "Créature",
            Self::Scenery => "Décor",
            Self::Wall => "Mur",
            Self::Tile => "Tuile",
            Self::Misc => "Divers",
        }
    }
}

/// En-tête commun d'un prototype (`.pro`) et sous-type des items et décors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtoHeader {
    pub pid: u32,
    /// Numéro du nom dans le fichier de messages `pro_*.msg`.
    pub msg_id: i32,
    pub fid: u32,
    pub light_distance: i32,
    pub light_intensity: i32,
    pub flags: u32,
    /// Sous-type (item ou décor), -1 pour les autres familles.
    pub subtype: i32,
    /// Script du prototype (identifiant de script), -1 sans script. Items, créatures et décors.
    pub script_id: i32,
    /// Données de créature (créatures seulement).
    pub critter: Option<CritterInfo>,
}

/// Ce qu'une créature de la carte reprend de son prototype.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CritterInfo {
    pub ai_packet: i32,
    pub team: i32,
    /// Points de vie maximaux (base + formule de Fallout 2 : 15 + FOR + 2 × END).
    pub hit_points: i32,
    /// Points d'action (base + 5 + AGI / 2).
    pub action_points: i32,
}

impl ProtoHeader {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let pid = r.u32()?;
        let ty = ObjectType::from_index(pid >> 24)
            .ok_or_else(|| Error::format("prototype : type d'objet inconnu"))?;
        let msg_id = r.i32()?;
        let fid = r.u32()?;
        let light_distance = r.i32()?;
        let light_intensity = r.i32()?;
        let flags = r.u32()?;
        // Pour un item ou un décor : drapeaux étendus, script, puis le sous-type.
        let subtype = match ty {
            ObjectType::Item | ObjectType::Scenery => Reader::at(data, 32).i32()?,
            _ => -1,
        };
        let script_id = match ty {
            ObjectType::Item | ObjectType::Scenery | ObjectType::Critter => {
                Reader::at(data, 28).i32().unwrap_or(-1)
            }
            _ => -1,
        };
        let critter = (ty == ObjectType::Critter).then(|| {
            let at = |i: usize| Reader::at(data, 48 + i * 4).i32().unwrap_or(0);
            CritterInfo {
                ai_packet: Reader::at(data, 36).i32().unwrap_or(0),
                team: Reader::at(data, 40).i32().unwrap_or(0),
                hit_points: at(7) + 15 + at(0) + 2 * at(2),
                action_points: at(8) + 5 + at(5) / 2,
            }
        });
        Ok(Self {
            pid,
            msg_id,
            fid,
            light_distance,
            light_intensity,
            flags,
            subtype,
            script_id,
            critter,
        })
    }

    pub fn object_type(&self) -> Option<ObjectType> {
        ObjectType::from_index(self.pid >> 24)
    }
}

/// Sous-type d'un objet (item ou décor), lu dans son prototype.
pub fn proto_subtype(data: &[u8]) -> Result<(ObjectType, i32)> {
    let h = ProtoHeader::parse(data)?;
    let ty = h
        .object_type()
        .ok_or_else(|| Error::format("prototype : type d'objet inconnu"))?;
    Ok((ty, h.subtype))
}

/// Sous-types d'items (valeurs stockées dans les prototypes).
pub mod item_kind {
    pub const ARMOR: i32 = 0;
    pub const CONTAINER: i32 = 1;
    pub const DRUG: i32 = 2;
    pub const WEAPON: i32 = 3;
    pub const AMMO: i32 = 4;
    pub const MISC: i32 = 5;
    pub const KEY: i32 = 6;
}

/// Sous-types de décors.
pub mod scenery_kind {
    pub const DOOR: i32 = 0;
    pub const STAIRS: i32 = 1;
    pub const ELEVATOR: i32 = 2;
    pub const LADDER_BOTTOM: i32 = 3;
    pub const LADDER_TOP: i32 = 4;
    pub const GENERIC: i32 = 5;
}

/// Une carte déclarée dans `data/maps.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapDef {
    pub number: u32,
    /// Nom du fichier sans extension (`map_name`).
    pub file: String,
    /// Nom affiché dans le jeu (`lookup_name`).
    pub display: Option<String>,
}

pub fn parse_maps_txt(data: &[u8]) -> Vec<MapDef> {
    let text = String::from_utf8_lossy(data);
    let mut out = Vec::new();
    let mut cur: Option<(u32, Option<String>, Option<String>)> = None;
    let close = |cur: &mut Option<(u32, Option<String>, Option<String>)>, out: &mut Vec<MapDef>| {
        if let Some((number, file, display)) = cur.take()
            && let Some(file) = file
        {
            out.push(MapDef {
                number,
                file,
                display,
            });
        }
    };
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if let Some(rest) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            close(&mut cur, &mut out);
            if let Some(n) = rest
                .trim()
                .strip_prefix("Map ")
                .or_else(|| rest.trim().strip_prefix("map "))
            {
                cur = n.trim().parse().ok().map(|n| (n, None, None));
            }
        } else if let Some((k, v)) = line.split_once('=')
            && let Some(c) = cur.as_mut()
        {
            let v = v.trim();
            match k.trim().to_ascii_lowercase().as_str() {
                "map_name" if !v.is_empty() => c.1 = Some(v.to_ascii_lowercase()),
                "lookup_name" if !v.is_empty() => c.2 = Some(v.to_string()),
                _ => {}
            }
        }
    }
    close(&mut cur, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::parse_lst;

    #[test]
    fn lst_names_stop_after_dot_int_even_on_damaged_lines() {
        let lst = parse_lst(
            b"obj_dude.int   ; player
Brohd12.intried ; Map script
Talus.intn DeppD; Talus
art.frm
",
        );
        assert_eq!(lst, ["obj_dude.int", "Brohd12.int", "Talus.int", "art.frm"]);
    }

    use super::*;

    #[test]
    fn lst_takes_first_word() {
        let v = parse_lst(b"a.frm\r\nb.frm ; note\r\nobj,1,2\r\n");
        assert_eq!(v, ["a.frm", "b.frm", "obj"]);
    }

    #[test]
    fn maps_txt_sections() {
        let t = b"; c\n[Map 000]\nlookup_name=Desert 1\nmap_name=desert1\n[Map 001]\nmap_name=Foo ; x\n";
        let m = parse_maps_txt(t);
        assert_eq!(m.len(), 2);
        assert_eq!(
            m[0],
            MapDef {
                number: 0,
                file: "desert1".into(),
                display: Some("Desert 1".into())
            }
        );
        assert_eq!(m[1].file, "foo");
    }

    #[test]
    fn palette_scales_six_bit() {
        let mut d = vec![0u8; 768];
        d[3] = 63;
        let p = Palette::parse(&d).unwrap();
        assert_eq!(p.colors[1][0], 255);
        assert_eq!(p.colors[0][3], 0);
    }
}
