//! The message runtime (0.2, `runtime-design.md`; the owner, 20 Nov: "call a decision llm dont hardcode the rules"):
//! the engine keeps facts and does acts, and a Docket program, `delivery.kind`, decides what reaches an agent, when,
//! and batched how.
//!
//! - Each agent has one delivery ticket, hidden, opened on the text of `delivery.kind` that runs: its
//!   `delivery_opened` line carries the text, so a replay runs what ran. When the text changes, the agent's next run
//!   opens its next ticket on the new text and the old one closes. The ticket and its runs are numbered from DESK,
//!   apart from the other hidden tickets.
//! - The delivery events (`arrived`, `free`, `stopped`) come as `event` lines and run one at a time, in the order they
//!   came, each in a hidden ticket under the agent's delivery ticket. An event that comes while a run is open waits
//!   behind it, whole: none is merged into another or lost. The one exception: an event that brings an owner's post
//!   the open run was not offered ends that run (its step withdrawn, so a late answer is refused) and runs first, so
//!   his post never waits behind a decision call.
//! - What the handler sees of what waits is read when its run begins, not when its event came: a run never offers
//!   what an earlier run handed.
//! - The acts are facts. `deliver` hands items as one message (`Effect::Hand`); they are in flight until coordd's
//!   `given` lines cover them. `keep` holds an item waiting, below the agent's read mark too, until a `deliver`
//!   carries it, and an agent's post does not answer what was kept from it. `report` says an item cannot be delivered
//!   now. None of them is a post. All three are rebuilt by replay, from the runs the record's inputs drive.
//! - Everything the runtime records is an `event` line (syntax::DECIDED): the delivery events and the answers to a
//!   run's step as inputs, `delivery_opened` and `delivery_acted` as outputs. No other line of a delivery ticket or its
//!   runs is written (`delivery_line`) but coordd's `start_failed` for a copy it could not start, an input no run
//!   makes: replay makes the runs again from those inputs. So an engine without the
//!   runtime (train 26) replays such a record as recorded, line by line, and refuses none.

use super::*;
use crate::line::PostKind;
use std::collections::VecDeque;

/// The kind of an agent's delivery ticket, and the program it runs (`agent-base/kinds/delivery.kind`).
pub const DELIVERY: &str = "delivery";

/// One agent's delivery, as the machine keeps it (rebuilt by replay).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Desk {
    /// Its delivery ticket now.
    pub ticket: Option<u64>,
    /// The events waiting for the run before them, in the order they came: name and value as their lines have them.
    pub queue: VecDeque<(String, Value)>,
    /// The run open now: a hidden ticket under a delivery ticket of the agent's.
    pub busy: Option<u64>,
    /// The items kept (`keep`), each with why: waiting until a `deliver` carries them.
    pub kept: BTreeMap<String, String>,
    /// The items reported (`report`), each with why: still waiting, not deliverable when reported.
    pub reported: BTreeMap<String, String>,
    /// What `deliver` handed and coordd has not said is in yet: each message's items, and the rest it named.
    pub handing: Vec<Hand>,
    /// Its last acts, the newest last, ACTS_KEPT of them (`last_acts`): what the Docket's act chips show. Nothing
    /// more is written for them; replay makes them again with the runs.
    pub acts: VecDeque<crate::view::ViewActed>,
    /// The copies of the agent its delivery made (`fork A at PLACE`, fresh-lead-3 N-1104 p1154) and has not stopped,
    /// the newest last: each copy's name and the delivery its session was cut at. Rebuilt by replay with the runs, as
    /// the acts are; empty in every desk written before it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub copies: Vec<(String, String)>,
    /// The delivery its turn now running began with, as the last `turn_began` line for it said (TURN_BEGAN): where a
    /// copy of it is cut. None before its first such line, or when that line named none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub began: Option<String>,
}

/// The call a copy is started for (`Effect::Copy`): its prompt, agent-base's `prompts/copy.md` (coordd's
/// `fresh_prompt`). No call file declares it; a copy is handed posts, never a step.
pub const COPY_CALL: &str = "copy";

/// How many of an agent's acts its desk keeps (`Desk::acts`).
pub const ACTS_KEPT: usize = 8;

/// One message `deliver` made: ITEMS not yet covered by a `given` line, and REST, named at its foot. DIGEST: what
/// changed on its structures, given with it by first lines (I-1480, `deliver … digest PULL`); while the message is
/// being handed it waits no more, and once ITEMS are in, what of it no `given` line covered waits again.
/// FORK: the message is a `fork … at PLACE` (part 4, docket-core N-1446 p2096): coordd restarts the agent under its
/// own name on a copy of its session cut after PLACE's `given` line, then hands the message there. None for a
/// `deliver`, and in every desk written before it.
/// IDLE: made in a run of `free`, which coordd raises while the agent is idle. Fold's call (agent-base delivery.kind)
/// may outlast that idle: a turn begun meanwhile (its own job's end begins one) gets the message at its end, never in
/// it, as `arrived` keeps what comes while it works (coordd delivery.rs `hand_now`; gemini-advisor N-1446 p2365, 5b:
/// N-1446.p2262 typed into docket-edge6's running turn; proto-mind p2370 (a)). False for one that carries a step,
/// which goes into a turn begun as it was handed, as before (consult-3 N-1396 p300, 2: a turn of its own answers it
/// when that one leaves it); for an `arrived`'s, which may go into the turn; and in every desk written before it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Hand {
    pub items: Vec<String>,
    pub rest: Vec<String>,
    pub digest: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub idle: bool,
}

/// The ids an act names: a list of items or ids, or one of either. An item is a waiting record (its `id`), an id a
/// string.
fn ids_of(v: &Value) -> Vec<String> {
    let one = |v: &Value| v.as_str().or_else(|| v.get("id").and_then(Value::as_str)).map(str::to_string);
    let mut out: Vec<String> = Vec::new();
    let all: Vec<String> = match v {
        Value::Array(xs) => xs.iter().filter_map(one).collect(),
        v => one(v).into_iter().collect(),
    };
    for i in all {
        if !out.contains(&i) {
            out.push(i);
        }
    }
    out
}

/// The record line of an act (syntax::ACTED): AGENT's delivery did ACT with ITEMS, REST named, WHY. Only the items
/// waiting when it ran; none is a post. The act is kept on AGENT's desk too (`Desk::acts`), and coordd told
/// (`Effect::Acted`), so the agent's view carries it.
fn acted(desks: &mut Counted<BTreeMap<String, Desk>>, cx: &mut Cx, agent: &str, act: &str, items: &[String], rest: &[String], why: Option<&str>) {
    acted_with(desks, cx, agent, act, items, rest, &[], None, why)
}

/// [`acted`], a `deliver`'s DIGEST on its line too when it has one (I-1480), and a `fork`'s place, AT (part 4): a line
/// without them is as before.
#[allow(clippy::too_many_arguments)]
fn acted_with(desks: &mut Counted<BTreeMap<String, Desk>>, cx: &mut Cx, agent: &str, act: &str, items: &[String], rest: &[String], digest: &[String], at: Option<&str>, why: Option<&str>) {
    let mut value = json!({"agent": agent, "act": act, "items": items});
    if !rest.is_empty() {
        value["rest"] = json!(rest);
    }
    if !digest.is_empty() {
        value["digest"] = json!(digest);
    }
    if let Some(at) = at {
        value["at"] = json!(at);
    }
    if let Some(why) = why {
        value["why"] = json!(why);
    }
    cx.line(Line::Event { at: cx.now.clone(), name: syntax::ACTED.into(), value, fired: Vec::new(), counted: Vec::new(), on: None });
    let acts = &mut desks.edit().entry(agent.to_string()).or_default().acts;
    acts.push_back(crate::view::ViewActed::of(act, &cx.now, items, rest, why));
    while acts.len() > ACTS_KEPT {
        acts.pop_front();
    }
    cx.effect(Effect::Acted { agent: agent.to_string() });
}

/// Post N of ticket T, when ID names one (`N-412.p7`): N all digits, so `p+5`, `p 5` or `p` name none (`parse` alone
/// takes a sign; consult-3 N-1397.p74). The one parse of a post's id, with [`post_ref`] under it: every reader of one,
/// or check for one, uses them.
pub fn post_id(id: &str) -> Option<(u64, u32)> {
    let (t, n) = post_ref(id)?;
    Some((parse_ticket(t)?, n))
}

/// The post a delivery of posts ID ends at: [`post_id`] of `N-412.p7`, or of his post told late on its own,
/// `N-412.p7.told` (`TOLD`; proto-lead N-1446 p569, (c2)). Every reader that takes a delivery's id for its post (the
/// rule Reply, coordd's rendering and handing) reads it so.
pub fn delivered_post(id: &str) -> Option<(u64, u32)> {
    told_post(id).or_else(|| post_id(id))
}

/// His post a delivery of its own told late (`N-412.p7.told` → N-412, 7): None for any other id.
pub fn told_post(id: &str) -> Option<(u64, u32)> {
    post_id(id.strip_suffix(super::TOLD)?.strip_suffix('.')?)
}

/// The post delivery ID is of: ID itself, or the post a take-back's notice (`N-412.p7.taken_back` → `N-412.p7`) says
/// was struck. The one parse of a notice's id (proto-lead N-1397 p146): `Machine::informs_only` reads it so, and
/// `Machine::reply_threads` reads [`delivered_post`] of it. `delivered_post` alone reads a notice as no post, as
/// coordd's rendering and handing must.
pub fn post_of(id: &str) -> &str {
    id.strip_suffix(super::TAKEN_BACK).and_then(|p| p.strip_suffix('.')).unwrap_or(id)
}

/// [`post_id`] with its ticket as ID writes it (`N-412.p7` → `N-412`, 7), for a reader that checks the ticket as
/// written or says it back (coordd's `ticket_of`, the client's requests; consult-3 N-1396 p280).
pub fn post_ref(id: &str) -> Option<(&str, u32)> {
    let (t, rest) = id.split_once('.')?;
    let n = rest.strip_prefix('p').filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))?;
    Some((t, n.parse().ok()?))
}

/// The place ID names, its ticket as written: a post (`N-412.p7` → `N-412`, Some(7), by [`post_ref`]) or a ticket
/// alone (`N-412` → `N-412`, None); None for anything else with a `.` in it (`N-412.p+5`, `N-412.s3`), which names no
/// post. What a star, a quote reply and `Machine::place_of` read (consult-3 N-1396 p310).
pub fn post_or_ticket(id: &str) -> Option<(&str, Option<u32>)> {
    match post_ref(id) {
        Some((t, n)) => Some((t, Some(n))),
        None => (!id.contains('.')).then_some((id, None)),
    }
}

/// Step N of ticket T, when delivery ID gives it (`N-412.s3`), and whether ID is the step itself: `.s3` is; its
/// reminder (`.again`), its give-back after a move (`.back`) and a refusal of an answer to it (`.refusedK`) are not. A
/// withdrawal's notice (`.s3.withdrawn`) gives none. N all digits, as `post_id`'s: the one parse of a step's delivery
/// (consult-3 N-1396 p280).
pub fn step_id(id: &str) -> Option<(u64, u32, bool)> {
    let digits = |n: &str| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit());
    let (t, rest) = parse_delivery(id)?;
    let rest = rest.strip_prefix('s')?;
    let (n, after) = rest.split_once('.').map_or((rest, None), |(n, a)| (n, Some(a)));
    let gives = match after {
        None | Some("again" | "back") => true,
        Some(a) => a.strip_prefix("refused").is_some_and(digits),
    };
    (gives && digits(n)).then_some(())?;
    Some((t, n.parse().ok()?, after.is_none()))
}

/// The ids ITEMS names that are among WAITING, in ITEMS' order.
fn still_in(waiting: &[String], items: &Value) -> Vec<String> {
    ids_of(items).into_iter().filter(|i| waiting.contains(i)).collect()
}

/// Whether waiting item I is a post of the owner's that wakes the agent: not one that only informs it, which goes with
/// the rest. One he sent to wait for the turn's end (`waits`) is his all the same once the agent is idle; `arrived`
/// reads `waits` itself (delivery.kind).
fn his_now(i: &Value) -> bool {
    i.get("owner") == Some(&json!(true)) && i.get("inform") != Some(&json!(true))
}

/// Whether AGENT opened X or works on it (`coord item start`): X is its own, first in its digest (I-1480, N-1481 p25;
/// coordd's foot draws it so, delivery.rs `digest_named`).
pub fn own_of(x: &Ticket, agent: &str) -> bool {
    x.opener.as_deref() == Some(agent) || x.started.iter().any(|a| a == agent)
}

/// The posts by BY on X that post P answers first (I-1480): each told to P's writer (it wakes it), up to what P's
/// writer had been handed when it wrote (`Ticket::answers`), and answered by none of its posts before P. Whom a post
/// naming nobody names, by the rule Reply or by hand (`Machine::unanswered`), are these posts' authors. By CLOSE (the
/// era [`ERA_CLOSES`]) a post answers M only by the ledger's rule (`closes`): one that only passes M on to another
/// answers it neither as P nor before P (paper-research N-1479 p107); by WHOM (the era [`ERA_QUIET`]) a result only as
/// any post does.
fn answered_first<'a>(x: &'a Ticket, p: &Post, by: &str, close: bool, whom: bool) -> Vec<&'a Post> {
    let answers = |q: &Post, m: &Post| if close { closes(x, m, q, whom) } else { x.answers(q, m.n) };
    // its writer's posts before it, read once: only one after M can answer M
    let before: Vec<&Post> = x.posts.iter().filter(|q| q.n < p.n && q.by == p.by && !q.dropped).collect();
    let answered_before = |m: &Post| before.iter().any(|q| q.n > m.n && answers(q, m));
    x.posts.iter().filter(|m| m.n < p.n && m.by == by && m.visible() && answers(p, m) && m.wakes().any(|a| *a == p.by) && !answered_before(m)).collect()
}

/// Whether post M asks agent Y to act (I-1480): typed an ask, read or blocker to Y; or untyped and naming Y (p4's
/// (a)), but for a name the rule gave it and its writer did not type (`Post::answering`: it answers Y's own post; an
/// `@` the writer typed always asks, paper-research N-1481 p40).
pub(super) fn asks(m: &Post, y: &str) -> bool {
    match &m.kind {
        Some(k) => k.to().iter().any(|a| a == y),
        None => m.names.iter().any(|a| a == y) && !m.answering.iter().any(|a| a == y),
    }
}

/// Whether P, a `result` by Y on X, is the first of Y's posts on X since a post of AGENT's there that asks Y
/// (`asks`) that answers that ask or is a result (`edge_closes`): the waits-on edge, scoped to the ask's own ticket
/// and closed by its first such post (paper-research N-1481 p20, proto-reader p18). By CLOSE (the era
/// [`ERA_CLOSES`]) it closes by the ledger's rule (`closes`, paper-research N-1479 p107): it stays open past a post of
/// Y's that only passes the ask on, a result Y wrote before it was handed the ask (consult-3 p127), and one halted or
/// moved away (proto-lead p144); and P goes only when it answers the ask itself, so AGENT is woken once, by the post
/// that answers its ask (p130). By WHOM (the era [`ERA_QUIET`]) it stays open past a result Y wrote to another, and P
/// goes only when it names AGENT or answers its ask.
fn waits_on(x: &Ticket, p: &Post, agent: &str, close: bool, whom: bool) -> bool {
    let closed = |m: &Post, q: &Post| if close { closes(x, m, q, whom) } else { edge_closes(x, m, q) };
    x.posts.iter().filter(|m| m.n < p.n && m.by == agent && m.visible() && asks(m, &p.by)).any(|m| !x.posts.iter().any(|q| q.n < p.n && q.by == p.by && closed(m, q)) && (!close || closes(x, m, p, whom)))
}

/// The waits-on edge's close (`waits_on`): Q comes after M, is not dropped, and answers M (written with M handed to
/// its writer and not kept from it, `Ticket::answers`) or is a `result`.
fn edge_closes(x: &Ticket, m: &Post, q: &Post) -> bool {
    q.n > m.n && !q.dropped && (x.answers(q, m.n) || matches!(q.kind, Some(PostKind::Result)))
}

/// Whether Q, a post on X, answers post M there for M's writer W, for the ledger alone (`Machine::ledger`, `asked`,
/// `Machine::his_answered_in`, and through them `TicketView::unanswered_owner` and the foot's `owed_posts`; I-1455 (B), paper-research N-1479 p94, p98, p105 on consult-3's
/// p84): the waits-on edge's close (`edge_closes`), with Q written with M handed to its writer (`Ticket::answers`) in
/// each way, a `result` too (p94's point 2; consult-3's p127, proto-reader's p128: a post put into the writer's running
/// turn counts as handed); and Q names W (typed, or filled in by the reply rule, `Post::answering`), or is a `result`,
/// or names M among the reply tool's ids (`answers`, or its further ids on X, `Ticket::also_here`). Only a post shown
/// where it was written answers, as the scan before took them (`visible() && moved.is_none()`; consult-3's p136 point
/// 3, proto-lead p142): not one taken back, held, halted for his approval (until he approves it) or moved to another
/// thread. A post that only passes M on to another, typing only that one's name, does not answer it. Delivery's `push`
/// closes by it from the era [`ERA_CLOSES`] (`waits_on`, `answered_first`; paper-research p107), and by the edge's own
/// close before it, so that recorded `free` lines hand on replay what they handed (p105).
/// WHOM, the era [`ERA_QUIET`]: a `result` answers M only as any post does, by naming W, answering M or naming it
/// among its further ids, but for one that names nobody and answers nothing, which answers as before (gemini-advisor
/// N-1446 p2365, 4: docket-core's p2293, a result to paper-research answering p2284, answered docket-edge5's ask to
/// docket-core and woke it; proto-mind p2444).
pub(super) fn closes(x: &Ticket, m: &Post, q: &Post, whom: bool) -> bool {
    let result = matches!(q.kind, Some(PostKind::Result)) && (!whom || (q.names.is_empty() && q.answers.is_none() && x.also_here(q).is_empty()));
    let to_it = || q.names.iter().any(|a| *a == m.by) || result || q.answers == Some(m.n) || x.also_here(q).contains(&m.n);
    edge_closes(x, m, q) && x.answers(q, m.n) && q.visible() && q.moved.is_none() && to_it()
}

/// How Q, which closes M by the rule before the era [`ERA_CLOSES`] (`edge_closes`), does not by the ledger's
/// (`closes`): "not shown" (held, halted or taken back), "moved" (to another thread), "unhanded" (a result written
/// before its writer was handed M), "result to another" (from the era [`ERA_QUIET`]: it names another or answers
/// another post), else "hand-off" (it names neither M's writer nor M: it passes M on).
fn not_closing(x: &Ticket, m: &Post, q: &Post) -> &'static str {
    if !q.visible() {
        "not shown"
    } else if q.moved.is_some() {
        "moved"
    } else if !x.answers(q, m.n) {
        "unhanded"
    } else if matches!(q.kind, Some(PostKind::Result)) {
        "result to another"
    } else {
        "hand-off"
    }
}

/// Where the two closes part for P on X and AGENT's asks of P's writer there: the last post of the writer's up to P
/// that closes such an ask by the rule before and not by the ledger's (`not_closing`), with how. P itself, when it is
/// one; else the one before it that closed the edge before. WHOM: the ledger's as from the era [`ERA_QUIET`].
fn parting(x: &Ticket, p: &Post, agent: &str, whom: bool) -> Option<(u32, &'static str)> {
    x.posts.iter()
        .filter(|m| m.n < p.n && m.by == agent && m.visible() && asks(m, &p.by))
        .flat_map(|m| x.posts.iter().filter(move |q| q.n > m.n && q.n <= p.n && q.by == p.by && edge_closes(x, m, q) && !closes(x, m, q, whom)).map(move |q| (q.n, not_closing(x, m, q))))
        .max_by_key(|(n, _)| *n)
}

/// An agent post N of T goes to otherwise by the ledger's close than by the rule before the era [`ERA_CLOSES`]
/// (`Machine::close_changes`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloseChange {
    pub agent: String,
    /// Why it goes to AGENT now by the rule before and by the ledger's (`push_by`); None: pulled.
    pub before: Option<&'static str>,
    pub by: Option<&'static str>,
    /// The post of its writer's where the two rules part (`parting`): N itself, or, for a post that goes now and was
    /// pulled before, the one before it that closed AGENT's edge then.
    pub at: u32,
    /// How that post closes by the one rule and not the other (`not_closing`); "same" where both take the same post
    /// as the answer and only the why differs (from the era a result that answers goes as "reply", not "result").
    pub how: &'static str,
}

impl Machine {
    /// The line of a delivery event NAME with VALUE: it names its agent (and, but for `free`, its item by id); a
    /// delivery program must be loaded. Nothing else is read until it runs.
    pub(super) fn complete_delivery(&self, at: At, name: String, value: Value) -> Result<Line, String> {
        let Some(agent) = value.get("agent").and_then(Value::as_str).filter(|a| !a.is_empty()) else {
            return Err(format!("The event {name} names no agent."));
        };
        if !self.defs.kinds.contains_key(DELIVERY) {
            return Err(format!("No delivery program is loaded (`kind {DELIVERY}`): {name} for {agent} is not taken."));
        }
        if name != syntax::FREE && value.get("item").and_then(Value::as_str).is_none() {
            return Err(format!("The event {name} names no item."));
        }
        Ok(Line::Event { at, name, value, fired: Vec::new(), counted: Vec::new(), on: None })
    }

    /// The line of BY's answer HEAD, TEXT to the step of delivery run C (`Input::Answer`): a DECIDED event, and only
    /// for an answer that fits the step owed. One that does not is refused here and nothing is counted: coordd, which
    /// answers the decision call, answers Give(everything) for one that failed, so the record holds only answers taken.
    pub(super) fn complete_decided(&self, at: At, c: u64, by: String, head: Value, text: Text) -> Result<Line, String> {
        let agent = self.run_agent(c)?;
        let Status::Awaits { agent: owes, owed, .. } = &self.tickets[&c].status else {
            return Err(format!("{agent}'s delivery run {} awaits no answer now.", self.id(c)));
        };
        if *owes != by {
            return Err(format!("{agent}'s delivery run {} awaits {owes}, not {by}.", self.id(c)));
        }
        let fits = owed.head.coerce(&normal_head(&head)).map(|h| fill_text(&owed.head, h, &text)).and_then(|h| owed.head.conforms(&h));
        if let Err(why) = fits {
            return Err(format!("That answer does not fit step {} of {}'s delivery run {}: {why}. Nothing was counted.", owed.step, agent, self.id(c)));
        }
        let value = json!({"agent": agent, "run": c, "by": by, "head": head, "text": text});
        Ok(Line::Event { at, name: syntax::DECIDED.into(), value, fired: Vec::new(), counted: Vec::new(), on: None })
    }

    /// A DECIDED line: the answer it carries, taken by the run's step as any answer is (`Line::Answer`).
    pub(super) fn exec_decided(&mut self, cx: &mut Cx, at: &At, value: &Value) -> (bool, Result<String, String>) {
        let c = value.get("run").and_then(Value::as_u64).unwrap_or_default();
        if let Err(e) = self.run_agent(c) {
            return (false, Err(e));
        }
        let by = value.get("by").and_then(Value::as_str).unwrap_or_default().to_string();
        let head = value.get("head").cloned().unwrap_or(Value::Null);
        let text: Text = match value.get("text").cloned().map(serde_json::from_value) {
            Some(Ok(t)) => t,
            _ => return (false, Err(String::from("The decision's line carries no text."))),
        };
        self.exec_line(&Line::Answer { at: at.clone(), t: c, by, head, text }, cx)
    }

    /// Whether L is a line of a delivery ticket or of a run under one: not written (the record keeps the runtime's
    /// `event` lines only), as replay makes the runs again. coordd's `start_failed` for a copy a run made (`act_fork`)
    /// names the copy's delivery ticket, and no run makes it: it is written, so that a replay drops the copy as the
    /// machine did (`start_failed`), and a later post goes to a new copy there too. Before, the line was dropped here,
    /// and a replay kept a copy that never ran (proto-mind's copy proof on engine 81, p3335; paper-research N-1446
    /// p3336). Every engine since N-101 reads the line by its agent alone, so an engine before reads it the same.
    pub(super) fn delivery_line(&self, l: &Line) -> bool {
        if matches!(l, Line::Event { .. } | Line::StartFailed { .. }) {
            return false;
        }
        line_tickets(l).into_iter().any(|t| t >= DESK && (self.delivery_agent(t).is_some() || self.tickets.get(&t).and_then(|x| x.under).is_some_and(|d| self.delivery_agent(d).is_some())))
    }

    /// Delivery event NAME with VALUE: run now in the agent's delivery ticket, or queued behind the run open there;
    /// one that brings the owner a post the open run was not offered ends that run and runs first.
    pub(super) fn delivery_event(&mut self, cx: &mut Cx, name: &str, value: &Value) -> (bool, Result<String, String>) {
        let agent = value.get("agent").and_then(Value::as_str).unwrap_or_default().to_string();
        let busy = self.desks.get(&agent).and_then(|d| d.busy).filter(|c| self.tickets.get(c).is_some_and(Ticket::is_open));
        if let Some(c) = busy {
            if self.brings_his(&agent, name, value, c) {
                self.desks.edit().get_mut(&agent).unwrap().queue.push_front((name.to_string(), value.clone()));
                let why = "the owner's post came; it goes first";
                let r = Raised { exc: "Cancelled".into(), value: json!({"by": "coord", "why": why}) };
                let mut work = Vec::new();
                self.cancel(cx, c, "coord", why, &r, &mut work);
                self.drive(cx, work);
                return (true, Ok(format!("{agent}'s open delivery run {} ended: the owner's post goes first.", self.id(c))));
            }
            let d = self.desks.edit().get_mut(&agent).unwrap();
            d.queue.push_back((name.to_string(), value.clone()));
            let n = d.queue.len();
            return (true, Ok(format!("Waits behind {agent}'s delivery run {} ({n} waiting).", self.id(c))));
        }
        match self.delivery_run(cx, &agent, name, value) {
            Some(w) => {
                self.drive(cx, vec![w]);
                (true, Ok(format!("Run for {agent}.")))
            }
            None => (true, Ok(format!("Nothing to decide for {agent}."))),
        }
    }

    /// Whether event NAME with VALUE for AGENT brings an owner's post that run C, open, was not offered. A post of his
    /// the owner's hold keeps (one from before it, line 327) brings nothing: it goes at no run while the hold is on.
    fn brings_his(&self, agent: &str, name: &str, value: &Value, c: u64) -> bool {
        let Some(x) = self.tickets.get(&c) else { return false };
        let offered: BTreeSet<String> = x.form.get("waiting").map(ids_of).unwrap_or_default().into_iter().chain(x.form.get("item").map(ids_of).unwrap_or_default()).collect();
        let his = |i: &Value| his_now(i) && i.get("id").and_then(Value::as_str).is_some_and(|id| !offered.contains(id));
        let waiting: Vec<Value> = self.waiting_held(agent, self.due(agent)).into_iter().filter(|(_, held)| !held).map(|(i, _)| i).collect();
        match name {
            syntax::ARRIVED => value.get("item").and_then(Value::as_str).and_then(|id| waiting.iter().find(|i| i.get("id").and_then(Value::as_str) == Some(id))).is_some_and(his),
            syntax::FREE => waiting.iter().any(his),
            _ => false,
        }
    }

    /// Run event NAME with VALUE for AGENT in its delivery ticket: the work that runs its handler, its run marked open;
    /// None when there is nothing to decide (its item waits no more, nothing waits, its ticket has no handler for it
    /// or its `where` does not hold).
    fn delivery_run(&mut self, cx: &mut Cx, agent: &str, name: &str, value: &Value) -> Option<Work> {
        let p = self.delivery_value(agent, name, value)?;
        let d = self.delivery_ticket(cx, agent)?;
        let x = &self.tickets[&d];
        let h = x.handlers.iter().find(|h| h.ev == name)?.clone();
        let mut env: Env = x.form.as_object().map(|o| o.iter().map(|(a, b)| (a.clone(), b.clone())).collect()).unwrap_or_default();
        env.insert("this".into(), json!(self.id(d)));
        env.insert(h.x.clone(), p.clone());
        // run on replay as live: a record with delivery events comes from an engine that ran their handlers, and the
        // handler is the recorded program's (`delivery_ticket`)
        if h.cond.as_ref().is_some_and(|c| eval(c, &env, &x.ctx) != json!(true)) {
            return None;
        }
        let (c, body) = self.handler_ticket(cx, d, name, &h, p, agent, None)?;
        self.desks.edit().entry(agent.to_string()).or_default().busy = Some(c);
        cx.effect(Effect::Changed { t: d });
        Some(Work::Run(c, body, env))
    }

    /// What the handler of event NAME for AGENT is given, read now: `free` all that waits (None when nothing does);
    /// `arrived` and `stopped` the item their line names (None when it waits no more) beside it. Each post's `lead` is
    /// whether its author is above AGENT in the team's tree, as the event's `above` names them (coordd's tree, the one
    /// the policy's `above` facts are drawn from; consult-3 N-1397.p13, 1), and holding a role named so is nothing to
    /// it. No arm of agent-base's delivery.kind reads it since I-1480's c, where a lead's post waits for the turn's end
    /// as any other does (paper-research N-1481 p15). It is given still for the delivery tickets open on the text
    /// before, whose `arrived` put a lead's post into its worker's turn, until each agent's next run opens one on this.
    ///
    /// What the owner's hold on agents' mail keeps (line 327, `mail_held`) is never among what may be handed:
    /// `waiting` and `owner` are what waits but for it, so neither `next` nor a foot's `listing` sees it. `free` gives
    /// it apart, as `held`, with HOLD the words to keep it with (`MailHold::why`; none when nothing is held); `arrived`
    /// gives HOLD when its item is held. `free` runs while anything waits, held or not.
    fn delivery_value(&self, agent: &str, name: &str, value: &Value) -> Option<Value> {
        let above: Vec<&str> = value.get("above").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
        let mut all = self.waiting_held(agent, self.due(agent));
        for (i, _) in &mut all {
            let lead = i.get("what").and_then(Value::as_str) == Some("post") && i.get("from").and_then(Value::as_str).is_some_and(|f| above.contains(&f));
            i["lead"] = json!(lead);
        }
        let (held, waiting): (Vec<(Value, bool)>, Vec<(Value, bool)>) = all.into_iter().partition(|(_, h)| *h);
        let (held, waiting): (Vec<Value>, Vec<Value>) = (held.into_iter().map(|(i, _)| i).collect(), waiting.into_iter().map(|(i, _)| i).collect());
        let why = self.mail_hold.as_ref().map(MailHold::why);
        let text = |f: &str| value.get(f).and_then(Value::as_str).unwrap_or_default().to_string();
        if name == syntax::FREE {
            let owner: Vec<&Value> = waiting.iter().filter(|i| his_now(i)).collect();
            // the oldest step its own act raised, which goes at once (one: a message holds at most one step)
            let raised: Vec<&Value> = waiting.iter().filter(|i| i.get("raised") == Some(&json!(true))).take(1).collect();
            let writing = value.get("writing") == Some(&json!(true));
            let hold = why.filter(|_| !held.is_empty());
            // what goes now and what is pulled (I-1480), each in WAITING's order
            let (push, pull): (Vec<&Value>, Vec<&Value>) = waiting.iter().partition(|i| i.get("push") == Some(&json!(true)));
            // what fold reads (syntax.rs FREE): as coordd said it, recorded with the event, so a replay reads the same
            let worked = value.get("worked") == Some(&json!(true));
            let forkable = value.get("forkable") == Some(&json!(true));
            // the brain's lines (syntax.rs FREE's `told`): pulled, so `fold` never orders them; each goes whole
            let told: Vec<&Value> = waiting.iter().filter(|i| i.get("into_turn") == Some(&json!(true)) && i.get("push") != Some(&json!(true))).collect();
            // its copy (syntax.rs FREE's `copy`, N-1104 p1154): its posts, whole with his; whether it still answers, as
            // coordd read it with the event
            let copy = self.copy_of(agent);
            let copied: Vec<&Value> = waiting.iter().filter(|i| copy.is_some() && i.get("from").and_then(Value::as_str) == copy.as_deref()).collect();
            let answering = copy.is_some() && value.get("answering") == Some(&json!(true));
            return (!waiting.is_empty() || !held.is_empty()).then(|| json!({"agent": agent, "doing": text("doing"), "waiting": waiting, "owner": owner, "raised": raised, "writing": writing, "held": held, "hold": hold, "push": push, "pull": pull, "worked": worked, "since": text("since"), "place": text("place"), "forkable": forkable, "told": told, "copy": copy, "copied": copied, "answering": answering}));
        }
        let id = value.get("item").and_then(Value::as_str)?;
        let find = |xs: &[Value]| xs.iter().find(|i| i.get("id").and_then(Value::as_str) == Some(id)).cloned();
        let (item, hold) = match find(&waiting) {
            Some(i) => (i, None),
            None => (find(&held)?, why),
        };
        Some(match name {
            syntax::STOPPED => json!({"agent": agent, "item": item, "why": text("why")}),
            // the copy's facts (syntax.rs ARRIVED, N-1104 p1154): FORKABLE and COPY_ANSWERS as coordd read them with the
            // event, so a replay reads the same; BEGAN (its `turn_began` line's) and COPY, the machine's
            _ => {
                let flag = |f: &str| value.get(f) == Some(&json!(true));
                json!({"agent": agent, "item": item, "waiting": waiting, "hold": hold, "began": self.began(agent).unwrap_or_default(), "forkable": flag("forkable"), "copy_answers": flag("copy_answers"), "copy": self.copy_of(agent)})
            }
        })
    }

    /// AGENT's delivery ticket for the run about to begin: its ticket now, or its next, opened on the text of
    /// `delivery.kind` loaded now when it has none or that text changed (on replay, where the record opened one, its
    /// `delivery_opened` line, on the text it recorded, and nowhere else). The ticket it replaces closes: no run of it
    /// is open, as a run begins only when the one before it has closed.
    fn delivery_ticket(&mut self, cx: &mut Cx, agent: &str) -> Option<u64> {
        let now = self.desks.get(agent).and_then(|d| d.ticket).filter(|d| self.tickets.get(d).is_some_and(Ticket::is_open));
        let c = self.next_desk();
        let new: Option<(String, BTreeMap<String, CallSig>)> = match &cx.recorded {
            Some(rec) => rec.iter().find_map(|l| match l {
                Line::Event { name, value, .. } if name == syntax::DELIVERY_OPENED && value.get("t").and_then(Value::as_u64) == Some(c) => {
                    let program = value.get("program").and_then(Value::as_str)?.to_string();
                    let calls = serde_json::from_value(value.get("calls").cloned().unwrap_or_else(|| json!({}))).ok()?;
                    Some((program, calls))
                }
                _ => None,
            }),
            None => {
                let text = self.defs.kinds.get(DELIVERY).map(|k| k.text.clone())?;
                (now.map(|d| &self.tickets[&d].program) != Some(&text)).then(|| {
                    let calls = self.calls_named(&text);
                    (text, calls)
                })
            }
        };
        let Some((text, calls)) = new else { return now };
        let k = self.recorded_program(DELIVERY, &text, &calls, cx).ok()?.0;
        let mut y = self.new_ticket(c, DELIVERY, None, None, json!({"agent": agent}), json!({}), text.clone(), &k, &cx.now);
        y.handlers.retain(|h| syntax::delivery_event(&h.ev));
        y.status = Status::Rests { env: Env::new(), scope: BTreeMap::new() };
        y.handling = Some(DELIVERY.into());
        y.raised_by = Some(agent.to_string());
        y.calls = calls.clone();
        self.tickets.edit().insert(c, y);
        let value = json!({"agent": agent, "t": c, "program": text, "calls": calls});
        cx.line(Line::Event { at: cx.now.clone(), name: syntax::DELIVERY_OPENED.into(), value, fired: Vec::new(), counted: Vec::new(), on: None });
        if let Some(old) = now {
            let mut none = Vec::new();
            self.close(cx, old, Status::Done(Value::Null), &mut none);
        }
        self.desks.edit().entry(agent.to_string()).or_default().ticket = Some(c);
        Some(c)
    }

    /// Hidden ticket C under U closed: when C was the open run of U's agent's delivery, the next event waiting runs
    /// (those with nothing left to decide pass).
    pub(super) fn delivery_closed(&mut self, cx: &mut Cx, u: u64, c: u64, work: &mut Vec<Work>) {
        let Some(agent) = self.delivery_agent(u) else { return };
        if self.desks.get(&agent).and_then(|d| d.busy) != Some(c) {
            return;
        }
        self.desks.edit().get_mut(&agent).unwrap().busy = None;
        loop {
            let Some((name, value)) = self.desks.edit().get_mut(&agent).and_then(|d| d.queue.pop_front()) else { return };
            if let Some(w) = self.delivery_run(cx, &agent, &name, &value) {
                work.push(w);
                return;
            }
        }
    }

    /// The agent whose delivery ticket D is.
    pub(super) fn delivery_agent(&self, d: u64) -> Option<String> {
        let x = self.tickets.get(&d).filter(|x| x.kind == DELIVERY && x.handling.as_deref() == Some(DELIVERY))?;
        x.form.get("agent").and_then(Value::as_str).map(str::to_string)
    }

    /// What waits for AGENT now (0.2, runtime-design §1), oldest ticket first: each post due to it that is for it
    /// (it wakes or informs it), each other thing due (a step, with the posts it carries; a closing notice; a
    /// withdrawal; words given), and each post kept from it, below its read mark too; not what is being handed.
    pub fn waiting(&self, agent: &str) -> Vec<Value> {
        self.waiting_of(agent, self.due(agent))
    }

    /// `waiting` of AGENT, DUE being what `due` gives it (read for several agents at once, `due_all`).
    pub fn waiting_of(&self, agent: &str, due: Vec<Delivery>) -> Vec<Value> {
        self.waiting_held(agent, due).into_iter().map(|(i, _)| i).collect()
    }

    /// `waiting_of`, each item with whether the owner's hold on agents' mail keeps it now (line 327): a post by
    /// `mail_held_post`, anything else by `mail_held` of its delivery.
    fn waiting_held(&self, agent: &str, due: Vec<Delivery>) -> Vec<(Value, bool)> {
        let desk = self.desks.get(agent);
        let kept = |id: &str| desk.and_then(|d| d.kept.get(id)).cloned();
        let held = |t: u64, n: u32| self.mail_held_post(t, n).is_some() || self.stopped_held(agent, t, n);
        let handing: BTreeSet<&String> = desk.map(|d| d.handing.iter().flat_map(|h| h.items.iter().chain(&h.digest)).collect()).unwrap_or_default();
        let mut out: Vec<(Value, bool)> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for d in due {
            let Some(x) = self.tickets.get(&d.t) else { continue };
            match &d.what {
                // his post told to it late (`ToldLate`): the one post, an item of its own by the delivery's id, so it
                // is handed and given as its own delivery (proto-lead N-1446 p569, (c2)); his post handed to a copy
                // (`deliver_to_copy`) is the copy's to answer, so it wakes the copy and goes to it now, as it does to
                // its agent (`free`'s `owner`, `push_of`'s "owner")
                What::Posts { unread } if told_post(&d.id).is_some() => {
                    let copy = self.copy_owner(agent).is_some();
                    for p in unread {
                        let mut item = self.post_item(x, p, agent, kept(&d.id));
                        item["id"] = json!(d.id);
                        if copy && p.by == "owner" {
                            for (f, v) in [("inform", json!(false)), ("push", json!(true)), ("why", json!("owner")), ("member", json!(false))] {
                                item[f] = v;
                            }
                        }
                        seen.insert(d.id.clone());
                        out.push((item, held(d.t, p.n)));
                    }
                }
                What::Posts { unread } => {
                    for p in unread.iter().filter(|p| p.wakes().chain(p.inform.iter()).any(|a| a == agent) && !self.answered_by_own(x, p, agent)) {
                        let item = self.post_item(x, p, agent, kept(&format!("{}.p{}", d.ticket, p.n)));
                        seen.insert(item["id"].as_str().unwrap_or_default().to_string());
                        out.push((item, held(d.t, p.n)));
                    }
                }
                what => {
                    // a step of a run its own act raised (item.kind's `seek` at `coord item start`): from it, and into
                    // its running turn (delivery.kind's `arrived`), or at once when it is idle (`free`'s `raised`)
                    let raised = matches!(what, What::Step { .. }) && x.raised_for(agent);
                    let (kind, from, words) = match what {
                        What::Step { call, .. } => ("step", x.opener.clone().or_else(|| raised.then(|| agent.to_string())).unwrap_or_else(|| "coordd".into()), crate::label::call(self, call)),
                        What::Closed { .. } => ("closed", String::from("coordd"), format!("{} closed", crate::label::title(self, x))),
                        What::Withdrawn { call, .. } => ("withdrawn", String::from("coordd"), format!("the step {} withdrawn", crate::label::call(self, call))),
                        What::Refused { call, .. } => ("refused", String::from("coordd"), format!("an answer to {} refused", crate::label::call(self, call))),
                        What::Words { from, .. } => ("words", from.clone(), String::from("words given as they are")),
                        // a post it had, taken back (0.2 B8): into a running turn at once (delivery.kind's `arrived`);
                        // his take-back is his word, first and alone when it is idle, as his posts are
                        What::TakenBack { n, by } => ("taken_back", by.clone(), format!("{}.p{n} taken back: not to act on", d.ticket)),
                        What::Posts { .. } => unreachable!(),
                    };
                    let owner = kind == "taken_back" && from == "owner";
                    seen.insert(d.id.clone());
                    let item = json!({"id": d.id, "thread": d.ticket, "label": crate::label::title(self, x), "from": from, "owner": owner, "names": true, "inform": d.inform, "lead": false, "answers": null, "text": words, "what": kind, "kept": kept(&d.id), "waits": false, "raised": raised, "about_turn": self.about_turn(x, agent), "into_turn": false, "push": !d.inform, "why": (!d.inform).then_some(kind), "own": own_of(x, agent), "member": false});
                    out.push((item, self.mail_held(&d).is_some()));
                }
            }
        }
        // kept below the read mark: waiting still, until a `deliver` carries it
        for (id, why) in desk.map(|d| &d.kept).into_iter().flatten() {
            let Some((t, n)) = post_id(id) else { continue };
            let Some(x) = self.tickets.get(&t) else { continue };
            let Some(p) = x.posts.iter().find(|p| p.n == n && p.visible() && p.moved.is_none()) else { continue };
            if !seen.contains(id) && !self.answered_by_own(x, p, agent) {
                out.push((self.post_item(x, p, agent, Some(why.clone())), held(t, n)));
            }
        }
        out.retain(|(i, _)| !i.get("id").and_then(Value::as_str).is_some_and(|id| handing.contains(&id.to_string())));
        // his posts a turn his own Esc cut under his hold left waiting go with his next post to AGENT, in one message
        // (`stopped_held`; consult-3 N-412.p2203, paper-research p2205)
        let stopped = |i: &Value| i.get("id").and_then(Value::as_str).and_then(post_id).is_some_and(|(t, n)| self.stopped_held(agent, t, n));
        if out.iter().any(|(i, h)| !*h && his_now(i) && !stopped(i)) {
            for (i, h) in &mut out {
                if stopped(i) {
                    *h = false;
                }
            }
        }
        out
    }

    /// Post P of X, waiting for AGENT, as the delivery events carry it (`syntax::waiting_type`). PUSH and WHY: whether
    /// it goes to AGENT now and why (`push_of`, I-1480); OWN: AGENT opened X or works on it (`own_of`).
    fn post_item(&self, x: &Ticket, p: &Post, agent: &str, kept: Option<String>) -> Value {
        let id = self.id(x.t);
        let why = self.push_of(x, p, agent);
        json!({
            "id": format!("{id}.p{}", p.n),
            "thread": id,
            "label": crate::label::title(self, x),
            "from": p.by,
            "owner": p.by == "owner",
            "names": p.names.iter().any(|a| a == agent),
            "inform": !p.wakes().any(|a| a == agent),
            // whether its author is above the agent: the event's to say (`delivery_value`)
            "lead": false,
            "answers": p.answers.map(|n| format!("{id}.p{n}")),
            "text": Text::Post { post: PostRef { t: x.t, n: p.n } }.value(),
            "what": "post",
            "kept": kept,
            "waits": p.by == "owner" && x.waits.contains(&p.n),
            "raised": false,
            "about_turn": self.about_turn(x, agent),
            "into_turn": p.into_turn.iter().any(|a| a == agent),
            "push": why.is_some(),
            "why": why,
            "own": own_of(x, agent),
            "member": why.is_none() && p.only_as_member(agent),
        })
    }

    /// Whether AGENT answered post P of X, another agent's, by a post of its own after it there, from the era
    /// [`ERA_QUIET`] (paper-research N-1446 p2367, 3): a reply-tool answer to it (`Post::answers`, or among its further
    /// ids, `Ticket::also_here`), or one written with P handed (`Ticket::answers`: P up to what it had been handed, and
    /// not kept from it). Such a post waits no more, due or kept: N-1446's p2295 came while paper-research worked and was
    /// kept for its turn's end; it answered it in p2298 (the reply tool's `to`), and 5 s later was handed it whole, for
    /// a Nothing turn. A post that only names P's writer is no answer: written without P (kept, or not yet handed), it
    /// proves nothing of what the agent read, and P would be lost (paper-research N-1446 p2430). Never his: his post
    /// goes until it is handed (`read_own`).
    fn answered_by_own(&self, x: &Ticket, p: &Post, agent: &str) -> bool {
        self.era >= ERA_QUIET
            && p.by != "owner"
            && p.by != agent
            && x.posts.iter().any(|q| q.n > p.n && q.by == agent && !q.dropped && !q.taken_back && (q.answers == Some(p.n) || x.also_here(q).contains(&p.n) || x.answers(q, p.n)))
    }

    /// Whether post P of X goes to AGENT now (I-1480 rule 1, paper-research N-1481 p1, p4, p7, p10, p12, p20), and
    /// why; None: it is pulled, given at AGENT's next turn's start in its digest (rule 2). The owner's post goes as
    /// it wakes AGENT, as before (rule 3: "owner"); so does one the policy tells it of (`told`, "told"). An agent's
    /// post P by Y goes to X when:
    /// - P is typed an ask, read or blocker to X ("ask", "read", "blocker");
    /// - P is untyped and names X (p4's (a): an untyped `@X` is an ask to X, "named"), but for a name the rule gave
    ///   it and its writer did not type (`Post::answering`: the rule for a post naming nobody, or a turn's end's Reply,
    ///   names the authors it answers; N-1481 p40, p43);
    /// - P is a hold on an item X works on (`coord item start`, `Ticket::started`; "hold");
    /// - P answers a post of X's that asked Y (`asks`), on X or by the reply tool's further ids ("reply"): by what
    ///   P answers, however it was sent (p12), so a turn answering several posts goes to each such sender;
    /// - P is a `result` by Y on the ticket where X's ask to Y stands, the first of Y's posts there since the ask
    ///   that answers it or is a result (p20, the waits-on edge: once, then closed; "result").
    /// An `@X` on a typed fyi or result pulls; so does an answer to X's fyi or result. From the era [`ERA_CLOSES`] P
    /// answers X's ask, as a reply and as the result X waits on, only by the ledger's rule (`closes`, paper-research
    /// N-1479 p107): one that only passes it on is pulled, and the edge waits on (`waits_on`).
    fn push_of(&self, x: &Ticket, p: &Post, agent: &str) -> Option<&'static str> {
        self.push_by(x, p, agent, self.era >= ERA_CLOSES)
    }

    /// `push_of`, by the ledger's close (`closes`) when CLOSE, else by the rule before the era [`ERA_CLOSES`].
    fn push_by(&self, x: &Ticket, p: &Post, agent: &str, close: bool) -> Option<&'static str> {
        if p.by == "owner" {
            return p.wakes().any(|a| a == agent).then_some("owner");
        }
        // from the era ERA_QUIET an fyi wakes nobody, told, typed to it or answering its ask: it is pulled
        // (paper-research N-1446 p2367, 5a: docket-edge6's lock notices, each a Nothing turn for two agents)
        let quiet = self.era >= ERA_QUIET;
        if quiet && matches!(p.kind, Some(PostKind::Fyi)) {
            return None;
        }
        if p.told.iter().any(|a| a == agent) {
            return Some("told");
        }
        // from the era ERA_QUIET a result answers an ask only as any post does (`closes`' WHOM; gemini-advisor p2365, 4)
        let answered = answered_first(x, p, agent, close, quiet);
        match &p.kind {
            Some(k) if k.to().iter().any(|a| a == agent) => return Some(k.is()),
            Some(PostKind::Hold { on }) if self.works_on(on, agent) => return Some("hold"),
            None if asks(p, agent) => return Some("named"),
            _ => {}
        }
        let further = p.also.iter().filter_map(|i| post_id(i)).filter_map(|(u, n)| self.tickets.get(&u).and_then(|y| Some((y, y.posts.iter().find(|m| m.n == n)?))));
        if answered.into_iter().any(|m| asks(m, &p.by)) || further.into_iter().any(|(_, m)| m.by == agent && asks(m, &p.by)) {
            return Some("reply");
        }
        if matches!(p.kind, Some(PostKind::Result)) && waits_on(x, p, agent, close, quiet) {
            return Some("result");
        }
        None
    }

    /// Post N of T: each agent it goes to otherwise by the ledger's close than by the rule before the era
    /// [`ERA_CLOSES`] (`push_by`), with why it goes by each, and where and how the two rules part. Read just after a
    /// post is made, to begin the era (`Machine::apply`), and to count what the era changes on a record (proto-lead
    /// N-1479 p109, 1; each way on its line, p144). The agents it may go to otherwise are those with a post on T before
    /// it: an ask, and so a reply or a result to one, is theirs.
    pub fn close_changes(&self, t: u64, n: u32) -> Vec<CloseChange> {
        let Some(x) = self.tickets.get(&t) else { return Vec::new() };
        let Some(p) = x.posts.iter().find(|p| p.n == n).filter(|p| p.by != "owner") else { return Vec::new() };
        let agents: BTreeSet<&str> = x.posts.iter().filter(|m| m.n < n && m.by != p.by && m.by != "owner").map(|m| m.by.as_str()).collect();
        agents.into_iter().filter_map(|a| {
            let (before, by) = (self.push_by(x, p, a, false), self.push_by(x, p, a, true));
            (before != by).then(|| {
                let (at, how) = parting(x, p, a, self.era >= ERA_QUIET).unwrap_or((n, "same"));
                CloseChange { agent: a.to_string(), before, by, at, how }
            })
        }).collect()
    }

    /// Whether AGENT works on item ON (`I-1474`): it said so with `coord item start` (`Ticket::started`).
    fn works_on(&self, on: &str, agent: &str) -> bool {
        parse_ticket(on).and_then(|t| self.tickets.get(&t)).is_some_and(|i| i.started.iter().any(|a| a == agent))
    }

    /// Whether X is about what AGENT's running turn holds: X stands at a post (`Machine::outline_parent`: its tie, else
    /// the ticket it was opened under) of a thread the turn was handed something on (`given_since`, as a question's
    /// `about` is read, machine.rs `about`). Its brain's line stands at the owner's post it is about, so while the turn
    /// his post went into runs, the line is about it (0.3; consult-3 N-1444 p20.3). A fact of every item; only an item
    /// whose post asks it (`into_turn`) went in on it, by the `arrived` of a delivery ticket opened on the text before
    /// 76: on 76's no line goes into a running turn, and the brain's waits for its end, whole (`free`'s `told`;
    /// fresh-lead-3 N-1540 p5, p18).
    fn about_turn(&self, x: &Ticket, agent: &str) -> bool {
        let Some((p, _)) = self.outline_parent(x.t) else { return false };
        self.given_since.get(agent).is_some_and(|g| g.iter().any(|d| parse_delivery(d).is_some_and(|(t, _)| t == p)))
    }

    /// The agent whose delivery run C is, if C is one: coordd answers its watcher's step itself (`next`).
    pub fn run_of(&self, c: u64) -> Option<String> {
        self.run_agent(c).ok()
    }

    /// The agent whose delivery run C is (a hidden ticket under its delivery ticket).
    pub(super) fn run_agent(&self, c: u64) -> Result<String, String> {
        self.tickets.get(&c).and_then(|x| x.under).and_then(|d| self.delivery_agent(d)).ok_or_else(|| String::from("the delivery acts run only in a handler of an agent's delivery ticket (`delivery.kind`)"))
    }

    /// A `turn_began` line (TURN_BEGAN): the delivery its agent's turn began with is kept on its desk (`Desk::began`),
    /// in place of the one before.
    pub(super) fn turn_began_read(&mut self, value: &Value) -> (bool, Result<String, String>) {
        let Some(agent) = value.get("agent").and_then(Value::as_str).filter(|a| !a.is_empty()) else {
            return (false, Err(String::from("A turn began names its agent.")));
        };
        let began = value.get("delivery").and_then(Value::as_str).filter(|d| !d.is_empty()).map(str::to_string);
        if self.desks.get(agent).map(|d| &d.began) != Some(&began) {
            self.desks.edit().entry(agent.to_string()).or_default().began = began.clone();
        }
        (true, Ok(match began {
            Some(d) => format!("{agent}'s turn began with {d}."),
            None => format!("{agent}'s turn began."),
        }))
    }

    /// The delivery AGENT's turn began with, as its last `turn_began` line said (`Desk::began`).
    pub fn began(&self, agent: &str) -> Option<String> {
        self.desks.get(agent).and_then(|d| d.began.clone())
    }

    /// AGENT's copy now (`Desk::copies`, the newest), if its delivery made one and has not stopped it.
    pub fn copy_of(&self, agent: &str) -> Option<String> {
        self.desks.get(agent).and_then(|d| d.copies.last()).map(|(c, _)| c.clone())
    }

    /// The agent COPY is a copy of (`Desk::copies`), while it is one.
    pub fn copy_owner(&self, copy: &str) -> Option<String> {
        self.desks.iter().find(|(_, d)| d.copies.iter().any(|(c, _)| c == copy)).map(|(a, _)| a.clone())
    }

    /// Whether COPY has a post of his it was handed (`deliver` to a copy, a `ToldLate`) and has not taken yet: with it
    /// in a turn, coordd says its agent's copy is still answering (`free`'s `answering`).
    pub fn copy_owed(&self, copy: &str) -> bool {
        self.tickets.values().any(|x| x.told_late.iter().any(|e| e.to == copy && !e.given))
    }

    /// Whether COPY is a copy run C's delivery made and has not stopped (`stop` ends it, not its turn).
    pub(super) fn copy_in_run(&self, c: u64, copy: &str) -> bool {
        self.run_agent(c).ok().and_then(|a| self.desks.get(&a)).is_some_and(|d| d.copies.iter().any(|(n, _)| n == copy))
    }

    /// `fork A at PLACE` in run C (fresh-lead-3 N-1104 p1154; paper-research N-1446 p3020, 1): a copy of C's agent A,
    /// beside it, its session cut after the line delivery PLACE was given on; named as a scoped agent of A's delivery
    /// ticket D (`f{D}-{N}`), kept by D, so it is not ended until a `stop` of it (`Machine::ended`). coordd starts it
    /// (`Effect::Copy`); one it cannot start is dropped from A's copies by its `start_failed` line. On the record as
    /// the act's line, as every act is; replay makes the copy again with the run, and starts nothing.
    pub(super) fn act_fork(&mut self, cx: &mut Cx, c: u64, a: &Value, place: &Value) -> Result<String, String> {
        let agent = self.run_agent(c)?;
        if a.as_str() != Some(agent.as_str()) {
            return Err(format!("`fork … at` in {agent}'s delivery copies {agent}, not {}", crate::print::value(a)));
        }
        let Some(place) = place.as_str().filter(|p| !p.is_empty()).map(str::to_string) else {
            return Err(format!("`fork {agent} at` names no delivery to cut its session at"));
        };
        let d = self.tickets[&c].under.expect("a run is under its delivery ticket (run_agent)");
        let copy = self.next_scoped(d);
        self.tickets.edit().get_mut(&d).unwrap().kept.push(copy.clone());
        self.desks.edit().entry(agent.clone()).or_default().copies.push((copy.clone(), place.clone()));
        acted_with(&mut self.desks, cx, &agent, "fork", &[], &[], &[], Some(&place), Some(&format!("a copy of its session cut there, {copy}")));
        cx.effect(Effect::Copy { agent: copy.clone(), t: d, call: COPY_CALL.into(), from: agent, place });
        Ok(copy)
    }

    /// `deliver COPY ITEMS` in run C, COPY a copy C's delivery made: of ITEMS, the posts waiting for C's agent are
    /// handed to COPY, each on its own whatever COPY has read (`ToldLate`, `pN.told`), by COPY's own delivery; they
    /// still wait for the agent, and the run decides what it gets of them (delivery.kind keeps them for its turn's
    /// end). Anything else ITEMS names is not a copy's: left as it is.
    fn deliver_to_copy(&mut self, cx: &mut Cx, agent: &str, copy: &str, items: &Value) -> Result<(), String> {
        let waiting = self.waiting_ids(agent);
        let posts: Vec<String> = still_in(&waiting, items).into_iter().filter(|i| post_id(i).is_some()).collect();
        for (t, n) in posts.iter().filter_map(|i| post_id(i)) {
            let Some(x) = self.tickets.edit().get_mut(&t) else { continue };
            if !x.late_for(copy, n) {
                x.told_late.push(ToldLate { n, to: copy.to_string(), given: false });
                cx.effect(Effect::Changed { t });
            }
        }
        acted_with(&mut self.desks, cx, agent, "deliver", &posts, &[], &[], None, Some(&format!("to its copy, {copy}")));
        cx.effect(Effect::Deliver { to: copy.to_string() });
        Ok(())
    }

    /// `stop COPY` in run C, COPY a copy C's delivery made: it ends, its session with it (`Effect::End`), and is no
    /// longer the agent's copy nor kept by its delivery ticket. On the record as the act's line.
    pub(super) fn stop_copy(&mut self, cx: &mut Cx, c: u64, copy: &str) {
        let Ok(agent) = self.run_agent(c) else { return };
        self.drop_copy(copy);
        acted_with(&mut self.desks, cx, &agent, "stop", &[], &[], &[], None, Some(&format!("its copy, {copy}")));
        cx.effect(Effect::End { agent: copy.to_string() });
    }

    /// COPY is nobody's copy any more, nor kept by a delivery ticket: stopped (`stop_copy`), or coordd could not start
    /// it (`start_failed`).
    pub(super) fn drop_copy(&mut self, copy: &str) {
        let owners: Vec<String> = self.desks.iter().filter(|(_, d)| d.copies.iter().any(|(n, _)| n == copy)).map(|(a, _)| a.clone()).collect();
        // another scoped agent (a step's, a `let`'s) is kept by its own ticket, not here
        if owners.is_empty() {
            return;
        }
        for a in owners {
            self.desks.edit().get_mut(&a).unwrap().copies.retain(|(n, _)| n != copy);
        }
        let keeping: Vec<u64> = self.tickets.iter().filter(|(_, x)| x.kept.iter().any(|k| k == copy)).map(|(t, _)| *t).collect();
        for d in keeping {
            self.tickets.edit().get_mut(&d).unwrap().kept.retain(|k| k != copy);
        }
        // his posts handed to it and not taken: nobody's to take now (they still wait for its agent)
        let owing: Vec<u64> = self.tickets.iter().filter(|(_, x)| x.told_late.iter().any(|e| e.to == copy && !e.given)).map(|(t, _)| *t).collect();
        for t in owing {
            self.tickets.edit().get_mut(&t).unwrap().told_late.retain(|e| e.to != copy || e.given);
        }
    }

    /// Post N of T, written by a copy (`Desk::copies`), is told to the copy's agent late (`ToldLate`, `pN.told`): it
    /// waits for the agent whole, whatever the agent has read of T or is told of it, and `free` gives it as `copied`
    /// (fresh-lead-3 N-1104 p1159: "what the copy told you, whole"; as a post naming him it would come by its first
    /// line). Made as the post is, from the desk the record's runs made, so a replay makes the same.
    pub(super) fn copy_posted(&mut self, cx: &mut Cx, t: u64, n: u32) {
        let Some(by) = self.tickets.get(&t).and_then(|x| x.posts.iter().find(|p| p.n == n)).map(|p| p.by.clone()) else { return };
        let Some(agent) = self.copy_owner(&by) else { return };
        let x = self.tickets.edit().get_mut(&t).unwrap();
        if !x.late_for(&agent, n) {
            x.told_late.push(ToldLate { n, to: agent, given: false });
            cx.effect(Effect::Changed { t });
        }
    }

    /// The ids of what waits for AGENT now, in their order: read once for an act (`waiting` reads every ticket's due).
    fn waiting_ids(&self, agent: &str) -> Vec<String> {
        self.waiting(agent).iter().filter_map(|i| i.get("id").and_then(Value::as_str).map(str::to_string)).collect()
    }

    /// `deliver A ITEMS listing REST` in run C: of what it names, what waits for C's agent is handed as one message;
    /// the rest it names that waits is named at its foot. A kept item handed is kept no more. A thread reads in order:
    /// a post handed brings with it the posts of its thread before it that wait and are not kept, in their order, so
    /// none is named at the foot and then passed by the read mark of the `given` that covers it.
    ///
    /// What the owner's hold on agents' mail keeps (line 327) is not handed nor named at the foot, wherever `deliver`
    /// names it or would bring it along: it is kept, with the hold's words (`MailHold::why`), so the read mark passing
    /// it loses nothing. delivery.kind never names it (`delivery_value`); this is for what was decided before the hold
    /// went on (a `next` open then) and for a post before his on its thread whose own event is still to run.
    ///
    /// DIGEST (I-1480, `deliver … digest PULL`): of what it names, what waits and is neither handed nor held goes with
    /// the message by its first line, given as ITEMS are; a post of it before a handed one on its thread is not brought
    /// along whole, since the read mark passing it passes what was given, but for one the hold keeps, which is kept
    /// (proto-reader N-1481 p39). Nothing goes when no item is handed, and it waits on: a digest starts no turn.
    /// - His post, and a post that asked to go into a turn (`into_turn`: the brain's), is never cut to its first
    ///   line: of DIGEST, it goes whole with the message, first, as `next` gave it before (coordd's
    ///   `his_post_that_only_informs_it_holds_nothing_back`; N-1481 p44). From the era [`ERA_QUIET`] the brain's alone:
    ///   his in DIGEST only informs the agent, and goes by its first line (paper-research N-1446 p2367, 1).
    /// - From the era [`ERA_QUIET`] a post brought along before a handed one on its thread that only informs the agent
    ///   (it does not go now, `push_of`) goes by its first line, as of DIGEST, whether the act names one or not
    ///   (paper-research N-1446 p2367, 2).
    /// - A message holds at most one step, since the agent's last message answers it: a step ITEMS names after the
    ///   first is named at the foot, first, and waits on (as coordd's answer to `next` kept it on the text before:
    ///   clerk.rs's `next_ok`, gone with c's coordd half, docket-core2 N-1481 p142).
    /// An act without DIGEST, as every program before it wrote, does neither.
    ///
    /// FROM (`fork A at PLACE with ITEMS …`, part 4, docket-core N-1446 p2096): the same message, made so, whose
    /// agent goes on from a copy of its session cut after PLACE's `given` line (its work's delivery id, `free`'s
    /// `place`). Its line says `fork` with `at`, and its hand carries PLACE for coordd (`Hand::fork`). A PLACE that is
    /// empty or "-" (no work given, `free`'s own) names no place: the act is a `deliver`, said so on its line.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn act_deliver(&mut self, cx: &mut Cx, c: u64, a: &Value, items: &Value, rest: Option<&Value>, digest: Option<&Value>, from: Option<&Value>) -> Result<(), String> {
        let agent = self.run_agent(c)?;
        // its copy (`fork … at`, N-1104 p1154): handed the posts on their own, by its own delivery
        if let Some(copy) = a.as_str().filter(|x| *x != agent && self.copy_in_run(c, x)) {
            return self.deliver_to_copy(cx, &agent, copy, items);
        }
        if a.as_str() != Some(agent.as_str()) {
            return Err(format!("`deliver` in {agent}'s delivery hands to {agent} or a copy of it its delivery made, not to {}", crate::print::value(a)));
        }
        let all = self.waiting_held(&agent, self.due(&agent));
        let id_of = |i: &Value| i.get("id").and_then(Value::as_str).map(str::to_string);
        let waiting: Vec<String> = all.iter().filter_map(|(i, _)| id_of(i)).collect();
        let held: BTreeSet<String> = all.iter().filter(|(_, h)| *h).filter_map(|(i, _)| id_of(i)).collect();
        let named = still_in(&waiting, items);
        let kept = self.kept(&agent);
        let digesting = digest.is_some();
        let is = |i: &String, f: &str, v: Value| all.iter().any(|(w, _)| id_of(w).as_ref() == Some(i) && w.get(f) == Some(&v));
        // what goes by its first line is given so, not whole with a post after it on its thread (I-1480)
        let mut pulled: Vec<String> = digest.map(|r| still_in(&waiting, r)).unwrap_or_default().into_iter().filter(|i| !named.contains(i)).collect();
        // but his words and the brain's go whole, first, when a message goes; from the era ERA_QUIET only the brain's:
        // what is pulled does not go now, so his there only informs it (routed or told to another), and goes by its first
        // line (paper-research N-1446 p2367, 1)
        let quiet = self.era >= ERA_QUIET;
        let whole: Vec<String> = if named.iter().any(|i| !held.contains(i)) { pulled.iter().filter(|i| !held.contains(*i) && ((!quiet && is(i, "owner", json!(true))) || is(i, "into_turn", json!(true)))).cloned().collect() } else { Vec::new() };
        pulled.retain(|i| !whole.contains(i));
        let named: Vec<String> = whole.into_iter().chain(named).collect();
        let mut items: Vec<String> = Vec::new();
        // brought along before a handed post but only informing the agent: from the era ERA_QUIET by its first line, so
        // that the read mark passes what was given, not whole (paper-research N-1446 p2367, 2)
        let mut swept: Vec<String> = Vec::new();
        for i in &named {
            if let Some((t, n)) = post_id(i) {
                let mut before: Vec<(u32, &String)> = waiting
                    .iter()
                    .filter(|w| !items.contains(*w) && !(pulled.contains(*w) && !held.contains(*w)) && (named.contains(*w) || !kept.contains_key(*w)))
                    .filter_map(|w| post_id(w).filter(|(u, m)| *u == t && *m < n).map(|(_, m)| (m, w)))
                    .collect();
                before.sort();
                for (_, w) in before {
                    if quiet && !named.contains(w) && !held.contains(w) && !is(w, "push", json!(true)) {
                        if !swept.contains(w) {
                            swept.push(w.clone());
                        }
                    } else {
                        items.push(w.clone());
                    }
                }
            }
            if !items.contains(i) {
                items.push(i.clone());
            }
        }
        let holding: Vec<String> = items.iter().filter(|i| held.contains(*i)).cloned().collect();
        items.retain(|i| !held.contains(i));
        // one step a message: a later one waits, first at the foot
        let steps: Vec<String> = items.iter().filter(|i| digesting && is(i, "what", json!("step"))).skip(1).cloned().collect();
        items.retain(|i| !steps.contains(i));
        let digest: Vec<String> = if items.is_empty() { Vec::new() } else { pulled.into_iter().chain(swept).filter(|i| !items.contains(i) && !held.contains(i)).collect() };
        let rest: Vec<String> = rest.map(|r| still_in(&waiting, r)).unwrap_or_default().into_iter().filter(|i| !items.contains(i) && !held.contains(i) && !digest.contains(i) && !steps.contains(i)).collect();
        let rest: Vec<String> = steps.into_iter().chain(rest).collect();
        let fork: Option<String> = from.and_then(Value::as_str).filter(|p| !p.is_empty() && *p != "-").map(str::to_string);
        let unplaced = (from.is_some() && fork.is_none()).then_some("fork with no place: delivered");
        acted_with(&mut self.desks, cx, &agent, if fork.is_some() { "fork" } else { "deliver" }, &items, &rest, &digest, fork.as_deref(), unplaced);
        if !holding.is_empty() {
            let why = self.mail_hold.as_ref().map(MailHold::why).unwrap_or_default();
            self.act_hold(cx, c, true, &json!(holding), &json!(why))?;
        }
        if items.is_empty() {
            return Ok(());
        }
        // made while the agent was idle: a turn begun since gets it at its end (`Hand::idle`), unless it carries a step
        let idle = self.tickets.get(&c).and_then(|x| x.handling.as_deref()) == Some(syntax::FREE) && !items.iter().any(|i| is(i, "what", json!("step")));
        let d = self.desks.edit().entry(agent.clone()).or_default();
        for i in items.iter().chain(&digest) {
            d.kept.remove(i);
            d.reported.remove(i);
        }
        d.handing.push(Hand { items: items.clone(), rest: rest.clone(), digest: digest.clone(), fork, idle });
        for t in items.iter().chain(&digest).filter_map(|i| parse_delivery(i)).map(|(t, _)| t).collect::<BTreeSet<_>>() {
            cx.effect(Effect::Changed { t });
        }
        cx.effect(Effect::Hand { to: agent, items, rest, digest });
        Ok(())
    }

    /// `keep ITEMS WHY` (KEEP) or `report ITEMS WHY` in run C: each of them that waits for C's agent is kept, or
    /// reported, with WHY.
    pub(super) fn act_hold(&mut self, cx: &mut Cx, c: u64, keep: bool, items: &Value, why: &Value) -> Result<(), String> {
        let agent = self.run_agent(c)?;
        let why = why.as_str().unwrap_or_default().to_string();
        let items = still_in(&self.waiting_ids(&agent), items);
        let act = if keep { "keep" } else { "report" };
        acted(&mut self.desks, cx, &agent, act, &items, &[], Some(&why));
        if items.is_empty() {
            return Ok(());
        }
        let d = self.desks.edit().entry(agent).or_default();
        for i in &items {
            if keep { d.kept.insert(i.clone(), why.clone()) } else { d.reported.insert(i.clone(), why.clone()) };
        }
        for t in items.iter().filter_map(|i| parse_delivery(i)).map(|(t, _)| t).collect::<BTreeSet<_>>() {
            cx.effect(Effect::Changed { t });
        }
        Ok(())
    }

    /// The owner's Release (MAIL_HOLD off, line 327): what his hold kept, each item kept with WHY (its `keep`'s words,
    /// `MailHold::why`), is decided again. An item still due is kept no more, so coordd's round raises it as it raises
    /// anything new (`free`, or `arrived` in a turn) and the program decides it as if no hold had been. A post below
    /// its agent's read mark (a post of his after it on its thread was handed) waits only by its keep, so it stays
    /// kept, for ENDED (`MailHold::ended`), until the agent's next `free` decides it (coordd owes one to each agent so
    /// kept from at the Release, and to no other, `desk_released`). Each item's ticket is changed. How many were let
    /// go; on replay the same.
    pub(super) fn hold_lifted(&mut self, cx: &mut Cx, why: &str, ended: &str) -> usize {
        let agents: Vec<String> = self.desks.iter().filter(|(_, d)| d.kept.values().any(|w| w == why)).map(|(a, _)| a.clone()).collect();
        let mut lifted = 0;
        for agent in agents {
            let due: BTreeSet<String> = self
                .due(&agent)
                .into_iter()
                .flat_map(|d| match d.what {
                    What::Posts { .. } if told_post(&d.id).is_some() => vec![d.id],
                    What::Posts { unread } => unread.iter().map(|p| format!("{}.p{}", d.ticket, p.n)).collect::<Vec<String>>(),
                    _ => vec![d.id],
                })
                .collect();
            let d = self.desks.edit().get_mut(&agent).unwrap();
            let held: Vec<String> = d.kept.iter().filter(|(_, w)| *w == why).map(|(i, _)| i.clone()).collect();
            for i in &held {
                if due.contains(i) {
                    d.kept.remove(i);
                } else {
                    d.kept.insert(i.clone(), ended.to_string());
                }
            }
            lifted += held.len();
            for t in held.iter().filter_map(|i| parse_delivery(i)).map(|(t, _)| t).collect::<BTreeSet<_>>() {
                cx.effect(Effect::Changed { t });
            }
        }
        lifted
    }

    /// TO was given DELIVERY of T up to post UPTO: what it covers of what was being handed to TO is in (the delivery
    /// itself, and the posts of T up to UPTO). What it names (the delivery, its last post) is kept or reported no
    /// more, however it was handed (typed into its terminal, say); a post kept below UPTO stays kept, still owed.
    pub(super) fn delivery_given(&mut self, t: u64, to: &str, delivery: &str, upto: u32) {
        // his posts it covers that waited for his next post or his Release are given: they wait no more
        if let Some(s) = self.stopped_his.get_mut(to) {
            s.retain(|(u, n)| *u != t || *n > upto);
            if s.is_empty() {
                self.stopped_his.remove(to);
            }
        }
        let last = format!("{}.p{upto}", self.id(t));
        if self.desks.get(to).is_some_and(|d| [delivery, last.as_str()].iter().any(|i| d.kept.contains_key(*i) || d.reported.contains_key(*i))) {
            let d = self.desks.edit().get_mut(to).unwrap();
            for i in [delivery, last.as_str()] {
                d.kept.remove(i);
                d.reported.remove(i);
            }
        }
        if self.desks.get(to).is_none_or(|d| d.handing.is_empty()) {
            return;
        }
        let covered = |i: &String| i == delivery || post_id(i).is_some_and(|(u, n)| u == t && n <= upto);
        let d = self.desks.edit().get_mut(to).unwrap();
        for h in d.handing.iter_mut() {
            h.items.retain(|i| !covered(i));
            h.digest.retain(|i| !covered(i));
        }
        d.handing.retain(|h| !h.items.is_empty());
    }

    /// For INPUT, a turn's end whose reply answers a post kept from its agent and handed since, the agent's own post
    /// to write first (`apply`): when its reply on that thread written while the post was kept answers it by the rule
    /// before the runtime, the engine before it would write no reply here, and the record would part from it there and
    /// at every later reply on the thread. Written as the agent's post (an input, which both replay as written), under
    /// the post it answers, naming whom the reply would name; the turn's end then finds it answered and replies no more.
    /// So too for a turn whose own post there, written with the post handed, only passed it on or is not shown where it
    /// was written: the rule Reply answers what the turn held by the ledger's rule (`Machine::unanswered_by`,
    /// paper-research N-1479 p136), and the rule before counted that turn answered and replied nowhere; and for one
    /// whose reply-tool answer named a post this turn was handed among its further ids, though a post put into the
    /// turn after it is not answered (consult-3 N-1479 p340, on N-1397 p58's `to` of "p4, p3").
    /// With it, of its names the Reply's own that its message did not type (its `answering`, as `Machine::reply`'s).
    pub(super) fn kept_reply(&self, input: &Input) -> Option<(Input, Vec<String>)> {
        let Input::TurnEnded { agent, text, later, go: None, .. } = input else { return None };
        if text.len == 0 || self.desks.is_empty() {
            return None;
        }
        let given = self.turn_given(agent, later);
        let (t, mut names, upto, before) = self.reply_names_of(agent, &given)?;
        let x = self.tickets.get(&t)?;
        // the rule before: answered by a post of its own numbered after UPTO, written with UPTO handed; or by one whose
        // reply-tool further ids here answer a post of this turn's (`reply_names_of`'s own test before the ledger's)
        if !x.posts.iter().any(|p| p.by == *agent && p.n > upto && (upto <= x.answers_upto(p) || x.further_in_turn(p, before, upto))) {
            return None;
        }
        let named = self.reply_named.as_ref().filter(|(a, _)| a == agent).map(|(_, n)| n.clone()).unwrap_or_default();
        let answering: Vec<String> = names.iter().filter(|a| !named.contains(a)).cloned().collect();
        for a in named {
            if a != *agent && !names.contains(&a) && self.readers.as_ref().is_none_or(|r| r.contains(&a)) {
                names.push(a);
            }
        }
        Some((Input::Post { t, by: agent.clone(), text: Text::Loc { loc: text.clone() }, names, told: Vec::new(), inform: Vec::new(), answers: Some(upto), kind: None }, answering))
    }

    /// The posts of T kept from AGENT up to post UPTO: what a post of AGENT's written now does not answer.
    pub(super) fn kept_posts(&self, agent: &str, t: u64, upto: u32) -> BTreeSet<u32> {
        let Some(d) = self.desks.get(agent) else { return BTreeSet::new() };
        d.kept.keys().filter_map(|i| post_id(i)).filter(|(u, n)| *u == t && *n <= upto).map(|(_, n)| n).collect()
    }

    /// The messages handed and not yet in, to hand again after a start (`resume_effects`).
    pub(super) fn handing_effects(&self) -> Vec<Effect> {
        self.desks.iter().flat_map(|(a, d)| d.handing.iter().map(move |h| Effect::Hand { to: a.clone(), items: h.items.clone(), rest: h.rest.clone(), digest: h.digest.clone() })).collect()
    }

    /// AGENT's items kept now, each with why.
    pub fn kept(&self, agent: &str) -> BTreeMap<String, String> {
        self.desks.get(agent).map(|d| d.kept.clone()).unwrap_or_default()
    }

    /// AGENT's items reported now, each with why.
    pub fn reported(&self, agent: &str) -> BTreeMap<String, String> {
        self.desks.get(agent).map(|d| d.reported.clone()).unwrap_or_default()
    }

    /// AGENT's delivery's last acts, the newest first (`Desk::acts`): its view's `acted`.
    pub fn last_acts(&self, agent: &str) -> Vec<crate::view::ViewActed> {
        self.desks.get(agent).map(|d| d.acts.iter().rev().cloned().collect()).unwrap_or_default()
    }

    /// Whether a delivery run of AGENT's is open (a decision in flight): `free` is not raised for it meanwhile.
    pub fn deciding(&self, agent: &str) -> bool {
        self.desks.get(agent).and_then(|d| d.busy).is_some_and(|c| self.tickets.get(&c).is_some_and(Ticket::is_open))
    }

    /// The events waiting for AGENT's open run, in order (names).
    pub fn delivery_queue(&self, agent: &str) -> Vec<String> {
        self.desks.get(agent).map(|d| d.queue.iter().map(|(n, _)| n.clone()).collect()).unwrap_or_default()
    }

    /// Whether a delivery program is loaded (`kind delivery`): coordd raises the delivery events only then.
    pub fn delivery_loaded(&self) -> bool {
        self.defs.kinds.contains_key(DELIVERY)
    }

    /// The messages `deliver` made for AGENT that no `given` line has covered yet, oldest first.
    pub fn hands(&self, agent: &str) -> Vec<Hand> {
        self.desks.get(agent).map(|d| d.handing.clone()).unwrap_or_default()
    }

    /// The owner's urgent hand (N-1788, `Line::Urgent`): his posts waiting for AGENT, not held and not already on
    /// their way, go to it as one message now, into its running turn too (not an idle hand, which waits for the turn's
    /// end: `Hand::idle`), whatever its delivery program would have done with them. Nothing of his waiting is an error,
    /// said, not a silent nothing.
    pub(super) fn urgent(&mut self, cx: &mut Cx, agent: &str, by: &str) -> Result<String, String> {
        if by != "owner" {
            return Err(String::from("Only the owner hands his words over at once."));
        }
        let handing: BTreeSet<String> = self.desks.get(agent).map(|d| d.handing.iter().flat_map(|h| h.items.iter().chain(&h.digest)).cloned().collect()).unwrap_or_default();
        let items: Vec<String> = self
            .waiting_held(agent, self.due(agent))
            .into_iter()
            .filter(|(i, held)| !held && i.get("owner") == Some(&json!(true)))
            .filter_map(|(i, _)| i.get("id").and_then(Value::as_str).map(str::to_string))
            .filter(|i| !handing.contains(i))
            .collect();
        if items.is_empty() {
            return Err(format!("Nothing of the owner's waits for {agent} to be handed."));
        }
        acted_with(&mut self.desks, cx, agent, "urgent", &items, &[], &[], None, None);
        let d = self.desks.edit().entry(agent.to_string()).or_default();
        for i in &items {
            d.kept.remove(i);
            d.reported.remove(i);
        }
        d.handing.push(Hand { items: items.clone(), rest: Vec::new(), digest: Vec::new(), fork: None, idle: false });
        for t in items.iter().filter_map(|i| parse_delivery(i)).map(|(t, _)| t).collect::<BTreeSet<_>>() {
            cx.effect(Effect::Changed { t });
        }
        cx.effect(Effect::Hand { to: agent.to_string(), items: items.clone(), rest: Vec::new(), digest: Vec::new() });
        Ok(format!("{} handed to {agent} now: {}.", if items.len() == 1 { "One item of the owner's is" } else { "The owner's items are" }, items.join(", ")))
    }

    /// The items an event raised for AGENT names, queued or in its open run: raised already, not to be raised again.
    pub fn raised_items(&self, agent: &str) -> BTreeSet<String> {
        let Some(d) = self.desks.get(agent) else { return BTreeSet::new() };
        let open = d.busy.and_then(|c| self.tickets.get(&c)).filter(|x| x.is_open()).and_then(|x| x.form.get("item")).and_then(|i| i.get("id")).and_then(Value::as_str).map(str::to_string);
        d.queue.iter().filter_map(|(_, v)| v.get("item").and_then(Value::as_str).map(str::to_string)).chain(open).collect()
    }
}
