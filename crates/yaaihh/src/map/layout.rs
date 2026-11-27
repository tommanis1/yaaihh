//! The Map's layout (I-1478, docket-edge5's part: docket-edge6 N-1479 p8, p12): each project's island, its nodes and
//! the links between them, made from what the window holds, and each node placed once in the one world and kept there
//! (p715: "the same places, more detail as you go in").
//!
//! - **What it reads**, and nothing else: the outline's lines as Insight (3) answered them (insight.rs
//!   `Insights::threads`, the rows of `Request::Insight { OpenToDone }`, docket-machine `insight::threads`, which
//!   insight (2)'s `per_topic` reads too), or from a coordd before `INSIGHT_PROTOCOL` the lines its snapshot holds
//!   ([`lines_of`]); the snapshot's tickets and agents (live.rs `Live`); its coordd's accounts (`Request::Accounts`, as
//!   the Accounts page holds them). Nothing here asks coordd, reads a record or a log, or reads words in posts.
//! - **Topics** (paper-research N-1479 p9: the record's own links, then communities). A line stands in the topic of the
//!   line at the top of its chain of parents (`Thread::parent`: the line it was opened from or stands under, the walk
//!   insight (2) makes): an item, or a thread with lines under it, is a topic of its own, named as that line is. A line
//!   alone at the outline's root stands in the topic of an item its opening post names: read off the item's side, as
//!   every snapshot holds each item's view and its `TicketView::mentioned` lists every post that names it, on tickets
//!   open or closed long ago (coordd's index of the ids in every post's words; consult-3 N-1479 p30: the line's own
//!   view is in the snapshot only while open or for a day). Else, way 2, in the topic whose lines its agents are on, each
//!   agent counting as it is rare (one on every line says nothing of where a line belongs); else with the lines given
//!   to the same agents as it, a topic named by them.
//! - **Places, once** (docket-edge6 p8, items 3 and 6: nothing reflows, no simulation runs at view time). An island's
//!   topics are placed greedily, the largest first, each in the free room nearest the topics it shares agents with, so
//!   linked regions sit near each other; a topic's lines in rows by their numbers, with room left under them. Each
//!   place is kept ([`Kept`], `map.json` beside ui.json, [`path`]) and read back as it is: a new line takes free room
//!   in its topic, a new topic free room in its island, a new island the room to the right of the others; a topic
//!   with no room left grows down into free room, or goes on in a region of its own beside it; an island grows down.
//!   Nothing placed moves.
//! - **Rows for posts** (docket-edge6 N-1479 p23: level 4 puts posts top-down by number at a fixed row height inside
//!   the line's rect, and the layout sizes the rect for the rows): a line is placed with rows for twice its posts or
//!   more ([`Rows`]), and with the room its rows grow into next kept free. Every post's row, on every line, is one size
//!   ([`LINE_W`] by [`ROW_H`]), so level 4 reads alike in each thread and a post's level-5 card at its row's start is
//!   the same everywhere (docket-edge8 N-1479 p71, p112): a card is its rows, one column of them up to [`ROWS_MOST`],
//!   past that columns of it. A line whose posts fill its rows takes twice as many, down, where that is free and it
//!   has one column short of [`ROWS_MOST`], else a column to its right; a row's height and width never change, so a
//!   new post moves no other ([`Rows::cell`]).
//! - **What moves**: agents are dots on the topic they are on now (p715 level 2: "agents as dots on the topic they
//!   work on"), in a row along its head; with none, along their island's. Accounts are meters in the island's corner.
//!   Neither is kept.
//! - **Rank**: a line's [`Tier`] is his open ask on it (a post of his no later post answers), what waits on him, a
//!   failed or broken one, the rest; its weight its posts. A topic and an island take their lines' highest tier and
//!   the sum of their weights.
//! - **What its far levels draw** beside the nodes: the island's badge ([`Badge`]), its agents held on a limit by what
//!   holds them (p715 path 2, [`Cluster`]), the accounts' meters ([`Meter`]) and his posts as lit points ([`Lit`]).
//! - **Programs** (docket-edge8 N-1479 p17, p23; p715 path 3 starts at a handler): the handlers that ran, each a kind's
//!   `when` with the runs it made under the tickets the snapshot holds ([`handled`]), lie in the island's region
//!   Programs ([`PROGRAMS`]), each laid as a line is, its runs its posts: a card of one-size rows, kept, grown as its
//!   runs fill it, a new one put in the region's free room or a region beside it. Never by way 1 or 2: Programs is
//!   placed as a topic is, through the same loops (docket-edge5 N-1479 p211, 2). Its runs are put in its rows by
//!   map/join.rs, oldest first, and nothing keeps a run's row: **posts never move, runs can** (proto-lead p209). When a
//!   ticket leaves the snapshot, a day after it closed, its runs go, and the handler's later runs move up as many rows.
//!   No row is taken off a card when runs go: a card stays as large as its most runs (on 21 Nov research's thread.kind
//!   `when` on `agent_post` listed 1,122 runs under the 149 open tickets alone: five columns). Uncapped: a card of
//!   only the latest runs would move every row at each new run, and a run he double-clicks must not slide away under
//!   him (proto-lead N-1479 p244).
//!
//! Not drawn, so nothing promises them (paper-research N-1479 p9, p14; docket-edge6 p12): heads and their reads (until
//! I-1480's `read`), what a build carried and the build band itself (its own head after this one: a read-only Insight
//! arm for the window-release lines), his asks' ring, fill and trail (until (B)'s `Machine::ledger` is on proto-0.3:
//! his posts are lit points), a topic tied when a thread opens (his to decide), Codex's and the gateway's own use in
//! the corner (the window holds no meter of either: the gateway is a node, with no fill).
//!
//! Ids are only unique within a project (consult-3 N-1479 p16), so each island is a [`Graph`] of its own, all in the
//! one world, side by side.

use super::{Edge, Graph, Kind, Link, Node, Rect, Refused, State, Tier};
use coord_types::{AgentView, LimitWindow, NativeAccounts, TicketView};
use docket_machine::insight::Thread;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

/// A line's card at its least, in world units: a chat column, which level 4 draws the thread's posts in.
const LINE_W: f64 = 180.0;
const LINE_H: f64 = 240.0;
/// A line's rows when it is placed: twice its posts or more, a power of two in its one column (so its card's whole
/// units are exact), at least this many (N-1479 p18 said 16: a thread placed at its first post would fill at its
/// 32nd).
const ROWS_LEAST: u32 = 64;
/// A post's row, on every line, is [`LINE_W`] by this (3.75): 48 to 1, a line of words at level 4, which a line's fit
/// at full width draws 22 points tall in 1080 (N-1479 p112).
const ROW_H: f64 = LINE_H / ROWS_LEAST as f64;
/// The most rows in a column of a line (1,920 units tall): a line of more than half as many posts is placed in
/// columns of this many, and one that fills this many grows by a column, as a level-4 fit opens one column.
const ROWS_MOST: u32 = 512;
/// Between lines, and between topics.
const GAP: f64 = 24.0;
const TOPIC_GAP: f64 = 96.0;
/// A topic's margin, and its head: its label, then a row of its agents' dots.
const TOPIC_PAD: f64 = 40.0;
const TOPIC_HEAD: f64 = 96.0;
/// An agent's dot, and where its row is under the top of its topic (or of its island).
const DOT: f64 = 24.0;
const DOTS_Y: f64 = 56.0;
const ISLAND_DOTS_Y: f64 = 120.0;
/// An island's margin; its head (its label and badge, the agents on no topic, the accounts' corner); between islands.
const ISLAND_PAD: f64 = 200.0;
const ISLAND_HEAD: f64 = 240.0;
const ISLAND_GAP: f64 = 2_000.0;
const ISLAND_LEAST_W: f64 = 2_400.0;
/// An account's meter, and where the corner's row is under the island's top.
const METER_W: f64 = 160.0;
const METER_H: f64 = 48.0;
const METERS_Y: f64 = 40.0;
/// The room left for what is still to come: a topic's area over its lines', an island's over its topics'.
const TOPIC_ROOM: f64 = 2.0;
const ISLAND_ROOM: f64 = 1.4;
/// What a line's agents must share with a topic's lines for it to stand there (way 2): one agent on a tenth of the
/// lines (its weight `ln 10`, 2.3) on half of the topic's.
const JOIN: f64 = 1.0;
/// A day, in seconds: his posts of the last day are lit, as the snapshot holds the tickets closed in it.
const DAY: i64 = 86_400;
/// The kept file's shape: a file of a later one is left as it is.
const SHAPE: u32 = 1;

/// One project, as the window holds it: what its island is made from.
pub struct Island<'a> {
    /// `Proj::id`, which the island's places are kept by, and its name.
    pub id: &'a str,
    pub name: &'a str,
    /// Its outline's lines in the order of their numbers: Insight (3)'s rows, else [`lines_of`] its snapshot.
    pub lines: &'a [Thread],
    /// Its snapshot's tickets and agents (`Live::tickets`, `Live::agents`).
    pub tickets: &'a BTreeMap<u64, TicketView>,
    pub agents: &'a [AgentView],
    /// Its coordd's last answer to Accounts, when one came.
    pub accounts: Option<&'a NativeAccounts>,
}

/// An island laid out: its nodes and links (its project first), and what its far levels draw from them.
#[derive(Debug)]
pub struct Laid {
    pub graph: Graph,
    /// Each line's rows for its posts, by its node.
    pub rows: BTreeMap<u32, Rows>,
    pub badge: Badge,
    pub held: Vec<Cluster>,
    pub meters: Vec<Meter>,
    pub lit: Vec<Lit>,
}

/// A line's rows for its posts (docket-edge6 N-1479 p23): ROWS in each of its COLS columns, each row of one height
/// and width for as long as the line is drawn, as one that fills takes more rows under it or a column beside it, never
/// a smaller row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rows {
    pub rows: u32,
    pub cols: u32,
}

impl Rows {
    /// Post N's row (from 1) in its line's rect R: top-down by number, then the next column; none past the last, when
    /// the line is full (no room was free to grow into; level 4 says how many more).
    pub fn cell(&self, r: Rect, n: u32) -> Option<Rect> {
        let i = n.checked_sub(1)?;
        let (col, row) = (i / self.rows, i % self.rows);
        if col >= self.cols {
            return None;
        }
        let (w, h) = (r.w / self.cols as f64, r.h / self.rows as f64);
        Some(Rect::new(r.x + col as f64 * w, r.y + row as f64 * h, w, h))
    }
}

/// The island's one badge (p715 level 1): its lines that wait on him; what is stopped or broken (its agents stopped,
/// its open tickets broken); its agents held on a limit ("16 held · limit", path 2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Badge {
    pub waits: usize,
    pub stopped: usize,
    pub held: usize,
}

/// Agents held, by what holds them (p715 path 2: "a red cluster of agents points at one node: Gateway · Opus cooling
/// until 13:31"): ON, that node (the gateway, for a rate limit that is not their account's, `AgentView::limit_until`;
/// the account they came from, for those that wait with every account out, `account_waiting`), none when the window
/// knows of none; the AGENTS' nodes; the MODEL they ran; UNTIL when (seconds, the latest of theirs); WHY, the 429's own
/// words.
#[derive(Clone, Debug, PartialEq)]
pub struct Cluster {
    pub on: Option<u32>,
    pub agents: Vec<u32>,
    pub model: String,
    pub until: Option<i64>,
    pub why: Option<String>,
}

/// An account's meter in the island's corner: its NODE; the share USED of its fuller window (5 hours or the week),
/// and when that one starts again (RESETS); OUT until when it takes no request, and WHY (`NativeAccount::out_for`).
/// The gateway's has none of them.
#[derive(Clone, Debug, PartialEq)]
pub struct Meter {
    pub node: u32,
    pub used: Option<f64>,
    pub resets: Option<i64>,
    pub out: Option<i64>,
    pub why: Option<String>,
}

/// A post of his, a lit point on its line (p715 level 2 and path 1): its LINE's node, the POST's id (`N-1104.p678`,
/// what `lens::zoom` opens), OPEN while no later post answers it (`TicketView::unanswered_owner`), each agent it
/// ASKED with the post that answered it (`TicketView::asked`, (B)'s ledger: none from a coordd before
/// `ASKED_PROTOCOL`, nor for a post that asks nobody). His posts of the last day, and the older ones still open.
/// Or, TO_HIM, an agent's post that asks him and waits on him (threads.rs `open_to_owner`, the one test his card,
/// Needs and the waits-on-you list read: proto-window N-1479 p315), lit while it does, whatever its age; OPEN then,
/// and asking no agent.
#[derive(Clone, Debug, PartialEq)]
pub struct Lit {
    pub line: u32,
    pub post: String,
    pub open: bool,
    pub asked: Vec<(String, Option<String>)>,
    pub to_him: bool,
}

/// The places kept (`map.json`): each island's, by its project's id. Read back as they are; only what is new is added.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Kept {
    #[serde(default)]
    pub shape: u32,
    #[serde(default)]
    pub islands: BTreeMap<String, KeptIsland>,
}

/// An island's places: its own rect, its topics' regions and its lines' cards, by id. A rect is `[x, y, w, h]`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KeptIsland {
    pub rect: [f64; 4],
    #[serde(default)]
    pub topics: BTreeMap<String, KeptTopic>,
    #[serde(default)]
    pub lines: BTreeMap<String, KeptLine>,
}

/// A topic's region: its rect and label; OF, the topic it goes on from when it is a region of its own beside it
/// (`topic:I-1469~2` of `topic:I-1469`); WITH, the agents a topic of way 2 is named by (none for the others).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KeptTopic {
    pub rect: [f64; 4],
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub with: Option<Vec<String>>,
}

/// A line's card, the region it lies in, and its rows for its posts ([`Rows`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KeptLine {
    pub rect: [f64; 4],
    pub topic: String,
    pub rows: u32,
    pub cols: u32,
}

/// The kept places' file: `map.json` beside ui.json (`map-dev.json` beside ui-dev.json); no window from before reads
/// it, so the way back is untouched.
pub fn path() -> PathBuf {
    let name = if crate::DEV.load(std::sync::atomic::Ordering::Relaxed) { "map-dev.json" } else { "map.json" };
    Path::new(&crate::projects::config_path()).with_file_name(name)
}

/// The places kept at P, none when there is no file yet. Why not, when it is there and this window cannot read it (a
/// later window's shape, or not JSON): then it is left as it is, and this window's places are not written over it, as
/// the lens pins' file is (sidebar.rs; consult-3 N-1446 p326).
pub fn read(p: &Path) -> Result<Kept, String> {
    match std::fs::read_to_string(p) {
        Ok(t) => match serde_json::from_str::<Kept>(&t) {
            Ok(k) if k.shape <= SHAPE => Ok(k),
            Ok(k) => Err(format!("its shape {} is a later window's", k.shape)),
            Err(e) => Err(e.to_string()),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Kept::default()),
        Err(e) => Err(e.to_string()),
    }
}

/// Write K to P whole or not at all (state.rs `write_whole`, as ui.json).
pub fn write(p: &Path, k: &Kept) -> std::io::Result<()> {
    crate::state::write_whole(p, &serde_json::to_string(k).unwrap_or_default())
}

/// The lines a snapshot holds, as Insight (3)'s rows give them, for a coordd before `INSIGHT_PROTOCOL`: its tickets
/// drawn as lines (`TicketView::outline`), each with its own agents (given anything on it), its opener for BY (the
/// rows' `opened_by` walks further: a binding, a program's opener), and none of the lines closed before the last day.
pub fn lines_of(tickets: &BTreeMap<u64, TicketView>) -> Vec<Thread> {
    tickets
        .values()
        .filter_map(|t| {
            let o = t.outline.as_ref()?;
            let agents: BTreeSet<&str> = t.given.iter().map(|g| g.to.as_str()).collect();
            Some(Thread {
                id: t.id.clone(),
                label: o.label.clone(),
                kind: t.kind.clone(),
                opened: o.opened_at.clone(),
                by: t.opener.clone().filter(|a| a != "owner").or_else(|| t.binding.clone()),
                ended: o.ended_at.clone(),
                state: o.state.clone(),
                archived: o.archived,
                last: t.posts.last().map(|p| p.n),
                parent: o.parent.clone(),
                agents: agents.into_iter().map(String::from).collect(),
                started: t.started.clone(),
            })
        })
        .collect()
}

/// The region the handlers that ran lie in: the island's Programs, which p715's explorer lists. One beside it, once it
/// is full, is `programs~2`, as a topic's is. No topic is named so: theirs are `topic:…`.
pub const PROGRAMS: &str = "programs";

/// A handler that ran, as the snapshot's `ran` lists give it: a kind's `when`, by the kind it is written in and the
/// event it is on (`ViewRan::from`, `event`), and its runs under every ticket the snapshot holds, oldest first.
pub struct Handled<'a> {
    /// Its node's id, `handler:{kind}:{event}` (docket-edge6 N-1479 p23's `handler:{kind}:{when}`).
    pub id: String,
    pub kind: &'a str,
    pub event: &'a str,
    /// Each run, with the id of the ticket it ran under.
    pub runs: Vec<(&'a str, &'a docket_machine::view::ViewRan)>,
}

/// The handlers that ran under TICKETS, by kind and event, each with its runs by number (the order they opened in).
/// Not by the `when`'s line, though a run's line tells two `when`s of one event apart (picture.rs `handler_rows`):
/// the line is the one the `when` was at when the run opened, so each edit above it in its kind would make another
/// handler, with a place of its own kept for good (docket-edge5 N-1479 p211, 5: research's thread.kind's one `when` on
/// `agent_post` stood at five lines on 21 Nov). Two `when`s of one event in one kind are one handler here. A run of no
/// `when` (a delivery's desk run, a stop) is no handler's, and is not drawn. Only the tickets the snapshot holds are
/// counted: the open ones, and those closed in the last day.
pub fn handled(tickets: &BTreeMap<u64, TicketView>) -> Vec<Handled<'_>> {
    let mut by: BTreeMap<(&str, &str), Handled> = BTreeMap::new();
    for (under, r) in tickets.values().flat_map(|t| t.ran.iter().map(move |r| (t.id.as_str(), r))) {
        let (Some(kind), Some(_)) = (r.from.as_deref(), r.line) else { continue };
        let event = r.event.as_str();
        by.entry((kind, event)).or_insert_with(|| Handled { id: format!("handler:{kind}:{event}"), kind, event, runs: Vec::new() }).runs.push((under, r));
    }
    let number = |id: &str| id.trim_start_matches(|c: char| !c.is_ascii_digit()).parse::<u64>().unwrap_or(u64::MAX);
    by.into_values()
        .map(|mut h| {
            h.runs.sort_by(|(_, a), (_, b)| number(&a.id).cmp(&number(&b.id)).then_with(|| a.id.cmp(&b.id)));
            h
        })
        .collect()
}

/// Handler H as the layout lays a line ([`rows_for`], [`card`], [`claim`] size it unchanged): `last`, a line's last
/// post's number, holds its count of runs, so that it has rows for them as a line has for its posts; its label is its
/// kind and its event in words (as a thread's handler rows say it: picture/docket.rs `event_words`); at work while a
/// run of it is, else quiet, so that Programs' colour says a program is running (docket-edge5 N-1479 p211, 3).
fn handler_line(h: &Handled) -> Thread {
    let state = if h.runs.iter().any(|(_, r)| r.closed_at.is_none()) { "at work" } else { "quiet" };
    Thread {
        id: h.id.clone(),
        label: format!("{} · {}", h.kind, crate::picture::docket::event_words(h.event)),
        kind: "handler".into(),
        opened: h.runs.first().map(|(_, r)| r.opened_at.clone()).unwrap_or_default(),
        by: None,
        ended: None,
        state: state.into(),
        archived: false,
        last: Some(u32::try_from(h.runs.len()).unwrap_or(u32::MAX)),
        parent: None,
        agents: Vec::new(),
        started: Vec::new(),
    }
}

/// What this module reads of island ISL, as one number: islands of one key lay alike at one NOW in one [`Kept`]. A
/// relay lays an island again when its key changes or NOW passes a time a state laid changes at, and else keeps what
/// it laid (proto-lead N-1479 p118, p124). It is what is read, and only that:
/// - the island's id and name; each line but when it opened, and of its opener and its end only whether it has one;
/// - of each ticket its id, kind, parent and `under`, the posts that name it (`mentioned`: which post of which
///   ticket), his posts' numbers and times, `unanswered_owner`, and whether it is broken and whether closed; of each
///   run it lists (`ran`), its id, its `when`'s kind and whether it has a line, its event and whether it ended;
/// - of each agent its name, state, model, `on`, items, its limit (`limit_until`, `limit_why`), and its wait for an
///   account (`account_waiting`, `account_from`, `account_until`);
/// - whether the accounts came and `gateway`, and of each account its id, label, tier, its two windows' use and
///   reset, `out_until` and `out_for`.
///
/// Not NOW (p122): what changes with time alone, a limit or an account's `out_until` passing and his post going out of
/// the last day ([`last_day_ends`]), is the relay's to time. Not an agent's tools, nor the words of a post, nor the
/// others' posts' times: nothing here reads them.
///
/// It is made of three parts, each what one kind of event can change, so a relay can make again only the part whose
/// source moved (proto-window N-1479 p134: an agent's tool call keys no ticket): [`key_lines`], [`key_agents`],
/// [`key_accounts`].
pub fn key(isl: &Island) -> u64 {
    let mut h = DefaultHasher::new();
    (key_lines(isl), key_agents(isl.agents), key_accounts(isl.accounts)).hash(&mut h);
    h.finish()
}

/// [`key`]'s part of island ISL's id, name, lines and tickets: what a ticket's event or Insight's rows can change. Of
/// each run a ticket lists (`ran`), what [`handled`] reads: its id, its `when`'s kind and whether it has a line, its
/// event, and whether it ended.
pub fn key_lines(isl: &Island) -> u64 {
    let mut h = DefaultHasher::new();
    (isl.id, isl.name, isl.lines.len()).hash(&mut h);
    for x in isl.lines {
        (&x.id, &x.label, &x.kind, x.by.is_some(), x.ended.is_some(), &x.state, x.archived, x.last, &x.parent, &x.agents, &x.started).hash(&mut h);
    }
    isl.tickets.len().hash(&mut h);
    for t in isl.tickets.values() {
        (&t.id, &t.kind, &t.parent, &t.under, &t.unanswered_owner, t.broken.is_some(), t.closed_at.is_some()).hash(&mut h);
        let named: Vec<(&str, u32)> = t.mentioned.iter().map(|m| (m.by.as_str(), m.post)).collect();
        let his: Vec<(u32, &str)> = t.posts.iter().filter(|p| p.by == "owner").map(|p| (p.n, p.at.as_str())).collect();
        // his asks' rings and an agent's ask of him ([`Lit`]): an answer coming, an ask closing, lays again
        let asked: Vec<(u32, &str, Option<&str>)> = t.asked.iter().map(|a| (a.post, a.agent.as_str(), a.answer.as_deref())).collect();
        let to_him: Vec<u32> = t.posts.iter().filter(|p| crate::threads::open_to_owner(t, p)).map(|p| p.n).collect();
        (named, his, asked, to_him).hash(&mut h);
        let ran: Vec<(&str, Option<&str>, bool, &str, bool)> = t.ran.iter().map(|r| (r.id.as_str(), r.from.as_deref(), r.line.is_some(), r.event.as_str(), r.closed_at.is_some())).collect();
        ran.hash(&mut h);
    }
    h.finish()
}

/// [`key`]'s part of AGENTS: what an agent's event can change.
pub fn key_agents(agents: &[AgentView]) -> u64 {
    let mut h = DefaultHasher::new();
    agents.len().hash(&mut h);
    for a in agents {
        (&a.name, &a.state, &a.model, &a.on, &a.items, a.limit_until, &a.limit_why, a.account_waiting, &a.account_from, a.account_until).hash(&mut h);
    }
    h.finish()
}

/// [`key`]'s part of the accounts answer A: what an answer can change.
pub fn key_accounts(a: Option<&NativeAccounts>) -> u64 {
    let mut h = DefaultHasher::new();
    let window = |w: &Option<LimitWindow>| w.as_ref().map(|w| (w.used_percent.to_bits(), w.resets_at));
    a.map(|n| (n.gateway, n.accounts.len())).hash(&mut h);
    for a in a.map_or(&[][..], |n| &n.accounts[..]) {
        (&a.id, &a.label, a.tier, window(&a.five_hour), window(&a.seven_day), a.out_until, &a.out_for).hash(&mut h);
    }
    h.finish()
}

/// The first time after NOW that one of his posts in TICKETS lit for being of the last day (`build`'s lit: open, or
/// of the last day) is lit no more: the relay's last-day boundary (proto-lead N-1479 p124). His open posts stay lit
/// whatever their time.
pub fn last_day_ends(tickets: &BTreeMap<u64, TicketView>, now: i64) -> Option<i64> {
    tickets
        .values()
        .flat_map(|t| t.posts.iter().filter(|p| p.by == "owner" && !t.unanswered_owner.contains(&p.n)))
        .filter_map(|p| posted(&p.at))
        .map(|at| at + DAY + 1)
        .filter(|end| *end > now)
        .min()
}

/// A post's time AT, in seconds; none when it does not read.
fn posted(at: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(at).ok().map(|t| t.timestamp())
}

/// ISLANDS laid out at NOW (seconds), each in the places KEPT holds for it, KEPT given every place it lacked; and
/// whether KEPT changed (the caller writes it then). An island is refused only by a fault of this module: each place
/// is made inside its parent. Run when an island's [`key`] changes, or NOW passes a time a state laid changes at, not
/// at each frame: it takes milliseconds at the record's size and compares the whole of KEPT (proto-window N-1479 p29).
pub fn lay(islands: &[Island], kept: &mut Kept, now: i64) -> (Vec<Result<Laid, Refused>>, bool) {
    let before = kept.clone();
    kept.shape = SHAPE;
    let mut out = Vec::with_capacity(islands.len());
    // the right edge of the islands drawn so far, kept or not: two new ones in one call never share a place
    let mut next = 0.0_f64;
    for isl in islands {
        let ctx = Ctx::new(isl);
        let fresh = !kept.islands.contains_key(isl.id);
        if fresh {
            let x = kept.islands.values().map(|k| k.rect[0] + k.rect[2] + ISLAND_GAP).fold(next, f64::max);
            let mut k = KeptIsland { rect: [x, 0.0, 0.0, 0.0], ..Default::default() };
            place(&ctx, &mut k, true);
            next = next.max(k.rect[0] + k.rect[2] + ISLAND_GAP);
            out.push(build(&ctx, &k, now));
            // an island whose lines have not come yet is drawn where it would go, and kept only once they have: its
            // width is decided by them
            if !isl.lines.is_empty() {
                kept.islands.insert(isl.id.to_string(), k);
            }
            continue;
        }
        let k = kept.islands.get_mut(isl.id).expect("held");
        place(&ctx, k, false);
        next = next.max(k.rect[0] + k.rect[2] + ISLAND_GAP);
        out.push(build(&ctx, k, now));
    }
    let changed = *kept != before;
    (out, changed)
}

/// What an island's lines say, looked up once.
struct Ctx<'a> {
    isl: &'a Island<'a>,
    by_id: HashMap<&'a str, &'a Thread>,
    tickets: HashMap<&'a str, &'a TicketView>,
    /// The lines some line stands under.
    above: HashSet<&'a str>,
    /// Each agent's weight, `ln(lines / its lines)`: an agent on every line weighs nothing.
    idf: HashMap<&'a str, f64>,
    /// Each agent's lines ([`agents`]): how many, and its latest.
    lines_of: HashMap<&'a str, (usize, &'a Thread)>,
    /// The item each line's opening post names (the module doc), by the line's id.
    named: HashMap<&'a str, &'a Thread>,
    /// The handlers that ran ([`handled`]), each as a line ([`handler_line`]), in Programs.
    handlers: Vec<Thread>,
}

impl<'a> Ctx<'a> {
    fn new(isl: &'a Island<'a>) -> Ctx<'a> {
        let by_id: HashMap<&str, &Thread> = isl.lines.iter().map(|x| (x.id.as_str(), x)).collect();
        let tickets = isl.tickets.values().map(|t| (t.id.as_str(), t)).collect();
        let above = isl.lines.iter().filter_map(|x| x.parent.as_deref()).filter(|p| by_id.contains_key(p)).collect();
        let mut lines_of: HashMap<&str, (usize, &Thread)> = HashMap::new();
        for x in isl.lines {
            for a in agents(x) {
                let e = lines_of.entry(a).or_insert((0, x));
                e.0 += 1;
                e.1 = x;
            }
        }
        let n = isl.lines.len().max(1) as f64;
        let idf = lines_of.iter().map(|(a, (c, _))| (*a, (n / *c as f64).ln())).collect();
        // from each item's view the lines whose post 1 names it, the item of the lowest number first
        let mut named: HashMap<&str, &Thread> = HashMap::new();
        for t in isl.tickets.values().filter(|t| t.kind == "item") {
            let Some(item) = by_id.get(t.id.as_str()).copied() else { continue };
            for m in t.mentioned.iter().filter(|m| m.post == 1 && m.by != t.id) {
                if let Some(x) = by_id.get(m.by.as_str()) {
                    named.entry(x.id.as_str()).or_insert(item);
                }
            }
        }
        let handlers = handled(isl.tickets).iter().map(handler_line).collect();
        Ctx { isl, by_id, tickets, above, idf, lines_of, named, handlers }
    }

    /// The line at the top of X's chain of parents (X when it stands under none); a loop ends the walk.
    fn root<'b>(&'b self, x: &'b Thread) -> &'b Thread {
        let mut at = x;
        for _ in 0..self.isl.lines.len() {
            match at.parent.as_deref().and_then(|p| self.by_id.get(p)) {
                Some(up) => at = up,
                None => break,
            }
        }
        at
    }

    /// The line ticket ID counts for: itself when it is one, else the nearest it runs or stands under.
    fn line_of(&self, id: &str) -> Option<&'a Thread> {
        let mut at = id.to_string();
        for _ in 0..32 {
            if let Some(x) = self.by_id.get(at.as_str()) {
                return Some(*x);
            }
            let t = self.tickets.get(at.as_str())?;
            at = t.under.clone().or_else(|| t.parent.clone())?;
        }
        None
    }

    /// The item X's opening post names, as the items' views list the posts that name them (the module doc).
    fn named_item(&self, x: &Thread) -> Option<&'a Thread> {
        self.named.get(x.id.as_str()).copied()
    }
}

/// X's agents: given anything on it, and an item's starters; each once.
fn agents(x: &Thread) -> BTreeSet<&str> {
    x.agents.iter().chain(&x.started).map(String::as_str).collect()
}

fn rect(a: [f64; 4]) -> Rect {
    Rect::new(a[0], a[1], a[2], a[3])
}

fn arr(r: Rect) -> [f64; 4] {
    [r.x, r.y, r.w, r.h]
}

/// Whether A and B are at least GAP apart.
fn apart(a: &Rect, b: &Rect, gap: f64) -> bool {
    a.right() + gap <= b.x || b.right() + gap <= a.x || a.bottom() + gap <= b.y || b.bottom() + gap <= a.y
}

/// X's rows for its posts now, when it is placed: for twice its posts, at least [`ROWS_LEAST`], a power of two in one
/// column up to [`ROWS_MOST`]; past that, as many columns of [`ROWS_MOST`] as they take.
fn rows_for(x: &Thread) -> Rows {
    let want = (2 * u64::from(x.last.unwrap_or(0))).max(u64::from(ROWS_LEAST));
    match u32::try_from(want) {
        Ok(w) if w <= ROWS_MOST => Rows { rows: w.next_power_of_two(), cols: 1 },
        _ => Rows { rows: ROWS_MOST, cols: u32::try_from(want.div_ceil(u64::from(ROWS_MOST))).unwrap_or(u32::MAX) },
    }
}

/// X kept at C in region T, with its rows.
fn kept_line(x: &Thread, c: Rect, t: &str) -> KeptLine {
    let s = rows_for(x);
    KeptLine { rect: arr(c), topic: t.to_string(), rows: s.rows, cols: s.cols }
}

/// The room a line at R with rows S holds: its card, and the room [`fill_lines`] grows it into next, as much again
/// under it while it has one column short of [`ROWS_MOST`], else a column beside it.
fn claim(r: Rect, s: Rows) -> Rect {
    if s.cols == 1 && s.rows < ROWS_MOST {
        Rect::new(r.x, r.y, r.w, 2.0 * r.h)
    } else {
        Rect::new(r.x, r.y, r.w + LINE_W, r.h)
    }
}

/// X's card for its posts now: its rows ([`rows_for`]), each [`LINE_W`] by [`ROW_H`]. Every place kept is in whole
/// world units (a power of two of [`ROW_H`] from [`ROWS_LEAST`] on is), so the file reads back exactly what was written
/// (serde_json's own reading of a float may differ in its last bit) and a card kept inside its region stays so.
fn card(x: &Thread) -> (f64, f64) {
    let s = rows_for(x);
    (LINE_W * f64::from(s.cols), ROW_H * f64::from(s.rows))
}

/// The room X's card holds when it is placed ([`claim`]), as a width and a height.
fn claimed(x: &Thread) -> (f64, f64) {
    let (w, h) = card(x);
    let c = claim(Rect::new(0.0, 0.0, w, h), rows_for(x));
    (c.w, c.h)
}

/// The region a topic goes on from (itself, but for a region beside one).
fn base_of(k: &KeptIsland, t: &str) -> String {
    k.topics.get(t).and_then(|x| x.of.clone()).unwrap_or_else(|| t.to_string())
}

/// The agents of each topic's lines, and how many lines: what way 2 and the topics' nearness weigh.
#[derive(Default)]
struct Profile<'a> {
    lines: usize,
    on: BTreeMap<&'a str, usize>,
}

impl<'a> Profile<'a> {
    fn add(&mut self, x: &'a Thread) {
        self.lines += 1;
        for a in agents(x) {
            *self.on.entry(a).or_default() += 1;
        }
    }
    fn share(&self, a: &str) -> f64 {
        self.on.get(a).map_or(0.0, |c| *c as f64 / self.lines.max(1) as f64)
    }
}

/// How near two topics are: their agents, each by its weight and the lesser of its shares.
fn near(ctx: &Ctx, a: &Profile, b: &Profile) -> f64 {
    a.on.keys().filter(|x| b.on.contains_key(*x)).map(|x| ctx.idf.get(x).copied().unwrap_or(0.0) * a.share(x).min(b.share(x))).sum()
}

/// A way-2 topic's name: the agents it is named by.
fn with_label(names: &[String]) -> String {
    match names.len() {
        0 => "given to no agent".into(),
        1..=3 => format!("with {}", names.join(", ")),
        n => format!("with {} +{}", names[..3].join(", "), n - 3),
    }
}

/// Every line of CTX given its place in K, those K holds kept as they are (FRESH: K is a new island, placed whole).
fn place<'a>(ctx: &'a Ctx<'a>, k: &mut KeptIsland, fresh: bool) {
    // a kept line whose region is gone from the file is placed again
    let regions: BTreeSet<String> = k.topics.keys().cloned().collect();
    k.lines.retain(|_, l| regions.contains(&l.topic));
    fill_lines(ctx, k);
    // each new line's topic, by the module doc's ways, in the order of their numbers; each handler's, Programs
    let mut topic: HashMap<&str, String> = HashMap::new();
    let mut profiles: BTreeMap<String, Profile<'a>> = BTreeMap::new();
    let mut lone: Vec<&'a Thread> = Vec::new();
    for (j, x) in ctx.isl.lines.iter().chain(&ctx.handlers).enumerate() {
        let t = match k.lines.get(&x.id) {
            Some(l) => Some(base_of(k, &l.topic)),
            None if j >= ctx.isl.lines.len() => Some(PROGRAMS.to_string()),
            None => new_topic_of(ctx, k, x),
        };
        match t {
            Some(t) => {
                profiles.entry(t.clone()).or_default().add(x);
                topic.insert(&x.id, t);
            }
            None => lone.push(x),
        }
    }
    // way 2: the topic whose lines its agents are on, else the one named by its very agents
    let mut by_with: BTreeMap<Vec<String>, String> = k.topics.iter().filter_map(|(id, t)| t.with.clone().map(|w| (w, base_of(k, id)))).collect();
    let mut labels: BTreeMap<String, (String, Option<Vec<String>>)> = BTreeMap::new();
    for x in lone {
        let ag = agents(x);
        let mut best: Option<(f64, &String)> = None;
        for (t, p) in &profiles {
            let score: f64 = ag.iter().map(|a| ctx.idf.get(a).copied().unwrap_or(0.0) * p.share(a)).sum();
            if score >= JOIN && best.is_none_or(|(s, _)| score > s) {
                best = Some((score, t));
            }
        }
        let t = match best {
            Some((_, t)) => t.clone(),
            None => {
                let names: Vec<String> = ag.iter().map(|a| a.to_string()).collect();
                by_with
                    .entry(names.clone())
                    .or_insert_with(|| {
                        let t = format!("topic:{}", x.id);
                        labels.insert(t.clone(), (with_label(&names), Some(names)));
                        t
                    })
                    .clone()
            }
        };
        profiles.entry(t.clone()).or_default().add(x);
        topic.insert(&x.id, t);
    }
    // the new lines, by topic: into a region kept, else into a new one
    let mut new: BTreeMap<String, Vec<&'a Thread>> = BTreeMap::new();
    for x in ctx.isl.lines.iter().chain(&ctx.handlers).filter(|x| !k.lines.contains_key(&x.id)) {
        new.entry(topic[x.id.as_str()].clone()).or_default().push(x);
    }
    labels.entry(PROGRAMS.to_string()).or_insert_with(|| ("Programs".into(), None));
    let mut regions_new: Vec<(String, String, Vec<&'a Thread>)> = Vec::new();
    for (t, xs) in new {
        let label = labels
            .get(&t)
            .cloned()
            .or_else(|| k.topics.get(&t).map(|x| (x.label.clone(), x.with.clone())))
            .unwrap_or_else(|| (ctx.by_id.get(t.trim_start_matches("topic:")).map_or_else(|| t.clone(), |x| x.label.clone()), None));
        let last = k.topics.keys().filter(|r| base_of(k, r) == t).max_by_key(|r| region_n(r)).cloned();
        let Some(mut at) = last else {
            regions_new.push((t.clone(), t, xs));
            labels.entry(regions_new.last().unwrap().0.clone()).or_insert(label);
            continue;
        };
        let mut rest = Vec::new();
        for x in xs {
            if !rest.is_empty() || !into_region(k, &at, x) {
                rest.push(x);
            }
        }
        if !rest.is_empty() {
            let n = k.topics.keys().filter(|r| base_of(k, r) == t).map(|r| region_n(r)).max().unwrap_or(1) + 1;
            at = format!("{t}~{n}");
            labels.insert(at.clone(), label);
            regions_new.push((at, t, rest));
        }
    }
    // the new regions, the largest first, each in the free room nearest those it shares agents with
    let mut sized: Vec<(String, String, Vec<&'a Thread>, Rect, Vec<[f64; 2]>)> = Vec::new();
    if fresh {
        // a new island as wide as the room its regions want, with some left over, and no narrower than the widest
        let alone: Vec<Rect> = regions_new.iter().map(|(_, _, xs)| region_size(xs, None).0).collect();
        let total: f64 = alone.iter().map(|r| (r.w + TOPIC_GAP) * (r.h + TOPIC_GAP)).sum();
        let widest = alone.iter().map(|r| r.w).fold(0.0, f64::max);
        k.rect[2] = ((total * ISLAND_ROOM).sqrt().ceil().max(widest) + 2.0 * ISLAND_PAD).max(ISLAND_LEAST_W);
        k.rect[3] = ISLAND_HEAD + ISLAND_PAD;
    }
    for (id, base, xs) in regions_new {
        let (r, offs) = region_size(&xs, Some(k.rect[2] - 2.0 * ISLAND_PAD));
        sized.push((id, base, xs, r, offs));
    }
    sized.sort_by(|a, b| (b.3.w * b.3.h).total_cmp(&(a.3.w * a.3.h)).then_with(|| a.0.cmp(&b.0)));
    for (id, base, xs, r, offs) in sized {
        let mine = profiles.get(&base);
        let mut sum = 0.0;
        let mut at = [0.0, 0.0];
        for (o, t) in &k.topics {
            let ob = base_of(k, o);
            // a region beside the topic it goes on from is placed nearest it
            let w = match (mine, profiles.get(&ob)) {
                _ if ob == base => 4.0,
                (Some(a), Some(b)) => near(ctx, a, b),
                _ => 0.0,
            };
            if w > 0.0 {
                let c = rect(t.rect).mid();
                at = [at[0] + w * c[0], at[1] + w * c[1]];
                sum += w;
            }
        }
        let goal = (sum > 0.0).then(|| [at[0] / sum, at[1] / sum]);
        let r = free_room(k, r.w, r.h, goal);
        let (label, with) = labels.remove(&id).unwrap_or_default();
        k.topics.insert(id.clone(), KeptTopic { rect: arr(r), label, of: (id != base).then_some(base), with });
        for (x, o) in xs.iter().zip(offs) {
            let (w, h) = card(x);
            k.lines.insert(x.id.clone(), kept_line(x, Rect::new(r.x + TOPIC_PAD + o[0], r.y + TOPIC_HEAD + o[1], w, h), &id));
        }
        let bottom = r.bottom() + ISLAND_PAD;
        if bottom > k.rect[1] + k.rect[3] {
            k.rect[3] = bottom - k.rect[1];
        }
    }
    if fresh {
        // room under the topics for those still to come
        let used = k.rect[3] - ISLAND_HEAD - ISLAND_PAD;
        k.rect[3] += (used * (ISLAND_ROOM - 1.0)).ceil();
    }
}

/// The number of a region beside the topic it goes on from (`topic:I-1~3`: 3; the topic's own: 1).
fn region_n(r: &str) -> u32 {
    r.rsplit_once('~').and_then(|(_, n)| n.parse().ok()).unwrap_or(1)
}

/// The topic a new line X stands in by way 1: that of the nearest line kept above it, else that of the top of its
/// chain, an item, or a thread with lines under it, else that of an item its opening post names. None: way 2's.
fn new_topic_of(ctx: &Ctx, k: &KeptIsland, x: &Thread) -> Option<String> {
    let mut up = x.parent.as_deref();
    for _ in 0..ctx.isl.lines.len() {
        let Some(p) = up.and_then(|p| ctx.by_id.get(p)) else { break };
        if let Some(l) = k.lines.get(&p.id) {
            return Some(base_of(k, &l.topic));
        }
        up = p.parent.as_deref();
    }
    let r = ctx.root(x);
    if r.id != x.id || x.kind == "item" || ctx.above.contains(x.id.as_str()) {
        return Some(format!("topic:{}", r.id));
    }
    let item = ctx.named_item(x)?;
    Some(match k.lines.get(&item.id) {
        Some(l) => base_of(k, &l.topic),
        None => format!("topic:{}", ctx.root(item).id),
    })
}

/// A new region's rect (at 0, 0) for lines XS in rows by their numbers, each with the room its card holds
/// ([`claim`]), at most MOST wide inside, with the room left for more under them; each card's place inside its content.
fn region_size(xs: &[&Thread], most: Option<f64>) -> (Rect, Vec<[f64; 2]>) {
    let cards: Vec<(f64, f64)> = xs.iter().map(|x| claimed(x)).collect();
    let widest = cards.iter().map(|c| c.0).fold(0.0, f64::max);
    let area: f64 = cards.iter().map(|(w, h)| (w + GAP) * (h + GAP)).sum();
    let mut cw = (area * TOPIC_ROOM).sqrt().ceil().max(2.0 * (LINE_W + GAP)).max(widest);
    if let Some(m) = most {
        cw = cw.min((m - 2.0 * TOPIC_PAD).max(widest));
    }
    let (mut x, mut y, mut row) = (0.0, 0.0, 0.0_f64);
    let mut offs = Vec::with_capacity(cards.len());
    for (w, h) in cards {
        if x > 0.0 && x + w > cw {
            x = 0.0;
            y += row + GAP;
            row = 0.0;
        }
        offs.push([x, y]);
        x += w + GAP;
        row = row.max(h);
    }
    let used = y + row;
    let room = (used * (TOPIC_ROOM - 1.0)).ceil().max(2.0 * LINE_H + GAP);
    (Rect::new(0.0, 0.0, cw + 2.0 * TOPIC_PAD, TOPIC_HEAD + used + room + TOPIC_PAD), offs)
}

/// The free room in island K nearest GOAL for a region W by H, apart from every region there (none: the highest,
/// then the leftmost); under them all when none is free, where the island grows.
fn free_room(k: &KeptIsland, w: f64, h: f64, goal: Option<[f64; 2]>) -> Rect {
    let isl = rect(k.rect);
    let (left, top, right) = (isl.x + ISLAND_PAD, isl.y + ISLAND_HEAD, isl.right() - ISLAND_PAD);
    let placed: Vec<Rect> = k.topics.values().map(|t| rect(t.rect)).collect();
    if let Some(r) = spot(&placed, [left, top, right, f64::INFINITY], w, h, TOPIC_GAP, goal) {
        return r;
    }
    let y = placed.iter().map(|p| p.bottom() + TOPIC_GAP).fold(top, f64::max);
    Rect::new(left, y, w, h)
}

/// The place for a W by H rect inside BOUNDS (left, top, right, bottom), GAP apart from each of PLACED: the first
/// free one beside or under a placed one, nearest GOAL (none: the highest, then the leftmost).
fn spot(placed: &[Rect], bounds: [f64; 4], w: f64, h: f64, gap: f64, goal: Option<[f64; 2]>) -> Option<Rect> {
    let [left, top, right, bottom] = bounds;
    let mut at = vec![[left, top]];
    for p in placed {
        at.push([p.right() + gap, p.y]);
        at.push([p.x, p.bottom() + gap]);
        at.push([left, p.bottom() + gap]);
    }
    let fits = |[x, y]: [f64; 2]| x >= left - 1e-9 && y >= top - 1e-9 && x + w <= right + 1e-9 && y + h <= bottom + 1e-9;
    let cost = |[x, y]: [f64; 2]| match goal {
        Some([gx, gy]) => ((x + w / 2.0 - gx).powi(2) + (y + h / 2.0 - gy).powi(2)).sqrt() + 0.25 * (y - top),
        None => (y - top) + (x - left) * 1e-3,
    };
    let mut at: Vec<([f64; 2], f64)> = at.into_iter().filter(|a| fits(*a)).map(|a| (a, cost(a))).collect();
    at.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0[1].total_cmp(&b.0[1])).then(a.0[0].total_cmp(&b.0[0])));
    at.into_iter().map(|([x, y], _)| Rect::new(x, y, w, h)).find(|c| placed.iter().all(|p| apart(p, c, gap)))
}

/// The room each of region T's lines in K holds ([`claim`]), but SKIP's.
fn siblings(k: &KeptIsland, t: &str, skip: &str) -> Vec<Rect> {
    k.lines
        .iter()
        .filter(|(id, l)| l.topic == t && id.as_str() != skip)
        .map(|(_, l)| claim(rect(l.rect), Rows { rows: l.rows, cols: l.cols }))
        .collect()
}

/// The content of region R: left, top, right, bottom.
fn content(r: Rect) -> [f64; 4] {
    [r.x + TOPIC_PAD, r.y + TOPIC_HEAD, r.right() - TOPIC_PAD, r.bottom() - TOPIC_PAD]
}

/// New line X put in free room in region T of K, the region grown down into free room when it has none; false when
/// it cannot be (the line goes on in a region beside it).
fn into_region(k: &mut KeptIsland, t: &str, x: &Thread) -> bool {
    let (w, h) = card(x);
    let (cw, ch) = claimed(x);
    let r = rect(k.topics[t].rect);
    // each line with the room its rows grow into
    let held = siblings(k, t, "");
    if let Some(c) = spot(&held, content(r), cw, ch, GAP, None) {
        k.lines.insert(x.id.clone(), kept_line(x, Rect::new(c.x, c.y, w, h), t));
        return true;
    }
    // grown down: under its lowest line, with room for one more under it
    let low = held.iter().map(|s| s.bottom() + GAP).fold(r.y + TOPIC_HEAD, f64::max);
    let grown = Rect::new(r.x, r.y, r.w, (low + ch + GAP + 2.0 * LINE_H + TOPIC_PAD - r.y).max(r.h));
    let others_apart = k.topics.iter().filter(|(id, _)| id.as_str() != t).all(|(_, o)| apart(&rect(o.rect), &grown, TOPIC_GAP));
    if cw > r.w - 2.0 * TOPIC_PAD || !others_apart {
        return false;
    }
    let Some(c) = spot(&held, content(grown), cw, ch, GAP, None) else { return false };
    k.topics.get_mut(t).expect("a region of K").rect = arr(grown);
    k.lines.insert(x.id.clone(), kept_line(x, Rect::new(c.x, c.y, w, h), t));
    let bottom = grown.bottom() + ISLAND_PAD;
    if bottom > k.rect[1] + k.rect[3] {
        k.rect[3] = bottom - k.rect[1];
    }
    true
}

/// Each kept line whose posts have filled its rows takes more at its place, so that no post moves (docket-edge6
/// N-1479 p23): twice its rows, its card as much taller, into the room under it where that is free in its region and
/// it has one column short of [`ROWS_MOST`]; else one more column, its card a column wider; until its posts have rows,
/// or neither is free, and it is full. Free is apart from the room each other line of its region holds ([`claim`]).
/// Once it has gone wider it only goes wider: its rows doubled then would take each later column's posts into the one
/// before it (proto-window N-1479 p29).
fn fill_lines(ctx: &Ctx, k: &mut KeptIsland) {
    for x in ctx.isl.lines.iter().chain(&ctx.handlers) {
        let posts = x.last.unwrap_or(0);
        let Some(l) = k.lines.get(&x.id) else { continue };
        if u64::from(posts) <= u64::from(l.rows) * u64::from(l.cols) {
            continue;
        }
        let mut l = l.clone();
        let [left, top, right, bottom] = content(rect(k.topics[&l.topic].rect));
        let others = siblings(k, &l.topic, &x.id);
        let free = |c: &Rect| c.x >= left && c.y >= top && c.right() <= right && c.bottom() <= bottom && others.iter().all(|s| apart(s, c, GAP));
        while u64::from(posts) > u64::from(l.rows) * u64::from(l.cols) {
            let r = rect(l.rect);
            let down = Rect::new(r.x, r.y, r.w, 2.0 * r.h);
            let wider = Rect::new(r.x, r.y, r.w + r.w / l.cols as f64, r.h);
            if l.cols == 1 && l.rows < ROWS_MOST && free(&down) {
                l.rect = arr(down);
                l.rows *= 2;
            } else if free(&wider) {
                l.rect = arr(wider);
                l.cols += 1;
            } else {
                break;
            }
        }
        k.lines.insert(x.id.clone(), l);
    }
}

/// What X's colour says.
fn line_state(x: &Thread) -> State {
    match x.state.as_str() {
        _ if x.archived => State::Ended,
        "waits on you" => State::WaitsOnHim,
        "at work" => State::Working,
        s if s.starts_with("waits on ") => State::Working,
        "quiet" => State::Idle,
        "failed" => State::Stopped,
        _ if x.ended.is_some() => State::Ended,
        _ => State::Plain,
    }
}

/// X's tier (its ticket T, when the snapshot holds it): his open ask on it, what waits on him, failed or broken, the
/// rest. An archived line is out of his lists, and the rest.
fn line_tier(x: &Thread, t: Option<&TicketView>) -> Tier {
    if x.archived {
        Tier::Rest
    } else if t.is_some_and(|t| !t.unanswered_owner.is_empty()) {
        Tier::HisAsk
    } else if x.state == "waits on you" {
        Tier::WaitsOnHim
    } else if x.state == "failed" || t.is_some_and(|t| t.broken.is_some()) {
        Tier::Stopped
    } else {
        Tier::Rest
    }
}

/// What A's dot says at NOW: held on a limit (`limit_until` still ahead, or waiting with every account out), working,
/// stopped, else idle.
fn agent_state(a: &AgentView, now: i64) -> State {
    if a.limit_until.is_some_and(|t| t > now) || a.account_waiting {
        State::Held
    } else if a.state == "working" {
        State::Working
    } else if a.state == "stopped" {
        State::Stopped
    } else {
        State::Idle
    }
}

/// A region's state from its lines': waits on him, else stopped, else at work, else all ended, else quiet.
fn region_state(states: &[State]) -> State {
    let any = |s: State| states.contains(&s);
    if any(State::WaitsOnHim) {
        State::WaitsOnHim
    } else if any(State::Stopped) {
        State::Stopped
    } else if any(State::Working) {
        State::Working
    } else if !states.is_empty() && states.iter().all(|s| *s == State::Ended) {
        State::Ended
    } else {
        State::Idle
    }
}

/// Island CTX's nodes and links in its places K, at NOW: its project, its regions (but those with none of its lines
/// now), its lines, its agents, its corner; and what its far levels draw.
fn build(ctx: &Ctx, k: &KeptIsland, now: i64) -> Result<Laid, Refused> {
    let isl = rect(k.rect);
    let mut nodes = vec![Node::new(format!("project:{}", ctx.isl.id), Kind::Project, ctx.isl.name, None, isl)];
    let mut edges = Vec::new();
    // the regions with lines drawn now, in the order of their ids
    let drawn: BTreeSet<&str> = ctx.isl.lines.iter().chain(&ctx.handlers).filter_map(|x| k.lines.get(&x.id)).map(|l| l.topic.as_str()).collect();
    let mut region: HashMap<&str, u32> = HashMap::new();
    for (id, t) in k.topics.iter().filter(|(id, _)| drawn.contains(id.as_str())) {
        region.insert(id, nodes.len() as u32);
        let label = match t.with {
            Some(_) => t.label.clone(),
            None => ctx.by_id.get(base_of(k, id).trim_start_matches("topic:")).map_or_else(|| t.label.clone(), |x| x.label.clone()),
        };
        nodes.push(Node::new(id.clone(), Kind::Topic, label, Some(0), rect(t.rect)));
    }
    // the lines, then the handlers in Programs, each with its rows (a line's for its posts, a handler's for its runs)
    let mut line: HashMap<&str, u32> = HashMap::new();
    let mut rows = BTreeMap::new();
    let mut states: BTreeMap<u32, Vec<State>> = BTreeMap::new();
    for (j, x) in ctx.isl.lines.iter().chain(&ctx.handlers).enumerate() {
        let Some(l) = k.lines.get(&x.id) else { continue };
        let up = region[l.topic.as_str()];
        let t = ctx.tickets.get(x.id.as_str()).copied();
        let kind = if j >= ctx.isl.lines.len() {
            Kind::Handler
        } else if x.kind == "item" {
            Kind::Item
        } else {
            Kind::Thread
        };
        let mut n = Node::new(x.id.clone(), kind, x.label.clone(), Some(up), rect(l.rect));
        n.state = line_state(x);
        n.tier = line_tier(x, t);
        n.weight = x.last.unwrap_or(0) as f32;
        n.his = kind != Kind::Handler && x.by.is_none();
        states.entry(up).or_default().push(n.state);
        rows.insert(nodes.len() as u32, Rows { rows: l.rows, cols: l.cols });
        line.insert(&x.id, nodes.len() as u32);
        nodes.push(n);
    }
    // a line's links: to the line it stands under (an item: it names it), and to an item its opening post names
    for x in ctx.isl.lines {
        let Some(&i) = line.get(x.id.as_str()) else { continue };
        if let Some(&p) = x.parent.as_deref().and_then(|p| line.get(p)) {
            edges.push(Edge::new(i, p, if nodes[p as usize].kind == Kind::Item { Link::Names } else { Link::Branch }));
        }
        if x.parent.is_none()
            && let Some(&it) = ctx.named_item(x).and_then(|y| line.get(y.id.as_str()))
        {
            edges.push(Edge::new(i, it, Link::Names));
        }
    }
    // a region and the island take their lines' highest tier and the sum of their weights
    for (&r, s) in &states {
        let lines: Vec<&Node> = nodes.iter().filter(|n| n.parent == Some(r)).collect();
        let tier = lines.iter().map(|n| n.tier).min().unwrap_or_default();
        let weight = lines.iter().map(|n| n.weight).sum();
        let n = &mut nodes[r as usize];
        n.state = region_state(s);
        n.tier = tier;
        n.weight = weight;
    }
    // the agents, each on the region of the line it is on, else of an item it works on, else of its latest line
    let mut sorted: Vec<&AgentView> = ctx.isl.agents.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let mut on: BTreeMap<Option<u32>, Vec<(&AgentView, Option<u32>)>> = BTreeMap::new();
    for a in sorted {
        let at = a.on.as_deref().and_then(|id| ctx.line_of(id)).or_else(|| a.items.iter().find_map(|i| ctx.by_id.get(i.as_str()).copied()));
        let latest = || ctx.lines_of.get(a.name.as_str()).map(|(_, x)| *x);
        let at_line = at.and_then(|x| line.get(x.id.as_str()).copied());
        let r = at.or_else(latest).and_then(|x| k.lines.get(&x.id)).and_then(|l| region.get(l.topic.as_str()).copied());
        on.entry(r).or_default().push((a, at_line));
    }
    let mut held: BTreeMap<(String, String), Cluster> = BTreeMap::new();
    let mut badge = Badge::default();
    let mut wanted: BTreeSet<String> = BTreeSet::new();
    for (r, list) in &on {
        let (row, y) = match r {
            Some(r) => (nodes[*r as usize].rect, nodes[*r as usize].rect.y + DOTS_Y),
            None => (Rect::new(isl.x, isl.y, isl.w / 2.0, ISLAND_HEAD), isl.y + ISLAND_DOTS_Y),
        };
        let pad = if r.is_some() { TOPIC_PAD } else { ISLAND_PAD };
        let step = ((row.w - 2.0 * pad) / list.len() as f64).min(DOT + 8.0);
        let size = (step * DOT / (DOT + 8.0)).min(DOT);
        for (j, (a, at_line)) in list.iter().enumerate() {
            let i = nodes.len() as u32;
            let mut n = Node::new(format!("agent:{}", a.name), Kind::Agent, a.name.clone(), Some(r.unwrap_or(0)), Rect::new(row.x + pad + j as f64 * step, y, size, size));
            n.state = agent_state(a, now);
            n.tier = if matches!(n.state, State::Held | State::Stopped) { Tier::Stopped } else { Tier::Rest };
            n.weight = ctx.lines_of.get(a.name.as_str()).map_or(0, |(c, _)| *c) as f32;
            match n.state {
                State::Held => badge.held += 1,
                State::Stopped => badge.stopped += 1,
                _ => {}
            }
            if let Some(l) = at_line {
                edges.push(Edge::new(i, *l, Link::Worked));
            }
            if n.state == State::Held {
                let (holder, until, why) = if a.limit_until.is_some_and(|t| t > now) {
                    ("account:gateway".to_string(), a.limit_until, a.limit_why.clone())
                } else {
                    (a.account_from.as_ref().map(|f| format!("account:{f}")).unwrap_or_default(), a.account_until, None)
                };
                wanted.insert(holder.clone());
                let c = held.entry((holder, a.model.clone())).or_insert_with(|| Cluster { on: None, agents: Vec::new(), model: a.model.clone(), until: None, why: None });
                c.agents.push(i);
                c.until = c.until.max(until);
                c.why = c.why.take().or(why);
            }
            nodes.push(n);
        }
    }
    // the corner: his accounts by their priority, then the gateway when the coord has it or an agent is held on it
    let mut accounts: Vec<&coord_types::NativeAccount> = ctx.isl.accounts.map(|a| a.accounts.iter().collect()).unwrap_or_default();
    accounts.sort_by(|a, b| b.tier.cmp(&a.tier).then(a.id.cmp(&b.id)));
    let gateway = ctx.isl.accounts.is_some_and(|a| a.gateway) || wanted.contains("account:gateway");
    let count = accounts.len() + gateway as usize;
    let mw = ((isl.w / 2.0 - ISLAND_PAD) / count.max(1) as f64 - GAP).min(METER_W);
    let mut meters = Vec::new();
    let mut corner: HashMap<String, u32> = HashMap::new();
    let mut slot = 0;
    let at = |slot: usize| Rect::new(isl.right() - ISLAND_PAD - (slot + 1) as f64 * (mw + GAP) + GAP, isl.y + METERS_Y, mw, METER_H);
    for a in &accounts {
        let i = nodes.len() as u32;
        let mut n = Node::new(format!("account:{}", a.id), Kind::Account, if a.label.is_empty() { a.id.clone() } else { a.label.clone() }, Some(0), at(slot));
        slot += 1;
        let fuller = [&a.five_hour, &a.seven_day].into_iter().flatten().max_by(|x, y| x.used_percent.total_cmp(&y.used_percent));
        let out = a.out_until.filter(|t| *t > now);
        n.state = match a.out_for.as_deref() {
            Some("authentication_failed" | "billing_error") => State::Stopped,
            _ if out.is_some() => State::Held,
            _ => State::Plain,
        };
        n.tier = if n.state == State::Plain { Tier::Rest } else { Tier::Stopped };
        n.weight = fuller.map_or(0.0, |w| w.used_percent as f32 / 100.0);
        meters.push(Meter { node: i, used: fuller.map(|w| w.used_percent / 100.0), resets: fuller.map(|w| w.resets_at), out, why: out.and(a.out_for.clone()) });
        corner.insert(n.id.clone(), i);
        nodes.push(n);
    }
    if gateway && !corner.contains_key("account:gateway") {
        let i = nodes.len() as u32;
        let mut n = Node::new("account:gateway", Kind::Account, "Gateway", Some(0), at(slot));
        if wanted.contains("account:gateway") {
            n.state = State::Held;
            n.tier = Tier::Stopped;
        }
        meters.push(Meter { node: i, used: None, resets: None, out: None, why: None });
        corner.insert(n.id.clone(), i);
        nodes.push(n);
    }
    let mut held: Vec<Cluster> = held.into_iter().map(|((h, _), mut c)| {
        c.on = corner.get(&h).copied();
        c
    }).collect();
    held.sort_by(|a, b| b.agents.len().cmp(&a.agents.len()).then(a.model.cmp(&b.model)));
    // his posts, lit: those of the last day, and the older ones still open, each with the agents it asks; and an
    // agent's post that asks him, while it waits on him
    let mut lit = Vec::new();
    for x in ctx.isl.lines {
        let (Some(&i), Some(t)) = (line.get(x.id.as_str()), ctx.tickets.get(x.id.as_str())) else { continue };
        for p in &t.posts {
            let post = format!("{}.p{}", t.id, p.n);
            if p.by != "owner" {
                if crate::threads::open_to_owner(t, p) {
                    lit.push(Lit { line: i, post, open: true, asked: Vec::new(), to_him: true });
                }
                continue;
            }
            let open = t.unanswered_owner.contains(&p.n);
            let recent = posted(&p.at).is_some_and(|at| at >= now - DAY);
            if open || recent {
                let asked = t.asked.iter().filter(|a| a.post == p.n).map(|a| (a.agent.clone(), a.answer.clone())).collect();
                lit.push(Lit { line: i, post, open, asked, to_him: false });
            }
        }
    }
    // the badge: what waits on him, what is stopped or broken
    badge.waits = nodes.iter().filter(|n| matches!(n.kind, Kind::Thread | Kind::Item) && n.state == State::WaitsOnHim).count();
    badge.stopped += ctx.isl.tickets.values().filter(|t| t.closed_at.is_none() && t.broken.is_some()).count();
    // (not Programs': its runs are no posts, and would weigh with his posts in the island's rank)
    let regions: Vec<&Node> = nodes.iter().filter(|n| n.kind == Kind::Topic && !n.id.starts_with(PROGRAMS)).collect();
    let tier = regions.iter().map(|n| n.tier).min().unwrap_or_default();
    let weight = regions.iter().map(|n| n.weight).sum();
    let p = &mut nodes[0];
    p.tier = tier;
    p.weight = weight;
    p.state = if badge.waits > 0 {
        State::WaitsOnHim
    } else if badge.stopped > 0 {
        State::Stopped
    } else if badge.held > 0 {
        State::Held
    } else {
        State::Plain
    };
    Ok(Laid { graph: Graph::new(nodes, edges)?, rows, badge, held, meters, lit })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A line of the outline, made up: ID, its KIND, LABEL, the line it stands under, its agents, its last post.
    fn line(id: &str, kind: &str, parent: Option<&str>, agents: &[&str], last: u32) -> Thread {
        Thread {
            id: id.into(),
            label: format!("{id} label"),
            kind: kind.into(),
            opened: "2026-11-20T10:00:00Z".into(),
            by: None,
            ended: None,
            state: "quiet".into(),
            archived: false,
            last: Some(last),
            parent: parent.map(String::from),
            agents: agents.iter().map(|a| a.to_string()).collect(),
            started: Vec::new(),
        }
    }

    fn agent(name: &str, state: &str, on: Option<&str>) -> AgentView {
        let mut a: AgentView = serde_json::from_value(json!({
            "id": "", "name": name, "parent": "lead", "kind": "", "model": "opus", "state": state, "since": "2026-11-20T10:00:00Z",
            "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null
        }))
        .unwrap();
        a.on = on.map(String::from);
        a
    }

    /// An item's view in the snapshot, made up: ID, and the posts that name it, each (ticket, post).
    fn item(id: &str, named_by: &[(&str, u32)]) -> TicketView {
        let mut v = ticket(id, &[("docket-core", "2026-11-01T10:00:00Z")], &[], &[]);
        v.kind = "item".into();
        v.mentioned = serde_json::from_value(json!(named_by.iter().map(|(by, n)| json!({"by": by, "post": n, "label": by, "who": "owner", "at": "2026-11-01T10:00:00Z"})).collect::<Vec<_>>())).unwrap();
        v
    }

    /// A ticket of the snapshot, made up: ID with POSTS (each by, at) and the posts of his no later post answers.
    fn ticket(id: &str, posts: &[(&str, &str)], unanswered: &[u32], mentions: &[&str]) -> TicketView {
        let n: u64 = id.trim_start_matches(|c: char| !c.is_ascii_digit()).parse().unwrap();
        let posts: Vec<_> = posts.iter().enumerate().map(|(i, (by, at))| json!({"n": i + 1, "at": at, "by": by, "text": "words", "names": []})).collect();
        serde_json::from_value(json!({"id": id, "n": n, "kind": "thread", "opener": "owner", "ctx": {}, "form": {}, "title": id,
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": "2026-11-20T10:00:00Z",
            "given": [], "posts": posts, "unanswered_owner": unanswered, "mentions": {"1": mentions}}))
        .unwrap()
    }

    const NOW: i64 = 1_795_000_000; // 2026-11-18

    fn island<'a>(id: &'a str, lines: &'a [Thread], tickets: &'a BTreeMap<u64, TicketView>, agents: &'a [AgentView], accounts: Option<&'a NativeAccounts>) -> Island<'a> {
        Island { id, name: id, lines, tickets, agents, accounts }
    }

    /// Made-up lines of research's shape: two items with checks under them, a thread with sub-threads, lone threads.
    fn research() -> Vec<Thread> {
        vec![
            line("I-1469", "item", None, &["accounts-2"], 4),
            line("N-1470", "thread", Some("I-1469"), &["accounts-2", "consult-3"], 30),
            line("I-1478", "item", None, &["docket-edge6", "docket-edge5"], 2),
            line("N-1479", "thread", Some("I-1478"), &["docket-edge6", "docket-edge5", "docket-edge8"], 20),
            line("N-1104", "thread", None, &["paper-research"], 700),
            line("N-1200", "thread", Some("N-1104"), &["paper-research", "docket-edge7"], 5),
            line("N-1300", "thread", None, &["docket-edge5", "paper-research"], 3),
            line("N-1301", "thread", None, &["paper-research"], 2),
            line("N-1302", "thread", None, &["paper-research"], 1),
            line("N-1303", "thread", None, &[], 1),
        ]
    }

    fn places(l: &Laid) -> Vec<(String, Rect)> {
        l.graph.nodes().iter().filter(|n| matches!(n.kind, Kind::Project | Kind::Topic | Kind::Item | Kind::Thread)).map(|n| (n.id.clone(), n.rect)).collect()
    }

    fn topic_of<'g>(g: &'g Graph, id: &str) -> &'g str {
        let i = g.find_in(0, id).unwrap();
        &g.node(g.node(i).parent.unwrap()).id
    }

    /// Way 1 first: a line stands in the topic of the top of its chain, an item or a thread with lines under it, and a
    /// line alone with its opening post naming an item in that item's; way 2 then: a rare agent shared takes a line to
    /// its topic, and lines given to the same agents only, or to none, stand together.
    #[test]
    fn topics_come_from_the_records_links_then_from_shared_agents() {
        let mut lines = research();
        // N-1304 closed long ago: no view of its own in the snapshot, only I-1469's list of the posts naming it
        lines.push(line("N-1304", "thread", None, &["consult-3"], 1));
        lines.push(line("N-1306", "thread", None, &["consult-3"], 4));
        let tickets: BTreeMap<u64, TicketView> = [(1469, item("I-1469", &[("N-1304", 1), ("N-1306", 3)]))].into();
        let mut kept = Kept::default();
        let (laid, changed) = lay(&[island("research", &lines, &tickets, &[], None)], &mut kept, NOW);
        assert!(changed);
        let g = &laid[0].as_ref().unwrap().graph;
        assert_eq!(topic_of(g, "N-1470"), "topic:I-1469", "a check under its item");
        assert_eq!(topic_of(g, "I-1469"), "topic:I-1469", "an item is a topic of its own");
        assert_eq!(topic_of(g, "N-1200"), "topic:N-1104", "a sub-thread with the thread it came from");
        assert_eq!(topic_of(g, "N-1304"), "topic:I-1469", "named by its opening post");
        assert_eq!(topic_of(g, "N-1306"), "topic:N-1306", "named by its third post only: way 2's");
        assert_eq!(topic_of(g, "N-1300"), "topic:I-1478", "docket-edge5 is rare, and on the Map's lines");
        assert_eq!(topic_of(g, "N-1301"), "topic:N-1301", "paper-research is on many lines: it weighs little");
        assert_eq!(topic_of(g, "N-1302"), "topic:N-1301", "given to the same agent only");
        assert_eq!(g.node(g.find_in(0, "topic:N-1301").unwrap()).label, "with paper-research");
        assert_eq!(g.node(g.find_in(0, "topic:N-1303").unwrap()).label, "given to no agent");
        assert_eq!(g.node(g.find_in(0, "topic:I-1469").unwrap()).label, "I-1469 label", "named as its line is");
        // the links: a check names its item, a sub-thread branches off its thread, a thread names the item it names
        let e = |a: &str, b: &str| g.edges().iter().find(|e| e.from == g.find_in(0, a).unwrap() && e.to == g.find_in(0, b).unwrap()).map(|e| e.link);
        assert_eq!(e("N-1470", "I-1469"), Some(Link::Names));
        assert_eq!(e("N-1200", "N-1104"), Some(Link::Branch));
        assert_eq!(e("N-1304", "I-1469"), Some(Link::Names));
        // way 2's topic is found again from the kept file: a new line given to paper-research only joins it
        lines.push(line("N-1305", "thread", None, &["paper-research"], 1));
        let (laid, _) = lay(&[island("research", &lines, &tickets, &[], None)], &mut kept, NOW);
        assert_eq!(topic_of(&laid[0].as_ref().unwrap().graph, "N-1305"), "topic:N-1301");
    }

    /// The same record gives the same places, laid afresh or read back from the kept file, and reading it back changes
    /// nothing in it.
    #[test]
    fn the_same_record_gives_the_same_places() {
        let lines = research();
        let tickets = BTreeMap::new();
        let mut a = Kept::default();
        let mut b = Kept::default();
        let one = places(lay(&[island("research", &lines, &tickets, &[], None)], &mut a, NOW).0[0].as_ref().unwrap());
        let two = places(lay(&[island("research", &lines, &tickets, &[], None)], &mut b, NOW).0[0].as_ref().unwrap());
        assert_eq!(one, two);
        assert_eq!(a, b);
        let text = serde_json::to_string(&a).unwrap();
        let mut back: Kept = serde_json::from_str(&text).unwrap();
        let (again, changed) = lay(&[island("research", &lines, &tickets, &[], None)], &mut back, NOW);
        assert_eq!(places(again[0].as_ref().unwrap()), one);
        assert!(!changed, "nothing new: the file is not written");
        // every node lies inside its parent (Graph::new refuses one that does not), regions and lines apart
        let g = &again[0].as_ref().unwrap().graph;
        let topics: Vec<&Node> = g.nodes().iter().filter(|n| n.kind == Kind::Topic).collect();
        for (i, x) in topics.iter().enumerate() {
            for y in &topics[i + 1..] {
                assert!(apart(&x.rect, &y.rect, TOPIC_GAP), "{} and {} overlap", x.id, y.id);
            }
        }
    }

    /// A new thread, a new topic and a thread whose posts grew move no other node: each takes free room, and only
    /// the island grows, down.
    #[test]
    fn what_is_new_moves_no_other_node() {
        let mut lines = research();
        let tickets = BTreeMap::new();
        let mut kept = Kept::default();
        // N-1470's posts, each in its row: none moves when its posts outgrow its rows
        let cells = |l: &Laid| {
            let i = l.graph.find_in(0, "N-1470").unwrap();
            let r = l.rows[&i];
            (1..=30).map(|n| r.cell(l.graph.node(i).rect, n).unwrap()).collect::<Vec<_>>()
        };
        let (first, _) = lay(&[island("research", &lines, &tickets, &[], None)], &mut kept, NOW);
        let before = places(first[0].as_ref().unwrap());
        let cells_before = cells(first[0].as_ref().unwrap());
        assert_eq!(first[0].as_ref().unwrap().rows[&first[0].as_ref().unwrap().graph.find_in(0, "N-1470").unwrap()], Rows { rows: 64, cols: 1 });
        lines.push(line("N-1480", "thread", Some("N-1479"), &["docket-edge6"], 1));
        lines.push(line("I-1490", "item", None, &["accounts-9"], 1));
        lines[1].last = Some(400); // N-1470 grew
        let (laid, changed) = lay(&[island("research", &lines, &tickets, &[], None)], &mut kept, NOW);
        assert!(changed);
        let after = places(laid[0].as_ref().unwrap());
        for (id, r) in &before {
            let now = after.iter().find(|(i, _)| i == id).unwrap().1;
            match id.as_str() {
                "project:research" => assert_eq!((now.x, now.y, now.w), (r.x, r.y, r.w), "the island grows only down"),
                t if t.starts_with("topic:") => assert!((now.x, now.y, now.w) == (r.x, r.y, r.w) && now.h >= r.h, "{t} moved"),
                "N-1470" => assert!((now.x, now.y) == (r.x, r.y) && now.w >= r.w && now.h >= 2.0 * r.h, "{id}: its rows doubled at its place"),
                _ => assert_eq!(now, *r, "{id} moved"),
            }
        }
        let l = laid[0].as_ref().unwrap();
        assert_eq!(cells(l), cells_before, "no post moved");
        let i = l.graph.find_in(0, "N-1470").unwrap();
        let (r, rect) = (l.rows[&i], l.graph.node(i).rect);
        assert!(r.rows >= 128, "it took the room under it: {r:?}");
        assert!((1..=r.rows * r.cols).all(|n| rect.holds(&r.cell(rect, n).unwrap())), "every row inside its line");
        assert_eq!(r.cell(rect, r.rows * r.cols + 1), None, "past its last row: full");
        let g = &l.graph;
        assert_eq!(topic_of(g, "N-1480"), "topic:I-1478", "under a kept line: in its topic");
        assert_eq!(topic_of(g, "I-1490"), "topic:I-1490");
    }

    /// A full line with no room under it goes wider; once it has, it only goes wider, so when room comes under it later
    /// its rows stay as they are and the posts of its second column keep their cells (proto-window N-1479 p29).
    #[test]
    fn a_line_gone_wider_never_grows_down() {
        let tickets = BTreeMap::new();
        let mut lines = vec![line("I-1", "item", None, &["a"], 1), line("N-2", "thread", Some("I-1"), &["a"], 1)];
        let mut kept = Kept::default();
        lay(&[island("p", &lines, &tickets, &[], None)], &mut kept, NOW);
        // nothing free under N-2: its region ends at its card
        let k = kept.islands.get_mut("p").unwrap();
        let l = k.lines["N-2"].clone();
        let t = k.topics.get_mut(&l.topic).unwrap();
        t.rect[3] = l.rect[1] + l.rect[3] + TOPIC_PAD - t.rect[1];
        let cells = |l: &Laid, upto: u32| {
            let i = l.graph.find_in(0, "N-2").unwrap();
            (l.rows[&i], (1..=upto).map(|n| l.rows[&i].cell(l.graph.node(i).rect, n)).collect::<Vec<_>>())
        };
        lines[1].last = Some(100);
        let (laid, _) = lay(&[island("p", &lines, &tickets, &[], None)], &mut kept, NOW);
        let (r, wide) = cells(laid[0].as_ref().unwrap(), 128);
        assert_eq!(r, Rows { rows: 64, cols: 2 }, "no room under it: a column to its right");
        assert!(wide.iter().all(Option::is_some));
        // room under it now, as a region grows down for a new line; its posts fill again
        let k = kept.islands.get_mut("p").unwrap();
        k.topics.get_mut(&l.topic).unwrap().rect[3] += 4_000.0;
        k.rect[3] += 4_000.0;
        lines[1].last = Some(200);
        let (laid, _) = lay(&[island("p", &lines, &tickets, &[], None)], &mut kept, NOW);
        let (after, cells_after) = cells(laid[0].as_ref().unwrap(), 128);
        assert_eq!(after.rows, 64, "its rows stay: {after:?}");
        assert_eq!(cells_after, wide, "posts 1 to 128 keep their cells");
    }

    /// Every post's row is one size, on every line, whatever its posts (docket-edge8 N-1479 p71, p112): lines of 10,
    /// 200, 300, 720 (his chat's) and 2,126 posts (the runner's) all give rows of 180 by 3.75, so level 4 reads alike in
    /// each and a post's level-5 card at its row's start (3:2, 5.625 by 3.75) is the same everywhere. A line of more
    /// than 256 posts takes columns of 512 rows; one that fills 512 rows in its one column grows by a column, not down.
    #[test]
    fn every_posts_row_is_one_size_on_every_line() {
        let tickets = BTreeMap::new();
        let posts = [("N-2", 10), ("N-3", 200), ("N-4", 300), ("N-5", 720), ("N-6", 2_126)];
        let mut lines = vec![line("I-1", "item", None, &["a"], 1)];
        lines.extend(posts.iter().map(|(id, n)| line(id, "thread", Some("I-1"), &["a"], *n)));
        let mut kept = Kept::default();
        let (laid, _) = lay(&[island("p", &lines, &tickets, &[], None)], &mut kept, NOW);
        let shape = |l: &Laid, id: &str| {
            let i = l.graph.find_in(0, id).unwrap();
            (l.rows[&i], l.graph.node(i).rect)
        };
        let l = laid[0].as_ref().unwrap();
        for (id, n) in posts {
            let (r, rect) = shape(l, id);
            assert!(2 * n <= r.rows * r.cols, "{id}: rows for twice its posts: {r:?}");
            for p in [1, n, r.rows * r.cols] {
                let c = r.cell(rect, p).unwrap();
                assert_eq!((c.w, c.h), (LINE_W, ROW_H), "{id}'s post {p}: a row of one size, {r:?} in {rect:?}");
            }
        }
        assert_eq!(shape(l, "N-2").0, Rows { rows: 64, cols: 1 });
        assert_eq!(shape(l, "N-3").0, Rows { rows: 512, cols: 1 });
        assert_eq!(shape(l, "N-4").0, Rows { rows: 512, cols: 2 });
        let (r5, c5) = shape(l, "N-5");
        assert_eq!((r5, c5.w, c5.h), (Rows { rows: 512, cols: 3 }, 540.0, 1_920.0), "his chat: three columns of 512");
        assert_eq!(shape(l, "N-6").0, Rows { rows: 512, cols: 9 });
        // N-3 fills its 512 rows: a column to its right, never down
        let (r3, c3) = shape(l, "N-3");
        lines[2].last = Some(700);
        let (laid, _) = lay(&[island("p", &lines, &tickets, &[], None)], &mut kept, NOW);
        let (r, rect) = shape(laid[0].as_ref().unwrap(), "N-3");
        assert_eq!(r, Rows { rows: 512, cols: 2 }, "a column to its right");
        assert_eq!((rect.x, rect.y, rect.h), (c3.x, c3.y, c3.h), "at its place, no taller");
        assert!((1..=512).all(|p| r.cell(rect, p) == r3.cell(c3, p)), "posts 1 to 512 keep their cells");
    }

    /// A topic with no room left grows down into free room, else goes on in a region of its own beside it, which
    /// takes its next lines; nothing placed moves.
    #[test]
    fn a_full_topic_grows_or_goes_on_beside_itself() {
        let mut lines = vec![line("I-1", "item", None, &["a"], 1)];
        let tickets = BTreeMap::new();
        let mut kept = Kept::default();
        lay(&[island("p", &lines, &tickets, &[], None)], &mut kept, NOW);
        for n in 2..60 {
            lines.push(line(&format!("N-{n}"), "thread", Some("I-1"), &["a"], 1));
            let before = kept.islands["p"].lines.clone();
            let (laid, _) = lay(&[island("p", &lines, &tickets, &[], None)], &mut kept, NOW);
            assert!(laid[0].is_ok(), "every line inside its region: {:?}", laid[0].as_ref().err());
            for (id, l) in before {
                assert_eq!(kept.islands["p"].lines[&id].rect, l.rect, "{id} moved");
            }
        }
        // a topic in the way of its growing: put one right under I-1's region
        let k = kept.islands.get_mut("p").unwrap();
        let r = rect(k.topics["topic:I-1"].rect);
        k.topics.insert("topic:X".into(), KeptTopic { rect: [r.x, r.bottom() + TOPIC_GAP, r.w, 400.0], label: "x".into(), of: None, with: None });
        k.rect[3] = r.bottom() + TOPIC_GAP + 400.0 + ISLAND_PAD - k.rect[1];
        lines.push(line("N-1000", "thread", None, &["x"], 1));
        let k = kept.islands.get_mut("p").unwrap();
        k.lines.insert("N-1000".into(), KeptLine { rect: [r.x + TOPIC_PAD, r.bottom() + TOPIC_GAP + TOPIC_HEAD, LINE_W, LINE_H], topic: "topic:X".into(), rows: 64, cols: 1 });
        let mut placed_in = Vec::new();
        for n in 60..200 {
            lines.push(line(&format!("N-{n}"), "thread", Some("I-1"), &["a"], 1));
            let (laid, _) = lay(&[island("p", &lines, &tickets, &[], None)], &mut kept, NOW);
            let g = &laid[0].as_ref().unwrap().graph;
            placed_in.push(topic_of(g, &format!("N-{n}")).to_string());
        }
        assert!(placed_in.iter().any(|t| t == "topic:I-1~2"), "it went on beside itself: {placed_in:?}");
        let k = &kept.islands["p"];
        assert_eq!(k.topics["topic:I-1~2"].of.as_deref(), Some("topic:I-1"));
        assert_eq!(k.topics["topic:I-1~2"].label, "I-1 label");
        let regions: Vec<Rect> = k.topics.values().map(|t| rect(t.rect)).collect();
        for (i, a) in regions.iter().enumerate() {
            for b in &regions[i + 1..] {
                assert!(apart(a, b, TOPIC_GAP));
            }
        }
    }

    /// Two projects that both have an N-12, an agent of one name and an account of one id lie apart, each island a
    /// graph of its own, each node inside its own project (consult-3 N-1479 p16).
    #[test]
    fn two_islands_with_the_same_ids_lie_apart() {
        let lines = vec![line("N-12", "thread", None, &["docket-edge5"], 3)];
        let tickets = BTreeMap::new();
        let agents = vec![agent("docket-edge5", "working", Some("N-12"))];
        let acc = NativeAccounts { accounts: vec![coord_types::NativeAccount { id: "codex".into(), ..Default::default() }], ..Default::default() };
        let mut kept = Kept::default();
        let (laid, _) = lay(&[island("research", &lines, &tickets, &agents, Some(&acc)), island("cav-paper", &lines, &tickets, &agents, Some(&acc))], &mut kept, NOW);
        let (a, b) = (&laid[0].as_ref().unwrap().graph, &laid[1].as_ref().unwrap().graph);
        assert!(!a.node(0).rect.meets(&b.node(0).rect), "the islands lie apart");
        for (g, p) in [(a, "project:research"), (b, "project:cav-paper")] {
            for id in ["N-12", "agent:docket-edge5", "account:codex"] {
                let i = g.find_in(0, id).unwrap();
                assert_eq!(g.node(g.path(i)[0]).id, p);
            }
        }
        assert_eq!(b.node(b.find_in(0, "N-12").unwrap()).rect.x - a.node(a.find_in(0, "N-12").unwrap()).rect.x, b.node(0).rect.x - a.node(0).rect.x, "placed alike");
        // two new islands in one call, the first with no rows yet (drawn, not kept): apart all the same (consult-3 p30)
        let mut kept = Kept::default();
        let (laid, _) = lay(&[island("a", &[], &tickets, &[], None), island("b", &lines, &tickets, &[], None)], &mut kept, NOW);
        let (a, b) = (&laid[0].as_ref().unwrap().graph, &laid[1].as_ref().unwrap().graph);
        assert!(!a.node(0).rect.meets(&b.node(0).rect), "{:?} {:?}", a.node(0).rect, b.node(0).rect);
        assert!(!kept.islands.contains_key("a") && kept.islands.contains_key("b"));
    }

    /// Path 1, as far as the record draws it (N-1479 p12): his post lit on its line, open while unanswered, and from
    /// the post's line its topic and its item are reached; his old answered posts are not lit.
    #[test]
    fn his_posts_are_lit_and_reach_their_item() {
        let lines = research();
        let tickets: BTreeMap<u64, TicketView> = [(1470, ticket("N-1470", &[("owner", "2026-11-17T09:00:00Z"), ("accounts-2", "2026-11-17T09:10:00Z"), ("owner", "2026-11-17T22:00:00Z")], &[3], &[]))].into();
        let mut kept = Kept::default();
        let (laid, _) = lay(&[island("research", &lines, &tickets, &[], None)], &mut kept, NOW);
        let l = laid[0].as_ref().unwrap();
        assert_eq!(l.lit, vec![Lit { line: l.graph.find_in(0, "N-1470").unwrap(), post: "N-1470.p3".into(), open: true, asked: Vec::new(), to_him: false }], "p1 is a day old and answered");
        let g = &l.graph;
        let line = g.find_in(0, "N-1470").unwrap();
        assert_eq!(g.node(line).tier, Tier::HisAsk);
        let trail: Vec<&str> = g.path(line).iter().map(|i| g.node(*i).id.as_str()).collect();
        assert_eq!(trail, ["project:research", "topic:I-1469", "N-1470"]);
        assert!(g.edges().iter().any(|e| e.from == line && g.node(e.to).id == "I-1469" && e.link == Link::Names));
        assert_eq!(g.node(g.find_in(0, "topic:I-1469").unwrap()).tier, Tier::HisAsk, "a region takes its lines' highest tier");
        assert_eq!(g.node(0).tier, Tier::HisAsk);
    }

    /// His asks' rings (p715 path 1; docket-edge5 N-1479 p313): his post lit with each agent it asks and the post that
    /// answered, from (B)'s ledger (`TicketView::asked`); an agent's post that asks him lit while it waits on him
    /// (threads.rs `open_to_owner`, proto-window p315), however old, and no more once its asks close or his answer to
    /// them is recorded; a post of his asking nobody asks none.
    #[test]
    fn his_asks_are_lit_with_the_agents_asked_and_an_agents_ask_of_him_while_it_waits() {
        let lines = research();
        let posts = [("owner", "2026-11-17T09:00:00Z"), ("accounts-2", "2026-11-10T09:10:00Z"), ("owner", "2026-11-17T22:00:00Z"), ("consult-3", "2026-11-17T22:10:00Z"), ("owner", "2026-11-17T23:00:00Z")];
        let mut t = ticket("N-1470", &posts, &[3], &[]);
        t.asked = serde_json::from_value(json!([{"post": 3, "agent": "accounts-2"}, {"post": 3, "agent": "consult-3", "answer": "N-1470.p4"}])).unwrap();
        // accounts-2's p2, a week old, asks him
        t.posts[1].asks = serde_json::from_value(json!([{"at": [0, 5]}])).unwrap();
        t.posts[1].asks_open = true;
        t.posts[1].told = vec!["owner".into()];
        let lit = |t: &TicketView| {
            let tickets: BTreeMap<u64, TicketView> = [(1470, t.clone())].into();
            let (laid, _) = lay(&[island("research", &lines, &tickets, &[], None)], &mut Kept::default(), NOW);
            let l = laid.into_iter().next().unwrap().unwrap();
            let line = l.graph.find_in(0, "N-1470").unwrap();
            assert!(l.lit.iter().all(|x| x.line == line));
            l.lit.into_iter().map(|x| (x.post, x.open, x.asked, x.to_him)).collect::<Vec<_>>()
        };
        let asked = |a: Option<&str>| vec![("accounts-2".to_string(), a.map(String::from)), ("consult-3".to_string(), Some("N-1470.p4".to_string()))];
        let ask = ("N-1470.p2".to_string(), true, Vec::new(), true);
        let p5 = ("N-1470.p5".to_string(), false, Vec::new(), false);
        assert_eq!(lit(&t), [ask.clone(), ("N-1470.p3".into(), true, asked(None), false), p5.clone()], "p1 a day old, p4 an answer");
        // accounts-2 answers his p3 (the ledger closes its own ask of him on it): p3 lit for the day, with both answers
        let mut both = t.clone();
        both.asked[0].answer = Some("N-1470.p6".into());
        both.unanswered_owner.clear();
        both.posts[1].asks_open = false;
        assert_eq!(lit(&both), [("N-1470.p3".into(), false, asked(Some("N-1470.p6")), false), p5.clone()]);
        // his answer to p2 recorded (the card filled in): it waits on him no more
        let mut answered = t.clone();
        answered.posts[4].answers = Some(2);
        assert_eq!(lit(&answered).first(), Some(&("N-1470.p3".into(), true, asked(None), false)));
        // accounts-2's p2 typed an ask to him (`coord post --ask owner`, I-1480), marking no passage: lit the same
        let mut typed = t.clone();
        typed.posts[1].asks.clear();
        typed.posts[1].told.clear();
        typed.posts[1].names = vec!["owner".into()];
        typed.posts[1].kind = Some(docket_machine::PostKind::Ask { to: vec!["owner".into()] });
        assert_eq!(lit(&typed), [ask.clone(), ("N-1470.p3".into(), true, asked(None), false), p5.clone()], "a typed ask waits on him");
        typed.posts[1].asks_open = false;
        assert_eq!(lit(&typed).first(), Some(&("N-1470.p3".into(), true, asked(None), false)), "closed, it waits no more");
    }

    /// Path 2: agents held on the gateway's limit are one cluster pointing at the gateway's node, with the latest
    /// reset and the 429's words; one waiting with every account out points at the account it came from; the badge
    /// counts them, its dots are on the topic of the line each is on.
    #[test]
    fn held_agents_point_at_what_holds_them() {
        let lines = research();
        let tickets = BTreeMap::new();
        let held = |name: &str, t: i64| AgentView { limit_until: Some(t), limit_why: Some("429 Opus".into()), ..agent(name, "idle", Some("N-1479")) };
        let agents = vec![
            held("docket-edge5", NOW + 600),
            held("docket-edge6", NOW + 900),
            held("docket-edge8", NOW - 60), // its reset has passed: idle
            AgentView { account_waiting: true, account_from: Some("work".into()), account_until: Some(NOW + 60), ..agent("accounts-2", "idle", Some("N-1470")) },
            agent("consult-3", "working", Some("N-1470")),
            agent("dev", "stopped", None),
        ];
        let acc = NativeAccounts {
            accounts: vec![coord_types::NativeAccount {
                id: "work".into(),
                label: "Work".into(),
                tier: 2,
                five_hour: Some(coord_types::LimitWindow { used_percent: 40.0, resets_at: NOW + 100 }),
                seven_day: Some(coord_types::LimitWindow { used_percent: 90.0, resets_at: NOW + 9_000 }),
                out_until: Some(NOW + 60),
                out_for: Some("rate_limit".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut kept = Kept::default();
        let (laid, _) = lay(&[island("research", &lines, &tickets, &agents, Some(&acc))], &mut kept, NOW);
        let l = laid[0].as_ref().unwrap();
        let g = &l.graph;
        let id = |i: u32| g.node(i).id.as_str();
        assert_eq!(l.badge, Badge { waits: 0, stopped: 1, held: 3 });
        assert_eq!(g.node(0).state, State::Stopped);
        assert_eq!(l.held.len(), 2);
        let gw = &l.held[0];
        assert_eq!(gw.on.map(id), Some("account:gateway"));
        assert_eq!(gw.agents.iter().map(|i| id(*i)).collect::<Vec<_>>(), ["agent:docket-edge5", "agent:docket-edge6"]);
        assert_eq!((gw.until, gw.why.as_deref(), gw.model.as_str()), (Some(NOW + 900), Some("429 Opus"), "opus"));
        assert_eq!(l.held[1].on.map(id), Some("account:work"));
        assert_eq!(g.node(g.find_in(0, "account:gateway").unwrap()).state, State::Held);
        // the dots: on the topic of the line each is on; one on none along the island's head
        assert_eq!(topic_of(g, "agent:docket-edge5"), "topic:I-1478");
        assert_eq!(topic_of(g, "agent:consult-3"), "topic:I-1469");
        assert_eq!(g.node(g.find_in(0, "agent:docket-edge8").unwrap()).state, State::Idle);
        assert_eq!(topic_of(g, "agent:dev"), "project:research");
        assert!(g.edges().iter().any(|e| id(e.from) == "agent:consult-3" && id(e.to) == "N-1470" && e.link == Link::Worked));
        // the meter: its fuller window, out until a minute from now
        let m = l.meters.iter().find(|m| id(m.node) == "account:work").unwrap();
        assert_eq!((m.used, m.resets, m.out, m.why.as_deref()), (Some(0.9), Some(NOW + 9_000), Some(NOW + 60), Some("rate_limit")));
        assert_eq!(g.node(m.node).state, State::Held);
    }

    /// A run of a handler, made up: number I past the hidden tickets' first, of the `when` at LINE of KIND on EVENT, for
    /// POST, ended (as "Keep") or still going.
    fn ran(i: u64, kind: &str, line: u32, event: &str, post: Option<u32>, ended: bool) -> serde_json::Value {
        let at = format!("2026-11-18T09:{:02}:00Z", i % 60);
        json!({"id": format!("O-{}", docket_machine::HIDDEN + i), "event": event, "from": kind, "line": line, "post": post, "opened_at": at,
            "closed_at": if ended { json!(at) } else { json!(null) }, "said": if ended { json!("Keep") } else { json!(null) }})
    }

    /// Programs (p715 path 3; docket-edge8 N-1479 p17, p23): each handler that ran is a node of its own kind in the
    /// island's region Programs, placed as a topic is, with rows for its runs as a line has for its posts. A handler is
    /// a kind and an event: a `when` moved in its kind (its runs at two lines) is one handler, and a run of no `when` is
    /// none's. A new handler, and runs that fill one, move nothing placed. A record read with no run (as a window from
    /// before handlers reads one) draws no Programs and leaves its places in the kept file as they are.
    #[test]
    fn the_handlers_that_ran_lie_in_programs_and_move_nothing_placed() {
        let lines = research();
        let none = BTreeMap::new();
        let others = |l: &Laid| places(l).into_iter().filter(|(id, _)| !id.starts_with("project:") && !id.starts_with(PROGRAMS)).collect::<Vec<_>>();
        // the watcher's `when` on agent_post ran 40 times under N-1104, the last still going, the first 10 when it stood
        // at line 11 of its kind, the rest at 12; base's on owner_post for two of his posts; and a stop's run, of no `when`
        let mut chat = ticket("N-1104", &[("owner", "2026-11-18T09:00:00Z"), ("owner", "2026-11-18T09:10:00Z")], &[], &[]);
        let mut runs: Vec<serde_json::Value> = (1..=40).map(|i| ran(i, "watcher", if i <= 10 { 11 } else { 12 }, "agent_post", None, i < 40)).collect();
        runs.extend([ran(41, "base", 43, "owner_post", Some(1), true), ran(42, "base", 43, "owner_post", Some(2), true)]);
        runs.push(json!({"id": format!("O-{}", docket_machine::HIDDEN + 43), "event": "stopped", "opened_at": "2026-11-18T09:30:00Z", "said": "stopped"}));
        chat.ran = serde_json::from_value(json!(runs)).unwrap();
        let mut tickets: BTreeMap<u64, TicketView> = [(1104, chat)].into();
        let mut kept = Kept::default();
        let (laid, changed) = lay(&[island("research", &lines, &tickets, &[], None)], &mut kept, NOW);
        assert!(changed);
        let l = laid[0].as_ref().unwrap();
        let g = &l.graph;
        let first = others(l);
        let programs = g.find_in(0, PROGRAMS).expect("Programs");
        assert_eq!((g.node(programs).kind, g.node(programs).label.as_str()), (Kind::Topic, "Programs"));
        let handlers: Vec<&Node> = g.nodes().iter().filter(|n| n.kind == Kind::Handler).collect();
        assert_eq!(handlers.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["handler:base:owner_post", "handler:watcher:agent_post"], "one each: a `when` moved is one handler, a run of no `when` none's");
        assert!(handlers.iter().all(|n| n.parent == Some(programs) && !n.his));
        let (base, watcher) = (g.find_in(0, "handler:base:owner_post").unwrap(), g.find_in(0, "handler:watcher:agent_post").unwrap());
        assert_eq!(g.node(base).label, "base · when your words land");
        assert_eq!(g.node(watcher).label, "watcher · when an agent posts");
        assert_eq!((g.node(watcher).state, g.node(watcher).weight, g.node(base).state), (State::Working, 40.0, State::Idle), "one of the watcher's runs still going");
        assert_eq!(g.node(programs).state, State::Working, "Programs says a program is running");
        assert_eq!((l.rows[&watcher], l.rows[&base]), (Rows { rows: 128, cols: 1 }, Rows { rows: ROWS_LEAST, cols: 1 }), "rows for twice its runs, as a line's for its posts");
        let regions: Vec<&Node> = g.nodes().iter().filter(|n| n.kind == Kind::Topic).collect();
        for (i, x) in regions.iter().enumerate() {
            for y in &regions[i + 1..] {
                assert!(apart(&x.rect, &y.rect, TOPIC_GAP), "{} and {} overlap", x.id, y.id);
            }
        }
        let topics: f32 = regions.iter().filter(|n| n.id != PROGRAMS).map(|n| n.weight).sum();
        assert_eq!(g.node(0).weight, topics, "its runs are no posts: Programs weighs nothing in the island's rank");
        // a new handler, and the watcher's runs past its rows: nothing placed moves; the watcher grows at its place
        let was: Vec<(String, Rect)> = g.nodes().iter().map(|n| (n.id.clone(), n.rect)).collect();
        let w0 = g.node(watcher).rect;
        let mut more: Vec<serde_json::Value> = (44..=300).map(|i| ran(i, "watcher", 12, "agent_post", None, true)).collect();
        more.push(ran(301, "thread", 7, "ask", None, true));
        tickets.get_mut(&1104).unwrap().ran.extend(serde_json::from_value::<Vec<docket_machine::ViewRan>>(json!(more)).unwrap());
        let (laid, changed) = lay(&[island("research", &lines, &tickets, &[], None)], &mut kept, NOW);
        assert!(changed);
        let g = &laid[0].as_ref().unwrap().graph;
        for (id, r) in &was {
            let now = g.node(g.find_in(0, id).unwrap()).rect;
            assert_eq!([now.x, now.y], [r.x, r.y], "{id} at its place");
            if !(id.starts_with("project:") || id.starts_with(PROGRAMS) || id == "handler:watcher:agent_post") {
                assert_eq!(now, *r, "{id} as it was");
            }
        }
        let w1 = g.node(g.find_in(0, "handler:watcher:agent_post").unwrap()).rect;
        assert!(w1.w * w1.h > w0.w * w0.h, "the watcher grew: {w0:?} to {w1:?}");
        let ask = g.find_in(0, "handler:thread:ask").expect("the new handler laid");
        assert!(g.node(g.node(ask).parent.unwrap()).id.starts_with(PROGRAMS), "in Programs, or a region beside it");
        // read with no run: no Programs, no handler, and the kept file as it was
        let held = kept.clone();
        let (laid, changed) = lay(&[island("research", &lines, &none, &[], None)], &mut kept, NOW);
        assert!(!changed && kept == held, "nothing written over their places");
        let l = laid[0].as_ref().unwrap();
        assert!(l.graph.find_in(0, PROGRAMS).is_none() && !l.graph.nodes().iter().any(|n| n.kind == Kind::Handler));
        assert_eq!(others(l), first, "the topics and their lines where they lay");
    }

    /// The kept file: none yet reads as no places; written whole and read back the same; one of a later shape, or not
    /// JSON, is left as it is (an error, and the caller writes nothing over it).
    #[test]
    fn the_kept_file_is_read_back_or_left_as_it_is() {
        let dir = std::env::temp_dir().join(format!("yaaihh-map-kept-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("map.json");
        assert_eq!(read(&p), Ok(Kept::default()));
        let lines = research();
        let tickets = BTreeMap::new();
        let mut kept = Kept::default();
        lay(&[island("research", &lines, &tickets, &[], None)], &mut kept, NOW);
        write(&p, &kept).unwrap();
        assert_eq!(read(&p), Ok(kept));
        std::fs::write(&p, r#"{"shape": 9, "islands": {}}"#).unwrap();
        assert!(read(&p).is_err());
        std::fs::write(&p, "not json").unwrap();
        assert!(read(&p).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// At the record's size (research: 951 lines, 883 of them at the outline's root; 40 agents), made up: laid out
    /// afresh, and again from the kept file with one new line. Times printed for the head.
    #[test]
    fn lays_the_records_size_in_time() {
        let names: Vec<String> = (0..40).map(|i| format!("agent-{i}")).collect();
        let mut lines = Vec::new();
        for i in 0..883 {
            let kind = if i < 68 && i % 3 == 0 { "item" } else { "thread" };
            let ag = [names[i % 40].as_str(), names[(i * 7 + 3) % 40].as_str(), "paper-research"];
            lines.push(line(&format!("N-{}", 10_000 + i), kind, None, &ag[..1 + i % 3], (i * 7 % 300) as u32));
        }
        for i in 0..68 {
            lines.push(line(&format!("N-{}", 20_000 + i), "thread", Some(&format!("N-{}", 10_000 + i)), &[names[i % 40].as_str()], (i % 50) as u32));
        }
        let agents: Vec<AgentView> = names.iter().enumerate().map(|(i, a)| agent(a, "working", Some(format!("N-{}", 20_000 + i % 68).as_str()))).collect();
        let tickets = BTreeMap::new();
        let mut kept = Kept::default();
        let t = std::time::Instant::now();
        let (laid, _) = lay(&[island("research", &lines, &tickets, &agents, None)], &mut kept, NOW);
        let fresh = t.elapsed();
        let l = laid[0].as_ref().unwrap();
        assert_eq!(l.graph.nodes().iter().filter(|n| matches!(n.kind, Kind::Thread | Kind::Item)).count(), 951);
        lines.push(line("N-40000", "thread", None, &["agent-7"], 1));
        let t = std::time::Instant::now();
        let (laid, changed) = lay(&[island("research", &lines, &tickets, &agents, None)], &mut kept, NOW);
        let again = t.elapsed();
        assert!(laid[0].is_ok() && changed);
        let topics = laid[0].as_ref().unwrap().graph.nodes().iter().filter(|n| n.kind == Kind::Topic).count();
        let file = serde_json::to_string(&kept).unwrap().len();
        println!("951 lines, {topics} topics: afresh {fresh:?}, one new line {again:?}; the kept file {file} bytes");
        assert!(fresh.as_secs_f64() < 5.0 && again.as_secs_f64() < 1.0, "unoptimised (the quick profile): afresh {fresh:?}, again {again:?}");
    }

    /// A made-up island with one of each thing [`key`] reads, each of its fields set.
    #[derive(Clone)]
    struct World {
        id: String,
        name: String,
        lines: Vec<Thread>,
        tickets: BTreeMap<u64, TicketView>,
        agents: Vec<AgentView>,
        accounts: Option<NativeAccounts>,
    }

    fn world() -> World {
        let mut n2 = line("N-2", "thread", Some("I-1"), &["docket-edge5"], 2);
        n2.started = vec!["docket-edge6".into()];
        let mut chat = ticket("N-2", &[("owner", "2026-11-17T10:00:00Z"), ("docket-edge5", "2026-11-17T11:00:00Z")], &[1], &[]);
        chat.under = Some("I-1".into());
        // his p1 asks docket-edge5, whose p2 asks him (the rings: docket-edge5 N-1479 p313)
        chat.asked = serde_json::from_value(json!([{"post": 1, "agent": "docket-edge5"}])).unwrap();
        chat.posts[1].asks = serde_json::from_value(json!([{"at": [0, 5]}])).unwrap();
        chat.posts[1].asks_open = true;
        chat.posts[1].told = vec!["owner".into()];
        // one run of base's `when` on owner_post, for his p1, ended: a handler in Programs (docket-edge5 N-1479 p245)
        chat.ran = serde_json::from_value(json!([ran(1, "base", 43, "owner_post", Some(1), true)])).unwrap();
        let a = AgentView {
            items: vec!["I-1".into()],
            limit_until: Some(NOW + 60),
            limit_why: Some("429 Opus".into()),
            account_from: Some("work".into()),
            account_until: Some(NOW + 90),
            ..agent("docket-edge5", "working", Some("N-2"))
        };
        let w = |used: f64, at: i64| Some(LimitWindow { used_percent: used, resets_at: at });
        let account = coord_types::NativeAccount {
            id: "work".into(),
            label: "Work".into(),
            tier: 1,
            five_hour: w(40.0, NOW + 100),
            seven_day: w(10.0, NOW + 1_000),
            out_until: Some(NOW + 30),
            out_for: Some("rate_limited".into()),
            ..Default::default()
        };
        World {
            id: "research".into(),
            name: "research".into(),
            lines: vec![line("I-1", "item", None, &["docket-edge5"], 1), n2],
            tickets: [(1, item("I-1", &[("N-2", 1)])), (2, chat)].into(),
            agents: vec![a],
            accounts: Some(NativeAccounts { accounts: vec![account], gateway: true, ..Default::default() }),
        }
    }

    fn island_of(w: &World) -> Island<'_> {
        Island { id: &w.id, name: &w.name, lines: &w.lines, tickets: &w.tickets, agents: &w.agents, accounts: w.accounts.as_ref() }
    }

    fn chat(w: &mut World) -> &mut TicketView {
        w.tickets.get_mut(&2).unwrap()
    }

    fn work(w: &mut World) -> &mut coord_types::NativeAccount {
        &mut w.accounts.as_mut().unwrap().accounts[0]
    }

    /// The relay's key (proto-lead N-1479 p118, p124): each field the layout reads changes it; an agent's tools, a
    /// post's words, the others' posts' times and when a line opened, which it does not read, leave it as it was, and
    /// the island lays alike with them changed. Of a run (Programs, [`handled`]) it reads its id, its `when`'s kind,
    /// whether it has a `when`, its event and whether it ended; not what it came to, when it opened, its post, nor its
    /// `when`'s line, which an edit moves (docket-edge5 p245).
    #[test]
    fn the_key_changes_with_each_field_laid_from_and_with_nothing_else() {
        let base = world();
        let k = key(&island_of(&base));
        assert_eq!(key(&island_of(&base.clone())), k, "one island, one key");
        let laid_from: &[(&str, fn(&mut World))] = &[
            ("the island's id", |w| w.id = "prod".into()),
            ("its name", |w| w.name = "prod".into()),
            ("a line's id", |w| w.lines[1].id = "N-3".into()),
            ("a line's label", |w| w.lines[1].label = "other".into()),
            ("a line's kind", |w| w.lines[1].kind = "item".into()),
            ("whether an agent opened it", |w| w.lines[1].by = Some("docket-edge6".into())),
            ("whether it ended", |w| w.lines[1].ended = Some("2026-11-18T10:00:00Z".into())),
            ("a line's state", |w| w.lines[1].state = "waits on you".into()),
            ("whether it is archived", |w| w.lines[1].archived = true),
            ("its last post", |w| w.lines[1].last = Some(3)),
            ("the line it stands under", |w| w.lines[1].parent = None),
            ("its agents", |w| w.lines[1].agents.push("consult-3".into())),
            ("its starters", |w| w.lines[1].started.clear()),
            ("a line more", |w| w.lines.push(line("N-4", "thread", None, &[], 1))),
            ("a ticket's id", |w| chat(w).id = "N-3".into()),
            ("a ticket's kind", |w| chat(w).kind = "item".into()),
            ("its parent", |w| chat(w).parent = Some("I-1".into())),
            ("what it stands under", |w| chat(w).under = None),
            ("which post names an item", |w| w.tickets.get_mut(&1).unwrap().mentioned[0].post = 2),
            ("which ticket names it", |w| w.tickets.get_mut(&1).unwrap().mentioned[0].by = "N-5".into()),
            ("his post's number", |w| chat(w).posts[0].n = 3),
            ("his post's time", |w| chat(w).posts[0].at = "2026-11-17T12:00:00Z".into()),
            ("a post his", |w| chat(w).posts[1].by = "owner".into()),
            ("his posts unanswered", |w| chat(w).unanswered_owner.clear()),
            ("an answer to his ask", |w| chat(w).asked[0].answer = Some("N-2.p2".into())),
            ("an agent more it asks", |w| chat(w).asked.push(serde_json::from_value(json!({"post": 1, "agent": "consult-3"})).unwrap())),
            ("an agent's ask of him closing", |w| chat(w).posts[1].asks_open = false),
            ("his answer to it recorded", |w| chat(w).posts[0].answers = Some(2)),
            ("whether it is broken", |w| chat(w).broken = Some("NoCall".into())),
            ("whether it is closed", |w| chat(w).closed_at = Some("2026-11-18T10:00:00Z".into())),
            ("a ticket more", |w| drop(w.tickets.insert(5, ticket("N-5", &[], &[], &[])))),
            ("a run's id", |w| chat(w).ran[0].id = "O-9".into()),
            ("its `when`'s kind", |w| chat(w).ran[0].from = Some("watcher".into())),
            ("whether it ran a `when`", |w| chat(w).ran[0].line = None),
            ("its event", |w| chat(w).ran[0].event = "agent_post".into()),
            ("whether the run ended", |w| chat(w).ran[0].closed_at = None),
            ("an agent's name", |w| w.agents[0].name = "docket-edge6".into()),
            ("its state", |w| w.agents[0].state = "idle".into()),
            ("its model", |w| w.agents[0].model = "sonnet".into()),
            ("the ticket it is on", |w| w.agents[0].on = Some("I-1".into())),
            ("its items", |w| w.agents[0].items.clear()),
            ("its limit's end", |w| w.agents[0].limit_until = Some(NOW + 61)),
            ("why", |w| w.agents[0].limit_why = None),
            ("whether it waits for an account", |w| w.agents[0].account_waiting = true),
            ("the account it is from", |w| w.agents[0].account_from = None),
            ("until when", |w| w.agents[0].account_until = Some(NOW + 91)),
            ("an agent more", |w| w.agents.push(agent("consult-3", "idle", None))),
            ("whether the accounts came", |w| w.accounts = None),
            ("the gateway", |w| w.accounts.as_mut().unwrap().gateway = false),
            ("an account's id", |w| work(w).id = "home".into()),
            ("its label", |w| work(w).label = String::new()),
            ("its tier", |w| work(w).tier = 2),
            ("its five hours' use", |w| work(w).five_hour.as_mut().unwrap().used_percent = 41.0),
            ("their reset", |w| work(w).five_hour.as_mut().unwrap().resets_at += 1),
            ("its week's use", |w| work(w).seven_day.as_mut().unwrap().used_percent = 11.0),
            ("its week's reset", |w| work(w).seven_day.as_mut().unwrap().resets_at += 1),
            ("a window gone", |w| work(w).five_hour = None),
            ("out until", |w| work(w).out_until = None),
            ("out for", |w| work(w).out_for = Some("billing_error".into())),
            ("an account more", |w| w.accounts.as_mut().unwrap().accounts.push(coord_types::NativeAccount { id: "codex".into(), ..Default::default() })),
        ];
        for (what, change) in laid_from {
            let mut w = base.clone();
            change(&mut w);
            assert_ne!(key(&island_of(&w)), k, "{what} changes the key");
        }
        let unread: &[(&str, fn(&mut World))] = &[
            ("an agent's tools", |w| w.agents[0].tools.push(serde_json::from_value(json!({"id": "t1", "tool": "Bash", "what": "ls", "at": "2026-11-18T10:00:00Z"})).unwrap())),
            ("its last tool", |w| w.agents[0].last_tool = Some("Bash".into())),
            ("a post's words", |w| chat(w).posts[0].text = serde_json::from_value(json!("other words")).unwrap()),
            ("another's post's time", |w| chat(w).posts[1].at = "2026-11-17T12:00:00Z".into()),
            ("when a line opened", |w| w.lines[1].opened = "2026-11-19T10:00:00Z".into()),
            ("what a run came to", |w| chat(w).ran[0].said = Some("Nothing".into())),
            ("when it opened", |w| chat(w).ran[0].opened_at = "2026-11-18T10:00:00Z".into()),
            ("the post it ran for", |w| chat(w).ran[0].post = Some(2)),
            ("its `when`'s line, moved", |w| chat(w).ran[0].line = Some(44)),
        ];
        // all that is laid, but the graph's index of ids (a HashMap, in no one order)
        let laid = |w: &World| {
            let (out, changed) = lay(&[island_of(w)], &mut Kept::default(), NOW);
            let l = out[0].as_ref().unwrap();
            format!("{:?}", (l.graph.nodes(), l.graph.edges(), &l.rows, &l.badge, &l.held, &l.meters, &l.lit, changed))
        };
        let before = laid(&base);
        for (what, change) in unread {
            let mut w = base.clone();
            change(&mut w);
            assert_eq!(key(&island_of(&w)), k, "{what} leaves the key");
            assert_eq!(laid(&w), before, "{what} lays alike");
        }
    }
}
