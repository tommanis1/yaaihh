//! A call's template, parsed and checked against its parameters' docket types: coordd's load reads a step's template
//! so (policy/call.rs `Calls::one`, which leaves a call whose template fails out, and every kind with a step of it),
//! and docket-ls refuses what it refuses, at the line (paper-research N-1446 p1842: `{{ madex }}` let through). Moved
//! here from coordd's policy/call.rs whole; coordd renders the nodes (`render`) and checks a nets' call's template in
//! the policy's types (`Calls::check`).
//!
//! The language: `{{ path }}`, `{{ if path }}`, `{{ for x in path }}`, `{{ match path }}` with an arm `{{ C(a, b) }}`
//! for each alternative, `{{ end }}`, and `{{ include "file" }}` (coordd's policy/call.rs doc says what each means).

use crate::Type;
use std::collections::BTreeMap;

/// A template, parsed. A hole, an `if`, a `for` and a `match` keep their line (in the call's file; 0 where it is not known). A hole's
/// AS is how its value reads, set when the template is checked (a value carries no type); shared by its copies.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Text(String),
    Hole(Vec<String>, usize, As),
    If(Vec<String>, usize, Vec<Node>),
    For(String, Vec<String>, usize, Vec<Node>),
    Match(Vec<String>, usize, Vec<Arm>),
}

/// How a hole's value reads, by its type: a `Ref` as an answer writes it (`a 02e0410, b 1a2b3c4`), a list of them
/// one per line; else as it is.
pub type As = std::sync::Arc<std::sync::OnceLock<Shape>>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Ref,
    Refs,
}

/// An arm of a template's `{{ match }}`: `{{ C(a, b) }}`, its alternative C, the names its first fields are bound to
/// in BODY, and its line. FIELDS, those fields' names, are set when the template is checked (a value's fields are
/// read by name: a JSON object keeps no order); shared by the arm's copies.
#[derive(Clone, Debug, PartialEq)]
pub struct Arm {
    pub ctor: String,
    pub names: Vec<String>,
    pub body: Vec<Node>,
    pub line: usize,
    pub fields: std::sync::Arc<std::sync::OnceLock<Vec<String>>>,
}

/// What a template tag opened and its `{{ end }}` closes.
enum Open {
    Top,
    If(Vec<String>, usize),
    For(String, Vec<String>, usize),
    /// the matched path, its line, the arms so far and the one being read (its body is the stack's)
    Match(Vec<String>, usize, Vec<Arm>, Option<(String, Vec<String>, usize)>),
}

/// TAG read as an arm of a `{{ match }}`: `C` or `C(a, b)`, C capitalised.
fn arm_tag(tag: &str) -> Option<(String, Vec<String>)> {
    let (c, rest) = match tag.split_once('(') {
        Some((c, r)) => (c.trim(), Some(r.strip_suffix(')')?)),
        None => (tag.trim(), None),
    };
    let word = |w: &str| !w.is_empty() && w.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-');
    if !c.chars().next().is_some_and(char::is_uppercase) || !word(c) {
        return None;
    }
    let names: Vec<String> = rest.map_or(Vec::new(), |r| if r.trim().is_empty() { Vec::new() } else { r.split(',').map(|n| n.trim().to_string()).collect() });
    names.iter().all(|n| word(n)).then(|| (c.to_string(), names))
}

/// A template's `{{ match P }}` (at LINE) with ARMS on a value with alternatives ALTS (each with its fields): as a
/// program's `match`, each alternative has exactly one arm, and an arm's names are bound to its alternative's first
/// fields, in order. Gives each arm's bound names with their types. A hole in an arm that reads a name another arm
/// binds, and this one does not, is refused with its line. LIST names the alternatives' type for the refusals.
pub fn arms_of<T: Clone>(p: &[String], line: usize, arms: &[Arm], alts: &[(String, Vec<(String, T)>)], list: &str) -> Result<Vec<Vec<(String, T)>>, String> {
    let at = |l: usize| if l == 0 { String::new() } else { format!("line {l}: ") };
    let p = p.join(".");
    let mut out = Vec::new();
    for (k, a) in arms.iter().enumerate() {
        let Some((_, fs)) = alts.iter().find(|(c, _)| *c == a.ctor) else {
            return Err(format!("{}{{{{ {} }}}}: {} is not an alternative of `{p}` (its alternatives: {list})", at(a.line), a.ctor, a.ctor));
        };
        if arms[..k].iter().any(|b| b.ctor == a.ctor) {
            return Err(format!("{}{{{{ {} }}}}: a second arm for {} in {{{{ match {p} }}}}", at(a.line), a.ctor, a.ctor));
        }
        if a.names.len() > fs.len() {
            return Err(format!("{}{{{{ {} }}}}: {} has {} fields, the arm names {}", at(a.line), a.ctor, a.ctor, fs.len(), a.names.len()));
        }
        let _ = a.fields.set(fs.iter().take(a.names.len()).map(|(f, _)| f.clone()).collect());
        out.push(a.names.iter().cloned().zip(fs.iter().map(|(_, t)| t.clone())).collect::<Vec<_>>());
    }
    if let Some((c, _)) = alts.iter().find(|(c, _)| !arms.iter().any(|a| a.ctor == *c)) {
        return Err(format!("{}{{{{ match {p} }}}} has no arm for {c}", at(line)));
    }
    // a name bound in another arm only, read in this one
    for (k, a) in arms.iter().enumerate() {
        let others: Vec<(&str, &str)> = arms.iter().enumerate().filter(|(j, _)| *j != k).flat_map(|(_, b)| b.names.iter().map(|n| (n.as_str(), b.ctor.as_str()))).filter(|(n, _)| !a.names.iter().any(|m| m == n)).collect();
        if let Some((n, l, c)) = holes(&a.body).into_iter().find_map(|(h, l)| others.iter().find(|(n, _)| *n == h).map(|(n, c)| (*n, l, *c))) {
            return Err(format!("{}{{{{ {n} }}}} reads a field of the {c} arm, in the {} arm of {{{{ match {p} }}}}", at(l), a.ctor));
        }
    }
    Ok(out)
}

/// The first names of the holes in NODES, with their lines, a name a `for` or an inner arm binds not counted.
fn holes(nodes: &[Node]) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    for n in nodes {
        match n {
            Node::Hole(p, l, _) => out.push((p[0].clone(), *l)),
            Node::If(_, _, b) => out.extend(holes(b)),
            Node::For(x, _, _, b) => out.extend(holes(b).into_iter().filter(|(h, _)| h != x)),
            Node::Match(_, _, arms) => {
                for a in arms {
                    out.extend(holes(&a.body).into_iter().filter(|(h, _)| !a.names.contains(h)));
                }
            }
            Node::Text(_) => {}
        }
    }
    out
}

/// Parse TEXT, a template whose first line is line FIRST of its file (0: not known), INCLUDE giving the text of an
/// `{{ include "f" }}`'s file F (beside the template's). A `{{ match }}`, an arm of one or the `{{ end }}` closing it,
/// alone on its line, takes the line with it. A refusal with a line starts `line N: `, as `check_step`'s do.
pub fn parse(text: &str, first: usize, include: &dyn Fn(&str) -> Result<String, String>) -> Result<Vec<Node>, String> {
    parse_at(text, first, 0, include)
}

/// `parse`, DEPTH counting includes, against cycles.
/// A `{{ match }}`, an arm of one or the `{{ end }}` closing it, alone on its line, takes the line with it.
fn parse_at(text: &str, first: usize, depth: usize, include: &dyn Fn(&str) -> Result<String, String>) -> Result<Vec<Node>, String> {
    if depth > 8 {
        return Err("includes nest deeper than 8".into());
    }
    // tokens: text and tags, each tag with its line
    let mut toks: Vec<(bool, String, usize)> = Vec::new();
    let mut rest = text;
    let mut line = first;
    while let Some(i) = rest.find("{{") {
        toks.push((false, rest[..i].to_string(), 0));
        line += rest[..i].matches('\n').count();
        let j = rest[i..].find("}}").ok_or("an unclosed {{")? + i;
        toks.push((true, rest[i + 2..j].trim().to_string(), if first == 0 { 0 } else { line }));
        rest = &rest[j + 2..];
    }
    toks.push((false, rest.to_string(), 0));
    let at = |l: usize| if l == 0 { String::new() } else { format!("line {l}: ") };
    // the text as written: whether a tag is alone on its line does not change when the one before took its line
    let written: Vec<String> = toks.iter().map(|t| t.1.clone()).collect();
    let mut stack: Vec<(Open, Vec<Node>)> = vec![(Open::Top, Vec::new())];
    let path = |s: &str| s.split('.').map(|p| p.trim().to_string()).collect::<Vec<_>>();
    for k in 0..toks.len() {
        let (tag, t, line) = toks[k].clone();
        if !tag {
            if !t.is_empty() {
                stack.last_mut().unwrap().1.push(Node::Text(t));
            }
            continue;
        }
        let in_match = matches!(stack.last().unwrap().0, Open::Match(..));
        let arm = if in_match { arm_tag(&t) } else { None };
        let block = t.starts_with("match ") || arm.is_some() || (t == "end" && in_match);
        if block {
            // alone on its line: the spaces before it and the line break after it go
            let lead = written[k - 1].trim_end_matches([' ', '\t']);
            let after = written.get(k + 1).map_or("", String::as_str).trim_start_matches([' ', '\t']);
            let starts = (k == 1 && lead.is_empty()) || lead.ends_with('\n');
            if starts && (after.starts_with('\n') || k + 2 == toks.len() && after.is_empty()) {
                let body = &mut stack.last_mut().unwrap().1;
                if let Some(Node::Text(x)) = body.last_mut() {
                    x.truncate(x.trim_end_matches([' ', '\t']).len());
                    if x.is_empty() {
                        body.pop();
                    }
                }
                if let Some(next) = toks.get_mut(k + 1) {
                    next.1 = next.1.trim_start_matches([' ', '\t']).strip_prefix('\n').unwrap_or("").to_string();
                }
            }
        }
        if let Some((c, names)) = arm {
            let (Open::Match(_, _, arms, cur), body) = stack.last_mut().unwrap() else { unreachable!() };
            let body = std::mem::take(body);
            match cur.take() {
                Some((c, names, line)) => arms.push(Arm { ctor: c, names, body, line, fields: Default::default() }),
                None if body.iter().any(|n| !matches!(n, Node::Text(x) if x.trim().is_empty())) => {
                    return Err(format!("{}text between {{{{ match }}}} and its first arm: an arm is `{{{{ C }}}}` or `{{{{ C(field, ...) }}}}`", at(line)));
                }
                None => {}
            }
            *cur = Some((c, names, line));
        } else if let Some(p) = t.strip_prefix("match ") {
            stack.push((Open::Match(path(p), line, Vec::new(), None), Vec::new()));
        } else if let Some(p) = t.strip_prefix("if ") {
            stack.push((Open::If(path(p), line), Vec::new()));
        } else if let Some(r) = t.strip_prefix("for ") {
            let (x, p) = r.split_once(" in ").ok_or("`{{ for x in path }}`")?;
            stack.push((Open::For(x.trim().into(), path(p), line), Vec::new()));
        } else if t == "end" {
            let (open, body) = stack.pop().unwrap();
            let node = match open {
                Open::Top => return Err(format!("{}an {{{{ end }}}} with no if, for or match", at(line))),
                Open::If(p, l) => Node::If(p, l, body),
                Open::For(x, p, l) => Node::For(x, p, l, body),
                Open::Match(p, l, mut arms, cur) => {
                    let Some((c, names, line)) = cur else { return Err(format!("{}{{{{ match {} }}}} has no arm", at(l), p.join("."))) };
                    arms.push(Arm { ctor: c, names, body, line, fields: Default::default() });
                    Node::Match(p, l, arms)
                }
            };
            stack.last_mut().unwrap().1.push(node);
        } else if let Some(f) = t.strip_prefix("include ") {
            let f = f.trim().trim_matches('"');
            let text = include(f).map_err(|e| format!("include {f}: {e}"))?;
            let nodes = parse_at(text.trim_end_matches('\n'), 0, depth + 1, include)?;
            stack.last_mut().unwrap().1.extend(nodes);
        } else {
            stack.last_mut().unwrap().1.push(Node::Hole(path(&t), line, As::default()));
        }
    }
    if stack.len() != 1 {
        return Err("an {{ if }}, {{ for }} or {{ match }} without its {{ end }}".into());
    }
    Ok(stack.pop().unwrap().1)
}

/// Check a ticket step's template NODES against SCOPE, its names in docket's types: every path starts at a name in
/// scope and goes through fields a record has; a hole is not a record (a list of plain values is one per line); `for` goes over a list. What a type
/// does not say (a sum, `Any`) is let through: the kinds' values are checked by the machine.
pub fn check_step(nodes: &[Node], scope: &BTreeMap<String, Type>) -> Result<(), String> {
    use crate::Type as D;
    // a refusal says its line, so docket-ls puts it there (paper-research N-1446 p1842)
    let at = |l: usize| if l == 0 { String::new() } else { format!("line {l}: ") };
    fn path(scope: &BTreeMap<String, D>, p: &[String]) -> Result<D, String> {
        let names = || scope.keys().cloned().collect::<Vec<_>>().join(", ");
        let mut t = scope.get(&p[0]).cloned().ok_or_else(|| format!("{{{{ {} }}}}: no `{}` here (the call's parameters: {})", p.join("."), p[0], names()))?;
        for f in &p[1..] {
            let inner = match t { D::Opt(x) => *x, x => x };
            t = match inner {
                D::Record(fs) => fs.iter().find(|(n, _)| n == f).map(|(_, t)| t.clone()).ok_or_else(|| format!("{{{{ {} }}}}: no field `{f}` (it has {})", p.join("."), fs.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")))?,
                D::Variant(_) | D::Any => return Ok(D::Any),
                other => return Err(format!("{{{{ {} }}}}: a {other:?} has no field `{f}`", p.join("."))),
            };
        }
        Ok(t)
    }
    for n in nodes {
        match n {
            Node::Text(_) => {}
            // a list of plain values fills a hole one per line (paper-research N-101.p6); a record, or a list of
            // lists or records, does not
            Node::Hole(p, line, how) => match path(scope, p).map_err(|e| format!("{}{e}", at(*line)))? {
                // a value with no one way to be written (paper-research N-101 p880): a sum, a tuple, a value of no
                // known type
                t if crate::check::unwritable(&t, p.len() == 1).is_some() => {
                    return Err(format!("{}{{{{ {} }}}}: {}", at(*line), p.join("."), crate::check::unwritable(&t, p.len() == 1).unwrap_or_default()));
                }
                D::Ref => {
                    let _ = how.set(Shape::Ref);
                }
                D::Opt(t) if *t == D::Ref => {
                    let _ = how.set(Shape::Ref);
                }
                D::List(t) if *t == D::Ref => {
                    let _ = how.set(Shape::Refs);
                }
                D::List(t) if !matches!(*t, D::List(_) | D::Record(_)) => {}
                D::List(_) | D::Record(_) => return Err(format!("{}{{{{ {} }}}} is a record or a list of lists or records: a hole takes one value, or a list of plain values (one per line)", at(*line), p.join("."))),
                _ => {}
            },
            Node::If(p, line, body) => {
                let mut inner = scope.clone();
                if let (D::Opt(t), 1) = (path(scope, p).map_err(|e| format!("{}{e}", at(*line)))?, p.len()) {
                    inner.insert(p[0].clone(), *t);
                }
                check_step(body, &inner)?;
            }
            Node::For(x, p, line, body) => {
                let t = match path(scope, p).map_err(|e| format!("{}{e}", at(*line)))? {
                    D::List(t) => *t,
                    D::Opt(t) if matches!(*t, D::List(_)) => match *t { D::List(t) => *t, _ => unreachable!() },
                    D::Any => D::Any,
                    t => return Err(format!("{}{{{{ for {x} in {} }}}} goes over a {t:?}: it takes a list", at(*line), p.join("."))),
                };
                let mut inner = scope.clone();
                inner.insert(x.clone(), t);
                check_step(body, &inner)?;
            }
            Node::Match(p, line, arms) => {
                let alts = match path(scope, p).map_err(|e| format!("{}{e}", at(*line)))? {
                    D::Variant(cs) => cs,
                    t => return Err(format!("{}{{{{ match {} }}}} is on a {t}: it takes a value of a type with alternatives", at(*line), p.join("."))),
                };
                let list = alts.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>().join(", ");
                for (a, bound) in arms.iter().zip(arms_of(p, *line, arms, &alts, &list)?) {
                    let mut inner = scope.clone();
                    inner.extend(bound);
                    check_step(&a.body, &inner)?;
                }
            }
        }
    }
    Ok(())
}
