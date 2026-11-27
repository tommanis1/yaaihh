//! The machine: the rules of semantics.tex over all tickets, as a pure function of state and input. `apply` takes
//! one input and a time and gives the lines to append and the effects to carry out; `replay` applies the recorded
//! lines again and checks that it makes the same outputs. No IO, no clock, no threads.

use crate::answers;
use crate::check::{self, CallSig};
use crate::line::{At, Closing, Delivery, Effect, Fired, Input, Line, Post, Question, Raised, What, Worked};
use crate::syntax::{self, Bound, Every, Expr, Kind, Named, Pat, Prog, Worker};
use crate::types::{Loc, Part, PostRef, Span, Text, Type};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

mod asks_head;
mod delivery;
mod ledger;
pub use ledger::Commitment;
pub use delivery::{ACTS_KEPT, CloseChange, DELIVERY, Desk, Hand, delivered_post, own_of, post_id, post_of, told_post, post_or_ticket, post_ref, step_id};
mod ask_minded;
pub use ask_minded::HeldAsk;
mod violated;
mod duties;
mod fork;
mod parts;
mod retext;
mod seen;
mod stop;
pub use parts::Held;
pub use seen::Seen;

/// What a ticket may run: kinds and call signatures, already checked.
#[derive(Clone, Debug, Default)]
pub struct Defs {
    pub kinds: BTreeMap<String, Kind>,
    pub calls: BTreeMap<String, CallSig>,
    pub named: Named,
    /// The `on` bindings, in file order (syntax.tex, Bindings).
    pub bindings: Vec<syntax::Binding>,
    /// The project's events (`event ev { f: T ... }`): their values' fields.
    pub events: syntax::Events,
    /// The first ticket number (above the highest item number moved from today).
    pub first: u64,
    /// `coord ask` waits for its answer in the asker's turn (the project's `block_ask`; plan/design-two-systems.md,
    /// section 10). Off: an ask is a question ticket, answered when it closes.
    pub block_ask: bool,
    /// Each call's label: the first line of its doc (`///`), by name; a call with none is not here (`label`).
    pub call_docs: BTreeMap<String, String>,
    /// The project's policy declares a duty that can be violated (N-101, `Violated`): an agent's answer to a step is
    /// first put to the policy (`violated.rs`), and a `try` with a step to an agent may handle Violated. Set at build.
    pub duties: bool,
    /// By kind and call, by place of its answer, the calls the agent named there is given later (`given::agents_given`): an
    /// answer naming one whose model does not meet their classes is not taken (coordd). Set at build.
    pub agents_given: crate::given::Given,
    /// By kind and call, the lists of its answer whose entries name another list's elements (`given::names_given`):
    /// the forming of the answer says so and refuses an entry that names none (coordd). Set at build.
    pub names: crate::given::Names,
    /// The kinds `kind_as` read from tickets' own texts, kept.
    pub read: KindsRead,
}

/// The kinds `Defs::kind_as` read from tickets' own texts, by kind and text, each read once for these defs (`None`: the
/// text did not read). A ticket opened under an older text of its kind had it parsed again at every reading of it (the
/// title of each item waiting for an agent, every view of it): a tenth of the follow round's hold on the lock
/// (paper-research N-1396 p417). Defs made again (a reload) start with none, and so does a copy.
#[derive(Default)]
pub struct KindsRead(std::sync::Mutex<BTreeMap<String, BTreeMap<String, Option<std::sync::Arc<Kind>>>>>);

impl Clone for KindsRead {
    fn clone(&self) -> Self {
        KindsRead::default()
    }
}

impl std::fmt::Debug for KindsRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KindsRead")
    }
}

/// A kind as `Defs::kind_as` gives it: today's, or one read from a ticket's own text and kept (`KindsRead`).
pub enum KindAs<'a> {
    Today(&'a Kind),
    Read(std::sync::Arc<Kind>),
}

impl std::ops::Deref for KindAs<'_> {
    type Target = Kind;
    fn deref(&self) -> &Kind {
        match self {
            KindAs::Today(k) => k,
            KindAs::Read(k) => k,
        }
    }
}

impl KindAs<'_> {
    pub fn into_owned(self) -> Kind {
        match self {
            KindAs::Today(k) => k.clone(),
            KindAs::Read(k) => std::sync::Arc::unwrap_or_clone(k),
        }
    }
}

impl Defs {
    /// KIND as a ticket's own TEXT has it (its form, result and raises), not as today's files do: today's where TEXT
    /// is today's text, else TEXT read, with today's file and place (N-101 p460, rider 2: a reload may change a kind
    /// none of whose tickets is open, and one closed keeps the form it was opened with); today's when TEXT does not
    /// read. TEXT is read once for these defs (`KindsRead`).
    pub fn kind_as<'a>(&'a self, kind: &str, text: &str) -> Option<KindAs<'a>> {
        let today = self.kinds.get(kind);
        if today.is_some_and(|k| k.text == text) || text.is_empty() {
            return today.map(KindAs::Today);
        }
        let mut read = self.read.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let kept = match read.get(kind).and_then(|texts| texts.get(text)) {
            Some(k) => k.clone(),
            None => {
                let k = syntax::parse_kind(kind, text, 1, &self.named).ok().map(|mut k| {
                    if let Some(t) = today {
                        (k.file, k.line, k.letter, k.width) = (t.file.clone(), t.line, t.letter, t.width);
                    }
                    std::sync::Arc::new(k)
                });
                read.entry(kind.to_string()).or_default().insert(text.to_string(), k.clone());
                k
            }
        };
        match kept {
            Some(k) => Some(KindAs::Read(k)),
            None => today.map(KindAs::Today),
        }
    }

    /// Parse and check the kind files FILES (path and text, in order: a later kind of the same name replaces an
    /// earlier one, so a project's kinds come after the system's). Gives the defs and why each refused kind was
    /// refused, with file and line.
    pub fn build(files: &[(String, String)], calls: BTreeMap<String, CallSig>, named: Named, first: u64) -> (Defs, Vec<String>) {
        Defs::build_with(files, calls, named, first, false)
    }

    /// As `build`, for a project whose policy declares duties (DUTIES; `Defs.duties`).
    pub fn build_with(files: &[(String, String)], calls: BTreeMap<String, CallSig>, named: Named, first: u64, duties: bool) -> (Defs, Vec<String>) {
        let mut errors = Vec::new();
        let mut events = syntax::Events::new();
        for (file, text) in files {
            match syntax::parse_events(file, text, &named) {
                Ok(es) => {
                    for (n, fs) in es {
                        if events.insert(n.clone(), fs).is_some() {
                            errors.push(format!("{file}: event {n} is declared twice"));
                        }
                    }
                }
                Err(e) => errors.push(e),
            }
        }
        let mut kinds: BTreeMap<String, Kind> = BTreeMap::new();
        for (file, text) in files {
            let (ks, errs) = syntax::parse_kinds_each(file, text, &named);
            errors.extend(errs);
            for k in ks {
                kinds.insert(k.name.clone(), k);
            }
        }
        let (kinds, errs) = check::check_kinds_with(kinds.into_values().collect(), &calls, &events, Some(&named), duties);
        errors.extend(errs);
        let mut bindings = Vec::new();
        for (file, text) in files {
            match syntax::parse_bindings(file, text) {
                Ok(bs) => {
                    for b in bs {
                        let mut b = match b {
                            Ok(b) => b,
                            Err(e) => {
                                errors.push(e);
                                continue;
                            }
                        };
                        match check::check_binding(&mut b, &calls, &kinds, &events) {
                            Ok(()) => bindings.push(b),
                            Err(e) => errors.push(format!("{}:{}: binding refused: {e}", b.file, b.line)),
                        }
                    }
                }
                Err(e) => errors.push(e),
            }
        }
        let agents_given = crate::given::agents_given(&kinds, &calls);
        let names = crate::given::names_given(&kinds, &calls);
        (Defs { kinds, calls, named, bindings, events, first, block_ask: false, duties, call_docs: BTreeMap::new(), agents_given, names, read: KindsRead::default() }, errors)
    }
}


pub type Env = BTreeMap<String, Value>;

/// Who holds a `hold` whose role nobody fills.
pub const NOBODY: &str = "nobody";
/// Who answers a `decide` first: the policy's rules (N-101).
pub const POLICY: &str = "policy";
/// A decision's argument named `not` (N-101 p162, p166): a list of tickets (one ticket counts as a list of one); an
/// element of a list argument equal to one of them is left out of that list for the decision: from its `X-count` and
/// from the single element a field takes (`decide_facts`, `decided_fields`).
pub const NOT: &str = "not";
/// The values a decision's `not` in ARGS names: its elements, or the one value, or none.
fn nots(args: &Value) -> Vec<Value> {
    match args.get(NOT) {
        Some(Value::Array(a)) => a.clone(),
        Some(Value::Null) | None => Vec::new(),
        Some(v) => vec![v.clone()],
    }
}

/// A step as it is owed.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Owed {
    pub step: u32,
    pub call: String,
    /// The arguments by the call's parameter names.
    pub args: Value,
    pub blind: bool,
    pub head: Type,
    /// A `hold`: owed, never delivered, reminded or expired.
    pub hold: bool,
    /// The variables in scope at the step, their types and values: where a withdrawal's program runs.
    pub scope: BTreeMap<String, Type>,
    pub env: Env,
    /// An inline call's text, rendered in the step's scope.
    pub template: Option<String>,
    /// The references its template marks (`{{@K}}`), as kept: texts held at a place.
    pub refs: Vec<Value>,
    /// The owner's text for the prompt, edited at its halt (slow mode): sent in place of what the call renders.
    pub prompt: Option<String>,
    /// The owner's template for it, edited once (`Line::Template`): rendered in place of the call's.
    pub edit: Option<String>,
    /// What answers it by itself, a system event's line (its step's `on`, [`syntax::On`]), read in ENV; or what bears
    /// out its worker's answer (its `until`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on: Vec<syntax::On>,
    /// Its worker's answer that waits for a line to bear it out, by its `until` ([`UntilAnswer`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unborne: Option<UntilAnswer>,
}

/// An answer to a step with an `until` ([`syntax::On`] with no answer) that no line has borne out yet, as it was given:
/// kept on the step ([`Owed::unborne`]) until a line does ([`Machine::until_answer`]), or its worker answers again.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UntilAnswer {
    pub by: String,
    pub head: Value,
    pub text: Text,
    pub at: At,
}

impl Owed {
    /// Its `until`s that concern answer HEAD: those whose `where` reads only fields HEAD has.
    pub fn untils<'a>(&'a self, head: &'a Value) -> impl Iterator<Item = &'a syntax::On> + 'a {
        self.on.iter().filter(move |o| o.answer.is_none() && o.fields.iter().all(|f| head.get(f).is_some()))
    }
}

/// A step a ticket waits on, as its delivery would carry it ([`Machine::owed_give`]): its number, the agent it waits
/// on, its call and arguments (readable), its answer's type, an inline call's text, the owner's template for it
/// (`Owed::edit`), his text at its halt (`Owed::prompt`), and the texts its template marks.
#[derive(Clone, Debug, PartialEq)]
pub struct OwedGive {
    pub step: u32,
    pub agent: String,
    pub call: String,
    pub args: Value,
    pub head: Type,
    pub template: Option<String>,
    pub edit: Option<String>,
    pub prompt: Option<String>,
    pub refs: Vec<Text>,
}

/// The owner's once-edit of a call's template on a ticket (`Line::Template`, at AT): the next step of CALL on T (not
/// TREE), once (USED after), or every later step of CALL on T and the tickets under it (TREE), the latest nearest first.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TemplateEdit {
    pub t: u64,
    pub call: String,
    pub tree: bool,
    pub text: String,
    pub used: bool,
    pub at: At,
}

/// A moment of a slow ticket held for the owner (plan/layers.md, Part I §5, 3): N, its number on the ticket, when it
/// halted, and what it holds back. Only the owner's approval lets it through; an edit changes what goes.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Halt {
    pub n: u32,
    pub at: At,
    pub what: Halting,
    /// Its place among all the halts made, on every ticket: what turning slow off lets through in order.
    pub seq: u64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Halting {
    /// Step STEP is owed and not yet given to its worker (the one the ticket awaits); START, the call a fresh agent is
    /// started for, when the step's worker is one (it is started once approved); FROM, the agent it is a fork of.
    Give { step: u32, start: Option<String>, from: Option<String> },
    /// The answer to step STEP, formed (or given explicitly) by BY: HEAD, from TEXT. Accepted once approved.
    Take { step: u32, by: String, head: Value, text: Text },
    /// Post POST of the ticket, and WITH, the posts told with it (his burst's, one tell: E1b): given to nobody until
    /// approved (`Post.halted`), all at once, one card for one tell (docket-core4 N-1446 p184, proto-reader p302).
    /// CHOSE: who chose whom it wakes, when that was an answer and not a rule (the watcher's `route` for the owner's
    /// words, the owner N-94).
    Deliver { post: u32, with: Vec<u32>, edited: bool, chose: Option<String> },
    /// The ticket was opened and its program, PATTERN, has not started: it runs, on its form, once approved.
    Open { pattern: Prog },
    /// A policy answer: QUESTION (fills, holds, expired) for step STEP, ANSWER and the rule behind it (PROVENANCE).
    /// Used once approved.
    Decide { step: u32, question: String, answer: Value, provenance: Value },
    /// A delivery that starts TO's turn and is none of the above (paper-research N-89.p2, 2): the closing notice, a
    /// step's reminder, a withdrawal, a refusal, words given. ID, the delivery's (`N-3.c`); WHAT, as it would be given.
    /// Delivered once approved.
    Notice { id: String, to: String, what: What },
}

impl Halting {
    pub fn name(&self) -> &'static str {
        match self {
            Halting::Give { .. } => "give",
            Halting::Take { .. } => "take",
            Halting::Deliver { .. } => "deliver",
            Halting::Open { .. } => "open",
            Halting::Decide { .. } => "decide",
            Halting::Notice { .. } => "notice",
        }
    }
}

/// A ticket's status (semantics.tex's σ, with `Asking`: waiting for policy, which coordd asks off its lock).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Status {
    Asking { owed: Owed, role: String },
    Awaits {
        agent: String,
        owed: Owed,
        since: At,
        /// The session took the step (Take); a turn's end may answer it from then on.
        taken: bool,
        /// Answers that did not type; the third raises Malformed.
        refusals: u32,
        /// The last refusal, and whether it was delivered.
        refused: Option<(String, bool)>,
        /// Remind: none yet, due (not yet delivered), given.
        reminded: Option<bool>,
        /// Back (`sN.back`): the step taken, its agent moved into the step's layer since (back from a `coord go`, or
        /// given it while elsewhere): the step handed to it again there, the arrival first (paper-research N-101
        /// p629). None: not moved so; due; given.
        back: Option<bool>,
        /// A turn's output being formed into the head.
        forming: Option<Loc>,
    },
    /// Waiting for children: one opened (its result is the value), or the parts of a `par` (the tuple of theirs).
    Waits { children: Vec<u64>, par: bool },
    Done(Value),
    Failed(Raised),
    /// At a hole (`...`): open, owing nothing, until an edit gives the rest of its program, checked in SCOPE and run
    /// with ENV.
    Rests { env: Env, scope: BTreeMap<String, Type> },
    /// Stopped at RAISED, a failure that nothing above it handles (paper-research N-101 p934): open, its stack as it was,
    /// every ticket above it still waiting. TO is given the step of the hidden ticket BY; its answer Again runs NODE
    /// again in ENV, where it stopped; Ended unwinds RAISED as a raise does.
    Stopped { raised: Raised, node: Prog, env: Env, to: String, by: u64 },
}

/// Words a program gave an agent as they are (`give`): to whom, the words, and whether it has been given them.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Gift {
    pub to: String,
    pub text: Text,
    pub given: bool,
}

/// The rest of a ticket's pattern, one frame at a time (semantics.tex's K).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Frame {
    Bind { x: Bound, rest: Prog, env: Env },
    /// An `iterate`: rounds LEFT after the one running, the last measure, the body and where it runs.
    Loop { x: String, left: u32, measure: Option<i64>, body: Prog, env: Env },
    /// A `for`: the elements after the one running, the rounds' results so far, the body and where it runs.
    Each { x: String, rest: Vec<Value>, done: Vec<Value>, body: Prog, env: Env },
    /// A `layer`: ID (`E-3.l1`), on ON (a commit or branch; none, the root's head). While it is on a ticket's frames
    /// the layer is active (`Machine::active_layers`).
    Layer { id: String, on: Option<String> },
    /// A `try` around what runs: its arms, and where they run.
    Try { arms: Vec<crate::syntax::Handler>, env: Env },
    /// A `with`: the ticket's context as it was before, put back when its body ends.
    With { ctx: Value },
    /// A `let agent`: the agent it made, ended when its body ends.
    Scope { agent: String },
    /// A `during { p } when ev(x) do { q }` (N-101): WHEN is the handler, run in a hidden ticket under this one, in
    /// ENV (what was in scope where `during` stands, typed SCOPE) with `this` and X, once per event while the body
    /// runs. One look at a time: BUSY is the hidden ticket of the look running; an event that comes meanwhile is
    /// kept (MISSED, the latest one, and who raised it) and the next look runs on it when this one closes, so what happens during a
    /// look is seen by the next (as the hole's minder is asked). When the body ends, a look still running is
    /// cancelled. ASKS: questions to the owner waiting for a look before they are asked (ask_minded.rs), each looked at
    /// in turn, before what was MISSED.
    During { whens: Vec<syntax::When>, scope: BTreeMap<String, Type>, env: Env, busy: Option<u64>, missed: Option<(String, Value, String)>, asks: Vec<HeldAsk> },
    /// A `gather` waiting at the owner's step of `gather` (E1b): the step's answer goes on as his burst, the event's
    /// value over every post the handler holds by then (`Machine::owner_value`).
    Gather,
}

/// Post N, taken back by BY at AT after an agent had it (0.2 B8, N-1396): TO, the agents that had it on its ticket,
/// each told once by a `pN.taken_back` delivery; TOLD, those given that delivery.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Struck {
    pub n: u32,
    pub at: At,
    pub by: String,
    pub to: Vec<String>,
    pub told: Vec<String>,
}

/// His post N, held under his question about it (`Ticket::asked`) and then told, for TO, an agent the tell chose,
/// whatever TO's read mark: TO may have been shown the posts after it meanwhile, so it is handed to TO on its own
/// (`pN.told`, [`TOLD`]), once, GIVEN then, and `Ticket::unread` never shows it TO: one path, no race with a `given`
/// still to come, no double (consult-3 N-1446 p576, proto-lead p576 on p569's (c2)). Derived from the lines: the tell
/// of an asked post, and the `given` line of `pN.told`. Or his post an agent's post read though not handed to it,
/// before the era [`ERA_HIS_HANDED`], told late by that era's line; from the era any giving to TO that hands N gives it
/// (`Ticket::hands`): one a hand-over on its way carried when the line told it, whose `given` comes after it, and
/// which, not taken, leaves it due as `pN.told` (proto-reader N-1479 p204, consult-3 p212). A giving that does not show
/// it leaves it out (`Ticket::left_out`), so no other hands it (consult-3 p262).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToldLate {
    pub n: u32,
    pub to: String,
    pub given: bool,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GivenRec {
    pub to: String,
    pub delivery: String,
    pub upto: u32,
    pub at: At,
    pub place: Option<Loc>,
    /// The agent's read mark on the ticket before this delivery raised it (what `unread` puts back).
    pub before: u32,
    /// Made due again by `unread`: handed, never read.
    pub unread: bool,
    /// When the turn it was given in ended (`Line::TurnEnded`): read, as far as coordd knows a turn's end (part 1 of
    /// the message runtime makes that the session's own fact). Set from the lines, so a replay has it; no line of its
    /// own (part 4, N-818 post 32: the line under the owner's post).
    pub read: Option<At>,
    /// When the text entered the model's input, as the session wrote it (0.2 A1: delivered, not queued; the `event`
    /// DELIVERED coordd records once for this giving, matched by its place). None until then, and for a program agent.
    pub delivered: Option<At>,
    /// Made due again because the process it was given to ended before its text entered (`DUE_AGAIN` with `exited`):
    /// not counted toward "given twice, not read".
    pub lost: bool,
    /// His posts once held under his question (`Ticket::was_asked`), up to UPTO, that the agent had not been handed
    /// when this was handed over: its mark passed them, and this did not hand them (`Ticket::hands`). The `given`
    /// line's, as coordd worked them out at the hand-over (`Machine::left_out`; proto-lead N-1446 p631), `[]` too; when
    /// it says nothing, worked out at `Given` from the lines before it (`Ticket::skipped_now`), so a replay has it either way
    /// (proto-reader N-1446 p595, proto-lead p595).
    pub skipped: BTreeSet<u32>,
    /// Made due again because it carried a post of the owner's into a turn that ended without answering it
    /// (`Machine::his_at_end`): handed again once, alone, and said so (`handed_before`); never read unanswered.
    pub left: bool,
}

impl GivenRec {
    /// Made due again since, by `unread` or for leaving his post unanswered in its turn (`left`): handed, and not where
    /// its delivery is now; a giving of the same delivery after it is a new one.
    pub fn again(&self) -> bool {
        self.unread || self.left
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Ticket {
    pub t: u64,
    pub kind: String,
    pub parent: Option<u64>,
    pub under: Option<u64>,
    pub opener: Option<String>,
    /// What it was opened with; Null for a closed run, whose form is `form_text`.
    pub form: Value,
    /// A closed run's form (a handler's or a stop's, `is_run`) as its JSON text, `form` then Null. What a run was given
    /// is read again only should it run again (`restart`, Undo: as values again) or be shown (`Machine::view`), and as
    /// text it takes about a tenth of what it takes as values: delivery's `free` and `arrived` runs held 3.5 GB of
    /// research's 9.7 so (paper-research N-1446 p1900, p1960). `form_of` and `form_field` read it either way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form_text: Option<String>,
    pub ctx: Value,
    pub program: String,
    /// The calls PROGRAM runs against, as its line recorded them (calls on the record); a call not among them is
    /// looked up above it, then in today's (`call_of`).
    pub calls: BTreeMap<String, CallSig>,
    pub posts: Vec<Post>,
    pub steps: u32,
    pub bound: u64,
    pub status: Status,
    pub k: Vec<Frame>,
    /// For each agent, the last post it was shown (semantics.tex's μ).
    pub mu: BTreeMap<String, u32>,
    pub given: Vec<GivenRec>,
    /// The closing notices given, as `AGENT NOTICE` (`orch c2`): each close has its own.
    pub closed_given: BTreeSet<String>,
    /// (AGENT, DELIVERY) handed twice and read neither time (`Line::Unread` with the delivery unread already): its
    /// second giving is "not read" in the view's given rows. Made by the replay, as every field here; nothing in the
    /// machine reads it (paper-research N-412.p1844, 2).
    pub unread_twice: BTreeSet<(String, String)>,
    /// The notices the owner let through (slow mode), as `AGENT DELIVERY`: not halted again when given back.
    pub let_through: BTreeSet<String>,
    /// The notices the owner refused (slow mode, `Machine::reject`), as `AGENT DELIVERY`: never given, nor halted again.
    #[serde(default)]
    pub refused_notices: BTreeSet<String>,
    /// The layers it has entered (`E-3.l1`, `E-3.l2`, ...).
    pub layers: u32,
    /// Whom policy's `notify` told of the close, beside the opener.
    pub told_closed: BTreeSet<String>,
    /// Of those, the ones policy said inform: their notice starts no turn (Delivery.inform).
    pub informed: BTreeSet<String>,
    /// A question an agent opened in a turn: the tickets that turn was handed (what it asks about). Those are not
    /// quiet while it is open, nor until its answer has been handed back (paper-research m1225 b).
    pub about: Vec<u64>,
    pub children: Vec<u64>,
    pub opened_at: At,
    pub closed_at: Option<At>,
    /// Replay made other outputs than the log has: shown, never papered over.
    pub broken: Option<String>,
    /// Scoped agents made for its steps (`fresh`), counted; those alive.
    pub scoped: u32,
    pub live: Vec<String>,
    /// Agents made by `let agent`, alive while their scope runs (an answer does not end them, as it does a fresh one).
    pub kept: Vec<String>,
    /// Each step given, by number: its call and who holds it (the last holder when it passed on). Rebuilt by replay;
    /// what a step's label and a fresh agent's are read from (`label`).
    pub step_calls: BTreeMap<u32, (String, String)>,
    /// Steps withdrawn (Silent, Cancel): agent, step, call, whether the agent was told, and by whom and why when an
    /// agent did it.
    pub withdrawn: Vec<(String, u32, String, bool, Option<(String, String)>)>,
    /// Posts taken back after an agent had them (`TAKEN_BACK`): drawn struck through for the owner; the post itself is
    /// `taken_back`, owed by nobody and handed to nobody again.
    pub struck: Vec<Struck>,
    /// His posts on it held under his own question about them (a `Line::Step` with `asked`): untold and unshown,
    /// they no longer hold back the posts after them (`seen_upto`), so what he chose goes at once (proto-lead N-1446
    /// p569, (c2)); each until his answer tells, drops or releases it.
    pub asked: BTreeSet<u32>,
    /// His posts told while they were `asked`, for each agent the tell chose (`ToldLate`).
    pub told_late: Vec<ToldLate>,
    /// His posts ever `asked`, kept once his answer tells, drops or releases them: an agent's mark may have passed one
    /// it was not handed (`GivenRec::skipped`).
    pub was_asked: BTreeSet<u32>,
    /// The owner's steps that went on by themselves (`Line::Lapsed`): step, seconds waited, and why (`time`, `closed`).
    pub lapsed: Vec<(u32, u64, String)>,
    /// Who said they work on it (`started` lines), in order.
    pub started: Vec<String>,
    /// Its latest tie in the outline (`Line::Tied`): under which thread, at which of its posts; Some(None) untied.
    /// None: never tied, it stands where it was opened (`Machine::outline_parent`).
    pub tie: Option<Option<(u64, u32)>>,
    /// Its title (`Line::Titled`): its label from then on.
    pub titled: Option<String>,
    /// How it ended (`Line::Ended`, `coord end`): done, dropped or kept, why, by whom and when.
    pub ended: Option<crate::view::ViewEnded>,
    /// Read, no reply (paper-research N-19): each agent whose turn, given only posts here up to post N, ended with
    /// nothing to post (Nothing), with that N. Kept from the TurnEnded lines, so a restart has it.
    pub quiet: Vec<(String, u32)>,
    /// What it owes when it closes (`goal`): the policy's acts that must hold (Owed).
    pub goals: Vec<String>,
    /// Its program returned this value, and policy is asked whether its goals hold.
    pub holding: Option<Value>,
    /// How often it has closed: a ticket repaired or rekinded closes again, and each closing notice has its own id
    /// (`c`, then `c2`, ...).
    pub closes: u32,
    /// An `any n of` it waits for: N, the results so far in the order they came, and the first failure.
    pub any: Option<(u32, Vec<Value>, Option<Raised>)>,
    /// A branch of a `par` or an `any` (kind `part`): the type it returns.
    pub branch: Option<Type>,
    /// The last `open`, `par` or `any` it reached: the scope and the type of what it waits for there, as the type
    /// check found them, and the values in scope. Read while it waits on the tickets below it (an edit then).
    pub waiting: Option<(syntax::Wait, Env)>,
    /// Its recorded program no longer checks against today's calls (found on replay) and the log leaves it open: its
    /// next step fails with NoCall, and Repair gives it a program that does.
    pub unchecked: bool,
    /// The kinds its recorded program opens that it checked against as they were, not as today's files have them
    /// (found on replay, `recorded_program_as`): a reload changed their form, result or raises since.
    pub loose: BTreeSet<String>,
    /// The last reload whose new text it took (live edits, `retext`).
    pub reloaded: Option<crate::view::Reloaded>,
    /// The words its program gave as they are (`give`), in order: delivery `T.wN` is the Nth.
    pub gives: Vec<Gift>,
    /// The turns that answered its gifts: the agent and its answer's place (where a one-off's answer is, so that a
    /// thread made of it later can carry it).
    pub answers: Vec<(String, Loc)>,
    /// Whose act raised the event that opened it (the event's `from`: the owner, for the owner's message). Shown as
    /// its opener; not its opener in the machine, so its close tells nobody (a one-off would tell the owner each time).
    pub raised_by: Option<String>,
    /// The binding that opened it, as written: `Fired.binding`, which every `Event` line keeps, so a replay fills it
    /// for old tickets too. The clock (or event) of the project's it ran for (0.2 line 26); none if opened otherwise.
    pub binding: Option<String>,
    /// Its kind's `when` handlers (its own and its base's), as when it was opened or rekinded.
    pub handlers: Vec<syntax::When>,
    /// What had happened on it (`Ticket::mark`) when its hole was last asked, or when it reached a hole of its kind's
    /// own: the hole is asked again only once that has moved (paper-research m1164, E).
    pub asked_at: Option<(usize, usize, usize)>,
    /// The hole being shaped (its handler running): its variables and their types, where `continue` goes on.
    pub shaping: Option<(Env, BTreeMap<String, Type>)>,
    /// The minder's answers at its holes, in order.
    pub shaped: Vec<Shaped>,
    /// The posts whose asks the owner said there is nothing to answer, each with when.
    pub nothing: BTreeMap<u32, At>,
    /// The posts whose asks were withdrawn (`Line::AsksWithdrawn`), each with when, by whom and why.
    pub asks_withdrawn: BTreeMap<u32, (At, String, String)>,
    /// Of an agent's posts, the last post of the ticket it had been handed when its words were written: a reply of a
    /// turn, what that turn held (not what its turn end calls later); a post by hand, what the agent had been handed
    /// there when it posted (paper-research N-27 post 43). A post not here answers every post before it. Derived from
    /// the lines (given, turn_ended, replied, post); no field of its own.
    pub answered_upto: BTreeMap<u32, u32>,
    /// Of an agent's posts, the posts up to its `answered_upto` that were kept from it when it was written (0.2,
    /// `keep`): it does not answer them. Derived from the lines, as `answered_upto` is.
    pub not_answered: BTreeMap<u32, BTreeSet<u32>>,
    /// Posts of this ticket answered by a post on another ticket (`Line::AnswersAlso`): post, by whom, the answer's id.
    pub answered_from: Vec<(u32, String, String)>,
    /// Posts of this ticket a ticket handed in for at its open, its form's `for` naming the post or an item whose note
    /// it is (`Machine::handed_in_for`; docket-edge5 N-1446 p1494, 8): post, the ticket's opener, its id. The ASKED
    /// ledger's third answer (`owed_with`), read by it alone. Derived from the lines, as `answered_from` is.
    pub handed_in: Vec<(u32, String, String)>,
    /// The owner's posts sent to wait for the turn's end of whom they tell (`Line::Waits`): no own post of theirs
    /// reads past one before it is given them (`read_own`).
    pub waits: BTreeSet<u32>,
    /// An intake ticket whose message was sent to wait (`Line::Waits` without a post): the owner's post its program
    /// makes waits.
    pub waits_message: bool,
    /// The policy's hold on the words his message opened it with (`owner_message`'s `hold`; I-1474, the owner, N-1445
    /// p9: "hold them for 15s by default"): `thread.kind`'s `gather` before `intake` takes it and holds the burst there
    /// (N-1104 p935/p936); an older thread's recorded program without `gather` leaves it for `Prog::Post` to hold its
    /// first post as it ran. Read from its line, so a replay holds as it ran; none: not held.
    pub words_hold: Option<u64>,
    /// Whether `thread.kind`'s `gather` before `intake` already held its opening burst: when `Prog::Post` then posts
    /// the burst, it joins a burst already gathering on TARGET (`Place(target)`) and otherwise goes without a second hold.
    pub words_gathered: bool,
    /// The ask it stands for: a `when ask` handler's run, under the ticket the asker held a step of.
    pub ask: Option<AskOf>,
    /// A question to the owner this look holds until it closes (ask_minded.rs).
    pub held_ask: Option<HeldAsk>,
    /// The owner's words being formed against the step it awaits of the owner (paper-research m1185, B): posted on
    /// ticket ON, naming NAMES, if they do not form.
    pub owner_words: Option<(u64, Text, Vec<String>, Vec<String>, Vec<String>)>,
    /// The ticket event whose handler it runs, when it is a ticket of its own for that (`owner_post`): plumbing, in no
    /// list the owner, an agent or a model is shown (paper-research m1304), numbered apart (HIDDEN).
    pub handling: Option<String>,
    /// The `when` it runs, when it runs one (`handler_ticket`): the kind it was written in and its line, as they were
    /// when it opened, so a run from before a reload keeps the line it ran (`view::ran`). A delivery's desk run and a
    /// stop run no `when` of a kind and have none.
    pub when: Option<(String, usize)>,
    /// The owner's post its `when owner_post` handler holds.
    pub post_of: Option<PostOf>,
    /// The owner's star: it orders his list and does nothing else (design-two-systems §16, 3).
    pub starred: bool,
    /// Every star on it and on its posts: which post (none: the ticket), by whom, when (the owner N-234.p3).
    pub stars: Vec<crate::view::ViewStar>,
    /// The owner archived it (`Line::Archive`): no longer of importance to him. Nothing at or under it is handed while
    /// it is (`archived_by`); it is not an end.
    pub archived: bool,
    /// Who is told once that the owner archived it (policy's `notify` and `inform`, event "archived"): each agent,
    /// whether quietly (informed: at its next turn, starting none), and whether it was handed the notice.
    pub told_archived: BTreeMap<String, (bool, bool)>,
    /// Why step STEP's reminder comes: AGENT's last answer read as none of its outcomes, in the forming call's sentence
    /// (paper-research N-101 p561), or the turn it was put into ended without its answer ([`RemindWhy::Left`]). Gone
    /// once the step is given again for another cause (an ask's answer).
    pub remind_why: Option<(u32, RemindWhy)>,
    /// The answers to step STEP that were read as none of its outcomes, in order, by whoever held it: what the one it
    /// is passed up to is shown (paper-research N-101 p757).
    pub not_read: Vec<(u32, crate::line::NotRead)>,
    /// What its program has done so far, node by node: what a picture of it is drawn from (§16, 9). Rebuilt by replay.
    pub trace: Trace,
    /// The owner switched it to slow (`Machine::slow`: it and every ticket below it halt).
    pub slow: bool,
    /// Its moments halted for the owner, oldest first; HALTED, how many it has had (the next one's number).
    pub halts: Vec<Halt>,
    pub halted: u32,
    /// When a halt of it last went: let through or dropped by the owner (`approve`), or a notice no longer due. What
    /// held the outline's line above it open for him, cleared then (`view::stopped_owing`). Rebuilt by replay.
    pub halt_gone: Option<At>,
    /// Who answered its latest step accepted (rebuilt by replay): for a handler, whether a rule or an answer chose
    /// what it does.
    pub last_by: Option<String>,
    /// A part of a `par` in order that has not started: the parts it stands on, and what it runs once they have
    /// reached the state (N-101, parts by dependency).
    pub held: Option<Held>,
    /// An agent's answer to its step put to the policy (`violated.rs`): taken, or raised as Violated, once answered.
    pub pending: Option<violated::Pending>,
    /// The node it ran last, the environment it ran in and how many steps it had given before it: where it runs again
    /// from when it stops there (`stop`). A `layer`'s node is kept by its id as well, for a NoLayer raised at it.
    pub last_node: Option<(Prog, Env, u32)>,
    pub layer_nodes: BTreeMap<String, (Prog, Env, u32)>,
    /// The ticket that stopped, for the hidden ticket whose step asks what it does next (`stop`).
    pub stop_of: Option<u64>,
    /// Where it has stopped (`stop_place`): a second stop at the same place goes to the owner.
    pub stops: Vec<String>,
    /// The raise it is given comes from a ticket below it, which did not stop: it unwinds, never stops.
    pub passing: bool,
}

/// A node of a ticket's program: the program it is in (an index of `Trace.parts`), its line there, and the node in one
/// line (`print::node`: two nodes on one line differ in their words).
pub type TraceKey = (usize, usize, String);

/// What a ticket's program has done (design-two-systems §16, 9): one visit per node it has reached, the latest (a
/// node run again in a loop keeps its last visit), so it is bounded by the programs it ran.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Trace {
    /// The programs it ran: its kind's first; then each that filled a hole or took a step's place, where it did.
    pub parts: Vec<TracePart>,
    /// The part running now; none while a handler of its own runs in it (the hole's, asking the minder).
    pub src: Option<usize>,
    pub visits: BTreeMap<TraceKey, Visit>,
    /// The last node reached that does something (not a `match` or a block): where it stands while it waits.
    pub now: Option<TraceKey>,
    /// That node, until what it gives back comes.
    pub pending: Option<TraceKey>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TracePart {
    /// 0: the kind's program, or one an edit gave (then BY says whose); N: the minder's answer N at a hole.
    pub source: u32,
    pub by: Option<String>,
    pub prog: Prog,
    /// The node it stands in for (the hole it filled, the step it replaced); none for the kind's.
    pub at: Option<TraceKey>,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Visit {
    /// What came back (none yet, or nothing but done).
    pub value: Option<Value>,
    /// A match's arm taken.
    pub arm: Option<usize>,
    /// The tickets it opened (`open`; a `par`'s or an `any`'s branches, in order).
    pub tickets: Vec<u64>,
    /// A step's worker; whom a `tell` or a `post` named.
    pub who: Option<String>,
    /// When it was reached (on replay, the time of the log line that got it there).
    pub at: Option<At>,
    /// A step's numbers on the ticket, one per time it was reached (a step in a loop: each round), kept across visits.
    pub steps: Vec<u32>,
    /// A step's answer type as it ran (its call's head then, or the inline `-> T`).
    pub ty: Option<Type>,
    /// A step not given to its agent for its model (`Line::ModelRefused`): the agent, why, and when; until the step is
    /// reached again.
    pub refused: Option<(String, String, At)>,
}

impl Trace {
    /// The part that is its kind's text: the first, or the one the last reload gave (`retext.rs`).
    pub fn kind_part(&self) -> usize {
        self.parts.iter().rposition(|p| p.source == 0 && p.at.is_none()).unwrap_or(0)
    }

    pub fn new(p: Prog) -> Trace {
        Trace { parts: vec![TracePart { source: 0, by: None, prog: p, at: None }], src: Some(0), visits: BTreeMap::new(), now: None, pending: None }
    }

    /// P runs in place of the node it stands at now: from the minder's answer SOURCE, or an edit of BY's.
    fn fill(&mut self, source: u32, by: Option<String>, p: &Prog) {
        self.parts.push(TracePart { source, by, prog: p.clone(), at: self.now.clone() });
        self.src = Some(self.parts.len() - 1);
        self.pending = None;
    }

    /// Back in the part of the node it stands at (a handler of its own has ended there).
    fn back(&mut self) {
        self.src = self.now.as_ref().map(|k| k.0).or(self.src);
        self.pending = None;
    }

    fn came_back(&mut self, v: Value) {
        if self.src.is_none() {
            return;
        }
        if let Some(k) = self.pending.take() {
            if let Some(n) = self.visits.get_mut(&k) {
                n.value = (v != Type::unit_value()).then_some(v);
            }
        }
    }
}

/// The owner's posts a `when owner_post` handler holds: post N of ticket T, and WITH, his posts on T that joined it
/// while it gathered his burst (`gather`, E1b), in his order; DONE once each is told, dropped or released. HOLD: the
/// seconds the policy held N for (its `post` line's), none when it was not held, and `gather` then waits for nothing.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PostOf {
    pub t: u64,
    pub n: u32,
    pub done: bool,
    pub with: Vec<u32>,
    pub hold: Option<u64>,
}

impl PostOf {
    /// Every post it holds, N first, in his order.
    pub fn posts(&self) -> Vec<u32> {
        std::iter::once(self.n).chain(self.with.iter().copied()).collect()
    }
    /// Whether M is one of its posts.
    pub fn holds(&self, m: u32) -> bool {
        self.n == m || self.with.contains(&m)
    }
}

/// The kind of a ticket that runs an `owner_post` handler.
pub const OWNER_POST: &str = "owner_post";
/// The event coordd sends at the first start of an engine that hands the owner's held posts (paper-research
/// N-807.p78, 5; p81, 3): one per post held then under a question to him (`clarify`), its value `{t, n}`, handed to
/// every agent on its thread as his next words would hand it (`hand_unanswered`); one with no post when none is held.
/// The record's lines say it was done, so a restart or a replay hands nothing twice.
pub const HELD_HANDED: &str = "held_handed";

/// The way back's after-replay step (proto-lead N-1446 p431, 2): sent by coordd at its first live moment after a
/// replay, never inside one, one per post of his that no line told and whose handler this release did not load
/// (`Machine::untold_posts`), its value `{t, n}` and `hold`, the policy's hold when coordd sent it (`owner_hold`, E1b),
/// as his typed post's line has it. The post goes to this release's own `owner_post` as if it had just arrived
/// (proto-lead p819, (b)): held, it joins a burst a handler of its ticket gathers, or opens one that gathers, so posts
/// he typed close together, which a later release held as one burst, are decided once here too. A line with no hold
/// (57's, whose untold posts went each alone, p803 (a); or one sent under a policy with none) holds nothing. Its
/// watcher chooses whom it is for, and on a slowed thread this release's own card holds it. A line of the machine's
/// own, as HELD_HANDED is: replayed, it runs again and holds as it ran, so coordd sends none for a post it has run.
pub const UNTOLD: &str = "untold";

/// The event coordd writes to begin a record's era ([`Machine::era`]), its value `{"era": N}`: a rule a replay derives
/// with no line of its own applies from that line on, so the record's lines before it replay as they ran (consult-3,
/// N-807 post 118). Era [`ERA_CLAIMS`]: the handed agent's turn-end reply is its claim on the owner's `@all`. coordd
/// writes it only in a record that holds an `@all` of his, at a start or just before his first: a record with none
/// never gets it. Era [`ERA_NAMES`]: an agent named like a ticket's number is an agent. coordd writes it only in a record
/// where such a name is or is about to be. Era [`ERA_PUT_IN`]: a post put into a turn is read at its end, his once
/// answered, and a cut turn leaves his posts due again. coordd writes it only in a record that holds a post put in, at
/// a start, or just before the next giving into a turn or cut holding his post. Era [`ERA_CLOSES`]: delivery's push
/// takes an ask as answered by the ledger's rule. The machine writes that one itself (`Machine::apply`), just after the
/// first post that goes otherwise by it. Era [`ERA_HIS_HANDED`]: his post is read by its agent's own post only once
/// handed to it. coordd writes it only in a record where an agent's post read his post not handed to it, on the line
/// after that post, naming them (`passed`), and the line tells them to their agents late, each given by its `.told` or
/// by any giving that hands it. The eras add up: a record in era 2 is in era 1 too. An engine from before reads it as
/// an event nothing takes.
pub const ERA: &str = "era";

/// The era from which the turn-end reply of the agent the owner's `@all` is handed to is its claim ([`ERA`]): before
/// it, such a reply answers nothing, as it was drawn when it was made.
pub const ERA_CLAIMS: u32 = 1;

/// The era from which a name is a ticket posting on itself only when it is that ticket's id exactly ([`ERA`];
/// consult-3, N-807 post 118). Before it, every name `parse_ticket` reads was one: `consult-3`, `accounts-2` and
/// `here-1` read as tickets 3, 2 and 1, and those agents were left out of a thread's `@all`, the owner's posts handed
/// at the install (`others_on`) and the watcher's `when agent_post` (`agent_landed`).
pub const ERA_NAMES: u32 = 2;

/// The era from which a post put into a running turn before its last message is read at that turn's end, answered on
/// its thread or not, as the reply tool's Nothing says of an agent's post ("counts as read when your turn ends"); only
/// one still queued then (`later`) is the next turn's ([`ERA`]; paper-research N-1397.p91). His post a turn held,
/// put in or handed again for being left so, or held by a turn cut short, is read only once answered: else it comes
/// again once, then is owed (`his_at_end`; paper-research N-412.p2194, p2197, p2202). Before it, one the turn left
/// unanswered stayed given, the next turn's (N-101 p1214), and the replies of the record were drawn so; but no turn
/// began with it, and the delivery program's `free` waited on it (p81: docket-core, idle two hours on build 34). The
/// era's line reads those still carried then (`carried`).
pub const ERA_PUT_IN: u32 = 3;

/// The era from which delivery's push takes an ask as answered only by the ledger's rule, `closes` ([`ERA`];
/// paper-research N-1479 p107, proto-lead p109): a post of Y's that only passes X's ask on, a result Y wrote before it
/// was handed the ask, or one halted or moved away neither goes to X as Y's reply nor closes X's waits-on edge, and
/// the post that answers the ask goes to X, once: as Y's reply (`answered_first`), or by the edge (`waits_on`) where
/// the ask did not wake Y. Before it, the edge closed at Y's first post after the ask that Y had been handed, or a
/// result (`edge_closes`), and the record's `free` lines handed by that. Begun by the machine (`Machine::apply`), just
/// after the first post that goes to an agent otherwise by it (`Machine::close_changes`): every line before it hands
/// the same by either rule. Its number is 4 (proto-lead N-1479 p344), below docket-core2's `ERA_HIS_HANDED`, 5, which
/// coordd begins after `apply` returns: a post that begins both gets this era's line, then that one's with its
/// `passed` (proto-reader p342).
pub const ERA_CLOSES: u32 = 4;

/// The era from which the owner's post is read by its agent's own post only once it was handed to it
/// (`Ticket::read_own`; [`ERA`]; consult-3 N-1479 p151, proto-lead p174). His post to a working agent goes into its
/// turn at a round (`GivenInto`), to an idle one at its `free`, and the agent's post on his thread before that round
/// was written without it. Before it, his post was read as if put in at once: such a post, in the turn or its last
/// message (proto-engine p153), passed it, and it was never handed (research's N-1104.p102 on 19 Nov, passed by
/// paper-research's p104 18 s later). Nothing says before such a post that it comes, so the era begins on the line
/// after the first, and that line tells the posts it read to their agents late (`told_late`), each handed on its own,
/// or by a hand-over on its way then, whose `given` hands it (`ToldLate`; proto-reader N-1479 p204, consult-3 p212).
/// It is 5, after delivery-close's era 4 ([`ERA_CLOSES`]), which the machine begins inside `Machine::apply`, just after
/// its post, and coordd's check for this one reads after that (proto-lead N-1479 p344, p349): one post that both read
/// his post not handed and goes otherwise by the close begins both, era 4 on the line after it, then this one.
pub const ERA_HIS_HANDED: u32 = 5;

/// The era from which delivery hands an agent what is for it and no more ([`ERA`]; paper-research N-1446 p2367, on
/// gemini-advisor's audit p2365: of the 200 posts handed whole since build 71, 100 were others' talk or answered
/// already, and 22 of 84 turns woke for nothing):
/// - his post that only informs the agent (routed or told to another) goes by its first line in the digest, never
///   whole and first (`act_deliver`'s `whole`; before it, his went whole, N-1481 p44): 15 such leaks of N-1104 into
///   builders' briefs;
/// - a post brought along before a handed one on its thread goes whole only when it is for the agent (it goes now,
///   `push_of`); one that only informs it goes by its first line, with or without the act's digest (`act_deliver`'s
///   thread sweep): fresh-lead-3 was handed 23 posts of N-1104, 17.8 KB, for one 93-byte post of his;
/// - a typed fyi wakes nobody, not as a reply either: it is pulled (`push_by`): docket-edge6's lock notices woke two
///   agents each;
/// - another agent's post the agent answered by a post of its own after it waits no more, kept or due (`waiting_held`,
///   `answered_by_own`): paper-research answered N-1446's p2295 by the reply tool, kept for its turn's end, and 5 s
///   later was handed it whole, for a Nothing turn. A post that only names its writer is no answer to it unless written
///   with it handed: an ask that came meanwhile, kept, still goes (p2430).
/// - a typed result answers an ask by the ledger's rule (`closes`) only as any post does: it names the ask's writer,
///   answers the ask (`Post::answers`) or names it among its further ids; one that names nobody and answers nothing
///   answers as before (names are filled in by the rule for a post naming nobody, so it reaches whom it could answer).
///   Delivery's push, the ledger and the rule Reply read it so: docket-core's N-1446.p2293, a result naming
///   paper-research and answering its p2284, answered the asks of accounts-lead, docket-edge5 and docket-edge6 to
///   docket-core on N-1446, went to each as the reply it waited on and woke the three (proto-mind, p2371 (c), p2444).
/// The audit's last, `fold`'s race (`Hand`), is proto-mind's by p2371's split, with its own way back.
/// Begun by coordd at a start on a record in the era [`ERA_HIS_HANDED`] (`load_tickets`): every line before it hands as
/// it did. A record that reaches that era later begins this one at its next start.
pub const ERA_QUIET: u32 = 6;

/// The owner's `@all` (N-1134; the owner, N-1104 posts 22 and 26: "@ all means anybody can claim it right and then
/// answer", "reach all but only 1 needs to claim it"): his post reaches every agent on its ticket (the handler's
/// `p.all`), and the first of them to answer it takes it; another's answer to it after that is refused
/// (`Machine::taken`).
pub const ALL: &str = "all";

/// Where the owner's `@all` post stands (`Machine::all_state`): whom it is handed to now (the post's told, or whom the
/// last pass handed it to), the claim (the first answer to it, by whom and its number), and the passes before the
/// claim (by whom, and the post that passed it), in order.
#[derive(Clone, Debug, PartialEq)]
pub struct AllState {
    pub handed: Vec<String>,
    pub claim: Option<(String, u32)>,
    pub passed: Vec<(String, u32)>,
}

/// Why a step comes again (`Ticket::remind_why`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RemindWhy {
    /// Its last answer read as none of its outcomes: the forming call's sentence, as its reminder says it (N-101 p561)
    /// and the picture shows it (`ViewStatus::Awaits::read_none`).
    Read(String),
    /// The turn it was put into ended, or was cut, without its answer (`Machine::left_in_turn`): no reading, so its
    /// reminder says so in its own words (`What::Step::left`, remind.tmpl's `left`) and no view has a reading for it
    /// (consult-3 N-1396 p300, 1).
    Left,
}

/// The event coordd records when a delivery entered an agent's model input (0.2 A1, N-1397 post 5): `{t, to, delivery,
/// at, place, began}`, the giving whose `where` is PLACE. A record line of the machine's own, as HELD_HANDED is; an
/// older engine replays it as an event nothing binds.
pub const DELIVERED: &str = "delivered";

/// The event coordd records when deliveries it gave an agent are due again from a fact of its session, not its whole
/// `given_since` (0.2 A4, rule 19 on the fact): `{agent, deliveries, exited}`. A giving whose process ended before its
/// text entered (EXITED) is due again and is not counted toward "given twice, not read": its session did not drop it.
/// One that left the queue without entering, or was never seen there, is counted as `Line::Unread` counts. An older
/// engine replays it as an event nothing binds; its own rule then makes all of `given_since` due again.
pub const DUE_AGAIN: &str = "due_again";

/// What Given did for AGENT's delivery (SUFFIX, given up to UPTO from BEFORE) on X, undone, so it is due again. ERA:
/// the record's.
fn undo_given(x: &mut Ticket, agent: &str, suffix: &str, upto: u32, before: u32, era: u32) {
    // what Given did, undone, so the delivery is due again
    if x.mu.get(agent) == Some(&upto) {
        x.mu.insert(agent.to_string(), before);
    }
    x.closed_given.remove(&format!("{agent} {suffix}"));
    for w in x.withdrawn.iter_mut().filter(|w| w.0 == *agent && suffix == format!("s{}.withdrawn", w.1)) {
        w.3 = false;
    }
    for s in x.struck.iter_mut().filter(|s| suffix == format!("p{}.taken_back", s.n)) {
        s.told.retain(|a| a != agent);
    }
    for e in x.told_late.iter_mut().filter(|e| e.to == agent && suffix == format!("p{}.{TOLD}", e.n)) {
        e.given = false;
    }
    // from the era ERA_HIS_HANDED any giving that hands his post told late gave it (`ToldLate`): due again once none
    // standing does (`Ticket::handed`, which leaves out what was made due again)
    if era >= ERA_HIS_HANDED {
        let open: Vec<u32> = x.told_late.iter().filter(|e| e.to == agent && e.given && !x.handed(agent, e.n)).map(|e| e.n).collect();
        for e in x.told_late.iter_mut().filter(|e| e.to == agent && open.contains(&e.n)) {
            e.given = false;
        }
    }
    if let Some(g) = gift_index(&suffix).and_then(|i| x.gives.get_mut(i)).filter(|g| g.to == *agent) {
        g.given = false;
    }
    if let Status::Awaits { agent: a, owed, taken, refused, reminded, back, .. } = &mut x.status {
        let s = format!("s{}", owed.step);
        if *a == *agent {
            if suffix == s {
                *taken = false;
            } else if suffix == format!("{s}.back") && *back == Some(true) {
                *back = Some(false);
            } else if suffix == format!("{s}.again") && *reminded == Some(true) {
                *reminded = Some(false);
            } else if suffix.starts_with(&format!("{s}.refused")) {
                if let Some((_, g)) = refused {
                    *g = false;
                }
            }
        }
    }
}

/// The owner's hold on agents' mail (line 327, N-1324): switched on and off by his window's `Hold` and `Release` and by
/// `coord hold-mail`, one record line each, an `event` of the machine's own as `held_handed` is: `{"by": "owner", "on":
/// BOOL}`. The machine keeps the fact (`Machine::mail_hold`, `Machine::mail_held`); what is handed while it is on is the
/// delivery program's to decide, as all delivery is in 0.2 (`delivery.kind`, docket-core2; N-1324 posts 27 and 28).
pub const MAIL_HOLD: &str = "mail_hold";

/// The owner's switch of an agent's brain (proto-lead N-1446 p857.2): his press of Brain on or off on the agent's card,
/// one record line each way, an `event` of the machine's own as `mail_hold` is: `{"by": "owner", "agent": NAME, "on":
/// BOOL}`. The binding that opens the brain stays in the project's kind file (coordd writes no kind file); while his
/// switch is off no new pass of NAME's brain opens, whichever way it would (`Machine::brain_off`): the binding does not
/// fire for NAME, an open or a rekind is refused, a step's `open` is stuck. One already at work runs to its end (proto-
/// lead N-1446 p985, p987). The record is the one place that says whether it is on: no line for NAME, it is on
/// wherever a binding opens it.
pub const BRAIN_SWITCH: &str = "brain_switch";

/// The kind of an agent's brain (agent-base's brain.kind): the kind his `BRAIN_SWITCH` stops opening.
pub const BRAIN: &str = "brain";

/// His sign-off of one of his asks with his word of worth (his N-1002 p2: "estimate "utility""; consult-2's design, N-1002
/// p6: utility is his word at sign-off, one press, with what it took beside it; paper-research N-1446 p1786): his press
/// of `Worth it`, `Fine` or `Not worth it` behind the ask's `⋯` in Open → done, one record line, an `event` of the
/// machine's own as `brain_switch` is: `{"by": "owner", "t": T, "n": N, "worth": WORD}`, WORD one of [`WORTHS`], or
/// `none`, which takes it back. Only his, and only on a post of his the ASKED ledger says asks an agent
/// (`Machine::asked`), which a replay does not ask again: a line once written stands. The last line for the post holds
/// (`Machine::signed_off`); the graph reads each as a `signed_off` edge (`AskTimes::signed`). An engine from before it
/// reads the line as an event no binding is on: recorded, nothing changed (the way back, docket-edge5 p1793). Ending
/// his thread signs off none of its asks: one fact, not two.
pub const SIGNED_OFF: &str = "signed_off";

/// His words of worth at a sign-off ([`SIGNED_OFF`]), as its line writes them, in the order his press offers them.
pub const WORTHS: [&str; 3] = ["worth_it", "fine", "not_worth_it"];

/// A turn of an agent began (fresh-lead-3 N-1104 p1154: "made by the session driver when a turn begins
/// (`turn_began`, which is new)"; docket-edge6 N-1446 p3082): coordd's line when an agent's session shows a turn
/// running, one record line, an `event` of the machine's own as `signed_off` is: `{"agent": NAME, "delivery": ID}`,
/// ID the delivery the turn began with as its given places say, none when coordd knows of none. The machine keeps it
/// for NAME until its next turn begins (a turn's end line is not always kept, so it is not read for this): where a
/// copy of NAME is cut while the turn runs (`arrived`'s `began`, delivery.kind's `fork e.agent at e.began`). The policy's `working` is made by it (agent-base
/// policy/working.eflint). An engine from before it reads the line as an event no binding is on: recorded, nothing
/// changed, as with `signed_off`.
pub const TURN_BEGAN: &str = "turn_began";

/// The owner's hold on agents' mail while it is on (MAIL_HOLD): AT, its line's time. It keeps every post but his, and
/// his whenever written: his posts are the messages he sends while he holds the rest ("keep them interupted / esc'ed so
/// i can be the one to sedn them messages"), so one of his written before the hold that had not gone yet goes as it
/// would without it (paper-research N-1396 p235, a). What it keeps is read from who wrote it, so a replay and the first
/// round after a start give the same answer. One exception, from the lines too: his post a turn his own Esc cut while
/// it was on made due again waits with what it keeps, until his next post to that agent, which it goes with, or his
/// Release (`Machine::stopped_held`; paper-research N-412.p2205).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MailHold {
    pub at: At,
}

impl MailHold {
    /// Whether the hold keeps a post written by BY: every post but the owner's.
    pub fn keeps(&self, by: &str) -> bool {
        by != "owner"
    }

    /// Why the hold keeps an item, the words its `keep` carries (delivery.kind's `hold`; paper-research N-1396 p180):
    /// "the owner holds agents' mail since HH:MM UTC", the hold line's time, in UTC as the record's times are, so the
    /// words replay the same wherever coordd runs (paper-research N-1396 p235, b; the window's own held row shows his
    /// local time). Release lifts the keeps made with these words (`Machine::hold_lifted`).
    pub fn why(&self) -> String {
        format!("the owner holds agents' mail since {} UTC", hm(&self.at))
    }

    /// What a post the hold kept below its agent's read mark is kept for once the hold ended at END (Release): it waits
    /// only by its keep, so it stays kept, for the agent's next `free` to decide. "… from HH:MM to HH:MM UTC".
    pub fn ended(&self, end: &str) -> String {
        format!("the owner held agents' mail from {} to {} UTC", hm(&self.at), hm(end))
    }
}

/// The release his window runs and the one coordd runs (I-1457; proto-lead N-1446 p509, p610): a record line of the
/// machine's own, as `mail_hold` is, `{"release": NAME, "contains": [NAME, …], "engine": {"release": NAME, "contains":
/// [NAME, …]}}`. His window names its own build (`restart::ours`) in its hello, and coordc's bridge hands it to coordd
/// (`Request::WindowRelease`); coordd adds the release folder its own binary runs from as `engine`, absent when it
/// runs where it was built. Each half's `contains` lists the releases that its commit contains, as release.sh wrote
/// them in its folder at its cut (`contains.txt`), or, for a window-only edge, which has no folder, in
/// `releases/edges/NAME.txt`; it is empty when it has neither. An ask kept by build B is in his window while either
/// half of the last such line names B or lists it ([`WindowRelease::has`]), so an engine put in alone (`release.sh
/// live --no-exe`) counts as his window does, and the ledger compares names from the record and runs no git. coordd
/// writes a line only when either half differs from the last one recorded ([`Machine::window_release`]); a window
/// that names no release writes none.
pub const WINDOW_RELEASE: &str = "window_release";

/// A train branch's tip moved (N-1511 p43; paper-research N-1446 p2139, 1, p2152): an `event` line of the machine's
/// own, which coordd's git round writes when it reads a `train-` branch of a project repository at another commit than
/// its last such line says (`Core::trains_moved`): `{repo, branch, tip, tree, parents, from, commits, unlisted}`
/// (`syntax::system_event`), COMMITS those the move brought (the tip's history less FROM's), each with its parents and
/// tree, the newest first; a branch new since coordd's last read brings its tip alone. A step's `on` takes it, and the
/// facts read it ([`Seen`]: `reaches` walks its commits' parents). The machine runs no git: a line says what was read
/// when it was written, and a replay reads the same. No binding or handler is on it (`syntax::step_only`).
pub const MERGED: &str = "merged";

/// His ask taken (N-1511 p47; paper-research N-1446 p2139, 2, p2204, p2354): an `event` line of the machine's own,
/// which coordd writes after the input at which the first head is handed in for his post, not at the post, while a
/// binding is on it (`graph::Builder::first_handed`): a post no work answers opens no goal. coordd names the post; the
/// machine writes the value as
/// the ledger has it then (`Machine::asked_value`): `{post, by, to, title}` (`syntax::system_event`), TO whom it asks,
/// TITLE its first line. Once for each post (`Machine::asks_raised`), so each ask opens its goals once, by the bindings
/// on it (item.kind's `on asked(e) … open item(…)`), and a replay opens the same from the line's `fired`. No handler
/// is on it.
pub const ASKED: &str = "asked";

/// A goal's duties as the project's policy read them over the goal's lines (N-1511 p47, p49; paper-research N-1446
/// p2139, 2, p2204): the `decided` line that answers its `duties` question (`machine/duties.rs`), raised as this event
/// to the goal's step that awaits it by an `on` (`where e.goal == this`): `{goal, owed, by, broken}`, OWED its duties
/// that still hold, BY the lines that met one, BROKEN the first owed one violated, with the line it broke at. Only its
/// step reads it (`syntax::step_only`), and only the line raised after it: none is kept for a step given later.
pub const DUTIES: &str = "duties";

/// One half of a [`WINDOW_RELEASE`] line: a release that runs, and the earlier ones it contains.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Running {
    pub release: String,
    pub contains: BTreeSet<String>,
}

impl Running {
    /// The half VALUE names, `{"release", "contains"}`; none when it names no release.
    pub fn of(value: &Value) -> Option<Running> {
        let release = value.get("release").and_then(Value::as_str).filter(|r| !r.is_empty())?.to_string();
        let contains = value.get("contains").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(String::from).collect();
        Some(Running { release, contains })
    }

    /// Whether build B is in this half: B is it, or one it contains.
    pub fn has(&self, b: &str) -> bool {
        self.release == b || self.contains.contains(b)
    }
}

/// The last [`WINDOW_RELEASE`] line read: the release his window runs, and the one coordd runs when the line names it.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WindowRelease {
    pub window: Running,
    pub engine: Option<Running>,
}

impl WindowRelease {
    /// A `window_release` line's value; none when its window half names no release, or its `engine` names none.
    pub fn of(value: &Value) -> Option<WindowRelease> {
        let engine = match value.get("engine") {
            None | Some(Value::Null) => None,
            Some(e) => Some(Running::of(e)?),
        };
        Some(WindowRelease { window: Running::of(value)?, engine })
    }

    /// The line's value, as [`WindowRelease::of`] reads it.
    pub fn value(&self) -> Value {
        let half = |r: &Running| json!({"release": r.release, "contains": r.contains});
        let mut v = half(&self.window);
        if let Some(e) = &self.engine {
            v["engine"] = half(e);
        }
        v
    }

    /// Whether build B is in his window: either half names it or lists it (p610).
    pub fn has(&self, b: &str) -> bool {
        self.window.has(b) || self.engine.as_ref().is_some_and(|e| e.has(b))
    }

    /// Whether cut NAME is in his window as a program's `in_window` asks ([`IN_WINDOW`]; proto-mind N-1446 p1516): the
    /// window's half has it, as its release or one it contains, and with ENGINE coordd's half has it too.
    pub fn in_window(&self, name: &str, engine: bool) -> bool {
        self.window.has(name) && (!engine || self.engine.as_ref().is_some_and(|e| e.has(name)))
    }
}

/// The call of a step the machine answers itself from his window's release (N-1446 p1491, 6; proto-mind p1516): a
/// program's `hold W in_window(name, engine)` (agent-base calls/in_window.call) waits until the last
/// [`WINDOW_RELEASE`] line has the cut NAME in his window ([`WindowRelease::in_window`]), and is answered then by the
/// ticket itself, `Has(release, contains)`, the window's half: in that line's apply, or in the apply that reaches the
/// step when the last line kept has it already (`Machine::line_answers`), the same live and on replay. W may
/// answer it by hand, for a window that names no release. An engine from before has no such call of its own: the step
/// stays owed to W.
pub const IN_WINDOW: &str = "in_window";

/// HH:MM of AT, a record time (UTC); AT itself when it is not one.
fn hm(at: &str) -> &str {
    at.get(11..16).filter(|s| s.as_bytes().get(2) == Some(&b':')).unwrap_or(at)
}

/// The line of a take-back after an agent had the post (0.2 B8, N-1396): the `event` kind, `{t, n, by, to}`, so an
/// engine from before reads it as an event nothing takes and refuses nothing. The post is struck, not gone, and each
/// agent in TO is told once (`What::TakenBack`). Written by the machine alone (`Input::TakeBack`, `Input::StruckFor`).
pub const TAKEN_BACK: &str = "taken_back";

/// The suffix of a post's delivery told late (`ToldLate`): `N-5.p3.told`, his post 3, told after it waited on his
/// question, handed on its own to an agent the tell chose (proto-lead N-1446 p569, (c2); p576). It carries that one
/// post, as a posts delivery does ([`delivered_post`]); coordd hands it whole, by its id, and its `given` line says so.
/// It tells, never only informs, and moves no mark.
pub const TOLD: &str = "told";

/// The kind of a ticket that runs an `agent_post` handler (design-two-systems §16).
pub const AGENT_POST: &str = "agent_post";

/// Where the numbers of tickets that run a handler start: apart from the ones the owner sees, so N-1 is followed by
/// N-2 whatever ran between them.
pub const HIDDEN: u64 = 1 << 40;

/// Where the message runtime's hidden tickets are numbered (an agent's delivery ticket and its runs, delivery.rs):
/// apart from the other hidden tickets, so that those are numbered in its records as an engine without it numbers
/// them, and train 26 replays them as recorded.
pub const DESK: u64 = HIDDEN + (1 << 39);

/// A blocking ask, as the ticket running its handler holds it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AskOf {
    pub by: String,
    /// Its words, where the machine has them (a `coord ask`'s; none for a step's Ask whose question is a place).
    pub text: String,
    /// The question as asked: its words, or the place they are at (a step's Ask: the question is the agent's answer,
    /// by place, which coordd reads when it gives it on; paper-research N-101 p524: it reached the lead printed as a
    /// record).
    pub words: Value,
    /// The asker's command is waiting for the answer (until the answer, or AskLater).
    pub waiting: bool,
    pub replied: bool,
    /// The ticket whose step the asker held: while the ask is unanswered, a turn's end does not answer that step
    /// (an ask cut at its wait stays the step's; the late answer's turn answers it, docket-edge m1217).
    pub held: u64,
    /// The asker's own question ticket (`coord ask` while it held the step): its closing notice is the answer, so
    /// closing gives the asker nothing more than the step again.
    pub own: bool,
}

/// The kind of a ticket that runs an ask's handler.
pub const ASK: &str = "ask";

/// An answer at a hole, carried out.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Shaped {
    pub n: u32,
    pub at: At,
    pub answer: Value,
    pub refused: Option<String>,
    pub undone: bool,
    /// The ticket's steps when it was carried out, and when it was undone: the steps numbered after the first and up
    /// to the second are its program's, taken back (paper-research m1463, 1).
    pub steps: u32,
    pub undone_steps: u32,
    /// The hole it answered: its variables and their types (where Undo puts the ticket back).
    pub env: Env,
    pub scope: BTreeMap<String, Type>,
}

impl Ticket {
    /// Whether a handler or a stop runs in it (`handler_ticket`, stop.rs): a hidden ticket under the one it runs for.
    /// A delivery ticket handles too, but is under none.
    pub fn is_run(&self) -> bool {
        self.handling.is_some() && self.under.is_some()
    }

    /// Its form, read back from its text where it is a closed run's (`form_text`).
    pub fn form_of(&self) -> std::borrow::Cow<'_, Value> {
        match &self.form_text {
            Some(text) => std::borrow::Cow::Owned(serde_json::from_str(text).unwrap_or_default()),
            None => std::borrow::Cow::Borrowed(&self.form),
        }
    }

    /// Field F of its form (`form_of`): a closed run's text is read back only when it names F.
    pub fn form_field(&self, f: &str) -> Option<Value> {
        match &self.form_text {
            Some(text) if text.contains(&format!("\"{f}\":")) => serde_json::from_str::<Value>(text).ok()?.get(f).cloned(),
            Some(_) => None,
            None => self.form.get(f).cloned(),
        }
    }

    /// Closed, a run keeps its form as text (`form_text`).
    fn pack_form(&mut self) {
        if self.is_run() && self.form_text.is_none() {
            self.form_text = Some(std::mem::take(&mut self.form).to_string());
        }
    }

    /// It runs again, or is read as it was given: its form as values again.
    fn unpack_form(&mut self) {
        if let Some(text) = self.form_text.take() {
            self.form = serde_json::from_str(&text).unwrap_or_default();
        }
    }

    /// The answers to step STEP not read, when it was passed up to HOLDER: someone else's are among them. Else none (a
    /// reminder says its own why).
    pub fn passed(&self, step: u32, holder: &str) -> Vec<crate::line::NotRead> {
        let of: Vec<_> = self.not_read.iter().filter(|(s, _)| *s == step).map(|(_, n)| n.clone()).collect();
        if of.iter().any(|n| n.by != holder) { of } else { Vec::new() }
    }

    pub fn is_open(&self) -> bool {
        !matches!(self.status, Status::Done(_) | Status::Failed(_))
    }
    /// Whether this is a run an act of AGENT's raised (`handling` "act", `raised_by` AGENT; `event_on`'s `by`): a step
    /// owed in it is AGENT's own act's (`waiting_of`'s `raised`, `Machine::owing`).
    pub fn raised_for(&self, agent: &str) -> bool {
        self.handling.as_deref() == Some("act") && self.raised_by.as_deref() == Some(agent)
    }
    /// An agents' thread (design part 9b): ended as it opened, with the result Between; a place for posts that nothing
    /// runs, which waits on whoever was told a post and has not ended a turn on it.
    pub fn between(&self) -> bool {
        matches!(&self.status, Status::Done(v) if v.get("head").and_then(Value::as_str) == Some("Between"))
    }
    /// Posts AGENT has not been shown, not its own; a post moved away is shown where it went, not here.
    /// A held post, and those after it, wait: an agent's read mark never passes a post it has not been given. His post
    /// told to AGENT late (`told_late`) is handed on its own, not here.
    /// A step carries these whole, its ticket's posts being its case; a delivery of posts alone, `posts_unread`.
    pub fn unread(&self, agent: &str) -> Vec<Post> {
        let mu = self.mu.get(agent).copied().unwrap_or(0);
        let upto = self.seen_upto();
        self.posts.iter().filter(|p| p.n > mu && p.n <= upto && p.by != agent && p.moved.is_none() && p.visible() && !self.late_for(agent, p.n)).cloned().collect()
    }
    /// What a delivery of posts alone (`What::Posts`) shows AGENT: of `unread`, only what reaches it: the posts that
    /// wake or inform it, and the owner's `@all` such a post passes on to it. None of the thread's history goes with
    /// them, since its mark or before it; it reads that with `coord show` if it needs it. The owner reads his threads
    /// whole. What informs it only as one on the thread (policy's inform, `Post::only_as_member`) is among these, for the
    /// delivery program to give by its first line (delivery.kind's `member`). Paper-research N-1446 p2059: N-1446's
    /// 2,057 posts went whole into two new agents' first deliveries, 4 MB each, past their context; p2092, the same
    /// for an agent with a mark (the owner's yes, N-1104 p923, to his p912: a delivery is ordered and placed first).
    pub fn posts_unread(&self, agent: &str) -> Vec<Post> {
        let unread = self.unread(agent);
        if agent == "owner" {
            return unread;
        }
        let reaches = |p: &Post| p.wakes().any(|a| a == agent) || p.inform.iter().any(|a| a == agent);
        let passed: BTreeSet<u32> = unread.iter().filter(|p| reaches(p)).filter_map(|p| p.passes).collect();
        unread.into_iter().filter(|p| reaches(p) || passed.contains(&p.n)).collect()
    }
    /// Whether post N is told to AGENT late (`ToldLate`), handed or not.
    pub fn late_for(&self, agent: &str, n: u32) -> bool {
        self.told_late.iter().any(|e| e.to == agent && e.n == n)
    }
    /// Whether post N was told to AGENT late and is not given to it yet (`ToldLate`): due, though its mark is past.
    pub fn owes_late(&self, agent: &str, n: u32) -> bool {
        self.told_late.iter().any(|e| e.to == agent && e.n == n && !e.given)
    }
    /// The posts up to UPTO that AGENT's mark may have passed and it was not handed (`handed`): his once held under his
    /// question (`was_asked`), held still, told to it late and not given yet (`ToldLate`), or told to others only. A
    /// post of AGENT's written now answers none of them (proto-lead N-1446 p569, (c2); p595). None on a record from
    /// before: no mark passed a held post there.
    pub fn passed_unhanded(&self, agent: &str, upto: u32) -> BTreeSet<u32> {
        self.was_asked.range(..=upto).filter(|n| !self.handed(agent, **n)).copied().collect()
    }
    /// What a post of AGENT's written now, having been handed up to UPTO, does not answer: `passed_unhanded`, and the
    /// posts past its mark it was not handed, which UPTO passes only when it reaches his post told late and given
    /// (`reach`). Nothing more on a record from before: UPTO never passed the mark there.
    pub fn not_answered_by(&self, agent: &str, upto: u32) -> BTreeSet<u32> {
        let mu = self.mu.get(agent).copied().unwrap_or(0);
        let mut out = self.passed_unhanded(agent, upto);
        out.extend(self.posts.iter().filter(|p| p.n > mu && p.n <= upto && p.by != agent && !self.late_for(agent, p.n)).map(|p| p.n));
        out
    }
    /// How far G, a `given` row of this ticket, reaches for what its writer answers: its UPTO, or for his post told
    /// late (`N-1.p5.told`, given with the mark as it stood, which it does not move) that post, when later.
    pub fn reach(g: &GivenRec) -> u32 {
        told_post(&g.delivery).map_or(g.upto, |(_, n)| g.upto.max(n))
    }
    /// Whether G, a `given` row of this ticket, handed post N: the one test of "had it" (proto-reader N-1446 p595,
    /// proto-lead p595), for the take-back (`Machine::had_post`, `take_back_checked`), what a post answers
    /// (`passed_unhanded`), whom a thread waits on and where his post is. Extends `reach`: see `hands_in`.
    pub fn hands(g: &GivenRec, n: u32) -> bool {
        Self::hands_in(&g.delivery, g.upto, &g.skipped, n)
    }
    /// Whether DELIVERY, given up to UPTO passing SKIPPED unhanded, hands post N: his post told late (`N-1.p5.told`)
    /// hands that post, whatever UPTO; any delivery hands a post up to UPTO it did not pass unhanded. On a record from
    /// before (c2) nothing is skipped and nothing is told late: UPTO alone, as it always was.
    pub fn hands_in(delivery: &str, upto: u32, skipped: &BTreeSet<u32>, n: u32) -> bool {
        told_post(delivery).is_some_and(|(_, m)| m == n) || (n <= upto && !skipped.contains(&n))
    }
    /// Whether G, a `given` row of this ticket, carried post N, of what it hands (`hands`): his post told late, its own,
    /// wherever the mark it was given with stood, or one past that mark (BEFORE). What one giving brought, for the
    /// readers of a turn's givings (`Machine::his_in`, `claimed_in`, `left_before`, `answered_into`, `read_given`): the
    /// range (BEFORE, UPTO] alone is empty for a `.told` row, which moved no mark (consult-3 N-1446 p717).
    pub fn gave(g: &GivenRec, n: u32) -> bool {
        told_post(&g.delivery).is_some_and(|(_, m)| m == n) || (n > g.before && Self::hands(g, n))
    }
    /// The posts G, a `given` row of this ticket, brought its agent ([`Ticket::gave`]) but its own, oldest first: what
    /// its delivery showed as `unread`. The graph's handed edges and coordd's check that a delivery's place holds them
    /// read this (paper-research N-1446 p1376, 3; p1380).
    pub fn brought<'a>(&'a self, g: &'a GivenRec) -> impl Iterator<Item = &'a Post> + 'a {
        self.posts.iter().filter(move |p| p.by != g.to && Self::gave(g, p.n))
    }
    /// Whether AGENT has been handed post N (`hands`), by a delivery not made due again.
    pub fn handed(&self, agent: &str, n: u32) -> bool {
        self.given.iter().any(|g| g.to == agent && !g.again() && Self::hands(g, n))
    }
    /// `GivenRec::skipped` of a delivery given to AGENT now, up to UPTO, before its mark moves, when its line does not
    /// say (`Line::Given`): `left_out`, of what AGENT is shown now (`unread`). After a start nothing says what a
    /// hand-over showed, so his post told late while it was on its way is left out, and goes once more, as `.told`: at
    /// worst shown twice, never lost (consult-3 N-1479 p353, paper-research p352, p355).
    pub fn skipped_now(&self, agent: &str, upto: u32) -> BTreeSet<u32> {
        if !self.may_leave_out(agent, upto) {
            return BTreeSet::new();
        }
        let shown: Vec<u32> = self.unread(agent).iter().map(|p| p.n).collect();
        self.left_out(agent, upto, &shown)
    }
    /// Whether a delivery to AGENT up to UPTO may leave a post out (`left_out`): one of his up to UPTO was once asked,
    /// or one told to AGENT late is not given yet. Neither on a record from before c2.
    pub fn may_leave_out(&self, agent: &str, upto: u32) -> bool {
        self.was_asked.range(..=upto).next().is_some() || self.late_open(agent, upto).next().is_some()
    }
    /// His posts up to UPTO told to AGENT late and not given yet (`ToldLate`), by number.
    pub fn late_open<'a>(&'a self, agent: &'a str, upto: u32) -> impl Iterator<Item = u32> + 'a {
        self.told_late.iter().filter(move |e| e.to == agent && !e.given && e.n <= upto).map(|e| e.n)
    }
    /// What a delivery to AGENT up to UPTO showing SHOWN leaves out, so passes unhanded: his posts once asked up to UPTO
    /// that AGENT was not handed before (`handed`), and his posts told to AGENT late up to UPTO and not given yet
    /// (`ToldLate`): `GivenRec::skipped`. With the told-late ones `hands` is exact: a giving that does not show his post
    /// told late does not hand it, whatever its UPTO, so only the giving that shows it closes its row (consult-3
    /// N-1479 p262, proto-reader p264). c2's are asked posts, counted either way.
    pub fn left_out(&self, agent: &str, upto: u32, shown: &[u32]) -> BTreeSet<u32> {
        let asked = self.was_asked.range(..=upto).filter(|n| !self.handed(agent, **n)).copied();
        asked.chain(self.late_open(agent, upto)).filter(|n| !shown.contains(n)).collect()
    }
    /// `handed_upto`, with his posts told late and given (`reach`): what a post AGENT writes now may answer.
    pub fn answerable_upto(&self, agent: &str) -> u32 {
        self.given.iter().filter(|g| g.to == agent && !g.again()).map(Self::reach).max().unwrap_or(0)
    }
    /// AGENT posted N in the record's era ERA: it has read up to it, but not past a post held from it (it is given that
    /// one once told), nor past a post that wakes it and was never handed to it. Before the era ERA_HIS_HANDED it read
    /// his so; each of his it read so goes in PASSED as (ticket, post, AGENT), for the line beginning that era
    /// (`Machine::passed_his`).
    pub fn read_own(&mut self, agent: impl Into<String>, n: u32, era: u32, passed: &mut Vec<(u64, u32, String)>) {
        let agent = agent.into();
        // a post that names or tells AGENT and has not been handed to it: AGENT's own post did not read it, so it stays
        // due (paper-research N-560 p7, p10: N-476.p20, held for docket-edge7's turn's end, was passed by its p23 in
        // that turn and never given). Not one the agent did read without a `given` line: one typed into its own
        // terminal (`Line::Typed`), nor, before the era ERA_HIS_HANDED, the owner's, read as put into its running turn
        // at once (only his were; his sent to wait were not, N-101 p1390): it goes in at a round, and from that era his
        // too stays due until handed (`handed`; consult-3 N-1479 p151). HIS: by that era's rule.
        let handed = self.handed_upto(&agent);
        let seen = self.seen_upto();
        let unhanded = |p: &Post, his: bool| p.moved.is_none() && p.visible() && p.by != agent && (p.by != "owner" || self.waits.contains(&p.n) || (his && !self.handed(&agent, p.n))) && p.typed.as_deref() != Some(agent.as_str());
        let upto = |his: bool| n.min(seen).min(self.posts.iter().filter(|p| p.n > handed && p.n <= n).find(|p| unhanded(*p, his) && p.wakes().any(|a| *a == agent)).map_or(u32::MAX, |p| p.n - 1));
        let mu = self.mu.get(&agent).copied().unwrap_or(0);
        let read = upto(era >= ERA_HIS_HANDED);
        if era < ERA_HIS_HANDED {
            let held = mu.max(upto(true));
            passed.extend(self.posts.iter().filter(|p| p.n > held && p.n <= read && p.by == "owner" && unhanded(*p, true) && p.wakes().any(|a| *a == agent)).map(|p| (self.t, p.n, agent.clone())));
        }
        self.mu.insert(agent, mu.max(read));
    }
    /// The last post of this ticket post P answers: what its writer had been handed when it was written
    /// (`answered_upto`), or every post before it.
    pub fn answers_upto(&self, p: &Post) -> u32 {
        self.answered_upto.get(&p.n).copied().unwrap_or(p.n.saturating_sub(1))
    }
    /// Whether post P answers post M of this ticket: M is up to what P's writer had been handed, and was not kept
    /// from it then (`not_answered`).
    pub fn answers(&self, p: &Post, m: u32) -> bool {
        m <= self.answers_upto(p) && !self.not_answered.get(&p.n).is_some_and(|k| k.contains(&m))
    }
    /// Whether post P answers post M for the rule Reply of a turn holding up to UPTO (`Machine::reply_names_of`): as
    /// `answers`, but not a reply-tool answer to a post past UPTO, one put into the turn while it ran. The turn's last
    /// message still answers what the turn began with, as the footer of what was put in says (paper-research N-1397
    /// p49: hs-ls's turn began on N-1396 p179, its reply-tool answer #182 to p180 counted as answering p179, and its
    /// last message, a head of 40 minutes, was posted nowhere).
    /// Of the posts it holds, P answers those among the reply tool's further ids on this ticket (`also_here`: a `to`
    /// of "p180, p179", consult-3 N-1397 p58).
    pub fn answers_for_turn(&self, p: &Post, m: u32, upto: u32) -> bool {
        self.answers(p, m) && (p.answers.is_none_or(|a| a <= upto) || self.also_here(p).contains(&m))
    }
    /// The posts of this ticket P answers besides its `answers`: the reply tool's further ids on this ticket itself
    /// (`also`, `Line::AnswersAlso`), which `answered_from` does not keep (it holds only another ticket's).
    pub fn also_here(&self, p: &Post) -> Vec<u32> {
        p.also.iter().filter_map(|a| post_id(a)).filter(|(u, _)| *u == self.t).map(|(_, m)| m).collect()
    }
    /// Whether P's reply-tool further ids here (`also_here`) answer a post of a turn handed this ticket's posts after
    /// BEFORE, up to UPTO: the rule before the ledger's took that turn as answered (consult-3, N-1397 p58). The one
    /// test of it, for `Machine::reply_names_of` and `kept_reply` (proto-reader N-1479 p360).
    pub fn further_in_turn(&self, p: &Post, before: u32, upto: u32) -> bool {
        self.also_here(p).iter().any(|m| *m > before && *m <= upto && self.answers(p, *m))
    }
    /// The last post of this ticket AGENT has been handed (a delivery made due again, never read, not counted).
    pub fn handed_upto(&self, agent: &str) -> u32 {
        self.given.iter().filter(|g| g.to == agent && !g.again()).map(|g| g.upto).max().unwrap_or(0)
    }
    /// The last post an agent may be shown: the one before the first held or halted post, else the last. His post
    /// held under his own question about it (`asked`) holds nothing back (proto-lead N-1446 p569, (c2)).
    pub fn seen_upto(&self) -> u32 {
        self.posts.iter().find(|p| (p.held.is_some() && !self.asked.contains(&p.n)) || p.halted).map_or(self.last(), |p| p.n - 1)
    }
    /// His posts an agent may be shown now: shown (`Post::visible`) and inside `seen_upto` (`Machine::posts_reached`).
    pub fn his_shown(&self) -> Vec<u32> {
        let upto = self.seen_upto();
        self.posts.iter().filter(|p| p.n <= upto && p.by == "owner" && p.visible()).map(|p| p.n).collect()
    }
    pub fn last(&self) -> u32 {
        self.posts.len() as u32
    }
    /// What has happened on it, for its hole: its posts (not coordd's), the turns that answered its gifts, the tickets
    /// below it that closed.
    pub fn mark(&self, closed_below: usize) -> (usize, usize, usize) {
        (self.posts.iter().filter(|p| p.by != "coordd").count(), self.answers.len(), closed_below)
    }
    /// The id, within the ticket, of the notice of its latest close: `c`, then `c2`, ...
    pub fn close_notice(&self) -> String {
        if self.closes > 1 { format!("c{}", self.closes) } else { "c".into() }
    }
}

/// What `apply` gives: lines to append (in order, the input first), effects to carry out off the lock, and the
/// reply to whoever gave the input.
#[derive(Debug)]
pub struct Out {
    pub lines: Vec<Line>,
    pub effects: Vec<Effect>,
    pub reply: Result<String, String>,
}

impl Default for Out {
    fn default() -> Out {
        Out { lines: Vec::new(), effects: Vec::new(), reply: Ok(String::new()) }
    }
}

enum Work {
    Run(u64, Prog, Env),
    Resume(u64, Value),
    Throw(u64, Raised),
}

struct Cx {
    now: At,
    lines: Vec<Line>,
    effects: Vec<Effect>,
    /// In replay: the recorded outputs of this input (a child's program is taken from them).
    recorded: Option<Vec<Line>>,
}

impl Cx {
    fn line(&mut self, l: Line) {
        self.lines.push(l);
    }
    fn effect(&mut self, e: Effect) {
        if self.recorded.is_none() && !self.effects.contains(&e) {
            self.effects.push(e);
        }
    }
}

/// A part of the machine that `due` reads ([`Machine::changes`]): read as T, written only through [`Counted::edit`],
/// which counts the write. So a write the count does not see does not compile (consult-3, N-412; paper-research N-788).
#[derive(Debug, Default, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Counted<T> {
    v: T,
    writes: u64,
}

impl<T> Counted<T> {
    pub fn new(v: T) -> Self {
        Counted { v, writes: 0 }
    }
    /// T to write: counted once, whatever is done with it.
    pub fn edit(&mut self) -> &mut T {
        self.writes += 1;
        &mut self.v
    }
    pub fn writes(&self) -> u64 {
        self.writes
    }
}

impl<T> std::ops::Deref for Counted<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.v
    }
}

/// The threads around a ticket X that may close its posts' asks (`Machine::closers`), read once for X
/// (`Machine::around`): those that follow it (their form's `follows` is X), and those tied at one of its posts, each with
/// that post. Both in ticket order, as `closers` read them.
#[derive(Debug, Default)]
pub struct Around {
    follows: Vec<u64>,
    tied: Vec<(u32, u64)>,
}

/// Every ticket's [`Around`], read in one pass over the tickets (`Machine::arounds`): for each id some form's `follows`
/// names, the tickets whose form names it; for each ticket some other is tied at, those tied there, each with that post;
/// both in ticket order. Read from the tickets alone: the id is matched as the form wrote it, so the kinds read at the
/// time (`with_defs`) do not change it.
#[derive(Debug, Default)]
pub struct Arounds {
    follows: BTreeMap<String, Vec<u64>>,
    tied: BTreeMap<u64, Vec<(u32, u64)>>,
}

/// Whether post P of X may ask anything open (`Machine::asks_open`, before what closed it is read): it asks, it is not
/// taken back (p1102 counts a take-back as a close: the views do not draw the post, proto-reader N-1479 p295), and on a
/// closed ticket it is told to the owner.
fn may_ask(x: &Ticket, p: &Post) -> bool {
    p.asks_open && !p.taken_back && (x.is_open() || p.wakes().any(|w| w == "owner"))
}

/// Whether post P asked the owner anything, open or closed since: passages marked on it, or an agent's post typed an
/// ask to him (`PostKind::asks_owner`, as its push sets `asks_open`). How such a post's asks were closed is shown
/// (`AsksClosed`), a typed ask's as a marked post's (paper-research N-1479 p318, proto-window p319). The window reads
/// this one rule for which posts asked him (yaaihh threads.rs `open_to_owner`, I-1480).
pub fn asked_him(p: &Post) -> bool {
    !p.asks.is_empty() || (p.by != "owner" && p.kind.as_ref().is_some_and(crate::line::PostKind::asks_owner))
}

/// What MAKE reads from the tickets, kept in SLOT with WRITES, the count of ticket writes it was made at, and made again
/// once that count has moved: one snapshot's views share it (`Machine::unhanded`, `Machine::arounds`).
fn kept_unwritten<T>(slot: &std::sync::Mutex<Option<(u64, std::sync::Arc<T>)>>, writes: u64, make: impl FnOnce() -> T) -> std::sync::Arc<T> {
    let mut kept = slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((w, v)) = kept.as_ref() {
        if *w == writes {
            return v.clone();
        }
    }
    let v = std::sync::Arc::new(make());
    *kept = Some((writes, v.clone()));
    v
}

/// A thread an agent's turn was given posts on, when the turn holds more than one (`Machine::reply_threads`, N-807).
#[derive(Clone, Debug, PartialEq)]
pub struct ReplyThread {
    pub t: u64,
    pub id: String,
    /// The last post given there that is not the agent's own: what a part with no other address answers.
    pub last: u32,
    /// The authors of the posts given there, in order, each with its last post given: what `@NAME` answers.
    pub authors: Vec<(String, u32)>,
    /// Some post given there tells or names the agent (not only informs it).
    pub told: bool,
    /// The agent answered it by hand once handed it (a post of its own there answering what it was given or made in a
    /// later second, or the reply tool from another thread: `Machine::answered_given`).
    pub answered: bool,
    /// The owner's `@all` handed to the agent there and claimed by nobody, among the posts given there
    /// (`Machine::all_held`): what a part for the thread answers in place of `last`, so that it is the claim, as the rule
    /// Reply's post is (consult-3, N-807 post 118).
    pub all: Option<u32>,
}

/// Saved and loaded whole (serde) as a replay's state at a cut (coordd's `ticket/replay_state.rs`, I-1509), but for
/// what is not the record's: the defs, read again and named by the state's key, and the two caches of views.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Machine {
    #[serde(skip)]
    pub defs: Counted<Defs>,
    /// Counted up at every input put and every replay (`apply`, `replay_with`); with the writes to what `due` reads,
    /// [`Machine::changes`].
    changes: u64,
    /// Each agent's role as the record's `role` lines last said it (N-101, runner.rs).
    pub roles: BTreeMap<String, String>,
    /// For each agent, the roles the policy has it hold besides its own (`Line::Role`'s `also`).
    pub also: BTreeMap<String, Vec<String>>,
    pub tickets: Counted<BTreeMap<u64, Ticket>>,
    /// For each agent, the owner's posts not handed to it (`view::owner_unhanded`), with the count of ticket writes it was
    /// made at: made once while no ticket changes, so one snapshot's views share it (part 4's cost, N-818 post 32).
    /// Nothing of the record's; a Mutex, not a cell, so the machine stays Sync.
    #[serde(skip)]
    unhanded: std::sync::Mutex<Option<(u64, std::sync::Arc<crate::view::Unhanded>)>>,
    /// Every ticket's threads around it (`Arounds`), kept as `unhanded` is: a snapshot asked `around` for every view with
    /// a post that asks, each a pass over every ticket (docket-core N-1446 p744).
    #[serde(skip)]
    arounds: std::sync::Mutex<Option<(u64, std::sync::Arc<Arounds>)>>,
    /// Tickets that came to owe the owner a step whose call has a `default` (N-487): all a clock that lapses them
    /// reads, never a walk of every ticket. Kept loosely: one that owes something else by now is passed over and
    /// dropped by its reader (`lapsing`).
    pub lapsing: BTreeSet<u64>,
    /// The owner's once-edits of call templates, in the order made (`Line::Template`).
    pub templates: Vec<TemplateEdit>,
    next: u64,
    /// The least numbers a hidden ticket (from HIDDEN) and one of the message runtime's (from DESK) may take: past
    /// every one the record gave, opened here or refused (`number_taken`), as `next` is for the ones he sees.
    hidden_past: u64,
    desk_past: u64,
    /// The way back (proto-lead N-1446 p398, clause 1): the tickets a record opened that this release does not load,
    /// each of a kind it does not have (an aside), or opened by an input on one (its thread, rejoin and handlers).
    /// Made by `replay_with`; an input on one is refused before it runs, and only its posts, `Told` and `Halted` on
    /// tickets loaded here are taken (`as_recorded`).
    pub not_loaded: BTreeSet<u64>,
    /// The posts of his whose handler this release did not load (clause 1's handler, `handler_ticket`, or one such
    /// handler's run took it in, `gathering`) and that no line has told yet: made by `replay_with`, sent on by coordd
    /// (`untold_posts`, UNTOLD).
    pub untold: BTreeSet<(u64, u32)>,
    /// The handlers the input being replayed opened that this release does not load (`handler_ticket`), for
    /// `replay_with` to say.
    handlers_unloaded: Vec<u64>,
    /// The handlers of his posts this release did not load, each with the ticket it runs under, while the record has
    /// not closed it (`as_recorded`): a post of his that opens no handler of its own while one is open was taken into
    /// that one's run (0.3's `gather`), so it is one this release left untold too (`owner_landed`).
    gathering: BTreeMap<u64, u64>,
    /// For each `every N of` binding (by its text), the events it has counted.
    pub counts: BTreeMap<String, u64>,
    /// For each clock binding (by its text), when it last fired.
    pub clocks: BTreeMap<String, At>,
    /// For each kind, and each call, the last reload that named it (`Line::Reload`): the Kinds answer's mark.
    pub kinds_edited: BTreeMap<String, crate::view::LiveEdit>,
    pub calls_edited: BTreeMap<String, crate::view::LiveEdit>,
    /// While replaying: the tickets the log leaves closed, with how (the last of their closes).
    closed_in_log: BTreeMap<u64, (Option<Value>, Option<Raised>)>,
    /// In replay: for each kind a reload named (its text changed), the last such line; and the line being replayed.
    /// A recorded text that opens one of them before that line may have checked against it as it was
    /// (`with_kinds_as_they_were`).
    reloaded_at: BTreeMap<String, usize>,
    replaying: Option<usize>,
    /// While replaying, what the lines replayed so far looked up ahead and found not so: the kinds taken as no reload
    /// changed them after the line (`with_kinds_as_they_were`, a Mutex as it reads), and the tickets asked whether the
    /// log leaves them closed (`unchecked`). A replay stopped at a cut goes on from it only with a tail that says
    /// nothing else of them ([`Machine::reaches_back`]); its end lets them go.
    asked_kinds: std::sync::Mutex<BTreeSet<String>>,
    asked_closed: BTreeSet<u64>,
    /// For each agent, the deliveries it was given since its last turn ended (the rule Reply asks for exactly one).
    given_since: BTreeMap<String, BTreeSet<String>>,
    /// For each agent, the deliveries of posts it was given twice and read neither time, each with its ticket and the
    /// post it showed up to: due again at its next turn's end or its process's end, never left behind (0.2 A4;
    /// consult-3 N-818.p76, 2; p78, 2). From the lines.
    missed: BTreeMap<String, BTreeMap<String, (u64, u32)>>,
    /// Of those, the ones put into a turn already running (`Line::Given` with `into`).
    into_since: BTreeMap<String, BTreeSet<String>>,
    /// Of those, before the era [`ERA_PUT_IN`], the posts put into a turn that ended without answering them, kept for
    /// the next turn (`Line::TurnEnded`): that era's line reads them.
    carried: BTreeMap<String, BTreeSet<String>>,
    /// For each agent, the deliveries carrying a post of the owner's that it was handed again for leaving it unanswered
    /// in a turn (`GivenRec::left`) and left unanswered again, each with its ticket and the post it showed up to: not
    /// handed a third time and not read, out of `given_since`, and named at the foot of each of its deliveries until
    /// it answers (`owed_posts`; paper-research N-412.p2197, 1). From the lines.
    owed: BTreeMap<String, BTreeMap<String, (u64, u32)>>,
    /// The agents whose running turn the owner stopped (`Line::Esc` by him), until that turn ends or is cut. From the
    /// lines.
    stopped_by_him: BTreeSet<String>,
    /// For each agent, his posts to it (ticket, post) that a turn his own Esc cut while his hold was on made due again:
    /// they wait with what the hold keeps, until his next post to it, which they go with, or his Release
    /// (`stopped_held`; paper-research N-412.p2202, 2, and p2205). From the lines.
    stopped_his: BTreeMap<String, BTreeSet<(u64, u32)>>,
    /// The record holds the `held_handed` event (HELD_HANDED): coordd sends it no more.
    pub held_handed: bool,
    /// The record's era ([`ERA`]), from its last `era` line read: 0 in a record with none.
    pub era: u32,
    /// His posts an agent's own post read though not handed to it, in the input applied last, in a record before the
    /// era [`ERA_HIS_HANDED`] (`Ticket::read_own`): (ticket, post, agent). coordd begins that era on the line after the
    /// input, naming them (`passed`), and that line tells each to its agent late. Not from the lines.
    pub passed_his: Vec<(u64, u32, String)>,
    /// The owner's hold on agents' mail (MAIL_HOLD), while it is on.
    pub mail_hold: Option<MailHold>,
    /// His switch of each agent's brain (BRAIN_SWITCH), from its last line: on or off, and since that line's time.
    pub brain_switched: BTreeMap<String, (bool, At)>,
    /// His sign-off of each of his asks (SIGNED_OFF), by its ticket and post, from its last line: his word of worth, and
    /// since that line's time. None taken back.
    pub signed_off: BTreeMap<(u64, u32), (String, At)>,
    /// The release his window runs and the one coordd runs, from the last [`WINDOW_RELEASE`] line read; none before
    /// the first.
    pub window_release: Option<WindowRelease>,
    /// What the record's own lines have shown, for the steps' `on`s and the facts they read ([`Seen`]): kept as each
    /// line is carried out (`Machine::line_answers`), live and on replay alike.
    #[serde(default)]
    pub seen: Seen,
    /// His posts an [`ASKED`] line has named, by ticket and post: none is raised again (`Machine::asked_value`).
    #[serde(default)]
    pub asks_raised: BTreeSet<(u64, u32)>,
    /// The goals, by ticket: each from the line at which a step of its first awaits a [`DUTIES`] line until it is
    /// closed, with its lines kept and its ask of its duties (`machine/duties.rs`).
    #[serde(default)]
    pub goal_lines: BTreeMap<u64, duties::Goal>,
    /// By his post's id, the lines of the input that handed in its first head, while the `asked` line coordd raises
    /// after it is still to come (`Machine::first_head_for`): the goal that line opens begins with them, so the head's
    /// opener and its `read` step are among its lines (`goal_lines_after`; paper-research N-1446 p2354).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub first_heads: BTreeMap<String, Vec<Value>>,
    /// How many times goals' duties were asked (`asked`, question `duties`), live and on replay: what p2204 counts.
    #[serde(default)]
    pub duties_asked: u64,
    /// Each goal's latest `duties` line, by ticket: the policy's answer as the line holds it, with the line's time
    /// (`at`), kept once the goal closes: its view's `duties` (`TicketView::duties`; paper-research N-1446 p2328, 2).
    #[serde(default)]
    pub duties_seen: BTreeMap<u64, Value>,
    /// The tickets in whose box the owner is writing (`syntax::WRITING`), each since its `on` line.
    pub writing: BTreeMap<u64, At>,
    /// Who can read a reply (paper-research N-17 post 5): the rule Reply names nobody else (a program agent, coord's
    /// own notices, a name coordd does not know). Told by coordd before each input; None (the tests): everyone. Never
    /// asked on replay, which takes the names from the recorded Replied line.
    pub readers: Option<BTreeSet<String>>,
    /// The seconds the policy holds the owner's typed posts before any agent is given them (`lapse-after("gather",
    /// N)`; the owner, N-1445 p9): told by coordd before each input, as READERS is; his `Input::Post` is written
    /// with it (`Line::Post`'s `hold`), and so are his message and UNTOLD (their value's `hold`, `hold_on`); a replay
    /// reads the line. None (the tests, a policy with none): not held.
    pub owner_hold: Option<u64>,
    /// The agents a turn's last message names with `@` (paper-research N-581.p82): AGENT and the names, told by coordd
    /// just before that turn's TurnEnded (checked against the team's norms there); the rule Reply names them beside
    /// `unanswered`. Taken by the next TurnEnded, whoever's; None on replay, which takes the names from the recorded
    /// Replied line, and in the tests unless set.
    pub reply_named: Option<(String, Vec<String>)>,
    /// The Reply's own names of the post `kept_reply` writes for a turn's end, the `@`s its message typed left out
    /// (`Line::Post`'s `answering`): set by `apply` for that post's completion alone.
    kept_answering: Option<Vec<String>>,
    /// What the turn ending did ([`Worked`], insight (1)): AGENT and its tools and model, told by coordd just before
    /// that turn's TurnEnded and cleared after it, from what its transcript's follower read; written on that agent's
    /// `turn_ended` line, on no other's. None on replay, which reads them from the recorded line, and in the tests
    /// unless set.
    pub turn_worked: Option<(String, Worked)>,
    /// The halts made so far, on every ticket (`Halt.seq`).
    halts_made: u64,
    /// Whether a halt of any ticket has gone (a [`Ticket::halt_gone`] set; it never clears): `view::stopped_owing`
    /// looks down a closed line's tree for one only then, a test made where a halt goes and not on each outline row
    /// (consult-3 N-1446 p585, 1). Rebuilt by replay.
    pub(crate) halt_went: bool,
    /// The scopes slow is set over (`Line::Slowed`): every ticket one covers halts as if it were slow (`covers`).
    pub slowed: Counted<BTreeSet<String>>,
    /// Scoped agents coordd could not start, with why (Line::StartFailed): a step owed to one raises NoWorker. A new
    /// start of it (a restart) clears it.
    unstarted: BTreeMap<String, String>,
    /// For each ticket, the last value given under each argument name to any of its steps (rebuilt by replay): what
    /// a decision's picked ticket gives a field while it is still open (`decided_fields`).
    step_args: BTreeMap<u64, Map<String, Value>>,
    /// Each decision answered (by a rule or by the owner), in order: the ticket it was asked on, its name, its
    /// arguments, the constructor chosen (rebuilt by replay; `decide_facts`' `earlier-C`).
    decided: Vec<(u64, String, Value, String)>,
    /// Each agent made while the record ran, in order (rebuilt by replay): a `made` line (`coord new`,
    /// `coord fork`) and a program's `let agent`, each with the ticket it was made on (none: an agent owing no step),
    /// who made it (an agent, or `program`) and the agent it copies, if any (item 11; `Machine::made`).
made: Vec<Made>,
    /// Agents' names the registry had that the record did not make (`Line::Taken`): never given to an agent the
    /// program makes.
    pub taken: BTreeSet<String>,
    /// The names of the agents the program made for steps and scopes on this record (`next_scoped`, rebuilt by replay).
    scoped_names: BTreeSet<String>,
    /// Each agent's last move between views (`Line::Moved`, rebuilt by replay; docket-edge N-238: `AgentView.moved`).
    pub moved: BTreeMap<String, LastMove>,
    /// Each agent's delivery (0.2, delivery.rs): its delivery ticket, the events waiting for its run, what is kept,
    /// reported and being handed. Rebuilt by replay; `waiting` and so `due`'s readers read it.
    pub(crate) desks: Counted<BTreeMap<String, Desk>>,
}

/// An agent's last move between views (`Machine::moved`): when, from where and to where (none: its own folder), why,
/// and the files it left uncommitted where it stood.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LastMove {
    pub at: At,
    pub from: Option<String>,
    pub to: Option<String>,
    pub why: String,
    pub left: Vec<String>,
}

/// An agent made (`Machine::made`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Made {
    pub agent: String,
    pub by: String,
    pub from: Option<String>,
    pub t: Option<u64>,
    /// The name the program made it under (`let agent NAME`): its label's first word (`label`).
    pub name: Option<String>,
}

impl Machine {
    /// Who holds ROLE, by the record's role lines (`Line::Role`: an agent's own role, and those the policy has it hold
    /// besides): the one answer to who holds a role, wherever the machine or coordd asks it.
    pub fn holders(&self, role: &str) -> Vec<String> {
        self.roles.iter().filter(|(a, r)| *r == role || self.also.get(*a).is_some_and(|x| x.iter().any(|r| r == role))).map(|(a, _)| a.clone()).collect()
    }

    /// The roles AGENT holds by the record: its own first, then those the policy gives it besides.
    /// Whether BY may tie or title X (WHAT): the owner, X's opener, or an agent holding the role `lead`.
    fn may_outline(&self, x: &Ticket, by: &str, what: &str) -> Result<(), String> {
        if by == "owner" || x.opener.as_deref() == Some(by) || self.roles_of(by).iter().any(|r| r == "lead") {
            return Ok(());
        }
        let opener = x.opener.as_deref().map(|o| format!(", its opener ({o})")).unwrap_or_default();
        Err(format!("{by} may not {what} {}: the owner{opener} or a lead may.", self.id(x.t)))
    }

    /// The thread T stands under in the outline: its latest tie (none when untied), else the ticket it was opened under
    /// or by. [`Machine::outline_parent`] without the post its line stands at, which scans that thread's posts: a walk up
    /// the outline over many tickets (insight (2)'s, under coordd's lock; consult-3 N-1446 p674, 3) takes this.
    pub fn outline_up(&self, t: u64) -> Option<u64> {
        let x = self.tickets.get(&t)?;
        match x.tie {
            Some(tie) => tie.map(|(p, _)| p),
            None => x.under.or(x.parent),
        }
    }

    /// The thread T stands under in the outline ([`Machine::outline_up`]), and the post of it at which its line stands:
    /// the tie's, else that ticket's last post before T opened.
    pub fn outline_parent(&self, t: u64) -> Option<(u64, u32)> {
        let x = self.tickets.get(&t)?;
        let p = self.outline_up(t)?;
        let n = match x.tie {
            Some(tie) => tie?.1,
            None => self.tickets.get(&p).map_or(0, |y| y.posts.iter().filter(|q| q.at <= x.opened_at).map(|q| q.n).max().unwrap_or(0)),
        };
        Some((p, n))
    }

    pub fn roles_of(&self, agent: &str) -> Vec<String> {
        self.roles.get(agent).into_iter().chain(self.also.get(agent).into_iter().flatten()).cloned().collect()
    }

    /// What reads the machine's state may keep its answer for as long as this stays (coordd's kept `due_all`,
    /// `Tickets::machine_due`, a2h-speed N-560 p37): counted up at every input put and every replay (`apply`,
    /// `replay_with`) and at every write to what `due` reads besides (the tickets with their posts and statuses,
    /// `slowed`, `defs` through `id`), which are [`Counted`] and so written only through `edit`.
    pub fn changes(&self) -> u64 {
        self.changes + self.defs.writes() + self.tickets.writes() + self.slowed.writes() + self.desks.writes()
    }

    /// F read with DEFS in place of the machine's definitions, then its own put back: a reload tried, not made (the
    /// owner's Reach, and the checks a reload makes before it is taken). Nothing is counted (`changes` stays), and what
    /// `due` kept stays as it was: anything F made it keep, read with DEFS, is dropped. DEFS are given back, and the
    /// machine's own put back on every way out of F, a panic too (consult-3 N-1396 p308, 4).
    pub fn with_defs<R>(&mut self, defs: Defs, f: impl FnOnce(&Machine) -> R) -> (R, Defs) {
        let cache = |m: &Machine| m.unhanded.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        let kept = cache(self);
        let mine = std::mem::replace(&mut self.defs.v, defs);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(self)));
        let defs = std::mem::replace(&mut self.defs.v, mine);
        cache(self);
        *self.unhanded.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = kept;
        match r {
            Ok(r) => (r, defs),
            Err(p) => std::panic::resume_unwind(p),
        }
    }

    /// Start this machine's count above OTHER's, for a machine that replaces OTHER (coordd's `Tickets::replayed`):
    /// nothing kept from OTHER is taken for this one.
    pub fn start_above(&mut self, other: &Machine) {
        self.changes += other.changes();
    }

    pub fn load(defs: Defs) -> Machine {
        let next = defs.first.max(1);
        Machine { defs: Counted::new(defs), changes: 0, roles: BTreeMap::new(), also: BTreeMap::new(), tickets: Counted::new(BTreeMap::new()), unhanded: std::sync::Mutex::new(None), arounds: std::sync::Mutex::new(None), lapsing: BTreeSet::new(), templates: Vec::new(), next, hidden_past: HIDDEN, desk_past: DESK, not_loaded: BTreeSet::new(), untold: BTreeSet::new(), handlers_unloaded: Vec::new(), gathering: BTreeMap::new(), counts: BTreeMap::new(), clocks: BTreeMap::new(), kinds_edited: BTreeMap::new(), calls_edited: BTreeMap::new(), closed_in_log: BTreeMap::new(), reloaded_at: BTreeMap::new(), replaying: None, asked_kinds: Default::default(), asked_closed: BTreeSet::new(), given_since: BTreeMap::new(), missed: BTreeMap::new(), into_since: BTreeMap::new(), carried: BTreeMap::new(), owed: BTreeMap::new(), stopped_by_him: BTreeSet::new(), stopped_his: BTreeMap::new(), held_handed: false, era: 0, passed_his: Vec::new(), mail_hold: None, brain_switched: BTreeMap::new(), signed_off: BTreeMap::new(), window_release: None, seen: Seen::default(), asks_raised: BTreeSet::new(), goal_lines: BTreeMap::new(), first_heads: BTreeMap::new(), duties_asked: 0, duties_seen: BTreeMap::new(), writing: BTreeMap::new(), readers: None, owner_hold: None, reply_named: None, kept_answering: None, turn_worked: None, halts_made: 0, halt_went: false, slowed: Counted::new(BTreeSet::new()), unstarted: BTreeMap::new(), step_args: BTreeMap::new(), decided: Vec::new(), made: Vec::new(), taken: BTreeSet::new(), scoped_names: BTreeSet::new(), moved: BTreeMap::new(), desks: Counted::new(BTreeMap::new()) }
    }

    /// Some binding is on event NAME: coordd sends its own events (TURN_END, COMPACTING) and the machine raises OWNER_TO
    /// only then, and a runner's event no binding is on is kept on its ticket.
    pub fn bound(&self, name: &str) -> bool {
        self.defs.bindings.iter().any(|b| b.ev == name)
    }

    /// How many events `every N of` binding B counts before it fires, as TIMES says where N comes from: as written,
    /// VALUE's field, the field of the result of the last ticket B fired when that closed done, or the first of an `or`
    /// that gives one. None: none does.
    fn count_times(&self, b: &syntax::Binding, times: &syntax::Times, value: &Value) -> Option<u64> {
        let whole = |v: Option<&Value>| v.and_then(Value::as_u64).filter(|n| *n > 0);
        match times {
            syntax::Times::N(n) => Some(u64::from(*n)),
            syntax::Times::Field { f, .. } => whole(value.get(f)),
            syntax::Times::Last { f } => match self.tickets.values().rev().find(|x| x.binding.as_deref() == Some(b.text.as_str())).map(|x| &x.status) {
                Some(Status::Done(r)) => whole(r.get(f)),
                _ => None,
            },
            syntax::Times::Or(a, c) => self.count_times(b, a, value).or_else(|| self.count_times(b, c, value)),
        }
    }

    /// The clock bindings due at NOW (never fired, or their time gone since they last did), by their text: coordd
    /// feeds each back as the event `every` with that text.
    pub fn clocks_due(&self, now: &str) -> Vec<String> {
        self.defs.bindings.iter().filter(|b| self.clock_due(b, now)).map(|b| b.text.clone()).collect()
    }

    /// The `when D is o` bindings, each with what the policy is shown to decide D (plan/layers.md I §3): its text, D,
    /// o, and the record's factors as the decision's call fields. OPEN: the tickets of the binding's kind open now;
    /// SINCE: minutes since it last fired (absent if never); for each kind K, CLOSED-K: its tickets closed done since
    /// then (since the start if never), OLDEST-K: minutes since the first of those closed, and FAILED-K: its tickets
    /// that failed or were cancelled since then. WAITING-K: its tickets closed done that no ticket of the binding's kind
    /// closed done has carried (named in what came back to one of its steps), whenever they closed, and OLDEST-WAITING-K:
    /// minutes since the first of those closed; so what a failed binding ticket carried waits again at once
    /// (paper-research N-101 post 166, docket-core3's C-225). Asked by coordd every
    /// minute, as `expired` is, in the facts of where the open tickets stand (`standing`); an answer of o comes back as
    /// the event `when` (`Input::Event`).
    pub fn when_asks(&self, now: &str) -> Vec<(String, String, String, BTreeMap<String, String>)> {
        let mins = |a: &str| matches!((secs(now), secs(a)), (Some(n), Some(l)) if n >= l).then(|| (secs(now).unwrap() - secs(a).unwrap()) / 60);
        let mut out = Vec::new();
        for b in &self.defs.bindings {
            let Every::When { decision, option } = &b.every else { continue };
            let last = self.clocks.get(&b.text);
            let mut f = BTreeMap::new();
            f.insert("open".to_string(), self.tickets.values().filter(|x| x.kind == b.kind && x.is_open() && x.handling.is_none()).count().to_string());
            if let Some(m) = last.and_then(|l| mins(l)) {
                f.insert("since".to_string(), m.to_string());
            }
            // closed done and failed apart: a failed piece is no reason to cut a build (N-101's stand-in run, B-8)
            let mut closed: BTreeMap<&str, (u64, &str)> = BTreeMap::new();
            let mut failed: BTreeMap<&str, u64> = BTreeMap::new();
            for x in self.tickets.values().filter(|x| x.handling.is_none() && x.t < HIDDEN) {
                let Some(c) = x.closed_at.as_deref() else { continue };
                if last.is_some_and(|l| c <= l.as_str()) {
                    continue;
                }
                if !matches!(x.status, Status::Done(_)) {
                    *failed.entry(x.kind.as_str()).or_insert(0) += 1;
                    continue;
                }
                let e = closed.entry(x.kind.as_str()).or_insert((0, c));
                e.0 += 1;
                if c < e.1 {
                    e.1 = c;
                }
            }
            for (k, (n, first)) in closed {
                f.insert(format!("closed-{k}"), n.to_string());
                if let Some(m) = mins(first) {
                    f.insert(format!("oldest-{k}"), m.to_string());
                }
            }
            for (k, n) in failed {
                f.insert(format!("failed-{k}"), n.to_string());
            }
            // carried: the tickets named in what came back to the steps of a ticket of the binding's kind closed done
            let mut carried: BTreeSet<String> = BTreeSet::new();
            for y in self.tickets.values().filter(|y| y.kind == b.kind && matches!(y.status, Status::Done(_))) {
                for v in y.trace.visits.values().filter_map(|v| v.value.as_ref()) {
                    strings_in(v, &mut carried);
                }
            }
            // waiting: closed done, carried by no build closed done, and still wanted by what opened it. A ticket
            // opened by another waits only while that one is open: once it has closed, the piece was accepted (built
            // green on the line), taken out (its plan raised), or its change was cancelled or replanned, and it never
            // counts again (docket-core3's item 11, N-101 p432: an Out piece and one carried under another name
            // counted for ever, and an empty build was cut every minute)
            let mut waiting: BTreeMap<&str, (u64, &str)> = BTreeMap::new();
            for x in self.tickets.values().filter(|x| x.handling.is_none() && x.t < HIDDEN && x.kind != b.kind && matches!(x.status, Status::Done(_))) {
                let Some(c) = x.closed_at.as_deref() else { continue };
                if carried.contains(&self.id(x.t)) {
                    continue;
                }
                if x.parent.is_some_and(|p| !self.tickets.get(&p).is_some_and(Ticket::is_open)) {
                    continue;
                }
                let e = waiting.entry(x.kind.as_str()).or_insert((0, c));
                e.0 += 1;
                if c < e.1 {
                    e.1 = c;
                }
            }
            for (k, (n, first)) in waiting {
                f.insert(format!("waiting-{k}"), n.to_string());
                if let Some(m) = mins(first) {
                    f.insert(format!("oldest-waiting-{k}"), m.to_string());
                }
            }
            out.push((b.text.clone(), decision.clone(), option.clone(), f));
        }
        out
    }

    /// The facts a `when D is o` binding's decision is asked in, beside `when_asks`' fields, the same for every binding:
    /// where the open tickets stand (the dev program's lack 10, proto-mind N-1446 p1502, p1505: a cut goes when a head
    /// that answers one of his posts stands at the cut's hold, and waits while an engine head is in its merge). For each
    /// open ticket whose program waits at a step or a hold (`Awaits`, or `Asking` while the policy fills its role or
    /// decides it), STANDS(T, CALL), CALL as the step's `Owed::call` names it (`cut_of`, `decide:green`), and
    /// STANDS-WITH(T, PARAM, VALUE) for each argument of that step that is a string (a commit, the `commit` of
    /// `cut_of(commit, merge)`). For each open ticket that hands in for a post (`handed_in_for`, lack 8: its form's
    /// `for` names the post, or an item whose ask or note it is), HANDS-IN(T, POST, BY), BY the post's writer
    /// (`owner` for his). T and POST as `Machine::id` writes them. Not the hidden tickets, nor a run of a handler. A
    /// project that reads them declares them in its own policy (coordd supplies only the facts a policy declares), so
    /// an older release, which states none, still loads it.
    pub fn standing(&self) -> Vec<(String, Vec<String>)> {
        let mut out = Vec::new();
        for (t, x, at) in self.open_steps() {
            let id = self.id(t);
            if let Some(owed) = at {
                out.push(("stands".to_string(), vec![id.clone(), owed.call.clone()]));
                for (p, v) in owed.args.as_object().into_iter().flatten() {
                    if let Some(s) = v.as_str() {
                        out.push(("stands-with".to_string(), vec![id.clone(), p.clone(), s.to_string()]));
                    }
                }
            }
            if let Some((u, n)) = self.handed_in_for(&x.form) {
                let by = self.tickets.get(&u).and_then(|y| y.posts.iter().find(|p| p.n == n)).map(|p| p.by.clone()).unwrap_or_default();
                out.push(("hands-in".to_string(), vec![id, format!("{}.p{n}", self.id(u)), by]));
            }
        }
        out
    }

    /// The open tickets `standing` and `stands` read, by number, each with the step or hold its program waits at now
    /// (`Awaits`, or `Asking` while its role is filled or a decision asked), none when it waits at neither: not the
    /// hidden tickets, nor a handler's run.
    fn open_steps(&self) -> impl Iterator<Item = (u64, &Ticket, Option<&Owed>)> {
        self.tickets.range(..HIDDEN).filter(|(_, x)| x.is_open() && x.handling.is_none()).map(|(t, x)| {
            let at = match &x.status {
                Status::Awaits { owed, .. } | Status::Asking { owed, .. } => Some(owed),
                _ => None,
            };
            (*t, x, at)
        })
    }

    /// `stands` as a `when` binding reads it as it fires (`syntax::STANDS`; the dev program's lack 5, proto-mind N-1446
    /// p1502): for each call, the ids of the open tickets that stand at a step or a hold of it now (STANDS of
    /// `standing`), by number; `[]` at a call none stands at, so every `stands.CALL` the check lets through is a list.
    fn stands(&self) -> Value {
        let mut at: Map<String, Value> = self.defs.calls.keys().filter(|c| syntax::is_ident(c)).map(|c| (c.clone(), json!([]))).collect();
        for (t, _, owed) in self.open_steps() {
            if let Some(Value::Array(ids)) = owed.and_then(|o| at.get_mut(&o.call)) {
                ids.push(json!(self.id(t)));
            }
        }
        Value::Object(at)
    }

    /// When clock binding B is due, in seconds from the epoch: `i64::MIN` (due at any time) when it never fired by its
    /// text (an edited binding is a new text), else its period after it last did. None for a binding that is no clock,
    /// or whose last firing does not read. The one rule `clock_due` and `clock_next` read (consult-3 N-1396 p317, 2).
    fn clock_due_secs(&self, b: &syntax::Binding) -> Option<i64> {
        let Every::Clock(d) = b.every else { return None };
        match self.clocks.get(&b.text) {
            None => Some(i64::MIN),
            Some(last) => secs(last).map(|l| l + d),
        }
    }

    fn clock_due(&self, b: &syntax::Binding, now: &str) -> bool {
        match self.clock_due_secs(b) {
            None => false,
            Some(i64::MIN) => true,
            Some(due) => secs(now).is_some_and(|n| n >= due),
        }
    }

    /// When clock binding B fires next, read at NOW: NOW when `clock_due` says it is due, else when it will be; none for
    /// a binding that is no clock.
    pub fn clock_next(&self, b: &syntax::Binding, now: &str) -> Option<At> {
        let due = self.clock_due_secs(b)?;
        Some(if self.clock_due(b, now) { now.to_string() } else { at_secs(due) })
    }

    /// The number the next ticket opened will have.
    pub fn next_number(&self) -> u64 {
        self.next
    }

    /// The number of the next hidden ticket, but for the message runtime's (DESK).
    pub fn next_hidden(&self) -> u64 {
        self.tickets.range(HIDDEN..DESK).next_back().map_or(HIDDEN, |(m, _)| m + 1).max(self.hidden_past)
    }

    /// The number of the message runtime's next hidden ticket.
    pub fn next_desk(&self) -> u64 {
        self.tickets.range(DESK..).next_back().map_or(DESK, |(m, _)| m + 1).max(self.desk_past)
    }

    /// Line L of the record, replayed: a number it gave a ticket, or a card on one (`Halted`), stays taken whether
    /// this machine made it or not, so nothing here hands it out again. A ticket it cannot replay (0.3's aside, in a
    /// program it does not read; a hidden ticket of a kind it does not have) keeps its number, and a card it did not
    /// make (0.3's burst card) keeps its number on its ticket (paper-research N-1446 p327; proto-lead p346.3; proto-
    /// reader p347, consult-3 p349). On a record this machine replays whole it changes nothing: every number on it
    /// was made here, so each counter is past it already. A ticket a binding fired has its number on the event alone
    /// (`Fired`, no `Open` or `Opened` line: 0.3's `on owner_to(e) open …`, a binding this machine may not have), and
    /// an imported item's is on its `Open` (consult-3 p430).
    fn number_taken(&mut self, l: &Line) {
        let ts: Vec<u64> = match l {
            Line::Open { t, .. } | Line::Opened { t, .. } => vec![*t],
            Line::Event { name, value, fired, .. } => {
                let desk = value.get("t").and_then(Value::as_u64).filter(|_| name == syntax::DELIVERY_OPENED);
                fired.iter().map(|f| f.t).chain(desk).collect()
            }
            Line::Halted { t, n, .. } => {
                if self.tickets.get(t).is_some_and(|x| x.halted < *n) {
                    self.tickets.edit().get_mut(t).unwrap().halted = *n;
                }
                return;
            }
            _ => return,
        };
        for t in ts {
            let past = if t < HIDDEN { &mut self.next } else if t < DESK { &mut self.hidden_past } else { &mut self.desk_past };
            *past = (*past).max(t + 1);
        }
    }

    /// The posts of his that coordd sends on after a replay (UNTOLD; proto-lead N-1446 p431, 2): each whose handler
    /// this release did not load, that no line told, and that nothing holds or has routed since.
    pub fn untold_posts(&self) -> Vec<(u64, u32)> {
        self.untold.iter().filter(|(t, n)| self.tickets.get(t).and_then(|x| x.posts.iter().find(|p| p.n == *n)).is_some_and(|p| p.held.is_none() && !p.routed && p.told.is_empty())).copied().collect()
    }

    /// Why INPUT is refused before it runs on replay (the way back, proto-lead N-1446 p398, clause 1): it is on a
    /// ticket the record opened that this release does not load (`not_loaded`): an open under one, or an input whose
    /// every ticket is one. None when it is not.
    fn on_not_loaded(&self, input: &Line) -> Option<String> {
        if let Line::Open { under, .. } = input {
            return under.filter(|u| self.not_loaded.contains(u)).map(|u| format!("There is no ticket {u} to open it under."));
        }
        let ts = line_tickets(input);
        (!ts.is_empty() && ts.iter().all(|t| self.not_loaded.contains(t))).then(|| format!("There is no ticket {}.", ts[0]))
    }

    /// Line L, which an input on a ticket this release does not load recorded (`replay_with`, the way back, clause
    /// 1): a ticket it opened is not loaded either; a line on no ticket loaded here is passed over; on one loaded, a
    /// post is added with the number, author, step and whom it names and informs recorded, a `Told` tells that post as
    /// recorded, a `Halted` of a delivery halts the post whose line is just before it, or else the run of `Told` lines
    /// just before it, a card with the number recorded (one this release's count has passed is a problem, and the
    /// card is this release's next), and any other `Halted` has its number taken (`number_taken`; its card is not on
    /// the line). Any other line on a loaded ticket is a problem, said in what this answers.
    fn as_recorded(&mut self, cx: &mut Cx, l: &Line) -> Option<String> {
        let opened = match l {
            Line::Open { t, .. } | Line::Opened { t, .. } => Some(*t),
            Line::Event { name, value, .. } if name == syntax::DELIVERY_OPENED => value.get("t").and_then(Value::as_u64),
            _ => None,
        };
        if let Some(t) = opened.filter(|t| !self.tickets.contains_key(t)) {
            self.not_loaded.insert(t);
            return None;
        }
        if let Line::Closed { t, .. } = l {
            self.gathering.remove(t);
        }
        if !line_tickets(l).iter().any(|t| self.tickets.contains_key(t)) {
            return None;
        }
        match l {
            Line::Posted { at, t, n, by, step, names, inform, into_turn } => {
                let x = self.tickets.edit().get_mut(t).unwrap();
                if *n != x.last() + 1 {
                    return Some(format!("the record has post {n} on {t} from an input on a ticket this release does not load, where its next post is {}", x.last() + 1));
                }
                // the record has no words of a post a program made: it keeps its place, number and author, and goes to
                // whom it names and informs as the program's `post` sends it (proto-mind N-1446 p802)
                let text = Text::New(String::from("(A post made under a later release. Its words are not on the record.)"));
                x.posts.push(Post { n: *n, at: at.clone(), by: by.clone(), text, head: None, step: *step, names: names.clone(), told: Vec::new(), auto: false, moved: None, from: None, held: None, halted: false, dropped: false, routed: false, asks: Vec::new(), asks_open: false, asks_closed: None, answers: None, inform: inform.clone(), typed: None, cleaned_from: None, split_from: None, on: Vec::new(), also: Vec::new(), would_close: None, taken_back: false, joins: None, passes: None, comments: Vec::new(), into_turn: into_turn.clone(), kind: None, answering: Vec::new() });
                for a in names.iter().chain(inform) {
                    cx.effect(Effect::Deliver { to: a.clone() });
                }
                cx.effect(Effect::Changed { t: *t });
                None
            }
            Line::Told { t, n, to, inform, joins, .. } => {
                let Some(p) = self.tickets.edit().get_mut(t).unwrap().posts.iter_mut().find(|p| p.n == *n) else {
                    return Some(format!("the record tells post {n} of {t}, which is not there, from an input on a ticket this release does not load"));
                };
                p.held = None;
                p.routed = true;
                p.told = to.clone();
                p.inform = inform.clone();
                p.joins = *joins;
                for a in to.iter().chain(inform) {
                    cx.effect(Effect::Deliver { to: a.clone() });
                }
                cx.effect(Effect::Changed { t: *t });
                self.untold.remove(&(*t, *n));
                None
            }
            // the posts its writer halted for his approval on a slow ticket, as the record gives them on T just before
            // it: `halt_post`'s one post, whose `posted` line is just before it, or else `tell_held`'s, the run of
            // `told` lines just before it, never a mix (proto-reader N-1446 p947, p1025; consult-3 p1026); they reach
            // nobody until he lets them go; `deliver` is `Halting::Deliver`'s name. A run of `told` lines only part of
            // which the card holds (a split told in part by a rule) is a shape no writer makes: every tell of one
            // input halts or none does (consult-3 p1029); the writers' order is kept by the burst test in gather.rs
            // and `a_halted_delivery_comes_right_after_the_lines_of_what_it_holds` (proto-lead p1044)
            Line::Halted { t, n, what, .. } if what == "deliver" => {
                let r = cx.recorded.clone().unwrap_or_default();
                let before = r.iter().position(|x| x == l).map_or(&r[..0], |i| &r[..i]);
                let mut ns: Vec<u32> = match before.last() {
                    Some(Line::Posted { t: u, n, .. }) if u == t => vec![*n],
                    _ => before.iter().rev().map_while(|x| match x { Line::Told { t: u, n, .. } if u == t => Some(*n), _ => None }).collect(),
                };
                ns.reverse();
                let x = self.tickets.edit().get_mut(t).unwrap();
                ns.retain(|m| x.posts.iter().any(|p| p.n == *m));
                let Some(&first) = ns.first() else { return None };
                // its card's number as recorded, the one his `approve` names: a count here already past it is a
                // problem, never a quiet other number (consult-3 N-1446 p1026), and the posts are halted all the same,
                // under this release's next card (`halt` numbers it), so none reaches an agent without his approval
                // (consult-3 p1047, proto-reader p1048, proto-lead p1049); `number_taken` moves the count after the
                // input
                let clash = (x.halted >= *n).then(|| format!("the record halts card {n} on {t} from an input on a ticket this release does not load; it is card {} here", x.halted + 1));
                for p in x.posts.iter_mut().filter(|p| ns.contains(&p.n)) {
                    p.halted = true;
                }
                if clash.is_none() {
                    x.halted = n - 1;
                }
                self.halt(cx, *t, Halting::Deliver { post: first, with: ns[1..].to_vec(), edited: false, chose: None });
                clash
            }
            Line::Halted { .. } => None,
            _ => Some(format!("the record has {} on a ticket this release loads, from an input on one it does not", show(std::slice::from_ref(l)))),
        }
    }

    /// The ticket's id as shown: the kind's first letter, a dash, the number (`C-12`).
    /// `view::owner_unhanded`, kept until a ticket is written (`Counted::writes`).
    pub fn unhanded(&self) -> std::sync::Arc<crate::view::Unhanded> {
        kept_unwritten(&self.unhanded, self.tickets.writes(), || crate::view::owner_unhanded(self))
    }

    /// Every ticket's threads around it (`Arounds`), kept until a ticket is written (`Counted::writes`).
    fn arounds(&self) -> std::sync::Arc<Arounds> {
        kept_unwritten(&self.arounds, self.tickets.writes(), || {
            let mut all = Arounds::default();
            for y in self.tickets.values() {
                if let Some(id) = y.form.get("follows").and_then(Value::as_str) {
                    all.follows.entry(id.to_string()).or_default().push(y.t);
                }
                if let Some(Some((t, n))) = y.tie {
                    if t != y.t {
                        all.tied.entry(t).or_default().push((n, y.t));
                    }
                }
            }
            all
        })
    }

    /// Who opens what a program on ticket P opens, for a kind's `opener`: the agent whose open is P or a ticket above
    /// it (walking up), else P itself, as that program's own post with no `as` is. Never the owner: the only words
    /// posted as his are those he gave himself (an "owner" opener above is passed over).
    fn program_author(&self, p: u64) -> String {
        let mut u = Some(p);
        while let Some(y) = u.and_then(|u| self.tickets.get(&u)) {
            match &y.opener {
                Some(o) if o != "owner" => return o.clone(),
                _ => u = y.parent,
            }
        }
        self.id(p)
    }

    pub fn id(&self, t: u64) -> String {
        let x = self.tickets.get(&t);
        let k = x.and_then(|x| self.defs.kinds.get(&x.kind));
        let letter = k.and_then(|k| k.letter).or_else(|| x.and_then(|x| x.kind.chars().next())).map_or('T', |c| c.to_ascii_uppercase());
        let width = k.map_or(0, |k| k.width);
        format!("{letter}-{t:0width$}")
    }

    /// The ticket whose id is ID exactly as [`Machine::id`] writes it (`N-412`, `I-001`). There is none for a number
    /// that no ticket has, nor for a ticket's number written another way: `412`, `n-412`, `I-1`, or an agent's name
    /// that reads as one (`consult-3`, `a2`; the era [`ERA_NAMES`]).
    pub fn ticket_named(&self, id: &str) -> Option<u64> {
        parse_ticket(id).filter(|t| self.tickets.contains_key(t) && self.id(*t) == id)
    }

    /// Apply INPUT at time NOW.
    pub fn apply(&mut self, input: Input, now: At) -> Out {
        self.passed_his.clear();
        let mut out = self.apply_kept(input, now.clone());
        // the era ERA_CLOSES, begun just after the first post that goes to an agent otherwise by the ledger's close,
        // before anything reads whom it goes to (its `push`, read when a delivery event is raised): a replay reads the
        // post, then the era's line, as it was
        if self.era < ERA_CLOSES && !out.lines.is_empty() {
            let ts: BTreeSet<u64> = out.lines.iter().flat_map(line_tickets).collect();
            let at = &now;
            let made: Vec<(u64, u32)> = ts.into_iter().flat_map(|t| self.tickets.get(&t).into_iter().flat_map(move |x| x.posts.iter().filter(move |p| p.at == *at).map(move |p| (t, p.n)))).collect();
            if made.iter().any(|(t, n)| !self.close_changes(*t, *n).is_empty()) {
                let era = self.apply_one(Input::Event { name: ERA.into(), value: json!({"era": ERA_CLOSES}) }, now);
                out.lines.extend(era.lines);
                out.effects.extend(era.effects);
            }
        }
        out
    }

    /// `apply`, with a turn's reply that the rule before the runtime counts answered written first (`kept_reply`).
    fn apply_kept(&mut self, input: Input, now: At) -> Out {
        // a turn's reply to a post kept from it that the rule before the runtime counts answered: written first as the
        // agent's own post, so that the engine before it replays the record as it was written (delivery.rs)
        if let Some((post, answering)) = self.kept_reply(&input) {
            self.kept_answering = Some(answering);
            let mut out = self.apply_one(post, now.clone());
            self.kept_answering = None;
            if out.reply.is_ok() {
                let then = self.apply_one(input, now);
                out.lines.extend(then.lines);
                out.effects.extend(then.effects);
                // the turn's end finds the post and answers nothing more: what was said is the post
                if then.reply.is_ok() {
                    out.reply = then.reply;
                }
                return out;
            }
        }
        self.apply_one(input, now)
    }

    fn apply_one(&mut self, input: Input, now: At) -> Out {
        self.changes += 1;
        let _duties = check::duties(self.defs.duties);
        let line = match self.complete(input, &now) {
            Ok(l) => l,
            Err(e) => return Out { reply: Err(e), ..Out::default() },
        };
        let mut cx = Cx { now, lines: Vec::new(), effects: Vec::new(), recorded: None };
        let (logged, reply) = self.exec(&line, &mut cx);
        let mut lines = Vec::new();
        if logged {
            // an input to a delivery run (its step given to the watcher) is not written either: replay makes the run
            // again from the runtime's `event` lines, and an engine without it would find no such ticket (`exec`)
            if !self.delivery_line(&line) {
                lines.push(line);
            }
            lines.append(&mut cx.lines);
        }
        Out { lines, effects: cx.effects, reply }
    }

    /// Replay the log LINES at start. Gives what diverged (an empty list when the log and the machine agree); the
    /// tickets concerned are marked broken.
    pub fn replay(&mut self, lines: &[Line]) -> Vec<String> {
        self.replay_with(lines, &mut |_, _, _| {})
    }

    /// [`Machine::replay`], AFTER called with the machine as it stands after each input of LINES: the input's index and
    /// the input with its recorded outputs (coordd's joint compares with it, N-568 part 5). Three parts: what the whole
    /// of LINES says ahead of each line ([`Machine::look_ahead`]), the lines run in order, and the end
    /// ([`Machine::replay_end`]); a replay stopped at a cut ([`Machine::replay_to`]) and gone on from there
    /// ([`Machine::replay_on`]) runs the same three.
    pub fn replay_with(&mut self, lines: &[Line], after: &mut dyn FnMut(&Machine, usize, &[Line])) -> Vec<String> {
        let problems = self.replay_to(lines, after);
        self.replay_end();
        problems
    }

    /// A replay stopped at a cut (coordd's saved replay, I-1509): LINES, the record from its first line to the cut,
    /// replayed as [`Machine::replay_with`] replays them, all but its end. The machine is then what a replay of the whole
    /// record holds after those lines, as long as the lines after the cut do not answer differently what these looked up
    /// ahead (`asked_kinds`, `asked_closed`; [`Machine::reaches_back`]). Gives what diverged, as `replay_with`.
    pub fn replay_to(&mut self, lines: &[Line], after: &mut dyn FnMut(&Machine, usize, &[Line])) -> Vec<String> {
        self.changes += 1;
        // the log is what happened: how it leaves each ticket closed, kept even where the machine now differs
        self.closed_in_log.clear();
        self.reloaded_at.clear();
        self.not_loaded.clear();
        self.untold.clear();
        self.handlers_unloaded.clear();
        self.gathering.clear();
        self.asked_closed.clear();
        self.asked_kinds.lock().unwrap_or_else(|e| e.into_inner()).clear();
        self.look_ahead(lines, 0);
        self.replay_lines(lines, 0, after)
    }

    /// A replay stopped at a cut ([`Machine::replay_to`], or one read back) gone on with TAIL, the record's lines after
    /// it, the first of them the record's line FROM (from 0), up to a cut again: AFTER is called with each input's
    /// index in the record. Err, the machine untouched, when the tail answers differently what the lines before it
    /// looked up ahead ([`Machine::reaches_back`]): only a replay from the record's first line gives what those lines do
    /// then. Its end ([`Machine::replay_end`]) is the caller's, at the record's last line.
    pub fn replay_on(&mut self, tail: &[Line], from: usize, after: &mut dyn FnMut(&Machine, usize, &[Line])) -> Result<Vec<String>, String> {
        if let Some(why) = self.reaches_back(tail, from) {
            return Err(why);
        }
        self.look_ahead(tail, from);
        Ok(self.replay_lines(tail, from, after))
    }

    /// Whether the log leaves T an agents' thread (closed `Between`), as the replay's end ([`Machine::replay_end`])
    /// closes it whatever the machine holds of it before: coordd's saved replay keeps the joint's history of the
    /// tickets held at a cut (I-1509), and one the end makes a thread is held.
    pub fn log_leaves_between(&self, t: u64) -> bool {
        self.closed_in_log.get(&t).is_some_and(|(result, raised)| raised.is_none() && result.as_ref().and_then(|v| v.get("head")).and_then(Value::as_str) == Some("Between"))
    }

    /// Why TAIL (from the record's line FROM, from 0) changes what the lines before it, replayed, did or looked up
    /// ahead: an output first, of an input before the cut; a reload of a kind they took as no reload changed after
    /// them (`with_kinds_as_they_were`); or a line that changes whether the log leaves closed a ticket whose recorded
    /// program they found no longer checks (`unchecked`). None: the lines before it did as a replay of the whole
    /// record does.
    pub fn reaches_back(&self, tail: &[Line], from: usize) -> Option<String> {
        // a cut inside an input's group: its outputs after the cut are the group's, which the lines before ran without
        if tail.first().is_some_and(|l| !l.is_input()) {
            return Some(format!("line {} is an output of an input before the cut", from + 1));
        }
        let kinds = self.asked_kinds.lock().unwrap_or_else(|e| e.into_inner());
        tail.iter().enumerate().find_map(|(i, l)| {
            let at = from + i + 1;
            match l {
                Line::Reload { kinds: named, .. } => named.iter().find(|k| kinds.contains(*k)).map(|k| format!("line {at} reloads kind {k}, which a line before the cut took as no reload changed after it")),
                Line::Closed { t, .. } | Line::Undo { t, .. } | Line::Reopen { t, .. } => self.asked_closed.contains(t).then(|| format!("line {at} changes whether the log leaves {} closed, which a line before the cut asked", self.id(*t))),
                Line::Edited { t, what, .. } if (what == "repair" || what == "rekind") && self.asked_closed.contains(t) => Some(format!("line {at} changes whether the log leaves {} closed, which a line before the cut asked", self.id(*t))),
                Line::Keep { ts, .. } => ts.first().filter(|t| self.asked_closed.contains(t)).map(|t| format!("line {at} changes whether the log leaves {} closed, which a line before the cut asked", self.id(*t))),
                _ => None,
            }
        })
    }

    /// What LINES (the first the record's line FROM, from 0) say ahead of the lines before them: the last reload of
    /// each kind, and how the log leaves each ticket closed. Taken over the whole record before a replay runs a line,
    /// and over a tail before a replay goes on with it, so a replay to a cut and on from it has the record's.
    fn look_ahead(&mut self, lines: &[Line], from: usize) {
        for (i, l) in lines.iter().enumerate() {
            let i = from + i;
            match l {
                Line::Reload { kinds, .. } => {
                    for k in kinds {
                        self.reloaded_at.insert(k.clone(), i);
                    }
                }
                Line::Closed { t, result, raised, .. } => {
                    self.closed_in_log.insert(*t, (result.clone(), raised.clone()));
                }
                Line::Edited { t, what, .. } if what == "repair" || what == "rekind" => {
                    self.closed_in_log.remove(t);
                }
                // an answer at a hole undone: a ticket it closed rests again; an item reopened
                Line::Undo { t, .. } | Line::Reopen { t, .. } => {
                    self.closed_in_log.remove(t);
                }
                // the first of a run of one-offs kept rests again
                Line::Keep { ts, .. } => {
                    if let Some(t) = ts.first() {
                        self.closed_in_log.remove(t);
                    }
                }
                _ => {}
            }
        }
    }

    /// LINES run in order as a replay runs them, the first the record's line FROM (from 0): each input with its
    /// recorded outputs, AFTER called after it with its index in the record. Gives what diverged, each at its line in
    /// the record.
    fn replay_lines(&mut self, lines: &[Line], from: usize, after: &mut dyn FnMut(&Machine, usize, &[Line])) -> Vec<String> {
        let _duties = check::duties(self.defs.duties);
        let mut problems = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            let input = &lines[i];
            let mut j = i + 1;
            while j < lines.len() && !lines[j].is_input() {
                j += 1;
            }
            let recorded = lines[i + 1..j].to_vec();
            // the input's place in the record
            let n = from + i;
            if !input.is_input() {
                problems.push(format!("line {}: an output with no input before it", n + 1));
                i = j;
                continue;
            }
            let at = line_at(input);
            self.replaying = Some(n);
            let mut cx = Cx { now: at, lines: Vec::new(), effects: Vec::new(), recorded: Some(recorded.clone()) };
            // the way back (clause 1): an input on a ticket this release does not load is refused for that alone, and
            // does not run
            let on = self.on_not_loaded(input);
            let took = on.is_some();
            let (logged, reply) = match on {
                Some(why) => (false, Err(why)),
                None => self.exec(input, &mut cx),
            };
            // a handler it opened that this release does not load (clause 1's handler, `handler_ticket`)
            let handlers = std::mem::take(&mut self.handlers_unloaded);
            // and so is the open of a kind it does not load (`exec_line`)
            let took = took || matches!(input, Line::Open { t, .. } if self.not_loaded.contains(t));
            let why = if !logged {
                Some(format!("the input is now refused: {}", reply.err().unwrap_or_default()))
            } else if !alike(&cx.lines, &recorded) {
                Some(format!("the machine now makes {} where the log has {}", show(&cx.lines), show(&recorded)))
            } else {
                None
            };
            if took {
                // its refusal is the problem line, and no ticket is broken for it; of the lines it recorded, those on
                // tickets loaded here are taken as recorded, posts, `Told` and `Halted`, and any other is a problem
                problems.push(format!("line {}: {}", n + 1, why.unwrap_or_default()));
                if let Line::Open { t, .. } = input {
                    self.not_loaded.insert(*t);
                }
                for (k, l) in recorded.iter().enumerate() {
                    if let Some(why) = self.as_recorded(&mut cx, l) {
                        let msg = format!("line {}: {why}", n + 2 + k);
                        for t in line_tickets(l) {
                            if let Some(x) = self.tickets.edit().get_mut(&t) {
                                x.broken.get_or_insert(msg.clone());
                            }
                        }
                        problems.push(msg);
                    }
                }
            } else if let Some(rest) = Some(&handlers).filter(|h| logged && !h.is_empty()).and_then(|_| unmade(&cx.lines, &recorded)) {
                // its one allowed line is that handler's open, and nothing is broken for it; what the record has that
                // the machine did not make (the handler's run) is taken as `as_recorded` takes an unloaded input's
                let ids: Vec<String> = handlers.iter().map(|c| self.id(*c)).collect();
                problems.push(format!("line {}: the handler the record opens at it ({}) is not loaded: this release cannot parse its recorded program.", n + 1, ids.join(", ")));
                for l in &rest {
                    if let Some(why) = self.as_recorded(&mut cx, l) {
                        let msg = format!("line {}: {why}", n + 1);
                        for t in line_tickets(l) {
                            if let Some(x) = self.tickets.edit().get_mut(&t) {
                                x.broken.get_or_insert(msg.clone());
                            }
                        }
                        problems.push(msg);
                    }
                }
            } else if let Some(why) = why {
                let msg = format!("line {}: {why}", n + 1);
                for t in line_tickets(input).into_iter().chain(recorded.iter().flat_map(line_tickets)) {
                    if let Some(x) = self.tickets.edit().get_mut(&t) {
                        x.broken.get_or_insert(msg.clone());
                    }
                }
                problems.push(msg);
            }
            // every number the record gave, at its input: opened here or refused, it stays taken
            lines[i..j].iter().for_each(|l| self.number_taken(l));
            after(self, n, &lines[i..j]);
            i = j;
        }
        self.replaying = None;
        problems
    }

    /// The end of a replay, at the record's last line ([`Machine::replay_with`]; after [`Machine::replay_on`]): what the
    /// log says ahead is let go, an open ticket that would open a kind that changed since it checked against it is
    /// marked, and a ticket the log closed is closed as the log says.
    pub fn replay_end(&mut self) {
        let _duties = check::duties(self.defs.duties);
        self.reloaded_at.clear();
        self.asked_closed.clear();
        self.asked_kinds.lock().unwrap_or_else(|e| e.into_inner()).clear();
        // an open ticket that would still open a kind it checked against as it was: it would open it with the fields
        // it had, so it no longer checks
        let stale: Vec<(u64, String)> = self
            .tickets
            .values()
            .filter(|x| x.is_open() && !x.loose.is_empty() && !self.closed_in_log.contains_key(&x.t))
            .filter_map(|x| self.opens_ahead(x.t).intersection(&x.loose).next().map(|k| (x.t, format!("it would open a {k}, whose form, result or raises changed since its text was checked"))))
            .collect();
        for (t, why) in stale {
            self.unchecked(t, &why);
        }
        // a ticket the log closed is closed as the log says, where the machine now leaves it open or ends it otherwise
        for (t, (result, raised)) in std::mem::take(&mut self.closed_in_log) {
            let Some(x) = self.tickets.edit().get_mut(&t) else { continue };
            let logged = match raised {
                Some(r) => Status::Failed(r),
                None => Status::Done(result.unwrap_or(Value::Null)),
            };
            // what an answer carries is not on the record (answers.rs): the same result without it is the same
            let same = match (&x.status, &logged) {
                (Status::Done(v), Status::Done(w)) => answers::plain(v.clone()) == *w,
                (s, l) => s == l,
            };
            if !same {
                let was_open = x.is_open();
                x.status = logged;
                x.k.clear();
                x.live.clear();
                x.kept.clear();
                x.holding = None;
                x.any = None;
                x.broken.get_or_insert_with(|| format!("closed as the log says; the machine now {}", if was_open { "leaves it open" } else { "ends it otherwise" }));
            }
        }
    }

    /// What must be done again after a replay: the questions in flight, the heads being formed, the deliveries due.
    pub fn resume_effects(&self) -> Vec<Effect> {
        let mut out = Vec::new();
        let mut agents = BTreeSet::new();
        for x in self.tickets.values() {
            // a policy answer halted for the owner is not asked again
            if x.halts.iter().any(|h| matches!(h.what, Halting::Decide { .. })) {
                agents.extend(self.recipients(x));
                continue;
            }
            match &x.status {
                Status::Asking { owed, role } if role == violated::ROLE => out.extend(self.violated_question(x.t, owed.step)),
                Status::Asking { owed, role } => out.push(Effect::Ask { t: x.t, step: owed.step, question: Question::Fills { role: role.clone(), call: owed.call.clone() } }),
                _ if x.holding.is_some() => out.push(Effect::Ask { t: x.t, step: x.closes + 1, question: Question::Holds { goals: x.goals.clone() } }),
                Status::Awaits { agent, owed, forming: Some(loc), .. } => {
                    out.push(Effect::Form { t: x.t, step: owed.step, agent: agent.clone(), call: owed.call.clone(), head: owed.head.clone(), text: loc.clone(), words: None })
                }
                Status::Awaits { agent, owed, .. } if x.owner_words.is_some() => {
                    let words = x.owner_words.as_ref().and_then(|(_, w, ..)| if let Text::New(w) = w { Some(w.clone()) } else { None });
                    out.push(Effect::Form { t: x.t, step: owed.step, agent: agent.clone(), call: owed.call.clone(), head: owed.head.clone(), text: Loc { file: String::new(), offset: 0, len: 0 }, words })
                }
                _ => {}
            }
            agents.extend(self.recipients(x));
        }
        // the goals' asks of their duties that stand (machine/duties.rs)
        out.extend(self.goal_lines.keys().filter_map(|g| self.duties_question(*g)));
        out.extend(agents.into_iter().filter(|a| !self.due(a).is_empty()).map(|to| Effect::Deliver { to }));
        // what the delivery program handed and no `given` line says is in: handed again (delivery.rs)
        out.extend(self.handing_effects());
        out
    }

    // ---------------------------------------------------------------- queries

    /// T's view; a closed run's (`coord show` of it) on its form as it was given, read back from its text (`form_text`).
    pub fn view(&self, t: u64) -> Option<crate::view::TicketView> {
        let x = self.tickets.get(&t)?;
        if x.form_text.is_none() {
            return Some(crate::view::TicketView::of(self, x));
        }
        let mut y = x.clone();
        y.unpack_form();
        Some(crate::view::TicketView::of(self, &y))
    }

    /// Who takes part in T, in order of first appearance: its opener, then each post's author and those it names.
    /// What `--thread` names (paper-research m883).
    pub fn participants(&self, t: u64) -> Vec<String> {
        let Some(x) = self.tickets.get(&t) else { return Vec::new() };
        let mut out: Vec<String> = x.opener.iter().cloned().collect();
        for p in &x.posts {
            for a in std::iter::once(&p.by).chain(&p.names) {
                if !out.contains(a) {
                    out.push(a.clone());
                }
            }
        }
        out
    }

    /// Whether post N of T is his own words (proto-lead N-1446 p83, p105; consult-3 p88.2, p110): by the owner, and
    /// not the words an agent opened T with, which thread.kind before ls-opener posted again as his (its `post text as
    /// owner` for a Named open: N-1445 #2, paper-research's opener, record lines 103218-103219; today's posts them once,
    /// as the opener's): a post by the owner whose words are post 1's, when post 1 is an agent's, is not his. The one
    /// test of it, for the holder's report (coordd's route job: YOURS, its cut, the fallback's last post, RECENT's
    /// names) and the brain's reading.
    pub fn his_words(&self, t: u64, n: u32) -> bool {
        let Some(x) = self.tickets.get(&t) else { return false };
        let Some(p) = x.posts.iter().find(|p| p.n == n) else { return false };
        if p.by != "owner" {
            return false;
        }
        match x.posts.iter().find(|q| q.n == 1) {
            Some(first) if n != 1 && first.by != "owner" => self.text(&first.text) != self.text(&p.text),
            _ => true,
        }
    }

    /// The tickets AGENT is on, open or resting at a hole: it opened one, posted on it, was named or told there, or holds
    /// its step. An owner's post goes bare only to an agent on that ticket alone (design-two-systems §4; docket-edge
    /// m1259: lead, on two threads, guessed which one 'go ahead' was for).
    pub fn open_on(&self, agent: &str) -> Vec<u64> {
        self.tickets
            .values()
            .filter(|x| x.is_open() && x.handling.is_none())
            .filter(|x| matches!(&x.status, Status::Awaits { agent: a, .. } if a == agent) || x.opener.as_deref() == Some(agent) || x.posts.iter().any(|p| p.by == agent || p.names.iter().chain(&p.told).any(|n| n == agent)))
            .map(|x| x.t)
            .collect()
    }

    /// Open tickets, in number order (a ticket resting at a hole is not open work).
    pub fn open(&self) -> Vec<u64> {
        self.tickets.values().filter(|x| x.is_open() && x.handling.is_none() && !matches!(x.status, Status::Rests { .. })).map(|x| x.t).collect()
    }

    /// Every step owed or held, in ticket order, in one walk of the tickets: agent, ticket, the step. `owes` and `holds`
    /// are one agent's; coordd's moves read every agent's at once, once a round (moves.rs `Reads`).
    pub fn steps_awaited(&self) -> impl Iterator<Item = (&str, u64, &Owed)> + '_ {
        self.tickets.values().filter_map(|x| match &x.status {
            Status::Awaits { agent, owed, .. } => Some((agent.as_str(), x.t, owed)),
            _ => None,
        })
    }

    /// The steps AGENT owes: ticket, step, call.
    pub fn owes(&self, agent: &str) -> Vec<(u64, u32, String)> {
        self.steps_awaited().filter(|(a, _, owed)| *a == agent && !owed.hold).map(|(_, t, owed)| (t, owed.step, owed.call.clone())).collect()
    }

    /// The steps AGENT holds (`hold`): ticket, step, call. Owed, but nobody waits for them to be answered soon.
    pub fn holds(&self, agent: &str) -> Vec<(u64, u32, String)> {
        self.steps_awaited().filter(|(a, _, owed)| *a == agent && owed.hold).map(|(_, t, owed)| (t, owed.step, owed.call.clone())).collect()
    }

    /// The open tickets AGENT opened (what it waits on).
    pub fn waits(&self, agent: &str) -> Vec<u64> {
        self.tickets.values().filter(|x| x.is_open() && x.opener.as_deref() == Some(agent)).map(|x| x.t).collect()
    }

    /// The step held at give halt N of ticket T (slow mode), as its delivery would carry it: call, arguments, an
    /// inline call's text, the owner's once-edited template and the texts it marks. coordd renders the prompt the owner sees from these.
    pub fn halted_give(&self, t: u64, n: u32) -> Option<(String, Value, Option<String>, Option<String>, Vec<Text>)> {
        let x = self.tickets.get(&t)?;
        let h = x.halts.iter().find(|h| h.n == n)?;
        let Halting::Give { step, .. } = &h.what else { return None };
        let o = self.owed_give(t)?;
        if o.step != *step {
            return None;
        }
        Some((o.call, o.args, o.template, o.edit, o.refs))
    }

    /// The step ticket T waits on now (it awaits its answer), as its delivery would carry it (`halted_give`'s, at a
    /// halt): what coordd renders its prompt from, and the owner's dry run of a draft (`Request::DryRun`, H2).
    pub fn owed_give(&self, t: u64) -> Option<OwedGive> {
        let Status::Awaits { agent, owed, .. } = &self.tickets.get(&t)?.status else { return None };
        let refs = owed.refs.iter().map(|r| Text::of(&self.readable(r)).unwrap_or_else(|| Text::New(self.readable(r).as_str().unwrap_or("(a text that is not there)").to_string()))).collect();
        Some(OwedGive {
            step: owed.step,
            agent: agent.clone(),
            call: owed.call.clone(),
            args: self.readable(&owed.args),
            head: owed.head.clone(),
            template: owed.template.clone(),
            edit: owed.edit.clone(),
            prompt: owed.prompt.clone(),
            refs,
        })
    }

    /// What is due to AGENT now, oldest ticket first.
    pub fn due(&self, agent: &str) -> Vec<Delivery> {
        self.due_among(agent, self.tickets.values())
    }

    /// `due` of AGENT on the tickets XS alone: what `due` gives on them, each delivery being on the ticket that made it
    /// (`due_on`) and what is done over the whole only taking deliveries away. `quiet` asks it of a thread and what was
    /// raised from it, where a pass over every ticket for each agent on it, at each step for each resting thread with a
    /// minder, held the lock (paper-research N-1396 p424).
    pub fn due_among<'a>(&self, agent: &str, xs: impl Iterator<Item = &'a Ticket>) -> Vec<Delivery> {
        let mut out = Vec::new();
        for x in xs {
            let mut mine = self.due_on(x, agent);
            if mine.is_empty() {
                continue;
            }
            // a notice halted (slow mode): not given until the owner approves it
            mine.retain(|d| !x.halts.iter().any(|h| matches!(&h.what, Halting::Notice { id, to, .. } if *id == d.id && to == agent)) && !x.refused_notices.contains(&format!("{agent} {}", d.id)));
            self.hold_archived(x, &mut mine);
            out.append(&mut mine);
        }
        if out.is_empty() {
            return out;
        }
        // a waiting ask is the agent's held step: nothing else is handed meanwhile (m1171; no ticket stops a turn any
        // more, N-36 point 2, so none passes it)
        if self.waiting_ask(agent).is_some() {
            out.clear();
        }
        // a step waiting on its holder's late ask: nothing more of its ticket until the answer (paper-research m1218)
        if !out.is_empty() {
            let asking = self.asks_held_by();
            out.retain(|d| !asking.get(agent).is_some_and(|ts| ts.contains(&d.t)));
        }
        // posts it missed twice: until its next turn's end (0.2 A4)
        out.retain(|d| !self.missed_waits(agent, d));
        out
    }

    /// The agents with an ask waiting (`waiting_ask`), read in one pass: the pass over every agent's due asked each of
    /// them, a pass over every ticket for each (paper-research N-1396 p424).
    fn asks_waiting_by(&self) -> BTreeSet<&str> {
        self.tickets.values().filter_map(|x| x.ask.as_ref()).filter(|a| a.waiting && !a.replied).map(|a| a.by.as_str()).collect()
    }

    /// The tickets whose step waits on its holder's late ask (`asks_on`), by the asking agent, read in one pass over
    /// the tickets: `asks_on` makes two for each delivery it is asked of, and the pass over every agent's due asked it
    /// of each of hundreds, most of the pass (paper-research N-1396 p417).
    fn asks_held_by(&self) -> BTreeMap<&str, BTreeSet<u64>> {
        let mut out: BTreeMap<&str, BTreeSet<u64>> = BTreeMap::new();
        for a in self.tickets.values().filter(|y| y.is_open()).filter_map(|y| y.ask.as_ref()).filter(|a| !a.replied) {
            out.entry(a.by.as_str()).or_default().insert(a.held);
        }
        for (_, q) in self.held_asks() {
            out.entry(q.ask.by.as_str()).or_default().insert(q.ask.held);
        }
        out
    }

    /// `due` of each of AGENTS, in one pass over the tickets (paper-research N-101 p1797): of each ticket only the
    /// agents it can have anything due for are asked (`due_on`), the rest have nothing there by `due_on`'s own clauses:
    /// the agent its step awaits, words given to, a withdrawal for, its opener and those told of its close, and the
    /// agents a post not yet seen by them wakes or informs. Each agent's list is what `due` gives it.
    pub fn due_all(&self, agents: &[String]) -> BTreeMap<String, Vec<Delivery>> {
        let asked: BTreeSet<&str> = agents.iter().map(String::as_str).collect();
        let mut out: BTreeMap<String, Vec<Delivery>> = agents.iter().map(|a| (a.clone(), Vec::new())).collect();
        for x in self.tickets.values() {
            for a in self.may_be_due(x).into_iter().filter(|a| asked.contains(a)) {
                let mut mine = self.due_on(x, a);
                if mine.is_empty() {
                    continue;
                }
                mine.retain(|d| !x.halts.iter().any(|h| matches!(&h.what, Halting::Notice { id, to, .. } if *id == d.id && to == a)));
                self.hold_archived(x, &mut mine);
                out.get_mut(a).unwrap().append(&mut mine);
            }
        }
        let asking = self.asks_held_by();
        let waiting = self.asks_waiting_by();
        for (a, due) in out.iter_mut() {
            if waiting.contains(a.as_str()) {
                due.clear();
            }
            let held = asking.get(a.as_str());
            due.retain(|d| !held.is_some_and(|ts| ts.contains(&d.t)) && !self.missed_waits(a, d));
        }
        out
    }

    /// The agents ticket X can have anything due for (`due_all`): the agent its step awaits, words given to, a
    /// withdrawal for, those not yet told of a take-back of a post they had (0.2 B8), its opener and those told of its
    /// close, those told it is archived, and the agents a post not yet seen by them wakes or informs. The others have nothing there by `due_on`'s own clauses.
    fn may_be_due<'a>(&self, x: &'a Ticket) -> BTreeSet<&'a str> {
        let mut may: BTreeSet<&str> = BTreeSet::new();
        if let Status::Awaits { agent, .. } = &x.status {
            may.insert(agent);
        }
        may.extend(x.gives.iter().map(|g| g.to.as_str()));
        may.extend(x.withdrawn.iter().map(|w| w.0.as_str()));
        may.extend(x.struck.iter().flat_map(|s| s.to.iter().filter(|a| !s.told.contains(*a)).map(String::as_str)));
        may.extend(x.told_late.iter().filter(|e| !e.given).map(|e| e.to.as_str()));
        if !x.is_open() {
            may.extend(x.opener.as_deref());
            may.extend(x.told_closed.iter().map(String::as_str));
        }
        may.extend(x.told_archived.keys().map(String::as_str));
        let upto = x.seen_upto();
        for p in x.posts.iter().filter(|p| p.n <= upto && p.moved.is_none() && p.visible()) {
            for n in p.wakes().chain(p.inform.iter()) {
                if !may.contains(n.as_str()) && p.by != *n && p.n > x.mu.get(n.as_str()).copied().unwrap_or(0) {
                    may.insert(n);
                }
            }
        }
        may
    }

    /// Ticket X archived, or under one the owner archived: nothing of it is handed (MINE keeps only the notice that he
    /// archived it), and it is kept as it is, to be handed as it would have been at Unarchive (paper-research
    /// N-867.p21; docket-core7 N-867.p25). A step held so is not silent either: tickets.eflint's `expired` asks
    /// `archived`.
    fn hold_archived(&self, x: &Ticket, mine: &mut Vec<Delivery>) {
        if self.archived_by(x.t).is_some() {
            mine.retain(|d| d.id.rsplit_once('.').is_some_and(|(_, s)| s == ARCHIVED_NOTICE));
        }
    }

    /// The ticket the owner archived that T is, or is under (its parent, or the ticket it runs under, and so on up),
    /// worked out at each call, as `slow_by` is: a ticket that is no longer under it is handed again with no line of
    /// its own (docket-core3, N-867.p27, 2).
    pub fn archived_by(&self, t: u64) -> Option<u64> {
        let mut u = t;
        let mut seen = BTreeSet::new();
        loop {
            let x = self.tickets.get(&u)?;
            if x.archived {
                return Some(u);
            }
            if !seen.insert(u) {
                return None;
            }
            u = x.parent.or(x.under)?;
        }
    }

    /// The owner unarchives T: it is as it was, and what was held at or under it is handed now, as it would have been.
    fn unarchive(&mut self, cx: &mut Cx, t: u64) {
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.archived = false;
        x.told_archived.clear();
        cx.effect(Effect::Changed { t });
        let mut woken = BTreeSet::new();
        for u in self.tickets.keys().copied().filter(|u| self.archived_by(*u).is_none() && self.at_or_under(*u, t)) {
            let y = &self.tickets[&u];
            woken.extend(self.may_be_due(y).into_iter().filter(|a| self.due_on(y, a).iter().any(|d| !d.inform)).map(str::to_string));
        }
        for a in woken {
            cx.effect(Effect::Deliver { to: a });
        }
    }

    /// Who holds work at or under T, told once when the owner archives it (the policy's `works-under`, event
    /// "archived"): each agent a step at or under T awaits, and each that opened an open ticket under it. Not the owner.
    pub fn works_under(&self, t: u64) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for x in self.tickets.values().filter(|x| x.is_open() && x.handling.is_none() && self.at_or_under(x.t, t)) {
            if let Status::Awaits { agent, .. } = &x.status {
                out.insert(agent.clone());
            }
            if x.t != t {
                out.extend(x.opener.clone());
            }
        }
        out.remove("owner");
        out
    }

    /// Whether U is T, or under it (`archived_by`'s walk).
    fn at_or_under(&self, u: u64, t: u64) -> bool {
        let (mut v, mut seen) = (u, BTreeSet::new());
        loop {
            if v == t {
                return true;
            }
            match self.tickets.get(&v).and_then(|x| x.parent.or(x.under)) {
                Some(w) if seen.insert(v) => v = w,
                _ => return false,
            }
        }
    }

    /// What ticket X has due for AGENT, notices halted or not.
    fn due_on(&self, x: &Ticket, agent: &str) -> Vec<Delivery> {
        let mut out = Vec::new();
        {
            // the ticket's id, built at the first delivery made: most tickets have none for AGENT, and building it for
            // each of them at each delivery event was a fifth of research's replay (docket-edge5, N-1446 p2877)
            let id_once = std::cell::OnceCell::new();
            let id = || id_once.get_or_init(|| self.id(x.t)).as_str();
            let mk = |suffix: String, upto: u32, what: What| Delivery { id: format!("{}.{suffix}", id()), to: agent.to_string(), t: x.t, ticket: id().to_string(), kind: x.kind.clone(), upto, what, inform: false };
            let mut carries_posts = false;
            if let Status::Awaits { agent: a, owed, taken, refused, reminded, back, forming, .. } = &x.status {
                // a hold is never delivered: its worker answers it when it chooses, and a refusal is in the reply
                // a step halted before it is given (slow mode): not delivered until the owner approves it
                let halted = x.halts.iter().any(|h| matches!(h.what, Halting::Give { step, .. } if step == owed.step));
                // a step halted before it is given carries the posts it would carry: they wait with it and are given
                // once, in the step (paper-research N-101 p1015: the owner's words went alone to orch, which answered
                // them before the step came)
                if a == agent && !owed.hold && halted && !taken && !owed.blind {
                    carries_posts = true;
                }
                if a == agent && !owed.hold && !halted {
                    // a reminder's why: a reading's sentence, or that its turn was left without the answer
                    let why = |again: bool| x.remind_why.as_ref().filter(|(s, _)| again && *s == owed.step).map(|(_, w)| w);
                    let step = |unread: Vec<Post>, again: bool| What::Step { step: owed.step, call: owed.call.clone(), args: self.readable(&owed.args), unread, blind: owed.blind, head: owed.head.clone(), again, template: owed.template.clone(), prompt: owed.prompt.clone(), edit: owed.edit.clone(), refs: owed.refs.iter().map(|r| Text::of(&self.readable(r)).unwrap_or_else(|| Text::New(self.readable(r).as_str().unwrap_or("(a text that is not there)").to_string()))).collect(), why: match why(again) { Some(RemindWhy::Read(w)) => Some(w.clone()), _ => None }, left: why(again) == Some(&RemindWhy::Left), passed: x.passed(owed.step, a) };
                    let mu = x.mu.get(agent).copied().unwrap_or(0);
                    if !taken {
                        let unread = if owed.blind { Vec::new() } else { self.with_words(self.shown(x.unread(agent))) };
                        let upto = if owed.blind { mu } else { x.seen_upto() };
                        out.push(mk(format!("s{}", owed.step), upto, step(unread, false)));
                        carries_posts = !owed.blind;
                    } else if *reminded == Some(false) && forming.is_none() {
                        // not while an answer to it is being read: what is given again then waits for the reading
                        // (paper-research N-101 p999, 2a: the step given again for an ask's answer went out in the
                        // second the answer's reading began, and read as "could not be read")
                        out.push(mk(format!("s{}.again", owed.step), mu, step(Vec::new(), true)));
                    }
                    if *taken && *back == Some(false) {
                        out.push(mk(format!("s{}.back", owed.step), mu, step(Vec::new(), false)));
                    }
                    if let Some((why, false)) = refused {
                        if let Status::Awaits { refusals, .. } = &x.status {
                            let what = What::Refused { step: owed.step, call: owed.call.clone(), why: why.clone(), left: 3 - refusals, head: owed.head.clone() };
                            out.push(mk(format!("s{}.refused{refusals}", owed.step), mu, what));
                        }
                    }
                }
            }
            // words given to the agent (`give`): the words alone, from whoever opened the ticket
            for (i, g) in x.gives.iter().enumerate() {
                if g.to == agent && !g.given {
                    let from = x.opener.clone().unwrap_or_else(|| String::from("owner"));
                    out.push(mk(format!("w{}", i + 1), x.mu.get(agent).copied().unwrap_or(0), What::Words { text: g.text.clone(), from }));
                }
            }
            // the owner archived it: told once (`Line::Notified`, event "archived"), in the words alone
            if let Some((quiet, false)) = x.told_archived.get(agent).filter(|_| x.archived) {
                let text = Text::New(format!("The owner archived {}: it is no longer of importance to him. Nothing at or under it is handed to you while it is archived; a step of it you owe waits, kept, and comes again if he unarchives it. Nothing on it is closed or taken back.", id()));
                out.push(Delivery { inform: *quiet, ..mk(ARCHIVED_NOTICE.to_string(), x.mu.get(agent).copied().unwrap_or(0), What::Words { text, from: "owner".into() }) });
            }
            for (a, step, call, given, edit) in &x.withdrawn {
                if a == agent && !given {
                    let (by, why) = edit.clone().unzip();
                    out.push(mk(format!("s{step}.withdrawn"), x.mu.get(agent).copied().unwrap_or(0), What::Withdrawn { step: *step, call: call.clone(), by, why }));
                }
            }
            // a post it had, taken back since (0.2 B8, N-1396): told once, whether it works or not
            for s in x.struck.iter().filter(|s| s.to.iter().any(|a| a == agent) && !s.told.iter().any(|a| a == agent)) {
                out.push(mk(format!("p{}.taken_back", s.n), x.mu.get(agent).copied().unwrap_or(0), What::TakenBack { n: s.n, by: s.by.clone() }));
            }
            if !x.is_open() && (x.opener.as_deref() == Some(agent) || x.told_closed.contains(agent)) && !x.closed_given.contains(&format!("{agent} {}", x.close_notice())) {
                let (result, raised) = match &x.status {
                    Status::Done(v) => (Some(v.clone()), None),
                    Status::Failed(r) => (None, Some(r.clone())),
                    _ => unreachable!(),
                };
                let inform = x.informed.contains(agent) && x.opener.as_deref() != Some(agent);
                out.push(Delivery { inform, ..mk(x.close_notice(), x.seen_upto(), What::Closed { result, raised, unread: self.with_words(self.shown(x.unread(agent))) }) });
                carries_posts = true;
            }
            if !carries_posts {
                let mu = x.mu.get(agent).copied().unwrap_or(0);
                let upto = x.seen_upto();
                let unread = || x.posts.iter().filter(|p| p.n > mu && p.n <= upto && p.by != agent && p.moved.is_none() && p.visible() && !x.late_for(agent, p.n));
                if unread().any(|p| p.wakes().any(|n| n == agent)) {
                    out.push(mk(format!("p{upto}"), upto, What::Posts { unread: self.shown(x.posts_unread(agent)) }));
                } else if unread().any(|p| p.inform.iter().any(|n| n == agent)) {
                    // only informed (paper-research m1237): there at its next turn, starting none
                    out.push(Delivery { inform: true, ..mk(format!("p{upto}"), upto, What::Posts { unread: self.shown(x.posts_unread(agent)) }) });
                }
            }
            // his post told to it after it waited on his question (`ToldLate`): that post alone, on its own, after the
            // posts above (proto-lead N-1446 p569, (c2); p576); given, it moves no mark, as a take-back's notice
            for e in x.told_late.iter().filter(|e| e.to == agent && !e.given) {
                if let Some(p) = x.posts.iter().find(|p| p.n == e.n && p.visible() && p.moved.is_none()) {
                    let mu = x.mu.get(agent).copied().unwrap_or(0);
                    out.push(mk(format!("p{}.{TOLD}", e.n), mu, What::Posts { unread: self.shown(vec![p.clone()]) }));
                }
            }
        }
        out
    }

    /// On a slow thread, each delivery now due that would start an agent's turn and has no halt of its own (a step
    /// given halts as `give`, a post as `deliver`) halts as a `notice`: the closing notice, a reminder, a withdrawal,
    /// a refusal, words given (paper-research N-89.p2, 2). Run after every input, before coordd asks what is due.
    fn halt_notices(&mut self, cx: &mut Cx) {
        let slow: Vec<u64> = self.tickets.keys().copied().filter(|t| self.slow(*t)).collect();
        for t in slow {
            let x = &self.tickets[&t];
            let mut agents = self.recipients(x);
            agents.extend(x.told_closed.iter().cloned());
            agents.extend(x.gives.iter().map(|g| g.to.clone()));
            agents.remove("owner");
            let mut new = Vec::new();
            let mut due: BTreeSet<(String, String)> = BTreeSet::new();
            for a in agents {
                for d in self.due_on(x, &a) {
                    due.insert((d.id.clone(), a.clone()));
                    let starts = !d.inform && matches!(d.what, What::Step { again: true, .. } | What::Closed { .. } | What::Withdrawn { .. } | What::Refused { .. } | What::Words { .. });
                    let key = format!("{a} {}", d.id);
                    if starts && !x.let_through.contains(&key) && !x.refused_notices.contains(&key) && !x.halts.iter().any(|h| matches!(&h.what, Halting::Notice { id, to, .. } if *id == d.id && *to == a)) {
                        new.push(Halting::Notice { id: d.id, to: a.clone(), what: d.what });
                    }
                }
            }
            // a notice halted that is no longer due goes (a reminder of a step answered since; N-101's stand-in run)
            let x = self.tickets.edit().get_mut(&t).unwrap();
            let before = x.halts.len();
            x.halts.retain(|h| !matches!(&h.what, Halting::Notice { id, to, .. } if !due.contains(&(id.clone(), to.clone()))));
            if x.halts.len() != before {
                x.halt_gone = Some(cx.now.clone());
                self.halt_went = true;
                cx.effect(Effect::Changed { t });
            }
            for h in new {
                self.halt(cx, t, h);
            }
        }
    }

    /// Text T as it can be read: a part of a ticket's text becomes the span of that text it names (a new text's
    /// words, or the place within a place). None when the ticket or its text is not there, or the span is outside it.
    pub fn text(&self, t: &Text) -> Option<Text> {
        if let Text::Post { post: PostRef { t, n } } = t {
            let p = self.tickets.get(t)?.posts.iter().find(|p| p.n == *n)?;
            return self.text(&p.text);
        }
        if let Text::Joined { joined } = t {
            return joined.iter().map(|p| self.text(p)).collect::<Option<Vec<_>>>().map(|joined| Text::Joined { joined });
        }
        let Text::Part { part: Part { t, at, len } } = t else { return Some(t.clone()) };
        let base = self.tickets.get(t).and_then(|x| x.form.get("text")).and_then(Text::of)?;
        let (at, len) = (*at as usize, *len as usize);
        match base {
            Text::New(s) => s.get(at..at.checked_add(len)?).map(|w| Text::New(w.to_string())),
            // the place's decoded text, which only the surfaces read: cut there
            Text::Loc { loc } => at.checked_add(len).map(|_| Text::Span { span: Span { loc, at: at as u64, len: len as u64 } }),
            _ => None,
        }
    }

    /// V as it is shown (step args, a view's form and result): each part of a ticket's text in it becomes the text it
    /// names (Machine::text), or words saying it is not there; everything else as it is.
    pub fn readable(&self, v: &Value) -> Value {
        // what answers carry stays in the engine (N-101, answers.rs)
        let v = &answers::plain(v.clone());
        match v {
            Value::Object(o) if o.len() == 1 && o.contains_key("post") => match Text::of(v) {
                Some(t @ Text::Post { .. }) => {
                    let Text::Post { post } = &t else { unreachable!() };
                    self.text(&t).map(|t| t.value()).unwrap_or_else(|| Value::String(format!("(post {} of {}, which is not there)", post.n, self.id(post.t))))
                }
                _ => v.clone(),
            },
            Value::Object(o) if o.len() == 1 && o.contains_key("part") => match Text::of(v) {
                Some(t @ Text::Part { .. }) => {
                    let Text::Part { part } = &t else { unreachable!() };
                    self.text(&t).map(|t| t.value()).unwrap_or_else(|| Value::String(format!("(a part of {}'s text that is not there)", self.id(part.t))))
                }
                _ => v.clone(),
            },
            Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), self.readable(x))).collect()),
            Value::Array(xs) => Value::Array(xs.iter().map(|x| self.readable(x)).collect()),
            _ => v.clone(),
        }
    }

    /// POSTS as they are shown (views, deliveries): their parts resolved.
    pub fn shown(&self, mut posts: Vec<Post>) -> Vec<Post> {
        for p in &mut posts {
            if let Text::Part { part } = &p.text {
                p.text = self.text(&p.text).unwrap_or_else(|| Text::New(format!("(a part of {}'s text that is not there)", self.id(part.t))));
            } else if let Text::Post { post } = &p.text {
                p.text = self.text(&p.text).unwrap_or_else(|| Text::New(format!("(post {} of {}, which is not there)", post.n, self.id(post.t))));
            } else if let Text::Joined { .. } = &p.text {
                p.text = self.text(&p.text).unwrap_or_else(|| Text::New("(a text one of whose pieces is not there)".into()));
            }
        }
        posts
    }

    /// Of POSTS, those with words, as a step or a close carries them: a post with no text (an answer given in the app
    /// with its fields only) is not shown as "NAME wrote:" over nothing (paper-research N-101 p1008, 1).
    fn with_words(&self, mut posts: Vec<Post>) -> Vec<Post> {
        posts.retain(|p| !matches!(&p.text, Text::New(s) if s.trim().is_empty()));
        posts
    }

    /// The open tickets below T that await someone, and whom: what a post on a waiting ticket should name.
    pub fn leaves(&self, t: u64) -> Vec<(u64, String, String)> {
        let mut out = Vec::new();
        let Some(x) = self.tickets.get(&t) else { return out };
        match &x.status {
            Status::Awaits { agent, owed, .. } => out.push((t, agent.clone(), owed.call.clone())),
            Status::Waits { children, .. } => children.iter().for_each(|c| out.extend(self.leaves(*c))),
            _ => {}
        }
        out
    }

    fn recipients(&self, x: &Ticket) -> BTreeSet<String> {
        let mut s: BTreeSet<String> = x.posts.iter().flat_map(|p| p.names.iter().cloned()).collect();
        if let Status::Awaits { agent, .. } = &x.status {
            s.insert(agent.clone());
        }
        if let Some(o) = &x.opener {
            s.insert(o.clone());
        }
        s.extend(x.withdrawn.iter().map(|w| w.0.clone()));
        s
    }

    /// Every step owed now: ticket, step, agent, since when (what `expired` is asked about).
    pub fn awaiting(&self) -> Vec<(u64, u32, String, At)> {
        self.tickets
            .values()
            .filter_map(|x| match &x.status {
                // time halted is not waiting (slow mode): a step held for the owner is not asked about
                Status::Awaits { agent, owed, since, .. } if !owed.hold && x.halts.is_empty() => Some((x.t, owed.step, agent.clone(), since.clone())),
                _ => None,
            })
            .collect()
    }

    /// Whether T halts (slow mode): it or a ticket above it (its parent, or the ticket it runs under) is switched to
    /// slow; the ticket whose switch it is.
    pub fn slow_by(&self, t: u64) -> Option<u64> {
        let mut u = t;
        loop {
            let x = self.tickets.get(&u)?;
            if x.slow {
                return Some(u);
            }
            u = x.parent.or(x.under)?;
        }
    }

    /// Whether T halts (slow mode): slow is set over a scope that covers it, or a ticket at or above T is switched.
    pub fn slow(&self, t: u64) -> bool {
        self.slow_scope(t).is_some() || self.slow_by(t).is_some()
    }

    /// The scope slow is set over that covers T, if one does (`Line::Slowed`).
    pub fn slow_scope(&self, t: u64) -> Option<&str> {
        self.slowed.iter().find(|s| covers(s, t)).map(String::as_str)
    }

    /// Whether a ticket binding BINDING opens on event NAME with VALUE is slow: what made it fire came from a slow
    /// thread. An event about a ticket (its `ticket`, or a runner's `name`) that halts; for `when D is o`, a ticket
    /// closed done since the binding last fired that halts (its closes are what the policy was shown). A clock's
    /// binding (`every D`) comes from no thread.
    fn fired_slow(&self, name: &str, value: &Value, binding: &str) -> bool {
        if name == "when" {
            let last = self.clocks.get(binding);
            return self.tickets.values().any(|x| x.handling.is_none() && matches!(x.status, Status::Done(_)) && x.closed_at.as_deref().is_some_and(|c| last.is_none_or(|l| c > l.as_str())) && self.slow(x.t));
        }
        ["ticket", "name"].iter().filter_map(|k| value.get(*k).and_then(Value::as_str)).filter_map(parse_ticket).any(|t| self.tickets.contains_key(&t) && self.slow(t))
    }

    /// The layers active now: each `layer` scope on an open ticket's frames, outermost first: (id, ticket, on).
    pub fn active_layers(&self) -> Vec<(String, u64, Option<String>)> {
        let mut out = Vec::new();
        for x in self.tickets.values().filter(|x| x.is_open()) {
            for f in &x.k {
                if let Frame::Layer { id, on } = f {
                    out.push((id.clone(), x.t, on.clone()));
                }
            }
        }
        out
    }

    /// The layer a step of T runs in: the innermost on T's frames, else on its parent's (a child opened or a branch
    /// of a `par` inside a layer shares its workspace).
    pub fn layer_of(&self, t: u64) -> Option<String> {
        let mut u = t;
        loop {
            let x = self.tickets.get(&u)?;
            if let Some(id) = x.k.iter().rev().find_map(|f| if let Frame::Layer { id, .. } = f { Some(id.clone()) } else { None }) {
                return Some(id);
            }
            u = x.parent?;
        }
    }

    /// Whether U is T or below it (by parent or `under`).
    fn is_under(&self, u: u64, t: u64) -> bool {
        let mut v = u;
        loop {
            if v == t {
                return true;
            }
            match self.tickets.get(&v).and_then(|x| x.parent.or(x.under)) {
                Some(w) => v = w,
                None => return false,
            }
        }
    }

    /// Halt WHAT on T: numbered, logged (`Halted`), shown.
    fn halt(&mut self, cx: &mut Cx, t: u64, what: Halting) {
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.halted += 1;
        let n = x.halted;
        cx.line(Line::Halted { at: cx.now.clone(), t, n, what: what.name().into() });
        self.halts_made += 1;
        x.halts.push(Halt { n, at: cx.now.clone(), what, seq: self.halts_made });
        cx.effect(Effect::Changed { t });
    }

    /// Post N of T, written by someone other than the owner: on a slow ticket it halts before anyone is given it.
    fn halt_post(&mut self, cx: &mut Cx, t: u64, n: u32) {
        if !self.slow(t) {
            return;
        }
        let Some(p) = self.tickets.edit().get_mut(&t).and_then(|x| x.posts.iter_mut().find(|p| p.n == n)) else { return };
        if p.by == "owner" {
            return;
        }
        // a post that wakes no agent is shown and makes no card (the owner N-94, 2): those it informs see it now
        if !p.wakes().any(|a| *a != p.by && a != "owner" && a != NOBODY) {
            return;
        }
        p.halted = true;
        self.halt(cx, t, Halting::Deliver { post: n, with: Vec::new(), edited: false, chose: None });
    }

    // ---------------------------------------------------------------- inputs

    /// Make INPUT a complete input line, or say why it cannot be taken (nothing changes then).
    fn complete(&self, input: Input, now: &At) -> Result<Line, String> {
        let at = now.clone();
        Ok(match input {
            Input::Open { kind, under, opener, form, text, names, told } => {
                let k = self.defs.kinds.get(&kind).ok_or_else(|| {
                    format!("There is no kind `{kind}`. The kinds are: {}.", self.defs.kinds.keys().cloned().collect::<Vec<_>>().join(", "))
                })?;
                let form = Type::Record(k.form.clone()).coerce(&form).map_err(|e| format!("The form of a {kind} does not fit: {e}."))?;
                // his switch of that agent's brain is off (BRAIN_SWITCH): no new pass opens
                if let Some(off) = self.brain_off(&kind, form.get("agent")) {
                    return Err(format!("{off}: no new pass of it opens until the owner switches it on."));
                }
                let form = given_opener(k, form, &opener);
                let text = text.filter(|w| !left_to_program(k, &form, w, &names, &told));
                // a question asked by an agent that holds a step is that step's ask (paper-research N-101 p524, 2)
                let asked_on = if kind == "question" { self.held_by(&opener) } else { None };
                Line::Open { at, t: self.next, kind, under, opener, form, text, names, told, esc: Vec::new(), program: k.text.clone(), calls: self.calls_named(&k.text), closed: None, asked_on }
            }
            Input::Import { t, kind, opener, form, text, closed } => {
                let k = self.defs.kinds.get(&kind).ok_or_else(|| format!("There is no kind `{kind}`."))?;
                if t == 0 || self.tickets.contains_key(&t) {
                    return Err(format!("Ticket number {t} is taken."));
                }
                let form = Type::Record(k.form.clone()).coerce(&form).map_err(|e| format!("The form of a {kind} does not fit: {e}."))?;
                let form = given_opener(k, form, &opener);
                let closed = match closed {
                    Some(c) => Some(Closing { head: k.result.coerce(&normal_head(&c.head)).map_err(|e| format!("The result of a {kind} does not fit: {e}."))?, ..c }),
                    None => None,
                };
                Line::Open { at, t, kind, under: None, opener, form, text, names: Vec::new(), told: Vec::new(), esc: Vec::new(), program: k.text.clone(), calls: self.calls_named(&k.text), closed, asked_on: None }
            }
            Input::Started { t, by } => Line::Started { at, t, by },
            // at TO's last post: its line for T stands there
            Input::Tie { t, to, by } => Line::Tied { at, t, to, n: to.and_then(|p| self.tickets.get(&p)).and_then(|x| x.posts.last()).map_or(0, |p| p.n), by },
            Input::Title { t, label, by } => Line::Titled { at, t, label, by },
            Input::End { t, by, how, why } => Line::Ended { at, t, by, how, why },
            Input::Move { t, n, to, by } => match to {
                Some(to) => Line::Move { at, t, n, to, new: None, by },
                // to a thread of its own (paper-research m1166): a resting thread to the agent the post was addressed
                // to, numbered and with its program now, as an open is
                None => {
                    let k = self.defs.kinds.get("thread").ok_or("There is no kind thread to move a post into.")?;
                    Line::Move { at, t, n, to: self.next, new: Some(k.text.clone()), by }
                }
            },
            Input::Keep { ts, by } => Line::Keep { at, ts, by },
            Input::NothingToAnswer { t, n, by } => Line::NothingToAnswer { at, t, n, by },
            Input::WithdrawAsks { t, n, by, why } => Line::AsksWithdrawn { at, t, n, by, why },
            // had by an agent already (given up to it, or read for its handler): struck, each agent that had it told; the
            // line is the `event` kind (TAKEN_BACK), so an engine from before reads it as an event nothing takes
            Input::TakeBack { t, n, by } => match self.had_post(t, n) {
                Some(to) => Line::Event { at, name: TAKEN_BACK.into(), value: json!({"t": t, "n": n, "by": by, "to": to}), fired: Vec::new(), counted: Vec::new(), on: None },
                None => Line::TakenBack { at, t, n, by },
            },
            // taken back while a hand-over carrying it was on its way to TO, which took it (consult-3, N-818 p74)
            Input::StruckFor { t, n, to } => {
                let id = self.id(t);
                let p = self.tickets.get(&t).and_then(|x| x.posts.iter().find(|p| p.n == n)).ok_or_else(|| format!("{id} has no post {n}."))?;
                if !p.taken_back {
                    return Err(format!("Post {n} of {id} is not taken back."));
                }
                Line::Event { at, name: TAKEN_BACK.into(), value: json!({"t": t, "n": n, "by": p.by, "to": [to]}), fired: Vec::new(), counted: Vec::new(), on: None }
            }
            Input::Star { t, on, by, post } => Line::Star { at, t, on, by, post },
            Input::Comments { t, n, by, comments } => Line::Comments { at, t, n, by, comments },
            Input::CommentResolved { t, n, id, resolved, by } => Line::CommentResolved { at, t, n, id, resolved, by },
            Input::Archive { t, on, by } => Line::Archive { at, t, on, by },
            Input::Reopen { t, by } => Line::Reopen { at, t, by },
            Input::Undo { t, by } => Line::Undo { at, t, by },
            Input::Ask { by, text, context } => Line::Ask { at, by, text, context },
            Input::AskLater { by } => Line::AskLater { at, by },
            Input::Post { t, by, text, names, told, inform, answers, kind } => {
                // the owner's `@all` is the first answer's (N-1134): a later one from another agent is not posted
                if let Some(why) = answers.filter(|_| by != "owner").and_then(|n| self.taken(t, n, &by)) {
                    return Err(why);
                }
                // whom a typed ask, read or blocker is for is named by it (I-1480), as an @ would: the window draws it
                // to them, and a release that does not read `kind` sees whom it was for; a hold names whoever works on
                // the item it is on (`Ticket::started`), so it is due to them wherever it is posted
                let mut names = names;
                let on = match &kind {
                    Some(crate::line::PostKind::Hold { on }) => parse_ticket(on).and_then(|i| self.tickets.get(&i)).map(|i| i.started.clone()).unwrap_or_default(),
                    _ => Vec::new(),
                };
                for a in kind.iter().flat_map(|k| k.to()).chain(&on) {
                    if !names.contains(a) && *a != by {
                        names.push(a.clone());
                    }
                }
                // a post that names nobody (design part 9b, N-27 point 3c): it reaches the authors of the posts told to
                // its writer there that it has not answered, as a reply of its turn would; the owner when he takes part
                // and every other agent on it hear of it by policy (`notify`, `inform`; paper-research m1237)
                // those names its writer did not type: it answers them and asks them nothing (`answering`, N-1481 p40);
                // `kept_reply`'s post brings the Reply's own
                let mut answering = self.kept_answering.clone().unwrap_or_default();
                if names.is_empty() && by != "owner" {
                    let last = self.tickets.get(&t).map_or(0, |x| x.last());
                    names = self.unanswered(t, &by, last);
                    answering = names.clone();
                }
                // his typed words are held as the policy says (E1b), on the line, so a replay holds them as it was
                let hold = if by == "owner" { self.owner_hold.filter(|s| *s > 0) } else { None };
                Line::Post { at, t, by, text, names, told, inform, esc: Vec::new(), answers, passes: None, hold, kind, answering }
            }
            Input::Pass { t, by, n, to, text } => {
                if let Some(why) = self.pass_refused(t, n, &by, &to) {
                    return Err(why);
                }
                Line::Post { at, t, by, text: Text::New(text), names: vec![to], told: Vec::new(), inform: Vec::new(), esc: Vec::new(), answers: None, passes: Some(n), hold: None, kind: None, answering: Vec::new() }
            }
            // the answer to a delivery run's step: a `delivery_decided` event (delivery.rs)
            Input::Answer { t, by, head, text } if self.run_agent(t).is_ok() => return self.complete_decided(at, t, by, head, text),
            Input::Answer { t, by, head, text } => Line::Answer { at, t, by, head, text },
            Input::TurnEnded { agent, delivery, text, later, stood, go } => {
                let given = self.turn_given(&agent, &later);
                let informs = self.informs_only(&agent, &given).into_iter().collect();
                let Worked { tools, model } = self.turn_worked.as_ref().filter(|(a, _)| *a == agent).map(|(_, w)| w.clone()).unwrap_or_default();
                Line::TurnEnded { at, agent, delivery, text, later, informs, stood, go, tools, model }
            }
            Input::TurnCut { agent } => Line::TurnCut { at, agent },
            Input::Esc { by, agent, also, all } => Line::Esc { at, by, agent, also, all },
            Input::Unread { agent } => Line::Unread { at, agent },
            Input::Formed { t, step, head, why, not_taken, call } => Line::Formed { at, t, step, head, why, not_taken, call },
            Input::Given { t, to, delivery, upto, place, skipped } => Line::Given { at, t, to, delivery, upto, place, into: false, skipped },
            Input::GivenInto { t, to, delivery, upto, place, skipped } => Line::Given { at, t, to, delivery, upto, place, into: true, skipped },
            Input::Decided { t, step, question, answer, provenance } => Line::Decided { at, t, step, question, answer, provenance },
            Input::Pinned { loc, to } => Line::Pinned { at, loc, to },
            Input::Expired { t, step, provenance } => Line::Expired { at, t, step, provenance },
            Input::Lapsed { t, step, head, after, because } => Line::Lapsed { at, t, step, head, after, because },
            Input::LayerFailed { layer, why } => {
                let t = self.active_layers().into_iter().find(|(l, _, _)| *l == layer).map(|(_, t, _)| t).ok_or_else(|| format!("No layer {layer} is active."))?;
                Line::LayerFailed { at, t, layer, why }
            }
            Input::StartFailed { t, agent, why } => Line::StartFailed { at, t, agent, why },
            Input::RunnerRefused { t, step, why } => Line::RunnerRefused { at, t, step, why },
            Input::Unfit { t, step, why, call } => Line::Unfit { at, t, step, why, call },
            Input::ModelRefused { t, step, agent, why } => Line::ModelRefused { at, t, step, agent, why },
            Input::Made { t, agent, by, from } => Line::Made { at, t, agent, by, from },
            Input::Taken { names } => Line::Taken { at, names },
            Input::Moved { t, agent, from, to, why, killed, left } => Line::Moved { at, t, agent, from, to, why, killed, left },
            Input::Notified { t, event, to, inform, post, provenance } => {
                // nobody is both told and informed of one post (paper-research N-27 post 27, 2): an agent the post
                // already reaches, by its writer, names or told, is not written as informed of it; the state drops
                // such a one anyway (Line::Notified below), as Line::Post's filter does
                let p = post.and_then(|n| self.tickets.get(&t)?.posts.iter().find(|p| p.n == n));
                let reached = |a: &String| p.is_some_and(|p| *a == p.by || p.names.contains(a) || p.told.contains(a));
                let inform = inform.into_iter().filter(|a| !reached(a) && !to.contains(a)).collect();
                Line::Notified { at, t, event, to, inform, post, provenance }
            }
            Input::Unrendered { t, step, why } => Line::Unrendered { at, t, step, why },
            Input::Typed { t, n, agent } => Line::Typed { at, t, n, agent },
            Input::AnswersAlso { t, n, also } => Line::AnswersAlso { at, t, n, also },
            Input::PostedOn { t, n, on } => Line::PostedOn { at, t, n, on },
            Input::Cleaned { t, n, words } => Line::Cleaned { at, t, n, words },
            Input::Split { t, n, from } => Line::Split { at, t, n, from },
            Input::Waits { t, n } => Line::Waits { at, t, n },
            Input::Held { agent, ids, why } => Line::Held { at, agent, ids, why },
            Input::Slow { t, on, by } => Line::Slow { at, t, on, by },
            Input::Slowed { scope, on, by } => {
                if !SCOPES.contains(&scope.as_str()) {
                    return Err(format!("There is no scope `{scope}` slow is set over; the scopes are: {}.", SCOPES.join(", ")));
                }
                Line::Slowed { at, scope, on, by }
            }
            Input::Approve { t, n, by } => Line::Approve { at, t, n, by },
            Input::HaltEdit { t, n, by, value } => Line::HaltEdit { at, t, n, by, value },
            Input::HaltReject { t, n, by, reason } => Line::HaltReject { at, t, n, by, reason },
            Input::Urgent { agent, by } => Line::Urgent { at, agent, by },
            Input::Cancel { t, by, why } => Line::Edited { at, t, by, why, what: "cancel".into(), program: None, kind: None, form: None, calls: BTreeMap::new() },
            Input::Withdraw { t, by, program, why } => Line::Edited { at, t, by, why, what: "withdraw".into(), calls: self.calls_named(&program), program: Some(program), kind: None, form: None },
            Input::Edit { t, by, program, why } => Line::Edited { at, t, by, why, what: "edit".into(), calls: self.calls_named(&program), program: Some(program), kind: None, form: None },
            Input::Repair { t, by, program, why } => Line::Edited { at, t, by, why, what: "repair".into(), calls: self.calls_named(&program), program: Some(program), kind: None, form: None },
            Input::Rekind { t, by, kind, form, why } => {
                let k = self.defs.kinds.get(&kind).ok_or_else(|| format!("There is no kind `{kind}`. The kinds are: {}.", self.defs.kinds.keys().cloned().collect::<Vec<_>>().join(", ")))?;
                let form = Type::Record(k.form.clone()).coerce(&form).map_err(|e| format!("The form of a {kind} does not fit: {e}."))?;
                // a ticket made a pass of that agent's brain while his switch of it is off (BRAIN_SWITCH): as an open
                if let Some(off) = self.brain_off(&kind, form.get("agent")) {
                    return Err(format!("{off}: no new pass of it opens until the owner switches it on."));
                }
                Line::Edited { at, t, by, why, what: "rekind".into(), program: Some(k.text.clone()), kind: Some(kind), form: Some(form), calls: self.calls_named(&k.text) }
            }
            Input::Unchecked { act, by, t, kind, why } => Line::Unchecked { at, act, by, t, kind, why },
            // an act (N-101): recorded for every act, on the ticket its agent owes a step on
            Input::Event { name, value } if name == "act" => {
                let (agent, is, text, more) = crate::runner::act_of_event(&value);
                return self.complete(Input::Act { agent, is, text, more, t: None }, now);
            }
            Input::Template { t, call, scope, text, by, why } => {
                if by != "owner" {
                    return Err("Only the owner edits a prompt.".into());
                }
                if scope != "step" && scope != "tree" {
                    return Err(format!("A prompt is edited for `step` (the next step of {call}) or `tree` (every later one, under it too), not `{scope}`."));
                }
                let x = self.tickets.get(&t).ok_or_else(|| format!("There is no ticket {t}."))?;
                if !x.is_open() {
                    return Err(format!("{} is closed: no step of it is given again.", self.id(t)));
                }
                if !self.defs.calls.contains_key(&call) {
                    return Err(format!("There is no call `{call}`."));
                }
                if text.trim().is_empty() {
                    return Err("The prompt's template is empty.".into());
                }
                Line::Template { at, t, call, scope, text, by, why }
            }
            Input::Reload { by, why, commit, kinds, calls } => Line::Reload { at, by, why, commit, kinds, calls },
            Input::Retext { t, by, why } => {
                let x = self.tickets.get(&t).ok_or_else(|| format!("There is no ticket {t}."))?;
                let k = self.defs.kinds.get(&x.kind).ok_or_else(|| format!("There is no kind `{}` now.", x.kind))?;
                Line::Edited { at, t, by, why, what: "reload".into(), calls: self.calls_named(&k.text), program: Some(k.text.clone()), kind: None, form: None }
            }
            Input::Role { agent, role, also } => {
                if agent.is_empty() || role.is_empty() || also.iter().any(String::is_empty) {
                    return Err("A role line names an agent and its role.".into());
                }
                Line::Role { at, agent, role, also }
            }
            Input::Act { agent, is, text, more, t: given } => {
                if agent.is_empty() || is.is_empty() {
                    return Err("An act names its agent and what it is.".into());
                }
                // the ticket owed; a given one only when the agent owes none (an edit coordd found in a layer: the
                // layer's ticket, for a minder's), and only one there is
                let t = self.owing(&agent).and_then(|id| parse_ticket(&id)).or(given.filter(|t| self.tickets.contains_key(t)));
                Line::Act { at, t, agent, is, text, more }
            }
            // a delivery event (0.2): run in the agent's delivery ticket (delivery.rs)
            Input::Event { name, value } if syntax::delivery_event(&name) => return self.complete_delivery(at, name, value),
            Input::Event { name, .. } if name == syntax::DECIDED || syntax::delivery_output(&name) => return Err(format!("{name} is a line the message runtime writes itself; it is not sent.")),
            // the owner's held posts handed at the install (HELD_HANDED): a record line of the machine's own
            Input::Event { name, value } if name == HELD_HANDED || name == ERA => Line::Event { at, name, value, fired: Vec::new(), counted: Vec::new(), on: None },
            // his post sent on after a replay (UNTOLD): a record line of the machine's own, written with the policy's
            // hold as his typed post's line is (`Line::Post`'s `hold`), so a replay holds it as it ran
            Input::Event { name, mut value } if name == UNTOLD => {
                self.hold_on(&mut value);
                Line::Event { at, name, value, fired: Vec::new(), counted: Vec::new(), on: None }
            }
            // a delivery that entered an agent's input (DELIVERED): a record line of the machine's own
            Input::Event { name, value } if name == DELIVERED || name == DUE_AGAIN => Line::Event { at, name, value, fired: Vec::new(), counted: Vec::new(), on: None },
            // the owner's hold on agents' mail switched (MAIL_HOLD, line 327): a record line of the machine's own; coordd
            // lets only the owner switch it
            Input::Event { name, value } if name == MAIL_HOLD => {
                if value.get("on").and_then(Value::as_bool).is_none() {
                    return Err(String::from("A hold on agents' mail is switched on or off."));
                }
                Line::Event { at, name, value, fired: Vec::new(), counted: Vec::new(), on: None }
            }
            // his switch of an agent's brain (BRAIN_SWITCH, N-1446 p857.2): a record line of the machine's own, as the
            // hold's; coordd lets only the owner switch it
            Input::Event { name, value } if name == BRAIN_SWITCH => {
                if value.get("agent").and_then(Value::as_str).is_none_or(str::is_empty) || value.get("on").and_then(Value::as_bool).is_none() {
                    return Err(String::from("An agent's brain is switched on or off, naming the agent."));
                }
                Line::Event { at, name, value, fired: Vec::new(), counted: Vec::new(), on: None }
            }
            // his sign-off of an ask with his word of worth (SIGNED_OFF): a record line of the machine's own, as the
            // brain switch's; coordd lets only the owner say it. Live, the post must be an ask of his now
            Input::Event { name, value } if name == SIGNED_OFF => {
                let (t, n, _) = Machine::sign_off_of(&value)?;
                let his = self.tickets.get(&t).and_then(|x| x.posts.iter().find(|p| p.n == n)).is_some_and(|p| p.by == "owner");
                if !his || self.asked(t, n).is_empty() {
                    return Err(format!("{}.p{n} is no ask of his: the ledger says it asks no agent to act.", self.id(t)));
                }
                Line::Event { at, name, value, fired: Vec::new(), counted: Vec::new(), on: None }
            }
            // the release his window runs (WINDOW_RELEASE, I-1457): a record line of the machine's own; coordd lets only
            // the owner's bridge say it
            Input::Event { name, value } if name == WINDOW_RELEASE => {
                if WindowRelease::of(&value).is_none() {
                    return Err(String::from("A window's release line names the release, and its engine's when it has one."));
                }
                // it reaches programs (N-1446 p1491, 6): the bindings on it fire as on a project's event
                let (fired, counted, _) = self.fire_bindings(&at, &name, &value);
                Line::Event { at, name, value, fired, counted, on: None }
            }
            // a train branch's move (MERGED): a record line of the machine's own, which coordd's git round alone writes
            Input::Event { name, value } if name == MERGED => {
                let ty = syntax::system_event(MERGED).unwrap_or(Type::Any);
                let Some(value) = ty.coerce(&value).ok().filter(|v| ty.conforms(v).is_ok()) else {
                    return Err(String::from("A merged line names the repository, the branch, its tip with its tree and parents, and the commits the move brought."));
                };
                Line::Event { at, name, value, fired: Vec::new(), counted: Vec::new(), on: None }
            }
            // his ask taken (ASKED), at its first head: a record line of the machine's own, which coordd alone writes,
            // naming the post; its value is the ledger's now, once for each post, and the bindings on it fire
            Input::Event { name, value } if name == ASKED => {
                let post = value.get("post").and_then(Value::as_str).and_then(post_ref).and_then(|(t, n)| Some((self.ticket_named(t)?, n)));
                let Some((t, n)) = post else { return Err(String::from("An asked line names a post of his.")) };
                let Some(value) = self.asked_value(t, n) else {
                    return Err(format!("{}.p{n} is no ask of his still to be raised: it asks no agent to act, or an asked line named it.", self.id(t)));
                };
                let (fired, counted, _) = self.fire_bindings(&at, &name, &value);
                Line::Event { at, name, value, fired, counted, on: None }
            }
            // the owner writing in a ticket's box (WRITING, 0.3): a record line of the machine's own; coordd lets only the
            // owner say it
            Input::Event { name, value } if name == syntax::WRITING => {
                let on = value.get("t").and_then(Value::as_u64).filter(|t| *t < HIDDEN && self.tickets.contains_key(t)).and(value.get("on").and_then(Value::as_bool));
                if on.is_none() {
                    return Err(String::from("Writing is switched on or off in a ticket's box."));
                }
                Line::Event { at, name, value, fired: Vec::new(), counted: Vec::new(), on: None }
            }
            // a turn begun (TURN_BEGAN, N-1104 p1154): a record line of the machine's own, which coordd alone writes
            Input::Event { name, value } if name == TURN_BEGAN => {
                if value.get("agent").and_then(Value::as_str).is_none_or(str::is_empty) {
                    return Err(String::from("A turn began names its agent."));
                }
                Line::Event { at, name, value, fired: Vec::new(), counted: Vec::new(), on: None }
            }
            // the machine's own line, never sent as an event
            Input::Event { name, .. } if name == TAKEN_BACK => return Err(format!("{TAKEN_BACK} is written by a take-back, not sent.")),
            Input::Event { name, mut value } => {
                // a declared event's value is held to its fields
                if let Some(fs) = self.defs.events.get(&name) {
                    let ty = Type::Record(fs.clone());
                    value = ty.coerce(&value).and_then(|v| ty.conforms(&v).map(|_| v)).map_err(|e| format!("The event {name} does not fit its declaration: {e}."))?;
                }
                // his message: its words are held as his typed post is, the policy's hold on its line (I-1474); sent
                // while an open thread gathers his opening burst before `intake` (N-1104 p935/p936), it joins that burst
                if name == "owner_message" {
                    self.hold_on(&mut value);
                    if let Some(t) = self.gathering_thread(&value) {
                        return Ok(Line::Event { at, name, value, fired: Vec::new(), counted: Vec::new(), on: Some(t) });
                    }
                }
                // sent about a ticket whose step is open and whose kind handles it: the ticket's, not the bindings'
                if let Some(t) = self.event_ticket(&name, &value) {
                    return Ok(Line::Event { at, name, value, fired: Vec::new(), counted: Vec::new(), on: Some(t) });
                }
                // the runner's events the record keeps (runner.rs): on the ticket they name, whether or not it handles them
                let recorded = crate::runner::recorded(&name);
                let on = value.get("ticket").and_then(Value::as_str).and_then(|id| self.ticket_named(id)).filter(|_| recorded);
                if recorded && !self.bound(&name) {
                    return Ok(Line::Event { at, name, value, fired: Vec::new(), counted: Vec::new(), on });
                }
                let (fired, counted, why) = self.fire_bindings(&at, &name, &value);
                if fired.is_empty() && counted.is_empty() && !recorded {
                    return Err(if why.is_empty() { format!("No binding is on {name}.") } else { why.join("; ") });
                }
                // a runner's event no binding fired on: kept on the ticket it names
                if fired.is_empty() && counted.is_empty() {
                    return Ok(Line::Event { at, name, value, fired, counted, on });
                }
                Line::Event { at, name, value, fired, counted, on: None }
            }
        })
    }

    /// The bindings on event NAME that fire with VALUE at AT, as its line records them (`Line::Event`'s `fired`), those
    /// `every N of` ones that count it, and why each other one did not fire: a project's event's, and the system's own
    /// that reach programs (`syntax::system_event`; N-1446 p1491, 6). Nothing applied: coordd's leg A of head 3a reads
    /// with it what each `asked` line of a record would open (paper-research N-1446 p2209).
    pub fn fire_bindings(&self, at: &str, name: &str, value: &Value) -> (Vec<Fired>, Vec<String>, Vec<String>) {
        let mut fired: Vec<Fired> = Vec::new();
        let mut counted = Vec::new();
        let mut why = Vec::new();
        // where the open tickets stand as a decision's binding fires, which its arguments may read (STANDS)
        let stands = (name == "when").then(|| self.stands());
        for b in self.defs.bindings.iter().filter(|b| b.ev == name) {
            let Some(k) = self.defs.kinds.get(&b.kind) else { continue };
            let mut env: Env = [(b.x.clone(), value.clone())].into_iter().collect();
            if let (Every::When { .. }, Some(s)) = (&b.every, &stands) {
                env.insert(syntax::STANDS.to_string(), s.clone());
            }
            match b.every {
                // a clock's event names the binding, and fires it only when it is due
                Every::Clock(_) if value.as_str() != Some(b.text.as_str()) => continue,
                Every::Clock(_) if !self.clock_due(b, at) => {
                    why.push(format!("`{}` is not due", b.text));
                    continue;
                }
                // a decision's event names the binding and the option the policy answered
                Every::When { .. } if value.get("binding").and_then(Value::as_str) != Some(b.text.as_str()) => continue,
                Every::When { ref option, .. } if value.get("answer").and_then(Value::as_str) != Some(option.as_str()) => {
                    why.push(format!("`{}`: the policy answered {}", b.text, value.get("answer").unwrap_or(&Value::Null)));
                    continue;
                }
                _ => {}
            }
            let holds = b.conds.iter().all(|(l, r)| {
                let l = eval(l, &env, &json!({}));
                match r {
                    None => l == json!(true),
                    Some((eq, r)) => (l == eval(r, &env, &json!({}))) == *eq,
                }
            });
            if !holds {
                why.push(format!("`{}`: its `where` does not hold", b.text));
                continue;
            }
            // counted since it last fired; it fires at N or more, N from where the binding says (`Times`; N-1444
            // p16: N may change between two firings)
            if let Every::Count(times) = &b.every {
                counted.push(b.text.clone());
                let c = self.counts.get(&b.text).copied().unwrap_or(0) + 1;
                match self.count_times(b, times, value) {
                    Some(n) if c >= n => {}
                    Some(_) => continue,
                    None => {
                        why.push(format!("`{}`: {times} gives no N", b.text));
                        continue;
                    }
                }
            }
            let form: serde_json::Map<String, Value> =
                k.form.iter().enumerate().map(|(i, (n, _))| (n.clone(), b.args.get(i).map_or(Value::Null, |a| eval(a, &env, &json!({}))))).collect();
            match Type::Record(k.form.clone()).coerce(&Value::Object(form)) {
                Ok(form) => {
                    // his switch of that agent's brain is off (BRAIN_SWITCH): the binding stays, and does not fire
                    if let Some(off) = self.brain_off(&b.kind, form.get("agent")) {
                        why.push(format!("`{}`: {off}", b.text));
                        continue;
                    }
                    // his message's binding opens it for him: its opener is the owner; another binding's is
                    // left none (its kind posts as the ticket itself, never as his)
                    let form = if name == "owner_message" { given_opener(k, form, "owner") } else { form };
                    fired.push(Fired { t: self.next + fired.len() as u64, kind: b.kind.clone(), binding: b.text.clone(), form, program: k.text.clone(), calls: self.calls_named(&k.text) })
                }
                Err(e) => why.push(format!("{}:{}: `{}`: the form of a {} does not fit: {e}", b.file, b.line, b.text, b.kind)),
            }
        }
        (fired, counted, why)
    }

    /// The ticket a project's event NAME is sent about, if it is that ticket's: VALUE's `ticket` names it (its id), it is
    /// open with a step owed (the sender holds it: the runner's step), and its kind has a `when NAME` handler.
    fn event_ticket(&self, name: &str, value: &Value) -> Option<u64> {
        let id = value.get("ticket").and_then(Value::as_str)?;
        let t = self.ticket_named(id)?;
        self.takes_on(t, name).then_some(t)
    }

    /// Whether ticket T takes event NAME now: it is open, its step is open (it awaits an answer, or the policy's), and
    /// its kind or a `during` it is in handles NAME.
    fn takes_on(&self, t: u64, name: &str) -> bool {
        let Some(x) = self.tickets.get(&t) else { return false };
        x.is_open() && matches!(x.status, Status::Awaits { .. } | Status::Asking { .. }) && (self.during_on(t, name).is_some() || x.handlers.iter().any(|h| h.ev == name))
    }

    /// Whether project event NAME with VALUE is a ticket's to handle (a kind's `when`, a `during`'s), as `apply` would
    /// take it: coordd sends it then even if no binding is on it.
    pub fn takes_event(&self, name: &str, value: &Value) -> bool {
        // a delivery event (0.2) is the delivery program's, when one is loaded (delivery.rs)
        (syntax::delivery_event(name) && self.defs.kinds.contains_key(DELIVERY)) || self.event_ticket(name, value).is_some()
    }

    /// The ticket on which AGENT owes a step now (the newest, if several): where what it does is an `act`, its check's
    /// layer is read (`layer_of`), and an agent it makes gets its thread. A step its own act raised (item.kind's `seek`
    /// at its `coord item start`, a hidden run whose id is above every ticket's) gives way to any other it owes: its
    /// acts stay with the work it was on, as a `during`'s `when act` on that work reads them (consult-3 N-1396 p264,
    /// 2); once that is answered, or when it owes nothing else, the raised one.
    pub fn owing(&self, agent: &str) -> Option<String> {
        // an answer being put to the policy is not taken yet: its agent still holds the step (violated.rs)
        let asking = |x: &Ticket| matches!(&x.status, Status::Asking { role, .. } if role == violated::ROLE) && x.pending.as_ref().is_some_and(|p| p.by == agent);
        let owes = |x: &&Ticket| x.is_open() && (matches!(&x.status, Status::Awaits { agent: a, .. } if a == agent) || asking(x));
        // the newest the agent's own act did not raise, else the newest it did (`min_by_key` keeps the first of equals)
        self.tickets.values().rev().filter(owes).min_by_key(|x| x.raised_for(agent)).map(|x| self.id(x.t))
    }

    /// The `during` whose handler takes event EV sent about T: the innermost on T's frames, else on its parent's, and
    /// so up (a step of the block may be owed in a ticket the block opened). The ticket it is on and the frame's place.
    fn during_on(&self, t: u64, ev: &str) -> Option<(u64, usize)> {
        let mut u = Some(t);
        while let Some(v) = u {
            let x = self.tickets.get(&v)?;
            if let Some(i) = x.k.iter().rposition(|f| matches!(f, Frame::During { whens, .. } if whens.iter().any(|w| w.ev == ev))) {
                return Some((v, i));
            }
            u = x.parent;
        }
        None
    }

    /// The handler of the `during` at frame I of ticket U that takes event EV with VALUE: the first written for EV
    /// whose `where` holds, with the environment it runs in. None when no `where` holds.
    pub(super) fn during_when(&self, u: u64, i: usize, ev: &str, value: &Value) -> Option<(syntax::When, Env)> {
        let Some(Frame::During { whens, env, .. }) = self.tickets[&u].k.get(i) else { return None };
        whens.iter().filter(|w| w.ev == ev).find_map(|w| {
            let mut env = env.clone();
            env.insert("this".into(), json!(self.id(u)));
            env.insert(w.x.clone(), value.clone());
            (!w.cond.as_ref().is_some_and(|c| eval(c, &env, &self.tickets[&u].ctx) != json!(true))).then(|| (w.clone(), env))
        })
    }

    /// Event EV with VALUE, raised by BY, taken by the `during` at frame I of ticket U: its first handler for EV whose
    /// `where` holds runs as a look in a hidden ticket under U, when no look runs; while one runs, it is kept for the
    /// next.
    fn during_event(&mut self, cx: &mut Cx, u: u64, i: usize, ev: &str, value: &Value, by: &str) -> Result<String, String> {
        let Some(Frame::During { whens, scope, busy, .. }) = self.tickets[&u].k.get(i).cloned() else { return Err(format!("{} has no during for {ev}", self.id(u))) };
        let Some((when, env)) = self.during_when(u, i, ev, value) else {
            let what = if whens.iter().filter(|w| w.ev == ev).count() > 1 { "no `where` of its during holds" } else { "the `where` of its during does not hold" };
            return Ok(format!("Taken by {}; {what}.", self.id(u)));
        };
        if busy.is_some_and(|c| self.tickets.get(&c).is_some_and(Ticket::is_open)) {
            if let Some(Frame::During { missed, .. }) = self.tickets.edit().get_mut(&u).unwrap().k.get_mut(i) {
                *missed = Some((ev.to_string(), value.clone(), by.to_string()));
            }
            return Ok(format!("Kept by {} for its next look: one look at a time.", self.id(u)));
        }
        if !logged_run(cx, ev, u) {
            return Ok(format!("Taken by {}; no look ran.", self.id(u)));
        }
        let Some((c, body)) = self.handler_ticket(cx, u, ev, &when, value.clone(), by, Some(&scope)) else {
            return Ok(format!("Taken by {}; its look is not loaded in this release.", self.id(u)));
        };
        if let Some(Frame::During { busy, .. }) = self.tickets.edit().get_mut(&u).unwrap().k.get_mut(i) {
            *busy = Some(c);
        }
        cx.effect(Effect::Changed { t: u });
        self.drive(cx, vec![Work::Run(c, body, env)]);
        Ok(format!("Looked at on {}.", self.id(u)))
    }

    /// Hidden ticket C, a look of a `during` on ticket U, closed: the next look runs on what came meanwhile, if
    /// anything did.
    fn look_closed(&mut self, cx: &mut Cx, u: u64, c: u64) {
        // a question it held is asked now, or not (ask_minded.rs)
        self.ask_looked(cx, c);
        let Some(x) = self.tickets.edit().get_mut(&u) else { return };
        let Some(i) = x.k.iter().position(|f| matches!(f, Frame::During { busy: Some(b), .. } if *b == c)) else { return };
        let Some(Frame::During { busy, .. }) = x.k.get_mut(i) else { return };
        *busy = None;
        // a question waiting for a look goes first; what was missed comes after it
        if self.ask_next(cx, u, i) {
            return;
        }
        let Some(Frame::During { missed, .. }) = self.tickets.edit().get_mut(&u).and_then(|x| x.k.get_mut(i)) else { return };
        if let Some((ev, value, by)) = missed.take() {
            let _ = self.during_event(cx, u, i, &ev, &value, &by);
        }
    }

    /// End the look C of a `during` whose block ended (WHY): cancelled if it still runs.
    fn end_look(&mut self, cx: &mut Cx, c: Option<u64>, why: &str) {
        let Some(c) = c.filter(|c| self.tickets.get(c).is_some_and(Ticket::is_open)) else { return };
        let r = Raised { exc: "Cancelled".into(), value: json!({"by": "coord", "why": why}) };
        let mut none = Vec::new();
        self.cancel(cx, c, "coord", why, &r, &mut none);
    }

    /// Project event NAME with VALUE on ticket T (`Line::Event`'s `on`): T's `when NAME` handler, if its `where` holds,
    /// runs in a hidden ticket under T, raised by the agent T's step is owed to; an `act`'s by the act's agent, whose
    /// own act it is (its steps go into its turn: `waiting_of`'s `raised`). A `during`'s look keeps the step's holder.
    fn event_on(&mut self, cx: &mut Cx, t: u64, name: &str, value: &Value) -> Result<String, String> {
        let x = self.tickets.get(&t).ok_or_else(|| format!("There is no ticket {t}."))?;
        if let Some((u, i)) = self.during_on(t, name) {
            let by = match &x.status {
                _ if syntax::system_event(name).is_some() => NOBODY.to_string(),
                Status::Awaits { agent, .. } => agent.clone(),
                _ => NOBODY.to_string(),
            };
            return self.during_event(cx, u, i, name, value, &by);
        }
        let h = x.handlers.iter().find(|h| h.ev == name).ok_or_else(|| format!("{} has no handler for {name}", self.id(t)))?.clone();
        let by = match (name, value.get("by").and_then(Value::as_str), &x.status) {
            ("act", Some(a), _) => a.to_string(),
            (n, _, _) if syntax::system_event(n).is_some() => NOBODY.to_string(),
            (_, _, Status::Awaits { agent, .. }) => agent.clone(),
            _ => NOBODY.to_string(),
        };
        let mut env: Env = x.form.as_object().map(|o| o.iter().map(|(a, b)| (a.clone(), b.clone())).collect()).unwrap_or_default();
        env.insert("this".into(), json!(self.id(t)));
        env.insert(h.x.clone(), value.clone());
        if h.cond.as_ref().is_some_and(|c| eval(c, &env, &x.ctx) != json!(true)) || !logged_run(cx, name, t) {
            return Ok(format!("Taken by {}; its handler's `where` does not hold.", self.id(t)));
        }
        let Some((c, body)) = self.handler_ticket(cx, t, name, &h, value.clone(), &by, None) else {
            return Ok(format!("Taken by {}; its handler is not loaded in this release.", self.id(t)));
        };
        cx.effect(Effect::Changed { t });
        self.drive(cx, vec![Work::Run(c, body, env)]);
        Ok(format!("Handled on {}.", self.id(t)))
    }

    /// The tickets FIRED, by the bindings on event NAME with VALUE at AT, opened and run, and those of COUNTED counted:
    /// a project's event's line, and the system's own that reach programs (`syntax::system_event`; N-1446 p1491, 6).
    /// Whether it changed anything, and the reply; nothing is changed when one of them cannot open.
    fn open_fired(&mut self, cx: &mut Cx, at: &At, name: &str, value: &Value, fired: &[Fired], counted: &[String]) -> (bool, Result<String, String>) {
        let no = |e: String| (false, Err(e));
        let mut ks = Vec::new();
        for f in fired {
            if self.tickets.contains_key(&f.t) {
                return no(format!("ticket {} exists already", f.t));
            }
            let (k, bad, loose) = match self.recorded_program_as(&f.kind, &f.program, &f.calls, cx) {
                Ok(k) => k,
                Err(e) => return no(e),
            };
            if let (None, Err(e)) = (&bad, Type::Record(k.form.clone()).conforms(&f.form)) {
                return no(format!("The form of a {} does not fit: {e}.", f.kind));
            }
            ks.push((k, bad, loose));
        }
        // what made it fire came from a slow thread: the ticket opened halts as well (N-101's stand-in run)
        let slow: Vec<bool> = fired.iter().map(|f| self.fired_slow(name, value, &f.binding)).collect();
        // all are numbered before any runs, so the children they open come after them
        for b in counted {
            *self.counts.entry(b.clone()).or_insert(0) += 1;
        }
        // an `every N of` binding that fired counts afresh from here
        for f in fired.iter().filter(|f| counted.contains(&f.binding)) {
            self.counts.insert(f.binding.clone(), 0);
        }
        if name == "every" || name == "when" {
            for f in fired {
                self.clocks.insert(f.binding.clone(), at.clone());
            }
        }
        if fired.is_empty() && counted.is_empty() {
            return (true, Ok(String::from("Recorded.")));
        }
        if fired.is_empty() {
            return (true, Ok(format!("Counted by {}.", counted.join("; "))));
        }
        for ((f, (k, bad, loose)), slow) in fired.iter().zip(&ks).zip(slow) {
            let mut x = self.new_ticket(f.t, &f.kind, None, None, f.form.clone(), json!({}), f.program.clone(), k, at);
            // the label and the quoted words given with the owner's words (`say` with `label`, `under`
            // T.pN:FROM+LEN; N-447) are kept in a thread's form, where they are read; they are not passed by
            // the binding, so a project's own older `thread` form still binds (docket-core3: a thread's only)
            if f.kind == "thread" {
                if let Some(o) = x.form.as_object_mut() {
                    for key in ["label", "quote"] {
                        if let Some(v) = value.get(key).filter(|v| !v.is_null()) {
                            if o.get(key).is_none_or(Value::is_null) {
                                o.insert(key.into(), v.clone());
                            }
                        }
                    }
                }
            }
            x.calls = f.calls.clone();
            x.raised_by = value.get("from").and_then(Value::as_str).map(str::to_string);
            x.binding = Some(f.binding.clone());
            x.slow = slow;
            // his message's words are held as the policy said when it came, read from its line (I-1474)
            if name == "owner_message" {
                x.words_hold = value.get("hold").and_then(Value::as_u64).filter(|s| *s > 0);
            }
            // an `esc` key in the value is read past: no ticket stops a turn (N-36 point 2)
            self.tickets.edit().insert(f.t, x);
            self.next = self.next.max(f.t + 1);
            if let Some(x) = self.tickets.edit().get_mut(&f.t) {
                x.loose = loose.clone();
            }
            if let Some(why) = bad {
                self.unchecked(f.t, why);
            }
            // the owner's words opened at a post (`say` with `under`, N-447): tied there from the open
            self.tie_at_open(cx, f.t, value.get("tied_at"));
        }
        for (f, (k, _, _)) in fired.iter().zip(ks) {
            let env: Env = f.form.as_object().map(|o| o.iter().map(|(a, b)| (a.clone(), b.clone())).collect()).unwrap_or_default();
            self.drive(cx, vec![Work::Run(f.t, k.pattern, env)]);
            cx.effect(Effect::Changed { t: f.t });
        }
        (true, Ok(format!("Opened {}.", fired.iter().map(|f| self.id(f.t)).collect::<Vec<_>>().join(", "))))
    }

    fn ticket(&self, t: u64) -> Result<&Ticket, String> {
        self.tickets.get(&t).ok_or_else(|| format!("There is no ticket {t}."))
    }

    /// Apply an input line: whether it changed anything (and so is logged), and the reply.
    /// LINE carried out, then any hole that something has happened at since it was last asked is asked (its `when
    /// hole` handler run): the same in a live act and on replay.
    fn exec(&mut self, line: &Line, cx: &mut Cx) -> (bool, Result<String, String>) {
        // an edit's program runs against the calls its line records: the ticket keeps them, unless the edit is refused
        let kept = match line {
            Line::Edited { t, calls, .. } if !calls.is_empty() => self.tickets.edit().get_mut(t).map(|x| {
                let was = x.calls.clone();
                x.calls.extend(calls.clone());
                (*t, was)
            }),
            _ => None,
        };
        let from = cx.lines.len();
        let r = self.exec_line(line, cx);
        // the steps the record's own lines answer (IN_WINDOW, a step's `on`), before any hole is asked; then the goals'
        // lines kept and their duties asked where a line touched them (machine/duties.rs)
        if r.0 {
            self.line_answers(cx, line);
            self.goal_lines_after(cx, line, from);
        }
        // posts AGENT missed twice are due again at its turn's end (0.2 A4)
        if let (true, Line::TurnEnded { agent, .. }) = (r.0, line) {
            self.release_missed(cx, agent);
        }
        if let (Some((t, was)), false) = (kept, r.0) {
            if let Some(x) = self.tickets.edit().get_mut(&t) {
                x.calls = was;
            }
        }
        if r.0 {
            self.wake_holes(cx);
            self.halt_notices(cx);
        }
        // the message runtime's runs are made again by replay from its `event` lines: no other line of them is written
        cx.lines.retain(|l| !self.delivery_line(l));
        r
    }

    /// Whether nobody on X holds a delivery of it unanswered: none is due to an agent, and none was given in a turn
    /// that has not ended (paper-research m1185, A: the minder is asked once the ticket is quiet, not while an agent
    /// is still answering what moved it).
    fn quiet(&self, x: &Ticket) -> bool {
        // the owner's post held by its handler: nothing is settled on it yet (paper-research m1302, 3)
        if x.posts.iter().any(|p| p.held.is_some()) {
            return false;
        }
        // a handler running under it (an agent's post being converted, §16, 1): the hole is asked once it is done
        if self.tickets.values().any(|y| y.under == Some(x.t) && y.handling.is_some() && y.is_open()) {
            return false;
        }
        let mut on: BTreeSet<&str> = BTreeSet::new();
        for p in &x.posts {
            on.insert(&p.by);
            on.extend(p.names.iter().chain(&p.told).map(String::as_str));
        }
        on.extend(x.gives.iter().map(|g| g.to.as_str()));
        // the questions and asks agents raised from it: while one is open, or its answer not yet handed back and
        // acted on, the ticket is not quiet (paper-research m1225 b: the minder ran on N-4 while lead's Q-6 was open)
        let mut on_t = vec![x.t];
        for y in self.tickets.values() {
            let raised = y.about.contains(&x.t) || y.ask.as_ref().is_some_and(|a| a.held == x.t);
            if !raised {
                continue;
            }
            if y.is_open() && !y.ask.as_ref().is_some_and(|a| a.replied) {
                return false;
            }
            on_t.push(y.t);
            on.extend(y.opener.as_deref().or(y.ask.as_ref().map(|a| a.by.as_str())));
        }
        // what is due to each on these tickets alone (`due_among`), not a pass over every ticket for each
        on_t.sort_unstable();
        on_t.dedup();
        on.into_iter().filter(|a| *a != "owner" && *a != NOBODY).all(|a| {
            !self.given_since(a).iter().any(|d| parse_delivery(d).is_some_and(|(t, _)| on_t.contains(&t))) && !self.due_among(a, on_t.iter().filter_map(|t| self.tickets.get(t))).iter().any(|d| !d.inform)
        })
    }

    /// The tickets below T that have closed.
    fn closed_below(&self, t: u64) -> usize {
        self.tickets.get(&t).map_or(0, |x| x.children.iter().filter(|c| self.tickets.get(c).is_some_and(|y| !y.is_open())).count())
    }

    /// Run the `when hole` handler of each resting ticket that has one, whose `where` holds, and on which something
    /// has happened since its hole was last asked. The handler stands in the hole's place, in its scope, `this` the
    /// ticket.
    fn wake_holes(&mut self, cx: &mut Cx) {
        loop {
            let due: Vec<(u64, syntax::When, Env)> = self
                .tickets
                .values()
                .filter_map(|x| {
                    let Status::Rests { env, .. } = &x.status else { return None };
                    let h = x.handlers.iter().find(|h| h.ev == "hole")?;
                    if !hole_logged(cx, x.t) {
                        return None;
                    }
                    let mark = x.mark(self.closed_below(x.t));
                    if x.asked_at == Some(mark) || !self.quiet(x) {
                        return None;
                    }
                    // nobody is asked on a ticket the owner archived: its hole is put to the minder after Unarchive, if
                    // something happened on it since it was last asked (paper-research N-867.p21)
                    if self.archived_by(x.t).is_some() {
                        return None;
                    }
                    let mut env = env.clone();
                    env.insert("this".into(), json!(self.id(x.t)));
                    env.insert(h.x.clone(), json!({}));
                    if let Some(c) = &h.cond {
                        if eval(c, &env, &x.ctx) != json!(true) {
                            return None;
                        }
                    }
                    Some((x.t, h.clone(), env))
                })
                .collect();
            if due.is_empty() {
                return;
            }
            for (t, h, env) in due {
                let closed = self.closed_below(t);
                let dropped: Vec<String> = self.tickets[&t].k.iter().filter_map(|f| if let Frame::Scope { agent } = f { Some(agent.clone()) } else { None }).collect();
                for a in dropped {
                    self.end_kept(cx, t, &a);
                }
                let x = self.tickets.edit().get_mut(&t).unwrap();
                let Status::Rests { env: henv, scope } = std::mem::replace(&mut x.status, Status::Waits { children: Vec::new(), par: false }) else { unreachable!() };
                x.shaping = Some((henv, scope));
                x.asked_at = Some(x.mark(closed));
                x.k.clear();
                // the hole's handler is not the ticket's program: it is not traced (the hole stays where it stands)
                x.trace.src = None;
                // the handler's program, recorded where its run begins; an old log's has none: today's runs
                let recorded = cx.recorded.as_ref().map(|rec| {
                    rec.iter()
                        .find_map(|l| match l {
                            Line::Handling { t: u, event, program, .. } if *u == t && event == "hole" => Some(program.clone()),
                            _ => None,
                        })
                        .unwrap_or_default()
                });
                let (body, program) = self.handler_run(t, &h, recorded, None);
                let x = self.tickets.edit().get_mut(&t).unwrap();
                x.bound = u64::from(x.steps) + check::bound(&body, &self.defs.kinds);
                if !program.is_empty() {
                    cx.line(Line::Handling { at: cx.now.clone(), t, event: "hole".into(), program });
                }
                cx.effect(Effect::Changed { t });
                self.drive(cx, vec![Work::Run(t, body, env)]);
            }
        }
    }

    /// `continue V` at T: the minder's answer at the hole being shaped, carried out. Rest: T rests at the hole again.
    /// Program(text): checked as an Edit at the hole is (`check::check_shape`) and run there; a program that does not
    /// check is refused and T rests, the reason kept. Close(result): T closes with it, if it fits the kind's result.
    fn carry_out(&mut self, cx: &mut Cx, t: u64, v: Value) -> Option<Work> {
        // what an answer carries (`@by`, `@text`) is for the program, never on a line (N-101 post 31); the shaped
        // line and its replay stay as before them
        let v = answers::plain(v);
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let Some((env, scope)) = x.shaping.take() else {
            return Some(Work::Throw(t, Raised { exc: "Stuck".into(), value: json!({"why": "continue is used where no hole is being shaped"}) }));
        };
        let n = x.shaped.len() as u32 + 1;
        let kind = self.kind_of(&self.tickets[&t]);
        let (work, refused) = match (v.get("head").and_then(Value::as_str), kind) {
            (_, Err(e)) => (None, Some(e)),
            (Some("Rest"), _) => (None, None),
            (Some("Program"), Ok(k)) => match check::check_shape(&k, &scope, v.get("text").and_then(Value::as_str).unwrap_or(""), &self.defs.calls, &self.defs.kinds) {
                Ok(p) => {
                    let x = self.tickets.edit().get_mut(&t).unwrap();
                    x.bound += check::bound(&p, &self.defs.kinds);
                    x.trace.fill(n, None, &p);
                    (Some(Work::Run(t, p, env.clone())), None)
                }
                Err(e) => (None, Some(format!("the program does not check: {e}"))),
            },
            (Some("Close"), Ok(k)) => {
                let r = v.get("result").map(normal_head).unwrap_or(Value::Null);
                match k.result.conforms(&r) {
                    Ok(()) => {
                        let tr = &mut self.tickets.edit().get_mut(&t).unwrap().trace;
                        tr.back();
                        if let Some(h) = tr.now.clone().and_then(|k| tr.visits.get_mut(&k)) {
                            h.value = Some(r.clone());
                        }
                        (Some(Work::Resume(t, r)), None)
                    }
                    Err(e) => (None, Some(format!("the result does not fit a {}: {e}", k.result))),
                }
            }
            (h, _) => (None, Some(format!("not an answer at a hole: {}", h.unwrap_or("no head")))),
        };
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.shaped.push(Shaped { n, at: cx.now.clone(), answer: v.clone(), refused: refused.clone(), undone: false, steps: x.steps, undone_steps: 0, env: env.clone(), scope: scope.clone() });
        cx.line(Line::Shaped { at: cx.now.clone(), t, n, answer: v, refused });
        cx.effect(Effect::Changed { t });
        if work.is_none() {
            x.status = Status::Rests { env, scope };
            x.k.clear();
            x.trace.back();
        }
        work
    }

    /// The owner's words TEXT, put on ticket T naming NAMES: if T, or a ticket below it, awaits a step of the owner
    /// (a decision the minder's program opened), they are formed against that step instead of posted (Effect::Form
    /// with the words); `Formed` then answers it, or posts them on T. The ticket formed for, if any.
    /// HELD: a held post's question takes them too, as before N-807.p78 (a replay of a record from then); otherwise a
    /// step of an `owner_post` handler is passed over (his press answers it, `hand_unanswered`).
    #[allow(clippy::too_many_arguments)]
    fn form_owner_words(&mut self, cx: &mut Cx, t: u64, text: &Text, names: &[String], told: &[String], inform: &[String], held: bool) -> Option<u64> {
        let Text::New(words) = text else { return None };
        let mut todo = vec![t];
        let mut found = None;
        while let Some(v) = todo.pop() {
            let x = &self.tickets[&v];
            if let Status::Awaits { agent, owed, forming: None, .. } = &x.status {
                if agent == "owner" && !owed.hold && owed.call != syntax::GATHER && x.owner_words.is_none() && (held || x.post_of.is_none()) {
                    found = Some((v, owed.step, owed.call.clone(), owed.head.clone()));
                    break;
                }
            }
            todo.extend(self.tickets.values().filter(|y| y.is_open() && (y.parent == Some(v) || y.under == Some(v))).map(|y| y.t));
        }
        let (u, step, call, head) = found?;
        self.tickets.edit().get_mut(&u).unwrap().owner_words = Some((t, text.clone(), names.to_vec(), told.to_vec(), inform.to_vec()));
        cx.line(Line::Forming { at: cx.now.clone(), t: u, step });
        cx.effect(Effect::Form { t: u, step, agent: "owner".into(), call, head, text: Loc { file: String::new(), offset: 0, len: 0 }, words: Some(words.clone()) });
        cx.effect(Effect::Changed { t: u });
        Some(u)
    }

    /// AGENT answered T's step a second time and that answer too has none of the forms the step takes (N-159.p16): the
    /// step stays with AGENT, and a post on T by T says so to the lead (the agent with role `lead`; the owner when
    /// there is none). It is about the work: what AGENT gave and what the step needs, never how forming went. A record
    /// from before this has no such post; it replays as recorded.
    fn unformed_twice(&mut self, cx: &mut Cx, t: u64, agent: &str) {
        let id = self.id(t);
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let Status::Awaits { owed, .. } = &x.status else { return };
        let (step, call, head) = (owed.step, owed.call.clone(), owed.head.clone());
        let n = x.last() + 1;
        let needs = match &head {
            Type::Variant(cs) => format!("one of {}", cs.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>().join(", ")),
            h => format!("a {h}"),
        };
        // a record from before the pass-up replays as recorded: with the post to the lead, or (older still) with nothing
        let posted = |r: &[Line]| r.iter().any(|l| matches!(l, Line::Posted { t: u, n: m, by: b, step: None, .. } if *u == t && *m == n && *b == id));
        let passed = |r: &[Line]| r.iter().any(|l| matches!(l, Line::PassedUp { t: u, step: s, .. } if *u == t && *s == step));
        // since the stop (N-101 p999, 2b): the ticket stops at the step, and the lead is asked Again or Ended; a record
        // that passed the step up replays so, and so does a coord with no `stopped` call
        let stopped = |r: &[Line]| r.iter().any(|l| matches!(l, Line::Stopped { t: u, .. } if *u == t));
        match cx.recorded.as_deref() {
            Some(r) if posted(r) => {}
            Some(r) if passed(r) => return self.pass_unread(cx, t, agent, &needs),
            Some(r) if !stopped(r) => return,
            _ if !self.defs.calls.contains_key(stop::CALL) => return self.pass_unread(cx, t, agent, &needs),
            _ => return self.unread_stops(cx, t, agent, &needs),
        }
        let lead = self.holders("lead").into_iter().next().unwrap_or_else(|| "owner".to_string());
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let text = format!(
            "{id}'s step {step} ({call}) waits on {agent}: {agent} answered twice, and neither answer is {needs}, which is what the step needs. The step stays with {agent}: it can be answered with `coord answer {id} HEAD ...`, or {agent} told what is missing."
        );
        let names = if lead == agent { Vec::new() } else { vec![lead] };
        x.posts.push(Post { n, at: cx.now.clone(), by: id.clone(), text: Text::New(text), head: None, step: None, names: names.clone(), told: Vec::new(), auto: false, moved: None, from: None, held: None, halted: false, dropped: false, routed: false, asks: Vec::new(), asks_open: false, asks_closed: None, answers: None, inform: Vec::new(), typed: None, cleaned_from: None, split_from: None, on: Vec::new(), also: Vec::new(), would_close: None, taken_back: false, joins: None, passes: None, comments: Vec::new(), into_turn: Vec::new(), kind: None, answering: Vec::new() });
        cx.line(x.posts.last().unwrap().posted(t));
        self.halt_post(cx, t, n);
        for a in names {
            cx.effect(Effect::Deliver { to: a });
        }
    }

    /// AGENT's second answer to T's step was not read either: T stops at the step as at any failure it can run again
    /// from (paper-research N-101 p999, 2b), Malformed with the reading's reason, and the lead (else the owner) is asked
    /// Again (the step is given afresh) or Ended. The program named the step's worker; it is not handed to another.
    fn unread_stops(&mut self, cx: &mut Cx, t: u64, agent: &str, needs: &str) {
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let Status::Awaits { owed, forming, .. } = &mut x.status else { return };
        *forming = None;
        let step = owed.step;
        let why = x.not_read.iter().rev().find(|(s, n)| *s == step && n.by == agent).map(|(_, n)| n.why.clone()).unwrap_or_default();
        let why = if why.is_empty() { format!("{agent} answered step {step} twice and neither answer was read as {needs}") } else { format!("{agent} answered step {step} twice and neither answer was read as {needs}: {why}") };
        x.remind_why = None;
        self.drive(cx, vec![Work::Throw(t, Raised { exc: "Malformed".into(), value: json!({"step": step, "why": why}) })]);
    }

    /// AGENT's second answer to T's step was not read either (paper-research N-101 p757): the step passes to the lead
    /// (an agent with role `lead` that is not AGENT), else to the owner, as an Ask nobody takes does. It is shown
    /// every answer not read, whole, with the reading's reasons; AGENT's later answers are posts, not answers to it.
    fn pass_unread(&mut self, cx: &mut Cx, t: u64, agent: &str, needs: &str) {
        let to = self.holders("lead").into_iter().find(|l| l != agent).filter(|_| agent != "owner").unwrap_or_else(|| "owner".to_string());
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let Status::Awaits { owed, .. } = &x.status else { return };
        let owed = owed.clone();
        let why = x.not_read.iter().rev().find(|(s, n)| *s == owed.step && n.by == agent).map(|(_, n)| n.why.clone()).unwrap_or_default();
        let why = if why.is_empty() { format!("{agent} answered twice and neither answer was read as {needs}") } else { format!("{agent} answered twice and neither answer was read as {needs}: {why}") };
        x.remind_why = None;
        cx.line(Line::PassedUp { at: cx.now.clone(), t, step: owed.step, from: agent.to_string(), to: to.clone(), why });
        self.await_step(cx, t, to, owed);
    }

    /// The runner refused a step on T that did not come from the program through coordd (`step_refused`, N-169): a
    /// post on T by T names the lead (the agent with role `lead`; the owner when there is none) and says who tried
    /// what (runner.rs `refused_text`). A record from before this has no such post; it replays as recorded.
    fn refused_step(&mut self, cx: &mut Cx, t: u64, value: &Value) {
        let id = self.id(t);
        let lead = self.holders("lead").into_iter().next().unwrap_or_else(|| "owner".to_string());
        let who = value.get("agent").and_then(Value::as_str).unwrap_or_default().to_string();
        let Some(x) = self.tickets.edit().get_mut(&t) else { return };
        let n = x.last() + 1;
        if cx.recorded.as_ref().is_some_and(|r| !r.iter().any(|l| matches!(l, Line::Posted { t: u, n: m, by: b, step: None, .. } if *u == t && *m == n && *b == id))) {
            return;
        }
        let text = crate::runner::refused_text(&id, value);
        let names = if lead == who { Vec::new() } else { vec![lead] };
        x.posts.push(Post { n, at: cx.now.clone(), by: id.clone(), text: Text::New(text), head: None, step: None, names: names.clone(), told: Vec::new(), auto: false, moved: None, from: None, held: None, halted: false, dropped: false, routed: false, asks: Vec::new(), asks_open: false, asks_closed: None, answers: None, inform: Vec::new(), typed: None, cleaned_from: None, split_from: None, on: Vec::new(), also: Vec::new(), would_close: None, taken_back: false, joins: None, passes: None, comments: Vec::new(), into_turn: Vec::new(), kind: None, answering: Vec::new() });
        cx.line(x.posts.last().unwrap().posted(t));
        self.halt_post(cx, t, n);
        for a in names {
            cx.effect(Effect::Deliver { to: a });
        }
        cx.effect(Effect::Changed { t });
    }

    /// The owner's words TEXT posted on T, naming NAMES (they did not form as the answer to the owner's step).
    fn owner_post(&mut self, cx: &mut Cx, t: u64, text: Text, names: Vec<String>, told: Vec<String>, inform: Vec<String>) {
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let n = x.last() + 1;
        let inform = inform.into_iter().filter(|a| !names.contains(a) && !told.contains(a)).collect();
        x.posts.push(Post { n, at: cx.now.clone(), by: "owner".into(), text, head: None, step: None, names: names.clone(), told: told.clone(), auto: false, moved: None, from: None, held: None, halted: false, dropped: false, routed: false, asks: Vec::new(), asks_open: false, asks_closed: None, answers: None, inform, typed: None, cleaned_from: None, split_from: None, on: Vec::new(), also: Vec::new(), would_close: None, taken_back: false, joins: None, passes: None, comments: Vec::new(), into_turn: Vec::new(), kind: None, answering: Vec::new() });
        x.mu.insert("owner".into(), n);
        cx.line(x.posts.last().unwrap().posted(t));
        cx.effect(Effect::Changed { t });
        if let Some(w) = self.owner_landed(cx, t, n, None) {
            self.drive(cx, vec![w]);
            return;
        }
        for a in names.iter().chain(&told) {
            cx.effect(Effect::Deliver { to: a.clone() });
        }
    }

    /// The owner's words landed on T as post N (a thread's first words, a Place, a pick, a Move, the owner's own
    /// post; design-two-systems §13): if T's kind has a `when owner_post` handler whose `where` holds, the post is
    /// held, given to no agent, and the handler runs in a ticket of its own under T (`handling`, numbered from
    /// HIDDEN), its value P: the words by reference, the agents the post names, the @NAMEs in the words, the other
    /// agents on T, and with `@all` in the words every agent on T (`all`; none without it). ESC: the agents the words
    /// stop if woken. The work that runs it; None when there is no handler, and the post goes as policy says.
    fn owner_landed(&mut self, cx: &mut Cx, t: u64, n: u32, hold: Option<u64>) -> Option<Work> {
        // his words are never held (docket-core3 N-867.p27; paper-research N-867.p29): writing on a ticket he archived,
        // or under one, says it matters to him again, and it is unarchived before they go. coordd writes his own
        // post's `Archive` line first; this covers the words a program lands (a Place, a pick, a Move)
        if let Some(a) = self.archived_by(t) {
            self.unarchive(cx, a);
        }
        let x = &self.tickets[&t];
        let h = x.handlers.iter().find(|h| h.ev == "owner_post")?.clone();
        x.posts.iter().find(|p| p.n == n)?;
        let p = self.owner_value(t, &[n]);
        let x = &self.tickets[&t];
        let mut env: Env = x.form.as_object().map(|o| o.iter().map(|(a, b)| (a.clone(), b.clone())).collect()).unwrap_or_default();
        env.insert("this".into(), json!(self.id(t)));
        env.insert(h.x.clone(), p.clone());
        if h.cond.as_ref().is_some_and(|c| eval(c, &env, &x.ctx) != json!(true)) {
            return None;
        }
        if !logged_run(cx, OWNER_POST, t) {
            // no run of its own on the record: one of his posts' handlers this release did not load is open on T, and
            // took it into its run (0.3's `gather`), so it is left untold as that one's own post is
            if self.gathering.values().any(|u| *u == t) {
                self.untold.insert((t, n));
            }
            return None;
        }
        let Some((c, body)) = self.handler_ticket(cx, t, OWNER_POST, &h, p, "owner", None) else {
            // a handler this release does not load: a line may yet tell the post as recorded (`as_recorded`); else
            // coordd sends it on after the replay (UNTOLD)
            self.untold.insert((t, n));
            if let Some(c) = self.handlers_unloaded.last() {
                self.gathering.insert(*c, t);
            }
            return None;
        };
        self.tickets.edit().get_mut(&c).unwrap().post_of = Some(PostOf { t, n, done: false, with: Vec::new(), hold });
        let id = self.id(c);
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.posts.iter_mut().find(|p| p.n == n).unwrap().held = Some(id);
        cx.effect(Effect::Changed { t });
        Some(Work::Run(c, body, env))
    }

    /// The `owner_post` event's value for his posts NS of T, in his order: one post, or his burst once `gather` has
    /// gathered it (E1b). The words by reference (the post's, or theirs joined), the agents they name, the @NAMEs in
    /// their words, the other agents on T, every agent on T (those named first), the post of T they answer, with `@all`
    /// in any of them every agent on T (`all`; none without it), and NS (`posts`).
    ///
    /// A burst whose posts do not all carry one same choice of his ([`Machine::his_choices`]; E1b hold 1, consult-3
    /// N-1446 p281, proto-lead p325) goes each post where he chose: `chosen` holds its posts that carry a choice, those
    /// going to the same agents in one part (`{posts, to}`, as route's Part), and `left` the posts that carry none
    /// (their numbers and their words), the holder's to decide, none when every post carries one (proto-reader p445:
    /// then nothing is asked); the names, the answer and `@all` are none, and the agents it names are those the posts
    /// left name, whom they were typed to. Otherwise `chosen` is empty, `left` is the whole burst, and it carries what
    /// each post carries, as one post does.
    ///
    /// Its posts that say `@all` ([`Machine::his_claimed`]) are not in `chosen`: they are `claimed`, one part (`{posts,
    /// to, text}`, `to` every agent on T), which one agent takes as a lone `@all` is taken (claims.call; proto-lead
    /// N-1446 p475, N-1134: "of whom one takes it"), and `text` leaves them out too: it is the words the holder reads.
    /// `claimed` is none when such a burst has no `@all` post, and for one post or a burst whose posts carry one same
    /// choice, where `chosen` is empty too.
    fn owner_value(&self, t: u64, ns: &[u32]) -> Value {
        let x = &self.tickets[&t];
        let chosen = self.his_choices(t, ns);
        let all_posts = self.his_claimed(t, ns);
        let claimed_to = chosen.iter().find(|(n, _)| all_posts.contains(n)).map(|(_, to)| to.clone());
        let chosen: Vec<(u32, Vec<String>)> = chosen.into_iter().filter(|(n, _)| !all_posts.contains(n)).collect();
        let (mut naming, mut names, mut answers, mut all): (Vec<String>, Vec<String>, Option<u32>, bool) = (Vec::new(), Vec::new(), None, false);
        for post in ns.iter().filter(|n| !chosen.iter().any(|(c, _)| c == *n) && !all_posts.contains(n)).filter_map(|n| x.posts.iter().find(|p| p.n == *n)) {
            let words = match self.text(&post.text) {
                Some(Text::New(s)) => s,
                _ => String::new(),
            };
            for a in &post.names {
                if !naming.contains(a) {
                    naming.push(a.clone());
                }
                if at_name(&words, a) && !names.contains(a) {
                    names.push(a.clone());
                }
            }
            answers = answers.or(post.answers);
            all |= at_name(&words, ALL);
        }
        let left: Vec<u32> = ns.iter().copied().filter(|n| !chosen.iter().any(|(c, _)| c == n) && !all_posts.contains(n)).collect();
        let mut parts: Vec<(Vec<u32>, Vec<String>)> = Vec::new();
        for (n, to) in chosen {
            let set = |a: &[String]| a.iter().cloned().collect::<std::collections::BTreeSet<String>>();
            match parts.iter_mut().find(|(_, t)| set(t) == set(&to)) {
                Some((ns, _)) => ns.push(n),
                None => parts.push((vec![n], to)),
            }
        }
        let chosen: Vec<Value> = parts.iter().map(|(ns, to)| json!({"posts": ns, "to": to})).collect();
        let others = self.others_on(t, &naming);
        let agents: Vec<String> = naming.iter().chain(&others).cloned().collect();
        let one = |n: u32| Text::Post { post: PostRef { t, n } };
        let words = |ns: &[u32]| match ns {
            [n] => one(*n),
            _ => Text::Joined { joined: ns.iter().map(|n| one(*n)).collect() },
        };
        let claimed = claimed_to.map(|to| json!({"posts": all_posts, "to": to, "text": words(&all_posts).value()}));
        // his words for the rest of the handler: the claimed posts are claims's, so route's step names what it reads
        // (proto-reader N-1446 p526)
        let rest: Vec<u32> = ns.iter().copied().filter(|n| !all_posts.contains(n)).collect();
        json!({"text": words(&rest).value(), "naming": naming, "names": names, "others": others, "agents": agents, "answers": answers, "all": all.then(|| agents.clone()), "posts": ns, "chosen": chosen, "claimed": claimed, "left": {"posts": left, "text": words(&left).value()}})
    }

    /// His own choices in his posts NS of T, when they do not all carry one same choice (E1b hold 1: a burst the holder
    /// decides, proto-reader N-1446 p307, proto-lead p325): each post of NS that carries one (`his_choice`),
    /// with whom he chose; empty when none of them carries one, or all carry the same agents (the burst goes as one post
    /// does). The one reading of it: the burst's value (`chosen`), and coordd's route job and its check of the holder's
    /// answer (clerk.rs `route_ok`).
    pub fn his_choices(&self, t: u64, ns: &[u32]) -> Vec<(u32, Vec<String>)> {
        let choices: Vec<Option<Vec<String>>> = ns.iter().map(|n| self.his_choice(t, *n)).collect();
        let set = |c: &Option<Vec<String>>| c.as_ref().map(|c| c.iter().cloned().collect::<std::collections::BTreeSet<String>>());
        if choices.iter().all(Option::is_none) || choices.iter().all(|c| c.is_some() && set(c) == set(&choices[0])) {
            return Vec::new();
        }
        ns.iter().zip(choices).filter_map(|(n, c)| c.map(|c| (*n, c))).collect()
    }

    /// The posts of his choices in NS of T ([`Machine::his_choices`]) that are his `@all` ([`Machine::all_state`]): in
    /// such a burst they go through claims, one agent taking them as one takes a lone `@all` (proto-lead N-1446 p475),
    /// not to every agent with the posts left to the holder. The one reading of it: the burst's value (`claimed`), and
    /// coordd's route job, whose burst leaves them out (claims has handed them before route is asked).
    pub fn his_claimed(&self, t: u64, ns: &[u32]) -> Vec<u32> {
        self.his_choices(t, ns).into_iter().map(|(n, _)| n).filter(|n| self.all_state(t, *n).is_some()).collect()
    }

    /// His own choice of whom his post N of T goes to, when it carries one: with `@all` in its words every agent on T;
    /// when it answers a post, or its words @NAME an agent it names, the agents it names, as base.kind's `To(p.naming)`
    /// sends it alone. None for a post that leaves it to the holder, or that would go to nobody.
    fn his_choice(&self, t: u64, n: u32) -> Option<Vec<String>> {
        let post = self.tickets.get(&t)?.posts.iter().find(|p| p.n == n)?;
        let words = match self.text(&post.text) {
            Some(Text::New(s)) => s,
            _ => String::new(),
        };
        let agents: Vec<String> = if at_name(&words, ALL) {
            post.names.iter().cloned().chain(self.others_on(t, &post.names)).collect()
        } else if post.answers.is_some() || post.names.iter().any(|a| at_name(&words, a)) {
            post.names.clone()
        } else {
            return None;
        };
        Some(agents).filter(|a| !a.is_empty())
    }

    /// VALUE, the value of a line of his that coordd sends (his message, UNTOLD), with the policy's hold on it
    /// (`owner_hold`), as his typed post's line has it (`Line::Post`'s `hold`, E1b), so a replay holds as it ran. Never
    /// a hold the sender wrote; none when the policy has none.
    fn hold_on(&self, value: &mut Value) {
        if let Some(o) = value.as_object_mut() {
            o.remove("hold");
            if let Some(s) = self.owner_hold.filter(|s| *s > 0) {
                o.insert("hold".into(), json!(s));
            }
        }
    }

    /// His post N of T landed under HOLD, the policy's hold on its line (E1b): his typed post, his post sent on after a
    /// replay (UNTOLD), his message's words (I-1474). With a hold, while a handler of T gathers his burst it joins that
    /// burst (`gather_into`), and Err is the burst's first post; else as `owner_landed`: its handler's work, or None
    /// when there is no handler.
    fn owner_lands(&mut self, cx: &mut Cx, t: u64, n: u32, hold: Option<u64>) -> Result<Option<Work>, u32> {
        if hold.is_some() {
            if let Some(first) = self.gather_into(cx, t, n) {
                return Err(first);
            }
        }
        Ok(self.owner_landed(cx, t, n, hold))
    }

    /// His post N of T, typed while a handler of T gathers his burst (waits at its owner's step of `gather`, E1b): it
    /// joins that burst, held with the others and run by no handler of its own, so the holder decides them once (the
    /// owner, N-1445 p9: "hold them for 15s by default and ask a docket program to split them"). The first post of
    /// the burst it joined, if one gathers.
    fn gather_into(&mut self, cx: &mut Cx, t: u64, n: u32) -> Option<u32> {
        let gathers = |u: &Ticket| u.is_open() && u.post_of.as_ref().is_some_and(|p| p.t == t && !p.done) && matches!(&u.status, Status::Awaits { agent, owed, forming: None, .. } if agent == "owner" && owed.call == syntax::GATHER);
        let u = self.tickets.values().find(|u| gathers(u)).map(|u| u.t)?;
        let of = self.tickets.edit().get_mut(&u).unwrap().post_of.as_mut().unwrap();
        of.with.push(n);
        let first = of.n;
        let id = self.id(u);
        self.tickets.edit().get_mut(&t).unwrap().posts.iter_mut().find(|p| p.n == n).unwrap().held = Some(id);
        cx.effect(Effect::Changed { t });
        cx.effect(Effect::Changed { t: u });
        Some(first)
    }

    /// The open thread ticket gathering his opening burst (`thread.kind`'s `gather` before `intake`, N-1104 p935/p936)
    /// that his next `owner_message` VALUE joins, if any: under the policy's hold (`value["hold"]`), to the same agent,
    /// with the same `follows` and no `tied_at` or `split_from` of its own.
    fn gathering_thread(&self, value: &Value) -> Option<u64> {
        if !value.get("hold").and_then(Value::as_u64).is_some_and(|s| s > 0) || value.get("split_from").is_some() || value.get("tied_at").is_some() {
            return None;
        }
        let to = value.get("to");
        let follows = value.get("follows").filter(|v| !v.is_null());
        self.tickets.values().rev().find(|u| {
            u.is_open()
                && u.kind == "thread"
                && u.post_of.is_none()
                && u.tie.is_none()
                && u.form.get("to") == to
                && u.form.get("follows").filter(|v| !v.is_null()) == follows
                && matches!(&u.status, Status::Awaits { agent, owed, forming: None, .. } if agent == "owner" && owed.call == syntax::GATHER)
        }).map(|u| u.t)
    }

    /// His next `owner_message` VALUE, sent while thread T gathers his opening burst before `intake`: its words join T's
    /// burst (its form's `text` and the `gather` step's argument), and any @NAMEs it carries join T's `names` (and make
    /// T `Named` when it names someone), so the burst is sorted and posted once.
    fn gather_into_thread(&mut self, cx: &mut Cx, t: u64, value: &Value) -> (bool, Result<String, String>) {
        let Some(x) = self.tickets.edit().get_mut(&t) else {
            return (false, Err(format!("There is no ticket {t}.")));
        };
        let more = value.get("text").and_then(Value::as_str).unwrap_or("");
        let prev = x.form.get("text").and_then(Value::as_str).unwrap_or("");
        let joined = if prev.is_empty() { more.to_string() } else if more.is_empty() { prev.to_string() } else { format!("{prev}\n{more}") };
        if let Some(o) = x.form.as_object_mut() {
            o.insert("text".into(), Value::String(joined.clone()));
            if let Some(ns) = value.get("names").and_then(Value::as_array) {
                let mut names: Vec<Value> = o.get("names").and_then(Value::as_array).cloned().unwrap_or_default();
                for n in ns {
                    if !names.contains(n) {
                        names.push(n.clone());
                    }
                }
                o.insert("names".into(), Value::Array(names));
            }
            if value.get("addressed").and_then(|a| a.get("head")).and_then(Value::as_str) == Some("Named") {
                o.insert("addressed".into(), json!({"head": "Named"}));
            }
        }
        let updated_names = x.form.get("names").cloned();
        let updated_addressed = x.form.get("addressed").cloned();
        for f in &mut x.k {
            if let Frame::Bind { env, .. } | Frame::Try { env, .. } = f {
                if let Some(ns) = &updated_names {
                    if env.contains_key("names") {
                        env.insert("names".into(), ns.clone());
                    }
                }
                if let Some(addr) = &updated_addressed {
                    if env.contains_key("addressed") {
                        env.insert("addressed".into(), addr.clone());
                    }
                }
            }
        }
        if let Status::Awaits { owed, .. } = &mut x.status {
            if let Some(o) = owed.args.as_object_mut() {
                o.insert("text".into(), Value::String(joined));
            }
        }
        cx.effect(Effect::Changed { t });
        (true, Ok(format!("Held with {}.", self.id(t))))
    }

    /// The hidden ticket under T that runs handler H of ticket event EV (`handling`), its value P, raised by BY.
    /// Its program, run: the one recorded in its Opened line (`handler_program`), which that line records again.
    /// SCOPE: what the handler sees, a `during`'s (its frame's), else T's form.
    /// On replay, a handler whose recorded program this release cannot parse is not loaded (the way back, clause 1;
    /// proto-lead N-1446 p431): no ticket, no Opened line, no NoCall; its number is kept (`not_loaded`), and None.
    fn handler_ticket(&mut self, cx: &mut Cx, t: u64, ev: &str, h: &syntax::When, p: Value, by: &str, scope: Option<&BTreeMap<String, Type>>) -> Option<(u64, Prog)> {
        let c = if self.delivery_agent(t).is_some() { self.next_desk() } else { self.next_hidden() };
        if self.recorded_handler(cx, c, t).is_some_and(|p| !p.is_empty() && syntax::parse_prog(&p, 1).is_err()) {
            self.not_loaded.insert(c);
            self.handlers_unloaded.push(c);
            return None;
        }
        let (body, program) = self.handler_program(cx, c, t, h, scope);
        let k = Kind { pattern: body.clone(), result: Type::Any, ..loose_kind(ev) };
        let mut y = self.new_ticket(c, ev, None, None, p, self.child_ctx(t), String::new(), &k, &cx.now);
        y.under = Some(t);
        y.raised_by = Some(by.into());
        y.handling = Some(ev.into());
        y.when = Some((h.from.clone(), h.line));
        self.tickets.edit().insert(c, y);
        cx.line(Line::Opened { at: cx.now.clone(), t: c, kind: ev.into(), parent: t, program, calls: BTreeMap::new() });
        Some((c, body))
    }

    /// What handler H of ticket T runs in the hidden ticket C, and the program text C's Opened line records: a
    /// handler's run carries its program as a ticket does (paper-research m1425), so a later change to a base handler
    /// never changes how an old log replays. The text is H's body, printed live and as recorded on replay; it runs
    /// parsed and checked (loosely, as a recorded ticket's program is) where H stands: T's form, `this`, and H's value,
    /// so live and replay run the same program. One that no longer parses raises NoCall. A recorded Opened line with no program (logs from before 12 Nov) runs today's handler,
    /// and records none again.
    fn handler_program(&self, cx: &Cx, c: u64, t: u64, h: &syntax::When, scope: Option<&BTreeMap<String, Type>>) -> (Prog, String) {
        let recorded = self.recorded_handler(cx, c, t);
        self.handler_run(t, h, recorded, scope)
    }

    /// The program text the record's Opened line gives hidden ticket C under T, on replay: empty when it has none (a
    /// log from before 12 Nov); None live, and for a delivery ticket's run, which writes no Opened line (it runs its
    /// handler as printed live, the program being its delivery ticket's, on the record: delivery.rs).
    fn recorded_handler(&self, cx: &Cx, c: u64, t: u64) -> Option<String> {
        cx.recorded.as_ref().filter(|_| self.delivery_agent(t).is_none()).map(|rec| {
            rec.iter()
                .find_map(|l| match l {
                    Line::Opened { t: ct, program, .. } if *ct == c => Some(program.clone()),
                    _ => None,
                })
                .unwrap_or_default()
        })
    }

    /// What handler H of ticket T runs, and the program text the log records for it: live (RECORDED None) H's body,
    /// printed; on replay the text recorded, or with none recorded (an old log) today's handler, recording none. The
    /// text runs parsed and checked where H stands (see `handler_program`). Both the hidden tickets' handlers and the
    /// hole's, which runs in T itself (`Line::Handling`), are chosen here.
    fn handler_run(&self, t: u64, h: &syntax::When, recorded: Option<String>, during: Option<&BTreeMap<String, Type>>) -> (Prog, String) {
        let text = match recorded {
            None => crate::print::prog(&h.body, 0),
            // an old log's: today's handler runs (paper-research m1425)
            Some(text) if text.is_empty() => return (h.body.clone(), text),
            Some(text) => text,
        };
        let mut p = match syntax::parse_prog(&text, 1) {
            Ok(p) => p,
            Err(e) => return (Prog::Raise { exc: "NoCall".into(), e: Some(Expr::Str(format!("the handler's recorded program no longer parses: {e}"))), at: 0 }, text),
        };
        let k = self.tickets.get(&t).and_then(|x| self.kind_of(x).ok());
        let mut scope: BTreeMap<String, Type> = during.cloned().unwrap_or_else(|| k.as_ref().map(|k| k.form.iter().cloned().collect()).unwrap_or_default());
        scope.insert("this".into(), Type::Ticket);
        scope.insert(h.x.clone(), syntax::event_type(&h.ev, &self.defs.events).unwrap_or(Type::Any));
        let raises = k.map(|k| k.raises).unwrap_or_default();
        let _ = check::check_in_with(&mut p, &Type::Any, &scope, &raises, &self.defs.calls, &self.defs.kinds, Some(&self.defs.named), false);
        (p, text)
    }

    /// AGENT's words landed on T as post N (rule Reply's post, its own `coord post`; not an answer to a step,
    /// design-two-systems §16): if T's kind has a `when agent_post` handler whose `where` holds, it runs in a hidden
    /// ticket under T, its value P the words by reference and the agent. The post is not held: it is shown and goes
    /// as policy says. T is not quiet while the handler runs, so its hole is asked once the post is converted.
    fn agent_landed(&mut self, cx: &mut Cx, t: u64, n: u32) -> Option<Work> {
        let x = &self.tickets[&t];
        let h = x.handlers.iter().find(|h| h.ev == AGENT_POST)?.clone();
        let post = x.posts.iter().find(|p| p.n == n)?;
        let by = post.by.clone();
        if by == "owner" || by == "coordd" || by == NOBODY || self.ticket_name(&by) {
            return None;
        }
        let owner_on = post.wakes().any(|w| w == "owner") || self.participants(t).iter().any(|a| a == "owner");
        let p = json!({"text": Text::Post { post: PostRef { t, n } }.value(), "by": by, "owner_on": owner_on});
        let mut env: Env = x.form.as_object().map(|o| o.iter().map(|(a, b)| (a.clone(), b.clone())).collect()).unwrap_or_default();
        env.insert("this".into(), json!(self.id(t)));
        env.insert(h.x.clone(), p.clone());
        if h.cond.as_ref().is_some_and(|c| eval(c, &env, &x.ctx) != json!(true)) || !logged_run(cx, AGENT_POST, t) {
            return None;
        }
        let (c, body) = self.handler_ticket(cx, t, AGENT_POST, &h, p, &by, None)?;
        cx.effect(Effect::Changed { t });
        Some(Work::Run(c, body, env))
    }

    /// `mark p MARKS` in T: the asks found in agent's post P (the `agent_post` event's value) stored on it, open.
    fn mark_post(&mut self, cx: &mut Cx, p: &Value, marks: &Value) -> Result<(), String> {
        let Some(Text::Post { post: PostRef { t, n } }) = p.get("text").and_then(Text::of) else {
            return Err(String::from("`mark` is given no agent's post"));
        };
        let marks: Vec<crate::asks::Mark> = marks.as_array().into_iter().flatten().filter_map(|m| serde_json::from_value::<crate::asks::Mark>(m.clone()).ok()).filter(|m| m.at[0] < m.at[1] && m.options.iter().all(|o| o[0] < o[1])).collect();
        let x = self.tickets.edit().get_mut(&t).ok_or("`mark`: no such ticket")?;
        let post = x.posts.iter_mut().find(|q| q.n == n).ok_or("`mark`: no such post")?;
        if post.by == "owner" {
            return Err(String::from("`mark` marks an agent's post, not the owner's"));
        }
        // a post typed an ask to him asks him already (pushed open); the watcher's marks on an old thread add passages
        // to it and close nothing
        post.asks_open = !marks.is_empty() || (post.asks_open && post.kind.as_ref().is_some_and(crate::line::PostKind::asks_owner));
        post.asks = marks.clone();
        cx.line(Line::Marked { at: cx.now.clone(), t, n, marks });
        cx.effect(Effect::Changed { t });
        Ok(())
    }

    /// Whether the asks of post N of X are open: it has some, the owner has not said there is nothing to answer, and
    /// since it its author has not posted on X, been woken by a post there (an inform is not enough), or been given a
    /// step of X (design-two-systems §16, 4). On a closed ticket (an agents' thread is closed from the start, part 9b)
    /// only a post told to the owner asks him, and a thread following X opened since closes it too: his words for a
    /// closed ticket open one (§16, 10; paper-research N-27, 3h).
    pub fn asks_open(&self, x: &Ticket, n: u32) -> bool {
        let Some(p) = x.posts.iter().find(|p| p.n == n) else { return false };
        may_ask(x, p) && self.closers(x, p).is_empty()
    }

    /// What closed the asks of post P of X, each with when, in order: a later post by its author or waking it, a step
    /// of X given it since (its delivery shows the posts up to P or later; a reminder or a refusal is no new step, and
    /// a step of a minder's answer since undone gives the asks back, paper-research m1463, 1), the owner's word that
    /// there is nothing to answer, the owner's cancel of X after it, a thread following X opened after it.
    pub(crate) fn closers(&self, x: &Ticket, p: &Post) -> Vec<(At, crate::line::AsksClosed)> {
        self.closers_in(x, p, &self.around(x))
    }

    /// The threads around X that may close its posts' asks (`Around`), from every ticket's (`Machine::arounds`).
    pub fn around(&self, x: &Ticket) -> Around {
        let all = self.arounds();
        Around { follows: all.follows.get(&self.id(x.t)).cloned().unwrap_or_default(), tied: all.tied.get(&x.t).cloned().unwrap_or_default() }
    }

    /// The asks of post P of X as a view shows them, with AROUND (X's, read at the first post that needs it): whether
    /// they are open, how they were closed, and which of the owner's targets would close them. What `asks_open`,
    /// `asks_closed` and `would_close` give, from one reading of what closed them where those made up to four, each
    /// with its own pass over every ticket: a long thread's view asked it of each of its posts, and was most of the
    /// follow round's hold on the lock (paper-research N-1396 p417).
    pub fn asks_shown(&self, x: &Ticket, p: &Post, around: &std::cell::OnceCell<Around>) -> (bool, Option<crate::line::AsksClosed>, Option<crate::line::WouldClose>) {
        let may = may_ask(x, p);
        let asked = asked_him(p);
        if !may && !asked {
            return (false, None, None);
        }
        let closers = self.closers_in(x, p, around.get_or_init(|| self.around(x)));
        let open = may && closers.is_empty();
        let closed = if !asked || open { None } else { closers.into_iter().next().map(|(_, c)| c) };
        (open, closed, open.then(|| self.closes_open(x, p)))
    }

    /// `closers` with AROUND, X's threads that may close its asks, read before. A post typed an ask to the owner
    /// (`PostKind::asks_owner`) closes only on what is his or the asker's own act on it (paper-research N-1446 p1102):
    /// his post waking the asker, his "nothing to answer", the asker's withdrawal (`coord ask --drop`, an end), his
    /// cancel, and a thread his words opened following X or tied at it that reaches the asker: typed to it, or its
    /// first post waking it (N-1479 p300). His post aimed at one post (`answers`, `also`) closes only the ask it
    /// answers; a plain one closes each earlier ask of the asker's (p305). Its taking back closes it (`may_ask`). The
    /// asker's next post, another's post waking the asker and a step given it leave it open: an ask to him waits on him
    /// (proto-window N-1479 p275).
    fn closers_in(&self, x: &Ticket, p: &Post, around: &Around) -> Vec<(At, crate::line::AsksClosed)> {
        use crate::line::AsksClosed;
        let a = &p.by;
        let his = p.kind.as_ref().is_some_and(crate::line::PostKind::asks_owner);
        // his post that says which posts it answers answers those; a plain one, whatever the asker asked him before it
        let his_answer = |q: &Post| q.by == "owner" && ((q.answers.is_none() && q.also.is_empty()) || q.answers == Some(p.n) || x.also_here(q).contains(&p.n));
        let mut out = Vec::new();
        for q in x.posts.iter().filter(|q| q.n > p.n && q.visible()) {
            if q.by == *a && !his {
                out.push((q.at.clone(), AsksClosed::Posted { post: q.n }));
            } else if q.wakes().any(|w| w == a) && (!his || his_answer(q)) {
                out.push((q.at.clone(), AsksClosed::Answered { post: q.n, by: q.by.clone() }));
            }
        }
        // the step itself (`T.sN`), not its reminder or what came after it
        let step = |d: &str| step_id(d).filter(|(_, _, itself)| *itself).map(|(_, s, _)| s);
        for g in x.given.iter().filter(|g| !his && g.to == *a && g.upto >= p.n) {
            let Some(s) = step(&g.delivery) else { continue };
            if x.shaped.iter().any(|h| h.undone && h.steps < s && s <= h.undone_steps) {
                continue;
            }
            let minder = x.shaped.iter().rev().find(|h| h.refused.is_none() && h.steps < s).filter(|h| h.answer.get("head").and_then(Value::as_str) == Some("Program")).map(|h| h.n);
            out.push((g.at.clone(), AsksClosed::Told { step: s, minder }));
        }
        if let Some(at) = x.nothing.get(&p.n) {
            out.push((at.clone(), AsksClosed::NothingToAnswer));
        }
        if let Some((at, by, why)) = x.asks_withdrawn.get(&p.n) {
            out.push((at.clone(), AsksClosed::Withdrawn { by: by.clone(), why: why.clone() }));
        }
        // the owner's cancel is his act on the whole ticket: what was asked before it closes with it (paper-research
        // N-27 post 35, 2); a close of any other kind leaves it open, and a post after his cancel asks again
        if let (Status::Failed(r), Some(at)) = (&x.status, &x.closed_at) {
            if r.exc == "Cancelled" && r.value.get("by").and_then(Value::as_str) == Some("owner") && p.at <= *at {
                out.push((at.clone(), AsksClosed::Cancelled));
            }
        }
        // his words open the threads that follow X or are tied at a post (`owner_message`, raised by him); an agent's
        // thread there is no answer of his
        let by_him = |y: &Ticket| !his || y.raised_by.as_deref() == Some("owner");
        // a thread that reaches the asker: typed to it, or its first post wakes it (N-447 p18; the owner's words open a
        // thread before its first post is made)
        let reaches = |y: &Ticket| {
            let typed = y.form.get("to").and_then(Value::as_str) == Some(a.as_str()) || y.form.get("typed").and_then(Value::as_array).is_some_and(|ts| ts.iter().any(|v| v.as_str() == Some(a.as_str())));
            typed || y.posts.first().is_some_and(|f| f.wakes().any(|w| w == a))
        };
        // a thread following X; for an ask typed to him, his answer only: one that reaches the asker (p300)
        for y in around.follows.iter().filter_map(|t| self.tickets.get(t)).filter(|y| y.opened_at > p.at && by_him(y) && (!his || reaches(y))) {
            out.push((y.opened_at.clone(), AsksClosed::Followed { ticket: self.id(y.t) }));
        }
        // an answer given in a subthread: a thread tied at the post (its tie, not where it was opened) that reaches the
        // asker
        for y in around.tied.iter().filter(|(n, _)| *n == p.n).filter_map(|(_, t)| self.tickets.get(t)).filter(|y| y.opened_at > p.at && by_him(y) && reaches(y)) {
            out.push((y.opened_at.clone(), AsksClosed::Subthread { ticket: self.id(y.t) }));
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// BY withdraws the open asks of post N of T, with WHY (paper-research N-461; N-447 p4, p5): they close as
    /// `AsksClosed::Withdrawn`, no delivery is made and nobody is woken. Who may is coordd's to check (the post's
    /// author, or an agent above it); here the owner is refused, as his act for an ask is "nothing to answer", and so
    /// is a post that asks nothing open. `end` (docket-core4, threads piece 1) calls this for the ender's own asks
    /// after its own check, so a refused end changes nothing; its `ended` line then replays them.
    fn withdraw_asks(&mut self, cx: &mut Cx, at: &At, t: u64, n: u32, by: &str, why: &str) -> Result<String, String> {
        if by == "owner" {
            return Err(String::from("The owner closes an ask with \"nothing to answer\"; withdrawing it is the asker's."));
        }
        if why.trim().is_empty() || why.trim().contains('\n') {
            return Err(String::from("A withdrawal says why, in one line."));
        }
        let id = self.id(t);
        let x = self.tickets.get(&t).ok_or_else(|| format!("There is no ticket {id}."))?;
        let p = x.posts.iter().find(|p| p.n == n).ok_or_else(|| format!("{id} has no post {n}."))?;
        if p.by == "owner" {
            return Err(format!("Post {n} of {id} is the owner's: it asks him nothing."));
        }
        if !self.asks_open(x, n) {
            return Err(format!("Post {n} of {id} asks nothing open."));
        }
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.posts.iter_mut().find(|p| p.n == n).unwrap().asks_open = false;
        x.asks_withdrawn.insert(n, (at.clone(), by.to_string(), why.trim().to_string()));
        cx.effect(Effect::Changed { t });
        Ok(format!("Post {n} of {id}: its asks withdrawn."))
    }

    /// The posts of X whose asks are open, other than BY's, each with its author: what `end` refuses on.
    pub fn others_open_asks(&self, x: &Ticket, by: &str) -> Vec<(String, u32)> {
        let around = std::cell::OnceCell::new();
        x.posts.iter().filter(|p| p.by != by && self.asks_shown(x, p, &around).0).map(|p| (p.by.clone(), p.n)).collect()
    }

    /// Which of the owner's three targets would close the open asks of post N of X (paper-research N-447 p16, 1; the
    /// shape p18); None when they are not open. On a closed ticket each does: his words there open a thread that
    /// follows it (`Followed`), and a subthread typed to the asker closes them (`Subthread`). An ask typed to him closes
    /// only on a following thread that reaches the asker (N-1479 p300): his reply typed to it does, and his words at
    /// the foot typed to nobody are Routed. On an open one his reply
    /// typed to the asker does (base.kind's `when owner_post` sends an answer to whom it was typed to), and so does a
    /// subthread; his post at the foot typed to nobody does when the asker is the only other agent on X (base's
    /// handler sends it there), and is Routed otherwise: the watcher's `route`, or with no handler policy's `notify`,
    /// decides. A kind with a handler of its own is taken as base's.
    pub fn would_close(&self, x: &Ticket, n: u32) -> Option<crate::line::WouldClose> {
        if !self.asks_open(x, n) {
            return None;
        }
        Some(self.closes_open(x, x.posts.iter().find(|p| p.n == n)?))
    }

    /// `would_close` of post P of X, whose asks are open.
    fn closes_open(&self, x: &Ticket, p: &Post) -> crate::line::WouldClose {
        use crate::line::{Closes, WouldClose};
        if !x.is_open() {
            let foot = if p.kind.as_ref().is_some_and(crate::line::PostKind::asks_owner) { Closes::Routed } else { Closes::Yes };
            return WouldClose { foot, reply: Closes::Yes, subthread: Closes::Yes };
        }
        let foot = if x.handlers.iter().any(|h| h.ev == "owner_post") && self.others_on(x.t, &[]) == [p.by.clone()] { Closes::Yes } else { Closes::Routed };
        WouldClose { foot, reply: Closes::Yes, subthread: Closes::Yes }
    }

    /// How the asks of post N of X were closed, when it asked him (`asked_him`: marked, or typed an ask to him) and they
    /// are not open: the first of what closed them.
    /// None on a closed ticket nothing closed them on.
    pub fn asks_closed(&self, x: &Ticket, n: u32) -> Option<crate::line::AsksClosed> {
        let p = x.posts.iter().find(|p| p.n == n)?;
        if !asked_him(p) || self.asks_open(x, n) {
            return None;
        }
        self.closers(x, p).into_iter().next().map(|(_, c)| c)
    }

    /// The agents whose asks on resting or closed X wait on the owner, each with its post: open, and the hole asked since
    /// it and the minder resting (or no minder): until then the minder is still looking (paper-research m1392, e). Not a
    /// post the owner is only informed of: named or told, he is asked. Its asks are the post itself, typed an ask to him
    /// (`PostKind::asks_owner`, open from its push), or the passages marked on it (`mark`, on records from before the
    /// watcher's `asks` went, proto-lead N-1479 p247); both read as `asks_shown`.
    pub fn asks_waiting(&self, x: &Ticket) -> Vec<(String, u32)> {
        if !matches!(x.status, Status::Rests { .. }) && x.is_open() {
            return Vec::new();
        }
        // on a closed ticket no minder looks: an open ask waits on the owner at once (3h)
        let minder = x.is_open() && x.handlers.iter().any(|h| h.ev == "hole");
        let mut out: Vec<(String, u32)> = Vec::new();
        let around = std::cell::OnceCell::new();
        let mut k = 0;
        for p in &x.posts {
            if p.by != "coordd" {
                k += 1;
            }
            let asked = !minder || x.asked_at.is_some_and(|m| m.0 >= k);
            // a post that only informs him asks him nothing (paper-research N-800.p12): orders between agents are not
            // his to answer
            let only_informed = p.inform.iter().any(|a| a == "owner") && !p.names.iter().chain(&p.told).any(|a| a == "owner");
            if asked && !only_informed && self.asks_shown(x, p, &around).0 {
                out.retain(|(a, _)| *a != p.by);
                out.push((p.by.clone(), p.n));
            }
        }
        out
    }

    /// The agents on T other than NAMING: those who take part in it (`participants`), not the owner, coordd or a
    /// ticket posting on itself.
    fn others_on(&self, t: u64, naming: &[String]) -> Vec<String> {
        // nor the relay, here or as another project wrote it: records from before N-65 hold its notices
        let relay = |a: &str| a == "coord-relay" || a.ends_with("/coord-relay");
        self.participants(t).into_iter().filter(|a| a != "owner" && a != "coordd" && a != NOBODY && !relay(a) && !self.ticket_name(a) && !naming.contains(a)).collect()
    }

    /// NAME, on a ticket, is a ticket posting on itself and no agent: from the era [`ERA_NAMES`] only a ticket's id
    /// exactly; before it, any name `parse_ticket` reads, as the record's lines from then were drawn.
    fn ticket_name(&self, a: &str) -> bool {
        parse_ticket(a).is_some_and(|t| self.era < ERA_NAMES || self.id(t) == a)
    }

    /// NAME reads as a ticket's number but is no ticket's id (`consult-3`, `a2`): a name the era [`ERA_NAMES`] makes an
    /// agent's.
    pub fn named_like_a_ticket(&self, a: &str) -> bool {
        parse_ticket(a).is_some_and(|t| self.id(t) != a)
    }

    /// A ticket of the record has a name on it that reads as a ticket's number and is none ([`ERA_NAMES`]: coordd
    /// begins that era at a start then).
    pub fn has_names_like_tickets(&self) -> bool {
        self.tickets.keys().any(|t| self.participants(*t).iter().any(|a| self.named_like_a_ticket(a)))
    }

    /// The agents whose work WORDS are about (the owner's `@all`, `claims`; consult-lead N-1158 post 1): for each open
    /// ticket the words name by its id (`N-12`), the agents its step awaits, or the step of a ticket under it.
    pub fn about(&self, words: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for w in words.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')) {
            let Some(t) = w.contains('-').then(|| parse_ticket(w)).flatten() else { continue };
            if !self.tickets.contains_key(&t) || !self.id(t).eq_ignore_ascii_case(w) {
                continue;
            }
            let mut todo = vec![t];
            while let Some(u) = todo.pop() {
                let Some(x) = self.tickets.get(&u) else { continue };
                if let Status::Awaits { agent, .. } = &x.status {
                    if agent != "owner" && agent != WATCHER {
                        out.insert(agent.clone());
                    }
                }
                todo.extend(x.children.iter().copied());
            }
        }
        out
    }

    /// Where the owner's `@all` post N of T stands ([`ALL`]; consult-lead N-1158 post 1, N-807 post 90 and N-412 post
    /// 1900): None when N is no `@all` of his. Anybody it reached may claim it: the first answer to it is the claim,
    /// whatever its words. Passing on is an act of its own (`Input::Pass`), by the agent it is handed to.
    pub fn all_state(&self, t: u64, n: u32) -> Option<AllState> {
        let x = self.tickets.get(&t)?;
        all_of(&x.posts, n, |t| match self.text(t) {
            Some(Text::New(w)) => Some(w),
            _ => None,
        })
    }

    /// The owner's `@all` on T handed to AGENT and claimed by nobody, the last such up to post UPTO: what AGENT's
    /// turn-end answer there answers, so that it is the claim (the rule Reply's post, and a part for T of a last message
    /// split over threads, `ReplyThread::all`; consult-lead N-412 post 1900, consult-3 N-807 post 118).
    pub fn all_held(&self, t: u64, agent: &str, upto: u32) -> Option<u32> {
        let x = self.tickets.get(&t)?;
        x.posts.iter().filter(|p| p.by == "owner" && p.n <= upto).map(|p| p.n).filter(|m| self.all_state(t, *m).is_some_and(|st| st.claim.is_none() && st.handed.iter().any(|h| h == agent))).last()
    }

    /// The record holds an `@all` of the owner's ([`ERA`]: coordd begins the era at a start then).
    pub fn has_all(&self) -> bool {
        self.tickets.values().any(|x| x.posts.iter().any(|p| p.by == "owner" && self.all_state(x.t, p.n).is_some()))
    }

    /// INPUT brings the owner's words with `@all` in them, by his post, his message or a ticket he opens ([`ERA`]: coordd
    /// begins the era just before it, in a record that has no era yet).
    pub fn carries_all(&self, input: &Input) -> bool {
        let says = |w: &str| at_name(w, ALL);
        match input {
            Input::Post { by, text, .. } if by == "owner" => matches!(self.text(text), Some(Text::New(w)) if says(&w)),
            Input::Open { opener, text: Some(text), .. } if opener == "owner" => matches!(self.text(text), Some(Text::New(w)) if says(&w)),
            Input::Event { name, value } if name == "owner_message" => value.get("text").and_then(Value::as_str).is_some_and(says),
            _ => false,
        }
    }

    /// Why agent BY's answer to the owner's `@all` post N of T is not posted ([`Machine::all_state`]): another agent
    /// claimed it already. The words say who answered and where, and that BY may post its text on the thread as an
    /// ordinary post. The claim's own agent may answer it again.
    fn taken(&self, t: u64, n: u32, by: &str) -> Option<String> {
        let st = self.all_state(t, n)?;
        let id = self.id(t);
        let (a, m) = st.claim.as_ref().filter(|(a, _)| a != by)?;
        Some(format!("{id}.p{n} is the owner's @all, and {a} answered it in {id}.p{m}: it is theirs. If yours adds something, post it on {id} without answering p{n}."))
    }

    /// Why BY may not pass the owner's `@all` post N of T on to agent TO (`Input::Pass`; consult-lead N-412 post 1900):
    /// N is no `@all` of his; it was claimed; BY does not hold it (only the agent it is handed to passes it on); TO is
    /// BY; or TO held it before and passed it on (no ping-pong). In each it stays with whom it is handed to.
    pub fn pass_refused(&self, t: u64, n: u32, by: &str, to: &str) -> Option<String> {
        let id = self.id(t);
        let Some(st) = self.all_state(t, n) else {
            return Some(format!("{id}.p{n} is not the owner's @all: only an @all of his is passed on. To hand a post to another agent, name them in a post of your own."));
        };
        if let Some((a, m)) = &st.claim {
            return Some(format!("{id}.p{n} is the owner's @all, and {a} answered it in {id}.p{m}: it is theirs, and nothing is passed on."));
        }
        if !st.handed.iter().any(|h| h == by) {
            return Some(format!("{id}.p{n} is the owner's @all, handed to {}: only they pass it on. To take it, answer it.", st.handed.join(", ")));
        }
        if to == by {
            return Some(format!("{id}.p{n} is the owner's @all, and it is handed to you already: answer it, or pass it to another agent."));
        }
        if let Some((_, m)) = st.passed.iter().find(|(a, _)| a == to) {
            return Some(format!("{to} held {id}.p{n}, the owner's @all, and passed it on in {id}.p{m}: it is not passed back, and it stays with you. Answer it, or pass it to another agent."));
        }
        None
    }

    /// The owner's post the handler running in T holds (T, or the nearest above it that runs one): its ticket.
    fn holder(&self, t: u64) -> Result<u64, String> {
        let mut u = t;
        while self.tickets[&u].post_of.is_none() {
            u = self.tickets[&u].parent.ok_or("`tell` and `drop` are used where no owner's post is held")?;
        }
        if self.tickets[&u].post_of.as_ref().is_some_and(|p| p.done) {
            return Err(String::from("the owner's post was told or dropped already"));
        }
        Ok(u)
    }

    /// `tell AGENTS p [joining J]` in T: the held post wakes AGENTS; every other agent on its ticket is informed; J,
    /// the earlier post of his it goes on from.
    /// ONLY: of the posts it holds, those it tells (`tell AGENTS NS`, a Split's part, E1b); none, all.
    fn tell(&mut self, cx: &mut Cx, t: u64, agents: Vec<String>, joins: Option<u32>, only: Option<Vec<u32>>) -> Result<(), String> {
        self.tell_held(cx, t, agents, true, joins, only)
    }

    /// A replay of an input whose recorded lines formed the owner's words against the question about a held post (a
    /// record from before N-807.p78): it replays as it was.
    fn formed_held(&self, cx: &Cx) -> bool {
        cx.recorded.as_ref().is_some_and(|r| r.iter().any(|l| matches!(l, Line::Forming { t: u, .. } if self.tickets.get(u).is_some_and(|y| y.post_of.is_some()))))
    }

    /// Whether U is an `owner_post` handler holding its post and waiting on the owner's answer to a question about it
    /// (base.kind's `clarify`), with no forming call out on it (his words being formed are its `owner_words`): a record
    /// from before N-807.p78 formed his words against it, and his posts that came while that call was out were posts of
    /// their own beside it, replayed so (consult-3, N-412.p1355: research's N-1104 at 14:15:17Z).
    fn unanswered_hold(&self, u: &Ticket) -> bool {
        matches!(&u.status, Status::Awaits { agent, owed, forming: None, .. } if agent == "owner" && Self::question_about_held(u, owed))
    }

    /// Whether OWED, a step of the owner's in U, is his question about the posts U holds (`unanswered_hold`, as it is
    /// put; `Line::Step`'s `asked`).
    fn question_about_held(u: &Ticket, owed: &Owed) -> bool {
        // not a burst being gathered (E1b): his next words join it, and hand nothing
        u.is_open() && u.post_of.as_ref().is_some_and(|p| !p.done) && u.owner_words.is_none() && !owed.hold && owed.call != syntax::GATHER
    }

    /// U's step OWED, put to the owner, asks him about the posts U holds (base.kind's `clarify`): while it waits they
    /// are untold and unshown, and hold back none of the posts after them (`Ticket::asked`), which reach whom they
    /// wake now (`posts_reached`); an agent reads them before the post he is asked about (proto-lead N-1446 p569,
    /// (c2)). That post goes as he answers: told (`tell_held`, to each agent he chose by `p{n}.told`), or dropped,
    /// seen by nobody.
    fn asked_about(&mut self, cx: &mut Cx, u: u64) {
        let Some(of) = self.tickets[&u].post_of.clone() else { return };
        let on = of.t;
        let before = self.tickets[&on].his_shown();
        let x = self.tickets.edit().get_mut(&on).unwrap();
        let held: Vec<u32> = of.posts().into_iter().filter(|m| x.posts.iter().any(|p| p.n == *m && p.held.is_some())).collect();
        x.was_asked.extend(held.iter().copied());
        x.asked.extend(held);
        self.posts_reached(cx, on, &before);
        for a in self.participants(on) {
            cx.effect(Effect::Deliver { to: a });
        }
        cx.effect(Effect::Changed { t: on });
    }

    /// The owner's posts on T held under a question to him he did not answer (paper-research N-807.p78, 3): each is
    /// handed to every agent on T, as a post naming them all is, in his order, and its handler ends. Nothing withdraws a
    /// post of his but his own press of "drop it". How many.
    fn hand_unanswered(&mut self, cx: &mut Cx, t: u64) -> usize {
        let mut held: Vec<(u32, u64)> = self.unanswered_holds().into_iter().filter(|(t2, _, _)| *t2 == t).map(|(_, n, u)| (n, u)).collect();
        held.sort();
        for (_, u) in &held {
            self.hand_one(cx, *u);
        }
        held.len()
    }

    /// The owner's posts held under a question to him: (thread, post, handler), in his order.
    pub fn unanswered_holds(&self) -> Vec<(u64, u32, u64)> {
        let mut out: Vec<(u64, u32, u64)> = self.tickets.values().filter(|u| self.unanswered_hold(u)).filter_map(|u| u.post_of.as_ref().map(|p| (p.t, p.n, u.t))).collect();
        out.sort();
        out
    }

    /// The post handler U holds goes to every agent on its thread, as a post naming them all; U ends, cancelled.
    fn hand_one(&mut self, cx: &mut Cx, u: u64) {
        let Some(PostOf { t, n, .. }) = self.tickets[&u].post_of.clone() else { return };
        let naming = self.tickets[&t].posts.iter().find(|p| p.n == n).map(|p| p.names.clone()).unwrap_or_default();
        let all: Vec<String> = naming.iter().cloned().chain(self.others_on(t, &naming)).collect();
        // held and not done (`unanswered_hold`): `holder` finds it
        if self.tell_held(cx, u, all, false, None, None).is_err() {
            return;
        }
        let why = "the owner wrote again without answering its question: his post is handed to every agent on the thread";
        let r = Raised { exc: "Cancelled".into(), value: json!({"by": "owner", "why": why}) };
        let mut work = Vec::new();
        self.cancel(cx, u, "owner", why, &r, &mut work);
        self.drive(cx, work);
    }

    /// `tell` with CHOSEN: an agent's answer may have chosen whom (halted in slow mode, saying who chose); not for
    /// the owner's own rule (`hand_unanswered`). JOINS: the earlier post of his on the ticket this one goes on from,
    /// kept on the post and its `Told` line; one that is not an earlier post of his there is left out.
    fn tell_held(&mut self, cx: &mut Cx, t: u64, agents: Vec<String>, chosen: bool, joins: Option<u32>, only: Option<Vec<u32>>) -> Result<(), String> {
        let u = self.holder(t)?;
        let of = self.tickets[&u].post_of.clone().unwrap();
        let on = of.t;
        // the posts it tells, in his order: the one post it holds, or of his burst those still held (E1b), and of
        // them ONLY those named, a Split's part
        let still = |m: &u32| of.with.is_empty() || self.tickets[&on].posts.iter().any(|p| p.n == *m && p.held.is_some());
        let ns: Vec<u32> = of.posts().into_iter().filter(|m| still(m) && only.as_ref().is_none_or(|o| o.contains(m))).collect();
        let Some(&first) = ns.first() else {
            return Err(format!("none of the posts {} is one of his it still holds", only.unwrap_or_default().iter().map(|m| format!("#{m}")).collect::<Vec<_>>().join(", ")));
        };
        let mut to: Vec<String> = Vec::new();
        for a in agents.into_iter().filter(|a| a != "owner" && a != NOBODY) {
            if !to.contains(&a) {
                to.push(a);
            }
        }
        let mut naming: Vec<String> = Vec::new();
        for p in self.tickets[&on].posts.iter().filter(|p| ns.contains(&p.n)) {
            for a in &p.names {
                if !naming.contains(a) {
                    naming.push(a.clone());
                }
            }
        }
        let joins = joins.filter(|j| *j < first && self.tickets[&on].posts.iter().any(|q| q.n == *j && q.by == "owner"));
        let mut inform: Vec<String> = naming.iter().filter(|a| !to.contains(a)).cloned().collect();
        inform.extend(self.others_on(on, &naming).into_iter().filter(|a| !to.contains(a)));
        // the owner's words are not halted, nor whom a rule of the handler tells (he named or typed to them, one agent
        // on the ticket, an answer to an ask, his own `clarify`); whom an agent's answer chose (the watcher's `route`)
        // is, saying who chose (slow mode; the owner N-94, 1)
        let chose = self.tickets[&u].last_by.clone().filter(|b| b != "owner" && chosen);
        let halts = self.slow(on) && chose.is_some();
        let before = self.tickets[&on].his_shown();
        let mut asked: Vec<u32> = Vec::new();
        // each post of the burst after its first goes on from that first, one thought (hs-ls's `joins`)
        for (i, m) in ns.iter().enumerate() {
            let joins = if i == 0 { joins } else { Some(first) };
            let x = self.tickets.edit().get_mut(&on).unwrap();
            if x.asked.remove(m) {
                asked.push(*m);
            }
            let p = x.posts.iter_mut().find(|p| p.n == *m).unwrap();
            p.held = None;
            p.routed = true;
            p.told = to.clone();
            p.inform = inform.clone();
            p.joins = joins;
            cx.line(Line::Told { at: cx.now.clone(), t: on, n: *m, to: to.clone(), inform: inform.clone(), joins });
            if halts {
                self.tickets.edit().get_mut(&on).unwrap().posts.iter_mut().find(|p| p.n == *m).unwrap().halted = true;
            }
        }
        // halted, the tell's posts are one card, and reach whom they wake together when his card lets them go
        // (`approve`; docket-core4 N-1446 p184); not halted, they reach them now, the burst at once, unless a post
        // before them is still held or halted, and then when that one goes (`posts_reached`)
        if halts {
            self.halt(cx, on, Halting::Deliver { post: first, with: ns[1..].to_vec(), edited: false, chose });
        }
        self.tell_asked(on, &asked);
        self.posts_reached(cx, on, &before);
        // done once none of its posts is held any more
        let left = of.posts().iter().any(|m| !ns.contains(m) && self.tickets[&on].posts.iter().any(|p| p.n == *m && p.held.is_some()));
        self.tickets.edit().get_mut(&u).unwrap().post_of.as_mut().unwrap().done = !left;
        for a in to.iter().chain(&inform) {
            cx.effect(Effect::Deliver { to: a.clone() });
        }
        cx.effect(Effect::Changed { t: on });
        Ok(())
    }

    /// OWNER_TO for each agent of TO that the owner's posts NS of T wake, none for whom they only inform, while a
    /// binding is on it (the brain, 0.3; proto-mind N-1446 p36): `{agent, posts, tied_at}`, POSTS their ids in his
    /// order, TIED_AT the last of them. Raised through coordd (`Effect::Event`), so the record has it as an `event` line
    /// and a start replays that line, not this. Raised when his post reaches whom it wakes (proto-lead N-1446 p155),
    /// that is when an agent may first be shown it ([`Machine::posts_reached`]), never before: at the hand
    /// (`tell_held`), a held post being handed once (`holder`); or, its delivery halted (slow mode, the watcher chose),
    /// when his card lets it go (`approve`), for whom it wakes as he left them; none while his card holds it.
    fn owner_to(&self, cx: &mut Cx, t: u64, ns: &[u32], to: &[String]) {
        if !self.bound(syntax::OWNER_TO) {
            return;
        }
        let id = self.id(t);
        let posts: Vec<String> = ns.iter().map(|n| format!("{id}.p{n}")).collect();
        let Some(tied_at) = posts.last().cloned() else { return };
        for a in to {
            cx.effect(Effect::Event { name: syntax::OWNER_TO.into(), value: json!({"agent": a, "posts": posts, "tied_at": tied_at}) });
        }
    }

    /// His posts on T that an agent may be shown now and were not at BEFORE (`Ticket::his_shown` then): each has
    /// reached whom it wakes, so `owner_to` is raised for them here, one for each agent they wake, with that agent's
    /// posts in his order (docket-core4 N-1446 p527, proto-lead p530). A post of his told while one before it is still
    /// held or halted is not shown before that one goes (an `@all` his burst's claims told, a part his Unclear told):
    /// it reaches whom it wakes then, when that one is told, dropped, taken back, released or let through by his card.
    /// One his own question holds (`Ticket::asked`) holds back nothing: the posts after it are shown, and raised for,
    /// as he asks (`asked_about`), and it is raised for when he tells it, not before (proto-lead p569, (c2)). BEFORE is
    /// read just ahead of the step that changes it, so a post is raised for once. Only his words are raised for
    /// (`Machine::his_words`): an agent's opener that thread.kind before ls-opener posted again as his is not (N-1445
    /// #2; proto-lead N-1446 p587, proto-reader and consult-3 p648).
    fn posts_reached(&self, cx: &mut Cx, t: u64, before: &[u32]) {
        let Some(x) = self.tickets.get(&t) else { return };
        let shown = x.his_shown();
        let mut woken: Vec<(String, Vec<u32>)> = Vec::new();
        for p in x.posts.iter().filter(|p| shown.contains(&p.n) && !before.contains(&p.n) && self.his_words(t, p.n)) {
            for a in p.wakes().filter(|a| *a != "owner" && *a != NOBODY) {
                match woken.iter_mut().find(|(b, _)| b == a) {
                    Some((_, ns)) if ns.last() != Some(&p.n) => ns.push(p.n),
                    Some(_) => {}
                    None => woken.push((a.clone(), vec![p.n])),
                }
            }
        }
        for (a, ns) in woken {
            self.owner_to(cx, t, &ns, &[a]);
        }
    }

    /// His posts NS of T, held under his question about them (`Ticket::asked`) and just told or let go: each is handed
    /// to every agent it wakes (whom the tell chose, or policy's names and told when let go), whatever that agent's read
    /// mark, as a delivery of its own, `p{n}.told` (`Ticket::told_late`), which extends the take-back's
    /// `p{n}.taken_back` (`Struck`); its ordinary unread leaves it out (consult-3 N-1446 p576, proto-lead p576 on p569's
    /// (c2)). One path: no race with a `given` still to come, no double. Worked out from the lines (the tell of an asked
    /// post), so a replay hands the same; a record from before has no asked post.
    fn tell_asked(&mut self, t: u64, ns: &[u32]) {
        let Some(x) = self.tickets.get(&t) else { return };
        let mut late: Vec<ToldLate> = Vec::new();
        for p in x.posts.iter().filter(|p| ns.contains(&p.n)) {
            for a in p.wakes().filter(|a| *a != "owner" && *a != NOBODY && **a != p.by) {
                if !x.late_for(a, p.n) && !late.iter().any(|e| e.to == *a && e.n == p.n) {
                    late.push(ToldLate { n: p.n, to: a.clone(), given: false });
                }
            }
        }
        if !late.is_empty() {
            self.tickets.edit().get_mut(&t).unwrap().told_late.extend(late);
        }
    }

    /// The open handler of post N of T (a ticket of its own whose post it is), if any.
    fn post_handler(&self, t: u64, n: u32) -> Option<u64> {
        self.tickets.values().find(|u| u.is_open() && u.post_of.as_ref().is_some_and(|q| q.t == t && q.holds(n) && !q.done)).map(|u| u.t)
    }

    /// The tickets a take-back of post N of T would reach (`take_back_post`): T, then its handler and everything under
    /// it. coordd refuses the take-back while a delivery of any of them is on its way (handed, no `given` line yet;
    /// docket-core3, N-447 p97).
    pub fn take_back_reaches(&self, t: u64, n: u32) -> Vec<u64> {
        let mut out = vec![t];
        let mut todo: Vec<u64> = self.post_handler(t, n).into_iter().collect();
        while let Some(v) = todo.pop() {
            out.push(v);
            todo.extend(self.tickets.get(&v).into_iter().flat_map(|y| y.children.iter().copied()).filter(|c| self.tickets.contains_key(c)));
        }
        out
    }

    /// What hand-over D to AGENT leaves out (`Ticket::left_out`, of the posts it carries), worked out as it is handed:
    /// coordd keeps it with the hand-over, asks it for a take-back while it is on its way (`Ticket::hands_in`, its
    /// `backs`), and puts it on its `given` line, so his press between the two changes neither (proto-reader N-1446
    /// p621, proto-lead p631), an empty set too (consult-3 p661). None when no post of his up to D's mark was ever
    /// asked and none told to AGENT late is open up to it (`Ticket::may_leave_out`): nothing to leave out, and its line
    /// is as before (a post asked later was held at the hand-over, so it is past D's mark). With one told late open,
    /// what D shows decides: the retry of a hand-over shows his post from the delivery program's items, which
    /// `Ticket::unread` never does, so worked out at `Given` instead it would leave out the post it shows (proto-reader
    /// N-1479 p264); one told late after D is handed is added at its `given` (`left_out_given`).
    pub fn left_out(&self, agent: &str, d: &Delivery) -> Option<BTreeSet<u32>> {
        let x = self.tickets.get(&d.t).filter(|x| x.may_leave_out(agent, d.upto))?;
        Some(x.left_out(agent, d.upto, &d.shows()))
    }

    /// What the hand-over of T to AGENT up to UPTO, which showed SHOWN (`Delivery::shows`) and left out AT_HAND as it
    /// was handed (`left_out`), leaves out at its `given`, for that line: AT_HAND, and his posts told to AGENT late
    /// since, up to UPTO, that it does not show (`Ticket::late_open`). The era's line tells late one already on its way
    /// (machine.rs ERA_HIS_HANDED): the hand-over showing it hands it, and one not showing it does not, so its line
    /// says so even when AT_HAND is None; without, the line's `skipped_now`, of what `Ticket::unread` shows, would
    /// leave out the post it showed (proto-reader N-1479 p264). None as AT_HAND when none is open.
    pub fn left_out_given(&self, agent: &str, t: u64, upto: u32, shown: &[u32], at_hand: Option<BTreeSet<u32>>) -> Option<BTreeSet<u32>> {
        let Some(x) = self.tickets.get(&t).filter(|x| x.late_open(agent, upto).next().is_some()) else { return at_hand };
        let mut out = at_hand.unwrap_or_default();
        out.extend(x.late_open(agent, upto).filter(|n| !shown.contains(n)));
        Some(out)
    }

    /// BY takes back its post N of T (paper-research N-447 p16, 2; the form p30): only while nobody has been handed it,
    /// no `given` line on T reaching it and no step that carries it given (its handler's `route`). Held by a handler
    /// that has handed nothing yet: the handler's ticket ends with it, cancelled. A thread's first post is its words:
    /// the thread is cancelled instead. The post keeps its number and is invisible from now on: given to nobody, due to
    /// nobody, closing no ask, not drawn.
    fn take_back_post(&mut self, cx: &mut Cx, t: u64, n: u32, by: &str) -> Result<String, String> {
        self.take_back_checked(cx, t, n, by, false)?;
        Ok(format!("Post {n} of {} taken back.", self.id(t)))
    }

    /// Whether post N of T was had by an agent, as a take-back reads it (0.2 B8, N-1396): Some with the agents a `given`
    /// row of T handed it (`Ticket::hands`: up to it or past and not passed unhanded, or his post told late by its own
    /// row), its author not among them, in the order given; Some with none when only its handler's tickets were given
    /// (read for its handler, which is cancelled and tells its agent). None: nobody had it, and it is gone; or it is
    /// taken back or dropped already, which `take_back_post` refuses.
    fn had_post(&self, t: u64, n: u32) -> Option<Vec<String>> {
        let x = self.tickets.get(&t)?;
        let by = x.posts.iter().find(|p| p.n == n && !p.taken_back && !p.dropped).map(|p| p.by.as_str())?;
        let mut to: Vec<String> = Vec::new();
        // only who can read it (`readers`, as coordd tells it): not a program agent, nor the watcher coordd answers itself
        for g in x.given.iter().filter(|g| Ticket::hands(g, n) && g.to != by && g.to != WATCHER && self.readers.as_ref().is_none_or(|r| r.contains(&g.to))) {
            if !to.contains(&g.to) {
                to.push(g.to.clone());
            }
        }
        let read = self.take_back_reaches(t, n).into_iter().skip(1).any(|u| self.tickets.get(&u).is_some_and(|y| !y.given.is_empty()));
        (!to.is_empty() || read).then_some(to)
    }

    /// BY's post N of T, taken back after an agent had it (`TAKEN_BACK`; 0.2 B8, N-1396): what `take_back_post` does,
    /// though it was given, and struck rather than gone. Its handler is cancelled as before: a step it handed is
    /// withdrawn from its agent, who is told so. Each of TO is told once (`What::TakenBack`), at once, working or idle.
    /// Taken back already (in a hand-over coordd had sent, which its agent then took, `Input::StruckFor`): TO are told.
    fn strike_post(&mut self, cx: &mut Cx, t: u64, n: u32, by: &str, to: Vec<String>) -> Result<String, String> {
        let id = self.id(t);
        let x = self.tickets.get(&t).ok_or_else(|| format!("There is no ticket {id}."))?;
        let p = x.posts.iter().find(|p| p.n == n).ok_or_else(|| format!("{id} has no post {n}."))?;
        if p.by != by {
            return Err(format!("Post {n} of {id} is {}'s: only its author takes it back.", p.by));
        }
        if !p.taken_back {
            self.take_back_checked(cx, t, n, by, true)?;
        }
        let at = cx.now.clone();
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let i = match x.struck.iter().position(|s| s.n == n) {
            Some(i) => i,
            None => {
                x.struck.push(Struck { n, at, by: by.to_string(), to: Vec::new(), told: Vec::new() });
                x.struck.len() - 1
            }
        };
        for a in &to {
            if !x.struck[i].to.contains(a) {
                x.struck[i].to.push(a.clone());
            }
        }
        for a in &to {
            cx.effect(Effect::Deliver { to: a.clone() });
        }
        cx.effect(Effect::Changed { t });
        Ok(if to.is_empty() { format!("Post {n} of {id} taken back, struck.") } else { format!("Post {n} of {id} taken back, struck; {} told.", to.join(", ")) })
    }

    /// `take_back_post`'s checks and act; HAD (`strike_post`): an agent had it, so its `given` rows and its handler's
    /// refuse nothing.
    fn take_back_checked(&mut self, cx: &mut Cx, t: u64, n: u32, by: &str, had: bool) -> Result<(), String> {
        let id = self.id(t);
        let x = self.tickets.get(&t).ok_or_else(|| format!("There is no ticket {id}."))?;
        let p = x.posts.iter().find(|p| p.n == n).ok_or_else(|| format!("{id} has no post {n}."))?;
        if p.by != by {
            return Err(format!("Post {n} of {id} is {}'s: only its author takes it back.", p.by));
        }
        if p.taken_back || p.dropped {
            return Err(format!("Post {n} of {id} is {} already.", if p.taken_back { "taken back" } else { "dropped" }));
        }
        if n == 1 {
            return Err(format!("Post 1 of {id} is its words: cancel the thread instead."));
        }
        if p.halted {
            return Err(format!("Post {n} of {id} is halted for the owner's approval: it is dropped there."));
        }
        // handed by a `given` row (`Ticket::hands`): not by one whose mark passed it unhanded, held under his question
        if let Some(g) = x.given.iter().find(|g| Ticket::hands(g, n)).filter(|_| !had) {
            return Err(format!("Post {n} of {id} was handed to {} already: it cannot be taken back.", g.to));
        }
        // its handler: a ticket of its own whose post is this one, and everything under it
        if let Some(u) = self.post_handler(t, n) {
            let mut todo = vec![u];
            while let Some(v) = todo.pop().filter(|_| !had) {
                let y = &self.tickets[&v];
                if let Some(g) = y.given.first() {
                    return Err(format!("Post {n} of {id} is being read by {} for its handler: it cannot be taken back.", g.to));
                }
                todo.extend(y.children.iter().copied().filter(|c| self.tickets.contains_key(c)));
            }
            let of = self.tickets.edit().get_mut(&u).unwrap().post_of.as_mut().unwrap();
            if of.with.is_empty() {
                of.done = true;
                let r = Raised { exc: "Cancelled".into(), value: json!({"by": by, "why": "its post was taken back"}) };
                let mut work = Vec::new();
                self.cancel(cx, u, by, "its post was taken back", &r, &mut work);
                self.drive(cx, work);
            } else if of.n == n {
                // one post of his burst (E1b): it leaves the burst, which goes on with the rest
                of.n = of.with.remove(0);
            } else {
                of.with.retain(|m| *m != n);
            }
        }
        let before = self.tickets[&t].his_shown();
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.asked.remove(&n);
        let p = x.posts.iter_mut().find(|p| p.n == n).unwrap();
        p.held = None;
        p.taken_back = true;
        // what was held back behind it is due now
        self.posts_reached(cx, t, &before);
        for a in self.participants(t) {
            cx.effect(Effect::Deliver { to: a });
        }
        cx.effect(Effect::Changed { t });
        Ok(())
    }

    /// `drop p` in T: the held post is withdrawn; no agent sees it.
    fn drop_post(&mut self, cx: &mut Cx, t: u64) -> Result<(), String> {
        let u = self.holder(t)?;
        let of = self.tickets[&u].post_of.clone().unwrap();
        let on = of.t;
        self.tickets.edit().get_mut(&u).unwrap().post_of.as_mut().unwrap().done = true;
        let before = self.tickets[&on].his_shown();
        // every post it holds still (his burst's, E1b)
        for n in of.posts() {
            let x = self.tickets.edit().get_mut(&on).unwrap();
            let p = x.posts.iter_mut().find(|p| p.n == n).unwrap();
            if !of.with.is_empty() && p.held.is_none() {
                continue;
            }
            p.held = None;
            p.dropped = true;
            x.asked.remove(&n);
            cx.line(Line::Dropped { at: cx.now.clone(), t: on, n });
        }
        // what was held back behind it is due now; one his question held was seen by nobody, and nothing is raised for
        // it (proto-lead N-1446 p569, (c2))
        self.posts_reached(cx, on, &before);
        for a in self.participants(on) {
            cx.effect(Effect::Deliver { to: a });
        }
        cx.effect(Effect::Changed { t: on });
        Ok(())
    }

    /// The handler running in U ended having neither told nor dropped its post: it goes as policy said (the post's
    /// names and told), as if there were no handler, so a program's mistake never loses the owner's words.
    fn release(&mut self, cx: &mut Cx, u: u64) {
        let Some(of) = self.tickets[&u].post_of.clone().filter(|p| !p.done) else { return };
        let on = of.t;
        self.tickets.edit().get_mut(&u).unwrap().post_of.as_mut().unwrap().done = true;
        let before = self.tickets[&on].his_shown();
        // every post it holds still (his burst's, E1b; a Split may have told some)
        let mut wake: Vec<String> = Vec::new();
        let mut asked: Vec<u32> = Vec::new();
        for n in of.posts() {
            let x = self.tickets.edit().get_mut(&on).unwrap();
            let p = x.posts.iter_mut().find(|p| p.n == n).unwrap();
            if !of.with.is_empty() && p.held.is_none() {
                continue;
            }
            p.held = None;
            wake.extend(p.names.iter().chain(&p.told).cloned());
            if x.asked.remove(&n) {
                asked.push(n);
            }
            cx.line(Line::Released { at: cx.now.clone(), t: on, n });
        }
        self.tell_asked(on, &asked);
        self.posts_reached(cx, on, &before);
        for a in self.participants(on).into_iter().chain(wake) {
            cx.effect(Effect::Deliver { to: a });
        }
        cx.effect(Effect::Changed { t: on });
    }

    /// A text in an inline call's places: Some(Some(words)) for the machine's own words (the owner's, a program's),
    /// put in as they are; Some(None) for one held at a place (an agent's turn), which only the surfaces read: it stays
    /// a reference, marked in the text (`{{@K}}`, What::Step.refs[K]) and filled where the delivery is rendered.
    /// None: not a text.
    fn inline_words(&self, v: &Value) -> Option<Option<String>> {
        let t = Text::of(v).filter(|t| !matches!(t, Text::New(_)))?;
        Some(match self.text(&t) {
            Some(Text::New(s)) => Some(s),
            _ => None,
        })
    }

    /// Whether AGENT has an ask unanswered on the step it holds of T (one that waits in its turn, or went later).
    fn asks_on(&self, agent: &str, t: u64) -> bool {
        self.tickets.values().any(|y| y.is_open() && y.ask.as_ref().is_some_and(|a| a.by == agent && a.held == t && !a.replied))
            || self.held_asks().iter().any(|(_, q)| q.ask.by == agent && q.ask.held == t)
    }

    /// BY's waiting ask TEXT (CONTEXT) on HELD, the ticket whose step it holds: run by the nearest `when ask` above
    /// HELD; WAITING, its command waits for the answer. MINDED: looked at first by the `during` around the step, if
    /// one takes it (ask_minded.rs). What BY is told, or why it cannot ask so.
    #[allow(clippy::too_many_arguments)]
    fn ask_waiting(&mut self, cx: &mut Cx, by: &String, text: &String, context: Option<String>, held: u64, waiting: bool, minded: bool) -> Result<String, String> {
        let mut u = held;
        let q = json!({"by": by, "text": text, "context": context});
        let h = loop {
            let x = &self.tickets[&u];
            if let Some(h) = x.handlers.iter().find(|h| h.ev == "ask") {
                let mut env: Env = x.form.as_object().map(|o| o.iter().map(|(a, b)| (a.clone(), b.clone())).collect()).unwrap_or_default();
                env.insert("this".into(), json!(self.id(u)));
                env.insert(h.x.clone(), q.clone());
                if h.cond.as_ref().is_none_or(|c| eval(c, &env, &x.ctx) == json!(true)) {
                    break Some((h.clone(), env));
                }
            }
            match x.parent.or(x.under) {
                Some(p) => u = p,
                None => break None,
            }
        };
        let Some((h, env)) = h else { return Err(format!("No ticket {by} works on takes asks: ask with `coord ask SUBJECT`, which does not wait.")) };
        if minded {
            let q = HeldAsk { ask: AskOf { by: by.clone(), text: text.clone(), words: json!(text), waiting, replied: false, held, own: false }, context: context.clone(), head: false, stopped: false };
            if let Ok(said) = self.ask_minded(cx, q) {
                return Ok(said);
            }
        }
        // on replay, only where the log has the ask's ticket opened under the ticket that took it
        if !logged_run(cx, ASK, u) {
            return Err(format!("No ticket {by} works on takes asks: ask with `coord ask SUBJECT`, which does not wait."));
        }
        let c = self.next.max(self.tickets.range(..HIDDEN).next_back().map_or(0, |(m, _)| m + 1));
        let (body, program) = self.handler_program(cx, c, u, &h, None);
        let k = Kind { pattern: body.clone(), result: Type::Any, ..loose_kind(ASK) };
        let mut x = self.new_ticket(c, ASK, None, None, q.clone(), self.child_ctx(u), String::new(), &k, &cx.now);
        x.under = Some(u);
        x.raised_by = Some(by.clone());
        x.ask = Some(AskOf { by: by.clone(), text: text.clone(), words: json!(text), waiting, replied: false, held, own: false });
        self.tickets.edit().insert(c, x);
        self.next = c + 1;
        cx.line(Line::Opened { at: cx.now.clone(), t: c, kind: ASK.into(), parent: u, program, calls: BTreeMap::new() });
        cx.effect(Effect::Changed { t: c });
        self.drive(cx, vec![Work::Run(c, body, env)]);
        Ok(format!("Asked on {} ({}).", self.id(u), self.id(c)))
    }

    /// The `mail_hold` line (MAIL_HOLD, line 327) applied: on, the hold begins (`MailHold`); off, the hold ends, and
    /// what it kept is decided again (`hold_lifted`). A second `on`, or an `off` with no hold, changes nothing
    /// and says so.
    fn mail_hold_switched(&mut self, cx: &mut Cx, at: &At, value: &Value) -> (bool, Result<String, String>) {
        if value.get("on").and_then(Value::as_bool).unwrap_or(false) {
            if let Some(h) = &self.mail_hold {
                return (true, Ok(format!("Agents' mail is held already, since {}.", h.at)));
            }
            self.mail_hold = Some(MailHold { at: at.clone() });
            return (true, Ok(String::from("Agents' mail is held: until Release, only the owner's posts pass.")));
        }
        match self.mail_hold.take() {
            Some(h) => {
                // his posts a turn his Esc cut under it left waiting are his posts again, decided as such
                self.stopped_his.clear();
                let mut said = format!("Agents' mail is let go (held since {}).", h.at);
                match self.hold_lifted(cx, &h.why(), &h.ended(at)) {
                    0 => {}
                    1 => said.push_str(" The one thing it kept is decided again."),
                    n => said.push_str(&format!(" The {n} things it kept are decided again.")),
                }
                (true, Ok(said))
            }
            None => (true, Ok(String::from("Agents' mail is not held."))),
        }
    }

    /// The `brain_switch` line (BRAIN_SWITCH) read: his switch of the agent's brain is as it says, since the line's time.
    /// A line that says what holds already changes nothing and says so (coordd writes none: `Core::ticket_brain`).
    fn brain_switch_read(&mut self, at: &At, value: &Value) -> (bool, Result<String, String>) {
        let (Some(agent), Some(on)) = (value.get("agent").and_then(Value::as_str), value.get("on").and_then(Value::as_bool)) else {
            return (false, Err(String::from("An agent's brain is switched on or off, naming the agent.")));
        };
        let word = if on { "on" } else { "off" };
        if self.brain_is_on(agent) == on {
            return (true, Ok(format!("{agent}'s brain is {word} already.")));
        }
        self.brain_switched.insert(agent.to_string(), (on, at.clone()));
        (true, Ok(format!("{agent}'s brain is {word}.")))
    }

    /// Whether AGENT's brain is on: his last switch of it (BRAIN_SWITCH) said on, or he has never switched it.
    pub fn brain_is_on(&self, agent: &str) -> bool {
        self.brain_switched.get(agent).is_none_or(|(on, _)| *on)
    }

    /// The post a `signed_off` line's VALUE (SIGNED_OFF) names, by its ticket and number, and his word: one of
    /// [`WORTHS`], or `none`; by him alone. Else why it is refused, live and on replay.
    fn sign_off_of(value: &Value) -> Result<(u64, u32, String), String> {
        let t = value.get("t").and_then(Value::as_u64);
        let n = value.get("n").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok());
        let worth = value.get("worth").and_then(Value::as_str).filter(|w| *w == "none" || WORTHS.contains(w));
        let (Some(t), Some(n), Some(worth)) = (t, n, worth) else {
            return Err(String::from("A sign-off names his post by its ticket and number, and his word: worth_it, fine or not_worth_it, or none to take it back."));
        };
        if value.get("by").and_then(Value::as_str) != Some("owner") {
            return Err(String::from("Only the owner signs off his asks."));
        }
        Ok((t, n, worth.to_string()))
    }

    /// The `signed_off` line (SIGNED_OFF) read: his word on the post is as it says, since the line's time; `none` takes
    /// it back. A line that says what holds already changes nothing and says so (coordd writes none:
    /// `Core::ticket_sign_off`).
    fn signed_off_read(&mut self, at: &At, value: &Value) -> (bool, Result<String, String>) {
        let (t, n, worth) = match Machine::sign_off_of(value) {
            Ok(x) => x,
            Err(e) => return (false, Err(e)),
        };
        let id = format!("{}.p{n}", self.id(t));
        match (self.signed_off(t, n), worth.as_str()) {
            (None, "none") => (true, Ok(format!("{id} is not signed off."))),
            (Some(w), _) if w == worth => (true, Ok(format!("{id} is signed off {w} already."))),
            (_, "none") => {
                self.signed_off.remove(&(t, n));
                (true, Ok(format!("{id}'s sign-off is taken back.")))
            }
            _ => {
                let said = format!("{id} is signed off {worth}.");
                self.signed_off.insert((t, n), (worth, at.clone()));
                (true, Ok(said))
            }
        }
    }

    /// His word of worth on post N of T ([`WORTHS`]), when he has signed it off (SIGNED_OFF) and not taken it back.
    pub fn signed_off(&self, t: u64, n: u32) -> Option<&str> {
        self.signed_off.get(&(t, n)).map(|(w, _)| w.as_str())
    }

    /// Why his switch keeps a ticket of KIND whose form's `agent` is AGENT from opening: KIND is the brain's ([`BRAIN`])
    /// and his last switch of that agent is off, "w2's brain is switched off, since 10:20 UTC": the line's time in UTC
    /// as the record's times are, named so, as the mail hold's words are (`MailHold::why`), so it does not read as
    /// his card's own clock (proto-reader N-1446 p1031). None: it opens. The one check of every way a new pass would
    /// open (proto-lead N-1446 p985): a binding (`input`'s event, which does not fire), `coord open` and a rekind
    /// (`complete`, refused), a step's `open` (`Prog::Open`, stuck). A pass at work is none of these, and runs to its
    /// end (p987).
    fn brain_off(&self, kind: &str, agent: Option<&Value>) -> Option<String> {
        let agent = agent.and_then(Value::as_str).filter(|_| kind == BRAIN)?;
        self.brain_switched.get(agent).filter(|(on, _)| !*on).map(|(_, at)| format!("{agent}'s brain is switched off, since {} UTC", hm(at)))
    }

    /// Whether a binding of today's defs opens a brain ([`BRAIN`]) for AGENT: one whose `where` holds for an event
    /// naming AGENT as `agent` and whose brain's form names AGENT, as research's `on owner_to(m) where m.agent ==
    /// "paper-research" open brain(m.agent, …)` does. A clock's or a decision's binding names no agent, and is none.
    /// Its limit (proto-reader N-1446 p984): the `where` is read with only `agent` in the event, so a binding whose
    /// `where` reads another field (`m.posts != []`) shows no switch on the card though it opens passes; whoever writes
    /// such a binding extends this and its test.
    pub fn brain_bound(&self, agent: &str) -> bool {
        let value = json!({"agent": agent});
        self.defs.bindings.iter().filter(|b| b.kind == BRAIN && matches!(b.every, Every::Each | Every::Count(_))).any(|b| {
            let Some(k) = self.defs.kinds.get(&b.kind) else { return false };
            let env: Env = [(b.x.clone(), value.clone())].into_iter().collect();
            let holds = b.conds.iter().all(|(l, r)| {
                let l = eval(l, &env, &json!({}));
                match r {
                    None => l == json!(true),
                    Some((eq, r)) => (l == eval(r, &env, &json!({}))) == *eq,
                }
            });
            let named = k.form.iter().position(|(n, _)| n == "agent").and_then(|i| b.args.get(i)).map(|a| eval(a, &env, &json!({})));
            holds && named.as_ref().and_then(Value::as_str) == Some(agent)
        })
    }

    /// AGENT's last brain pass ([`BRAIN`] whose form names AGENT): its ticket, the one opened last. A pass is a seen
    /// ticket, so the hidden ones and the desks are not read.
    pub fn last_brain_pass(&self, agent: &str) -> Option<&Ticket> {
        self.tickets.range(..HIDDEN).rev().map(|(_, x)| x).find(|x| x.kind == BRAIN && x.form.get("agent").and_then(Value::as_str) == Some(agent))
    }

    /// Whether the owner's hold on agents' mail keeps delivery D (line 327), and since when: while the hold is on, every
    /// delivery that carries no post of his, whenever written, so a step, its reminder, a clock's firing, words
    /// given, a close or a withdrawal is kept as an agent's post is. His take-back of a post an agent had (0.2 B8) is
    /// not kept: the agent is told at once, or it goes on acting on what he took back (consult-3, N-818 post 81); an
    /// agent's take-back is kept with the rest. What the delivery program does with it is its own: delivery.kind keeps
    /// it with the hold's words (`MailHold::why`; the events give it apart, `delivery_value`). None: not kept.
    pub fn mail_held(&self, d: &Delivery) -> Option<&At> {
        let h = self.mail_hold.as_ref()?;
        let posts = match &d.what {
            What::TakenBack { by, .. } if by == "owner" => return None,
            What::Posts { unread } | What::Closed { unread, .. } => unread.as_slice(),
            _ => &[],
        };
        (!posts.iter().any(|p| !h.keeps(&p.by))).then_some(&h.at)
    }

    /// Since when the owner is writing in ticket T's box (`syntax::WRITING`, 0.3), while he is: E1b's hold stays open
    /// meanwhile, under its ceiling (proto-engine N-1446 p33, proto-lead p61). None: he is not.
    pub fn writing_since(&self, t: u64) -> Option<&At> {
        self.writing.get(&t)
    }

    /// Whether post N of ticket T, his to AGENT, waits with what his hold keeps because a turn his own Esc cut while it
    /// was on made it due again (`stopped_his`): until his next post to AGENT, which it goes with (`waiting_held`), or
    /// his Release (paper-research N-412.p2202, 2, and p2205: "keep them interrupted so i can be the one to send them
    /// messages").
    pub fn stopped_held(&self, agent: &str, t: u64, n: u32) -> bool {
        self.mail_hold.is_some() && self.stopped_his.get(agent).is_some_and(|s| s.contains(&(t, n)))
    }

    /// Whether the owner's hold on agents' mail keeps post N of ticket T (line 327), and since when: every post but his
    /// (`MailHold::keeps`). The flag on each post a delivery shows (`held ·`). None: not kept, or no such post.
    pub fn mail_held_post(&self, t: u64, n: u32) -> Option<&At> {
        let h = self.mail_hold.as_ref()?;
        let p = self.tickets.get(&t)?.posts.iter().find(|p| p.n == n)?;
        h.keeps(&p.by).then_some(&h.at)
    }

    /// The ticket whose step AGENT holds: one it was given, else one awaiting it.
    fn held_by(&self, agent: &str) -> Option<u64> {
        let holds = |x: &&Ticket, given: bool| matches!(&x.status, Status::Awaits { agent: a, taken, .. } if a == agent && (*taken || !given));
        self.tickets.values().find(|x| holds(x, true)).or_else(|| self.tickets.values().find(|x| holds(x, false))).map(|x| x.t)
    }

    /// The ticket of AGENT's ask that waits in its turn, if any.
    fn waiting_ask(&self, agent: &str) -> Option<u64> {
        self.tickets.values().find(|x| x.ask.as_ref().is_some_and(|a| a.by == agent && a.waiting && !a.replied)).map(|x| x.t)
    }

    /// The ask T stands for (T, or the nearest above it running an ask's handler) answered with V: in the asker's turn
    /// while it waits (Effect::Answered), else given to it as words.
    fn reply_ask(&mut self, cx: &mut Cx, t: u64, v: &Value) -> Result<(), String> {
        let mut u = t;
        while self.tickets[&u].ask.is_none() {
            u = self.tickets[&u].parent.ok_or("`reply` is used where no ask is being answered")?;
        }
        let x = self.tickets.edit().get_mut(&u).unwrap();
        let a = x.ask.as_mut().unwrap();
        if a.replied {
            return Err(String::from("the ask is answered already"));
        }
        // Answered(text) is its text; anything else as it reads
        let text = match v {
            Value::String(s) => s.clone(),
            _ => match (v.get("head").and_then(Value::as_str), v.get("text").and_then(Value::as_str), v.get("why").and_then(Value::as_str)) {
                (_, Some(s), _) => s.to_string(),
                (Some(h), None, Some(w)) => format!("{h}: {w}"),
                _ => crate::print::value(v),
            },
        };
        a.replied = true;
        let (to, later) = (a.by.clone(), !a.waiting);
        a.waiting = false;
        cx.line(Line::AskAnswered { at: cx.now.clone(), t: u, to: to.clone(), later });
        // the step it held is the asker's again, handed with the answer (its one line, `again`), its clock afresh
        let held = x.ask.as_ref().map(|a| a.held);
        if let Some(h) = held.filter(|h| *h != u) {
            let now = cx.now.clone();
            if let Some(Status::Awaits { agent, since, reminded, taken: true, .. }) = self.tickets.edit().get_mut(&h).map(|y| &mut y.status) {
                if *agent == to {
                    *since = now;
                    if later {
                        *reminded = Some(false);
                    }
                    cx.effect(Effect::Changed { t: h });
                }
                // given again for the answer, not for an answer unread
                self.tickets.edit().get_mut(&h).unwrap().remind_why = None;
            }
        }
        let x = self.tickets.edit().get_mut(&u).unwrap();
        if later {
            let ask = x.ask.as_ref().map(|a| a.text.clone()).unwrap_or_default();
            let said = if ask.is_empty() { format!("The answer to your question: {text}") } else { format!("The answer to your question \"{ask}\": {text}") };
            x.gives.push(Gift { to: to.clone(), text: Text::New(said), given: false });
            let n = x.gives.len() as u32;
            cx.line(Line::Gave { at: cx.now.clone(), t: u, to: to.clone(), n });
        } else {
            cx.effect(Effect::Answered { agent: to.clone(), text });
        }
        cx.effect(Effect::Deliver { to });
        cx.effect(Effect::Changed { t: u });
        Ok(())
    }

    /// Whether AGENT, about to be given a step of T, waits through asks on one T answers: T is (under) the handler of
    /// an ask by A, and AGENT is A, or its own waiting ask is answered (through more asks) by tickets awaiting A.
    fn cycle(&self, t: u64, agent: &str) -> Option<String> {
        let mut askers = Vec::new();
        let mut u = Some(t);
        while let Some(v) = u {
            let x = &self.tickets[&v];
            if let Some(a) = x.ask.as_ref().filter(|a| !a.replied) {
                askers.push(a.by.clone());
            }
            u = x.parent.or(x.under);
        }
        // whom each agent waits on: the agents the tickets below its waiting ask await
        let waits_on = |a: &str| -> Vec<String> {
            let Some(root) = self.waiting_ask(a) else { return Vec::new() };
            let mut out = Vec::new();
            let mut todo = vec![root];
            while let Some(v) = todo.pop() {
                let x = &self.tickets[&v];
                if let Status::Awaits { agent, .. } = &x.status {
                    out.push(agent.clone());
                }
                todo.extend(self.tickets.values().filter(|y| y.parent == Some(v) || y.under == Some(v)).map(|y| y.t));
            }
            out
        };
        for asker in askers {
            let mut seen = vec![agent.to_string()];
            let mut path = vec![agent.to_string()];
            let mut i = 0;
            while i < seen.len() {
                let a = seen[i].clone();
                if a == asker {
                    return Some(if a == agent {
                        format!("{agent} is the one asking ({} waits on its answer)", asker)
                    } else {
                        format!("{agent} waits on {} through asks, and {asker} waits on this", path.join(" -> "))
                    });
                }
                for b in waits_on(&a) {
                    if !seen.contains(&b) {
                        path.push(b.clone());
                        seen.push(b);
                    }
                }
                i += 1;
            }
        }
        None
    }

    fn exec_line(&mut self, line: &Line, cx: &mut Cx) -> (bool, Result<String, String>) {
        let no = |e: String| (false, Err(e));
        match line {
            Line::Open { at, t, kind, under, opener, form, text, names, told, program, closed, calls, asked_on, .. } => {
                if self.tickets.contains_key(t) {
                    return no(format!("ticket {t} exists already"));
                }
                let (k, bad, loose) = match self.recorded_program_as(kind, program, calls, cx) {
                    Ok(k) => k,
                    Err(e) => return no(e),
                };
                // on replay, a kind this release does not load (the way back, proto-lead N-1446 p398; proto-reader): one
                // today's kinds lack whose recorded program does not even parse here (0.3's aside, written in a later
                // release's words). It is refused for that alone, whatever its program says, and stays not loaded. A
                // kind gone from today's whose program parses (the `words` of research's record; one that no longer
                // checks against today's calls, tests/changed_calls.rs) opens as it always did
                if cx.recorded.is_some() && bad.is_some() && !self.defs.kinds.contains_key(kind) && syntax::parse_kind(kind, program, 1, &self.defs.named).is_err() {
                    self.not_loaded.insert(*t);
                    return no(format!("{kind} is a kind this release does not load."));
                }
                if let (None, Err(e)) = (&bad, Type::Record(k.form.clone()).conforms(form)) {
                    let want = k.form.iter().map(|(n, t)| format!("{n}: {t}")).collect::<Vec<_>>().join(", ");
                    return no(format!("The form of a {kind} does not fit: {e}. A {kind} takes {want}."));
                }
                if let Some(u) = under {
                    if !self.tickets.contains_key(u) {
                        return no(format!("There is no ticket {u} to open it under."));
                    }
                }
                // a kind that closes at once (a note) has no step to carry it: it must reach someone now; so must an
                // agents' thread, which ends as it opens (Sent, design part 9b)
                let at_once = closes_at_once(&k, form);
                if at_once && names.is_empty() && told.is_empty() && closed.is_none() {
                    let here = match under.filter(|u| self.tickets[u].is_open()) {
                        Some(u) => format!(" Or post on {} instead, where its next step carries it.", self.id(u)),
                        None => String::new(),
                    };
                    return no(format!("A {kind} closes at once, so it reaches only those it names, and it names nobody. Name who should read it with @.{here}"));
                }
                let ctx = under.map_or(json!({}), |u| self.child_ctx(u));
                let mut x = self.new_ticket(*t, kind, None, Some(opener.clone()), form.clone(), ctx, program.clone(), &k, at);
                x.calls = calls.clone();
                x.under = *under;
                if kind == "question" {
                    x.about = self.given_since(opener).iter().filter_map(|d| parse_delivery(d).map(|(t, _)| t)).collect::<BTreeSet<_>>().into_iter().collect();
                }
                // the step's ask: its closing notice is the answer, and the step waits on it (asks_on)
                if let Some(h) = asked_on.filter(|h| self.tickets.get(h).is_some_and(|y| matches!(&y.status, Status::Awaits { agent, .. } if agent == opener))) {
                    let words = form.get("ask").cloned().unwrap_or(Value::Null);
                    x.ask = Some(AskOf { by: opener.clone(), text: words.as_str().unwrap_or_default().to_string(), words, waiting: false, replied: false, held: h, own: true });
                }
                if let Some(text) = text {
                    x.posts.push(Post { n: 1, at: at.clone(), by: opener.clone(), text: text.clone(), head: None, step: None, names: names.clone(), told: told.clone() , auto: false, moved: None, from: None, held: None, halted: false, dropped: false, routed: false, asks: Vec::new(), asks_open: false, asks_closed: None, answers: None, inform: Vec::new(), typed: None, cleaned_from: None, split_from: None, on: Vec::new(), also: Vec::new(), would_close: None, taken_back: false, joins: None, passes: None, comments: Vec::new(), into_turn: Vec::new(), kind: None, answering: Vec::new() });
                    x.mu.insert(opener.clone(), 1);
                }
                if at_once {
                    // the opener wrote all there is: no closing notice of its own note
                    x.closed_given.insert(format!("{opener} c"));
                }
                for a in names.iter().chain(told) {
                    cx.effect(Effect::Deliver { to: a.clone() });
                }
                if let Some(c) = closed {
                    // imported closed: as it was, nobody told, its program not run
                    if let (None, Err(e)) = (&bad, k.result.conforms(&c.head)) {
                        return no(format!("The result of a {kind} does not fit: {e}."));
                    }
                    let n = x.last() + 1;
                    x.posts.push(Post { n, at: c.at.clone(), by: c.by.clone(), text: c.text.clone().unwrap_or(Text::New(String::new())), head: Some(c.head.clone()), step: None, names: Vec::new(), told: Vec::new() , auto: false, moved: None, from: None, held: None, halted: false, dropped: false, routed: false, asks: Vec::new(), asks_open: false, asks_closed: None, answers: None, inform: Vec::new(), typed: None, cleaned_from: None, split_from: None, on: Vec::new(), also: Vec::new(), would_close: None, taken_back: false, joins: None, passes: None, comments: Vec::new(), into_turn: Vec::new(), kind: None, answering: Vec::new() });
                    x.status = Status::Done(c.head.clone());
                    x.closed_at = Some(c.at.clone());
                    x.closed_given.insert(format!("{opener} c"));
                    x.read_own(c.by.clone(), n, self.era, &mut self.passed_his);
                    self.tickets.edit().insert(*t, x);
                    self.next = self.next.max(*t + 1);
                    self.hand_in(cx, *t, opener, form);
                    cx.effect(Effect::Changed { t: *t });
                    return (true, Ok(format!("Imported {}, closed.", self.id(*t))));
                }
                self.tickets.edit().insert(*t, x);
                self.next = self.next.max(*t + 1);
                self.hand_in(cx, *t, opener, form);
                if let Some(x) = self.tickets.edit().get_mut(t) {
                    x.loose = loose;
                }
                if let Some(why) = &bad {
                    self.unchecked(*t, why);
                }
                if let Some(u) = under {
                    self.tickets.edit().get_mut(u).unwrap().children.push(*t);
                }
                // a subthread opened at a post (`coord msg --under`, N-447): tied there from its open
                self.tie_at_open(cx, *t, form.get("tied_at"));
                let env: Env = form.as_object().map(|o| o.iter().map(|(a, b)| (a.clone(), b.clone())).collect()).unwrap_or_default();
                // an agent's ticket opened under a slow one halts before its program runs (the owner's does not)
                if opener != "owner" && under.is_some() && !at_once && self.slow(*t) {
                    self.halt(cx, *t, Halting::Open { pattern: k.pattern });
                    cx.effect(Effect::Changed { t: *t });
                    return (true, Ok(format!("Opened {}; it waits for the owner's approval (slow mode).", self.id(*t))));
                }
                self.drive(cx, vec![Work::Run(*t, k.pattern, env)]);
                // an agents' thread ends as it opens (Between, design part 9b): its opener wrote all there is, as a
                // note's did; no closing notice of its own message
                if let Some(x) = self.tickets.edit().get_mut(t).filter(|x| x.between()) {
                    x.closed_given.insert(format!("{opener} c"));
                    // its message is its first post: read for what it asks the owner, as a later post is (3h)
                    if text.is_some() {
                        if let Some(w) = self.agent_landed(cx, *t, 1) {
                            self.drive(cx, vec![w]);
                        }
                    }
                }
                cx.effect(Effect::Changed { t: *t });
                (true, Ok(format!("Opened {}.", self.id(*t))))
            }
            Line::Template { t, call, scope, text, .. } => {
                let said = self.template_edit(cx, *t, call, scope == "tree", text);
                (true, Ok(said))
            }
            Line::Reload { at, by, why, commit, kinds, calls } => {
                let mark = crate::view::LiveEdit { at: at.clone(), by: by.clone(), why: why.clone(), commit: commit.clone() };
                self.kinds_edited.extend(kinds.iter().map(|k| (k.clone(), mark.clone())));
                self.calls_edited.extend(calls.iter().map(|c| (c.clone(), mark.clone())));
                let names = |w: &str, v: &Vec<String>| (!v.is_empty()).then(|| format!("{w} {}", v.join(", ")));
                let changed: Vec<String> = [names("kinds", kinds), names("calls", calls)].into_iter().flatten().collect();
                (true, Ok(if changed.is_empty() { "The kinds and calls are read again; none changed.".into() } else { format!("The kinds and calls are read again: {} changed or new.", changed.join("; ")) }))
            }
            Line::Role { agent, role, also, .. } => {
                self.roles.insert(agent.clone(), role.clone());
                if also.is_empty() {
                    self.also.remove(agent);
                    (true, Ok(format!("{agent} is {role}.")))
                } else {
                    self.also.insert(agent.clone(), also.clone());
                    (true, Ok(format!("{agent} is {role}, and {}.", also.join(", "))))
                }
            }
            // an act: its ticket's handler takes it if it has one; else it is only recorded
            Line::Act { t, agent, is, text, more, .. } => {
                let value = crate::runner::act_value(agent, is, text, t.map(|t| self.id(t)), more);
                match t.filter(|_| self.event_ticket("act", &value).is_some()) {
                    Some(t) => match self.event_on(cx, t, "act", &value) {
                        Ok(r) => (true, Ok(r)),
                        Err(e) => no(e),
                    },
                    None => (true, Ok(String::from("Recorded."))),
                }
            }
            // a delivery event (0.2): run in the agent's delivery ticket, or queued behind its open run (delivery.rs)
            Line::Event { name, value, fired, on: None, .. } if syntax::delivery_event(name) && fired.is_empty() => {
                let (name, value) = (name.clone(), value.clone());
                self.delivery_event(cx, &name, &value)
            }
            Line::Event { at, name, value, fired, on: None, .. } if name == syntax::DECIDED && fired.is_empty() => {
                let (at, value) = (at.clone(), value.clone());
                self.exec_decided(cx, &at, &value)
            }
            // the record's era (ERA): the rules of it apply from this line on
            Line::Event { at, name, value, .. } if name == ERA => {
                let era = value.get("era").and_then(Value::as_u64).and_then(|e| u32::try_from(e).ok()).unwrap_or(0);
                let was = self.era;
                self.era = self.era.max(era);
                // ERA_PUT_IN: the posts put in that turns ended before it carried are read here, as such a turn's end
                // reads them from now on, so no `free` waits on them (paper-research N-1397.p91); his, unless answered
                // since, are made due again, as such a turn's end does with his (`his_at_end`, N-412.p2197, 4)
                if was < ERA_PUT_IN && self.era >= ERA_PUT_IN {
                    for (agent, carried) in std::mem::take(&mut self.carried) {
                        let Some(g) = self.given_since.get_mut(&agent) else { continue };
                        let held: BTreeSet<String> = carried.into_iter().filter(|d| g.remove(d)).collect();
                        if g.is_empty() {
                            self.given_since.remove(&agent);
                        }
                        let (his, theirs): (BTreeSet<String>, BTreeSet<String>) = held.into_iter().partition(|d| !self.his_in(&agent, d).is_empty() && !self.answered_into(&agent, d));
                        self.read_given(cx, at, &agent, &theirs);
                        self.hand_his_again(cx, &agent, &his);
                    }
                }
                // ERA_HIS_HANDED: his posts the agent's post before it read though not handed (`passed`, coordd's from
                // `passed_his`) are told to it late, each handed on its own whatever its mark (`told_late`), as from
                // this line its post would have left them due
                if was < ERA_HIS_HANDED && self.era >= ERA_HIS_HANDED {
                    for p in value.get("passed").and_then(Value::as_array).into_iter().flatten() {
                        let (Some(t), Some(n), Some(a)) = (p[0].as_u64(), p[1].as_u64().and_then(|n| u32::try_from(n).ok()), p[2].as_str()) else { continue };
                        let Some(x) = self.tickets.edit().get_mut(&t) else { continue };
                        if !x.late_for(a, n) {
                            x.told_late.push(ToldLate { n, to: a.to_string(), given: false });
                            cx.effect(Effect::Changed { t });
                        }
                    }
                }
                (true, Ok(format!("The record's era is {} from here.", self.era)))
            }
            // the owner's hold on agents' mail (line 327): on, each ticket's mark is taken; off, what it held comes due
            Line::Event { at, name, value, .. } if name == MAIL_HOLD => self.mail_hold_switched(cx, at, value),
            // his switch of an agent's brain (BRAIN_SWITCH): the last line read for the agent is kept; it changes nothing else
            Line::Event { at, name, value, .. } if name == BRAIN_SWITCH => self.brain_switch_read(at, value),
            // a turn begun (TURN_BEGAN): the delivery it began with is kept for its agent until the turn's end
            Line::Event { name, value, .. } if name == TURN_BEGAN => self.turn_began_read(value),
            // his sign-off of an ask (SIGNED_OFF): the last line read for the post is kept; it changes nothing else
            Line::Event { at, name, value, .. } if name == SIGNED_OFF => self.signed_off_read(at, value),
            // the release his window runs and coordd's (I-1457): the last line read is kept, and it reaches programs
            // (N-1446 p1491, 6). The tickets that take it are those open before it: what the bindings on it fired then
            // opens, with the release kept, and nothing changes if one cannot open; then each ticket that takes it runs
            // its handler, in the tickets' order. A line from before it reached programs fired none, and its group has
            // no handler's run: it replays as it ran
            // a train branch's move (MERGED): kept for the steps' `on`s and the facts they read
            // (`Machine::line_answers`); it changes nothing else
            Line::Event { name, value, .. } if name == MERGED => {
                let s = |k: &str| value.get(k).and_then(Value::as_str).unwrap_or_default();
                let n = value.get("commits").and_then(Value::as_array).map_or(0, Vec::len) as u64 + value.get("unlisted").and_then(Value::as_u64).unwrap_or(0);
                (true, Ok(format!("{} of {} is at {}, {n} commits brought.", s("branch"), s("repo"), seen::short(s("tip")))))
            }
            Line::Event { at, name, value, fired, counted, .. } if name == WINDOW_RELEASE => {
                let takes: Vec<u64> = self.tickets.values().filter(|x| x.handling.is_none()).map(|x| x.t).filter(|t| self.takes_on(*t, name)).collect();
                let before = std::mem::replace(&mut self.window_release, WindowRelease::of(value));
                let opened = match self.open_fired(cx, at, name, value, fired, counted).1 {
                    Ok(s) => s,
                    Err(e) => {
                        self.window_release = before;
                        return no(e);
                    }
                };
                let mut said = match &self.window_release {
                    Some(WindowRelease { window, engine: Some(e) }) => format!("His window runs {}; coordd runs {}.", window.release, e.release),
                    Some(WindowRelease { window, engine: None }) => format!("His window runs {}.", window.release),
                    None => String::from("His window runs no release named."),
                };
                if !fired.is_empty() {
                    said = format!("{said} {opened}");
                }
                // a `during` above several tickets that take it looks once
                let mut looked = Vec::new();
                for t in takes {
                    let whose = self.during_on(t, name).unwrap_or((t, usize::MAX));
                    if looked.contains(&whose) || !self.takes_on(t, name) {
                        continue;
                    }
                    looked.push(whose);
                    let r = self.event_on(cx, t, name, value);
                    said = format!("{said} {}", r.unwrap_or_else(|e| e));
                }
                (true, Ok(said))
            }
            // the owner writing in a ticket's box (WRITING, 0.3): kept with since when; a second `on`, or an `off` with
            // none, changes nothing and is not logged
            Line::Event { at, name, value, .. } if name == syntax::WRITING => {
                let (Some(t), Some(on)) = (value.get("t").and_then(Value::as_u64), value.get("on").and_then(Value::as_bool)) else {
                    return no(String::from("Writing is switched on or off in a ticket's box."));
                };
                let id = self.id(t);
                match (on, self.writing.get(&t)) {
                    (true, Some(since)) => (false, Ok(format!("The owner is writing on {id} already, since {since}."))),
                    (false, None) => (false, Ok(format!("The owner is not writing on {id}."))),
                    (true, None) => {
                        self.writing.insert(t, at.clone());
                        (true, Ok(format!("The owner is writing on {id}.")))
                    }
                    (false, Some(_)) => {
                        self.writing.remove(&t);
                        (true, Ok(format!("The owner stopped writing on {id}.")))
                    }
                }
            }
            // a take-back of a post an agent had (0.2 B8, N-1396): struck, each agent that had it told
            Line::Event { name, value, .. } if name == TAKEN_BACK => {
                let num = |k: &str| value.get(k).and_then(Value::as_u64);
                let (Some(t), Some(n), Some(by)) = (num("t"), num("n").and_then(|n| u32::try_from(n).ok()), value.get("by").and_then(Value::as_str)) else {
                    return no(String::from("A take-back names its ticket, its post and their author."));
                };
                let to: Vec<String> = value.get("to").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
                let by = by.to_string();
                match self.strike_post(cx, t, n, &by, to) {
                    Ok(said) => (true, Ok(said)),
                    Err(e) => no(e),
                }
            }
            // the way back's after-replay step (UNTOLD): his post that no line told goes to this release's own handler
            // as his typed post does, held as its line says (UNTOLD's doc)
            Line::Event { name, value, .. } if name == UNTOLD => {
                let (t, n) = (value.get("t").and_then(Value::as_u64), value.get("n").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()));
                let Some((t, n)) = t.zip(n).filter(|(t, n)| self.tickets.get(t).is_some_and(|x| x.posts.iter().any(|p| p.n == *n))) else {
                    return no(String::from("An untold post is one of his, named by its ticket and number."));
                };
                self.untold.remove(&(t, n));
                // a line with no hold (57's, or one sent under a policy with none) holds nothing
                let hold = value.get("hold").and_then(Value::as_u64).filter(|s| *s > 0);
                // sent on while a handler of the ticket gathers his burst: it joins that burst (E1b), as a Post does
                match self.owner_lands(cx, t, n, hold) {
                    Err(first) => (true, Ok(format!("Post {n} of {} goes to its handler, held with his post {first}.", self.id(t)))),
                    Ok(Some(w)) => {
                        self.drive(cx, vec![w]);
                        (true, Ok(format!("Post {n} of {} goes to its handler.", self.id(t))))
                    }
                    Ok(None) => (true, Ok(format!("Post {n} of {} has no handler here.", self.id(t)))),
                }
            }
            // the owner's posts held under a question to him at the install: handed to every agent on their threads, once
            Line::Event { name, value, .. } if name == HELD_HANDED => {
                self.held_handed = true;
                let (t, n) = (value.get("t").and_then(Value::as_u64), value.get("n").and_then(Value::as_u64));
                let Some((t, n)) = t.zip(n) else { return (true, Ok(String::from("No post of the owner's is held under a question to him."))) };
                let u = self.tickets.values().find(|u| self.unanswered_hold(u) && u.post_of.as_ref().is_some_and(|p| p.t == t && u64::from(p.n) == n)).map(|u| u.t);
                match u {
                    Some(u) => {
                        self.hand_one(cx, u);
                        (true, Ok(format!("Post {n} of {} handed to every agent on it.", self.id(t))))
                    }
                    None => (true, Ok(format!("Post {n} of {} is held under no question now.", self.id(t)))),
                }
            }
            // a delivery entered the agent's model input (DELIVERED): the giving at its place is delivered; one that began a
            // turn is that turn's to answer, whatever a cut before it forgot (an esc with it queued: the session dequeues it
            // after the interrupt, N-1397 post 6, 2), unless a turn's end read it already
            Line::Event { name, value, .. } if name == DELIVERED => {
                let (t, to, delivery) = (value.get("t").and_then(Value::as_u64), value.get("to").and_then(Value::as_str), value.get("delivery").and_then(Value::as_str));
                let place: Option<Loc> = value.get("place").and_then(|p| serde_json::from_value(p.clone()).ok());
                let (Some(t), Some(to), Some(delivery), Some(place)) = (t, to, delivery, place) else { return (true, Ok(String::from("A delivery with no ticket, agent or place: nothing is delivered."))) };
                let when = value.get("at").and_then(Value::as_str).unwrap_or_default().to_string();
                let began = value.get("began").and_then(Value::as_bool).unwrap_or(false);
                let Some(x) = self.tickets.edit().get_mut(&t) else { return (true, Ok(format!("There is no ticket {t}."))) };
                let Some(g) = x.given.iter_mut().find(|g| g.to == to && g.delivery == delivery && g.place.as_ref().is_some_and(|p| p.file == place.file && p.offset == place.offset)) else {
                    return (true, Ok(format!("{to} was given {delivery} at no such place.")));
                };
                if g.delivered.is_some() {
                    return (true, Ok(format!("{delivery} was delivered to {to} already.")));
                }
                g.delivered = Some(when);
                let (read, before, upto) = (g.read.is_some(), g.before, g.upto);
                // shown under his posts (`TicketView::whereabouts`, the view's given rows)
                if x.posts.iter().any(|p| p.by == "owner" && p.n > before && p.n <= upto) {
                    cx.effect(Effect::Changed { t });
                }
                if began && !read && self.given_since.get(to).is_none_or(|g| !g.contains(delivery)) {
                    self.given_since.entry(to.to_string()).or_default().insert(delivery.to_string());
                    return (true, Ok(format!("{delivery} entered {to}'s input as a prompt: the turn it began holds it.")));
                }
                (true, Ok(format!("{delivery} entered {to}'s input.")))
            }
            Line::Event { at, name, value, .. } if name == DUE_AGAIN => {
                let Some(agent) = value.get("agent").and_then(Value::as_str).map(String::from) else { return (true, Ok(String::from("Due again for no agent: nothing."))) };
                let exited = value.get("exited").and_then(Value::as_bool).unwrap_or(false);
                let named: Vec<String> = value.get("deliveries").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default();
                // only what is still AGENT's since its last turn ended: a turn's end or cut between coordd's read and
                // this line has settled the rest
                let given: Vec<String> = match self.given_since.get_mut(&agent) {
                    Some(g) => named.into_iter().filter(|d| g.remove(d)).collect(),
                    None => Vec::new(),
                };
                if self.given_since.get(&agent).is_some_and(BTreeSet::is_empty) {
                    self.given_since.remove(&agent);
                }
                if let Some(i) = self.into_since.get_mut(&agent) {
                    i.retain(|d| !given.contains(d));
                }
                if let Some(c) = self.carried.get_mut(&agent) {
                    c.retain(|d| !given.contains(d));
                }
                // a process's end: posts it missed twice are due again too, to the next process
                let released = if exited { self.release_missed(cx, &agent) } else { Vec::new() };
                if given.is_empty() {
                    return (true, Ok(if released.is_empty() { format!("{agent} has none of them since its last turn ended: nothing is due again.") } else { format!("{agent}'s process ended: {} due again.", released.join(", ")) }));
                }
                self.unread_given(cx, at, &agent, given, exited)
            }
            // a step the runner refused, kept on the ticket it names that does not handle it: said to the lead (N-169)
            Line::Event { on: Some(t), name, value, .. } if name == "step_refused" && self.event_ticket(name, value) != Some(*t) => {
                let (t, value) = (*t, value.clone());
                self.refused_step(cx, t, &value);
                (true, Ok(String::from("Recorded.")))
            }
            // a runner's event kept on a ticket that does not handle it (now): only recorded (runner.rs)
            Line::Event { on: Some(t), name, value, .. } if crate::runner::recorded(name) && self.event_ticket(name, value) != Some(*t) => (true, Ok(String::from("Recorded."))),
            Line::Event { on: Some(t), name, value, .. } if name == "owner_message" => self.gather_into_thread(cx, *t, value),
            Line::Event { on: Some(t), name, value, .. } => {
                let t = *t;
                match self.event_on(cx, t, name, value) {
                    Ok(r) => (true, Ok(r)),
                    Err(e) => no(e),
                }
            }
            // his ask taken (ASKED): read once for each post, the tickets its bindings opened opening
            Line::Event { at, name, value, fired, counted, .. } if name == ASKED => {
                let post = value.get("post").and_then(Value::as_str).and_then(post_ref).and_then(|(t, n)| Some((self.ticket_named(t)?, n)));
                let Some(p) = post.filter(|p| !self.asks_raised.contains(p)) else {
                    return no(String::from("An asked line names a post no asked line named before."));
                };
                let r = self.open_fired(cx, at, name, value, fired, counted);
                if r.0 {
                    self.asks_raised.insert(p);
                }
                r
            }
            Line::Event { at, name, value, fired, counted, .. } => self.open_fired(cx, at, name, value, fired, counted),
            Line::Post { at, t, by, text, names, told, inform, esc, answers, passes, hold, kind, answering } => {
                if let Err(e) = self.ticket(*t) {
                    return no(e);
                }
                // the owner's words on a ticket that owes the owner a step (it or one below it): formed against it first
                // (ESC: an old line's `--esc`, read as it was written; new lines have none)
                if by == "owner" && esc.is_empty() && self.tickets[t].is_open() {
                    // a record from before N-807.p78 formed his words against the question about a held post, and
                    // replays so; from it on, that question is answered by his press alone, and his words are a post
                    // of their own, after each held post he did not answer is handed to every agent on the thread
                    let held = self.formed_held(cx);
                    if !held {
                        self.hand_unanswered(cx, *t);
                    }
                    if let Some(u) = self.form_owner_words(cx, *t, text, names, told, inform, held) {
                        return (true, Ok(format!("Read as the answer to {}'s step for you, if it is one; posted on {} if not.", self.id(u), self.id(*t))));
                    }
                }
                let x = &self.tickets[t];
                if names.is_empty() && told.is_empty() && inform.is_empty() && (!x.is_open() || matches!(x.status, Status::Waits { .. })) {
                    let why = if x.is_open() { "waits on the tickets below it" } else { "is closed" };
                    let leaves = self.leaves(*t);
                    let list = if leaves.is_empty() {
                        String::from("nothing below it is open")
                    } else {
                        leaves.iter().map(|(l, a, c)| format!("{} awaits {a} ({c})", self.id(*l))).collect::<Vec<_>>().join("; ")
                    };
                    return no(format!("{} {why}, so a post naming nobody reaches nobody in time. Post on the ticket where the work is, or name who should read it ({list}).", self.id(*t)));
                }
                let id = self.id(*t);
                let x = self.tickets.edit().get_mut(t).unwrap();
                let n = x.last() + 1;
                // the reply to post N, drawn under it: any post of the ticket. The owner's (a quote reply, paper-research
                // N-101 p474; when N has asks, they close with it as before) or an agent's (the reply tool, N-101.p1285)
                let answers = (*answers).filter(|a| x.posts.iter().any(|p| p.n == *a));
                // the owner's `@all` it passes on (`Input::Pass`): a post of the ticket
                let passes = (*passes).filter(|a| x.posts.iter().any(|p| p.n == *a));
                if by != "owner" {
                    // what it was handed, and his posts told to it late and given (`Ticket::answerable_upto`)
                    let h = x.answerable_upto(by);
                    x.answered_upto.insert(n, h);
                    // what was kept from it is not answered by it (0.2, delivery.rs), nor what it was not handed
                    // (`Ticket::not_answered_by`)
                    let passed = x.not_answered_by(by, h);
                    let mut kept = self.kept_posts(by, *t, h);
                    kept.extend(passed);
                    let x = self.tickets.edit().get_mut(t).unwrap();
                    if !kept.is_empty() {
                        x.not_answered.insert(n, kept);
                    }
                }
                let x = self.tickets.edit().get_mut(t).unwrap();
                // an agent's post typed an ask to him asks him from here, as a marked post did from its marks (`mark_post`)
                let asks_open = by != "owner" && kind.as_ref().is_some_and(crate::line::PostKind::asks_owner);
                x.posts.push(Post { n, at: at.clone(), by: by.clone(), text: text.clone(), head: None, step: None, names: names.clone(), told: told.clone() , auto: false, moved: None, from: None, held: None, halted: false, dropped: false, routed: false, asks: Vec::new(), asks_open, asks_closed: None, answers, inform: inform.iter().filter(|a| !names.contains(a) && !told.contains(a)).cloned().collect(), typed: None, cleaned_from: None, split_from: None, on: Vec::new(), also: Vec::new(), would_close: None, taken_back: false, joins: None, passes, comments: Vec::new(), into_turn: Vec::new(), kind: kind.clone(), answering: answering.clone() });
                x.read_own(by.clone(), n, self.era, &mut self.passed_his);
                cx.effect(Effect::Changed { t: *t });
                self.halt_post(cx, *t, n);
                if by == "owner" {
                    // typed while a handler of the ticket gathers his burst: it joins that burst (E1b)
                    match self.owner_lands(cx, *t, n, *hold) {
                        Err(first) => return (true, Ok(format!("Posted on {id} as post {n}, held with his post {first}."))),
                        Ok(Some(w)) => {
                            self.drive(cx, vec![w]);
                            return (true, Ok(format!("Posted on {id} as post {n}.")));
                        }
                        Ok(None) => {}
                    }
                }
                for a in names.iter().chain(told) {
                    cx.effect(Effect::Deliver { to: a.clone() });
                }
                // a copy's post waits for its agent whole (`copy_posted`, N-1104 p1159)
                self.copy_posted(cx, *t, n);
                if let Some(w) = self.agent_landed(cx, *t, n) {
                    self.drive(cx, vec![w]);
                }
                (true, Ok(format!("Posted on {id} as post {n}.")))
            }
            Line::Answer { t, by, head, text, .. } => {
                let x = match self.ticket(*t) {
                    Ok(x) => x,
                    Err(e) => return no(e),
                };
                let Status::Awaits { agent, owed, .. } = &x.status else {
                    return no(format!("{} awaits no answer now.", self.id(*t)));
                };
                // a hold may be answered by anyone `may edit` lets (coordd asks it); the answer is BY's
                if agent != by && !owed.hold {
                    return no(format!("{} awaits {agent}, not {by}.", self.id(*t)));
                }
                let (step, hold) = (owed.step, owed.hold);
                let raw = normal_head(head);
                // the runner ended the step's job (`RUNNER end`) and the call does not declare Ended: raised (runner.rs)
                if crate::runner::ends(agent, &raw, &owed.head) {
                    let r = crate::runner::ended(&raw);
                    self.drive(cx, vec![Work::Throw(*t, r)]);
                    return (true, Ok(format!("Step {step} of {} ended: the runner ended its job; Ended raised.", self.id(*t))));
                }
                let head = owed.head.coerce(&raw).map(|h| fill_text(&owed.head, h, text));
                // what was typed cannot be read as the step's answer (a list or a record not written as JSON, a word
                // for a number): refused by the command, saying what it takes, and not one of the step's three; only
                // an answer read that does not fit counts (paper-research N-101 p602). Such a refusal is not logged, so
                // a replay meets only lines that were taken or counted, and runs them as they were.
                if let (Err(why), None, false) = (&head, &cx.recorded, hold) {
                    return no(format!(
                        "That cannot be read as an answer to step {step} of {}: {why}. Nothing was counted. The step takes {}; a list or a record is given as JSON (`--parts '[{{\"name\": ...}}]'`), or the whole answer as one JSON object (`coord answer {} HEAD - < answer.json`).",
                        self.id(*t),
                        owed.head,
                        self.id(*t)
                    ));
                }
                let head = head.and_then(|h| owed.head.conforms(&h).map(|()| h));
                // what an `on` of the step gives is its event's alone, never a hand's (`syntax::On`; p2255, 3)
                if let Some(why) = head.as_ref().ok().and_then(|h| on_only(owed, h, &x.kind, &self.id(*t))) {
                    return no(why);
                }
                match head {
                    Ok(head) => {
                        let slow = by != "owner" && self.slow(*t);
                        self.take_answer(cx, *t, by, head, text.clone(), step);
                        if let Some(evs) = self.unborne_for(*t) {
                            return (true, Ok(format!("Step {step} of {} keeps your answer until {evs} bears it out; none kept does yet. It is taken when one does, or answer again.", self.id(*t))));
                        }
                        (true, Ok(format!("Answered step {step} of {}{}.", self.id(*t), if slow { "; it waits for the owner's approval (slow mode)" } else { "" })))
                    }
                    // replay: an answer the log took when it was given is taken again, though today's call no longer
                    // has its constructor (intake's OneOff, design §16, 3); the recorded program's arm for it runs
                    Err(_) if cx.recorded.as_ref().is_some_and(|r| !r.iter().any(|l| matches!(l, Line::Refused { t: u, .. } if u == t))) && !hold => {
                        self.take_answer(cx, *t, by, raw, text.clone(), step);
                        (true, Ok(format!("Answered step {step} of {}.", self.id(*t))))
                    }
                    // a hold's worker answers in its own time: an answer that does not fit is only refused
                    Err(why) if hold => no(format!("That answer does not fit step {step} of {}: {why}.", self.id(*t))),
                    Err(why) => {
                        let left = self.refuse(cx, *t, step, &why);
                        let more = if left == 0 { String::from(" That was the third; the step failed as Malformed.") } else { format!(" {left} more and the step fails.") };
                        (true, Err(format!("That answer does not fit step {step} of {}: {why}.{more}", self.id(*t))))
                    }
                }
            }
            Line::TurnEnded { at, agent, delivery, text, later, informs, stood, go, .. } => {
                // what the turn was given since the agent's last turn ended: counted afresh after every kept turn end,
                // and a turn that was given something is always kept, so the log replays the same count. What was
                // handed after its last message was written (LATER) is its next turn's: counted on (N-17 post 10)
                // what was put into the turn running (the owner's post, N-101 p1214) is not this turn's to reply to
                // unless the turn holds nothing else. From the era ERA_PUT_IN, put in before its last message, it
                // entered its input and is read at its end, answered on its thread or not; before it, unanswered on
                // its thread, it was the next turn's
                let given = self.turn_given(agent, later);
                // whom its last message names with `@`, if coordd said so for this turn (taken, so it never outlives it)
                let named: Vec<String> = self.reply_named.take().filter(|(a, _)| a == agent).map(|(_, n)| n).unwrap_or_default();
                let into = self.into_since.remove(agent).unwrap_or_default();
                // a turn his Esc was to stop that ended all the same: what it held is read or left as any turn's end
                self.stopped_by_him.remove(agent);
                // a step put in is not carried: the next turn, about something else, would answer it (`left_in_turn`)
                let carry = self.era < ERA_PUT_IN;
                let (late, ended): (BTreeSet<String>, BTreeSet<String>) = self.given_since.remove(agent).unwrap_or_default().into_iter().partition(|d| later.contains(d) || (carry && into.contains(d) && !given.contains(d) && !self.answered_into(agent, d) && step_id(d).is_none()));
                // his posts put into it, or handed again for being left so, are read only once answered: decided once
                // its last message is posted below (`his_at_end`, paper-research N-412.p2194)
                let his: BTreeSet<String> = if carry { BTreeSet::new() } else { ended.iter().filter(|d| self.his_in_turn(agent, d, &into)).cloned().collect() };
                // what this turn was given is read now (the first turn end after its Given, N-818 post 32); what is the
                // next turn's is read when that one ends
                self.read_given(cx, at, agent, &ended.difference(&his).cloned().collect());
                // a step put in after the last message was written begins the next turn: that turn's own, its last
                // message answers it
                let late_into: BTreeSet<String> = late.iter().filter(|d| into.contains(*d) && later.contains(*d) && step_id(d).is_none()).cloned().collect();
                if !late_into.is_empty() {
                    self.into_since.insert(agent.clone(), late_into);
                }
                // the posts put in that it carried, before the era ERA_PUT_IN: that era's line reads them
                self.carried.remove(agent);
                let carried: BTreeSet<String> = late.iter().filter(|d| !later.contains(*d)).cloned().collect();
                if !carried.is_empty() {
                    self.carried.insert(agent.clone(), carried);
                }
                if !late.is_empty() {
                    self.given_since.insert(agent.clone(), late);
                }
                let out = 'end: {
                    let answered = self.answered_by_turn(agent, delivery.as_deref(), later, stood.as_deref(), &into);
                    // a step put in that the turn ended without answering comes again, alone (`left_in_turn`)
                    self.left_in_turn(cx, at, agent, &into, later, answered.map(|(t, _)| t));
                    // a turn that asked to go to another layer answers no step: its last message is a post on the
                    // ticket whose step it would have answered (N-101 p414)
                    if let Some(l) = go {
                        let Some((t, _)) = answered else {
                            break 'end (true, Ok(format!("{agent} goes to layer {l}; its turn answered no step.")));
                        };
                        let x = self.tickets.edit().get_mut(&t).unwrap();
                        let n = x.last() + 1;
                        if text.len > 0 {
                            x.posts.push(Post { n, at: at.clone(), by: agent.clone(), text: Text::Loc { loc: text.clone() }, head: None, step: None, names: Vec::new(), told: Vec::new(), auto: true, moved: None, from: None, held: None, halted: false, dropped: false, routed: false, asks: Vec::new(), asks_open: false, asks_closed: None, answers: None, inform: Vec::new(), typed: None, cleaned_from: None, split_from: None, on: Vec::new(), also: Vec::new(), would_close: None, taken_back: false, joins: None, passes: None, comments: Vec::new(), into_turn: Vec::new(), kind: None, answering: Vec::new() });
                            x.read_own(agent, n, self.era, &mut self.passed_his);
                            cx.effect(Effect::Changed { t });
                        }
                        break 'end (true, Ok(format!("{agent} goes to layer {l}; its turn answered no step{}.", if text.len > 0 { format!(", its last message is post {n} on {}", self.id(t)) } else { String::new() })));
                    }
                    let Some((t, step)) = answered else {
                        break 'end match delivery {
                            // a turn that held only a step put in (`turn_delivery`) read it: kept, so a replay reads it
                            // too and it comes again as it did (`left_in_turn`)
                            None if ended.iter().any(|d| into.contains(d) && step_id(d).is_some()) => (true, Err(String::from("No step was answered."))),
                            None => {
                                let given: BTreeSet<String> = given.difference(&his).cloned().collect();
                                if !given.is_empty() {
                                    self.given_since.entry(agent.clone()).or_default().extend(given);
                                }
                                no(String::from("No step was answered."))
                            }
                            Some(_) => match {
                                self.answered_gifts(cx, agent, &given, text);
                                let told: BTreeSet<String> = given.iter().filter(|d| !informs.contains(d)).cloned().collect();
                                self.reply(cx, agent, &told, text, &named)
                            } {
                                Some(said) => (true, Ok(said)),
                                None => (true, Err(String::from("No step was answered."))),
                            },
                        };
                    };
                    let x = self.tickets.edit().get_mut(&t).unwrap();
                    let Status::Awaits { forming, owed, .. } = &mut x.status else { unreachable!() };
                    *forming = Some(text.clone());
                    let (call, head) = (owed.call.clone(), owed.head.clone());
                    cx.line(Line::Forming { at: at.clone(), t, step });
                    cx.effect(Effect::Form { t, step, agent: agent.clone(), call, head, text: text.clone(), words: None });
                    (true, Ok(String::new()))
                };
                // his posts it held, decided now that its last message is posted; a change there keeps the line
                if self.his_at_end(cx, at, agent, &his) { (true, out.1) } else { out }
            }
            Line::Unread { at, agent } => {
                self.into_since.remove(agent);
                self.carried.remove(agent);
                let given = self.given_since.remove(agent).unwrap_or_default();
                if given.is_empty() {
                    return no(format!("{agent} was given nothing since its last turn ended."));
                }
                self.unread_given(cx, at, agent, given.into_iter().collect(), false)
            }
            Line::TurnCut { at, agent } => {
                // a step put into the turn cut comes again, as at a turn's end without its answer: the next turn, about
                // something else, would answer it (`left_in_turn`, consult-3 N-1396 p300, 3); kept, since it was given
                let into = self.into_since.remove(agent).unwrap_or_default();
                self.left_in_turn(cx, at, agent, &into, &[], None);
                self.carried.remove(agent);
                let g = self.given_since.remove(agent).unwrap_or_default();
                // his posts to it that the turn held, put in or begun with, are not lost with it: the cut leaves them as
                // a turn's end leaves one it did not answer, read once answered, else made due again, or owed when
                // left so before (`his_at_end`; paper-research N-412.p2202, 2), from the era ERA_PUT_IN. An agent of
                // another project (`P/A`) has no turn here: its giving is the relay's carrying, done (coordd `hand_far`)
                let rule = self.era >= ERA_PUT_IN && !agent.contains('/');
                let his: BTreeSet<String> = if rule { g.iter().filter(|d| !self.his_in(agent, d).is_empty()).cloned().collect() } else { BTreeSet::new() };
                let left = his.iter().filter(|d| !self.answered_into(agent, d)).count();
                // his `@all` that another agent claimed is settled for this one: read, as one it answered (`his_post`;
                // consult-3 N-412.p2203, 1)
                let settled: BTreeSet<String> = if rule { g.iter().filter(|d| !his.contains(*d) && self.claimed_in(agent, d)).cloned().collect() } else { BTreeSet::new() };
                self.read_given(cx, at, agent, &settled);
                // cut by his own Esc while his hold is on: what it makes due again of his waits with what the hold
                // keeps, until his next post to it or his Release (`stopped_held`; paper-research N-412.p2205)
                let stopped: BTreeSet<(u64, u32)> = if self.stopped_by_him.remove(agent) && self.mail_hold.is_some() {
                    his.iter().filter(|d| !self.answered_into(agent, d) && !self.left_all_before(agent, d, &self.his_in(agent, d))).flat_map(|d| self.his_in(agent, d).into_iter().map(|n| (parse_delivery(d).map(|(t, _)| t).unwrap_or_default(), n)).collect::<Vec<_>>()).collect()
                } else {
                    BTreeSet::new()
                };
                if !stopped.is_empty() {
                    self.stopped_his.entry(agent.clone()).or_default().extend(stopped);
                }
                let changed = self.his_at_end(cx, at, agent, &his) || !settled.is_empty();
                match g.len() {
                    0 if changed => (true, Ok(format!("{agent}'s turn was cut."))),
                    0 => no(format!("{agent} was given nothing since its last turn ended.")),
                    n if left > 0 => (true, Ok(format!("{agent}'s turn was cut: {} given to it forgotten; {left} with the owner's post not answered, left as at a turn's end.", n - left))),
                    n => (true, Ok(format!("{agent}'s turn was cut: {n} given to it forgotten."))),
                }
            }
            // `coord esc` (N-36): BY stops AGENT's running turn; on the record, nothing else changes. coordd checked
            // who may and that a turn runs; what it holds back until the stop lands is its own (`stopping`)
            // the project-wide act (line 200): one line, one Interrupt for each agent it names
            Line::Esc { by, agent, also, .. } => {
                for a in std::iter::once(agent).chain(also) {
                    cx.effect(Effect::Interrupt { agent: a.clone() });
                    // his own: the turn it cuts leaves his posts waiting while his hold is on (`stopped_his`)
                    if by == "owner" {
                        self.stopped_by_him.insert(a.clone());
                    }
                }
                if also.is_empty() {
                    (true, Ok(format!("{agent}'s turn is being stopped by {by}.")))
                } else {
                    (true, Ok(format!("The turns of {agent}, {} are being stopped by {by}.", also.join(", "))))
                }
            }
            Line::Formed { at, t, step, head, why, .. } => {
                let Some(x) = self.tickets.get(t) else { return no(format!("There is no ticket {t}.")) };
                // formed after the ticket closed (a cancelled branch of `any` whose agent answered anyway): nothing to do
                if !x.is_open() {
                    return (false, Ok(String::new()));
                }
                if x.owner_words.is_some() {
                    let fits = match &x.status {
                        // his words that give what an `on` of the step gives are a post, as any not its answer
                        Status::Awaits { agent, owed, .. } if agent == "owner" && owed.step == *step => head.as_ref().map(normal_head).filter(|h| owed.head.conforms(h).is_ok() && on_only(owed, h, &x.kind, &self.id(*t)).is_none()),
                        _ => None,
                    };
                    let (on, text, names, told, inform) = self.tickets.edit().get_mut(t).unwrap().owner_words.take().unwrap();
                    match fits {
                        Some(h) => self.accept(cx, *t, "owner", h, text, *step),
                        // not an answer to the step: the words are a post, as they would have been
                        None => self.owner_post(cx, on, text, names, told, inform),
                    }
                    return (true, Ok(String::new()));
                }
                let Status::Awaits { agent, owed, forming: Some(loc), reminded, .. } = &x.status else { return no(String::from("Not forming.")) };
                if owed.step != *step {
                    return no(String::from("Not forming that step."));
                }
                let (agent, loc, reminded) = (agent.clone(), loc.clone(), *reminded);
                let typed = head.as_ref().map(normal_head).filter(|h| owed.head.conforms(h).is_ok());
                // an agent's words read as what an `on` of the step gives are not read: it is told why, as for any
                let only = typed.as_ref().and_then(|h| on_only(owed, h, &x.kind, &self.id(*t)));
                let typed = typed.filter(|_| only.is_none());
                let why = only.or_else(|| why.clone());
                match typed {
                    Some(h) => self.take_answer(cx, *t, &agent, h, Text::Loc { loc }, *step),
                    None => {
                        let x = self.tickets.edit().get_mut(t).unwrap();
                        x.remind_why = why.clone().filter(|w| !w.trim().is_empty()).map(|w| (*step, RemindWhy::Read(w)));
                        x.not_read.push((*step, crate::line::NotRead { by: agent.clone(), text: Text::Loc { loc: loc.clone() }, why: why.clone().unwrap_or_default().trim().to_string() }));
                        // the first answer of AGENT's to the step not read is reminded, the second is not (N-101 p999, 2a):
                        // counted by the answers not read, not by `reminded`, which the step given again for an ask's
                        // answer sets too. A record from before this, which passed the step up at the first, replays
                        // as recorded.
                        let unread = x.not_read.iter().filter(|(s, n)| *s == *step && n.by == agent).count();
                        let passed = cx.recorded.as_ref().is_some_and(|r| r.iter().any(|l| matches!(l, Line::PassedUp { t: u, step: s, .. } if u == t && s == step)));
                        let first = if passed { reminded.is_none() } else { unread == 1 };
                        let Status::Awaits { forming, reminded: r, .. } = &mut x.status else { unreachable!() };
                        *forming = None;
                        if first {
                            *r = Some(false);
                            cx.line(Line::Reminded { at: at.clone(), t: *t, step: *step });
                            cx.effect(Effect::Deliver { to: agent });
                        } else {
                            self.unformed_twice(cx, *t, &agent);
                        }
                        cx.effect(Effect::Changed { t: *t });
                    }
                }
                (true, Ok(String::new()))
            }
            Line::Given { at, t, to, delivery, upto, place, into, skipped } => {
                let id = self.id(*t);
                let Some(x) = self.tickets.edit().get_mut(t) else { return no(format!("There is no ticket {t}.")) };
                // the same delivery seen again (its place found in the transcript after its sender said it was taken)
                if let Some(i) = x.given.iter().position(|g| g.to == *to && g.delivery == *delivery && !g.again()) {
                    // recorded before with less than it gave (seen in the transcript after a restart, nothing handed
                    // to say how far: upto 0, prod's research N-345.p1, paper-research N-101 p1334): the session holds
                    // it, so it is given up to UPTO now, once, and is due no more; no turn is counted for it again
                    if *upto > x.given[i].upto {
                        // what it passes unhanded up to UPTO (`GivenRec::skipped`): the line's, else worked out before
                        // the mark moves
                        let skipped = match skipped {
                            Some(s) => s.clone(),
                            None => x.skipped_now(to, *upto),
                        };
                        let g = &mut x.given[i];
                        g.skipped.extend(skipped);
                        g.upto = *upto;
                        if g.place.is_none() {
                            g.place = place.clone();
                        }
                        let mu = x.mu.entry(to.clone()).or_insert(0);
                        *mu = (*mu).max(*upto);
                        cx.effect(Effect::Changed { t: *t });
                        self.delivery_given(*t, to, delivery, *upto);
                        return (true, Ok(format!("{to} holds {delivery}: given up to post {upto} now.")));
                    }
                    let g = &mut x.given[i];
                    if g.place.is_none() && place.is_some() {
                        g.place = place.clone();
                        cx.effect(Effect::Changed { t: *t });
                        return (true, Ok(format!("{delivery} was found in {to}'s transcript.")));
                    }
                    return no(format!("{to} was given {delivery} already."));
                }
                let suffix = delivery.split_once('.').map(|(_, s)| s.to_string()).unwrap_or_default();
                // what its mark passes unhanded (`GivenRec::skipped`): what the hand-over left out, on the line, else
                // worked out before the mark moves
                let skipped = match skipped {
                    Some(s) => s.clone(),
                    None => x.skipped_now(to, *upto),
                };
                let mu = x.mu.entry(to.clone()).or_insert(0);
                let before = *mu;
                *mu = (*mu).max(*upto);
                x.given.push(GivenRec { to: to.clone(), delivery: delivery.clone(), upto: *upto, at: at.clone(), place: place.clone(), before, unread: false, read: None, delivered: None, lost: false, left: false, skipped });
                self.given_since.entry(to.clone()).or_default().insert(delivery.clone());
                if *into {
                    self.into_since.entry(to.clone()).or_default().insert(delivery.clone());
                }
                let x = self.tickets.edit().get_mut(t).unwrap();
                if suffix == "c" || suffix.strip_prefix('c').is_some_and(|n| n.parse::<u32>().is_ok()) {
                    x.closed_given.insert(format!("{to} {suffix}"));
                }
                for w in x.withdrawn.iter_mut().filter(|w| w.0 == *to && suffix == format!("s{}.withdrawn", w.1)) {
                    w.3 = true;
                }
                for s in x.struck.iter_mut().filter(|s| suffix == format!("p{}.taken_back", s.n) && s.to.contains(to) && !s.told.contains(to)) {
                    s.told.push(to.clone());
                }
                // his post told late is given as itself (`N-1.p5.told`), and from the era ERA_HIS_HANDED by any giving
                // to its agent that hands it (`ToldLate`): one on its way when that era's line told it late
                let (g, era) = (x.given.last().unwrap(), self.era >= ERA_HIS_HANDED);
                for e in x.told_late.iter_mut().filter(|e| e.to == *to && (suffix == format!("p{}.{TOLD}", e.n) || (era && Ticket::hands(g, e.n)))) {
                    e.given = true;
                }
                if let Some(g) = gift_index(&suffix).and_then(|i| x.gives.get_mut(i)).filter(|g| g.to == *to) {
                    g.given = true;
                }
                if suffix == ARCHIVED_NOTICE {
                    if let Some((_, given)) = x.told_archived.get_mut(to) {
                        *given = true;
                    }
                }
                if let Status::Awaits { agent, owed, taken, refused, reminded, back, .. } = &mut x.status {
                    let s = format!("s{}", owed.step);
                    if agent == to {
                        if suffix == s {
                            *taken = true;
                        } else if suffix == format!("{s}.back") && *back == Some(false) {
                            *back = Some(true);
                        } else if suffix == format!("{s}.again") && *reminded == Some(false) {
                            *reminded = Some(true);
                        } else if suffix.starts_with(&format!("{s}.refused")) {
                            if let Some((_, g)) = refused {
                                *g = true;
                            }
                        }
                    }
                }
                cx.effect(Effect::Changed { t: *t });
                // what the program handed and this covers is in (delivery.rs)
                self.delivery_given(*t, to, delivery, *upto);
                (true, Ok(format!("{to} was given {delivery} ({id}, posts up to {upto}).")))
            }
            Line::Decided { t, step, question, answer, provenance, .. } if question == "holds" => {
                let Some(x) = self.tickets.edit().get_mut(t) else { return no(format!("There is no ticket {t}.")) };
                // 0: as logged before asks were numbered
                if *step != 0 && *step != x.closes + 1 {
                    return no(String::from("Not asking that: an earlier ask whether its goals hold."));
                }
                if x.holding.is_none() {
                    return no(String::from("Not asking whether its goals hold."));
                }
                if self.slow(*t) {
                    self.halt(cx, *t, Halting::Decide { step: *step, question: question.clone(), answer: answer.clone(), provenance: provenance.clone() });
                    return (true, Ok(String::new()));
                }
                self.use_holds(cx, *t, answer);
                (true, Ok(String::new()))
            }
            Line::Typed { t, n, agent, .. } => {
                let Some(p) = self.tickets.edit().get_mut(t).and_then(|x| x.posts.iter_mut().find(|p| p.n == *n)) else { return no(format!("There is no post {n} on ticket {t}.")) };
                p.typed = Some(agent.clone());
                // the agent read it as it was typed (into a turn already running, too: paper-research N-101 p1127, 3)
                self.tickets.edit().get_mut(t).unwrap().read_own(agent.clone(), *n, self.era, &mut self.passed_his);
                cx.effect(Effect::Changed { t: *t });
                (true, Ok(String::new()))
            }
            Line::Held { .. } => (true, Ok(String::new())),
            Line::Waits { t, n, .. } => {
                let Some(x) = self.tickets.edit().get_mut(t) else { return no(format!("There is no ticket {t}.")) };
                match n {
                    Some(n) => {
                        if !x.posts.iter().any(|p| p.n == *n && p.by == "owner") {
                            return no(format!("There is no post {n} of the owner's on ticket {t}."));
                        }
                        x.waits.insert(*n);
                    }
                    None => x.waits_message = true,
                }
                (true, Ok(String::new()))
            }
            Line::Cleaned { t, n, words, .. } => {
                let (id, w) = (self.id(*t), self.id(*words));
                if let Err(why) = self.cleaned_words(*words) {
                    return no(format!("{w} {why}."));
                }
                let Some(p) = self.tickets.edit().get_mut(t).and_then(|x| x.posts.iter_mut().find(|p| p.n == *n)) else { return no(format!("There is no post {n} on {id}.")) };
                if p.by != "owner" {
                    return no(format!("{id}.p{n} is not the owner's; only his posts are his words cleaned."));
                }
                p.cleaned_from = Some(w.clone());
                cx.effect(Effect::Changed { t: *t });
                (true, Ok(format!("{id}.p{n} is {w} cleaned; as typed, its form's text.")))
            }
            Line::Split { t, n, from, .. } => {
                let id = self.id(*t);
                if from.message.trim().is_empty() || from.of < 2 || from.point < 1 || from.point > from.of {
                    return no(format!("{id}.p{n}: a point {} of {} of message {:?} is no point of a split.", from.point, from.of, from.message));
                }
                let Some(p) = self.tickets.edit().get_mut(t).and_then(|x| x.posts.iter_mut().find(|p| p.n == *n)) else { return no(format!("There is no post {n} on {id}.")) };
                if p.by != "owner" {
                    return no(format!("{id}.p{n} is not the owner's; only his messages are split by point."));
                }
                p.split_from = Some(from.clone());
                cx.effect(Effect::Changed { t: *t });
                (true, Ok(format!("{id}.p{n} is point {} of {} of his message {}.", from.point, from.of, from.message)))
            }
            Line::AnswersAlso { t, n, also, .. } => {
                let from = self.id(*t);
                let Some(by) = self.tickets.get(t).and_then(|x| x.posts.iter().find(|p| p.n == *n)).map(|p| p.by.clone()) else { return no(format!("There is no post {n} on {from}.")) };
                // each a post of a ticket, not the answer itself
                let mut ok = Vec::new();
                for a in also {
                    let Some((u, m)) = post_id(a) else { return no(format!("{a} is no message id.")) };
                    if (u, m) == (*t, *n) || !self.tickets.get(&u).is_some_and(|y| y.posts.iter().any(|p| p.n == m)) {
                        return no(format!("{a} is no message this post can answer."));
                    }
                    ok.push((u, m, a.clone()));
                }
                for (u, m, _) in &ok {
                    if u != t {
                        self.tickets.edit().get_mut(u).unwrap().answered_from.push((*m, by.clone(), format!("{from}.p{n}")));
                        cx.effect(Effect::Changed { t: *u });
                    }
                }
                let p = self.tickets.edit().get_mut(t).unwrap().posts.iter_mut().find(|p| p.n == *n).unwrap();
                p.also = ok.iter().map(|(_, _, a)| a.clone()).collect();
                cx.effect(Effect::Changed { t: *t });
                (true, Ok(format!("{from}.p{n} answers {} too.", p.also.join(", "))))
            }
            Line::PostedOn { t, n, on, .. } => {
                let from = self.id(*t);
                let Some(by) = self.tickets.get(t).and_then(|x| x.posts.iter().find(|p| p.n == *n)).map(|p| p.by.clone()) else { return no(format!("There is no post {n} on {from}.")) };
                // each a post of the same writer on another ticket
                for a in on {
                    let Some((u, m)) = post_id(a) else { return no(format!("{a} is no message id.")) };
                    if u == *t || !self.tickets.get(&u).is_some_and(|y| y.posts.iter().any(|p| p.n == m && p.by == by)) {
                        return no(format!("{a} is no post of {by} on another thread."));
                    }
                }
                let p = self.tickets.edit().get_mut(t).unwrap().posts.iter_mut().find(|p| p.n == *n).unwrap();
                p.on = on.clone();
                cx.effect(Effect::Changed { t: *t });
                (true, Ok(format!("{from}.p{n} is also on {}.", on.join(", "))))
            }
            Line::Slow { t, on, by, .. } => {
                if by != "owner" {
                    return no(String::from("Only the owner switches a thread to slow."));
                }
                let id = self.id(*t);
                let Some(x) = self.tickets.edit().get_mut(t) else { return no(format!("There is no ticket {t}.")) };
                if x.slow == *on {
                    let project = if !*on && self.slow_scope(*t).is_some() { "; it halts while slow is set over the project" } else { "" };
                    return (false, Ok(format!("{id} is {} already{project}.", if *on { "slow" } else { "not slow" })));
                }
                x.slow = *on;
                cx.effect(Effect::Changed { t: *t });
                if *on {
                    return (true, Ok(format!("{id} is slow: each step given, answer taken, post delivered, ticket opened, policy answer used and notice given under it waits for your approval.")));
                }
                // what waits under it goes through, in the order it halted (paper-research N-89.p2, 3)
                let mut waiting: Vec<(u64, u64, u32)> = self.tickets.values().filter(|y| !y.halts.is_empty() && !self.slow(y.t) && self.is_under(y.t, *t)).flat_map(|y| y.halts.iter().map(move |h| (h.seq, y.t, h.n))).collect();
                waiting.sort();
                for (_, u, n) in &waiting {
                    let _ = self.approve(cx, *u, *n, "owner");
                }
                (true, Ok(format!("{id} is not slow: nothing more halts{}.", if waiting.is_empty() { String::new() } else { format!("; the {} halted under it went through, in order", waiting.len()) })))
            }
            // whom the policy lets set it coordd asked before the line was written (the owner is not asked)
            Line::Slowed { scope, on, .. } => {
                let held = self.slowed.contains(scope);
                if held == *on {
                    return (false, Ok(format!("Slow is {} the {scope} already.", if *on { "set over" } else { "not set over" })));
                }
                if *on {
                    self.slowed.edit().insert(scope.clone());
                } else {
                    self.slowed.edit().remove(scope);
                }
                // every open ticket's view says whether it halts
                for t in self.tickets.values().filter(|x| x.is_open()).map(|x| x.t).collect::<Vec<_>>() {
                    cx.effect(Effect::Changed { t });
                }
                if *on {
                    return (true, Ok(format!("Slow is set over the {scope}: each step given, answer taken, post delivered, ticket opened, policy answer used and notice given on a ticket it covers waits for your approval.")));
                }
                // what waits goes through, in the order it halted, but for what a ticket's switch or another scope holds
                let mut waiting: Vec<(u64, u64, u32)> = self.tickets.values().filter(|y| !y.halts.is_empty() && !self.slow(y.t)).flat_map(|y| y.halts.iter().map(move |h| (h.seq, y.t, h.n))).collect();
                waiting.sort();
                for (_, u, n) in &waiting {
                    let _ = self.approve(cx, *u, *n, "owner");
                }
                (true, Ok(format!("Slow is lifted from the {scope}: nothing more halts but under a slow ticket{}.", if waiting.is_empty() { String::new() } else { format!("; the {} halted went through, in order", waiting.len()) })))
            }
            Line::Approve { t, n, by, .. } => match self.approve(cx, *t, *n, by) {
                Ok(said) => (true, Ok(said)),
                Err(e) => no(e),
            },
            Line::Urgent { agent, by, .. } => match self.urgent(cx, agent, by) {
                Ok(said) => (true, Ok(said)),
                Err(e) => no(e),
            },
            Line::HaltReject { t, n, by, reason, .. } => match self.reject(cx, *t, *n, by, reason.as_deref()) {
                Ok(said) => (true, Ok(said)),
                Err(e) => no(e),
            },
            Line::HaltEdit { t, n, by, value, .. } => match self.halt_edit(cx, *t, *n, by, value) {
                Ok(said) => (true, Ok(said)),
                Err(e) => no(e),
            },
            Line::Halted { .. } => no(String::from("not an input line")),
            // a goal's duties as the policy read them: the `duties` line its step reads (machine/duties.rs)
            Line::Decided { at, t, step, question, answer, .. } if question == duties::QUESTION => match self.decided_duties(cx, *t, *step, answer, at) {
                Ok(said) => (true, Ok(said)),
                Err(e) => no(e),
            },
            // a model chosen for a call made for the ticket (a fresh step, the clerk's split, a forming): a record
            Line::Decided { t, step, question, .. } if question == "model-for" => {
                let Some(x) = self.tickets.get(t) else { return no(format!("There is no ticket {t}.")) };
                if *step > x.steps {
                    return no(format!("{} has no step {step}.", self.id(*t)));
                }
                (true, Ok(String::new()))
            }
            Line::Decided { t, step, question, answer, .. } if question == violated::QUESTION => match self.decided_violated(cx, *t, *step, answer) {
                Ok(said) => (true, Ok(said)),
                Err(e) => no(e),
            },
            Line::Decided { t, step, question, answer, provenance, .. } if question == "decide" => {
                let Some(x) = self.tickets.get(t) else { return no(format!("There is no ticket {t}.")) };
                let Status::Asking { owed, role } = &x.status else { return no(String::from("Not asking.")) };
                if owed.step != *step || role != POLICY {
                    return no(String::from("Not asking that."));
                }
                // a rule's answer halts on a slow ticket, as any policy answer; none (no rule decides) goes to the owner
                if self.slow(*t) && !answer.is_null() {
                    self.halt(cx, *t, Halting::Decide { step: *step, question: question.clone(), answer: answer.clone(), provenance: provenance.clone() });
                    return (true, Ok(String::new()));
                }
                self.use_decide(cx, *t, answer);
                (true, Ok(String::new()))
            }
            Line::Decided { t, step, question, answer, provenance, .. } => {
                let Some(x) = self.tickets.get(t) else { return no(format!("There is no ticket {t}.")) };
                let Status::Asking { owed, .. } = &x.status else { return no(String::from("Not asking.")) };
                if owed.step != *step || question != "fills" {
                    return no(String::from("Not asking that."));
                }
                // the decision that names coordd's own worker (its minder) is used at once (the owner N-94, 3)
                if self.slow(*t) && !answer.as_str().is_some_and(coordds_own) {
                    self.halt(cx, *t, Halting::Decide { step: *step, question: question.clone(), answer: answer.clone(), provenance: provenance.clone() });
                    return (true, Ok(String::new()));
                }
                self.use_fills(cx, *t, answer);
                (true, Ok(String::new()))
            }
            Line::Pinned { loc, to, .. } => {
                let mut changed = false;
                for x in self.tickets.edit().values_mut() {
                    for p in x.posts.iter_mut() {
                        if matches!(&p.text, Text::Loc { loc: l } if l == loc) {
                            p.text = Text::Loc { loc: to.clone() };
                            changed = true;
                        }
                    }
                    // a form's text held by its place (an agent's turn on a filing): the copy, so its parts still read; a
                    // closed run's form kept as text (`form_text`) is read back where it holds a place, and kept so again
                    let packed = x.form_text.as_deref().is_some_and(|f| f.contains("\"loc\":"));
                    if packed {
                        x.unpack_form();
                    }
                    if let Value::Object(o) = &mut x.form {
                        for v in o.values_mut() {
                            if Text::of(v).is_some_and(|t| t == Text::Loc { loc: loc.clone() }) {
                                *v = Text::Loc { loc: to.clone() }.value();
                                changed = true;
                            }
                        }
                    }
                    if packed {
                        x.pack_form();
                    }
                    if let Status::Awaits { forming: Some(l), .. } = &mut x.status {
                        if l == loc {
                            *l = to.clone();
                            changed = true;
                        }
                    }
                }
                if changed { (true, Ok(String::new())) } else { no(String::from("Nothing references that place.")) }
            }
            Line::LayerFailed { t, layer, why, .. } => {
                if !self.tickets.get(t).is_some_and(|x| x.is_open() && x.k.iter().any(|f| matches!(f, Frame::Layer { id, .. } if id == layer))) {
                    return no(format!("No layer {layer} is active on {}.", self.id(*t)));
                }
                self.layer_failed(cx, *t, layer, why);
                (true, Ok(format!("{layer} could not be made: {why}. Its scope raises NoLayer.")))
            }
            Line::Expired { t, step, provenance, .. } => {
                // a step that waits on its holder's ask is not silent: the wait is the answerer's (paper-research m1218)
                if let Some(Status::Awaits { agent, .. }) = self.tickets.get(t).map(|x| &x.status) {
                    if self.asks_on(agent, *t) {
                        return no(String::from("Its step waits on its holder's ask."));
                    }
                }
                let Some(x) = self.tickets.get(t) else { return no(format!("There is no ticket {t}.")) };
                let Status::Awaits { owed, .. } = &x.status else { return no(String::from("Not awaiting.")) };
                if owed.step != *step || owed.hold {
                    return no(String::from("Not awaiting that step, or it is held."));
                }
                if self.slow(*t) {
                    self.halt(cx, *t, Halting::Decide { step: *step, question: "expired".into(), answer: json!(true), provenance: provenance.clone() });
                    return (true, Ok(String::new()));
                }
                self.use_expired(cx, *t);
                (true, Ok(String::new()))
            }
            Line::Lapsed { t, step, head, after, because, .. } => {
                let Some(x) = self.tickets.get(t) else { return no(format!("There is no ticket {t}.")) };
                let Status::Awaits { agent, owed, .. } = &x.status else { return no(format!("{} awaits no answer now.", self.id(*t))) };
                if agent != "owner" || owed.step != *step || owed.hold {
                    return no(format!("{} does not await step {step} of the owner now.", self.id(*t)));
                }
                if self.slow(*t) {
                    return no(format!("{} is slowed: its owner's steps wait for him.", self.id(*t)));
                }
                let dflt = self.call_of(*t, &owed.call).and_then(|c| c.default);
                if dflt.is_none() || head.get("head").and_then(Value::as_str) != dflt.as_deref() {
                    return no(format!("Step {step} of {} takes no answer by itself, or not that one.", self.id(*t)));
                }
                let head = owed.head.coerce(&normal_head(head)).unwrap_or_else(|_| head.clone());
                self.tickets.edit().get_mut(t).unwrap().lapsed.push((*step, *after, because.clone()));
                self.take_answer(cx, *t, "owner", head, Text::New(String::new()), *step);
                (true, Ok(format!("Step {step} of {} went on by itself after {after} s ({because}).", self.id(*t))))
            }
            Line::Unchecked { .. } => (true, Ok(String::new())),
            Line::Unrendered { t, step, why, .. } => {
                let Some(x) = self.tickets.get(t) else { return no(format!("There is no ticket {t}.")) };
                let Status::Awaits { owed, .. } = &x.status else { return no(String::from("Not awaiting.")) };
                if owed.step != *step {
                    return no(String::from("Not awaiting that step."));
                }
                let call = owed.call.clone();
                // the agent was never given the step, so nothing is withdrawn from it
                self.drive(cx, vec![Work::Throw(*t, Raised { exc: "NoCall".into(), value: json!({"call": call, "why": why}) })]);
                (true, Ok(String::new()))
            }
            // a reload's new text of its kind, for what it has not run (live edits, retext.rs)
            Line::Edited { t, by, why, what, program: Some(text), calls, .. } if what == "reload" => self.retext(cx, *t, by, why, text, calls),
            Line::Edited { t, by, why, what, .. } if what == "cancel" => {
                let Some(x) = self.tickets.get(t) else { return no(format!("There is no ticket {t}.")) };
                if !x.is_open() {
                    return no(format!("{} is closed already.", self.id(*t)));
                }
                let r = Raised { exc: "Cancelled".into(), value: json!({"by": by, "why": why}) };
                let mut work = Vec::new();
                self.cancel(cx, *t, by, why, &r, &mut work);
                self.drive(cx, work);
                (true, Ok(String::new()))
            }
            Line::Edited { t, by, why, what, program, .. } if what == "withdraw" => {
                let Some(x) = self.tickets.get(t) else { return no(format!("There is no ticket {t}.")) };
                let id = self.id(*t);
                let (owed, agent) = match &x.status {
                    Status::Awaits { agent, owed, .. } => (owed.clone(), Some(agent.clone())),
                    Status::Asking { owed, .. } => (owed.clone(), None),
                    _ => return no(format!("{id} awaits no step now.")),
                };
                let Some(text) = program.as_deref().filter(|p| !p.trim().is_empty()) else {
                    return no(String::from("A withdrawal needs the program that takes the step's place."));
                };
                let (k, mut bad) = match self.kind_of(x) {
                    Ok(k) => (k, None),
                    // on replay the log is what happened: the act stands, the ticket is marked
                    Err(e) if cx.recorded.is_some() => (loose_kind(&x.kind), Some(e)),
                    Err(e) => return no(e),
                };
                let mut p = match syntax::parse_prog(text, 1) {
                    Ok(p) => p,
                    Err(e) => return no(format!("The program does not parse: {e}")),
                };
                if let Err(e) = check::check_in_with(&mut p, &owed.head, &owed.scope, &k.raises, &self.calls_at(*t), &self.defs.kinds, Some(&self.defs.named), cx.recorded.is_none()).or_else(|e| if cx.recorded.is_some() { bad.get_or_insert(e); Ok(()) } else { Err(e) }) {
                    let vars = owed.scope.iter().map(|(n, t)| format!("{n}: {t}")).collect::<Vec<_>>().join(", ");
                    return no(format!("The program cannot stand where step {} ({}) of {id} is: {e}. It must give {}; in scope: {}.", owed.step, owed.call, owed.head, if vars.is_empty() { "nothing" } else { &vars }));
                }
                let x = self.tickets.edit().get_mut(t).unwrap();
                if let Some(a) = &agent {
                    let d = format!("{id}.s{}", owed.step);
                    if x.given.iter().any(|g| g.to == *a && g.delivery == d) {
                        x.withdrawn.push((a.clone(), owed.step, owed.call.clone(), false, Some((by.clone(), why.clone()))));
                        cx.effect(Effect::Deliver { to: a.clone() });
                        self.ask_withdrawn(cx, *t, a, owed.step, by, why);
                    }
                }
                let x = self.tickets.edit().get_mut(t).unwrap();
                x.status = Status::Waits { children: Vec::new(), par: false };
                cx.effect(Effect::Changed { t: *t });
                if let Some(a) = &agent {
                    self.end_scoped(cx, *t, Some(a));
                }
                if let Some(why) = &bad {
                    self.unchecked(*t, why);
                }
                self.tickets.edit().get_mut(t).unwrap().trace.fill(0, Some(by.clone()), &p);
                self.drive(cx, vec![Work::Run(*t, p, owed.env)]);
                (true, Ok(format!("Step {} of {id} withdrawn.", owed.step)))
            }
            Line::Edited { t, by, what, program, .. } if what == "edit" => {
                if !self.tickets.contains_key(t) {
                    return no(format!("There is no ticket {t}."));
                }
                if let Some(r) = self.edit_hole(cx, *t, by, program.as_deref(), false) {
                    return r;
                }
                let x = &self.tickets[t];
                let id = self.id(*t);
                if matches!(&x.status, Status::Waits { children, .. } if !children.is_empty()) {
                    return self.edit_waiting(cx, *t, program.as_deref());
                }
                let owed = match &x.status {
                    Status::Awaits { owed, .. } | Status::Asking { owed, .. } => owed.clone(),
                    _ => return no(format!("{id} awaits no step and waits on no ticket below it: a ticket is edited while it awaits a step, waits on tickets below it, or rests at a hole.")),
                };
                let Some(text) = program.as_deref().filter(|p| !p.trim().is_empty()) else { return no(String::from("An edit needs the program that takes the answer.")) };
                let (k, mut bad) = match self.kind_of(x) {
                    Ok(k) => (k, None),
                    // on replay the log is what happened: the act stands, the ticket is marked
                    Err(e) if cx.recorded.is_some() => (loose_kind(&x.kind), Some(e)),
                    Err(e) => return no(e),
                };
                let mut p = match syntax::parse_prog(text, 1) {
                    Ok(p) => p,
                    Err(e) => return no(format!("The program does not parse: {e}")),
                };
                let mut scope = owed.scope.clone();
                scope.insert("answer".into(), owed.head.clone());
                if let Err(e) = check::check_in_with(&mut p, &k.result, &scope, &k.raises, &self.calls_at(*t), &self.defs.kinds, Some(&self.defs.named), cx.recorded.is_none()).or_else(|e| if cx.recorded.is_some() { bad.get_or_insert(e); Ok(()) } else { Err(e) }) {
                    let vars = scope.iter().map(|(n, t)| format!("{n}: {t}")).collect::<Vec<_>>().join(", ");
                    return no(format!("The program cannot follow step {} of {id}: {e}. It must give {}; in scope: {vars}.", owed.step, k.result));
                }
                let more = check::bound(&p, &self.defs.kinds);
                // the frames go, and with them the agents of the `let agent` scopes they were in
                let dropped: Vec<String> = self.tickets[t].k.iter().filter_map(|f| if let Frame::Scope { agent } = f { Some(agent.clone()) } else { None }).collect();
                // one the step in flight awaits ends when it answers, as a fresh agent does
                let awaited = match &self.tickets[t].status {
                    Status::Awaits { agent, .. } => Some(agent.clone()),
                    _ => None,
                };
                for a in dropped {
                    if awaited.as_ref() == Some(&a) {
                        let x = self.tickets.edit().get_mut(t).unwrap();
                        x.kept.retain(|k| *k != a);
                        x.live.push(a);
                    } else {
                        self.end_kept(cx, *t, &a);
                    }
                }
                let x = self.tickets.edit().get_mut(t).unwrap();
                // and a `with`'s context: it is what it was outside them
                if let Some(c) = x.k.iter().find_map(|f| if let Frame::With { ctx } = f { Some(ctx.clone()) } else { None }) {
                    x.ctx = c;
                }
                x.k = vec![Frame::Bind { x: Bound::Var("answer".into()), rest: p, env: owed.env.clone() }];
                x.bound = u64::from(x.steps) + more;
                cx.effect(Effect::Changed { t: *t });
                if let Some(why) = &bad {
                    self.unchecked(*t, why);
                }
                (true, Ok(format!("{id} edited: after step {} it runs the new program (at most {more} more step(s)).", owed.step)))
            }
            Line::Edited { t, by, why, what, program, .. } if what == "repair" => {
                let Some(x) = self.tickets.get(t) else { return no(format!("There is no ticket {t}.")) };
                let id = self.id(*t);
                if !matches!(x.status, Status::Failed(_)) {
                    return no(format!("{id} has not failed: only a failed ticket is repaired."));
                }
                if let Some(p) = x.parent {
                    if !matches!(&self.tickets[&p].status, Status::Waits { children, .. } if children.contains(t)) {
                        return no(format!("{id}'s failure has been taken up by {} already; repair that one.", self.id(p)));
                    }
                }
                let Some(text) = program.as_deref().filter(|p| !p.trim().is_empty()) else { return no(String::from("A repair needs the program to run.")) };
                let (k, mut bad) = match self.kind_of(x) {
                    Ok(k) => (k, None),
                    // on replay the log is what happened: the act stands, the ticket is marked
                    Err(e) if cx.recorded.is_some() => (loose_kind(&x.kind), Some(e)),
                    Err(e) => return no(e),
                };
                let mut p = match syntax::parse_prog(text, 1) {
                    Ok(p) => p,
                    Err(e) => return no(format!("The program does not parse: {e}")),
                };
                if let Err(e) = check::check_program(&mut p, &k.form, &k.result, &k.raises, &self.calls_at(*t), &self.defs.kinds, Some(&self.defs.named), cx.recorded.is_none()).or_else(|e| if cx.recorded.is_some() { bad.get_or_insert(e); Ok(()) } else { Err(e) }) {
                    let form = k.form.iter().map(|(n, t)| format!("{n}: {t}")).collect::<Vec<_>>().join(", ");
                    return no(format!("The program cannot be {id}'s: {e}. It must give {}; in scope: {}.", k.result, if form.is_empty() { "nothing".into() } else { form }));
                }
                let _ = (by, why);
                let more = check::bound(&p, &self.defs.kinds);
                let x = self.tickets.edit().get_mut(t).unwrap();
                x.bound = u64::from(x.steps) + more;
                self.restart(cx, *t, None, p);
                if let Some(why) = &bad {
                    self.unchecked(*t, why);
                }
                (true, Ok(format!("{id} repaired: it runs again.")))
            }
            Line::Edited { t, by, why, what, program, kind: Some(nk), form: Some(form), calls, .. } if what == "rekind" => {
                let Some(x) = self.tickets.get(t) else { return no(format!("There is no ticket {t}.")) };
                let id = self.id(*t);
                let Some(text) = program else { return no(String::from("A rekind needs the new kind's program.")) };
                let (k2, bad, loose) = match self.recorded_program_as(nk, text, calls, cx) {
                    Ok(k) => k,
                    Err(e) => return no(e),
                };
                if let (None, Err(e)) = (&bad, Type::Record(k2.form.clone()).conforms(form)) {
                    return no(format!("The form of a {nk} does not fit: {e}."));
                }
                if let (Some(p), true) = (x.parent, cx.recorded.is_none()) {
                    // the parent's pattern waits for it: the new kind keeps its promise
                    if !x.is_open() {
                        return no(format!("{id} has given {} its result already; it cannot change kind now.", self.id(p)));
                    }
                    let k = match self.kind_of(x) {
                        Ok(k) => k,
                        Err(e) => return no(e),
                    };
                    let more: Vec<&String> = k2.raises.iter().filter(|e| !k.raises.contains(e)).collect();
                    if k2.result != k.result || !more.is_empty() {
                        return no(format!("{} waits for {id} as a {}: a {nk} must return {} and raise nothing more than {} (it returns {}{}).", self.id(p), x.kind, k.result, if k.raises.is_empty() { "nothing".into() } else { k.raises.join(", ") }, k2.result, if more.is_empty() { String::new() } else { format!(" and raises {}", more.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")) }));
                    }
                }
                if x.is_open() {
                    self.take_back(cx, *t, by, why);
                }
                let x = self.tickets.edit().get_mut(t).unwrap();
                x.kind = nk.clone();
                x.form = form.clone();
                x.program = text.clone();
                x.calls = calls.clone();
                x.goals = k2.goals.clone();
                x.handlers = k2.handlers.clone();
                x.shaping = None;
                x.bound = u64::from(x.steps) + check::bound(&k2.pattern, &self.defs.kinds);
                self.restart(cx, *t, Some(form.clone()), k2.pattern);
                if let Some(x) = self.tickets.edit().get_mut(t) {
                    x.loose = loose;
                }
                if let Some(why) = &bad {
                    self.unchecked(*t, why);
                }
                (true, Ok(format!("{id} is a {nk} now: {}.", self.id(*t))))
            }
            Line::Move { at, t, n, to, new, by } => {
                let mut reply = None;
                if let Some(program) = new {
                    // to a new thread: its kind is the program's (`kind thread`; a note in older logs)
                    if self.tickets.contains_key(to) {
                        return no(format!("Ticket number {to} is taken."));
                    }
                    let Some(p) = self.tickets.get(t).and_then(|x| x.posts.iter().find(|p| p.n == *n && p.moved.is_none())).cloned() else {
                        return no(format!("{} has no post {n} to move.", self.id(*t)));
                    };
                    let kind = program.lines().next().and_then(|l| l.strip_prefix("kind ")).and_then(|l| l.split_whitespace().next()).unwrap_or("thread").to_string();
                    let (k, bad) = match self.recorded_program(&kind, program, &BTreeMap::new(), cx) {
                        Ok(k) => k,
                        Err(e) => return no(e),
                    };
                    if kind == "thread" {
                        // to the agent it was addressed to (the first it names, else its author), resting with it on it
                        let Some(agent) = p.names.iter().find(|a| *a != "owner").cloned().or_else(|| (p.by != "owner").then(|| p.by.clone())) else {
                            return no(format!("{}.p{n} is addressed to no agent, so it can be no thread of its own.", self.id(*t)));
                        };
                        let form = json!({"to": agent, "names": [], "text": p.text.value(), "addressed": {"head": "Named"}});
                        let mut x = self.new_ticket(*to, &kind, None, None, form.clone(), json!({}), program.clone(), &k, at);
                        x.raised_by = Some(by.clone());
                        let env: Env = form.as_object().map(|o| o.iter().map(|(a, b)| (a.clone(), b.clone())).collect()).unwrap_or_default();
                        x.status = Status::Rests { env, scope: k.form.iter().cloned().collect() };
                        self.tickets.edit().insert(*to, x);
                        cx.line(Line::Rests { at: at.clone(), t: *to });
                        // the agent's reply to it, if that is the next post, goes with it
                        reply = self.tickets[t].posts.iter().find(|q| q.n > *n && q.moved.is_none()).filter(|q| q.by == agent && q.auto && q.names.contains(&p.by)).map(|q| q.n);
                    } else {
                        let x = self.new_ticket(*to, &kind, None, Some(by.clone()), json!({}), json!({}), program.clone(), &k, at);
                        self.tickets.edit().insert(*to, x);
                    }
                    self.next = self.next.max(to + 1);
                    if let Some(why) = &bad {
                        self.unchecked(*to, why);
                    }
                    if kind != "thread" {
                        self.drive(cx, vec![Work::Run(*to, k.pattern, Env::new())]);
                    }
                }
                let made = new.is_some();
                let mut work = Vec::new();
                let moved = self.move_post(cx, *t, *n, *to, by, &mut work);
                let r = match (moved, reply) {
                    (Ok(said), Some(r)) => match self.move_post(cx, *t, r, *to, by, &mut work) {
                        Ok(more) => (true, Ok(format!("{said} {more}"))),
                        Err(e) => (true, Ok(format!("{said} (its reply stayed: {e})"))),
                    },
                    (Ok(said), None) => (true, Ok(said)),
                    (Err(e), _) => no(e),
                };
                // a new thread arrives with what was moved into it: its hole is not asked for that
                if made && matches!(self.tickets.get(to).map(|x| &x.status), Some(Status::Rests { .. })) {
                    let mark = self.tickets[to].mark(self.closed_below(*to));
                    self.tickets.edit().get_mut(to).unwrap().asked_at = Some(mark);
                }
                self.drive(cx, work);
                r
            }
            Line::Ask { by, text, context, .. } => {
                // on replay the log is what happened, whatever the switch says today
                if !self.defs.block_ask && cx.recorded.is_none() {
                    return no(String::from("In this project an ask does not hold your turn for the owner's answer. Put the question to him with `coord ask SUBJECT`, which does not wait, and go on: his answer comes as a message. To read a file outside your folders, use the Read tool on its whole path instead: he is asked in your chat."));
                }
                if self.waiting_ask(by).is_some() || self.held_asks().iter().any(|(_, q)| q.ask.by == *by) {
                    return no(format!("{by} has an ask waiting already: one at a time."));
                }
                // the ticket whose step BY holds, or the nearest above it whose kind takes asks
                let Some(held) = self.held_by(by) else { return no(format!("{by} holds no step, so its ask has no ticket to wait on: ask with `coord ask SUBJECT`, which does not wait.")) };
                match self.ask_waiting(cx, by, text, context.clone(), held, true, true) {
                    Ok(r) => (true, Ok(r)),
                    Err(e) => no(e),
                }
            }
            Line::AskLater { by, .. } => {
                let Some(t) = self.waiting_ask(by) else {
                    // a question held for a look (ask_minded.rs): it goes on, answered later
                    let Some(q) = self.held_ask_mut(by).filter(|q| q.ask.waiting) else { return no(format!("{by} has no ask waiting.")) };
                    q.ask.waiting = false;
                    cx.effect(Effect::Deliver { to: by.clone() });
                    return (true, Ok(format!("{by}'s ask, held for a look, is answered later, as a delivery.")));
                };
                if let Some(a) = self.tickets.edit().get_mut(&t).unwrap().ask.as_mut() {
                    a.waiting = false;
                }
                cx.effect(Effect::Changed { t });
                // what was held back while it waited is due now
                cx.effect(Effect::Deliver { to: by.clone() });
                (true, Ok(format!("{by}'s ask ({}) is answered later, as a delivery.", self.id(t))))
            }
            Line::Undo { t, by, .. } => {
                let id = self.id(*t);
                let Some(x) = self.tickets.get(t) else { return no(format!("There is no ticket {t}.")) };
                let Some(i) = x.shaped.iter().rposition(|s| !s.undone && s.refused.is_none() && s.answer.get("head").and_then(Value::as_str) != Some("Rest")) else {
                    return no(format!("{id} has no answer at a hole to undo."));
                };
                let s = x.shaped[i].clone();
                if x.is_open() {
                    self.take_back(cx, *t, by, "undone");
                }
                let closed = self.closed_below(*t);
                let x = self.tickets.edit().get_mut(t).unwrap();
                x.shaped[i].undone = true;
                x.shaped[i].undone_steps = x.steps;
                // back at the hole the answer filled or closed with
                let tr = &mut x.trace;
                if let Some(at) = tr.parts.iter().rev().find(|q| q.by.is_none() && q.source == s.n).and_then(|q| q.at.clone()) {
                    tr.now = Some(at);
                }
                tr.back();
                if let Some(h) = tr.now.clone().and_then(|k| tr.visits.get_mut(&k)) {
                    h.value = None;
                }
                x.status = Status::Rests { env: s.env, scope: s.scope };
                x.closed_at = None;
                // open again: a run's form as values again (`form_text`)
                x.unpack_form();
                x.k.clear();
                x.shaping = None;
                x.asked_at = Some(x.mark(closed));
                cx.line(Line::Rests { at: cx.now.clone(), t: *t });
                cx.effect(Effect::Changed { t: *t });
                (true, Ok(format!("{id}: the answer at its hole is undone; it rests there again.")))
            }
            Line::Keep { ts, by, .. } => match self.keep(cx, ts, by) {
                Ok(said) => (true, Ok(said)),
                Err(e) => no(e),
            },
            Line::NothingToAnswer { at, t, n, by } => {
                if by != "owner" {
                    return no(String::from("Only the owner says a post asks him nothing to answer."));
                }
                let id = self.id(*t);
                let Some(p) = self.tickets.edit().get_mut(t).and_then(|x| x.posts.iter_mut().find(|p| p.n == *n)) else { return no(format!("{id} has no post {n}.")) };
                if !p.asks_open {
                    return (false, Ok(format!("Post {n} of {id} asks nothing open.")));
                }
                p.asks_open = false;
                self.tickets.edit().get_mut(t).unwrap().nothing.insert(*n, at.clone());
                cx.effect(Effect::Changed { t: *t });
                (true, Ok(format!("Post {n} of {id}: nothing to answer.")))
            }
            Line::AsksWithdrawn { at, t, n, by, why } => match self.withdraw_asks(cx, at, *t, *n, by, why) {
                Ok(said) => (true, Ok(said)),
                Err(e) => no(e),
            },
            Line::TakenBack { t, n, by, .. } => match self.take_back_post(cx, *t, *n, by) {
                Ok(said) => (true, Ok(said)),
                Err(e) => no(e),
            },
            Line::Reopen { t, .. } => {
                let Some(x) = self.tickets.get(t) else { return no(format!("There is no ticket {t}.")) };
                let id = self.id(*t);
                // nothing else reopens: a thread or a note stays closed (design-two-systems §16)
                if x.kind != "item" {
                    return no(format!("{id} is not an item: only an item is reopened."));
                }
                if x.is_open() {
                    return no(format!("{id} is open."));
                }
                let k = match self.kind_of(x) {
                    Ok(k) => k,
                    Err(e) => return no(e),
                };
                self.restart(cx, *t, None, k.pattern);
                (true, Ok(format!("{id} is open again.")))
            }
            Line::Archive { t, on, by, .. } => {
                let id = self.id(*t);
                let Some(x) = self.tickets.get(t).filter(|x| x.handling.is_none()) else { return no(format!("There is no ticket {id}.")) };
                // his alone, whatever the policy says, and on replay too, so that a record a broken coordd wrote cannot
                // archive his threads (docket-core3, N-867.p27, 1)
                if by != "owner" {
                    return no(String::from("Only the owner archives a ticket, or unarchives one: it says what is of importance to him."));
                }
                match (*on, x.archived) {
                    (true, true) => return (false, Ok(format!("{id} is archived already."))),
                    (false, false) => return (false, Ok(format!("{id} is not archived."))),
                    _ => {}
                }
                // taken live from this release; the one before reads it only in a record it replays (two trains,
                // N-807.p12), so the way back replays what this one writes
                if !*on {
                    self.unarchive(cx, *t);
                    return (true, Ok(format!("{id} unarchived: it is back as it was.")));
                }
                let x = self.tickets.edit().get_mut(t).unwrap();
                x.archived = true;
                x.told_archived.clear();
                cx.effect(Effect::Changed { t: *t });
                // whoever holds work under it is told once (notify.eflint, event "archived")
                cx.effect(Effect::Notify { t: *t, event: "archived".into(), actor: by.clone(), post: None });
                (true, Ok(format!("{id} archived: it leaves your lists, and nothing at or under it is handed to anyone until you unarchive it.")))
            }
            Line::Comments { at, t, n, by, comments } => {
                let id = self.id(*t);
                let Some(x) = self.tickets.edit().get_mut(t) else { return no(format!("There is no ticket {id}.")) };
                let Some(p) = x.posts.iter_mut().find(|p| p.n == *n) else { return no(format!("There is no post {n} on {id}.")) };
                if p.by != *by {
                    return no(format!("Post {n} of {id} is {}'s: only its writer gives its comments.", p.by));
                }
                let mut ids: std::collections::HashSet<&str> = p.comments.iter().map(|c| c.id.as_str()).collect();
                if let Some(c) = comments.iter().find(|c| c.id.is_empty() || !ids.insert(&c.id)) {
                    return no(format!("Comment {:?} on {id}.p{n}: each needs an id of its own.", c.id));
                }
                p.comments.extend(comments.iter().cloned().map(|c| crate::comment::Comment { author: by.clone(), at: if c.at.is_empty() { at.clone() } else { c.at }, ..c }));
                cx.effect(Effect::Changed { t: *t });
                (true, Ok(format!("{} on {id}.p{n}.", if comments.len() == 1 { "1 comment".to_string() } else { format!("{} comments", comments.len()) })))
            }
            Line::CommentResolved { t, n, id: c, resolved, by, .. } => {
                let id = self.id(*t);
                let Some(x) = self.tickets.edit().get_mut(t) else { return no(format!("There is no ticket {id}.")) };
                let Some(p) = x.posts.iter_mut().find(|p| p.n == *n) else { return no(format!("There is no post {n} on {id}.")) };
                if by != "owner" && p.by != *by {
                    return no(format!("Only the owner or {} resolves a comment of {id}.p{n}.", p.by));
                }
                let Some(k) = p.comments.iter_mut().find(|k| k.id == *c) else { return no(format!("{id}.p{n} has no comment {c}.")) };
                if k.resolved == *resolved {
                    return (false, Ok(format!("Comment {c} of {id}.p{n} is {} already.", if *resolved { "resolved" } else { "open" })));
                }
                k.resolved = *resolved;
                cx.effect(Effect::Changed { t: *t });
                (true, Ok(format!("Comment {c} of {id}.p{n} {}.", if *resolved { "resolved" } else { "reopened" })))
            }
            Line::Star { at, t, on, by, post } => {
                let id = self.id(*t);
                let Some(x) = self.tickets.edit().get_mut(t) else { return no(format!("There is no ticket {id}.")) };
                if let Some(n) = post {
                    if !x.posts.iter().any(|p| p.n == *n) {
                        return no(format!("There is no post {n} on {id}."));
                    }
                }
                let what = post.map_or_else(|| id.clone(), |n| format!("{id}.p{n}"));
                let whose = if by == "owner" { String::from("you") } else { by.clone() };
                // the owner opening a post starred for him (paper-research N-101.p1295): his unstar of a post ends every
                // star on it, whoever made it, and is the record that he opened it
                if !*on && by == "owner" && post.is_some() && x.stars.iter().any(|s| s.post == *post) {
                    x.stars.retain(|s| s.post != *post);
                    cx.effect(Effect::Changed { t: *t });
                    return (true, Ok(format!("{what} opened by you; its stars end.")));
                }
                match (*on, x.stars.iter().position(|s| s.post == *post && s.by == *by)) {
                    (true, Some(_)) => return (false, Ok(format!("{what} is starred by {whose} already."))),
                    // the owner's thread starred before the stars were kept by whom: `starred` alone
                    (false, None) if post.is_none() && by == "owner" && x.starred => {}
                    (false, None) => return (false, Ok(format!("{what} is not starred by {whose}."))),
                    (true, None) => x.stars.push(crate::view::ViewStar { post: *post, by: by.clone(), at: at.clone() }),
                    (false, Some(i)) => _ = x.stars.remove(i),
                }
                // the owner's own star on a thread orders his list, as before
                if post.is_none() && by == "owner" {
                    x.starred = *on;
                }
                cx.effect(Effect::Changed { t: *t });
                (true, Ok(format!("{what} {} by {whose}.", if *on { "starred" } else { "unstarred" })))
            }
            Line::Tied { t, to, n, by, .. } => {
                let id = self.id(*t);
                let Some(x) = self.tickets.get(t) else { return no(format!("There is no ticket {id}.")) };
                if let Err(e) = self.may_outline(x, by, "tie") {
                    return no(e);
                }
                let before = self.outline_parent(*t);
                match to {
                    Some(p) => {
                        let pid = self.id(*p);
                        if !self.tickets.contains_key(p) {
                            return no(format!("There is no ticket {pid}."));
                        }
                        if p == t {
                            return no(format!("{id} cannot be tied under itself."));
                        }
                        // a loop: P's outline parents reach T
                        let mut chain = vec![*p];
                        let mut u = *p;
                        while let Some((q, _)) = self.outline_parent(u) {
                            chain.push(q);
                            if q == *t {
                                let path: Vec<String> = chain.iter().map(|c| self.id(*c)).collect();
                                return no(format!("{pid} is already under {id} ({}): tying {id} under {pid} would make a loop.", path.join(" → ")));
                            }
                            if chain.len() > self.tickets.len() {
                                break;
                            }
                            u = q;
                        }
                        if before == Some((*p, *n)) {
                            return (false, Ok(format!("{id} is tied under {pid} already.")));
                        }
                        self.tickets.edit().get_mut(t).unwrap().tie = Some(Some((*p, *n)));
                        cx.effect(Effect::Changed { t: *t });
                        cx.effect(Effect::Changed { t: *p });
                        if let Some((o, _)) = before.filter(|(o, _)| o != p) {
                            cx.effect(Effect::Changed { t: o });
                        }
                        (true, Ok(format!("{id} is tied under {pid}, at its post {n}.")))
                    }
                    None => {
                        if before.is_none() {
                            return (false, Ok(format!("{id} is under no thread.")));
                        }
                        self.tickets.edit().get_mut(t).unwrap().tie = Some(None);
                        cx.effect(Effect::Changed { t: *t });
                        if let Some((o, _)) = before {
                            cx.effect(Effect::Changed { t: o });
                        }
                        (true, Ok(format!("{id} is untied: it stands under no thread in the outline.")))
                    }
                }
            }
            Line::Titled { t, label, by, .. } => {
                let id = self.id(*t);
                let Some(x) = self.tickets.get(t) else { return no(format!("There is no ticket {id}.")) };
                // the agent a thread of the owner's was written to gives it its first title as it answers (N-807.p4): his
                // threads were headed by his raw first line, nobody else titling them. Live since the release after the
                // one that took it on replay only (E12, 31492c8a, carries that), so a `back` reads such a line
                // (docket-core3, N-807.p12; his "good titles", N-985)
                let first_by_addressee = x.titled.is_none() && x.opener.as_deref() == Some("owner") && x.form.get("to").and_then(Value::as_str) == Some(by.as_str());
                if !first_by_addressee {
                    if let Err(e) = self.may_outline(x, by, "title") {
                        return no(e);
                    }
                }
                let label = label.trim();
                if label.is_empty() {
                    return no(format!("A title for {id} needs words."));
                }
                if x.titled.as_deref() == Some(label) {
                    return (false, Ok(format!("{id} is titled so already.")));
                }
                self.tickets.edit().get_mut(t).unwrap().titled = Some(label.to_string());
                cx.effect(Effect::Changed { t: *t });
                if let Some((p, _)) = self.outline_parent(*t) {
                    cx.effect(Effect::Changed { t: p });
                }
                (true, Ok(format!("{id} is titled \"{label}\".")))
            }
            Line::Ended { at, t, by, how, why } => self.end_thread(cx, *t, by, how, why, at),
            Line::Started { t, by, .. } => {
                let Some(x) = self.tickets.edit().get_mut(t) else { return no(format!("There is no ticket {t}.")) };
                if x.kind != "item" || !x.is_open() {
                    return no(format!("{} is not an open item.", self.id(*t)));
                }
                if x.started.contains(by) {
                    return (false, Ok(format!("{by} works on {} already.", self.id(*t))));
                }
                x.started.push(by.clone());
                cx.effect(Effect::Changed { t: *t });
                // the start handed to the item as an act, `is` "start", on the path an act's line takes (`Line::Act`):
                // its kind's `when act`, where it has one, runs raised by the starter (item.kind's `seek`); an item
                // opened before that handler has none in its program, and a replay runs only the runs its record has
                let value = crate::runner::act_value(by, "start", "", Some(self.id(*t)), &serde_json::Map::new());
                if self.event_ticket("act", &value).is_some() {
                    let _ = self.event_on(cx, *t, "act", &value);
                }
                (true, Ok(format!("{by} works on {}.", self.id(*t))))
            }
            Line::Notified { t, to, inform, post: Some(n), .. } => {
                // whom policy adds to a post made without asking it (an agent's turn reply)
                let Some(p) = self.tickets.edit().get_mut(t).and_then(|x| x.posts.iter_mut().find(|p| p.n == *n)) else { return no(format!("There is no post {n} on ticket {t}.")) };
                let new: Vec<String> = to.iter().filter(|a| **a != p.by && !p.names.contains(a) && !p.told.contains(a)).cloned().collect();
                let quiet: Vec<String> = inform.iter().filter(|a| **a != p.by && !p.names.contains(a) && !p.told.contains(a) && !new.contains(a) && !p.inform.contains(a)).cloned().collect();
                if new.is_empty() && quiet.is_empty() {
                    return (false, Ok(String::new()));
                }
                p.told.extend(new.iter().cloned());
                p.inform.extend(quiet);
                for a in new {
                    cx.effect(Effect::Deliver { to: a });
                }
                cx.effect(Effect::Changed { t: *t });
                (true, Ok(String::new()))
            }
            // whom policy tells, once, that the owner archived T (`notify` and `inform`, event "archived"): the one
            // thing handed from an archived ticket; nothing once he has unarchived it
            Line::Notified { t, event, to, inform, .. } if event == "archived" => {
                let Some(x) = self.tickets.edit().get_mut(t).filter(|x| x.archived) else { return (false, Ok(String::new())) };
                let new: Vec<(String, bool)> = to.iter().map(|a| (a.clone(), false)).chain(inform.iter().filter(|a| !to.contains(a)).map(|a| (a.clone(), true))).filter(|(a, _)| a != "owner" && !x.told_archived.contains_key(a)).collect();
                if new.is_empty() {
                    return (false, Ok(String::new()));
                }
                for (a, quiet) in new {
                    x.told_archived.insert(a.clone(), (quiet, false));
                    if !quiet {
                        cx.effect(Effect::Deliver { to: a });
                    }
                }
                cx.effect(Effect::Changed { t: *t });
                (true, Ok(String::new()))
            }
            Line::Notified { t, to, inform, .. } => {
                let Some(x) = self.tickets.edit().get_mut(t) else { return no(format!("There is no ticket {t}.")) };
                let new: Vec<String> = to.iter().filter(|a| x.opener.as_ref() != Some(*a) && !x.told_closed.contains(*a)).cloned().collect();
                let quiet: Vec<String> = inform.iter().filter(|a| x.opener.as_ref() != Some(*a) && !x.told_closed.contains(*a) && !new.contains(a)).cloned().collect();
                if new.is_empty() && quiet.is_empty() {
                    return (false, Ok(String::new())); // nobody more to tell: nothing to log
                }
                for a in new {
                    x.told_closed.insert(a.clone());
                    cx.effect(Effect::Deliver { to: a });
                }
                // informed: due at the agent's next turn, which this does not start
                for a in quiet {
                    x.told_closed.insert(a.clone());
                    x.informed.insert(a);
                }
                cx.effect(Effect::Changed { t: *t });
                (true, Ok(String::new()))
            }
            Line::StartFailed { agent, why, .. } => self.start_failed(cx, agent, why),
            Line::RunnerRefused { t, step, why, .. } => {
                let Some(x) = self.tickets.get(t).filter(|x| x.is_open()) else { return no(format!("There is no open ticket {t}.")) };
                let Status::Awaits { agent, owed, .. } = &x.status else { return no(String::from("Not awaiting.")) };
                if agent != crate::runner::RUNNER || owed.step != *step {
                    return no(String::from("Not awaiting that step of the runner."));
                }
                let r = Raised { exc: "RunnerRefused".into(), value: json!({"head": "RunnerRefused", "call": owed.call, "why": why}) };
                let id = self.id(*t);
                self.drive(cx, vec![Work::Throw(*t, r)]);
                (true, Ok(format!("The runner refused step {step} of {id}: {why}. It raises RunnerRefused.")))
            }
            Line::ModelRefused { t, step, agent, why, .. } => {
                let Some(x) = self.tickets.get(t).filter(|x| x.is_open()) else { return no(format!("There is no open ticket {t}.")) };
                let Status::Awaits { agent: a, owed, .. } = &x.status else { return no(String::from("Not awaiting.")) };
                if a != agent || owed.step != *step {
                    return no(format!("Not awaiting step {step} of {agent}."));
                }
                let r = Raised { exc: "NoWorker".into(), value: json!({"head": "NoWorker", "agent": agent, "why": why}) };
                let id = self.id(*t);
                // kept on the step, where the picture shows it (docket-edge N-235)
                let x = self.tickets.edit().get_mut(t).unwrap();
                if let Some(v) = x.trace.now.clone().and_then(|k| x.trace.visits.get_mut(&k)) {
                    v.refused = Some((agent.clone(), why.clone(), cx.now.clone()));
                }
                self.drive(cx, vec![Work::Throw(*t, r)]);
                (true, Ok(format!("Step {step} of {id} is not given to {agent}: {why}. It raises NoWorker.")))
            }
            Line::Moved { at: when, t, agent, from, to, why, left, .. } => {
                self.moved.insert(agent.clone(), LastMove { at: when.clone(), from: from.clone(), to: to.clone(), why: why.clone(), left: left.clone() });
                // into the layer of T's step, which it was given already (and has not answered): handed it there again,
                // the step itself, not a notice that only names it (paper-research N-101 p629: an agent back from
                // `coord go` answered "nothing to run" to the notice)
                if let Some(t) = *t {
                    let layer = self.layer_of(t);
                    if let Some(x) = self.tickets.edit().get_mut(&t)
                        && let Status::Awaits { agent: a, taken: true, forming: None, back, .. } = &mut x.status
                        && a == agent
                        && to.is_some()
                        && layer == *to
                    {
                        *back = Some(false);
                        cx.effect(Effect::Deliver { to: agent.clone() });
                        cx.effect(Effect::Changed { t });
                    }
                }
                let at = |v: &Option<String>| v.clone().unwrap_or_else(|| String::from("its own folder"));
                (true, Ok(format!("{agent} moved from {} to {} ({why}).", at(from), at(to))))
            }
            Line::Taken { names, .. } => {
                self.taken.extend(names.iter().cloned());
                (true, Ok(format!("{} name(s) taken by agents this record did not make.", names.len())))
            }
            Line::Made { t, agent, by, from, .. } => {
                self.made.push(Made { agent: agent.clone(), by: by.clone(), from: from.clone(), t: *t, name: None });
                let how = from.as_ref().map_or_else(|| String::from("a new agent"), |f| format!("a copy of {f}"));
                (true, Ok(format!("{agent} was made by {by}, {how}.")))
            }
            Line::Unfit { t, step, why, .. } => {
                let Some(x) = self.tickets.get(t).filter(|x| x.is_open()) else { return no(format!("There is no open ticket {t}.")) };
                let Status::Awaits { owed, .. } = &x.status else { return no(String::from("Not awaiting.")) };
                if owed.step != *step || owed.hold {
                    return no(String::from("Not awaiting that step."));
                }
                let id = self.id(*t);
                let left = self.refuse(cx, *t, *step, why);
                let more = if left == 0 { String::from(" That was the third; the step failed as Malformed.") } else { format!(" {left} more and the step fails.") };
                (true, Err(format!("That answer does not fit step {step} of {id}: {why}.{more}")))
            }
            _ => no(String::from("not an input line")),
        }
    }

    /// GIVEN, given to AGENT and not read, made due again once (`Line::Unread`, and `DUE_AGAIN` for some of them): what
    /// Given did is undone, so the next hand-over carries each and says it is a second one. One made due again before
    /// and still not read is said on its ticket, not handed a third time, unless EXITED (its process ended before its
    /// text entered: the session did not drop it, 0.2 A4) or that earlier time was itself a process's end.
    fn unread_given(&mut self, cx: &mut Cx, at: &At, agent: &String, given: Vec<String>, exited: bool) -> (bool, Result<String, String>) {
        let (mut again, mut twice) = (Vec::new(), Vec::new());
        for d in &given {
            let Some((t, suffix)) = parse_delivery(d) else { continue };
            let Some(x) = self.tickets.edit().get_mut(&t) else { continue };
            if !exited && x.given.iter().any(|g| g.to == *agent && g.delivery == *d && g.unread && !g.lost) {
                twice.push((t, d.clone()));
                continue;
            }
            let Some(g) = x.given.iter_mut().rev().find(|g| g.to == *agent && g.delivery == *d) else { continue };
            g.unread = true;
            g.lost = exited;
            g.read = None;
            let (upto, before) = (g.upto, g.before);
            undo_given(x, agent, &suffix, upto, before, self.era);
            cx.effect(Effect::Changed { t });
            again.push(d.clone());
        }
        // handed twice and not read: said on the ticket, not handed a third time now (a step: Silent's business; posts:
        // at the agent's next turn's end or process's end)
        for (t, d) in &twice {
            let x = self.tickets.edit().get_mut(t).unwrap();
            x.unread_twice.insert((agent.clone(), d.clone()));
            // posts (his or any agent's; a step stays Silent's business) are due again, never left behind, and held
            // back until AGENT's next turn's end or its process's end (`missed`, `release_missed`): a later post on the
            // ticket brings them along (its delivery is another); the line under each says why meanwhile (the view's
            // given rows: not read). "Nothing is dropped or replaced by the engine" (runtime-design §2; consult-3,
            // N-818.p78, 2)
            let suffix = d.split_once('.').map(|(_, s)| s.to_string()).unwrap_or_default();
            if let Some((upto, before, reach)) = x.given.iter().rev().find(|g| g.to == *agent && g.delivery == *d).map(|g| (g.upto, g.before, Ticket::reach(g))) {
                // his post told late (`N-1.p5.told`, given with the mark as it stood): that post
                let shown = match told_post(d) {
                    Some((_, n)) => x.posts.iter().any(|p| p.n == n && p.by != *agent),
                    None => post_id(d).is_some() && x.posts.iter().any(|p| p.n > before && p.n <= upto && p.by != *agent),
                };
                if shown {
                    let g = x.given.iter_mut().rev().find(|g| g.to == *agent && g.delivery == *d).unwrap();
                    (g.unread, g.read) = (true, None);
                    undo_given(x, agent, &suffix, upto, before, self.era);
                    self.missed.entry(agent.clone()).or_default().insert(d.clone(), (*t, reach));
                }
            }
            let x = self.tickets.edit().get_mut(t).unwrap();
            let n = x.last() + 1;
            x.posts.push(Post { n, at: at.clone(), by: String::from("coordd"), text: Text::New(format!("{agent} was given {d} twice and did not read it.")), head: None, step: None, names: Vec::new(), told: Vec::new(), auto: true, moved: None, from: None, held: None, halted: false, dropped: false, routed: false, asks: Vec::new(), asks_open: false, asks_closed: None, answers: None, inform: Vec::new(), typed: None, cleaned_from: None, split_from: None, on: Vec::new(), also: Vec::new(), would_close: None, taken_back: false, joins: None, passes: None, comments: Vec::new(), into_turn: Vec::new(), kind: None, answering: Vec::new() });
            cx.effect(Effect::Changed { t: *t });
        }
        if !again.is_empty() {
            cx.effect(Effect::Deliver { to: agent.clone() });
        }
        let twice: Vec<&str> = twice.iter().map(|(_, d)| d.as_str()).collect();
        (true, Ok(format!("{agent} did not read what it was given: {} due again{}.", if again.is_empty() { String::from("nothing") } else { again.join(", ") }, if twice.is_empty() { String::new() } else { format!("; given twice, not read: {}", twice.join(", ")) })))
    }

    /// AGENT's deliveries of posts, given twice and read neither time (`missed`, made due again then and held
    /// back): handed again from now, at its turn's end or its process's end (0.2 A4: "nothing withdrawn or lost";
    /// consult-3 N-818.p76, 2). Handed again and missed again, they wait again: once per turn's end, never a clock.
    fn release_missed(&mut self, cx: &mut Cx, agent: &str) -> Vec<String> {
        let Some(ds) = self.missed.remove(agent) else { return Vec::new() };
        let mut again = Vec::new();
        for (d, (t, _)) in ds {
            let Some(x) = self.tickets.edit().get_mut(&t) else { continue };
            x.unread_twice.remove(&(agent.to_string(), d.clone()));
            cx.effect(Effect::Changed { t });
            again.push(d);
        }
        if !again.is_empty() {
            cx.effect(Effect::Deliver { to: agent.to_string() });
        }
        again
    }

    /// Whether AGENT has posts missed twice waiting for its next turn's end or process's end (`release_missed`).
    pub fn missed(&self, agent: &str) -> bool {
        self.missed.get(agent).is_some_and(|m| !m.is_empty())
    }

    /// Whether delivery D carries only posts AGENT missed twice, held back until its next turn's end (`missed`): posts on
    /// the ticket of one it missed, none after the post that one showed up to that wakes AGENT. Its id is not the missed
    /// one's when coordd's own post (shown to nobody) came after it; a later post that wakes it makes another that
    /// brings them along, and one that only informs it waits with them (research's N-1104.p411, 20 Nov 06:04: it would
    /// have kept the thread from its minder).
    fn missed_waits(&self, agent: &str, d: &Delivery) -> bool {
        let What::Posts { unread } = &d.what else { return false };
        self.missed.get(agent).is_some_and(|m| m.values().any(|(t, upto)| *t == d.t && unread.iter().all(|p| p.n <= *upto || !p.wakes().any(|w| w == agent))))
    }

    /// AGENT's givings since its last turn ended (`given_since`), each the newest giving of its delivery to AGENT: what
    /// rule 19 reads to tell a giving still queued, delivered, or lost (0.2 A4).
    pub fn given_rows(&self, agent: &str) -> Vec<GivenRec> {
        let Some(g) = self.given_since.get(agent) else { return Vec::new() };
        g.iter().filter_map(|d| parse_delivery(d).and_then(|(t, _)| self.tickets.get(&t)).and_then(|x| x.given.iter().rev().find(|r| r.to == agent && r.delivery == *d)).cloned()).collect()
    }

    /// The step AGENT's turn answers: the one delivered in that turn, else the oldest taken step it owes.
    /// The ticket a turn of AGENT would be replied on (the rule Reply), if it is given MORE besides what it was given
    /// since its last turn ended: every delivery is posts (`T.pN`; not a step, a reminder, a refusal or a closing
    /// notice) on one ticket T. Posts on T given in several deliveries are one thing; posts on two tickets, or a step
    /// among them, are several, and nothing is posted for them. The hand-over asks this too, so the footer it writes
    /// (reply by ending your turn, or with coord post) says what the rule will do (paper-research m988).
    /// The deliveries AGENT was given since its last turn ended (or its turn was cut): the one count of what its
    /// turn holds. The rule Reply reads it at the turn's end; coordd's hand-over reads it to hand at most one step
    /// and, while it is not empty, only more on the same tickets (paper-research m995). A step its own act raised is
    /// put into its running turn beside what it holds (delivery.kind's `arrived`), and answered there
    /// (`turn_delivery`).
    pub fn given_since(&self, agent: &str) -> Vec<String> {
        self.given_since.get(agent).map(|g| g.iter().cloned().collect()).unwrap_or_default()
    }

    /// Of `given_since`, the deliveries put into AGENT's turn while it ran (`Line::Given` with `into`).
    pub fn into_since(&self, agent: &str) -> Vec<String> {
        self.into_since.get(agent).map(|g| g.iter().cloned().collect()).unwrap_or_default()
    }

    /// Whether a cut of AGENT's turn now holds a post of the owner's to it, which the cut leaves due again
    /// (`Line::TurnCut`, from the era [`ERA_PUT_IN`]): coordd begins that era just before such a cut, as before the
    /// first giving into a turn. Not for an agent of another project (`P/A`).
    pub fn cut_holds_his(&self, agent: &str) -> bool {
        !agent.contains('/') && self.given_since.get(agent).is_some_and(|g| g.iter().any(|d| !self.his_in(agent, d).is_empty()))
    }

    /// Whether a post put into a turn is given now, in a turn running or carried from one that ended before the era
    /// [`ERA_PUT_IN`]: coordd begins that era at a start then.
    pub fn holds_put_in(&self) -> bool {
        self.into_since.values().chain(self.carried.values()).any(|s| !s.is_empty())
    }

    /// ENDED, given to AGENT, read at AT (a turn's end, or the era [`ERA_PUT_IN`]'s line for what turns before it
    /// carried): each giving's `read`, once (N-818 post 32), shown only under his posts (`TicketView::whereabouts`), so
    /// the view changes when it carried one.
    fn read_given(&mut self, cx: &mut Cx, at: &At, agent: &str, ended: &BTreeSet<String>) {
        for d in ended {
            let Some((t, _)) = parse_delivery(d) else { continue };
            let Some(x) = self.tickets.edit().get_mut(&t) else { continue };
            let Some(g) = x.given.iter_mut().rev().find(|g| g.to == agent && g.delivery == *d) else { continue };
            if g.read.is_some() {
                continue;
            }
            g.read = Some(at.clone());
            let g = g.clone();
            if x.posts.iter().any(|p| p.by == "owner" && Ticket::gave(&g, p.n)) {
                cx.effect(Effect::Changed { t });
            }
        }
    }

    /// The delivery a turn of AGENT ending now began with (`TurnEnded`'s `delivery`), of GIVEN, what it was given less
    /// what came after its last message: of its own deliveries (not put into it while it ran) a step, else the one its
    /// `turn_began` line names (`began`: the one given first in its session, as coordd read it), else the first;
    /// when it holds none of its own, the first put in that is not a step. A step put into a turn is answered there
    /// with `coord answer` (step.tmpl's `put`), or comes again alone (`left_in_turn`), never by that turn's last
    /// message, which is about what the turn began with (consult-3 N-1396 p280). Not the first by its text alone: GIVEN
    /// is sorted as text, and a `deliver … digest` gives its digest's thread a `given` of its own, so a digest's post on
    /// a thread numbered lower named the turn in place of the item it was handed (N-1104 p1160 for six agents whose
    /// item was N-1446 p3020; docket-edge5's audit N-1446 p3263, proto-mind p3271, paper-research p3273). The line's
    /// `delivery` alone changes: the machine reads it as a step, and a step still comes first.
    pub fn turn_delivery(&self, agent: &str, given: &[String]) -> Option<String> {
        let into = self.into_since.get(agent);
        let (put, own): (Vec<&String>, Vec<&String>) = given.iter().partition(|d| into.is_some_and(|i| i.contains(*d)));
        let began = self.began(agent);
        let own = own.iter().find(|d| step_id(d).is_some()).or_else(|| own.iter().find(|d| began.as_deref() == Some(d.as_str()))).or(own.first());
        own.or_else(|| put.iter().find(|d| step_id(d).is_none())).map(|d| d.to_string())
    }

    /// What AGENT's turn ending now holds for the rule Reply: its deliveries since its last turn ended, not LATER, less
    /// those put into it while it ran (`Line::Given` with `into`, N-101 p1214). When it holds nothing else (it began
    /// with a delivery put in at its end, or with none), the ones put in that it has not answered on their thread; a
    /// step put in is not among them, as its last message does not answer it (`turn_delivery`, `left_in_turn`).
    fn turn_given(&self, agent: &str, later: &[String]) -> BTreeSet<String> {
        let into = self.into_since.get(agent);
        let (put, own): (BTreeSet<String>, BTreeSet<String>) = self.given_since.get(agent).into_iter().flatten().filter(|d| !later.contains(*d)).cloned().partition(|d| into.is_some_and(|i| i.contains(d)));
        if !own.is_empty() {
            return own;
        }
        put.into_iter().filter(|d| step_id(d).is_none() && !self.answered_into(agent, d)).collect()
    }

    /// Whether AGENT answered delivery D on its ticket: a post of its own there after what D gave it.
    fn answered_into(&self, agent: &str, d: &str) -> bool {
        self.answered_given(agent, d, false)
    }

    /// `answered_into`; with SINCE, only by what it did once handed D, for the rule Reply's parts (`reply_threads`,
    /// read live alone, so no era): a post of its own there answering a post D gave, or made in a later second than
    /// the giving, or its reply-tool answer from another thread. A post of its own numbered past what D gave but made
    /// before it was handed D, or in the same second, is no answer to D: written about something else, it would send
    /// the turn's words without an address to another thread (proto-mind N-1446 p2631, docket-core p2632).
    fn answered_given(&self, agent: &str, d: &str, since: bool) -> bool {
        let Some(x) = parse_delivery(d).and_then(|(t, _)| self.tickets.get(&t)) else { return false };
        let Some(g) = x.given.iter().rev().find(|g| g.to == agent && g.delivery == d) else { return false };
        let once_handed = |p: &Post| !since || p.answers.is_some_and(|m| Ticket::gave(g, m)) || p.at.get(..19) > g.at.get(..19);
        x.posts.iter().any(|p| p.by == agent && p.n > Ticket::reach(g) && !p.dropped && once_handed(p)) || x.answered_from.iter().any(|(m, by, _)| by == agent && Ticket::gave(g, *m))
    }

    /// AGENT's newest giving of delivery D: its ticket (`delivered_post`: his post told late too), and its row, whose
    /// posts are those it carried (`Ticket::gave`).
    fn giving_of(&self, agent: &str, d: &str) -> Option<(&Ticket, &GivenRec)> {
        let x = delivered_post(d).and_then(|(t, _)| self.tickets.get(&t))?;
        let g = x.given.iter().rev().find(|g| g.to == agent && g.delivery == d)?;
        Some((x, g))
    }

    /// Whether post P of T is the owner's to AGENT that a turn of it must answer: his, shown, waking it, and no `@all`
    /// of his that another agent claimed (`all_state`). That one is settled for the rest, read at their turn's end or
    /// cut as an agent's post is: `taken` refuses their answer to it (consult-3 N-412.p2203, 1).
    fn his_post(&self, t: u64, p: &Post, agent: &str) -> bool {
        p.by == "owner" && p.visible() && p.wakes().any(|w| w == agent) && self.all_state(t, p.n).is_none_or(|s| s.claim.is_none_or(|(a, _)| a == agent))
    }

    /// The owner's posts that AGENT's newest giving of delivery D carried that a turn of it must answer (`his_post`):
    /// his posts to it.
    fn his_in(&self, agent: &str, d: &str) -> Vec<u32> {
        let Some((x, g)) = self.giving_of(agent, d) else { return Vec::new() };
        x.posts.iter().filter(|p| Ticket::gave(g, p.n) && self.his_post(x.t, p, agent)).map(|p| p.n).collect()
    }

    /// Whether AGENT's newest giving of delivery D carried an `@all` of his to it that another agent claimed: settled
    /// for AGENT (`his_post`).
    fn claimed_in(&self, agent: &str, d: &str) -> bool {
        let Some((x, g)) = self.giving_of(agent, d) else { return false };
        x.posts.iter().any(|p| Ticket::gave(g, p.n) && p.by == "owner" && p.visible() && p.wakes().any(|w| w == agent) && !self.his_post(x.t, p, agent))
    }

    /// Whether one of HIS, posts of the owner's on delivery D's ticket, was handed to AGENT before in a giving made due
    /// again for being left unanswered in a turn (`GivenRec::left`): compared by post number, so a new id for it (a
    /// later post came on its ticket meanwhile) is the same post.
    fn left_before(&self, agent: &str, d: &str, his: &[u32]) -> bool {
        let Some(x) = delivered_post(d).and_then(|(t, _)| self.tickets.get(&t)) else { return false };
        x.given.iter().any(|g| g.to == agent && g.left && his.iter().any(|n| Ticket::gave(g, *n)))
    }

    /// Whether every one of HIS was handed to AGENT before in a giving left so (`left_before`): delivery D left now is
    /// owed. One of them at its first leaving (a post of his that came on its ticket before it was handed again) brings
    /// it again, the others with it, so each post of his comes again once (consult-3 N-412.p2203, 2).
    fn left_all_before(&self, agent: &str, d: &str, his: &[u32]) -> bool {
        !his.is_empty() && his.iter().all(|n| self.left_before(agent, d, std::slice::from_ref(n)))
    }

    /// Whether delivery D, given to AGENT in its turn ending now, carries his post to it put into that turn (INTO) or
    /// handed again for being left so (`left_before`): read only once answered (`his_at_end`).
    fn his_in_turn(&self, agent: &str, d: &str, into: &BTreeSet<String>) -> bool {
        let his = self.his_in(agent, d);
        !his.is_empty() && (into.contains(d) || self.left_before(agent, d, &his))
    }

    /// HIS: of what AGENT's turn ending at AT held, the deliveries carrying his post to it put into the turn, or
    /// handed again for being left so (`his_in_turn`), decided once the turn's last message is posted (the rule Reply);
    /// or, at a turn cut short, every delivery it held carrying his post to it (`Line::TurnCut`, p2202, 2).
    /// One it answered is read: a post of its own on his thread after the delivery, compared by post number, never by
    /// time (that message, the reply tool's, or `coord post`), or the reply tool's answer to it from another thread
    /// (`answered_into`). One put in and left unanswered is made due again, once (`hand_his_again`). One left so again,
    /// every post of his in it left before (`left_all_before`), is owed (`owed`): not handed a third time, not read,
    /// out of `given_since`, so the next post for AGENT raises `free` (paper-research N-412.p2194, p2197, 1 and 2;
    /// consult-3 p2198). And what AGENT owed and has answered since, or an `@all` of his another agent claimed since,
    /// is read and owed no more. Whether any of this was so, so the line is kept and replays the same.
    fn his_at_end(&mut self, cx: &mut Cx, at: &At, agent: &str, his: &BTreeSet<String>) -> bool {
        // answered, or an `@all` of his another agent claimed since (`his_post`): owed no more
        let paid: BTreeSet<String> = self.owed.get(agent).map(|o| o.keys().filter(|d| self.answered_into(agent, d) || self.his_in(agent, d).is_empty()).cloned().collect()).unwrap_or_default();
        if let Some(o) = self.owed.get_mut(agent) {
            o.retain(|d, _| !paid.contains(d));
            if o.is_empty() {
                self.owed.remove(agent);
            }
        }
        self.read_given(cx, at, agent, &paid);
        let (answered, left): (BTreeSet<String>, BTreeSet<String>) = his.iter().cloned().partition(|d| self.answered_into(agent, d));
        self.read_given(cx, at, agent, &answered);
        let (twice, once): (BTreeSet<String>, BTreeSet<String>) = left.into_iter().partition(|d| self.left_all_before(agent, d, &self.his_in(agent, d)));
        for d in twice {
            let Some((t, upto)) = delivered_post(&d).and_then(|(t, _)| self.tickets.get(&t)?.given.iter().rev().find(|g| g.to == agent && g.delivery == d).map(|g| (t, g.upto))) else { continue };
            self.owed.entry(agent.to_string()).or_default().insert(d, (t, upto));
            cx.effect(Effect::Changed { t });
        }
        self.hand_his_again(cx, agent, &once);
        !his.is_empty() || !paid.is_empty()
    }

    /// AGENT's deliveries DS, each carrying his post to it into a turn that ended without answering it, or was cut
    /// (`his_at_end`), or over the era [`ERA_PUT_IN`]'s line, made due again, once: what Given did is undone, as `unread_given` does,
    /// so the next `free` hands his post first and alone, said to be handed again (`GivenRec::left`, `handed_before`).
    /// A later delivery on its ticket, read, moved the read mark past it: put back below it all the same, and what
    /// came after it comes with it. Not for his post told late (`N-5.p3.told`): it comes again by its own id, its
    /// `told_late` row's `given` undone, and its giving moved no mark, so a later delivery's posts past it stay read
    /// (consult-3 N-1446 p733).
    fn hand_his_again(&mut self, cx: &mut Cx, agent: &str, ds: &BTreeSet<String>) {
        for d in ds {
            let Some((t, suffix)) = parse_delivery(d) else { continue };
            let Some(x) = self.tickets.edit().get_mut(&t) else { continue };
            let Some(g) = x.given.iter_mut().rev().find(|g| g.to == agent && g.delivery == *d) else { continue };
            (g.left, g.read) = (true, None);
            let (upto, before) = (g.upto, g.before);
            undo_given(x, agent, suffix, upto, before, self.era);
            if told_post(d).is_none() && x.mu.get(agent).is_some_and(|m| *m > before) {
                x.mu.insert(agent.to_string(), before);
            }
            cx.effect(Effect::Changed { t });
        }
        if !ds.is_empty() {
            cx.effect(Effect::Deliver { to: agent.to_string() });
        }
    }

    /// The owner's posts AGENT owes an answer to (`owed`: handed again for being left unanswered in a turn, and left
    /// so again) and has not answered since (`answered_into`), as post ids (`N-5.p3`), in ticket and post order:
    /// named at the foot of each of its deliveries until it answers (paper-research N-412.p2197, 1), or until an agent
    /// it asks or was handed to answers it (`his_answered`, the ledger's rule; paper-research N-1446 p2348, p2355).
    pub fn owed_posts(&self, agent: &str) -> Vec<String> {
        self.owed_his(agent).into_iter().filter(|(t, n)| !self.his_answered(*t, *n)).map(|(t, n)| format!("{}.p{n}", self.id(t))).collect()
    }

    /// Of what AGENT owes (`owed_his`), the posts of his an agent they ask or were handed to answered (`his_answered`), so
    /// that `owed_posts` names them no more, as post ids: what a record's replay counts (paper-research N-1446 p2348).
    pub fn owed_answered(&self, agent: &str) -> Vec<String> {
        self.owed_his(agent).into_iter().filter(|(t, n)| self.his_answered(*t, *n)).map(|(t, n)| format!("{}.p{n}", self.id(t))).collect()
    }

    /// The owner's posts AGENT owes (`owed`) and has not answered since (`answered_into`), as (ticket, post), in that
    /// order, each once.
    fn owed_his(&self, agent: &str) -> Vec<(u64, u32)> {
        let Some(o) = self.owed.get(agent) else { return Vec::new() };
        let mut out: Vec<(u64, u32)> = o.iter().filter(|(d, _)| !self.answered_into(agent, d)).flat_map(|(d, (t, _))| self.his_in(agent, d).into_iter().map(move |n| (*t, n))).collect();
        out.sort();
        out.dedup();
        out
    }

    /// Of delivery D for AGENT, put into its running turn (INTO) or not, the owner's posts that turn must answer or have
    /// again (`his_at_end`), as post ids (`N-5.p3`): his posts to it that D carries, when D goes into a running turn or
    /// hands again one of his left unanswered in a turn (`left_before`, made so only in the era [`ERA_PUT_IN`], which
    /// coordd begins just before the first giving into a turn). The reply tool's Nothing is no answer to them
    /// (channel.py; paper-research N-412.p2202, 1); to the rest it says read.
    pub fn his_to_answer(&self, agent: &str, d: &Delivery, into: bool) -> Vec<String> {
        let What::Posts { unread } = &d.what else { return Vec::new() };
        let his: Vec<u32> = unread.iter().filter(|p| self.his_post(d.t, p, agent)).map(|p| p.n).collect();
        if his.is_empty() || !(into || self.left_before(agent, &d.id, &his)) {
            return Vec::new();
        }
        his.into_iter().map(|n| format!("{}.p{n}", d.ticket)).collect()
    }

    /// Whether AGENT's giving of delivery D on ticket T up to UPTO is the one it owes an answer to (`owed`): the line
    /// under his post says `not answered` (`ViewGiven::owed`). Not once each post of his in it was answered by an agent
    /// it asks or was handed to (`his_answered`), as `owed_posts` names it no more.
    pub fn owes_giving(&self, agent: &str, d: &str, t: u64, upto: u32) -> bool {
        self.owed.get(agent).is_some_and(|o| o.get(d) == Some(&(t, upto))) && self.his_in(agent, d).iter().any(|n| !self.his_answered(t, *n))
    }

    /// Whether DELIVERY was handed to AGENT before and not read (made due again by `unread`, or for carrying his post
    /// into a turn that left it unanswered, `GivenRec::left`): its text says it is handed a second time. A delivery of
    /// posts that carries his post so left is that, compared by post number as `left_before` is: a post on its ticket
    /// before it was handed again gave it a new id (consult-3 N-412.p2203, 2).
    pub fn handed_before(&self, agent: &str, delivery: &str) -> bool {
        let Some((t, suffix)) = parse_delivery(delivery) else { return false };
        let Some(x) = self.tickets.get(&t) else { return false };
        if x.given.iter().any(|g| g.to == agent && g.delivery == delivery && g.again()) {
            return true;
        }
        let Some(upto) = suffix.strip_prefix('p').and_then(|u| u.parse::<u32>().ok()) else { return false };
        let his: Vec<u32> = x.posts_unread(agent).iter().filter(|p| p.n <= upto && self.his_post(t, p, agent)).map(|p| p.n).collect();
        self.left_before(agent, delivery, &his)
    }

    /// Whether a turn of AGENT that began with DELIVERY, ending now, answers a step (its TurnEnded forms that step's
    /// answer) rather than being read by the rule Reply.
    pub fn turn_answers_step(&self, agent: &str, delivery: Option<&str>, later: &[String], stood: Option<&str>) -> bool {
        self.answered_by_turn(agent, delivery, later, stood, self.into_since.get(agent).unwrap_or(&BTreeSet::new())).is_some()
    }

    /// Whom the rule Reply would name for AGENT's turn ending now, on which ticket: None when it would post nothing
    /// there (not one ticket's posts, or it posted there itself after them). Names: the authors of the posts told to
    /// it there (C), an automatic reply's too (post 5: a chain ends with Nothing, not by the rule); only who can
    /// read a reply (`readers`). Empty: nobody can, and coordd posts nothing (an empty text), nor promises a reply.
    pub fn reply_names(&self, agent: &str, later: &[String]) -> Option<(u64, Vec<String>)> {
        self.given_since.get(agent)?;
        let given = self.turn_given(agent, later);
        let informs = self.informs_only(agent, &given);
        let told: BTreeSet<String> = given.difference(&informs).cloned().collect();
        self.reply_names_of(agent, &told).map(|(t, names, ..)| (t, names))
    }

    /// Of GIVEN, AGENT's deliveries, those of posts that only informed it (none of the posts it was handed in it
    /// tells or names it), when another one told it something: they do not make its turn "several" (N-27 point 3c).
    /// A post's delivery (`N-1.p5`, read by `post_id`) or a take-back's notice (`N-1.p5.taken_back`, whose post is
    /// struck) can; his post told late (`N-1.p5.told`) never does; a step, words or a notice of another kind cannot.
    /// Empty when every one only informed it.
    pub fn informs_only(&self, agent: &str, given: &BTreeSet<String>) -> BTreeSet<String> {
        let only = |d: &String| -> bool {
            // his post told late tells, never only informs (proto-lead N-1446 p576)
            if told_post(d).is_some() {
                return false;
            }
            let posts = post_of(d);
            let Some((t, _)) = post_id(posts) else { return false };
            let Some(x) = self.tickets.get(&t) else { return false };
            let Some(g) = x.given.iter().rev().find(|g| g.to == agent && g.delivery == *d) else { return false };
            !x.posts.iter().any(|p| p.n > g.before && p.n <= g.upto && p.by != agent && p.visible() && p.wakes().any(|w| w == agent))
        };
        let out: BTreeSet<String> = given.iter().filter(|d| only(d)).cloned().collect();
        if out.len() == given.len() { BTreeSet::new() } else { out }
    }

    /// Whether what AGENT's turn ending now holds for the rule Reply shows it nothing: only deliveries of posts, and in
    /// none of them a post of another's that is shown (`Post::visible`). Such a turn was handed a notice of coordd's
    /// before the install that keeps those in the journal (paper-research N-412.p1844, 2), and its last message is
    /// posted nowhere, as for Nothing (consult-3 N-412.p1982, 2). coordd asks it live; a record's TurnEnded line holds
    /// the text it was given, so the replay reads what happened.
    pub fn turn_shows_nothing(&self, agent: &str, later: &[String]) -> bool {
        let given = self.turn_given(agent, later);
        !given.is_empty()
            && given.iter().all(|d| {
                let Some((t, _)) = post_id(d) else { return false };
                let Some(x) = self.tickets.get(&t) else { return false };
                let Some(g) = x.given.iter().rev().find(|g| g.to == agent && g.delivery == *d) else { return false };
                !x.posts.iter().any(|p| p.n > g.before && p.n <= g.upto && p.by != agent && p.visible())
            })
    }

    /// What an agents' thread T waits on (design part 9b, N-27 point 3d): each agent told a post there (named or told,
    /// not the owner, who has no turns, nor one that cannot read) that has not ended a turn handed it, nor posted there
    /// since; with the last such post. Derived from the posts, the hand-overs and the turn ends; no line of its own.
    pub fn waits_between(&self, t: u64) -> Vec<(String, u32)> {
        let Some(x) = self.tickets.get(&t).filter(|x| x.between()) else { return Vec::new() };
        let mut told: Vec<(String, u32)> = Vec::new();
        for p in x.posts.iter().filter(|p| p.visible()) {
            for a in p.wakes().filter(|a| **a != p.by && *a != "owner") {
                match told.iter_mut().find(|(b, _)| b == a) {
                    Some(e) => e.1 = p.n,
                    None => told.push((a.clone(), p.n)),
                }
            }
        }
        told.into_iter()
            .filter(|(a, n)| {
                let answered = x.posts.iter().any(|p| p.by == *a && p.n > *n && !p.dropped && x.answers(p, *n));
                let readable = self.readers.as_ref().is_none_or(|r| r.contains(a));
                // handed and its turn not ended yet (the delivery still counted), or not handed yet (`Ticket::hands`)
                let handed = self.given_since.get(a).is_some_and(|g| g.iter().any(|d| x.given.iter().any(|r| r.to == *a && r.delivery == *d && Ticket::hands(r, *n))));
                let ended = x.given.iter().any(|r| r.to == *a && Ticket::hands(r, *n)) && !handed;
                readable && !answered && !ended
            })
            .collect()
    }

    /// Whom a post by AGENT on T that names nobody reaches, by hand or by the rule Reply (design part 9b, N-27 point
    /// 3c): the authors of the posts told to AGENT there, up to post UPTO, that it has not answered (posted there
    /// since), once each, in order; only who can read it (`readers`). The owner, when he takes part, and every other
    /// agent on T hear of it by policy (notify.eflint).
    pub fn unanswered(&self, t: u64, agent: &str, upto: u32) -> Vec<String> {
        self.unanswered_by(t, agent, upto, None)
    }

    /// [`Machine::unanswered`]; for the rule Reply of a turn handed T's posts after BEFORE (TURN), a reply-tool answer
    /// to a post put into the turn does not answer the posts the turn holds (`Ticket::answers_for_turn`), so their
    /// authors are named; and of its posts there, AGENT's own answers one only by the ledger's rule (delivery.rs
    /// `closes`; paper-research N-1479 p136): a post that only passes it on to another ("@accounts-2 …"), or is not
    /// shown where it was written, leaves its author named.
    fn unanswered_by(&self, t: u64, agent: &str, upto: u32, turn: Option<u32>) -> Vec<String> {
        let Some(x) = self.tickets.get(&t) else { return Vec::new() };
        // answered: up to what its posts there had been handed when written, not every post numbered below them
        // (and not kept from it then: 0.2, `keep`)
        let own: Vec<&Post> = x.posts.iter().filter(|p| p.by == agent && !p.dropped).collect();
        // a post kept from it is not one it answers now: its author is not named for it; nor one it was not handed that
        // its mark passed (`Ticket::passed_unhanded`)
        let mut kept = self.kept_posts(agent, t, upto);
        kept.extend(x.passed_unhanded(agent, upto));
        let mut out: Vec<String> = Vec::new();
        let answered = |q: &Post, p: &Post| match turn {
            Some(before) => x.answers_for_turn(q, p.n, upto) && (p.n <= before || delivery::closes(x, p, q, self.era >= ERA_QUIET)),
            None => x.answers(q, p.n),
        };
        // his post handed to a copy (`deliver_to_copy`, a `ToldLate`) is told to the copy, though it wakes its agent:
        // the copy's answer names him (paper-research N-1446 p3020, 1)
        let copy = self.copy_owner(agent).is_some();
        let told = |p: &Post| p.wakes().any(|w| w == agent) || (copy && x.late_for(agent, p.n));
        for p in x.posts.iter().filter(|p| !own.iter().any(|q| answered(q, p)) && !kept.contains(&p.n) && p.n <= upto && p.by != agent && p.visible() && told(p)) {
            if !out.contains(&p.by) && self.readers.as_ref().is_none_or(|r| r.contains(&p.by)) {
                out.push(p.by.clone());
            }
        }
        out
    }

    /// Of the turn handed GIVEN, the ticket the rule Reply posts on, whom it names, the last post it holds there (UPTO),
    /// and AGENT's read mark there before it was handed them (BEFORE): None when its own posts answered them.
    fn reply_names_of(&self, agent: &str, given: &BTreeSet<String>) -> Option<(u64, Vec<String>, u32, u32)> {
        let t = Self::one_ticket(given.iter())?;
        let x = self.tickets.get(&t)?;
        // up to his post told late too, when later (`Ticket::reach`)
        let upto = x.given.iter().filter(|g| g.to == agent && given.contains(&g.delivery)).map(Ticket::reach).max()?;
        let before = x.given.iter().filter(|g| g.to == agent && given.contains(&g.delivery)).map(|g| g.before).min().unwrap_or(0);
        // answered already: a post of its own written with all of it handed (by hand after the delivery); not its
        // turn before's reply numbered after it, which answered what that turn held (N-27 post 43); nor its reply-tool
        // answer to a post put into this turn (`Ticket::answers_for_turn`), unless one of the tool's further ids here
        // is of this turn's (consult-3, N-1397 p58: a `to` of "p180, p179"). And its posts answered what the turn held
        // by the ledger's rule (`unanswered_by`): a hand-off does not swallow the turn's last message to him
        // (paper-research N-1479 p136), which goes to whom it left unanswered. Replayed as recorded: a Replied line's
        // names are the record's, and where it has none the Reply posts none (`reply`); live, where the rule before
        // counts the turn answered, the message is written first as the agent's own post (`kept_reply`).
        let own_after = x.posts.iter().any(|p| p.by == agent && p.n > upto && (x.answers_for_turn(p, upto, upto) || x.further_in_turn(p, before, upto)));
        let unanswered = self.unanswered_by(t, agent, upto, Some(before));
        if own_after && unanswered.is_empty() {
            return None;
        }
        // or answered from another ticket with the reply tool's `to` list (N-101 p1361)
        if x.answered_from.iter().any(|(m, by, _)| by == agent && *m == upto) {
            return None;
        }
        // the one rule for a post that names nobody (N-27 point 3c; C of N-17 post 7 narrowed in point 1): the
        // authors of the posts told to it there that it has not answered; a post it was only informed of names nobody
        x.posts.iter().filter(|p| p.n <= upto && p.by != agent).last()?;
        Some((t, unanswered, upto, before))
    }

    /// The ticket AGENT's turn would be replied on if it is handed MORE now besides what it was given: what only
    /// informs it does not make it several (N-27 point 3c), unless that is all there is.
    pub fn reply_ticket(&self, agent: &str, more: &[String]) -> Option<u64> {
        let given = self.turn_given(agent, &[]);
        let informs = self.informs_only(agent, &given);
        let due = self.due(agent);
        let inform_now = |d: &String| due.iter().any(|x| x.id == *d && x.inform);
        let told: Vec<&String> = given.iter().filter(|d| !informs.contains(*d)).chain(more.iter().filter(|d| !inform_now(d))).collect();
        if told.is_empty() {
            return Self::one_ticket(given.iter().chain(more));
        }
        Self::one_ticket(told.into_iter())
    }

    /// The threads AGENT's turn ending now was given posts on, when what told it something spans more than one (the
    /// rule Reply then posts nothing, N-807): in the order they were handed (the place in its session, else the time),
    /// each with the last post given there, the authors given, whether it told the agent something, and whether the
    /// agent answered it by hand once handed it (`answered_given`). None when the turn holds one ticket's posts (the
    /// rule Reply's case, as before), or anything but posts (a step, words). Each delivery is read by [`delivered_post`] of its [`post_of`]: a post's
    /// delivery, his post told late, and a take-back's notice, read on its ticket as `informs_only` reads it; a notice
    /// carries no post, so it makes no thread of its own, and a turn that holds one is parted as any other (proto-lead
    /// N-1397 p143: such a turn on several threads lost its words without an address; p146). ONE: also when
    /// it holds one ticket's posts, or only posts that inform it: its last message has a part for a thread it was not
    /// handed, and coordd posts it in parts, each where it goes (paper-research N-412.p2208).
    pub fn reply_threads(&self, agent: &str, later: &[String], one: bool) -> Option<Vec<ReplyThread>> {
        self.given_since.get(agent)?;
        let given = self.turn_given(agent, later);
        let informs = self.informs_only(agent, &given);
        let told: BTreeSet<String> = given.difference(&informs).cloned().collect();
        if !one && (told.is_empty() || Self::one_ticket(told.iter()).is_some()) {
            return None;
        }
        // each ticket once: the earliest hand-over's place, the posts of all its deliveries
        // and his posts told late there (`N-1.p5.told`), each its delivery's one post
        let mut by_t: BTreeMap<u64, ((At, u64), u32, u32, bool, bool, Vec<u32>)> = BTreeMap::new();
        for d in &given {
            let (t, n) = delivered_post(post_of(d))?;
            let x = self.tickets.get(&t)?;
            let g = x.given.iter().rev().find(|g| g.to == agent && g.delivery == *d)?;
            let key = (g.at.clone(), g.place.as_ref().map_or(0, |p| p.offset));
            let late = told_post(d).is_some();
            let e = by_t.entry(t).or_insert((key.clone(), u32::MAX, 0, false, false, Vec::new()));
            e.0 = e.0.clone().min(key);
            if late {
                e.5.push(n);
            } else {
                e.1 = e.1.min(g.before);
                e.2 = e.2.max(g.upto);
            }
            e.3 |= told.contains(d);
            e.4 |= self.answered_given(agent, d, true);
        }
        let mut out: Vec<((At, u64), ReplyThread)> = Vec::new();
        for (t, (key, before, upto, told, answered, late)) in by_t {
            let x = &self.tickets[&t];
            let mut authors: Vec<(String, u32)> = Vec::new();
            for p in x.posts.iter().filter(|p| (p.n > before && p.n <= upto || late.contains(&p.n)) && p.by != agent && p.visible()) {
                match authors.iter_mut().find(|(a, _)| *a == p.by) {
                    Some(e) => e.1 = p.n,
                    None => authors.push((p.by.clone(), p.n)),
                }
            }
            let Some(last) = authors.iter().map(|(_, n)| *n).max() else { continue };
            out.push((key, ReplyThread { t, id: self.id(t), last, authors, told, answered, all: self.all_held(t, agent, upto.max(last)) }));
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        (out.len() > 1 || one && !out.is_empty()).then(|| out.into_iter().map(|(_, r)| r).collect())
    }

    fn one_ticket<'a>(ds: impl Iterator<Item = &'a String>) -> Option<u64> {
        let mut t = None;
        for d in ds {
            // a post's delivery, or his post told late (`N-1.p5.told`)
            let (u, _) = delivered_post(d)?;
            if t.is_some_and(|t| t != u) {
                return None;
            }
            t = Some(u);
        }
        t
    }

    /// The rule Reply (paper-research m952, m961, m988): AGENT's turn was given, since its last turn ended (GIVEN),
    /// only posts on one ticket T (`reply_ticket`); it answered no step, said something (TEXT), and did not post on T
    /// itself after what it was given. Then its last message is posted on T as its own, by reference, naming the
    /// author of the last post it was given; or nobody, when that post was itself such a reply (so two agents never
    /// thank each other forever). No model, no policy: a line. NAMED: whom the message names with `@` (coordd's
    /// `reply_at_names`), named too, after those, as a post made with `coord post` names them (N-581.p82): an order to
    /// a third agent in a reply wakes it. Nothing new for the loop: the named agent's own reply comes back to the
    /// writer as after a `coord post` (that post is unanswered there), and Nothing ends the exchange.
    fn reply(&mut self, cx: &mut Cx, agent: &str, given: &BTreeSet<String>, text: &Loc, named: &[String]) -> Option<String> {
        if text.len == 0 {
            // nothing to post (the turn said Nothing): read, no reply, up to the last post it was given (N-19)
            let t = Self::one_ticket(given.iter())?;
            let x = self.tickets.edit().get_mut(&t)?;
            let upto = x.given.iter().filter(|g| g.to == agent && given.contains(&g.delivery)).map(Ticket::reach).max()?;
            if !x.quiet.iter().any(|(a, n)| a == agent && *n == upto) {
                x.quiet.push((agent.into(), upto));
                cx.effect(Effect::Changed { t });
            }
            return None;
        }
        let (t, mut names, upto, _) = self.reply_names_of(agent, given)?;
        // the Reply's own names, but those its message typed too: it answers them and asks them nothing (N-1481 p43)
        let mut answering: Vec<String> = names.iter().filter(|a| !named.contains(a)).cloned().collect();
        for a in named {
            if a != agent && !names.contains(a) && self.readers.as_ref().is_none_or(|r| r.contains(a)) {
                names.push(a.clone());
            }
        }
        let id = self.id(t);
        let n = self.tickets[&t].last() + 1;
        // on replay a log written before N-27 post 43 has no reply where its rule posted none: as it was
        if cx.recorded.as_ref().is_some_and(|rec| !rec.iter().any(|l| matches!(l, Line::Replied { t: u, n: m, .. } if *u == t && *m == n))) {
            return None;
        }
        // on replay the log is what happened: the names its Replied line has, whatever today's rule says (the rule
        // named nobody after an automatic reply before post 5)
        if let Some(rec) = cx.recorded.as_ref() {
            if let Some(Line::Replied { names: r, answering: a, .. }) = rec.iter().find(|l| matches!(l, Line::Replied { t: u, n: m, .. } if *u == t && *m == n)) {
                names = r.clone();
                // and its answering: none on a line from before it, whose names all ask (N-1481 p42)
                answering = a.clone();
            }
        }
        // the owner's `@all` handed to AGENT, claimed by nobody, among what its turn was given: its last message is its
        // answer and so the claim, as the reply tool's would be (consult-lead N-412 post 1900: every answer is a claim).
        // Derived as the post is made, so a replay makes it the same; no line says it. Only in the record's era of it
        // (ERA_CLAIMS): a reply from before answered nothing, and is drawn so still (consult-3, N-807 post 118).
        let claims = if self.era >= ERA_CLAIMS { self.all_held(t, agent, upto) } else { None };
        let mut kept = self.kept_posts(agent, t, upto);
        kept.extend(self.tickets[&t].not_answered_by(agent, upto));
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.answered_upto.insert(n, upto);
        // what was kept from it is not answered by it (0.2, delivery.rs), nor what it was not handed
        // (`Ticket::not_answered_by`)
        if !kept.is_empty() {
            x.not_answered.insert(n, kept);
        }
        x.posts.push(Post { n, at: cx.now.clone(), by: agent.into(), text: Text::Loc { loc: text.clone() }, head: None, step: None, names: names.clone(), told: Vec::new(), auto: true, moved: None, from: None, held: None, halted: false, dropped: false, routed: false, asks: Vec::new(), asks_open: false, asks_closed: None, answers: claims, inform: Vec::new(), typed: None, cleaned_from: None, split_from: None, on: Vec::new(), also: Vec::new(), would_close: None, taken_back: false, joins: None, passes: None, comments: Vec::new(), into_turn: Vec::new(), kind: None, answering: answering.clone() });
        x.read_own(agent, n, self.era, &mut self.passed_his);
        cx.line(Line::Replied { at: cx.now.clone(), t, n, by: agent.into(), names: names.clone(), answering });
        self.halt_post(cx, t, n);
        for a in &names {
            cx.effect(Effect::Deliver { to: a.clone() });
        }
        // a post like any other: policy says who else hears of it (paper-research m1240)
        cx.effect(Effect::Notify { t, event: "post".into(), actor: agent.into(), post: Some(n) });
        cx.effect(Effect::Changed { t });
        // a copy's last message waits for its agent whole (`copy_posted`, N-1104 p1159)
        self.copy_posted(cx, t, n);
        if let Some(w) = self.agent_landed(cx, t, n) {
            self.drive(cx, vec![w]);
        }
        Some(format!("Replied on {id} as post {n}."))
    }

    /// A turn given words (a gift, T.wN) keeps where its answer is on T, so that a one-off made a thread later can
    /// carry it. Not a post: nothing is said on T.
    fn answered_gifts(&mut self, cx: &mut Cx, agent: &str, given: &BTreeSet<String>, text: &Loc) {
        if text.len == 0 {
            return;
        }
        for d in given {
            let Some((t, rest)) = parse_delivery(d) else { continue };
            if !rest.strip_prefix('w').is_some_and(|n| n.parse::<u32>().is_ok()) {
                continue;
            }
            if let Some(x) = self.tickets.edit().get_mut(&t) {
                x.answers.push((agent.to_string(), text.clone()));
                cx.effect(Effect::Changed { t });
            }
        }
    }

    /// PUT: what was put into the turn while it ran (`into_since`). A step put into a turn, whatever put it there (the
    /// agent's own act, or `free`'s `next` handing as a turn began), is answered there with `coord answer`, or by a
    /// turn of its own, never as the oldest taken step: not while it was put into this turn (its own deliveries have
    /// the turn's last message), nor while it is to come again for being left (`RemindWhy::Left`, a turn it came into
    /// ended or was cut without its answer), so no turn about something else answers it (consult-3 N-1396 p280, p300).
    fn answered_by_turn(&self, agent: &str, delivery: Option<&str>, later: &[String], stood: Option<&str>, put: &BTreeSet<String>) -> Option<(u64, u32)> {
        // a step whose holder's ask on it is still unanswered is not answered by a turn's end: the answer comes first;
        // nor one handed after the turn's last message was written (LATER, N-17 post 10)
        let asking = |t: u64| self.asks_on(agent, t);
        // of the step itself, or its reminder, give-back or refusal (`step_id`; `s1` is not `s12`)
        let late = |t: u64, step: u32| later.iter().any(|d| step_id(d).is_some_and(|(u, s, _)| u == t && s == step));
        // where the agent stood (N-101 p414): a step in that layer or in none; another layer's by `coord answer`
        let here = |t: u64| stood.is_none_or(|s| self.layer_of(t).is_none_or(|l| l == s));
        let open = |x: &Ticket| match &x.status {
            Status::Awaits { agent: a, owed, taken: true, forming: None, .. } if a == agent && !asking(x.t) && !late(x.t, owed.step) && here(x.t) => Some(owed.step),
            _ => None,
        };
        if let Some((t, s, _)) = delivery.and_then(step_id) {
            if self.tickets.get(&t).and_then(open) == Some(s) {
                return Some((t, s));
            }
        }
        let put_in = |x: &Ticket| put.iter().any(|d| step_id(d).is_some_and(|(t, _, _)| t == x.t));
        let mut best: Option<(&At, u64, u32)> = None;
        for x in self.tickets.values() {
            if let (Some(step), Status::Awaits { since, reminded, .. }) = (open(x), &x.status) {
                let left = *reminded == Some(false) && matches!(&x.remind_why, Some((s, RemindWhy::Left)) if *s == step);
                if put_in(x) || left {
                    continue;
                }
                if best.is_none_or(|(s, _, _)| since < s) {
                    best = Some((since, x.t, step));
                }
            }
        }
        best.map(|(_, t, s)| (t, s))
    }

    /// AGENT's turn ended, or was cut (`Line::TurnCut`), with a step still owed that was put into it (INTO, not handed
    /// after its last message, LATER) and taken there, whatever put it there, and the turn's last message answering
    /// another (ANSWERED) or nothing: the step comes again, once, as its reminder (`.again`, [`RemindWhy::Left`] its
    /// why), so a turn of its own answers it (a step its own act raised, alone at `free`); never the oldest taken step
    /// of a turn about something else (`answered_by_turn`, consult-3 N-1396 p280, p300).
    fn left_in_turn(&mut self, cx: &mut Cx, at: &At, agent: &str, into: &BTreeSet<String>, later: &[String], answered: Option<u64>) {
        for (t, n, _) in into.iter().filter(|d| !later.contains(*d)).filter_map(|d| step_id(d)) {
            if answered == Some(t) {
                continue;
            }
            let Some(x) = self.tickets.get(&t) else { continue };
            if !matches!(&x.status, Status::Awaits { agent: a, owed, taken: true, forming: None, reminded, .. } if a == agent && owed.step == n && *reminded != Some(false)) {
                continue;
            }
            let x = self.tickets.edit().get_mut(&t).unwrap();
            let Status::Awaits { reminded, .. } = &mut x.status else { unreachable!() };
            *reminded = Some(false);
            x.remind_why = Some((n, RemindWhy::Left));
            cx.line(Line::Reminded { at: at.clone(), t, step: n });
            cx.effect(Effect::Changed { t });
            cx.effect(Effect::Deliver { to: agent.to_string() });
        }
    }

    // ---------------------------------------------------------------- rules

    /// A thread or item T opened at a post (`tied_at`: `N-234.p11`, or `N-234` at its last post; paper-research
    /// N-447, piece 2), or any ticket whose kind is drawn (`drawn line`, `drawn mark`, `drawn note`; proto-lead N-1446
    /// p17): stands under that ticket from its open, as a tie makes it (`Line::Tied`). A
    /// place that is not there (another record's id, a post gone) leaves it untied: coordd checked it before.
    fn tie_at_open(&mut self, cx: &mut Cx, t: u64, at: Option<&Value>) {
        let Some((p, n)) = at.and_then(Value::as_str).and_then(|a| self.place_of(a)) else { return };
        // a line or a mark (`view::stands`): what its kind says, decided before the edit
        if p == t || !self.tickets.get(&t).is_some_and(|x| crate::view::stands(self, x)) {
            return;
        }
        let Some(x) = self.tickets.edit().get_mut(&t) else { return };
        x.tie = Some(Some((p, n)));
        cx.effect(Effect::Changed { t: p });
    }

    /// The place ID names for a thread to stand at (`post_or_ticket`, its ticket by `ticket_named`): a post
    /// `N-234.p11` (ticket and post there), or a ticket `N-234` at its last post; None when either is not there.
    pub fn place_of(&self, id: &str) -> Option<(u64, u32)> {
        let (t, n) = post_or_ticket(id)?;
        let t = self.ticket_named(t)?;
        match n {
            Some(n) => self.tickets[&t].posts.iter().any(|p| p.n == n).then_some((t, n)),
            None => Some((t, self.tickets[&t].posts.last().map_or(0, |p| p.n))),
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// Edit at a hole: PROGRAM takes the hole's place, checked against the kind's result with what was in scope at
    /// the hole, and runs from there. A thread dropped is `return OneOff`; one made a ticket is `open ...`, and so on.
    /// `Line::Ended`: BY ends thread T, HOW `done`, `dropped` or `kept`, with WHY (paper-research N-447, piece 1;
    /// one-thread.md's `Done(why)`, `Dropped(why)`, `Kept`). Resting at its hole (or the hole out with the minder), the
    /// hole is filled with the result, as an edit fills it; an agents' thread (`Between`) keeps its result and is
    /// marked ended. Refused for a thread at work, one that has a result, one ended already, and any other kind. Who
    /// may is coordd's (the policy's `edit`, asked before the line).
    fn end_thread(&mut self, cx: &mut Cx, t: u64, by: &str, how: &str, why: &str, at: &At) -> (bool, Result<String, String>) {
        let no = |e: String| (false, Err(e));
        let Some(x) = self.tickets.get(&t) else { return no(format!("There is no ticket {t}.")) };
        let id = self.id(t);
        if x.kind != "thread" {
            let close = if x.kind == "item" { " (`coord item close` or `coord item drop` ends an item)" } else { "" };
            return no(format!("{id} is a {}, not a thread: only a thread ends so{close}.", x.kind));
        }
        if let Some(e) = &x.ended {
            return no(format!("{id} has ended already ({}, by {}: {}).", e.how, e.by, e.why));
        }
        let why = why.trim();
        if why.is_empty() {
            return no(format!("Ending {id} needs one line saying why."));
        }
        // one line, as a withdrawal's why (`withdraw_asks`), which the end gives the ender's asks
        if why.contains('\n') {
            return no(format!("Ending {id} takes one line saying why, not several."));
        }
        let quoted = format!("\"{}\"", why.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n"));
        let program = match how {
            "done" => format!("return Done({quoted})"),
            "dropped" => format!("return Dropped({quoted})"),
            "kept" => String::from("return Kept"),
            _ => return no(format!("A thread ends done, dropped or kept, not {how}.")),
        };
        // asks to the owner on it (N-447 p2, 3; p5): another's still open refuses the end, by name; the ender's own are
        // withdrawn after the end; the owner's end closes them all as his "nothing to answer"
        if by != "owner" {
            let others = self.others_open_asks(x, by);
            if !others.is_empty() {
                let which: Vec<String> = others.iter().map(|(a, n)| format!("post {n} ({a})")).collect();
                return no(format!("{id} waits on the owner: what {} asks him is open. The owner ends it, or the asker withdraws the asks first.", which.join(", ")));
            }
        }
        let open_asks: Vec<u32> = x.posts.iter().filter(|p| self.asks_open(x, p.n)).map(|p| p.n).collect();
        let ended = crate::view::ViewEnded { how: how.to_string(), why: why.to_string(), by: by.to_string(), at: at.clone() };
        let parent = self.outline_parent(t).map(|(p, _)| p);
        let said = if x.between() {
            format!("{id} has ended ({how}): it leaves the outline.")
        } else if !x.is_open() {
            let head = match &x.status {
                Status::Done(v) => v.get("head").and_then(Value::as_str).unwrap_or("a value").to_string(),
                _ => String::from("a failure"),
            };
            return no(format!("{id} is closed already, with {head}."));
        } else {
            match self.edit_hole(cx, t, by, Some(&program), true) {
                Some((true, Ok(_))) => format!("{id} has ended ({how}): it leaves the outline."),
                Some(r) => return r,
                None => return no(format!("{id} is at work, not resting at its hole: it can be ended when it rests, and `coord cancel {id}` stops it now.")),
            }
        };
        for n in open_asks {
            if by == "owner" {
                let x = self.tickets.edit().get_mut(&t).unwrap();
                if let Some(p) = x.posts.iter_mut().find(|p| p.n == n) {
                    p.asks_open = false;
                }
                x.nothing.insert(n, at.clone());
            } else {
                let _ = self.withdraw_asks(cx, at, t, n, by, why);
            }
        }
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.ended = Some(ended);
        cx.effect(Effect::Changed { t });
        if let Some(p) = parent {
            cx.effect(Effect::Changed { t: p });
        }
        (true, Ok(said))
    }

    /// The edit at T's hole (`coord edit` while it rests, or while the minder has the hole: the minder's step is
    /// withdrawn), the program PROGRAM filling it; None when T is at no hole. WIDEN (an end, `Line::Ended`): a ticket
    /// opened by an event, whose result no ticket reads, gives a result of its kind as it is today, though its own
    /// text is older (an open thread's, from before `Done` and `Dropped`).
    fn edit_hole(&mut self, cx: &mut Cx, t: u64, by: &str, program: Option<&str>, widen: bool) -> Option<(bool, Result<String, String>)> {
        let x = self.tickets.get(&t)?;
        let id = self.id(t);
        if let Status::Rests { env, scope } = &x.status {
            let (env, scope) = (env.clone(), scope.clone());
            return Some(self.fill_hole(cx, t, by, program, env, scope, widen));
        }
        // the hole is out with the minder: the edit fills it, and the minder's step is withdrawn
        let (Some((env, scope)), Status::Awaits { owed, .. } | Status::Asking { owed, .. }) = (&x.shaping, &x.status) else { return None };
        if owed.call != SHAPE {
            return None;
        }
        let (env, scope, step) = (env.clone(), scope.clone(), owed.step);
        let agent = match &x.status {
            Status::Awaits { agent, .. } => Some(agent.clone()),
            _ => None,
        };
        let was = (x.status.clone(), x.shaping.clone());
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.status = Status::Rests { env: env.clone(), scope: scope.clone() };
        x.shaping = None;
        let r = self.fill_hole(cx, t, by, program, env, scope, widen);
        let x = self.tickets.edit().get_mut(&t).unwrap();
        if r.1.is_err() {
            (x.status, x.shaping) = was;
        } else if let Some(a) = agent {
            let d = format!("{id}.s{step}");
            if x.given.iter().any(|g| g.to == a && g.delivery == d) {
                x.withdrawn.push((a.clone(), step, SHAPE.to_string(), false, None));
                cx.effect(Effect::Deliver { to: a });
            }
        }
        Some(r)
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_hole(&mut self, cx: &mut Cx, t: u64, by: &str, program: Option<&str>, env: Env, scope: BTreeMap<String, Type>, widen: bool) -> (bool, Result<String, String>) {
        let no = |e: String| (false, Err(e));
        let id = self.id(t);
        let Some(text) = program.filter(|p| !p.trim().is_empty()) else { return no(String::from("An edit at a hole needs the program that fills it.")) };
        let x = &self.tickets[&t];
        let (mut k, mut bad) = match self.kind_of(x) {
            Ok(k) => (k, None),
            Err(e) if cx.recorded.is_some() => (loose_kind(&x.kind), Some(e)),
            Err(e) => return no(e),
        };
        if widen && x.parent.is_none() && x.under.is_none() {
            if let Some(today) = self.defs.kinds.get(&x.kind) {
                k.result = today.result.clone();
            }
        }
        let mut p = match syntax::parse_prog(text, 1) {
            Ok(p) => p,
            Err(e) => return no(format!("The program does not parse: {e}")),
        };
        if let Err(e) = check::check_in_with(&mut p, &k.result, &scope, &k.raises, &self.calls_at(t), &self.defs.kinds, Some(&self.defs.named), cx.recorded.is_none()).or_else(|e| if cx.recorded.is_some() { bad.get_or_insert(e); Ok(()) } else { Err(e) }) {
            let vars = scope.iter().map(|(n, t)| format!("{n}: {t}")).collect::<Vec<_>>().join(", ");
            return no(format!("The program cannot fill the hole of {id}: {e}. It must give {}; in scope: {}.", k.result, if vars.is_empty() { "nothing" } else { &vars }));
        }
        let more = check::bound(&p, &self.defs.kinds);
        // the frames go, and with them the agents of the `let agent` scopes the hole was in
        let dropped: Vec<String> = self.tickets[&t].k.iter().filter_map(|f| if let Frame::Scope { agent } = f { Some(agent.clone()) } else { None }).collect();
        for a in dropped {
            self.end_kept(cx, t, &a);
        }
        let x = self.tickets.edit().get_mut(&t).unwrap();
        if let Some(c) = x.k.iter().find_map(|f| if let Frame::With { ctx } = f { Some(ctx.clone()) } else { None }) {
            x.ctx = c;
        }
        x.k.clear();
        x.trace.back();
        x.trace.fill(0, Some(by.to_string()), &p);
        x.bound = u64::from(x.steps) + more;
        x.status = Status::Waits { children: Vec::new(), par: false };
        cx.effect(Effect::Changed { t });
        if let Some(why) = &bad {
            self.unchecked(t, why);
        }
        self.drive(cx, vec![Work::Run(t, p, env)]);
        (true, Ok(format!("{id}'s hole filled: it runs the new program (at most {more} more step(s)).")))
    }

    /// Move (the owner's act, and a program's `move`): post N of T goes to ticket TO, as the same post (its author,
    /// words and names), and stays on T as a stub saying where it went. Whoever it names and had not been given it on
    /// T is given it on TO; whoever had read it there has read it on TO too, where it is the last post.
    fn move_post(&mut self, cx: &mut Cx, t: u64, n: u32, to: u64, by: &str, work: &mut Vec<Work>) -> Result<String, String> {
        if t == to {
            return Err(String::from("A post is moved to another ticket."));
        }
        let (from_id, to_id) = (self.id(t), self.id(to));
        if !self.tickets.contains_key(&to) {
            return Err(format!("There is no ticket {to_id}."));
        }
        let x = self.tickets.get(&t).ok_or_else(|| format!("There is no ticket {from_id}."))?;
        let p = x.posts.iter().find(|p| p.n == n).ok_or_else(|| format!("{from_id} has no post {n}."))?.clone();
        if let Some(w) = &p.moved {
            return Err(format!("{from_id}.p{n} was moved to {w} already."));
        }
        if p.held.is_some() || p.dropped {
            return Err(format!("{from_id}.p{n} is {}.", if p.dropped { "dropped" } else { "not given to anyone yet" }));
        }
        let read_on_t: Vec<String> = x.mu.iter().filter(|(_, m)| **m >= n).map(|(a, _)| a.clone()).collect();
        // a post typed an ask to him takes its ask along, open where it lands while it was open here; marked passages
        // stay with the post moved, as before
        let typed_ask = p.kind.as_ref().is_some_and(crate::line::PostKind::asks_owner);
        let asks_open = typed_ask && self.asks_open(x, n);
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let q = x.posts.iter_mut().find(|q| q.n == n).unwrap();
        q.moved = Some(to_id.clone());
        if typed_ask {
            q.asks_open = false;
        }
        let y = self.tickets.edit().get_mut(&to).unwrap();
        let m = y.last() + 1;
        y.posts.push(Post { n: m, at: cx.now.clone(), by: p.by.clone(), text: p.text.clone(), head: None, step: None, names: p.names.clone(), told: p.told.clone(), auto: p.auto, moved: None, from: Some(format!("{from_id}.p{n}")), held: None, halted: false, dropped: false, routed: false, asks: Vec::new(), asks_open, asks_closed: None, answers: None, inform: p.inform.clone(), typed: None, cleaned_from: None, split_from: None, on: Vec::new(), also: Vec::new(), would_close: None, taken_back: false, joins: None, passes: None, comments: p.comments.clone(), into_turn: Vec::new(), kind: p.kind.clone(), answering: p.answering.clone() });
        for a in &read_on_t {
            if y.mu.get(a).copied().unwrap_or(0) + 1 == m {
                y.mu.insert(a.clone(), m);
            }
        }
        cx.effect(Effect::Changed { t });
        cx.effect(Effect::Changed { t: to });
        // the owner's words moved here land here: its handler says whom they are for
        if p.by == "owner" {
            if let Some(w) = self.owner_landed(cx, to, m, None) {
                work.push(w);
                return Ok(format!("{from_id}.p{n} moved to {to_id} as post {m} (by {by})."));
            }
        }
        for a in p.wakes() {
            if !read_on_t.contains(a) {
                cx.effect(Effect::Deliver { to: a.clone() });
            }
        }
        Ok(format!("{from_id}.p{n} moved to {to_id} as post {m} (by {by})."))
    }

    /// The one-offs TS made one thread (the owner's "make this a ticket", over a run of one-offs): each a thread that
    /// ended OneOff, all to the same agent, in order, with no other thread to that agent between the first and the
    /// last. The first of them rests again, holding in order each one's words (the owner's, naming the agent) and the
    /// agent's answers to them (by their places); the others end Moved to it.
    fn keep(&mut self, cx: &mut Cx, ts: &[u64], by: &str) -> Result<String, String> {
        let (&first, &last) = (ts.first().ok_or("Keep takes one-offs: none were named.")?, ts.last().unwrap());
        if ts.windows(2).any(|w| w[0] >= w[1]) {
            return Err(String::from("The one-offs are named in the order they came, each once."));
        }
        let to_of = |x: &Ticket| x.form.get("to").and_then(Value::as_str).map(str::to_string);
        let mut agent: Option<String> = None;
        for t in ts {
            let x = self.tickets.get(t).ok_or_else(|| format!("There is no ticket {t}."))?;
            let one_off = matches!(&x.status, Status::Done(v) if v.get("head").and_then(Value::as_str) == Some("OneOff"));
            if x.kind != "thread" || !one_off {
                return Err(format!("{} is not a one-off.", self.id(*t)));
            }
            let a = to_of(x).ok_or_else(|| format!("{} was said to nobody.", self.id(*t)))?;
            if agent.as_ref().is_some_and(|b| *b != a) {
                return Err(format!("{} was said to {a}, the others to {}.", self.id(*t), agent.unwrap()));
            }
            agent = Some(a);
        }
        let agent = agent.unwrap();
        if let Some(x) = self.tickets.range(first..=last).map(|(_, x)| x).find(|x| x.kind == "thread" && !ts.contains(&x.t) && to_of(x).as_deref() == Some(agent.as_str())) {
            return Err(format!("{} came between them, to {agent}: a run of one-offs has no other thread in it.", self.id(x.t)));
        }
        let first_id = self.id(first);
        let mut posts = Vec::new();
        for t in ts {
            let x = &self.tickets[t];
            if let Some(words) = x.form.get("text").and_then(Text::of) {
                posts.push(("owner".to_string(), words, vec![agent.clone()], false));
            }
            for (a, loc) in x.answers.iter().filter(|(a, _)| *a == agent) {
                posts.push((a.clone(), Text::Loc { loc: loc.clone() }, vec!["owner".to_string()], true));
            }
        }
        for t in &ts[1..] {
            let x = self.tickets.edit().get_mut(t).unwrap();
            let result = json!({"head": "Moved", "to": first_id});
            x.status = Status::Done(result.clone());
            x.closed_at = Some(cx.now.clone());
            x.closes += 1;
            cx.line(Line::Closed { at: cx.now.clone(), t: *t, result: Some(result), raised: None });
            cx.effect(Effect::Changed { t: *t });
        }
        let k = self.defs.kinds.get("thread").cloned();
        let x = self.tickets.edit().get_mut(&first).unwrap();
        for (a, text, names, auto) in posts {
            let n = x.last() + 1;
            x.posts.push(Post { n, at: cx.now.clone(), by: a, text, head: None, step: None, names, told: Vec::new(), auto, moved: None, from: None, held: None, halted: false, dropped: false, routed: false, asks: Vec::new(), asks_open: false, asks_closed: None, answers: None, inform: Vec::new(), typed: None, cleaned_from: None, split_from: None, on: Vec::new(), also: Vec::new(), would_close: None, taken_back: false, joins: None, passes: None, comments: Vec::new(), into_turn: Vec::new(), kind: None, answering: Vec::new() });
        }
        let last_n = x.last();
        // everyone on it has read it all: it is a record, not news
        x.mu.insert(agent.clone(), last_n);
        x.mu.insert("owner".into(), last_n);
        let env: Env = x.form.as_object().map(|o| o.iter().map(|(a, b)| (a.clone(), b.clone())).collect()).unwrap_or_default();
        let scope = k.map(|k| k.form.into_iter().collect()).unwrap_or_default();
        x.status = Status::Rests { env, scope };
        x.closed_at = None;
        cx.line(Line::Rests { at: cx.now.clone(), t: first });
        cx.effect(Effect::Changed { t: first });
        Ok(format!("{first_id} is a thread now: {} one-off(s) to {agent}, {last_n} post(s) (by {by}).", ts.len()))
    }

    fn new_ticket(&self, t: u64, kind: &str, parent: Option<u64>, opener: Option<String>, form: Value, ctx: Value, program: String, k: &Kind, at: &At) -> Ticket {
        Ticket {
            tie: None,
            titled: None,
            ended: None,
            t,
            kind: kind.to_string(),
            parent,
            under: None,
            opener,
            form,
            form_text: None,
            ctx,
            program,
            calls: BTreeMap::new(),
            posts: Vec::new(),
            steps: 0,
            bound: check::bound(&k.pattern, &self.defs.kinds),
            status: Status::Waits { children: Vec::new(), par: false },
            k: Vec::new(),
            mu: BTreeMap::new(),
            given: Vec::new(),
            closed_given: BTreeSet::new(),
            unread_twice: BTreeSet::new(),
            let_through: BTreeSet::new(),
            refused_notices: BTreeSet::new(),
            layers: 0,
            told_closed: BTreeSet::new(),
            informed: BTreeSet::new(),
            about: Vec::new(),
            children: Vec::new(),
            opened_at: at.clone(),
            closed_at: None,
            broken: None,
            loose: BTreeSet::new(),
            reloaded: None,
            scoped: 0,
            live: Vec::new(),
            kept: Vec::new(),
            step_calls: BTreeMap::new(),
            withdrawn: Vec::new(),
            struck: Vec::new(),
            asked: BTreeSet::new(),
            told_late: Vec::new(),
            was_asked: BTreeSet::new(),
            lapsed: Vec::new(),
            started: Vec::new(),
            quiet: Vec::new(),
            goals: k.goals.clone(),
            holding: None,
            closes: 0,
            any: None,
            branch: None,
            waiting: None,
            unchecked: false,
            gives: Vec::new(),
            answers: Vec::new(),
            raised_by: None,
            binding: None,
            handlers: k.handlers.clone(),
            asked_at: None,
            shaping: None,
            shaped: Vec::new(),
            nothing: BTreeMap::new(),
            asks_withdrawn: BTreeMap::new(),
            answered_upto: BTreeMap::new(),
            not_answered: BTreeMap::new(),
            answered_from: Vec::new(),
            handed_in: Vec::new(),
            waits: BTreeSet::new(),
            waits_message: false,
            words_hold: None,
            words_gathered: false,
            ask: None,
            held_ask: None,
            owner_words: None,
            handling: None,
            when: None,
            post_of: None,
            starred: false,
            archived: false,
            told_archived: BTreeMap::new(),
            stars: Vec::new(),
            remind_why: None,
            not_read: Vec::new(),
            trace: Trace::new(k.pattern.clone()),
            last_by: None,
            held: None,
            pending: None,
            last_node: None,
            layer_nodes: BTreeMap::new(),
            stop_of: None,
            stops: Vec::new(),
            passing: false,
            slow: false,
            halts: Vec::new(),
            halt_gone: None,
            halted: 0,
        }
    }

    /// What ticket X is checked against when it is changed (Withdraw, Edit, Repair, Rekind): its kind as recorded;
    /// for a branch of a `par` or an `any`, which has no kind of its own, a kind with no form, the branch's type, and
    /// what the nearest ticket above with a kind may raise.
    /// The kind ticket X runs as its own text has it (`Defs::kind_as`).
    pub fn own_kind<'a>(&'a self, x: &Ticket) -> Option<KindAs<'a>> {
        self.defs.kind_as(&x.kind, &x.program)
    }

    fn kind_of(&self, x: &Ticket) -> Result<Kind, String> {
        if x.kind == ASK && x.ask.is_some() {
            return Ok(Kind { result: Type::Any, ..loose_kind(ASK) });
        }
        if x.kind != "part" {
            let calls = self.calls_at(x.t);
            // a text that opens a kind changed since (a reload, N-101 p460) checks with that kind as it was
            return self.parse_program_in(&x.kind, &x.program, false, &calls).or_else(|e| self.with_kinds_as_they_were(&x.kind, &x.program, &calls).map(|(k, _)| k).ok_or(e));
        }
        let above = x.parent.and_then(|p| self.tickets.get(&p)).ok_or_else(|| format!("{} is a branch with nothing above it", self.id(x.t)))?;
        let up = self.kind_of(above)?;
        let result = x.branch.clone().unwrap_or(Type::Any);
        Ok(Kind { name: "part".into(), file: String::new(), line: 0, letter: None, width: 0, form: Vec::new(), result, raises: up.raises, goals: Vec::new(), pattern: Prog::Return { e: Expr::None, at: 0 }, text: String::new(), base: None, handlers: Vec::new(), doc: Default::default(), drawn: None })
    }

    /// The calls program TEXT names (a word of it that is a call's name, or a decision's), as they are now: what its
    /// line records, so that a restart runs it against them and not today's (calls on the record; the owner, N-221.p3:
    /// a ticket's state is what the record says happened, whichever text was loaded then).
    fn calls_named(&self, text: &str) -> BTreeMap<String, CallSig> {
        let mut out = BTreeMap::new();
        let words = text.split(|c: char| !(c.is_alphanumeric() || c == '_')).chain(text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-')));
        for w in words.filter(|w| !w.is_empty()) {
            for n in [w.to_string(), format!("{}{w}", syntax::DECIDE)] {
                if let Some(c) = self.defs.calls.get(&n) {
                    out.insert(n, c.clone());
                }
            }
        }
        out
    }

    /// Today's calls with RECORDED over them.
    fn calls_with(&self, recorded: &BTreeMap<String, CallSig>) -> BTreeMap<String, CallSig> {
        let mut c = self.defs.calls.clone();
        c.extend(recorded.iter().map(|(n, s)| (n.clone(), s.clone())));
        c
    }

    /// The calls ticket T runs against: today's, with those recorded for the tickets above it over them, and its own
    /// over those.
    fn calls_at(&self, t: u64) -> BTreeMap<String, CallSig> {
        let mut chain = Vec::new();
        let mut u = Some(t);
        while let Some(x) = u.and_then(|v| self.tickets.get(&v)) {
            chain.push(x);
            u = x.parent.or(x.under);
        }
        let mut c = self.defs.calls.clone();
        for x in chain.iter().rev() {
            c.extend(x.calls.iter().map(|(n, s)| (n.clone(), s.clone())));
        }
        c
    }

    /// Call NAME as ticket T runs it: as recorded for T, else for the nearest ticket above it, else today's.
    /// The owner's steps owed now that may go on by themselves (`lapsing`): ticket, step, call, its default, since
    /// when; the tickets that owe something else by now are dropped from the set.
    pub fn lapsing(&mut self) -> Vec<(u64, u32, String, String, At)> {
        let mut out = Vec::new();
        let mut gone = Vec::new();
        for &t in &self.lapsing {
            match self.tickets.get(&t).map(|x| &x.status) {
                Some(Status::Awaits { agent, owed, since, .. }) if agent == "owner" && !owed.hold => match self.call_of(t, &owed.call).and_then(|c| c.default) {
                    Some(d) => out.push((t, owed.step, owed.call.clone(), d, since.clone())),
                    None => gone.push(t),
                },
                _ => gone.push(t),
            }
        }
        for t in gone {
            self.lapsing.remove(&t);
        }
        out
    }

    pub fn call_of(&self, t: u64, name: &str) -> Option<CallSig> {
        let mut u = Some(t);
        while let Some(x) = u.and_then(|v| self.tickets.get(&v)) {
            if let Some(c) = x.calls.get(name) {
                return Some(c.clone());
            }
            u = x.parent.or(x.under);
        }
        self.defs.calls.get(name).cloned()
    }

    /// KIND's program from its recorded TEXT, and why it no longer checks when it does not. Live, a program that does
    /// not check is refused. On replay it runs as recorded (the log is what happened): checked against today's calls
    /// loosened (`loose_calls`: a call that has since gained or lost a parameter, as `cut(branch)` became `cut()`), so
    /// its constructors still build by field name; else parsed but unchecked; one that no longer parses becomes
    /// `raise NoCall`. Either way it is marked as no longer checking (`unchecked`).
    fn recorded_program(&self, kind: &str, text: &str, calls: &BTreeMap<String, CallSig>, cx: &Cx) -> Result<(Kind, Option<String>), String> {
        self.recorded_program_as(kind, text, calls, cx).map(|(k, bad, _)| (k, bad))
    }

    /// `recorded_program`, and the kinds TEXT opens taken as they were (`Ticket::loose`): on replay, a text that opens
    /// a kind whose form, result or raises a reload has since changed (allowed while no ticket of it is open, N-101
    /// p460) checks with those kinds loosened, and is not marked; `replay` marks an open one whose remainder would
    /// still open one of them.
    fn recorded_program_as(&self, kind: &str, text: &str, calls: &BTreeMap<String, CallSig>, cx: &Cx) -> Result<(Kind, Option<String>, BTreeSet<String>), String> {
        let calls = self.calls_with(calls);
        match self.parse_program_in(kind, text, cx.recorded.is_none(), &calls) {
            Ok(k) => Ok((k, None, BTreeSet::new())),
            Err(e) if cx.recorded.is_some() => {
                if let Some((k, loose)) = self.with_kinds_as_they_were(kind, text, &calls) {
                    return Ok((k, None, loose));
                }
                let (k, bad) = self.recorded_unchecked(kind, text, &calls, e);
                Ok((k, bad, BTreeSet::new()))
            }
            Err(e) => Err(e),
        }
    }

    /// TEXT checked against CALLS and today's kinds, with those it opens loosened (their form's fields optional and of
    /// any type, room for more, their result first as today's, then of any type): the program and the fewest kinds
    /// that had to be, or none when it does not check so either.
    fn with_kinds_as_they_were(&self, kind: &str, text: &str, calls: &BTreeMap<String, CallSig>) -> Option<(Kind, BTreeSet<String>)> {
        let k = syntax::parse_kind(kind, text, 1, &self.defs.named).ok()?;
        // on replay, only a kind a reload named after the line being replayed: one no reload changed since is as it
        // was, and a text that does not check against it never did
        // (one taken as unchanged is kept as asked: a reload of it after a cut reaches back, `reaches_back`)
        let changed = |n: &String| {
            let changed = self.replaying.is_none_or(|i| self.reloaded_at.get(n).is_some_and(|r| *r > i));
            if !changed {
                self.asked_kinds.lock().unwrap_or_else(|e| e.into_inner()).insert(n.clone());
            }
            changed
        };
        let opened: BTreeSet<String> = check::opens(&k.pattern).into_iter().chain(k.handlers.iter().flat_map(|h| check::opens(&h.body))).filter(|n| self.defs.kinds.contains_key(n) && changed(n)).collect();
        if opened.is_empty() {
            return None;
        }
        let try_with = |loose: &BTreeSet<String>, any: bool| -> Option<Kind> {
            let mut kinds = self.defs.kinds.clone();
            for n in loose {
                let k = kinds.get_mut(n).unwrap();
                let mut form: Vec<(String, Type)> = k.form.iter().map(|(n, _)| (n.clone(), Type::Opt(Box::new(Type::Any)))).collect();
                for i in form.len()..16 {
                    form.push((format!("arg{}", i + 1), Type::Opt(Box::new(Type::Any))));
                }
                k.form = form;
                if any {
                    k.result = Type::Any;
                }
            }
            self.parse_program_with(kind, text, false, calls, &kinds).ok()
        };
        for any in [false, true] {
            let Some(mut found) = try_with(&opened, any) else { continue };
            // the fewest: each one that checks as today's files have it is taken so
            let mut loose = opened.clone();
            for n in &opened {
                let mut fewer = loose.clone();
                fewer.remove(n);
                if let Some(k) = try_with(&fewer, any) {
                    (loose, found) = (fewer, k);
                }
            }
            return Some((found, loose));
        }
        None
    }

    /// TEXT that does not check against today's (E, why): against the calls loosened, else read unchecked, else
    /// `raise NoCall`; marked.
    fn recorded_unchecked(&self, kind: &str, text: &str, calls: &BTreeMap<String, CallSig>, e: String) -> (Kind, Option<String>) {
        if let Ok(k) = self.parse_program_in(kind, text, false, &loose_calls(calls)) {
            return (k, Some(e));
        }
        let k = syntax::parse_kind(kind, text, 1, &self.defs.named)
            .unwrap_or_else(|_| Kind { pattern: Prog::Raise { exc: "NoCall".into(), e: Some(Expr::Str(e.clone())), at: 0 }, text: text.to_string(), ..loose_kind(kind) });
        (k, Some(e))
    }

    /// Ticket T's program did not check on replay (WHY): marked broken, and unless the log leaves it closed, its next
    /// step fails with NoCall.
    fn unchecked(&mut self, t: u64, why: &str) {
        let closed = self.closed_in_log.contains_key(&t);
        // asked of what the log says ahead while replaying: kept as asked (`reaches_back`)
        if self.replaying.is_some() {
            self.asked_closed.insert(t);
        }
        if let Some(x) = self.tickets.edit().get_mut(&t) {
            x.broken.get_or_insert_with(|| format!("its recorded program no longer checks: {why}"));
            x.unchecked = !closed;
        }
    }

    /// KIND's program from its TEXT (a ticket's recorded program), parsed and checked against CALLS (the ones it ran
    /// against: `calls_with`, `calls_at`) and today's kinds.
    fn parse_program_in(&self, kind: &str, text: &str, strict: bool, calls: &BTreeMap<String, check::CallSig>) -> Result<Kind, String> {
        self.parse_program_with(kind, text, strict, calls, &self.defs.kinds)
    }

    /// `parse_program_in` against KINDS.
    fn parse_program_with(&self, kind: &str, text: &str, strict: bool, calls: &BTreeMap<String, check::CallSig>, kinds: &BTreeMap<String, Kind>) -> Result<Kind, String> {
        let mut k = syntax::parse_kind(kind, text, 1, &self.defs.named)?;
        // the base's handlers as today's kinds have them (a ticket's text holds its own kind's only)
        if let Some(b) = check::base_of(&k, &self.defs.kinds) {
            if let Some(base) = self.defs.kinds.get(&b).filter(|b| b.name != k.name) {
                check::inherit(&mut k, base);
            }
        }
        let handlers = std::mem::take(&mut k.handlers);
        let mut whole = Kind { handlers, ..k.clone() };
        check::check_program_kind(&mut whole, calls, kinds, &self.defs.events, Some(&self.defs.named), strict).map_err(|e| format!("kind {kind}: {e}"))?;
        Ok(whole)
    }

    /// The owner lets halt N of T through: what it held back happens now, as edited. One that no longer stands (the
    /// step it held was withdrawn meanwhile, the ticket closed) goes, and says so.
    fn approve(&mut self, cx: &mut Cx, t: u64, n: u32, by: &str) -> Result<String, String> {
        if by != "owner" {
            return Err(String::from("Only the owner approves what halted."));
        }
        let id = self.id(t);
        let x = self.tickets.edit().get_mut(&t).ok_or_else(|| format!("There is no ticket {t}."))?;
        let i = x.halts.iter().position(|h| h.n == n).ok_or_else(|| format!("{id} has no halt {n} waiting."))?;
        let h = x.halts.remove(i);
        x.halt_gone = Some(cx.now.clone());
        self.halt_went = true;
        cx.effect(Effect::Changed { t });
        let open = x.is_open();
        let gone = || Ok(format!("Halt {n} of {id} no longer stands; nothing was let through."));
        match h.what {
            Halting::Give { step, start, from } => {
                let Status::Awaits { agent, owed, since, .. } = &mut x.status else { return gone() };
                if owed.step != step || !open {
                    return gone();
                }
                // its wait starts now: time halted is not waiting
                *since = cx.now.clone();
                let agent = agent.clone();
                if let Some(call) = start {
                    cx.effect(self.start_of(agent.clone(), t, call, from));
                }
                cx.effect(Effect::Deliver { to: agent.clone() });
                Ok(format!("Step {step} of {id} goes to {agent}."))
            }
            Halting::Take { step, by: who, head, text } => {
                if !matches!(&x.status, Status::Awaits { owed, .. } if owed.step == step) || !open {
                    return gone();
                }
                self.accept(cx, t, &who, head, text, step);
                Ok(format!("{who}'s answer to step {step} of {id} is taken."))
            }
            Halting::Deliver { post, with, .. } => {
                // the card's posts, one tell's (his burst's, E1b), each still there; all go at once
                let ns: Vec<u32> = std::iter::once(post).chain(with).filter(|n| x.posts.iter().any(|p| p.n == *n)).collect();
                if ns.is_empty() {
                    return gone();
                }
                let before = x.his_shown();
                let mut to: Vec<String> = Vec::new();
                for p in x.posts.iter_mut().filter(|p| ns.contains(&p.n)) {
                    p.halted = false;
                    for a in p.wakes().chain(&p.inform) {
                        if !to.contains(a) {
                            to.push(a.clone());
                        }
                    }
                }
                // his posts reach whom they wake now, as he left them at his card, and so do his posts behind it,
                // an agent's card too (the brain's `owner_to`)
                self.posts_reached(cx, t, &before);
                for a in to.iter().chain(&self.participants(t)) {
                    cx.effect(Effect::Deliver { to: a.clone() });
                }
                let posts = match ns.as_slice() {
                    [n] => format!("Post {n} of {id} is"),
                    _ => format!("Posts {} of {id} are", ns.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")),
                };
                Ok(format!("{posts} delivered{}.", if to.is_empty() { String::new() } else { format!(" ({})", to.join(", ")) }))
            }
            Halting::Open { pattern } => {
                if !open {
                    return gone();
                }
                let env: Env = x.form.as_object().map(|o| o.iter().map(|(a, b)| (a.clone(), b.clone())).collect()).unwrap_or_default();
                self.drive(cx, vec![Work::Run(t, pattern, env)]);
                Ok(format!("{id} runs."))
            }
            Halting::Decide { step, question, answer, .. } => {
                let stands = match question.as_str() {
                    "fills" | "decide" => matches!(&x.status, Status::Asking { owed, .. } if owed.step == step),
                    "holds" => x.holding.is_some() && (step == 0 || step == x.closes + 1),
                    _ => matches!(&x.status, Status::Awaits { owed, .. } if owed.step == step),
                };
                if !stands {
                    return gone();
                }
                match question.as_str() {
                    "fills" => self.use_fills(cx, t, &answer),
                    "decide" => self.use_decide(cx, t, &answer),
                    "holds" => self.use_holds(cx, t, &answer),
                    _ if answer == json!(true) => self.use_expired(cx, t),
                    // edited to "not expired": the step waits again, from now
                    _ => {
                        if let Status::Awaits { since, .. } = &mut self.tickets.edit().get_mut(&t).unwrap().status {
                            *since = cx.now.clone();
                        }
                    }
                }
                Ok(format!("The policy's answer ({question}: {answer}) is used on {id}."))
            }
            Halting::Notice { id: d, to, .. } => {
                let x = &self.tickets[&t];
                if !self.due_on(x, &to).iter().any(|e| e.id == d) {
                    return gone();
                }
                self.tickets.edit().get_mut(&t).unwrap().let_through.insert(format!("{to} {d}"));
                cx.effect(Effect::Deliver { to: to.clone() });
                Ok(format!("{d} goes to {to}."))
            }
        }
    }

    /// The owner refuses halt N of T (N-1788: slow mode's "no"), for REASON: what it held back is not done, and nothing
    /// is left halted for it. A step not yet given, an answer not yet taken, a ticket not yet started or a policy
    /// answer not yet used: the ticket is cancelled, Cancelled{by, why}, its open children with it, so it never waits on
    /// what he refused. His post or posts of one card: dropped, reaching nobody (`Line::Dropped` each). A notice: never
    /// given to its agent, nor halted again. One that no longer stands goes, and says so, as at an approval.
    fn reject(&mut self, cx: &mut Cx, t: u64, n: u32, by: &str, reason: Option<&str>) -> Result<String, String> {
        if by != "owner" {
            return Err(String::from("Only the owner refuses what halted."));
        }
        let id = self.id(t);
        let x = self.tickets.edit().get_mut(&t).ok_or_else(|| format!("There is no ticket {t}."))?;
        let i = x.halts.iter().position(|h| h.n == n).ok_or_else(|| format!("{id} has no halt {n} waiting."))?;
        let h = x.halts.remove(i);
        x.halt_gone = Some(cx.now.clone());
        self.halt_went = true;
        cx.effect(Effect::Changed { t });
        let open = x.is_open();
        let why = match reason.map(str::trim).filter(|r| !r.is_empty()) {
            Some(r) => format!("halt {n} refused: {r}"),
            None => format!("halt {n} refused"),
        };
        match h.what {
            Halting::Give { .. } | Halting::Take { .. } | Halting::Open { .. } | Halting::Decide { .. } => {
                if !open {
                    return Ok(format!("Halt {n} of {id} no longer stands; nothing was done."));
                }
                let r = Raised { exc: "Cancelled".into(), value: json!({"by": by, "why": why}) };
                let mut work = Vec::new();
                self.cancel(cx, t, by, &why, &r, &mut work);
                self.drive(cx, work);
                Ok(format!("{id} is cancelled: {why}."))
            }
            Halting::Deliver { post, with, .. } => {
                let ns: Vec<u32> = std::iter::once(post).chain(with).collect();
                let mut dropped = Vec::new();
                for p in x.posts.iter_mut().filter(|p| ns.contains(&p.n) && !p.dropped && !p.taken_back) {
                    p.halted = false;
                    p.held = None;
                    p.dropped = true;
                    dropped.push(p.n);
                }
                for n in &dropped {
                    x.asked.remove(n);
                    cx.line(Line::Dropped { at: cx.now.clone(), t, n: *n });
                }
                match dropped.as_slice() {
                    [] => Ok(format!("Halt {n} of {id} no longer stands; nothing was done.")),
                    [p] => Ok(format!("Post {p} of {id} is dropped: {why}.")),
                    ps => Ok(format!("Posts {} of {id} are dropped: {why}.", ps.iter().map(u32::to_string).collect::<Vec<_>>().join(", "))),
                }
            }
            Halting::Notice { id: d, to, .. } => {
                x.refused_notices.insert(format!("{to} {d}"));
                Ok(format!("{d} is not given to {to}: {why}."))
            }
        }
    }

    /// The owner changes what halt N of T lets through. VALUE by the halt: a give's `prompt` (the text sent) and
    /// `worker`; a take's `head`, which must type; a deliver's `wakes`; an open's `form`, which must type; a decide's
    /// `answer`. The halt stays; only an approval lets it through. An agent's words are never edited.
    fn halt_edit(&mut self, cx: &mut Cx, t: u64, n: u32, by: &str, value: &Value) -> Result<String, String> {
        if by != "owner" {
            return Err(String::from("Only the owner edits what halted."));
        }
        let id = self.id(t);
        let kind = self.tickets.get(&t).map(|x| self.kind_of(x));
        let x = self.tickets.edit().get_mut(&t).ok_or_else(|| format!("There is no ticket {t}."))?;
        let i = x.halts.iter().position(|h| h.n == n).ok_or_else(|| format!("{id} has no halt {n} waiting."))?;
        let field = |k: &str| value.get(k).filter(|v| !v.is_null());
        let said = match &mut x.halts[i].what {
            Halting::Give { step, start, .. } => {
                let Status::Awaits { agent, owed, .. } = &mut x.status else { return Err(format!("{id} awaits no step.")) };
                if owed.step != *step {
                    return Err(format!("Step {step} of {id} is no longer the one it awaits."));
                }
                let mut said = Vec::new();
                if let Some(w) = field("worker") {
                    let w = w.as_str().filter(|w| !w.trim().is_empty() && *w != "owner").ok_or("The worker is an agent's name.")?;
                    if start.is_some() || x.live.contains(agent) {
                        return Err(String::from("This step's worker is an agent made for it alone; it cannot be given to another."));
                    }
                    *agent = w.to_string();
                    said.push(format!("goes to {w}"));
                }
                if let Some(p) = field("prompt") {
                    let p = p.as_str().ok_or("The prompt is a text.")?;
                    owed.prompt = Some(p.to_string());
                    said.push(String::from("its prompt is your text"));
                }
                if said.is_empty() {
                    return Err(String::from("A step given is edited by its `prompt` or its `worker`."));
                }
                format!("Step {step} of {id}: {}.", said.join("; "))
            }
            Halting::Take { step, head, .. } => {
                let h = field("head").ok_or("An answer taken is edited by its `head`.")?;
                let Status::Awaits { owed, .. } = &x.status else { return Err(format!("{id} awaits no step.")) };
                let typed = owed.head.coerce(&normal_head(h)).and_then(|h| owed.head.conforms(&h).map(|()| h)).map_err(|e| format!("That head does not fit step {step} ({}): {e}.", owed.head))?;
                *head = typed;
                format!("The answer to step {step} of {id} is your head now.")
            }
            Halting::Deliver { post, with, edited, .. } => {
                let w = field("wakes").and_then(Value::as_array).ok_or("A post delivered is edited by `wakes`, the agents it wakes.")?;
                let wakes: Vec<String> = w.iter().filter_map(|a| a.as_str().map(str::to_string)).filter(|a| a != "owner").collect();
                // every post the card holds (one tell's, one choice: a Split's part has one `to`)
                let ns: Vec<u32> = std::iter::once(*post).chain(with.iter().copied()).collect();
                *edited = true;
                if !x.posts.iter().any(|p| ns.contains(&p.n)) {
                    return Err(format!("{id} has no post {post}."));
                }
                for p in x.posts.iter_mut().filter(|p| ns.contains(&p.n)) {
                    p.routed = true;
                    p.told = wakes.clone();
                }
                let posts = match ns.as_slice() {
                    [n] => format!("Post {n} of {id} wakes"),
                    _ => format!("Posts {} of {id} wake", ns.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")),
                };
                format!("{posts} {} once approved.", if wakes.is_empty() { String::from("nobody") } else { wakes.join(", ") })
            }
            Halting::Open { .. } => {
                let f = field("form").ok_or("A ticket opened is edited by its `form`.")?;
                let k = kind.unwrap()?;
                let form = Type::Record(k.form.clone()).coerce(f).and_then(|v| Type::Record(k.form.clone()).conforms(&v).map(|()| v)).map_err(|e| format!("That form does not fit a {}: {e}.", x.kind))?;
                x.form = form;
                format!("{id} runs on your form once approved.")
            }
            Halting::Decide { question, answer, .. } => {
                let a = value.get("answer").ok_or("A policy answer is edited by its `answer`.")?;
                let fits = match question.as_str() {
                    "fills" => a.is_null() || a.as_str().is_some_and(|s| !s.trim().is_empty()),
                    "decide" => a.is_null() || a.as_str().is_some_and(|s| !s.trim().is_empty()),
                    "holds" => a.as_array().is_some_and(|xs| xs.iter().all(Value::is_string)),
                    _ => a.is_boolean(),
                };
                if !fits {
                    return Err(format!("That answer does not fit `{question}`: {}.", match question.as_str() { "fills" => "an agent's name, or null for nobody", "decide" => "an answer's name, or null for the owner to answer", "holds" => "the list of the goals that do not hold", _ => "true (expired) or false" }));
                }
                *answer = a.clone();
                format!("The policy's answer ({question}) on {id} is yours now: {a}.")
            }
            Halting::Notice { .. } => return Err(String::from("A notice is let through as it is: approve it, or leave it halted.")),
        };
        cx.effect(Effect::Changed { t });
        Ok(said)
    }

    /// Policy's answer to whether T's goals hold (the ones that do not), used: T closes.
    fn use_holds(&mut self, cx: &mut Cx, t: u64, answer: &Value) {
        let Some(v) = self.tickets.edit().get_mut(&t).unwrap().holding.take() else { return };
        let unmet: Vec<String> = answer.as_array().map(|a| a.iter().filter_map(|g| g.as_str().map(str::to_string)).collect()).unwrap_or_default();
        let end = match unmet.first() {
            None => Status::Done(v),
            Some(g) => Status::Failed(Raised { exc: "Owed".into(), value: json!({"goal": g, "goals": unmet, "value": v}) }),
        };
        let mut work = Vec::new();
        self.close(cx, t, end, &mut work);
        self.drive(cx, work);
    }

    /// Policy's answer to who fills the role T asks about (an agent, or nobody), used.
    /// The policy's ANSWER to the decision T asks: a constructor of its answers, by name, its fields from the
    /// decision's arguments of the same names, given back where `decide` stands; null, or one that names no answer or
    /// whose fields the arguments do not hold, makes it the owner's to answer, as a step owed to him (his card shows
    /// the decision, its arguments and its answers). An LLM policy would answer between the rules and the owner; none
    /// is built.
    fn use_decide(&mut self, cx: &mut Cx, t: u64, answer: &Value) {
        let Status::Asking { owed, .. } = &self.tickets[&t].status else { return };
        let owed = owed.clone();
        match self.decided_fields(t, &owed, answer) {
            Some(v) => {
                if let (Some(name), Some(c)) = (owed.call.strip_prefix(syntax::DECIDE), v.get("head").and_then(Value::as_str)) {
                    self.decided.push((t, name.to_string(), owed.args.clone(), c.to_string()));
                }
                self.tickets.edit().get_mut(&t).unwrap().status = Status::Waits { children: Vec::new(), par: false };
                cx.effect(Effect::Changed { t });
                self.drive(cx, vec![Work::Resume(t, v)]);
            }
            None => self.await_step(cx, t, "owner".into(), owed),
        }
    }

    /// The value of the policy's ANSWER to the decision OWED (N-101 p158-p162): a constructor of its answers, by name
    /// (or `{head, F: ...}`, coordd's with the fields it gives, such as `why`, the rule's text). A field takes, first
    /// found: the decision's argument of its name; the answer's field of its name; for a field of type T, the single
    /// element of the decision's one argument of type [T] other than `not` (`not` applied); then, from a field already filled with a
    /// ticket (the picked ticket), that ticket's result's field of its name when it is closed done, else the last
    /// value given under that name to one of its steps; an optional field none of these fills is none. None: the
    /// answer names no constructor, or a field has no source (the owner answers). A decision among agents (`-> Agent`,
    /// N-1134's `claims`) is answered by an agent's name, one of the agents its arguments give it.
    fn decided_fields(&self, t: u64, owed: &Owed, answer: &Value) -> Option<Value> {
        if owed.head == Type::Agent {
            // an agent's name, one of those its arguments give it or another (`claims`' answerer, consult-lead N-1158)
            let a = answer.as_str().filter(|a| !a.is_empty())?;
            return Some(json!(a));
        }
        let (c, given) = match answer {
            Value::String(c) => (c.as_str(), None),
            Value::Object(o) => (o.get("head")?.as_str()?, Some(o)),
            _ => return None,
        };
        let fs = owed.head.constructor(c)?;
        let params = self.call_of(t, &owed.call).map(|s| s.params).unwrap_or_default();
        let not = nots(&owed.args);
        let mut v = Map::new();
        v.insert("head".into(), json!(c));
        let mut left = Vec::new();
        for (f, ft) in fs {
            let found = owed.args.get(f).filter(|a| !a.is_null()).cloned().or_else(|| given.and_then(|g| g.get(f)).filter(|a| !a.is_null()).cloned()).or_else(|| {
                let lists: Vec<&String> = params.iter().filter(|(n, pt)| n != NOT && matches!(pt, Type::List(e) if **e == *ft)).map(|(n, _)| n).collect();
                let [l] = lists[..] else { return None };
                let left: Vec<&Value> = owed.args.get(l)?.as_array()?.iter().filter(|e| !not.contains(e)).collect();
                match left[..] {
                    [one] => Some(one.clone()),
                    _ => None,
                }
            });
            match found {
                Some(a) => {
                    v.insert(f.clone(), a);
                }
                None => left.push((f, ft)),
            }
        }
        // the picked ticket: a field filled with a ticket of this machine
        let picked = fs.iter().filter(|(_, ft)| *ft == Type::Ticket).find_map(|(f, _)| v.get(f).and_then(Value::as_str).and_then(parse_ticket).filter(|p| self.tickets.contains_key(p)));
        for (f, ft) in left {
            let from = picked.and_then(|p| match &self.tickets[&p].status {
                Status::Done(r) => r.get(f).filter(|a| !a.is_null()).cloned(),
                _ => self.step_args.get(&p).and_then(|a| a.get(f)).cloned(),
            });
            match from {
                Some(a) => v.insert(f.clone(), a),
                None if matches!(ft, Type::Opt(_)) => v.insert(f.clone(), Value::Null),
                None => return None,
            };
        }
        Some(Value::Object(v))
    }

    /// The work T's steps are for (N-101 p166): T, or the nearest ticket above it, of a kind the project wrote: never
    /// a `par`'s or an `any`'s branch (kind `part`), which is the engine's own. Its id.
    pub fn work_of(&self, t: u64) -> Option<String> {
        let mut at = Some(t);
        while let Some(u) = at {
            let x = self.tickets.get(&u)?;
            if x.kind != "part" && x.branch.is_none() && self.defs.kinds.contains_key(&x.kind) {
                return Some(self.id(u));
            }
            at = x.parent;
        }
        None
    }

    /// The agents made for T's thread (item 11, G1): those a ticket in the tree under T's root made, by `let agent`
    /// or fresh for a step, and still has.
    pub fn made_for(&self, t: u64) -> Vec<String> {
        let mut root = t;
        while let Some(p) = self.tickets.get(&root).and_then(|x| x.parent) {
            root = p;
        }
        let in_tree = |mut u: u64| loop {
            if u == root {
                return true;
            }
            match self.tickets.get(&u).and_then(|x| x.parent) {
                Some(p) => u = p,
                None => return false,
            }
        };
        let mut out: Vec<String> = self.tickets.iter().filter(|(u, _)| in_tree(**u)).flat_map(|(_, x)| x.kept.iter().chain(&x.live).cloned()).collect();
        out.extend(self.made.iter().filter(|m| m.t.is_some_and(in_tree)).map(|m| m.agent.clone()));
        out.sort();
        out.dedup();
        out
    }

    /// The top ticket of T's tree, as its id: the thread T is in.
    pub fn thread_of(&self, t: u64) -> String {
        self.id(self.root_of(t))
    }

    /// The top ticket of T's tree.
    pub fn root_of(&self, t: u64) -> u64 {
        let mut root = t;
        while let Some(p) = self.tickets.get(&root).and_then(|x| x.parent) {
            root = p;
        }
        root
    }

    /// The thread AGENT was made for, as its top ticket: from the last `made` line naming it with a ticket (`coord
    /// new` by an agent owing a step, `let agent`). None: not made by a ticket (the owner's, the roster's, or made by
    /// an agent that owed no step).
    /// The ticket AGENT was made in by `let agent NAME`, and NAME (`label`).
    pub fn made_as(&self, agent: &str) -> Option<(u64, String)> {
        self.made.iter().rev().find(|m| m.agent == agent).and_then(|m| Some((m.t?, m.name.clone()?)))
    }

    pub fn made_in(&self, agent: &str) -> Option<u64> {
        self.made.iter().rev().find(|m| m.agent == agent)?.t.map(|t| self.root_of(t))
    }

    /// Whether thread ROOT is still open: running, or an agents' thread (ended as it opened, a place for posts).
    pub fn thread_open(&self, root: u64) -> bool {
        self.tickets.get(&root).is_some_and(|x| x.is_open() || x.between())
    }

    /// The name of the next agent the program makes for ticket T (a fresh step's, a `let agent`'s): fT-N, N counted on
    /// past the names the registry has that the record did not make (`taken`).
    fn next_scoped(&mut self, t: u64) -> String {
        let x = self.tickets.edit().get_mut(&t).unwrap();
        loop {
            x.scoped += 1;
            let agent = format!("f{t}-{}", x.scoped);
            if !self.taken.contains(&agent) {
                self.scoped_names.insert(agent.clone());
                return agent;
            }
        }
    }

    /// Of NAMES (the registry's), those named as the program names its agents (fT-N) that this record neither made nor
    /// has as taken already: what a `taken` line is to say.
    pub fn not_made_here(&self, names: &[String]) -> Vec<String> {
        let pattern = |a: &str| a.strip_prefix('f').and_then(|r| r.split_once('-')).is_some_and(|(t, n)| t.parse::<u64>().is_ok() && n.parse::<u32>().is_ok());
        names.iter().filter(|a| pattern(a) && !self.scoped_names.contains(*a) && !self.taken.contains(*a)).cloned().collect()
    }

    /// Whether AGENT is one the program made for a step or a scope (fT-N: T's Nth), ended or not, or one of that name
    /// an earlier record made (`taken`): one that is given no other work.
    pub fn program_made(&self, agent: &str) -> bool {
        let Some((t, n)) = agent.strip_prefix('f').and_then(|r| r.split_once('-')).and_then(|(t, n)| Some((t.parse::<u64>().ok()?, n.parse::<u32>().ok()?))) else { return false };
        self.taken.contains(agent) || self.tickets.get(&t).is_some_and(|x| n >= 1 && n <= x.scoped)
    }

    /// Every agent made while the record ran (item 11): who made it, the agent it copies, and the thread it was made
    /// in (none: made by an agent that owed no step).
    pub fn made(&self) -> Vec<(String, String, Option<String>, Option<String>)> {
        self.made.iter().map(|m| (m.agent.clone(), m.by.clone(), m.from.clone(), m.t.map(|t| self.thread_of(t)))).collect()
    }

    /// What the policy is shown, beside its arguments, when decision NAME is asked on T at NOW (N-101 p158-p162): for
    /// each list argument X, `X-count`, its elements less any equal to `not`; and of the decision's first Ticket
    /// argument P: for each of its answers C, `earlier-C`, how often a decision of this name with the same P was
    /// answered C before, in the tree under T's parent (T's own when it has none); `first-minutes`, minutes since the
    /// first ticket of T's kind in that tree was opened.
    /// Why words ticket W is not one a post can be the cleaned text of: it must be the owner's `words` ticket, ended in
    /// `Send` (N-487). Ok when it is.
    pub fn cleaned_words(&self, w: u64) -> Result<(), String> {
        let Some(x) = self.tickets.get(&w) else { return Err("is no ticket".into()) };
        if x.kind != "words" || x.opener.as_deref() != Some("owner") {
            return Err("is not the owner's words".into());
        }
        match &x.status {
            Status::Done(v) if v["head"] == "Send" => Ok(()),
            _ => Err("has not ended in Send, so no cleaned words came of it".into()),
        }
    }

    pub fn decide_facts(&self, t: u64, name: &str, args: &Value, now: &str) -> Map<String, Value> {
        let mut out = Map::new();
        let Some(sig) = self.call_of(t, &format!("{}{name}", syntax::DECIDE)) else { return out };
        let not = nots(args);
        for (n, ty) in sig.params.iter().filter(|(n, _)| n != NOT) {
            if let (Type::List(_), Some(a)) = (ty, args.get(n).and_then(Value::as_array)) {
                out.insert(format!("{n}-count"), json!(a.iter().filter(|e| !not.contains(e)).count()));
            }
        }
        let Some(p) = sig.params.iter().find(|(_, ty)| *ty == Type::Ticket).and_then(|(n, _)| args.get(n)).filter(|v| !v.is_null()) else { return out };
        let Some(x) = self.tickets.get(&t) else { return out };
        let root = x.parent.unwrap_or(t);
        let under = |mut u: u64| loop {
            if u == root {
                return true;
            }
            match self.tickets.get(&u).and_then(|y| y.parent) {
                Some(up) => u = up,
                None => return false,
            }
        };
        let pname = sig.params.iter().find(|(_, ty)| *ty == Type::Ticket).map(|(n, _)| n.clone()).unwrap_or_default();
        if let Type::Variant(cs) = &sig.head {
            for (c, _) in cs {
                let k = self.decided.iter().filter(|(u, dn, da, dc)| dn == name && dc == c && da.get(&pname) == Some(p) && under(*u)).count();
                out.insert(format!("earlier-{c}"), json!(k));
            }
        }
        let first = self.tickets.values().filter(|y| y.kind == x.kind && under(y.t)).filter_map(|y| secs(&y.opened_at)).min();
        if let (Some(f), Some(n)) = (first, secs(now)) {
            out.insert("first-minutes".into(), json!((n - f).max(0) / 60));
        }
        out
    }

    /// Whether T is an ask ticket's, or a step run for one: a question's.
    fn in_ask(&self, mut t: u64) -> bool {
        loop {
            let Some(x) = self.tickets.get(&t) else { return false };
            if x.kind == ASK {
                return true;
            }
            match x.parent {
                Some(p) => t = p,
                None => return false,
            }
        }
    }

    fn use_fills(&mut self, cx: &mut Cx, t: u64, answer: &Value) {
        let Status::Asking { owed, role } = &self.tickets[&t].status else { return };
        let (owed, role) = (owed.clone(), role.clone());
        let t = &t;
        match answer.as_str().filter(|a| !a.trim().is_empty()) {
            Some(a) => {
                let a = a.to_string();
                match self.cycle(*t, &a) {
                    // to the lead first; to the owner when the lead is the one asking, or in the cycle
                    Some(why) if role != "lead" => {
                        cx.line(Line::PassedUp { at: cx.now.clone(), t: *t, step: owed.step, from: a.clone(), to: "lead".into(), why: why.clone() });
                        cx.line(Line::Asked { at: cx.now.clone(), t: *t, step: owed.step, question: "fills".into(), role: "lead".into() });
                        cx.effect(Effect::Ask { t: *t, step: owed.step, question: Question::Fills { role: "lead".into(), call: owed.call.clone() } });
                        cx.effect(Effect::Changed { t: *t });
                        self.tickets.edit().get_mut(t).unwrap().status = Status::Asking { owed, role: "lead".into() };
                    }
                    Some(why) => {
                        cx.line(Line::PassedUp { at: cx.now.clone(), t: *t, step: owed.step, from: a, to: "owner".into(), why: why.clone() });
                        self.up_or_owner(cx, *t, owed, why);
                    }
                    None => self.await_step(cx, *t, a, owed),
                }
            }
            // a hold nobody fills is held by nobody: answered by whoever policy lets
            None if owed.hold => self.await_step(cx, *t, NOBODY.into(), owed),
            // a question nobody takes goes to the owner; it never comes back to the asker as a failure, to be asked
            // again (paper-research N-101 p644: an Ask with no lead came back four times)
            None if self.in_ask(*t) => {
                let why = format!("nobody fills {role}");
                cx.line(Line::PassedUp { at: cx.now.clone(), t: *t, step: owed.step, from: String::new(), to: "owner".into(), why: why.clone() });
                self.up_or_owner(cx, *t, owed, why);
            }
            None => self.drive(cx, vec![Work::Throw(*t, Raised { exc: "NoWorker".into(), value: json!({"role": role, "call": owed.call}) })]),
        }
    }

    /// T's step OWED, passed up to the owner for WHY: when its call answers `Up(why)`, that is its answer, so the
    /// kind's own way up runs (the library's `question`: `consider`, then the owner's step), as when the lead passes
    /// a question up; else the step is the owner's (paper-research N-101 p1119, 1: the lead asking is an `Up` with
    /// that as its why). A record from before this, which gave the step to the owner, replays as recorded.
    fn up_or_owner(&mut self, cx: &mut Cx, t: u64, owed: Owed, why: String) {
        let ups = matches!(&owed.head, Type::Variant(cs) if cs.iter().any(|(c, fs)| c == "Up" && matches!(fs.as_slice(), [(f, Type::Text)] if f == "why")));
        let recorded = cx.recorded.as_ref().is_some_and(|r| r.iter().any(|l| matches!(l, Line::Step { t: u, step: s, .. } if *u == t && *s == owed.step)));
        if !ups || recorded {
            return self.await_step(cx, t, "owner".into(), owed);
        }
        self.tickets.edit().get_mut(&t).unwrap().status = Status::Waits { children: Vec::new(), par: false };
        cx.effect(Effect::Changed { t });
        self.drive(cx, vec![Work::Resume(t, json!({"head": "Up", "why": why}))]);
    }

    /// Policy says T's step has waited too long: it is withdrawn, and fails as Silent.
    fn use_expired(&mut self, cx: &mut Cx, t: u64) {
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let Status::Awaits { agent, owed, .. } = &x.status else { return };
        let (agent, step, call) = (agent.clone(), owed.step, owed.call.clone());
        x.withdrawn.push((agent.clone(), step, call, false, None));
        cx.effect(Effect::Deliver { to: agent.clone() });
        self.drive(cx, vec![Work::Throw(t, Raised { exc: "Silent".into(), value: json!(agent) })]);
    }

    fn accept(&mut self, cx: &mut Cx, t: u64, by: &str, head: Value, text: Text, step: u32) {
        // the call's question (`asks C`): not taken, but asked; the answer goes back to the asker (N-101)
        if self.asked_by_head(cx, t, by, &head, &text, step) {
            return;
        }
        // the program goes on with who gave the answer and the whole of it (N-101: `by of`, `text of`)
        let carried = answers::carried(head.clone(), by, &text);
        // the owner's answer to a decision counts as its answer, as a rule's does (`decide_facts`)
        if let Status::Awaits { owed, .. } = &self.tickets[&t].status {
            if let (Some(name), Some(c)) = (owed.call.strip_prefix(syntax::DECIDE), head.get("head").and_then(Value::as_str)) {
                self.decided.push((t, name.to_string(), owed.args.clone(), c.to_string()));
            }
        }
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.last_by = Some(by.to_string());
        // the agent it awaited (a hold may be answered by another)
        let awaited = match &x.status {
            Status::Awaits { agent, .. } => agent.clone(),
            _ => by.to_string(),
        };
        // the watcher's verdict is not posted: a one-off posts nothing, and the verdict is the answer line and the
        // ticket's result; nor is an answer at a hole (the minder never posts: its answer is the `shaped` line)
        let shape = matches!(&x.status, Status::Awaits { owed, .. } if owed.call == SHAPE);
        if awaited != WATCHER && !shape {
            let n = x.last() + 1;
            x.posts.push(Post { n, at: cx.now.clone(), by: by.to_string(), text, head: Some(head.clone()), step: Some(step), names: Vec::new(), told: Vec::new(), auto: false, moved: None, from: None, held: None, halted: false, dropped: false, routed: false, asks: Vec::new(), asks_open: false, asks_closed: None, answers: None, inform: Vec::new(), typed: None, cleaned_from: None, split_from: None, on: Vec::new(), also: Vec::new(), would_close: None, taken_back: false, joins: None, passes: None, comments: Vec::new(), into_turn: Vec::new(), kind: None, answering: Vec::new() });
            x.read_own(by.to_string(), n, self.era, &mut self.passed_his);
            cx.line(x.posts.last().unwrap().posted(t));
        }
        x.status = Status::Waits { children: Vec::new(), par: false };
        cx.effect(Effect::Changed { t });
        self.end_scoped(cx, t, Some(&awaited));
        self.drive(cx, vec![Work::Resume(t, carried)]);
    }

    /// Refuse an answer to STEP; the refusals left before Malformed.
    fn refuse(&mut self, cx: &mut Cx, t: u64, step: u32, why: &str) -> u32 {
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let Status::Awaits { refusals, refused, agent, forming, .. } = &mut x.status else { unreachable!() };
        *refusals += 1;
        *refused = Some((why.to_string(), false));
        *forming = None;
        let (left, agent) = (3 - *refusals, agent.clone());
        cx.line(Line::Refused { at: cx.now.clone(), t, step, why: why.to_string() });
        cx.effect(Effect::Changed { t });
        if left == 0 {
            self.drive(cx, vec![Work::Throw(t, Raised { exc: "Malformed".into(), value: json!({"step": step, "why": why}) })]);
        } else {
            cx.effect(Effect::Deliver { to: agent });
        }
        left
    }

    /// The once-edits that reach X's steps, as its view shows them (`TicketView::templates`, N-208): the one its owed
    /// step carries, its unused `step` edits, then the `tree` edits of X and the tickets above it (parent, else the
    /// ticket it is under, as `template_for` goes), nearest first, the latest per ticket and call.
    pub fn templates_seen(&self, x: &Ticket) -> Vec<crate::view::ViewTemplate> {
        use crate::view::ViewTemplate;
        let mut out = Vec::new();
        let owed = match &x.status {
            Status::Awaits { owed, .. } | Status::Asking { owed, .. } => Some(owed),
            _ => None,
        };
        if let Some(owed) = owed {
            if let Some(text) = &owed.edit {
                let tree = !self.templates.iter().any(|e| e.t == x.t && e.call == owed.call && !e.tree && e.text == *text);
                let on = if tree { self.templates.iter().rev().find(|e| e.tree && e.call == owed.call && e.text == *text).map_or(x.t, |e| e.t) } else { x.t };
                let at = self.templates.iter().rev().find(|e| e.call == owed.call && e.text == *text && e.tree == tree && (tree || e.t == x.t)).map(|e| e.at.clone()).unwrap_or_default();
                out.push(ViewTemplate { call: owed.call.clone(), on: self.id(on), tree, step: Some(owed.step), text: text.clone(), at });
            }
        }
        out.extend(self.templates.iter().filter(|e| e.t == x.t && !e.tree && !e.used).map(|e| ViewTemplate { call: e.call.clone(), on: self.id(e.t), tree: false, step: None, text: e.text.clone(), at: e.at.clone() }));
        let mut v = Some(x.t);
        while let Some(u) = v {
            let mut calls: Vec<&str> = Vec::new();
            for e in self.templates.iter().rev().filter(|e| e.t == u && e.tree) {
                if !calls.contains(&e.call.as_str()) {
                    calls.push(&e.call);
                    out.push(ViewTemplate { call: e.call.clone(), on: self.id(u), tree: true, step: None, text: e.text.clone(), at: e.at.clone() });
                }
            }
            v = self.tickets.get(&u).and_then(|y| y.parent.or(y.under));
        }
        out
    }

    /// The owner's once-edit of CALL's template for a step of it on T owed now: an unused `step` edit on T (used by
    /// this), else the latest `tree` edit on T or the nearest ticket above it.
    fn template_for(&mut self, t: u64, call: &str) -> Option<String> {
        if let Some(e) = self.templates.iter_mut().find(|e| e.t == t && e.call == call && !e.tree && !e.used) {
            e.used = true;
            return Some(e.text.clone());
        }
        let mut v = Some(t);
        while let Some(u) = v {
            if let Some(e) = self.templates.iter().rev().find(|e| e.t == u && e.call == call && e.tree) {
                return Some(e.text.clone());
            }
            v = self.tickets.get(&u).and_then(|x| x.parent.or(x.under));
        }
        None
    }

    /// The owner's once-edit of CALL's template on T (TREE: under it too) taken: a step of CALL owed and not yet
    /// given takes it now (on T, or under it for TREE); else the next one owed does.
    fn template_edit(&mut self, cx: &mut Cx, t: u64, call: &str, tree: bool, text: &str) -> String {
        let id = self.id(t);
        let mut e = TemplateEdit { t, call: call.to_string(), tree, text: text.to_string(), used: false, at: cx.now.clone() };
        let mut now: Vec<String> = Vec::new();
        let ts: Vec<u64> = self.tickets.keys().copied().filter(|u| if tree { self.is_under(*u, t) } else { *u == t }).collect();
        for u in ts {
            let uid = self.id(u);
            let x = self.tickets.edit().get_mut(&u).unwrap();
            if let Status::Awaits { owed, taken: false, .. } = &mut x.status {
                if owed.call == call && owed.template.is_none() {
                    owed.edit = Some(text.to_string());
                    now.push(format!("step {} of {uid}", owed.step));
                    cx.effect(Effect::Changed { t: u });
                    e.used = !tree;
                }
            }
            if e.used {
                break;
            }
        }
        self.templates.push(e);
        let later = if tree { format!("every later step of {call} on {id} and the tickets under it") } else if now.is_empty() { format!("the next step of {call} on {id}") } else { String::new() };
        match (now.is_empty(), later.is_empty()) {
            (false, true) => format!("Your prompt for {call} goes with {}.", now.join(", ")),
            (false, false) => format!("Your prompt for {call} goes with {}, and with {later}.", now.join(", ")),
            _ => format!("Your prompt for {call} goes with {later}."),
        }
    }

    fn await_step(&mut self, cx: &mut Cx, t: u64, agent: String, owed: Owed) {
        self.await_step_of(cx, t, agent, owed, None, None)
    }

    /// The step OWED is AGENT's (N-101, an agent is its conversation): an agent the program made (`let agent`, `fresh`)
    /// whose block or step has ended is started again on its own conversation, in the layer of T's step (coordd: the
    /// session it kept, `roster restart`; a boxed one keeps its uid, home and transcript), and lives for this step: it
    /// ends when the step is answered, or when T closes.
    fn await_or_restart(&mut self, cx: &mut Cx, t: u64, agent: String, owed: Owed) {
        if !self.ended(&agent) {
            return self.await_step(cx, t, agent, owed);
        }
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.live.push(agent.clone());
        cx.line(Line::Agent { at: cx.now.clone(), t, agent: agent.clone(), what: "restarted".into() });
        let call = owed.call.clone();
        self.await_step_of(cx, t, agent, owed, Some(call), None)
    }

    /// The results `result of` reads in node P (its own expressions, not those of the programs inside it, which run
    /// later): each ticket named, by id, with what it closed with; NoResult when one is open, failed or not there.
    fn results(&self, p: &Prog, env: &Env, ctx: &Value) -> Result<Map<String, Value>, Raised> {
        let mut out = Map::new();
        let mut found = Vec::new();
        for e in node_exprs(p) {
            result_ofs(e, &mut found);
        }
        for e in found {
            let v = eval(e, env, ctx);
            let id = v.as_str().unwrap_or_default().to_string();
            let why = match parse_ticket(&id).and_then(|t| self.tickets.get(&t).filter(|x| self.id(x.t) == id)) {
                Some(x) => match &x.status {
                    Status::Done(r) => {
                        out.insert(id, r.clone());
                        continue;
                    }
                    Status::Failed(r) => format!("{id} failed ({})", r.exc),
                    _ => format!("{id} is open"),
                },
                None => format!("there is no ticket {}", crate::print::value(&v)),
            };
            return Err(Raised { exc: "NoResult".into(), value: json!({"head": "NoResult", "ticket": id, "why": why}) });
        }
        Ok(out)
    }

    /// Whether AGENT is one the program made (fT-N: T's Nth) that has ended: no open ticket keeps it (`let agent`) or
    /// has it live (a step's).
    pub fn ended(&self, agent: &str) -> bool {
        let Some((t, n)) = agent.strip_prefix('f').and_then(|r| r.split_once('-')).and_then(|(t, n)| Some((t.parse::<u64>().ok()?, n.parse::<u32>().ok()?))) else { return false };
        (self.taken.contains(agent) || self.tickets.get(&t).is_some_and(|x| n >= 1 && n <= x.scoped))
            && !self.tickets.values().any(|x| x.is_open() && (x.kept.iter().any(|a| a == agent) || x.live.iter().any(|a| a == agent)))
    }

    /// The step OWED is AGENT's; START, the call a fresh agent made for it is started for. On a slow ticket it halts
    /// before it is given (a step to the owner, or a hold, which is not given, does not), and the fresh agent is
    /// started only once the owner approves it.
    fn await_step_of(&mut self, cx: &mut Cx, t: u64, agent: String, mut owed: Owed, start: Option<String>, from: Option<String>) {
        // the owner's once-edit of its call's template, if one reaches it (an inline call has no template to replace)
        if owed.template.is_none() && owed.edit.is_none() {
            owed.edit = self.template_for(t, &owed.call);
        }
        // a step coordd answers itself (the watcher's calls, its own minder) is not halted (the owner N-94, 3)
        let halts = !owed.hold && agent != "owner" && !coordds_own(&agent) && self.slow(t);
        // his question about the posts its handler holds (`asked_about`); on replay as its line says, so a record from
        // before, whose posts held back what came after them, replays so
        let asked = match &cx.recorded {
            Some(rec) => rec.iter().any(|l| matches!(l, Line::Step { t: u, step, asked: true, .. } if *u == t && *step == owed.step)),
            None => agent == "owner" && Self::question_about_held(&self.tickets[&t], &owed),
        };
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.step_calls.insert(owed.step, (owed.call.clone(), agent.clone()));
        cx.line(Line::Step { at: cx.now.clone(), t, step: owed.step, worker: agent.clone(), call: owed.call.clone(), hold: owed.hold, asked });
        if owed.args.as_object().is_some_and(|a| !a.is_empty()) {
            cx.line(Line::StepArgs { at: cx.now.clone(), t, step: owed.step, args: owed.args.clone() });
        }
        if let Some(a) = owed.args.as_object() {
            let last = self.step_args.entry(t).or_default();
            for (k, v) in a.iter().filter(|(_, v)| !v.is_null()) {
                last.insert(k.clone(), v.clone());
            }
        }
        let (hold, step) = (owed.hold, owed.step);
        let lapses = agent == "owner" && !hold;
        let call = owed.call.clone();
        x.status = Status::Awaits { agent: agent.clone(), owed, since: cx.now.clone(), taken: false, refusals: 0, refused: None, reminded: None, back: None, forming: None };
        if lapses && self.call_of(t, &call).is_some_and(|c| c.default.is_some()) {
            self.lapsing.insert(t);
        }
        if halts {
            self.halt(cx, t, Halting::Give { step, start, from });
        } else {
            if let Some(call) = start {
                cx.effect(self.start_of(agent.clone(), t, call, from));
            }
            if !hold {
                cx.effect(Effect::Deliver { to: agent });
            }
        }
        if asked {
            self.asked_about(cx, t);
        }
        cx.effect(Effect::Changed { t });
    }

    /// The answer HEAD (from TEXT) to STEP of T by BY: accepted, or on a slow ticket halted for the owner first (the
    /// owner's own answer is not). A later answer to the same step takes the place of one still halted.
    fn take_answer(&mut self, cx: &mut Cx, t: u64, by: &str, head: Value, text: Text, step: u32) {
        // kept while no line bears it out, by the step's `until` (`until_waits`); then put to the policy when it
        // declares duties: does this answer break one (N-101, violated.rs)
        if self.until_waits(cx, t, by, &head, &text) || self.ask_violated(cx, t, by, &head, &text, step) {
            return;
        }
        self.take_answer_now(cx, t, by, head, text, step);
    }

    /// Whether BY's answer HEAD (from TEXT) to T's step waits for a line to bear it out: an `until` of the step
    /// concerns it ([`Owed::untils`]) and no line kept bears it out for any. It is then kept on the step
    /// ([`Owed::unborne`]), in place of one kept before, until a line does ([`Machine::until_answer`]); its worker may
    /// answer again meanwhile.
    fn until_waits(&mut self, cx: &mut Cx, t: u64, by: &str, head: &Value, text: &Text) -> bool {
        let Some(x) = self.tickets.get(&t) else { return false };
        let Status::Awaits { owed, .. } = &x.status else { return false };
        let untils: Vec<&syntax::On> = owed.untils(head).collect();
        if untils.is_empty() || untils.iter().any(|o| self.bears_out(x, owed, o, head, None).is_some()) {
            return false;
        }
        if let Status::Awaits { owed, forming, .. } = &mut self.tickets.edit().get_mut(&t).unwrap().status {
            owed.unborne = Some(UntilAnswer { by: by.to_string(), head: head.clone(), text: text.clone(), at: cx.now.clone() });
            *forming = None;
        }
        cx.effect(Effect::Changed { t });
        true
    }

    /// What the answer kept on T's step waits for ([`Owed::unborne`]): a line of the events of the `until`s that
    /// concern it, as said to its worker.
    pub fn unborne_for(&self, t: u64) -> Option<String> {
        let Status::Awaits { owed, .. } = &self.tickets.get(&t)?.status else { return None };
        let u = owed.unborne.as_ref()?;
        Some(owed.untils(&u.head).map(|o| format!("a {} line", o.ev)).collect::<Vec<_>>().join(" or "))
    }

    /// The time of a line that bears out answer HEAD to X's step OWED by its `until` O (his window's has none), if
    /// one does: O's `where` holds on it, read with HEAD's fields it names after the step's scope
    /// ([`Machine::on_line`]).
    fn bears_out(&self, x: &Ticket, owed: &Owed, o: &syntax::On, head: &Value, line: Option<(&str, &At, &Value)>) -> Option<Option<At>> {
        let mut env = owed.env.clone();
        env.insert("this".into(), json!(self.id(x.t)));
        for f in &o.fields {
            env.insert(f.clone(), head.get(f).cloned().unwrap_or(Value::Null));
        }
        self.on_line(o, line, &env, &x.ctx, |at, _| Some(at.cloned()))
    }

    /// T's step answered with the answer kept on it ([`Owed::unborne`]) once a line bears it out by an `until` of its:
    /// LINE when given, else each line kept. Taken as its worker gave it, the policy's duties asked as for any answer.
    fn until_answer(&mut self, cx: &mut Cx, t: u64, line: Option<(&str, &At, &Value)>) {
        let Some(x) = self.tickets.get(&t).filter(|x| x.is_open()) else { return };
        let Status::Awaits { owed, .. } = &x.status else { return };
        let Some(u) = &owed.unborne else { return };
        if !owed.untils(&u.head).any(|o| self.bears_out(x, owed, o, &u.head, line).is_some()) {
            return;
        }
        let (step, u) = (owed.step, u.clone());
        if let Status::Awaits { owed, .. } = &mut self.tickets.edit().get_mut(&t).unwrap().status {
            owed.unborne = None;
        }
        self.take_answer(cx, t, &u.by, u.head, u.text, step);
    }

    /// As `take_answer`, the policy's duties not asked (again).
    fn take_answer_now(&mut self, cx: &mut Cx, t: u64, by: &str, head: Value, text: Text, step: u32) {
        if by == "owner" || !self.slow(t) {
            return self.accept(cx, t, by, head, text, step);
        }
        // coordd's own answer is taken at once (the owner N-94, 3), except its minder's Program or Close at a hole: that
        // changes the program or ends the ticket, and halts; Rest halts nothing
        if coordds_own(by) {
            let shape = matches!(&self.tickets[&t].status, Status::Awaits { owed, .. } if owed.call == SHAPE);
            let changes = matches!(head.get("head").and_then(Value::as_str), Some("Program" | "Close"));
            if !(by == MINDER && shape && changes) {
                return self.accept(cx, t, by, head, text, step);
            }
        }
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.halts.retain(|h| !matches!(h.what, Halting::Take { step: s, .. } if s == step));
        if let Status::Awaits { forming, .. } = &mut x.status {
            *forming = None;
        }
        self.halt(cx, t, Halting::Take { step, by: by.into(), head, text });
    }

    /// The steps the record's own lines answer, once LINE is carried out (its lines and facts kept first, [`Seen`]):
    /// the `in_window` steps ([`IN_WINDOW`]) the last [`WINDOW_RELEASE`] line kept answers, and those a system event's
    /// line answers by their `on` ([`Machine::on_answer`]). After a line that is an event ([`Machine::event_of`]),
    /// those of every ticket that awaits one it may answer, in the tickets' order, each reading that line alone, for it
    /// read those before at its step; after a line the facts change by, those of every ticket that awaits one whose
    /// `on` reads them, each reading every line kept again (a cut made before the merged line of its commit); after
    /// any line, those its apply reached (its `step` lines), each reading every line kept; and so on for those the
    /// answers reach in turn.
    fn line_answers(&mut self, cx: &mut Cx, line: &Line) {
        let raised = self.event_of(line);
        let facts = self.seen.keep(&cx.now, line, raised.as_ref().map(|(ev, v)| (*ev, v)));
        let mut ts: Vec<(u64, bool)> = match (&raised, facts) {
            (None, false) => Vec::new(),
            _ => {
                let takes = |o: &Owed| raised.as_ref().is_some_and(|(ev, _)| (*ev == WINDOW_RELEASE && o.call == IN_WINDOW) || o.on.iter().any(|on| on.ev == *ev));
                let again = |o: &Owed| facts && o.on.iter().any(syntax::On::reads_facts);
                self.tickets.values().filter_map(|x| match &x.status {
                    Status::Awaits { owed, .. } if again(owed) => Some((x.t, false)),
                    Status::Awaits { owed, .. } if takes(owed) => Some((x.t, true)),
                    _ => None,
                }).collect()
            }
        };
        let now = cx.now.clone();
        let mut read = 0;
        loop {
            ts.extend(cx.lines[read..].iter().filter_map(|l| match l {
                Line::Step { t, .. } => Some((*t, false)),
                _ => None,
            }));
            read = cx.lines.len();
            if ts.is_empty() {
                return;
            }
            for (t, at_line) in std::mem::take(&mut ts) {
                let line = raised.as_ref().filter(|_| at_line).map(|(ev, v)| (*ev, &now, v));
                self.in_window_answer(cx, t);
                self.on_answer(cx, t, line);
                self.until_answer(cx, t, line);
            }
        }
    }

    /// The system event LINE is, its name and value: his window's ([`WINDOW_RELEASE`], as the machine keeps it), the
    /// git round's ([`MERGED`]), a goal's duties ([`DUTIES`], the policy's answer to its question), or an act that is
    /// one (`syntax::ACT_EVENTS`), its value typed (`syntax::system_event`); none for any other line, or an act whose
    /// fields do not fit its event's type.
    fn event_of(&self, line: &Line) -> Option<(&'static str, Value)> {
        match line {
            Line::Decided { question, answer, .. } if question == duties::QUESTION => Some((DUTIES, duties::duties_value(answer)?)),
            Line::Event { name, .. } if name == WINDOW_RELEASE => Some((WINDOW_RELEASE, self.window_release.as_ref()?.value())),
            Line::Event { name, value, .. } if name == MERGED => Some((MERGED, value.clone())),
            Line::Act { t, agent, is, text, more, .. } => {
                let ev = syntax::act_event(is, more.get("phase").and_then(Value::as_str))?;
                let ty = syntax::system_event(ev)?;
                let value = ty.coerce(&crate::runner::act_value(agent, is, text, t.map(|t| self.id(t)), more)).ok()?;
                ty.conforms(&value).ok().map(|()| (ev, value))
            }
            _ => None,
        }
    }

    /// T's step answered by T with the answer of its first `on` ([`syntax::On`]) whose `where` holds on a line of its
    /// event ([`Machine::on_line`]). Taken at once, as `in_window`'s is, for it is a line of the record's and no one's
    /// judgment.
    fn on_answer(&mut self, cx: &mut Cx, t: u64, line: Option<(&str, &At, &Value)>) {
        let Some(x) = self.tickets.get(&t).filter(|x| x.is_open()) else { return };
        let Status::Awaits { owed, .. } = &x.status else { return };
        // its `where` reads THIS, the ticket, as a handler's does (`e.goal == this`)
        let mut env = owed.env.clone();
        env.insert("this".into(), json!(self.id(t)));
        let found = owed.on.iter().find_map(|o| {
            let answer = o.answer.as_ref()?;
            self.on_line(o, line, &env, &x.ctx, |at, env| {
                let mut answer = answer.clone();
                self.seen.read(&mut answer, env, &x.ctx);
                let head = owed.head.coerce(&eval(&answer, env, &x.ctx)).ok().filter(|h| owed.head.conforms(h).is_ok())?;
                Some((head, at.map_or_else(|| format!("The {} line answers it.", o.ev), |at| format!("The {} line of {at} answers it.", o.ev))))
            })
        });
        let Some((head, text)) = found else { return };
        let step = owed.step;
        let by = self.id(t);
        self.accept(cx, t, &by, head, Text::New(text), step);
    }

    /// What THEN takes of the first line of O's event on which O's `where` holds, read in ENV with O's X bound to the
    /// line's value and its facts read from the lines kept ([`Seen::read`]), given the line's time and that ENV: LINE
    /// alone when given, the event's line just carried out; else each line kept, the newest first ([`Seen::lines`];
    /// his window's last, which has no time here).
    fn on_line<T>(&self, o: &syntax::On, line: Option<(&str, &At, &Value)>, env: &Env, ctx: &Value, mut then: impl FnMut(Option<&At>, &Env) -> Option<T>) -> Option<T> {
        let window = self.window_release.as_ref().map(WindowRelease::value);
        let lines: Vec<(Option<&At>, &Value)> = match line {
            _ if o.ev == WINDOW_RELEASE => window.iter().filter(|_| line.is_none_or(|(ev, _, _)| ev == o.ev)).map(|v| (None, v)).collect(),
            Some((ev, at, v)) if ev == o.ev => vec![(Some(at), v)],
            Some(_) => Vec::new(),
            None => self.seen.lines(&o.ev).into_iter().map(|(at, v)| (Some(at), v)).collect(),
        };
        lines.into_iter().find_map(|(at, value)| {
            let mut env = env.clone();
            env.insert(o.x.clone(), value.clone());
            let mut cond = o.cond.clone();
            if let Some(c) = cond.as_mut() {
                self.seen.read(c, &env, ctx);
            }
            if cond.as_ref().is_some_and(|c| eval(c, &env, ctx) != json!(true)) {
                return None;
            }
            then(at, &env)
        })
    }

    /// T's `in_window` step answered by T, when T awaits one and the last window's line has its cut in his window: taken
    /// at once, as coordd's own answers are, for it is a fact of his window and no one's judgment.
    fn in_window_answer(&mut self, cx: &mut Cx, t: u64) {
        let Some(w) = &self.window_release else { return };
        let Some(Status::Awaits { owed, .. }) = self.tickets.get(&t).filter(|x| x.is_open()).map(|x| &x.status) else { return };
        let Some(name) = owed.args.get("name").and_then(Value::as_str).filter(|_| owed.call == IN_WINDOW) else { return };
        let engine = owed.args.get("engine").and_then(Value::as_bool) == Some(true);
        if !w.in_window(name, engine) {
            return;
        }
        let Ok(head) = owed.head.coerce(&json!({"head": "Has", "release": w.window.release, "contains": w.window.contains})) else { return };
        let text = match (&w.engine, engine) {
            (Some(e), true) => format!("His window runs {} and coordd runs {}: both have {name}.", w.window.release, e.release),
            _ => format!("His window runs {}, which has {name}.", w.window.release),
        };
        let step = owed.step;
        let by = self.id(t);
        self.accept(cx, t, &by, head, Text::New(text), step);
    }

    fn drive(&mut self, cx: &mut Cx, mut work: Vec<Work>) {
        while let Some(w) = work.pop() {
            // a ticket closed meanwhile (a sibling of `any` cancelled before its branch ran) does nothing more
            let t = match &w {
                Work::Run(t, ..) | Work::Resume(t, _) | Work::Throw(t, _) => *t,
            };
            if !self.tickets.get(&t).is_some_and(Ticket::is_open) {
                continue;
            }
            match w {
                Work::Run(t, p, env) => self.run(cx, t, p, env, &mut work),
                Work::Resume(t, v) => {
                    let x = self.tickets.edit().get_mut(&t).unwrap();
                    x.trace.came_back(answers::plain(v.clone()));
                    match x.k.pop() {
                        Some(Frame::Bind { x: b, rest, mut env }) => {
                            bind(&b, v, &mut env);
                            work.push(Work::Run(t, rest, env));
                        }
                        Some(Frame::Loop { x: name, left, measure, body, env }) => {
                            let z = v.get("value").cloned().unwrap_or(Value::Null);
                            match v.get("head").and_then(Value::as_str) {
                                Some("Done") => work.push(Work::Resume(t, z)),
                                _ => {
                                    let m = v.get("measure").and_then(Value::as_i64);
                                    if let (Some(m), Some(last)) = (m, measure) {
                                        if m >= last {
                                            work.push(Work::Throw(t, Raised { exc: "Stalled".into(), value: z }));
                                            continue;
                                        }
                                    }
                                    if left == 0 {
                                        // as a stop (stop.rs), Again is one round more from where the last left off
                                        let x = self.tickets.edit().get_mut(&t).unwrap();
                                        if x.handling.is_none() {
                                            let mut again = env.clone();
                                            again.insert(name.clone(), z.clone());
                                            let one = Prog::Iterate { x: name.clone(), init: syntax::Expr::Var(name.clone()), n: 1, at: body.at(), body: Box::new(body.clone()) };
                                            x.last_node = Some((one, again, x.steps));
                                        }
                                        work.push(Work::Throw(t, Raised { exc: "Exhausted".into(), value: z }));
                                        continue;
                                    }
                                    let mut benv = env.clone();
                                    benv.insert(name.clone(), z);
                                    let x = self.tickets.edit().get_mut(&t).unwrap();
                                    let left = if left == syntax::UNBOUNDED { left } else { left - 1 };
                                    x.k.push(Frame::Loop { x: name, left, measure: m.or(measure), body: body.clone(), env });
                                    work.push(Work::Run(t, body, benv));
                                }
                            }
                        }
                        // the layer's scope ends (coordd ends the view, `active_layers`); its value is its body's, as
                        // with `let agent` and `with` (paper-research's ruling, plan/layers.md, after N-92)
                        Some(Frame::Layer { .. }) => work.push(Work::Resume(t, v)),
                        Some(Frame::Each { x: name, mut rest, mut done, body, env }) => {
                            done.push(v);
                            if rest.is_empty() {
                                work.push(Work::Resume(t, Value::Array(done)));
                            } else {
                                let mut benv = env.clone();
                                benv.insert(name.clone(), rest.remove(0));
                                x.k.push(Frame::Each { x: name, rest, done, body: body.clone(), env });
                                work.push(Work::Run(t, body, benv));
                            }
                        }
                        // a value leaves a try as it is
                        Some(Frame::Try { .. }) => work.push(Work::Resume(t, v)),
                        // the owner's step of `gather` answered (by itself, or his press): his burst, every post held now
                        Some(Frame::Gather) => {
                            let burst = match self.tickets[&t].post_of.clone() {
                                Some(p) => self.owner_value(p.t, &p.posts()),
                                None => self.tickets[&t].form.get("text").cloned().unwrap_or(v),
                            };
                            work.push(Work::Resume(t, burst));
                        }
                        Some(Frame::With { ctx }) => {
                            x.ctx = ctx;
                            work.push(Work::Resume(t, v));
                        }
                        Some(Frame::Scope { agent }) => {
                            self.end_kept(cx, t, &agent);
                            work.push(Work::Resume(t, v));
                        }
                        // the body's value is the whole's; the look still running ends with it
                        Some(Frame::During { busy, asks, .. }) => {
                            self.end_look(cx, busy, "the block it watched ended");
                            for q in asks {
                                self.ask_after_look(cx, q);
                            }
                            work.push(Work::Resume(t, v));
                        }
                        // Owed: a ticket with goals closes done only when policy says they hold
                        None if !x.goals.is_empty() => {
                            let goals = x.goals.clone();
                            x.holding = Some(v);
                            // asked as the number of the close it is about, so an answer to an earlier ask is refused
                            let step = self.tickets[&t].closes + 1;
                            cx.line(Line::Asked { at: cx.now.clone(), t, step, question: "holds".into(), role: goals.join(", ") });
                            cx.effect(Effect::Ask { t, step, question: Question::Holds { goals } });
                            cx.effect(Effect::Changed { t });
                        }
                        None => self.close(cx, t, Status::Done(v), &mut work),
                    }
                }
                Work::Throw(t, r) => {
                    // a failure of the engine's that nothing above takes: it stops there (stop.rs)
                    if self.stops_at(cx, t, &r) {
                        self.stop(cx, t, r, false, &mut work);
                        continue;
                    }
                    // unwind to the nearest try with an arm for it; none: the ticket fails
                    let x = self.tickets.edit().get_mut(&t).unwrap();
                    x.trace.came_back(json!({"raised": r.exc}));
                    let mut caught = None;
                    let mut left = Vec::new();
                    let mut looks = Vec::new();
                    let mut held = Vec::new();
                    while let Some(f) = x.k.pop() {
                        match f {
                            Frame::Try { arms, env } => {
                                if let Some(h) = arms.into_iter().find(|h| h.exc == r.exc) {
                                    caught = Some((h, env));
                                    break;
                                }
                            }
                            Frame::With { ctx } => x.ctx = ctx,
                            Frame::Scope { agent } => left.push(agent),
                            Frame::During { busy, asks, .. } => {
                                looks.extend(busy);
                                held.extend(asks);
                            }
                            _ => {}
                        }
                    }
                    // the scopes it leaves end their agents, and the looks of the `during`s it leaves
                    for a in left {
                        self.end_kept(cx, t, &a);
                    }
                    for q in held {
                        self.ask_after_look(cx, q);
                    }
                    for c in looks {
                        self.end_look(cx, Some(c), "the block it watched raised");
                    }
                    let x = self.tickets.edit().get_mut(&t).unwrap();
                    match caught {
                        Some((h, mut env)) => {
                            if let Some(name) = h.x {
                                env.insert(name, r.value.clone());
                            }
                            x.status = Status::Waits { children: Vec::new(), par: false };
                            cx.effect(Effect::Changed { t });
                            // a fresh agent whose step failed (Silent, Malformed) ends with it
                            self.end_scoped(cx, t, None);
                            work.push(Work::Run(t, h.body, env));
                        }
                        None => self.close(cx, t, Status::Failed(r), &mut work),
                    }
                }
            }
        }
    }

    fn run(&mut self, cx: &mut Cx, t: u64, p: Prog, env: Env, work: &mut Vec<Work>) {
        // where it runs again from, should it stop here (`stop`); a run that handles an event never stops (`stops_at`
        // returns before it reads this), so it keeps none: a delivery run's env holds what it was given, which its form
        // holds already (paper-research N-1446 p1936)
        let x = self.tickets.edit().get_mut(&t).unwrap();
        if x.handling.is_none() {
            x.last_node = Some((p.clone(), env.clone(), x.steps));
        }
        // what the program reads of its context, and the ticket itself for `post N` (never stored)
        let mut ctx = self.tickets[&t].ctx.clone();
        if let Value::Object(o) = &mut ctx {
            o.insert("#t".into(), json!(t));
        }
        // what `result of` reads in this node: the results of the tickets it names, or NoResult
        match self.results(&p, &env, &ctx) {
            Ok(rs) if !rs.is_empty() => {
                if let Value::Object(o) = &mut ctx {
                    o.insert("#results".into(), Value::Object(rs));
                }
            }
            Ok(_) => {}
            Err(r) => return work.push(Work::Throw(t, r)),
        }
        // `text of`/`by of` on what no step gave: NoResult, as `result of` on an open ticket (N-101)
        let mut reads = Vec::new();
        for e in node_exprs(&p) {
            answers::reads(e, &mut reads);
        }
        // a record from before `text of` kept held texts (a Text::Joined) has the NoResult it raised then: as recorded
        let raised_then = |e: &Expr| {
            let value = answers::missing(e, &eval(e.inner(), &env, &ctx));
            let joined = Text::of(&eval(e, &env, &ctx)).is_some_and(|x| matches!(x, Text::Joined { .. }));
            joined && cx.recorded.as_ref().is_some_and(|r| r.iter().any(|l| matches!(l, Line::Closed { t: c, raised: Some(r), .. } | Line::Stopped { t: c, raised: r, .. } if *c == t && r.exc == "NoResult" && r.value == value)))
        };
        if let Some(e) = reads.into_iter().find(|e| eval(e, &env, &ctx).is_null() || raised_then(e)) {
            let value = answers::missing(e, &eval(e.inner(), &env, &ctx));
            return work.push(Work::Throw(t, Raised { exc: "NoResult".into(), value }));
        }
        // a `find` that finds nothing: NotFound, before anything else is done (N-101 p226, G6)
        let mut missed = Vec::new();
        for e in node_exprs(&p) {
            not_found(e, &env, &ctx, &mut missed);
        }
        if let Some(Expr::Find { x, e, cond }) = missed.first() {
            let value = json!({"head": "NotFound", "find": crate::print::expr(&Expr::Find { x: x.clone(), e: e.clone(), cond: cond.clone() })});
            return work.push(Work::Throw(t, Raised { exc: "NotFound".into(), value }));
        }
        let key = self.enter(t, &p, &env, &ctx, &cx.now);
        // an `any` over a list: its N (the arm below is shared with `par`)
        let any = if let Prog::Any { n, each: Some(_), .. } = &p { Some(*n) } else { None };
        // where it waits, should it come to wait on tickets below it here (what an edit then is checked in)
        if let Prog::Open { wait, .. } | Prog::Par { wait, .. } | Prog::Any { wait, .. } = &p {
            self.tickets.edit().get_mut(&t).unwrap().waiting = Some((wait.clone(), env.clone()));
        }
        match p {
            Prog::Return { e, .. } => work.push(Work::Resume(t, eval(&e, &env, &ctx))),
            Prog::Raise { exc, e, .. } => work.push(Work::Throw(t, Raised { exc, value: e.map_or(Value::Null, |e| answers::plain(eval(&e, &env, &ctx))) })),
            Prog::Bind { x, first, rest } => {
                self.tickets.edit().get_mut(&t).unwrap().k.push(Frame::Bind { x, rest: *rest, env: env.clone() });
                work.push(Work::Run(t, *first, env));
            }
            Prog::Try { body, arms, .. } => {
                self.tickets.edit().get_mut(&t).unwrap().k.push(Frame::Try { arms, env: env.clone() });
                work.push(Work::Run(t, *body, env));
            }
            Prog::LetAgent { x: name, from, body, .. } => {
                // `let agent x = fork a`: the agent it copies, named before anything is made
                let from = match from.as_ref().map(|a| fork::fork_from(a, &env, &ctx)).transpose() {
                    Ok(from) => from,
                    Err(r) => return work.push(Work::Throw(t, r)),
                };
                let agent = self.next_scoped(t);
                self.made.push(Made { agent: agent.clone(), by: String::from("program"), from: from.clone(), t: Some(t), name: Some(name.clone()) });
                let x = self.tickets.edit().get_mut(&t).unwrap();
                x.kept.push(agent.clone());
                x.k.push(Frame::Scope { agent: agent.clone() });
                cx.line(match &from {
                    Some(from) => Line::Forked { at: cx.now.clone(), t, agent: agent.clone(), from: from.clone() },
                    None => Line::Agent { at: cx.now.clone(), t, agent: agent.clone(), what: "made".into() },
                });
                let call = check::first_call(&body, &name).unwrap_or_default();
                cx.effect(self.start_of(agent.clone(), t, call, from));
                let mut env = env;
                env.insert(name, json!(agent));
                work.push(Work::Run(t, *body, env));
            }
            Prog::During { body, whens, scope, .. } => {
                self.tickets.edit().get_mut(&t).unwrap().k.push(Frame::During { whens, scope, env: env.clone(), busy: None, missed: None, asks: Vec::new() });
                work.push(Work::Run(t, *body, env));
            }
            // as `coord esc`: the agent's running turn ends (coordd holds back what is due to it until the stop lands,
            // and sends none to an agent out of a turn); what it owes stays owed
            Prog::Stop { e, .. } => {
                // a copy the run's delivery made (`fork … at`, delivery.rs): it ends, its session with it (N-1104 p1154)
                if let Some(c) = eval(&e, &env, &ctx).as_str().filter(|c| self.copy_in_run(t, c)) {
                    self.stop_copy(cx, t, c);
                    return work.push(Work::Resume(t, Value::Null));
                }
                // in the record as the agent's line `stopped` (N-101's stand-in run found it written nowhere)
                if let Some(agent) = eval(&e, &env, &ctx).as_str() {
                    cx.line(Line::Agent { at: cx.now.clone(), t, agent: agent.to_string(), what: "stopped".into() });
                    cx.effect(Effect::Interrupt { agent: agent.to_string() });
                    // a question of its that this look holds is not asked (ask_minded.rs)
                    self.ask_stopped(t, agent);
                    // its answer being put to the policy counts as never given (violated.rs)
                    self.drop_violated(cx, agent);
                }
                work.push(Work::Resume(t, Value::Null));
            }
            Prog::Decide { name, args, .. } => {
                let call = format!("{}{name}", syntax::DECIDE);
                let Some(sig) = self.call_of(t, &call) else {
                    return work.push(Work::Throw(t, Raised { exc: "NoCall".into(), value: json!({"call": call}) }));
                };
                let mut a = Map::new();
                for (i, (n, _)) in sig.params.iter().enumerate() {
                    a.insert(n.clone(), args.get(i).map_or(Value::Null, |e| answers::plain(eval(e, &env, &ctx))));
                }
                let x = self.tickets.edit().get_mut(&t).unwrap();
                x.steps += 1;
                let owed = Owed { step: x.steps, call, args: Value::Object(a.clone()), blind: false, head: sig.head.clone(), hold: false, scope: BTreeMap::new(), env: env.clone(), template: None, refs: Vec::new(), prompt: None, edit: None, on: Vec::new(), unborne: None };
                cx.line(Line::Asked { at: cx.now.clone(), t, step: owed.step, question: "decide".into(), role: POLICY.into() });
                cx.effect(Effect::Ask { t, step: owed.step, question: Question::Decide { name, args: Value::Object(a) } });
                cx.effect(Effect::Changed { t });
                x.status = Status::Asking { owed, role: POLICY.into() };
            }
            Prog::With { f, e, body, .. } => {
                let v = eval(&e, &env, &ctx);
                let x = self.tickets.edit().get_mut(&t).unwrap();
                let before = x.ctx.clone();
                if let Value::Object(o) = &mut x.ctx {
                    o.insert(f, v);
                } else {
                    x.ctx = json!({ f: v });
                }
                x.k.push(Frame::With { ctx: before });
                work.push(Work::Run(t, *body, env));
            }
            Prog::Match { e, arms, guards, .. } => {
                let v = eval(&e, &env, &ctx);
                for (i, (pat, body)) in arms.into_iter().enumerate() {
                    let mut env2 = env.clone();
                    if matches(&pat, &v, &mut env2) {
                        // an arm's `where`: taken only when it holds, with the pattern's names bound
                        if guards.get(i).and_then(Option::as_ref).is_some_and(|c| eval(c, &env2, &ctx) != json!(true)) {
                            continue;
                        }
                        if let Some(n) = key.and_then(|k| self.tickets.edit().get_mut(&t).unwrap().trace.visits.get_mut(&k)) {
                            n.arm = Some(i);
                        }
                        work.push(Work::Run(t, body, env2));
                        return;
                    }
                }
                work.push(Work::Throw(t, Raised { exc: "Stuck".into(), value: json!({"unmatched": v}) }));
            }
            Prog::Post { e, on, by, naming, informing, scope, .. } => {
                let v = match &e {
                    Expr::Str(s) if !syntax::placeholders(s).is_empty() => Value::String(fill_post(s, &env, &scope)),
                    _ => eval(&e, &env, &ctx),
                };
                let text = Text::of(&v).unwrap_or(Text::New(v.to_string()));
                // in the handler of a project's event, a post goes on the ticket the event was sent about, by its
                // sender (the runner's outcome, a reply under its line: plan/layers.md I §3)
                let sent = {
                    let x = &self.tickets[&t];
                    x.handling.as_deref().filter(|e| syntax::ticket_event(e).is_none() && syntax::system_event(e).is_none()).and(x.under.zip(x.raised_by.clone()))
                };
                // in the handler of a ticket event (a `during`'s look, a `when act`), the post goes on the ticket the
                // handler belongs to, by that ticket: its hidden ticket is plumbing nobody is shown (N-101.p64)
                // (a record from before this put such a post on the hidden ticket itself: replayed as it was, N-170)
                let home = {
                    let x = &self.tickets[&t];
                    // the program's own post on it: by the hidden ticket, answering no step
                    let id = self.id(t);
                    let old = cx.recorded.as_ref().is_some_and(|r| r.iter().any(|l| matches!(l, Line::Posted { t: u, by: b, step: None, .. } if *u == t && *b == id)));
                    x.handling.as_deref().filter(|e| syntax::ticket_event(e).is_some() || syntax::system_event(e).is_some()).and(x.under).filter(|_| !old).unwrap_or(t)
                };
                let target = match on.map(|o| eval(&o, &env, &ctx)) {
                    None => sent.as_ref().map_or(home, |(u, _)| *u),
                    Some(o) => match o.as_str().and_then(parse_ticket).filter(|u| self.tickets.contains_key(u)) {
                        Some(u) => u,
                        None => {
                            work.push(Work::Throw(t, Raised { exc: "Stuck".into(), value: json!({"post on": o, "why": "no such ticket"}) }));
                            return;
                        }
                    },
                };
                let by_ticket = by.is_none() && sent.is_none();
                let by = match by.as_deref() {
                    None => sent.map_or_else(|| self.id(home), |(_, a)| a),
                    Some("owner") => "owner".to_string(),
                    Some(w) => env.get(w).and_then(Value::as_str).map_or(w.to_string(), str::to_string),
                };
                let mut names: Vec<String> = Vec::new();
                for n in &naming {
                    match eval(n, &env, &ctx) {
                        Value::String(a) => names.push(a),
                        Value::Array(xs) => names.extend(xs.iter().filter_map(|a| a.as_str().map(str::to_string))),
                        _ => {}
                    }
                }
                names.retain(|a| *a != by);
                names.dedup();
                // whom it informs: at their next delivery, starting no turn (`Post.inform`), or into the turn that holds
                // the thread it stands at (`Post.into_turn`: its words ask it for whom they inform, policy's inform
                // never, even when policy adds an agent to the same post; proto-reader N-1446 p177, consult-3 p181);
                // one it names it wakes
                let mut inform: Vec<String> = Vec::new();
                for n in &informing {
                    match eval(n, &env, &ctx) {
                        Value::String(a) => inform.push(a),
                        Value::Array(xs) => inform.extend(xs.iter().filter_map(|a| a.as_str().map(str::to_string))),
                        _ => {}
                    }
                }
                inform.retain(|a| *a != by && !names.contains(a));
                inform.dedup();
                let held = self.formed_held(cx);
                // his words placed on the thread by intake hand the posts he left held there, as his words posted on
                // it do (paper-research N-412.p1356, 2); a handler's own post as him (the old `Again` arm's, replayed
                // from a record) hands nothing
                if by == "owner" && !held && self.tickets[&t].post_of.is_none() && self.tickets[&target].is_open() {
                    self.hand_unanswered(cx, target);
                }
                if by == "owner" && self.form_owner_words(cx, target, &text, &names, &[], &[], held).is_some() {
                    work.push(Work::Resume(t, Type::unit_value()));
                    return;
                }
                let x = self.tickets.edit().get_mut(&target).unwrap();
                let n = x.last() + 1;
                x.posts.push(Post { n, at: cx.now.clone(), by: by.clone(), text, head: None, step: None, names: names.clone(), told: Vec::new(), auto: false, moved: None, from: None, held: None, halted: false, dropped: false, routed: false, asks: Vec::new(), asks_open: false, asks_closed: None, answers: None, inform: inform.clone(), typed: None, cleaned_from: None, split_from: None, on: Vec::new(), also: Vec::new(), would_close: None, taken_back: false, joins: None, passes: None, comments: Vec::new(), into_turn: inform.clone(), kind: None, answering: Vec::new() });
                let posted = x.posts.last().unwrap().posted(target);
                // a post by the ticket itself reads nothing; its author has read up to it
                if !by_ticket {
                    x.read_own(by.clone(), n, self.era, &mut self.passed_his);
                }
                let owner = by == "owner";
                // the owner's message of an intake ticket sent to wait: the post made of it waits
                if owner && [Some(t), self.tickets[&t].under].into_iter().flatten().any(|u| self.tickets.get(&u).is_some_and(|y| y.waits_message)) {
                    self.tickets.edit().get_mut(&target).unwrap().waits.insert(n);
                }
                cx.line(posted);
                cx.effect(Effect::Changed { t: target });
                self.halt_post(cx, target, n);
                if owner {
                    // his message's words: on a thread that gathered before intake (`words_gathered`), the 15 s hold is
                    // done and it only joins a burst already gathering on TARGET (`Place(target)`); an older thread's
                    // recorded program without `gather` still has `words_hold` here and holds its first post as it ran
                    let (hold, gathered) = (self.tickets[&t].words_hold, self.tickets[&t].words_gathered);
                    if (hold.is_some() || gathered) && self.gather_into(cx, target, n).is_some() {
                        work.push(Work::Resume(t, Type::unit_value()));
                        return;
                    }
                    if let Some(w) = self.owner_landed(cx, target, n, hold) {
                        work.push(Work::Resume(t, Type::unit_value()));
                        work.push(w);
                        return;
                    }
                }
                for a in names.iter().chain(&inform) {
                    cx.effect(Effect::Deliver { to: a.clone() });
                }
                work.push(Work::Resume(t, Type::unit_value()));
            }
            Prog::Give { w, e, .. } => {
                let v = eval(&e, &env, &ctx);
                let text = Text::of(&v).unwrap_or(Text::New(v.to_string()));
                let to = env.get(&w).and_then(Value::as_str).map_or(w, str::to_string);
                let x = self.tickets.edit().get_mut(&t).unwrap();
                x.gives.push(Gift { to: to.clone(), text, given: false });
                let n = x.gives.len() as u32;
                cx.line(Line::Gave { at: cx.now.clone(), t, to: to.clone(), n });
                cx.effect(Effect::Deliver { to });
                cx.effect(Effect::Changed { t });
                work.push(Work::Resume(t, Type::unit_value()));
            }
            Prog::Move { n, to, .. } => {
                let n = eval(&n, &env, &ctx).as_u64().unwrap_or(0) as u32;
                let to = eval(&to, &env, &ctx);
                let by = self.id(t);
                match to.as_str().and_then(parse_ticket).ok_or_else(|| format!("{to} is no ticket")).and_then(|u| self.move_post(cx, t, n, u, &by, work)) {
                    Ok(_) => work.push(Work::Resume(t, Type::unit_value())),
                    Err(why) => work.push(Work::Throw(t, Raised { exc: "Stuck".into(), value: json!({"move": n, "why": why}) })),
                }
            }
            Prog::Hole { scope, eager, .. } => {
                let closed = self.closed_below(t);
                let x = self.tickets.edit().get_mut(&t).unwrap();
                // a hole reached in a hole's handler (nobody fills the minder's role) rests at the hole it stood for
                let (env, scope) = match x.shaping.take() {
                    Some(s) => {
                        x.trace.back();
                        s
                    }
                    None => (env, scope),
                };
                x.status = Status::Rests { env, scope };
                // a kind's own hole is asked once something happens after it; a hole in the minder's program at once
                // (unless nothing has happened since the minder was last asked: never twice for the same state)
                if !eager {
                    x.asked_at = Some(x.mark(closed));
                }
                cx.line(Line::Rests { at: cx.now.clone(), t });
                cx.effect(Effect::Changed { t });
            }
            Prog::Reply { a, .. } => {
                let v = eval(&a, &env, &ctx);
                match self.reply_ask(cx, t, &v) {
                    Ok(()) => work.push(Work::Resume(t, Type::unit_value())),
                    Err(why) => work.push(Work::Throw(t, Raised { exc: "Stuck".into(), value: json!({"why": why}) })),
                }
            }
            Prog::Tell { agents, p, joins, .. } => {
                // `tell AGENTS NS`: some of his posts the handler holds, by number (a Split's part, E1b)
                let only = match eval(&p, &env, &ctx) {
                    Value::Array(xs) if xs.iter().all(Value::is_u64) => Some(xs.iter().filter_map(Value::as_u64).filter_map(|n| u32::try_from(n).ok()).collect()),
                    _ => None,
                };
                let v = eval(&agents, &env, &ctx);
                let agents = match &v {
                    Value::Array(xs) => xs.iter().filter_map(|a| a.as_str().map(str::to_string)).collect(),
                    Value::String(a) => vec![a.clone()],
                    _ => Vec::new(),
                };
                let joins = joins.as_ref().and_then(|j| eval(j, &env, &ctx).as_u64()).and_then(|n| u32::try_from(n).ok());
                match self.tell(cx, t, agents, joins, only) {
                    Ok(()) => work.push(Work::Resume(t, Type::unit_value())),
                    Err(why) => work.push(Work::Throw(t, Raised { exc: "Stuck".into(), value: json!({"why": why}) })),
                }
            }
            Prog::Mark { p, marks, .. } => {
                let (p, marks) = (eval(&p, &env, &ctx), eval(&marks, &env, &ctx));
                match self.mark_post(cx, &p, &marks) {
                    Ok(()) => work.push(Work::Resume(t, Type::unit_value())),
                    Err(why) => work.push(Work::Throw(t, Raised { exc: "Stuck".into(), value: json!({"why": why}) })),
                }
            }
            Prog::Deliver { a, items, rest, digest, from, .. } => {
                let (a, items, rest) = (eval(&a, &env, &ctx), eval(&items, &env, &ctx), rest.map(|r| eval(&r, &env, &ctx)));
                let (digest, from) = (digest.map(|r| eval(&r, &env, &ctx)), from.map(|f| eval(&f, &env, &ctx)));
                match self.act_deliver(cx, t, &a, &items, rest.as_ref(), digest.as_ref(), from.as_ref()) {
                    Ok(()) => work.push(Work::Resume(t, Type::unit_value())),
                    Err(why) => work.push(Work::Throw(t, Raised { exc: "Stuck".into(), value: json!({"why": why}) })),
                }
            }
            Prog::Fork { a, place, .. } => {
                let (a, place) = (eval(&a, &env, &ctx), eval(&place, &env, &ctx));
                match self.act_fork(cx, t, &a, &place) {
                    Ok(c) => work.push(Work::Resume(t, json!(c))),
                    Err(why) => work.push(Work::Throw(t, Raised { exc: "Stuck".into(), value: json!({"why": why}) })),
                }
            }
            Prog::Keep { items, why, .. } => {
                let (items, why) = (eval(&items, &env, &ctx), eval(&why, &env, &ctx));
                match self.act_hold(cx, t, true, &items, &why) {
                    Ok(()) => work.push(Work::Resume(t, Type::unit_value())),
                    Err(why) => work.push(Work::Throw(t, Raised { exc: "Stuck".into(), value: json!({"why": why}) })),
                }
            }
            Prog::Report { items, why, .. } => {
                let (items, why) = (eval(&items, &env, &ctx), eval(&why, &env, &ctx));
                match self.act_hold(cx, t, false, &items, &why) {
                    Ok(()) => work.push(Work::Resume(t, Type::unit_value())),
                    Err(why) => work.push(Work::Throw(t, Raised { exc: "Stuck".into(), value: json!({"why": why}) })),
                }
            }
            Prog::Drop { .. } => match self.drop_post(cx, t) {
                Ok(()) => work.push(Work::Resume(t, Type::unit_value())),
                Err(why) => work.push(Work::Throw(t, Raised { exc: "Stuck".into(), value: json!({"why": why}) })),
            },
            // his burst (E1b): held by the policy when posted (or at a thread's open before intake, `words_hold`), it
            // waits at the owner's step of `gather`, his next posts/messages joining it (`gather_into`,
            // `gather_into_thread`), and goes on as the burst (`Frame::Gather`); not held, it is the post at once
            Prog::Gather { e, at } => match self.tickets[&t].post_of.clone() {
                Some(p) if p.hold.is_some() && !p.done => {
                    self.tickets.edit().get_mut(&t).unwrap().k.push(Frame::Gather);
                    let step = Prog::Step { w: Worker::Owner, call: syntax::GATHER.into(), args: vec![syntax::Expr::Field(Box::new(e), "text".into())], at, hold: false, scope: BTreeMap::new(), inline: None, on: Vec::new() };
                    work.push(Work::Run(t, step, env));
                }
                None if self.tickets[&t].words_hold.is_some() => {
                    let x = self.tickets.edit().get_mut(&t).unwrap();
                    x.words_hold = None;
                    x.words_gathered = true;
                    x.k.push(Frame::Gather);
                    let step = Prog::Step { w: Worker::Owner, call: syntax::GATHER.into(), args: vec![e], at, hold: false, scope: BTreeMap::new(), inline: None, on: Vec::new() };
                    work.push(Work::Run(t, step, env));
                }
                _ => work.push(Work::Resume(t, eval(&e, &env, &ctx))),
            },
            Prog::Continue { e, .. } => {
                let v = eval(&e, &env, &ctx);
                if let Some(w) = self.carry_out(cx, t, v) {
                    work.push(w);
                }
            }
            Prog::Step { w, call, args, hold, scope, inline, on, .. } => {
                if self.tickets[&t].unchecked {
                    work.push(Work::Throw(t, Raised { exc: "NoCall".into(), value: json!({"call": call, "why": "the ticket's recorded program no longer checks against today's calls; repair it"}) }));
                    return;
                }
                let (template, refs) = match inline.as_ref().map(|(text, _)| render_inline(text, &env, &scope, |v| self.inline_words(v))) {
                    Some((t, r)) => (Some(t), r),
                    None => (None, Vec::new()),
                };
                let sig = match &inline {
                    Some((_, head)) => Some(CallSig { name: call.clone(), params: Vec::new(), head: head.clone(), blind: false, asks: None, cond: None, default: None }),
                    None => self.call_of(t, &call),
                };
                let Some(sig) = sig else {
                    work.push(Work::Throw(t, Raised { exc: "NoCall".into(), value: json!({"call": call}) }));
                    return;
                };
                let x = self.tickets.edit().get_mut(&t).unwrap();
                x.steps += 1;
                let n = x.steps;
                if let Some(v) = key.as_ref().and_then(|k| x.trace.visits.get_mut(k)) {
                    v.steps.push(n);
                    v.ty = Some(sig.head.clone());
                }
                let mut a = Map::new();
                for (i, (n, _)) in sig.params.iter().enumerate() {
                    a.insert(n.clone(), args.get(i).map_or(Value::Null, |e| answers::plain(eval(e, &env, &ctx))));
                }
                let owed = Owed { step: x.steps, call: call.clone(), args: Value::Object(a), blind: sig.blind, head: sig.head.clone(), hold, scope, env: env.clone(), template, refs, prompt: None, edit: None, on, unborne: None };
                match w {
                    Worker::Named(n) => {
                        let agent = env.get(&n).and_then(Value::as_str).map_or(n, str::to_string);
                        match self.unstarted_raise(&agent) {
                            Some(r) => {
                                self.tickets.edit().get_mut(&t).unwrap().steps -= 1;
                                work.push(Work::Throw(t, r));
                            }
                            None => self.await_or_restart(cx, t, agent, owed),
                        }
                    }
                    Worker::Owner => self.await_step(cx, t, "owner".into(), owed),
                    Worker::Expr(e) => match eval(&e, &env, &ctx) {
                        Value::String(agent) if !agent.is_empty() => match self.unstarted_raise(&agent) {
                            Some(r) => {
                                self.tickets.edit().get_mut(&t).unwrap().steps -= 1;
                                work.push(Work::Throw(t, r));
                            }
                            None => self.await_or_restart(cx, t, agent, owed),
                        },
                        v => {
                            x.steps -= 1;
                            work.push(Work::Throw(t, Raised { exc: "NoWorker".into(), value: json!({"head": "NoWorker", "why": format!("the worker {} is {v}, not an agent", crate::print::expr(&e))}) }));
                        }
                    },
                    Worker::Role(role) => {
                        cx.line(Line::Asked { at: cx.now.clone(), t, step: owed.step, question: "fills".into(), role: role.clone() });
                        cx.effect(Effect::Ask { t, step: owed.step, question: Question::Fills { role: role.clone(), call } });
                        cx.effect(Effect::Changed { t });
                        x.status = Status::Asking { owed, role };
                    }
                    Worker::Fork(a) => match fork::fork_from(&a, &env, &ctx) {
                        Ok(from) => self.fork_step(cx, t, from, owed, call.clone()),
                        Err(r) => {
                            x.steps -= 1;
                            work.push(Work::Throw(t, r));
                        }
                    },
                    Worker::Fresh => {
                        // a short-lived agent made for this step alone, ended when it is answered (owner m613)
                        let agent = self.next_scoped(t);
                        let x = self.tickets.edit().get_mut(&t).unwrap();
                        x.live.push(agent.clone());
                        cx.line(Line::Agent { at: cx.now.clone(), t, agent: agent.clone(), what: "made".into() });
                        self.await_step_of(cx, t, agent, owed, Some(call.clone()), None);
                    }
                }
            }
            Prog::Open { kind, args, .. } => {
                let c = self.next.max(self.tickets.range(..HIDDEN).next_back().map_or(0, |(m, _)| m + 1));
                // its program and the calls it names: as the log has them on replay, as they are now live
                let (program, calls) = match &cx.recorded {
                    Some(rec) => rec.iter().find_map(|l| match l {
                        Line::Opened { t: ct, program, calls, .. } if *ct == c => Some((program.clone(), calls.clone())),
                        _ => None,
                    }),
                    None => self.defs.kinds.get(&kind).map(|k| (k.text.clone(), self.calls_named(&k.text))),
                }
                .unzip();
                let calls = calls.unwrap_or_default();
                let (k, bad, loose) = match program.as_ref().ok_or_else(|| format!("no kind {kind}")).and_then(|p| self.recorded_program_as(&kind, p, &calls, cx)) {
                    Ok(k) => k,
                    Err(e) => {
                        work.push(Work::Throw(t, Raised { exc: "Stuck".into(), value: json!({"open": kind, "why": e}) }));
                        return;
                    }
                };
                let program = program.unwrap();
                let mut form = Map::new();
                let mut cenv = Env::new();
                for (i, (n, _)) in k.form.iter().enumerate() {
                    let v = args.get(i).map_or(Value::Null, |e| eval(e, &env, &ctx));
                    form.insert(n.clone(), v.clone());
                    cenv.insert(n.clone(), v);
                }
                // a pass of an agent's brain his switch of it is off for (BRAIN_SWITCH): stuck, as at a kind there is not
                if let Some(off) = self.brain_off(&kind, form.get("agent")) {
                    work.push(Work::Throw(t, Raised { exc: "Stuck".into(), value: json!({"open": kind, "why": off}) }));
                    return;
                }
                // who opened it, where its kind declares `opener`: the agent whose open is behind this program, else
                // this ticket; never the owner (proto-lead N-1446 p168; proto-reader p165)
                if declares_opener(&k) {
                    let by = Value::String(self.program_author(t));
                    form.insert(OPENER.into(), by.clone());
                    cenv.insert(OPENER.into(), by);
                }
                let mut child = self.new_ticket(c, &kind, Some(t), None, Value::Object(form), self.child_ctx(t), program.clone(), &k, &cx.now);
                child.calls = calls.clone();
                self.tickets.edit().insert(c, child);
                self.next = c + 1;
                if let Some(x) = self.tickets.edit().get_mut(&c) {
                    x.loose = loose;
                }
                if let Some(why) = &bad {
                    self.unchecked(c, why);
                }
                let x = self.tickets.edit().get_mut(&t).unwrap();
                x.children.push(c);
                x.status = Status::Waits { children: vec![c], par: false };
                if let Some(n) = key.and_then(|k| x.trace.visits.get_mut(&k)) {
                    n.tickets.push(c);
                }
                cx.line(Line::Opened { at: cx.now.clone(), t: c, kind, parent: t, program, calls });
                cx.effect(Effect::Changed { t });
                cx.effect(Effect::Changed { t: c });
                // a slow ticket's child is opened halted: its program runs once the owner approves it
                if self.slow(c) {
                    self.halt(cx, c, Halting::Open { pattern: k.pattern });
                } else {
                    work.push(Work::Run(c, k.pattern, cenv));
                }
            }
            Prog::Layer { on, body, .. } => {
                // a branch, a commit, or commits by repository (an object, passed on as its JSON text)
                let on = on.map(|e| eval(&e, &env, &ctx)).and_then(|v| match v {
                    Value::String(s) => Some(s),
                    Value::Object(_) => Some(v.to_string()),
                    _ => None,
                });
                let id = self.id(t);
                let x = self.tickets.edit().get_mut(&t).unwrap();
                x.layers += 1;
                let id = format!("{id}.l{}", x.layers);
                if let Some(n) = x.last_node.clone() {
                    x.layer_nodes.insert(id.clone(), n);
                }
                x.k.push(Frame::Layer { id, on });
                cx.effect(Effect::Changed { t });
                work.push(Work::Run(t, *body, env));
            }
            Prog::For { x, e, n, body, .. } => {
                let mut xs = match eval(&e, &env, &ctx) {
                    Value::Array(xs) => xs,
                    _ => Vec::new(),
                };
                if xs.len() > n as usize {
                    work.push(Work::Throw(t, Raised { exc: "Exhausted".into(), value: json!({"rounds": xs.len(), "most": n}) }));
                    return;
                }
                if xs.is_empty() {
                    work.push(Work::Resume(t, json!([])));
                    return;
                }
                let mut benv = env.clone();
                benv.insert(x.clone(), xs.remove(0));
                self.tickets.edit().get_mut(&t).unwrap().k.push(Frame::Each { x, rest: xs, done: Vec::new(), body: (*body).clone(), env });
                work.push(Work::Run(t, *body, benv));
            }
            Prog::Iterate { x, init, n, body, .. } => {
                let v = eval(&init, &env, &ctx);
                if n == 0 {
                    work.push(Work::Throw(t, Raised { exc: "Exhausted".into(), value: v }));
                    return;
                }
                let mut benv = env.clone();
                benv.insert(x.clone(), v);
                let left = if n == syntax::UNBOUNDED { n } else { n - 1 };
                self.tickets.edit().get_mut(&t).unwrap().k.push(Frame::Loop { x, left, measure: None, body: (*body).clone(), env });
                work.push(Work::Run(t, *body, benv));
            }
            Prog::Par { branches, tys, each: None, .. } => self.parts(cx, t, branches.into_iter().zip(tys.into_iter().map(Some).chain(std::iter::repeat(None))).map(|(b, ty)| (b, env.clone(), ty)).collect(), work, key),
            Prog::Any { n, branches, tys, each: None, .. } => {
                self.tickets.edit().get_mut(&t).unwrap().any = Some((n, Vec::new(), None));
                self.parts(cx, t, branches.into_iter().zip(tys.into_iter().map(Some).chain(std::iter::repeat(None))).map(|(b, ty)| (b, env.clone(), ty)).collect(), work, key);
            }
            Prog::Par { branches, tys, each: Some(each), .. } if each.order.is_some() => self.ordered_par(cx, t, branches, tys, each, &env, &ctx, work, key),
            Prog::Par { mut branches, tys, each: Some(each), .. } | Prog::Any { mut branches, tys, each: Some(each), .. } => {
                // one branch per element of the list (plan/layers.md I §3); more than the most opens nothing and raises,
                // as does an `any` of fewer than its N; none: the empty list at once
                let xs = match eval(&each.e, &env, &ctx) {
                    Value::Array(xs) => xs,
                    _ => Vec::new(),
                };
                if xs.len() > each.n as usize || any.is_some_and(|n| xs.len() < n as usize) {
                    work.push(Work::Throw(t, Raised { exc: "Exhausted".into(), value: json!({"branches": xs.len(), "most": each.n, "of": any}) }));
                    return;
                }
                if xs.is_empty() {
                    work.push(Work::Resume(t, json!([])));
                    return;
                }
                if let Some(n) = any {
                    self.tickets.edit().get_mut(&t).unwrap().any = Some((n, Vec::new(), None));
                }
                let b = branches.remove(0);
                let ty = tys.into_iter().next();
                let runs = xs.into_iter().map(|v| {
                    let mut benv = env.clone();
                    benv.insert(each.x.clone(), v);
                    (b.clone(), benv, ty.clone())
                });
                self.parts(cx, t, runs.collect(), work, key);
            }
        }
    }

    /// P reached on T: its visit in T's trace (none for a `<-`, which is its parts, or while a handler of T's own
    /// runs). Its key, for what the node records as it runs (a match's arm, the tickets it opened).
    fn enter(&mut self, t: u64, p: &Prog, env: &Env, ctx: &Value, now: &At) -> Option<TraceKey> {
        if matches!(p, Prog::Bind { .. }) {
            return None;
        }
        let src = self.tickets.get(&t)?.trace.src?;
        let (what, text) = crate::print::node(p);
        let key = (src, p.at(), text);
        let agents = |v: Value| match v {
            Value::Array(xs) => xs.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "),
            Value::String(a) => a,
            v => v.to_string(),
        };
        let who = match p {
            Prog::Step { w: Worker::Named(n), .. } => Some(env.get(n).and_then(Value::as_str).map_or(n.clone(), str::to_string)),
            Prog::Step { w: Worker::Expr(e), .. } => Some(agents(eval(e, env, ctx))),
            Prog::Step { w, .. } => Some(crate::print::worker(w)),
            Prog::Tell { agents: a, .. } => Some(agents(eval(a, env, ctx))),
            Prog::Post { naming, .. } if !naming.is_empty() => Some(naming.iter().map(|e| agents(eval(e, env, ctx))).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ")),
            _ => None,
        };
        let tr = &mut self.tickets.edit().get_mut(&t)?.trace;
        let steps = tr.visits.get(&key).map(|v| v.steps.clone()).unwrap_or_default();
        tr.visits.insert(key.clone(), Visit { who, at: Some(now.clone()), steps, ..Visit::default() });
        if !matches!(what, "match" | "try" | "iterate" | "for" | "layer" | "with" | "let") {
            tr.now = Some(key.clone());
            tr.pending = Some(key.clone());
        }
        Some(key)
    }

    /// The branches of a `par` or an `any`, each a child ticket of kind `part`, run in its own posts.
    /// Each branch with the environment it runs in and its type (an edit of it is checked against it).
    fn parts(&mut self, cx: &mut Cx, t: u64, branches: Vec<(Prog, Env, Option<Type>)>, work: &mut Vec<Work>, key: Option<TraceKey>) {
        self.parts_held(cx, t, branches, work, key, None);
    }

    /// As `parts`; with HOLDS (a `par` in order), branch I does not run until the branches HOLDS[I] (indices) have
    /// reached the state REACH (`parts::Held`).
    fn parts_held(&mut self, cx: &mut Cx, t: u64, branches: Vec<(Prog, Env, Option<Type>)>, work: &mut Vec<Work>, key: Option<TraceKey>, holds: Option<(Vec<Vec<usize>>, Option<String>)>) {
        let mut children = Vec::new();
        let mut runs = Vec::new();
        for (b, env, ty) in branches {
            let c = self.next.max(self.tickets.range(..HIDDEN).next_back().map_or(0, |(m, _)| m + 1));
            let k = Kind { name: "part".into(), file: String::new(), line: 0, letter: None, width: 0, form: Vec::new(), result: Type::Any, raises: Vec::new(), goals: Vec::new(), pattern: b.clone(), text: String::new(), base: None, handlers: Vec::new(), doc: Default::default(), drawn: None };
            let mut child = self.new_ticket(c, "part", Some(t), None, json!({}), self.child_ctx(t), String::new(), &k, &cx.now);
            child.branch = ty;
            // a branch is a piece of its parent's program: its nodes are keyed as they are there
            child.trace = Trace { visits: BTreeMap::new(), now: None, pending: None, ..self.tickets[&t].trace.clone() };
            self.tickets.edit().insert(c, child);
            self.next = c + 1;
            cx.line(Line::Opened { at: cx.now.clone(), t: c, kind: "part".into(), parent: t, program: String::new(), calls: BTreeMap::new() });
            cx.effect(Effect::Changed { t: c });
            children.push(c);
            runs.push(Work::Run(c, b, env));
        }
        let runs = match holds {
            Some((holds, reach)) => self.hold_parts(cx, &children, runs, holds, reach),
            None => runs,
        };
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.children.extend(children.iter().copied());
        if let Some(n) = key.and_then(|k| x.trace.visits.get_mut(&k)) {
            n.tickets.extend(children.iter().copied());
        }
        x.status = Status::Waits { children, par: true };
        cx.effect(Effect::Changed { t });
        // the first branch runs first
        work.extend(runs.into_iter().rev());
    }

    /// Branch T of P's `any n of` has closed: P resumes with the first N results once it has them, or raises the
    /// first failure once N can no longer be had; either way the branches still open are cancelled.
    fn any_closed(&mut self, cx: &mut Cx, p: u64, t: u64, children: &[u64], any: (u32, Vec<Value>, Option<Raised>), work: &mut Vec<Work>) {
        let (n, mut got, mut first) = any;
        match &self.tickets[&t].status {
            Status::Done(v) => got.push(v.clone()),
            Status::Failed(r) => {
                first.get_or_insert_with(|| r.clone());
            }
            _ => unreachable!(),
        }
        let open: Vec<u64> = children.iter().copied().filter(|c| self.tickets.get(c).is_some_and(Ticket::is_open)).collect();
        let end = if got.len() >= n as usize {
            Ok(Value::Array(got.clone()))
        } else if got.len() + open.len() < n as usize {
            Err(first.clone().unwrap())
        } else {
            self.tickets.edit().get_mut(&p).unwrap().any = Some((n, got, first));
            return;
        };
        let x = self.tickets.edit().get_mut(&p).unwrap();
        x.any = None;
        // not waiting while the rest close, so that their closes do not reach it
        x.status = Status::Waits { children: Vec::new(), par: false };
        let (by, why) = (self.id(p), format!("any {n} of: {}", if end.is_ok() { "enough results" } else { "too few can still come" }));
        let r = Raised { exc: "Cancelled".into(), value: json!({"by": by, "why": why}) };
        for c in open {
            let mut none = Vec::new();
            self.cancel(cx, c, &by, &why, &r, &mut none);
        }
        work.push(match end {
            Ok(v) => Work::Resume(p, v),
            Err(r) => {
                // the failures of its branches, which the `any` took: it unwinds
                self.tickets.edit().get_mut(&p).unwrap().passing = true;
                Work::Throw(p, r)
            }
        });
    }

    fn close(&mut self, cx: &mut Cx, t: u64, end: Status, work: &mut Vec<Work>) {
        // File: a filing's pieces are checked before it closes; one that cannot be placed fails it, and nothing is filed
        let mut end = end;
        let mut filed = Vec::new();
        let files = self.files(t);
        if files {
            if let Status::Done(v) = &end {
                match self.file_plan(t, v) {
                    Ok(plan) => filed = plan,
                    Err(why) => end = Status::Failed(Raised { exc: "Unplaced".into(), value: json!({"head": "Unplaced", "why": why}) }),
                }
            }
            if matches!(end, Status::Failed(_)) {
                // the owner typed it, so a filing that fails comes back to the owner
                self.tickets.edit().get_mut(&t).unwrap().told_closed.insert("owner".into());
            }
        }
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let (result, raised) = match &end {
            Status::Done(v) => (Some(v.clone()), None),
            Status::Failed(r) => (None, Some(r.clone())),
            _ => unreachable!(),
        };
        x.status = end;
        x.closed_at = Some(cx.now.clone());
        x.closes += 1;
        // where it would run again from, should it stop (`stop`): closed, it stops no more, and it runs again only from its
        // start (Repair, Rekind, Reopen: `restart`) or from a hole's own env (Undo), so it keeps none (p1936)
        (x.last_node, x.layer_nodes) = Default::default();
        // a run's form kept as text: what follows here reads none, and nothing reads it again but a run again or a view
        // (`form_text`; paper-research N-1446 p1960)
        x.pack_form();
        // the asker's own question (`coord ask` while it held a step): its closing notice is the answer; the step it
        // held is its again, handed once more (`again`), its clock afresh (paper-research N-101 p524, 2)
        if let Some(a) = x.ask.as_mut().filter(|a| a.own && !a.replied) {
            a.replied = true;
            let (to, h) = (a.by.clone(), a.held);
            cx.line(Line::AskAnswered { at: cx.now.clone(), t, to: to.clone(), later: true });
            let now = cx.now.clone();
            if let Some(Status::Awaits { agent, since, reminded, taken: true, .. }) = self.tickets.edit().get_mut(&h).map(|y| &mut y.status) {
                if *agent == to {
                    *since = now;
                    *reminded = Some(false);
                    cx.effect(Effect::Changed { t: h });
                }
            }
            if let Some(y) = self.tickets.edit().get_mut(&h) {
                y.remind_why = None;
            }
            cx.effect(Effect::Deliver { to });
        }
        let x = self.tickets.edit().get_mut(&t).unwrap();
        // an ask whose handler ended without a reply is answered with how it ended, so the asker does not wait on
        if x.ask.as_ref().is_some_and(|a| !a.replied) {
            let said = match (&result, &raised) {
                (_, Some(r)) => format!("Your question got no answer: {} ({}).", r.exc, crate::print::value(&r.value)),
                _ => String::from("Your question was closed without an answer."),
            };
            let _ = self.reply_ask(cx, t, &json!(said));
        }
        self.release(cx, t);
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let (parent, opener) = (x.parent, x.opener.clone());
        self.end_scoped(cx, t, None);
        for a in self.tickets[&t].kept.clone() {
            self.end_kept(cx, t, &a);
        }
        // a result of none is written as no result, as the log reads it back (`return none`, a handler's)
        cx.line(Line::Closed { at: cx.now.clone(), t, result: result.clone().filter(|v| !v.is_null()).map(answers::plain), raised: raised.clone() });
        cx.effect(Effect::Changed { t });
        if let Some(o) = &opener {
            cx.effect(Effect::Deliver { to: o.clone() });
        }
        if parent.is_none() && !self.turn_filing(t) && self.tickets[&t].handling.is_none() {
            // an escalation, or a ticket of an agent's: policy may tell more than the opener (Close)
            let actor = self.tickets[&t].posts.last().map(|p| p.by.clone()).or(opener).unwrap_or_default();
            cx.effect(Effect::Notify { t, event: "closed".into(), actor, post: None });
        }
        let _ = (result, raised);
        if files {
            self.file(cx, filed);
        }
        // a look of a `during` ended: the next runs on what came meanwhile
        if let (Some(_), Some(u)) = (&self.tickets[&t].handling, self.tickets[&t].under) {
            self.look_closed(cx, u, t);
            // a delivery run ended: the agent's next event waiting runs (delivery.rs)
            self.delivery_closed(cx, u, t, work);
        }
        // a stop's step answered: the ticket that stopped runs again or unwinds (stop.rs)
        if let Some(s) = self.tickets[&t].stop_of {
            return self.stop_answered(cx, s, t, work);
        }
        // a part in order: those that stand on what closed start, or never will (N-101)
        if self.part_closed(cx, t, work) {
            return;
        }
        // Child, then Join or Throw once no child it waits for is open
        let Some(p) = parent else { return };
        let Some(Status::Waits { children, par }) = self.tickets.get(&p).map(|px| px.status.clone()) else { return };
        if !children.contains(&t) {
            return;
        }
        if let Some(any) = self.tickets[&p].any.clone() {
            return self.any_closed(cx, p, t, &children, any, work);
        }
        if children.iter().any(|c| self.tickets.get(c).is_some_and(Ticket::is_open)) {
            return;
        }
        // the first failure in the order written; a part that never started because one it stood on failed is not one
        if let Some(r) = parts::first_failure(children.iter().map(|c| &self.tickets[c].status)) {
            // it did not stop below, so something above takes it, or it was ended there: it unwinds
            self.tickets.edit().get_mut(&p).unwrap().passing = true;
            work.push(Work::Throw(p, r));
            return;
        }
        let mut values = Vec::new();
        for c in &children {
            match &self.tickets[c].status {
                Status::Done(v) => values.push(v.clone()),
                _ => unreachable!(),
            }
        }
        work.push(Work::Resume(p, if par { Value::Array(values) } else { values.pop().unwrap_or(Value::Null) }));
    }

    /// Whether T is a filing of the owner's words, which the rule File files. A filing in an old log with `by` in its
    /// form was an agent's turn (S6, switched off by paper-research m952): it files nothing and tells nobody.
    fn files(&self, t: u64) -> bool {
        self.tickets[&t].kind == "filing" && !self.turn_filing(t)
    }

    fn turn_filing(&self, t: u64) -> bool {
        let x = &self.tickets[&t];
        x.kind == "filing" && x.form.get("by").is_some_and(|b| !b.is_null())
    }

    /// The rule File (semantics.tex, The owner's words), its check: filing F done with V = Filed(pieces) puts each piece
    /// on its ticket as BY's, a post of that span of F's text, or the answer to a step that awaits the owner when the
    /// piece names a result. Gives what to file, or why it cannot all be placed: a ticket that is not there, a post
    /// that would reach nobody in time, an answer that does not type, a span outside the text, words of the owner's
    /// that no piece covers. All or nothing: the owner's words are never filed in part.
    fn file_plan(&self, f: u64, v: &Value) -> Result<Vec<Filing>, String> {
        let base = self.tickets[&f].form.get("text").and_then(Text::of);
        let pieces = v.get("pieces").and_then(Value::as_array).cloned().unwrap_or_default();
        if pieces.is_empty() {
            return Err("nothing was placed".into());
        }
        let mut plan = Vec::new();
        let mut answered = BTreeSet::new();
        for (i, p) in pieces.iter().enumerate() {
            plan.push(self.piece(f, i + 1, p, &mut answered)?);
        }
        if let Some(Text::New(s)) = &base {
            let mut covered = vec![false; s.len()];
            for x in &plan {
                let p = x.part();
                covered[p.at as usize..(p.at + p.len) as usize].iter_mut().for_each(|c| *c = true);
            }
            let mut left = Vec::new();
            let mut cur = String::new();
            for (i, c) in s.char_indices() {
                if covered[i] {
                    left.push(std::mem::take(&mut cur));
                } else {
                    cur.push(c);
                }
            }
            left.push(cur);
            let left: Vec<String> = left.into_iter().filter(|w| w.chars().any(char::is_alphanumeric)).map(|w| format!("\"{}\"", w.trim())).collect();
            if !left.is_empty() {
                return Err(format!("no piece places {}", left.join(", ")));
            }
        }
        Ok(plan)
    }

    /// Piece N (P) of filing F, checked (`file_plan`).
    fn piece(&self, f: u64, n: usize, p: &Value, answered: &mut BTreeSet<u64>) -> Result<Filing, String> {
        let (at, len) = (p.get("at").and_then(Value::as_u64).unwrap_or(0), p.get("len").and_then(Value::as_u64).unwrap_or(0));
        let part = Part { t: f, at, len };
        let words = match self.text(&Text::Part { part: part.clone() }) {
            Some(w) if len > 0 => w,
            _ => return Err(format!("piece {n}: bytes {at} to {} are not a span of the text", at.saturating_add(len))),
        };
        let id = p.get("ticket").and_then(Value::as_str).unwrap_or("");
        let t = parse_ticket(id).filter(|t| self.tickets.contains_key(t)).ok_or_else(|| format!("piece {n}: there is no ticket {id}"))?;
        let x = &self.tickets[&t];
        match p.get("answer").and_then(Value::as_str) {
            None => {
                if !x.is_open() || matches!(x.status, Status::Waits { .. }) {
                    let why = if x.is_open() { "waits on the tickets below it" } else { "is closed" };
                    return Err(format!("piece {n}: {id} {why}, so a post there reaches nobody in time"));
                }
                Ok(Filing::Post(t, part))
            }
            Some(w) => {
                let Status::Awaits { agent, owed, .. } = &x.status else { return Err(format!("piece {n}: {id} awaits no answer")) };
                if agent != "owner" {
                    return Err(format!("piece {n}: {id} awaits {agent}, not the owner"));
                }
                if !answered.insert(t) {
                    return Err(format!("piece {n}: {id} is answered twice"));
                }
                let head = owed.head.coerce(&json!({"head": w})).map(|h| fill_text(&owed.head, h, &words)).and_then(|h| owed.head.conforms(&h).map(|()| h));
                Ok(Filing::Answer(t, part, owed.step, head.map_err(|e| format!("piece {n}: `{w}` does not answer {id}'s step: {e}"))?))
            }
        }
    }

    /// The rule File, applied (after `file_plan`): the posts, then the answers, all the owner's.
    fn file(&mut self, cx: &mut Cx, plan: Vec<Filing>) {
        let by = "owner";
        let mut answers = Vec::new();
        for x in plan {
            match x {
                Filing::Post(t, part) => {
                    let x = self.tickets.edit().get_mut(&t).unwrap();
                    let n = x.last() + 1;
                    x.posts.push(Post { n, at: cx.now.clone(), by: by.into(), text: Text::Part { part }, head: None, step: None, names: Vec::new(), told: Vec::new(), auto: false, moved: None, from: None, held: None, halted: false, dropped: false, routed: false, asks: Vec::new(), asks_open: false, asks_closed: None, answers: None, inform: Vec::new(), typed: None, cleaned_from: None, split_from: None, on: Vec::new(), also: Vec::new(), would_close: None, taken_back: false, joins: None, passes: None, comments: Vec::new(), into_turn: Vec::new(), kind: None, answering: Vec::new() });
                    x.read_own(by, n, self.era, &mut self.passed_his);
                    if let Status::Awaits { agent, .. } = &x.status {
                        cx.effect(Effect::Deliver { to: agent.clone() });
                    }
                    cx.line(x.posts.last().unwrap().posted(t));
                    cx.effect(Effect::Changed { t });
                }
                Filing::Answer(t, part, step, head) => answers.push((t, part, step, head)),
            }
        }
        for (t, part, step, head) in answers {
            // an answer before it may have closed this one (a branch of `any` cancelled)
            if !matches!(&self.tickets[&t].status, Status::Awaits { owed, .. } if owed.step == step) {
                continue;
            }
            self.accept(cx, t, by, head, Text::Part { part }, step);
        }
    }

    /// End ticket T's scoped agents: BY alone (its step was answered), or all (the ticket closed).
    /// The context a ticket opened under T starts with (semantics.tex, Context down the tree): T's as it is at the
    /// open (with its `with`s), and `ctx.K` = T for T's kind K (a `part` is no kind of its own and adds nothing).
    fn child_ctx(&self, t: u64) -> Value {
        let x = &self.tickets[&t];
        let mut c = match &x.ctx {
            Value::Object(o) => o.clone(),
            _ => Map::new(),
        };
        if x.kind != "part" {
            c.insert(x.kind.clone(), json!(self.id(t)));
        }
        Value::Object(c)
    }

    /// Cancel (semantics.tex): T fails as R; a step it awaited is withdrawn from its agent, who is told if it had
    /// been given it; its open children are cancelled first, their failures going nowhere. T's parent sees an
    /// ordinary failure (Throw).
    fn cancel(&mut self, cx: &mut Cx, t: u64, by: &str, why: &str, r: &Raised, work: &mut Vec<Work>) {
        let id = self.id(t);
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let mut asked = None;
        if let Status::Awaits { agent, owed, .. } = &x.status {
            let d = format!("{id}.s{}", owed.step);
            if x.given.iter().any(|g| g.to == *agent && g.delivery == d) {
                x.withdrawn.push((agent.clone(), owed.step, owed.call.clone(), false, Some((by.to_string(), why.to_string()))));
                cx.effect(Effect::Deliver { to: agent.clone() });
                asked = Some((agent.clone(), owed.step));
            }
        }
        if let Some((a, s)) = asked {
            self.ask_withdrawn(cx, t, &a, s, by, why);
        }
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.k.clear();
        x.holding = None;
        x.any = None;
        x.held = None;
        // closed before its children, so that their closes do not resume it
        let stopped = if let Status::Stopped { by: s, .. } = &x.status { Some(*s) } else { None };
        x.status = Status::Failed(r.clone());
        let children = x.children.clone();
        for c in children.into_iter().chain(stopped) {
            if self.tickets.get(&c).is_some_and(Ticket::is_open) {
                let mut none = Vec::new();
                self.cancel(cx, c, by, why, r, &mut none);
            }
        }
        self.close(cx, t, Status::Failed(r.clone()), work);
    }

    /// Layer LAYER on T's frames could not be made (WHY): what runs inside it is taken back (a step T awaits withdrawn,
    /// its open children, all opened inside, cancelled), its frames down to the layer's go as a raise unwinds them,
    /// and NoLayer(why) is raised at the layer's scope, where a `try` around it may catch it.
    fn layer_failed(&mut self, cx: &mut Cx, t: u64, layer: &str, why: &str) {
        let id = self.id(t);
        let by = "coord";
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let mut asked = None;
        if let Status::Awaits { agent, owed, .. } = &x.status {
            let d = format!("{id}.s{}", owed.step);
            if x.given.iter().any(|g| g.to == *agent && g.delivery == d) {
                x.withdrawn.push((agent.clone(), owed.step, owed.call.clone(), false, Some((by.to_string(), why.to_string()))));
                cx.effect(Effect::Deliver { to: agent.clone() });
                asked = Some((agent.clone(), owed.step));
            }
        }
        if let Some((a, s)) = asked {
            self.ask_withdrawn(cx, t, &a, s, by, why);
        }
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.any = None;
        // not waiting while its children close, so that their closes do not resume it
        x.status = Status::Done(Value::Null);
        let r = Raised { exc: "NoLayer".into(), value: json!({"head": "NoLayer", "why": why, "layer": layer}) };
        for c in x.children.clone() {
            if self.tickets.get(&c).is_some_and(|y| y.is_open() && y.parent == Some(t)) {
                let mut none = Vec::new();
                self.cancel(cx, c, by, why, &r, &mut none);
            }
        }
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let mut left = Vec::new();
        while let Some(f) = x.k.pop() {
            match f {
                Frame::Layer { id, .. } if id == layer => break,
                Frame::With { ctx } => x.ctx = ctx,
                Frame::Scope { agent } => left.push(agent),
                _ => {}
            }
        }
        x.status = Status::Waits { children: Vec::new(), par: false };
        // should it stop, it runs again from the layer, asking for it anew
        if let Some(n) = x.layer_nodes.get(layer).cloned() {
            x.last_node = Some(n);
        }
        for a in left {
            self.end_kept(cx, t, &a);
        }
        self.end_scoped(cx, t, None);
        cx.effect(Effect::Changed { t });
        self.drive(cx, vec![Work::Throw(t, r)]);
    }

    /// Take back what T waits for: a step it awaited is withdrawn from its agent (told if it had been given it),
    /// its open children are cancelled, their failures going nowhere, and its scoped agents end.
    fn take_back(&mut self, cx: &mut Cx, t: u64, by: &str, why: &str) {
        let id = self.id(t);
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let mut asked = None;
        if let Status::Awaits { agent, owed, .. } = &x.status {
            let d = format!("{id}.s{}", owed.step);
            if x.given.iter().any(|g| g.to == *agent && g.delivery == d) {
                x.withdrawn.push((agent.clone(), owed.step, owed.call.clone(), false, Some((by.to_string(), why.to_string()))));
                cx.effect(Effect::Deliver { to: agent.clone() });
                asked = Some((agent.clone(), owed.step));
            }
        }
        if let Some((a, s)) = asked {
            self.ask_withdrawn(cx, t, &a, s, by, why);
        }
        let x = self.tickets.edit().get_mut(&t).unwrap();
        // the frames go, and with them a `with`'s context
        if let Some(c) = x.k.iter().find_map(|f| if let Frame::With { ctx } = f { Some(ctx.clone()) } else { None }) {
            x.ctx = c;
        }
        x.k.clear();
        x.holding = None;
        x.any = None;
        // not waiting while its children close, so that their closes do not resume it
        x.status = Status::Done(Value::Null);
        let children = x.children.clone();
        let r = Raised { exc: "Cancelled".into(), value: json!({"by": by, "why": why}) };
        // what its pattern opened; not what was opened under it (an item's checks), which hangs on its id
        for c in children {
            if self.tickets.get(&c).is_some_and(|y| y.is_open() && y.parent == Some(t)) {
                let mut none = Vec::new();
                self.cancel(cx, c, by, why, &r, &mut none);
            }
        }
        self.end_scoped(cx, t, None);
        for a in self.tickets[&t].kept.clone() {
            self.end_kept(cx, t, &a);
        }
    }

    /// Step STEP of T, withdrawn from AGENT by BY for WHY (Cancel, Withdraw, Undo): an open ask AGENT raised while
    /// holding it is answered with the withdrawal (paper-research m1469, 2). One that waits in its turn gets the
    /// withdrawal as its command's result (Effect::AnsweredWith; the delivery is then given); one that went later
    /// needs nothing more, the withdrawal being no longer held behind it. The ask's ticket and the question it opened
    /// close as Dismissed, the question kind's own result, so it leaves the owner's list.
    fn ask_withdrawn(&mut self, cx: &mut Cx, t: u64, agent: &str, step: u32, by: &str, why: &str) {
        let Some(u) = self.tickets.values().find(|y| y.is_open() && y.ask.as_ref().is_some_and(|a| a.by == agent && a.held == t && !a.replied)).map(|y| y.t) else { return };
        let id = self.id(t);
        let a = self.tickets.edit().get_mut(&u).unwrap().ask.as_mut().unwrap();
        a.replied = true;
        // the asker's own question ticket stands on its own: it stays open, no longer holding the step back
        if a.own {
            cx.effect(Effect::Changed { t: u });
            return;
        }
        let later = !a.waiting;
        a.waiting = false;
        cx.line(Line::AskAnswered { at: cx.now.clone(), t: u, to: agent.to_string(), later });
        if !later {
            let x = self.tickets.edit().get_mut(&t).unwrap();
            let upto = x.mu.get(agent).copied().unwrap_or(0);
            let kind = x.kind.clone();
            if let Some(w) = x.withdrawn.iter_mut().rev().find(|w| w.0 == agent && w.1 == step) {
                w.3 = true;
                let (by, why) = w.4.clone().unzip();
                let what = What::Withdrawn { step, call: w.2.clone(), by, why };
                let d = Delivery { id: format!("{id}.s{step}.withdrawn"), to: agent.to_string(), t, ticket: id.clone(), kind, upto, what, inform: false };
                cx.effect(Effect::AnsweredWith { agent: agent.to_string(), d });
            }
        }
        let v = json!({"head": "Dismissed", "why": Text::New(format!("step {step} of {id}, which it was asked about, was withdrawn by {by}: {why}")).value()});
        // not waiting while the question closes, so that its close does not resume it
        self.tickets.edit().get_mut(&u).unwrap().status = Status::Done(Value::Null);
        let mut none = Vec::new();
        for c in self.tickets[&u].children.clone() {
            if self.tickets.get(&c).is_some_and(|y| y.is_open() && y.parent == Some(u)) {
                self.take_back(cx, c, by, why);
                self.close(cx, c, Status::Done(v.clone()), &mut none);
            }
        }
        self.take_back(cx, u, by, why);
        self.close(cx, u, Status::Done(v), &mut none);
    }

    /// T runs P from the start, open again (Repair, Rekind): with FORM as its form when given, else its own. Its
    /// opener is told when it closes, as for any ticket; a parent waiting for it takes its new result.
    fn restart(&mut self, cx: &mut Cx, t: u64, form: Option<Value>, p: Prog) {
        let x = self.tickets.edit().get_mut(&t).unwrap();
        // a closed run's form, kept as text, is what it runs on again (`form_text`); a new form replaces it
        match form {
            Some(f) => (x.form, x.form_text) = (f, None),
            None => x.unpack_form(),
        }
        x.status = Status::Waits { children: Vec::new(), par: false };
        x.k.clear();
        x.trace = Trace::new(p.clone());
        x.holding = None;
        x.any = None;
        x.unchecked = false;
        x.closed_at = None;
        x.closed_given.clear();
        x.told_closed.clear();
        x.informed.clear();
        let env: Env = x.form.as_object().map(|o| o.iter().map(|(a, b)| (a.clone(), b.clone())).collect()).unwrap_or_default();
        cx.effect(Effect::Changed { t });
        self.drive(cx, vec![Work::Run(t, p, env)]);
    }

    /// The agent of a `let agent` whose scope T has left (or T closed).
    /// `coord edit T` while T waits on the tickets below it (an `open`, a `par`, an `any`; paper-research N-101 p1631):
    /// PROGRAM, in the scope where it waits and with `answer` the value it waits for, is what T does with that value.
    /// The tickets below are not touched, and no line before this one changes.
    ///
    /// What the program starts with is entered at once, around the value: each `x <- …` whose first part it goes
    /// into, and each `try`, with its handlers. A `try { tops <- return answer; … } handle { E(w) => … }`, as `coord
    /// show --program` prints it, thus handles E raised by a part still running, as the frames it replaces did. The
    /// rest runs once the value comes (with `answer` bound to it, where the program goes on past it). Its frames go,
    /// as an edit's do; one the tickets below run inside (a `let agent`'s, a `layer`'s, a `during`'s) is not ended
    /// under them: such an edit is refused.
    fn edit_waiting(&mut self, cx: &mut Cx, t: u64, program: Option<&str>) -> (bool, Result<String, String>) {
        let no = |e: String| (false, Err(e));
        let x = &self.tickets[&t];
        let id = self.id(t);
        let below = match &x.status {
            Status::Waits { children, .. } => children.iter().map(|c| self.id(*c)).collect::<Vec<_>>().join(", "),
            _ => unreachable!(),
        };
        if let Some(f) = x.k.iter().find_map(|f| match f {
            Frame::Scope { agent } => Some(format!("the `let agent` of {agent}")),
            Frame::Layer { id, .. } => Some(format!("layer {id}")),
            Frame::During { whens, .. } => Some(format!("a `during … when {}`", whens.iter().map(|w| w.ev.as_str()).collect::<Vec<_>>().join(" … when "))),
            _ => None,
        }) {
            return no(format!("{id} waits on {below} inside {f}, which an edit would end under them: it is edited once they close, or the ticket is repaired."));
        }
        let Some((wait, env)) = x.waiting.clone() else { return no(format!("{id} waits on {below}, and where it waits is not known (its program was not checked).")) };
        let ty = match (&wait.ty, cx.recorded.is_some()) {
            (Some(ty), _) => ty.clone(),
            // on replay the log is what happened
            (None, true) => Type::Any,
            (None, false) => return no(format!("{id} waits on {below}, and the type of what it waits for is not known (its program was not checked).")),
        };
        let Some(text) = program.filter(|p| !p.trim().is_empty()) else { return no(String::from("An edit needs the program that takes the answer.")) };
        let (k, mut bad) = match self.kind_of(x) {
            Ok(k) => (k, None),
            Err(e) if cx.recorded.is_some() => (loose_kind(&x.kind), Some(e)),
            Err(e) => return no(e),
        };
        let p = match syntax::parse_prog(text, 1) {
            Ok(p) => p,
            Err(e) => return no(format!("The program does not parse: {e}")),
        };
        let vars = |scope: &BTreeMap<String, Type>| scope.iter().map(|(n, t)| format!("{n}: {t}")).collect::<Vec<_>>().join(", ");
        let mut scope = wait.scope.clone();
        scope.insert("answer".into(), ty.clone());
        let check = |p: &mut Prog, scope: &BTreeMap<String, Type>, m: &Self| check::check_in_with(p, &k.result, scope, &k.raises, &m.calls_at(t), &m.defs.kinds, Some(&m.defs.named), cx.recorded.is_none());
        let mut plain = p.clone();
        if let Err(e) = check(&mut plain, &scope, self).or_else(|e| if cx.recorded.is_some() { bad.get_or_insert(e); Ok(()) } else { Err(e) }) {
            return no(format!("The program cannot follow what {id} waits for ({below}): {e}. It must give {}; in scope: {}.", k.result, vars(&scope)));
        }
        // what is entered now does not have the value: `answer` is bound only where the program goes on past it
        let mut p = around(p);
        let mut now = wait.scope.clone();
        now.insert(IN_FLIGHT.into(), ty.clone());
        if let Err(e) = check(&mut p, &now, self).or_else(|e| if cx.recorded.is_some() { bad.get_or_insert(e); Ok(()) } else { Err(e) }) {
            return no(format!(
                "The program names `answer` where it is not there yet: what it starts with (its first `x <- …` parts and `try`s) is entered now, around the value {id} waits for, and the handlers of those `try`s and what follows those `x <- …` run without `answer` ({e}). Bind it there: `x <- return answer`."
            ));
        }
        let more = check::bound(&p, &self.defs.kinds);
        let frames = enter_around(p, &env);
        let x = self.tickets.edit().get_mut(&t).unwrap();
        // a `with`'s context: it is what it was outside them (the tickets below keep the one they were opened with)
        if let Some(c) = x.k.iter().find_map(|f| if let Frame::With { ctx } = f { Some(ctx.clone()) } else { None }) {
            x.ctx = c;
        }
        x.k = frames;
        x.bound = u64::from(x.steps) + more;
        cx.effect(Effect::Changed { t });
        if let Some(why) = &bad {
            self.unchecked(t, why);
        }
        (true, Ok(format!("{id} edited: it still waits on {below}, and runs the new program on what they give (at most {more} more step(s)).")))
    }

    fn end_kept(&mut self, cx: &mut Cx, t: u64, agent: &str) {
        let x = self.tickets.edit().get_mut(&t).unwrap();
        if let Some(i) = x.kept.iter().position(|a| a == agent) {
            x.kept.remove(i);
            cx.line(Line::Agent { at: cx.now.clone(), t, agent: agent.to_string(), what: "ended".into() });
            cx.effect(Effect::End { agent: agent.to_string() });
        }
    }

    fn end_scoped(&mut self, cx: &mut Cx, t: u64, by: Option<&str>) {
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let ending: Vec<String> = x.live.iter().filter(|a| by.is_none_or(|b| b == a.as_str())).cloned().collect();
        x.live.retain(|a| !ending.contains(a));
        // a withdrawal owed to an agent that ends is told nobody: it is stopped (live round: the reconcile loop kept
        // handing a cancelled branch's withdrawal to its stopped agent)
        for w in x.withdrawn.iter_mut().filter(|w| ending.contains(&w.0)) {
            w.3 = true;
        }
        for agent in ending {
            cx.line(Line::Agent { at: cx.now.clone(), t, agent: agent.clone(), what: "ended".into() });
            cx.effect(Effect::End { agent });
        }
    }
}

impl Type {
    pub fn unit_value() -> Value {
        Value::Array(Vec::new())
    }
}

/// The form field in which a kind is told who opened its ticket (thread.kind posts its words as it): an agent's open,
/// the agent; his message's binding, the owner; a program's open, `program_author`; another binding's, none (hs-ls
/// N-1446 p73: N-1445's #2 showed an agent's words as his). The machine gives it at the open, over whatever the form
/// held, and the open's line (`Line::Open`, `Fired`) records it: a coordd from before reads it there as any form
/// value, through its env, so a record replays with the same authors there (consult-3 p181).
pub const OPENER: &str = "opener";

fn declares_opener(k: &Kind) -> bool {
    k.form.iter().any(|(n, _)| n == OPENER)
}

/// FORM with `opener` given BY, where kind K declares it.
fn given_opener(k: &Kind, mut form: Value, by: &str) -> Value {
    if let (true, Some(o)) = (declares_opener(k), form.as_object_mut()) {
        o.insert(OPENER.into(), Value::String(by.into()));
    }
    form
}

/// Whether a ticket of kind K with FORM closes as it opens: a kind with no step and nothing it opens (a note), or an
/// agents' thread (Sent, design part 9b). It reaches only those its open names.
fn closes_at_once(k: &Kind, form: &Value) -> bool {
    let sent = form.get("addressed").and_then(|a| a.get("head")).and_then(Value::as_str) == Some("Sent");
    (!check::has_step(&k.pattern) && check::opens(&k.pattern).is_empty()) || sent
}

/// Whether an open's own TEXT is left to its program, which posts it: its kind declares `opener` (it posts its words as
/// whoever opened it), the open is `Named` (thread.kind posts at once, naming the form's `to` and `names`; an Unnamed
/// open waits on the watcher's intake, which may put the words on another ticket), the form's `text` is these very
/// words, the ticket does not close as it opens, the open tells nobody, and each agent it names is the form's `to` or
/// in its `names`, whom that post names (N-1445: `coord open thread … --text W W` gave W, then W again; consult-3
/// N-1446 p254). The open's line then records no text, so the words are posted once, by the program, here and in a
/// coordd from before that replays the line. Any other open keeps its text as its own #1, as before.
fn left_to_program(k: &Kind, form: &Value, text: &Text, names: &[String], told: &[String]) -> bool {
    let named = form.get("addressed").and_then(|a| a.get("head")).and_then(Value::as_str) == Some("Named");
    let posted_to = |a: &String| form.get("to").and_then(Value::as_str) == Some(a.as_str()) || form.get("names").and_then(Value::as_array).is_some_and(|l| l.iter().any(|x| x.as_str() == Some(a.as_str())));
    declares_opener(k) && named && !closes_at_once(k, form) && form.get("text").and_then(Text::of).as_ref() == Some(text) && told.is_empty() && names.iter().all(posted_to)
}

/// A head as given: a bare constructor name is the constructor with no fields.
fn normal_head(h: &Value) -> Value {
    match h {
        Value::String(s) => json!({"head": s}),
        h => h.clone(),
    }
}

/// An answer whose constructor has one text field, left out: that field is the answer's text.
/// Why HEAD, given by hand at OWED's step of a KIND (ID), is refused: an `on` of the step gives its constructor
/// ([`syntax::On`]'s rule; paper-research N-1446 p2255, 3), and how the lead ends it when that line never comes.
/// None when no `on` gives it.
fn on_only(owed: &Owed, head: &Value, kind: &str, id: &str) -> Option<String> {
    let c = head.get("head").and_then(Value::as_str)?;
    let o = owed.on.iter().find(|o| o.gives() == Some(c))?;
    let drop = if kind == "item" { ", or drops it (`coord item drop`)" } else { "" };
    Some(format!(
        "{c} at step {} of {id} comes only from its `{}` line, never by hand: when that line will not come, its lead cancels, withdraws or repairs {id}{drop}.",
        owed.step, o.ev
    ))
}

fn fill_text(ty: &Type, mut h: Value, text: &Text) -> Value {
    let Some(c) = h.get("head").and_then(Value::as_str) else { return h };
    let Some(fs) = ty.constructor(c) else { return h };
    let texts: Vec<&String> = fs.iter().filter(|(_, t)| matches!(t, Type::Text) || matches!(t, Type::Opt(i) if **i == Type::Text)).map(|(n, _)| n).collect();
    if let ([f], Some(o)) = (texts.as_slice(), h.as_object_mut()) {
        if o.get(*f).is_none_or(Value::is_null) {
            o.insert((*f).clone(), text.value());
        }
    }
    h
}

fn bind(b: &Bound, v: Value, env: &mut Env) {
    match b {
        Bound::Var(x) => {
            env.insert(x.clone(), v);
        }
        Bound::Tuple(xs) => {
            let vs = v.as_array().cloned().unwrap_or_default();
            for (i, x) in xs.iter().enumerate() {
                env.insert(x.clone(), vs.get(i).cloned().unwrap_or(Value::Null));
            }
        }
    }
}

/// Whether a handler of event EV raised on ticket T is run (paper-research m1432): live, always; on replay, only where
/// the input's own recorded lines open that run (`Opened` of kind EV under T). A log that has no such run (one made
/// before the handler existed) replays without it: the post goes to policy, the ask is refused, as they did then.
fn logged_run(cx: &Cx, ev: &str, t: u64) -> bool {
    cx.recorded.as_ref().is_none_or(|rec| rec.iter().any(|l| matches!(l, Line::Opened { kind, parent, .. } if kind == ev && *parent == t)))
}

/// Whether the hole's handler of T is run (m1435): live, always; on replay, where the input's recorded lines have its
/// `handling` line, or else, for logs from before b155419 only (which wrote no `handling` line), the run's own first
/// line: the minder's role asked (`asked fills`) or its `shape` step given. Do not widen the second reading: logs
/// written since b155419 always have the `handling` line where the handler ran. With neither, T rests, as it did.
fn hole_logged(cx: &Cx, t: u64) -> bool {
    cx.recorded.as_ref().is_none_or(|rec| {
        rec.iter().any(|l| match l {
            Line::Handling { t: u, event, .. } => *u == t && event == "hole",
            Line::Asked { t: u, question, .. } => *u == t && question == "fills",
            Line::Step { t: u, call, .. } => *u == t && call == SHAPE,
            _ => false,
        })
    })
}

pub fn eval(e: &Expr, env: &Env, ctx: &Value) -> Value {
    match e {
        Expr::Str(s) => Value::String(s.clone()),
        Expr::Int(n) => json!(n),
        Expr::Bool(b) => json!(b),
        Expr::None => Value::Null,
        Expr::Var(v) if v == crate::types::AGENTS && !env.contains_key(v) => crate::types::agents_marker(),
        Expr::Var(v) => env.get(v).cloned().unwrap_or(Value::Null),
        Expr::Ctx(f) => ctx.get(f).cloned().unwrap_or(Value::Null),
        Expr::Field(inner, f) => field(&eval(inner, env, ctx), f),
        Expr::Find { x, e, cond } => found(x, &eval(e, env, ctx), cond, env, ctx).unwrap_or(Value::Null),
        // `all`/`any`: whether COND holds for every element, or for one, X naming each as `find`'s does
        Expr::Quant { all, x, e, cond } => {
            let xs = eval(e, env, ctx);
            let mut env = env.clone();
            let mut holds = |v: &Value| {
                env.insert(x.to_string(), v.clone());
                eval(cond, &env, ctx) == json!(true)
            };
            let xs = xs.as_array().map(Vec::as_slice).unwrap_or_default();
            json!(if *all { xs.iter().all(&mut holds) } else { xs.iter().any(&mut holds) })
        }
        Expr::Con { name, args, fields } => {
            let mut o = Map::new();
            o.insert("head".into(), json!(name));
            for (i, f) in fields.iter().enumerate() {
                o.insert(f.clone(), args.get(i).map_or(Value::Null, |a| eval(a, env, ctx)));
            }
            Value::Object(o)
        }
        Expr::List(xs) | Expr::Tuple(xs) => Value::Array(xs.iter().map(|x| eval(x, env, ctx)).collect()),
        Expr::Record(fs) => Value::Object(fs.iter().map(|(n, x)| (n.clone(), eval(x, env, ctx))).collect()),
        // the ticket it runs on is in the context run() evaluates in (`#t`)
        Expr::Cmp { l, r, eq } => json!((answers::plain(eval(l, env, ctx)) == answers::plain(eval(r, env, ctx))) == *eq),
        // two numbers ordered; the checker lets only Ints here, so anything else (a missing value) holds no order
        Expr::Rel { l, r, rel } => json!(matches!((eval(l, env, ctx).as_i64(), eval(r, env, ctx).as_i64()), (Some(a), Some(b)) if rel.holds(a, b))),
        Expr::TextOf { e, ty } => answers::text_of(&eval(e, env, ctx), ty.as_ref()),
        Expr::ByOf(e) => answers::by_of(&eval(e, env, ctx)),
        Expr::Append(l, r) => {
            let item = |v: Value| match v {
                Value::Array(a) => a,
                Value::Null => Vec::new(),
                v => vec![v],
            };
            Value::Array(item(eval(l, env, ctx)).into_iter().chain(item(eval(r, env, ctx))).collect())
        }
        Expr::And(l, r) => json!(eval(l, env, ctx) == json!(true) && eval(r, env, ctx) == json!(true)),
        Expr::Or(l, r) => json!(eval(l, env, ctx) == json!(true) || eval(r, env, ctx) == json!(true)),
        Expr::Not(c) => json!(eval(c, env, ctx) != json!(true)),
        // read into its value before an `on` is evaluated (`Seen::read`), the one place the checker lets it stand
        Expr::Fact { .. } => Value::Null,
        // resolved before the node ran (`Machine::results`), into the context it reads
        Expr::ResultOf { e, .. } => eval(e, env, ctx).as_str().and_then(|id| ctx.get("#results")?.get(id)).cloned().unwrap_or(Value::Null),
        Expr::Post(n) => Text::Post { post: PostRef { t: ctx.get("#t").and_then(Value::as_u64).unwrap_or(0), n: *n } }.value(),
    }
}

/// The expressions node P evaluates itself (not those of the programs it holds).
fn node_exprs(p: &Prog) -> Vec<&Expr> {
    match p {
        Prog::Return { e, .. } | Prog::Give { e, .. } | Prog::Continue { e, .. } | Prog::Stop { e, .. } | Prog::For { e, .. } | Prog::With { e, .. } => vec![e],
        Prog::Match { e, guards, .. } => std::iter::once(e).chain(guards.iter().flatten()).collect(),
        Prog::Raise { e, .. } => e.iter().collect(),
        Prog::Step { w, args, .. } => args.iter().chain(if let Worker::Expr(e) | Worker::Fork(e) = w { Some(e) } else { None }).collect(),
        Prog::LetAgent { from, .. } => from.iter().collect(),
        Prog::Open { args, .. } | Prog::Decide { args, .. } => args.iter().collect(),
        Prog::Post { e, on, naming, informing, .. } => std::iter::once(e).chain(on.iter()).chain(naming.iter()).chain(informing.iter()).collect(),
        Prog::Move { n, to, .. } => vec![n, to],
        Prog::Reply { q, a, .. } => vec![q, a],
        Prog::Tell { agents, p, joins, .. } => std::iter::once(agents).chain(std::iter::once(p)).chain(joins.as_ref()).collect(),
        Prog::Drop { p, .. } | Prog::Gather { e: p, .. } => vec![p],
        Prog::Mark { p, marks, .. } => vec![p, marks],
        Prog::Deliver { a, items, rest, from, .. } => std::iter::once(a).chain(std::iter::once(items)).chain(rest.as_ref()).chain(from.as_ref()).collect(),
        Prog::Keep { items, why, .. } | Prog::Report { items, why, .. } => vec![items, why],
        Prog::Fork { a, place, .. } => vec![a, place],
        Prog::Iterate { init, .. } => vec![init],
        Prog::Layer { on, .. } => on.iter().collect(),
        Prog::Par { each, .. } | Prog::Any { each, .. } => each.iter().map(|x| &x.e).collect(),
        _ => Vec::new(),
    }
}

/// Field F of V; of a list, F of each element, in order (`e.f` on a list, N-101 p226, G9).
fn field(v: &Value, f: &str) -> Value {
    match v {
        Value::Array(xs) => Value::Array(xs.iter().map(|x| field(x, f)).collect()),
        v => v.get(f).cloned().unwrap_or(Value::Null),
    }
}

/// The first element of the list XS for which COND holds with X bound to it (`find`), if any.
fn found(x: &str, xs: &Value, cond: &Expr, env: &Env, ctx: &Value) -> Option<Value> {
    let mut env = env.clone();
    xs.as_array()?.iter().find(|v| {
        env.insert(x.to_string(), (*v).clone());
        eval(cond, &env, ctx) == json!(true)
    }).cloned()
}

/// Every `find` in E (outside its own condition) whose list has no element its condition holds for.
fn not_found<'a>(e: &'a Expr, env: &Env, ctx: &Value, out: &mut Vec<&'a Expr>) {
    match e {
        Expr::Find { x, e: xs, cond } => {
            not_found(xs, env, ctx, out);
            if found(x, &eval(xs, env, ctx), cond, env, ctx).is_none() {
                out.push(e);
            }
        }
        Expr::Field(y, _) | Expr::TextOf { e: y, .. } | Expr::ByOf(y) | Expr::ResultOf { e: y, .. } | Expr::Not(y) | Expr::Quant { e: y, .. } => not_found(y, env, ctx, out),
        Expr::Con { args: xs, .. } | Expr::List(xs) | Expr::Tuple(xs) | Expr::Fact { args: xs, .. } => xs.iter().for_each(|y| not_found(y, env, ctx, out)),
        Expr::Record(fs) => fs.iter().for_each(|(_, y)| not_found(y, env, ctx, out)),
        Expr::Cmp { l, r, .. } | Expr::Rel { l, r, .. } | Expr::And(l, r) | Expr::Or(l, r) | Expr::Append(l, r) => {
            not_found(l, env, ctx, out);
            not_found(r, env, ctx, out);
        }
        _ => {}
    }
}

/// Every `result of` in E, its ticket expression.
fn result_ofs<'a>(e: &'a Expr, out: &mut Vec<&'a Expr>) {
    match e {
        Expr::ResultOf { e, .. } => {
            result_ofs(e, out);
            out.push(e);
        }
        Expr::Field(x, _) | Expr::TextOf { e: x, .. } | Expr::ByOf(x) | Expr::Find { e: x, .. } | Expr::Quant { e: x, .. } | Expr::Not(x) => result_ofs(x, out),
        Expr::Con { args: xs, .. } | Expr::List(xs) | Expr::Tuple(xs) | Expr::Fact { args: xs, .. } => xs.iter().for_each(|x| result_ofs(x, out)),
        Expr::Record(fs) => fs.iter().for_each(|(_, x)| result_ofs(x, out)),
        Expr::Cmp { l, r, .. } | Expr::Rel { l, r, .. } | Expr::And(l, r) | Expr::Or(l, r) | Expr::Append(l, r) => {
            result_ofs(l, out);
            result_ofs(r, out);
        }
        _ => {}
    }
}

#[cfg(test)]
pub(crate) fn tests_matches(p: &Pat, v: &Value, env: &mut Env) -> bool {
    matches(p, v, env)
}

fn matches(p: &Pat, v: &Value, env: &mut Env) -> bool {
    match p {
        Pat::Wild => true,
        Pat::Var(x) => {
            env.insert(x.clone(), v.clone());
            true
        }
        Pat::Tuple(ps) => match v.as_array() {
            Some(vs) if vs.len() == ps.len() => ps.iter().zip(vs).all(|(p, v)| matches(p, v, env)),
            _ => false,
        },
        Pat::Nil => v.as_array().is_some_and(Vec::is_empty),
        Pat::None => v.is_null(),
        Pat::One(p) => v.as_array().is_some_and(|vs| vs.len() == 1 && matches(p, &vs[0], env)),
        Pat::Con { name, args, fields } => {
            // a declared record's value as an agent gives it carries no head (one the program built has its name)
            let record = v.is_object() && v.get("head").is_none();
            if !record && v.get("head").and_then(Value::as_str) != Some(name.as_str()) {
                return false;
            }
            args.iter().zip(fields).all(|(p, f)| matches(p, v.get(f).unwrap_or(&Value::Null), env))
        }
    }
}

/// Whether WORDS name agent A with an @ (`@fixer`, not `@fixers`).
pub fn at_name(words: &str, a: &str) -> bool {
    names_agent(words, a, false)
}

/// Whether WORDS name agent A as a word of its own: `@A` with no letter, digit or `_` right before the `@`; when BARE,
/// also `A` alone with none of those nor `-` right before it (`fixer,` but not `pre-fixer`); either way no letter,
/// digit, `_` or `-` right after it (`@fixers`, `fixer2` and `fixer-2` name no fixer). The one reading of a name in
/// words: [`at_name`] is it with the `@`; coordd's `next` line quotes the lines of a post that name its agent with or
/// without one (`answered_column`, paper-research N-1397.p169; proto-reader p171: one dash rule for both).
pub fn names_agent(words: &str, a: &str, bare: bool) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    if bare && a.is_empty() {
        return false;
    }
    words.match_indices(a).any(|(i, _)| {
        if words[i + a.len()..].chars().next().is_some_and(|c| word(c) || c == '-') {
            return false;
        }
        let mut before = words[..i].chars();
        match before.next_back() {
            Some('@') => !before.next_back().is_some_and(word),
            b => bare && !b.is_some_and(|c| word(c) || c == '-'),
        }
    })
}

/// The state of the owner's `@all` post N among POSTS (a ticket's, the machine's or a view's; WORDS gives a post's text
/// as new words, None when it has none): [`Machine::all_state`]. None when post N is not the owner's or has no `@all`.
/// In the posts' order, up to the claim: a post that passes it (`passes`) by the agent holding it hands it to the agent
/// that post names; an answer to it by an agent is the claim. A post taken back counts for neither.
pub fn all_of(posts: &[Post], n: u32, words: impl Fn(&Text) -> Option<String>) -> Option<AllState> {
    let p = posts.iter().find(|p| p.n == n && p.by == "owner")?;
    if !at_name(&words(&p.text)?, ALL) {
        return None;
    }
    let mut st = AllState { handed: p.told.clone(), claim: None, passed: Vec::new() };
    for q in posts.iter().filter(|q| q.by != "owner" && !q.taken_back) {
        if q.passes == Some(n) {
            if let Some(to) = q.names.first().filter(|_| st.handed.contains(&q.by)) {
                st.passed.push((q.by.clone(), q.n));
                st.handed = vec![to.clone()];
            }
        } else if q.answers == Some(n) {
            st.claim = Some((q.by.clone(), q.n));
            break;
        }
    }
    Some(st)
}

/// `C-12.s3.again` → (12, "s3.again"). Any letter, or none, before the number.
pub fn parse_delivery(d: &str) -> Option<(u64, &str)> {
    let (t, rest) = d.split_once('.')?;
    Some((parse_ticket(t)?, rest))
}

/// `C-12`, `c-12`, `12` → 12.
pub fn parse_ticket(s: &str) -> Option<u64> {
    let s = s.trim();
    let digits = s.trim_start_matches(|c: char| c.is_ascii_alphabetic()).trim_start_matches('-');
    digits.parse().ok()
}

/// CALLS as a recorded program from before a call changed is replayed against (`recorded_program`): each call's
/// parameters by today's names, of any type and all optional, padded to as many as a step may give; its answers as
/// today's. What a step's arguments were named is not compared on replay (step_args).
fn loose_calls(calls: &BTreeMap<String, check::CallSig>) -> BTreeMap<String, check::CallSig> {
    const MOST: usize = 16;
    calls
        .iter()
        .map(|(n, c)| {
            let mut params: Vec<(String, Type)> = c.params.iter().map(|(p, _)| (p.clone(), Type::Opt(Box::new(Type::Any)))).collect();
            for i in params.len()..MOST {
                params.push((format!("arg{}", i + 1), Type::Opt(Box::new(Type::Any))));
            }
            (n.clone(), check::CallSig { params, ..c.clone() })
        })
        .collect()
}

/// A kind that checks nothing: what an act on a ticket whose recorded program no longer checks is put against on
/// replay.
fn loose_kind(name: &str) -> Kind {
    Kind { name: name.to_string(), file: String::new(), line: 0, letter: None, width: 0, form: Vec::new(), result: Type::Any, raises: Vec::new(), goals: Vec::new(), pattern: Prog::Return { e: Expr::None, at: 0 }, text: String::new(), base: None, handlers: Vec::new(), doc: Default::default(), drawn: None }
}

/// Seconds since the epoch of a time as coordd writes it (`2026-11-11T09:30:00Z`): the window's graph page places the
/// record's times across by it (yaaihh map/lanes.rs).
pub fn secs(at: &str) -> Option<i64> {
    let n = |r: std::ops::Range<usize>| at.get(r).and_then(|s| s.parse::<i64>().ok());
    let (y, m, d, hh, mm, ss) = (n(0..4)?, n(5..7)?, n(8..10)?, n(11..13)?, n(14..16)?, n(17..19)?);
    // days from the civil date (H. Hinnant's algorithm)
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * 86_400 + hh * 3600 + mm * 60 + ss)
}

/// The time S seconds from the epoch, as `secs` reads one (`YYYY-MM-DDTHH:MM:SSZ`): its inverse (H. Hinnant's
/// civil-from-days). The one writer of a time from seconds: coordd's `core::at_secs` and `now` write through it.
pub fn at_secs(s: i64) -> At {
    let (days, rem) = (s.div_euclid(86_400), s.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

pub(crate) fn line_at(l: &Line) -> At {
    serde_json::to_value(l).ok().and_then(|v| v.get("at").and_then(Value::as_str).map(str::to_string)).unwrap_or_default()
}

/// The lines replay compares: all but `step_args`, which a log from before it lacks (N-101 post 55).
fn compared(lines: &[Line]) -> Vec<&Line> {
    lines.iter().filter(|l| !matches!(l, Line::StepArgs { .. })).collect()
}

/// Whether the machine's line MADE is RECORDED, as replay reads it: the same, but for whom a `posted` line's post
/// names, informs and asks into the turn of, where RECORDED has none of them, as a log from before they were written
/// has none (proto-mind N-1446 p802).
fn same(made: &Line, recorded: &Line) -> bool {
    match (made, recorded) {
        (Line::Posted { at, t, n, by, step, .. }, Line::Posted { names, inform, into_turn, .. }) if names.is_empty() && inform.is_empty() && into_turn.is_empty() => {
            *recorded == Line::Posted { at: at.clone(), t: *t, n: *n, by: by.clone(), step: *step, names: Vec::new(), inform: Vec::new(), into_turn: Vec::new() }
        }
        _ => made == recorded,
    }
}

/// Whether MINE makes the lines RECORDED has, in their order, as `compared` and `same` read them.
fn alike(mine: &[Line], recorded: &[Line]) -> bool {
    let (mine, recorded) = (compared(mine), compared(recorded));
    mine.len() == recorded.len() && mine.iter().zip(&recorded).all(|(m, r)| same(m, r))
}

/// The lines of RECORDED (as `compared` and `same` read them) that MINE does not make, in their order, when MINE makes
/// nothing RECORDED lacks; None when it does (the way back's handler not loaded, `replay_with`).
fn unmade(mine: &[Line], recorded: &[Line]) -> Option<Vec<Line>> {
    let mut rest = compared(recorded);
    for l in compared(mine) {
        let i = rest.iter().position(|r| same(l, r))?;
        rest.remove(i);
    }
    Some(rest.into_iter().cloned().collect())
}

pub(crate) fn line_tickets(l: &Line) -> Vec<u64> {
    if let Line::Event { fired, .. } = l {
        return fired.iter().map(|f| f.t).collect();
    }
    serde_json::to_value(l).ok().and_then(|v| v.get("t").and_then(Value::as_u64)).into_iter().collect()
}

fn show(ls: &[Line]) -> String {
    if ls.is_empty() {
        return String::from("nothing");
    }
    ls.iter().map(|l| serde_json::to_string(l).unwrap_or_default()).collect::<Vec<_>>().join(" ")
}

/// One piece of a filing, checked (`Machine::file_plan`): a post on a ticket, or the answer to a step that awaits
/// the owner.
enum Filing {
    Post(u64, Part),
    Answer(u64, Part, u32, Value),
}

impl Filing {
    fn part(&self) -> &Part {
        match self {
            Filing::Post(_, p) | Filing::Answer(_, p, ..) => p,
        }
    }
}

/// The worker coordd answers for the thread kind's `intake` (as it answers the clerk's step). Named so that `minder`
/// stays free for the role policy will fill later (paper-research m1116).
pub const WATCHER: &str = "watcher";

/// The call that asks what to do at a hole (agent-base shape.call): its answer is never posted.
pub const SHAPE: &str = "shape";

/// The worker coordd answers the minder's steps as where no agent fills the role (coordd's `shape::MINDER`).
pub const MINDER: &str = "minder";

/// The scopes slow can be set over (`Line::Slowed`; the policy's `scope`, agent-base/policy/slow.eflint).
pub const SCOPES: [&str; 1] = ["project"];

/// Whether slow set over SCOPE covers ticket T: "project" covers every ticket. A later scope (an agent's, a thread's)
/// is one more arm here and its `scope` in the policy; who may set it is the policy's alone.
fn covers(scope: &str, _t: u64) -> bool {
    scope == "project"
}

/// Whether W is a worker coordd answers for itself (the watcher, its own minder): on a slow thread its steps make no
/// card (the owner N-94, 3), but for the minder's Program or Close.
pub fn coordds_own(w: &str) -> bool {
    w == WATCHER || w == MINDER
}

/// The suffix of the delivery that tells an agent the owner archived a ticket (`Ticket::told_archived`).
pub const ARCHIVED_NOTICE: &str = "archived";

/// F as it is: an archive is taken live from this release (the release before took one only on replay, and its
/// tests wrote this one's record inside this).
#[cfg(test)]
pub(crate) fn archiving_live<R>(f: impl FnOnce() -> R) -> R {
    f()
}

/// The gift a delivery suffix `wN` names: its index in the ticket's gives.
fn gift_index(suffix: &str) -> Option<usize> {
    suffix.strip_prefix('w')?.parse::<usize>().ok()?.checked_sub(1)
}

/// A `Ref`, or a list of them, in a text's `{{ }}` of type T (SCOPE's, by `check::place_type`): as an answer writes
/// it (`types::ref_words`), a list joined by "; " when one of them names several repositories, else by ", ".
fn ref_place(v: &Value, t: Option<&Type>) -> Option<String> {
    let t = match t? {
        Type::Opt(x) => (**x).clone(),
        t => t.clone(),
    };
    match (t, v) {
        (Type::Ref, v) => crate::types::ref_words(v),
        (Type::List(x), Value::Array(xs)) if *x == Type::Ref => {
            let ws = xs.iter().map(crate::types::ref_words).collect::<Option<Vec<_>>>()?;
            Some(ws.join(if xs.iter().all(Value::is_string) { ", " } else { "; " }))
        }
        _ => None,
    }
}

/// A post's TEXT with each `{{ name }}` / `{{ name.field }}` filled from ENV (N-101.p42): a string as it is, a `Ref`
/// as an answer writes it (by its type in SCOPE), a list of them joined by ", ", none as "none", anything else as
/// JSON; a place the scope lacks stays as written.
fn fill_post(text: &str, env: &Env, scope: &BTreeMap<String, Type>) -> String {
    let mut out = String::new();
    let mut last = 0;
    for (name, path, span) in syntax::placeholders(text) {
        out.push_str(&text[last..span.start]);
        let v = path.iter().try_fold(env.get(&name), |v, f| Some(v.and_then(|v| v.get(f))));
        let word = |v: &Value| match v {
            Value::String(s) => s.clone(),
            Value::Null => "none".to_string(),
            v => v.to_string(),
        };
        let t = crate::check::place_type(scope, &name, &path);
        match v.flatten() {
            Some(v) if ref_place(v, t.as_ref()).is_some() => out.push_str(&ref_place(v, t.as_ref()).unwrap_or_default()),
            Some(Value::Array(xs)) => out.push_str(&xs.iter().map(word).collect::<Vec<_>>().join(", ")),
            Some(v) => out.push_str(&word(v)),
            None => out.push_str(&text[span.clone()]),
        }
        last = span.end;
    }
    out.push_str(&text[last..]);
    out
}

/// An inline call's TEXT with each `{{ name }}` / `{{ name.field }}` filled from ENV: a string as it is, a `Ref` as
/// an answer writes it (by its type in SCOPE), anything else as JSON (a place the checker let through and the scope
/// lacks stays as written).
fn render_inline(text: &str, env: &Env, scope: &BTreeMap<String, Type>, words: impl Fn(&Value) -> Option<Option<String>>) -> (String, Vec<Value>) {
    let mut refs: Vec<Value> = Vec::new();
    let mut out = String::new();
    let mut last = 0;
    for (name, path, span) in syntax::placeholders(text) {
        out.push_str(&text[last..span.start]);
        let v = path.iter().try_fold(env.get(&name), |v, f| Some(v.and_then(|v| v.get(f))));
        let t = crate::check::place_type(scope, &name, &path);
        match v.flatten() {
            Some(Value::String(s)) => out.push_str(s),
            Some(v) if ref_place(v, t.as_ref()).is_some() => out.push_str(&ref_place(v, t.as_ref()).unwrap_or_default()),
            Some(v) if words(v).is_some() => match words(v).flatten() {
                Some(w) => out.push_str(&w),
                None => {
                    out.push_str(&format!("{{{{@{}}}}}", refs.len()));
                    refs.push(v.clone());
                }
            },
            Some(v) => out.push_str(&v.to_string()),
            None => out.push_str(&text[span.clone()]),
        }
        last = span.end;
    }
    out.push_str(&text[last..]);
    (out, refs)
}

/// Every string in V, at any depth (a ticket named in a value is its id, a string).
fn strings_in(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::String(s) => {
            out.insert(s.clone());
        }
        Value::Array(xs) => xs.iter().for_each(|x| strings_in(x, out)),
        Value::Object(o) => o.values().for_each(|x| strings_in(x, out)),
        _ => {}
    }
}

/// The name the value a ticket waits for has in an edit's program while it is not there yet (no program can write it).
const IN_FLIGHT: &str = "(in flight)";

/// An edit's program P for a ticket waiting on tickets below it, as it is entered (`Machine::edit_waiting`): down
/// through its first `x <- …` parts and its `try` bodies to what needs the value: `return answer` becomes the value
/// itself; anything else S becomes `answer <- (the value); S`.
fn around(p: Prog) -> Prog {
    match p {
        Prog::Bind { x, first, rest } => Prog::Bind { x, first: Box::new(around(*first)), rest },
        Prog::Try { body, arms, at } => Prog::Try { body: Box::new(around(*body)), arms, at },
        Prog::Return { e: Expr::Var(a), at } if a == "answer" => Prog::Return { e: Expr::Var(IN_FLIGHT.into()), at },
        s => {
            let at = s.at();
            Prog::Bind { x: Bound::Var("answer".into()), first: Box::new(Prog::Return { e: Expr::Var(IN_FLIGHT.into()), at }), rest: Box::new(s) }
        }
    }
}

/// The frames P (made by `around`) puts on a ticket as it is entered in ENV: those of its `x <- …` and `try`s, down to
/// the value it waits for.
fn enter_around(p: Prog, env: &Env) -> Vec<Frame> {
    let mut k = Vec::new();
    let mut p = p;
    loop {
        p = match p {
            Prog::Bind { x, first, rest } => {
                k.push(Frame::Bind { x, rest: *rest, env: env.clone() });
                *first
            }
            Prog::Try { body, arms, .. } => {
                k.push(Frame::Try { arms, env: env.clone() });
                *body
            }
            _ => return k,
        };
    }
}
