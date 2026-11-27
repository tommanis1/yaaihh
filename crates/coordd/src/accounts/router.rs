//! The router: the one way to a model. The model is the policy's (`model-for`); the router picks only the place
//! that serves it, one of the owner's accounts or the gateway, by the policy's `account-for` (policy/placement.eflint).
//! [`take`] binds an agent's start under the harness's lock, [`Router::call`] runs coordd's own calls and
//! [`Router::served`] lists what the places serve. Nothing else names the gateway.

use super::mover::now_s;
use super::{limits, plain, read_json, signin, write_json, Entry, Harness, Kind, Lock, Store};
use crate::policy::{
    call::{self, Failure},
    compile::Scope,
    facts::{Proxy, CALL_BOUND},
    pick::Picker,
    solve::Instance,
    syntax::Decl,
    Places,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The gateway's name among the places (policy/placement.eflint). No account is named so: an account's id has no
/// space ([`super::Store::bad_id`]).
pub const GATEWAY: &str = "the gateway";

/// Claude Code's program, as the owner's PATH finds it (the sign-in's environment, [`signin::env`]).
pub const CLAUDE: &str = "claude";

/// The file in $COORD that keeps the gateway's list of models as last read ([`Router::served`]): the list a failed
/// read leaves in place, and what [`gateway_serves`] answers from.
const GATEWAY_MODELS: &str = "gateway-models.json";

/// How long a model the gateway's list stops naming still counts as served (seconds): longer than the gateway's
/// cooldown after a transient error, during which its list leaves out a model whose only login is cooling down.
const GONE_AFTER: i64 = 600;

/// The empty folder in $COORD a call on an account is run from ([`Router::call`]): Claude Code finds nothing there.
const CALL_FOLDER: &str = "call-folder";

/// The file in $COORD that holds [`CALL_SYSTEM`], for Claude Code's `--system-prompt-file`.
const CALL_SYSTEM_FILE: &str = "call-system-prompt.txt";

/// The system prompt of a call on an account, in place of Claude Code's own (an agent's, with its tools and its
/// ways): one line, as the call goes to the gateway with none.
const CALL_SYSTEM: &str = "Answer the request with the structured output it asks for.\n";

/// The gateway's list of models as last read ([`GATEWAY_MODELS`]): when the coord's first read was and when this one
/// (seconds), each model's id and provider (`owned_by`), and each an earlier read named that the reads since have not,
/// with since when (the first read that lacked it), for [`GONE_AFTER`]. A file without the first read's time reads as
/// none, as before any read.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct GatewayModels {
    first: i64,
    at: i64,
    models: Vec<(String, String)>,
    #[serde(default)]
    missing: Vec<(String, String, i64)>,
}

impl GatewayModels {
    /// The list READ at NOW, after KEPT (none: the first read, at NOW): what it names, and what KEPT named or missed
    /// and it does not, missing since the first read that lacked it, until that is [`GONE_AFTER`] ago.
    fn after(kept: Option<GatewayModels>, read: Vec<(String, String)>, now: i64) -> GatewayModels {
        let first = kept.as_ref().map_or(now, |k| k.first);
        let before = kept.into_iter().flat_map(|k| k.missing.into_iter().chain(k.models.into_iter().map(|(id, p)| (id, p, now))));
        let missing = before.filter(|(id, _, since)| now - since < GONE_AFTER && !read.iter().any(|(m, _)| m == id)).collect();
        GatewayModels { first, at: now, models: read, missing }
    }

    /// The ids the gateway serves by these reads ([`GatewayModels::listed`]), or None (every model served) until
    /// [`GONE_AFTER`] has passed since the coord's first read.
    fn serving(&self) -> Option<Vec<String>> {
        (self.at - self.first >= GONE_AFTER).then(|| self.listed().into_iter().map(|(id, _)| id).collect())
    }

    /// What the gateway serves by these reads: what the last one named, and what it stopped naming less than
    /// [`GONE_AFTER`] before.
    fn listed(&self) -> Vec<(String, String)> {
        self.models.iter().cloned().chain(self.missing.iter().map(|(id, p, _)| (id.clone(), p.clone()))).collect()
    }
}

/// The 200s coordd's own calls got ([`Router::call`]), by coord, then by place and model: when the last answered
/// call was sent (seconds). An agent held on a rate limit for that model at that place goes on then
/// (`Core::serves_again`). Calls made only to probe a place, and the gateway's list, are not noted.
static SERVED: std::sync::Mutex<BTreeMap<String, BTreeMap<(String, String), i64>>> = std::sync::Mutex::new(BTreeMap::new());

/// MODEL as the place that serves it names it: its [`base_id`] without its `:EFFORT` (an agent's model line,
/// `claude-opus-5-5:low`), as an answer's own `model` names it.
pub fn served_model(model: &str) -> &str {
    base_id(call::model_effort(model).0)
}

/// PLACE answered COORD's call of MODEL, sent at SENT ([`SERVED`]).
pub(crate) fn note_served(coord: &str, place: &str, model: &str, sent: i64) {
    let mut served = SERVED.lock().unwrap_or_else(|e| e.into_inner());
    let at = served.entry(coord.to_string()).or_default().entry((place.to_string(), served_model(model).to_string())).or_insert(sent);
    *at = (*at).max(sent);
}

/// The 200s COORD's own calls got since this was last asked ([`SERVED`]), taken: each place and model, with when the
/// last call it answered was sent.
pub fn served_taken(coord: &str) -> BTreeMap<(String, String), i64> {
    SERVED.lock().unwrap_or_else(|e| e.into_inner()).remove(coord).unwrap_or_default()
}

/// The pairs of models (asked, named) where a 200 of the gateway named another model than the one asked, each said
/// once in coordd's log while it runs ([`answered_as`]).
static ANSWERED_AS: std::sync::Mutex<std::collections::BTreeSet<(String, String)>> = std::sync::Mutex::new(std::collections::BTreeSet::new());

/// The log line, once per pair while coordd runs, for a call of ASKED that the gateway answered as NAMED, another
/// model; it carries the two names and the time AT only.
fn answered_as(asked: &str, named: &str, at: i64) -> Option<String> {
    let asked = served_model(asked);
    let new = served_model(named) != asked && ANSWERED_AS.lock().unwrap_or_else(|e| e.into_inner()).insert((asked.to_string(), named.to_string()));
    new.then(|| format!("coordd: the gateway answered {asked} with {named} at {}", crate::core::at_secs(at)))
}

/// What the choice came to.
#[derive(Debug, Clone, PartialEq)]
pub enum Choice {
    /// This account (its id).
    Account(String),
    /// The gateway: of the places that can take it, the policy put the gateway first. The agent runs through it,
    /// bound to no account, and is looked at again at BACK ([`look_again`]). WHY: the policy's reason, its words (the
    /// first part of the deciding rule's comment); None when no account could take it, as every signed-in one was out.
    Gateway { back: i64, why: Option<String> },
    /// No place can take it and some account that serves the model is out; the first comes back then (seconds since
    /// the epoch). The agent waits and says so (the agreed list's 7).
    AllOut { until: i64 },
    /// No place can take it and no account that serves the model is out: none is signed in.
    None,
}

/// What a choice is made with besides the accounts' own files: the policy as read (`Places::sources`, its fault if it
/// could not be), whether the gateway is one of the places (the coord has it, and it serves MODEL:
/// [`gateway_serves`]), and the model asked.
pub struct Placing {
    pub policy: Result<Vec<(Scope, Vec<Decl>)>, String>,
    pub gateway: bool,
    /// The model, without its `:EFFORT`: the gateway's id (`claude-sonnet-5-5`), or an opencode agent's
    /// `PROVIDER/MODEL`.
    pub model: String,
}

/// `turn.json`: per tier, the id of the place that was chosen last (the gateway's name for the gateway).
pub type Turns = BTreeMap<i64, String>;

/// H's `turn.json` as on file (none: no turn taken yet).
pub fn turns(store: &Store, h: Harness) -> Turns {
    read_json(&store.harness_dir(h).join("turn.json")).unwrap_or_default()
}

/// MODEL (no `:EFFORT`) without the variant Claude Code adds in brackets: `claude-fable-5-2[1m]` is
/// `claude-fable-5-2`. Models are compared by this id wherever they meet a list.
pub fn base_id(model: &str) -> &str {
    model.split_once('[').map_or(model, |(base, _)| base)
}

/// Whether MODEL is one of Anthropic's own names (`claude-…`), which Claude Code on the owner's accounts runs.
pub fn anthropic(model: &str) -> bool {
    model.starts_with("claude-")
}

/// MODEL's entry in COORD's reads of the gateway's list ([`GATEWAY_MODELS`], what the last read named or stopped
/// naming), compared as [`served_model`]: its id and provider. None for a model no read has named.
fn gateway_listed(coord: &str, model: &str) -> Option<(String, String)> {
    let id = served_model(model);
    read_json::<GatewayModels>(&Path::new(coord).join(GATEWAY_MODELS))?.listed().into_iter().find(|(m, _)| served_model(m) == id)
}

/// The family of MODEL on the gateway (`claude`, `codex`, `gemini`): its provider in the gateway's list, else
/// `claude` for one of Anthropic's names; None for any other model no read has named.
pub fn gateway_family(coord: &str, model: &str) -> Option<String> {
    gateway_listed(coord, model).map(|(_, p)| crate::policy::facts::family(&p).to_string()).or_else(|| anthropic(served_model(model)).then(|| "claude".to_string()))
}

/// Whether the gateway serves MODEL (compared by [`base_id`]) by COORD's reads of its list: only lists that have
/// lacked it for [`GONE_AFTER`] say no; before that, every model.
pub fn gateway_serves(coord: &str, model: &str) -> bool {
    gateway_ids(coord).is_none_or(|ids| ids.iter().any(|id| id == base_id(model)))
}

/// The ids COORD's reads of the gateway's list count ([`GATEWAY_MODELS`], [`GatewayModels::serving`]): None before any
/// read, and until [`GONE_AFTER`] after the first.
fn gateway_ids(coord: &str) -> Option<Vec<String>> {
    read_json::<GatewayModels>(&Path::new(coord).join(GATEWAY_MODELS)).and_then(|g| g.serving())
}

/// Whether the gateway's list names MODEL, compared by [`served_model`], once COORD's reads count
/// ([`gateway_ids`]); before that it names none.
fn gateway_names(coord: &str, model: &str) -> bool {
    gateway_ids(coord).is_some_and(|ids| ids.iter().any(|id| served_model(id) == served_model(model)))
}

/// COORD's reads of the gateway's list ([`GATEWAY_MODELS`]) as if made SECS earlier: a test's way to the time
/// [`GONE_AFTER`] counts.
#[cfg(test)]
pub(crate) fn age_reads(coord: &str, secs: i64) {
    let path = Path::new(coord).join(GATEWAY_MODELS);
    let mut g: GatewayModels = read_json(&path).expect("a read of the gateway's list");
    (g.first, g.at) = (g.first - secs, g.at - secs);
    g.missing.iter_mut().for_each(|(_, _, since)| *since -= secs);
    write_json(&path, &g).unwrap();
}

/// COORD's read of the gateway's list at AT naming LIST (id, provider), after the reads before it
/// ([`GatewayModels::after`]): a test's stand-in for [`Router::served`].
#[cfg(test)]
pub(crate) fn gateway_read(coord: &str, list: &[(&str, &str)], at: i64) {
    let path = Path::new(coord).join(GATEWAY_MODELS);
    let list = list.iter().map(|(id, p)| (id.to_string(), p.to_string())).collect();
    write_json(&path, &GatewayModels::after(read_json(&path), list, at)).unwrap();
}

/// The policy's `claude-code-model` table as PICKER holds it: models by the gateway's id, each with the `--model` a call
/// of it is given on the owner's accounts ([`Router::call`]), and listed as theirs while one is signed in
/// ([`Router::served`]).
fn claude_models(picker: &Picker) -> Vec<(String, String)> {
    picker.holding("claude-code-model").into_iter().filter_map(|r| Some((r.first()?.clone(), r.get(1)?.clone()))).collect()
}

/// When an agent the policy put on the gateway is looked at again (seconds): the first time something of ACCOUNTS
/// that limits it changes after NOW, a signed-in account's being out or its week, which starts again. With none
/// named, [`limits::UNKNOWN_RESET`].
pub fn look_again(accounts: &[Entry], now: i64) -> i64 {
    accounts
        .iter()
        .filter(|e| e.state.signed_in)
        .flat_map(|e| [e.state.out_until, e.state.seven_day.as_ref().map(|w| w.resets_at)])
        .flatten()
        .filter(|t| *t > now)
        .min()
        .unwrap_or(limits::UNKNOWN_RESET)
}

/// The place for MODEL (no `:EFFORT`), for one more agent AGENT or one call, at NOW (seconds): what POLICY's
/// `account-for` chooses among ACCOUNTS and, with GATEWAY, the gateway. Each account is given its tier, whether it
/// can take an agent of MODEL now ([`super::State::can_take_model`]: an agy account may be out for MODEL alone),
/// whether messages are known to reach it and how many agents run on it, its plan and its weekly use while that week
/// lasts, and whether it serves MODEL: a Claude Code account each of Anthropic's own ([`anthropic`]), an opencode
/// account its provider's (only those of the model's provider are given, `take`'s PROVIDER), an agy account the models
/// it offers; the gateway serves it (GATEWAY says so). Each place gets its turn in its tier's round, counted from the
/// one after TURNS' last chosen there, by id, going round. TURNS is brought up to date. With no place to choose, the
/// first account that serves MODEL and is back for it.
pub fn choose(accounts: &[Entry], turns: &mut Turns, policy: &[(Scope, Vec<Decl>)], gateway: bool, agent: &str, model: &str, now: i64) -> Result<Choice, String> {
    let fact = |f: &str, args: Vec<Value>| (f.to_string(), args);
    let mut facts = Vec::new();
    for e in accounts {
        facts.push(fact("place", vec![json!(e.id)]));
        facts.push(fact("place-tier", vec![json!(e.id), json!(e.account.tier)]));
        if e.state.can_take_model(model, now) {
            facts.push(fact("usable", vec![json!(e.id)]));
        }
        if e.state.heard.is_none() {
            facts.push(fact("unheard", vec![json!(e.id)]));
        }
        facts.push(fact("taken", vec![json!(e.id), json!(e.bound.len())]));
        if let Some(p) = &e.state.plan {
            facts.push(fact("plan", vec![json!(e.id), json!(p)]));
        }
        // rounded down: at or over a whole ceiling exactly when Claude Code's per cent is
        if let Some(w) = e.state.seven_day.as_ref().filter(|w| w.resets_at > now) {
            facts.push(fact("weekly-used", vec![json!(e.id), json!(w.used_percent.floor() as i64)]));
        }
    }
    if gateway {
        facts.push(fact("place", vec![json!(GATEWAY)]));
        facts.push(fact("usable", vec![json!(GATEWAY)]));
        facts.push(fact("serves", vec![json!(GATEWAY), json!(model)]));
    }
    // the rounds: each place's tier as the policy holds it (the gateway's is the policy's own)
    let places: Vec<String> = accounts.iter().map(|e| e.id.clone()).chain(gateway.then(|| GATEWAY.to_string())).collect();
    let picker = Picker::new(policy, &facts)?;
    let tiers: BTreeMap<String, i64> = picker
        .holding("place-tier")
        .into_iter()
        .filter_map(|t| Some((t.first()?.clone(), t.get(1)?.parse().ok()?)))
        .filter(|(p, _)| places.contains(p))
        .collect();
    // an opencode account serves its provider's models (chosen by provider before), an agy account the models it
    // offers, named as the agent's model names them (`agy/…`: `agy::record`, `agy::record_own` keep them so; all while
    // it has not been asked yet: agy-rpc refuses one it does not offer), a Claude Code one Anthropic's
    let serving: Vec<&Entry> = accounts
        .iter()
        .filter(|e| match e.harness {
            Harness::Opencode => true,
            Harness::Agy => e.state.models.is_empty() || e.state.models.iter().any(|m| m == model),
            Harness::Claude => anthropic(model),
        })
        .collect();
    for e in &serving {
        facts.push(fact("serves", vec![json!(e.id), json!(model)]));
    }
    let mut rounds: BTreeMap<i64, Vec<&str>> = BTreeMap::new();
    for (p, t) in &tiers {
        rounds.entry(*t).or_default().push(p);
    }
    for (t, ids) in &rounds {
        let start = turns.get(t).and_then(|last| ids.iter().position(|id| *id > last.as_str())).unwrap_or(0);
        for (i, id) in ids.iter().enumerate() {
            facts.push(fact("turn", vec![json!(id), json!((i + ids.len() - start) % ids.len())]));
        }
    }
    let call = BTreeMap::from([("agent".to_string(), agent.to_string()), ("model".to_string(), model.to_string())]);
    let inst = Instance { kind: "account-for".into(), call, given: vec![], unmet: vec![] };
    let (outcome, _) = Picker::new(policy, &facts)?.pick(&inst, &[])?;
    let Some(chosen) = outcome.chosen else {
        // an account out for MODEL alone is back for it then; an Option's None is below every Some
        return Ok(match serving.iter().filter(|e| e.state.signed_in).filter_map(|e| e.state.out_until.max(e.state.model_out(model, now))).min() {
            Some(until) => Choice::AllOut { until },
            None => Choice::None,
        });
    };
    if let Some(t) = tiers.get(&chosen) {
        turns.insert(*t, chosen.clone());
    }
    if chosen != GATEWAY {
        return Ok(Choice::Account(chosen));
    }
    let why = if !serving.iter().any(|e| e.state.signed_in) {
        Some("no account is signed in".to_string())
    } else if !serving.iter().any(|e| e.state.can_take_model(model, now)) {
        None
    } else {
        // the rule that put the gateway before the best account, in its own words up to the first `:` or `.`
        Some(match outcome.decided_by.map(|c| c.why.unwrap_or(c.rule)) {
            Some(w) => format!("the placement policy: {}", w.split([':', '.']).next().unwrap_or(&w).trim()),
            None => "the placement policy".to_string(),
        })
    };
    Ok(Choice::Gateway { back: look_again(accounts, now), why })
}

/// Choose a place of H for AGENT of COORD and bind it when it is an account, under the harness's lock. A bound
/// agent whose account can still take its model keeps it; AVOID leaves that account out; PROVIDER limits the choice to
/// that provider's accounts.
#[allow(clippy::too_many_arguments)]
pub fn take(store: &Store, h: Harness, coord: &str, agent: &str, avoid: Option<&str>, provider: Option<&str>, placing: &Placing, now: i64, stamp: &str) -> Result<Choice, String> {
    let _lock = locked(store, h)?;
    let accounts: Vec<Entry> = store.list(h).into_iter().filter(|e| provider.is_none_or(|p| e.account.provider.as_deref() == Some(p))).collect();
    if let Some(e) = accounts.iter().find(|e| e.bound.iter().any(|b| b.coord == coord && b.agent == agent)) {
        if avoid != Some(e.id.as_str()) && e.state.can_take_model(&placing.model, now) {
            return Ok(Choice::Account(e.id.clone()));
        }
    }
    let others: Vec<Entry> = accounts.into_iter().filter(|e| avoid != Some(e.id.as_str())).collect();
    let choice = turned(store, h, &others, placing, agent, now)?;
    if let Choice::Account(id) = &choice {
        store.bind(h, id, coord, agent, stamp)?;
    }
    Ok(choice)
}

/// H's folder, made if need be, with its lock taken, which is waited for.
fn locked(store: &Store, h: Harness) -> Result<Lock, String> {
    store.ready()?;
    let dir = store.harness_dir(h);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Lock::take(&dir.join("lock")).map_err(|e| format!("{}: {e}", dir.display()))
}

/// [`choose`] among H's ACCOUNTS for AGENT with PLACING, on H's turns, which are written back when they changed;
/// under H's lock ([`locked`]).
fn turned(store: &Store, h: Harness, accounts: &[Entry], placing: &Placing, agent: &str, now: i64) -> Result<Choice, String> {
    let policy = placing.policy.as_ref().map_err(|e| format!("the placement policy: {e}"))?;
    let was = turns(store, h);
    let mut turns = was.clone();
    let choice = choose(accounts, &mut turns, policy, placing.gateway, agent, &placing.model, now).map_err(|e| format!("the placement policy: {e}"))?;
    if turns != was {
        let dir = store.harness_dir(h);
        write_json(&dir.join("turn.json"), &turns).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    Ok(choice)
}

/// What the places serve ([`Router::served`]).
pub struct Served {
    /// Each model with its provider: the gateway's (as last read, when this read failed), then those of the owner's
    /// accounts the gateway does not list.
    pub models: Vec<(String, String)>,
    /// The gateway's ids as this read leaves them ([`GatewayModels::serving`]: one its list stopped naming less than
    /// [`GONE_AFTER`] ago counts), None while every model counts as served (the coord's first read less than
    /// [`GONE_AFTER`] ago); why not, when the read failed.
    pub read: Result<Option<Vec<String>>, String>,
}

impl Served {
    /// The models, or why there are none: the read's failure, when nothing was kept and no account serves one.
    pub fn models(self) -> Result<Vec<(String, String)>, String> {
        match (self.models.is_empty(), self.read) {
            (true, Err(e)) => Err(e),
            (_, _) => Ok(self.models),
        }
    }
}

/// The router of one coord ([module](self)): where its policy is, which `account-for` is asked of; its gateway's
/// address and key; the owner's accounts; Claude Code's program; curl, for a plain call on a Go account
/// ([`super::plain`]); Codex's program, for its own models; and how long a call may take on the owner's accounts ([`CALL_BOUND`]; a test's is shorter).
#[derive(Clone, Debug)]
pub struct Router {
    places: Places,
    proxy: Proxy,
    accounts: Store,
    program: PathBuf,
    curl: PathBuf,
    codex: PathBuf,
    bound: Duration,
}

#[cfg(test)]
thread_local! {
    /// A test's accounts and stand-in Claude Code, for the routers made on its thread. Without them a test's router has
    /// no accounts: never the owner's.
    pub static TEST_ACCOUNTS: std::cell::RefCell<Option<(Store, PathBuf)>> = const { std::cell::RefCell::new(None) };
}

impl Router {
    /// The router of PLACES' coord: its gateway ([`Proxy::of`]), the owner's accounts ([`Store::of_env`]) and
    /// [`CLAUDE`].
    pub fn of(places: &Places) -> Router {
        #[cfg(test)]
        let (accounts, program) = TEST_ACCOUNTS.with(|t| t.borrow().clone()).unwrap_or_else(|| {
            (Store { root: PathBuf::new(), projects: PathBuf::new(), fault: Some("a test's router has no accounts".into()) }, CLAUDE.into())
        });
        #[cfg(not(test))]
        let (accounts, program) = (Store::of_env(), PathBuf::from(CLAUDE));
        let codex = PathBuf::from("codex");
        Router { places: places.clone(), proxy: Proxy::of(&places.coord), accounts, program, curl: "curl".into(), codex, bound: CALL_BOUND }
    }

    /// What the places serve, read now: the gateway's list; the policy's `claude-code-model` models while one of the
    /// owner's Claude Code accounts is usable; each usable opencode account's models with its provider; agy's; and
    /// those Codex's own catalog lists while the coord's Codex is signed in ([`super::codex::models`]). A failed read of
    /// the gateway gives the list kept. Blocking.
    pub fn served(&self) -> Served {
        let path = Path::new(&self.places.coord).join(GATEWAY_MODELS);
        let read = self.gateway_models();
        let kept = match &read {
            Ok(list) => {
                let kept = GatewayModels::after(read_json(&path), list.clone(), now_s());
                if let Err(e) = write_json(&path, &kept) {
                    eprintln!("coordd: {}: {e}", path.display());
                }
                Some(kept)
            }
            Err(_) => read_json::<GatewayModels>(&path),
        };
        let mut models = kept.as_ref().map(|g| g.listed()).unwrap_or_default();
        let on = |e: &Entry| e.state.signed_in && e.state.off.is_none() && e.state.leaving.is_none();
        let mut add = |id: String, provider: &str| {
            if !models.iter().any(|(m, _)| *m == id) {
                models.push((id, provider.to_string()));
            }
        };
        if self.accounts.list(Harness::Claude).iter().any(on) {
            let table = self.places.sources().and_then(|p| Picker::new(&p, &[])).map(|p| claude_models(&p)).unwrap_or_default();
            for (id, _) in table {
                add(id, "anthropic");
            }
        }
        for e in self.accounts.list(Harness::Opencode).iter().filter(|&e| on(e)) {
            if let Some(provider) = &e.account.provider {
                for id in &e.state.models {
                    add(id.clone(), provider);
                }
            }
        }
        // an agy account's, as t3code or agy last named them (`agy/gemini-…`), so New agent offers them
        for e in self.accounts.list(Harness::Agy).iter().filter(|&e| on(e)) {
            for id in &e.state.models {
                add(id.clone(), "agy");
            }
        }
        for id in super::codex::models(&self.codex, &self.places.coord) {
            add(id, "openai");
        }
        Served { models, read: read.map(|_| kept.and_then(|g| g.serving())) }
    }

    /// The gateway's `GET /v1/models`: each model's id and provider (`owned_by`).
    fn gateway_models(&self) -> Result<Vec<(String, String)>, String> {
        let resp = self
            .proxy
            .agent()
            .get(&format!("{}/v1/models", self.proxy.url))
            .header("Authorization", &format!("Bearer {}", self.proxy.key))
            .call()
            .map_err(|e| format!("the proxy's models: {e}"))?;
        let status = resp.status();
        let body: Value = resp.into_body().read_json().map_err(|e| format!("the proxy's models: {e}"))?;
        if !status.is_success() {
            return Err(format!("the proxy's models: {status}: {body}"));
        }
        Ok(body["data"]
            .as_array()
            .ok_or("the proxy's models: no data")?
            .iter()
            .filter_map(|m| Some((m["id"].as_str()?.to_string(), m["owned_by"].as_str().unwrap_or("").to_string())))
            .collect())
    }

    /// Run PROMPT on MODEL (`MODEL:EFFORT`), its answer held to SCHEMA and CHECK, where `account-for` puts it: one of
    /// the owner's Claude Code accounts ([`Router::take_call`]) or the gateway while its list names MODEL. An account at its
    /// limit is marked out and one that fails otherwise is passed over for this call, and the next place is asked. A model
    /// no account can take goes to the gateway. The whole call is bounded by [`CALL_BOUND`]. The answer and the account it
    /// ran on (None: the gateway). Blocking.
    pub fn call(&self, name: &str, prompt: &str, schema: &Value, model: &str, check: &dyn Fn(&Value) -> Result<(), String>) -> (Result<Value, Failure>, Option<String>) {
        let (got, place) = self.counted(name, prompt, schema, model, check);
        (got.map(|(answer, _)| answer), place)
    }

    /// [`Router::call`], with the tokens its place counted beside the answer (None: it gave no count).
    pub fn counted(&self, name: &str, prompt: &str, schema: &Value, model: &str, check: &dyn Fn(&Value) -> Result<(), String>) -> (Result<(Value, Option<u64>), Failure>, Option<String>) {
        if let Some(id) = plain::go_model(model) {
            return self.on_go(name, prompt, schema, model, id, check);
        }
        let bare = call::model_effort(model).0;
        let placing = Placing { policy: self.places.sources(), gateway: gateway_serves(&self.places.coord, bare), model: bare.to_string() };
        let table = placing.policy.as_ref().ok().and_then(|p| Picker::new(p, &[]).ok()).map(|p| claude_models(&p)).unwrap_or_default();
        let cc = table.into_iter().find(|(m, _)| m == base_id(bare)).map(|(_, cc)| format!("{cc}{}", &bare[base_id(bare).len()..]));
        let deadline = Instant::now() + self.bound;
        let (mut tried, mut back, mut failed): (Vec<String>, Option<i64>, Option<(Failure, String)>) = (Vec::new(), None, None);
        let until = loop {
            if !tried.is_empty() && Instant::now() >= deadline {
                break None;
            }
            match self.take_call(&placing, cc.is_some(), &tried, now_s()) {
                Err(e) => return (Err(Failure::Error(e)), None),
                Ok(Choice::Gateway { .. }) => return (self.on_gateway(name, prompt, schema, model, check, deadline), None),
                Ok(Choice::Account(id)) => {
                    let cc = cc.as_deref().expect("take_call gives an account only for a model with a --model there");
                    match self.on_account(&id, cc, prompt, schema, model, check, deadline) {
                        Err(Failure::Out(until)) => back = Some(back.map_or(until, |b| b.min(until))),
                        Err(Failure::Error(e)) => failed = Some((Failure::Error(e), id.clone())),
                        answer => return (answer, Some(id)),
                    }
                    tried.push(id);
                }
                Ok(Choice::AllOut { until }) => break Some(until),
                Ok(Choice::None) => break None,
            }
        };
        match (failed, back.into_iter().chain(until).min()) {
            (Some((f, id)), _) => (Err(f), Some(id)),
            (None, Some(t)) => (Err(Failure::Out(t)), None),
            (None, None) => (self.on_gateway(name, prompt, schema, model, check, deadline), None),
        }
    }

    /// The place for one call with PLACING: [`turned`] among the owner's Claude Code accounts that run on a sign-in (a
    /// key account's key reaches a session alone, by roster's `apiKeyHelper`), less those TRIED, under the harness's
    /// lock; nothing is bound. With ACCOUNTS false (the call's model has no `--model` there), or without the store (a
    /// test's router, or no HOME), there are no accounts and no turns.
    fn take_call(&self, placing: &Placing, accounts: bool, tried: &[String], now: i64) -> Result<Choice, String> {
        let h = Harness::Claude;
        if !accounts || self.accounts.ready().is_err() {
            let policy = placing.policy.as_ref().map_err(|e| format!("the placement policy: {e}"))?;
            return choose(&[], &mut Turns::new(), policy, placing.gateway, "", &placing.model, now).map_err(|e| format!("the placement policy: {e}"));
        }
        let _lock = locked(&self.accounts, h)?;
        let accounts: Vec<Entry> = self.accounts.list(h).into_iter().filter(|e| e.account.kind == Kind::Signin && !tried.contains(&e.id)).collect();
        turned(&self.accounts, h, &accounts, placing, "", now)
    }

    /// The call on OpenCode Go model ID (MODEL as asked): one plain completion ([`plain::call`]) on each Go account
    /// that can take it in turn, until one answers, all within [`plain::PLAIN_BOUND`]. The answer and the account.
    fn on_go(&self, name: &str, prompt: &str, schema: &Value, model: &str, id: &str, check: &dyn Fn(&Value) -> Result<(), String>) -> (Result<(Value, Option<u64>), Failure>, Option<String>) {
        let deadline = Instant::now() + self.bound.min(plain::PLAIN_BOUND);
        let body = plain::body(name, prompt, schema, id, call::model_effort(model).1);
        let mut last = None;
        for acc in plain::accounts(&self.accounts, id, now_s()) {
            let sent = now_s();
            match plain::call(&self.curl, &self.accounts.usage_key_file(&acc), &body, check, deadline) {
                Ok(a) => {
                    note_served(&self.places.coord, &acc, model, sent);
                    return (Ok(a), Some(acc));
                }
                Err(f) => last = Some((f, acc)),
            }
            if Instant::now() >= deadline {
                break;
            }
        }
        match last {
            Some((f, acc)) => (Err(f), Some(acc)),
            None => (Err(Failure::Error(format!("no OpenCode Go account with a kept key can take {model}"))), None),
        }
    }

    /// The call on the gateway: its chat completion ([`call::request_body`]), ended at DEADLINE. A 200 is the gateway
    /// serving MODEL ([`SERVED`]), whatever its answer is worth.
    fn on_gateway(&self, name: &str, prompt: &str, schema: &Value, model: &str, check: &dyn Fn(&Value) -> Result<(), String>, deadline: Instant) -> Result<(Value, Option<u64>), Failure> {
        let body = call::request_body(name, prompt, schema, model);
        let sent = now_s();
        let resp = self
            .proxy
            .agent()
            .post(&format!("{}/v1/chat/completions", self.proxy.url))
            .config()
            .timeout_global(Some(deadline.saturating_duration_since(Instant::now())))
            .build()
            .header("Authorization", &format!("Bearer {}", self.proxy.key))
            .header("Content-Type", "application/json")
            .send(call::head_first(&body))
            .map_err(|e| Failure::Error(e.to_string()))?;
        let status = resp.status();
        let v: Value = resp.into_body().read_json().map_err(|e| Failure::Error(format!("{status}: {e}")))?;
        if !status.is_success() {
            return Err(Failure::Error(format!("{status}: {}", v.get("error").unwrap_or(&v))));
        }
        // the model the 200 names when the gateway's list names it too (an answer from another model frees no agent
        // held on the one asked); else the one asked
        let answered = v["model"].as_str().filter(|m| gateway_names(&self.places.coord, m)).unwrap_or(model);
        note_served(&self.places.coord, GATEWAY, answered, sent);
        if let Some(line) = v["model"].as_str().and_then(|m| answered_as(model, m, now_s())) {
            eprintln!("{line}");
        }
        call::answer_with(&v, check).map(|a| (a, call::completion_tokens(&v)))
    }

    /// The call on account ID: one fresh Claude Code call in print mode, in the account's folder, from
    /// [`CALL_FOLDER`], with no tools, settings, skills, MCP or kept session and [`CALL_SYSTEM`] as its system prompt;
    /// its answer held to SCHEMA and CHECK; ended at DEADLINE. What it said of the account's limit is written to the
    /// account's state ([`limits::apply`]). With no answer and the account out: [`Failure::Out`].
    #[allow(clippy::too_many_arguments)]
    fn on_account(&self, id: &str, cc: &str, prompt: &str, schema: &Value, model: &str, check: &dyn Fn(&Value) -> Result<(), String>, deadline: Instant) -> Result<(Value, Option<u64>), Failure> {
        let h = Harness::Claude;
        let effort = call::model_effort(model).1;
        let coord = Path::new(&self.places.coord);
        let (folder, system) = (coord.join(CALL_FOLDER), coord.join(CALL_SYSTEM_FILE));
        let local = |e: std::io::Error| Failure::Error(format!("{}: {e}", coord.display()));
        std::fs::create_dir_all(&folder).map_err(local)?;
        if std::fs::read_to_string(&system).ok().as_deref() != Some(CALL_SYSTEM) {
            std::fs::write(&system, CALL_SYSTEM).map_err(local)?;
        }
        let schema = call::head_first(&call::answer_schema(schema));
        let mut cmd = std::process::Command::new(&self.program);
        cmd.env_clear().envs(signin::env(&self.accounts.config(h, id))).env("TZ", "UTC").current_dir(&folder);
        cmd.args(["-p", "--model", cc, "--output-format", "stream-json", "--verbose", "--json-schema", &schema]);
        cmd.args(["--tools", "", "--setting-sources", "", "--disable-slash-commands", "--strict-mcp-config", "--no-session-persistence"]);
        cmd.arg("--system-prompt-file").arg(&system).args(effort.map(|e| ["--effort", e]).into_iter().flatten());
        cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).process_group(0);
        let ran = |e: std::io::Error| Failure::Error(format!("{} on account {id}: {e}", self.program.display()));
        let sent = now_s();
        let mut child = cmd.spawn().map_err(ran)?;
        let mut input = child.stdin.take().ok_or_else(|| Failure::Error(format!("{} on account {id}: no input", self.program.display())))?;
        let prompt = prompt.to_string();
        std::thread::spawn(move || input.write_all(prompt.as_bytes()));
        let Some(out) = output_by(child, deadline).map_err(ran)? else {
            return Err(Failure::Error(format!("Claude Code on account {id} gave no answer within the call's {} s, and was ended", self.bound.as_secs())));
        };
        let frames: Vec<Value> = String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let result = frames.iter().rev().find(|v| v["type"] == "result");
        let answer = result.filter(|r| r["is_error"] == false && r["structured_output"].is_object());
        // a turn the account answered, of the schema or not, is the account serving MODEL ([`SERVED`])
        if result.is_some_and(|r| r["is_error"] == false) {
            note_served(&self.places.coord, id, model, sent);
        }
        let now = now_s();
        let event = frames.iter().rev().find(|v| v["type"] == "rate_limit_event").map(|v| limits::from_event(&v["rate_limit_info"]));
        // the refused turn only when the event did not say it is out: the event names when it is back, the refusal never
        let named = event.as_ref().is_some_and(|e| matches!(e.verdict, limits::Verdict::Out(_)));
        let refused = frames.iter().rev().filter(|_| !named).find_map(|v| {
            let error = v["error"].as_str().filter(|_| v["type"] == "assistant")?;
            let words: Vec<&str> = v["message"]["content"].as_array()?.iter().filter_map(|c| c["text"].as_str()).collect();
            Some(limits::from_failed(&json!({"error": error, "last_assistant_message": words.join(" ")}), false, "")).filter(|s| s.out_for == Some(limits::RATE_LIMIT))
        });
        let said: Vec<limits::Said> = event.into_iter().chain(refused.filter(|_| answer.is_none())).collect();
        if !said.is_empty() {
            match self.accounts.update(h, id, |s| said.iter().for_each(|x| limits::apply(s, x, now))) {
                Ok(s) => {
                    if let Some(until) = s.out_until.filter(|t| *t > now && answer.is_none()) {
                        return Err(Failure::Out(until));
                    }
                }
                Err(e) => eprintln!("coordd: account {id}: what a call said of it was not kept: {e}"),
            }
        }
        if let Some(r) = answer {
            return call::answered(&r["structured_output"], check).map(|a| (a, result_tokens(r)));
        }
        Err(match result {
            Some(r) if r["subtype"] == "error_max_structured_output_retries" => Failure::Invalid("Claude Code gave no answer of the schema".into()),
            Some(r) if r["is_error"] == false => Failure::Invalid("no structured output".into()),
            Some(r) => Failure::Error(format!("Claude Code on account {id}: {}", r["result"].as_str().or(r["subtype"].as_str()).unwrap_or("an error"))),
            None => {
                let err = String::from_utf8_lossy(&out.stderr);
                Failure::Error(format!("Claude Code on account {id} gave no answer ({}): {}", out.status, err.trim().lines().last().unwrap_or("")))
            }
        })
    }
}

/// What Claude Code's result frame R says the call read and wrote: its input, cache and output tokens.
fn result_tokens(r: &Value) -> Option<u64> {
    let u = &r["usage"];
    let cached = u["cache_creation_input_tokens"].as_u64().unwrap_or(0) + u["cache_read_input_tokens"].as_u64().unwrap_or(0);
    Some(u["input_tokens"].as_u64()? + cached + u["output_tokens"].as_u64()?)
}

/// CHILD's output once it ends, or None when it still runs at DEADLINE; then its process group is killed and
/// reaped. Output is read on its own threads, and a process outside the group holding it holds the call no longer
/// than DEADLINE.
pub(super) fn output_by(mut child: std::process::Child, deadline: Instant) -> std::io::Result<Option<std::process::Output>> {
    fn drain(from: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut got = Vec::new();
            if let Some(mut r) = from {
                let _ = r.read_to_end(&mut got);
            }
            got
        })
    }
    let (stdout, stderr) = (drain(child.stdout.take()), drain(child.stderr.take()));
    let waiting = |done: &dyn Fn() -> std::io::Result<bool>| -> std::io::Result<bool> {
        while !done()? {
            if Instant::now() >= deadline {
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(true)
    };
    let ended = waiting(&|| ended(&child))?;
    // SAFETY: signals the process group this child leads, whose leader is not reaped yet (`ended` leaves it, and only
    // this function waits on it), so the id is still its own
    unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
    let status = child.wait()?;
    if !ended || !waiting(&|| Ok(stdout.is_finished() && stderr.is_finished()))? {
        return Ok(None);
    }
    let joined = |t: std::thread::JoinHandle<Vec<u8>>| t.join().unwrap_or_default();
    Ok(Some(std::process::Output { status, stdout: joined(stdout), stderr: joined(stderr) }))
}

/// Whether CHILD has ended, seen without reaping it (`WNOWAIT`): its id, and its group's, stay its own.
fn ended(child: &std::process::Child) -> std::io::Result<bool> {
    // SAFETY: an all-zero `siginfo_t` is a valid one, and waitid writes only into it, which outlives the call
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    if unsafe { libc::waitid(libc::P_PID, child.id(), &mut info, libc::WEXITED | libc::WNOHANG | libc::WNOWAIT) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: waitid with WNOHANG leaves the zeroed info as it was while the child runs, and fills it in once it ended
    Ok(unsafe { info.si_pid() } != 0)
}

#[cfg(test)]
mod tests {
    use super::super::testing::Tmp;
    use super::super::{Account, State, Window};
    use super::*;

    /// The shipped placement policy, read as coordd reads it.
    fn policy() -> Vec<(Scope, Vec<Decl>)> {
        let file = format!("{}/../../agent-base/policy/placement.eflint", env!("CARGO_MANIFEST_DIR"));
        vec![(Scope::Global, crate::policy::syntax::parse(&file, &std::fs::read_to_string(&file).unwrap()).unwrap())]
    }

    fn entry(id: &str, tier: i64, signed_in: bool, out_until: Option<i64>) -> Entry {
        Entry {
            harness: Harness::Claude,
            id: id.into(),
            account: Account { label: id.into(), tier, added: "x".into(), kind: Default::default(), provider: None },
            state: State { signed_in, out_until, ..State::default() },
            bound: vec![],
        }
    }

    /// E with PLAN and its week at USED per cent, which starts again at RESETS.
    fn week(mut e: Entry, plan: &str, used: f64, resets: i64) -> Entry {
        (e.state.plan, e.state.seven_day) = (Some(plan.into()), Some(Window { used_percent: used, resets_at: resets }));
        e
    }

    /// An agent's model: one of the policy's `claude-code-model`, which every Claude Code account serves.
    const M: &str = "claude-opus-5-5";

    fn pick(accounts: &[Entry], turns: &mut Turns, gateway: bool, now: i64) -> Choice {
        choose(accounts, turns, &policy(), gateway, "w", M, now).unwrap()
    }

    fn account(id: &str) -> Choice {
        Choice::Account(id.into())
    }

    #[test]
    fn the_highest_tier_that_can_take_it_and_round_robin_within_it() {
        let accounts = [entry("a", 10, true, None), entry("b", 10, true, None), entry("c", 0, true, None), entry("d", 20, false, None)];
        let mut turns = Turns::new();
        let picks: Vec<Choice> = (0..3).map(|_| pick(&accounts, &mut turns, false, 0)).collect();
        // d is of the highest tier but not signed in; a and b take turns; c's tier is not reached while they can
        assert_eq!(picks, [account("a"), account("b"), account("a")]);
    }

    #[test]
    fn an_account_that_is_out_is_passed_over_and_the_next_tier_serves() {
        let accounts = [entry("a", 10, true, Some(500)), entry("b", 10, true, Some(900)), entry("c", 0, true, None)];
        let mut turns = Turns::new();
        assert_eq!(pick(&accounts, &mut turns, false, 100), account("c"));
        // at 500 a is back, and its tier is first again
        assert_eq!(pick(&accounts, &mut turns, false, 500), account("a"));
    }

    #[test]
    fn all_out_says_when_the_first_comes_back_and_none_signed_in_says_so() {
        let out = [entry("a", 10, true, Some(500)), entry("b", 0, true, Some(300)), entry("c", 0, false, None)];
        assert_eq!(pick(&out, &mut Turns::new(), false, 100), Choice::AllOut { until: 300 });
        assert_eq!(pick(&[entry("c", 0, false, None)], &mut Turns::new(), false, 100), Choice::None);
        assert_eq!(pick(&[], &mut Turns::new(), false, 100), Choice::None);
    }

    #[test]
    fn the_gateway_is_the_last_place_and_says_why_it_was_taken() {
        // an account of the window's lowest tier (1) comes before the gateway (0)
        let mut turns = Turns::new();
        assert_eq!(pick(&[entry("a", 1, true, None)], &mut turns, true, 100), account("a"));
        // every one out: the gateway, looked at again when the first is back, in the words of every account out
        let out = [entry("a", 10, true, Some(500)), entry("b", 1, true, Some(300))];
        assert_eq!(pick(&out, &mut turns, true, 100), Choice::Gateway { back: 300, why: None });
        // none signed in: the gateway, with no time to look again
        let none = [entry("a", 10, false, None)];
        assert_eq!(pick(&none, &mut turns, true, 100), Choice::Gateway { back: limits::UNKNOWN_RESET, why: Some("no account is signed in".into()) });
        assert_eq!(pick(&[], &mut turns, true, 100), Choice::Gateway { back: limits::UNKNOWN_RESET, why: Some("no account is signed in".into()) });
        // the gateway's turn is kept in its tier's round under its own name
        assert_eq!(turns.get(&0).map(String::as_str), Some(GATEWAY));
    }

    #[test]
    fn an_account_at_its_plans_weekly_ceiling_has_no_space_while_another_place_has() {
        let (now, resets) = (1_000, 9_000);
        let mut turns = Turns::new();
        // a is at its Max ceiling (85 >= 80): b takes the agents of the top tier, again and again
        let mut accounts = vec![week(entry("a", 30, true, None), "max", 85.0, resets), week(entry("b", 30, true, None), "max", 40.0, resets), week(entry("c", 10, true, None), "pro", 50.0, resets)];
        assert_eq!((0..3).map(|_| pick(&accounts, &mut turns, false, now)).collect::<Vec<_>>(), [account("b"), account("b"), account("b")]);
        // b at its ceiling exactly (80.0; 79.9 is under it): the next tier, c
        accounts[1] = week(entry("b", 30, true, None), "max", 79.9, resets);
        assert_eq!(pick(&accounts, &mut turns, false, now), account("b"));
        accounts[1] = week(entry("b", 30, true, None), "max", 80.0, resets);
        assert_eq!(pick(&accounts, &mut turns, false, now), account("c"));
        // c at its Pro ceiling (70) too: with the gateway, the gateway, in the policy's words, looked at when a week starts again
        accounts[2] = week(entry("c", 10, true, None), "pro", 70.0, resets - 1);
        let why = Some("the placement policy: no space at its plan's weekly ceiling".to_string());
        assert_eq!(pick(&accounts, &mut turns, true, now), Choice::Gateway { back: resets - 1, why });
        // and with no gateway the agent does not stand still: the top tier by its turn (b was last there)
        assert_eq!(pick(&accounts, &mut turns, false, now), account("a"));
        // a week that is over says nothing: from its reset on the top tier has space again, and goes on round (a
        // was the last there, so b)
        assert_eq!(pick(&accounts, &mut turns, true, resets), account("b"));
        // a plan with no ceiling (a Team seat), or none known, is never full
        let seat = [week(entry("s", 30, true, None), "team", 99.0, resets), entry("k", 30, true, None)];
        let mut turns = Turns::new();
        assert_eq!((0..2).map(|_| pick(&seat, &mut turns, true, now)).collect::<Vec<_>>(), [account("k"), account("s")]);
    }

    #[test]
    fn a_burst_of_35_starts_goes_round_its_tier_and_one_account_alone_in_the_top_tier_takes_them_all() {
        let t = Tmp::new("burst");
        let h = Harness::Claude;
        let placing = Placing { policy: Ok(policy()), gateway: true, model: M.into() };
        // his seat alone in the top tier, at 50 % of its week (a Team plan: no ceiling), the gateway below
        t.store.create(h, "seat", "Seat", 30, crate::accounts::Kind::Signin, "x").unwrap();
        t.store.update(h, "seat", |s| (s.signed_in, s.heard, s.plan, s.seven_day) = (true, Some(50), Some("team".into()), Some(Window { used_percent: 50.0, resets_at: 9_000 }))).unwrap();
        let on: Vec<Choice> = (0..35).map(|i| take(&t.store, h, "research", &format!("w{i}"), None, None, &placing, 100, "now").unwrap()).collect();
        assert!(on.iter().all(|c| *c == account("seat")), "{on:?}");
        assert_eq!(t.store.get(h, "seat").unwrap().bound.len(), 35);
        // a second account in that tier: the next 35 go round the two, none twice in a row
        t.store.create(h, "two", "Two", 30, crate::accounts::Kind::Signin, "x").unwrap();
        t.store.update(h, "two", |s| (s.signed_in, s.heard) = (true, Some(50))).unwrap();
        let on: Vec<Choice> = (35..70).map(|i| take(&t.store, h, "research", &format!("w{i}"), None, None, &placing, 100, "now").unwrap()).collect();
        assert!(on.windows(2).all(|p| p[0] != p[1]), "{on:?}");
        assert_eq!((on.iter().filter(|c| **c == account("seat")).count(), on.iter().filter(|c| **c == account("two")).count()), (17, 18));
        // a restart of each keeps its own: nothing moves
        let again: Vec<Choice> = (0..70).map(|i| take(&t.store, h, "research", &format!("w{i}"), None, None, &placing, 100, "now").unwrap()).collect();
        assert_eq!(again.iter().filter(|c| **c == account("seat")).count(), 35 + 17);
    }

    #[test]
    fn an_account_messages_are_not_known_to_reach_takes_one_agent_and_the_rest_go_elsewhere_until_it_is_heard() {
        let t = Tmp::new("unheard");
        let h = Harness::Claude;
        let placing = Placing { policy: Ok(policy()), gateway: true, model: M.into() };
        // his seat, just signed in, alone in the top tier: the first of 35 goes there, the 34 others through the
        // gateway, which says why
        t.store.create(h, "seat", "Seat", 30, crate::accounts::Kind::Signin, "x").unwrap();
        t.store.update(h, "seat", |s| (s.signed_in, s.heard) = (true, None)).unwrap();
        let on: Vec<Choice> = (0..35).map(|i| take(&t.store, h, "research", &format!("w{i}"), None, None, &placing, 100, "now").unwrap()).collect();
        assert_eq!(on[0], account("seat"));
        let why = Some("the placement policy: messages not yet known to reach it, with agents on it already".to_string());
        assert!(on[1..].iter().all(|c| matches!(c, Choice::Gateway { why: w, .. } if *w == why)), "{on:?}");
        assert_eq!(t.store.get(h, "seat").unwrap().bound.len(), 1);
        // with nothing else to take it, no gateway and no other account, the seat takes one more all the same
        let alone = Placing { policy: Ok(policy()), gateway: false, model: M.into() };
        assert_eq!(take(&t.store, h, "research", "y", None, None, &alone, 100, "now").unwrap(), account("seat"));
        // without the gateway, the next tier takes them
        t.store.create(h, "low", "Low", 10, crate::accounts::Kind::Signin, "x").unwrap();
        t.store.update(h, "low", |s| (s.signed_in, s.heard) = (true, Some(50))).unwrap();
        let low = Placing { policy: Ok(policy()), gateway: false, model: M.into() };
        assert_eq!(take(&t.store, h, "research", "x1", None, None, &low, 100, "now").unwrap(), account("low"));
        // its test arrived: the seat takes the next ones
        t.store.update(h, "seat", |s| s.heard = Some(120)).unwrap();
        assert_eq!(take(&t.store, h, "research", "x2", None, None, &placing, 130, "now").unwrap(), account("seat"));
        // found deaf: it takes none, and the restart of the one on it does not keep it: both go to the next tier
        t.store.update(h, "seat", |s| (s.heard, s.deaf) = (None, Some(crate::accounts::Sign { since: 140, agent: "w0".into() }))).unwrap();
        assert_eq!(take(&t.store, h, "research", "x3", None, None, &placing, 150, "now").unwrap(), account("low"));
        assert_eq!(take(&t.store, h, "research", "w0", None, None, &placing, 150, "now").unwrap(), account("low"));
    }

    #[test]
    fn starts_made_at_once_take_turns_under_the_lock() {
        let t = Tmp::new("at-once");
        let h = Harness::Claude;
        for id in ["a", "b"] {
            t.store.create(h, id, id, 10, crate::accounts::Kind::Signin, "x").unwrap();
            t.store.update(h, id, |s| (s.signed_in, s.heard) = (true, Some(50))).unwrap();
        }
        let (store, placing) = (&t.store, &Placing { policy: Ok(policy()), gateway: false, model: M.into() });
        let on: Vec<Choice> = std::thread::scope(|sc| {
            let runs: Vec<_> = (0..8).map(|i| sc.spawn(move || take(store, h, "research", &format!("w{i}"), None, None, placing, 100, "now").unwrap())).collect();
            runs.into_iter().map(|r| r.join().unwrap()).collect()
        });
        assert_eq!((on.iter().filter(|c| **c == account("a")).count(), on.iter().filter(|c| **c == account("b")).count()), (4, 4));
        assert_eq!(t.store.get(h, "a").unwrap().bound.len() + t.store.get(h, "b").unwrap().bound.len(), 8);
    }

    #[test]
    fn take_binds_keeps_a_binding_over_a_restart_and_leaves_the_account_a_move_avoids() {
        let t = Tmp::new("take");
        let h = Harness::Claude;
        for id in ["a", "b"] {
            t.store.create(h, id, id, 0, crate::accounts::Kind::Signin, "x").unwrap();
            t.store.update(h, id, |s| (s.signed_in, s.heard) = (true, Some(50))).unwrap();
        }
        let placing = Placing { policy: Ok(policy()), gateway: false, model: M.into() };
        let take = |agent: &str, avoid: Option<&str>| take(&t.store, h, "research", agent, avoid, None, &placing, 100, "now").unwrap();
        assert_eq!(take("one", None), account("a"));
        assert_eq!(take("two", None), account("b"));
        // a restart of one: its own account again, and the turn does not advance
        assert_eq!(take("one", None), account("a"));
        assert_eq!(take("three", None), account("a"));
        // a move of one away from a
        assert_eq!(take("one", Some("a")), account("b"));
        assert_eq!(t.store.bound_to(h, "research", "one").unwrap().id, "b");
        // a is out: one's next start does not go back to it, and two stays on b
        t.store.update(h, "a", |s| s.out_until = Some(1000)).unwrap();
        t.store.update(h, "b", |s| s.out_until = Some(700)).unwrap();
        assert_eq!(take("one", None), Choice::AllOut { until: 700 });
        assert_eq!(t.store.bound_to(h, "research", "one").unwrap().id, "b");
        // a policy that could not be read is said, and nothing is bound
        let broken = Placing { policy: Err("placement.eflint:3: no such form".into()), gateway: false, model: M.into() };
        assert_eq!(super::take(&t.store, h, "research", "four", None, None, &broken, 100, "now"), Err("the placement policy: placement.eflint:3: no such form".into()));
        assert!(t.store.bound_to(h, "research", "four").is_none());
    }

    #[test]
    fn the_look_again_is_the_first_change_of_an_out_or_a_week_after_now() {
        let accounts = [entry("a", 10, true, Some(500)), week(entry("b", 10, true, None), "max", 90.0, 400), week(entry("c", 10, false, Some(200)), "max", 90.0, 150)];
        // c is not signed in: nothing of it counts
        assert_eq!(look_again(&accounts, 100), 400);
        assert_eq!(look_again(&accounts, 400), 500);
        assert_eq!(look_again(&accounts, 500), limits::UNKNOWN_RESET);
    }

    /// What a place serves is asked of the policy (`Requires serves(option, call.model)`): a Claude Code account serves
    /// the models of `claude-code-model`, and no other; the gateway what its list names. The router changes only the
    /// way: a Codex model never goes to an account, and with no place that serves it nothing is chosen, an account out
    /// not counted as one that comes back for it.
    #[test]
    fn an_account_serves_each_of_anthropics_models_and_the_gateway_what_its_list_names() {
        let ask = |accounts: &[Entry], gateway: bool, model: &str| choose(accounts, &mut Turns::new(), &policy(), gateway, "w", model, 100).unwrap();
        let (a, out) = ([entry("a", 10, true, None)], [entry("b", 10, true, Some(500))]);
        assert_eq!(ask(&a, true, "claude-sonnet-5-5"), account("a"), "his account before the gateway, for a model it serves");
        assert_eq!(ask(&a, false, "claude-haiku-4-5-20251001"), account("a"));
        assert!(matches!(ask(&a, true, "gpt-6.2-sol"), Choice::Gateway { .. }), "a Codex model goes to the gateway");
        assert_eq!(ask(&a, false, "gpt-6.2-sol"), Choice::None, "and to no account when the gateway does not serve it");
        assert_eq!(ask(&out, false, "gpt-6.2-sol"), Choice::None, "an account out is no account that comes back for it");
        assert_eq!(ask(&out, false, "claude-sonnet-5-5"), Choice::AllOut { until: 500 });
        assert_eq!(ask(&a, true, "claude-unknown-9"), account("a"), "one of Anthropic's the table does not name: his account all the same (p88)");
        assert_eq!(ask(&a, false, "claude-fable-5-2[1m]"), account("a"), "and a variant /model gives (p87)");
    }

    /// Only lists that lack the model say it is not served: none before any read nor in the first ten minutes, and a
    /// read that names it again starts its span anew. A variant is its base id.
    #[test]
    fn the_gateway_serves_every_model_until_ten_minutes_after_its_first_read_and_then_what_its_lists_name() {
        let t = Tmp::new("gateway-serves");
        let coord = t.dir.display().to_string();
        let read = |list: &[(&str, &str)], at: i64| gateway_read(&coord, list, at);
        const WITHOUT: &[(&str, &str)] = &[("gpt-6.2-sol", "openai"), ("claude-fable-5-2", "anthropic")];
        const WITH: &[(&str, &str)] = &[("gpt-6.2-sol", "openai"), ("claude-fable-5-2", "anthropic"), ("claude-sonnet-5-5", "anthropic")];
        assert!(gateway_serves(&coord, "claude-sonnet-5-5"), "no read yet");
        read(WITHOUT, 1_000);
        assert!(gateway_serves(&coord, "claude-sonnet-5-5") && gateway_serves(&coord, "claude-opus-5-5"), "the first read leaves them out");
        read(WITH, 1_300);
        read(WITHOUT, 1_599);
        assert!(gateway_serves(&coord, "claude-opus-5-5"), "599 s after the first read");
        read(WITHOUT, 1_600);
        assert!(!gateway_serves(&coord, "claude-opus-5-5"), "600 s after the first read, though the one at 1300 named a model it had not");
        assert!(gateway_serves(&coord, "claude-sonnet-5-5"), "named at 1300: missing since 1599");
        assert!(gateway_serves(&coord, "gpt-6.2-sol") && gateway_serves(&coord, "claude-fable-5-2[1m]"));
        read(WITHOUT, 2_198);
        assert!(gateway_serves(&coord, "claude-sonnet-5-5"));
        read(WITHOUT, 2_199);
        assert!(!gateway_serves(&coord, "claude-sonnet-5-5") && gateway_serves(&coord, "claude-fable-5-2"), "600 s of reads without it");
    }

    /// A model the list stops naming stays served until every read over [`GONE_AFTER`] has lacked it; a read naming it
    /// again ends the span. What is kept reads back from the file.
    #[test]
    fn a_model_the_gateways_list_leaves_out_is_served_until_every_read_for_ten_minutes_lacked_it() {
        let t = Tmp::new("gateway-span");
        let (coord, path) = (t.dir.display().to_string(), t.dir.join(GATEWAY_MODELS));
        let (sonnet, sol) = (("claude-sonnet-5-5".to_string(), "anthropic".to_string()), ("gpt-6.2-sol".to_string(), "openai".to_string()));
        let ids = |g: &GatewayModels| g.listed().into_iter().map(|(id, _)| id).collect::<Vec<_>>();
        let read = |list: Vec<(String, String)>, at: i64| {
            let g = GatewayModels::after(read_json(&path), list, at);
            write_json(&path, &g).unwrap();
            g
        };
        read(vec![sonnet.clone(), sol.clone()], 0);
        read(vec![sonnet.clone(), sol.clone()], 1_000);
        assert_eq!(ids(&read(vec![sol.clone()], 1_010)), ["gpt-6.2-sol", "claude-sonnet-5-5"]);
        assert!(gateway_serves(&coord, "claude-sonnet-5-5"), "one read without it");
        assert!(read(vec![sonnet.clone(), sol.clone()], 1_070).missing.is_empty(), "named again a minute later");
        assert!(gateway_serves(&coord, "claude-sonnet-5-5"));
        read(vec![sol.clone()], 1_100);
        read(vec![], 1_650);
        assert_eq!(ids(&read(vec![], 1_699)), ["claude-sonnet-5-5", "gpt-6.2-sol"], "599 s without it");
        assert!(gateway_serves(&coord, "claude-sonnet-5-5"));
        assert_eq!(ids(&read(vec![], 1_700)), ["gpt-6.2-sol"], "600 s: gone; gpt-6.2-sol has been missing for 50");
        assert!(!gateway_serves(&coord, "claude-sonnet-5-5") && gateway_serves(&coord, "gpt-6.2-sol"));
    }

    /// The child named in DIR/`child.ID` was ended with the program that started it: awaited while it lives (gone, or
    /// a zombie its new parent has not reaped), it did not write DIR/`ended.ID`, as it does when it ends by itself.
    fn child_ended(dir: &Path, id: &str) {
        let child = std::fs::read_to_string(dir.join(format!("child.{id}"))).unwrap_or_default().trim().to_string();
        assert!(!child.is_empty(), "the program named its child");
        let lives = || std::fs::read_to_string(format!("/proc/{child}/stat")).is_ok_and(|s| !s.rsplit(')').next().is_some_and(|r| r.trim_start().starts_with('Z')));
        while lives() {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!dir.join(format!("ended.{id}")).exists(), "its child {child} was ended, not left to end by itself");
    }

    /// One still running at its deadline is ended with everything it started, and gives no output.
    #[test]
    fn a_program_still_running_at_its_deadline_is_ended_with_all_it_started() {
        let t = Tmp::new("output-by");
        let program = t.program("slow", "#!/bin/sh\n( sleep 60; echo > \"$1/ended.x\" ) & echo $! > \"$1/child.x\"\nwait\n");
        let mut cmd = std::process::Command::new(&program);
        cmd.arg(&t.dir).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).process_group(0);
        let running = cmd.spawn().unwrap();
        while !std::fs::read_to_string(t.dir.join("child.x")).unwrap_or_default().ends_with('\n') {
            assert!(!ended(&running).unwrap(), "the program ended before it named its child");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(output_by(running, Instant::now()).unwrap().is_none(), "no output from one still running");
        child_ended(&t.dir, "x");
    }

    /// The stand-in Claude Code: it writes, beside itself and by the account whose folder it runs under, its arguments
    /// one a line, its folder and what is in it, its input and its environment's names; and answers in stream-json,
    /// "from ACCOUNT" with a `rate_limit_event` that gives its week, or, for an account named in `out`, its
    /// `rate_limit_event` rejected until the time in `resets` and a turn refused at the limit. For one named in `slow`
    /// it first waits on a child of its own, which ends by itself after a minute, writing `ended.ID` as it does; for one
    /// in `lingers` it leaves such a child, whose id it writes to `child.ID`, running on its output when it ends; for
    /// one in `broken` it ends with its login refused. Made up: no model is called.
    const STAND_IN: &str = r#"#!/bin/sh
d=$(dirname "$0")
acct=$(basename "$(dirname "$CLAUDE_CONFIG_DIR")")
for a in "$@"; do printf '%s\n' "$a"; done > "$d/args.$acct"
pwd > "$d/cwd.$acct"
ls -A > "$d/ls.$acct"
cat > "$d/stdin.$acct"
env | sed 's/=.*//' | sort > "$d/env.$acct"
echo "$CLAUDE_CONFIG_DIR" > "$d/config.$acct"
echo '{"type":"system","subtype":"init","session_id":"s"}'
if grep -qx "$acct" "$d/slow" 2>/dev/null; then
  ( sleep 60; echo > "$d/ended.$acct" ) & wait
fi
if grep -qx "$acct" "$d/lingers" 2>/dev/null; then
  ( sleep 60; echo > "$d/ended.$acct" ) & echo $! > "$d/child.$acct"
fi
if grep -qx "$acct" "$d/broken" 2>/dev/null; then
  echo '{"type":"result","subtype":"success","is_error":true,"result":"Invalid API key · Please run /login","session_id":"s"}'
  exit 1
fi
if grep -qx "$acct" "$d/out" 2>/dev/null; then
  echo '{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","rateLimitType":"five_hour","resetsAt":'"$(cat "$d/resets")"'},"session_id":"s"}'
  echo '{"type":"assistant","message":{"content":[{"type":"text","text":"You have hit your limit"}]},"error":"rate_limit","session_id":"s"}'
  echo '{"type":"result","subtype":"success","is_error":true,"result":"You have hit your limit","session_id":"s"}'
else
  echo '{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","unifiedWindows":{"seven_day":{"utilization":0.25,"resetsAt":4102444800}}},"session_id":"s"}'
  echo '{"type":"result","subtype":"success","is_error":false,"result":"","structured_output":{"answer":"from '"$acct"'"},"session_id":"s"}'
fi
"#;

    /// The router's call, on made-up accounts: the call runs on a free account as one fresh call, never on a key
    /// account; an account at its limit is marked out; with all out and the gateway not serving, out until the first is
    /// back. Codex models and models the list does not name go to the gateway. A refused login is passed over; a call past
    /// its bound fails. Each 200 is noted by place and model; an answer naming another model is noted for it when the list
    /// names it. A model's family on the gateway is its provider's.
    #[test]
    fn a_call_runs_where_the_policy_puts_its_model_as_one_fresh_call_and_goes_on_past_an_account_at_its_limit() {
        let t = Tmp::new("router-call");
        let h = Harness::Claude;
        for (id, tier, kind) in [("a", 10, Kind::Signin), ("b", 10, Kind::Signin), ("k", 30, Kind::Key)] {
            t.store.create(h, id, id, tier, kind, "x").unwrap();
            t.store.update(h, id, |s| (s.signed_in, s.heard) = (true, Some(50))).unwrap();
        }
        let eleven = now_s() + 3600;
        t.store.update(h, "a", |s| s.out_until = Some(eleven)).unwrap();
        let program = t.program("claude", STAND_IN);
        TEST_ACCOUNTS.with(|x| *x.borrow_mut() = Some((t.store.clone(), program)));
        const CODEX_ONLY: &[(&str, &str)] = &[("gpt-6.2-sol", "openai")];
        let asked = crate::clean::tests::stand_in_serving(CODEX_ONLY, |q| {
            let answer = json!({"choices": [{"finish_reason": "stop", "message": {"content": json!({"answer": "from the gateway"}).to_string()}}]});
            match q["model"].as_str() {
                Some("gpt-6-astra") => ("400 Bad Request", json!({"error": {"message": "unknown provider for model gpt-6-astra"}})),
                // the gateway answers with another model than the one asked, a snapshot its list does not give
                Some("gpt-6.2-sol-max") => ("200 OK", json!({"model": "gpt-6.2-sol", "choices": answer["choices"]})),
                Some("gpt-6.2-sol") => ("200 OK", json!({"model": "gpt-6.2-sol-2026-11-01", "choices": answer["choices"]})),
                Some("claude-unknown-9") => ("200 OK", json!({"model": "claude-unknown-9-20261101", "choices": answer["choices"]})),
                _ => ("200 OK", answer),
            }
        });
        let coord = t.dir.join("coord");
        std::fs::create_dir_all(&coord).unwrap();
        let places = Places { base: format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), coord: coord.display().to_string(), ..Default::default() };
        let router = Router::of(&places);
        let served = router.served();
        assert_eq!(served.read, Ok(None), "the coord's first read: every model counts as served");
        assert!(gateway_serves(&places.coord, "claude-sonnet-5-5"));
        age_reads(&places.coord, GONE_AFTER);
        let served = router.served();
        assert_eq!(served.read, Ok(Some(vec!["gpt-6.2-sol".to_string()])), "ten minutes on, what its lists name");
        let family = |m: &str| gateway_family(&places.coord, m);
        assert_eq!([family("gpt-6.2-sol:low"), family("claude-opus-5-5[1m]"), family("gpt-6-astra")], [Some("codex".into()), Some("claude".into()), None], "the family a gateway login's type names: its provider on the list, Anthropic's own names, none for one never listed");
        assert!(served.models.contains(&("claude-sonnet-5-5".into(), "anthropic".into())), "his accounts' models are served: {:?}", served.models);
        let schema = json!({"type": "string"});
        let ask = |model: &str| router.call("whom", "made up", &schema, model, &|_| Ok(()));
        let read = |what: &str| std::fs::read_to_string(t.dir.join(what)).unwrap_or_default();

        assert_eq!(ask("claude-sonnet-5-5"), (Ok(json!("from b")), Some("b".to_string())));
        assert_eq!(served_taken(&places.coord).into_keys().collect::<Vec<_>>(), [("b".to_string(), "claude-sonnet-5-5".to_string())], "B served it: noted by its place and model");
        let system = coord.join(CALL_SYSTEM_FILE);
        let args: Vec<String> = read("args.b").lines().map(String::from).collect();
        let want = [
            "-p", "--model", "claude-sonnet-5-5", "--output-format", "stream-json", "--verbose", "--json-schema",
            &call::head_first(&call::answer_schema(&schema)),
            "--tools", "", "--setting-sources", "", "--disable-slash-commands", "--strict-mcp-config", "--no-session-persistence",
            "--system-prompt-file", &system.display().to_string(),
        ];
        assert_eq!(args, want);
        assert_eq!(std::fs::read_to_string(&system).unwrap(), CALL_SYSTEM);
        assert_eq!(read("cwd.b").trim(), coord.join(CALL_FOLDER).display().to_string());
        assert_eq!(read("ls.b"), "", "run from an empty folder");
        assert_eq!(read("stdin.b"), "made up", "the prompt on its input");
        assert_eq!(read("config.b").trim(), t.store.config(h, "b").display().to_string());
        let env: Vec<String> = read("env.b").lines().map(String::from).collect();
        for name in ["CLAUDE_CONFIG_DIR", "HOME", "PATH", "BROWSER", "TZ"] {
            assert!(env.contains(&name.to_string()), "{name}: {env:?}");
        }
        assert!(env.iter().all(|n| ["CLAUDE_CONFIG_DIR", "HOME", "PATH", "BROWSER", "TZ", "PWD", "OLDPWD", "SHLVL", "_"].contains(&n.as_str())), "nothing of coordd's: {env:?}");
        assert!(t.store.get(h, "b").unwrap().bound.is_empty(), "a call binds nothing");
        assert_eq!(read("args.k"), "", "a key account takes no call");
        assert!(asked.lock().unwrap().is_empty(), "nothing went to the gateway");
        let b = t.store.state(h, "b").unwrap();
        assert_eq!((b.seven_day.map(|w| w.resets_at), b.limits_from.as_str()), (Some(4_102_444_800), "rate_limit_event"), "what the answered call said of b is kept");

        // A back, but at its limit when asked: out until the time it said, and B answers
        let resets = eleven + 600;
        t.store.update(h, "a", |s| s.out_until = None).unwrap();
        std::fs::write(t.dir.join("out"), "a\n").unwrap();
        std::fs::write(t.dir.join("resets"), resets.to_string()).unwrap();
        assert_eq!(ask("claude-sonnet-5-5"), (Ok(json!("from b")), Some("b".to_string())));
        assert_eq!(served_taken(&places.coord).into_keys().collect::<Vec<_>>(), [("b".to_string(), "claude-sonnet-5-5".to_string())], "A at its limit served nothing");
        assert_eq!(read("args.a").lines().nth(2), Some("claude-sonnet-5-5"), "A was asked first, its turn in the round");
        assert_eq!(t.store.state(h, "a").unwrap().out_until, Some(resets));

        // B at its limit too, and the gateway does not serve it: out, until the first is back
        std::fs::write(t.dir.join("out"), "a\nb\n").unwrap();
        assert_eq!(ask("claude-sonnet-5-5"), (Err(Failure::Out(resets)), None));
        assert!(served_taken(&places.coord).is_empty(), "both at their limit: nothing served");
        assert!(Failure::Out(resets).at_limit());

        // a Codex model: the gateway, as before; also once its lists have stopped naming it; one it never named: the
        // gateway's refusal
        assert_eq!(ask("gpt-6.2-sol:low"), (Ok(json!("from the gateway")), None));
        assert_eq!(served_taken(&places.coord).into_keys().collect::<Vec<_>>(), [(GATEWAY.to_string(), "gpt-6.2-sol".to_string())], "the gateway's 200, its effort aside: the model asked, as the snapshot it names is not listed");
        assert_eq!(answered_as("gpt-6.2-sol", "gpt-6.2-sol-2026-11-01", 0), None, "the call said the pair in coordd's log, once");
        let sent = std::mem::take(&mut *asked.lock().unwrap());
        assert_eq!(sent.iter().map(|q| (q["model"].as_str().unwrap_or_default(), q["reasoning_effort"].as_str().unwrap_or_default())).collect::<Vec<_>>(), [("gpt-6.2-sol", "low")]);
        write_json(&coord.join(GATEWAY_MODELS), &GatewayModels { first: now_s() - GONE_AFTER, at: now_s(), models: vec![], missing: vec![] }).unwrap();
        assert!(!gateway_serves(&places.coord, "gpt-6.2-sol"));
        assert_eq!(ask("gpt-6.2-sol"), (Ok(json!("from the gateway")), None), "no other place serves it, whatever the list says");
        assert_eq!(served_taken(&places.coord).into_keys().collect::<Vec<_>>(), [(GATEWAY.to_string(), "gpt-6.2-sol".to_string())]);
        assert_eq!(router.served().read, Ok(Some(vec!["gpt-6.2-sol".to_string()])));
        assert!(matches!(ask("gpt-6-astra"), (Err(Failure::Error(e)), None) if e.starts_with("400") && !Failure::Error(e.clone()).at_limit()));
        assert!(served_taken(&places.coord).is_empty(), "a read of the list and a refusal serve nothing");
        assert_eq!(ask("gpt-6.2-sol-max"), (Ok(json!("from the gateway")), None));
        assert_eq!(answered_as("gpt-6.2-sol-max", "gpt-6.2-sol", 0), None, "said once");
        let (a, b) = ("made-up-asked", "made-up-named");
        assert_eq!(answered_as(a, b, 0), Some(format!("coordd: the gateway answered {a} with {b} at {}", crate::core::at_secs(0))), "the two names and the time only");
        assert_eq!((answered_as(a, b, 5), answered_as("made-up-asked:low", "made-up-asked", 5)), (None, None), "once per pair; the model asked, its effort aside, is no other");
        assert_eq!(served_taken(&places.coord).into_keys().collect::<Vec<_>>(), [(GATEWAY.to_string(), "gpt-6.2-sol".to_string())], "answered by another model: the agents held on the one asked stay held");
        assert_eq!(ask("claude-unknown-9"), (Ok(json!("from the gateway")), None), "no --model for it on his accounts: the gateway answers");
        assert_eq!(served_taken(&places.coord).into_keys().collect::<Vec<_>>(), [(GATEWAY.to_string(), "claude-unknown-9".to_string())], "answered by a snapshot the list does not give: the model asked");

        // A's login refused: B answers, and A is as it was; both refused: B's error, the last asked
        for id in ["a", "b"] {
            t.store.update(h, id, |s| s.out_until = None).unwrap();
        }
        std::fs::write(t.dir.join("out"), "").unwrap();
        std::fs::write(t.dir.join("broken"), "a\n").unwrap();
        std::fs::remove_file(t.dir.join("args.a")).unwrap();
        let a = t.store.state(h, "a").unwrap();
        assert_eq!(ask("claude-sonnet-5-5"), (Ok(json!("from b")), Some("b".to_string())));
        assert!(t.dir.join("args.a").exists(), "A was asked first");
        assert_eq!(t.store.state(h, "a").unwrap(), a, "a refused login is passed over, not written");
        std::fs::write(t.dir.join("broken"), "a\nb\n").unwrap();
        assert!(matches!(ask("claude-sonnet-5-5"), (Err(Failure::Error(e)), Some(id)) if id == "b" && e.ends_with("Please run /login")));

        // A still running at the bound: the call fails then, B not asked, A's child not waited out (it would have
        // written `ended.a`); whether A had started it by then is the machine's load, so its end with A is
        // `output_by`'s test
        std::fs::write(t.dir.join("broken"), "").unwrap();
        std::fs::write(t.dir.join("slow"), "a\n").unwrap();
        let quick = Router { bound: Duration::from_secs(1), ..Router::of(&places) };
        let (a, b_asked, began) = (t.store.state(h, "a").unwrap(), read("args.b"), Instant::now());
        let (answer, on) = quick.call("whom", "made up", &schema, "claude-sonnet-5-5", &|_| Ok(()));
        let (took, waited) = (began.elapsed(), t.dir.join("ended.a").exists());
        assert!(matches!(&answer, Err(Failure::Error(e)) if e.ends_with("gave no answer within the call's 1 s, and was ended")), "{answer:?}");
        assert!(on.as_deref() == Some("a") && took >= quick.bound && !waited, "{on:?} after {took:?}, its child waited out: {waited}");
        assert_eq!((t.store.state(h, "a").unwrap(), read("args.b")), (a, b_asked), "A's state as it was, and B not asked");
        std::fs::write(t.dir.join("slow"), "").unwrap();

        // one that answers and ends, a child of its own left on its output: its answer at its end, the child ended
        std::fs::write(t.dir.join("lingers"), "a\nb\n").unwrap();
        let (answer, on) = ask("claude-sonnet-5-5");
        let on = on.expect("on an account");
        assert_eq!(answer, Ok(json!(format!("from {on}"))), "on {on}");
        child_ended(&t.dir, &on);
        std::fs::write(t.dir.join("lingers"), "").unwrap();

        // a coord in its first ten minutes: a 200 is noted for the model asked, whatever the answer names
        let fresh = t.dir.join("fresh");
        std::fs::create_dir_all(&fresh).unwrap();
        let first = Places { base: places.base.clone(), coord: fresh.display().to_string(), ..Default::default() };
        let router_first = Router::of(&first);
        assert_eq!(router_first.served().read, Ok(None));
        assert_eq!(router_first.call("whom", "made up", &schema, "gpt-6.2-sol-max", &|_| Ok(())), (Ok(json!("from the gateway")), None));
        assert_eq!(served_taken(&first.coord).into_keys().collect::<Vec<_>>(), [(GATEWAY.to_string(), "gpt-6.2-sol-max".to_string())], "no list counted yet: the model asked");

        // the gateway gone: its last list is kept, and what it serves does not change
        let gone = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = Some(format!("http://{gone}")));
        let served = Router::of(&places).served();
        assert!(served.read.is_err());
        assert!(served.models.contains(&("gpt-6.2-sol".into(), "openai".into())) && served.models.contains(&("claude-sonnet-5-5".into(), "anthropic".into())), "{:?}", served.models);
        assert!(gateway_serves(&places.coord, "gpt-6.2-sol") && !gateway_serves(&places.coord, "claude-sonnet-5-5"));
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
        TEST_ACCOUNTS.with(|x| *x.borrow_mut() = None);
    }

    /// His opencode accounts' models are served as opencode named them, with its provider: those of accounts
    /// signed in, not off and not being removed; a model two accounts name once; none is Anthropic's to the router.
    #[test]
    fn a_signed_in_opencode_accounts_models_are_served_with_its_provider() {
        let t = Tmp::new("router-opencode");
        let h = Harness::Opencode;
        let named = |p: &str, ms: &[&str]| ms.iter().map(|m| format!("{p}/{m}")).collect::<Vec<_>>();
        for (id, provider, ms) in [("go", "opencode-go", ["kimi-k3", "claude-haiku-5-5"]), ("go2", "opencode-go", ["kimi-k3", "glm-5.3"]), ("zen", "opencode", ["big-pickle", "kimi-k3"])] {
            t.store.create_opencode(id, id, 10, provider, "x").unwrap();
            t.store.update(h, id, |s| (s.signed_in, s.models) = (true, named(provider, &ms))).unwrap();
        }
        for id in ["off", "leaving", "out"] {
            t.store.create_opencode(id, id, 10, "opencode-go", "x").unwrap();
            t.store
                .update(h, id, |s| {
                    (s.signed_in, s.models) = (id != "out", named("opencode-go", &[id]));
                    (s.off, s.leaving) = ((id == "off").then_some(1), (id == "leaving").then_some(1));
                })
                .unwrap();
        }
        TEST_ACCOUNTS.with(|x| *x.borrow_mut() = Some((t.store.clone(), t.dir.join("no-claude"))));
        let gone = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = Some(format!("http://{gone}")));
        let coord = t.dir.join("coord");
        std::fs::create_dir_all(&coord).unwrap();
        let places = Places { base: format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), coord: coord.display().to_string(), ..Default::default() };
        let mut served = Router::of(&places).served().models().expect("his accounts' models, the gateway not read");
        served.sort();
        let want = [("opencode-go/claude-haiku-5-5", "opencode-go"), ("opencode-go/glm-5.3", "opencode-go"), ("opencode-go/kimi-k3", "opencode-go"), ("opencode/big-pickle", "opencode"), ("opencode/kimi-k3", "opencode")];
        assert_eq!(served, want.map(|(m, p)| (m.to_string(), p.to_string())));
        assert!(!anthropic("opencode-go/claude-haiku-5-5"), "opencode Go's model is not one of Anthropic's to the router");
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
        TEST_ACCOUNTS.with(|x| *x.borrow_mut() = None);
    }

    /// Codex's own catalog's listed models are served while the coord's Codex is signed in, beside the accounts'; a
    /// hidden one is not; signed out, none.
    #[test]
    fn codexs_own_listed_models_are_served_while_it_is_signed_in() {
        use std::os::unix::fs::PermissionsExt;
        let t = Tmp::new("router-codex");
        TEST_ACCOUNTS.with(|x| *x.borrow_mut() = Some((t.store.clone(), t.dir.join("no-claude"))));
        let gone = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = Some(format!("http://{gone}")));
        let coord = t.dir.join("coord");
        std::fs::create_dir_all(coord.join("codex-home")).unwrap();
        let codex = t.dir.join("codex");
        let catalog = r#"{"models":[{"slug":"gpt-6.1-terra","visibility":"list"},{"slug":"gpt-reserve","visibility":"hide"},{"slug":"gpt-6-luna","visibility":"list"}]}"#;
        let stand = |signed: bool| {
            let status = if signed { "echo 'Logged in using ChatGPT'" } else { "echo 'Not logged in'; exit 1" };
            let script = format!("#!/bin/sh\n[ \"$CODEX_HOME\" = '{}' ] || exit 9\ncase $1 in login) {status} ;; debug) echo '{catalog}' ;; esac\n", coord.join("codex-home").display());
            std::fs::write(&codex, script).unwrap();
            std::fs::set_permissions(&codex, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        let places = Places { base: format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), coord: coord.display().to_string(), ..Default::default() };
        let served = || {
            let mut r = Router::of(&places);
            r.codex = codex.clone();
            r.served().models
        };
        stand(true);
        assert_eq!(served(), [("gpt-6.1-terra", "openai"), ("gpt-6-luna", "openai")].map(|(m, p)| (m.to_string(), p.to_string())));
        stand(false);
        assert!(served().is_empty(), "signed out: none");
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
        TEST_ACCOUNTS.with(|x| *x.borrow_mut() = None);
    }

    /// A Go model's call goes the plain way on his Go accounts with a kept key, never to Claude Code or the gateway: a
    /// free model on one that is out too, a paid one only on one that is not; an account that fails passes the call on
    /// to the next, and with none left the call fails in words.
    #[test]
    fn a_go_models_call_is_one_plain_completion_on_a_go_account_with_a_kept_key() {
        use std::os::unix::fs::PermissionsExt;
        let t = Tmp::new("router-plain");
        let h = Harness::Opencode;
        for id in ["a-bad", "b-out", "c-go", "d-nokey"] {
            t.store.create_opencode(id, id, 10, "opencode-go", "x").unwrap();
            t.store.update(h, id, |s| (s.signed_in, s.out_until) = (true, (id == "b-out").then_some(now_s() + 3600))).unwrap();
            if id != "d-nokey" {
                super::super::opencode::keep_usage_key(&t.store, id, &coord_types::Secret::new(format!("sk-{id}"))).unwrap();
            }
        }
        // a stand-in curl: the answer canned for the account whose key file it was given (`resp-ID`)
        let (curl, said) = (t.dir.join("curl"), |id: &str, text: String| std::fs::write(t.dir.join(format!("resp-{id}")), text).unwrap());
        let content = |id: &str| json!({"choices": [{"finish_reason": "stop", "message": {"content": json!({"answer": format!("from {id}")}).to_string()}}]}).to_string();
        said("a-bad", "{\"error\":{\"message\":\"overloaded\"}}\n429".into());
        for id in ["b-out", "c-go"] {
            said(id, format!("{}\n200", content(id)));
        }
        let script = format!("#!/bin/sh\nfor a in \"$@\"; do case \"$a\" in @*/usage-key) k=\"${{a#@}}\";; esac; done\ncat >/dev/null\ncat \"{}/resp-$(basename \"$(dirname \"$k\")\")\"\n", t.dir.display());
        std::fs::write(&curl, script).unwrap();
        std::fs::set_permissions(&curl, std::fs::Permissions::from_mode(0o755)).unwrap();
        TEST_ACCOUNTS.with(|x| *x.borrow_mut() = Some((t.store.clone(), t.dir.join("no-claude"))));
        let coord = t.dir.join("coord");
        std::fs::create_dir_all(&coord).unwrap();
        let places = Places { base: format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), coord: coord.display().to_string(), ..Default::default() };
        let router = Router { curl, ..Router::of(&places) };
        let schema = json!({"type": "string"});
        assert_eq!(router.call("ahead", "p", &schema, "opencode-go/beam-free", &|_| Ok(())), (Ok(json!("from b-out")), Some("b-out".into())), "a-bad passed over, b-out takes a free model");
        assert_eq!(router.call("ahead", "p", &schema, "opencode-go/glm-5.3", &|_| Ok(())), (Ok(json!("from c-go")), Some("c-go".into())), "a paid model not on b-out");
        assert!(served_taken(&places.coord).contains_key(&("b-out".to_string(), "opencode-go/beam-free".to_string())));
        for id in ["b-out", "c-go"] {
            said(id, "{\"error\":{\"type\":\"FreeUsageLimitError\",\"message\":\"limit\"}}\n429".into());
        }
        let (got, on) = router.call("ahead", "p", &schema, "opencode-go/beam-free", &|_| Ok(()));
        assert_eq!((got, on), (Err(Failure::Error("429 FreeUsageLimitError: limit".into())), Some("c-go".into())), "the last account's failure");
        let none = Router { accounts: Store { root: PathBuf::new(), projects: PathBuf::new(), fault: Some("none".into()) }, ..router };
        assert_eq!(none.call("ahead", "p", &schema, "opencode-go/beam-free", &|_| Ok(())).0, Err(Failure::Error("no OpenCode Go account with a kept key can take opencode-go/beam-free".into())));
        TEST_ACCOUNTS.with(|x| *x.borrow_mut() = None);
    }

    /// Nobody but the router names the gateway's paths: outside tests, `/v1/chat/completions` and `/v1/models`
    /// are the router's alone.
    #[test]
    fn only_the_router_names_the_gateways_paths() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let (mut found, mut dirs) = (Vec::new(), vec![src.clone()]);
        while let Some(dir) = dirs.pop() {
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                let p = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                if p.is_dir() {
                    if name != "tests" {
                        dirs.push(p);
                    }
                    continue;
                }
                if !name.ends_with(".rs") || name.starts_with("tests") || name.ends_with("_tests.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&p).unwrap();
                let lines: Vec<&str> = text.lines().collect();
                // up to the file's test module
                let end = lines.windows(2).position(|w| w[0].trim() == "#[cfg(test)]" && w[1].contains("mod ")).unwrap_or(lines.len());
                let code = lines[..end].join("\n");
                for path in ["/v1/chat/completions", "/v1/models"] {
                    if code.contains(path) {
                        found.push(format!("{} {path}", p.strip_prefix(&src).unwrap().display()));
                    }
                }
            }
        }
        found.sort();
        // plain.rs names Go's own endpoint ([`plain::GO_CHAT_URL`]), not the gateway's
        assert_eq!(found, ["accounts/plain.rs /v1/chat/completions", "accounts/router.rs /v1/chat/completions", "accounts/router.rs /v1/models"]);
    }
}
