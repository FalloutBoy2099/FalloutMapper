//! Structure d'un fichier `.int` : code de démarrage, table des procédures, noms, chaînes, puis corps des procédures.

use crate::opcode::{self, O_FLOATOP, O_INTOP, O_STRINGOP};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("script tronqué (à l'octet {0})")]
    Truncated(usize),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, ScriptError>;

pub mod proc_flags {
    pub const TIMED: u32 = 0x01;
    pub const CONDITIONAL: u32 = 0x02;
    pub const IMPORT: u32 = 0x04;
    pub const EXPORT: u32 = 0x08;
    pub const CRITICAL: u32 = 0x10;
}

#[derive(Debug, Clone)]
pub struct Procedure {
    pub name: String,
    /// Décalage du nom dans la table des noms.
    pub name_offset: u32,
    pub flags: u32,
    pub time: u32,
    pub condition_offset: u32,
    /// Adresse du corps dans le fichier (0 si importée ou non implémentée).
    pub body_offset: u32,
    pub arg_count: u32,
    pub body_size: u32,
}

impl Procedure {
    pub fn is_implemented(&self) -> bool {
        self.flags & proc_flags::IMPORT == 0 && self.body_offset != 0 && self.body_size > 0
    }

    /// Libellé court des drapeaux (`timed`, `export`…).
    pub fn flag_labels(&self) -> Vec<&'static str> {
        [
            (proc_flags::TIMED, "timed"),
            (proc_flags::CONDITIONAL, "conditional"),
            (proc_flags::IMPORT, "import"),
            (proc_flags::EXPORT, "export"),
            (proc_flags::CRITICAL, "critical"),
        ]
        .iter()
        .filter(|(f, _)| self.flags & f != 0)
        .map(|(_, l)| *l)
        .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instruction {
    /// Adresse dans le fichier.
    pub offset: u32,
    pub code: u16,
    pub arg: u32,
}

impl Instruction {
    pub fn size(&self) -> u32 {
        if opcode::has_argument(self.code) {
            6
        } else {
            2
        }
    }
}

/// Table de chaînes (noms ou textes) adressées par leur décalage dans la table.
#[derive(Debug, Default, Clone)]
pub struct StringTable {
    by_offset: HashMap<u32, String>,
    order: Vec<u32>,
}

impl StringTable {
    pub fn get(&self, offset: u32) -> Option<&str> {
        self.by_offset.get(&offset).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (u32, &str)> {
        self.order.iter().map(|&o| (o, self.by_offset[&o].as_str()))
    }
}

/// Variable globale du script (déclarée hors des procédures).
#[derive(Debug, Clone)]
pub struct GlobalVar {
    pub name: String,
    /// Valeur initiale (opcode de chargement d'une constante).
    pub init: Instruction,
}

/// Variable exportée vers les autres scripts, avec sa valeur initiale.
#[derive(Debug, Clone)]
pub struct ExportedVar {
    pub name: String,
    pub value: Instruction,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub globals: Vec<GlobalVar>,
    pub exported_vars: Vec<ExportedVar>,
    pub procedures: Vec<Procedure>,
    pub bodies: Vec<Vec<Instruction>>,
    pub names: StringTable,
    pub strings: StringTable,
    pub file_size: usize,
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.data.len())
            .ok_or(ScriptError::Truncated(self.pos))?;
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.bytes(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.bytes(4)?.try_into().unwrap()))
    }
}

fn read_instruction(c: &mut Cursor) -> Result<Instruction> {
    let offset = c.pos as u32;
    let code = c.u16()?;
    if !opcode::is_valid(code) {
        return Err(ScriptError::Invalid(format!(
            "opcode invalide 0x{code:04X} à 0x{offset:08X}"
        )));
    }
    let arg = if opcode::has_argument(code) {
        c.u32()?
    } else {
        0
    };
    Ok(Instruction { offset, code, arg })
}

fn read_table(c: &mut Cursor) -> Result<StringTable> {
    let mut t = StringTable::default();
    let length = c.u32()?;
    if length == 0xFFFF_FFFF {
        return Ok(t);
    }
    let mut total = 0u32;
    while total < length {
        let len = c.u16()? as usize;
        if len < 2 || len % 2 != 0 {
            return Err(ScriptError::Invalid("longueur de chaîne invalide".into()));
        }
        let bytes = c.bytes(len)?;
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(len);
        let text: String = bytes[..end].iter().map(|&b| b as char).collect();
        let offset = total + 6;
        t.by_offset.insert(offset, text);
        t.order.push(offset);
        total += 2 + len as u32;
    }
    if c.u32()? != 0xFFFF_FFFF {
        return Err(ScriptError::Invalid(
            "terminateur de table de chaînes invalide".into(),
        ));
    }
    Ok(t)
}

/// Nombre d'opcodes du code de démarrage commun à tous les scripts compilés.
const STARTUP_OPCODES: usize = 17;

/// Lit le code situé entre les tables et le premier corps : variables exportées, valeurs initiales des globales.
/// Tolérant : un en-tête inattendu donne des listes vides plutôt qu'un échec.
fn read_header_tail(
    c: &mut Cursor,
    first_body: usize,
    names: &StringTable,
    proc_name_offsets: &[u32],
) -> (Vec<GlobalVar>, Vec<ExportedVar>) {
    const O_SET_GLOBAL: u16 = 0x802C;
    const O_STORE_EXTERNAL: u16 = 0x8015;
    const O_EXPORT_VAR: u16 = 0x8016;
    const O_EXPORT_PROC: u16 = 0x8017;

    let mut tail = Vec::new();
    match read_instruction(c) {
        Ok(i) if i.code == O_SET_GLOBAL => {}
        _ => return (Vec::new(), Vec::new()),
    }
    while c.pos < first_body {
        match read_instruction(c) {
            Ok(i) => tail.push(i),
            Err(_) => return (Vec::new(), Vec::new()),
        }
    }

    let is_value = |i: &Instruction| matches!(i.code, O_STRINGOP | O_FLOATOP | O_INTOP);
    let mut pending: Vec<Instruction> = Vec::new();
    let mut exported = Vec::new();
    let mut used_names: Vec<u32> = proc_name_offsets.to_vec();
    for ins in tail {
        match ins.code {
            O_EXPORT_VAR => {
                if let Some(n) = pending.pop().filter(|n| n.code == O_STRINGOP) {
                    used_names.push(n.arg);
                }
            }
            O_STORE_EXTERNAL => {
                if pending.len() >= 2 {
                    let name = pending.pop().unwrap();
                    let value = pending.pop().unwrap();
                    if name.code == O_STRINGOP && is_value(&value) {
                        used_names.push(name.arg);
                        exported.push(ExportedVar {
                            name: names.get(name.arg).unwrap_or("?").to_string(),
                            value,
                        });
                    }
                }
            }
            O_EXPORT_PROC => {
                pending.pop();
                pending.pop();
            }
            _ if is_value(&ins) => pending.push(ins),
            _ => {}
        }
    }

    // Les noms restants sont ceux des variables globales, dans l'ordre, s'ils sont aussi nombreux qu'elles.
    let remaining: Vec<&str> = names
        .iter()
        .filter(|(o, _)| !used_names.contains(o))
        .map(|(_, n)| n)
        .collect();
    let by_name = remaining.len() == pending.len();
    let globals = pending
        .into_iter()
        .enumerate()
        .map(|(i, init)| GlobalVar {
            name: if by_name {
                remaining[i].to_string()
            } else {
                format!("GVar{i}")
            },
            init,
        })
        .collect();
    (globals, exported)
}

impl Program {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut c = Cursor { data, pos: 0 };
        for _ in 0..STARTUP_OPCODES {
            read_instruction(&mut c)?;
        }

        let count = c.u32()? as usize;
        if count > data.len() / 24 {
            return Err(ScriptError::Invalid("nombre de procédures invalide".into()));
        }
        let mut raw = Vec::with_capacity(count);
        for _ in 0..count {
            let name = c.u32()?;
            let flags = c.u32()?;
            let time = c.u32()?;
            let condition_offset = c.u32()?;
            let body_offset = c.u32()?;
            let arg_count = c.u32()?;
            raw.push((name, flags, time, condition_offset, body_offset, arg_count));
        }
        let names = read_table(&mut c)?;
        let strings = read_table(&mut c)?;

        // Taille d'un corps : jusqu'au corps suivant (ou la fin du fichier).
        let mut starts: Vec<u32> = raw
            .iter()
            .filter(|r| r.1 & proc_flags::IMPORT == 0 && r.4 != 0 && (r.4 as usize) < data.len())
            .map(|r| r.4)
            .collect();
        starts.sort_unstable();
        starts.dedup();
        let size_of = |start: u32| -> u32 {
            let next = starts
                .iter()
                .copied()
                .find(|&s| s > start)
                .unwrap_or(data.len() as u32);
            next - start
        };

        let first_body = starts.first().copied().unwrap_or(data.len() as u32) as usize;
        let (globals, exported_vars) = read_header_tail(
            &mut c,
            first_body,
            &names,
            &raw.iter().map(|r| r.0).collect::<Vec<_>>(),
        );

        let mut procedures = Vec::with_capacity(count);
        let mut bodies = Vec::with_capacity(count);
        for (name, flags, time, condition_offset, body_offset, arg_count) in raw {
            let implemented = flags & proc_flags::IMPORT == 0
                && body_offset != 0
                && (body_offset as usize) < data.len();
            let body_size = if implemented { size_of(body_offset) } else { 0 };
            let mut body = Vec::new();
            if implemented {
                let mut bc = Cursor {
                    data,
                    pos: body_offset as usize,
                };
                let end = (body_offset + body_size) as usize;
                while bc.pos < end {
                    body.push(read_instruction(&mut bc)?);
                }
            }
            procedures.push(Procedure {
                name_offset: name,
                name: names
                    .get(name)
                    .map_or_else(|| format!("proc_{}", procedures.len()), str::to_string),
                flags,
                time,
                condition_offset,
                body_offset,
                arg_count,
                body_size,
            });
            bodies.push(body);
        }
        Ok(Self {
            globals,
            exported_vars,
            procedures,
            bodies,
            names,
            strings,
            file_size: data.len(),
        })
    }

    pub fn procedure_by_name(&self, name: &str) -> Option<usize> {
        self.procedures
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Texte d'un opérande chaîne : table des textes, sinon table des noms.
    pub fn string_arg(&self, offset: u32) -> Option<&str> {
        self.strings.get(offset).or_else(|| self.names.get(offset))
    }

    pub fn is_int_literal(ins: &Instruction) -> bool {
        ins.code == O_INTOP
    }
}
