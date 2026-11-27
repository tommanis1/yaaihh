//! coordd's state and what changes it (design §5-7): the agents (registry, process, turn), mail and its delivery,
//! items and rights, project views, and the numbered events every change emits.
//!
//! History is append-only JSON lines in the coord folder: `registry.jsonl` (agents: id, name, parent, kind, model,
//! sessions), `mail/log.jsonl` (mail and deliveries), `items/log.jsonl` (opened, closed, started). Live state
//! (processes, states, turns) is journalled to /run/coordd/live.json (COORDD_LIVE), which a box restart empties.
//! The design: vault designs/coordd-core-2026-11-10.md (indexes kept per record, no disk or solve under the lock, one
//! record per agent kind, delivery acknowledged once, norms solved per situation and kept).

use crate::cgroup;
use crate::project::Project;
use crate::transcript::{self, Background};
use coord_types::{subject, AgentMode, AgentView, HarnessCapabilities, Body, DeafDo, Event, ItemRecord, MailView, ModelUsage, Notice, Quote, Recognised, Reply, Sent, Snapshot, TeamGrant, Tool, ViewData};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

/// A runner step held back because the runner's command could not start (`Core::runner_unsent`): its ticket, why
/// (the last error), since when (the first failure), and when it is tried again.
#[derive(Clone, Debug)]
pub struct Unsent {
    pub t: u64,
    pub step: u32,
    pub call: String,
    pub worker: String,
    pub tries: u32,
    pub why: String,
    pub since: String,
    pub next: std::time::Instant,
}

/// How long a held runner step waits before it is tried again: no limit on tries, one a minute (paper-research N-101
/// p448).
pub const RUNNER_AGAIN: Duration = Duration::from_secs(60);

pub use coord_types::harness::CLAUDE_EFFORTS;

/// TOKENS as the window says a point: thousands as `350k`, an odd count whole.
fn k(tokens: u64) -> String {
    if tokens % 1000 == 0 { format!("{}k", tokens / 1000) } else { tokens.to_string() }
}

/// What a place serves again ([`Core::served_again`]): one model (an answered turn or call of it, as
/// `router::served_model` names it), the models of one family on the gateway (his press on a gateway login of that
/// type, `router::gateway_family`: a claude login's frees no Codex agent; proto-reader N-1446 p962), or every model (his
/// press on his own account).
#[derive(Clone, Copy, Debug)]
pub enum Served<'a> {
    Model(&'a str),
    Family(&'a str),
    Every,
}

/// The words a turn stopped on rate limit L is given again with (`Core::hand_over`), as coord's: when and on what it
/// stopped, and to go on (paper-research N-1397 p142); given before its reset, what served its model again
/// (`Core::served_again`).
fn resume_words(places: &crate::policy::Places, l: &Limit) -> Option<String> {
    let (by, over) = (l.served.as_deref(), l.served.is_none() && l.what.is_some());
    let reset = l.served.is_none() && l.what.is_none();
    let env = json!({"at": at_secs(l.at), "on": l.stopped_on(), "why": l.why, "by": by, "over": over, "reset": reset});
    crate::render::template_in(places, "resume", &env).map_err(|e| eprintln!("coordd: rendering the resume words: {e}")).ok().flatten()
}

/// SET is a folder's name, as a renderings set must be (`Core::set_renderings`): not empty, no `/`, not hidden.
pub fn renderings_name(set: &str) -> Result<(), String> {
    if set.is_empty() || set.starts_with('.') || set.contains('/') {
        return Err(format!("coord add: not a renderings folder's name: {set:?}"));
    }
    Ok(())
}

/// The folder process PID runs in, for an agent whose session has not said its own yet (N-1888); not asked for a boxed
/// project's, whose folders are its box's paths.
fn started_in(pid: i32) -> Option<String> {
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok().map(|p| p.display().to_string())
}

/// A mode as the window names it (`Core::mode`): `Yolo`, else `Safe`.
/// What KIND's harness takes, in a box or not, read off its measured table; branching a session is Claude Code's only.
pub(crate) fn capabilities(kind: &str, boxed: bool) -> HarnessCapabilities {
    let Some(t) = coord_types::harness::HarnessCapabilities::of(kind, boxed) else { return HarnessCapabilities::default() };
    HarnessCapabilities {
        system_prompt_file: t.prompt_file.taken() || t.prompt_append.taken(),
        system_prompt_replace: t.prompt_replace.taken(),
        tool_disabling: t.tools_off.taken(),
        safety_toggle: t.safety_check_off.taken(),
        prompt_parts_off: t.parts_off.taken(),
        turn_branching: kind == "claude",
    }
}

/// An agent's pace as the owner reads it: Step or Run.
pub(crate) fn mode_name(mode: AgentMode) -> &'static str {
    match mode {
        AgentMode::Step => "Step",
        AgentMode::Run => "Run",
    }
}

fn mode_word(yolo: bool) -> &'static str {
    if yolo { "Yolo" } else { "Safe" }
}

/// A process's environment (/proc/PID/environ, NUL-separated) keeps Claude Code's safety check on: roster lifts it
/// with CLAUDE_CODE_EVEN_MORE_DANGEROUSLY_SKIP_PERMISSIONS (harness-settings).
pub(crate) fn started_check(environ: &[u8]) -> bool {
    !environ.split(|c| *c == 0).any(|w| w == b"CLAUDE_CODE_EVEN_MORE_DANGEROUSLY_SKIP_PERMISSIONS=1")
}

/// A process's command line (/proc/PID/cmdline, NUL-separated) starts a session in the owner's Yolo:
/// `--dangerously-skip-permissions` after coordc's `--`, Claude Code's (`coordc run NAME -- claude …`) or agy-rpc's,
/// which gives it agy (`coordc run NAME -- roster said … -- agy-rpc serve …`), as roster starts them.
pub(crate) fn started_yolo(cmdline: &[u8]) -> bool {
    cmdline.split(|c| *c == 0).skip_while(|w| *w != b"--").any(|w| w == b"--dangerously-skip-permissions")
}

/// Whether an agent of KIND has the owner's Safe · Yolo: Claude Code, and agy, which agy-rpc runs with the same flag
/// (or, through t3code, in a full-access thread; N-1956). Codex and opencode have no such mode.
pub(crate) fn takes_mode(kind: &str) -> bool {
    matches!(kind, "claude" | "agy")
}
use std::time::Duration;
use tokio::sync::broadcast;

/// The message runtime's coordd half: the delivery events raised, the program's messages handed (0.2 head 2).
#[path = "delivery.rs"]
mod delivery;
#[cfg(test)]
#[path = "tests/comments.rs"]
mod comments_tests;

/// Its tests' coord and text helpers, for the tests outside `core` (handover's, ticket's): [`delivery_tests::his_into`].
#[cfg(test)]
pub(crate) use delivery::tests as delivery_tests;

/// An agent's brain, coordd's half: each pass answered from one call on a report from the record (0.3, brain.kind).
#[path = "brain.rs"]
mod brain;

/// The record's one graph as coordd keeps and serves it (`Tickets::graph`, `Request::Graph`; proto-window N-1446
/// p1383, head 1): its test, on delivery's coord.
#[cfg(test)]
#[path = "tests/graph_kept.rs"]
mod graph_kept;

const RING: usize = 20_000;
/// How long the hooks may say a turn runs while the screen shows none before the screen decides (`Core::presence_agrees`).
const STALE_HOOKS: Duration = Duration::from_secs(60);
/// How long a turn begun on a prompt nothing of the model's has followed may stand, the screen showing none and the
/// transcript still, before the screen decides (`Core::presence_agrees`). Longer than STALE_HOOKS: the longest
/// silence seen with the screen idle in a turn that ran (Tovrin-dev, 17 Nov) was 110 s, after its first entry.
const UNANSWERED: Duration = Duration::from_secs(300);

/// Where live state is journalled: COORDD_LIVE, else /run/coordd/live.json (a box's; a host coordd's is under
/// $XDG_RUNTIME_DIR, bin/host-coordd).

fn live_path() -> String {
    std::env::var("COORDD_LIVE").unwrap_or_else(|_| "/run/coordd/live.json".into())
}

#[cfg(test)]
thread_local! {
    /// A test's clock, which `now` gives while it is set: a leg that puts a record's lines in at their own times (head
    /// 3a's leg B, paper-research N-1446 p2255, 4).
    pub static TEST_NOW: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

pub fn now() -> String {
    #[cfg(test)]
    if let Some(t) = TEST_NOW.with(|n| n.borrow().clone()) {
        return t;
    }
    // UTC, seconds, as transcripts write it, so the two compare as text
    at_secs(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64)
}

/// A time as he reads it: hours and minutes on his clock, the minute it falls in (no "Z"; the record stays UTC,
/// paper-research N-1446 p231). ZONE is the machine's (`chrono::Local`), as the window writes every time, or one a test
/// pins; it is his clock only while coordd and his window run on one machine (proto-lead N-1446 p240). A caller that
/// must not show him a time too early rounds up first (`unanswered_words_in`), per proto-lead.
pub fn his_clock<Z: chrono::TimeZone>(t: chrono::DateTime<chrono::Utc>, zone: &Z) -> String
where
    Z::Offset: std::fmt::Display,
{
    t.with_timezone(zone).format("%H:%M").to_string()
}

/// The time T seconds after the epoch, as `now` writes it (`docket_machine::at_secs`, the one writer).
pub fn at_secs(t: i64) -> String {
    docket_machine::at_secs(t)
}

/// A mail record as the log keeps it. Records of the old mail layer read as well (their other fields are ignored).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mail {
    pub id: String,
    pub at: String,
    pub from: String,
    pub to: Vec<String>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub esc: bool,
    /// Goes in while the agent's turn runs, after its next tool call, without stopping it (Claude Code takes a channel
    /// message mid-turn as typed input: after the tool result); a Codex agent's waits for its turn's end.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub soon: bool,
    /// `question` for a question to the owner; empty for a message.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    /// The mail this one answers; the old mail layer's `in_reply_to`, so its threads carry over.
    #[serde(default, alias = "in_reply_to", skip_serializing_if = "Option::is_none")]
    pub re: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quotes: Vec<Quote>,
}

/// What `run` and the hooks know of an agent's live session.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Live {
    /// A compaction of its context runs (its PreCompact hook), until its SessionStart with source `compact` or the
    /// turn's end: shown as the view's `compacting` (paper-research N-101 p1259 point 4), its turn state unchanged.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub compacting: bool,
    /// When the compaction that runs, or the last one, began.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compacting_since: Option<String>,
    /// The coord messages its session holds in its queue now, as the follower last read them (file, the enqueue's
    /// offset): given, not yet in the model's input (0.2 A4: rule 19 lets these wait). Not on the wire.
    #[serde(skip)]
    pub queue: Vec<(String, u64)>,
    /// A dialog coord-pty told of standing on its screen (`Core::popup`): when it was told, its title, and when the
    /// screen was last read again for it. Until its transcript grows after that, a new process starts, or its screen,
    /// read again each minute meanwhile (`Job::DialogLooked`), no longer shows the title: rule 19's minute does not
    /// count (consult-3, N-818.p76 and p78, 1). Not on the wire.
    #[serde(skip)]
    pub dialog: Option<(std::time::Instant, String, std::time::Instant)>,
    /// The last hand-over reached no channel socket of its session (`channel_unreached`'s why), and when: its coord
    /// server is not running, so it takes nothing and reads as stopped (`Core::agent_state`), until its folder changes
    /// (channel.py binds a socket there) or a new process starts (paper-research N-1397.p38, 3: yaaihh's session had
    /// none for an hour, read as idle, and each of its `free`s asked `next`). Not journalled: found again after a
    /// start.
    #[serde(skip)]
    pub unreached: Option<(String, std::time::SystemTime)>,
    /// Claude Code's own word on its screen that this session takes no channel message, as its terminal holder read
    /// it (`Request::Busy`'s `channels_off`: "Channels are not currently available" in a session signed in nowhere,
    /// accounts-lead's walk N-1446 p2423): it is handed nothing, since what it took would be dropped unseen, and reads
    /// as stopped, until a new process starts; what is for it waits (paper-research p2424). Not journalled: its holder
    /// tells it again each minute.
    #[serde(skip)]
    pub channels_off: Option<String>,
    /// The deliveries handed to its session that, idle a minute, never reached it (rule 19's `Core::not_read`): shown
    /// (`AgentView::deaf`), not a stop, so what is due goes again as before and the account rule (`account_unseen`)
    /// acts on its account; for a holder that does not read its screen's word. Until a delivery is seen in the session
    /// (`ticket_seen`) or a new process starts. Not journalled.
    #[serde(skip)]
    pub handed_unseen: Option<String>,
    /// Claude Code's error line above its input box that its sign-in is missing or spent ("Not logged in · Please run
    /// /login", its holder's `Request::Busy` `no_sign_in`), while it stands there: the turn it ended has ended (`failed`),
    /// so it reads idle and restarts, not "working" for good; it is handed nothing, which would fail the same
    /// (accounts-lead N-1446 p2446, fix 2). Not journalled: its holder tells it again each minute.
    #[serde(skip)]
    pub no_sign_in: Option<String>,
    pub pid: Option<i32>,
    pub state: String,
    pub since: String,
    /// The tool calls running (from its transcript; a Codex agent's from its hooks): not journalled, the record
    /// gives them again.
    #[serde(skip)]
    pub tools: BTreeMap<String, Tool>,
    #[serde(skip)]
    pub last_tool: Option<String>,
    /// When its process began: background work started before it died with the old process.
    pub started: String,
    /// Counts turns: a turn end that finishes after a new turn began leaves that turn alone.
    #[serde(default)]
    pub turn: u64,
    /// When coordd last took a turn's end of it (`Core::end_turn`): a job of it on the build runner that ended after
    /// is `fresh` (N-1334). Journalled, so a coordd restart does not show an old end again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_ended: Option<String>,
    /// Whether its screen shows a turn running, as its terminal holder last told (`Core::busy`); None until one tells
    /// (a holder from before), when the transcript decides. Journalled: a holder tells only a change, so a coordd
    /// restart that forgot it fell back to the transcript until the screen next changed (10 Nov, MEANING).
    #[serde(default)]
    pub screen_busy: Option<bool>,
    /// Whether a turn runs, as the session's hooks last said (design-two-systems §10, presence from hooks): its
    /// UserPromptSubmit says one began, Stop or StopFailure that it ended, and an interrupt (which runs no hook) is the
    /// transcript's. Decides before the screen, which stays the fallback; a disagreement is logged (`main::follow`).
    /// None until a hook says (a session from before this coordd). Journalled with the screen's.
    #[serde(default)]
    pub hook_busy: Option<bool>,
    /// Since when the hooks and the screen have disagreed, and whether that was logged.
    #[serde(skip)]
    pub disagree: Option<(std::time::Instant, bool)>,
    /// Since when a process started by `coord run` has waited for its session's start hook: nothing is handed to it
    /// until that comes, since a session that is still loading drops what it is given (paper-research m1005, 1). Not
    /// journalled: a process found running at load is past its start.
    #[serde(skip)]
    pub awaiting_start: Option<std::time::Instant>,
    /// A Claude Code agent's new effort or model (`coord effort`, `coord model`) is recorded and waits for this process
    /// to take it: it is restarted on its own session once not in a turn (`Core::to_restart`), and nothing is handed to
    /// it before. Holds the MODEL:EFFORT this process runs on (as the record wrote it), so a later change back to it
    /// drops the restart. A new
    /// process starts without it (it runs at the recorded level). Journalled, so a coordd restart still restarts it.
    #[serde(default)]
    pub effort_due: Option<String>,
    /// This process is to go to another account, or onto or off the owner's accounts (accounts/mover.rs): why, in
    /// words. As with `effort_due` it is restarted on its own session once not in a turn, and nothing is handed to it
    /// before. Journalled.
    #[serde(default)]
    pub account_due: Option<String>,
    /// This process was started in Yolo: its command line had Claude Code's `--dangerously-skip-permissions` at its
    /// `coord run` (`Core::starts_in_yolo`), on a host session. What the view's `mode` reads while it runs, never the
    /// record's asked mode, so a failed restart, and coordd's own restart after one, still read what runs (consult-3,
    /// N-1400 p5, p7). Journalled, as `compact_runs` is.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub yolo_runs: bool,
    /// The owner's new mode (`Core::mode`) waits for this process to take it, as `effort_due` does: restarted on its
    /// own session once not in a turn. A change back to `yolo_runs` drops it; a restart that did not happen takes it
    /// away (`Core::restarted`), and the view's `mode_due` says the mode waits for the next start. Journalled.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub yolo_due: bool,
    /// The safety check this process was started with (`Core::starts_check`); None: not read. Journalled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_runs: Option<bool>,
    /// The owner's new safety check waits for this process to take it, as `yolo_due` does (`Core::check_taken`).
    /// Journalled.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub check_due: bool,
    /// When a turn of this process was cut by its account's limit (its `failed` hook with a limit, accounts/mover.rs),
    /// seconds: it is restarted under an account that can take requests, and that start goes on with the cut turn
    /// by Claude Code's own way (`Core::account_for_end`). Its parent is not told of a turn of this process that ends on
    /// an API error from then on: the restart is what follows. Journalled; gone with the process, since the one that
    /// takes its place took the turn up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_cut: Option<i64>,
    /// The account on which a delivery handed to this process was never seen, the first such miss there
    /// (`Core::account_unseen`): it restarts on that account to take the delivery again, and until it has, no other
    /// miss there makes the account deaf, as both may be this one session's (consult-3, N-412 post 2131). Journalled;
    /// gone with the process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unseen_on: Option<String>,
    /// The compaction point this process runs at: the one it was started with (`Agent::compact` at `coord run`; none:
    /// the coord's own point, or Codex's default), or the one `/autocompact` typed in it set since (`compact_typed`).
    /// `coord compact` reaches a harness at a start only (N-490 p5). Journalled, so a coordd restart still knows what
    /// the running session compacts at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact_runs: Option<u64>,
    /// The point given to this Claude Code session's terminal as `/autocompact` (`Core::compact`, N-804.p16) that its
    /// record has not answered yet; its answer (`compact_typed`) clears it. Journalled with the rest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact_typing: Option<u64>,
    /// A turn of this process stopped on a rate limit, not its account's (its `failed` hook says `rate_limit`, and no
    /// account's limit cut it: accounts/mover.rs): nothing is handed to it until the limit resets, and then it is given
    /// its turn again, once (`Core::hand_over`). Five turns the gateway's 429 stopped at 02:08Z on 21 Nov stood, read
    /// as idle with nothing due, until he restarted them at 03:59 (paper-research N-1397 p142). Journalled, so a coordd
    /// restart keeps it; gone with the process, and with a turn that begins meanwhile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<Limit>,
    /// The wait, seconds, of this process's last turn stopped on a rate limit that named no reset (`Core::limit_of`):
    /// the next such stop waits twice as long, at most `LIMIT_WAIT_MOST`, so a limit that holds on gives its turn again
    /// a few times, not at every key-again-after (consult-3 N-1397 p151; proto-lead's bound there: 60, 120, 240 s …).
    /// None again once a turn ends by its Stop, on another error, or by an esc (coordd's own, `Core::interrupted`, or
    /// one typed in its terminal, the transcript's cut). Journalled with the rest; gone with the process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_wait: Option<i64>,
    /// A `fork` waits for this process (part 4, `Core::fork_now`): it is restarted under its own name on a copy of its
    /// session cut where the fork goes on from ([`ForkCut`]), once not in a turn, as for `effort_due`, and nothing is
    /// handed to it before; the message goes to the new process. Journalled; a new process starts without it, and a
    /// restart that did not happen takes it away (`Core::restarted`): the message goes then as a `deliver`'s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_due: Option<ForkCut>,
    /// The owner's edit of its system prompt waits for this process ([`SystemDue`], `Core::system_prompt`): it is
    /// restarted with the version, once not in a turn, as for `effort_due`. Journalled; a new process starts without
    /// it, and a restart that did not happen takes it away (`Core::restarted`): it runs with the prompt it had.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_due: Option<SystemDue>,
    /// An agy session's last pull (`coord read-brain`, pull.rs; N-1998): from it on, nothing is pushed into the session
    /// but the owner's words while no turn runs. Journalled; gone with the process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull: Option<crate::pull::Pull>,
    /// What was pushed to an agy session and taken, not yet named by a pull: its next pull's work, answered by its
    /// report (pull.rs; N-1998 p7, 1). Gone with the process.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pushed: Vec<String>,
}

/// The flavor's guesses of models whose maker is not named, at its root (`model_list`; walk item 29).
pub const MODEL_GUESSES: &str = "model-guesses.toml";
/// Past this a `model-guesses.toml` is not read: its four entries are about 1 KB.
pub const MODEL_GUESSES_CAP: u64 = 64 * 1024;

/// The cut of `Request::SystemPrompt` from AFTER, a byte of FILE, the transcript of the session its agent runs
/// ([`Core::system_prompt_from`]): a copy cut there, where AFTER ends a turn of it (`follow::turn_end`); none without
/// AFTER. It reads the transcript up to AFTER, tens to hundreds of MB for a long-running agent, so it runs off the core
/// lock (docket-core N-1446 p2471).
pub fn system_cut(file: Option<String>, after: Option<u64>) -> Result<Option<ForkCut>, Reply> {
    let (Some(file), Some(upto)) = (file, after) else { return Ok(None) };
    if !crate::follow::turn_end(&file, upto) {
        return Err(err(format!("coord system-prompt: byte {upto} is not the end of a turn of {file}")));
    }
    Ok(Some(ForkCut { place: format!("byte {upto}"), file, upto }))
}

/// The owner's edit of an agent's system prompt (`Live::system_due`): VERSION, the number of its file
/// `$COORD/prompts/NAME.system/VERSION.md`; REPLACE, in place of Claude Code's own rather than appended; CUT, the copy
/// of its session it goes on from (PLACE the bytes, as said), None for its conversation as it stands. roster's
/// `restart NAME --system VERSION append|replace [--cut FILE UPTO]` starts it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SystemDue {
    pub version: u32,
    pub replace: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut: Option<ForkCut>,
}

/// Where a `fork` cuts an agent's session (`Live::fork_due`): PLACE, the delivery it goes on from; FILE, its session's
/// transcript; UPTO, the bytes of FILE the copy keeps, up to the end of the entry that took PLACE's message
/// (`follow::fork_cut`). roster's `restart NAME --cut FILE UPTO` makes the copy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ForkCut {
    pub place: String,
    pub file: String,
    pub upto: u64,
}

/// The longest wait for a rate limit that names no reset (`Live::limit_wait`): 30 minutes (proto-lead N-1397 p151).
const LIMIT_WAIT_MOST: i64 = 30 * 60;

/// A turn stopped on a rate limit (`Live::limit`): AT, when (seconds); UNTIL, when the limit resets, as its words named
/// it, else at the policy's `key-again-after`, or when its model was served again before that (`Core::served_again`);
/// WHY, the words; SERVED, what served it again then (`the gateway answered docket-core's claude-opus-5-5 at 11:06`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Limit {
    pub at: i64,
    pub until: i64,
    pub why: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub served: Option<String>,
    /// What stopped it when not a rate limit: the provider overloaded, a server error (`Core::limit_of`; the owner's
    /// walk item 14, yaaihh/fresh N-1729 p24). None: a rate limit, as every limit journalled before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub what: Option<String>,
}

impl Limit {
    /// What stopped the turn, in words: "a rate limit", "the provider overloaded", "a server error".
    pub fn stopped_on(&self) -> &str {
        self.what.as_deref().unwrap_or("a rate limit")
    }
}

impl Live {
    /// This process is to be restarted on its session once not in a turn (a new effort or model, another account).
    pub fn restart_due(&self) -> bool {
        self.effort_due.is_some() || self.account_due.is_some() || self.yolo_due || self.check_due || self.fork_due.is_some() || self.system_due.is_some()
    }
}

/// What coordd hands an agent's session, off the core's lock (`main::senders`): one sender per agent does them in
/// order, so a session slow to answer holds up only its own mail, never the core (owner m249, 9 Nov: a 10 s push to
/// one agent under the lock made orch's Stop hook miss its 5 s, and the mail in its late answer was lost).
pub enum Job {
    /// A ticket changed: restate the agents shown waiting (`Core::refresh_waits`), which needs the lock and may mail.
    Waits,
    /// Mail IDS through Claude Code's coord channel, the first of SOCKS that answers (`channel_socks`; LINE: the
    /// channel's events).
    /// TICKETS: Docket deliveries to render and add to LINE's events (their ids are in IDS beside the mails').
    /// AGAIN: the deliveries among TICKETS handed a second time, not read the first (`not_read`).
    /// REPLIES_ON: the ticket the rule Reply will post this turn's last message on, if the agent is given nothing else
    /// (`Core::ticket_replies_on`): what a posts delivery's footer says.
    /// REST: what the delivery program's message names at its foot, waiting on (`deliver … listing REST`), by id.
    /// DIGEST: what goes with it by first lines (`deliver … digest PULL`, I-1480), by id, drawn at its foot
    /// (`Core::digest_named`); its deliveries are among IDS, given with the message.
    Channel { name: String, ids: Vec<String>, socks: Vec<String>, line: String, tickets: Vec<(docket_machine::Delivery, docket_machine::TicketView)>, replies_on: Option<String>, again: Vec<String>, into: bool, rest: Vec<String>, digest: Vec<String> },
    /// Mail IDS through a Codex app-server (TEXT: the batch).
    /// EFFORT: the reasoning effort its record names (MODEL:EFFORT), given with the turn this starts (`coord effort`).
    Codex { name: String, ids: Vec<String>, rpc: String, session: String, socket: String, text: String, tickets: Vec<(docket_machine::Delivery, docket_machine::TicketView)>, replies_on: Option<String>, again: Vec<String>, effort: Option<String>, into: bool, rest: Vec<String>, digest: Vec<String> },
    /// Docket deliveries to a program agent (the runner): run by the project's runner command, then given.
    Program { name: String, deliveries: Vec<(docket_machine::Delivery, docket_machine::TicketView)> },
    /// Docket deliveries to a host session (`Core::hand_hosts`): written to the feed for coord-relay, then given.
    Host { name: String, deliveries: Vec<(docket_machine::Delivery, docket_machine::TicketView)> },
    /// Docket deliveries to `clerk`, the agent that files the owner's words (kind filing): coordd answers its steps
    /// itself (`ticket_answer("clerk", ...)`), so it has no session and no registry entry.
    Clerk { deliveries: Vec<(docket_machine::Delivery, docket_machine::TicketView)> },
    /// The watcher's step of a pass of an agent's brain (0.3, brain.rs), answered by coordd from one call
    /// (`clerk::answer`), on a QUEUE of the agent's own (`brain NAME`): no agent's call waits behind another's, nor the
    /// clerk's work behind it. A delivery run's `next` is no job: it is answered at once, with no call (I-1480,
    /// delivery.rs `next_given`).
    Deliver { queue: String, delivery: docket_machine::Delivery },
    /// Docket deliveries to the minder where no agent holds the role (`shape::MINDER`): coordd answers its shape steps
    /// itself (`shape::answer`).
    Minder { deliveries: Vec<(docket_machine::Delivery, docket_machine::TicketView)> },
    /// Docket deliveries to `coord` (change::COORD), the text-change kind's check and load: coordd answers them itself
    /// (change.rs, N-865), the check read off the lock, the load under it.
    Coord { jobs: Vec<crate::change::Job> },
    /// Start AGENT, a short-lived agent made for step CALL of ticket VIEW (`step fresh`): a roster agent with the
    /// review prompt, its one delivery the step (it is then due to it like any agent's). FROM: a fork of that agent,
    /// its conversation copied as it stands now (`step fork`, `let agent x = fork`, N-101; fork.rs). CUT: a copy of
    /// FROM (`Effect::Copy`, N-1104 p1154), its session cut after the line that delivery was given on, not as it stands.
    Start { agent: String, call: String, view: docket_machine::TicketView, layer: Option<String>, from: Option<String>, cut: Option<String> },
    /// Layer ID's view made (SPEC), or the layer ended (none): `layer::open`, `layer::end`.
    Layer { id: String, spec: Option<Result<crate::layer::Spec, String>> },
    /// Remove each layer ID whose ticket has closed, saying whether the top ticket of its tree has (`layer::remove`).
    LayersRemove { ids: Vec<(String, bool)> },
    /// End AGENT: pin what ticket VIEW references in its transcript (`Input::Pinned`), then remove it.
    End { agent: String, view: Option<docket_machine::TicketView> },
    /// Escape typed into NAME's terminal (Claude Code), or a Codex interrupt.
    Interrupt { name: String, pty: String, codex: Option<(String, String, String)> },
    /// LINE given to NAME's terminal holder at PTY (`pty::COMMAND`), which types it once no turn runs and its input
    /// box is empty (`Core::compact`'s `/autocompact N`, N-804.p16).
    Command { name: String, pty: String, line: String },
    /// NAME restarted on its own session (`roster restart`), so it runs at the effort its record now names
    /// (`coord effort`); PID: its process before, to tell whether it was.
    Restart { name: String, pid: Option<i32> },
    /// AGENT's TEXT posted on ticket T as its `coord post` posts it (its norms asked off the lock), under the post
    /// ANSWERS when it names one: a part of its last message for a thread it was not handed (`Core::reply_in_parts`,
    /// paper-research N-412.p2208).
    Post { agent: String, t: String, text: String, answers: Option<u32> },
    /// NAME's screen read again from its terminal holder, off the lock: once it no longer shows the dialog TITLE
    /// coord-pty told of, `Core::dialog_gone` (rule 19's minute counts again; consult-3, N-818.p78, 1).
    DialogLooked { name: String, title: String },
    /// NAME's turn failed on its account's limit: read which reset the turn's row in TRANSCRIPT names
    /// (`accounts::limits::row_of`, rows from SINCE on, milliseconds), and hand it to `Core::account_said` as
    /// `limit_row` under the config folder CONFIG the hook named. TRANSCRIPT is the hook's word, so the agent's: it is
    /// read only when it lies under CONFIG's `projects`.
    LimitRow { name: String, transcript: String, config: Value, since: i64 },
    /// Read the resets Claude Code offers sign-in account ID off agent NAME's screen, where a turn just met the
    /// account's limit (the notice under it; accounts/resets.rs `offers`), and keep them in the account's state. On
    /// NAME's own queue: read before NAME moves off the account, since its `Restart` is queued after.
    ResetOffers { name: String, id: String },
    /// Move an agent to another view, out of its turn (moves.rs, N-101 p406 C).
    Move(crate::moves::Move),
    /// Write what a session said of its account's limits into the account's state (accounts/mover.rs `Keep`): the
    /// account's lock is waited for here, never under the core's lock. One sender does them all, in their order.
    AccountSaid(crate::accounts::mover::Keep),
    /// Ask Claude Code whether sign-in account ID is signed in, and record it (main.rs `account_check`), as the
    /// owner's `coord account check ID` does, without its read of the windows: a turn of a session on it was refused
    /// its login.
    AccountCheck { id: String },
    /// Read OpenCode Go's usage for account ID with the key coordd keeps for it and record it (main.rs `go_read`): a
    /// turn of an opencode agent on it ended (`Core::opencode_said`). The read uses nothing of the plan. Read again at
    /// once while a turn's end during the read asked for one more (`Core::go_read_done`).
    GoUsage { id: String },
}

/// A delivery handed without an id ([`Core::bare_handed`]): its id, the text sent, and the transcript and the length it
/// had at the hand-over, after which its entry is looked for.
#[derive(Clone, Debug)]
pub struct Bare {
    pub id: String,
    pub text: String,
    pub file: String,
    pub from: u64,
}

/// Mail on its way to an agent's session: its sender has not had the session's answer yet.
pub struct Inflight {
    pub ids: Vec<String>,
    pub tries: u32,
    /// The rate limit whose turn this hand-over gives again (`Live::limit`, which keeps it till then): taken, that
    /// limit is over (`Core::handed`). Not journalled: a coordd restart before it is taken finds the limit still there,
    /// and gives the turn again (consult-3 N-1397 p151). The limit itself, not a flag, so a turn given again that
    /// begins and is stopped anew before its hand-over's answer comes keeps the new limit.
    pub resume: Option<Limit>,
}

pub struct Agent {
    pub id: String,
    pub name: String,
    pub parent: String,
    /// Its role (registry `role` events; empty: a member).
    pub role: String,
    pub kind: String,
    pub model: String,
    pub session: Value,
    pub live: Live,
    pub background: Option<Background>,
    /// When it was first registered.
    pub created: String,
    /// Its sessions' transcripts, in the order they began.
    pub sessions: Vec<String>,
    /// Its tokens over all of its sessions, and its transcripts' latest time (counted by the usage reader, main.rs).
    pub usage: Vec<ModelUsage>,
    /// How full its context is: its latest request, by the usage reader (`Core::set_context`).
    pub context: Option<coord_types::AgentContext>,
    pub last_active: Option<String>,
    /// Why its last turn ended without finishing (Claude Code's StopFailure: an API error), until a new turn begins.
    pub failed: Option<String>,
    /// In a boxed project: why its box could not be made, at its registration or its last start (N-101 post 434). It is
    /// not started until its box is made; `coord ls` shows the reason.
    pub no_box: Option<String>,
    /// The last turn whose end coordd took (`Core::end_turn`): taken once, however often a poll sees it ended.
    pub ended_turn: u64,
    /// A Claude Code turn's end taken (its Stop hook, or the screen) before its transcript showed it: nothing is
    /// handed to it until the transcript has (the turn's final output read as its reply, or its end shown with none,
    /// or a cut), so what would begin the next turn cannot come first and its answer be read as this turn's reply
    /// (paper-research N-101 p1412: N-3.p2, sent to wait, was handed at the Stop hook, and its answer was posted as the
    /// reply to N-3.p1). Since when; a transcript that shows no end within OUTPUT_WAIT is let go, in the reconcile log,
    /// so nothing stays held for good.
    pub output_owed: Option<std::time::Instant>,
    /// The transcript began its next turn before the Stop hook of the turn before reached coordd: that Stop, when it
    /// comes, is the old turn's and ends nothing (consult-3, N-807.p67).
    pub stop_owed: bool,
    /// The transcript has shown the first entry of the turn coordd counts as running (`Core::transcript_read`): a
    /// begin it shows after that is a next turn, whose turn before ended unseen.
    pub begin_seen: bool,
    /// Its memory (MB) and CPU time (s) as the timer last read them from its cgroup (`main::count_usage`).
    pub mem_mb: Option<u64>,
    pub cpu_s: Option<f64>,
    /// Its transcript's and its folder's bytes as the task last read them (`footprint::Kept`).
    pub footprint: (Option<u64>, Option<u64>),
    /// Its session's transcript, as `note_session` found it.
    transcript_file: Option<String>,
    /// Its team grant (`coord team grant`, registry `team`), with the agents added under it.
    pub team: Option<TeamGrant>,
    /// The agent under whose grant it was added (registry `add` with `member_of`).
    pub member_of: Option<String>,
    /// The agent whose conversation it is a copy of (registry `add` with `from`: `roster fork`, the window's Spin off):
    /// a spin-off, which the owner ends with Join back, Close or Delete (`Core::spin_end`; his walk item 19).
    pub from: Option<String>,
    /// What its last turn's background work waited on (`Core::turn_end_state`), kept so a refresh of the project's
    /// waits (`Core::refresh_waits`) can restate the whole wait.
    pub bg_waits: Vec<String>,
    /// When the owner ended it for good (`coord ended NAME`, registry `ended`): it is never started again, no step is
    /// given to it, and its sessions stay readable. Only the owner undoes it (`coord ended --undo NAME`).
    pub ended: Option<String>,
    /// It runs on the owner's own accounts (registry `native`; accounts/mover.rs), not through the model route.
    pub native: bool,
    /// The renderings its deliveries are drawn from (`Places::renderings`; registry `renderings`): set by its `add`
    /// line or a `renderings` line, kept over a re-add; none, the project's own.
    pub renderings: Option<String>,
    /// The account its session was last started with (`Core::account_for_end`), for the view.
    pub account: Option<String>,
    /// Every account was out when its start or move was last tried: when
    /// to try again (seconds since the epoch). Its restart is not tried again before, and one that does not run is
    /// started then (`Core::to_restart`).
    pub account_wait: Option<i64>,
    /// Its last start found no account signed in at all (`Core::account_for_end`): what it waits for is a sign-in,
    /// and its view says so. Read only while `account_wait` is set.
    pub account_none: bool,
    /// It was started to go on with a turn its account's limit had cut (`Core::account_for_end` answered Claude Code's
    /// variable), until a turn of it begins. While it stays, Claude Code has not taken the turn up, and its view
    /// says so (`Core::account_note`).
    pub account_resumed: bool,
    /// Its process ended when coordd last went down (a reboot, a stop of the machine), and this coordd handed it to
    /// `roster resume` or starts it again in its view (`Core::hand_over_running`). Its next start goes on with a turn
    /// that end cut, when its transcript shows one running (`Core::account_for_begin`, paper-research N-1104 p785: the
    /// stop at 00:35 on 22 Nov dropped every running turn). Held until its next process claims it (`new_process`),
    /// so a start refused meanwhile (every account out) still goes on with it later. Not kept over a coordd restart.
    pub down_cut: bool,
    /// It is the owner's accounts' and runs through the gateway meanwhile, as its account was out and no other could
    /// take it (accounts/mover.rs, the registry's `account` line with `left`), until one is back.
    pub gateway: Option<crate::accounts::mover::OnGateway>,
    /// Its last move, onto an account or the gateway: the registry's last `account` line of it
    /// (accounts/mover.rs `account_record`), for its view (0.2 list, line 22: "the window says which agent went where
    /// and why").
    pub account_move: Option<crate::accounts::mover::AccountMove>,
    /// Why it was put on the owner's accounts (the switch, `coord native`, a gateway list without its model), until the
    /// move its next start makes onto one says it in place of "started". Kept here for one that did not run then; a
    /// running one carries it on its process (`Live::account_due`), which a reload does not keep, so the next coordd
    /// reads it back from the registry's `native` line, and the `account` line of that move clears it.
    pub native_why: Option<String>,
    /// Where its context compacts from its next start (`coord compact`, or `/autocompact` typed in its session:
    /// `compact_typed`; registry `compact`; paper-research N-490 p8), given to that start in its harness's own way
    /// (session.json's `compact`, roster: Claude Code's `--autocompact`, Codex's `model_auto_compact_token_limit`).
    /// None: the coord's own point (`context_window`) for Claude Code, Codex's own default for Codex.
    pub compact: Option<u64>,
    /// When `compact` was last set (the registry line's `at`): a point typed in the session before that is not taken
    /// again when its transcript is read again.
    pub compact_at: String,
    /// Its model's whole context window, as its status line last gave it (`context_window.context_window_size`).
    pub context_max: Option<u64>,
    /// Its mode is Yolo: its Claude Code session starts with Claude Code's own bypass of permission prompts (roster
    /// reads session.json's `mode`; lib/route.sh `session_place`). False: Safe, his own permission mode. Its own mode,
    /// else the project's default, else the yaaihh-wide one (`Core::resolve_mode`). What the running process was
    /// started with is its `Live::yolo_runs`.
    pub yolo: bool,
    /// The mode the owner set for this agent itself (`Core::mode`; registry `mode`, kept over a re-add). None: it
    /// takes the default.
    pub own_mode: Option<bool>,
    /// Where `yolo` comes from: `agent`, `project`, `yaaihh`, or empty when nothing set it (Safe).
    pub mode_from: &'static str,
    /// Its pace, the owner's (N-1788; registry `control`, kept over a restart): `Step`, each turn that would go to it
    /// staged for him as it is (no gathering of his burst, no fold call choosing what goes); `Run`, as ever.
    pub control: AgentMode,
    /// What the last applying of the owner's system prompt to it said when it failed (roster's own words); None once
    /// one applied. Shown in its view (`AgentView::apply_error`), not only in coordd's log.
    pub apply_error: Option<String>,
}

/// What an agent can take now (`Core::agent_state`, 0.2 A1). Compacting and starting are handled as working by the
/// program (the owner: "yes just like if an agent is working"); they are kept apart so the line under his message says
/// why (`sent · paper-research is compacting since 12:25`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AgentState {
    Working,
    Idle,
    /// A compaction of its context runs (`Live::compacting`), since when it began.
    Compacting { since: Option<String> },
    /// Its process cannot take a prompt yet: a due restart (a new effort, model, account or Safe · Yolo), its account's
    /// limit cut its turn, a new process before its start hook, a pause after its session did not take what it was
    /// handed (rule 11 of consult-3's reading), an esc stopping its turn, its turn's end taken before its session shows
    /// it, or a move to another view (docket-core2, N-807.p127).
    Starting { why: String },
    /// No process: ended for good by the owner, or not running.
    Stopped { why: String },
    /// Not running, every account out: when it is tried again (seconds since the epoch).
    OutOfLimit { until: Option<i64> },
}

impl Agent {
    pub fn running(&self) -> bool {
        self.live.pid.is_some()
    }
    /// Why its session takes no messages, in his words with what he can do (`AgentView::deaf`; paper-research N-1446
    /// p2427): its screen's word first (`Live::channels_off`), then no channel socket (`Live::unreached`), then
    /// deliveries handed and never seen there (`Live::handed_unseen`). None while it takes them, or it does not run.
    /// With the words, what he can do as acts (`AgentView::deaf_do`, proto-window N-1446 p2538): the words end with
    /// the acts' own words, in their order, joined by ", or ", so the window makes each a press without reading ours.
    /// None when nothing is his: an account move is due, and it restarts to take them again.
    pub fn deaf_words(&self) -> Option<(String, Vec<DeafDo>)> {
        if !self.running() {
            return None;
        }
        let account = |words: &str| DeafDo { act: coord_types::DEAF_ACCOUNT.into(), words: words.into() };
        let (why, can) = if let Some(why) = &self.live.no_sign_in {
            (format!("its turn failed and it takes no messages (Claude Code says “{why}”)"), vec![account("pick an account for this agent")])
        } else if let Some(why) = &self.live.channels_off {
            // an organization's policy turns them off for every session of that account; otherwise the session is
            // signed in to no account that has them (a scratch HOME, an API key), or goes through another provider
            let todo = if why.contains("org policy") || why.contains("silently dropped") { "pick another account for this agent" } else { "pick an account for this agent" };
            (format!("its session takes no messages (Claude Code says “{why}”)"), vec![account(todo)])
        } else if let Some((why, _)) = &self.live.unreached {
            (format!("its session takes nothing ({why})"), vec![DeafDo::restart()])
        } else {
            let why = format!("what was handed to its session never reached it ({})", self.live.handed_unseen.as_ref()?);
            if self.live.account_due.is_some() {
                return Some((format!("{why}; it restarts to take them again"), Vec::new()));
            }
            (why, vec![DeafDo::restart(), account("pick an account for this agent")])
        };
        let todo: Vec<&str> = can.iter().map(|d| d.words.as_str()).collect();
        Some((format!("{why}; {}", todo.join(", or ")), can))
    }
    /// Its session runs in Yolo (`Live::yolo_runs`); one that does not run, its next start's: the owner's `yolo`.
    pub fn yolo_runs(&self) -> bool {
        if self.running() { self.live.yolo_runs } else { self.yolo }
    }
    /// Its session's transcript: the file the session writes (`written_transcript`), not only the path its hook named.
    /// Found once, when the session is named (`note_session`), not on every look.
    pub fn transcript(&self) -> Option<String> {
        self.transcript_file.clone()
    }
    fn socket(&self) -> String {
        self.session.get("socket").and_then(Value::as_str).unwrap_or("").to_string()
    }
    /// Count the session's transcript among its sessions, once; a path under a folder that has MOVED is read where
    /// it went (`moved_transcript`).
    fn note_session(&mut self, moved: &[(String, String)]) {
        self.transcript_file = self.session.get("transcript_path").and_then(Value::as_str).map(|named| {
            let named = moved_transcript(named, moved).unwrap_or_else(|| named.to_string());
            written_transcript(&named).unwrap_or(named)
        });
        if let Some(t) = self.transcript().filter(|t| !self.sessions.contains(t)) {
            self.sessions.push(t);
        }
    }
}

/// The mail's indexes (design coordd-core-2026-11-10 §1), by position in `Core::mails`.
#[derive(Default)]
struct MailIndex {
    /// A mail's position by its id (the first, should an id repeat).
    id: HashMap<String, usize>,
    /// Each mail's thread root, by position.
    root: Vec<usize>,
    /// Each thread's mails, oldest first, by its root.
    thread: HashMap<usize, Vec<usize>>,
    /// Each sender's mails, oldest first.
    by_from: HashMap<String, Vec<usize>>,
    /// Each recipient's mail not delivered or taken back.
    held: HashMap<String, std::collections::BTreeSet<usize>>,
    /// A message net's rewrite of a mail, by position: the position of the mail it stands for, whose place it takes
    /// in its recipients' order (log `stands_for`).
    stands_for: HashMap<usize, usize>,
    /// How many messages (`mN`) and questions (`qN`) there are: the next numbers.
    messages: usize,
    questions: usize,
}

/// The caller's name as a sender: the owner, or the agent.
pub enum Who {
    Owner,
    Agent(String),
}

impl Who {
    pub fn name(&self) -> String {
        match self {
            Who::Owner => "owner".into(),
            Who::Agent(a) => a.clone(),
        }
    }
}

pub struct Core {
    pub coord: String,
    pub base: String,
    pub agents: BTreeMap<String, Agent>,
    mails: Vec<Mail>,
    /// The mail indexes, kept as each mail is added (design coordd-core-2026-11-10 §1): never a scan of the mail.
    ix: MailIndex,
    delivered: HashSet<(String, String)>,
    /// (mail, recipient) its sender took back before it was delivered (`unsend`).
    withdrawn: HashSet<(String, String)>,
    /// Questions withdrawn (`coord ask --drop`).
    dropped: HashSet<String>,
    pub(crate) items: Vec<ItemRecord>,
    pub views: BTreeMap<String, ViewData>,
    /// The flavour's look (change.rs, N-865/N-866): its `look.toml` as read at start and by a text-change's load.
    pub look: Option<Value>,
    /// A text-change's load found committed with no line at this start (change.rs `text_recover_line`): its ticket
    /// and commit, which its owed load step answers with.
    pub text_recovered: Option<(u64, String)>,
    pub project: Option<Project>,
    seq: u64,
    /// This run of coordd (protocol 1): its start time and pid, in the snapshot, so a watch resumes only within it.
    pub run: String,
    /// The events a reconnecting watcher may be sent instead of a snapshot: each still news, at most [`RING`]. An
    /// event that carries whole state (an agent's view, a project view, a snapshot) leaves the ring when a
    /// newer one replaces it, so the ring's size follows what changed, not how often a view source prints (8 Nov:
    /// 20,000 kept copies of a 470 KB runner view ran coordd to 23 GB and the box's OOM killer). A ticket's change
    /// replaces its change before it, carrying the posts that one carried (`kept_change`); a ticket whole, both.
    pub(crate) ring: VecDeque<(u64, Option<Event>)>,
    /// Where in the ring the latest event of each thing that whole-state events carry is (`event_key`), by its seq:
    /// the one a newer event of it empties, found without a scan.
    latest: HashMap<String, u64>,
    /// How many events the ring holds (its emptied places not counted).
    held_events: usize,
    /// The held events' length as JSON, as a watcher is sent them (`json_len`), and each one's by its seq, taken off
    /// when it leaves the ring: what the minute's costs line says the ring holds (paper-research N-1446 p2299).
    ring_bytes: u64,
    ring_len: HashMap<u64, u64>,
    /// Each ticket's posts as its last event sent them, by id, one hash a post (`ticket_body`): what a `ticketchange`
    /// leaves out.
    pub(crate) sent_posts: HashMap<String, Vec<u64>>,
    /// The last event no longer in the ring that a watcher would still need: one at or after it is caught up from
    /// the ring.
    floor: u64,
    pub tx: broadcast::Sender<Event>,
    /// The agents' model requests as their transcripts record them, by request id (`coord_types::Sent`; requests
    /// already older than [`SENT_DAYS`] when read are not kept).
    sent: HashMap<String, Sent>,
    /// The questions the recogniser found in agents' plain mails to the owner, by mail (vault
    /// designs/decisions-policies-forks-2026-11-08.md §8): such a mail starts a question thread like `coord ask`.
    recognised: HashMap<String, Recognised>,
    /// What the kinds' and calls' check refused and what replay found diverging, as last told (`Core::refusals`).
    pub refusals: Vec<String>,
    /// What coordd could not do and says so (paper-research N-8), each with what it is of: "log" (the instance's: a
    /// record log cannot be written), else the call that failed on its ticket ("intake", "minder"). In memory
    /// only: a restart clears them.
    notices: Vec<(String, Notice)>,
    /// The engine's own calls (`intake`, `minder`) whose having no model to run on coordd's journal has said, until one
    /// of that kind finds a model again (`own_call_failed`).
    no_model_said: HashSet<String>,
    /// The agents' reads outside their folders waiting on the owner (read_ask.rs); in memory only: a restart lets their
    /// calls run out and the agents read again.
    pub read_asks: crate::read_ask::Asks,
    /// Each agent group's `oom_kill` count (its `memory.events`) as last read: a rise is a process killed at its
    /// memory limit, told to the agent once (`oom_round`); read first at coordd's start, so no earlier rise is told.
    pub oom_seen: BTreeMap<String, u64>,
    /// The mails a net filed from an agent's turn (a live `ask` or `post`), with the run that filed each.
    filed: HashMap<String, String>,
    /// The filed mails the owner took back (`Request::Unfile`): no longer a question, hidden from threads.
    unfiled: HashSet<String>,
    /// Mails held for their message net (`deliver` live): delivered only as the net says, or unchanged when it takes
    /// nothing. Not kept over a restart: a mail still held then is delivered unchanged.
    clerking: HashSet<String>,
    /// The agents the owner's rename is running for (`owner_roster`: from his press until roster's answer): no edit of
    /// their system prompt is taken meanwhile, as the rename moves its files (docket-core N-1446 p2471).
    pub renaming: HashSet<String>,
    /// The `harness.toml` files `switches_view` read, each with the (modified time, length) it was read at: a view
    /// reads a file again only when that changed, so a snapshot costs a stat per file, not a parse.
    pub harness_files: std::sync::Mutex<HashMap<std::path::PathBuf, (Option<(std::time::SystemTime, u64)>, Option<toml::Table>)>>,
    /// Mails their message net did not pass on as they were, with what it did instead (the mails it delivered in their
    /// place, or why it sent them back): never delivered themselves unless the owner says so (`Request::Release`).
    clerked: HashMap<String, String>,
    /// Mail being handed to each agent's session, until the session's answer (`handed`).
    pub inflight: HashMap<String, Inflight>,
    /// When an agent's session may be tried again after it did not take mail.
    retry_at: HashMap<String, std::time::Instant>,
    /// Since when each agent has held deliveries given to it while idle (`not_read`).
    idle_given: HashMap<String, std::time::Instant>,
    /// The agents a `roster restart` for a new effort runs for (`Job::Restart`), so it is started once.
    pub(crate) restarting: HashSet<String>,
    /// What each agent's last `fork` came to (`Core::fork_now`, part 4): decided once a place, while its message waits
    /// to be handed. Not journalled: a coordd started meanwhile decides again, and the copy's session is not the one
    /// the place was given in, so the message goes as a `deliver`'s.
    pub(crate) forked: HashMap<String, delivery::Forked>,
    /// The agents a move to another view runs for (`Job::Move`, moves.rs), so it is started once.
    pub moving: HashSet<String>,
    /// A step's agent started (the machine's `Effect::Start`, `to_start_owing`, a move) and when, with the step it
    /// was started for (none: any): not started again for that step before `STARTED_WAIT` passes, running or not. A
    /// start ends the session running, so a mark cleared when the agent came up let the next look start it again
    /// while that one was coming up, and each start ended the one before (Tovrin-dev, 11:12Z 17 Nov: f1729-1 four
    /// times in 15 s, just after its move).
    pub started_owing: HashMap<String, (std::time::Instant, Option<(u64, u32)>)>,
    /// The agents that ran in a view when coordd last went down, to be started again in it (`hand_over_running`,
    /// `to_move`).
    pub resuming: Vec<String>,
    /// The id of the line's view as the line is now (`layer::line_now`), read by the follow loop off the lock while an
    /// agent stands in a line's view; none before the first read or when it cannot be read (then no agent is moved for
    /// it). An agent out of its turn standing in another `line@` is moved to it (`moves::Core::move_due`).
    pub line_now: Option<String>,
    /// Per agent, the delivery handed it without an id (`render::delivery`, SINGLE) and not yet found in its transcript:
    /// at most one (`Core::bare_pending` keeps a second from being rendered so).
    pub bare: HashMap<String, Bare>,
    /// How often an agent's held mail was handed over and not taken: its pause grows with them.
    retry_tries: HashMap<String, u32>,
    /// Per agent, the ticket deliveries of a hand-over its session did not answer `ok` to: the next hand-over gives
    /// those alone, as many of them as are still due, and what came since goes after it (consult-3, N-807.p112, 1).
    /// The session may have taken them all the same (the answer lost after the write): channel.py then drops the
    /// message whole, which it can only when it holds nothing it did not pass before (each part's key, `keys`).
    retry: HashMap<String, Vec<String>>,
    /// Deliveries to the runner whose command could not run at all (not there, not runnable, no runner named): left
    /// un-given and held back (docket-core3 and docket-core5, N-159 post 46), each tried again on coordd's clock a
    /// minute after it last failed, for as long as it is owed, and at once when the runner is seen up or a later runner
    /// command runs (paper-research N-101 p448: the execute bit put back sent nothing for 27 minutes). By delivery id.
    pub runner_unsent: BTreeMap<String, Unsent>,
    /// How long a held runner step waits before it is tried again: RUNNER_AGAIN (a test shortens it).
    pub runner_after: Duration,
    /// Where jobs for the agents' sessions go (`main::senders`).
    pub jobs: Option<tokio::sync::mpsc::UnboundedSender<Job>>,
    /// Wakes the transcript follower (`main::follow`) at once: a hook, new mail.
    pub poke: std::sync::Arc<tokio::sync::Notify>,
    /// The owner's accounts (accounts/mod.rs): `~/agent-coord/accounts`, shared by his coordds.
    pub accounts: crate::accounts::Store,
    /// The switch `All agents use my accounts` as last set (the registry's `native_all`; accounts/mover.rs); None:
    /// never set, so it turns on by itself at the first account that has signed in.
    pub native_all: Option<coord_types::AllAgents>,
    /// The sessions that run on an account, with their agent and account (accounts/mover.rs `Sessions`).
    pub account_sessions: crate::accounts::mover::Sessions,
    /// When the key accounts that are out are looked at next for the policy's `key-again-after` (accounts/mover.rs).
    pub key_again: crate::accounts::mover::KeyAgain,
    /// The sign-ins that run, by account id (accounts/signin.rs; `main::account_signin`).
    pub signins: HashMap<String, std::sync::Arc<crate::accounts::signin::SignIn>>,
    /// The opencode sign-ins running in this coordd, by account (`accounts::opencode::Run`): one at a time per
    /// account, as for Claude Code's.
    pub opencode_runs: HashMap<String, std::sync::Arc<crate::accounts::opencode::Run>>,
    /// The t3code sign-ins of agy accounts running in this coordd, by account (`accounts::agy::SignIn`): one at a
    /// time per account, as for Claude Code's.
    pub agy_signins: HashMap<String, std::sync::Arc<crate::accounts::agy::SignIn>>,
    /// The opencode accounts whose key (`Request::AccountKeyStore`) waits for its run to be in `opencode_runs`, from
    /// the take of the lock that found the account opencode's (`main::account_key_store`): an `AccountCancel` meanwhile
    /// takes the account out, and `main::opencode_key` ends the run as it starts, before anything is typed into it.
    pub opencode_keys: HashSet<String>,
    /// The OpenCode Go accounts whose usage is being read after a turn's end (`Job::GoUsage`), one read at a time, and
    /// whether a turn's end during that read asked for one more after it (`Core::opencode_said`, `Core::go_read_done`).
    pub go_reading: HashMap<String, bool>,
    /// The port of the telemetry receiver (accounts/telemetry.rs), once it runs: started with the first agent that
    /// runs on an account (`main::telemetry_up`).
    pub telemetry_port: Option<u16>,
    /// What `journal` last wrote (live.json, running.json): written again only when it changes.
    journalled: (Vec<u8>, Vec<u8>),
    /// The compiled norms and the situations solved with them, until the policy files, an agent or a role change.
    norm_cache: std::cell::RefCell<Option<NormCache>>,
    /// `coord duties`' last answer, with the facts and the policy files' stamp it was for (`Core::duties_ask`).
    duties_kept: std::cell::RefCell<Option<(Vec<(String, Vec<String>)>, Vec<(String, Option<std::time::SystemTime>)>, Vec<crate::policy::interp::Duty>)>>,
    /// The policy's times coordd acts on by the clock: `lapse-after` by call, `key-again-after` (lapse.rs).
    pub lapse_times: std::cell::RefCell<crate::lapse::Times>,
    /// The watch connections up for each window start (lapse.rs, `Request::Watch`'s `box`).
    pub boxes: crate::lapse::Boxes,
    /// The threads his says of cleaned words opened, with their words ticket, until the post of his words is there
    /// or the thread closes (`cleaned_pending_round`, N-564).
    pub cleaned_pending: Vec<(u64, u64)>,
    /// The same for his says of a point of a message split by point, with the point (`split_pending_round`, N-696).
    pub split_pending: Vec<(u64, docket_machine::SplitFrom)>,
    /// Counts the changes of agents, roles and the record's facts (ticket/effects.rs) the norms' situations are made
    /// of (`NormCache::generation`).
    pub(crate) norm_gen: u64,
    /// The sidebar's parts as the policy states them (`Core::parts`), read again when `norm_gen` moves or the policy
    /// files change.
    parts_cache: std::cell::RefCell<Option<PartsCache>>,
    /// Each agent's view's ticket and items (`Core::on_and_items`), kept with the machine's `changes` they were read at:
    /// both scan every ticket, and a view is made at each changed tool list and begun turn (docket-core8, N-412 p1607).
    view_kept: std::cell::RefCell<HashMap<String, (u64, Option<String>, Vec<String>)>>,
    /// The parts the last snapshot carried (`Core::parts_round`).
    parts_told: std::cell::RefCell<Option<crate::policy::parts::Parts>>,
    /// The clocks the last snapshot carried, as written (`last` left out: a firing is told by the ticket it opens).
    clocks_told: std::cell::RefCell<Option<Vec<docket_machine::ClockView>>>,
    /// The proxy's models and their families (`Core::set_models`), the norms' `model` and `family` facts.
    models: Vec<(String, String)>,
    /// The model the policy gives each task the watcher's steps run, as last solved off the lock (`main::read_models`):
    /// what the Kinds answer says of a call coordd runs (paper-research N-1396 p235).
    pub task_models: crate::policy::TaskModels,
    /// When the owner was last mailed about each project event fingerprint (`Core::project_event`).
    notified: HashMap<String, std::time::Instant>,
    /// The owner's hold (`Core::hold`): when it was last looked at, and the first mail it lets go.
    hold: std::cell::RefCell<(Option<std::time::Instant>, Option<usize>)>,
    /// The dialog titles the terminal holders reported (`Core::popup`), read from `popups.jsonl` when first needed.
    popups: Option<HashSet<String>>,
    /// The folders the sessions' transcripts moved from and to, read from `sessions-moved` at load (`moved_transcript`).
    sessions_moved: Vec<(String, String)>,
    /// The agents whose chat the owner is writing in (`Request::Writing`), each with the pid of the process that said so
    /// (the app's `coord bridge`), while that process lives: a fact of `free`, for the delivery program (p1127, 2).
    pub(crate) writing: BTreeMap<String, i32>,
    /// The tickets in whose box the owner is writing (`Request::Writing` with a thread), each with the pid of the
    /// process that said so, while that process lives (`threads_writing_ended`).
    writing_on: BTreeMap<u64, i32>,
    /// What coordd keeps of each agent's delivery between rounds (delivery.rs).
    desk_seen: HashMap<String, delivery::Seen>,
    /// In tests, the `hand_key` of the last `to_hand_over` that handed nothing and changed nothing it reads: a call on
    /// the same key that hands something is an input `hand_key` misses, which the live follow round would leave for a
    /// sweep (paper-research N-1446 p3381), and fails the test that made it.
    #[cfg(test)]
    hand_quiet: Option<u64>,
    /// In tests, how the watcher's `fold` is answered at once, in the round that raised it (delivery.rs `fold_inline`):
    /// by default as a failed call is, everything waiting; None leaves it to a `Job::Deliver`.
    #[cfg(test)]
    pub fold_by: Option<fn(&crate::clerk::Fold) -> Value>,
    /// The machine's changes, the policy's two numbers, and whether he was writing to no agent, when an update of the
    /// owner model was last looked for (brain.rs `update_owner_model`): it is looked for again only once one of them
    /// has moved.
    owner_model_seen: Option<(u64, u64, u64, bool)>,
    /// The machine's changes and the owner model as his window was last told it then (brain.rs `owner_model_round`).
    owner_model_told: Option<(u64, Option<coord_types::OwnerModelView>)>,
    /// Each agent's step (ticket, step) whose turn last ended on an API error (`settle_waits`): a second such end on
    /// the same step gives it up as refused (docket-core3, N-101 p1213: f37-1's `consider` was refused twice and stood).
    api_failed: HashMap<String, (u64, u32)>,
    /// Deliveries held for a return to an earlier release (`Request::Hold`, release.sh coord-back --esc): nothing is
    /// handed while it is on; not kept across a restart.
    pub holding: bool,
    /// The decisions among agents asked a second time because their answer named an agent that could no longer be
    /// woken (`among_again`, his `@all`): (ticket, step), so each is asked again once.
    pub among_asked_again: std::collections::HashSet<(u64, u32)>,
    /// The first prompt each agent was launched with (`main::launch`): its session's first user entry, which is no
    /// words typed into its terminal (`typed`).
    pub(crate) launched_with: HashMap<String, String>,
    /// Each agent's jobs on the build runner, as its view gives them (`Core::set_jobs`, N-1334).
    runner_jobs: HashMap<String, Vec<coord_types::Job>>,
    /// The runner's jobs whose caller went before their end, by job id: `cut` or `ended`, and when, as decided at the
    /// first read that found the caller gone (`Core::set_jobs`); kept while the job's file is there. Not journalled.
    runner_gone: HashMap<String, (String, String)>,
    /// The record logs (mail, tickets) held open while this Core is (`jsonl::essential`).
    record_held: Vec<crate::jsonl::HeldOpen>,
    /// Docket's tickets (`crate::ticket`).
    pub tickets: crate::ticket::Tickets,
}

/// The norms compiled from the policy files as STAMP had them, with agents and roles as of GENERATION, and the situations
/// solved with them, by the agents brought into the thread asked about (design coordd-core-2026-11-10 §5).
/// The policy's parts (`policy::parts`), with what they were read from, as `NormCache` keeps the norms.
struct PartsCache {
    stamp: Vec<(String, Option<std::time::SystemTime>)>,
    generation: u64,
    /// When the policy files were last compared with STAMP (at most every 2 s).
    checked: std::time::Instant,
    parts: crate::policy::parts::Parts,
}

struct NormCache {
    stamp: Vec<(String, Option<std::time::SystemTime>)>,
    generation: u64,
    /// When the policy files were last compared with STAMP (at most every 2 s).
    checked: std::time::Instant,
    norms: Result<crate::policy::interp::Acts, String>,
    solved: HashMap<Vec<String>, Result<crate::policy::interp::Shown, String>>,
}

/// How many net runs a snapshot carries.

/// How many days of the agents' requests coordd keeps for the usage page.
const SENT_DAYS: i64 = 32;

fn read_jsonl(path: &str) -> Vec<Value> {
    std::fs::read_to_string(path).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

/// The first ticket number above the item numbers IDS (today's items, moved onto tickets): what the tickets of a
/// record are numbered from (`load_tickets`).
pub fn first_ticket<'a>(ids: impl Iterator<Item = &'a str>) -> u64 {
    ids.filter_map(docket_machine::parse_ticket).max().map_or(1, |m| m + 1)
}

/// [`first_ticket`] of COORD's items log as `Core::load` reads it, for a program that reads the record without a
/// core (`coordd save-replay`, ticket/replay_state.rs).
pub fn first_ticket_of(coord: &str) -> u64 {
    let items: Vec<ItemRecord> = read_jsonl(&format!("{coord}/items/log.jsonl")).into_iter().filter_map(|r| serde_json::from_value(r).ok()).collect();
    first_ticket(items.iter().map(|r| r.id.as_str()))
}


/// Append V to the log at PATH, on the writer thread (`jsonl::start_writer`): no disk write under the core's lock.
pub(crate) fn append(path: &str, v: &Value) {
    crate::jsonl::later(path, v.to_string());
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Names no agent may take, made or renamed: those that answer as someone else. `owner` and `coord` are the owner and
/// coordd; `box` is the worker of a step owed to his window (`step box judge`, lapse.rs `BOX`), whose answers the
/// owner's connection gives, so an agent named so would answer them (docket-core3, N-487 p67).
pub(crate) fn reserved(name: &str) -> bool {
    matches!(name, "owner" | "coord") || name == crate::lapse::BOX
}

impl Core {
    pub fn load(coord: &str, base: &str) -> Core {
        let (tx, _) = broadcast::channel(4096);
        let mut core = Core {
            coord: coord.into(),
            base: base.into(),
            agents: BTreeMap::new(),
            mails: Vec::new(),
            ix: MailIndex::default(),
            delivered: HashSet::new(),
            withdrawn: HashSet::new(),
            dropped: HashSet::new(),
            items: Vec::new(),
            views: BTreeMap::new(),
            look: None,
            text_recovered: None,
            project: None,
            tickets: Default::default(),
            seq: 0,
            run: format!("{}-{}", now(), std::process::id()),
            ring: VecDeque::new(),
            latest: HashMap::new(),
            held_events: 0,
            ring_bytes: 0,
            ring_len: HashMap::new(),
            sent_posts: HashMap::new(),
            floor: 0,
            tx,
            sent: HashMap::new(),
            recognised: HashMap::new(),
            refusals: Vec::new(),
            notices: Vec::new(),
            no_model_said: HashSet::new(),
            read_asks: Default::default(),
            oom_seen: BTreeMap::new(),
            filed: HashMap::new(),
            unfiled: HashSet::new(),
            clerking: HashSet::new(),
            renaming: HashSet::new(),
            harness_files: Default::default(),
            clerked: HashMap::new(),
            inflight: HashMap::new(),
            retry_at: HashMap::new(),
            idle_given: HashMap::new(),
            restarting: HashSet::new(),
            forked: HashMap::new(),
            moving: HashSet::new(),
            started_owing: HashMap::new(),
            resuming: Vec::new(),
            line_now: None,
            bare: HashMap::new(),
            retry_tries: HashMap::new(),
            retry: HashMap::new(),
            runner_unsent: Default::default(),
            runner_after: RUNNER_AGAIN,
            jobs: None,
            poke: Default::default(),
            accounts: crate::accounts::Store::of_env(),
            native_all: None,
            account_sessions: Default::default(),
            // a key account may have been left out before this coordd started
            key_again: crate::accounts::mover::KeyAgain::look(),
            signins: Default::default(),
            opencode_runs: Default::default(),
            agy_signins: Default::default(),
            opencode_keys: Default::default(),
            go_reading: Default::default(),
            telemetry_port: None,
            journalled: Default::default(),
            norm_cache: Default::default(),
            duties_kept: Default::default(),
            lapse_times: Default::default(),
            boxes: Default::default(),
            cleaned_pending: Vec::new(),
            split_pending: Vec::new(),
            norm_gen: 0,
            parts_cache: Default::default(),
            view_kept: Default::default(),
            parts_told: Default::default(),
            clocks_told: Default::default(),
            hold: Default::default(),
            models: Vec::new(),
            task_models: Default::default(),
            notified: HashMap::new(),
            popups: None,
            sessions_moved: Vec::new(),
            writing: BTreeMap::new(),
            writing_on: BTreeMap::new(),
            desk_seen: HashMap::new(),
            #[cfg(test)]
            hand_quiet: None,
            #[cfg(test)]
            fold_by: Some(crate::clerk::fold_failed),
            owner_model_seen: None,
            owner_model_told: None,
            api_failed: HashMap::new(),
            holding: false,
            among_asked_again: Default::default(),
            launched_with: HashMap::new(),
            runner_jobs: HashMap::new(),
            runner_gone: HashMap::new(),
            record_held: Vec::new(),
        };
        // the record logs: a line lost from one would leave a hole the next start replays (jsonl::write_line); held
        // open, so a coordd out of descriptors still takes their lines
        core.record_held = vec![crate::jsonl::essential(&core.path("mail/log.jsonl")), crate::jsonl::essential(&core.path("tickets/log.jsonl"))];
        core.sessions_moved = read_sessions_moved(&core.path("sessions-moved"));
        core.load_registry();
        for r in read_jsonl(&core.path("mail/log.jsonl")) {
            match r.get("ev").and_then(Value::as_str) {
                Some("mail") => {
                    if let Ok(m) = serde_json::from_value::<Mail>(r) {
                        core.index_mail(m);
                    }
                }
                Some("delivered") => {
                    core.delivered.insert((s(&r, "id"), s(&r, "to")));
                }
                Some("withdrawn") => {
                    core.withdrawn.insert((s(&r, "id"), s(&r, "to")));
                }
                Some("soon") => {
                    if let Some(&i) = core.ix.id.get(&s(&r, "id")) {
                        core.mails[i].soon = true;
                    }
                }
                Some("drop") => {
                    core.dropped.insert(s(&r, "id"));
                }
                Some("recognised") => {
                    if let Ok(q) = serde_json::from_value::<Recognised>(r["recognised"].clone()) {
                        core.recognised.insert(s(&r, "id"), q);
                    }
                }
                Some("filed") => {
                    core.filed.insert(s(&r, "id"), s(&r, "run"));
                }
                Some("unfile") => {
                    core.unfiled.insert(s(&r, "id"));
                }
                Some("clerked") => {
                    core.clerked.insert(s(&r, "id"), s(&r, "what"));
                }
                Some("released") => {
                    core.clerked.remove(&s(&r, "id"));
                }
                Some("stands_for") => {
                    if let (Some(&new), Some(&orig)) = (core.ix.id.get(&s(&r, "id")), core.ix.id.get(&s(&r, "for"))) {
                        core.ix.stands_for.insert(new, orig);
                    }
                }
                _ => {}
            }
        }
        // each recipient's held mail: what no delivery or withdrawal took
        for (i, m) in core.mails.iter().enumerate() {
            for t in &m.to {
                let key = (m.id.clone(), t.clone());
                if !core.delivered.contains(&key) && !core.withdrawn.contains(&key) {
                    core.ix.held.entry(t.clone()).or_default().insert(i);
                }
            }
        }
        for r in read_jsonl(&core.path("items/log.jsonl")) {
            // today's items log, read only for its one import onto tickets (items.rs) and to keep its numbers
            if let Ok(rec) = serde_json::from_value::<ItemRecord>(r) {
                core.items.push(rec);
            }
        }
        core
    }

    pub fn path(&self, rel: &str) -> String {
        format!("{}/{rel}", self.coord)
    }

    /// The registry, or (first start) the agents the old layer knew: agents/NAME/{parent,session.json}.
    fn load_registry(&mut self) {
        let path = self.path("registry.jsonl");
        if !std::path::Path::new(&path).exists() {
            let mut names: Vec<String> = std::fs::read_dir(self.path("agents"))
                .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect())
                .unwrap_or_default();
            names.sort();
            for name in names {
                let dir = self.path(&format!("agents/{name}"));
                let Ok(session) = std::fs::read_to_string(format!("{dir}/session.json")) else { continue };
                let session: Value = serde_json::from_str(&session).unwrap_or(Value::Null);
                let parent = std::fs::read_to_string(format!("{dir}/parent")).map(|p| p.trim().to_string()).unwrap_or_default();
                let id = format!("AGT-{:03}", self.agents.len() + 1);
                append(&path, &json!({"ev": "add", "id": id, "name": name, "parent": if parent.is_empty() { "owner" } else { &parent },
                    "kind": s(&session, "kind"), "model": s(&session, "model"), "at": now(), "why": "carried over from the old layer"}));
                append(&path, &json!({"ev": "session", "name": name, "session": session, "at": now()}));
                let mut a = self.new_agent(id, name.clone(), parent, s(&session, "kind"), s(&session, "model"), now());
                a.sessions.extend(session.get("transcript_path").and_then(Value::as_str).map(|t| moved_transcript(t, &self.sessions_moved).unwrap_or_else(|| t.to_string())));
                self.agents.insert(name, a);
            }
            return;
        }
        for r in read_jsonl(&path) {
            let name = s(&r, "name");
            match r.get("ev").and_then(Value::as_str) {
                Some("add") => {
                    let mut a = self.new_agent(s(&r, "id"), name.clone(), s(&r, "parent"), s(&r, "kind"), s(&r, "model"), s(&r, "at"));
                    a.from = r.get("from").and_then(Value::as_str).filter(|f| !f.is_empty()).map(String::from);
                    if let Some(o) = self.agents.remove(&name) {
                        a.id = o.id;
                        a.parent = o.parent;
                        // a re-add keeps the role the owner set (N-74: Tovrin's lead, senior and consultant had lost theirs)
                        a.role = o.role;
                        a.session = o.session;
                        a.transcript_file = o.transcript_file;
                        a.created = o.created;
                        a.sessions = o.sessions;
                        a.team = o.team;
                        a.member_of = o.member_of;
                        // what it was copied from stays what its first `add` said
                        a.from = o.from.or(a.from);
                        // a re-add does not undo the owner's end of it
                        a.ended = o.ended;
                        // nor where it runs: on the owner's accounts, on which, or through the gateway meanwhile
                        (a.native, a.account, a.gateway) = (o.native, o.account, o.gateway);
                        a.renderings = o.renderings;
                        // nor the mode the owner set it to
                        (a.yolo, a.own_mode, a.mode_from) = (o.yolo, o.own_mode, o.mode_from);
                    } else if let Some(lead) = r.get("member_of").and_then(Value::as_str) {
                        // a new member counts against its lead's grant at the time
                        if let Some(g) = self.agents.get_mut(lead).and_then(|l| l.team.as_mut()) {
                            g.used += 1;
                        }
                        a.member_of = Some(lead.into());
                    }
                    if let Some(set) = r.get("renderings").and_then(Value::as_str) {
                        a.renderings = Some(set.into());
                    }
                    self.agents.insert(name, a);
                }
                Some("renderings") => {
                    if let Some(a) = self.agents.get_mut(&name) {
                        a.renderings = r.get("set").and_then(Value::as_str).map(String::from);
                    }
                }
                Some("team") => {
                    if let Some(a) = self.agents.get_mut(&name) {
                        a.team = Some(TeamGrant { granted: r["granted"].as_u64().unwrap_or(0) as u32, used: 0, by: s(&r, "by") });
                    }
                }
                Some("rename") => self.renamed(&name, &s(&r, "to")),
                Some("role") => {
                    if let Some(a) = self.agents.get_mut(&name) {
                        a.role = s(&r, "role");
                    }
                }
                Some("control") => {
                    if let (Some(a), Ok(mode)) = (self.agents.get_mut(&name), serde_json::from_value::<AgentMode>(r["mode"].clone())) {
                        a.control = mode;
                    }
                }
                Some("model") => {
                    if let Some(a) = self.agents.get_mut(&name) {
                        a.model = s(&r, "model");
                    }
                }
                Some("native") => {
                    if let Some(a) = self.agents.get_mut(&name) {
                        a.native = r.get("on").and_then(Value::as_bool) == Some(true);
                        a.native_why = r.get("why").and_then(Value::as_str).filter(|_| a.native).map(String::from);
                    }
                }
                Some("account") => {
                    if let Some(a) = self.agents.get_mut(&name) {
                        a.account = r.get("account").and_then(Value::as_str).map(String::from);
                        // through the gateway while the account it left is out: looked at again at the first round
                        a.gateway = r.get("left").map(|left| crate::accounts::mover::OnGateway { left: left.as_str().map(String::from), since: s(&r, "at"), back: 0 });
                        a.account_move = Some(crate::accounts::mover::AccountMove { why: s(&r, "why"), since: s(&r, "at"), from: r.get("from").and_then(Value::as_str).map(String::from) });
                        a.native_why = None;
                    }
                }
                Some("native_all") => {
                    self.native_all = Some(coord_types::AllAgents {
                        on: r.get("on").and_then(Value::as_bool) == Some(true),
                        at: r.get("secs").and_then(Value::as_i64).unwrap_or(0),
                        by_account: r.get("by_account").and_then(Value::as_str).map(String::from),
                    });
                }
                Some("compact") => {
                    if let Some(a) = self.agents.get_mut(&name) {
                        a.compact = r.get("tokens").and_then(Value::as_u64).filter(|t| *t > 0);
                        a.compact_at = s(&r, "at");
                    }
                }
                Some("mode") => {
                    if let Some(a) = self.agents.get_mut(&name) {
                        match r.get("mode").and_then(Value::as_str) {
                            // its own cleared: the default, as `resolve_modes` takes it
                            Some("") => (a.own_mode, a.mode_from) = (None, ""),
                            m => {
                                let own = m == Some("yolo");
                                (a.yolo, a.own_mode, a.mode_from) = (own, Some(own), "agent");
                            }
                        }
                    }
                }
                Some("ended") => {
                    if let Some(a) = self.agents.get_mut(&name) {
                        a.ended = if r.get("undo").and_then(Value::as_bool) == Some(true) { None } else { Some(s(&r, "at")) };
                    }
                }
                Some("session") => {
                    if let Some(a) = self.agents.get_mut(&name) {
                        a.session = r.get("session").cloned().unwrap_or(Value::Null);
                        a.note_session(&self.sessions_moved);
                    }
                }
                _ => {}
            }
        }
    }

    /// What `coord find` reads beyond the tickets (paper-research N-101 p1026): the mail, and each agent's sessions.
    pub(crate) fn find_sources(&self) -> (Vec<Mail>, Vec<(String, Vec<String>)>) {
        (self.mails.clone(), self.sessions())
    }

    fn new_agent(&self, id: String, name: String, parent: String, kind: String, model: String, created: String) -> Agent {
        Agent {
            id,
            name,
            parent: if parent.is_empty() { "owner".into() } else { parent },
            role: String::new(),
            kind: if kind.is_empty() { "claude".into() } else { kind },
            model,
            session: Value::Null,
            live: Live { state: "stopped".into(), since: now(), ..Default::default() },
            background: None,
            created,
            sessions: Vec::new(),
            usage: Vec::new(),
            context: None,
            last_active: None,
            failed: None,
            no_box: None,
            native: false,
            renderings: None,
            account: None,
            account_wait: None,
            account_none: false,
            account_resumed: false,
            down_cut: false,
            gateway: None,
            account_move: None,
            native_why: None,
            ended_turn: u64::MAX, // no turn end taken yet (a turn number never reaches it)
            output_owed: None,
            stop_owed: false,
            begin_seen: false,
            mem_mb: None,
            cpu_s: None,
            footprint: (None, None),
            transcript_file: None,
            team: None,
            member_of: None,
            from: None,
            bg_waits: Vec::new(),
            ended: None,
            compact: None,
            compact_at: String::new(),
            context_max: None,
            yolo: false,
            own_mode: None,
            mode_from: "",
            control: AgentMode::Run,
            apply_error: None,
        }
    }

    // ---- events ------------------------------------------------------------------------------------------------

    pub fn emit(&mut self, body: Body) {
        self.seq += 1;
        let ev = Event { seq: self.seq, body };
        // what the ring keeps of it, when not itself: a ticket's change merged with the change it replaces
        let mut kept = None;
        if matches!(ev.body, Body::Snapshot(_)) {
            // a snapshot carries everything: the ring before it is no longer news
            self.ring.clear();
            self.latest.clear();
            self.held_events = 0;
            self.ring_len.clear();
            self.ring_bytes = 0;
        } else if let Some(key) = event_key(&ev.body) {
            // a ticket whole takes the place of its change too
            if let Body::Ticket(v) = &ev.body
                && let Some(old) = self.latest.remove(&format!("c {}", v.id))
            {
                self.unring(old);
            }
            if let Some(old) = self.latest.insert(key, ev.seq)
                && let Some(gone) = self.unring(old)
            {
                kept = kept_change(gone.body, &ev);
            }
        }
        let kept = kept.unwrap_or_else(|| ev.clone());
        let sizing = std::time::Instant::now();
        let len = json_len(&kept);
        let sizing = sizing.elapsed();
        self.ring_len.insert(ev.seq, len);
        self.ring_bytes += len;
        self.ring.push_back((ev.seq, Some(kept)));
        self.held_events += 1;
        while self.held_events > RING {
            if let Some((s, Some(old))) = self.ring.pop_front() {
                self.held_events -= 1;
                self.ring_bytes -= self.ring_len.remove(&s).unwrap_or(0);
                self.floor = s;
                if let Some(k) = event_key(&old.body).filter(|k| self.latest.get(k) == Some(&s)) {
                    self.latest.remove(&k);
                }
            }
        }
        while matches!(self.ring.front(), Some((_, None))) {
            self.ring.pop_front();
        }
        crate::counts::ring_now(self.held_events, self.ring_bytes, sizing);
        let _ = self.tx.send(ev);
    }

    /// The ring's place of event SEQ emptied, if it holds it: the event, out of the ring's count and bytes.
    fn unring(&mut self, seq: u64) -> Option<Event> {
        let at = self.ring.partition_point(|(s, _)| *s < seq);
        let (s, slot) = self.ring.get_mut(at)?;
        if *s != seq {
            return None;
        }
        let gone = slot.take()?;
        self.held_events -= 1;
        self.ring_bytes -= self.ring_len.remove(&seq).unwrap_or(0);
        Some(gone)
    }

    /// What a watch from FROM in RUN starts with (protocol 1): `Resumed`, then the events after FROM, when FROM was
    /// numbered in this run and the ring still holds all of them; else none, and the watch starts with a snapshot. No
    /// RUN (a watcher from before protocol 1): the events after FROM, as before.
    pub fn resume(&self, from: u64, run: Option<&str>) -> Option<Vec<Event>> {
        match run {
            None => self.since(from),
            Some(r) if r == self.run => {
                let after = self.since(from)?;
                Some(std::iter::once(Event { seq: from, body: Body::Resumed { from } }).chain(after).collect())
            }
            Some(_) => None,
        }
    }

    /// Events after FROM, when the ring still holds all of them.
    pub fn since(&self, from: u64) -> Option<Vec<Event>> {
        (from >= self.floor && from <= self.seq).then(|| {
            let at = self.ring.partition_point(|(s, _)| *s <= from);
            self.ring.range(at..).filter_map(|(_, e)| e.clone()).collect()
        })
    }

    pub fn snapshot(&self) -> Event {
        self.snapshot_with(self.ticket_views())
    }

    /// [`Core::snapshot`] with its tickets' views TICKETS built already (a watch builds them a few at a time, the lock
    /// let go between: `chunked_snapshot`).
    pub fn snapshot_with(&self, tickets: Vec<docket_machine::TicketView>) -> Event {
        let parts = self.parts();
        let snap = Snapshot {
            project: self.project.as_ref().map(|p| p.raw.clone()),
            agents: self.agents.keys().map(|n| self.view(n)).collect(),
            // the latest mail, and every question's thread however old (the owner's inbox lists the open ones)
            mail: self
                .mails
                .iter()
                .enumerate()
                .filter(|(i, m)| *i + 500 >= self.mails.len() || self.thread_root(&m.id).is_some())
                .map(|(_, m)| self.mail_view(m))
                .collect(),
            views: self.views.values().cloned().collect(),
            refusals: self.refusals.clone(),
            tickets,
            notices: self.notices(),
            slowed: self.tickets.m.slowed.iter().cloned().collect(),
            run: Some(self.run.clone()),
            parts: parts.places.clone(),
            places: Some({
                let start = self.start_dir(&json!({}));
                coord_types::Places { own_homes: start.is_none(), start, ..crate::server_reads::places(&self.coord, self.project.as_ref()) }
            }),
            clocks: self.tickets.m.clock_views(),
            mail_hold: self.tickets.m.mail_hold.as_ref().map(|h| h.at.clone()),
            owner_model: self.owner_model_view(),
            // the flavour's look as the text-change act loaded it (change.rs `read_look`, N-865, N-866)
            look: self.look.clone(),
        };
        *self.parts_told.borrow_mut() = Some(parts);
        *self.clocks_told.borrow_mut() = Some(self.clocks_as_written());
        Event { seq: self.seq, body: Body::Snapshot(snap) }
    }

    /// The machine's clocks without when each last fired: what `parts_round` compares.
    fn clocks_as_written(&self) -> Vec<docket_machine::ClockView> {
        self.tickets.m.clock_views().into_iter().map(|c| docket_machine::ClockView { last: None, ..c }).collect()
    }

    /// Agent NAME's brain as its card shows it (`AgentView.brain`; proto-lead N-1446 p857.2, proto-window N-1475): his
    /// switch, from the record's last `brain_switch` line for NAME (on, with no time, before his first press), and its
    /// last pass, opened when and its id. None when no binding opens a brain for NAME (`Machine::brain_bound`), so its
    /// card shows no switch.
    fn brain_view(&self, name: &str) -> Option<coord_types::BrainView> {
        let m = &self.tickets.m;
        if !m.brain_bound(name) {
            return None;
        }
        let last = m.last_brain_pass(name);
        Some(coord_types::BrainView {
            on: m.brain_is_on(name),
            since: m.brain_switched.get(name).map(|(_, at)| at.clone()),
            last_pass: last.map(|x| x.opened_at.clone()),
            last_id: last.map(|x| m.id(x.t)),
            ready: self.brain_ready(name),
        })
    }

    /// Agent NAME's brain's suggestions: each open pass's post halted for his approval (slow mode), oldest first, with
    /// its first line as the record holds it.
    fn brain_ready(&self, name: &str) -> Vec<coord_types::BrainSuggestion> {
        use docket_machine::machine::{Halting, BRAIN};
        let m = &self.tickets.m;
        let passes = m.tickets.values().filter(|x| x.kind == BRAIN && x.is_open() && x.form.get("agent").and_then(Value::as_str) == Some(name));
        let mut out: Vec<(u64, coord_types::BrainSuggestion)> = passes
            .flat_map(|x| x.halts.iter().filter_map(move |h| match &h.what {
                Halting::Deliver { post, .. } => {
                    let words = x.posts.iter().find(|p| p.n == *post).and_then(|p| crate::label::read(&p.text)).unwrap_or_default();
                    let first = words.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default().to_string();
                    Some((h.seq, coord_types::BrainSuggestion { ticket: x.t, id: m.id(x.t), halt: h.n, post: *post, words: first }))
                }
                _ => None,
            }))
            .collect();
        out.sort_by_key(|(seq, _)| *seq);
        out.into_iter().map(|(_, s)| s).collect()
    }

    pub fn view(&self, name: &str) -> AgentView {
        let a = &self.agents[name];
        let (on, items) = self.on_and_items(name);
        let part = self.parts().of.get(name).cloned().unwrap_or_default();
        let (mem_mb, cpu_s) = if a.running() { (a.mem_mb, a.cpu_s) } else { (None, None) };
        // held on a rate limit, out of a turn: its row says until when (proto-lead N-1397 p160)
        let held = a.live.limit.as_ref().filter(|_| a.running() && !self.in_turn(a));
        let (deaf, deaf_do) = a.deaf_words().unzip();
        AgentView {
            id: a.id.clone(),
            name: a.name.clone(),
            parent: a.parent.clone(),
            role: a.role.clone(),
            part,
            // the ticket it is on, as its label ranks them, so the window draws that ticket's label and id (N-447 p48)
            on,
            kind: a.kind.clone(),
            model: a.model.clone(),
            state: a.live.state.clone(),
            compacting: a.live.compacting.then(|| a.live.compacting_since.clone().unwrap_or_else(|| a.live.since.clone())),
            since: a.live.since.clone(),
            last_tool: a.live.last_tool.clone(),
            tools: a.live.tools.values().cloned().collect(),
            items,
            transcript: a.transcript(),
            cwd: a.session.get("cwd").and_then(Value::as_str).map(String::from).or_else(|| a.live.pid.filter(|_| a.running() && !self.boxed()).and_then(started_in)),
            mem_mb,
            cpu_s,
            transcript_bytes: a.footprint.0,
            folder_bytes: a.footprint.1,
            created: a.created.clone(),
            sessions: a.sessions.len(),
            last_active: a.last_active.clone(),
            usage: a.usage.clone(),
            team: a.team.clone(),
            member_of: a.member_of.clone(),
            no_box: a.no_box.clone(),
            stands: crate::moves::stand(&self.coord, &a.name),
            moved: self.tickets.m.moved.get(&a.name).map(|m| coord_types::AgentMoved { at: m.at.clone(), from: m.from.clone(), to: m.to.clone(), why: m.why.clone(), left: m.left.clone() }),
            context: self.context_of(a),
            // an opencode or agy agent runs on the account of its harness it is bound to (its own sign-in or key, never
            // the gateway)
            account: if let Some(h) = crate::accounts::Harness::named(&a.kind).filter(|h| *h != crate::accounts::Harness::Claude) {
                self.accounts.bound_to(h, &self.coord_id(), &a.name).map(|e| e.id)
            } else {
                a.account.clone().filter(|_| a.native)
            },
            native: a.native,
            account_note: self.account_note(a),
            // what runs (a stopped one: its next start), and what he asked while that differs (hs-ls, N-1400 p6)
            mode: a.yolo_runs().then(|| "yolo".to_string()),
            mode_due: (a.yolo_runs() != a.yolo).then(|| (if a.yolo { "yolo" } else { "safe" }).to_string()),
            mode_from: (!a.mode_from.is_empty()).then(|| a.mode_from.to_string()),
            restarting: self.restarting.contains(&a.name),
            safety_check_runs: a.live.check_runs.filter(|_| a.kind == "claude" && a.running()),
            account_why: a.account_move.as_ref().map(|m| m.why.clone()),
            account_since: a.account_move.as_ref().map(|m| m.since.clone()),
            account_from: a.account_move.as_ref().and_then(|m| m.from.clone()),
            account_until: self.account_until(a),
            account_waiting: !a.running() && a.native && a.account_wait.is_some(),
            limit_until: held.map(|l| l.until),
            // not a rate limit: what stopped it before the words, which the window says (`Limit::what`)
            limit_why: held.map(|l| match &l.what { Some(w) => format!("{w}: {}", l.why), None => l.why.clone() }),
            acted: self.tickets.m.last_acts(name),
            jobs: self.runner_jobs.get(name).cloned().unwrap_or_default(),
            background: a.background.as_ref().map(|b| b.running_calls()).unwrap_or_default(),
            brain: self.brain_view(name),
            deaf,
            deaf_do: deaf_do.unwrap_or_default(),
            // a dialog its holder told of, standing until its screen or session moves on (`Live::dialog`)
            dialog: a.live.dialog.as_ref().filter(|_| a.running()).map(|(_, title, _)| title.clone()),
            control: a.control,
            capabilities: capabilities(&a.kind, self.boxed()),
            switches: self.switches_view(name, &a.kind),
            apply_error: a.apply_error.clone(),
            // a copy the window's Spin off made (registry), else one a pattern's or an agent's `coord fork` made (the
            // record's `made` line)
            from: a.from.clone().or_else(|| self.tickets.m.made().into_iter().find(|(m, ..)| m == name).and_then(|(_, _, f, _)| f)),
            read_asks: self.read_asks.of(name),
        }
    }

    /// Agent NAME's permission request INPUT as read_ask.rs judges it, against the folders it reads (`[read.NAME]`,
    /// `box_read`); a read that waits shows on its card at once.
    pub fn read_judge(&mut self, name: &str, input: &Value) -> crate::read_ask::Judged {
        if !self.agents.contains_key(name) {
            return crate::read_ask::Judged::Pass;
        }
        let folders = self.project.as_ref().map(|p| p.agent_read(name)).unwrap_or_default();
        let home = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default());
        let judged = self.read_asks.judge(name, input, &home, &folders, &now());
        if matches!(judged, crate::read_ask::Judged::Wait(..)) {
            self.changed(name);
        }
        judged
    }

    fn mail_view(&self, m: &Mail) -> MailView {
        MailView {
            id: m.id.clone(),
            at: m.at.clone(),
            from: m.from.clone(),
            to: m.to.clone(),
            subject: subject(&m.body),
            body: m.body.clone(),
            esc: m.esc,
            soon: m.soon,
            delivered: m.to.iter().filter(|t| self.delivered.contains(&(m.id.clone(), (*t).clone()))).cloned().collect(),
            withdrawn: m.to.iter().filter(|t| self.withdrawn.contains(&(m.id.clone(), (*t).clone()))).cloned().collect(),
            kind: m.kind.clone(),
            re: m.re.clone(),
            quotes: m.quotes.clone(),
            state: (self.is_question(m) && m.re.is_none()).then(|| self.question_state(&m.id).into()),
            recognised: self.recognised.get(&m.id).cloned(),
            filed: self.filed.get(&m.id).cloned(),
            unfiled: self.unfiled.contains(&m.id),
            clerk: if self.clerking.contains(&m.id) { Some("with the clerk".into()) } else { self.clerked.get(&m.id).cloned() },
        }
    }

    /// Add mail M to the mails and their indexes: its id, its thread (the root of the mail it answers; a mail that
    /// answers none, or one not known, is its own root), its sender's mails, the numbering. Not its recipients' held
    /// mail: [`Core::new_mail`] adds that, the log's load after its deliveries are read.
    fn index_mail(&mut self, m: Mail) -> usize {
        let i = self.mails.len();
        let root = m.re.as_deref().and_then(|r| self.ix.id.get(r)).map_or(i, |&p| self.ix.root[p]);
        self.ix.id.entry(m.id.clone()).or_insert(i);
        self.ix.root.push(root);
        self.ix.thread.entry(root).or_default().push(i);
        self.ix.by_from.entry(m.from.clone()).or_default().push(i);
        if m.kind == "question" { self.ix.questions += 1 } else { self.ix.messages += 1 }
        self.mails.push(m);
        i
    }

    pub(crate) fn find_mail(&self, id: &str) -> Option<&Mail> {
        self.ix.id.get(id).map(|&i| &self.mails[i])
    }

    /// The first mail of the thread ID is in (a question or a plain mail).
    pub(crate) fn root_of(&self, id: &str) -> Option<String> {
        self.ix.id.get(id).map(|&i| self.mails[self.ix.root[i]].id.clone())
    }

    /// The mails of the thread whose first mail is ROOT, oldest first (none when ROOT is not a thread's first mail).
    fn thread(&self, root: &str) -> Vec<&Mail> {
        match self.ix.id.get(root) {
            Some(&i) if self.ix.root[i] == i => self.ix.thread.get(&i).map_or(Vec::new(), |t| t.iter().map(|&j| &self.mails[j]).collect()),
            _ => Vec::new(),
        }
    }

    /// Mail ID no longer waits for NAME (delivered or taken back).
    fn unhold(&mut self, id: &str, name: &str) {
        if let (Some(&i), Some(h)) = (self.ix.id.get(id), self.ix.held.get_mut(name)) {
            h.remove(&i);
        }
    }

    /// Who is in the thread ROOT (owner 8 Nov): whoever wrote in it or was sent a mail in it, in order of joining.
    pub(crate) fn participants(&self, root: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for m in self.thread(root) {
            for n in std::iter::once(&m.from).chain(&m.to) {
                if !out.contains(n) {
                    out.push(n.clone());
                }
            }
        }
        out
    }

    /// The first question of the thread ID is in, when it is in one.
    fn thread_root(&self, id: &str) -> Option<String> {
        let m = &self.mails[self.ix.root[*self.ix.id.get(id)?]];
        self.is_question(m).then(|| m.id.clone())
    }

    /// M asks the owner: a question (`coord ask`), or a mail the recogniser found a question in.
    /// A filed question the owner took back is none.
    fn is_question(&self, m: &Mail) -> bool {
        (m.kind == "question" || self.recognised.contains_key(&m.id)) && !self.unfiled.contains(&m.id)
    }

    /// A thread's state: dropped, else open while one of its questions has no word of the owner's after it (an
    /// agent's plain reply in the thread does not reopen it), else answered.
    fn question_state(&self, root: &str) -> &'static str {
        if self.dropped.contains(root) {
            return "dropped";
        }
        let mut waiting = false;
        for m in self.thread(root) {
            if self.is_question(m) {
                waiting = true;
            } else if m.from == "owner" {
                waiting = false;
            }
        }
        if waiting { "open" } else { "answered" }
    }

    /// What an agent is given for M: a mail in a thread names what it answers and that mail's subject, so the agent
    /// has the context after its own context was compacted, and knows the id to reply in the thread with.
    ///
    /// An agent brought into a thread it was never in (no earlier mail of the thread from or to it) gets the thread so
    /// far first, each mail clipped, so it has the conversation it joins (owner 8 Nov).
    fn delivery_text(&self, m: &Mail, to: &str) -> String {
        let mut text = self.thread_text(m);
        if m.kind == "question" && to != "owner" && to != m.from {
            let root = m.re.as_deref().and_then(|r| self.root_of(r)).unwrap_or_else(|| m.id.clone());
            text = format!(
                "({} asked the owner this as {}; you get it as its lead. If you can answer or settle it, reply in its thread with coord msg --thread {} TEXT: {} gets your answer and the owner sees it there, so the owner need not. If it needs the owner, leave it.)\n{text}",
                m.from, m.id, root, m.from
            );
        }
        let Some(root) = m.re.as_deref().and_then(|r| self.root_of(r)) else { return text };
        let before: Vec<&Mail> = self.thread(&root).into_iter().take_while(|x| x.id != m.id).collect();
        if before.iter().any(|x| x.from == to || x.to.iter().any(|t| t == to)) {
            return text;
        }
        let so_far: Vec<String> = before.iter().map(|x| format!("[{} · from {} to {}]\n{}", x.id, x.from, x.to.join(", "), crate::transcript::clip(&x.body, 2000))).collect();
        format!(
            "(you are brought into the thread of {root}: {}; it so far, oldest first; reply to everyone in it with coord msg --thread {})\n{}\n(the mail for you:)\n{text}",
            subject(&self.find_mail(&root).map_or(String::new(), |r| r.body.clone())),
            m.id,
            so_far.join("\n\n")
        )
    }

    fn thread_text(&self, m: &Mail) -> String {
        match m.re.as_deref().and_then(|r| Some((r, self.find_mail(r)?))) {
            Some((r, q)) => {
                let what = if m.kind == "question" { "follows up" } else if q.kind == "question" { "answers" } else { "re" };
                format!("({what} {r}: {}; reply to everyone in this thread with coord msg --thread {})\n{}", subject(&q.body), m.id, m.body)
            }
            None => m.body.clone(),
        }
    }

    fn batch_text(&self, to: &str, ms: &[Mail]) -> String {
        let places = self.places_for(to);
        let head = |m: &Mail| crate::render::head(&places, Some(&m.id), &m.from, None).map_err(|e| eprintln!("coordd: {to}: rendering {}'s head: {e}", m.id)).unwrap_or_default();
        let one = |m: &Mail| match head(m) {
            h if h.is_empty() => self.delivery_text(m, to),
            h => format!("{h}\n{}", self.delivery_text(m, to)),
        };
        ms.iter().map(one).collect::<Vec<_>>().join("\n\n")
    }

    pub fn changed(&mut self, name: &str) {
        let v = self.view(name);
        self.emit(Body::Agent(v));
        self.journal();
    }

    fn set_state(&mut self, name: &str, state: &str) {
        let a = self.agents.get_mut(name).unwrap();
        if a.live.state != state {
            a.live.state = state.into();
            a.live.since = now();
        }
    }

    /// A new turn of NAME began (its transcript shows one running; Codex: a prompt or mail pushed to it).
    pub fn begin_turn(&mut self, name: &str) {
        let a = self.agents.get_mut(name).unwrap();
        a.live.turn += 1;
        a.begin_seen = false;
        a.failed = None;
        a.live.limit = None;
        if std::mem::take(&mut a.account_resumed) {
            eprintln!("coordd: {name}: a turn began after its restart for the turn its account's limit cut");
        }
        self.set_state(name, "working");
        // the session driver's line (N-1104 p1154): the delivery the turn began with, where a copy of it is cut while it
        // runs, and what the policy's `working` is made by (agent-base policy/working.eflint). None when no giving has
        // its place read yet: then the giving's `delivered` line brings the line again, naming it (`ticket_delivered`)
        let began = self.turn_began(name);
        let value = serde_json::json!({"agent": name, "delivery": (!began.is_empty()).then_some(began)});
        if let Err(e) = self.ticket_input(docket_machine::Input::Event { name: docket_machine::TURN_BEGAN.into(), value }) {
            eprintln!("coordd: {name}: its turn began, and the record refused the line: {e}");
        }
        // in a layer: among those who had a turn there since the last look (found.rs)
        crate::found::turn_began(&self.coord, name);
    }

    /// Journal the running agents' live state (process, state, turn, start; not their tool calls) to /run, and who
    /// runs to the project's disk, each only when it changed (design §6), on the writer thread.
    fn journal(&mut self) {
        let live = self.live_journalled();
        let bytes = serde_json::to_vec(&live).unwrap_or_default();
        // who runs, on the project's disk: the journal above is in /run, which a reboot empties
        let names = serde_json::to_vec(&live.keys().collect::<Vec<_>>()).unwrap_or_default();
        if bytes != self.journalled.0 {
            crate::jsonl::replace_later(&live_path(), bytes.clone());
            self.journalled.0 = bytes;
        }
        if names != self.journalled.1 {
            crate::jsonl::replace_later(&self.running_path(), names.clone());
            self.journalled.1 = names;
        }
    }

    /// What `journal` writes: each running agent's live state, by name (`recover` reads it back).
    fn live_journalled(&self) -> BTreeMap<&String, &Live> {
        self.agents.iter().filter(|(_, a)| a.running()).map(|(n, a)| (n, &a.live)).collect()
    }

    /// `$COORD/running.json`: the agents running, as of coordd's last change (`journal`).
    fn running_path(&self) -> String {
        format!("{}/running.json", self.coord)
    }

    /// At start, after `recover`: the agents that ran when coordd last went down and run no more (a reboot, owner
    /// m172, 9 Nov) go to `$COORD/resume.json`, for `roster resume` (box.sh start, host-coordd start) to resume each
    /// from its session as its own user (coordd runs as root in an sbx box). Not called again: an agent that ends
    /// later while coordd runs (its /exit, a roster down) was stopped and stays stopped.
    pub fn hand_over_running(&mut self, recovered: &[(String, i32)]) {
        let was: Vec<String> = std::fs::read(self.running_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let mut gone: Vec<&String> = was.iter().filter(|n| self.agents.contains_key(*n) && !recovered.iter().any(|(r, _)| r == *n)).collect();
        // an agent that ran in a view (`agents/NAME/layer`: a layer's or the line's) is not resumed by roster: it would
        // run outside its view, in the project's own folder, and outside its box (seen on 15 Nov). coordd starts it
        // again in it, or in the line's view if that has gone (moves.rs `to_move`, N-101 p406 C). A step's
        // short-lived agent is too while it still owes or holds its step: nothing else answers it, and nothing fails it
        // (docket-core3, N-101: f17-2, made for P-17.s3 and down with the machine, held it 14 minutes unseen); one
        // whose step is done is not
        let mut viewed = Vec::new();
        let owing = |n: &str| !self.tickets.m.owes(n).is_empty() || !self.tickets.m.holds(n).is_empty();
        gone.retain(|n| match crate::moves::stand(&self.coord, n) {
            Some(l) => {
                if !is_fresh(n) || owing(n) {
                    eprintln!("coordd: {n} is started again by coordd: it ran in {}", crate::moves::shown(Some(&l)));
                    viewed.push(n.to_string());
                }
                false
            }
            None => true,
        });
        // a Claude Code agent's turn that the end of its process cut goes on at its next start (`Agent::down_cut`)
        for n in gone.iter().map(|n| n.as_str()).chain(viewed.iter().map(String::as_str)) {
            if let Some(a) = self.agents.get_mut(n).filter(|a| a.kind == "claude") {
                a.down_cut = true;
            }
        }
        self.resuming = viewed;
        // the rest, a program (`roster tab`: the runner, owner 10:13 17 Nov, N-186.p39) as an agent: roster starts each
        // again as its own user, a program from agents/NAME/tab.json (coordd runs as root in an sbx box)
        for n in &gone {
            let what = if self.agents.get(*n).is_some_and(|a| a.kind == "program") { "its program tab" } else { "its session" };
            eprintln!("coordd: {n} ran when coordd last went down and runs no more: handed to roster resume, which starts {what} again");
        }
        if !gone.is_empty() {
            let _ = std::fs::write(format!("{}/resume.json", self.coord), serde_json::to_vec(&gone).unwrap_or_default());
        }
        self.journal();
    }

    /// After a coordd restart: the live state of agents whose cgroups still hold processes.
    pub fn recover(&mut self) -> Vec<(String, i32)> {
        let saved: HashMap<String, Live> =
            std::fs::read(live_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let mut watch = Vec::new();
        for (name, procs) in cgroup::live_agents() {
            let Some(a) = self.agents.get_mut(&name) else { continue };
            let live = saved.get(&name).cloned().unwrap_or_default();
            let pid = live.pid.filter(|p| procs.contains(p)).unwrap_or(procs[0]);
            a.live = Live { pid: Some(pid), state: if live.state.is_empty() { "idle".into() } else { live.state }, ..live };
            // the state goes on from the journal; the transcript's reader (`main::follow`) corrects it from the record
            if let Some(t) = a.transcript() {
                a.background = Some(Background::new(&t, &a.live.started).found_running());
            }
            watch.push((name, pid));
        }
        watch
    }

    // ---- tokens ------------------------------------------------------------------------------------------------

    /// Each agent's session transcripts, for the usage reader.
    pub fn sessions(&self) -> Vec<(String, Vec<String>)> {
        self.agents.values().map(|a| (a.name.clone(), a.sessions.clone())).collect()
    }

    /// NAME's tokens and latest activity as the usage reader counted them; an agent event when they changed.
    pub fn set_usage(&mut self, name: &str, usage: Vec<ModelUsage>, last: Option<String>) {
        let Some(a) = self.agents.get_mut(name) else { return };
        if (&a.usage, &a.last_active) != (&usage, &last) {
            (a.usage, a.last_active) = (usage, last);
            self.changed(name);
        }
    }

    /// NAME's context as the usage reader found it (paper-research N-101 p1259 point 5).
    pub fn set_context(&mut self, name: &str, context: Option<coord_types::AgentContext>) {
        let Some(a) = self.agents.get_mut(name) else { return };
        if a.context != context {
            a.context = context;
            self.changed(name);
        }
    }

    /// NAME's context as its view shows it (paper-research N-490 p8): what the usage reader found, its window the point
    /// its running session was started with (else the coord's), `next` the point its next start takes when that
    /// differs, and `max` its status line's.
    fn context_of(&self, a: &Agent) -> Option<coord_types::AgentContext> {
        let mut c = a.context.clone()?;
        let coords = c.window;
        let next = a.compact.or(coords);
        c.window = if a.running() { a.live.compact_runs.or(coords) } else { next };
        c.next = next.filter(|n| Some(*n) != c.window);
        // how the next point reaches it (N-804.p16): typed into a running Claude Code session and not answered yet; a
        // restart, where the session answered that another window holds it; else its next start (Codex, a stopped one)
        c.next_by = c.next.map(|_| match () {
            _ if a.running() && a.kind == "claude" && a.live.compact_typing.is_some() => "typing",
            _ if a.running() && a.kind == "claude" => "restart",
            _ => "start",
        }.to_string());
        // a Claude Code agent's from its status line; a Codex agent's as its session's file gave it (the reader's)
        c.max = a.context_max.or(c.max);
        Some(c)
    }

    /// NAME's status line ran (agent-base/claude/statusline, its hook `statusline`): its model's whole window
    /// (`context_window.context_window_size`), when it says one.
    fn statusline_context(&mut self, name: &str, input: &Value) {
        let Some(max) = input.pointer("/context_window/context_window_size").and_then(Value::as_u64).filter(|m| *m > 0) else { return };
        let a = self.agents.get_mut(name).unwrap();
        if a.context_max != Some(max) {
            a.context_max = Some(max);
            self.changed(name);
        }
    }

    /// `coord compact NAME TOKENS` (paper-research N-490 p8): where agent NAME compacts its context, kept as `effort`
    /// keeps a level: the registry (`compact`) and session.json at once, which roster gives a start in its harness's
    /// own way: a Claude Code agent's as its launch flag `--autocompact`, a Codex agent's as
    /// `model_auto_compact_token_limit` (the owner, N-541.p28: "translate that to eg autocompact in claude code other
    /// things store it properly"). A running Claude Code session takes it at once (N-804.p16, the owner: "doing it by
    /// typin / into the claude code nad thus locking hte ui for a bit ofr the user is fine"): its terminal's holder is
    /// given `/autocompact TOKENS` and types it once no turn runs and its input box is empty (`pty::COMMAND`); the
    /// session's answer in its record is what it runs at (`compact_typed`), with no time limit on it. A running Codex
    /// session is not restarted for it: Codex takes a point at a start only (N-490 p5). TOKENS 0: the coord's own point
    /// again for Claude Code (`context_window`), Codex's own default for Codex. The owner's, or an agent's for the
    /// agents below it; never the agent's own.
    pub fn compact(&mut self, who: &Who, name: &str, tokens: u64) -> Reply {
        let name = &match self.not_across("compact", name) {
            Ok(n) => n,
            Err(e) => return err(e),
        };
        let name = name.as_str();
        let Some(a) = self.agents.get(name) else { return err(format!("coord compact: no agent named {name}")) };
        if !self.above(who, name) {
            return err(format!("coord compact: only the agents above {name} set where it compacts"));
        }
        // each harness takes the point in its own way at a start (roster): Claude Code as --autocompact, Codex as
        // model_auto_compact_token_limit; another kind has none we know
        let codex = a.kind == "codex";
        if a.kind != "claude" && !codex {
            return err(format!("coord compact: {name} is {} agent; only a Claude Code or Codex agent's point is kept", kind_article(&a.kind)));
        }
        // Claude Code caps a point at its model's window itself; Codex is not known to, so a point above the window
        // its session's file gave is refused here
        let whole = a.context.as_ref().and_then(|c| c.max).filter(|_| codex);
        let (lo, hi) = (coord_types::COMPACT_MIN, whole.map_or(coord_types::COMPACT_MAX, |w| w.min(coord_types::COMPACT_MAX)));
        let own = if codex { "Codex's own" } else { "the coord's" };
        if tokens != 0 && !(lo..=hi).contains(&tokens) {
            return err(format!("coord compact: {tokens} is not a point: from {} to {} tokens, or 0 for {own}", k(lo), k(hi)));
        }
        let new = (tokens > 0).then_some(tokens);
        let shown = |p: Option<u64>| p.map_or(own.to_string(), k);
        if a.compact == new {
            return ok(format!("{name} already compacts at {} from its next start", shown(new)));
        }
        let old = a.compact;
        let runs = if a.running() { a.live.compact_runs } else { None };
        let running = a.running();
        let at = now();
        // a running Claude Code session is given the command in its terminal, unless it runs at the point already
        let pty = coord_types::pty::socket(&self.coord, name);
        let typed = (running && !codex && runs != new && pty.exists()).then(|| new.or(Self::context_window(&self.coord, &self.base)));
        let a = self.agents.get_mut(name).unwrap();
        (a.compact, a.compact_at) = (new, at.clone());
        if let Some(point) = typed {
            a.live.compact_typing = Some(point.unwrap_or(0));
            let line = format!("/autocompact {}", point.map_or("auto".to_string(), |p| p.to_string()));
            if let Some(jobs) = &self.jobs {
                let _ = jobs.send(Job::Command { name: name.into(), pty: pty.to_string_lossy().into_owned(), line });
            }
        }
        append(&self.path("registry.jsonl"), &json!({"ev": "compact", "name": name, "tokens": tokens, "at": at}));
        self.write_session(name);
        self.changed(name);
        let change = format!("{name}: compacts at {} → {}", shown(old), shown(new));
        ok(match () {
            _ if running && runs == new => format!("{change}; its session runs at it already"),
            _ if typed.is_some() => format!("{change}; typed into its session as /autocompact once no turn runs and its input box is empty"),
            _ => format!("{change} from its next start"),
        })
    }

    /// `/autocompact` typed in NAME's Claude Code session set SET, AT (its transcript, `follow::local`): the point
    /// asked for is the agent's own from then on, as `set_model` keeps a `/model` pick (the owner, 19 Nov: "manual
    /// update for autocomapct on cc shoudl laso work … all setting overwrite from cc by the user hsould worl and be
    /// tracked back into ours"). The session runs at it at once: roster gives a point as the launch flag
    /// `--autocompact`, which the command typed in the session replaces; only a session that another setting holds
    /// (`held_at`: one started while the coord's settings file still set a window) stays where it is until its next
    /// start. `auto`: no point of its own again, and this session at Claude Code's own for its model. A point set
    /// since AT (`compact_at`) stands: the transcript is read again after a coordd restart.
    pub fn compact_typed(&mut self, name: &str, set: crate::follow::Typed, at: &str) {
        let Some(a) = self.agents.get_mut(name).filter(|a| a.kind == "claude") else { return };
        if at.get(..19) <= a.compact_at.get(..19) {
            return;
        }
        let runs = set.held_at.or(set.asked).or(a.context_max);
        let at = format!("{}Z", at.get(..19).unwrap_or(at));
        (a.compact, a.compact_at) = (set.asked, at.clone());
        if a.running() {
            a.live.compact_runs = runs;
            a.live.compact_typing = None;
        }
        append(&self.path("registry.jsonl"), &json!({"ev": "compact", "name": name, "tokens": set.asked.unwrap_or(0), "at": at, "by": "session"}));
        self.write_session(name);
        self.changed(name);
    }

    /// Where the coord's Claude Code sessions compact when an agent has no point of its own: `autoCompactWindow` of
    /// the project's `claude-settings.json` (beside its project file), else of agent-base's `claude/settings.json`.
    /// The key is the coord's point and no setting of the session: roster and `sandbox::home` leave it out of the
    /// settings file a session is started with (on the command line that file stands above what the owner sets
    /// inside Claude Code, and `/autocompact` did nothing: N-541 post 34), and roster gives the point as
    /// `--autocompact`.
    pub fn context_window(coord: &str, base: &str) -> Option<u64> {
        let project = std::fs::read_to_string(format!("{coord}/project")).ok().and_then(|p| Some(std::path::Path::new(p.lines().next()?).parent()?.join("claude-settings.json").display().to_string()));
        project.into_iter().chain([format!("{base}/claude/settings.json")]).find_map(|p| serde_json::from_slice::<Value>(&std::fs::read(p).ok()?).ok()?.get("autoCompactWindow")?.as_u64())
    }

    /// The running agents, for the timer that reads their cgroups.
    pub fn running_names(&self) -> Vec<String> {
        self.agents.values().filter(|a| a.running()).map(|a| a.name.clone()).collect()
    }

    /// Each agent with its transcript and whether it runs, for the task that reads their sizes (`footprint::Kept`).
    pub fn footprint_paths(&self) -> Vec<(String, Option<String>, bool)> {
        self.agents.values().map(|a| (a.name.clone(), a.transcript(), a.running())).collect()
    }

    /// NAME's transcript and folder bytes as the timer read them; its view is told again when they moved.
    pub fn set_footprint(&mut self, name: &str, footprint: (Option<u64>, Option<u64>)) {
        let Some(a) = self.agents.get_mut(name) else { return };
        if a.footprint != footprint {
            a.footprint = footprint;
            self.changed(name);
        }
    }

    /// NAME's memory and CPU as the timer read them (off the lock); its view is told again when they moved (a whole MB,
    /// a whole second).
    /// Each agent's jobs on the build runner (N-1334), from the runner's job files READ (`jobs::read`) and the build
    /// machine as kpi-data's file says it (MACHINE); an agent's view is sent again when its jobs changed. Those that
    /// run come first, the newest first, then the last five that ended, the latest first.
    /// - A job whose caller is gone before its end is `cut` when its agent's session went (restarted since the job
    ///   began, or stopped), at the session's going; else it `ended` with no rc when its file was last written (the
    ///   agent stopped it). That is decided once, at the first read that finds the caller gone, and kept by the job's
    ///   id while its file is there (consult-3, N-1334 p15): a job the agent stopped stays `ended` when its session
    ///   restarts later. A restarted coordd keeps none of it, so its first read decides again: a session restart while
    ///   coordd was down makes a job whose caller went before it `cut`.
    /// - One that ended after its agent's last turn end is `fresh` (consult-lead, N-1334 p2: `done` and `failed` stand
    ///   through the turn the job's end began). Before coordd has taken a turn's end of it, its transcript's latest
    ///   time stands for that.
    /// - One waiting for its machine (sending, queued) is `unreachable` exactly while kpi-data's file says that
    ///   machine does not answer, the one source of that fact (consult-lead p2); while the file says it answers, the
    ///   runner's own `unreachable` reads as `sending`, which the runner tries again. A file that says nothing of the
    ///   job's machine (none fresh, `jobs::machine`, or one that names another) leaves the runner's own step as it is
    ///   (paper-research N-1446 p2778: one nobody wrote since 22 Nov held every waiting job at `unreachable`, p2777).
    pub fn set_jobs(&mut self, read: Vec<crate::jobs::Read>, machine: Option<crate::jobs::Machine>) {
        let mut by: HashMap<String, Vec<coord_types::Job>> = HashMap::new();
        // the decisions of the last read; only those of jobs read again are kept
        let mut was_gone = std::mem::take(&mut self.runner_gone);
        for r in read {
            let Some(a) = self.agents.get(&r.agent).filter(|_| r.coord == self.coord) else { continue };
            let mut j = r.job;
            if j.runs() && !r.alive {
                let gone = was_gone.remove(&j.id).unwrap_or_else(|| {
                    let went = if a.running() { (a.live.started > j.since).then(|| a.live.started.clone()) } else { Some(a.live.since.clone()) };
                    match went {
                        Some(at) => ("cut".into(), at),
                        None => ("ended".into(), r.written.clone()),
                    }
                });
                self.runner_gone.insert(j.id.clone(), gone.clone());
                (j.step, j.step_since) = gone;
                (j.what, j.done, j.of, j.ahead) = (None, None, None, None);
            }
            let said = machine.as_ref().filter(|m| j.machine.as_deref() == Some(m.name.as_str()));
            if let (Some(m), true) = (said, matches!(j.step.as_str(), "sending" | "queued" | "unreachable")) {
                if !m.on {
                    j.step = "unreachable".into();
                } else if j.step == "unreachable" {
                    j.step = "sending".into();
                }
            }
            j.fresh = !j.runs() && a.live.turn_ended.as_ref().or(a.last_active.as_ref()).is_none_or(|t| j.step_since > *t);
            by.entry(r.agent).or_default().push(j);
        }
        let names: Vec<String> = self.runner_jobs.keys().chain(by.keys()).cloned().collect::<BTreeSet<_>>().into_iter().collect();
        for name in names {
            if !self.agents.contains_key(&name) {
                self.runner_jobs.remove(&name);
                continue;
            }
            let mut all = by.remove(&name).unwrap_or_default();
            let ended = all.iter().filter(|j| !j.runs()).count();
            all.sort_by(|x, y| y.runs().cmp(&x.runs()).then_with(|| if x.runs() { y.since.cmp(&x.since) } else { y.step_since.cmp(&x.step_since) }));
            all.truncate(all.len() - ended.saturating_sub(5));
            if self.runner_jobs.get(&name).map_or(all.is_empty(), |was| *was == all) {
                continue;
            }
            if all.is_empty() {
                self.runner_jobs.remove(&name);
            } else {
                self.runner_jobs.insert(name.clone(), all);
            }
            let v = self.view(&name);
            self.emit(Body::Agent(v));
        }
    }

    /// What agent NAME's next hand-over begins with (N-1334; paper-research: "an agent whose job was stopped with its
    /// session is told so at its next hand-over, from the job files"): a line for each job of it cut with its session
    /// since its last turn end. Said in each hand-over until its next turn end, as a layer's warning is said until a
    /// turn given it ends; it starts no turn by itself and is no post.
    pub fn job_notices(&self, name: &str) -> Vec<String> {
        self.runner_jobs
            .get(name)
            .into_iter()
            .flatten()
            .filter(|j| j.step == "cut" && j.fresh)
            .filter_map(|j| crate::render::template_in(&self.places_for(name), "stopped", &json!({"job": j.reference(), "at": crate::jobs::clock(&j.step_since)})).map_err(|e| eprintln!("coordd: {name}: rendering a stopped job's notice: {e}")).ok().flatten())
            .collect()
    }

    pub fn set_cgroup(&mut self, name: &str, mem_mb: Option<u64>, cpu_s: Option<f64>) {
        let Some(a) = self.agents.get_mut(name) else { return };
        let moved = a.mem_mb != mem_mb || a.cpu_s.map(f64::floor) != cpu_s.map(f64::floor);
        (a.mem_mb, a.cpu_s) = (mem_mb, cpu_s);
        if moved {
            self.changed(name);
        }
    }

    /// Keep the requests the usage reader read of NAME's sessions (a request read again replaces what it was).
    /// The requests kept (`add_sent`), for the usage reader's kept state.
    pub fn sent_kept(&self) -> Vec<(String, Sent)> {
        self.sent.iter().map(|(id, s)| (id.clone(), s.clone())).collect()
    }

    /// The requests kept at an earlier start, put back (the usage reader then reads only what is new).
    pub fn restore_sent(&mut self, sent: Vec<(String, Sent)>) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
        let cutoff = now - SENT_DAYS * 86_400_000;
        for (id, s) in sent.into_iter().filter(|(_, s)| s.at >= cutoff) {
            self.sent.entry(id).or_insert(s);
        }
        crate::counts::sent_now(self.sent.len());
    }

    pub fn add_sent(&mut self, name: &str, ended: Vec<transcript::Ended>) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
        let cutoff = now - SENT_DAYS * 86_400_000;
        for e in ended.into_iter().filter(|e| e.at >= cutoff) {
            self.sent.insert(e.id, Sent { agent: name.into(), at: e.at, model: e.model, tokens: e.tokens, cache: e.cache });
        }
        crate::counts::sent_now(self.sent.len());
    }

    /// The requests of the agents below WHO (all for the owner) that ended at or after SINCE, oldest first.
    pub fn requests(&self, who: &Who, since: i64) -> Reply {
        let mut out: Vec<&Sent> = self.sent.values().filter(|s| s.at >= since && self.above(who, &s.agent)).collect();
        out.sort_by_key(|s| s.at);
        ok(serde_json::to_string(&out).unwrap())
    }

    // ---- references and rights ---------------------------------------------------------------------------------

    /// WHO is above agent NAME (the owner is above everyone).
    pub fn above(&self, who: &Who, name: &str) -> bool {
        let Who::Agent(me) = who else { return true };
        let (mut cur, mut seen) = (name.to_string(), HashSet::new());
        while let Some(a) = self.agents.get(&cur) {
            if &a.parent == me {
                return true;
            }
            if !seen.insert(cur.clone()) {
                break;
            }
            cur = a.parent.clone();
        }
        false
    }

    /// The host sessions: Claude Code sessions on the host that coord-relay reaches (`$COORD/hosts/NAME` holds the
    /// session's id), not agents here.
    pub fn hosts(&self) -> Vec<String> {
        std::fs::read_dir(self.path("hosts"))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| !n.starts_with('.') && !self.agents.contains_key(n))
            .collect()
    }

    /// What is due to a host session (a note naming it, a post it is told of) goes to it as mail did: through the feed
    /// to coord-relay (paper-research m1052, 1: on Tovrin the owner's own sessions and paper-research are host
    /// sessions, and the cut-over must not cut them off). Everything due at once: no turn of theirs is followed, so no
    /// one-step rule and no Reply; they answer with `coord msg --thread`.
    pub fn hand_hosts(&mut self) {
        let Some(jobs) = self.jobs.clone() else { return };
        // off unless the project file says host_relay = true: production's relay reads every coord's feed
        if !self.project.as_ref().is_some_and(|p| p.project.host_relay) {
            return;
        }
        for h in self.hosts() {
            let due = self.due_now(&h);
            if due.is_empty() {
                continue;
            }
            let deliveries = self.ticket_handing(&h, due);
            let _ = jobs.send(Job::Host { name: h, deliveries });
        }
    }

    /// Delivery D to host session NAME, rendered as CONTENT with META: a relay line in the feed, as `coord msg` to a host
    /// wrote one, headed as a Codex agent's text is; then given.
    pub fn to_host(&mut self, name: &str, d: &docket_machine::Delivery, content: &str, meta: &Value) {
        let f = |k: &str| meta.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        let step = if f("step").is_empty() { String::new() } else { format!(" step {}", f("step")) };
        let text = format!("[{} · from {} · ticket {} {}{step}]\n{content}", d.id, f("from"), d.ticket, d.kind);
        append(&self.path("feed.jsonl"), &json!({"at": now(), "from": f("from"), "kind": "msg", "id": d.id, "to": name, "text": text, "delivery": "relay"}));
        self.ticket_given(name, d);
    }

    pub(crate) fn is_host(&self, name: &str) -> bool {
        std::path::Path::new(&self.path(&format!("hosts/{name}"))).is_file()
    }

    // ---- agents ------------------------------------------------------------------------------------------------

    pub fn add(&mut self, who: &Who, name: &str, kind: &str, model: &str) -> Reply {
        self.add_from(who, name, kind, model, None)
    }

    /// [`Core::add`], NAME being a copy of FROM's conversation (`roster fork`: `coord add NAME KIND MODEL --from FROM`),
    /// recorded on its `add` line: the spin-off of FROM that the window shows with Join back, Close and Delete
    /// (`AgentView::from`). A FROM that is no agent is refused, nothing recorded.
    pub fn add_from(&mut self, who: &Who, name: &str, kind: &str, model: &str, from: Option<&str>) -> Reply {
        let from = from.filter(|f| !f.is_empty());
        if let Some(f) = from.filter(|f| !self.agents.contains_key(*f)) {
            return err(format!("coord add: {name} is a copy of {f}, which is no agent"));
        }
        if name.is_empty() || name.contains('/') || reserved(name) {
            return err(format!("coord add: not a name: {name:?}"));
        }
        let parent = who.name();
        // the models the project lets an agent run (`run-model`; a policy without it lets every model): for the owner's
        // agents too (owner 10 Nov: the yaaihh box runs Haiku and Luna only, to keep its cost down)
        if kind != "program" {
            if let Err(why) = self.may_run(model) {
                return err(format!("coord add: {name} may not run {model}: {why}"));
            }
        }
        if let Some(a) = self.agents.get_mut(name) {
            // already as said (a made agent, registered by coordd, then by roster's `coord add` as it starts): nothing
            // to record again (docket-core4, N-101 p400)
            if a.kind == kind && a.model == model {
                return ok(a.id.clone());
            }
            a.kind = kind.into();
            a.model = model.into();
            let (id, parent) = (a.id.clone(), a.parent.clone());
            append(&self.path("registry.jsonl"), &json!({"ev": "add", "id": id, "name": name, "parent": parent, "kind": kind, "model": model, "at": now()}));
            self.changed(name);
            self.box_agent(name);
            return ok(id);
        }
        // an agent staffing: as the norms allow (`add-agent`); under its team grant, a member counted against it
        let mut member_of = None;
        if let Who::Agent(me) = who {
            if let Err(why) = self.permitted("add-agent", &[me], "none") {
                return err(format!("coord add: {me} may not add agents ({why}); a team is asked for with `coord team ask`"));
            }
            if let Some(g) = self.agents.get_mut(me).and_then(|a| a.team.as_mut()).filter(|g| g.used < g.granted) {
                g.used += 1;
                member_of = Some(me.clone());
            }
        }
        let id = format!("AGT-{:03}", self.agents.len() + 1);
        let mut rec = json!({"ev": "add", "id": id, "name": name, "parent": parent, "kind": kind, "model": model, "at": now()});
        if let Some(f) = from {
            rec["from"] = json!(f);
        }
        if let Some(l) = &member_of {
            rec["member_of"] = json!(l);
            self.changed(l);
        }
        append(&self.path("registry.jsonl"), &rec);
        let mut a = self.new_agent(id.clone(), name.into(), parent, kind.into(), model.into(), now());
        a.member_of = member_of;
        a.from = from.map(String::from);
        self.agents.insert(name.into(), a);
        self.norm_gen += 1;
        self.native_as_all(name);
        self.mode_at_add(name);
        self.changed(name);
        self.box_agent(name);
        ok(id)
    }

    /// Agent NAME's deliveries drawn from the renderings SET from now on (`coord add NAME KIND MODEL --renderings SET`):
    /// a `renderings` line in the registry. SET is a folder's name, nothing more.
    pub fn set_renderings(&mut self, name: &str, set: &str) -> Result<(), String> {
        renderings_name(set)?;
        if let Some(a) = self.agents.get_mut(name).filter(|a| a.renderings.as_deref() != Some(set)) {
            a.renderings = Some(set.into());
            append(&self.path("registry.jsonl"), &json!({"ev": "renderings", "name": name, "set": set, "at": now()}));
            self.changed(name);
        }
        Ok(())
    }

    /// The places NAME's deliveries are rendered from: the project's, with NAME's renderings ([`Agent::renderings`]).
    pub fn places_for(&self, name: &str) -> crate::policy::Places {
        crate::policy::Places { renderings: self.agents.get(name).and_then(|a| a.renderings.clone()), ..self.places() }
    }

    /// In a boxed project, agent NAME made its own user from the moment it is registered (N-101 post 406, the joint
    /// proposal's A): its uid, its limits on `agents/NAME` and its home, built for the project's path (`sandbox::ready`;
    /// every start does it again, `launch`). Programs (the runner) are left out. Its inside uid; none when the project
    /// is not boxed or NAME is a program; why not, otherwise, logged: the registration stands, and its start tries again.
    pub(crate) fn box_agent(&mut self, name: &str) -> Option<u32> {
        let p = self.project.as_ref().filter(|p| p.project.boxed)?;
        if self.agents.get(name).is_some_and(|a| a.kind == "program") {
            return None;
        }
        let coord = self.places().coord;
        let dir = p.repo().unwrap_or_default();
        let h = crate::sandbox::Home { coord: &coord, name, k: 0, dir: &dir, base: &self.base, project_settings: Some(format!("{}/claude-settings.json", p.dir)), owner: format!("{}/.claude.json", std::env::var("HOME").unwrap_or_default()), setup: p.box_setup() };
        let made = crate::sandbox::ready(&h, &p.agent_limits(name, true));
        self.boxed_or_not(name, made.as_ref().err().cloned());
        match made {
            Ok(k) => Some(k),
            Err(e) => {
                eprintln!("coordd: {name}: its box at registration: {e} (it is not started until its box is made)");
                None
            }
        }
    }

    /// Agent NAME's box was made (WHY none) or not (`no_box`, shown by `coord ls`).
    pub(crate) fn boxed_or_not(&mut self, name: &str, why: Option<String>) {
        if let Some(a) = self.agents.get_mut(name) {
            if a.no_box != why {
                a.no_box = why;
                self.changed(name);
            }
        }
    }

    /// Register NAME, an agent BY made from its turn (`coord new`, `coord fork`; item 11), on MODEL, a copy of FROM if
    /// given: BY its parent, FROM on the registry's `add`. The policy's `make-agent` let it (make.rs), in place of
    /// `add-agent`; the models the project lets run hold as for any agent, and a team grant counts it as `add` does.
    pub(crate) fn add_made(&mut self, by: &str, name: &str, model: &str, from: Option<&str>) -> Result<(), String> {
        if name.is_empty() || name.contains('/') || reserved(name) || name.starts_with('-') {
            return Err(format!("not a name: {name:?}"));
        }
        if self.agents.contains_key(name) {
            return Err(format!("{name} is an agent already"));
        }
        self.may_run(model).map_err(|why| format!("{name} may not run {model}: {why}"))?;
        let mut member_of = None;
        if let Some(g) = self.agents.get_mut(by).and_then(|a| a.team.as_mut()).filter(|g| g.used < g.granted) {
            g.used += 1;
            member_of = Some(by.to_string());
        }
        let id = format!("AGT-{:03}", self.agents.len() + 1);
        let mut rec = json!({"ev": "add", "id": id, "name": name, "parent": by, "kind": "claude", "model": model, "at": now()});
        if let Some(f) = from {
            rec["from"] = json!(f);
        }
        if let Some(l) = &member_of {
            rec["member_of"] = json!(l);
            self.changed(l);
        }
        append(&self.path("registry.jsonl"), &rec);
        let mut a = self.new_agent(id, name.into(), by.into(), "claude".into(), model.into(), now());
        a.member_of = member_of;
        self.agents.insert(name.into(), a);
        self.norm_gen += 1;
        self.native_as_all(name);
        self.mode_at_add(name);
        self.changed(name);
        self.box_agent(name);
        Ok(())
    }

    /// Agent NAME is called TO from now on: its id, session, folder (agents/TO) and children go with it; mail and
    /// items already written keep the name they were written with. Only a stopped agent: a running one's processes
    /// carry its name (AGENT_NAME, its cgroup), so it is renamed between a stop and its restart (`roster rename`).
    pub fn rename(&mut self, who: &Who, name: &str, to: &str) -> Reply {
        if to.is_empty() || to.contains('/') || reserved(to) {
            return err(format!("coord rename: not a name: {to:?}"));
        }
        if self.agents.contains_key(to) || self.is_host(to) {
            return err(format!("coord rename: {to} is taken"));
        }
        let Some(a) = self.agents.get(name) else { return err(format!("coord rename: no agent {name}")) };
        if !self.above(who, name) {
            return err(format!("coord rename: only the agents above {name} rename it"));
        }
        if a.live.pid.is_some_and(|p| std::path::Path::new(&format!("/proc/{p}")).exists()) {
            return err(format!("coord rename: {name} is running; stop it first (roster rename does both)"));
        }
        let (from_dir, to_dir) = (self.path(&format!("agents/{name}")), self.path(&format!("agents/{to}")));
        if std::path::Path::new(&from_dir).exists() {
            if let Err(e) = std::fs::rename(&from_dir, &to_dir) {
                return err(format!("coord rename: {from_dir}: {e}"));
            }
        }
        cgroup::forget(name);
        append(&self.path("registry.jsonl"), &json!({"ev": "rename", "name": name, "to": to, "at": now()}));
        // its account is bound to it by name: it keeps it under the new one (accounts/mover.rs)
        self.account_renamed(name, to);
        self.renamed(name, to);
        self.norm_gen += 1;
        let snap = self.snapshot();
        self.emit(snap.body);
        self.journal();
        ok(String::new())
    }

    /// Agent NAME's role is ROLE from now on: the owner's to set (the teams are the owner's).
    pub fn set_role(&mut self, who: &Who, name: &str, role: &str) -> Reply {
        if !matches!(who, Who::Owner) {
            return err("coord role: only the owner sets roles".into());
        }
        // a role is a word the policy's norms speak of (agent-base/policy/norms.eflint: lead, consultant, ...)
        if role.is_empty() || !role.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return err(format!("coord role: not a role: {role:?} (a word, as the policy's norms name it)"));
        }
        let Some(a) = self.agents.get_mut(name) else { return err(format!("coord role: no agent {name}")) };
        a.role = role.into();
        self.norm_gen += 1;
        append(&self.path("registry.jsonl"), &json!({"ev": "role", "name": name, "role": role, "at": now()}));
        self.record_roles();
        self.changed(name);
        ok(String::new())
    }

    /// The owner sets agent NAME's pace (`Request::AgentPace`): one registry `control` line a change, kept over
    /// a restart of coordd. Back in Run, what was staged for it goes as it comes.
    pub fn set_agent_mode(&mut self, who: &Who, name: &str, mode: AgentMode) -> Reply {
        if !matches!(who, Who::Owner) {
            return err("only the owner sets an agent's pace".into());
        }
        let Some(a) = self.agents.get_mut(name) else { return err(format!("no agent {name}")) };
        if a.control == mode {
            return ok(format!("{name} is in {} already", mode_name(mode)));
        }
        a.control = mode;
        append(&self.path("registry.jsonl"), &json!({"ev": "control", "name": name, "mode": mode, "at": now()}));
        self.changed(name);
        self.poke.notify_one();
        ok(format!("{name} is in {} now", mode_name(mode)))
    }

    /// `coord ended NAME`: the owner ends agent NAME for good, recorded in the registry. It is never started again, is no
    /// option for `fills` and is not in an `agents` list; its sessions are kept. UNDO (`coord ended --undo NAME`): the
    /// owner takes the mark back. A running agent is stopped first, by the owner.
    pub fn set_ended(&mut self, who: &Who, name: &str, undo: bool) -> Reply {
        if !matches!(who, Who::Owner) {
            return err("coord ended: only the owner ends an agent".into());
        }
        let Some(a) = self.agents.get_mut(name) else { return err(format!("coord ended: no agent {name}")) };
        match (undo, &a.ended) {
            (false, Some(at)) => return err(format!("coord ended: {name} was ended at {at}")),
            (true, None) => return err(format!("coord ended --undo: {name} is not ended")),
            (false, None) if a.running() => return err(format!("coord ended: {name} is running; stop it first")),
            _ => {}
        }
        let at = now();
        a.ended = if undo { None } else { Some(at.clone()) };
        let mut r = json!({"ev": "ended", "name": name, "at": at});
        if undo {
            r["undo"] = json!(true);
        }
        append(&self.path("registry.jsonl"), &r);
        self.norm_gen += 1;
        // its role lines at once: an ended agent holds none, and an undone end gives its role back (`record_roles`)
        self.record_roles();
        self.changed(name);
        ok(String::new())
    }

    /// `coord team ask TEXT`: a request ticket (`ticket_team_ask`; slice 4). A number at TEXT's end is how many agents.
    pub async fn team_ask(&mut self, who: &Who, text: &str) -> Reply {
        let (words, n) = match text.trim().rsplit_once(char::is_whitespace) {
            Some((w, n)) if n.parse::<u32>().is_ok() => (w.to_string(), n.parse().ok()),
            _ => (text.to_string(), None),
        };
        self.ticket_team_ask(who, &words, n)
    }

    /// `coord team grant NAME N`: NAME's team may have N agents from now on (a new grant starts its count again). The
    /// owner, or an agent the norms let approve NAME's team (`approve-team`).
    pub async fn team_grant(&mut self, who: &Who, name: &str, granted: u32) -> Reply {
        if !self.agents.contains_key(name) {
            return err(format!("coord team grant: no agent {name}"));
        }
        let by = who.name();
        if let Who::Agent(me) = who {
            match self.norm_verdict("approve-team", &[me, name], "none") {
                Ok(Some(v)) if v.enabled => {}
                Ok(Some(v)) => return err(format!("coord team grant: {me} may not approve {name}'s team ({})", v.why)),
                Ok(None) => return err("coord team grant: the policy has no act approve-team: only the owner grants".into()),
                Err(e) => return err(format!("coord team grant: the norms: {e}")),
            }
        }
        let _ = self.apply_grant(name, granted, &by);
        let body = format!("Team granted: up to {granted} agent(s) in your team (by {by}). Add them with `roster fork {name} --name NEW TEXT` (a copy of your conversation) or `roster add NEW MODEL PROMPT-FILE DIR` (a fresh agent, working in DIR).");
        self.msg(who, None, vec![name.into()], &body, false, false, None, Vec::new()).await
    }

    /// NAME's team may have GRANTED agents from now on, granted BY (`coord team grant`, a request closed Granted):
    /// recorded in the registry. The count starts again.
    pub fn apply_grant(&mut self, name: &str, granted: u32, by: &str) -> Result<(), String> {
        let a = self.agents.get_mut(name).ok_or_else(|| format!("no agent {name}"))?;
        a.team = Some(TeamGrant { granted, used: 0, by: by.to_string() });
        append(&self.path("registry.jsonl"), &json!({"ev": "team", "name": name, "granted": granted, "by": by, "at": now()}));
        self.norm_gen += 1;
        self.changed(name);
        Ok(())
    }

    /// The proxy's models and their families, as last read (`set_models`).
    pub fn models(&self) -> &[(String, String)] {
        &self.models
    }

    /// The proxy's models and their families (`main::read_models`), for the norms' `model` and `family` facts.
    pub fn set_models(&mut self, models: Vec<(String, String)>) {
        if models != self.models {
            self.models = models;
            self.norm_gen += 1;
        }
    }

    /// What a solve of the tasks' models takes (`policy::solve_task_models`, off the lock), as it stands: where the
    /// policy is, the watcher's tasks (read at load, `Tickets::call_files`), what they were last solved from.
    pub fn task_models_ask(&self) -> crate::policy::TasksAsk {
        crate::policy::TasksAsk { places: self.places(), tasks: self.tickets.call_files.tasks.clone(), kept: self.task_models.key.clone() }
    }

    /// The tasks' models solved off the lock (KEY: what from), kept for the Kinds answer.
    pub fn set_task_models(&mut self, (key, by_task): (String, BTreeMap<String, crate::policy::TaskModel>)) {
        self.task_models = crate::policy::TaskModels { key, by_task, solves: self.task_models.solves + 1 };
    }

    /// The model Claude Code's Agent tool runs for NAME's call INPUT: its `model` (an alias is the newest undated model
    /// of that line the proxy serves), else NAME's own (no model, or the `fork` type, which runs the parent's).
    fn subagent_model(&self, name: &str, input: &Value) -> Option<String> {
        let own = || self.agents[name].model.split(':').next().unwrap_or("").to_string();
        let ti = &input["tool_input"];
        let Some(m) = ti["model"].as_str().filter(|m| !m.is_empty() && ti["subagent_type"].as_str() != Some("fork")) else { return Some(own()) };
        if self.models.iter().any(|(id, _)| id == m) {
            return Some(m.into());
        }
        let line = format!("claude-{m}-");
        self.models
            .iter()
            .filter_map(|(id, _)| {
                let v: Option<Vec<u64>> = id.strip_prefix(&line)?.split('-').map(|p| if p.len() < 8 { p.parse().ok() } else { None }).collect();
                Some((v?, id))
            })
            .max()
            .map(|(_, id)| id.clone())
    }

    /// Claude Code's PreToolUse of the Agent tool by NAME: refused unless the norms enable `start-subagent` for its model;
    /// the answer is Claude Code's hook output (vault designs/teams-and-tiers-2026-11-10.md §4).
    fn subagent_check(&self, name: &str, input: &Value) -> String {
        let why = match self.subagent_model(name, input) {
            None => Some(format!("no model the proxy serves is called {:?}", input["tool_input"]["model"])),
            Some(m) => self.may_run(&m).and_then(|_| self.permitted("start-subagent", &[name, &m], "none")).err().map(|w| format!("a subagent on {m}: {w}")),
        };
        match why {
            None => String::new(),
            Some(why) => json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny",
                "permissionDecisionReason": format!("The team policy refuses this subagent: {why}. A subagent runs a lower model tier of your own family: pass `model` (haiku, sonnet, opus, fable) below yours. For work that needs your tier or another family, ask for a team (`coord team ask`).")}})
            .to_string(),
        }
    }

    /// The agents the owner wrote to in THREAD ("none": no thread), sorted: the one part of the norms' situation a
    /// thread changes.
    fn brought_in(&self, thread: &str) -> Vec<String> {
        let mut out: Vec<String> = self.thread(thread).into_iter().filter(|m| m.from == "owner").flat_map(|m| m.to.iter().filter(|t| self.agents.contains_key(*t)).cloned()).collect();
        out.sort();
        out.dedup();
        out
    }

    /// The situation the norms are asked in (policy/norms.rs): every agent and its role (a member when it has none),
    /// the threads "none" and "t", and BROUGHT brought into "t". Every thread is asked as "t" with its own brought-in
    /// agents, so a situation solved once answers every thread with the same ones (the norms name no thread).
    pub(crate) fn norm_facts(&self, brought: &[String]) -> Vec<(String, Vec<String>)> {
        let mut f: Vec<(String, Vec<String>)> = Vec::new();
        for a in self.agents.values() {
            f.push(("agent".into(), vec![a.name.clone()]));
            f.push(("role".into(), vec![a.name.clone(), if a.role.is_empty() { "member".into() } else { a.role.clone() }]));
        }
        // teams and tiers (vault designs/teams-and-tiers-2026-11-10.md §3)
        for (m, fam) in &self.models {
            f.push(("model".into(), vec![m.clone()]));
            f.push(("family".into(), vec![m.clone(), fam.clone()]));
        }
        for a in self.agents.values() {
            f.push(("runs".into(), vec![a.name.clone(), a.model.split(':').next().unwrap_or("").to_string()]));
            if self.agents.contains_key(&a.parent) {
                f.push(("parent".into(), vec![a.name.clone(), a.parent.clone()]));
            }
            // every agent above it, up its parents (the norms' `esc`: an agent may stop those below it)
            let (mut up, mut seen) = (a.parent.clone(), HashSet::new());
            while let Some(p) = self.agents.get(&up).filter(|_| seen.insert(up.clone())) {
                f.push(("below".into(), vec![a.name.clone(), p.name.clone()]));
                up = p.parent.clone();
            }
            if let Some(g) = &a.team {
                f.push(("granted".into(), vec![a.name.clone(), g.granted.to_string()]));
                f.push(("used".into(), vec![a.name.clone(), g.used.to_string()]));
            }
            if a.member_of.is_some() {
                f.push(("member".into(), vec![a.name.clone()]));
            }
        }
        f.push(("thread".into(), vec!["none".into()]));
        f.push(("thread".into(), vec!["t".into()]));
        for b in brought {
            f.push(("brought-in".into(), vec![b.clone(), "t".into()]));
        }
        // a ticket is asked as "t" too (`name`): those taking part in it, as the threads' brought-in
        f.push(("ticket".into(), vec!["none".into()]));
        f.push(("ticket".into(), vec!["t".into()]));
        for b in brought {
            f.push(("takes-part".into(), vec![b.clone(), "t".into()]));
        }
        // and the facts the record's acts made and have not ended (ticket/effects.rs; the engine page's move 6)
        f.extend(self.tickets.effects.facts());
        f
    }

    /// The sidebar's parts (N-412 ask 2): the policy's `part` and `part-place` instances, read once each time
    /// `norm_gen` moves or the policy files change (compared at most every 2 s), not at every view. A policy that
    /// cannot be read gives none, said on stderr.
    pub(crate) fn parts(&self) -> crate::policy::parts::Parts {
        let mut cache = self.parts_cache.borrow_mut();
        let now = std::time::Instant::now();
        let stale = match cache.as_mut() {
            None => true,
            Some(c) if c.generation != self.norm_gen => true,
            Some(c) if now.duration_since(c.checked) >= Duration::from_secs(2) => {
                c.checked = now;
                self.places().stamp() != c.stamp
            }
            Some(_) => false,
        };
        if stale {
            let places = self.places();
            let parts = crate::policy::parts::load(&places).unwrap_or_else(|e| {
                eprintln!("coordd: the sidebar's parts: {e}");
                Default::default()
            });
            *cache = Some(PartsCache { stamp: places.stamp(), generation: self.norm_gen, checked: now, parts });
        }
        cache.as_ref().unwrap().parts.clone()
    }

    /// Each minute (ticket/mod.rs `expiry`): the parts as the policy states them now, and the clocks as the kinds write
    /// them; when either differs from what the last snapshot carried, a new snapshot carries them (the owner edited the
    /// policy; a reload added, dropped or changed a clock, 0.2 line 26). Its agents' views carry their parts with it.
    pub fn parts_round(&mut self) {
        let now = self.parts();
        let clocks = self.clocks_as_written();
        if self.parts_told.borrow().as_ref().is_some_and(|told| *told != now) || self.clocks_told.borrow().as_ref().is_some_and(|told| *told != clocks) {
            let snap = self.snapshot();
            self.emit(snap.body);
        }
    }

    /// The norms' verdict on ACT for ARGS about THREAD: from the situations solved already (`NormCache`), solving only
    /// a situation not met since the policy, an agent or a role last changed (design coordd-core-2026-11-10 §5).
    /// None: the policy has no such act.
    pub(crate) fn norm_verdict(&self, act: &str, args: &[&str], thread: &str) -> Result<Option<crate::policy::norms::Verdict>, String> {
        self.with_norms(thread, |norms, shown| {
            let Some(a) = norms.acts.get(act) else { return Ok(None) };
            // the thread fields are asked as "t" (or "none"), as the situation has them
            let args: Vec<&str> = args.iter().zip(a.fields.iter().map(|(_, ty)| ty.as_str()).chain(std::iter::repeat(""))).map(|(v, ty)| if ty == "thread" || ty == "ticket" { if thread == "none" { "none" } else { "t" } } else { *v }).collect();
            norms.verdict(shown, act, &args)
        })
    }

    /// F given the norms and the situation about THREAD solved (`NormCache`; see `norm_verdict`).
    fn with_norms<R>(&self, thread: &str, f: impl FnOnce(&crate::policy::interp::Acts, &crate::policy::interp::Shown) -> Result<R, String>) -> Result<R, String> {
        let mut cache = self.norm_cache.borrow_mut();
        let now = std::time::Instant::now();
        let stale = match cache.as_mut() {
            None => true,
            Some(c) if c.generation != self.norm_gen => true,
            Some(c) if now.duration_since(c.checked) >= Duration::from_secs(2) => {
                c.checked = now;
                self.places().stamp() != c.stamp
            }
            Some(_) => false,
        };
        if stale {
            let places = self.places();
            *cache = Some(NormCache { stamp: places.stamp(), generation: self.norm_gen, checked: now, norms: crate::policy::interp::acts(&places), solved: HashMap::new() });
        }
        let c = cache.as_mut().unwrap();
        let norms = c.norms.as_ref().map_err(|e| e.clone())?;
        let brought = if thread == "none" {
            Vec::new()
        } else if let Some(t) = docket_machine::parse_ticket(thread).filter(|t| self.tickets.m.tickets.contains_key(t)) {
            // a ticket: those taking part in it, sorted, so one situation answers every ticket with the same ones
            let given = self.tickets.m.tickets[&t].given.iter().map(|g| g.to.clone());
            let mut p: Vec<String> = given.chain(self.takers(t)).collect();
            p.sort();
            p.dedup();
            p
        } else {
            self.brought_in(thread)
        };
        if !c.solved.contains_key(&brought) {
            let shown = norms.solve(&self.norm_facts(&brought));
            c.solved.insert(brought.clone(), shown);
        }
        let shown = c.solved[&brought].as_ref().map_err(|e| e.clone())?;
        f(norms, shown)
    }

    /// `coord duties [--as NAME]` (paper-research N-568 part 6): the duties that hold now in the team's situation, as
    /// the interpreter answers (eflint-rs, whatever the mode: norms.rs has none), each with its holder, claimant,
    /// whether it is violated and the acts that end it. An agent gets those it holds; the owner those HOLDER holds, or
    /// all. Nothing written.
    pub fn duties(&self, who: &Who, holder: Option<&str>) -> Reply {
        match self.duties_ask(who, holder) {
            DutiesAsk::Done(r) => r,
            DutiesAsk::Run(job) => match job.run() {
                Ok(list) => duties_reply(job.holder.as_deref(), &list),
                Err(e) => err(format!("coord duties: the norms could not be asked: {e}")),
            },
        }
    }

    /// `coord duties` under the lock (consult-3, N-581.p19: the interpreter is not run on the caller's path under the
    /// lock): who may ask for whose, and the facts and policy files the answer stands on. The last answer, when it was
    /// for the same facts and files, is given at once; otherwise the job to run off the lock ([`DutiesJob::run`]), whose
    /// answer [`Core::duties_keep`] keeps.
    pub fn duties_ask(&self, who: &Who, holder: Option<&str>) -> DutiesAsk {
        let holder = match (who, holder) {
            (Who::Agent(me), None) => Some(me.clone()),
            (Who::Agent(me), Some(a)) if a == me => Some(me.clone()),
            (Who::Agent(_), Some(a)) => return DutiesAsk::Done(err(format!("coord duties: an agent asks for its own duties, not {a}'s"))),
            (Who::Owner, h) => h.map(String::from),
        };
        let places = self.places();
        let job = DutiesJob { holder, facts: self.norm_facts(&[]), stamp: places.stamp(), places };
        match &*self.duties_kept.borrow() {
            Some((facts, stamp, list)) if *facts == job.facts && *stamp == job.stamp => DutiesAsk::Done(duties_reply(job.holder.as_deref(), list)),
            _ => DutiesAsk::Run(job),
        }
    }

    /// Keep JOB's answer LIST for the next `coord duties` on the same facts and files.
    pub fn duties_keep(&self, job: DutiesJob, list: Vec<crate::policy::interp::Duty>) {
        *self.duties_kept.borrow_mut() = Some((job.facts, job.stamp, list));
    }

    /// `coord can [THING]` (paper-research N-568 p15, part 6: the drivers): the acts ACTOR may do now at THING (a
    /// ticket, or none: the team), as the norms stand: each instance whose actor is ACTOR, enabled, or greyed with the
    /// clause that fails (it holds and is suppressed). The same answer the window's menu reads. The owner asks for an
    /// agent's (he is bound by no act of the norms); an agent for itself.
    pub fn enabled(&self, who: &Who, thing: Option<&str>, actor: Option<&str>) -> Reply {
        let actor = match (who, actor) {
            (Who::Agent(me), None) => me.clone(),
            (Who::Agent(me), Some(a)) if a == me => me.clone(),
            (Who::Agent(_), Some(a)) => return err(format!("coord can: an agent asks for its own acts, not {a}'s")),
            (Who::Owner, Some(a)) if self.agents.contains_key(a) => a.to_string(),
            (Who::Owner, Some(a)) => return err(format!("coord can: no agent named {a}")),
            (Who::Owner, None) => return err("coord can: the owner is bound by no act of the norms; name an agent (--as NAME) to see its acts".into()),
        };
        // THING: a ticket (its thread's acts); one or more identifiers, comma-separated (the team's acts that name any
        // of them: an agent's row, or the rows the window draws for a team; what a team is, is the library's, N-604.p3);
        // none (all the team's acts)
        let (thread, about): (String, Option<HashSet<String>>) = match thing.map(str::trim) {
            None | Some("") => ("none".to_string(), None),
            Some(t) if docket_machine::parse_ticket(t).is_some() => match docket_machine::parse_ticket(t).filter(|n| self.tickets.m.tickets.contains_key(n)) {
                Some(_) => (t.to_string(), None),
                None => return err(format!("coord can: no ticket {t}")),
            },
            Some(t) => {
                let names: HashSet<String> = t.split(',').map(str::trim).filter(|n| !n.is_empty()).map(String::from).collect();
                ("none".to_string(), Some(names))
            }
        };
        let answers = self.with_norms(&thread, |norms, shown| Ok(norms.instances(shown, &actor)));
        match answers {
            Ok(list) => ok(list
                .into_iter()
                .filter(|a| about.as_ref().is_none_or(|names| a.objects.iter().any(|o| names.contains(o))))
                .map(|mut a| {
                    // the situation asks a thread as "t": the thing asked about
                    for o in a.objects.iter_mut().filter(|o| *o == "t") {
                        *o = thread.clone();
                    }
                    serde_json::to_string(&a).unwrap()
                })
                .collect::<Vec<_>>()
                .join("\n")),
            Err(e) => err(format!("coord can: the norms could not be asked: {e}")),
        }
    }

    /// Whether the norms enable ACT on ARGS (about THREAD): Ok, or why not. A policy without the act lets it through;
    /// one that cannot be read or solved does too, said on stderr, so a bad edit does not stop the team.
    pub(crate) fn permitted(&self, act: &str, args: &[&str], thread: &str) -> Result<(), String> {
        match self.norm_verdict(act, args, thread) {
            Ok(Some(v)) if !v.enabled => Err(v.why),
            Ok(_) => Ok(()),
            Err(e) => {
                eprintln!("coordd: norms: {act}({}): {e}; let through", args.join(", "));
                Ok(())
            }
        }
    }

    /// The models an agent of the project may run (`run-model`), the gateway's and his accounts' (`Router::served`),
    /// Haiku first, then by name (yaaihh's New agent).
    pub fn runnable_models(&self) -> Vec<String> {
        let mut out: Vec<String> = self.models.iter().map(|(id, _)| id.clone()).filter(|m| self.may_run(m).is_ok()).collect();
        out.sort_by_key(|m| (!m.contains("haiku"), std::cmp::Reverse(version(m)), m.clone()));
        out.dedup();
        return out;

        /// A haiku model's version: its number parts, a date (8 digits) left out, so claude-haiku-5-5 comes before
        /// claude-haiku-4-5-20251001 and claude-3-5-haiku-20241022; the other models keep name order.
        fn version(m: &str) -> Vec<u32> {
            if !m.contains("haiku") {
                return Vec::new();
            }
            m.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty() && p.len() < 8).filter_map(|p| p.parse().ok()).collect()
        }
    }

    /// The runnable models (`runnable_models`), each with what people guess it is where its maker is not named: read
    /// at each ask from the one `model-guesses.toml` at the flavor's root (the folder of the `projects` folder the
    /// project is in), never a copy, so an edit there shows at the next New agent. A link, a file over
    /// MODEL_GUESSES_CAP or one that is not TOML gives no guesses, and says why on coordd's stderr.
    pub fn model_list(&self) -> Vec<coord_types::harness::ModelEntry> {
        let guesses = self.model_guesses();
        self.runnable_models()
            .into_iter()
            .map(|id| {
                let g = guesses.get(&id);
                let field = |k: &str| g.and_then(|t| t.get(k)).and_then(|v| v.as_str()).map(str::to_string);
                coord_types::harness::ModelEntry { guess: field("guess"), read: field("read"), id }
            })
            .collect()
    }

    /// The flavor's `model-guesses.toml`, a table per model id; empty where there is none or it does not read.
    fn model_guesses(&self) -> toml::Table {
        let Some(path) = self.flavor_root().map(|f| f.join(MODEL_GUESSES)) else { return toml::Table::new() };
        match std::fs::symlink_metadata(&path) {
            Err(_) => return toml::Table::new(),
            Ok(m) if m.file_type().is_symlink() || m.len() > MODEL_GUESSES_CAP => {
                eprintln!("coordd: {} is a link or over {} KB: no guesses", path.display(), MODEL_GUESSES_CAP / 1024);
                return toml::Table::new();
            }
            Ok(_) => {}
        }
        std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| t.parse::<toml::Table>().map_err(|e| e.to_string()))
            .unwrap_or_else(|e| {
                eprintln!("coordd: {}: {e}", path.display());
                toml::Table::new()
            })
    }

    /// NAME's switches merged as `harness-settings` merges them; a file is parsed again only when its stamp changes.
    pub(crate) fn switches_view(&self, name: &str, kind: &str) -> coord_types::HarnessSwitchesView {
        let mut v = coord_types::HarnessSwitchesView { safety_check: !self.boxed(), ..Default::default() };
        if !coord_types::harness::HARNESSES.contains(&kind) {
            return v;
        }
        let project = self.project.as_ref().map(|p| p.dir.clone()).filter(|d| !d.is_empty()).map(std::path::PathBuf::from);
        let levels = [
            ("flavor", self.flavor_root().map(|f| f.join("harness.toml"))),
            ("project", project.map(|d| d.join("harness.toml"))),
            ("agent", Some(std::path::Path::new(&self.coord).join("agents").join(name).join("harness.toml"))),
        ];
        let mut cache = self.harness_files.lock().unwrap_or_else(|e| e.into_inner());
        for (level, path) in levels {
            let Some(path) = path else { continue };
            let stamp = std::fs::metadata(&path).ok().map(|m| (m.modified().unwrap_or(std::time::UNIX_EPOCH), m.len()));
            let fresh = cache.get(&path).is_some_and(|(s, _)| *s == stamp);
            if !fresh {
                let table = stamp.and_then(|_| std::fs::read_to_string(&path).ok()).and_then(|t| t.parse::<toml::Table>().ok());
                cache.insert(path.clone(), (stamp, table));
            }
            let Some(t) = cache.get(&path).and_then(|(_, t)| t.as_ref()).and_then(|t| t.get(kind)).and_then(|t| t.as_table()) else { continue };
            let list = |x: &toml::Value| x.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect::<Vec<_>>());
            if let Some(c) = t.get("safety_check").and_then(|x| x.as_bool()) {
                v.safety_check = c;
                v.from.insert("safety_check".into(), level.into());
            }
            if let Some(l) = t.get("tools_off").and_then(list) {
                v.tools_off = l;
                v.from.insert("tools_off".into(), level.into());
            }
            if let Some(l) = t.get("parts_off").and_then(list) {
                v.parts_off = l;
                v.from.insert("parts_off".into(), level.into());
            }
        }
        v
    }

    /// Whether the project runs its agents in a box (its project file's `boxed`).
    pub(crate) fn boxed(&self) -> bool {
        self.project.as_ref().is_some_and(|p| p.project.boxed)
    }

    /// `Request::HarnessSwitches`: refused unless the owner's and the harness can take each switch; written atomically.
    pub fn harness_switches(&mut self, who: &Who, name: &str, safety_check: Option<bool>, tools_off: Option<Vec<String>>, parts_off: Option<Vec<String>>) -> Reply {
        if !matches!(who, Who::Owner) {
            return err("coord harness: the owner's".into());
        }
        let Some(a) = self.agents.get(name) else { return err(format!("coord harness: no agent {name}")) };
        let kind = a.kind.clone();
        let can = capabilities(&kind, self.boxed());
        for (given, may, what) in [
            (safety_check.is_some(), can.safety_toggle, "safety_check (safety_toggle false)"),
            (tools_off.is_some(), can.tool_disabling, "tools_off (tool_disabling false)"),
            (parts_off.is_some(), can.prompt_parts_off, "parts_off (prompt_parts_off false)"),
        ] {
            if given && !may {
                return err(format!("coord harness: {name}'s harness ({kind}) cannot take {what}"));
            }
        }
        if safety_check.is_none() && tools_off.is_none() && parts_off.is_none() {
            return err("coord harness: no switch given".into());
        }
        let list = |v: Vec<String>| toml::Value::Array(v.into_iter().map(toml::Value::String).collect());
        let r = self.edit_switches(name, &kind, |table| {
            if let Some(c) = safety_check {
                table.insert("safety_check".into(), toml::Value::Boolean(c));
            }
            if let Some(t) = tools_off {
                table.insert("tools_off".into(), list(t));
            }
            if let Some(p) = parts_off {
                table.insert("parts_off".into(), list(p));
            }
        });
        if r.ok && safety_check.is_some() { self.check_restart(name, r) } else { r }
    }

    /// Reply R to a change of NAME's safety check, saying the restart it is due for it.
    fn check_restart(&mut self, name: &str, r: Reply) -> Reply {
        if !self.check_taken(Some(name)) {
            return r;
        }
        let busy = self.agents.get(name).is_some_and(|a| a.live.state == "working" && a.ended_turn != a.live.turn);
        ok(format!("{}; its safety check changes, so it restarts on its session {}", r.out, if busy { "when its turn ends" } else { "now" }))
    }

    /// The owner's switch KEY of NAME removed, so it takes the project's or the flavor's again.
    pub fn clear_switch(&mut self, who: &Who, name: &str, key: &str) -> Reply {
        if !matches!(who, Who::Owner) {
            return err("coord harness: the owner's".into());
        }
        let Some(kind) = self.agents.get(name).map(|a| a.kind.clone()) else { return err(format!("coord harness: no agent {name}")) };
        let r = self.edit_switches(name, &kind, |table| drop(table.remove(key)));
        if r.ok && key == "safety_check" { self.check_restart(name, r) } else { r }
    }

    /// NAME's `harness.toml`, its KIND table changed by EDIT, written atomically.
    fn edit_switches(&mut self, name: &str, kind: &str, edit: impl FnOnce(&mut toml::Table)) -> Reply {
        let dir = std::path::Path::new(&self.coord).join("agents").join(name);
        let file = dir.join("harness.toml");
        let mut doc: toml::Table = match std::fs::read_to_string(&file) {
            Ok(t) => match t.parse() {
                Ok(d) => d,
                Err(e) => return err(format!("coord harness: {} is not TOML ({e}); nothing written", file.display())),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => toml::Table::new(),
            Err(e) => return err(format!("coord harness: {}: {e}", file.display())),
        };
        let table = doc.entry(kind.to_string()).or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let Some(table) = table.as_table_mut() else { return err(format!("coord harness: {}'s {kind} is not a table; nothing written", file.display())) };
        edit(table);
        let text = match toml::to_string(&doc) {
            Ok(t) => t,
            Err(e) => return err(format!("coord harness: {e}")),
        };
        let tmp = dir.join(".harness.toml.new");
        if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&tmp, &text)).and_then(|_| std::fs::rename(&tmp, &file)) {
            let _ = std::fs::remove_file(&tmp);
            return err(format!("coord harness: {}: {e}", file.display()));
        }
        append(&self.path("registry.jsonl"), &json!({"ev": "harness", "name": name, "harness": kind, "at": now()}));
        self.changed(name);
        ok(format!("{name}'s {kind} switches written to {}; they take effect at its next start", file.display()))
    }

    /// The flavor's root: the folder of the `projects` folder the project's own folder is in (as the look's, `text_at`).
    fn flavor_root(&self) -> Option<std::path::PathBuf> {
        self.project.as_ref().map(|p| p.dir.as_str()).filter(|d| !d.is_empty()).and_then(crate::settings::flavor_root)
    }

    /// Whether the project lets an agent run MODEL (`MODEL:EFFORT` too): the norms' `run-model` (a policy without the act
    /// lets every model).
    pub(crate) fn may_run(&self, model: &str) -> Result<(), String> {
        let base = model.split(':').next().unwrap_or(model);
        self.permitted("run-model", &[base], "none")
    }

    /// The norms' verdict on ACT for ARGS about THREAD (`coord norm`, the owner's: why mail is refused).
    pub fn norm(&self, act: &str, args: &[String], thread: &str) -> Reply {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        match self.norm_verdict(act, &args, thread) {
            Ok(Some(v)) => ok(format!("{}: {}", if v.enabled { "enabled" } else { "not enabled" }, v.why)),
            Ok(None) => ok(format!("the policy has no act {act}: nothing rules on it")),
            Err(e) => err(format!("coord norm: {e}")),
        }
    }

    /// The registry's side of a rename: the agent under its new name, its children under it.
    fn renamed(&mut self, name: &str, to: &str) {
        if let Some(mut a) = self.agents.remove(name) {
            a.name = to.into();
            self.agents.insert(to.into(), a);
        }
        for a in self.agents.values_mut().filter(|a| a.parent == name) {
            a.parent = to.into();
        }
    }

    /// PID becomes agent NAME (its cgroup), watched by its pidfd from then on.
    pub fn run(&mut self, caller: &cgroup::Caller, name: &str, pid: i32) -> Result<(), String> {
        let a = self.agents.get(name).ok_or_else(|| format!("coord run: no agent {name}; coord add registers it first"))?;
        let who = match caller {
            cgroup::Caller::Owner => Who::Owner,
            cgroup::Caller::Agent(c) => Who::Agent(c.clone()),
            cgroup::Caller::Unclaimed => Who::Owner, // a terminal holder's program: only a registered, stopped agent (below)
            cgroup::Caller::Project => return Err("coord run: a project source cannot start agents".into()),
            cgroup::Caller::SignIn => return Err("coord run: a sign-in's program cannot start agents".into()),
        };
        if !self.above(&who, name) {
            return Err(format!("coord run: only the agents above {name} start it"));
        }
        if let Some(at) = &a.ended {
            return Err(format!("coord run: {name} was ended by the owner at {at}; `coord ended --undo {name}` takes that back"));
        }
        if let Some(p) = a.live.pid {
            if std::path::Path::new(&format!("/proc/{p}")).exists() {
                return Err(format!("coord run: {name} is running (pid {p})"));
            }
        }
        // its own limits (`[limits.NAME]`), on its cgroup before it is in it; a boxed one's were written with its box's
        let own = self.project.as_ref().map(|p| p.agent_limits(name, false)).unwrap_or_default();
        if own != crate::sandbox::Limits::default() {
            crate::sandbox::limit(name, &own).map_err(|e| format!("coord run: its limits: {e}"))?;
        }
        cgroup::delegate(name).and_then(|_| cgroup::place(pid, &cgroup::agent_cgroup(name))).map_err(|e| format!("coord run: cgroup: {e}"))?;
        let yolo = self.starts_in_yolo(name, pid);
        self.new_process(name, pid, yolo);
        let check = self.starts_check(name, pid);
        if let Some(a) = self.agents.get_mut(name) {
            a.live.check_runs = check;
        }
        Ok(())
    }

    /// Whether NAME's new process PID starts in the owner's Yolo (0.2 line 25): at its `coord run`, PID is still
    /// coordc, whose command line holds the session's whole one after `--`, the flag lib/route.sh `session_place` gave
    /// it included; read there, it is what runs, not what the record asked, also when he changed it between roster's
    /// read of session.json and this start. A Claude Code or agy agent (`takes_mode`) of a host session only: a box's
    /// sessions (COORD_IN_BOX, a boxed project) bypass prompts whatever he set, the box being the boundary, and have no
    /// such mode.
    pub(crate) fn starts_in_yolo(&self, name: &str, pid: i32) -> bool {
        !self.boxed_sessions()
            && self.agents.get(name).is_some_and(|a| takes_mode(&a.kind))
            && started_yolo(&std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default())
    }

    /// Whether NAME's new process PID starts with Claude Code's safety check on, read in its environment as
    /// `starts_in_yolo` reads its command line; None for another harness or an environment it cannot read.
    pub(crate) fn starts_check(&self, name: &str, pid: i32) -> Option<bool> {
        self.agents.get(name).filter(|a| a.kind == "claude")?;
        std::fs::read(format!("/proc/{pid}/environ")).ok().map(|e| started_check(&e))
    }

    /// Each running Claude Code agent whose switches give another safety check than its process runs with is due a
    /// restart on its session, as for a mode; NAMED, one whose check is not known is due one too, as its check was
    /// just set. Whether NAMED is due one now.
    pub fn check_taken(&mut self, named: Option<&str>) -> bool {
        let names: Vec<String> = self.agents.values().filter(|a| a.kind == "claude" && a.running()).map(|a| a.name.clone()).collect();
        for name in names {
            let want = self.switches_view(&name, "claude").safety_check;
            let a = self.agents.get_mut(&name).unwrap();
            let due = a.live.check_runs.map_or(named == Some(name.as_str()), |runs| runs != want);
            if due != a.live.check_due {
                a.live.check_due = due;
                self.changed(&name);
                self.poke.notify_one();
            }
        }
        named.is_some_and(|n| self.agents.get(n).is_some_and(|a| a.live.check_due))
    }

    /// This coord's sessions run in boxes: a coordd in a docker box (COORD_IN_BOX), or a boxed project's light
    /// sandbox (`box_agent`). Their sessions bypass permission prompts already (the owner, N-216.p1).
    fn boxed_sessions(&self) -> bool {
        std::env::var_os("COORD_IN_BOX").is_some_and(|v| !v.is_empty()) || self.project.as_ref().is_some_and(|p| p.project.boxed)
    }

    /// NAME's new process PID (`coord run`): its live state starts again, its turns counted from 0, so no turn end
    /// of its last process is taken for one of this one's (a restart on its conversation, as every move is: the
    /// first turn after it had its last process's last turn's number and was dropped as taken, its answer never
    /// formed; docket-try3's boxed Codex try, N-243).
    /// YOLO: it was started in the owner's Yolo (`starts_in_yolo`).
    pub(crate) fn new_process(&mut self, name: &str, pid: i32, yolo: bool) {
        let Some(a) = self.agents.get_mut(name) else { return };
        let awaiting_start = harness(&a.kind).then(std::time::Instant::now);
        // its last turn's end is the agent's, not the process's: a job that ended before it is not shown fresh again
        let turn_ended = a.live.turn_ended.take();
        a.live = Live { pid: Some(pid), state: "idle".into(), since: now(), started: now(), awaiting_start, compact_runs: a.compact, yolo_runs: yolo, turn_ended, ..Default::default() };
        a.ended_turn = u64::MAX;
        a.background = None;
        a.down_cut = false;
        // a new process's holder tells its own screen (`Live` starts with none)
        self.changed(name);
    }

    /// Merge INPUT's session facts into NAME's session (a key INPUT lacks keeps its value: the Codex socket roster
    /// gave), record it, and write agents/NAME/session.json, which `roster restart` reads (coordd its one writer).
    pub(crate) fn merge_session(&mut self, name: &str, input: &Value) {
        let a = self.agents.get_mut(name).unwrap();
        let mut session = a.session.as_object().cloned().unwrap_or_default();
        // the folder the session runs in is where its process started (startup, resume; roster's own), which is where
        // Claude Code keeps its transcript: a compaction or a /clear reports the shell's cwd at the time, wherever the
        // agent had gone, and `roster restart` resumed there and found no conversation (Tovrin's orch, N-64)
        let moved = matches!(input.get("source").and_then(Value::as_str), Some("compact" | "clear")) && session.get("cwd").is_some_and(|c| !c.is_null());
        for (k, v) in input.as_object().into_iter().flatten() {
            if k == "cwd" && moved {
                continue;
            }
            if ["session_id", "transcript_path", "cwd", "socket", "source"].contains(&k.as_str()) && !v.is_null() {
                session.insert(k.clone(), v.clone());
            }
        }
        a.session = Value::Object(session.clone());
        a.note_session(&self.sessions_moved);
        self.write_session(name);
        append(&self.path("registry.jsonl"), &json!({"ev": "session", "name": name, "session": Value::Object(session), "at": now()}));
    }

    /// Write agents/NAME/session.json from NAME's session, kind and model.
    fn write_session(&self, name: &str) {
        let a = &self.agents[name];
        let mut file = a.session.as_object().cloned().unwrap_or_default();
        file.insert("name".into(), json!(name));
        file.insert("kind".into(), json!(a.kind));
        file.insert("model".into(), json!(a.model));
        // the agent's own compaction point, which roster gives its next start (`compact`)
        file.remove("compact");
        if let Some(t) = a.compact {
            file.insert("compact".into(), json!(t));
        }
        // its mode, which roster gives its next start: only Yolo is written, with where it came from
        file.remove("mode");
        file.remove("mode_from");
        if a.yolo {
            file.insert("mode".into(), json!("yolo"));
        }
        if !a.mode_from.is_empty() {
            file.insert("mode_from".into(), json!(a.mode_from));
        }
        crate::jsonl::replace_later(&self.path(&format!("agents/{name}/session.json")), serde_json::to_vec(&file).unwrap());
    }

    /// NAME was set to MODEL with `/model` (its transcript), so a restart resumes it on MODEL, not the one it was added
    /// with (owner, 10 Nov: CONSULT, set to Fable 5.2 with `/model`, came back on Fable 5.1 at every restart; "if i
    /// change a model with /model it should never auto overwrite that"). Only a pick counts, not a reply's model: a
    /// fallback Claude Code makes on its own is no choice. An effort (`MODEL:EFFORT`, roster) stays.
    pub fn set_model(&mut self, name: &str, model: &str) {
        let Some(current) = self.agents.get(name).map(|a| a.model.clone()) else { return };
        let (old, effort) = match current.split_once(':') {
            Some((m, e)) => (m.to_string(), format!(":{e}")),
            None => (current.clone(), String::new()),
        };
        if model == old {
            return;
        }
        // a pick the project does not let it run is not kept: a restart goes back to the model it may run
        if let Err(why) = self.may_run(model) {
            eprintln!("coordd: {name} picked {model} with /model, which the policy does not allow ({why}); kept {old}");
            return;
        }
        let m = format!("{model}{effort}");
        self.agents.get_mut(name).unwrap().model = m.clone();
        append(&self.path("registry.jsonl"), &json!({"ev": "model", "name": name, "model": m, "at": now()}));
        self.write_session(name);
        self.changed(name);
    }

    /// `coord effort NAME LEVEL` (owner, 13 Nov: "we need a command for that", orch changing its agents' effort): NAME
    /// runs at LEVEL from now on. The owner's, or an agent's for the agents below it. LEVEL is one NAME's harness takes:
    /// Claude Code's `--effort` levels, or for a Codex agent those its model lists (CODEX: `codex debug models`, read off
    /// the lock; None for an agent of another kind). Its record at once, as `set_model` keeps a /model pick: the
    /// registry's model MODEL:LEVEL and session.json, so every later start runs at LEVEL. The running session takes it
    /// when not in a turn, and a turn is never stopped for it (`coord esc` first does that): a Claude Code agent is
    /// restarted on its own session (`to_restart`), as `/effort` typed into it would also save LEVEL as the owner's
    /// own default for that model (Claude Code 2.1.323); a Codex agent's next turn is started at LEVEL (`Job::Codex`).
    pub fn effort(&mut self, who: &Who, name: &str, level: &str, codex: Option<Result<Vec<String>, String>>) -> Reply {
        let name = &match self.not_across("effort", name) {
            Ok(n) => n,
            Err(e) => return err(e),
        };
        let name = name.as_str();
        let Some(a) = self.agents.get(name) else { return err(format!("coord effort: no agent named {name}")) };
        if !self.above(who, name) {
            return err(format!("coord effort: only the agents above {name} change its effort"));
        }
        let (model, old) = match a.model.split_once(':') {
            Some((m, e)) => (m.to_string(), Some(e.to_string())),
            None => (a.model.clone(), None),
        };
        let levels: Vec<String> = match (a.kind.as_str(), codex) {
            ("codex", Some(Ok(l))) => l,
            ("codex", Some(Err(why))) => return err(format!("coord effort: the levels {model} takes are not known: {why}")),
            (k, _) => match coord_types::harness::efforts(k) {
                Some(l) => l.iter().map(|l| l.to_string()).collect(),
                None => return err(format!("coord effort: {name} is {} agent; it has no effort", kind_article(k))),
            },
        };
        if !levels.iter().any(|l| l == level) {
            return err(format!("coord effort: {level} is not a level {name}'s model takes ({})", levels.join(", ")));
        }
        if old.as_deref() == Some(level) {
            return ok(format!("{name} already runs at {level}"));
        }
        let m = format!("{model}:{level}");
        let (running, busy, claude, back) = self.model_due(name, &m);
        let change = format!("{name}: effort {} → {level}", old.as_deref().unwrap_or("default"));
        ok(if !running {
            format!("{change} from its next start")
        } else if !claude {
            format!("{change} from its next turn")
        } else if back {
            format!("{change}; its session runs at {level} already, so it does not restart")
        } else if busy {
            format!("{change} when its turn ends (it restarts on its session; its background tasks end)")
        } else {
            format!("{change}; restarted on its session (its background tasks ended)")
        })
    }

    /// NAME's record takes MODEL:EFFORT M (`effort`, `model`): a running Claude Code session is restarted on it
    /// (`to_restart`), unless M is what it runs already (a second change while one is due: N-39 post 4). Whether it
    /// runs, is in a turn, is Claude Code's, and was set back.
    fn model_due(&mut self, name: &str, m: &str) -> (bool, bool, bool, bool) {
        let a = self.agents.get_mut(name).unwrap();
        let before = std::mem::replace(&mut a.model, m.to_string());
        let (running, busy, claude) = (a.running(), a.live.state == "working" && a.ended_turn != a.live.turn, a.kind == "claude");
        let mut back = false;
        if running && claude {
            let runs = a.live.effort_due.clone().unwrap_or(before);
            back = runs == m;
            a.live.effort_due = (!back).then_some(runs);
        }
        append(&self.path("registry.jsonl"), &json!({"ev": "model", "name": name, "model": m, "at": now()}));
        self.write_session(name);
        self.changed(name);
        self.poke.notify_one();
        (running, busy, claude, back)
    }

    /// `coord model NAME MODEL` (paper-research N-101 p1259 point 5, p1434): a Claude Code agent's model, by an agent
    /// above NAME or the owner, on `effort`'s path: recorded with the effort it runs at, and its session restarted on
    /// its conversation with `--model` once not in a turn. A `gpt-*` model would make it a Codex agent, which is a new
    /// agent, not a change: refused; so is a Codex agent's, set at its start. MODEL:EFFORT sets its effort with it, by one
    /// restart (the session panel's Apply, N-804.p7).
    pub fn model(&mut self, who: &Who, name: &str, model: &str) -> Reply {
        let name = &match self.not_across("model", name) {
            Ok(n) => n,
            Err(e) => return err(e),
        };
        let name = name.as_str();
        let Some(a) = self.agents.get(name) else { return err(format!("coord model: no agent named {name}")) };
        if !self.above(who, name) {
            return err(format!("coord model: only the agents above {name} change its model"));
        }
        if a.kind != "claude" {
            return err(format!("coord model: {name} is {} agent; its model is set at its start", kind_article(&a.kind)));
        }
        // MODEL:EFFORT sets both, by one restart (the session panel's one Apply, N-804.p7; MODEL_EFFORT_PROTOCOL)
        let (model, level) = match model.split_once(':') {
            Some((m, l)) => (m, Some(l)),
            None => (model, None),
        };
        if model.is_empty() || model.contains(':') || model.contains(char::is_whitespace) || kind_of_model(model) != "claude" {
            return err(format!("coord model: {model:?} is not a Claude model name (no gpt-*, no PROVIDER/MODEL; MODEL:EFFORT sets its effort too)"));
        }
        if let Some(l) = level.filter(|l| !CLAUDE_EFFORTS.contains(l)) {
            return err(format!("coord model: {l} is not an effort Claude Code takes ({})", CLAUDE_EFFORTS.join(", ")));
        }
        let (old, effort) = match a.model.split_once(':') {
            Some((m, e)) => (m.to_string(), Some(e.to_string())),
            None => (a.model.clone(), None),
        };
        let new_effort = level.map(str::to_string).or(effort.clone());
        if old == model && new_effort == effort {
            return ok(format!("{name} already runs on {}", a.model));
        }
        let m = new_effort.as_ref().map_or(model.to_string(), |e| format!("{model}:{e}"));
        let (running, busy, _, back) = self.model_due(name, &m);
        let mut moves = Vec::new();
        if old != model {
            moves.push(format!("model {old} → {model}"));
        }
        if new_effort != effort {
            moves.push(format!("effort {} → {}", effort.as_deref().unwrap_or("default"), new_effort.as_deref().unwrap_or("default")));
        }
        let change = format!("{name}: {}", moves.join(", "));
        ok(if !running {
            format!("{change} from its next start")
        } else if back {
            format!("{change}; its session runs on {m} already, so it does not restart")
        } else if busy {
            format!("{change} when its turn ends (it restarts on its session; its background tasks end)")
        } else {
            format!("{change}; restarted on its session (its background tasks ended)")
        })
    }

    /// The owner's Restart in the session panel (N-804.p9: his own words, "restart yourself so compaction can go to
    /// 600k", which roster refuses an agent below him): NAME's session restarted on its own conversation with its
    /// record (MODEL:EFFORT, its compaction point), on `effort`'s path: once not in a turn, a turn never cut. By the
    /// owner or an agent above NAME.
    pub fn restart(&mut self, who: &Who, name: &str) -> Reply {
        let name = &match self.not_across("restart", name) {
            Ok(n) => n,
            Err(e) => return err(e),
        };
        let name = name.as_str();
        let Some(a) = self.agents.get(name) else { return err(format!("coord restart: no agent named {name}")) };
        let (running, claude, busy) = (a.running(), a.kind == "claude", a.live.state == "working" && a.ended_turn != a.live.turn);
        if !running {
            return ok(format!("{name} does not run; its next start takes its record"));
        }
        if !claude {
            return ok(format!("{name} is {} agent: each turn starts afresh, so its next turn takes its record", kind_article(&a.kind)));
        }
        if !self.above(who, name) {
            return err(format!("coord restart: only the agents above {name} restart it"));
        }
        let a = self.agents.get_mut(name).unwrap();
        if a.live.effort_due.is_none() {
            a.live.effort_due = Some(a.model.clone());
        }
        self.changed(name);
        self.poke.notify_one();
        ok(if busy {
            format!("{name} restarts when its turn ends (on its session; its background tasks end)")
        } else {
            format!("{name}: restarted on its session (its background tasks ended)")
        })
    }

    /// `coord mode NAME safe|yolo` (0.2 line 25, N-1400; the owner: "add a button … to allow yolo or safety … if the
    /// setting is changed the underlying claude code is restarted with the new permission"): a Claude Code or agy
    /// agent's permission mode (`takes_mode`), the owner's only. Yolo: its session starts with Claude Code's own
    /// `--dangerously-skip-permissions` (roster, lib/route.sh `session_place`), or agy-rpc's, which agy asks nothing
    /// under (roster `agy_agent`, N-1956); safe: his own permission mode, as before, or agy asking coordd before each
    /// tool call. Kept as `effort` keeps a level:
    /// the registry (`mode`) and session.json at once, so every start keeps it: a restart for an effort, a model or an
    /// account, `roster restart`, coordd's own. A fork starts in safe: its session.json is its own (roster `fork`).
    /// The running session is restarted on its own conversation on `effort`'s path (`yolo_due`): now when not in a
    /// turn, at its turn's end in one, never cut; set back to the mode it runs in, nothing restarts. The view's `mode`
    /// is what the session was started with (`Live::yolo_runs`), changed only by a new process; `mode_due` what he
    /// asked while they differ. Asked again after a restart that did not happen, it is tried again.
    /// Refused from every agent (its cgroup, a sandboxed one's included, so an agent in Yolo cannot set another or
    /// itself), and where sessions run in boxes (COORD_IN_BOX, a boxed project), which bypass prompts already: the box
    /// is the boundary.
    /// Known limit, not this request's: a process an agent starts outside its cgroup is the owner to coordd (`cgroup::
    /// identify_path`), as for every owner request; the owner put malice out of scope (14 and 16 Nov).
    pub fn mode(&mut self, who: &Who, name: &str, mode: &str) -> Reply {
        if !matches!(who, Who::Owner) {
            return err("coord mode: an agent's Safe · Yolo is the owner's".into());
        }
        if self.boxed_sessions() {
            return err("coord mode: this coord's sessions run in boxes, which bypass prompts already (the box is the boundary)".into());
        }
        let yolo = match mode {
            "yolo" => true,
            "safe" => false,
            "" => return self.clear_mode(name),
            _ => return err(format!("coord mode: {mode} is not a mode: safe or yolo")),
        };
        let name = &match self.not_across("mode", name) {
            Ok(n) => n,
            Err(e) => return err(e),
        };
        let name = name.as_str();
        let Some(a) = self.agents.get(name) else { return err(format!("coord mode: no agent named {name}")) };
        if !takes_mode(&a.kind) {
            return err(format!("coord mode: {name} is a {} agent; Safe · Yolo is a Claude Code or agy agent's (its permission mode)", a.kind));
        }
        // what its session runs in (a stopped one: its next start's), against what it was set to before; a mode it
        // had by default becomes its own
        let (asked, runs, own) = (a.yolo, a.yolo_runs(), a.own_mode == Some(yolo) || a.mode_from.is_empty() && !yolo);
        if asked == yolo && runs == yolo {
            if own {
                return ok(format!("{name} is in {} already", mode_word(yolo)));
            }
            // in it by a default: now its own, so a change of the default leaves it
            let a = self.agents.get_mut(name).unwrap();
            (a.own_mode, a.mode_from) = (Some(yolo), "agent");
            append(&self.path("registry.jsonl"), &json!({"ev": "mode", "name": name, "mode": mode, "at": now()}));
            self.write_session(name);
            self.changed(name);
            return ok(format!("{name} is in {} already; it is its own now, whatever the default", mode_word(yolo)));
        }
        let (running, busy) = (a.running(), a.live.state == "working" && a.ended_turn != a.live.turn);
        let a = self.agents.get_mut(name).unwrap();
        (a.yolo, a.own_mode, a.mode_from) = (yolo, Some(yolo), "agent");
        if running {
            // due while the session runs in the other; asked again after a restart that did not happen, it is tried again
            a.live.yolo_due = runs != yolo;
        }
        if !own {
            append(&self.path("registry.jsonl"), &json!({"ev": "mode", "name": name, "mode": mode, "at": now()}));
            eprintln!("coordd: the owner set {name} to {}", mode_word(yolo));
            self.write_session(name);
        } else {
            eprintln!("coordd: the owner asked {name}'s {} again; its session runs in {}", mode_word(yolo), mode_word(runs));
        }
        self.changed(name);
        self.poke.notify_one();
        let change = if !own {
            format!("{name}: {} → {}", mode_word(asked), mode_word(yolo))
        } else {
            format!("{name}: {} again (its session still runs in {})", mode_word(yolo), mode_word(runs))
        };
        ok(if !running {
            format!("{change} from its next start")
        } else if runs == yolo {
            format!("{change}; its session runs in {} already, so it does not restart", mode_word(yolo))
        } else if busy {
            format!("{change} when its turn ends (it restarts on its session; its background tasks end)")
        } else {
            format!("{change}; restarted on its session (its background tasks ended)")
        })
    }

    /// NAME's own mode cleared, so it takes the default again (restarting into it as `resolve_modes` does).
    fn clear_mode(&mut self, name: &str) -> Reply {
        let Some(a) = self.agents.get_mut(name) else { return err(format!("coord mode: no agent named {name}")) };
        if a.own_mode.is_none() {
            return ok(format!("{name} has no mode of its own already"));
        }
        a.own_mode = None;
        append(&self.path("registry.jsonl"), &json!({"ev": "mode", "name": name, "mode": "", "at": now()}));
        self.resolve_modes();
        self.changed(name);
        let (yolo, from) = self.default_mode();
        ok(format!("{name} takes the default now: {}{}", mode_word(yolo), if from.is_empty() { String::new() } else { format!(", set for {}", if from == "project" { "this project" } else { "every project" }) }))
    }

    /// The default mode of an agent that has no mode of its own, and where it is set: the project file's
    /// `agent_mode`, else `yaaihh.toml`'s beside the coords (`project::wide_mode`), else Safe from nowhere.
    fn default_mode(&self) -> (bool, &'static str) {
        if let Some(y) = self.project.as_ref().and_then(|p| p.project.agent_mode.as_deref()).and_then(crate::project::mode_yolo) {
            return (y, "project");
        }
        crate::project::wide_mode(&self.coord).map_or((false, ""), |y| (y, "yaaihh"))
    }

    /// Every agent's mode from its own, else the default (`default_mode`), taken again after the project file or
    /// `yaaihh.toml` is read: one whose mode changes has its session.json written for its next start, and a running
    /// one is restarted into it once not in a turn, as for the owner's own `mode`.
    pub fn resolve_modes(&mut self) {
        let (wide, from) = self.default_mode();
        let changed: Vec<String> = self
            .agents
            .values_mut()
            .filter(|a| takes_mode(&a.kind))
            .filter_map(|a| {
                let (yolo, mode_from) = a.own_mode.map_or((wide, from), |own| (own, "agent"));
                if (a.yolo, a.mode_from) == (yolo, mode_from) {
                    return None;
                }
                (a.yolo, a.mode_from) = (yolo, mode_from);
                if a.running() {
                    a.live.yolo_due = a.live.yolo_runs != yolo;
                }
                Some(a.name.clone())
            })
            .collect();
        for name in &changed {
            let a = &self.agents[name];
            eprintln!("coordd: {name}: its mode is {} ({})", mode_word(a.yolo), if a.mode_from.is_empty() { "nothing set" } else { a.mode_from });
            self.write_session(name);
            self.changed(name);
        }
        if !changed.is_empty() {
            self.poke.notify_one();
        }
    }

    /// NAME, just registered, takes its mode by default, in its session.json before roster, which registered it,
    /// starts it.
    fn mode_at_add(&mut self, name: &str) {
        if self.agents.get(name).is_some_and(|a| takes_mode(&a.kind)) {
            self.resolve_modes();
            if !self.agents[name].mode_from.is_empty() {
                crate::jsonl::flush();
            }
        }
    }

    /// The Claude Code agents to restart now for the effort their record names (`effort`) or for their account:
    /// running, not in a turn, nothing on its way to them, no restart running for them; and those that do not run
    /// because their start found no account that could take them (`account_wait`), which are started. Each is counted
    /// as restarting until `restarted`.
    pub fn to_restart(&mut self) -> Vec<(String, Option<i32>)> {
        // one that waited for an account and whose own is back goes on as it is (accounts/mover.rs)
        self.account_back();
        // a key account out for its rate limit is taken again when the policy says (accounts/mover.rs)
        self.key_again();
        // one that went on through the gateway returns when an account can take it again
        self.gateway_back();
        let now = crate::accounts::mover::now_s();
        let names: Vec<(String, Option<i32>)> = self
            .agents
            .values()
            .filter(|a| if a.running() { a.live.restart_due() && !(a.live.state == "working" && a.ended_turn != a.live.turn) } else { a.native && a.account_wait.is_some() })
            // one that found every account out, or the accounts' lock held, is not tried again before its time
            // (`account_for`)
            .filter(|a| a.account_wait.is_none_or(|t| t <= now))
            .filter(|a| !self.inflight.contains_key(&a.name) && !self.restarting.contains(&a.name))
            .map(|a| (a.name.clone(), a.live.pid))
            .collect();
        for (n, _) in &names {
            // the wait is over with this try: one found on it afterwards is the try's own (`restarted`)
            self.agents.get_mut(n).unwrap().account_wait = None;
        }
        self.restarting.extend(names.iter().map(|(n, _)| n.clone()));
        // each one's view says a restart runs (`AgentView.restarting`: the window holds his message box meanwhile)
        for (n, _) in &names {
            self.changed(n);
        }
        names
    }

    /// NAME's restart for its effort or its account (`Job::Restart`) is over: a new process took it (its `Live`
    /// starts without the marks). Still PID, it was not restarted: the marks go, as it would otherwise be tried again
    /// and again; its record keeps the level, which its next start takes, and the log says why. Not so when the try
    /// was refused for want of an account (`Core::account_for_end` set `account_wait`, which `to_restart` had taken
    /// away before the try): it stays due and is tried again then.
    pub fn restarted(&mut self, name: &str, pid: Option<i32>, why: &str) {
        // its view no longer says a restart runs
        if self.restarting.remove(name) && self.agents.contains_key(name) {
            self.changed(name);
        }
        let Some(a) = self.agents.get_mut(name) else { return };
        if a.account_wait.is_none() {
            if a.live.restart_due() && a.live.pid == pid {
                if a.live.effort_due.take().is_some() {
                    eprintln!("coordd: {name}: not restarted for its effort {} ({why}); it runs at it from its next start", a.model);
                }
                if std::mem::take(&mut a.live.yolo_due) {
                    eprintln!("coordd: {name}: not restarted for its mode {} ({why}); it runs in {} until its next start", mode_word(a.yolo), mode_word(a.live.yolo_runs));
                }
                if std::mem::take(&mut a.live.check_due) {
                    eprintln!("coordd: {name}: not restarted for its safety check ({why}); it runs with the one it had until its next start");
                }
                if let Some(due) = a.live.account_due.take() {
                    eprintln!("coordd: {name}: not restarted for its account ({due}): {why}");
                }
                // its fork's message goes as a `deliver`'s, to the session it runs (`fork_now`)
                if let Some(cut) = a.live.fork_due.take() {
                    eprintln!("coordd: {name}: fork at {} delivered as deliver: not restarted on its cut copy ({why})", cut.place);
                    self.forked.insert(name.to_string(), delivery::Forked { place: cut.place, words: None });
                }
                if let Some(due) = a.live.system_due.take() {
                    eprintln!("coordd: {name}: not restarted with system prompt {} ({why}); it runs with the one it had", due.version);
                }
                self.changed(name);
            } else if a.native && !a.running() && !why.is_empty() {
                // it does not run and is not tried again: said, as nothing else would
                eprintln!("coordd: {name}: not started on an account: {why}");
                self.changed(name);
            }
        }
        self.poke.notify_one();
    }

    /// NAME's restart with system prompt VERSION (`Job::Restart`, `Live::system_due`) is over: WHY, roster's own words
    /// when it failed (empty: it applied). Kept on the agent and in its view (`AgentView::apply_error`) until one
    /// applies, so the owner reads why his edit did not take, not only coordd's log.
    pub fn system_applied(&mut self, name: &str, version: u32, why: &str) {
        let Some(a) = self.agents.get_mut(name) else { return };
        let now = (!why.is_empty()).then(|| format!("system prompt {version} not applied: {why}"));
        if a.apply_error != now {
            a.apply_error = now;
            self.changed(name);
        }
    }

    /// `coord session NAME JSON`: session facts from an agent above NAME (roster, for a Codex thread).
    pub fn session(&mut self, who: &Who, name: &str, input: &Value) -> Reply {
        if !self.agents.contains_key(name) {
            return err(format!("coord session: no agent {name}"));
        }
        if !self.above(who, name) {
            return err(format!("coord session: only the agents above {name} set it"));
        }
        self.merge_session(name, input);
        // `ready`: a Codex thread with no turn yet (New agent with no first words), whose SessionStart comes only with
        // its first turn: what is due goes to it now, not after START_WAIT
        if input.get("ready").and_then(Value::as_bool) == Some(true) {
            self.agents.get_mut(name).unwrap().live.awaiting_start = None;
        }
        self.changed(name);
        ok(String::new())
    }

    /// Agent NAME's process PID has exited.
    pub fn exited(&mut self, name: &str, pid: i32) {
        let Some(a) = self.agents.get_mut(name) else { return };
        if a.live.pid != Some(pid) {
            return;
        }
        a.live.pid = None;
        a.live.tools.clear();
        a.live.pull = None;
        a.live.pushed.clear();
        // one that ends while it waits for an account is not started again by coordd: only a start that was refused is
        a.account_wait = None;
        let kind = a.kind.clone();
        // what the process was given and never put into its model's input went with it: due again (0.2 A4)
        self.ticket_lost_with_process(name);
        self.ticket_turn_cut(name); // what its cut turn was given is forgotten, for Reply (docket-core)
        self.set_state(name, "stopped");
        self.changed(name);
        // liveness (supervision-nets §Shape, coordd 4): a process that ends is an event, for the bindings on it
        self.ticket_event("agent_exit", &json!({"name": name, "pid": pid, "kind": kind, "at": now()}));
    }

    // ---- hooks -------------------------------------------------------------------------------------------------

    /// A hook event of agent NAME. A Claude Code agent's turns, tool calls and mail are read from its transcript
    /// (`main::follow`): its hooks wake that reader and name the session (`start`), and change nothing else; an API
    /// error's reason (`failed`) is kept for its idle notice. A Codex agent's turns still follow its hooks.
    pub fn hook(&mut self, name: &str, event: &str, input: &Value) -> HookNext {
        if !self.agents.contains_key(name) {
            return HookNext::Done(String::new());
        }
        self.poke.notify_one();
        // an edit by one of Claude Code's own tools is the agent's act `edit` (runner.rs; N-101)
        if event == "post" {
            self.act_of_hook(name, input);
        }
        let claude = !served(&self.agents[name].kind);
        // what the event says of the account the session runs on: its windows, or that it is out (accounts/limits.rs);
        // written off this lock
        if let Some(keep) = self.account_said(name, event, input) {
            self.account_keep(keep);
        }
        // a turn ended by its Stop (on an API error Claude Code runs StopFailure, `failed`, in its place): a rate limit
        // that names no reset waits from key-again-after again (`Live::limit_wait`)
        if event == "stop" {
            self.agents.get_mut(name).unwrap().live.limit_wait = None;
        }
        match event {
            // a compaction begins: shown until it is over (`Live::compacting`)
            "compact" => {
                let live = &mut self.agents.get_mut(name).unwrap().live;
                let began = !live.compacting;
                (live.compacting, live.compacting_since) = (true, Some(now()));
                self.changed(name);
                // the brain's trigger (0.3; proto-mind N-1446): once per compaction, for the bindings on it
                if began {
                    self.ticket_event(docket_machine::syntax::COMPACTING, &json!({"agent": name}));
                }
                HookNext::Done(String::new())
            }
            "start" => {
                self.agents.get_mut(name).unwrap().live.compacting = false;
                // a /clear ends the turn it was in, in the same process; a startup or resume follows a new process, whose
                // exit cut already, and a cut then would forget what the new session was handed (N-66/N-67, m1005)
                if input.get("source").and_then(Value::as_str) == Some("clear") {
                    self.ticket_turn_cut(name);
                }
                self.merge_session(name, input);
                self.account_session(name);
                let a = self.agents.get_mut(name).unwrap();
                a.live.awaiting_start = None; // its session is ready to be handed what is due
                if let Some(t) = a.transcript() {
                    a.background = Some(Background::new(&t, &a.live.started).found_running());
                }
                if self.agents[name].live.state != "working" {
                    self.set_state(name, "idle");
                }
                self.changed(name);
                HookNext::Done(String::new())
            }
            "prompt" => {
                if claude {
                    self.hook_busy(name, true);
                }
                // a new prompt is past any compaction: one Claude Code refused ("Not enough messages to compact") ran its
                // PreCompact hook and sends no SessionStart (docket-try3, 17 Nov; paper-research N-101 p1445)
                self.compaction_over(name);
                // held mail goes in by one way only, the channel after the turn (`main::follow`)
                if !claude {
                    self.begin_turn(name);
                    self.changed(name);
                }
                HookNext::Done(String::new())
            }
            // a Claude Code agent's turns and tool calls are its transcript's (`main::follow`): its hooks only wake the
            // reader (above), so a hook costs nothing under the lock (design coordd-core-2026-11-10 §3)
            // the one hook that decides: Claude Code's Agent tool, by the team policy (vault designs/teams-and-tiers-2026-11-10.md)
            "pre" if claude && matches!(input["tool_name"].as_str(), Some("Agent" | "Task")) => HookNext::Done(self.subagent_check(name, input)),
            "stop" if claude => {
                // the Stop of a turn the transcript has already shown followed by the next: that one runs on
                let owed = self.agents.get_mut(name).map(|a| std::mem::take(&mut a.stop_owed)).unwrap_or(false);
                if !owed {
                    self.hook_busy(name, false);
                }
                HookNext::Done(String::new())
            }
            "pre" | "post" if claude => HookNext::Done(String::new()),
            "pre" => {
                let id = s(input, "tool_use_id");
                let ti = input.get("tool_input").cloned().unwrap_or(Value::Null);
                let what = ["command", "file_path", "pattern", "description", "prompt"]
                    .iter()
                    .find_map(|k| ti.get(*k).and_then(Value::as_str))
                    .unwrap_or("")
                    .chars()
                    .take(200)
                    .collect();
                if !claude && self.agents[name].live.state != "working" {
                    // A tool call starts a turn only when it is the agent's own (in its transcript). Claude Code also runs tools outside any turn after one ends (8 Nov: DEBUG and LAZY
                    // stayed "working" for hours on a ScheduleWakeup and a `true` that no transcript holds, likely
                    // its run-ahead of the suggested next prompt), with no turn end to follow, so their mail was held.
                    // A subagent's tool (`agent_id`) outside a turn is a background agent's: the agent itself waits on
                    // it ("waiting on ..."), which takes mail like the end of a turn (owner m191, 9 Nov); a foreground
                    // subagent runs inside its parent's turn, which is already "working".
                    let own = input.get("agent_id").is_none();
                    if !own {
                        if input.get("agent_id").is_some() {
                            return HookNext::Done(String::new());
                        }
                        eprintln!("coordd: {name}: tool {} ({}) outside a turn and not in its transcript; no turn starts", s(input, "tool_name"), id);
                        return HookNext::Done(String::new());
                    }
                    self.begin_turn(name);
                }
                let a = self.agents.get_mut(name).unwrap();
                let at = now();
                a.live.last_tool = Some(at.clone());
                if !id.is_empty() {
                    a.live.tools.insert(id.clone(), Tool { id, tool: s(input, "tool_name"), what, about: crate::follow::about(&ti), at });
                }
                self.changed(name);
                HookNext::Done(String::new())
            }
            "post" => {
                let id = s(input, "tool_use_id");
                self.agents.get_mut(name).unwrap().live.tools.remove(&id);
                self.changed(name);
                HookNext::Done(String::new())
            }
            "failed" => {
                // the turn ended on an API error (Claude Code's StopFailure; Stop does not run then): it ended all the
                // same, and its parent is told why; on a rate limit not its account's, it is given its turn again
                let why = [s(input, "error"), s(input, "error_details")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(": ");
                let (limit, wait) = self.limit_of(name, input);
                let a = self.agents.get_mut(name).unwrap();
                a.live.tools.clear();
                a.failed = Some(if why.is_empty() { "an API error".into() } else { why.chars().take(300).collect() });
                (a.live.limit, a.live.limit_wait) = (limit, wait);
                if claude {
                    self.hook_busy(name, false);
                }
                if claude { HookNext::Done(String::new()) } else { HookNext::CodexStop }
            }
            "stop" => {
                self.agents.get_mut(name).unwrap().live.tools.clear();
                HookNext::CodexStop // the Codex turn still runs at Stop: coordd waits for its end first
            }
            // its status line (agent-base/claude/statusline): the model's whole window
            "statusline" => {
                self.statusline_context(name, input);
                HookNext::Done(String::new())
            }
            _ => HookNext::Done(String::new()),
        }
    }

    /// Agent NAME's turn ended (its transcript says so, or Codex's app-server): taken once per turn. Its turn number,
    /// and the background work still running; None when this turn's end was taken already. The waits and the state
    /// follow (`main::turn_waits`); its output goes to the bindings from `Core::ticket_turn_ended`.
    pub fn end_turn(&mut self, name: &str) -> Option<(u64, Vec<String>)> {
        let a = self.agents.get_mut(name)?;
        if a.ended_turn == a.live.turn {
            return None;
        }
        a.ended_turn = a.live.turn;
        a.live.turn_ended = Some(now());
        a.live.tools.clear();
        a.live.compacting = false;
        let bg = a.background.as_mut().map(|b| b.still_running()).unwrap_or_default();
        // the step it owes is silent from here, not from when it was given (ticket/quiet.rs)
        self.tickets.quiet.turn_ended(name, &now());
        let turn = a.live.turn;
        // its account may be out by now: the rule for a move is asked between turns (accounts/mover.rs)
        self.account_turn_ended(name);
        Some((turn, bg))
    }

    /// What one read of NAME's transcript showed, taken in its order (`main`'s follow loop): the deliveries seen, the
    /// words typed, a cut (CUT, the interrupt's offset) and each ended turn's final output (OUTPUT, oldest first), each
    /// at its place. A turn's end that a next turn followed in the same read (ENDED false after it, or a later mark)
    /// is also coordd's: that turn ended, the next begun; those ends are returned for `turn_waits`.
    pub fn transcript_read(&mut self, name: &str, at: Vec<(String, String, u64, u64)>, bare: Vec<(String, String, u64, u64)>, typed: Vec<docket_machine::Loc>, output: &[(String, u64, u64)], cut: Option<u64>, begins: &[u64], running_old: bool) -> Vec<(String, (u64, Vec<String>))> {
        self.transcript_read_queued(name, at, bare, typed, output, cut, begins, running_old, Vec::new(), &[])
    }

    /// [`Core::transcript_read`], with what the session's queue showed (0.2 A1, N-1397 post 5): ENTERED, the deliveries
    /// that entered the model's input, each taken in its place among the rest (`Core::ticket_delivered`); QUEUED, at
    /// each turn's end, the places of what was still queued then, which are the next turn's (`Core::ticket_turn_ended`).
    #[allow(clippy::too_many_arguments)]
    pub fn transcript_read_queued(&mut self, name: &str, at: Vec<(String, String, u64, u64)>, bare: Vec<(String, String, u64, u64)>, typed: Vec<docket_machine::Loc>, output: &[(String, u64, u64)], cut: Option<u64>, begins: &[u64], running_old: bool, entered: Vec<(crate::follow::Entered, String)>, queued: &[(u64, Vec<(u64, u64)>)]) -> Vec<(String, (u64, Vec<String>))> {
        let mut entered = entered;
        let mut ends = Vec::new();
        // the turn coordd counts as running began before the follower (a start's window): its first entry is seen, so
        // a begin after it is a next turn and its Stop ends it (consult-3, N-412.p1690)
        if running_old && let Some(a) = self.agents.get_mut(name).filter(|a| !served(&a.kind) && a.live.state == "working" && a.ended_turn != a.live.turn) {
            a.begin_seen = true;
        }
        // what was seen before a turn cut short (an esc's interrupt), then the cut, then what came after
        // (docket-core, docket-edge m1153); and the same at each turn's end in this read: what was seen
        // before it, its final output (the rule Reply, a step's answer), then the turn a queued delivery
        // began at once, taken as begun, then what came after (consult-3, N-807.p61: a turn's end and the
        // next turn's prompt in one read lost the output, and the delivery counted for the old turn)
        // 0: a turn's end with its output, 1: a cut, 2: a turn begun after an end (by a prompt, a queued delivery or a
        // notice: none of them may run a prompt hook)
        let mut marks: Vec<(u64, u8, Option<(String, u64, u64)>)> = output.iter().map(|o| (o.1 + o.2, 0, Some(o.clone()))).collect();
        marks.extend(cut.map(|c| (c, 1, None)));
        marks.extend(begins.iter().map(|b| (*b, 2, None)));
        marks.sort_by_key(|m| (m.0, m.1));
        let (mut at, mut bare, mut typed) = (at, bare, typed);
        for k in 0..=marks.len() {
            let upto = marks.get(k).map(|m| m.0);
            let before = |o: u64| upto.is_none_or(|u| o < u);
            let (now_at, rest_at): (Vec<_>, Vec<_>) = std::mem::take(&mut at).into_iter().partition(|(_, _, o, _)| before(*o));
            let (now_bare, rest_bare): (Vec<_>, Vec<_>) = std::mem::take(&mut bare).into_iter().partition(|(_, _, o, _)| before(*o));
            (at, bare) = (rest_at, rest_bare);
            // a ticket delivery (`C-12.s3`) seen in the transcript: its place is where the text it was given is
            for (id, file, offset, len) in now_at {
                let loc = docket_machine::Loc { file, offset, len };
                for id in self.entry_is(name, &[id], None, &loc) {
                    self.ticket_seen(name, &id, loc.clone());
                }
            }
            // an id-less coord entry (the owner's words to an agent holding only them): the delivery handed
            // before it, by position, before the turn's end is taken below
            for (body, file, offset, len) in now_bare {
                self.bare_seen(name, &body, docket_machine::Loc { file, offset, len });
            }
            // what entered the model's input before the mark: delivered, after its place was found above
            let (now_entered, rest_entered): (Vec<_>, Vec<_>) = std::mem::take(&mut entered).into_iter().partition(|(e, _)| before(e.entry));
            entered = rest_entered;
            for (e, file) in now_entered {
                let place = docket_machine::Loc { file, offset: e.place.0, len: e.place.1 };
                let ids: Vec<String> = (!e.id.is_empty()).then(|| e.id.clone()).into_iter().collect();
                for id in self.entry_is(name, &ids, None, &place) {
                    self.ticket_delivered(name, &id, place.clone(), &e.at, e.began);
                }
            }
            // words typed into its terminal: the owner's post, given in this turn (paper-research N-101 p1107)
            let (now_typed, rest_typed): (Vec<_>, Vec<_>) = std::mem::take(&mut typed).into_iter().partition(|l: &docket_machine::Loc| before(l.offset));
            typed = rest_typed;
            for loc in now_typed {
                self.typed(name, loc);
            }
            match marks.get(k) {
                None => {}
                Some((_, 1, _)) => {
                    // an esc typed in its own terminal comes only as this cut, never through `interrupted`: a 429's
                    // doubled wait back at key-again-after all the same (proto-reader N-1397 p163)
                    if let Some(a) = self.agents.get_mut(name) {
                        a.live.limit_wait = None;
                    }
                    self.ticket_turn_cut(name)
                }
                Some((_, 2, _)) => {
                    // the next turn runs: coordd's count ends the one before, if its Stop has not yet come, and begins
                    // this one; the hooks are told it runs, and that old Stop, still to come, ends nothing
                    if !served(&self.agents[name].kind) && self.agents[name].running() {
                        let a = &self.agents[name];
                        let (running, seen, stop_to_come) = (a.live.state == "working" && a.ended_turn != a.live.turn, a.begin_seen, a.live.hook_busy == Some(true));
                        if running && !seen {
                            // the first entry of the turn coordd already counts as begun (its prompt hook came first)
                        } else {
                            if running {
                                if let Some(e) = self.end_turn(name) {
                                    ends.push((name.to_string(), e));
                                }
                                self.agents.get_mut(name).unwrap().stop_owed = stop_to_come;
                            }
                            self.begin_turn(name);
                            self.hook_busy(name, true);
                        }
                        self.agents.get_mut(name).unwrap().begin_seen = true;
                    }
                }
                // the turn's final output (the transcript's end_turn message): it answers the step the turn
                // owes, if it owes one (owner m613). Taken here, not at end_turn: a Claude Code turn mostly
                // ends by its Stop hook, before or after this read, and the dev copy's first triage answer
                // was lost that way
                Some((end, _, Some((file, offset, len)))) => {
                    // what the session still held queued when this turn ended: given, not in its input (0.2 A1)
                    let still: Vec<docket_machine::Loc> = queued.iter().filter(|(e, _)| e == end).flat_map(|(_, q)| q.iter().map(|(o, l)| docket_machine::Loc { file: file.clone(), offset: *o, len: *l })).collect();
                    self.ticket_turn_ended_queued(name, docket_machine::Loc { file: file.clone(), offset: *offset, len: *len }, &still);
                }
                Some(_) => {}
            }
        }
        ends
    }

    /// NAME's turn's end taken by coordd with no stop in its transcript read yet (`main`'s follow loop), SPAN the text
    /// said since its last prompt. After a Stop hook the transcript's stop record comes, with the text written up to
    /// it, and is taken then, never an earlier message (consult-3, N-807.p69, 1); with no Stop hook (the hooks gone
    /// stale, the screen) none will come, so SPAN is its final output, taken once.
    pub fn turn_end_without_stop(&mut self, name: &str, span: Option<docket_machine::Loc>) {
        if self.agents.get(name).is_some_and(|a| a.live.hook_busy == Some(false)) {
            return;
        }
        if let Some(loc) = span {
            self.ticket_turn_ended(name, loc);
            self.output_read(name);
        }
    }

    /// NAME's transcript, at its follower's first read after a start, ends with a turn's end (END, that turn's final
    /// output: `follow::Follower::take_start_end`), a turn that ended while coordd was stopped (paper-research N-1397
    /// p43). The output is given back, to be taken as at any turn's end (`transcript_read_queued`), when the record
    /// still holds that turn (`turn_holds`: something given before the output, after the replay and `load_tickets`' cut
    /// of a process gone); a start after a turn's end already taken posts nothing again. coordd's own count of the turn
    /// ends by its rule (`presence_agrees`: the hooks' turn, whose Stop hook found no coordd, is over once the screen
    /// has shown none and the transcript its end, unchanged, for 60 s; consult-3, N-1397 p53, 1).
    pub fn start_output(&self, name: &str, end: Option<docket_machine::Loc>) -> Option<(String, u64, u64)> {
        let loc = end?;
        (!self.turn_holds(name, &loc, &[]).0.is_empty()).then_some((loc.file, loc.offset, loc.len))
    }

    /// NAME's turn end was taken before its transcript showed it (a Claude Code agent's Stop hook, or its screen):
    /// nothing is handed to it until the transcript has (`Agent::output_owed`, `Core::output_read`).
    pub fn owe_output(&mut self, name: &str) {
        if let Some(a) = self.agents.get_mut(name).filter(|a| a.kind == "claude") {
            a.output_owed = Some(std::time::Instant::now());
        }
    }

    /// NAME's transcript has shown its turn's end: its final output read, its end with none, or a cut.
    pub fn output_read(&mut self, name: &str) {
        if let Some(a) = self.agents.get_mut(name) {
            a.output_owed = None;
        }
    }

    /// A correction the transcript made to what the hooks said (or did not), logged to count how often they miss.
    pub fn reconciled(&self, name: &str, what: &str) {
        eprintln!("coordd: reconcile {name}: {what}");
        append(&self.path("reconcile.jsonl"), &json!({"at": now(), "agent": name, "what": what}));
    }

    /// The second half: the state the turn ended in, and "X idle" to the parent when it waits on nothing.
    pub async fn turn_end_state(&mut self, name: &str, turn: u64, waits: Vec<String>) {
        if self.agents.get(name).map(|a| (a.live.turn, a.running())) != Some((turn, true)) {
            return; // a new turn began meanwhile, or the agent stopped
        }
        self.agents.get_mut(name).unwrap().bg_waits = waits.clone();
        self.settle_waits(name, waits).await;
    }

    /// What agent NAME waits on in the tickets (plan/edge.md S4: derived, no waits source): each open ticket it opened
    /// that waits on someone else, as `C-25 (fixer: repair, F-26)`. A ticket that waits on NAME itself is work it
    /// owes, not a wait.
    pub fn ticket_waits(&self, name: &str) -> Vec<String> {
        use docket_machine::ViewStatus;
        let mut out = Vec::new();
        // only the tickets NAME opened (`ticket_view_ids`): building every shown ticket's view for each agent held the
        // lock 2.4 s per refresh with 9 agents on research's record (a2h-speed, N-459). And only the machine's view of
        // each: its status and waits are the machine's; the shown view's mentions, prompts and unsent posts (40 ms a
        // ticket on research's record) are not read here, and each ticket change queued this for every waiting agent
        // ('job waits' held the lock 1-1.6 s, dozens of times an hour after E1's switch). A closed one is not
        // built at all: it waits on nobody
        for v in self.ticket_view_ids(|x| x.is_open() && x.opener.as_deref().or(x.raised_by.as_deref()) == Some(name)).into_iter().filter_map(|t| self.tickets.m.view(t)) {
            if v.opener.as_deref() != Some(name) || matches!(v.status, ViewStatus::Done { .. } | ViewStatus::Failed { .. }) {
                continue;
            }
            // a hold (an item's close) is owed but waited on by nobody: its opener does not wait on it (docket-core m809)
            let held = |w: &&docket_machine::WaitsOn| {
                docket_machine::parse_ticket(&w.ticket)
                    .and_then(|t| self.tickets.m.tickets.get(&t))
                    .is_some_and(|x| matches!(&x.status, docket_machine::Status::Awaits { owed, .. } if owed.hold))
            };
            let waits: Vec<&docket_machine::WaitsOn> = v.waits_on.iter().filter(|w| !held(w)).collect();
            if waits.is_empty() || waits.iter().any(|w| w.agent == name) {
                continue;
            }
            let on: Vec<String> = waits.iter().map(|w| format!("{}: {}, {}", w.agent, w.call, w.ticket)).collect();
            out.push(format!("{} ({})", v.id, on.join("; ")));
        }
        out
    }

    /// The tickets changed: every running agent shown waiting, or idle, restated from its background work and its
    /// tickets; one left waiting on nothing goes idle, with its parent's notice, as at a turn end. An agent in a turn
    /// is left to its turn end.
    pub async fn refresh_waits(&mut self) {
        let out_of_turn: Vec<String> = self
            .agents
            .values()
            .filter(|a| a.running() && (a.live.state.starts_with("waiting on ") || a.live.state == "idle"))
            .map(|a| a.name.clone())
            .collect();
        for name in out_of_turn {
            let a = &self.agents[&name];
            let now_waits: Vec<String> = a.bg_waits.iter().cloned().chain(self.ticket_waits(&name)).collect();
            // an idle agent that still waits on nothing stays as it is: no second idle notice
            if a.live.state == "idle" && now_waits.is_empty() {
                continue;
            }
            // the waits just read are the ones settled: not read a second time (N-459)
            self.settled(&name, now_waits).await;
        }
    }

    /// NAME's turn ended on an API error WHY while it holds a step: the first time it is handed again as before; the
    /// second time on the same step the step is given up (`Input::ModelRefused`, NoWorker), which stops its ticket and
    /// goes to the lead as any stop does, instead of standing with nobody holding it.
    fn api_refused(&mut self, name: &str, why: &str) {
        let held = self.tickets.m.tickets.values().find_map(|x| match &x.status {
            docket_machine::Status::Awaits { agent, owed, .. } if agent == name => Some((x.t, owed.step)),
            _ => None,
        });
        let Some((t, step)) = held else {
            self.api_failed.remove(name);
            return;
        };
        if self.api_failed.get(name) != Some(&(t, step)) {
            self.api_failed.insert(name.to_string(), (t, step));
            return;
        }
        self.api_failed.remove(name);
        let why = format!("its turn ended on an API error twice ({why})");
        if let Err(e) = self.ticket_input(docket_machine::Input::ModelRefused { t, step, agent: name.to_string(), why }) {
            eprintln!("coordd: {name}: its step given up after two API errors: {e}");
        }
    }

    /// What NAME's `failed` hook INPUT says of a rate limit that is not its account's (`Live::limit`): when its words
    /// name the reset (the gateway's `(reset after 18m48s)`, `limits::reset_after`; Codex's `try again at Nov 27th, 2026
    /// 11:52 AM`, `limits::try_again_at`), until then; else for the policy's
    /// `key-again-after`, the time a key out for its rate limit with no time named is taken again, which is what the
    /// gateway is (paper-research N-1397 p142: two of the five 429s named none), doubled each time such a stop comes
    /// again with no turn ended by its Stop between, to `LIMIT_WAIT_MOST` (proto-lead N-1397 p151). None for another
    /// error, a limit its account's (the account mover restarts it to go on: `Live::account_cut`), or no time at all:
    /// its parent is then told, as before. With it the `Live::limit_wait` to keep: this wait when no reset was named,
    /// the one before when one was, none for another error. Said in the journal.
    fn limit_of(&self, name: &str, input: &Value) -> (Option<Limit>, Option<i64>) {
        let live = &self.agents[name].live;
        // what is waited out: a rate limit, and the provider's passing failures, its 529 "overloaded" and a server
        // error, which Claude Code ends a turn on once its own retries are spent (the owner's walk item 14: fresh's
        // turn of 25 Nov 11:09Z stayed stopped till he typed); any other error is its parent's to hear of. An agy
        // agent's turn is given again only at a reset Google named: nothing is asked of Google again on a guessed
        // wait (N-1956.p1: no automatic retries on an agy account), so its passing failures are its parent's too
        let error = s(input, "error");
        let agy = self.agents[name].kind == "agy";
        let what = match error.as_str() {
            crate::accounts::limits::RATE_LIMIT => None,
            "overloaded" | "server_error" if agy => return (None, None),
            "overloaded" => Some("the provider overloaded"),
            "server_error" => Some("a server error"),
            _ => return (None, None),
        };
        if live.account_cut.is_some() {
            return (None, None);
        }
        let words = [s(input, "error_details"), s(input, "last_assistant_message")].into_iter().map(|w| w.trim().to_string()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" · ");
        let at = crate::accounts::mover::now_s();
        // agy-rpc gives Google's reset as its time (`resets_at`), not in words
        let given = || agy.then(|| input.get("resets_at").and_then(Value::as_i64)).flatten().filter(|t| *t > at).map(|t| t - at);
        let named = what.is_none().then(|| crate::accounts::limits::reset_after(&words).or_else(|| crate::accounts::limits::try_again_at(&words, at)).or_else(given)).flatten();
        let (until, from, wait) = match named {
            Some(secs) => (at + secs, String::new(), live.limit_wait),
            None if agy => {
                eprintln!("coordd: {name}: its agy turn stopped on a rate limit that names no reset: its parent is told, nothing is given again");
                return (None, None);
            }
            None => {
                let Some(again) = self.key_again_after() else {
                    eprintln!("coordd: {name}: its turn stopped on {} that names no reset, and the policy states no key-again-after: its parent is told", what.unwrap_or("a rate limit"));
                    return (None, None);
                };
                let again = again as i64;
                let secs = live.limit_wait.map_or(again, |w| (w * 2).min(LIMIT_WAIT_MOST).max(again));
                let how = if secs == again { format!("the policy's key-again-after, {secs} s") } else { format!("{secs} s, the wait before doubled, from the policy's key-again-after ({again} s) to {LIMIT_WAIT_MOST} s at most") };
                (at + secs, format!(" (no reset named: {how})"), Some(secs))
            }
        };
        let why: String = if words.is_empty() { error } else { words.chars().take(300).collect() };
        eprintln!("coordd: {name}: its turn stopped on {} ({why}); nothing is handed to it until {}{from}, or till its model is served again, then it is given its turn again", what.unwrap_or("a rate limit"), at_secs(until));
        (Some(Limit { at, until, why, served: None, what: what.map(str::to_string) }), wait)
    }

    /// NAME's state out of a turn: waiting on its background work WAITS and its tickets, or idle with its parent told.
    async fn settle_waits(&mut self, name: &str, mut waits: Vec<String>) {
        waits.extend(self.ticket_waits(name));
        self.settled(name, waits).await;
    }

    /// NAME's state out of a turn from all its WAITS, its background work's and its tickets' (`settle_waits`).
    async fn settled(&mut self, name: &str, waits: Vec<String>) {
        if waits.is_empty() {
            self.set_state(name, "idle");
            let parent = self.agents[name].parent.clone();
            self.changed(name);
            // no idle notice (S5): the state is visible; a turn that ended on an API error is still told. Not one cut
            // by its account's limit (accounts/mover.rs): that turn goes on by itself at its restart under an account
            // that can take it, so nobody is told and its step is not counted as refused (paper-research, N-412 p207).
            // Nor one stopped on another rate limit: it is given its turn again at the reset (`Live::limit`; N-1397 p142)
            let a = &self.agents[name];
            if let Some(why) = a.failed.clone().filter(|_| a.live.account_cut.is_none() && a.live.limit.is_none()) {
                self.notice(&parent, &format!("{name} idle: its turn ended on an API error ({why})")).await;
                self.api_refused(name, &why);
            }
        } else {
            self.set_state(name, &format!("waiting on {}", waits.join(", ")));
            self.changed(name);
        }
    }

    // ---- nets ---------------------------------------------------------------------------------------------------

    /// One line of the project's event source (`project::Header::events`): raised to the nets as its `event`, about
    /// its `to` agent if it names one; with `"notify": "owner"`, mailed to the owner from coord unless the same
    /// `fingerprint` was mailed within the hour (supervision-nets principle 5: one word per fault per window).
    pub async fn project_event(&mut self, v: Value) {
        let Some(event) = v.get("event").and_then(Value::as_str).map(str::to_owned) else {
            return eprintln!("coordd: project event without `event`: {v}");
        };
        // the runner seen alive or gone: a step given to it is not silent while it lives (ticket/quiet.rs)
        let seen = match event.as_str() {
            "runner_up" => Some(true),
            "runner_down" => Some(false),
            "runner_seen" => Some(v.get("up").and_then(Value::as_bool).unwrap_or(false)),
            _ => None,
        };
        if let Some(up) = seen {
            // in the journal, so a check can see that the runner was seen before a runner step waits on it
            eprintln!("coordd: tickets: the runner is seen {} ({event})", if up { "alive" } else { "gone" });
            self.tickets.quiet.runner_seen(up, &now());
        }
        // the runner is up (its own `up`, or the source's first look when coordd starts): the steps held back because
        // their command could not run go at the next hand-over, once each (N-159 post 48)
        if matches!(event.as_str(), "runner_up" | "runner_seen") && !self.runner_unsent.is_empty() {
            eprintln!("coordd: tickets: the runner is up ({event}): {} held back go again", self.runner_unsent.keys().cloned().collect::<Vec<_>>().join(" "));
            self.runner_again();
        }
        // a check a ticket's step asked for (project::to_runner names it by the ticket's own id): the runner's answer
        // to it. Any other name is the runner's own check (Tovrin's `printer-1`, which parse_ticket reads as 1): nothing
        // to the tickets, no answer, no refusal, no log line (paper-research N-27 post 35, 3)
        // an agent's own check asked through coordd (`coord check`, runner.rs): its act `check` and a notice to it
        if event == "check_end" && !self.check_answered(&v) {
            let ticket = v.get("name").and_then(Value::as_str).and_then(|n| self.tickets.m.ticket_named(n));
            if let Some(t) = ticket {
                let reason = v.get("reason").and_then(Value::as_str).unwrap_or("").to_string();
                let head = if v.get("result").and_then(Value::as_str) == Some("green") { json!({"head": "Green"}) } else { json!({"head": "Red", "log": reason}) };
                if let Err(e) = self.ticket_answer("runner", t, head, docket_machine::Text::New(reason)) {
                    eprintln!("coordd: tickets: the runner's check_end for {t}: {e}");
                }
            }
        }
        // the runner's answer to a step of its contract (`RUNNER step`, project::to_runner; N-101): NAME the ticket, CALL
        // the step's call, the rest the answer (its head and fields), held to the call's answer type by the machine. An
        // answer to a step the ticket no longer owes (withdrawn, cancelled, another call) is refused, in the journal
        let mut v = v;
        if event == "step_end" {
            let ticket = v.get("name").and_then(Value::as_str).and_then(|n| self.tickets.m.ticket_named(n));
            let owed = ticket.and_then(|t| match &self.tickets.m.tickets[&t].status {
                docket_machine::Status::Awaits { agent, owed, .. } => Some((agent.clone(), owed.call.clone())),
                _ => None,
            });
            let call = v.get("call").and_then(Value::as_str).unwrap_or_default().to_string();
            match (ticket, owed) {
                (Some(t), Some((agent, owes))) if owes == call => {
                    let mut head = v.clone();
                    if let Value::Object(o) = &mut head {
                        // `kind`: the runner's history line's own, which its source passes on (runner-v2)
                        for k in ["event", "name", "call", "at", "kind"] {
                            o.remove(k);
                        }
                    }
                    let said = format!("{call}: {}", head.get("head").and_then(Value::as_str).unwrap_or("?"));
                    if let Err(e) = self.ticket_answer(&agent, t, head, docket_machine::Text::New(said)) {
                        eprintln!("coordd: tickets: the runner's step_end for {}: {e}", self.tickets.m.id(t));
                    }
                }
                (Some(t), owed) => eprintln!("coordd: tickets: the runner's step_end for {} answers {call}, and it owes {}", self.tickets.m.id(t), owed.map_or("no step".to_string(), |(_, c)| c)),
                (None, _) => eprintln!("coordd: tickets: a step_end names no ticket of this coord: {v}"),
            }
        }
        // what the runner sees while it holds a step of a ticket (`outcome`), and a step's answer, name the ticket as
        // NAME; a ticket's handler reads it as `ticket`
        if matches!(event.as_str(), "outcome" | "step_end" | "job_start" | "job_measure" | "job_end" | "step_refused") && v.get("ticket").is_none() && v.get("name").is_some_and(Value::is_string) {
            if let (Some(n), Value::Object(o)) = (v.get("name").cloned(), &mut v) {
                o.insert("ticket".into(), n);
            }
        }
        self.ticket_event(&event, &v);
        if v.get("notify").and_then(Value::as_str) != Some("owner") {
            return;
        }
        let fp = v.get("fingerprint").and_then(Value::as_str).map_or_else(|| event.clone(), str::to_owned);
        let hour = Duration::from_secs(3600);
        if self.notified.get(&fp).is_some_and(|t| t.elapsed() < hour) {
            return;
        }
        self.notified.retain(|_, t| t.elapsed() < hour);
        self.notified.insert(fp.clone(), std::time::Instant::now());
        let field = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
        let what = [field("cause"), field("reason"), field("why"), field("excerpt")].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n");
        let body = format!("{event}: {fp}\n\n{what}\n\nat {} (the project's event source; the same fingerprint is not mailed again within the hour)", field("at"));
        self.notice("owner", &body).await;
    }

    /// Where the policy's and calls' files are (`crate::policy::Places`).
    /// What the owner's file requests need of the coord (`files`): its folder, the project's folder, `[project]
    /// files` and its repositories, the agents' working folders, and where kinds and calls are read from.
    pub fn file_places(&self) -> crate::files::Places {
        let project = self.project.as_ref().map(|p| p.dir.clone()).filter(|d| !d.is_empty());
        let files = match self.project.as_ref().and_then(|p| p.raw.pointer("/project/files")) {
            Some(Value::String(s)) => vec![s.clone()],
            Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(String::from).collect(),
            _ => Vec::new(),
        };
        crate::files::Places {
            home: std::env::var("HOME").unwrap_or_default(),
            coord: self.coord.clone(),
            folders: crate::project::folders(&self.coord, &self.base, project.as_deref()),
            project,
            files,
            repos: self.project.as_ref().map(|p| p.repos()).unwrap_or_default(),
            cwds: self.agents.values().filter_map(|a| a.session.get("cwd").and_then(Value::as_str)).map(String::from).collect(),
            look: self.text_at().and_then(|a| a.look),
        }
    }

    /// The project's folder: its file's `repo` (`Project::repo`), else the folder new-project wrote to the coord's
    /// `repo` (the New project dialog's Folder, or the empty `work` it made), when it is a folder.
    pub fn project_folder(&self) -> Option<String> {
        let made = || std::fs::read_to_string(format!("{}/repo", self.coord)).ok().map(|r| r.trim().to_string()).filter(|r| std::path::Path::new(r).is_dir());
        self.project.as_ref().and_then(|p| p.repo()).or_else(made).filter(|d| !self.in_coord(d))
    }

    /// Whether folder D is the coord's own or inside it (an agent's home, an old new-project's `work`): no project folder.
    fn in_coord(&self, d: &str) -> bool {
        let (d, c) = (d.trim_end_matches('/'), self.coord.trim_end_matches('/'));
        d == c || d.strip_prefix(c).is_some_and(|r| r.starts_with('/'))
    }

    /// Where a fresh agent starts, for step context CTX (`{}` for the owner's New agent): main.rs `fresh_dir` over CTX,
    /// the project's folder, the folders its agents (programs aside) run in and its files folder, none inside the
    /// coord's folder; None: in its own home ([`crate::homes::made`]), never the coord's folder or a scratch one in it
    /// (the owner's Stream 4A, N-1979). The one rule for a step's fresh agent, the owner's New agent and the snapshot's
    /// `places.start`.
    pub fn start_dir(&self, ctx: &Value) -> Option<String> {
        let cwds: Vec<String> = self.agents.values().filter(|a| a.kind != "program").filter_map(|a| a.session.get("cwd").and_then(Value::as_str).map(str::to_string)).filter(|d| !self.in_coord(d)).collect();
        let files = self.project.as_ref().map(|p| p.dir.clone());
        crate::fresh_dir(ctx, self.project_folder().as_deref(), &cwds, files.as_deref()).filter(|d| !self.in_coord(d))
    }

    /// `Request::SystemPrompt` (protocol 51; the owner N-1104 p996, accounts-lead N-1446 p2447, go p2456), its
    /// refusals: the owner's version TEXT of agent NAME's system prompt (in place of its harness's own with REPLACE),
    /// from AFTER's turn end when given, is refused, nothing written, when not the owner's; no such agent; its harness
    /// takes no system prompt file of its own, or not in place of its own with REPLACE, or no cut of its session with
    /// AFTER ([`capabilities`]; boxed or not alike: roster gives the file to the start inside the box); no text; a fork
    /// or an edit due already; NAME being renamed (`owner_roster`); with AFTER, a session whose transcript coordd does
    /// not know. Else, with AFTER, that transcript, for [`system_cut`] to read off the lock. No bound on its length:
    /// the harness reads it from its file, not from one argument.
    pub fn system_prompt_from(&self, who: &Who, name: &str, text: &str, replace: bool, after: Option<u64>) -> Result<Option<String>, Reply> {
        if !matches!(who, Who::Owner) {
            return Err(err("coord system-prompt: the owner's".into()));
        }
        let Some(a) = self.agents.get(name) else { return Err(err(format!("coord system-prompt: no agent {name}"))) };
        let can = capabilities(&a.kind, self.boxed());
        if !can.system_prompt_file {
            return Err(err(format!("coord system-prompt: {name}'s harness ({}) takes no system prompt of its own (its capabilities: system_prompt_file false)", a.kind)));
        }
        if replace && !can.system_prompt_replace {
            return Err(err(format!("coord system-prompt: {name}'s harness ({}) takes one only beside its own (system_prompt_replace false)", a.kind)));
        }
        if after.is_some() && !can.turn_branching {
            return Err(err(format!("coord system-prompt: {name}'s harness ({}) cannot go on from a cut of its session (turn_branching false)", a.kind)));
        }
        if text.trim().is_empty() {
            return Err(err("coord system-prompt: no text".into()));
        }
        if a.live.fork_due.is_some() || a.live.system_due.is_some() {
            return Err(err(format!("coord system-prompt: {name} is about to restart already (a fork or an edit); edit again once it has")));
        }
        if self.renaming.contains(name) {
            return Err(err(format!("coord system-prompt: {name} is being renamed; edit it under its new name once it has restarted")));
        }
        match after {
            None => Ok(None),
            Some(_) => a.transcript().map(Some).ok_or_else(|| err(format!("coord system-prompt: {name} runs no session whose transcript coordd knows"))),
        }
    }

    /// `Request::SystemPrompt`, applied under the lock once [`system_cut`] has read its turn's end (CUT) off it: the
    /// owner's version TEXT of agent NAME's system prompt from now, appended to Claude Code's own or with REPLACE in its
    /// place, as a branch in its history. Refused as [`Core::system_prompt_from`] refuses, checked again here as the
    /// lock was let go, and when CUT is of another session than NAME runs now. Written as its own file,
    /// `$COORD/prompts/NAME.system/N.md`, N one more than its last and never overwritten; NAME is restarted with it once
    /// not in a turn (`Live::system_due`), on its conversation as it stands, or with CUT on a copy of its session cut
    /// there, its old session kept as it was. Its start's `began` line (roster's) records the version and the session
    /// it was cut from.
    pub fn system_prompt(&mut self, who: &Who, name: &str, text: &str, replace: bool, cut: Option<ForkCut>) -> Reply {
        let file = match self.system_prompt_from(who, name, text, replace, cut.as_ref().map(|c| c.upto)) {
            Ok(f) => f,
            Err(r) => return r,
        };
        if let Some(c) = cut.as_ref().filter(|c| file.as_deref() != Some(c.file.as_str())) {
            return err(format!("coord system-prompt: {name} runs another session than {} now; pick its turn again", c.file));
        }
        let dir = format!("{}/prompts/{name}.system", self.coord);
        let last = std::fs::read_dir(&dir).into_iter().flatten().flatten().filter_map(|e| e.file_name().to_str()?.strip_suffix(".md")?.parse::<u32>().ok()).max().unwrap_or(0);
        let version = last + 1;
        let file = format!("{dir}/{version}.md");
        let written = std::fs::create_dir_all(&dir).and_then(|_| std::fs::OpenOptions::new().write(true).create_new(true).open(&file)).and_then(|mut f| std::io::Write::write_all(&mut f, format!("{}\n", text.trim_end()).as_bytes()));
        if let Err(e) = written {
            return err(format!("coord system-prompt: {file}: {e}"));
        }
        let from = match &cut {
            Some(c) => format!("from a copy of its session cut after {} bytes", c.upto),
            None => "on its conversation as it stands".to_string(),
        };
        eprintln!("coordd: {name}: system prompt {version} ({}) given; it restarts with it {from} once not in a turn", if replace { "in place of Claude Code's own" } else { "appended" });
        self.agents.get_mut(name).unwrap().live.system_due = Some(SystemDue { version, replace, cut });
        self.changed(name);
        self.journal();
        self.poke.notify_one();
        ok(format!("{name} goes on with system prompt {version} {from}, once its turn ends"))
    }

    pub fn places(&self) -> crate::policy::Places {
        crate::policy::Places { base: self.base.clone(), project: self.project.as_ref().map(|p| p.dir.clone()), coord: self.coord.clone(), ..Default::default() }
    }

    /// `Request::Release`: the owner has a mail its net held or sent back delivered as it was.
    pub async fn release(&mut self, who: &Who, id: &str) -> Reply {
        if who.name() != "owner" {
            return err("release: the owner releases a held mail".into());
        }
        let Some(m) = self.find_mail(id).cloned() else { return err(format!("release: no mail {id}")) };
        let was = self.clerking.remove(id) | self.clerked.remove(id).is_some();
        if !was {
            return err(format!("release: {id} is not held by a net"));
        }
        append(&self.path("mail/log.jsonl"), &json!({"ev": "released", "id": id, "at": now()}));
        let v = self.mail_view(&m);
        self.emit(Body::Mail(v));
        for t in m.to.iter().filter(|t| self.agents.contains_key(*t)).cloned().collect::<Vec<_>>() {
            self.deliver(&t, &m).await;
        }
        ok(format!("{id} released"))
    }

    /// `Request::Unfile`: the owner takes back what a net filed: a filed question is a question no more, a filed reply
    /// is hidden from its thread.
    pub fn unfile(&mut self, who: &Who, id: &str) -> Reply {
        if who.name() != "owner" {
            return err("unfile: the owner takes back what a net filed".into());
        }
        let Some(run) = self.filed.get(id).cloned() else { return err(format!("unfile: {id} was not filed by a net")) };
        if self.unfiled.insert(id.into()) {
            append(&self.path("mail/log.jsonl"), &json!({"ev": "unfile", "id": id, "run": run, "at": now()}));
            let v = self.mail_view(self.find_mail(id).expect("a filed id is a mail"));
            self.emit(Body::Mail(v));
            self.thread_changed(id);
        }
        ok(format!("{id} unfiled (filed by {run})"))
    }

    /// What the kinds' and calls' check refused and what replay found diverging (`Tickets::errors`), told when it
    /// changed.
    pub fn refusals(&mut self) {
        let errors = self.tickets.errors.clone();
        if errors != self.refusals {
            self.refusals = errors.clone();
            self.emit(Body::Refusals(errors));
        }
    }

    // ---- mail --------------------------------------------------------------------------------------------------

    /// Whether the owner holds the agents' mail (`$COORD/hold` exists, owner 10 Nov: the box back up with every agent
    /// idle until the owner writes to them), looked at again when 2 s old: then only the owner's mail sent since the
    /// hold began goes; the rest stays held until the file is removed. Some(first mail position the hold lets go).
    fn hold(&self) -> Option<usize> {
        let mut h = self.hold.borrow_mut();
        if h.0.is_none_or(|at| at.elapsed() >= Duration::from_secs(2)) {
            let on = std::path::Path::new(&self.path("hold")).exists();
            h.1 = match (on, h.1) {
                (true, None) => Some(self.mails.len()),
                (true, was) => was,
                (false, _) => None,
            };
            h.0 = Some(std::time::Instant::now());
        }
        h.1
    }

    fn pending(&self, name: &str) -> Vec<Mail> {
        let hold = self.hold();
        let going = self.inflight.get(name);
        let waits = |m: &&Mail| {
            let key = (m.id.clone(), name.to_string());
            !self.delivered.contains(&key) && !self.withdrawn.contains(&key) && !self.clerking.contains(&m.id) && !self.clerked.contains_key(&m.id)
                && !going.is_some_and(|g| g.ids.contains(&m.id))
        };
        let lets_go = |i: &usize| hold.is_none_or(|from| *i >= from && self.mails[*i].from == "owner");
        // in sending order: a mail still with its message net holds back the mail sent after it, so a later net run
        // that ends first does not overtake it (owner m544, 10 Nov: "so is it fast ?" arrived after "faster i mean"),
        // and a net's rewrite takes the place of the mail it stands for, not the place of its own later number (owner
        // m547). A mail the net sent back is out of the way: it waits for the owner, the rest goes on.
        let Some(h) = self.ix.held.get(name) else { return Vec::new() };
        let mut order: Vec<usize> = h.iter().copied().filter(lets_go).collect();
        order.sort_by_key(|i| (self.ix.stands_for.get(i).copied().unwrap_or(*i), *i));
        order.into_iter().map(|i| &self.mails[i]).take_while(|m| !self.clerking.contains(&m.id)).filter(waits).cloned().collect()
    }

    /// The ticket deliveries A takes now: out of a turn only (a step is a turn's work).
    /// What is due to NAME that may go now: all of it, or, while $COORD/hold is there (box.sh start, the cut-over's
    /// steps), only posts written by the owner alone, as the hold lets only the owner's mail through (`pending`); a step,
    /// a notice or another's post waits until the hold is lifted (docket-core m1072: the hold held only the old mail).
    pub fn due_now(&self, name: &str) -> Vec<docket_machine::Delivery> {
        self.due_now_of(self.ticket_due(name))
    }

    /// `due_now` of an agent whose `ticket_due` is DUE (read for several at once, `Tickets::due_all`).
    pub(crate) fn due_now_of(&self, mut due: Vec<docket_machine::Delivery>) -> Vec<docket_machine::Delivery> {
        let now = std::time::Instant::now();
        due.retain(|d| self.runner_unsent.get(&d.id).is_none_or(|u| u.next <= now));
        if self.hold().is_none() {
            return due;
        }
        due.into_iter().filter(|d| matches!(&d.what, docket_machine::What::Posts { unread } if !unread.is_empty() && unread.iter().all(|p| p.by == "owner"))).collect()
    }

    /// Runner delivery D could not be sent because the runner's command could not start (WHY): held back, no longer on
    /// its way, and tried again `runner_after` (a minute) from now. True when the journal is to say so: the first failure, or an
    /// error other than the last one (one line when the error changes, not one per try).
    pub fn runner_held(&mut self, name: &str, d: &docket_machine::Delivery, why: &str) -> bool {
        // not on its way any more, or `due` would never offer it again (until coordd restarted)
        self.tickets.handed.remove(&(name.to_string(), d.id.clone()));
        let next = std::time::Instant::now() + self.runner_after;
        match self.runner_unsent.get_mut(&d.id) {
            Some(u) => {
                u.next = next;
                u.tries += 1;
                let changed = u.why != why;
                u.why = why.to_string();
                changed
            }
            None => {
                let (step, call) = match &d.what {
                    docket_machine::What::Step { step, call, .. } => (*step, call.clone()),
                    _ => (0, String::new()),
                };
                self.runner_unsent.insert(d.id.clone(), Unsent { t: d.t, step, call, worker: name.to_string(), tries: 1, why: why.to_string(), since: now(), next });
                true
            }
        }
    }

    /// What came of sending runner delivery D to NAME's command (SENT, `project::to_runner`): false when it is held back
    /// and not given, true when it is to be given (it ran: taken, or refused, which the step raises).
    pub fn runner_outcome(&mut self, name: &str, d: &docket_machine::Delivery, sent: &Result<(), crate::project::NotSent>) -> bool {
        match sent {
            // could not run at all: nothing answered, so it is not given; held back and tried again a minute on, for
            // as long as it is owed, the journal told once per error (N-159 post 46; paper-research N-101 p448)
            Err(n) if !n.ran => {
                if self.runner_held(name, d, &n.why) {
                    eprintln!("coordd: tickets: {} to {name}, held back and tried again each minute until the runner's command runs: {}", d.id, n.journal);
                }
                return false;
            }
            Err(n) => {
                self.runner_sent(&d.id);
                eprintln!("coordd: tickets: {} to {name}: {}", d.id, n.journal);
            }
            // the runner's command ran again (a step, not a notice, which runs nothing): what was held back goes at the
            // next hand-over
            Ok(()) if matches!(d.what, docket_machine::What::Step { again: false, .. }) => {
                self.runner_sent(&d.id);
                self.runner_again();
            }
            Ok(()) => {}
        }
        true
    }

    /// Every held runner step goes at the next hand-over (the runner seen up, or a later runner command ran); held
    /// again, with its first failure's time, if it still cannot start.
    pub fn runner_again(&mut self) {
        let now = std::time::Instant::now();
        for u in self.runner_unsent.values_mut() {
            u.next = now;
        }
    }

    /// Runner delivery ID went to the runner's command, which ran (given, or refused: either way not held).
    pub fn runner_sent(&mut self, id: &str) {
        self.runner_unsent.remove(id);
    }

    /// Ticket T's runner steps held back, as its view shows them (`TicketView::unsent`, docket-edge N-236).
    pub fn runner_unsent_of(&self, t: u64) -> Vec<docket_machine::ViewUnsent> {
        self.runner_unsent
            .values()
            .filter(|u| u.t == t)
            .map(|u| docket_machine::ViewUnsent { step: u.step, call: u.call.clone(), worker: u.worker.clone(), why: Some(u.why.clone()), since: Some(u.since.clone()), tries: Some(u.tries) })
            .collect()
    }

    /// What A, an agent the delivery program does not decide for (no `kind delivery` loaded), is handed now:
    /// everything due, out of a turn; nothing while it is in one or an esc stops it. Nothing here chooses: with the
    /// program loaded it does (delivery.rs).
    fn tickets_now(&self, a: &Agent) -> Vec<docket_machine::Delivery> {
        if self.tickets.stopping.contains(&a.name) || self.in_turn(a) {
            return Vec::new();
        }
        self.due_now(&a.name)
    }

    /// Whether A is in a turn whose end has not been taken.
    pub(crate) fn in_turn(&self, a: &Agent) -> bool {
        a.live.state == "working" && a.ended_turn != a.live.turn
    }

    /// Words typed by hand into NAME's terminal, at LOC of its transcript (paper-research N-101 p1107, p1127): a post
    /// by whoever sent the keys (`typist`: the owner from his app or his shell, an agent through `coord attach`), by
    /// reference to the entry, marked typed there. Typed when no turn runs, it is given to NAME in the turn it begins,
    /// so the turn's last message is posted there as the reply (the rule Reply); what agents send waits for that turn's
    /// end (the delivery program's `arrived`). Typed into a running turn (Claude Code queues it there), it is read, not given: that
    /// turn's last message goes where its delivery puts it. On the thread the typist last wrote to NAME on when NAME has
    /// read all of it, else on a new thread. Not taken: its launch prompt, an empty entry, and words nobody can be named
    /// for (no keys note, or coordd's own keys last), which are a journal line.
    pub fn typed(&mut self, name: &str, loc: docket_machine::Loc) {
        if !self.agents.get(name).is_some_and(|a| harness(&a.kind)) {
            return;
        }
        let Some(text) = crate::follow::text_at(&loc.file, loc.offset, loc.len).filter(|t| !t.trim().is_empty()) else { return };
        if let Some(first) = self.launched_with.get(name) {
            let (f, t) = (first.trim(), text.trim());
            if !f.is_empty() && (f == t || t.ends_with(f) || f.ends_with(t)) {
                self.launched_with.remove(name);
                return;
            }
        }
        // who typed it: the last sender of keys to its terminal before the entry was written (coord-pty's
        // `NAME.keys`); not posted when nobody can be named (paper-research N-101 p1127, 1)
        let by = match self.typist(name, &loc) {
            Ok(by) => by,
            Err(why) => {
                eprintln!("coordd: {name}: a prompt in its transcript at {}:{} not posted: {why}", loc.file, loc.offset);
                return;
            }
        };
        use docket_machine::{Input, Text};
        let m = &self.tickets.m;
        let thread = m
            .tickets
            .values()
            .filter(|x| x.kind == "thread")
            // nothing on it left unread by NAME (its own posts it has): the typed words are given up to their own
            .filter(|x| { let mu = x.mu.get(name).copied().unwrap_or(0); x.posts.iter().all(|p| p.n <= mu || p.by == name) })
            .filter_map(|x| x.posts.iter().rev().find(|p| p.by == by && (p.names.iter().any(|n| n == name) || p.typed.as_deref() == Some(name))).map(|p| (p.at.clone(), x.t)))
            .max()
            .map(|(_, t)| t);
        let names = vec![name.to_string()];
        let posted = match thread {
            Some(t) => self.ticket_input(Input::Post { t, by: by.clone(), text: Text::Loc { loc: loc.clone() }, names: names.clone(), told: Vec::new(), inform: Vec::new(), answers: None, kind: None }).map(|_| t),
            None => {
                let form = crate::ticket::sent_form(&by, &names, &text);
                // the last thread before it, below the hidden tickets (the delivery program's among them)
                let before = self.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().map(|(t, _)| *t);
                self.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: by.clone(), form, text: Some(Text::Loc { loc: loc.clone() }), names: names.clone(), told: Vec::new() })
                    .and_then(|_| self.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().map(|(t, _)| *t).filter(|t| Some(*t) != before).ok_or_else(|| String::from("no thread was opened")))
            }
        };
        let t = match posted {
            Ok(t) => t,
            Err(e) => {
                eprintln!("coordd: {name}: words typed in its terminal not posted: {e}");
                return;
            }
        };
        let Some(n) = self.tickets.m.tickets.get(&t).and_then(|x| x.posts.last()).map(|p| p.n) else { return };
        let delivery = format!("{}.p{n}", self.tickets.m.id(t));
        // given in the turn it began; typed into a turn already given something (Claude Code queues it there), it is
        // read but not counted for the rule Reply: that turn's last message goes where its delivery puts it (p1127, 3)
        let began = self.ticket_given_since(name).is_empty();
        let mut inputs = vec![Input::Typed { t, n, agent: name.to_string() }];
        if began {
            inputs.insert(0, Input::Given { t, to: name.to_string(), delivery, upto: n, skipped: Default::default(), place: Some(loc) });
        }
        for input in inputs {
            if let Err(e) = self.ticket_input(input) {
                eprintln!("coordd: {name}: words typed in its terminal: {e}");
            }
        }
    }

    /// Who typed the prompt at LOC of NAME's transcript: the last sender of keys to NAME's terminal before the entry
    /// was written, by its cgroup (`cgroup::identify_path`): the owner (his app's terminal, or a shell of his), or the
    /// agent that ran `coord attach`. Not coordd's own keys (an esc), nor a holder's.
    fn typist(&self, name: &str, loc: &docket_machine::Loc) -> Result<String, String> {
        let at = crate::follow::written_at(&loc.file, loc.offset).and_then(|t| crate::transcript::epoch_ms(&t)).ok_or("the entry says no time")?;
        let keys = std::fs::read_to_string(coord_types::pty::keys(&self.coord, name)).map_err(|_| "no keys reached its terminal through coord-pty")?;
        // the keys arrive before the entry is written, on the host's one clock: a quarter second for the two reads of it
        let last = keys.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ms"].as_i64().is_some_and(|ms| ms <= at + 250)).last().ok_or("no keys reached its terminal before it")?;
        if last["pid"].as_i64() == Some(i64::from(std::process::id())) {
            return Err("the last keys were coordd's own".into());
        }
        match crate::cgroup::identify_path(last["cgroup"].as_str().unwrap_or("")) {
            crate::cgroup::Caller::Owner => Ok("owner".into()),
            crate::cgroup::Caller::Agent(a) => Ok(a),
            _ => Err("the last keys came from no one coordd can name".into()),
        }
    }

    /// `Request::Writing`: the owner is writing in his chat with AGENT (ON), or stopped; said by process PID, whose end
    /// (the app closed) ends it as well (paper-research N-101 p1127, 2).
    pub fn owner_writing(&mut self, who: &Who, agent: &str, on: bool, pid: i32) -> Reply {
        if who.name() != "owner" {
            return err("writing: the owner's own".into());
        }
        if !self.agents.contains_key(agent) {
            return err(format!("writing: no agent {agent}"));
        }
        // the delivery program reads it on the agent's next `free`, and its end owes one (delivery.rs)
        if on {
            self.writing.insert(agent.to_string(), pid);
        } else {
            self.writing.remove(agent);
        }
        self.poke.notify_one();
        ok(String::new())
    }

    /// `Request::Writing` with a thread (0.3; proto-lead N-1446 p26): the owner is writing in ticket THREAD's box (ON),
    /// or stopped, said by process PID. This extends `owner_writing` from his chat with an agent to a ticket's box,
    /// recorded: one `writing` line each way (`syntax::WRITING`), an event of the machine's own as `mail_hold` is, and
    /// the machine keeps since when (`Machine::writing_since`) for the hold of his posts there. The saying process's
    /// end writes it off at the next round, as it ends his writing to an agent (`threads_writing_ended`). A second
    /// `on`, or an `off` with none, writes nothing.
    pub fn owner_writing_on(&mut self, who: &Who, thread: &str, on: bool, pid: i32) -> Reply {
        if who.name() != "owner" {
            return err("writing: the owner's own".into());
        }
        let Some(t) = docket_machine::parse_ticket(thread).filter(|t| *t < docket_machine::HIDDEN && self.tickets.m.tickets.contains_key(t) && self.tickets.m.id(*t) == thread) else {
            return err(format!("writing: no ticket {thread}"));
        };
        if on {
            self.writing_on.insert(t, pid);
        } else {
            self.writing_on.remove(&t);
        }
        match self.ticket_input(docket_machine::Input::Event { name: docket_machine::syntax::WRITING.into(), value: json!({"t": t, "on": on}) }) {
            Ok(r) => ok(r),
            Err(e) => err(format!("writing: {e}")),
        }
    }

    /// Each ticket the owner was writing in whose saying process has ended, written off (`owner_writing_on`); and
    /// after a start, each the record left on, which no process has said since: the window says it again as he
    /// types. Run at each hand-over round.
    pub(crate) fn threads_writing_ended(&mut self) {
        self.writing_on.retain(|_, p| alive(*p));
        let ended: Vec<u64> = self.tickets.m.writing.keys().filter(|t| !self.writing_on.contains_key(*t)).copied().collect();
        for t in ended {
            if let Err(e) = self.ticket_input(docket_machine::Input::Event { name: docket_machine::syntax::WRITING.into(), value: json!({"t": t, "on": false}) }) {
                eprintln!("coordd: the owner's writing on ticket {t} not written off: {e}");
            }
        }
    }

    /// The ids of the items, old and new (Docket's ticket numbers start above them).
    pub fn item_ids(&self) -> Vec<String> {
        self.items.iter().map(|r| r.id.clone()).collect()
    }

    /// The held mail A takes now: all of it out of a turn; in one (a Claude Code agent's) only the mail sent to go in
    /// soon.
    fn handed_now(&self, a: &Agent) -> Vec<Mail> {
        let held = self.pending(&a.name);
        if a.live.state != "working" || a.ended_turn == a.live.turn {
            return held;
        }
        if a.kind != "claude" {
            return Vec::new();
        }
        held.into_iter().filter(|m| m.soon).collect()
    }

    fn mark(&mut self, name: &str, ms: &[Mail], how: &str) {
        for m in ms {
            append(&self.path("mail/log.jsonl"), &json!({"ev": "delivered", "id": m.id, "to": name, "at": now(), "how": how}));
            self.delivered.insert((m.id.clone(), name.into()));
            self.unhold(&m.id, name);
            self.emit(Body::Delivered { id: m.id.clone(), to: name.into(), how: how.into() });
        }
    }

    /// The first question of ID's thread again, with its new state.
    fn thread_changed(&mut self, id: &str) {
        if let Some(root) = self.thread_root(id).filter(|r| r != id) {
            let v = self.mail_view(self.find_mail(&root).expect("a thread's root is a mail"));
            self.emit(Body::Mail(v));
        }
    }

    /// `coord ask --list`: WHO's questions and the question threads WHO was brought into (the owner: every one), oldest first, each as its id, state, when and
    /// subject, then the rest of its thread: the owner's words in full (the answers), the agents' follow-ups and
    /// replies by their subject.
    pub fn questions(&self, who: &Who) -> Reply {
        let me = who.name();
        let roots: Vec<&Mail> = self
            .mails
            .iter()
            .filter(|m| self.is_question(m) && m.re.is_none() && (me == "owner" || m.from == me || self.participants(&m.id).contains(&me)))
            .collect();
        if roots.is_empty() {
            return ok("no questions".into());
        }
        let when = |at: &str| at.get(..16).unwrap_or(at).replace('T', " ");
        let out: Vec<String> = roots
            .iter()
            .map(|q| {
                let by = if q.from != me { format!(" · {}", q.from) } else { String::new() };
                let mut lines = vec![format!("{} · {}{by} · asked {} · {}", q.id, self.question_state(&q.id), when(&q.at), coord_types::subject(&q.body))];
                for m in self.thread(&q.id).into_iter().filter(|m| m.id != q.id) {
                    let text = if m.from == "owner" { m.body.trim().replace('\n', "\n    ") } else { coord_types::subject(&m.body) };
                    let what = if m.from == "owner" { "answer" } else if m.kind == "question" { "follow-up" } else { "reply" };
                    lines.push(format!("  {} · {what} from {} · {}\n    {text}", m.id, m.from, when(&m.at)));
                }
                lines.join("\n")
            })
            .collect();
        ok(out.join("\n\n"))
    }

    /// `coord ask --drop`: the agent that asked a question, or the owner, withdraws it (its whole thread).
    pub fn drop_question(&mut self, who: &Who, id: &str, why: Option<String>) -> Reply {
        let Some(root) = self.thread_root(id) else { return err(format!("coord ask --drop: {id} is not a question")) };
        let me = who.name();
        if me != "owner" && self.find_mail(&root).is_some_and(|r| r.from != me) {
            return err(format!("coord ask --drop: {root} is another agent's question"));
        }
        if self.dropped.insert(root.clone()) {
            append(&self.path("mail/log.jsonl"), &json!({"ev": "drop", "id": root, "by": me, "why": why, "at": now()}));
            let v = self.mail_view(self.find_mail(&root).expect("a thread's root is a mail"));
            self.emit(Body::Mail(v));
        }
        ok(format!("{root} dropped"))
    }

    /// Why nothing more is taken, when a record log (the ticket log, the mail log) failed: the words of the refusal,
    /// and the instance's notice raised with them if it is not yet (paper-research N-8). None while both take lines.
    pub fn log_fault(&mut self) -> Option<String> {
        let why = [self.path("tickets/log.jsonl"), self.path("mail/log.jsonl")].iter().find_map(|p| crate::jsonl::fault(p))?;
        let text = format!("coordd cannot write its log ({why}); nothing more is taken until coordd is restarted");
        if !self.notices.iter().any(|(of, _)| of == "log") {
            self.notices.push(("log".into(), Notice { ticket: None, text: text.clone(), at: now() }));
            self.emit(Body::Notices(self.notices.iter().map(|(_, n)| n.clone()).collect()));
        }
        Some(text)
    }

    /// Ticket TICKET's notice of OF (a model call: "intake", "minder"): raised with TEXT, replacing one of the
    /// same OF there; cleared with None (a later call of the same kind on it went through). Told when it changed.
    pub fn ticket_notice(&mut self, ticket: &str, of: &str, text: Option<String>) {
        let at = self.notices.iter().position(|(o, n)| o == of && n.ticket.as_deref() == Some(ticket));
        match (at, text) {
            (None, None) => return,
            (Some(i), None) => {
                self.notices.remove(i);
            }
            (Some(i), Some(text)) => self.notices[i].1 = Notice { ticket: Some(ticket.into()), text, at: now() },
            (None, Some(text)) => self.notices.push((of.into(), Notice { ticket: Some(ticket.into()), text, at: now() })),
        }
        self.emit(Body::Notices(self.notices.iter().map(|(_, n)| n.clone()).collect()));
    }

    /// The engine's own call OF (the watcher's `intake`, the minder's look) on TICKET answered, FAILED if it failed: said
    /// on the ticket in WORDS, and cleared by a later call of that kind there that goes through (paper-research N-8);
    /// but a call that found no model to run on (DECISION: none chosen, none tried; a new project with no route) is no
    /// fault of his thread. Its step is passed over as before, coordd's journal says why once, until a call of that kind
    /// finds a model again, and nothing is drawn under his post: tooling says its failures in its journal, never as a
    /// message (his rule of 14 Nov; paper-research N-1446 p3284, on try-80's first thread). Where such calls run is his
    /// to say, so none is placed on his accounts here.
    pub fn own_call_failed(&mut self, ticket: &str, of: &str, decision: Option<&coord_types::Decision>, failed: Option<String>, words: impl FnOnce(String) -> String) {
        match failed {
            Some(why) if decision.is_some_and(crate::policy::no_model) => {
                if self.no_model_said.insert(of.to_string()) {
                    eprintln!("coordd: {of} on {ticket}: no model to run on ({why}); the step is passed over, nothing is said on the ticket (said once until a call of this kind finds a model)");
                }
                self.ticket_notice(ticket, of, None);
            }
            Some(why) => self.ticket_notice(ticket, of, Some(words(why))),
            None => {
                if decision.is_some_and(|d| d.chosen.is_some()) {
                    self.no_model_said.remove(of);
                }
                self.ticket_notice(ticket, of, None);
            }
        }
    }

    /// The notices now.
    pub fn notices(&self) -> Vec<Notice> {
        self.notices.iter().map(|(_, n)| n.clone()).collect()
    }

    /// The owner's answer to the old mail RE (an open question, qN, or a mail in its thread), given on the release as a
    /// note to TO (`ticket_msg`): kept as the old mail layer kept it too, a mail from the owner `re` RE with BODY and
    /// QUOTES, delivered already to each of TO (NOTE carried the words), so the thread reads answered in the inbox and
    /// the Decisions view, now and after a restart, and the mail is never handed (paper-research m1602).
    pub fn keep_owner_answer(&mut self, re: &str, to: &[String], body: &str, quotes: Vec<Quote>, note: &str) -> String {
        let n = self.mails.iter().filter_map(|m| m.id.strip_prefix('m')?.parse::<u64>().ok()).max().unwrap_or(0) + 1;
        let m = Mail { id: format!("m{n}"), at: now(), from: "owner".into(), to: to.to_vec(), body: body.into(), esc: false, soon: false, kind: String::new(), re: Some(re.into()), quotes };
        let mut line = serde_json::to_value(&m).expect("a mail is JSON");
        line["ev"] = json!("mail");
        append(&self.path("mail/log.jsonl"), &line);
        for t in to {
            append(&self.path("mail/log.jsonl"), &json!({"ev": "delivered", "id": m.id, "to": t, "at": m.at, "how": format!("as note {note}")}));
            self.delivered.insert((m.id.clone(), t.clone()));
        }
        let id = m.id.clone();
        let i = self.index_mail(m);
        let v = self.mail_view(&self.mails[i]);
        self.emit(Body::Mail(v));
        // the thread's first question shows its state: answered now
        if let Some(root) = self.thread_root(&id).filter(|r| *r != id) {
            let v = self.mail_view(self.find_mail(&root).expect("a thread's root is a mail"));
            self.emit(Body::Mail(v));
        }
        id
    }

    /// The recipients of mail ID still waiting for it, when WHO sent it.
    fn waiting_for(&self, who: &Who, id: &str, verb: &str) -> Result<(Mail, Vec<String>), String> {
        let Some(m) = self.find_mail(id).cloned() else { return Err(format!("coord {verb}: no mail {id}")) };
        if m.from != who.name() {
            return Err(format!("coord {verb}: {id} is {}'s, not yours", m.from));
        }
        let left: Vec<String> = m.to.iter().filter(|t| {
            let key = (m.id.clone(), (*t).clone());
            !self.delivered.contains(&key) && !self.withdrawn.contains(&key) && self.agents.contains_key(*t)
        }).cloned().collect();
        if left.is_empty() {
            return Err(format!("coord {verb}: {id} has reached everyone it was for"));
        }
        Ok((m, left))
    }


    /// Take mail ID back from the recipients it has not reached (the composer's Cancel and Edit).
    pub fn unsend(&mut self, who: &Who, id: &str) -> Reply {
        let (m, left) = match self.waiting_for(who, id, "unsend") {
            Ok(x) => x,
            Err(e) => return err(e),
        };
        for t in &left {
            append(&self.path("mail/log.jsonl"), &json!({"ev": "withdrawn", "id": m.id, "to": t, "at": now()}));
            self.withdrawn.insert((m.id.clone(), t.clone()));
            self.unhold(&m.id, t);
        }
        let v = self.mail_view(&m);
        self.emit(Body::Mail(v));
        for t in &left {
            self.changed(t);
        }
        ok(format!("{id} taken back from {}", left.join(", ")))
    }


    /// `coord msg` without policy's answer asked (`ticket_msg` with it): a note, or a post on the ticket it replies to.
    /// SOON has no meaning for a post, whose delivery waits for the turn's end or is an `--esc`.
    #[allow(clippy::too_many_arguments)]
    pub async fn msg(&mut self, who: &Who, as_host: Option<String>, to: Vec<String>, body: &str, esc: bool, soon: bool, re: Option<String>, quotes: Vec<Quote>) -> Reply {
        let _ = soon;
        self.ticket_msg(who, as_host, to, body, esc, re, quotes, crate::ticket::Said::default())
    }

    /// Mail M for agent NAME: it goes when NAME is not in a turn (`main::follow` hands it over); an old mail's esc
    /// stops nothing (N-36: only `coord esc` and the owner's Escape key stop a turn). A note when it waits.
    async fn deliver(&mut self, name: &str, m: &Mail) -> Option<String> {
        if !self.agents[name].running() {
            return Some(format!("{name} is not running; held, delivered with its next turn"));
        }
        if !harness(&self.agents[name].kind) {
            return Some(format!("{name} is a program with no session; held for coord mail"));
        }
        self.poke.notify_one();
        if m.soon && self.agents[name].kind == "claude" && self.agents[name].live.state == "working" {
            return Some(format!("{name} is working; it gets the mail after its next tool call"));
        }
        if self.agents[name].live.state == "working" {
            return Some(format!("{name} is working; held until its turn ends"));
        }
        None
    }

    /// After an interrupt: a Codex agent's turn is over and cut (a Claude Code agent's transcript says both itself: its
    /// interrupt entry is the cut, `main::follow`).
    pub fn interrupted(&mut self, name: &str) {
        // the stop has landed: what the tickets hold for NAME goes once its turn is over (docket-core, m1138)
        if self.tickets.stopping.remove(name) {
            self.poke.notify_one();
        }
        // a turn an esc stopped runs no Stop hook, and ended on no rate limit: one that names no reset waits from
        // key-again-after again (`Live::limit_wait`; consult-3, proto-reader N-1397 p157)
        if let Some(a) = self.agents.get_mut(name) {
            a.live.limit_wait = None;
        }
        if self.agents.get(name).is_some_and(|a| served(&a.kind)) {
            // a Codex (or opencode) agent has no transcript follower: its cut is written here, so what the stopped turn was given is
            // forgotten as a Claude Code agent's is, and the message after an esc is handed (paper-research N-36 post 8)
            self.ticket_turn_cut(name);
            self.agents.get_mut(name).unwrap().live.tools.clear();
            self.set_state(name, "idle");
            self.changed(name);
            self.poke.notify_one();
        }
    }

    /// NAME's running tool calls as its transcript shows them now (Claude Code: the record, `follow.rs`).
    pub fn set_tools(&mut self, name: &str, tools: Vec<Tool>) {
        let Some(a) = self.agents.get_mut(name) else { return };
        if let Some(t) = tools.iter().map(|t| t.at.clone()).max() {
            a.live.last_tool = Some(t);
        }
        a.live.tools = tools.into_iter().map(|t| (t.id.clone(), t)).collect();
        self.changed(name);
    }

    /// Rule 19 on the session's own facts (0.2 A4, N-1397 post 5, 3): of what an idle agent was given since its last
    /// turn ended, a giving whose text entered the model's input was read (`GivenRec::delivered`); one its session still
    /// holds in its queue waits, for as long as it is there; one seen in the queue that left it without entering is due
    /// again at once (`Core::ticket_due_again`). The process's end is `Core::ticket_lost_with_process`.
    ///
    /// One never seen in the session at all (its channel took it and no enqueue line came) waits a minute from the
    /// later of its handing and the agent's going idle, then is due again. The minute bounds only Claude Code writing
    /// one line after a channel message is handed to it, which it does at once whether idle, working or compacting;
    /// never the model's work (consult-3's read of accounts-lead's D22 plan, D 3, on the same minute for an account).
    /// When it runs out on a slow session the cost is one more handing, a duplicate, never a loss. It does not count
    /// while a dialog stands on the agent's screen (`Live::dialog`): a screen standing on one session is not a session
    /// that dropped what it was given (consult-3, N-818.p76).
    pub fn not_read(&mut self) {
        const UNSEEN: Duration = Duration::from_secs(60);
        let now = std::time::Instant::now();
        let names: Vec<(String, bool)> = self
            .agents
            .values()
            .filter(|a| harness(&a.kind))
            .map(|a| (a.name.clone(), a.running() && a.live.state != "working" && a.live.awaiting_start.is_none()))
            .collect();
        for (name, idle) in names {
            if self.inflight.contains_key(&name) {
                continue;
            }
            let rows = if idle { self.tickets.m.given_rows(&name) } else { Vec::new() };
            // a Codex rollout has no queue: a message seen in it is in the model's input
            let (queue, codex) = (&self.agents[&name].live.queue, self.agents[&name].kind == "codex");
            let (mut lost, mut unseen) = (Vec::new(), Vec::new());
            for g in rows.into_iter().filter(|g| g.delivered.is_none()) {
                match &g.place {
                    Some(_) if codex => {}
                    Some(p) if queue.iter().any(|(f, o)| *f == p.file && *o == p.offset) => {}
                    Some(_) => lost.push(g.delivery),
                    None => unseen.push(g.delivery),
                }
            }
            if unseen.is_empty() {
                self.idle_given.remove(&name);
            } else if let Some((_, title, looked)) = self.agents[&name].live.dialog.clone() {
                // a dialog standing: the minute does not count; its screen is read again each minute, off the lock
                self.idle_given.remove(&name);
                if now.duration_since(looked) >= UNSEEN {
                    self.agents.get_mut(&name).unwrap().live.dialog.as_mut().unwrap().2 = now;
                    if let Some(jobs) = &self.jobs {
                        let _ = jobs.send(Job::DialogLooked { name: name.clone(), title });
                    }
                }
            } else if now.duration_since(*self.idle_given.entry(name.clone()).or_insert(now)) >= UNSEEN {
                self.idle_given.remove(&name);
                self.reconciled(&name, &format!("idle a minute with {} handed and no line of its session for it: not read", unseen.join(" ")));
                // shown on its row and under his posts as not delivered, until one is seen there (`Live::handed_unseen`)
                self.agents.get_mut(&name).unwrap().live.handed_unseen = Some(unseen.join(" "));
                self.changed(&name);
                // its account's too: a first restarts it there; a second, once the first's session is gone, with none
                // seen between, makes it deaf (0.2 line 22)
                self.account_unseen(&name);
                lost.append(&mut unseen);
            }
            if lost.is_empty() {
                continue;
            }
            self.reconciled(&name, &format!("{} left its session's queue without entering its input: not read", lost.join(" ")));
            self.ticket_due_again(&name, &lost, false);
        }
    }

    /// NAME's screen, read again (`Job::DialogLooked`), no longer shows the dialog TITLE coord-pty told of: rule 19's
    /// minute counts again. A dialog told of since, with another title, stays.
    pub fn dialog_gone(&mut self, name: &str, title: &str) {
        if let Some(a) = self.agents.get_mut(name).filter(|a| a.live.dialog.as_ref().is_some_and(|(_, t, _)| t == title)) {
            a.live.dialog = None;
            self.changed(name);
        }
    }

    /// What agent NAME can take now (0.2 A1): the one answer the delivery events read (`desk_class`; `state(a)`, N-1397
    /// post 1, 2; post 5, 4). None for a name coordd does not know.
    pub fn agent_state(&self, name: &str) -> Option<AgentState> {
        let a = self.agents.get(name)?;
        let now = std::time::Instant::now();
        Some(if a.ended.is_some() {
            AgentState::Stopped { why: String::from("ended by the owner") }
        } else if !a.running() && a.account_wait.is_some() {
            AgentState::OutOfLimit { until: a.account_wait }
        } else if !a.running() {
            AgentState::Stopped { why: String::from("not running") }
        } else if let Some((why, _)) = &a.live.unreached {
            AgentState::Stopped { why: format!("its session takes nothing: {why}") }
        } else if let Some(why) = &a.live.no_sign_in {
            AgentState::Stopped { why: format!("its turn failed: Claude Code says “{why}”") }
        } else if let Some(why) = &a.live.channels_off {
            AgentState::Stopped { why: format!("its session takes no messages: Claude Code says “{why}”") }
        } else if a.live.compacting {
            AgentState::Compacting { since: a.live.compacting_since.clone() }
        } else if let Some(why) = a.live.account_due.as_ref().map(|w| format!("moving to another account: {w}")).or_else(|| a.live.effort_due.as_ref().map(|e| format!("restarting on {e}"))).or_else(|| a.live.yolo_due.then(|| format!("restarting into {}", mode_word(a.yolo)))).or_else(|| a.live.check_due.then(|| "restarting with its safety check changed".to_string())).or_else(|| a.live.fork_due.as_ref().map(|f| format!("going on from a copy of its session cut at {}", f.place))).or_else(|| a.live.system_due.as_ref().map(|d| format!("restarting with system prompt {}", d.version))) {
            // every restart `Live::restart_due` names: an account, an effort or model, the owner's Safe · Yolo (consult-3,
            // N-1397.p19, 1), a fork (part 4), the owner's edit of its system prompt (N-1104 p996)
            AgentState::Starting { why }
        } else if a.live.account_cut.is_some() {
            AgentState::Starting { why: String::from("restarting: its account's limit cut its turn") }
        } else if a.live.awaiting_start.is_some() {
            AgentState::Starting { why: String::from("its new process is starting") }
        } else if self.retry_at.get(name).is_some_and(|t| *t > now) {
            AgentState::Starting { why: String::from("its session did not take what it was handed: handed again shortly") }
        } else if let Some(l) = a.live.limit.as_ref().filter(|l| l.until > crate::accounts::mover::now_s() && !self.in_turn(a)) {
            // held until its rate limit resets, then given its turn again (`Live::limit`, N-1397 p142)
            AgentState::Starting { why: format!("its last turn stopped on {}: given its turn again at {}", l.stopped_on(), at_secs(l.until)) }
        } else if self.tickets.stopping.contains(name) {
            // an esc stops its turn, until `Core::interrupted` (docket-core2, N-807.p127)
            AgentState::Starting { why: String::from("its turn is being stopped") }
        } else if a.output_owed.is_some() {
            // its turn's end was taken before its transcript showed it: the next message's answer would be read as
            // this turn's reply (N-101 p1412)
            AgentState::Starting { why: String::from("its turn's last message is not yet in its session") }
        } else if self.moving.contains(name) {
            AgentState::Starting { why: String::from("moving to another view") }
        } else if self.in_turn(a) {
            AgentState::Working
        } else {
            AgentState::Idle
        })
    }

    /// The running agents that may take mail now (not in a turn, nothing on its way to them, not waiting to be tried
    /// again) and have mail held, once the delivery events owed now are raised (delivery.rs): what the delivery
    /// program hands, or, for an agent it does not decide for, what is due. An agent held on a rate limit whose model
    /// coordd's own call got answered since is given its turn again in this pass (`Core::serves_again`).
    pub fn to_hand_over(&mut self) -> Vec<String> {
        #[cfg(test)]
        let key = self.hand_key();
        let names = self.to_hand_over_now();
        #[cfg(test)]
        {
            assert!(self.hand_quiet != Some(key) || names.is_empty(), "handed to {names:?} with nothing changed that hand_key reads");
            self.hand_quiet = (names.is_empty() && self.hand_key() == key).then_some(key);
        }
        names
    }

    fn to_hand_over_now(&mut self) -> Vec<String> {
        self.reached_again();
        self.raise_delivery();
        self.update_owner_model();
        self.owner_model_round();
        self.serves_again();
        if self.holding {
            return Vec::new();
        }
        let (now, now_s) = (std::time::Instant::now(), crate::accounts::mover::now_s());
        let ready: Vec<&Agent> = self
            .agents
            .values()
            // a program (the runner's terminal) has no session to take mail: it stays held, readable with coord mail
            // not in a turn: idle, waiting, or its turn's end taken while its waits are still being found (`turn_waits`
            // reads the project's waits, seconds): held mail goes first, as the Stop hook's answer did
            // a Claude Code agent in a turn takes the mail sent to go in soon: its session queues it for after the
            // running tool call
            .filter(|a| a.running() && !self.inflight.contains_key(&a.name))
            // a turn's end the transcript has not shown yet: its reply is read first (`Agent::output_owed`)
            .filter(|a| a.output_owed.is_none_or(|t| now.duration_since(t) >= OUTPUT_WAIT))
            // a new effort waits for its restart (`to_restart`): what is due goes to the session that runs at it
            .filter(|a| !a.live.restart_due())
            .filter(|a| self.retry_at.get(&a.name).is_none_or(|t| *t <= now))
            // a new process takes nothing before its session's start hook; one that never sends it is handed to after
            // START_WAIT all the same, so nothing stays held for good
            .filter(|a| a.live.awaiting_start.is_none_or(|t| now.duration_since(t) >= START_WAIT))
            // a session whose coord server answers on no channel socket takes nothing (`Live::unreached`)
            .filter(|a| a.live.unreached.is_none())
            // nor one whose Claude Code said it ignores the coord channel: what it took would be dropped unseen
            // (`Live::channels_off`)
            .filter(|a| a.live.channels_off.is_none())
            // nor one whose last turn ended on Claude Code's sign-in error, while it stands (`Live::no_sign_in`)
            .filter(|a| a.live.no_sign_in.is_none())
            // a turn stopped on a rate limit: nothing until it resets, which would only meet it again (`Live::limit`)
            .filter(|a| a.live.limit.as_ref().is_none_or(|l| l.until <= now_s))
            .collect::<Vec<_>>();
        // their due read in one pass of the tickets (paper-research N-101 p1797); of those the delivery program hands
        // to, not at all: what it hands is its messages (N-1397.p38)
        let names: Vec<String> = ready.iter().filter(|a| !self.runs_delivery(a)).map(|a| a.name.clone()).collect();
        let mut all = if names.is_empty() { BTreeMap::new() } else { self.tickets.due_all(&names) };
        ready
            .into_iter()
            .filter(|a| {
                let due = all.remove(&a.name).unwrap_or_default();
                // its turn stopped on a rate limit that has reset: given again, whatever else is due (`Live::limit`)
                if self.resume_due(a) {
                    return true;
                }
                if self.pulls(a) {
                    // pushed only the owner's words, while no turn runs (pull.rs)
                    !self.tickets.stopping.contains(&a.name) && !self.in_turn(a) && (self.handed_now(a).iter().any(|m| m.from == "owner") || self.due_now(&a.name).iter().any(|d| !d.inform && crate::pull::from_owner(d)))
                } else if self.runs_delivery(a) {
                    // the delivery program's messages not yet handed, as one (delivery.rs); none while an esc stops its turn,
                    // nor while it compacts: what comes meanwhile is one message when it is over (runtime-design §8,
                    // 19 Nov 12:28:33)
                    !a.live.compacting && (!self.handed_now(a).is_empty() || (!self.tickets.stopping.contains(&a.name) && !self.hand_now(&a.name, self.in_turn(a)).items.is_empty()))
                } else if harness(&a.kind) {
                    !self.handed_now(a).is_empty() || (!self.tickets.stopping.contains(&a.name) && !self.in_turn(a) && self.due_now_of(due).iter().any(|d| !d.inform))
                } else {
                    // a program agent takes ticket deliveries only (the runner: a check), never mail
                    self.due_now_of(due).iter().any(|d| !d.inform)
                }
            })
            .map(|a| a.name.clone())
            .collect()
    }

    /// What `to_hand_over` reads, as one number (paper-research N-1446 p3381: the hand-over runs when something changed,
    /// not once a second whether or not anything did): the machine's changes (a record line, an event raised), the
    /// mail and what of it is on its way, held, delivered or taken back, the owner's holds, the deliveries handed and
    /// stopping, who he is writing to while that process lives, the policy's owner-model numbers, and each agent's
    /// state with the waits `to_hand_over` counts out (a retry, a rate limit, a start or a reply owed that has waited
    /// long enough). Equal on two rounds, the second has nothing to hand: `main`'s follow round skips it, but for a
    /// sweep now and then that would show an input missing here (`counts::hand_round`). First, what changes with no
    /// line of its own and is cheap to look at: a socket come back (`reached_again`), a model answered
    /// (`serves_again`), a writer gone (`threads_writing_ended`).
    pub fn hand_key(&mut self) -> u64 {
        use std::hash::{Hash, Hasher};
        self.reached_again();
        self.serves_again();
        self.threads_writing_ended();
        let (now, now_s) = (std::time::Instant::now(), crate::accounts::mover::now_s());
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (self.tickets.m.changes(), self.holding, self.hold(), self.owner_model_within(), self.owner_model_rebuild_after()).hash(&mut h);
        (self.mails.len(), self.delivered.len(), self.withdrawn.len(), self.clerking.len(), self.clerked.len()).hash(&mut h);
        self.ix.held.values().map(|s| s.len()).sum::<usize>().hash(&mut h);
        let mut inflight: Vec<(&String, &Vec<String>)> = self.inflight.iter().map(|(n, g)| (n, &g.ids)).collect();
        inflight.sort();
        inflight.hash(&mut h);
        let mut retry: Vec<(&String, bool)> = self.retry_at.iter().map(|(n, t)| (n, *t <= now)).collect();
        retry.sort();
        retry.hash(&mut h);
        let mut moving: Vec<&String> = self.moving.iter().collect();
        moving.sort();
        moving.hash(&mut h);
        self.tickets.stopping.hash(&mut h);
        let mut handed: Vec<(&String, &String, u64, u32)> = self.tickets.handed.iter().map(|((a, d), (t, n, _))| (a, d, *t, *n)).collect();
        handed.sort();
        handed.hash(&mut h);
        for (name, pid) in &self.writing {
            (name, alive(*pid)).hash(&mut h);
        }
        for a in self.agents.values() {
            (&a.name, a.running(), &a.live.state, a.ended_turn, a.live.turn, a.live.compacting, a.live.restart_due(), a.live.pull.is_some()).hash(&mut h);
            (a.output_owed.map(|t| now.duration_since(t) >= OUTPUT_WAIT), a.live.awaiting_start.map(|t| now.duration_since(t) >= START_WAIT)).hash(&mut h);
            (a.live.unreached.is_some(), a.live.channels_off.is_some(), a.live.no_sign_in.is_some(), a.live.limit.as_ref().map(|l| l.until <= now_s)).hash(&mut h);
            format!("{:?}", self.agent_state(&a.name)).hash(&mut h);
        }
        h.finish()
    }

    /// The program's agents (fT-N) that owe a step, run no session and are to be started again for it on their own
    /// conversation, in the step's layer (`main::start_agent` through `Job::Start`): (agent, ticket, call, layer). The
    /// machine starts such an agent only when it is new or its block had ended (`start_of`); one resting in its block
    /// between steps (a piece's minder) is handed its next step by delivery alone, which a session that went down
    /// meanwhile never takes: P-1729's step 9 and P-1724's step 3 went to minders down since the reboot and stood
    /// unseen (paper-research, N-101 p1778). One that never ran (no session kept) is the machine's to start; one ended
    /// by the owner, moving, handed to, or on a halted ticket is not started; each once until it runs or STARTED_WAIT.
    pub fn to_start_owing(&mut self) -> Vec<(String, u64, String, Option<String>)> {
        const STARTED_WAIT: std::time::Duration = std::time::Duration::from_secs(600);
        let now = std::time::Instant::now();
        self.started_owing.retain(|_, (at, _)| now.duration_since(*at) < STARTED_WAIT);
        let m = &self.tickets.m;
        let mut out = Vec::new();
        for (name, a) in &self.agents {
            if !is_fresh(name) || !harness(&a.kind) || a.running() || a.ended.is_some() || m.ended(name) {
                continue;
            }
            if self.moving.contains(name) || self.inflight.contains_key(name) || self.tickets.stopping.contains(name) || a.live.awaiting_start.is_some() {
                continue;
            }
            if !std::path::Path::new(&format!("{}/agents/{name}/session.json", self.coord)).is_file() {
                continue;
            }
            let mut owed = m.owes(name);
            owed.sort();
            if let Some((t, step, call)) = owed.into_iter().find(|(t, _, _)| m.tickets.get(t).is_some_and(|x| x.is_open() && x.halts.is_empty())) {
                if self.started_owing.get(name).is_some_and(|(_, s)| s.is_none_or(|s| s == (t, step))) {
                    continue;
                }
                out.push((name.clone(), t, step, call, m.layer_of(t)));
            }
        }
        out.into_iter()
            .map(|(n, t, step, call, layer)| {
                self.started_owing.insert(n.clone(), (now, Some((t, step))));
                (n, t, call, layer)
            })
            .collect()
    }

    /// The layer `coord start NAME` starts NAME in: that of the first open step it owes there, while the layer is
    /// active (a step's agent started at the owner's seat that came up in the line's view made a turn no step could
    /// count, paper-research N-101 p1778, 3); none otherwise, the line's view of a boxed project.
    pub fn start_layer(&self, name: &str) -> Option<(u64, String)> {
        let m = &self.tickets.m;
        let active: Vec<String> = m.active_layers().into_iter().map(|(l, _, _)| l).collect();
        let mut owed = m.owes(name);
        owed.sort();
        owed.into_iter().find_map(|(t, _, _)| m.layer_of(t).filter(|l| active.contains(l)).map(|l| (t, l)))
    }

    /// Hand NAME's held mail to its session: a job for its sender (`main::senders`); the mail is on its way until the
    /// sender has the session's answer (`handed`): taken is delivered, not taken is held and tried again.
    pub fn hand_over(&mut self, name: &str) {
        let Some(jobs) = self.jobs.clone() else { return };
        if let Some(job) = self.hand_job(name, false) {
            let _ = jobs.send(job);
        }
    }

    /// The job `hand_over` sends NAME's session; PULL, the one its own pull takes instead (pull.rs), the gate on what is
    /// pushed to a session that pulls (`pushed_to`) not applied.
    pub(crate) fn hand_job(&mut self, name: &str, pull: bool) -> Option<Job> {
        // handed while its turn's reply was still owed: the transcript showed no end in OUTPUT_WAIT (`to_hand_over`)
        if self.agents.get_mut(name).and_then(|a| a.output_owed.take()).is_some() {
            self.reconciled(name, "its hooks ended its turn and its transcript showed no end for two minutes: handed to all the same");
        }
        // nor to a session that is to be restarted for a new effort (`to_restart`)
        if !self.agents[name].running() || self.agents[name].live.restart_due() {
            return None;
        }
        if !harness(&self.agents[name].kind) {
            // a held step no longer owed (its ticket closed, the step withdrawn) is held no more
            let owed: HashSet<String> = self.tickets.m.due(name).into_iter().map(|d| d.id).collect();
            self.runner_unsent.retain(|id, _| owed.contains(id));
            let due = self.due_now(name);
            if due.is_empty() {
                return None;
            }
            let deliveries = self.ticket_handing(name, due);
            return Some(Job::Program { name: name.into(), deliveries });
        }
        // to be moved to another view first (moves.rs): what is due is handed there, once it runs in it
        if self.moving.contains(name) || self.move_due(name).is_some() {
            return None;
        }
        let mut held = self.handed_now(&self.agents[name]);
        // what the delivery program handed, as one message, the rest it names at its foot, and what goes with it by
        // first lines (delivery.rs `Going`); what it made at its `free` waits for the end of a turn running now
        // (`hand_now`'s IN_TURN: a turn begun while fold's call was out)
        let going = if pull {
            // its own pull takes what is due, in a turn or not: the turn is its own, calling read_brain
            delivery::Going { items: if self.tickets.stopping.contains(name) { Vec::new() } else { self.due_now(name) }, ..Default::default() }
        } else if self.pulls(&self.agents[name]) {
            // a session that pulls is pushed only the owner's words, of all that is due, and only while no turn runs
            // (pull.rs, N-1998): not the delivery program's message, which its pull takes
            let idle = !self.tickets.stopping.contains(name) && !self.in_turn(&self.agents[name]);
            delivery::Going { items: if idle { self.due_now(name).into_iter().filter(|d| !d.inform && crate::pull::from_owner(d)).collect() } else { Vec::new() }, ..Default::default() }
        } else if !self.runs_delivery(&self.agents[name]) {
            delivery::Going { items: self.tickets_now(&self.agents[name]), ..Default::default() }
        } else if self.tickets.stopping.contains(name) {
            Default::default()
        } else {
            self.hand_now(name, self.in_turn(&self.agents[name]))
        };
        let delivery::Going { items: mut due, rest, digest, mut given, fork } = going;
        // a `fork` (part 4): its session cut back first, and the message handed to the copy with coord's words ahead
        let forked = match fork {
            Some(place) => match self.fork_now(name, &place) {
                delivery::ForkNow::Restart => return None,
                delivery::ForkNow::Hand(words) => words,
            },
            None => {
                self.forked.remove(name);
                None
            }
        };
        // after a hand-over not answered: its own deliveries first, those still due; when none is, what is due now
        if let Some(again) = (!pull).then(|| self.retry.remove(name)).flatten() {
            let kept: Vec<docket_machine::Delivery> = due.iter().filter(|d| again.contains(&d.id)).cloned().collect();
            if !kept.is_empty() {
                due = kept;
            }
        }
        // a pull takes nothing a push on its way carries (N-1998 p7, 2)
        if pull {
            let flying = self.inflight.get(name).map(|g| g.ids.clone()).unwrap_or_default();
            due.retain(|d| !flying.contains(&d.id));
            given.retain(|d| !flying.contains(&d.id));
            held.retain(|m| !flying.contains(&m.id));
        }
        // and of its held mail the owner's, while no turn runs (its items above)
        if !pull && self.pulls(&self.agents[name]) {
            let idle = !self.in_turn(&self.agents[name]);
            held.retain(|m| idle && m.from == "owner");
            given.clear();
        }
        // its turn stopped on a rate limit that has reset: given again, in coord's words ahead of what else goes, once
        // (`Live::limit`; paper-research N-1397 p142). The limit stays till its session takes this (`handed`), so a
        // coordd restart meanwhile gives it again (consult-3 N-1397 p151). So too with whatever else starts its turn
        // while the limit stands, his own post under his Hold among them: his Hold keeps back the resume as a turn of
        // its own, not that its last turn was cut; told there, the resume is spent (proto-lead N-1397 p189)
        let starts = !self.in_turn(&self.agents[name]) && (!held.is_empty() || !due.is_empty());
        let resume = if !pull && (self.resume_due(&self.agents[name]) || starts) { self.agents[name].live.limit.clone() } else { None };
        if held.is_empty() && due.is_empty() && resume.is_none() {
            return None;
        }
        if let Some(l) = &resume {
            match &l.served {
                Some(by) => eprintln!("coordd: {name}: given its turn again: its model is served again since the rate limit that stopped it at {} ({by})", at_secs(l.at)),
                None => eprintln!("coordd: {name}: given its turn again: the rate limit that stopped it at {} has reset", at_secs(l.at)),
            }
        }
        let mut ids: Vec<String> = held.iter().map(|m| m.id.clone()).collect();
        ids.extend(due.iter().map(|d| d.id.clone()));
        // the same count the rule Reply takes at the turn's end, with what is handed now (paper-research m988, 3)
        let due_ids: Vec<String> = due.iter().map(|d| d.id.clone()).collect();
        let replies_on = if due.is_empty() { None } else { self.ticket_replies_on(name, &due_ids) };
        let again: Vec<String> = due_ids.iter().filter(|id| self.ticket_handed_before(name, id)).cloned().collect();
        // put into the turn running (N-101 p1214): taken, given as such (`GivenInto`); the turn's last message is not
        // posted for them, so the footer asks for the reply tool
        let into = self.in_turn(&self.agents[name]) && !due.is_empty();
        let replies_on = if into { None } else { replies_on };
        if into {
            for id in &due_ids {
                self.tickets.into.insert((name.to_string(), id.clone()));
            }
        }
        let tickets = self.ticket_handing(name, due);
        // the digest's deliveries: handed with the message, and given when it is (`ticket_handed`), so the read mark
        // passes what its foot lists (I-1480, N-1481 p26)
        ids.extend(self.ticket_handing(name, given).into_iter().map(|(d, _)| d.id));
        let a = &self.agents[name];
        let job = if served(&a.kind) {
            let text = if held.is_empty() { String::new() } else { self.batch_text(name, &held) };
            let text = match resume.as_ref().and_then(|l| resume_words(&self.places_for(name), l)) {
                Some(r) if text.is_empty() => r,
                Some(r) => format!("{r}\n\n{text}"),
                None => text,
            };
            let effort = a.model.split_once(':').map(|(_, e)| e.to_string());
            Job::Codex { name: name.into(), ids: ids.clone(), rpc: rpc_bin(&self.base, &a.kind), session: s(&a.session, "session_id"), socket: a.socket(), text, tickets, replies_on, again, effort, into, rest, digest }
        } else {
            // old mail goes as before, an event each ahead of the tickets, not in their one message (render::handover):
            // none comes now (research's mail log, read 20 Nov: the last went 2026-11-12T18:41:42Z, m1641)
            let mut events: Vec<Value> = resume.iter().filter_map(|l| resume_words(&self.places_for(name), l)).map(|r| crate::render::event(r, json!({"from": "coord"}))).collect();
            events.extend(forked.iter().map(|w| crate::render::event(w.clone(), json!({"from": "coord"}))));
            events.extend(held.iter().map(|m| crate::render::event(self.delivery_text(m, name), json!({"from": m.from, "id": m.id}))));
            let socks = channel_socks(&self.path(&format!("agents/{name}")), &s(&a.session, "session_id"));
            Job::Channel { name: name.into(), ids: ids.clone(), socks, line: format!("{}\n", json!({"events": events})), tickets, replies_on, again, into, rest, digest }
        };
        // a pull's are its own: a push's tries, its log lines and its entry stay as they are
        if pull {
            return Some(job);
        }
        let tries = self.retry_tries.remove(name).unwrap_or(0);
        for id in &ids {
            append(&self.path("mail/log.jsonl"), &json!({"ev": "sent", "id": id, "to": name, "at": now(), "try": tries + 1}));
        }
        self.inflight.insert(name.into(), Inflight { ids, tries: tries + 1, resume });
        Some(job)
    }

    /// Where A's model is reached: the owner's account it runs on (its id), else the gateway.
    fn place_of(a: &Agent) -> &str {
        match &a.account {
            Some(id) if a.native && a.gateway.is_none() => id,
            _ => crate::accounts::router::GATEWAY,
        }
    }

    /// PLACE served MODELS ([`Served`]) for what was sent at SENT, BY saying how (`the gateway answered docket-core's
    /// claude-opus-5-5 at 11:06`): each agent held on a rate limit (`Live::limit`) at that place on one of those
    /// models, whose turn stopped before SENT, is due its turn again now, not at the reset its 429 named (paper-research
    /// N-1446 p873: sixteen agents stopped at 11:01 on 21 Nov on Opus's cooldown "till 13:31", his own session was
    /// answered on Opus through the gateway from 11:06, and none went on until he restarted them at 11:08).
    /// `resume_due` and the hand-over that follows give it as at a reset, once: the resume takes the limit. What serves
    /// is real work only, an agent's turn, coordd's own call or his press, never a call made to see (proto-lead p886):
    /// when every session on the model is held and he presses nothing, they wait for the reset. The agents given again.
    pub fn served_again(&mut self, place: &str, models: Served, sent: i64, by: &str) -> Vec<String> {
        use crate::accounts::router::{gateway_family, served_model};
        let now = crate::accounts::mover::now_s();
        let again: Vec<String> = self
            .agents
            .values()
            .filter(|a| a.live.limit.as_ref().is_some_and(|l| l.until > now && l.at < sent) && Self::place_of(a) == place)
            .filter(|a| match models {
                Served::Model(m) => served_model(m) == served_model(&a.model),
                Served::Family(f) => gateway_family(&self.coord, &a.model).as_deref() == Some(f),
                Served::Every => true,
            })
            .map(|a| a.name.clone())
            .collect();
        for name in &again {
            let l = self.agents.get_mut(name).unwrap().live.limit.as_mut().unwrap();
            eprintln!("coordd: {name}: held on a rate limit till {}, but {by}: given its turn again now", at_secs(l.until));
            (l.until, l.served) = (sent.min(now), Some(by.to_string()));
            self.changed(name);
        }
        if !again.is_empty() {
            self.journal();
            self.poke.notify_one();
        }
        again
    }

    /// His press cleared a gateway login's cooldown, of a login whose type is FAMILY (`Request::GatewayCleared`: the
    /// window's `⋯ Clear cooldown`, or his reset of a gateway login that went through; paper-research N-1446 p957): the
    /// gateway serves that family's models again, as of the press ([`Core::served_again`], [`Served::Family`]). The
    /// owner's only.
    pub fn gateway_cleared(&mut self, who: &Who, family: &str) -> Reply {
        if !matches!(who, Who::Owner) {
            return err("gateway: the owner's".into());
        }
        let (family, now) = (family.trim().to_lowercase(), crate::accounts::mover::now_s());
        let by = format!("his press cleared a {family} login's cooldown on the gateway at {}", at_secs(now));
        let again = self.served_again(crate::accounts::router::GATEWAY, Served::Family(&family), now, &by);
        if again.is_empty() {
            return ok(format!("gateway: none held on {family} models"));
        }
        ok(format!("gateway: {} held on {family} models go on ({})", again.len(), again.join(", ")))
    }

    /// The 200s coordd's own calls got since the last pass (`router::served_taken`): [`Core::served_again`] for each
    /// place and model.
    fn serves_again(&mut self) {
        for ((place, model), sent) in crate::accounts::router::served_taken(&self.coord) {
            let way = if place == crate::accounts::router::GATEWAY { String::from("the gateway") } else { format!("account {place}") };
            self.served_again(&place, Served::Model(&model), sent, &format!("{way} answered coordd's own call of {model} at {}", at_secs(sent)));
        }
    }

    /// NAME's session showed replies the API gave (`Follower::take_answers`: each one's model, and when the input it
    /// answers was written): [`Core::served_again`] for each, where NAME's model is reached. Not while NAME is held
    /// itself: a reply in its own session is a turn of its own, which ends its hold the usual way.
    pub fn answered(&mut self, name: &str, answers: &[(String, String)]) {
        if answers.is_empty() || !self.agents.values().any(|a| a.live.limit.is_some()) {
            return;
        }
        let Some(place) = self.agents.get(name).filter(|a| a.live.limit.is_none()).map(|a| Self::place_of(a).to_string()) else { return };
        // the latest of each model: one pass each
        let mut latest: BTreeMap<&str, i64> = BTreeMap::new();
        for (model, asked) in answers {
            if let Ok(t) = chrono::DateTime::parse_from_rfc3339(asked) {
                let at = latest.entry(model.as_str()).or_insert(t.timestamp());
                *at = (*at).max(t.timestamp());
            }
        }
        let way = if place == crate::accounts::router::GATEWAY { String::from("the gateway") } else { format!("account {place}") };
        for (model, sent) in latest {
            self.served_again(&place, Served::Model(model), sent, &format!("{way} answered {name}'s {model} at {}", at_secs(sent)));
        }
    }

    /// Whether A's turn stopped on a rate limit is to be given again now (`Live::limit`): the limit has reset, no turn
    /// runs, none is being stopped and its context is not being compacted, and the owner's hold on agents' mail is off
    /// (he holds it to be the one who sends them messages: paper-research N-1396 p235).
    fn resume_due(&self, a: &Agent) -> bool {
        harness(&a.kind)
            && a.live.limit.as_ref().is_some_and(|l| l.until <= crate::accounts::mover::now_s())
            && !self.in_turn(a)
            && !a.live.compacting
            && !self.tickets.stopping.contains(&a.name)
            && self.tickets.m.mail_hold.is_none()
    }

    /// NAME, held on a rate limit out of a turn (`Live::limit`), is stopped (WHY: his `coord esc`, an esc on the record):
    /// the turn it is held for is not given again, and a 429 after this waits from key-again-after (proto-lead N-1397
    /// p189: his Esc cancels the resume; his row says "idle since …" again). False when it is not held.
    pub fn limit_dropped(&mut self, name: &str, why: &str) -> bool {
        if !self.agents.get(name).is_some_and(|a| a.live.limit.is_some() && !self.in_turn(a)) {
            return false;
        }
        let a = self.agents.get_mut(name).unwrap();
        let until = a.live.limit.take().map_or(0, |l| l.until);
        a.live.limit_wait = None;
        self.reconciled(name, &format!("held on a rate limit till {}, stopped by {why}: that turn is not given again", at_secs(until)));
        self.journal();
        self.changed(name);
        true
    }

    /// Whether NAME has a delivery handed without an id that its transcript has not shown yet: while it has, nothing
    /// more is rendered without one, so the position of an id-less entry names one delivery only.
    pub fn bare_pending(&self, name: &str) -> bool {
        self.bare.contains_key(name)
    }

    /// Delivery ID went to NAME without an id, as CONTENT: its place in the transcript is the first id-less coord
    /// channel entry after where the transcript ends now (paper-research m1114: by position, the body the check).
    pub fn bare_handed(&mut self, name: &str, id: &str, content: &str) {
        // a Claude Code session's transcript only: a Codex thread's words carry their head ([from X]), not a place
        let Some(file) = self.agents.get(name).filter(|a| a.kind == "claude").and_then(|a| a.transcript()) else { return };
        let from = std::fs::metadata(&file).map_or(0, |m| m.len());
        let b = Bare { id: id.into(), text: content.trim().into(), file, from };
        if let Some(old) = self.bare.insert(name.into(), b) {
            eprintln!("coordd: {name}: {} handed without an id while {} was never seen; that one keeps no place", id, old.id);
        }
    }

    /// NAME's transcript shows an id-less coord channel entry BODY at LOC: the delivery handed before it, if the body
    /// is what was sent ([`Core::entry_is`]); its place is then LOC.
    pub fn bare_seen(&mut self, name: &str, body: &str, loc: docket_machine::Loc) {
        for id in self.entry_is(name, &[], Some(body), &loc) {
            self.ticket_seen(name, &id, loc.clone());
        }
    }

    /// Which deliveries an entry of NAME's session at LOC is: the one answer that the place a giving is found at, its
    /// delivered mark, and so the minute's check of what was given and not read (`not_read`), all take, for an entry
    /// with an id and for the owner's words alone (paper-research N-1446 p3215). IDS when it carries any (a ticket
    /// delivery's, with a dot). With none, the owner's words BODY are the delivery handed without an id before LOC
    /// (`bare_handed`) when BODY is what was sent (a mismatch is logged and nothing taken: never guessed); else, as the
    /// prompt that takes them out of the queue (no BODY), the giving to NAME already placed at LOC, their enqueue, so
    /// that it is delivered as an entry with an id would be. Before, words alone were placed but never delivered: a
    /// minute's check, or one right after the turn they began ended, called them lost, handed them again as "your
    /// session did not take it up", and the turn's answer was posted nowhere (docket-edge7's run on eng-79, p3214).
    fn entry_is(&mut self, name: &str, ids: &[String], body: Option<&str>, loc: &docket_machine::Loc) -> Vec<String> {
        if !ids.is_empty() {
            return ids.iter().filter(|id| id.contains('.')).cloned().collect();
        }
        if let (Some(body), Some(b)) = (body, self.bare.get(name))
            && loc.file == b.file
            && loc.offset >= b.from
        {
            if body.trim() != b.text {
                eprintln!("coordd: {name}: an id-less entry at {}:{} is not {} as sent; not taken", loc.file, loc.offset, b.id);
                return Vec::new();
            }
            return vec![self.bare.remove(name).unwrap().id];
        }
        let same = |l: &docket_machine::Loc| l.file == loc.file && l.offset == loc.offset;
        self.tickets.m.given_rows(name).into_iter().filter(|g| g.delivered.is_none() && g.place.as_ref().is_some_and(same)).map(|g| g.delivery).collect()
    }

    /// The sender's answer for NAME's mail IDS: its session took them (Codex: delivered; Claude Code: now waiting to
    /// be seen in the transcript), or did not (held again, tried again after a pause).
    /// NAME's hand-over reached no channel socket of its session (WHY, `channel_unreached`): it takes nothing, and
    /// reads as stopped, until its folder changes or a new process starts (`Live::unreached`).
    pub fn unreached(&mut self, name: &str, why: &str) {
        if let Some(a) = self.agents.get_mut(name).filter(|a| a.running()) {
            a.live.unreached = Some((why.to_string(), std::time::SystemTime::now()));
        }
    }

    /// Each agent marked unreached whose folder has changed since (channel.py binds its socket there): tried again.
    fn reached_again(&mut self) {
        let coord = self.coord.clone();
        for a in self.agents.values_mut() {
            let Some((_, at)) = &a.live.unreached else { continue };
            if std::fs::metadata(format!("{coord}/agents/{}", a.name)).and_then(|m| m.modified()).is_ok_and(|m| m > *at) {
                a.live.unreached = None;
            }
        }
    }

    pub fn handed(&mut self, name: &str, ids: &[String], took: bool, how: &str) {
        // what its own pull took: given and its mail marked, and nothing of a push's touched (N-1998 p7, 2)
        if how == "pull" {
            self.ticket_handed(name, ids, took);
            let ms: Vec<Mail> = ids.iter().filter_map(|id| self.find_mail(id).cloned()).collect();
            self.mark(name, &ms, how);
            self.changed(name);
            return;
        }
        // pushed to an agy session and taken: its next pull's work, its report's to answer (N-1998 p7, 1)
        if let Some(a) = self.agents.get_mut(name).filter(|a| took && a.kind == "agy") {
            a.live.pushed.extend(ids.iter().filter(|id| !a.live.pushed.contains(id)).cloned().collect::<Vec<_>>());
        }
        if !self.inflight.get(name).is_some_and(|g| g.ids == ids) {
            // seen meanwhile, or handed again; a ticket delivery seen is given already (`ticket_seen`)
            self.ticket_handed(name, ids, took);
            return;
        }
        self.ticket_handed(name, ids, took);
        if !took {
            // not taken: no step was begun, and the retry hands it again
            // a fresh agent ended meanwhile is not started again: what it did not take is dropped, not retried
            if is_fresh(name) && !self.agents.get(name).is_some_and(|a| a.running()) {
                self.inflight.remove(name);
                let dropped = self.ticket_drop_ended(name);
                self.reconciled(name, &format!("ended; {} dropped, not handed again", if dropped.is_empty() { ids.join(" ") } else { dropped.join(" ") }));
                return;
            }
            // the turn it gave again is given again with the retry: its limit is still there (`Live::limit`)
            let g = self.inflight.remove(name).unwrap();
            // the next hand-over gives these first, so a session that took them unanswered is not given them twice
            let tickets: Vec<String> = ids.iter().filter(|id| id.contains('.')).cloned().collect();
            if !tickets.is_empty() {
                self.retry.insert(name.into(), tickets);
            }
            let pause = Duration::from_secs((5u64 << g.tries.min(4)).min(60));
            self.retry_at.insert(name.into(), std::time::Instant::now() + pause);
            let what = if ids.is_empty() { String::from("its turn given again") } else { ids.join(" ") };
            self.reconciled(name, &format!("its session did not take {what}; again in {} s", pause.as_secs()));
            self.retry_tries.insert(name.into(), g.tries); // the next hand-over counts on from them
            return;
        }
        // taken is delivered (design coordd-core-2026-11-10 §4): the session has it, queued behind a menu or not; a
        // retry's pause is over (the delivery events read it as starting, delivery.rs)
        let resumed = self.inflight.remove(name).and_then(|g| g.resume);
        // the turn it gave again is its session's now: that limit is over, and not a newer one (`Inflight::resume`)
        if let Some(a) = self.agents.get_mut(name).filter(|a| resumed.is_some() && a.live.limit == resumed) {
            a.live.limit = None;
        }
        self.retry_tries.remove(name);
        self.retry_at.remove(name);
        let ms: Vec<Mail> = ids.iter().filter_map(|id| self.find_mail(id).cloned()).collect();
        self.mark(name, &ms, how);
        if how == "codex" {
            self.begin_turn(name);
        }
        self.changed(name);
    }

    /// NAME's transcript showed mail IDS: delivered, if no acknowledgement said so already (a session from before).
    pub fn seen(&mut self, name: &str, ids: &[String]) {
        let mut done = Vec::new();
        for id in ids {
            let key = (id.clone(), name.to_string());
            if self.delivered.contains(&key) {
                continue;
            }
            // a queued message and its entry may come in one read: once
            if let Some(m) = self.find_mail(id).filter(|m| m.to.iter().any(|t| t == name) && !done.iter().any(|d: &Mail| d.id == m.id)).cloned() {
                done.push(m);
            }
        }
        if let Some(g) = self.inflight.get_mut(name) {
            g.ids.retain(|i| !ids.contains(i));
            if g.ids.is_empty() {
                self.inflight.remove(name);
                self.retry_tries.remove(name);
            }
        }
        if !done.is_empty() {
            self.mark(name, &done, "channel");
            self.changed(name);
        }
    }

    /// A notice from coord (S5): an agents' thread opened by `coord` naming TO (Sent(coord), design part 9b), which ends
    /// at once and reaches TO.
    async fn notice(&mut self, to: &str, body: &str) {
        self.note_from_coord(to, body);
    }

    /// `Request::Hold`: the owner holds every hand-over (ON) or lets them go again, for a return to an earlier release
    /// (paper-research N-101.p1309, 1: the return's wait for turns to end never ended while this coordd still handed
    /// posts). A journal line says it.
    pub fn hold_deliveries(&mut self, who: &Who, on: bool) -> Reply {
        if who.name() != "owner" {
            return err("coord hold: the owner's, for a return (release.sh coord-back --esc)".into());
        }
        self.holding = on;
        eprintln!("coordd: deliveries {} (coord hold {})", if on { "held: nothing is handed" } else { "handed again" }, if on { "on" } else { "off" });
        self.poke.notify_one();
        ok(if on { "Deliveries held: nothing is handed until coord hold off, or coordd restarts.".into() } else { "Deliveries handed again.".into() })
    }

    /// `Request::Notice`: the owner's tool sends agent TO a notice as coord's (paper-research N-101.p1309, 2: release.sh's
    /// "go on" was the owner's and opened a thread in his stream).
    pub fn coord_notice(&mut self, who: &Who, to: &str, text: &str) -> Reply {
        if who.name() != "owner" {
            return err("coord notice: the owner's tools'".into());
        }
        if !self.agents.contains_key(to) {
            return err(format!("coord notice: no agent named {to}"));
        }
        self.note_from_coord(to, text);
        ok(format!("a notice from coord to {to}"))
    }

    /// Each agent group's `oom_kill` count now (`agents/NAME/memory.events`); a group without the file has none.
    pub fn oom_counts() -> BTreeMap<String, u64> {
        let dir = crate::cgroup::root().join("agents");
        let mut out = BTreeMap::new();
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten().filter(|e| e.path().is_dir()) {
            let Ok(text) = std::fs::read_to_string(e.path().join("memory.events")) else { continue };
            let n = text.lines().find_map(|l| l.strip_prefix("oom_kill ")).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
            out.insert(e.file_name().to_string_lossy().into_owned(), n);
        }
        out
    }

    /// The counts COUNTS against the last read: each agent whose count rose is told once that a process of its box
    /// was killed at its memory limit, with the project's `box_memory_hint`, on the ticket of the step it owes, else
    /// of its layer, else as coord's notice (paper-research N-101 p1785). The counts are kept for the next round.
    pub fn oom_round(&mut self, counts: BTreeMap<String, u64>) {
        for (name, n) in &counts {
            let before = self.oom_seen.get(name).copied();
            if *n <= before.unwrap_or(0) || !self.agents.contains_key(name) {
                continue;
            }
            let p = self.project.as_ref();
            let boxed = p.is_some_and(|p| p.project.boxed) && self.agents.get(name).is_some_and(|a| a.kind != "program");
            let limit = p.and_then(|p| p.agent_limits(name, boxed).memory).map(|m| format!(" ({m})")).unwrap_or_default();
            let hint = p.and_then(|p| p.project.box_memory_hint.clone()).filter(|h| !h.trim().is_empty()).map(|h| format!("; {}", h.trim())).unwrap_or_default();
            let text = format!("A process of yours was killed at your box's memory limit{limit}{hint}.");
            let on = self.tickets.m.awaiting().into_iter().find(|(_, _, a, _)| a == name).map(|(t, ..)| t).or_else(|| crate::found::layer_of(&self.coord, name).and_then(|l| docket_machine::parse_ticket(&l.ticket)));
            match on {
                Some(t) => {
                    let input = docket_machine::Input::Post { t, by: "coord".into(), text: docket_machine::Text::New(text.clone()), names: vec![name.clone()], told: vec![name.clone()], inform: Vec::new(), answers: None, kind: None };
                    if let Err(e) = self.ticket_input(input) {
                        eprintln!("coordd: the memory notice to {name} on {}: {e}", self.tickets.m.id(t));
                        self.note_from_coord(name, &text);
                    }
                }
                None => self.note_from_coord(name, &text),
            }
        }
        self.oom_seen = counts;
    }

    pub(crate) fn note_from_coord(&mut self, to: &str, body: &str) {
        let names = vec![to.to_string()];
        let form = crate::ticket::sent_form("coord", &names, body);
        let input = docket_machine::Input::Open { kind: "thread".into(), under: None, opener: "coord".into(), form, text: Some(docket_machine::Text::New(body.to_string())), names, told: Vec::new() };
        if let Err(e) = self.ticket_input(input) {
            eprintln!("coordd: a notice to {to}: {e}");
        }
    }


    /// esc: stop NAME's running turn, by its sender (`main::senders`). Claude Code: the one keystroke Escape, typed
    /// through its terminal's holder (`coord-pty`), after which its transcript shows the turn ended; Codex: interrupt.
    pub(crate) fn interrupt(&mut self, name: &str) {
        let Some(jobs) = self.jobs.clone() else { return };
        let a = &self.agents[name];
        let codex = served(&a.kind).then(|| (rpc_bin(&self.base, &a.kind), s(&a.session, "session_id"), a.socket()));
        let _ = jobs.send(Job::Interrupt { name: name.into(), pty: coord_types::pty::socket(&self.coord, name).to_string_lossy().into_owned(), codex });
    }

    pub fn mail_list(&self, who: &Who, id: Option<&str>) -> Reply {
        let me = who.name();
        if let Some(id) = id {
            let Some(m) = self.find_mail(id) else { return err(format!("coord mail: no mail {id}")) };
            let mut out = format!("{}  {}  from {} to {}{}\n", m.id, m.at, m.from, m.to.join(", "), if m.esc { "  (esc)" } else { "" });
            for t in &m.to {
                let key = (m.id.clone(), t.clone());
                let got = if self.delivered.contains(&key) { "delivered" } else if self.withdrawn.contains(&key) { "withdrawn" } else { "held" };
                out += &format!("  {t}: {got}\n");
            }
            out += &m.body;
            return ok(out);
        }
        let lines: Vec<String> = self
            .mails
            .iter()
            .filter(|m| me == "owner" || m.from == me || m.to.contains(&me))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .take(40)
            .rev()
            .map(|m| format!("{:>6} {} {} -> {}: {}", m.id, m.at.get(11..16).unwrap_or(""), m.from, m.to.join(","), subject(&m.body)))
            .collect();
        ok(lines.join("\n"))
    }

    pub fn state_lines(&self) -> Reply {
        // a fifth column only when its box could not be made: roster reads the third, the state
        let lines: Vec<String> = self.agents.values().map(|a| format!("{}\t{}\t{}\t{}{}", a.name, a.id, a.live.state, a.parent, a.ended.as_ref().map(|at| format!("\tended by the owner at {at}")).or_else(|| a.no_box.as_ref().map(|w| format!("\tnot started, no box: {w}"))).unwrap_or_default())).collect();
        ok(lines.join("\n"))
    }

    /// Whether WHO may read NAME's session: an agent above it, or one the norms let (read-session; refused when the
    /// norms cannot say). `coord log`, and `coord fork` of another agent (item 11), which copies it.
    pub(crate) fn may_read_session(&self, who: &Who, name: &str) -> bool {
        self.above(who, name)
            || match who {
                Who::Agent(me) => matches!(self.norm_verdict("read-session", &[me, name], "none"), Ok(Some(v)) if v.enabled),
                _ => false,
            }
    }

    pub fn log(&self, who: &Who, name: &str, last: usize) -> Reply {
        let name = &match self.not_across("log", name) {
            Ok(n) => n,
            Err(e) => return err(e),
        };
        let name = name.as_str();
        let Some(a) = self.agents.get(name) else { return err(format!("coord log: no agent {name}")) };
        // beyond the agents above it, whom the norms let read it (read-session: the consultant)
        // (a permission to read: refused when the norms cannot say, unlike mail, which goes through)
        if !self.may_read_session(who, name) {
            return err(format!("coord log: only the agents above {name}, and those the norms let (read-session), read its session"));
        }
        let Some(path) = a.transcript() else { return err(format!("coord log: {name} has no session yet")) };
        ok(transcript::log_text(name, &path, last.max(1)))
    }

    /// The session a page of NAME's is read from, for yaaihh (agents above NAME, as for `coord log`): its current one,
    /// or SESSION, another of its transcripts ([`one_of_its_sessions`]; `Request::Transcript`'s `session`), with his
    /// system-prompt edits that fall in it (docket-machine `Builder::restarted`), the session each from a turn went
    /// off given as its transcript's path beside it (path fix 6 point 3); None while it has none. The page itself is
    /// read off the lock (main.rs `transcript_page`, paper-research N-1446 p2313).
    pub fn transcript_of(&self, who: &Who, name: &str, session: Option<&str>) -> Result<Option<(String, Vec<docket_machine::graph::Restarted>)>, String> {
        let Some(a) = self.agents.get(name) else { return Err(format!("coord transcript: no agent {name}")) };
        if !self.above(who, name) {
            return Err(format!("coord transcript: only the agents above {name} read its session"));
        }
        let Some(current) = a.transcript() else { return Ok(None) };
        let path = match session {
            Some(s) if !one_of_its_sessions(&current, s) => return Err(format!("coord transcript: {s} is not one of {name}'s sessions")),
            Some(s) => s.to_string(),
            None => current,
        };
        let at = std::path::Path::new(&path);
        let id = at.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        let mut marks = self.tickets.graph.restarted(name, id).to_vec();
        for l in marks.iter_mut().filter_map(|r| r.line.as_mut()) {
            l.from = at.with_file_name(format!("{}.jsonl", l.from)).display().to_string();
        }
        Ok(Some((path, marks)))
    }

    /// Where the proxy's log of agent NAME's requests is: the project's `proxy_log` folder as the file writes it, and
    /// NAME's Claude Code session id, which the proxy logs as X-Claude-Code-Session-Id. `RawPage` reads the log here,
    /// off the lock, with coordd's own home (main.rs `raw_page`, N-802); a window from before it reads the log on its
    /// host (N-40), and gets the folder as the file writes it.
    /// Codex sessions are not told apart in that log.
    ///
    /// An opencode agent calls its providers itself: its calls are the files its plugin writes in its coord folder,
    /// `raw/` beside its session (agent-base `opencode/plugin/coord.js`; opencode-rpc's `$CODEX_SOCK`), under its
    /// opencode session's id. The project's proxy_log is not needed for it.
    pub fn proxy_source(&self, who: &Who, name: &str) -> Result<coord_types::RawSource, String> {
        let a = self.agents.get(name).ok_or(format!("coord raw: no agent {name}"))?;
        if !self.above(who, name) {
            return Err(format!("coord raw: only the agents above {name} read its session"));
        }
        if a.kind == "agy" {
            return Err(format!("coord raw: {name} is an agy agent; its requests go from agy (through t3code on a sign-in account) to Google and are written nowhere of ours"));
        }
        if a.kind == "opencode" {
            let session = a.session.get("session_id").and_then(Value::as_str).filter(|_| !a.socket().is_empty()).ok_or(format!("coord raw: {name} has no session yet"))?;
            return Ok(coord_types::RawSource { dir: format!("{}/raw", a.socket().trim_end_matches('/')), session: session.to_string() });
        }
        let p = self.project.as_ref().ok_or("coord raw: the proxy's log is named in the project file, and there is none")?;
        let dir = p.project.proxy_log.clone().ok_or("coord raw: the project file names no proxy_log folder")?;
        if a.kind == "codex" {
            return Err(format!("coord raw: {name} is a Codex agent; the proxy's log does not say which Codex session a request is for"));
        }
        let session = a
            .session
            .get("session_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| a.transcript().and_then(|t| std::path::Path::new(&t).file_stem().map(|s| s.to_string_lossy().into_owned())))
            .ok_or(format!("coord raw: {name} has no session yet"))?;
        Ok(coord_types::RawSource { dir, session })
    }

    /// Agent NAME's kind and session file, for W above it: what its raw pane reads when no gateway log or record does.
    pub fn raw_file(&self, who: &Who, name: &str) -> Result<(String, Option<String>), String> {
        let a = self.agents.get(name).ok_or(format!("coord raw: no agent {name}"))?;
        if !self.above(who, name) {
            return Err(format!("coord raw: only the agents above {name} read its session"));
        }
        Ok((a.kind.clone(), a.transcript()))
    }

    /// Where agent NAME's raw pane reads from when no gateway's log holds its requests (fix 5, accounts-lead N-1446
    /// p2446, p2502): a Claude Code agent's session file, its own record of each request (`transcript::record_page`),
    /// with what is missing in his words. So for one on one of his own accounts and not riding the gateway meanwhile
    /// (roster starts it with nothing of the route), whatever the project names; and for every one when the project
    /// names no proxy_log. None when its requests are in the log, when it is no Claude Code agent with a session, or
    /// when W is not above it.
    pub fn record_source(&self, who: &Who, name: &str) -> Option<(String, String)> {
        let a = self.agents.get(name).filter(|a| a.kind == "claude" && self.above(who, name))?;
        // two lines over the pane, in his words (paper-research N-1446 p2494)
        let about = if a.native && a.account.is_some() && a.gateway.is_none() {
            "This agent runs on your own account, not through the gateway, so this is its own session record of each request: system prompt, tools, messages, settings and the answer. It has no HTTP status."
        } else if self.project.as_ref().is_some_and(|p| p.project.proxy_log.is_some()) {
            return None;
        } else {
            "No gateway log in this project, so this is the agent's own session record of each request: system prompt, tools, messages, settings and the answer. It has no HTTP status and nothing a gateway adds."
        };
        Some((a.transcript()?, about.to_string()))
    }

    /// Image N of the line at OFFSET of NAME's transcript SESSION (a file its sessions were in: the current one or an
    /// earlier one of the same agent's, as a chat may still show an older part).
    pub fn transcript_image(&self, who: &Who, name: &str, session: &str, offset: u64, n: u32) -> Reply {
        let Some(a) = self.agents.get(name) else { return err(format!("coord image: no agent {name}")) };
        if !self.above(who, name) {
            return err(format!("coord image: only the agents above {name} read its session"));
        }
        let Some(current) = a.transcript() else { return err(format!("coord image: {name} has no session")) };
        if !one_of_its_sessions(&current, session) {
            return err(format!("coord image: {session} is not one of {name}'s sessions"));
        }
        match transcript::image(session, offset, n) {
            Some((media, data)) => ok(serde_json::json!({"media": media, "data": data}).to_string()),
            None => err(format!("coord image: no image {n} at byte {offset} of {session}")),
        }
    }

    // ---- items -------------------------------------------------------------------------------------------------

    /// The open items agent NAME said it works on (`coord item start`: the item tickets' started lines, E2).
    /// Agent NAME's ticket as its view names it and its open items: read again only when the machine has changed since
    /// they were last read for it (its `changes`, which every ticket write counts), else as kept.
    pub fn on_and_items(&self, name: &str) -> (Option<String>, Vec<String>) {
        let key = self.tickets.m.changes();
        if let Some((k, on, items)) = self.view_kept.borrow().get(name) {
            if *k == key {
                return (on.clone(), items.clone());
            }
        }
        let on = docket_machine::label::agent_on(&self.tickets.m, name, &crate::label::read).map(|t| self.tickets.m.id(t));
        let items = self.items_of(name);
        self.view_kept.borrow_mut().insert(name.to_string(), (key, on.clone(), items.clone()));
        (on, items)
    }

    pub fn items_of(&self, name: &str) -> Vec<String> {
        self.tickets.m.tickets.values().filter(|x| x.kind == "item" && x.is_open() && x.started.iter().any(|s| s == name)).map(|x| self.tickets.m.id(x.t)).collect()
    }

    /// Agent NAME's hooks say a turn runs (BUSY) or none (`Live::hook_busy`); an interrupt's entry in the transcript
    /// says none too, since it runs no hook. The transcript's reader (`main::follow`) takes it, woken for it.
    pub fn hook_busy(&mut self, name: &str, busy: bool) {
        if let Some(a) = self.agents.get_mut(name).filter(|a| a.kind == "claude") {
            if a.live.hook_busy != Some(busy) {
                a.live.hook_busy = Some(busy);
                self.poke.notify_one();
                self.journal();
            }
        }
    }

    /// Whether NAME's hooks and screen agree on a running turn; a disagreement that lasts 5 s is logged once
    /// (reconcile.jsonl), so the fallback's worth is counted (paper-research m1162, 9).
    /// The hooks saying a turn runs while the screen has shown none for 60 s are the stale side: a turn's end that
    /// could not reach coordd (a box holding the socket of a coordd since restarted: C-1616's orchestrator, N-101.p1270)
    /// left it working for ever. The screen decides then, and the turn ends; a turn the screen shows again begins again.
    /// GREW: when NAME's transcript last grew. While it grows the turn runs whatever the screen shows, and the hooks
    /// decide (Tovrin-dev, 17 Nov: 14 such ends, each a turn still running, its end 2 to 50 s later; f1655-1 was given
    /// its step again into it; paper-research N-101 p1459: a wait lasts while the awaited process lives).
    /// ENDED: the transcript has the turn's end (`follow::Follower::ended`). A turn whose Stop hook could not reach
    /// coordd has written its end there; one writing a long final answer has not, and Claude Code writes the answer
    /// only once it is whole, so the transcript does not grow for a minute or more while the screen may read idle
    /// (Tovrin-dev, 17 Nov: B-ROUTES, B-EVIDENCE, B-CUTOFF, a `thinking` entry then 70 to 110 s of silence, the Stop hook
    /// 10 to 49 s after the screen decided; docket-core5, N-101 p1550). The screen decides only once the transcript too
    /// says the turn is over.
    /// UNANSWERED: the transcript ends in a turn begun on a prompt that nothing of the model's has followed
    /// (`Follower::unanswered`). A session resumed onto a background task's notice wrote it and sat at its prompt, the
    /// screen idle, no Stop to come and an esc ending nothing: the hooks decided for 75 and 87 min (research, 25 Nov
    /// 21:32 CET, docket-core and docket-edge5; its child check died unseen). After UNANSWERED the screen decides.
    pub fn presence_agrees(&mut self, name: &str, hooks: Option<bool>, screen: Option<bool>, grew: std::time::Instant, ended: bool, unanswered: bool) {
        let Some(a) = self.agents.get_mut(name) else { return };
        match (hooks, screen) {
            (Some(true), Some(false)) if unanswered && a.live.disagree.is_some_and(|(since, _)| since.elapsed() >= UNANSWERED) && grew.elapsed() >= UNANSWERED => {
                a.live.disagree = None;
                self.hook_busy(name, false);
                self.reconciled(name, "the hooks say a turn running, the screen none, and the transcript ends in a prompt nothing of the model's has followed, unchanged for 300 s; the screen decides: the turn never ran, and what waits for it is handed");
            }
            (Some(true), Some(false)) if a.live.disagree.is_some_and(|(since, _)| since.elapsed() >= STALE_HOOKS) && grew.elapsed() >= STALE_HOOKS && ended => {
                a.live.disagree = None;
                self.hook_busy(name, false);
                self.reconciled(name, "the hooks say a turn running, the screen none, and the transcript has the turn's end and has not grown, for 60 s; the screen decides: no Stop hook came");
            }
            (Some(h), Some(sc)) if h != sc => {
                let (since, told) = *a.live.disagree.get_or_insert((std::time::Instant::now(), false));
                if !told && since.elapsed() >= Duration::from_secs(5) {
                    a.live.disagree = Some((since, true));
                    let say = |b: bool| if b { "a turn running" } else { "none" };
                    self.reconciled(name, &format!("the hooks say {}, the screen {}, for 5 s; the hooks decide", say(h), say(sc)));
                }
            }
            _ => a.live.disagree = None,
        }
    }

    /// Whether no turn of NAME's runs, as a round of `follow` reads it: the hooks, else the screen, else the transcript
    /// (TRANSCRIPT_ENDED: it has the last turn's end; SINCE: when the turn it shows began; CUT: it ends in an interrupt,
    /// which runs no Stop hook). The 60 s rule (`presence_agrees`) is given the transcript's own end, and the hooks are
    /// read again after it, so a Stop hook that never came ends the turn in that round. It was given "not what the hooks
    /// say", false whenever they said a turn ran, so the rule never fired: docket-core3 stayed working and was given
    /// nothing from 13:39 to 18:03 on 18 Nov, its Stop hook sent while `coord-switch` had coordd down (a2h-speed, N-560).
    pub fn turn_ended_now(&mut self, name: &str, transcript_ended: bool, since: &str, grew: std::time::Instant, cut: bool, unanswered: bool) -> bool {
        // the hooks before the screen (design-two-systems §10): an interrupt runs no Stop hook, so its entry, when the
        // transcript ends there, says the hooks' turn is over
        if cut && transcript_ended {
            self.hook_busy(name, false);
        }
        let Some(a) = self.agents.get(name) else { return transcript_ended };
        let stale = !transcript_ended && !since.is_empty() && since.get(..19) < a.live.started.get(..19);
        let (hooks, screen) = (a.live.hook_busy, a.live.screen_busy);
        self.presence_agrees(name, hooks, screen, grew, transcript_ended, unanswered);
        let a = &self.agents[name];
        a.live.hook_busy.or(a.live.screen_busy).map_or(transcript_ended || stale, |b| !b)
    }

    /// Agent NAME's terminal holder read on its screen that its session takes no channel message (WHY, Claude Code's
    /// words), or nothing of it (None: not seen, or a holder from before this word): `Live::channels_off`, as told with
    /// each `Request::Busy`, so once a minute at least.
    pub fn channels_off(&mut self, name: &str, why: Option<&str>) {
        if let Some(a) = self.agents.get_mut(name).filter(|a| a.kind == "claude" && a.running() && a.live.channels_off.as_deref() != why) {
            a.live.channels_off = why.map(String::from);
            self.changed(name);
            self.poke.notify_one();
        }
    }

    /// Agent NAME's terminal holder read Claude Code's sign-in error above its input box (WHY), or none there now:
    /// `Live::no_sign_in`. When it comes, the turn the hooks hold is over, as at a StopFailure (`hook` "failed"): its
    /// `failed` says why, its parent is told at its end, and it restarts when asked, not refused as "a turn is running"
    /// (accounts-lead N-1446 p2446, fix 2: the hooks said a turn ran, the screen none, and the hooks decided).
    pub fn no_sign_in(&mut self, name: &str, why: Option<&str>) {
        let Some(a) = self.agents.get_mut(name).filter(|a| a.kind == "claude" && a.running() && a.live.no_sign_in.as_deref() != why) else { return };
        a.live.no_sign_in = why.map(String::from);
        if let Some(w) = why {
            a.live.tools.clear();
            a.failed = Some(w.to_string());
            self.hook_busy(name, false);
            self.reconciled(name, &format!("its screen says “{w}”: its turn ended on it, and nothing is handed while it stands"));
        }
        self.changed(name);
        self.poke.notify_one();
    }

    /// Agent NAME's screen shows a turn running (BUSY) or none, as its terminal holder saw it: the state of a Claude
    /// Code agent from then on, taken by the transcript's reader (`main::follow`), woken for it.
    pub fn busy(&mut self, name: &str, busy: bool) {
        if let Some(a) = self.agents.get_mut(name).filter(|a| a.kind == "claude") {
            a.live.screen_busy = Some(busy);
            self.poke.notify_one();
            self.journal();
        }
        // the screen idle again (no turn running, held IDLE_AFTER): a compaction is over, ended or refused; while one
        // runs the footer shows "esc to interrupt", so this never comes during it
        if !busy {
            self.compaction_over(name);
        }
    }

    /// NAME's compaction, if one was marked, is over: the view's state is its turn's again (`Live::compacting`). Also
    /// when its transcript shows the compaction's boundary written since its follower began (0.2 A1: a missed hook).
    pub(crate) fn compaction_over(&mut self, name: &str) {
        if let Some(a) = self.agents.get_mut(name).filter(|a| a.live.compacting) {
            a.live.compacting = false;
            self.changed(name);
        }
    }

    /// A dialog TITLE on agent NAME's screen, standing unanswered 10 s, as its terminal holder saw it (`Request::Popup`): the owner is told the
    /// first time a title is seen (`$COORD/popups.jsonl` keeps every report), so a new popup is noticed; nothing answers
    /// it (owner, 10 Nov).
    pub fn popup(&mut self, name: &str, title: &str, screen: &str) {
        if let Some(a) = self.agents.get_mut(name) {
            let now = std::time::Instant::now();
            a.live.dialog = Some((now, title.to_string(), now));
            self.changed(name);
        }
        if self.popups.is_none() {
            self.popups = Some(read_jsonl(&self.path("popups.jsonl")).iter().map(|r| s(r, "title")).collect());
        }
        let new = self.popups.as_mut().unwrap().insert(title.to_string());
        append(&self.path("popups.jsonl"), &json!({"at": now(), "agent": name, "title": title, "new": new, "screen": screen}));
        if new {
            let body = format!("New dialog on {name}'s screen: {title}\n\nIt has stood 10 s with nothing answering it, and coord-pty has not seen this one before. The screen:\n\n~~~~\n{screen}\n~~~~");
            self.note_from_coord("owner", &body);
        }
    }

    /// A view source's line: its new data (or, with none, the data it had) and error; sent on only when it changed.
    pub fn set_view(&mut self, id: &str, data: Option<Value>, error: Option<String>) {
        let v = self.views.entry(id.into()).or_insert_with(|| ViewData { id: id.into(), data: None, error: None });
        let data = data.or_else(|| v.data.take());
        if (&v.data, &v.error) == (&data, &error) {
            return;
        }
        (v.data, v.error) = (data, error);
        let v = v.clone();
        self.emit(Body::View(v));
    }
}

/// The channel sockets in agent folder DIR to try, in order, for session SESSION: the session's own when it exists
/// (channel.py: one socket per session; 8 Nov, two sessions of one agent, the named one is right); else every other
/// session's socket, the newest first, then the shared one. After a /clear Claude Code keeps its channel server, named
/// for the session before, while the hooks name the new one (docket-live, 11 Nov: w2 got nothing after a /clear); that
/// server feeds the new session. One whose server is gone refuses the connection, and the next is tried.
pub fn channel_socks(dir: &str, session: &str) -> Vec<String> {
    let own = format!("{dir}/channel-{session}.sock");
    if !session.is_empty() && std::path::Path::new(&own).exists() {
        return vec![own];
    }
    let mut others: Vec<(std::time::SystemTime, String)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().into_string().ok()?;
            (n.starts_with("channel-") && n.ends_with(".sock")).then(|| (e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), format!("{dir}/{n}")))
        })
        .collect();
    others.sort_by(|a, b| b.0.cmp(&a.0));
    let mut out: Vec<String> = others.into_iter().map(|(_, p)| p).collect();
    out.push(format!("{dir}/channel.sock"));
    out
}

/// Why no channel socket of an agent took a hand-over, for coordd's log: each socket tried with its connect error
/// (TRIED); none there at all is channel.py, the session's coord server, not having made one, and the folder's group
/// entry is said with it, since a boxed agent writes there by its group only (N-101 p1236: an inherited group::r-x made
/// every hand-over "did not take" with nothing saying why).
pub fn channel_unreached(tried: &[(String, std::io::Error)]) -> String {
    let dir = tried.last().and_then(|(s, _)| std::path::Path::new(s).parent()).map(|p| p.display().to_string()).unwrap_or_default();
    if tried.iter().all(|(_, e)| e.kind() == std::io::ErrorKind::NotFound) {
        let group = std::process::Command::new("getfacl")
            .args(["-cp", &dir])
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).lines().find(|l| l.starts_with("group::")).map(str::to_string));
        let group = match group {
            Some(g) if g.get(7..).is_some_and(|p| !p.contains('w')) => format!("; the folder's group entry is {g}, so a boxed agent cannot make it there"),
            Some(g) => format!(" (the folder's group entry is {g})"),
            None => String::new(),
        };
        return format!("channel.py, its session's coord server, made no channel socket in {dir}{group}");
    }
    let each: Vec<String> = tried.iter().map(|(s, e)| format!("{}: {e}", s.rsplit('/').next().unwrap_or(s))).collect();
    format!("no channel socket in {dir} answers ({}): channel.py is not running", each.join("; "))
}

pub enum HookNext {
    Done(String),
    /// A Codex turn ends: coordd waits for its app-server to say so, then takes it (`Core::end_turn`).
    CodexStop,
}

impl Job {
    /// Its kind, as the minute's cost line names it (`counts`).
    pub fn kind(&self) -> &'static str {
        match self {
            Job::Waits => "job waits",
            Job::Channel { .. } => "job channel",
            Job::Codex { .. } => "job codex",
            Job::Program { .. } => "job program",
            Job::Host { .. } => "job host",
            Job::Clerk { .. } => "job clerk",
            Job::Deliver { .. } => "job deliver",
            Job::Minder { .. } => "job minder",
            Job::Coord { .. } => "job coord",
            Job::Start { .. } => "job start",
            Job::Layer { .. } => "job layer",
            Job::LayersRemove { .. } => "job layers remove",
            Job::End { .. } => "job end",
            Job::Interrupt { .. } => "job interrupt",
            Job::Command { .. } => "job command",
            Job::LimitRow { .. } => "job limit row",
            Job::ResetOffers { .. } => "job reset offers",
            Job::Restart { .. } => "job restart",
            Job::Post { .. } => "job post",
            Job::Move(_) => "job move",
            Job::AccountSaid(_) => "job account said",
            Job::AccountCheck { .. } => "job account check",
            Job::GoUsage { .. } => "job go usage",
            Job::DialogLooked { .. } => "job dialog looked",
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Job::Channel { name, .. } | Job::Codex { name, .. } | Job::Program { name, .. } | Job::Host { name, .. } | Job::Interrupt { name, .. } | Job::Command { name, .. } | Job::Restart { name, .. } | Job::ResetOffers { name, .. } => name,
            Job::Start { agent, .. } | Job::End { agent, .. } | Job::Post { agent, .. } => agent,
            Job::Move(m) => &m.agent,
            Job::Clerk { .. } => "clerk",
            Job::Deliver { queue, .. } => queue,
            Job::Layer { .. } | Job::LayersRemove { .. } => "layers",
            Job::AccountSaid(_) => "accounts",
            // Claude Code's answer is waited for: not in the line of the writes above
            Job::AccountCheck { .. } => "account check",
            // Go's page is waited for: not in the line of the writes above
            Job::GoUsage { .. } => "go usage",
            Job::Minder { .. } => "minder",
            Job::Coord { .. } => "coord",
            // a queue of its own: a file read that must not stand behind what is being handed to the agent
            Job::LimitRow { .. } => "limits",
            Job::Waits => "waits",
            // a queue of its own: the screen is read while a dialog may hold what is being handed to the agent
            Job::DialogLooked { .. } => "screens",
        }
    }
}

/// The `FROM TO` pairs of FILE (`$COORD/sessions-moved`), one per line, a blank or `#` line skipped: folders whose
/// transcripts now lie elsewhere. Tovrin's sessions ran in a container whose home was /home/agent; their transcripts
/// are kept under the coord, and the registry still names the container's paths (paper-research N-101 p1045, p1060).
/// No pairs when there is no file.
fn read_sessions_moved(file: &str) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(file) else { return Vec::new() };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once(char::is_whitespace).map(|(a, b)| (a.trim_end_matches('/').to_string(), b.trim().trim_end_matches('/').to_string())))
        .collect()
}

/// Whether SESSION is one of the transcripts of the agent whose current one is CURRENT: that one, or a `.jsonl`
/// beside it (in the same folder), as an earlier one, resumed or compacted from, or the one a cut copy was cut from
/// is. A request cannot name any file (`Core::transcript_image`, `Core::transcript_of`).
fn one_of_its_sessions(current: &str, session: &str) -> bool {
    session == current || (std::path::Path::new(current).parent() == std::path::Path::new(session).parent() && session.ends_with(".jsonl"))
}

/// Where TRANSCRIPT is now, when it does not exist and lies under a folder MOVED names and the same path under the
/// folder it went to exists. The registry is not rewritten: the path it names is read there.
fn moved_transcript(transcript: &str, moved: &[(String, String)]) -> Option<String> {
    if moved.is_empty() || std::path::Path::new(transcript).exists() {
        return None;
    }
    moved.iter().find_map(|(from, to)| {
        let rest = transcript.strip_prefix(from.as_str()).filter(|r| r.starts_with('/'))?;
        let there = format!("{to}{rest}");
        std::path::Path::new(&there).exists().then_some(there)
    })
}

/// The file a Claude Code session really writes, when the TRANSCRIPT path its hook gave does not exist: the same file
/// name in another project folder. Claude Code names the path after the session's current directory, but a session
/// resumed from another directory goes on writing the file in the folder where it began (CONE and MEANING, begun in
/// .wt/cone and .wt/meaning and resumed from their agda/ subfolders, showed empty chats; 9 Nov, m151). None when the
/// path exists or no other folder has the file.
fn written_transcript(transcript: &str) -> Option<String> {
    let path = std::path::Path::new(transcript);
    if path.exists() {
        return None;
    }
    let file = path.file_name()?;
    let projects = path.parent()?.parent()?;
    std::fs::read_dir(projects).ok()?.flatten().map(|d| d.path().join(file)).find(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned())
}

/// What a whole-state event is of (the agent, the project view, the mail, the refusals, the owner model): a newer
/// event of the same carries all it did, so the ring keeps only the newer.
/// How many bytes VALUE's JSON takes, as `serde_json` writes it, counted as it is written: nothing is kept.
fn json_len<T: serde::Serialize>(value: &T) -> u64 {
    struct Count(u64);
    impl std::io::Write for Count {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0 += b.len() as u64;
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut n = Count(0);
    let _ = serde_json::to_writer(&mut n, value);
    n.0
}

/// What the ring keeps of NEW, a ticket's change, in place of OLD, that ticket's change before it: NEW with the posts
/// OLD carried that it does not, merged by number as the window merges a change into what it holds (live.rs), so a
/// watcher that resumes from before OLD ends where OLD and NEW would have left it. A change leaves the ticket's whole
/// event where it is, so a watcher resuming after that one is sent what moved, not the ticket whole; the ring holds at
/// most the two a ticket (paper-research N-1446 p2604: a change carries the view whole but its posts, research's N-1446
/// 1.2 MB of it, and changes had no key, so 1,564 of them held 1 GB as JSON, 1.8 GB of heap, until a snapshot cleared
/// the ring). None: NEW is kept as it is.
fn kept_change(old: Body, new: &Event) -> Option<Event> {
    let (Body::TicketChange { view: was, .. }, Body::TicketChange { view, posts }) = (old, &new.body) else { return None };
    let mut merged = was.posts;
    merged.retain(|p| !view.posts.iter().any(|q| q.n == p.n));
    merged.extend(view.posts.iter().cloned());
    merged.sort_by_key(|p| p.n);
    let view = docket_machine::TicketView { posts: merged, ..view.clone() };
    Some(Event { seq: new.seq, body: Body::TicketChange { view, posts: *posts } })
}

fn event_key(b: &Body) -> Option<String> {
    match b {
        Body::Agent(a) => Some(format!("a {}", a.name)),
        Body::View(v) => Some(format!("v {}", v.id)),
        Body::Mail(m) => Some(format!("m {}", m.id)),
        Body::Refusals(_) => Some("r".into()),
        Body::Notices(_) => Some("n".into()),
        Body::OwnerModel(_) => Some("o".into()),
        Body::Ticket(t) => Some(format!("t {}", t.id)),
        Body::TicketChange { view, .. } => Some(format!("c {}", view.id)),
        _ => None,
    }
}

/// The note of a new item: DIR/ID-SLUG.md, owned like DIR (coordd runs as root; the agents edit notes).
/// Item ID's new note: its file name (`ID-SLUG.md`, SLUG the title's first six words) and its text.
pub(crate) fn note_text(id: &str, title: &str, by: &str) -> (String, String) {
    let slug: String = title
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    let day = &now()[..10];
    (format!("{id}-{slug}.md"), format!("---\ntitle: {title}\nopened: {day}\nby: {by}\n---\n\n# {id} {title}\n\n## Log\n- {day} opened by {by}.\n"))
}

pub(crate) fn write_note(dir: &str, id: &str, title: &str, by: &str) -> Result<String, String> {
    use std::os::unix::fs::MetadataExt;
    let (name, text) = note_text(id, title, by);
    let path = format!("{dir}/{name}");
    let meta = std::fs::metadata(dir).map_err(|e| format!("{dir}: {e}"))?;
    std::fs::write(&path, text).map_err(|e| format!("{path}: {e}"))?;
    let _ = std::os::unix::fs::chown(&path, Some(meta.uid()), Some(meta.gid()));
    Ok(path)
}

pub fn ok(out: String) -> Reply {
    Reply { ok: true, out, post: None }
}

pub fn err(out: String) -> Reply {
    Reply { ok: false, out, post: None }
}

/// The harness a roster agent on MODEL runs in: coord-types' one rule, which the window's New agent reads too.
pub use coord_types::harness::kind_of_model;

/// Whether an agent of KIND is a harness's session: Claude Code, Codex, opencode or agy (`served`); any other kind is
/// a program with no session.
pub fn harness(kind: &str) -> bool {
    matches!(kind, "claude" | "codex" | "opencode" | "agy")
}

/// Whether an agent of KIND is a session coordd reaches through an rpc program (`rpc_bin`) rather than a terminal and a
/// channel: Codex (its app-server), opencode (its `opencode serve`) and agy (agy-rpc's session, through t3code or its
/// own ACP client). Their turns begin and end by their hooks and the program's `wait-idle`, their mail goes in by the
/// program's `send`, an esc is its `interrupt` (tool-scan, N-591 p109).
pub fn served(kind: &str) -> bool {
    matches!(kind, "codex" | "opencode" | "agy")
}

/// The rpc program of a served agent of KIND (`served`), under agent-base BASE: `codex-rpc`, or `opencode-rpc` or
/// `agy-rpc`, which take the same commands and arguments.
pub fn rpc_bin(base: &str, kind: &str) -> String {
    format!(
        "{base}/bin/{}",
        match kind {
            "opencode" => "opencode-rpc",
            "agy" => "agy-rpc",
            _ => "codex-rpc",
        }
    )
}

/// KIND with its article, as the refusals name it: "a codex", "an opencode" (consult-lead, N-412 post 1750).
pub fn kind_article(kind: &str) -> String {
    let an = kind.chars().next().is_some_and(|c| "aeiouAEIOU".contains(c));
    format!("{} {kind}", if an { "an" } else { "a" })
}

/// Run the rpc program RPC (`rpc_bin`) with ARGS against the session SOCKET names, for thread SESSION; whether it
/// succeeded.
pub async fn codex_rpc(rpc: &str, session: &str, socket: &str, args: &[&str]) -> bool {
    codex_rpc_at(rpc, session, socket, args, None).await
}

/// `codex_rpc`, with EFFORT the reasoning effort a turn it starts runs at (CODEX_EFFORT; `coord effort`).
pub async fn codex_rpc_at(rpc: &str, session: &str, socket: &str, args: &[&str], effort: Option<&str>) -> bool {
    let mut cmd = tokio::process::Command::new(rpc);
    cmd.arg(args[0]).arg(session).args(&args[1..]).env("CODEX_SOCK", socket);
    if let Some(e) = effort {
        cmd.env("CODEX_EFFORT", e);
    }
    cmd.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    cmd.status().await.map(|s| s.success()).unwrap_or(false)
}

/// Whether NAME is a fresh step's scoped agent, fT-N (ticket/mod.rs `Effect::End`).
pub fn is_fresh(name: &str) -> bool {
    name.strip_prefix('f').and_then(|r| r.split_once('-')).is_some_and(|(t, n)| !t.is_empty() && !n.is_empty() && t.bytes().all(|b| b.is_ascii_digit()) && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Whether process PID is still running.
/// How long a new process may go without its session's start hook before it is handed to all the same
/// (`Core::to_hand_over`).
const START_WAIT: std::time::Duration = std::time::Duration::from_secs(120);
/// How long a turn's end the transcript has not shown waits before the agent is handed to all the same
/// (`Core::to_hand_over`).
const OUTPUT_WAIT: std::time::Duration = std::time::Duration::from_secs(120);

fn alive(pid: i32) -> bool {
    pid > 0 && std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// The net under a lost hand-over (paper-research m1005, 3; m1013), the real way: handed, taken, no turn, a minute idle,
/// the reconcile loop's `not_read`, the second hand-over's text; a second miss leaves its post and no third one; the log
/// replays to the same.
#[cfg(test)]
mod not_read_tests {
    use super::*;
    use coord_types::{DEAF_ACCOUNT, DEAF_RESTART};
    use docket_machine::{Input, Text};

    /// The jobs hand_over sent: the ids each Channel job carried and those it hands again.
    fn channel_jobs(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Job>) -> Vec<(Vec<String>, Vec<String>, Vec<(docket_machine::Delivery, docket_machine::TicketView)>)> {
        let mut out = Vec::new();
        while let Ok(j) = rx.try_recv() {
            if let Job::Channel { ids, again, tickets, .. } = j {
                out.push((ids, again, tickets));
            }
        }
        out
    }

    /// A minute idle since the handing, as the reconcile loop would find it after one.
    fn a_minute_later(core: &mut Core) {
        core.not_read(); // the first look starts the minute
        let past = std::time::Instant::now() - Duration::from_secs(61);
        core.idle_given.insert("w2".into(), past);
        core.not_read();
    }

    /// A return to an earlier release (paper-research N-101.p1309): the owner's hold hands nothing, so a stopped turn
    /// stays stopped; off, the same is handed. coord's notice to an agent is coord's thread, not his.
    #[test]
    fn a_hold_hands_nothing_and_a_notice_is_coords() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-holding-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        let r = core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("what is 6 times 7?".into())), names: vec!["w2".into()], told: vec![] });
        assert!(r.is_ok(), "{r:?}");
        assert_eq!(core.to_hand_over(), vec!["w2".to_string()]);
        assert!(!core.hold_deliveries(&Who::Agent("w2".into()), true).ok, "the owner's only");
        assert!(core.hold_deliveries(&Who::Owner, true).ok);
        assert!(core.to_hand_over().is_empty(), "held: nothing is handed");
        assert!(core.hold_deliveries(&Who::Owner, false).ok);
        assert_eq!(core.to_hand_over(), vec!["w2".to_string()]);
        // the notice: a thread opened by coord, naming w2
        assert!(!core.coord_notice(&Who::Owner, "nobody", "go on").ok);
        assert!(core.coord_notice(&Who::Owner, "w2", "go on").ok);
        let x = core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().1;
        assert_eq!((x.opener.as_deref(), x.posts[0].by.as_str(), x.posts[0].names.clone()), (Some("coord"), "coord", vec!["w2".to_string()]));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// The first round after the install that shows coordd's notices to nobody (consult-3 N-412.p1982, 2; research's
    /// N-412.p1835, given to consult-3 and not read): a record from before holds a notice of coordd's handed to w2 and to
    /// w3 at their places, not read. The new coordd loads it: the notice is due to nobody; seen again at its place it is
    /// not given again; w3's turn ending on it posts nothing on that thread, though its text names w1; a minute idle
    /// makes w2's due again once, hands nothing and writes no "twice" fact.
    #[test]
    fn a_notice_of_coordds_handed_before_the_install_and_not_read_is_handed_nothing_more_and_replied_on_nowhere() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-notice-unread-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let registry: String = ["w1", "w2", "w3"].iter().enumerate().map(|(i, n)| format!("{}\n", json!({"ev": "add", "id": format!("AGT-00{}", i + 1), "name": n, "parent": "owner", "kind": "claude", "model": "m", "at": "x"}))).collect();
        std::fs::write(coord.join("registry.jsonl"), registry).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let r = core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("what is 6 times 7?".into())), names: vec!["w1".into(), "w2".into(), "w3".into()], told: vec![] });
        assert!(r.is_ok(), "{r:?}");
        let d1 = core.ticket_due("w1").into_iter().next().expect("the note is due to w1");
        let t = d1.t;
        // w2 and w3 read the note: a turn on it that ends with Nothing
        for a in ["w2", "w3"] {
            let d = core.ticket_due(a).into_iter().find(|d| d.t == t).expect("the note is due");
            let r = core.ticket_input(Input::Given { t, to: a.into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None });
            assert!(r.is_ok(), "{r:?}");
            let f = coord.join(format!("{a}-read.jsonl"));
            let entry = format!("{}\n", json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": "Nothing"}]}}));
            std::fs::write(&f, &entry).unwrap();
            core.ticket_turn_ended(a, docket_machine::Loc { file: f.display().to_string(), offset: 0, len: entry.len() as u64 });
            assert!(core.ticket_due(a).is_empty() && core.ticket_given_since(a).is_empty(), "{a} read the note");
        }
        // coordd's notice, as the machine makes it: w1 given the note twice and reading it neither time
        for _ in 0..2 {
            let r = core.ticket_input(Input::Given { t, to: "w1".into(), delivery: d1.id.clone(), upto: d1.upto, skipped: Default::default(), place: None });
            assert!(r.is_ok(), "{r:?}");
            core.ticket_unread("w1");
        }
        let notice = core.tickets.m.tickets[&t].posts.last().map(|p| (p.n, p.by.clone())).unwrap();
        assert_eq!(notice, (2, "coordd".to_string()));
        // the coordd before the install handed it on to w2 and w3 (it was shown then): each took it at a place and
        // began no turn on it
        let id = format!("{}.p2", core.tickets.m.id(t));
        let place = |a: &str| docket_machine::Loc { file: format!("/nonexistent/{a}.jsonl"), offset: 40, len: 5 };
        for a in ["w2", "w3"] {
            let r = core.ticket_input(Input::Given { t, to: a.into(), delivery: id.clone(), upto: 2, skipped: Default::default(), place: Some(place(a)) });
            assert!(r.is_ok(), "{r:?}");
        }
        drop(core);

        // the install: the new coordd reads that record, its agents running on (found by `recover` before the load: an
        // agent found stopped has its turn cut at the load)
        let mut core = Core::load(&coord.display().to_string(), &base);
        for a in ["w1", "w2", "w3"] {
            core.agents.get_mut(a).unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        }
        core.load_tickets();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        assert_eq!((core.ticket_given_since("w2"), core.ticket_given_since("w3")), (vec![id.clone()], vec![id.clone()]), "given and not read, as the record has it");
        for a in ["w1", "w2", "w3"] {
            assert!(core.ticket_due(a).is_empty(), "{a}: nothing due");
        }
        assert!(core.to_hand_over().is_empty());
        let path = coord.join("tickets/log.jsonl");
        let log = || std::fs::read_to_string(&path).unwrap_or_default();
        let lines = log().lines().count();
        // the follower's first read sees each entry at its place: that giving, nothing written
        for a in ["w2", "w3"] {
            core.ticket_seen(a, &id, place(a));
        }
        assert_eq!(log().lines().count(), lines, "seen at its place: nothing written");
        let posts = core.tickets.m.tickets[&t].posts.len();

        // w3's turn ends holding it (begun on what it took before the start): its last message is posted nowhere
        let transcript = coord.join("w3.jsonl");
        let entry = format!("{}\n", json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": "Noted, @w1, your note is read."}]}}));
        std::fs::write(&transcript, &entry).unwrap();
        core.ticket_turn_ended("w3", docket_machine::Loc { file: transcript.display().to_string(), offset: 0, len: entry.len() as u64 });
        let x = &core.tickets.m.tickets[&t];
        assert_eq!(x.posts.len(), posts, "no reply on the notice's thread: {:?}", x.posts.last().map(|p| (p.n, p.by.clone())));
        assert!(core.ticket_given_since("w3").is_empty(), "its turn's end took it");
        assert!(core.ticket_due("w1").is_empty() && core.to_hand_over().is_empty(), "nobody woken");
        let reconcile = core.path("reconcile.jsonl");
        let journal = || std::fs::read_to_string(&reconcile).unwrap_or_default();
        assert!(journal().lines().any(|l| l.contains("\"agent\":\"w3\"") && l.contains("posted nowhere")), "{}", journal());

        // w2, idle a minute: made due again once, which hands nothing, and no "twice" fact; the next minute, nothing
        a_minute_later(&mut core);
        assert!(core.ticket_given_since("w2").is_empty());
        assert!(core.ticket_due("w2").is_empty() && core.to_hand_over().is_empty(), "due again, but it shows nothing");
        a_minute_later(&mut core);
        assert!(channel_jobs(&mut rx).is_empty(), "nothing handed");
        assert!(!journal().lines().any(|l| l.contains("\"agent\":\"w2\"") && l.contains("given twice")), "{}", journal());
        assert_eq!(log().matches("\"ev\":\"unread\"").count(), 3, "w1's two and w2's one: {}", log());
        assert_eq!(core.tickets.m.tickets[&t].posts.len(), posts, "nothing posted");
        // and the log replays to the same
        let mut again = Core::load(&coord.display().to_string(), &base);
        again.load_tickets();
        let (a, b) = (&again.tickets.m.tickets[&t], &core.tickets.m.tickets[&t]);
        let rows = |x: &docket_machine::Ticket| x.given.iter().map(|g| (g.to.clone(), g.delivery.clone(), g.unread)).collect::<Vec<_>>();
        assert_eq!((a.posts.len(), rows(a), &a.unread_twice), (posts, rows(b), &b.unread_twice));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// What he can do about a session that takes no messages comes as acts beside the words (`AgentView::deaf_do`,
    /// proto-window N-1446 p2538), each act's words where the words end, in their order, joined by ", or ": an account
    /// for a sign-in error or channels off (another one under an organization's policy), a restart for no channel
    /// socket, both for deliveries never seen there, and none while an account move is due.
    #[test]
    fn what_he_can_do_about_a_session_that_takes_no_messages_comes_as_acts_ending_its_words() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-deafdo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        let running = || Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        let cases: Vec<(&str, Live, Vec<(&str, &str)>)> = vec![
            ("a sign-in error", Live { no_sign_in: Some("Not logged in · Please run /login".into()), ..running() }, vec![(DEAF_ACCOUNT, "pick an account for this agent")]),
            ("channels off", Live { channels_off: Some("Channels are not currently available".into()), ..running() }, vec![(DEAF_ACCOUNT, "pick an account for this agent")]),
            ("an organization's policy", Live { channels_off: Some("Inbound messages will be silently dropped".into()), ..running() }, vec![(DEAF_ACCOUNT, "pick another account for this agent")]),
            ("no channel socket", Live { unreached: Some(("no channel socket".into(), std::time::SystemTime::now())), ..running() }, vec![(DEAF_RESTART, "restart it")]),
            ("never seen there", Live { handed_unseen: Some("m5".into()), ..running() }, vec![(DEAF_RESTART, "restart it"), (DEAF_ACCOUNT, "pick an account for this agent")]),
            ("never seen there, an account move due", Live { handed_unseen: Some("m5".into()), account_due: Some("work".into()), ..running() }, vec![]),
        ];
        for (what, live, acts) in cases {
            core.agents.get_mut("w2").unwrap().live = live;
            let v = core.view("w2");
            let deaf = v.deaf.clone().unwrap_or_else(|| panic!("{what}: no words"));
            assert_eq!(v.deaf_do.iter().map(|d| (d.act.as_str(), d.words.as_str())).collect::<Vec<_>>(), acts, "{what}");
            let todo: Vec<&str> = acts.iter().map(|(_, w)| *w).collect();
            let end = if todo.is_empty() { "; it restarts to take them again".to_string() } else { format!("; {}", todo.join(", or ")) };
            assert!(deaf.ends_with(&end), "{what}: {deaf:?} ends with {end:?}");
        }
        // it takes them: neither
        core.agents.get_mut("w2").unwrap().live = running();
        let v = core.view("w2");
        assert_eq!((v.deaf, v.deaf_do), (None, vec![]));
        // the wire: none is left out, and a window from before reads the words alone
        core.agents.get_mut("w2").unwrap().live = Live { handed_unseen: Some("m5".into()), ..running() };
        let wire = serde_json::to_value(core.view("w2")).unwrap();
        assert_eq!(wire["deaf_do"], json!([{"act": "restart", "words": "restart it"}, {"act": "account", "words": "pick an account for this agent"}]));
        core.agents.get_mut("w2").unwrap().live = running();
        assert!(serde_json::to_value(core.view("w2")).unwrap().get("deaf_do").is_none(), "none: not on the wire");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A delivery handed and never seen in its session, a minute idle, marks the session (`Live::handed_unseen`;
    /// paper-research N-1446 p2424, for a holder that does not read Claude Code's word): its view says so, with what he
    /// can do, and what is due is handed again as before, no stop; a delivery seen there ends the mark.
    #[test]
    fn a_delivery_never_seen_marks_its_session_until_one_is_seen_there() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-unseen-mark-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        let r = core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("what is 6 times 7?".into())), names: vec!["w2".into()], told: vec![] });
        assert!(r.is_ok(), "{r:?}");
        let hand = |core: &mut Core, rx: &mut tokio::sync::mpsc::UnboundedReceiver<Job>, what: &str| {
            assert_eq!(core.to_hand_over(), vec!["w2".to_string()], "{what}: due {:?}, given since {:?}", core.ticket_due("w2"), core.ticket_given_since("w2"));
            core.hand_over("w2");
            let ids = channel_jobs(rx).into_iter().next().unwrap().0;
            core.handed("w2", &ids, true, "channel");
            ids
        };
        let ids = hand(&mut core, &mut rx, "the first");
        assert!(core.to_hand_over().is_empty(), "given: nothing more is handed");
        core.not_read();
        assert_eq!(core.view("w2").deaf, None, "not within the minute");
        // as in the leg below: a turn seen running in between starts the minute again
        core.agents.get_mut("w2").unwrap().live.state = "working".into();
        core.not_read();
        core.agents.get_mut("w2").unwrap().live.state = "idle".into();
        a_minute_later(&mut core);
        assert_eq!(core.view("w2").deaf, Some(format!("what was handed to its session never reached it ({}); restart it, or pick an account for this agent", ids[0])));
        // no stop: handed again, and seen in its session this time
        let ids2 = hand(&mut core, &mut rx, "again");
        core.ticket_seen("w2", &ids2[0], docket_machine::Loc { file: coord.join("w2.jsonl").display().to_string(), offset: 7, len: 1 });
        assert_eq!(core.view("w2").deaf, None, "seen there: the mark is over");
        let _ = std::fs::remove_dir_all(&coord);
    }

    #[test]
    fn a_note_never_read_is_handed_again_once_then_said_on_its_ticket() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-unread-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        let r = core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("what is 6 times 7?".into())), names: vec!["w2".into()], told: vec![] });
        assert!(r.is_ok(), "{r:?}");

        // handed and taken; the session drops it and no turn begins
        let hand = |core: &mut Core, rx: &mut tokio::sync::mpsc::UnboundedReceiver<Job>| {
            assert_eq!(core.to_hand_over(), vec!["w2".to_string()]);
            core.hand_over("w2");
            let jobs = channel_jobs(rx);
            assert_eq!(jobs.len(), 1);
            let (ids, _, _) = &jobs[0];
            core.handed("w2", ids, true, "channel");
            jobs.into_iter().next().unwrap()
        };
        let (ids, again, _) = hand(&mut core, &mut rx);
        let id = ids[0].clone();
        assert!(again.is_empty(), "the first hand-over is no second one");
        assert_eq!(core.ticket_given_since("w2"), vec![id.clone()]);
        assert!(core.to_hand_over().is_empty(), "given: nothing more is handed");

        // not within the minute, and not while a turn runs
        core.not_read();
        assert_eq!(core.ticket_given_since("w2"), vec![id.clone()]);
        core.agents.get_mut("w2").unwrap().live.state = "working".into();
        core.not_read();
        assert!(!core.idle_given.contains_key("w2"), "a turn running starts the minute again");
        core.agents.get_mut("w2").unwrap().live.state = "idle".into();

        // a minute idle: due again, handed again saying so
        a_minute_later(&mut core);
        assert!(core.ticket_given_since("w2").is_empty());
        let (ids2, again, tickets) = hand(&mut core, &mut rx);
        assert_eq!((ids2.clone(), again.clone()), (vec![id.clone()], vec![id.clone()]));
        let (d, view) = &tickets[0];
        let (text, meta) = crate::render::delivery(&core.places(), d, view, true, false, again.contains(&d.id), false, None).unwrap();
        assert!(text.starts_with("This was handed to you before") && text.contains("what is 6 times 7?"), "{text}");
        assert_eq!(meta["again"], json!("second hand-over"));

        // missed again: said on its ticket, not handed a third time
        a_minute_later(&mut core);
        let t = d.t;
        let x = &core.tickets.m.tickets[&t];
        let last = x.posts.last().unwrap();
        assert_eq!((last.by.as_str(), last.text.clone()), ("coordd", Text::New(format!("w2 was given {id} twice and did not read it."))));
        let posts = x.posts.len();
        assert!(core.to_hand_over().is_empty() && core.ticket_due("w2").is_empty(), "no third hand-over");
        // shown to nobody: the journal says it, the view leaves the post out and its givings say not read
        // (paper-research N-412.p1844, 2; consult-lead p1900, 2)
        let journal = std::fs::read_to_string(core.path("reconcile.jsonl")).unwrap_or_default();
        assert!(journal.lines().any(|l| l.contains(&format!("given twice, not read: {id}"))), "{journal}");
        let v = core.tickets.m.view(t).unwrap();
        assert!(v.posts.iter().all(|p| p.by != "coordd"), "{:?}", v.posts.iter().map(|p| (p.n, p.by.clone())).collect::<Vec<_>>());
        let rows: Vec<bool> = v.given.iter().filter(|g| g.to == "w2" && g.delivery == id).map(|g| g.unread).collect();
        assert_eq!(rows, vec![true, true], "both givings of the delivery say not read");

        // the log replays to the same
        let mut again_core = Core::load(&coord.display().to_string(), &base);
        again_core.agents.get_mut("w2").unwrap().live.pid = Some(std::process::id() as i32);
        again_core.load_tickets();
        let log = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap_or_default();
        let replayed = again_core.tickets.m.tickets[&t].posts.len();
        let due = again_core.ticket_due("w2");
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!(log.matches("\"ev\":\"unread\"").count(), 2, "{log}");
        assert_eq!(replayed, posts);
        assert!(due.is_empty());
    }

    /// A post of the owner's held under a question to him when an engine that hands them first starts (paper-research
    /// N-807.p78, 5; consult-3 p80, 3): handed to every agent on its thread by one record line; a restart after it, and
    /// the replay it makes, hand it no second time.
    #[test]
    fn a_post_held_at_the_install_is_handed_once_however_often_coordd_starts() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-held-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let reg = |id: &str, name: &str| format!("{{\"ev\":\"add\",\"id\":\"{id}\",\"name\":\"{name}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n");
        std::fs::write(coord.join("registry.jsonl"), reg("AGT-001", "lead") + &reg("AGT-002", "fixer")).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        assert!(!core.tickets.m.held_handed, "a record with nothing in it at the start: no line");
        // as ticket/mod.rs's route test: his thread to lead, lead and fixer on it, then his words placed there by intake
        let v = json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": [], "text": "the release", "addressed": {"head": "Unnamed"}, "esc": []});
        core.ticket_input(docket_machine::Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        core.ticket_answer(docket_machine::WATCHER, t, json!({"head": "Thread"}), docket_machine::Text::New(String::new())).unwrap();
        for (by, text, names) in [("lead", "@fixer take the pane", "fixer"), ("fixer", "Taken.", "lead")] {
            core.ticket_input(docket_machine::Input::Post { t, by: by.into(), text: docket_machine::Text::New(text.into()), names: vec![names.into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        }
        let v = json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": [], "text": "when does the runner come?", "addressed": {"head": "Unnamed"}, "esc": []});
        core.ticket_input(docket_machine::Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let u = *core.tickets.m.tickets.keys().filter(|k| **k < docket_machine::HIDDEN).next_back().unwrap();
        let tid = core.tickets.m.id(t);
        core.ticket_answer(docket_machine::WATCHER, u, json!({"head": "Place", "ticket": tid}), docket_machine::Text::New(String::new())).unwrap();
        let n = core.tickets.m.tickets[&t].last();
        let h = core.tickets.m.tickets.values().find(|x| x.is_open() && x.post_of.as_ref().is_some_and(|p| p.t == t && p.n == n)).map(|x| x.t).unwrap_or_else(|| panic!("no handler holds post {n}: {:?}", core.tickets.m.tickets[&t].posts.iter().map(|p| (p.n, p.by.clone(), p.names.clone(), p.told.clone(), p.held.clone())).collect::<Vec<_>>()));
        core.ticket_answer(docket_machine::WATCHER, h, json!({"head": "Unclear", "why": "the last post is by fixer"}), docket_machine::Text::New(String::new())).unwrap();
        let held = core.tickets.m.unanswered_holds().len();
        drop(core);
        // the install: the new engine's first start
        let mut first = Core::load(&coord.display().to_string(), &base);
        first.load_tickets();
        let told = first.tickets.m.tickets[&t].posts.iter().find(|p| p.n == n).map(|p| p.told.clone()).unwrap_or_default();
        let left = first.tickets.m.unanswered_holds().len();
        drop(first);
        // a restart after it
        let mut again = Core::load(&coord.display().to_string(), &base);
        again.load_tickets();
        let told_again = again.tickets.m.tickets[&t].posts.iter().find(|p| p.n == n).map(|p| p.told.clone()).unwrap_or_default();
        let log = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!(held, 1);
        assert_eq!(left, 0, "handed at the first start");
        let mut want = vec!["lead".to_string(), "fixer".to_string()];
        let mut got = told.clone();
        want.sort();
        got.sort();
        assert_eq!(got, want, "to every agent on the thread");
        assert_eq!(told_again, told);
        assert_eq!(log.matches("held_handed").count(), 1, "one line, never again: {log}");
        assert_eq!(log.lines().filter(|l| l.contains("\"ev\":\"told\"") && l.contains(&format!("\"n\":{n}"))).count(), 1, "told once: {log}");
    }
}

#[cfg(test)]
mod channel_socks_tests {
    use super::channel_socks;

    /// After a /clear: the hooks name the new session, the channel server still listens under the old one's name.
    #[test]
    fn after_a_clear_the_old_sessions_server_is_tried() {
        let dir = std::env::temp_dir().join(format!("docket-socks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.display().to_string();
        // a server gone (its file left) and the live one, newer
        let gone = std::os::unix::net::UnixListener::bind(dir.join("channel-aaa.sock")).unwrap();
        drop(gone);
        std::thread::sleep(std::time::Duration::from_millis(20));
        let live = std::os::unix::net::UnixListener::bind(dir.join("channel-old.sock")).unwrap();
        let socks = channel_socks(&d, "new");
        assert_eq!(socks, vec![format!("{d}/channel-old.sock"), format!("{d}/channel-aaa.sock"), format!("{d}/channel.sock")]);
        // the first that answers is the live one
        let answered = socks.iter().find(|p| std::os::unix::net::UnixStream::connect(p).is_ok()).cloned();
        assert_eq!(answered, Some(format!("{d}/channel-old.sock")));
        // the session's own socket wins when it exists (8 Nov: two sessions of one agent)
        let own = std::os::unix::net::UnixListener::bind(dir.join("channel-new.sock")).unwrap();
        assert_eq!(channel_socks(&d, "new"), vec![format!("{d}/channel-new.sock")]);
        drop((live, own));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A hand-over no socket took says why in coordd's log: no socket made, with the folder's group entry (N-101 p1236),
    /// or sockets there whose server is gone.
    #[test]
    fn a_hand_over_no_socket_took_says_why() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("docket-unreached-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o2750)).unwrap();
        let d = dir.display().to_string();
        let tried = |socks: Vec<String>| -> Vec<(String, std::io::Error)> { socks.into_iter().map(|s| { let e = std::os::unix::net::UnixStream::connect(&s).unwrap_err(); (s, e) }).collect() };
        let none = super::channel_unreached(&tried(channel_socks(&d, "sid")));
        assert_eq!(none, format!("channel.py, its session's coord server, made no channel socket in {d}; the folder's group entry is group::r-x, so a boxed agent cannot make it there"));
        // a socket whose server is gone: a socket file no listener ever had. A listener bound and dropped here could
        // answer still, held by a child another test forked before its exec (the suite runs them together)
        let old = std::ffi::CString::new(dir.join("channel-old.sock").display().to_string()).unwrap();
        assert_eq!(unsafe { libc::mknod(old.as_ptr(), libc::S_IFSOCK | 0o600, 0) }, 0, "{}", std::io::Error::last_os_error());
        let gone = super::channel_unreached(&tried(channel_socks(&d, "sid")));
        assert!(gone.starts_with(&format!("no channel socket in {d} answers (channel-old.sock: Connection refused")), "{gone}");
        assert!(gone.ends_with("channel.py is not running"), "{gone}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod session_cwd_tests {
    use super::*;

    /// N-64: a compaction or a /clear in another folder (the agent had gone into its memory folder) keeps the folder
    /// the session started in, which `roster restart` resumes in; a startup or a resume sets it.
    #[test]
    fn a_compaction_elsewhere_keeps_the_sessions_folder() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-session-cwd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"orch\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        let file = |core: &Core| -> Value { serde_json::from_slice(&std::fs::read(core.path("agents/orch/session.json")).unwrap()).unwrap() };
        core.hook("orch", "start", &json!({"source": "startup", "session_id": "s1", "cwd": "/d/orch", "transcript_path": "/t/s1.jsonl"}));
        assert_eq!(file(&core)["cwd"], json!("/d/orch"));
        for source in ["compact", "clear"] {
            core.hook("orch", "start", &json!({"source": source, "session_id": "s1", "cwd": "/d/orch/memory", "transcript_path": "/t/s1.jsonl"}));
            let f = file(&core);
            assert_eq!((f["cwd"].clone(), f["source"].clone()), (json!("/d/orch"), json!(source)), "{source}");
        }
        // a new process: where it started counts
        core.hook("orch", "start", &json!({"source": "resume", "session_id": "s1", "cwd": "/d/elsewhere", "transcript_path": "/t/s1.jsonl"}));
        assert_eq!(file(&core)["cwd"], json!("/d/elsewhere"));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// How full an agent's context is (paper-research N-101 p1259 point 5): the window from the coord's Claude settings,
    /// the used count set by the usage reader, in the agent's view.
    #[test]
    fn the_view_says_how_full_the_context_is() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-context-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"orch\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let c = coord.display().to_string();
        let from_base = Core::context_window(&c, &base);
        // the project's own point (its claude-settings.json beside its project file) stands over agent-base's; the
        // file roster starts sessions with ($COORD/claude-settings.json) says none
        std::fs::create_dir_all(coord.join("proj")).unwrap();
        std::fs::write(coord.join("project"), format!("{}\n", coord.join("proj/team.docket").display())).unwrap();
        std::fs::write(coord.join("claude-settings.json"), r#"{"autoCompactWindow": 123000}"#).unwrap();
        assert_eq!(Core::context_window(&c, &base), from_base, "the session's settings file is not where the point is kept");
        std::fs::write(coord.join("proj/claude-settings.json"), r#"{"autoCompactWindow": 200000}"#).unwrap();
        let from_coord = Core::context_window(&c, &base);
        std::fs::remove_file(coord.join("project")).unwrap();
        let mut core = Core::load(&c, &base);
        assert_eq!(core.view("orch").context, None);
        core.set_context("orch", Some(coord_types::AgentContext { used: 123456, window: from_coord, ..Default::default() }));
        let v = serde_json::to_value(core.view("orch")).unwrap();
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!(from_base, Some(350000), "agent-base's own");
        assert_eq!(from_coord, Some(200000));
        assert_eq!(v["context"], json!({"used": 123456, "window": 200000}));
    }

    /// A fresh agent whose session has not said its folder yet shows the folder its process runs in; its session's own,
    /// once said, wins (N-1888).
    #[test]
    fn a_fresh_agents_folder_is_its_process_folder_until_its_session_says() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-fresh-cwd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"fresh\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        assert_eq!(core.view("fresh").cwd, None, "not running: nothing known");
        core.agents.get_mut("fresh").unwrap().live.pid = Some(std::process::id() as i32);
        let here = std::env::current_dir().unwrap().display().to_string();
        assert_eq!(core.view("fresh").cwd.as_deref(), Some(here.as_str()));
        core.hook("fresh", "start", &json!({"source": "startup", "session_id": "s1", "cwd": "/d/fresh", "transcript_path": "/t/s1.jsonl"}));
        assert_eq!(core.view("fresh").cwd.as_deref(), Some("/d/fresh"));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A compaction shows in the view's `compacting` (paper-research N-101 p1259 point 4): from its PreCompact hook
    /// until its SessionStart with source `compact`, or the turn's end; the state stays the turn's own, so a turn
    /// that compacts is still a turn that runs (the owner, N-541 post 35).
    #[test]
    fn a_compaction_shows_while_it_runs() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-compacting-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"orch\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.hook("orch", "start", &json!({"source": "startup", "session_id": "s1", "cwd": "/d/orch", "transcript_path": "/t/s1.jsonl"}));
        core.agents.get_mut("orch").unwrap().live.pid = Some(std::process::id() as i32);
        core.begin_turn("orch");
        core.hook("orch", "compact", &json!({"trigger": "auto"}));
        let said = |core: &Core| (core.view("orch").compacting.is_some(), core.view("orch").state);
        let during = said(&core);
        core.hook("orch", "start", &json!({"source": "compact", "session_id": "s1", "cwd": "/d/orch", "transcript_path": "/t/s1.jsonl"}));
        let after = said(&core);
        core.hook("orch", "compact", &json!({"trigger": "manual"}));
        core.end_turn("orch");
        let ended = core.view("orch").compacting;
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!(during, (true, "working".to_string()), "it compacts, and its turn runs");
        assert_eq!(after, (false, "working".to_string()));
        assert_eq!(ended, None, "the turn's end ends it too");
    }

    /// A compaction Claude Code refuses ("Not enough messages to compact") runs its PreCompact hook and sends no
    /// SessionStart (docket-try3, 17 Nov): the holder's screen idle again ends the mark, and so does a new prompt
    /// (paper-research N-101 p1445).
    #[test]
    fn a_refused_compaction_does_not_stay_on() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-refused-compact-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"orch\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.hook("orch", "start", &json!({"source": "startup", "session_id": "s1", "cwd": "/d/orch", "transcript_path": "/t/s1.jsonl"}));
        core.agents.get_mut("orch").unwrap().live.pid = Some(std::process::id() as i32);
        // refused while idle: the screen showed the hook running, then is idle again
        core.hook("orch", "compact", &json!({"trigger": "manual"}));
        core.busy("orch", true);
        let during = core.view("orch").compacting.is_some();
        core.busy("orch", false);
        let idle_again = core.view("orch").compacting;
        // refused, then a prompt before the holder reported the screen idle
        core.hook("orch", "compact", &json!({"trigger": "manual"}));
        core.hook("orch", "prompt", &json!({"prompt": "go on"}));
        let prompted = core.view("orch").compacting;
        let _ = std::fs::remove_dir_all(&coord);
        assert!(during, "the screen busy is the compaction itself");
        assert_eq!(idle_again, None, "the screen idle again ends it");
        assert_eq!(prompted, None, "a new prompt ends it");
    }
}

#[cfg(test)]
mod role_tests {
    use super::*;

    /// N-74: an agent added, given a role, then added again keeps its role through a replay of the registry, and the
    /// norms' facts name it; the live re-add keeps it too.
    #[test]
    fn a_re_add_keeps_the_role() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-role-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), concat!(
            "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"CONSULT\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"1\"}\n",
            "{\"ev\":\"role\",\"name\":\"CONSULT\",\"role\":\"consultant\",\"at\":\"2\"}\n",
            "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"CONSULT\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m2\",\"at\":\"3\"}\n",
        )).unwrap();
        let has_role = |core: &Core| core.norm_facts(&[]).contains(&("role".to_string(), vec!["CONSULT".to_string(), "consultant".to_string()]));
        let core = Core::load(&coord.display().to_string(), &base);
        assert_eq!(core.agents["CONSULT"].role, "consultant");
        assert_eq!(core.agents["CONSULT"].model, "m2");
        assert!(has_role(&core));
        // the live path: an add for a name already there, then a replay of what it wrote
        let mut core = core;
        let r = core.add(&Who::Owner, "CONSULT", "claude", "m3");
        assert!(r.ok, "{:?}", r.out);
        assert!(has_role(&core));
        let core = Core::load(&coord.display().to_string(), &base);
        assert_eq!((core.agents["CONSULT"].role.as_str(), core.agents["CONSULT"].model.as_str()), ("consultant", "m3"));
        assert!(has_role(&core));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A registry naming a transcript where a container wrote it is read where `sessions-moved` says the folder
    /// went: the agent's transcript, its sessions and `coord log`'s page are the moved file; a folder that only begins
    /// with the same letters is not moved, and with no file the path is the one named (paper-research N-101 p1060).
    #[test]
    fn a_moved_session_is_read_where_it_went() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-moved-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        let kept = coord.join("claude-projects/-home-agent-x");
        std::fs::create_dir_all(&kept).unwrap();
        std::fs::write(kept.join("s1.jsonl"), "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n").unwrap();
        std::fs::write(kept.join("s2.jsonl"), "").unwrap();
        let named = "/nowhere-docket-moved/.claude/projects/-home-agent-x/s1.jsonl";
        let near = "/nowhere-docket-moved/.claude/projects-old/-home-agent-x/s2.jsonl";
        let session = |name: &str, t: &str| format!("{{\"ev\":\"session\",\"name\":\"{name}\",\"session\":{{\"transcript_path\":\"{t}\"}},\"at\":\"2\"}}\n");
        std::fs::write(coord.join("registry.jsonl"), [
            "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"dev\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"1\"}\n".to_string(),
            "{\"ev\":\"add\",\"id\":\"AGT-002\",\"name\":\"near\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"1\"}\n".to_string(),
            session("dev", named),
            session("near", near),
        ].concat()).unwrap();
        let c = coord.display().to_string();
        let core = Core::load(&c, &base);
        assert_eq!(core.agents["dev"].transcript().as_deref(), Some(named), "no sessions-moved: the path named");
        std::fs::write(coord.join("sessions-moved"), format!("# the container's home\n\n/nowhere-docket-moved/.claude/projects/ {c}/claude-projects\n")).unwrap();
        let core = Core::load(&c, &base);
        let there = format!("{c}/claude-projects/-home-agent-x/s1.jsonl");
        assert_eq!(core.agents["dev"].transcript(), Some(there.clone()));
        assert_eq!(core.agents["dev"].sessions, vec![there.clone()]);
        assert_eq!(core.agents["near"].transcript().as_deref(), Some(near));
        let (session, _) = core.transcript_of(&Who::Owner, "dev", None).unwrap().unwrap();
        assert_eq!(session, there);
        let page = serde_json::to_string(&transcript::page(&session, None, None)).unwrap();
        assert!(page.contains("hello"), "{page}");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// An agent that ran when coordd went down and runs no more is handed to `roster resume`, unless it was made for a
    /// step in a layer: resumed by roster it would run outside its view and its box (15 Nov).
    #[test]
    fn an_agent_of_a_layer_is_not_resumed_outside_it() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-resume-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(coord.join("agents/f3-1")).unwrap();
        std::fs::write(coord.join("agents/f3-1/layer"), "E-3.l1\n").unwrap();
        std::fs::write(coord.join("registry.jsonl"), concat!(
            "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"dev\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"1\"}\n",
            "{\"ev\":\"add\",\"id\":\"AGT-002\",\"name\":\"f3-1\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"2\"}\n",
        )).unwrap();
        std::fs::write(coord.join("running.json"), r#"["dev","f3-1"]"#).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.hand_over_running(&[]);
        let resume: Vec<String> = serde_json::from_slice(&std::fs::read(coord.join("resume.json")).unwrap()).unwrap();
        assert_eq!(resume, vec!["dev".to_string()]);
        // its next start goes on with a turn that end cut (paper-research N-1104 p785)
        assert!(core.agents["dev"].down_cut);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A program tab (the runner) that ran when coordd went down is handed to `roster resume` as an agent is, which
    /// starts it again from its tab.json; one still running in its cgroup (coordd alone restarted) is not: no second copy.
    #[test]
    fn a_program_tab_gone_with_the_machine_is_handed_to_roster_resume_one_still_running_is_not() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-resume-program-{}", std::process::id()));
        for recovered in [vec![], vec![("runner".to_string(), 7)]] {
            let _ = std::fs::remove_dir_all(&coord);
            std::fs::create_dir_all(coord.join("agents/runner")).unwrap();
            std::fs::write(coord.join("agents/runner/tab.json"), r#"{"dir":"/d","cmd":["sh","-c","exec runner"]}"#).unwrap();
            std::fs::write(coord.join("registry.jsonl"), concat!(
                "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"dev\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"1\"}\n",
                "{\"ev\":\"add\",\"id\":\"AGT-002\",\"name\":\"runner\",\"parent\":\"owner\",\"kind\":\"program\",\"model\":\"-\",\"at\":\"2\"}\n",
            )).unwrap();
            std::fs::write(coord.join("running.json"), r#"["dev","runner"]"#).unwrap();
            let mut core = Core::load(&coord.display().to_string(), &base);
            core.hand_over_running(&recovered);
            let resume: Vec<String> = serde_json::from_slice(&std::fs::read(coord.join("resume.json")).unwrap()).unwrap();
            let want: &[&str] = if recovered.is_empty() { &["dev", "runner"] } else { &["dev"] };
            assert_eq!(resume, want);
            // a program has no turn to go on with
            assert!(core.agents["dev"].down_cut && !core.agents["runner"].down_cut);
        }
        let _ = std::fs::remove_dir_all(&coord);
    }
}

#[cfg(test)]
mod host_tests {
    use super::*;
    use docket_machine::Text;

    /// A note naming a host session reaches it as mail did: a relay line in the feed for coord-relay, with the host's
    /// footer; given then, so not again; its reply by `coord msg --thread` is a post on the note (paper-research m1052).
    /// The next host job sent, other jobs (a ticket's Waits) passed over.
    fn host_job(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Job>) -> Option<(String, Vec<(docket_machine::Delivery, docket_machine::TicketView)>)> {
        while let Ok(j) = rx.try_recv() {
            if let Job::Host { name, deliveries } = j {
                return Some((name, deliveries));
            }
        }
        None
    }

    #[test]
    fn a_note_to_a_host_session_goes_through_the_relay() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-host-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(coord.join("hosts")).unwrap();
        std::fs::write(coord.join("hosts/research"), "00000000-0000-0000-0000-000000000000\n").unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"lead\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        assert_eq!(core.hosts(), vec!["research".to_string()]);
        // not without the project file's host_relay: a dev or scratch coord writes no relay line
        let r = core.ticket_msg(&Who::Agent("lead".into()), None, vec!["research".into()], "first", false, None, Vec::new(), crate::ticket::Said::default());
        assert!(r.ok, "{}", r.out);
        core.hand_hosts();
        assert!(host_job(&mut rx).is_none(), "host_relay off: nothing handed");
        core.project = Some(crate::project::Project { project: crate::project::Header { host_relay: true, ..Default::default() }, ..Default::default() });
        core.hand_hosts();
        let (_, first) = host_job(&mut rx).expect("a host job");
        for (d, _) in &first {
            core.ticket_given("research", d);
        }
        let r = core.ticket_msg(&Who::Agent("lead".into()), None, vec!["research".into()], "Is the board right?", false, None, Vec::new(), crate::ticket::Said::default());
        assert!(r.ok, "{}", r.out);
        core.hand_hosts();
        let (name, deliveries) = host_job(&mut rx).expect("a host job");
        assert_eq!((name.as_str(), deliveries.len()), ("research", 1));
        core.hand_hosts();
        assert!(host_job(&mut rx).is_none(), "on its way: not handed twice");
        let (d, view) = &deliveries[0];
        let (content, meta) = crate::render::delivery(&core.places(), d, view, false, false, false, true, None).unwrap();
        assert!(content.ends_with(&format!("Reply with: coord msg --thread {} TEXT", d.ticket)), "{content}");
        assert!(!content.contains("End your turn"), "{content}");
        core.to_host("research", d, &content, &meta);
        let feed = std::fs::read_to_string(coord.join("feed.jsonl")).unwrap();
        let line: Value = serde_json::from_str(feed.lines().last().unwrap()).unwrap();
        assert_eq!((line["kind"].as_str(), line["delivery"].as_str(), line["to"].as_str(), line["from"].as_str()), (Some("msg"), Some("relay"), Some("research"), Some("lead")));
        assert!(line["text"].as_str().unwrap().starts_with(&format!("[{} · from lead · ticket {} thread]\nlead wrote:\nIs the board right?", d.id, d.ticket)), "{line}");
        assert!(core.ticket_due("research").is_empty(), "given");
        // the host answers on the thread: a post on the note, which tells lead
        let r = core.ticket_msg(&Who::Owner, Some("research".into()), Vec::new(), "Yes, it is.", false, Some(d.ticket.clone()), Vec::new(), crate::ticket::Said::default());
        assert!(r.ok, "{}", r.out);
        let x = &core.tickets.m.tickets[&d.t];
        let _ = std::fs::remove_dir_all(&coord);
        let p = x.posts.last().unwrap();
        assert_eq!((p.by.as_str(), p.text.clone(), p.names.clone()), ("research", Text::New("Yes, it is.".into()), vec!["lead".to_string()]));
    }
}

#[cfg(test)]
mod hold_tests {
    use super::*;
    use docket_machine::{Input, Loc, Text};

    /// While $COORD/hold is there only the owner's posts go; the rest goes when it is lifted (docket-core m1072).
    #[test]
    fn the_hold_holds_ticket_deliveries_but_the_owners_posts() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-hold-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"lead\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n{\"ev\":\"add\",\"id\":\"AGT-002\",\"name\":\"w2\",\"parent\":\"lead\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let open = |core: &mut Core, by: &str, text: &str| {
            let r = core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: by.into(), form: json!({"to": by, "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": by}, "follows": null}), text: Some(Text::New(text.into())), names: vec!["w2".into()], told: vec![] });
            assert!(r.is_ok(), "{r:?}");
        };
        std::fs::write(coord.join("hold"), "").unwrap();
        open(&mut core, "lead", "from lead");
        open(&mut core, "owner", "from the owner");
        let held: Vec<String> = core.due_now("w2").iter().map(|d| d.id.clone()).collect();
        assert_eq!((core.ticket_due("w2").len(), held.len()), (2, 1), "{held:?}");
        std::fs::remove_file(coord.join("hold")).unwrap();
        *core.hold.borrow_mut() = (None, None); // the 2 s look-again, passed
        let lifted = core.due_now("w2").len();
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!(lifted, 2);
    }

    fn coord_of(tag: &str) -> (std::path::PathBuf, Core) {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"lead\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n{\"ev\":\"add\",\"id\":\"AGT-002\",\"name\":\"w2\",\"parent\":\"lead\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        (coord, core)
    }

    /// An agent's mode: its own when set, else the project file's `agent_mode`, else `yaaihh.toml`'s beside the
    /// coords, else Safe; written to session.json with where it came from, for roster's next start; a new agent takes
    /// the default at its registration; a project file with another word for a mode does not load.
    /// An agent's renderings set is its add line's or its last `renderings` line's, a re-add keeps it, and every other
    /// agent has none; its deliveries are rendered from it (`places_for`). A set that is no folder's name is refused.
    #[test]
    fn an_agents_renderings_are_its_registry_lines() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let top = std::env::temp_dir().join(format!("renderings-reg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let coord = top.join("coord");
        std::fs::create_dir_all(&coord).unwrap();
        let add = |n: &str, more: &str| format!("{{\"ev\":\"add\",\"id\":\"AGT-{n}\",\"name\":\"{n}\",\"parent\":\"owner\",\"kind\":\"agy\",\"model\":\"m\",\"at\":\"x\"{more}}}\n");
        let lines = [add("pa", ",\"renderings\":\"words\""), add("pb", ""), add("pc", ""), "{\"ev\":\"renderings\",\"name\":\"pb\",\"set\":\"short\"}\n".into(), add("pb", ""), add("pd", ",\"renderings\":\"words\""), "{\"ev\":\"renderings\",\"name\":\"pd\",\"set\":null}\n".into()];
        std::fs::write(coord.join("registry.jsonl"), lines.concat()).unwrap();
        let core = Core::load(&coord.display().to_string(), &base);
        let set = |c: &Core, n: &str| c.places_for(n).renderings;
        assert_eq!([set(&core, "pa"), set(&core, "pb"), set(&core, "pc"), set(&core, "pd")], [Some("words".into()), Some("short".into()), None, None]);
        // `coord add --renderings`: registered with it, and so it is read again at the next start
        let mut core = Core::load(&coord.display().to_string(), &base);
        assert!(core.add(&Who::Owner, "neon", "agy", "m").ok);
        core.set_renderings("neon", "words").unwrap();
        assert!(core.set_renderings("neon", "../x").is_err() && core.set_renderings("neon", ".x").is_err() && core.set_renderings("neon", "").is_err());
        let again = Core::load(&coord.display().to_string(), &base);
        assert_eq!((set(&again, "neon"), set(&again, "pc")), (Some("words".into()), None));
        let _ = std::fs::remove_dir_all(&top);
    }

    #[test]
    fn an_agents_mode_is_its_own_else_the_projects_else_the_yaaihh_wide_one() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let top = std::env::temp_dir().join(format!("acc-mode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let coord = top.join("coord");
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"lead\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n{\"ev\":\"add\",\"id\":\"AGT-002\",\"name\":\"w2\",\"parent\":\"lead\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let c = coord.display().to_string();
        let mut core = Core::load(&c, &base);
        let of = |core: &Core, n: &str| (core.agents[n].yolo, core.agents[n].mode_from);
        let file = |n: &str| -> (Option<String>, Option<String>) {
            let v: Value = serde_json::from_str(&std::fs::read_to_string(coord.join(format!("agents/{n}/session.json"))).unwrap_or_default()).unwrap_or(Value::Null);
            (v["mode"].as_str().map(String::from), v["mode_from"].as_str().map(String::from))
        };
        let project = |mode: &str| {
            std::fs::write(top.join("project.toml"), format!("[project]\nagent_mode = \"{mode}\"\n")).unwrap();
            std::fs::write(coord.join("project"), top.join("project.toml").display().to_string()).unwrap();
            crate::project::Project::load(&c)
        };
        core.resolve_modes();
        assert_eq!(of(&core, "lead"), (false, ""), "nothing set: Safe");
        // for every project
        std::fs::write(top.join("yaaihh.toml"), "agent_mode = \"yolo\"\n").unwrap();
        core.resolve_modes();
        assert_eq!((of(&core, "lead"), of(&core, "w2")), ((true, "yaaihh"), (true, "yaaihh")));
        assert_eq!(file("lead"), (Some("yolo".into()), Some("yaaihh".into())), "roster's next start reads it");
        // the project's over it
        core.project = project("safe").unwrap();
        core.resolve_modes();
        assert_eq!(of(&core, "lead"), (false, "project"));
        assert_eq!(file("lead"), (None, Some("project".into())));
        // the agent's own over both, kept over a load from the registry
        assert!(core.mode(&Who::Owner, "lead", "yolo").out.contains("lead: Safe → Yolo"));
        core.resolve_modes();
        assert_eq!((of(&core, "lead"), of(&core, "w2")), ((true, "agent"), (false, "project")));
        let mut again = Core::load(&c, &base);
        again.project = project("safe").unwrap();
        again.resolve_modes();
        assert_eq!((of(&again, "lead"), of(&again, "w2")), ((true, "agent"), (false, "project")));
        // a new agent takes the default as it is registered
        again.project = project("yolo").unwrap();
        again.resolve_modes();
        assert!(again.add(&Who::Owner, "w3", "claude", "claude-haiku-5-5").out.starts_with("AGT-"));
        assert_eq!(of(&again, "w3"), (true, "project"));
        assert_eq!(file("w3"), (Some("yolo".into()), Some("project".into())));
        // an agy agent too (N-1956), not a Codex one, which has no such mode
        assert!(again.add(&Who::Owner, "g4", "agy", "agy/gemini-3.8-flash-high").out.starts_with("AGT-"));
        assert!(again.add(&Who::Owner, "x5", "codex", "gpt-x").out.starts_with("AGT-"));
        assert_eq!((of(&again, "g4"), of(&again, "x5")), ((true, "project"), (false, "")));
        assert_eq!((file("g4"), file("x5").0), ((Some("yolo".into()), Some("project".into())), None));
        let refused = project("auto").err().unwrap_or_default();
        let _ = std::fs::remove_dir_all(&top);
        assert!(refused.contains("agent_mode = \"auto\" is not a mode"), "{refused}");
    }

    /// A Claude Code turn's end taken by its Stop hook before its transcript showed it (paper-research N-101 p1412):
    /// what is due (here the owner's post) is not handed until the turn's reply is read, so it cannot begin the next
    /// turn first; then it is.
    #[test]
    fn nothing_is_handed_before_the_turns_reply_is_read() {
        let (coord, mut core) = coord_of("owed");
        core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("when you are done".into())), names: vec!["w2".into()], told: vec![] }).unwrap();
        let w2 = core.agents.get_mut("w2").unwrap();
        w2.live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), turn: 1, ..Default::default() };
        w2.ended_turn = 1;
        core.owe_output("w2");
        let held = core.to_hand_over();
        core.output_read("w2");
        let then = core.to_hand_over();
        let _ = std::fs::remove_dir_all(&coord);
        assert!(held.is_empty(), "{held:?}");
        assert_eq!(then, vec!["w2".to_string()]);
    }

    /// A turn stopped on the gateway's 429 (paper-research N-1397 p142; the words are those of the row docket-core's
    /// transcript ended with at 02:11:39Z on 21 Nov): held from every hand-over until the reset its words name, its
    /// parent not told; then given its turn again once, in coord's words ahead of what came for it meanwhile, again
    /// with the retry when its session did not take it, and again after a coordd restart before it was taken (its limit
    /// kept in the journal till then, nothing of the hand-over: consult-3 N-1397 p151). Words that name no reset hold it
    /// for the policy's key-again-after (60 s in agent-base); a turn begun meanwhile ends the hold; another error is
    /// told to its parent as before.
    #[test]
    fn a_turn_stopped_on_a_rate_limit_is_held_until_its_reset_then_given_its_turn_again_once() {
        let (coord, mut core) = coord_of("limit");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        let now = crate::accounts::mover::now_s;
        let notices = |core: &Core| core.tickets.m.tickets.values().filter(|x| x.opener.as_deref() == Some("coord")).count();
        let channel = |rx: &mut tokio::sync::mpsc::UnboundedReceiver<Job>| std::iter::from_fn(|| rx.try_recv().ok()).find_map(|j| match j {
            Job::Channel { name, ids, line, tickets, .. } if name == "w2" => Some((ids, line, tickets.len())),
            _ => None,
        });
        let row = "API Error: Request rejected (429) · All credentials for model claude-opus-5-5 are cooling down (last error: rate_limit_error: This request would exceed your account's rate limit. Please try again later.) (reset after 18m48s)";
        core.hook("w2", "prompt", &json!({}));
        let before = now();
        core.hook("w2", "failed", &json!({"hook_event_name": "StopFailure", "error": "rate_limit", "last_assistant_message": row}));
        let l = core.agents["w2"].live.limit.clone().expect("held");
        assert!((before + 1128..=now() + 1128).contains(&l.until) && l.why == row, "{l:?}");
        rt.block_on(core.settled("w2", Vec::new()));
        assert_eq!(notices(&core), 0, "its parent is not told");
        // the owner's post comes for it meanwhile: held, and its state says until when
        core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("when the limit lets you".into())), names: vec!["w2".into()], told: vec![] }).unwrap();
        assert!(core.to_hand_over().is_empty(), "held until the reset");
        let state = core.agent_state("w2");
        assert!(matches!(&state, Some(AgentState::Starting { why }) if why.contains("rate limit") && why.contains(&at_secs(l.until))), "{state:?}");
        // and its view, so his row says so, not "idle since" (proto-lead N-1397 p160), through an event as sent
        let v = core.view("w2");
        assert_eq!((v.state.as_str(), v.limit_until, v.limit_why.as_deref()), ("idle", Some(l.until), Some(row)));
        let line = serde_json::to_string(&coord_types::Event { seq: 1, body: coord_types::Body::Agent(v.clone()) }).unwrap();
        let back: coord_types::Event = serde_json::from_str(&line).unwrap_or_else(|e| panic!("the window cannot read it: {e}\n{line}"));
        assert_eq!(back.body, coord_types::Body::Agent(v), "{line}");
        // the reset passes: its turn given again, with the post
        core.agents.get_mut("w2").unwrap().live.limit.as_mut().unwrap().until = now() - 1;
        let l = core.agents["w2"].live.limit.clone().unwrap();
        // till it is given, its view still says so: the time passed, the turn not given yet, as while his mail hold
        // keeps it (proto-window N-1397 p163)
        assert_eq!(core.view("w2").limit_until, Some(l.until), "held past its time");
        assert!(core.ticket_mail_hold(&Who::Owner, true).ok);
        // (his own post, due here, is not kept: what his hold keeps is the resume)
        assert!(!core.resume_due(&core.agents["w2"]), "his hold keeps its resume");
        assert_eq!(core.view("w2").limit_until, Some(l.until), "held past its time, under his hold");
        assert!(core.ticket_mail_hold(&Who::Owner, false).ok);
        assert_eq!(core.to_hand_over(), vec!["w2".to_string()]);
        core.hand_over("w2");
        let (ids, line, n) = channel(&mut rx).expect("handed");
        assert!(line.contains("Your last turn stopped at") && line.contains("(reset after 18m48s)") && line.contains("go on with that turn") && line.contains("\"from\":\"coord\""), "{line}");
        assert_eq!((n, core.agents["w2"].live.limit.as_ref()), (1, Some(&l)), "the owner's post with it, the limit kept till it is taken");
        // coordd restarts before its session took it: the journal holds the limit, nothing is on its way, and the next
        // hand-over gives its turn again
        let journal = serde_json::to_vec(&core.live_journalled()).unwrap();
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let mut again = Core::load(&coord.display().to_string(), &base);
        again.load_tickets();
        let saved: HashMap<String, Live> = serde_json::from_slice(&journal).unwrap();
        again.agents.get_mut("w2").unwrap().live = saved["w2"].clone();
        let (tx2, mut rx2) = tokio::sync::mpsc::unbounded_channel();
        again.jobs = Some(tx2);
        assert!(again.inflight.is_empty());
        assert_eq!(again.to_hand_over(), vec!["w2".to_string()], "given again after the restart");
        again.hand_over("w2");
        let (_, line, _) = channel(&mut rx2).expect("handed after the restart");
        assert!(line.contains("Your last turn stopped at") && line.contains("(reset after 18m48s)"), "{line}");
        drop(again);
        // not taken: given again with the retry
        core.handed("w2", &ids, false, "channel");
        assert_eq!(core.agents["w2"].live.limit.as_ref(), Some(&l), "kept");
        core.retry_at.remove("w2");
        core.hand_over("w2");
        let (ids, line, _) = channel(&mut rx).expect("handed again");
        assert!(line.contains("Your last turn stopped at"), "{line}");
        // taken: once, and the limit is over, in its view too
        core.handed("w2", &ids, true, "channel");
        assert!(core.agents["w2"].live.limit.is_none(), "taken");
        assert_eq!((core.view("w2").limit_until, core.view("w2").limit_why), (None, None));
        core.hand_over("w2");
        assert!(channel(&mut rx).is_none_or(|(_, line, _)| !line.contains("Your last turn stopped at")), "given once");
        // words that name no reset: the policy's key-again-after; a turn begun meanwhile ends the hold
        core.hook("w2", "failed", &json!({"error": "rate_limit", "last_assistant_message": "API Error: Request rejected (429) · This request would exceed your account's rate limit. Please try again later."}));
        let l = core.agents["w2"].live.limit.clone().expect("held");
        assert_eq!(l.until - l.at, 60, "{l:?}");
        core.begin_turn("w2");
        assert!(core.agents["w2"].live.limit.is_none(), "a turn began");
        // another error: its parent is told, as before (overloaded is waited out since the owner's walk item 14)
        core.hook("w2", "failed", &json!({"error": "invalid_request"}));
        assert!(core.agents["w2"].live.limit.is_none());
        rt.block_on(core.settled("w2", Vec::new()));
        let told = notices(&core);
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!(told, 1, "the notice of the other error");
    }

    /// A turn held on the gateway's 429 "till 13:31" goes on the first time its model is served again the same way, not
    /// at that reset (paper-research N-1446 p873; proto-lead p886's two tests; p894's press). Stopped at "11:01", with
    /// nothing answered and nothing pressed it waits; a reply on its model in a session on another place (lead on his
    /// account), coordd's own call of another model, or one sent before the stop, frees nothing; lead's session on the
    /// gateway answered on claude-opus-5-5 at "11:06" frees it in that hand-over, once, in words that say what served
    /// it. coordd's own call of the model frees it too, and so does his press on the account it runs on, which leaves
    /// one held on the gateway held; that one his press on a claude gateway login frees (`Core::gateway_cleared`),
    /// once, and not one held there on a Codex model, which a codex login's press frees, and coordd's own call of its
    /// model again.
    #[test]
    fn a_turn_held_on_a_rate_limit_goes_on_once_its_model_is_answered_again_the_same_way() {
        let (coord, mut core) = coord_of("served-again");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        let now = crate::accounts::mover::now_s;
        let channel = |rx: &mut tokio::sync::mpsc::UnboundedReceiver<Job>| std::iter::from_fn(|| rx.try_recv().ok()).find_map(|j| match j {
            Job::Channel { name, ids, line, .. } if name == "w2" => Some((ids, line)),
            _ => None,
        });
        let iso = |t: i64| chrono::DateTime::from_timestamp(t, 0).unwrap().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let row = "API Error: Request rejected (429) · All credentials for model claude-opus-5-5 are cooling down (last error: rate_limit_error: This request would exceed your account's rate limit. Please try again later.) (reset after 2h29m10s)";
        for (name, model) in [("w2", "claude-opus-5-5:high"), ("lead", "claude-opus-5-5")] {
            let a = core.agents.get_mut(name).unwrap();
            a.model = model.into();
            a.live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        }
        // w2 through the gateway stops on the 429 at "11:01", five and a half minutes ago: held till "13:31"
        let hold = |core: &mut Core, ago: i64| {
            core.hook("w2", "prompt", &json!({}));
            core.hook("w2", "failed", &json!({"hook_event_name": "StopFailure", "error": "rate_limit", "last_assistant_message": row}));
            let l = core.agents.get_mut("w2").unwrap().live.limit.as_mut().expect("held");
            (l.at, l.until) = (now() - ago, now() - ago + 8950);
            l.at
        };
        let stop = hold(&mut core, 330);
        let six = stop + 300;
        let until = |core: &Core| core.agents["w2"].live.limit.as_ref().map(|l| l.until);
        // nothing answered, nothing pressed: it waits for the reset
        assert!(core.to_hand_over().is_empty() && until(&core) == Some(stop + 8950), "held till the reset");
        // lead on his account `work`, answered on the model at 11:06: another way, so w2 waits
        {
            let lead = core.agents.get_mut("lead").unwrap();
            (lead.native, lead.account) = (true, Some("work".into()));
        }
        core.answered("lead", &[("claude-opus-5-5".into(), iso(six))]);
        // coordd's own call of another model at 11:06, and of the model sent before the stop
        crate::accounts::router::note_served(&core.coord, crate::accounts::router::GATEWAY, "claude-sonnet-5-5", six);
        crate::accounts::router::note_served(&core.coord, crate::accounts::router::GATEWAY, "claude-opus-5-5", stop - 5);
        assert!(core.to_hand_over().is_empty() && until(&core) == Some(stop + 8950), "nothing of these frees it");
        // lead through the gateway, its reply asked at 11:06: w2 goes on in this hand-over, once
        {
            let lead = core.agents.get_mut("lead").unwrap();
            (lead.native, lead.account) = (false, None);
        }
        core.answered("lead", &[("claude-opus-5-5".into(), iso(six - 20)), ("claude-opus-5-5".into(), iso(six))]);
        let l = core.agents["w2"].live.limit.clone().expect("still held till it is given");
        let by = format!("the gateway answered lead's claude-opus-5-5 at {}", at_secs(six));
        assert_eq!((l.until, l.served.as_deref()), (six, Some(by.as_str())));
        assert_eq!(core.to_hand_over(), vec!["w2".to_string()]);
        core.hand_over("w2");
        let (ids, line) = channel(&mut rx).expect("handed");
        assert!(line.contains("Your last turn stopped at") && line.contains(&format!("Its model is served again ({by}): go on with that turn")), "{line}");
        core.handed("w2", &ids, true, "channel");
        assert!(core.agents["w2"].live.limit.is_none(), "taken");
        core.answered("lead", &[("claude-opus-5-5".into(), iso(now()))]);
        assert!(core.to_hand_over().is_empty() && channel(&mut rx).is_none(), "given once");
        // held again: coordd's own call of the model, sent after the stop, frees it (its effort aside)
        let stop = hold(&mut core, 100);
        crate::accounts::router::note_served(&core.coord, crate::accounts::router::GATEWAY, "claude-opus-5-5:low", stop + 50);
        assert_eq!(core.to_hand_over(), vec!["w2".to_string()]);
        assert_eq!(core.agents["w2"].live.limit.as_ref().and_then(|l| l.served.clone()), Some(format!("the gateway answered coordd's own call of claude-opus-5-5 at {}", at_secs(stop + 50))));
        core.hand_over("w2");
        let (ids, _) = channel(&mut rx).expect("handed");
        core.handed("w2", &ids, true, "channel");
        // w2 on his account `work`, held again, and lead held through the gateway: his press on `work` frees w2 alone
        {
            let w2 = core.agents.get_mut("w2").unwrap();
            (w2.native, w2.account) = (true, Some("work".into()));
        }
        hold(&mut core, 100);
        core.hook("lead", "prompt", &json!({}));
        core.hook("lead", "failed", &json!({"hook_event_name": "StopFailure", "error": "rate_limit", "last_assistant_message": row}));
        core.agents.get_mut("lead").unwrap().live.limit.as_mut().unwrap().at = now() - 100;
        core.owner_cleared("work", "his press, it can take requests,");
        let served = core.agents["w2"].live.limit.as_ref().and_then(|l| l.served.clone()).unwrap_or_default();
        assert!(served.starts_with("his press, it can take requests, on account work at "), "{served}");
        assert!(core.agents["lead"].live.limit.as_ref().is_some_and(|l| l.until > now() && l.served.is_none()), "lead, through the gateway, still held");
        assert_eq!(core.to_hand_over(), vec!["w2".to_string()]);
        core.hand_over("w2");
        let (ids, _) = channel(&mut rx).expect("handed");
        core.handed("w2", &ids, true, "channel");
        // his press on a gateway login (`Request::GatewayCleared`, N-1446 p957), the owner's only, frees those held
        // through the gateway on that family's models, once: a claude login's frees lead on claude-opus-5-5, and not w2
        // on a Codex model the gateway's list names (proto-reader p962); a codex login's then frees w2
        {
            let w2 = core.agents.get_mut("w2").unwrap();
            (w2.native, w2.account, w2.model) = (false, None, "gpt-6.2-sol:low".into());
        }
        hold(&mut core, 100);
        crate::accounts::router::gateway_read(&core.coord, &[("gpt-6.2-sol", "openai")], now());
        let held = |core: &Core, name: &str| core.agents[name].live.limit.as_ref().is_some_and(|l| l.until > now() && l.served.is_none());
        let r = core.gateway_cleared(&Who::Agent("lead".into()), "claude");
        assert!(!r.ok && held(&core, "lead"), "the owner's only: {}", r.out);
        let r = core.gateway_cleared(&Who::Owner, "Claude");
        assert_eq!((r.ok, r.out.as_str()), (true, "gateway: 1 held on claude models go on (lead)"));
        assert!(held(&core, "w2"), "a claude login's press frees no Codex agent");
        let served = core.agents["lead"].live.limit.as_ref().and_then(|l| l.served.clone()).unwrap_or_default();
        assert!(served.starts_with("his press cleared a claude login's cooldown on the gateway at "), "{served}");
        assert_eq!(core.to_hand_over(), vec!["lead".to_string()]);
        assert_eq!(core.gateway_cleared(&Who::Owner, "claude").out, "gateway: none held on claude models", "once");
        assert_eq!(core.gateway_cleared(&Who::Owner, "codex").out, "gateway: 1 held on codex models go on (w2)");
        assert_eq!(core.to_hand_over(), vec!["lead".to_string(), "w2".to_string()]);
        // held again on gpt-6.2-sol: coordd's own call of it, answered by a snapshot the gateway's list does not give, is
        // noted for gpt-6.2-sol (`Router::on_gateway`; consult-3 p1015, proto-lead p1019), and w2 goes on
        core.hand_over("w2");
        let (ids, _) = channel(&mut rx).expect("handed");
        core.handed("w2", &ids, true, "channel");
        hold(&mut core, 100);
        assert!(held(&core, "w2"));
        crate::accounts::router::note_served(&core.coord, crate::accounts::router::GATEWAY, "gpt-6.2-sol", now());
        assert!(core.to_hand_over().contains(&"w2".to_string()));
        let served = core.agents["w2"].live.limit.as_ref().and_then(|l| l.served.clone()).unwrap_or_default();
        assert!(served.starts_with("the gateway answered coordd's own call of gpt-6.2-sol at "), "{served}");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A 429 that names no reset, again and again (proto-lead N-1397 p151): each wait twice the one before, from the
    /// policy's key-again-after (60 s in agent-base), to 30 minutes at most, so an 18-minute limit gives its turn again
    /// about five times, not eighteen; a turn that ends by its Stop, or on another error, or that an esc stopped (no
    /// Stop hook runs then: consult-3, proto-reader N-1397 p157), puts it back at 60 s. A 429
    /// that names its reset waits for that and leaves the doubling as it was. Turns given again, each stopped on its
    /// 429 again, with a begin between as the hand-over makes.
    /// The owner's walk item 14 (yaaihh/fresh N-1729 p24): fresh's turn of 25 Nov 11:09:55Z ended on the provider's 529
    /// "Overloaded" once Claude Code's 8 retries were spent, and stayed stopped till he typed. A turn Claude Code's
    /// StopFailure ends on `overloaded` or `server_error` is waited out as a rate limit naming no reset is, from the
    /// policy's key-again-after: held, its parent not told, its state and its row saying what stopped it and when it
    /// goes on, then given its turn again once, in coord's words; another error is still its parent's to hear of.
    #[test]
    fn a_turn_stopped_on_the_provider_overloaded_is_waited_out_and_given_again() {
        let (coord, mut core) = coord_of("overloaded");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        let now = crate::accounts::mover::now_s;
        let notices = |core: &Core| core.tickets.m.tickets.values().filter(|x| x.opener.as_deref() == Some("coord")).count();
        let row = "API Error: 529 {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}";
        // two in a row: the second waits twice as long, as a rate limit naming no reset does
        for (error, what, secs) in [("overloaded", "the provider overloaded", 60), ("server_error", "a server error", 120)] {
            core.hook("w2", "prompt", &json!({}));
            let before = now();
            core.hook("w2", "failed", &json!({"hook_event_name": "StopFailure", "error": error, "last_assistant_message": row}));
            let l = core.agents["w2"].live.limit.clone().unwrap_or_else(|| panic!("{error}: held"));
            assert!((before + secs..=now() + secs).contains(&l.until) && l.why == row && l.stopped_on() == what, "{error}: {l:?}");
            rt.block_on(core.settled("w2", Vec::new()));
            assert_eq!(notices(&core), 0, "{error}: its parent is not told");
            let state = core.agent_state("w2");
            assert!(matches!(&state, Some(AgentState::Starting { why }) if why.contains(what) && why.contains(&at_secs(l.until))), "{error}: {state:?}");
            assert_eq!(core.view("w2").limit_why, Some(format!("{what}: {row}")), "{error}: the row's words");
            assert!(core.to_hand_over().is_empty(), "{error}: held");
            // the wait passes: its turn given again, once
            core.agents.get_mut("w2").unwrap().live.limit.as_mut().unwrap().until = now() - 1;
            core.hand_over("w2");
            let line = std::iter::from_fn(|| rx.try_recv().ok()).find_map(|j| match j {
                Job::Channel { name, ids, line, .. } if name == "w2" => Some((ids, line)),
                _ => None,
            });
            let (ids, line) = line.unwrap_or_else(|| panic!("{error}: given again"));
            assert!(line.contains(&format!("Your last turn stopped at {} on {what}", at_secs(l.at))) && line.contains("The wait is over"), "{error}: {line}");
            core.handed("w2", &ids, true, "channel");
            assert!(core.agents["w2"].live.limit.is_none(), "{error}: taken");
        }
        // another error: its parent is told, as before
        core.hook("w2", "prompt", &json!({}));
        core.hook("w2", "failed", &json!({"hook_event_name": "StopFailure", "error": "invalid_request"}));
        assert!(core.agents["w2"].live.limit.is_none());
        rt.block_on(core.settled("w2", Vec::new()));
        let told = notices(&core);
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!(told, 1, "the notice of the other error");
    }

    /// N-1793 (the owner's overnight list, item 2): a Codex agent's turn that ends on the provider's overload or a
    /// server error, which codex-rpc's `failure` (as opencode-rpc's and agy-rpc's) now names in Claude Code's words, is
    /// held and waited out as a Claude Code agent's is, its parent not told; before, it came as Codex's own code
    /// (`serverOverloaded`) and the turn stayed stopped till he typed.
    #[test]
    fn a_codex_turn_stopped_on_the_provider_overloaded_is_waited_out_too() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-codex-overloaded-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"lead\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n{\"ev\":\"add\",\"id\":\"AGT-003\",\"name\":\"cx\",\"parent\":\"lead\",\"kind\":\"codex\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        core.agents.get_mut("cx").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        let notices = |core: &Core| core.tickets.m.tickets.values().filter(|x| x.opener.as_deref() == Some("coord")).count();
        for (error, what) in [("overloaded", "the provider overloaded"), ("server_error", "a server error")] {
            core.hook("cx", "prompt", &json!({}));
            let next = core.hook("cx", "failed", &json!({"session_id": "t1", "error": error, "error_details": "serverOverloaded · again"}));
            assert!(matches!(next, HookNext::CodexStop), "{error}: Codex's turn is ended by coordd as before");
            let l = core.agents["cx"].live.limit.clone().unwrap_or_else(|| panic!("{error}: held"));
            assert_eq!(l.stopped_on(), what, "{error}: {l:?}");
            rt.block_on(core.settled("cx", Vec::new()));
            assert_eq!(notices(&core), 0, "{error}: its parent is not told");
            core.agents.get_mut("cx").unwrap().live.limit = None;
        }
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-1956.p1 (no automatic retries on an agy account): an agy agent's turn that ends on the provider's overload, a
    /// server error or a rate limit naming no reset is not waited out and given again: its parent is told. One naming
    /// its reset is held till then, as any agent's.
    #[test]
    fn an_agy_turn_is_given_again_only_at_a_reset_google_named() {
        let (coord, mut core) = coord_of("agy-again");
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        core.agents.get_mut("w2").unwrap().kind = "agy".into();
        core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        let notices = |core: &Core| core.tickets.m.tickets.values().filter(|x| x.opener.as_deref() == Some("coord")).count();
        for (i, error) in ["overloaded", "server_error", "rate_limit"].into_iter().enumerate() {
            core.hook("w2", "prompt", &json!({}));
            core.hook("w2", "failed", &json!({"session_id": "s", "error": error, "error_details": "Error 503, Message: The model is overloaded."}));
            assert!(core.agents["w2"].live.limit.is_none(), "{error}: not held");
            rt.block_on(core.settled("w2", Vec::new()));
            assert_eq!(notices(&core), i + 1, "{error}: its parent is told");
        }
        // agy-rpc names Google's reset as its time (`resets_at`)
        core.hook("w2", "prompt", &json!({}));
        let at = crate::accounts::mover::now_s();
        core.hook("w2", "failed", &json!({"session_id": "s", "error": "rate_limit", "error_details": "Resource has been exhausted", "resets_at": at + 90}));
        let l = core.agents["w2"].live.limit.clone().expect("a named reset: held");
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!(l.until, at + 90);
    }

    #[test]
    fn a_rate_limit_that_names_no_reset_waits_twice_as_long_each_time_it_comes_back() {
        let (coord, mut core) = coord_of("limit-wait");
        core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        let none = json!({"error": "rate_limit", "last_assistant_message": "API Error: Request rejected (429) · This request would exceed your account's rate limit. Please try again later."});
        let named = json!({"error": "rate_limit", "last_assistant_message": "API Error: Request rejected (429) · All credentials for model claude-opus-5-5 are cooling down (reset after 5m)"});
        let wait = |core: &mut Core, input: &Value| -> i64 {
            core.begin_turn("w2");
            core.hook("w2", "failed", input);
            let l = core.agents["w2"].live.limit.clone().expect("held");
            l.until - l.at
        };
        let waits: Vec<i64> = (0..3).map(|_| wait(&mut core, &none)).collect();
        assert_eq!(waits, vec![60, 120, 240], "three in a row");
        assert_eq!(wait(&mut core, &named), 300, "a reset named: that");
        let waits: Vec<i64> = (0..4).map(|_| wait(&mut core, &none)).collect();
        assert_eq!(waits, vec![480, 960, 1800, 1800], "on from the one before, to 30 minutes");
        assert_eq!(core.agents["w2"].live.limit_wait, Some(1800), "kept in its live state, which is journalled");
        // a turn that ends by its Stop: from 60 s again
        core.begin_turn("w2");
        core.hook("w2", "stop", &json!({}));
        assert_eq!(wait(&mut core, &none), 60, "after a turn ended by its Stop");
        assert_eq!(wait(&mut core, &none), 120);
        // another error: from 60 s again
        core.begin_turn("w2");
        core.hook("w2", "failed", &json!({"error": "invalid_request"}));
        assert_eq!(wait(&mut core, &none), 60, "after another error");
        assert_eq!(wait(&mut core, &none), 120);
        // a turn an esc stopped: from 60 s again
        core.begin_turn("w2");
        core.interrupted("w2");
        let after = wait(&mut core, &none);
        let again = wait(&mut core, &none);
        // an esc typed in its own terminal: coordd knows it by the transcript's cut only (follow.rs), never through
        // `interrupted`; from 60 s again all the same (proto-reader N-1397 p163)
        core.begin_turn("w2");
        core.transcript_read("w2", Vec::new(), Vec::new(), Vec::new(), &[], Some(0), &[], false);
        let typed = wait(&mut core, &none);
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!((after, again, typed), (60, 120, 60), "after an esc: his, then one typed in its terminal");
    }

    /// Codex at its usage limit names the reset as a time: the agent is held till then, each time the stop comes again,
    /// never given its turn again on the doubled wait before it.
    #[test]
    fn a_codex_usage_limit_holds_its_agent_till_the_time_it_names() {
        use chrono::TimeZone;
        let (coord, mut core) = coord_of("limit-codex");
        core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        let now = crate::accounts::mover::now_s();
        let reset = chrono::Local.timestamp_opt(now + 20 * 3600, 0).unwrap();
        let reset_s = reset.timestamp() - reset.timestamp() % 60;
        let words = format!("You\u{2019}ve hit your usage limit. Upgrade to Pro (https://chatgpt.com/explore/pro), visit https://chatgpt.com/codex/settings/usage to purchase more credits or try again at {}.", reset.format("%b %-dth, %Y %-I:%M %p"));
        for _ in 0..4 {
            core.begin_turn("w2");
            core.hook("w2", "failed", &json!({"error": "rate_limit", "last_assistant_message": words}));
            let l = core.agents["w2"].live.limit.clone().expect("held");
            assert_eq!(l.until, reset_s, "till the time named: {l:?}");
            assert!(!core.resume_due(&core.agents["w2"]), "not given its turn again before the reset");
        }
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// His Esc on an agent held on a rate limit, out of a turn (proto-lead N-1397 p189): the turn it is held for is not
    /// given again. Pressed before the reset, at which it would be given its turn again: its state says the rate limit
    /// before and not after (consult-3 N-1397 p197), nothing is handed, its limit is gone and its view carries no hold,
    /// so his row reads "idle since …" (proto-lead N-1468 p15), the wait is back at key-again-after, and the reconcile
    /// log says who stopped it.
    #[test]
    fn his_esc_on_an_agent_held_on_a_rate_limit_cancels_its_resume() {
        let (coord, mut core) = coord_of("limit-esc");
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        let none = json!({"error": "rate_limit", "last_assistant_message": "API Error: Request rejected (429) · This request would exceed your account's rate limit. Please try again later."});
        for _ in 0..2 {
            core.hook("w2", "prompt", &json!({}));
            core.hook("w2", "failed", &none);
            rt.block_on(core.settled("w2", Vec::new()));
        }
        assert_eq!(core.agents["w2"].live.limit_wait, Some(120), "held twice, its wait doubled");
        // past its reset it would be given its turn again; his Esc comes before it
        let now = crate::accounts::mover::now_s();
        core.agents.get_mut("w2").unwrap().live.limit.as_mut().unwrap().until = now - 1;
        assert_eq!(core.to_hand_over(), vec!["w2".to_string()], "given again at its reset, but for his Esc");
        core.agents.get_mut("w2").unwrap().live.limit.as_mut().unwrap().until = now + 600;
        let state = core.agent_state("w2");
        assert!(format!("{state:?}").contains("rate limit"), "{state:?}");
        assert_eq!(core.view("w2").limit_until, Some(now + 600), "its row holds");
        let r = core.ticket_esc(&Who::Owner, vec!["w2".into()]);
        assert!(r.ok && r.out.contains("will not be given"), "{}", r.out);
        let live = &core.agents["w2"].live;
        assert_eq!((live.limit.as_ref(), live.limit_wait), (None, None), "the resume gone, the wait from key-again-after");
        assert!(core.to_hand_over().is_empty(), "nothing handed, nor at its reset");
        assert_eq!((core.view("w2").limit_until, core.view("w2").limit_why), (None, None), "its row: no hold");
        let state = core.agent_state("w2");
        assert!(!format!("{state:?}").contains("rate limit"), "{state:?}");
        let log = std::fs::read_to_string(core.path("reconcile.jsonl")).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&coord);
        assert!(log.contains("stopped by owner's esc: that turn is not given again"), "{log}");
    }

    /// His post to an agent held on a rate limit, past its reset under his Hold (proto-lead N-1397 p189): his Hold keeps
    /// back the resume as a turn of its own, not that the last turn was cut. The hand-over his post starts carries
    /// "Your last turn stopped at …" with it, once: the limit is spent there, and the next poll hands nothing.
    #[test]
    fn his_post_to_an_agent_held_on_a_rate_limit_carries_its_resume_under_his_hold() {
        let (coord, mut core) = coord_of("limit-his");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        let channel = |rx: &mut tokio::sync::mpsc::UnboundedReceiver<Job>| std::iter::from_fn(|| rx.try_recv().ok()).find_map(|j| match j {
            Job::Channel { name, ids, line, tickets, .. } if name == "w2" => Some((ids, line, tickets.len())),
            _ => None,
        });
        let row = "API Error: Request rejected (429) · All credentials for model claude-opus-5-5 are cooling down (reset after 18m48s)";
        core.hook("w2", "prompt", &json!({}));
        core.hook("w2", "failed", &json!({"hook_event_name": "StopFailure", "error": "rate_limit", "last_assistant_message": row}));
        rt.block_on(core.settled("w2", Vec::new()));
        // the reset passed, his Hold on: the resume waits
        core.agents.get_mut("w2").unwrap().live.limit.as_mut().unwrap().until = crate::accounts::mover::now_s() - 1;
        assert!(core.ticket_mail_hold(&Who::Owner, true).ok);
        assert!(core.to_hand_over().is_empty(), "his Hold keeps the resume as a turn of its own");
        // his post: one hand-over with both
        core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("go on when you can".into())), names: vec!["w2".into()], told: vec![] }).unwrap();
        assert_eq!(core.to_hand_over(), vec!["w2".to_string()]);
        core.hand_over("w2");
        let (ids, line, n) = channel(&mut rx).expect("handed");
        assert!(line.contains("Your last turn stopped at") && line.contains("(reset after 18m48s)"), "{line}");
        assert_eq!(n, 1, "his post with it");
        core.handed("w2", &ids, true, "channel");
        assert!(core.agents["w2"].live.limit.is_none(), "the resume spent");
        // nothing more: not at the next poll, nor once he lets go
        assert!(core.to_hand_over().is_empty());
        core.hand_over("w2");
        assert!(channel(&mut rx).is_none(), "given once");
        assert!(core.ticket_mail_hold(&Who::Owner, false).ok);
        let after = core.to_hand_over();
        let _ = std::fs::remove_dir_all(&coord);
        assert!(after.is_empty(), "{after:?}");
    }

    /// Words typed into the agent's terminal (paper-research N-101 p1107, p1127): a post by whoever sent the keys
    /// (coord-pty's `NAME.keys`), here the owner, on a new thread, marked typed, given in that turn and never handed
    /// again, and the turn's last message posted there as the reply; the next typed words go on the same thread; the
    /// launch prompt is no typed words, and words nobody can be named for are not posted. Typed by an agent that
    /// attached, into a turn already running: its post, on its own thread, not given (Claude Code queues it into that
    /// turn) and not handed later, and the turn's reply goes where its delivery puts it.
    #[test]
    fn words_typed_in_the_terminal_are_the_owners_post_and_the_turn_replies_to_them() {
        let (coord, mut core) = coord_of("typed");
        let transcript = coord.join("w2.jsonl");
        let line = |v: Value| format!("{v}\n");
        let ts = |s: u32| format!("2026-11-16T10:00:{s:02}.000Z");
        let ms = |s: u32| crate::transcript::epoch_ms(&ts(s)).unwrap();
        let user = |s: u32, text: &str| line(json!({"type": "user", "timestamp": ts(s), "message": {"role": "user", "content": text}}));
        let first = user(1, "Begin with the notes.");
        let unknown = user(3, "who typed this?");
        let asked = user(5, "is the build green?");
        let said = line(json!({"type": "assistant", "timestamp": ts(7), "message": {"role": "assistant", "content": [{"type": "text", "text": "Yes, B-4 is green."}], "stop_reason": "end_turn"}}));
        let again = user(9, "thanks; and B-5?");
        let lead = user(11, "and B-6, from lead");
        let said2 = line(json!({"type": "assistant", "timestamp": ts(13), "message": {"role": "assistant", "content": [{"type": "text", "text": "B-5 is red."}], "stop_reason": "end_turn"}}));
        let all = [&first, &unknown, &asked, &said, &again, &lead, &said2];
        std::fs::write(&transcript, all.iter().map(|s| s.as_str()).collect::<String>()).unwrap();
        let file = transcript.display().to_string();
        let at = |i: usize| Loc { file: file.clone(), offset: all[..i].iter().map(|s| s.len()).sum::<usize>() as u64, len: all[i].len() as u64 };
        core.launched_with.insert("w2".into(), "Begin with the notes.\n".into());
        core.typed("w2", at(0));
        assert!(core.tickets.m.tickets.values().all(|x| x.kind != "thread"), "the launch prompt is not typed words");
        core.typed("w2", at(1));
        assert!(core.tickets.m.tickets.values().all(|x| x.kind != "thread"), "no keys note: nobody to name, not posted");
        // the keys: the owner's app (a cgroup outside coordd's), coordd's own (an esc), then lead's `coord attach`
        let keys = coord_types::pty::keys(&coord.display().to_string(), "w2");
        std::fs::create_dir_all(keys.parent().unwrap()).unwrap();
        let k = |s: u32, pid: u32, cgroup: String| format!("{}\n", json!({"ms": ms(s), "pid": pid, "cgroup": cgroup}));
        let lead_cg = format!("{}/agents/lead/main", crate::cgroup::root_rel());
        std::fs::write(&keys, format!("{}{}{}", k(2, std::process::id(), String::new()), k(5, 1, "/user.slice/app.slice/yaaihh.scope".into()), k(10, 2, lead_cg))).unwrap();
        core.typed("w2", at(1));
        assert!(core.tickets.m.tickets.values().all(|x| x.kind != "thread"), "the last keys before it were coordd's own");
        core.typed("w2", at(2));
        let x = core.tickets.m.tickets.values().find(|x| x.kind == "thread").expect("a thread").clone();
        assert_eq!((x.posts[0].by.as_str(), x.posts[0].typed.as_deref()), ("owner", Some("w2")));
        assert!(core.ticket_due("w2").is_empty(), "given in the turn it began, not handed again");
        core.ticket_turn_ended("w2", at(3));
        let x = core.tickets.m.tickets[&x.t].clone();
        assert_eq!((x.posts.len(), x.posts[1].by.as_str(), x.posts[1].auto), (2, "w2", true), "{:?}", x.posts);
        core.typed("w2", at(4));
        let threads = core.tickets.m.tickets.values().filter(|x| x.kind == "thread").count();
        let y = core.tickets.m.tickets[&x.t].clone();
        // lead's words, typed while that turn runs: lead's post on a thread of its own, not given, not due
        core.typed("w2", at(5));
        let l = core.tickets.m.tickets.values().find(|z| z.kind == "thread" && z.t != x.t).expect("lead's thread").clone();
        assert_eq!((l.posts.len(), l.posts[0].by.as_str(), l.posts[0].typed.as_deref()), (1, "lead", Some("w2")), "{:?}", l.posts);
        assert!(l.given.iter().all(|g| g.to != "w2"), "typed into a running turn: not given");
        assert!(core.ticket_due("w2").is_empty(), "read as it was typed: not handed later");
        core.ticket_turn_ended("w2", at(6));
        let (y2, l2) = (core.tickets.m.tickets[&x.t].clone(), core.tickets.m.tickets[&l.t].clone());
        assert_eq!((y2.posts.len(), y2.posts[3].by.as_str(), l2.posts.len()), (4, "w2", 1), "the reply goes where the turn's delivery puts it");
        // after a restart, from the record: the same
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let mut back = Core::load(&coord.display().to_string(), &base);
        back.load_tickets();
        let z = back.tickets.m.tickets[&x.t].clone();
        let lz = back.tickets.m.tickets[&l.t].clone();
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!((threads, y.posts.len(), y.posts[2].typed.as_deref()), (1, 3, Some("w2")));
        assert_eq!(z.posts.iter().map(|p| p.typed.clone()).collect::<Vec<_>>(), vec![Some("w2".to_string()), None, Some("w2".to_string()), None]);
        assert_eq!(lz.posts[0].typed.as_deref(), Some("w2"));
    }

    /// A process killed at a box's memory limit (paper-research N-101 p1785): told once per rise of the group's
    /// `oom_kill`, on the ticket of the step the agent owes, with the project's hint; no rise read at the start, none
    /// for a count that stayed, and the fact alone with no hint set.
    #[test]
    fn a_process_killed_at_the_box_memory_limit_is_told_to_its_agent_once() {
        let (coord, mut core) = coord_of("oom");
        let project = |hint: &str| -> crate::project::Project { toml::from_str(&format!("[project]\nname = \"x\"\nbox = true\nbox_memory = \"10G\"\n{hint}")).unwrap() };
        core.project = Some(project("box_memory_hint = \"a build of that size runs as a step of the runner (`coord check`)\""));
        // a step w2 owes, as a minder's is
        let t = core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "lead".into(), form: json!({"to": "lead", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "lead"}, "follows": null}), text: Some(Text::New("the build".into())), names: vec!["w2".into()], told: vec![] }).map(|_| *core.tickets.m.tickets.keys().next_back().unwrap()).unwrap();
        let posts = |core: &Core| core.tickets.m.tickets[&t].posts.iter().filter(|p| p.by == "coord").count();
        let on_t = |core: &Core| core.tickets.m.tickets[&t].posts.iter().rev().find(|p| p.by == "coord").and_then(|p| if let Text::New(s) = &p.text { Some(s.clone()) } else { None });
        let counts = |n: u64| BTreeMap::from([("w2".to_string(), n), ("lead".to_string(), 0)]);
        // read at the start: 2 already, none told
        core.oom_seen = counts(2);
        core.oom_round(counts(2));
        let threads = |core: &Core| core.tickets.m.tickets.values().filter(|x| x.opener.as_deref() == Some("coord")).count();
        assert_eq!((posts(&core), threads(&core)), (0, 0), "nothing for what rose before the start");
        // a rise: one notice
        core.oom_round(counts(3));
        let told = if posts(&core) == 1 { on_t(&core) } else { None };
        let notice = core.tickets.m.tickets.values().filter(|x| x.opener.as_deref() == Some("coord")).filter_map(|x| x.posts.first()).filter_map(|p| if let Text::New(s) = &p.text { Some(s.clone()) } else { None }).next();
        let text = told.or(notice).expect("told once");
        assert_eq!(text, "A process of yours was killed at your box's memory limit (10G); a build of that size runs as a step of the runner (`coord check`).");
        // the same count again: nothing more
        let before = (posts(&core), threads(&core));
        core.oom_round(counts(3));
        assert_eq!((posts(&core), threads(&core)), before, "one notice per rise");
        // no hint set: the fact alone
        core.project = Some(project(""));
        core.oom_round(counts(4));
        let last = core.tickets.m.tickets.values().flat_map(|x| x.posts.iter()).filter(|p| p.by == "coord").filter_map(|p| if let Text::New(s) = &p.text { Some(s.clone()) } else { None }).last();
        assert_eq!(last.as_deref(), Some("A process of yours was killed at your box's memory limit (10G)."));
        // on the ticket of a step it owes, the notice is coord's post there, told to it
        let out = core.ticket_input(Input::Post { t, by: "coord".into(), text: Text::New("A process of yours was killed …".into()), names: vec!["w2".into()], told: vec!["w2".into()], inform: vec![], answers: None, kind: None });
        assert!(out.is_ok(), "{out:?}");
        assert!(core.tickets.m.due("w2").iter().any(|d| d.t == t), "w2 is given it");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// The clocks (0.2 line 26): an agent's own program in `kinds/agents/NAME.kind` is read with the kinds, and its clock
    /// is in the snapshot with whose it is; a firing reaches the window as the ticket it opens, naming its binding, and
    /// sends no snapshot; a reload that drops the clock sends one at the next round, without it.
    #[test]
    fn a_clock_is_in_the_snapshot_with_whose_it_is_and_a_reload_that_changes_it_sends_one() {
        let (coord, mut core) = coord_of("clocks");
        let dir = coord.join("project");
        std::fs::create_dir_all(dir.join("kinds/agents")).unwrap();
        let clock = "every 1h open question(\"hourly\")";
        std::fs::write(dir.join("kinds/agents/lead.kind"), format!("kind lookout\n  letter W\n  form why: Text\n  result Text\n  pattern\n    return why\n\n{clock}\n")).unwrap();
        // a bad file there is refused with its file and line, as a bad kind file is; the others still load
        std::fs::write(dir.join("kinds/agents/bad.kind"), "every 1h open nosuch(\"x\")\n").unwrap();
        let mut p: crate::project::Project = toml::from_str("[project]\nname = \"x\"\n").unwrap();
        p.dir = dir.display().to_string();
        core.project = Some(p);
        core.load_tickets();
        let bad = core.tickets.errors.iter().filter(|e| e.contains("kinds/agents/bad.kind:1: binding refused")).count();
        assert_eq!(bad, 1, "{:#?}", core.tickets.errors);
        let Body::Snapshot(s) = core.snapshot().body else { panic!() };
        assert_eq!(s.clocks.len(), 1, "{:?} {:?}", s.clocks, s.refusals);
        let c = &s.clocks[0];
        assert_eq!((c.binding.as_str(), c.kind.as_str(), c.every, c.by.as_deref(), c.last.clone()), (clock, "question", Some(3600), Some("lead"), None));
        assert!(c.file.ends_with("kinds/agents/lead.kind"), "{}", c.file);
        // the Kinds answer: each kind's file, whose own it is, the bindings that open it as written
        let r = core.ticket_kinds();
        assert!(r.ok, "{}", r.out);
        let kinds: Vec<serde_json::Value> = serde_json::from_str(&r.out).unwrap();
        let of = |n: &str| kinds.iter().find(|k| k["kind"] == n).cloned().unwrap_or_else(|| panic!("no {n}: {kinds:?}"));
        let (q, own) = (of("question"), of("lookout"));
        assert_eq!((q["by"].clone(), q["bindings"].clone()), (json!(null), json!([clock])));
        assert!(q["file"].as_str().is_some_and(|f| f.ends_with("question.kind")), "{q}");
        assert_eq!((own["by"].clone(), own["bindings"].clone()), (json!("lead"), json!([])));
        assert!(own["file"].as_str().is_some_and(|f| f.ends_with("kinds/agents/lead.kind")), "{own}");
        // it fires: its ticket names it; the round sends no snapshot for that
        let seq = core.seq;
        core.ticket_input(Input::Event { name: "every".into(), value: json!(clock) }).unwrap();
        let opened = core.tickets.m.tickets.values().filter(|x| x.binding.as_deref() == Some(clock)).map(|x| x.t).collect::<Vec<_>>();
        assert_eq!(opened.len(), 1);
        assert_eq!(core.tickets.m.view(opened[0]).unwrap().binding.as_deref(), Some(clock));
        assert!(core.tickets.m.clock_views()[0].last.is_some());
        core.parts_round();
        let snaps = |core: &Core, seq| core.since(seq).unwrap().iter().filter_map(|e| if let Body::Snapshot(s) = &e.body { Some(s.clone()) } else { None }).collect::<Vec<_>>();
        assert!(snaps(&core, seq).is_empty(), "a firing sends no snapshot");
        // the clock is taken out and the kinds read again: the next round sends a snapshot without it
        std::fs::write(dir.join("kinds/agents/lead.kind"), "").unwrap();
        core.load_tickets();
        let seq = core.seq;
        core.parts_round();
        let sent = snaps(&core, seq);
        assert_eq!(sent.len(), 1);
        assert!(sent[0].clocks.is_empty(), "{:?}", sent[0].clocks);
        assert!(serde_json::to_value(&sent[0]).unwrap().get("clocks").is_none(), "no clocks: not sent");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// The sidebar's parts (N-412 ask 2, N-427): the project policy's `part` and `part-place` on the agents' views and in
    /// the snapshot; an agent without one has none; the policy edited, the next round sends a new snapshot, and a round
    /// with nothing changed sends none.
    #[test]
    fn the_policys_parts_are_on_the_views_and_in_the_snapshot() {
        let (coord, mut core) = coord_of("parts");
        let dir = coord.join("project");
        std::fs::create_dir_all(dir.join("policy")).unwrap();
        let decl = "Fact part Identified by agent * String\nFact part-place Identified by String * Int\n";
        std::fs::write(dir.join("policy/parts.eflint"), format!("{decl}+part(\"w2\", \"yaaihh project/Engine\").\n+part-place(\"Papers\", 2).\n+part-place(\"yaaihh project\", 1).\n")).unwrap();
        let mut p: crate::project::Project = toml::from_str("[project]\nname = \"x\"\n").unwrap();
        p.dir = dir.display().to_string();
        core.project = Some(p);
        assert_eq!(core.view("w2").part, "yaaihh project/Engine");
        assert_eq!(core.view("lead").part, "", "no part fact: none");
        assert!(serde_json::to_value(core.view("lead")).unwrap().get("part").is_none(), "and not sent");
        let Body::Snapshot(s) = core.snapshot().body else { panic!() };
        assert_eq!(s.parts, vec![("yaaihh project".to_string(), 1), ("Papers".to_string(), 2)]);
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["parts"], json!([["yaaihh project", 1], ["Papers", 2]]), "the app reads [[name, place], ...]");
        // nothing changed: no snapshot
        let seq = core.seq;
        core.parts_round();
        assert_eq!(core.seq, seq, "nothing sent");
        // the owner moves w2 and adds lead: read again (here at once: an agent's change moves norm_gen; a file's edit is
        // seen within 2 s), and the round sends a snapshot that carries them
        std::fs::write(dir.join("policy/parts.eflint"), format!("{decl}+part(\"w2\", \"Papers\").\n+part(\"lead\", \"Papers\").\n+part-place(\"Papers\", 1).\n")).unwrap();
        core.norm_gen += 1;
        core.parts_round();
        let sent = core.since(seq).unwrap();
        let snaps: Vec<_> = sent.iter().filter_map(|e| if let Body::Snapshot(s) = &e.body { Some(s) } else { None }).collect();
        assert_eq!(snaps.len(), 1, "{sent:?}");
        assert_eq!(snaps[0].parts, vec![("Papers".to_string(), 1)]);
        assert_eq!(snaps[0].agents.iter().map(|a| (a.name.as_str(), a.part.as_str())).collect::<Vec<_>>(), vec![("lead", "Papers"), ("w2", "Papers")]);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// 0.2 B10 (consult-3, N-1402.p2): the owner's `@team` and `@PART` in a post on a thread name each running agent of
    /// the group, by the window's rule: `coord_types::groups` given the parts and views the snapshot carries resolves
    /// every tag as coordd does.
    #[test]
    fn the_owners_group_tag_in_a_post_names_each_running_agent_of_it() {
        let (coord, mut core) = coord_of("groups");
        let dir = coord.join("project");
        std::fs::create_dir_all(dir.join("policy")).unwrap();
        let decl = "Fact part Identified by agent * String\nFact part-place Identified by String * Int\n";
        std::fs::write(dir.join("policy/parts.eflint"), format!("{decl}+part(\"w2\", \"yaaihh project/Engine\").\n+part(\"lead\", \"Papers\").\n+part-place(\"Papers\", 2).\n+part-place(\"yaaihh project/Engine\", 1).\n")).unwrap();
        let mut p: crate::project::Project = toml::from_str("[project]\nname = \"x\"\n").unwrap();
        p.dir = dir.display().to_string();
        core.project = Some(p);
        core.agents.get_mut("w2").unwrap().live.state = "idle".into();
        core.agents.get_mut("lead").unwrap().live.state = "stopped".into();
        // one rule: what coordd resolves is what the window resolves from the snapshot's parts and views
        let Body::Snapshot(snap) = core.snapshot().body else { panic!() };
        let members: Vec<coord_types::groups::Member> = snap.agents.iter().map(|a| coord_types::groups::Member { name: &a.name, part: &a.part, running: a.state != "stopped" }).collect();
        for text in ["@team", "@Engine now", "@yaaihh-project", "@papers", "@w2 and @Engine", "@nobody", "no tag"] {
            let window: Vec<String> = coord_types::groups::reached(text, &snap.parts, &members).into_iter().map(String::from).collect();
            assert_eq!(core.owner_groups(text), window, "{text}");
        }
        assert_eq!(core.owner_groups("@team"), ["w2"], "lead is stopped");
        assert_eq!(core.owner_groups("@yaaihh-project"), ["w2"], "a top part takes its nested parts' agents");
        assert!(core.owner_groups("@Papers").is_empty());
        // his post on a thread names the group's running agents
        // his words to the lead, naming it: a thread, open while it waits on the lead
        core.ticket_input(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": "lead", "typed": ["lead"], "names": ["lead"], "text": "the engine", "addressed": {"head": "Named"}}) }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        let id = core.tickets.m.id(t);
        assert!(core.tickets.m.tickets[&t].is_open(), "{id} waits on the lead");
        let r = core.ticket_post(&Who::Owner, &id, "@Engine have a look".into(), None, None, crate::ticket::Said::default(), None, false);
        assert!(r.ok, "{}", r.out);
        let post = core.tickets.m.tickets[&t].posts.last().unwrap();
        assert_eq!(post.text, Text::New("@Engine have a look".into()), "his post is on the thread");
        assert!(post.names.contains(&"w2".to_string()), "{:?}", post.names);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// An agent's view says the ticket it is on (docket-edge6 N-447 p48), the one its label names: by id; none while
    /// it is on none, and then not sent; the highest of those it was named in; one it holds a step in before all.
    #[test]
    fn an_agents_view_says_the_ticket_it_is_on() {
        let (coord, mut core) = coord_of("agent-on");
        assert_eq!(core.view("w2").on, None);
        assert!(serde_json::to_value(core.view("w2")).unwrap().get("on").is_none(), "none: not sent");
        let open = |core: &mut Core, by: &str, to: &str| -> u64 {
            let t = core.tickets.m.next_number();
            core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: by.into(), form: json!({"to": to, "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": by}, "follows": null}), text: Some(Text::New("the parser".into())), names: vec![to.into()], told: vec![] }).unwrap();
            t
        };
        let t = open(&mut core, "owner", "w2");
        let id = core.tickets.m.id(t);
        assert_eq!(core.view("w2").on, Some(id.clone()), "{}", docket_machine::label::agent(&core.tickets.m, "w2", &crate::label::read));
        assert_eq!(serde_json::to_value(core.view("w2")).unwrap()["on"], json!(id));
        let label = docket_machine::label::agent(&core.tickets.m, "w2", &crate::label::read);
        match docket_machine::label::named(&core.tickets.m, t, &crate::label::read) {
            Some(n) => assert!(label.ends_with(&format!(" of {n}")), "the label names the same ticket: {label}"),
            None => assert!(!label.contains(" of "), "{label}"),
        }
        let u = open(&mut core, "lead", "w2");
        assert_eq!(core.view("w2").on, Some(core.tickets.m.id(u)), "the highest");
        assert_eq!(core.view("lead").on, None, "named in none");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// Mentions (threads piece 3): a post naming another thread, one of its posts and an agent puts the ids on its own
    /// view and a line back on the named thread's, which is sent again at once; its own thread and a post that is not
    /// there are not lines back; nothing is recorded; the index read at start is the one kept as posts land.
    #[test]
    fn a_posts_ids_are_its_mentions_and_the_named_get_a_line_back() {
        let (coord, mut core) = coord_of("mentions");
        let open = |core: &mut Core, by: &str, text: &str| -> u64 {
            let t = core.tickets.m.next_number();
            core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: by.into(), form: json!({"to": "w2", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": by}, "follows": null}), text: Some(Text::New(text.into())), names: vec!["w2".into()], told: vec![] }).unwrap();
            t
        };
        let a = open(&mut core, "lead", "the parser");
        let ia = core.tickets.m.id(a);
        let record = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap_or_default();
        let seq = core.seq;
        let b = open(&mut core, "w3", &format!("about {ia} and {ia}.p1 and {ia}.p9, ask @w2 and @nobody"));
        let ib = core.tickets.m.id(b);
        core.ticket_input(Input::Post { t: b, by: "w3".into(), text: Text::New(format!("{ib}.p1 again, and {ia}")), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        // its own view: the ids its posts hold, as written; @nobody is no agent, its own ticket is not named
        let vb = core.ticket_shown(b).unwrap();
        assert_eq!(vb.mentions, BTreeMap::from([(1, vec![ia.clone(), format!("{ia}.p1"), "w2".to_string()]), (2, vec![ia.clone()])]));
        assert!(vb.mentioned.is_empty(), "naming itself is no line back: {:?}", vb.mentioned);
        // the named thread: one line per naming post and named post, oldest first; p9 is not there, so it is the ticket
        let va = core.ticket_shown(a).unwrap();
        let lines: Vec<(String, u32, Option<u32>, String)> = va.mentioned.iter().map(|l| (l.by.clone(), l.post, l.of, l.who.clone())).collect();
        assert_eq!(lines, vec![(ib.clone(), 1, None, "w3".into()), (ib.clone(), 1, Some(1), "w3".into()), (ib.clone(), 2, None, "w3".into())]);
        assert_eq!(va.mentioned[0].label, docket_machine::label::ticket(&core.tickets.m, b, &crate::label::read));
        assert!(serde_json::to_value(&va).unwrap()["mentioned"][0].get("of").is_none(), "no post named: no `of`");
        // as each naming post landed, the named thread got its new lines back, and not its view again (N-427 p11); put
        // together they are the view's
        let sent = core.since(seq).unwrap();
        assert!(!sent.iter().any(|e| matches!(&e.body, Body::Ticket(v) | Body::TicketChange { view: v, .. } if v.id == ia)), "{sent:?}");
        let got: Vec<docket_machine::ViewMention> = sent.iter().filter_map(|e| if let Body::Mentioned { id, lines } = &e.body { assert_eq!(*id, ia); Some(lines.clone()) } else { None }).flatten().collect();
        assert_eq!(got, va.mentioned, "the lines sent, in order, are the view's");
        assert_eq!(sent.iter().filter(|e| matches!(e.body, Body::Mentioned { .. })).count(), 2, "one per landing post");
        // the request: a ticket's, a post's, an agent's (with or without @), and nothing for what is not there
        let ask = |core: &Core, id: &str| core.ticket_mentions(id).out.lines().map(|l| serde_json::from_str::<docket_machine::ViewMention>(l).unwrap()).map(|l| (l.by, l.post, l.of)).collect::<Vec<_>>();
        assert_eq!(ask(&core, &ia).len(), 3);
        assert_eq!(ask(&core, &format!("{ia}.p1")), vec![(ib.clone(), 1, Some(1))]);
        assert_eq!(ask(&core, "@w2"), vec![(ib.clone(), 1, None)]);
        assert_eq!(ask(&core, "w2"), ask(&core, "@w2"));
        assert!(ask(&core, "nobody").is_empty() && ask(&core, &format!("{ia}.p9")).is_empty());
        assert!(!core.ticket_mentions(" ").ok);
        // nothing recorded beyond the posts themselves
        let now = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap_or_default();
        assert!(now.starts_with(&record) && !now[record.len()..].contains("mention"), "{}", &now[record.len()..]);
        // read at start: the same
        let mut fresh = crate::ticket::mentions::Mentions::default();
        fresh.read_all(&core.tickets.m, &crate::label::read);
        let agent = |n: &str| core.agents.contains_key(n);
        let label = |u: u64| docket_machine::label::ticket(&core.tickets.m, u, &crate::label::read);
        assert_eq!(fresh.lines(&core.tickets.m, &agent, &label, &ia), va.mentioned);
        assert_eq!(fresh.held(&core.tickets.m, &agent, b), vb.mentions);
        // the record graph's `named` edges (paper-research N-1446 p1337): a post → what it names that is there, at its
        // node's line; a post the graph does not have (b's p2 here) names nothing in it
        let mut g = docket_machine::graph::Graph::default();
        g.node(format!("{ib}.p1"), docket_machine::graph::NodeKind::Post, 7);
        fresh.named_into(&core.tickets.m, &agent, &mut g);
        let named: Vec<(String, String, docket_machine::graph::EdgeKind, usize)> = g.edges.iter().map(|e| (e.from.clone(), e.to.clone(), e.kind, e.line().unwrap())).collect();
        let p1 = format!("{ib}.p1");
        let k = docket_machine::graph::EdgeKind::Named;
        assert_eq!(named, vec![(p1.clone(), ia.clone(), k, 7), (p1.clone(), format!("{ia}.p1"), k, 7), (p1, "agent:w2".to_string(), k, 7)]);
        // the commits' `named` edges (paper-research N-1446 p1364, p1370): a commit an agent here wrote → what its
        // `Item:` and its subject name that is there, read as a post's words are (his post in words too), at the
        // commit's time; one by no agent here (w3 is in no registry here), or not in the graph, names nothing in it
        let commit = |hash: &str, agent: &str, subject: String, items: Vec<String>| docket_machine::graph_git::Commit { hash: hash.into(), at: "2026-11-22T08:41:33Z".into(), subject, agent: agent.into(), items, ..Default::default() };
        let commits = [
            commit("c1c1c1c1aa", "lead", format!("fix the parser (owner {ib} p1); see {ib}.p9, @w2 and Q-404"), vec![ia.clone()]),
            commit("c2c2c2c2aa", "w3", format!("about {ia}"), vec![format!("{ia}.p1")]),
            commit("c3c3c3c3aa", "lead", format!("about {ia}"), vec![]),
        ];
        let mut g = docket_machine::graph::Graph::default();
        docket_machine::graph_git::add(&mut g, &commits[..2], &[]);
        crate::ticket::mentions::commits_into(&core.tickets.m, &agent, &commits, &mut g);
        let named: Vec<(String, String, docket_machine::graph::Source, Option<String>)> = g.edges.iter().filter(|e| e.kind == k).map(|e| (e.from.clone(), e.to.clone(), e.source.clone(), e.when.clone())).collect();
        let c1 = |to: &str| ("commit:c1c1c1c1".to_string(), to.to_string(), docket_machine::graph::Source::Commit("c1c1c1c1".into()), Some("2026-11-22T08:41:33Z".to_string()));
        assert_eq!(named, vec![c1(&ia), c1(&format!("{ib}.p1")), c1(&ib), c1("agent:w2")]);
        assert_eq!([&ia, &format!("{ib}.p1")].map(|id| g.get(id).map(|n| n.kind)), [Some(docket_machine::graph::NodeKind::Ticket), Some(docket_machine::graph::NodeKind::Post)]);
        // w3 takes its post 2 back before it is handed: its ids leave b's view and its line back leaves a's, and a's
        // view goes out again so the window drops the line (docket-core3, N-458 p10)
        let seq = core.seq;
        let r = core.ticket_take_back(&Who::Agent("w3".into()), &ib, 2);
        assert!(r.ok, "{}", r.out);
        assert_eq!(core.ticket_shown(b).unwrap().mentions, BTreeMap::from([(1, vec![ia.clone(), format!("{ia}.p1"), "w2".to_string()])]));
        let va = core.ticket_shown(a).unwrap();
        assert_eq!(va.mentioned.iter().map(|l| (l.by.clone(), l.post)).collect::<Vec<_>>(), vec![(ib.clone(), 1), (ib.clone(), 1)]);
        let sent = core.since(seq).unwrap();
        assert!(sent.iter().any(|e| matches!(&e.body, Body::Ticket(v) | Body::TicketChange { view: v, .. } if v.id == ia && v.mentioned == va.mentioned)), "{sent:?}");
        assert_eq!(ask(&core, &ia).len(), 2);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// What the ring and `Sent` hold, in the minute's costs line (paper-research N-1446 p2299): the ring's events and
    /// their bytes as JSON as a recount of the ring gives them, after a whole-state event replaced, after a snapshot,
    /// with a big ticket change in (its sizing's time said: p2301) and past RING; and the line read back says them,
    /// with the minute's sizing and Sent's count.
    #[test]
    fn the_minutes_line_says_what_the_ring_and_sent_hold() {
        let (coord, mut core) = coord_of("ringline");
        let recount = |core: &Core| {
            let held: Vec<&Event> = core.ring.iter().filter_map(|(_, e)| e.as_ref()).collect();
            (held.len(), held.iter().map(|e| serde_json::to_vec(e).unwrap().len() as u64).sum::<u64>())
        };
        // a whole-state event (refusals) a newer one replaces leaves the ring; a delivery stays
        let big = "x".repeat(300_000);
        for i in 0..4 {
            core.emit(Body::Refusals(vec![format!("{big}{i}")]));
            core.emit(Body::Delivered { id: format!("N-1.p{i}"), to: "w2".into(), how: big.clone() });
        }
        assert_eq!((core.held_events, core.ring_bytes), recount(&core), "after the replaced ones");
        assert_eq!(core.held_events, 5, "the last refusals and the four deliveries");
        // a snapshot clears what was before it
        let snap = core.snapshot();
        core.emit(snap.body);
        assert_eq!((core.held_events, core.ring_bytes), recount(&core), "after a snapshot");
        assert_eq!(core.held_events, 1);
        // a big ticket change: a thread of 100 posts of about 10 KB, sent whole as one change
        let form = json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null});
        core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form, text: Some(Text::New("big".into())), names: vec!["w2".into()], told: vec![] }).unwrap();
        let t = *core.tickets.m.tickets.keys().next_back().unwrap();
        for i in 0..100 {
            let text = format!("post {i}: {}", "a line with \"quotes\", `code` and words ä\n".repeat(260));
            core.ticket_input(Input::Post { t, by: "w2".into(), text: Text::New(text), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        }
        let view = core.ticket_shown(t).unwrap();
        let posts = view.posts.len() as u32;
        let change = Event { seq: core.seq + 1, body: Body::TicketChange { view, posts } };
        let sizing = std::time::Instant::now();
        let len = json_len(&change);
        println!("sizing a ticket change of {posts} posts, {} KB as JSON: {:?}", len / 1000, sizing.elapsed());
        assert!(len >= 1_000_000, "a big one: {len}");
        core.emit(change.body);
        assert_eq!((core.held_events, core.ring_bytes), recount(&core), "with a big ticket change");
        // past RING the oldest go
        for i in 0..RING + 3 {
            core.emit(Body::Delivered { id: format!("N-2.p{i}"), to: "w2".into(), how: big[..60].into() });
        }
        assert_eq!((core.held_events, core.ring_bytes), recount(&core), "past RING");
        assert_eq!(core.held_events, RING);
        // three requests read, one of them read again
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        let ended = |id: &str| transcript::Ended { id: id.into(), at: now, model: "m".into(), tokens: Default::default(), cache: None, said: None };
        core.add_sent("w2", vec![ended("a"), ended("b")]);
        core.add_sent("w2", vec![ended("b"), ended("c")]);
        // the line, read back
        let line = crate::counts::kept_line(core.held_events, core.ring_bytes, 7, Duration::from_micros(1234), core.sent.len());
        let word = |w: &str| line.split(w).nth(1).and_then(|r| r.split_whitespace().next()).unwrap_or_else(|| panic!("{w:?} in {line}"));
        let after = |w: &str| -> u64 { word(w).parse().unwrap_or_else(|_| panic!("{w:?} in {line}")) };
        assert_eq!((after("watchers' ring holds "), after(" events, "), after(" ms for "), after("Sent holds ")), (RING as u64, core.ring_bytes / 1_000_000, 7, 3), "{line}");
        assert_eq!(word("ring sizing "), "1.23", "{line}");
        assert!(core.ring_bytes >= 1_000_000, "a whole MB to read back: {}", core.ring_bytes);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A page of an agent's session is read off the core's lock (paper-research N-1446 p2313): while the page waits on
    /// its session (a FIFO, whose opening waits for a writer), the lock is free for the rest; the session opened, the
    /// page comes, of that session.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_transcript_page_is_read_off_the_lock() {
        let (coord, mut core) = coord_of("page-off-lock");
        let fifo = coord.join("w2-session.jsonl");
        let at = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        // SAFETY: AT is a path, NUL-terminated
        assert_eq!(unsafe { libc::mkfifo(at.as_ptr(), 0o600) }, 0, "mkfifo {fifo:?}");
        core.agents.get_mut("w2").unwrap().transcript_file = Some(fifo.display().to_string());
        let shared = crate::Shared::new(core);
        let asked = {
            let shared = shared.clone();
            tokio::spawn(async move { crate::handle(&shared, &cgroup::Caller::Owner, 0, coord_types::Request::Transcript { name: "w2".into(), before: None, after: None, session: None }).await })
        };
        // the page is waiting on its session: the lock is free meanwhile
        tokio::time::sleep(Duration::from_millis(300)).await;
        let free = tokio::time::timeout(Duration::from_secs(2), shared.lock()).await.map(drop).is_ok();
        // the session opened for writing: the page goes on, and comes. Opened without waiting (it fails while no reader
        // has it open), tried for 5 s, so a page that never opened its session fails the leg instead of hanging it
        let writer = {
            use std::os::unix::fs::OpenOptionsExt;
            let open = || std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(&fifo);
            let mut tries = 0;
            loop {
                match open() {
                    Ok(w) => break Some(w),
                    Err(_) if tries < 100 => {
                        tries += 1;
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    Err(_) => break None,
                }
            }
        };
        let reading = writer.is_some();
        drop(writer);
        let r = tokio::time::timeout(Duration::from_secs(10), asked).await.expect("the page came").unwrap();
        let _ = std::fs::remove_dir_all(&coord);
        assert!(free, "the core's lock was held while the page waited on its session");
        assert!(reading, "the page never opened its session: {}", r.out);
        assert!(r.ok, "{}", r.out);
        let page: coord_types::Transcript = serde_json::from_str(&r.out).unwrap();
        assert_eq!((page.session, page.entries.len()), (fifo.display().to_string(), 0));
    }

    /// Another transcript of an agent's, by its path (`Request::Transcript`'s `session`; path fix 6 point 3,
    /// accounts-lead N-1446 p2651): the old line his system-prompt edit from a turn went off, read on from the cut. Each
    /// page comes with his edits that fall in its session (`Transcript::restarted`), the session each went off given as
    /// its transcript's path beside it. A path not beside the agent's current transcript is refused, as an image's is.
    /// Asked as a window's JSON, so that a coordd that reads the current session whatever is named fails it.
    #[tokio::test]
    async fn an_agents_old_line_is_read_by_its_path_beside_his_edit() {
        let (coord, mut core) = coord_of("old-line");
        let dir = coord.join("projects");
        std::fs::create_dir_all(&dir).unwrap();
        let line = |text: &str| format!("{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":\"{text}\"}}}}\n");
        let one = line("turn one");
        std::fs::write(dir.join("s1.jsonl"), format!("{one}{}", line("turn two on line one"))).unwrap();
        std::fs::write(dir.join("c0ff.jsonl"), format!("{one}{}", line("on line two"))).unwrap();
        std::fs::write(coord.join("elsewhere.jsonl"), line("not its")).unwrap();
        let (s1, c0ff) = (dir.join("s1.jsonl").display().to_string(), dir.join("c0ff.jsonl").display().to_string());
        core.agents.get_mut("w2").unwrap().transcript_file = Some(c0ff.clone());
        // roster's line for his edit from turn 1: a copy of s1 cut after it
        let cut = one.len();
        let more = json!({"phase": "system", "session": "c0ff", "from": "s1", "cut": cut.to_string(), "system": "2", "replace": ""});
        core.ticket_input(Input::Act { agent: "w2".into(), is: "began".into(), text: "system".into(), more: more.as_object().unwrap().clone(), t: None }).unwrap();
        let shared = crate::Shared::new(core);
        let owner = cgroup::Caller::Owner;
        let ask = |v: Value| crate::handle(&shared, &owner, 0, serde_json::from_value(v).unwrap());
        let words = |page: &Value| page["entries"].as_array().into_iter().flatten().filter_map(|e| e["text"].as_str().map(str::to_string)).collect::<Vec<_>>();
        // its current transcript, line 2; line 1 by its path, from the cut on; and a file not beside its transcript
        let current = ask(json!({"op": "transcript", "name": "w2"})).await;
        let old = ask(json!({"op": "transcript", "name": "w2", "session": s1, "after": cut})).await;
        let elsewhere = ask(json!({"op": "transcript", "name": "w2", "session": coord.join("elsewhere.jsonl").display().to_string()})).await;
        // an image's check is the same one
        let image = |session: &str| json!({"op": "image", "name": "w2", "session": session, "offset": 0, "n": 0});
        let (image_old, image_elsewhere) = (ask(image(&s1)).await, ask(image(&coord.join("elsewhere.jsonl").display().to_string())).await);
        let _ = std::fs::remove_dir_all(&coord);
        // line 2 with his edit at the cut, and the old line's path
        assert!(current.ok, "{}", current.out);
        let page: Value = serde_json::from_str(&current.out).unwrap();
        assert_eq!(page["session"], json!(c0ff));
        let marks = page["restarted"].as_array().cloned().unwrap_or_default();
        assert_eq!(marks.len(), 1, "his edit, in the session read: {}", current.out);
        assert_eq!((&marks[0]["offset"], &marks[0]["system"]), (&json!(cut), &json!("2")));
        assert_eq!(marks[0]["line"], json!({"k": 2, "from": s1, "from_line": 1, "turns": 0}), "line 2, off line 1, its path beside");
        // line 1's turn after the cut, as it happened
        assert!(old.ok, "{}", old.out);
        let page: Value = serde_json::from_str(&old.out).unwrap();
        assert_eq!((&page["session"], &page["start"]), (&json!(s1), &json!(cut)));
        assert_eq!(words(&page), ["turn two on line one"], "{}", old.out);
        assert!(page.get("restarted").is_none(), "line 1 never changes");
        assert!(!elsewhere.ok && elsewhere.out.contains("is not one of w2's sessions"), "{}", elsewhere.out);
        assert!(!image_old.ok && image_old.out.starts_with("coord image: no image 0 at byte 0"), "beside it, looked in: {}", image_old.out);
        assert!(!image_elsewhere.ok && image_elsewhere.out.contains("is not one of w2's sessions"), "{}", image_elsewhere.out);
    }

    /// A watch resumed after a reconnect (protocol 1, N-412 ask 13): in the same run, `Resumed` and then only what was
    /// missed, `Resumed` alone when nothing was; another run (a coordd started again numbers from 0 again), or a
    /// number older than the ring holds, gets none, and the watch starts with a snapshot, which names the run.
    #[test]
    fn a_watch_resumes_only_in_its_own_run() {
        let (coord, mut core) = coord_of("resume");
        let Body::Snapshot(snap) = core.snapshot().body else { panic!() };
        assert_eq!(snap.run.as_deref(), Some(core.run.as_str()));
        let run = core.run.clone();
        core.emit(Body::Refusals(vec!["one".into()]));
        let at = core.seq;
        core.emit(Body::Notices(Vec::new()));
        let bodies = |r: Option<Vec<Event>>| r.map(|v| v.into_iter().map(|e| crate::counts::body_kind(&e.body)).collect::<Vec<_>>());
        assert_eq!(bodies(core.resume(at, Some(&run))), Some(vec!["resumed", "notices"]));
        assert_eq!(bodies(core.resume(core.seq, Some(&run))), Some(vec!["resumed"]), "nothing missed");
        assert_eq!(bodies(core.resume(at, Some("2026-11-18T09:00:00Z-1"))), None, "another run");
        assert_eq!(bodies(core.resume(core.seq + 5, Some(&run))), None, "a number this run never reached");
        // a watcher from before protocol 1 names no run, and is served as before
        assert_eq!(bodies(core.resume(at, None)), Some(vec!["notices"]));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A ticket's change is sent as its new and changed posts with the count (paper-research N-101 p1165): whole the
    /// first time, then only what moved; the posts merged by number give the ticket whole.
    #[test]
    fn a_tickets_change_carries_only_its_new_and_changed_posts() {
        let (coord, mut core) = coord_of("change");
        let id_of = |b: &Body| match b {
            Body::Ticket(v) | Body::TicketChange { view: v, .. } => Some(v.id.clone()),
            _ => None,
        };
        // the ticket events a watching window was sent since the last call, for ticket ID (the ring keeps one a ticket,
        // `kept_change`)
        let mut rx = core.tx.subscribe();
        let mut since = |id: &str| -> Vec<Body> { std::iter::from_fn(|| rx.try_recv().ok()).map(|e| e.body).filter(|b| id_of(b).as_deref() == Some(id)).collect() };
        core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("one".into())), names: vec!["w2".into()], told: vec![] }).unwrap();
        let t = *core.tickets.m.tickets.keys().next_back().unwrap();
        let id = core.tickets.m.id(t);
        let first = since(&id);
        assert!(matches!(first.first(), Some(Body::Ticket(_))), "whole the first time: {}", first.len());
        // the reader: whole tickets taken, changes merged by number
        let mut held = core.ticket_shown(t).unwrap();
        for text in ["two", "three"] {
            core.ticket_input(Input::Post { t, by: "w2".into(), text: Text::New(text.into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        }
        core.ticket_input(Input::Typed { t, n: 2, agent: "w2".into() }).unwrap();
        let events = since(&id);
        let mut sent = Vec::new();
        for b in events {
            match b {
                Body::Ticket(v) => held = v,
                Body::TicketChange { view, posts } => {
                    sent.push((view.posts.iter().map(|p| p.n).collect::<Vec<_>>(), posts));
                    let mut posts_now = std::mem::take(&mut held.posts);
                    posts_now.retain(|p| !view.posts.iter().any(|q| q.n == p.n));
                    posts_now.extend(view.posts.clone());
                    posts_now.sort_by_key(|p| p.n);
                    held = coord_types::TicketView { posts: posts_now, ..view };
                }
                _ => {}
            }
        }
        let whole = core.ticket_shown(t).unwrap();
        let _ = std::fs::remove_dir_all(&coord);
        // each post sent once as it came, and post 2 again alone when it changed in place
        assert!(sent.contains(&(vec![2], 2)) && sent.contains(&(vec![3], 3)) && sent.last() == Some(&(vec![2], 3)), "{sent:?}");
        assert_eq!(serde_json::to_value(&held).unwrap(), serde_json::to_value(&whole).unwrap());
    }

    /// A ticket's changes leave one change of it in the ring, beside its whole event (paper-research N-1446 p2604:
    /// research's ring held 1,564 changes, 1 GB as JSON, until a snapshot cleared it): the newest, carrying the posts of
    /// those it replaced. A watcher that resumes after any event ends with the tickets the live stream gave, merged as the
    /// window merges them (live.rs: a whole ticket taken; a change's posts merged by number, stale when not as many as it
    /// says), which are the tickets whole; resumed after a ticket's whole event, it is not sent the ticket whole again.
    #[test]
    fn a_resumed_watcher_holds_what_the_live_one_holds_from_one_change_a_ticket() {
        let (coord, mut core) = coord_of("ringtix");
        let mut rx = core.tx.subscribe();
        let from = core.seq;
        let form = json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null});
        let open = |core: &mut Core, text: &str| {
            core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: form.clone(), text: Some(Text::New(text.into())), names: vec!["w2".into()], told: vec![] }).unwrap();
            *core.tickets.m.tickets.keys().next_back().unwrap()
        };
        let post = |core: &mut Core, t: u64, text: &str| {
            core.ticket_input(Input::Post { t, by: "w2".into(), text: Text::New(text.into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        };
        // two threads changing in turn, posts new and changed in place, a delivery between
        let a = open(&mut core, "one");
        post(&mut core, a, "a two");
        let b = open(&mut core, "other");
        core.emit(Body::Delivered { id: "N-9.p1".into(), to: "w2".into(), how: "channel".into() });
        post(&mut core, a, "a three");
        post(&mut core, b, "b two");
        core.ticket_input(Input::Typed { t: a, n: 2, agent: "w2".into() }).unwrap();
        post(&mut core, a, "a four");
        post(&mut core, b, "b three");
        core.ticket_input(Input::Typed { t: b, n: 2, agent: "w2".into() }).unwrap();
        let (ia, ib) = (core.tickets.m.id(a), core.tickets.m.id(b));
        let live: Vec<Event> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert!(live.iter().filter(|e| matches!(e.body, Body::TicketChange { .. })).count() >= 4, "changes sent: {live:?}");
        // the window's merge, by ticket: its view and whether it is stale
        type Held = BTreeMap<String, (coord_types::TicketView, bool)>;
        let apply = |held: &mut Held, e: &Event| match &e.body {
            Body::Ticket(v) => {
                held.insert(v.id.clone(), (v.clone(), false));
            }
            Body::TicketChange { view, posts } => {
                let (mut all, stale) = held.remove(&view.id).map_or((Vec::new(), true), |(v, s)| (v.posts, s));
                all.retain(|p| !view.posts.iter().any(|q| q.n == p.n));
                all.extend(view.posts.clone());
                all.sort_by_key(|p| p.n);
                let stale = stale || all.len() as u32 != *posts;
                held.insert(view.id.clone(), (coord_types::TicketView { posts: all, ..view.clone() }, stale));
            }
            _ => {}
        };
        let held_by = |events: &[Event]| {
            let mut held = Held::new();
            events.iter().for_each(|e| apply(&mut held, e));
            held
        };
        let want = held_by(&live);
        // the ring: one whole event and one change a ticket, its bytes as a recount gives them
        let ids: Vec<(&str, String)> = core.ring.iter().filter_map(|(_, e)| e.as_ref()).filter_map(|e| match &e.body {
            Body::Ticket(v) => Some(("whole", v.id.clone())),
            Body::TicketChange { view, .. } => Some(("change", view.id.clone())),
            _ => None,
        }).collect();
        let mut once = ids.clone();
        once.sort();
        once.dedup();
        assert_eq!(once.len(), ids.len(), "one whole and one change a ticket in the ring: {ids:?}");
        assert!([&ia, &ib].iter().all(|id| ids.contains(&("whole", id.to_string())) && ids.contains(&("change", id.to_string()))), "{ids:?}");
        let held: Vec<&Event> = core.ring.iter().filter_map(|(_, e)| e.as_ref()).collect();
        assert_eq!((core.held_events, core.ring_bytes), (held.len(), held.iter().map(|e| serde_json::to_vec(e).unwrap().len() as u64).sum::<u64>()));
        // a watcher that had the live stream up to each event, then resumed after it
        for k in 0..=live.len() {
            let at = k.checked_sub(1).map_or(from, |i| live[i].seq);
            let mut got = held_by(&live[..k]);
            core.since(at).unwrap().iter().for_each(|e| apply(&mut got, e));
            assert_eq!(got, want, "resumed after event {at}");
        }
        // resumed after a ticket's whole event, it is sent what moved since
        let whole = live.iter().find(|e| matches!(&e.body, Body::Ticket(v) if v.id == ia)).map(|e| e.seq).unwrap();
        let again: Vec<u64> = core.since(whole).unwrap().iter().filter(|e| matches!(&e.body, Body::Ticket(v) if v.id == ia)).map(|e| e.seq).collect();
        assert!(again.is_empty(), "{ia} whole again at {again:?}");
        // and what it holds is each ticket whole
        for (t, id) in [(a, &ia), (b, &ib)] {
            assert!(!want[id].1, "{id} is not stale");
            assert_eq!(serde_json::to_value(&want[id].0).unwrap(), serde_json::to_value(core.ticket_shown(t).unwrap()).unwrap(), "{id}");
        }
        let _ = std::fs::remove_dir_all(&coord);
    }
}

#[cfg(test)]
mod bare_tests {
    use super::*;
    use docket_machine::{Input, Text};
    use std::io::Write;

    /// The owner's words to an agent that holds only them go without an id, and are found in its transcript by
    /// position, the body the check (paper-research m1114): an id-less entry from before the hand-over is not it, one
    /// with other words is not taken, the one after it with the words sent is; and no second delivery goes without an
    /// id while one is outstanding.
    #[test]
    fn words_without_an_id_are_found_by_position_and_never_two_at_once() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-bare-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        let transcript = coord.join("w2.jsonl");
        let entry = |body: &str| format!("{}\n", json!({"type": "user", "isMeta": true, "message": {"content": format!("<channel source=\"coord\" from=\"owner\">\n{body}\n</channel>")}}));
        std::fs::write(&transcript, entry("what is 6 times 7?")).unwrap(); // the same words, before the hand-over
        let w2 = core.agents.get_mut("w2").unwrap();
        w2.live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        w2.transcript_file = Some(transcript.display().to_string());

        let r = core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New("what is 6 times 7?".into())), names: vec!["w2".into()], told: vec![] });
        assert!(r.is_ok(), "{r:?}");
        // the delivery program's `free` hands his post
        core.raise_delivery();
        core.hand_over("w2");
        let (ids, d, view) = loop {
            match rx.try_recv().expect("a channel job") {
                Job::Channel { ids, tickets, .. } => break (ids, tickets[0].0.clone(), tickets[0].1.clone()),
                _ => continue,
            }
        };
        // alone (Reply posts there) and single (nothing else held): the words, the tag only from, the key off the meta
        let (content, meta) = crate::render::delivery(&core.places(), &d, &view, true, true, false, false, None).unwrap();
        assert_eq!(content.trim(), "what is 6 times 7?");
        assert_eq!(meta, json!({"from": "owner", "key": d.id}));
        // not single: the id is in the tag, as before
        let (_, meta) = crate::render::delivery(&core.places(), &d, &view, true, false, false, false, None).unwrap();
        assert_eq!(meta["id"], json!(d.id));

        core.bare_handed("w2", &d.id, &content);
        core.handed("w2", &ids, true, "channel");
        assert!(core.bare_pending("w2"), "outstanding until its entry is seen: nothing more goes without an id");
        let t = docket_machine::parse_delivery(&d.id).unwrap().0;
        let place = |core: &Core| core.tickets.m.tickets[&t].given.iter().find(|g| g.delivery == d.id).and_then(|g| g.place.clone());
        assert!(place(&core).is_none(), "taken by the channel, no place yet");

        let len = |p: &std::path::Path| std::fs::metadata(p).unwrap().len();
        let f = |body: &str| {
            let offset = len(&transcript);
            let e = entry(body);
            std::fs::OpenOptions::new().append(true).open(&transcript).unwrap().write_all(e.as_bytes()).unwrap();
            (crate::follow::bare_body(&format!("<channel source=\"coord\" from=\"owner\">\n{body}\n</channel>")).unwrap(), docket_machine::Loc { file: transcript.display().to_string(), offset, len: e.len() as u64 - 1 })
        };
        // the entry from before the hand-over is not it
        core.bare_seen("w2", "what is 6 times 7?", docket_machine::Loc { file: transcript.display().to_string(), offset: 0, len: 10 });
        assert!(core.bare_pending("w2") && place(&core).is_none());
        // other words after it: logged, not taken
        let (body, loc) = f("something else");
        core.bare_seen("w2", &body, loc);
        assert!(core.bare_pending("w2") && place(&core).is_none());
        // the words sent: taken, at that place
        let (body, loc) = f("what is 6 times 7?");
        core.bare_seen("w2", &body, loc.clone());
        assert!(!core.bare_pending("w2"));
        assert_eq!(place(&core), Some(loc));

        // at most one outstanding: a second hand-over without an id replaces a first never seen (logged), so a
        // position never names two
        core.bare_handed("w2", "N-9.p1", "one");
        core.bare_handed("w2", "N-10.p1", "two");
        assert_eq!(core.bare.len(), 1);
        assert_eq!(core.bare["w2"].id, "N-10.p1");
    }

    /// A new project's agent given his "hello" and nothing else (docket-edge7's walk, N-1446 p3050, 1): his words alone
    /// go with no id, Claude Code enqueues them, dequeues them and writes them as the prompt of a turn, the agent answers
    /// and stops. The rule Reply posts that answer on his thread: the turn holds his words, not the next one (follow.rs
    /// takes them out of the queue at that prompt; before, they stayed queued, `handed_after` counted them as the next
    /// turn's, and the answer was posted nowhere while the thread said "Asked, no answer yet").
    #[test]
    fn the_owners_words_alone_dequeued_as_a_prompt_are_answered_on_his_thread() {
        use std::io::Write;
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-bare-reply-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        let transcript = coord.join("w2.jsonl");
        std::fs::write(&transcript, "").unwrap();
        let w2 = core.agents.get_mut("w2").unwrap();
        w2.live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        w2.transcript_file = Some(transcript.display().to_string());
        core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "w2", "typed": ["w2"], "names": [], "text": "hello", "addressed": {"head": "Unnamed"}, "follows": null}), text: Some(Text::New("hello".into())), names: vec!["w2".into()], told: vec![] }).unwrap();
        let t = *core.tickets.m.tickets.keys().filter(|t| **t < docket_machine::HIDDEN).max().unwrap();
        core.raise_delivery();
        core.hand_over("w2");
        let (ids, d, view) = loop {
            match rx.try_recv().expect("a channel job") {
                Job::Channel { ids, tickets, .. } => break (ids, tickets[0].0.clone(), tickets[0].1.clone()),
                _ => continue,
            }
        };
        let (content, meta) = crate::render::delivery(&core.places(), &d, &view, true, true, false, false, None).unwrap();
        assert_eq!(meta, json!({"from": "owner", "key": d.id}), "his words alone, with no id");
        core.bare_handed("w2", &d.id, &content);
        core.handed("w2", &ids, true, "channel");
        // as the walk's session wrote it: enqueue, dequeue, the prompt, the answer, the Stop hook's summary
        let tag = format!("<channel source=\"coord\" from=\"owner\">\n{}\n</channel>", content.trim());
        let lines = [
            json!({"type": "queue-operation", "operation": "enqueue", "timestamp": "2026-11-24T20:21:38.937Z", "sessionId": "s", "content": tag}),
            json!({"type": "queue-operation", "operation": "dequeue", "timestamp": "2026-11-24T20:21:38.991Z", "sessionId": "s"}),
            json!({"type": "user", "isMeta": true, "origin": {"kind": "channel"}, "timestamp": "2026-11-24T20:21:39.047Z", "message": {"role": "user", "content": tag}}),
            json!({"type": "assistant", "timestamp": "2026-11-24T20:21:40.000Z", "message": {"role": "assistant", "stop_reason": "end_turn", "content": [{"type": "text", "text": "Hello. I'm up and waiting for a task."}]}}),
            json!({"type": "system", "subtype": "stop_hook_summary", "timestamp": "2026-11-24T20:21:40.795Z"}),
        ];
        core.hook("w2", "prompt", &json!({}));
        core.begin_turn("w2");
        core.hook("w2", "stop", &json!({}));
        // the follower begun before the session writes, as coordd's is
        let mut f = crate::follow::Follower::new(&transcript.display().to_string());
        let mut w = std::fs::OpenOptions::new().append(true).open(&transcript).unwrap();
        for l in &lines {
            writeln!(w, "{l}").unwrap();
        }
        drop(w);
        let r = f.read();
        let path = f.path.clone();
        let outputs: Vec<(String, u64, u64)> = f.take_outputs().into_iter().map(|(o, l)| (path.clone(), o, l)).collect();
        let at = r.at.into_iter().map(|(id, o, l)| (id, path.clone(), o, l)).collect();
        let bare = r.bare.into_iter().map(|(b, o, l)| (b, path.clone(), o, l)).collect();
        let entered = r.entered.into_iter().map(|e| (e, path.clone())).collect();
        core.transcript_read_queued("w2", at, bare, Vec::new(), &outputs, r.cut, &r.begins, r.running_old, entered, &r.queued);
        let posts: Vec<String> = core.tickets.m.tickets[&t].posts.iter().filter(|p| p.by == "w2").filter_map(|p| crate::label::read(&p.text)).collect();
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!(outputs.len(), 1, "the turn's answer is its output");
        assert_eq!(posts, vec!["Hello. I'm up and waiting for a task.".to_string()], "posted on his thread as the reply");
    }

    /// The run on eng-79 (docket-edge7, N-1446 p3214), whose answer the test above did not cover: try-1's session lines
    /// as it wrote them (testdata/bare-hello-eng79.jsonl), read in two parts, with the check of what was given and not
    /// read (`not_read`) between, after the turn's Stop and before its answer is read. His words alone are delivered at
    /// their prompt as words with an id would be (`entry_is`), so the check finds nothing lost, nothing is handed again
    /// ("your session did not take it up", which try-1 answered with Nothing), and the answer is posted on his thread.
    #[test]
    fn the_owners_words_alone_are_delivered_at_their_prompt_and_a_check_between_hands_nothing_again() {
        use std::io::Write;
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-bare-check-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        let transcript = coord.join("w2.jsonl");
        std::fs::write(&transcript, "").unwrap();
        let w2 = core.agents.get_mut("w2").unwrap();
        w2.live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        w2.transcript_file = Some(transcript.display().to_string());
        core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "w2", "typed": ["w2"], "names": [], "text": "hello", "addressed": {"head": "Unnamed"}, "follows": null}), text: Some(Text::New("hello".into())), names: vec!["w2".into()], told: vec![] }).unwrap();
        let t = *core.tickets.m.tickets.keys().filter(|t| **t < docket_machine::HIDDEN).max().unwrap();
        core.raise_delivery();
        core.hand_over("w2");
        let (ids, d, view) = loop {
            match rx.try_recv().expect("a channel job") {
                Job::Channel { ids, tickets, .. } => break (ids, tickets[0].0.clone(), tickets[0].1.clone()),
                _ => continue,
            }
        };
        let (content, meta) = crate::render::delivery(&core.places(), &d, &view, true, true, false, false, None).unwrap();
        assert_eq!((meta, content.trim()), (json!({"from": "owner", "key": d.id}), "hello"), "his words alone, with no id, as the run's session has them");
        core.bare_handed("w2", &d.id, &content);
        core.handed("w2", &ids, true, "channel");
        let lines: Vec<&str> = include_str!("../testdata/bare-hello-eng79.jsonl").lines().collect();
        let mut f = crate::follow::Follower::new(&transcript.display().to_string());
        let path = f.path.clone();
        let read = |core: &mut Core, f: &mut crate::follow::Follower, part: &[&str]| {
            let mut w = std::fs::OpenOptions::new().append(true).open(&transcript).unwrap();
            for l in part {
                writeln!(w, "{l}").unwrap();
            }
            drop(w);
            let r = f.read();
            let outputs: Vec<(String, u64, u64)> = f.take_outputs().into_iter().map(|(o, l)| (path.clone(), o, l)).collect();
            let at = r.at.into_iter().map(|(id, o, l)| (id, path.clone(), o, l)).collect();
            let bare = r.bare.into_iter().map(|(b, o, l)| (b, path.clone(), o, l)).collect();
            let entered = r.entered.into_iter().map(|e| (e, path.clone())).collect();
            core.transcript_read_queued("w2", at, bare, Vec::new(), &outputs, r.cut, &r.begins, r.running_old, entered, &r.queued);
            core.agents.get_mut("w2").unwrap().live.queue = f.queued_now().into_iter().map(|(o, _)| (path.clone(), o)).collect();
            outputs.len()
        };
        core.hook("w2", "prompt", &json!({}));
        core.begin_turn("w2");
        // the enqueue, the dequeue, a reminder, the prompt
        assert_eq!(read(&mut core, &mut f, &lines[..5]), 0);
        let given = |core: &Core| core.tickets.m.tickets[&t].given.iter().filter(|g| g.to == "w2" && g.delivery == d.id).map(|g| (g.place.is_some(), g.delivered.clone())).collect::<Vec<_>>();
        assert_eq!(given(&core), vec![(true, Some("2026-11-24T23:27:59Z".to_string()))], "placed at its enqueue, delivered at its prompt");
        // its turn ends (the Stop hook) before the answer is read, and the check runs then, as it did 1 s after
        core.hook("w2", "stop", &json!({}));
        core.not_read();
        let log = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap_or_default();
        assert!(!log.contains("due_again"), "nothing handed again");
        assert!(core.ticket_given_since("w2").contains(&d.id), "the turn still holds his words");
        assert_eq!(read(&mut core, &mut f, &lines[5..]), 1, "the answer is the turn's output");
        let posts: Vec<String> = core.tickets.m.tickets[&t].posts.iter().filter(|p| p.by == "w2").filter_map(|p| crate::label::read(&p.text)).collect();
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!(posts, vec!["Hello. I'm try-1, ready for a task. The working directory is `/home/t/agent-coord/docket-try/walk2`.".to_string()], "posted on his thread as the reply");
    }
}

#[cfg(test)]
mod presence_tests {
    use super::*;

    /// A Stop hook that never came (N-560: docket-core3's, sent while `coord-switch` had coordd down): the hooks say a
    /// turn runs, the screen none, and the transcript, read by a `Follower` as `follow` reads it, has the turn's end and
    /// has stopped growing. The 60 s rule ends the turn through `follow`'s own call, and the held mail is handed.
    #[test]
    fn a_stop_hook_that_never_came_ends_the_turn_by_the_transcript_and_the_held_mail_is_handed() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-lost-stop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        // in a turn: its prompt hook came, its screen showed it working
        core.hook("w2", "prompt", &json!({}));
        core.begin_turn("w2");
        core.busy("w2", true);
        // its turn ended in the transcript; the Stop hook never came; the screen reads idle
        let transcript = coord.join("w2.jsonl");
        let lines = [
            json!({"type": "user", "timestamp": "2026-11-18T12:39:11Z", "message": {"role": "user", "content": "go on"}}),
            json!({"type": "assistant", "timestamp": "2026-11-18T12:39:55Z", "message": {"role": "assistant", "stop_reason": "end_turn", "content": [{"type": "text", "text": "done"}]}}),
            json!({"type": "system", "subtype": "turn_duration", "timestamp": "2026-11-18T12:39:55Z"}),
        ];
        std::fs::write(&transcript, lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        let mut f = crate::follow::Follower::new(&transcript.display().to_string());
        f.read();
        assert!(f.ended, "the transcript has the turn's end");
        core.busy("w2", false);
        // mail for it, held while it works
        let r = core.ticket_input(docket_machine::Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(docket_machine::Text::New("what is 6 times 7?".into())), names: vec!["w2".into()], told: vec![] });
        assert!(r.is_ok(), "{r:?}");
        assert!(core.in_turn(&core.agents["w2"]), "in a turn: what is due waits on the delivery program's `arrived`");
        let quiet = std::time::Instant::now() - Duration::from_secs(120); // not grown for two minutes
        assert!(!core.turn_ended_now("w2", f.ended, &f.running_since, quiet, false, false), "not at once: the hooks decide for 60 s");
        assert_eq!(core.agents["w2"].live.hook_busy, Some(true));
        core.agents.get_mut("w2").unwrap().live.disagree = Some((std::time::Instant::now() - Duration::from_secs(61), true));
        assert!(core.turn_ended_now("w2", f.ended, &f.running_since, quiet, false, false), "after 60 s the screen and the transcript decide");
        assert_eq!(core.agents["w2"].live.hook_busy, Some(false));
        assert!(std::fs::read_to_string(coord.join("reconcile.jsonl")).unwrap_or_default().contains("no Stop hook came"));
        // what `follow` does with an end: the turn's end taken, and the held mail handed
        assert!(core.end_turn("w2").is_some());
        assert!(!core.in_turn(&core.agents["w2"]), "its end taken: all that is held may be handed");
        assert_eq!(core.to_hand_over(), vec!["w2".to_string()]);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A session resumed onto a background task's notice (research, 25 Nov 21:32 CET: docket-core and docket-edge5):
    /// the transcript ends in the notice, nothing of the model's after it, the screen idle, no Stop to come. The 60 s
    /// rule waits on the transcript's end, which never comes; the turn ends at 300 s and the held mail is handed. A
    /// turn the model has answered in (a `thinking` entry), or one waiting on its tool's result, is not this.
    #[test]
    fn a_turn_begun_on_a_notice_the_model_never_answered_ends_by_the_screen_and_the_held_mail_is_handed() {
        use std::io::Write;
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-unanswered-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        let line = |v: serde_json::Value| format!("{v}\n");
        let notice = json!({"type": "user", "timestamp": "2026-11-25T20:32:20Z", "message": {"role": "user", "content": "<task-notification>\n<status>killed</status>\n<summary>Background shell command didn't finish before the previous session ended</summary>\n</task-notification>"}});
        let transcript = coord.join("w2.jsonl");
        std::fs::write(&transcript, line(json!({"type": "user", "timestamp": "2026-11-25T20:00:00Z", "message": {"role": "user", "content": "go on"}})) + &line(json!({"type": "assistant", "timestamp": "2026-11-25T20:00:09Z", "message": {"role": "assistant", "stop_reason": "end_turn", "content": [{"type": "text", "text": "done"}]}})) + &line(json!({"type": "system", "subtype": "turn_duration", "timestamp": "2026-11-25T20:00:09Z"}))).unwrap();
        let mut f = crate::follow::Follower::new(&transcript.display().to_string());
        f.read();
        assert!(f.ended && !f.unanswered());
        // the new process writes the notice: a turn begins, as coordd's count sees it, and the model is never asked
        std::fs::OpenOptions::new().append(true).open(&transcript).unwrap().write_all(line(notice).as_bytes()).unwrap();
        let r = f.read();
        assert_eq!(r.begins.len(), 1, "the notice begins a turn");
        assert!(!f.ended && f.unanswered(), "begun on the notice, nothing after it");
        core.hook("w2", "prompt", &json!({}));
        core.begin_turn("w2");
        core.busy("w2", false);
        let r = core.ticket_input(docket_machine::Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(docket_machine::Text::New("what is 6 times 7?".into())), names: vec!["w2".into()], told: vec![] });
        assert!(r.is_ok(), "{r:?}");
        assert!(core.in_turn(&core.agents["w2"]), "held while it reads as working");
        let quiet = std::time::Instant::now() - Duration::from_secs(400);
        core.agents.get_mut("w2").unwrap().live.disagree = Some((std::time::Instant::now() - Duration::from_secs(61), true));
        assert!(!core.turn_ended_now("w2", f.ended, &f.running_since, quiet, false, f.unanswered()), "past 60 s: the 60 s rule waits on an end the transcript never shows");
        core.agents.get_mut("w2").unwrap().live.disagree = Some((std::time::Instant::now() - Duration::from_secs(301), true));
        assert!(!core.turn_ended_now("w2", f.ended, &f.running_since, std::time::Instant::now() - Duration::from_secs(10), false, f.unanswered()), "a transcript grown within 300 s: not yet");
        assert!(core.turn_ended_now("w2", f.ended, &f.running_since, quiet, false, f.unanswered()), "300 s, idle and still: the screen decides");
        assert_eq!(core.agents["w2"].live.hook_busy, Some(false));
        assert!(std::fs::read_to_string(coord.join("reconcile.jsonl")).unwrap_or_default().contains("ends in a prompt nothing of the model's has followed"));
        assert!(core.end_turn("w2").is_some());
        assert_eq!(core.to_hand_over(), vec!["w2".to_string()], "what waited is handed");
        // the model answered in it (its thinking written, the rest to come): a turn running, not this
        let mut g = crate::follow::Follower::new(&transcript.display().to_string());
        g.read();
        assert!(g.unanswered());
        let thinking = json!({"type": "assistant", "timestamp": "2026-11-25T20:32:30Z", "message": {"role": "assistant", "content": [{"type": "thinking", "thinking": "..."}]}});
        let tool = json!({"type": "assistant", "timestamp": "2026-11-25T20:32:31Z", "message": {"role": "assistant", "content": [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "true"}}]}});
        let result = json!({"type": "user", "timestamp": "2026-11-25T20:32:32Z", "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]}});
        std::fs::OpenOptions::new().append(true).open(&transcript).unwrap().write_all((line(thinking) + &line(tool)).as_bytes()).unwrap();
        g.read();
        assert!(!g.ended && !g.unanswered(), "answered: a turn running");
        std::fs::OpenOptions::new().append(true).open(&transcript).unwrap().write_all(line(result).as_bytes()).unwrap();
        g.read();
        assert!(!g.ended && !g.unanswered(), "a tool's result waits on the model's next request, which the turn makes itself");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// The first read after a coordd start (consult-3, N-412.p1690): a new follower over a transcript that already holds
    /// turns reads them in its window, and none of their begins is a turn beginning now. Idle, the agent stays idle with
    /// its turn count, no Stop owed and no turn ended. Working on a turn whose first entry is in that window, the turn
    /// is not ended or begun again, and its Stop hook ends it at once.
    #[test]
    fn a_starts_first_read_begins_no_turn_for_the_old_turns_in_its_window() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let user = |text: &str| json!({"type": "user", "timestamp": "2026-11-19T19:30:00Z", "message": {"role": "user", "content": text}});
        let said = |text: &str| json!({"type": "assistant", "timestamp": "2026-11-19T19:30:10Z", "message": {"role": "assistant", "stop_reason": "end_turn", "content": [{"type": "text", "text": text}]}});
        let dur = json!({"type": "system", "subtype": "turn_duration", "timestamp": "2026-11-19T19:30:10Z"});
        let tool = json!({"type": "assistant", "timestamp": "2026-11-19T19:31:00Z", "message": {"role": "assistant", "content": [{"type": "tool_use", "id": "toolu_1", "name": "Bash", "input": {"command": "true"}}]}});
        for working in [false, true] {
            let coord = std::env::temp_dir().join(format!("docket-start-read-{}-{working}", std::process::id()));
            let _ = std::fs::remove_dir_all(&coord);
            std::fs::create_dir_all(&coord).unwrap();
            std::fs::write(coord.join("registry.jsonl"), format!("{}\n", json!({"ev": "add", "id": "AGT-001", "name": "w2", "parent": "owner", "kind": "claude", "model": "m", "at": "x"}))).unwrap();
            let mut core = Core::load(&coord.display().to_string(), &base);
            core.load_tickets();
            core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
            // as the journal holds it at the start: a turn ended by its Stop, and (working) the next begun by its prompt
            core.hook("w2", "prompt", &json!({}));
            core.begin_turn("w2");
            core.hook("w2", "stop", &json!({}));
            let _ = core.end_turn("w2");
            // what `settled` makes of a turn's end with nothing waited on
            core.agents.get_mut("w2").unwrap().live.state = "idle".into();
            if working {
                core.hook("w2", "prompt", &json!({}));
                core.begin_turn("w2");
                core.agents.get_mut("w2").unwrap().begin_seen = false;
            }
            // the transcript before the start: two ended turns, and (working) the third running a tool call
            let mut lines = vec![user("one"), said("first"), dur.clone(), user("two"), said("second"), dur.clone()];
            if working {
                lines.extend([user("three"), tool.clone()]);
            }
            let transcript = coord.join("w2.jsonl");
            std::fs::write(&transcript, lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
            let turn = core.agents["w2"].live.turn;
            // coordd starts: a new follower, its first read taken as `main::follow` takes it
            let mut f = crate::follow::Follower::new(&transcript.display().to_string());
            let follow = |core: &mut Core, f: &mut crate::follow::Follower| -> Vec<(String, (u64, Vec<String>))> {
                let r = f.read();
                let path = f.path.clone();
                let outputs: Vec<(String, u64, u64)> = f.take_outputs().into_iter().map(|(o, l)| (path.clone(), o, l)).collect();
                let at = r.at.into_iter().map(|(id, o, l)| (id, path.clone(), o, l)).collect();
                let mut ends = core.transcript_read("w2", at, Vec::new(), Vec::new(), &outputs, r.cut, &r.begins, r.running_old);
                let ended = core.turn_ended_now("w2", f.ended, &f.running_since.clone(), f.grew, r.cut.is_some(), f.unanswered());
                let working = core.agents["w2"].live.state == "working";
                if !ended && !working {
                    core.begin_turn("w2");
                } else if ended && working && let Some(e) = core.end_turn("w2") {
                    ends.push(("w2".to_string(), e));
                    core.agents.get_mut("w2").unwrap().live.state = "idle".into();
                }
                ends
            };
            let ends = follow(&mut core, &mut f);
            let a = &core.agents["w2"];
            let after = (a.live.state.clone(), a.live.turn, a.stop_owed, ends.len());
            let state = if working { "working" } else { "idle" };
            assert_eq!(after, (state.to_string(), turn, false, 0), "working {working}: no old begin is a turn now");
            if working {
                assert!(core.agents["w2"].begin_seen, "the running turn's first entry is seen");
                // its Stop hook ends it at once: nothing swallows it as an old turn's
                core.hook("w2", "stop", &json!({}));
                let ends = follow(&mut core, &mut f);
                let a = &core.agents["w2"];
                assert_eq!((a.live.state.as_str(), a.live.turn, ends.len()), ("idle", turn, 1), "the Stop ended the turn");
            }
            let _ = std::fs::remove_dir_all(&coord);
        }
    }

    /// A delivery put into a turn as it ended, which the session runs as its next prompt at once (consult-3,
    /// N-807.p61): one read of the transcript holds the old turn's end and the new turn's prompt. The old turn's last
    /// message is posted by the rule Reply on its thread, coordd ends that turn and begins the next, and the next
    /// turn's last message goes on the thread of the delivery that began it, not on the old one's.
    #[test]
    fn a_turns_end_and_the_next_prompt_in_one_read_post_each_reply_on_its_own_thread() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-next-prompt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let reg: String = [("AGT-001", "lead", "owner"), ("AGT-002", "w2", "lead"), ("AGT-003", "w3", "owner")].iter().map(|(id, n, p)| format!("{}\n", json!({"ev": "add", "id": id, "name": n, "parent": p, "kind": "claude", "model": "m", "at": "x"}))).collect();
        std::fs::write(coord.join("registry.jsonl"), reg).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let open = |core: &mut Core, text: &str| -> u64 {
            core.ticket_input(docket_machine::Input::Open { kind: "thread".into(), under: None, opener: "w3".into(), form: json!({"to": "w3", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "w3"}, "follows": null}), text: Some(docket_machine::Text::New(text.into())), names: vec!["w2".into()], told: vec![] }).unwrap();
            *core.tickets.m.tickets.keys().filter(|t| **t < docket_machine::HIDDEN).max().unwrap()
        };
        let take = |core: &mut Core, t: u64, into: bool| -> String {
            let d = core.ticket_due("w2").into_iter().find(|d| d.t == t).expect("due");
            core.tickets.handed.insert(("w2".into(), d.id.clone()), (d.t, d.upto, Default::default()));
            if into {
                core.tickets.into.insert(("w2".into(), d.id.clone()));
            }
            core.ticket_handed("w2", std::slice::from_ref(&d.id), true);
            d.id
        };
        core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        let a = open(&mut core, "measure the drag");
        let a_id = take(&mut core, a, false);
        core.hook("w2", "prompt", &json!({}));
        core.begin_turn("w2");
        // B is put into the turn as it ends
        let b = open(&mut core, "the second pane is narrow");
        let b_id = take(&mut core, b, true);
        let chan = |id: &str, ticket: &str| json!({"type": "user", "timestamp": "2026-11-19T13:42:15Z", "message": {"role": "user", "content": format!("<channel source=\"coord\" from=\"w3\" id=\"{id}\" kind=\"thread\" ticket=\"{ticket}\">\nx\n</channel>")}});
        let said = |text: &str| json!({"type": "assistant", "timestamp": "2026-11-19T13:42:50Z", "message": {"role": "assistant", "stop_reason": "end_turn", "content": [{"type": "text", "text": text}]}});
        let dur = json!({"type": "system", "subtype": "turn_duration", "timestamp": "2026-11-19T13:42:50Z"});
        let transcript = coord.join("w2.jsonl");
        std::fs::write(&transcript, "").unwrap();
        let mut f = crate::follow::Follower::new(&transcript.display().to_string());
        let (ta, tb) = (core.tickets.m.id(a), core.tickets.m.id(b));
        let write = |lines: &[Value]| {
            use std::io::Write;
            let mut w = std::fs::OpenOptions::new().append(true).open(&transcript).unwrap();
            for l in lines {
                writeln!(w, "{l}").unwrap();
            }
        };
        let read = |core: &mut Core, f: &mut crate::follow::Follower| {
            let r = f.read();
            let path = f.path.clone();
            let outputs: Vec<(String, u64, u64)> = f.take_outputs().into_iter().map(|(o, l)| (path.clone(), o, l)).collect();
            let at = r.at.into_iter().map(|(id, o, l)| (id, path.clone(), o, l)).collect();
            core.transcript_read("w2", at, Vec::new(), Vec::new(), &outputs, r.cut, &r.begins, r.running_old);
            outputs.len()
        };
        // the old turn's end and B's prompt, Claude Code's Stop hook between them, in one read
        core.hook("w2", "stop", &json!({}));
        write(&[chan(&a_id, &ta), said("the drag measures 3 px"), dur.clone(), chan(&b_id, &tb)]);
        let turn = core.agents["w2"].live.turn;
        let n1 = read(&mut core, &mut f);
        let after_first = (core.agents["w2"].live.turn, core.in_turn(&core.agents["w2"]), core.agents["w2"].live.hook_busy);
        // B's turn ends
        write(&[said("the pane is 40 px narrow"), dur]);
        let n2 = read(&mut core, &mut f);
        let posts = |t: u64| core.tickets.m.tickets[&t].posts.iter().filter(|p| p.by == "w2").filter_map(|p| crate::label::read(&p.text)).collect::<Vec<String>>();
        let (on_a, on_b) = (posts(a), posts(b));
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!(n1, 1, "the old turn's output is taken though the next turn began in the same read");
        assert_eq!(after_first, (turn + 1, true, Some(true)), "the old turn ended and the next one runs");
        assert_eq!(n2, 1);
        assert_eq!(on_a, vec!["the drag measures 3 px".to_string()], "the old turn's reply on its own thread");
        assert_eq!(on_b, vec!["the pane is 40 px narrow".to_string()], "the next turn's reply on the thread that began it");
    }

    /// A turn's end is the session's stop (consult-3, N-807.p63): an `end_turn` message the session writes on after
    /// posts nothing; at the stop, the text since the last prompt is posted, once. A turn that stops on a background
    /// job posts its half-way message, and the turn the job's notice begins posts the rest on the same thread.
    #[test]
    fn a_turns_end_is_the_sessions_stop_and_the_answer_after_a_background_job_goes_on_its_thread() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-stop-end-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let reg: String = [("AGT-001", "lead", "owner"), ("AGT-002", "w2", "lead"), ("AGT-003", "w3", "owner")].iter().map(|(id, n, p)| format!("{}\n", json!({"ev": "add", "id": id, "name": n, "parent": p, "kind": "claude", "model": "m", "at": "x"}))).collect();
        std::fs::write(coord.join("registry.jsonl"), reg).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let open = |core: &mut Core, text: &str| -> u64 {
            core.ticket_input(docket_machine::Input::Open { kind: "thread".into(), under: None, opener: "w3".into(), form: json!({"to": "w3", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "w3"}, "follows": null}), text: Some(docket_machine::Text::New(text.into())), names: vec!["w2".into()], told: vec![] }).unwrap();
            *core.tickets.m.tickets.keys().filter(|t| **t < docket_machine::HIDDEN).max().unwrap()
        };
        let take = |core: &mut Core, t: u64| -> String {
            let d = core.ticket_due("w2").into_iter().find(|d| d.t == t).expect("due");
            core.tickets.handed.insert(("w2".into(), d.id.clone()), (d.t, d.upto, Default::default()));
            core.ticket_handed("w2", std::slice::from_ref(&d.id), true);
            d.id
        };
        let transcript = coord.join("w2.jsonl");
        std::fs::write(&transcript, "").unwrap();
        core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        core.agents.get_mut("w2").unwrap().background = Some(crate::transcript::Background::new(&transcript.display().to_string(), ""));
        let mut f = crate::follow::Follower::new(&transcript.display().to_string());
        let ts = "2026-11-19T15:00:00Z";
        let chan = |id: &str, ticket: &str| json!({"type": "user", "timestamp": ts, "message": {"role": "user", "content": format!("<channel source=\"coord\" from=\"w3\" id=\"{id}\" kind=\"thread\" ticket=\"{ticket}\">\nx\n</channel>")}});
        let said = |text: &str| json!({"type": "assistant", "timestamp": ts, "message": {"role": "assistant", "stop_reason": "end_turn", "content": [{"type": "text", "text": text}]}});
        let tool = |id: &str| json!({"type": "assistant", "timestamp": ts, "message": {"role": "assistant", "content": [{"type": "tool_use", "id": id, "name": "Bash", "input": {"command": "true"}}]}});
        let result = |id: &str, bg: Option<&str>| {
            let mut v = json!({"type": "user", "timestamp": ts, "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": id, "content": "ok"}]}});
            if let Some(b) = bg {
                v["toolUseResult"] = json!({"backgroundTaskId": b});
            }
            v
        };
        let stop = json!({"type": "system", "subtype": "stop_hook_summary", "timestamp": ts});
        let notice_of = |id: &str| json!({"type": "user", "timestamp": ts, "message": {"role": "user", "content": format!("<task-notification>\n<task-id>{id}</task-id>\n<status>completed</status>\n</task-notification>")}});
        let notice = notice_of("b1");
        let write = |lines: &[Value]| {
            use std::io::Write;
            let mut w = std::fs::OpenOptions::new().append(true).open(&transcript).unwrap();
            for l in lines {
                writeln!(w, "{l}").unwrap();
            }
        };
        let read = |core: &mut Core, f: &mut crate::follow::Follower| {
            let r = f.read();
            let path = f.path.clone();
            let outputs: Vec<(String, u64, u64)> = f.take_outputs().into_iter().map(|(o, l)| (path.clone(), o, l)).collect();
            let at = r.at.into_iter().map(|(id, o, l)| (id, path.clone(), o, l)).collect();
            core.transcript_read("w2", at, Vec::new(), Vec::new(), &outputs, r.cut, &r.begins, r.running_old);
        };
        let posts = |core: &Core, t: u64| core.tickets.m.tickets[&t].posts.iter().filter(|p| p.by == "w2").filter_map(|p| crate::label::read(&p.text)).collect::<Vec<String>>();
        // 1. a false end: `end_turn`, and the session writes on with no prompt
        let a = open(&mut core, "measure the drag");
        let a_id = take(&mut core, a);
        core.begin_turn("w2");
        write(&[chan(&a_id, &core.tickets.m.id(a)), said("looking at it")]);
        read(&mut core, &mut f);
        let at_false_end = posts(&core, a);
        // the final message written and the Stop hook come before the follower reads it: the end waits for the
        // transcript's stop, so the half-way text is not taken (consult-3, N-807.p69, 1)
        write(&[tool("t9"), result("t9", None), said("the drag measures 3 px")]);
        core.hook("w2", "stop", &json!({}));
        let span = f.said_span().map(|(o, l)| docket_machine::Loc { file: f.path.clone(), offset: o, len: l });
        core.turn_end_without_stop("w2", span);
        let at_hook = posts(&core, a);
        write(&[stop.clone()]);
        read(&mut core, &mut f);
        let at_stop = posts(&core, a);
        // the hooks' end taking the same text again posts nothing more
        let again = f.said_span();
        core.end_turn("w2");
        // 2. in the order a session takes them (consult-3, N-412.p1205): T1 on B starts a background job and stops
        let b = open(&mut core, "run the tests");
        let b_id = take(&mut core, b);
        core.begin_turn("w2");
        write(&[chan(&b_id, &core.tickets.m.id(b)), tool("u1"), result("u1", Some("b1")), said("the tests run in the background"), stop.clone()]);
        read(&mut core, &mut f);
        core.end_turn("w2");
        core.agents.get_mut("w2").unwrap().live.state = "idle".into();
        let half = posts(&core, b);
        let carried = |core: &Core| core.tickets.carry.get("w2").map(|c| c.iter().filter_map(|(_, to)| to.as_ref().map(|x| x.0)).collect::<Vec<u64>>()).unwrap_or_default();
        let carried_b = carried(&core);
        // T2: in the wait it is handed a post on C, answers it and stops, the job still running
        let c = open(&mut core, "and the third pane?");
        let c_id = take(&mut core, c);
        write(&[chan(&c_id, &core.tickets.m.id(c)), said("the third pane is drawn"), stop.clone()]);
        read(&mut core, &mut f);
        core.end_turn("w2");
        core.agents.get_mut("w2").unwrap().live.state = "idle".into();
        let carried_c = carried(&core);
        // T3: the job's notice begins a turn in coordd's count, with no prompt hook (consult-3, N-807.p69, 3)
        write(&[notice]);
        read(&mut core, &mut f);
        let at_notice = (core.agents["w2"].live.state.clone(), core.in_turn(&core.agents["w2"]));
        write(&[said("3 passed, 0 failed"), stop.clone()]);
        read(&mut core, &mut f);
        core.end_turn("w2");
        let whole = posts(&core, b);
        let on_c = posts(&core, c);
        let after = core.tickets.carry.get("w2").cloned();
        // a later turn given nothing (an update's) posts on neither
        core.agents.get_mut("w2").unwrap().live.state = "idle".into();
        write(&[json!({"type": "user", "timestamp": ts, "message": {"role": "user", "content": "an update"}}), said("noted"), stop.clone()]);
        read(&mut core, &mut f);
        let later = (posts(&core, b).len(), posts(&core, c).len());
        // 3. coordd restarts while a job runs (consult-3, N-412.p1253, 1): T4 on D starts b2 and stops; the restart
        // leaves the carry empty and makes the reader again; T5 on E in the wait; b2's notice posts on neither
        let turn = |core: &mut Core, f: &mut crate::follow::Follower, lines: &[Value]| {
            core.agents.get_mut("w2").unwrap().live.state = "idle".into();
            write(lines);
            read(core, f);
            core.end_turn("w2");
        };
        let d = open(&mut core, "build the exe");
        let d_id = take(&mut core, d);
        let d_tid = core.tickets.m.id(d);
        turn(&mut core, &mut f, &[chan(&d_id, &d_tid), tool("u2"), result("u2", Some("b2")), said("the exe builds on the tower"), stop.clone()]);
        core.tickets.carry.clear();
        core.agents.get_mut("w2").unwrap().background = Some(crate::transcript::Background::new(&transcript.display().to_string(), "").found_running());
        let e = open(&mut core, "and the sidebar?");
        let e_id = take(&mut core, e);
        let e_tid = core.tickets.m.id(e);
        turn(&mut core, &mut f, &[chan(&e_id, &e_tid), said("the sidebar is fixed"), stop.clone()]);
        let carried_e = carried(&core);
        turn(&mut core, &mut f, &[notice_of("b2"), said("the exe is built"), stop.clone()]);
        let restart = (posts(&core, d), posts(&core, e));
        // 4. a turn cut after it starts a job (consult-3, p1253, 2): T7 on G starts b3 and is cut; T8 on H in the
        // wait; b3's notice posts on neither
        let g = open(&mut core, "run the replay");
        let g_id = take(&mut core, g);
        core.agents.get_mut("w2").unwrap().live.state = "idle".into();
        write(&[chan(&g_id, &core.tickets.m.id(g)), tool("u3"), result("u3", Some("b3"))]);
        read(&mut core, &mut f);
        core.ticket_turn_cut("w2");
        core.end_turn("w2");
        write(&[stop.clone()]);
        read(&mut core, &mut f);
        let h = open(&mut core, "and the footer?");
        let h_id = take(&mut core, h);
        let h_tid = core.tickets.m.id(h);
        turn(&mut core, &mut f, &[chan(&h_id, &h_tid), said("the footer is drawn"), stop.clone()]);
        let carried_h = core.tickets.carry.get("w2").cloned().unwrap_or_default();
        turn(&mut core, &mut f, &[notice_of("b3"), said("the replay passed"), stop.clone()]);
        let cut = (posts(&core, g), posts(&core, h));
        // 5. a carried job whose notice comes into a turn that is then cut (consult-3, N-412.p1510): T9 on J starts b4
        // and stops; T10 on K starts b5, takes b4's notice and is cut; T11, b5's notice, given nothing: posts on neither
        let j = open(&mut core, "run the bench");
        let j_id = take(&mut core, j);
        let j_tid = core.tickets.m.id(j);
        turn(&mut core, &mut f, &[chan(&j_id, &j_tid), tool("u4"), result("u4", Some("b4")), said("the bench runs"), stop.clone()]);
        let k = open(&mut core, "and the replay?");
        let k_id = take(&mut core, k);
        core.agents.get_mut("w2").unwrap().live.state = "idle".into();
        write(&[chan(&k_id, &core.tickets.m.id(k)), tool("u5"), result("u5", Some("b5")), notice_of("b4")]);
        read(&mut core, &mut f);
        core.ticket_turn_cut("w2");
        core.end_turn("w2");
        let carried_k = core.tickets.carry.get("w2").cloned().unwrap_or_default();
        write(&[stop.clone()]);
        read(&mut core, &mut f);
        turn(&mut core, &mut f, &[notice_of("b5"), said("the replay passed"), stop.clone()]);
        let cut_notice = (posts(&core, j), posts(&core, k));
        let on_a = posts(&core, a);
        let _ = std::fs::remove_dir_all(&coord);
        assert!(at_false_end.is_empty(), "an end_turn alone posts nothing: {at_false_end:?}");
        assert!(at_hook.is_empty(), "the Stop hook before the read takes no half-way text: {at_hook:?}");
        assert_eq!(at_stop, vec!["the drag measures 3 px".to_string()], "the stop posts the turn's final text");
        assert_eq!(again, None, "nothing said since the stop");
        assert_eq!(on_a.len(), 1, "once");
        assert_eq!(half, vec!["the tests run in the background".to_string()]);
        assert_eq!(carried_b, vec![b], "B's job carried while it runs");
        assert_eq!(carried_c, vec![b], "a turn on C in the wait neither moves the carry nor adds C");
        assert_eq!(at_notice, ("working".to_string(), true), "the notice begins a turn in coordd's count");
        assert_eq!(on_c, vec!["the third pane is drawn".to_string()], "C has only its own answer");
        assert_eq!(whole, vec!["the tests run in the background".to_string(), "3 passed, 0 failed".to_string()], "B has its half and its rest, nothing of C's");
        assert_eq!(after, None, "the job's notice taken: carried no further");
        assert_eq!(later, (2, 1), "a turn given nothing after it posts on neither");
        assert!(carried_e.is_empty(), "work found running at the start is carried by no turn: {carried_e:?}");
        assert_eq!(restart, (vec!["the exe builds on the tower".to_string()], vec!["the sidebar is fixed".to_string()]), "after a restart the notice posts on neither");
        assert_eq!(carried_h, vec![("shell b3".to_string(), None)], "the cut turn's work carried with no thread");
        assert!(cut.0.is_empty() && cut.1 == vec!["the footer is drawn".to_string()], "after a cut the notice posts on neither: {cut:?}");
        assert_eq!(carried_k, vec![("shell b5".to_string(), None)], "a carried job whose notice came into the cut turn is carried no further");
        assert_eq!(cut_notice, (vec!["the bench runs".to_string()], Vec::<String>::new()), "J has only its half, K nothing of the later turn: {cut_notice:?}");
    }

    /// The same, with the old turn's Stop hook reaching coordd after the read that began the next turn (consult-3,
    /// N-807.p67): that Stop is the old turn's and ends nothing; the next turn runs on until its own Stop, and its
    /// reply lands on its own thread.
    #[test]
    fn the_old_turns_stop_coming_after_the_next_prompt_ends_nothing() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-late-stop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let reg: String = [("AGT-001", "lead", "owner"), ("AGT-002", "w2", "lead"), ("AGT-003", "w3", "owner")].iter().map(|(id, n, p)| format!("{}\n", json!({"ev": "add", "id": id, "name": n, "parent": p, "kind": "claude", "model": "m", "at": "x"}))).collect();
        std::fs::write(coord.join("registry.jsonl"), reg).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let open = |core: &mut Core, text: &str| -> u64 {
            core.ticket_input(docket_machine::Input::Open { kind: "thread".into(), under: None, opener: "w3".into(), form: json!({"to": "w3", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "w3"}, "follows": null}), text: Some(docket_machine::Text::New(text.into())), names: vec!["w2".into()], told: vec![] }).unwrap();
            *core.tickets.m.tickets.keys().filter(|t| **t < docket_machine::HIDDEN).max().unwrap()
        };
        let take = |core: &mut Core, t: u64, into: bool| -> String {
            let d = core.ticket_due("w2").into_iter().find(|d| d.t == t).expect("due");
            core.tickets.handed.insert(("w2".into(), d.id.clone()), (d.t, d.upto, Default::default()));
            if into {
                core.tickets.into.insert(("w2".into(), d.id.clone()));
            }
            core.ticket_handed("w2", std::slice::from_ref(&d.id), true);
            d.id
        };
        core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        let a = open(&mut core, "measure the drag");
        let a_id = take(&mut core, a, false);
        core.hook("w2", "prompt", &json!({}));
        core.begin_turn("w2");
        // B is put into the turn as it ends
        let b = open(&mut core, "the second pane is narrow");
        let b_id = take(&mut core, b, true);
        let chan = |id: &str, ticket: &str| json!({"type": "user", "timestamp": "2026-11-19T13:42:15Z", "message": {"role": "user", "content": format!("<channel source=\"coord\" from=\"w3\" id=\"{id}\" kind=\"thread\" ticket=\"{ticket}\">\nx\n</channel>")}});
        let said = |text: &str| json!({"type": "assistant", "timestamp": "2026-11-19T13:42:50Z", "message": {"role": "assistant", "stop_reason": "end_turn", "content": [{"type": "text", "text": text}]}});
        let dur = json!({"type": "system", "subtype": "turn_duration", "timestamp": "2026-11-19T13:42:50Z"});
        let transcript = coord.join("w2.jsonl");
        std::fs::write(&transcript, "").unwrap();
        let mut f = crate::follow::Follower::new(&transcript.display().to_string());
        let (ta, tb) = (core.tickets.m.id(a), core.tickets.m.id(b));
        let write = |lines: &[Value]| {
            use std::io::Write;
            let mut w = std::fs::OpenOptions::new().append(true).open(&transcript).unwrap();
            for l in lines {
                writeln!(w, "{l}").unwrap();
            }
        };
        let read = |core: &mut Core, f: &mut crate::follow::Follower| {
            let r = f.read();
            let path = f.path.clone();
            let outputs: Vec<(String, u64, u64)> = f.take_outputs().into_iter().map(|(o, l)| (path.clone(), o, l)).collect();
            let at = r.at.into_iter().map(|(id, o, l)| (id, path.clone(), o, l)).collect();
            core.transcript_read("w2", at, Vec::new(), Vec::new(), &outputs, r.cut, &r.begins, r.running_old);
            outputs.len()
        };
        // the old turn's end and B's prompt in one read; its Stop hook comes after it
        write(&[chan(&a_id, &ta), said("the drag measures 3 px"), dur.clone(), chan(&b_id, &tb)]);
        let turn = core.agents["w2"].live.turn;
        let n1 = read(&mut core, &mut f);
        let after_first = (core.agents["w2"].live.turn, core.in_turn(&core.agents["w2"]), core.agents["w2"].live.hook_busy);
        core.hook("w2", "stop", &json!({}));
        let still = (core.in_turn(&core.agents["w2"]), core.agents["w2"].live.hook_busy, core.turn_ended_now("w2", false, "", std::time::Instant::now(), false, false));
        // B's turn ends, its own Stop first
        core.hook("w2", "stop", &json!({}));
        let b_stop = core.agents["w2"].live.hook_busy;
        write(&[said("the pane is 40 px narrow"), dur]);
        let n2 = read(&mut core, &mut f);
        let posts = |t: u64| core.tickets.m.tickets[&t].posts.iter().filter(|p| p.by == "w2").filter_map(|p| crate::label::read(&p.text)).collect::<Vec<String>>();
        let (on_a, on_b) = (posts(a), posts(b));
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!(n1, 1, "the old turn's output is taken though the next turn began in the same read");
        assert_eq!(after_first, (turn + 1, true, Some(true)), "the old turn ended and the next one runs");
        assert_eq!(n2, 1);
        assert_eq!(still, (true, Some(true), false), "the late Stop is the old turn's: B runs on");
        assert_eq!(b_stop, Some(false), "B's own Stop ends B");
        assert_eq!(on_a, vec!["the drag measures 3 px".to_string()], "the old turn's reply on its own thread");
        assert_eq!(on_b, vec!["the pane is 40 px narrow".to_string()], "the next turn's reply on the thread that began it");
    }

    /// Presence from the hooks (paper-research m1162, 9): UserPromptSubmit says a turn runs, Stop or StopFailure that
    /// it ended; a disagreement with the screen is logged once, after 5 s.
    #[test]
    fn the_hooks_say_whether_a_turn_runs_and_a_disagreement_with_the_screen_is_logged() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-presence-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        let hooks = |core: &Core| core.agents["w2"].live.hook_busy;
        assert_eq!(hooks(&core), None, "nothing said yet: the screen decides");
        core.hook("w2", "prompt", &json!({}));
        assert_eq!(hooks(&core), Some(true));
        core.hook("w2", "pre", &json!({"tool_name": "Bash", "tool_use_id": "t1"}));
        assert_eq!(hooks(&core), Some(true), "a tool call says nothing new");
        core.hook("w2", "stop", &json!({}));
        assert_eq!(hooks(&core), Some(false));
        core.hook("w2", "prompt", &json!({}));
        core.hook("w2", "failed", &json!({"error": "overloaded"}));
        assert_eq!(hooks(&core), Some(false), "an API error ends the turn too");

        let log = || std::fs::read_to_string(coord.join("reconcile.jsonl")).unwrap_or_default();
        let quiet = std::time::Instant::now() - Duration::from_secs(120);
        core.presence_agrees("w2", Some(false), Some(true), quiet, true, false);
        assert!(log().is_empty(), "not at once");
        core.agents.get_mut("w2").unwrap().live.disagree = Some((std::time::Instant::now() - Duration::from_secs(6), false));
        core.presence_agrees("w2", Some(false), Some(true), quiet, true, false);
        core.presence_agrees("w2", Some(false), Some(true), quiet, true, false);
        assert_eq!(log().lines().count(), 1, "once: {}", log());
        assert!(log().contains("the hooks say none, the screen a turn running"), "{}", log());
        core.presence_agrees("w2", Some(true), Some(true), quiet, true, false);
        assert!(core.agents["w2"].live.disagree.is_none(), "agreeing again ends the episode");
        // the hooks' turn end never came (a box on a restarted coordd's socket, N-101.p1270): the screen decides at 60 s
        core.hook("w2", "prompt", &json!({}));
        core.presence_agrees("w2", Some(true), Some(false), quiet, true, false);
        assert_eq!(hooks(&core), Some(true), "not at once");
        // the transcript still growing: the turn runs, the screen is wrong (Tovrin-dev's 14 of 17 Nov, N-101 p1459)
        core.agents.get_mut("w2").unwrap().live.disagree = Some((std::time::Instant::now() - Duration::from_secs(61), true));
        core.presence_agrees("w2", Some(true), Some(false), std::time::Instant::now() - Duration::from_secs(10), true, false);
        assert_eq!(hooks(&core), Some(true), "no end while the transcript grows");
        assert!(!log().contains("the screen decides"), "{}", log());
        // a long final answer: the transcript's last entry began it (`thinking`), and nothing more comes until it is
        // whole; the screen may read idle the while (B-ROUTES, B-EVIDENCE, B-CUTOFF on Tovrin-dev, N-101 p1550)
        core.agents.get_mut("w2").unwrap().live.disagree = Some((std::time::Instant::now() - Duration::from_secs(61), true));
        core.presence_agrees("w2", Some(true), Some(false), quiet, false, false);
        assert_eq!(hooks(&core), Some(true), "no end while the transcript has none");
        assert!(!log().contains("the screen decides"), "{}", log());
        core.agents.get_mut("w2").unwrap().live.disagree = Some((std::time::Instant::now() - Duration::from_secs(61), true));
        core.presence_agrees("w2", Some(true), Some(false), quiet, true, false);
        assert_eq!(hooks(&core), Some(false), "the screen decides");
        assert!(log().contains("the screen none, and the transcript has the turn's end and has not grown, for 60 s; the screen decides: no Stop hook came"), "{}", log());
        core.agents.get_mut("w2").unwrap().live.disagree = Some((std::time::Instant::now() - Duration::from_secs(61), true));
        core.presence_agrees("w2", Some(false), Some(true), quiet, true, false);
        assert_eq!(hooks(&core), Some(false), "a screen showing a turn the hooks do not is left to the hooks");
        let _ = std::fs::remove_dir_all(&coord);
    }
}

/// Old mail questions on the release (paper-research m1602): the owner's answer to an old qN is a note to the asker and
/// a mail record too, so the question reads answered, now and after a restart, and the mail is never handed; an
/// agent's --re to an old mail stays a plain note; an asker withdraws its old question by its old id.
#[cfg(test)]
mod old_question_tests {
    use super::*;
    use crate::ticket::Said;

    fn coord(name: &str) -> std::path::PathBuf {
        let coord = std::env::temp_dir().join(format!("docket-oldq-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(coord.join("mail")).unwrap();
        std::fs::write(
            coord.join("registry.jsonl"),
            "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"orch\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n{\"ev\":\"add\",\"id\":\"AGT-002\",\"name\":\"dev\",\"parent\":\"orch\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n",
        )
        .unwrap();
        let mail = [
            json!({"ev": "mail", "id": "q1", "at": "2026-11-10T10:00:00Z", "from": "dev", "to": ["owner"], "body": "Raise or return None?", "kind": "question"}),
            json!({"ev": "mail", "id": "m2", "at": "2026-11-10T10:01:00Z", "from": "orch", "to": ["dev"], "body": "Look at calc.py"}),
            json!({"ev": "delivered", "id": "m2", "to": "dev", "at": "2026-11-10T10:01:01Z", "how": "channel"}),
            json!({"ev": "mail", "id": "q3", "at": "2026-11-10T10:02:00Z", "from": "dev", "to": ["owner"], "body": "May I rename it?", "kind": "question"}),
        ];
        std::fs::write(coord.join("mail/log.jsonl"), mail.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        coord
    }

    fn load(coord: &std::path::Path) -> Core {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        core
    }

    fn mail_lines(coord: &std::path::Path) -> Vec<Value> {
        read_jsonl(&coord.join("mail/log.jsonl").display().to_string())
    }

    #[test]
    fn the_owners_answer_to_an_old_question_is_a_note_and_its_mail_record() {
        let coord = coord("answer");
        let mut core = load(&coord);
        assert_eq!(core.question_state("q1"), "open");
        let before = mail_lines(&coord).len();
        let r = core.ticket_msg(&Who::Owner, None, vec!["dev".into()], "Raise ValueError.", false, Some("q1".into()), Vec::new(), Said::default());
        assert!(r.ok, "{}", r.out);
        // the note: to dev, its words beginning `re q1`
        let n = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        let x = &core.tickets.m.tickets[&n];
        assert_eq!((x.kind.as_str(), x.between()), ("thread", true), "an agents' thread now (design part 9b)");
        assert!(matches!(&x.posts[0].text, docket_machine::Text::New(t) if t.starts_with("re q1\n\nRaise ValueError.")), "{:?}", x.posts[0].text);
        assert!(core.tickets.due("dev").iter().any(|d| d.t == n), "dev is handed the note");
        // the mail record and its delivered line
        let lines = mail_lines(&coord);
        assert_eq!(lines.len(), before + 2, "{lines:?}");
        let (m, d) = (&lines[before], &lines[before + 1]);
        assert_eq!((m["ev"].as_str(), m["from"].as_str(), m["re"].as_str(), m["body"].as_str()), (Some("mail"), Some("owner"), Some("q1"), Some("Raise ValueError.")));
        assert_eq!(m["id"], "m3", "the next mN after the highest (questions are numbered apart, as qN)");
        assert_eq!(m["to"], json!(["dev"]));
        assert_eq!((d["ev"].as_str(), d["id"].as_str(), d["to"].as_str()), (Some("delivered"), Some("m3"), Some("dev")));
        assert_eq!(d["how"].as_str().map(|h| h.starts_with("as note ")), Some(true));
        // answered, and nothing of the mail due
        assert_eq!(core.question_state("q1"), "answered");
        assert_eq!(core.question_state("q3"), "open");
        assert!(core.pending("dev").is_empty(), "{:?}", core.pending("dev").iter().map(|m| &m.id).collect::<Vec<_>>());
        // after a restart: the same
        let core = load(&coord);
        assert_eq!(core.question_state("q1"), "answered");
        assert!(core.pending("dev").is_empty());
        let _ = std::fs::remove_dir_all(&coord);
    }

    #[test]
    fn an_agents_reply_to_old_mail_stays_a_plain_note() {
        let coord = coord("agent");
        let mut core = load(&coord);
        let before = mail_lines(&coord).len();
        let r = core.ticket_msg(&Who::Agent("dev".into()), None, Vec::new(), "Done, calc.py is fine.", false, Some("m2".into()), Vec::new(), Said::default());
        assert!(r.ok, "{}", r.out);
        let n = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        assert_eq!((core.tickets.m.tickets[&n].kind.as_str(), core.tickets.m.tickets[&n].between()), ("thread", true));
        assert!(core.tickets.due("orch").iter().any(|d| d.t == n));
        assert_eq!(mail_lines(&coord).len(), before, "no mail record");
        assert_eq!(core.question_state("q1"), "open");
        let _ = std::fs::remove_dir_all(&coord);
    }

    #[test]
    fn an_asker_withdraws_its_old_question_by_its_old_id() {
        let coord = coord("drop");
        let mut core = load(&coord);
        assert!(!core.drop_question(&Who::Agent("orch".into()), "q3", None).ok, "another agent's question");
        let r = core.drop_question(&Who::Agent("dev".into()), "q3", Some("renamed it already".into()));
        assert!(r.ok, "{}", r.out);
        assert_eq!(core.question_state("q3"), "dropped");
        let core = load(&coord);
        assert_eq!(core.question_state("q3"), "dropped");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// The app's Dismiss on an old question's card (paper-research m1626): Request::Drop from the owner, for any asker.
    #[test]
    fn the_owner_dismisses_an_agents_old_question() {
        let coord = coord("dismiss");
        let mut core = load(&coord);
        let before = mail_lines(&coord).len();
        let r = core.drop_question(&Who::Owner, "q1", Some("dismissed by the owner".into()));
        assert!(r.ok, "{}", r.out);
        assert_eq!(r.out, "q1 dropped");
        assert_eq!(core.question_state("q1"), "dropped");
        assert_eq!(core.question_state("q3"), "open", "only that question");
        let lines = mail_lines(&coord);
        assert_eq!(lines.len(), before + 1, "{lines:?}");
        let d = &lines[before];
        assert_eq!((d["ev"].as_str(), d["id"].as_str(), d["by"].as_str(), d["why"].as_str()), (Some("drop"), Some("q1"), Some("owner"), Some("dismissed by the owner")));
        assert_eq!(core.mail_view(core.find_mail("q1").unwrap()).state.as_deref(), Some("dropped"));
        // a second Dismiss writes nothing more
        assert!(core.drop_question(&Who::Owner, "q1", Some("dismissed by the owner".into())).ok);
        assert_eq!(mail_lines(&coord).len(), before + 1);
        // after a restart: the same
        let core = load(&coord);
        assert_eq!(core.question_state("q1"), "dropped");
        assert_eq!(core.question_state("q3"), "open");
        assert_eq!(core.mail_view(core.find_mail("q1").unwrap()).state.as_deref(), Some("dropped"));
        let _ = std::fs::remove_dir_all(&coord);
    }
}

/// What coordd says when it could not do something (paper-research N-8): a record log it cannot write, a model call that
/// failed.
#[cfg(test)]
mod notice_tests {
    use super::*;
    use crate::ticket::Said;
    use std::os::unix::fs::PermissionsExt;

    fn coord(name: &str) -> std::path::PathBuf {
        let coord = std::env::temp_dir().join(format!("docket-notice-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(coord.join("mail")).unwrap();
        std::fs::write(
            coord.join("registry.jsonl"),
            "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"orch\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n{\"ev\":\"add\",\"id\":\"AGT-002\",\"name\":\"dev\",\"parent\":\"orch\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n",
        )
        .unwrap();
        coord
    }

    fn load(coord: &std::path::Path) -> Core {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        core
    }

    fn tickets(core: &Core) -> usize {
        core.tickets.m.tickets.range(..docket_machine::HIDDEN).count()
    }

    fn msg(core: &mut Core, body: &str) -> Reply {
        core.ticket_msg(&Who::Owner, None, vec!["dev".into()], body, false, None, Vec::new(), Said::default())
    }

    #[test]
    fn a_log_that_cannot_be_written_takes_nothing_more_and_says_so() {
        let coord = coord("log");
        let mut core = load(&coord);
        assert!(msg(&mut core, "one").ok);
        let log = coord.join("tickets/log.jsonl");
        let kept = std::fs::read_to_string(&log).unwrap();
        let n = tickets(&core);
        // in its place a copy its lines cannot open: the log is held open (jsonl::essential), so a mode on the file held
        // would not stop them, nor would a coordd out of descriptors
        std::fs::rename(&log, coord.join("tickets/log.jsonl.held")).unwrap();
        std::fs::copy(coord.join("tickets/log.jsonl.held"), &log).unwrap();
        std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o444)).unwrap();
        if std::fs::OpenOptions::new().append(true).open(&log).is_ok() {
            eprintln!("run as root: a file's mode does not stop its writes, nothing to see");
            return;
        }
        // the failing write is the input's own: refused with the notice's words, nothing kept that is not in the log
        // (paper-research N-8 post 16: the lines go in before the answer, no window)
        let why = format!("coordd cannot write its log ({}: Permission denied (os error 13)); nothing more is taken until coordd is restarted", log.display());
        let r = msg(&mut core, "two");
        assert!(!r.ok);
        assert_eq!(r.out, why);
        assert_eq!(tickets(&core), n, "the machine is the log's");
        let r = msg(&mut core, "three");
        assert!(!r.ok);
        assert_eq!(r.out, why);
        assert_eq!(core.log_fault().as_deref(), Some(why.as_str()));
        // the instance's notice, with the same words, once; in the snapshot too
        let ns = core.notices();
        assert_eq!(ns.len(), 1, "{ns:?}");
        assert_eq!((ns[0].ticket.as_deref(), ns[0].text.as_str()), (None, why.as_str()));
        match core.snapshot().body {
            Body::Snapshot(s) => assert_eq!(s.notices, ns),
            _ => unreachable!(),
        }
        // the mail log's writers are refused as well (main.rs writes_record asks log_fault), reads go on
        assert!(core.ticket_list(&Who::Owner, &Value::Null).ok);
        assert_eq!(std::fs::read_to_string(&log).unwrap(), kept, "the log is what it was before the failure");
        // a restart (the file writable again): the state is the log's prefix, and no notice
        std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o644)).unwrap();
        crate::jsonl::forget(&log.display().to_string());
        let mut core = load(&coord);
        assert_eq!(tickets(&core), n);
        assert!(core.notices().is_empty());
        assert!(msg(&mut core, "four").ok);
        let _ = std::fs::remove_dir_all(&coord);
    }

    #[test]
    fn a_model_call_that_failed_is_said_on_its_ticket_until_one_goes_through() {
        let coord = coord("call");
        let mut core = load(&coord);
        // a gateway that serves a model and refuses the call: one tried and failed. One that serves none (the 503 to
        // everything this used before) finds no model to run on, which is passed over unsaid
        // (`an_engine_call_with_no_model_to_run_on_is_passed_over_with_no_notice_under_his_post`)
        let _asked = crate::clean::tests::stand_in_serving(&[("claude-sonnet-5-5", "anthropic"), ("gpt-6.2-sol", "openai"), ("gpt-6-astra", "openai")], |_| ("503 Service Unavailable", json!({"error": {"message": "refused"}})));
        // the owner's words to dev, no @name: the watcher's intake sorts them
        let v = json!({"from": "owner", "to": "dev", "typed": ["dev"], "names": [], "text": "Is the build green?", "addressed": {"head": "Unnamed"}, "esc": []});
        core.ticket_input(docket_machine::Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let owed = |core: &Core, call: &str| core.ticket_due(docket_machine::WATCHER).into_iter().find(|d| matches!(&d.what, docket_machine::What::Step { call: c, .. } if c == call));
        let d = owed(&core, "intake").expect("the intake step");
        let job = core.clerk_job(&d).expect("intake's job");
        let (head, decision, failed) = crate::clerk::answer(&job);
        assert_eq!(head["head"], "Thread", "a failed intake is a thread");
        assert!(failed.is_some(), "{decision:?}");
        core.clerk_answered(&job, head, decision, failed.clone());
        let thread = job.ticket.clone();
        let ns = core.notices();
        assert_eq!(ns.len(), 1, "{ns:?}");
        assert_eq!(ns[0].ticket.as_deref(), Some(thread.as_str()));
        assert_eq!(ns[0].text, format!("your words were filed as a thread without being sorted: the call failed ({})", failed.unwrap()));
        // dev's post on the thread: today's thread kind asks the watcher nothing of it (the watcher's `asks` gone,
        // proto-lead N-1479 p247), so no step, no call to fail, no notice, no mark. A thread opened on the kind from
        // before still makes the step, and coordd answers it Nothing with no call
        // (`an_agents_thread_makes_no_model_call_the_owner_told_or_not`, at 64)
        let t = core.tickets.m.tickets.range(..docket_machine::HIDDEN).map(|(t, _)| *t).find(|t| core.tickets.m.id(*t) == thread).unwrap();
        let post = docket_machine::Input::Post { t, by: "dev".into(), text: docket_machine::Text::New("Green. Shall I merge it?".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None };
        core.ticket_input(post).unwrap();
        assert!(owed(&core, "asks").is_none(), "no asks step");
        let v = core.tickets.m.view(t).unwrap();
        assert!(v.posts.iter().all(|p| p.asks.is_empty() && !p.asks_open), "{:?}", v.posts);
        let ns = core.notices();
        assert_eq!(ns.len(), 1, "the intake's alone: {ns:?}");
        // a later intake call there that goes through clears its notice
        core.clerk_answered(&job, serde_json::json!({"head": "Thread"}), None, None);
        assert!(core.notices().is_empty(), "{:?}", core.notices());
        // a restart clears them all
        let core = load(&coord);
        assert!(core.notices().is_empty());
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A new project's first thread (try-80 on engine 80, docket-edge7 N-1446 p3282): the watcher's intake and the
    /// minder's look found no model to run on (their decision: none chosen, none tried, "no option satisfies the hard
    /// rules"), and two red lines stood under his post. Now each step is passed over as before (a thread, a rest),
    /// coordd's journal says why once per kind, and no notice is drawn (his rule of 14 Nov, paper-research p3284). A
    /// call that did try a model and failed is still said on the ticket, and a call that finds a model again lets the
    /// journal say the next one.
    #[test]
    fn an_engine_call_with_no_model_to_run_on_is_passed_over_with_no_notice_under_his_post() {
        let coord = coord("nomodel");
        let mut core = load(&coord);
        let none = coord_types::Decision { fault: Some("no option satisfies the hard rules".into()), ..Default::default() };
        let failed = Some(crate::policy::unanswered_words(&none));
        assert!(crate::policy::no_model(&none), "none chosen, none tried");
        let v = json!({"from": "owner", "to": "dev", "typed": ["dev"], "names": [], "text": "hello", "addressed": {"head": "Unnamed"}, "esc": []});
        core.ticket_input(docket_machine::Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let d = core.ticket_due(docket_machine::WATCHER).into_iter().find(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "intake")).expect("the intake step");
        let job = core.clerk_job(&d).expect("intake's job");
        core.clerk_answered(&job, json!({"head": "Thread"}), Some(none.clone()), failed.clone());
        assert!(core.notices().is_empty(), "nothing drawn under his post: {:?}", core.notices());
        assert!(core.no_model_said.contains("intake"), "the journal said it");
        let t = core.tickets.m.tickets.range(..docket_machine::HIDDEN).map(|(t, _)| *t).find(|t| core.tickets.m.id(*t) == job.ticket).expect("filed as a thread all the same");
        // the minder's look on the same thread, its job as the limit test below makes it
        let v = core.ticket_shown(t).expect("the thread's view");
        let defs = &core.tickets.m.defs;
        let kind = defs.kinds["thread"].clone();
        let mjob = crate::shape::Job {
            t,
            step: 1,
            ticket: job.ticket.clone(),
            ctx: crate::shape::context(&v, defs, &[("dev".into(), String::new())]),
            scope: kind.form.iter().cloned().collect(),
            calls: defs.calls.clone(),
            kinds: defs.kinds.clone(),
            head: crate::shape::head_for(defs, &kind).expect("the thread's shape head"),
            kind,
            places: core.places(),
        };
        core.minder_answered(&mjob, json!({"head": "Rest"}), Some(none.clone()), failed.clone());
        assert!(core.notices().is_empty(), "nor the minder's: {:?}", core.notices());
        assert!(core.no_model_said.contains("minder"));
        // a call that tried a model and failed is still said there
        let tried = coord_types::Decision { attempts: vec![coord_types::Attempt { model: "m".into(), failure: Some("500".into()), account: None }], ..Default::default() };
        core.clerk_answered(&job, json!({"head": "Thread"}), Some(tried), Some("no model answered".into()));
        assert_eq!(core.notices().len(), 1, "{:?}", core.notices());
        // one that goes through clears it, and the journal may say the next no-model one again
        let chose = coord_types::Decision { chosen: Some("m".into()), ..Default::default() };
        core.clerk_answered(&job, json!({"head": "Thread"}), Some(chose), None);
        assert!(core.notices().is_empty() && !core.no_model_said.contains("intake") && core.no_model_said.contains("minder"));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// Every model at its limit, as the gateway refused Codex from 19:52Z on 20 Nov (the owner's p635, "no option
    /// satisfies the hard rules"), and the fallback's Sonnet 5.5 as well; a stand-in, no model is called. The intake's
    /// and the minder's notices say why in the one copy's words (policy `unanswered_words`), as route and the
    /// cleaning do: the family at its limit and when the gateway tries it again, never the policy's fault (paper-research
    /// N-1446 p1813).
    #[test]
    fn a_call_refused_at_every_limit_is_said_in_his_words_on_its_ticket() {
        const SERVED: &[(&str, &str)] = &[("claude-sonnet-5-5", "anthropic"), ("gpt-6.2-sol", "openai"), ("gpt-6-astra", "openai")];
        let coord = coord("limit");
        let mut core = load(&coord);
        let _asked = crate::clean::tests::stand_in_serving(SERVED, |q| {
            let model = q["model"].as_str().unwrap_or_default();
            let error = json!({"code": "model_cooldown", "last_upstream_error": "usage_limit_reached: The usage limit has been reached", "model": model, "reset_seconds": 683,
                "message": format!("All credentials for model {model} are cooling down (last error: usage_limit_reached: The usage limit has been reached)")});
            ("429 Too Many Requests", json!({"error": error}))
        });
        // the words a failed call's decision D is said in, each of its attempts refused at its limit
        let words = |d: &Option<coord_types::Decision>| {
            let d = d.as_ref().expect("the call's decision");
            assert!(!d.attempts.is_empty() && d.attempts.iter().all(|a| a.failure.as_deref().is_some_and(|f| f.contains("usage_limit_reached"))), "{d:?}");
            let why = crate::policy::unanswered_words(d);
            assert!(why.contains("is at its usage limit") && why.contains("the gateway tries it again at ") && !why.contains("hard rules"), "{why}");
            why
        };
        let said = |core: &Core, start: &str| core.notices().into_iter().find(|n| n.text.starts_with(start)).unwrap_or_else(|| panic!("no notice {start:?}: {:?}", core.notices())).text;
        let owed = |core: &Core, call: &str| core.ticket_due(docket_machine::WATCHER).into_iter().find(|d| matches!(&d.what, docket_machine::What::Step { call: c, .. } if c == call));

        // the owner's words to dev: the watcher's intake
        let v = json!({"from": "owner", "to": "dev", "typed": ["dev"], "names": [], "text": "Is the build green?", "addressed": {"head": "Unnamed"}, "esc": []});
        core.ticket_input(docket_machine::Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let job = core.clerk_job(&owed(&core, "intake").expect("the intake step")).expect("intake's job");
        let (head, decision, failed) = crate::clerk::answer(&job);
        assert_eq!(head["head"], "Thread");
        let why = words(&decision);
        assert_eq!(failed.as_deref(), Some(why.as_str()));
        core.clerk_answered(&job, head, decision, failed);
        let thread = job.ticket.clone();
        assert_eq!(said(&core, "your words"), format!("your words were filed as a thread without being sorted: the call failed ({why})"));

        // dev's post on the thread: the watcher's asks is gone, so no step and no call to fail
        let t = core.tickets.m.tickets.range(..docket_machine::HIDDEN).map(|(t, _)| *t).find(|t| core.tickets.m.id(*t) == thread).unwrap();
        let post = docket_machine::Input::Post { t, by: "dev".into(), text: docket_machine::Text::New("Green. Shall I merge it?".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None };
        core.ticket_input(post).unwrap();
        assert!(owed(&core, "asks").is_none(), "no asks step");

        // the minder on the thread's hole, its job as `minder_job` makes it
        let v = core.ticket_shown(t).expect("the thread's view");
        let defs = &core.tickets.m.defs;
        let kind = defs.kinds["thread"].clone();
        let job = crate::shape::Job {
            t,
            step: 1,
            ticket: thread.clone(),
            ctx: crate::shape::context(&v, defs, &[("dev".into(), String::new())]),
            scope: kind.form.iter().cloned().collect(),
            calls: defs.calls.clone(),
            kinds: defs.kinds.clone(),
            head: crate::shape::head_for(defs, &kind).expect("the thread's shape head"),
            kind,
            places: core.places(),
        };
        let (head, decision, failed) = crate::shape::answer(&job);
        assert_eq!(head["head"], "Rest");
        let why = words(&decision);
        assert_eq!(failed.as_deref(), Some(why.as_str()));
        core.minder_answered(&job, head, decision, failed);
        assert_eq!(said(&core, "the minder"), format!("the minder did not look: the call failed ({why})"));
        assert_eq!(core.notices().len(), 2, "{:?}", core.notices());
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
        let _ = std::fs::remove_dir_all(&coord);
    }
}

#[cfg(test)]
mod box_at_registration_tests {
    use super::*;

    /// N-101 post 434: an agent whose box could not be made says why in `coord ls`, in a fifth column (roster reads
    /// the third, the state), until its box is made.
    #[test]
    fn coord_ls_shows_why_an_agent_has_no_box() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-no-box-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        assert!(core.add(&Who::Owner, "orch", "claude", "claude-haiku-5-5").ok);
        let line = |core: &Core| core.state_lines().out.lines().find(|l| l.starts_with("orch\t")).unwrap().to_string();
        assert_eq!(line(&core).split('\t').count(), 4);
        core.boxed_or_not("orch", Some("no sub-uid range".into()));
        let l = line(&core);
        assert_eq!(l.split('\t').nth(2), Some("stopped"), "the state stays where roster reads it: {l}");
        assert!(l.ends_with("\tnot started, no box: no sub-uid range"), "{l}");
        // the app reads it from the agent's view (docket-edge N-236)
        assert_eq!(core.view("orch").no_box.as_deref(), Some("no sub-uid range"));
        core.boxed_or_not("orch", None);
        assert_eq!(line(&core).split('\t').count(), 4, "made: the reason is gone");
        assert_eq!(core.view("orch").no_box, None);
        assert!(!serde_json::to_string(&core.view("orch")).unwrap().contains("no_box"), "none: not in the view at all");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// N-101 post 406 (the joint proposal's A): in a boxed project an agent is its own user from its registration, its
    /// home built then for the project's path, whoever registers it; a program (the runner) is left out. Needs this
    /// user's sub-uid range: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn a_boxed_projects_agent_gets_its_uid_and_home_when_it_is_registered() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let t = std::env::temp_dir().join(format!("docket-box-reg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let coord = t.join("coord");
        let project = coord.join("files");
        let repo = t.join("repo");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join(".gitignore"), ".conversations/\n").unwrap();
        let o = std::process::Command::new("bash").arg("-c").arg("git init -q && git add . && git -c user.name=t -c user.email=t@t commit -qm one").current_dir(&repo).output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let r = std::fs::canonicalize(&repo).unwrap().display().to_string();
        std::fs::write(project.join("project.toml"), format!("[project]\nname = \"b\"\nrepos = {{ r = \"{r}\" }}\nrepo = \"r\"\nbox = true\n")).unwrap();
        std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
        std::fs::write(coord.join(".proxy-key"), "stand-in\n").unwrap();
        let c = coord.display().to_string();
        let mut core = Core::load(&c, &base);
        core.project = Some(crate::project::Project::load(&c).expect("the project file").expect("a project"));
        assert!(core.add(&Who::Owner, "orch", "claude", "claude-haiku-5-5").ok);
        let k = crate::sandbox::known_uid(&c, "orch").expect("its uid, from its registration");
        let d = format!("{c}/box/orch");
        let s: Value = serde_json::from_str(&std::fs::read_to_string(format!("{d}/claude-settings.json")).expect("its settings, built at registration")).unwrap();
        assert_eq!(s["permissions"]["defaultMode"], "bypassPermissions");
        crate::sandbox::in_ns("jq -e --arg r \"$2\" '.projects[$r].hasTrustDialogAccepted' \"$1/home/.claude.json\" >/dev/null", &[&d, &r]).expect("its home trusts the project's path");
        assert!(core.add(&Who::Owner, "runner", "program", "none").ok);
        assert!(crate::sandbox::known_uid(&c, "runner").is_none(), "a program is no user of its own");
        assert!(!std::path::Path::new(&format!("{c}/box/runner")).exists());
        let _ = k;
        crate::sandbox::in_ns("rm -rf \"$1\"", &[&t.display().to_string()]).unwrap();
    }
}

#[cfg(test)]
mod reserved_names {
    /// No agent is made or renamed `box`: it would answer the steps owed to his window (docket-core3, N-487 p67).
    #[test]
    fn the_box_is_no_agents_name() {
        assert!(super::reserved("box") && super::reserved("owner") && super::reserved("coord"));
        assert!(!super::reserved("boxer") && !super::reserved("docket-core8"));
    }
}

/// `coord duties`' answer under the lock: given, or to be run off it.
pub enum DutiesAsk {
    Done(Reply),
    Run(DutiesJob),
}

/// What the interpreter needs for `coord duties`, taken under the lock: run off it.
pub struct DutiesJob {
    pub holder: Option<String>,
    facts: Vec<(String, Vec<String>)>,
    stamp: Vec<(String, Option<std::time::SystemTime>)>,
    places: crate::policy::Places,
}

impl DutiesJob {
    /// The duties that hold in the job's facts, by eflint-rs (no lock held).
    pub fn run(&self) -> Result<Vec<crate::policy::interp::Duty>, String> {
        crate::policy::interp::acts(&self.places)?.duties(&self.facts)
    }
}

/// LIST as `coord duties` answers it: a JSON `DutyAnswer` per line, HOLDER's only when one is named.
pub fn duties_reply(holder: Option<&str>, list: &[crate::policy::interp::Duty]) -> Reply {
    ok(list
        .iter()
        .filter(|d| holder.is_none_or(|h| d.holder.as_deref() == Some(h)))
        .map(|d| {
            let d = d.clone();
            serde_json::to_string(&coord_types::DutyAnswer { duty: d.duty, objects: d.objects, holder: d.holder, claimant: d.claimant, violated: d.violated, ended_by: d.ended_by }).unwrap()
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

/// The delivered moment and the turn it belongs to (0.2 A1, N-1397 posts 5 and 6), through `transcript_read_queued` as
/// `main::follow` calls it: a giving is delivered when its text entered the model's input, matched to its own enqueue by
/// place, and a turn holds what entered it, never what was still queued when it ended.
#[cfg(test)]
mod delivered_tests {
    use super::*;

    struct T {
        core: Core,
        coord: std::path::PathBuf,
        transcript: std::path::PathBuf,
        f: crate::follow::Follower,
    }

    fn msg(id: &str, ticket: &str) -> String {
        format!("<channel source=\"coord\" from=\"w3\" id=\"{id}\" kind=\"thread\" ticket=\"{ticket}\">\nx\n</channel>")
    }
    fn enqueue(id: &str, ticket: &str, at: &str) -> Value {
        json!({"type": "queue-operation", "operation": "enqueue", "timestamp": at, "content": msg(id, ticket)})
    }
    fn dequeue(at: &str) -> Value {
        json!({"type": "queue-operation", "operation": "dequeue", "timestamp": at})
    }
    fn remove(id: &str, ticket: &str, at: &str) -> Value {
        json!({"type": "queue-operation", "operation": "remove", "timestamp": at, "content": msg(id, ticket)})
    }
    fn prompt(id: &str, ticket: &str, at: &str) -> Value {
        json!({"type": "user", "isMeta": true, "origin": {"kind": "channel"}, "timestamp": at, "message": {"role": "user", "content": msg(id, ticket)}})
    }
    fn tool_use(at: &str) -> Value {
        json!({"type": "assistant", "timestamp": at, "message": {"role": "assistant", "content": [{"type": "tool_use", "id": "toolu_1", "name": "Bash", "input": {"command": "true"}}]}})
    }
    fn tool_result(at: &str) -> Value {
        json!({"type": "user", "timestamp": at, "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "toolu_1", "content": "ok"}]}})
    }
    fn said(text: &str, at: &str) -> Value {
        json!({"type": "assistant", "timestamp": at, "message": {"role": "assistant", "stop_reason": "end_turn", "content": [{"type": "text", "text": text}]}})
    }
    fn stop(at: &str) -> Value {
        json!({"type": "system", "subtype": "stop_hook_summary", "timestamp": at})
    }
    fn interrupt(at: &str) -> Value {
        json!({"type": "user", "timestamp": at, "message": {"role": "user", "content": [{"type": "text", "text": "[Request interrupted by user]"}]}})
    }

    impl T {
        /// lead, w2 under it, w3: w2 idle, its transcript empty and followed from its start.
        fn new(name: &str) -> T {
            let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
            let coord = std::env::temp_dir().join(format!("docket-delivered-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&coord);
            std::fs::create_dir_all(&coord).unwrap();
            let reg: String = [("AGT-001", "lead", "owner"), ("AGT-002", "w2", "lead"), ("AGT-003", "w3", "owner")].iter().map(|(id, n, p)| format!("{}\n", json!({"ev": "add", "id": id, "name": n, "parent": p, "kind": "claude", "model": "m", "at": "x"}))).collect();
            std::fs::write(coord.join("registry.jsonl"), reg).unwrap();
            let mut core = Core::load(&coord.display().to_string(), &base);
            core.load_tickets();
            core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
            let transcript = coord.join("w2.jsonl");
            std::fs::write(&transcript, "").unwrap();
            let f = crate::follow::Follower::new(&transcript.display().to_string());
            T { core, coord, transcript, f }
        }
        /// A thread of w3's to w2 with TEXT: its ticket and label.
        fn open(&mut self, text: &str) -> (u64, String) {
            self.core.ticket_input(docket_machine::Input::Open { kind: "thread".into(), under: None, opener: "w3".into(), form: json!({"to": "w3", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "w3"}, "follows": null}), text: Some(docket_machine::Text::New(text.into())), names: vec!["w2".into()], told: vec![] }).unwrap();
            let t = *self.core.tickets.m.tickets.keys().filter(|t| **t < docket_machine::HIDDEN).max().unwrap();
            (t, self.core.tickets.m.id(t))
        }
        /// What is due to w2 on T, handed to its session and taken (the channel's answer): its id.
        fn hand(&mut self, t: u64, into: bool) -> String {
            let d = self.core.ticket_due("w2").into_iter().find(|d| d.t == t).expect("due");
            self.core.tickets.handed.insert(("w2".into(), d.id.clone()), (d.t, d.upto, Default::default()));
            if into {
                self.core.tickets.into.insert(("w2".into(), d.id.clone()));
            }
            self.core.ticket_handed("w2", std::slice::from_ref(&d.id), true);
            d.id
        }
        fn write(&self, lines: &[Value]) {
            use std::io::Write;
            let mut w = std::fs::OpenOptions::new().append(true).open(&self.transcript).unwrap();
            for l in lines {
                writeln!(w, "{l}").unwrap();
            }
        }
        /// One read as `main::follow` takes it, and the turn's begin or end the follower shows: what each turn that
        /// ended in it did put in place first (insight (1)), as main.rs's loop does before the ends are taken.
        fn read(&mut self) {
            let r = self.f.read();
            let path = self.f.path.clone();
            let outputs: Vec<(String, u64, u64)> = self.f.take_outputs().into_iter().map(|(o, l)| (path.clone(), o, l)).collect();
            let worked: Vec<(String, u64, docket_machine::Worked)> = self.f.take_worked().into_iter().map(|(e, w)| (path.clone(), e, w)).collect();
            self.core.tickets.worked.entry("w2".into()).or_default().extend(worked);
            let at = r.at.into_iter().map(|(id, o, l)| (id, path.clone(), o, l)).collect();
            let entered = r.entered.into_iter().map(|e| (e, path.clone())).collect();
            self.core.transcript_read_queued("w2", at, Vec::new(), Vec::new(), &outputs, r.cut, &r.begins, r.running_old, entered, &r.queued);
            self.core.agents.get_mut("w2").unwrap().live.queue = self.f.queued_now().into_iter().map(|(o, _)| (path.clone(), o)).collect();
        }
        /// w2's posts on T, as text.
        fn posts(&self, t: u64) -> Vec<String> {
            self.core.tickets.m.tickets[&t].posts.iter().filter(|p| p.by == "w2").filter_map(|p| crate::label::read(&p.text)).collect()
        }
        /// When the giving of ID to w2 at its Nth giving was delivered.
        fn delivered(&self, t: u64, id: &str, nth: usize) -> Option<String> {
            self.core.tickets.m.tickets[&t].given.iter().filter(|g| g.to == "w2" && g.delivery == id).nth(nth).and_then(|g| g.delivered.clone())
        }
        fn log(&self) -> String {
            std::fs::read_to_string(self.coord.join("tickets/log.jsonl")).unwrap_or_default()
        }
    }
    impl Drop for T {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.coord);
        }
    }

    /// The 194 shape (research's sessions, 8 to 20 Nov: 194 deliveries queued while a turn's last message was written,
    /// taken by today's rule for the turn that never saw them): B is handed as A's turn writes its last message, the turn
    /// ends, and B is dequeued as the next prompt. A's turn answers A only; the turn B began answers B, on B's thread;
    /// B is delivered at its prompt, not at its enqueue. The record replays to the same.
    #[test]
    fn a_delivery_queued_as_a_turn_ends_is_answered_by_the_turn_it_begins_not_the_one_ending() {
        let mut x = T::new("queued-end");
        let (a, ta) = x.open("measure the drag");
        let a_id = x.hand(a, false);
        x.core.hook("w2", "prompt", &json!({}));
        x.core.begin_turn("w2");
        x.write(&[enqueue(&a_id, &ta, "2026-11-19T13:41:00.000Z"), dequeue("2026-11-19T13:41:00.010Z"), prompt(&a_id, &ta, "2026-11-19T13:41:00.050Z"), tool_use("2026-11-19T13:41:10.000Z"), tool_result("2026-11-19T13:41:11.000Z")]);
        x.read();
        // B is handed while the last message is being written
        let (b, tb) = x.open("the second pane is narrow");
        let b_id = x.hand(b, false);
        x.write(&[enqueue(&b_id, &tb, "2026-11-19T13:42:40.000Z")]);
        x.read();
        assert!(x.core.ticket_given_since("w2").contains(&b_id), "given");
        assert_eq!(x.delivered(b, &b_id, 0), None, "given, not delivered: still queued");
        // A's turn ends; B is its next prompt, in one read
        x.core.hook("w2", "stop", &json!({}));
        x.write(&[said("the drag measures 3 px", "2026-11-19T13:42:50.000Z"), stop("2026-11-19T13:42:50.100Z"), dequeue("2026-11-19T13:42:50.200Z"), prompt(&b_id, &tb, "2026-11-19T13:42:51.230Z")]);
        x.read();
        assert_eq!(x.delivered(b, &b_id, 0).as_deref(), Some("2026-11-19T13:42:51Z"), "delivered at its prompt");
        assert_eq!(x.delivered(a, &a_id, 0).as_deref(), Some("2026-11-19T13:41:00Z"));
        assert!(x.core.ticket_given_since("w2").contains(&b_id), "the turn B began holds it");
        x.write(&[said("the pane is 40 px narrow", "2026-11-19T13:43:20.000Z"), stop("2026-11-19T13:43:20.100Z")]);
        x.read();
        assert_eq!(x.posts(a), vec!["the drag measures 3 px".to_string()], "A's turn answers A alone");
        assert_eq!(x.posts(b), vec!["the pane is 40 px narrow".to_string()], "the turn B began answers B on its thread");
        let log = x.log();
        assert_eq!(log.matches("\"name\":\"delivered\"").count(), 2, "one fact per giving: {log}");
        // the record replays to the same
        let (posts, given): (Vec<_>, Vec<_>) = (x.core.tickets.m.tickets[&b].posts.clone(), x.core.tickets.m.tickets[&b].given.clone());
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let mut again = Core::load(&x.coord.display().to_string(), &base);
        again.load_tickets();
        assert!(again.tickets.errors.is_empty(), "{:?}", again.tickets.errors);
        assert_eq!((again.tickets.m.tickets[&b].posts.clone(), again.tickets.m.tickets[&b].given.clone()), (posts, given));
    }

    /// Insight (1)'s order at a turn's end (proto-lead N-1446 p546; consult-3's trace, p543): the Stop hook comes first
    /// and takes nothing; the follower's read then finds the transcript's stop, puts the turn's tally in place and takes
    /// its end, so the turn's `turn_ended` line carries its tools.
    #[test]
    fn a_turns_tools_reach_its_line_when_the_stop_hook_comes_before_the_read() {
        let mut x = T::new("worked-order");
        let (a, ta) = x.open("measure the drag");
        let a_id = x.hand(a, false);
        x.core.hook("w2", "prompt", &json!({}));
        x.core.begin_turn("w2");
        x.write(&[enqueue(&a_id, &ta, "2026-11-19T13:41:00.000Z"), dequeue("2026-11-19T13:41:00.010Z"), prompt(&a_id, &ta, "2026-11-19T13:41:00.050Z"), tool_use("2026-11-19T13:41:10.000Z"), tool_result("2026-11-19T13:41:11.000Z")]);
        x.read();
        // the Stop hook first: no turn's end is taken by it
        x.core.hook("w2", "stop", &json!({}));
        let ended = |x: &T| x.log().lines().map(|l| serde_json::from_str::<Value>(l).unwrap()).filter(|v| v["ev"] == "turn_ended").collect::<Vec<_>>();
        assert_eq!(ended(&x), Vec::<Value>::new(), "the hook alone takes nothing");
        // then the read of the transcript's stop
        x.write(&[said("the drag measures 3 px", "2026-11-19T13:42:50.000Z"), stop("2026-11-19T13:42:50.100Z")]);
        x.read();
        let lines = ended(&x);
        assert_eq!(lines.iter().map(|v| (v["delivery"].as_str(), v["tools"].clone())).collect::<Vec<_>>(), vec![(Some(a_id.as_str()), json!({"Bash": 1}))], "{lines:?}");
        assert_eq!(x.core.tickets.worked.get("w2").map(Vec::len), Some(0), "its tally taken");
    }

    /// The converse: B removed into the running turn at a tool result before its last message entered that turn, so the
    /// turn ending answers it.
    #[test]
    fn a_delivery_removed_into_a_turn_before_its_last_message_is_that_turns() {
        let mut x = T::new("removed");
        x.core.hook("w2", "prompt", &json!({}));
        x.core.begin_turn("w2");
        // a turn its hooks began, working: words typed into the terminal would be his post, so none here
        x.write(&[tool_use("2026-11-19T13:41:10.000Z")]);
        x.read();
        let (b, tb) = x.open("the second pane is narrow");
        let b_id = x.hand(b, false);
        x.write(&[enqueue(&b_id, &tb, "2026-11-19T13:41:20.000Z"), tool_result("2026-11-19T13:41:40.000Z"), remove(&b_id, &tb, "2026-11-19T13:41:40.100Z")]);
        x.read();
        assert_eq!(x.delivered(b, &b_id, 0).as_deref(), Some("2026-11-19T13:41:40Z"), "delivered at its remove");
        x.core.hook("w2", "stop", &json!({}));
        x.write(&[said("the pane is 40 px narrow", "2026-11-19T13:42:50.000Z"), stop("2026-11-19T13:42:50.100Z")]);
        x.read();
        assert_eq!(x.posts(b), vec!["the pane is 40 px narrow".to_string()], "the turn it entered answers it");
        assert!(x.core.ticket_given_since("w2").is_empty());
    }

    /// The esc shape (consult-3, N-1397 post 6, 2): B queued in A's turn, the owner interrupts, and the session dequeues
    /// B and writes it after the interrupt as the next turn's prompt. The cut forgets what A's turn was given; the turn B
    /// began holds B all the same, and its reply goes on B's thread. A's turn, cut, posts nothing.
    #[test]
    fn a_delivery_queued_at_an_esc_is_answered_by_the_turn_it_begins_after_the_cut() {
        let mut x = T::new("esc");
        let (a, ta) = x.open("measure the drag");
        let a_id = x.hand(a, false);
        x.core.hook("w2", "prompt", &json!({}));
        x.core.begin_turn("w2");
        x.write(&[enqueue(&a_id, &ta, "2026-11-16T07:53:40.000Z"), dequeue("2026-11-16T07:53:40.010Z"), prompt(&a_id, &ta, "2026-11-16T07:53:40.050Z"), tool_use("2026-11-16T07:53:50.000Z"), tool_result("2026-11-16T07:53:56.000Z")]);
        x.read();
        let (b, tb) = x.open("and why the first test");
        let b_id = x.hand(b, true);
        x.write(&[enqueue(&b_id, &tb, "2026-11-16T07:53:59.253Z")]);
        x.read();
        x.write(&[dequeue("2026-11-16T07:54:13.864Z"), interrupt("2026-11-16T07:54:13.851Z"), prompt(&b_id, &tb, "2026-11-16T07:54:13.881Z")]);
        x.read();
        assert_eq!(x.delivered(b, &b_id, 0).as_deref(), Some("2026-11-16T07:54:13Z"), "delivered after the interrupt");
        assert_eq!(x.core.ticket_given_since("w2"), vec![b_id.clone()], "the cut forgot A's; the turn B began holds B");
        x.core.hook("w2", "stop", &json!({}));
        x.write(&[said("the first test is the drag one", "2026-11-16T07:54:40.000Z"), stop("2026-11-16T07:54:40.100Z")]);
        x.read();
        assert_eq!(x.posts(b), vec!["the first test is the drag one".to_string()], "B's turn's reply on B's thread");
        assert!(x.posts(a).is_empty(), "A's turn was cut");
    }

    /// A Codex thread with no turn yet (New agent with no first words, N-1997): Codex's SessionStart comes only with a
    /// first turn, so its start waits until roster's `coord session` says `ready`, then takes what is due at once
    #[test]
    fn a_session_said_ready_takes_what_is_due_without_its_start_hook() {
        let mut x = T::new("ready");
        let state = |x: &T| x.core.agent_state("w2").unwrap();
        x.core.agents.get_mut("w2").unwrap().live.awaiting_start = Some(std::time::Instant::now());
        assert!(x.core.session(&Who::Owner, "w2", &json!({"session_id": "t-1", "cwd": "/w"})).ok);
        assert_eq!(state(&x), AgentState::Starting { why: "its new process is starting".into() });
        assert!(x.core.session(&Who::Owner, "w2", &json!({"session_id": "t-1", "cwd": "/w", "ready": true})).ok);
        assert_eq!(state(&x), AgentState::Idle);
        assert!(!x.core.agents["w2"].session.as_object().unwrap().contains_key("ready"), "no fact of its session");
    }

    /// What w2 can take now, the one answer docket-core2's delivery events read (N-1397 post 1, 2): not running, idle,
    /// in a turn, compacting (from its hook to its transcript's boundary), starting (a restart due, a new process before
    /// its start hook, an esc stopping its turn, its turn's end not yet in its session, a move), ended, and every
    /// account out.
    #[test]
    fn an_agents_state_is_one_answer_from_its_facts() {
        let mut x = T::new("state");
        let state = |x: &T| x.core.agent_state("w2").unwrap();
        assert_eq!(state(&x), AgentState::Idle);
        x.core.hook("w2", "prompt", &json!({}));
        x.core.begin_turn("w2");
        assert_eq!(state(&x), AgentState::Working);
        x.core.hook("w2", "compact", &json!({}));
        let since = x.core.agents["w2"].live.compacting_since.clone();
        assert!(since.is_some());
        assert_eq!(state(&x), AgentState::Compacting { since });
        // its boundary in the transcript ends it, the hook's SessionStart missed
        x.write(&[json!({"type": "system", "subtype": "compact_boundary", "timestamp": "2026-11-20T09:10:00.000Z", "compactMetadata": {"durationMs": 1000}})]);
        let r = x.f.read();
        assert!(r.compacted);
        x.core.compaction_over("w2");
        assert_eq!(state(&x), AgentState::Working);
        x.core.agents.get_mut("w2").unwrap().live.effort_due = Some("fable:high".into());
        assert_eq!(state(&x), AgentState::Starting { why: "restarting on fable:high".into() });
        x.core.agents.get_mut("w2").unwrap().live.effort_due = None;
        // the owner's Safe · Yolo waits for a restart as an effort does, named by the mode he asked
        let w2 = x.core.agents.get_mut("w2").unwrap();
        (w2.yolo, w2.live.yolo_due) = (true, true);
        assert!(x.core.agents["w2"].live.restart_due());
        assert_eq!(state(&x), AgentState::Starting { why: "restarting into Yolo".into() });
        x.core.agents.get_mut("w2").unwrap().live.yolo_due = false;
        x.core.agents.get_mut("w2").unwrap().live.awaiting_start = Some(std::time::Instant::now());
        assert!(matches!(state(&x), AgentState::Starting { .. }));
        x.core.agents.get_mut("w2").unwrap().live.awaiting_start = None;
        // an esc stopping its turn, its turn's end taken before its session shows it, a move to another view: each
        // holds what it is handed, as a start does (docket-core2, N-807.p127)
        x.core.tickets.stopping.insert("w2".into());
        assert_eq!(state(&x), AgentState::Starting { why: "its turn is being stopped".into() });
        x.core.tickets.stopping.clear();
        x.core.agents.get_mut("w2").unwrap().output_owed = Some(std::time::Instant::now());
        assert_eq!(state(&x), AgentState::Starting { why: "its turn's last message is not yet in its session".into() });
        x.core.agents.get_mut("w2").unwrap().output_owed = None;
        x.core.moving.insert("w2".into());
        assert_eq!(state(&x), AgentState::Starting { why: "moving to another view".into() });
        x.core.moving.clear();
        assert_eq!(state(&x), AgentState::Working);
        x.core.agents.get_mut("w2").unwrap().live = Live::default();
        assert_eq!(state(&x), AgentState::Stopped { why: "not running".into() });
        x.core.agents.get_mut("w2").unwrap().account_wait = Some(1_763_640_000);
        assert_eq!(state(&x), AgentState::OutOfLimit { until: Some(1_763_640_000) });
        x.core.agents.get_mut("w2").unwrap().ended = Some("2026-11-20T09:11:00Z".into());
        assert_eq!(state(&x), AgentState::Stopped { why: "ended by the owner".into() });
        assert_eq!(serde_json::to_value(AgentState::Compacting { since: Some("2026-11-20T09:09:00Z".into()) }).unwrap(), json!({"state": "compacting", "since": "2026-11-20T09:09:00Z"}));
        assert_eq!(x.core.agent_state("nobody"), None);
    }

    /// Condition 1 (consult-3, N-1397 post 6): the same id given twice, the first giving delivered, then coordd restarts
    /// between the second enqueue and its remove. The first read's window holds the old enqueue and remove: they are the
    /// first giving's, which has its fact already, so nothing is written for the second. Its own remove writes it once.
    #[test]
    fn a_restart_between_a_second_givings_enqueue_and_remove_marks_it_delivered_only_at_its_own_remove() {
        let mut x = T::new("twice");
        let (d, td) = x.open("is the cable in");
        let id = x.hand(d, false);
        x.core.hook("w2", "prompt", &json!({}));
        x.core.begin_turn("w2");
        x.write(&[tool_use("2026-11-20T07:59:30.000Z"), enqueue(&id, &td, "2026-11-20T08:00:00.000Z"), tool_result("2026-11-20T08:00:05.000Z"), remove(&id, &td, "2026-11-20T08:00:05.100Z")]);
        x.read();
        assert_eq!(x.delivered(d, &id, 0).as_deref(), Some("2026-11-20T08:00:05Z"));
        // made due again with no turn's end between (today's rule 19, which head 2 makes read this fact), and handed again
        x.core.ticket_unread("w2");
        let again = x.hand(d, false);
        assert_eq!(again, id, "the same id");
        x.write(&[enqueue(&id, &td, "2026-11-20T09:00:00.000Z")]);
        x.read();
        assert_eq!(x.delivered(d, &id, 1), None, "the second giving: queued");
        // coordd restarts: the record replayed, a new follower over the whole file
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        x.core = Core::load(&x.coord.display().to_string(), &base);
        x.core.load_tickets();
        x.core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "working".into(), ..Default::default() };
        x.f = crate::follow::Follower::new(&x.transcript.display().to_string());
        x.read();
        assert_eq!(x.delivered(d, &id, 0).as_deref(), Some("2026-11-20T08:00:05Z"));
        assert_eq!(x.delivered(d, &id, 1), None, "the old remove is not the second giving's");
        assert_eq!(x.log().matches("\"name\":\"delivered\"").count(), 1);
        x.write(&[tool_result("2026-11-20T09:05:00.000Z"), remove(&id, &td, "2026-11-20T09:05:00.100Z")]);
        x.read();
        assert_eq!(x.delivered(d, &id, 1).as_deref(), Some("2026-11-20T09:05:00Z"), "its own remove");
        assert_eq!(x.log().matches("\"name\":\"delivered\"").count(), 2, "once each");
    }

    /// The lines rule 19 wrote for w2: its `unread` lines and its `due_again` events (exited or not).
    fn due_lines(x: &T) -> (usize, Vec<(Vec<String>, bool)>) {
        let log = x.log();
        let lines: Vec<Value> = log.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let unread = lines.iter().filter(|l| l["ev"] == "unread" && l["agent"] == "w2").count();
        let due = lines.iter().filter(|l| l["ev"] == "event" && l["name"] == docket_machine::DUE_AGAIN).map(|l| (l["value"]["deliveries"].as_array().unwrap().iter().map(|d| d.as_str().unwrap().to_string()).collect(), l["value"]["exited"] == true)).collect();
        (unread, due)
    }

    /// Rule 19 on the session's facts (0.2 A4, N-1397 post 5, 3), for an idle agent: a giving its session still holds
    /// in its queue waits, past any minute; one never seen in the session is due again after the minute that bounds
    /// Claude Code writing its enqueue, alone (the queued one stays given); one seen there that left the queue without
    /// entering is due again at once. The record replays to the same.
    #[test]
    fn an_idle_agents_giving_waits_while_its_session_queues_it_and_is_due_again_once_it_is_lost() {
        let mut x = T::new("rule19");
        let (a, ta) = x.open("measure the drag");
        let a_id = x.hand(a, false);
        x.write(&[enqueue(&a_id, &ta, "2026-11-20T09:20:00.000Z")]);
        x.read();
        assert_eq!(x.core.agents["w2"].live.queue.len(), 1, "queued in its session");
        let a_minute_later = |x: &mut T| {
            x.core.not_read();
            x.core.idle_given.insert("w2".into(), std::time::Instant::now() - Duration::from_secs(61));
            x.core.not_read();
        };
        a_minute_later(&mut x);
        assert_eq!(x.core.ticket_given_since("w2"), vec![a_id.clone()], "still queued: it waits");
        assert_eq!(due_lines(&x), (0, Vec::new()));
        // B: its channel took it and no line of the session came
        let (b, _) = x.open("the pane is narrow");
        let b_id = x.hand(b, false);
        x.core.not_read();
        assert!(x.core.ticket_given_since("w2").contains(&b_id), "within the minute");
        a_minute_later(&mut x);
        assert_eq!(x.core.ticket_given_since("w2"), vec![a_id.clone()], "B due again; A, queued, stays given");
        assert_eq!(due_lines(&x), (0, vec![(vec![b_id.clone()], false)]));
        assert!(x.core.ticket_due("w2").iter().any(|d| d.id == b_id));
        // the session's queue holds A no more, and A never entered: due again at once, all it was given
        x.core.agents.get_mut("w2").unwrap().live.queue.clear();
        x.core.not_read();
        assert!(x.core.ticket_given_since("w2").is_empty());
        assert_eq!(due_lines(&x).0, 1, "the unread line, as before: {}", x.log());
        assert!(x.core.ticket_due("w2").iter().any(|d| d.id == a_id));
        let mut again = Core::load(&x.coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
        again.load_tickets();
        let rows = |c: &Core, t: u64| c.tickets.m.tickets[&t].given.iter().map(|g| (g.delivery.clone(), g.unread, g.lost)).collect::<Vec<_>>();
        assert_eq!((rows(&again, a), rows(&again, b)), (rows(&x.core, a), rows(&x.core, b)));
    }

    /// 0.2 line 22 on rule 19's facts: a giving never seen in w2's session is a miss of the account w2 runs on, and w2
    /// restarts on it; a second of that same session (its restart did not take) is not counted, one after w2's new
    /// process has started, with none seen between, makes the account deaf, so w2 is due to move (accounts/mover.rs);
    /// while a dialog stands nothing is judged; a giving seen in the session, its enqueue, makes the account heard again.
    #[test]
    fn givings_never_seen_make_the_account_deaf_and_one_seen_heard_and_a_dialog_judges_nothing() {
        let mut x = T::new("deaf");
        let acc = crate::accounts::testing::Tmp::new("core-deaf");
        let h = crate::accounts::Harness::Claude;
        acc.store.create(h, "seat", "Seat", 30, crate::accounts::Kind::Signin, "x").unwrap();
        acc.store.update(h, "seat", |s| (s.signed_in, s.heard) = (true, None)).unwrap();
        x.core.accounts = acc.store.clone();
        let w2 = x.core.agents.get_mut("w2").unwrap();
        (w2.native, w2.account) = (true, Some("seat".into()));
        let state = || acc.store.state(h, "seat").unwrap();
        let a_minute_later = |x: &mut T| {
            x.core.not_read();
            x.core.idle_given.insert("w2".into(), std::time::Instant::now() - Duration::from_secs(61));
            x.core.not_read();
        };
        // a dialog stands on w2's screen: the minute does not count, and the seat is not judged
        let (a, _) = x.open("measure the drag");
        let a_id = x.hand(a, false);
        let now = std::time::Instant::now();
        x.core.agents.get_mut("w2").unwrap().live.dialog = Some((now, "Do you trust the files in this folder?".into(), now));
        a_minute_later(&mut x);
        assert!(state().missed.is_none() && x.core.ticket_given_since("w2").contains(&a_id));
        // gone: a minute later A is due again, and the seat has a miss, w2 still on it, to restart on it
        x.core.agents.get_mut("w2").unwrap().live.dialog = None;
        a_minute_later(&mut x);
        assert_eq!(state().missed.map(|m| m.agent).as_deref(), Some("w2"));
        assert!(state().can_take(crate::accounts::mover::now_s()));
        assert!(x.core.agents["w2"].live.account_due.as_deref().unwrap().contains("never seen in its session"), "{:?}", x.core.agents["w2"].live.account_due);
        // its restart does not take (the same process still runs: the mark goes, and A is handed to that session again),
        // and it never sees A again: both misses may be the one session's, the seat is not deaf
        let pid = x.core.agents["w2"].live.pid;
        x.core.restarted("w2", pid, "made-up: still the same process");
        assert!(x.core.agents["w2"].live.account_due.is_none() && x.core.agents["w2"].live.unseen_on.as_deref() == Some("seat"));
        assert_eq!(x.hand(a, false), a_id);
        a_minute_later(&mut x);
        assert!(state().deaf.is_none() && state().missed.is_some());
        // given twice and not read, A waits for w2's next process (rule 19); the old one ends and a new one starts on the
        // seat, and handed A again, it never sees it either: the seat is deaf, and w2 is to move
        x.core.exited("w2", pid.unwrap());
        x.core.new_process("w2", 4242, false);
        x.core.agents.get_mut("w2").unwrap().live.awaiting_start = None;
        // (its delivery is named past coordd's post now, as rule 19 hands it after a process's end)
        x.hand(a, false);
        a_minute_later(&mut x);
        assert_eq!(state().deaf.map(|d| d.agent).as_deref(), Some("w2"));
        assert!(x.core.agents["w2"].live.account_due.as_deref().unwrap().contains("does not take messages"), "{:?}", x.core.agents["w2"].live.account_due);
        // B is seen in w2's session after all, its enqueue: the seat takes messages
        let (b, tb) = x.open("the pane is narrow");
        let b_id = x.hand(b, false);
        x.core.hook("w2", "prompt", &json!({}));
        x.core.begin_turn("w2");
        x.write(&[enqueue(&b_id, &tb, "2026-11-20T10:00:00.000Z"), dequeue("2026-11-20T10:00:00.010Z"), prompt(&b_id, &tb, "2026-11-20T10:00:00.050Z")]);
        x.read();
        assert!(x.delivered(b, &b_id, 0).is_some());
        let s = state();
        assert!(s.heard.is_some() && s.missed.is_none() && s.deaf.is_none(), "{s:?}");
    }

    /// A process that ends (a restart, an account move, a crash) takes its queue with it (0.2 A4: 27 messages still
    /// queued when their session files ended, in one day): a giving that never entered is due again, to the next
    /// process; one that entered was read. Ended so twice, it is handed again each time, never "given twice, not read".
    #[test]
    fn a_process_that_ends_with_a_giving_queued_has_it_due_again_and_one_that_entered_read() {
        let mut x = T::new("exited");
        let (a, ta) = x.open("measure the drag");
        let a_id = x.hand(a, false);
        x.core.hook("w2", "prompt", &json!({}));
        x.core.begin_turn("w2");
        x.write(&[enqueue(&a_id, &ta, "2026-11-20T09:30:00.000Z"), dequeue("2026-11-20T09:30:00.010Z"), prompt(&a_id, &ta, "2026-11-20T09:30:00.050Z"), tool_use("2026-11-20T09:30:05.000Z")]);
        x.read();
        let (b, tb) = x.open("the pane is narrow");
        let b_id = x.hand(b, true);
        x.write(&[enqueue(&b_id, &tb, "2026-11-20T09:30:06.000Z")]);
        x.read();
        assert!(x.delivered(a, &a_id, 0).is_some() && x.delivered(b, &b_id, 0).is_none(), "A entered, B queued");
        // the process ends mid-turn, B still queued
        x.core.exited("w2", std::process::id() as i32);
        assert_eq!(due_lines(&x), (0, vec![(vec![b_id.clone()], true)]), "{}", x.log());
        let due: Vec<String> = x.core.ticket_due("w2").into_iter().map(|d| d.id).collect();
        assert!(due.contains(&b_id) && !due.contains(&a_id), "B due again, A read: {due:?}");
        // the next process is handed B, and ends before B entered
        x.core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        assert_eq!(x.hand(b, false), b_id);
        x.write(&[enqueue(&b_id, &tb, "2026-11-20T09:31:00.000Z")]);
        x.read();
        x.core.exited("w2", std::process::id() as i32);
        assert_eq!(due_lines(&x).1.len(), 2);
        assert!(x.core.ticket_due("w2").iter().any(|d| d.id == b_id), "due again a second time");
        assert!(!x.core.tickets.m.tickets[&b].posts.iter().any(|p| p.by == "coordd"), "never given twice, not read");
    }


    /// His post never seen by w2, twice (consult-3 N-818.p76, 2; 0.2 line 4, "nothing withdrawn or lost"): not left
    /// behind. Held until w2's next turn's end or start, the line under it saying not read and nothing posted for anyone
    /// to see; w2's process ends and it is due again, to the next one.
    #[test]
    fn his_post_never_seen_twice_waits_for_the_agents_next_start_and_is_handed_again() {
        let mut x = T::new("missed");
        x.core.ticket_input(docket_machine::Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(docket_machine::Text::New("are you there?".into())), names: vec!["w2".into()], told: vec![] }).unwrap();
        let t = *x.core.tickets.m.tickets.keys().filter(|t| **t < docket_machine::HIDDEN).max().unwrap();
        let a_minute_later = |x: &mut T| {
            x.core.not_read();
            x.core.idle_given.insert("w2".into(), std::time::Instant::now() - Duration::from_secs(61));
            x.core.not_read();
        };
        let id = x.hand(t, false);
        a_minute_later(&mut x);
        assert_eq!(x.hand(t, false), id, "handed again once");
        a_minute_later(&mut x);
        assert!(x.core.ticket_due("w2").is_empty(), "held: not handed a third time now");
        let v = x.core.tickets.m.view(t).unwrap();
        assert!(v.posts.iter().all(|p| p.by != "coordd"), "nothing posted for anyone to see");
        assert_eq!(v.given.iter().filter(|g| g.to == "w2").map(|g| g.unread).collect::<Vec<_>>(), vec![true, true], "the line under it: not read");
        // w2's process ends: due again, to the next one (its delivery is named past coordd's post, shown to nobody)
        x.core.exited("w2", std::process::id() as i32);
        assert!(x.core.ticket_due("w2").iter().any(|d| d.t == t), "{}", x.log());
        let mut again = Core::load(&x.coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
        again.load_tickets();
        assert!(again.ticket_due("w2").iter().any(|d| d.t == t), "the record replays to the same");
    }


    /// The dialog hold ends when the screen no longer shows it (consult-3 N-818.p78, 1): answering a dialog need not
    /// write a line, so while the hold is on its screen is read again each minute, off the lock (`Job::DialogLooked`);
    /// once the title is gone the minute counts, and a giving never seen is due again with no line of the transcript.
    #[test]
    fn a_dialog_hold_is_read_again_each_minute_and_ends_once_the_screen_no_longer_shows_it() {
        let mut x = T::new("dialog");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        x.core.jobs = Some(tx);
        let (a, _) = x.open("measure the drag");
        let a_id = x.hand(a, false);
        let title = "Do you want to proceed?";
        x.core.popup("w2", title, "Do you want to proceed?\n❯ 1. Yes\n  2. No");
        // the window is told (N-1863 p3): the agent's view carries the dialog, sent as its event
        let sent = |x: &T| x.core.since(0).unwrap_or_default().iter().rev().find_map(|e| match &e.body {
            coord_types::Body::Agent(v) if v.name == "w2" => Some(v.dialog.clone()),
            _ => None,
        });
        assert_eq!((x.core.view("w2").dialog.as_deref(), sent(&x)), (Some(title), Some(Some(title.to_string()))));
        let looked = |rx: &mut tokio::sync::mpsc::UnboundedReceiver<Job>| {
            let mut out = Vec::new();
            while let Ok(j) = rx.try_recv() {
                if let Job::DialogLooked { name, title } = j {
                    out.push((name, title));
                }
            }
            out
        };
        let a_minute_later = |x: &mut T| {
            x.core.not_read();
            x.core.idle_given.insert("w2".into(), std::time::Instant::now() - Duration::from_secs(61));
            x.core.not_read();
        };
        // the minute does not count while it stands
        a_minute_later(&mut x);
        assert_eq!(x.core.ticket_given_since("w2"), vec![a_id.clone()], "a dialog stands: it waits");
        assert!(looked(&mut rx).is_empty(), "read again only a minute after it was told");
        // a minute after: its screen is read again, once a minute
        x.core.agents.get_mut("w2").unwrap().live.dialog.as_mut().unwrap().2 = std::time::Instant::now() - Duration::from_secs(61);
        x.core.not_read();
        assert_eq!(looked(&mut rx), vec![("w2".to_string(), title.to_string())]);
        x.core.not_read();
        assert!(looked(&mut rx).is_empty(), "once a minute");
        // another title's answer leaves it standing; this one's ends it, and the minute runs
        x.core.dialog_gone("w2", "Allow this edit?");
        assert!(x.core.agents["w2"].live.dialog.is_some());
        x.core.dialog_gone("w2", title);
        assert_eq!((x.core.view("w2").dialog, sent(&x)), (None, Some(None)), "gone: the window told so");
        a_minute_later(&mut x);
        assert!(x.core.ticket_given_since("w2").is_empty(), "{}", x.log());
        assert!(x.core.ticket_due("w2").iter().any(|d| d.id == a_id));
    }

}

#[cfg(test)]
mod runner_jobs_tests {
    use super::*;
    use crate::jobs::{Machine, Read};
    use coord_types::Job;

    fn t(hm: &str) -> String {
        format!("2026-11-20T{hm}:00Z")
    }

    /// A job of w2's, `test yaaihh · edge5-typing ec7f09cb`, begun at SINCE, at STEP since STEP_SINCE.
    fn job(id: &str, step: &str, since: &str, step_since: &str) -> Job {
        Job { id: id.into(), verb: "test".into(), packages: vec!["yaaihh".into()], branch: Some("edge5-typing".into()), commit: Some("ec7f09cb".into()), since: t(since), step: step.into(), step_since: t(step_since), ..Default::default() }
    }

    fn read(coord: &str, job: Job, alive: bool) -> Read {
        Read { agent: "w2".into(), coord: coord.into(), alive, written: t("06:31"), job }
    }

    fn ids(core: &Core) -> Vec<(String, String, bool)> {
        core.view("w2").jobs.into_iter().map(|j| (j.id, j.step, j.fresh)).collect()
    }

    /// N-1334, as consult-lead ruled it (p2): w2's view has its running jobs newest first, then the last five that
    /// ended; one that ended after its last turn end is fresh until the next; a job waiting for its machine says
    /// `unreachable` while kpi-data's file says the machine does not answer, `sending` while it says it answers, and
    /// the runner's own step when no fresh file speaks of its machine (p2778); a job whose caller went with its
    /// session is `cut` and told at its next hand-over, one stopped while the session ran on ended with no rc, and
    /// stays so when the session restarts later (consult-3, p15); another coord's file is passed over; a view goes out
    /// only when the jobs changed.
    #[test]
    fn an_agents_jobs_stand_as_ruled_and_one_cut_with_its_session_is_told_until_its_next_turn_end() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-runner-jobs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        core.agents.get_mut("w2").unwrap().live = Live { pid: Some(std::process::id() as i32), state: "working".into(), started: t("06:00"), turn_ended: Some(t("06:10")), turn: 3, ..Default::default() };
        let c = core.coord.clone();
        let tower = |on| Some(Machine { name: "yaaihh-build-1".into(), on });
        let files = |r1_alive: bool, r3_alive: bool| {
            let mut v = vec![
                read(&c, Job { done: Some(41), of: Some(230), ..job("r1", "compiling", "06:20", "06:21") }, r1_alive),
                // the runner found the machine silent; kpi-data's file is the one source of that
                read(&c, Job { machine: Some("yaaihh-build-1".into()), ..job("r2", "unreachable", "06:25", "06:25") }, true),
                read(&c, Job { what: Some("yaaihh".into()), done: Some(3), of: Some(293), ..job("r3", "testing", "06:26", "06:27") }, r3_alive),
                read(&c, Job { rc: Some(101), passed: Some(292), failed: Some(1), ..job("e1", "ended", "06:11", "06:15") }, true),
                read("/elsewhere/research", job("x1", "compiling", "06:29", "06:29"), true),
            ];
            v.extend((0..6).map(|k| read(&c, Job { rc: Some(0), ..job(&format!("old{k}"), "ended", &format!("05:0{k}"), &format!("05:1{k}")) }, false)));
            v
        };
        let find = |core: &Core, id: &str| core.view("w2").jobs.into_iter().find(|j| j.id == id).unwrap();
        let mut rx = core.tx.subscribe();
        core.set_jobs(files(true, true), tower(true));
        let row = |id: &str, step: &str, fresh: bool| (id.to_string(), step.to_string(), fresh);
        assert_eq!(ids(&core), [row("r3", "testing", false), row("r2", "sending", false), row("r1", "compiling", false), row("e1", "ended", true), row("old5", "ended", false), row("old4", "ended", false), row("old3", "ended", false), row("old2", "ended", false)]);
        assert!(matches!(rx.try_recv(), Ok(Event { body: Body::Agent(a), .. }) if a.jobs.len() == 8));
        core.set_jobs(files(true, true), tower(true));
        assert!(rx.try_recv().is_err(), "nothing changed: no view sent");
        assert!(core.job_notices("w2").is_empty());
        // kpi-data's file says the tower does not answer: the job waiting for it says so
        core.set_jobs(files(true, true), tower(false));
        assert_eq!(find(&core, "r2").step, "unreachable");
        // no fresh file (p2778): the runner's own step stands, its `unreachable` as it wrote it, not `sending`
        core.set_jobs(files(true, true), None);
        assert_eq!(find(&core, "r2").step, "unreachable");
        // a file about another machine says nothing of this one's: the same
        core.set_jobs(files(true, true), Some(Machine { name: "other".into(), on: true }));
        assert_eq!(find(&core, "r2").step, "unreachable");
        core.set_jobs(files(true, true), tower(true));
        assert_eq!(find(&core, "r2").step, "sending");
        // r1's caller is gone while the session runs on (the agent stopped it): ended, no rc, at its file's last write
        core.set_jobs(files(false, true), tower(true));
        let r1 = find(&core, "r1");
        assert_eq!((r1.step.as_str(), r1.rc, r1.step_since.as_str(), r1.done, r1.fresh), ("ended", None, t("06:31").as_str(), None, true));
        assert!(core.job_notices("w2").is_empty(), "stopped by the agent itself: nothing to tell it");
        // the session restarts after both began, and r3's caller goes with it: r3 is cut at the restart and told at
        // the next hand-over; r1, stopped by the agent before, stays as it was decided
        core.new_process("w2", std::process::id() as i32, false);
        core.agents.get_mut("w2").unwrap().live.started = t("06:30");
        core.set_jobs(files(false, false), tower(true));
        let r1 = find(&core, "r1");
        assert_eq!((r1.step.as_str(), r1.step_since.as_str()), ("ended", t("06:31").as_str()), "not cut by a restart after the agent stopped it");
        let r3 = find(&core, "r3");
        assert_eq!((r3.step.as_str(), r3.step_since.as_str(), r3.fresh, r3.what), ("cut", t("06:30").as_str(), true, None), "the restart kept its last turn end");
        let said = format!("Your job `test yaaihh · edge5-typing ec7f09cb` was stopped at {} with your session; it did not finish.", crate::jobs::clock(&t("06:30")));
        assert_eq!(core.job_notices("w2"), [said], "r3 only");
        // its next turn's end: nothing fresh, nothing told
        core.agents.get_mut("w2").unwrap().live.turn = 4;
        assert!(core.end_turn("w2").is_some());
        core.set_jobs(files(false, false), tower(true));
        assert!(core.view("w2").jobs.iter().all(|j| !j.fresh) && core.job_notices("w2").is_empty());
        assert_eq!(core.runner_gone.len(), 2, "r1's and r3's decisions, kept while their files are there");
        // no file left: no jobs, and a view says so
        while rx.try_recv().is_ok() {}
        core.set_jobs(Vec::new(), None);
        assert!(core.view("w2").jobs.is_empty() && core.runner_gone.is_empty());
        assert!(matches!(rx.try_recv(), Ok(Event { body: Body::Agent(a), .. }) if a.jobs.is_empty()));
        let _ = std::fs::remove_dir_all(&coord);
    }
}
