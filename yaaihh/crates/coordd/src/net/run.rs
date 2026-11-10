//! Running a checked net on one event (vault designs/policy-language-draft-2026-11-08.md §16.3–§16.5, §16.9). Values
//! are JSON; an enum value is an object whose `kind` names its variant. Every call is `model-for` and a run (fresh: a
//! decision per call; in a session: the session's model, decided at its first call); every `decide` is a policy
//! decision; both are recorded in the decision log and named in the run's steps. Effects are collected, not taken
//! here: a dry run logs them (coordd's nets are dry until the owner switches them on).

use super::syntax::{Arg, Effect, Expr, Net, Op, Part, Pat, Per, Place, Stmt};
use crate::policy::{Engine, Session};
use coord_types::{Given, NetEffect, NetStep};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// What a run may read of coordd's state, taken when it starts.
#[derive(Clone, Debug, Default)]
pub struct World {
    /// Each agent's open questions (`Question` values).
    pub questions: HashMap<String, Vec<Value>>,
    /// How many of each agent's latest turn ends in a row its net nudged.
    pub nudged: HashMap<String, i64>,
    /// Each running agent as the watch sees it (`Watched`: name, parent, state, held_min, idle_min, quiet_min, items).
    pub agents: Vec<Value>,
}

/// Where the sessions are kept, and a lock per session so one call runs in it at a time.
#[derive(Clone)]
pub struct Sessions {
    pub dir: String,
    locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

impl Sessions {
    pub fn new(dir: String) -> Sessions {
        Sessions { dir, locks: Arc::default() }
    }

    fn lock(&self, key: &str) -> Arc<Mutex<()>> {
        self.locks.lock().unwrap().entry(key.into()).or_default().clone()
    }

    pub fn path(&self, key: &str) -> String {
        format!("{}/{}.json", self.dir, key.replace('/', "_"))
    }

    fn load(&self, key: &str, name: &str) -> Session {
        // a session the harness never started (or a chat-history session of before m116) starts afresh
        std::fs::read_to_string(self.path(key))
            .ok()
            .and_then(|t| serde_json::from_str::<Session>(&t).ok())
            .filter(|s| s.id.is_some())
            .unwrap_or(Session { name: name.into(), ..Default::default() })
    }

    fn save(&self, key: &str, s: &Session) {
        let _ = std::fs::create_dir_all(&self.dir);
        let tmp = format!("{}.tmp", self.path(key));
        if std::fs::write(&tmp, serde_json::to_string(s).unwrap()).is_ok() {
            let _ = std::fs::rename(tmp, self.path(key));
        }
    }
}

/// The outcome of a run.
#[derive(Debug, Default)]
pub struct Outcome {
    pub steps: Vec<NetStep>,
    pub effects: Vec<NetEffect>,
    pub fault: Option<String>,
    /// Events raised (`raise`), by name with their value; a fault is raised as `fault` by the caller.
    pub raised: Vec<(String, Value)>,
    /// Timers set (`raise NAME(...) after D`): the event, its value, D in seconds.
    pub later: Vec<(String, Value, u64)>,
}

struct Stop(String);

/// Seconds since the epoch.
fn epoch() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

struct Rt<'a> {
    engine: &'a Engine,
    sessions: &'a Sessions,
    world: &'a World,
    net: &'a Net,
    /// The agent the event is about (calls run for it; `per agent` sessions are its).
    agent: Option<String>,
    env: HashMap<String, Value>,
    out: Outcome,
    /// This net's runs today, read once per run (`Rt::today`).
    today: std::cell::OnceCell<Vec<Value>>,
}

/// Run NET on the event VALUE about AGENT.
pub fn run(engine: &Engine, sessions: &Sessions, world: &World, net: &Net, agent: Option<String>, value: Value) -> Outcome {
    let mut rt = Rt { engine, sessions, world, net, agent, env: HashMap::new(), out: Outcome::default(), today: Default::default() };
    rt.env.insert(net.var.clone(), value);
    if let Err(Stop(f)) = rt.block(&net.body) {
        rt.out.fault = Some(f);
    }
    rt.out
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Object(o) if o.contains_key("kind") => o["kind"].as_str().unwrap_or("").to_string(),
        v => v.to_string(),
    }
}

impl<'a> Rt<'a> {
    fn block(&mut self, stmts: &[Stmt]) -> Result<(), Stop> {
        // names a block's `let`s bind are its own: an outer value of the same name comes back after it
        let mut shadowed: Vec<(String, Option<Value>)> = Vec::new();
        let r = (|| {
            for s in stmts {
                if let Stmt::Let { var, .. } = s {
                    if !shadowed.iter().any(|(v, _)| v == var) {
                        shadowed.push((var.clone(), self.env.get(var).cloned()));
                    }
                }
                self.stmt(s)?;
            }
            Ok(())
        })();
        for (v, old) in shadowed {
            match old {
                Some(x) => self.env.insert(v, x),
                None => self.env.remove(&v),
            };
        }
        r
    }

    fn stmt(&mut self, s: &Stmt) -> Result<(), Stop> {
        match s {
            Stmt::Let { var, value, at } => {
                let v = self.expr(value, at.line)?;
                self.env.insert(var.clone(), v);
            }
            Stmt::Set { path, value, add, at } => {
                let v = self.expr(value, at.line)?;
                let root = self.env.get_mut(&path[0]).ok_or(Stop(format!("line {}: `{}` is not defined", at.line, path[0])))?;
                let slot = path[1..].iter().fold(root, |cur, f| &mut cur[f.as_str()]);
                if *add {
                    if !slot.is_array() {
                        *slot = json!([]);
                    }
                    let arr = slot.as_array_mut().unwrap();
                    match v {
                        Value::Array(xs) => arr.extend(xs),
                        x => arr.push(x),
                    }
                } else {
                    *slot = v;
                }
            }
            Stmt::If { cond, then, els, at } => {
                if self.cond(cond, at.line)? {
                    self.block(then)?
                } else {
                    self.block(els)?
                }
            }
            Stmt::For { var, list, filter, body, at, .. } => {
                // `par for` runs one by one as well for now: a session takes one call at a time anyway
                let items = self.expr(list, at.line)?.as_array().cloned().unwrap_or_default();
                let old = self.env.get(var).cloned();
                for x in items {
                    self.env.insert(var.clone(), x);
                    if let Some(f) = filter {
                        if !self.cond(f, at.line)? {
                            continue;
                        }
                    }
                    self.block(body)?;
                }
                match old {
                    Some(x) => self.env.insert(var.clone(), x),
                    None => self.env.remove(var),
                };
            }
            Stmt::Match { scrut, arms, at } => {
                let v = self.expr(scrut, at.line)?;
                for arm in arms {
                    let mut binds: Vec<(String, Value)> = Vec::new();
                    let hit = match &arm.pat {
                        Pat::Wild => true,
                        Pat::None => v.is_null(),
                        Pat::Some(x) => {
                            binds.push((x.clone(), v.clone()));
                            !v.is_null()
                        }
                        Pat::Option(o) => v.as_str() == Some(o),
                        Pat::Variant(name, bs) => {
                            let hit = v["kind"].as_str() == Some(name);
                            if hit && !bs.is_empty() {
                                // fields by position, in the order the enum declares them (the variant of that name
                                // whose fields the value has)
                                let keys: std::collections::BTreeSet<&String> = v.as_object().map(|o| o.keys().filter(|k| *k != "kind").collect()).unwrap_or_default();
                                let calls = self.engine.calls().map_err(|e| Stop(format!("line {}: {e}", arm.at.line)))?;
                                let order: Vec<String> = calls
                                    .enums
                                    .values()
                                    .flatten()
                                    .find(|(n, fs)| n == name && fs.iter().map(|(f, _)| f).collect::<std::collections::BTreeSet<_>>() == keys)
                                    .map(|(_, fs)| fs.iter().map(|(f, _)| f.clone()).collect())
                                    .unwrap_or_default();
                                for (b, f) in bs.iter().zip(order) {
                                    binds.push((b.clone(), v[f.as_str()].clone()));
                                }
                            }
                            hit
                        }
                    };
                    if !hit {
                        continue;
                    }
                    let saved: Vec<(String, Option<Value>)> = binds.iter().map(|(b, _)| (b.clone(), self.env.get(b).cloned())).collect();
                    for (b, x) in binds {
                        self.env.insert(b, x);
                    }
                    if let Some(g) = &arm.guard {
                        if !self.cond(g, arm.at.line)? {
                            for (b, old) in saved {
                                match old {
                                    Some(x) => self.env.insert(b, x),
                                    None => self.env.remove(&b),
                                };
                            }
                            continue;
                        }
                    }
                    let r = self.block(&arm.body);
                    for (b, old) in saved {
                        match old {
                            Some(x) => self.env.insert(b, x),
                            None => self.env.remove(&b),
                        };
                    }
                    return r;
                }
                return Err(Stop(format!("line {}: no arm matched {v}", at.line)));
            }
            Stmt::Effect { effect, args, at } => {
                let names: &[&str] = match effect {
                    Effect::Deliver => &["to", "text", "thread"],
                    Effect::Reject => &["to", "why"],
                    Effect::Nudge => &["agent", "text"],
                    Effect::Ask => &["from", "text", "on"],
                    Effect::Post => &["from", "text", "thread"],
                    Effect::Tell => &["to", "text"],
                    Effect::Accept => &["wake"],
                    Effect::Requeue | Effect::Hold => &["job", "why"],
                    Effect::Evict => &["job", "entry", "why"],
                    Effect::Fault(_) | Effect::Raise(..) => &[],
                };
                let mut obj = serde_json::Map::new();
                for (k, a) in args.iter().enumerate() {
                    let name = a.name.clone().or_else(|| names.get(k).map(|s| s.to_string())).unwrap_or_else(|| format!("{k}"));
                    obj.insert(name, self.expr(&a.value, at.line)?);
                }
                let args = Value::Object(obj);
                self.out.effects.push(NetEffect { line: at.line, effect: effect.name(), args: args.clone(), ..Default::default() });
                match effect {
                    Effect::Fault(n) => return Err(Stop(format!("{n}: {}", args.as_object().unwrap().values().map(text).collect::<Vec<_>>().join(", ")))),
                    Effect::Raise(e, None) => self.out.raised.push((e.clone(), args)),
                    Effect::Raise(e, Some(d)) => self.out.later.push((e.clone(), args, *d)),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn cond(&mut self, e: &Expr, line: usize) -> Result<bool, Stop> {
        Ok(match self.expr(e, line)? {
            Value::Bool(b) => b,
            Value::String(s) => s == "yes", // a yes | no decision
            v => return Err(Stop(format!("line {line}: {v} is not a condition"))),
        })
    }

    fn args(&mut self, args: &[Arg], params: &[String], line: usize) -> Result<serde_json::Map<String, Value>, Stop> {
        let mut out = serde_json::Map::new();
        let mut pos = 0;
        for a in args {
            let name = match &a.name {
                Some(n) => n.clone(),
                None => {
                    pos += 1;
                    params.get(pos - 1).cloned().unwrap_or_default()
                }
            };
            out.insert(name, self.expr(&a.value, line)?);
        }
        for p in params {
            out.entry(p.clone()).or_insert(Value::Null);
        }
        Ok(out)
    }

    /// Call NAME in context S as PLACE says, or as `context-for` decides when the policy has that decision and the call
    /// did not fix the use (FIXED, `with use: X`). The context's lifetime ends it first (`for N calls`, `for D`);
    /// the owner's note on it (`KEY.note` beside it) goes before the prompt once.
    #[allow(clippy::too_many_arguments)]
    fn in_context(&mut self, name: &str, s: &str, place: &Place, fixed: Option<String>, input: Value, agent: &str, given: Vec<Given>, line: usize) -> Result<Result<crate::policy::Called, String>, Stop> {
        let decl = self.net.sessions.iter().find(|d| d.name == s).cloned();
        let per = decl.as_ref().map_or(Per::Owner, |d| d.per);
        let key = match per {
            Per::Agent => format!("{s}@{agent}"),
            Per::Owner => format!("{s}@owner"),
            Per::Day => format!("{s}@{}", &crate::core::now()[..10]),
        };
        let lock = self.sessions.lock(&key);
        let _held = lock.lock().unwrap();
        let mut session = self.sessions.load(&key, &key);
        let now = epoch();
        let spent = decl.as_ref().is_some_and(|d| d.calls.is_some_and(|n| session.calls >= n) || d.ttl.is_some_and(|t| session.began > 0 && now >= session.began + t));
        if spent {
            session = crate::policy::Session { name: key.clone(), ..Default::default() };
        }
        let written = match place {
            Place::With(_) => "with",
            _ => "in",
        };
        let used = match fixed {
            Some(u) => u,
            None => self.context_for(name, s, written, &session, now, line)?,
        };
        let note_file = format!("{}.note", self.sessions.path(&key).trim_end_matches(".json"));
        let note = std::fs::read_to_string(&note_file).map(|n| format!("A note from the owner on this context: {}", n.trim())).unwrap_or_default();
        let r = match used.as_str() {
            "fresh" => self.engine.call_prefaced(name, input, agent, given, &note),
            "digest" => {
                let preface = [note.clone(), self.digest()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("\n\n");
                self.engine.call_prefaced(name, input, agent, given, &preface)
            }
            "with" | "in" => {
                let fork = used == "with";
                let r = self.engine.call_in(name, input, agent, &mut session, given, &note, fork);
                if !fork && r.as_ref().is_ok_and(|c| c.answer.is_some()) {
                    session.began = if session.began == 0 { now } else { session.began };
                    session.calls += 1;
                    session.last = now;
                }
                self.sessions.save(&key, &session);
                r
            }
            other => return Err(Stop(format!("line {line}: context-for chose `{other}` (fresh, digest, with, in)"))),
        };
        if r.as_ref().is_ok_and(|c| c.answer.is_some()) && !note.is_empty() {
            let _ = std::fs::remove_file(&note_file);
        }
        self.out.steps.push(NetStep { line, what: format!("context {key}: {used}"), decision: None, model: None, out: None });
        Ok(r)
    }

    /// `context-for` (supervision-nets §8a), when the policy declares it: the use for call NAME in context S, written
    /// WRITTEN, from the facts: the context's calls, seconds since its last use (-1: never), the net. Else WRITTEN.
    fn context_for(&mut self, name: &str, s: &str, written: &str, session: &crate::policy::Session, now: u64, line: usize) -> Result<String, Stop> {
        if !self.engine.decisions().is_ok_and(|d| d.contains_key("context-for")) {
            return Ok(written.into());
        }
        let idle = if session.last == 0 { -1 } else { (now - session.last) as i64 };
        let fields = std::collections::BTreeMap::from([
            ("net".to_string(), self.net.name.clone()),
            ("call".to_string(), name.to_string()),
            ("context".to_string(), s.to_string()),
            ("written".to_string(), written.to_string()),
            ("calls".to_string(), session.calls.to_string()),
            ("idle".to_string(), idle.to_string()),
        ]);
        let d = self.engine.decide("context-for", fields, Vec::new()).map_err(|e| Stop(format!("line {line}: decide context-for: {e}")))?;
        self.out.steps.push(NetStep { line, what: "decide context-for".into(), decision: Some(d.id.clone()), model: None, out: d.chosen.clone().map(Value::String) });
        d.chosen.ok_or(Stop(format!("no_route: line {line}: decide context-for ({}): {}", d.id, d.fault.unwrap_or_default())))
    }

    /// This net's runs today (UTC) before this one, from the nets' log (its last 32 MB, a day of every net's runs
    /// with room: 10 Nov the clerk's alone wrote about 5 MB a day): the record a stateless net reads its history from
    /// (supervision-nets §8: the state lives in the logs). Read once per run; a line is parsed only when it names
    /// this net.
    fn today(&self) -> &Vec<Value> {
        self.today.get_or_init(|| {
            let log = format!("{}/log.jsonl", self.sessions.dir.trim_end_matches("/sessions"));
            let today = &crate::core::now()[..10];
            let named = format!("\"net\":\"{}\"", self.net.name);
            let tail = std::fs::File::open(&log).ok().map(|mut f| {
                use std::io::{Read, Seek, SeekFrom};
                let len = f.metadata().map_or(0, |m| m.len());
                let _ = f.seek(SeekFrom::Start(len.saturating_sub(32 << 20)));
                let mut b = Vec::new();
                let _ = f.read_to_end(&mut b);
                String::from_utf8_lossy(&b).into_owned()
            });
            tail.unwrap_or_default()
                .lines()
                .filter(|l| l.contains(&named))
                .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                .filter(|r| r["net"] == self.net.name.as_str() && r["at"].as_str().is_some_and(|a| a.starts_with(today)))
                .collect()
        })
    }

    /// The digest a `digest` call is given: this net's runs today, from the nets' log (its last 2 MB), the latest 20,
    /// each its time, event, agent, effects and fault. Written by code: the facts of the day, not a model's summary.
    fn digest(&self) -> String {
        let runs: Vec<String> = self
            .today()
            .iter()
            .map(|r| {
                let effects: Vec<String> = r["effects"].as_array().into_iter().flatten().map(|e| format!("{} {}", text(&e["effect"]), e["args"])).collect();
                let fault = r["fault"].as_str().map(|f| format!("; fault: {f}")).unwrap_or_default();
                format!("- {} {} {}: {}{fault}", text(&r["at"]), text(&r["event"]), text(&r["agent"]), if effects.is_empty() { "no effect".into() } else { effects.join("; ") })
            })
            .collect();
        let latest = &runs[runs.len().saturating_sub(20)..];
        if latest.is_empty() {
            return format!("Digest: net {} has not run today before this.", self.net.name);
        }
        format!("Digest: net {}'s runs today, oldest first ({} of {}):\n{}", self.net.name, latest.len(), runs.len(), latest.join("\n"))
    }

    fn given(&mut self, with: &[(String, Expr)], line: usize) -> Result<Vec<Given>, Stop> {
        with.iter()
            .map(|(f, e)| Ok(Given { field: f.clone(), value: text(&self.expr(e, line)?), by: format!("net {} line {line}", self.net.name), at: crate::core::now() }))
            .collect()
    }

    fn expr(&mut self, e: &Expr, line: usize) -> Result<Value, Stop> {
        Ok(match e {
            Expr::Path(p) => {
                let root = self.env.get(&p[0]).ok_or(Stop(format!("line {line}: `{}` is not defined", p[0])))?;
                p[1..].iter().fold(root, |v, f| v.get(f).unwrap_or(&Value::Null)).clone()
            }
            Expr::Str(parts) => {
                let mut s = String::new();
                for p in parts {
                    match p {
                        Part::Text(t) => s.push_str(t),
                        Part::Hole(h) => s.push_str(&text(&self.expr(h, line)?)),
                    }
                }
                Value::String(s)
            }
            Expr::Int(n) => json!(n),
            Expr::Bool(b) => json!(b),
            Expr::None => Value::Null,
            Expr::Owner => json!("owner"),
            Expr::Variant(v) => json!({"kind": v}),
            Expr::Cmp(a, op, b) => {
                let (x, y) = (self.expr(a, line)?, self.expr(b, line)?);
                // an enum value compares with a bare variant by its kind
                let key = |v: &Value| if v.is_object() && v.get("kind").is_some() { v["kind"].clone() } else { v.clone() };
                let (x, y) = (key(&x), key(&y));
                json!(match op {
                    Op::Eq => x == y,
                    Op::Ne => x != y,
                    _ => {
                        let (Some(a), Some(b)) = (x.as_i64(), y.as_i64()) else { return Err(Stop(format!("line {line}: {x} and {y} are not numbers"))) };
                        match op {
                            Op::Lt => a < b,
                            Op::Le => a <= b,
                            Op::Gt => a > b,
                            _ => a >= b,
                        }
                    }
                })
            }
            Expr::And(a, b) => json!(self.cond(a, line)? && self.cond(b, line)?),
            Expr::Or(a, b) => json!(self.cond(a, line)? || self.cond(b, line)?),
            Expr::Not(a) => json!(!self.cond(a, line)?),
            Expr::Builtin { name, args } => {
                let vals: Vec<Value> = args.iter().map(|a| self.expr(&a.value, line)).collect::<Result<_, _>>()?;
                let first = vals.first().cloned().unwrap_or(Value::Null);
                match name.as_str() {
                    "questions" => json!(self.world.questions.get(&text(&first)).cloned().unwrap_or_default()),
                    "all_questions" => json!(self.world.questions.values().flatten().cloned().collect::<Vec<_>>()),
                    "nudged_in_row" => json!(self.world.nudged.get(&text(&first)).copied().unwrap_or(0)),
                    "joined" => json!(first.as_array().map(|xs| xs.iter().map(text).collect::<Vec<_>>().join(", ")).unwrap_or_default()),
                    "len" => json!(first.as_array().map(|a| a.len()).or(first.as_str().map(|s| s.chars().count())).unwrap_or(0)),
                    "answer" => json!({"kind": "Answer", "q": first}),
                    "check" => json!({"kind": "Check", "check": first}),
                    "mail" => json!({"kind": "Mail", "from": first}),
                    "agents" => json!(self.world.agents),
                    "runs_today" => {
                        let (field, value) = (text(&first), vals.get(1).cloned().unwrap_or(Value::Null));
                        json!(self.today().iter().filter(|r| r["input"][&field] == value).count())
                    }
                    "marked_today" => {
                        let mark = text(&first);
                        let marked = |r: &Value| r["effects"].as_array().into_iter().flatten().filter(|e| e["args"]["text"].as_str().is_some_and(|t| t.contains(&mark))).count();
                        json!(self.today().iter().map(marked).sum::<usize>())
                    }
                    other => return Err(Stop(format!("line {line}: no built-in {other}"))),
                }
            }
            Expr::Decide { name, args, with, at } => {
                let mut fields = std::collections::BTreeMap::new();
                for a in args {
                    let v = self.expr(&a.value, at.line)?;
                    if !v.is_null() {
                        fields.insert(a.name.clone().unwrap_or_default(), text(&v));
                    }
                }
                if let Some(agent) = &self.agent {
                    fields.entry("agent".into()).or_insert(agent.clone());
                }
                let given = self.given(with, at.line)?;
                let d = self.engine.decide(name, fields, given).map_err(|e| Stop(format!("line {}: decide {name}: {e}", at.line)))?;
                self.out.steps.push(NetStep { line: at.line, what: format!("decide {name}"), decision: Some(d.id.clone()), model: None, out: d.chosen.clone().map(Value::String) });
                match d.chosen {
                    Some(c) => Value::String(c),
                    None => return Err(Stop(format!("no_route: line {}: decide {name} ({}): {}", at.line, d.id, d.fault.unwrap_or_default()))),
                }
            }
            Expr::Call { name, args, place, with, at } => {
                let calls = self.engine.calls().map_err(|e| Stop(format!("line {}: {e}", at.line)))?;
                let c = calls.calls.get(name).ok_or(Stop(format!("line {}: no call {name}", at.line)))?.clone();
                let params: Vec<String> = c.params.iter().map(|(p, _)| p.clone()).collect();
                let input = Value::Object(self.args(args, &params, at.line)?);
                // `with use: X` fixes the context's use (no `context-for`); the other fields go to `model-for`
                let (fixed, fields): (Vec<_>, Vec<_>) = with.iter().cloned().partition(|(f, _)| f == "use");
                let fixed = match fixed.first() {
                    Some((_, e)) => Some(text(&self.expr(e, at.line)?)),
                    None => None,
                };
                let given = self.given(&fields, at.line)?;
                let agent = self.agent.clone().unwrap_or_else(|| "owner".into());
                let called = match place.context() {
                    None => {
                        let preface = if *place == Place::Digest { self.digest() } else { String::new() };
                        self.engine.call_prefaced(name, input, &agent, given, &preface)
                    }
                    Some(s) => self.in_context(name, s, place, fixed, input, &agent, given, at.line)?,
                }
                .map_err(|e| Stop(format!("line {}: call {name}: {e}", at.line)))?;
                let model = called.decision.chosen.clone().or_else(|| called.decision.attempts.last().map(|a| a.model.clone()));
                self.out.steps.push(NetStep { line: at.line, what: format!("{name}"), decision: Some(called.decision.id.clone()), model, out: called.answer.clone() });
                match called.answer {
                    Some(a) => a,
                    None => {
                        let why = called.decision.fault.clone().or_else(|| called.decision.attempts.last().and_then(|a| a.failure.clone())).unwrap_or_default();
                        return Err(Stop(format!("call_failed: line {}: {name} ({}): {why}", at.line, called.decision.id)));
                    }
                }
            }
        })
    }
}
