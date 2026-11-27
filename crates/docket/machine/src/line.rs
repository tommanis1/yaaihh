//! The log's lines, the machine's effects and deliveries.
//!
//! A line is either an **input** (something from outside: a request, a turn's end, a policy answer, a delivery
//! taken), written before it is applied, or an **output** (what the machine did with it: a step owed, a child
//! opened, a ticket closed). Replay applies the inputs again and compares the outputs it makes with the recorded
//! ones: the self-check of "the log is the state". No line copies a text another line or a transcript already holds:
//! texts are `Text::Loc`, and output lines name what they derive (a step's number, a post's number), not its content.

use crate::check::CallSig;
use crate::types::{Loc, Text, Type};
use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A time, given by the caller (the machine reads no clock): UTC, as coordd writes it.
pub type At = String;

/// What an agent's turn did, as its transcript shows it (S6, insight (1): his p640 "track how many turs go into wokr",
/// p641 "our unit is work x model"): TOOLS, its own tool calls by tool name, a subagent's not among them; MODEL, the
/// model that answered its last message. Read by coordd's follower as the turn runs and written on the turn's
/// `turn_ended` line ([`Line::TurnEnded`]); the machine is told it for the turn ending ([`crate::Machine::turn_worked`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Worked {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tools: BTreeMap<String, u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "ev", rename_all = "snake_case")]
pub enum Line {
    // ---- inputs ----
    /// A ticket opened by a request (`coord open`): its number, its program as the kind file had it, its form, and
    /// the opener's first post if any.
    Open {
        at: At,
        t: u64,
        kind: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        under: Option<u64>,
        opener: String,
        form: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<Text>,
        /// The agents the first post names, and those policy's `notify` adds (asked before, off the lock).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        names: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        told: Vec<String>,
        /// Old lines only: those named whose running turn was stopped for it (the `--esc` that `coord esc` replaced,
        /// N-36); read, never written.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        esc: Vec<String>,
        program: String,
        /// The calls PROGRAM names, as they were when it was written (calls on the record, paper-research N-101 p392;
        /// the owner, N-221.p3): a replay checks and runs PROGRAM against these, not today's. None on older lines:
        /// today's calls then, as before.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        calls: BTreeMap<String, CallSig>,
        /// An imported ticket (today's items) that was closed already: it closes so at once, its program not run,
        /// nobody told.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        closed: Option<Closing>,
        /// A question an agent opened while it held a step (`coord ask`): the ticket of that step. The question is the
        /// step's ask: the step waits on it, and is given again when it closes (paper-research N-101 p524, 2). None on
        /// older lines: a question then stood apart from the step, as before.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        asked_on: Option<u64>,
    },
    /// A post on a ticket (`coord post`); NAMES are the agents it names, TOLD those policy's `notify` adds.
    Post {
        at: At,
        t: u64,
        by: String,
        text: Text,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        names: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        told: Vec<String>,
        /// Those policy's `inform` tells: the post is due at their next turn, which it does not start (m1237).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        inform: Vec<String>,
        /// Old lines only: those named whose running turn was stopped for it (the `--esc` that `coord esc` replaced,
        /// N-36); read, never written.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        esc: Vec<String>,
        /// The owner's answers to the asks of post N of the ticket (the card filled in, design-two-systems §16, 1).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        answers: Option<u32>,
        /// It passes the owner's `@all`, post N of the ticket, on to the agent it names (`Input::Pass`, `coord pass`;
        /// consult-lead N-412 post 1900): an older engine reads it as the post it is, naming that agent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passes: Option<u32>,
        /// The owner's typed post, held by the policy for these seconds before any agent is given it
        /// (`Machine::owner_hold`, `lapse-after("gather", N)`; the owner, N-1445 p9): base.kind's `gather` waits
        /// on it, and his next posts on the ticket join it meanwhile (E1b). None: not held, as every post before it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hold: Option<u64>,
        /// What the post is, as its writer typed it (`PostKind`, I-1480): whom it pushes to and who reads it
        /// later. None: untyped, as every post before it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<PostKind>,
        /// Of NAMES, those the rule for a post naming nobody gave it (`Machine::unanswered`), or `kept_reply` the turn's
        /// end's Reply's, that its writer did not type: it answers them and asks them nothing, so it is pulled by them
        /// (paper-research N-1481 p40, p43: an `@` the writer typed always asks). Empty, and not written: every name
        /// typed, as every post before it.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        answering: Vec<String>,
    },
    /// An explicit answer to the step a ticket awaits (`coord answer`, a program agent's verdict).
    Answer { at: At, t: u64, by: String, head: Value, text: Text },
    /// AGENT's turn was cut short (its process exited, or its session started again) with deliveries given and no
    /// turn end: what it was given is forgotten, so its next turn counts afresh (the rule Reply).
    TurnCut { at: At, agent: String },
    /// AGENT was given deliveries and never began a turn on them (idle, no turn running, for a while: coordd's
    /// reconcile, paper-research m1005): each is made due again, once; one given twice already gets a post on its
    /// ticket instead ('given twice, not read'), and a step there is left to Silent.
    Unread { at: At, agent: String },
    /// An agent's turn ended; TEXT is where its final output is in its transcript, DELIVERY the ticket delivery the
    /// turn began with, if any. LATER: deliveries given since its last turn ended that it did not read, handed after
    /// TEXT was written (N-17 post 10): still counted for its next turn.
    TurnEnded {
        at: At,
        agent: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delivery: Option<String>,
        text: Loc,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        later: Vec<String>,
        /// Deliveries of posts that only informed it, beside one that told it something: they do not make the turn
        /// "several", and the rule Reply reads past them (N-27 point 3c).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        informs: Vec<String>,
        /// Where the agent stood in the turn (coordd's moves, N-101 p414): a layer's id, the line's view (`line@...`),
        /// or empty for the project's own folder. The turn's end answers only a step in that layer or in none; another
        /// layer's step is answered by `coord answer`. None (lines from before): any step, as before.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stood: Option<String>,
        /// The turn asked to go to layer GO (`coord go`): it answers no step, and its last message is a post on the
        /// ticket whose step it would have answered.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        go: Option<String>,
        /// What the turn did, as its transcript showed coordd's follower ([`Worked`]): its own tool calls by tool name,
        /// and the model that answered it. Empty and none on lines from before, for a turn begun before coordd's
        /// follower began, and for an end taken with no stop in the transcript; never guessed (insight (1)).
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        tools: BTreeMap<String, u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    /// The head formed from a turn's output (none: it could not be formed, or did not type). WHY, with none: the
    /// forming call's one sentence on why the answer gives none of the step's outcomes, which the reminder tells the
    /// agent (paper-research N-101 p561); NOT_TAKEN: coordd's own reason, for the record only (a reading that did not
    /// fit, parts that could not be cut, an empty answer). CALL: where coordd keeps the forming calls' prompts and replies, under the coord's folder
    /// (paper-research N-101 p554, 1). Both absent on older lines.
    Formed {
        at: At,
        t: u64,
        step: u32,
        head: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        why: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        not_taken: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        call: Option<String>,
    },
    /// A session took a delivery: posts up to UPTO; WHERE, its place in the recipient's transcript.
    Given {
        at: At,
        t: u64,
        to: String,
        delivery: String,
        upto: u32,
        #[serde(default, rename = "where", skip_serializing_if = "Option::is_none")]
        place: Option<Loc>,
        /// Put into a turn already running (the owner's post, N-101 p1214): it does not count for that turn's rule
        /// Reply (`Machine::turn_given`). From the era `ERA_PUT_IN`, in its input before the turn's last message, it
        /// is read at the turn's end, answered or not, and only one still queued then is the next turn's
        /// (paper-research N-1397.p91); before it, unanswered on its thread, it was the next turn's.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        into: bool,
        /// His posts once asked, or told to TO late, up to UPTO that the hand-over left out (`GivenRec::skipped`), as
        /// coordd worked them out when it handed the delivery over (`Machine::left_out`), so a press of his before the
        /// session took it, a Send or a take-back, changes nothing (proto-reader N-1446 p621, proto-lead p631; consult-3
        /// p661), with those told to TO late since that it did not show (`Machine::left_out_given`). Written when a post
        /// of his up to UPTO had been asked, or one told to TO late is not given yet (`Ticket::may_leave_out`; consult-3
        /// N-1479 p262, proto-reader p264), `[]` when the hand-over left none of them out. None when
        /// coordd did not hand it (a first read after a start, words typed into a terminal) or nothing up to UPTO could
        /// be left out: then worked out as the line is read (`Ticket::skipped_now`), as on a record from before.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        skipped: Option<std::collections::BTreeSet<u32>>,
    },
    /// A policy answer the machine asked for. For `fills`, ANSWER is the agent chosen or null.
    Decided { at: At, t: u64, step: u32, question: String, answer: Value, provenance: Value },
    /// A text moved to its one copy, its transcript being removed.
    Pinned { at: At, loc: Loc, to: Loc },
    /// Policy says the step has waited too long (`expired`): it is withdrawn and fails as Silent.
    Expired { at: At, t: u64, step: u32, provenance: Value },
    /// The owner's step S of T answered by itself (N-487): its call's `default` HEAD, taken as his answer with no
    /// text, AFTER seconds after it was owed, BECAUSE `time` (the policy's `lapse-after` passed) or `closed` (the box
    /// that showed it is gone). Taken only while T still awaits S of the owner, not held, not slowed, and HEAD is the
    /// call's default; else refused as a stale input (he answered first).
    Lapsed { at: At, t: u64, step: u32, head: Value, after: u64, because: String },
    /// Policy's `notify` for EVENT on T (asked off the lock): the agents TO are told too.
    /// INFORM: whom policy's `inform` names (and `notify` does not): told, without that starting a turn.
    /// POST: the post it was asked for (event `post`, an agent's turn reply by the rule Reply): those told and informed
    /// are that post's (paper-research m1240).
    Notified { at: At, t: u64, event: String, to: Vec<String>, #[serde(default, skip_serializing_if = "Vec::is_empty")] inform: Vec<String>, #[serde(default, skip_serializing_if = "Option::is_none")] post: Option<u32>, provenance: Value },
    /// BY's act ACT (`open` of a KIND, `post` on T) went unchecked: the policy could not be read or solved (WHY), and,
    /// as for mail, it was let through. Written just before the act's own line; it changes nothing.
    Unchecked {
        at: At,
        act: String,
        by: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        t: Option<u64>,
        kind: String,
        why: String,
    },
    /// coordd could not render step STEP of T for its agent (WHY: its call is not loaded, or its template fails): the
    /// step fails at once with NoCall, its agent never told.
    Unrendered { at: At, t: u64, step: u32, why: String },
    /// Event NAME with VALUE (`to_nets`): the bindings on it opened the tickets FIRED (Trigger).
    /// COUNTED: the `every N of` bindings that counted it, whether or not they fired.
    Event {
        at: At,
        name: String,
        value: Value,
        fired: Vec<Fired>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        counted: Vec<String>,
        /// The ticket whose step the event's sender holds and whose kind handles it (`when ev(x)`, plan/layers.md
        /// I §3): its handler runs there and no binding is asked.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on: Option<u64>,
    },
    /// BY works on T (`coord item start`): the source of policy's `started(a, i)`.
    Started { at: At, t: u64, by: String },
    /// BY ties T under TO in the outline (`coord tie`, paper-research N-101 p1676) at TO's post N, the last it had, or
    /// unties it (TO none). For the outline only: what is below a ticket for a cancel, a close or the policy (`under`,
    /// `parent`) does not change.
    Tied {
        at: At,
        t: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<u64>,
        #[serde(default)]
        n: u32,
        by: String,
    },
    /// BY names T LABEL from now on (`coord title`): what it is listed as, and what its parent's line for it shows.
    Titled { at: At, t: u64, label: String, by: String },
    /// BY ended thread T (`coord end`, paper-research N-447): HOW is `done`, `dropped` or `kept`, WHY its one line.
    /// A thread resting at its hole has it filled with `return Done(why)`, `return Dropped(why)` or `return Kept`; an
    /// agents' thread (`Between`) keeps its result and is marked ended. Either leaves the outline.
    Ended { at: At, t: u64, by: String, how: String, why: String },
    /// Post N of T moved to ticket TO by BY (Move: the owner's act). NEW: the program of a note made for it, when it
    /// moves to a new thread (TO is then that note's number).
    Move {
        at: At,
        t: u64,
        n: u32,
        to: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        new: Option<String>,
        by: String,
    },
    /// The one-offs TS (threads that ended OneOff, to the same agent, one after another) made one thread by BY: the
    /// first of them, resting, holds the words of each and the agent's answers to them, in order.
    Keep { at: At, ts: Vec<u64>, by: String },
    /// The owner says post N of T asks nothing that needs his answer: its asks are no longer open (§16, 4).
    NothingToAnswer { at: At, t: u64, n: u32, by: String },
    /// BY withdraws the asks of post N of T, with WHY: the asker (or an agent above it, coordd's check) says they no
    /// longer need the owner's answer; nobody is told, and the owner sees the card closed with the why (paper-research
    /// N-461, N-447 p4/p5). The owner's own act for an ask is NothingToAnswer.
    AsksWithdrawn { at: At, t: u64, n: u32, by: String, why: String },
    /// BY takes back its post N of T before anyone was handed it (paper-research N-447 p16, 2; the form p30): it keeps
    /// its number and is drawn no more; a handler holding it that had handed nothing ends with it.
    TakenBack { at: At, t: u64, n: u32, by: String },
    /// BY's COMMENTS on passages, given with its post N of T, as data (`Post::comments`); the post's text words them.
    Comments { at: At, t: u64, n: u32, by: String, comments: Vec<crate::comment::Comment> },
    /// BY resolves comment ID of post N of T (RESOLVED), or reopens it.
    CommentResolved { at: At, t: u64, n: u32, id: String, resolved: bool, by: String },
    /// BY stars T, or its post POST (ON), or takes the star off: a mark that does nothing else; the owner's on a
    /// thread orders his list (the owner N-234.p3: agents mark too, and posts).
    Star { at: At, t: u64, on: bool, by: String, #[serde(default, skip_serializing_if = "Option::is_none")] post: Option<u32> },
    /// The owner archives T (ON), or unarchives it: T is no longer of importance to him (the owner, N-1073 post 1). It
    /// is not an end: nothing is closed or taken back. While T is archived nothing at or under it is handed to anyone
    /// and its hole is not put to the minder; what is held is kept, and is handed at Unarchive as it would have been
    /// (tickets.eflint `archived`; paper-research N-867.p21, docket-core7 N-867.p25). Only the owner's: one by anyone
    /// else is refused, on replay too (docket-core3 N-867.p27). Written live from this release; the release before
    /// reads it in a record it replays (two trains, N-807.p12).
    Archive { at: At, t: u64, on: bool, by: String },
    /// BY reopened item T, closed while its vault note still says it is open (the vault wins; orch m14290,
    /// paper-research m1518): its close is held by the lead again. Items only.
    Reopen { at: At, t: u64, by: String },
    /// BY undid T's last accepted answer at a hole (the minder's): what it started is taken back and T rests at the
    /// hole again, not asked again until something new happens on it.
    Undo { at: At, t: u64, by: String },
    /// BY asked TEXT (CONTEXT, what follows its first line) and waits for the answer in its turn (a blocking ask): the
    /// `ask` event on the ticket whose step BY holds, its kind's `when ask` handler run under it.
    Ask { at: At, by: String, text: String, #[serde(default, skip_serializing_if = "Option::is_none")] context: Option<String> },
    /// BY's waiting ask stopped waiting (its command returned "later"): its answer comes as a delivery.
    AskLater { at: At, by: String },
    /// BY stopped AGENT's running turn (`coord esc`, the page's esc button; N-36): the one way a turn is stopped
    /// besides the owner's Escape key in its terminal. Written only when a turn ran and Escape was typed. ALL: the
    /// project-wide act (`coord esc --all`, the window's Esc all; line 200, N-1324), one line for the press: AGENT and
    /// ALSO are every agent that was in a turn then, each stopped. The two fields are left out when empty, so a line
    /// of one esc is as before, and an engine from before them reads this one as AGENT's esc (it changes nothing on
    /// the record either way).
    Esc {
        at: At,
        by: String,
        agent: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        also: Vec<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        all: bool,
    },
    /// BY changed T (WHAT: `cancel`, `withdraw`), for WHY; PROGRAM, what a withdrawal puts where the step was.
    /// coordd asked `may edit` before.
    Edited {
        at: At,
        t: u64,
        by: String,
        why: String,
        what: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        program: Option<String>,
        /// For `rekind`: the new kind and its form (PROGRAM is its text as the kind file has it).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        form: Option<Value>,
        /// The calls PROGRAM names, as they were when it was written (calls on the record, paper-research N-101 p392;
        /// the owner, N-221.p3): a replay checks and runs PROGRAM against these, not today's. None on older lines:
        /// today's calls then, as before.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        calls: BTreeMap<String, CallSig>,
    },

    /// BY (the owner) switched T to slow (ON) or back: while it or a ticket above it is slow, each of five moments on
    /// it halts before its effect goes out, until the owner approves it (`Halted`; plan/layers.md, Part I §5, 3).
    Slow { at: At, t: u64, on: bool, by: String },
    /// BY set slow over SCOPE (ON) or lifted it, as the policy's act `set-slow` or `lift-slow` let BY (coordd asks it;
    /// the owner is not asked): while it holds, every moment of a ticket SCOPE covers halts as on a slow ticket
    /// (`Machine::slow`). The scope known so far: "project", every ticket, those opened later too (N-412 ask 5). A
    /// ticket's own switch stays `Slow`.
    Slowed { at: At, scope: String, on: bool, by: String },
    /// BY let halt N of T through: what it held back happens now, as edited.
    Approve { at: At, t: u64, n: u32, by: String },
    /// BY changed what halt N of T will let through (VALUE, by the halt's kind: a give's `prompt` or `worker`, a take's
    /// `head`, a deliver's `wakes`, an open's `form`, a decide's `answer`). It stays halted.
    HaltEdit { at: At, t: u64, n: u32, by: String, value: Value },
    /// BY refused halt N of T (N-1788): what it held back is not done (`Machine::reject`), for REASON when given.
    HaltReject {
        at: At,
        t: u64,
        n: u32,
        by: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// BY (the owner) handed his words waiting for AGENT over at once, into its running turn too (N-1788,
    /// `Machine::urgent`): the items handed are on the `acted` line it writes.
    Urgent { at: At, agent: String, by: String },
    /// Layer LAYER of ticket T could not be made, for WHY (coordd: no `repo` or `root` set, the kernel refused the
    /// mount): what runs inside it is taken back and the layer's scope raises NoLayer(why).
    LayerFailed { at: At, t: u64, layer: String, why: String },
    /// coordd: scoped agent AGENT of T could not be started, for WHY (a fork, fresh, let agent or restart; N-101): the
    /// step owed to it raises NoWorker {agent, why}, now if one is owed, else when the next one is.
    StartFailed { at: At, t: u64, agent: String, why: String },
    /// coordd: the command that gives step STEP of T to the project's runner (`RUNNER step`, `RUNNER check`) exited
    /// refused, the runner's message WHY (N-159 post 43): that step raises RunnerRefused {call, why}, so it does not
    /// stand waiting in silence; the runner's later steps are given as ever.
    RunnerRefused { at: At, t: u64, step: u32, why: String },
    /// coordd: the answer to step STEP of T (given or formed) is refused for WHY, a thing only coordd can see: an
    /// Agent field names no registered agent (item 11, G5). Refused as an answer that does not fit is, and formed
    /// again from the agent's next.
    Unfit {
        at: At,
        t: u64,
        step: u32,
        why: String,
        /// Where the forming calls of the answer refused are kept (Formed's CALL), if it was formed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        call: Option<String>,
    },
    /// coordd: step STEP of T is not given to AGENT, whose model does not meet the step's call's `where`, as WHY says
    /// (the model and the condition; the model classes, paper-research N-101): the step raises NoWorker {agent, why}.
    ModelRefused { at: At, t: u64, step: u32, agent: String, why: String },
    /// coordd: AGENT was made by BY (`coord new`; `coord fork`, a copy of FROM) while BY owed a step on T (none: it owed
    /// none), its thread the agents made for it (item 11; paper-research N-101 p272). It changes nothing in the
    /// machine but its list of agents made (`Machine::made`), what the policy's `made` facts are drawn from.
    Made {
        at: At,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        t: Option<u64>,
        agent: String,
        by: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<String>,
    },
    /// coordd: NAMES are agents the registry has that this record did not make (a record set aside, the registry kept;
    /// docket-core3 N-101 p609): a step's or a scope's agent is never given one of them, it takes the next free name
    /// (paper-research p610). Written at start when the registry has such a name not yet on the record.
    Taken { at: At, names: Vec<String> },
    /// coordd: AGENT was moved from the view it stood in (FROM: a layer, or `line@SHA`, the line's read-only view; none:
    /// its own folder) to TO's, its session ended and resumed on the same conversation there (agents and layers,
    /// paper-research N-101 p405, C): for a step in TO (on T), at the end of FROM, or after coordd's restart (WHY).
    /// KILLED: what was left running in its leaf and ended; LEFT: the files it left uncommitted in FROM, which stay
    /// there. It changes nothing in the machine.
    Moved {
        at: At,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        t: Option<u64>,
        agent: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<String>,
        why: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        killed: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        left: Vec<String>,
    },
    /// AGENT's role is ROLE from now on (N-101 post 27, F): who is the runner, who is the lead, and the other roles the
    /// policy's norms name, as the record's own lines. coordd writes it at start for each role the record does not have
    /// as the registry has it, and on `coord role`. It changes nothing in the machine but its table of roles.
    /// ALSO: the roles the policy has AGENT hold besides ROLE (its own `+role(AGENT, R)` lines; coordd's
    /// `roles_held`), so that whoever the machine names for a role is whoever the policy says holds it; none on a record
    /// from before them.
    Role {
        at: At,
        agent: String,
        role: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        also: Vec<String>,
    },
    /// What AGENT did in its turn (N-101, docket-core5): IS what it is (`edit` from a changing tool, `submit` when it
    /// asked the runner's check, `check` when that check came back, or what the runner says), TEXT what it ran and
    /// what came back, MORE the act's other fields as given (`tool`, `symbol`, `files`, `check`, `result`...). T: the
    /// ticket AGENT owes a step on, where a `when act` or a `during ... when act` handler takes it; absent when it
    /// owes none. Written for every act; it changes nothing when no handler takes it.
    Act {
        at: At,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        t: Option<u64>,
        agent: String,
        is: String,
        #[serde(default)]
        text: String,
        #[serde(flatten)]
        more: serde_json::Map<String, Value>,
    },
    /// BY (the owner) edited the prompt of CALL's steps on T once (the app's step panel; docket-edge's proposal on N-101, 15 Nov;
    /// paper-research post 145, 8): TEXT, a template in the call's own slots, is rendered in place of the call's
    /// template for the next step of CALL on T (SCOPE `step`), or for every step of CALL on T and the tickets under it
    /// from now on (`tree`). The call's file is not changed; the step's delivery, written at its place, holds the prompt.
    Template { at: At, t: u64, call: String, scope: String, text: String, by: String, why: String },
    /// BY had coordd read the kinds and calls again, all checked together, after COMMIT (the project's files changed):
    /// KINDS and CALLS are those changed or new. Tickets opened from here run the kinds as they are now; a running ticket
    /// keeps the program on its open line. A record: a replay reads today's files from the start.
    Reload {
        at: At,
        by: String,
        why: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        kinds: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        calls: Vec<String>,
    },

    // ---- outputs ----
    /// A moment of T halted for the owner (T is slow): halt N, WHAT it is (give, take, deliver, open, decide).
    Halted { at: At, t: u64, n: u32, what: String },
    /// A child opened by a ticket's `open`, with its program as the kind file had it then.
    Opened {
        at: At,
        t: u64,
        kind: String,
        parent: u64,
        program: String,
        /// The calls PROGRAM names, as they were when it was written (calls on the record, paper-research N-101 p392;
        /// the owner, N-221.p3): a replay checks and runs PROGRAM against these, not today's. None on older lines:
        /// today's calls then, as before.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        calls: BTreeMap<String, CallSig>,
    },
    /// A step is owed by WORKER.
    Step {
        at: At,
        t: u64,
        step: u32,
        worker: String,
        call: String,
        /// A `hold`: owed but not delivered.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        hold: bool,
        /// The owner's step about his posts its handler holds (base.kind's `clarify`, `Machine::question_about_held`):
        /// while it waits they are untold and unshown and hold back none of the posts after them (`Ticket::asked`;
        /// proto-lead N-1446 p569, (c2)). Not on a record's lines from before: its posts held back what came after
        /// them, and replay so.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        asked: bool,
    },
    /// The arguments of step STEP of T, by the call's parameter names: written right after its `step` line when it
    /// has any (N-101 post 55, E; record-to-events.md point 5). Not compared in replay, so a log from before it
    /// replays as it is (`compared`).
    StepArgs { at: At, t: u64, step: u32, args: Value },
    /// Policy is asked who fills ROLE for the step.
    Asked { at: At, t: u64, step: u32, question: String, role: String },
    /// Post N was added (by an answer, a formed head, or a `post` in the pattern), with whom it names, informs and asks
    /// into the turn of (NAMES, INFORM, INTO_TURN), as its `Post` has them (`Post::posted`): a release that does not
    /// load the input it came from takes it as recorded, to them (`as_recorded`; proto-mind N-1446 p802: an aside's
    /// passed-up post replayed naming nobody). A log from before has none of the three, and replay compares them only
    /// where the recorded line has one (`same`).
    Posted {
        at: At,
        t: u64,
        n: u32,
        by: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<u32>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        names: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        inform: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        into_turn: Vec<String>,
    },
    /// The rule Reply: AGENT's turn, given only posts on T and answering no step, said its last message there as post N,
    /// naming the author of the last post it was given (nobody when that post was itself a reply).
    /// ANSWERING: of NAMES, the Reply's own, not the `@`s its message typed (`Line::Post`'s `answering`); none on a line
    /// from before it, and a replay takes it from the line, as NAMES.
    Replied { at: At, t: u64, n: u32, by: String, #[serde(default, skip_serializing_if = "Vec::is_empty")] names: Vec<String>, #[serde(default, skip_serializing_if = "Vec::is_empty")] answering: Vec<String> },
    /// An answer to the step did not type.
    Refused { at: At, t: u64, step: u32, why: String },
    /// The worker is told the step again, once.
    Reminded { at: At, t: u64, step: u32 },
    /// A turn's output is being formed into the step's head.
    Forming { at: At, t: u64, step: u32 },
    /// T's program gave words to agent TO as they are (`give`), the Nth gift of T.
    Gave { at: At, t: u64, to: String, n: u32 },
    /// T's program reached a hole (`...`): it rests, open, owing nothing, until an edit gives it more.
    Rests { at: At, t: u64 },
    /// Part T of a `par` in order (`as K after A [reach KIND]`, N-101) is opened and does not start until each part of
    /// ON (its siblings) has reached the state: the latest ticket of kind REACH under it closed Done, else the part
    /// itself closed Done.
    Holds { at: At, t: u64, on: Vec<u64>, #[serde(default, skip_serializing_if = "Option::is_none")] reach: Option<String> },
    /// Part T, held, starts now: every part it stands on has reached the state.
    Freed { at: At, t: u64 },
    /// T's handler of EVENT (`hole`) begins to run in T itself, as PROGRAM: what replay runs (paper-research m1430;
    /// the handlers run in tickets of their own record theirs in their Opened line).
    Handling { at: At, t: u64, event: String, program: String },
    /// The ask T stands for was answered (`reply`), to TO: in its turn (Effect::Answered), or LATER as a delivery.
    AskAnswered { at: At, t: u64, to: String, #[serde(default, skip_serializing_if = "std::ops::Not::not")] later: bool },
    /// Step STEP of T was to go to FROM, who waits through asks on the one T answers: it goes to TO (`lead`, the
    /// role asked again, or `owner`) instead, WHY saying so.
    PassedUp { at: At, t: u64, step: u32, from: String, to: String, why: String },
    /// T stopped at a failure nothing above it handles (paper-research N-101 p934): RAISED is the failure, and TO (the
    /// lead, else the owner) is given the step `stopped` to say whether it runs again or ends. Its stack stays.
    Stopped { at: At, t: u64, raised: Raised, to: String },
    /// The minder's answer at T's hole (N, its count on T), carried out: ANSWER as given; REFUSED, why it was not
    /// (a program that does not check, a result that does not fit): T rests then. Not a post.
    Shaped { at: At, t: u64, n: u32, answer: Value, #[serde(default, skip_serializing_if = "Option::is_none")] refused: Option<String> },
    /// The owner's post N of T, held by its handler, wakes TO; INFORM, the others on T, see it at their next turn
    /// (`tell`, design-two-systems §13); JOINS, the earlier post of his it goes on from (`tell … joining`).
    Told { at: At, t: u64, n: u32, #[serde(default, skip_serializing_if = "Vec::is_empty")] to: Vec<String>, #[serde(default, skip_serializing_if = "Vec::is_empty")] inform: Vec<String>, #[serde(default, skip_serializing_if = "Option::is_none")] joins: Option<u32> },
    /// The owner's post N of T withdrawn by its handler (`drop`): no agent sees it.
    Dropped { at: At, t: u64, n: u32 },
    /// The owner's post N of T, held by a handler that ended having neither told nor dropped it: delivered as
    /// policy says, as if there were no handler.
    Released { at: At, t: u64, n: u32 },
    /// What agent's post N of T asks the owner, as the watcher's passages found in it (`mark`, design §16, 2).
    Marked { at: At, t: u64, n: u32, marks: Vec<crate::asks::Mark> },
    /// An agent of a program: WHAT is `made`, `ended`, `restarted` (a step to it after its block ended) or `stopped`
    /// (its running turn ended by `stop`, its step still owed).
    Agent { at: At, t: u64, agent: String, what: String },
    /// A scoped agent began as a fork of FROM (`step fork`, `let agent x = fork`, N-101): a copy of FROM's conversation
    /// as it stood then; FROM goes on unchanged. It ends as any scoped agent does (an `agent` line, `ended`).
    Forked { at: At, t: u64, agent: String, from: String },
    /// Post N of T was typed by hand into AGENT's terminal (`Post::typed`): his words to it, already in its turn.
    Typed { at: At, t: u64, n: u32, agent: String },
    /// Post N of T answers the messages ALSO too (`N-355.p1`, on any ticket): the reply tool's `to` list (paper-research
    /// N-101 p1361). Each of them is answered as if by a post of N's writer on its own ticket (`Ticket::answered_from`).
    AnswersAlso { at: At, t: u64, n: u32, also: Vec<String> },
    /// Post N of T was posted, the same words by the same writer, on the other threads too: ON, those posts' ids
    /// (`N-12.p4`), one message on several threads (the owner's words N-354.p1, N-355.p1; paper-research N-695.p1, 2).
    /// Nothing is given to anyone for it.
    PostedOn { at: At, t: u64, n: u32, on: Vec<String> },
    /// The owner's post N of T is the cleaned text of his words ticket WORDS (N-487): his words as he typed them stay
    /// that ticket's form `text`, reached from the post (`Post::cleaned_from`, `coord show T.pN`). Nothing is given to
    /// anyone for it.
    Cleaned { at: At, t: u64, n: u32, words: u64 },
    /// The owner's post N of T is point POINT of OF of one message of his split by point before he sent it (N-696,
    /// paper-research N-590 p12): each point he took went as his own post, to its own thread, and each carries the
    /// same MESSAGE, so the others are found from any one (`Post::split_from`). Nothing is given to anyone for it.
    Split { at: At, t: u64, n: u32, from: SplitFrom },
    /// The owner's post N of T waits for the turn's end of whom it tells (his "when done", paper-research N-101 p1216,
    /// p1390): no post of theirs reads past it before it is given them. Without N, the message the intake ticket T
    /// carries: the owner's post its program makes of it (`Ticket::waits_message`).
    Waits {
        at: At,
        t: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        n: Option<u32>,
    },
    /// What was due to AGENT and held back at a hand-over (IDS), and WHY: the owner's message to it goes first and
    /// alone (paper-research N-101 p1099). A record line only; nothing replays from it.
    Held { at: At, agent: String, ids: Vec<String>, why: String },
    Closed {
        at: At,
        t: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        raised: Option<Raised>,
    },
}

impl Line {
    pub fn is_input(&self) -> bool {
        // the message runtime's record lines are `event` lines, and two of them are outputs (syntax::delivery_output)
        if let Line::Event { name, fired, on: None, .. } = self {
            if fired.is_empty() && crate::syntax::delivery_output(name) {
                return false;
            }
        }
        matches!(
            self,
            Line::Open { .. } | Line::Post { .. } | Line::Answer { .. } | Line::TurnEnded { .. } | Line::TurnCut { .. } | Line::Unread { .. } | Line::Formed { .. } | Line::Given { .. } | Line::Decided { .. } | Line::Pinned { .. } | Line::Expired { .. } | Line::Lapsed { .. } | Line::Notified { .. } | Line::Unchecked { .. } | Line::Event { .. } | Line::Unrendered { .. } | Line::Edited { .. } | Line::Started { .. } | Line::Tied { .. } | Line::Titled { .. } | Line::Ended { .. } | Line::Move { .. } | Line::Keep { .. } | Line::Undo { .. } | Line::Ask { .. } | Line::AskLater { .. } | Line::NothingToAnswer { .. } | Line::AsksWithdrawn { .. } | Line::TakenBack { .. } | Line::Comments { .. } | Line::CommentResolved { .. } | Line::Star { .. } | Line::Archive { .. } | Line::Reopen { .. } | Line::Esc { .. } | Line::Slow { .. } | Line::Slowed { .. } | Line::Approve { .. } | Line::HaltEdit { .. } | Line::HaltReject { .. } | Line::Urgent { .. } | Line::LayerFailed { .. } | Line::StartFailed { .. } | Line::RunnerRefused { .. } | Line::Unfit { .. } | Line::ModelRefused { .. } | Line::Made { .. } | Line::Moved { .. } | Line::Act { .. } | Line::Role { .. } | Line::Template { .. } | Line::Reload { .. } | Line::Taken { .. } | Line::Typed { .. } | Line::Held { .. } | Line::AnswersAlso { .. } | Line::PostedOn { .. } | Line::Cleaned { .. } | Line::Split { .. } | Line::Waits { .. }
        )
    }
}

/// How an imported ticket was closed: by whom, when, with which result and words.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Closing {
    pub by: String,
    pub at: At,
    pub head: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<Text>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Raised {
    pub exc: String,
    pub value: Value,
}

/// What comes from outside. The machine completes each into its input line (a number for a new ticket, the program
/// it runs, the step a turn's end answers).
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Open { kind: String, under: Option<u64>, opener: String, form: Value, text: Option<Text>, names: Vec<String>, told: Vec<String> },
    /// KIND: what the post is, as its writer typed it (`coord post`'s flag, the reply tool's `kind`; I-1480).
    Post { t: u64, by: String, text: Text, names: Vec<String>, told: Vec<String>, inform: Vec<String>, answers: Option<u32>, kind: Option<PostKind> },
    /// BY passes the owner's `@all`, post N of T, on to agent TO (`coord pass`, the pass tool; consult-lead N-412 post
    /// 1900): a post of TEXT by BY naming TO (`Line::Post`'s `passes`), refused unless BY holds it, nobody claimed it and
    /// TO never held it before (`Machine::pass_refused`). Whether TO can be woken is coordd's to check.
    Pass { t: u64, by: String, n: u32, to: String, text: String },
    Answer { t: u64, by: String, head: Value, text: Text },
    TurnEnded { agent: String, delivery: Option<String>, text: Loc, later: Vec<String>, stood: Option<String>, go: Option<String> },
    TurnCut { agent: String },
    /// BY stops AGENT's running turn (`coord esc`), and ALSO's with it; ALL: the project-wide act (Line::Esc).
    Esc { by: String, agent: String, also: Vec<String>, all: bool },
    Unread { agent: String },
    /// WHY and CALL as `Line::Formed` has them.
    Formed { t: u64, step: u32, head: Option<Value>, why: Option<String>, not_taken: Option<String>, call: Option<String> },
    /// SKIPPED: what the hand-over left out (`Machine::left_out`), None when coordd does not know (`Line::Given`).
    Given { t: u64, to: String, delivery: String, upto: u32, place: Option<Loc>, skipped: Option<std::collections::BTreeSet<u32>> },
    /// Post N of T answers ALSO too (`Line::AnswersAlso`).
    AnswersAlso { t: u64, n: u32, also: Vec<String> },
    /// Post N of T is also posted as ON (`Line::PostedOn`).
    PostedOn { t: u64, n: u32, on: Vec<String> },
    /// Post N of T is words ticket WORDS cleaned (`Line::Cleaned`).
    Cleaned { t: u64, n: u32, words: u64 },
    /// Post N of T is a point of a message of his split by point (`Line::Split`).
    Split { t: u64, n: u32, from: SplitFrom },
    /// The owner's post N of T, or the message T carries, waits for the turn's end (`Line::Waits`).
    Waits { t: u64, n: Option<u32> },
    /// A delivery taken by a session in a running turn (`Line::Given` with `into`).
    GivenInto { t: u64, to: String, delivery: String, upto: u32, place: Option<Loc>, skipped: Option<std::collections::BTreeSet<u32>> },
    Decided { t: u64, step: u32, question: String, answer: Value, provenance: Value },
    Pinned { loc: Loc, to: Loc },
    Expired { t: u64, step: u32, provenance: Value },
    Lapsed { t: u64, step: u32, head: Value, after: u64, because: String },
    Notified { t: u64, event: String, to: Vec<String>, inform: Vec<String>, post: Option<u32>, provenance: Value },
    Unchecked { act: String, by: String, t: Option<u64>, kind: String, why: String },
    Event { name: String, value: Value },
    Unrendered { t: u64, step: u32, why: String },
    /// Post N of T was typed into AGENT's terminal (`Line::Typed`).
    Typed { t: u64, n: u32, agent: String },
    /// Held back from AGENT at a hand-over (`Line::Held`).
    Held { agent: String, ids: Vec<String>, why: String },
    /// Today's item T, moved over once with its number kept (an unused one): as `Open`, and when CLOSED is given,
    /// closed so at once.
    Import { t: u64, kind: String, opener: String, form: Value, text: Option<Text>, closed: Option<Closing> },
    /// BY works on the open item T.
    Started { t: u64, by: String },
    /// BY ties T under TO in the outline, or unties it (None).
    Tie { t: u64, to: Option<u64>, by: String },
    /// BY names T LABEL.
    Title { t: u64, label: String, by: String },
    /// BY ends thread T: HOW `done`, `dropped` or `kept`, WHY its one line.
    End { t: u64, by: String, how: String, why: String },
    /// Post N of T moved to ticket TO, or to a new thread (a note) when None.
    Move { t: u64, n: u32, to: Option<u64>, by: String },
    /// One-offs made one thread.
    Keep { ts: Vec<u64>, by: String },
    NothingToAnswer { t: u64, n: u32, by: String },
    WithdrawAsks { t: u64, n: u32, by: String, why: String },
    TakeBack { t: u64, n: u32, by: String },
    Comments { t: u64, n: u32, by: String, comments: Vec<crate::comment::Comment> },
    CommentResolved { t: u64, n: u32, id: String, resolved: bool, by: String },
    /// Post N of T was taken back while a hand-over carrying it was on its way to TO (coordd's `handed`), and TO
    /// took it: TO had it, so it is struck and TO is told (0.2 B8, N-1396; consult-3, N-818 p74).
    StruckFor { t: u64, n: u32, to: String },
    Star { t: u64, on: bool, by: String, post: Option<u32> },
    /// The owner archives T (ON) or unarchives it (`Line::Archive`).
    Archive { t: u64, on: bool, by: String },
    Reopen { t: u64, by: String },
    Undo { t: u64, by: String },
    /// A blocking ask by BY (`coord ask` with block_ask): TEXT its first line, CONTEXT the rest.
    Ask { by: String, text: String, context: Option<String> },
    /// BY's blocking ask stopped waiting.
    AskLater { by: String },
    /// Withdraw: PROGRAM takes the place of the step T awaits, checked in the step's scope against its head.
    Withdraw { t: u64, by: String, program: String, why: String },
    /// Edit: PROGRAM, in which `answer` is the answer to the step T awaits, is what T does with it from now on.
    Edit { t: u64, by: String, program: String, why: String },
    /// Repair: the failed ticket T runs PROGRAM, checked as its kind's pattern would be.
    Repair { t: u64, by: String, program: String, why: String },
    /// Rekind: T becomes a KIND with FORM, and runs its pattern; its posts stay.
    Rekind { t: u64, by: String, kind: String, form: Value, why: String },
    /// Cancel: T ends as failed, `Cancelled { by, why }`; its open children are cancelled in turn.
    Cancel { t: u64, by: String, why: String },
    /// Slow mode on T switched ON or off by BY (the owner).
    Slow { t: u64, on: bool, by: String },
    /// Slow over SCOPE set (ON) or lifted by BY, whom coordd's policy let.
    Slowed { scope: String, on: bool, by: String },
    /// Halt N of T let through by BY.
    Approve { t: u64, n: u32, by: String },
    /// Halt N of T edited by BY.
    HaltEdit { t: u64, n: u32, by: String, value: Value },
    /// Halt N of T refused by BY, for REASON.
    HaltReject { t: u64, n: u32, by: String, reason: Option<String> },
    /// The owner's words for AGENT handed at once, by BY (`Line::Urgent`).
    Urgent { agent: String, by: String },
    /// coordd: layer LAYER could not be made, for WHY (`Line::LayerFailed`).
    LayerFailed { layer: String, why: String },
    /// coordd: scoped agent AGENT of T could not be started, for WHY (`Line::StartFailed`).
    StartFailed { t: u64, agent: String, why: String },
    /// coordd: the runner refused step STEP of T, saying WHY (`Line::RunnerRefused`).
    RunnerRefused { t: u64, step: u32, why: String },
    /// coordd: the answer to step STEP of T is refused for WHY (`Line::Unfit`).
    Unfit { t: u64, step: u32, why: String, call: Option<String> },
    /// coordd: step STEP of T refused to AGENT for its model, saying WHY (`Line::ModelRefused`).
    ModelRefused { t: u64, step: u32, agent: String, why: String },
    /// coordd: AGENT made by BY, a copy of FROM, while BY owed a step on T (`Line::Made`).
    Made { t: Option<u64>, agent: String, by: String, from: Option<String> },
    /// coordd: agents the registry has that the record did not make (`Line::Taken`).
    Taken { names: Vec<String> },
    /// coordd: AGENT moved between views (`Line::Moved`).
    Moved { t: Option<u64>, agent: String, from: Option<String>, to: Option<String>, why: String, killed: Vec<String>, left: Vec<String> },
    /// AGENT's role is ROLE (`Line::Role`).
    Role { agent: String, role: String, also: Vec<String> },
    /// An act of AGENT (`Line::Act`); `Event { name: "act" }` with `{by, is, text, ...}` is taken as one too. On the
    /// ticket AGENT owes a step on; T, given, only when it owes none (an edit coordd found in a layer falls on the
    /// layer's ticket: a minder's, coordd found.rs).
    Act { agent: String, is: String, text: String, more: serde_json::Map<String, Value>, t: Option<u64> },
    /// The owner's once-edit of CALL's prompt on T (`Line::Template`).
    Template { t: u64, call: String, scope: String, text: String, by: String, why: String },
    /// The kinds and calls read again (`Line::Reload`).
    Reload { by: String, why: String, commit: Option<String>, kinds: Vec<String>, calls: Vec<String> },
    /// After a reload, open ticket T of an edited kind takes its kind's new text for what it has not run (live edits;
    /// `Line::Edited` with what `reload`), or keeps its old text and the reply says why.
    Retext { t: u64, by: String, why: String },
}

/// A ticket an `on` binding opened: its number, kind, the binding as written, its form, and its program as the kind
/// file had it then.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fired {
    pub t: u64,
    pub kind: String,
    pub binding: String,
    pub form: Value,
    pub program: String,
    /// The calls PROGRAM names, as they were then (as `Line::Open`'s).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub calls: BTreeMap<String, CallSig>,
}

/// A question to policy, carried out by coordd off its lock and answered by `Input::Decided`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Question {
    /// Who fills ROLE for the step (a Decision): the call's name and the ticket's facts go with it.
    Fills { role: String, call: String },
    /// Whether the GOALS of the ticket hold (Owed): each the policy's act of that name. Answered with
    /// `Input::Decided { step: 0, question: "holds", answer: [the goals that do not hold] }`.
    Holds { goals: Vec<String> },
    /// The policy's decision NAME on ARGS (`decide`, N-101): answered with `Input::Decided { question: "decide",
    /// answer }`, the answer a constructor of the decision's answers by name (its fields from ARGS of the same names),
    /// or null when no rule decides it: the owner answers then.
    Decide { name: String, args: Value },
    /// Whether AGENT's answer ANSWERED to step STEP (call CALL) breaks a duty (N-101, `Violated`; asked only when the
    /// policy declares duties): answered with `Input::Decided { question: "violated", answer: [{duty, holder,
    /// claimant?, work?}...] }`, [] when none is broken; null when the ask ended unanswered because the ticket no
    /// longer asks it (closed while it was asked): a record only.
    Violated { agent: String, call: String, answered: Value },
    /// Goal GOAL's duties (its id), read over LINES, its lines as the record writes them (`machine/duties.rs`; N-1511
    /// p47): answered with `Input::Decided { question: "duties", answer: {goal, owed, by, broken} }` (the `duties`
    /// event's value, `syntax::system_event`), null when the ask ended unanswered or the policy could not read them.
    Duties { goal: String, lines: Vec<Value> },
}

impl Question {
    /// Its name, as its `asked` and `decided` lines write it.
    pub fn name(&self) -> &'static str {
        match self {
            Question::Fills { .. } => "fills",
            Question::Holds { .. } => "holds",
            Question::Decide { .. } => "decide",
            Question::Violated { .. } => "violated",
            Question::Duties { .. } => "duties",
        }
    }
}

/// What the machine asks coordd to do, after the lock is released.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// TO may have a new delivery (`Machine::due`): hand over when its session is free.
    Deliver { to: String },
    /// The program delivers ITEMS to TO as one message, REST named at its foot as still waiting (0.2, `deliver`):
    /// coordd renders and hands it, then reports each ticket's part with `Input::Given`. DIGEST (I-1480): the changes
    /// on TO's structures, given with it by their first lines, and reported with `Input::Given` as ITEMS are.
    Hand { to: String, items: Vec<String>, rest: Vec<String>, digest: Vec<String> },
    /// Ask policy; answer with `Input::Decided { t, step, .. }`.
    Ask { t: u64, step: u32, question: Question },
    /// Form the step's head from TEXT with a stateless call whose return type is HEAD; answer with `Input::Formed`.
    /// WORDS: the owner's words to form instead (a message placed on a ticket that owes the owner a step); TEXT is
    /// then no place.
    Form { t: u64, step: u32, agent: String, call: String, head: Type, text: Loc, words: Option<String> },
    /// A scoped agent for ticket T begins: start it for CALL.
    Start {
        agent: String,
        t: u64,
        call: String,
        /// The layer it starts in (`Machine::layer_of`): its view is the agent's cwd.
        layer: Option<String>,
    },
    /// A scoped agent for ticket T begins as a fork of FROM (N-101): a copy of FROM's conversation as it stands now,
    /// resumed as a session of its own, in LAYER's view if any (in a boxed layer, the copy in its own home), for CALL.
    Fork {
        agent: String,
        t: u64,
        call: String,
        layer: Option<String>,
        from: String,
    },
    /// A copy of FROM begins for FROM's delivery ticket T (`fork FROM at PLACE`, N-1104 p1154): a scoped agent started
    /// from FROM's session cut after the line delivery PLACE was given on (the turn FROM is in began there), as part 4's
    /// restart cuts it (coordd `Core::fork_place`, follow.rs `fork_cut`), beside FROM, which goes on untouched; for
    /// CALL, its prompt (agent-base `prompts/copy.md`). It is handed what its delivery hands it, and ends at `End`.
    Copy { agent: String, t: u64, call: String, from: String, place: String },
    /// The scoped agent ends: pin what the ticket references from its transcript, then remove it.
    End { agent: String },
    /// Stop AGENT's running turn (`coord esc`, Line::Esc): Escape through its terminal's holder, or a Codex interrupt.
    Interrupt { agent: String },
    /// Ask policy whom `notify` adds for EVENT on T, ACTOR having caused it; answer with `Input::Notified`. POST: the
    /// post the event is (`post`), given back in the answer.
    Notify { t: u64, event: String, actor: String, post: Option<u32> },
    /// Ticket T changed (emit its view).
    Changed { t: u64 },
    /// AGENT's delivery did an act (`Desk::acts`): emit its view, which carries its last acts.
    Acted { agent: String },
    /// AGENT's waiting ask is answered: TEXT is its command's result.
    Answered { agent: String, text: String },
    /// AGENT's waiting ask was about a step since withdrawn: D, the withdrawal, is its command's result as the
    /// surfaces render it (paper-research m1469, 2).
    AnsweredWith { agent: String, d: Delivery },
    /// The machine raises event NAME with VALUE (the brain's OWNER_TO, 0.3; proto-mind N-1446 p36): coordd sends it
    /// back as it sends its own events (`ticket_event`), so it is a recorded `event` line, counted and fired there.
    Event { name: String, value: Value },
}

/// What a post is, as its writer typed it (I-1480, paper-research N-1481 p1; the owner, N-1104 p707: "push pull is
/// the right idea"): who must act on it now, and who reads it at their next turn's start. An `ask`, `read` or
/// `blocker` is for TO, who must act; a `hold` stops the work on item ON (`I-…`) until it is settled; a `result` is
/// what its writer did or found; an `fyi` is news nobody has to act on. On the line as
/// `{"is": "read", "to": ["w2"], "head": "core2-posted 149feba4"}`; a post with none is untyped.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "is", rename_all = "lowercase")]
pub enum PostKind {
    Ask { to: Vec<String> },
    Read { to: Vec<String>, head: String },
    Blocker { to: Vec<String> },
    Hold { on: String },
    Result,
    Fyi,
}

impl PostKind {
    /// Its word: ask, read, blocker, hold, result, fyi.
    pub fn is(&self) -> &'static str {
        match self {
            PostKind::Ask { .. } => "ask",
            PostKind::Read { .. } => "read",
            PostKind::Blocker { .. } => "blocker",
            PostKind::Hold { .. } => "hold",
            PostKind::Result => "result",
            PostKind::Fyi => "fyi",
        }
    }

    /// Whom it asks to act: the TO of an ask, read or blocker; nobody for the others.
    pub fn to(&self) -> &[String] {
        match self {
            PostKind::Ask { to } | PostKind::Read { to, .. } | PostKind::Blocker { to } => to,
            _ => &[],
        }
    }

    /// Whether it is an ask to the owner (`coord post --ask owner`, the reply tool's `ask` with `to` owner): an agent's
    /// such post asks him as marked passages did, open from when it is pushed (`Post.asks_open`) until his answer, his
    /// "nothing to answer", the asker's withdrawal, its taking back or his cancel (paper-research N-1446 p1102).
    pub fn asks_owner(&self) -> bool {
        matches!(self, PostKind::Ask { to } if to.iter().any(|a| a == "owner"))
    }
}

/// A post as deliveries and views carry it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Post {
    pub n: u32,
    pub at: At,
    pub by: String,
    pub text: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
    /// Whom policy's `notify` added.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub told: Vec<String>,
    /// Whom policy's `inform` added: it reaches them at their next turn, starting none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inform: Vec<String>,
    /// Whom its own words ask to go into the running turn of while that turn holds the thread it stands at: the
    /// agents a program's `post … informing A` names (the brain's line, 0.3), each by name, so an agent policy informs
    /// of the same post (`Line::Notified`) is not put into its turn (proto-reader N-1446 p177, consult-3 p181).
    /// Policy's inform never asks it (proto-reader p112: an agent's post on a thread under another is not put into
    /// the parent's turn).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub into_turn: Vec<String>,
    /// Posted by the rule Reply (an agent's turn given one post and answering nothing): its last message.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto: bool,
    /// Moved to this ticket (`N-5`): the post stays here as a stub.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moved: Option<String>,
    /// Moved here from this post (`N-4.p1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// The owner's post, held by the `when owner_post` handler running in this ticket (its id): no agent is given it
    /// in any form until the handler tells or drops it (design-two-systems §13).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held: Option<String>,
    /// Halted for the owner (slow mode, a `deliver` halt): no agent is given it, nor a post after it, until he
    /// approves it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub halted: bool,
    /// Dropped by its handler: no agent ever sees it; the owner sees it struck through.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dropped: bool,
    /// Taken back by its author before anyone was handed it (`Line::TakenBack`, paper-research N-447 p16, 2): it keeps
    /// its number, no agent is given it, and the views do not draw it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub taken_back: bool,
    /// Its handler said whom it wakes (`tell`): TOLD are woken, INFORM the others on the ticket; NAMES (whom it was
    /// typed to, and each @NAME) no longer wake.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub routed: bool,
    /// The owner's earlier post on this ticket that this one of his goes on from, as its handler told it (`tell …
    /// joining`; the thread's holder, route.call's `joins`): those told read which post it goes on from, and one still
    /// theirs to answer is answered with it (hs-ls N-1446 p38). None for a post told without it, and in a record from
    /// before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joins: Option<u32>,
    /// What it asks the owner (an agent's post, `mark`): passages of its words, by offset (`asks::Mark`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asks: Vec<crate::asks::Mark>,
    /// Its asks wait on the owner: from `mark` until its author is told something on the ticket after it (a post
    /// that wakes it, a step given it), posts there again, or the owner says there is nothing to answer.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub asks_open: bool,
    /// The owner's post that answers the asks of post N (the card filled in): drawn inside that post.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answers: Option<u32>,
    /// How its asks were closed, when it has some and they are not open (a view's, `Machine::asks_closed`; design
    /// §16, paper-research m1463, 3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asks_closed: Option<AsksClosed>,
    /// Typed by hand into this agent's terminal (its Claude Code or Codex session), not given through coord: the
    /// owner's words as his transcript holds them (paper-research N-101 p1107).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typed: Option<String>,
    /// The further messages it answers (the reply tool's `to` list, paper-research N-101 p1361): `N-355.p1`, on this
    /// ticket or another (`Line::AnswersAlso`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<String>,
    /// What it is, as its writer typed it (`Line::Post`'s `kind`, I-1480); None: untyped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<PostKind>,
    /// Of its names, those the rule for a post naming nobody, or the turn's end's Reply, gave it and its writer did not
    /// type: its answer to them, which asks them nothing by itself (`Line::Post`'s and `Line::Replied`'s `answering`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answering: Vec<String>,
    /// Its writer's comments on passages, as data (`Line::Comments`, `Line::CommentResolved`); none on a post from
    /// before them, whose text alone holds them (`comment::read_back`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<crate::comment::Comment>,
    /// It passes the owner's `@all`, post N of this ticket, on to the agent it names (`Line::Post`'s `passes`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passes: Option<u32>,
    /// The same message posted on other threads too (`N-12.p4`, `Line::PostedOn`): each says the others.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on: Vec<String>,
    /// The owner's words ticket whose cleaned text this post is (`W-12`, `Line::Cleaned`, N-487): what he typed is
    /// that ticket's form `text`, one click away in his window and a line in `coord show`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleaned_from: Option<String>,
    /// The message of his this post is a point of, when he split it by point before sending (`Line::Split`, N-696):
    /// the posts with the same `message` are its other points, each on its own thread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split_from: Option<SplitFrom>,
    /// On a post whose asks are open to the owner (a view's, `Machine::would_close`): which of his posts would close
    /// them, for the one box's line (paper-research N-447 p16, 1; the shape p18).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub would_close: Option<WouldClose>,
}

/// One message of the owner's split by point (N-696): MESSAGE, the key his window gave the message when he sent it
/// (the same on every point of it); POINT, this one's place among its OF points, from 1.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitFrom {
    pub message: String,
    pub point: u32,
    pub of: u32,
}

/// Whether each of the owner's three targets would close the open asks of a post (N-447 p18): FOOT, his post at the
/// thread's foot typed to nobody; REPLY, his reply at the post typed to its author (the card's); SUBTHREAD, the first
/// post of a thread tied at the post, typed to its author.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WouldClose {
    pub foot: Closes,
    pub reply: Closes,
    pub subthread: Closes,
}

/// Yes, no, or only if the routing (the watcher's `route`, policy's `notify`) takes it to the asker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Closes {
    Yes,
    No,
    Routed,
}

/// How a post's asks were closed: what came first after it of what closes them (`Machine::asks_open`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "how")]
pub enum AsksClosed {
    /// Post POST, by BY, woke the asker: the owner's answers (its `answers` names the post), or anyone's words to it.
    Answered { post: u32, by: String },
    /// Step STEP of the ticket was given the asker; MINDER, the minder's answer whose program has it, if one does.
    Told {
        step: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        minder: Option<u32>,
    },
    /// The owner said there is nothing to answer.
    NothingToAnswer,
    /// The asker posted again: post POST.
    Posted { post: u32 },
    /// A thread following the ticket was opened since: TICKET (the owner's words for a closed ticket open one).
    Followed { ticket: String },
    /// The owner cancelled the ticket after it: his act on the whole ticket.
    Cancelled,
    /// BY withdrew them, with WHY: the asker, or an agent above it (`Line::AsksWithdrawn`).
    Withdrawn { by: String, why: String },
    /// A thread tied at the post, opened after it, whose first post wakes the asker: TICKET (an answer given in a
    /// subthread, N-447 p18).
    Subthread { ticket: String },
    /// A way this build does not know, from a newer engine: read, not refused (paper-research N-447 p19, 3).
    #[serde(other)]
    Unknown,
}

impl Post {
    /// Its `posted` line, on T: every post's line is written from the post it records.
    pub fn posted(&self, t: u64) -> Line {
        let (names, inform, into_turn) = (self.names.clone(), self.inform.clone(), self.into_turn.clone());
        Line::Posted { at: self.at.clone(), t, n: self.n, by: self.by.clone(), step: self.step, names, inform, into_turn }
    }
    /// Whom it wakes: TOLD once its handler has routed it, else those it names and those policy told.
    pub fn wakes(&self) -> Box<dyn Iterator<Item = &String> + '_> {
        if self.routed { Box::new(self.told.iter()) } else { Box::new(self.names.iter().chain(&self.told)) }
    }
    /// Whether it reaches AGENT only as one on its ticket: by policy's inform (notify.eflint's `inform`, every agent
    /// taking part), not by its names, whom it tells, a program's `informing` (`into_turn`) or its handler's route. An
    /// agent's post is informed by the policy alone (`Line::Post`'s `inform`, and a `notified` line's); the owner's,
    /// routed, by its route's others (`Line::Told`), which is the route's word, not membership. No record change
    /// tells them apart (paper-research N-1446 p2092; proto-mind N-1511 p20).
    pub fn only_as_member(&self, agent: &str) -> bool {
        self.by != "owner" && !self.routed && self.inform.iter().any(|a| a == agent) && !self.wakes().any(|a| a == agent) && !self.into_turn.iter().any(|a| a == agent)
    }
    /// Whether an agent may be shown it: not held, not dropped, and not coordd's own ("AGENT was given D twice and did
    /// not read it."): that is a fact for coordd's journal and the line under the post it is about, never a message
    /// (paper-research N-412.p1844, 2; consult-lead p1900, 2). It keeps its number, so a record replays the same.
    pub fn visible(&self) -> bool {
        self.held.is_none() && !self.dropped && !self.halted && !self.taken_back && self.by != "coordd"
    }
}

/// Something due to an agent from a ticket: rendered by the surfaces when the session is free, then reported with
/// `Input::Given { delivery: id, upto }`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Delivery {
    /// `C-12.s3` (a step), `C-12.s3.again` (its reminder), `C-12.s3.refused2` (a refusal), `C-12.p7` (posts up to
    /// 7), `C-12.c` (the closing notice).
    pub id: String,
    pub to: String,
    pub t: u64,
    /// The ticket's id as shown, `C-12`.
    pub ticket: String,
    pub kind: String,
    /// The post number the delivery shows up to: what `Given.upto` reports.
    pub upto: u32,
    pub what: What,
    /// Policy said inform, not notify: it is there at the agent's next turn and starts none (plan/design-two-systems.md,
    /// section 10).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inform: bool,
}

impl Delivery {
    /// The numbers of the posts it shows: its unread ones, for a posts, step or closing delivery; none else.
    pub fn shows(&self) -> Vec<u32> {
        match &self.what {
            What::Posts { unread } | What::Step { unread, .. } | What::Closed { unread, .. } => unread.iter().map(|p| p.n).collect(),
            _ => Vec::new(),
        }
    }
}

/// An answer to a step that its reading read as none of the step's outcomes: BY's TEXT, and WHY, the reading's
/// sentence (empty when it gave none).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NotRead {
    pub by: String,
    pub text: Text,
    pub why: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "what")]
pub enum What {
    /// A step owed: ARGS by the call's parameter names; HEAD the type of the answer; AGAIN when it is the reminder.
    Step {
        step: u32,
        call: String,
        args: Value,
        unread: Vec<Post>,
        blind: bool,
        head: Type,
        again: bool,
        /// An inline call's text, its places filled (CALL is `inline`): what the worker is asked, as it stands.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        template: Option<String>,
        /// What TEMPLATE's markers `{{@K}}` stand for: texts held at a place (an agent's turn), filled with their
        /// words where the delivery is rendered (render.rs), as a Loc is read.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        refs: Vec<Text>,
        /// The owner's text for the prompt, edited at the step's halt (slow mode): sent as it is, in place of what the
        /// call would render.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
        /// The owner's template for this step, edited once on its ticket or a ticket above it (`Line::Template`):
        /// rendered with ARGS in place of the call's template.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        edit: Option<String>,
        /// A reminder's (AGAIN): the forming call's sentence on why the last answer is none of the step's outcomes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        why: Option<String>,
        /// A reminder's (AGAIN): the turn the step was put into ended, or was cut, without its answer
        /// (`RemindWhy::Left`); WHY is then none.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        left: bool,
        /// A step passed up after its agent's second answer was not read (paper-research N-101 p757): every answer to
        /// it that was not read, whole, each with the reading's reason.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        passed: Vec<NotRead>,
    },
    /// Posts naming the agent.
    Posts { unread: Vec<Post> },
    /// The ticket closed: RESULT, or RAISED.
    Closed { result: Option<Value>, raised: Option<Raised>, unread: Vec<Post> },
    /// STEP was withdrawn from the agent: it waited too long (Silent), or BY withdrew it or cancelled the ticket, for
    /// WHY. An answer to it now is refused.
    Withdrawn {
        step: u32,
        call: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        by: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        why: Option<String>,
    },
    /// The answer to STEP did not type; LEFT refusals before the step fails as Malformed.
    Refused { step: u32, call: String, why: String, left: u32, head: Type },
    /// Words given as they are (`give`, a one-off), FROM whom they are: nothing is posted, nothing answers them.
    Words { text: Text, from: String },
    /// Post N of the ticket, which the agent had, was taken back by BY (0.2 B8, N-1396): it is not to be acted on.
    TakenBack { n: u32, by: String },
}
