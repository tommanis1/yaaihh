//! Checking nets before they are used: types
//! against the events and the call templates (§7), matches cover their value's cases and no arm is dead, each net
//! ends its event as the event requires (proper completion: a turn end is accepted or nudged exactly once on every
//! path; a message is delivered or rejected at least once), the net compiled to a workflow net whose skeleton is
//! sound (option to complete, proper completion, no dead transitions), and the bound nets together raise no event
//! in a cycle without a wait. A net that fails is refused with its file and line; nothing is skipped silently.

use super::syntax::{Arg, At, Binding, Effect, EventDecl, Expr, File, Net, Op, Part, Pat, Per, Stmt, Switch};
use crate::policy::call::{Calls, Type, ty};
use crate::policy::compile::DecisionType;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The events coordd raises itself (§16.1); users declare only the events their nets raise.
pub const SYSTEM_EVENTS: &str = r#"
event owner_message { id: String, from: String, text: String, thread: String?, to: [String], open: [Question], context: [Said], quoted: [Quoted] }
event agent_message { id: String, from: String, to: [String], text: String, thread: String? }
event turn_end { agent: String, turn: Int, text: String, sent: [Said], waits: [String], open: [Question], continues: Bool, repeat: Bool }
event question { q: String, from: String, text: String }
event answer { q: String, from: String, text: String }
event check_done { check: String, agent: String, result: String }
event fault { net: String, step: String, name: String, detail: String }
"#;


/// A value's type as the checker sees it: a call type, or what has no call type.
#[derive(Clone, Debug, PartialEq)]
pub enum T {
    Ty(Type),
    /// A decision's outcome: one of its options.
    Decision(String),
    /// `none`: any optional, or the wake `Nothing`.
    None,
    /// A bare variant name (`Question`), compared with an enum value.
    Variant(String),
}

impl T {
    fn show(&self) -> String {
        match self {
            T::Ty(t) => show(t),
            T::Decision(d) => format!("decision {d}"),
            T::None => "none".into(),
            T::Variant(v) => format!("variant {v}"),
        }
    }
}

pub fn show(t: &Type) -> String {
    match t {
        Type::Str => "String".into(),
        Type::Int => "Int".into(),
        Type::Bool => "Bool".into(),
        Type::Opt(t) => format!("{}?", show(t)),
        Type::List(t) => format!("[{}]", show(t)),
        Type::Named(n) => n.clone(),
    }
}

/// What a net may read: the events, the call types and calls, the policy's decisions.
pub struct Env<'a> {
    pub calls: &'a Calls,
    pub events: &'a BTreeMap<String, EventDecl>,
    pub event_fields: BTreeMap<String, Vec<(String, Type)>>,
    pub decisions: &'a BTreeMap<String, DecisionType>,
}

impl<'a> Env<'a> {
    pub fn new(calls: &'a Calls, events: &'a BTreeMap<String, EventDecl>, decisions: &'a BTreeMap<String, DecisionType>) -> Result<Env<'a>, String> {
        let mut event_fields = BTreeMap::new();
        for (n, e) in events {
            let fields = e.fields.iter().map(|(f, t)| Ok((f.clone(), ty(t).map_err(|x| format!("{}: event {n}: {f}: {x}", e.at))?))).collect::<Result<Vec<_>, String>>()?;
            event_fields.insert(n.clone(), fields);
        }
        Ok(Env { calls, events, event_fields, decisions })
    }

    fn record(&self, n: &str) -> Option<&Vec<(String, Type)>> {
        self.event_fields.get(n).or_else(|| self.calls.types.get(n))
    }
    fn variants(&self, n: &str) -> Option<&Vec<(String, Vec<(String, Type)>)>> {
        self.calls.enums.get(n)
    }
}

/// A finding against a net: where and what.
fn at_err(at: &At, msg: impl std::fmt::Display) -> String {
    format!("{at}: {msg}")
}

/// Whether a value of type HAVE may stand where WANT is wanted.
fn fits(have: &T, want: &Type, env: &Env) -> bool {
    match (have, want) {
        (T::None, Type::Opt(_)) => true,
        (T::None, Type::Named(n)) => n == "Wake",
        (T::Ty(h), w) if h == w => true,
        (T::Ty(h), Type::Opt(w)) => fits(&T::Ty(h.clone()), w, env),
        (T::Ty(Type::List(h)), Type::List(w)) => fits(&T::Ty((**h).clone()), w, env),
        (T::Variant(v), Type::Named(n)) => env.variants(n).is_some_and(|vs| vs.iter().any(|(x, _)| x == v) && vs.iter().all(|(_, f)| f.is_empty())),
        _ => false,
    }
}

/// What a value can be shown as in a string hole or a decision's field: a scalar, an enum (its variant), or an
/// optional of one.
fn scalar(t: &T, env: &Env) -> bool {
    match t {
        T::Ty(Type::Str | Type::Int | Type::Bool) | T::None | T::Variant(_) => true,
        T::Ty(Type::Opt(t)) => scalar(&T::Ty((**t).clone()), env),
        T::Ty(Type::Named(n)) => env.variants(n).is_some(),
        T::Decision(_) => true,
        _ => false,
    }
}

/// The checker's state for one net.
struct Ck<'a, 'b> {
    env: &'b Env<'a>,
    net: &'b Net,
    /// Effects found, for the cross-net check: the events it raises.
    raises: BTreeSet<String>,
}

type Scope = HashMap<String, T>;

impl<'a, 'b> Ck<'a, 'b> {
    fn path(&self, scope: &Scope, p: &[String], at: &At) -> Result<T, String> {
        let mut t = scope.get(&p[0]).cloned().ok_or(at_err(at, format!("`{}` is not defined here", p[0])))?;
        for f in &p[1..] {
            let T::Ty(Type::Named(n)) = &t else {
                return Err(at_err(at, format!("`{}`: `{f}` of a {}, which has no fields (an optional is opened by `match`)", p.join("."), t.show())));
            };
            if self.env.variants(n).is_some() {
                if f == "kind" {
                    t = T::Ty(Type::Str);
                    continue;
                }
                return Err(at_err(at, format!("`{}`: {n} is an enum: match on it to read its fields", p.join("."))));
            }
            let rec = self.env.record(n).ok_or(at_err(at, format!("no type `{n}`")))?;
            t = T::Ty(rec.iter().find(|(x, _)| x == f).map(|(_, t)| t.clone()).ok_or(at_err(at, format!("`{}`: {n} has no field `{f}`", p.join("."))))?);
        }
        Ok(t)
    }

    /// Match ARGS to the parameters PARAMS (positional first, then by name; every parameter given once).
    fn bind<'x>(&self, what: &str, params: &[(String, Type)], args: &'x [Arg], at: &At) -> Result<Vec<(&'x Expr, Type)>, String> {
        let mut given: Vec<Option<&Expr>> = vec![None; params.len()];
        let mut pos = 0;
        for a in args {
            let k = match &a.name {
                None => {
                    let k = pos;
                    pos += 1;
                    k
                }
                Some(n) => params.iter().position(|(p, _)| p == n).ok_or(at_err(at, format!("{what} has no parameter `{n}` ({})", names(params))))?,
            };
            if k >= params.len() {
                return Err(at_err(at, format!("{what} takes {} values ({})", params.len(), names(params))));
            }
            if given[k].is_some() {
                return Err(at_err(at, format!("{what}: `{}` given twice", params[k].0)));
            }
            given[k] = Some(&a.value);
        }
        params
            .iter()
            .zip(given)
            .map(|((p, t), e)| match e {
                Some(e) => Ok((e, t.clone())),
                None if matches!(t, Type::Opt(_)) => Ok((&NONE, t.clone())),
                None => Err(at_err(at, format!("{what}: `{p}` is not given"))),
            })
            .collect()
    }

    fn args_fit(&mut self, what: &str, scope: &Scope, params: &[(String, Type)], args: &[Arg], at: &At) -> Result<(), String> {
        for (e, want) in self.bind(what, params, args, at)? {
            let have = self.expr(scope, e, at)?;
            if !fits(&have, &want, self.env) {
                return Err(at_err(at, format!("{what}: a {} where a {} is wanted", have.show(), show(&want))));
            }
        }
        Ok(())
    }

    fn expr(&mut self, scope: &Scope, e: &Expr, at: &At) -> Result<T, String> {
        Ok(match e {
            Expr::Path(p) => self.path(scope, p, at)?,
            Expr::Str(parts) => {
                for p in parts {
                    if let Part::Hole(h) = p {
                        let t = self.expr(scope, h, at)?;
                        if !scalar(&t, self.env) {
                            return Err(at_err(at, format!("a {{{{ }}}} hole shows a String, Int, Bool or enum, not a {}", t.show())));
                        }
                    }
                }
                T::Ty(Type::Str)
            }
            Expr::Int(_) => T::Ty(Type::Int),
            Expr::Bool(_) => T::Ty(Type::Bool),
            Expr::None => T::None,
            Expr::Owner => T::Ty(Type::Str),
            Expr::Variant(v) => T::Variant(v.clone()),
            Expr::Call { name, args, place, with, at } => {
                let c = self.env.calls.calls.get(name).ok_or(at_err(at, format!("no call `{name}` (a .call file's `call {name}(...)`)")))?;
                if let super::syntax::Place::In(s) = place {
                    if !self.net.sessions.iter().any(|d| &d.name == s) {
                        return Err(at_err(at, format!("no session `{s}` in net {} (`session {s}: per agent`)", self.net.name)));
                    }
                    if let Some((f, _)) = with.iter().find(|(f, _)| f != "model") {
                        return Err(at_err(at, format!("a call in session {s} runs on the session's model: `with {f}` is for fresh calls")));
                    }
                }
                for (f, v) in with {
                    if !matches!(v, Expr::Str(_)) {
                        return Err(at_err(at, format!("`with {f}:` takes a string")));
                    }
                }
                let params = c.params.clone();
                self.args_fit(&format!("call {name}"), scope, &params, args, at)?;
                T::Ty(c.returns.clone())
            }
            Expr::Decide { name, args, with, at } => {
                if !self.env.decisions.contains_key(name) {
                    return Err(at_err(at, format!("no decision `{name}` in the policy (`Decision {name}` in a .eflint file)")));
                }
                for a in args {
                    if a.name.is_none() {
                        return Err(at_err(at, format!("decide {name}: name each field (`decide {name}(agent: t.agent)`): the policy reads them as call.FIELD")));
                    }
                    let t = self.expr(scope, &a.value, at)?;
                    if !scalar(&t, self.env) {
                        return Err(at_err(at, format!("decide {name}: `{}` is a {}; a decision reads strings, numbers, booleans and enum variants", a.name.as_deref().unwrap_or(""), t.show())));
                    }
                }
                for (f, v) in with {
                    if !matches!(v, Expr::Str(_)) {
                        return Err(at_err(at, format!("`with {f}:` takes a string")));
                    }
                }
                T::Decision(name.clone())
            }
            Expr::Builtin { name, args } => {
                let (params, ret): (Vec<(String, Type)>, Type) = match name.as_str() {
                    "questions" => (vec![("agent".into(), Type::Str)], Type::List(Box::new(Type::Named("Question".into())))),
                    "all_questions" => (vec![], Type::List(Box::new(Type::Named("Question".into())))),
                    "nudged_in_row" => (vec![("agent".into(), Type::Str)], Type::Int),
                    "joined" => (vec![("list".into(), Type::List(Box::new(Type::Str)))], Type::Str),
                    "answer" => (vec![("q".into(), Type::Str)], Type::Named("Wake".into())),
                    "check" => (vec![("check".into(), Type::Str)], Type::Named("Wake".into())),
                    "mail" => (vec![("from".into(), Type::Str)], Type::Named("Wake".into())),
                    "len" => {
                        let [a] = args.as_slice() else { return Err(at_err(at, "len takes one value")) };
                        match self.expr(scope, &a.value, at)? {
                            T::Ty(Type::List(_) | Type::Str) => return Ok(T::Ty(Type::Int)),
                            t => return Err(at_err(at, format!("len of a {}: it takes a list or a string", t.show()))),
                        }
                    }
                    _ => {
                        let hint = if self.env.calls.calls.contains_key(name) { format!(" (`{name}` is a call: say where it runs, `{name}(...) fresh` or `in SESSION`)") } else { String::new() };
                        return Err(at_err(at, format!("no built-in `{name}` (questions, all_questions, nudged_in_row, joined, len, answer, check, mail){hint}")));
                    }
                };
                self.args_fit(name, scope, &params, args, at)?;
                T::Ty(ret)
            }
            Expr::Cmp(a, op, b) => {
                let (ta, tb) = (self.expr(scope, a, at)?, self.expr(scope, b, at)?);
                let ok = match op {
                    Op::Eq | Op::Ne => comparable(&ta, &tb, self.env) || comparable(&tb, &ta, self.env),
                    _ => ta == T::Ty(Type::Int) && tb == T::Ty(Type::Int),
                };
                if !ok {
                    return Err(at_err(at, format!("cannot compare a {} with a {}", ta.show(), tb.show())));
                }
                T::Ty(Type::Bool)
            }
            Expr::And(a, b) | Expr::Or(a, b) => {
                self.cond(scope, a, at)?;
                self.cond(scope, b, at)?;
                T::Ty(Type::Bool)
            }
            Expr::Not(a) => {
                self.cond(scope, a, at)?;
                T::Ty(Type::Bool)
            }
        })
    }

    /// A condition: a Bool, or a decision whose options are `yes | no` (true when it decides yes).
    fn cond(&mut self, scope: &Scope, e: &Expr, at: &At) -> Result<(), String> {
        match self.expr(scope, e, at)? {
            T::Ty(Type::Bool) => Ok(()),
            T::Decision(d) => {
                let listed = &self.env.decisions[&d].listed;
                if listed.len() == 2 && listed.contains(&"yes".to_string()) && listed.contains(&"no".to_string()) {
                    Ok(())
                } else {
                    Err(at_err(at, format!("`if decide {d}` needs its options to be `yes | no`; match on it instead")))
                }
            }
            t => Err(at_err(at, format!("a condition is a Bool, not a {}", t.show()))),
        }
    }

    fn block(&mut self, scope: &Scope, stmts: &[Stmt]) -> Result<(), String> {
        let mut scope = scope.clone();
        for s in stmts {
            self.stmt(&mut scope, s)?;
        }
        Ok(())
    }

    fn stmt(&mut self, scope: &mut Scope, s: &Stmt) -> Result<(), String> {
        match s {
            Stmt::Let { var, value, at } => {
                let t = self.expr(scope, value, at)?;
                scope.insert(var.clone(), t);
            }
            Stmt::Set { path, value, add, at } => {
                let target = self.path(scope, path, at)?;
                let have = self.expr(scope, value, at)?;
                let ok = match (&target, add) {
                    (T::Ty(t), false) => fits(&have, t, self.env),
                    (T::Ty(Type::List(t)), true) => fits(&have, t, self.env) || fits(&have, &Type::List(t.clone()), self.env),
                    _ => false,
                };
                if !ok {
                    let verb = if *add { "+=" } else { ":=" };
                    return Err(at_err(at, format!("`{} {verb}` a {}: it is a {}", path.join("."), have.show(), target.show())));
                }
            }
            Stmt::If { cond, then, els, at } => {
                self.cond(scope, cond, at)?;
                self.block(scope, then)?;
                self.block(scope, els)?;
            }
            Stmt::For { var, list, filter, body, at, .. } => {
                let T::Ty(Type::List(item)) = self.expr(scope, list, at)? else { return Err(at_err(at, "`for x in` takes a list")) };
                let mut inner = scope.clone();
                inner.insert(var.clone(), T::Ty(*item));
                if let Some(f) = filter {
                    self.cond(&inner, f, at)?;
                }
                self.block(&inner, body)?;
            }
            Stmt::Match { scrut, arms, at } => {
                let t = self.expr(scope, scrut, at)?;
                self.arms(scope, &t, arms, at)?;
            }
            Stmt::Effect { effect, args, at } => self.effect(scope, effect, args, at)?,
        }
        Ok(())
    }

    fn arms(&mut self, scope: &Scope, t: &T, arms: &[super::syntax::Arm], at: &At) -> Result<(), String> {
        // the cases the value has, and which an unguarded arm has covered so far
        let cases: Vec<String> = match t {
            T::Ty(Type::Named(n)) if self.env.variants(n).is_some() => self.env.variants(n).unwrap().iter().map(|(v, _)| v.clone()).collect(),
            T::Ty(Type::Opt(_)) => vec!["some".into(), "none".into()],
            T::Decision(d) => {
                let listed = &self.env.decisions[d].listed;
                if listed.is_empty() {
                    return Err(at_err(at, format!("decision {d} chooses among a fact's instances (`Options {}`); a net matches on decisions whose options are listed", self.env.decisions[d].options)));
                }
                listed.clone()
            }
            other => return Err(at_err(at, format!("match on a {}: it takes an enum, an optional or a decision", other.show()))),
        };
        let mut covered: BTreeSet<String> = BTreeSet::new();
        for arm in arms {
            let mut inner = scope.clone();
            let case: Option<String> = match (&arm.pat, t) {
                (Pat::Wild, _) => None,
                (Pat::Variant(v, binds), T::Ty(Type::Named(n))) => {
                    let (_, fields) = self.env.variants(n).unwrap().iter().find(|(x, _)| x == v).ok_or(at_err(&arm.at, format!("{n} has no variant `{v}` ({})", cases.join(", "))))?;
                    if !binds.is_empty() && binds.len() != fields.len() {
                        return Err(at_err(&arm.at, format!("{v} has {} fields ({}): bind all or none", fields.len(), names(fields))));
                    }
                    for (b, (_, ft)) in binds.iter().zip(fields) {
                        inner.insert(b.clone(), T::Ty(ft.clone()));
                    }
                    Some(v.clone())
                }
                (Pat::Some(x), T::Ty(Type::Opt(inner_t))) => {
                    inner.insert(x.clone(), T::Ty((**inner_t).clone()));
                    Some("some".into())
                }
                (Pat::None, T::Ty(Type::Opt(_))) => Some("none".into()),
                (Pat::Option(o), T::Decision(d)) => {
                    if !cases.contains(o) {
                        return Err(at_err(&arm.at, format!("decision {d} has no option `{o}` ({})", cases.join(", "))));
                    }
                    Some(o.clone())
                }
                (p, t) => return Err(at_err(&arm.at, format!("the pattern {p:?} does not fit a {}", t.show()))),
            };
            // an arm whose cases an earlier unguarded arm took is dead
            let dead = match &case {
                Some(c) => covered.contains(c),
                None => cases.iter().all(|c| covered.contains(c)),
            };
            if dead {
                return Err(at_err(&arm.at, "a dead arm: the arms before it take every value it could match"));
            }
            if let Some(g) = &arm.guard {
                self.cond(&inner, g, &arm.at)?;
            } else {
                match case {
                    Some(c) => {
                        covered.insert(c);
                    }
                    None => covered.extend(cases.iter().cloned()),
                }
            }
            self.block(&inner, &arm.body)?;
        }
        let missing: Vec<&String> = cases.iter().filter(|c| !covered.contains(*c)).collect();
        if !missing.is_empty() {
            return Err(at_err(at, format!("the match does not cover {} (an arm without `if`, or `_`)", missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "))));
        }
        Ok(())
    }

    fn effect(&mut self, scope: &Scope, effect: &Effect, args: &[Arg], at: &At) -> Result<(), String> {
        let s = |n: &str, t: Type| (n.to_string(), t);
        let opt = |t: Type| Type::Opt(Box::new(t));
        let params: Vec<(String, Type)> = match effect {
            Effect::Deliver => {
                // `to` is one name or several
                let (to, rest) = args.split_first().ok_or(at_err(at, "deliver(to, text, thread)"))?;
                let t = self.expr(scope, &to.value, at)?;
                if !(fits(&t, &opt(Type::Str), self.env) || fits(&t, &Type::List(Box::new(Type::Str)), self.env)) {
                    return Err(at_err(at, format!("deliver: `to` is a name or a list of names, not a {}", t.show())));
                }
                return self.args_fit("deliver", scope, &[s("text", Type::Str), s("thread", opt(Type::Str))], rest, at);
            }
            Effect::Reject => vec![s("to", Type::Str), s("why", Type::Str)],
            Effect::Nudge => vec![s("agent", Type::Str), s("text", Type::Str)],
            Effect::Ask => vec![s("from", Type::Str), s("text", Type::Str), s("on", opt(Type::Str))],
            Effect::Post => vec![s("from", Type::Str), s("text", Type::Str), s("thread", Type::Str)],
            Effect::Tell => vec![s("to", Type::Str), s("text", Type::Str)],
            Effect::Accept => vec![s("wake", Type::Named("Wake".into()))],
            Effect::Fault(_) => {
                self.raises.insert("fault".into());
                for a in args {
                    let t = self.expr(scope, &a.value, at)?;
                    let _ = t;
                }
                return Ok(());
            }
            Effect::Raise(e) => {
                let decl = self.env.events.get(e).ok_or(at_err(at, format!("no event `{e}` (`event {e} {{ ... }}` in a .net file)")))?;
                if decl.at.file == "system" {
                    return Err(at_err(at, format!("`{e}` is coordd's own event: a net raises only events declared in .net files (faults: `fault NAME(...)`)")));
                }
                self.raises.insert(e.clone());
                self.env.event_fields[e].clone()
            }
        };
        if matches!(effect, Effect::Accept | Effect::Nudge) && self.net.event != "turn_end" {
            return Err(at_err(at, format!("`{}` ends an agent's turn: only a net on turn_end has one", effect.name())));
        }
        self.args_fit(&effect.name(), scope, &params, args, at)
    }
}

static NONE: Expr = Expr::None;

fn names(fields: &[(String, Type)]) -> String {
    fields.iter().map(|(f, t)| format!("{f}: {}", show(t))).collect::<Vec<_>>().join(", ")
}

/// Whether A and B may be compared with `==`.
fn comparable(a: &T, b: &T, env: &Env) -> bool {
    match (a, b) {
        (T::Ty(x), T::Ty(y)) if x == y => true,
        (T::Ty(Type::Opt(x)), T::Ty(y)) => **x == *y,
        (T::Ty(Type::Opt(_)), T::None) => true,
        (T::Ty(Type::Named(n)), T::Variant(v)) => env.variants(n).is_some_and(|vs| vs.iter().any(|(x, _)| x == v)),
        (T::Decision(_), T::Ty(Type::Str)) => true,
        _ => false,
    }
}

// ---- proper completion ---------------------------------------------------------------------------------------------

/// How many ending effects a path has (0, 1, 2 = more), and whether it ended in a fault.
type Ends = BTreeSet<(u8, bool)>;

/// The ending effects EVENT requires on every path that does not fault: `exactly` one, or `at least` one, of these.
fn obligation(event: &str) -> Option<(bool, &'static [&'static str])> {
    match event {
        "turn_end" => Some((true, &["accept", "nudge"])),
        "owner_message" | "agent_message" => Some((false, &["deliver", "reject"])),
        _ => None,
    }
}

fn then(a: &Ends, b: &Ends) -> Ends {
    let mut out = Ends::new();
    for &(n, f) in a {
        if f {
            out.insert((n, f));
            continue;
        }
        for &(m, g) in b {
            out.insert(((n + m).min(2), g));
        }
    }
    out
}

/// The ends of STMTS counting the effects COUNTED; NONEMPTY: lists known to have an element here (in the `else` of
/// `if len(xs) == 0`, and the `then` of `len(xs) > 0`).
fn ends(stmts: &[Stmt], counted: &[&str], nonempty: &BTreeSet<Vec<String>>) -> Ends {
    let mut acc: Ends = [(0, false)].into();
    for s in stmts {
        let e = match s {
            Stmt::Effect { effect: Effect::Fault(_), .. } => [(0, true)].into(),
            Stmt::Effect { effect, .. } => [(u8::from(counted.contains(&effect.name().as_str())), false)].into(),
            Stmt::Let { .. } | Stmt::Set { .. } => [(0, false)].into(),
            Stmt::If { cond, then: t, els, .. } => {
                let (mut ne_then, mut ne_else) = (nonempty.clone(), nonempty.clone());
                if let Expr::Cmp(a, op, b) = cond {
                    if let (Expr::Builtin { name, args }, Expr::Int(0)) = (&**a, &**b) {
                        if let (true, [Arg { value: Expr::Path(p), .. }]) = (name == "len", args.as_slice()) {
                            match op {
                                Op::Eq => ne_else.insert(p.clone()),
                                Op::Gt | Op::Ne => ne_then.insert(p.clone()),
                                _ => false,
                            };
                        }
                    }
                }
                let mut u = ends(t, counted, &ne_then);
                u.extend(ends(els, counted, &ne_else));
                u
            }
            Stmt::Match { arms, .. } => arms.iter().flat_map(|a| ends(&a.body, counted, nonempty)).collect(),
            Stmt::For { list, filter, body, .. } => {
                let b = ends(body, counted, nonempty);
                // at least once only over a list known not to be empty and with no `where`
                let once = filter.is_none() && matches!(list, Expr::Path(p) if nonempty.contains(p));
                let mut u: Ends = if once { b.clone() } else { [(0, false)].into_iter().chain(b.iter().copied()).collect() };
                loop {
                    let more: Ends = u.union(&then(&u, &b)).copied().collect();
                    if more == u {
                        break;
                    }
                    u = more;
                }
                u
            }
        };
        acc = then(&acc, &e);
    }
    acc
}

// ---- the workflow net ----------------------------------------------------------------------------------------------

/// A net's skeleton as a workflow net: places `0..places`, `i` = 0 and `o` = 1; each transition a step.
#[derive(Debug, Default)]
pub struct Workflow {
    pub places: usize,
    pub transitions: Vec<(usize, usize, String, At)>,
}

impl Workflow {
    fn place(&mut self) -> usize {
        self.places += 1;
        self.places - 1
    }
    fn tr(&mut self, from: usize, to: usize, label: String, at: &At) {
        self.transitions.push((from, to, label, at.clone()));
    }

    /// STMTS from place P to place Q.
    fn block(&mut self, stmts: &[Stmt], p: usize, q: usize) {
        let mut cur = p;
        for (k, s) in stmts.iter().enumerate() {
            let next = if k + 1 == stmts.len() { q } else { self.place() };
            self.stmt(s, cur, next);
            cur = next;
        }
        if stmts.is_empty() {
            self.tr(p, q, "skip".into(), &At::default());
        }
    }

    fn stmt(&mut self, s: &Stmt, p: usize, q: usize) {
        let at = s.at();
        match s {
            Stmt::Effect { effect: Effect::Fault(n), .. } => self.tr(p, 1, format!("fault {n}"), at), // a fault ends the run
            Stmt::Effect { effect, .. } => self.tr(p, q, effect.name(), at),
            Stmt::Let { var, .. } => self.tr(p, q, format!("let {var}"), at),
            Stmt::Set { path, .. } => self.tr(p, q, format!("set {}", path.join(".")), at),
            Stmt::If { then, els, .. } => {
                let (a, b) = (self.place(), self.place());
                self.tr(p, a, "if: then".into(), at);
                self.tr(p, b, "if: else".into(), at);
                self.block(then, a, q);
                self.block(els, b, q);
            }
            Stmt::Match { arms, .. } => {
                for arm in arms {
                    let a = self.place();
                    self.tr(p, a, format!("match: {:?}", arm.pat), &arm.at);
                    self.block(&arm.body, a, q);
                }
            }
            Stmt::For { var, body, par, .. } => {
                // `par for`'s iterations are one by one in the skeleton too (each takes the loop's one token)
                let kw = if *par { "par for" } else { "for" };
                let (l, b) = (self.place(), self.place());
                self.tr(p, l, format!("{kw} {var}: enter"), at);
                self.tr(l, b, format!("for {var}: next"), at);
                self.block(body, b, l);
                self.tr(l, q, format!("for {var}: done"), at);
            }
        }
    }

    /// Soundness of the skeleton (van der Aalst): from `i`, every reachable marking can reach `o`, `o` is reached with
    /// no token left elsewhere, and every transition can fire. The skeleton's transitions take one token and give
    /// one, so a marking is the place of the one token.
    fn sound(&self) -> Result<(), String> {
        let mut reach = vec![false; self.places];
        let mut fired = vec![false; self.transitions.len()];
        let mut todo = vec![0usize];
        reach[0] = true;
        while let Some(p) = todo.pop() {
            for (k, (from, to, _, _)) in self.transitions.iter().enumerate() {
                if *from == p {
                    fired[k] = true;
                    if !reach[*to] {
                        reach[*to] = true;
                        todo.push(*to);
                    }
                }
            }
        }
        if let Some(k) = fired.iter().position(|f| !f) {
            let (_, _, label, at) = &self.transitions[k];
            return Err(format!("{at}: a dead step ({label}): no run reaches it (is it after a `fault`?)"));
        }
        // option to complete: o reachable backwards from every reachable place
        let mut back = vec![false; self.places];
        back[1] = true;
        let mut changed = true;
        while changed {
            changed = false;
            for (from, to, _, _) in &self.transitions {
                if back[*to] && !back[*from] {
                    back[*from] = true;
                    changed = true;
                }
            }
        }
        if let Some(p) = (0..self.places).find(|&p| reach[p] && !back[p]) {
            return Err(format!("place {p} is reached but the run cannot end from it"));
        }
        if self.transitions.iter().any(|(from, _, _, _)| *from == 1) {
            return Err("a step after the end".into());
        }
        Ok(())
    }
}

/// Compile NET to its workflow net.
pub fn workflow(net: &Net) -> Workflow {
    let mut w = Workflow { places: 2, transitions: Vec::new() };
    w.block(&net.body, 0, 1);
    w
}

// ---- one net, and the bound set ------------------------------------------------------------------------------------

/// Check NET: the events it raises when it passes.
pub fn net(env: &Env, net: &Net) -> Result<BTreeSet<String>, String> {
    let decl = env.events.get(&net.event).ok_or(at_err(&net.at, format!("net {}: no event `{}`", net.name, net.event)))?;
    let mut seen = BTreeSet::new();
    for s in &net.sessions {
        if !seen.insert(&s.name) {
            return Err(at_err(&s.at, format!("session `{}` declared twice", s.name)));
        }
        if s.per == Per::Agent && !matches!(net.event.as_str(), "turn_end" | "agent_message" | "question" | "check_done" | "owner_message") {
            return Err(at_err(&s.at, format!("`per agent`: event {} names no agent", net.event)));
        }
    }
    let mut ck = Ck { env, net, raises: BTreeSet::new() };
    let mut scope = Scope::new();
    scope.insert(net.var.clone(), T::Ty(Type::Named(decl.name.clone())));
    ck.block(&scope, &net.body)?;
    if let Some((exactly, effects)) = obligation(&net.event) {
        let e = ends(&net.body, effects, &BTreeSet::new());
        let bad = e.iter().find(|(n, f)| !f && if exactly { *n != 1 } else { *n == 0 });
        if let Some((n, _)) = bad {
            let what = effects.join(" or ");
            let how = match (exactly, n) {
                (_, 0) => "not at all".to_string(),
                (true, _) => "more than once".to_string(),
                _ => unreachable!(),
            };
            let need = if exactly { "exactly once" } else { "at least once" };
            return Err(at_err(&net.at, format!("net {}: a path ends its {} {how}; every path must {what} {need} (or fault)", net.name, net.event)));
        }
    }
    workflow(net).sound().map_err(|e| format!("net {}: {e}", net.name))?;
    Ok(ck.raises)
}

/// Where a binding applies, narrowest last.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope2 {
    Global,
    Project,
    Agent,
}

/// The loaded nets: those that passed, the bindings that stand, and every refusal.
#[derive(Debug, Default)]
pub struct Loaded {
    pub nets: BTreeMap<String, Net>,
    pub bindings: Vec<(Scope2, Binding)>,
    pub events: BTreeMap<String, EventDecl>,
    pub errors: Vec<String>,
    /// The live set (`nets/live`).
    pub live: LiveSet,
}

/// The live set: which effects are taken for real, its lines that stand by scope.
#[derive(Clone, Debug, Default)]
pub struct LiveSet(Vec<(Scope2, Switch)>);

impl LiveSet {
    /// The lines SWITCHES kept, and the refusals: a `live` line for an effect coordd does not take (not in TAKEN), and
    /// a line that says the opposite of an earlier one at the same scope for the same agents.
    pub fn new(switches: Vec<(Scope2, Switch)>, taken: &[&str]) -> (LiveSet, Vec<String>) {
        let (mut kept, mut errors): (Vec<(Scope2, Switch)>, Vec<String>) = (Vec::new(), Vec::new());
        for (scope, s) in switches {
            let scope = if s.agents.is_empty() { scope } else { Scope2::Agent };
            if s.live && !taken.contains(&s.effect.as_str()) {
                errors.push(at_err(&s.at, format!("refused: coordd does not take `{}` yet (only {}); it stays dry", s.effect, taken.join(", "))));
                continue;
            }
            if let Some((_, x)) = kept.iter().find(|(sc, x)| *sc == scope && x.effect == s.effect && x.live != s.live && x.agents == s.agents) {
                errors.push(at_err(&s.at, format!("refused: says the opposite of {} for `{}`", x.at, s.effect)));
                continue;
            }
            kept.push((scope, s));
        }
        (LiveSet(kept), errors)
    }

    /// Whether EFFECT of a run for AGENT is taken for real: the narrowest line that names it, or dry.
    pub fn is_live(&self, effect: &str, agent: Option<&str>) -> bool {
        self.0
            .iter()
            .filter(|(_, s)| s.effect == effect && (s.agents.is_empty() || agent.is_some_and(|a| s.agents.iter().any(|x| x == a))))
            .max_by_key(|(sc, _)| *sc)
            .is_some_and(|(_, s)| s.live)
    }
}

impl Loaded {
    /// Whether EFFECT of a run for AGENT is taken for real (the live set's say).
    pub fn is_live(&self, effect: &str, agent: Option<&str>) -> bool {
        self.live.is_live(effect, agent)
    }

    /// The net for EVENT of AGENT (none when the event names no agent): the narrowest binding.
    pub fn resolve(&self, event: &str, agent: Option<&str>) -> Option<&Net> {
        self.bindings
            .iter()
            .filter(|(_, b)| b.event == event && (b.agents.is_empty() || agent.is_some_and(|a| b.agents.iter().any(|x| x == a))))
            .max_by_key(|(s, _)| *s)
            .and_then(|(_, b)| self.nets.get(&b.net))
    }
}

/// Check FILES (system first, then project: a project net replaces a system net of the same name) and BINDINGS
/// (global, then project), against CALLS and DECISIONS.
pub fn load(calls: &Calls, decisions: &BTreeMap<String, DecisionType>, files: Vec<File>, bindings: Vec<(Scope2, Binding)>) -> Loaded {
    let mut out = Loaded::default();
    let system = super::syntax::parse("system", SYSTEM_EVENTS).expect("the system events parse");
    for mut e in system.events {
        e.at.file = "system".into();
        out.events.insert(e.name.clone(), e);
    }
    let mut nets: BTreeMap<String, Net> = BTreeMap::new();
    for f in files {
        for e in f.events {
            if let Some(old) = out.events.get(&e.name) {
                out.errors.push(at_err(&e.at, format!("event `{}` is declared already ({})", e.name, old.at)));
                continue;
            }
            out.events.insert(e.name.clone(), e);
        }
        for n in f.nets {
            nets.insert(n.name.clone(), n);
        }
    }
    for t in ["Question", "Said", "Quoted", "Wake"] {
        if !calls.types.contains_key(t) && !calls.enums.contains_key(t) {
            out.errors.push(format!("the system type {t} is missing from the calls"));
        }
    }
    let env = match Env::new(calls, &out.events, decisions) {
        Ok(e) => e,
        Err(e) => {
            out.errors.push(e);
            return out;
        }
    };
    let mut raises: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (name, n) in nets {
        match net(&env, &n) {
            Ok(r) => {
                raises.insert(name.clone(), r);
                out.nets.insert(name, n);
            }
            Err(e) => out.errors.push(format!("refused: {e}")),
        }
    }
    // bindings: to a known event and a net that passed and is on that event; one per event, scope and agent
    let mut stood: Vec<(Scope2, Binding)> = Vec::new();
    for (scope, b) in bindings {
        let scope = if b.agents.is_empty() { scope } else { Scope2::Agent };
        if !out.events.contains_key(&b.event) {
            out.errors.push(at_err(&b.at, format!("no event `{}`", b.event)));
            continue;
        }
        match out.nets.get(&b.net) {
            None => {
                out.errors.push(at_err(&b.at, format!("no net `{}` that passed its check: the binding is refused", b.net)));
                continue;
            }
            Some(n) if n.event != b.event => {
                out.errors.push(at_err(&b.at, format!("net {} is on {}, not {}", b.net, n.event, b.event)));
                continue;
            }
            _ => {}
        }
        let clash = stood.iter().find(|(s, x)| {
            *s == scope && x.event == b.event && (x.agents.is_empty() && b.agents.is_empty() || x.agents.iter().any(|a| b.agents.contains(a)))
        });
        if let Some((_, x)) = clash {
            out.errors.push(at_err(&b.at, format!("{} is bound already at this scope ({}): the narrower binding wins, not a second one", b.event, x.at)));
            continue;
        }
        stood.push((scope, b));
    }
    // raised events in a cycle (an event's net raises, through other nets, that event again) run without a wait: refused
    let edges: Vec<(String, String, String)> = stood
        .iter()
        .flat_map(|(_, b)| raises.get(&b.net).into_iter().flatten().map(|r| (b.event.clone(), r.clone(), b.net.clone())))
        .collect();
    let mut cyclic: BTreeSet<String> = BTreeSet::new();
    for (start, _, _) in &edges {
        // depth-first from START: a path back to START is a cycle
        let mut stack = vec![(start.clone(), vec![start.clone()])];
        let mut seen = BTreeSet::new();
        while let Some((e, path)) = stack.pop() {
            for (from, to, net) in &edges {
                if from != &e {
                    continue;
                }
                if to == start {
                    if cyclic.insert(start.clone()) {
                        let chain = path.iter().chain(std::iter::once(to)).cloned().collect::<Vec<_>>().join(" → ");
                        out.errors.push(format!("refused: the nets raise {chain} (last by {net}), a cycle without a wait; its bindings are dropped"));
                    }
                } else if seen.insert(to.clone()) {
                    let mut p = path.clone();
                    p.push(to.clone());
                    stack.push((to.clone(), p));
                }
            }
        }
    }
    out.bindings = stood.into_iter().filter(|(_, b)| !cyclic.contains(&b.event)).collect();
    out
}

#[cfg(test)]
mod tests {
    use super::super::syntax::{bindings, parse};
    use super::*;

    fn calls() -> Calls {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("coordd-net-check-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).unwrap();
        let types = "type Piece {\n  kind: PieceKind\n  text: String\n}\ntype PieceKind = Question | Report\ntype Clarity = Clear | Unclear(why: String)\n";
        std::fs::write(dir.join("c.call"), format!("{types}call split(text: String) -> [Piece]\n  task clerk\n  template\n    {{{{ text }}}}\ncall clarity(text: String) -> Clarity\n  task clerk\n  template\n    {{{{ text }}}}\n")).unwrap();
        let loaded = Calls::load(&[dir.display().to_string()], &Default::default()).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert!(loaded.types.contains_key("Question") && loaded.enums.contains_key("Wake"), "the system types come with every load");
        loaded
    }

    fn decisions() -> BTreeMap<String, DecisionType> {
        let d = |n: &str, l: &[&str]| (n.to_string(), DecisionType { name: n.into(), options: l.join(" | "), listed: l.iter().map(|s| s.to_string()).collect(), at: Default::default(), judge: None });
        [d("end-of-turn", &["accept", "nudge", "escalate"]), d("needs-spellcheck", &["yes", "no"])].into()
    }

    fn check(src: &str) -> Loaded {
        let f = parse("t.net", src).unwrap();
        let b = bindings("b", "on turn_end use t\n").unwrap().into_iter().map(|b| (Scope2::Global, b)).collect();
        load(&calls(), &decisions(), vec![f], b)
    }

    #[test]
    fn a_turn_end_net_that_ends_exactly_once_passes() {
        let l = check(
            "net t on turn_end(t) session clerk: per agent {\n let ps = split(t.text) in clerk\n for p in ps where p.kind == Question { ask(t.agent, p.text) }\n match decide end-of-turn(agent: t.agent, nudged: nudged_in_row(t.agent)) {\n  accept => accept(wake: none)\n  nudge => nudge(t.agent, \"go on\")\n  escalate => { tell(owner, \"{{ t.agent }} stopped\"); accept(wake: answer(\"q1\")) }\n }\n}\n",
        );
        assert!(l.errors.is_empty(), "{:?}", l.errors);
        assert!(l.resolve("turn_end", Some("orch")).is_some());
    }

    #[test]
    fn refusals_name_their_line() {
        // escalate does not end the turn
        let l = check("net t on turn_end(t) {\n match decide end-of-turn(agent: t.agent) {\n  accept => accept(wake: none)\n  nudge => nudge(t.agent, \"x\")\n  escalate => tell(owner, \"x\")\n }\n}\n");
        assert!(l.errors.iter().any(|e| e.contains("not at all")), "{:?}", l.errors);
        assert!(l.resolve("turn_end", None).is_none());
        // a match that misses a case
        let l = check("net t on turn_end(t) {\n match clarity(t.text) fresh {\n  Clear => accept(wake: none)\n }\n}\n");
        assert!(l.errors.iter().any(|e| e.contains("t.net:2:") && e.contains("does not cover Unclear")), "{:?}", l.errors);
        // a step after a fault is dead
        let l = check("net t on turn_end(t) {\n fault oops(t.agent)\n accept(wake: none)\n}\n");
        assert!(l.errors.iter().any(|e| e.contains("dead step")), "{:?}", l.errors);
        // a field that does not exist
        let l = check("net t on turn_end(t) {\n tell(owner, t.who)\n accept(wake: none)\n}\n");
        assert!(l.errors.iter().any(|e| e.contains("t.net:2:") && e.contains("no field `who`")), "{:?}", l.errors);
        // a dead arm
        let l = check("net t on turn_end(t) {\n match clarity(t.text) fresh {\n  _ => accept(wake: none)\n  Clear => accept(wake: none)\n }\n}\n");
        assert!(l.errors.iter().any(|e| e.contains("dead arm")), "{:?}", l.errors);
    }

    #[test]
    fn a_message_lost_on_an_empty_list_is_refused_unless_guarded() {
        let lost = "net m on owner_message(m) session clerk: per owner {\n for p in split(m.text) in clerk { deliver(m.to, p.text, m.thread) }\n}\n";
        let f = parse("m.net", lost).unwrap();
        let b = bindings("b", "on owner_message use m\n").unwrap().into_iter().map(|b| (Scope2::Global, b)).collect();
        let l = load(&calls(), &decisions(), vec![f], b);
        assert!(l.errors.iter().any(|e| e.contains("not at all")), "{:?}", l.errors);
        let kept = "net m on owner_message(m) session clerk: per owner {\n let ps = split(m.text) in clerk\n if len(ps) == 0 { deliver(m.to, m.text, m.thread) } else { for p in ps { deliver(m.to, p.text, m.thread) } }\n}\n";
        let l = load(&calls(), &decisions(), vec![parse("m.net", kept).unwrap()], vec![]);
        assert!(l.errors.is_empty(), "{:?}", l.errors);
    }

    /// The shipped clerk (agent-base's nets, calls and policy) passes its check, and binds both events.
    #[test]
    fn the_shipped_clerk_passes() {
        let base = format!("{}/../../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let engine = crate::policy::Engine::start(crate::policy::Places { base: base.clone(), coord: std::env::temp_dir().display().to_string(), ..Default::default() });
        let l = super::super::load(&crate::policy::Places { base, ..Default::default() }, &engine);
        assert!(l.errors.is_empty(), "{:#?}", l.errors);
        assert_eq!(l.resolve("turn_end", Some("orch")).map(|n| n.name.as_str()), Some("turn_end"));
        assert_eq!(l.resolve("owner_message", None).map(|n| n.name.as_str()), Some("owner_message"));
        assert!(l.is_live("ask", Some("orch")) && l.is_live("post", Some("orch")));
        assert!(!l.is_live("deliver", Some("orch")) && !l.is_live("nudge", Some("orch")) && !l.is_live("accept", None));
    }

    #[test]
    fn the_narrowest_live_line_wins_and_untaken_effects_stay_dry() {
        let sw = |scope, t: &str| super::super::syntax::switches("live", t).unwrap().into_iter().map(move |s| (scope, s)).collect::<Vec<_>>();
        let mut all = sw(Scope2::Global, "live ask\nlive post\nlive deliver\n");
        all.extend(sw(Scope2::Project, "dry ask\nlive ask where agent in {orch}\ndry post where agent in {orch}\nlive post where agent in {orch}\n"));
        let (l, errors) = LiveSet::new(all, &["ask", "post"]);
        assert!(errors.iter().any(|e| e.contains("does not take `deliver`")), "{errors:?}");
        assert!(errors.iter().any(|e| e.contains("says the opposite")), "{errors:?}");
        assert!(!l.is_live("deliver", Some("orch")));
        assert!(!l.is_live("ask", Some("CONE")) && l.is_live("ask", Some("orch")));
        assert!(l.is_live("post", Some("CONE")) && !l.is_live("post", Some("orch")));
        assert!(super::super::syntax::switches("live", "live shout\n").unwrap_err().contains("no effect `shout`"));
    }

    #[test]
    fn raised_events_in_a_cycle_are_refused() {
        let src = "event ping { x: String }\nevent pong { x: String }\nnet a on ping(e) { raise pong(x: e.x) }\nnet b on pong(e) { raise ping(e.x) }\n";
        let b = bindings("b", "on ping use a\non pong use b\n").unwrap().into_iter().map(|b| (Scope2::Project, b)).collect();
        let l = load(&calls(), &decisions(), vec![parse("p.net", src).unwrap()], b);
        assert!(l.errors.iter().any(|e| e.contains("cycle")), "{:?}", l.errors);
        assert!(l.bindings.is_empty());
    }
}
