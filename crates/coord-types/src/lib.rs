//! The types coordd and its readers share (vault designs/yaaihh-2026-11-07.md §7): the requests on coordd's socket,
//! their replies, and the numbered events of `coord watch`. yaaihh reads the same stream, so these are defined once.

pub use docket_machine::{Anchor, ClockView, Comment, PostWhere, TicketView, ViewMention, Waiting, Where, WhereAnswer, WhereStep};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod groups;

/// The socket coordd listens on inside the box.
pub const SOCKET: &str = "/run/coordd.sock";

/// The kind of the project's owner model, its standing ticket (agent-base/kinds/owner_model.kind; coordd brain.rs), and
/// [`OWNER_UPDATE`], the kind of an update of it. Hidden (paper-research N-1446 p1121, proto-lead p1123): the window's
/// Threads lists neither (yaaihh threads.rs `listed`); the Map draws the standing one at the project's level, and he
/// opens it from there.
pub const OWNER_MODEL: &str = "owner_model";
/// The kind of an update of the project's owner model, a ticket under its standing one ([`OWNER_MODEL`]).
pub const OWNER_UPDATE: &str = "owner_update";

/// The bridge's protocol (`coordc bridge`, the app's pipe; N-412 ask 13): the number this build speaks. The bridge's
/// first line is its [`Hello`], and an app's first line on the bridge's stdin is its own, so a client and a server
/// built from different releases (an app on Windows, coordd in WSL) each say when the other is too old or too new
/// instead of misreading it. 1: the hello itself, and the owner's AddAgent, ForkAgent and RenameAgent. 2: the bridge's
/// Lsp (docket-ls hovers). 3: Snapshot.places, Subject, Harness and HarnessLatest. 4: the owner's files:
/// Roots, ListDir, ReadFile, Locate, Sources and EditFile. 5: Mentions (and the views' mentions and mentioned). 6: CleanText
/// (the owner's typing cleaned). 7: Compact (an agent's compaction point, `AgentContext.next`).
/// 8: the owner's accounts (Native, Accounts and the account requests, `NativeAccounts`; the window's `ACCOUNTS`).
/// 9: a thread's end, asks withdrawn and posts taken back (EndThread, WithdrawAsks, TakeBack; `AsksClosed::Withdrawn` and
/// `Subthread`, `would_close`; `Say`'s and `coord msg`'s `under`).
/// 10: the acts an agent may do now (Enabled, `ActAnswer`; `ENABLED_PROTOCOL`).
/// 11: an API-key account (AccountKey, AccountKeyStore; `AccountNew.key`, `NativeAccount.key` and `key_since`; `ACCOUNT_KEY`).
/// 12: ListDir's git states (`ListDir.git`, `DirEntry.git`; `FILE_STATES_PROTOCOL`). 13: the duties that hold (Duties,
/// `DutyAnswer`; `DUTIES_PROTOCOL`).
/// 14: an account removed, put back, a key removed from the window (AccountRemove, AccountPutBack, AccountKeyRemove,
/// `NativeAccounts.removed`; `ACCOUNT_REMOVE`). 15: every agent on the owner's accounts (AccountsAll,
/// `NativeAccounts.all` and `gateway`, `NativeAccount.on_gateway`; `ACCOUNTS_ALL`).
/// 16: not used.
/// 17: his message split into points (`Request::Split`, `Post.split_from` and `Say.split_from`; `SPLIT_PROTOCOL`).
/// 18: an account in use removed once its agents have moved (AccountKeep, `NativeAccount.leaving` and
/// `locked_by`; `ACCOUNT_LEAVE`).
/// 19: one message on several threads (`Request::Post.on`, `Post.on`; `POSTED_ON_PROTOCOL`).
/// 20: not used.
/// 21: not used: kept for the raw pane, which missed train 7 (N-802.p12).
/// 22: one restart sets model and effort, and Restart (`Request::Model` with `MODEL:EFFORT`, `Request::Restart`;
/// `MODEL_EFFORT_PROTOCOL`).
/// 23: the raw pane's lines read by coordd (`Request::RawPage`; `RAW_PAGE_PROTOCOL`).
/// 24: the Harnesses page's switches set through coordd (`Request::HarnessSet`; `RULES_PROTOCOL`).
/// 25: an agent's Safe · Yolo, the owner's (`Request::Mode`, `AgentView.mode`, `mode_due` and `restarting`;
/// `MODE_PROTOCOL`).
/// 26: where each post of the owner's is for each agent it went to (`TicketView.whereabouts`; `WHEREABOUTS_PROTOCOL`).
/// 27: every agent's running turn stopped at once (`Request::EscAll`; `ESC_ALL_PROTOCOL`).
/// 28: `@team` and `@PART` in the owner's post on a thread (coordd resolves them, [`groups`]; `GROUPS_PROTOCOL`).
/// 29: an opencode account (`AccountNew.harness` and `provider`, `AccountSignin.method`, `NativeAccount.provider`;
/// `OPENCODE_ACCOUNTS`).
/// 30: his archive taken live (`Request::Archive` with `on: true`; `ARCHIVE_PROTOCOL`).
/// 31: a sign-in account's reset `Use` applied, its offers given, and its check reading its windows
/// (`Request::AccountReset`, `NativeAccount.resets`; `ACCOUNT_RESET`).
/// 32: agents' mail held by the owner (`Request::MailHold`, `Snapshot.mail_hold`; `MAIL_HOLD_PROTOCOL`).
/// 33: an account renamed, turned off or on, a sign-in account's resets offered at its limit with no notice seen, its
/// weeks of one model, and the resets `/limit-reset` lists on the owner's check (`Request::AccountEdit`,
/// `NativeAccount.off`, `model_weeks`, `held`; `ACCOUNT_EDIT`; N-1437).
/// 34: what a draft of a kind or call would reach, before the owner saves it (`Request::Reach`; `REACH_PROTOCOL`).
/// 35: the record as written about a ticket, for the 0.3 window's Events, Record and Wire layers (`Request::Record`;
/// `RECORD_PROTOCOL`).
/// 36: insights into his data, work and talk (1), open to done (3) and agents per topic (2) (from (3)'s rows, each with
/// its parent line, the agents given anything there and an item's starters), and each turn's tools and model on its
/// `turn_ended` line (`Request::Insight`; `INSIGHT_PROTOCOL`); the release his window runs, which nothing gates on: an
/// older coordd refuses it as no request, and the bridge drops the answer (`Request::WindowRelease`; I-1457, p600).
/// 37: an agent's brain switched on and off by the owner, and its state on the agent's view (`Request::Brain`,
/// `AgentView.brain`; `BRAIN_PROTOCOL`; proto-lead N-1446 p857.2, N-1475).
/// 38: his press cleared a gateway login's cooldown, and the agents held through the gateway on that family's models
/// go on (`Request::GatewayCleared`; `GATEWAY_CLEARED_PROTOCOL`; N-1446 p957).
/// 39: his posts' asks on a ticket's view, each agent asked with the post that answered it (`TicketView::asked`, the
/// ledger's one rule; `ASKED_PROTOCOL`; I-1455 (B), proto-lead N-1479 p86).
/// 40: the project's owner model as coordd reads it, on the snapshot and at each change (`Snapshot.owner_model`,
/// `Body::OwnerModel`; `OWNER_MODEL_PROTOCOL`; proto-reader N-1446 p1212, B; proto-lead p1227), numbered at its merge.
/// 41: the record's one graph, the part of it an ask names, by the layers he switches on (`Request::Graph`,
/// `GraphAnswer`; `GRAPH_PROTOCOL`; proto-window N-1446 p1383, head 1), numbered at its merge.
/// 42: a node's words on his click, a slice of a session first: owner only, its place the record's own line and
/// served only when its bytes hold the words of each post the giving brought (`Request::Raw`; `RAW_PROTOCOL`;
/// docket-edge8 N-1446 p1418, the guard read right p1419), numbered at its merge (paper-research p1425).
/// 43: the graph's page (head 2): an ask by item as by who and what is worked on now (`Ask::items`, `Ask::now`,
/// `GraphAnswer::now`), and around his post its course (`GraphAnswer::course`) (`GRAPH_PAGE_PROTOCOL`; proto-window
/// N-1446 p1383, paper-research p1385 B, p1429 1), numbered at its merge.
/// 44: an OpenCode Go account's usage as Go's usage page names it, its 5-hour, weekly and monthly windows, and whether
/// coordd keeps its key to read it (`NativeAccount.month`, `usage_key`; `GO_USAGE_PROTOCOL`; the owner, N-1104 p808),
/// numbered at its merge (paper-research N-616 p37: 43 is the graph page's).
/// 45: an agent's folder, the file view: its starts, loaded texts by version, reads, sessions, and commits and jobs
/// with their worktrees, each row saying its source (`Ask::folder`, `GraphAnswer::folder`; `FOLDER_PROTOCOL`;
/// docket-edge6 N-1446 p1529, read right p1530), numbered at its merge (43 the graph page's, 44 Go's usage).
/// 46: in an agent's folder, a system prompt block's text on his click (`Request::Raw` of its text node), and a
/// session's requests alone (`Ask::session`) (`TEXTS_PROTOCOL`; docket-edge6 N-1446 p1641, read right p1642),
/// numbered at its merge.
/// 47: the window's look sent live, the flavour's `look.toml` as coordd took it (`Snapshot.look`, `Body::Look`;
/// `LOOK_PROTOCOL`; here-2's head A and docket-core6's text-change, N-866, N-865; N-1104 p113), numbered at its merge.
/// 48: a step's prompt as it would be sent were the owner's draft saved, nothing sent (`Request::DryRun`,
/// `DryRunAnswer`; `DRY_RUN_PROTOCOL`; the owner N-216.p1; docket-edge8 N-1446 p1824, H2), numbered at its merge.
/// 49: his asks from asked to his sign-off, Open → done's third row, and his sign-off of one with his word of worth
/// (`Insight::Asks`, `Request::SignOff`; `ASKS_PROTOCOL`; paper-research N-1446 p1786), numbered at its merge.
/// 50: an agy account (Google's Antigravity CLI), signed in by t3code's own Google sign-in or on a Gemini API key, and
/// agy agents on it (`AccountNew.harness` `agy`, `NativeAccount.models`; `AGY_ACCOUNTS`; the owner, N-1104 p880 and
/// p886; accounts-lead N-1446 p2038, read right p2039 and p2043).
/// 51: the owner's edit of an agent's system prompt as a branch in its history (`Request::SystemPrompt`;
/// `SYSTEM_PROMPT_PROTOCOL`; the owner N-1104 p996; accounts-lead N-1446 p2447, go paper-research p2456), numbered at
/// its merge.
/// 52: a repository added to the project from its Home, the owner's (`Request::AddRepo`; `ADD_REPO_PROTOCOL`; the
/// owner N-1104 p999; accounts-lead N-1446 p2446; docket-edge8's shape p2452, read right p2455 and p2456), numbered at
/// its merge.
/// 53: an agent's system prompt as a node, each version counted in requests, turns and steps, and one version's
/// requests under their turns and steps (`Ask::prompt`, `Ask::version`; `PROMPT_PROTOCOL`; fix 4, docket-edge6 N-1446
/// p2496, read right p2504), numbered at its merge.
/// 54: a goal's latest duties on its ticket's view, each owed duty's holder and the line that met each duty
/// (`TicketView::duties`; `GOAL_DUTIES_PROTOCOL`; paper-research N-1446 p2328, 2; the shape docket-edge6 p2421,
/// proto-window p2434), numbered at its merge.
/// 55: an agent's transcript other than its current one, by its path (`Request::Transcript`'s `session`), and his
/// system-prompt edits in the one read (`Transcript::restarted`; `TRANSCRIPT_SESSION_PROTOCOL`; path fix 6 point 3,
/// accounts-lead N-1446 p2524, p2651), after edge6-met77's 54, numbered at its merge.
/// 56: the devices on Home, read by coordd from the one file of the machines (`Request::Machines`;
/// `MACHINES_PROTOCOL`; docket-edge7 for the owner's N-1104 p1070, on here-2's f2df1afd), after H-1591's 55, numbered
/// at its merge.
/// 57: what the store holds, the runner's rows and a page of a row's entries, asked of its service (`Request::Store`;
/// `STORE_PROTOCOL`; Home's Store, paper-research's N-1104 p1069 with his yes p1071; a2h-speed N-1446 p2815,
/// docket-edge8's names p2904, agreed p2927), numbered at its merge.
/// 58: a request now gone (a repository in a browser viewer); its number stays taken (`REPO_VIEW_PROTOCOL`).
/// 59: everything of phase 1, in one release: the owner's turn-by-turn control (`CONTROL_PROTOCOL`), the models with
/// their guesses and each harness's table (`MODEL_LIST_PROTOCOL`), a store finding deleted (`STORE_FREE_PROTOCOL`), a
/// PDF page, the settings at each level and the repositories' state (`HOME_PROTOCOL`), a spin-off's end and a fork's source
/// (`SPIN_PROTOCOL`).
/// Each request added later raises it by one, in the release that brings it, so an app gates each page on the number
/// that brought its request.
/// 60: everything v0.4 brings past phase 1, in one release: the code view (`CODE_VIEW_PROTOCOL`), a rewind's fork
/// (`REWIND_PROTOCOL`), comments on posts (`COMMENTS_PROTOCOL`), a brain that says it is ready (`BRAIN_READY_PROTOCOL`),
/// cold storage (`COLD_PROTOCOL`), the native usage as summed cells (`USAGE_CELLS`).
/// 61: the standing texts by file (`STANDING_PROTOCOL`). 62: the system prompt file an agent is given (`GIVEN_PROTOCOL`).
/// 63: the code view in the window's colours, its view kept by the window and its server let go when the window leaves
/// it (`CodeTarget::Workspace`, `Request::CodeView.looks`, `Request::CodeViewLeft`; `CODE_WARM_PROTOCOL`). 64: the
/// owner's Safe · Yolo for an agy agent too (`Request::Mode`; `AGY_MODE_PROTOCOL`; N-1956), numbered at its merge.
/// 65: his messages held to go later (`Request::Schedule`; `SCHEDULE_PROTOCOL`). 66: a Safe agent's read outside its
/// folders as a card in its chat (`AgentView.read_asks`, `Request::ReadAnswer`; `READ_ASK_PROTOCOL`).
pub const PROTOCOL: u32 = 66;

/// Brings [`Request::Mode`] for an agy agent: below it an agy agent's panel and the Policy page offer no Safe · Yolo,
/// as the coordd refuses it.
pub const AGY_MODE_PROTOCOL: u32 = 64;

/// Brings [`Request::CodeView`]: below it a press on a file, commit or repository opens no code view.
pub const CODE_VIEW_PROTOCOL: u32 = 60;

/// Brings [`codeview::CodeTarget::Workspace`], [`Request::CodeView`]'s looks and [`Request::CodeViewLeft`]: below it the
/// window neither sends its colours nor lets a coordd's server go.
pub const CODE_WARM_PROTOCOL: u32 = 63;

/// Brings [`Request::Schedule`]: below it the window draws no Schedule beside Send.
pub const SCHEDULE_PROTOCOL: u32 = 65;

/// Brings [`Request::ReadAnswer`]: below it no read card is drawn, as no coordd sends `read_asks`.
pub const READ_ASK_PROTOCOL: u32 = 66;

/// Brings [`Request::ModelList`] and [`Request::HarnessCapabilities`]; below it New agent keeps [`Request::Models`].
pub const MODEL_LIST_PROTOCOL: u32 = 59;

/// The protocol that brings [`Request::UsageCells`]: below it the Usage page shows the gateway's usage alone.
pub const USAGE_CELLS: u32 = 60;

/// 58, taken by a request now gone; nothing is gated on it.
pub const REPO_VIEW_PROTOCOL: u32 = 58;

/// Brings [`Request::HaltReject`], [`Request::DeliverUrgent`], [`Request::AgentPace`], [`Request::HarnessSwitches`],
/// `Esc`'s `step`, and `AgentView`'s `control`, `capabilities`, `switches` and `apply_error`.
pub const CONTROL_PROTOCOL: u32 = 59;

/// Brings [`Request::StoreFree`]: below it Home's Store shows what the walk found with no press to delete it.
pub const STORE_FREE_PROTOCOL: u32 = 59;

/// Brings [`Request::PdfPage`], [`Request::Settings`] and [`Request::RepoState`]: below it no PDF view, no Settings
/// page and no repository state on Home.
pub const HOME_PROTOCOL: u32 = 59;

/// The protocol that brings [`Request::SpinEnd`] and [`AgentView::from`]: a spin-off's Join back, Close and Delete
/// show only from a coordd that says this or more.
pub const SPIN_PROTOCOL: u32 = 59;

/// Brings `Request::ForkAgent`'s `at`: the Map's Rewind / Branch from here, a copy cut at a past turn's end.
pub const REWIND_PROTOCOL: u32 = 60;

/// Brings `Request::Msg`'s `comments` and [`Request::CommentResolve`]: below it his comments go as words alone.
pub const COMMENTS_PROTOCOL: u32 = 60;

/// Brings [`BrainView::ready`]: below it a brain's satellite says no suggestions waiting.
pub const BRAIN_READY_PROTOCOL: u32 = 60;

/// Brings [`Request::Standing`]: below it System prompt marks the standing text as one, not by the file it came from.
pub const STANDING_PROTOCOL: u32 = 61;

/// Brings [`Request::Given`]: below it System prompt before an agent's first request shows the standing texts alone.
pub const GIVEN_PROTOCOL: u32 = 62;

/// What coordd's refusal of [`Request::Given`] says when the agent has no file (roster writes it at its start), so the
/// window tells it from any other refusal.
pub const NO_GIVEN_FILE: &str = "has no system prompt file";

/// The system prompt file roster gave an agent at its last start (`system-prompt.md`, roster `system_prompt`): its
/// PATH and TEXT; REPLACE, in place of the harness's own (else appended to it); HIS, the owner's version, not the
/// standing text. DIFF, roster's `system-prompt.md.diff.json` beside it, where a prompt recipe made it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GivenPrompt {
    pub path: String,
    pub text: String,
    pub replace: bool,
    pub his: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<PromptDiff>,
}

/// How a prompt recipe made an agent's system prompt from its harness's own (roster's `system-prompt.md.diff.json`,
/// gap-homes N-1981 p2): its segments in the harness's order, then what was added; the kept and added texts, joined
/// by a blank line, are the file sent. NOTES: what the recipe could not do (a cut that matches nothing).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptDiff {
    #[serde(default)]
    pub harness: String,
    #[serde(default)]
    pub how: String,
    #[serde(default)]
    pub notes: Vec<String>,
    pub segments: Vec<PromptSegment>,
}

/// One segment: KIND `kept`, `stripped`, `added` or `missing`; LABEL its part's heading or the recipe's name for it
/// (`#4 Pronouns`, `pack fable`, `standing text`); TOKENS estimated (bytes ÷ 4).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSegment {
    pub kind: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub tokens: usize,
    #[serde(default)]
    pub source: String,
}

/// One of the standing texts roster appends to Claude Code's own (agent-base/bin/roster `standing`), in its order:
/// FROM `global` (standing.md at the root of the project's repository's main checkout), `project` (beside the project
/// file) or `agent-base` (agent-base's own, where the project has none); its PATH and TEXT as the file holds it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandingPart {
    pub from: String,
    pub path: String,
    pub text: String,
}

/// How the owner ends a spin-off (`Request::SpinEnd`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpinEnd {
    /// What it found goes to the agent it was copied from, in its own words; then its session ends and it is ended
    /// for good (`Request::Ended`): in none of his lists.
    Join,
    /// Its session ends; it stays a stopped agent, which he can start again on its own conversation.
    Close,
    /// Its session ends and it is ended for good, nothing handed back: in none of his lists, its sessions kept.
    Delete,
}

/// The protocol that brings [`Request::Cold`]: below it Home shows no cold storage.
pub const COLD_PROTOCOL: u32 = 60;

/// The protocol that brings [`Request::Store`] (Home's Store, a2h-speed N-1446 p2815): below it Home leaves the Store
/// out. 57, after the devices' 56, numbered at its merge.
pub const STORE_PROTOCOL: u32 = 57;

/// The protocol that brought `Request::Transcript`'s `session` and [`Transcript::restarted`]: the agent's history
/// opens the old line beside his system-prompt edit from a turn only from a coordd that says this or more; an older one
/// would read the current session in its place (path fix 6 point 3).
pub const TRANSCRIPT_SESSION_PROTOCOL: u32 = 55;

/// The protocol that brought [`Request::SystemPrompt`]: the agent page's Edit of its system prompt is shown on a coordd
/// that says this or more.
pub const SYSTEM_PROMPT_PROTOCOL: u32 = 51;

/// The protocol that brought [`Body::Look`] (here-2, N-412 post 1107): coordd sends a look that changed only to an app
/// whose hello says this or more; an older one has the look in its next snapshot (`Snapshot.look`, which it ignores).
/// 47, the next free number after the texts' 46, numbered at its merge.
pub const LOOK_PROTOCOL: u32 = 47;

/// The protocol that brings [`Request::DryRun`] (H2, the owner's "we can dry run it ??", N-216.p1): the window offers
/// "Show it as it would be sent" under a prompt being edited only to a coordd whose hello says this or more. 48, the
/// next free number after the look's 47, numbered at its merge.
pub const DRY_RUN_PROTOCOL: u32 = 48;

/// The line a stand-in window writes on its output once its link has brought the snapshot (`YAAIHH_STILL_AFTER=look`):
/// coordd's text-change still (coordd change.rs, consult-lead N-412 p1203) then sends the look, and the stand-in draws
/// once that `Body::Look` came on its link, as his window takes a change.
pub const STILL_WATCHING: &str = "yaaihh: watching";

/// The protocol from which an OpenCode Go account's card carries Go's usage (`NativeAccount.month`, `usage_key`, and
/// its `five_hour` and `seven_day` from Go's usage page): below it the window draws no windows on an opencode card,
/// and its `Add opencode` sheet says yaaihh keeps the key nowhere. Numbered when it merges: the next free number.
pub const GO_USAGE_PROTOCOL: u32 = 44;

/// The protocol from which coordd answers [`Request::Graph`]: the window asks it only of a coordd whose hello says this
/// or more, and below it says the graph is not served there. Numbered when it merges: the next free number.
pub const GRAPH_PROTOCOL: u32 = 41;

/// The protocol from which the graph's answer is the page's (head 2): an ask names items as it names who, asks what
/// is worked on now, and around his post is given its course. Below it a coordd leaves those out of its answer unsaid,
/// so the window draws the Map's own places there, not the page (map/lanes.rs `serves`). 43, the next free number after
/// Raw's 42, numbered at its merge.
pub const GRAPH_PAGE_PROTOCOL: u32 = 43;

/// The protocol from which coordd answers [`Request::GatewayCleared`]: the window sends it, at his `Clear cooldown` on
/// a gateway login or his reset of one that went through, only to a coordd whose hello says this or more; below it,
/// it sends nothing and the press does what it did. 38, fixed on its branch before the merge, after the brain switch's
/// 37 in the same train (proto-lead N-1446 p978, p982); a one-line commit takes 37 if the switch misses the cut.
pub const GATEWAY_CLEARED_PROTOCOL: u32 = 38;

/// The protocol from which a ticket's view carries `asked` (docket-machine `TicketView::asked`, `Machine::asked`):
/// below it a view has none and says nothing of his asks, so the window draws his posts as lit points, not rings, and
/// takes `unanswered_owner` as the older coordd made it. 39, fixed on its branch before the merge (proto-lead N-1479
/// p86); a one-line commit takes the next number if another request reaches a cut first.
pub const ASKED_PROTOCOL: u32 = 39;

/// The protocol that brings the owner model's reading to the window (`Snapshot.owner_model`, `Body::OwnerModel`;
/// proto-reader N-1446 p1212, B): the bridge gives `Body::OwnerModel` only to an app at it or more, and the Map draws
/// the node only from a coordd that speaks it (proto-lead p1227). Numbered when it merges: the next free number.
pub const OWNER_MODEL_PROTOCOL: u32 = 40;

/// The protocol from which coordd answers [`Request::Raw`]: a node's words on a click, a slice of a session first
/// (paper-research N-1446 p1380). The window asks it only of a coordd at this number or later; below it, a slice's
/// card says its words are not served there. 42, numbered at its merge in train 67, after Graph's 41, with no
/// gap: the band and Sent, out of this train, take the next free numbers when they come (paper-research p1425).
pub const RAW_PROTOCOL: u32 = 42;

/// The protocol from which coordd answers an ask's `folder` (docket-machine `graph_slice::Ask::folder`): the window
/// asks it only of a coordd at this number or later; an older one reads the ask as one for a slice of the whole
/// graph. 45, numbered at its merge, the next free number after the graph page's 43 and Go's usage's 44.
pub const FOLDER_PROTOCOL: u32 = 45;

/// The protocol from which coordd answers, in an agent's folder, [`Request::Raw`] of a system prompt block's text node
/// (its text, read from the shape line of the request that carried it, served only when one of its blocks hashes to
/// the node) and an ask's `session` (docket-machine `graph_slice::Ask::session`: that session's requests alone)
/// (paper-research N-1446 p1642, 2 and 3). The window asks them only of a coordd at this number or later; below it a
/// block's or a session's popup says they are not served there. 46, numbered at its merge.
pub const TEXTS_PROTOCOL: u32 = 46;

/// The protocol from which coordd answers an ask's `prompt` and `version` (docket-machine `graph_slice::Ask::prompt`,
/// `Graph::prompt`: an agent's system prompt, the node his System prompt press opens; fix 4): the window asks them only
/// of a coordd at this number or later; an older one reads the ask as one for a slice of the whole graph. 53, numbered
/// at its merge (train 76).
pub const PROMPT_PROTOCOL: u32 = 53;

/// The protocol from which a goal's ticket view carries its latest duties (docket-machine `TicketView::duties`): a goal
/// with none says "duties: not asked yet" from a coordd at this number or later, "duties: waiting for coordd" before
/// (proto-window N-1446 p2434). 54, numbered at its merge. Not `DUTIES_PROTOCOL`, 13's: the duties that hold.
pub const GOAL_DUTIES_PROTOCOL: u32 = 54;

/// The protocol that brings [`Request::Reach`] (line 26's live edits, N-1396 p294): the window asks what an edit
/// reaches only of a coordd whose hello says this or more; below it, it asks nothing and Save is as before. Set at its
/// merge into a cut, in the cut's order: 34, after `ACCOUNT_EDIT` (consult-3 N-1396 p317).
pub const REACH_PROTOCOL: u32 = 34;

/// The protocol that brings [`Request::Mode`] and `AgentView.mode`, `mode_due` and `restarting` (0.2 line 25, N-1400):
/// the window offers its `Safe · Yolo` control only to a coordd whose hello says this or more. Set at its merge into a
/// cut, in the cut's order (paper-research N-412 p1987).
pub const MODE_PROTOCOL: u32 = 25;

/// The protocol from which coordd's ticket views carry `whereabouts` (part 4 of the message runtime, N-818 post 32):
/// a window that reads it from a coordd saying less in its hello falls back to what it works out itself. 26 from its
/// merge into the 0.2 train, after line 25 (30 in its head).
pub const WHEREABOUTS_PROTOCOL: u32 = 26;

/// The protocol from which coordd reads `@team` and `@PART` in the owner's post on a thread as each running agent of
/// the group named (0.2 B10, [`groups`]). Below it a post's group tag would reach nobody, so the window refuses it
/// there and says where it goes (a Say carries the group's agents as names at any protocol). 28 from its merge into
/// the 0.2 train (docket-core N-1396 p22; 25 in its head).
pub const GROUPS_PROTOCOL: u32 = 28;

/// The protocol that brought [`Request::CleanText`] (N-487; fixed at its merge as 6, the first of the pieces read to
/// go in). An app offers cleaning only when the other side's hello says this or more.
pub const CLEAN_TEXT_PROTOCOL: u32 = 6;

/// The window's pause before it starts a dropped bridge again (yaaihh link.rs; doubling while it fails at once). A
/// dropped pipe is back after it; coordd counts a window closed only after a few of them have passed with no watch
/// up for its box (coordd lapse.rs `BOX_GONE_AFTER`), so the two cannot drift (paper-research N-487 p30).
pub const PIPE_PAUSE_MIN: std::time::Duration = std::time::Duration::from_secs(3);

/// The protocol that brings `AsksClosed::Withdrawn` and `AsksClosed::Subthread` in the views (with `withdraw_asks`,
/// `end`, `would_close`; docket-core N-447 p39, fixed at its merge as 9): a window that said less in its hello is sent
/// them as the closest it can read, `NothingToAnswer` and `Followed` (coordc's bridge, `older_asks_closed`).
pub const ASKS_CLOSED_PROTOCOL: u32 = 9;

/// The protocol from which [`Request::Model`] takes `MODEL:EFFORT`, both set by one restart (the session panel's one
/// Apply, N-804.p7; before it the window sends `model`, then `effort`), and which brings [`Request::Restart`] (its
/// Restart, N-804.p9). 22: 21 is docket-edge's (N-802).
pub const MODEL_EFFORT_PROTOCOL: u32 = 22;

/// The oldest protocol of the other side this build still speaks. An app that sends no hello (one from before
/// protocol 1) counts as 0, and keeps working while this is 0.
pub const PROTOCOL_MIN: u32 = 0;

/// The protocol that brings [`Request::Split`] (N-696): the window offers his message's points only to a coordd whose
/// hello says this or more. Fixed at its merge as 17.
pub const SPLIT_PROTOCOL: u32 = 17;

/// The protocol that brings `Request::Compact` and `AgentContext.next` (paper-research N-490.p8): a window sends
/// `compact` only to a coordd at this number or later. Fixed at its merge as 7.
pub const COMPACT_POINT_PROTOCOL: u32 = 7;

/// The protocol from which coordd applies `Request::AccountReset` and gives `NativeAccount::resets`, and from which
/// `Request::AccountCheck` of a sign-in account also reads its windows: a window shows `Use`, and says its check reads
/// the limits, only to a coordd at this number or later. 31 from its merge into the 0.2 train, after ARCHIVE 30 (D20,
/// accounts-lead's c63281a2, read right consult-3 N-412; u32::MAX in its head).
pub const ACCOUNT_RESET: u32 = 31;

/// The protocol from which coordd applies `Request::AccountEdit` (an account renamed, turned off or on) and gives
/// `NativeAccount::off`, from which it offers a sign-in account's resets at its limit with no notice seen, from which
/// it gives `NativeAccount::model_weeks`, and from which a sign-in account's `Request::AccountCheck` by the owner also
/// reads the resets Claude Code's `/limit-reset` lists, given as `NativeAccount::held` (N-1437): a window shows
/// `Rename` and `Turn off`, and says its check reads the resets, only to a coordd at this number or later. Numbered at
/// its merge into the 0.2 train, before `REACH_PROTOCOL` (consult-3 N-1396 p317).
pub const ACCOUNT_EDIT: u32 = 33;

/// The protocol from which coordd answers [`Request::Record`]: the record as written about a ticket, for the 0.3 window's
/// Record and Events layers (proto-window N-1446 p10, p84). A window offers those layers only to a coordd at this
/// number or later. 35, numbered at its merge into proto-0.3, after `REACH_PROTOCOL` (proto-lead N-1446; u32::MAX in its
/// head).
pub const RECORD_PROTOCOL: u32 = 35;

/// The most lines one [`Request::Record`] answers: about a screen and a half of his thread's busiest minutes (N-1104's
/// p586–p587 is 59 lines of the record), and well under a megabyte when its `opened` lines carry their programs.
pub const RECORD_CAP: u32 = 400;

/// The protocol from which coordd answers [`Request::Insight`], every [`Insight`]: the rows of an insight into his data,
/// for the 0.3 window's insight lenses (S6, proto-lead N-1446 p388), and from which it writes on each turn's
/// `turn_ended` line its tools and model (`docket_machine::Worked`, insight (1)). A window asks for them only of a
/// coordd at this number or later. One number for (1), (2) and (3), which go into one 0.3 cut (proto-lead N-1446 p537,
/// 1): 36, given at their merge into proto-0.3 (u32::MAX in their heads); (2) came before the cut, on (3)'s rows, so
/// it rides on 36.
pub const INSIGHT_PROTOCOL: u32 = 36;

/// The protocol from which coordd answers [`Insight::Asks`] and [`Request::SignOff`]: his asks from asked to his
/// sign-off, Open → done's third row, and his press of a word of worth on one (paper-research N-1446 p1786). A window
/// asks for the row, and offers the press, only of a coordd at this number or later; below it Open → done is as it was.
/// 49 at its merge into train 69, the next free number after the dry run's 48 (47 and 48 went to the look and the dry
/// run; it was 47 on its branch, from train 68's 46).
pub const ASKS_PROTOCOL: u32 = 49;

/// The protocol that brought agy accounts (the owner, N-1104 p880: "i want to act as t3cdoe so have a t3code -agy
/// logingin basiclly"; p886; plans N-1446 p2038, read right p2039 and p2043): `AccountNew` with harness `agy`, a
/// sign-in account (t3code 0.1.11 as it ships, one server per account, its own Google sign-in: `AccountSignin` answers
/// Google's address, `AccountCode` takes the code he pasted) or, with `key`, a Gemini API key account (the key field's
/// `AccountKeyStore` writes its key file); `NativeAccount.models`, the models agy offers on it. An older coordd would
/// read such an `AccountNew` as a Claude Code account's, so the window offers the tile only from this one on. 50, the
/// next free number after the asks' 49.
pub const AGY_ACCOUNTS: u32 = 50;

/// The protocol that brings [`Request::AddRepo`] (fix 3, the project's Home): the window offers "Add a repository…"
/// only to a coordd whose hello says this or more. 51, the next free number after agy's 50, numbered at its merge.
pub const ADD_REPO_PROTOCOL: u32 = 52;

/// The hosts an agy sign-in link may name for the window to open it ([`agy_link_opens`]): Google's own sign-in, the
/// address t3code's Google sign-in gives.
pub const AGY_SIGN_IN_HOSTS: &[&str] = &["accounts.google.com"];

/// Whether URL is an agy sign-in link the window opens: `https`, to a host of [`AGY_SIGN_IN_HOSTS`] exactly (no user
/// part, no other port).
pub fn agy_link_opens(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    AGY_SIGN_IN_HOSTS.contains(&host)
}

/// An insight into his data (S6; his p646 "an insight i can get into my data"): which rows [`Request::Insight`] asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Insight {
    /// (3) open → done: every line of the outline, a thread or an item, with when it opened and ended, the line it
    /// stands under, the agents given anything there and, an item's, who started it (`docket_machine::insight::Thread`);
    /// (2) agents per topic is drawn from the same rows.
    OpenToDone,
    /// (1) work and talk: every turn of the record, with what it did as its `turn_ended` line says
    /// (`docket_machine::insight::Turn`), read from the record's file off coordd's lock.
    WorkTalk,
    /// (3)'s third row, his asks from asked to his sign-off (paper-research N-1446 p1786): each post of his the ASKED
    /// ledger says asks an agent, as the record's graph that coordd keeps reads its course to the last point reached,
    /// with his sign-off, its post's and thread's labels, what it cost and his posts on it once it was in his window
    /// (`docket_machine::insight::Ask`). From [`ASKS_PROTOCOL`].
    Asks,
}

/// One line of the record as written (`Request::Record`): its number in tickets/log.jsonl, counted from 1, and its text.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RecordLine {
    pub line: u64,
    pub text: String,
}

/// The protocol that brings `Request::AccountRemove`, `AccountPutBack` and `AccountKeyRemove`: a window offers
/// Remove, Put back and Remove key only to a coordd at this number or later. Fixed at its merge as 14.
pub const ACCOUNT_REMOVE: u32 = 14;

/// The protocol from which `Request::AccountRemove` on an account that agents run on is not refused: the agents move
/// at the end of their turn and the account is removed then (`NativeAccount.leaving`; `Request::AccountKeep` takes
/// it back meanwhile). A window asks its one question ("orch and 2 others move to Personal ...") only of a coordd at this
/// number or later; an older one refuses in words. Also brings `NativeAccount.locked_by`.
/// Fixed at its merge as 18.
pub const ACCOUNT_LEAVE: u32 = 18;

/// The protocol that brought `Body::Mentioned`: the bridge gives it only to an app whose hello says this or more (an
/// older one could not read it, and has no line back to add it to). Fixed at the merge (docket-core N-447 p39).
pub const MENTIONED_BODY: u32 = 5;

/// The protocol that brought `Request::Enabled` (paper-research N-568 p15, part 6: the drivers): the window asks for an
/// agent's acts only of a coordd whose hello says this or more. Fixed at the merge.
pub const ENABLED_PROTOCOL: u32 = 10;

/// The protocol that brought `Request::Duties` (paper-research N-568 part 6; the night's E3): the window asks for
/// the duties that hold only of a coordd whose hello says this or more. Fixed at its merge as 13.
pub const DUTIES_PROTOCOL: u32 = 13;

/// A duty that holds, as the interpreter answers it (`Request::Duties`; docket-edge6's "needs you", N-568.p20): DUTY
/// with its OBJECTS (its fields, holder and claimant among them), its HOLDER and CLAIMANT, whether it is VIOLATED
/// (said, not prevented), and the acts that end it (ENDED_BY, its `Terminated by`): the row offers those.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct DutyAnswer {
    pub duty: String,
    #[serde(default)]
    pub objects: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimant: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub violated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ended_by: Vec<String>,
}

/// The protocol that brought ListDir's git states (paper-research N-607, the owner's N-606.p1): the window asks with
/// `git: true` only of a coordd whose hello says this or more, and reads each entry's `git` (`DirEntry`). Fixed at its
/// merge as 12.
pub const FILE_STATES_PROTOCOL: u32 = 12;

/// One entry of a `ListDir` answer (the answer is a JSON array of them). OUTSIDE: it would not open (a link out of the
/// roots, a credential place). GIT, asked with `git: true` (`FILE_STATES_PROTOCOL`): `"new"`, untracked and not
/// ignored; `"ignored"`, ignored by git (build output, and a hand-written file a project keeps out of git alike: the window
/// says which it shows them as; consult-3, N-581.p10). None: tracked, a folder git reports neither way as a whole, a
/// folder outside a repository, or git not there or failing. A value the window does not know reads as none.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct DirEntry {
    pub name: String,
    pub dir: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub outside: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
}

/// The protocol that brings [`Request::RawPage`] (N-802): the raw pane asks coordd for its lines, on Windows and on
/// Linux alike; below it the Linux window reads the proxy's log itself (`RawSource`) and the Windows one says that it
/// needs a newer server release. 23: it missed train 7, whose coordd is at 22 and does not serve it (N-802.p12).
pub const RAW_PAGE_PROTOCOL: u32 = 23;

/// The protocol that brings [`Request::HarnessSet`] (N-802.p23): the Harnesses page's switches are set by coordd, the
/// one writer of the proxy's rules file, from the window on Windows and on Linux alike. Below it the Windows window
/// says that it needs a newer server release, and the Linux window writes the file itself as before.
pub const RULES_PROTOCOL: u32 = 24;

/// The protocol that brings [`Request::EscAll`] (line 200, N-1324; 27 from its merge into the 0.2 train, after MODE 25 and WHEREABOUTS 26): the window offers
/// Esc all only when the other side's hello says this or more.
pub const ESC_ALL_PROTOCOL: u32 = 27;

/// The protocol that brings his archive taken live ([`Request::Archive`] with `on: true`; the owner N-1073, N-867 p21,
/// p34): the window offers `Archive` only to a coordd whose hello says this or more; below it the release before reads an
/// archive only in a record it replays and refuses his, in words. The views' `archived` and `archived_under`, and
/// `FindHit.archived`, are read at any number: they are skipped when unset. 30 from its merge into the 0.2 train
/// (Archive B 988604f5, read right consult-3 N-1396 p114; 25 in its head).
pub const ARCHIVE_PROTOCOL: u32 = 30;

/// The protocol that brings [`Request::MailHold`] and `Snapshot.mail_hold` (line 327, N-1324; 32 from its merge into
/// the 0.2 train, after ARCHIVE 30 and ACCOUNT_RESET 31): the window offers Hold and Release only when the other side's
/// hello says this or more.
pub const MAIL_HOLD_PROTOCOL: u32 = 32;

/// The protocol that brings [`Request::Brain`] and `AgentView.brain` (proto-lead N-1446 p857.2; the window half,
/// proto-window N-1475): the agent's card shows its Brain switch only when the other side's hello says this or more.
pub const BRAIN_PROTOCOL: u32 = 37;

/// The protocol that brings [`Request::Machines`] (here-2's N-1136 for the owner's N-989 post 48; the devices on Home,
/// docket-edge7 for his N-1104 p1070): the devices and their numbers from one file coordd reads by a fixed name; below
/// it Home leaves the devices out. 56, after H-1591's 55, numbered at its merge.
pub const MACHINES_PROTOCOL: u32 = 56;

/// A model × provider pair of the proxy's harness rules (`Request::HarnessSet`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RulePair {
    pub model: String,
    pub provider: String,
}

/// The cells past the cursor asked and the next cursor; RESTARTED: the file is new, drop what was held.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageCells {
    pub cells: Vec<usage_rollup::Cell>,
    pub next: u64,
    #[serde(default)]
    pub restarted: bool,
}

/// The protocol that brought `Request::Post`'s `on` and the view's `Post::on` (one message on several threads,
/// paper-research N-695.p1, 2; docket-edge4 N-725): the window offers "Also on…" only of a coordd whose hello says this
/// or more. Fixed at its merge as 19.
pub const POSTED_ON_PROTOCOL: u32 = 19;

/// One act instance as the norms stand now (`Request::Enabled`): ACT with its OBJECTS (its fields after the actor; a
/// thread field is the thing asked about), whether it is ENABLED, and WHY not when it is not (the `Conditioned by`
/// lines that fail): a driver's menu entry, or its greyed entry and why.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ActAnswer {
    pub act: String,
    #[serde(default)]
    pub objects: Vec<String>,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// Not enabled: the rule's own words, the `//` comment right above the first failing `Conditioned by` line, or above
    /// the act when that line has none (consult-4, paper-research N-587.p8; docket-edge6 N-747): what a greyed entry
    /// shows, `why` kept for the "Do it anyway" dialog. None when enabled or when the policy says nothing there. An
    /// older window ignores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub says: Option<String>,
}

/// The bounds of an agent's compaction point (`Request::Compact`), in tokens: Claude Code's own for `--autocompact`
/// and `/autocompact` (it caps a point at the model's window).
pub const COMPACT_MIN: u64 = 100_000;
pub const COMPACT_MAX: u64 = 1_000_000;

/// The first line of each side of the bridge, as `{"hello": Hello}`: the protocol it speaks and the oldest it still
/// speaks, and its release. The bridge's names the folder its coordc runs from, and says why it ends when it refuses
/// the app (`refused`), after which it exits. The window's names the build it runs (its `YAAIHH_RELEASE`), in its
/// first hello and in a second one once it has taken over from an older window; coordc hands each such name to coordd
/// (`Request::WindowRelease`; I-1457), with the time it gives (`since`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Hello {
    pub protocol: u32,
    pub min: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
    /// The window's, with its release: when it became his window, by its own clock, in UTC as the record writes a
    /// time. That is its start, or for a window that took over from an older one, the "go" it was shown at
    /// (yaaihh restart.rs). So the `window_release` line says when his window had the build, and not when coordd
    /// heard of it: a build that carries an engine starts coordd again as it swaps, and the line came 3 to 8 minutes
    /// late on 21 Nov, the next bridge's hello after coordd's start (paper-research N-1446 p1392; proto-window p1410).
    /// None from an older window, and in a hello that names no release.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
}

impl Hello {
    /// This build's hello: PROTOCOL, PROTOCOL_MIN, and RELEASE.
    pub fn ours(release: Option<String>) -> Hello {
        Hello { protocol: PROTOCOL, min: PROTOCOL_MIN, release, refused: None, since: None }
    }

    /// Why this build cannot speak with a side that said OTHER (none: it can).
    pub fn mismatch(other: &Hello) -> Option<String> {
        if other.protocol < PROTOCOL_MIN {
            return Some(format!("the other side speaks protocol {}, older than the oldest this one speaks ({PROTOCOL_MIN}): update it", other.protocol));
        }
        if other.min > PROTOCOL {
            return Some(format!("the other side needs protocol {} or newer, and this one speaks {PROTOCOL}: update this one", other.min));
        }
        None
    }
}

/// One running tool call of an agent (from its pre hook until its post hook).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Tool {
    pub id: String,
    pub tool: String,
    pub what: String,
    pub at: String,
    /// What the call says it does, in its own words: its `description` when the tool gives one (Bash, Task), cut to one
    /// line (paper-research N-516.p3). None: it gives none, and the window draws the tool and `what`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
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
    /// His system-prompt edits that fall in that transcript, all of them whatever the page (docket-machine
    /// `Builder::restarted`; path fix 6 point 3, accounts-lead N-1446 p2524): where the agent's history says "System
    /// prompt changed", and, made from a turn, the old line beside it, its session given as its transcript's path.
    /// From [`TRANSCRIPT_SESSION_PROTOCOL`]; none from an older coordd.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restarted: Vec<docket_machine::graph::Restarted>,
}

/// Where the model proxy's log of an agent's requests is (the raw pane, N-40): the project file's `proxy_log` as it
/// is written there (`~/` and absolute paths are the app's to read, on the host, with its own home), and the agent's
/// Claude Code session id, which the proxy logs as X-Claude-Code-Session-Id. coordd never opens the log: a box does
/// not see the folder, and must not, since each file's HEADERS hold the account's keys.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RawSource {
    pub dir: String,
    pub session: String,
}

/// One of an account's usage windows (5 hours, a week), as its harness last said it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LimitWindow {
    /// Per cent of the window used, 0 to 100.
    pub used_percent: f64,
    /// When it starts again, seconds since the epoch; [`NOT_STARTED`] for a window not used yet, which has not begun.
    pub resets_at: i64,
}

/// [`LimitWindow::resets_at`] of a window not used yet: it begins at its first use, so no time says when it starts
/// again (t3code's reading of an Antigravity window, `resetsAt` null; paper-research N-1446 p2464). Never a running
/// window: none is over before it or out for it.
pub const NOT_STARTED: i64 = 0;

/// One of a sign-in account's weeks of one model, as Claude Code's `/usage` names it beside the weekly of all models
/// (`Current week (Fable)`, `Current week (Sonnet only)`): its MODEL in those words, and its window. Shown on the card
/// (N-1437: the gateway's card has its Fable weekly row); the account's own limits are its 5-hour and weekly. An agy
/// sign-in's windows are each of some models (its families': "Claude & GPT (shared) · 5-hour", "Gemini · Weekly"),
/// so all of them are these, MODEL t3code's label, which names the window's length.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelWeek {
    pub model: String,
    pub window: LimitWindow,
}

/// One model an agy account is out for (docket-core N-1446 p2543): a turn of it was refused by its limit, and the account
/// still takes its other models. t3code's windows are each of a family of models and name none of them, so only a
/// refused turn says which model is out. MODEL as the account's `models` name it (`agy/gemini-3.8-flash-high`); until
/// when (seconds: the time the refusal named, else coordd's rule for an out with none named, its `limits::UNKNOWN_RESET`
/// with no window to take one from); since when (seconds); the refusal's words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelOut {
    pub model: String,
    pub until: i64,
    pub since: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<String>,
}

/// The owner's accounts of the native path (N-412 ask 12) as one coordd sees them, for the app's Accounts page
/// (`Request::Accounts { json: true }`). Accounts are shared by all of the owner's coordds; the usage rows are each
/// coord's own, so `usage` is this coord's share and the app adds up the shares of the coords it is linked to.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NativeAccounts {
    /// This coord's folder name (the `COORD` of `bound/COORD.AGENT`).
    pub coord: String,
    /// The month `usage` covers, `YYYY-MM`.
    pub month: String,
    pub accounts: Vec<NativeAccount>,
    /// This coord's switch `All agents use my accounts` as last set; None: never set here, which is off.
    #[serde(default)]
    pub all: Option<AllAgents>,
    /// This coord has the gateway: an agent whose account is out, with no other to take it, goes on through it until
    /// one is back (`NativeAccount::on_gateway`). Without it such an agent waits.
    #[serde(default)]
    pub gateway: bool,
    /// The accounts the owner removed, newest first: their folders are set aside whole, and their ids are never an
    /// account's again (a new account whose id would be one of them is given the next free one, `work-2`).
    #[serde(default)]
    pub removed: Vec<RemovedAccount>,
}

/// The agent name coordd's own calls on a key account would be bound and counted under. **Nothing uses it:** coordd's
/// own calls run on Codex first (the owner, 10 Nov; paper-research, N-591 post 42); the fallback's Claude model runs
/// where the router puts it, one of his sign-in accounts among them, binding nothing, and never on a key account
/// (coordd's `accounts::router`; each call's account is in its decision's [`Attempt::account`]). Kept, with
/// [`NativeAccount::own_calls`], for the window built against it.
pub const OWN_CALLS: &str = "coord";

/// An account the owner removed (`Request::AccountRemove`), as its set-aside folder says.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RemovedAccount {
    /// The harness it belonged to (`claude`).
    pub harness: String,
    pub id: String,
    pub label: String,
    /// The set-aside folder's name, `ID.STAMP`: what its usage rows from before the removal are kept under.
    pub name: String,
    /// When it was removed (seconds); 0 when the folder's name does not say.
    pub at: i64,
    /// Claude Code has not signed this account out in its set-aside folder: its sign-out failed (no network), or
    /// coordd ended before it ran. The folder may still hold a login, so the line says "not signed out". Never set
    /// for a key account, which has no sign-in. From [`ACCOUNT_LEAVE`]; an older coordd never says it.
    #[serde(default)]
    pub not_signed_out: bool,
}

/// The protocol that brings [`Request::AccountsAll`] and what the page reads of it ([`NativeAccounts::all`],
/// [`NativeAccounts::gateway`], [`NativeAccount::on_gateway`]; the owner, N-613; paper-research, N-614).
/// Fixed at its merge as 15.
pub const ACCOUNTS_ALL: u32 = 15;

/// One coordd's switch `All agents use my accounts` (its registry's last `native_all` line): every Claude Code agent
/// of the coord runs on the owner's accounts (ON) or through the gateway, and new agents start there. A single agent
/// can be put the other way after it (`Request::Native`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AllAgents {
    pub on: bool,
    /// When it was set (seconds).
    pub at: i64,
    /// It turned on by itself, at the first sight of an account that had signed in and answered its check: that
    /// account's label. None: the owner set it.
    pub by_account: Option<String>,
}

/// One account: what the owner says of it, what coordd last saw of it, its agents, and this coord's month of it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NativeAccount {
    /// The harness it belongs to (`claude`).
    pub harness: String,
    pub id: String,
    pub label: String,
    /// The Accounts page's `Priority` number: a larger one is taken first; accounts of one number are one tier.
    pub tier: i64,
    /// It runs on an API key in its folder, not on a sign-in: `signed_in` then says whether the key is set, and it has
    /// no limit windows.
    #[serde(default)]
    pub key: bool,
    /// When a key account's key file was last written (seconds), for "Key set 18 Nov"; None without a key.
    #[serde(default)]
    pub key_since: Option<i64>,
    /// The harness's own answer to "is it signed in", and when it was asked (seconds).
    pub signed_in: bool,
    /// A sign-in of it runs in this coordd now (Claude Code's own command, waiting for the browser or the code):
    /// `Request::AccountSignin` answers its URL again, nothing is started twice.
    #[serde(default)]
    pub signin_running: bool,
    /// An opencode account's provider, as opencode names it (`opencode-go`); None for Claude Code's (from
    /// [`OPENCODE_ACCOUNTS`]).
    #[serde(default)]
    pub provider: Option<String>,
    /// An OpenCode Go account whose key coordd keeps to read Go's usage with, and for nothing else (the owner, N-1104
    /// p808; from [`GO_USAGE_PROTOCOL`] on): its card's `Check limits` reads it. False on every other account.
    #[serde(default)]
    pub usage_key: bool,
    pub signed_in_seen: i64,
    /// When the login runs out, if the harness said (seconds).
    pub login_expires: Option<i64>,
    /// Its plan as Claude Code names it (`claude auth status`'s `subscriptionType`: `max`, `pro`, `team`), when it said
    /// one: the card's "Plan max", as the gateway's card says its logins' plan. None on a key account and an opencode one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    /// The address of the person signed in on it, as Claude Code's `auth status` says it for a claude.ai login: whom his
    /// browser signed in, which the gateway's Move compares with the login it moves (N-1446 p2952). None while signed
    /// out, on another harness's account, and from a coordd before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_in_as: Option<String>,
    /// Claude Code's `/usage` said its requests run on extra usage (paid past the plan's limits: "You are currently
    /// using your overages to power your Claude Code usage. We will automatically switch you back to your subscription
    /// rate limits when they reset"): until when (seconds), the reset of the window it used up. None when it did not, or
    /// that time has passed. Only said on the card: an account at its limit is out all the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_usage_until: Option<i64>,
    pub five_hour: Option<LimitWindow>,
    pub seven_day: Option<LimitWindow>,
    /// An OpenCode Go account's month, as Go's usage page last named it (from [`GO_USAGE_PROTOCOL`] on;
    /// its 5-hour and weekly are `five_hour` and `seven_day`, `limits_from` then `go usage`). None on every other
    /// account: Claude Code names no month.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub month: Option<LimitWindow>,
    /// Its weeks of one model, as `/usage` last named them, while they run (from [`ACCOUNT_EDIT`] on). Empty until a
    /// `/usage` read (the card's `Check limits`, the window opener's look): a session's status line has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model_weeks: Vec<ModelWeek>,
    /// When `/usage` last named them (seconds), for the card's "as of": nothing else says them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_weeks_seen: Option<i64>,
    /// The models an agy account is out for while their time runs ([`ModelOut`]): the card says each. When all of
    /// the models it offers are out, `out_until` says so for the account as well. Empty on every other account, and
    /// from a coordd before it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models_out: Vec<ModelOut>,
    /// How its harness says it signed in, an agy account's as t3code names it (`oauth-personal`, `oauth-business`), for
    /// the card's kind line; None from a coordd before it, or not signed in. A kind of sign-in, never any of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<String>,
    /// An agy sign-in account's way in, by coordd's look at its folder: `agy`, signed in by agy's own sign-in (every
    /// sign-in made since this field), or `t3code`, signed in by t3code before it and not since. The window chooses
    /// its words by it, and names no program without it. None on every other account, and from a coordd before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_in_by: Option<String>,
    /// When the windows were last seen (seconds), and from what.
    pub limits_seen: i64,
    pub limits_from: String,
    /// It takes no request until then (seconds); None or past: it can.
    pub out_until: Option<i64>,
    /// While it is out: why, as Claude Code's failed turn named it (`rate_limit`; for a key account also
    /// `authentication_failed`, its key was refused, and `billing_error`, no credit: those two only the owner
    /// clears), since when (seconds), and the words Claude Code showed. None when no signal said.
    #[serde(default)]
    pub out_for: Option<String>,
    #[serde(default)]
    pub out_since: Option<i64>,
    #[serde(default)]
    pub out_words: Option<String>,
    /// A key account out for `rate_limit` with no time named: when the policy takes it again (seconds;
    /// `key-again-after`). None: not such an account, or the policy gives no time and the owner clears it.
    #[serde(default)]
    pub again_at: Option<i64>,
    pub agents: Vec<NativeAgent>,
    /// Always empty: a call coordd runs on an account binds nothing and is not listed here; its decision's
    /// [`Attempt::account`] says where it ran ([`OWN_CALLS`]). The field stays for the window built against it.
    #[serde(default)]
    pub own_calls: Vec<String>,
    /// It was removed and put back then (seconds) and has not signed in since: the removal had Claude Code sign it
    /// out, so the card says to sign in again. None otherwise.
    #[serde(default)]
    pub put_back: Option<i64>,
    /// The owner asked for its removal then (seconds) and agents still run on it: it takes no agent, each of its
    /// agents moves at the end of its turn, and it is removed when none is left (`Request::AccountRemove`). The card
    /// says so and offers to keep it (`Request::AccountKeep`). None otherwise.
    #[serde(default)]
    pub leaving: Option<i64>,
    /// The owner turned it off then (seconds; `Request::AccountEdit` with `off`): it takes no agent, and its agents
    /// move at the end of their turn. None while it is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub off: Option<i64>,
    /// The coord whose coordd holds this account's lock at the moment, when it is another than the one asked: a
    /// click on the card that is not answered yet waits for it ("waiting for RESEARCH's coordd to finish with this
    /// account"). None when nobody holds it, which is nearly always.
    #[serde(default)]
    pub locked_by: Option<String>,
    /// This coord's agents that left it when its limit was met and no other account could take them, and run
    /// through the gateway until one is back (`since`: when they went).
    #[serde(default)]
    pub on_gateway: Vec<NativeAgent>,
    /// This coord's requests on it in `month`, per model that answered.
    pub usage: Vec<AccountUsage>,
    /// The resets Claude Code offers it now, one `Use` each on the card ([`ACCOUNT_RESET`]). Given only at its limit (out
    /// for it, or a window last read used up: a reset is for a limit met): the kinds of [`NativeAccount::held`]; when
    /// Claude Code has listed none, `Fable reset` and `full reset` with no count, its own confirm then saying what it
    /// holds ([`ACCOUNT_EDIT`]). A kind it did not apply at this limit is left out until the limit ends or it lists its
    /// resets again; one its chooser said it cannot use at this limit comes with that [`ResetOffer::why`] and takes no
    /// `Use`. Empty on a key account and an opencode one.
    #[serde(default)]
    pub resets: Vec<ResetOffer>,
    /// The resets Claude Code last listed as held, at any time, for the card's lines (`2 left · use by Nov 27`): its
    /// `/limit-reset` list read on the owner's `Check limits` (Claude Code alone in the account's folder, answered with
    /// Escape: nothing is chosen or used there), or the notice an agent's screen showed at a limit (`/limit-reset to use
    /// your Fable reset · 2 left · use by Nov 30`). Those past their `use_by`, and a kind not applied at this limit,
    /// are left out. Only [`NativeAccount::resets`] take `Use` ([`ACCOUNT_EDIT`]). Empty when none was listed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub held: Vec<ResetOffer>,
    /// When Claude Code last listed them (seconds), for the card's "as of".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held_seen: Option<i64>,
    /// The owner's last press of one of them ([`Request::AccountReset`]): while it is applied, and how it went (a press
    /// whose session said nothing within 90 s is given as refused, in those words).
    #[serde(default)]
    pub reset: Option<ResetTry>,
    /// Messages do not reach its sessions: two deliveries handed to sessions on it were never seen there, with none
    /// seen between, the second handed to this agent (on 20 Nov a Team seat whose organisation has channels off).
    /// It takes no agent and its agents move, until a delivery enters there after all, it is signed in again, or the
    /// owner clears it. None while messages are known to arrive, or not known yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deaf: Option<String>,
    /// The models its harness offers on it, as an agent's model names them (`agy/gemini-…`; [`AGY_ACCOUNTS`]): an agy
    /// account's, as t3code or agy last named them at its check. Empty on every other account.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
}

/// A reset Claude Code offers an account ([`NativeAccount::resets`]), in its words.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResetOffer {
    /// Claude Code's own word for it, shown as given ("Fable reset", "full reset", ...); sent back unchanged as
    /// [`Request::AccountReset`]'s KIND.
    pub kind: String,
    /// How many of it are left ("{left} left"), when it said.
    #[serde(default)]
    pub left: Option<u32>,
    /// Until when it can be used ("use by {date}"), seconds since the epoch, when it said.
    #[serde(default)]
    pub use_by: Option<i64>,
    /// Why Claude Code cannot use it at the limit the account is at, in its words, as its `/limit-reset` chooser draws
    /// such a kind ("Full reset · can't be used while you're at your 5-hour limit", "Fable reset · doesn't refill
    /// your weekly limit"): the card shows its `Use` dimmed with them. None: nothing said it cannot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// What using it gives back, in Claude Code's words: its confirm's body without the weekly day ("Refills your Fable
    /// and 5-hour limit now · returns what Fable used to your weekly limit"), or its chooser's note ("frees up only
    /// what Fable used this week"). None when neither was shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gives: Option<String>,
}

/// The owner's last press of a reset's `Use` on an account ([`NativeAccount::reset`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResetTry {
    /// The offer's KIND, as pressed.
    pub kind: String,
    /// When he pressed (seconds).
    pub since: i64,
    /// Claude Code's refusal in its words: that kind's `Use` is not offered again at this limit, unless it says to try
    /// again ([`ResetTry::again`]). None while it is applied.
    #[serde(default)]
    pub refused: Option<String>,
    /// Claude Code's words once it was applied ("Fable reset used · your weekly reset day stays Monday · 1 Fable
    /// reset left"). None while it is applied, and when it was refused.
    #[serde(default)]
    pub done: Option<String>,
    /// It is not known to have been used, and the same kind may be pressed again: Claude Code could not confirm it went
    /// through, or could not reset and used nothing, and says to try again ("Couldn't confirm the reset went through ·
    /// if you're still at the limit in a moment, try again", "Couldn't reset your limits · nothing was used · try again
    /// in a moment"), or its session ended before Claude Code said how it went. `refused` holds those words; the
    /// kind stays offered, and the card's button says `Try again`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub again: bool,
    /// Claude Code refused it because this login must sign in again first ("Couldn't reset your limits with this
    /// login · run /login, then try again"): the card's `Sign in` is in sight, as for a login refused, until he has
    /// signed it in again (proto-reader I-1469 p91).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub login: bool,
}

/// `coord account key`'s refusal anywhere but in a terminal of the owner's own: in an agent's session or terminal the
/// keystrokes would pass through that agent. Said by coordc (`AGENT_NAME` set) and by coordd (the caller's cgroup).
pub const KEY_NOT_HERE: &str = "coord account key: type this in a terminal of your own, not in an agent's";

/// The protocol from which coordd makes opencode accounts and signs them in with opencode's own sign-in
/// (`AccountNew.harness` and `provider`, `AccountSignin.method`; the key field's `AccountKeyStore` on such an
/// account is written to opencode's key prompt, not to a file), and the page reads `NativeAccount.provider`. An
/// older coordd would read such an `AccountNew` as a Claude Code account's, so the window offers the tile only from
/// this one on. 29 from its merge into the 0.2 train (docket-core, N-1396 post 22: MODE 25, WHEREABOUTS 26, ESC_ALL 27,
/// GROUPS 28).
pub const OPENCODE_ACCOUNTS: u32 = 29;

/// The hosts an opencode sign-in's link (its `Go to: URL`) may go to for the window to open it (docket-core3,
/// N-591 post 121, 4): the ChatGPT way's (opencode 1.18.34, plugin/openai/codex.ts). Any other link, or one not
/// `https`, is shown as text and not opened.
pub const OPENCODE_SIGN_IN_HOSTS: &[&str] = &["auth.openai.com"];

/// Whether URL is an opencode sign-in link the window opens: `https`, to a host of [`OPENCODE_SIGN_IN_HOSTS`]
/// exactly (no user part, no other port).
pub fn opencode_link_opens(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    OPENCODE_SIGN_IN_HOSTS.contains(&host)
}

/// The protocol from which coordd takes the window's key field (`Request::AccountKeyStore`) and the page says when a
/// key was set (`NativeAccount::key_since`). Fixed at its merge as 11.
pub const ACCOUNT_KEY: u32 = 11;

/// A secret on its one way through a request: the API key the owner types into the window's key field
/// (`Request::AccountKeyStore`). Whatever prints a request for debugging prints `Secret(…)` for it; it has no
/// `Display`, compares with nothing, and its bytes are overwritten when it is dropped. On the wire it is the plain
/// string, as it must be to arrive.
#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(secret: String) -> Self {
        Secret(secret)
    }

    /// The secret itself, for the one place that writes it where it belongs.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("Secret(…)")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        let mut bytes = std::mem::take(&mut self.0).into_bytes();
        bytes.fill(0);
        drop(std::hint::black_box(bytes));
    }
}

/// What coordc asks of a key account's key file (`Request::AccountKey`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyStep {
    /// Where the file is: the reply's `out` is its path.
    Where,
    /// coordc has written it: the account can take agents.
    Set,
    /// coordc has removed it: the account takes none.
    Removed,
}

/// An agent bound to an account, of any of the owner's coords.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NativeAgent {
    pub coord: String,
    pub agent: String,
    /// Since when (RFC 3339).
    pub since: String,
    /// For this coord's agents: that it waits for an account, or moves once not in a turn, and why.
    pub note: Option<String>,
}

/// One model's requests on an account in a month, from the usage rows (`accounts::usage`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AccountUsage {
    pub model: String,
    pub requests: u64,
    /// Failed requests (telemetry's `api_error`) on the account with this model.
    pub failed: u64,
    pub tokens: Tokens,
    /// The 1-hour part of `tokens.cache_write`.
    pub cache_write_1h: u64,
    /// The summed duration of the requests whose telemetry said one, and how many did: an average is the one over
    /// the other.
    pub duration_ms: u64,
    pub timed: u64,
}

/// What coordd gives for [`Request::DryRun`]: the step whose arguments fill the prompt, and its text as that step's
/// agent would get it (its call's prompt in the agent's `step` rendering, with no posts: those unread when it is sent
/// come with it); or, with ON none, no ticket waits on a step of the call.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct DryRunAnswer {
    /// The ticket whose waiting step fills it (`N-12`); none: no ticket waits on a step of the call now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
    /// That step's number, and the agent it waits on.
    #[serde(default)]
    pub step: u32,
    #[serde(default)]
    pub by: String,
    /// Whether ON is the ticket asked about; false: the newest open ticket waiting on a step of the call.
    #[serde(default)]
    pub asked: bool,
    /// Whether that step has a prompt of the owner's own for it (his prompt for that thread, or his text at its halt),
    /// which a draft of the program does not change: the text is that.
    #[serde(default)]
    pub own: bool,
    /// The text as its agent would get it.
    #[serde(default)]
    pub text: String,
}

/// What coordd gives for [`Request::Graph`]: the slice of the one graph, and each of its nodes' labels as
/// `Request::Label` reads them (one source: coordd's `labels`, the commits' subjects from git).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct GraphAnswer {
    pub slice: docket_machine::graph_slice::Slice,
    pub labels: Vec<Label>,
    /// What is worked on now and what is not, at coordd's clock, when the ask asked it (`Ask::now`; `Graph::now`): each
    /// agent's turn and last job, and his asks not yet in his window, the canvas's right edge. Its asks' posts are
    /// labelled in LABELS too, after the slice's nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now: Option<docket_machine::graph::Now>,
    /// Around his post (`Ask::around`), its course as the graph reaches it (`Graph::ask_times`): asked, reached, taken,
    /// the first commit, merged, the first green, in his window, which the page draws as the spans between them
    /// (paper-research N-1446 p1429, 1). None around any other node, or one the graph does not hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub course: Option<docket_machine::graph::AskTimes>,
    /// The folders where agents worked, as the record names them, that are gone or whose repository git no longer
    /// reads, each by the topmost of its folders gone: git is read in none of them, nor are they put under another
    /// repository (paper-research N-1446 p1455).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gone: Vec<String>,
    /// An agent's folder, when the ask names one (`Ask::folder`; the file view, paper-research N-1446 p1441), given
    /// alone, with no slice: docket-machine `Graph::folder`'s rows, counted in coordd. None from a coordd before
    /// [`FOLDER_PROTOCOL`]. Its system prompt's rows the same way, when the ask names that (`Ask::prompt`,
    /// `Graph::prompt`), from [`PROMPT_PROTOCOL`] on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<Vec<docket_machine::graph_slice::Row>>,
}

/// A reference's label (`Request::Label`): ID as asked, what it is (ticket, step, post, agent, commit; empty when
/// nothing is known by that id), its LABEL, and for a step or a post the label of the ticket it is on.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Label {
    pub id: String,
    pub is: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
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
    /// The sidebar's part it is shown in (N-412 ask 2): its `part(agent, name)` in the policy, "/" nesting a part under
    /// another. Empty: no such fact (the app puts it in "Pool", or alone above the parts).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub part: String,
    /// The ticket it is on (docket-edge6 N-447 p48): its id, as `label::agent_on` gives it, the one its label names (it
    /// holds a step there now, else an open one it was named in, else a closed one, highest number). None: on none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
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
    /// Its current transcript's size in bytes, from a stat at most every 30 s. None: no transcript, or an older coordd.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_bytes: Option<u64>,
    /// The bytes in its own folder (`agents/NAME`, links not followed), read with `transcript_bytes`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_bytes: Option<u64>,
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
    /// Why it has no box and is not started (a boxed project never starts an agent outside its sandbox; `coord ls`'s
    /// "not started, no box: WHY"). Docket-edge N-236: coordd fills it from the agent's `no_box`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_box: Option<String>,
    /// Where it stands (coordd's moves, N-101 p414): a layer's id, the line's view (`line@...`); none: the project's
    /// own folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stands: Option<String>,
    /// Its last move between views, if it was ever moved (docket-edge N-238).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moved: Option<AgentMoved>,
    /// How full its context is (paper-research N-101 p1259 point 5): a Claude Code agent's latest request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<AgentContext>,
    /// Since when a compaction of its context runs (its harness said one began; over at the harness's word, a new
    /// prompt or the turn's end). In a field of its own: `state` stays its turn's, so whatever asks whether a turn
    /// runs holds while it compacts (the owner, N-541 post 35: "it just says in a turn state should show as
    /// compactions"; until E5 the state itself read `compacting`, and each place that asked for `working` lost the
    /// turn for that time).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compacting: Option<String>,
    /// The account it runs on (N-412 ask 12, the native path: its harness signed in to the owner's own account, nothing
    /// of ours in the path): the account's id; an opencode agent's is the opencode account it is bound to, from
    /// [`GO_USAGE_PROTOCOL`] (before, none). None: it goes through the project's model route, as every agent did,
    /// or it is on the owner's accounts (`native`) and runs on none now: it waits for one, or goes on through the
    /// gateway while every account is out (`account_note` says which).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// The owner put it on his accounts (the switch, or the agent's own `My accounts`), whether or not it runs on one
    /// at the moment (protocol [`ACCOUNTS_ALL`]; before it, `account` being there said so).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub native: bool,
    /// What is due about its account, in words: a move when its turn ends, or that every account is out and until
    /// when (the owner: when every account is out the agent waits and says so).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_note: Option<String>,
    /// The permission mode its running session was started with (from [`MODE_PROTOCOL`]): `yolo`, Claude Code's own
    /// bypass of permission prompts; none, safe: his own permission mode, as every host session ran before. Read from
    /// the session's own command line at its start and kept for that process, so it changes only when a new process
    /// takes the change, and a failed restart (or coordd's own restart after one) still reads what runs (consult-3,
    /// N-1400 p5, p7). A stopped agent's is its next start's. A Claude Code agent's only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// The mode the owner asked for (`Request::Mode`) that its running session was not started in, `safe` or `yolo`:
    /// its restart into it runs, waits for its turn's end, or failed (then its next start takes it). None when they
    /// agree. From [`MODE_PROTOCOL`] (hs-ls, N-1400 p6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode_due: Option<String>,
    /// Where its mode comes from: `agent` (set for it), `project` (the project file's `agent_mode`) or `yaaihh`
    /// (`yaaihh.toml`'s, for every project). None: nothing set it, so Safe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode_from: Option<String>,
    /// A restart of its session runs now (a new mode, effort, model or account): from coordd sending it until the new
    /// session is up or the try is over. Not while the restart only waits for its turn's end. From [`MODE_PROTOCOL`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub restarting: bool,
    /// The safety check its running Claude Code session was started with (`safety_check` the next start's): on, or
    /// off when its start lifted it. None while it does not run, when coordd could not read it, and from an older coordd.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_check_runs: Option<bool>,
    /// Where it runs now came from its last move (the registry's last `account` line of it; 0.2 list, line 22: "the
    /// window says which agent went where and why"): why, in coordd's words ("its account does not take messages",
    /// "the placement policy: no space at its plan's weekly ceiling", "the owner moved it"), since when (RFC 3339),
    /// and the account it came from (its id; None from no account). None of the three before its first move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_why: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_since: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_from: Option<String>,
    /// When it next looks for an account, if a time is known (seconds): on the gateway, when it looks again; waiting
    /// with every account out, when the first is back. None when no time is known or nothing waits (consult-3, N-412
    /// post 2122: the cell reads this, not `account_note`'s words).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_until: Option<i64>,
    /// It does not run and waits for an account that can take it, which starts it (`account_until` says when, if a
    /// time is known); the cell's `waiting` (accounts-2, N-1398 post 17: not read out of `account_note`'s words).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub account_waiting: bool,
    /// Its last turn stopped on a rate limit that is not its account's, and nothing is handed to it till that resets
    /// (coordd's `Live::limit`, N-1397 p142): when it is given its turn again (seconds, as `account_until`), and the
    /// 429's own words; still set once that time has passed and the turn waits on something else (his mail hold). So
    /// its row says so, not "idle since …" (proto-lead N-1397 p160). None while a turn runs, when it does not run,
    /// and from a coordd before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_until: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_why: Option<String>,
    /// Its delivery's last acts, the newest first (`Machine::last_acts`, up to 8): what each did, to which items, and
    /// why; what the Docket's act chips open (paper-research N-1396 p223, p247). None from a coordd before it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acted: Vec<docket_machine::ViewActed>,
    /// Its build and test jobs on the runner (N-1334; the owner, N-989 post 18: "the human needs to spinner or the
    /// completion bard just so they have something to look at"): those running, newest first, then the last five
    /// that ended. Read from the files the runner's caller side keeps, one per job; an older coordd sends none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<Job>,
    /// The background shells and subagents it started that still run, each with what it said it does and since
    /// when (N-1334): what a `waiting on ...` state waits on, in the call's own words. An older coordd sends none,
    /// and the state's own words stand.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub background: Vec<Tool>,
    /// Its brain (0.3, agent-base/kinds/brain.kind; from `BRAIN_PROTOCOL`): None when no binding opens `brain` for it
    /// (`Machine::brain_bound`; research's is in its own program, `kinds/agents/paper-research.kind`), so its card shows
    /// no switch; and from a coordd before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brain: Option<BrainView>,
    /// Its session takes no messages, in coordd's words with what he can do about it (paper-research N-1446 p2424,
    /// p2427: "pick an account for this agent"): Claude Code said on its screen that it ignores the coord channel (its
    /// holder's `Request::Busy` `channels_off`), its coord server answers on no channel socket, or what was handed to
    /// it, idle a minute, never reached its session. Under his post the agent's entry reads "not delivered", never
    /// "sent". None while it takes them, and from a coordd before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deaf: Option<String>,
    /// What he can do about `deaf`: each act with its words as they stand at the end of `deaf`, in that order, joined
    /// there by ", or " (proto-window N-1446 p2538), so the window makes each words a press. Empty when nothing is his
    /// ("it restarts to take them again"), and from a coordd before it: `deaf` is then words only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deaf_do: Vec<DeafDo>,
    /// A dialog stands on its terminal waiting for an answer, as its holder told (`Request::Popup`): its title. None
    /// when none stands, it does not run, and from a coordd before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialog: Option<String>,
    /// Its pace (N-1788): Run, what is for it handed as it comes; Step, the owner's turn by turn: his posts to it are
    /// neither held to gather nor ordered by a model, each staged as its next turn for him to approve or reject.
    #[serde(default)]
    pub control: AgentMode,
    /// What its harness can take (`Core::capabilities`, by its kind and where it runs): the window offers only these.
    #[serde(default)]
    pub capabilities: HarnessCapabilities,
    /// The harness switches its next start takes (`Core::switches_view`).
    #[serde(default)]
    pub switches: HarnessSwitchesView,
    /// Why the last apply of a change of his to it failed (a restart for his system prompt), in the words of what
    /// failed; none once one succeeds, and from a coordd before [`CONTROL_PROTOCOL`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply_error: Option<String>,
    /// The agent whose conversation it is a copy of: a spin-off (`roster fork`, the window's Spin off), which he ends with Join back, Close or Delete (`Request::SpinEnd`). None for any other agent, and from
    /// a coordd before [`SPIN_PROTOCOL`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Its reads outside its folders that wait on the owner's answer (`Request::ReadAnswer`), oldest first; none from
    /// a coordd before them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub read_asks: Vec<ReadAsk>,
}

/// An agent's read outside its folders that its harness would have asked a person about: its call waits on his answer.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadAsk {
    /// coordd's number for it, what his answer names.
    pub id: u64,
    /// The path it reads, as coordd judged it (absolute, `..` and links resolved).
    pub path: String,
    /// The folder "Allow read-only for this folder" would allow: none where only this path may be allowed (a path at
    /// the top of the home or the machine).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    pub at: String,
}

/// His answer to a [`ReadAsk`].
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReadAnswer {
    /// The ask's folder read-only for the agent from now on, kept in the project file's `[read.NAME]`.
    Folder,
    /// This call only.
    Once,
    Deny,
}

/// An agent's pace (N-1788): Run, its turns follow its mail as coordd hands it; Step, the owner's turn by turn.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    #[default]
    Run,
    Step,
}

/// An agent's harness switches as merged from the flavor's, project's and agent's `harness.toml`; FROM names each
/// key's level, a key not in it is at its default.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct HarnessSwitchesView {
    #[serde(default)]
    pub safety_check: bool,
    #[serde(default)]
    pub tools_off: Vec<String>,
    #[serde(default)]
    pub parts_off: Vec<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub from: std::collections::BTreeMap<String, String>,
}

/// What an agent's harness can take (N-1788), as coordd knows it from the harness's kind and where it runs. A window
/// draws a control only where its field is true; coordd refuses a request a field says the harness cannot take.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct HarnessCapabilities {
    /// A system prompt of his given from a file at its start (`Request::SystemPrompt`, appended to the harness's own).
    #[serde(default)]
    pub system_prompt_file: bool,
    /// His system prompt in place of the harness's own (`Request::SystemPrompt`'s `replace`).
    #[serde(default)]
    pub system_prompt_replace: bool,
    /// Tools turned off for it at its start (`Request::HarnessSwitches`' `tools_off`).
    #[serde(default)]
    pub tool_disabling: bool,
    /// Its harness's own safety check lifted (`Request::HarnessSwitches`' `safety_check`; Claude Code's on dangerous
    /// rm commands).
    #[serde(default)]
    pub safety_toggle: bool,
    /// Parts of its harness's own system prompt left out (`Request::HarnessSwitches`' `parts_off`).
    #[serde(default)]
    pub prompt_parts_off: bool,
    /// Its conversation branched at a past turn's end (a fork or a system prompt from a cut).
    #[serde(default)]
    pub turn_branching: bool,
}

/// [`DeafDo::act`] for picking an account for the agent (its page's Session panel, the Account row).
pub const DEAF_ACCOUNT: &str = "account";
/// [`DeafDo::act`] for restarting it (`Request::Restart`).
pub const DEAF_RESTART: &str = "restart";

/// One thing he can do about an agent whose session takes no messages (`AgentView::deaf_do`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeafDo {
    /// [`DEAF_ACCOUNT`] or [`DEAF_RESTART`]; a String, so an act a newer coordd names is no decode error.
    pub act: String,
    /// Its words, as they end `AgentView::deaf`.
    pub words: String,
}

impl DeafDo {
    /// Restart it, in coordd's words.
    pub fn restart() -> DeafDo {
        DeafDo { act: DEAF_RESTART.into(), words: "restart it".into() }
    }
}

/// An agent's brain as its card shows it (`AgentView.brain`): his switch, read from the record's `brain_switch` lines
/// (`Request::Brain`), and its last pass. With no line on the record it is on: the binding in the agent's program is
/// what turns it on, and the switch only stops its passes while off.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct BrainView {
    /// Whether new passes open: false from his press of Off until his press of On.
    pub on: bool,
    /// When he last pressed it (RFC 3339), none before his first press.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// When its last pass opened (RFC 3339), and that pass's ticket id, which the card opens in the Docket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_pass: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_id: Option<String>,
    /// Its passes' posts to the agent halted for his approval, oldest first: "brain: ready (N suggestions)", each let
    /// through by `Request::Approve` or dropped by `Request::HaltReject` ([`BRAIN_READY_PROTOCOL`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ready: Vec<BrainSuggestion>,
}

/// A brain pass's post halted for his approval: its ticket (number and id), the halt, the post and its first line.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct BrainSuggestion {
    pub ticket: u64,
    pub id: String,
    pub halt: u32,
    pub post: u32,
    pub words: String,
}

/// The project's owner model as coordd reads it from the record (coordd brain.rs `owner_model_read`, its one rule):
/// what the Map's node draws, with no rule of the window's own, so it says the same at any age of the record
/// (`Snapshot.owner_model`, `Body::OwnerModel`; `OWNER_MODEL_PROTOCOL`; proto-reader N-1446 p1212, B). None where no
/// node stands: none opened, or the lead stopped it.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct OwnerModelView {
    /// Its node's ticket id: the open `owner_model` ticket with the highest number below the hidden ones.
    pub node: String,
    /// Whether an update of it is open.
    pub running: bool,
    /// The model that stands: none while none does (none made yet, or every update since the node opened failed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelMade>,
    /// The newest closed update, when it made no model (its call failed, or its step was refused); none once a later
    /// one made one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<UpdateEnded>,
}

/// The update that made the model that stands (`OwnerModelView.model`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ModelMade {
    /// Its ticket id.
    pub update: String,
    /// The newest of his posts it read, as a post id (`N-1104.p700`), which the window names with its label.
    pub upto: String,
    /// When it closed (RFC 3339).
    pub at: String,
    /// The first line of its text that says something: not blank, not a heading, its markers taken off (coordd's rule,
    /// proto-window N-1446 p1227). What the inspector shows after "a model's reading of you, not your words"
    /// (paper-research p1209); the window only cuts it to fit.
    pub first_line: String,
}

/// An update that ended with no model (`OwnerModelView.failed`): its ticket id and when it closed (RFC 3339).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct UpdateEnded {
    pub update: String,
    pub at: String,
}

/// One job of an agent on the build runner (`AgentView.jobs`), as the file of its caller's side says it
/// (`~/agent-coord/build-runner/jobs/`, written by `build-runner`): what is built, where it stands, and how far,
/// when the thing itself gives a count. No command line is in it.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Job {
    /// The file's name without `.json`.
    pub id: String,
    /// `test`, `build`, `check`, `clippy`.
    pub verb: String,
    /// The packages named (`-p`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<String>,
    /// A target other than the machine's own (the Windows exe's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// The build machine it was sent to; none: the runner on this machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    /// When its caller began.
    pub since: String,
    /// `unreachable` (its machine does not answer, as kpi-data's file of the machines says), `sending`, `queued`,
    /// `compiling`, `testing`, `back` (its files come back), `ended`, or `cut`: its caller went with its agent's
    /// session before its end (the session restarted since the job began, or stopped).
    pub step: String,
    /// Since when it is in that step; for `ended` and `cut`, when that was.
    pub step_since: String,
    /// `testing`: the test binary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub what: Option<String>,
    /// How far, when the step gives a count: units compiled of cargo's total, tests ended of the binary's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of: Option<u64>,
    /// `queued`: how many wait before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ahead: Option<u64>,
    /// Tests that failed: so far while `testing`, in all once `ended`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<u64>,
    /// `ended`: the tests that passed, when it ran any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passed: Option<u64>,
    /// `ended`: cargo's exit code; none when its caller was stopped before cargo's end while the session ran on (the
    /// agent stopped it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rc: Option<i32>,
    /// It ended, or was cut, and its agent has not ended a turn since: the turn that follows is where the agent says
    /// it, and the window shows the result on the agent's line until that turn's end (consult-lead, N-1334 post 2).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fresh: bool,
}

impl Job {
    /// Whether it still runs: any step but `ended` and `cut`.
    pub fn runs(&self) -> bool {
        !matches!(self.step.as_str(), "ended" | "cut")
    }

    /// The label of its reference (consult-lead, N-1334 p2: `test yaaihh · edge5-typing ec7f09cb`): its verb, the
    /// packages it names and a target other than the machine's own. Its branch and commit are the id.
    pub fn label(&self) -> String {
        std::iter::once(self.verb.as_str()).chain(self.packages.iter().map(String::as_str)).chain(self.target.as_deref()).collect::<Vec<_>>().join(" ")
    }

    /// Its reference in words, as an agent is told it: the label, then its branch and commit, what is known of them.
    pub fn reference(&self) -> String {
        match [self.branch.as_deref(), self.commit.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(" ") {
            id if id.is_empty() => self.label(),
            id => format!("{} · {id}", self.label()),
        }
    }
}

/// An agent's context (`AgentView.context`): USED, the tokens its latest request's prompt held (input, cache read and
/// cache written); WINDOW, where its running session compacts it: the point it was started with (`Request::Compact`)
/// or the one `/autocompact` typed in it set since, else the coord's point, if set; NEXT, the point kept for its next start, only when
/// it differs from WINDOW (a point is taken at a start only, N-490 p5); MAX, the model's whole window as its status
/// line last gave it (`context_window.context_window_size`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AgentContext {
    pub used: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<u64>,
    /// The model's whole window: the status line input's `context_window_size`. None: not known (the window draws
    /// 1M, every Claude model run has it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<u64>,
    /// The point kept for its next start (`Request::Compact`), when it differs from WINDOW, the point it runs with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<u64>,
    /// USED by kind, about (N-804.p7: the owner's "a breakdown of the context by type"): its session's record since its
    /// last compaction, USED shared out by each kind's share of the characters, the start's part (system prompt, tool
    /// definitions, CLAUDE.md, memory) its own kind. Empty: not known (a Codex session, an older coordd).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<ContextPart>,
    /// How NEXT reaches the running session, when there is one (N-804.p16): `typing`, coordd gave its terminal
    /// `/autocompact NEXT` and the session has not answered yet (typed once no turn runs and its input box is empty);
    /// `restart`, the session answered that a window it was started with stands above the command (started before
    /// E6), so a restart applies it; `start`, it is taken at its harness's next start only (a Codex agent's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_by: Option<String>,
}

/// One kind's part of an agent's context (`AgentContext.parts`): KIND is `start`, `messages`, `summary`, `words`,
/// `thinking`, `tool_calls` or `tool_results`; TOKENS, about.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ContextPart {
    pub kind: String,
    pub tokens: u64,
}

/// An agent's last move between views (`AgentView.moved`; the ticket record's `moved` line): when, from where and to
/// where (a layer's id, the line's view `line@...`; none: the project's own folder), why, and the files it left
/// uncommitted where it stood (they stay there).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AgentMoved {
    pub at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    pub why: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub left: Vec<String>,
}

/// One hit of `Request::Find`: ticket T (its ID `N-221`, its TITLE), its post POST (none: the ticket itself) written
/// BY at AT, WORDS (the post's first line, or the line the words were found in); STAR: who starred it and when.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct FindHit {
    pub id: String,
    pub t: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<u32>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub by: String,
    #[serde(default)]
    pub at: String,
    #[serde(default)]
    pub words: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub star: Option<(String, String)>,
    /// The owner archived T, or a ticket T is under (`TicketView::archived`, `archived_under`): search marks it, and
    /// his starred list leaves it out.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub archived: bool,
}

/// The words of a search Q as `Request::Find` matches them: split at white space, each lowercased.
pub fn query_words(q: &str) -> Vec<String> {
    q.split_whitespace().map(str::to_lowercase).collect()
}

/// Whether TEXT holds every one of WORDS (as [`query_words`] gives them), case apart: what `Request::Find` finds a
/// post, a mail or a conversation's passage by, and the window's titles only a thread (one rule, consult-3 N-412 p2163).
pub fn holds_words(text: &str, words: &[String]) -> bool {
    let low = text.to_lowercase();
    words.iter().all(|w| low.contains(w.as_str()))
}

#[cfg(test)]
mod find_tests {
    /// A text holds a search's words in any order and any case; with one of them missing, it does not.
    #[test]
    fn a_text_holds_a_searchs_words_in_any_order_and_case() {
        let w = super::query_words(" Titles  ONLY ");
        assert_eq!(w, ["titles", "only"]);
        assert!(super::holds_words("only the TITLES", &w));
        assert!(!super::holds_words("the titles", &w));
    }
}

/// One reading of an agent's answer to step STEP of a ticket (`Request::Forming`): the forming file coordd kept
/// (`tickets/forming/T.sSTEP.N.json`, FILE relative to the coord's folder), the AGENT whose answer it read, and from
/// its `formed` line: AT, the HEAD's name (none: the answer gave none of the step's outcomes), WHY (the forming call's
/// sentence, which the agent was told) and NOT_TAKEN (coordd's own reason). CALLS: each call made, its prompt and
/// reply whole.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct FormingRead {
    pub file: String,
    pub step: u32,
    pub n: u32,
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_taken: Option<String>,
    #[serde(default)]
    pub calls: Vec<FormingCall>,
}

/// One forming call of a `FormingRead`: the CALL (`form-head`, `form-parts`), the MODEL that answered, the PROMPT
/// sent and the REPLY (none: no reply came).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct FormingCall {
    pub call: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub reply: Option<serde_json::Value>,
}

/// A team grant (vault designs/teams-and-tiers-2026-11-10.md): up to GRANTED agents, USED added so far, granted BY.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct TeamGrant {
    pub granted: u32,
    pub used: u32,
    pub by: String,
}

/// Token counts of requests: the graph's (a request node's, `docket_machine::graph::Node::tokens`), kept under
/// this name for the usage rows.
pub use docket_machine::graph::Tokens;

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
    /// The rule's sentence for its reader: its doc, else its `//` comment (N-101 p1091); none when it has neither.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

/// The options one entry of a recorded decision's `excluded` names (a `decided` line's provenance, read as JSON): an
/// entry written as [`Exclusion`] names its one option (`option`); one written by rule names every option its rule
/// left out (`options`), each rule once on its line (coordd's `excluded_by_rule`; paper-research N-1446 p2667).
pub fn excluded_options(e: &serde_json::Value) -> Vec<&str> {
    match e.get("options").and_then(serde_json::Value::as_array) {
        Some(os) => os.iter().filter_map(serde_json::Value::as_str).collect(),
        None => e.get("option").and_then(serde_json::Value::as_str).into_iter().collect(),
    }
}

/// What made one rule of a decision hold, or not, in the facts it was handed (proto-mind N-1446 p1502 (9)): the facts
/// its `When` read as they stood, and the facts handed it that those came down to through the policy's `Derived from`
/// lines, so the answer's `why` can name them (the jobs and failing tests of a commit that is not green).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Grounds {
    /// The rule as written with its place, as an exclusion's or a cost's `rule`.
    pub rule: String,
    /// Its place: the file's name and the line (`dev.eflint:71`).
    pub at: String,
    /// Its sentence for the reader, as an exclusion's `why`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// Whether its `When` held. When it did not, FACTS are what it says `not` of that held.
    pub held: bool,
    /// The options an `Only` leaves out when its `When` holds; none for a preference.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bars: Vec<String>,
    /// The facts, as the policy writes them (`red-on("6298415b")`, `not names-item("6298415b", "I-8")`).
    pub facts: Vec<String>,
    /// What the derived ones among FACTS came down to: the facts handed it and the policy's own, in the order read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub from: Vec<String>,
}

/// One preference's cost for an option: the preference as written with its place, its level, the cost.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Cost {
    pub rule: String,
    pub level: i64,
    pub cost: i64,
    /// The rule's sentence for its reader, as an exclusion's `why`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

/// One try of a decided call: the model and, when it failed, why (a failure excludes the model and the decision is
/// solved again, the draft's §5.4).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Attempt {
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    /// The way it went (coordd's router, `accounts::router`): the owner's account it ran on, by id; none, the gateway,
    /// as every attempt recorded before the router went.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
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
    /// The words coordd's own call was given, rendered, when it keeps them: route's `whom`, the thread's holder, so
    /// what the holder read is on the record's `decided` line (the owner's fundamentals §8b, first rule; consult-3
    /// N-1446 p36) and the window's wire can show it. None for every other call, and in a record from before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Fallback mode (the owner, 20 Nov): `model-for`'s every option was refused for a limit, so the call went to the
    /// option of the policy's `fallback-model`; this says so and why. The refused attempts come first in `attempts`,
    /// and the solve's fields (`optimal`, `excluded`, `costs`) are `fallback-model`'s. None for any other decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
    /// What made its rules hold, or not, when it was decided in facts it was handed (proto-mind N-1446 p1502 (9)):
    /// each rule's [`Grounds`]. Empty for a decision on the coord's own facts, and in a record from before it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grounds: Vec<Grounds>,
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

/// One record of today's `items/log.jsonl`: opened, closed, started (an agent works on it). Read once, for the import
/// of items onto tickets (coordd items.rs); nothing writes it any more.
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
    pub views: Vec<ViewData>,
    /// What the kinds' and policy's check refused, as `Body::Refusals` last told it.
    #[serde(default)]
    pub refusals: Vec<String>,
    /// Docket's tickets: the open ones and those closed in the last day.
    #[serde(default)]
    pub tickets: Vec<TicketView>,
    /// What coordd could not do and says so, as `Body::Notices` last told it.
    #[serde(default)]
    pub notices: Vec<Notice>,
    /// The scopes slow is set over (N-412 ask 5; `Machine::slowed`): ["project"] while every ticket halts. A new
    /// snapshot is sent when it changes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slowed: Vec<String>,
    /// This coordd's run (protocol 1): what a watch resumed after a reconnect names (`Request::Watch`'s `run`), so it
    /// resumes only in the run its events were numbered in. None from a coordd before protocol 1: never resumed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    /// The sidebar's parts with their places, `[[name, place], ...]` by place (N-412 ask 2): the policy's
    /// `part-place(name, n)`, so a part with nobody in it is shown too. A new snapshot is sent when they change.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<(String, i64)>,
    /// The project's clocks and policy-decided bindings (`Machine::clock_views`), each with when it last fired: what a
    /// program's clock is drawn from (0.2 line 26). A new snapshot is sent when they change but for `last`
    /// (`Core::parts_round`); a firing reaches the window as the ticket it opens, which names its `binding`. None from a
    /// coordd before it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clocks: Vec<ClockView>,
    /// The project's folders as the server sees them (N-412 ask 13, p129): what the app read from
    /// `~/agent-coord/ID/project` and the file it names on its own machine. None from a coordd before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub places: Option<Places>,
    /// Since when the owner holds agents' mail (line 327, `Request::MailHold`; from `MAIL_HOLD_PROTOCOL`): the time of
    /// his Hold, or none when agents' mail flows. A new snapshot is sent when it changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mail_hold: Option<String>,
    /// The project's owner model as coordd reads it (`OWNER_MODEL_PROTOCOL`): none where no node stands, and from a
    /// coordd before it. `Body::OwnerModel` tells each change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_model: Option<OwnerModelView>,
    /// The project's look as loaded (`look.toml`, [`look`]): what coordd's check took, as it was written. None when
    /// the project has none, or from a coordd before it: the window then draws its built-in look.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<Value>,
}

/// The project's folders on the server (`Snapshot.places`): the coord's folder (`conversations/`, `prompts/` under
/// it), the project file's folder (DEFINITION), `repo` and `library` as coordd resolves them, and `[project] files`
/// with `~/` expanded. Each a path on the server; none when the project has no file or does not set it. And START.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Places {
    pub coord: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// Where the owner's New agent starts an agent now, by coordd's one rule (`Core::start_dir`): the project's folder
    /// (the file's `repo`, else the folder new-project wrote to the coord's `repo`), else the folder most of its agents
    /// run in, else the coord's folder. The project's Home marks it (docket-edge8 N-1446 p2452, p2455). None from a
    /// coordd before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    /// The project's repositories by name, each folder as coordd resolves it (`~/` and a relative path made whole), in
    /// the file's order: what an agent's Repos drawer lists.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<(String, String)>,
    /// The project has no folder of its own: New agent starts each agent in its own home, `agents/NAME/home`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub own_homes: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
}

/// Something coordd could not do, for the owner to see (paper-research N-8): its ticket log or mail log cannot be
/// written (the instance's), or a model call failed (a ticket's). Kept in memory only, never in a log: a restart clears
/// them all; a ticket's is cleared too by a later call of the same kind on it that succeeds.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Notice {
    /// The ticket it is about (its id, as `TicketView::id`), or none: the whole instance.
    pub ticket: Option<String>,
    pub text: String,
    pub at: String,
}

/// What one event says.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Body {
    Snapshot(Snapshot),
    Agent(AgentView),
    Mail(MailView),
    Delivered { id: String, to: String, how: String },
    View(ViewData),
    /// The kinds' and policy's check found these (each a refusal; empty: all pass).
    Refusals(Vec<String>),
    /// A ticket's whole state, after it changed.
    Ticket(TicketView),
    /// A ticket's change (paper-research N-101 p1165; docket-edge N-233 post 159): VIEW whole but for its posts, which
    /// are only those new or changed since the last event of this ticket; POSTS is how many it has now. Posts only
    /// grow and keep their numbers. A reader whose count then differs asks for the ticket whole (`Request::Ticket`).
    TicketChange { view: TicketView, posts: u32 },
    /// A post landed whose words name ticket ID or one of its posts (threads piece 3): LINES are its new lines back,
    /// newest of all, to go at the end of the view's `mentioned`. The view itself is not sent again for it (a ticket
    /// named often, as N-101, would be rebuilt whole for one line). Only to an app at `MENTIONED_BODY` or more.
    Mentioned { id: String, lines: Vec<ViewMention> },
    /// The notices now, the whole list, each time one is raised or cleared.
    Notices(Vec<Notice>),
    /// The project's owner model as coordd reads it now (`Snapshot.owner_model`), each time it changes. Only to an app
    /// at `OWNER_MODEL_PROTOCOL` or more.
    OwnerModel(Option<OwnerModelView>),
    /// A watch resumed after event FROM of the same run (protocol 1): the events missed since follow, and no snapshot.
    /// Always its first event, so a resumed stream starts with something even when nothing was missed.
    Resumed { from: u64 },
    /// The window's look, after a text-change loaded it (`Snapshot.look`; null: there is none now). Only to an app at
    /// [`LOOK_PROTOCOL`] or more.
    Look(Value),
}

/// One line of `coord watch`: `{"seq": N, "<kind>": {...}}`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub seq: u64,
    #[serde(flatten)]
    pub body: Body,
}

/// The refusal of a message sent with esc (paper-research N-36): stopping a turn and sending words are two acts.
pub const ESC_APART: &str = "esc is its own command: coord esc NAME, then coord msg NAME TEXT";

/// A request on coordd's socket, one JSON line. Who asks is not part of it: coordd knows the caller by its process's
/// cgroup. The one exception is an agy agent on a sign-in account, which declares its name beside the request
/// ([`DECLARED`]), since its commands run in the account's t3code server.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Request {
    /// Mail to agents, host sessions or the owner (a thread, or a post on the ticket `re` names). `esc` is refused
    /// (N-36: `Esc` stops a turn, apart from any words); read so an old client gets the refusal, not a parse error.
    /// `as_host` lets the owner side send for a host session.
    /// `re` names the mail or question it answers; `quotes` where the passages it quotes came from. One that carries a
    /// kind goes as `typedmsg` ([`Request::line`]).
    #[serde(alias = "typedmsg")]
    Msg {
        to: Vec<String>,
        body: String,
        #[serde(default)]
        esc: bool,
        /// Taken and meaning nothing: the owner's post goes into the running turn unless WAIT.
        #[serde(default)]
        soon: bool,
        /// The owner's "when done" (N-101 p1216): his post waits for the turn's end, not put into the running turn.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        wait: bool,
        #[serde(default)]
        as_host: Option<String>,
        #[serde(default)]
        re: Option<String>,
        #[serde(default)]
        quotes: Vec<Quote>,
        /// His comments on passages, as data (`COMMENTS_PROTOCOL`): coordd words them into the post and keeps them on it.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        comments: Vec<Comment>,
        /// From another project (N-44, the relay as the owner side, with `as_host` PROJECT/NAME): the ticket there,
        /// `P:N-12`, the thread here came in from; posted on that thread, or one opened with it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin: Option<String>,
        /// `coord msg --under T[.pN[:FROM+LEN]]` (paper-research N-447, piece 2): the thread it opens stands under T at
        /// post N (T's last post when none) from its open; FROM+LEN, the words of that post it is opened on, in
        /// characters (p71). Refused with `re`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        under: Option<String>,
        /// `--label TEXT`: the thread's label, else cut from the first line.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        /// What the post on the thread `re` names is (I-1480, `coord msg --re ID --ask NAME …`), as `Post`'s KIND.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<docket_machine::PostKind>,
    },
    /// Take back mail ID from the recipients it has not reached yet (its sender only): it is never delivered to them.
    /// Refused when it reached every one already.
    Unsend { id: String },
    /// `coord esc NAME[,NAME...]` (N-36): stop the running turn of each agent in TO, and nothing else (no words, no
    /// ticket, no post). The owner may stop anyone; an agent whom the norms' act `esc` allows (the base rule: the
    /// agents below it). The reply says, per agent, "NAME's turn is being stopped", "NAME's turn is already being
    /// stopped" or "NAME is not in a turn"; a refusal names the rule. What is sent to an agent being stopped is handed
    /// once the stop has landed and its turn is over.
    Esc {
        to: Vec<String>,
        /// Each of TO is put in Step (`AgentMode`) in the same request, before its turn is stopped, so what is handed
        /// to it after the stop is staged for the owner's approval (N-1788; from [`CONTROL_PROTOCOL`]). The owner's.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        step: bool,
    },
    /// `coord esc --all`, the window's Esc all (line 200, N-1324; from `ESC_ALL_PROTOCOL`): stop the running turn of
    /// every agent of this project that is in one (Claude Code's and Codex's), and nothing else: one `esc` line names
    /// them all, nothing is held, a turn that begins after it runs, and it does not keep them stopped. Who may: as
    /// `Esc` for each of them, all checked before any is stopped. The reply: "being stopped: A, B", "already being
    /// stopped: C", or "No agent is in a turn: nothing was stopped."
    EscAll,
    /// `coord hold-mail on|off`, the window's Hold and Release (line 327, N-1324; from `MAIL_HOLD_PROTOCOL`); the
    /// owner's alone. One record line each way, so a restart keeps it. On: of what is written from then on, only the
    /// owner's posts pass; every other post, step, reminder and clock's firing is kept from the agents until Off.
    /// The engine keeps the fact (the hold, each ticket's mark); the delivery program keeps what it holds (0.2's
    /// `delivery.kind`). The reply says which; a second Hold, or a Release with no hold, writes nothing and says so.
    MailHold { on: bool },
    /// The window's Brain switch on AGENT's card (from `BRAIN_PROTOCOL`; proto-lead N-1446 p857.2): the owner's
    /// alone. One record line each press, `brain_switch` `{by, agent, on}`, which only coordd writes, so a restart
    /// keeps it; the binding stays in the agent's program. Off: no new pass of AGENT's brain opens until On; one
    /// already at work runs to its end. The reply says which; a press that changes nothing writes nothing and says so.
    Brain { agent: String, on: bool },
    /// His sign-off of his ask POST (`N-1104.p113`) with his word of worth WORTH (`worth_it`, `fine`, `not_worth_it`;
    /// `none` takes it back), his press behind its `⋯` in Open → done (from [`ASKS_PROTOCOL`]; paper-research N-1446
    /// p1786): the owner's alone, on a post of his the ledger says asks an agent. One record line, `signed_off` `{by,
    /// t, n, worth}`, which only coordd writes, so a restart keeps it. A press that changes nothing writes nothing and
    /// says so.
    SignOff { post: String, worth: String },
    /// `coord effort NAME LEVEL`: NAME runs at reasoning effort LEVEL from now on; the owner's, or an agent's for the
    /// agents below it. Its record (the registry's model MODEL:LEVEL, session.json) at once; the running session takes
    /// it when not in a turn: a Claude Code agent restarted on its own session (`roster restart`), nothing handed to it
    /// before; a Codex agent at its next turn. The reply: "NAME: effort OLD → NEW" (OLD "default" when none was set),
    /// then for a Claude Code agent "; restarted on its session (its background tasks ended)", or " when its turn
    /// ends (it restarts on its session; its background tasks end)" in a turn; for a Codex agent " from its next
    /// turn"; for a stopped agent " from its next start". Set back to the level the session runs at, nothing restarts.
    Effort { name: String, level: String },
    /// `coord compact NAME TOKENS`: where a Claude Code or Codex agent's context is compacted, from its next start (the
    /// point is fixed at a session's start, N-490.p5): kept per agent as `effort` is and given at its next start in
    /// its harness's own way (Claude Code's launch flag `--autocompact`; Codex's `model_auto_compact_token_limit`); a
    /// running session is not restarted for it. `/autocompact` typed in a Claude Code agent's session sets the same
    /// point, and that session runs at it at once. No point of its own: the coord's (Claude Code),
    /// Codex's own default (Codex; a point above its model's window is refused). The reply says when it takes effect. From `COMPACT_POINT_PROTOCOL`. TOKENS from `COMPACT_MIN`
    /// to `COMPACT_MAX`, or 0 for the coord's own again; the owner's, or an agent's for the agents below it. The reply:
    /// "NAME: compacts at OLD → NEW from its next start" (`350k`; "the coord's" for none of its own).
    Compact { name: String, tokens: u64 },
    /// `coord can [--as NAME] [THING]` (paper-research N-568 p15, part 6): the acts an actor may do now at THING (a
    /// ticket; identifiers, comma-separated: the acts naming any of them, N-604.p3; none: all the team's), as the norms stand: one `ActAnswer` per line of the reply's `out`, enabled or greyed with
    /// why. An agent asks for its own; the owner names the agent (ACTOR), being bound by none. From `ENABLED_PROTOCOL`.
    Enabled {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thing: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor: Option<String>,
    },
    /// `coord duties [--as NAME]` (paper-research N-568 part 6): the duties that hold now, as the interpreter answers
    /// (eflint-rs, whatever the project's `norms`): one `DutyAnswer` per line of the reply's `out`. An agent asks for
    /// those it holds; the owner for those HOLDER holds, or all. From `DUTIES_PROTOCOL`.
    Duties {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        holder: Option<String>,
    },
    /// `coord model NAME MODEL`: a Claude Code agent's model, on `Effort`'s path (its session restarted on it once not
    /// in a turn; set back to what it runs, nothing restarts). A `gpt-*` model, a Codex agent: refused.
    Model { name: String, model: String },
    /// An agent's session restarted on its own conversation, as for a new effort (its record's MODEL:EFFORT and
    /// compaction point), once not in a turn: the owner's Restart in the session panel (N-804.p9). By the owner or an
    /// agent above NAME. From MODEL_EFFORT_PROTOCOL.
    Restart { name: String },
    /// `coord mode NAME safe|yolo` (0.2 line 25, N-1400): a Claude Code agent's permission mode, the owner's only (refused
    /// from any agent, and on a coordd in a box, whose sessions bypass prompts already). `yolo`: its session starts with
    /// Claude Code's `--dangerously-skip-permissions`; `safe`: the owner's own permission mode, as before. Kept with the
    /// agent as its effort is (the registry, session.json), so every start keeps it; the running session is restarted
    /// on its own conversation, on `Effort`'s path: now when not in a turn, at the turn's end in one; set back to the mode
    /// the session runs in, nothing restarts. The reply: "NAME: Safe → Yolo" then when it takes effect, as `Effort`'s
    /// says. From [`MODE_PROTOCOL`].
    Mode { name: String, mode: String },
    /// The owner's answer to agent NAME's read ask ID ([`ReadAsk`]); the owner's only. Its path and folder are coordd's
    /// own, never the window's.
    #[serde(rename = "read_answer")]
    ReadAnswer { name: String, id: u64, answer: ReadAnswer },
    /// A question to the owner (its first line is its subject); it does not wait: the reply's `out` is its id, the
    /// owner's answer comes as mail. `on` makes it a follow-up in that question's thread.
    Ask { body: String, #[serde(default)] on: Option<String> },
    /// The ask tool's ask (agent-base/claude/channel.py; plan/design-two-systems.md, docket-edge's 8): TEXT, CONTEXT
    /// the rest. The reply comes when it is answered, at most ASK_WAIT later ("the answer will come later" then); the
    /// caller hanging up first is the same cut. Where an ask cannot wait (the project's block_ask off, the agent holds
    /// no step), it is asked as `ask` is and the reply comes at once.
    #[serde(rename = "ask_wait")]
    AskWait { text: String, #[serde(default)] context: Option<String> },
    /// Withdraw a question (the agent that asked it, or the owner), with why.
    Drop { id: String, #[serde(default)] why: Option<String> },
    /// The owner takes back what a net filed (a mail with `filed`): a question is dropped, a reply hidden.
    Unfile { id: String },
    /// The owner has a mail its message net held or sent back delivered as it was.
    Release { id: String },
    /// My questions (the owner: every agent's), each with its state and its thread, the answers in full.
    Questions,
    /// My recent mail, or one mail.
    Mail { id: Option<String> },
    /// Each agent's state.
    State,
    /// An agent's session as text (agents below the caller).
    Log { name: String, last: usize },
    /// What a ticket of KIND needs (`coord open KIND`): its form, result and pattern.
    Kind { kind: String },
    /// Every kind, as JSON: `[{kind, letter, form: [[name, Type]], result, raises, bound, text}]`.
    Kinds,
    /// Every layer of the project, open or ended, as JSON (coordd's layer.rs `Layer`): `[{id, ticket, repo, base, on,
    /// branch, state, holder, view, up, outputs, at, ended, notes}]`. VIEW (`/proc/HOLDER/root/REPO`, while open) is
    /// what the view shows; UP is what the layer changed (a deletion a character device 0/0).
    Layers,
    /// The standing texts roster appends after Claude Code's own, each with the file it is ([`StandingPart`]), as JSON.
    Standing,
    /// The system prompt file agent NAME was given at its last start ([`GivenPrompt`]), as JSON.
    Given { name: String },
    /// Bring every open boxed layer's branch into the project's repository (`coordc layer-sync`; coordd's
    /// sandbox.rs): a boxed agent's git hook after a ref changed in its view.
    LayerSync,
    /// Open a ticket of KIND (`coord open`), under ticket UNDER if given; the form's fields as typed (strings are read
    /// as the form's types want); TEXT a first post.
    Open { kind: String, #[serde(default)] under: Option<String>, form: Value, #[serde(default)] text: Option<String> },
    /// Answer the step ticket T awaits from the caller: HEAD `{"head": C, field: value, ...}`; a sole text field left
    /// out is TEXT.
    Answer { t: String, head: Value, #[serde(default)] text: Option<String> },
    /// A post on ticket T; `@NAME`s in TEXT name who must read it. ANSWERS, the post it answers: the owner's, whose asks
    /// it answers (the card filled in, design-two-systems §16, 1); an agent's, the reply tool's (it names that post's
    /// author too; N-101.p1285). The owner's only: TO, whom he typed it to (it names that agent too). His words for a
    /// closed ticket open a new thread that follows it (§16, item 10). WAIT (the owner's "when done", N-101 p1216): it
    /// is not put into the running turn of an agent it reaches, but given at that turn's end. ALSO: the further
    /// messages it answers (`N-355.p1`, any ticket): the reply tool's `to` list (N-101 p1361). One that carries a kind
    /// goes as `typedpost` ([`Request::line`]).
    #[serde(alias = "typedpost")]
    Post {
        t: String,
        text: String,
        #[serde(default)]
        to: Option<String>,
        #[serde(default)]
        answers: Option<u32>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        wait: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        also: Vec<String>,
        /// The owner's words ticket (`W-12`) whose cleaned text TEXT is (N-487): recorded with the post (a `cleaned`
        /// line), so what he typed is one step away; refused unless it is his `words` ticket ended in Send.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        typed: Option<String>,
        /// The point of one message of his split by point that TEXT is (N-696, `Request::Split`): recorded with the post
        /// (a `split` line), the owner's only.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        split_from: Option<docket_machine::SplitFrom>,
        /// More threads (`N-12`) the same words are posted on, by the same writer, each its own post checked as any
        /// post is; each post says the others (`Post::on`). One message on several threads (the owner's N-354.p1,
        /// N-355.p1). From `POSTED_ON_PROTOCOL`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        on: Vec<String>,
        /// His comments on passages, as data (`COMMENTS_PROTOCOL`), as `Msg`'s.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        comments: Vec<Comment>,
        /// What the post is, as its writer typed it (I-1480; `coord post --ask NAME`, the reply tool's `kind`):
        /// `{"is": "ask", "to": ["w2"]}` and the like (`docket_machine::PostKind`); none, untyped. Its TO are checked
        /// as an `@NAME` is and named; a hold's ON is an item (consult-3 N-1481 p35).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<docket_machine::PostKind>,
    },
    /// The owner: post N of ticket T asks nothing he needs to answer (its asks close, §16, 4).
    NothingToAnswer { t: String, n: u32 },
    /// Withdraw the asks of post N of ticket T, with WHY (`coord ask --drop T.pN WHY`): the post's author, or an agent
    /// above it; nobody is told, and the owner sees the card closed with the why (paper-research N-461, N-447 p4/p5).
    #[serde(rename = "withdraw_asks")]
    WithdrawAsks { t: String, n: u32, why: String },
    /// Take back post N of ticket T (`coord take-back T.pN`), its author only, while nobody has been handed it: it keeps
    /// its number and is drawn no more (paper-research N-447 p16, 2; the form p30).
    #[serde(rename = "take_back")]
    TakeBack { t: String, n: u32 },
    /// Pass the owner's `@all`, post N of ticket T, on to agent TO, with WHY if given (`coord pass T.pN NAME [WHY]`, the
    /// pass tool; consult-lead N-412 post 1900): only the agent it is handed to, while nobody has claimed it, to an
    /// agent of the project that can be woken now and never held it before. One post by the passer naming TO; TO
    /// then holds it, to answer (the claim) or pass on.
    Pass { t: String, n: u32, to: String, #[serde(default, skip_serializing_if = "Option::is_none")] why: Option<String> },
    /// The owner stars ticket T (ON) or takes the star off: it orders his list and does nothing else (§16, 3).
    Star { t: String, on: bool },
    /// The owner archives ticket T (ON), or unarchives it: it is no longer of importance to him, and leaves his lists
    /// (the window's `Archive`, `coord archive T`; the owner N-1073). Not an end: nothing is closed. Only his: anyone
    /// else is refused in words.
    Archive { t: String, on: bool },
    /// Tie thread T under thread TO in the outline (`coord tie T TO`), or untie it (TO `-`): for the outline only
    /// (paper-research N-101 p1676).
    Tie { t: String, to: String },
    /// Title thread T LABEL from now on (`coord title T TEXT`).
    Title { t: String, label: String },
    /// End the threads TS (`coord end T [T...] HOW WHY`, paper-research N-447; `{"op": "end_thread", …}`): HOW `done`,
    /// `dropped` or `kept`, WHY one line, the same for all; each refused (who may, at work, an ask to the owner open
    /// that the ender may not withdraw) is said by name and the others end. Each
    /// leaves the outline; its parent's line for it stays, marked ended. Who may: the policy's `edit` (its opener, an
    /// agent above it, a lead), and the owner. A thread resting at its hole, or an agents' thread; one at work is refused.
    #[serde(rename = "end_thread")]
    EndThread { ts: Vec<String>, how: String, why: String },
    /// The outline: every open thread and item, one row each (`docket_machine::view::ViewOutline`, its children left
    /// out: each row gives its parent), as JSON lines.
    Outline,
    /// The owner (a return to an earlier release, `release.sh coord-back --esc`): ON, coordd hands nothing more to the
    /// agents' sessions, mail or tickets, so a turn stopped stays stopped; OFF, it hands again. Not kept across a
    /// restart (paper-research N-101.p1309, 1).
    Hold { on: bool },
    /// An agy agent's pull (`coord read-brain NOTE`, N-1998): what is due to it, handed in the answer, its briefing as
    /// std.bridges.agy's `Briefing` (JSON); NOTE, where it stands.
    #[serde(rename = "read-brain")]
    ReadBrain {
        #[serde(default)]
        note: String,
    },
    /// An agent's report (`coord report-work`): SUMMARY and the REFS that show it, its answer to what its pulls handed;
    /// ITEM, the item it works on (started if not yet).
    #[serde(rename = "report-work")]
    ReportWork {
        summary: String,
        #[serde(default)]
        refs: Vec<String>,
        #[serde(default)]
        item: Option<String>,
    },
    /// The owner's tools: a notice from coord to agent TO, TEXT its words. It is coord's, not the owner's: a thread
    /// opened by coord, never in his stream (`release.sh`'s "go on" after a return; N-101.p1309, 2).
    Notice { to: String, text: String },
    /// The owner switches thread T to slow (ON) or back: each step given, answer taken, post delivered, ticket opened
    /// and policy decision used on T and the tickets under it halts for his approval first (plan/layers.md I §5, 3).
    /// T may be "project": slow set over the whole project, every ticket, those opened later too; off, what halted
    /// goes through in order but for what a slow ticket holds (N-412 ask 5). The owner may; an agent as the policy's
    /// act `set-slow` / `lift-slow` lets it (agent-base/policy/slow.eflint).
    Slow { t: String, on: bool },
    /// The owner lets halt N of ticket T through, as it stands or as he edited it (docket-core's Input::Approve).
    Approve { t: String, n: u32 },
    /// The owner refuses halt HALT of ticket TICKET (N-1788; from [`CONTROL_PROTOCOL`]): what halted is not done, and
    /// nothing is left halted for it. A step not yet given, an answer not yet taken, a ticket not yet started or a
    /// policy answer not yet used: the ticket is cancelled, Cancelled{by: owner, why: "halt N refused: REASON"} (to go
    /// another way instead, `Withdraw` before refusing). His posts of a card: dropped, reaching nobody. A notice: never
    /// given. REASON goes on the record's `halt_reject` line. The owner's.
    HaltReject {
        ticket: u64,
        halt: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// The owner's words waiting for AGENT, handed now (N-1788; from [`CONTROL_PROTOCOL`]): with INTERRUPT its running
    /// turn is stopped (as `Esc`) and they are handed once it has; without, they are handed at the end of its running
    /// tool call, inside its turn, with nothing stopped. The owner's.
    DeliverUrgent { agent: String, interrupt: bool },
    /// The owner sets agent NAME's pace (N-1788; from [`CONTROL_PROTOCOL`]): `AgentMode::Step` or `Run`, kept over a
    /// restart (registry `control`). Back in Run, what was staged for it is handed as it comes.
    AgentPace { name: String, mode: AgentMode },
    /// The owner's harness switches for NAME, written to `$COORD/agents/NAME/harness.toml`; a key given replaces, one
    /// left out is kept; they apply at its next start.
    HarnessSwitches {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        safety_check: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tools_off: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parts_off: Option<Vec<String>>,
    },
    /// The owner edits halt N of ticket T before approving it: a step's {prompt, worker}, an answer's head, a post's
    /// wakes, an opened ticket's form, a decision's answer (docket-core's Input::HaltEdit; shapes in ViewHalt).
    HaltEdit { t: String, n: u32, value: Value },
    /// The owner undoes the minder's last answer at ticket T's hole that was not undone, a Rest or a refusal aside: T
    /// rests at that hole again (the card's Undo, §16, 3; docket-core's Input::Undo).
    Undo { t: String },
    /// Cancel ticket T, with why (policy's `may edit`): its step withdrawn, its open children cancelled, it fails
    /// Cancelled{by, why} (docket-core's Input::Cancel).
    Cancel { t: String, why: String },
    /// Withdraw ticket T's step, with why (policy's `may edit`): PROGRAM, pattern text checked in the step's scope
    /// against its head, goes on in its place (docket-core's Input::Withdraw).
    Withdraw { t: String, program: String, why: String },
    /// Repair the failed ticket T: it runs PROGRAM, checked as its kind's pattern would be (policy's `may edit`).
    Repair { t: String, program: String, why: String },
    /// Edit T: PROGRAM, in which `answer` names the answer to the step T awaits, replaces all T does after it
    /// (policy's `may edit`); TicketView.continuation is the program to start from.
    Edit { t: String, program: String, why: String },
    /// Rekind T: it becomes a KIND with FORM and runs that kind's pattern; its posts stay (policy's `may edit`).
    Rekind { t: String, kind: String, form: Value, why: String },
    /// The owner edits the prompt of CALL's steps on T once (the app's step panel): TEXT, a template in the call's own
    /// slots, for the next step of CALL on T (SCOPE `step`) or every later one on T and the tickets under it (`tree`),
    /// for WHY. The call's file is not changed (docket-core's Input::Template).
    Template { t: String, call: String, scope: String, text: String, why: String },
    /// The owner has coordd read the project's kinds and calls again, after COMMIT (the project's files changed), for
    /// WHY: all checked together and taken only if all pass; else the old ones stay and the errors are the answer
    /// (docket-core's Input::Reload).
    Reload {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit: Option<String>,
        why: String,
    },
    /// What a reload would do were the file at PATH (a kind's or a call's, or one not there yet) TEXT, nothing changed
    /// (the owner's draft, before he saves it; N-1396 p301): read and refused as `Reload` reads them, its answer
    /// `Reload`'s with each ticket's `reach` and `said`, and `clocks`, each binding the draft edits with its next
    /// firing. The owner's only.
    Reach { path: String, text: String },
    /// A step of CALL as it would be sent were the owner's draft saved, nothing sent, recorded or counted (his dry run,
    /// N-216.p1; docket-edge8 N-1446 p1824, H2). It is filled from a step a ticket waits on now: ON's, when ON waits on
    /// a step of CALL; else, for a draft of the program, the newest open ticket's that does. With PATH, TEXT is the call
    /// file at PATH as drafted, read as `Reach` reads it; with none, TEXT is his prompt for ON's thread alone, as
    /// `Template` would keep it. The owner's only. Its answer: a [`DryRunAnswer`].
    DryRun {
        call: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        text: String,
    },
    /// Ticket T as JSON (a `TicketView`).
    Ticket { t: String },
    /// What each of IDS reads as beside its id (the owner N-286.p1, paper-research N-101 p800): a ticket `C-1`, a step
    /// `C-1.s2`, a post `N-101.p5`, an agent by its name, a commit `REPO:SHA` or a bare SHA. JSON lines of `Label`.
    Label { ids: Vec<String> },
    /// The record's one graph (docket-machine `graph`, made along coordd's replay at its start and kept with each input
    /// it records), the part of it ASK names: by the layers on, around a node, by who and when, the newest up to a
    /// limit (`graph_slice`; paper-research N-1446 p1376 part 5, proto-window p1383 head 1). JSON: a `GraphAnswer`.
    /// From `GRAPH_PROTOCOL`.
    Graph { ask: docket_machine::graph_slice::Ask },
    /// The posts whose words name ID (threads piece 3, paper-research N-412 p166/p169): a ticket (it or one of its posts),
    /// a post `N-412.p30`, an agent by NAME or `@NAME`. Oldest first, JSON lines of `ViewMention`; none when ID is not
    /// there. Nothing is recorded.
    Mentions { id: String },
    /// The owner's find (the owner N-234.p3), over every ticket, closed long ago too: with STARRED every star, on a
    /// ticket or a post, the owner's first; else the posts whose words hold every word of Q (case apart), newest first.
    /// JSON lines of `FindHit`.
    /// WIDE (`coord find`, paper-research N-101 p1026): the mail and the agents' conversations as well, read on the host;
    /// their hits have no ticket (T 0), their id the mail's or `NAME.SESSION@OFFSET`.
    Find { #[serde(default)] q: String, #[serde(default)] starred: bool, #[serde(default, skip_serializing_if = "std::ops::Not::not")] wide: bool },
    /// The owner is writing (ON) in his chat with AGENT, or no longer: while he is, nothing from agents is given to
    /// it (paper-research N-101 p1099, 2). The owner's only; no timer: it lasts until he says it ends, the process that
    /// said it ends (the app's `coord bridge`, when the app closes; p1127, 2), or coordd restarts. The app says off when
    /// his box is emptied. THREAD (0.3; proto-lead N-1446 p26): his writing in that ticket's box instead, AGENT then
    /// empty: a record line each way (docket-machine `syntax::WRITING`), which the hold of his posts there reads.
    Writing { #[serde(default)] agent: String, on: bool, #[serde(default, skip_serializing_if = "Option::is_none")] thread: Option<String> },
    /// How the answers to ticket T's steps were read: every forming file coordd kept for it, by step and in order,
    /// each with its `formed` line (paper-research N-101 p623). A JSON list of `FormingRead`.
    Forming { t: String },
    /// The record as written about ticket T, for the window's Record layer (the fundamentals §2, layer 4; proto-window
    /// N-1446 p10): the lines of T and of its handlers' runs (the hidden tickets under it; a subthread has a record of
    /// its own), each with its line number, oldest first. They start at post AT of T (none: at T's opening), or after line AFTER for the next page.
    /// At most CAP lines (0: [`RECORD_CAP`]), never more than that. JSON lines of `RecordLine`. Read off the core lock
    /// (consult-3 N-1446 p34); nothing is recorded.
    Record {
        t: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<u64>,
        #[serde(default)]
        cap: u32,
    },
    /// The owner's: node ID's words from its source, asked on a click and never kept (the record's graph, proto-window
    /// N-1446 p1383; paper-research p1380). Served for a slice of a session (`docket_machine::graph::slice_id`): coordd
    /// takes the giving's place from its record, never from the ask, and gives that place's text only when it holds the
    /// words of each post that giving brought; else it refuses, saying why. Read off the core lock; nothing is
    /// recorded, logged or kept. From [`RAW_PROTOCOL`].
    Raw { id: String },
    /// The owner's: the rows of insight WHICH (S6, proto-lead N-1446 p388), read under the core lock from the machine
    /// coordd holds, as JSON lines; nothing is recorded, no model is asked, nothing outside coordd is read. For
    /// `Insight::OpenToDone`, one `docket_machine::insight::Thread` per line of the outline.
    Insight { which: Insight },
    /// The owner's bridge: the release his window runs, as its hello named it (I-1457; proto-lead N-1446 p509). coordd
    /// writes it, and the release coordd itself runs (p610), as a `window_release` line only when either one, or the
    /// list of earlier releases its folder says it contains, differs from the last one recorded; coordc sends it for
    /// every hello that names one. SINCE: the hello's (`Hello::since`), when his window became his by its own clock,
    /// which the line carries; none from an older window or coordc, and an older coordd reads the request without it.
    #[serde(rename = "window_release")]
    WindowRelease {
        release: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        since: Option<String>,
    },
    /// Tickets as JSON lines: FILTER `{}`, `{"waiting": true}` (awaiting the caller), `{"mine": true}` (opened by
    /// the caller), `{"kind": K}`.
    Tickets { #[serde(default)] filter: Value },
    /// The text at LOC (a post, a formed answer's text or a delivery, by reference): the Tickets page reads them
    /// through coordd, since a boxed project's transcripts are not on the host. Served only at a place a line of the
    /// record gives a text at (coordd `ticket::text_places`); any other is refused (paper-research N-1446 p1419, 2).
    Text { loc: docket_machine::Loc },
    /// The owner's plain message to agent TO, with the agents NAMES tagged in it (`@NAME`): the event `owner_message`,
    /// which opens a thread (design-two-systems §3: tagged, a thread for sure; else the watcher's intake decides a post
    /// on an open ticket or a thread; there is no one-off, §16). No TO: whoever policy says fills lead takes it, and the
    /// words are typed to nobody. To stop TO's turn, `Esc` first (N-36).
    /// FOLLOWS: what the owner quoted (paper-research N-101 p470, the owner N-234.p5): a post (`N-221.p5`), or a
    /// ticket (`N-221`) for a session message that is no post. The words are then his post on that ticket, under the
    /// post quoted (its `answers`), open or closed: no `owner_message`, no thread, no intake.
    /// They go into TO's running turn (N-101 p1214) unless WAIT, his "when done" (p1216): then at its end.
    /// UNDER (paper-research N-447, piece 2): his answer "in a subthread", a post `N-234.p11` or a ticket `N-234` (at
    /// its last post): the words open a thread (no intake: he placed them), tied there from its open; LABEL its label,
    /// else cut from the first line. Refused with FOLLOWS.
    Say {
        #[serde(default)]
        to: Option<String>,
        #[serde(default)]
        names: Vec<String>,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        follows: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        wait: bool,
        /// A subthread at a post: `T.pN` (or `T`, its last post), and `:FROM+LEN` for the words of that post it is
        /// opened on, in characters, a selection's start and length (paper-research N-447 p35, p71).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        under: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        /// A new top thread with nothing that routes it (N-447 p35): Named, so no intake, and no tie.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        thread: bool,
        /// His words ticket whose cleaned text TEXT is, as `Post::typed` (N-487, docket-edge2 N-564): recorded on the
        /// post of his that carries TEXT; refused before anything is sent unless it is his words ticket ended in Send.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        typed: Option<String>,
        /// The point of one message of his split by point that TEXT is (N-696, `Request::Split`): as `Post::split_from`,
        /// recorded on the post of his that carries TEXT.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        split_from: Option<docket_machine::SplitFrom>,
        /// His comments on passages, as data (`COMMENTS_PROTOCOL`), as `Msg`'s.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        comments: Vec<Comment>,
    },
    /// The owner moves post N of ticket T to ticket TO, or to a new thread (None): T keeps a stub.
    Move { t: String, n: u32, to: Option<String> },
    /// The owner makes the one-offs TS (consecutive, to one agent) a thread: the first rests again with the words and
    /// the agent's answers as posts, the others end moved to it.
    Keep { ts: Vec<String> },
    /// The owner makes thread T, at its hole, a one-off after all (an Edit at the hole with `return OneOff`).
    DropThread { t: String },
    /// `start ID`, `open TITLE`, `close ID WHY`.
    Item { action: String, arg: String, #[serde(default)] why: Option<String> },
    /// Register an agent; the caller becomes its parent. RENDERINGS: the set its deliveries are drawn from; FROM: the
    /// agent whose conversation it is a copy of (`roster fork`).
    Add {
        name: String,
        kind: String,
        model: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        renderings: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<String>,
    },
    /// An agent made from the caller's turn (`coord new`, `coord fork`; item 11): NEW, a fresh agent or (FROM) a copy
    /// of FROM's conversation, on MODEL (else FROM's, else the caller's), TEXT its first words. The policy's
    /// `make-agent` decides; it is recorded as the caller's act `made`.
    /// FORK: `coord fork` (a copy of FROM, else of the caller), not `coord new`; an empty NEW is FROM-fork, -fork2, ...
    Make { new: String, #[serde(default)] from: Option<String>, #[serde(default)] model: Option<String>, text: String, #[serde(default)] fork: bool },
    /// `coord go [ID]` (N-101 p414): the calling agent is moved to layer ID (or the layer of ticket ID) when its turn
    /// ends; none: where it stands and each step it holds with its layer.
    Go { #[serde(default)] to: Option<String> },
    /// The owner's `roster add` or `roster restart` of agent NAME in a boxed project, through coordd (N-101 post 406, the
    /// joint proposal's A): coordd starts it as any agent of the project (`launch`), its own user. MODEL, PROMPT (a
    /// file) and DIR as `roster add` takes them; no MODEL: started again on its own conversation.
    Start { name: String, #[serde(default)] model: Option<String>, #[serde(default)] prompt: Option<String>, #[serde(default)] dir: Option<String>, #[serde(default)] force: bool },
    /// Agent NAME is called TO from now on (stopped agents only; its session, id and history stay).
    Rename { name: String, to: String },
    /// The owner's New agent (yaaihh; protocol 1, N-412 ask 13): NAME on MODEL, its first prompt TEXT (written to
    /// `$COORD/prompts/NAME.md`), in DIR, else the coord's folder: coordd runs `roster add`, as the app ran it through
    /// a shell on the host before. The owner's only.
    AddAgent { name: String, model: String, text: String, #[serde(default)] dir: Option<String> },
    /// The owner's Fork (yaaihh; protocol 1): a new agent on a copy of FROM's conversation, called NEW (else FROM-fork,
    /// -fork2, ...), on MODEL and in DIR (else FROM's), TEXT after the fork note: coordd runs `roster fork`. The
    /// owner's only. AT (seconds, `REWIND_PROTOCOL`): the copy is cut at the end of FROM's last turn that ended by then
    /// (the Map's Rewind / Branch from here), not as it stands.
    ForkAgent { from: String, #[serde(default)] new: Option<String>, #[serde(default)] model: Option<String>, #[serde(default)] dir: Option<String>, #[serde(default)] text: Option<String>, #[serde(default, skip_serializing_if = "Option::is_none")] at: Option<i64> },
    /// The owner's Rename (yaaihh; protocol 1): agent NAME is called TO once its turn has ended (stopped, renamed,
    /// resumed as TO): coordd runs `roster rename`; the reply comes when it is done. The owner's only.
    RenameAgent { name: String, to: String },
    /// The owner ends spin-off NAME, an agent copied from another's conversation (`AgentView::from`), as HOW says:
    /// joined back, closed or deleted (`SPIN_PROTOCOL`; the owner's walk item 19). The reply comes when it is done. The
    /// owner's only.
    SpinEnd { name: String, how: SpinEnd },
    /// The owner resolves comment ID of post N of thread T (RESOLVED), or reopens it (`COMMENTS_PROTOCOL`).
    CommentResolve { t: String, n: u32, id: String, resolved: bool },
    /// The owner's edit of agent NAME's system prompt, as a branch in its history (protocol 51; the owner N-1104 p996:
    /// "allow me to wedit it as a hsitry brnahc/jump"; accounts-lead N-1446 p2447): TEXT is its system prompt from now,
    /// appended to Claude Code's own, or in its place with REPLACE. NAME goes on from its conversation as it stands, or
    /// with AFTER from a copy of its session cut after AFTER bytes of its transcript, the end of the turn he picked, its
    /// old session kept as it was; once not in a turn. Claude Code's only; the owner's only.
    SystemPrompt {
        name: String,
        text: String,
        #[serde(default)]
        replace: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<u64>,
    },
    /// `coord native NAME on|off` (N-412 ask 12): Claude Code agent NAME runs on one of the owner's own accounts from
    /// now on (ON), chosen by tier and turn, or through the project's model route again (off). A running session is
    /// restarted on its conversation once not in a turn. The owner's only.
    Native { name: String, on: bool },
    /// The Accounts page's switch `All agents use my accounts` (protocol [`ACCOUNTS_ALL`]): every Claude Code agent
    /// of this coord runs on the owner's accounts from now on (ON), or through the gateway again, each from its own
    /// turn's end by the restart `Native` uses (no running turn is cut), and new agents start there. One registry
    /// line (`native_all`), beside each agent's own. The owner's only.
    AccountsAll { on: bool },
    /// `coordc account-for NAME`, roster's at each start of a Claude Code agent: the folder NAME's session runs with
    /// (Claude Code's `CLAUDE_CONFIG_DIR`, the account chosen for it and recorded), or nothing when NAME goes through
    /// the model route. An error when it is to run on an account and none can take it. For an opencode agent: the
    /// folder of the owner's opencode account of its model's provider, then `NAME=VALUE` lines of its folders, or
    /// nothing when none of that provider is signed in (it runs on folders of its own).
    AccountFor { name: String },
    /// `coordc account-bound NAME`: the agy account agy agent NAME is bound to, in `AccountFor`'s lines, nothing
    /// chosen (cutover.sh, for coordd-v2's start of NAME). The owner's only.
    AccountBound { name: String },
    /// `coord account move NAME [ACCOUNT]`: agent NAME goes to another account (ACCOUNT, else the next by tier and turn) once
    /// not in a turn. The owner's only.
    MoveAgent { name: String, #[serde(default)] to: Option<String> },
    /// `coord accounts`: the owner's accounts with their tier, state and agents, as text; with JSON (the app's Accounts
    /// page), the reply's `out` is a [`NativeAccounts`] as JSON.
    Accounts { #[serde(default)] json: bool },
    /// The native usage's cells past byte AFTER ([`UsageCells`] as JSON); the owner's only.
    #[serde(rename = "usage_cells")]
    UsageCells { #[serde(default)] after: u64 },
    /// `coord account new ID [LABEL] [--tier N] [--key]`: a new account's folder, not signed in yet; with KEY an
    /// account that runs on an API key the owner puts in its folder (`AccountKey`), with no key yet. The owner's only.
    ///
    /// From [`OPENCODE_ACCOUNTS`]: with HARNESS `opencode`, an opencode account for PROVIDER (opencode's id of it,
    /// `opencode-go`), its folders made empty and nobody signed in; KEY is then not used (opencode keeps a key it is
    /// given, `AccountKeyStore`). From [`AGY_ACCOUNTS`]: with HARNESS `agy`, an agy account, on KEY a Gemini API key
    /// account (its key field's `AccountKeyStore`), else a sign-in account (t3code's own Google sign-in), PROVIDER not
    /// used. Without HARNESS, Claude Code's.
    AccountNew {
        id: String,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        tier: i64,
        #[serde(default)]
        key: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        harness: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider: Option<String>,
    },
    /// `coord account key ID [--remove]`, coordc's three questions about key account ID's key file (the key itself is
    /// in none of them: coordc reads it from the owner's terminal and writes the file): where the file is; that it
    /// has been written; that it has been removed. The owner's only.
    AccountKey { id: String, step: KeyStep },
    /// The key field of the window's Accounts page (protocol [`ACCOUNT_KEY`]): KEY is the API key the owner typed for
    /// key account ID. coordd writes it to the account's key file (mode 600) and keeps it nowhere else: the request
    /// is answered on a path of its own, off the lock, with no record, no journal line of it and no event, and the
    /// reply says `account ID: key stored` or `account ID: key not stored: WHY`. The owner's only (the caller's
    /// cgroup); coordc has no command that sends it (`coord account key` types the key into the file itself). A new
    /// key replaces the one there; nothing ever answers with a key.
    ///
    /// On an opencode account (from [`OPENCODE_ACCOUNTS`]), KEY goes to no file of ours: coordd runs opencode's own
    /// `auth login` for the account's provider and writes KEY once into its masked `Enter your API key` prompt, then
    /// asks `opencode auth list` whether opencode keeps a sign-in for it. The same path, with no record, journal line
    /// or event; the reply says `account ID: key stored` once opencode says so.
    AccountKeyStore { id: String, key: Secret },
    /// `coord account tier ID N`: account ID's tier. The owner's only. Answered as the [`Request::AccountEdit`] of
    /// that tier alone.
    AccountTier { id: String, tier: i64 },
    /// The card's `Rename` and `Turn off` · `Turn on`, and a tier (from [`ACCOUNT_EDIT`] on; the owner's only): the
    /// owner's change of account ID's own settings, each given changed and the rest as they were: its TIER, its LABEL
    /// (words, not empty), whether it is OFF. An account that is off takes no agent; its Claude Code agents move to
    /// another account, or go on through the gateway, at the end of their turn, as a removed one's do, and they may
    /// come back once it is on (`NativeAccount.off`). An opencode agent keeps its account while its server runs, as
    /// for a removal. The gateway's `Disable` · `Enable` on his own accounts.
    AccountEdit {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tier: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        off: Option<bool>,
    },
    /// `coord account signin ID [--email EMAIL]`: Claude Code's own sign-in is started under account ID's folder, and
    /// the reply is the URL the owner opens (one that still runs answers its URL again). It ends by the browser's
    /// redirect or by a pasted code (`AccountCode`); what Claude Code then says of the account is recorded. The
    /// owner's only.
    ///
    /// On an opencode account (from [`OPENCODE_ACCOUNTS`]): opencode's own `auth login` for its provider, with METHOD
    /// the way opencode names (`ChatGPT Pro/Plus (headless)`). The reply's first line is the link opencode says to
    /// open, the second, if there is one, its instructions (`Enter code: …`). A code the browser shows goes in by
    /// `AccountCode` as for Claude Code.
    AccountSignin {
        id: String,
        #[serde(default)]
        email: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        method: Option<String>,
    },
    /// `coord account code ID CODE`: the code the browser showed, handed to the sign-in that runs for ID. The owner's only.
    AccountCode { id: String, code: String },
    /// `coord account check ID`, and the card's refresh: ask Claude Code whether ID is signed in (`claude auth status`),
    /// record and say it. Of a signed-in sign-in account its windows too, by Claude Code's own `/usage` in the account's
    /// folder, which sends nothing to a model, recorded as a session's status line would be (a coordd before that says
    /// the login alone); the reply then ends `· 5-hour 17% until … · weekly 6% until …`, or why they were not read.
    AccountCheck { id: String },
    /// `coord account cancel ID`: end the sign-in that runs for ID. The owner's only.
    AccountCancel { id: String },
    /// `coord account clear ID`: account ID can take requests again, whatever was last said of its limits (for one
    /// marked out with no reset time known). The owner's only.
    AccountClear { id: String },
    /// The window's word that his press cleared a gateway login's cooldown: the `⋯ Clear cooldown` of a gateway login, or
    /// a reset of one that went through (paper-research N-1446 p957). FAMILY is the login's type as the window reads it
    /// (`claude`, `codex`, `gemini`), the family of the models the gateway then serves again. Each agent held on a rate
    /// limit through the gateway on a model of that family, whose turn stopped before the press, goes on with that turn
    /// now, once, not at the reset its 429 named. The reply names them: `gateway: 2 held on claude models go on (ann,
    /// bob)`, or `gateway: none held on claude models`. The owner's only; from [`GATEWAY_CLEARED_PROTOCOL`] on.
    GatewayCleared { family: String },
    /// `coord account remove ID`: account ID is no account any more. Its folder is set aside whole, under
    /// `ROOT/removed/`, with nothing in it read or deleted, and Claude Code then signs it out itself there (`claude
    /// auth logout`). While agents of any coord run on it (from [`ACCOUNT_LEAVE`] on; refused before) it is marked
    /// instead (`NativeAccount.leaving`): it takes no agent, each of its agents moves to another account, or goes on
    /// through the gateway, at the end of its turn, and the account is removed when none is left. Refused in words
    /// when they would have nowhere to go. The owner's only.
    AccountRemove { id: String },
    /// The card's `Keep it` (from [`ACCOUNT_LEAVE`] on; the owner's only): account ID, which is being removed and
    /// waits for its agents (`NativeAccount.leaving`), stays an account after all. The agents that moved meanwhile
    /// stay where they went.
    AccountKeep { id: String },
    /// A removed account is an account again (from `ACCOUNT_REMOVE` on; the owner's only): NAME is its set-aside
    /// folder's name, as [`NativeAccounts::removed`] lists it. The folder is moved back whole, and for a sign-in
    /// account Claude Code is asked whether it is signed in still (the removal signed it out, unless that failed).
    AccountPutBack { name: String },
    /// Key account ID's key file is deleted, by coordd (from `ACCOUNT_REMOVE` on; the owner's only): the window's
    /// `Remove key`, what `coord account key ID --remove` does from a terminal. The account stays, with no key.
    AccountKeyRemove { id: String },
    /// The card's `Use` of a reset Claude Code offers account ID ([`NativeAccount::resets`]; the owner's only, from
    /// [`ACCOUNT_RESET`] on): KIND is the offer's `kind` as given. Only this press puts `/limit-reset` into a Claude
    /// Code session in the account's folder, one coordd starts on a terminal of its own for it and ends after; nothing
    /// of coordd's sends it by itself (the owner, 20 Nov: "DO NOT USE MY LIMIT RESETS"). The window shows a `Use` only
    /// where coordd gave offers. Reply: ok once the session is starting, or the refusal in words; how it went comes
    /// as `NativeAccount::reset`.
    AccountReset { id: String, kind: String },
    /// Commit HASH's subject line (N-412 ask 13, p129: the runs view's), in REPO, a name among the project's `repos`,
    /// else the project's `repo`. The owner's only. Reply: the subject, or why there is none.
    Subject { hash: String, #[serde(default)] repo: Option<String> },
    /// The Harness page's data on the server (p129, p135), read on the machine at once: `{"versions": [{name,
    /// installed, kept, latest: null, how}], "pairs": <the proxy's /v1/harness-models on the coord's route> | null,
    /// "error": why no pairs | null}` as JSON in the reply. The owner's only.
    Harness,
    /// The newest Codex upstream (`npm view @openai/codex version`), asked only when the owner presses for it; the reply
    /// comes when npm ends, with npm's error as it is. The owner's only.
    HarnessLatest,
    /// Switch PAIRS of harness HARNESS on or off in the proxy's rules file (each pair's exact rules replaced by one at
    /// the end); ALL_ON: every pair of HARNESS on again (its rules dropped), PAIRS and ON unread. coordd is the file's
    /// one writer ([`RULES_PROTOCOL`]): it takes the file's name from the proxy's `/v1/harness-models`, never opens the
    /// proxy's config, and answers `{"file", "rules"}` as written; what the proxy has in force is read back with
    /// [`Request::Harness`]. The owner's only.
    HarnessSet {
        harness: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pairs: Vec<RulePair>,
        #[serde(default)]
        on: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        all_on: bool,
    },
    /// The Machines page's numbers ([`MACHINES_PROTOCOL`], N-1136 post 6): `~/agent-coord/machines/machines.json`,
    /// kpi-data's, read by that name only (a plain file, a link not followed, at most 2 MB): `{"text": <the file's
    /// text, unread by coordd> | null, "age_secs": <since it was written> | null}` as JSON, at once. When the file is
    /// missing or 30 s old, the ask also starts one run of `agent-base/bin/machines-sample` beside the answer, never
    /// two at once and with no time limit of coordd's (N-412 posts 1958, 1963); its sample shows at the next ask.
    /// Nothing runs while the page does not ask. The owner's only.
    Machines,
    /// The owner's typing cleaned (protocol [`CLEAN_TEXT_PROTOCOL`], N-487): fresh calls on TEXT alone, its misspellings and words run
    /// together or split fixed and nothing else, one call a paragraph; ids, hashes, paths, names, code and quoted
    /// lines come back as they are, or the answer is refused. KEEP: his own words (the window's dictionary), kept as
    /// the agents' names and the project's `keep-words` are, and never a doubt; none from an older window. Reply, as JSON: `{head: "Sure", cleaned}`,
    /// `{head: "Borderline", cleaned, doubts: [{typed, would: [..]}], per100}` (a doubt left as typed; PER100 the doubts per 100 words) or `{head: "Unreadable", why}`, each with `model` and
    /// `ms`; or why there is none. Nothing is posted or kept.
    /// The owner's only.
    #[serde(rename = "clean_text")]
    CleanText { text: String, #[serde(default, skip_serializing_if = "Vec::is_empty")] keep: Vec<String> },
    /// His message's points proposed (N-696; consult-3 N-590 p9): his TEXT as typed and the threads he is on to one
    /// fresh call (the words program's `split`); nothing is posted or recorded. `out`: `{"head": "One"}`, or
    /// `{"head": "Points", "points": [{"words", "from", "len", "to", "label"}]}` with TO a thread's id, `new` or
    /// `here`; the window shows them under the box and sends what he takes. The owner's only.
    Split { text: String },
    /// The owner's Files page roots (protocol 4, as ListDir, ReadFile, Locate, Sources and EditFile): `[{name, path, here}]`, as the app made them from the host's disk.
    Roots,
    /// The owner's folder PATH, inside a root: `[DirEntry]`, folders first, no `.git`. GIT (`FILE_STATES_PROTOCOL`):
    /// each entry's git state too; git is run only then.
    ListDir {
        path: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        git: bool,
    },
    /// The owner's file PATH, inside a root or a picture anywhere, never a credential place: `{size, modified, data}`,
    /// modified the mtime in ms, data the base64 of LEN bytes (at most 1 MiB) from FROM; `len: 0` is the stat alone.
    ReadFile {
        path: String,
        #[serde(default)]
        from: Option<u64>,
        #[serde(default)]
        len: Option<u64>,
    },
    /// The full path of PATH as a chat names it: itself when absolute, else under CWD or a parent of it, else the
    /// first file under SEARCH whose path ends in it: `{path, readable}`, readable whether ReadFile serves it.
    Locate {
        path: String,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        search: Vec<String>,
    },
    /// Every kinds and calls file coordd reads, with its text: `{kinds: [{path, text}], calls: [{path, text}]}`.
    Sources,
    /// The owner's edit of a kinds or calls file of the project or its library: refused unless the file is still
    /// OLD (None: a new file); written, and committed with MESSAGE where it is in git.
    EditFile {
        path: String,
        #[serde(default)]
        old: Option<String>,
        text: String,
        message: String,
    },
    /// The owner's repository NAME (a plain name) at PATH, a folder on coordd's machine (`~/` or a whole path) that is
    /// a git repository's top, added to the project from its Home (fix 3; docket-edge8's shape N-1446 p2452, read
    /// right p2455): one key of the project file's `[project] repos`, written so the rest of the file stays as it is,
    /// and committed where the file is in git; a project with no file gets one, `$COORD/definition/project.toml`,
    /// named in `$COORD/project`. Refused, nothing written, unless the file as it would be passes the checks coordd
    /// loads it with. Then the project file is read again and a snapshot sent (`ADD_REPO_PROTOCOL`).
    AddRepo { name: String, path: String },
    /// Open TARGET in coordd's openvscode-server, in THEME (`dark`, `light`); answers a [`codeview::CodeViewAnswer`]
    /// as JSON. The owner's ([`CODE_VIEW_PROTOCOL`]).
    CodeView {
        target: codeview::CodeTarget,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        theme: Option<String>,
        /// The window's colours, which the workbench takes as its own ([`CODE_WARM_PROTOCOL`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        looks: Option<codeview::CodeLooks>,
    },
    /// The window has closed its view of this coordd's code view: the server is stopped once no window is connected to
    /// it, else left to its idle wait. The owner's ([`CODE_WARM_PROTOCOL`]).
    CodeViewLeft,
    /// The owner's messages held to go later: one held, the list, one sent now or dropped (`schedule::Sched`).
    Schedule { act: schedule::Sched },
    /// Agent NAME's role from now on (a word the policy's norms name: `lead`, `consultant`, ...); the owner sets it.
    Role { name: String, role: String },
    /// `coord ended NAME`: the owner ends agent NAME for good (never started again, given no step, its sessions kept);
    /// `coord ended --undo NAME` takes it back. The owner's only.
    Ended { name: String, #[serde(default)] undo: bool },
    /// `coord team ask TEXT`: the calling agent asks to form a team; TEXT goes to the approvers the norms name.
    TeamAsk { text: String },
    /// An act of the calling agent (N-101): IS what it is (`edit`: the symbols server, once a changing tool changed
    /// something), TEXT what it did, MORE its other fields (`tool`, `symbol`, `files`). Recorded as the record's `act`
    /// line, on the ticket the agent owes a step on; coordd writes `submit` and `check` itself and refuses them here.
    Act { is: String, #[serde(default)] text: String, #[serde(default)] more: serde_json::Map<String, serde_json::Value> },
    /// `coord check [--quick|--typecheck|--proofs] [REPO:BRANCH...]`: the calling agent's own check, through coordd to
    /// the project's runner (`RUNNER check`); no REFS: its layer's branch in every repository. Its answer comes back
    /// as a notice and as the act `check` (N-101, runner-contract.md "Agreed", e).
    Check { #[serde(default)] mode: Option<String>, #[serde(default)] refs: Vec<String>, #[serde(default, skip_serializing_if = "Vec::is_empty")] flip: Vec<String> },
    /// `coord jobs`: the runner's running jobs, as `RUNNER jobs` says them, read now.
    Jobs,
    /// `coord store [ROW] [--from N]`: what the store holds (Home's Store, a2h-speed N-1446 p2815), the runner's answer
    /// read now from its service, one JSON object: its rows (`crates`, the build machine's compiled crates; `binaries`,
    /// this PC's part), or with ROW a page of that row's entries from FROM (`entries`, `total`), newest use first.
    Store {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        row: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<u64>,
    },
    /// Home's press on one finding of the store page's walk (the owner's walk, items 20 and 23): the runner's service
    /// asked `{"free": PATH}`, which deletes PATH when the last walk found it as a target folder or a record copy and
    /// it still is one (build-runner `free`); the owner's alone. Its answer `{"freed": BYTES, "path"}`.
    StoreFree { path: String },
    /// Home's cold storage (the owner's walk item 20): with no ACT its targets, rules, dry run and what it holds; `evict`
    /// the dry run's items moved now, `restore` the item that was at PATH brought back, `auto` rule RULE made automatic
    /// or manual (ON). All but the read are the owner's.
    Cold {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        act: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rule: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on: Option<bool>,
    },
    /// Page PAGE (from 1) of the owner's PDF at PATH, inside a root as `ReadFile` serves it, drawn by coordd as a PNG
    /// WIDTH pixels wide (default 1200, at most 2400): `{pages, page, data}`, data the PNG's base64 (the owner's walk
    /// item 22: a PDF read inside the window, by nothing of an editor's).
    PdfPage {
        path: String,
        page: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<u32>,
    },
    /// The owner's settings at each level, read with no SCOPE; else KEY (`agent_mode`: `safe`/`yolo`; `safety_check`:
    /// `true`/`false`) set to VALUE at SCOPE (`yaaihh`, `project` or `agent:NAME`), or unset with `""`. Its answer
    /// `{yaaihh: {KEY: VALUE}, project: {KEY: VALUE}, said}`, an unset key null; an agent's own are in its view.
    Settings {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    },
    /// Each repository of the project: its branch, last commit and uncommitted files, `[{name, branch, head, subject,
    /// at, dirty, ahead, behind}]`, a field git did not give left out.
    RepoState,
    /// `coord end JOB WHY`: `RUNNER end JOB --by WHO WHY` (the owner, or an agent whose role is lead): the job ends, and
    /// the step it belonged to answers Ended(by, why).
    End { job: String, why: String },
    /// `coord team grant NAME N`: agent NAME's team may have N agents from now on.
    TeamGrant { name: String, granted: u32 },
    /// The norms' verdict on act ACT with field values ARGS, about mail thread THREAD (`none`), and why.
    Norm { act: String, args: Vec<String>, thread: String },
    /// The models an agent of the project may run (the gateway's and his accounts', Claude Code's and opencode's, as
    /// `run-model` lets them), one per line, Haiku first (yaaihh's New agent).
    Models,
    /// The same models, each a [`harness::ModelEntry`] in a JSON list: where its maker is not named, with what people
    /// guess it is and where that was read, from the one `model-guesses.toml` at the flavor's root (walk item 29;
    /// [`MODEL_LIST_PROTOCOL`]).
    ModelList,
    /// What each harness lets the owner's switches do, in a box and outside one: a JSON list of
    /// [`harness::HarnessCapabilities`], so a switch one cannot take is shown so ([`MODEL_LIST_PROTOCOL`]).
    HarnessCapabilities,
    /// The calling process becomes agent NAME (it then execs the agent).
    Run { name: String },
    /// Session facts of agent NAME known before its first hook (a Codex thread id and app-server socket), merged into
    /// what its hooks report; agents above NAME only.
    Session { name: String, session: Value },
    /// A Claude Code or Codex hook event, with the hook's input.
    Hook { event: String, input: Value },
    /// The numbered event stream, from after `from` when given. RUN (protocol 1): the coordd run `from` was numbered
    /// in (the snapshot's `run`); the stream resumes only in that run, else it starts with a snapshot, since a coordd
    /// started again numbers its events from 0. BOX (N-487): the window start this watch is for (`coordc bridge --box
    /// ID`), so coordd knows the window is open while one is up; an owner's step of a ticket whose form names a box (`window`)
    /// with none up goes on at once. None from an older bridge: its steps go on by time only.
    Watch {
        from: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<String>,
        #[serde(default, rename = "box", skip_serializing_if = "Option::is_none")]
        box_id: Option<String>,
    },
    /// A page of an agent's session as entries (agents below the caller): the lines ending before byte `before`, or
    /// from byte `after` on, or the session's last part; the reply's `out` is a [`Transcript`] as JSON. SESSION: of
    /// that transcript of the agent's (a path, as [`Request::Image`]'s, beside its current one), not its current one:
    /// the old line his system-prompt edit from a turn went off, read on from the cut ([`TRANSCRIPT_SESSION_PROTOCOL`];
    /// path fix 6 point 3). A coordd below it reads the current one whatever is named, so a window names one only to a
    /// coordd at it.
    Transcript {
        name: String,
        #[serde(default)]
        before: Option<u64>,
        #[serde(default)]
        after: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session: Option<String>,
    },
    /// Where the model proxy's log of agent NAME's requests is (agents below the caller, the owner): the raw pane reads
    /// it itself, on the host (N-40); the reply's `out` is a [`RawSource`] as JSON. An opencode agent's are the files
    /// its plugin writes in its own coord folder (`raw/`), under its opencode session.
    RawSource { name: String },
    /// The raw pane's lines of agent NAME's requests (agents below the caller, the owner; [`RAW_PAGE_PROTOCOL`]):
    /// coordd reads the proxy's log where [`RawSource`] says it is, bodies only (the proxylog crate), off its lock.
    /// SPAN: one turn's requests, from its first time to before its second; else the requests before time BEFORE, or
    /// from time AFTER on, or the last ones. The reply's `out` is a `proxylog::RawPage` as JSON.
    RawPage {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<(u64, u64)>,
    },
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
    /// CHANNELS_OFF: Claude Code's own word on the screen, once seen in this session, that it takes no channel message
    /// (its start line "… ignored (server:coord)" and the reason under it, as "Channels are not currently available"):
    /// what coordd hands it would be dropped unseen (paper-research N-1446 p2424; accounts-lead's walk p2423).
    /// NO_SIGN_IN: Claude Code's error line just above its input box that its sign-in is missing or spent ("Not logged
    /// in · Please run /login"), while it stands there: the turn it ended has ended, and what coordd hands it would fail
    /// the same (accounts-lead N-1446 p2446, fix 2).
    Busy {
        name: String,
        busy: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channels_off: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        no_sign_in: Option<String>,
    },
    /// The bridge's own (`coordc bridge`, next to the holders' sockets; coordd refuses them): attach to NAME's
    /// terminal, which answers with a [`PtyEvent::Screen`] and then its output; `rows`/`cols` 0 keep its size.
    Attach { name: String, #[serde(default)] rows: u16, #[serde(default)] cols: u16 },
    /// Bytes typed into NAME's terminal.
    Keys { name: String, #[serde(with = "b64")] data: Vec<u8> },
    /// The size of the view on NAME's terminal, in cells (the latest client to type or resize sets the terminal's).
    Resize { name: String, rows: u16, cols: u16 },
    /// Stop relaying NAME's terminal.
    Detach { name: String },
    /// The bridge's own, like Attach: MESSAGE (one JSON-RPC message) to the coord's shared LANG server
    /// (`$COORD/lsp/LANG.sock`, the project's `[[lsp]]`); its messages come back as `{"lsp":{"lang","message"}}` lines
    /// and its end as `{"lsp":{"lang","closed"}}` (N-412 p130: docket-ls's hovers in the Windows window).
    Lsp { lang: String, message: Value },
    /// The model requests of the caller's agents (all of them for the owner) that ended at or after `since`
    /// (milliseconds since the epoch), as their transcripts record them; the reply's `out` is a list of [`Sent`] as
    /// JSON, oldest first.
    Requests { since: i64 },
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

/// PATH (either separator) as the owner reads it in a line of the window: its last folder and its name
/// (`kinds/a.kind`), not where it sits on the machine that has it (N-802).
pub fn short_path(path: &str) -> String {
    let parts: Vec<&str> = path.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    parts[parts.len().saturating_sub(2)..].join("/")
}

pub mod codeview;
pub mod schedule;
pub mod harness;
pub mod look;
pub mod template;

/// What a Save of a kind or call says when it is refused or half done, in the owner's words (paper-research N-802, 2):
/// one wording for coordd's `EditFile` and the window's own write on Linux. The file is named by its folder and name
/// (`kinds/a.kind`), never by where it sits on the machine that wrote it.
pub mod save_words {
    use crate::short_path as short;
    pub fn not_kinds(path: &str) -> String {
        format!("Not saved: {} is not a kinds or calls file.", short(path))
    }
    pub fn no_folder(path: &str) -> String {
        format!("Not saved: the folder of {} is not there.", short(path))
    }
    pub fn outside(path: &str) -> String {
        format!("Not saved: {} is outside the project's folder and its library.", short(path))
    }
    pub fn protected(path: &str, why: &str) -> String {
        format!("Not saved: {} is in a place that is never written ({why}).", short(path))
    }
    pub fn link(path: &str) -> String {
        format!("Not saved: {} is a link. Edit the file it points to.", short(path))
    }
    pub fn changed(path: &str) -> String {
        format!("Not saved: {} changed since you opened it. Open the edit again to start from its new text.", short(path))
    }
    pub fn exists(path: &str) -> String {
        format!("Not saved: {} was made meanwhile. Open the edit again.", short(path))
    }
    /// Not written: the system's error E (permission, a full disk).
    pub fn unwritten(path: &str, e: impl std::fmt::Display) -> String {
        format!("Not saved: {} could not be written ({e}).", short(path))
    }
    /// Written, but git's STEP (`add`, `commit`) refused, with git's ERR: its first line.
    pub fn uncommitted(step: &str, err: &str) -> String {
        let first = err.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("no reason given");
        format!("Saved, but not committed: git {step} said \u{201c}{first}\u{201d}.")
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn a_refusal_names_the_file_by_its_folder_and_name_whatever_the_machine() {
            assert_eq!(crate::short_path("/home/t/Documents/papers/proj/kinds/a.kind"), "kinds/a.kind");
            assert_eq!(crate::short_path("C:\\Users\\t\\proj\\calls\\post.call"), "calls/post.call");
            assert_eq!(crate::short_path("a.kind"), "a.kind");
            assert_eq!(super::changed("/srv/x/kinds/a.kind"), "Not saved: kinds/a.kind changed since you opened it. Open the edit again to start from its new text.");
            assert_eq!(super::uncommitted("commit", "\nerror: nothing\nmore"), "Saved, but not committed: git commit said \u{201c}error: nothing\u{201d}.");
        }
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
    /// Client: a line for the program (a slash command, N-804.p16), without its Enter. The holder types it, Enter
    /// included, at the first moment no turn runs, no dialog stands and the program's input box holds nothing of
    /// anyone's (nothing typed there, every key sent before drawn), with no key of a client written in between; a
    /// later line replaces one not typed yet. No answer: the program's own record says what it did.
    pub const COMMAND: u8 = b'C';
    /// Client: words for the program, each its own prompt. The holder types them, oldest first, as one bracketed
    /// paste with its Enter (their ESC bytes taken out, so nothing of them is typed as keys) once the program is ready
    /// for one: it has turned bracketed paste on, no `--confirm` text waits for its Enter, and no dialog stands. One a
    /// look. No answer.
    pub const PASTE: u8 = b'P';

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

    /// Who sent keys to NAME's terminal (`$COORD/pty/NAME.keys`): one JSON line `{"ms", "pid", "cgroup"}` each time a
    /// sender other than the last types, and again after a second's pause, written by the holder; coordd reads who typed a
    /// prompt from it (paper-research N-101 p1127).
    pub fn keys(coord: &str, name: &str) -> PathBuf {
        dir(coord).join(format!("{name}.keys"))
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

/// The refusal coordc and channel.py give for a typed post or msg ([`Request::line`]) that coordd took as no request:
/// a coordd from before the kind (I-1480; paper-research N-1481 p48).
pub const KIND_UNTAKEN: &str = "this coordd takes no kind: name whom you ask with @NAME";

impl Request {
    /// The request as its line on coordd's socket. A post or msg that carries a kind goes as `typedpost` or `typedmsg`,
    /// the variants' aliases: a coordd from before the kind refuses that as no request, where it would take `post` and
    /// drop the kind it does not know, so an ask would be posted with nobody named and wait with nothing to end it
    /// (proto-reader N-1481 p47, p54; paper-research p56). Every other request goes as before.
    pub fn line(&self) -> String {
        let Some(op) = self.typed_op() else { return serde_json::to_string(self).unwrap_or_default() };
        let mut v = serde_json::to_value(self).unwrap_or_default();
        v["op"] = op.into();
        v.to_string()
    }

    /// The request's [`line`](Request::line), with agent NAME declared beside it ([`DECLARED`]). coordc sends this in
    /// an agent's session (AGENT_NAME). A coordd that does not read the field takes the line as the request alone.
    pub fn declared_line(&self, name: &str) -> String {
        let mut v: Value = serde_json::from_str(&self.line()).unwrap_or_default();
        v[DECLARED] = name.into();
        v.to_string()
    }

    /// ANSWER to this request as its client says it: a coordd from before the kind refusing the typed op
    /// ([`Request::line`]) is said in [`KIND_UNTAKEN`]'s words. Any other answer as it is.
    pub fn answered(&self, answer: Reply) -> Reply {
        match self.typed_op() {
            Some(op) if !answer.ok && answer.out.contains(&format!("unknown variant `{op}`")) => Reply { ok: false, out: KIND_UNTAKEN.into(), post: None },
            _ => answer,
        }
    }

    /// The op this request goes as when it carries a post's kind.
    fn typed_op(&self) -> Option<&'static str> {
        match self {
            Request::Post { kind: Some(_), .. } => Some("typedpost"),
            Request::Msg { kind: Some(_), .. } => Some("typedmsg"),
            _ => None,
        }
    }
}

/// The field of a request line that names the agent sending it ([`Request::declared_line`]). An agy agent on a
/// sign-in account needs it: its commands run in the account's t3code server, whose cgroup names no agent
/// (paper-research N-1446 p2099; the owner's N-1104 p930). coordd takes the name only from a caller in that server's
/// cgroup, and only a name bound to that account (coordd's `accounts::agy::declares`). From any other caller the field
/// is ignored, and the caller's cgroup names it.
pub const DECLARED: &str = "as";

/// The agent a request LINE declares ([`DECLARED`]), when the line is a JSON object that names one.
pub fn declared(line: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Line {
        #[serde(rename = "as")]
        name: Option<String>,
    }
    serde_json::from_str::<Line>(line).ok()?.name
}

/// coordd's answer to a request (a watch answers with events instead).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    #[serde(default)]
    pub out: String,
    /// The post a post or a send made, its thread's id and its number (N-1788; from [`CONTROL_PROTOCOL`]), so no
    /// reader parses `out`'s words for it. None for any other reply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<(String, u32)>,
}

/// A mail's subject: its first non-empty line.
pub fn subject(body: &str) -> String {
    body.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").chars().take(120).collect()
}

#[cfg(test)]
mod hello_tests {
    use super::*;

    #[test]
    fn a_hello_says_when_the_other_side_is_too_old_or_too_new() {
        let h = |protocol, min| Hello { protocol, min, release: None, refused: None, since: None };
        assert_eq!(Hello::mismatch(&Hello::ours(None)), None, "the same build");
        // an app from before protocol 1 sends no hello and counts as 0, which this build still speaks
        assert_eq!(Hello::mismatch(&h(0, 0)), None);
        assert!(Hello::mismatch(&h(PROTOCOL + 1, PROTOCOL + 1)).is_some_and(|w| w.contains("update this one")));
        assert_eq!(Hello::mismatch(&h(PROTOCOL + 1, PROTOCOL)), None, "a newer side that still speaks this one");
        // read by an older side, which knows none of the fields it adds
        let line = serde_json::to_string(&serde_json::json!({"hello": Hello::ours(Some("r".into()))})).unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!((v["hello"]["protocol"].as_u64(), v["hello"]["min"].as_u64(), v["hello"]["release"].as_str()), (Some(PROTOCOL as u64), Some(PROTOCOL_MIN as u64), Some("r")));
        assert!(v["hello"].get("refused").is_none(), "no refused when none: {line}");
    }

    /// The compaction point's request and fields on the wire (paper-research N-490 p8): op `compact`; `max` and `next`
    /// left out when none, so an older reader sees the context as before, and read back when there.
    #[test]
    fn the_compaction_point_reads_back() {
        let r: Request = serde_json::from_str(r#"{"op":"compact","name":"w","tokens":500000}"#).unwrap();
        assert!(matches!(&r, Request::Compact { name, tokens: 500_000 } if name == "w"));
        assert_eq!(serde_json::to_value(&r).unwrap(), serde_json::json!({"op": "compact", "name": "w", "tokens": 500000}));
        let c = AgentContext { used: 1, window: Some(350_000), max: Some(1_000_000), next: Some(500_000), ..Default::default() };
        assert_eq!(serde_json::from_str::<AgentContext>(&serde_json::to_string(&c).unwrap()).unwrap(), c);
        let old = AgentContext { used: 1, window: Some(350_000), ..Default::default() };
        assert_eq!(serde_json::to_value(&old).unwrap(), serde_json::json!({"used": 1, "window": 350000}));
        assert!(COMPACT_MIN < COMPACT_MAX);
    }

    /// `Request::Duties` and its answer on the wire: op `duties`, `holder` left out when none; a `DutyAnswer` reads
    /// back, its optional fields left out when empty.
    #[test]
    fn the_duties_read_back() {
        let r: Request = serde_json::from_str(r#"{"op":"duties"}"#).unwrap();
        assert!(matches!(r, Request::Duties { holder: None }), "{r:?}");
        assert_eq!(serde_json::to_value(&Request::Duties { holder: Some("summ".into()) }).unwrap(), serde_json::json!({"op": "duties", "holder": "summ"}));
        let bare: DutyAnswer = serde_json::from_str(r#"{"duty":"write-summary"}"#).unwrap();
        assert_eq!(bare, DutyAnswer { duty: "write-summary".into(), ..Default::default() });
        for d in [bare, DutyAnswer { duty: "write-summary".into(), objects: vec!["summ".into(), "owner".into(), "N-1".into()], holder: Some("summ".into()), claimant: Some("owner".into()), violated: true, ended_by: vec!["summarize".into()] }] {
            let line = serde_json::to_string(&d).unwrap();
            assert_eq!(line.contains("violated"), d.violated, "{line}");
            assert_eq!(serde_json::from_str::<DutyAnswer>(&line).unwrap(), d);
        }
    }

    /// RawPage on the wire (N-802.p13): a turn's span read back as sent, the fields not given left out.
    #[test]
    fn a_raw_page_request_reads_back() {
        let asked = Request::RawPage { name: "dev".into(), before: None, after: None, span: Some((5, 9)) };
        assert_eq!(serde_json::to_value(&asked).unwrap(), serde_json::json!({"op": "rawpage", "name": "dev", "span": [5, 9]}));
        let r: Request = serde_json::from_str(&serde_json::to_string(&asked).unwrap()).unwrap();
        assert!(matches!(&r, Request::RawPage { name, before: None, after: None, span: Some((5, 9)) } if name == "dev"), "{r:?}");
        let r: Request = serde_json::from_str(r#"{"op":"rawpage","name":"dev","before":7}"#).unwrap();
        assert!(matches!(r, Request::RawPage { before: Some(7), after: None, span: None, .. }), "{r:?}");
    }

    /// Transcript on the wire (path fix 6 point 3, accounts-lead N-1446 p2651): a request naming no session is the bytes
    /// it was before, which an older coordd reads; one naming a session reads back with it. A page with his edits reads
    /// back whole, Now's with no line, one without them has no key, and a window from before them reads the rest of it.
    #[test]
    fn a_transcript_request_naming_a_session_and_a_page_with_his_edits_read_back() {
        let plain = Request::Transcript { name: "dev".into(), before: None, after: Some(9), session: None };
        assert_eq!(serde_json::to_value(&plain).unwrap(), serde_json::json!({"op": "transcript", "name": "dev", "before": null, "after": 9}));
        let named = Request::Transcript { name: "dev".into(), before: None, after: Some(2100), session: Some("/p/s1.jsonl".into()) };
        let r: Request = serde_json::from_str(&serde_json::to_string(&named).unwrap()).unwrap();
        assert!(matches!(&r, Request::Transcript { before: None, after: Some(2100), session: Some(s), .. } if s == "/p/s1.jsonl"), "{r:?}");
        use docket_machine::graph::{Branched, Restarted};
        let line = Some(Branched { k: 2, from: "/p/s1.jsonl".into(), from_line: 1, turns: 7 });
        let restarted = vec![
            Restarted { offset: 2100, at: "2026-11-24T10:12:00Z".into(), system: "2".into(), line },
            Restarted { offset: 2900, at: "2026-11-24T10:30:00Z".into(), system: "3".into(), line: None },
        ];
        let page = Transcript { next: 2900, session: "/p/c0ff.jsonl".into(), restarted, ..Default::default() };
        let text = serde_json::to_string(&page).unwrap();
        assert_eq!(serde_json::from_str::<Transcript>(&text).unwrap(), page);
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["restarted"][1], serde_json::json!({"offset": 2900, "at": "2026-11-24T10:30:00Z", "system": "3"}), "Now: no line");
        assert!(serde_json::to_value(Transcript::default()).unwrap().get("restarted").is_none());
        // a window from before them: the page as it knew it
        #[derive(Deserialize)]
        struct Before {
            entries: Vec<Entry>,
            start: u64,
            next: u64,
            session: String,
        }
        let b: Before = serde_json::from_str(&text).unwrap();
        assert_eq!((b.entries.len(), b.start, b.next, b.session.as_str()), (0, 0, 2900, "/p/c0ff.jsonl"));
    }

    /// Record on the wire (proto-window N-1446 p84): the post it starts at and the page after a line left out when not
    /// given; a bare request is a ticket's record from its opening, RECORD_CAP lines; a line reads back as written.
    #[test]
    fn a_record_request_reads_back() {
        let asked = Request::Record { t: "N-1104".into(), at: Some(587), after: None, cap: 0 };
        assert_eq!(serde_json::to_value(&asked).unwrap(), serde_json::json!({"op": "record", "t": "N-1104", "at": 587, "cap": 0}));
        let r: Request = serde_json::from_str(r#"{"op":"record","t":"N-1104"}"#).unwrap();
        assert!(matches!(&r, Request::Record { t, at: None, after: None, cap: 0 } if t == "N-1104"), "{r:?}");
        let l = RecordLine { line: 101208, text: r#"{"ev":"post","at":"2026-11-20T18:23:17Z","t":1104,"by":"owner"}"#.into() };
        assert_eq!(serde_json::from_str::<RecordLine>(&serde_json::to_string(&l).unwrap()).unwrap(), l);
    }

    /// DryRun on the wire (H2, docket-edge8 N-1446 p1824): a draft of the program with its file; his prompt for one
    /// thread with none, and the ticket left out when not given. Its answer reads back, and a bare one (none waiting)
    /// names no ticket.
    #[test]
    fn a_dry_run_request_reads_back() {
        let asked = Request::DryRun { call: "answer".into(), on: Some("Q-5".into()), path: Some("/p/calls/answer.call".into()), text: "call answer(ask: Text)".into() };
        assert_eq!(serde_json::to_value(&asked).unwrap(), serde_json::json!({"op": "dryrun", "call": "answer", "on": "Q-5", "path": "/p/calls/answer.call", "text": "call answer(ask: Text)"}));
        let r: Request = serde_json::from_str(r#"{"op":"dryrun","call":"answer","text":"Say {{ ask }}"}"#).unwrap();
        assert!(matches!(&r, Request::DryRun { call, on: None, path: None, text } if call == "answer" && text == "Say {{ ask }}"), "{r:?}");
        let a = DryRunAnswer { on: Some("Q-5".into()), step: 2, by: "pr".into(), asked: true, own: false, text: "A question for you:".into() };
        assert_eq!(serde_json::from_str::<DryRunAnswer>(&serde_json::to_string(&a).unwrap()).unwrap(), a);
        assert_eq!(serde_json::to_value(DryRunAnswer::default()).unwrap(), serde_json::json!({"step": 0, "by": "", "asked": false, "own": false, "text": ""}));
        assert_eq!(serde_json::from_str::<DryRunAnswer>("{}").unwrap(), DryRunAnswer::default());
    }

    /// Add a repository on the wire (fix 3, the project's Home): its op `addrepo`, its name and folder as given; the
    /// protocol that brings it is the window's and coordd's own.
    #[test]
    fn an_add_repo_request_reads_back() {
        let asked = Request::AddRepo { name: "tovrin".into(), path: "~/code/tovrin".into() };
        assert_eq!(serde_json::to_value(&asked).unwrap(), serde_json::json!({"op": "addrepo", "name": "tovrin", "path": "~/code/tovrin"}));
        assert!(matches!(serde_json::from_str::<Request>(&asked.line()), Ok(Request::AddRepo { name, path }) if name == "tovrin" && path == "~/code/tovrin"));
        assert!(ADD_REPO_PROTOCOL <= PROTOCOL);
    }

    /// The browser viewer's request is gone (N-1866), its number kept; the numbers after it as each release set them.
    #[test]
    fn the_repo_view_request_is_gone_and_the_numbers_stay() {
        assert!(serde_json::from_str::<Request>(r#"{"op":"repoview","name":"yaaihh"}"#).is_err());
        assert!(REPO_VIEW_PROTOCOL > STORE_PROTOCOL && REPO_VIEW_PROTOCOL < MODEL_LIST_PROTOCOL);
        for p in [MODEL_LIST_PROTOCOL, CONTROL_PROTOCOL, STORE_FREE_PROTOCOL, HOME_PROTOCOL, SPIN_PROTOCOL] {
            assert_eq!(p, 59);
        }
        for p in [REWIND_PROTOCOL, COMMENTS_PROTOCOL, BRAIN_READY_PROTOCOL, COLD_PROTOCOL, USAGE_CELLS, CODE_VIEW_PROTOCOL] {
            assert_eq!(p, 60);
        }
        assert_eq!((STANDING_PROTOCOL, GIVEN_PROTOCOL, CODE_WARM_PROTOCOL, AGY_MODE_PROTOCOL, SCHEDULE_PROTOCOL, READ_ASK_PROTOCOL), (61, 62, 63, 64, 65, PROTOCOL));
        assert_eq!(serde_json::to_value(Request::Standing).unwrap(), serde_json::json!({"op": "standing"}));
        assert_eq!(serde_json::to_value(Request::Given { name: "pa-temp".into() }).unwrap(), serde_json::json!({"op": "given", "name": "pa-temp"}));
    }

    /// The owner's turn-by-turn control (N-1788): each request's line as the window sends it, read back; Esc without
    /// `step` is the line it always was; a reply's post rides beside its words and is left out when there is none; an
    /// agent from a coordd before it reads as Run with no capabilities.
    #[test]
    fn the_control_requests_read_back() {
        let asked = Request::HaltReject { ticket: 12, halt: 3, reason: Some("not this one".into()) };
        assert_eq!(serde_json::to_value(&asked).unwrap(), serde_json::json!({"op": "haltreject", "ticket": 12, "halt": 3, "reason": "not this one"}));
        assert!(matches!(serde_json::from_str::<Request>(&asked.line()), Ok(Request::HaltReject { ticket: 12, halt: 3, reason: Some(r) }) if r == "not this one"));
        let asked = Request::DeliverUrgent { agent: "w1".into(), interrupt: true };
        assert_eq!(serde_json::to_value(&asked).unwrap(), serde_json::json!({"op": "deliverurgent", "agent": "w1", "interrupt": true}));
        let asked = Request::AgentPace { name: "w1".into(), mode: AgentMode::Step };
        assert_eq!(serde_json::to_value(&asked).unwrap(), serde_json::json!({"op": "agentpace", "name": "w1", "mode": "step"}));
        assert!(matches!(serde_json::from_str::<Request>(&asked.line()), Ok(Request::AgentPace { mode: AgentMode::Step, .. })));
        assert_eq!(serde_json::to_value(Request::Esc { to: vec!["w1".into()], step: false }).unwrap(), serde_json::json!({"op": "esc", "to": ["w1"]}));
        assert!(matches!(serde_json::from_str::<Request>(r#"{"op": "esc", "to": ["w1"], "step": true}"#), Ok(Request::Esc { step: true, .. })));
        let r = Reply { ok: true, out: "Posted on N-12 as post 4.".into(), post: Some(("N-12".into(), 4)) };
        assert_eq!(serde_json::to_value(&r).unwrap(), serde_json::json!({"ok": true, "out": "Posted on N-12 as post 4.", "post": ["N-12", 4]}));
        assert_eq!(serde_json::to_value(Reply { ok: true, out: String::new(), post: None }).unwrap(), serde_json::json!({"ok": true, "out": ""}));
        assert_eq!(serde_json::from_str::<HarnessCapabilities>("{}").unwrap(), HarnessCapabilities::default());
        assert_eq!(serde_json::from_str::<HarnessSwitchesView>("{}").unwrap(), HarnessSwitchesView::default());
        let sw = HarnessSwitchesView { safety_check: true, tools_off: vec!["WebSearch".into()], parts_off: vec![], from: [("tools_off".to_string(), "project".to_string())].into() };
        assert_eq!(serde_json::to_value(&sw).unwrap(), serde_json::json!({"safety_check": true, "tools_off": ["WebSearch"], "parts_off": [], "from": {"tools_off": "project"}}));
        let asked = Request::HarnessSwitches { name: "w1".into(), safety_check: None, tools_off: Some(vec!["WebFetch".into()]), parts_off: None };
        assert_eq!(serde_json::to_value(&asked).unwrap(), serde_json::json!({"op": "harnessswitches", "name": "w1", "tools_off": ["WebFetch"]}));
        assert!(matches!(serde_json::from_str::<Request>(&asked.line()), Ok(Request::HarnessSwitches { safety_check: None, tools_off: Some(t), parts_off: None, .. }) if t == ["WebFetch"]));
    }

    #[test]
    fn a_usage_cells_request_reads_back() {
        let asked = Request::UsageCells { after: 42 };
        assert_eq!(serde_json::to_value(&asked).unwrap(), serde_json::json!({"op": "usage_cells", "after": 42}));
        assert!(matches!(serde_json::from_str::<Request>(&asked.line()), Ok(Request::UsageCells { after: 42 })));
        assert!(USAGE_CELLS < PROTOCOL);
    }

    /// Insight on the wire (S6): the insight by its name.
    #[test]
    fn an_insight_request_reads_back() {
        let asked = Request::Insight { which: Insight::OpenToDone };
        assert_eq!(serde_json::to_value(asked).unwrap(), serde_json::json!({"op": "insight", "which": "open-to-done"}));
        let r: Request = serde_json::from_str(r#"{"op":"insight","which":"open-to-done"}"#).unwrap();
        assert!(matches!(r, Request::Insight { which: Insight::OpenToDone }), "{r:?}");
        assert_eq!(serde_json::to_value(Request::Insight { which: Insight::WorkTalk }).unwrap(), serde_json::json!({"op": "insight", "which": "work-talk"}));
        assert_eq!(serde_json::to_value(Request::Insight { which: Insight::Asks }).unwrap(), serde_json::json!({"op": "insight", "which": "asks"}));
    }

    /// His sign-off on the wire (paper-research N-1446 p1786): op `signoff`, his post and his word; from ASKS_PROTOCOL,
    /// which no older coordd says.
    #[test]
    fn a_sign_off_request_reads_back() {
        let asked = Request::SignOff { post: "N-1104.p113".into(), worth: "worth_it".into() };
        assert_eq!(serde_json::to_value(&asked).unwrap(), serde_json::json!({"op": "signoff", "post": "N-1104.p113", "worth": "worth_it"}));
        let r: Request = serde_json::from_str(r#"{"op":"signoff","post":"N-16.p1","worth":"none"}"#).unwrap();
        assert!(matches!(&r, Request::SignOff { post, worth } if post == "N-16.p1" && worth == "none"), "{r:?}");
        assert!(ASKS_PROTOCOL > INSIGHT_PROTOCOL && ASKS_PROTOCOL <= PROTOCOL);
    }

    /// The window's release on the wire (I-1457): its name only.
    /// I-1480 (proto-reader N-1481 p54, paper-research p56): an untyped post or msg goes as before; a typed one goes as
    /// `typedpost` or `typedmsg` and reads back with its kind; a coordd from before the kind, whose Request has no such
    /// variant, refuses that line as no request (main.rs, before anything is posted), and the client says so in
    /// `KIND_UNTAKEN`'s words, as channel.py does.
    /// N-1446 p2099: an agent's coordc declares its name beside the request. The line reads back as the same request
    /// (a typed one and one with no fields too), and the name is read from it. A line that declares nothing, or is
    /// no JSON, names nobody.
    #[test]
    fn a_declared_name_rides_beside_the_request_and_reads_back() {
        use docket_machine::PostKind;
        let post = |kind| Request::Post { t: "N-12".into(), text: "check the lock".into(), to: None, answers: None, wait: false, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), kind, comments: Vec::new() };
        let line = post(None).declared_line("gemini-advisor");
        assert_eq!(declared(&line).as_deref(), Some("gemini-advisor"));
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap()[DECLARED], "gemini-advisor");
        assert!(matches!(serde_json::from_str::<Request>(&line), Ok(Request::Post { t, kind: None, .. }) if t == "N-12"));
        let typed = post(Some(PostKind::Ask { to: vec!["w2".into()] })).declared_line("a");
        assert_eq!(serde_json::from_str::<Value>(&typed).unwrap()["op"], "typedpost");
        assert!(matches!(serde_json::from_str::<Request>(&typed), Ok(Request::Post { kind: Some(PostKind::Ask { .. }), .. })));
        assert!(matches!(serde_json::from_str::<Request>(&Request::Pty.declared_line("a")), Ok(Request::Pty)));
        assert_eq!(declared(&post(None).line()), None);
        assert_eq!(declared("not json"), None);
    }

    #[test]
    fn a_typed_post_goes_as_its_own_op_and_a_coordd_from_before_refuses_it() {
        use docket_machine::PostKind;
        let post = |kind| Request::Post { t: "N-12".into(), text: "check the lock".into(), to: None, answers: None, wait: false, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), kind, comments: Vec::new() };
        let msg = |kind| Request::Msg { to: Vec::new(), body: "check the lock".into(), esc: false, soon: false, wait: false, as_host: None, re: Some("N-12".into()), quotes: Vec::new(), comments: Vec::new(), origin: None, under: None, label: None, kind };
        let ask = || Some(PostKind::Ask { to: vec!["w2".into()] });
        let op = |r: &Request| serde_json::from_str::<Value>(&r.line()).unwrap()["op"].clone();
        assert_eq!((op(&post(None)), op(&msg(None))), (Value::from("post"), Value::from("msg")));
        assert_eq!((op(&post(ask())), op(&msg(ask()))), (Value::from("typedpost"), Value::from("typedmsg")));
        assert_eq!(post(None).line(), serde_json::to_string(&post(None)).unwrap(), "an untyped post's line as before");
        assert!(matches!(serde_json::from_str::<Request>(&post(ask()).line()), Ok(Request::Post { kind: Some(PostKind::Ask { to }), .. }) if to == ["w2"]));
        assert!(matches!(serde_json::from_str::<Request>(&msg(ask()).line()), Ok(Request::Msg { kind: Some(PostKind::Ask { .. }), .. })));
        // a coordd from before: its Request has `post` and `msg` and no alias, and it drops a field it does not know
        #[derive(Debug, Deserialize)]
        #[serde(tag = "op", rename_all = "lowercase")]
        #[allow(dead_code)]
        enum Before {
            Post { t: String, text: String },
            Msg { body: String },
        }
        let before = |r: &Request| match serde_json::from_str::<Before>(&r.line()) {
            Ok(_) => Reply { ok: true, out: "Posted on N-12 as post 5.".into(), post: None },
            Err(e) => Reply { ok: false, out: format!("coordd: not a request: {e}"), post: None },
        };
        for r in [post(ask()), msg(ask())] {
            assert_eq!(r.answered(before(&r)), Reply { ok: false, out: KIND_UNTAKEN.into(), post: None }, "{}", r.line());
        }
        assert_eq!(post(None).answered(before(&post(None))), Reply { ok: true, out: "Posted on N-12 as post 5.".into(), post: None });
        // any other answer is the client's as it is
        let refused = Reply { ok: false, out: "coord post: ask names nobody: name whom it asks".into(), post: None };
        assert_eq!(post(ask()).answered(refused.clone()), refused);
        // channel.py's reply tool sends a typed post as `typedpost` and says the same words
        let py = std::fs::read_to_string(format!("{}/../../agent-base/claude/channel.py", env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert!(py.lines().any(|l| l == format!("KIND_UNTAKEN = {KIND_UNTAKEN:?}")), "channel.py's KIND_UNTAKEN");
        assert!(py.contains(r#"req["op"] = "typedpost""#) && py.contains("unknown variant `typedpost`"), "channel.py's typed op");
    }

    #[test]
    fn a_window_release_request_reads_back() {
        let asked = Request::WindowRelease { release: "20261120-232357-a633aa96".into(), since: None };
        assert_eq!(serde_json::to_value(&asked).unwrap(), serde_json::json!({"op": "window_release", "release": "20261120-232357-a633aa96"}));
        let r: Request = serde_json::from_value(serde_json::to_value(&asked).unwrap()).unwrap();
        assert!(matches!(r, Request::WindowRelease { release, since: None } if release == "20261120-232357-a633aa96"));
        // with his window's time (paper-research N-1446 p1392): it reads back, and a hello carries it the same way
        let asked = Request::WindowRelease { release: "20261121-101928-4ee5c14e".into(), since: Some("2026-11-21T09:34:49Z".into()) };
        let v = serde_json::to_value(&asked).unwrap();
        assert_eq!(v, serde_json::json!({"op": "window_release", "release": "20261121-101928-4ee5c14e", "since": "2026-11-21T09:34:49Z"}));
        assert!(matches!(serde_json::from_value(v).unwrap(), Request::WindowRelease { since: Some(s), .. } if s == "2026-11-21T09:34:49Z"));
        let h = Hello { since: Some("2026-11-21T09:34:49Z".into()), ..Hello::ours(Some("20261121-101928-4ee5c14e".into())) };
        let back: Hello = serde_json::from_value(serde_json::to_value(&h).unwrap()).unwrap();
        assert_eq!(back, h);
        assert_eq!(serde_json::to_value(Hello::ours(None)).unwrap().get("since"), None, "a hello with no time writes none");
    }

    /// His press on a gateway login on the wire (N-1446 p957, p959): op `gatewaycleared`, the login's family; from
    /// GATEWAY_CLEARED_PROTOCOL on, which no older coordd says.
    #[test]
    fn a_gateway_cleared_request_reads_back() {
        let asked = Request::GatewayCleared { family: "claude".into() };
        assert_eq!(serde_json::to_value(&asked).unwrap(), serde_json::json!({"op": "gatewaycleared", "family": "claude"}));
        let r: Request = serde_json::from_str(r#"{"op":"gatewaycleared","family":"codex"}"#).unwrap();
        assert!(matches!(r, Request::GatewayCleared { family } if family == "codex"));
        assert!(GATEWAY_CLEARED_PROTOCOL > INSIGHT_PROTOCOL && GATEWAY_CLEARED_PROTOCOL <= PROTOCOL);
    }

    /// ListDir's git states on the wire (N-607): `git` left out when false and read as false when absent (an older
    /// window's request); an entry's `outside` and `git` left out when none, an older coordd's entry read whole.
    #[test]
    fn a_listings_git_states_read_back() {
        let r: Request = serde_json::from_str(r#"{"op":"listdir","path":"/x"}"#).unwrap();
        assert!(matches!(&r, Request::ListDir { path, git: false } if path == "/x"), "{r:?}");
        assert_eq!(serde_json::to_value(&Request::ListDir { path: "/x".into(), git: false }).unwrap(), serde_json::json!({"op": "listdir", "path": "/x"}));
        let r: Request = serde_json::from_str(&serde_json::to_string(&Request::ListDir { path: "/x".into(), git: true }).unwrap()).unwrap();
        assert!(matches!(r, Request::ListDir { git: true, .. }));
        let old: Vec<DirEntry> = serde_json::from_str(r#"[{"name":"src","dir":true},{"name":"keys","dir":true,"outside":true}]"#).unwrap();
        assert_eq!(old[1], DirEntry { name: "keys".into(), dir: true, outside: true, git: None });
        for e in [DirEntry { name: "a.md".into(), dir: false, outside: false, git: None }, DirEntry { name: "target".into(), dir: true, outside: false, git: Some("ignored".into()) }, DirEntry { name: "n.rs".into(), dir: false, outside: true, git: Some("new".into()) }] {
            let line = serde_json::to_string(&e).unwrap();
            assert_eq!((line.contains("outside"), line.contains("git")), (e.outside, e.git.is_some()), "{line}");
            assert_eq!(serde_json::from_str::<DirEntry>(&line).unwrap(), e);
        }
    }

    /// The Accounts page with what [`ACCOUNT_LEAVE`] adds, read back as the window reads it: the page is a reply's
    /// `out` (`Request::Accounts { json: true }`), not an event, so [`NativeAccounts`] is the type decoded. An older
    /// coordd's page has none of the three fields and reads as none of them (docket-core3, N-412 post 631).
    #[test]
    fn the_accounts_page_reads_back_with_an_account_being_removed_and_a_removed_one_not_signed_out() {
        let page = NativeAccounts {
            coord: "research".into(),
            month: "2026-11".into(),
            accounts: vec![NativeAccount { harness: "claude".into(), id: "work".into(), leaving: Some(1_795_000_000), locked_by: Some("docket".into()), ..Default::default() }],
            removed: vec![RemovedAccount { harness: "claude".into(), id: "old".into(), label: "Old".into(), name: "old.20261119-004100".into(), at: 1_795_000_860, not_signed_out: true }],
            ..Default::default()
        };
        let text = serde_json::to_string(&page).unwrap();
        assert!(text.contains(r#""not_signed_out":true"#) && text.contains(r#""locked_by":"docket""#), "{text}");
        assert_eq!(serde_json::from_str::<NativeAccounts>(&text).unwrap_or_else(|e| panic!("the window cannot read it: {e}\n{text}")), page);
        let mut older: Value = serde_json::from_str(&text).unwrap();
        for field in ["leaving", "locked_by"] {
            older["accounts"][0].as_object_mut().unwrap().remove(field);
        }
        older["removed"][0].as_object_mut().unwrap().remove("not_signed_out");
        let read: NativeAccounts = serde_json::from_value(older).unwrap();
        assert_eq!((read.accounts[0].leaving, read.accounts[0].locked_by.clone(), read.removed[0].not_signed_out), (None, None, false));
    }

    /// I-1469's fields on the Accounts page (proto-lead's read, I-1469 p22 point 5): an account's plan and extra usage,
    /// a reset kind's reason it cannot be used and what it gives back, a press to try again. They read back, and each
    /// is left out of the line when none; an older coordd's page has none of them and reads as none; a window that
    /// does not know them reads the rest. The page is a reply's `out`, not an event, so the page is what is decoded.
    #[test]
    fn the_accounts_page_reads_back_with_a_plan_extra_usage_and_a_resets_words() {
        let page = NativeAccounts {
            coord: "research".into(),
            month: "2026-11".into(),
            accounts: vec![NativeAccount {
                harness: "claude".into(),
                id: "work".into(),
                plan: Some("max".into()),
                extra_usage_until: Some(1_795_000_000),
                resets: vec![
                    ResetOffer { kind: "Fable reset".into(), left: Some(2), gives: Some("frees up only what Fable used this week".into()), ..Default::default() },
                    ResetOffer { kind: "5-hour reset".into(), why: Some("can't be used while you're at your weekly limit".into()), ..Default::default() },
                ],
                reset: Some(ResetTry { kind: "full reset".into(), since: 1_794_999_000, refused: Some("Couldn't confirm the reset went through".into()), again: true, ..Default::default() }),
                ..Default::default()
            }],
            ..Default::default()
        };
        let text = serde_json::to_string(&page).unwrap();
        assert_eq!(serde_json::from_str::<NativeAccounts>(&text).unwrap_or_else(|e| panic!("the window cannot read it: {e}\n{text}")), page);
        let bare = serde_json::to_string(&NativeAccount { resets: vec![ResetOffer::default()], reset: Some(ResetTry::default()), ..Default::default() }).unwrap();
        assert!(["plan", "extra_usage_until", "why", "gives", "\"again\"", "\"login\""].iter().all(|f| !bare.contains(f)), "{bare}");
        // a press refused for its login: said, and none from an older coordd
        let login = ResetTry { kind: "full reset".into(), since: 1, refused: Some("Couldn't reset your limits with this login".into()), login: true, ..Default::default() };
        assert_eq!(serde_json::from_str::<ResetTry>(&serde_json::to_string(&login).unwrap()).unwrap(), login);
        assert!(!serde_json::from_str::<ResetTry>(r#"{"kind": "full reset", "since": 1}"#).unwrap().login);
        let mut want = page.clone();
        let a = &mut want.accounts[0];
        (a.plan, a.extra_usage_until) = (None, None);
        a.resets.iter_mut().for_each(|o| (o.why, o.gives) = (None, None));
        a.reset.as_mut().unwrap().again = false;
        let fields = ["plan", "extra_usage_until", "why", "gives", "again"];
        // an older coordd's page: none of them
        let mut older: Value = serde_json::from_str(&text).unwrap();
        let unknown = |f: &str| format!("{f}_unknown_here");
        for f in fields {
            assert_eq!(super::whereabouts_read_by_the_window::rename(&mut older, f, &unknown(f)), 1, "{f}");
        }
        let mut newer = older.clone();
        strip(&mut older, &fields.map(unknown));
        assert_eq!(serde_json::from_value::<NativeAccounts>(older).unwrap(), want, "an older coordd's page");
        // a window that does not know them: the names are none of its own, and it reads the rest
        assert_eq!(serde_json::from_value::<NativeAccounts>(newer.take()).unwrap_or_else(|e| panic!("an older window cannot read it: {e}")), want);
    }

    /// V without the keys NAMES, wherever they stand.
    fn strip(v: &mut Value, names: &[String]) {
        match v {
            Value::Object(o) => {
                o.retain(|k, _| !names.contains(k));
                o.values_mut().for_each(|x| strip(x, names));
            }
            Value::Array(a) => a.iter_mut().for_each(|x| strip(x, names)),
            _ => {}
        }
    }

    /// `Request::Enabled` and its answer on the wire (part 6, N-568 p15): op `enabled`, both fields optional and left
    /// out when none; an `ActAnswer` reads back, `why` left out when enabled.
    #[test]
    fn the_enabled_acts_read_back() {
        let r: Request = serde_json::from_str(r#"{"op":"enabled","thing":"N-3"}"#).unwrap();
        assert!(matches!(&r, Request::Enabled { thing: Some(t), actor: None } if t == "N-3"), "{r:?}");
        assert_eq!(serde_json::to_value(&Request::Enabled { thing: None, actor: None }).unwrap(), serde_json::json!({"op": "enabled"}));
        for a in [ActAnswer { act: "esc".into(), objects: vec!["dev".into()], enabled: true, why: None, says: None }, ActAnswer { act: "edit".into(), objects: vec!["N-3".into()], enabled: false, why: Some("`x` fails".into()), says: None }, ActAnswer { act: "edit".into(), objects: vec!["N-3".into()], enabled: false, why: Some("`x` fails".into()), says: Some("only the thread's lead edits".into()) }] {
            let line = serde_json::to_string(&a).unwrap();
            assert_eq!(line.contains("why"), !a.enabled, "{line}");
            assert_eq!(line.contains("says"), a.says.is_some(), "{line}");
            assert_eq!(serde_json::from_str::<ActAnswer>(&line).unwrap(), a);
        }
    }
}

#[cfg(test)]
mod job_tests {
    use super::*;

    fn agent() -> serde_json::Value {
        serde_json::json!({"id": "", "name": "a", "parent": "", "kind": "claude", "model": "", "state": "idle", "since": "",
            "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})
    }

    /// An agent's jobs, its background work and its brain (`BrainView`, the window's Brain switch; proto-reader N-1446
    /// p912) come through an event as sent (flattened `Event`, N-541), and a view without them, an older coordd's, reads
    /// as before and is sent as before.
    #[test]
    fn an_agents_jobs_background_work_and_brain_decode_through_an_event() {
        let mut a: AgentView = serde_json::from_value(agent()).unwrap();
        assert!(a.jobs.is_empty() && a.background.is_empty() && a.brain.is_none(), "an older coordd's view has none");
        let bare = serde_json::to_value(&a).unwrap();
        assert!(bare.get("jobs").is_none() && bare.get("background").is_none() && bare.get("brain").is_none(), "left out when empty: {bare}");
        a.jobs = vec![
            Job { id: "41233-1763619362".into(), verb: "test".into(), packages: vec!["yaaihh".into()], branch: Some("edge5-typing".into()),
                  commit: Some("ec7f09cb".into()), machine: Some("yaaihh-build-1".into()), since: "2026-11-20T07:16:02+0100".into(),
                  step: "testing".into(), step_since: "2026-11-20T07:18:40+0100".into(), what: Some("yaaihh".into()), done: Some(181),
                  of: Some(293), failed: Some(1), ..Default::default() },
            Job { id: "40100-1763619000".into(), verb: "build".into(), target: Some("x86_64-pc-windows-gnu".into()), since: "2026-11-20T07:10:00+0100".into(),
                  step: "ended".into(), step_since: "2026-11-20T07:12:00+0100".into(), rc: Some(0), fresh: true, ..Default::default() },
        ];
        a.background = vec![Tool { id: "bash_7".into(), tool: "Bash".into(), what: "dd if=live.img of=/dev/sdb".into(),
                                   at: "2026-11-19T13:02:00Z".into(), about: Some("Write the new version onto the stick".into()) }];
        // as coordd's `brain_view` writes it: off since his press, its last pass opened when, and its id
        a.brain = Some(BrainView { on: false, since: Some("2026-11-21T10:20:00Z".into()), last_pass: Some("2026-11-21T09:42:00Z".into()), last_id: Some("B-12".into()), ready: Vec::new() });
        let line = serde_json::to_string(&Event { seq: 3, body: Body::Agent(a.clone()) }).unwrap();
        let back: Event = serde_json::from_str(&line).unwrap();
        assert_eq!(back.body, Body::Agent(a.clone()), "{line}");
        assert!(a.jobs[0].runs() && !a.jobs[1].runs());
        assert!(!Job { step: "cut".into(), ..Default::default() }.runs());
    }

    /// An older window reads a view that carries jobs: the fields it does not know are passed over. The older view is
    /// spelled here with the fields the window had before them.
    #[test]
    fn a_view_with_jobs_is_read_by_a_window_that_does_not_know_them() {
        #[derive(Deserialize)]
        struct Older {
            name: String,
            state: String,
        }
        let mut v = agent();
        v["jobs"] = serde_json::json!([{"id": "1-2", "verb": "test", "since": "s", "step": "queued", "step_since": "s", "ahead": 3, "a_field_of_later": true}]);
        v["background"] = serde_json::json!([{"id": "bash_7", "tool": "Bash", "what": "x", "at": "t"}]);
        let old: Older = serde_json::from_value(v.clone()).unwrap();
        assert_eq!((old.name.as_str(), old.state.as_str()), ("a", "idle"));
        // and this window reads a job that carries a field of a later runner's
        let now: AgentView = serde_json::from_value(v).unwrap();
        assert_eq!((now.jobs[0].step.as_str(), now.jobs[0].ahead, now.background[0].about.clone()), ("queued", Some(3), None));
    }
}

#[cfg(test)]
mod compact_tests {
    use super::*;

    /// An agent's whole window and its point for the next start come through an event as sent (flattened `Event`),
    /// and a view without them reads as before.
    #[test]
    fn a_context_with_its_whole_window_and_next_point_decodes_through_an_event() {
        let mut a: AgentView = serde_json::from_value(serde_json::json!({"id": "", "name": "a", "parent": "", "kind": "claude", "model": "",
            "state": "idle", "since": "", "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap();
        a.context = Some(AgentContext { used: 143_400, window: Some(330_000), max: Some(1_000_000), next: Some(580_000), ..Default::default() });
        let line = serde_json::to_string(&Event { seq: 3, body: Body::Agent(a.clone()) }).unwrap();
        let back: Event = serde_json::from_str(&line).unwrap();
        assert_eq!(back.body, Body::Agent(a.clone()));
        // its parts by kind (N-804.p7) come through too, and are left out when none
        let parts = vec![ContextPart { kind: "start".into(), tokens: 20_000 }, ContextPart { kind: "tool_results".into(), tokens: 123_400 }];
        a.context = Some(AgentContext { used: 143_400, parts: parts.clone(), ..Default::default() });
        let line = serde_json::to_string(&Event { seq: 3, body: Body::Agent(a.clone()) }).unwrap();
        let back: Event = serde_json::from_str(&line).unwrap();
        assert!(matches!(&back.body, Body::Agent(b) if b.context.as_ref().is_some_and(|c| c.parts == parts)), "{line}");
        // how the next point reaches the running session (N-804.p16)
        a.context = Some(AgentContext { used: 143_400, window: Some(350_000), next: Some(600_000), next_by: Some("typing".into()), ..Default::default() });
        let line = serde_json::to_string(&Event { seq: 3, body: Body::Agent(a.clone()) }).unwrap();
        let back: Event = serde_json::from_str(&line).unwrap();
        assert!(matches!(&back.body, Body::Agent(b) if b.context.as_ref().is_some_and(|c| c.next_by.as_deref() == Some("typing"))), "{line}");
        a.context = Some(AgentContext { used: 1, window: None, ..Default::default() });
        let line = serde_json::to_string(&Event { seq: 4, body: Body::Agent(a) }).unwrap();
        assert!(!line.contains("\"max\"") && !line.contains("\"next\"") && !line.contains("\"parts\"") && !line.contains("\"next_by\""), "left out when none: {line}");
        let r: Request = serde_json::from_str(r#"{"op":"restart","name":"a"}"#).unwrap();
        assert!(matches!(r, Request::Restart { ref name } if name == "a"));
        let r: Request = serde_json::from_str(r#"{"op":"compact","name":"a","tokens":600000}"#).unwrap();
        assert!(matches!(r, Request::Compact { ref name, tokens: 600_000 } if name == "a"));
    }

    /// 0.2 line 25 (N-1400): `Mode` on the wire as the window sends it, and the view's `mode`, `mode_due` and
    /// `restarting` read back through an event; a safe agent with nothing due and no restart running writes none of
    /// them, so a view reads as before.
    #[test]
    fn an_agents_mode_and_its_restart_decode_through_an_event() {
        let r: Request = serde_json::from_str(r#"{"op":"mode","name":"a","mode":"yolo"}"#).unwrap();
        assert!(matches!(&r, Request::Mode { name, mode } if name == "a" && mode == "yolo"), "{r:?}");
        assert_eq!(serde_json::to_value(&Request::Mode { name: "a".into(), mode: "safe".into() }).unwrap(), serde_json::json!({"op": "mode", "name": "a", "mode": "safe"}));
        let mut a: AgentView = serde_json::from_value(serde_json::json!({"id": "", "name": "a", "parent": "", "kind": "claude", "model": "",
            "state": "idle", "since": "", "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap();
        assert_eq!((a.mode.as_deref(), a.mode_due.as_deref(), a.restarting), (None, None, false), "a view from before reads as safe, nothing due, no restart");
        let line = serde_json::to_string(&Event { seq: 1, body: Body::Agent(a.clone()) }).unwrap();
        assert!(!line.contains("\"mode\"") && !line.contains("\"mode_due\"") && !line.contains("\"restarting\""), "left out when safe and not restarting: {line}");
        // a failed restart into Safe: the session runs in Yolo, Safe is due
        (a.mode, a.mode_due, a.restarting) = (Some("yolo".into()), Some("safe".into()), true);
        let line = serde_json::to_string(&Event { seq: 2, body: Body::Agent(a.clone()) }).unwrap();
        let back: Event = serde_json::from_str(&line).unwrap();
        assert_eq!(back.body, Body::Agent(a), "{line}");
    }
}

/// The words program's new view fields as the window reads them (paper-research N-487 p42; N-541: a field that
/// decoded in coordd's tests and not in the window): a whole snapshot and a ticket event, through this crate's own
/// `Event`, the body flattened as the bridge writes it, holding a post with `cleaned_from`, a step with `lapses_at`
/// and a lapsed mark.
#[cfg(test)]
mod words_read_by_the_window {
    use super::*;
    use docket_machine::{Defs, Input, Machine, Text, ViewStatus};
    use serde_json::json;

    const CALLS: &str = "call look(text: Text) -> Go | AsTyped\n  task look\n  default Go\n";
    const KINDS: &str = "kind words\n  form text: Text\n  result Send(text: Text) | Back(why: Text)\n  pattern\n    o <- step owner look(text);\n    match o {\n      Go => return Send(\"cleaned\")\n      AsTyped => return Send(text)\n    }\n";

    /// A ticket's view as the machine makes it, for the other tests that read a view back.
    pub(super) fn a_ticket() -> docket_machine::TicketView {
        let mut named = Default::default();
        let calls = docket_machine::syntax::parse_calls("words.call", CALLS, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
        let (defs, errors) = Defs::build(&[("words.kind".to_string(), KINDS.to_string())], calls, named, 200);
        assert!(errors.is_empty(), "{errors:?}");
        let mut m = Machine::load(defs);
        m.apply(Input::Open { kind: "words".into(), under: None, opener: "owner".into(), form: json!({"text": "teh words"}), text: None, names: vec![], told: vec![] }, "2026-11-18T16:00:01Z".into()).reply.unwrap();
        m.view(200).unwrap()
    }

    /// A giving's `unread` mark (docket-edge4's "handed twice · not read", consult-lead N-412.p1900, 2) and its `owed`
    /// mark (his post left unanswered in two turns, `delivered · not answered`, paper-research N-412.p2197, 1) come
    /// through a ticket's event as sent, are left out when false, and a row from an older coordd, which has neither,
    /// reads as read and not owed.
    #[test]
    fn a_givings_unread_and_owed_marks_decode_through_an_event_and_are_left_out_when_false() {
        use docket_machine::view::ViewGiven;
        let mut v = a_ticket();
        let row = |unread: bool, at: &str| ViewGiven { to: "orch".into(), delivery: "N-200.p1".into(), upto: 1, at: at.into(), place: None, unread, delivered: None, owed: false, skipped: Default::default() };
        v.given = vec![row(true, "2026-11-20T06:00:00Z"), row(true, "2026-11-20T06:02:00Z"), row(false, "2026-11-20T06:04:00Z"), ViewGiven { owed: true, ..row(false, "2026-11-20T06:06:00Z") }];
        // a row whose mark passed his post held under his question ((c2), `GivenRec::skipped`): the set goes as sent
        v.given[2].skipped.insert(1);
        for body in [Body::Ticket(v.clone()), Body::Snapshot(Snapshot { tickets: vec![v.clone()], ..Default::default() })] {
            let ev = Event { seq: 11, body };
            let line = serde_json::to_string(&ev).unwrap();
            assert_eq!(serde_json::from_str::<Event>(&line).unwrap_or_else(|e| panic!("the window cannot read it: {e}\n{line}")), ev);
            assert_eq!(line.matches("\"unread\":true").count(), 2, "{line}");
            assert!(!line.contains("\"unread\":false"), "left out when false: {line}");
            assert_eq!(line.matches("\"skipped\":[1]").count(), 1, "left out when none: {line}");
        }
        // the rows alone: the ticket's own `owed` (it owes the owner) is another field
        let rows = serde_json::to_string(&v.given).unwrap();
        assert_eq!(rows.matches("\"owed\":true").count(), 1, "{rows}");
        assert!(!rows.contains("\"owed\":false"), "left out when false: {rows}");
        let old: ViewGiven = serde_json::from_value(json!({"to": "orch", "delivery": "N-200.p1", "upto": 1, "at": "2026-11-20T06:00:00Z"})).unwrap();
        assert!(!old.unread && !old.owed, "an older coordd's row reads as read, not owed");
        assert!(old.skipped.is_empty() && old.hands(1), "and hands what its upto reaches");
    }

    /// When a giving's text entered the agent's model input (0.2 A1, B7's `delivered 12:28`, N-818 post 72): on the
    /// given row and on each `Where`, through a ticket's event and a snapshot as sent, left out when none, and none
    /// from an older coordd that sends no key.
    #[test]
    fn a_givings_delivered_time_decodes_through_an_event_on_its_row_and_its_where_and_is_left_out_when_none() {
        use docket_machine::view::ViewGiven;
        use docket_machine::{PostWhere, Where};
        let mut v = a_ticket();
        let row = |delivered: Option<&str>| ViewGiven { to: "orch".into(), delivery: "N-200.p1".into(), upto: 1, at: "2026-11-20T06:00:00Z".into(), place: None, unread: false, delivered: delivered.map(String::from), owed: false, skipped: Default::default() };
        v.given = vec![row(Some("2026-11-20T06:01:43Z")), row(None)];
        let at = |delivered: Option<&str>| Where { agent: "orch".into(), waiting: None, handed: Some("2026-11-20T06:00:00Z".into()), delivered: delivered.map(String::from), read: None, step: None, answered: None };
        v.whereabouts = vec![PostWhere { n: 1, agents: vec![at(Some("2026-11-20T06:01:43Z")), at(None)] }];
        for body in [Body::Ticket(v.clone()), Body::Snapshot(Snapshot { tickets: vec![v.clone()], ..Default::default() })] {
            let ev = Event { seq: 14, body };
            let line = serde_json::to_string(&ev).unwrap();
            assert_eq!(serde_json::from_str::<Event>(&line).unwrap_or_else(|e| panic!("the window cannot read it: {e}\n{line}")), ev);
            assert_eq!(line.matches("\"delivered\":\"2026-11-20T06:01:43Z\"").count(), 2, "{line}");
            assert!(!line.contains("\"delivered\":null"), "left out when none: {line}");
        }
        let old: ViewGiven = serde_json::from_value(json!({"to": "orch", "delivery": "N-200.p1", "upto": 1, "at": "2026-11-20T06:00:00Z"})).unwrap();
        assert_eq!(old.delivered, None, "an older coordd's row: not delivered, so `sent`");
        let old: Where = serde_json::from_value(json!({"agent": "orch", "handed": "2026-11-20T06:00:00Z"})).unwrap();
        assert_eq!(old.delivered, None);
    }

    /// His posts' asks (`TicketView::asked`, `ASKED_PROTOCOL`; I-1455 (B)): each agent asked with its answer, through a
    /// ticket's event, its change and a snapshot as sent; an open one has no `answer` key, and a view with none has no
    /// `asked` key, as from a coordd before 39, so an older window reads it as it did.
    #[test]
    fn a_views_asks_decode_through_an_event_and_none_is_no_key() {
        use docket_machine::ViewAsked;
        let mut v = a_ticket();
        assert!(!serde_json::to_string(&v).unwrap().contains("\"asked\""), "none is no key");
        v.asked = vec![ViewAsked { post: 1, agent: "w2".into(), answer: Some("N-200.p3".into()) }, ViewAsked { post: 1, agent: "w3".into(), answer: None }];
        v.unanswered_owner = vec![1];
        for body in [Body::Ticket(v.clone()), Body::TicketChange { view: v.clone(), posts: 3 }, Body::Snapshot(Snapshot { tickets: vec![v.clone()], ..Default::default() })] {
            let ev = Event { seq: 15, body };
            let line = serde_json::to_string(&ev).unwrap();
            assert_eq!(serde_json::from_str::<Event>(&line).unwrap_or_else(|e| panic!("the window cannot read it: {e}\n{line}")), ev);
            assert_eq!(line.matches("\"answer\":\"N-200.p3\"").count(), 1, "{line}");
            assert!(!line.contains("\"answer\":null"), "an open ask has no answer key: {line}");
        }
        let old: ViewAsked = serde_json::from_value(json!({"post": 1, "agent": "w3"})).unwrap();
        assert_eq!(old.answer, None);
    }

    #[test]
    fn a_snapshot_with_a_lapsed_mark_a_lapse_time_and_a_cleaned_post_reads_back_whole() {
        let mut named = Default::default();
        let calls = docket_machine::syntax::parse_calls("words.call", CALLS, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
        let (defs, errors) = Defs::build(&[("words.kind".to_string(), KINDS.to_string())], calls, named, 200);
        assert!(errors.is_empty(), "{errors:?}");
        let mut m = Machine::load(defs);
        let at = |s: u32| format!("2026-11-18T16:00:{s:02}Z");
        let open = |m: &mut Machine, s| m.apply(Input::Open { kind: "words".into(), under: None, opener: "owner".into(), form: json!({"text": "teh words"}), text: None, names: vec![], told: vec![] }, at(s)).reply.unwrap();
        open(&mut m, 1);
        open(&mut m, 2);
        // 200 went on by itself; his post of its cleaned words names it
        m.apply(Input::Lapsed { t: 200, step: 1, head: json!({"head": "Go"}), after: 30, because: "closed".into() }, at(31)).reply.unwrap();
        m.apply(Input::Post { t: 200, by: "owner".into(), text: Text::New("cleaned".into()), names: vec!["dev".into()], told: vec![], inform: vec![], answers: None, kind: None }, at(32)).reply.unwrap();
        let n = m.tickets[&200].last();
        m.apply(Input::Cleaned { t: 200, n, words: 200 }, at(33)).reply.unwrap();
        let done = m.view(200).unwrap();
        assert!(matches!(done.status, ViewStatus::Done { .. }));
        assert_eq!(done.lapsed.len(), 1);
        assert!(done.posts.iter().any(|p| p.cleaned_from.is_some()));
        // 201 still waits on him, and goes on at a time coordd sets on its view (lapse.rs `lapses_at`)
        let mut waiting = m.view(201).unwrap();
        waiting.lapses_at = Some("2026-11-18T16:00:32Z".into());
        let snap = Event { seq: 9, body: Body::Snapshot(Snapshot { tickets: vec![done.clone(), waiting.clone()], ..Default::default() }) };
        let line = serde_json::to_string(&snap).unwrap();
        let back: Event = serde_json::from_str(&line).unwrap_or_else(|e| panic!("the window cannot read the snapshot: {e}\n{line}"));
        assert_eq!(back, snap);
        let Body::Snapshot(s) = &back.body else { panic!() };
        assert_eq!((s.tickets[0].lapsed[0].mark(), s.tickets[1].lapses_at.as_deref()), ("went on by itself: his window was closed".to_string(), Some("2026-11-18T16:00:32Z")));
        assert_eq!(s.tickets[0].posts.iter().find(|p| p.n == n).and_then(|p| p.cleaned_from.clone()), Some(m.id(200)));
        // and as a ticket's own event
        for body in [Body::Ticket(done.clone()), Body::TicketChange { view: done, posts: n }] {
            let ev = Event { seq: 10, body };
            let line = serde_json::to_string(&ev).unwrap();
            assert_eq!(serde_json::from_str::<Event>(&line).unwrap_or_else(|e| panic!("{e}\n{line}")), ev);
        }
    }

    /// The owner model on the wire (proto-reader N-1446 p1212, B): a snapshot with none leaves the key out, and one
    /// from a coordd before it reads as none; a snapshot with it, and each `Body::OwnerModel`, a node or none, read back
    /// whole through `Event`, the window's reader. The change goes under `ownermodel`, the key coordd's bridge keeps from
    /// an app before `OWNER_MODEL_PROTOCOL` (client.rs `for_app`).
    #[test]
    fn the_owner_model_reads_back_through_an_event() {
        let none = serde_json::to_value(Snapshot::default()).unwrap();
        assert!(none.get("owner_model").is_none(), "{none}");
        let old: Snapshot = serde_json::from_value(json!({"project": null, "agents": [], "mail": [], "views": []})).unwrap();
        assert_eq!(old.owner_model, None);
        let made = ModelMade { update: "O-6".into(), upto: "N-2.p1".into(), at: "2026-11-21T21:24:00Z".into(), first_line: "He wants one box, not two.".into() };
        let view = OwnerModelView { node: "O-4".into(), running: true, model: Some(made), failed: Some(UpdateEnded { update: "O-7".into(), at: "2026-11-21T21:30:00Z".into() }) };
        let start = OwnerModelView { node: "O-4".into(), ..Default::default() };
        let start_v = serde_json::to_value(&start).unwrap();
        assert!(start_v.get("model").is_none() && start_v.get("failed").is_none(), "{start_v}");
        for body in [Body::Snapshot(Snapshot { owner_model: Some(view.clone()), ..Default::default() }), Body::OwnerModel(Some(view)), Body::OwnerModel(Some(start)), Body::OwnerModel(None)] {
            let ev = Event { seq: 13, body };
            let line = serde_json::to_string(&ev).unwrap();
            if !matches!(ev.body, Body::Snapshot(_)) {
                assert!(serde_json::from_str::<Value>(&line).unwrap().get("ownermodel").is_some(), "{line}");
            }
            assert_eq!(serde_json::from_str::<Event>(&line).unwrap_or_else(|e| panic!("the window cannot read it: {e}\n{line}")), ev);
        }
    }

    /// The clocks (0.2 line 26): a snapshot from a coordd before them reads with none; one with a clock, a `when`
    /// binding and a ticket naming the binding that opened it reads back whole, what is not set left out.
    #[test]
    fn a_snapshot_without_clocks_reads_as_none_and_one_with_them_reads_back_whole() {
        let old: Snapshot = serde_json::from_value(json!({"project": null, "agents": [], "mail": [], "views": []})).unwrap();
        assert!(old.clocks.is_empty());
        let clock = ClockView { binding: "every 15m open checkup(\"x\")".into(), kind: "checkup".into(), every: Some(900), when: None, by: Some("paper-research".into()), file: "/p/kinds/agents/paper-research.kind".into(), line: 3, last: Some("2026-11-20T13:00:00Z".into()) };
        let when = ClockView { binding: "when cut-build is build open batch(\"line\")".into(), kind: "batch".into(), every: None, when: Some(("cut-build".into(), "build".into())), by: None, file: "/p/kinds/cut.kind".into(), line: 12, last: None };
        let w = serde_json::to_value(&when).unwrap();
        assert!(["every", "by", "last"].iter().all(|f| w.get(f).is_none()) && w["when"] == json!(["cut-build", "build"]), "{w}");
        let mut t = a_ticket();
        t.binding = Some(clock.binding.clone());
        let snap = Event { seq: 9, body: Body::Snapshot(Snapshot { tickets: vec![t], clocks: vec![clock, when], ..Default::default() }) };
        let line = serde_json::to_string(&snap).unwrap();
        assert_eq!(serde_json::from_str::<Event>(&line).unwrap_or_else(|e| panic!("the window cannot read the snapshot: {e}\n{line}")), snap);
    }
}

/// The mentions as the window reads them (docket-core3, N-458 p10; N-542's rule): `Body::Mentioned` with a line that
/// names a post and one that names the ticket, and a ticket's view carrying both `mentioned` and `mentions`, each
/// written as coordd sends it and read back through `Event`, the window's own reader.
#[cfg(test)]
mod mentions_read_by_the_window {
    use super::*;
    use docket_machine::ViewMention;

    #[test]
    fn a_mention_line_and_a_view_with_mentions_read_back_through_an_event() {
        let line = |post: u32, of: Option<u32>| ViewMention { by: "N-446".into(), post, label: "the label".into(), who: "paper-research".into(), at: "2026-11-18T16:00:01Z".into(), of };
        let lines = vec![line(2, Some(3)), line(5, None)];
        let mut words = words_read_by_the_window::a_ticket();
        words.mentioned = lines.clone();
        words.mentions = [(1, vec!["N-412".to_string(), "N-412.p30".into()]), (12, vec!["@docket-core".into()])].into_iter().collect();
        for body in [Body::Mentioned { id: "N-481".into(), lines }, Body::Ticket(words.clone()), Body::Snapshot(Snapshot { tickets: vec![words], ..Default::default() })] {
            let ev = Event { seq: 11, body };
            let text = serde_json::to_string(&ev).unwrap();
            assert_eq!(serde_json::from_str::<Event>(&text).unwrap_or_else(|e| panic!("the window cannot read it: {e}\n{text}")), ev);
        }
    }

    /// A running tool call's `about` (paper-research N-516.p3) inside an agent's event and a snapshot, read back the
    /// same way (docket-core3, N-458 p11, p14); a call without one leaves the key out.
    #[test]
    fn a_running_tools_about_reads_back_through_an_event() {
        let mut a: AgentView = serde_json::from_value(serde_json::json!({"id": "", "name": "w", "parent": "", "kind": "claude", "model": "",
            "state": "working", "since": "", "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap();
        let tool = |id: &str, about: Option<&str>| Tool { id: id.into(), tool: "Bash".into(), what: "cargo test".into(), at: "2026-11-18T16:00:01Z".into(), about: about.map(String::from) };
        a.tools = vec![tool("t1", Some("Run the coord-types tests")), tool("t2", None)];
        for body in [Body::Agent(a.clone()), Body::Snapshot(Snapshot { agents: vec![a], ..Default::default() })] {
            let ev = Event { seq: 12, body };
            let text = serde_json::to_string(&ev).unwrap();
            assert_eq!(text.matches("\"about\"").count(), 1, "left out when none: {text}");
            assert_eq!(serde_json::from_str::<Event>(&text).unwrap_or_else(|e| panic!("the window cannot read it: {e}\n{text}")), ev);
        }
    }
}

/// Where his posts are as the window reads them (part 4, N-818 post 32; docket-core3's two conditions, post 33): a
/// ticket's view carrying `whereabouts` written as coordd sends it and read back whole through `Event`, the window's own
/// reader; and the same view read by a window that does not know the field (an older one), which leaves it out.
#[cfg(test)]
mod whereabouts_read_by_the_window {
    use super::*;

    fn a_ticket_with_whereabouts() -> TicketView {
        let mut v = words_read_by_the_window::a_ticket();
        let on = |agent: &str, ahead| Where { agent: agent.into(), waiting: Some(Waiting { ahead }), handed: None, delivered: None, read: None, step: None, answered: None };
        let handed = Where {
            agent: "paper-research".into(),
            waiting: None,
            handed: Some("2026-11-19T20:01:00Z".into()),
            delivered: Some("2026-11-19T20:02:41Z".into()),
            read: Some("2026-11-19T20:03:10Z".into()),
            step: Some(WhereStep { on: "C-812".into(), step: 2, call: "plan".into(), since: "2026-11-19T20:03:11Z".into() }),
            answered: Some(WhereAnswer { post: "N-1104.p15".into(), at: "2026-11-19T20:04:00Z".into() }),
        };
        v.whereabouts = vec![PostWhere { n: 1, agents: vec![handed, on("consult-lead", 2)] }, PostWhere { n: 7, agents: vec![on("paper-research", 0)] }];
        v
    }

    /// FROM renamed TO wherever it is a key; how many times.
    pub(super) fn rename(v: &mut serde_json::Value, from: &str, to: &str) -> usize {
        match v {
            serde_json::Value::Object(o) => {
                let here = o.remove(from).map(|x| o.insert(to.into(), x)).is_some() as usize;
                here + o.iter_mut().filter(|(k, _)| k.as_str() != to).map(|(_, x)| rename(x, from, to)).sum::<usize>()
            }
            serde_json::Value::Array(a) => a.iter_mut().map(|x| rename(x, from, to)).sum(),
            _ => 0,
        }
    }

    #[test]
    fn a_view_with_whereabouts_reads_back_through_an_event() {
        let v = a_ticket_with_whereabouts();
        for body in [Body::Ticket(v.clone()), Body::TicketChange { view: v.clone(), posts: 7 }, Body::Snapshot(Snapshot { tickets: vec![v], ..Default::default() })] {
            let ev = Event { seq: 12, body };
            let text = serde_json::to_string(&ev).unwrap();
            assert_eq!(serde_json::from_str::<Event>(&text).unwrap_or_else(|e| panic!("the window cannot read it: {e}\n{text}")), ev);
        }
        // none is no key, as from a coordd before WHEREABOUTS_PROTOCOL
        assert!(!serde_json::to_string(&words_read_by_the_window::a_ticket()).unwrap().contains("whereabouts"));
    }

    /// The field under a name this window does not know stands for a window older than it: the event still reads, and
    /// the view is whole but for the field.
    #[test]
    fn a_window_that_does_not_know_the_field_reads_the_rest() {
        let v = a_ticket_with_whereabouts();
        let mut want = v.clone();
        want.whereabouts = Vec::new();
        for body in [Body::Ticket(v.clone()), Body::Snapshot(Snapshot { tickets: vec![v.clone()], ..Default::default() })] {
            let mut j = serde_json::to_value(Event { seq: 13, body }).unwrap();
            assert_eq!(rename(&mut j, "whereabouts", "whereabouts_unknown_here"), 1);
            let text = j.to_string();
            let back: Event = serde_json::from_str(&text).unwrap_or_else(|e| panic!("an older window cannot read it: {e}\n{text}"));
            match back.body {
                Body::Ticket(t) => assert_eq!(t, want),
                Body::Snapshot(s) => assert_eq!(s.tickets, vec![want.clone()]),
                b => panic!("{b:?}"),
            }
        }
    }
}

#[cfg(test)]
mod machines_tests {
    use super::*;

    /// The Machines page's request (N-1136 post 6) goes as `{"op": "machines"}` and reads back, at a number its own.
    #[test]
    fn the_machines_request_reads_back_at_its_own_number() {
        assert_eq!(serde_json::to_value(&Request::Machines).unwrap(), serde_json::json!({"op": "machines"}));
        assert!(matches!(serde_json::from_str::<Request>(r#"{"op":"machines"}"#).unwrap(), Request::Machines));
        assert!(MACHINES_PROTOCOL > RULES_PROTOCOL && MACHINES_PROTOCOL <= PROTOCOL);
    }

    /// Home's Store (a2h-speed N-1446 p2815) asks `{"op": "store"}` for the rows, with `row` and `from` for a page of a
    /// row's entries, and reads back; at a number after the devices'.
    #[test]
    fn the_store_request_reads_back_with_and_without_a_row() {
        assert_eq!(serde_json::to_value(&Request::Store { row: None, from: None }).unwrap(), serde_json::json!({"op": "store"}));
        let page = Request::Store { row: Some("crates".into()), from: Some(200) };
        assert_eq!(serde_json::to_value(&page).unwrap(), serde_json::json!({"op": "store", "row": "crates", "from": 200}));
        assert!(matches!(serde_json::from_str::<Request>(r#"{"op":"store","row":"crates","from":200}"#).unwrap(),
            Request::Store { row: Some(r), from: Some(200) } if r == "crates"));
        assert!(matches!(serde_json::from_str::<Request>(r#"{"op":"store"}"#).unwrap(), Request::Store { row: None, from: None }));
        assert!(STORE_PROTOCOL > MACHINES_PROTOCOL && STORE_PROTOCOL <= PROTOCOL);
    }

    /// Home's press on a finding of the walk asks `{"op": "storefree", "path"}`; at a number after the viewer's.
    #[test]
    fn the_store_free_request_reads_back() {
        let q = Request::StoreFree { path: "/w/target".into() };
        assert_eq!(serde_json::to_value(&q).unwrap(), serde_json::json!({"op": "storefree", "path": "/w/target"}));
        assert!(matches!(serde_json::from_str::<Request>(r#"{"op":"storefree","path":"/w/target"}"#).unwrap(), Request::StoreFree { path } if path == "/w/target"));
        assert!(STORE_FREE_PROTOCOL > REPO_VIEW_PROTOCOL);
        assert!(STORE_FREE_PROTOCOL < PROTOCOL);
    }

    /// Home's cold storage asks `{"op": "cold"}`, and its acts with their fields; at a number after Home's.
    #[test]
    fn the_cold_request_reads_back() {
        let read = Request::Cold { act: None, path: None, rule: None, on: None };
        assert_eq!(serde_json::to_value(&read).unwrap(), serde_json::json!({"op": "cold"}));
        let auto = Request::Cold { act: Some("auto".into()), path: None, rule: Some(1), on: Some(true) };
        assert_eq!(serde_json::to_value(&auto).unwrap(), serde_json::json!({"op": "cold", "act": "auto", "rule": 1, "on": true}));
        assert!(matches!(serde_json::from_str::<Request>(r#"{"op":"cold","act":"restore","path":"/w/wt"}"#).unwrap(), Request::Cold { act: Some(a), path: Some(p), .. } if a == "restore" && p == "/w/wt"));
        assert!(COLD_PROTOCOL > HOME_PROTOCOL && COLD_PROTOCOL <= PROTOCOL);
    }

    /// The PDF page, the settings and the repositories' state; at the number after the store's delete.
    #[test]
    fn the_pdf_page_settings_and_repo_state_requests_read_back() {
        let q = Request::PdfPage { path: "/w/paper/main.pdf".into(), page: 2, width: None };
        assert_eq!(serde_json::to_value(&q).unwrap(), serde_json::json!({"op": "pdfpage", "path": "/w/paper/main.pdf", "page": 2}));
        let m = Request::Settings { scope: Some("project".into()), key: Some("agent_mode".into()), value: Some("yolo".into()) };
        assert_eq!(serde_json::to_value(&m).unwrap(), serde_json::json!({"op": "settings", "scope": "project", "key": "agent_mode", "value": "yolo"}));
        assert!(matches!(serde_json::from_str::<Request>(r#"{"op":"settings"}"#).unwrap(), Request::Settings { scope: None, key: None, value: None }));
        assert_eq!(serde_json::to_value(Request::RepoState).unwrap(), serde_json::json!({"op": "repostate"}));
        assert!(HOME_PROTOCOL < PROTOCOL);
    }

    /// A rewind's fork names its time, a resolve its comment, a post its comments; an older line has none of them.
    #[test]
    fn a_rewind_a_resolve_and_a_posts_comments_read_back() {
        let f = Request::ForkAgent { from: "lead".into(), new: None, model: None, dir: None, text: None, at: Some(1_764_000_000) };
        assert_eq!(serde_json::to_value(&f).unwrap()["at"], 1_764_000_000);
        assert!(matches!(serde_json::from_str::<Request>(&f.line()), Ok(Request::ForkAgent { at: Some(1_764_000_000), .. })));
        let r = Request::CommentResolve { t: "N-1".into(), n: 2, id: "c1".into(), resolved: true };
        assert!(matches!(serde_json::from_str::<Request>(&r.line()), Ok(Request::CommentResolve { n: 2, resolved: true, .. })));
        let c = Comment { target: Anchor { quote: "the lock".into(), ..Default::default() }, body: "whose?".into(), ..Default::default() };
        let p = Request::Post { t: "N-1".into(), text: "and".into(), to: None, answers: None, wait: false, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), comments: vec![c.clone()], kind: None };
        assert!(matches!(serde_json::from_str::<Request>(&p.line()), Ok(Request::Post { comments, .. }) if comments == [c]));
        assert!(matches!(serde_json::from_str::<Request>(r#"{"op":"post","t":"N-1","text":"x"}"#), Ok(Request::Post { comments, .. }) if comments.is_empty()));
    }
}
