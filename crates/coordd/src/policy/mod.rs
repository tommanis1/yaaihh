//! The policy engine (vault designs/policy-language-draft-2026-11-08.md, "eFLINT/A"; first slice): decisions,
//! preferences and typed LLM calls, compiled to answer set programs and solved by clingo, one engine per coordd.
//!
//! - [`syntax`]: the language (Fact, Decision, Prefer, Only, Judge); [`compile`]: to clingo (§4, §5, §14);
//!   [`solve`]: one clingo control on a thread of its own, decisions with their provenance (§8);
//!   [`facts`]: what the system supplies (the models the places serve, their families and prices); [`call`]: LLM calls
//!   as typed programs (§7).
//! - Files: the system's prelude and global policy in `AGENT_BASE/policy/*.eflint` (the prelude declares the facts
//!   and the built-in decisions, `model-for`), its calls in `AGENT_BASE/calls/*.call`; a project's in
//!   `PROJECT/policy/*.eflint` (scope project), `PROJECT/policy/agents/NAME.eflint` (scope agent NAME) and
//!   `PROJECT/calls/*.call` (a call or type of the same name replaces the system's), PROJECT being the folder of the
//!   project file.
//! - Every decided call gives its [`coord_types::Decision`] back to the caller, which records it on the ticket the
//!   call served as a `decided` line, `model-for` (S6: the decisions log is no longer written).
//!
//! eFLINT's norms (Act, Duty) are not compiled yet: the parser refuses them by line, so a policy that has them does
//! not load rather than load without them.

pub mod call;
pub mod compile;
pub mod facts;
pub mod interp;
pub mod norms;
pub mod parts;
pub mod pick;
pub mod solve;
pub mod syntax;

use crate::accounts::router::Router;
use coord_types::{Attempt, Decision, Given};
use compile::Scope;
use serde_json::Value;
use solve::{Instance, Solver};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

/// How long the model facts are used before they are read from the places again (`Router::served`).
const FACTS_FOR: Duration = Duration::from_secs(600);

/// The file in $COORD naming what answers a decision (`model-for`, `fills`, ...) and who holds a role
/// ([`Decider`]): `solver`, `both` or `picker`. None, or any other word, is `both`, as coordd is released.
pub const DECIDER_FILE: &str = "decider";

/// The decision a call coordd runs itself goes to when `model-for`'s every option was refused for a limit (fallback
/// mode, [`Engine::call_prompt`]): asked with the same fields; the prelude declares it, decisions.eflint gives its
/// option per task.
pub const FALLBACK: &str = "fallback-model";

/// The decision a call taken by [`chain_for`] is recorded as, when a model of the chain answered.
pub const CHAIN_FOR: &str = "chain-for";

/// What answers a decision (paper-research N-568 p14): clingo's solver, as before the picker; `both`, the solver
/// answering and the picker answering the same question beside it, each difference one journal line
/// ([`pick::beside`]); or the picker alone. Read again at each decision, so writing the file takes effect at the
/// next one; the way back is `solver`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decider {
    Solver,
    Both,
    Picker,
}

/// Where the policy's, the calls' and the nets' files are.
#[derive(Clone, Debug, Default)]
pub struct Places {
    /// AGENT_BASE: `policy/` and `calls/` of the system.
    pub base: String,
    /// The project file's folder, if the project has one.
    pub project: Option<String>,
    /// $COORD: the proxy key, the decision log, the nets' log and sessions.
    pub coord: String,
    /// The renderings an agent's deliveries are drawn from (`coord add NAME KIND MODEL --renderings SET`): the folder
    /// SET of the project's `renderings/`, else of the system's, and nothing else; none, the project's own and then
    /// the system's.
    pub renderings: Option<String>,
    /// Texts read in place of the files at these paths (or as files there, when none is): an edit being tried on the
    /// Clerk page. Empty for the real thing.
    pub overlay: Overlay,
}

/// Edited texts by path (`Places::overlay`).
pub type Overlay = BTreeMap<String, String>;

/// The files of DIR ending in `.EXT`, with OVERLAY's texts in place of theirs and its paths in DIR added, by name.
/// Clingo's grounder shares state across the process (its string table), so two solves in two threads at once can
/// crash it: every use of clingo holds this lock. It may be taken again on the thread that holds it.
static CLINGO: std::sync::Mutex<()> = std::sync::Mutex::new(());

thread_local! {
    static HOLDS_CLINGO: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Held while clingo is used (see [`clingo`]).
pub struct ClingoGuard(Option<std::sync::MutexGuard<'static, ()>>);

impl Drop for ClingoGuard {
    fn drop(&mut self) {
        if self.0.is_some() {
            HOLDS_CLINGO.with(|h| h.set(false));
        }
    }
}

/// Take the process's clingo lock, or nothing if this thread holds it already.
pub fn clingo() -> ClingoGuard {
    if HOLDS_CLINGO.with(|h| h.get()) {
        return ClingoGuard(None);
    }
    let g = CLINGO.lock().unwrap_or_else(|p| p.into_inner());
    HOLDS_CLINGO.with(|h| h.set(true));
    ClingoGuard(Some(g))
}

pub fn files_in(dir: &str, ext: &str, overlay: &Overlay) -> Result<Vec<(String, String)>, String> {
    paths_in(dir, ext, overlay).into_iter().map(|p| Ok((p.clone(), text_at(&p, overlay)?))).collect()
}

/// The paths `files_in` reads, by name: DIR's files ending in `.EXT`, and OVERLAY's paths in DIR.
pub fn paths_in(dir: &str, ext: &str, overlay: &Overlay) -> BTreeSet<String> {
    let mut paths: BTreeSet<String> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == ext)).map(|p| p.display().to_string()).collect())
        .unwrap_or_default();
    paths.extend(overlay.keys().filter(|p| std::path::Path::new(p.as_str()).parent().is_some_and(|d| d == std::path::Path::new(dir)) && p.ends_with(&format!(".{ext}"))).cloned());
    paths
}

/// The text of the file at PATH, OVERLAY's when it has one.
pub fn text_at(path: &str, overlay: &Overlay) -> Result<String, String> {
    match overlay.get(path) {
        Some(t) => Ok(t.clone()),
        None => std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}")),
    }
}

/// Whether decision D found no model to run its call on: none chosen and none tried (on a new project with no route,
/// say, the engine's own calls: `Core::own_call_failed`).
pub fn no_model(d: &Decision) -> bool {
    d.chosen.is_none() && d.attempts.is_empty()
}

/// Why no model answered the call D decided, in his words. This is the one copy (paper-research N-1446 p203): route's
/// failed answer, the cleaning's line (clean.rs `clean_one`), the intake's, asks' and minder's notices (clerk.rs, shape.rs;
/// N-1446 p1813) and the window read it. A model the gateway turned away
/// at its limit (status 429, as the call writes it: `Error("429 …")`; tonight's: `{"code":"model_cooldown",
/// "last_upstream_error":"usage_limit_reached: …","reset_seconds":683,…}`; a refusal that only quotes a number with
/// 429 in it, `N-1429`, is not one) is named by its family, with the time the gateway tries it again: D's `at` plus
/// the 429's own `reset_seconds`, rounded up to the next minute, on his clock (`core::his_clock`): the machine's zone,
/// as the window writes every time (yaaihh when.rs), with no "Z". That is his clock only while coordd and his window
/// run on one machine (proto-lead N-1446 p240); the record's `at` stays UTC (paper-research N-1446 p231). A 429 that
/// gives no `reset_seconds` gets no time. A model every place that serves it refused at its limit (the router's
/// `Failure::Out`: his accounts out, the gateway not serving it) is named so, with the first account's return, rounded
/// up as well. Otherwise it is D's fault, then the last refusal's words, as the cleaning said before.
pub fn unanswered_words(d: &Decision) -> String {
    unanswered_words_in(d, &chrono::Local)
}

/// [`unanswered_words`] with its time in ZONE (a test pins one, so it does not hang on the zone of the machine it runs
/// on).
pub fn unanswered_words_in<Z: chrono::TimeZone>(d: &Decision, zone: &Z) -> String
where
    Z::Offset: std::fmt::Display,
{
    let limited = d.attempts.iter().rev().find_map(|a| a.failure.as_deref().filter(|f| f.starts_with(r#"Error("429 "#) || f.starts_with("Out(")).map(|f| (a, f)));
    if let Some((a, f)) = limited {
        let family = if a.model.starts_with("gpt-") || a.model.contains("codex") {
            "Codex"
        } else if a.model.starts_with("claude-") {
            "Claude"
        } else {
            a.model.as_str()
        };
        // rounded up to the next minute, so he never tries again before the gateway does (proto-lead, N-1446)
        let up = |t: chrono::DateTime<chrono::Utc>| crate::core::his_clock(t + chrono::Duration::seconds((60 - t.timestamp().rem_euclid(60)) % 60), zone);
        // every place that serves it out (the router's `Failure::Out`): the first of his accounts back, if one said when
        if let Some(until) = f.strip_prefix("Out(").and_then(|n| n.strip_suffix(')')?.parse::<i64>().ok()) {
            return match chrono::DateTime::from_timestamp(until, 0).filter(|_| until != crate::accounts::limits::UNKNOWN_RESET) {
                Some(t) => format!("{family} is at its limit on every account of yours that runs it ({}); the first is back at {}", a.model, up(t)),
                None => format!("{family} is at its limit on every account of yours that runs it ({})", a.model),
            };
        }
        let limit = if f.contains("usage_limit") { "at its usage limit" } else { "turning calls away" };
        let again = reset_seconds(f).and_then(|s| Some(up(chrono::DateTime::parse_from_rfc3339(&d.at).ok()?.to_utc() + chrono::Duration::seconds(s))));
        return match again {
            Some(t) => format!("{family} is {limit} ({}, 429); the gateway tries it again at {t}", a.model),
            None => format!("{family} is {limit} ({}, 429)", a.model),
        };
    }
    let why = d.fault.clone().unwrap_or_else(|| "no model answered".into());
    match d.attempts.last().and_then(|a| a.failure.as_deref()).map(crate::clean::failure_words) {
        Some(last) if !last.is_empty() => format!("{why}; {last}"),
        _ => why,
    }
}

/// The `reset_seconds` of a 429's body as the gateway gives it, written into the failure F (quoted or escaped).
fn reset_seconds(f: &str) -> Option<i64> {
    let rest = &f[f.find("reset_seconds")? + "reset_seconds".len()..];
    let n: String = rest.trim_start_matches(['\\', '"', ':', ' ']).chars().take_while(char::is_ascii_digit).collect();
    n.parse().ok()
}

impl Places {
    /// The coord's [`Decider`] ([`DECIDER_FILE`]); another word than the three is journalled and taken as `both`.
    pub fn decider(&self) -> Decider {
        match std::fs::read_to_string(format!("{}/{DECIDER_FILE}", self.coord)) {
            Err(_) => Decider::Both,
            Ok(t) => match t.trim() {
                "solver" => Decider::Solver,
                "picker" => Decider::Picker,
                "both" | "" => Decider::Both,
                other => {
                    eprintln!("coordd: decider: {}/{DECIDER_FILE} says {other:?}, not solver, both or picker: both", self.coord);
                    Decider::Both
                }
            },
        }
    }

    /// The policy files by scope, in order: the system's (prelude first), the project's, each agent's.
    pub(crate) fn sources(&self) -> Result<Vec<(Scope, Vec<syntax::Decl>)>, String> {
        let read = |dir: &str| -> Result<Vec<(String, String)>, String> {
            let Ok(rd) = std::fs::read_dir(dir) else { return Ok(Vec::new()) };
            // duties.eflint is the duty engine's (ticket/violated.rs: eflint-rs reads it, Duty and #include with it)
            let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "eflint") && !crate::ticket::is_duties_file(p)).collect();
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
        // a project's file replaces the base file of the same name (agent-base's notify.eflint is a default a project
        // may say otherwise about; an act cannot be declared twice)
        let project = match &self.project {
            Some(p) => read(&format!("{p}/policy"))?,
            None => Vec::new(),
        };
        let name = |f: &str| std::path::Path::new(f).file_name().map(|s| s.to_os_string());
        let mut global = read(&format!("{}/policy", self.base))?;
        global.retain(|(f, _)| !project.iter().any(|(g, _)| name(g) == name(f)));
        let mut scopes = vec![(Scope::Global, parse_all(global)?)];
        if let Some(p) = &self.project {
            scopes.push((Scope::Project, parse_all(project)?));
            for (f, t) in read(&format!("{p}/policy/agents"))? {
                let name = std::path::Path::new(&f).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                scopes.push((Scope::Agent(name), syntax::parse(&f, &t)?));
            }
        }
        Ok(scopes)
    }

    /// The policy files (system, project, agents) with their modification times: what the compiled norms were made
    /// from, to tell when they must be made again.
    pub fn stamp(&self) -> Vec<(String, Option<std::time::SystemTime>)> {
        // with the files in eFLINT whole (interp.rs) and the project file, whose `norms` says which engine answers
        let mut dirs = vec![format!("{}/policy", self.base), format!("{}/policy/eflint", self.base)];
        if let Some(p) = &self.project {
            dirs.push(format!("{p}/policy"));
            dirs.push(format!("{p}/policy/agents"));
            dirs.push(format!("{p}/policy/eflint"));
        }
        let mut out: Vec<(String, Option<std::time::SystemTime>)> = dirs
            .iter()
            .flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "eflint"))
            .map(|p| (p.display().to_string(), std::fs::metadata(&p).and_then(|m| m.modified()).ok()))
            .collect();
        if let Some(p) = &self.project {
            let f = format!("{p}/project.toml");
            let t = std::fs::metadata(&f).and_then(|m| m.modified()).ok();
            out.push((f, t));
        }
        out.sort();
        out
    }

    pub fn call_dirs(&self) -> Vec<String> {
        // agent-base's, the project's library's (N-101), the project's
        crate::project::folders(&self.coord, &self.base, self.project.as_deref()).iter().map(|d| crate::project::place(d, "calls")).collect()
    }
}

/// The engine thread's state.
struct State {
    places: Places,
    router: Router,
    /// None while the decider is `picker`.
    solver: Option<Result<Solver, String>>,
    /// None while the decider is `solver`; made from the same policy and facts as the solver, when it is made too.
    picker: Option<Result<Arc<pick::Picker>, String>>,
    facts_at: Option<Instant>,
}

impl State {
    /// The solver and the picker the decider MODE needs, with fresh facts: the policy compiled again and the models
    /// read again when stale and no decision is open on the solver (so an edited policy file takes effect within
    /// [`FACTS_FOR`]), or when MODE needs one not made yet.
    fn ready(&mut self, mode: Decider) {
        let stale = self.facts_at.is_none_or(|t| t.elapsed() > FACTS_FOR);
        let idle = self.solver.as_ref().is_none_or(|s| s.as_ref().map_or(true, |s| s.may_refresh()));
        let missing = (mode != Decider::Picker && self.solver.is_none()) || (mode != Decider::Solver && self.picker.is_none());
        if !(idle && (stale || missing)) {
            return;
        }
        let made = self.places.sources().and_then(|s| Ok((compile::compile(&s)?, facts::model_facts(&self.router.served().models()?), s)));
        self.solver = (mode != Decider::Picker).then(|| {
            let (c, f, _) = made.clone()?;
            let mut s = Solver::new(c);
            s.set_facts(f)?;
            Ok(s)
        });
        self.picker = (mode != Decider::Solver).then(|| {
            let (c, f, s) = made.clone()?;
            solve::supplied(&c.facts, &f)?;
            pick::guarded(|| pick::Picker::new(&s, &f)).map(Arc::new)
        });
        self.facts_at = Some(Instant::now());
    }

    fn solver(&mut self) -> Result<&mut Solver, String> {
        self.solver.get_or_insert_with(|| Err("not loaded yet".into())).as_mut().map_err(|e| format!("the policy: {e}"))
    }

    fn picker(&self) -> Result<Arc<pick::Picker>, String> {
        self.picker.clone().unwrap_or_else(|| Err("not loaded yet".into())).map_err(|e| format!("the policy: {e}"))
    }
}

type Job = Box<dyn FnOnce(&mut State) + Send>;

/// The engine: a handle to its thread.
#[derive(Clone)]
pub struct Engine {
    tx: mpsc::Sender<Job>,
    router: Router,
}

/// The effort policy SCOPES give TASK (`+effort-for(TASK, LEVEL).`): the first such instance; none when there is none.
pub(crate) fn effort_for(scopes: &[(Scope, Vec<syntax::Decl>)], task: &str) -> Option<String> {
    scopes.iter().flat_map(|(_, ds)| ds).find_map(|d| match &d.kind {
        syntax::DeclKind::Instance { fact, args } if fact == "effort-for" => match args.as_slice() {
            [syntax::Expr::Str(t), syntax::Expr::Str(level)] if t == task => Some(level.clone()),
            _ => None,
        },
        _ => None,
    })
}

/// The models policy SCOPES give TASK's calls first, in the order written (`+chain-for(TASK, MODEL).`, walk #26):
/// each where the router puts it, an OpenCode Go model as one plain completion (`accounts::plain`), a Claude model on
/// his accounts or the gateway; none, the task goes to `model-for` alone.
pub(crate) fn chain_for(scopes: &[(Scope, Vec<syntax::Decl>)], task: &str) -> Vec<String> {
    scopes.iter().flat_map(|(_, ds)| ds).filter_map(|d| match &d.kind {
        syntax::DeclKind::Instance { fact, args } if fact == CHAIN_FOR => match args.as_slice() {
            [syntax::Expr::Str(t), syntax::Expr::Str(model)] if t == task => Some(model.clone()),
            _ => None,
        },
        _ => None,
    }).collect()
}

/// Whether MODEL of a chain is passed over SPENT into it: a Go model once [`crate::accounts::plain::PLAIN_ALL`] has
/// gone, so slow free models leave the rest of the chain its turn; any other model is always tried.
fn chain_skips(model: &str, spent: Duration) -> bool {
    crate::accounts::plain::go_model(model).is_some() && spent >= crate::accounts::plain::PLAIN_ALL
}

/// The seconds policy SCOPES give an owner's step of CALL before it goes on by itself (`+lapse-after(CALL, SECONDS).`,
/// N-487): the shortest instance; none, the step waits for him as any other.
pub(crate) fn lapse_after(scopes: &[(Scope, Vec<syntax::Decl>)], call: &str) -> Option<u64> {
    call_seconds(scopes, "lapse-after", call)
}

/// The seconds such a step waits at most while he writes in its ticket's box (`+lapse-at-most(CALL, SECONDS).`, E1b):
/// the shortest instance; none, his writing does not hold it.
pub(crate) fn lapse_at_most(scopes: &[(Scope, Vec<syntax::Decl>)], call: &str) -> Option<u64> {
    call_seconds(scopes, "lapse-at-most", call)
}

/// The shortest of SCOPES' instances of FACT(CALL, SECONDS).
fn call_seconds(scopes: &[(Scope, Vec<syntax::Decl>)], name: &str, call: &str) -> Option<u64> {
    scopes
        .iter()
        .flat_map(|(_, ds)| ds)
        .filter_map(|d| match &d.kind {
            syntax::DeclKind::Instance { fact, args } if fact == name => match args.as_slice() {
                [syntax::Expr::Str(c), syntax::Expr::Int(n)] if c == call && *n >= 0 => Some(*n as u64),
                _ => None,
            },
            _ => None,
        })
        .min()
}

/// The seconds policy SCOPES give a key account that is out for its rate limit with no time named before it is taken
/// again (`+key-again-after(SECONDS).`, policy/accounts.eflint; paper-research, N-547 post 16): the shortest
/// instance; none, such an account stays out until the owner clears it.
pub(crate) fn key_again_after(scopes: &[(Scope, Vec<syntax::Decl>)]) -> Option<u64> {
    number_fact(scopes, "key-again-after")
}

/// The seconds before the owner's burst that policy SCOPES let the brain's attention pass read his posts the agent
/// has not answered (`+unanswered-within(SECONDS).`, policy/tickets.eflint; coordd brain.rs): the shortest
/// instance; none, it reads none.
pub(crate) fn unanswered_within(scopes: &[(Scope, Vec<syntax::Decl>)]) -> Option<u64> {
    number_fact(scopes, "unanswered-within")
}

/// The seconds before the owner's burst that policy SCOPES let the brain's attention pass read his earlier posts as an
/// index (`+earlier-within(SECONDS).`, policy/tickets.eflint; coordd brain.rs EARLIER): the shortest instance; none, it
/// reads none.
pub(crate) fn earlier_within(scopes: &[(Scope, Vec<syntax::Decl>)]) -> Option<u64> {
    number_fact(scopes, "earlier-within")
}

/// How many of his earlier posts policy SCOPES let the index list (`+earlier-lines(N).`; coordd brain.rs EARLIER): the
/// smallest instance; none, the code's.
pub(crate) fn earlier_lines(scopes: &[(Scope, Vec<syntax::Decl>)]) -> Option<u64> {
    number_fact(scopes, "earlier-lines")
}

/// The seconds before the newest post of its update that policy SCOPES let the owner model read his posts when it is
/// built whole (`+owner-model-within(SECONDS).`, policy/tickets.eflint; coordd brain.rs `owner_model_env`): the
/// shortest instance; none, coordd opens no update of the owner model.
pub(crate) fn owner_model_within(scopes: &[(Scope, Vec<syntax::Decl>)]) -> Option<u64> {
    number_fact(scopes, "owner-model-within")
}

/// After how many misses since it was built whole policy SCOPES have the owner model built whole again
/// (`+owner-model-rebuild-after(N).`, owner_model.kind's `owner_update`): the smallest instance; none, coordd opens no
/// update of the owner model.
pub(crate) fn owner_model_rebuild_after(scopes: &[(Scope, Vec<syntax::Decl>)]) -> Option<u64> {
    number_fact(scopes, "owner-model-rebuild-after")
}

/// The window opener's rule policy SCOPES give (accounts/opener.rs; policy/accounts.eflint): its four waits
/// (`window-look-after-reset`, `-after-opening`, `-idle`, `-failed`), its message (`window-opening(MODEL, PROMPT)`), and
/// the plans' weekly ceilings placement.eflint states for the placement too (`weekly-ceiling(PLAN, N)`, the lowest of
/// one plan's). None unless every wait and the message are stated: then no window is opened.
pub(crate) fn opener(scopes: &[(Scope, Vec<syntax::Decl>)]) -> Option<crate::accounts::opener::Times> {
    let instances = |fact: &'static str| scopes.iter().flat_map(|(_, ds)| ds).filter_map(move |d| match &d.kind {
        syntax::DeclKind::Instance { fact: f, args } if f == fact => Some(args),
        _ => None,
    });
    let (model, prompt) = instances("window-opening").find_map(|args| match args.as_slice() {
        [syntax::Expr::Str(m), syntax::Expr::Str(p)] => Some((m.clone(), p.clone())),
        _ => None,
    })?;
    let mut ceilings = std::collections::BTreeMap::new();
    for args in instances("weekly-ceiling") {
        if let [syntax::Expr::Str(plan), syntax::Expr::Int(n)] = args.as_slice() {
            if *n >= 0 {
                let c = ceilings.entry(plan.clone()).or_insert(*n as u64);
                *c = (*c).min(*n as u64);
            }
        }
    }
    let wait = |fact| number_fact(scopes, fact).map(|n| n as i64);
    Some(crate::accounts::opener::Times {
        after_reset: wait("window-look-after-reset")?,
        after_opening: wait("window-look-after-opening")?,
        idle: wait("window-look-idle")?,
        failed: wait("window-look-failed")?,
        model,
        prompt,
        ceilings,
    })
}

/// The number policy SCOPES give FACT (`+FACT(N).`, one non-negative number, N-696: a number of ours the policy
/// states): the smallest instance; none when no file says it.
pub(crate) fn number_fact(scopes: &[(Scope, Vec<syntax::Decl>)], fact: &str) -> Option<u64> {
    scopes
        .iter()
        .flat_map(|(_, ds)| ds)
        .filter_map(|d| match &d.kind {
            syntax::DeclKind::Instance { fact: f, args } if f == fact => match args.as_slice() {
                [syntax::Expr::Int(n)] if *n >= 0 => Some(*n as u64),
                _ => None,
            },
            _ => None,
        })
        .min()
}

/// MODEL with EFFORT as its `:EFFORT` (`call::model_effort` reads it); MODEL as it is when there is none.
fn with_effort(model: &str, effort: Option<&str>) -> String {
    match effort {
        Some(e) => format!("{model}:{e}"),
        None => model.to_string(),
    }
}

/// The model the policy gives the calls coordd runs for one task (the Kinds answer's, paper-research N-1396 p235): the
/// option `model-for` takes, with the effort `effort-for` puts on it; or why it gives none.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct TaskModel {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

/// The tasks' models as last solved (`Core::task_models`): by task, what they were solved from
/// (`task_models_key`), and how many solves made them.
#[derive(Debug, Default)]
pub struct TaskModels {
    pub key: String,
    pub by_task: BTreeMap<String, TaskModel>,
    pub solves: u64,
}

/// What a solve of the tasks' models takes, read under coordd's lock (`Core::task_models_ask`): where the policy is,
/// each task the watcher's steps run with the call coordd runs for it, and what they were last solved from.
pub struct TasksAsk {
    pub places: Places,
    pub tasks: BTreeMap<String, String>,
    pub kept: String,
}

/// What the tasks' models come from: the policy's files as they stand (`Places::stamp`), the tasks, the models SERVED.
pub fn task_models_key(places: &Places, tasks: &BTreeMap<String, String>, served: &[(String, String)]) -> String {
    format!("{:?} {tasks:?} {served:?}", places.stamp())
}

/// ASK's tasks' models solved, with what they were solved from, when that is not what they were last solved from;
/// none when it is (consult-3, N-1396 p237: solved again only when a policy or kind file changes). Off the lock,
/// blocking (`main::read_models`).
pub fn solve_task_models(ask: TasksAsk, served: &[(String, String)]) -> Option<(String, BTreeMap<String, TaskModel>)> {
    let key = task_models_key(&ask.places, &ask.tasks, served);
    if key == ask.kept {
        return None;
    }
    let by_task = if ask.tasks.is_empty() { BTreeMap::new() } else { Engine::start(ask.places).task_models(&ask.tasks) };
    Some((key, by_task))
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
        let router = Router::of(&places);
        let (pl, ro) = (places.clone(), router.clone());
        std::thread::Builder::new()
            .name("policy".into())
            .spawn(move || {
                // clingo's control is not Send: it is made and used on this thread only
                let mut st = State { places: pl, router: ro, solver: None, picker: None, facts_at: None };
                for job in rx {
                    job(&mut st);
                }
            })
            .expect("the policy thread starts");
        Engine { tx, router }
    }

    /// Run F on the engine's thread and wait for its result (blocking: from a blocking task).
    fn with<R: Send + 'static>(&self, f: impl FnOnce(&mut State) -> R + Send + 'static) -> Result<R, String> {
        let (rtx, rrx) = mpsc::sync_channel(1);
        self.tx.send(Box::new(move |st| { let _ = rtx.send(f(st)); })).map_err(|_| "the policy thread ended".to_string())?;
        rrx.recv().map_err(|_| "the policy thread ended".to_string())
    }

    /// What `model-for` is asked for call CALL that coordd runs for AGENT as task TASK, and the effort the policy gives
    /// TASK (`effort-for`): the one place both are made, for [`Engine::call_prompt`], which runs on them, and
    /// [`Engine::task_models`], which shows them (consult-3 N-1396 p269). Blocking.
    fn call_ask(&self, task: &str, call: &str, agent: &str) -> Result<(BTreeMap<String, String>, Option<String>), String> {
        // `place` is `call`: an LLM call, not an agent's model (`agent`, models.rs), so the clerk's rules hold here only
        let fields = BTreeMap::from([("task".to_string(), task.to_string()), ("agent".to_string(), agent.to_string()), ("call".to_string(), call.to_string()), ("place".to_string(), "call".to_string())]);
        // the effort policy gives the task (`effort-for`), on the model model-for chooses: `luna` becomes `luna:low`
        let t = task.to_string();
        let effort = self.with(move |st| st.places.sources().map(|s| effort_for(&s, &t)).unwrap_or_default())?;
        Ok((fields, effort))
    }

    /// Run PROMPT, made by code (not a `.call` file), for AGENT as task TASK: the model decided by `model-for` as for
    /// [`Engine::call`], the answer held to SCHEMA and CHECK, at the place the router gives that model
    /// ([`Router::call`]). Blocking.
    ///
    /// Fallback mode (the owner, 20 Nov 21:34, "should fallbakc to eg sonnet 5.5 becuase morel imits are in sight. will
    /// jsut be fallabck mode a bit"; paper-research N-1397 p117): when `model-for`'s every option was refused for a
    /// limit ([`call::Failure::at_limit`]), the same call goes to the option of [`FALLBACK`], which the policy gives
    /// per task (decisions.eflint); for a task it gives none, the call fails as before. That model goes out with no
    /// effort, since `effort-for` is the effort of `model-for`'s model. Every call asks `model-for` first, so the first
    /// call its model answers after the limit is back on it, with no clock. The decision says it went by the fallback,
    /// and why ([`Decision::fallback`]).
    ///
    /// A task the policy gives a [`chain_for`] chain goes to its models first, in order (a Go model passed over by
    /// [`chain_skips`]); when none answers, the call goes on as above, and the failed attempts come first in its
    /// decision.
    pub fn call_prompt(&self, task: &str, name: &str, prompt: &str, schema: &Value, agent: &str, check: &dyn Fn(&Value) -> Result<(), String>) -> Result<Called, String> {
        let t = task.to_string();
        let chain = self.with(move |st| st.places.sources().map(|s| chain_for(&s, &t)).unwrap_or_default())?;
        let (start, mut tried) = (Instant::now(), Vec::new());
        for model in chain {
            if chain_skips(&model, start.elapsed()) {
                continue;
            }
            let (ran, account) = self.router.call(name, prompt, schema, &model, check);
            tried.push(Attempt { model: model.clone(), failure: ran.as_ref().err().map(|f| format!("{f:?}")), account });
            if let Ok(a) = ran {
                let (fields, _) = self.call_ask(task, name, agent)?;
                let decision = Decision { at: crate::core::now(), decision: CHAIN_FOR.into(), call: fields, chosen: Some(model), attempts: tried, ..Default::default() };
                return Ok(Called { answer: Some(a), decision });
            }
        }
        let mut c = self.model_for_prompt(task, name, prompt, schema, agent, check)?;
        c.decision.attempts.splice(0..0, tried);
        Ok(c)
    }

    /// [`Engine::call_prompt`] by `model-for`, and by [`FALLBACK`] when every option was refused for a limit.
    fn model_for_prompt(&self, task: &str, name: &str, prompt: &str, schema: &Value, agent: &str, check: &dyn Fn(&Value) -> Result<(), String>) -> Result<Called, String> {
        let (fields, effort) = self.call_ask(task, name, agent)?;
        let run = |model: &str, effort: Option<&str>| self.router.call(name, prompt, schema, &with_effort(model, effort), check);
        let mut limits = 0;
        let (answer, first) = self.decided("model-for", fields.clone(), Vec::new(), Vec::new(), |model: &str| {
            let ran = run(model, effort.as_deref());
            limits += usize::from(ran.0.as_ref().is_err_and(call::Failure::at_limit));
            ran
        })?;
        if answer.is_some() || first.attempts.is_empty() || limits < first.attempts.len() {
            return Ok(Called { answer, decision: first });
        }
        let (answer, then) = self.decided(FALLBACK, fields, Vec::new(), Vec::new(), |model: &str| run(model, None))?;
        // no option for this task, or no `fallback-model` in the policy: the call fails as it did
        if then.attempts.is_empty() {
            return Ok(Called { answer: None, decision: first });
        }
        let refused: Vec<&str> = first.attempts.iter().map(|a| a.model.as_str()).collect();
        let fallback = format!("fallback mode: every option model-for gave was refused for a limit (429 at the gateway: {})", refused.join(", "));
        let attempts = first.attempts.iter().chain(&then.attempts).cloned().collect();
        Ok(Called { answer, decision: Decision { attempts, fallback: Some(fallback), ..then } })
    }

    /// The decision KIND for FIELDS solved, its option taken as the answer (no call runs on it): the record, `chosen`
    /// the answer; `fault` when the policy has no such decision or no option is left. Blocking.
    pub fn decide(&self, kind: &str, fields: BTreeMap<String, String>) -> Result<Decision, String> {
        self.decided::<()>(kind, fields, Vec::new(), Vec::new(), |_| (Ok(()), None)).map(|(_, d)| d)
    }

    /// For each of TASKS (task -> the call coordd runs for it), the model `model-for` takes as clerk.rs asks it when a
    /// watcher's step of it comes, with its task's effort: [`Engine::call_ask`] for the watcher, decided as
    /// [`Engine::decide`] decides, nothing recorded. Blocking.
    pub fn task_models(&self, tasks: &BTreeMap<String, String>) -> BTreeMap<String, TaskModel> {
        tasks
            .iter()
            .map(|(task, call)| {
                let m = match self.call_ask(task, call, docket_machine::WATCHER).and_then(|(fields, effort)| Ok((self.decide("model-for", fields)?, effort))) {
                    Ok((d, effort)) => match d.chosen {
                        Some(model) => TaskModel { model: Some(model), effort, why: None },
                        None => TaskModel { why: Some(d.fault.unwrap_or_else(|| "no option left".into())), ..Default::default() },
                    },
                    Err(e) => TaskModel { why: Some(e), ..Default::default() },
                };
                (task.clone(), m)
            })
            .collect()
    }

    /// The model of an agent made for a step or by `coord new` (models.rs): `model-for` decided with FIELDS (`place`
    /// is `agent`), the models UNMET left out as not meeting the call's `where` (FIELDS' `where`). Blocking.
    pub fn decide_model(&self, fields: BTreeMap<String, String>, unmet: Vec<String>) -> Result<Decision, String> {
        self.decided::<()>("model-for", fields, Vec::new(), unmet, |_| (Ok(()), None)).map(|(_, d)| d)
    }

    /// The models the places serve, each with its family (what `model-for` chooses among, [`Router::served`]).
    /// Blocking.
    pub fn served(&self) -> Result<Vec<(String, String)>, String> {
        Ok(self.router.served().models()?.into_iter().map(|(id, provider)| (id, facts::family(&provider).to_string())).collect())
    }

    /// The decision KIND for FIELDS solved and RUN on its option, a failed option excluded and the decision solved
    /// again until one runs or none is left; recorded either way, each attempt with the account RUN says it ran on. The coord's [`Decider`] says what answers: in
    /// `both` the solver's answer is the one taken and the picker's is set beside it, on this thread, off the clingo
    /// lock and the policy thread ([`pick::beside`]).
    fn decided<T>(&self, kind: &str, fields: BTreeMap<String, String>, given: Vec<Given>, unmet: Vec<String>, mut run: impl FnMut(&str) -> (Result<T, call::Failure>, Option<String>)) -> Result<(Option<T>, Decision), String> {
        let inst = Instance { kind: kind.into(), call: fields, given, unmet };
        let mut record = Decision { id: String::new(), at: crate::core::now(), decision: inst.kind.clone(), call: inst.call.clone(), given: inst.given.clone(), ..Default::default() };
        let i = inst.clone();
        let (mode, opened, picker) = self.with(move |st| {
            let mode = st.places.decider();
            st.ready(mode);
            let opened = if mode == Decider::Picker { Ok(None) } else { st.solver().and_then(|s| s.open(&i)).map(Some) };
            (mode, opened, (mode != Decider::Solver).then(|| st.picker()))
        })?;
        let picker = picker.unwrap_or_else(|| Err("the decider is solver".into()));
        let d = match (opened, &picker) {
            (Ok(d), _) if mode != Decider::Picker => d,
            (Ok(_), Ok(_)) => None,
            (opened, picked) => {
                let e = opened.err().or_else(|| picked.as_ref().err().cloned()).unwrap_or_default();
                if mode == Decider::Both {
                    if let Some(line) = pick::beside(picker.as_deref().map_err(String::as_str), &inst, &[], Err(&e)) {
                        eprintln!("{line}");
                    }
                }
                record.fault = Some(e);
                return Ok((None, record));
            }
        };
        let mut failed: Vec<String> = Vec::new();
        let mut out = None;
        loop {
            let o = match &d {
                Some(d) => {
                    let (dd, i) = (d.clone(), inst.clone());
                    let o = self.with(move |st| st.solver().and_then(|s| s.solve(&dd, &i)))??;
                    if mode == Decider::Both {
                        if let Some(line) = pick::beside(picker.as_deref().map_err(String::as_str), &inst, &failed, Ok(&o)) {
                            eprintln!("{line}");
                        }
                    }
                    o
                }
                None => match picker.as_deref().map_err(String::clone).and_then(|p| pick::guarded(|| p.pick(&inst, &failed))) {
                    Ok((o, _)) => o,
                    Err(e) => {
                        record.fault = Some(e);
                        break;
                    }
                },
            };
            (record.optimal, record.excluded, record.costs, record.runner_up, record.decided_by) = (o.optimal, o.excluded, o.costs, o.runner_up, o.decided_by);
            let Some(option) = o.chosen else {
                record.fault = o.fault.or(Some("no option left".into()));
                break;
            };
            let (ran, account) = run(&option);
            match ran {
                Ok(a) => {
                    record.attempts.push(Attempt { model: option.clone(), failure: None, account });
                    record.chosen = Some(option);
                    out = Some(a);
                    break;
                }
                Err(f) => {
                    record.attempts.push(Attempt { model: option.clone(), failure: Some(format!("{f:?}")), account });
                    failed.push(option.clone());
                    if let Some(d) = &d {
                        let (dd, m) = (d.clone(), option.clone());
                        self.with(move |st| st.solver().and_then(|s| s.fail(&dd, &m)))??;
                    }
                }
            }
        }
        if let Some(dd) = d {
            self.with(move |st| st.solver().and_then(|s| s.close(&dd)))??;
        }
        Ok((out, record))
    }
}

#[cfg(test)]
mod effort_tests {
    use super::*;

    /// paper-research m1405: a policy's `+effort-for(TASK, LEVEL).` puts the effort on TASK's request and on no other
    /// task's; a task with none goes out exactly as before (no suffix, no `reasoning_effort`). The system policy gives
    /// none since its one, the watcher's `asks`, went (proto-lead N-1479 p247): here a project's line gives intake's.
    #[test]
    fn the_effort_policy_gives_a_task_is_on_its_request_and_no_other() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let system = Places { base: base.clone(), ..Default::default() }.sources().unwrap();
        for task in ["asks", "intake", "route", "clerk", "form", docket_machine::SHAPE] {
            assert_eq!(effort_for(&system, task), None, "the system's policy gives {task} none");
        }
        let project = std::env::temp_dir().join(format!("coordd-effort-{}", std::process::id()));
        std::fs::create_dir_all(project.join("policy")).unwrap();
        std::fs::write(project.join("policy/effort.eflint"), "+effort-for(\"intake\", \"low\").\n").unwrap();
        let scopes = Places { base, project: Some(project.display().to_string()), ..Default::default() }.sources();
        let _ = std::fs::remove_dir_all(&project);
        let scopes = scopes.unwrap();
        assert_eq!(effort_for(&scopes, "intake").as_deref(), Some("low"));
        for task in ["asks", "route", "clerk", "form", docket_machine::SHAPE] {
            assert_eq!(effort_for(&scopes, task), None, "{task}");
        }
        // the policy still compiles with the fact type and the instance
        compile::compile(&scopes).unwrap();
        let schema = serde_json::json!({"type": "string"});
        let intake = call::request_body("sort", "p", &schema, &with_effort("gpt-5.6-luna", effort_for(&scopes, "intake").as_deref()));
        assert_eq!((intake["model"].as_str(), intake["reasoning_effort"].as_str()), (Some("gpt-5.6-luna"), Some("low")));
        let route = call::request_body("whom", "p", &schema, &with_effort("gpt-5.6-luna", effort_for(&scopes, "route").as_deref()));
        let keys: Vec<&String> = route.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["max_tokens", "messages", "model", "response_format"]);
        assert_eq!(route["model"], "gpt-5.6-luna");
    }

    /// agent-base's policy gives the brain's turn-end pass (`ahead`) its chain, the free Go models then Haiku then
    /// Sonnet (the owner, Q-1828), and no other task one; the policy compiles with the fact declared in its own file.
    #[test]
    fn the_turn_end_pass_alone_has_a_chain_free_go_models_then_haiku_then_sonnet() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let scopes = Places { base, ..Default::default() }.sources().unwrap();
        let ahead = chain_for(&scopes, "ahead");
        assert_eq!(ahead, ["opencode-go/hungerg-free", "opencode-go/beam-free:low", "claude-haiku-5-5", "claude-sonnet-5-5"]);
        for task in ["attend", "elect", "route", "clerk", "clean", "intake", "split", docket_machine::SHAPE] {
            assert!(chain_for(&scopes, task).is_empty(), "{task}");
        }
        compile::compile(&scopes).unwrap();
    }

    /// Past the plain budget a Go model is passed over and a Claude model is still tried: slow free models never cost
    /// the chain its Claude end.
    #[test]
    fn past_the_plain_budget_only_go_models_are_passed_over() {
        let (soon, late) = (Duration::from_secs(1), crate::accounts::plain::PLAIN_ALL);
        assert!(!chain_skips("opencode-go/beam-free:low", soon));
        assert!(chain_skips("opencode-go/beam-free:low", late));
        assert!(!chain_skips("claude-haiku-5-5", late) && !chain_skips("claude-sonnet-5-5", late + late));
    }
}

#[cfg(test)]
mod unanswered_tests {
    use super::*;

    fn decided(at: &str, attempts: &[(&str, &str)], fault: Option<&str>) -> Decision {
        let attempts = attempts.iter().map(|(m, f)| Attempt { model: m.to_string(), failure: (!f.is_empty()).then(|| f.to_string()), account: None }).collect();
        Decision { at: at.into(), attempts, fault: fault.map(String::from), ..Default::default() }
    }

    /// The words of a failed call, the one copy (paper-research N-1446 p203): tonight's 429 as the decision records it
    /// (coordd.log 27044) gives the gateway's time on his clock (p231), a 429 that gives none gives no time, and
    /// anything else is said as the cleaning said it before.
    #[test]
    fn a_failed_call_is_said_in_his_words_with_the_gateways_time_only_when_it_gives_one() {
        let tonight = r#"Error("429 Too Many Requests: {\"code\":\"model_cooldown\",\"last_upstream_error\":\"usage_limit_reached: The usage limit has been reached\",\"message\":\"All credentials for model gpt-6.2-sol are cooling down (last error: usage_limit_reached: The usage limit has been reached)\",\"model\":\"gpt-6.2-sol\",\"reset_seconds\":683,\"reset_time\":\"11m23s\"}")"#;
        let fault = Some("no option satisfies the hard rules");
        // on his clock: Amsterdam on 20 Nov is +01:00 (CET), pinned so the tower's zone does not matter
        let amsterdam = chrono::FixedOffset::east_opt(3600).unwrap();
        assert_eq!(
            unanswered_words_in(&decided("2026-11-20T20:36:00Z", &[("gpt-6.2-sol", tonight)], fault), &amsterdam),
            "Codex is at its usage limit (gpt-6.2-sol, 429); the gateway tries it again at 21:48",
            "20:47:23Z is said as 21:48, the next minute up, so he never tries before the gateway does"
        );
        let on_the_minute = tonight.replace(r#"\"reset_seconds\":683"#, r#"\"reset_seconds\":660"#);
        assert_ne!(on_the_minute, tonight);
        assert_eq!(
            unanswered_words_in(&decided("2026-11-20T20:36:00Z", &[("gpt-6.2-sol", on_the_minute.as_str())], fault), &amsterdam),
            "Codex is at its usage limit (gpt-6.2-sol, 429); the gateway tries it again at 21:47",
            "20:47:00Z is already a whole minute"
        );
        // and in the machine's own zone, as the window writes its times
        let local = chrono::DateTime::parse_from_rfc3339("2026-11-20T20:48:00Z").unwrap().with_timezone(&chrono::Local).format("%H:%M").to_string();
        assert_eq!(
            unanswered_words(&decided("2026-11-20T20:36:00Z", &[("gpt-6.2-sol", tonight)], fault)),
            format!("Codex is at its usage limit (gpt-6.2-sol, 429); the gateway tries it again at {local}")
        );
        let untimed = r#"Error("429 Too Many Requests: {\"code\":\"usage_limit_reached\",\"message\":\"The usage limit has been reached\"}")"#;
        assert_eq!(unanswered_words(&decided("2026-11-20T21:36:00Z", &[("gpt-6.2-sol", untimed)], fault)), "Codex is at its usage limit (gpt-6.2-sol, 429)", "no time is made up");
        let rate = r#"Error("429 Too Many Requests: {\"message\":\"slow down\"}")"#;
        assert_eq!(unanswered_words(&decided("2026-11-20T21:36:00Z", &[("claude-haiku-5-5", rate)], fault)), "Claude is turning calls away (claude-haiku-5-5, 429)");
        // a check's refusal, as the cleaning's line gave it before
        let refused = r#"Invalid("the call changed a code, id or name: `N-487`")"#;
        assert_eq!(unanswered_words(&decided("x", &[("gpt-6.2-sol", refused)], Some("refused"))), "refused; the call changed a code, id or name: `N-487`");
        // a refusal that quotes a number with 429 in it is not the gateway's 429 (proto-reader N-1446 p328)
        let quotes = r#"Invalid("the call changed a code, id or name: `N-1429`")"#;
        assert_eq!(unanswered_words(&decided("x", &[("gpt-6.2-sol", quotes)], Some("refused"))), "refused; the call changed a code, id or name: `N-1429`");
        assert_eq!(unanswered_words(&decided("x", &[], fault)), "no option satisfies the hard rules");
        assert_eq!(unanswered_words(&decided("x", &[], None)), "no model answered");
    }

    /// Every place that serves the model out (the router's `Failure::Out`, as the record holds it): named so, with the
    /// first of his accounts back on his clock, rounded up as the gateway's time is; with none said, no time. The last
    /// limit in the attempts is the one said: the fallback's, after Codex's 429s.
    #[test]
    fn every_account_that_runs_it_out_is_said_with_the_first_return() {
        let amsterdam = chrono::FixedOffset::east_opt(3600).unwrap();
        let back = chrono::DateTime::parse_from_rfc3339("2026-11-21T09:59:30Z").unwrap().timestamp();
        let tonight = r#"Error("429 Too Many Requests: {\"code\":\"model_cooldown\",\"last_upstream_error\":\"usage_limit_reached\",\"reset_seconds\":683}")"#;
        let out = format!("{:?}", call::Failure::Out(back));
        assert_eq!(out, format!("Out({back})"));
        assert_eq!(
            unanswered_words_in(&decided("2026-11-21T09:00:00Z", &[("gpt-6.2-sol", tonight), ("claude-sonnet-5-5", &out)], None), &amsterdam),
            "Claude is at its limit on every account of yours that runs it (claude-sonnet-5-5); the first is back at 11:00"
        );
        let unknown = format!("{:?}", call::Failure::Out(crate::accounts::limits::UNKNOWN_RESET));
        assert_eq!(unanswered_words(&decided("x", &[("claude-sonnet-5-5", &unknown)], None)), "Claude is at its limit on every account of yours that runs it (claude-sonnet-5-5)");
        assert!(call::Failure::Out(back).at_limit(), "it counts for fallback mode as the gateway's 429 does");
    }

    /// The way an attempt went is on its record (proto-lead, I-1469 p22, 5): an account's id; a record written before
    /// the router, with no such field, reads as the gateway, and the gateway's attempt is written as before.
    #[test]
    fn an_attempt_recorded_before_the_router_reads_as_the_gateway() {
        let old: Attempt = serde_json::from_str(r#"{"model":"gpt-6.2-sol","failure":"Truncated"}"#).unwrap();
        assert_eq!(old.account, None);
        assert_eq!(serde_json::to_value(&old).unwrap(), serde_json::json!({"model": "gpt-6.2-sol", "failure": "Truncated"}));
        let on = Attempt { model: "claude-sonnet-5-5".into(), failure: None, account: Some("b".into()) };
        assert_eq!(serde_json::to_value(&on).unwrap(), serde_json::json!({"model": "claude-sonnet-5-5", "account": "b"}));
        assert_eq!(serde_json::from_value::<Attempt>(serde_json::to_value(&on).unwrap()).unwrap(), on);
    }

    #[test]
    fn his_times_round_up_for_a_retry_and_down_for_a_hold() {
        // proto-lead's rule (N-1446), both pinned to his +01:00 (Amsterdam on 20 Nov) so the tower's zone does not matter
        let amsterdam = chrono::FixedOffset::east_opt(3600).unwrap();
        let at = |s: &str| chrono::DateTime::parse_from_rfc3339(s).unwrap().to_utc();
        // the retry: the gateway's 20:47:23Z is said as the next minute up, so he never tries before it does
        let tonight = r#"Error("429 Too Many Requests: {\"code\":\"model_cooldown\",\"reset_seconds\":683}")"#;
        assert_eq!(
            unanswered_words_in(&decided("2026-11-20T20:36:00Z", &[("gpt-6.2-sol", tonight)], None), &amsterdam),
            "Codex is turning calls away (gpt-6.2-sol, 429); the gateway tries it again at 21:48"
        );
        // the hold: p590 at 18:24:50Z plus 15 s goes on at 18:25:05Z, said as the minute it falls in
        assert_eq!(crate::lapse::goes_through_words(at("2026-11-20T18:25:05Z"), &amsterdam), "goes through at 19:25");
        assert_eq!(crate::lapse::goes_through_words(at("2026-11-20T18:25:59Z"), &amsterdam), "goes through at 19:25");
        assert_eq!(crate::core::his_clock(at("2026-11-20T20:47:23Z"), &amsterdam), "21:47", "his_clock itself never rounds up");
    }
}
