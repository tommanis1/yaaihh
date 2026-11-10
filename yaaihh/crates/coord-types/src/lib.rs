//! The types coordd and its readers share (vault designs/yaaihh-2026-11-07.md §7): the requests on coordd's socket,
//! their replies, and the numbered events of `coord watch`. yaaihh reads the same stream, so these are defined once.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The socket coordd listens on inside the box.
pub const SOCKET: &str = "/run/coordd.sock";

/// One running tool call of an agent (from its pre hook until its post hook).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Tool {
    pub id: String,
    pub tool: String,
    pub what: String,
    pub at: String,
}

/// One entry of a session: a prompt, mail, the agent's reply, a tool call or its result.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    /// The byte offset of the transcript line it comes from (one line can give several entries).
    pub offset: u64,
    pub at: String,
    /// user, mail, assistant, tool, result.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    pub text: String,
    /// The change an edit made, as a unified diff (`--- PATH`, `+++ PATH`, hunks): on a Claude Code edit's
    /// result (its `structuredPatch`, or a new file's content) and on a Codex `apply_patch` call (its
    /// `patch_apply_end`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
    /// The images it shows (a pasted screenshot, a read image file's result), as their places among its line's
    /// images in reading order; each is fetched with [`Request::Image`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<u32>,
}

/// A page of a session: its entries, the offset of its first line, and the offset after its last whole line.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Transcript {
    pub entries: Vec<Entry>,
    pub start: u64,
    pub next: u64,
    /// The transcript the offsets are in: an agent's session moves to a new file when it resumes or compacts
    /// (8 Nov: the chat kept asking after byte 127M of the old file and showed nothing new).
    #[serde(default)]
    pub session: String,
}

/// An agent as readers see it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentView {
    /// The short reference, e.g. AGT-012.
    pub id: String,
    pub name: String,
    pub parent: String,
    /// Its place in its team, a word the policy's norms speak of (agent-base/policy/norms.eflint): `lead`, `senior`,
    /// `member` (also when empty), `consultant` (the owner's adviser beside the teams, owner 9 Nov).
    #[serde(default)]
    pub role: String,
    pub kind: String,
    pub model: String,
    /// working, idle, waiting on …, stopped.
    pub state: String,
    pub since: String,
    /// The last tool call's start (a turn that is working with an old one may hang).
    pub last_tool: Option<String>,
    pub tools: Vec<Tool>,
    /// The open items the agent said it works on.
    pub items: Vec<String>,
    pub transcript: Option<String>,
    /// The folder the agent's session runs in (its session record's `cwd`): what a relative path in its chat means.
    #[serde(default)]
    pub cwd: Option<String>,
    pub mem_mb: Option<u64>,
    pub cpu_s: Option<f64>,
    /// When the agent was first registered.
    #[serde(default)]
    pub created: String,
    /// How many sessions it has had (Claude Code sessions, Codex threads).
    #[serde(default)]
    pub sessions: usize,
    /// The time of the latest line of any of its transcripts.
    #[serde(default)]
    pub last_active: Option<String>,
    /// Its tokens since it was registered, per model and service tier, summed over all of its sessions.
    #[serde(default)]
    pub usage: Vec<ModelUsage>,
    /// Its team grant (`coord team grant`): how many agents its team may have, how many it added, who granted it.
    #[serde(default)]
    pub team: Option<TeamGrant>,
    /// The agent whose team it was added to (under that agent's grant).
    #[serde(default)]
    pub member_of: Option<String>,
}

/// A team grant (vault designs/teams-and-tiers-2026-11-10.md): up to GRANTED agents, USED added so far, granted BY.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct TeamGrant {
    pub granted: u32,
    pub used: u32,
    pub by: String,
}

/// Token counts of requests. `input` is the prompt not read from or written to the cache, so the prompt is
/// `input + cache_read + cache_write`; `reasoning` is the part of `output` the model spent thinking.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    #[serde(default)]
    pub reasoning: u64,
}

impl Tokens {
    pub fn prompt(&self) -> u64 {
        self.input + self.cache_read + self.cache_write
    }
    pub fn total(&self) -> u64 {
        self.prompt() + self.output
    }
}

impl std::ops::AddAssign for Tokens {
    fn add_assign(&mut self, o: Tokens) {
        self.input += o.input;
        self.output += o.output;
        self.cache_read += o.cache_read;
        self.cache_write += o.cache_write;
        self.reasoning += o.reasoning;
    }
}

impl std::ops::SubAssign for Tokens {
    fn sub_assign(&mut self, o: Tokens) {
        self.input = self.input.saturating_sub(o.input);
        self.output = self.output.saturating_sub(o.output);
        self.cache_read = self.cache_read.saturating_sub(o.cache_read);
        self.cache_write = self.cache_write.saturating_sub(o.cache_write);
        self.reasoning = self.reasoning.saturating_sub(o.reasoning);
    }
}

/// The prompt size above which a request is long-context. The proxy's usage page (cpa `usage`,
/// LONG_CONTEXT_INPUT_TOKEN_THRESHOLD) prices those at its long-context rate, so they are counted apart.
pub const LONG_PROMPT: u64 = 272_000;

/// One model's tokens at one service tier: all of them, and the part from long-context requests.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ModelUsage {
    pub model: String,
    /// The service tier the requests ran at, as the transcript names it (empty when it names none).
    #[serde(default)]
    pub tier: String,
    #[serde(flatten)]
    pub tokens: Tokens,
    /// The part of `tokens` from requests whose prompt exceeded [`LONG_PROMPT`].
    #[serde(default)]
    pub long: Tokens,
}

/// A mail as readers see it: the record with its subject and who has it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MailView {
    pub id: String,
    pub at: String,
    pub from: String,
    pub to: Vec<String>,
    pub subject: String,
    pub body: String,
    pub esc: bool,
    /// Sent to go in after the agent's next tool call, its turn not stopped.
    #[serde(default)]
    pub soon: bool,
    pub delivered: Vec<String>,
    /// The recipients its sender took it back from before it reached them (`Request::Unsend`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub withdrawn: Vec<String>,
    /// `question` for a question to the owner (`coord ask`, id `qN`); empty for a message.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    /// The mail or question this one answers or follows up; a question's thread is it and everything `re` it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub re: Option<String>,
    /// Where the passages the body quotes came from (the owner's comments on selected text).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quotes: Vec<Quote>,
    /// A thread's first question only: `open` (its last word is the agent's), `answered` (the owner's) or `dropped`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// The question coordd recognised in this mail of an agent to the owner (a question it did not `coord ask`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recognised: Option<Recognised>,
    /// A net filed this mail from an agent's turn (a question opened, or the agent's words put in its thread): the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filed: Option<String>,
    /// The owner took the filing back (`Request::Unfile`): a filed question is dropped, a filed reply hidden.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unfiled: bool,
    /// Its message net's say, when `deliver` is live: `with the clerk` while the net runs, then `delivered as m.., m..`
    /// (it delivered others in its place) or `sent back: WHY`; none once passed on as it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clerk: Option<String>,
}

/// A question recognised in a mail (vault designs/decisions-policies-forks-2026-11-08.md §8): the question in the
/// recogniser's words, the passage of the mail it is, and the policy decision that chose the model which read it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Recognised {
    pub question: String,
    pub quote: String,
    pub decision: String,
    pub model: String,
}

/// A field of a decision its caller fixed (vault designs/policy-language-draft-2026-11-08.md §14.4), who gave it, when.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Given {
    pub field: String,
    pub value: String,
    pub by: String,
    pub at: String,
}

/// Why an option of a decision was excluded: a named reason (`requires`, `only`, `failed`, `given`) and the rule as
/// written with its place, or the given field.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Exclusion {
    pub option: String,
    pub reason: String,
    pub rule: String,
}

/// One preference's cost for an option: the preference as written with its place, its level, the cost.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Cost {
    pub rule: String,
    pub level: i64,
    pub cost: i64,
}

/// One try of a decided call: the model and, when it failed, why (a failure excludes the model and the decision is
/// solved again, the draft's §5.4).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Attempt {
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
}

/// A policy decision as coordd records it (the draft's §8, "dig into" every outcome).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Decision {
    pub id: String,
    pub at: String,
    /// The decision type (`model-for`).
    pub decision: String,
    /// The call's fields the policy read (`task`, `agent`, `call`).
    pub call: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub given: Vec<Given>,
    /// The option finally used (after any fallback).
    pub chosen: Option<String>,
    /// Every option optimal at the final solve (more than one: a tie; the first by name is taken, no judge yet).
    #[serde(default)]
    pub optimal: Vec<String>,
    #[serde(default)]
    pub excluded: Vec<Exclusion>,
    /// The chosen option's cost per preference, highest level first.
    #[serde(default)]
    pub costs: Vec<Cost>,
    /// The best other option and its costs.
    pub runner_up: Option<(String, Vec<Cost>)>,
    /// The preference that decided between them: the highest level where their costs differ (its cost: by how much
    /// the runner-up was worse).
    pub decided_by: Option<Cost>,
    #[serde(default)]
    pub attempts: Vec<Attempt>,
    /// Why no option was used (`no_route`, every option failed, the policy did not load).
    pub fault: Option<String>,
}

/// A passage the owner commented on: its text and where it came from, an entry of an agent's session
/// (`AGENT@OFFSET`, the entry's transcript offset) or a mail or question (its id).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Quote {
    pub source: String,
    pub text: String,
    /// Its place: byte ranges `[start, end)` of the source's Markdown (empty when not known; then the text is found).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub at: Vec<[usize; 2]>,
}

/// One record of an item's history: opened, closed, started (an agent works on it).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ItemRecord {
    pub ev: String,
    pub id: String,
    pub at: String,
    pub by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

/// A project view's latest data, or why there is none.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ViewData {
    pub id: String,
    pub data: Option<Value>,
    pub error: Option<String>,
}

/// Everything at one moment: the first event of a watch.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    /// The project file as JSON (its views, columns and fields are yaaihh's to draw), or none.
    pub project: Option<Value>,
    pub agents: Vec<AgentView>,
    pub mail: Vec<MailView>,
    pub items: Vec<ItemRecord>,
    pub views: Vec<ViewData>,
    /// The latest net runs, oldest first.
    #[serde(default)]
    pub nets: Vec<NetRun>,
    #[serde(default)]
    pub net_errors: Vec<String>,
}

/// One run of a net on an event (vault designs/policy-language-draft-2026-11-08.md §15–§16): its steps, the effects
/// it took (in a dry run: would have taken), a fault that ended it, the events it raised.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct NetRun {
    /// `N1`, `N2`, ...
    pub id: String,
    pub at: String,
    pub net: String,
    pub event: String,
    /// The agent the event is about, if any.
    #[serde(default)]
    pub agent: Option<String>,
    /// The event's value.
    pub input: Value,
    pub steps: Vec<NetStep>,
    pub effects: Vec<NetEffect>,
    #[serde(default)]
    pub fault: Option<String>,
    #[serde(default)]
    pub raised: Vec<String>,
    /// No effect was taken: each was only logged (an effect is taken when it is in the live set, `nets/live`).
    pub dry: bool,
    /// The owner's marks on its steps and effects (the latest per part), live or dry alike.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<NetMark>,
}

/// The owner's mark on one part of a run (owner m168, 9 Nov: corrections whether the decision is live or not, so a
/// judgment is tried before it is switched on): the `k`-th step (a call or decision) or effect named `what`, right
/// or wrong, with a note. Kept in `$COORD/nets/marks.jsonl`; a later mark on the same part replaces it, and one with
/// verdict `none` takes it back.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct NetMark {
    pub run: String,
    /// `step` or `effect`.
    pub kind: String,
    /// The call or decision (`turn`, `decide end-of-turn`) or the effect (`ask`, `nudge`).
    pub what: String,
    /// Which of the run's parts of that name (0: the first).
    pub k: usize,
    /// `right`, `wrong` or `none`.
    pub verdict: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub at: String,
}

/// One marked part of a run run again (`Request::NetReplay`): what it gave then and what it gives now.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Replayed {
    pub mark: NetMark,
    pub before: Option<Value>,
    pub after: Option<Value>,
    /// Why the run could not be run again (the run is gone from the log, the edited nets are refused, ...).
    #[serde(default)]
    pub error: Option<String>,
}

/// A step of a run that asked something: a call (its decision and model) or a decision, and what it gave.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct NetStep {
    pub line: usize,
    pub what: String,
    #[serde(default)]
    pub decision: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub out: Option<Value>,
}

/// An effect of a run: its name (`deliver`, `nudge`, `fault NAME`, ...) and its values by name.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct NetEffect {
    pub line: usize,
    pub effect: String,
    pub args: Value,
    /// Taken for real (the effect is in the live set, `nets/live`); otherwise only logged.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub live: bool,
    /// A live effect's outcome: what it made (`q15`, `m340`) or why it was not taken (`not taken: ...`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done: Option<String>,
}

/// A file of the nets the Clerk page shows and edits: a net file, bindings, the live set or a call file.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct NetSource {
    pub path: String,
    /// `global` (agent-base) or `project`.
    pub scope: String,
    /// `net`, `bindings`, `live` or `call`.
    pub kind: String,
    pub text: String,
    /// Where an edit of it is saved: the project's file of the same name (it then replaces this one), or itself.
    pub target: String,
}

/// A net that passed its check: where it is, the calls it makes, and its bindings (`global`, `project`, `agent X, Y`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct NetInfo {
    pub name: String,
    pub event: String,
    pub file: String,
    pub line: usize,
    pub calls: Vec<String>,
    pub bound: Vec<String>,
}

/// `Request::Nets`: the nets' files, the nets that passed, and the refusals.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct NetsView {
    pub sources: Vec<NetSource>,
    pub nets: Vec<NetInfo>,
    pub errors: Vec<String>,
    /// Every standing mark (the latest per part), for the judgments' accuracy.
    #[serde(default)]
    pub marks: Vec<NetMark>,
}

/// A net drawn as written (`Request::NetGraph`, the Clerk page's drawing): its event, then its body as nested blocks,
/// so a drawing that stacks a sequence, puts a branch's arms side by side and frames a loop's body never crosses lines.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct NetGraph {
    pub name: String,
    /// `on EVENT(VAR)` with the net's session declarations, as the first node.
    pub start: FlowNode,
    pub body: NetFlow,
}

/// A block of a drawn net.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum NetFlow {
    /// One after the other.
    Seq(Vec<NetFlow>),
    Node(FlowNode),
    /// An `if` or `match`: HEAD (the test, or the call or decision it matches on), then one arm per outcome.
    Branch { head: FlowNode, arms: Vec<FlowArm> },
    /// A `for`: HEAD, then BODY once per element.
    Each { head: FlowNode, body: Box<NetFlow> },
}

impl Default for NetFlow {
    fn default() -> Self {
        NetFlow::Seq(Vec::new())
    }
}

/// An arm of a branch: its pattern or "yes"/"no", and what it does.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct FlowArm {
    pub label: String,
    pub line: usize,
    pub body: NetFlow,
}

/// A step of a drawn net. KIND: `start`, `call` (a model call: SESSION names its session, none = a fresh call),
/// `decide`, `test` (an `if` or `match` on data), `each`, `data` (a `let` or `:=` without a call), `effect`, `fault`,
/// `raise`. LINES: the first and last source line it covers (a branch: the whole statement). OUT: what leaves the
/// net from it (an event raised, a wake). TERMINAL: nothing after it runs (a fault).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct FlowNode {
    pub kind: String,
    pub label: String,
    #[serde(default)]
    pub detail: String,
    pub lines: (usize, usize),
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub out: Vec<String>,
    #[serde(default)]
    pub terminal: bool,
}

/// An edit tried or saved on the Clerk page: the text for the file at `target`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct NetEdit {
    pub target: String,
    pub text: String,
}

/// What one event says.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Body {
    Snapshot(Snapshot),
    Agent(AgentView),
    Mail(MailView),
    Delivered { id: String, to: String, how: String },
    Item(ItemRecord),
    View(ViewData),
    /// A net ran.
    Net(NetRun),
    /// The nets' check found these (each a refusal; empty: all pass).
    Neterrors(Vec<String>),
}

/// One line of `coord watch`: `{"seq": N, "<kind>": {...}}`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub seq: u64,
    #[serde(flatten)]
    pub body: Body,
}

/// A request on coordd's socket, one JSON line. Who asks is not part of it: coordd knows the caller by its process.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Request {
    /// Mail to agents, host sessions or the owner; `esc` stops the recipients' turns first (agents below only).
    /// `as_host` lets the owner side send for a host session.
    /// `re` names the mail or question it answers; `quotes` where the passages it quotes came from.
    Msg {
        to: Vec<String>,
        body: String,
        #[serde(default)]
        esc: bool,
        /// In after the agent's next tool call, its turn not stopped (`esc` wins).
        #[serde(default)]
        soon: bool,
        #[serde(default)]
        as_host: Option<String>,
        #[serde(default)]
        re: Option<String>,
        #[serde(default)]
        quotes: Vec<Quote>,
    },
    /// Take back mail ID from the recipients it has not reached yet (its sender only): it is never delivered to them.
    /// Refused when it reached every one already.
    Unsend { id: String },
    /// Deliver mail ID now to the recipients still waiting for it, stopping their turns first (its sender only; the
    /// esc right: agents below the sender). SOON: after their next tool calls instead, no turn stopped.
    Now { id: String, #[serde(default)] soon: bool },
    /// A question to the owner (its first line is its subject); it does not wait: the reply's `out` is its id, the
    /// owner's answer comes as mail. `on` makes it a follow-up in that question's thread.
    Ask { body: String, #[serde(default)] on: Option<String> },
    /// Withdraw a question (the agent that asked it, or the owner), with why.
    Drop { id: String, #[serde(default)] why: Option<String> },
    /// The owner takes back what a net filed (a mail with `filed`): a question is dropped, a reply hidden.
    Unfile { id: String },
    /// The owner has a mail its message net held or sent back delivered as it was.
    Release { id: String },
    /// The nets' files and the nets that passed (a `NetsView` as JSON).
    Nets,
    /// Run the net bound to EVENT (for AGENT) on VALUE, dry, with EDITS in place of the files (none: the current
    /// version), in fresh sessions; the run as a `NetRun` (JSON, id `try`), or the refusals of the edited version.
    NetTry { #[serde(default)] edits: Vec<NetEdit>, event: String, #[serde(default)] agent: Option<String>, value: Value },
    /// Write EDIT's text to its target, when the nets with it pass their check (no refusal the files did not have).
    NetSave { edit: NetEdit },
    /// The owner marks a part of a run right or wrong (`NetMark`; its `at` is set by coordd).
    NetMark { mark: NetMark },
    /// Every marked run run again, dry and in fresh sessions, with EDITS in place of the files (none: the current
    /// version): a `Vec<Replayed>` as JSON, one per standing mark.
    NetReplay { #[serde(default)] edits: Vec<NetEdit> },
    /// The nets of `.net` file text TEXT (named FILE in errors) drawn as written: a `Vec<NetGraph>`, or the parse error.
    NetGraph { file: String, text: String },
    /// My questions (the owner: every agent's), each with its state and its thread, the answers in full.
    Questions,
    /// My recent mail, or one mail.
    Mail { id: Option<String> },
    /// Each agent's state.
    State,
    /// An agent's session as text (agents below the caller).
    Log { name: String, last: usize },
    /// `start ID`, `open TITLE`, `close ID WHY`.
    Item { action: String, arg: String, #[serde(default)] why: Option<String> },
    /// Register an agent; the caller becomes its parent.
    Add { name: String, kind: String, model: String },
    /// Agent NAME is called TO from now on (stopped agents only; its session, id and history stay).
    Rename { name: String, to: String },
    /// Agent NAME's role from now on (a word the policy's norms name: `lead`, `consultant`, ...); the owner sets it.
    Role { name: String, role: String },
    /// `coord team ask TEXT`: the calling agent asks to form a team; TEXT goes to the approvers the norms name.
    TeamAsk { text: String },
    /// `coord team grant NAME N`: agent NAME's team may have N agents from now on.
    TeamGrant { name: String, granted: u32 },
    /// The norms' verdict on act ACT with field values ARGS, about mail thread THREAD (`none`), and why.
    Norm { act: String, args: Vec<String>, thread: String },
    /// The calling process becomes agent NAME (it then execs the agent).
    Run { name: String },
    /// Session facts of agent NAME known before its first hook (a Codex thread id and app-server socket), merged into
    /// what its hooks report; agents above NAME only.
    Session { name: String, session: Value },
    /// A Claude Code or Codex hook event, with the hook's input.
    Hook { event: String, input: Value },
    /// The numbered event stream, from after `from` when given.
    Watch { from: Option<u64> },
    /// A page of an agent's session as entries (agents below the caller): the lines ending before byte `before`, or
    /// from byte `after` on, or the session's last part; the reply's `out` is a [`Transcript`] as JSON.
    Transcript { name: String, #[serde(default)] before: Option<u64>, #[serde(default)] after: Option<u64> },
    /// Image N (an [`Entry`]'s `images`) of the line at byte OFFSET of NAME's transcript SESSION; the reply's `out`
    /// is `{"media": TYPE, "data": BASE64}`.
    Image { name: String, session: String, offset: u64, n: u32 },
    /// The calling process is an agent's terminal holder (`coord-pty`): coordd puts it in the cgroup `pty`, where
    /// what it starts is unclaimed until it runs `coord run`.
    Pty,
    /// A terminal holder (`coord-pty`) saw a dialog on NAME's screen: TITLE (its question) and the SCREEN's lines
    /// around it. Only told: the holder answers nothing, coordd tells the owner the first time a title is seen.
    Popup { name: String, title: String, screen: String },
    /// A terminal holder (`coord-pty`) tells whether NAME's screen shows a turn running (Claude Code's footer reads
    /// "esc to interrupt" while one runs): the state of a Claude Code agent (owner, 10 Nov), told on each change.
    Busy { name: String, busy: bool },
    /// The bridge's own (`coordc bridge`, next to the holders' sockets; coordd refuses them): attach to NAME's
    /// terminal, which answers with a [`PtyEvent::Screen`] and then its output; `rows`/`cols` 0 keep its size.
    Attach { name: String, #[serde(default)] rows: u16, #[serde(default)] cols: u16 },
    /// Bytes typed into NAME's terminal.
    Keys { name: String, #[serde(with = "b64")] data: Vec<u8> },
    /// The size of the view on NAME's terminal, in cells (the latest client to type or resize sets the terminal's).
    Resize { name: String, rows: u16, cols: u16 },
    /// Stop relaying NAME's terminal.
    Detach { name: String },
    /// The model requests of the caller's agents (all of them for the owner) that ended at or after `since`
    /// (milliseconds since the epoch), as their transcripts record them; the reply's `out` is a list of [`Sent`] as
    /// JSON, oldest first.
    Requests { since: i64 },
    /// The record of policy decision ID (a [`Decision`] as JSON), e.g. the model a recognised question was read with.
    Decision { id: String },
    /// The part of view ID before time BEFORE (RFC 3339), from the view's `older` source in the project file: a view
    /// that holds only its recent part (the commits) gives the rest when the page asks. The reply's `out` is the
    /// source's JSON.
    Older { id: String, before: String },
}

/// One model request of an agent's session, as its transcript records it: when it ended (milliseconds since the
/// epoch), the model that answered and its tokens. The proxy keeps no agent identity, so the usage page tells its
/// requests apart by these (the same model and token counts, ending at the same time).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Sent {
    pub agent: String,
    pub at: i64,
    pub model: String,
    pub tokens: Tokens,
    /// How the request used the prompt cache (Claude's requests; None for others).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<CacheUse>,
}

/// A request's use of the prompt cache, against the previous request of its session (owner 8 Nov,
/// vault/designs/cache-misses-2026-11-08.md).
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct CacheUse {
    /// The part of the cache write kept for 1 h (the rest for 5 min).
    pub write_1h: u64,
    /// Milliseconds since the session's previous request; None for its first (a cold write, not a miss).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<i64>,
    /// The previous request's context written again here instead of read: max(0, min(cache write, previous context −
    /// cache read)), counted from [`MISS_FLOOR`] tokens on.
    pub rewritten: u64,
    /// Why it was rewritten, when it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub miss: Option<Miss>,
}

/// The fewest rewritten tokens that count as a miss (below it, small prefix edits).
pub const MISS_FLOOR: u64 = 1_000;

/// Why a request rewrote cached context.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Miss {
    /// The gap since the previous request outlived the cached entry (5 min or 1 h).
    Expired,
    /// Within the entry's lifetime, but the prefix changed (a model or effort switch, a compaction, the tool list).
    Invalidated,
}

/// What an attached terminal tells the bridge's reader, one `{"pty": …}` line each.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PtyEvent {
    /// The screen as it is (bytes that draw it on an emulator of that size); it comes first and after a re-attach.
    Screen { name: String, rows: u16, cols: u16, #[serde(with = "b64")] data: Vec<u8> },
    /// Output as it comes.
    Output { name: String, #[serde(with = "b64")] data: Vec<u8> },
    /// The terminal has a new size; the output that follows is drawn for it.
    Size { name: String, rows: u16, cols: u16 },
    /// The program ended (how, as text); the screen stays as it was.
    Exit { name: String, status: String },
}

impl PtyEvent {
    pub fn name(&self) -> &str {
        match self {
            PtyEvent::Screen { name, .. } | PtyEvent::Output { name, .. } | PtyEvent::Size { name, .. } | PtyEvent::Exit { name, .. } => name,
        }
    }
}

/// The effects of the net language (the parser's keywords before `(`; the Clerk page colours them).
pub const NET_EFFECTS: &[&str] = &["deliver", "reject", "nudge", "ask", "post", "tell", "accept", "requeue", "evict", "hold"];

pub mod netlayout;

/// Bytes in JSON as base64.
mod b64 {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(v))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        STANDARD.decode(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

/// The holders' wire (vault designs/yaaihh-2026-11-07.md, "Terminals: a holder per agent"). `coord-pty NAME` serves
/// `$COORD/pty/NAME.sock`; each message either way is a frame: one kind byte, the payload's length (4 bytes, big
/// endian), the payload. A size is rows then cols, two bytes each, big endian. Closing the connection detaches.
pub mod pty {
    use serde::{Deserialize, Serialize};
    use std::io::{self, Read};
    use std::path::{Path, PathBuf};

    /// Client: attach at size (0, 0: keep the terminal's). Answered by SCREEN, then OUTPUT, SIZE and EXIT as they come.
    pub const ATTACH: u8 = b'A';
    /// Client: bytes typed. From an attached client it also makes that client's size the terminal's (tmux "latest").
    pub const KEYS: u8 = b'K';
    /// Client: its size; the terminal takes it.
    pub const RESIZE: u8 = b'R';
    /// Client: the screen as plain text, please. Holder: that text.
    pub const TEXT: u8 = b'T';
    /// Client: what the holder runs, please. Holder: an [`Info`] as JSON.
    pub const INFO: u8 = b'I';
    /// Client: hang up, as closing a terminal window does (SIGHUP to the program). Answered by EXIT once it ended.
    pub const HANGUP: u8 = b'H';
    /// Holder: the size, then bytes that draw the screen as it is (cursor, colours, modes) on an emulator of that size.
    pub const SCREEN: u8 = b'S';
    /// Holder: the program's output.
    pub const OUTPUT: u8 = b'O';
    /// Holder: the terminal's new size.
    pub const SIZE: u8 = b'Z';
    /// Holder: the program ended; how, as text.
    pub const EXIT: u8 = b'X';

    /// What a holder runs.
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct Info {
        pub name: String,
        /// The holder's pid and its program's.
        pub pid: i32,
        pub child: i32,
        pub command: Vec<String>,
        pub rows: u16,
        pub cols: u16,
        /// The window title the program set.
        pub title: String,
        /// How the program ended, once it has.
        pub exited: Option<String>,
    }

    pub fn dir(coord: &str) -> PathBuf {
        Path::new(coord).join("pty")
    }

    /// NAME's socket.
    pub fn socket(coord: &str, name: &str) -> PathBuf {
        dir(coord).join(format!("{name}.sock"))
    }

    /// NAME's final screen, as text, written when its program ended.
    pub fn last(coord: &str, name: &str) -> PathBuf {
        dir(coord).join(format!("{name}.last"))
    }

    pub fn frame(kind: u8, payload: &[u8]) -> Vec<u8> {
        let mut f = Vec::with_capacity(5 + payload.len());
        f.push(kind);
        f.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        f.extend_from_slice(payload);
        f
    }

    /// The next frame; an error at the connection's end.
    pub fn read(r: &mut impl Read) -> io::Result<(u8, Vec<u8>)> {
        let mut head = [0u8; 5];
        r.read_exact(&mut head)?;
        let mut payload = vec![0u8; u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize];
        r.read_exact(&mut payload)?;
        Ok((head[0], payload))
    }

    pub fn size(rows: u16, cols: u16) -> [u8; 4] {
        let (r, c) = (rows.to_be_bytes(), cols.to_be_bytes());
        [r[0], r[1], c[0], c[1]]
    }

    /// The size a payload starts with.
    pub fn parse_size(p: &[u8]) -> Option<(u16, u16)> {
        Some((u16::from_be_bytes([*p.first()?, *p.get(1)?]), u16::from_be_bytes([*p.get(2)?, *p.get(3)?])))
    }
}

/// coordd's answer to a request (a watch answers with events instead).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    #[serde(default)]
    pub out: String,
}

/// A mail's subject: its first non-empty line.
pub fn subject(body: &str) -> String {
    body.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").chars().take(120).collect()
}
