//! The policy engine: decisions,
//! preferences and typed LLM calls, compiled to answer set programs and solved by clingo, one engine per coordd.
//!
//! - [`syntax`]: the language (Fact, Decision, Prefer, Only, Judge); [`compile`]: to clingo (§4, §5, §14);
//!   [`solve`]: one clingo control on a thread of its own, decisions with their provenance (§8);
//!   [`facts`]: what the system supplies (the models from the proxy, their families and prices); [`call`]: LLM calls
//!   as typed programs (§7).
//! - Files: the system's prelude and global policy in `AGENT_BASE/policy/*.eflint` (the prelude declares the facts
//!   and the built-in decisions, `model-for`), its calls in `AGENT_BASE/calls/*.call`; a project's in
//!   `PROJECT/policy/*.eflint` (scope project), `PROJECT/policy/agents/NAME.eflint` (scope agent NAME) and
//!   `PROJECT/calls/*.call` (a call or type of the same name replaces the system's), PROJECT being the folder of the
//!   project file.
//! - Every decided call is recorded in `$COORD/decisions/log.jsonl` as a [`coord_types::Decision`], read back with
//!   `Request::Decision`.
//!
//! eFLINT's norms (Act, Duty) are not compiled yet: the parser refuses them by line, so a policy that has them does
//! not load rather than load without them.

pub mod call;
pub mod compile;
pub mod facts;
pub mod harness;
pub mod solve;
pub mod syntax;

use coord_types::{Attempt, Decision, Given};
use compile::Scope;
use serde_json::{Value, json};
use solve::{Instance, Solver};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How long the model facts are used before they are read from the proxy again.
const FACTS_FOR: Duration = Duration::from_secs(600);

/// Where the policy's, the calls' and the nets' files are.
#[derive(Clone, Debug, Default)]
pub struct Places {
    /// AGENT_BASE: `policy/`, `calls/` and `nets/` of the system.
    pub base: String,
    /// The project file's folder, if the project has one.
    pub project: Option<String>,
    /// $COORD: the proxy key, the decision log, the nets' log and sessions.
    pub coord: String,
    /// Texts read in place of the files at these paths (or as files there, when none is): an edit being tried on the
    /// Clerk page. Empty for the real thing.
    pub overlay: Overlay,
}

/// Edited texts by path (`Places::overlay`).
pub type Overlay = BTreeMap<String, String>;

/// The files of DIR ending in `.EXT`, with OVERLAY's texts in place of theirs and its paths in DIR added, by name.
pub fn files_in(dir: &str, ext: &str, overlay: &Overlay) -> Result<Vec<(String, String)>, String> {
    let mut paths: BTreeSet<String> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == ext)).map(|p| p.display().to_string()).collect())
        .unwrap_or_default();
    paths.extend(overlay.keys().filter(|p| std::path::Path::new(p.as_str()).parent().is_some_and(|d| d == std::path::Path::new(dir)) && p.ends_with(&format!(".{ext}"))).cloned());
    paths.into_iter().map(|p| Ok((p.clone(), text_at(&p, overlay)?))).collect()
}

/// The text of the file at PATH, OVERLAY's when it has one.
pub fn text_at(path: &str, overlay: &Overlay) -> Result<String, String> {
    match overlay.get(path) {
        Some(t) => Ok(t.clone()),
        None => std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}")),
    }
}

impl Places {
    /// The policy files by scope, in order: the system's (prelude first), the project's, each agent's.
    fn sources(&self) -> Result<Vec<(Scope, Vec<syntax::Decl>)>, String> {
        let read = |dir: &str| -> Result<Vec<(String, String)>, String> {
            let Ok(rd) = std::fs::read_dir(dir) else { return Ok(Vec::new()) };
            let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "eflint")).collect();
            // the prelude before the rest of its folder; then by name (file order is priority order within a scope)
            files.sort_by_key(|p| (p.file_stem().is_none_or(|s| s != "prelude"), p.clone()));
            files.into_iter().map(|p| Ok((p.display().to_string(), std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?))).collect()
        };
        let parse_all = |files: Vec<(String, String)>| -> Result<Vec<syntax::Decl>, String> {
            let mut out = Vec::new();
            for (f, t) in files {
                out.extend(syntax::parse(&f, &t)?);
            }
            Ok(out)
        };
        let mut scopes = vec![(Scope::Global, parse_all(read(&format!("{}/policy", self.base))?)?)];
        if let Some(p) = &self.project {
            scopes.push((Scope::Project, parse_all(read(&format!("{p}/policy"))?)?));
            for (f, t) in read(&format!("{p}/policy/agents"))? {
                let name = std::path::Path::new(&f).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                scopes.push((Scope::Agent(name), syntax::parse(&f, &t)?));
            }
        }
        Ok(scopes)
    }

    fn call_dirs(&self) -> Vec<String> {
        let mut dirs = vec![format!("{}/calls", self.base)];
        dirs.extend(self.project.as_ref().map(|p| format!("{p}/calls")));
        dirs
    }
}

/// The engine thread's state.
struct State {
    places: Places,
    proxy: facts::Proxy,
    solver: Result<Solver, String>,
    facts_at: Option<Instant>,
}

impl State {
    /// The solver with fresh facts: the policy compiled again and the models read again when stale and nothing is open
    /// (so an edited policy file takes effect within [`FACTS_FOR`]).
    fn ready(&mut self) -> Result<&mut Solver, String> {
        let stale = self.facts_at.is_none_or(|t| t.elapsed() > FACTS_FOR);
        let idle = self.solver.as_ref().map_or(true, |s| s.may_refresh());
        if stale && idle {
            let compiled = self.places.sources().and_then(|s| compile::compile(&s));
            self.solver = compiled.and_then(|c| {
                let mut s = Solver::new(c);
                s.set_facts(facts::models(&self.proxy)?)?;
                Ok(s)
            });
            self.facts_at = Some(Instant::now());
        }
        self.solver.as_mut().map_err(|e| format!("the policy: {e}"))
    }
}

type Job = Box<dyn FnOnce(&mut State) + Send>;

/// The engine: a handle to its thread.
#[derive(Clone)]
pub struct Engine {
    tx: mpsc::Sender<Job>,
    places: Places,
    proxy: facts::Proxy,
}

/// A standing context: a harness session (Codex or Claude Code), its harness and model decided
/// once at its start (and that decision), and the harness's own id for it, by which every later call resumes it.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub name: String,
    pub harness: Option<String>,
    pub model: Option<String>,
    pub decision: Option<String>,
    /// The harness's session (Codex thread, Claude Code session) id; none until the first call started it.
    pub id: Option<String>,
}

/// What a decided call gave.
pub struct Called {
    /// The answer (of the call's return type), when a model gave one.
    pub answer: Option<Value>,
    pub decision: Decision,
}

impl Engine {
    pub fn start(places: Places) -> Engine {
        let (tx, rx) = mpsc::channel::<Job>();
        let proxy = facts::Proxy::of(&places.coord);
        let (pl, px) = (places.clone(), proxy.clone());
        std::thread::Builder::new()
            .name("policy".into())
            .spawn(move || {
                // clingo's control is not Send: it is made and used on this thread only
                let mut st = State { places: pl, proxy: px, solver: Err("not loaded yet".into()), facts_at: None };
                for job in rx {
                    job(&mut st);
                }
            })
            .expect("the policy thread starts");
        Engine { tx, places, proxy }
    }

    /// Run F on the engine's thread and wait for its result (blocking: from a blocking task).
    fn with<R: Send + 'static>(&self, f: impl FnOnce(&mut State) -> R + Send + 'static) -> Result<R, String> {
        let (rtx, rrx) = mpsc::sync_channel(1);
        self.tx.send(Box::new(move |st| { let _ = rtx.send(f(st)); })).map_err(|_| "the policy thread ended".to_string())?;
        rrx.recv().map_err(|_| "the policy thread ended".to_string())
    }

    /// The calls and types of the system's and the project's `.call` files.
    pub fn calls(&self) -> Result<call::Calls, String> {
        call::Calls::load(&self.places.call_dirs(), &self.places.overlay)
    }

    /// The policy's decision types (compiled from the files, without facts: the nets' check needs no models).
    pub fn decisions(&self) -> Result<BTreeMap<String, compile::DecisionType>, String> {
        self.places.sources().and_then(|s| compile::compile(&s)).map(|c| c.decisions)
    }

    /// Run call NAME on INPUT (its parameters by name) for AGENT: the model decided by `model-for` (with the caller's
    /// GIVEN fields), a failed model excluded and the decision solved again until a model answers or none is left.
    /// The decision is recorded either way. Blocking.
    pub fn call(&self, name: &str, input: Value, agent: &str, given: Vec<Given>) -> Result<Called, String> {
        let calls = self.calls()?;
        let c = self.checked(&calls, name, &input)?;
        let fields = BTreeMap::from([("task".to_string(), c.task.clone()), ("agent".to_string(), agent.to_string()), ("call".to_string(), name.to_string())]);
        let (answer, decision) = self.decided("model-for", fields, given, |model| call::run(&calls, &c, &input, model, &self.proxy))?;
        Ok(Called { answer, decision })
    }

    /// Run call NAME on INPUT in SESSION (a standing context, §16.4): on its first call the session's model is decided
    /// by `model-for` with `call` = `session:NAME` and `place` = `session` (so the policy keeps to models a harness
    /// runs, `harness(option, _)`), falling back as [`Engine::call`] does, and the harness session is started; every
    /// later call resumes it by id on the same harness and model. A later call that fails twice gives the session up:
    /// a new one starts as a first call does, without the model that failed (the attempts show both).
    pub fn call_in(&self, name: &str, input: Value, agent: &str, session: &mut Session, given: Vec<Given>) -> Result<Called, String> {
        let calls = self.calls()?;
        let c = self.checked(&calls, name, &input)?;
        let coord = self.places.coord.clone();
        if let (Some(harness), Some(model), Some(id)) = (session.harness.clone(), session.model.clone(), session.id.clone()) {
            if let Some(g) = given.iter().find(|g| g.field == "model" && g.value != model) {
                return Err(format!("session_model: the session {} runs {model}; the call gives model {}", session.name, g.value));
            }
            let mut d = Decision { id: session.decision.clone().unwrap_or_default(), at: crate::core::now(), decision: "model-for".into(), chosen: Some(model.clone()), ..Default::default() };
            // once more on a failure (a harness hiccup), then the session is given up for a new one on another model
            for _ in 0..2 {
                match call::run_session(&calls, &c, &input, &harness, &model, &self.proxy, &coord, Some(&id)) {
                    Ok((a, _)) => {
                        d.attempts.push(Attempt { model, failure: None });
                        return Ok(Called { answer: Some(a), decision: d });
                    }
                    Err(f) => d.attempts.push(Attempt { model: model.clone(), failure: Some(format!("{f:?}")) }),
                }
            }
            let failed = model.clone();
            *session = Session { name: session.name.clone(), ..Default::default() };
            let mut called = self.call_in_new(&calls, &c, &input, agent, session, given, Some(&failed))?;
            d.attempts.append(&mut called.decision.attempts);
            called.decision.attempts = d.attempts;
            return Ok(called);
        }
        self.call_in_new(&calls, &c, &input, agent, session, given, None)
    }

    /// A new session for call C on INPUT: its model decided by `model-for` as for a session's first call, a model that
    /// fails excluded, and FAILED (the model the session it replaces failed on) never tried.
    #[allow(clippy::too_many_arguments)]
    fn call_in_new(&self, calls: &call::Calls, c: &call::Call, input: &Value, agent: &str, session: &mut Session, given: Vec<Given>, failed: Option<&str>) -> Result<Called, String> {
        let coord = self.places.coord.clone();
        let fields = BTreeMap::from([
            ("task".to_string(), c.task.clone()),
            ("agent".to_string(), agent.to_string()),
            ("call".to_string(), format!("session:{}", session.name)),
            ("place".to_string(), "session".to_string()),
        ]);
        let mut started = None;
        let (answer, decision) = self.decided("model-for", fields, given, |model| {
            if failed == Some(model) {
                return Err(call::Failure::Error(format!("{model} failed in the session this one replaces")));
            }
            let harness = self.harness(model).map_err(call::Failure::Error)?;
            call::run_session(calls, c, input, &harness, model, &self.proxy, &coord, None).map(|(a, id)| {
                started = Some((harness, id));
                a
            })
        })?;
        if let (Some(model), Some((harness, id))) = (&decision.chosen, started) {
            *session = Session { name: session.name.clone(), harness: Some(harness), model: Some(model.clone()), decision: Some(decision.id.clone()), id: Some(id) };
        }
        Ok(Called { answer, decision })
    }

    /// The harness that runs MODEL (the system fact `harness(model, H)`).
    fn harness(&self, model: &str) -> Result<String, String> {
        let m = model.to_string();
        self.with(move |st| st.ready().map(|s| s.value("harness", &m)))??.ok_or(format!("no harness runs {model}"))
    }

    /// Decide KIND for the call FIELDS (with the caller's GIVEN fields) without running anything: a decision whose
    /// options are its outcome (a net's `decide`, §16.3). Recorded like a call's.
    pub fn decide(&self, kind: &str, fields: BTreeMap<String, String>, given: Vec<Given>) -> Result<Decision, String> {
        Ok(self.decided(kind, fields, given, |_| Ok::<(), call::Failure>(()))?.1)
    }

    /// The decision KIND for FIELDS solved and RUN on its option, a failed option excluded and the decision solved
    /// again until one runs or none is left; recorded either way.
    fn decided<T>(&self, kind: &str, fields: BTreeMap<String, String>, given: Vec<Given>, mut run: impl FnMut(&str) -> Result<T, call::Failure>) -> Result<(Option<T>, Decision), String> {
        let inst = Instance { kind: kind.into(), call: fields, given };
        let mut record = Decision { id: String::new(), at: crate::core::now(), decision: inst.kind.clone(), call: inst.call.clone(), given: inst.given.clone(), ..Default::default() };
        let i = inst.clone();
        let d = match self.with(move |st| st.ready().and_then(|s| s.open(&i)))? {
            Ok(d) => d,
            Err(e) => {
                record.fault = Some(e);
                return Ok((None, self.record(record)));
            }
        };
        let mut out = None;
        loop {
            let (dd, i) = (d.clone(), inst.clone());
            let o = self.with(move |st| st.ready().and_then(|s| s.solve(&dd, &i)))??;
            (record.optimal, record.excluded, record.costs, record.runner_up, record.decided_by) = (o.optimal, o.excluded, o.costs, o.runner_up, o.decided_by);
            let Some(option) = o.chosen else {
                record.fault = o.fault.or(Some("no option left".into()));
                break;
            };
            match run(&option) {
                Ok(a) => {
                    record.attempts.push(Attempt { model: option.clone(), failure: None });
                    record.chosen = Some(option);
                    out = Some(a);
                    break;
                }
                Err(f) => {
                    record.attempts.push(Attempt { model: option.clone(), failure: Some(format!("{f:?}")) });
                    let (dd, m) = (d.clone(), option.clone());
                    self.with(move |st| st.ready().and_then(|s| s.fail(&dd, &m)))??;
                }
            }
        }
        let dd = d.clone();
        self.with(move |st| st.ready().and_then(|s| s.close(&dd)))??;
        Ok((out, self.record(record)))
    }

    /// Call NAME of CALLS, with INPUT checked against its parameters.
    fn checked(&self, calls: &call::Calls, name: &str, input: &Value) -> Result<call::Call, String> {
        let c = calls.calls.get(name).ok_or(format!("no call `{name}` in {}", self.places.call_dirs().join(", ")))?.clone();
        for (p, t) in &c.params {
            calls.conforms(t, input.get(p).unwrap_or(&Value::Null)).map_err(|e| format!("call {name} ({}): the input {p}: {e}", c.at))?;
        }
        Ok(c)
    }

    /// Append RECORD to the decision log under the next id (`D1`, `D2`, ...), on the engine's thread so that calls
    /// recorded at the same time get different ids.
    fn record(&self, record: Decision) -> Decision {
        let coord = self.places.coord.clone();
        let back = record.clone();
        self.with(move |_| append_record(&coord, record)).unwrap_or(back)
    }
}

fn append_record(coord: &str, mut record: Decision) -> Decision {
    let path = format!("{coord}/decisions/log.jsonl");
    record.id = format!("D{}", crate::jsonl::lines(&path) + 1);
    if let Err(e) = crate::jsonl::append(&path, &serde_json::to_string(&record).unwrap()) {
        eprintln!("coordd: {path}: {e}");
    }
    record
}

/// Decision ID of the log in COORD, as JSON.
pub fn decision(coord: &str, id: &str) -> Result<String, String> {
    let text = std::fs::read_to_string(format!("{coord}/decisions/log.jsonl")).map_err(|_| "no decisions yet".to_string())?;
    text.lines()
        .find(|l| serde_json::from_str::<Value>(l).is_ok_and(|v| v["id"] == json!(id)))
        .map(String::from)
        .ok_or(format!("no decision {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The slice end to end against the live proxy (`cargo test -p coordd -- --ignored live`): agent-base's prelude,
    /// defaults and `recognise-question`, the models the proxy serves, a decision recorded and read back; then a given
    /// field (`family = claude`) honoured. Needs COORD (for the proxy key) and AGENT_BASE.
    #[test]
    #[ignore]
    fn live_recognise_question_decides_calls_and_records() {
        let tmp = std::env::temp_dir().join(format!("coordd-policy-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let key = std::fs::read_to_string(format!("{}/.proxy-key", std::env::var("COORD").unwrap())).unwrap();
        std::fs::write(tmp.join(".proxy-key"), key).unwrap();
        let coord = tmp.display().to_string();
        let e = Engine::start(Places { base: std::env::var("AGENT_BASE").unwrap(), coord: coord.clone(), ..Default::default() });
        let input = json!({"message": {"from": "orch", "text": "The runner is green again. Should I merge core/l1 into dev now, or wait for the proof check?"}});
        let c = e.call("recognise-question", input.clone(), "orch", Vec::new()).unwrap();
        eprintln!("{}", serde_json::to_string_pretty(&c.decision).unwrap());
        assert!(c.answer.as_ref().is_some_and(|a| a["question"].is_string()), "{:?}", c.answer);
        assert!(c.decision.decided_by.is_some());
        assert!(decision(&coord, &c.decision.id).unwrap().contains(&c.decision.id));
        let given = vec![Given { field: "family".into(), value: "claude".into(), by: "test".into(), at: crate::core::now() }];
        let c = e.call("recognise-question", input, "orch", given).unwrap();
        eprintln!("{}", serde_json::to_string_pretty(&c.decision).unwrap());
        assert!(c.decision.chosen.as_deref().is_some_and(|m| m.starts_with("claude")), "{:?}", c.decision);
        std::fs::remove_dir_all(&tmp).ok();
    }
}
