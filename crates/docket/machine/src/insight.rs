//! Insights into a project's data, as lenses (S6; his p646: "should be in 3.0 as a insight i can get into my data";
//! proto-lead N-1446 p388). An insight is a query over what coordd holds plus how the window draws it (structure.md
//! §9): here the query, over the machine coordd holds, read under its lock (`Request::Insight`), nothing recorded and
//! no model asked; then its numbers, one function each, which the window's lens and the tests share, so a number on
//! screen is the number tested.
//!
//! Insight (3), **open → done** (his N-1002: "track threads initial time -> completion … we cannot have features or
//! threads stay open for days"; N-1020: "completion … as a kpi … completion rate time it takes"): every line of the
//! outline, a thread or an item ([`view::outlined`]), with when it was opened, who opened it and when it ended. It
//! ended when the outline stopped holding it open ([`ViewOutline::of`]'s `open` and `ended_at`): it closed (its
//! program returned: done, dropped, kept; or failed), or, an agents' thread, it was ended (`coord end`); one that
//! still owed the owner then ended when it stopped owing him (proto-lead N-1446 p473, 3). A thread closed `Moved` is
//! not counted: his words went onto another thread (p473, 2). His threads (he or his message opened them) and the
//! others (an agent opened them, or a program: a clock, a `when`, an agent's event) are counted apart (p499, 1; p581).
//! What it counts is [`OpenToDone`].
//!
//! Its third row, **his asks** from asked to his sign-off (his N-1002 p2: "track threads initial time -> completion and
//! estimate "utility""; consult-2's design, N-1002 p6; paper-research N-1446 p1786): each post of his the ASKED ledger
//! says asks an agent to act (`Machine::asked`, the graph's `asks` edges: every post of his that wakes an agent, so
//! nearly every one, a short "eh fine" too), as the record's graph reads its course to the last point it has reached
//! (`Graph::asks_now`), with his sign-off and its word of worth (`machine::SIGNED_OFF`, `AskTimes::signed`) and, beside
//! it, what it took: its cost (`Graph::costs_at`), and his posts on it after it was in his window
//! (`Graph::his_after`). A row is an ask, its thread its second label: since N-1104 his asks share one thread that
//! never ends (paper-research p1786, 1). The ledger's rule says what an ask is; nothing here picks among them. What it
//! counts is [`Asks`].
//!
//! Insight (1), **work and talk** (his p640: "track how many turs go into wokr"; p641: "our unit is work x model (size
//! / maybe)"): every turn the record holds (a `turn_ended` line: an agent's turn given something from a thread), with
//! what it did as its line says ([`Worked`](crate::Worked): its own tool calls by name, the model that answered). A
//! turn worked when it called a tool other than coord's own (it read, searched, ran, edited, built); it only talked
//! when it called coord's alone ([`coords_own`]: replies, asks, `coord` run alone in a shell) or none; and what it did
//! is not known when its line says neither (a line from before turns carried it). Read from the record's file, off
//! coordd's lock, as S4a's Record is: the machine keeps no turn once ended. What it counts is [`WorkTalk`]: by agent,
//! by model (no size is made up: each model by its name) and by hour.
//!
//! Insight (2), **agents per topic** (his N-1104.p646: "agents per topic time to oc,mption"; paper-research's brief,
//! line 20: "agents per topic (thread or item)"; proto-lead N-1446 p653): a topic is a thread of the outline with what
//! stands under it, or an item. A line's agents are those given anything on it and, an item's, those who started it;
//! a topic's are the distinct names of its lines, so its number adds up to the names on its lines' rows (proto-lead
//! p671). Beside each, its time from open to done, as (3) counts it. Counted from (3)'s rows, which carry each line's
//! agents, starters and the line it stands under ([`Thread::agents`], [`Thread::started`], [`Thread::parent`]). What
//! it counts is [`AgentsPerTopic`].

use crate::graph::{AskNow, Cost};
use crate::line::{At, Line};
use crate::machine::{HIDDEN, Machine, OPENER, Status, Ticket, coordds_own, secs};
use crate::runner::RUNNER;
use crate::view::{self, ViewOutline};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A day, in seconds: a thread open this long or longer is "open for days" (his N-1002).
pub const DAY: i64 = 86_400;

/// A line of the outline as insights (3) and (2) count it: ID and LABEL as the outline has them, its KIND (`thread`,
/// `item`, or another kind drawn as a line), when it was OPENED, BY the agent or the binding that opened it
/// ([`opened_by`]; none: he or his message did), when it ENDED (none while the outline holds it open:
/// [`ViewOutline::ended_at`]), its STATE in the outline's one word (done, dropped, kept, failed when it ended; waits on
/// you, waits on NAME, quiet, at work while open), whether the owner ARCHIVED it, LAST, the number of its last post,
/// where the lines of its end follow; for (2), its PARENT, the line it stands under (none at the outline's root), the
/// AGENTS given anything on it and on what stands under it that is no line of its own, and, an item's, the agents who
/// STARTED it. Each list by name, each name once.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub opened: At,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<At>,
    pub state: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub archived: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub started: Vec<String>,
}

/// Every line of M's outline, open or ended, in the order of their numbers, but those closed `Moved`: what
/// `Request::Insight` answers for insights (3) and (2), one JSON line each. A line's PARENT is the line its outline
/// parent ([`Machine::outline_up`]) is, or stands under. Its AGENTS are the `given` lines' agents of every ticket
/// that counts for it: itself, and what stands under it that is no line of its own, its runs, steps, branches and
/// marks, and a line closed `Moved` (each ticket counts for the nearest line it stands under). The workers coordd
/// answers itself (the watcher, its minder: [`coordds_own`]) and the runner are given steps and are no agents: not
/// counted.
pub fn threads(m: &Machine) -> Vec<Thread> {
    let rows: BTreeSet<u64> = m.tickets.values().filter(|x| x.t < HIDDEN && view::outlined(m, x) && !matches!(&x.status, Status::Done(v) if v["head"] == "Moved")).map(|x| x.t).collect();
    let mut agents: BTreeMap<u64, BTreeSet<&str>> = BTreeMap::new();
    for y in m.tickets.values().filter(|y| !y.given.is_empty()) {
        if let Some(l) = row_of(m, &rows, y.t) {
            agents.entry(l).or_default().extend(y.given.iter().map(|g| g.to.as_str()).filter(|a| !coordds_own(a) && *a != RUNNER));
        }
    }
    let names = |v: BTreeSet<&str>| v.into_iter().map(String::from).collect::<Vec<_>>();
    rows.iter()
        .map(|t| {
            let x = &m.tickets[t];
            let o = ViewOutline::of(m, x, false);
            let parent = m.outline_up(x.t).and_then(|p| row_of(m, &rows, p)).map(|p| m.id(p));
            let agents = names(agents.remove(t).unwrap_or_default());
            let started = names(x.started.iter().map(String::as_str).collect());
            Thread { id: o.id, label: o.label, kind: x.kind.clone(), opened: o.opened_at, by: opened_by(m, x), ended: o.ended_at, state: o.state, archived: o.archived, last: x.posts.last().map(|p| p.n), parent, agents, started }
        })
        .collect()
}

/// The row of ROWS ticket T counts for: T when it is one, else the nearest one it stands under in the outline (a tie
/// that would make a loop is refused, so the walk ends); none when it stands under none.
fn row_of(m: &Machine, rows: &BTreeSet<u64>, mut t: u64) -> Option<u64> {
    while !rows.contains(&t) {
        t = m.outline_up(t)?;
    }
    Some(t)
}

/// Who opened X when not the owner: its opener (the open's, or the one the machine gave its form, `OPENER`); else the
/// binding that fired it, as written (a clock's `every 1h …`, an agent's event's `on …`), unless he raised it (his
/// message, or a press of his: the event's `from` is the owner); else, opened by a program (no opener, or the ticket
/// that program runs on as its opener: machine.rs `program_author`), who opened that ticket, walking up. None when that
/// is the owner: he opened it, or a binding he raised did (proto-lead N-1446 p581: only those are his; p637, 1).
pub fn opened_by(m: &Machine, x: &Ticket) -> Option<String> {
    let mut y = Some(x);
    while let Some(z) = y {
        // a closed run's form is its text (`form_text`), read back only where it names the field
        let packed = z.form_text.as_ref().and_then(|_| z.form_field(OPENER));
        let given = if z.form_text.is_some() { packed.as_ref() } else { z.form.get(OPENER) };
        match z.opener.as_deref().or_else(|| given.and_then(serde_json::Value::as_str)) {
            Some("owner") => return None,
            Some(a) if m.ticket_named(a).is_none() => return Some(a.to_string()),
            _ => {}
        }
        match (&z.binding, z.raised_by.as_deref()) {
            (Some(_), Some("owner")) => return None,
            (Some(b), _) => return Some(b.clone()),
            (None, _) => y = z.parent.and_then(|p| m.tickets.get(&p)),
        }
    }
    None
}

/// Insight (3)'s numbers: HIS, over the lines he or his message opened (the four tiles), and the AGENTS', over those
/// an agent or a program opened, not counted in his completion ([`Thread::by`]; proto-lead N-1446 p499, 1; p581).
#[derive(Clone, Debug, PartialEq)]
pub struct OpenToDone<'a> {
    pub his: Counts<'a>,
    pub agents: Counts<'a>,
}

/// Insight (3)'s numbers over some lines of the outline at a time NOW, each line with its seconds: from open to end
/// when it ended, else open at NOW (none when a time does not read as coordd writes one, never counted as zero):
/// - OPENED, every line (the record's whole life), the newest first;
/// - ENDED, those that ended ([`Thread::ended`]), the latest end first; so the completion rate, ended over opened
///   ([`Counts::rate`]);
/// - MEDIAN, the time from open to end of those, in seconds: the middle one of them in order, the shorter of the two
///   middles for an even count;
/// - OPEN, those still open, the longest open first, and DAYS, those of them open a [`DAY`] or longer at NOW.
#[derive(Clone, Debug, PartialEq)]
pub struct Counts<'a> {
    pub opened: Vec<(&'a Thread, Option<i64>)>,
    pub ended: Vec<(&'a Thread, Option<i64>)>,
    pub median: Option<i64>,
    pub open: Vec<(&'a Thread, Option<i64>)>,
    pub days: Vec<(&'a Thread, i64)>,
}

impl Counts<'_> {
    /// Ended over opened, none when nothing was opened.
    pub fn rate(&self) -> Option<f64> {
        (!self.opened.is_empty()).then(|| self.ended.len() as f64 / self.opened.len() as f64)
    }
}

/// [`OpenToDone`] of THREADS (in the order of their numbers, as [`threads`] gives them) at NOW, seconds since the epoch.
pub fn open_to_done(threads: &[Thread], now: i64) -> OpenToDone<'_> {
    let (his, agents): (Vec<&Thread>, Vec<&Thread>) = threads.iter().partition(|x| x.by.is_none());
    OpenToDone { his: counts(his, now), agents: counts(agents, now) }
}

/// One of his asks as Open → done's asks row lists it ([`asks`]): ASK, its course as the graph reads it, at the last
/// point reached and who was behind it, his sign-off in it (`AskTimes::signed`); WORDS, his post's first line as coordd
/// labels a post; ON, the label of the thread it is on; COST, what was sent for it (`Graph::costs_at`: the requests
/// in steps handed it, those of steps on posts that name it beside, never added in), none where coordd gave none;
/// AFTER, his posts on it once it was in his window ([`crate::graph::Graph::his_after`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ask {
    pub ask: AskNow,
    pub words: String,
    pub on: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
}

/// The asks row's numbers ([`asks`]), each a list of his asks:
/// - ASKED, every one, the newest first;
/// - STUCK, nobody moves: not in his window and not signed off, open in the ledger (no agent it asks has answered it in
///   a post that closes it), and no commit, merge or green job has come for it: the agents' to move. The longest
///   waiting first (the oldest asked);
/// - WORDS, answered in words: closed in the ledger with no commit for it, the latest close first;
/// - WINDOW, in his window: a build carrying its commits was installed there, the latest first;
/// - SIGNED, signed off, the latest sign-off first;
/// - YOURS, of WINDOW, those he has not signed off: the only ones that wait for him (paper-research N-1446 p1786, 2),
///   the longest in his window first.
#[derive(Clone, Debug, PartialEq)]
pub struct Asks<'a> {
    pub asked: Vec<&'a Ask>,
    pub stuck: Vec<&'a Ask>,
    pub words: Vec<&'a Ask>,
    pub window: Vec<&'a Ask>,
    pub signed: Vec<&'a Ask>,
    pub yours: Vec<&'a Ask>,
}

/// [`Asks`] of ROWS, in the order he asked them, as coordd gives them.
pub fn asks(rows: &[Ask]) -> Asks<'_> {
    // a function, not a closure: what it gives borrows from its argument
    fn t(a: &Ask) -> &crate::graph::AskTimes {
        &a.ask.times
    }
    let asked: Vec<&Ask> = rows.iter().rev().collect();
    let mut stuck: Vec<&Ask> = rows
        .iter()
        .filter(|a| {
            let x = t(a);
            x.in_window.is_none() && x.signed.is_none() && x.closed.is_none() && x.first_commit.is_none() && x.merged.is_none() && x.first_green.is_none()
        })
        .collect();
    stuck.sort_by(|a, b| (&t(a).asked, &t(a).post).cmp(&(&t(b).asked, &t(b).post)));
    let mut words: Vec<&Ask> = rows.iter().filter(|a| t(a).closed.is_some() && t(a).first_commit.is_none()).collect();
    words.sort_by(|a, b| t(b).closed.as_ref().map(|r| &r.at).cmp(&t(a).closed.as_ref().map(|r| &r.at)));
    let mut window: Vec<&Ask> = rows.iter().filter(|a| t(a).in_window.is_some()).collect();
    window.sort_by(|a, b| t(b).in_window.as_ref().map(|r| &r.at).cmp(&t(a).in_window.as_ref().map(|r| &r.at)));
    let mut signed: Vec<&Ask> = rows.iter().filter(|a| t(a).signed.is_some()).collect();
    signed.sort_by(|a, b| t(b).signed.as_ref().map(|s| &s.at).cmp(&t(a).signed.as_ref().map(|s| &s.at)));
    let mut yours: Vec<&Ask> = window.iter().copied().filter(|a| t(a).signed.is_none()).collect();
    yours.reverse();
    Asks { asked, stuck, words, window, signed, yours }
}

/// His ask A's seconds from asked to his sign-off when he signed it off, else open at NOW; none when a time does not
/// read as coordd writes one (never counted as zero).
pub fn ask_took(a: &Ask, now: i64) -> Option<i64> {
    let x = &a.ask.times;
    Some(x.signed.as_ref().map_or(Some(now), |s| secs(&s.at))? - secs(x.asked.as_deref()?)?)
}

/// [`Counts`] of THREADS, in the order of their numbers, at NOW.
fn counts(threads: Vec<&Thread>, now: i64) -> Counts<'_> {
    let opened: Vec<(&Thread, Option<i64>)> = threads.into_iter().rev().map(|x| (x, took(x, now))).collect();
    let (mut ended, mut open): (Vec<_>, Vec<_>) = opened.iter().copied().partition(|(x, _)| x.ended.is_some());
    ended.sort_by(|a, b| b.0.ended.cmp(&a.0.ended));
    open.sort_by_key(|(x, s)| (std::cmp::Reverse(*s), x.id.clone()));
    let median = median(ended.iter().filter_map(|(_, s)| *s).collect());
    let days = open.iter().filter_map(|(x, s)| Some((*x, (*s)?))).filter(|(_, s)| *s >= DAY).collect();
    Counts { opened, ended, median, open, days }
}

/// X's seconds from open to end when it ended, else open at NOW; none when a time does not read as coordd writes one
/// (never counted as zero).
pub fn took(x: &Thread, now: i64) -> Option<i64> {
    Some(x.ended.as_deref().map_or(Some(now), secs)? - secs(&x.opened)?)
}

/// The middle one of V in order, the smaller of the two middles for an even count; none for none.
fn median<T: Ord + Copy>(mut v: Vec<T>) -> Option<T> {
    v.sort_unstable();
    v.get(v.len().checked_sub(1)? / 2).copied()
}

/// Insight (2)'s numbers ([`per_topic`]): the THREADS' topics (a line of the outline that is no item: a thread, or a
/// line of another kind, with what stands under it) and the ITEMS'.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentsPerTopic<'a> {
    pub threads: Topics<'a>,
    pub items: Topics<'a>,
}

/// The topics of one kind: LIST, every one, the most agents first, then the newest first; the median number of their
/// AGENTS; the median time from open to end of those that ended (TOOK).
#[derive(Clone, Debug, PartialEq)]
pub struct Topics<'a> {
    pub list: Vec<Topic<'a>>,
    pub agents: Option<usize>,
    pub took: Option<i64>,
}

/// A topic: its LINE; its LINES (a thread's: itself and every line under it, in the order of their numbers; an item's:
/// itself); its AGENTS, by name, each once: every line's [`Thread::agents`] and [`Thread::started`]; SECS, its time from
/// open to end, or open ([`took`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Topic<'a> {
    pub line: &'a Thread,
    pub agents: Vec<&'a str>,
    pub lines: Vec<&'a Thread>,
    pub secs: Option<i64>,
}

/// [`AgentsPerTopic`] of THREADS (in the order of their numbers, as [`threads`] gives them) at NOW, seconds since the
/// epoch.
pub fn per_topic<'a>(threads: &'a [Thread], now: i64) -> AgentsPerTopic<'a> {
    let ids: BTreeMap<&str, &Thread> = threads.iter().map(|x| (x.id.as_str(), x)).collect();
    // each line, under itself and every line its parents lead up to, in the order of their numbers
    let mut under: BTreeMap<&str, Vec<&Thread>> = BTreeMap::new();
    for x in threads {
        let mut y = Some(x);
        while let Some(z) = y {
            under.entry(z.id.as_str()).or_default().push(x);
            y = z.parent.as_deref().and_then(|p| ids.get(p).copied());
        }
    }
    let (items, lines): (Vec<&Thread>, Vec<&Thread>) = threads.iter().rev().partition(|x| x.kind == "item");
    let topics = |xs: Vec<&'a Thread>| {
        let mut list: Vec<Topic<'a>> = xs
            .into_iter()
            .map(|x| {
                let lines = if x.kind == "item" { vec![x] } else { under[x.id.as_str()].clone() };
                let agents: BTreeSet<&str> = lines.iter().flat_map(|l| l.agents.iter().chain(&l.started).map(String::as_str)).collect();
                Topic { line: x, agents: agents.into_iter().collect(), lines, secs: took(x, now) }
            })
            .collect();
        list.sort_by_key(|t| std::cmp::Reverse(t.agents.len()));
        let agents = median(list.iter().map(|t| t.agents.len()).collect());
        let took = median(list.iter().filter(|t| t.line.ended.is_some()).filter_map(|t| t.secs).collect());
        Topics { list, agents, took }
    };
    AgentsPerTopic { threads: topics(lines), items: topics(items) }
}

/// A turn as insight (1) counts it, from its `turn_ended` line ([`turn`]): the AGENT's, ended AT, begun with DELIVERY
/// (the post it was given, `N-1104.p586`: where the Events layer shows it), what it did (TOOLS and MODEL,
/// [`Worked`](crate::Worked)), and LINE, the number of its line in the record (from 1), where the Record layer shows
/// it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub agent: String,
    pub at: At,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tools: BTreeMap<String, u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub line: u64,
}

/// The turn record line LINE, number N, is, when it is a `turn_ended`: what `Request::Insight` answers for insight (1),
/// one JSON line each.
pub fn turn(line: &Line, n: u64) -> Option<Turn> {
    let Line::TurnEnded { at, agent, delivery, tools, model, .. } = line else { return None };
    Some(Turn { agent: agent.clone(), at: at.clone(), delivery: delivery.clone(), tools: tools.clone(), model: model.clone(), line: n })
}

/// Whether TOOL, a tool's name as a turn's line records it, is one of coord's own, the ones that only talk: its MCP
/// server's, as Claude Code names them (`mcp__coord__reply`, `mcp__coord__ask`) and as Codex does (`coord__reply`); and
/// a shell's call that runs coord alone (`coord post …`, `coord msg …`: agents talk so, as standing.md tells them;
/// proto-reader N-1446 p511), which coordd's follower records as the shell's name with [`COORD_IN_A_SHELL`] after it
/// (`Bash(coord)`, `shell(coord)`). Any other shell call is work.
pub fn coords_own(tool: &str) -> bool {
    tool.starts_with("mcp__coord__") || tool.starts_with("coord__") || tool.ends_with(COORD_IN_A_SHELL)
}

/// What coordd's follower writes after a shell's name for a call that runs coord alone ([`coords_own`]).
pub const COORD_IN_A_SHELL: &str = "(coord)";

/// What a turn did ([`Turn::did`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Did {
    Work,
    Talk,
    NotKnown,
}

impl Turn {
    /// Work when it called a tool not of coord's own ([`coords_own`]); Talk when it called only those, or none and its
    /// line names the model that answered; NotKnown when its line names neither tools nor model.
    pub fn did(&self) -> Did {
        if self.tools.keys().any(|t| !coords_own(t)) {
            Did::Work
        } else if self.model.is_some() || !self.tools.is_empty() {
            Did::Talk
        } else {
            Did::NotKnown
        }
    }
}

/// One row of insight (1): its LABEL (an agent, a model, an hour, or "" for turns whose line names no model) and its
/// turns by what they did, each list the newest first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tally<'a> {
    pub label: String,
    pub work: Vec<&'a Turn>,
    pub talk: Vec<&'a Turn>,
    pub not_known: Vec<&'a Turn>,
}

impl<'a> Tally<'a> {
    fn add(&mut self, x: &'a Turn) {
        match x.did() {
            Did::Work => self.work.push(x),
            Did::Talk => self.talk.push(x),
            Did::NotKnown => self.not_known.push(x),
        }
    }

    /// Its turns, every one.
    pub fn len(&self) -> usize {
        self.work.len() + self.talk.len() + self.not_known.len()
    }

    /// Work turns over the turns whose work is known (work and talk), none when there is none.
    pub fn share(&self) -> Option<f64> {
        let known = self.work.len() + self.talk.len();
        (known > 0).then(|| self.work.len() as f64 / known as f64)
    }
}

/// Insight (1)'s numbers over TURNS ([`turn`]'s, in the record's order): ALL of them, then by AGENTS and by MODELS, the
/// most turns first (then by label), and by HOURS, the hour of the day it ended in (`2026-11-20T18`, UTC, as coordd
/// writes a time), the latest first. Each list of turns the newest first.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkTalk<'a> {
    pub all: Tally<'a>,
    pub agents: Vec<Tally<'a>>,
    pub models: Vec<Tally<'a>>,
    pub hours: Vec<Tally<'a>>,
}

/// [`WorkTalk`] of TURNS.
pub fn work_talk(turns: &[Turn]) -> WorkTalk<'_> {
    let mut all = Tally::default();
    let mut by: [BTreeMap<&str, Tally>; 3] = Default::default();
    for x in turns.iter().rev() {
        all.add(x);
        let keys = [x.agent.as_str(), x.model.as_deref().unwrap_or_default(), x.at.get(..13).unwrap_or(&x.at)];
        for (m, k) in by.iter_mut().zip(keys) {
            m.entry(k).or_insert_with(|| Tally { label: k.to_string(), ..Default::default() }).add(x);
        }
    }
    let [mut agents, mut models, hours] = by.map(|m| m.into_values().collect::<Vec<_>>());
    for v in [&mut agents, &mut models] {
        v.sort_by_key(|t| std::cmp::Reverse(t.len()));
    }
    WorkTalk { all, agents, models, hours: hours.into_iter().rev().collect() }
}
