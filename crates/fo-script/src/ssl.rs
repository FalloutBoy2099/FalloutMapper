//! Analyse statique d'une source SSL (sans la compiler) : lexeur, préprocesseur (`#include`, `#define`, conditions), repérage des
//! procédures, puis faits par procédure : appels aux autres procédures, variables globales / de carte / locales lues et écrites,
//! messages affichés, objets créés. Le but est de répondre vite à « qui appelle quoi ? » et « où sert cette variable ? »
//! sans passer par le compilateur.
//!
//! Ce n'est pas un analyseur complet du langage : les arguments non littéraux (une variable, une expression) ne sont pas suivis ;
//! leur nombre est seulement compté (`dynamic`).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ident,
    Num,
    Str,
    Punct,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: Kind,
    pub text: String,
    /// Ligne (à partir de 1) du fichier d'où vient le jeton ; pour un jeton issu d'une macro, ligne de l'appel.
    pub line: u32,
    /// 0 : le fichier analysé ; sinon un fichier inclus.
    pub file: u16,
}

/// Fournit le texte d'un fichier inclus (`#include "x.h"`).
pub trait SourceProvider {
    fn read(&self, name: &str) -> Option<String>;
}

/// Cherche les inclusions dans des dossiers (la source elle-même est lue en Latin-1, comme le compilateur).
#[derive(Debug)]
pub struct DirProvider {
    pub dirs: Vec<PathBuf>,
}

impl SourceProvider for DirProvider {
    fn read(&self, name: &str) -> Option<String> {
        let name = name.replace('\\', "/");
        for d in &self.dirs {
            let direct = d.join(&name);
            let path = if direct.is_file() {
                Some(direct)
            } else {
                find_ignore_case(d, &name)
            };
            if let Some(p) = path
                && let Ok(bytes) = std::fs::read(p)
            {
                return Some(bytes.iter().map(|&b| b as char).collect());
            }
        }
        None
    }
}

/// Recherche insensible à la casse (les sources viennent de Windows, les en-têtes sont souvent écrits en minuscules ou en majuscules).
fn find_ignore_case(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut cur = dir.to_path_buf();
    for part in name.split('/').filter(|p| !p.is_empty() && *p != ".") {
        let found = std::fs::read_dir(&cur)
            .ok()?
            .flatten()
            .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?;
        cur = found.path();
    }
    cur.is_file().then_some(cur)
}

/// Fournisseur en mémoire (tests).
#[derive(Default, Debug)]
pub struct MemoryProvider(pub HashMap<String, String>);

impl SourceProvider for MemoryProvider {
    fn read(&self, name: &str) -> Option<String> {
        self.0.get(name).cloned()
    }
}

/// Découpe un texte en jetons ; les commentaires `//` et `/* */` disparaissent.
pub fn lex(text: &str, first_line: u32, file: u16) -> Vec<Token> {
    let b: Vec<char> = text.chars().collect();
    let (mut i, mut line) = (0usize, first_line);
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '/' && b.get(i + 1) == Some(&'/') {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && b.get(i + 1) == Some(&'*') {
            i += 2;
            while i < b.len() && !(b[i] == '*' && b.get(i + 1) == Some(&'/')) {
                if b[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i += 2;
        } else if c == '"' {
            let start = i + 1;
            i += 1;
            while i < b.len() && b[i] != '"' && b[i] != '\n' {
                i += 1;
            }
            out.push(Token {
                kind: Kind::Str,
                text: b[start..i.min(b.len())].iter().collect(),
                line,
                file,
            });
            i += 1;
        } else if c.is_ascii_digit() {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == '.') {
                i += 1;
            }
            out.push(Token {
                kind: Kind::Num,
                text: b[start..i].iter().collect(),
                line,
                file,
            });
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < b.len() && (b[i].is_alphanumeric() || b[i] == '_') {
                i += 1;
            }
            out.push(Token {
                kind: Kind::Ident,
                text: b[start..i].iter().collect(),
                line,
                file,
            });
        } else {
            // Opérateurs à deux caractères utiles pour reconnaître `:=` et les comparaisons.
            let two: String = b[i..(i + 2).min(b.len())].iter().collect();
            if matches!(
                two.as_str(),
                ":=" | "==" | "!=" | "<=" | ">=" | "&&" | "||" | "<<" | ">>" | "+=" | "-="
            ) {
                out.push(Token {
                    kind: Kind::Punct,
                    text: two,
                    line,
                    file,
                });
                i += 2;
            } else {
                out.push(Token {
                    kind: Kind::Punct,
                    text: c.to_string(),
                    line,
                    file,
                });
                i += 1;
            }
        }
    }
    out
}

/// Valeur d'un littéral numérique (décimal, `0x…`).
pub fn number(text: &str) -> Option<i64> {
    let t = text.trim_end_matches(['L', 'l', 'u', 'U']);
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        i64::from_str_radix(h, 16).ok()
    } else {
        t.parse().ok()
    }
}

#[derive(Debug, Clone)]
struct Macro {
    params: Option<Vec<String>>,
    body: Vec<Token>,
}

struct Pre<'a> {
    provider: &'a dyn SourceProvider,
    macros: HashMap<String, Macro>,
    out: Vec<Token>,
    includes: Vec<String>,
    missing: Vec<String>,
    files: u16,
}

impl Pre<'_> {
    fn process(&mut self, text: &str, file: u16, depth: u32) {
        // Lignes logiques : une barre oblique inversée en fin de ligne continue la ligne.
        let mut lines: Vec<(u32, String)> = Vec::new();
        let mut pending: Option<(u32, String)> = None;
        for (n, raw) in text.lines().enumerate() {
            let n = n as u32 + 1;
            let cont = raw.trim_end().ends_with('\\');
            let piece = if cont {
                raw.trim_end().trim_end_matches('\\')
            } else {
                raw
            };
            match pending.take() {
                Some((first, mut acc)) => {
                    acc.push(' ');
                    acc.push_str(piece);
                    if cont {
                        pending = Some((first, acc));
                    } else {
                        lines.push((first, acc));
                    }
                }
                None => {
                    if cont {
                        pending = Some((n, piece.to_string()));
                    } else {
                        lines.push((n, piece.to_string()));
                    }
                }
            }
        }
        if let Some(p) = pending {
            lines.push(p);
        }
        // Pile des conditions : (branche active, une branche a déjà été prise, le parent est actif).
        let mut cond: Vec<(bool, bool, bool)> = Vec::new();
        let active = |c: &Vec<(bool, bool, bool)>| c.last().is_none_or(|x| x.0);
        // Les blocs de texte hors directive sont lexés d'un seul tenant : une macro à arguments peut s'étendre sur plusieurs lignes.
        let mut block = String::new();
        let mut block_line = 1u32;
        let mut block_gap = 0u32;
        let flush = |this: &mut Self, block: &mut String, line: u32| {
            if !block.is_empty() {
                let toks = lex(block, line, file);
                this.out.extend(toks);
                block.clear();
            }
        };
        let mut last_line = 0u32;
        for (n, line) in lines {
            let t = line.trim_start();
            if let Some(rest) = t.strip_prefix('#') {
                let rest = rest.trim_start();
                let (word, args) = rest
                    .split_once(|c: char| c.is_whitespace())
                    .unwrap_or((rest, ""));
                let args = args.trim();
                match word {
                    "ifdef" | "ifndef" | "if" => {
                        let parent = active(&cond);
                        let name = args.split_whitespace().next().unwrap_or("");
                        let value = match word {
                            "ifdef" => self.macros.contains_key(name),
                            "ifndef" => !self.macros.contains_key(name),
                            _ => self.eval_if(args),
                        };
                        cond.push((parent && value, parent && value, parent));
                    }
                    "else" => {
                        if let Some(c) = cond.last_mut() {
                            c.0 = c.2 && !c.1;
                            c.1 = c.1 || c.0;
                        }
                    }
                    "elif" => {
                        let v = self.eval_if(args);
                        if let Some(c) = cond.last_mut() {
                            c.0 = c.2 && !c.1 && v;
                            c.1 = c.1 || c.0;
                        }
                    }
                    "endif" => {
                        cond.pop();
                    }
                    _ if !active(&cond) => {}
                    "define" => {
                        let name_end = args
                            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                            .unwrap_or(args.len());
                        let name = &args[..name_end];
                        if !name.is_empty() {
                            let after = &args[name_end..];
                            // Une macro à paramètres : la parenthèse suit le nom sans espace.
                            let (params, body_text) = match after.strip_prefix('(') {
                                Some(p) => match p.split_once(')') {
                                    Some((ps, body)) => (
                                        Some(
                                            ps.split(',')
                                                .map(|s| s.trim().to_string())
                                                .filter(|s| !s.is_empty())
                                                .collect(),
                                        ),
                                        body,
                                    ),
                                    None => (None, after),
                                },
                                None => (None, after),
                            };
                            self.macros.insert(
                                name.to_string(),
                                Macro {
                                    params,
                                    body: lex(body_text, n, file),
                                },
                            );
                        }
                    }
                    "undef" => {
                        self.macros
                            .remove(args.split_whitespace().next().unwrap_or(""));
                    }
                    "include" => {
                        flush(self, &mut block, block_line);
                        // Le nom est entre guillemets ou chevrons ; ce qui suit (un commentaire) est ignoré.
                        let name = match args.chars().next() {
                            Some(open @ ('"' | '<')) => {
                                let close = if open == '"' { '"' } else { '>' };
                                args[1..].split(close).next().unwrap_or("").trim()
                            }
                            _ => args.split_whitespace().next().unwrap_or(""),
                        };
                        self.includes.push(name.to_string());
                        if depth < 16 {
                            match self.provider.read(name) {
                                Some(text) => {
                                    self.files = self.files.saturating_add(1);
                                    let id = self.files;
                                    self.process(&text, id, depth + 1);
                                }
                                None => self.missing.push(name.to_string()),
                            }
                        }
                    }
                    _ => {}
                }
            } else if active(&cond) {
                if block.is_empty() {
                    block_line = n;
                    block_gap = 0;
                } else {
                    // Les lignes vides ou masquées entre deux lignes du bloc gardent les numéros de ligne justes.
                    for _ in 0..(n - last_line) {
                        block.push('\n');
                    }
                    block_gap += n - last_line;
                }
                block.push_str(&line);
                last_line = n;
            } else {
                flush(self, &mut block, block_line);
            }
        }
        let _ = block_gap;
        flush(self, &mut block, block_line);
    }

    fn eval_if(&self, expr: &str) -> bool {
        let e = expr.trim();
        if let Some(n) = number(e) {
            return n != 0;
        }
        if let Some(inner) = e.strip_prefix("defined") {
            let name = inner.trim().trim_matches(|c| c == '(' || c == ')').trim();
            return self.macros.contains_key(name);
        }
        if let Some(m) = self.macros.get(e) {
            return m
                .body
                .first()
                .and_then(|t| number(&t.text))
                .is_none_or(|n| n != 0);
        }
        true
    }

    /// Étend les macros d'un flux de jetons (récursivement, avec une profondeur limitée).
    fn expand(&self, toks: &[Token], depth: u32, hide: &HashSet<String>) -> Vec<Token> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < toks.len() {
            let t = &toks[i];
            let macro_def = (t.kind == Kind::Ident && depth < 12 && !hide.contains(&t.text))
                .then(|| self.macros.get(&t.text))
                .flatten();
            let Some(m) = macro_def else {
                out.push(t.clone());
                i += 1;
                continue;
            };
            let mut inner_hide = hide.clone();
            inner_hide.insert(t.text.clone());
            let retag = |mut v: Vec<Token>| {
                for x in &mut v {
                    x.line = t.line;
                    x.file = t.file;
                }
                v
            };
            match &m.params {
                None => {
                    out.extend(self.expand(&retag(m.body.clone()), depth + 1, &inner_hide));
                    i += 1;
                }
                Some(params) => {
                    if toks.get(i + 1).is_none_or(|n| n.text != "(") {
                        out.push(t.clone());
                        i += 1;
                        continue;
                    }
                    // Arguments : jusqu'à la parenthèse fermante correspondante, séparés par les virgules de niveau 0.
                    let (mut j, mut level) = (i + 2, 1);
                    let mut args: Vec<Vec<Token>> = vec![Vec::new()];
                    while j < toks.len() {
                        match toks[j].text.as_str() {
                            "(" => level += 1,
                            ")" => {
                                level -= 1;
                                if level == 0 {
                                    break;
                                }
                            }
                            "," if level == 1 => {
                                args.push(Vec::new());
                                j += 1;
                                continue;
                            }
                            _ => {}
                        }
                        if let Some(a) = args.last_mut() {
                            a.push(toks[j].clone());
                        }
                        j += 1;
                    }
                    let mut body = Vec::new();
                    for bt in &m.body {
                        match params.iter().position(|p| *p == bt.text) {
                            Some(k) if bt.kind == Kind::Ident => {
                                body.extend(args.get(k).cloned().unwrap_or_default())
                            }
                            _ => body.push(bt.clone()),
                        }
                    }
                    out.extend(self.expand(&retag(body), depth + 1, &inner_hide));
                    i = j + 1;
                }
            }
        }
        out
    }
}

/// Les faits d'une procédure.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcFacts {
    pub name: String,
    /// 0 : définie dans le fichier analysé ; sinon dans un fichier inclus (les lignes ne valent alors que pour ce fichier).
    pub file: u16,
    /// Ligne de `procedure nom … begin` et ligne du `end` qui la ferme.
    pub line: u32,
    pub end_line: u32,
    pub arg_count: usize,
    pub calls: BTreeSet<String>,
    pub gvar_read: BTreeSet<i64>,
    pub gvar_write: BTreeSet<i64>,
    pub mvar_read: BTreeSet<i64>,
    pub mvar_write: BTreeSet<i64>,
    pub lvar_read: BTreeSet<i64>,
    pub lvar_write: BTreeSet<i64>,
    /// Numéros de message (`mstr`, `Reply`, `NOption`, `message_str`…).
    pub messages: BTreeSet<i64>,
    /// Prototypes créés (`create_object_sid`, `create_object`).
    pub created: BTreeSet<i64>,
    /// Références dont l'argument n'est pas un littéral (non suivies).
    pub dynamic: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ScriptFacts {
    pub procedures: Vec<ProcFacts>,
    /// Procédures déclarées (`procedure x;`) sans corps dans ce fichier.
    pub undefined: Vec<String>,
    pub includes: Vec<String>,
    /// Inclusions introuvables : les macros qu'elles définissent manquent, les numéros peuvent rester non résolus.
    pub missing_includes: Vec<String>,
}

impl ScriptFacts {
    pub fn find(&self, name: &str) -> Option<&ProcFacts> {
        self.procedures.iter().find(|p| p.name == name)
    }

    /// Procédures qui appellent `name`.
    pub fn callers_of(&self, name: &str) -> Vec<&str> {
        self.procedures
            .iter()
            .filter(|p| p.calls.contains(name))
            .map(|p| p.name.as_str())
            .collect()
    }

    /// Procédures jamais appelées et qui ne sont pas des points d'entrée du moteur (`start`, `*_p_proc`).
    pub fn unused(&self) -> Vec<&str> {
        self.procedures
            .iter()
            .filter(|p| !is_entry_point(&p.name) && self.callers_of(&p.name).is_empty())
            .map(|p| p.name.as_str())
            .collect()
    }

    /// Procédures qui lisent ou écrivent la variable globale `n`.
    pub fn users_of_gvar(&self, n: i64) -> Vec<(&str, bool)> {
        let mut out = Vec::new();
        for p in &self.procedures {
            if p.gvar_write.contains(&n) {
                out.push((p.name.as_str(), true));
            } else if p.gvar_read.contains(&n) {
                out.push((p.name.as_str(), false));
            }
        }
        out
    }

    pub fn all_gvars(&self) -> BTreeSet<i64> {
        self.procedures
            .iter()
            .flat_map(|p| p.gvar_read.iter().chain(&p.gvar_write))
            .copied()
            .collect()
    }
}

/// Points d'entrée appelés par le moteur : `start` et les procédures `*_p_proc` (et `*_proc` des dialogues du moteur).
fn is_entry_point(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "start"
        || n.ends_with("_p_proc")
        || matches!(n.as_str(), "talk_p_proc" | "combat" | "critter_p_proc")
}

/// Fonctions dont le premier argument littéral est un numéro de variable globale / de carte / locale (lecture ou écriture).
fn var_kind(name: &str) -> Option<(u8, bool)> {
    Some(match name {
        "global_var" => (0, false),
        "set_global_var" => (0, true),
        "map_var" => (1, false),
        "set_map_var" => (1, true),
        "local_var" => (2, false),
        "set_local_var" => (2, true),
        _ => return None,
    })
}

/// Fonctions dont le premier argument est un numéro de message du script (fichier `.msg` du même nom).
fn is_message_fn(name: &str) -> bool {
    matches!(
        name,
        "mstr"
            | "Reply"
            | "NMessage"
            | "GMessage"
            | "BMessage"
            | "NOption"
            | "GOption"
            | "BOption"
            | "NLowOption"
            | "GLowOption"
            | "BLowOption"
    )
}

fn is_create_fn(name: &str) -> bool {
    matches!(
        name,
        "create_object_sid" | "create_object" | "create_object_sid_"
    )
}

/// Analyse `text` : préprocesse (les inclusions viennent de `provider`), repère les procédures, puis relève leurs faits.
pub fn analyze(text: &str, provider: &dyn SourceProvider) -> ScriptFacts {
    let mut pre = Pre {
        provider,
        macros: HashMap::new(),
        out: Vec::new(),
        includes: Vec::new(),
        missing: Vec::new(),
        files: 0,
    };
    pre.process(text, 0, 0);
    let raw = std::mem::take(&mut pre.out);
    let expanded = pre.expand(&raw, 0, &HashSet::new());
    // Les procédures des en-têtes comptent aussi (le compilateur les inclut dans le script) ; elles portent le numéro de leur fichier.
    let toks: Vec<&Token> = expanded.iter().collect();
    let mut facts = ScriptFacts {
        includes: pre.includes.clone(),
        missing_includes: pre.missing.clone(),
        ..ScriptFacts::default()
    };

    // 1. Noms de procédures (déclarations et définitions).
    let mut names: BTreeSet<String> = BTreeSet::new();
    for (i, t) in toks.iter().enumerate() {
        if t.kind == Kind::Ident
            && t.text == "procedure"
            && let Some(n) = toks.get(i + 1).filter(|n| n.kind == Kind::Ident)
        {
            names.insert(n.text.clone());
        }
    }
    // 2. Corps.
    let mut defined: BTreeSet<String> = BTreeSet::new();
    let mut i = 0;
    while i < toks.len() {
        let t = toks[i];
        if !(t.kind == Kind::Ident && t.text == "procedure") {
            i += 1;
            continue;
        }
        let Some(name) = toks.get(i + 1).filter(|n| n.kind == Kind::Ident) else {
            i += 1;
            continue;
        };
        let mut j = i + 2;
        let mut arg_count = 0;
        if toks.get(j).is_some_and(|x| x.text == "(") {
            let mut level = 0;
            while j < toks.len() {
                match toks[j].text.as_str() {
                    "(" => level += 1,
                    ")" => {
                        level -= 1;
                        if level == 0 {
                            j += 1;
                            break;
                        }
                    }
                    "variable" => arg_count += 1,
                    _ => {}
                }
                j += 1;
            }
        }
        if toks.get(j).is_none_or(|x| x.text != "begin") {
            // Déclaration en avant (`procedure nom;`).
            i = j.max(i + 2);
            continue;
        }
        let begin = j;
        let (mut depth, mut k) = (0i32, begin);
        let mut end = toks.len().saturating_sub(1);
        while k < toks.len() {
            match toks[k].text.as_str() {
                "begin" if toks[k].kind == Kind::Ident => depth += 1,
                "end" if toks[k].kind == Kind::Ident => {
                    depth -= 1;
                    if depth == 0 {
                        end = k;
                        break;
                    }
                }
                _ => {}
            }
            k += 1;
        }
        let mut p = ProcFacts {
            name: name.text.clone(),
            file: t.file,
            line: t.line,
            end_line: toks[end].line,
            arg_count,
            ..ProcFacts::default()
        };
        scan_body(&toks[begin + 1..end.max(begin + 1)], &names, &mut p);
        defined.insert(p.name.clone());
        facts.procedures.push(p);
        i = end + 1;
    }
    facts.undefined = names.difference(&defined).cloned().collect();
    facts
}

fn scan_body(body: &[&Token], names: &BTreeSet<String>, p: &mut ProcFacts) {
    let mut i = 0;
    while i < body.len() {
        let t = body[i];
        if t.kind != Kind::Ident {
            i += 1;
            continue;
        }
        let name = t.text.as_str();
        // Appel d'une procédure du script (`nom(...)` ou `call nom`).
        if names.contains(name) && name != p.name
            || (names.contains(name) && name == p.name && i > 0 && body[i - 1].text == "call")
        {
            p.calls.insert(name.to_string());
        } else if name == p.name && i > 0 && body[i - 1].text != "procedure" {
            // Appel récursif.
            p.calls.insert(name.to_string());
        }
        let open = body.get(i + 1).is_some_and(|x| x.text == "(");
        let first = body.get(i + 2).filter(|_| open);
        let literal = first.and_then(|x| (x.kind == Kind::Num).then(|| number(&x.text)).flatten());
        let counted = var_kind(name).is_some()
            || is_message_fn(name)
            || is_create_fn(name)
            || name == "message_str";
        if counted && open {
            if name == "message_str" {
                // `message_str(sid, n)` : le numéro est le deuxième argument.
                let n = body[i + 2..]
                    .iter()
                    .position(|x| x.text == ",")
                    .and_then(|c| body.get(i + 2 + c + 1))
                    .and_then(|x| (x.kind == Kind::Num).then(|| number(&x.text)).flatten());
                match n {
                    Some(n) => {
                        p.messages.insert(n);
                    }
                    None => p.dynamic += 1,
                }
            } else {
                match literal {
                    // Un littéral suivi d'un opérateur (`n + 1`) n'est pas la valeur seule : on ne la suit pas.
                    Some(n)
                        if body
                            .get(i + 3)
                            .is_some_and(|x| matches!(x.text.as_str(), ")" | ",")) =>
                    {
                        if let Some((kind, write)) = var_kind(name) {
                            let set = match (kind, write) {
                                (0, false) => &mut p.gvar_read,
                                (0, true) => &mut p.gvar_write,
                                (1, false) => &mut p.mvar_read,
                                (1, true) => &mut p.mvar_write,
                                (_, false) => &mut p.lvar_read,
                                (_, true) => &mut p.lvar_write,
                            };
                            set.insert(n);
                        } else if is_message_fn(name) {
                            p.messages.insert(n);
                        } else {
                            p.created.insert(n);
                        }
                    }
                    _ => p.dynamic += 1,
                }
            }
        }
        i += 1;
    }
}

/// Nombre de références par variable globale dans tout le script (utile pour classer).
pub fn gvar_usage(facts: &ScriptFacts) -> BTreeMap<i64, (usize, usize)> {
    let mut m: BTreeMap<i64, (usize, usize)> = BTreeMap::new();
    for p in &facts.procedures {
        for &n in &p.gvar_read {
            m.entry(n).or_default().0 += 1;
        }
        for &n in &p.gvar_write {
            m.entry(n).or_default().1 += 1;
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> MemoryProvider {
        let mut m = HashMap::new();
        m.insert("defs.h".to_string(), "#define GVAR_QUEST 42\n#define LVAR_SEEN 3\n#define say(n) display_msg(mstr(n))\n#ifndef GUARD\n#define GUARD 1\n#define OTHER 7\n#endif\n".to_string());
        MemoryProvider(m)
    }

    const SRC: &str = r#"
#include "defs.h"
procedure start;
procedure helper(variable a, variable b);
procedure unused_one;

procedure start begin
   if (global_var(GVAR_QUEST) == 0) then begin
      set_global_var(GVAR_QUEST, 1);
      call helper(1, 2);
   end
   say(100);
end

procedure helper(variable a, variable b) begin
   variable x;
   x := local_var(LVAR_SEEN);
   set_local_var(LVAR_SEEN, 1);
   display_msg(message_str(SCRIPT_REALNAME, 205));
   x := create_object_sid(0x1000012, 0, 0, -1);
   x := map_var(OTHER);
   set_global_var(GVAR_QUEST + 1, 3);
end

procedure unused_one begin
end
"#;

    #[test]
    fn finds_procedures_calls_and_variables() {
        let f = analyze(SRC, &provider());
        assert!(f.missing_includes.is_empty(), "{:?}", f.missing_includes);
        let names: Vec<&str> = f.procedures.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["start", "helper", "unused_one"]);
        let start = f.find("start").unwrap();
        assert!(start.calls.contains("helper"));
        assert_eq!(start.gvar_read.iter().copied().collect::<Vec<_>>(), [42]);
        assert_eq!(start.gvar_write.iter().copied().collect::<Vec<_>>(), [42]);
        assert_eq!(
            start.messages.iter().copied().collect::<Vec<_>>(),
            [100],
            "macro `say(n)` développée"
        );
        let helper = f.find("helper").unwrap();
        assert_eq!(helper.arg_count, 2);
        assert_eq!(helper.lvar_read.iter().copied().collect::<Vec<_>>(), [3]);
        assert_eq!(helper.lvar_write.iter().copied().collect::<Vec<_>>(), [3]);
        assert_eq!(helper.messages.iter().copied().collect::<Vec<_>>(), [205]);
        assert_eq!(
            helper.created.iter().copied().collect::<Vec<_>>(),
            [0x1000012]
        );
        assert_eq!(helper.mvar_read.iter().copied().collect::<Vec<_>>(), [7]);
        assert_eq!(
            helper.dynamic, 1,
            "`GVAR_QUEST + 1` n'est pas un littéral seul"
        );
        assert_eq!(f.callers_of("helper"), ["start"]);
        assert_eq!(f.unused(), ["unused_one"]);
        assert_eq!(f.users_of_gvar(42), [("start", true)]);
    }

    #[test]
    fn missing_include_is_reported_and_conditions_follow_defines() {
        let f = analyze(
            "#include \"nope.h\"\n#ifdef NOPE\nprocedure hidden begin\nend\n#endif\nprocedure shown begin\nend\n",
            &MemoryProvider::default(),
        );
        assert_eq!(f.missing_includes, ["nope.h"]);
        assert_eq!(
            f.procedures
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["shown"]
        );
    }

    #[test]
    fn nested_begin_end_and_lines() {
        let f = analyze(
            "procedure a begin\n  if (1) then begin\n    while (1) do begin\n    end\n  end\nend\nprocedure b begin\nend\n",
            &MemoryProvider::default(),
        );
        let (a, b) = (f.find("a").unwrap(), f.find("b").unwrap());
        assert_eq!((a.line, a.end_line, b.line), (1, 6, 7));
    }

    #[test]
    fn recursion_and_forward_declarations() {
        let f = analyze(
            "procedure loopy;\nprocedure never;\nprocedure loopy begin\n  call loopy;\nend\n",
            &MemoryProvider::default(),
        );
        assert_eq!(
            f.find("loopy")
                .unwrap()
                .calls
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            ["loopy"]
        );
        assert_eq!(f.undefined, ["never"]);
    }
}
