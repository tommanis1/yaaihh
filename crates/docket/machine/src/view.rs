//! A ticket as the surfaces show it (plan/interface.md §5).

use crate::line::{At, Post, Raised};
use crate::machine::{Machine, Status, Ticket};
use crate::types::{Loc, Text, Type};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A reload whose new text a ticket took (`Machine::retext`): AT, BY, WHY as the reload's; SAID, its line of the
/// reload's answer; WITHDRAWN, the steps it withdrew, its own and its branches', whose statements the new text no
/// longer has.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reloaded {
    pub at: At,
    pub by: String,
    pub why: String,
    pub said: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub withdrawn: Vec<Withdrew>,
}

/// A step a reload withdrew: STEP of ticket ON (`C-12`, or a branch's id), of CALL, from WORKER (none when no one
/// had been found for it yet).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Withdrew {
    pub on: String,
    pub step: u32,
    pub call: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
}

/// What the definitions in place would do to open ticket T if a reload put them in (`Machine::reach`, the owner's Reach
/// of a draft, nothing changed): its CASE, and SAID, the engine's words for it. TOOK, for a ticket of a kind whose text
/// they change: whether it takes the new text (`retext`), and WHY not. AGENT: whose delivery ticket it is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reach {
    pub t: u64,
    pub case: ReachCase,
    pub said: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub took: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

/// How an edit reaches a ticket ([`Reach`]), in the order one case of a ticket's pieces outweighs another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReachCase {
    /// It takes the new text or words, but nothing they change is ahead of it.
    Past,
    /// What it has not run holds a node the edit changes or adds, or a step of an edited call still to be sent.
    Ahead,
    /// A step of the edited call ahead of it, or the one it stands at, goes with the owner's own prompt: edited at its
    /// halt, or his once-edit of the call's template (`Line::Template`), on it or on a ticket above it.
    Own,
    /// The step it stands at keeps the words it has: given, or being handed and so going with whichever text its
    /// rendering read (a call edit); or made when it became owed (a kind edit).
    Given,
    /// The node it stands at is gone from the new text: what is in flight is withdrawn (`retext`).
    Withdrawn,
    /// It keeps its old text (`retext`'s refusal).
    Kept,
}

/// The last reload that named a kind or a call (`Line::Reload`): when, by whom, why, after which commit. The mark a live
/// edit leaves, read from the record, so a restart and a replay show the same.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveEdit {
    pub at: At,
    pub by: String,
    pub why: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TicketView {
    /// `C-12`.
    pub id: String,
    pub n: u64,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub under: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opener: Option<String>,
    /// The binding that opened it, as written (`Ticket::binding`): the clock or event of the project's it ran for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<String>,
    pub ctx: Value,
    pub form: Value,
    /// The form's first text field when it is new text, else the first post's (empty when that is a place).
    pub title: String,
    pub status: ViewStatus,
    pub posts: Vec<Post>,
    /// Its posts taken back after an agent had them (0.2 B8, N-1396), drawn struck through: kept out of POSTS, so a
    /// window from before draws them gone, as it draws a post taken back before anyone had it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub struck: Vec<ViewStruck>,
    pub children: Vec<String>,
    pub program: String,
    /// The most steps it can take; the steps taken.
    pub bound: u64,
    pub spent: u32,
    pub given: Vec<ViewGiven>,
    pub opened_at: At,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<At>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broken: Option<String>,
    /// The last reload whose new text it took (live edits): what the reload's answer said of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reloaded: Option<Reloaded>,
    /// Whom the open ticket waits on, down its tree: the steps owed at its leaves (none when it is closed). A step of a
    /// handler running under it (`handlers`: the owner's held post on clarify, or the watcher's route) is its own, named
    /// with its id, so a thread resting at its hole still reads as waiting on the owner (paper-research m1313, 2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waits_on: Vec<WaitsOn>,
    /// It owes the owner something (`Machine::owes_owner`): what the page, the sidebar's badge and the outline read,
    /// one function for all (paper-research N-447 p7). An optional field: an older window ignores it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub owed: bool,
    /// The layer its steps run in (the innermost on its frames or its parent's, `Machine::layer_of`), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    /// What it will do with what it waits for, as program text in which `answer` is that (`print::continuation`):
    /// what an edit starts from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<String>,
    /// Who said they work on it (an item's `coord item start`), in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub started: Vec<String>,
    /// Read, no reply (paper-research N-19): who ended a turn given posts here with Nothing, and up to which post.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quiet: Vec<ViewQuiet>,
    /// Where each post of the owner's is, for each agent it went to (`whereabouts`; part 4 of the message runtime,
    /// N-818 post 32): what the line under his post is drawn from. A list, not a map by post number: a map's number
    /// keys go out as strings, which the window once could not read (N-541). Older windows ignore it; coordd sends it
    /// from `WHEREABOUTS_PROTOCOL` on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub whereabouts: Vec<PostWhere>,
    /// Its line in the outline, with the threads under it (`ViewOutline`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<ViewOutline>,
    /// How it stands in the thread it stands at, as its kind says (`drawn`): a line, a mark, or neither. A view from
    /// a coordd before the clause has none, and the window keeps its rule from before (thread and item are lines).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawn: Option<crate::syntax::Drawn>,
    /// How it ended, when it was ended (`coord end`, paper-research N-447): done, dropped or kept, why, by whom, when.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<ViewEnded>,
    /// The posts elsewhere whose words name it or one of its posts (paper-research N-412 p166/p169, threads piece 3),
    /// oldest first: filled by coordd, which reads every post's words and keeps the ids they hold (`ViewMention`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mentioned: Vec<ViewMention>,
    /// By post number, the ids its posts' words hold (`N-412`, `N-412.p30`, an agent's NAME for `@NAME`), as written,
    /// each once, in the order written; posts that hold none are left out (docket-core4 N-412 p168). Filled by coordd.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty", deserialize_with = "by_post")]
    pub mentions: std::collections::BTreeMap<u32, Vec<String>>,
    /// The owner's once-edits of call templates that reach this ticket's steps and are not used up (N-208; `coord
    /// template`): the edit the step owed now carries (`step`), its own `step` edits not used yet, and the `tree` edits of
    /// it and the tickets above it, nearest first, the latest per ticket and call. The app shows, for a step of CALL, the
    /// first of CALL here; none: the call's own template.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub templates: Vec<ViewTemplate>,
    /// The minder's answers at its holes, in order (docket-edge's chip and its Undo, the last not undone).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shaped: Vec<ViewShaped>,
    /// The owner's steps that went on by themselves (`Line::Lapsed`, N-487): which post holds the answer, after how
    /// many seconds, and why (`time`, `closed`). Wherever that post is shown it says so ("went on by itself after
    /// 30 s"), so it is never read as his word.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lapsed: Vec<ViewLapsed>,
    /// When the owner's step owed now goes on by itself (its call's `default`, the policy's `lapse-after`): coordd
    /// writes it into the view it sends, from its clock; the machine, which reads no clock, leaves it none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lapses_at: Option<At>,
    /// While it rests at a hole or its hole is being shaped: the variables in scope there and their types (what a
    /// program at the hole is checked in, `check::check_shape`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole: Option<std::collections::BTreeMap<String, Type>>,
    /// With HOLE: where the owner stands among the posts, facts the minder is given rather than left to work out
    /// (paper-research m1226).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<ViewOwner>,
    /// The owner's posts that an agent they ask has not answered, oldest first (paper-research N-101.p1285, 3: what the
    /// reply tool did not answer stays shown as unanswered, to him and to the agent): those of `asked` with an agent
    /// whose answer is None, by the ledger's one rule (`Machine::ledger`, I-1455 (B)), but not one an agent it asks or
    /// was handed to answered, which is answered for all (`Machine::his_answered_in`, the rule the foot of each delivery
    /// reads; paper-research N-1446 p2355). The Threads page's "Asked, no answer yet" lists these.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unanswered_owner: Vec<u32>,
    /// His posts that ask agents to act, each agent asked with the post that answered it, if any (`Machine::asked`,
    /// I-1455 (B)): the ring of his post on the Map, open while one has none. By post, then in the order it asks
    /// them. A coordd before `ASKED_PROTOCOL` sends none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asked: Vec<ViewAsked>,
    /// A goal's latest duties, its last `duties` line's value with that line's time: `{at, goal, owed, holders, by,
    /// met, broken}`, kept once it closes (`Machine::duties_seen`; paper-research N-1446 p2328, 2; the shape
    /// docket-edge6 p2421, proto-window p2434): what its card and row draw. None before its first answer, and from a
    /// coordd before `GOAL_DUTIES_PROTOCOL`. The first use of "a view reads a recorded decision", here in code for one
    /// question: it leaves the engine when N-1546's notion lands (paper-research N-1446 p2616, p2635).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duties: Option<Value>,
    /// The open tickets under it that run a handler (`Ticket.handling`: the owner's post held while `when owner_post`
    /// decides whom it is for). Listed nowhere else; here for the clarify chip, reached through `Post.held`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handlers: Vec<TicketView>,
    /// Every run of a handler under it, open and closed, oldest first (`ran`): what its picture's handler rows count
    /// and open, and the run a post's mark opens (docket-edge8 N-1446 p21; consult-3 p34; proto-lead p37). One list:
    /// `handlers` keeps the open ones whole, as before.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ran: Vec<ViewRan>,
    /// The owner's star (design-two-systems §16, 3).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub starred: bool,
    /// Every star on it and its posts, oldest first: the owner's and the agents' (the owner N-234.p3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stars: Vec<ViewStar>,
    /// Its program as a tree, its top statements in order, each with where it stands (§16, 9): what a picture of
    /// it is drawn from. The programs the minder wrote stand under the hole each filled.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub run: Vec<RunNode>,
    /// The owner archived it (`Line::Archive`): no longer of importance to him, out of his lists, the row that takes
    /// `Unarchive`. Not an end: its status is as it was. Nothing at or under it is handed while it is archived.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub archived: bool,
    /// The ticket above it that the owner archived (its parent, or the ticket it runs under, and so on up), as
    /// SLOW_BY names a slow one: it leaves his lists with that one. Worked out at each snapshot, never stored, so a
    /// ticket no longer under it comes back with no line of its own (`Machine::archived_by`; docket-core3 N-867.p27).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_under: Option<String>,
    /// It halts (slow mode): it or a ticket above it is switched to slow; SLOW_BY, the id of the one that is.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub slow: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slow_by: Option<String>,
    /// The scope slow is set over that makes it halt ("project"; `Machine::slow_scope`), beside SLOW_BY: an app that
    /// knows only SLOW_BY sees SLOW and no ticket whose switch it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slow_scope: Option<String>,
    /// Its moments halted for the owner, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub halts: Vec<ViewHalt>,
    /// The tickets below it (children, branches, handler tickets) with halts waiting.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub halted: Vec<String>,
    /// Its runner steps coordd holds back unsent (the runner's command could not start): coordd fills it as the view
    /// leaves it, the machine never does (docket-edge N-236, for docket-core2's held step).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unsent: Vec<ViewUnsent>,
}

/// A post taken back after an agent had it (0.2 B8, N-1396): the post as it was, AT when it was taken back, TO the
/// agents that had it on its ticket, TOLD those given the notice of it (`What::TakenBack`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewStruck {
    pub post: Post,
    pub at: At,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub to: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub told: Vec<String>,
}

/// A star: on post POST (none: the ticket itself), by BY, AT when.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewStar {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<u32>,
    pub by: String,
    pub at: At,
}

/// A step of the ticket held back unsent: STEP of CALL to WORKER; WHY it was not sent and SINCE when, as coordd keeps
/// them (none until it does).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewUnsent {
    pub step: u32,
    pub call: String,
    pub worker: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// How many times it was tried and could not start (coordd tries it again each minute).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tries: Option<u32>,
}

/// A step not given to AGENT because its model does not meet the call's `where` (`Line::ModelRefused`): WHY, as the
/// line says it, AT when.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewRefused {
    pub agent: String,
    pub why: String,
    pub at: String,
}

/// A moment halted for the owner (slow mode): what is about to happen, and what an edit changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "what", rename_all = "snake_case")]
pub enum ViewHalt {
    /// Step STEP about to be given to WORKER. PROMPT: the exact text that would be sent (coordd renders it; the
    /// owner's own once EDITED).
    Give { n: u32, at: At, step: u32, call: String, worker: String, prompt: String, edited: bool },
    /// The answer to step STEP about to be taken: WORKER's WORDS, the HEAD formed from them, of type TYPE.
    Take { n: u32, at: At, step: u32, call: String, worker: String, words: Text, head: Value, #[serde(rename = "type")] ty: Type },
    /// Post POST (by BY) about to be delivered: whom it WAKES, whom it INFORMS. WITH: the posts of his told with it, one
    /// tell, one card (E1b), going when it goes; none for a post told alone (as before).
    /// CHOSE: who chose whom it wakes when an answer did (the watcher's `route`); none when a rule did.
    Deliver { n: u32, at: At, post: u32, #[serde(default, skip_serializing_if = "Vec::is_empty")] with: Vec<u32>, by: String, wakes: Vec<String>, inform: Vec<String>, edited: bool, #[serde(default, skip_serializing_if = "Option::is_none")] chose: Option<String> },
    /// This ticket, opened, about to run: its KIND and FORM, the form's FIELDS.
    Open { n: u32, at: At, ticket: String, kind: String, form: Value, fields: Vec<(String, Type)> },
    /// A policy answer about to be used: QUESTION for STEP, ANSWER, the rule behind it (PROVENANCE); ROLE, the role a
    /// `fills` asks about.
    Decide { n: u32, at: At, step: u32, question: String, answer: Value, provenance: Value, #[serde(default, skip_serializing_if = "Option::is_none")] role: Option<String> },
    /// A delivery about to start TO's turn (the closing notice, a reminder, a withdrawal, a refusal, words given):
    /// DELIVERY its id, NOTICE which (closed, again, withdrawn, refused, words), DETAIL the delivery's `what` as
    /// coordd's deliveries carry it.
    Notice { n: u32, at: At, to: String, delivery: String, notice: String, detail: Value },
}

/// A map by post number, read from JSON, whose keys are strings ("2"). serde_json turns them back into numbers only when
/// it reads the map itself: inside a `#[serde(flatten)]` (coord-types' `Event`, every line of the bridge) the content
/// is buffered first and a key "2" is refused as not a u32, so the window could read no snapshot that held a mention
/// (paper-research N-539: "Nothing between you and paper-research yet" in the owner's window from 697780a on).
fn by_post<'de, D: serde::Deserializer<'de>>(d: D) -> Result<std::collections::BTreeMap<u32, Vec<String>>, D::Error> {
    let m: std::collections::BTreeMap<String, Vec<String>> = Deserialize::deserialize(d)?;
    m.into_iter().map(|(k, v)| k.parse().map(|k| (k, v)).map_err(serde::de::Error::custom)).collect()
}

fn halts_of(m: &Machine, x: &Ticket) -> Vec<ViewHalt> {
    use crate::machine::Halting;
    let (agent, owed, role) = match &x.status {
        Status::Awaits { agent, owed, .. } => (Some(agent.clone()), Some(owed), None),
        Status::Asking { owed, role } => (None, Some(owed), Some(role.clone())),
        _ => (None, None, None),
    };
    x.halts
        .iter()
        .map(|h| {
            let (n, at) = (h.n, h.at.clone());
            match &h.what {
                Halting::Give { step, .. } => ViewHalt::Give {
                    n,
                    at,
                    step: *step,
                    call: owed.map(|o| o.call.clone()).unwrap_or_default(),
                    worker: agent.clone().unwrap_or_default(),
                    prompt: owed.and_then(|o| o.prompt.clone()).unwrap_or_default(),
                    edited: owed.is_some_and(|o| o.prompt.is_some()),
                },
                Halting::Take { step, by, head, text } => ViewHalt::Take {
                    n,
                    at,
                    step: *step,
                    call: owed.map(|o| o.call.clone()).unwrap_or_default(),
                    worker: by.clone(),
                    words: text.clone(),
                    head: m.readable(head),
                    ty: owed.map(|o| o.head.clone()).unwrap_or(Type::Any),
                },
                Halting::Deliver { post, with, edited, chose } => {
                    let p = x.posts.iter().find(|p| p.n == *post);
                    ViewHalt::Deliver {
                        n,
                        at,
                        post: *post,
                        with: with.clone(),
                        by: p.map(|p| p.by.clone()).unwrap_or_default(),
                        wakes: p.map(|p| p.wakes().cloned().collect()).unwrap_or_default(),
                        inform: p.map(|p| p.inform.clone()).unwrap_or_default(),
                        edited: *edited,
                        chose: chose.clone(),
                    }
                }
                Halting::Open { .. } => ViewHalt::Open {
                    n,
                    at,
                    ticket: m.id(x.t),
                    kind: x.kind.clone(),
                    form: m.readable(&x.form),
                    fields: m.own_kind(x).map(|k| k.form.clone()).unwrap_or_default(),
                },
                Halting::Decide { step, question, answer, provenance } => {
                    ViewHalt::Decide { n, at, step: *step, question: question.clone(), answer: answer.clone(), provenance: provenance.clone(), role: if question == "fills" { role.clone() } else { None } }
                }
                Halting::Notice { id, to, what } => {
                    use crate::line::What;
                    let notice = match what {
                        What::Closed { .. } => "closed",
                        What::Step { .. } => "again",
                        What::Withdrawn { .. } => "withdrawn",
                        What::Refused { .. } => "refused",
                        What::Words { .. } => "words",
                        What::Posts { .. } => "posts",
                        What::TakenBack { .. } => "taken_back",
                    };
                    ViewHalt::Notice { n, at, to: to.clone(), delivery: id.clone(), notice: notice.into(), detail: serde_json::to_value(what).unwrap_or(Value::Null) }
                }
            }
        })
        .collect()
}

/// T and the tickets below it (children, branches, handler tickets, by parent or `under`), in number order.
fn below(m: &Machine, t: u64) -> Vec<&Ticket> {
    let mut todo = vec![t];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(u) = todo.pop() {
        if seen.insert(u) {
            todo.extend(m.tickets.values().filter(|y| y.parent == Some(u) || y.under == Some(u)).map(|y| y.t));
        }
    }
    seen.into_iter().filter_map(|u| m.tickets.get(&u)).collect()
}

/// T and the tickets below it ([`below`]) with halts waiting, in number order.
fn halted_under(m: &Machine, t: u64) -> Vec<u64> {
    if m.tickets.values().all(|y| y.halts.is_empty()) {
        return Vec::new();
    }
    below(m, t).into_iter().filter(|y| !y.halts.is_empty()).map(|y| y.t).collect()
}

/// Whom X waits on, each once, "owner" among them: the steps owed at its tree's leaves (a halted ticket's as the
/// owner's), those of the handlers running under it, a halt down its tree, the asks left with him, and on an agents'
/// thread the agents it waits on (the outline's waits).
fn outline_waits(m: &Machine, x: &Ticket) -> Vec<String> {
    let halted = halted_under(m, x.t);
    let mut waits: Vec<String> = m
        .leaves(x.t)
        .into_iter()
        .filter(|(t, _, _)| !halted.contains(t))
        .map(|(_, a, _)| a)
        .chain(m.tickets.values().filter(|y| y.under == Some(x.t) && y.handling.is_some() && y.is_open()).flat_map(|y| m.leaves(y.t)).map(|(_, a, _)| a))
        .chain((!halted.is_empty()).then(|| "owner".to_string()))
        .chain(m.asks_waiting(x).into_iter().map(|_| "owner".to_string()))
        .chain(m.waits_between(x.t).into_iter().map(|(a, _)| a))
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    waits.retain(|a| seen.insert(a.clone()));
    waits
}

/// When X, closed, stopped being open in the outline ([`ViewOutline::open`]): when it closed (its `coord end`, else
/// its program's return), or later, when the last of what it owed the owner then was cleared, each source of
/// [`outline_waits`] that names him on a closed ticket: the asks of a post there waking him closed (the first of their
/// closers, `Machine::closers`), a step of his in a run handling an event under it answered (its answer's post; one
/// that lapsed is answered so too; one withdrawn, at the run's close), a halt down its tree gone
/// (`Ticket::halt_gone`, looked for once one has gone: `Machine::halt_went`). None while it runs. Insight (3)'s end:
/// a thread ended when it stopped owing him (proto-lead N-1446 p473, 3); a run's step at its answer, as the outline
/// lets the thread go then, not at the run's close (proto-reader p592: N-9 at 10:07, not 10:40).
fn stopped_owing(m: &Machine, x: &Ticket) -> Option<At> {
    let closed = x.ended.as_ref().map(|e| e.at.clone()).or_else(|| x.closed_at.clone())?;
    let asks = x.posts.iter().filter(|p| !p.asks.is_empty() && p.wakes().any(|w| w == "owner")).filter_map(|p| m.closers(x, p).into_iter().next().map(|(at, _)| at));
    let runs = m.tickets.values().filter(|y| y.under == Some(x.t) && y.handling.is_some()).flat_map(|y| {
        let answered = move |n: &u32| y.posts.iter().find(|p| p.step == Some(*n)).map(|p| p.at.clone()).or_else(|| y.closed_at.clone());
        y.step_calls.iter().filter(|(_, (_, who))| who == "owner").filter_map(move |(n, _)| answered(n))
    });
    let halts: Vec<At> = if m.halt_went { below(m, x.t).into_iter().filter_map(|y| y.halt_gone.clone()).collect() } else { Vec::new() };
    std::iter::once(closed).chain(asks).chain(runs).chain(halts).max()
}

/// A clock of the project's (`every D`) or a binding the policy decides (`when D is O`), beside the tickets it opened
/// (their `binding`): what a program's clock is drawn from (0.2 line 26).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClockView {
    /// As written (`Binding.text`), as its tickets name it.
    pub binding: String,
    /// The kind it opens.
    pub kind: String,
    /// Its period in seconds (`every D`); none for `when D is O`, which the policy decides each minute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub every: Option<i64>,
    /// `[decision, option]` (`when D is O`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<(String, String)>,
    /// The agent whose own program it is (its file `kinds/agents/NAME.kind`); none: the project's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    pub file: String,
    pub line: usize,
    /// When it last fired (`Machine::clocks`); none if it never has: it fires at coordd's next round.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<At>,
}

/// One act of an agent's delivery (the message runtime's `delivery_acted` line, syntax::ACTED): ACT (`deliver`, `keep`,
/// `report`) at AT, the ITEMS it took that still waited, the REST a `deliver` named at its foot, and WHY a `keep` or
/// `report` gave. Of ITEMS and of REST only the first ACT_IDS are kept, and MORE_ITEMS, MORE_REST count the others the
/// line names (a keep of everything waiting for a busy agent named 362 on research's record): the line has them all.
/// What the Docket's act chips open (paper-research N-1396 p223, p247); rebuilt by replay, as the desk is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewActed {
    pub act: String,
    pub at: At,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub more_items: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rest: Vec<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub more_rest: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

/// How many of an act's items, and of its rest, `ViewActed` keeps.
pub const ACT_IDS: usize = 8;

fn is_zero(n: &usize) -> bool {
    *n == 0
}

impl ViewActed {
    /// ACT at AT with ITEMS, REST and WHY as its line names them, kept as the view keeps them (ACT_IDS of each).
    pub fn of(act: &str, at: &At, items: &[String], rest: &[String], why: Option<&str>) -> ViewActed {
        let first = |xs: &[String]| xs.iter().take(ACT_IDS).cloned().collect();
        ViewActed { act: act.to_string(), at: at.clone(), items: first(items), more_items: items.len().saturating_sub(ACT_IDS), rest: first(rest), more_rest: rest.len().saturating_sub(ACT_IDS), why: why.map(str::to_string) }
    }
}

/// The agent whose own program FILE is: `…/kinds/agents/NAME.kind` (0.2 line 26), else none.
pub fn written_by(file: &str) -> Option<String> {
    let p = std::path::Path::new(file);
    let dir = p.parent()?;
    if dir.file_name()? != "agents" || dir.parent()?.file_name()? != "kinds" {
        return None;
    }
    p.file_stem()?.to_str().map(String::from)
}

impl Machine {
    /// The clocks (`every D`) and policy-decided bindings (`when D is O`) of the defs, each with when it last fired:
    /// the snapshot's `clocks`. An `on` or `every N of` binding is no clock and is not listed.
    pub fn clock_views(&self) -> Vec<ClockView> {
        self.defs
            .bindings
            .iter()
            .filter_map(|b| {
                let (every, when) = match &b.every {
                    crate::syntax::Every::Clock(s) => (Some(*s), None),
                    crate::syntax::Every::When { decision, option } => (None, Some((decision.clone(), option.clone()))),
                    _ => return None,
                };
                Some(ClockView { binding: b.text.clone(), kind: b.kind.clone(), every, when, by: written_by(&b.file), file: b.file.clone(), line: b.line, last: self.clocks.get(&b.text).cloned() })
            })
            .collect()
    }
}

impl Machine {
    /// X owes the owner something (paper-research N-447 p6, p7): a step of his at a leaf of its tree (a stop's step of
    /// his included, as the handler under it), a halt down its tree, or an agent's asks left with him; whatever X's
    /// state, closed too. The one answer the outline, the thread's page and the sidebar's badge read.
    pub fn owes_owner(&self, x: &Ticket) -> bool {
        outline_waits(self, x).iter().any(|a| a == "owner")
    }

    /// Where X's wait on the owner comes from, as `outline_waits` and the view's `waits_on` take it: each step of his
    /// owed down its tree or in a handler it carries, as (its ticket, its call, the id the view's `waits_on` names it
    /// by: a handler's under X's). What else waits on him there is `owner_waits_else`, read apart: coordd reads these
    /// steps for every view it sends (`lapse_shown`), and the asks that `owner_waits_else` reads were a pass over every
    /// ticket for each, never used there (docket-core N-1446 p744). The machine reads no clock: coordd reads this with
    /// its own, so a step of his that goes on by itself (`lapses_at`) is not his to answer (proto-reader N-1446 p136,
    /// proto-lead p140).
    pub fn owner_steps(&self, x: &Ticket) -> Vec<(u64, String, String)> {
        let halted = halted_under(self, x.t);
        let own = self.leaves(x.t).into_iter().map(|(t, a, call)| (t, a, call, self.id(t)));
        let carried = self.tickets.values().filter(|y| y.under == Some(x.t) && y.handling.is_some() && y.is_open()).flat_map(|y| self.leaves(y.t)).map(|(t, a, call)| (t, a, call, self.id(x.t)));
        own.chain(carried).filter(|(t, a, _, _)| a == "owner" && !halted.contains(t)).map(|(t, _, call, named)| (t, call, named)).collect()
    }

    /// Whether anything waits on the owner at X besides his steps (`owner_steps`): a halt below it, an agent's asks left
    /// with him.
    pub fn owner_waits_else(&self, x: &Ticket) -> bool {
        !halted_under(self, x.t).is_empty() || !self.asks_waiting(x).is_empty()
    }
}

fn halted_below(m: &Machine, t: u64) -> Vec<String> {
    let mut out: Vec<u64> = Vec::new();
    if m.tickets.values().all(|y| y.halts.is_empty()) {
        return Vec::new();
    }
    let mut todo: Vec<u64> = m.tickets.values().filter(|y| y.parent == Some(t) || y.under == Some(t)).map(|y| y.t).collect();
    let mut seen = std::collections::BTreeSet::new();
    while let Some(u) = todo.pop() {
        if !seen.insert(u) {
            continue;
        }
        if m.tickets.get(&u).is_some_and(|y| !y.halts.is_empty()) {
            out.push(u);
        }
        todo.extend(m.tickets.values().filter(|y| y.parent == Some(u) || y.under == Some(u)).map(|y| y.t));
    }
    out.sort();
    out.into_iter().map(|u| m.id(u)).collect()
}

/// A node of a ticket's program and where it stands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunNode {
    /// step, open, post, tell, drop, mark, give, move, return, raise, hole, continue, reply; match and its `arm`s;
    /// par, any and their `branch`es; try and its `handle`s; iterate, with, let.
    pub what: String,
    /// Its line in the program it is in.
    pub line: u32,
    /// The program it is in: 0 the kind's (or an edit's, with BY); N the minder's answer N (`ViewShaped.n`).
    pub source: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    /// The node in one line (`x <- step ...`; a match without its arms; an arm is its pattern, a handle its exception).
    pub text: String,
    /// A step's worker (as written until it runs, then the agent); whom a `tell` or a `post` named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub who: Option<String>,
    /// done, now, to_come, not_taken.
    pub state: String,
    /// When it was reached (its last visit), for putting nodes and posts in one order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<At>,
    /// What came back (a step's answer, an opened ticket's result, what a `return` gave).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// The tickets it opened.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tickets: Vec<String>,
    /// A match's arms; an arm's, a branch's, a block's statements; the program that filled a hole or took a step's
    /// place.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<RunNode>,
    /// A step's rounds, one per time it was given (a step in a loop has one each round), oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rounds: Vec<RunRound>,
    /// A step's answer type (its call's head, or the inline step's `-> T`).
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub ty: Option<Type>,
    /// A step refused to its agent for its model (`model_refused`; docket-edge N-235, for docket-core4 to keep).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<ViewRefused>,
}

/// One round of a step: its number on the ticket; whom it was given to, when, and where the exact prompt sent is (the
/// delivery's place in the worker's transcript); when it was answered, with what head, and the answer's post.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunRound {
    pub step: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub given: Option<At>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<Loc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered: Option<At>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<u32>,
    /// Answered by coordd itself (`coordds_own`: the watcher, the minder), with a call of its own: its wire is the
    /// record's lines for the step, not an agent's turn.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub own: bool,
    /// For a step given more than once (a reminder, its process lost, passed up), the giving its answer came after (the
    /// latest, while it has none), when that is not the first: an agent's wire is that one's requests from then to
    /// `answered`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub took: Option<RunTook>,
}

/// A giving of a step's round (`RunRound::took`): to whom, when, and its place in that one's transcript.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunTook {
    pub to: String,
    pub at: At,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<Loc>,
}

/// A run of a handler under a ticket (`Ticket.handling`), as `TicketView::ran` lists it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewRan {
    /// Its own ticket, which `Request::Ticket` opens.
    pub id: String,
    /// The event it ran for (`owner_post`, `agent_post`, `ask`, ...).
    pub event: String,
    /// The `when` it ran: the kind it was written in and its line there, as they were when it opened (`Ticket.when`).
    /// None for a run of no `when` of a kind (a delivery's desk run, a stop).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// The post it ran for: the owner's post it held (`Ticket.post_of`), an agent's post it read (its value's text).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<u32>,
    pub opened_at: At,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<At>,
    /// What it came to, in a line (`said`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub said: Option<String>,
}

/// The runs of a handler under X (`TicketView::ran`), open and closed, oldest first; with them, the open ones'
/// views (`TicketView::handlers`), from one pass over the tickets.
fn runs_under(m: &Machine, x: &Ticket) -> (Vec<ViewRan>, Vec<TicketView>) {
    // by number, which is the order they opened in: a ticket's runs are all hidden, or all desk tickets under a delivery
    // ticket (`handler_ticket`), each numbered in its own range
    let runs: Vec<&Ticket> = m.tickets.values().filter(|y| y.under == Some(x.t) && y.handling.is_some()).collect();
    let open = runs.iter().filter(|y| y.is_open()).map(|y| TicketView::of(m, y)).collect();
    let ran = runs
        .iter()
        .map(|y| ViewRan {
            id: m.id(y.t),
            event: y.handling.clone().unwrap_or_default(),
            from: y.when.as_ref().map(|(k, _)| k.clone()),
            line: y.when.as_ref().map(|(_, l)| *l as u32),
            // a closed run's form is its text (`form_text`), read back only where it names a `text`
            post: y.post_of.as_ref().map(|p| p.n).or_else(|| match y.form_field("text").as_ref().and_then(Text::of) {
                Some(Text::Post { post }) if post.t == x.t => Some(post.n),
                _ => None,
            }),
            opened_at: y.opened_at.clone(),
            closed_at: y.closed_at.clone(),
            said: said(m, y),
        })
        .collect();
    (ran, open)
}

/// The answer to Y's last step as its trace keeps it: its head and fields, as `answers::plain` leaves them, so neither
/// who gave it (`Ticket.last_by`) nor its words (docket-edge5's wording, N-1446 p336; docket-edge2 p334). None before any
/// step of Y is answered. Taken out of `came_to` (docket-edge5's edge5-said 6196fa0f, consult-3 N-1446 p244).
pub(crate) fn last_answer(y: &Ticket) -> Option<&Value> {
    y.trace.visits.values().filter_map(|v| Some((v.steps.iter().max()?, v.value.as_ref()?))).max_by_key(|(n, _)| **n).map(|(_, h)| h)
}

/// What ticket Y came to (docket-edge5's order, N-1446, for proto-reader's p197): why it was ended; else its result,
/// once it is done and the result says something; else the answer to its last step, as its trace keeps it (coordd's own
/// answers make no post), while it runs or when its result says nothing (none, an empty list or record: a handler that
/// told a post at once returns what its `tell` gave); else what it raised. A value, or a line in words already. None
/// while it runs with no answer yet.
fn came_to(m: &Machine, y: &Ticket) -> Option<Result<Value, String>> {
    let empty = |v: &Value| match v {
        Value::Null => true,
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        Value::String(s) => s.is_empty(),
        _ => false,
    };
    if let Some(e) = &y.ended {
        return Some(Err(e.why.clone()));
    }
    if let Status::Done(v) = &y.status {
        let v = m.readable(v);
        if !empty(&v) {
            return Some(Ok(v));
        }
    }
    if let Some(h) = last_answer(y) {
        return Some(Ok(m.readable(h)));
    }
    match &y.status {
        Status::Failed(r) => Some(Err(format!("raised {}", r.exc))),
        _ => None,
    }
}

/// What ticket Y came to (`came_to`), in one line as a value prints. A handler's run says itself with it
/// (`ViewRan::said`): its watcher's answer, since every `when` ends in what says nothing.
pub fn said(m: &Machine, y: &Ticket) -> Option<String> {
    let s = match came_to(m, y)? {
        Ok(v) => crate::print::value(&v),
        Err(s) => s,
    };
    Some(crate::label::first_line(&s, 160)).filter(|s| !s.is_empty())
}

/// What a mark came to (`came_to`), in words for his margin, never as a value prints (proto-reader N-1446 p197,
/// consult-3 p205): a value with one text field by that text (`Failed(why)`), any other by its head word (Quiet,
/// Said, Joined), a text as it is. A list, a number or a record says nothing, and the mark's post or its label stands.
/// None while it runs: a step's answer on the way (a brain's `Read`, an aside's `Through`) is a step on the way, not what
/// it came to, so its label stands, with its state beside it (consult-3 p244, p258). A mark that raised says what it
/// raised, in the raise's own words (its `why`; a raise whose value is a text, "raised X: TEXT", since that text may be
/// a sentence, the engine's own NoCall, or only a name, a step's Silent its agent; else "raised X"), never the answer of
/// a step before it: a brain whose `elect` was refused a third time raised Malformed, and its label stood with nothing
/// said (consult-3 p258.1, proto-reader p262; a text's raise, consult-3 p504).
pub fn said_in_words(m: &Machine, y: &Ticket) -> Option<String> {
    if y.ended.is_none() && y.is_open() {
        return None;
    }
    if let (None, Status::Failed(r)) = (&y.ended, &y.status) {
        let why = r.value.get("why").and_then(Value::as_str).filter(|w| !w.trim().is_empty());
        let text = r.value.as_str().filter(|w| !w.trim().is_empty());
        let s = why.map_or_else(|| text.map_or_else(|| format!("raised {}", r.exc), |w| format!("raised {}: {w}", r.exc)), str::to_string);
        return Some(crate::label::first_line(&s, 160)).filter(|s| !s.is_empty());
    }
    let s = match came_to(m, y)? {
        Err(s) => s,
        Ok(Value::String(s)) => s,
        Ok(Value::Object(o)) => {
            let head = o.get("head")?.as_str()?;
            let mut fields = o.iter().filter(|(k, _)| *k != "head");
            match (fields.next(), fields.next()) {
                (Some((_, Value::String(t))), None) => t.clone(),
                _ => head.to_string(),
            }
        }
        Ok(_) => return None,
    };
    Some(crate::label::first_line(&s, 160)).filter(|s| !s.is_empty())
}

/// Ticket X's program as RunNodes, from its trace.
fn run_of(m: &Machine, x: &Ticket) -> Vec<RunNode> {
    // a handler's ticket has one as any ticket has (its program is the handler's body; docket-edge N-93)
    if x.kind == "part" {
        return Vec::new();
    }
    let mut out = Vec::new();
    // its kind's text as it runs it: the first, or the one a reload gave it (live edits)
    let part = x.trace.kind_part();
    Tree { m, x }.seq(part, &x.trace.parts[part].prog, false, None, &mut out);
    out
}

struct Tree<'a> {
    m: &'a Machine,
    x: &'a Ticket,
}

impl Tree<'_> {
    /// The statements of P (a `<-` is its first part, then the rest), in part PART. SKIP: in an arm not taken.
    fn seq(&self, part: usize, p: &crate::syntax::Prog, skip: bool, bind: Option<String>, out: &mut Vec<RunNode>) {
        use crate::syntax::Prog;
        match p {
            Prog::Bind { x, first, rest } => {
                let b = crate::print::bound_of(x);
                self.seq(part, first, skip, (b != "_").then_some(b), out);
                self.seq(part, rest, skip, None, out);
            }
            _ => out.push(self.node(part, p, skip, bind)),
        }
    }

    fn block(&self, what: &str, text: String, line: usize, part: usize, body: &crate::syntax::Prog, skip: bool) -> RunNode {
        let mut nodes = Vec::new();
        self.seq(part, body, skip, None, &mut nodes);
        let state = if nodes.iter().any(|n| n.state == "now") {
            "now"
        } else if nodes.iter().any(|n| n.state == "done") {
            "done"
        } else if skip || !self.x.is_open() {
            "not_taken"
        } else {
            "to_come"
        };
        let q = &self.x.trace.parts[part];
        RunNode { what: what.into(), line: line as u32, source: q.source, by: q.by.clone(), text, who: None, state: state.into(), at: None, value: None, tickets: Vec::new(), nodes, rounds: Vec::new(), ty: None, refused: None }
    }

    fn node(&self, part: usize, p: &crate::syntax::Prog, skip: bool, bind: Option<String>) -> RunNode {
        use crate::syntax::Prog;
        let tr = &self.x.trace;
        let (what, text) = crate::print::node(p);
        let key = (part, p.at(), text.clone());
        let visit = tr.visits.get(&key);
        let mut nodes = Vec::new();
        match p {
            Prog::Match { arms, .. } => {
                for (i, (pat, body)) in arms.iter().enumerate() {
                    let not = skip || visit.is_some_and(|v| v.arm != Some(i));
                    nodes.push(self.block("arm", crate::print::pat(pat), body.at(), part, body, not));
                }
            }
            Prog::Par { branches, each: Some(_), .. } | Prog::Any { branches, each: Some(_), .. } => {
                // a branch per element, each in a ticket of its own; the one written when none has run
                let ts: Vec<u64> = visit.map(|v| v.tickets.clone()).unwrap_or_default();
                if ts.is_empty() {
                    nodes.push(self.block("branch", "each branch".into(), branches[0].at(), part, &branches[0], skip));
                }
                for (i, c) in ts.iter().enumerate() {
                    let x = self.m.tickets.get(c).unwrap_or(self.x);
                    nodes.push(Tree { m: self.m, x }.block("branch", format!("branch {}", i + 1), branches[0].at(), part, &branches[0], skip));
                }
            }
            Prog::Par { branches, .. } | Prog::Any { branches, .. } => {
                for (i, b) in branches.iter().enumerate() {
                    // a branch runs in a ticket of its own, which keeps its trace
                    let x = visit.and_then(|v| v.tickets.get(i)).and_then(|c| self.m.tickets.get(c)).unwrap_or(self.x);
                    nodes.push(Tree { m: self.m, x }.block("branch", format!("branch {}", i + 1), b.at(), part, b, skip));
                }
            }
            Prog::Iterate { body, .. } | Prog::For { body, .. } | Prog::Layer { body, .. } | Prog::With { body, .. } | Prog::LetAgent { body, .. } => self.seq(part, body, skip, None, &mut nodes),
            Prog::Try { body, arms, .. } => {
                self.seq(part, body, skip, None, &mut nodes);
                for h in arms {
                    nodes.push(self.block("handle", format!("{}{}", h.exc, h.x.as_ref().map(|x| format!("({x})")).unwrap_or_default()), h.body.at(), part, &h.body, skip));
                }
            }
            _ => {}
        }
        // the program that filled this hole, or took this step's place (an undone answer of the minder's is gone)
        let undone = |n: u32| self.x.shaped.iter().any(|s| s.n == n && s.undone);
        if let Some(j) = tr.parts.iter().rposition(|q| q.at.as_ref() == Some(&key) && !(q.by.is_none() && undone(q.source))) {
            self.seq(j, &tr.parts[j].prog, false, None, &mut nodes);
        }
        let state = if nodes.iter().any(|n| n.state == "now") || (visit.is_some() && tr.now.as_ref() == Some(&key) && self.x.is_open() && nodes.is_empty()) {
            "now"
        } else if visit.is_some() {
            "done"
        } else if skip || !self.x.is_open() {
            "not_taken"
        } else {
            "to_come"
        };
        let q = &tr.parts[part];
        let who = visit.and_then(|v| v.who.clone()).or_else(|| match p {
            Prog::Step { w, .. } => Some(crate::print::worker(w)),
            _ => None,
        });
        let rounds = visit.map(|v| v.steps.iter().map(|s| self.round(*s, who.as_deref())).collect()).unwrap_or_default();
        RunNode {
            what: what.into(),
            line: p.at() as u32,
            source: q.source,
            by: q.by.clone(),
            text: bind.map_or(text.clone(), |b| format!("{b} <- {text}")),
            who,
            state: state.into(),
            at: visit.and_then(|v| v.at.clone()),
            value: visit.and_then(|v| v.value.clone()).map(|v| self.m.readable(&v)),
            tickets: visit.map(|v| v.tickets.iter().map(|c| self.m.id(*c)).collect()).unwrap_or_default(),
            nodes,
            rounds,
            refused: visit.and_then(|v| v.refused.clone()).map(|(agent, why, at)| ViewRefused { agent, why, at }),
            // as it ran; a step not reached yet, as written (its inline head, or its call's today)
            ty: visit.and_then(|v| v.ty.clone()).or_else(|| match p {
                Prog::Step { inline: Some((_, head)), .. } => Some(head.clone()),
                Prog::Step { call, .. } => self.m.defs.calls.get(call).map(|c| c.head.clone()),
                _ => None,
            }),
        }
    }

    /// Round STEP of this ticket: the delivery of the step (the first given, with its place once found), the answer's
    /// post, and for a step given again the giving its answer came after (`took`). WHO: the step's worker, whose it is
    /// when no giving says (coordd's own are not handed through a session).
    fn round(&self, step: u32, who: Option<&str>) -> RunRound {
        let d = format!("{}.s{step}", self.m.id(self.x.t));
        let g = self.x.given.iter().filter(|g| g.delivery == d).min_by_key(|g| g.place.is_none());
        let a = self.x.posts.iter().find(|p| p.step == Some(step));
        let gs: Vec<&crate::machine::GivenRec> = self.x.given.iter().filter(|g| g.delivery == d).collect();
        let last = gs.iter().rposition(|g| a.is_none_or(|a| g.at <= a.at));
        let to = g.map(|g| g.to.clone());
        RunRound {
            step,
            own: to.as_deref().or(who).is_some_and(crate::machine::coordds_own),
            to,
            given: gs.first().map(|g| g.at.clone()),
            place: g.and_then(|g| g.place.clone()),
            answered: a.map(|p| p.at.clone()),
            head: a.and_then(|p| p.head.clone()).map(|h| self.m.readable(&h)),
            post: a.map(|p| p.n),
            took: last.filter(|i| *i > 0).map(|i| RunTook { to: gs[i].to.clone(), at: gs[i].at.clone(), place: gs[i].place.clone() }),
        }
    }
}

/// One agent asked by his post POST (`TicketView::asked`), with ANSWER, the post of its that answered it
/// (`N-1104.p679`), None while it is open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewAsked {
    pub post: u32,
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
}

/// `TicketView::asked` of ticket X: of his posts, each agent asked (`Machine::asked`).
fn asked_of(m: &Machine, x: &Ticket) -> Vec<ViewAsked> {
    x.posts
        .iter()
        .filter(|p| p.by == "owner")
        .flat_map(|p| m.asked_post(x, p).into_iter().map(move |(agent, answer)| ViewAsked { post: p.n, agent, answer }))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewOwner {
    /// The owner has posted since the last agent's post.
    pub spoke_last: bool,
    /// The agents who posted after the owner's last post (all who posted, if the owner never did), in the order of
    /// their first such post: their replies wait on the owner.
    pub unanswered: Vec<String>,
}

impl ViewOwner {
    /// Posts held or dropped (design-two-systems §13) are not among them: the minder is shown neither.
    pub fn of(posts: &[Post]) -> ViewOwner {
        let posts: Vec<Post> = posts.iter().filter(|p| p.visible()).cloned().collect();
        let posts = &posts[..];
        let agent = |p: &&Post| p.by != "owner" && p.by != crate::machine::NOBODY && p.by != "minder";
        let last_owner = posts.iter().rposition(|p| p.by == "owner");
        let after = &posts[last_owner.map_or(0, |i| i + 1)..];
        let mut unanswered: Vec<String> = Vec::new();
        for p in after.iter().filter(agent) {
            if !unanswered.contains(&p.by) {
                unanswered.push(p.by.clone());
            }
        }
        ViewOwner { spoke_last: last_owner.is_some() && unanswered.is_empty(), unanswered }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewTemplate {
    pub call: String,
    /// The ticket it was made on.
    pub on: String,
    /// Made for that ticket and the tickets under it.
    pub tree: bool,
    /// The step owed now that carries it, if one does (it goes with that step's prompt).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
    pub text: String,
    /// When he made it (its `Line::Template`'s time): what the reach names it by.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub at: crate::At,
}

/// How a thread was ended (`Line::Ended`, `coord end`): HOW `done`, `dropped` or `kept`, WHY its one line, BY whom, AT.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewEnded {
    pub how: String,
    pub why: String,
    pub by: String,
    pub at: At,
}

/// A post whose words name a ticket, one of its posts, or an agent (paper-research N-412 p166/p169): post POST of
/// ticket BY, BY's LABEL (its line in the outline), WHO wrote it and AT; OF, the post that was named, when a post was.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewMention {
    pub by: String,
    pub post: u32,
    pub label: String,
    pub who: String,
    pub at: At,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of: Option<u32>,
}

/// A thread as the outline shows it (paper-research N-101 p1676, c): its label, the thread it stands under and at
/// which of that one's posts, who has it, its state in one word, and the threads under it in the order of the posts
/// they stand at. Only threads and items are lines of it; what else is under a thread is behind the click.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewOutline {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_post: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub who: Option<String>,
    /// done, dropped, kept or failed; waits on you; waits on NAME, …; quiet; at work.
    pub state: String,
    /// It waits on the owner: its state is "waits on you".
    #[serde(default)]
    pub owed: bool,
    /// The agents it waits on (the owner not among them), whatever its state word.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on: Vec<String>,
    pub open: bool,
    /// When it stopped being open, none while it is open: when it closed, or later, when it stopped owing the owner
    /// ([`stopped_owing`]). Insight (3)'s end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<At>,
    /// Why it ended, when it was ended (`coord end`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    pub opened_at: At,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ViewOutline>,
    /// The words of the post it stands at that it was opened on (`under` T.pN:FROM+LEN, its form's `quote`), as
    /// they were then; none when the whole post was its place (paper-research N-447 p71).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    /// The owner archived it (`TicketView::archived`); its lines under it go with it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub archived: bool,
    /// A mark's: what it came to, in words (`said_in_words`: Quiet, Said, a failure's why); none for a line, or while
    /// it runs. The window's mark line draws it after the mark's own post (outline.rs `mark_says`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub said: Option<String>,
}

/// How X stands in the thread it stands at (`Machine::outline_parent`): as its kind says (`drawn line`, `drawn mark`,
/// `drawn note`; proto-lead N-1446 p17), by today's kind, since how a ticket is drawn is a view of it and not what it
/// runs (a live edit of the clause draws its closed tickets anew too). A kind that says nothing, or one no longer
/// defined, keeps the rule from before the clause: a thread or an item is a line, nothing else is drawn.
pub fn drawn(m: &Machine, x: &Ticket) -> Option<crate::syntax::Drawn> {
    use crate::syntax::Drawn;
    m.defs.kinds.get(&x.kind).and_then(|k| k.drawn).or_else(|| matches!(x.kind.as_str(), "thread" | "item").then_some(Drawn::Line))
}

/// The tickets that are lines of the outline: drawn as a line (`drawn`).
pub fn outlined(m: &Machine, x: &Ticket) -> bool {
    drawn(m, x) == Some(crate::syntax::Drawn::Line)
}

/// The tickets that stand in a thread at all: its lines and its marks (a note is a mark). A mark is one of a thread's
/// children where it stands, never a line of the outline of its own (`outline_rows` leaves it out).
pub fn stands(m: &Machine, x: &Ticket) -> bool {
    drawn(m, x).is_some()
}

impl ViewOutline {
    /// X's line, with CHILDREN its lines under it (none: a row of `coord outline`, whose rows give their parents).
    pub fn of(m: &Machine, x: &Ticket, children: bool) -> ViewOutline {
        let (parent, at_post) = m.outline_parent(x.t).map_or((None, None), |(p, n)| (Some(m.id(p)), Some(n)));
        let waits = outline_waits(m, x);
        let owed = waits.iter().any(|a| a == "owner");
        let who = waits.iter().find(|a| *a != "owner").cloned().or_else(|| x.form.get("to").and_then(Value::as_str).map(String::from)).or_else(|| x.opener.clone()).or_else(|| x.raised_by.clone());
        let between = matches!(&x.status, Status::Done(v) if v["head"] == "Between");
        let state = match &x.status {
            // what owes him shows whatever its state: a closed thread with an ask of his open (3h), an older ended one
            // (paper-research N-447 p7)
            _ if owed => "waits on you".to_string(),
            // an agents' thread ended (`coord end`): how it was ended
            _ if between && x.ended.is_some() => x.ended.as_ref().map_or_else(String::new, |e| e.how.clone()),
            Status::Done(v) if !between => match v["head"].as_str() {
                Some("Dropped") => "dropped".to_string(),
                Some("Kept") => "kept".to_string(),
                // a pass that said nothing, or one whose call failed (brain.kind's Quiet, Same and Failed): a mark that
                // came to nothing, which stands nowhere in his thread (outline.rs `marks`; proto-mind N-1446 p270)
                Some("Quiet" | "Same") => "said nothing".to_string(),
                Some("Failed") => "failed".to_string(),
                _ => "done".to_string(),
            },
            Status::Failed(_) => "failed".to_string(),
            // one it waits on only, and that one has it: at work (docket-edge N-397.p2)
            _ if waits.len() == 1 && who.as_ref() == waits.first() => "at work".to_string(),
            _ if !waits.is_empty() => format!("waits on {}", waits.join(", ")),
            _ if between || matches!(x.status, Status::Rests { .. }) => "quiet".to_string(),
            _ => "at work".to_string(),
        };
        // an agents' thread (Between) is open work whatever its state, quiet too (paper-research N-101 p1760), till it is
        // ended (N-447); so is one that owes him (N-447 p7)
        let open = x.is_open() || (between && x.ended.is_none()) || owed;
        // the thread a ticket stands under first: `stands` reads its kind by name, and this pass runs over every ticket
        // for each view's line, under coordd's lock (the most of what 51 added to a snapshot, docket-core N-1446 p744)
        let mut kids: Vec<(u32, u64)> = if children {
            m.tickets.values().filter(|y| y.t != x.t && m.outline_up(y.t) == Some(x.t) && stands(m, y)).filter_map(|y| m.outline_parent(y.t).map(|(_, n)| (n, y.t))).collect()
        } else {
            Vec::new()
        };
        kids.sort();
        ViewOutline {
            id: m.id(x.t),
            label: crate::label::ticket(m, x.t, &crate::label::own_words),
            parent,
            at_post,
            who,
            owed,
            on: waits.iter().filter(|a| *a != "owner").cloned().collect(),
            state,
            open,
            ended_at: if open { None } else { stopped_owing(m, x) },
            why: x.ended.as_ref().map(|e| e.why.clone()),
            opened_at: x.opened_at.clone(),
            children: kids.into_iter().filter_map(|(_, t)| m.tickets.get(&t)).map(|y| ViewOutline::of(m, y, false)).collect(),
            quote: x.form.get("quote").and_then(|q| q.get("words")).and_then(Value::as_str).map(str::to_string),
            archived: x.archived,
            said: drawn(m, x).is_some_and(crate::syntax::Drawn::is_mark).then(|| said_in_words(m, x)).flatten(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewQuiet {
    pub agent: String,
    /// The last post it was given there.
    pub upto: u32,
}

/// The owner's post N, and where it is for each agent it went to (`TicketView::whereabouts`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PostWhere {
    pub n: u32,
    pub agents: Vec<Where>,
}

/// Where a post of the owner's is for one AGENT it went to (it wakes the agent: typed to it, named, or told by its
/// handler): every fact there is, so the window shows the newest and its click the whole chain with its times
/// (consult-lead, N-818 post 31, 1). None of them is a record line of its own; each is read off the lines there are.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Where {
    pub agent: String,
    /// Not handed to AGENT yet (its read mark here is below the post).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting: Option<Waiting>,
    /// When the delivery that carried it was given (`Line::Given`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handed: Option<At>,
    /// When that delivery's text entered AGENT's model input (`GivenRec::delivered`, 0.2 A1): `delivered 12:28` on the
    /// line under his post; until then a handed post reads `sent` (docket-edge4, N-818 post 72; consult-3 post 74).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered: Option<At>,
    /// When the turn it was given in ended (`GivenRec::read`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read: Option<At>,
    /// A step AGENT owes now on this ticket, or on one under it opened after the post.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<WhereStep>,
    /// The first post of AGENT's that answers it, here or on another ticket: one that answers it by number
    /// (`answers`, `AnswersAlso`), or a reply naming him written once AGENT had been handed it (`answers_upto`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered: Option<WhereAnswer>,
}

/// AHEAD: how many of his posts to the same agent, on any ticket, came before this one and are not handed to it either
/// ("2 of yours before this one", docket-edge4 N-818 post 28).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Waiting {
    pub ahead: u32,
}

/// The step owed: on ticket ON (its id), step STEP, named by its CALL (the name `waits_on` carries; an inline step's,
/// the first line of its text), owed SINCE.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WhereStep {
    pub on: String,
    pub step: u32,
    pub call: String,
    pub since: At,
}

/// The answering POST by its id (`N-1104.p14`), posted AT.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WhereAnswer {
    pub post: String,
    pub at: At,
}

/// For each agent, the owner's posts not handed to it, oldest first by (time, ticket, post) (`owner_unhanded`).
pub type Unhanded = std::collections::BTreeMap<String, Vec<(At, u64, u32)>>;

/// For each agent, the owner's visible posts, on any ticket, that wake it and that it has not been handed (its read
/// mark on their ticket is below them), oldest first by (time, ticket, post): what `Waiting.ahead` counts. Made once
/// for all the views until a ticket changes (`Machine::unhanded`).
pub fn owner_unhanded(m: &Machine) -> Unhanded {
    let mut out = Unhanded::new();
    for x in m.tickets.values() {
        for p in x.posts.iter().filter(|p| p.by == "owner" && p.visible() && p.moved.is_none()) {
            for a in p.wakes().filter(|a| *a != "owner") {
                // or told it late, its mark past (`Ticket::owes_late`)
                if x.mu.get(a.as_str()).copied().unwrap_or(0) < p.n || x.owes_late(a, p.n) {
                    out.entry(a.clone()).or_default().push((p.at.clone(), x.t, p.n));
                }
            }
        }
    }
    for l in out.values_mut() {
        l.sort();
        l.dedup();
    }
    out
}

/// `TicketView::whereabouts` of ticket X: for each of the owner's visible posts that wakes an agent, where it is for
/// each such agent. `ahead` is read from `Machine::unhanded`, asked only when a post here waits.
pub fn whereabouts(m: &Machine, x: &Ticket) -> Vec<PostWhere> {
    let mine: Vec<&Post> = x.posts.iter().filter(|p| p.by == "owner" && p.visible() && p.moved.is_none()).collect();
    if mine.is_empty() {
        return Vec::new();
    }
    let id = m.id(x.t);
    let shown: Vec<&Post> = x.posts.iter().filter(|p| p.visible() && p.moved.is_none()).collect();
    // the steps owed down this ticket's tree and on the tickets under it: (ticket, agent, step, call, since, opened)
    let mut owed: Vec<(u64, String, u32, String, At, At)> = Vec::new();
    let mut steps_of = |t: u64| {
        for (u, agent, call) in m.leaves(t) {
            let Some(y) = m.tickets.get(&u) else { continue };
            if let Status::Awaits { owed: o, since, .. } = &y.status {
                let call = if call == crate::syntax::INLINE { o.template.as_deref().and_then(|t| t.lines().map(str::trim).find(|l| !l.is_empty())).unwrap_or(&call).to_string() } else { call };
                owed.push((u, agent, o.step, call, since.clone(), y.opened_at.clone()));
            }
        }
    };
    steps_of(x.t);
    for y in m.tickets.values().filter(|y| y.under == Some(x.t) && y.handling.is_none() && y.is_open()) {
        steps_of(y.t);
    }
    let mut unhanded: Option<std::sync::Arc<Unhanded>> = None;
    let mut out = Vec::new();
    for p in mine {
        let mut agents: Vec<Where> = Vec::new();
        for a in p.wakes().filter(|a| *a != "owner") {
            if agents.iter().any(|w| w.agent == *a) {
                continue;
            }
            let mu = x.mu.get(a.as_str()).copied().unwrap_or(0);
            // told to it late (`ToldLate`): waiting until its own delivery is given, which hands it (`Ticket::hands`)
            let late = x.late_for(a, p.n);
            let waiting = if mu < p.n || x.owes_late(a, p.n) {
                let list = unhanded.get_or_insert_with(|| m.unhanded());
                let ahead = list.get(a.as_str()).map_or(0, |l| l.iter().filter(|(at, t, n)| (at.as_str(), *t, *n) < (p.at.as_str(), x.t, p.n)).count());
                Some(Waiting { ahead: ahead as u32 })
            } else {
                None
            };
            // a giving made due again (not read, or his post left unanswered in its turn) is not where it is
            let given = x.given.iter().find(|g| g.to == *a && !g.again() && Ticket::hands(g, p.n));
            let step = owed
                .iter()
                .find(|(u, agent, _, _, _, opened)| agent == a && (*u == x.t || *opened >= p.at))
                .map(|(u, _, step, call, since, _)| WhereStep { on: m.id(*u), step: *step, call: call.clone(), since: since.clone() });
            let here = shown
                .iter()
                .find(|q| {
                    q.n > p.n
                        && q.by == *a
                        && (q.answers == Some(p.n)
                            || q.also.iter().any(|o| *o == format!("{id}.p{}", p.n))
                            // a reply naming him answers only what its writer had been handed when it wrote it, the
                            // machine's own rule (`waits_between`; consult-3, N-412 post 1916)
                            || q.answers.is_none() && q.names.iter().any(|n| n == "owner") && p.wakes().any(|w| *w == q.by) && x.answers_upto(q) >= p.n && (!late || x.answers(q, p.n)))
                })
                .map(|q| WhereAnswer { post: format!("{id}.p{}", q.n), at: q.at.clone() });
            let elsewhere = || {
                x.answered_from.iter().filter(|(n, by, _)| *n == p.n && by == a).find_map(|(_, _, post)| {
                    let (t, n) = crate::post_id(post)?;
                    let q = m.tickets.get(&t)?.posts.iter().find(|q| q.n == n)?;
                    Some(WhereAnswer { post: post.clone(), at: q.at.clone() })
                })
            };
            let answered = match (here, elsewhere()) {
                (Some(h), Some(e)) => Some(if e.at < h.at { e } else { h }),
                (h, e) => h.or(e),
            };
            agents.push(Where { agent: a.clone(), waiting, handed: given.map(|g| g.at.clone()), delivered: given.and_then(|g| g.delivered.clone()), read: given.and_then(|g| g.read.clone()), step, answered });
        }
        if !agents.is_empty() {
            out.push(PostWhere { n: p.n, agents });
        }
    }
    out
}

/// An owner's step that went on by itself: its STEP, the POST that holds the answer, AFTER seconds, BECAUSE.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewLapsed {
    pub step: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<u32>,
    pub after: u64,
    pub because: String,
}

impl ViewLapsed {
    /// What is shown with the post: "went on by itself after 30 s", or "went on by itself: his window was closed".
    pub fn mark(&self) -> String {
        if self.because == "closed" { "went on by itself: his window was closed".to_string() } else { format!("went on by itself after {} s", self.after) }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewShaped {
    pub n: u32,
    pub at: At,
    pub answer: Value,
    pub undone: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
}

/// A step owed at a leaf of a ticket's tree: on which ticket, by whom, for which call.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WaitsOn {
    pub ticket: String,
    pub agent: String,
    pub call: String,
    /// `asks`: the owner, for the asks of this post (its author's; one line per asker, design §16, 4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<u32>,
    /// The layer the step is in (`Machine::layer_of`; N-101 p414): coord show and the app say it beside its agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "is", rename_all = "snake_case")]
pub enum ViewStatus {
    /// Waiting for policy to say who fills ROLE.
    Asking { role: String, call: String, step: u32, head: Type },
    /// HEAD: the type of the answer (the owner's buttons, one per constructor).
    /// HOLD: a `hold`, owed but not delivered ('holds' rather than 'owes').
    Awaits {
        agent: String,
        call: String,
        step: u32,
        since: At,
        taken: bool,
        refusals: u32,
        forming: bool,
        head: Type,
        args: Value,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        hold: bool,
        /// The last reading of the agent's answer to this step read none of its outcomes (the agent was reminded):
        /// the reading call's why, empty when it gave none (paper-research N-101 p638).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        read_none: Option<String>,
        /// The step was passed up to AGENT after its holder's second answer was not read (paper-research N-101 p757):
        /// every answer to it not read, whole, with the reading's reasons.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        passed: Vec<crate::line::NotRead>,
    },
    Waits { children: Vec<String> },
    /// A part of a `par` in order that has not started (N-101): it waits on the parts ON (its siblings) to reach the
    /// state: their latest ticket of kind REACH closed Done, else their part closed Done.
    Holds {
        on: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reach: Option<String>,
    },
    /// At a hole (`...`): a place for posts that nothing runs; not open work. An Edit fills the hole.
    Resting,
    /// Stopped at RAISED, a failure nothing above it handles (stop.rs): TO is asked, in the hidden ticket BY, whether
    /// it runs again or ends.
    Stopped { raised: Raised, to: String, by: String },
    Done { result: Value },
    Failed { raised: Raised },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewGiven {
    pub to: String,
    pub delivery: String,
    pub upto: u32,
    pub at: At,
    #[serde(default, rename = "where", skip_serializing_if = "Option::is_none")]
    pub place: Option<Loc>,
    /// Made due again since, never read (`Line::Unread`): two such rows of one delivery to one agent are "handed twice ·
    /// not read" on the line under the post (consult-lead N-412.p1900, 2). Left out when false, so an older window
    /// reads the row as before.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unread: bool,
    /// When the text entered the agent's model input (`GivenRec::delivered`, 0.2 A1): B7's `delivered 12:28` comes from
    /// this alone, never from `at` (docket-edge4, N-818 post 72). Left out when none; an older coordd sends none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered: Option<At>,
    /// His posts up to UPTO its mark passed and it did not hand (`GivenRec::skipped`, (c2)): the window's line under
    /// his post reads the row by `hands`. Left out when none, so an older window reads the row as before.
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub skipped: std::collections::BTreeSet<u32>,
    /// The giving the agent owes an answer to (`Machine::owes_giving`): his post, handed again for being left
    /// unanswered in a turn, and left so again; not handed a third time, not read. `delivered · not answered` on the
    /// line under his post (paper-research N-412.p2197, 1). Left out when false, so an older window reads the row as
    /// before.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub owed: bool,
}

impl ViewGiven {
    /// Whether this row handed post N: `Ticket::hands`, the engine's own test, for the window.
    pub fn hands(&self, n: u32) -> bool {
        Ticket::hands_in(&self.delivery, self.upto, &self.skipped, n)
    }
}

impl TicketView {
    pub fn of(m: &Machine, x: &Ticket) -> TicketView {
        let halted = halted_under(m, x.t);
        let (ran, handlers) = runs_under(m, x);
        let asked = asked_of(m, x);
        let around = std::cell::OnceCell::new();
        let form = m.readable(&x.form);
        let title = crate::label::title(m, x);
        let status = match &x.status {
            Status::Asking { owed, role } => ViewStatus::Asking { role: role.clone(), call: owed.call.clone(), step: owed.step, head: owed.head.clone() },
            Status::Awaits { agent, owed, since, taken, refusals, forming, reminded, .. } => {
                // a reminder for a turn left without the answer is no reading (consult-3 N-1396 p300, 1)
                let read_none = match x.remind_why.as_ref().filter(|(s, _)| *s == owed.step).map(|(_, w)| w) {
                    Some(crate::RemindWhy::Left) => None,
                    why => reminded.is_some().then(|| match why {
                        Some(crate::RemindWhy::Read(w)) => w.clone(),
                        _ => String::new(),
                    }),
                };
                ViewStatus::Awaits { agent: agent.clone(), call: owed.call.clone(), step: owed.step, since: since.clone(), taken: *taken, refusals: *refusals, forming: forming.is_some(), head: owed.head.clone(), args: m.readable(&owed.args), hold: owed.hold, read_none, passed: x.passed(owed.step, agent) }
            }
            Status::Waits { .. } if x.held.is_some() => {
                let h = x.held.as_ref().unwrap();
                ViewStatus::Holds { on: h.on.iter().map(|c| m.id(*c)).collect(), reach: h.reach.clone() }
            }
            Status::Waits { children, .. } => ViewStatus::Waits { children: children.iter().map(|c| m.id(*c)).collect() },
            Status::Rests { .. } => ViewStatus::Resting,
            Status::Stopped { raised, to, by, .. } => ViewStatus::Stopped { raised: raised.clone(), to: to.clone(), by: m.id(*by) },
            Status::Done(v) => ViewStatus::Done { result: m.readable(v) },
            Status::Failed(r) => ViewStatus::Failed { raised: r.clone() },
        };
        let mut v = TicketView {
            id: m.id(x.t),
            n: x.t,
            kind: x.kind.clone(),
            parent: x.parent.map(|p| m.id(p)),
            under: x.under.map(|p| m.id(p)),
            // an event's ticket shows who raised the event (the owner's message: the owner)
            opener: x.opener.clone().or_else(|| x.raised_by.clone()),
            binding: x.binding.clone(),
            ctx: x.ctx.clone(),
            form,
            title,
            status,
            // a post's asks are open as of now (`Machine::asks_open`), not as marked
            // what closes their asks read once for the whole thread (`Machine::asks_shown`), not for each post
            posts: m.shown(
                x.posts
                    .iter()
                    .filter(|p| !p.taken_back && p.by != "coordd")
                    .map(|p| {
                        let (asks_open, asks_closed, would_close) = m.asks_shown(x, p, &around);
                        Post { asks_open, asks_closed, would_close, ..p.clone() }
                    })
                    .collect(),
            ),
            struck: x
                .struck
                .iter()
                .filter_map(|s| x.posts.iter().find(|p| p.n == s.n).map(|p| ViewStruck { post: m.shown(vec![p.clone()]).remove(0), at: s.at.clone(), to: s.to.clone(), told: s.told.clone() }))
                .collect(),
            children: x.children.iter().map(|c| m.id(*c)).collect(),
            program: x.program.clone(),
            bound: x.bound,
            spent: x.steps,
            given: {
                // which giving is the last of its delivery to its agent, read in one pass from the end: a scan of the
                // later givings for each one held the lock on a long thread (paper-research N-1396 p424)
                let mut later: std::collections::HashSet<(&str, &str)> = std::collections::HashSet::new();
                let mut last: Vec<bool> = x.given.iter().rev().map(|g| later.insert((g.to.as_str(), g.delivery.as_str()))).collect();
                last.reverse();
                x.given
                    .iter()
                    .zip(last)
                    .map(|(g, last)| {
                        // the last giving of a delivery handed twice and read neither time is not read either
                        let twice = last && x.unread_twice.contains(&(g.to.clone(), g.delivery.clone()));
                        ViewGiven { to: g.to.clone(), delivery: g.delivery.clone(), upto: g.upto, at: g.at.clone(), place: g.place.clone(), unread: g.unread || twice, delivered: g.delivered.clone(), owed: last && m.owes_giving(&g.to, &g.delivery, x.t, g.upto), skipped: g.skipped.clone() }
                    })
                    .collect()
            },
            opened_at: x.opened_at.clone(),
            closed_at: x.closed_at.clone(),
            broken: x.broken.clone(),
            reloaded: x.reloaded.clone(),
            waits_on: m
                .leaves(x.t)
                .into_iter()
                .filter(|(t, _, _)| !halted.contains(t))
                .map(|(t, agent, call)| WaitsOn { ticket: m.id(t), agent, call, post: None, layer: m.layer_of(t) })
                .chain(m.tickets.values().filter(|y| y.under == Some(x.t) && y.handling.is_some() && y.is_open()).flat_map(|y| m.leaves(y.t)).filter(|(t, _, _)| !halted.contains(t)).map(|(_, agent, call)| WaitsOn { ticket: m.id(x.t), agent, call, post: None, layer: None }))
                // a ticket with halts waiting (slow mode) waits on the owner, whatever its step's worker (N-88)
                .chain(halted.iter().map(|u| WaitsOn { ticket: m.id(*u), agent: "owner".into(), call: "halt".into(), post: None, layer: None }))
                // an agent's asks the minder left with the owner: a line to show, not a step he owes (m1392, e)
                .chain(m.asks_waiting(x).into_iter().map(|(_, n)| WaitsOn { ticket: m.id(x.t), agent: "owner".into(), call: "asks".into(), post: Some(n), layer: None }))
                // an agents' thread: each agent told a post that has not ended a turn handed it (design part 9b)
                .chain(m.waits_between(x.t).into_iter().map(|(agent, n)| WaitsOn { ticket: m.id(x.t), agent, call: "post".into(), post: Some(n), layer: None }))
                .collect(),
            // at a halted open, what comes after is the child's whole program
            layer: m.layer_of(x.t),
            continuation: crate::print::continuation(x).or_else(|| x.halts.iter().any(|h| matches!(h.what, crate::machine::Halting::Open { .. })).then(|| x.program.clone())),
            started: x.started.clone(),
            quiet: x.quiet.iter().map(|(agent, upto)| ViewQuiet { agent: agent.clone(), upto: *upto }).collect(),
            whereabouts: whereabouts(m, x),
            // filled where one ticket is shown (coordd's `ticket_shown`): its children are a scan of every ticket
            outline: None,
            drawn: drawn(m, x),
            ended: x.ended.clone(),
            // filled where one ticket is shown (coordd's `ticket_shown`), from its index of every post's words
            mentioned: Vec::new(),
            mentions: Default::default(),
            templates: m.templates_seen(x),
            lapses_at: None,
            lapsed: x
                .lapsed
                .iter()
                .map(|(step, after, because)| ViewLapsed { step: *step, post: x.posts.iter().rev().find(|p| p.by == "owner" && p.step == Some(*step)).map(|p| p.n), after: *after, because: because.clone() })
                .collect(),
            shaped: x.shaped.iter().map(|s| ViewShaped { n: s.n, at: s.at.clone(), answer: m.readable(&s.answer), undone: s.undone, refused: s.refused.clone() }).collect(),
            hole: match &x.status {
                Status::Rests { scope, .. } => Some(scope.clone()),
                _ => x.shaping.as_ref().map(|(_, s)| s.clone()),
            },
            owner: (matches!(x.status, Status::Rests { .. }) || x.shaping.is_some()).then(|| ViewOwner::of(&x.posts)),
            unanswered_owner: {
                let mut open: Vec<u32> = asked.iter().filter(|a| a.answer.is_none()).map(|a| a.post).collect();
                open.dedup();
                // answered for all once one it asks or was handed answered it, as the foot reads it (paper-research N-1446 p2355)
                open.retain(|n| x.posts.iter().find(|p| p.n == *n).is_none_or(|p| !m.his_answered_in(x, p)));
                open
            },
            asked,
            duties: m.duties_seen.get(&x.t).cloned(),
            handlers,
            ran,
            starred: x.starred,
            stars: x.stars.clone(),
            archived: x.archived,
            archived_under: m.archived_by(x.t).filter(|u| *u != x.t).map(|u| m.id(u)),
            run: run_of(m, x),
            slow: m.slow(x.t),
            slow_by: m.slow_by(x.t).map(|u| m.id(u)),
            slow_scope: m.slow_scope(x.t).map(String::from),
            halts: halts_of(m, x),
            halted: halted_below(m, x.t),
            unsent: Vec::new(),
            owed: false,
        };
        // the same as `Machine::owes_owner`, read off the waits just made rather than made again under the lock
        v.owed = v.waits_on.iter().any(|w| w.agent == "owner");
        v
    }
}
