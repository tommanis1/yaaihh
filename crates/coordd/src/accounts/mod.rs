//! Accounts: a vendor's model in its own harness, run on the owner's own logins with nothing of ours in the network
//! path (the native path). Harnesses: Claude Code, opencode, agy.
//!
//! **An account is a folder.** Claude Code keeps one login per configuration folder (`CLAUDE_CONFIG_DIR`), and the
//! owner signs in there through Claude Code's own `claude auth login`. coordd never reads, writes, copies or refreshes a
//! login: it makes the `config/` folder and its `projects` link, and keeps what it knows in files of its own beside it.
//!
//! ```text
//! ROOT = $COORD_ACCOUNTS, else ~/agent-coord/accounts      the owner's accounts, shared by all his coordds
//! ROOT/claude/ID/account.json   ours: label, tier, kind (a sign-in, or an API key)       (Account)
//! ROOT/claude/ID/key            the owner's API key, for an account of kind key only: one line, 0600. Written by
//!                               coordc from his terminal (`coord account key ID`), read by Claude Code's own
//!                               `apiKeyHelper` (agent-base/claude/account-key). coordd reads only that it is there.
//! ROOT/claude/ID/state.json     ours: signed in or not, its plan, the limit windows, out until (State)
//! ROOT/claude/ID/bound/COORD/AGENT   ours: one file per agent that runs on this account  (Bound)
//! ROOT/claude/ID/config/        Claude Code's: its CLAUDE_CONFIG_DIR. Never read by us.
//! ROOT/claude/ID/config/projects -> ~/.claude/projects   a link we make before the first sign-in
//!                               ($COORD_CONVERSATIONS instead, where coordd is started with it: a try's own folder)
//! ROOT/claude/ID/config/.claude.json   Claude Code's. coordd never touches it; roster (bin/roster `home`) rewrites it
//!                               at each start to mark the agent's folder trusted and the first-start screens seen, as
//!                               it does with ~/.claude.json, under Claude Code's own lock on the file
//!                               (`.claude.json.lock`); nothing else of it is read or changed.
//! ROOT/claude/turn.json         ours: whose turn it is in each tier (round robin; the gateway by its place's name) (router.rs)
//! ROOT/removed/claude/ID.TIME/  an account the owner removed: its folder, moved here whole  (Store::remove)
//!
//! ROOT/opencode/ID/account.json ours, as Claude Code's, with the provider opencode signs in to (`opencode-go`)
//! ROOT/opencode/ID/state.json   ours: signed in or not, as `opencode auth list` says it  (opencode.rs)
//! ROOT/opencode/ID/bound/       ours, as Claude Code's
//! ROOT/opencode/ID/home/{data,config,state,cache,tmp}/   opencode's: its XDG folders and TMPDIR for every run on the
//!                               account ([`Store::session_env`]); its sign-in is in `data/opencode/auth.json`. Never
//!                               read by us. Made empty, 0700, before the first sign-in.
//! ROOT/removed/opencode/ID.TIME/  as for Claude Code
//! ```
//!
//! An account's id is one in all harnesses: a request names an account by its id alone, and the harness is the one
//! whose folder has it ([`Store::harness_of`]).
//!
//! **Why the link.** Claude Code finds a session to resume only under `projects/` of the folder it runs with. With
//! every account's `projects` the same folder, an agent's conversation stays where it is whichever account it runs on,
//! and a move is a restart: the same `--resume SESSION`, another `CLAUDE_CONFIG_DIR`.
//!
//! **The parts.**
//! - this file: the folders and coordd's own records of an account ([`Store`]);
//! - [`bind`]: where an agent runs, an account or the gateway, as the placement policy says (policy/placement.eflint:
//!   the first tier with space, round robin within it);
//! - [`mover`]: moving an agent to another account between turns, and, apart from it, the rule for when;
//! - [`signin`]: adding an account: Claude Code's own sign-in run for him, its URL shown by the app;
//! - [`opencode`]: an opencode account: opencode's own `auth login` on a terminal of its own, what he pasted written
//!   to its key prompt, and `auth list` for whether it is signed in;
//! - [`limits`]: what Claude Code says of an account's 5-hour and weekly windows, and that it is out;
//! - [`usage`], [`telemetry`]: one row per request with its agent, from the transcript and Claude Code's telemetry.
//!
//! Only agents of a project without a box run natively: a boxed agent is its own user and could not reach an
//! account folder.

pub mod router;
pub mod agy;
pub mod codex;
pub mod cells;
pub mod limits;
pub mod mover;
pub mod opencode;
pub mod opener;
pub mod plain;
pub mod resets;
pub mod signin;
pub mod telemetry;
pub mod usage;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The harnesses an account can belong to: Claude Code, opencode and agy ([`agy`]). Only Claude Code's accounts are
/// moved between (mover.rs); an opencode or agy account is chosen for an agent of its harness at its start and kept while
/// it runs, its folders given by [`Store::session_env`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Harness {
    Claude,
    Opencode,
    Agy,
}

impl Harness {
    /// Every harness, in the order the page lists their accounts.
    pub const ALL: [Harness; 3] = [Harness::Claude, Harness::Opencode, Harness::Agy];

    /// Its folder under the root, and its name in a request (`AccountNew.harness`) and on the page.
    pub fn dir(self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            Harness::Opencode => "opencode",
            Harness::Agy => "agy",
        }
    }

    /// The harness a request names (`claude`, `opencode`, `agy`); None for a word that is none of them.
    pub fn named(word: &str) -> Option<Harness> {
        Harness::ALL.into_iter().find(|h| h.dir() == word)
    }

    /// The harness as the owner knows it, for words.
    pub fn shown(self) -> &'static str {
        match self {
            Harness::Claude => "Claude Code",
            Harness::Opencode => "opencode",
            Harness::Agy => "agy",
        }
    }

    /// The program whose own sign-out an account of it in folder DIR has, for words: an agy account's is agy's own, or
    /// t3code's for one t3code signed in and agy's own sign-in has not since ([`agy::legacy`]); its key accounts have
    /// none.
    pub fn signs_out_of(self, dir: &Path) -> &'static str {
        match self {
            Harness::Agy if agy::legacy(dir) => "t3code",
            h => h.shown(),
        }
    }
}

/// `account.json`: what is set for an account. Its id is its folder's name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    /// What the owner calls it (the Accounts page shows it; the id stays).
    pub label: String,
    /// Its tier, the Accounts page's `Priority`: a larger number is taken first; accounts of one number are one tier.
    #[serde(default)]
    pub tier: i64,
    /// When it was added (RFC 3339).
    pub added: String,
    /// What it runs on: Claude Code's own sign-in in its folder (a file from before kinds says none and is this), or
    /// an API key in the file `key` beside this one.
    #[serde(default, skip_serializing_if = "Kind::is_signin")]
    pub kind: Kind,
    /// An opencode account's provider, as opencode names it (`opencode-go`, `openai`): what its sign-in is for
    /// (`opencode auth login -p PROVIDER`). None for Claude Code's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

/// What an account runs on ([`Account::kind`]). Tier, binding, the choice at an agent's start and the move between
/// turns are the same for both; they differ in what makes one usable and in what its limits are.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// The harness's own sign-in in `config/`: usable when the harness says it is signed in.
    #[default]
    Signin,
    /// An API key in the account's file `key`: usable when that file is there ([`Store::key_set`]). Its
    /// [`State::signed_in`] says that, so every rule that asks "can it take an agent" reads one field for both kinds.
    Key,
}

impl Kind {
    fn is_signin(&self) -> bool {
        *self == Kind::Signin
    }
}

/// One of an account's usage windows: the app's Accounts page reads it too, so it lives in coord-types.
pub use coord_types::LimitWindow as Window;

/// `state.json`: what coordd has seen of an account. Written by whichever of the owner's coordds saw it last
/// ([`Store::update`], under the account's lock); none of it is a secret.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    /// The harness's own answer to "is it signed in" (`claude auth status`), and when it was asked (seconds).
    #[serde(default)]
    pub signed_in: bool,
    #[serde(default)]
    pub signed_in_seen: i64,
    /// When the login runs out, if the harness said (seconds).
    #[serde(default)]
    pub login_expires: Option<i64>,
    /// The address of the person signed in, as Claude Code's `auth status` says it (`signin::Status::email`); None
    /// signed out, or when it said none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_in_as: Option<String>,
    /// The 5-hour and the weekly window, as last seen; None until a session on the account has said them.
    #[serde(default)]
    pub five_hour: Option<Window>,
    #[serde(default)]
    pub seven_day: Option<Window>,
    /// The month's window, as last seen: an OpenCode Go account's alone, from Go's usage page ([`limits::GO_USAGE`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub month: Option<Window>,
    /// Whether it has a 5-hour limit, as Claude Code last said (`limits::apply`: `get_usage` on the owner's press, the
    /// opening's `rate_limit_event`, any 5-hour window a session gave). None, as on state written before it was kept:
    /// not known, and the window opener takes it as having one, so its opening's event says (accounts/opener.rs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour_limit: Option<bool>,
    /// Its weeks of one model, as Claude Code's `get_usage` last named them (`limits::from_usage`): shown, not its limits.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model_weeks: Vec<coord_types::ModelWeek>,
    /// When `get_usage` last named them (seconds): the card's "as of", since nothing else says them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_weeks_seen: Option<i64>,
    /// Until when `get_usage` last said its requests run on extra usage (seconds; [`limits::Said::extra_usage`]): for the
    /// card alone. None when it said they do not, or never said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_usage_until: Option<i64>,
    /// When the windows were last seen (seconds), and from what ("statusline", "usage", "rate_limit_event"): set only
    /// by a signal that gave a window (`limits::apply`), so that the card's "as of" is the reading's time.
    #[serde(default)]
    pub limits_seen: i64,
    #[serde(default)]
    pub limits_from: String,
    /// It can take no request until then (seconds): a limit was met. None, or a time that has passed: it can.
    #[serde(default)]
    pub out_until: Option<i64>,
    /// While it is out: why (`limits::RATE_LIMIT`, `REFUSED`, `NO_CREDIT`; None when the signal did not say),
    /// since when (seconds), and the words Claude Code showed for it, for the card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out_for: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out_since: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out_words: Option<String>,
    /// When the owner put the account back after removing it (seconds; `Store::put_back`), while it has not signed
    /// in since: the removal had Claude Code sign it out, and the card says so. Cleared where a sign-in is recorded
    /// (`signin::record`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub put_back: Option<i64>,
    /// When its removal was asked while agents ran on it (seconds): it takes no agent, each coordd moves its agents off it
    /// at their turn's end (`mover::due`), and the first coordd that finds none left removes it. Cleared by the removal
    /// ([`Store::remove`]) and by `Keep it`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leaving: Option<i64>,
    /// When the owner turned it off (seconds; main.rs `account_store`, `Request::AccountEdit`): as while it is
    /// [`State::leaving`], it takes no agent and each coordd moves its own agents off it at the end of their turn, but
    /// it stays an account, and takes agents again once he turns it on. The gateway's `Disable` on his own accounts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub off: Option<i64>,
    /// How Claude Code says the account is signed in (`claude auth status`: `claude.ai` for a subscription login).
    #[serde(default)]
    pub auth_method: Option<String>,
    /// Its plan as Claude Code names it (`claude auth status`: [`signin::Status::plan`]), when it said one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    /// An opencode account's models, as opencode last named them for its provider while it was signed in
    /// ([`opencode::models`], at each check). The router serves them (`Router::served`), so New agent offers them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
    /// The models an agy account is out for ([`coord_types::ModelOut`]), each refused by its limit: it still takes the
    /// others ([`State::can_take_model`]) and none once all are out ([`State::models_all_out`]). Kept by `limits::apply`
    /// until each time; the owner's clear takes them all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models_out: Vec<coord_types::ModelOut>,
    /// When a delivery was last seen in a session on it (seconds; `Core::ticket_seen`). None after a sign-in and after
    /// a missed delivery ([`State::missed`]): it then takes one agent, whose deliveries decide (policy/placement.eflint).
    /// An account on file from before this field counts as heard at 0 ([`heard_before`]), so None is written, not left out.
    #[serde(default = "heard_before")]
    pub heard: Option<i64>,
    /// A delivery handed to a session on it was never seen there (no line of the session for it within a minute idle,
    /// no dialog standing: `Core::not_read`), and none has been seen in a session on it since. A second such while
    /// this stands makes it [`State::deaf`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missed: Option<Sign>,
    /// Two deliveries handed to sessions on it were never seen there, with none seen between: messages do not
    /// reach its sessions (on 20 Nov a Team seat whose organisation has channels off). It takes no agent and its agents
    /// move, until a delivery is seen in a session on it after all, it is signed in again, or the owner clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deaf: Option<Sign>,
    /// A sign-in account's login was refused to a turn (`authentication_failed`: 401 or 403 through all of Claude
    /// Code's own retries) and no turn on it has ended answered since. That agent restarts on it, as one session's
    /// login may be stale where the account's is not, and Claude Code is asked whether it is signed in. A second
    /// refused turn while this stands makes the account out ([`limits::REFUSED`]; `limits::apply`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<Sign>,
    /// When the window opener looks at it next (seconds; accounts/opener.rs): shared by all of the owner's coordds, so
    /// one looks and the others see it done. None: at the next round.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_look: Option<i64>,
    /// What the opener's last look found, in its words ("window open until 15:00"), said in the journal when it
    /// changes, as the gateway's opener does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_said: Option<String>,
    /// When the opener last sent its opening message (seconds), whatever came of it: no other within its idle wait
    /// while no window is known to run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_opened: Option<i64>,
    /// The resets Claude Code holds for it, as it last named them (accounts/resets.rs): the notice on an agent's screen
    /// when a turn met its limit, or `/limit-reset`'s list read on the owner's `Check limits`; since when
    /// (`resets_seen`, seconds); and the owner's last press of one ([`coord_types::ResetTry`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resets: Vec<coord_types::ResetOffer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_seen: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset: Option<coord_types::ResetTry>,
    /// When a look at its resets began (seconds; `resets::busy`, main.rs `resets_look`): one at a time among his
    /// coordds, on the account's one terminal of its own, and no press while it runs. Taken away at its end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_look: Option<i64>,
    /// The kinds the owner pressed `Use` on at its limit and Claude Code did not apply, each with when that limit ends
    /// (seconds; [`resets::limit_end`]): no longer offered ([`resets::offered`]) until then, until it is back in
    /// ([`State::back_in`]), or until a notice names its resets again ([`resets::noticed`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resets_refused: Vec<(String, i64)>,
}

/// [`State::heard`] of an account on file from before it was kept.
fn heard_before() -> Option<i64> {
    Some(0)
}

/// What one agent's session showed of its account, since when (seconds): the delivery never seen there
/// ([`State::missed`], [`State::deaf`]), the turn whose login was refused ([`State::refused`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sign {
    pub since: i64,
    pub agent: String,
}

impl State {
    /// It can take an agent now: signed in, not out, not out for every model it offers ([`State::models_all_out`]), not
    /// deaf, not turned off, and not on its way to being removed. For a choice that places no agent of one model; an
    /// agent's own is [`State::can_take_model`].
    pub fn can_take(&self, now: i64) -> bool {
        self.signed_in && self.leaving.is_none() && self.off.is_none() && self.deaf.is_none() && self.out_until.is_none_or(|t| t <= now) && self.models_all_out(now).is_none()
    }

    /// It can take an agent of MODEL (no `:EFFORT`) now: [`State::can_take`], and not out for MODEL.
    pub fn can_take_model(&self, model: &str, now: i64) -> bool {
        self.can_take(now) && self.model_out(model, now).is_none()
    }

    /// Until when it is out for MODEL ([`State::models_out`]), while that time is still to come.
    pub fn model_out(&self, model: &str, now: i64) -> Option<i64> {
        self.models_out.iter().find(|o| o.model == model && o.until > now).map(|o| o.until)
    }

    /// Every model it offers is out, until the first is back; never while its models are unknown.
    pub fn models_all_out(&self, now: i64) -> Option<i64> {
        self.models.iter().map(|m| self.model_out(m, now)).min().flatten()
    }

    /// Until when it takes no agent for being out (seconds), while that is still to come: its own `out_until`, or every
    /// model it offers out ([`State::models_all_out`]), whichever ends later.
    pub fn out_now(&self, now: i64) -> Option<i64> {
        self.out_until.filter(|t| *t > now).max(self.models_all_out(now))
    }

    /// The clear ("it can take requests"): [`State::back_in`], its models back, a deaf account tried again
    /// and an earlier refused turn forgotten.
    pub fn cleared(&mut self) {
        self.back_in();
        self.models_out.clear();
        (self.missed, self.deaf, self.refused) = (None, None, None);
    }

    /// The owner signed it in again (main.rs `signed_in_anew`): its login's refusals before count no more, and it is not
    /// out for them; whether messages reach it is not known again, as after a first sign-in. What else it is out for,
    /// a limit, stands. A reset Claude Code refused for this login ([`resets::login`]) is over, and its kind is offered
    /// again at this limit.
    pub fn signed_in_again(&mut self) {
        (self.refused, self.heard, self.missed, self.deaf) = (None, None, None, None);
        if self.out_for.as_deref() == Some(limits::REFUSED) {
            self.back_in();
        }
        if let Some(r) = self.reset.take_if(|r| r.login) {
            self.resets_refused.retain(|(kind, _)| *kind != r.kind);
        }
    }

    /// It can take requests again: `out_until` and what was kept of its being out go, the resets refused at its limit
    /// among them.
    pub fn back_in(&mut self) {
        (self.out_until, self.out_for, self.out_since, self.out_words) = (None, None, None, None);
        self.resets_refused.clear();
    }
}

/// An agent bound to an account: the file `bound/COORD/AGENT`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bound {
    /// The coord (its folder's name) and the agent in it.
    pub coord: String,
    pub agent: String,
    /// Since when (RFC 3339).
    pub since: String,
}

/// One account as the rest of coordd sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub harness: Harness,
    pub id: String,
    pub account: Account,
    pub state: State,
    pub bound: Vec<Bound>,
}

/// An account the owner removed, as its set-aside folder says ([`Store::removed`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Removed {
    pub id: String,
    /// The folder's name under `ROOT/removed/HARNESS`: `ID.STAMP`.
    pub name: String,
    pub label: String,
    /// When it was removed (seconds), from STAMP; 0 when the name's stamp is not a time.
    pub at: i64,
    /// Its folder still has the mark [`NOT_SIGNED_OUT`]: Claude Code has not signed it out there.
    pub not_signed_out: bool,
}

/// A file in a removed sign-in account's set-aside folder: Claude Code has not signed it out there yet. Written at
/// [`Store::remove`], taken away when the sign-out answered ([`Store::signed_out`]), so an unfinished removal says so.
const NOT_SIGNED_OUT: &str = "not-signed-out";

/// A file in an OpenCode Go account's folder, outside opencode's `home/`: the key the owner let coordd keep to read
/// Go's usage with and send its plain calls, as the one header curl sends (`opencode::go_usage`, `plain::call`). Written by `opencode::keep_usage_key`, mode
/// 600; never read back by coordd; deleted with the account ([`Store::remove`]).
const USAGE_KEY: &str = "usage-key";

/// The accounts' folder ([module](self) layout).
#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
    /// Where conversations are kept, which every account's `projects` links to: `~/.claude/projects`.
    projects: PathBuf,
    /// Why there is no store (`of_env` without a home folder): nothing is read or written then, and every change says
    /// this.
    fault: Option<String>,
}

impl Store {
    /// The owner's accounts: `$COORD_ACCOUNTS`, else `~/agent-coord/accounts`; conversations in `$COORD_CONVERSATIONS`,
    /// else `~/.claude/projects` (Claude Code's own place; the variable is for a try, whose stand-in writes its
    /// transcripts in a folder of the try's and never under the owner's `~/.claude`). Both are coordd's own
    /// environment, set by whoever starts it (`host-coordd`), not an agent's.
    /// Without `HOME` there is no such place: the store is then one that holds nothing and refuses every change.
    pub fn of_env() -> Store {
        let Some(home) = std::env::var("HOME").ok().filter(|h| h.starts_with('/')) else {
            return Store { root: PathBuf::new(), projects: PathBuf::new(), fault: Some("HOME is not set: the owner's accounts have no place".into()) };
        };
        let root = std::env::var("COORD_ACCOUNTS").ok().filter(|r| !r.is_empty()).unwrap_or_else(|| format!("{home}/agent-coord/accounts"));
        let projects = std::env::var("COORD_CONVERSATIONS").ok().filter(|p| p.starts_with('/')).unwrap_or_else(|| format!("{home}/.claude/projects"));
        Store::at(root, projects)
    }

    /// The accounts under ROOT, their conversations in PROJECTS.
    pub fn at(root: impl Into<PathBuf>, projects: impl Into<PathBuf>) -> Store {
        Store { root: root.into(), projects: projects.into(), fault: None }
    }

    /// An Err when there is no store (`of_env`).
    pub fn ready(&self) -> Result<(), String> {
        self.fault.clone().map_or(Ok(()), Err)
    }

    /// An Err when there is no store, or no folder for account ID's `projects` to link to. That folder is under the
    /// owner's `~/.claude`, where coordd makes nothing, so without it no account is made or started on.
    fn conversations(&self, id: &str) -> Result<(), String> {
        self.ready()?;
        if !self.projects.is_dir() {
            return Err(format!("account {id}: there is no {} for its conversations (Claude Code makes it at its first session; coordd makes nothing there)", self.projects.display()));
        }
        Ok(())
    }

    /// Why NAME cannot be a coord's or an agent's name in a binding (it names a folder or a file under `bound/`).
    fn bad_name(name: &str) -> Option<String> {
        (name.is_empty() || name.len() > 200 || name.starts_with('.') || name.contains(['/', '\0'])).then(|| format!("{name:?} cannot name a binding (empty, a leading dot or a slash)"))
    }

    /// The folder of account ID: `ROOT/HARNESS/ID`.
    pub fn dir(&self, h: Harness, id: &str) -> PathBuf {
        self.root.join(h.dir()).join(id)
    }

    /// The harness's own folder of account ID (Claude Code's `CLAUDE_CONFIG_DIR`). Never read by coordd.
    pub fn config(&self, h: Harness, id: &str) -> PathBuf {
        self.dir(h, id).join("config")
    }

    /// The account whose harness folder CONFIG is ([`Store::config`]), if it is one of H's accounts.
    pub fn of_config(&self, h: Harness, config: &Path) -> Option<String> {
        self.ready().ok()?;
        let mut parts = config.strip_prefix(self.harness_dir(h)).ok()?.components();
        let id = parts.next()?.as_os_str().to_str()?.to_string();
        (parts.next()?.as_os_str() == "config" && parts.next().is_none() && Store::bad_id(&id).is_none() && self.dir(h, &id).join("account.json").is_file()).then_some(id)
    }

    /// The harness's folder under the root (where `turn.json` is).
    pub fn harness_dir(&self, h: Harness) -> PathBuf {
        self.root.join(h.dir())
    }

    /// Why ID cannot be an account's id (it names a folder).
    pub fn bad_id(id: &str) -> Option<String> {
        (id.is_empty() || id.len() > 40 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') || id.starts_with('-'))
            .then(|| format!("{id:?} is not an account's id (letters, digits, - and _)"))
    }

    /// The folder of account ID; None for a word that is not an id ([`Store::bad_id`]: `..`, a slash), so no request
    /// reaches a folder outside the store.
    fn own(&self, h: Harness, id: &str) -> Option<PathBuf> {
        Store::bad_id(id).is_none().then(|| self.dir(h, id))
    }

    /// The accounts of H the owner removed ([`Store::remove`]), newest first: the folders under
    /// `ROOT/removed/HARNESS`, each with the label its own `account.json` has (ours; nothing of Claude Code's is read
    /// there).
    pub fn removed(&self, h: Harness) -> Vec<Removed> {
        if self.fault.is_some() {
            return Vec::new();
        }
        let dirs = std::fs::read_dir(self.root.join("removed").join(h.dir())).into_iter().flatten().flatten();
        let mut out: Vec<Removed> = dirs
            .filter_map(|d| {
                let name = d.file_name().into_string().ok()?;
                let (id, stamp) = name.rsplit_once('.')?;
                let account: Account = read_json(&d.path().join("account.json"))?;
                let at = chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%d-%H%M%S").map_or(0, |t| t.and_utc().timestamp());
                Some(Removed { id: id.to_string(), label: account.label, at, name: name.clone(), not_signed_out: d.path().join(NOT_SIGNED_OUT).is_file() })
            })
            .collect();
        out.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.name.cmp(&b.name)));
        out
    }

    /// The removed account whose set-aside folder is NAME (`ID.STAMP`, as [`Store::removed`] lists it) is an account
    /// again: the folder is moved back whole to where an account of its id is looked for. Its id is free, since no
    /// account is made under a removed one's ([`Store::fresh_id`]); refused should one be there all the same. Under
    /// the harness's lock, which the choice of an account takes (`router::take`), waited for: off the core's lock. The
    /// id.
    pub fn put_back(&self, h: Harness, name: &str) -> Result<String, String> {
        self.ready()?;
        // NAME is the caller's word: only a name `removed` itself lists is a folder here
        let Some(r) = self.removed(h).into_iter().find(|r| r.name == name) else { return Err(format!("no removed account {name:?}")) };
        if let Some(why) = Store::bad_id(&r.id) {
            return Err(why);
        }
        let dir = self.harness_dir(h);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let _lock = Lock::take(&dir.join("lock")).map_err(|e| format!("{}: {e}", dir.display()))?;
        let (from, to) = (self.root.join("removed").join(h.dir()).join(&r.name), self.dir(h, &r.id));
        if to.exists() {
            return Err(format!("account {}: there is one already", r.id));
        }
        std::fs::rename(&from, &to).map_err(|e| format!("account {}: {}: {e}", r.id, from.display()))?;
        // an account again: whether it is signed in is asked of Claude Code (main.rs), not read from the mark
        let _ = std::fs::remove_file(to.join(NOT_SIGNED_OUT));
        Ok(r.id)
    }

    /// Claude Code has signed the removed account out in its set-aside folder ASIDE ([`Store::remove`]'s path): the
    /// mark [`NOT_SIGNED_OUT`] goes. A folder put back meanwhile has none.
    pub fn signed_out(&self, aside: &Path) {
        let _ = std::fs::remove_file(aside.join(NOT_SIGNED_OUT));
    }

    /// Key account ID's key file is deleted (the window's `Remove key`); a file that is not there is no failure.
    /// Whether the account then has no key is [`Store::key_set`]'s to say (`mover::key_step`, Removed).
    pub fn key_remove(&self, h: Harness, id: &str) -> Result<(), String> {
        self.ready()?;
        match self.get(h, id).map(|e| e.account.kind) {
            None => return Err(format!("no account {id}")),
            Some(Kind::Signin) => return Err(format!("account {id} runs on a sign-in, not on a key")),
            Some(Kind::Key) => {}
        }
        match std::fs::remove_file(self.key_file(h, id)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("account {id}: its key file: {e}")),
            _ => Ok(()),
        }
    }

    /// ID, unless a removed account of H had it: then ID with the first free number after it (`work-2`), so nothing
    /// kept of a removed account is read as a new one's.
    pub fn fresh_id(&self, h: Harness, id: &str) -> String {
        let removed: Vec<String> = self.removed(h).into_iter().map(|r| r.id).collect();
        if !removed.iter().any(|r| r == id) {
            return id.to_string();
        }
        // an id is at most 40 letters (`bad_id`)
        let base: String = id.chars().take(36).collect();
        (2..1000).map(|n| format!("{base}-{n}")).find(|c| !removed.contains(c) && !self.dir(h, c).exists()).unwrap_or_else(|| id.to_string())
    }

    /// A new account ID: its folder, `account.json`, an empty `state.json`, and `config/` with the `projects` link, all
    /// before anyone signs in, so that Claude Code never makes a `projects` folder of its own there. Refused if ID is
    /// taken; where ID was a removed account's, the account is made under [`Store::fresh_id`]'s, which the entry
    /// says. An account of kind key starts with no key.
    pub fn create(&self, h: Harness, id: &str, label: &str, tier: i64, kind: Kind, now: &str) -> Result<Entry, String> {
        self.make(h, id, Account { label: label.to_string(), tier, added: now.to_string(), kind, provider: None })
    }

    /// A new opencode account ID for PROVIDER (opencode's name of it, `opencode-go`): as [`Store::create`], its
    /// `home/` folders made empty, nobody signed in. Whether opencode knows the provider is opencode's to say at the
    /// sign-in; a word that could be no provider's (empty, a space, a leading `-`) is refused here.
    pub fn create_opencode(&self, id: &str, label: &str, tier: i64, provider: &str, now: &str) -> Result<Entry, String> {
        if let Some(why) = opencode::bad_provider(provider) {
            return Err(why);
        }
        self.make(Harness::Opencode, id, Account { label: label.to_string(), tier, added: now.to_string(), kind: Kind::Signin, provider: Some(provider.to_string()) })
    }

    /// A new agy account ID: as [`Store::create`], its `home/` made empty and nobody signed in. KIND Signin signs in by
    /// t3code's own Google sign-in on the account's t3code server ([`agy`]); Key runs on a Gemini API key the owner
    /// then gives (the window's key field), and starts with none.
    pub fn create_agy(&self, id: &str, label: &str, tier: i64, kind: Kind, now: &str) -> Result<Entry, String> {
        self.make(Harness::Agy, id, Account { label: label.to_string(), tier, added: now.to_string(), kind, provider: None })
    }

    /// [`Store::create`] of ACCOUNT. An id is one account's in all harnesses: one that another harness has is
    /// refused, so a request that names an account by its id alone finds one ([`Store::harness_of`]).
    fn make(&self, h: Harness, id: &str, account: Account) -> Result<Entry, String> {
        if h == Harness::Claude {
            self.conversations(id)?;
        }
        self.ready()?;
        if let Some(why) = Store::bad_id(id) {
            return Err(why);
        }
        let id = &self.fresh_id(h, id);
        if let Some(other) = Harness::ALL.into_iter().filter(|o| *o != h).find(|o| self.dir(*o, id).exists()) {
            return Err(format!("account {id}: there is one already ({}'s)", other.shown()));
        }
        let dir = self.dir(h, id);
        if dir.exists() {
            return Err(format!("account {id}: there is one already"));
        }
        let fail = |e: std::io::Error| format!("account {id}: {}: {e}", dir.display());
        std::fs::create_dir_all(dir.join("bound")).map_err(fail)?;
        mode(&dir, 0o700).map_err(fail)?;
        write_json(&dir.join("account.json"), &account).map_err(fail)?;
        write_json(&dir.join("state.json"), &State::default()).map_err(fail)?;
        self.layout(h, id)?;
        Ok(Entry { harness: h, id: id.to_string(), account, state: State::default(), bound: vec![] })
    }

    /// Where key account ID's key is: the file `key` beside `account.json`, not under `config/` (Claude Code's own
    /// folder holds nothing of ours).
    pub fn key_file(&self, h: Harness, id: &str) -> PathBuf {
        self.dir(h, id).join("key")
    }

    /// When account ID's key file was last written (seconds), if it is there. Its content is never read here.
    pub fn key_since(&self, h: Harness, id: &str) -> Option<i64> {
        let written = std::fs::symlink_metadata(self.own(h, id)?.join("key")).ok()?.modified().ok()?;
        Some(written.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64)
    }

    /// Whether account ID's key file is there to be used: a file of its own (no link), not empty, and readable by the
    /// owner alone. Its content is never read here. An Err says what is wrong with a file that is there: one that
    /// others can read is not used.
    pub fn key_set(&self, h: Harness, id: &str) -> Result<bool, String> {
        use std::os::unix::fs::PermissionsExt;
        let file = self.own(h, id).ok_or_else(|| format!("no account {id}"))?.join("key");
        let meta = match std::fs::symlink_metadata(&file) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(format!("account {id}: {}: {e}", file.display())),
        };
        if !meta.is_file() {
            return Err(format!("account {id}: {} is not a file of its own", file.display()));
        }
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(format!("account {id}: {} can be read by others (mode {:o}); set the key again (coord account key {id})", file.display(), meta.permissions().mode() & 0o777));
        }
        Ok(meta.len() > 0)
    }

    /// Where OpenCode Go account ID's usage key is kept ([`USAGE_KEY`]).
    pub fn usage_key_file(&self, id: &str) -> PathBuf {
        self.dir(Harness::Opencode, id).join(USAGE_KEY)
    }

    /// Whether account ID's usage key is there to be sent, by what [`Store::key_set`] asks of a key file: a file of its
    /// own, not empty, the owner's alone. Its content is never read here.
    pub fn usage_key_set(&self, id: &str) -> bool {
        use std::os::unix::fs::PermissionsExt;
        self.own(Harness::Opencode, id).and_then(|d| std::fs::symlink_metadata(d.join(USAGE_KEY)).ok()).is_some_and(|m| m.is_file() && m.len() > 0 && m.permissions().mode() & 0o077 == 0)
    }

    /// The account's `config/` as a session needs it: there, ours alone (0700), and `projects` the link to where
    /// conversations are kept. Called at creation and before every start on the account. An Err says what is in the
    /// way: a `projects` that is a folder of its own holds conversations nothing else would find, and is the owner's to
    /// look at, so nothing is moved or removed here. Refused while the conversations' folder is missing
    /// (`conversations`).
    pub fn layout(&self, h: Harness, id: &str) -> Result<(), String> {
        if h != Harness::Claude {
            return self.layout_home(h, id);
        }
        self.conversations(id)?;
        let config = self.own(h, id).ok_or_else(|| format!("no account {id}"))?.join("config");
        let fail = |e: std::io::Error| format!("account {id}: {}: {e}", config.display());
        std::fs::create_dir_all(&config).map_err(fail)?;
        mode(&config, 0o700).map_err(fail)?;
        let link = config.join("projects");
        match std::fs::symlink_metadata(&link) {
            Err(_) => std::os::unix::fs::symlink(&self.projects, &link).map_err(fail),
            Ok(m) if m.file_type().is_symlink() => match std::fs::read_link(&link) {
                Ok(to) if to == self.projects => Ok(()),
                Ok(to) => Err(format!("account {id}: {} links to {}, not to {}", link.display(), to.display(), self.projects.display())),
                Err(e) => Err(fail(e)),
            },
            Ok(_) => Err(format!(
                "account {id}: {} is a folder of its own, not the link to {}: conversations begun there are found by no other account",
                link.display(),
                self.projects.display()
            )),
        }
    }

    /// Opencode account ID's `home/`: the folder opencode's own files go under, as `HOME` of its sign-in and as the
    /// parent of the XDG folders of every run ([`Store::session_env`]). Never read by coordd.
    pub fn home(&self, id: &str) -> PathBuf {
        self.dir(Harness::Opencode, id).join("home")
    }

    /// [`Store::layout`] of an opencode or agy account (H): `home/`, and for opencode its `data`, `config`, `state`,
    /// `cache` and `tmp`, each a folder of its own (no link), ours alone (0700). What the harness wrote in them stays
    /// as it is: nothing in them is read, moved or removed. An Err names what is in the way.
    fn layout_home(&self, h: Harness, id: &str) -> Result<(), String> {
        self.ready()?;
        let dir = self.own(h, id).filter(|d| d.join("account.json").is_file()).ok_or_else(|| format!("no account {id}"))?;
        let home = dir.join("home");
        let under: &[&str] = if h == Harness::Opencode { &opencode::HOME_DIRS } else { &[] };
        for d in std::iter::once(home.clone()).chain(under.iter().map(|d| home.join(d))) {
            let fail = |e: std::io::Error| format!("account {id}: {}: {e}", d.display());
            match std::fs::symlink_metadata(&d) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::fs::create_dir(&d).map_err(fail)?,
                Err(e) => return Err(fail(e)),
                Ok(m) if m.is_dir() => {}
                Ok(_) => return Err(format!("account {id}: {} is not a folder of its own (a link or a file): {}'s files would go elsewhere", d.display(), h.shown())),
            }
            mode(&d, 0o700).map_err(fail)?;
        }
        Ok(())
    }

    /// The environment a session of harness H runs with on account ID, beside its own. For opencode, its XDG folders and
    /// TMPDIR under the account's `home/`, its project config and `~/.claude` not read, no autoupdate. For Claude Code,
    /// `CLAUDE_CONFIG_DIR`. For agy, a sign-in account's `AGY_SIGNIN`, or `T3_HOME` on one t3code signed in
    /// ([`agy::session_env`]). Nothing in it is a secret.
    pub fn session_env(&self, h: Harness, id: &str) -> Vec<(String, String)> {
        let dir = self.dir(h, id);
        match h {
            Harness::Claude => vec![("CLAUDE_CONFIG_DIR".into(), dir.join("config").display().to_string())],
            Harness::Opencode => opencode::folder_env(&dir),
            Harness::Agy => agy::session_env(&dir, self.kind(h, id).unwrap_or_default()),
        }
    }

    /// The harness whose account ID is, if one's is: ids are one account's in all harnesses ([`Store::make`]).
    pub fn harness_of(&self, id: &str) -> Option<Harness> {
        self.ready().ok()?;
        Harness::ALL.into_iter().find(|h| self.own(*h, id).is_some_and(|d| d.join("account.json").is_file()))
    }

    /// Every account of H, by id.
    pub fn list(&self, h: Harness) -> Vec<Entry> {
        self.ids(h).iter().filter_map(|id| self.get(h, id)).collect()
    }

    /// Account ID, if there is one.
    pub fn get(&self, h: Harness, id: &str) -> Option<Entry> {
        self.ready().ok()?;
        let dir = self.own(h, id)?;
        let account: Account = read_json(&dir.join("account.json"))?;
        let state: State = read_json(&dir.join("state.json")).unwrap_or_default();
        let coords = std::fs::read_dir(dir.join("bound")).into_iter().flatten().flatten();
        let mut bound: Vec<Bound> = coords.flat_map(|c| std::fs::read_dir(c.path()).into_iter().flatten().flatten()).filter_map(|e| read_json(&e.path())).collect();
        bound.sort_by(|a, b| (&a.coord, &a.agent).cmp(&(&b.coord, &b.agent)));
        Some(Entry { harness: h, id: id.to_string(), account, state, bound })
    }

    /// Account ID's kind alone (one small file), if there is such an account.
    pub fn kind(&self, h: Harness, id: &str) -> Option<Kind> {
        self.ready().ok()?;
        read_json::<Account>(&self.own(h, id)?.join("account.json")).map(|a| a.kind)
    }

    /// Account ID's state alone (one small file), if there is such an account.
    pub fn state(&self, h: Harness, id: &str) -> Option<State> {
        self.ready().ok()?;
        let dir = self.own(h, id)?;
        dir.join("account.json").is_file().then(|| read_json(&dir.join("state.json")).unwrap_or_default())
    }

    /// The ids of H's accounts, in order.
    fn ids(&self, h: Harness) -> Vec<String> {
        if self.fault.is_some() {
            return Vec::new();
        }
        let dirs = std::fs::read_dir(self.harness_dir(h)).into_iter().flatten().flatten();
        let mut ids: Vec<String> = dirs.filter(|e| e.path().join("account.json").is_file()).filter_map(|e| e.file_name().into_string().ok()).collect();
        ids.sort();
        ids
    }

    /// Change account ID's `account.json` (its label, its tier), under the account's lock, which is waited for: not
    /// under the core's lock.
    pub fn set_account(&self, h: Harness, id: &str, change: impl FnOnce(&mut Account)) -> Result<Account, String> {
        self.ready()?;
        let dir = self.own(h, id).ok_or_else(|| format!("no account {id}"))?;
        let _lock = Lock::take(&dir.join("lock")).map_err(|e| format!("account {id}: {e}"))?;
        let mut a: Account = read_json(&dir.join("account.json")).ok_or_else(|| format!("no account {id}"))?;
        change(&mut a);
        write_json(&dir.join("account.json"), &a).map_err(|e| format!("account {id}: {e}"))?;
        Ok(a)
    }

    /// Change account ID's `state.json`, read and written under the account's lock, so that two coordds that see
    /// different things of one account (one its sign-in, one its windows) do not overwrite each other. The lock is
    /// waited for: not under the core's lock.
    pub fn update(&self, h: Harness, id: &str, change: impl FnOnce(&mut State)) -> Result<State, String> {
        self.ready()?;
        let dir = self.own(h, id).filter(|d| d.join("account.json").is_file()).ok_or_else(|| format!("no account {id}"))?;
        let _lock = Lock::take(&dir.join("lock")).map_err(|e| format!("account {id}: {e}"))?;
        let mut s: State = read_json(&dir.join("state.json")).unwrap_or_default();
        change(&mut s);
        write_json(&dir.join("state.json"), &s).map_err(|e| format!("account {id}: {e}"))?;
        Ok(s)
    }

    /// The agents, of any coord, that run on account ID.
    pub fn running_on(&self, h: Harness, id: &str) -> Vec<Bound> {
        self.get(h, id).map(|e| e.bound).unwrap_or_default()
    }

    /// Why account ID cannot be set aside this moment, if it cannot: the agents, of any coord, that still run on it.
    pub fn in_use(&self, h: Harness, id: &str) -> Option<String> {
        let on: Vec<String> = self.running_on(h, id).iter().map(|b| format!("{}.{}", b.coord, b.agent)).collect();
        (!on.is_empty()).then(|| format!("account {id}: {} still {} on it", on.join(", "), if on.len() == 1 { "runs" } else { "run" }))
    }

    /// The coord whose coordd holds account ID's lock at this moment, if one does ([`Lock::holder`]).
    pub fn locked_by(&self, h: Harness, id: &str) -> Option<String> {
        Lock::holder(&self.own(h, id)?.join("lock"))
    }

    /// Account ID is no account any more: its folder is set aside whole as `ROOT/removed/HARNESS/ID.STAMP` (STAMP:
    /// when, as a file name), where no account is looked for. Nothing in it is read or deleted: what Claude Code
    /// keeps there is the owner's to throw away. Refused while an agent of any coord is bound to it, with their
    /// names: a session may run under the folder. Off the core's lock: the account's lock is waited for, so no choice
    /// of another coordd lands on it meanwhile. The path it now has.
    pub fn remove(&self, h: Harness, id: &str, stamp: &str) -> Result<PathBuf, String> {
        self.set_aside(h, id, stamp, false)
    }

    /// [`Store::remove`] for an account being removed that has no agent left: only if it is still being removed, read
    /// under the account's lock, so a `Keep it` since the round's look wins and nothing is touched.
    pub fn remove_left(&self, h: Harness, id: &str, stamp: &str) -> Result<PathBuf, String> {
        self.set_aside(h, id, stamp, true)
    }

    /// [`Store::remove`]; with LEFT, [`Store::remove_left`].
    fn set_aside(&self, h: Harness, id: &str, stamp: &str, left: bool) -> Result<PathBuf, String> {
        self.ready()?;
        let dir = self.own(h, id).filter(|d| d.join("account.json").is_file()).ok_or_else(|| format!("no account {id}"))?;
        let _lock = Lock::take(&dir.join("lock")).map_err(|e| format!("account {id}: {e}"))?;
        let state = dir.join("state.json");
        let marked = read_json::<State>(&state).filter(|s| s.leaving.is_some());
        if left && marked.is_none() {
            return Err(format!("account {id}: not being removed"));
        }
        if let Some(why) = self.in_use(h, id) {
            return Err(why);
        }
        let aside = self.root.join("removed").join(h.dir());
        let to = aside.join(format!("{id}.{stamp}"));
        let fail = |e: std::io::Error| format!("account {id}: {}: {e}", to.display());
        std::fs::create_dir_all(&aside).map_err(fail)?;
        mode(&self.root.join("removed"), 0o700).map_err(fail)?;
        if to.exists() {
            return Err(format!("account {id}: {} is there already", to.display()));
        }
        // a sign-in account's folder says it is not signed out until Claude Code has done it (`NOT_SIGNED_OUT`).
        // Written before the `leaving` mark is touched: if it cannot be written, nothing has changed
        let signin = read_json::<Account>(&dir.join("account.json")).is_some_and(|a| a.kind == Kind::Signin);
        if signin {
            std::fs::write(dir.join(NOT_SIGNED_OUT), format!("{} has not signed this account out here.\n", h.signs_out_of(&dir))).map_err(|e| format!("account {id}: {e}"))?;
        }
        // it is removed now: the `leaving` mark goes last before the move, and a failure from here puts it back
        let undo = || {
            if let Some(s) = &marked {
                let _ = write_json(&state, s);
            }
            let _ = std::fs::remove_file(dir.join(NOT_SIGNED_OUT));
        };
        if let Some(s) = &marked {
            if let Err(e) = write_json(&state, &State { leaving: None, ..s.clone() }) {
                undo();
                return Err(format!("account {id}: {e}"));
            }
        }
        if let Err(e) = std::fs::rename(&dir, &to) {
            undo();
            return Err(fail(e));
        }
        // the key kept for Go's usage goes with the account: a removed account's folder holds no key of ours
        match std::fs::remove_file(to.join(USAGE_KEY)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => eprintln!("coordd: account {id}: its usage key in {} was not deleted: {e}", to.display()),
            _ => {}
        }
        Ok(to)
    }

    /// AGENT of COORD runs on account ID from now: its `bound/COORD/AGENT`, and none under any other account of H.
    /// The two names are a folder and a file of their own, so no pair of names is another pair's file.
    pub fn bind(&self, h: Harness, id: &str, coord: &str, agent: &str, now: &str) -> Result<(), String> {
        self.ready()?;
        if let Some(why) = Store::bad_name(coord).or_else(|| Store::bad_name(agent)) {
            return Err(format!("account {id}: {why}"));
        }
        let dir = self.own(h, id).filter(|d| d.join("account.json").is_file()).ok_or_else(|| format!("no account {id}"))?.join("bound").join(coord);
        self.unbind(h, coord, agent);
        std::fs::create_dir_all(&dir).map_err(|e| format!("account {id}: {e}"))?;
        write_json(&dir.join(agent), &Bound { coord: coord.to_string(), agent: agent.to_string(), since: now.to_string() }).map_err(|e| format!("account {id}: {e}"))
    }

    /// AGENT of COORD runs on no account of H any more.
    pub fn unbind(&self, h: Harness, coord: &str, agent: &str) {
        if Store::bad_name(coord).or_else(|| Store::bad_name(agent)).is_some() {
            return;
        }
        for id in self.ids(h) {
            let _ = std::fs::remove_file(self.dir(h, &id).join("bound").join(coord).join(agent));
        }
    }

    /// AGENT of COORD does not run on account ID of H any more; a binding of it to another account stays. Whether
    /// there was one to take away.
    pub fn unbind_from(&self, h: Harness, id: &str, coord: &str, agent: &str) -> bool {
        if Store::bad_name(coord).or_else(|| Store::bad_name(agent)).is_some() {
            return false;
        }
        self.own(h, id).is_some_and(|d| std::fs::remove_file(d.join("bound").join(coord).join(agent)).is_ok())
    }

    /// The account of H that AGENT of COORD is bound to, if any.
    pub fn bound_to(&self, h: Harness, coord: &str, agent: &str) -> Option<Entry> {
        if Store::bad_name(coord).or_else(|| Store::bad_name(agent)).is_some() {
            return None;
        }
        let ids = self.ids(h);
        ids.iter().find(|id| self.dir(h, id).join("bound").join(coord).join(agent).is_file()).and_then(|id| self.get(h, id))
    }
}

/// An exclusive lock on a file (`flock`), held until dropped: the owner's coordds are separate processes that share
/// the accounts' folder.
///
/// It is waited for, however long another holds it, so **nothing that takes it runs under the core's lock**: every
/// change of the store ([`Store::update`], [`Store::set_account`], `router::take`, usage.rs) is made by a task of its
/// own, and the core takes the result (`mover.rs`: [`mover::Choosing`], [`mover::Keep`]; main.rs for what the owner
/// types). A coordd that holds a lock long holds up those tasks of another, never its core.
pub struct Lock(std::fs::File);

/// Who this process is in a lock file it holds; coordd says it at its start ([`i_am`]).
static HOLDER: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// This process is the coordd of COORD: what a lock it holds says to whoever looks at the file.
pub fn i_am(coord: &str) {
    let _ = HOLDER.set(format!("the coordd of {coord} (pid {})", std::process::id()));
}

impl Lock {
    /// Wait for the lock on PATH (made if missing), however long another holds it.
    pub fn take(path: &Path) -> std::io::Result<Lock> {
        use std::os::fd::AsRawFd;
        let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(path)?;
        // SAFETY: flock on a file descriptor this function owns
        if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Lock::held(f))
    }

    /// The coord whose coordd holds the lock on PATH at this moment: asked without waiting (a shared lock tried and
    /// let go at once), then read from the file, where its holder wrote who it is ([`i_am`]). None when nobody holds
    /// it, when the file is not there, and when the holder is no coordd that named itself. A name left in the file
    /// by a holder that died says nothing: the lock went with its process, and the try finds it free.
    pub fn holder(path: &Path) -> Option<String> {
        use std::os::fd::AsRawFd;
        let f = std::fs::File::open(path).ok()?;
        // SAFETY: flock on a file descriptor this function owns; closed, and so let go, when F is dropped
        if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) } == 0 {
            return None;
        }
        let said = std::fs::read_to_string(path).ok()?;
        Some(said.strip_prefix("the coordd of ")?.split(" (pid ").next()?.to_string()).filter(|c| !c.is_empty())
    }

    /// F's lock is this process's: the file says who holds it, until it is let go.
    fn held(f: std::fs::File) -> Lock {
        use std::os::unix::fs::FileExt;
        let me = HOLDER.get().cloned().unwrap_or_else(|| format!("pid {}", std::process::id()));
        let _ = f.set_len(0).and_then(|()| f.write_all_at(me.as_bytes(), 0));
        Lock(f)
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        // nobody holds it from here on, so the file names nobody
        let _ = self.0.set_len(0);
        // SAFETY: the descriptor is this lock's own, still open
        unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
    }
}

/// PATH's JSON as a T; None if it is missing or is not one.
pub(crate) fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// V as PATH's JSON, whole or not at all: written beside it and renamed over it.
pub(crate) fn write_json<T: Serialize>(path: &Path, v: &T) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".new.{}", std::process::id()));
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, serde_json::to_vec_pretty(v).map_err(std::io::Error::other)?)?;
    std::fs::rename(&tmp, path)
}

fn mode(path: &Path, m: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(m))
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    /// A store in a folder of its own under the system's temporary folder, removed when dropped.
    pub struct Tmp {
        pub dir: PathBuf,
        pub store: Store,
    }

    impl Tmp {
        /// A key account's record in a folder beside the harness's own, `ROOT/planted`: what the word `../planted`
        /// would name as an id. The folder.
        pub fn planted(&self) -> PathBuf {
            let dir = self.dir.join("accounts/planted");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("account.json"), r#"{"label": "P", "tier": 0, "added": "x", "kind": "key"}"#).unwrap();
            dir
        }

        pub fn new(name: &str) -> Tmp {
            let dir = std::env::temp_dir().join(format!("coordd-accounts-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            // the conversations' folder is Claude Code's: the store makes none
            std::fs::create_dir_all(dir.join("home/.claude/projects")).unwrap();
            let store = Store::at(dir.join("accounts"), dir.join("home/.claude/projects"));
            Tmp { dir, store }
        }

        /// A stand-in program NAME in the test's folder, running SCRIPT ([`program`]).
        pub fn program(&self, name: &str, script: &str) -> PathBuf {
            let p = self.dir.join(name);
            program(&p, script);
            p
        }
    }

    /// SCRIPT as the program AT: written beside it as `AT.script`, which nothing runs, and put in place by
    /// [`copy_program`]. Every test of coordd that makes a program it runs makes it so.
    pub fn program(at: &Path, script: &str) {
        let src = at.with_file_name(format!("{}.script", at.file_name().unwrap().to_string_lossy()));
        std::fs::write(&src, script).unwrap();
        copy_program(&src, at);
    }

    /// FROM as the program AT, put in place by a `cp` of its own, so the tests' process never holds a program open for
    /// writing (a child started meanwhile would hold the open file and fail with ETXTBSY).
    pub fn copy_program(from: &Path, at: &Path) {
        let _ = std::fs::remove_file(at);
        assert!(std::process::Command::new("cp").arg(from).arg(at).status().unwrap().success());
        mode(at, 0o755).unwrap();
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::Tmp;
    use super::*;

    /// A program made by [`testing::program`] while other threads start processes runs at once, every round.
    #[test]
    fn a_program_made_while_other_threads_start_processes_runs_at_once() {
        use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
        let t = Tmp::new("busy");
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let starting: Vec<_> = (0..4)
            .map(|_| {
                let stop = stop.clone();
                std::thread::spawn(move || {
                    let mut n = 0u32;
                    while !stop.load(Relaxed) {
                        std::process::Command::new("true").status().unwrap();
                        n += 1;
                    }
                    n
                })
            })
            .collect();
        let ran: Vec<_> = (0..300)
            .map(|i| std::process::Command::new(t.program(&format!("p{i}"), "#!/bin/sh\nexit 0\n")).status().map_err(|e| format!("round {i}: {e}")))
            .collect();
        stop.store(true, Relaxed);
        let started: u32 = starting.into_iter().map(|h| h.join().unwrap()).sum();
        assert!(started > 0, "processes started meanwhile");
        let failed: Vec<_> = ran.iter().filter_map(|r| r.as_ref().err()).collect();
        assert!(failed.is_empty(), "{} of 300 rounds failed, first: {:?}; {started} processes started meanwhile", failed.len(), failed.first());
    }

    #[test]
    fn a_new_account_has_its_records_and_the_projects_link_and_no_login() {
        let t = Tmp::new("create");
        let e = t.store.create(Harness::Claude, "work", "Work", 10, Kind::Signin, "2026-11-18T12:00:00Z").unwrap();
        assert_eq!((e.account.label.as_str(), e.account.tier), ("Work", 10));
        let config = t.store.config(Harness::Claude, "work");
        assert_eq!(std::fs::read_link(config.join("projects")).unwrap(), t.dir.join("home/.claude/projects"));
        // nothing but the link is in Claude Code's folder: the login is Claude Code's to make
        assert_eq!(std::fs::read_dir(&config).unwrap().count(), 1);
        assert!(!e.state.signed_in && !e.state.can_take(0));
        assert!(t.store.create(Harness::Claude, "work", "again", 0, Kind::Signin, "x").unwrap_err().contains("already"));
        assert!(t.store.create(Harness::Claude, "../x", "bad", 0, Kind::Signin, "x").is_err());
    }

    #[test]
    fn a_word_that_is_no_id_names_no_account_whatever_lies_where_it_points() {
        let t = testing::Tmp::new("no-id");
        let h = Harness::Claude;
        t.store.create(h, "k", "K", 0, Kind::Key, "x").unwrap();
        let planted = t.planted();
        let id = "../planted";
        // the path such a word would build is the planted folder, and the record there reads as a key account's
        assert_eq!(std::fs::canonicalize(t.store.dir(h, id)).unwrap(), std::fs::canonicalize(&planted).unwrap());
        assert!(read_json::<Account>(&planted.join("account.json")).is_some_and(|a| a.kind == Kind::Key));
        assert!(t.store.get(h, id).is_none() && t.store.state(h, id).is_none() && t.store.key_since(h, id).is_none());
        for refused in [
            t.store.key_set(h, id).map(|_| ()),
            t.store.update(h, id, |s| s.signed_in = true).map(|_| ()),
            t.store.set_account(h, id, |a| a.tier = 9).map(|_| ()),
            t.store.bind(h, id, "research", "w", "x"),
            t.store.layout(h, id),
        ] {
            assert_eq!(refused, Err(format!("no account {id}")));
        }
        // nothing was made or changed there
        let there: Vec<_> = std::fs::read_dir(&planted).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(there, ["account.json"]);
    }

    #[test]
    fn a_key_account_is_one_by_its_record_and_its_key_file_counts_only_as_the_owners_alone() {
        use std::os::unix::fs::PermissionsExt;
        let t = Tmp::new("key");
        let h = Harness::Claude;
        t.store.create(h, "k", "K", 0, Kind::Key, "x").unwrap();
        t.store.create(h, "s", "S", 0, Kind::Signin, "x").unwrap();
        // the record names the kind for a key account only: a sign-in account's reads as before there were kinds
        let record = |id: &str| std::fs::read_to_string(t.dir.join(format!("accounts/claude/{id}/account.json"))).unwrap();
        assert!(record("k").contains("\"kind\": \"key\"") && !record("s").contains("kind"), "{} {}", record("k"), record("s"));
        assert_eq!((t.store.get(h, "k").unwrap().account.kind, t.store.get(h, "s").unwrap().account.kind), (Kind::Key, Kind::Signin));
        // beside the record, not in Claude Code's folder
        let file = t.store.key_file(h, "k");
        assert_eq!(file, t.dir.join("accounts/claude/k/key"));
        let write = |text: &str, mode: u32| {
            std::fs::write(&file, text).unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
        };
        assert_eq!(t.store.key_set(h, "k"), Ok(false), "no file");
        write("made-up\n", 0o600);
        assert_eq!(t.store.key_set(h, "k"), Ok(true));
        write("", 0o600);
        assert_eq!(t.store.key_set(h, "k"), Ok(false), "an empty file is no key");
        write("made-up\n", 0o640);
        let why = t.store.key_set(h, "k").unwrap_err();
        assert!(why.contains("can be read by others (mode 640)") && !why.contains("made-up"), "{why}");
        // a link to a file elsewhere is not the account's own
        std::fs::remove_file(&file).unwrap();
        std::fs::write(t.dir.join("elsewhere"), "made-up\n").unwrap();
        std::os::unix::fs::symlink(t.dir.join("elsewhere"), &file).unwrap();
        assert!(t.store.key_set(h, "k").unwrap_err().contains("not a file of its own"));
    }

    #[test]
    fn a_removed_account_is_set_aside_whole_and_one_in_use_is_not_removed() {
        let t = Tmp::new("remove");
        let h = Harness::Claude;
        t.store.create(h, "work", "Work", 10, Kind::Signin, "x").unwrap();
        // what Claude Code keeps there, as a stand-in: never read, and it goes with the folder
        std::fs::write(t.store.config(h, "work").join(".credentials.json"), "claude code's own").unwrap();
        t.store.bind(h, "work", "research", "orch", "x").unwrap();
        t.store.bind(h, "work", "docket", "lead", "x").unwrap();
        let why = t.store.remove(h, "work", "20261118-200000").unwrap_err();
        assert_eq!(why, "account work: docket.lead, research.orch still run on it");
        t.store.unbind(h, "docket", "lead");
        assert_eq!(t.store.in_use(h, "work").unwrap(), "account work: research.orch still runs on it");
        assert_eq!(t.store.running_on(h, "work").iter().map(|b| b.agent.as_str()).collect::<Vec<_>>(), ["orch"]);
        assert!(t.store.get(h, "work").is_some(), "refused: it is an account still");
        // its removal is asked meanwhile: it takes no agent, and the mark goes with the removal
        t.store.update(h, "work", |s| (s.signed_in, s.leaving) = (true, Some(5))).unwrap();
        assert!(!t.store.state(h, "work").unwrap().can_take(10));
        t.store.unbind(h, "research", "orch");
        assert_eq!(t.store.in_use(h, "work"), None);
        // a round found it free and being removed, and he clicked `Keep it` before the round's removal had the
        // account's lock: the removal reads the mark again under the lock, and the account stays, untouched
        let held = Lock::take(&t.store.dir(h, "work").join("lock")).unwrap();
        let store = t.store.clone();
        let round = std::thread::spawn(move || store.remove_left(h, "work", "20261118-200000"));
        std::thread::sleep(std::time::Duration::from_millis(300));
        // `Keep it`, as `Store::update` writes it under the lock this test holds in its place
        let state = t.store.dir(h, "work").join("state.json");
        write_json(&state, &State { leaving: None, ..read_json::<State>(&state).unwrap() }).unwrap();
        drop(held);
        assert_eq!(round.join().unwrap().unwrap_err(), "account work: not being removed");
        assert!(t.store.get(h, "work").is_some() && t.store.removed(h).is_empty());
        assert_eq!(std::fs::read_to_string(t.store.config(h, "work").join(".credentials.json")).unwrap(), "claude code's own");
        // asked for again, and not kept this time: the round's removal goes through
        t.store.update(h, "work", |s| s.leaving = Some(6)).unwrap();
        // the folder's own mark cannot be written (a folder stands where the file goes): the removal did not happen
        // and is still on its way, the account as it was
        std::fs::create_dir(t.store.dir(h, "work").join(NOT_SIGNED_OUT)).unwrap();
        assert!(t.store.remove_left(h, "work", "20261118-200000").unwrap_err().starts_with("account work: "));
        assert_eq!(t.store.state(h, "work").unwrap().leaving, Some(6));
        assert!(t.store.get(h, "work").is_some() && t.store.removed(h).is_empty());
        std::fs::remove_dir(t.store.dir(h, "work").join(NOT_SIGNED_OUT)).unwrap();
        let aside = t.store.remove_left(h, "work", "20261118-200000").unwrap();
        assert_eq!(aside, t.dir.join("accounts/removed/claude/work.20261118-200000"));
        // whole, with Claude Code's own as it was; no account of that id any more
        assert_eq!(std::fs::read_to_string(aside.join("config/.credentials.json")).unwrap(), "claude code's own");
        assert!(aside.join("account.json").is_file() && t.store.get(h, "work").is_none() && t.store.list(h).is_empty());
        assert_eq!(read_json::<State>(&aside.join("state.json")).unwrap().leaving, None, "put back, it would stay");
        assert_eq!(t.store.remove(h, "work", "20261118-200001").unwrap_err(), "no account work");
        assert!(t.store.remove(h, "../removed", "x").is_err());
        // listed as removed, with its label and when; its id is never an account's again: the next one is `work-2`,
        // and after that one is removed too, `work-3`
        let removed = t.store.removed(h);
        assert_eq!(removed.iter().map(|r| (r.id.as_str(), r.label.as_str(), r.name.as_str())).collect::<Vec<_>>(), [("work", "Work", "work.20261118-200000")]);
        assert_eq!(removed[0].at, chrono::DateTime::parse_from_rfc3339("2026-11-18T20:00:00Z").unwrap().timestamp());
        // Claude Code has not signed it out there yet, and the folder says so until it has
        assert!(removed[0].not_signed_out && aside.join(NOT_SIGNED_OUT).is_file());
        assert_eq!(t.store.create(h, "work", "Work again", 10, Kind::Signin, "x").unwrap().id, "work-2");
        assert_eq!(t.store.create(h, "other", "Other", 10, Kind::Signin, "x").unwrap().id, "other");
        assert_eq!(t.store.list(h).iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["other", "work-2"]);
        let second = t.store.remove(h, "work-2", "20261118-200500").unwrap();
        t.store.signed_out(&second);
        assert_eq!(t.store.removed(h).iter().map(|r| (r.id.as_str(), r.not_signed_out)).collect::<Vec<_>>(), [("work-2", false), ("work", true)]);
        // a key account has no sign-in: its folder is never marked
        t.store.create(h, "k", "Key", 10, Kind::Key, "x").unwrap();
        let key = t.store.remove(h, "k", "20261118-190000").unwrap();
        assert!(!key.join(NOT_SIGNED_OUT).exists() && !t.store.removed(h).last().unwrap().not_signed_out);
        std::fs::remove_dir_all(key).unwrap();
        assert_eq!(t.store.create(h, "work", "Work, third", 10, Kind::Signin, "x").unwrap().id, "work-3");
        assert_eq!(t.store.removed(h).iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["work-2.20261118-200500", "work.20261118-200000"], "newest first");
        // put back: the folder is an account again, whole, under its own id; only a name the list has is one
        assert_eq!(t.store.put_back(h, "../claude/other").unwrap_err(), "no removed account \"../claude/other\"");
        assert_eq!(t.store.put_back(h, "work.20261118-200001").unwrap_err(), "no removed account \"work.20261118-200001\"");
        assert_eq!(t.store.put_back(h, "work.20261118-200000").unwrap(), "work");
        assert!(!t.store.dir(h, "work").join(NOT_SIGNED_OUT).exists(), "an account again: Claude Code is asked, the mark is gone");
        let back = t.store.get(h, "work").unwrap();
        assert_eq!((back.account.label.as_str(), std::fs::read_to_string(t.store.config(h, "work").join(".credentials.json")).unwrap().as_str()), ("Work", "claude code's own"));
        assert_eq!(t.store.removed(h).iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["work-2.20261118-200500"]);
        assert_eq!(t.store.list(h).iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["other", "work", "work-3"]);
    }

    #[test]
    fn a_projects_folder_of_its_own_is_said_and_left() {
        let t = Tmp::new("layout");
        t.store.create(Harness::Claude, "a", "A", 0, Kind::Signin, "x").unwrap();
        let link = t.store.config(Harness::Claude, "a").join("projects");
        std::fs::remove_file(&link).unwrap();
        std::fs::create_dir_all(link.join("-w-repo")).unwrap();
        let why = t.store.layout(Harness::Claude, "a").unwrap_err();
        assert!(why.contains("a folder of its own"), "{why}");
        assert!(link.join("-w-repo").is_dir());
    }

    #[test]
    fn state_changes_keep_what_another_wrote_and_an_agent_is_bound_to_one_account() {
        let t = Tmp::new("state");
        let h = Harness::Claude;
        t.store.create(h, "a", "A", 0, Kind::Signin, "x").unwrap();
        t.store.create(h, "b", "B", 0, Kind::Signin, "x").unwrap();
        t.store.update(h, "a", |s| s.signed_in = true).unwrap();
        t.store.update(h, "a", |s| s.out_until = Some(100)).unwrap();
        let a = t.store.get(h, "a").unwrap();
        assert!(a.state.signed_in && !a.state.can_take(99) && a.state.can_take(100));
        assert!(t.store.update(h, "none", |_| ()).is_err());

        t.store.bind(h, "a", "research", "orch", "t1").unwrap();
        t.store.bind(h, "b", "research", "orch", "t2").unwrap();
        assert!(t.store.get(h, "a").unwrap().bound.is_empty());
        assert_eq!(t.store.bound_to(h, "research", "orch").unwrap().id, "b");
        assert_eq!(t.store.bound_to(h, "cav-paper", "orch"), None);
        t.store.unbind(h, "research", "orch");
        assert_eq!(t.store.bound_to(h, "research", "orch"), None);
    }

    #[test]
    fn an_account_on_file_from_before_heard_was_kept_counts_as_heard_and_one_not_heard_stays_so() {
        let before: State = serde_json::from_str(r#"{"signed_in": true, "signed_in_seen": 5}"#).unwrap();
        assert_eq!(before.heard, Some(0));
        let fresh = State { signed_in: true, ..State::default() };
        let text = serde_json::to_string(&fresh).unwrap();
        assert!(text.contains(r#""heard":null"#), "{text}");
        assert_eq!(serde_json::from_str::<State>(&text).unwrap().heard, None);
    }

    #[test]
    fn names_with_dots_are_bindings_of_their_own_and_a_name_that_is_no_file_is_refused() {
        let t = Tmp::new("names");
        let h = Harness::Claude;
        t.store.create(h, "a", "A", 0, Kind::Signin, "x").unwrap();
        t.store.create(h, "b", "B", 0, Kind::Signin, "x").unwrap();
        // coord `a.b` with agent `c`, and coord `a` with agent `b.c`: two agents
        t.store.bind(h, "a", "a.b", "c", "t").unwrap();
        t.store.bind(h, "b", "a", "b.c", "t").unwrap();
        assert_eq!(t.store.bound_to(h, "a.b", "c").unwrap().id, "a");
        assert_eq!(t.store.bound_to(h, "a", "b.c").unwrap().id, "b");
        t.store.unbind(h, "a", "b.c");
        assert_eq!(t.store.bound_to(h, "a.b", "c").unwrap().id, "a", "the other's binding stays");
        for (coord, agent) in [("", "w"), ("research", ""), ("..", "w"), ("research", "../../x"), ("re/search", "w"), ("research", ".hidden")] {
            assert!(t.store.bind(h, "a", coord, agent, "t").is_err(), "{coord:?} {agent:?}");
            assert_eq!(t.store.bound_to(h, coord, agent), None);
        }
        assert_eq!(t.store.get(h, "a").unwrap().bound.len(), 1);
    }

    #[test]
    fn nothing_is_made_under_the_owners_claude_folder_and_no_home_is_no_store() {
        let t = Tmp::new("nohome");
        let h = Harness::Claude;
        // Claude Code never ran for this user: the account is refused, and its conversations' folder is not made
        std::fs::remove_dir_all(t.dir.join("home/.claude")).unwrap();
        let why = t.store.create(h, "a", "A", 0, Kind::Signin, "x").unwrap_err();
        assert!(why.contains("coordd makes nothing there"), "{why}");
        assert!(!t.dir.join("home/.claude").exists() && t.store.get(h, "a").is_none(), "and no half-made account is left");
        // without a home folder there is no store: nothing is listed, and every change says why
        let none = Store { root: PathBuf::new(), projects: PathBuf::new(), fault: Some("HOME is not set".into()) };
        assert!(none.list(h).is_empty() && none.get(h, "a").is_none() && none.state(h, "a").is_none());
        for r in [none.create(h, "z", "Z", 0, Kind::Signin, "x").map(|_| ()), none.bind(h, "z", "research", "w", "t"), none.update(h, "z", |_| ()).map(|_| ()), none.layout(h, "z")] {
            assert_eq!(r.unwrap_err(), "HOME is not set");
        }
    }

    #[test]
    fn a_lock_another_holds_is_waited_for_and_a_change_made_meanwhile_is_kept() {
        let t = Tmp::new("lock");
        let h = Harness::Claude;
        t.store.create(h, "a", "A", 0, Kind::Signin, "x").unwrap();
        let path = t.store.dir(h, "a").join("lock");
        let held = Lock::take(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), format!("pid {}", std::process::id()), "the file names its holder");
        // a change asked for meanwhile waits: it is neither refused nor made
        let (store, done) = (t.store.clone(), std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)));
        let d = done.clone();
        let change = std::thread::spawn(move || {
            let r = store.update(h, "a", |s| s.out_until = Some(100));
            d.store(true, std::sync::atomic::Ordering::SeqCst);
            r
        });
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(!done.load(std::sync::atomic::Ordering::SeqCst) && t.store.state(h, "a").unwrap().out_until.is_none());
        // who holds it, for the card of a click that waits: a coordd that named itself, by its coord; this test's
        // process did not, and is none
        assert_eq!(t.store.locked_by(h, "a"), None);
        std::fs::write(&path, "the coordd of docket (pid 7)").unwrap();
        assert_eq!(t.store.locked_by(h, "a").as_deref(), Some("docket"));
        drop(held);
        assert_eq!(change.join().unwrap().unwrap().out_until, Some(100));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "", "let go, the file names nobody");
        // a name left by a holder that died is nobody's: the lock is free
        std::fs::write(&path, "the coordd of docket (pid 7)").unwrap();
        assert_eq!(t.store.locked_by(h, "a"), None);
    }
}
