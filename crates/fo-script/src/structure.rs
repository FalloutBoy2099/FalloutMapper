//! Structuration du flot de contrôle : transforme les `goto` produits par le compilateur en `if` / `else` / `while`,
//! `break` et `continue`.
//!
//! Schémas reconnus (adresses croissantes vers le bas) :
//!
//! ```text
//! if (!c) goto Lelse;  A  goto Lend;  Lelse:  B  Lend:     =>  if (c) { A } else { B }
//! if (!c) goto Lend;   A  Lend:                            =>  if (c) { A }
//! Lhead: if (!c) goto Lexit;  A  goto Lhead;  Lexit:       =>  while (c) { A }
//! ```
//!
//! Tout saut qui ne rentre pas dans ces schémas reste un `goto` : le résultat est toujours correct, jamais inventé.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

/// Instruction linéaire issue de la simulation de la pile.
#[derive(Debug, Clone)]
pub(crate) enum Line {
    Label(u32),
    Stmt(u32, String),
    /// Saut inconditionnel vers une adresse connue.
    Goto(u32, u32),
    /// `if (!cond) goto target` : la condition conservée est celle qui *continue* l'exécution.
    Cond(u32, String, u32),
}

impl Line {
    pub(crate) fn address(&self) -> Option<u32> {
        match self {
            Line::Label(_) => None,
            Line::Stmt(a, _) | Line::Goto(a, _) | Line::Cond(a, _, _) => Some(*a),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Node {
    Label(u32),
    Stmt(u32, String),
    Goto(u32, u32),
    Break(u32),
    Continue(u32),
    If {
        cond: String,
        then: Vec<Node>,
        els: Vec<Node>,
    },
    While {
        cond: String,
        body: Vec<Node>,
    },
    DoWhile {
        cond: String,
        body: Vec<Node>,
    },
}

struct Ctx<'a> {
    lines: &'a [Line],
    label_at: HashMap<u32, usize>,
}

/// Boucle en cours de reconstruction : adresse de l'en-tête et de la sortie.
#[derive(Clone, Copy)]
struct LoopCtx {
    head: u32,
    exit: u32,
}

pub(crate) fn structure(lines: &[Line]) -> Vec<Node> {
    let label_at = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            if let Line::Label(a) = l {
                Some((*a, i))
            } else {
                None
            }
        })
        .collect();
    let ctx = Ctx { lines, label_at };
    let mut nodes = ctx.block(0, lines.len(), &mut Vec::new());
    let mut used = HashSet::new();
    collect_targets(&nodes, &mut used);
    drop_unused_labels(&mut nodes, &used);
    nodes
}

fn negate(cond: &str) -> String {
    format!("!({})", strip_parens(cond))
}

/// Retire une paire de parenthèses qui entoure toute l'expression.
fn strip_parens(s: &str) -> &str {
    let b = s.as_bytes();
    if b.first() != Some(&b'(') || b.last() != Some(&b')') {
        return s;
    }
    let mut depth = 0i32;
    for (i, &c) in b.iter().enumerate() {
        match c {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 && i != b.len() - 1 {
                    return s;
                }
            }
            _ => {}
        }
    }
    &s[1..s.len() - 1]
}

impl Ctx<'_> {
    fn block(&self, lo: usize, hi: usize, loops: &mut Vec<LoopCtx>) -> Vec<Node> {
        let mut out = Vec::new();
        let mut i = lo;
        while i < hi {
            match &self.lines[i] {
                Line::Label(addr) => {
                    if let Some((node, next)) = self.try_loop(i, *addr, hi, loops) {
                        out.push(node);
                        i = next;
                        continue;
                    }
                    out.push(Node::Label(*addr));
                }
                Line::Stmt(a, s) => out.push(Node::Stmt(*a, s.clone())),
                Line::Goto(a, t) => out.push(jump(*a, *t, loops)),
                Line::Cond(a, cond, t) => {
                    if let Some((node, next)) = self.try_if(i, cond, *t, hi, loops) {
                        out.push(node);
                        i = next;
                        continue;
                    }
                    // Saut conditionnel qui ne se structure pas : `if (!c) <saut>`.
                    out.push(Node::If {
                        cond: negate(cond),
                        then: vec![jump(*a, *t, loops)],
                        els: Vec::new(),
                    });
                }
            }
            i += 1;
        }
        // Un `continue` en toute fin de corps de boucle est superflu.
        if !loops.is_empty() && matches!(out.last(), Some(Node::Continue(_))) {
            out.pop();
        }
        out
    }

    fn try_loop(
        &self,
        i: usize,
        head: u32,
        hi: usize,
        loops: &mut Vec<LoopCtx>,
    ) -> Option<(Node, usize)> {
        // Dernier saut en arrière vers cette étiquette.
        let back_goto = (i + 1..hi)
            .rev()
            .find(|&j| matches!(self.lines[j], Line::Goto(_, t) if t == head));
        let back_cond = (i + 1..hi)
            .rev()
            .find(|&j| matches!(self.lines[j], Line::Cond(_, _, t) if t == head));
        // Saut conditionnel en arrière sans saut inconditionnel plus loin : boucle `do { } while (...)`.
        if let Some(j) = back_cond
            && back_goto.is_none_or(|g| j > g)
            && let Line::Cond(_, cond, _) = &self.lines[j]
        {
            loops.push(LoopCtx {
                head: u32::MAX,
                exit: u32::MAX,
            });
            let body = self.block(i + 1, j, loops);
            loops.pop();
            return Some((
                Node::DoWhile {
                    cond: negate(cond),
                    body,
                },
                j + 1,
            ));
        }
        let back = back_goto?;
        if let Some(Line::Cond(_, cond, exit)) = self.lines.get(i + 1)
            && self.label_at.get(exit).is_some_and(|&e| e > back)
        {
            loops.push(LoopCtx { head, exit: *exit });
            let body = self.block(i + 2, back, loops);
            loops.pop();
            return Some((
                Node::While {
                    cond: strip_parens(cond).to_string(),
                    body,
                },
                back + 1,
            ));
        }
        // Boucle sans condition de sortie en tête : `while (true)`.
        let exit = self
            .lines
            .get(back + 1)
            .and_then(|l| {
                if let Line::Label(a) = l {
                    Some(*a)
                } else {
                    None
                }
            })
            .unwrap_or(u32::MAX);
        loops.push(LoopCtx { head, exit });
        let body = self.block(i + 1, back, loops);
        loops.pop();
        Some((
            Node::While {
                cond: "true".into(),
                body,
            },
            back + 1,
        ))
    }

    fn try_if(
        &self,
        i: usize,
        cond: &str,
        target: u32,
        hi: usize,
        loops: &mut Vec<LoopCtx>,
    ) -> Option<(Node, usize)> {
        let end = *self.label_at.get(&target)?;
        if end <= i || end > hi {
            return None;
        }
        // `if / else` : la branche « alors » se termine par un saut en avant vers la fin de l'ensemble.
        if end > i + 1
            && let Line::Goto(_, t2) = &self.lines[end - 1]
            && let Some(&end2) = self.label_at.get(t2)
            && end2 > end
            && end2 <= hi
        {
            let then = self.block(i + 1, end - 1, loops);
            let els = self.block(end + 1, end2, loops);
            return Some((
                Node::If {
                    cond: strip_parens(cond).to_string(),
                    then,
                    els,
                },
                end2,
            ));
        }
        let then = self.block(i + 1, end, loops);
        Some((
            Node::If {
                cond: strip_parens(cond).to_string(),
                then,
                els: Vec::new(),
            },
            end,
        ))
    }
}

fn jump(at: u32, target: u32, loops: &[LoopCtx]) -> Node {
    match loops
        .iter()
        .rev()
        .find(|l| l.exit == target || l.head == target)
    {
        Some(l) if l.exit == target => Node::Break(at),
        Some(_) => Node::Continue(at),
        None => Node::Goto(at, target),
    }
}

fn collect_targets(nodes: &[Node], used: &mut HashSet<u32>) {
    for n in nodes {
        match n {
            Node::Goto(_, t) => {
                used.insert(*t);
            }
            Node::If { then, els, .. } => {
                collect_targets(then, used);
                collect_targets(els, used);
            }
            Node::While { body, .. } | Node::DoWhile { body, .. } => collect_targets(body, used),
            _ => {}
        }
    }
}

fn drop_unused_labels(nodes: &mut Vec<Node>, used: &HashSet<u32>) {
    nodes.retain(|n| !matches!(n, Node::Label(a) if !used.contains(a)));
    for n in nodes {
        match n {
            Node::If { then, els, .. } => {
                drop_unused_labels(then, used);
                drop_unused_labels(els, used);
            }
            Node::While { body, .. } | Node::DoWhile { body, .. } => drop_unused_labels(body, used),
            _ => {}
        }
    }
}

pub(crate) fn render(nodes: &[Node], depth: usize, out: &mut String) {
    let pad = "    ".repeat(depth);
    for n in nodes {
        match n {
            Node::Label(a) => {
                let _ = writeln!(out, "{}L_{a:04X}:", "    ".repeat(depth.saturating_sub(1)));
            }
            Node::Stmt(a, s) => {
                let _ = writeln!(out, "{pad}{s}    // {a:04X}");
            }
            Node::Goto(a, t) => {
                let _ = writeln!(out, "{pad}goto L_{t:04X};    // {a:04X}");
            }
            Node::Break(a) => {
                let _ = writeln!(out, "{pad}break;    // {a:04X}");
            }
            Node::Continue(a) => {
                let _ = writeln!(out, "{pad}continue;    // {a:04X}");
            }
            Node::While { cond, body } => {
                let _ = writeln!(out, "{pad}while ({cond}) {{");
                render(body, depth + 1, out);
                let _ = writeln!(out, "{pad}}}");
            }
            Node::DoWhile { cond, body } => {
                let _ = writeln!(out, "{pad}do {{");
                render(body, depth + 1, out);
                let _ = writeln!(out, "{pad}}} while ({cond});");
            }
            Node::If { cond, then, els } => {
                let _ = writeln!(out, "{pad}if ({cond}) {{");
                render(then, depth + 1, out);
                render_else(els, depth, out);
            }
        }
    }
}

/// Écrit la fin d'un `if` : `else if` quand la branche sinon n'est qu'un autre `if`.
fn render_else(els: &[Node], depth: usize, out: &mut String) {
    let pad = "    ".repeat(depth);
    match els {
        [] => {
            let _ = writeln!(out, "{pad}}}");
        }
        [
            Node::If {
                cond,
                then,
                els: inner,
            },
        ] => {
            let _ = writeln!(out, "{pad}}} else if ({cond}) {{");
            render(then, depth + 1, out);
            render_else(inner, depth, out);
        }
        _ => {
            let _ = writeln!(out, "{pad}}} else {{");
            render(els, depth + 1, out);
            let _ = writeln!(out, "{pad}}}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(a: u32, t: &str) -> Line {
        Line::Stmt(a, t.to_string())
    }

    fn text(lines: &[Line]) -> String {
        let mut out = String::new();
        render(&structure(lines), 1, &mut out);
        out
    }

    #[test]
    fn if_else() {
        let lines = vec![
            Line::Cond(1, "(x == 1)".into(), 20),
            s(5, "a();"),
            Line::Goto(8, 30),
            Line::Label(20),
            s(20, "b();"),
            Line::Label(30),
            s(30, "c();"),
        ];
        let t = text(&lines);
        assert!(t.contains("if (x == 1) {"), "{t}");
        assert!(t.contains("} else {"), "{t}");
        assert!(!t.contains("goto") && !t.contains("L_"), "{t}");
    }

    #[test]
    fn if_without_else_and_else_if_chain() {
        let lines = vec![
            Line::Cond(1, "(a)".into(), 20),
            s(5, "x();"),
            Line::Goto(8, 40),
            Line::Label(20),
            Line::Cond(21, "(b)".into(), 30),
            s(25, "y();"),
            Line::Goto(28, 40),
            Line::Label(30),
            s(31, "z();"),
            Line::Label(40),
            s(41, "end();"),
        ];
        let t = text(&lines);
        assert!(t.contains("} else if (b) {"), "{t}");
        assert!(t.contains("} else {"), "{t}");
    }

    #[test]
    fn while_with_break_and_continue() {
        let lines = vec![
            Line::Label(10),
            Line::Cond(10, "(i < 5)".into(), 90),
            Line::Cond(12, "(done)".into(), 14),
            Line::Goto(13, 90),
            Line::Label(14),
            s(14, "work();"),
            Line::Goto(20, 10),
            Line::Label(90),
            s(90, "after();"),
        ];
        let t = text(&lines);
        assert!(t.contains("while (i < 5) {"), "{t}");
        assert!(t.contains("if (done) {"), "{t}");
        assert!(t.contains("break;"), "{t}");
        assert!(!t.contains("goto"), "{t}");
    }

    #[test]
    fn do_while_from_backward_conditional_jump() {
        let lines = vec![
            Line::Label(10),
            s(10, "poll();"),
            Line::Cond(12, "(ready())".into(), 10),
            s(14, "go();"),
        ];
        let t = text(&lines);
        assert!(
            t.contains("do {") && t.contains("} while (!(ready()));"),
            "{t}"
        );
        assert!(!t.contains("goto"), "{t}");
    }

    #[test]
    fn unstructurable_jump_stays_a_goto() {
        let lines = vec![
            Line::Goto(1, 50),
            s(2, "a();"),
            Line::Label(50),
            s(50, "b();"),
        ];
        let t = text(&lines);
        assert!(t.contains("goto L_0032;"), "{t}");
        assert!(t.contains("L_0032:"), "{t}");
    }
}
