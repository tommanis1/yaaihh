//! One interpreter (paper-research N-568, part 3; consult-lead's one-interpreter.md §4.1, §6): a policy's acts asked
//! through eflint-rs, eFLINT whole, in place of norms.rs's answer set programs.
//!
//! The policy is the files norms.rs reads (`policy/*.eflint`, coordd's dialect), written out as eFLINT
//! ([`to_eflint`]), and after them the files in eFLINT whole: `policy/eflint/*.eflint` of agent-base (the library),
//! then of the project, so a project's `Extend Act` adds to a library act clause by clause rather than replacing a
//! file. Those files are not read by norms.rs (it reads `policy/` only, not its folders), so the old path is untouched.
//!
//! Which engine answers is the project's `norms` (`[project] norms = "clingo" | "both" | "eflint"`, [`Mode`]): `clingo`
//! (the default) answers with norms.rs alone; `both` answers with norms.rs and asks eflint-rs the same beside it, off
//! the caller's path, each difference one journal line (`coordd: norms both: ...`); `eflint` answers with eflint-rs.
//! `both` was the default, and a new project, its file saying none, compared every tick (700 to 1383 ms of each, a
//! lock held 10 to 15 s on one agent: the owner, paper-research N-1978 p3): a comparison is asked for by name now.

use crate::policy::syntax::{Cond, DeclKind, Expr, Lit, Op};
use eflint::ast::{Elem, Tagged};
use eflint::replay::{Event, Replay};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

pub type Facts = Vec<(String, Vec<String>)>;


/// The policy as eFLINT text, with the fact types' identifiers and the acts' fields.
#[derive(Clone, Debug)]
pub struct Spec {
    pub text: String,
    pub facts: BTreeMap<String, Vec<String>>,
    pub acts: BTreeMap<String, Vec<(String, String)>>,
    /// Each act's `Conditioned by` lines (file:line and text), for the why of an instance that holds and is suppressed.
    pub conditions: BTreeMap<String, Vec<String>>,
}

/// A type's name as an eFLINT variable's base (`String` is the type `string`).
fn base(ty: &str) -> String {
    match ty {
        "String" => "string".into(),
        "Int" => "int".into(),
        t => t.into(),
    }
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The policy of PLACES as eFLINT; TYPED: a string literal where an instance of a fact type is meant is written as
/// that instance.
pub fn to_eflint(places: &crate::policy::Places, typed: bool) -> Result<Spec, String> {
    to_eflint_without(places, typed, &BTreeSet::new())
}

/// [`to_eflint`] without the dialect files named REPLACED (by file name, the system's and a project's alike): a file
/// in eFLINT whole takes their place in this text (`// replaces:`, [`load`]). norms.rs and the decisions still read
/// them (`Places::sources` is not filtered).
fn to_eflint_without(places: &crate::policy::Places, typed: bool, replaced: &BTreeSet<String>) -> Result<Spec, String> {
    let scopes = places.sources()?;
    let file_name = |f: &str| std::path::Path::new(f).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let decls: Vec<_> = scopes.iter().flat_map(|(_, d)| d.iter()).filter(|d| !replaced.contains(&file_name(&d.at.file))).collect();
    let mut spec = Spec { text: String::new(), facts: BTreeMap::new(), acts: BTreeMap::new(), conditions: BTreeMap::new() };
    for d in &decls {
        match &d.kind {
            DeclKind::Fact { name, ids, derived } => {
                if !derived.is_empty() {
                    return Err(format!("{}: `Derived from` is not written out by this harness", d.at));
                }
                spec.facts.insert(name.clone(), ids.clone());
            }
            DeclKind::Extend { .. } => return Err(format!("{}: `Extend Fact` is not written out by this harness", d.at)),
            _ => {}
        }
    }
    // the fact types, each after the types it is identified by
    let mut done: BTreeSet<String> = BTreeSet::new();
    while done.len() < spec.facts.len() {
        let before = done.len();
        for (name, ids) in &spec.facts {
            if done.contains(name) || !ids.iter().all(|t| t == "String" || t == "Int" || done.contains(t)) {
                continue;
            }
            let by = if ids.len() == 1 { ids[0].clone() } else { ids.iter().enumerate().map(|(i, t)| format!("{}{}", base(t), i + 1)).collect::<Vec<_>>().join(" * ") };
            writeln!(spec.text, "Fact {name} Identified by {by}.").unwrap();
            done.insert(name.clone());
        }
        if done.len() == before {
            return Err(format!("fact types identified by undeclared types: {:?}", spec.facts.keys().filter(|k| !done.contains(*k)).collect::<Vec<_>>()));
        }
    }
    for d in &decls {
        if let DeclKind::Instance { fact, args } = &d.kind {
            let ids = spec.facts.get(fact).ok_or(format!("{}: +{fact}: no fact type", d.at))?;
            let vals: Vec<String> = args.iter().zip(ids).map(|(a, t)| match a {
                Expr::Str(s) => value(&spec.facts, t, s),
                Expr::Int(i) => value(&spec.facts, t, &i.to_string()),
                other => format!("{other:?}"),
            }).collect();
            writeln!(spec.text, "+{fact}({}).", vals.join(", ")).unwrap();
        }
    }
    for d in &decls {
        let DeclKind::Act { name, fields, holds, conditions } = &d.kind else { continue };
        let mut count: BTreeMap<String, usize> = BTreeMap::new();
        let mut vars: BTreeMap<String, (String, String)> = BTreeMap::new();
        for (f, ty) in fields {
            let n = count.entry(base(ty)).or_default();
            *n += 1;
            vars.insert(f.clone(), (format!("{}{}", base(ty), n), ty.clone()));
        }
        let v = |f: &String| vars[f].0.clone();
        writeln!(spec.text, "Act {name}").unwrap();
        for (i, (f, _)) in fields.iter().enumerate() {
            match (i, f.as_str()) {
                (0, _) => writeln!(spec.text, "  Actor {}", v(f)).unwrap(),
                (_, "recipient") => writeln!(spec.text, "  Recipient {}", v(f)).unwrap(),
                _ => {}
            }
        }
        let related: Vec<String> = fields.iter().enumerate().filter(|(i, (f, _))| *i > 0 && f != "recipient").map(|(_, (f, _))| v(f)).collect();
        if !related.is_empty() {
            writeln!(spec.text, "  Related to {}", related.join(", ")).unwrap();
        }
        let mut tr = Tr { facts: &spec.facts, vars: &vars, n: 0, typed };
        let hs: Vec<String> = holds.iter().map(|(at, c, _)| tr.cond(c).map_err(|e| format!("{at}: {e}"))).collect::<Result<_, _>>()?;
        if !hs.is_empty() {
            writeln!(spec.text, "  Holds when {}", hs.iter().map(|h| format!("({h})")).collect::<Vec<_>>().join(" || ")).unwrap();
        }
        for (at, c, _) in conditions {
            writeln!(spec.text, "  Conditioned by {}", tr.cond(c).map_err(|e| format!("{at}: {e}"))?).unwrap();
        }
        spec.text.push_str(".\n");
        spec.acts.insert(name.clone(), fields.clone());
        spec.conditions.insert(name.clone(), conditions.iter().map(|(at, _, t)| format!("`{t}` ({at})")).collect());
    }
    Ok(spec)
}

/// The `Conditioned by` clauses of a file in eFLINT whole, per act (`Act NAME` or `Extend Act NAME`), in its order,
/// each as "`TEXT` (KEY:LINE)"; a line's clauses are split at its top-level commas, as eFLINT reads them.
fn whole_conditions(key: &str, text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut act: Option<String> = None;
    for (i, l) in text.lines().enumerate() {
        let t = l.trim();
        if t.is_empty() || t.starts_with("//") {
            continue;
        }
        if !l.starts_with(char::is_whitespace) {
            act = t.strip_prefix("Extend Act ").or(t.strip_prefix("Act ")).and_then(|r| r.split_whitespace().next()).map(|n| n.trim_end_matches('.').to_string());
        }
        let Some(rest) = t.find("Conditioned by").map(|at| &t[at + "Conditioned by".len()..]) else { continue };
        let Some(a) = &act else { continue };
        let (mut depth, mut quoted, mut start) = (0i32, false, 0);
        let rest = rest.trim().trim_end_matches('.');
        let mut parts = Vec::new();
        for (j, c) in rest.char_indices() {
            match c {
                '"' => quoted = !quoted,
                '(' if !quoted => depth += 1,
                ')' if !quoted => depth -= 1,
                ',' if !quoted && depth == 0 => {
                    parts.push(&rest[start..j]);
                    start = j + 1;
                }
                _ => {}
            }
        }
        parts.push(&rest[start..]);
        for c in parts.into_iter().map(str::trim).filter(|c| !c.is_empty()) {
            out.push((a.clone(), format!("`{c}` ({key}:{})", i + 1)));
        }
    }
    out
}

/// The conditions of an act in the text's order: (eflint-rs's index, the clause as CLAUSES names it). eflint-rs puts
/// each `Conditioned by` clause in front of those before it (compile.rs `prepend`), keeping the order inside one
/// clause; CLAUSES are in the text's order, one clause's share the same "(FILE:LINE)". Without as many clauses as
/// conditions, eflint-rs's order and no names.
fn text_order(clauses: &[String], n: usize) -> Vec<(usize, Option<String>)> {
    if clauses.len() != n {
        return (0..n).map(|i| (i, None)).collect();
    }
    let at = |c: &String| c.rfind(" (").map(|i| c[i..].to_string()).unwrap_or_default();
    let mut groups: Vec<Vec<&String>> = Vec::new();
    for c in clauses {
        match groups.last_mut() {
            Some(g) if at(g[0]) == at(c) => g.push(c),
            _ => groups.push(vec![c]),
        }
    }
    // eflint-rs's index of each clause: the groups reversed, each in its own order
    let mut index = Vec::new();
    let mut i = 0;
    for g in groups.iter().rev() {
        index.push(i);
        i += g.len();
    }
    index.reverse();
    groups.iter().zip(index).flat_map(|(g, start)| g.iter().enumerate().map(move |(j, c)| (start + j, Some((*c).clone())))).collect()
}

/// The instance of type TY with identifier V, as eFLINT writes it: `agent("orch")`, `"lead"`, `90`.
fn value(facts: &BTreeMap<String, Vec<String>>, ty: &str, v: &str) -> String {
    match ty {
        "String" => quote(v),
        "Int" => v.parse::<i64>().map(|i| i.to_string()).unwrap_or_else(|_| quote(v)),
        t => match facts.get(t) {
            Some(ids) if ids.len() == 1 => format!("{t}({})", value(facts, &ids[0], v)),
            _ => format!("{t}({})", quote(v)),
        },
    }
}

/// A condition written as eFLINT over an act's fields (VARS: field name to its variable and type).
struct Tr<'a> {
    facts: &'a BTreeMap<String, Vec<String>>,
    vars: &'a BTreeMap<String, (String, String)>,
    n: usize,
    typed: bool,
}

/// A term: its text, the fresh variables it needs (name, type) and the facts that bind them.
struct Term {
    text: String,
    ty: Option<String>,
    fresh: Vec<String>,
    binds: Vec<String>,
}

impl Tr<'_> {
    /// A fresh variable over the instances of fact type F (eFLINT's quantifiers range over a type's instances, so
    /// `Exists int : ...` finds none: a value is reached through the fact that holds it, `waited101.int2`).
    fn fresh(&mut self, f: &str) -> String {
        self.n += 1;
        format!("{f}{}", 100 + self.n)
    }

    /// Field I's projection of a variable V over fact type F (a fact of one identifier is its value itself).
    fn proj(&self, v: &str, f: &str, i: usize) -> String {
        let ids = &self.facts[f];
        if ids.len() == 1 { v.to_string() } else { format!("{v}.{}{}", base(&ids[i]), i + 1) }
    }

    /// E as a term; HINT: the type the place it stands in wants. `_` is handled by the fact it stands in.
    fn expr(&mut self, e: &Expr, hint: Option<&str>) -> Result<Term, String> {
        let lit = |s: String, hint: Option<&str>, typed: bool, facts: &BTreeMap<String, Vec<String>>| match hint {
            Some(t) if typed && t != "String" && t != "Int" => value(facts, t, s.trim_matches('"')),
            _ => s,
        };
        Ok(match e {
            Expr::Field(f) => {
                let (v, ty) = self.vars.get(f).ok_or(format!("`{f}` is not a field"))?;
                Term { text: v.clone(), ty: Some(ty.clone()), fresh: vec![], binds: vec![] }
            }
            Expr::Str(s) => Term { text: lit(quote(s), hint, self.typed, self.facts), ty: None, fresh: vec![], binds: vec![] },
            Expr::Int(i) => Term { text: i.to_string(), ty: Some("Int".into()), fresh: vec![], binds: vec![] },
            Expr::Any => return Err("`_` stands only as a fact's argument".into()),
            Expr::App(f, args) => {
                let ids = self.facts.get(f).ok_or(format!("no fact type `{f}`"))?.clone();
                if ids.len() != args.len() + 1 {
                    return Err(format!("`{f}` read as a function takes {} arguments", ids.len() - 1));
                }
                let v = self.fresh(f);
                let (mut fresh, mut binds) = (vec![v.clone()], vec![v.clone()]);
                for (i, a) in args.iter().enumerate() {
                    if matches!(a, Expr::Any) {
                        continue;
                    }
                    let x = self.expr(a, Some(&ids[i]))?;
                    fresh.extend(x.fresh);
                    binds.extend(x.binds);
                    binds.push(format!("{} == {}", self.proj(&v, f, i), x.text));
                }
                let last = ids.len() - 1;
                Term { text: self.proj(&v, f, last), ty: Some(ids[last].clone()), fresh, binds }
            }
            Expr::Option | Expr::Call(_) => return Err("`option` and `call.FIELD` belong to decisions".into()),
        })
    }

    /// Fact F of ARGS: the bindings of its arguments' functions (outside), and the fact itself: `f(a, b)`, or with a
    /// `_` among them, `Exists f101 : f101 && f101.a1 == a`.
    fn fact(&mut self, f: &str, args: &[Expr]) -> Result<(Vec<String>, Vec<String>, String), String> {
        let ids = self.facts.get(f).ok_or(format!("no fact type `{f}`"))?.clone();
        if ids.len() != args.len() {
            return Err(format!("`{f}` has {} identifiers", ids.len()));
        }
        let (mut fresh, mut binds, mut texts) = (Vec::new(), Vec::new(), Vec::new());
        for (a, t) in args.iter().zip(&ids) {
            if matches!(a, Expr::Any) {
                texts.push(None);
                continue;
            }
            let x = self.expr(a, Some(t))?;
            fresh.extend(x.fresh);
            binds.extend(x.binds);
            texts.push(Some(x.text));
        }
        let atom = if texts.iter().all(Option::is_some) {
            format!("{f}({})", texts.iter().flatten().cloned().collect::<Vec<_>>().join(", "))
        } else {
            let v = self.fresh(f);
            let eqs: Vec<String> = std::iter::once(v.clone()).chain(texts.iter().enumerate().filter_map(|(i, t)| t.as_ref().map(|t| format!("{} == {t}", self.proj(&v, f, i))))).collect();
            exists(&[v], eqs)
        };
        Ok((fresh, binds, atom))
    }

    fn cond(&mut self, c: &Cond) -> Result<String, String> {
        let mut out = Vec::new();
        for l in c {
            out.push(match l {
                Lit::Holds(f, args) => {
                    let (fresh, binds, atom) = self.fact(f, args)?;
                    exists(&fresh, binds.into_iter().chain([atom]).collect())
                }
                Lit::Not(f, args) => {
                    // as norms.rs: a function's binding stands outside the negation, a `_` inside it
                    let (fresh, binds, atom) = self.fact(f, args)?;
                    exists(&fresh, binds.into_iter().chain([format!("Not({atom})")]).collect())
                }
                Lit::Cmp(a, op, b) => {
                    // a literal compared with a field or a function's value is of that one's type
                    let ta = self.expr(a, None)?;
                    let tb = self.expr(b, ta.ty.as_deref())?;
                    let ta = if matches!(a, Expr::Str(_)) { self.expr(a, tb.ty.as_deref())? } else { ta };
                    let op = match op {
                        Op::Eq => "==",
                        Op::Ne => "!=",
                        Op::Lt => "<",
                        Op::Le => "<=",
                        Op::Gt => ">",
                        Op::Ge => ">=",
                    };
                    let fresh: Vec<String> = ta.fresh.iter().chain(&tb.fresh).cloned().collect();
                    let binds: Vec<String> = ta.binds.into_iter().chain(tb.binds).chain([format!("{} {op} {}", ta.text, tb.text)]).collect();
                    exists(&fresh, binds)
                }
            });
        }
        Ok(out.join(" && "))
    }
}

/// `Exists VARS : BODY`, nested so that each variable is quantified only where a conjunct needs it: the conjunct
/// needing the fewest new variables goes first, its variables with it, and the rest inside. eFLINT's `Exists` tries
/// every combination of its variables' instances; nested, a combination is not tried once an outer conjunct fails
/// (`&&` stops at the first false). `family(model) == family(runs(actor))`, flat, tried family × family × runs (18 s
/// of 19.5 for every act on research's team, quick profile); nested, runs is bound by the actor first.
fn exists(vars: &[String], body: Vec<String>) -> String {
    let uses = |b: &str, v: &str| {
        b.match_indices(v).any(|(i, _)| {
            let after = b[i + v.len()..].chars().next();
            let before = b[..i].chars().last();
            !after.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-') && !before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-')
        })
    };
    let mut left: Vec<(String, Vec<String>)> = body.into_iter().map(|b| {
        let vs = vars.iter().filter(|v| uses(&b, v)).cloned().collect();
        (b, vs)
    }).collect();
    let mut bound: Vec<String> = Vec::new();
    // levels, outermost first: the variables brought in and the conjuncts that then can be asked
    let mut levels: Vec<(Vec<String>, Vec<String>)> = Vec::new();
    while !left.is_empty() {
        let new_of = |vs: &Vec<String>| vs.iter().filter(|v| !bound.contains(v)).count();
        // fewest new variables; then an equality, which binds what it brings in; then the text's order
        let (i, _) = left.iter().enumerate().min_by_key(|(i, (b, vs))| (new_of(vs), !b.contains("=="), *i)).unwrap();
        let brought: Vec<String> = vars.iter().filter(|v| left[i].1.contains(v) && !bound.contains(v)).cloned().collect();
        bound.extend(brought.iter().cloned());
        let (ready, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut left).into_iter().partition(|(_, vs)| vs.iter().all(|v| bound.contains(v)));
        left = rest;
        levels.push((brought, ready.into_iter().map(|(b, _)| b).collect()));
    }
    // a variable no conjunct names is still quantified (it must have an instance), outermost
    let unused: Vec<String> = vars.iter().filter(|v| !bound.contains(v)).cloned().collect();
    if !unused.is_empty() {
        levels.insert(0, (unused, Vec::new()));
    }
    let mut text = String::new();
    for (brought, conj) in levels.into_iter().rev() {
        let mut parts: Vec<String> = conj.iter().map(|b| format!("({b})")).collect();
        if !text.is_empty() {
            parts.push(text);
        }
        let inner = if parts.is_empty() { "True".to_string() } else { parts.join(" && ") };
        text = if brought.is_empty() { inner } else { format!("(Exists {} : {inner})", brought.join(", ")) };
    }
    text
}

/// The phrases that make FACTS hold (only the fact types the policy declares, as norms.rs is given them).
pub(crate) fn facts_text(spec: &Spec, facts: &Facts) -> String {
    let mut out = String::new();
    for (f, ids) in facts {
        let Some(types) = spec.facts.get(f).filter(|t| t.len() == ids.len()) else { continue };
        let vals: Vec<String> = ids.iter().zip(types).map(|(v, t)| value(&spec.facts, t, v)).collect();
        writeln!(out, "+{f}({}).", vals.join(", ")).unwrap();
    }
    out
}

/// The identifiers of an instance, outermost first: `notify(agent("a"), ...)` gives a's.
pub(crate) fn leaf(t: &Tagged) -> Vec<String> {
    match &t.elem {
        Elem::Str(s) => vec![s.to_string()],
        Elem::Int(i) => vec![i.to_string()],
        Elem::Prod(xs) => xs.iter().map(|x| leaf(x).join("/")).collect(),
    }
}


/// How the dialect's string literals are written where an instance of a fact type is meant: as that instance
/// (`agent1 != agent("owner")`), so a comparison is between instances of one type, as eFLINT compares.
pub const TYPED: bool = true;

/// Which engine answers a policy's acts: the project's `[project] norms`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    /// norms.rs alone: the default.
    #[default]
    Clingo,
    /// norms.rs answers; eflint-rs is asked the same beside it and each difference is journalled (said by name only).
    Both,
    /// eflint-rs answers.
    Eflint,
}

impl Mode {
    pub fn parse(s: &str) -> Result<Mode, String> {
        match s {
            "clingo" => Ok(Mode::Clingo),
            "both" => Ok(Mode::Both),
            "eflint" => Ok(Mode::Eflint),
            other => Err(format!("`norms = \"{other}\"`: one of clingo, both, eflint")),
        }
    }

    /// The mode the project file of PLACES sets (`clingo` when it sets none, or when there is no project file).
    pub fn of(places: &crate::policy::Places) -> Mode {
        let Some(dir) = &places.project else { return Mode::default() };
        let Ok(text) = std::fs::read_to_string(format!("{dir}/project.toml")) else { return Mode::default() };
        let v: toml::Value = match toml::from_str(&text) {
            Ok(v) => v,
            Err(_) => return Mode::default(),
        };
        match v.get("project").and_then(|p| p.get("norms")).and_then(|n| n.as_str()).map(Mode::parse) {
            Some(Ok(m)) => m,
            Some(Err(e)) => {
                eprintln!("coordd: norms: {dir}/project.toml: {e}; clingo");
                Mode::default()
            }
            None => Mode::default(),
        }
    }
}

/// Whether KEY is said in the journal for the first time in this process (and from now on, said).
fn journalled_once(key: &str) -> bool {
    static SAID: std::sync::Mutex<BTreeSet<String>> = std::sync::Mutex::new(BTreeSet::new());
    SAID.lock().map(|mut s| s.insert(key.to_string())).unwrap_or(false)
}

/// The policy of PLACES as one eFLINT specification: the dialect's files written out, then the library's files in
/// eFLINT whole (`AGENT_BASE/policy/eflint/`), then the project's (`PROJECT/policy/eflint/`).
pub fn load(places: &crate::policy::Places) -> Result<Spec, String> {
    // the files in eFLINT whole, in order: the library's, then the project's
    let dirs = std::iter::once(format!("{}/policy/eflint", places.base)).chain(places.project.iter().map(|p| format!("{p}/policy/eflint")));
    let mut whole: Vec<(String, String)> = Vec::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "eflint")).collect();
        files.sort();
        for f in files {
            let key = f.display().to_string();
            let text = match places.overlay.get(&key) {
                Some(t) => t.clone(),
                None => std::fs::read_to_string(&f).map_err(|e| format!("{key}: {e}"))?,
            };
            whole.push((key, text));
        }
    }
    // a file whose first line is `// replaces: A B` takes the place of the dialect's files A and B in this text
    // (part 4's library, teams.eflint and slow.eflint); each must be a dialect file of the system or the project
    let dialect_files: BTreeSet<String> = std::iter::once(format!("{}/policy", places.base))
        .chain(places.project.iter().map(|p| format!("{p}/policy")))
        .flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten())
        .filter(|e| e.path().extension().is_some_and(|x| x == "eflint"))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    let mut replaced: BTreeSet<String> = BTreeSet::new();
    for (key, text) in &whole {
        let Some(names) = text.lines().next().and_then(|l| l.trim().strip_prefix("// replaces:")) else { continue };
        for n in names.split_whitespace() {
            if !dialect_files.contains(n) {
                return Err(format!("{key}: `// replaces: {n}`: no dialect policy file {n} (in policy/ of the system or the project)"));
            }
            // a project's own file of that name is left out too: its rules are not asked of eflint-rs (norms.rs still
            // asks them), said once per file in the journal so it is not lost without a word (consult-3, N-578 p6)
            if let Some(p) = &places.project {
                let own = format!("{p}/policy/{n}");
                if std::path::Path::new(&own).exists() && journalled_once(&own) {
                    eprintln!("coordd: norms: {own} is left out of the eFLINT text: {key} replaces {n}; write its rules as `Extend` in {p}/policy/eflint/");
                }
            }
            replaced.insert(n.to_string());
        }
    }
    let mut spec = to_eflint_without(places, TYPED, &replaced)?;
    let dialect: BTreeSet<String> = spec.facts.keys().cloned().collect();
    for (key, text) in whole {
        // a type of the dialect's files declared again would replace theirs (and drop an `Extend` before it):
        // they are its one home while they are loaded (consult-lead and docket-core2, N-568 p46-p50)
        for l in text.lines() {
            let Some(name) = l.trim_start().strip_prefix("Fact ").and_then(|r| r.split_whitespace().next()) else { continue };
            let name = name.trim_end_matches('.');
            if dialect.contains(name) {
                return Err(format!("{key}: `Fact {name}` is declared by the dialect's policy files already (prelude.eflint and the rest of policy/), its one home while they load"));
            }
        }
        // and an act a dialect file still declares (one it does not replace) is declared twice
        for l in text.lines() {
            let Some(name) = l.trim_start().strip_prefix("Act ").and_then(|r| r.split_whitespace().next()) else { continue };
            let name = name.trim_end_matches('.');
            if spec.acts.contains_key(name) {
                return Err(format!("{key}: `Act {name}` is declared by the dialect's policy files already; name the file declaring it on this file's `// replaces:` line, or extend it with `Extend Act {name}`"));
            }
        }
        for (act, clause) in whole_conditions(&key, &text) {
            spec.conditions.entry(act).or_default().push(clause);
        }
        writeln!(spec.text, "// {key}").unwrap();
        spec.text.push_str(&text);
        if !text.ends_with('\n') {
            spec.text.push('\n');
        }
    }
    // the acts the whole-eFLINT files declare, by their fields as eflint-rs compiled them
    let r = Replay::from_text(&spec.text).map_err(|e| format!("eflint-rs does not load the policy: {e}"))?;
    // and the facts and acts the whole-eFLINT files declare, with their identifiers' types as eflint-rs compiled them
    use eflint::ast::{Domain, Kind};
    let ty = |d: &str| match d {
        "string" => "String".to_string(),
        "int" => "Int".to_string(),
        other => other.to_string(),
    };
    for (name, t) in &r.cfg.spec.decls {
        let name = name.to_string();
        if name == "int" || name == "string" || spec.facts.contains_key(&name) || spec.acts.contains_key(&name) {
            continue;
        }
        let ids: Vec<String> = match &t.domain {
            Domain::AnyString | Domain::Strings(_) => vec!["String".into()],
            Domain::AnyInt | Domain::Ints(_) => vec!["Int".into()],
            Domain::Products(vs) => vs.iter().map(|v| ty(&v.dom)).collect(),
            Domain::Time => vec!["Int".into()],
        };
        match t.kind {
            Kind::Act { .. } => {
                let fields = ids.iter().enumerate().map(|(i, t)| (if i == 0 { "actor".to_string() } else { t.clone() }, t.clone())).collect();
                spec.acts.insert(name, fields);
            }
            Kind::Fact { .. } => {
                spec.facts.insert(name, ids);
            }
            _ => {}
        }
    }
    Ok(spec)
}

/// What eflint-rs says of one act instance (docket-core5's `ActAnswer`): enabled, or holding and suppressed, why.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct Answer {
    pub act: String,
    /// The identifiers of its fields, in order, the actor first.
    pub objects: Vec<String>,
    pub enabled: bool,
    pub why: Option<String>,
}

/// What performing an act did: its effects (the facts it created and ended), whether it went through although not
/// enabled (`disabled`: eFLINT reports the violation, it does not prevent it), and the errors that stopped it.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct Effects {
    pub ok: bool,
    pub errors: Vec<String>,
    pub created: Vec<String>,
    pub terminated: Vec<String>,
    pub disabled: bool,
}

impl Spec {
    /// A state with FACTS stated: the specification loaded and the facts' phrases run.
    fn state(&self, facts: &Facts) -> Result<Replay, String> {
        let mut r = Replay::from_text(&self.text)?;
        let a = r.apply(&Event::Phrase(facts_text(self, facts)));
        if !a.ok || !a.errors.is_empty() {
            return Err(format!("eflint-rs: {}", a.errors.join("; ")));
        }
        Ok(r)
    }

    /// Every act instance that holds in FACTS (ACTOR: whose actor is ACTOR), enabled or suppressed.
    pub fn answers(&self, facts: &Facts, actor: Option<&str>) -> Result<Vec<Answer>, String> {
        self.answers_for(facts, None, actor, true)
    }

    /// [`Spec::answers`] for ACTS only (None: every act), with the refused rows or not (the `both` comparison reads
    /// only the enabled). The facts are stated with the acts left underived (`Replay::perform`, as a record replayed
    /// for one question), then the asked acts and what they read are derived once: stating them with `apply` derived
    /// every act and listed every transition after each fact's line (on research's team, 293 lines: 692 s in the quick
    /// profile, where deriving every act once takes 19 s; docket-core8, N-582).
    pub fn answers_for(&self, facts: &Facts, acts: Option<&[String]>, actor: Option<&str>, with_refused: bool) -> Result<Vec<Answer>, String> {
        use eflint::eval::{lazy_types, needed_lazy, saturate_only, Ev};
        let mut r = Replay::from_text(&self.text)?;
        let a = r.perform(&Event::Phrase(facts_text(self, facts)));
        if !a.ok || !a.errors.is_empty() {
            return Err(format!("eflint-rs: {}", a.errors.join("; ")));
        }
        let wanted: BTreeSet<eflint::ast::Id> =
            self.acts.keys().filter(|k| acts.is_none_or(|a| a.iter().any(|x| x == *k))).map(|k| eflint::ast::id(k)).collect();
        let err = |e: eflint::eval::RtErr| format!("eflint-rs: {}", e.message());
        let only = needed_lazy(&r.cfg.spec, &lazy_types(&r.cfg.spec), wanted.clone());
        r.cfg.state = saturate_only(&r.cfg.spec, r.cfg.state.clone(), &only).map_err(err)?;
        let mut out = Vec::new();
        {
            let mut ev = Ev::new(&r.cfg.spec, &r.cfg.state);
            for te in r.cfg.state.holding().filter(|te| wanted.contains(&te.dom)) {
                let objects = leaf(te);
                if actor.is_some_and(|x| objects.first().is_none_or(|o| o != x)) {
                    continue;
                }
                let ens = match ev.is_enabled(te) {
                    Ok(e) => e,
                    Err(e) if e.is_missing_input() => continue,
                    Err(e) => return Err(err(e)),
                };
                for en in ens {
                    let act = te.dom.to_string();
                    let why = (!en).then(|| self.suppressed_why(&act));
                    out.push(Answer { act, objects: objects.clone(), enabled: en, why });
                }
            }
        }
        if with_refused {
            out.extend(self.refused(&r, actor, &wanted)?);
        }
        out.sort();
        out.dedup();
        Ok(out)
    }

    /// The instances a `Conditioned by` refuses: eflint-rs derives no instance whose conditions fail, so each act's
    /// instances are derived again with its conditions left out, and each condition is asked alone of those that do
    /// not hold; the first that fails, in the text's order, is the why.
    fn refused(&self, r: &Replay, actor: Option<&str>, wanted: &BTreeSet<eflint::ast::Id>) -> Result<Vec<Answer>, String> {
        use eflint::eval::{saturate_only, Ev};
        use std::rc::Rc;
        let spec = &r.cfg.spec;
        let mut relaxed = spec.clone();
        let mut only = BTreeSet::new();
        for d in wanted {
            let d = d.clone();
            if let Some(t) = relaxed.decls.get_mut(&d).filter(|t| !t.conditions.is_empty()) {
                Rc::make_mut(t).conditions.clear();
                only.insert(d);
            }
        }
        if only.is_empty() {
            return Ok(Vec::new());
        }
        let err = |e: eflint::eval::RtErr| format!("eflint-rs: {}", e.message());
        let st = saturate_only(&relaxed, r.cfg.state.clone(), &only).map_err(err)?;
        // one specification per act and condition, with that condition alone, built when first asked
        let mut alone: BTreeMap<(eflint::ast::Id, usize), eflint::compile::Spec> = BTreeMap::new();
        let mut out = Vec::new();
        for te in st.holding().filter(|te| only.contains(&te.dom) && !r.cfg.state.holds(te)) {
            let objects = leaf(te);
            if actor.is_some_and(|x| objects.first().is_none_or(|o| o != x)) {
                continue;
            }
            let conds = &spec.decls[&te.dom].conditions;
            let act = te.dom.to_string();
            let order = text_order(self.conditions.get(&act).map(Vec::as_slice).unwrap_or_default(), conds.len());
            let mut failed = None;
            for &(i, ref clause) in &order {
                let c = &conds[i];
                let one = alone.entry((te.dom.clone(), i)).or_insert_with(|| {
                    let mut one = relaxed.clone();
                    Rc::make_mut(one.decls.get_mut(&te.dom).unwrap()).conditions = vec![c.clone()];
                    one
                });
                if Ev::new(one, &st).sat_conditions(te).map_err(err)?.iter().any(|b| !b) {
                    failed = Some((i, clause.clone()));
                    break;
                }
            }
            let Some((i, clause)) = failed else { continue };
            let why = match clause {
                Some(c) => format!("{act} holds and a `Conditioned by` fails: {c}"),
                None => format!("{act} holds and a `Conditioned by` fails: its condition {} of {}", i + 1, conds.len()),
            };
            out.push(Answer { act, objects, enabled: false, why: Some(why) });
        }
        Ok(out)
    }

    fn suppressed_why(&self, act: &str) -> String {
        match self.conditions.get(act) {
            Some(c) if !c.is_empty() => format!("{act} holds and is suppressed: a `Conditioned by` fails: {}", c.join("; ")),
            _ => format!("{act} holds and is suppressed"),
        }
    }

    /// Act ACT with field identifiers ARGS in FACTS: enabled or not and why, as norms.rs's [`Verdict`]; None when the
    /// policy has no such act.
    pub fn verdict(&self, facts: &Facts, act: &str, args: &[&str]) -> Result<Option<crate::policy::norms::Verdict>, String> {
        if !self.acts.contains_key(act) {
            return Ok(None);
        }
        let answers = self.answers_for(facts, Some(&[act.to_string()]), None, true)?;
        let mut v = verdict_of(&answers, act, args);
        if !v.enabled && v.why.ends_with("fails") {
            if let Some(c) = self.conditions.get(act).filter(|c| !c.is_empty()) {
                v.why = format!("{}: {}", v.why, c.join("; "));
            }
        }
        Ok(Some(v))
    }

    /// Perform ACT by ACTOR on OBJECTS (the other fields' identifiers, in order) in FACTS: its effects.
    pub fn perform(&self, facts: &Facts, act: &str, actor: &str, objects: &[&str]) -> Result<Effects, String> {
        use eflint::replay::Value;
        let mut r = self.state(facts)?;
        let fields = self.acts.get(act).ok_or(format!("the policy has no act {act}"))?;
        let typed = |ty: Option<&String>, v: &str| match ty.map(String::as_str) {
            Some("Int") => v.parse().map(Value::Int).unwrap_or_else(|_| Value::Str(v.into())),
            Some("String") => Value::Str(v.into()),
            None => v.parse().map(Value::Int).unwrap_or_else(|_| Value::Str(v.into())),
            // a type over Int (`Fact amount Identified by Int`) is given its number, not the string
            Some(t) if self.facts.get(t).is_some_and(|f| f.len() == 1 && f[0] == "Int") => {
                Value::Typed(t.into(), Box::new(v.parse().map(Value::Int).unwrap_or_else(|_| Value::Str(v.into()))))
            }
            Some(t) => Value::Typed(t.into(), Box::new(Value::Str(v.into()))),
        };
        let actor_v = typed(fields.first().map(|(_, t)| t), actor);
        let objs = objects.iter().enumerate().map(|(i, o)| typed(fields.get(i + 1).map(|(_, t)| t), o)).collect();
        let a = r.apply(&Event::Act { name: act.into(), actor: actor_v, objects: objs });
        Ok(Effects { ok: a.ok && a.errors.is_empty(), errors: a.errors, created: a.created.iter().map(|t| t.to_string()).collect(), terminated: a.terminated.iter().map(|t| t.to_string()).collect(), disabled: !a.disabled.is_empty() })
    }
}

/// A duty that holds (docket-edge6's `DutyAnswer`, docket-core5's summary duty): its holder and claimant, whether it
/// is violated (`Violated when`, in the state as stated), and the acts that end it (`Terminated by`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct Duty {
    pub duty: String,
    pub objects: Vec<String>,
    pub holder: Option<String>,
    pub claimant: Option<String>,
    pub violated: bool,
    pub ended_by: Vec<String>,
}

impl Spec {
    /// The duties that hold in FACTS.
    pub fn duties(&self, facts: &Facts) -> Result<Vec<Duty>, String> {
        let r = self.state(facts)?;
        let mut out: Vec<Duty> = r
            .duties()
            .into_iter()
            .map(|d| {
                let ended_by = match r.cfg.spec.find(&d.duty.dom).map(|t| &t.kind) {
                    Some(eflint::ast::Kind::Duty { terminating, .. }) => terminating.iter().map(|i| i.to_string()).collect(),
                    _ => Vec::new(),
                };
                Duty { duty: d.duty.dom.to_string(), objects: leaf(&d.duty), holder: d.holder.as_ref().map(|h| leaf(h).join("/")), claimant: d.claimant.as_ref().map(|c| leaf(c).join("/")), violated: d.violated, ended_by }
            })
            .collect();
        out.sort();
        out.dedup();
        Ok(out)
    }
}

/// The verdict on ACT(ARGS) among ANSWERS (every instance that holds).
pub fn verdict_of(answers: &[Answer], act: &str, args: &[&str]) -> crate::policy::norms::Verdict {
    match answers.iter().find(|a| a.act == act && a.objects.iter().map(String::as_str).eq(args.iter().copied())) {
        Some(a) if a.enabled => crate::policy::norms::Verdict { enabled: true, why: format!("{act} holds and is not suppressed") },
        Some(a) => crate::policy::norms::Verdict { enabled: false, why: a.why.clone().unwrap_or_default() },
        // eflint-rs (as the reference) derives no instance whose `Conditioned by` fails: it has no row of its own
        None => crate::policy::norms::Verdict { enabled: false, why: format!("{act} does not hold here: no `Holds when` of it holds, or a `Conditioned by` fails") },
    }
}

/// What the callers of norms.rs load in its place (`acts`): norms.rs and the interpreter as the project's [`Mode`]
/// has them, with the same calls (`solve_for`, `solve`, `verdict`, `acts`).
pub struct Acts {
    pub mode: Mode,
    places: crate::policy::Places,
    norms: Option<crate::policy::norms::Norms>,
    spec: Option<Spec>,
    /// The acts and their fields.
    pub acts: BTreeMap<String, crate::policy::norms::Act>,
}

/// A situation solved: norms.rs's atoms, or eflint-rs's instances that hold.
pub struct Shown {
    norms: Option<std::collections::HashSet<String>>,
    answers: Vec<Answer>,
}

/// The policy's acts for PLACES, as its mode has them.
pub fn acts(places: &crate::policy::Places) -> Result<Acts, String> {
    let mode = Mode::of(places);
    if mode == Mode::Eflint {
        let spec = load(places)?;
        let acts = spec.acts.iter().map(|(a, f)| (a.clone(), crate::policy::norms::Act { fields: f.clone(), holds: Vec::new(), conditions: Vec::new() })).collect();
        return Ok(Acts { mode, places: places.clone(), norms: None, spec: Some(spec), acts });
    }
    let n = crate::policy::norms::load(places)?;
    Ok(Acts { mode, places: places.clone(), acts: n.acts.clone(), norms: Some(n), spec: None })
}

impl Acts {
    /// Every act's standing in FACTS.
    pub fn solve(&self, facts: &[(String, Vec<String>)]) -> Result<Shown, String> {
        self.solved(None, facts)
    }

    /// The standing of ACTS (and those they name) in FACTS.
    pub fn solve_for(&self, acts: &[&str], facts: &[(String, Vec<String>)]) -> Result<Shown, String> {
        self.solved(Some(acts), facts)
    }

    fn solved(&self, acts: Option<&[&str]>, facts: &[(String, Vec<String>)]) -> Result<Shown, String> {
        match (&self.norms, &self.spec) {
            (Some(n), _) => {
                let started = std::time::Instant::now();
                let s = match acts {
                    Some(a) => n.solve_for(a, facts)?,
                    None => n.solve(facts)?,
                };
                if self.mode == Mode::Both {
                    let took = started.elapsed();
                    let asked: Vec<String> = acts.map(|a| a.iter().map(|s| s.to_string()).collect()).unwrap_or_else(|| n.acts.keys().cloned().collect());
                    compare_later(self.places.clone(), asked, facts.to_vec(), norms_enabled(n, &s), took);
                }
                Ok(Shown { norms: Some(s), answers: Vec::new() })
            }
            (None, Some(spec)) => Ok(Shown { norms: None, answers: spec.answers(&facts.to_vec(), None)? }),
            (None, None) => Err("no engine for the policy's acts".into()),
        }
    }

    /// The duties that hold in FACTS, by eflint-rs whatever the mode (norms.rs has no duties); none when eflint-rs
    /// cannot load the policy in `clingo` or `both` (said on stderr), an error in `eflint`.
    pub fn duties(&self, facts: &[(String, Vec<String>)]) -> Result<Vec<Duty>, String> {
        let spec = match &self.spec {
            Some(s) => s.clone(),
            None => match load(&self.places) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("coordd: norms: duties: {e}; none");
                    return Ok(Vec::new());
                }
            },
        };
        spec.duties(&facts.to_vec())
    }

    /// The instances of SHOWN whose actor is ACTOR, enabled or greyed with why (`coord can`, the window's menu):
    /// norms.rs's where it answers, eflint-rs's in `eflint`.
    pub fn instances(&self, shown: &Shown, actor: &str) -> Vec<coord_types::ActAnswer> {
        if let (Some(n), Some(s)) = (&self.norms, &shown.norms) {
            return n.instances(s, actor);
        }
        let mut out: Vec<coord_types::ActAnswer> = shown
            .answers
            .iter()
            .filter(|a| a.objects.first().is_some_and(|o| o == actor))
            .map(|a| coord_types::ActAnswer { act: a.act.clone(), objects: a.objects[1..].to_vec(), enabled: a.enabled, why: a.why.clone(), says: None })
            .collect();
        out.sort_by(|x, y| (&x.act, &x.objects).cmp(&(&y.act, &y.objects)));
        out
    }

    /// Act ACT with field identifiers ARGS as SHOWN has it; None when the policy has no such act.
    pub fn verdict(&self, shown: &Shown, act: &str, args: &[&str]) -> Result<Option<crate::policy::norms::Verdict>, String> {
        match (&self.norms, &shown.norms) {
            (Some(n), Some(s)) => n.verdict(s, act, args),
            _ if !self.acts.contains_key(act) => Ok(None),
            _ => Ok(Some(verdict_of(&shown.answers, act, args))),
        }
    }
}

/// One `both` comparison at a time; one asked while another runs is not asked (journalled as a count).
static COMPARING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static SKIPPED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// How long a comparison runs before the journal says so, in seconds (60; a test sets it lower).
static LONG_SECS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(60);
#[cfg(test)]
static SAID: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// One journal line of `both` (kept for the tests too).
fn journal(line: String) {
    eprintln!("{line}");
    #[cfg(test)]
    SAID.lock().unwrap_or_else(|e| e.into_inner()).push(line);
}

/// A difference on a text where a rule reads a derived type through `Not`, `Forall`, an aggregate, `Violated` or
/// `Enabled` may be the two semantics, not a fault of either: eflint-rs derives as the eFLINT reference does, in the
/// declarations' name order with nothing taken back; norms.rs answers by Clingo's stable models, the semantics the
/// language's authors moved to (GPCE 2025; paper-research, N-568.p81).
///
/// `both`: eflint-rs asked about ASKED in FACTS on a thread of its own, its enabled instances compared with norms.rs's
/// OURS; each difference one journal line. The caller has norms.rs's answer already and does not wait.
fn compare_later(places: crate::policy::Places, asked: Vec<String>, facts: Facts, ours: BTreeSet<(String, Vec<String>)>, norms_took: std::time::Duration) {
    use std::sync::atomic::Ordering;
    TIMING.lock().unwrap_or_else(|e| e.into_inner()).norms.add(norms_took);
    if COMPARING.swap(true, Ordering::AcqRel) {
        SKIPPED.fetch_add(1, Ordering::Relaxed);
        timing_line();
        return;
    }
    // a comparison still running after a minute says so once, with its size, so a thread that burns is seen at once;
    // the watcher ends as soon as the comparison does (its sender dropped; consult-3, N-582.p43)
    let size = format!("{} facts, {} acts asked", facts.len(), asked.len());
    let watched = size.clone();
    let long = std::time::Duration::from_secs(LONG_SECS.load(Ordering::Relaxed));
    let (done, ended) = std::sync::mpsc::channel::<()>();
    let _ = std::thread::Builder::new().name("norms-both-watch".into()).spawn(move || {
        if let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = ended.recv_timeout(long) {
            journal(format!("coordd: norms both: a comparison has run over {} s and goes on ({watched}); none other is started until it ends", long.as_secs()));
        }
    });
    let spawned = std::thread::Builder::new().name("norms-both".into()).spawn(move || {
        let started = std::time::Instant::now();
        let (said, took) = compare_timed(&places, &asked, &facts, &ours);
        drop(done);
        if started.elapsed() >= long {
            journal(format!("coordd: norms both: a comparison took {} s ({size})", started.elapsed().as_secs()));
        }
        if let Some(t) = took {
            TIMING.lock().unwrap_or_else(|e| e.into_inner()).eflint.add(t);
        }
        timing_line();
        if let Some(line) = said {
            let skipped = SKIPPED.swap(0, Ordering::Relaxed);
            eprintln!("coordd: norms both: {line}{}", if skipped > 0 { format!(" ({skipped} questions not compared while one ran)") } else { String::new() });
        }
        COMPARING.store(false, Ordering::Release);
    });
    if spawned.is_err() {
        COMPARING.store(false, Ordering::Release);
    }
}

/// The comparison of [`compare_later`]: None when eflint-rs enables exactly the instances norms.rs does; a panic of
/// eflint-rs is a line too, never the caller's.
pub fn compare(places: &crate::policy::Places, asked: &[String], facts: &Facts, ours: &BTreeSet<(String, Vec<String>)>) -> Option<String> {
    compare_timed(places, asked, facts, ours).0
}

/// [`compare`], with how long eflint-rs took to answer (the policy loaded already, as `eflint` has it cached).
fn compare_timed(places: &crate::policy::Places, asked: &[String], facts: &Facts, ours: &BTreeSet<(String, Vec<String>)>) -> (Option<String>, Option<std::time::Duration>) {
    let mut took = None;
    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<BTreeSet<(String, Vec<String>)>, String> {
        let spec = load(places)?;
        let started = std::time::Instant::now();
        let answers = spec.answers_for(facts, Some(asked), None, false)?;
        took = Some(started.elapsed());
        Ok(answers.into_iter().filter(|a| a.enabled && asked.contains(&a.act)).map(|a| (a.act, a.objects)).collect())
    }));
    (compare_said(run, asked, facts, ours), took)
}

fn compare_said(run: std::thread::Result<Result<BTreeSet<(String, Vec<String>)>, String>>, asked: &[String], facts: &Facts, ours: &BTreeSet<(String, Vec<String>)>) -> Option<String> {
    let ours: BTreeSet<_> = ours.iter().filter(|(a, _)| asked.contains(a)).cloned().collect();
    let show = |s: Vec<&(String, Vec<String>)>| s.iter().take(8).map(|(a, v)| format!("{a}({})", v.join(", "))).collect::<Vec<_>>().join(" ");
    match run {
        Ok(Ok(theirs)) if theirs == ours => None,
        Ok(Ok(theirs)) => Some(format!("{} in a situation of {} facts: norms.rs only [{}], eflint-rs only [{}]", asked.join(", "), facts.len(), show(ours.difference(&theirs).collect()), show(theirs.difference(&ours).collect()))),
        Ok(Err(e)) => Some(format!("{}: eflint-rs could not be asked: {e}", asked.join(", "))),
        Err(_) => Some(format!("{}: eflint-rs panicked", asked.join(", "))),
    }
}

/// How long the questions of `both` took, per engine, in buckets of milliseconds; one journal line a minute.
#[derive(Default)]
struct Took {
    /// under 10, 50, 200, 1000 ms, and the rest
    buckets: [u64; 5],
    max: std::time::Duration,
}

impl Took {
    const EDGES: [u128; 4] = [10, 50, 200, 1000];
    fn add(&mut self, t: std::time::Duration) {
        let i = Self::EDGES.iter().position(|e| t.as_millis() < *e).unwrap_or(4);
        self.buckets[i] += 1;
        self.max = self.max.max(t);
    }
    fn line(&self) -> String {
        let n: u64 = self.buckets.iter().sum();
        // the median's bucket: the first whose running count reaches half
        let mut run = 0;
        let names = ["<10", "10-50", "50-200", "200-1000", ">=1000"];
        let median = self.buckets.iter().zip(names).find(|(b, _)| {
            run += **b;
            run * 2 >= n && n > 0
        });
        format!(
            "{n} asked, median {} ms, worst {} ms, {} over 50 ms [{}]",
            median.map_or("-", |(_, m)| m),
            self.max.as_millis(),
            self.buckets[2..].iter().sum::<u64>(),
            self.buckets.iter().zip(names).map(|(b, m)| format!("{m}: {b}")).collect::<Vec<_>>().join(", ")
        )
    }
}

struct Timing {
    since: Option<std::time::Instant>,
    norms: Took,
    eflint: Took,
}

static TIMING: std::sync::Mutex<Timing> = std::sync::Mutex::new(Timing { since: None, norms: Took { buckets: [0; 5], max: std::time::Duration::ZERO }, eflint: Took { buckets: [0; 5], max: std::time::Duration::ZERO } });

/// Once a minute: the journal line of how long `both`'s questions took (norms.rs on the caller's path, eflint-rs on
/// its own thread, not under the core's lock today; under it once `eflint` answers).
fn timing_line() {
    let mut t = TIMING.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::Instant::now();
    let since = *t.since.get_or_insert(now);
    if now.duration_since(since) < std::time::Duration::from_secs(60) {
        return;
    }
    eprintln!("coordd: norms both took, last {} s: norms.rs {}; eflint-rs {}", now.duration_since(since).as_secs(), t.norms.line(), t.eflint.line());
    *t = Timing { since: Some(now), norms: Took::default(), eflint: Took::default() };
}

/// The act instances norms.rs has enabled in SHOWN (its atoms `in((enabled,ACT(..)),1)`).
pub fn norms_enabled(n: &crate::policy::norms::Norms, shown: &std::collections::HashSet<String>) -> BTreeSet<(String, Vec<String>)> {
    let preds: BTreeMap<String, String> = n.acts.keys().map(|a| (crate::policy::compile::pred(a), a.clone())).collect();
    let mut out = BTreeSet::new();
    for s in shown {
        let Some(inner) = s.strip_prefix("in((enabled,").and_then(|r| r.strip_suffix("),1)")) else { continue };
        let Some((p, rest)) = inner.split_once('(') else {
            if let Some(a) = preds.get(inner) {
                out.insert((a.clone(), vec![]));
            }
            continue;
        };
        let Some(act) = preds.get(p) else { continue };
        out.insert((act.clone(), terms(rest.strip_suffix(')').unwrap_or(rest))));
    }
    out
}

/// A clingo term list's values: `"a","b",3`.
fn terms(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut q, mut esc) = (Vec::new(), String::new(), false, false);
    for c in s.chars() {
        match (q, esc, c) {
            (true, true, _) => {
                cur.push(c);
                esc = false;
            }
            (true, false, '\\') => esc = true,
            (_, false, '"') => q = !q,
            (false, _, ',') => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::Places;

    fn agent_base() -> String {
        format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"))
    }

    /// The team's situation of norms.rs's own test (an_act_asked_alone_stands_as_in_the_whole_program).
    fn team() -> Facts {
        let mut facts: Facts = Vec::new();
        let mut f = |n: &str, v: &[&str]| facts.push((n.into(), v.iter().map(|s| s.to_string()).collect()));
        for (a, r) in [("orch", "lead"), ("dev", "member"), ("owner", "owner"), ("cons", "consultant")] {
            f("agent", &[a]);
            f("role", &[a, r]);
        }
        f("parent", &["dev", "orch"]);
        f("below", &["dev", "orch"]);
        f("above", &["orch", "dev"]);
        for t in ["none", "t"] {
            f("thread", &[t]);
        }
        for t in ["none", "t", "N-1", "P-2", "new"] {
            f("ticket", &[t]);
        }
        f("brought-in", &["dev", "t"]);
        f("takes-part", &["dev", "t"]);
        f("takes-part", &["cons", "N-1"]);
        f("ticket-kind", &["N-1", "thread"]);
        f("ticket-kind", &["new", "item"]);
        f("opener", &["N-1", "orch"]);
        f("awaits", &["P-2", "dev"]);
        f("waited", &["P-2", "190"]);
        f("event", &["post"]);
        f("model", &["claude-haiku-5-5"]);
        f("model", &["claude-opus-5-5"]);
        f("family", &["claude-haiku-5-5", "claude"]);
        f("family", &["claude-opus-5-5", "claude"]);
        f("runs", &["orch", "claude-opus-5-5"]);
        f("granted", &["orch", "3"]);
        f("used", &["orch", "1"]);
        f("source", &["orch"]);
        f("scope", &["project"]);
        facts
    }

    /// agent-base's acts, in eFLINT whole through eflint-rs, enable exactly the instances norms.rs enables.
    #[test]
    fn agent_bases_acts_answer_in_eflint_rs_as_in_norms_rs() {
        let places = Places { base: agent_base(), ..Default::default() };
        let n = crate::policy::norms::load(&places).unwrap();
        let facts: Facts = team().into_iter().filter(|(k, _)| n.facts.contains_key(k)).collect();
        let ours = norms_enabled(&n, &n.solve(&facts).unwrap());
        assert!(ours.iter().any(|(a, _)| a == "expired"), "{ours:?}");
        let asked: Vec<String> = n.acts.keys().cloned().collect();
        assert_eq!(compare(&places, &asked, &facts, &ours), None);
        // the facade in `eflint` answers each instance as norms.rs does
        let spec = load(&places).unwrap();
        let answers = spec.answers(&facts, None).unwrap();
        for (act, args) in &ours {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            assert!(verdict_of(&answers, act, &args).enabled, "{act}{args:?}");
        }
        assert!(!verdict_of(&answers, "esc", &["dev", "orch"]).enabled, "an agent does not stop the one above it");
        assert!(answers.iter().filter(|a| a.objects.first().is_some_and(|o| o == "orch")).count() <= answers.len());
    }

    /// A file in eFLINT whole that declares a type of the dialect's files again is refused with its name.
    #[test]
    fn a_dialect_type_declared_again_is_refused() {
        let root = std::env::temp_dir().join(format!("interp-twice-{}", std::process::id()));
        std::fs::create_dir_all(root.join("policy/eflint")).unwrap();
        std::fs::write(root.join("policy/t.eflint"), "Fact agent Identified by String\n").unwrap();
        std::fs::write(root.join("policy/eflint/lib.eflint"), "Fact agent Identified by String.\n").unwrap();
        let places = Places { base: root.display().to_string(), ..Default::default() };
        let e = load(&places).unwrap_err();
        assert!(e.contains("lib.eflint") && e.contains("`Fact agent`"), "{e}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A file in eFLINT whole whose first line is `// replaces: A` takes the place of dialect file A in part 3's text
    /// (a project's A too), its acts with its own fields; a replaced name that is no dialect file is an error; an act
    /// a dialect file still declares, declared again, is refused (part 4, docket-core8 on N-731).
    #[test]
    fn a_file_that_replaces_a_dialect_file_takes_its_place() {
        let root = std::env::temp_dir().join(format!("interp-replaces-{}", std::process::id()));
        let (base, project) = (root.join("base"), root.join("project"));
        std::fs::create_dir_all(base.join("policy/eflint")).unwrap();
        std::fs::create_dir_all(project.join("policy")).unwrap();
        std::fs::write(base.join("policy/prelude.eflint"), "Fact agent Identified by String\nFact role Identified by agent * String\n").unwrap();
        std::fs::write(base.join("policy/teams.eflint"), "Act ask-team\n  Actor agent\n  Holds when role(actor, \"lead\")\n").unwrap();
        std::fs::write(project.join("policy/teams.eflint"), "Act ask-team\n  Actor agent\n  Holds when role(actor, \"member\")\n").unwrap();
        std::fs::write(base.join("policy/other.eflint"), "Act esc\n  Actor agent\n  Recipient agent\n  Holds when role(actor, \"fixer\")\n").unwrap();
        let lib = base.join("policy/eflint/lib.eflint");
        std::fs::write(&lib, "// replaces: teams.eflint\nAct ask-team\n  Actor agent1\n  Recipient agent2\n  Holds when role(agent1, \"owner\").\n").unwrap();
        let places = Places { base: base.display().to_string(), project: Some(project.display().to_string()), ..Default::default() };
        let spec = load(&places).unwrap();
        assert_eq!(spec.acts["ask-team"], vec![("actor".to_string(), "agent".to_string()), ("agent".to_string(), "agent".to_string())], "the library's fields");
        assert!(!spec.text.contains("\"lead\"") && !spec.text.contains("\"member\""), "neither teams.eflint is written out:\n{}", spec.text);
        assert!(spec.acts.contains_key("esc"), "a file not replaced stays");
        let facts: Facts = vec![("agent".into(), vec!["o".into()]), ("agent".into(), vec!["d".into()]), ("role".into(), vec!["o".into(), "owner".into()])];
        let answers = spec.answers(&facts, Some("o")).unwrap();
        assert!(answers.iter().any(|a| a.act == "ask-team" && a.objects == ["o", "d"] && a.enabled), "{answers:?}");
        // norms.rs still reads the dialect's file
        assert!(crate::policy::norms::load(&places).unwrap().acts.contains_key("ask-team"));
        std::fs::write(&lib, "// replaces: teams.eflint nothing.eflint\nAct ask-team\n  Actor agent.\n").unwrap();
        let e = load(&places).unwrap_err();
        assert!(e.contains("lib.eflint") && e.contains("nothing.eflint"), "{e}");
        std::fs::write(&lib, "// replaces: teams.eflint\nAct esc\n  Actor agent1\n  Recipient agent2.\n").unwrap();
        let e = load(&places).unwrap_err();
        assert!(e.contains("lib.eflint") && e.contains("`Act esc`"), "{e}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The mode is the project file's `norms`, `clingo` without one or with a word not one of the three (N-1978 p3).
    #[test]
    fn the_project_file_says_which_engine_answers() {
        let dir = std::env::temp_dir().join(format!("interp-mode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let places = Places { base: agent_base(), project: Some(dir.display().to_string()), ..Default::default() };
        assert_eq!(Mode::of(&places), Mode::Clingo, "no project file");
        let fresh = "[project]\nhost_relay = true\n";
        for (text, m) in [(fresh, Mode::Clingo), ("[project]\nnorms = \"both\"\n", Mode::Both), ("[project]\nnorms = \"eflint\"\n", Mode::Eflint), ("[project]\nnorms = \"clingo\"\n", Mode::Clingo), ("[project]\nnorms = \"neither\"\n", Mode::Clingo)] {
            std::fs::write(dir.join("project.toml"), text).unwrap();
            assert_eq!(Mode::of(&places), m, "{text}");
        }
        std::fs::write(dir.join("project.toml"), "[project]\nnorms = \"eflint\"\n").unwrap();
        let a = acts(&places).unwrap();
        let s = a.solve_for(&["esc"], &team()).unwrap();
        assert!(a.verdict(&s, "esc", &["orch", "dev"]).unwrap().unwrap().enabled);
        assert!(!a.verdict(&s, "esc", &["dev", "orch"]).unwrap().unwrap().enabled);
        assert!(a.verdict(&s, "no-such-act", &[]).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The quantifiers nested: each variable brought in by the conjunct that binds it, the rest inside.
    #[test]
    fn a_functions_quantifiers_are_nested_each_where_it_is_bound() {
        let body = ["family101", "family101.model1 == model1", "family102", "runs103", "runs103.agent1 == agent1", "family102.model1 == runs103.model2", "family101.string2 == family102.string2"];
        let vars = ["family101", "family102", "runs103"].map(String::from);
        assert_eq!(
            exists(&vars, body.map(String::from).to_vec()),
            "(Exists family101 : (family101) && (family101.model1 == model1) && (Exists runs103 : (runs103) && (runs103.agent1 == agent1) && (Exists family102 : (family102) && (family102.model1 == runs103.model2) && (family101.string2 == family102.string2))))"
        );
        assert_eq!(exists(&[], vec!["a == b".into()]), "(a == b)");
        assert_eq!(exists(&["f1".into()], vec!["Not(g(x))".into()]), "(Exists f1 : True && (Not(g(x))))".replace("True && ", ""));
    }

    /// Stating the facts with the acts underived and deriving the asked acts once answers as stating them with every
    /// act derived after each line did (the old path: `apply`, its transitions), act by act, and for one act alone.
    #[test]
    fn the_asked_acts_derived_once_answer_as_every_act_derived_after_each_fact() {
        let places = Places { base: agent_base(), ..Default::default() };
        let spec = load(&places).unwrap();
        let facts = team();
        let old: BTreeSet<(String, Vec<String>, bool)> = spec
            .state(&facts)
            .unwrap()
            .cfg
            .transitions
            .iter()
            .filter(|(te, _)| spec.acts.contains_key(&*te.dom))
            .map(|(te, en)| (te.dom.to_string(), leaf(te), *en))
            .collect();
        let new: BTreeSet<(String, Vec<String>, bool)> = spec.answers_for(&facts, None, None, false).unwrap().into_iter().map(|a| (a.act, a.objects, a.enabled)).collect();
        assert!(!old.is_empty());
        assert_eq!(old, new);
        for act in spec.acts.keys() {
            let one: BTreeSet<_> = spec.answers_for(&facts, Some(&[act.clone()]), None, false).unwrap().into_iter().map(|a| (a.act, a.objects, a.enabled)).collect();
            let want: BTreeSet<_> = old.iter().filter(|(a, _, _)| a == act).cloned().collect();
            assert_eq!(one, want, "{act}");
        }
    }

    /// A comparison that runs long says so in the journal: once while it runs, and with its time when it ends.
    #[test]
    fn a_comparison_that_runs_long_says_so_in_the_journal() {
        use std::sync::atomic::Ordering;
        LONG_SECS.store(0, Ordering::Relaxed);
        let places = Places { base: agent_base(), ..Default::default() };
        while COMPARING.load(Ordering::Acquire) {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        compare_later(places, vec!["esc".into()], team(), BTreeSet::new(), std::time::Duration::from_millis(1));
        std::thread::sleep(std::time::Duration::from_millis(100));
        for _ in 0..600 {
            if !COMPARING.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
        LONG_SECS.store(60, Ordering::Relaxed);
        let said = SAID.lock().unwrap().clone();
        assert!(said.iter().any(|l| l.starts_with("coordd: norms both: a comparison took ") && l.contains("(36 facts, 1 acts asked)")), "{said:?}");
    }

    /// `both`'s timing line: the questions per bucket, the median's bucket, the worst, how many over 50 ms.
    #[test]
    fn the_both_timing_line_counts_each_engines_questions_by_bucket() {
        let mut t = Took::default();
        for ms in [3, 4, 30, 120, 1500] {
            t.add(std::time::Duration::from_millis(ms));
        }
        assert_eq!(t.line(), "5 asked, median 10-50 ms, worst 1500 ms, 2 over 50 ms [<10: 2, 10-50: 1, 50-200: 1, 200-1000: 0, >=1000: 1]");
        assert_eq!(Took::default().line(), "0 asked, median - ms, worst 0 ms, 0 over 50 ms [<10: 0, 10-50: 0, 50-200: 0, 200-1000: 0, >=1000: 0]");
    }

    /// An instance whose `Holds when` holds and one of whose `Conditioned by` fails is answered, not enabled, with the
    /// first clause that fails (its text, file and line); one whose `Holds when` does not hold has no row.
    #[test]
    fn an_instance_a_condition_refuses_is_answered_with_that_clause() {
        let root = std::env::temp_dir().join(format!("interp-refused-{}", std::process::id()));
        let (base, project) = (root.join("base"), root.join("project"));
        std::fs::create_dir_all(base.join("policy/eflint")).unwrap();
        std::fs::create_dir_all(project.join("policy/eflint")).unwrap();
        std::fs::write(base.join("policy/t.eflint"), "Fact agent Identified by String\nFact role Identified by agent * String\n").unwrap();
        std::fs::write(base.join("policy/eflint/lib.eflint"), "Fact amount Identified by Int.\nAct give-grant\n  Actor agent1\n  Recipient agent2\n  Related to amount\n  Holds when role(agent1, \"lead\")\n  Conditioned by amount >= 1.\n").unwrap();
        std::fs::write(project.join("policy/eflint/project.eflint"), "// the project's narrowing\nExtend Act give-grant\n  Conditioned by amount <= 3.\n").unwrap();
        let places = Places { base: base.display().to_string(), project: Some(project.display().to_string()), ..Default::default() };
        let spec = load(&places).unwrap();
        let facts: Facts = vec![("agent".into(), vec!["lead".into()]), ("agent".into(), vec!["c".into()]), ("role".into(), vec!["lead".into(), "lead".into()])]
            .into_iter()
            .chain(["0", "2", "5"].map(|n| ("amount".to_string(), vec![n.to_string()])))
            .collect();
        let answers = spec.answers(&facts, None).unwrap();
        let row = |n: &str| answers.iter().find(|a| a.act == "give-grant" && a.objects == ["lead", "c", n]).cloned();
        assert!(row("2").is_some_and(|a| a.enabled && a.why.is_none()), "{answers:?}");
        let zero = row("0").expect("0 is answered");
        assert!(!zero.enabled && zero.why.as_deref().is_some_and(|w| w.contains("`amount >= 1`") && w.contains("lib.eflint:7")), "{zero:?}");
        let five = row("5").expect("5 is answered");
        assert!(!five.enabled && five.why.as_deref().is_some_and(|w| w.contains("`amount <= 3`") && w.contains("project.eflint:3")), "{five:?}");
        // c is no lead: its `Holds when` does not hold, so it has no row
        assert!(!answers.iter().any(|a| a.objects.first().is_some_and(|o| o == "c")), "{answers:?}");
        assert_eq!(spec.verdict(&facts, "give-grant", &["lead", "c", "5"]).unwrap().unwrap().why, five.why.unwrap());
        assert!(spec.answers(&facts, Some("c")).unwrap().is_empty(), "the refused rows are the asked actor's only");
        // 0 and 5, each to both recipients (lead and c)
        assert_eq!(spec.answers(&facts, Some("lead")).unwrap().iter().filter(|a| !a.enabled).count(), 4);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// eFLINT whole: a library act with an effect, a project's `Extend Act` adding a condition to it clause by clause,
    /// a fact stated under an act, and an act performed although not enabled (reported, not prevented).
    #[test]
    fn a_projects_extend_adds_to_the_librarys_act_and_its_effects_are_answered() {
        let root = std::env::temp_dir().join(format!("interp-whole-{}", std::process::id()));
        let (base, project) = (root.join("base"), root.join("project"));
        std::fs::create_dir_all(base.join("policy/eflint")).unwrap();
        std::fs::create_dir_all(project.join("policy/eflint")).unwrap();
        std::fs::write(base.join("policy/t.eflint"), "Fact agent Identified by String\nFact role Identified by agent * String\n").unwrap();
        std::fs::write(base.join("policy/eflint/lib.eflint"), "Fact amount Identified by Int.\nFact grant Identified by agent * amount.\nAct give-grant\n  Actor agent1\n  Recipient agent2\n  Related to amount\n  Holds when role(agent1, \"lead\")\n  Creates grant(agent2, amount).\n").unwrap();
        std::fs::write(project.join("policy/eflint/project.eflint"), "Extend Act give-grant\n  Conditioned by amount <= 3.\n").unwrap();
        let places = Places { base: base.display().to_string(), project: Some(project.display().to_string()), ..Default::default() };
        let spec = load(&places).unwrap();
        assert!(spec.acts.contains_key("give-grant"), "{:?}", spec.acts.keys());
        let facts: Facts = vec![("agent".into(), vec!["lead".into()]), ("agent".into(), vec!["c".into()]), ("role".into(), vec!["lead".into(), "lead".into()]), ("amount".into(), vec!["2".into()]), ("amount".into(), vec!["5".into()])];
        assert_eq!(spec.facts.get("amount"), Some(&vec!["Int".to_string()]));
        assert_eq!(spec.acts["give-grant"], vec![("actor".to_string(), "agent".to_string()), ("agent".to_string(), "agent".to_string()), ("amount".to_string(), "amount".to_string())]);
        let with = spec;
        let answers = with.answers(&facts, Some("lead")).unwrap();
        let two = answers.iter().find(|a| a.objects == ["lead", "c", "2"]).expect("give 2 holds");
        assert!(two.enabled, "{answers:?}");
        // the project's condition narrows the act: eflint-rs, as the reference, derives no instance that fails it
        assert!(!answers.iter().any(|a| a.objects == ["lead", "c", "5"] && a.enabled), "{answers:?}");
        let five = with.verdict(&facts, "give-grant", &["lead", "c", "5"]).unwrap().unwrap();
        assert!(!five.enabled && five.why.contains("`Conditioned by` fails"), "{five:?}");
        let done = with.perform(&facts, "give-grant", "lead", &["c", "2"]).unwrap();
        assert!(done.ok && !done.disabled && done.created.iter().any(|c| c.contains("grant")), "{done:?}");
        // a duty the library declares, with the act that ends it
        let mut duty = with.clone();
        duty.text.push_str("Duty thank Holder agent1 Claimant agent2 Holds when grant(agent1, amount(2)) Terminated by give-grant.\n");
        let gave: Facts = facts.iter().cloned().chain([("grant".to_string(), vec!["c".to_string(), "2".to_string()]), ("agent".to_string(), vec!["c".to_string()])]).collect();
        duty.facts.insert("grant".into(), vec!["agent".into(), "amount".into()]);
        let ds = duty.duties(&gave).unwrap();
        assert!(ds.iter().any(|d| d.duty == "thank" && d.holder.as_deref() == Some("c") && d.ended_by == ["give-grant"]), "{ds:?}");
        let over = with.perform(&facts, "give-grant", "lead", &["c", "5"]).unwrap();
        assert!(over.disabled, "performed although not enabled is reported: {over:?}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
