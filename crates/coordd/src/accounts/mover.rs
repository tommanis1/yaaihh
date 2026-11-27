//! Moving an agent to another account, apart from the rule for when.
//!
//! **The rule** is [`due`], a function of the account's state on file, asked when an agent's turn ends and when
//! something new is seen of its account; its answer is a mark on the agent's process (`Live::account_due`).
//!
//! **The move** is coordd's restart (`Core::to_restart`, `roster restart`): once the agent is not in a turn, its session
//! is ended and started again on its own conversation. Every start asks `coordc account-for`, so a first start, a
//! restart and a move all go through the same choice (`router::take`). The conversation stays: every account's
//! `projects` is the same folder (mod.rs).
//!
//! **Nothing here writes the store under the core's lock.** Each change is three steps: the core says what is to be done
//! ([`Core::account_for_begin`], [`Core::move_begin`], [`Core::account_said`]), a task does it off the lock
//! ([`Choosing::run`], [`Keep::run`]), and the core takes the result ([`Core::account_for_end`], [`Core::move_end`],
//! [`Core::account_kept`]), reading the agent again.
//!
//! When every account is out, the agent stays as it is, marked, handed nothing, its view saying until when, and the
//! restart is tried again when the first account is back.

use super::{limits, opencode, router, telemetry, usage, Harness, State, Store};
use crate::core::{err, now, ok, Agent, Core, Who};
use crate::policy::call::model_effort;
use coord_types::Reply;
use serde_json::json;

/// A choice of account for an agent, to be made off the core's lock: the store's files are read and written under
/// its own lock, which is waited for while another of the owner's coordds holds it. Made by
/// [`Core::account_for_begin`] or [`Core::move_begin`]; [`Choosing::run`] makes it; the core takes the [`Chosen`].
pub struct Choosing {
    store: Store,
    /// The coord's id and the agent, as bindings name them.
    coord: String,
    name: String,
    how: How,
    /// The harness whose accounts it is chosen from: Claude Code's, or the agent's own (an opencode or agy agent's).
    harness: Harness,
    /// An opencode agent's: its model's provider, whose opencode accounts alone it is chosen from. None: every account
    /// of the harness.
    provider: Option<String>,
    /// Where the placement policy's files are, read off the core's lock; whether the gateway is a place: the coord has
    /// it and it serves the agent's model (never for an opencode agent's choice: the gateway is Claude's); and that
    /// model, without its `:EFFORT` (`router::Placing`).
    places: crate::policy::Places,
    gateway: bool,
    model: String,
    /// When the end of the agent's last process cut its turn (`Agent::down_cut`, `follow::cut_at`), seconds: its start
    /// goes on with that turn. None for every other choice.
    down_cut: Option<i64>,
}

/// Which account a [`Choosing`] is for.
/// A Claude Code agent of the owner's accounts running through the gateway meanwhile: its account's limit was met, no
/// other account could take it, and this coord has the gateway. It is bound to no account while it lasts; in the
/// registry it is the `account` line with `left`.
#[derive(Debug, Clone, PartialEq)]
pub struct OnGateway {
    /// The account it left, for that account's card.
    pub left: Option<String>,
    /// Since when (RFC 3339).
    pub since: String,
    /// When to look whether an account can take it again (seconds): the first reset the accounts named;
    /// [`limits::UNKNOWN_RESET`] when none did, and 0 after a load of the registry (at once).
    pub back: i64,
}

/// An agent's last move, onto an account or the gateway, as its registry line says it ([`Core::account_record`]).
#[derive(Debug, Clone, PartialEq)]
pub struct AccountMove {
    /// Why, in coordd's words.
    pub why: String,
    /// When (RFC 3339).
    pub since: String,
    /// The account it left; None from none.
    pub from: Option<String>,
}

/// When a change of where an agent runs reaches its session ([`Core::set_native`]).
enum When {
    /// It does not run: from its next start.
    NextStart,
    /// It is in a turn: by a restart on its session when the turn has ended.
    TurnEnd,
    /// By a restart on its session now.
    Now,
    /// It runs through the gateway already, as its account is out: nothing restarts.
    Already,
}

enum How {
    /// The next by tier and turn (`router::take`); with LEAVE, not the one the agent is on.
    Next { leave: bool },
    /// This one, named by the owner.
    To(String),
}

/// What a [`Choosing`] came to for agent NAME: the choice, already bound and its folder laid out when it is an
/// account, or why not; FROM is the account the agent was bound to before.
pub struct Chosen {
    name: String,
    /// The [`Choosing`]'s: the harness, and an opencode agent's provider.
    harness: Harness,
    provider: Option<String>,
    from: Option<String>,
    choice: Result<router::Choice, String>,
    /// The account chosen runs on a key and its key file is not there to be used: it was written as taking no agent.
    no_key: bool,
    /// FROM is being removed (`State::leaving`) or is turned off (`State::off`): the agent leaves it whatever the
    /// choice came to, and why ([`LEAVING`], [`OFF`]).
    leaving: Option<&'static str>,
    /// The [`Choosing`]'s.
    down_cut: Option<i64>,
}

impl Choosing {
    /// Make the choice and bind it. Blocking, and it may wait for the store's lock: never under the core's lock.
    pub fn run(self) -> Chosen {
        let h = self.harness;
        let (now_s, stamp) = (now_s(), now());
        let from = self.store.bound_to(h, &self.coord, &self.name);
        let leaving = from.as_ref().and_then(|e| match (e.state.leaving, e.state.off) {
            (Some(_), _) => Some(LEAVING),
            (None, Some(_)) => Some(OFF),
            (None, None) => None,
        });
        let from = from.map(|e| e.id);
        let choice = match &self.how {
            How::To(id) if from.as_deref() == Some(id.as_str()) => Ok(router::Choice::Account(id.clone())),
            How::To(id) => match self.store.get(h, id) {
                None => Err(format!("no account {id}")),
                Some(e) if !e.state.can_take_model(&self.model, now_s) => Err(format!("account {id} cannot take an agent now ({})", shown(e.account.kind, &e.state, now_s))),
                Some(_) => self.store.bind(h, id, &self.coord, &self.name, &stamp).map(|()| router::Choice::Account(id.clone())),
            },
            How::Next { leave } => {
                let placing = router::Placing { policy: self.places.sources(), gateway: self.gateway, model: self.model.clone() };
                router::take(&self.store, h, &self.coord, &self.name, from.as_deref().filter(|_| *leave), self.provider.as_deref(), &placing, now_s, &stamp)
            }
        };
        // an opencode agent that no account of its provider can take runs on folders of its own, and an agy agent not
        // at all: a binding left from an earlier model or sign-in is let go, so the card names only who runs on it
        if h != Harness::Claude && matches!(choice, Ok(router::Choice::None | router::Choice::AllOut { .. } | router::Choice::Gateway { .. })) {
            self.store.unbind(h, &self.coord, &self.name);
        }
        // the account's folder as a session needs it, before roster is told of it
        let choice = choice.and_then(|c| match &c {
            router::Choice::Account(id) => self.store.layout(h, id).map(|()| c),
            _ => Ok(c),
        });
        // a key account whose key file went since it was set takes no agent: written here, said by who takes this
        let no_key = match &choice {
            Ok(router::Choice::Account(id)) => self.store.get(h, id).is_some_and(|e| e.account.kind == super::Kind::Key) && !self.store.key_set(h, id).unwrap_or(false),
            _ => false,
        };
        if let (true, Ok(router::Choice::Account(id))) = (no_key, &choice) {
            let _ = self.store.update(h, id, |s| s.signed_in = false);
        }
        Chosen { name: self.name, harness: h, provider: self.provider, from, choice, no_key, leaving, down_cut: self.down_cut }
    }
}

/// `coord account key ID [--remove]`, coordc's three questions about key account ID's key file
/// (`Request::AccountKey`), and the last step of the window's key field. The key is in none of them: coordc reads it
/// from the owner's terminal and writes the file itself, so it is in no request, no record and no journal line of
/// coordd's. Blocking, and Set and Removed may wait for the account's lock: never under the core's lock. The Ok is
/// the answer; [`Core::account_key_end`] follows it.
///
/// - Where: the path of ID's key file, for an account of kind key.
/// - Set: the file is there ([`Store::key_set`]: the owner's alone and not empty; its content is not read), so the
///   account can take agents. Whatever was said of an earlier key's limits is dropped with it (a key that was
///   refused is replaced by this).
/// - Removed: the file is gone, and the account takes no agent.
pub fn key_step(store: &Store, id: &str, step: coord_types::KeyStep) -> Result<String, String> {
    use coord_types::KeyStep;
    let h = key_harness(store, id);
    match store.get(h, id).map(|e| e.account.kind) {
        None => return Err(format!("no account {id} (coord account new {id} --key)")),
        Some(super::Kind::Signin) => return Err(format!("account {id} runs on a sign-in, not on a key (coord account signin {id})")),
        Some(super::Kind::Key) => {}
    }
    if step == KeyStep::Where {
        return Ok(store.key_file(h, id).display().to_string());
    }
    let set = store.key_set(h, id)?;
    if set != (step == KeyStep::Set) {
        return Err(format!("account {id}'s key file is {}", if set { "still there" } else { "not there or empty" }));
    }
    let now = now_s();
    store.update(h, id, |s| *s = State { signed_in: set, signed_in_seen: now, auth_method: set.then(|| "api_key".to_string()), ..State::default() })?;
    Ok(format!("account {id}: {}", if set { "key set" } else { "key removed" }))
}

/// The harness whose key file account ID's key is kept in: an agy account's own, else Claude Code's (an opencode
/// account's key goes into opencode's own sign-in).
pub fn key_harness(store: &Store, id: &str) -> Harness {
    if store.harness_of(id) == Some(Harness::Agy) { Harness::Agy } else { Harness::Claude }
}

/// What a session said of its account's limits, to be written into the account's state off the core's lock
/// ([`Core::account_said`] makes it, [`Core::account_keep`] hands it on, [`Core::account_kept`] takes the result).
pub struct Keep {
    /// The agent whose session said it (none: the policy's word, [`Core::key_again`]), and the account it is said of,
    /// with the harness whose account it is.
    pub name: String,
    pub id: String,
    h: Harness,
    said: limits::Said,
    now: i64,
    /// What the journal says once it is kept: the reset a turn's transcript row named, a key that was refused, an
    /// account taken again by the policy.
    note: Option<String>,
    /// The policy's `key-again-after` seconds, when this is its word: written only if the account, as it is on file
    /// under its lock, is still due ([`limits::again_due`]), so that what another coordd wrote meanwhile stands.
    again: Option<u64>,
}

impl Keep {
    /// Write it; whether anything was written. Blocking, and it may wait for the account's lock: never under the
    /// core's lock.
    pub fn run(&self, store: &Store) -> Result<bool, String> {
        let mut written = false;
        store.update(self.h, &self.id, |s| {
            if self.again.is_none_or(|secs| limits::again_due(s, self.said.model.as_deref(), secs, self.now)) {
                limits::apply(s, &self.said, self.now);
                written = true;
            }
        })?;
        Ok(written)
    }
}

/// When the policy's `key-again-after` is looked at next ([`Core::key_again`]).
#[derive(Default)]
pub struct KeyAgain {
    /// The time (seconds) of the next look at the key accounts that are out; None: none is known to be out.
    at: Option<i64>,
    /// The policy's seconds at the last look: a change of them is looked at at once.
    secs: Option<u64>,
}

impl KeyAgain {
    /// Look at the next round: something says a key account may be out (coordd's start, a start that found every
    /// account out, a key account's failed turn).
    pub fn look() -> KeyAgain {
        KeyAgain { at: Some(0), secs: None }
    }
}

/// The sessions that run on an account, by session id: the agent and the account. The telemetry receiver names a
/// failed request's agent and account by it, from its own tasks, off the core's lock.
pub type Sessions = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, (String, String)>>>;

/// Now, in seconds since the epoch.
pub fn now_s() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// The rule: an agent on an account in STATE is to move, and why: the account is being removed ([`LEAVING`]) or
/// turned off ([`OFF`]), it can take no request (a limit was met), or it is no longer signed in.
pub fn due(state: &State, now: i64) -> Option<String> {
    if state.leaving.is_some() {
        return Some(LEAVING.into());
    }
    if state.off.is_some() {
        return Some(OFF.into());
    }
    if !state.signed_in {
        return Some("its account is not signed in any more".into());
    }
    if state.deaf.is_some() {
        return Some(DEAF.into());
    }
    state.out_until.filter(|t| *t > now).map(|t| match state.out_for.as_deref() {
        Some(limits::REFUSED) => "its account's requests are refused (authentication_failed); it is back when it is signed in again or cleared".into(),
        _ => format!("its account is out {}", until(t)),
    })
}

/// Why an agent moves off an account marked for removal (`State::leaving`).
const LEAVING: &str = "its account is being removed";

/// Why an agent moves off an account the owner turned off (`State::off`).
const OFF: &str = "its account is turned off";

/// Why an agent moves off a deaf account (`State::deaf`).
const DEAF: &str = "its account does not take messages: deliveries to its sessions were never seen there";
/// Why an agent restarts on its own account after a delivery handed to it was never seen in its session.
const UNSEEN_TURN: &str = "a delivery handed to it was never seen in its session: it restarts on its account to take it again";

/// Why an agent whose turn its sign-in account's login refused restarts ([`Core::account_said`]).
const REFUSED_TURN: &str = "its account's login refused its turn";

/// "until T", or that nothing said when (`limits::UNKNOWN_RESET`).
fn until(t: i64) -> String {
    if t == limits::UNKNOWN_RESET { "with no reset time known".into() } else { format!("until {}", crate::core::at_secs(t)) }
}

/// What a project with no way but the owner's accounts ([`Core::accounts_only`]) says when none can take an
/// agent: where to add one in his window.
const NO_ACCOUNT_YET: &str = "this project has no account yet: add one under Accounts";

impl Core {
    /// This coord's name among the owner's coords: its folder's.
    pub fn coord_id(&self) -> String {
        std::path::Path::new(&self.coord).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }

    /// Why NAME cannot run on the owner's accounts, if it cannot.
    fn not_native(&self, name: &str) -> Option<String> {
        let Some(a) = self.agents.get(name) else { return Some(format!("no agent named {name}")) };
        if a.kind == "opencode" {
            return Some(format!("{name} is an opencode agent: it runs on your opencode account of its model's provider by itself"));
        }
        if a.kind == "agy" {
            return Some(format!("{name} is an agy agent: it runs on one of your agy accounts by itself"));
        }
        if a.kind != "claude" {
            return Some(format!("{name} is {} agent; only Claude Code agents run on your accounts so far", crate::core::kind_article(&a.kind)));
        }
        if self.project.as_ref().is_some_and(|p| p.project.boxed) {
            return Some("a boxed project's agents do not run on your accounts yet (an agent there is its own user, and an account's folder is yours)".into());
        }
        let model = model_effort(&a.model).0;
        if !router::anthropic(model) {
            return Some(format!("{name} runs on {model}, which is not one of Anthropic's own model names (coord model {name} claude-...)"));
        }
        None
    }

    /// `coord native NAME on|off`: NAME runs on the owner's accounts from now on, or through the model route again.
    /// Its record at once; a running session is restarted on its conversation once not in a turn.
    pub fn native(&mut self, who: &Who, name: &str, on: bool) -> Reply {
        if !matches!(who, Who::Owner) {
            return err("coord native: the owner's".into());
        }
        if !self.agents.contains_key(name) {
            return err(format!("coord native: no agent named {name}"));
        }
        if let Some(why) = self.not_native(name).filter(|_| on) {
            return err(format!("coord native: {why}"));
        }
        if self.agents[name].native == on {
            return ok(format!("{name} already runs {}", if on { "on your accounts" } else { "through the gateway" }));
        }
        if let Err(why) = self.accounts.ready() {
            return err(format!("coord native: {why}"));
        }
        // with no account signed in its next start would be refused: said now, with nothing changed
        if on && !self.accounts.list(Harness::Claude).iter().any(|e| e.state.signed_in) {
            return err("coord native: no account is signed in (coord accounts)".into());
        }
        let change = format!("{name}: {}", if on { "on your accounts" } else { "through the gateway" });
        ok(match self.set_native(name, on, None) {
            When::NextStart => format!("{change} from its next start"),
            When::TurnEnd => format!("{change} when its turn ends (it restarts on its session; its background tasks end)"),
            When::Now => format!("{change}; restarted on its session (its background tasks ended)"),
            When::Already => format!("{change}; it runs through it already, as its account is out"),
        })
    }

    /// NAME, a Claude Code agent that can (`not_native`), runs on the owner's accounts from now on (ON) or through
    /// the model route again, for WHY (none: "onto your accounts" or "back to the gateway"): its registry line at
    /// once, and a session that runs is marked to restart on its conversation once not in a turn (`to_restart`), so no
    /// running turn is cut. One that does not run keeps WHY for the move its next start makes (`Agent::native_why`).
    /// When the change reaches its session.
    fn set_native(&mut self, name: &str, on: bool, why: Option<String>) -> When {
        let why = why.unwrap_or_else(|| if on { "onto your accounts" } else { "back to the gateway" }.into());
        crate::core::append(&self.path("registry.jsonl"), &json!({"ev": "native", "name": name, "on": on, "why": why, "at": now()}));
        // one that went on through the gateway while its account is out is where "off" puts it: nothing to restart
        let there = !on && self.agents[name].gateway.is_some();
        if !on {
            self.accounts.unbind(Harness::Claude, &self.coord_id(), name);
            self.account_record(name, None, &why, None);
        }
        let a = self.agents.get_mut(name).unwrap();
        a.native = on;
        a.account_wait = None;
        let running = a.running();
        let busy = a.live.state == "working" && a.ended_turn != a.live.turn;
        a.native_why = (on && !running).then(|| why.clone());
        if there {
            a.live.account_due = None;
        } else if running {
            a.live.account_due = Some(why);
        }
        self.changed(name);
        self.poke.notify_one();
        match (running, there, busy) {
            (false, ..) => When::NextStart,
            (true, true, _) => When::Already,
            (true, false, true) => When::TurnEnd,
            (true, false, false) => When::Now,
        }
    }

    /// The Accounts page's switch `All agents use my accounts` (`Request::AccountsAll`): every Claude Code agent of
    /// this coord runs on the owner's accounts from now on (ON), or through the gateway again, each from its own turn's end;
    /// new agents start there ([`Core::native_as_all`]).
    pub fn accounts_all(&mut self, who: &Who, on: bool) -> Reply {
        if !matches!(who, Who::Owner) {
            return err("all agents: the owner's".into());
        }
        if let Err(why) = self.accounts.ready() {
            return err(format!("all agents: {why}"));
        }
        // refused when it would leave every agent standing: with no account signed in, or with every account out and no
        // gateway. By itself it turns on only for an account that can take requests (`first_signed_in`).
        if on {
            let (accounts, now) = (self.accounts.list(Harness::Claude), now_s());
            let mut signed_in = accounts.iter().filter(|e| e.state.signed_in && e.state.leaving.is_none() && e.state.off.is_none()).peekable();
            if signed_in.peek().is_none() {
                return err("all agents: sign in to an account first".into());
            }
            if !self.gateway() && !accounts.iter().any(|e| e.state.can_take(now)) {
                return err(match signed_in.filter_map(|e| e.state.out_until).min() {
                    Some(t) if t != limits::UNKNOWN_RESET => format!("all agents: every account is out, the first until {}; no account can take requests now", crate::core::at_secs(t)),
                    _ => "all agents: every account is out; no account can take requests now".into(),
                });
            }
        }
        ok(self.all_agents(on, None))
    }

    /// The switch is set to ON, by the owner or, with BY_ACCOUNT, by itself ([`Core::all_by_itself`]): one registry
    /// line (`native_all`), then each agent it changes as [`Core::set_native`] changes one, with that agent's own
    /// line. So a replay of the registry gives the same switch and the same agents. An agent that cannot run on an
    /// account (boxed, another harness, a model that is not Anthropic's) stays. What was done, in words.
    fn all_agents(&mut self, on: bool, by_account: Option<String>) -> String {
        let at = now_s();
        crate::core::append(&self.path("registry.jsonl"), &json!({"ev": "native_all", "on": on, "by_account": by_account, "secs": at, "at": now()}));
        self.native_all = Some(coord_types::AllAgents { on, at, by_account });
        let mut names: Vec<String> = self.agents.values().filter(|a| a.kind == "claude" && a.ended.is_none() && a.native != on).map(|a| a.name.clone()).collect();
        names.sort();
        let (mut now, mut turn, mut next, mut there, mut stay) = (0, 0, 0, 0, 0);
        for name in names {
            if on && self.not_native(&name).is_some() {
                stay += 1;
                continue;
            }
            match self.set_native(&name, on, None) {
                When::Now => now += 1,
                When::TurnEnd => turn += 1,
                When::NextStart => next += 1,
                When::Already => there += 1,
            }
        }
        let parts = [
            (now, "moved"),
            (turn, "move when their turn ends"),
            (next, "do not run and start there"),
            (there, "run through it already, as their account is out"),
            (stay, "stay (boxed, or not on one of Anthropic's models)"),
        ];
        let said: Vec<String> = parts.iter().filter(|(n, _)| *n > 0).map(|(n, what)| format!("{n} {what}")).collect();
        let done = format!("all agents: on {}; {}", if on { "your accounts" } else { "the gateway" }, if said.is_empty() { "none to move".into() } else { said.join(", ") });
        eprintln!("coordd: {done}");
        done
    }

    /// The switch turns on by itself the first time this coordd sees an account that has signed in and answered its
    /// check (LABEL's; `main::count_usage` looks, off the lock, while the switch was never set), and never again: once
    /// there is a `native_all` line, by this or by the owner's click, only he changes it.
    ///
    /// It turns on by itself only where the coord has no gateway, where agents have no other way to run. With the
    /// gateway there, an account he adds moves nobody by itself, whatever its kind (0.2 list, line 22: "a sign-in
    /// never moves the team by itself"; on 20 Nov a sign-in moved 35 agents onto one seat whose agents then heard no
    /// message). That every agent goes onto his accounts is his to say, with the switch. KEY: the account runs on a
    /// key, as the journal line says.
    pub fn all_by_itself(&mut self, label: &str, key: bool) {
        if self.native_all.is_none() && !self.gateway() {
            let how = if key { "has its key set" } else { "has signed in" };
            eprintln!("coordd: all agents use the owner's accounts from now on: account {label} {how} (the Accounts page's switch turns it off)");
            self.all_agents(true, Some(label.to_string()));
        }
    }

    /// A new agent starts where the switch says: on the owner's accounts when it is on and NAME can run on one, with its
    /// own registry line. A project with no way but his accounts ([`Core::accounts_only`]) starts it there even before the
    /// switch has turned on by itself.
    pub fn native_as_all(&mut self, name: &str) {
        let on = match &self.native_all {
            Some(all) => all.on,
            None => self.accounts_only(),
        };
        if on && self.not_native(name).is_none() {
            crate::core::append(&self.path("registry.jsonl"), &json!({"ev": "native", "name": name, "on": true, "at": now()}));
            self.agents.get_mut(name).unwrap().native = true;
        }
    }

    /// The gateway's list as just read (ids SERVED, those it stopped naming less than `router::GONE_AFTER` ago
    /// included): each Claude Code agent on the model route whose model ([`router::base_id`]) the list no longer names goes
    /// onto the owner's accounts ([`Core::set_native`]), with a journal line and its row saying why. A failed read moves
    /// nobody. Only an agent that can run on an account (`not_native`), and only while one of his accounts is usable. It
    /// stays there when the gateway serves its model again.
    pub fn gateway_lost(&mut self, served: &[String]) {
        let mut lost: Vec<(String, String)> = self
            .agents
            .values()
            .filter(|a| a.kind == "claude" && a.ended.is_none() && !a.native)
            .map(|a| (a.name.clone(), model_effort(&a.model).0.to_string()))
            .filter(|(_, model)| !served.iter().any(|m| m == router::base_id(model)))
            .collect();
        if lost.is_empty() || !self.accounts.list(Harness::Claude).iter().any(|e| e.state.signed_in && e.state.off.is_none() && e.state.leaving.is_none()) {
            return;
        }
        lost.sort();
        for (name, model) in lost {
            if self.not_native(&name).is_some() {
                continue;
            }
            let why = format!("the gateway no longer serves {model}");
            eprintln!("coordd: {name}: onto your accounts: {why}");
            self.set_native(&name, true, Some(why));
        }
    }

    /// A project whose Claude Code agents can run only on the owner's accounts: it has its own Claude Code folder
    /// (`claude-home`) and no gateway. Both looked at, nothing read.
    pub fn accounts_only(&self) -> bool {
        !self.gateway() && std::path::Path::new(&self.path("claude-home")).is_dir()
    }

    /// This coord has the gateway: its sessions are given the route's key, by the test roster starts them with
    /// (`[ -s "$COORD/.proxy-key" ]`). The file is looked at, never read.
    pub fn gateway(&self) -> bool {
        std::fs::metadata(self.path(".proxy-key")).is_ok_and(|m| m.is_file() && m.len() > 0)
    }

    /// The agents that went on through the gateway ([`OnGateway`]) whose time to look has come: each is asked of the
    /// placement policy as its start would ask it, with nothing taken (no turn, no binding). One the policy would put
    /// on an account is marked to restart once not in a turn, where its start makes the choice; for the others
    /// nothing is restarted and the next look is at [`router::look_again`] (none named: at the owner's clear,
    /// [`Core::account_cleared`]). So it does not go back and forth: it leaves the gateway only for an account the
    /// policy gives it, one that can take requests by its own state and has space.
    pub fn gateway_back(&mut self) {
        let now_s = now_s();
        let due_now: Vec<String> = self.agents.values().filter(|a| a.running() && a.live.account_due.is_none() && a.gateway.as_ref().is_some_and(|g| g.back <= now_s)).map(|a| a.name.clone()).collect();
        if due_now.is_empty() {
            return;
        }
        // the accounts' own files, a few small ones, and the policy's, read under the core's lock as `account_back`
        // reads the accounts'
        let h = Harness::Claude;
        let (accounts, turns, policy) = (self.accounts.list(h), router::turns(&self.accounts, h), self.places().sources());
        let next = router::look_again(&accounts, now_s);
        let (mut back, mut stay) = (false, false);
        for name in due_now {
            let model = model_effort(&self.agents[&name].model).0.to_string();
            let to = match &policy {
                Ok(p) => router::choose(&accounts, &mut turns.clone(), p, router::gateway_serves(&self.coord, &model), &name, &model, now_s),
                Err(e) => Err(e.clone()),
            };
            if let Err(e) = &to {
                eprintln!("coordd: {name}: stays on the gateway, as the placement policy could not be asked: {e}");
            }
            let a = self.agents.get_mut(&name).unwrap();
            if matches!(to, Ok(router::Choice::Account(_))) {
                a.live.account_due = Some("an account can take it again".into());
                back = true;
            } else if let Some(g) = a.gateway.as_mut() {
                g.back = next;
                stay = true;
            }
            self.changed(&name);
        }
        if back {
            self.poke.notify_one();
        }
        if stay {
            self.wake_at(next);
        }
    }

    /// The choice of an account for NAME, made as HOW says, for its model.
    fn choosing(&self, name: &str, how: How) -> Choosing {
        let model = self.agents.get(name).map(|a| model_effort(&a.model).0.to_string()).unwrap_or_default();
        Choosing { store: self.accounts.clone(), coord: self.coord_id(), name: name.to_string(), how, harness: Harness::Claude, provider: None, places: self.places(), gateway: self.serves(&model), model, down_cut: None }
    }

    /// The gateway is a place for MODEL (no `:EFFORT`): this coord has it ([`Core::gateway`]) and it serves MODEL
    /// ([`router::gateway_serves`]).
    fn serves(&self, model: &str) -> bool {
        self.gateway() && router::gateway_serves(&self.coord, model)
    }

    /// `coordc account-for NAME` (roster, at each start of a Claude Code agent), its first part, under the core's lock:
    /// whether NAME runs on an account at all. An Err is the whole answer; an Ok is the choice to make off the lock
    /// ([`Choosing::run`]), whose result [`Core::account_for_end`] takes.
    /// DOWN_CUT: when the end of NAME's last process cut a turn (`follow::cut_at`): its start goes on with that turn in
    /// Claude Code's own way ([`resume_env`]), on the model route as on an account.
    pub fn account_for_begin(&self, who: &Who, name: &str, down_cut: Option<i64>) -> Result<Choosing, Reply> {
        if !self.above(who, name) && who.name() != name {
            return Err(err(format!("coord account: only the agents above {name} start it")));
        }
        let Some(a) = self.agents.get(name) else { return Err(err(format!("coord account: no agent named {name}"))) };
        if a.kind == "opencode" {
            return self.opencode_for_begin(name, &a.model);
        }
        if a.kind == "agy" {
            return self.agy_for_begin(name);
        }
        if !a.native {
            // the model route: no folder, then the lines that go on with a turn the end of its process cut, if one did
            let Some(at) = down_cut else { return Err(ok(String::new())) };
            eprintln!("coordd: {name}: started to go on with the turn the end of its last process cut {} s ago", now_s() - at);
            return Err(ok(std::iter::once(String::new()).chain(resume_env(Some(at), now_s())).collect::<Vec<_>>().join("\n")));
        }
        if let Some(why) = self.not_native(name) {
            return Err(err(format!("coord account: {why}")));
        }
        Ok(Choosing { down_cut, ..self.choosing(name, How::Next { leave: false }) })
    }

    /// `account-for` of opencode agent NAME on MODEL (`PROVIDER/MODEL`): chosen among the owner's opencode accounts of
    /// PROVIDER alone, never a Claude Code account or the gateway. Not in a boxed project.
    fn opencode_for_begin(&self, name: &str, model: &str) -> Result<Choosing, Reply> {
        let Some((provider, _)) = model.split_once('/') else { return Err(err(format!("coord account: {name}'s model {model:?} names no provider (PROVIDER/MODEL)"))) };
        if let Some(why) = super::opencode::bad_provider(provider) {
            return Err(err(format!("coord account: {name}'s model: {why}")));
        }
        if self.project.as_ref().is_some_and(|p| p.project.boxed) {
            return Err(err("coord account: a boxed project's agents do not run on your accounts yet (an agent there is its own user, and an account's folder is yours)".into()));
        }
        Ok(Choosing { harness: Harness::Opencode, provider: Some(provider.to_string()), gateway: false, ..self.choosing(name, How::Next { leave: false }) })
    }

    /// `account-for` of agy agent NAME: chosen among the owner's agy accounts (sign-in and key alike), never the gateway
    /// or another harness's account. Not in a boxed project.
    fn agy_for_begin(&self, name: &str) -> Result<Choosing, Reply> {
        if self.project.as_ref().is_some_and(|p| p.project.boxed) {
            return Err(err("coord account: a boxed project's agents do not run on your accounts yet (an agent there is its own user, and an account's folder is yours)".into()));
        }
        Ok(Choosing { harness: Harness::Agy, gateway: false, ..self.choosing(name, How::Next { leave: false }) })
    }

    /// `account-for`'s second part, under the core's lock again: the folder NAME's session runs with, the choice
    /// recorded. With every account out the reply is an error that says until when; the agent is marked as waiting,
    /// and coordd tries again then. An agent that was taken off the owner's accounts while the choice was made goes
    /// through the model route, and the binding just written is taken back.
    pub fn account_for_end(&mut self, chosen: Chosen) -> Reply {
        if chosen.harness != Harness::Claude {
            return self.bound_for_end(chosen);
        }
        let (h, now_s, name) = (Harness::Claude, now_s(), chosen.name.as_str());
        // a turn its account's limit cut, else one the end of its last process cut (`account_for_begin`)
        let down_cut = chosen.down_cut;
        let Some(a) = self.agents.get(name) else { return err(format!("coord account: no agent named {name}")) };
        if !a.native {
            self.accounts.unbind(h, &self.coord_id(), name);
            return ok(String::new());
        }
        self.agents.get_mut(name).unwrap().account_none = false;
        let gateway = self.serves(model_effort(&self.agents[name].model).0);
        // with the gateway, a policy that left the gateway out of the places (a project's) is taken as the system's
        // own leaves it: every account out, or none signed in while NAME's own is being removed or it runs through
        // the gateway already, is the gateway, and NAME does not stand still. A gateway gone while the choice was
        // made is every account out.
        let choice = match chosen.choice {
            Ok(router::Choice::None) if gateway && (chosen.leaving.is_some() || self.agents[name].gateway.is_some()) => Ok(router::Choice::Gateway { back: limits::UNKNOWN_RESET, why: None }),
            Ok(router::Choice::AllOut { until }) if gateway => Ok(router::Choice::Gateway { back: until, why: None }),
            Ok(router::Choice::Gateway { back, .. }) if !gateway => Ok(router::Choice::AllOut { until: back }),
            choice => choice,
        };
        match choice {
            Err(why) => err(format!("coord account: {why}")),
            // it waits, says so (`account_note`), and starts when an account can take it (`accounts_seen`). One whose account
            // is being removed stays bound to it meanwhile, so the account is not removed under an agent with nowhere to go
            Ok(router::Choice::None) => {
                let a = self.agents.get_mut(name).unwrap();
                (a.account_wait, a.account_none) = (Some(limits::UNKNOWN_RESET), true);
                self.changed(name);
                if self.accounts_only() {
                    err(format!("coord account: {NO_ACCOUNT_YET}; {name} is started when you do"))
                } else {
                    err(format!("coord account: no account can take {name}; it is started when you sign in to one"))
                }
            }
            // the policy put NAME on the gateway (every account is out, or none has space): it goes on through it,
            // bound to none, and returns when the policy would put it on an account (`gateway_back`); it does not
            // stand still
            Ok(router::Choice::Gateway { back, why }) => {
                if self.agents[name].gateway.is_some() {
                    self.agents.get_mut(name).unwrap().gateway.as_mut().unwrap().back = back;
                } else {
                    // what its session did on the account it leaves is read first, so those rows carry that account
                    self.usage_read(name);
                    self.accounts.unbind(h, &self.coord_id(), name);
                    let why = match (chosen.leaving, why) {
                        (Some(leaving), _) => format!("{leaving} and no other can take it now"),
                        (None, Some(why)) => why,
                        (None, None) => format!("every account is out {}", until(back)),
                    };
                    self.account_record(name, None, &format!("{why}; through the gateway meanwhile"), Some(back));
                }
                // one of them may be a key account out for its rate limit: the policy's look is due
                if back == limits::UNKNOWN_RESET {
                    self.key_again = KeyAgain::look();
                }
                let a = self.agents.get_mut(name).unwrap();
                a.account_wait = None;
                let cut = a.live.account_cut;
                if let Some(at) = cut {
                    eprintln!("coordd: {name}: started through the gateway to go on with the turn its account's limit cut {} s ago", now_s - at);
                    a.account_resumed = true;
                } else if let Some(at) = down_cut {
                    eprintln!("coordd: {name}: started through the gateway to go on with the turn the end of its last process cut {} s ago", now_s - at);
                }
                let cut = cut.or(down_cut);
                self.changed(name);
                self.wake_at(back);
                // no folder, which is the model route's start; then the lines that go on with a cut turn, or keep
                // one from being run again
                ok(std::iter::once(String::new()).chain(resume_env(cut, now_s)).collect::<Vec<_>>().join("\n"))
            }
            Ok(router::Choice::AllOut { until }) => {
                self.agents.get_mut(name).unwrap().account_wait = Some(until);
                self.changed(name);
                self.wake_at(until);
                // one of them may be a key account another coordd's session put out: the policy's look is due
                if until == limits::UNKNOWN_RESET {
                    self.key_again = KeyAgain::look();
                }
                err(if until == limits::UNKNOWN_RESET {
                    format!("coord account: every account is out and none says until when; {name} is started when one is back (coord account clear ID)")
                } else {
                    format!("coord account: every account is out; the first is back at {}, and {name} is started then", crate::core::at_secs(until))
                })
            }
            Ok(router::Choice::Account(id)) => {
                // a key account whose key file went since it was set takes no agent (`Choosing::run` saw it and wrote
                // it): said, and the start is tried again at the next round, when the choice passes it over
                if chosen.no_key {
                    eprintln!("coordd: account {id}: its key file is gone or not the owner's alone; it takes no agent until a key is set (coord account key {id})");
                    let again = now_s + 1;
                    self.agents.get_mut(name).unwrap().account_wait = Some(again);
                    self.wake_at(again);
                    return err(format!("coord account: account {id} has no key any more (coord account key {id}); {name} is tried on another"));
                }
                // two choices for one agent made side by side: the store holds the later binding, which is the agent's account
                let id = self.accounts.bound_to(h, &self.coord_id(), name).map_or(id, |e| e.id);
                let a = &self.agents[name];
                let why = a.live.account_due.clone().or_else(|| a.native_why.clone()).unwrap_or_else(|| "started".into());
                if a.account.as_deref() != Some(id.as_str()) {
                    // what its session did on the account it leaves is read first, so those rows carry that account
                    self.usage_read(name);
                    self.account_record(name, Some(&id), &why, None);
                }
                let a = self.agents.get_mut(name).unwrap();
                (a.account_wait, a.native_why) = (None, None);
                self.account_session(name);
                self.changed(name);
                // the folder, then the session's environment: NAME=VALUE to set, -NAME to remove
                let mut out = vec![self.accounts.config(h, &id).display().to_string()];
                let cut = self.agents[name].live.account_cut;
                out.extend(resume_env(cut.or(down_cut), now_s));
                if let Some(at) = cut {
                    eprintln!("coordd: {name}: started to go on with the turn its account's limit cut {} s ago", now_s - at);
                    self.agents.get_mut(name).unwrap().account_resumed = true;
                } else if let Some(at) = down_cut {
                    eprintln!("coordd: {name}: started to go on with the turn the end of its last process cut {} s ago", now_s - at);
                }
                if let Some(port) = self.telemetry_port {
                    out.extend(telemetry::env(port).into_iter().map(|(k, v)| format!("{k}={v}")));
                    out.extend(telemetry::UNSET.iter().map(|k| format!("-{k}")));
                }
                // a key account: where its key file is, for the session's `apiKeyHelper` (roster); a path, never the key
                if self.accounts.get(h, &id).is_some_and(|e| e.account.kind == super::Kind::Key) {
                    out.push(format!("@key {}", self.accounts.key_file(h, &id).display()));
                }
                ok(out.join("\n"))
            }
        }
    }

    /// `account-bound NAME`: the agy account agy agent NAME is bound to, in `account-for`'s lines (its folder, its
    /// session's environment, a key account's `@key PATH`), nothing chosen, bound or moved: what cutover.sh carries to
    /// coordd-v2, so NAME runs there on the account it runs on here. Paths only, never a key. The owner's only.
    pub fn account_bound(&self, who: &Who, name: &str) -> Reply {
        let h = super::Harness::Agy;
        if !matches!(who, Who::Owner) {
            return err("coord account-bound: the owner's".into());
        }
        match self.agents.get(name) {
            None => return err(format!("coord account-bound: no agent named {name}")),
            Some(a) if a.kind != "agy" => return err(format!("coord account-bound: {name} is {} agent, not an agy one", crate::core::kind_article(&a.kind))),
            Some(_) => {}
        }
        let Some(e) = self.accounts.bound_to(h, &self.coord_id(), name) else { return err(format!("coord account-bound: no agy account is bound to {name}")) };
        let mut out = vec![self.accounts.dir(h, &e.id).display().to_string()];
        out.extend(self.accounts.session_env(h, &e.id).into_iter().map(|(k, v)| format!("{k}={v}")));
        if self.accounts.kind(h, &e.id) == Some(super::Kind::Key) {
            out.push(format!("@key {}", self.accounts.key_file(h, &e.id).display()));
        }
        ok(out.join("\n"))
    }

    /// `account-for`'s second part for an opencode or agy agent: the account's folder, then its session's
    /// environment ([`Store::session_env`]: for opencode its XDG folders, where opencode keeps the sign-in, and TMPDIR
    /// under the account's `home/`, and the three `OPENCODE_DISABLE_*`, which roster sets over the agent's own
    /// folders; for an agy sign-in account its `AGY_SIGNIN`, or `T3_HOME`), and for an agy key account `@key PATH`,
    /// where its key file is (a path, never the key). For opencode, nothing when no account of its provider can take
    /// it: it runs on folders of its own with no sign-in. An agy agent has no way to Google but an account: with none that can take
    /// it the answer is an error that says so, and roster does not start it. No registry line and no move: such an
    /// agent keeps its session, and with it the account it started on, over a restart, so the binding in the store,
    /// which the account's card shows and which holds off its removal, is the record of it.
    fn bound_for_end(&mut self, chosen: Chosen) -> Reply {
        let (h, name) = (chosen.harness, chosen.name.as_str());
        let of = match (h, chosen.provider.as_deref()) {
            (Harness::Opencode, Some(p)) => format!("opencode account of {p}"),
            _ => format!("{} account", h.shown()),
        };
        if !self.agents.contains_key(name) {
            self.accounts.unbind(h, &self.coord_id(), name);
            return err(format!("coord account: no agent named {name}"));
        }
        match chosen.choice {
            Err(why) => err(format!("coord account: {why}")),
            Ok(router::Choice::Account(id)) => {
                eprintln!("coordd: {name}: on {of} {id}");
                let mut out = vec![self.accounts.dir(h, &id).display().to_string()];
                out.extend(self.accounts.session_env(h, &id).into_iter().map(|(k, v)| format!("{k}={v}")));
                if h == Harness::Agy && self.accounts.kind(h, &id) == Some(super::Kind::Key) {
                    out.push(format!("@key {}", self.accounts.key_file(h, &id).display()));
                }
                ok(out.join("\n"))
            }
            // the gateway is never chosen for one (its choice is made with `gateway: false`): it is no account either
            Ok(router::Choice::None | router::Choice::AllOut { .. } | router::Choice::Gateway { .. }) if h == Harness::Agy => {
                err(format!("coord account: no {of} can take {name} now (signed in, with its key set, not off, not out)"))
            }
            Ok(router::Choice::None | router::Choice::AllOut { .. } | router::Choice::Gateway { .. }) => {
                eprintln!("coordd: {name}: no {of} can take it; it runs with no sign-in");
                ok(String::new())
            }
        }
    }

    /// NAME's account from now on (None: the model route), in the registry, with where it was and why: the record of
    /// a move. With GATEWAY (TO is None then) it is still the owner's accounts' and runs through the gateway until one
    /// can take it, looked at again at that time (seconds).
    fn account_record(&mut self, name: &str, to: Option<&str>, why: &str, gateway: Option<i64>) {
        let Some(a) = self.agents.get_mut(name) else { return };
        let from = std::mem::replace(&mut a.account, to.map(String::from));
        eprintln!("coordd: {name}: account {} → {} ({why})", from.as_deref().unwrap_or("none"), to.unwrap_or(if gateway.is_some() { "the gateway" } else { "none" }));
        let at = now();
        let mut line = json!({"ev": "account", "name": name, "account": to, "from": from, "why": why, "at": at.clone()});
        a.account_move = Some(AccountMove { why: why.to_string(), since: at, from: from.clone() });
        a.gateway = gateway.map(|back| {
            // `left`, the account it goes back to look for: what a load of the registry knows it by
            line["left"] = json!(from);
            OnGateway { left: from.clone(), since: now(), back }
        });
        crate::core::append(&self.path("registry.jsonl"), &line);
    }

    /// A hook event of NAME's session that says something of the account it runs on (limits.rs: the status line's
    /// windows, a turn that failed on a limit, Claude Code's quota notices): kept in the account's state, and the
    /// rule is asked for the agents on it. A status line that says what is on file already is not written again
    /// within a minute.
    ///
    /// The account is the one whose folder the session runs under (`claude_config_dir`, which coordc adds from the
    /// hook's environment), not the one NAME is on now: the status line is handed over without being waited for, so
    /// an old session's can arrive after its agent was given another account, and is still of the one it left. An
    /// event that names no account's folder says nothing of one.
    ///
    /// Nothing is written here, under the core's lock: what is to be written is the [`Keep`] this gives, for
    /// [`Core::account_keep`].
    pub fn account_said(&mut self, name: &str, event: &str, input: &serde_json::Value) -> Option<Keep> {
        if let Some(h) = self.agents.get(name).and_then(|a| Harness::named(&a.kind)).filter(|h| *h != Harness::Claude) {
            return self.bound_said(name, h, event, input);
        }
        let (h, now) = (Harness::Claude, now_s());
        if !self.agents.get(name).is_some_and(|a| a.native) {
            return None;
        }
        let id = input.get("claude_config_dir").and_then(|d| d.as_str()).and_then(|d| self.accounts.of_config(h, std::path::Path::new(d)))?;
        // what is on file is read without the account's lock (one small file, two for a failed turn, whose meaning
        // depends on the account's kind); only a change is written
        let state = self.accounts.state(h, &id)?;
        let key = matches!(event, "failed" | "limit_row") && self.accounts.kind(h, &id) == Some(super::Kind::Key);
        let said = match event {
            "statusline" => limits::from_statusline(input),
            "failed" => limits::from_failed(input, key, name),
            "stop" => limits::Said::answered(),
            "notification" => limits::from_notification(input),
            // coordd's own: the reset a failed turn's transcript row names (`Job::LimitRow`)
            "limit_row" => limits::from_row(input, key),
            _ => return None,
        };
        // a turn that failed for its account (its limit; a key account's refused key or missing credit) and not for
        // the work was cut: its agent's next start goes on with it (`account_for`). The 402 that only the turn's
        // row names is such a one too, known when the row is read.
        let out = matches!(said.verdict, limits::Verdict::Out(_));
        if out && (event == "failed" || said.out_for == Some(limits::NO_CREDIT) && self.agents[name].live.account_cut.is_none()) {
            self.agents.get_mut(name).unwrap().live.account_cut = Some(now);
            self.changed(name);
        }
        // a sign-in's login refused this turn: the turn was cut by its account, and the agent restarts to go on with
        // it, on this account while it can take it (a session's login may be stale where the account's is not);
        // whether the account is still signed in is Claude Code's to say, asked off the lock
        let refused = matches!(said.session, Some(limits::Session::Refused(_)));
        if refused {
            let a = self.agents.get_mut(name).unwrap();
            a.live.account_cut = Some(now);
            if a.running() && a.live.account_due.is_none() {
                a.live.account_due = Some(REFUSED_TURN.into());
            }
            self.changed(name);
            self.poke.notify_one();
            if let Some(jobs) = &self.jobs {
                let _ = jobs.send(crate::core::Job::AccountCheck { id: id.clone() });
            }
        }
        // the hook names no time, and for a key account's 402 no reason (`unknown`); the turn's row in the transcript
        // does: read off the lock, handed back here as `limit_row` under the same folder
        if event == "failed" && (out || key && input["error"] == "unknown") {
            let since = crate::transcript::epoch_ms(&self.agents[name].live.started);
            if let (Some(jobs), Some(transcript), Some(since)) = (&self.jobs, input.get("transcript_path").and_then(|t| t.as_str()), since) {
                let _ = jobs.send(crate::core::Job::LimitRow { name: name.to_string(), transcript: transcript.to_string(), config: input["claude_config_dir"].clone(), since });
            }
        }
        // a sign-in's turn met its limit: the resets Claude Code offers there are in the notice on the agent's screen
        if event == "failed" && !key && said.out_for == Some(limits::RATE_LIMIT) {
            if let Some(jobs) = &self.jobs {
                let _ = jobs.send(crate::core::Job::ResetOffers { name: name.to_string(), id: id.clone() });
            }
        }
        let mut next = state.clone();
        limits::apply(&mut next, &said, now);
        if next == state || (State { limits_seen: state.limits_seen, ..next.clone() } == state && now - state.limits_seen < 60) {
            return None;
        }
        // said in the journal once it is kept: the reset a turn's row named; why a key account went out
        let note = match (event, said.out_for) {
            ("limit_row", Some(limits::RATE_LIMIT)) => {
                Some(format!("{name}: account {id} is out until {} ({} limit, from the turn's row in its transcript)", crate::core::at_secs(input["resets_at"].as_i64().unwrap_or_default()), input["window"].as_str().unwrap_or("its")))
            }
            (_, Some(why)) if key && limits::owners_to_clear(why) => {
                Some(format!("{name}: account {id} is out for {why} ({}); it does not come back by itself: the owner clears it, or sets a new key", if why == limits::REFUSED { "its key was refused" } else { "no credit" }))
            }
            (_, Some(why)) if key => Some(format!("{name}: account {id} is out for {why} with no time named; the policy's key-again-after says when it is taken again")),
            _ if refused && next.out_for.as_deref() == Some(limits::REFUSED) => Some(format!(
                "{name}: account {id} is out: its login was refused to a second turn, with none answered on it between; it does not come back by itself: the owner signs it in again or clears it"
            )),
            _ if refused => Some(format!("{name}: account {id} refused its turn's login; {name} restarts, and Claude Code is asked whether the account is signed in")),
            _ => None,
        };
        Some(Keep { name: name.to_string(), id, h, said, now, note, again: None })
    }

    /// [`Core::account_said`] for opencode or agy (H) agent NAME: a turn that ended on an error (`failed`) says something
    /// of its bound account. These harnesses hold the sign-in or key themselves, so their accounts are read as key accounts
    /// ([`limits::from_failed`] with KEY): a refused sign-in or key, or no credit, is out until the owner acts; the
    /// provider's limit until the harness's time, else the policy's `key-again-after`. Other errors say nothing. An agy
    /// limit puts the account out for the agent's model alone ([`super::State::models_out`]). Such an agent is not moved
    /// while it runs; its next start keeps the account only if it can take its model.
    ///
    /// Any turn's end on an OpenCode Go account whose key coordd keeps has Go's usage read again off the lock
    /// (`Job::GoUsage`), one read at a time ([`Core::go_read_done`]).
    fn bound_said(&mut self, name: &str, h: Harness, event: &str, input: &serde_json::Value) -> Option<Keep> {
        let now = now_s();
        if !matches!(event, "stop" | "failed") {
            return None;
        }
        // read without the account's lock, as the Claude path reads its own (one small file for each opencode
        // account, two for the bound one); only a change is written
        let e = self.accounts.bound_to(h, &self.coord_id(), name)?;
        if e.account.provider.as_deref() == Some(opencode::GO) && self.accounts.usage_key_set(&e.id) {
            if let Some(jobs) = &self.jobs {
                match self.go_reading.get_mut(&e.id) {
                    Some(again) => *again = true,
                    None => {
                        self.go_reading.insert(e.id.clone(), false);
                        let _ = jobs.send(crate::core::Job::GoUsage { id: e.id.clone() });
                    }
                }
            }
        }
        if event != "failed" {
            return None;
        }
        let words = input.get("error_details").and_then(|w| w.as_str()).map(str::trim).filter(|w| !w.is_empty()).map(String::from);
        let failed = limits::from_failed(input, true, name);
        let model = (h == Harness::Agy && failed.out_for == Some(limits::RATE_LIMIT)).then(|| model_effort(&self.agents[name].model).0.to_string());
        let said = limits::Said { words, model, ..failed };
        let why = said.out_for?;
        let mut next = e.state.clone();
        limits::apply(&mut next, &said, now);
        if next == e.state {
            return None;
        }
        let id = e.id;
        let note = match (why, &said.verdict, said.model.as_deref()) {
            (limits::RATE_LIMIT, _, Some(m)) => {
                let when = match (next.model_out(m, now), &said.verdict) {
                    (Some(limits::UNKNOWN_RESET) | None, _) => "with no time named and no window of it running; the policy's key-again-after says when it takes it again".to_string(),
                    (Some(t), limits::Verdict::Out(Some(_))) => format!("until {} (the time is {}'s, from the provider's answer)", crate::core::at_secs(t), h.shown()),
                    (Some(t), _) => format!("until {} (no time named: its windows' reset)", crate::core::at_secs(t)),
                };
                let all = match next.models_all_out(now) {
                    Some(t) => format!("; every model it offers is out, so it takes no agent {}", until(t)),
                    None => "; its other models still take agents".to_string(),
                };
                format!("{name}: account {id} is out for {m} {when}{all}")
            }
            (limits::RATE_LIMIT, limits::Verdict::Out(Some(until)), None) => format!("{name}: account {id} is out until {} (its provider's limit; the time is {}'s, from the provider's answer)", crate::core::at_secs(*until), h.shown()),
            (limits::RATE_LIMIT, _, None) => format!("{name}: account {id} is out for {why} with no time named; the policy's key-again-after says when it is taken again"),
            _ => format!("{name}: account {id} is out for {why} ({}); it does not come back by itself: the owner signs it in again, or clears it", if why == limits::REFUSED { "its provider refused its sign-in or key" } else { "no credit" }),
        };
        Some(Keep { name: name.to_string(), id, h, said, now, note: Some(note), again: None })
    }

    /// K is to be written: by the task that does the store's writes in their order (`Job::AccountSaid`), which then
    /// gives the result to [`Core::account_kept`]; in place where no such task runs (a core without its senders,
    /// as a test's).
    pub fn account_keep(&mut self, k: Keep) {
        match self.jobs.clone() {
            Some(jobs) => drop(jobs.send(crate::core::Job::AccountSaid(k))),
            None => {
                let kept = k.run(&self.accounts);
                self.account_kept(&k, kept);
            }
        }
    }

    /// K's write is over: the rule is asked for the agents on its account, and who waits with no time known is tried
    /// again; or the journal says why it was not kept.
    pub fn account_kept(&mut self, k: &Keep, kept: Result<bool, String>) {
        match kept {
            // the account was no longer as the policy's word found it: what stands on file stands
            Ok(false) => {}
            Ok(true) => {
                if let Some(note) = &k.note {
                    eprintln!("coordd: {note}");
                }
                // an account went out with no time named: the policy may say when it is taken again (`key_again`)
                if k.said.out_for == Some(limits::RATE_LIMIT) && k.said.verdict == limits::Verdict::Out(None) {
                    self.key_again = KeyAgain::look();
                }
                self.account_seen(&k.id);
                // something new is known of an account: who waits with no time known is tried again, and waits
                // until the time now known, or goes on
                let unknown: Vec<String> = self.agents.values().filter(|a| a.account_wait == Some(limits::UNKNOWN_RESET)).map(|a| a.name.clone()).collect();
                for waiting in unknown {
                    self.agents.get_mut(&waiting).unwrap().account_wait = Some(k.now);
                    self.changed(&waiting);
                }
                self.poke.notify_one();
            }
            Err(why) => match k.again {
                // looked at again after the policy's time once more, not at every round
                Some(secs) => {
                    eprintln!("coordd: account {}: not taken again by the policy: {why}", k.id);
                    let at = now_s().saturating_add(secs as i64);
                    self.key_again = KeyAgain { at: Some(at), secs: Some(secs) };
                    self.wake_at(at);
                }
                None => eprintln!("coordd: {}: what its session said of account {}'s limits was not kept: {why}", k.name, k.id),
            },
        }
    }

    /// The store, for a change of an account's own files that only the owner makes (`coord account WHAT`): made off
    /// the core's lock by who asks (main.rs `account_store`), as the store's lock is waited for.
    pub fn account_owners(&self, who: &Who, what: &str) -> Result<Store, Reply> {
        match who {
            Who::Owner => Ok(self.accounts.clone()),
            _ => Err(err(format!("coord account {what}: the owner's"))),
        }
    }

    /// An account can take requests again (`coord account clear ID`, or its key was set; written off the core's
    /// lock: main.rs): the agents that wait for an account are tried again now, not at the time they waited for.
    pub fn account_cleared(&mut self) {
        let now_s = now_s();
        let waiting: Vec<String> = self.agents.values().filter(|a| a.account_wait.is_some()).map(|a| a.name.clone()).collect();
        for name in waiting {
            self.agents.get_mut(&name).unwrap().account_wait = Some(now_s);
            self.changed(&name);
        }
        self.gateway_look();
        self.poke.notify_one();
    }

    /// Account ID cleared to take requests (his press `It can take requests`, `coord account clear ID`, or a reset of
    /// his): as [`Core::account_cleared`], and the agents held on a rate limit on it go on now (`Core::served_again`).
    pub fn owner_cleared(&mut self, id: &str, how: &str) {
        self.account_cleared();
        self.served_again(id, crate::core::Served::Every, now_s(), &format!("{how} on account {id} at {}", crate::core::at_secs(now_s())));
    }

    /// Something new is known of an account: the agents that went on through the gateway are looked at at the next
    /// round ([`Core::gateway_back`]), not only at the time they went with.
    fn gateway_look(&mut self) {
        for g in self.agents.values_mut().filter_map(|a| a.gateway.as_mut()) {
            g.back = 0;
        }
    }

    /// NAME's session runs on NAME's account, for the telemetry receiver (`Sessions`).
    pub fn account_session(&self, name: &str) {
        let Some(a) = self.agents.get(name).filter(|a| a.native) else { return };
        if let (Some(account), Some(session), Ok(mut m)) = (a.account.clone(), a.session.get("session_id").and_then(|s| s.as_str()), self.account_sessions.lock()) {
            m.insert(session.to_string(), (name.to_string(), account));
        }
    }

    /// The usage rows of what NAME's session did since the last read (usage.rs), on the account it runs on now: read
    /// off the core's lock, in a task of its own. An opencode agent's account is the one it is bound to, which it
    /// keeps over its restarts ([`Core::bound_for_end`]), looked up in that task; its follower writes each request
    /// as a Codex rollout's `token_count` (opencode-rpc `step`). A Claude Code agent through the gateway counts on no
    /// account, as the gateway's; the gateway's own records are never read. A Codex agent has no rows. Nothing
    /// without a runtime (a test's core).
    fn usage_read(&self, name: &str) {
        let harness = self.agents.get(name).and_then(|a| Harness::named(&a.kind));
        let bound = harness.filter(|h| *h != Harness::Claude);
        let Some(a) = self.agents.get(name).filter(|a| a.native || bound.is_some() || harness == Some(Harness::Claude)) else { return };
        let source = if a.native || bound.is_some() { usage_rollup::Source::Native } else { usage_rollup::Source::Gateway };
        let (Some(session), Some(transcript)) = (a.session.get("session_id").and_then(|s| s.as_str()).map(String::from), a.transcript()) else { return };
        let Ok(rt) = tokio::runtime::Handle::try_current() else { return };
        let account = if source == usage_rollup::Source::Gateway { Some(String::new()) } else { a.account.clone() };
        let (coord, coord_id, store, name) = (self.coord.clone(), self.coord_id(), self.accounts.clone(), name.to_string());
        rt.spawn_blocking(move || {
            let account = match bound {
                Some(h) => store.bound_to(h, &coord_id, &name).map(|e| e.id),
                None => account,
            };
            let Some(account) = account else { return };
            if let Err(why) = usage::turn_ended(&coord, &name, &session, &transcript, &account, source) {
                eprintln!("coordd: {name}: its usage rows were not written: {why}");
            }
        });
    }

    /// A read of Go account ID's usage after a turn's end (`Job::GoUsage`) is done: true when a turn's end during it
    /// asked for one more, which is then the read in flight; else no read is in flight, and the next turn's end
    /// starts one ([`Core::opencode_said`]).
    pub fn go_read_done(&mut self, id: &str) -> bool {
        match self.go_reading.get_mut(id) {
            Some(again) if *again => {
                *again = false;
                true
            }
            _ => {
                self.go_reading.remove(id);
                false
            }
        }
    }

    /// NAME's turn ended: its usage rows are read, and the rule is asked of its account's state.
    pub fn account_turn_ended(&mut self, name: &str) {
        self.usage_read(name);
        if !self.agents.get(name).is_some_and(|a| a.native && a.running() && a.live.account_due.is_none()) {
            return;
        }
        // the state of the account its session runs on: one small file, read under the core's lock
        let Some(state) = self.agents[name].account.as_deref().and_then(|id| self.accounts.state(Harness::Claude, id)) else { return };
        if let Some(why) = due(&state, now_s()) {
            self.agents.get_mut(name).unwrap().live.account_due = Some(why);
            self.changed(name);
            self.poke.notify_one();
        }
    }

    /// Something new was written of account ID's state (`limits.rs`): the rule is asked for each of this coord's
    /// agents on it. An agent in a turn is only marked; it moves when the turn has ended.
    pub fn account_seen(&mut self, id: &str) {
        self.gateway_look();
        let Some(state) = self.accounts.state(Harness::Claude, id) else { return };
        let Some(why) = due(&state, now_s()) else { return };
        // this coord's agents whose session runs on it
        let names: Vec<String> = self.agents.values().filter(|a| a.account.as_deref() == Some(id)).map(|a| a.name.clone()).collect();
        for name in names {
            if let Some(a) = self.agents.get_mut(&name).filter(|a| a.native && a.running() && a.live.account_due.is_none()) {
                a.live.account_due = Some(why.clone());
                self.changed(&name);
            }
        }
        self.poke.notify_one();
    }

    /// `coord account move NAME [ACCOUNT]`, its first part, under the core's lock: whether the owner may move NAME.
    /// An Err is the whole answer; an Ok is the new binding to make off the lock ([`Choosing::run`]: ACCOUNT, or the
    /// next by tier and turn, leaving the one it is on), which [`Core::move_end`] takes.
    pub fn move_begin(&self, who: &Who, name: &str, to: Option<&str>) -> Result<Choosing, Reply> {
        if !matches!(who, Who::Owner) {
            return Err(err("coord account move: the owner's".into()));
        }
        let Some(a) = self.agents.get(name) else { return Err(err(format!("coord account move: no agent named {name}"))) };
        if a.kind == "opencode" || a.kind == "agy" {
            return Err(err(format!("coord account move: {name} is {} agent: it stays on its account while its session runs", crate::core::kind_article(&a.kind))));
        }
        if !a.native {
            return Err(err(format!("coord account move: {name} goes through the gateway (coord native {name} on)")));
        }
        let how = match to {
            Some(id) => How::To(id.to_string()),
            None => How::Next { leave: true },
        };
        Ok(self.choosing(name, how))
    }

    /// The move's second part, under the core's lock again: the new binding is made; the session follows once not
    /// in a turn.
    pub fn move_end(&mut self, chosen: Chosen) -> Reply {
        let name = chosen.name.as_str();
        let id = match chosen.choice {
            Ok(router::Choice::Account(id)) => id,
            Ok(router::Choice::AllOut { until: t }) => return err(format!("coord account move: every other account is out {}", until(t))),
            // the owner's move takes it to another account or nowhere: the policy's gateway is not where he sent it
            Ok(router::Choice::Gateway { why, .. }) => {
                return err(format!("coord account move: no other account can take {name} now ({}); it stays where it is", why.unwrap_or_else(|| "every other account is out".into())))
            }
            Ok(router::Choice::None) => return err("coord account move: no other account is signed in".into()),
            Err(why) => return err(format!("coord account move: {why}")),
        };
        if chosen.no_key {
            return err(format!("coord account move: account {id} has no key any more (coord account key {id})"));
        }
        if chosen.from.as_deref() == Some(id.as_str()) {
            return ok(format!("{name} is on account {id} already"));
        }
        let Some(a) = self.agents.get_mut(name) else { return err(format!("coord account move: no agent named {name}")) };
        let running = a.running();
        let busy = a.live.state == "working" && a.ended_turn != a.live.turn;
        if running {
            a.live.account_due = Some("the owner moved it".into());
        } else {
            self.account_record(name, Some(&id), "the owner moved it", None);
        }
        self.changed(name);
        self.poke.notify_one();
        let change = format!("{name}: account {} → {id}", chosen.from.as_deref().unwrap_or("none"));
        ok(if !running {
            format!("{change} from its next start")
        } else if busy {
            format!("{change} when its turn ends (it restarts on its session; its background tasks end)")
        } else {
            format!("{change}; restarted on its session (its background tasks ended)")
        })
    }

    /// The agents that waited for an account (`account_wait`, every account was out) whose time has come, and those
    /// out of their turn that were marked to move because their account was turned off or was being removed ([`OFF`],
    /// [`LEAVING`]; it may have been turned on or kept since). One whose session still runs on the account it is
    /// bound to, which can take requests again, goes on as it is, with no restart (it was never ended), unless a turn
    /// of it was cut by the limit (`Live::account_cut`). Any other is restarted by `to_restart`, and its choice is
    /// made again.
    pub fn account_back(&mut self) {
        let now_s = now_s();
        let turn_over = |a: &Agent| !(a.live.state == "working" && a.ended_turn != a.live.turn);
        let waited: Vec<String> = self
            .agents
            .values()
            .filter(|a| match a.live.account_due.as_deref() {
                Some(OFF | LEAVING) if turn_over(a) => true,
                Some(_) => a.account_wait.is_some_and(|t| t <= now_s),
                None => false,
            })
            .map(|a| a.name.clone())
            .collect();
        for name in waited {
            let Some(e) = self.accounts.bound_to(Harness::Claude, &self.coord_id(), &name) else { continue };
            let a = self.agents.get_mut(&name).unwrap();
            // one whose turn was cut by the limit is restarted all the same: only a start re-opens that turn
            if a.account.as_deref() == Some(e.id.as_str()) && due(&e.state, now_s).is_none() && a.live.account_cut.is_none() {
                eprintln!("coordd: {name}: its account {} is back; it goes on", e.id);
                (a.live.account_due, a.account_wait) = (None, None);
                self.changed(&name);
            }
        }
    }

    /// The key accounts the policy takes again (`+key-again-after(SECONDS).`, policy/accounts.eflint): one out for its
    /// rate limit with no time named, SECONDS after it went out. Asked at each round; the first coordd to find the time
    /// passed writes it under the account's lock and journals it. Refused keys and no credit are never taken again here.
    /// opencode accounts are taken again the same way, and an agy account out for one model with no time named has that
    /// model taken again ([`limits::model_again_at`]).
    ///
    /// The accounts' files are read only when a look is due: at coordd's start, when something said an account may be out
    /// ([`KeyAgain::look`]), when the policy's number changed, and when the next one is due.
    pub fn key_again(&mut self) {
        let (secs, now) = (self.key_again_after(), now_s());
        if secs == self.key_again.secs && self.key_again.at.is_none_or(|at| at > now) {
            return;
        }
        let mut next: Option<i64> = None;
        let keys = self.accounts.list(Harness::Claude).into_iter().filter(|e| e.account.kind == super::Kind::Key).map(|e| (Harness::Claude, e));
        let opencode = self.accounts.list(Harness::Opencode).into_iter().map(|e| (Harness::Opencode, e));
        let agy = self.accounts.list(Harness::Agy).into_iter().map(|e| (Harness::Agy, e));
        for (h, e) in keys.chain(opencode).chain(agy) {
            let Some(secs) = secs else { continue };
            // the account out with no time named; an agy account out so for one model ([`limits::model_again_at`])
            let account = limits::again_at(&e.state, secs).map(|at| (None, at));
            let models = e.state.models_out.iter().filter_map(|o| Some((Some(o.model.as_str()), limits::model_again_at(&e.state, &o.model, secs)?)));
            for (model, at) in account.into_iter().chain(models) {
                if at > now {
                    next = Some(next.map_or(at, |n| n.min(at)));
                    continue;
                }
                let (said, note) = match model {
                    None => (limits::Said::again(), format!("account {} is taken again after {secs} s (policy), out for {} with no time named", e.id, limits::RATE_LIMIT)),
                    Some(m) => (limits::Said::again_model(m), format!("account {} takes {m} again after {secs} s (policy), out for it with no time named", e.id)),
                };
                self.account_keep(Keep { name: String::new(), id: e.id.clone(), h, said, now, note: Some(note), again: Some(secs) });
            }
        }
        self.key_again = KeyAgain { at: next, secs };
        if let Some(at) = next {
            self.wake_at(at);
        }
    }

    /// NAME is called TO from now on: the account it is bound to stays its account.
    pub fn account_renamed(&mut self, name: &str, to: &str) {
        let h = match self.agents.get(name) {
            Some(a) if a.kind == "opencode" => Harness::Opencode,
            Some(a) if a.kind == "agy" => Harness::Agy,
            Some(a) if a.native => Harness::Claude,
            _ => return,
        };
        let coord = self.coord_id();
        if let Some(e) = self.accounts.bound_to(h, &coord, name) {
            if let Err(why) = self.accounts.bind(h, &e.id, &coord, to, &now()) {
                eprintln!("coordd: {name} renamed {to}: its account {} is not bound to the new name ({why})", e.id);
            }
            self.accounts.unbind(h, &coord, name);
        }
    }

    /// What is due about A's account, for its view: a move when its turn ends, or that it waits for an account;
    /// that it does not run and is started when one can take it; or that a turn of it was cut by its account's limit
    /// and has not gone on. That last stays while it is so: a restart that did not happen, or one after which Claude
    /// Code did not take the turn up (its age bound, or a turn it does not count as interrupted), leaves the agent
    /// idle with these words on it rather than with nothing said.
    pub fn account_note(&self, a: &Agent) -> Option<String> {
        let Some(due) = a.live.account_due.as_ref() else {
            return if let Some(g) = a.gateway.as_ref().filter(|_| a.running()) {
                let back = Some(g.back).filter(|t| *t > now_s() && *t != limits::UNKNOWN_RESET).map(|t| format!(" ({})", until(t))).unwrap_or_default();
                Some(format!("through the gateway while no account can take it{back}"))
            } else if !a.running() && a.native && a.account_wait.is_some() && a.account_none {
                Some(if self.accounts_only() { format!("not running: {NO_ACCOUNT_YET}") } else { "not running: no account can take it; it is started when you sign in to one".into() })
            } else if !a.running() && a.native && a.account_wait.is_some() {
                Some(format!("not running; it is started when an account can take it ({})", a.account_wait.map(until).unwrap_or_default()))
            } else if a.live.state == "working" {
                None
            } else if a.account_resumed {
                Some("restarted to go on with the turn its account's limit cut; no turn of it has begun since".into())
            } else if a.live.account_cut.is_some() {
                Some("a turn of it was cut by its account's limit, and it was not restarted to go on with it (coordd's log says why)".into())
            } else {
                None
            };
        };
        Some(match a.account_wait.filter(|t| *t > now_s()) {
            Some(limits::UNKNOWN_RESET) if a.account_none => format!("no account can take it; it waits until you sign in to one ({due})"),
            Some(limits::UNKNOWN_RESET) => format!("every account is out and none says until when; it waits ({due})"),
            Some(t) => format!("every account is out; it waits until {} ({due})", crate::core::at_secs(t)),
            None => format!("moves when not in a turn: {due}"),
        })
    }

    /// When A next looks for an account, if a time is known (seconds): on the gateway, when it looks again; waiting with
    /// every account out, when the first is back ([`Core::account_note`] says the same in words).
    pub fn account_until(&self, a: &Agent) -> Option<i64> {
        a.gateway.as_ref().filter(|_| a.running()).map(|g| g.back).or(a.account_wait).filter(|t| *t > now_s() && *t != limits::UNKNOWN_RESET)
    }

    /// Wake the follower at UNTIL (seconds), when a restart that waits for an account is tried again.
    fn wake_at(&self, until: i64) {
        if until == limits::UNKNOWN_RESET {
            return;
        }
        let Ok(rt) = tokio::runtime::Handle::try_current() else { return };
        let poke = self.poke.clone();
        let wait = std::time::Duration::from_secs((until - now_s()).max(0) as u64 + 1);
        rt.spawn(async move {
            tokio::time::sleep(wait).await;
            poke.notify_one();
        });
    }

    /// What `coord accounts` needs of the core, copied under its lock. The accounts' files and the month's usage are
    /// read after, off it ([`Listing::reply`]).
    pub fn accounts_listing(&self) -> Listing {
        Listing {
            store: self.accounts.clone(),
            folder: self.coord.clone(),
            coord: self.coord_id(),
            notes: self.agents.values().filter_map(|a| Some((a.name.clone(), self.account_note(a)?))).collect(),
            key_again_after: self.key_again_after(),
            all: self.native_all.clone(),
            gateway: self.gateway(),
            on_gateway: self.agents.values().filter(|a| a.running()).filter_map(|a| Some((a.name.clone(), a.gateway.clone()?))).collect(),
            signins: self
                .signins
                .iter()
                .filter(|(_, s)| s.ended().is_none())
                .map(|(id, _)| id.clone())
                .chain(self.opencode_runs.iter().filter(|(_, r)| r.ended().is_none()).map(|(id, _)| id.clone()))
                .chain(self.agy_signins.iter().filter(|(_, r)| r.ended().is_none()).map(|(id, _)| id.clone()))
                .collect(),
        }
    }

    /// `coord account new ID [LABEL] [--tier N] [--key]`: a new account's folder, to be signed in to; with KEY, one
    /// that runs on an API key the owner then gives (`coord account key ID`).
    ///
    /// With HARNESS `opencode` (the window's `opencode` tile), an opencode account for PROVIDER: its folders made
    /// empty, signed in afterwards by opencode's own sign-in (`AccountSignin`, or the key field's `AccountKeyStore`).
    ///
    /// With HARNESS `agy` (the window's `Antigravity` tile), an agy account: its `home/` made empty; with KEY one on a
    /// Gemini API key the key field gives, else one signed in by t3code's own Google sign-in (`AccountSignin`), whose
    /// t3code server coordd starts once it is made (main.rs `agy_server_up`).
    #[allow(clippy::too_many_arguments)]
    pub fn account_new(&mut self, who: &Who, id: &str, label: Option<&str>, tier: i64, key: bool, harness: Option<&str>, provider: Option<&str>) -> Reply {
        if !matches!(who, Who::Owner) {
            return err("coord account new: the owner's".into());
        }
        match harness.map(|h| (h, Harness::named(h))) {
            None | Some((_, Some(Harness::Claude))) => {}
            Some((_, Some(Harness::Opencode))) => {
                let Some(provider) = provider else { return err("coord account new: an opencode account names its provider".into()) };
                return match self.accounts.create_opencode(id, label.unwrap_or(id), tier, provider, &now()) {
                    Ok(e) => {
                        let was = if e.id == id { String::new() } else { format!(" (a removed account had the id {id})") };
                        ok(format!("account {}: made, not signed in yet (opencode, {provider}){was}", e.id))
                    }
                    Err(why) => err(format!("coord account new: {why}")),
                };
            }
            Some((_, Some(Harness::Agy))) => {
                let kind = if key { super::Kind::Key } else { super::Kind::Signin };
                return match self.accounts.create_agy(id, label.unwrap_or(id), tier, kind, &now()) {
                    Ok(e) => {
                        let was = if e.id == id { String::new() } else { format!(" (a removed account had the id {id})") };
                        ok(format!("account {}: made, {} (agy){was}", e.id, if key { "no key yet" } else { "not signed in yet" }))
                    }
                    Err(why) => err(format!("coord account new: {why}")),
                };
            }
            Some((word, None)) => return err(format!("coord account new: no harness {word:?} (claude, opencode, agy)")),
        }
        let kind = if key { super::Kind::Key } else { super::Kind::Signin };
        let made = match self.accounts.create(Harness::Claude, id, label.unwrap_or(id), tier, kind, &now()) {
            Ok(e) => e.id,
            Err(why) => return err(format!("coord account new: {why}")),
        };
        // an id is never taken twice (`Store::fresh_id`): the answer names the one the account has
        let was = if made == id { String::new() } else { format!(" (a removed account had the id {id})") };
        if key {
            ok(format!("account {made}: made, no key yet (coord account key {made}, in a terminal of your own){was}"))
        } else {
            ok(format!("account {made}: made, not signed in yet ({}){was}", self.accounts.config(Harness::Claude, &made).display()))
        }
    }

    /// `coord account key ID [--remove]` and the window's key field, the first part, under the core's lock: whether
    /// the caller may. An agent, or anything typed in an agent's terminal (its cgroup says so), is refused at
    /// coordc's first question, before it reads a key, as keystrokes there pass through that agent's session. An Ok
    /// is the store, for [`key_step`] off the lock.
    pub fn account_key_begin(&self, who: &Who) -> Result<Store, Reply> {
        match who {
            Who::Owner => Ok(self.accounts.clone()),
            _ => Err(err(coord_types::KEY_NOT_HERE.into())),
        }
    }

    /// The last part, under the core's lock again, after [`key_step`] wrote that account ID's key is set or removed:
    /// its agents move when the key is gone, and those that wait for an account are tried when one is set.
    pub fn account_key_end(&mut self, id: &str, step: coord_types::KeyStep) {
        let set = match step {
            coord_types::KeyStep::Where => return,
            coord_types::KeyStep::Set => true,
            coord_types::KeyStep::Removed => false,
        };
        eprintln!("coordd: account {id}: {}", if set { "its key is set" } else { "its key is removed" });
        self.account_checked(id);
    }

    /// Whether account ID is signed in (or has its key) was asked and recorded (main.rs `account_check`: a sign-in's
    /// end, the owner's check, a refused turn; a key set or removed): its agents move when it is not, and those that
    /// wait for an account or went on through the gateway are tried again now, as it may be newly signed in.
    pub fn account_checked(&mut self, id: &str) {
        self.account_seen(id);
        self.account_cleared();
    }

    /// A delivery handed to NAME was seen in its session, its line for it (`Core::ticket_seen`): messages reach the
    /// sessions of the account it runs on. Written when that account was not known to take them (not heard since its
    /// sign-in, a miss standing, deaf); who waits on the gateway then looks again ([`Core::account_seen`]).
    pub fn account_reached(&mut self, name: &str) {
        self.account_session_said(name, limits::Said::reached());
    }

    /// A delivery handed to NAME was never seen in its session (`Core::not_read`). A first such is a miss, and NAME
    /// restarts on its account to take it again; a second after that restart, with none seen between, makes the account
    /// deaf, and its agents move.
    pub fn account_unseen(&mut self, name: &str) {
        self.account_session_said(name, limits::Said::unseen(name));
    }

    /// SAID of NAME's session, written into the state of the account it runs on off the lock (`Job::AccountSaid`),
    /// when it changes it. Nothing for an agent that runs on none of the owner's accounts.
    fn account_session_said(&mut self, name: &str, said: limits::Said) {
        let now = now_s();
        let Some(id) = self.agents.get(name).filter(|a| a.native).and_then(|a| a.account.clone()) else { return };
        let Some(state) = self.accounts.state(Harness::Claude, &id) else { return };
        // a delivery on an account known to take them: nothing to write
        if said.session == Some(limits::Session::Reached) && state.heard.is_some() && state.missed.is_none() && state.deaf.is_none() {
            return;
        }
        let unseen = matches!(said.session, Some(limits::Session::Unseen(_)));
        // a miss while the first stands, before that one's agent has restarted (still running on this account with the
        // first's mark): both may be that one session's, so it is not counted
        if unseen && state.missed.as_ref().and_then(|m| self.agents.get(&m.agent)).is_some_and(|a| a.running() && a.live.unseen_on.as_deref() == Some(id.as_str())) {
            return;
        }
        let mut next = state.clone();
        limits::apply(&mut next, &said, now);
        if next == state {
            return;
        }
        // a first miss: that agent restarts on this account, going on with its turn, and takes the delivery again
        if unseen && state.missed.is_none() {
            let a = self.agents.get_mut(name).unwrap();
            (a.live.unseen_on, a.live.account_cut) = (Some(id.clone()), Some(now));
            if a.running() && a.live.account_due.is_none() {
                a.live.account_due = Some(UNSEEN_TURN.into());
            }
            self.changed(name);
            self.poke.notify_one();
        }
        let note = match &said.session {
            Some(limits::Session::Unseen(_)) if next.deaf.is_some() && state.deaf.is_none() => Some(format!(
                "{name}: account {id} is deaf: a second delivery handed to a session on it was never seen there, with none seen between and the first's session gone; it takes no agent, and its agents move"
            )),
            Some(limits::Session::Unseen(_)) => Some(format!("{name}: account {id}: a delivery handed to {name} was never seen in its session; {name} restarts on it to take it again, and a miss after that, with none seen between, makes the account deaf")),
            Some(limits::Session::Reached) if state.deaf.is_some() => Some(format!("{name}: account {id} takes messages after all: a delivery was seen in {name}'s session")),
            _ => None,
        };
        self.account_keep(Keep { name: name.to_string(), id, h: Harness::Claude, said, now, note, again: None });
    }
}

/// The label of an account of STORE that has signed in, answered its check and can take requests now, and whether it
/// runs on a key (a sign-in account before a key account): what turns the switch `All agents use my accounts` on by
/// itself in a coord where it was never set ([`Core::all_by_itself`]). The accounts' files are read here, so off the
/// core's lock (`main::count_usage`'s round).
pub fn first_signed_in(store: &Store) -> Option<(String, bool)> {
    store.ready().ok()?;
    let now = now_s();
    let can: Vec<super::Entry> = store.list(Harness::Claude).into_iter().filter(|e| e.state.signed_in && due(&e.state, now).is_none()).collect();
    let key = |e: &&super::Entry| e.account.kind == super::Kind::Key;
    can.iter().find(|e| !key(e)).or(can.first()).map(|e| (e.account.label.clone(), key(&e)))
}

/// What a coordd's 5 s round reads of the accounts' files, off the core's lock ([`seen`]), for
/// [`Core::accounts_seen`].
pub struct Seen {
    /// An account that has signed in and can take requests now ([`first_signed_in`]).
    pub first: Option<(String, bool)>,
    /// The accounts marked for removal while agents ran on them (`State::leaving`), each with the agents of any coord
    /// still bound to it.
    pub leaving: Vec<(String, Vec<super::Bound>)>,
    /// The accounts the owner turned off (`State::off`) that agents are still bound to, each with them: they leave
    /// as from one being removed, and the account stays.
    pub off: Vec<(String, Vec<super::Bound>)>,
}

/// The accounts as a round sees them. Blocking (a few small files): not under the core's lock.
pub fn seen(store: &Store) -> Seen {
    let h = Harness::Claude;
    let leaving = store.list(h).into_iter().filter(|e| e.state.leaving.is_some()).map(|e| (e.id.clone(), store.running_on(h, &e.id))).collect();
    let off = store.list(h).into_iter().filter(|e| e.state.off.is_some() && e.state.leaving.is_none()).map(|e| (e.id.clone(), store.running_on(h, &e.id))).filter(|(_, bound)| !bound.is_empty()).collect();
    Seen { first: first_signed_in(store), leaving, off }
}

impl Core {
    /// The round's look at the accounts ([`Seen`]), under the core's lock:
    /// - the switch `All agents use my accounts` turns on by itself at the first account that can take requests
    ///   ([`Core::all_by_itself`]);
    /// - an agent waiting for a signed-in account is tried now, and one on the gateway with no time to look again returns
    ///   to its account;
    /// - this coord's agents leave an account being removed: a running one restarts when not in a turn, a stopped one is
    ///   unbound at once; one with nowhere to go stays bound and waits;
    /// - an account turned off, the same, for [`OFF`]; the account stays.
    ///
    /// Returns the accounts being removed that no agent of any coord is bound to any more; the caller removes them.
    pub fn accounts_seen(&mut self, seen: &Seen) -> Vec<String> {
        if let Some((label, key)) = &seen.first {
            self.all_by_itself(label, *key);
            let now = now_s();
            let waiting: Vec<String> = self.agents.values().filter(|a| a.account_none && a.account_wait == Some(limits::UNKNOWN_RESET)).map(|a| a.name.clone()).collect();
            for name in &waiting {
                self.agents.get_mut(name).unwrap().account_wait = Some(now);
                self.changed(name);
            }
            // one that went on through the gateway with no time named (its account was removed, or every account
            // was out and none said until when) has nothing that brings it back but this: an account can take it
            // now, so it is looked at in this round ([`Core::gateway_back`]). Once: the look takes the "no time" away
            let no_time = self.agents.values().any(|a| a.gateway.as_ref().is_some_and(|g| g.back == limits::UNKNOWN_RESET));
            if no_time {
                self.gateway_look();
            }
            if no_time || !waiting.is_empty() {
                self.poke.notify_one();
            }
        }
        let (h, coord) = (Harness::Claude, self.coord_id());
        let leaving = seen.leaving.iter().map(|(id, bound)| (id, bound, LEAVING));
        for (id, bound, why) in leaving.chain(seen.off.iter().map(|(id, bound)| (id, bound, OFF))) {
            for name in bound.iter().filter(|b| b.coord == coord).map(|b| b.agent.as_str()) {
                match self.agents.get(name) {
                    Some(a) if a.native && a.running() => {
                        if a.live.account_due.is_none() {
                            self.agents.get_mut(name).unwrap().live.account_due = Some(why.into());
                            self.changed(name);
                            self.poke.notify_one();
                        }
                    }
                    // a restart of it is on its way: its choice is being made, and leaves ID by itself. Or it waits
                    // for an account, having found nowhere to go: it stays, and is tried again at its time
                    Some(a) if self.restarting.contains(name) || a.native && a.account_wait.is_some() => {}
                    // it was stopped: unbound now; with the gateway it is one that goes on through it at its next
                    // start unless an account can take it then
                    Some(a) if a.native => {
                        if self.accounts.unbind_from(h, id, &coord, name) {
                            self.account_record(name, None, why, self.gateway().then_some(limits::UNKNOWN_RESET));
                            self.changed(name);
                        }
                    }
                    // no agent of the accounts any more, or none at all: a binding left behind
                    _ => drop(self.accounts.unbind_from(h, id, &coord, name)),
                }
            }
        }
        seen.leaving.iter().filter(|(_, bound)| bound.is_empty()).map(|(id, _)| id.clone()).collect()
    }
}

/// `coord accounts`, answered off the core's lock from what [`Core::accounts_listing`] copied under it.
pub struct Listing {
    store: Store,
    /// The coord's folder, where its usage rows are.
    folder: String,
    /// The coord's id, as bindings name it.
    coord: String,
    /// This coord's agents that wait for an account or are about to move, each with its note.
    notes: Vec<(String, String)>,
    /// The policy's `key-again-after` seconds, for when a key account out for its rate limit is taken again.
    key_again_after: Option<u64>,
    /// The switch `All agents use my accounts`, as last set.
    all: Option<coord_types::AllAgents>,
    /// This coord has the gateway.
    gateway: bool,
    /// This coord's running agents that went on through the gateway while their account is out.
    on_gateway: Vec<(String, OnGateway)>,
    /// The accounts a sign-in of this coordd's runs for now.
    signins: Vec<String>,
}

impl Listing {
    /// `coord accounts`: each account on a line: id, label, tier, state, agents. JSON (the app's Accounts page): a
    /// [`coord_types::NativeAccounts`]. Anyone may ask: labels, tiers, limits and who runs where are no secret among
    /// the owner's agents, and nothing of a login is in it.
    pub fn reply(&self, json: bool) -> Reply {
        if json {
            return ok(serde_json::to_string(&self.page()).unwrap_or_default());
        }
        let now_s = now_s();
        let accounts = self.store.list(Harness::Claude).into_iter().map(|e| {
            let agents: Vec<String> = e.bound.iter().map(|b| format!("{}.{}", b.coord, b.agent)).collect();
            format!("{}\t{}\ttier {}\t{}\t{}", e.id, e.account.label, e.account.tier, shown(e.account.kind, &e.state, now_s), if agents.is_empty() { "no agent".into() } else { agents.join(" ") })
        });
        let lines: Vec<String> = accounts.chain(self.notes.iter().map(|(name, note)| format!("{name}: {note}"))).collect();
        ok(if lines.is_empty() { "no account (coord account new ID)".into() } else { lines.join("\n") })
    }

    /// The accounts as the Accounts page shows them: each with its state, its agents (this coord's with their note),
    /// and this coord's month on it (`cells::by_account`).
    fn page(&self) -> coord_types::NativeAccounts {
        let month = chrono::Utc::now().format("%Y-%m").to_string();
        let now = now_s();
        let mut usage = super::cells::by_account(&self.folder, &month);
        let accounts = Harness::ALL
            .into_iter()
            .flat_map(|h| self.store.list(h))
            .map(|e| {
                let agents = e
                    .bound
                    .iter()
                    .map(|b| coord_types::NativeAgent {
                        coord: b.coord.clone(),
                        agent: b.agent.clone(),
                        since: b.since.clone(),
                        note: self.notes.iter().find(|(name, _)| b.coord == self.coord && *name == b.agent).map(|(_, note)| note.clone()),
                    })
                    .collect();
                let s = e.state;
                let key = e.account.kind == super::Kind::Key;
                // a sign-in account's, at its limit (resets.rs): a key or an opencode account has no resets of Claude Code's
                let (offers, held) = if e.harness == Harness::Claude && !key { (super::resets::offered(&s, now), super::resets::held(&s, now)) } else { Default::default() };
                let key_since = key.then(|| self.store.key_since(e.harness, &e.id)).flatten();
                let usage_key = e.account.provider.as_deref() == Some(super::opencode::GO) && self.store.usage_key_set(&e.id);
                // the policy's time (`Core::key_again`): a key account's, and an opencode or agy one's, read as a key's
                let again_at = self.key_again_after.filter(|_| key || e.harness != Harness::Claude).and_then(|secs| limits::again_at(&s, secs));
                let on_gateway = self
                    .on_gateway
                    .iter()
                    .filter(|(_, g)| g.left.as_deref() == Some(e.id.as_str()))
                    .map(|(name, g)| coord_types::NativeAgent { coord: self.coord.clone(), agent: name.clone(), since: g.since.clone(), note: None })
                    .collect();
                // an agy account's models, for its card (an opencode account's are the router's alone), and those it is
                // out for while their time runs; with all of them out, the account reads out (`State::models_all_out`)
                let models = if e.harness == Harness::Agy { s.models.clone() } else { Vec::new() };
                let models_out = s.models_out.iter().filter(|o| o.until > now).cloned().collect();
                let all_out = s.models_all_out(now);
                // an agy sign-in account's way in (`agy::legacy`), which the window's words follow (N-1956)
                let signed_in_by = (e.harness == Harness::Agy && !key).then(|| (if super::agy::legacy(&self.store.dir(e.harness, &e.id)) { "t3code" } else { "agy" }).to_string());
                coord_types::NativeAccount {
                    models,
                    signed_in_by,
                    models_out,
                    harness: e.harness.dir().to_string(),
                    provider: e.account.provider,
                    usage_key,
                    usage: usage.remove(&e.id).unwrap_or_default(),
                    on_gateway,
                    signin_running: self.signins.contains(&e.id),
                    // coordd makes no call of its own on an account (`coord_types::OWN_CALLS`)
                    own_calls: Vec::new(),
                    put_back: s.put_back.filter(|_| !s.signed_in),
                    leaving: s.leaving,
                    off: s.off,
                    locked_by: self.store.locked_by(e.harness, &e.id).filter(|c| *c != self.coord),
                    id: e.id,
                    label: e.account.label,
                    tier: e.account.tier,
                    key,
                    key_since,
                    signed_in: s.signed_in,
                    signed_in_seen: s.signed_in_seen,
                    login_expires: s.login_expires,
                    plan: s.plan,
                    signed_in_as: s.signed_in_as.filter(|_| s.signed_in),
                    extra_usage_until: s.extra_usage_until.filter(|t| *t > now),
                    five_hour: s.five_hour,
                    seven_day: s.seven_day,
                    month: s.month,
                    // a week past its reset is gone; one not begun (an agy window not used yet) has none and stays
                    model_weeks: s.model_weeks.into_iter().filter(|w| w.window.resets_at == coord_types::NOT_STARTED || w.window.resets_at > now).collect(),
                    model_weeks_seen: s.model_weeks_seen,
                    auth_method: s.auth_method.clone().filter(|_| s.signed_in),
                    limits_seen: s.limits_seen,
                    limits_from: s.limits_from,
                    out_until: s.out_until.max(all_out),
                    again_at,
                    out_for: s.out_for.or_else(|| all_out.map(|_| limits::RATE_LIMIT.to_string())),
                    out_since: s.out_since,
                    out_words: s.out_words,
                    deaf: s.deaf.map(|d| d.agent),
                    agents,
                    // at its limit (resets.rs), and what Claude Code last listed at any time; his last press, one
                    // that said nothing in time ended
                    resets: offers,
                    held_seen: s.resets_seen.filter(|_| !held.is_empty()),
                    held,
                    reset: s.reset.map(|r| super::resets::shown_try(r, now)),
                }
            })
            .collect();
        let mut removed: Vec<coord_types::RemovedAccount> = Harness::ALL
            .into_iter()
            .flat_map(|h| self.store.removed(h).into_iter().map(move |r| coord_types::RemovedAccount { harness: h.dir().to_string(), id: r.id, label: r.label, name: r.name, at: r.at, not_signed_out: r.not_signed_out }))
            .collect();
        removed.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.name.cmp(&b.name)));
        coord_types::NativeAccounts { coord: self.coord.clone(), month, accounts, all: self.all.clone(), gateway: self.gateway, removed }
    }
}

/// A tolerance for the start, added to the cut turn's age in [`resume_env`]; it is no limit on how long an agent may
/// wait, since the wait that happened is in the bound itself, whatever its length. Claude Code asks for a bound on
/// the turn's age and measures it from the transcript's row; this covers what coordd's own figure lacks of that:
/// the time between coordd's answer and Claude Code's look at the turn (roster ending the old process, the new one's
/// start), and the time by which the row is older than coordd's mark of the cut, which is the `failed` hook's
/// arrival, after the request's own retries. A start slower than this leaves the turn not taken up: the agent is
/// idle and its view says so (`Core::account_note`), as the journal does.
const RESUME_MARGIN: i64 = 600;

/// The environment lines (`NAME=VALUE` to set, `-NAME` to remove) of a start that goes on with a turn cut at CUT
/// (seconds) by its account's limit: `CLAUDE_CODE_RESUME_INTERRUPTED_TURN=1`, and its `_MAX_AGE_MS` set to the wait
/// plus [`RESUME_MARGIN`]. Without a cut turn both are removed. The continuation's text is Claude Code's, never ours.
fn resume_env(cut: Option<i64>, now: i64) -> Vec<String> {
    const TURN: &str = "CLAUDE_CODE_RESUME_INTERRUPTED_TURN";
    let turn = match cut {
        Some(at) => vec![format!("{TURN}=1"), format!("{TURN}_MAX_AGE_MS={}", ((now - at).max(0) + RESUME_MARGIN) * 1000)],
        None => vec![format!("-{TURN}"), format!("-{TURN}_MAX_AGE_MS")],
    };
    turn.into_iter().chain(["-CLAUDE_CODE_RESUME_PROMPT".to_string(), "-CLAUDE_CODE_RESUME_SHOWS_CUT_OFF_CALLS".to_string()]).collect()
}

/// An account's state in words, for `coord accounts`. A window is named while it runs: what was seen of one whose
/// reset has passed says nothing of the window after it.
pub fn shown(kind: super::Kind, s: &State, now: i64) -> String {
    let key = kind == super::Kind::Key;
    if s.leaving.is_some() {
        return "being removed, when no agent runs on it".into();
    }
    if s.off.is_some() {
        return "turned off: takes no agent".into();
    }
    if !s.signed_in {
        return if key { "no key" } else { "not signed in" }.into();
    }
    if let Some(d) = &s.deaf {
        return format!("deaf: deliveries never reached its sessions (the second {}'s)", d.agent);
    }
    let window = |name: &str, w: &Option<super::Window>| w.as_ref().filter(|w| w.resets_at > now).map(|w| format!("{name} {:.0}%", w.used_percent));
    let mut parts: Vec<String> = [window("5h", &s.five_hour), window("week", &s.seven_day)].into_iter().flatten().collect();
    // an agy account's models out (`State::models_out`), each by agy's name; with all of them out, the account is out
    // as well (`State::out_now`)
    parts.extend(s.models_out.iter().filter(|o| o.until > now).map(|o| format!("out for {} {}", o.model.strip_prefix(super::agy::MODEL_PREFIX).unwrap_or(&o.model), until(o.until))));
    match (s.out_now(now), s.out_for.as_deref()) {
        // the two only the owner clears, by name
        (Some(_), Some(limits::REFUSED)) if key => parts.insert(0, "out, key refused".into()),
        (Some(_), Some(limits::NO_CREDIT)) if key => parts.insert(0, "out, no credit".into()),
        (Some(_), Some(limits::REFUSED)) => parts.insert(0, "out, login refused".into()),
        (Some(t), _) => parts.insert(0, format!("out {}", until(t))),
        (None, _) => parts.insert(0, if key { "key set" } else { "signed in" }.into()),
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::super::testing::Tmp;
    use super::super::Lock;
    use super::*;
    use crate::core::Live;

    /// The requests that change the store, their three steps (main.rs `account_store`) one after the other: here
    /// nothing else holds the store's lock, unless a test holds it itself.
    impl Core {
        fn account_for(&mut self, who: &Who, name: &str) -> Reply {
            match self.account_for_begin(who, name, None) {
                Ok(choosing) => self.account_for_end(choosing.run()),
                Err(answered) => answered,
            }
        }

        fn move_agent(&mut self, who: &Who, name: &str, to: Option<&str>) -> Reply {
            match self.move_begin(who, name, to) {
                Ok(choosing) => self.move_end(choosing.run()),
                Err(answered) => answered,
            }
        }

        fn account_key(&mut self, who: &Who, id: &str, step: coord_types::KeyStep) -> Reply {
            let store = match self.account_key_begin(who) {
                Ok(store) => store,
                Err(answered) => return answered,
            };
            match key_step(&store, id, step) {
                Ok(answer) => {
                    self.account_key_end(id, step);
                    ok(answer)
                }
                Err(why) => err(format!("coord account key: {why}")),
            }
        }

        fn account_clear(&mut self, who: &Who, id: &str) -> Reply {
            let store = match self.account_owners(who, "clear") {
                Ok(store) => store,
                Err(answered) => return answered,
            };
            match store.update(Harness::Claude, id, State::cleared) {
                Ok(_) => {
                    self.owner_cleared(id, "his press, it can take requests,");
                    ok(format!("account {id}: can take requests again"))
                }
                Err(why) => err(format!("coord account clear: {why}")),
            }
        }
    }

    /// The account folder of `account_for`'s answer R, its first line (the session's environment lines follow).
    fn folder(r: Reply) -> String {
        assert!(r.ok, "{}", r.out);
        r.out.lines().next().unwrap_or_default().to_string()
    }

    /// A core in a coord folder of its own with agent `w` (a Claude Code agent on MODEL, running), and the accounts
    /// a and b (b's tier below a's), signed in.
    fn core(t: &Tmp, model: &str) -> Core {
        core_of(t, model, &["w"])
    }

    /// [`core`] with the agents NAMES, each as `w` is there.
    fn core_of(t: &Tmp, model: &str, names: &[&str]) -> Core {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = t.dir.join("research");
        std::fs::create_dir_all(&coord).unwrap();
        let adds: String = names.iter().enumerate().map(|(i, name)| format!("{{\"ev\":\"add\",\"id\":\"AGT-{:03}\",\"name\":\"{name}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"{model}\",\"at\":\"x\"}}\n", i + 1)).collect();
        std::fs::write(coord.join("registry.jsonl"), adds).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.accounts = t.store.clone();
        for (id, tier) in [("a", 10), ("b", 0)] {
            core.accounts.create(Harness::Claude, id, id, tier, crate::accounts::Kind::Signin, "x").unwrap();
            // heard: a delivery was seen in a session on it (the deaf tests make one that is not)
            core.accounts.update(Harness::Claude, id, |s| (s.signed_in, s.heard) = (true, Some(1))).unwrap();
        }
        for name in names {
            core.agents.get_mut(*name).unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        }
        core
    }

    #[test]
    fn an_opencode_agent_gets_an_opencode_account_of_its_models_provider_with_its_folders_and_never_a_claude_one() {
        let t = Tmp::new("oc-for");
        // the Claude Code accounts a and b are signed in; w is an opencode agent on OpenCode Go
        let mut core = core(&t, "x");
        let w = core.agents.get_mut("w").unwrap();
        (w.kind, w.model) = ("opencode".into(), "opencode-go/kimi-k2".into());
        let h = Harness::Opencode;
        // no opencode account: nothing, so it runs on folders of its own; no Claude Code account is looked at
        assert_eq!(core.account_for(&Who::Owner, "w"), ok(String::new()));
        assert!(t.store.bound_to(Harness::Claude, "research", "w").is_none());
        t.store.create_opencode("zen", "Zen", 5, "opencode", "x").unwrap();
        t.store.create_opencode("go", "Go", 0, "opencode-go", "x").unwrap();
        // made and not signed in: still nothing
        assert_eq!(core.account_for(&Who::Owner, "w"), ok(String::new()));
        for id in ["zen", "go"] {
            t.store.update(h, id, |s| s.signed_in = true).unwrap();
        }
        // Go's, though Zen's tier is higher: Zen is another provider's. Its folder, then its session's environment
        let r = core.account_for(&Who::Owner, "w");
        assert!(r.ok, "{}", r.out);
        let env: Vec<String> = t.store.session_env(h, "go").into_iter().map(|(k, v)| format!("{k}={v}")).collect();
        assert_eq!(r.out.lines().collect::<Vec<_>>(), std::iter::once(t.store.dir(h, "go").display().to_string()).chain(env).collect::<Vec<_>>());
        assert!(r.out.contains(&format!("XDG_DATA_HOME={}", t.store.home("go").join("data").display())), "{}", r.out);
        assert!(t.store.home("go").join("data").is_dir(), "its folders are laid out before roster is told");
        assert_eq!(t.store.bound_to(h, "research", "w").map(|e| e.id).as_deref(), Some("go"));
        // a restart keeps it; no registry line and no account on the agent's view (that is a Claude Code agent's)
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), t.store.dir(h, "go").display().to_string());
        assert!(core.agents["w"].account.is_none());
        assert!(!std::fs::read_to_string(t.dir.join("research/registry.jsonl")).unwrap().contains("\"ev\":\"account\""));
        // the model of a provider no account is signed in to: nothing, and the binding to Go is let go
        core.agents.get_mut("w").unwrap().model = "openai/gpt-6.2-sol".into();
        assert_eq!(core.account_for(&Who::Owner, "w"), ok(String::new()));
        assert!(t.store.bound_to(h, "research", "w").is_none());
        // a model that names no provider is refused in words; a move and the switch are not an opencode agent's
        core.agents.get_mut("w").unwrap().model = "kimi-k2".into();
        assert_eq!(core.account_for(&Who::Owner, "w"), err("coord account: w's model \"kimi-k2\" names no provider (PROVIDER/MODEL)".into()));
        assert!(!core.move_agent(&Who::Owner, "w", None).ok && !core.native(&Who::Owner, "w", true).ok);
        // a rename keeps its binding under the new name
        core.agents.get_mut("w").unwrap().model = "opencode-go/kimi-k2".into();
        core.account_for(&Who::Owner, "w");
        core.account_renamed("w", "w2");
        assert_eq!(t.store.bound_to(h, "research", "w2").map(|e| e.id).as_deref(), Some("go"));
        assert!(t.store.bound_to(h, "research", "w").is_none());
    }

    /// The agy account an agy agent is bound to, as `account-for` gave it, read again with nothing chosen (what the
    /// cutover carries to coordd-v2): the same lines on a sign-in and on a key, none while it is bound to none, the
    /// binding as it was after; the owner's only, and an agy agent's only.
    #[test]
    fn account_bound_reads_an_agy_agents_account_as_account_for_gave_it_and_chooses_nothing() {
        let t = Tmp::new("agy-bound");
        let mut core = core(&t, "x");
        let w = core.agents.get_mut("w").unwrap();
        (w.kind, w.model) = ("agy".into(), "agy/gemini-3.8-flash-high".into());
        let h = Harness::Agy;
        t.store.create_agy("g", "Google", 5, crate::accounts::Kind::Signin, "x").unwrap();
        t.store.create_agy("k", "Key", 0, crate::accounts::Kind::Key, "x").unwrap();
        t.store.update(h, "g", |s| (s.signed_in, s.models) = (true, vec!["agy/gemini-3.8-flash-high".into()])).unwrap();
        assert_eq!(core.account_bound(&Who::Owner, "w"), err("coord account-bound: no agy account is bound to w".into()), "none chosen by asking");
        assert!(t.store.bound_to(h, "research", "w").is_none());
        let given = core.account_for(&Who::Owner, "w");
        assert_eq!(core.account_bound(&Who::Owner, "w"), given);
        // the key account would be chosen now; the bound one is still the sign-in's, read as it is
        t.store.update(h, "g", |s| s.signed_in = false).unwrap();
        t.store.update(h, "k", |s| s.signed_in = true).unwrap();
        assert_eq!(core.account_bound(&Who::Owner, "w"), given, "read, not chosen again");
        assert_eq!(t.store.bound_to(h, "research", "w").map(|e| e.id).as_deref(), Some("g"));
        let on_key = core.account_for(&Who::Owner, "w");
        assert!(on_key.out.contains("@key "), "{}", on_key.out);
        assert_eq!(core.account_bound(&Who::Owner, "w"), on_key);
        assert_eq!(core.account_bound(&Who::Agent("w".into()), "w"), err("coord account-bound: the owner's".into()));
        core.agents.get_mut("w").unwrap().kind = "claude".into();
        assert_eq!(core.account_bound(&Who::Owner, "w"), err("coord account-bound: w is a claude agent, not an agy one".into()));
        assert_eq!(core.account_bound(&Who::Owner, "nobody"), err("coord account-bound: no agent named nobody".into()));
    }

    /// An agy agent's account is one of the owner's agy accounts: a sign-in account gives its folder and `AGY_SIGNIN`
    /// (`T3_HOME` while t3code's sign-in is its only one), a key account its folder and `@key`; one whose models leave
    /// out the agent's is passed over; with none, an error says so.
    #[test]
    fn an_agy_agent_gets_an_agy_account_that_offers_its_model_a_sign_ins_home_or_a_keys_file() {
        let t = Tmp::new("agy-for");
        let mut core = core(&t, "x");
        let w = core.agents.get_mut("w").unwrap();
        (w.kind, w.model) = ("agy".into(), "agy/gemini-3.8-flash-high".into());
        let h = Harness::Agy;
        // no agy account: refused in words, and no Claude Code account is looked at
        let none = err("coord account: no agy account can take w now (signed in, with its key set, not off, not out)".into());
        assert_eq!(core.account_for(&Who::Owner, "w"), none);
        assert!(t.store.bound_to(Harness::Claude, "research", "w").is_none());
        t.store.create_agy("g", "Google", 5, crate::accounts::Kind::Signin, "x").unwrap();
        t.store.create_agy("k", "Key", 0, crate::accounts::Kind::Key, "x").unwrap();
        // made, nobody signed in and no key: still none
        assert_eq!(core.account_for(&Who::Owner, "w"), none);
        // the sign-in account signed in, offering another model alone: passed over
        t.store.update(h, "g", |s| (s.signed_in, s.models) = (true, vec!["agy/gemini-4-pro".into()])).unwrap();
        assert_eq!(core.account_for(&Who::Owner, "w"), none);
        // offering it: its folder, then its profile, where agy's own sign-in keeps it
        t.store.update(h, "g", |s| s.models.push("agy/gemini-3.8-flash-high".into())).unwrap();
        let g = t.store.dir(h, "g");
        assert_eq!(core.account_for(&Who::Owner, "w"), ok(format!("{}\nAGY_SIGNIN={}", g.display(), g.join("home/profile").display())));
        assert!(g.join("home").is_dir(), "its home is laid out before roster is told");
        // one t3code signed in, and agy's own sign-in not used on it: its t3code home
        std::fs::create_dir_all(g.join("home/.t3")).unwrap();
        assert_eq!(core.account_for(&Who::Owner, "w"), ok(format!("{}\nT3_HOME={}", g.display(), g.join("home").display())));
        assert_eq!(t.store.bound_to(h, "research", "w").map(|e| e.id).as_deref(), Some("g"));
        assert!(core.agents["w"].account.is_none(), "no account on the agent's view: that is a Claude Code agent's");
        // signed out there, and the key account's key set: its folder, then its key's file
        t.store.update(h, "g", |s| s.signed_in = false).unwrap();
        t.store.update(h, "k", |s| s.signed_in = true).unwrap();
        let k = t.store.dir(h, "k");
        assert_eq!(core.account_for(&Who::Owner, "w"), ok(format!("{}\n@key {}", k.display(), t.store.key_file(h, "k").display())));
        assert_eq!(t.store.bound_to(h, "research", "w").map(|e| e.id).as_deref(), Some("k"));
        assert!(!core.move_agent(&Who::Owner, "w", None).ok && !core.native(&Who::Owner, "w", true).ok);
    }

    /// An agy turn refused by its limit puts its account out for the agent's model alone, on made-up accounts: g and k;
    /// cl's refusal puts g out for cl's model until its window's reset, gm keeps g, cl goes to k and back at the reset;
    /// with every model of g out it takes none; an out on k with no time is taken again by key-again-after.
    #[test]
    fn an_agy_turn_refused_by_its_limit_puts_its_account_out_for_that_model_alone() {
        let t = Tmp::new("agy-model-out");
        let mut core = core_of(&t, "x", &["cl", "gp", "gm"]);
        let (claude, gpt, gemini) = ("agy/claude-sonnet-4-6", "agy/gpt-oss-120b", "agy/gemini-3.8-flash-high");
        for (name, model) in [("cl", claude), ("gp", gpt), ("gm", gemini)] {
            let a = core.agents.get_mut(name).unwrap();
            (a.kind, a.model) = ("agy".into(), model.into());
        }
        let (h, now) = (Harness::Agy, now_s());
        let reset = now + 3600;
        let week = |model: &str, used_percent: f64, resets_at: i64| coord_types::ModelWeek { model: model.into(), window: crate::accounts::Window { used_percent, resets_at } };
        t.store.create_agy("g", "Google", 5, crate::accounts::Kind::Signin, "x").unwrap();
        t.store.create_agy("k", "Key", 0, crate::accounts::Kind::Key, "x").unwrap();
        t.store
            .update(h, "g", |s| {
                (s.signed_in, s.heard, s.models) = (true, Some(1), vec![claude.into(), gpt.into(), gemini.into()]);
                s.model_weeks = vec![week("Claude & GPT (shared) · 5-hour", 100.0, reset), week("Gemini · Weekly", 10.0, now + 86_400)];
            })
            .unwrap();
        t.store.update(h, "k", |s| (s.signed_in, s.heard, s.models) = (true, Some(1), vec![claude.into(), gemini.into()])).unwrap();
        // k's key file, made up, so that a choice of k does not find it gone (`Chosen::no_key`)
        crate::keyfile::write(&t.store.key_file(h, "k"), "made-up").unwrap();
        let on = |core: &mut Core, name: &str| {
            let r = core.account_for(&Who::Owner, name);
            assert!(r.ok, "{name}: {}", r.out);
            t.store.bound_to(h, "research", name).map(|e| e.id).unwrap()
        };
        let refused = |core: &mut Core, name: &str, input: serde_json::Value| assert!(matches!(core.hook(name, "failed", &input), crate::core::HookNext::CodexStop));
        let quota = "You have reached your Antigravity quota for this period.";
        let state = |id: &str| t.store.state(h, id).unwrap();
        let card = |core: &Core, id: &str| {
            let page: coord_types::NativeAccounts = serde_json::from_str(&core.accounts_listing().reply(true).out).unwrap();
            page.accounts.into_iter().find(|a| a.id == id).unwrap()
        };
        for name in ["cl", "gp", "gm"] {
            assert_eq!(on(&mut core, name), "g", "{name}: a used-up window is no verdict");
        }

        // cl's turn refused, no time named: out for its model until the used-up window's reset, the rest of g in
        refused(&mut core, "cl", json!({"session_id": "s", "error": "rate_limit", "error_details": quota}));
        let g = state("g");
        assert_eq!(g.models_out.iter().map(|o| (o.model.as_str(), o.until, o.words.as_deref())).collect::<Vec<_>>(), [(claude, reset, Some(quota))], "{g:?}");
        assert!(g.out_until.is_none() && g.can_take(now) && g.can_take_model(gemini, now) && !g.can_take_model(claude, reset - 1) && g.can_take_model(claude, reset));
        assert_eq!(on(&mut core, "gm"), "g", "its model is not out there");
        assert_eq!(on(&mut core, "cl"), "k", "its model is out on g");
        let c = card(&core, "g");
        assert_eq!((c.out_until, c.models_out.iter().map(|o| (o.model.as_str(), o.until)).collect::<Vec<_>>()), (None, vec![(claude, reset)]));
        let words = shown(crate::accounts::Kind::Signin, &g, now);
        assert!(words.starts_with("signed in") && words.ends_with(&format!("out for claude-sonnet-4-6 until {}", crate::core::at_secs(reset))), "{words}");

        // the reset passed: cl's model is g's again
        t.store.update(h, "g", |s| s.models_out.iter_mut().for_each(|o| o.until = now - 1)).unwrap();
        t.store.unbind(h, "research", "cl");
        assert_eq!(on(&mut core, "cl"), "g");

        // every model g offers out (gm's with agy-rpc's time): it takes no agent, and reads out until the first is back
        let named = now + 600;
        refused(&mut core, "cl", json!({"session_id": "s", "error": "rate_limit", "error_details": quota}));
        refused(&mut core, "gp", json!({"session_id": "s", "error": "rate_limit", "error_details": quota}));
        assert!(state("g").models_all_out(now).is_none() && state("g").can_take(now), "the Gemini model is not out yet");
        refused(&mut core, "gm", json!({"session_id": "s", "error": "rate_limit", "error_details": quota, "resets_at": named}));
        let g = state("g");
        assert_eq!((g.model_out(gemini, now), g.models_all_out(now), g.out_now(now), g.can_take(now)), (Some(named), Some(named), Some(named), false));
        let c = card(&core, "g");
        assert_eq!((c.out_until, c.out_for.as_deref(), c.models_out.len()), (Some(named), Some(limits::RATE_LIMIT), 3));
        assert_eq!(on(&mut core, "gm"), "k", "g takes none of them");
        // with its models not known, no model's being out puts it out
        t.store.update(h, "g", |s| s.models.clear()).unwrap();
        assert!(state("g").models_all_out(now).is_none() && state("g").can_take(now));

        // gm's on k, with no window, no time known: the policy's key-again-after (60 s) takes that model again
        refused(&mut core, "gm", json!({"session_id": "s", "error": "rate_limit", "error_details": quota}));
        assert_eq!(state("k").model_out(gemini, now), Some(limits::UNKNOWN_RESET));
        assert!(state("k").can_take_model(claude, now) && state("k").out_until.is_none());
        assert_eq!(card(&core, "k").models_out.iter().map(|o| (o.model.as_str(), o.until)).collect::<Vec<_>>(), [(gemini, limits::UNKNOWN_RESET)]);
        let later = |core: &mut Core, secs: i64| {
            t.store.update(h, "k", |s| s.models_out.iter_mut().for_each(|o| o.since -= secs)).unwrap();
            core.key_again = KeyAgain::look();
            core.key_again();
        };
        later(&mut core, 50);
        assert_eq!(state("k").model_out(gemini, now), Some(limits::UNKNOWN_RESET));
        later(&mut core, 11);
        assert!(state("k").models_out.is_empty() && state("k").can_take_model(gemini, now_s()));
        for id in ["a", "b"] {
            let s = t.store.state(Harness::Claude, id).unwrap();
            assert!(s.out_until.is_none() && s.models_out.is_empty(), "{id}: {s:?}");
        }
    }

    /// A turn cut by its process's end goes on at the next start in Claude Code's own way, on the model route as on an
    /// account; with no cut, an account's start removes the variables.
    #[test]
    fn a_turn_the_end_of_its_process_cut_goes_on_at_the_next_start_on_the_route_and_on_an_account() {
        let t = Tmp::new("down-cut");
        let mut core = core(&t, "claude-opus-5-5");
        let at = now_s() - 120;
        let resumed = |r: &Reply| {
            assert!(r.ok, "{}", r.out);
            let lines: Vec<&str> = r.out.lines().collect();
            assert!(lines.contains(&"CLAUDE_CODE_RESUME_INTERRUPTED_TURN=1"), "{}", r.out);
            let age: i64 = lines.iter().find_map(|l| l.strip_prefix("CLAUDE_CODE_RESUME_INTERRUPTED_TURN_MAX_AGE_MS=")).expect("a bound").parse().unwrap();
            assert!((720_000..=725_000).contains(&age), "the 120 s since the cut and the margin: {age}");
            lines[0].to_string()
        };
        // the model route
        let r = core.account_for_begin(&Who::Owner, "w", Some(at)).err().expect("the route's answer is whole");
        assert_eq!(resumed(&r), "", "no folder");
        assert_eq!(core.account_for_begin(&Who::Owner, "w", None).err(), Some(ok(String::new())));
        // an account
        core.agents.get_mut("w").unwrap().native = true;
        let chosen = core.account_for_begin(&Who::Owner, "w", Some(at)).ok().expect("w runs on an account").run();
        assert_eq!(resumed(&core.account_for_end(chosen)), t.store.config(Harness::Claude, "a").display().to_string());
        let r = core.account_for(&Who::Owner, "w");
        assert!(r.out.lines().any(|l| l == "-CLAUDE_CODE_RESUME_INTERRUPTED_TURN") && !r.out.contains("_TURN=1"), "{}", r.out);
    }

    /// An opencode agent's failed turn marks its bound account as a key account's is: the provider's limit until
    /// opencode's time, else until the policy takes it again; a refused key or no credit until the owner. opencode's own
    /// errors mark nothing. The agent is neither cut nor moved.
    #[test]
    fn an_opencode_agents_failed_turn_marks_its_account_as_a_keys_and_the_policy_takes_its_limit_again() {
        let t = Tmp::new("oc-failed");
        let mut core = core(&t, "x");
        let w = core.agents.get_mut("w").unwrap();
        (w.kind, w.model) = ("opencode".into(), "opencode-go/kimi-k2".into());
        let h = Harness::Opencode;
        t.store.create_opencode("go", "Go", 0, "opencode-go", "x").unwrap();
        t.store.update(h, "go", |s| s.signed_in = true).unwrap();
        assert!(core.account_for(&Who::Owner, "w").ok);
        let state = || t.store.state(h, "go").unwrap();
        let fails = |core: &mut Core, input: serde_json::Value| assert!(matches!(core.hook("w", "failed", &input), crate::core::HookNext::CodexStop));
        let card = |core: &Core| {
            let page: coord_types::NativeAccounts = serde_json::from_str(&core.accounts_listing().reply(true).out).unwrap();
            page.accounts.into_iter().find(|a| a.id == "go").unwrap()
        };
        let later = |core: &mut Core, secs: i64| {
            t.store.update(h, "go", |s| s.out_since = s.out_since.map(|at| at - secs)).unwrap();
            core.key_again = KeyAgain::look();
            core.key_again();
        };

        for word in ["APIError", "ContextOverflowError"] {
            let before = state();
            fails(&mut core, json!({"session_id": "s", "error": word, "error_details": "Forbidden: model not in plan"}));
            assert_eq!(state(), before, "{word}");
        }
        // its provider's limit, until the time opencode had from the provider's retry-after
        let until = now_s() + 3600;
        fails(&mut core, json!({"session_id": "s", "error": "rate_limit", "error_details": "Go limit reached", "resets_at": until}));
        let s = state();
        assert_eq!((s.out_until, s.out_for.as_deref(), s.out_words.as_deref()), (Some(until), Some(limits::RATE_LIMIT), Some("Go limit reached")));
        let live = &core.agents["w"].live;
        assert!(live.account_cut.is_none() && live.account_due.is_none() && core.agents["w"].failed.as_deref() == Some("rate_limit: Go limit reached"));
        assert_eq!(card(&core).again_at, None, "a time is named: the policy's is none of it");

        // a limit with no time named: the policy takes it again after its 60 s, and the card says when
        t.store.update(h, "go", State::cleared).unwrap();
        fails(&mut core, json!({"session_id": "s", "error": "rate_limit", "error_details": "Free usage exceeded"}));
        let s = state();
        assert!(limits::reset_unknown(&s) && s.out_for.as_deref() == Some(limits::RATE_LIMIT), "{s:?}");
        assert_eq!(card(&core).again_at, s.out_since.map(|at| at + 60));
        later(&mut core, 50);
        assert!(limits::reset_unknown(&state()));
        later(&mut core, 11);
        assert_eq!((state().out_until, state().out_for), (None, None));

        // a refused sign-in or key, and no credit: out until the owner acts; a sign-in anew takes a refusal away
        for (word, why) in [("authentication_failed", limits::REFUSED), ("billing_error", limits::NO_CREDIT)] {
            fails(&mut core, json!({"session_id": "s", "error": word, "error_details": "Unauthorized"}));
            assert_eq!(state().out_for.as_deref(), Some(why));
            later(&mut core, 600);
            assert_eq!((state().out_for.as_deref(), card(&core).again_at), (Some(why), None));
            t.store.update(h, "go", State::signed_in_again).unwrap();
            assert_eq!(state().out_for.as_deref(), (why == limits::NO_CREDIT).then_some(why), "only a refusal is a sign-in's to mend");
            t.store.update(h, "go", State::cleared).unwrap();
        }
        for id in ["a", "b"] {
            let s = t.store.state(Harness::Claude, id).unwrap();
            assert!(s.out_until.is_none() && s.out_for.is_none(), "{id}: {s:?}");
        }
    }

    /// A turn's end of an opencode agent on an OpenCode Go account whose key is kept has Go's usage read again; one read
    /// at a time, turn ends during a read giving one more after it.
    #[test]
    fn a_turns_end_on_go_reads_its_usage_one_read_at_a_time() {
        let t = Tmp::new("oc-go-usage");
        let mut core = core(&t, "x");
        let w = core.agents.get_mut("w").unwrap();
        (w.kind, w.model) = ("opencode".into(), "opencode-go/kimi-k2".into());
        let h = Harness::Opencode;
        t.store.create_opencode("go", "Go", 0, "opencode-go", "x").unwrap();
        t.store.update(h, "go", |s| s.signed_in = true).unwrap();
        assert!(core.account_for(&Who::Owner, "w").ok);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        let mut reads = || std::iter::from_fn(|| rx.try_recv().ok()).filter(|j| matches!(j, crate::core::Job::GoUsage { id } if id == "go")).count();
        let _ = core.hook("w", "stop", &json!({"session_id": "s"}));
        assert_eq!(reads(), 0, "no key kept for it");
        crate::keyfile::write(&t.store.usage_key_file("go"), "Authorization: Bearer made-up").unwrap();
        let _ = core.hook("w", "stop", &json!({"session_id": "s"}));
        assert_eq!(reads(), 1, "the first read");
        // three turn ends while it is read, one of them failed
        let _ = core.hook("w", "failed", &json!({"session_id": "s", "error": "APIError"}));
        let _ = core.hook("w", "stop", &json!({"session_id": "s"}));
        let _ = core.hook("w", "stop", &json!({"session_id": "s"}));
        assert_eq!(reads(), 0, "one read at a time");
        assert!(core.go_read_done("go"), "the first read done: one more, for the three turn ends during it");
        assert!(!core.go_read_done("go"), "the second done: no third, two reads in all");
        let _ = core.hook("w", "failed", &json!({"session_id": "s", "error": "rate_limit", "error_details": "Go limit reached"}));
        assert_eq!(reads(), 1, "a turn's end after them starts a read again, a failed turn's too");
        t.store.update(h, "go", |s| s.month = Some(crate::accounts::Window { used_percent: 20.0, resets_at: now_s() + 86_400 })).unwrap();
        let page: coord_types::NativeAccounts = serde_json::from_str(&core.accounts_listing().reply(true).out).unwrap();
        let go = page.accounts.into_iter().find(|a| a.id == "go").unwrap();
        assert!(go.usage_key && go.month.as_ref().is_some_and(|m| m.used_percent == 20.0), "{go:?}");
    }

    /// An opencode agent's view names the opencode account it is bound to, as a native agent's does.
    #[test]
    fn an_opencode_agents_view_names_the_account_it_is_bound_to() {
        let t = Tmp::new("oc-view-account");
        let mut core = core(&t, "x");
        let w = core.agents.get_mut("w").unwrap();
        (w.kind, w.model) = ("opencode".into(), "opencode-go/kimi-k2".into());
        t.store.create_opencode("go", "Go", 0, "opencode-go", "x").unwrap();
        t.store.update(Harness::Opencode, "go", |s| s.signed_in = true).unwrap();
        assert_eq!(core.view("w").account, None, "bound to none yet");
        assert!(core.account_for(&Who::Owner, "w").ok);
        assert_eq!(core.view("w").account.as_deref(), Some("go"));
    }

    /// W's restart ran and a new process took it: nothing is due any more.
    fn restarted(core: &mut Core) {
        restarted_as(core, "w");
    }

    /// [`restarted`] of agent NAME.
    fn restarted_as(core: &mut Core, name: &str) {
        core.restarted(name, None, "");
        let live = &mut core.agents.get_mut(name).unwrap().live;
        (live.account_due, live.account_cut, live.unseen_on) = (None, None, None);
    }

    #[test]
    fn the_listing_offers_resets_at_a_limit_to_a_sign_in_account_and_never_to_a_key_or_opencode_account() {
        let t = Tmp::new("offers");
        let core = core(&t, "claude-opus-5-5:high");
        core.accounts.create(Harness::Claude, "k", "k", 10, crate::accounts::Kind::Key, "x").unwrap();
        core.accounts.create_opencode("go", "go", 10, "opencode-go", "x").unwrap();
        let at_limit = |s: &mut State| (s.signed_in, s.out_until, s.out_for) = (true, Some(now_s() + 600), Some(limits::RATE_LIMIT.into()));
        for (h, id) in [(Harness::Claude, "a"), (Harness::Claude, "k"), (Harness::Opencode, "go")] {
            core.accounts.update(h, id, at_limit).unwrap();
        }
        let page: coord_types::NativeAccounts = serde_json::from_str(&core.accounts_listing().reply(true).out).unwrap();
        let kinds = |id: &str| page.accounts.iter().find(|a| a.id == id).unwrap().resets.iter().map(|o| o.kind.clone()).collect::<Vec<_>>();
        assert_eq!(kinds("a"), ["Fable reset", "full reset"], "no notice read: Claude Code's own confirm says what it holds");
        assert!(kinds("k").is_empty() && kinds("go").is_empty(), "no resets of Claude Code's on a key or opencode account");
        assert!(kinds("b").is_empty(), "b is at no limit");
        // its weeks of one model, as `get_usage` last named them: those that run
        let week = |model: &str, resets_at| coord_types::ModelWeek { model: model.into(), window: crate::accounts::Window { used_percent: 52.0, resets_at } };
        core.accounts.update(Harness::Claude, "b", |s| s.model_weeks = vec![week("Fable", now_s() + 3600), week("Sonnet only", now_s() - 1)]).unwrap();
        let page: coord_types::NativeAccounts = serde_json::from_str(&core.accounts_listing().reply(true).out).unwrap();
        assert_eq!(page.accounts.iter().find(|a| a.id == "b").unwrap().model_weeks.iter().map(|w| w.model.as_str()).collect::<Vec<_>>(), ["Fable"]);
        // its plan as `claude auth status` said it, and extra usage while it lasts
        core.accounts.update(Harness::Claude, "b", |s| (s.plan, s.extra_usage_until) = (Some("max".into()), Some(now_s() + 3600))).unwrap();
        core.accounts.update(Harness::Claude, "a", |s| s.extra_usage_until = Some(now_s() - 1)).unwrap();
        let page: coord_types::NativeAccounts = serde_json::from_str(&core.accounts_listing().reply(true).out).unwrap();
        let card = |id: &str| page.accounts.iter().find(|a| a.id == id).unwrap().clone();
        assert_eq!((card("b").plan.as_deref(), card("b").extra_usage_until.is_some()), (Some("max"), true));
        assert_eq!(card("a").extra_usage_until, None, "its limits started again: no more extra usage");
        // what `/limit-reset` last listed is given at rest, since when, with no `Use`; at a limit the same kinds take `Use`
        let listed = |kind: &str, use_by| coord_types::ResetOffer { kind: kind.into(), left: Some(2), use_by, ..Default::default() };
        let seen = now_s() - 60;
        for id in ["a", "b", "k"] {
            core.accounts.update(Harness::Claude, id, |s| crate::accounts::resets::noticed(s, vec![listed("Fable reset", None), listed("full reset", Some(now_s() - 1))], seen)).unwrap();
        }
        let page: coord_types::NativeAccounts = serde_json::from_str(&core.accounts_listing().reply(true).out).unwrap();
        let card = |id: &str| page.accounts.iter().find(|a| a.id == id).unwrap().clone();
        assert_eq!((card("b").held, card("b").held_seen, card("b").resets), (vec![listed("Fable reset", None)], Some(seen), Vec::new()), "at rest, one past its use-by left out");
        assert_eq!((card("a").held.len(), card("a").resets), (1, vec![listed("Fable reset", None)]));
        assert!(card("k").held.is_empty() && card("k").held_seen.is_none(), "none on a key account");
    }

    #[test]
    fn the_rule_says_out_and_signed_out_and_nothing_else() {
        let fine = State { signed_in: true, ..State::default() };
        assert_eq!(due(&fine, 100), None);
        assert_eq!(due(&State { out_until: Some(100), ..fine.clone() }, 100), None, "the time has come: it can take requests again");
        assert!(due(&State { out_until: Some(200), ..fine.clone() }, 100).unwrap().contains("out until"));
        assert!(due(&State::default(), 100).unwrap().contains("not signed in"));
    }

    #[test]
    fn an_agent_goes_through_the_route_until_the_owner_says_native_and_then_gets_an_account_at_each_start() {
        let t = Tmp::new("native");
        let mut core = core(&t, "claude-opus-5-5:high");
        // not native: roster is told nothing, and starts it as ever
        assert_eq!(core.account_for(&Who::Owner, "w"), ok(String::new()));
        assert!(!core.native(&Who::Agent("w".into()), "w", true).ok, "the owner's only");
        let r = core.native(&Who::Owner, "w", true);
        assert!(r.ok && r.out.contains("restarted on its session"), "{}", r.out);
        assert_eq!(core.to_restart().len(), 1, "it runs and is not in a turn: restarted now");
        // the restart's roster asks: account a (the higher tier), its folder
        let r = core.account_for(&Who::Owner, "w");
        assert_eq!(folder(r), t.store.config(Harness::Claude, "a").display().to_string());
        assert_eq!(core.view("w").account.as_deref(), Some("a"));
        // recorded, and read back by the next coordd
        let reg = std::fs::read_to_string(t.dir.join("research/registry.jsonl")).unwrap();
        assert!(reg.contains("\"ev\":\"native\"") && reg.contains("\"ev\":\"account\"") && reg.contains("\"account\":\"a\""), "{reg}");
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let again = Core::load(&t.dir.join("research").display().to_string(), &base);
        assert!(again.agents["w"].native && again.agents["w"].account.as_deref() == Some("a"));
    }

    /// A second Claude Code agent of CORE, running, and where it runs.
    fn another(core: &mut Core, name: &str) {
        assert!(core.add(&Who::Owner, name, "claude", "claude-opus-5-5").ok);
        core.agents.get_mut(name).unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
    }

    #[test]
    fn the_switch_puts_every_agent_on_the_accounts_at_its_turns_end_and_turns_on_by_itself_once() {
        let t = Tmp::new("all");
        let mut core = core(&t, "claude-opus-5-5");
        another(&mut core, "v");
        // a Codex agent stays where it is, and one in a turn is not cut
        assert!(core.add(&Who::Owner, "x", "codex", "gpt-5.5").ok);
        core.agents.get_mut("v").unwrap().live.state = "working".into();
        core.agents.get_mut("v").unwrap().live.turn = 1;
        assert_eq!(core.accounts_all(&Who::Agent("w".into()), true), err("all agents: the owner's".into()));
        assert!(core.native_all.is_none());
        // by itself, at the first account that has signed in and can take requests (a, the first by id)
        assert_eq!(first_signed_in(&t.store), Some(("a".to_string(), false)));
        // no account turns it on where the coord has the gateway, a sign-in as little as a key (0.2 line 22: "a sign-in
        // never moves the team by itself"): nothing is written and nobody moves; it does where there is no gateway
        let route = t.dir.join("research/.proxy-key");
        std::fs::write(&route, "made-up-route-key\n").unwrap();
        let registry = || std::fs::read_to_string(t.dir.join("research/registry.jsonl")).unwrap_or_default();
        let before = registry();
        core.all_by_itself("k", true);
        core.all_by_itself("a", false);
        assert!(core.native_all.is_none(), "an account moves nobody by itself beside the gateway");
        assert!(!core.agents["w"].native && !core.agents["v"].native && core.to_restart().is_empty());
        assert_eq!(registry(), before, "no native_all line, no agent's line");
        std::fs::remove_file(&route).unwrap();
        core.all_by_itself("a", false);
        let all = core.native_all.clone().unwrap();
        assert!(all.on && all.by_account.as_deref() == Some("a") && (now_s() - all.at).abs() < 5, "{all:?}");
        assert!(core.agents["w"].native && core.agents["v"].native && !core.agents["x"].native);
        // w, not in a turn, restarts now; v when its turn has ended
        assert_eq!(core.to_restart(), vec![("w".to_string(), core.agents["w"].live.pid)]);
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), t.store.config(Harness::Claude, "a").display().to_string());
        restarted(&mut core);
        assert!(core.to_restart().is_empty(), "v is in its turn");
        core.agents.get_mut("v").unwrap().ended_turn = 1;
        assert_eq!(core.to_restart().len(), 1);
        // a new agent starts there
        another(&mut core, "u");
        assert!(core.agents["u"].native);
        // the page is told, and the registry gives the next coordd the same switch and the same agents
        let page: coord_types::NativeAccounts = serde_json::from_str(&core.accounts_listing().reply(true).out).unwrap();
        assert_eq!(page.all, Some(all.clone()));
        assert!(!page.gateway, "this coord has no route's key");
        let load = |t: &Tmp| Core::load(&t.dir.join("research").display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
        let again = load(&t);
        assert_eq!(again.native_all, Some(all));
        assert_eq!(["w", "v", "u", "x"].map(|n| again.agents[n].native), [true, true, true, false]);
        // he turns it off: every agent back through the gateway, and it never turns on by itself again
        let r = core.accounts_all(&Who::Owner, false);
        assert!(r.ok && r.out.starts_with("all agents: on the gateway; "), "{}", r.out);
        assert!(["w", "v", "u"].iter().all(|n| !core.agents[*n].native));
        core.all_by_itself("a", false);
        assert_eq!(core.native_all.as_ref().map(|a| (a.on, a.by_account.clone())), Some((false, None)));
        another(&mut core, "s");
        assert!(!core.agents["s"].native);
        let again = load(&t);
        assert!(again.native_all.is_some_and(|a| !a.on) && again.agents.values().all(|a| !a.native));
        // on again by his click; with no account signed in it is refused and nothing changes
        assert!(core.accounts_all(&Who::Owner, true).out.starts_with("all agents: on your accounts; "));
        assert!(core.agents["w"].native && core.native_all.as_ref().is_some_and(|a| a.on && a.by_account.is_none()));
        // one agent the other way stays the exception
        assert!(core.native(&Who::Owner, "u", false).ok);
        assert!(!core.agents["u"].native && core.agents["w"].native);
        core.accounts_all(&Who::Owner, false);
        for id in ["a", "b"] {
            t.store.update(Harness::Claude, id, |s| s.signed_in = false).unwrap();
        }
        assert_eq!(first_signed_in(&t.store), None);
        // a key account whose key is set is found, after any sign-in account, and said to run on a key
        assert!(core.account_new(&Who::Owner, "k", Some("Work API"), 20, true, None, None).ok);
        t.store.update(Harness::Claude, "k", |s| s.signed_in = true).unwrap();
        assert_eq!(first_signed_in(&t.store), Some(("Work API".to_string(), true)));
        t.store.update(Harness::Claude, "b", |s| s.signed_in = true).unwrap();
        assert_eq!(first_signed_in(&t.store), Some(("b".to_string(), false)));
        for id in ["b", "k"] {
            t.store.update(Harness::Claude, id, |s| s.signed_in = false).unwrap();
        }
        assert_eq!(core.accounts_all(&Who::Owner, true), err("all agents: sign in to an account first".into()));
        assert!(core.native_all.as_ref().is_some_and(|a| !a.on));
    }

    /// The switch off, an agent through the gateway whose model the list no longer names goes onto his accounts with its
    /// row saying why. A list naming its model, no account signed in, a Codex agent or a failed read moves nobody; one in
    /// a turn moves when the turn ends.
    #[test]
    fn a_gateway_list_without_its_model_puts_an_agent_on_his_accounts_and_its_row_says_why() {
        let t = Tmp::new("gateway-lost");
        let mut core = core(&t, "claude-sonnet-5-5:high");
        another(&mut core, "v");
        // u does not run: it keeps the why for the move its next start makes
        assert!(core.add(&Who::Owner, "u", "claude", "claude-sonnet-5-5").ok && !core.agents["u"].running());
        core.agents.get_mut("v").unwrap().live.turn = 1;
        core.agents.get_mut("v").unwrap().live.state = "working".into();
        assert!(core.add(&Who::Owner, "x", "codex", "gpt-5.5").ok);
        std::fs::write(t.dir.join("research/.proxy-key"), "made-up-route-key\n").unwrap();
        assert!(!core.agents["w"].native && !core.agents["v"].native && core.native_all.is_none());
        core.gateway_lost(&["claude-sonnet-5-5".into(), "claude-opus-5-5".into(), "gpt-5.5".into()]);
        assert!(!core.agents["w"].native && !core.agents["v"].native && core.to_restart().is_empty(), "its model is served");
        for id in ["a", "b"] {
            t.store.update(Harness::Claude, id, |s| s.signed_in = false).unwrap();
        }
        core.gateway_lost(&["gpt-6.2-sol".into()]);
        assert!(!core.agents["w"].native && !core.agents["v"].native, "no account of his to go to");
        for id in ["a", "b"] {
            t.store.update(Harness::Claude, id, |s| s.signed_in = true).unwrap();
        }
        core.gateway_lost(&["gpt-6.2-sol".into()]);
        assert!(core.agents["w"].native && core.agents["v"].native && !core.agents["x"].native);
        let why = "the gateway no longer serves claude-sonnet-5-5";
        assert_eq!(core.account_note(&core.agents["w"]), Some(format!("moves when not in a turn: {why}")));
        assert_eq!(core.account_note(&core.agents["v"]), Some("moves when not in a turn: the gateway no longer serves claude-opus-5-5".into()));
        assert_eq!(core.to_restart(), vec![("w".to_string(), core.agents["w"].live.pid)], "v is in its turn");
        let registry = std::fs::read_to_string(t.dir.join("research/registry.jsonl")).unwrap();
        assert!(registry.lines().any(|l| l.contains(r#""ev":"native","name":"w","on":true"#)), "{registry}");
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), t.store.config(Harness::Claude, "a").display().to_string());
        assert_eq!(core.agents["w"].account_move.as_ref().map(|m| m.why.as_str()), Some(why));
        // u, not running: the why is its, in the registry too, and its start's move says it rather than "started"
        assert!(core.agents["u"].native && core.agents["u"].native_why.as_deref() == Some(why) && core.account_note(&core.agents["u"]).is_none());
        let load = || Core::load(&t.dir.join("research").display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
        assert_eq!(load().agents["u"].native_why.as_deref(), Some(why), "read back by the next coordd");
        assert!(core.account_for(&Who::Owner, "u").ok);
        assert_eq!((core.agents["u"].account_move.as_ref().map(|m| m.why.as_str()), core.agents["u"].native_why.as_deref()), (Some(why), None));
        let again = load();
        assert_eq!((again.agents["u"].account_move.as_ref().map(|m| m.why.as_str()), again.agents["u"].native_why.as_deref()), (Some(why), None), "its move is the last line");
    }

    /// The stand-in gateway's lists, with claude-sonnet-5-5 and without it.
    const WITH: &[(&str, &str)] = &[("claude-sonnet-5-5", "anthropic"), ("gpt-6.2-sol", "openai")];
    const WITHOUT: &[(&str, &str)] = &[("gpt-6.2-sol", "openai")];

    /// One read of LIST from the stand-in gateway, as main.rs `read_models` makes it: through the router, and
    /// `gateway_lost` with the ids it gives, none while every model counts as served.
    fn read_list(core: &mut Core, list: &'static [(&'static str, &'static str)]) {
        crate::clean::tests::stand_in_serving(list, |_| ("500 Internal Server Error", serde_json::json!({})));
        if let Some(ids) = router::Router::of(&core.places()).served().read.unwrap() {
            core.gateway_lost(&ids);
        }
    }

    /// The coord's first read of the gateway's list moves nobody: the ten minutes run from the first read.
    #[test]
    fn the_first_read_of_the_gateways_list_moves_nobody_until_ten_minutes_after_it() {
        let t = Tmp::new("gateway-first");
        let mut core = core(&t, "claude-sonnet-5-5");
        std::fs::write(t.dir.join("research/.proxy-key"), "made-up-route-key\n").unwrap();
        let coord = core.places().coord;
        assert!(!std::path::Path::new(&coord).join("gateway-models.json").exists(), "a fresh coord");
        read_list(&mut core, WITHOUT);
        assert!(!core.agents["w"].native && core.to_restart().is_empty() && router::gateway_serves(&coord, "claude-sonnet-5-5"), "the first read");
        router::age_reads(&coord, 300);
        read_list(&mut core, WITHOUT);
        assert!(!core.agents["w"].native && core.to_restart().is_empty() && router::gateway_serves(&coord, "claude-sonnet-5-5"), "five minutes on");
        router::age_reads(&coord, 300);
        read_list(&mut core, WITHOUT);
        assert!(core.agents["w"].native && !router::gateway_serves(&coord, "claude-sonnet-5-5"), "ten minutes after the first read");
        assert_eq!(core.account_note(&core.agents["w"]), Some("moves when not in a turn: the gateway no longer serves claude-sonnet-5-5".into()));
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
    }

    /// One read of the list that leaves a model out moves nobody when the next names it again.
    #[test]
    fn a_model_one_read_of_the_gateways_list_leaves_out_moves_nobody() {
        let t = Tmp::new("gateway-hiccup");
        let mut core = core(&t, "claude-sonnet-5-5");
        std::fs::write(t.dir.join("research/.proxy-key"), "made-up-route-key\n").unwrap();
        let coord = core.places().coord;
        read_list(&mut core, WITH);
        router::age_reads(&coord, 600);
        for list in [WITH, WITHOUT, WITH] {
            read_list(&mut core, list);
            assert!(!core.agents["w"].native && core.to_restart().is_empty() && router::gateway_serves(&coord, "claude-sonnet-5-5"), "{list:?}");
        }
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
    }

    /// A model with its `[1m]` variant is its base id where it meets a list, through the gateway and on his accounts.
    #[test]
    fn a_models_variant_is_its_base_id_in_the_gateways_list_and_his_accounts_run_it() {
        let t = Tmp::new("variant");
        let mut core = core(&t, "claude-fable-5-2[1m]");
        std::fs::write(t.dir.join("research/.proxy-key"), "made-up-route-key\n").unwrap();
        core.gateway_lost(&["claude-fable-5-2".into(), "gpt-6.2-sol".into()]);
        assert!(!core.agents["w"].native && core.to_restart().is_empty(), "its base id is listed");
        core.native(&Who::Owner, "w", true);
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), t.store.config(Harness::Claude, "a").display().to_string());
    }

    #[test]
    fn with_the_gateway_an_agent_whose_accounts_are_all_out_goes_on_through_it_and_returns_only_to_one_that_is_back() {
        let t = Tmp::new("gateway");
        let mut core = core(&t, "claude-opus-5-5");
        // this coord has the gateway: the route's key file, as roster looks for it (a made-up word; never read)
        std::fs::write(t.dir.join("research/.proxy-key"), "made-up\n").unwrap();
        let (h, on) = (Harness::Claude, |id: &str| t.store.config(Harness::Claude, id).display().to_string());
        core.native(&Who::Owner, "w", true);
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), on("a"));
        restarted(&mut core);
        // both accounts are out, a for an hour and b for two; w's turn was cut by a's limit
        let back = now_s() + 3600;
        t.store.update(h, "a", |s| s.out_until = Some(back)).unwrap();
        t.store.update(h, "b", |s| s.out_until = Some(back + 3600)).unwrap();
        core.agents.get_mut("w").unwrap().live.account_cut = Some(now_s() - 5);
        core.account_seen("a");
        assert_eq!(core.to_restart().len(), 1);
        // the restart's roster is told no folder, which is the route's start, and to go on with the cut turn
        let r = core.account_for(&Who::Owner, "w");
        assert!(r.ok && r.out.starts_with("\nCLAUDE_CODE_RESUME_INTERRUPTED_TURN=1\n") && !r.out.contains("OTEL"), "{:?}", r.out);
        let w = &core.agents["w"];
        assert!(w.native && w.account.is_none() && w.account_wait.is_none() && w.account_resumed, "still the accounts', on none, and not waiting");
        assert_eq!(w.gateway.as_ref().map(|g| (g.left.as_deref(), g.back)), Some((Some("a"), back)));
        assert!(t.store.bound_to(h, "research", "w").is_none(), "bound to no account meanwhile");
        restarted(&mut core);
        core.begin_turn("w");
        let w = core.agents.get_mut("w").unwrap();
        (w.ended_turn, w.live.state) = (w.live.turn, "idle".into());
        // the card of the account it left names it; the listing says where it runs
        let page: coord_types::NativeAccounts = serde_json::from_str(&core.accounts_listing().reply(true).out).unwrap();
        assert!(page.gateway);
        let card = |id: &str| page.accounts.iter().find(|a| a.id == id).unwrap();
        assert_eq!(card("a").on_gateway.iter().map(|g| g.agent.as_str()).collect::<Vec<_>>(), ["w"]);
        assert!(card("a").agents.is_empty() && card("b").on_gateway.is_empty());
        assert!(core.accounts_listing().reply(false).out.contains("w: through the gateway while no account can take it (until "), "{}", core.accounts_listing().reply(false).out);
        // its own view: on the owner's accounts still, on none of them now, and why
        let view = core.view("w");
        assert!(view.native && view.account.is_none() && view.account_note.is_some_and(|n| n.starts_with("through the gateway while no account can take it (until ")));
        // it does not go back and forth: nothing restarts it while no account can take requests, however often the
        // round looks, and a look that was asked for (something new of an account) finds the same
        assert!(core.to_restart().is_empty());
        core.account_seen("b");
        assert!(core.to_restart().is_empty() && core.agents["w"].live.account_due.is_none());
        assert_eq!(core.agents["w"].gateway.as_ref().map(|g| g.back), Some(back), "the next look is at the first reset");
        // the next coordd knows it by the registry and looks at once
        let again = Core::load(&t.dir.join("research").display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
        assert_eq!(again.agents["w"].gateway.as_ref().map(|g| (g.left.as_deref(), g.back)), Some((Some("a"), 0)));
        assert!(again.agents["w"].native && again.agents["w"].account.is_none());
        // a is back (the owner cleared it): w is marked, restarts when not in a turn, and is on a again
        assert!(core.account_clear(&Who::Owner, "a").ok);
        assert_eq!(core.to_restart().len(), 1);
        assert_eq!(core.agents["w"].live.account_due.as_deref(), Some("an account can take it again"));
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), on("a"));
        assert!(core.agents["w"].gateway.is_none() && core.agents["w"].account.as_deref() == Some("a"));
        restarted(&mut core);
        // out again, and through the gateway again; put back on the gateway by the owner it is there already
        t.store.update(h, "a", |s| s.out_until = Some(back)).unwrap();
        core.account_seen("a");
        assert_eq!(core.to_restart().len(), 1);
        assert!(core.account_for(&Who::Owner, "w").out.starts_with('\n'));
        restarted(&mut core);
        let r = core.native(&Who::Owner, "w", false);
        assert!(r.ok && r.out.contains("it runs through it already"), "{}", r.out);
        assert!(core.to_restart().is_empty() && core.agents["w"].gateway.is_none() && !core.agents["w"].native);
        // without the gateway it waits, as before
        std::fs::remove_file(t.dir.join("research/.proxy-key")).unwrap();
        core.native(&Who::Owner, "w", true);
        let r = core.account_for(&Who::Owner, "w");
        assert!(!r.ok && r.out.contains("every account is out"), "{}", r.out);
        assert!(core.agents["w"].gateway.is_none() && core.agents["w"].account_wait.is_some());
    }

    #[test]
    fn an_account_that_runs_out_moves_its_agent_between_turns_and_all_out_waits() {
        let t = Tmp::new("move");
        let mut core = core(&t, "claude-opus-5-5");
        core.native(&Who::Owner, "w", true);
        core.account_for(&Who::Owner, "w");
        restarted(&mut core);
        assert!(core.to_restart().is_empty());

        // a is out: the rule marks w at its turn's end, and the restart's choice is b
        let later = now_s() + 3600;
        t.store.update(Harness::Claude, "a", |s| s.out_until = Some(later)).unwrap();
        core.account_turn_ended("w");
        assert!(core.agents["w"].live.account_due.as_deref().unwrap().contains("out until"));
        assert!(core.view("w").account_note.unwrap().contains("moves when not in a turn"));
        assert_eq!(core.to_restart().len(), 1);
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), t.store.config(Harness::Claude, "b").display().to_string());
        let reg = std::fs::read_to_string(t.dir.join("research/registry.jsonl")).unwrap();
        assert!(reg.contains("\"account\":\"b\"") && reg.contains("\"from\":\"a\"") && reg.contains("out until"), "the move is recorded with where from and why: {reg}");

        // b out too: the restart is refused before the session is ended, w waits and says so, and is not tried again
        restarted(&mut core);
        t.store.update(Harness::Claude, "b", |s| s.out_until = Some(later + 60)).unwrap();
        core.account_seen("b");
        assert_eq!(core.to_restart().len(), 1);
        let r = core.account_for(&Who::Owner, "w");
        assert!(!r.ok && r.out.contains("every account is out"), "{}", r.out);
        let pid = core.agents["w"].live.pid;
        core.restarted("w", pid, "roster restart failed");
        assert!(core.agents["w"].live.account_due.is_some(), "still due");
        assert!(core.to_restart().is_empty(), "not before the first account is back");
        assert!(core.view("w").account_note.unwrap().contains("every account is out"));
        assert!(core.accounts_listing().reply(false).out.contains("w: every account is out"), "{}", core.accounts_listing().reply(false).out);
        // the page's form: w on b with its note, the tiers and states as on file
        let page: coord_types::NativeAccounts = serde_json::from_str(&core.accounts_listing().reply(true).out).unwrap();
        assert_eq!(page.coord, "research");
        let b = page.accounts.iter().find(|a| a.id == "b").unwrap();
        assert_eq!((b.tier, b.signed_in, b.out_until), (0, true, Some(later + 60)));
        assert!(b.agents.iter().any(|x| x.agent == "w" && x.coord == "research" && x.note.as_deref().is_some_and(|n| n.contains("every account is out"))), "{b:?}");
        // the time has come and a is back, not w's own b: restarted, and the choice is a
        core.agents.get_mut("w").unwrap().account_wait = Some(now_s() - 1);
        t.store.update(Harness::Claude, "a", |s| s.out_until = None).unwrap();
        assert_eq!(core.to_restart().len(), 1);
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), t.store.config(Harness::Claude, "a").display().to_string());
    }

    /// Two starts of one agent at once: each choice is made off the core's lock, one after the other under the
    /// store's. The second keeps what the first bound; and where the first's account went out in between, the binding
    /// on file is the second's, and both answers name it, in whichever order they come.
    #[test]
    fn two_choices_for_one_agent_side_by_side_end_on_the_one_binding_on_file() {
        let t = Tmp::new("two-choices");
        let mut core = core(&t, "claude-opus-5-5");
        let h = Harness::Claude;
        core.native(&Who::Owner, "w", true);
        let config = |id: &str| t.store.config(h, id).display().to_string();
        // side by side, nothing between them: the second keeps the first's
        let (one, two) = (core.account_for_begin(&Who::Owner, "w", None).ok().unwrap(), core.account_for_begin(&Who::Owner, "w", None).ok().unwrap());
        let (one, two) = (one.run(), two.run());
        assert_eq!((folder(core.account_for_end(two)), folder(core.account_for_end(one))), (config("a"), config("a")));
        assert_eq!(t.store.list(h).iter().map(|e| e.bound.len()).collect::<Vec<_>>(), [1, 0], "one binding");
        // the first's account goes out between the two: the second binds b, and the first's answer, coming last, is b too
        let (one, two) = (core.account_for_begin(&Who::Owner, "w", None).ok().unwrap(), core.account_for_begin(&Who::Owner, "w", None).ok().unwrap());
        let one = one.run();
        t.store.update(h, "a", |s| s.out_until = Some(now_s() + 3600)).unwrap();
        let two = two.run();
        assert_eq!((folder(core.account_for_end(two)), folder(core.account_for_end(one))), (config("b"), config("b")));
        assert_eq!(core.agents["w"].account.as_deref(), Some("b"));
        assert_eq!(t.store.list(h).iter().map(|e| e.bound.len()).collect::<Vec<_>>(), [0, 1], "one binding, the later one");
    }

    #[test]
    fn an_agent_whose_own_account_is_back_goes_on_without_a_restart() {
        let t = Tmp::new("back");
        let mut core = core(&t, "claude-opus-5-5");
        core.native(&Who::Owner, "w", true);
        core.account_for(&Who::Owner, "w");
        restarted(&mut core);
        // both out: w, on a, waits
        let soon = now_s() + 3600;
        for id in ["a", "b"] {
            t.store.update(Harness::Claude, id, |s| s.out_until = Some(soon)).unwrap();
        }
        core.account_turn_ended("w");
        assert_eq!(core.to_restart().len(), 1);
        assert!(!core.account_for(&Who::Owner, "w").ok);
        let pid = core.agents["w"].live.pid;
        core.restarted("w", pid, "roster restart failed");
        assert!(core.to_restart().is_empty() && core.agents["w"].live.account_due.is_some());
        // a's time has come: w's session is on a still, so nothing is restarted and what is due to it is handed again
        core.agents.get_mut("w").unwrap().account_wait = Some(now_s() - 1);
        t.store.update(Harness::Claude, "a", |s| s.out_until = None).unwrap();
        assert!(core.to_restart().is_empty());
        assert!(core.agents["w"].live.account_due.is_none() && core.agents["w"].account_wait.is_none());
        assert_eq!(core.view("w").account_note, None);
    }

    #[test]
    fn what_a_session_says_of_its_account_is_kept_and_moves_its_agent() {
        let t = Tmp::new("said");
        let mut core = core(&t, "claude-opus-5-5");
        core.native(&Who::Owner, "w", true);
        core.account_for(&Who::Owner, "w");
        restarted(&mut core);
        let at = now_s() + 3600;
        // a session's hook input names the folder it runs under (coordc adds it)
        let on = |id: &str| t.store.config(Harness::Claude, id).display().to_string();
        let line = |id: &str, five: f64| json!({"claude_config_dir": on(id), "rate_limits": {"five_hour": {"used_percentage": five, "resets_at": at}, "seven_day": {"used_percentage": 41.0, "resets_at": at + 400_000}}});
        let state = |id: &str| t.store.get(Harness::Claude, id).unwrap().state;
        // one that names no account's folder says nothing of an account
        core.hook("w", "statusline", &json!({"rate_limits": {"five_hour": {"used_percentage": 100.0, "resets_at": at}}}));
        core.hook("w", "statusline", &json!({"claude_config_dir": t.dir.join("elsewhere/config"), "rate_limits": {"five_hour": {"used_percentage": 100.0, "resets_at": at}}}));
        assert_eq!((state("a").five_hour, state("b").five_hour), (None, None));
        // the status line after an answer: both windows, below their limits
        core.hook("w", "statusline", &line("a", 24.0));
        assert_eq!(state("a").five_hour.map(|w| w.used_percent), Some(24.0));
        assert!(state("a").out_until.is_none() && core.agents["w"].live.account_due.is_none());
        assert!(core.accounts_listing().reply(false).out.contains("signed in, 5h 24%, week 41%"), "{}", core.accounts_listing().reply(false).out);
        assert_eq!(state("b").five_hour, None);
        // full: a is out until the window's reset, and w is to move
        core.hook("w", "statusline", &line("a", 100.0));
        assert_eq!(state("a").out_until, Some(at));
        assert!(core.agents["w"].live.account_due.as_deref().unwrap().contains("out until"));
        assert_eq!(core.to_restart().len(), 1);
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), t.store.config(Harness::Claude, "b").display().to_string());
        // the old session's status line, handed over without being waited for, arrives after w was given b: it is of
        // a still, and b is as it was
        core.hook("w", "statusline", &line("a", 100.0));
        assert_eq!((state("b").five_hour, state("b").out_until), (None, None));
        restarted(&mut core);
        // on b a turn fails on a limit before any window was seen: out, and nothing says until when
        core.hook("w", "failed", &json!({"claude_config_dir": on("b"), "error": "rate_limit", "error_details": "429 Too Many Requests"}));
        assert!(limits::reset_unknown(&state("b")));
        assert!(core.accounts_listing().reply(false).out.contains("out with no reset time known"), "{}", core.accounts_listing().reply(false).out);
        assert_eq!(core.to_restart().len(), 1);
        assert!(!core.account_for(&Who::Owner, "w").ok, "a is out too");
        let pid = core.agents["w"].live.pid;
        core.restarted("w", pid, "roster restart failed");
        assert!(core.to_restart().is_empty());
        // that turn was cut by the limit: w's parent is not told, and w is restarted to go on with it once an account
        // is back, also when that is its own. The owner clears b: w starts under b again, with Claude Code's own
        // variable and a bound that covers the wait
        let cut = core.agents["w"].live.account_cut.expect("the turn was cut");
        assert!(!core.account_clear(&Who::Agent("w".into()), "b").ok, "the owner's only");
        assert!(core.account_clear(&Who::Owner, "b").ok);
        assert_eq!(core.to_restart().len(), 1);
        let r = core.account_for(&Who::Owner, "w");
        let lines: Vec<&str> = r.out.lines().collect();
        assert_eq!(lines[0], on("b"));
        assert!(lines.contains(&"CLAUDE_CODE_RESUME_INTERRUPTED_TURN=1") && lines.contains(&"-CLAUDE_CODE_RESUME_PROMPT"), "{}", r.out);
        let bound: i64 = lines.iter().find_map(|l| l.strip_prefix("CLAUDE_CODE_RESUME_INTERRUPTED_TURN_MAX_AGE_MS=")).unwrap().parse().unwrap();
        assert!(bound >= RESUME_MARGIN * 1000 && bound <= (now_s() - cut + RESUME_MARGIN) * 1000, "{bound}");
        // the process that takes its place has no cut turn: its next start removes the variable
        restarted(&mut core);
        assert!(core.agents["w"].live.account_cut.is_none());
        // until a turn of it begins its view says what it was restarted for: Claude Code may not take the turn up
        assert!(core.view("w").account_note.is_some_and(|n| n.starts_with("restarted to go on with the turn its account's limit cut")));
        core.begin_turn("w");
        assert!(!core.agents["w"].account_resumed);
        core.agents.get_mut("w").unwrap().live.state = "idle".into();
        assert_eq!(core.view("w").account_note, None);
        assert!(core.account_for(&Who::Owner, "w").out.lines().any(|l| l == "-CLAUDE_CODE_RESUME_INTERRUPTED_TURN"));
        // a server throttle says nothing of the account
        core.hook("w", "failed", &json!({"error": "rate_limit", "last_assistant_message": "Server is temporarily limiting requests"}));
        assert_eq!(state("b").out_until, None);
    }

    #[test]
    fn a_lock_another_coordd_holds_is_waited_for_off_the_cores_lock_and_a_start_that_found_no_account_is_made_later() {
        let t = Tmp::new("held");
        let mut core = core(&t, "claude-opus-5-5");
        core.native(&Who::Owner, "w", true);
        core.account_for(&Who::Owner, "w");
        restarted(&mut core);
        let (h, at) = (Harness::Claude, now_s() + 3600);
        let full = json!({"claude_config_dir": t.store.config(h, "a").display().to_string(), "rate_limits": {"five_hour": {"used_percentage": 100.0, "resets_at": at}}});
        let wait = std::time::Duration::from_millis(300);
        // a's lock is held when w's session says a is full: the core only says what is to be written, and goes on;
        // the write waits in a task of its own, and nothing is written or lost meanwhile
        let held = Lock::take(&t.store.dir(h, "a").join("lock")).unwrap();
        let keep = core.account_said("w", "statusline", &full).expect("a change to write");
        let store = t.store.clone();
        let write = std::thread::spawn(move || {
            let kept = keep.run(&store);
            (keep, kept)
        });
        std::thread::sleep(wait);
        assert!(!write.is_finished() && core.to_restart().is_empty(), "held still, and the core's round goes on without it");
        assert_eq!(t.store.get(h, "a").unwrap().state.out_until, None);
        drop(held);
        // let go: it is written, the core takes the result, and w is to move
        let (keep, kept) = write.join().unwrap();
        core.account_kept(&keep, kept);
        assert_eq!(t.store.get(h, "a").unwrap().state.out_until, Some(at));
        assert_eq!(core.to_restart().len(), 1);
        // the accounts' lock is held when its move asks for an account: the choice waits for it, off the core's
        // lock, and is made when it is let go: no refusal, no second try
        let held = Lock::take(&t.store.harness_dir(h).join("lock")).unwrap();
        let choosing = core.account_for_begin(&Who::Owner, "w", None).ok().expect("w runs on an account");
        let choice = std::thread::spawn(move || choosing.run());
        std::thread::sleep(wait);
        assert!(!choice.is_finished() && t.store.bound_to(h, "research", "w").unwrap().id == "a", "held still: not chosen, not refused");
        drop(held);
        assert_eq!(folder(core.account_for_end(choice.join().unwrap())), t.store.config(h, "b").display().to_string());
        assert!(core.agents["w"].account_wait.is_none());
        let pid = core.agents["w"].live.pid;
        restarted(&mut core);
        // taken off the owner's accounts while its choice was being made: the model route, and the binding taken back
        let choosing = core.account_for_begin(&Who::Owner, "w", None).ok().unwrap();
        core.agents.get_mut("w").unwrap().native = false;
        assert_eq!(core.account_for_end(choosing.run()), ok(String::new()));
        assert!(t.store.bound_to(h, "research", "w").is_none());
        core.agents.get_mut("w").unwrap().native = true;
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), t.store.config(h, "b").display().to_string());
        restarted(&mut core);
        // w does not run and both accounts are out: its start is refused, and coordd makes it when one is back
        core.exited("w", pid.unwrap());
        t.store.update(h, "b", |s| s.out_until = Some(at + 60)).unwrap();
        assert!(!core.account_for(&Who::Owner, "w").ok);
        let note = core.view("w").account_note.unwrap_or_default();
        assert!(note.starts_with("not running; it is started when an account can take it (until "), "{note}");
        assert!(core.to_restart().is_empty(), "not before one is back");
        assert!(core.account_clear(&Who::Owner, "b").ok);
        assert_eq!(core.to_restart(), vec![("w".to_string(), None)]);
        // a start that fails for another reason is not tried again and again
        core.restarted("w", None, "roster restart failed: no terminal");
        assert!(core.to_restart().is_empty());
        assert_eq!(core.view("w").account_note, None);
        // one that ends while it waits for an account is not started by coordd
        let a = core.agents.get_mut("w").unwrap();
        (a.live.pid, a.account_wait) = (pid, Some(now_s() - 1));
        core.exited("w", pid.unwrap());
        assert!(core.agents["w"].account_wait.is_none() && core.to_restart().is_empty());
    }

    #[test]
    fn a_key_account_takes_agents_once_its_key_file_is_there_and_the_key_is_in_no_answer() {
        use coord_types::KeyStep::{Removed, Set, Where};
        use std::os::unix::fs::PermissionsExt;
        let t = Tmp::new("key-agents");
        let mut core = core(&t, "claude-opus-5-5");
        let h = Harness::Claude;
        let r = core.account_new(&Who::Owner, "k", Some("Work API"), 20, true, None, None);
        assert_eq!(r, ok("account k: made, no key yet (coord account key k, in a terminal of your own)".into()));
        assert!(core.accounts_listing().reply(false).out.lines().any(|l| l.contains("Work API") && l.contains("no key")), "{}", core.accounts_listing().reply(false).out);
        // an agent, or whatever is typed in an agent's terminal, is refused before coordc reads anything
        assert_eq!(core.account_key(&Who::Agent("w".into()), "k", Where), err(coord_types::KEY_NOT_HERE.into()));
        // one kind of thing in one folder: no key for a sign-in account
        assert!(core.account_key(&Who::Owner, "a", Where).out.contains("runs on a sign-in, not on a key"));
        assert!(core.account_key(&Who::Owner, "none", Where).out.contains("no account none"));
        // nor for a word that is no id, though a key account's record lies where it points: no path, no file touched
        let planted = t.planted();
        std::fs::write(planted.join("key"), "made-up-planted\n").unwrap();
        for step in [Where, Set, Removed] {
            let r = core.account_key(&Who::Owner, "../planted", step);
            assert!(!r.ok && r.out.contains("no account ../planted"), "{}", r.out);
        }
        assert_eq!(std::fs::read_to_string(planted.join("key")).unwrap(), "made-up-planted\n");
        let file = t.store.key_file(h, "k");
        assert_eq!(core.account_key(&Who::Owner, "k", Where), ok(file.display().to_string()));
        // "set" is what the file says, not what coordc says
        let r = core.account_key(&Who::Owner, "k", Set);
        assert!(!r.ok && r.out.contains("not there or empty"), "{}", r.out);
        assert!(!t.store.state(h, "k").unwrap().signed_in);
        // coordc's part, by hand: the file, the owner's alone
        std::fs::write(&file, "made-up-key\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        // what an earlier key left (refused, out with no time) goes with it
        t.store.update(h, "k", |s| s.out_until = Some(crate::accounts::limits::UNKNOWN_RESET)).unwrap();
        assert_eq!(core.account_key(&Who::Owner, "k", Set), ok("account k: key set".into()));
        let state = t.store.state(h, "k").unwrap();
        assert!(state.signed_in && state.out_until.is_none() && state.auth_method.as_deref() == Some("api_key"), "{state:?}");
        // w, native, starts on k (the highest tier): roster is told where the key's file is, and no sign-in account's
        // answer has such a line
        assert!(core.native(&Who::Owner, "w", true).ok);
        let r = core.account_for(&Who::Owner, "w");
        assert!(r.out.lines().any(|l| l == format!("@key {}", file.display())), "{}", r.out);
        assert_eq!(folder(r.clone()), t.store.config(h, "k").display().to_string());
        let page = core.accounts_listing().reply(true).out;
        let listing = core.accounts_listing().reply(false).out;
        assert!(listing.lines().any(|l| l.contains("Work API") && l.contains("key set")), "{listing}");
        let cards: coord_types::NativeAccounts = serde_json::from_str(&page).unwrap();
        assert_eq!(cards.accounts.iter().filter(|a| a.key).map(|a| a.id.as_str()).collect::<Vec<_>>(), ["k"]);
        for said in [&r.out, &page, &listing, &std::fs::read_to_string(t.dir.join("research/registry.jsonl")).unwrap()] {
            assert!(!said.contains("made-up-key"), "{said}");
        }
        // the key is removed: w moves between turns, and its next start is on a sign-in account, with no key line
        restarted(&mut core);
        std::fs::remove_file(&file).unwrap();
        assert_eq!(core.account_key(&Who::Owner, "k", Removed), ok("account k: key removed".into()));
        assert!(!t.store.state(h, "k").unwrap().signed_in);
        assert!(core.agents["w"].live.account_due.is_some(), "w's account takes no agent any more");
        let r = core.account_for(&Who::Owner, "w");
        assert!(!r.out.contains("@key"), "{}", r.out);
        assert_eq!(folder(r), t.store.config(h, "a").display().to_string());
        // a key file that went without `coord account key k --remove`: seen at the start, said, and w is tried again
        restarted(&mut core);
        std::fs::write(&file, "made-up-key\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(core.account_key(&Who::Owner, "k", Set).ok);
        assert!(core.move_agent(&Who::Owner, "w", Some("k")).ok);
        std::fs::remove_file(&file).unwrap();
        let r = core.account_for(&Who::Owner, "w");
        assert!(!r.ok && r.out.contains("account k has no key any more"), "{}", r.out);
        assert!(!t.store.state(h, "k").unwrap().signed_in && core.agents["w"].account_wait.is_some());
    }

    #[test]
    fn a_key_account_out_for_its_rate_limit_is_taken_again_at_the_policys_time_and_a_refused_key_only_by_the_owner() {
        use std::os::unix::fs::PermissionsExt;
        let t = Tmp::new("key-again");
        let mut core = core(&t, "claude-opus-5-5");
        let h = Harness::Claude;
        // the shipped policy gives the number we start from
        assert_eq!(core.key_again_after(), Some(60));
        // a policy of the test's own from here, so that its number can change
        let policy = t.dir.join("base/policy");
        std::fs::create_dir_all(&policy).unwrap();
        // the shipped placement beside it, which every start on an account asks
        std::fs::copy(format!("{}/../../agent-base/policy/placement.eflint", env!("CARGO_MANIFEST_DIR")), policy.join("placement.eflint")).unwrap();
        let give = |core: &mut Core, facts: &str| {
            std::fs::write(policy.join("accounts.eflint"), format!("Fact key-again-after Identified by Int\n{facts}")).unwrap();
            core.base = t.dir.join("base").display().to_string();
            core.lapse_times = Default::default();
        };
        give(&mut core, "+key-again-after(60).\n");
        assert_eq!(core.key_again_after(), Some(60));
        // k, a key account with its key, is the only account that can take w
        t.store.create(h, "k", "Work API", 20, super::super::Kind::Key, "x").unwrap();
        let file = t.store.key_file(h, "k");
        std::fs::write(&file, "made-up-key\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(core.account_key(&Who::Owner, "k", coord_types::KeyStep::Set).ok);
        for id in ["a", "b"] {
            t.store.update(h, id, |s| s.signed_in = false).unwrap();
        }
        let on = t.store.config(h, "k").display().to_string();
        let state = || t.store.state(h, "k").unwrap();
        // W's turn ends on the account for WHY: it is restarted, finds no account and waits with no time known
        let fails = |core: &mut Core, error: &str, words: &str| {
            assert_eq!(folder(core.account_for(&Who::Owner, "w")), on);
            restarted(core);
            core.hook("w", "failed", &json!({"claude_config_dir": on, "error": error, "last_assistant_message": words}));
            assert_eq!(core.to_restart().len(), 1);
            assert!(!core.account_for(&Who::Owner, "w").ok);
            let pid = core.agents["w"].live.pid;
            core.restarted("w", pid, "roster restart failed");
            assert_eq!(core.agents["w"].account_wait, Some(limits::UNKNOWN_RESET));
            assert!(core.to_restart().is_empty());
        };
        // the file's "since" moved back by SECS, as if that long had passed, and the round looks
        let later = |core: &mut Core, secs: i64| {
            t.store.update(h, "k", |s| s.out_since = s.out_since.map(|at| at - secs)).unwrap();
            core.key_again = KeyAgain::look();
        };
        assert!(core.native(&Who::Owner, "w", true).ok);
        // k's card on the page
        let card = |core: &Core| {
            let page: coord_types::NativeAccounts = serde_json::from_str(&core.accounts_listing().reply(true).out).unwrap();
            page.accounts.into_iter().find(|a| a.id == "k").unwrap()
        };

        // its rate limit: out with no time named, why and since when on file
        fails(&mut core, "rate_limit", "API Error: Rate limit reached");
        let s = state();
        assert!(limits::reset_unknown(&s) && s.out_for.as_deref() == Some(limits::RATE_LIMIT) && s.out_since.is_some_and(|at| (now_s() - at).abs() < 5), "{s:?}");
        // the card is given why, since when, and when the policy takes it again
        let k = card(&core);
        assert_eq!((k.out_for.as_deref(), k.out_since, k.again_at), (Some("rate_limit"), s.out_since, s.out_since.map(|at| at + 60)));
        assert!(core.accounts_listing().reply(false).out.contains("out with no reset time known"));
        // 50 s: not yet. 61 s: taken again at the round, and w, which waited with no time, is started on it
        later(&mut core, 50);
        assert!(core.to_restart().is_empty() && limits::reset_unknown(&state()));
        later(&mut core, 11);
        assert_eq!(core.to_restart().len(), 1);
        let s = state();
        assert_eq!((s.out_until, s.out_for, s.out_since), (None, None, None));
        assert_ne!(s.limits_from, "policy", "taking it in is no reading of its windows");
        // the policy's word is written only if the account is still as the look found it (another coordd's agent
        // may have taken it and met its limit again meanwhile): here it is in, and nothing is written
        let word = Keep { name: String::new(), id: "k".into(), h, said: limits::Said::again(), now: now_s(), note: None, again: Some(60) };
        assert_eq!(word.run(&t.store), Ok(false));

        // an edit of the number holds for an account that is out already; with the fact taken away it stays out
        fails(&mut core, "rate_limit", "API Error: Rate limit reached");
        give(&mut core, "");
        assert_eq!(core.key_again_after(), None);
        later(&mut core, 100_000);
        assert!(core.to_restart().is_empty() && limits::reset_unknown(&state()));
        give(&mut core, "+key-again-after(100000).\n+key-again-after(200000).\n");
        assert_eq!(core.key_again_after(), Some(100_000), "the shortest holds");
        assert_eq!(core.to_restart().len(), 1, "the number changed: looked at with no other reason");
        assert_eq!(state().out_until, None);

        // a refused key, and no credit: out for the owner to clear, whatever the policy's number and the time passed
        give(&mut core, "+key-again-after(60).\n");
        for (error, words, why) in [("authentication_failed", "Invalid API key · Fix external API key", limits::REFUSED), ("billing_error", "Credit balance is too low", limits::NO_CREDIT)] {
            fails(&mut core, error, words);
            later(&mut core, 100_000);
            assert!(core.to_restart().is_empty());
            let s = state();
            assert!(limits::reset_unknown(&s) && s.out_for.as_deref() == Some(why) && s.out_words.as_deref() == Some(words), "{s:?}");
            // the card is given which of the two it was, with Claude Code's words, and no time of the policy's; the
            // listing names it
            let card = card(&core);
            assert_eq!((card.out_for.as_deref(), card.out_words.as_deref(), card.again_at), (Some(why), Some(words), None));
            let listing = core.accounts_listing().reply(false).out;
            assert!(listing.lines().any(|l| l.contains("Work API") && l.contains(if why == limits::REFUSED { "out, key refused" } else { "out, no credit" })), "{listing}");
            // `coord account clear k`: in again, nothing of its being out is left, and w is started
            assert!(core.account_clear(&Who::Owner, "k").ok);
            let s = state();
            assert_eq!((s.out_until, s.out_for, s.out_since, s.out_words), (None, None, None, None));
            assert_eq!(core.to_restart().len(), 1);
        }
        // a new key clears a refused one too
        fails(&mut core, "authentication_failed", "Invalid API key");
        assert!(core.account_key(&Who::Owner, "k", coord_types::KeyStep::Set).ok);
        assert_eq!((state().out_until, state().out_for), (None, None));
        assert_eq!(core.to_restart().len(), 1);
    }

    #[test]
    fn the_reset_a_turns_row_names_ends_a_wait_with_no_time_known() {
        let t = Tmp::new("row");
        let mut core = core(&t, "claude-opus-5-5");
        // a is the only account that is signed in
        t.store.update(Harness::Claude, "b", |s| s.signed_in = false).unwrap();
        core.native(&Who::Owner, "w", true);
        core.account_for(&Who::Owner, "w");
        restarted(&mut core);
        let on = |id: &str| t.store.config(Harness::Claude, id).display().to_string();
        let state = |id: &str| t.store.get(Harness::Claude, id).unwrap().state;
        // a turn fails on a's limit before any window was seen: out, no time known, and w waits without one
        core.hook("w", "failed", &json!({"claude_config_dir": on("a"), "error": "rate_limit", "error_details": "429 Too Many Requests"}));
        assert!(limits::reset_unknown(&state("a")));
        assert_eq!(core.to_restart().len(), 1);
        assert!(!core.account_for(&Who::Owner, "w").ok);
        let pid = core.agents["w"].live.pid;
        core.restarted("w", pid, "roster restart failed");
        assert_eq!(core.agents["w"].account_wait, Some(limits::UNKNOWN_RESET));
        assert!(core.to_restart().is_empty());
        // a row of a folder that is no account's says nothing
        let at = now_s() + 1800;
        // (as `Job::LimitRow` hands it on: what is to be written, then its write)
        let row = |core: &mut Core, config: serde_json::Value| {
            if let Some(keep) = core.account_said("w", "limit_row", &json!({"claude_config_dir": config, "resets_at": at, "window": "five_hour"})) {
                core.account_keep(keep);
            }
        };
        row(&mut core, json!(t.dir.join("elsewhere/config")));
        assert!(limits::reset_unknown(&state("a")));
        // the turn's row names the reset: a is out until then, and w is tried again and waits until then
        row(&mut core, json!(on("a")));
        assert_eq!(state("a").out_until, Some(at));
        assert_ne!(state("a").limits_from, "transcript", "a reset named is no reading of the windows");
        assert_eq!(core.to_restart().len(), 1);
        let r = core.account_for(&Who::Owner, "w");
        assert!(!r.ok && r.out.contains("the first is back at"), "{}", r.out);
        assert_eq!(core.agents["w"].account_wait, Some(at));
    }

    #[test]
    fn the_switch_is_not_turned_on_while_no_account_can_take_requests() {
        let t = Tmp::new("switch-none");
        let mut core = core(&t, "claude-opus-5-5");
        let h = Harness::Claude;
        // both accounts are signed in and out, and the coord has no gateway: on, the switch would leave every agent
        // standing
        let back = now_s() + 3600;
        for id in ["a", "b"] {
            t.store.update(h, id, |s| s.out_until = Some(back)).unwrap();
        }
        let r = core.accounts_all(&Who::Owner, true);
        assert!(!r.ok && r.out.starts_with("all agents: every account is out, the first until ") && r.out.ends_with("; no account can take requests now"), "{}", r.out);
        assert!(core.native_all.is_none() && !core.agents["w"].native && core.agents["w"].live.account_due.is_none(), "nothing changed");
        // by itself it does not turn on either
        assert!(first_signed_in(&t.store).is_none());
        // none signed in, or the only one that is on its way to being removed: the words say what to do, with the
        // gateway too
        t.store.update(h, "a", |s| (s.out_until, s.leaving) = (None, Some(1))).unwrap();
        t.store.update(h, "b", |s| s.signed_in = false).unwrap();
        assert_eq!(core.accounts_all(&Who::Owner, true), err("all agents: sign in to an account first".into()));
        std::fs::write(t.dir.join("research/.proxy-key"), "made-up\n").unwrap();
        t.store.update(h, "a", |s| (s.signed_in, s.leaving) = (false, None)).unwrap();
        assert_eq!(core.accounts_all(&Who::Owner, true), err("all agents: sign in to an account first".into()));
        // off is never refused
        assert!(core.accounts_all(&Who::Owner, false).ok);
        // with the gateway and every account out it turns on: the agents go on through the gateway, as at a limit
        for id in ["a", "b"] {
            t.store.update(h, id, |s| (s.signed_in, s.out_until) = (true, Some(back))).unwrap();
        }
        assert!(core.accounts_all(&Who::Owner, true).ok && core.agents["w"].native);
        // and without it, with one account that can take requests
        std::fs::remove_file(t.dir.join("research/.proxy-key")).unwrap();
        assert!(core.accounts_all(&Who::Owner, false).ok);
        t.store.update(h, "b", |s| s.out_until = None).unwrap();
        assert!(core.accounts_all(&Who::Owner, true).ok && core.agents["w"].native);
    }

    #[test]
    fn with_every_account_at_its_weekly_ceiling_a_start_goes_through_the_gateway_and_comes_back_once_when_a_week_starts_again() {
        let t = Tmp::new("ceiling");
        let mut core = core(&t, "claude-opus-5-5");
        std::fs::write(t.dir.join("research/.proxy-key"), "made-up\n").unwrap();
        let (h, on) = (Harness::Claude, |id: &str| t.store.config(Harness::Claude, id).display().to_string());
        let week = now_s() + 3600;
        for (id, used) in [("a", 85.0), ("b", 92.0)] {
            t.store.update(h, id, |s| (s.plan, s.seven_day) = (Some("max".into()), Some(coord_types::LimitWindow { used_percent: used, resets_at: week }))).unwrap();
        }
        core.native(&Who::Owner, "w", true);
        // no account has space (Max, 80): w starts through the gateway, bound to none, to be looked at when a week starts again
        let r = core.account_for(&Who::Owner, "w");
        assert!(r.ok && r.out.starts_with('\n'), "{:?}", r.out);
        assert_eq!(core.agents["w"].gateway.as_ref().map(|g| (g.left.as_deref(), g.back)), Some((None, week)));
        assert!(t.store.bound_to(h, "research", "w").is_none());
        let registry = std::fs::read_to_string(t.dir.join("research/registry.jsonl")).unwrap();
        assert!(registry.contains("the placement policy: no space at its plan's weekly ceiling; through the gateway meanwhile"), "{registry}");
        // its view says why it runs where it runs, and from no account; a load of the registry says the same
        let view = core.view("w");
        assert_eq!((view.account_why.as_deref(), view.account_from.as_deref(), view.account_until), (Some("the placement policy: no space at its plan's weekly ceiling; through the gateway meanwhile"), None, Some(week)));
        assert!(view.account_since.is_some());
        let again = Core::load(&t.dir.join("research").display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
        assert_eq!(again.view("w").account_why, view.account_why);
        restarted(&mut core);
        // nothing restarts it before then, however often the round looks
        assert!(core.to_restart().is_empty());
        core.gateway_look();
        assert!(core.to_restart().is_empty() && core.agents["w"].live.account_due.is_none());
        assert_eq!(core.agents["w"].gateway.as_ref().map(|g| g.back), Some(week));
        // a's week starts again (Claude Code says 3 % of a new one): at the look, w is marked, and its start lands on a
        t.store.update(h, "a", |s| s.seven_day = Some(coord_types::LimitWindow { used_percent: 3.0, resets_at: week + 7 * 86_400 })).unwrap();
        core.gateway_look();
        assert_eq!(core.to_restart().len(), 1);
        assert_eq!(core.agents["w"].live.account_due.as_deref(), Some("an account can take it again"));
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), on("a"));
        restarted(&mut core);
        assert!(core.agents["w"].gateway.is_none() && core.agents["w"].account.as_deref() == Some("a"));
        assert_eq!(core.view("w").account_why.as_deref(), Some("an account can take it again"));
        assert!(core.to_restart().is_empty(), "it moved once");
        // the owner's move off a finds b full: it stays on a, and says so
        let r = core.move_agent(&Who::Owner, "w", None);
        assert!(!r.ok && r.out.contains("no other account can take w now (the placement policy: no space at its plan's weekly ceiling); it stays where it is"), "{}", r.out);
        assert_eq!(t.store.bound_to(h, "research", "w").map(|e| e.id).as_deref(), Some("a"));
    }

    #[test]
    fn a_sign_ins_refused_turn_restarts_that_agent_on_it_and_a_second_puts_it_out_and_moves_its_agents() {
        let t = Tmp::new("refused");
        let mut core = core_of(&t, "claude-opus-5-5", &["w", "x"]);
        let (h, on) = (Harness::Claude, |id: &str| t.store.config(Harness::Claude, id).display().to_string());
        let state = |id: &str| t.store.state(h, id).unwrap();
        let refused = json!({"claude_config_dir": on("a"), "error": "authentication_failed", "last_assistant_message": "Please run /login · API Error: 401"});
        for name in ["w", "x"] {
            core.native(&Who::Owner, name, true);
            assert_eq!(folder(core.account_for(&Who::Owner, name)), on("a"));
            restarted_as(&mut core, name);
        }
        // w's turn is refused its login: a takes agents still, and only w restarts, on a, to go on with that turn
        core.hook("w", "failed", &refused);
        assert_eq!(state("a").refused.map(|r| r.agent).as_deref(), Some("w"));
        assert!(state("a").can_take(now_s()));
        assert_eq!(core.agents["w"].live.account_due.as_deref(), Some(REFUSED_TURN));
        assert!(core.agents["w"].live.account_cut.is_some() && core.agents["x"].live.account_due.is_none());
        assert_eq!(core.to_restart().into_iter().map(|(n, _)| n).collect::<Vec<_>>(), ["w"]);
        let r = core.account_for(&Who::Owner, "w");
        assert!(r.out.lines().any(|l| l == "CLAUDE_CODE_RESUME_INTERRUPTED_TURN=1"), "{}", r.out);
        assert_eq!(folder(r), on("a"));
        restarted_as(&mut core, "w");
        assert!(core.accounts_listing().reply(false).out.contains("tier 10\tsigned in"), "{}", core.accounts_listing().reply(false).out);
        // a turn of x on a ends answered: that refusal was w's session's
        core.hook("x", "stop", &json!({"claude_config_dir": on("a")}));
        assert!(state("a").refused.is_none());
        // refused again, a first one again; then x's turn too, with none answered between: a is out for the owner
        core.hook("w", "failed", &refused);
        assert!(state("a").can_take(now_s()));
        core.hook("x", "failed", &refused);
        let a = state("a");
        assert_eq!((a.out_until, a.out_for.as_deref(), a.out_words.as_deref()), (Some(limits::UNKNOWN_RESET), Some(limits::REFUSED), Some("Please run /login · API Error: 401")));
        assert!(core.accounts_listing().reply(false).out.contains("tier 10\tout, login refused"), "{}", core.accounts_listing().reply(false).out);
        // both move to b, and say why
        assert_eq!(core.to_restart().len(), 2);
        for name in ["w", "x"] {
            assert_eq!(folder(core.account_for(&Who::Owner, name)), on("b"));
            restarted_as(&mut core, name);
            assert_eq!((core.view(name).account_why.as_deref(), core.view(name).account_from.as_deref()), (Some(REFUSED_TURN), Some("a")));
        }
        // a status line of w's old session, drawn after the refusal, does not bring a back
        core.hook("w", "statusline", &json!({"claude_config_dir": on("a"), "rate_limits": {"five_hour": {"used_percentage": 12, "resets_at": now_s() + 3600}}}));
        assert_eq!(state("a").out_for.as_deref(), Some(limits::REFUSED));
        // the owner signs a in again (main.rs `signin_ended`, then its check): a takes agents again; who went to b stays
        t.store.update(h, "a", State::signed_in_again).unwrap();
        core.account_checked("a");
        assert!(state("a").can_take(now_s()) && state("a").refused.is_none());
        assert!(core.to_restart().is_empty(), "who went to b stays there");
    }

    #[test]
    fn deliveries_never_seen_before_and_after_the_first_ones_agent_restarts_make_an_account_deaf_and_move_only_its_agents_and_one_seen_between_clears_the_first() {
        let t = Tmp::new("deaf");
        let mut core = core_of(&t, "claude-opus-5-5", &["w", "x", "y"]);
        let (h, on) = (Harness::Claude, |id: &str| t.store.config(Harness::Claude, id).display().to_string());
        let state = |id: &str| t.store.state(h, id).unwrap();
        // w and x on a, y on b
        t.store.bind(h, "b", "research", "y", "now").unwrap();
        for (name, id) in [("w", "a"), ("x", "a"), ("y", "b")] {
            core.native(&Who::Owner, name, true);
            assert_eq!(folder(core.account_for(&Who::Owner, name)), on(id));
            restarted_as(&mut core, name);
        }
        // a delivery handed to w is never seen in its session: a miss; a is not known to take messages any more, and
        // takes agents still; only w restarts, on a, going on with its turn, to take the delivery again
        core.account_unseen("w");
        let a = state("a");
        assert_eq!((a.heard, a.missed.as_ref().map(|m| m.agent.as_str()), a.deaf.is_some()), (None, Some("w"), false));
        assert!(a.can_take(now_s()));
        assert_eq!((core.agents["w"].live.account_due.as_deref(), core.agents["w"].live.unseen_on.as_deref()), (Some(UNSEEN_TURN), Some("a")));
        assert!(core.agents["w"].live.account_cut.is_some() && core.agents["x"].live.account_due.is_none());
        // before w has restarted, a second miss, its own or x's, may be w's one session's: a is not deaf
        core.account_unseen("w");
        core.account_unseen("x");
        assert!(state("a").deaf.is_none() && state("a").can_take(now_s()));
        assert_eq!(core.to_restart().into_iter().map(|(n, _)| n).collect::<Vec<_>>(), ["w"]);
        let r = core.account_for(&Who::Owner, "w");
        assert!(r.out.lines().any(|l| l == "CLAUDE_CODE_RESUME_INTERRUPTED_TURN=1"), "{}", r.out);
        assert_eq!(folder(r), on("a"));
        restarted_as(&mut core, "w");
        // one seen in x's session: the miss was w's session's
        core.account_reached("x");
        let a = state("a");
        assert!(a.heard.is_some() && a.missed.is_none());
        // a delivery on an account known to take them writes nothing
        let written = || std::fs::metadata(t.store.dir(h, "a").join("state.json")).and_then(|m| m.modified()).unwrap();
        let before = written();
        core.account_reached("w");
        assert_eq!(written(), before);
        // a miss of w's, and w restarts on a; then one of x's, with none seen between: a is deaf, takes no agent, and
        // only its agents move, to b
        core.account_unseen("w");
        assert_eq!(core.to_restart().into_iter().map(|(n, _)| n).collect::<Vec<_>>(), ["w"]);
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), on("a"));
        restarted_as(&mut core, "w");
        assert!(state("a").deaf.is_none());
        core.account_unseen("x");
        let a = state("a");
        assert_eq!(a.deaf.as_ref().map(|d| d.agent.as_str()), Some("x"));
        assert!(!a.can_take(now_s()));
        assert!(core.accounts_listing().reply(false).out.contains("tier 10\tdeaf: deliveries never reached its sessions (the second x's)"), "{}", core.accounts_listing().reply(false).out);
        let page: coord_types::NativeAccounts = serde_json::from_str(&core.accounts_listing().reply(true).out).unwrap();
        assert_eq!(page.accounts.iter().find(|a| a.id == "a").unwrap().deaf.as_deref(), Some("x"));
        assert_eq!(core.agents["w"].live.account_due.as_deref(), Some(DEAF));
        assert!(core.agents["y"].live.account_due.is_none());
        let mut moving: Vec<String> = core.to_restart().into_iter().map(|(n, _)| n).collect();
        moving.sort();
        assert_eq!(moving, ["w", "x"]);
        for name in ["w", "x"] {
            assert_eq!(folder(core.account_for(&Who::Owner, name)), on("b"));
            restarted_as(&mut core, name);
            assert_eq!((core.view(name).account_why.as_deref(), core.view(name).account_from.as_deref()), (Some(DEAF), Some("a")));
        }
        // the owner clears a (its organisation's channels are on now): it takes agents again, not known to take
        // messages until one enters there
        assert!(core.account_clear(&Who::Owner, "a").ok);
        let a = state("a");
        assert!(a.can_take(now_s()) && a.deaf.is_none() && a.missed.is_none() && a.heard.is_none());
    }

    #[test]
    fn an_account_heard_at_last_brings_an_agent_on_the_gateway_back_at_the_next_round() {
        let t = Tmp::new("heard-back");
        let mut core = core_of(&t, "claude-opus-5-5", &["w", "x"]);
        std::fs::write(t.dir.join("research/.proxy-key"), "made-up\n").unwrap();
        let (h, on) = (Harness::Claude, |id: &str| t.store.config(Harness::Claude, id).display().to_string());
        // a just signed in, the only account
        t.store.update(h, "a", |s| s.heard = None).unwrap();
        t.store.update(h, "b", |s| s.signed_in = false).unwrap();
        for name in ["w", "x"] {
            core.native(&Who::Owner, name, true);
        }
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), on("a"));
        restarted_as(&mut core, "w");
        // a has an agent and is not known to take messages: x goes through the gateway, which says why
        let r = core.account_for(&Who::Owner, "x");
        assert!(r.ok && r.out.starts_with('\n'), "{:?}", r.out);
        assert_eq!(core.view("x").account_why.as_deref(), Some("the placement policy: messages not yet known to reach it, with agents on it already; through the gateway meanwhile"));
        restarted_as(&mut core, "x");
        assert!(core.to_restart().is_empty());
        // a delivery is seen in w's session: a is heard, and x is marked at the next round and lands on a
        core.account_reached("w");
        assert!(t.store.state(h, "a").unwrap().heard.is_some());
        assert_eq!(core.to_restart().into_iter().map(|(n, _)| n).collect::<Vec<_>>(), ["x"]);
        assert_eq!(core.agents["x"].live.account_due.as_deref(), Some("an account can take it again"));
        assert_eq!(folder(core.account_for(&Who::Owner, "x")), on("a"));
    }

    #[test]
    fn a_new_sign_in_brings_an_agent_on_the_gateway_back_at_the_next_round() {
        let t = Tmp::new("signin-back");
        let mut core = core(&t, "claude-opus-5-5");
        std::fs::write(t.dir.join("research/.proxy-key"), "made-up\n").unwrap();
        let (h, on) = (Harness::Claude, |id: &str| t.store.config(Harness::Claude, id).display().to_string());
        // w on a, the only account signed in; a goes out for hours, and w goes on through the gateway
        t.store.update(h, "b", |s| s.signed_in = false).unwrap();
        core.native(&Who::Owner, "w", true);
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), on("a"));
        restarted(&mut core);
        let back = now_s() + 7200;
        t.store.update(h, "a", |s| (s.out_until, s.out_for) = (Some(back), Some(limits::RATE_LIMIT.into()))).unwrap();
        core.account_seen("a");
        assert_eq!(core.to_restart().len(), 1);
        let r = core.account_for(&Who::Owner, "w");
        assert!(r.ok && r.out.starts_with('\n'), "{:?}", r.out);
        assert_eq!(core.agents["w"].gateway.as_ref().map(|g| (g.left.as_deref(), g.back)), Some((Some("a"), back)));
        restarted(&mut core);
        assert!(core.to_restart().is_empty());
        // the owner signs b in: its end records what Claude Code says (`signin::record`), and nothing has told the
        // core yet; the check's end does (main.rs `account_check`), and w is marked at the round and lands on b
        let st = crate::accounts::signin::Status { logged_in: true, auth_method: Some("claude.ai".into()), expires: None, shown: None, email: None, plan: Some("max".into()) };
        crate::accounts::signin::record(&t.store, h, "b", &st, now_s()).unwrap();
        assert!(core.to_restart().is_empty());
        core.account_checked("b");
        assert_eq!(core.to_restart().len(), 1);
        assert_eq!(core.agents["w"].live.account_due.as_deref(), Some("an account can take it again"));
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), on("b"));
        restarted(&mut core);
        assert!(core.agents["w"].gateway.is_none() && core.agents["w"].account.as_deref() == Some("b"));
    }

    #[test]
    fn an_account_being_removed_takes_no_agent_and_its_agents_leave_it_when_not_in_a_turn() {
        let t = Tmp::new("leaving");
        let mut core = core(&t, "claude-opus-5-5");
        let (h, on) = (Harness::Claude, |id: &str| t.store.config(Harness::Claude, id).display().to_string());
        core.native(&Who::Owner, "w", true);
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), on("a"));
        restarted(&mut core);
        // a is marked for removal while w runs on it: nothing to remove yet, and w is marked to move
        t.store.update(h, "a", |s| s.leaving = Some(now_s())).unwrap();
        assert!(core.accounts_seen(&seen(&t.store)).is_empty());
        assert_eq!(core.agents["w"].live.account_due.as_deref(), Some("its account is being removed"));
        let listed = core.accounts_listing().reply(false).out;
        assert!(listed.contains("a\ta\ttier 10\tbeing removed, when no agent runs on it\tresearch.w"), "{listed}");
        let page: coord_types::NativeAccounts = serde_json::from_str(&core.accounts_listing().reply(true).out).unwrap();
        let a = page.accounts.iter().find(|a| a.id == "a").unwrap();
        assert!(a.leaving.is_some() && a.locked_by.is_none() && a.agents.len() == 1);
        // its restart, when not in a turn, lands on b; a is free, and the round names it for removal
        assert_eq!(core.to_restart().len(), 1);
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), on("b"));
        restarted(&mut core);
        assert_eq!(core.accounts_seen(&seen(&t.store)), ["a"]);
        t.store.remove(h, "a", "20261118-230000").unwrap();
        // b too, the last account, where the coord has the gateway: w goes on through it and b is free
        std::fs::write(t.dir.join("research/.proxy-key"), "made-up\n").unwrap();
        t.store.update(h, "b", |s| s.leaving = Some(now_s())).unwrap();
        assert!(core.accounts_seen(&seen(&t.store)).is_empty());
        assert_eq!(core.to_restart().len(), 1);
        let r = core.account_for(&Who::Owner, "w");
        assert!(r.ok && r.out.starts_with('\n'), "{:?}", r.out);
        let w = &core.agents["w"];
        assert!(w.native && w.account.is_none() && w.gateway.as_ref().is_some_and(|g| g.left.as_deref() == Some("b")));
        assert!(t.store.bound_to(h, "research", "w").is_none());
        restarted(&mut core);
        assert_eq!(core.view("w").account_note.as_deref(), Some("through the gateway while no account can take it"));
        assert_eq!(core.accounts_seen(&seen(&t.store)), ["b"]);
        t.store.remove_left(h, "b", "20261118-230100").unwrap();
        // no account at all now: w stays on the gateway, with no time at which it is looked at again
        assert!(core.accounts_seen(&seen(&t.store)).is_empty() && core.to_restart().is_empty());
        assert_eq!(core.agents["w"].gateway.as_ref().map(|g| g.back), Some(limits::UNKNOWN_RESET));
        // the owner has an account again that can take requests (put back and signed in; a new one is the same to
        // the round): w is brought back to it at the round, when not in a turn, and once
        t.store.put_back(h, "a.20261118-230000").unwrap();
        t.store.update(h, "a", |s| s.signed_in = true).unwrap();
        assert!(core.accounts_seen(&seen(&t.store)).is_empty());
        assert_eq!(core.to_restart().len(), 1);
        assert_eq!(core.agents["w"].live.account_due.as_deref(), Some("an account can take it again"));
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), on("a"));
        restarted(&mut core);
        assert!(core.agents["w"].gateway.is_none() && core.view("w").account_note.is_none(), "{:?}", core.view("w").account_note);
        assert!(core.accounts_seen(&seen(&t.store)).is_empty() && core.to_restart().is_empty(), "it moved once");
    }

    /// A project with its own `claude-home` and no route: a new agent starts on his accounts though the switch was never
    /// set; with none signed in it waits. With the route's key, or without the folder, it goes through the route.
    #[test]
    fn a_new_projects_agent_with_no_route_starts_on_his_accounts_and_says_where_to_add_one() {
        let t = Tmp::new("own-home");
        let mut core = core(&t, "claude-opus-5-5");
        let (h, on) = (Harness::Claude, |id: &str| t.store.config(Harness::Claude, id).display().to_string());
        assert!(core.native_all.is_none() && !core.accounts_only(), "no claude-home: as before");
        another(&mut core, "before");
        assert!(!core.agents["before"].native);
        std::fs::create_dir(t.dir.join("research/claude-home")).unwrap();
        let route = t.dir.join("research/.proxy-key");
        std::fs::write(&route, "made-up-route-key\n").unwrap();
        assert!(!core.accounts_only(), "with the route's key its agents go through it");
        another(&mut core, "routed");
        assert!(!core.agents["routed"].native);
        std::fs::remove_file(&route).unwrap();
        assert!(core.accounts_only());
        another(&mut core, "n");
        assert!(core.agents["n"].native && core.native_all.is_none(), "on his accounts, the switch untouched");
        assert!(std::fs::read_to_string(t.dir.join("research/registry.jsonl")).unwrap().contains(r#""ev":"native","name":"n","on":true"#), "its registry line");
        assert_eq!(folder(core.account_for(&Who::Owner, "n")), on("a"));
        restarted(&mut core);
        // none signed in: it waits, and says where to add one, nothing of routes or logins
        core.agents.get_mut("n").unwrap().live = Live::default();
        for id in ["a", "b"] {
            t.store.update(h, id, |s| s.signed_in = false).unwrap();
        }
        let r = core.account_for(&Who::Owner, "n");
        assert!(!r.ok && r.out == "coord account: this project has no account yet: add one under Accounts; n is started when you do", "{}", r.out);
        assert_eq!(core.view("n").account_note.as_deref(), Some("not running: this project has no account yet: add one under Accounts"));
        // he turns the switch off: a new agent is not put there (roster then refuses its start, in the same words)
        core.accounts_all(&Who::Owner, false);
        another(&mut core, "off");
        assert!(!core.agents["off"].native);
    }

    #[test]
    fn an_agent_whose_start_finds_no_account_signed_in_waits_says_so_and_is_started_when_one_is() {
        let t = Tmp::new("none-waits");
        let mut core = core(&t, "claude-opus-5-5");
        let (h, on) = (Harness::Claude, |id: &str| t.store.config(Harness::Claude, id).display().to_string());
        core.native(&Who::Owner, "w", true);
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), on("a"));
        restarted(&mut core);
        // w does not run, and both sign-ins are gone
        core.agents.get_mut("w").unwrap().live = Live::default();
        for id in ["a", "b"] {
            t.store.update(h, id, |s| s.signed_in = false).unwrap();
        }
        let r = core.account_for(&Who::Owner, "w");
        assert!(!r.ok && r.out.contains("no account can take w; it is started when you sign in to one"), "{}", r.out);
        assert_eq!(core.view("w").account_note.as_deref(), Some("not running: no account can take it; it is started when you sign in to one"));
        assert!(core.accounts_seen(&seen(&t.store)).is_empty() && core.to_restart().is_empty(), "not tried while none is signed in");
        // he signs in to b: the round sees it, and w is started there
        t.store.update(h, "b", |s| s.signed_in = true).unwrap();
        core.accounts_seen(&seen(&t.store));
        assert_eq!(core.to_restart().iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["w"]);
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), on("b"));
        core.restarted("w", None, "");
        // w runs on b, and b is to be removed with nowhere for w to go (a is signed out, the coord has no gateway):
        // its restart is refused, it stays on b and waits, b's card names it with why, and b is not removed
        let pid = Some(std::process::id() as i32);
        core.agents.get_mut("w").unwrap().live = Live { pid, state: "idle".into(), ..Default::default() };
        t.store.update(h, "b", |s| s.leaving = Some(now_s())).unwrap();
        assert!(core.accounts_seen(&seen(&t.store)).is_empty());
        assert_eq!(core.to_restart().len(), 1);
        assert!(!core.account_for(&Who::Owner, "w").ok);
        core.restarted("w", pid, "refused");
        assert_eq!(t.store.bound_to(h, "research", "w").map(|e| e.id).as_deref(), Some("b"));
        assert!(core.accounts_seen(&seen(&t.store)).is_empty() && core.to_restart().is_empty(), "b stays while w waits on it");
        let page: coord_types::NativeAccounts = serde_json::from_str(&core.accounts_listing().reply(true).out).unwrap();
        let b = page.accounts.iter().find(|a| a.id == "b").unwrap();
        assert!(b.leaving.is_some());
        assert_eq!(b.agents.iter().map(|g| (g.agent.as_str(), g.note.as_deref())).collect::<Vec<_>>(), [("w", Some("no account can take it; it waits until you sign in to one (its account is being removed)"))]);
        // he signs in to a: w moves there when the round sees it, and b is free
        t.store.update(h, "a", |s| s.signed_in = true).unwrap();
        assert!(core.accounts_seen(&seen(&t.store)).is_empty());
        assert_eq!(core.to_restart().len(), 1);
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), on("a"));
        restarted(&mut core);
        assert_eq!(core.accounts_seen(&seen(&t.store)), ["b"]);
        t.store.remove(h, "b", "20261118-233000").unwrap();
        // a is to be removed while w, bound to it, was stopped: unbound at once, and a is free at the next look
        core.agents.get_mut("w").unwrap().live = Live::default();
        t.store.update(h, "a", |s| s.leaving = Some(now_s())).unwrap();
        assert!(core.accounts_seen(&seen(&t.store)).is_empty());
        assert!(t.store.bound_to(h, "research", "w").is_none() && core.agents["w"].account.is_none() && core.agents["w"].gateway.is_none());
        assert_eq!(core.accounts_seen(&seen(&t.store)), ["a"]);
    }

    #[test]
    fn native_is_refused_while_no_account_is_signed_in() {
        let t = Tmp::new("none");
        let mut core = core(&t, "claude-opus-5-5");
        for id in ["a", "b"] {
            t.store.update(Harness::Claude, id, |s| s.signed_in = false).unwrap();
        }
        let r = core.native(&Who::Owner, "w", true);
        assert!(!r.ok && r.out.contains("no account is signed in"), "{}", r.out);
        assert!(!core.agents["w"].native && core.agents["w"].live.account_due.is_none());
    }

    #[test]
    fn the_owner_moves_an_agent_and_only_a_claude_model_runs_natively() {
        let t = Tmp::new("owner-move");
        let mut core = core(&t, "claude-opus-5-5");
        assert!(!core.move_agent(&Who::Owner, "w", None).ok, "not native yet");
        core.native(&Who::Owner, "w", true);
        core.account_for(&Who::Owner, "w");
        restarted(&mut core);
        let r = core.move_agent(&Who::Owner, "w", None);
        assert!(r.ok && r.out.contains("account a → b"), "{}", r.out);
        assert_eq!(folder(core.account_for(&Who::Owner, "w")), t.store.config(Harness::Claude, "b").display().to_string());
        restarted(&mut core);
        assert!(!core.move_agent(&Who::Owner, "w", Some("nowhere")).ok);
        let r = core.move_agent(&Who::Owner, "w", Some("a"));
        assert!(r.ok && r.out.contains("account b → a"), "{}", r.out);
        // off again: unbound, and roster is told nothing
        assert!(core.native(&Who::Owner, "w", false).ok);
        assert_eq!(t.store.bound_to(Harness::Claude, "research", "w"), None);
        assert_eq!(core.account_for(&Who::Owner, "w"), ok(String::new()));

        let t2 = Tmp::new("owner-move-kiro");
        let mut other = core_on(&t2, "kiro-opus-5.5");
        let r = other.native(&Who::Owner, "w", true);
        assert!(!r.ok && r.out.contains("not one of Anthropic's own model names"), "{}", r.out);
    }

    fn core_on(t: &Tmp, model: &str) -> Core {
        core(t, model)
    }
}
