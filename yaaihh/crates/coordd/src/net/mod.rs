//! Nets: process models bound to coordd's events, the
//! clerk among them. [`syntax`] parses `.net` files and bindings; [`check`] refuses a net that is ill-typed, misses a
//! case, can end its event wrongly, is unsound as a workflow net, or raises events in a cycle; [`run`] runs one on an
//! event with the policy engine (`crate::policy`) deciding every model and every `decide`.
//!
//! - Files: the system's `AGENT_BASE/nets/*.net` and `AGENT_BASE/nets/bindings` (scope global), the project's
//!   `PROJECT/nets/*.net` (a net of the same name replaces the system's) and `PROJECT/nets/bindings` (scope project;
//!   a `where agent in {..}` binding is scope agent). Read again on every event, so an edit takes effect at the next.
//! - Live set: every run's effects are logged (`$COORD/nets/log.jsonl`, told as `Body::Net`); an effect is also
//!   taken only when the live set switches it on: `AGENT_BASE/nets/live` (scope global) and `PROJECT/nets/live`
//!   (scope project), lines `live EFFECT [where agent in {..}]` or `dry EFFECT [...]`, the narrowest line winning as
//!   for bindings. Everything else is dry. coordd takes
//!   [`TAKEN`] so far; a `live` line for another effect is refused.
//! - One run at a time per agent (and one for events about no agent), so a session takes one call at a time and an
//!   agent's events are handled in order; agents' runs go on side by side.
//! - Raised events (`raise`) and faults go back in as events: a fault as `fault`, except from a net on `fault` itself
//!   (a fault's own fault is only logged, so no fault can feed itself).

pub mod check;
pub mod draw;
pub mod run;
pub mod syntax;

use crate::policy::Engine;
use coord_types::NetRun;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

type Shared = Arc<Mutex<crate::core::Core>>;

/// The effects coordd can take for real so far: filing an agent's turn (`ask` opens a question, `post` puts the
/// agent's words in its question's thread) and passing a message on (`deliver`, `reject`: with `deliver` live a
/// message is held until its net ran, `crate::core::Core::settle`). `nudge`, `tell` and `accept` are only logged.
pub const TAKEN: &[&str] = &["ask", "post", "deliver", "reject"];

/// An event for the nets: its name, the agent it is about, its value; DEPTH counts raises from raises, as a last
/// guard (the check refuses cycles already).
#[derive(Clone, Debug)]
pub struct Raised {
    pub event: String,
    pub agent: Option<String>,
    pub value: Value,
    pub depth: u32,
}

const MAX_DEPTH: u32 = 8;

pub use crate::policy::Places;

/// The nets' folders of PLACES by scope, widest first.
fn dirs(places: &Places) -> Vec<(check::Scope2, String)> {
    let mut dirs = vec![(check::Scope2::Global, format!("{}/nets", places.base))];
    dirs.extend(places.project.as_ref().map(|p| (check::Scope2::Project, format!("{p}/nets"))));
    dirs
}

/// Parse every `.net` file and the bindings: the files, the bindings by scope, and what did not parse.
fn read(places: &Places) -> (Vec<syntax::File>, Vec<(check::Scope2, syntax::Binding)>, Vec<String>) {
    let (mut files, mut bindings, mut errors) = (Vec::new(), Vec::new(), Vec::new());
    for (scope, dir) in dirs(places) {
        match crate::policy::files_in(&dir, "net", &places.overlay) {
            Ok(fs) => {
                for (f, t) in fs {
                    match syntax::parse(&f, &t) {
                        Ok(x) => files.push(x),
                        Err(e) => errors.push(format!("refused: {e}")),
                    }
                }
            }
            Err(e) => errors.push(format!("refused: {e}")),
        }
        let b = format!("{dir}/bindings");
        if let Ok(t) = crate::policy::text_at(&b, &places.overlay) {
            match syntax::bindings(&b, &t) {
                Ok(bs) => bindings.extend(bs.into_iter().map(|x| (scope, x))),
                Err(e) => errors.push(format!("refused: {e}")),
            }
        }
    }
    (files, bindings, errors)
}

/// The live set of PLACES (`nets/live` of each scope), and its refusals.
pub fn live(places: &Places) -> (check::LiveSet, Vec<String>) {
    let (mut lines, mut errors) = (Vec::new(), Vec::new());
    for (scope, dir) in dirs(places) {
        let l = format!("{dir}/live");
        if let Ok(t) = crate::policy::text_at(&l, &places.overlay) {
            match syntax::switches(&l, &t) {
                Ok(ss) => lines.extend(ss.into_iter().map(|x| (scope, x))),
                Err(e) => errors.push(format!("refused: {e} (every effect stays dry)")),
            }
        }
    }
    let (set, mut refused) = check::LiveSet::new(lines, TAKEN);
    errors.append(&mut refused);
    (set, errors)
}

/// Load and check the nets (blocking: reads files, compiles the policy).
pub(crate) fn load(places: &Places, engine: &Engine) -> check::Loaded {
    let (files, bindings, mut errors) = read(places);
    let calls = match engine.calls() {
        Ok(c) => c,
        Err(e) => {
            return check::Loaded { errors: vec![format!("the calls: {e}")], ..Default::default() };
        }
    };
    let decisions = match engine.decisions() {
        Ok(d) => d,
        Err(e) => {
            return check::Loaded { errors: vec![format!("the policy: {e}")], ..Default::default() };
        }
    };
    let mut l = check::load(&calls, &decisions, files, bindings);
    let (live, mut refused) = live(places);
    l.live = live;
    errors.append(&mut refused);
    errors.append(&mut l.errors);
    l.errors = errors;
    l
}

/// Start the nets: events sent on the returned channel are run by the net bound to them.
pub fn start(shared: Shared, places: Places, engine: Engine) -> mpsc::UnboundedSender<Raised> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Raised>();
    let sessions = run::Sessions::new(format!("{}/nets/sessions", places.coord));
    let back = tx.clone();
    let nudged: Arc<std::sync::Mutex<HashMap<String, i64>>> = Arc::new(std::sync::Mutex::new(nudge_counts(&places.coord)));
    tokio::spawn(async move {
        // one worker per agent (and "" for events about none): each runs its events in order
        let mut workers: HashMap<String, mpsc::UnboundedSender<Raised>> = HashMap::new();
        while let Some(ev) = rx.recv().await {
            let key = ev.agent.clone().unwrap_or_default();
            let w = workers.entry(key).or_insert_with(|| {
                let (wtx, mut wrx) = mpsc::unbounded_channel::<Raised>();
                let (shared, places, engine, sessions, back, nudged) = (shared.clone(), places.clone(), engine.clone(), sessions.clone(), back.clone(), nudged.clone());
                tokio::spawn(async move {
                    while let Some(ev) = wrx.recv().await {
                        let held = message_id(&ev);
                        one(&shared, &places, &engine, &sessions, &back, &nudged, ev).await;
                        // a message whose net did not run (none bound, refused, ...) is passed on unchanged
                        if let Some(id) = held {
                            let mut core = shared.lock().await;
                            let d = core.settle(&id, &mut [], None);
                            core.deliver_all(d).await;
                        }
                    }
                });
                wtx
            });
            let _ = w.send(ev);
        }
    });
    tx
}

/// Handle one event: the net bound to it, run, logged and told; what it raised sent back in.
async fn one(shared: &Shared, places: &Places, engine: &Engine, sessions: &run::Sessions, back: &mpsc::UnboundedSender<Raised>, nudged: &Arc<std::sync::Mutex<HashMap<String, i64>>>, ev: Raised) {
    let (p, e) = (places.clone(), engine.clone());
    let Ok(loaded) = tokio::task::spawn_blocking(move || load(&p, &e)).await else { return };
    shared.lock().await.net_check(loaded.errors.clone());
    let Some(net) = loaded.resolve(&ev.event, ev.agent.as_deref()).cloned() else { return };
    let mut world = shared.lock().await.net_world();
    world.nudged = nudged.lock().unwrap().clone();
    let value = ev.value.clone();
    let (e, s, n, a, v) = (engine.clone(), sessions.clone(), net.clone(), ev.agent.clone(), value.clone());
    let Ok(out) = tokio::task::spawn_blocking(move || run::run(&e, &s, &world, &n, a, v)).await else { return };
    // a turn end's nudges in a row, for nudged_in_row and the cap on it
    if ev.event == "turn_end" {
        if let Some(agent) = &ev.agent {
            let mut n = nudged.lock().unwrap();
            if out.effects.iter().any(|x| x.effect == "nudge") {
                *n.entry(agent.clone()).or_default() += 1;
            } else if out.fault.is_none() {
                n.insert(agent.clone(), 0);
            }
        }
    }
    // the live set's effects taken, in order (each run's effects come before a fault that stopped it, so they stand)
    let mut effects = out.effects;
    let (mut filed, mut asked) = (Vec::new(), Vec::new());
    let mut core = shared.lock().await;
    for e in effects.iter_mut().filter(|e| loaded.is_live(&e.effect, ev.agent.as_deref())) {
        e.live = true;
        let done = match e.effect.as_str() {
            "ask" => core.net_ask(&e.args).map(|m| {
                asked.push(json!({"q": m.id, "from": m.from, "text": m.body}));
                m.id
            }),
            "post" => core.net_post(&e.args),
            "deliver" | "reject" => continue, // the message's, below
            other => Err(format!("coordd does not take `{other}`")),
        };
        e.done = Some(match done {
            Ok(id) => {
                filed.push(id.clone());
                id
            }
            Err(why) => format!("not taken: {why}"),
        });
    }
    // a message held for its net: passed on, replaced or sent back as the run says; all or nothing (a run that failed
    // passes it on unchanged, one that faulted holds it and tells the sender)
    let mut deliver = Vec::new();
    if let Some(id) = message_id(&ev) {
        let declared = effects.iter().find(|e| e.effect.starts_with("fault ")).and(out.fault.clone());
        let failed = out.fault.is_some() && declared.is_none();
        let mut gate: Vec<&mut coord_types::NetEffect> = effects.iter_mut().filter(|e| e.live && (e.effect == "deliver" || e.effect == "reject")).collect();
        if failed || declared.is_some() {
            for e in gate.iter_mut() {
                e.done = Some(format!("not taken: the run {}", if failed { "failed, so the mail passed on unchanged" } else { "faulted, so the mail is held" }));
            }
            gate.clear();
        }
        deliver = core.settle(&id, &mut gate, declared.as_deref());
    }
    let record = NetRun {
        id: String::new(),
        at: crate::core::now(),
        net: net.name.clone(),
        event: ev.event.clone(),
        agent: ev.agent.clone(),
        input: value,
        steps: out.steps,
        dry: !effects.iter().any(|e| e.live),
        effects,
        fault: out.fault.clone(),
        raised: out.raised.iter().map(|(n, _)| n.clone()).collect(),
        marks: Vec::new(),
    };
    let record = log(&places.coord, record);
    core.mark_filed(&filed, &record.id);
    core.net_ran(record.clone());
    core.deliver_all(deliver).await;
    drop(core);
    if ev.depth < MAX_DEPTH {
        for q in asked {
            let _ = back.send(Raised { event: "question".into(), agent: ev.agent.clone(), value: q, depth: ev.depth + 1 });
        }
    }
    if ev.depth >= MAX_DEPTH {
        return;
    }
    for (name, v) in out.raised {
        let _ = back.send(Raised { event: name, agent: ev.agent.clone(), value: v, depth: ev.depth + 1 });
    }
    if let (Some(f), true) = (&out.fault, ev.event != "fault") {
        let step = f.split(':').next().unwrap_or("").to_string();
        let v = json!({"net": net.name, "step": step, "name": step, "detail": f, "run": record.id});
        let _ = back.send(Raised { event: "fault".into(), agent: ev.agent.clone(), value: v, depth: ev.depth + 1 });
    }
}

/// The nets as the Clerk page shows them: every file of each scope (an edit of one is saved as the project's file of
/// the same name, so it replaces the system's), the nets that passed with the calls they make and where they are
/// bound, and the refusals.
pub fn view(places: &Places) -> coord_types::NetsView {
    let engine = Engine::start(places.clone());
    let loaded = load(places, &engine);
    let mut sources = Vec::new();
    let target = |kind: &str, path: &str| -> String {
        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        places.project.as_ref().map_or(path.to_string(), |p| format!("{p}/{kind}/{name}"))
    };
    let mut scopes = vec![("global", places.base.clone())];
    scopes.extend(places.project.as_ref().map(|p| ("project", p.clone())));
    for (scope, root) in scopes {
        let mut add = |kind: &str, path: String, text: String, folder: &str| {
            sources.push(coord_types::NetSource { target: target(folder, &path), path, scope: scope.into(), kind: kind.into(), text });
        };
        for (path, text) in crate::policy::files_in(&format!("{root}/nets"), "net", &places.overlay).unwrap_or_default() {
            add("net", path, text, "nets");
        }
        for f in ["bindings", "live"] {
            let path = format!("{root}/nets/{f}");
            if let Ok(text) = std::fs::read_to_string(&path) {
                add(f, path, text, "nets");
            }
        }
        for (path, text) in crate::policy::files_in(&format!("{root}/calls"), "call", &places.overlay).unwrap_or_default() {
            add("call", path, text, "calls");
        }
    }
    let nets = loaded
        .nets
        .values()
        .map(|n| coord_types::NetInfo {
            name: n.name.clone(),
            event: n.event.clone(),
            file: n.at.file.clone(),
            line: n.at.line,
            calls: n.calls(),
            bound: loaded
                .bindings
                .iter()
                .filter(|(_, b)| b.net == n.name)
                .map(|(sc, b)| match sc {
                    check::Scope2::Agent => format!("agent {}", b.agents.join(", ")),
                    check::Scope2::Project => "project".into(),
                    check::Scope2::Global => "global".into(),
                })
                .collect(),
        })
        .collect();
    coord_types::NetsView { sources, nets, errors: loaded.errors, marks: marks(&places.coord) }
}

/// The refusals of the nets with EDITS that the files as they are do not have.
fn new_refusals(places: &Places, edits: &[coord_types::NetEdit]) -> Vec<String> {
    let before = load(places, &Engine::start(places.clone())).errors;
    let mut p = places.clone();
    p.overlay.extend(edits.iter().map(|e| (e.target.clone(), e.text.clone())));
    let after = load(&p, &Engine::start(p.clone())).errors;
    after.into_iter().filter(|e| !before.contains(e)).collect()
}

/// `Request::NetTry`: the net bound to EVENT (for AGENT) run dry on VALUE with EDITS, in fresh sessions (the clerk's own
/// are not touched), as a run with id `try` (not logged; its calls' decisions are). WORLD: coordd's state now.
pub fn try_run(places: &Places, edits: &[coord_types::NetEdit], event: &str, agent: Option<String>, value: Value, world: run::World) -> Result<NetRun, String> {
    let refused = new_refusals(places, edits);
    if !refused.is_empty() {
        return Err(refused.join("\n"));
    }
    let mut p = places.clone();
    p.overlay.extend(edits.iter().map(|e| (e.target.clone(), e.text.clone())));
    let engine = Engine::start(p.clone());
    let loaded = load(&p, &engine);
    try_loaded(places, &engine, &loaded, event, agent, value, world)
}

/// A dry run of the net LOADED binds to EVENT (for AGENT) on VALUE, in fresh sessions; a field of the event that
/// VALUE lacks (a run logged before the field was added) gets its type's empty value.
fn try_loaded(places: &Places, engine: &Engine, loaded: &check::Loaded, event: &str, agent: Option<String>, mut value: Value, world: run::World) -> Result<NetRun, String> {
    let net = loaded.resolve(event, agent.as_deref()).cloned().ok_or(format!("no net is bound to {event}{}", agent.as_ref().map(|a| format!(" for {a}")).unwrap_or_default()))?;
    if let (Some(decl), Some(obj)) = (loaded.events.get(event), value.as_object_mut()) {
        for (f, t) in &decl.fields {
            obj.entry(f.clone()).or_insert_with(|| match t.as_str() {
                t if t.starts_with('[') => json!([]),
                "Bool" => json!(false),
                "Int" => json!(0),
                "String" => json!(""),
                _ => Value::Null,
            });
        }
    }
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = format!("{}/nets/try/{}-{}", places.coord, std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let sessions = run::Sessions::new(dir.clone());
    let out = run::run(engine, &sessions, &world, &net, agent.clone(), value.clone());
    let _ = std::fs::remove_dir_all(&dir);
    Ok(NetRun {
        id: "try".into(),
        at: crate::core::now(),
        net: net.name.clone(),
        event: event.into(),
        agent,
        input: value,
        steps: out.steps,
        effects: out.effects,
        fault: out.fault,
        raised: out.raised.into_iter().map(|(n, _)| n).collect(),
        dry: true,
        marks: Vec::new(),
    })
}

/// The standing marks in COORD (`nets/marks.jsonl`): the latest per part of a run, those taken back left out.
pub fn marks(coord: &str) -> Vec<coord_types::NetMark> {
    let text = std::fs::read_to_string(format!("{coord}/nets/marks.jsonl")).unwrap_or_default();
    let mut out: Vec<coord_types::NetMark> = Vec::new();
    for m in text.lines().filter_map(|l| serde_json::from_str::<coord_types::NetMark>(l).ok()) {
        out.retain(|x| (&x.run, &x.kind, &x.what, x.k) != (&m.run, &m.kind, &m.what, m.k));
        out.push(m);
    }
    out.retain(|m| m.verdict != "none");
    out
}

/// `Request::NetMark`: MARK recorded (now), unless it names no part of a run in the log.
pub fn mark(coord: &str, mut mark: coord_types::NetMark) -> Result<coord_types::NetMark, String> {
    if !["step", "effect"].contains(&mark.kind.as_str()) || !["right", "wrong", "none"].contains(&mark.verdict.as_str()) {
        return Err(format!("a mark is on a step or an effect, right, wrong or none: {mark:?}"));
    }
    let run = find_run(coord, &mark.run).ok_or(format!("no run {}", mark.run))?;
    if part(&run, &mark.kind, &mark.what, mark.k).is_none() {
        return Err(format!("{} has no {} {} #{}", mark.run, mark.kind, mark.what, mark.k));
    }
    mark.at = crate::core::now();
    crate::jsonl::append(&format!("{coord}/nets/marks.jsonl"), &serde_json::to_string(&mark).unwrap()).map_err(|e| e.to_string())?;
    Ok(mark)
}

/// RUN with its standing marks MARKS attached.
pub fn with_marks(mut run: NetRun, marks: &[coord_types::NetMark]) -> NetRun {
    run.marks = marks.iter().filter(|m| m.run == run.id).cloned().collect();
    run
}

/// Run ID of COORD's log.
fn find_run(coord: &str, id: &str) -> Option<NetRun> {
    let text = std::fs::read_to_string(format!("{coord}/nets/log.jsonl")).ok()?;
    let needle = format!("\"id\":\"{id}\"");
    text.lines().filter(|l| l.contains(&needle)).filter_map(|l| serde_json::from_str::<NetRun>(l).ok()).find(|r| r.id == id)
}

/// What the K-th step or effect named WHAT of RUN gave: a step's answer, an effect's values.
fn part(run: &NetRun, kind: &str, what: &str, k: usize) -> Option<Value> {
    match kind {
        "step" => run.steps.iter().filter(|s| s.what == what).nth(k).map(|s| s.out.clone().unwrap_or(Value::Null)),
        _ => run.effects.iter().filter(|e| e.effect == what).nth(k).map(|e| e.args.clone()),
    }
}

/// `Request::NetReplay`: every run with a standing mark run again dry, with EDITS in place of the files, and each
/// marked part beside what it gave then (a part the new run has not: `after` none). WORLD: coordd's state now, for
/// what a run's event does not carry itself.
pub fn replay(places: &Places, edits: &[coord_types::NetEdit], world: run::World) -> Result<Vec<coord_types::Replayed>, String> {
    let refused = new_refusals(places, edits);
    if !refused.is_empty() {
        return Err(refused.join("\n"));
    }
    let mut p = places.clone();
    p.overlay.extend(edits.iter().map(|e| (e.target.clone(), e.text.clone())));
    let engine = Engine::start(p.clone());
    let loaded = load(&p, &engine);
    let marks = marks(&places.coord);
    let mut runs: Vec<String> = marks.iter().map(|m| m.run.clone()).collect();
    runs.dedup();
    let mut out = Vec::new();
    for id in runs.iter().collect::<std::collections::BTreeSet<_>>() {
        let mine: Vec<&coord_types::NetMark> = marks.iter().filter(|m| &m.run == id).collect();
        let old = find_run(&places.coord, id);
        let new = match &old {
            Some(r) => try_loaded(places, &engine, &loaded, &r.event, r.agent.clone(), r.input.clone(), world.clone()),
            None => Err(format!("no run {id} in the log")),
        };
        for m in mine {
            let before = old.as_ref().and_then(|r| part(r, &m.kind, &m.what, m.k));
            let (after, error) = match &new {
                Ok(r) => (part(r, &m.kind, &m.what, m.k), r.fault.clone()),
                Err(e) => (None, Some(e.clone())),
            };
            out.push(coord_types::Replayed { mark: m.clone(), before, after, error });
        }
    }
    Ok(out)
}

/// `Request::NetSave`: EDIT written to its target (a net, bindings, live or call file under a scope's `nets/` or
/// `calls/`) when the nets with it pass their check; else the refusals it would add.
pub fn save(places: &Places, edit: &coord_types::NetEdit) -> Result<String, String> {
    let path = std::path::Path::new(&edit.target);
    let (dir, name) = (path.parent().map(|d| d.display().to_string()).unwrap_or_default(), path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
    let roots: Vec<&String> = std::iter::once(&places.base).chain(places.project.as_ref()).collect();
    let in_scope = roots.iter().any(|r| dir == format!("{r}/nets") || dir == format!("{r}/calls"));
    let kind_ok = (dir.ends_with("/nets") && (name.ends_with(".net") || name == "bindings" || name == "live")) || (dir.ends_with("/calls") && name.ends_with(".call"));
    if !in_scope || !kind_ok {
        return Err(format!("{}: not a nets or calls file of the system or the project", edit.target));
    }
    let refused = new_refusals(places, std::slice::from_ref(edit));
    if !refused.is_empty() {
        return Err(refused.join("\n"));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{dir}: {e}"))?;
    std::fs::write(&edit.target, &edit.text).map_err(|e| format!("{}: {e}", edit.target))?;
    Ok(format!("saved {}", edit.target))
}

/// The mail a message event is about.
fn message_id(ev: &Raised) -> Option<String> {
    matches!(ev.event.as_str(), "owner_message" | "agent_message").then(|| ev.value["id"].as_str().map(String::from)).flatten()
}

/// Append RECORD to `$COORD/nets/log.jsonl` under the next id (`N1`, ...).
fn log(coord: &str, mut record: NetRun) -> NetRun {
    let path = format!("{coord}/nets/log.jsonl");
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = LOCK.lock().unwrap();
    record.id = format!("N{}", crate::jsonl::lines(&path) + 1);
    if let Err(e) = crate::jsonl::append(&path, &serde_json::to_string(&record).unwrap()) {
        eprintln!("coordd: {path}: {e}");
    }
    record
}

/// The nudges in a row of each agent, from the log (so a restart keeps the cap's count).
pub fn nudge_counts(coord: &str) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    let text = std::fs::read_to_string(format!("{coord}/nets/log.jsonl")).unwrap_or_default();
    for r in text.lines().filter_map(|l| serde_json::from_str::<NetRun>(l).ok()) {
        if r.event != "turn_end" {
            continue;
        }
        let Some(a) = r.agent else { continue };
        if r.effects.iter().any(|x| x.effect == "nudge") {
            *out.entry(a).or_default() += 1;
        } else if r.fault.is_none() {
            out.insert(a, 0);
        }
    }
    out
}

/// The runs of the log in COORD (the latest N), for a snapshot after a restart.
pub fn recent(coord: &str, n: usize) -> Vec<NetRun> {
    let text = std::fs::read_to_string(format!("{coord}/nets/log.jsonl")).unwrap_or_default();
    let all: Vec<NetRun> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let marks = marks(coord);
    all[all.len().saturating_sub(n)..].iter().cloned().map(|r| with_marks(r, &marks)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An edit is saved at project scope only when the nets with it pass; a refused edit names the checker's error.
    #[test]
    fn a_saved_edit_must_pass_the_check() {
        let tmp = std::env::temp_dir().join(format!("coordd-save-{}", std::process::id()));
        let base = format!("{}/../../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let places = Places { base: base.clone(), project: Some(tmp.display().to_string()), coord: tmp.join("coord").display().to_string(), ..Default::default() };
        let v = view(&places);
        let clerk = v.sources.iter().find(|s| s.kind == "net" && s.path.ends_with("clerk.net")).unwrap();
        assert_eq!(clerk.target, format!("{}/nets/clerk.net", tmp.display()));
        let turn_end = v.nets.iter().find(|n| n.name == "turn_end").unwrap();
        assert_eq!(turn_end.calls, vec!["turn"]);
        assert_eq!(turn_end.bound, vec!["global"]);
        let broken = coord_types::NetEdit { target: clerk.target.clone(), text: clerk.text.replace("accept(wake: none)                                    // the same turn end", "// the same turn end") };
        let e = save(&places, &broken).unwrap_err();
        assert!(e.contains("clerk.net"), "{e}");
        assert!(!std::path::Path::new(&clerk.target).exists());
        let fine = coord_types::NetEdit { target: clerk.target.clone(), text: clerk.text.replace("Go on with your work.", "Carry on.") };
        assert!(save(&places, &fine).unwrap().starts_with("saved"));
        assert!(std::fs::read_to_string(&clerk.target).unwrap().contains("Carry on."));
        let outside = coord_types::NetEdit { target: "/tmp/x.net".into(), text: String::new() };
        assert!(save(&places, &outside).is_err());
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// The clerk end to end against the live proxy (`cargo test -p coordd -- --ignored live_clerk --nocapture`; needs
    /// COORD for the proxy key): a turn end that names its next step, and the owner's "1 yes 2 no" to two open
    /// questions, each run on agent-base's nets, calls and policy.
    #[test]
    #[ignore]
    fn live_clerk_runs_both_nets() {
        let tmp = std::env::temp_dir().join(format!("coordd-clerk-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let key = std::fs::read_to_string(format!("{}/.proxy-key", std::env::var("COORD").unwrap())).unwrap();
        std::fs::write(tmp.join(".proxy-key"), key).unwrap();
        let coord = tmp.display().to_string();
        let base = format!("{}/../../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let places = Places { base, coord: coord.clone(), ..Default::default() };
        let engine = Engine::start(places.clone());
        let loaded = load(&places, &engine);
        assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
        let sessions = run::Sessions::new(format!("{coord}/nets/sessions"));
        let q = |id: &str, from: &str, text: &str| json!({"id": id, "from": from, "text": text, "thread": id});
        let mut world = run::World::default();
        world.questions.insert("orch".into(), vec![q("q12", "orch", "Should norm priorities follow the preference scopes?"), q("q13", "orch", "Write our own Rust translator, or wait for Chris's?")]);
        world.questions.insert("CONE".into(), vec![q("q20", "CONE", "Merge core/l1 into dev now, or wait for the proof check?")]);
        let text = "Landed L3c part 1 on landed (a3f9e1).\n\n[ran: coord ask \"Merge core/l1 into dev now, or wait for the proof check?\"]\n\nI asked q20 whether to merge core/l1 now. Separately: should the nudge cap be 3 or 5 turns? Next I'll queue the --quick check for part 2.";
        let turn = json!({"agent": "CONE", "turn": 7, "text": text});
        let out = run::run(&engine, &sessions, &world, loaded.resolve("turn_end", Some("CONE")).unwrap(), Some("CONE".into()), turn);
        eprintln!("turn_end: {out:#?}");
        assert!(out.fault.is_none(), "{:?}", out.fault);
        assert_eq!(out.effects.iter().filter(|e| e.effect == "nudge" || e.effect == "accept").count(), 1);
        let asks: Vec<&str> = out.effects.iter().filter(|e| e.effect == "ask").filter_map(|e| e.args["text"].as_str()).collect();
        assert!(asks.len() == 1 && asks[0].contains("nudge cap"), "{asks:?}");
        // the same turn tried with an edited nudge text: dry, in fresh sessions, the edit in its nudge
        let net = std::fs::read_to_string(format!("{base}/nets/clerk.net", base = places.base)).unwrap();
        let edit = coord_types::NetEdit { target: format!("{}/nets/clerk.net", places.base), text: net.replace("do that next.", "do that now.") };
        let tried = try_run(&places, &[edit], "turn_end", Some("CONE".into()), json!({"agent": "CONE", "turn": 7, "text": text}), world.clone()).unwrap();
        eprintln!("tried: {:?}", tried.effects);
        assert!(tried.effects.iter().all(|e| !e.live));
        let session: crate::policy::Session = serde_json::from_str(&std::fs::read_to_string(format!("{coord}/nets/sessions/clerk@CONE.json")).unwrap()).unwrap();
        eprintln!("session: {session:?}");
        assert!(session.harness.is_some() && session.id.is_some(), "{session:?}");
        let msg = json!({"id": "m900", "from": "owner", "text": "1 yes 2 no, do it ourselfs", "thread": null, "to": ["orch"]});
        let out = run::run(&engine, &sessions, &world, loaded.resolve("owner_message", Some("orch")).unwrap(), Some("orch".into()), msg);
        eprintln!("owner_message: {out:#?}");
        assert!(out.fault.is_none(), "{:?}", out.fault);
        assert!(out.effects.iter().any(|e| e.effect == "deliver" || e.effect == "reject"));
        std::fs::remove_dir_all(&tmp).ok();
    }
}
