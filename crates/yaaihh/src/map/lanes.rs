//! The record's graph around a focus, the Map at the Cloud depth where its coordd serves the page
//! (`GRAPH_PAGE_PROTOCOL`; `Request::Graph`, docket_machine::graph_slice), in place of the Cloud's places
//! (paper-research N-1446 p1385 B; proto-window p1383, head 2; his N-1104 p767: "think like idk google earth layers …
//! fitler by what sutf oyu want to see", p603: the global view "cannot show everything"). Nothing here makes a graph:
//! the slice is coordd's one graph's, its labels coordd's.
//! - **Time across**: x is the record's time, the camera's (camera.rs) along x alone, wheel and drag as the Map's; the
//!   right edge is now.
//! - **Lanes by layer**: one lane per layer on, his switches by kind over the canvas (his messages, agent to agent,
//!   deliveries, work, calls and prompts, files), and in a lane one row per agent where there is one ([`lay`]).
//! - **Cards and edges**: a node is a dot at its time in its lane and row with its label beside it where there is
//!   room; an edge is a line between two, in its layer's ink, drawn where it touches the focus or the card under the
//!   pointer, the others counted on their cards ("+12"). A card's edges at agents in a lane are one card, a fan
//!   ("reached 18: paper-research, docket-core, …"), that a click opens into its agents' rows; so a row is an agent's
//!   only where it has a card (paper-research N-1446 p1469, 1 to 3; p603's "explored by the user / navigated").
//! - **Folds**: a stretch with nothing in it is drawn short, its length over it ("— 2 h 10 m —"), so the camera's fit
//!   gives its room to what happened (p1469, 5).
//! - **Around a focus**: it opens around his newest post; a click on a card makes it the focus, its edges in the
//!   layers on running out from it, [`HOPS`] hops; Esc goes back to the focus before. The line under the switches says
//!   what is on, what the slice holds, the focus and its hops, and what was left out ([`line`]).
//! - **Far out**, where an hour is narrower than [`HOUR_LEAST`], each lane is its counts per hour (`Slice::hours`).
//! - **Now** (`GraphAnswer::now`, `Graph::now` at coordd's clock), the lane on top (p1469, 6): a row per agent in a
//!   turn, in words across the canvas, in columns: since when, its last post, its last job green or red, its items
//!   ([`doing`]); then his asks nobody took, and those the ASKED ledger holds open with no commit, each a row of its
//!   own that says its count, "his asks nobody took · 17", and how long its oldest and newest have waited; a click on
//!   it opens its list, a line an ask, its id, its first words and how long it has waited, under the row or, where the
//!   canvas is short there, over it ([`ask_list`]), and a click on a line makes that ask the focus (paper-research
//!   N-1446 p1510, 2: a bar with the newest ask's words read as a single ask).
//! - **His ask's course** (`GraphAnswer::course`; paper-research N-1446 p1429, 1), around his post: under the line,
//!   asked → reached → taken → first commit → merged → first green → in his window, each point's time and the span to
//!   it, the longest standing out by the times alone ([`course`]); on the canvas, the same along his post's row.

use super::camera::Camera;
use super::canvas::{CLICK, NOTCH, PER_NOTCH, bar_press, faded};
use crate::App;
use coord_types::GraphAnswer;
use docket_machine::graph::{AskTimes, NodeKind, Now, Point};
use docket_machine::graph_slice::{Ask, Layer, SliceNode};
use rune::{Color, El, LineCap, MagnifyEvent, PointerEvent, Shape, TextEdit, Tx, WheelEvent, canvas, col, el, row, text};
use rune_kit::{Button, ButtonVariant, Kit, Switch, TextInput};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The hops a focus runs out to until he sets others.
pub const HOPS: u32 = 3;
/// The most hops he can set.
const MOST_HOPS: u32 = 6;
/// The rows' names at the canvas's left, in points.
pub(super) const LEFT: f32 = 168.0;
/// Room right of now for what each agent is doing, in points.
const RIGHT: f32 = 300.0;
/// The now lane's columns for an agent ([`doing`]): its turn, its last post, its last job, in points; its items take
/// the rest.
const NOW_COLUMNS: [f32; 3] = [170.0, 400.0, 190.0];
/// A row, a lane's name over its rows, and the gap under a lane, in points.
const ROW: f32 = 26.0;
const HEAD: f32 = 20.0;
const GAP: f32 = 8.0;
/// A card's dot, and the most its words take beside it, in points.
const DOT: f32 = 9.0;
const WORDS: f32 = 220.0;
/// The least room for a card's words, in points: less, and only its dot is drawn.
const WORDS_LEAST: f32 = 28.0;
/// An hour narrower than this, in points, and each lane is its counts per hour: far out.
pub const HOUR_LEAST: f32 = 40.0;
/// Room left at the start of the span when the camera fits it, in points.
const ROOM: f32 = 16.0;
/// A stretch with nothing in it wider than `FOLD_LEAST` points at the camera's fit is drawn `FOLD` wide, its length
/// in words over it ([`Fold`]).
const FOLD: f32 = 96.0;
const FOLD_LEAST: f32 = 2.0 * FOLD;
/// A gap between two of the times drawn at least this many times their middle gap (the lower of two middles) is a
/// stretch with nothing in it, if it is wider than `FOLD_LEAST` too ([`folds_for`]): minutes apart are the slice's pace,
/// and a night with nothing in it is not.
const IDLE: i64 = 4;
/// The now lane's row of his asks nobody took, and of those the ASKED ledger holds open with no commit.
pub(super) const NOT_TAKEN: &str = "";
const OPEN: &str = "\u{1}open";
/// What [`under`] says of a row of his asks: this, then the row's name.
const ASKS: &str = "asks:";
/// A line of the list of his asks, and the list's width at most, in points.
const ASK_LINE: f32 = 20.0;
const ASK_LIST_W: f32 = 640.0;

/// The page's own state on the App.
#[derive(Default)]
pub struct Lanes {
    /// The ask as he set it, of project `ask_of`: the layers on, the focus and its hops; who and the items are the
    /// fields' ([`Lanes::who`], [`Lanes::item`]). The first focus is his newest post.
    pub ask: Ask,
    pub ask_of: usize,
    /// The foci before this one, the latest last: Esc goes back to it.
    pub back: Vec<String>,
    /// Who and the item as typed: names or ids, by commas or spaces.
    pub who: TextEdit,
    pub item: TextEdit,
    /// The ask on its way, of project `.0`; and asked again though held ("Read again").
    pub asked: Option<(usize, Ask)>,
    pub again: bool,
    /// The answer held, of project `.0`, to the ask `.1`: the slice, its labels and now, or why there is none.
    pub got: Option<(usize, Ask, Result<GraphAnswer, String>)>,
    /// x: seconds after `t0`, `cam.scale` points a second (y is the lanes', not the camera's); `dy` the lanes' scroll.
    pub cam: Camera,
    pub t0: i64,
    pub dy: f32,
    /// The canvas, in window points, and whether the camera was fitted to the answer held.
    pub area: [f32; 4],
    pub fitted: bool,
    /// A press on the canvas, and whether it moved; the pointer, and the card under it.
    pub press: Option<([f32; 2], bool)>,
    pub pointer: Option<[f32; 2]>,
    pub hover: Option<String>,
    /// The fans he opened into their agents, by their ids ([`Fan`]).
    pub open: BTreeSet<String>,
    /// The stretches with nothing in them drawn short, as the camera was fitted ([`Fold`]).
    pub folds: Vec<Fold>,
    /// The row of his asks in now's lane whose list he opened ([`NOT_TAKEN`] or [`OPEN`]), one at a time.
    pub listed: Option<String>,
}

/// A stretch of time with nothing in view, drawn short (paper-research N-1446 p1469, 5): from `from` to `to`, in
/// seconds, drawn as `kept` seconds of the camera's, its length said over it ("— 2 h 10 m —"). The course's spans
/// keep their true lengths in words.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fold {
    pub from: i64,
    pub to: i64,
    pub kept: f64,
}

/// Time T as the camera has it: the seconds of the folds before it cut to what each keeps, within one in proportion.
fn folded(folds: &[Fold], t: i64) -> f64 {
    let mut cut = 0.0;
    for f in folds {
        let whole = (f.to - f.from) as f64;
        if t >= f.to {
            cut += whole - f.kept;
        } else if t > f.from {
            cut += (t - f.from) as f64 * (1.0 - f.kept / whole);
        }
    }
    t as f64 - cut
}

/// The folds for TIMES (sorted) fitting FIRST..LAST into ROOM points, and the scale they leave. A gap between two
/// times is a stretch with nothing in it when it is at least [`IDLE`] times the middle gap and wider than
/// [`FOLD_LEAST`] at the fit with no folds; each is drawn [`FOLD`] wide, the longest first while the folds leave half
/// the room, and the rest of the span takes the room they leave. Once: what folding widens is not folded again, so
/// three even minutes have none, and a night after a day of posts folds though the day's gaps together are longer.
/// Only the times from FIRST to LAST count, those two among them: what the fit leaves out of view folds nothing (on
/// p678 the days before its focus folded, and the span left was too short to fit).
fn folds_for(times: &[i64], first: i64, last: i64, room: f64) -> (Vec<Fold>, f64) {
    let fit = |span: f64| (room / span.max(60.0)).clamp(super::camera::FARTHEST, super::camera::NEAREST);
    let scale = fit((last - first) as f64);
    let times: Vec<i64> = std::iter::once(first).chain(times.iter().copied().filter(|t| *t > first && *t < last)).chain(std::iter::once(last)).collect();
    let mut gaps: Vec<i64> = times.windows(2).map(|w| w[1] - w[0]).collect();
    gaps.sort();
    let Some(middle) = gaps.get(gaps.len().saturating_sub(1) / 2).copied() else { return (Vec::new(), scale) };
    let mut idle: Vec<(i64, i64)> = times.windows(2).filter(|w| w[1] - w[0] >= IDLE * middle.max(1) && (w[1] - w[0]) as f64 * scale > FOLD_LEAST as f64).map(|w| (w[0], w[1])).collect();
    idle.sort_by_key(|(a, b)| std::cmp::Reverse(b - a));
    idle.truncate((room / 2.0 / FOLD as f64) as usize);
    if idle.is_empty() {
        return (Vec::new(), scale);
    }
    idle.sort();
    let rest = (last - first) as f64 - idle.iter().map(|(a, b)| (b - a) as f64).sum::<f64>();
    let scale = fit(rest * room / (room - idle.len() as f64 * FOLD as f64));
    (idle.into_iter().map(|(from, to)| Fold { from, to, kept: FOLD as f64 / scale }).collect(), scale)
}

/// Whether the project shown has its Map drawn from its coordd's graph: a coordd at `GRAPH_PAGE_PROTOCOL` or later,
/// whose answer holds the page's items, now and course. An earlier one draws the Map's own places (canvas.rs) until
/// they go (paper-research N-1446 p1406).
pub fn serves(s: &App) -> bool {
    serves_in(s, s.cur)
}

/// Whether project PI's coordd serves the graph, as [`serves`] asks it of the project shown: the sidebar offers the
/// Map's row in PI's tree by it (sidebar.rs `map_row`; paper-research N-1446 p1928).
pub fn serves_in(s: &App, pi: usize) -> bool {
    s.projects.get(pi).is_some_and(|p| p.protocol >= coord_types::GRAPH_PAGE_PROTOCOL)
}

/// A layer's word on its switch and in the line (paper-research N-1446 p1376: his messages, agent to agent,
/// deliveries, work, calls and prompts, files).
pub fn word(l: Layer) -> &'static str {
    match l {
        Layer::His => "his messages",
        Layer::Agents => "agent to agent",
        Layer::Deliveries => "deliveries",
        Layer::Work => "work",
        Layer::Calls => "calls and prompts",
        Layer::Files => "files",
        // a newer coordd's layer, which this build does not know (p1574)
        Layer::Other => "other",
    }
}

/// A layer's ink: his the accent, as his posts are elsewhere.
fn ink(l: Layer, k: &Kit) -> Color {
    match l {
        Layer::His => k.c.text_accent,
        Layer::Agents => k.c.text_base,
        Layer::Deliveries => k.c.info.fg,
        Layer::Work => k.c.success.fg,
        Layer::Calls => k.c.warning.fg,
        Layer::Files => k.c.text_muted,
        Layer::Other => k.c.text_muted,
    }
}

/// The layers on: all six when the ask names none.
fn on(ask: &Ask) -> Vec<Layer> {
    Layer::ALL.into_iter().filter(|l| ask.layers.is_empty() || ask.layers.contains(l)).collect()
}

/// His newest post in project PI's snapshot (`N-1104.p767`): the focus the page opens around.
pub fn his_newest(s: &App, pi: usize) -> Option<String> {
    let p = s.projects.get(pi)?;
    p.live.tickets.values().flat_map(|t| t.posts.iter().filter(|x| x.by == "owner").map(move |x| (x.at.clone(), format!("{}.p{}", t.id, x.n)))).max().map(|x| x.1)
}

/// Names or ids typed by commas or spaces.
fn names(typed: &str) -> Vec<String> {
    typed.split(|c: char| c == ',' || c.is_whitespace()).map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect()
}

/// The ask the page wants of project PI now: his, with who and the items as typed, and now; none before a focus.
fn wanted(s: &App) -> Option<Ask> {
    let v = &s.lanes;
    v.ask.around.as_ref()?;
    Some(Ask { who: names(&v.who.text), items: names(&v.item.text), hops: if v.ask.hops == 0 { HOPS } else { v.ask.hops }, now: true, ..v.ask.clone() })
}

// ---- asking -------------------------------------------------------------------------------------------------------

/// The page's frame step, from the Map's poll: once it is shown, the focus is his newest post until he picks another,
/// and the ask he set is asked of the project's coordd, one at a time; again once it changes, and at "Read again".
/// Whether it asked.
pub fn poll(s: &mut App) -> bool {
    if super::canvas::drawn_in(s).is_none() || !serves(s) {
        return false;
    }
    let pi = s.cur;
    if s.lanes.ask_of != pi {
        s.lanes.ask = Ask::default();
        s.lanes.back.clear();
        s.lanes.ask_of = pi;
    }
    if s.lanes.ask.around.is_none() {
        let Some(newest) = his_newest(s, pi) else { return false };
        s.lanes.ask.around = Some(newest);
    }
    let Some(want) = wanted(s) else { return false };
    let v = &s.lanes;
    let held = v.got.as_ref().is_some_and(|(p, a, _)| *p == pi && *a == want);
    if v.asked.is_some() || (held && !v.again) {
        return false;
    }
    s.lanes.again = false;
    if !s.request_in(pi, coord_types::Request::Graph { ask: want.clone() }, crate::Pending::Graph(want.clone())) {
        return false;
    }
    s.lanes.asked = Some((pi, want));
    true
}

/// Project PI's answer to ASK (`Request::Graph`): the slice held, or why there is none. A new focus, other layers or
/// other hops fit the camera to it again; who, the item and "Read again" keep where he is.
pub fn came(s: &mut App, pi: usize, ask: Ask, ok: bool, out: String) {
    if s.lanes.asked.as_ref().is_some_and(|(p, a)| *p == pi && *a == ask) {
        s.lanes.asked = None;
    }
    let got = if ok { serde_json::from_str::<GraphAnswer>(&out).map_err(|e| format!("unreadable graph: {e}")) } else { Err(out) };
    let moved = s.lanes.got.as_ref().is_none_or(|(p, a, _)| *p != pi || a.around != ask.around || a.layers != ask.layers || a.hops != ask.hops);
    s.lanes.got = Some((pi, ask, got));
    if moved {
        s.lanes.fitted = false;
    }
}

/// Make ID the focus, the one before kept for Esc.
pub fn focus(s: &mut App, id: &str) {
    let v = &mut s.lanes;
    if v.ask.around.as_deref() == Some(id) {
        return;
    }
    if let Some(was) = v.ask.around.replace(id.to_string()) {
        v.back.push(was);
    }
}

/// Layer L switched on, or off; the last one on stays on (no layer on would ask for all of them).
pub fn toggle(s: &mut App, l: Layer) {
    let layers = &mut s.lanes.ask.layers;
    if layers.is_empty() {
        *layers = Layer::ALL.into_iter().collect();
    }
    if layers.contains(&l) {
        if layers.len() > 1 {
            layers.remove(&l);
        }
    } else {
        layers.insert(l);
    }
    if layers.len() == Layer::ALL.len() {
        layers.clear();
    }
}

// ---- where everything is -------------------------------------------------------------------------------------------

/// Where the cards of an answer are ([`lay`]).
#[derive(Debug, Default, PartialEq)]
pub struct Laid {
    /// The lanes top to bottom: each its layer (None: now's) and its rows' names, an agent's ("" where there is none;
    /// in now's lane, his asks nobody took).
    pub lanes: Vec<(Option<Layer>, Vec<String>)>,
    /// Each card: its node's place in the slice, its lane, its row, its time in seconds.
    pub cards: Vec<(usize, usize, usize, i64)>,
    /// The edges of a card at agents in one lane, one card each until he opens it ([`Fan`]).
    pub fans: Vec<Fan>,
    /// Each edge of a fan he opened, at an agent, who is a row and no card: its place in the slice, the lane of its
    /// layer, the agent's row there, its time. A delivery handed to an agent is then a mark in the agent's row of the
    /// deliveries lane, a line to it from the post.
    pub marks: Vec<(usize, usize, usize, i64)>,
    /// The nodes no x can place, having no time: said in the line, not drawn.
    pub untimed: usize,
    /// The first time and the last, now's when the answer has it.
    pub span: Option<(i64, i64)>,
}

/// A card's edges at agents in one lane, as one card (paper-research N-1446 p1469, 1): "reached 18: paper-research,
/// docket-core, …", at its first edge's time, in the row of the card it is of where that card is in its lane, else its
/// writer's, with one edge to that card. A click opens it into its agents, each a mark in its own row; another closes
/// it. `id`: `fan:NODE:LAYER`.
#[derive(Clone, Debug, PartialEq)]
pub struct Fan {
    pub id: String,
    pub node: String,
    pub lane: usize,
    pub row: usize,
    pub at: i64,
    pub edges: Vec<usize>,
    /// Its agents, each once, by its first edge's time.
    pub agents: Vec<String>,
}

/// What a fan's edges do, in words: a delivery's "reached", else its first edge's kind ("asks", "names", "edited").
fn fan_verb(a: &GraphAnswer, f: &Fan) -> String {
    use docket_machine::graph::EdgeKind as E;
    match f.edges.first().map(|e| a.slice.edges[*e].edge.kind) {
        Some(E::Handed | E::Woke | E::Informed | E::Took | E::Prompted | E::TurnOf | E::StartOf) | None => "reached".into(),
        Some(E::Ran) => "run by".into(),
        Some(E::Started) => "started by".into(),
        Some(E::Edited) => "edited by".into(),
        Some(E::Read) => "read by".into(),
        Some(k) => serde_json::to_value(k).ok().and_then(|v| v.as_str().map(|w| w.replace('_', " "))).unwrap_or_default(),
    }
}

/// A fan's words: "reached paper-research"; "reached 18: paper-research, docket-core, …".
fn fan_words(a: &GraphAnswer, f: &Fan) -> String {
    let verb = fan_verb(a, f);
    match f.agents.as_slice() {
        [one] => format!("{verb} {one}"),
        many => format!("{verb} {}: {}", many.len(), many.join(", ")),
    }
}

/// The lane a node's card is in, by its kind: none for an agent, who is a row, not a card.
fn home(n: &SliceNode) -> Option<Layer> {
    lane_of(n.node.kind, n.by.as_deref() == Some("owner"))
}

/// The lane a card of kind KIND is in, HIS a post of his: none for an agent or an account, a row, not a card. A
/// commit's, a session's, a job's, a build's and a branch's is work, as `Layer::of` puts every edge at one; the cloud
/// asks a focus that is none of his posts with its own (cloud.rs `own_layer`; paper-research N-1446 p1903, 1).
pub(crate) fn lane_of(kind: NodeKind, his: bool) -> Option<Layer> {
    Some(match kind {
        NodeKind::Agent | NodeKind::Account => return None,
        NodeKind::Post if his => Layer::His,
        NodeKind::Post | NodeKind::Ticket => Layer::Agents,
        // a slice of a session is a delivery; a session's texts, requests and tools are its side, work, as `Layer::of`
        // puts their edges (`Took`; `SessionOf`, `Loaded`, `Briefed`, `Sent`)
        NodeKind::Turn | NodeKind::Start | NodeKind::Slice => Layer::Deliveries,
        NodeKind::Commit | NodeKind::Session | NodeKind::Job | NodeKind::Build | NodeKind::Branch | NodeKind::Worktree | NodeKind::Text | NodeKind::Request | NodeKind::Tools => Layer::Work,
        NodeKind::File => Layer::Files,
        _ => Layer::Calls,
    })
}

/// A row's place among its lane's: his first, then the agents by name, then none.
fn rank(who: &str) -> (u8, String) {
    (if who == "owner" { 0 } else if who.is_empty() { 2 } else { 1 }, who.to_string())
}

/// Where A's cards go under the layers ON (none: all): now's lane on top when A has it, then a lane per layer on in
/// their order. A card is in its kind's lane ([`home`]), in its agent's row, the post's writer or the turn's agent
/// (`SliceNode::by`), else the agent an edge of the slice joins it to, else the row of none, "others". Where its kind's
/// lane is off, it is in the lane of its first edge in the slice, in that lane's row of others: a lane keeps its own
/// rows (his record's p678 with his and work on had a row in his lane for each of 40 agents that answered him, and
/// the work lane under them out of sight). A card's edges at agents in a lane are one card, a [`Fan`], until he opens
/// it (OPEN) into a mark in each agent's row ([`Laid::marks`]); its writer's edge is said by its label, not drawn. So
/// a row is an agent's only where it has a card: his p745's every delivery to 30 agents, stopped ones among them, was
/// a row each and a line each (paper-research N-1446 p1469, 1 and 3). Now's lane: the agents in a turn, then his asks
/// nobody took, then those the ASKED ledger holds open with no commit (p1469, 6).
pub fn lay(a: &GraphAnswer, on_: &BTreeSet<Layer>, open: &BTreeSet<String>) -> Laid {
    let lanes_on: Vec<Layer> = Layer::ALL.into_iter().filter(|l| on_.is_empty() || on_.contains(l)).collect();
    let mut layers: HashMap<&str, BTreeSet<Layer>> = HashMap::new();
    let mut agent: HashMap<&str, &str> = HashMap::new();
    for e in &a.slice.edges {
        for (x, y) in [(&e.edge.from, &e.edge.to), (&e.edge.to, &e.edge.from)] {
            layers.entry(x.as_str()).or_default().insert(e.layer);
            if let Some(name) = y.strip_prefix("agent:") {
                agent.entry(x.as_str()).or_insert(name);
            }
        }
    }
    let mut out = Laid::default();
    let mut rows: Vec<BTreeSet<(u8, String)>> = vec![BTreeSet::new(); lanes_on.len()];
    let mut placed = Vec::new();
    for (i, n) in a.slice.nodes.iter().enumerate() {
        let Some(kinds) = home(n) else { continue };
        let lane = if lanes_on.contains(&kinds) { Some(kinds) } else { layers.get(n.node.id.as_str()).and_then(|ls| ls.iter().copied().find(|l| lanes_on.contains(l))) };
        let own = lanes_on.contains(&kinds);
        let Some(lane) = lane.and_then(|l| lanes_on.iter().position(|x| *x == l)) else { continue };
        let Some(t) = n.node.when.as_deref().and_then(docket_machine::machine::secs) else {
            out.untimed += 1;
            continue;
        };
        let who = if own { n.by.clone().or_else(|| agent.get(n.node.id.as_str()).map(|x| x.to_string())).unwrap_or_default() } else { String::new() };
        rows[lane].insert(rank(&who));
        placed.push((i, lane, who, t));
    }
    let card_at: HashMap<&str, (usize, &str)> = placed.iter().map(|(i, lane, who, _)| (a.slice.nodes[*i].node.id.as_str(), (*lane, who.as_str()))).collect();
    // the edges at an agent, by the card at their other end and their lane: a fan each
    let mut fans: BTreeMap<(&str, usize), Vec<(usize, &str, i64)>> = BTreeMap::new();
    for (ei, e) in a.slice.edges.iter().enumerate() {
        let (name, other) = match (e.edge.from.strip_prefix("agent:"), e.edge.to.strip_prefix("agent:")) {
            (Some(x), None) => (x, e.edge.to.as_str()),
            (None, Some(x)) => (x, e.edge.from.as_str()),
            _ => continue,
        };
        let Some(lane) = lanes_on.iter().position(|l| *l == e.layer) else { continue };
        if card_at.get(other).is_some_and(|(l, w)| *l == lane && *w == name) {
            continue;
        }
        // its writer: the card's label says who
        if e.edge.kind == docket_machine::graph::EdgeKind::Wrote && e.edge.to == other {
            continue;
        }
        let Some(t) = e.edge.when.as_deref().and_then(docket_machine::machine::secs) else { continue };
        fans.entry((other, lane)).or_default().push((ei, name, t));
    }
    let mut marked = Vec::new();
    let mut fanned = Vec::new();
    for ((other, lane), mut es) in fans {
        es.sort_by_key(|x| x.2);
        let id = format!("fan:{other}:{:?}", lanes_on[lane]);
        if open.contains(&id) {
            for (ei, name, t) in es {
                rows[lane].insert(rank(name));
                marked.push((ei, lane, name.to_string(), t));
            }
            continue;
        }
        let who = match card_at.get(other) {
            Some((l, w)) if *l == lane => w.to_string(),
            _ => a.slice.nodes.iter().find(|n| n.node.id == other).and_then(|n| n.by.clone()).unwrap_or_default(),
        };
        rows[lane].insert(rank(&who));
        let mut agents: Vec<String> = Vec::new();
        for (_, name, _) in &es {
            if !agents.iter().any(|x| x == name) {
                agents.push(name.to_string());
            }
        }
        let at = es[0].2;
        fanned.push((Fan { id, node: other.to_string(), lane, row: 0, at, edges: es.iter().map(|x| x.0).collect(), agents }, who));
    }
    // now's lane on top: the agents in a turn, then his asks nobody took, then those open in the ledger
    let first = usize::from(a.now.is_some());
    if let Some(now) = &a.now {
        let mut agents: Vec<String> = now.agents.iter().filter(|x| x.turn.is_some()).map(|x| x.agent.clone()).collect();
        agents.sort();
        if !now.not_taken.is_empty() {
            agents.push(NOT_TAKEN.to_string());
        }
        if !now.open.is_empty() {
            agents.push(OPEN.to_string());
        }
        out.lanes.push((None, agents));
    }
    out.lanes.extend(lanes_on.iter().zip(&rows).map(|(l, r)| (Some(*l), r.iter().map(|x| x.1.clone()).collect())));
    let row_of = |lane: usize, who: &str| out.lanes[lane + first].1.iter().position(|w| w == who).unwrap_or(0);
    out.cards = placed.iter().map(|(i, lane, who, t)| (*i, lane + first, row_of(*lane, who.as_str()), *t)).collect();
    out.marks = marked.iter().map(|(ei, lane, who, t)| (*ei, lane + first, row_of(*lane, who.as_str()), *t)).collect();
    out.fans = fanned.into_iter().map(|(f, who)| Fan { row: row_of(f.lane, &who), lane: f.lane + first, ..f }).collect();
    let times: Vec<i64> = out.cards.iter().chain(&out.marks).map(|c| c.3).chain(out.fans.iter().map(|f| f.at)).chain(a.now.as_ref().and_then(|n| docket_machine::machine::secs(&n.at))).collect();
    out.span = times.iter().min().copied().zip(times.iter().max().copied());
    out
}

/// Every time a card, a fan or a mark of L is at, and now's, in order, each once: what the folds go between.
fn times_of(a: &GraphAnswer, l: &Laid) -> Vec<i64> {
    let mut v: Vec<i64> = l.cards.iter().chain(&l.marks).map(|c| c.3).chain(l.fans.iter().map(|f| f.at)).chain(a.now.as_ref().and_then(|n| docket_machine::machine::secs(&n.at))).collect();
    v.sort();
    v.dedup();
    v
}

/// Each lane's top, in canvas points before the scroll, and the lanes' whole height.
fn tops(l: &Laid) -> (Vec<f32>, f32) {
    let mut y = 0.0;
    let mut tops = Vec::new();
    for (_, rows) in &l.lanes {
        tops.push(y);
        y += HEAD + rows.len().max(1) as f32 * ROW + GAP;
    }
    (tops, y)
}

/// The middle of row ROW of lane LANE, in canvas points.
fn row_y(v: &Lanes, tops: &[f32], lane: usize, row: usize) -> f32 {
    tops[lane] + v.dy + HEAD + row as f32 * ROW + ROW / 2.0
}

/// Time T (seconds) across the canvas, in its points: the camera's seconds after `t0`, the folds drawn short.
fn x_of(v: &Lanes, t: i64) -> f32 {
    LEFT + v.cam.to_screen([folded(&v.folds, t) - folded(&v.folds, v.t0), 0.0])[0]
}

/// Whether the canvas is far out: an hour narrower than [`HOUR_LEAST`].
pub fn far(v: &Lanes) -> bool {
    (3600.0 * v.cam.scale) < HOUR_LEAST as f64
}

/// The answer held for the project shown, with the ask it answers.
fn held(s: &App) -> Option<(&Ask, &GraphAnswer)> {
    let (p, a, got) = s.lanes.got.as_ref()?;
    (*p == s.cur).then_some(())?;
    Some((a, got.as_ref().ok()?))
}

/// Each card drawn, by its node's id, and each fan by its own: where its dot's middle is, in canvas points.
fn dots(v: &Lanes, a: &GraphAnswer, laid: &Laid) -> Vec<(String, [f32; 2])> {
    let (tops, _) = tops(laid);
    let cards = laid.cards.iter().map(|&(i, lane, row, t)| (a.slice.nodes[i].node.id.clone(), [x_of(v, t), row_y(v, &tops, lane, row)]));
    cards.chain(laid.fans.iter().map(|f| (f.id.clone(), [x_of(v, f.at), row_y(v, &tops, f.lane, f.row)]))).collect()
}

/// The card under Q, a point of the canvas: the nearest dot within a dot's reach, else the words beside one.
fn under(s: &App, q: [f32; 2]) -> Option<String> {
    let (ask, a) = held(s)?;
    let laid = lay(a, &ask.layers, &s.lanes.open);
    // his asks in now's lane: a line of the list open is its ask; a row of them, its name or across, is its list
    if let Some(now) = &a.now {
        let (tops, _) = tops(&laid);
        let (lines, _) = ask_list(&s.lanes, now, &laid, &tops);
        if let Some((id, _)) = lines.iter().find(|(_, r)| q[0] >= r[0] && q[0] < r[0] + r[2] && q[1] >= r[1] && q[1] < r[1] + r[3]) {
            return Some(id.clone());
        }
        if let Some((row, _, _)) = ask_rows(&s.lanes, now, &laid, &tops).into_iter().find(|(_, _, y)| (q[1] - y).abs() < ROW / 2.0) {
            return Some(format!("{ASKS}{row}"));
        }
    }
    if q[0] < LEFT || far(&s.lanes) {
        return None;
    }
    let ds = dots(&s.lanes, a, &laid);
    let near = ds.iter().map(|(id, d)| (id, (d[0] - q[0]).hypot(d[1] - q[1]))).filter(|(_, r)| *r <= DOT).min_by(|x, y| x.1.total_cmp(&y.1));
    if let Some((id, _)) = near {
        return Some(id.clone());
    }
    ds.iter().find(|(_, d)| q[0] > d[0] && q[0] < d[0] + DOT + WORDS && (q[1] - d[1]).abs() < ROW / 2.0 - 3.0).map(|(id, _)| id.clone())
}

/// The rows of his asks in now's lane of LAID: each row's name ([`NOT_TAKEN`], [`OPEN`]), its asks in the order he
/// asked them, and its middle's y.
fn ask_rows<'a>(v: &Lanes, now: &'a Now, laid: &Laid, tops: &[f32]) -> Vec<(&'static str, &'a [String], f32)> {
    let Some(lane) = laid.lanes.iter().position(|(l, _)| l.is_none()) else { return Vec::new() };
    let rows = laid.lanes[lane].1.iter().enumerate();
    rows.filter_map(|(ri, name)| {
        let (row, asks) = match name.as_str() {
            NOT_TAKEN => (NOT_TAKEN, &now.not_taken),
            OPEN => (OPEN, &now.open),
            _ => return None,
        };
        Some((row, asks.as_slice(), row_y(v, tops, lane, ri)))
    })
    .collect()
}

/// The name of ROW, a row of his asks in now's lane ([`NOT_TAKEN`], [`OPEN`]), with its count in NOW (paper-research
/// N-1446 p1510, 2: "his asks nobody took · 17", "his asks open, no commit · 19"); None for another row.
fn ask_name(row: &str, now: Option<&Now>) -> Option<String> {
    let count = |f: fn(&Now) -> usize| now.map_or(0, f);
    match row {
        NOT_TAKEN => Some(crate::refs::count("his asks nobody took", count(|n| n.not_taken.len()))),
        OPEN => Some(crate::refs::count("his asks open, no commit", count(|n| n.open.len()))),
        _ => None,
    }
}

/// Where the words across a row of his asks start: past its NAME, drawn whole at 20 points (p1532: cut at the names'
/// width, "his asks open, no commit…" lost its count), and no nearer than the agents' columns.
fn ask_words_x(name: &str, k: &Kit) -> f32 {
    (20.0 + rune::text::measure(name, &[], k.ty.small, None).0 + 12.0).max(LEFT + 8.0)
}

/// The list of the asks of the row he opened (`Lanes::listed`), over the lanes (paper-research N-1446 p1510, 2): each
/// ask's post and its line's rect in canvas points, oldest first; and how many more, said on a last line where they do
/// not all fit. Under its row where the canvas holds it there; else over it, its foot at the row, as a menu by a
/// window's edge opens (p1532's stills: 17 under a row some 320 points over the canvas's foot ran past it); where
/// neither holds it, on the side with more room, as many as fit.
fn ask_list(v: &Lanes, now: &Now, laid: &Laid, tops: &[f32]) -> (Vec<(String, [f32; 4])>, usize) {
    let Some(listed) = v.listed.as_deref() else { return (Vec::new(), 0) };
    let Some((_, asks, y)) = ask_rows(v, now, laid, tops).into_iter().find(|r| r.0 == listed) else { return (Vec::new(), 0) };
    let [w, h] = [v.area[2], v.area[3]];
    let x = LEFT + 8.0;
    let wide = ASK_LIST_W.min(w - x - 8.0).max(0.0);
    // the room under the row and over it, the sheet's 4 points each side kept inside the canvas
    let (below, above) = ((h - (y + ROW / 2.0) - 8.0).max(0.0), (y - ROW / 2.0 - 8.0).max(0.0));
    let need = asks.len() as f32 * ASK_LINE;
    let up = need > below && (need <= above || above > below);
    let fit = (((if up { above } else { below }) / ASK_LINE).floor() as usize).min(asks.len());
    let shown = if fit < asks.len() { fit.saturating_sub(1) } else { fit };
    let drawn = shown + usize::from(shown < asks.len());
    let top = if up { y - ROW / 2.0 - 4.0 - drawn as f32 * ASK_LINE } else { y + ROW / 2.0 };
    (asks.iter().take(shown).enumerate().map(|(i, p)| (p.clone(), [x, top + i as f32 * ASK_LINE, wide, ASK_LINE])).collect(), asks.len() - shown)
}

/// The canvas laid out at R: the camera fitted to the answer held when it is new ([`fit`]), the span across the room between
/// the rows' names and now's words, each stretch with nothing in it folded short ([`folds_for`]).
pub fn laid_out(s: &mut App, r: [f32; 4]) {
    s.lanes.area = r;
    if s.lanes.fitted || r[2] <= 1.0 {
        return;
    }
    let Some((ask, a)) = held(s) else { return };
    let Some((first, last)) = fit(ask, a) else { return };
    let room = (r[2] - LEFT - RIGHT - 2.0 * ROOM).max(80.0) as f64;
    let (folds, scale) = folds_for(&times_of(a, &lay(a, &ask.layers, &s.lanes.open)), first, last, room);
    let v = &mut s.lanes;
    v.folds = folds;
    v.t0 = first;
    v.cam = Camera { at: [-(ROOM as f64) / scale, 0.0], scale };
    v.dy = 0.0;
    v.fitted = true;
}

/// The times the camera opens on for answer A to ASK: from its focus to the span's end, now's where A has it, so a
/// focus opens close in with what came of it, and what came before it is a drag or a wheel away; the whole span where
/// the focus has no time in the slice or is its last. On his record a focus 3 hops out reaches days back (p678 with
/// work on: 732 posts from 12 Nov), and the whole span opened far out, in hours.
fn fit(ask: &Ask, a: &GraphAnswer) -> Option<(i64, i64)> {
    let (first, last) = lay(a, &ask.layers, &BTreeSet::new()).span?;
    let focus = ask.around.as_ref().and_then(|f| a.slice.nodes.iter().find(|n| n.node.id == *f)).and_then(|n| n.node.when.as_deref()).and_then(docket_machine::machine::secs);
    Some((focus.filter(|f| *f > first && *f < last).unwrap_or(first), last))
}

// ---- his hands ------------------------------------------------------------------------------------------------------

/// Zoom across by FACTOR about Q, a point of the canvas: the time under it stays under it.
fn zoom(s: &mut App, factor: f64, q: [f32; 2]) {
    s.lanes.cam.zoom_about(factor, [q[0] - LEFT, 0.0]);
}

pub fn wheel(s: &mut App, e: &WheelEvent) -> bool {
    let d = if e.dy != 0.0 { e.dy } else { e.dx };
    zoom(s, PER_NOTCH.powf((d / NOTCH) as f64), [e.x, e.y]);
    true
}

pub fn magnify(s: &mut App, e: &MagnifyEvent) -> bool {
    zoom(s, (1.0 + e.delta as f64).max(0.05), [e.x, e.y]);
    true
}

/// A press, a drag and their end: a drag moves the time across and the lanes up and down; a press that did not move
/// makes the card under it the focus, or opens the fan under it into its agents (closes it, opened).
pub fn drag(s: &mut App, d: &rune::Drag) {
    let at = [d.x, d.y];
    match d.phase {
        rune::Phase::Down => s.lanes.press = Some((at, false)),
        rune::Phase::Move => {
            let Some((last, moved)) = s.lanes.press else { return };
            if moved || d.dx.hypot(d.dy) > CLICK {
                s.lanes.cam.shift([at[0] - last[0], 0.0]);
                s.lanes.dy = (s.lanes.dy + at[1] - last[1]).min(0.0);
                s.lanes.press = Some((at, true));
            }
        }
        rune::Phase::Up => {
            let moved = s.lanes.press.take().is_some_and(|(_, m)| m);
            if moved || d.cancelled() {
                return;
            }
            match under(s, at) {
                // a fan opens into its agents, and closes again
                Some(id) if id.starts_with("fan:") => {
                    if !s.lanes.open.remove(&id) {
                        s.lanes.open.insert(id);
                    }
                }
                // a row of his asks opens its list, and closes it again
                Some(id) if id.starts_with(ASKS) => {
                    let row = id[ASKS.len()..].to_string();
                    s.lanes.listed = if s.lanes.listed.as_deref() == Some(row.as_str()) { None } else { Some(row) };
                }
                // a card, or an ask of the list: the focus, the list closed
                Some(id) => {
                    s.lanes.listed = None;
                    focus(s, &id);
                }
                None => s.lanes.listed = None,
            }
        }
    }
}

/// The pointer over the canvas: the card under it hovered. Whether the view must be built again: the hovered card
/// changed, or one is hovered, its preview following the pointer.
pub fn pointed(s: &mut App, q: Option<[f32; 2]>) -> bool {
    s.lanes.pointer = q;
    let hover = q.and_then(|q| under(s, q));
    let changed = hover != s.lanes.hover;
    s.lanes.hover = hover;
    changed || s.lanes.hover.is_some()
}

/// A key while the page is shown: Esc goes back to the focus before, + and - zoom at the middle. Whether it was the
/// page's.
pub fn key(s: &mut App, name: &rune::Key) -> bool {
    let mid = [LEFT + (s.lanes.area[2] - LEFT) / 2.0, 0.0];
    match name {
        rune::Key::Escape => match s.lanes.back.pop() {
            Some(id) => s.lanes.ask.around = Some(id),
            None => return false,
        },
        rune::Key::Text(c) if c == "+" || c == "=" => zoom(s, PER_NOTCH, mid),
        rune::Key::Text(c) if c == "-" => zoom(s, 1.0 / PER_NOTCH, mid),
        _ => return false,
    }
    true
}

// ---- the words ------------------------------------------------------------------------------------------------------

/// Kind K in words, one and many: the line's counts ([`counted`]) and a template's groups (templates.rs).
pub(crate) fn kind_words(k: NodeKind) -> (&'static str, &'static str) {
    match k {
        NodeKind::Post => ("post", "posts"),
        NodeKind::Ticket => ("thread or item", "threads and items"),
        NodeKind::Turn => ("turn", "turns"),
        NodeKind::Start => ("start", "starts"),
        NodeKind::Slice => ("slice of a session", "slices of sessions"),
        NodeKind::Step => ("step", "steps"),
        NodeKind::Call => ("call", "calls"),
        NodeKind::CallFile => ("call file", "call files"),
        NodeKind::Template => ("prompt edit", "prompt edits"),
        NodeKind::Program => ("program", "programs"),
        NodeKind::Handler => ("handler", "handlers"),
        NodeKind::File => ("file", "files"),
        NodeKind::Commit => ("commit", "commits"),
        NodeKind::Session => ("session", "sessions"),
        NodeKind::Text => ("text a session read", "texts sessions read"),
        NodeKind::Request => ("request", "requests"),
        NodeKind::Tools => ("tool set", "tool sets"),
        NodeKind::Job => ("job", "jobs"),
        NodeKind::Branch => ("branch", "branches"),
        NodeKind::Worktree => ("worktree", "worktrees"),
        NodeKind::Build => ("build", "builds"),
        NodeKind::Agent => ("agent", "agents"),
        NodeKind::Account => ("account", "accounts"),
        _ => ("other", "others"),
    }
}

/// A count of nodes of kind K, in words: the line's, and a group's card in the cloud's focus (cloud.rs `focus_card`).
pub(crate) fn counted(k: NodeKind, n: usize) -> String {
    let (one, many) = kind_words(k);
    format!("{n} {}", if n == 1 { one } else { many })
}

/// What node N of slice A is, in words: its kind's ([`kind_words`]); but a text a start was started with is a system
/// prompt (docket-machine graph.rs `StartedWith`: his version N or the standing text), where the Map called it "text a
/// session read" under "try-1's system prompt, version 1, appended" (proto-window's walk, N-1446 p3038 4).
pub(crate) fn node_word(a: &GraphAnswer, n: &docket_machine::graph::Node) -> &'static str {
    if n.kind == NodeKind::Text && started_with(a, &n.id).next().is_some() {
        return "system prompt";
    }
    kind_words(n.kind).0
}

/// The edges of slice A from the starts that were started with text ID (docket-machine graph.rs `StartedWith`), oldest
/// first, as the slice gives its edges.
fn started_with<'a>(a: &'a GraphAnswer, id: &'a str) -> impl Iterator<Item = &'a docket_machine::graph::Edge> + 'a {
    a.slice.edges.iter().map(|e| &e.edge).filter(move |e| e.to == id && e.kind == docket_machine::graph::EdgeKind::StartedWith)
}

/// Whose system prompt text ID of slice A is: the agent whose starts were started with it, by the name a start's id
/// holds (`start:NAME@LINE`, as docket-machine graph.rs's `began` names it), and whether its newest such start had it in
/// place of Claude Code's own (the edge's words, [`docket_machine::graph::IN_PLACE`]). None for a text no start was
/// started with, and for one the starts of two agents were: the standing text every agent of a project starts with is
/// no one agent's to edit from its card on the Map (cloud.rs `card`; paper-research N-1446 p3041, 3).
pub(crate) fn prompt_of(a: &GraphAnswer, id: &str) -> Option<(String, bool)> {
    let mut whose: Option<(&str, bool)> = None;
    for e in started_with(a, id) {
        let name = e.from.strip_prefix("start:")?.rsplit_once('@')?.0;
        if whose.is_some_and(|(w, _)| w != name) {
            return None;
        }
        whose = Some((name, e.what.contains(docket_machine::graph::IN_PLACE)));
    }
    whose.map(|(w, replace)| (w.to_string(), replace))
}

/// The order the line counts kinds in.
const KINDS: [NodeKind; 21] = [
    NodeKind::Post,
    NodeKind::Ticket,
    NodeKind::Turn,
    NodeKind::Start,
    NodeKind::Slice,
    NodeKind::Step,
    NodeKind::Call,
    NodeKind::Template,
    NodeKind::CallFile,
    NodeKind::Program,
    NodeKind::Handler,
    NodeKind::Commit,
    NodeKind::Job,
    NodeKind::Build,
    NodeKind::Branch,
    NodeKind::Worktree,
    NodeKind::Session,
    NodeKind::Request,
    NodeKind::Tools,
    NodeKind::Text,
    NodeKind::File,
];

/// The line under the switches, always there: what is on, what the slice holds by kind, the focus and its hops, who
/// and the items, what the limit keeps and left out, and what has no time to be drawn at (paper-research N-1446 p1385:
/// "the line under the switches says the focus as id plus label"). A: the answer held, to ASK; none yet: what is asked.
pub fn line(ask: &Ask, a: Option<&GraphAnswer>) -> String {
    let mut parts = vec![format!("On: {}", on(ask).into_iter().map(word).collect::<Vec<_>>().join(", "))];
    if let Some(a) = a {
        let mut by: BTreeMap<usize, usize> = BTreeMap::new();
        for n in &a.slice.nodes {
            if let Some(k) = KINDS.iter().position(|k| *k == n.node.kind) {
                *by.entry(k).or_insert(0) += 1;
            }
        }
        parts.push(if by.is_empty() { "nothing in reach".to_string() } else { by.iter().map(|(k, n)| counted(KINDS[*k], *n)).collect::<Vec<_>>().join(", ") });
    }
    if let Some(at) = &ask.around {
        let label = a.and_then(|a| a.labels.iter().find(|l| l.id == *at)).map(|l| l.label.as_str()).filter(|l| !l.is_empty());
        let hops = if ask.hops == 0 { HOPS } else { ask.hops };
        parts.push(match (a.is_some_and(|a| a.slice.unknown), label) {
            (true, _) => format!("around {at}, which the graph does not hold"),
            (false, Some(l)) => format!("around {at} “{}”, {hops} hops", crate::refs::cut(l, 48)),
            (false, None) => format!("around {at}, {hops} hops"),
        });
    }
    if !ask.who.is_empty() {
        parts.push(format!("who: {}", ask.who.join(", ")));
    }
    if !ask.items.is_empty() {
        parts.push(format!("item: {}", ask.items.join(", ")));
    }
    if let Some(a) = a {
        // what the limit keeps (docket-machine's graph_slice `nearest`): around a node, the nearest hops first; in the
        // whole graph, the newest first (paper-research N-1446 p1772)
        let shown = a.slice.edges.len();
        let first = if ask.around.is_some() { "the nearest first" } else { "the newest first" };
        parts.push(match a.slice.left {
            0 => "nothing left out".to_string(),
            n => format!("{shown} edges of {}, {first}: {n} left out", shown + n),
        });
        let untimed = lay(a, &ask.layers, &BTreeSet::new()).untimed;
        if untimed > 0 {
            parts.push(format!("{untimed} with no time, not drawn"));
        }
    }
    parts.join(" · ")
}

/// What an agent of NOW is doing, the now lane's columns in paper-research's order (N-1446 p1469, 6: "in a turn
/// since when, its last post, its last job green or red, its item"): its turn; its last post with its first words as
/// LABELS has them (its name before them left out: the row says it); its last job, its state first, then what it ran;
/// its items started and not closed. Empty where it has none.
fn doing(a: &docket_machine::graph::AgentNow, labels: &HashMap<&str, &str>) -> [String; 4] {
    let turn = match (&a.turn, &a.idle_since) {
        (Some(t), _) => format!("in a turn since {}", crate::when::at(&t.since)),
        (None, Some(i)) => format!("idle since {}", crate::when::at(i)),
        (None, None) => "no turn yet".to_string(),
    };
    let post = a.last_post.as_ref().map_or(String::new(), |p| {
        let words = labels.get(p.id.as_str()).map_or(String::new(), |l| format!(" “{}”", crate::refs::cut(l.strip_prefix(&format!("{}: ", a.agent)).unwrap_or(l), 40)));
        format!("last post {}{words} {}", p.id, crate::when::at(&p.at))
    });
    let job = a.last_job.as_ref().map_or(String::new(), |j| {
        let state = match (j.ended.as_ref(), j.ok) {
            (None, _) => "running".to_string(),
            (Some(e), Some(true)) => format!("green at {}", crate::when::at(e)),
            (Some(e), Some(false)) => format!("red at {}", crate::when::at(e)),
            (Some(e), None) => format!("ended at {}", crate::when::at(e)),
        };
        if j.what.is_empty() { format!("job {state}") } else { format!("job {state}, {}", j.what) }
    });
    [turn, post, job, a.items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>().join(", ")]
}

/// A point of his ask's course as the page says it ([`course`]): the point, who or what reached it (an agent by its
/// name, a commit, a job, a build), its time in seconds, and the span to it from the point reached before it in time.
/// None for the first.
#[derive(Clone, Debug, PartialEq)]
pub struct Leg {
    pub point: Point,
    pub by: String,
    pub at: i64,
    pub took: Option<i64>,
}

/// Which span of a [`Course`] is the longest: a leg's, or the open one to now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Longest {
    Leg(usize),
    Open,
}

/// His ask's course as spans (paper-research N-1446 p1429, 1): the points the graph reached, each with the span to it;
/// the open span from the last to now while it is not in his window yet; the points not reached; and the longest span,
/// by the times alone, the first of equals. No threshold of ours says what is long.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Course {
    pub legs: Vec<Leg>,
    pub open: Option<i64>,
    pub not_yet: Vec<Point>,
    pub longest: Option<Longest>,
}

/// Course T (`AskTimes::points`) at NOW, coordd's clock in seconds; with no NOW, no open span. The points reached go
/// in the order the graph reached them, the course's own order among equal times, so each span runs from one point to
/// the next and the spans add up to the whole: a job for its item green before the commit, or the ledger's close after
/// the merge, takes its place in time and hides no span. The ASKED ledger's close (docket-edge5 N-1446 p1464: every
/// agent it asks has answered) is one point and ends nothing: the span to now runs on while his ask is not in his
/// window (paper-research p1469: "Keep it exactly as it is"; on his record an answer that takes his ask closes it in
/// the ledger at that moment, the work still to come). Where the ledger asks no one it never closes, so `Closed` is
/// not among the points not reached there.
pub fn course(t: &AskTimes, now: Option<i64>) -> Course {
    let mut c = Course::default();
    let mut reached = Vec::new();
    for (point, at, by) in t.points() {
        match at.and_then(|a| docket_machine::machine::secs(a)) {
            Some(at) => reached.push((at, point, by.map(|r| r.id.strip_prefix("agent:").unwrap_or(&r.id).to_string()).unwrap_or_default())),
            None if point == Point::Closed && t.asks.is_empty() => {}
            None => c.not_yet.push(point),
        }
    }
    // stable: equal times keep the course's order
    reached.sort_by_key(|(at, _, _)| *at);
    let mut last: Option<i64> = None;
    for (at, point, by) in reached {
        c.legs.push(Leg { point, by, at, took: last.map(|l| at - l) });
        last = Some(at);
    }
    if t.in_window.is_none() {
        c.open = last.zip(now).map(|(l, n)| (n - l).max(0));
    }
    if c.open.is_some() {
        c.not_yet.retain(|p| *p != Point::InWindow);
    }
    // a span of no time is no span: the longest is longer than that
    let mut most = 0;
    for (i, l) in c.legs.iter().enumerate() {
        if let Some(s) = l.took.filter(|s| *s > most) {
            most = s;
            c.longest = Some(Longest::Leg(i));
        }
    }
    if c.open.is_some_and(|s| s > most) {
        c.longest = Some(Longest::Open);
    }
    c
}

/// A point's word.
pub(crate) fn point_word(p: Point) -> &'static str {
    match p {
        Point::Asked => "asked",
        Point::Reached => "reached",
        Point::Taken => "taken",
        Point::Closed => "closed",
        Point::FirstCommit => "first commit",
        Point::Merged => "merged",
        Point::FirstGreen => "first green",
        Point::InWindow => "in your window",
    }
}

/// A leg in words, with who or what reached it: "reached docket-edge5", "taken by docket-edge5", "first commit
/// 0f40917b", "first green job 1-1"; his own post and his window by their word alone. Open → done's asks row says
/// where an ask stands in these words (insight.rs `stands`).
pub(crate) fn leg_words(l: &Leg) -> String {
    let by = match l.by.split_once(':') {
        Some(("commit", c)) => c.chars().take(8).collect(),
        Some(("job", j)) => format!("job {j}"),
        Some(("build", _)) => String::new(),
        _ => l.by.clone(),
    };
    match l.point {
        Point::Asked | Point::InWindow => point_word(l.point).to_string(),
        _ if by.is_empty() => point_word(l.point).to_string(),
        Point::Taken => format!("taken by {by}"),
        Point::Closed => format!("closed by {by}"),
        p => format!("{} {by}", point_word(p)),
    }
}

// ---- the drawing ----------------------------------------------------------------------------------------------------

/// The page: the switches and their line over the canvas.
pub fn page(s: &App, k: &Kit) -> El {
    let v = &s.lanes;
    let held = held(s);
    let asked_now = wanted(s);
    let mut said = match held {
        Some((ask, a)) => line(ask, Some(a)),
        None => asked_now.as_ref().map(|w| line(w, None)).unwrap_or_default(),
    };
    if v.asked.is_some() && held.is_some() {
        said.push_str(" · reading…");
    }
    let body = match (v.got.as_ref().filter(|g| g.0 == s.cur).map(|g| &g.2), &v.ask.around) {
        (Some(Ok(a)), _) => board(s, a, k),
        (Some(Err(e)), _) => note(e, k.c.danger.fg, k),
        (None, None) => note("No post of yours yet.", k.c.text_muted, k),
        (None, Some(_)) => note("Loading…", k.c.text_muted, k),
    };
    let strip = v.got.as_ref().filter(|g| g.0 == s.cur).and_then(|g| g.2.as_ref().ok()).and_then(|a| course_strip(a, k));
    col().key("lanes-page").w_full().h_full().child(switches(s, k)).child(text(said).font(k.ty.small).color(k.c.text_muted).wrap().key("lanes-line").px(12.0).py(4.0)).children(strip).child(body)
}

/// His ask's course under the line while the focus is his post (`GraphAnswer::course`; paper-research N-1446 p1429,
/// 1): each point reached, who or what reached it and when, the span to it from the one before, the longest standing
/// out; while it is not in his window yet, the span to now; then the points not reached.
fn course_strip(a: &GraphAnswer, k: &Kit) -> Option<El> {
    let now = a.now.as_ref().and_then(|n| docket_machine::machine::secs(&n.at));
    let c = course(a.course.as_ref()?, now);
    let span = |words: String, longest: bool| -> El {
        let t = text(words).font(if longest { k.ty.small_medium } else { k.ty.small }).color(if longest { k.c.text_accent } else { k.c.text_muted }).single_line();
        let b = row().px(6.0).py(2.0).radius(4.0).child(t);
        if longest { b.fill(faded(k.c.text_accent, 0.14)).key("lanes-course-longest") } else { b }
    };
    let cell = |key: String, words: String, under: String| -> El {
        col().key(key).child(text(words).font(k.ty.small).color(k.c.text_base).single_line()).child(text(under).font(k.ty.small).color(k.c.text_muted).single_line())
    };
    let mut r = row().key("lanes-course").w_full().flex_wrap().gap(6.0).px(12.0).py(4.0).items_center().child(text("Its course".to_string()).font(k.ty.small_medium).color(k.c.text_base).single_line());
    for (i, l) in c.legs.iter().enumerate() {
        if i > 0 {
            r = r.child(match l.took {
                Some(took) if took > 0 => span(format!("{} →", crate::when::took(took)), c.longest == Some(Longest::Leg(i))),
                // at the same moment as the point before: no span
                _ => text("·".to_string()).font(k.ty.small).color(k.c.text_faint).single_line(),
            });
        }
        r = r.child(cell(format!("lanes-course-{i}"), leg_words(l), crate::when::secs(l.at)));
    }
    if let (Some(took), Some(n)) = (c.open, now) {
        r = r.child(span(format!("{} so far →", crate::when::took(took)), c.longest == Some(Longest::Open))).child(cell("lanes-course-open".into(), "not in your window yet".into(), crate::when::secs(n)));
    }
    if !c.not_yet.is_empty() {
        r = r.child(text(format!("not reached: {}", c.not_yet.iter().map(|p| point_word(*p)).collect::<Vec<_>>().join(", "))).font(k.ty.small).color(k.c.text_faint).single_line().key("lanes-course-not-yet"));
    }
    Some(r)
}

fn note(why: &str, ink: Color, k: &Kit) -> El {
    col().key("lanes-note").w_full().grow(1.0).padding(24.0).child(text(why.to_string()).font(k.ty.body).color(ink).wrap())
}

/// The switches by kind, and the focus's hops, who and the item, and "Read again": a press on a switch or a button gives
/// the keys back to the Map (canvas.rs `bar_press`, paper-research N-1446 p2033), who and the item keep theirs.
fn switches(s: &App, k: &Kit) -> El {
    let v = &s.lanes;
    let on_now = on(&v.ask);
    let mut r = row().key("lanes-switches").w_full().gap(12.0).px(12.0).py(6.0);
    for l in Layer::ALL {
        let is = on_now.contains(&l);
        r = r.child(Switch::new(is).label(word(l)).on_toggle_tx(bar_press(move |s: &mut App| toggle(s, l))).build(k).key(format!("lanes-switch-{l:?}")));
    }
    let hops = if v.ask.hops == 0 { HOPS } else { v.ask.hops };
    let step = move |d: i32| move |s: &mut App| s.lanes.ask.hops = (hops as i32 + d).clamp(1, MOST_HOPS as i32) as u32;
    let fewer = Button::new("−").variant(ButtonVariant::Ghost).help("Fewer hops").on_press_tx(bar_press(step(-1))).build(k).key("lanes-fewer");
    let more = Button::new("+").variant(ButtonVariant::Ghost).help("More hops").on_press_tx(bar_press(step(1))).build(k).key("lanes-more");
    let newest = Button::new("Your newest post").variant(ButtonVariant::Ghost).help("Around your newest post").on_press_tx(bar_press(|s: &mut App| {
        if let Some(id) = his_newest(s, s.cur) {
            focus(s, &id);
        }
    }));
    let again = Button::new("Read again").variant(ButtonVariant::Ghost).help("As it stands now").on_press_tx(bar_press(|s: &mut App| s.lanes.again = true));
    let who = col().key("lanes-who").child(TextInput::new(&v.who).placeholder("Who").width(Some(120.0)).on_edit(|s: &mut App| &mut s.lanes.who).build(k));
    let item = col().key("lanes-item").child(TextInput::new(&v.item).placeholder("Item").width(Some(110.0)).on_edit(|s: &mut App| &mut s.lanes.item).build(k));
    let hops_said = text(format!("{hops} hops")).font(k.ty.small).color(k.c.text_muted).single_line();
    r.child(row().gap(2.0).child(fewer).child(hops_said).child(more)).child(who).child(item).child(newest.build(k)).child(again.build(k))
}

/// The canvas of answer A.
fn board(s: &App, a: &GraphAnswer, k: &Kit) -> El {
    let v = &s.lanes;
    let ask = held(s).map(|x| x.0.clone()).unwrap_or_default();
    let laid = lay(a, &ask.layers, &v.open);
    let (tops, _) = tops(&laid);
    let [w, h] = [v.area[2], v.area[3]];
    let labels: HashMap<&str, &str> = a.labels.iter().map(|l| (l.id.as_str(), l.label.as_str())).collect();
    // its presses the Map's, as canvas.rs `board`'s: a drag pans and a click opens, so not the window's page
    let mut c = col().key("lanes-canvas").w_full().grow(1.0).h(0.0).clip_rounded(0.0).fill(k.c.bg_deep).cursor(rune::Cursor::Grab).select_texts(false);
    // the lanes' bands, every other one a shade apart
    let mut bands = Shape::new();
    for (li, (_, rows)) in laid.lanes.iter().enumerate() {
        let (top, high) = (tops[li] + v.dy, HEAD + rows.len().max(1) as f32 * ROW);
        let shade = if li % 2 == 0 { faded(k.c.layer_01, 0.6) } else { faded(k.c.layer_02, 0.4) };
        bands = bands.fill(vec![[0.0, top], [w, top], [w, top + high], [0.0, top + high]], shade);
    }
    c = c.child(canvas(bands).absolute(0.0, 0.0).size(w.max(1.0), h.max(1.0)));
    c = c.children(breaks(s, k));
    c = c.children(course_line(s, a, &laid, &tops, k));
    if far(v) {
        c = c.children(hours(s, a, &laid, &tops, k));
    } else {
        c = c.children(cards(s, a, &laid, &labels, k));
    }
    if let Some(now) = &a.now {
        c = c.children(now_edge(s, now, &laid, &tops, &labels, k));
    }
    // the lanes' and rows' names over what passes under them at the left
    let mut names = col().key("lanes-names").absolute(0.0, 0.0).size(LEFT, h.max(1.0)).fill(k.c.bg_deep);
    for (li, (layer, rows)) in laid.lanes.iter().enumerate() {
        let (lane_word, lane_ink) = match layer {
            Some(l) => (word(*l).to_string(), ink(*l, k)),
            None => ("now".to_string(), k.c.text_base),
        };
        names = names.child(text(lane_word).font(k.ty.small_medium).color(lane_ink).single_line().absolute(10.0, tops[li] + v.dy + 2.0).key(format!("lanes-lane-{li}")));
        for (ri, who) in rows.iter().enumerate() {
            // his asks' rows say their count, whole, past the names' width (p1510, 2; p1532), their words past them
            // (`ask_words_x`); every other name cut at the names' width
            let (said, key, whole) = match (layer, who.as_str()) {
                (None, row @ (NOT_TAKEN | OPEN)) => {
                    let key = if row == NOT_TAKEN { "lanes-asks-name" } else { "lanes-open-name" };
                    (ask_name(row, a.now.as_ref()).unwrap_or_default(), key.to_string(), true)
                }
                (_, "") => ("others".to_string(), format!("lanes-name-{li}-{ri}"), false),
                (_, "owner") => ("you".to_string(), format!("lanes-name-{li}-{ri}"), false),
                (_, w) => (w.to_string(), format!("lanes-name-{li}-{ri}"), false),
            };
            let mut name = text(said).font(k.ty.small).color(k.c.text_muted).single_line();
            if !whole {
                name = name.truncate().w(LEFT - 26.0);
            }
            names = names.child(name.absolute(20.0, row_y(v, &tops, li, ri) - 8.0).key(key));
        }
    }
    c = c.child(names);
    c = c.children(asks_listed(s, a, &laid, &tops, &labels, k));
    if let (Some(id), Some(q)) = (&v.hover, v.pointer) {
        if let Some(i) = a.slice.nodes.iter().position(|n| n.node.id == *id) {
            c = c.child(preview(a, i, &labels, q, [w, h], k));
        } else if let Some(f) = laid.fans.iter().find(|f| f.id == *id) {
            c = c.child(fan_preview(a, f, q, [w, h], k));
        }
    }
    c.on_layout(|s: &mut App, r| laid_out(s, r))
        .on_drag(|s: &mut App, d: &mut rune::Drag, _tx: &mut Tx| drag(s, d))
        .on_wheel(|s: &mut App, e: WheelEvent, _tx: &mut Tx| wheel(s, &e))
        .on_magnify(|s: &mut App, e: MagnifyEvent, _tx: &mut Tx| magnify(s, &e))
        .on_pointer_move_local(|s: &mut App, p: PointerEvent, tx: &mut Tx| {
            // nothing hovered before or now: no view built (canvas.rs's move, docket-edge6 N-1446 p2356, 3)
            if !pointed(s, Some([p.x, p.y])) {
                tx.live_only();
            }
        })
        .on_hover_change(|s: &mut App, inside: bool, _tx: &mut Tx| {
            s.wheel_taken = inside;
            if !inside {
                pointed(s, None);
            }
        })
}

/// Close in (paper-research N-1446 p1469, 1, 2 and 4: p603's "explored by the user / navigated"): only the edges that
/// touch the focus or the card under the pointer are drawn, each card counting the others at it ("+12"); then each card's
/// dot and each fan's ring, its words where its row has room before the next one, and the marks of the fans he opened.
/// A fan's one edge to its card is drawn as the card's are.
fn cards(s: &App, a: &GraphAnswer, laid: &Laid, labels: &HashMap<&str, &str>, k: &Kit) -> Vec<El> {
    let v = &s.lanes;
    let [w, h] = [v.area[2], v.area[3]];
    let ds: HashMap<String, [f32; 2]> = dots(v, a, laid).into_iter().collect();
    let lit = |id: &str| v.hover.as_deref() == Some(id) || v.ask.around.as_deref() == Some(id);
    let lane_ink = |lane: usize| laid.lanes[lane].0.map(|l| ink(l, k)).unwrap_or(k.c.text_base);
    let mut out = Vec::new();
    let mut lines = Shape::new();
    // the edges not drawn, by the card at each end
    let mut hidden: HashMap<&str, usize> = HashMap::new();
    for e in &a.slice.edges {
        let (Some(p), Some(q)) = (ds.get(&e.edge.from), ds.get(&e.edge.to)) else { continue };
        if lit(&e.edge.from) || lit(&e.edge.to) {
            lines = lines.line(*p, *q, 1.5, LineCap::Round, faded(ink(e.layer, k), 0.95));
        } else {
            *hidden.entry(e.edge.from.as_str()).or_default() += 1;
            *hidden.entry(e.edge.to.as_str()).or_default() += 1;
        }
    }
    for f in &laid.fans {
        let (Some(p), Some(q)) = (ds.get(&f.node), ds.get(&f.id)) else { continue };
        if lit(&f.node) || lit(&f.id) {
            lines = lines.line(*p, *q, 1.5, LineCap::Round, faded(lane_ink(f.lane), 0.95));
        } else {
            *hidden.entry(f.node.as_str()).or_default() += 1;
        }
    }
    // the marks of the fans he opened, each a line from its other end's card to the agent's row
    let (tops, _) = tops(laid);
    let mut marks = Vec::new();
    for &(ei, lane, row, t) in &laid.marks {
        let e = &a.slice.edges[ei];
        let m = [x_of(v, t), row_y(v, &tops, lane, row)];
        let other = if e.edge.from.starts_with("agent:") { &e.edge.to } else { &e.edge.from };
        if let Some(p) = ds.get(other) {
            lines = lines.line(*p, m, 1.5, LineCap::Round, faded(ink(e.layer, k), if lit(other.as_str()) { 0.95 } else { 0.5 }));
        }
        marks.push(el().key(format!("lanes-mark-{ei}")).absolute(m[0] - DOT / 2.0 + 1.0, m[1] - DOT / 2.0 + 1.0).size(DOT - 2.0, DOT - 2.0).radius(DOT / 2.0).border(1.5, ink(e.layer, k)));
    }
    out.push(canvas(lines).absolute(0.0, 0.0).size(w.max(1.0), h.max(1.0)).key("lanes-edges"));
    out.extend(marks);
    // the cards and the fans by row, for the room each one's words have: up to the next in its row
    let mut by_row: BTreeMap<(usize, usize), Vec<(i64, Result<usize, usize>)>> = BTreeMap::new();
    for &(i, lane, row, t) in &laid.cards {
        by_row.entry((lane, row)).or_default().push((t, Ok(i)));
    }
    for (fi, f) in laid.fans.iter().enumerate() {
        by_row.entry((f.lane, f.row)).or_default().push((f.at, Err(fi)));
    }
    for ((lane, _), mut row) in by_row {
        row.sort();
        for (j, &(_, which)) in row.iter().enumerate() {
            let (id, words, dot) = match which {
                Ok(i) => {
                    let n = &a.slice.nodes[i];
                    let dot_ink = match (n.node.kind, n.node.ok) {
                        (NodeKind::Job, Some(true)) => k.c.success.fg,
                        (NodeKind::Job, Some(false)) => k.c.danger.fg,
                        (NodeKind::Job, None) => k.c.warning.fg,
                        _ => lane_ink(lane),
                    };
                    let mut dot = el().key(format!("lanes-card-{}", n.node.id)).size(DOT, DOT).radius(DOT / 2.0).fill(dot_ink);
                    if v.ask.around.as_deref() == Some(n.node.id.as_str()) {
                        dot = dot.border(2.0, k.c.text_base);
                    }
                    let words = labels.get(n.node.id.as_str()).copied().filter(|l| !l.is_empty()).unwrap_or(n.node.id.as_str()).to_string();
                    (n.node.id.as_str(), words, dot)
                }
                Err(fi) => {
                    let f = &laid.fans[fi];
                    let ring = el().key(format!("lanes-{}", f.id)).size(DOT, DOT).radius(DOT / 2.0).border(2.0, lane_ink(lane)).fill(k.c.bg_deep);
                    (f.id.as_str(), fan_words(a, f), ring)
                }
            };
            let Some(&[x, y]) = ds.get(id) else { continue };
            if x < LEFT - DOT || x > w + DOT || y < -ROW || y > h + ROW {
                continue;
            }
            out.push(dot.absolute(x - DOT / 2.0, y - DOT / 2.0));
            let mut left = x + DOT / 2.0 + 4.0;
            let next = row.get(j + 1).map(|(t2, _)| x_of(v, *t2)).unwrap_or(f32::MAX);
            let shade = if lit(id) { k.c.text_base } else { k.c.text_muted };
            // the edges at it not drawn
            if let Some(n) = hidden.get(id).filter(|n| **n > 0) {
                let said = format!("+{n}");
                let wide = 7.0 * said.len() as f32 + 2.0;
                if next - left - 4.0 >= wide {
                    out.push(text(said).font(k.ty.small).color(k.c.text_faint).single_line().absolute(left, y - 8.0).key(format!("lanes-more-{id}")));
                    left += wide + 2.0;
                }
            }
            let room = (next - left - 4.0).min(WORDS);
            if room >= WORDS_LEAST {
                out.push(text(words).font(k.ty.small).color(shade).single_line().truncate().w(room).absolute(left, y - 8.0).key(format!("lanes-words-{id}")));
            }
        }
    }
    out
}

/// The folds in view: each a band a shade off the canvas, its length over it ("— 2 h 10 m —") where it fits
/// (paper-research N-1446 p1469, 5).
fn breaks(s: &App, k: &Kit) -> Vec<El> {
    let v = &s.lanes;
    let [w, h] = [v.area[2], v.area[3]];
    let mut sh = Shape::new();
    let mut said = Vec::new();
    for f in &v.folds {
        let (x0, x1) = (x_of(v, f.from), x_of(v, f.to));
        if x1 < LEFT || x0 > w || x1 - x0 < 12.0 {
            continue;
        }
        let (p, q) = (x0 + 5.0, x1 - 5.0);
        sh = sh.fill(vec![[p, 0.0], [q, 0.0], [q, h.max(1.0)], [p, h.max(1.0)]], faded(k.c.bg_base, 0.8));
        if x1 - x0 >= 64.0 {
            let words = text(format!("— {} —", crate::when::took(f.to - f.from))).font(k.ty.small).color(k.c.text_faint).single_line();
            said.push(row().key(format!("lanes-fold-{}", f.from)).absolute(x0, 2.0).w(x1 - x0).center().child(words));
        }
    }
    let mut out = vec![canvas(sh).absolute(0.0, 0.0).size(w.max(1.0), h.max(1.0)).key("lanes-folds")];
    out.extend(said);
    out
}

/// The hover's card for fan F at Q: what it did and when, each of its agents, and that a click opens it.
fn fan_preview(a: &GraphAnswer, f: &Fan, q: [f32; 2], view: [f32; 2], k: &Kit) -> El {
    let (cw, ch) = (300.0, 96.0);
    let x = if q[0] + 16.0 + cw > view[0] { (q[0] - 16.0 - cw).max(0.0) } else { q[0] + 16.0 };
    let y = if q[1] + 16.0 + ch > view[1] { (q[1] - 16.0 - ch).max(0.0) } else { q[1] + 16.0 };
    let head = format!("{} {} · from {}", fan_verb(a, f), f.agents.len(), crate::when::secs(f.at));
    col().key("lanes-preview")
        .absolute(x, y)
        .w(cw)
        .padding(8.0)
        .gap(2.0)
        .fill(k.c.surface_raised)
        .border(1.0, k.c.border_base)
        .radius(k.radius.sm)
        .child(text(format!("{} · {head}", f.node)).font(k.ty.small_medium).color(k.c.text_base).single_line().truncate())
        .child(text(f.agents.join(", ")).font(k.ty.small).color(k.c.text_muted).wrap())
        .child(text("a click opens it into its agents".to_string()).font(k.ty.small).color(k.c.text_faint).wrap())
}

/// His ask's course along his post's row (`GraphAnswer::course`), at its foot: a tick at each point reached, a line
/// along each span, the longest thick with its time over it, and a thin one on to now while it is not in his window.
fn course_line(s: &App, a: &GraphAnswer, laid: &Laid, tops: &[f32], k: &Kit) -> Vec<El> {
    let v = &s.lanes;
    let Some(t) = a.course.as_ref() else { return Vec::new() };
    let Some(&(_, lane, row, _)) = laid.cards.iter().find(|c| a.slice.nodes[c.0].node.id == t.post) else { return Vec::new() };
    let now = a.now.as_ref().and_then(|n| docket_machine::machine::secs(&n.at));
    let c = course(t, now);
    let y = row_y(v, tops, lane, row) + ROW / 2.0 - 3.0;
    let [w, h] = [v.area[2], v.area[3]];
    let ink = k.c.text_accent;
    let mut sh = Shape::new();
    let mut said = None;
    let mut last: Option<f32> = None;
    for (i, l) in c.legs.iter().enumerate() {
        let x = x_of(v, l.at);
        if let (Some(p), Some(took)) = (last, l.took.filter(|s| *s > 0)) {
            let longest = c.longest == Some(Longest::Leg(i));
            sh = sh.line([p, y], [x, y], if longest { 4.0 } else { 1.5 }, LineCap::Round, faded(ink, if longest { 1.0 } else { 0.55 }));
            if longest {
                said = Some((took, (p + x) / 2.0));
            }
        }
        sh = sh.line([x, y - 4.0], [x, y + 4.0], 1.5, LineCap::Butt, ink);
        last = Some(x);
    }
    if let (Some(p), Some(n), Some(took)) = (last, now, c.open) {
        let (x, longest) = (x_of(v, n), c.longest == Some(Longest::Open));
        sh = sh.line([p, y], [x, y], if longest { 4.0 } else { 1.0 }, LineCap::Round, faded(ink, if longest { 0.8 } else { 0.35 }));
        if longest {
            said = Some((took, (p + x) / 2.0));
        }
    }
    let mut out = vec![canvas(sh).absolute(0.0, 0.0).size(w.max(1.0), h.max(1.0)).key("lanes-course-line")];
    if let Some((took, x)) = said {
        out.push(text(crate::when::took(took)).font(k.ty.small_medium).color(ink).single_line().absolute(x - 14.0, y + 2.0).key("lanes-course-line-longest"));
    }
    out
}

/// Far out: each lane its counts per hour (`Slice::hours`), the shade by the count, the count where it fits.
fn hours(s: &App, a: &GraphAnswer, laid: &Laid, tops: &[f32], k: &Kit) -> Vec<El> {
    let v = &s.lanes;
    let [w, _] = [v.area[2], v.area[3]];
    let most = a.slice.hours.values().flat_map(|h| h.values()).copied().max().unwrap_or(1).max(1) as f32;
    let mut out = Vec::new();
    let mut sh = Shape::new();
    for (li, (layer, rows)) in laid.lanes.iter().enumerate() {
        let Some(layer) = layer else { continue };
        let (top, high) = (tops[li] + v.dy + HEAD, rows.len().max(1) as f32 * ROW);
        for (hour, n) in a.slice.hours.get(layer).into_iter().flatten() {
            let Some(t) = docket_machine::machine::secs(&format!("{hour}:00:00Z")) else { continue };
            let (x0, x1) = (x_of(v, t), x_of(v, t + 3600));
            if x1 < LEFT || x0 > w {
                continue;
            }
            let shade = faded(ink(*layer, k), 0.15 + 0.7 * *n as f32 / most);
            sh = sh.fill(vec![[x0, top], [x1 - 1.0, top], [x1 - 1.0, top + high], [x0, top + high]], shade);
            if x1 - x0 >= 22.0 {
                out.push(text(n.to_string()).font(k.ty.small).color(k.c.text_base).single_line().absolute(x0 + 3.0, top + 3.0).key(format!("lanes-hour-{layer:?}-{hour}")));
            }
        }
    }
    let mut all = vec![canvas(sh).absolute(0.0, 0.0).size(w.max(1.0), v.area[3].max(1.0)).key("lanes-hours")];
    all.extend(out);
    all
}

/// Now, on top (paper-research N-1446 p1469, 6): now's line at the right edge; each agent in a turn its row, its turn a
/// bar from when it began, its last post a dot at its time, its last job a bar green, red or still running, and at the
/// edge what it is doing ([`doing`]); then his asks nobody took, and those the ledger holds open with no commit, each
/// row how long its oldest and newest have waited, and its list a click away ([`asks_listed`]).
fn now_edge(s: &App, now: &Now, laid: &Laid, tops: &[f32], labels: &HashMap<&str, &str>, k: &Kit) -> Vec<El> {
    let v = &s.lanes;
    let [w, h] = [v.area[2], v.area[3]];
    let Some(at) = docket_machine::machine::secs(&now.at) else { return Vec::new() };
    let nx = x_of(v, at);
    let Some(lane) = laid.lanes.iter().position(|(l, _)| l.is_none()) else { return Vec::new() };
    let rows = &laid.lanes[lane].1;
    let sh = Shape::new().line([nx, 0.0], [nx, h.max(1.0)], 1.0, LineCap::Butt, faded(k.c.text_faint, 0.9));
    let mut out = vec![text(format!("now, {}", crate::when::at(&now.at))).font(k.ty.small).color(k.c.text_faint).single_line().absolute(nx + 6.0, 2.0).key("lanes-now")];
    // in words across the canvas up to now (to its right edge where now is near the left or out of view), each in its
    // column, cut to fit: the right edge alone cut each row at its job
    let right = if nx.min(w) - LEFT >= 600.0 { nx.min(w) - 8.0 } else { w - 8.0 };
    for (ri, name) in rows.iter().enumerate() {
        let y = row_y(v, tops, lane, ri);
        let asks = match name.as_str() {
            NOT_TAKEN => Some((&now.not_taken, "lanes-asks")),
            OPEN => Some((&now.open, "lanes-open")),
            _ => None,
        };
        if let Some((asks, key)) = asks {
            // his asks, their count in the row's name: how long the oldest and the newest have waited, and the list a
            // click opens (a bar with the newest ask's words read as a single ask: paper-research N-1446 p1510, 2)
            let mark = if v.listed.as_deref() == Some(name.as_str()) { "▾" } else { "▸" };
            let said = match (waited(now, asks.first()), waited(now, asks.last())) {
                (Some(o), Some(n)) if asks.len() > 1 => format!("{mark} oldest {o}, newest {n}"),
                (Some(o), _) => format!("{mark} it has waited {o}"),
                _ => format!("{mark} the list"),
            };
            let x = ask_words_x(&ask_name(name, Some(now)).unwrap_or_default(), k);
            out.push(text(said).font(k.ty.small).color(k.c.warning.fg).single_line().truncate().w((right - x).max(0.0)).absolute(x, y - 8.0).key(key));
            continue;
        }
        let Some(a) = now.agents.iter().find(|a| a.agent == *name) else { continue };
        let job_ink = match a.last_job.as_ref().map(|j| (j.ended.is_some(), j.ok)) {
            Some((false, _)) => k.c.warning.fg,
            Some((true, Some(true))) => k.c.success.fg,
            Some((true, Some(false))) => k.c.danger.fg,
            _ => k.c.text_muted,
        };
        let inks = [k.c.text_muted, k.c.text_base, job_ink, k.c.text_muted];
        // the canvas's points start at the rows' names: the columns start right of them
        let mut x = LEFT + 8.0;
        for (i, said) in doing(a, labels).into_iter().enumerate() {
            let wide = NOW_COLUMNS.get(i).copied().unwrap_or(f32::MAX).min(right - x);
            if wide < WORDS_LEAST {
                break;
            }
            if !said.is_empty() {
                out.push(text(said).font(k.ty.small).color(inks[i]).single_line().truncate().w(wide).absolute(x, y - 8.0).key(format!("lanes-doing-{i}-{name}")));
            }
            x += wide + 12.0;
        }
    }
    let mut all = vec![canvas(sh).absolute(0.0, 0.0).size(w.max(1.0), h.max(1.0)).key("lanes-now-lines")];
    all.extend(out);
    all
}

/// How long his ask POST has waited by NOW, in words: from when he asked to now's time.
fn waited(now: &Now, post: Option<&String>) -> Option<String> {
    let at = docket_machine::machine::secs(&now.at)?;
    let asked = now.asks.iter().find(|a| Some(&a.times.post) == post)?.times.asked.as_deref().and_then(docket_machine::machine::secs)?;
    Some(crate::when::took(at - asked))
}

/// The list of his asks a row of them opened ([`ask_list`]), on a raised sheet over the lanes: a line each, its post's
/// id, its first words and how long it has waited, the one under the pointer lit; a click on one makes it the focus.
fn asks_listed(s: &App, a: &GraphAnswer, laid: &Laid, tops: &[f32], labels: &HashMap<&str, &str>, k: &Kit) -> Vec<El> {
    let v = &s.lanes;
    let Some(now) = &a.now else { return Vec::new() };
    let (lines, more) = ask_list(v, now, laid, tops);
    let Some(first) = lines.first().map(|l| l.1) else { return Vec::new() };
    let high = (lines.len() + usize::from(more > 0)) as f32 * ASK_LINE;
    let mut out = vec![el().key("lanes-asks-list").absolute(first[0] - 6.0, first[1] - 4.0).size(first[2] + 12.0, high + 8.0).radius(k.radius.sm).fill(k.c.surface_raised).border(1.0, k.c.border_base)];
    for (post, r) in &lines {
        // a post's label is "BY: its first line", and these are all his
        let words = labels.get(post.as_str()).map_or(String::new(), |l| crate::refs::cut(l.strip_prefix("owner: ").unwrap_or(l), 90));
        let mut line = row()
            .key(format!("lanes-asks-line-{post}"))
            .absolute(r[0], r[1])
            .size(r[2], r[3])
            .gap(10.0)
            .px(4.0)
            .center_y()
            .child(text(post.clone()).font(k.ty.small_medium).color(k.c.text_base).single_line())
            .child(text(words).font(k.ty.small).color(k.c.text_muted).single_line().truncate().grow(1.0).shrink().w(0.0))
            .child(text(waited(now, Some(post)).unwrap_or_default()).font(k.ty.small).color(k.c.text_faint).single_line());
        if v.hover.as_deref() == Some(post.as_str()) {
            line = line.radius(k.radius.sm).fill(k.c.layer_02);
        }
        out.push(line);
    }
    if more > 0 {
        let y = first[1] + lines.len() as f32 * ASK_LINE + 3.0;
        out.push(text(format!("and {more} more, which a taller window shows")).font(k.ty.small).color(k.c.text_faint).single_line().absolute(first[0] + 4.0, y).key("lanes-asks-more"));
    }
    out
}

/// The hover's card for node I of A at Q: its kind, who, when, its label, and its edges in the slice by layer.
fn preview(a: &GraphAnswer, i: usize, labels: &HashMap<&str, &str>, q: [f32; 2], view: [f32; 2], k: &Kit) -> El {
    let n = &a.slice.nodes[i];
    let mut by: BTreeMap<Layer, usize> = BTreeMap::new();
    for e in a.slice.edges.iter().filter(|e| e.edge.from == n.node.id || e.edge.to == n.node.id) {
        *by.entry(e.layer).or_insert(0) += 1;
    }
    let head = [Some(node_word(a, &n.node).to_string()), n.by.clone().map(|b| if b == "owner" { "yours".to_string() } else { format!("by {b}") }), n.node.when.as_deref().map(crate::when::at)].into_iter().flatten().collect::<Vec<_>>().join(" · ");
    let words = labels.get(n.node.id.as_str()).copied().unwrap_or("");
    let edges = by.iter().map(|(l, c)| format!("{c} {}", word(*l))).collect::<Vec<_>>().join(", ");
    let (cw, ch) = (300.0, 96.0);
    let x = if q[0] + 16.0 + cw > view[0] { (q[0] - 16.0 - cw).max(0.0) } else { q[0] + 16.0 };
    let y = if q[1] + 16.0 + ch > view[1] { (q[1] - 16.0 - ch).max(0.0) } else { q[1] + 16.0 };
    col().key("lanes-preview")
        .absolute(x, y)
        .w(cw)
        .padding(8.0)
        .gap(2.0)
        .fill(k.c.surface_raised)
        .border(1.0, k.c.border_base)
        .radius(k.radius.sm)
        .child(text(format!("{} · {head}", n.node.id)).font(k.ty.small_medium).color(k.c.text_base).single_line().truncate())
        .child(text(crate::refs::cut(words, 160)).font(k.ty.small).color(k.c.text_muted).wrap())
        .child(text(if edges.is_empty() { "no edge in this slice".to_string() } else { format!("edges here: {edges}") }).font(k.ty.small).color(k.c.text_faint).wrap())
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use coord_types::Label;
    use docket_machine::graph::{EdgeKind, Graph, Source};

    pub(in crate::map) const NOW: &str = "2026-11-22T08:20:00Z";

    /// A record's graph, as the engine keeps it, made up (times on 22 Nov, `HH:MM`; none: no time):
    /// - his ask N-1104.p700 at 08:01 on his chat, asking docket-edge5 (the ASKED ledger's `asks`), handed to it at
    ///   08:02, the delivery that began its turn; its answer N-1446.p1400 at 08:10, which closes it in the ledger;
    /// - docket-edge5's commit 0f40917b at 08:12 naming p700, and docket-core's job 1-1 on it from 08:13, green at 08:15;
    /// - his N-1479.p10 at 08:05, asking docket-core, handed to it at 08:06, the delivery that began its turn, and
    ///   taken by nobody.
    pub(in crate::map) fn graph() -> Graph {
        let mut g = Graph::default();
        let mut put = |from: (&str, NodeKind), to: (&str, NodeKind), k: EdgeKind, n: usize, hm: &str| {
            let at = (!hm.is_empty()).then(|| format!("2026-11-22T{hm}:00Z"));
            let a = g.node_from(from.0, from.1, Source::Line(n), at.clone());
            let b = g.node_from(to.0, to.1, Source::Line(n), at.clone());
            g.edge_from(&a, &b, k, Source::Line(n), at);
        };
        let (owner, e5, core) = (("agent:owner", NodeKind::Agent), ("agent:docket-edge5", NodeKind::Agent), ("agent:docket-core", NodeKind::Agent));
        let (p700, p1400, p10) = (("N-1104.p700", NodeKind::Post), ("N-1446.p1400", NodeKind::Post), ("N-1479.p10", NodeKind::Post));
        let (commit, job) = (("commit:0f40917b", NodeKind::Commit), ("job:1-1", NodeKind::Job));
        put(owner, p700, EdgeKind::Wrote, 1, "08:01");
        put(p700, ("N-1104", NodeKind::Ticket), EdgeKind::PostedIn, 1, "");
        put(p700, e5, EdgeKind::Asks, 1, "08:01");
        put(p700, e5, EdgeKind::Handed, 2, "08:02");
        put(p700, e5, EdgeKind::Prompted, 2, "08:02");
        put(owner, p10, EdgeKind::Wrote, 3, "08:05");
        put(p10, core, EdgeKind::Asks, 3, "08:05");
        put(p10, core, EdgeKind::Handed, 4, "08:06");
        put(p10, core, EdgeKind::Prompted, 4, "08:06");
        put(e5, p1400, EdgeKind::Wrote, 5, "08:10");
        put(p1400, p700, EdgeKind::Answered, 5, "08:10");
        put(p700, p1400, EdgeKind::ClosedBy, 5, "08:10");
        put(e5, commit, EdgeKind::Wrote, 6, "08:12");
        put(commit, p700, EdgeKind::Named, 6, "08:12");
        put(core, job, EdgeKind::Ran, 7, "08:13");
        put(job, commit, EdgeKind::Ran, 7, "08:13");
        put(job, commit, EdgeKind::Ended, 8, "08:15");
        g.get_mut("job:1-1").unwrap().ok = Some(true);
        g
    }

    /// Coordd's answer to ASK of [`graph`]: the engine's slice, the labels coordd would give, now at [`NOW`].
    pub(in crate::map) fn answer(ask: &Ask) -> GraphAnswer {
        let g = graph();
        let label = |id: &str, is: &str, l: &str| Label { id: id.into(), is: is.into(), label: l.into(), on: None };
        let labels = vec![
            label("N-1104.p700", "post", "Make the graph page"),
            label("N-1446.p1400", "post", "The page, around his post"),
            label("N-1479.p10", "post", "And the stills"),
            label("commit:0f40917b", "commit", "lanes: the graph page"),
        ];
        let course = ask.around.as_ref().and_then(|a| g.ask_times().into_iter().find(|t| t.post == *a));
        GraphAnswer { slice: g.slice(ask), labels, now: ask.now.then(|| g.now(NOW)), course, gone: Vec::new(), folder: None }
    }

    pub(in crate::map) fn around(id: &str) -> Ask {
        Ask { around: Some(id.into()), hops: HOPS, now: true, ..Default::default() }
    }

    fn t(hm: &str) -> i64 {
        docket_machine::machine::secs(&format!("2026-11-22T{hm}:00Z")).unwrap()
    }

    /// Each card where `lay` put it: its id, its lane's word (or "now"), its row's name, its time.
    fn placed(a: &GraphAnswer, l: &Laid) -> Vec<(String, String, String, i64)> {
        let lane = |i: usize| l.lanes[i].0.map_or("now", word).to_string();
        let mut v: Vec<_> = l.cards.iter().map(|&(i, ln, r, at)| (a.slice.nodes[i].node.id.clone(), lane(ln), l.lanes[ln].1[r].clone(), at)).collect();
        v.sort();
        v
    }

    /// paper-research N-1446 p1510, 2, p1532's stills: a row's list of his asks opens under the row where the canvas
    /// holds it there; else over it, its last line's foot at the row; where neither side holds it, on the side with
    /// more room, as many lines as fit with one more saying how many are left. Every line inside the canvas, oldest
    /// first. The canvas 900 points high, the row at 600 and at 100.
    #[test]
    fn a_list_of_his_asks_opens_under_its_row_or_over_it_and_stays_inside_the_canvas() {
        let a = answer(&around("N-1104.p700"));
        let laid = lay(&a, &BTreeSet::new(), &BTreeSet::new());
        let (tops, _) = tops(&laid);
        let now = a.now.clone().unwrap();
        let list = |now: &Now, at: f32, n: usize| {
            let mut v = Lanes { area: [0.0, 0.0, 1400.0, 900.0], listed: Some(NOT_TAKEN.into()), ..Default::default() };
            let (_, _, y0) = ask_rows(&v, now, &laid, &tops).into_iter().find(|r| r.0 == NOT_TAKEN).unwrap();
            v.dy = at - y0;
            let mut now = now.clone();
            now.not_taken = (0..n).map(|i| format!("N-9.p{i}")).collect();
            let (lines, more) = ask_list(&v, &now, &laid, &tops);
            let (first, last) = (lines.first().unwrap().1, lines.last().unwrap().1);
            let foot = last[1] + last[3] + if more > 0 { ASK_LINE } else { 0.0 };
            assert!(first[1] - 4.0 >= 0.0 && foot + 4.0 <= 900.0, "inside the canvas: {first:?} to {foot}");
            assert_eq!(lines.iter().map(|l| l.0.clone()).collect::<Vec<_>>(), now.not_taken[..lines.len()].to_vec(), "oldest first");
            (lines.len(), more, first[1], foot)
        };
        assert!(!now.not_taken.is_empty(), "the made-up now has his ask nobody took, so the row is laid");
        // 5 under a row at 600: under it, from its foot (613)
        assert_eq!(list(&now, 600.0, 5), (5, 0, 600.0 + ROW / 2.0, 600.0 + ROW / 2.0 + 5.0 * ASK_LINE));
        // 17 at 600: 279 points under it hold 13; over it, 579 hold all 17, the last line's foot 4 over the row's top
        let (n, more, top, foot) = list(&now, 600.0, 17);
        assert_eq!((n, more, foot), (17, 0, 600.0 - ROW / 2.0 - 4.0), "over the row, whole");
        assert_eq!(top, foot - 17.0 * ASK_LINE);
        // 60 at 600: over it, the side with more room, 28 lines: 27 asks and "33 more"
        let (n, more, top, foot) = list(&now, 600.0, 60);
        assert_eq!((n, more, top, foot), (27, 33, 600.0 - ROW / 2.0 - 4.0 - 28.0 * ASK_LINE, 600.0 - ROW / 2.0 - 4.0));
        // 60 at 100: under it, 779 points, 38 lines: 37 asks and "23 more"
        let (n, more, top, _) = list(&now, 100.0, 60);
        assert_eq!((n, more, top), (37, 23, 100.0 + ROW / 2.0));
    }

    /// paper-research N-1446 p1385 B, proto-window p1383: time across, a lane per layer on, a row per agent. Around his
    /// p700, all on: now's lane on top (p1469, 6), the agents in a turn and his asks nobody took; his post in his row of
    /// his lane, its answer in docket-edge5's of the agents', the commit and the job in their agents' rows of work (the
    /// commit by its writer, the job by who ran it); the delivery to docket-edge5 a fan in its writer's row of the
    /// deliveries lane (p1469, 1 and 3: no row for the agent it reached until he opens it), and the agent his ask asks
    /// (the ASKED ledger's `asks`) a fan in his row of his lane; the thread, with no time,
    /// not drawn and counted. With his and the deliveries alone on, the answer, whose own lane is off, is in his lane's
    /// row of others, by the edge that brought it. The fan opened: a mark in docket-edge5's row, and no fan.
    #[test]
    fn lay_puts_each_card_in_its_kinds_lane_and_its_agents_row_and_an_edge_at_an_agent_in_its_row() {
        let a = answer(&around("N-1104.p700"));
        let none = BTreeSet::new();
        let l = lay(&a, &BTreeSet::new(), &none);
        let rows = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(
            l.lanes,
            vec![
                (None, rows(&["docket-core", "docket-edge5", NOT_TAKEN])),
                (Some(Layer::His), rows(&["owner"])),
                (Some(Layer::Agents), rows(&["docket-edge5"])),
                (Some(Layer::Deliveries), rows(&["owner"])),
                (Some(Layer::Work), rows(&["docket-core", "docket-edge5"])),
                (Some(Layer::Calls), rows(&[])),
                (Some(Layer::Files), rows(&[])),
            ]
        );
        let p = |id: &str, lane: &str, row: &str, hm: &str| (id.to_string(), lane.to_string(), row.to_string(), t(hm));
        assert_eq!(
            placed(&a, &l),
            vec![
                p("N-1104.p700", "his messages", "owner", "08:01"),
                p("N-1446.p1400", "agent to agent", "docket-edge5", "08:10"),
                p("commit:0f40917b", "work", "docket-edge5", "08:12"),
                p("job:1-1", "work", "docket-core", "08:13"),
            ]
        );
        let fans: Vec<_> = l.fans.iter().map(|f| (f.id.as_str(), f.node.as_str(), l.lanes[f.lane].1[f.row].as_str(), f.at, f.agents.clone(), fan_words(&a, f))).collect();
        assert_eq!(
            fans,
            vec![
                ("fan:N-1104.p700:His", "N-1104.p700", "owner", t("08:01"), rows(&["docket-edge5"]), "asks docket-edge5".to_string()),
                ("fan:N-1104.p700:Deliveries", "N-1104.p700", "owner", t("08:02"), rows(&["docket-edge5"]), "reached docket-edge5".to_string()),
            ],
            "the agent it asks, as the ASKED ledger says, in his lane; the delivery in the deliveries'"
        );
        assert!(l.marks.is_empty(), "no mark until he opens it");
        assert_eq!((l.untimed, l.span), (1, Some((t("08:01"), t("08:20")))));
        // opened: a mark in the row of the agent it reached
        let opened: BTreeSet<String> = ["fan:N-1104.p700:Deliveries".to_string()].into();
        let o = lay(&a, &BTreeSet::new(), &opened);
        assert_eq!(o.lanes[3], (Some(Layer::Deliveries), rows(&["docket-edge5"])));
        let marks: Vec<_> = o.marks.iter().map(|&(e, ln, r, at)| (a.slice.edges[e].edge.kind, ln, o.lanes[ln].1[r].as_str(), at)).collect();
        assert_eq!((marks, o.fans.len()), (vec![(EdgeKind::Handed, 3, "docket-edge5", t("08:02")), (EdgeKind::Prompted, 3, "docket-edge5", t("08:02"))], 1), "the delivery and the turn it began; his lane's fan stays");
        // his and the deliveries alone
        let only = |ls: &[Layer]| Ask { layers: ls.iter().copied().collect(), ..around("N-1104.p700") };
        let a = answer(&only(&[Layer::His, Layer::Deliveries]));
        let l = lay(&a, &only(&[Layer::His, Layer::Deliveries]).layers, &none);
        assert_eq!(l.lanes, vec![(None, rows(&["docket-core", "docket-edge5", NOT_TAKEN])), (Some(Layer::His), rows(&["owner", ""])), (Some(Layer::Deliveries), rows(&["owner"]))]);
        assert_eq!(placed(&a, &l), vec![p("N-1104.p700", "his messages", "owner", "08:01"), p("N-1446.p1400", "his messages", "", "08:10")], "the answer in his lane's row of others");
        assert_eq!(l.fans.len(), 2, "the agent it asks and the delivery");
    }

    /// p1469, 5: a stretch with nothing in it drawn short. Three times a minute apart, nothing for 2 h 44 m, then one
    /// more a minute on, into 1000 points: the long gap is the one fold, drawn [`FOLD`] wide, and the minutes take the
    /// rest, drawn wide and not folded. Time inside a fold goes in proportion; past it, less what it cut. Even minutes:
    /// no gap is past the pace, so none folds; minutes and then two gaps of hours fold both. A day like his p678's: posts
    /// 10 m apart for 3 h, 2 h with none, 1 h more, a night of 8 h, 3 h more: the night folds though the day's gaps
    /// together are longer (9 h), and the 2 h is drawn, 118 points at the fit, under `FOLD_LEAST`.
    #[test]
    fn a_stretch_with_nothing_in_it_is_folded_short_and_the_rest_takes_the_room() {
        let (folds, scale) = folds_for(&[0, 60, 120, 10000, 10060], 0, 10060, 1000.0);
        assert_eq!(folds.iter().map(|f| (f.from, f.to)).collect::<Vec<_>>(), vec![(120, 10000)]);
        let wide = |a: i64, b: i64| (folded(&folds, b) - folded(&folds, a)) * scale;
        assert!((wide(0, 10060) - 1000.0).abs() < 2.0, "the room: {}", wide(0, 10060));
        assert!((wide(120, 10000) - FOLD as f64).abs() < 1.0, "the fold: {}", wide(120, 10000));
        assert!((wide(0, 60) - 1000.0 * 60.0 / 180.0 * (1000.0 - FOLD as f64) / 1000.0).abs() < 2.0, "a minute: {}", wide(0, 60));
        assert_eq!(folded(&folds, 60), 60.0, "before it, as it was");
        assert!((folded(&folds, 5060) - (120.0 + folds[0].kept / 2.0)).abs() < 1e-6, "halfway in, halfway across");
        let (none, _) = folds_for(&[0, 60, 120, 180], 0, 180, 1000.0);
        assert!(none.is_empty(), "even minutes: no fold");
        let (two, _) = folds_for(&[0, 60, 120, 7320, 7380, 14580], 0, 14580, 1000.0);
        assert_eq!(two.iter().map(|f| (f.from, f.to)).collect::<Vec<_>>(), vec![(120, 7320), (7380, 14580)], "both hours");
        let day: Vec<i64> = (0..=10800).step_by(600).chain((18000..=21600).step_by(600)).chain((50400..=61200).step_by(600)).collect();
        let (night, scale) = folds_for(&day, 0, 61200, 1000.0);
        assert_eq!(night.iter().map(|f| (f.from, f.to)).collect::<Vec<_>>(), vec![(21600, 50400)], "the night, not the 2 h");
        assert!(((folded(&night, 61200) - folded(&night, 0)) * scale - 1000.0).abs() < 2.0, "the room");
        // days before the span the fit shows: not folded, and the span takes the room as without them
        let (with_before, at) = folds_for(&[-200000, -90000, 0, 60, 120, 10000, 10060], 0, 10060, 1000.0);
        let (without, at_without) = folds_for(&[0, 60, 120, 10000, 10060], 0, 10060, 1000.0);
        assert_eq!((with_before, at), (without, at_without), "what is out of view folds nothing");
    }

    /// p1385: "the line under the switches says the focus as id plus label", and which layers are on (p1376: "a line
    /// that always says which layers are on"); what the slice holds by kind, who and the item, what the limit keeps (the
    /// nearest first around a node, the newest first in the whole graph: p1772) and what it left out, what has no time
    /// to be drawn at; a focus the graph does not hold, said so; before the answer, what is asked.
    #[test]
    fn the_line_says_what_is_on_what_the_slice_holds_its_focus_and_what_was_left_out() {
        let ask = around("N-1104.p700");
        assert_eq!(
            line(&ask, Some(&answer(&ask))),
            "On: his messages, agent to agent, deliveries, work, calls and prompts, files · 2 posts, 1 thread or item, 1 commit, 1 job · around N-1104.p700 “Make the graph page”, 3 hops · nothing left out · 1 with no time, not drawn"
        );
        let four = Ask { limit: 4, ..ask.clone() };
        // the nearest 4: p700's own 8 edges are its first hop, and the limit cuts there, each its own branch, so the
        // newest 4 of them: the commit naming it, the answer and its closing, one to docket-edge5 at 08:02; the job, two
        // hops off, is left out, and the thread's edge, with no time
        assert_eq!(line(&four, Some(&answer(&four))), "On: his messages, agent to agent, deliveries, work, calls and prompts, files · 2 posts, 1 commit · around N-1104.p700 “Make the graph page”, 3 hops · 4 edges of 13, the nearest first: 9 left out");
        // the whole graph's 4: the newest of its 17, the job's three and the commit naming p700
        let whole = Ask { limit: 4, now: true, ..Ask::default() };
        assert_eq!(line(&whole, Some(&answer(&whole))), "On: his messages, agent to agent, deliveries, work, calls and prompts, files · 1 post, 1 commit, 1 job · 4 edges of 17, the newest first: 13 left out");
        let narrowed = Ask { layers: [Layer::His, Layer::Work].into_iter().collect(), hops: 2, who: vec!["docket-edge5".into()], items: vec!["I-1469".into()], ..ask.clone() };
        assert_eq!(line(&narrowed, None), "On: his messages, work · around N-1104.p700, 2 hops · who: docket-edge5 · item: I-1469");
        let gone = around("N-9.p1");
        assert_eq!(line(&gone, Some(&answer(&gone))), "On: his messages, agent to agent, deliveries, work, calls and prompts, files · nothing in reach · around N-9.p1, which the graph does not hold · nothing left out");
    }

    /// A kind and a layer this build does not know, a newer coordd's (paper-research N-1446 p1574): the answer still
    /// decodes; the card of a node of that kind says "other", in the lane an unknown kind goes to (`home`), in its
    /// writer's row; an edge of that layer, which has no switch, draws no line; the rest as before.
    #[test]
    fn a_kind_this_build_does_not_know_decodes_and_its_card_says_other() {
        let a = answer(&around("N-1104.p700"));
        let node = a.slice.nodes.iter().position(|n| n.node.id == "commit:0f40917b").unwrap();
        let edge = a.slice.edges.iter().position(|e| e.edge.kind == EdgeKind::Named).unwrap();
        let mut v = serde_json::to_value(&a).unwrap();
        v["slice"]["nodes"][node]["kind"] = "made_up".into();
        v["slice"]["edges"][edge]["kind"] = "made_up".into();
        v["slice"]["edges"][edge]["layer"] = "made_up".into();
        let b: GraphAnswer = serde_json::from_value(v).unwrap();
        assert_eq!((b.slice.nodes[node].node.kind, b.slice.edges[edge].edge.kind, b.slice.edges[edge].layer), (NodeKind::Other, EdgeKind::Other, Layer::Other));
        assert_eq!((kind_words(NodeKind::Other).0, word(Layer::Other)), ("other", "other"));
        let l = lay(&b, &BTreeSet::new(), &BTreeSet::new());
        let p = |id: &str, lane: &str, row: &str, hm: &str| (id.to_string(), lane.to_string(), row.to_string(), t(hm));
        assert_eq!(
            placed(&b, &l),
            vec![
                p("N-1104.p700", "his messages", "owner", "08:01"),
                p("N-1446.p1400", "agent to agent", "docket-edge5", "08:10"),
                p("commit:0f40917b", "calls and prompts", "docket-edge5", "08:12"),
                p("job:1-1", "work", "docket-core", "08:13"),
            ]
        );
        assert_eq!(l.lanes.iter().filter_map(|(x, _)| *x).collect::<Vec<_>>(), Layer::ALL, "no lane for the layer it does not know");
    }

    /// A worktree (docket-edge8's git round; paper-research N-1446 p1574, gap 1): its card in the work lane, beside its
    /// commits and jobs, in the row of the agent its edge joins it to; its card and the line say "worktree".
    #[test]
    fn a_worktree_card_goes_in_the_work_lane_beside_its_commits_and_jobs() {
        let a = answer(&around("N-1104.p700"));
        let node = a.slice.nodes.iter().position(|n| n.node.id == "commit:0f40917b").unwrap();
        let mut v = serde_json::to_value(&a).unwrap();
        v["slice"]["nodes"][node]["kind"] = "worktree".into();
        let b: GraphAnswer = serde_json::from_value(v).unwrap();
        assert_eq!((b.slice.nodes[node].node.kind, kind_words(NodeKind::Worktree).0), (NodeKind::Worktree, "worktree"));
        let l = lay(&b, &BTreeSet::new(), &BTreeSet::new());
        let p = |id: &str, lane: &str, row: &str, hm: &str| (id.to_string(), lane.to_string(), row.to_string(), t(hm));
        assert_eq!(
            placed(&b, &l),
            vec![
                p("N-1104.p700", "his messages", "owner", "08:01"),
                p("N-1446.p1400", "agent to agent", "docket-edge5", "08:10"),
                p("commit:0f40917b", "work", "docket-edge5", "08:12"),
                p("job:1-1", "work", "docket-core", "08:13"),
            ]
        );
        let said = line(&around("N-1104.p700"), Some(&b));
        assert!(said.contains(" · 2 posts, 1 thread or item, 1 job, 1 worktree · "), "counted on the line, after the jobs: {said}");
    }

    /// A switch turns its layer off and on; all on again is all (the ask names none), and the last one on stays on.
    #[test]
    fn a_switch_turns_its_layer_off_and_on_and_the_last_one_on_stays() {
        let (_held, mut s) = crate::view_cost::test_app();
        toggle(&mut s, Layer::His);
        assert_eq!(on(&s.lanes.ask), vec![Layer::Agents, Layer::Deliveries, Layer::Work, Layer::Calls, Layer::Files]);
        toggle(&mut s, Layer::His);
        assert!(s.lanes.ask.layers.is_empty(), "all on: none named");
        for l in [Layer::His, Layer::Agents, Layer::Deliveries, Layer::Work, Layer::Calls, Layer::Files] {
            toggle(&mut s, l);
        }
        assert_eq!(on(&s.lanes.ask), vec![Layer::Files], "the last stays on");
    }

    /// The page opens from its focus to now: around docket-edge5's answer at 08:10 to now at 08:20, though the slice
    /// reaches back to his p700 at 08:01; around p700, the slice's first, the whole span; with no now, to the last card
    /// (the job, from 08:13).
    #[test]
    fn the_camera_opens_from_the_focus_to_now() {
        let at = |id: &str| {
            let ask = around(id);
            fit(&ask, &answer(&ask))
        };
        assert_eq!(at("N-1446.p1400"), Some((t("08:10"), t("08:20"))));
        assert_eq!(at("N-1104.p700"), Some((t("08:01"), t("08:20"))));
        let ask = Ask { now: false, ..around("N-1446.p1400") };
        assert_eq!(fit(&ask, &answer(&ask)), Some((t("08:10"), t("08:13"))), "to the last card, the job");
    }

    /// paper-research N-1446 p1469, 6: an agent of now says, in this order, one column each, its turn since when, its
    /// last post with its first words (its name before them left out), its last job green or red before what it ran,
    /// and its items; with no label for its last post, the post's id alone; idle, since when; nothing where it has
    /// none.
    #[test]
    fn an_agent_of_now_says_its_turn_its_last_post_its_last_job_and_its_items() {
        use docket_machine::graph::{AgentNow, JobNow, Reached, TurnNow};
        let r = |id: &str, at: &str| Reached { at: at.into(), id: id.into(), via: None };
        let mut a = AgentNow {
            agent: "docket-edge5".into(),
            turn: Some(TurnNow { since: "2026-11-22T12:40:00Z".into(), by: "N-1446.p1469".into(), kind: EdgeKind::Prompted }),
            idle_since: None,
            last_post: Some(r("N-1446.p1464", "2026-11-22T11:58:35Z")),
            items: vec![r("I-1478", "2026-11-21T09:00:00Z")],
            last_commit: None,
            last_job: Some(JobNow { at: "2026-11-22T11:54:38Z".into(), id: "job:7-1".into(), what: "test -p coordd".into(), commit: None, ended: Some("2026-11-22T11:55:07Z".into()), ok: Some(false) }),
            began: None,
            session: None,
        };
        let labels = HashMap::from([("N-1446.p1464", "docket-edge5: The ASKED ledger's close is now in the graph as edges")]);
        let at = crate::when::at;
        assert_eq!(
            doing(&a, &labels),
            [
                format!("in a turn since {}", at("2026-11-22T12:40:00Z")),
                format!("last post N-1446.p1464 “The ASKED ledger's close is now in the g…” {}", at("2026-11-22T11:58:35Z")),
                format!("job red at {}, test -p coordd", at("2026-11-22T11:55:07Z")),
                "I-1478".to_string(),
            ]
        );
        a.turn = None;
        a.idle_since = Some("2026-11-22T12:30:56Z".into());
        a.last_job = None;
        a.items.clear();
        assert_eq!(doing(&a, &HashMap::new()), [format!("idle since {}", at("2026-11-22T12:30:56Z")), format!("last post N-1446.p1464 {}", at("2026-11-22T11:58:35Z")), String::new(), String::new()]);
    }

    /// paper-research N-1446 p1429, 1: his ask's course as its spans, by the times alone. The made-up p700 at 08:20:
    /// reached 1 m after he asked, taken 8 m after (the longest) and closed in the ledger as it was, its commit 2 m on,
    /// green 3 m on, not merged, and 5 m so far not in his window. A p678 like his: green at 10:40 by a job for its
    /// item before the commit at 10:45, so in time the green comes first; in his window 45 m after the commit, the
    /// longest. Closed in the ledger as it was taken, with no commit: a point at the same moment, and the span to now
    /// runs on. Closed after the merge: in its place, and the span to now runs on. Open in the ledger: the close not
    /// reached yet. Of equal spans the first.
    /// The ASKED ledger's close (`Point::Closed`): docket-edge5 N-1446 p1464.
    #[test]
    fn the_course_is_its_points_with_the_span_to_each_and_the_longest_by_the_times_alone() {
        let p700 = graph().ask_times().into_iter().find(|x| x.post == "N-1104.p700").expect("his post");
        let c = course(&p700, Some(t("08:20")));
        let legs: Vec<(Point, String, Option<i64>)> = c.legs.iter().map(|l| (l.point, leg_words(l), l.took)).collect();
        let leg = |p: Point, w: &str, took: Option<i64>| (p, w.to_string(), took);
        assert_eq!(
            legs,
            [
                leg(Point::Asked, "asked", None),
                leg(Point::Reached, "reached docket-edge5", Some(60)),
                leg(Point::Taken, "taken by docket-edge5", Some(480)),
                leg(Point::Closed, "closed by docket-edge5", Some(0)),
                leg(Point::FirstCommit, "first commit 0f40917b", Some(120)),
                leg(Point::FirstGreen, "first green job 1-1", Some(180)),
            ]
        );
        assert_eq!((c.open, c.not_yet.clone(), c.longest), (Some(300), vec![Point::Merged], Some(Longest::Leg(2))));
        assert_eq!(course(&p700, None).open, None, "no clock, no span to now");
        let at = |hm: &str| format!("2026-11-22T{hm}:00Z");
        let r = |hm: &str, id: &str| Some(docket_machine::graph::Reached { at: at(hm), id: id.into(), via: None });
        let p678 = AskTimes {
            post: "N-1104.p678".into(),
            asked: Some(at("10:00")),
            reached: r("10:01", "agent:accounts-2"),
            taken: r("10:20", "agent:accounts-2"),
            first_commit: r("10:45", "commit:4ee5c14e"),
            merged: None,
            first_green: r("10:40", "job:3-1"),
            in_window: r("11:30", "build:20261122-113000-4ee5c14e"),
            asks: Vec::new(),
            closed: None,
            signed: None,
        };
        let c = course(&p678, Some(t("12:00")));
        let tooks: Vec<(Point, Option<i64>)> = c.legs.iter().map(|l| (l.point, l.took)).collect();
        assert_eq!(tooks, [(Point::Asked, None), (Point::Reached, Some(60)), (Point::Taken, Some(1140)), (Point::FirstGreen, Some(1200)), (Point::FirstCommit, Some(300)), (Point::InWindow, Some(2700))], "in time: green, then the commit");
        assert_eq!((c.open, c.not_yet.clone(), c.longest), (None, vec![Point::Merged], Some(Longest::Leg(5))));
        assert_eq!(leg_words(&c.legs[5]), "in your window");
        // closed in the ledger as it was taken, no commit (his p745 on his record): no span to the close, and the span
        // to now runs on from it, the longest
        let words = AskTimes { closed: r("10:20", "agent:accounts-2"), asks: vec!["agent:accounts-2".into()], first_commit: None, first_green: None, in_window: None, ..p678.clone() };
        let c = course(&words, Some(t("12:00")));
        let tooks: Vec<(Point, Option<i64>)> = c.legs.iter().map(|l| (l.point, l.took)).collect();
        assert_eq!(tooks, [(Point::Asked, None), (Point::Reached, Some(60)), (Point::Taken, Some(1140)), (Point::Closed, Some(0))]);
        assert_eq!((c.open, c.longest, c.not_yet.clone()), (Some(6000), Some(Longest::Open), vec![Point::FirstCommit, Point::Merged, Point::FirstGreen]));
        assert_eq!(leg_words(&c.legs[3]), "closed by accounts-2");
        // closed after the merge, not in his window: the close in its place in time, then the span to now runs on
        let late = AskTimes { merged: r("10:47", "commit:4ee5c14e"), closed: r("10:50", "agent:accounts-2"), asks: vec!["agent:accounts-2".into()], in_window: None, ..p678.clone() };
        let c = course(&late, Some(t("12:00")));
        let tooks: Vec<(Point, Option<i64>)> = c.legs.iter().map(|l| (l.point, l.took)).collect();
        assert_eq!(tooks, [(Point::Asked, None), (Point::Reached, Some(60)), (Point::Taken, Some(1140)), (Point::FirstGreen, Some(1200)), (Point::FirstCommit, Some(300)), (Point::Merged, Some(120)), (Point::Closed, Some(180))]);
        assert_eq!((c.open, c.longest, c.not_yet.clone()), (Some(4200), Some(Longest::Open), vec![]));
        // open in the ledger: the close is a point not reached yet
        let open = AskTimes { asks: vec!["agent:accounts-2".into()], first_commit: None, first_green: None, in_window: None, ..p678.clone() };
        let c = course(&open, Some(t("12:00")));
        assert_eq!((c.open, c.not_yet.clone()), (Some(6000), vec![Point::Closed, Point::FirstCommit, Point::Merged, Point::FirstGreen]));
        let even = AskTimes { reached: r("10:05", "agent:accounts-2"), taken: r("10:10", "agent:accounts-2"), first_commit: None, first_green: None, in_window: None, ..p678 };
        let c = course(&even, Some(t("10:10")));
        assert_eq!((c.open, c.longest, c.not_yet.clone()), (Some(0), Some(Longest::Leg(1)), vec![Point::FirstCommit, Point::Merged, Point::FirstGreen]), "5 m and 5 m: the first; 0 so far");
    }
}
