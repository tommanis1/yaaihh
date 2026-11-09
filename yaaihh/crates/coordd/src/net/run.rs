//! Running a checked net on one event. Values
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

    fn path(&self, key: &str) -> String {
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
}

struct Stop(String);

struct Rt<'a> {
    engine: &'a Engine,
    sessions: &'a Sessions,
    world: &'a World,
    net: &'a Net,
    /// The agent the event is about (calls run for it; `per agent` sessions are its).
    agent: Option<String>,
    env: HashMap<String, Value>,
    out: Outcome,
}

/// Run NET on the event VALUE about AGENT.
pub fn run(engine: &Engine, sessions: &Sessions, world: &World, net: &Net, agent: Option<String>, value: Value) -> Outcome {
    let mut rt = Rt { engine, sessions, world, net, agent, env: HashMap::new(), out: Outcome::default() };
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
                    Effect::Fault(_) | Effect::Raise(_) => &[],
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
                    Effect::Raise(e) => self.out.raised.push((e.clone(), args)),
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
                let given = self.given(with, at.line)?;
                let agent = self.agent.clone().unwrap_or_else(|| "owner".into());
                let called = match place {
                    Place::Fresh => self.engine.call(name, input, &agent, given),
                    Place::In(s) => {
                        let per = self.net.sessions.iter().find(|d| &d.name == s).map(|d| d.per).unwrap_or(Per::Owner);
                        let key = match per {
                            Per::Agent => format!("{s}@{agent}"),
                            Per::Owner => format!("{s}@owner"),
                        };
                        let lock = self.sessions.lock(&key);
                        let _held = lock.lock().unwrap();
                        let mut session = self.sessions.load(&key, &key);
                        let r = self.engine.call_in(name, input, &agent, &mut session, given);
                        self.sessions.save(&key, &session);
                        r
                    }
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
