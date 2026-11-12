//! Désassemblage brut et reconstruction d'un pseudo-source lisible (expressions reconstruites par simulation de la
//! pile, puis sauts structurés en `if` / `else` / `while` par le module `structure`).

use crate::messages::{MessageSource, NoMessages};
use crate::opcode::*;
use crate::program::{Instruction, Procedure, Program};
use crate::structure::{Line, render, structure};
use std::collections::BTreeSet;
use std::fmt::Write;

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn op_name(code: u16) -> String {
    lookup(code).map_or_else(|| format!("op_{code:04X}"), |i| i.name.to_string())
}

impl Program {
    /// Listing brut d'une procédure : une ligne par opcode.
    pub fn disassemble(&self, proc_index: usize) -> String {
        let mut out = String::new();
        for ins in self.bodies.get(proc_index).map_or(&[][..], Vec::as_slice) {
            let _ = write!(out, "{:08X}  {:04X}  ", ins.offset, ins.code);
            match ins.code {
                O_INTOP => {
                    let _ = write!(out, "int {} (0x{:X})", ins.arg as i32, ins.arg);
                }
                O_FLOATOP => {
                    let _ = write!(out, "float {}", f32::from_bits(ins.arg));
                }
                O_STRINGOP => {
                    let _ = write!(
                        out,
                        "string \"{}\"",
                        escape(self.string_arg(ins.arg).unwrap_or("?"))
                    );
                }
                c => out.push_str(&op_name(c)),
            }
            out.push('\n');
        }
        out
    }

    /// Pseudo-source du script : déclarations globales puis procédures.
    pub fn pseudo_source(&self) -> String {
        self.pseudo_source_with(&NoMessages)
    }

    /// Comme [`Program::pseudo_source`], avec le texte des messages en commentaire.
    pub fn pseudo_source_with(&self, msgs: &dyn MessageSource) -> String {
        let mut out = String::new();
        if !self.globals.is_empty() {
            out.push_str("// Variables globales\n");
            for g in &self.globals {
                let _ = writeln!(out, "variable {}{};", g.name, self.initializer(&g.init));
            }
            out.push('\n');
        }
        if !self.exported_vars.is_empty() {
            out.push_str("// Variables exportées\n");
            for v in &self.exported_vars {
                let _ = writeln!(
                    out,
                    "export variable {}{};",
                    v.name,
                    self.initializer(&v.value)
                );
            }
            out.push('\n');
        }
        for i in 0..self.procedures.len() {
            out.push_str(&self.procedure_source_with(i, msgs));
            out.push('\n');
        }
        out
    }

    /// ` := valeur`, ou rien quand la valeur est le zéro implicite.
    fn initializer(&self, ins: &Instruction) -> String {
        if ins.code == O_INTOP && ins.arg == 0 {
            String::new()
        } else {
            format!(" := {}", self.literal(ins))
        }
    }

    /// Nom d'une variable globale du script.
    fn global_name(&self, index: i32) -> Option<&str> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.globals.get(i))
            .map(|g| g.name.as_str())
    }

    /// Pseudo-source d'une procédure.
    pub fn procedure_source(&self, proc_index: usize) -> String {
        self.procedure_source_with(proc_index, &NoMessages)
    }

    /// Comme [`Program::procedure_source`], avec le texte des messages en commentaire.
    pub fn procedure_source_with(&self, proc_index: usize, msgs: &dyn MessageSource) -> String {
        let Some(p) = self.procedures.get(proc_index) else {
            return String::new();
        };
        let mut out = String::new();
        let args: Vec<String> = (0..p.arg_count)
            .map(|i| format!("variable arg{i}"))
            .collect();
        let _ = write!(out, "procedure {}", p.name);
        if !args.is_empty() {
            let _ = write!(out, "({})", args.join(", "));
        }
        let flags = p.flag_labels().join(", ");
        if !flags.is_empty() {
            let _ = write!(out, "  // {flags}");
        }
        if !p.is_implemented() {
            out.push_str(";\n");
            return out;
        }
        out.push_str(" begin\n");
        let mut locals = BTreeSet::new();
        let lines = self.body_lines(p, &self.bodies[proc_index], &mut locals, msgs);
        for l in &locals {
            let _ = writeln!(out, "    variable LVar{l};");
        }
        if !locals.is_empty() {
            out.push('\n');
        }
        render(&structure(&lines), 1, &mut out);
        out.push_str("end\n");
        out
    }

    fn literal(&self, ins: &Instruction) -> String {
        match ins.code {
            O_INTOP => (ins.arg as i32).to_string(),
            O_FLOATOP => format!("{:.5}", f32::from_bits(ins.arg)),
            _ => format!("\"{}\"", escape(self.string_arg(ins.arg).unwrap_or("?"))),
        }
    }

    /// Nom d'une variable locale : paramètre (`argN`) ou variable de la procédure (`LVarN`).
    fn local_name(&self, index: &Val, arg_count: u32, locals: &mut BTreeSet<i32>) -> String {
        match index.int {
            Some(i) if i >= 0 && (i as u32) < arg_count => format!("arg{i}"),
            Some(i) => {
                locals.insert(i);
                format!("LVar{i}")
            }
            None => format!("var[{}]", index.text),
        }
    }

    fn external_name(&self, name: &Val) -> String {
        name.soff
            .and_then(|o| self.names.get(o))
            .map_or_else(|| format!("extern[{}]", name.text), str::to_string)
    }

    fn global_ref(&self, index: &Val) -> String {
        index
            .int
            .and_then(|n| self.global_name(n))
            .map_or_else(|| format!("gvar[{}]", index.text), str::to_string)
    }

    fn body_lines(
        &self,
        proc: &Procedure,
        body: &[Instruction],
        locals: &mut BTreeSet<i32>,
        msgs: &dyn MessageSource,
    ) -> Vec<Line> {
        let mut lines = Vec::new();
        let mut stack: Vec<Val> = Vec::new();
        let pop = |stack: &mut Vec<Val>| stack.pop().unwrap_or_else(|| Val::text("/*?*/"));

        let mut targets = BTreeSet::new();
        for ins in body {
            let at = ins.offset;
            match ins.code {
                O_INTOP | O_FLOATOP | O_STRINGOP => stack.push(Val {
                    text: self.literal(ins),
                    int: (ins.code == O_INTOP).then_some(ins.arg as i32),
                    soff: (ins.code == O_STRINGOP).then_some(ins.arg),
                }),
                O_FETCH_GLOBAL => {
                    let i = pop(&mut stack);
                    stack.push(Val::text(self.global_ref(&i)));
                }
                O_STORE_GLOBAL => {
                    let i = pop(&mut stack);
                    let v = pop(&mut stack);
                    lines.push(Line::Stmt(
                        at,
                        format!("{} = {};", self.global_ref(&i), v.text),
                    ));
                }
                O_FETCH => {
                    let i = pop(&mut stack);
                    stack.push(Val::text(self.local_name(&i, proc.arg_count, locals)));
                }
                O_STORE => {
                    let i = pop(&mut stack);
                    let v = pop(&mut stack);
                    lines.push(Line::Stmt(
                        at,
                        format!(
                            "{} = {};",
                            self.local_name(&i, proc.arg_count, locals),
                            v.text
                        ),
                    ));
                }
                O_FETCH_EXTERNAL => {
                    let n = pop(&mut stack);
                    stack.push(Val::text(self.external_name(&n)));
                }
                O_STORE_EXTERNAL => {
                    let n = pop(&mut stack);
                    let v = pop(&mut stack);
                    lines.push(Line::Stmt(
                        at,
                        format!("{} = {};", self.external_name(&n), v.text),
                    ));
                }
                O_POP => {
                    let e = pop(&mut stack);
                    lines.push(Line::Stmt(at, format!("{};", e.text)));
                }
                O_POP_RETURN => {
                    let e = pop(&mut stack);
                    lines.push(Line::Stmt(at, format!("return {};", e.text)));
                }
                O_CALL => {
                    let target = pop(&mut stack);
                    let name = target
                        .int
                        .and_then(|i| usize::try_from(i).ok())
                        .and_then(|i| self.procedures.get(i))
                        .map_or_else(|| format!("call_proc({})", target.text), |p| p.name.clone());
                    let argc = pop(&mut stack)
                        .int
                        .map_or(0, |n| n.max(0) as usize)
                        .min(stack.len());
                    let args: Vec<String> = stack
                        .split_off(stack.len() - argc)
                        .into_iter()
                        .map(|v| v.text)
                        .collect();
                    stack.push(Val::text(format!("{name}({})", args.join(", "))));
                }
                O_JMP => {
                    let t = pop(&mut stack);
                    match t.int {
                        Some(target) => {
                            targets.insert(target as u32);
                            lines.push(Line::Goto(at, target as u32));
                        }
                        None => lines.push(Line::Stmt(at, format!("goto *({});", t.text))),
                    }
                }
                O_IF | O_WHILE => {
                    // L'adresse est empilée avant la condition.
                    let c = pop(&mut stack);
                    let t = pop(&mut stack);
                    match t.int {
                        Some(target) => {
                            targets.insert(target as u32);
                            lines.push(Line::Cond(at, c.text, target as u32));
                        }
                        None => lines.push(Line::Stmt(
                            at,
                            format!("if (!({})) goto *({});", c.text, t.text),
                        )),
                    }
                }
                code if is_plumbing(code) => {}
                code => self.generic(code, at, &mut stack, &mut lines, msgs),
            }
        }
        if !stack.is_empty() {
            let rest: Vec<String> = stack.into_iter().map(|v| v.text).collect();
            lines.push(Line::Stmt(
                body.last().map_or(0, |i| i.offset),
                format!("/* pile non vidée : {} */", rest.join(" | ")),
            ));
        }
        insert_labels(lines, &targets)
    }

    fn generic(
        &self,
        code: u16,
        at: u32,
        stack: &mut Vec<Val>,
        lines: &mut Vec<Line>,
        msgs: &dyn MessageSource,
    ) {
        let Some(info) = lookup(code) else {
            lines.push(Line::Stmt(at, format!("/* opcode inconnu {code:04X} */")));
            return;
        };
        let argc = (info.argc as usize).min(stack.len());
        let missing = info.argc as usize - argc;
        let vals = stack.split_off(stack.len() - argc);
        let note = message_args(code)
            .filter(|_| missing == 0)
            .and_then(|(list, id)| {
                let list_value = list.and_then(|l| vals.get(l)).and_then(|v| v.int);
                let text = msgs.message(list_value, vals.get(id)?.int?)?;
                Some((id, text))
            });
        let mut args: Vec<String> = vals.into_iter().map(|v| v.text).collect();
        if let Some((id, text)) = note {
            args[id] = format!("{} /* « {text} » */", args[id]);
        }
        for _ in 0..missing {
            args.insert(0, "/*?*/".to_string());
        }
        let list = args.join(", ");
        let call = if info.name.starts_with("/*") || info.name.starts_with("//") {
            format!("{} {list}", info.name.trim())
        } else {
            format!("{}({list})", info.name)
        };
        match info.kind {
            Kind::Statement => lines.push(Line::Stmt(at, format!("{call};"))),
            Kind::Expression | Kind::ExprStatement => {
                let text = if info.argc == 1 && matches!(info.name, "-" | "not" | "bwnot") {
                    format!("{}{}", if info.name == "-" { "-" } else { "not " }, args[0])
                } else {
                    call
                };
                stack.push(Val::text(text));
            }
            Kind::Infix => {
                if args.len() == 2 {
                    stack.push(Val::text(format!(
                        "({} {} {})",
                        args[0], info.name, args[1]
                    )));
                } else {
                    stack.push(Val::text(call));
                }
            }
        }
    }
}

/// Valeur de la pile simulée : son texte, et ce qu'on sait de sa nature quand c'est une constante.
struct Val {
    text: String,
    /// Constante entière.
    int: Option<i32>,
    /// Décalage de chaîne (table des textes ou des noms).
    soff: Option<u32>,
}

impl Val {
    fn text(t: impl Into<String>) -> Self {
        Self {
            text: t.into(),
            int: None,
            soff: None,
        }
    }
}

/// Place chaque étiquette juste avant la première instruction située à son adresse ou après.
fn insert_labels(lines: Vec<Line>, targets: &std::collections::BTreeSet<u32>) -> Vec<Line> {
    let mut out = Vec::with_capacity(lines.len() + targets.len());
    let mut pending = targets.iter().copied().peekable();
    for line in lines {
        if let Some(at) = line.address() {
            while let Some(t) = pending.next_if(|&t| t <= at) {
                out.push(Line::Label(t));
            }
        }
        out.push(line);
    }
    out.extend(pending.map(Line::Label));
    out
}

/// Opcodes de gestion de la pile d'adresses et des cadres : sans intérêt dans un pseudo-source.
fn is_plumbing(code: u16) -> bool {
    matches!(code, 0x8000 | 0x800C | 0x800D | 0x8019 | 0x801F..=0x8026 | 0x8029..=0x802B)
}

/// Opcodes qui désignent un message : (indice de l'argument « liste » s'il y en a un, indice de l'argument « numéro »).
fn message_args(code: u16) -> Option<(Option<usize>, usize)> {
    match code {
        0x8105 | 0x811E..=0x8120 | 0x8052..=0x8054 => Some((Some(0), 1)),
        0x8121 => Some((Some(1), 2)),
        _ => None,
    }
}
