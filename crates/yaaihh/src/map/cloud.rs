//! The visual layer (paper-research N-1446 p1630, p1632, p1656, on the study p1655; the owner, N-1104 p599: "a cloud
//! like obsisian thing with just as sort of the first layer highlter my messages", p603: "cannot shouw everyitng its
//! explroed byt he user / navigated", p767: "think like idk google earth layers", p818: "bare nodes is bad"): the Map's
//! canvas (canvas.rs, camera.rs, lod.rs) over coordd's one graph, in place of the timeline page (lanes.rs) wherever a
//! coordd serves the graph (`GRAPH_PAGE_PROTOCOL`). Nothing here makes a graph: the slices are coordd's, their labels
//! coordd's.
//!
//! - **It opens quiet** ([`cloud`], from [`base_ask`]: his layer, the whole graph): his posts as cards with their first
//!   words, inside the threads they are in, each thread a card with its title and how many of his posts it holds. No
//!   table, no rows, no counts on the canvas.
//! - **Placed by links, and kept** (cloud_place.rs `place`; Obsidian's graph view: link, repel and centre forces): a thread lies
//!   where the threads his posts name pull it. Once placed it stays (`cloud.json` beside map.json, kept as layout.rs
//!   keeps the Map's places): the forces move only what is new.
//! - **Zoom reveals the next layer in place** (Google Earth's, through lod.rs's bands): far out a thread is its card;
//!   nearer, his posts in it ([`Node::opens`]: once its cards would read), each with its answers' card beside it, joined
//!   by a line, the thread's order a path through them; nearer one, the agents' posts in that card.
//! - **A click goes in** (p824): canvas.rs flies into the card and selects it, and it is the focus whose slice
//!   ([`near_ask`]) is asked with the layers on.
//! - **Into his post, down to its steps and calls** (still 3, paper-research N-1446 p1734; the owner N-1104 p599: "i can
//!   dig down to the actual claude code layer"): in the focus's band, whom it reached, the run that handled it with
//!   its steps, each step's prompt template and calls on the wire ([`handled`]), and the requests that carried it, by
//!   the agent's session, each session's system prompt blocks, tools and files by their size ([`carried`]).
//! - **The switches** ([`PRESETS`]: Google Earth's styles as words he'd use, p1656: "Your messages", "With agents",
//!   "Everything"; then the six layers one by one): the layers the focus's slice is asked with.
//! - **The time bar** ([`time_bar`], under the switches; paper-research N-1511 p42, 2; the owner, N-1104 p849: "allow
//!   travel back over the timeline … the current map basically then becomes one timestep"): his posts by the hour
//!   across the record's span, now at its right end. A press on it shows the Map at that hour, the answers held as
//!   they were then ([`as_at`]), with nothing asked of coordd; a drag moves its mark, and the Map follows where he lets
//!   go. The threads stay where they are kept, each as large as it was, and what came later leaves its room empty.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use coord_types::GraphAnswer;
use docket_machine::graph::{EdgeKind, NodeKind};
use docket_machine::graph_slice::{Ask, Layer};
use rune::{Color, El, Shape, TextStyle, canvas, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit, SegmentedControl, Switch};

use super::canvas::faded;
use super::lod::READS;
use super::{Edge, Graph, Kind, Link, Node, Rect, Refused, Ring, Tier};
use crate::App;

/// His post's card in the world.
pub const CARD: [f64; 2] = [320.0, 120.0];
/// The room between cards, and around them in a thread.
pub const GAP: f64 = 24.0;
/// A thread's head over its posts, where its title goes: at least this (cloud_place.rs `lay_thread`).
pub const HEAD: f64 = 90.0;
/// The share of a card its words take once it has a band, one for every band: his post's, a step's, an answer's
/// (paper-research N-1446 p1796, 1b; 0.4 left a third of his opened post empty under his words in still 1 of his
/// p815). Its band is below, inside a gap of a fifth of GAP ([`band_cells`]); each opens once this share holds its
/// words ([`band_opens`]).
pub const WORDS: f64 = 0.25;
/// The share of the answers' bundle its head takes, its words ("2 answers: …"); its answers' cards below.
pub const BUNDLE_HEAD: f64 = 0.3;
/// The side on screen, in points, of his post's card at which its thread opens, so a thread of one post opens as soon
/// as its card would read and one of four hundred once its cards are seen at all.
const POST_OPENS: f64 = 64.0;
/// The side on screen at which his post opens, its band drawn: its answers' bundle, and the agents it reached.
const BAND_OPENS: f32 = 110.0;
/// The side on screen at which his post's answers' bundle opens, the answers' cards drawn in it.
const BUNDLE_OPENS: f32 = 140.0;
/// The side on screen at which an answer opens, the turn it was written in drawn in it.
const TURN_OPENS: f32 = 120.0;
/// The sides on screen at which, in his post's band, a run that handled it opens, its steps drawn in it; a step, its
/// prompt template and its calls on the wire; a session of the requests that carried it, the texts they carried.
const RUN_OPENS: f32 = 140.0;
const STEP_OPENS: f32 = 120.0;
const SESSION_OPENS: f32 = 140.0;
/// The most edges the base asks for: his layer whole (8,279 on 22 Nov).
/// The room between a card's words and its history's chip right of them (`card`).
const CHIP_GAP: f32 = 4.0;

pub const BASE_LIMIT: usize = 40_000;
/// The most edges a focus's slice asks for, and how far it runs from the focus.
pub const NEAR_LIMIT: usize = 6_000;
pub const NEAR_HOPS: u32 = 3;

/// The owner, as the record names him.
const OWNER: &str = "owner";

/// The cloud's own state on the App.
#[derive(Default)]
pub struct Cloud {
    /// The project the answers held are of.
    pub of: usize,
    /// His layer, the whole graph ([`base_ask`]): the cloud's cards, or why there are none.
    pub base: Option<Result<GraphAnswer, String>>,
    /// The card he went into last (`N-1104.p815`); history.rs keeps those before it, as places (its `Place::focus`,
    /// docket-edge8 N-1446 p1762), so ‹ › walk them.
    pub focus: Option<String>,
    /// The card "› on the Map" went to before the Map drew it (canvas.rs `on_map`): flown into once a build draws it,
    /// while it is still the focus. The Map stayed where it was, on the whole of a project's threads, his system
    /// prompt's card a dot among them, with no Edit on it (paper-research N-1446 p3041, 3).
    pub fly: Option<String>,
    /// The slice around the focus ([`near_ask`]), with the ask it answers.
    pub near: Option<(Ask, Result<GraphAnswer, String>)>,
    /// The layers on beside his (the switches); none: "Your messages".
    pub layers: BTreeSet<Layer>,
    /// The asks on their way, and "Read again" pressed.
    pub asked: Vec<Ask>,
    pub again: bool,
    /// The timeline page drawn in its place (lanes.rs), as before the cloud: until what is worked on now, its courses
    /// and his asks' lists go to a view of their own (paper-research N-1446 p1632), its tests draw it so.
    pub lanes: bool,
    /// The threads' places, kept, by id (`[x, y, w, h]`); read once from `cloud.json`, written when they change.
    pub kept: BTreeMap<String, [f64; 4]>,
    pub kept_read: Option<Result<(), String>>,
    /// The moment the Map shows ([`time_bar`]), in seconds: the answers as they were then ([`as_at`]). None: now, the
    /// bar's right end.
    pub at: Option<i64>,
    /// The time bar's width as last laid out, in points ([`time_bar`]).
    strip: f32,
    /// The time bar's ticks ([`ticks_of`]), oldest first: each a moment the Map's graph changed at (or, past
    /// [`TICKS`], the last of an even share of them), and how many of his posts came by it since the tick before.
    /// Made when the graph is built, so a frame's bar reads them and works nothing out.
    pub ticks: Vec<(i64, u32)>,
    /// The time bar's coloured marks, by tick (spacetime.rs `marks_of`), made with the ticks.
    pub marks: Vec<(usize, super::spacetime::Mark)>,
    /// How T is drawn (spacetime.rs), kept in ui.json.
    pub space: super::spacetime::Space,
    /// The causal slice open (causal.rs): the card's id and its past's, oldest first, the card last.
    pub causal: Option<(String, Vec<String>)>,
    /// The nodes whose history is unfolded on a rail beside their card, by id (ribbon.rs; the owner, N-1791 p1, 2).
    pub unfolded: BTreeSet<String>,
    /// The step whose Rewind / Branch from here is open, by its rail's node and its index (ribbon.rs).
    pub rewind: Option<(String, usize)>,
    /// The wheel's points not yet a step along the ticks ([`wheel_steps`]): a trackpad's small deltas add up to one.
    wheel_rest: f32,
    /// What the Map's graph was last built from: the base's and the near slice's sizes, the focus, the layers and the
    /// moment.
    built: Option<(usize, usize, Option<String>, BTreeSet<Layer>, Option<i64>, Option<i64>, usize)>,
    /// The threads of items other than the focus's (orbit.rs `peripheral`), made with the graph.
    pub side: Option<HashSet<u32>>,
}

/// The ask for the base: his layer, the whole graph.
pub fn base_ask() -> Ask {
    Ask { layers: [Layer::His].into_iter().collect(), limit: BASE_LIMIT, ..Ask::default() }
}

/// The ask for the slice around the focus FOCUS with LAYERS on beside his, and the focus's own layer ([`own_layer`]): a
/// commit his press on a template's row went to is asked with work in every style, so the cloud draws it with what
/// is one hop from it, and no more than the style says (paper-research N-1446 p1885, p1903, 1). A text is asked with
/// calls on too: the start that was started with it and the requests that carried it are a call's edges (docket-machine
/// graph_slice.rs `Layer::by`), and with work alone the slice around a system prompt held neither, so its card could
/// not say whose it is, nor edit it ([`edit_of`]; paper-research N-1446 p3041, 3). None for "Your messages" alone
/// and a focus with no layer of its own, his post among them, whose cards the base holds.
pub fn near_ask(focus: Option<&str>, layers: &BTreeSet<Layer>) -> Option<Ask> {
    let focus = focus?;
    let own = own_layer(focus);
    if layers.is_empty() && own.is_none() {
        return None;
    }
    let mut on: BTreeSet<Layer> = layers.clone();
    on.insert(Layer::His);
    on.extend(own);
    if focus.starts_with("text:") {
        on.insert(Layer::Calls);
    }
    Some(Ask { layers: on, around: Some(focus.to_string()), hops: NEAR_HOPS, limit: NEAR_LIMIT, ..Ask::default() })
}

/// The layer of node ID's own, by the kind its id names before its colon (`commit:3b2c4ac5`, as graph_git names it:
/// the kind's own word, `NodeKind`'s): the lane its card is in on the timeline page (lanes.rs `lane_of`). None for an id
/// that names no kind (his post, a thread, a step), one this build does not know, and an agent's (a row there).
fn own_layer(id: &str) -> Option<Layer> {
    let (word, _) = id.split_once(':')?;
    let kind: NodeKind = serde_json::from_value(serde_json::Value::String(word.to_string())).ok()?;
    (kind != NodeKind::Other).then(|| super::lanes::lane_of(kind, false)).flatten()
}

/// The presets of the switches (Google Earth's styles, his words: p1656), each the layers it puts on beside his.
pub const PRESETS: [(&str, &[Layer]); 3] = [
    ("Your messages", &[]),
    ("With agents", &[Layer::Agents]),
    ("Everything", &[Layer::Agents, Layer::Deliveries, Layer::Work, Layer::Calls, Layer::Files]),
];

/// The preset LAYERS are, if they are one.
fn preset_of(layers: &BTreeSet<Layer>) -> Option<usize> {
    PRESETS.iter().position(|(_, on)| on.len() == layers.len() && on.iter().all(|l| layers.contains(l)))
}

// ---- the graph --------------------------------------------------------------------------------------------------------

/// What a slice's node says beside its id: its label, and the thread a post is on.
fn labels(a: &GraphAnswer) -> HashMap<&str, &coord_types::Label> {
    a.labels.iter().map(|l| (l.id.as_str(), l)).collect()
}

/// A post's words: its label without its writer's name, which its card says otherwise ("owner: also stuff opencode…"
/// is his "also stuff opencode…").
fn words<'a>(label: &'a str, by: Option<&str>) -> &'a str {
    by.and_then(|b| label.strip_prefix(b)).and_then(|r| r.strip_prefix(": ")).unwrap_or(label)
}

/// Whether coordd's LABEL of a post by BY holds words past its writer's name (an empty post's label is `owner`).
fn has_words(label: &str, by: &str) -> bool {
    label.strip_prefix(by).is_none_or(|rest| !rest.trim_start_matches(':').trim().is_empty())
}

/// The thread a post or a ticket ID is in, by its id (`N-1104.p815` → `N-1104`).
fn thread_of(id: &str) -> &str {
    docket_machine::post_ref(id).map_or(id, |(t, _)| t)
}

/// The answer A as it was at T (seconds; the time bar's, [`Cloud::at`]): its edges at or before T, as coordd's slice
/// gives them for the same ask with `to` T (graph_slice.rs `Graph::slice`, its `when`), and the nodes at their ends
/// that were there by T, a node with no time of its own among them; read off the times the answer holds. coordd's labels
/// stay as they are now: a title has no history (proto-window N-1511 p3).
pub fn as_at(a: &GraphAnswer, t: i64) -> GraphAnswer {
    let to = docket_machine::machine::at_secs(t);
    let by = |w: Option<&str>| w.is_some_and(|w| w <= to.as_str());
    let mut out = a.clone();
    out.slice.edges.retain(|e| by(e.edge.when.as_deref()));
    let ends: HashSet<&str> = out.slice.edges.iter().flat_map(|e| [e.edge.from.as_str(), e.edge.to.as_str()]).collect();
    out.slice.nodes.retain(|n| ends.contains(n.node.id.as_str()) && (n.node.when.is_none() || by(n.node.when.as_deref())));
    out
}

/// The answer A from T on (seconds; his Archive's moment, archive.rs): its edges at or after T and the nodes at their
/// ends that came by then or have no time of their own, as [`as_at`] keeps what was there by a moment.
pub fn since(a: &GraphAnswer, t: i64) -> GraphAnswer {
    let from = docket_machine::machine::at_secs(t);
    let after = |w: Option<&str>| w.is_some_and(|w| w >= from.as_str());
    let mut out = a.clone();
    out.slice.edges.retain(|e| after(e.edge.when.as_deref()));
    let ends: HashSet<&str> = out.slice.edges.iter().flat_map(|e| [e.edge.from.as_str(), e.edge.to.as_str()]).collect();
    out.slice.nodes.retain(|n| ends.contains(n.node.id.as_str()) && (n.node.when.is_none() || after(n.node.when.as_deref())));
    out
}

/// The cloud of BASE (his layer, the whole graph), as the Map's graph of project PROJECT (`project:research`, named
/// NAME): one card per thread he posted in, placed by cloud_place.rs `place` from KEPT (which takes the places of the threads new to
/// it); his posts in it, oldest first, laid by cloud_place.rs `lay_thread`, each linked to his next (`Link::Next`);
/// beside each, joined to it by a line (`Link::Answered`), a card of the agents' posts that answer it (`answered`,
/// `answered_upto`, `closed_by`), each once, under the first of his it answers, and a line from each other of his it
/// answers; and his posts' links to the posts and threads they name (`named`). NEAR, the slice around his post it names,
/// goes inside that post's card and its answers'. Whether KEPT changed.
pub fn cloud(base: &GraphAnswer, near: Option<(&str, &GraphAnswer)>, project: &str, name: &str, kept: &mut BTreeMap<String, [f64; 4]>) -> Result<(Graph, bool), Refused> {
    let said = labels(base);
    let nodes: HashMap<&str, &docket_machine::graph_slice::SliceNode> = base.slice.nodes.iter().map(|n| (n.node.id.as_str(), n)).collect();
    let his = |id: &str| nodes.get(id).is_some_and(|n| n.node.kind == NodeKind::Post && n.by.as_deref() == Some(OWNER));
    // each of his posts' thread: its `posted_in`, else its id's
    let mut on: HashMap<&str, &str> = HashMap::new();
    for e in &base.slice.edges {
        if e.edge.kind == EdgeKind::PostedIn && his(&e.edge.from) {
            on.insert(e.edge.from.as_str(), e.edge.to.as_str());
        }
    }
    let mut threads: BTreeMap<&str, Vec<&docket_machine::graph_slice::SliceNode>> = BTreeMap::new();
    for n in base.slice.nodes.iter().filter(|n| his(&n.node.id)) {
        let t = on.get(n.node.id.as_str()).copied().unwrap_or_else(|| thread_of(&n.node.id));
        // not his words posted again on a handler's run (numbered from HIDDEN, as the machine keeps them apart from the
        // tickets he sees), nor a post of his with no words (coordd's label is its writer alone)
        let run = docket_machine::parse_ticket(t).is_some_and(|x| x >= docket_machine::HIDDEN);
        let wordless = said.get(n.node.id.as_str()).is_some_and(|l| !has_words(&l.label, OWNER));
        if !(run || wordless) {
            threads.entry(t).or_default().push(n);
        }
    }
    let number = |id: &str| docket_machine::post_ref(id).map_or(0, |(_, n)| n);
    for posts in threads.values_mut() {
        posts.sort_by(|a, b| (a.node.when.as_deref(), number(&a.node.id)).cmp(&(b.node.when.as_deref(), number(&b.node.id))));
    }
    // the threads in the order they began (his first post in each), so the spiral starts with the oldest
    let mut order: Vec<&str> = threads.keys().copied().collect();
    order.sort_by_key(|t| (threads[t][0].node.when.clone(), t.to_string()));
    let at: HashMap<&str, usize> = order.iter().enumerate().map(|(i, t)| (*t, i)).collect();
    // his posts' asks, and the answers that closed them (the ASKED ledger's: `asks`, `closed_by`), and each one's answers
    let mut asks: HashSet<&str> = HashSet::new();
    let mut closed: HashSet<&str> = HashSet::new();
    let mut answers: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut named: Vec<(&str, &str)> = Vec::new();
    for e in &base.slice.edges {
        let (f, t) = (e.edge.from.as_str(), e.edge.to.as_str());
        match e.edge.kind {
            EdgeKind::Asks if his(f) => {
                asks.insert(f);
            }
            EdgeKind::ClosedBy if his(f) => {
                closed.insert(f);
                if !his(t) && nodes.get(t).is_some_and(|n| n.node.kind == NodeKind::Post) {
                    answers.entry(f).or_default().push(t);
                }
            }
            EdgeKind::Answered | EdgeKind::AnsweredUpto if his(t) && !his(f) && nodes.get(f).is_some_and(|n| n.node.kind == NodeKind::Post) => answers.entry(t).or_default().push(f),
            EdgeKind::Named if his(f) => named.push((f, t)),
            _ => {}
        }
    }
    // each answer under the first of his posts it answers (the threads in their order, his posts in theirs); each other
    // post of his it answers joined to it by a line, so two threads sharing an answer are joined (paper-research N-1446
    // p1677, 1)
    let mut first: HashMap<&str, &str> = HashMap::new();
    let mut answers_of: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut shared: BTreeSet<(&str, &str)> = BTreeSet::new();
    for t in &order {
        for n in &threads[t] {
            let id = n.node.id.as_str();
            for a in answers.get(id).into_iter().flatten().copied() {
                match first.get(a).copied() {
                    None => {
                        first.insert(a, id);
                        answers_of.entry(id).or_default().push(a);
                    }
                    Some(f) if f != id => {
                        shared.insert((id, a));
                    }
                    Some(_) => {}
                }
            }
        }
    }
    // the thread each of his posts is in, by its place in the order
    let thread_at: HashMap<&str, usize> = order.iter().enumerate().flat_map(|(i, t)| threads[t].iter().map(move |n| (n.node.id.as_str(), i))).collect();
    // the threads' links: his post in one names a post of another, or another; or an answer it shares with another
    let mut between: BTreeMap<(usize, usize), u32> = BTreeMap::new();
    for (f, t) in &named {
        let (Some(a), Some(b)) = (on.get(f).copied().and_then(|x| at.get(x)), at.get(thread_of(t))) else { continue };
        if a != b {
            *between.entry((*a.min(b), *a.max(b))).or_default() += 1;
        }
    }
    for (f, a) in &shared {
        let (Some(x), Some(y)) = (thread_at.get(f), first.get(a).and_then(|p| thread_at.get(p))) else { continue };
        if x != y {
            *between.entry((*x.min(y), *x.max(y))).or_default() += 1;
        }
    }
    // his post's weight: its links, those it names or is named by, its answers and those it shares (lod.rs's ladder)
    let mut degree: HashMap<&str, u32> = HashMap::new();
    for (f, t) in &named {
        *degree.entry(*f).or_default() += 1;
        *degree.entry(*t).or_default() += 1;
    }
    for (f, _) in &shared {
        *degree.entry(*f).or_default() += 1;
    }
    for (p, m) in &answers_of {
        *degree.entry(*p).or_default() += m.len() as u32;
    }
    // each thread's own layout, from the links between his posts in it (cloud_place.rs `lay_thread`)
    let mut inside: BTreeMap<&str, Vec<(usize, usize, u32)>> = BTreeMap::new();
    {
        let k: HashMap<&str, usize> = threads.values().flat_map(|v| v.iter().enumerate().map(|(k, n)| (n.node.id.as_str(), k))).collect();
        let mut count: BTreeMap<(&str, usize, usize), u32> = BTreeMap::new();
        for (f, t) in &named {
            let (Some(tf), Some(a), Some(b)) = (on.get(f).copied(), k.get(f), k.get(t)) else { continue };
            if on.get(t) == Some(&tf) && a != b {
                *count.entry((tf, *a.min(b), *a.max(b))).or_default() += 1;
            }
        }
        for ((t, a, b), c) in count {
            inside.entry(t).or_default().push((a, b, c));
        }
    }
    // its cards: his posts in its order, each followed by its answers' card (p1677, 2: "each post a card with its
    // answers' card joined by a line"); where his k-th post's is
    use super::cloud_place::Piece;
    let mut pieces: BTreeMap<&str, (Vec<Piece>, Vec<usize>)> = BTreeMap::new();
    for t in &order {
        let (mut ps, mut his_at) = (Vec::new(), Vec::new());
        for n in &threads[t] {
            let k = ps.len();
            his_at.push(k);
            ps.push(Piece::His);
            if let Some(m) = answers_of.get(n.node.id.as_str()) {
                ps.push(Piece::Answers { of: k, n: m.len() });
            }
        }
        pieces.insert(*t, (ps, his_at));
    }
    // each thread's links to the others: its size's ceiling (cloud_place.rs `lay_thread`; paper-research N-1446 p1715, 1)
    let mut ties = vec![0u32; order.len()];
    for ((a, b), c) in &between {
        ties[*a] += c;
        ties[*b] += c;
    }
    let laid: BTreeMap<&str, super::cloud_place::Thread> = order.iter().enumerate().map(|(i, t)| (*t, super::cloud_place::lay_thread(&pieces[t].0, inside.get(t).map_or(&[][..], |v| v.as_slice()), ties[i]))).collect();
    let mut sized: Vec<(&str, [f64; 2])> = order.iter().map(|t| (*t, laid[t].size)).collect();
    let mut links: Vec<(usize, usize, u32)> = between.into_iter().map(|((a, b), c)| (a, b, c)).collect();
    // the focus when the cloud draws no card of it (none of his posts, their threads or their answers: a commit his
    // press on a template's row went to), one more island the size of his post's card, pulled toward the threads its
    // slice touches, kept as a thread is, so nothing he knows moves (paper-research N-1446 p1885, p1903, 2)
    let alone = near.filter(|(f, _)| !(at.contains_key(f) || thread_at.contains_key(f) || first.contains_key(f)));
    if let Some((f, a)) = alone {
        let k = sized.len();
        sized.push((f, CARD));
        let mut to: BTreeMap<usize, u32> = BTreeMap::new();
        for o in one_hop(a, f) {
            if let Some(t) = thread_at.get(o).or_else(|| at.get(o)).or_else(|| first.get(o).and_then(|p| thread_at.get(p))) {
                *to.entry(*t).or_default() += 1;
            }
        }
        links.extend(to.into_iter().map(|(t, c)| (t, k, c)));
    }
    let rects = super::cloud_place::place(&sized, &links, kept);
    let mut changed = false;
    for ((t, _), r) in sized.iter().zip(&rects) {
        let r = [r.x, r.y, r.w, r.h];
        if kept.get(*t) != Some(&r) {
            kept.insert(t.to_string(), r);
            changed = true;
        }
    }
    // the project round them all, with a card's room
    let (mut x0, mut y0, mut x1, mut y1) = (0.0f64, 0.0f64, CARD[0], CARD[1]);
    if let Some(first) = rects.first() {
        (x0, y0, x1, y1) = (first.x, first.y, first.right(), first.bottom());
    }
    for r in &rects {
        (x0, y0, x1, y1) = (x0.min(r.x), y0.min(r.y), x1.max(r.right()), y1.max(r.bottom()));
    }
    let room = 4.0 * GAP;
    let mut g = Graph::default();
    let p = g.push(Node::new(project, Kind::Project, name, None, Rect::new(x0 - room, y0 - room, x1 - x0 + 2.0 * room, y1 - y0 + 2.0 * room)))?;
    let mut index: HashMap<String, u32> = HashMap::new();
    for (t, r) in order.iter().zip(&rects) {
        let posts = &threads[t];
        let lay = &laid[t];
        let first = posts.first().and_then(|n| said.get(n.node.id.as_str())).map(|l| words(&l.label, Some(OWNER)));
        let title = super::topic::title(said.get(t).map(|l| l.label.as_str()), first, t);
        let open = posts.iter().filter(|n| asks.contains(n.node.id.as_str()) && !closed.contains(n.node.id.as_str())).count();
        let mut tn = Node::new(*t, Kind::Thread, title, Some(p), *r);
        tn.note = match (posts.len(), open) {
            (1, 0) => "1 of your posts".to_string(),
            (n, 0) => format!("{n} of your posts"),
            (n, o) => format!("{n} of your posts · {o} open"),
        };
        tn.his = true;
        tn.reads = Some(READS);
        tn.weight = posts.len() as f32;
        tn.tier = if open > 0 { Tier::HisAsk } else { Tier::Rest };
        // it opens once its cards would read (POST_OPENS across), at its own side then
        let card_side = (CARD[0] * CARD[1]).sqrt();
        tn.opens = Some((POST_OPENS * (r.w * r.h).sqrt() / card_side) as f32);
        let ti = g.push(tn)?;
        index.insert(t.to_string(), ti);
        let his_at = &pieces[t].1;
        let mut last: Option<u32> = None;
        for (k, n) in posts.iter().enumerate() {
            let at = lay.posts[his_at[k]];
            let pr = Rect::new(r.x + at.x, r.y + at.y, at.w, at.h);
            let id = n.node.id.as_str();
            let label = said.get(id).map(|l| words(&l.label, Some(OWNER))).filter(|l| !l.is_empty()).unwrap_or(id);
            let mut pn = Node::new(id, Kind::Post, label, Some(ti), pr);
            pn.his = true;
            pn.reads = Some(READS);
            pn.note = n.node.when.as_deref().map(|w| format!("you · {}", crate::when::at(w))).unwrap_or_else(|| "you".to_string());
            pn.ring = asks.contains(id).then(|| if closed.contains(id) { Ring::Answered } else { Ring::Open });
            pn.tier = if pn.ring == Some(Ring::Open) { Tier::HisAsk } else { Tier::Rest };
            // the slice around it, when it is the focus: in its band, the agents it reached, each run that handled it and
            // the requests that carried it, each a card (still 3: into a post, down to its steps and calls, and the
            // system prompt its requests carried; paper-research N-1446 p1734, the owner N-1104 p599)
            let deep = near.filter(|(f, _)| *f == id).map(|(_, a)| a);
            let reached = deep.map(|a| reached(a, id)).unwrap_or_default();
            let runs = deep.map(|a| handled(a, id)).unwrap_or_default();
            let sessions = deep.map(|a| carried(a, id)).unwrap_or_default();
            let band = usize::from(!reached.is_empty()) + runs.len() + usize::from(!sessions.is_empty());
            pn.weight = degree.get(id).copied().unwrap_or(0) as f32;
            if band > 0 {
                pn.opens = Some(band_opens(&pn, BAND_OPENS));
            }
            let pi = g.push(pn)?;
            index.insert(id.to_string(), pi);
            // the thread's order, a path through his posts in it (p1677, 2)
            if let Some(l) = last {
                g.link(Edge::new(l, pi, Link::Next))?;
            }
            last = Some(pi);
            let mut cells = band_cells(pr, band).into_iter();
            if !reached.is_empty()
                && let Some(r) = cells.next()
            {
                let mut bn = Node::new(format!("reached:{id}"), Kind::Agent, reached_words(&reached), Some(pi), r);
                bn.note = reached_note(&reached);
                bn.reads = Some(READS);
                bn.weight = reached.len() as f32;
                g.push(bn)?;
            }
            let told = deep.map(labels).unwrap_or_default();
            for (run, r) in runs.iter().zip(cells.by_ref()) {
                run_card(&mut g, pi, run, r, &told)?;
            }
            if !sessions.is_empty()
                && let Some(r) = cells.next()
            {
                carried_card(&mut g, pi, id, &sessions, r)?;
            }
            // its answers, the oldest first, bundled in one card (Sourcetrail's bundle: how many, and who) beside it in
            // the thread, at its piece, joined to it by a line (p1677, 2)
            let mut mine: Vec<&docket_machine::graph_slice::SliceNode> = answers_of.get(id).into_iter().flatten().filter_map(|a| nodes.get(a).copied()).collect();
            mine.sort_by(|a, b| (a.node.when.as_deref(), number(&a.node.id)).cmp(&(b.node.when.as_deref(), number(&b.node.id))));
            if mine.is_empty() {
                continue;
            }
            let bp = lay.posts[his_at[k] + 1];
            let br = Rect::new(r.x + bp.x, r.y + bp.y, bp.w, bp.h);
            let mut who: Vec<&str> = Vec::new();
            for a in &mine {
                if let Some(b) = a.by.as_deref().filter(|b| !who.contains(b)) {
                    who.push(b);
                }
            }
            let count = mine.len();
            let mut bn = Node::new(format!("answers:{id}"), Kind::Post, format!("{count} {}: {}", if count == 1 { "answer" } else { "answers" }, who.join(", ")), Some(ti), br);
            bn.note = mine.iter().filter_map(|a| a.node.when.as_deref()).max().map(|w| format!("the last {}", crate::when::at(w))).unwrap_or_default();
            bn.weight = count as f32;
            bn.opens = Some(BUNDLE_OPENS);
            bn.reads = Some(READS);
            let holder = g.push(bn)?;
            g.link(Edge::new(pi, holder, Link::Answered))?;
            for (j, (a, ar)) in mine.iter().zip(cells_in(br, count)).enumerate() {
                let aid = a.node.id.as_str();
                let by = a.by.as_deref();
                let words = said.get(aid).map(|l| l.label.as_str()).filter(|l| !l.is_empty()).unwrap_or(aid);
                let mut an = Node::new(aid, Kind::Post, words, Some(holder), ar);
                // who in its words ("paper-research: Not yet…"), else in its note
                let named = by.is_some_and(|b| words.starts_with(&format!("{b}:")));
                an.note = match (by.filter(|_| !named), a.node.when.as_deref()) {
                    (Some(b), Some(w)) => format!("{b} · {}", crate::when::at(w)),
                    (Some(b), None) => b.to_string(),
                    (None, Some(w)) => crate::when::at(w),
                    (None, None) => String::new(),
                };
                an.weight = (mine.len() - j) as f32;
                an.reads = Some(READS);
                // the focus's slice: the turn it was written in, in its lower part
                let turn = deep.and_then(|d| written_in(d, aid));
                if turn.is_some() {
                    an.opens = Some(band_opens(&an, TURN_OPENS));
                }
                let ai = g.push(an)?;
                index.insert(aid.to_string(), ai);
                if let Some(t) = turn {
                    let gap = gap_in(ar, 0.1);
                    let tr = Rect::new(ar.x + gap, ar.y + ar.h * WORDS + gap, ar.w - 2.0 * gap, ar.h * (1.0 - WORDS) - 2.0 * gap);
                    let by = t.by.as_deref().unwrap_or("an agent");
                    // what wrote the answer: the requests sent in its turn and their model (p1791, 3; proto-window p1795)
                    let words = match deep.map(|d| sent_in(d, t.node.id.as_str())) {
                        Some((n, model)) if n > 0 && !model.is_empty() => format!("{by}'s turn: {}, {model}", many(n, "request", "requests")),
                        Some((n, _)) if n > 0 => format!("{by}'s turn: {}", many(n, "request", "requests")),
                        _ => format!("{by}'s turn"),
                    };
                    let mut tn = Node::new(t.node.id.as_str(), Kind::Turn, words, Some(ai), tr);
                    tn.note = t.node.when.as_deref().map(|w| format!("ended {}", crate::when::at(w))).unwrap_or_default();
                    tn.reads = Some(READS);
                    g.push(tn)?;
                }
            }
        }
    }
    if let (Some((f, a)), Some(r)) = (alone, rects.get(order.len())) {
        focus_card(&mut g, p, f, a, *r, &index)?;
    }
    for (f, t) in named {
        let (Some(a), Some(b)) = (index.get(f), index.get(t).or_else(|| index.get(thread_of(t)))) else { continue };
        if a != b {
            g.link(Edge::new(*a, *b, Link::Names))?;
        }
    }
    for (f, a) in shared {
        if let (Some(x), Some(y)) = (index.get(f), index.get(a)) {
            g.link(Edge::new(*x, *y, Link::Answered))?;
        }
    }
    // each card's time and history, from the record's (its node's `when`), so the time bar's moment is drawn from this
    // one graph and never built again for it (canvas.rs `View::moment`)
    let times: HashMap<&str, i64> =
        base.slice.nodes.iter().chain(near.iter().flat_map(|(_, a)| a.slice.nodes.iter())).filter_map(|n| Some((n.node.id.as_str(), n.node.when.as_deref().and_then(docket_machine::machine::secs)?))).collect();
    g.date(|id| times.get(id).copied());
    Ok((g, changed))
}

/// The agents his post POST reached in slice A (`handed`, `woke`, `prompted`, `informed`), each with the strongest way
/// it did: those it handed it to or woke first, then those told at their next turn, by name within each.
fn reached<'a>(a: &'a GraphAnswer, post: &str) -> Vec<(&'a str, EdgeKind)> {
    let rank = |k: EdgeKind| match k {
        EdgeKind::Prompted => 0,
        EdgeKind::Woke => 1,
        EdgeKind::Handed => 2,
        _ => 3,
    };
    let mut by: BTreeMap<&str, EdgeKind> = BTreeMap::new();
    for e in &a.slice.edges {
        let k = e.edge.kind;
        if e.edge.from != post || !matches!(k, EdgeKind::Handed | EdgeKind::Woke | EdgeKind::Prompted | EdgeKind::Informed) {
            continue;
        }
        let who = e.edge.to.strip_prefix("agent:").unwrap_or(&e.edge.to);
        let w = by.entry(who).or_insert(k);
        if rank(k) < rank(*w) {
            *w = k;
        }
    }
    let mut v: Vec<(&str, EdgeKind)> = by.into_iter().collect();
    v.sort_by_key(|(who, k)| (rank(*k) == 3, *who));
    v
}

/// The reached card's words: how many, and the first of them by name ("reached 18: paper-research, docket-core, …").
fn reached_words(r: &[(&str, EdgeKind)]) -> String {
    const NAMED: usize = 6;
    let names: Vec<&str> = r.iter().take(NAMED).map(|(w, _)| *w).collect();
    let more = if r.len() > NAMED { ", …" } else { "" };
    format!("reached {}: {}{more}", r.len(), names.join(", "))
}

/// The reached card's note: how many each way ("woke 3 · handed 5 · told 10").
fn reached_note(r: &[(&str, EdgeKind)]) -> String {
    let count = |ks: &[EdgeKind]| r.iter().filter(|(_, k)| ks.contains(k)).count();
    [("woke", count(&[EdgeKind::Prompted, EdgeKind::Woke])), ("handed", count(&[EdgeKind::Handed])), ("told", count(&[EdgeKind::Informed]))]
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .map(|(w, n)| format!("{w} {n}"))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A run that handled his post (`handled_by`), in the focus's slice: the program it is a run of (`came_from`
/// `program:NAME`), when it began, and its steps in their order.
struct Ran<'a> {
    id: &'a str,
    program: Option<&'a str>,
    when: Option<&'a str>,
    steps: Vec<Stepped<'a>>,
}

/// A run's step: its prompt template (`came_from` `call:NAME`, its `.call` file) and its calls on the wire (`called`
/// `wire:STEP@MODEL`), each by its node and its model.
struct Stepped<'a> {
    id: &'a str,
    template: Option<&'a str>,
    wire: Vec<(&'a str, &'a str)>,
}

/// The runs slice A says handled post POST (graph_runs.rs `handled by`), the first first, each with its steps (`step_of`)
/// by their number (`O-….s1`), each step with its template and its calls. His p815's on research's record: `owner_post`
/// with one step, `gather`, which calls no model (proto-window N-1446 p1752).
fn handled<'a>(a: &'a GraphAnswer, post: &str) -> Vec<Ran<'a>> {
    let when: HashMap<&str, &str> = a.slice.nodes.iter().filter_map(|n| Some((n.node.id.as_str(), n.node.when.as_deref()?))).collect();
    let mut runs: Vec<Ran<'a>> = Vec::new();
    for e in a.slice.edges.iter().filter(|e| e.edge.kind == EdgeKind::HandledBy && e.edge.from == post) {
        if !runs.iter().any(|r| r.id == e.edge.to) {
            runs.push(Ran { id: e.edge.to.as_str(), program: None, when: when.get(e.edge.to.as_str()).copied(), steps: Vec::new() });
        }
    }
    let number = |id: &str| id.rsplit_once(".s").and_then(|(_, n)| n.parse::<u32>().ok()).unwrap_or(0);
    for r in &mut runs {
        for e in &a.slice.edges {
            match e.edge.kind {
                EdgeKind::CameFrom if e.edge.from == r.id => r.program = e.edge.to.strip_prefix("program:").or(r.program),
                EdgeKind::StepOf if e.edge.to == r.id && !r.steps.iter().any(|s| s.id == e.edge.from) => r.steps.push(Stepped { id: e.edge.from.as_str(), template: None, wire: Vec::new() }),
                _ => {}
            }
        }
        for s in &mut r.steps {
            let sid = s.id;
            for e in a.slice.edges.iter().filter(|e| e.edge.from == sid) {
                match e.edge.kind {
                    EdgeKind::CameFrom => s.template = e.edge.to.strip_prefix("call:").or(s.template),
                    EdgeKind::Called if !s.wire.iter().any(|(w, _)| *w == e.edge.to) => {
                        if let Some((_, model)) = e.edge.to.rsplit_once('@') {
                            s.wire.push((e.edge.to.as_str(), model));
                        }
                    }
                    _ => {}
                }
            }
        }
        r.steps.sort_by_key(|s| number(s.id));
    }
    runs
}

/// The requests that carried his post, by the agent's session they were sent in (their source's session file): the
/// agent (`sent_in` its turn, the turn's), how many, the latest one's model and effort, time and prompt tokens, and the
/// texts they carried (`carried`: the system prompt's blocks, the tools) and ran with (`ran_with`: its memory and
/// project files), each once: the Claude Code layer his p1104 digs down to ("i can dig down to the actual claude code
/// layer"), each text by its size, not its words.
struct Session<'a> {
    id: &'a str,
    by: Option<&'a str>,
    requests: usize,
    model: &'a str,
    last: Option<&'a str>,
    prompt: u64,
    texts: Vec<Block<'a>>,
}

/// A text a session's requests carried (`carried`) or ran with (`ran_with`): its node, what coordd says it is ("a
/// system prompt block, 57 bytes"), and how many of his post's requests had it, of how many sessions, each request
/// once though the record holds its edge twice (paper-research N-1446 p1821, H3: "carried by 29 requests of 4
/// sessions", p815's 57-byte block in all four).
struct Block<'a> {
    id: &'a str,
    what: &'a str,
    carried: bool,
    requests: usize,
    sessions: usize,
}

/// The sessions slice A says carried post POST (`for`: each request of the step it was handed in, follow.rs
/// `put_for`), the first first, each one's texts carried first, then the largest first. His p815's on research's record:
/// 29 requests in 4 sessions, proto-mind's 19, accounts-lead's 5, paper-research's 3 and docket-edge5's 2, each carrying
/// two system prompt blocks (57 bytes and some 9450) and its tools (proto-window N-1446 p1752).
fn carried<'a>(a: &'a GraphAnswer, post: &str) -> Vec<Session<'a>> {
    use docket_machine::graph::Source;
    let nodes: HashMap<&str, &docket_machine::graph_slice::SliceNode> = a.slice.nodes.iter().map(|n| (n.node.id.as_str(), n)).collect();
    let mut out: Vec<Session<'a>> = Vec::new();
    let mut of: HashMap<&str, usize> = HashMap::new();
    for e in a.slice.edges.iter().filter(|e| e.edge.kind == EdgeKind::For && e.edge.to == post) {
        let r = e.edge.from.as_str();
        let Some(n) = nodes.get(r).copied().filter(|_| !of.contains_key(r)) else { continue };
        let Source::Session(sid, _) = &n.node.source else { continue };
        let sid = sid.as_str();
        let k = match out.iter().position(|s| s.id == sid) {
            Some(k) => k,
            None => {
                out.push(Session { id: sid, by: None, requests: 0, model: "", last: None, prompt: 0, texts: Vec::new() });
                out.len() - 1
            }
        };
        let s = &mut out[k];
        s.requests += 1;
        if s.model.is_empty() || n.node.when.as_deref() >= s.last {
            (s.model, s.last) = (n.node.what.as_str(), n.node.when.as_deref());
            s.prompt = n.node.tokens.map_or(0, |t| t.prompt());
        }
        of.insert(r, k);
    }
    for e in &a.slice.edges {
        let Some(&k) = of.get(e.edge.from.as_str()) else { continue };
        let s = &mut out[k];
        let to = e.edge.to.as_str();
        match e.edge.kind {
            EdgeKind::SentIn if s.by.is_none() => s.by = nodes.get(to).copied().and_then(|t| t.by.as_deref()),
            EdgeKind::Carried | EdgeKind::RanWith if !s.texts.iter().any(|t| t.id == to) => {
                s.texts.push(Block { id: to, what: nodes.get(to).copied().map_or("", |t| t.node.what.as_str()), carried: e.edge.kind == EdgeKind::Carried, requests: 0, sessions: 0 });
            }
            _ => {}
        }
    }
    // each text's requests among his post's, and their sessions, each once
    let mut had: HashMap<&str, (HashSet<&str>, HashSet<usize>)> = HashMap::new();
    for e in a.slice.edges.iter().filter(|e| matches!(e.edge.kind, EdgeKind::Carried | EdgeKind::RanWith)) {
        if let Some(&k) = of.get(e.edge.from.as_str()) {
            let (rs, ss) = had.entry(e.edge.to.as_str()).or_default();
            rs.insert(e.edge.from.as_str());
            ss.insert(k);
        }
    }
    for s in &mut out {
        for t in &mut s.texts {
            (t.requests, t.sessions) = had.get(t.id).map_or((0, 0), |(rs, ss)| (rs.len(), ss.len()));
        }
        s.texts.sort_by_key(|t| (!t.carried, std::cmp::Reverse(bytes(t.what))));
    }
    out
}

/// A text's size, as coordd's `what` says it ("a system prompt block, 9455 bytes"; "24 tools, 33611 bytes"); 0 where
/// it says none.
fn bytes(what: &str) -> u64 {
    what.rsplit_once(", ").and_then(|(_, b)| b.strip_suffix(" bytes")).and_then(|b| b.parse().ok()).unwrap_or(0)
}

/// N of ONE, or of MORE when it is not one ("1 step", "19 requests").
fn many(n: usize, one: &str, more: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { more })
}

/// Run RUN's card at R in his post PI's band: the program it is a run of and how many steps, and in it a card for
/// each step, in the words coordd's labels TOLD give it ("his posts held together for a moment before any agent is
/// given them"), holding in its lower part its prompt template (its `.call` file) and each call it made on the wire.
fn run_card(g: &mut Graph, pi: u32, run: &Ran, r: Rect, told: &HashMap<&str, &coord_types::Label>) -> Result<(), Refused> {
    let steps = run.steps.len();
    let mut rn = Node::new(format!("run:{}", run.id), Kind::Run, format!("handled by {}", run.program.unwrap_or("a program")), Some(pi), r);
    rn.note = [Some(many(steps, "step", "steps")), run.when.map(crate::when::at)].into_iter().flatten().collect::<Vec<_>>().join(" · ");
    rn.reads = Some(READS);
    rn.weight = steps as f32;
    if steps > 0 {
        rn.opens = Some(RUN_OPENS);
    }
    let ri = g.push(rn)?;
    for (s, sr) in run.steps.iter().zip(cells_in(r, steps)) {
        let template = s.template.map(|t| format!("{t}.call"));
        let label = told.get(s.id).map(|l| l.label.clone()).filter(|l| !l.is_empty()).or_else(|| template.clone()).unwrap_or_else(|| s.id.to_string());
        let mut sn = Node::new(s.id, Kind::Step, label, Some(ri), sr);
        let number = s.id.rsplit_once(".s").map(|(_, n)| format!("step {n}"));
        let wire = if s.wire.is_empty() { "no call on the wire".to_string() } else { s.wire.iter().map(|(_, m)| *m).collect::<Vec<_>>().join(", ") };
        sn.note = [number, template.clone(), Some(wire)].into_iter().flatten().collect::<Vec<_>>().join(" · ");
        sn.reads = Some(READS);
        let inside = usize::from(template.is_some()) + s.wire.len();
        if inside > 0 {
            sn.opens = Some(band_opens(&sn, STEP_OPENS));
        }
        let si = g.push(sn)?;
        let mut cells = band_cells(sr, inside).into_iter();
        if let (Some(t), Some(name)) = (&template, s.template)
            && let Some(c) = cells.next()
        {
            let mut tn = Node::new(format!("{}/call:{name}", s.id), Kind::Text, format!("its prompt template, {t}"), Some(si), c);
            tn.note = "the call file it is formed from".to_string();
            tn.reads = Some(READS);
            g.push(tn)?;
        }
        for ((w, model), c) in s.wire.iter().zip(cells) {
            let mut wn = Node::new(*w, Kind::Call, format!("on the wire: {model}"), Some(si), c);
            wn.note = "the call for this step".to_string();
            wn.reads = Some(READS);
            g.push(wn)?;
        }
    }
    Ok(())
}

/// The card at R in his post PI's (ID's) band of the requests that carried it: how many and whose, and in it a card for
/// each session (the agent, how many, the latest's model and effort, prompt tokens and time), each holding the texts its
/// requests carried and ran with, each by its size.
fn carried_card(g: &mut Graph, pi: u32, id: &str, sessions: &[Session], r: Rect) -> Result<(), Refused> {
    let total: usize = sessions.iter().map(|s| s.requests).sum();
    let mut who: Vec<&str> = Vec::new();
    for b in sessions.iter().filter_map(|s| s.by) {
        if !who.contains(&b) {
            who.push(b);
        }
    }
    let mut qn = Node::new(format!("requests:{id}"), Kind::Post, format!("{} carried it: {}", many(total, "request", "requests"), who.join(", ")), Some(pi), r);
    let last = sessions.iter().filter_map(|s| s.last).max().map(|w| format!("the last {}", crate::when::at(w)));
    qn.note = [Some(many(sessions.len(), "session", "sessions")), last].into_iter().flatten().collect::<Vec<_>>().join(" · ");
    qn.opens = Some(BUNDLE_OPENS);
    qn.reads = Some(READS);
    qn.weight = total as f32;
    let qi = g.push(qn)?;
    for (s, sr) in sessions.iter().zip(cells_in(r, sessions.len())) {
        let sid = format!("requests:{id}/session:{}", s.id);
        let mut sn = Node::new(sid.as_str(), Kind::Call, format!("{}: {}, {}", s.by.unwrap_or("an agent"), many(s.requests, "request", "requests"), s.model), Some(qi), sr);
        let last = s.last.map(|w| format!("the last {}", crate::when::at(w)));
        sn.note = [Some(format!("{} prompt tokens", crate::circle::tokens(s.prompt))), last].into_iter().flatten().collect::<Vec<_>>().join(" · ");
        sn.reads = Some(READS);
        sn.weight = s.requests as f32;
        if !s.texts.is_empty() {
            sn.opens = Some(SESSION_OPENS);
        }
        let si = g.push(sn)?;
        for (t, tr) in s.texts.iter().zip(cells_in(sr, s.texts.len())) {
            let mut tn = Node::new(format!("{sid}/{}", t.id), Kind::Text, if t.what.is_empty() { t.id } else { t.what }, Some(si), tr);
            // who had it, of his post's requests (H3): "carried by 29 requests of 4 sessions", "ran with 19 requests of 1 session"
            tn.note = format!("{} {} of {}", if t.carried { "carried by" } else { "ran with" }, many(t.requests, "request", "requests"), many(t.sessions, "session", "sessions"));
            tn.reads = Some(READS);
            tn.weight = bytes(t.what) as f32;
            g.push(tn)?;
        }
    }
    Ok(())
}

/// The nodes one hop from F in slice A, in the slice's order, each once.
fn one_hop<'a>(a: &'a GraphAnswer, f: &str) -> Vec<&'a str> {
    let mut hop: Vec<&str> = Vec::new();
    for e in &a.slice.edges {
        let o = if e.edge.from == f {
            e.edge.to.as_str()
        } else if e.edge.to == f {
            e.edge.from.as_str()
        } else {
            continue;
        };
        if o != f && !hop.contains(&o) {
            hop.push(o);
        }
    }
    hop
}

/// Focus F of slice A, when the cloud draws no card of it, as a card of its own in project P at R (paper-research
/// N-1446 p1885, p1903, 3): its words coordd's label for it, else its node's `what` (a commit's subject; a job's verb
/// and totals), its kind and time under them. In its band the nodes one hop from it in A, but none the cloud draws
/// already (INDEX), by kind in the slice's order (p1916): a kind with one node is that node's card, a kind with more a
/// card saying how many ("18 jobs · 4 failed", lanes.rs `counted`) holding a card of each, opened as a card's band is.
/// A node that went wrong (`ok` false: a job that ended with an rc not 0) says "failed" in its note, nothing red.
fn focus_card(g: &mut Graph, p: u32, f: &str, a: &GraphAnswer, r: Rect, index: &HashMap<String, u32>) -> Result<(), Refused> {
    let told = labels(a);
    let nodes: HashMap<&str, &docket_machine::graph::Node> = a.slice.nodes.iter().map(|n| (n.node.id.as_str(), &n.node)).collect();
    let words = |id: &str| told.get(id).map(|l| l.label.clone()).filter(|l| !l.is_empty()).or_else(|| nodes.get(id).map(|n| n.what.clone()).filter(|w| !w.is_empty())).unwrap_or_else(|| id.to_string());
    let kind = |id: &str| nodes.get(id).map_or(NodeKind::Other, |n| n.kind);
    let failed = |id: &str| nodes.get(id).is_some_and(|n| n.ok == Some(false));
    let said = |id: &str| nodes.get(id).map_or(super::lanes::kind_words(NodeKind::Other).0, |n| super::lanes::node_word(a, n));
    let note = |id: &str| [Some(said(id).to_string()), failed(id).then(|| "failed".to_string()), nodes.get(id).and_then(|n| n.when.as_deref()).map(crate::when::at)].into_iter().flatten().collect::<Vec<_>>().join(" · ");
    let hop: Vec<&str> = one_hop(a, f).into_iter().filter(|o| !index.contains_key(*o)).collect();
    let mut kinds: Vec<(NodeKind, Vec<&str>)> = Vec::new();
    for o in &hop {
        match kinds.iter_mut().find(|(k, _)| *k == kind(o)) {
            Some((_, of)) => of.push(*o),
            None => kinds.push((kind(o), vec![*o])),
        }
    }
    let mut n = Node::new(f, Kind::Node(kind(f)), words(f), Some(p), r);
    n.note = note(f);
    n.reads = Some(READS);
    n.weight = hop.len() as f32;
    if !kinds.is_empty() {
        n.opens = Some(band_opens(&n, BAND_OPENS));
    }
    let fi = g.push(n)?;
    for ((k, of), c) in kinds.iter().zip(band_cells(r, kinds.len())) {
        // the node's own card, or its kind's, holding it with the rest of its kind
        let (holder, cells): (u32, Vec<(&str, Rect)>) = if let [o] = of.as_slice() {
            (fi, vec![(*o, c)])
        } else {
            let bad = of.iter().filter(|o| failed(o)).count();
            let said = [Some(super::lanes::counted(*k, of.len())), (bad > 0).then(|| format!("{bad} failed"))].into_iter().flatten().collect::<Vec<_>>().join(" · ");
            let mut on = Node::new(format!("{f}/{}", super::lanes::kind_words(*k).1), Kind::Node(*k), said, Some(fi), c);
            on.reads = Some(READS);
            on.weight = of.len() as f32;
            on.opens = Some(band_opens(&on, BAND_OPENS));
            (g.push(on)?, of.iter().copied().zip(band_cells(c, of.len())).collect())
        };
        for (o, c) in cells {
            let mut on = Node::new(o, Kind::Node(kind(o)), words(o), Some(holder), c);
            on.note = note(o);
            on.reads = Some(READS);
            g.push(on)?;
        }
    }
    Ok(())
}

/// The turn slice A says post POST was written in (`written_in`).
fn written_in<'a>(a: &'a GraphAnswer, post: &str) -> Option<&'a docket_machine::graph_slice::SliceNode> {
    let t = a.slice.edges.iter().find(|e| e.edge.kind == EdgeKind::WrittenIn && e.edge.from == post)?;
    a.slice.nodes.iter().find(|n| n.node.id == t.edge.to && n.node.kind == NodeKind::Turn)
}

/// How many requests slice A says were sent in TURN (`sent_in`), and the latest's model and effort ("claude-opus-5-5
/// xhigh"; empty where it says none): what wrote an answer written in that turn. His p815's on research's record:
/// accounts-lead's p816 in a turn of 5 requests, paper-research's p817 in one of 3, all claude-opus-5-5 xhigh; coordd's
/// watcher reading p816 for asks on gpt-6.2-sol is a run of its own, not this (proto-window N-1446 p1795). Each request
/// counts once: the slice may hold its edge twice (paper-research's session has each of p817's three twice, one per
/// line of the answer, its thinking and its tool use).
fn sent_in<'a>(a: &'a GraphAnswer, turn: &str) -> (usize, &'a str) {
    let nodes: HashMap<&str, &docket_machine::graph_slice::SliceNode> = a.slice.nodes.iter().map(|n| (n.node.id.as_str(), n)).collect();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut last: (Option<&str>, &str) = (None, "");
    for e in a.slice.edges.iter().filter(|e| e.edge.kind == EdgeKind::SentIn && e.edge.to == turn) {
        let Some(r) = nodes.get(e.edge.from.as_str()).filter(|_| seen.insert(e.edge.from.as_str())) else { continue };
        if last.1.is_empty() || r.node.when.as_deref() >= last.0 {
            last = (r.node.when.as_deref(), r.node.what.as_str());
        }
    }
    (seen.len(), last.1)
}

/// N cells in the band of his post's card R (below its words): one row while each would be at least half again as wide
/// as it is tall, as a bundle's ([`cells_in`]), else a grid of rows in that shape; each with a gap of a fifth of GAP
/// around it ([`gap_in`]). (Twice as wide, with the band three quarters of the card ([`WORDS`]), laid two answers in a
/// card of CARD's shape as two strips eight times as wide as tall, not side by side: paper-research N-1446 p1796, 1b.)
pub fn band_cells(r: Rect, n: usize) -> Vec<Rect> {
    let gap = gap_in(r, 0.2);
    grid(Rect::new(r.x + gap, r.y + r.h * WORDS + gap, r.w - 2.0 * gap, r.h * (1.0 - WORDS) - 2.0 * gap), n, gap, 1.5)
}

/// N cells inside the bundle's card R, below its head ([`BUNDLE_HEAD`]), a gap of a tenth of GAP around them
/// ([`gap_in`]): one row while each would be at least half again as wide as tall.
pub fn cells_in(r: Rect, n: usize) -> Vec<Rect> {
    let gap = gap_in(r, 0.1);
    let head = r.h * BUNDLE_HEAD;
    grid(Rect::new(r.x + gap, r.y + head, r.w - 2.0 * gap, r.h - head - gap), n, gap, 1.5)
}

/// The gap around and between the cells in card R: SHARE of GAP, at most a twentieth of R's shorter side, so a card
/// deep in the focus keeps room for what it holds. Still 3 of his p815: his post's card in the group chat is about
/// 145 by 54, a session in it about 13 by 4, and GAP's tenth left that session's five texts 0.33 wide and less than
/// none high; GAP's fifth left its run's step, about 60 by 4, less than no room for its prompt template.
fn gap_in(r: Rect, share: f64) -> f64 {
    (GAP * share).min(r.w.min(r.h) / 20.0)
}

/// N cells in BAND, GAP apart: one row while each would be at least WIDE times as wide as tall, else the fewest rows.
fn grid(band: Rect, n: usize, gap: f64, wide: f64) -> Vec<Rect> {
    if n == 0 {
        return Vec::new();
    }
    // as many columns as keep a cell at least 2 wide for 1 tall, the fewest rows that hold N
    let mut cols = n;
    while cols > 1 {
        let rows = n.div_ceil(cols) as f64;
        let (w, h) = ((band.w - (cols - 1) as f64 * gap) / cols as f64, (band.h - (rows - 1.0) * gap) / rows);
        if w >= wide * h {
            break;
        }
        cols -= 1;
    }
    let rows = n.div_ceil(cols);
    let (w, h) = ((band.w - (cols - 1) as f64 * gap) / cols as f64, (band.h - (rows - 1) as f64 * gap) / rows as f64);
    (0..n).map(|k| Rect::new(band.x + (k % cols) as f64 * (w + gap), band.y + (k / cols) as f64 * (h + gap), w, h)).collect()
}

// ---- asking ----------------------------------------------------------------------------------------------------------

/// The cloud made project PI's when it was another's, with its own answers, focus and places: at its poll, and at a
/// press that takes its focus before that poll (templates.rs `on_map`).
pub fn of_project(s: &mut App, pi: usize) {
    if s.cloud.of != pi {
        s.cloud = Cloud { of: pi, lanes: s.cloud.lanes, ..Cloud::default() };
    }
}

/// The cloud's frame step, from the Map's poll where a coordd serves the graph: his layer asked once a project shows
/// (and again at "Read again"); the slice around the focus asked with the layers on, again once either changes; and
/// the Map's graph built again from what came. Whether anything changed.
pub fn poll(s: &mut App) -> bool {
    let pi = s.cur;
    of_project(s, pi);
    if s.cloud.kept_read.is_none() {
        let at = super::cloud_place::path(s);
        let id = super::canvas::project_id(s, pi).unwrap_or_default();
        match super::cloud_place::read(&at, &id) {
            Ok(kept) => {
                s.cloud.kept = kept;
                s.cloud.kept_read = Some(Ok(()));
            }
            Err(e) => {
                crate::log::line(&format!("cloud: {} is left as it is, and the places are not kept: {e}", at.display()));
                s.cloud.kept_read = Some(Err(e));
            }
        }
    }
    let mut changed = false;
    let mut want = Vec::new();
    if s.cloud.base.is_none() || s.cloud.again {
        want.push(base_ask());
    }
    if let Some(near) = near_ask(s.cloud.focus.as_deref(), &s.cloud.layers)
        && (s.cloud.again || s.cloud.near.as_ref().is_none_or(|(a, _)| *a != near))
    {
        want.push(near);
    }
    s.cloud.again = false;
    for ask in want {
        if s.cloud.asked.contains(&ask) {
            continue;
        }
        if s.request_in(pi, coord_types::Request::Graph { ask: ask.clone() }, crate::Pending::Graph(ask.clone())) {
            s.cloud.asked.push(ask);
            changed = true;
        }
    }
    changed | build(s)
}

/// Project PI's answer to ASK (`Request::Graph`): the cloud's when it asked it, else the timeline page's (lanes.rs).
pub fn came(s: &mut App, pi: usize, ask: Ask, ok: bool, out: String) {
    if !(pi == s.cloud.of && s.cloud.asked.contains(&ask)) {
        return super::lanes::came(s, pi, ask, ok, out);
    }
    s.cloud.asked.retain(|a| *a != ask);
    let got = if ok { serde_json::from_str::<GraphAnswer>(&out).map_err(|e| format!("unreadable graph: {e}")) } else { Err(out) };
    if ask == base_ask() {
        s.cloud.base = Some(got);
    } else {
        s.cloud.near = Some((ask, got));
    }
}

/// The Map's graph built again from what the cloud holds, when that changed since it was last built: the places new to
/// it kept, and written when they could be read. Whether it was built.
pub fn build(s: &mut App) -> bool {
    let of = s.cloud.of;
    let (Some(project), Some(name)) = (super::canvas::project_id(s, of), s.projects.get(of).map(|p| p.name.clone())) else { return false };
    let (at, drawn) = (super::cloud_place::path(s), s.map.is_cloud());
    // his Archive (archive.rs): now, his posts from its moment on; the Archive shown, the answers as at its moment,
    // or at the time bar's when that is earlier
    let away = crate::archive::map_moment(s);
    // a new question or stop recovery is a new ticket: built again for it (fold.rs)
    let tickets = s.projects.get(of).map_or(0, |p| p.live.tickets.len());
    let c = &mut s.cloud;
    let Some(Ok(base)) = &c.base else { return false };
    let near = c.near.as_ref().filter(|(a, _)| Some(a) == near_ask(c.focus.as_deref(), &c.layers).as_ref()).and_then(|(_, r)| r.as_ref().ok());
    // the time bar's moment is not built: the graph at now is drawn as at it (canvas.rs `View::moment`); his Archive's
    // shown is
    let upto = match away {
        Some((t, true)) => Some(t),
        _ => None,
    };
    let from = away.filter(|(_, shown)| !shown).map(|(t, _)| t);
    let key = (base.slice.nodes.len() + base.slice.edges.len(), near.map_or(0, |a| a.slice.nodes.len() + a.slice.edges.len()), c.focus.clone(), c.layers.clone(), upto, from, tickets);
    if c.built.as_ref() == Some(&key) && drawn {
        return false;
    }
    // his questions and stop recoveries inside the thread they came from, worked out only for a build
    let into = s.projects.get(of).map(|p| super::fold::into(&p.live.tickets)).unwrap_or_default();
    // a past moment (the time bar's): the answers as they were then, the places kept read and never written, so what
    // came later keeps its room for when he comes back to now
    let began = std::time::Instant::now();
    let cut = |a: &GraphAnswer| -> GraphAnswer {
        let a = super::fold::folded(a, &into);
        let a = match from {
            Some(t) => since(&a, t),
            None => a,
        };
        match upto {
            Some(t) => as_at(&a, t),
            None => a,
        }
    };
    let then = (upto.is_some() || from.is_some() || !into.is_empty()).then(|| (cut(base), near.map(cut)));
    let built = match &then {
        Some((base, near)) if upto.is_some() => cloud(base, c.focus.as_deref().zip(near.as_ref()), &project, &name, &mut c.kept.clone()),
        Some((base, near)) => cloud(base, c.focus.as_deref().zip(near.as_ref()), &project, &name, &mut c.kept),
        None => cloud(base, c.focus.as_deref().zip(near), &project, &name, &mut c.kept),
    };
    let g = match built {
        Ok((g, changed)) => {
            // the build's time in his log, a past moment's and now's, as his release window takes it (paper-research
            // N-1446 p2192: a past build of his record took half a second in the quick build, p2177)
            let moment = upto.map_or_else(|| "now".to_string(), docket_machine::machine::at_secs);
            crate::log::line(&format!("cloud: built as at {moment} in {:.1} ms, {} nodes", began.elapsed().as_secs_f64() * 1e3, g.nodes().len()));
            if changed && c.kept_read == Some(Ok(())) && upto.is_none() {
                super::cloud_place::write(&at, project, &c.kept);
            }
            Some(g)
        }
        Err(e) => {
            crate::log::line(&format!("cloud: not built: {e:?}"));
            None
        }
    };
    c.built = Some(key);
    if let Some(g) = g {
        c.ticks = ticks_of(&g);
        let answers: Vec<&GraphAnswer> = [Some(base), near].into_iter().flatten().collect();
        c.marks = super::spacetime::marks_of(&answers, &g, &c.ticks);
        s.map.set_cloud(g);
        s.cloud.side = s.map.graph().and_then(|g| super::orbit::peripheral(s, g));
    }
    true
}

/// He went into the card ID (a click, a double-click, Enter, a row or a search's find): it is the focus, which
/// history.rs reads at the next frame as a place (docket-edge8 N-1446 p1762). A card the focus's slice drew inside it
/// (whom his post reached, the run that handled it and its steps, the requests that carried it and their texts) keeps
/// the focus: asked around its own id (`run:…`, `requests:…`), coordd knows none, and the cards he went into would be
/// gone.
pub fn went_in(s: &mut App, id: &str) {
    // the brain's own nodes (brain.rs): coordd has no slice around them, and the focus stays
    if let Some(g) = s.map.graph()
        && g.roots().find_map(|p| g.find_in(p, id)).is_some_and(|i| super::brain::ours(g, i))
    {
        return;
    }
    if let (Some(g), Some(focus)) = (s.map.graph(), s.cloud.focus.as_deref()) {
        let inside = g.roots().find_map(|p| g.find_in(p, id)).is_some_and(|i| g.path(i).iter().rev().skip(1).any(|p| g.node(*p).id == focus));
        if inside {
            return;
        }
    }
    s.cloud.focus = Some(id.to_string());
}

/// His press on card I in the cloud, after it went in (canvas.rs `drag`): a block a request carried opens its text
/// apart, as a folder's block does (popup.rs `What::Block`, its text asked of coordd by sidebar.rs
/// `apart_poll` as `Request::Raw` of its node; paper-research N-1446 p1821, H3), titled by the session's agent and the
/// block: "proto-mind · a system prompt block, 9449 bytes · its text". Only his press asks it: never a hover, a zoom,
/// a row or a search's find. Any other card's press is its going in alone: a file the requests ran with among them
/// ([`block_of`]).
pub fn pressed(s: &mut App, i: u32) {
    let opened = {
        let Some(g) = s.map.graph() else { return };
        let n = g.node(i);
        let (Some(id), Some(pi)) = (block_of(n), super::canvas::project_at(s, g, i)) else { return };
        // the session card's own words: "proto-mind: 19 requests, claude-opus-5-5 xhigh" (`carried_card`)
        let agent = n.parent.map_or("", |p| g.node(p).label.split_once(": ").map_or("", |(a, _)| a)).to_string();
        (crate::popup::title(&agent, &n.label, "its text"), crate::popup::What::Block { pi, agent, id: id.to_string() })
    };
    crate::popup::open(s, opened.0, opened.1);
}

/// Whose program or prompt template card I is, from the Kinds answer the window already holds (`s.tickets.kinds`, as
/// the threads and Kinds pages asked it; no new fetch: paper-research N-1446 p1821 H4, p1844): a run's handler by the
/// kind the snapshot says its `when` is written in (`ViewRan::from`), else by the one kind whose text has `when NAME`;
/// a prompt template by its call's file. In the editor's terms, one per thing ([`whose_words`]). None while the answer
/// is not held or is another project's (`kinds_of`: it is asked once, of the project shown then), for a handler two
/// kinds have, and for any other card.
fn whose(s: &App, g: &Graph, i: u32) -> Option<String> {
    let n = g.node(i);
    if !matches!(n.kind, Kind::Run | Kind::Text) || n.kind == Kind::Text && !n.id.contains("/call:") {
        return None;
    }
    let kinds = s.tickets.kinds.as_ref()?.as_ref().ok()?;
    let p = s.projects.get(super::canvas::project_at(s, g, i)?)?;
    if s.tickets.kinds_of.as_deref() != Some(p.id.as_str()) {
        return None;
    }
    let (file, by) = match n.kind {
        Kind::Run => {
            let (program, run) = (n.label.strip_prefix("handled by ")?, n.id.strip_prefix("run:")?);
            let from = p.live.tickets.values().flat_map(|t| &t.ran).find(|r| r.id == run).and_then(|r| r.from.as_deref());
            let k = match from {
                Some(f) => kinds.iter().find(|k| k.kind == f)?,
                None => {
                    let mut have = kinds.iter().filter(|k| writes_when(&k.text, program));
                    let k = have.next()?;
                    if have.next().is_some() {
                        return None;
                    }
                    k
                }
            };
            (k.file.as_str(), k.by.as_deref())
        }
        Kind::Text => {
            let (_, call) = n.id.rsplit_once("/call:")?;
            let mut files = kinds.iter().flat_map(|k| &k.calls).filter(|c| c.name == call).map(|c| c.file.as_str());
            let f = files.next()?;
            if files.any(|o| o != f) {
                return None;
            }
            (f, None)
        }
        _ => return None,
    };
    whose_words(s, p, file, by)
}

/// Whether kind TEXT has a `when NAME`: a line that starts, past its spaces, `when NAME(` or `when NAME `.
fn writes_when(text: &str, name: &str) -> bool {
    text.lines().any(|l| l.trim_start().strip_prefix("when ").and_then(|r| r.strip_prefix(name)).is_some_and(|r| r.starts_with(['(', ' '])))
}

/// Whose a kind or a call written in FILE is (BY: the agent whose own program it is, `kinds/agents/NAME.kind`), in the
/// editor's words: "proto-mind's own", "research's" (P's own, under its project file's folder), else as the editor
/// says it of the folders a project reads besides its own, "the library's" or "agent-base's" (extends edit.rs
/// `whose`, as [`crate::edit::whose_in`]). None from a coordd that sends no places.
fn whose_words(s: &App, p: &crate::Proj, file: &str, by: Option<&str>) -> Option<String> {
    if let Some(a) = by {
        return Some(format!("{a}'s own"));
    }
    let file = std::path::Path::new(file);
    if p.live.places.as_ref()?.definition.as_deref().is_some_and(|d| file.starts_with(d)) {
        return Some(format!("{}'s", p.name));
    }
    Some(format!("{}'s", crate::edit::whose_in(s, Some(p), file)))
}

/// The record's node of the block card N stands for: a text card in a session's (`requests:POST/session:S/text:…`,
/// [`carried_card`]) its requests carried, the text coordd reads apart. None for any other card: a prompt template is
/// a call file, a session's tools a list (`tools:…`), and a file the requests ran with (its memory, its note "ran with
/// …") is one coordd serves no text of (raw_off_lock: "only a system prompt block's text is served"), so a press on
/// it could only be refused (paper-research N-1446 p1889, 1: a window-high panel saying no).
fn block_of(n: &Node) -> Option<&str> {
    let (up, t) = n.id.rsplit_once('/').filter(|_| n.kind == Kind::Text && !n.note.starts_with("ran with "))?;
    (up.contains("/session:") && t.starts_with("text:")).then_some(t)
}

// ---- the drawing -----------------------------------------------------------------------------------------------------

/// The bar over the canvas: the presets, the layers one by one ("his messages" is the cloud itself, always on), "Whole
/// map" (canvas.rs `go_home`), "Read again" and the Brain switches, each giving the keys back to the Map once pressed
/// (canvas.rs `bar_press`). No line of counts. In a pane too narrow for them they wrap onto the next line, each whole,
/// never drawn past the pane's edge.
pub fn bar(s: &App, k: &Kit) -> El {
    let c = &s.cloud;
    let names: Vec<&str> = PRESETS.iter().map(|p| p.0).collect();
    let presets = SegmentedControl::new(&names, preset_of(&c.layers))
        .label("Show")
        .fit()
        .on_pick(|s: &mut App, i, tx: &mut rune::Tx| {
            s.cloud.layers = PRESETS[i].1.iter().copied().collect();
            super::canvas::keys_back(tx);
        })
        .build(k)
        .key("cloud-presets");
    let mut r = row().key("cloud-bar").w_full().flex_wrap().gap(12.0).px(12.0).py(6.0).items_center().child(presets);
    for l in Layer::ALL.into_iter().filter(|l| *l != Layer::His) {
        let on = c.layers.contains(&l);
        r = r.child(
            Switch::new(on)
                .label(super::lanes::word(l))
                .on_toggle_tx(super::canvas::bar_press(move |s: &mut App| {
                    if !s.cloud.layers.remove(&l) {
                        s.cloud.layers.insert(l);
                    }
                }))
                .build(k)
                .key(format!("cloud-switch-{l:?}")),
        );
    }
    let home = Button::new("Whole map").variant(ButtonVariant::Ghost).help("Whole map (Home)").on_press_tx(super::canvas::bar_press(super::canvas::go_home));
    let again = Button::new("Read again").variant(ButtonVariant::Ghost).help("Your messages now").on_press_tx(super::canvas::bar_press(|s: &mut App| s.cloud.again = true));
    r.child(home.build(k).key("cloud-home")).child(again.build(k).key("cloud-again")).children(super::brain::switches(s, k)).children(super::spacetime::switches(s, k))
}

/// The time bar's height, in points ([`time_bar`]).
const TIME_HIGH: f32 = 28.0;

/// The most ticks the time bar has: past it, each tick is the last of an even share of the moments (adaptive
/// buckets), so a long record's bar still has a tick a few points wide.
pub const TICKS: usize = 200;

/// The time bar's ticks for graph G ([`Cloud::ticks`]): every moment it changed at ([`super::Graph::moments`]), each a
/// turn's or a post's, or past [`TICKS`] the last of each of that many even shares; with each, how many of his posts came
/// after the tick before and by it.
pub fn ticks_of(g: &Graph) -> Vec<(i64, u32)> {
    let all = g.moments();
    let every = all.len().div_ceil(TICKS).max(1);
    let ticks: Vec<i64> = all.chunks(every).filter_map(|c| c.last().copied()).collect();
    let mut his = vec![0u32; ticks.len()];
    for n in g.nodes().iter().filter(|n| n.his && n.kind == Kind::Post && n.born != super::TIMELESS) {
        let k = ticks.partition_point(|t| *t < n.born);
        if let Some(c) = his.get_mut(k) {
            *c += 1;
        }
    }
    ticks.into_iter().zip(his).collect()
}

impl Cloud {
    /// The tick the Map is at: AT's, the last tick at or before it; now, the last.
    pub fn tick(&self) -> Option<usize> {
        let n = self.ticks.len();
        (n > 0).then(|| self.at.map_or(n - 1, |t| self.ticks.partition_point(|(x, _)| *x <= t).saturating_sub(1)))
    }

    /// The moment tick K shows: its time, the last tick now (None).
    fn moment_of(&self, k: usize) -> Option<i64> {
        (k + 1 < self.ticks.len()).then(|| self.ticks[k].0)
    }

    /// The span of the tick the Map is at, after the tick before it and up to it: what came or changed in it is lit
    /// ([`Cloud::fresh`]). None at now, nothing being lit then.
    pub fn span(&self) -> Option<(i64, i64)> {
        let at = self.at?;
        let k = self.tick()?;
        Some((if k == 0 { i64::MIN } else { self.ticks[k - 1].0 }, at))
    }

    /// Whether node N came or changed in the tick the Map is at ([`Cloud::span`]): its card's edge in the accent.
    pub fn fresh(&self, n: &Node) -> bool {
        self.span().is_some_and(|(lo, hi)| (n.born > lo && n.born <= hi) || n.events.iter().any(|e| e.at > lo && e.at <= hi))
    }

    /// The Map at the tick D ticks from the one it is at, the first and now its ends. Whether it moved.
    pub fn step(&mut self, d: i64) -> bool {
        let Some(k) = self.tick() else { return false };
        let to = (k as i64 + d).clamp(0, self.ticks.len() as i64 - 1) as usize;
        let at = self.moment_of(to);
        let moved = at != self.at;
        self.at = at;
        moved
    }
}

/// The tick X across the time bar's width W is over: the ticks spread evenly along it, one a turn or a post, not an
/// hour, so a busy hour is as wide as its turns and a quiet night takes no room.
fn tick_under(ticks: usize, x: f32, w: f32) -> usize {
    (((x / w.max(1.0)).clamp(0.0, 1.0) * ticks as f32) as usize).min(ticks.saturating_sub(1))
}

/// His press or drag on the time bar ([`time_bar`]): the Map at the tick under it, at once, as he presses and at each
/// move of the drag, as drawing it at a tick builds nothing (canvas.rs `View::moment`). The camera stays where it is
/// (canvas.rs `View::stay`); let go, the keys are the Map's again (canvas.rs `keys_back`), and a drag cancelled goes
/// back to where his press put it.
fn time_drag(s: &mut App, d: &rune::Drag, tx: &mut rune::Tx) {
    let k = tick_under(s.cloud.ticks.len(), d.x, d.w);
    let at = s.cloud.moment_of(k);
    if d.phase == rune::Phase::Up {
        super::canvas::keys_back(tx);
        if d.cancelled() {
            return;
        }
    }
    if at != s.cloud.at {
        s.cloud.at = at;
        s.map.set_moment(at);
        s.map.stay();
    }
}

/// The wheel's steps along the ticks for a wheel's delta D (its larger axis): one a notch, a trackpad's points adding up
/// to one ([`Cloud::wheel_rest`]); down and right later. Whether the Map moved.
pub fn wheel_steps(s: &mut App, e: &rune::WheelEvent) -> bool {
    let d = if e.dy.abs() >= e.dx.abs() { -e.dy } else { -e.dx };
    let rest = s.cloud.wheel_rest + d;
    let steps = (rest / super::canvas::NOTCH).trunc();
    s.cloud.wheel_rest = rest - steps * super::canvas::NOTCH;
    if steps == 0.0 {
        return false;
    }
    let moved = s.cloud.step(steps as i64);
    if moved {
        s.map.set_moment(s.cloud.at);
        s.map.stay();
    }
    moved
}

/// The time bar under the switches (paper-research N-1511 p42, 2; his N-1104 p849: "add a menu / span over it lets say
/// for time"; the owner N-1791 p1: by turns and events): a tick for each moment the Map changed at ([`ticks_of`]),
/// spread evenly, his posts' ticks taller by how many of his came then; the days apart by a line at the first tick of
/// each of his days, with its date; the moment shown ([`Cloud::at`]) a mark, the ticks after it faint; at the right,
/// that moment's time and which tick of how many, or "now", and "Now" back to it. A press or a drag on it is
/// [`time_drag`]; the wheel over it steps a tick a notch ([`wheel_steps`]), as Shift with the wheel over the canvas and
/// ← → do (canvas.rs `wheel`, `key`). Nothing where the cloud has no graph yet.
pub fn time_bar(s: &App, k: &Kit) -> El {
    let c = &s.cloud;
    let (Some(at), n) = (c.tick(), c.ticks.len()) else { return col().key("time-bar") };
    let (w, h) = (c.strip.max(1.0), TIME_HIGH);
    let step = w / n as f32;
    let most = c.ticks.iter().map(|(_, m)| *m).max().unwrap_or(1).max(1) as f32;
    let mut sh = Shape::new();
    // the days: a line at the first tick of each of his days, its date beside it where it fits before the bar's end
    let mut days = Vec::new();
    let mut day = crate::when::next_day(c.ticks[0].0);
    while let Some(t) = day {
        let i = c.ticks.partition_point(|(x, _)| *x < t);
        if i >= n {
            break;
        }
        let x = i as f32 * step;
        sh = sh.fill(vec![[x, 0.0], [x + 1.0, 0.0], [x + 1.0, h], [x, h]], k.c.border_muted);
        let first = c.ticks[i].0;
        let date = crate::when::day(first);
        if x + 4.0 + crate::picture::measure(&date, k.ty.small).0 <= w {
            days.push(text(date).font(k.ty.small).color(k.c.text_faint).single_line().absolute(x + 4.0, 0.0).key(format!("time-day-{first}")));
        }
        day = crate::when::next_day(first);
    }
    // each tick: a short mark, his posts' taller; those after the moment faint
    for (i, (_, his)) in c.ticks.iter().enumerate() {
        let tall = if *his > 0 { 5.0 + (h - 16.0) * (*his as f32 / most).sqrt() } else { 3.0 };
        let (x0, x1) = (i as f32 * step, (i as f32 + 1.0) * step - (step * 0.2).min(1.0));
        let ink = match (i <= at, *his > 0) {
            (true, true) => k.c.text_muted,
            (true, false) => k.c.text_faint,
            (false, _) => faded(k.c.text_faint, 0.35),
        };
        sh = sh.fill(vec![[x0, h - tall], [x1.max(x0 + 1.0), h - tall], [x1.max(x0 + 1.0), h], [x0, h]], ink);
    }
    // merges, holds, his asks and failed jobs: a dot each over its tick, in its ink
    for (i, m) in &c.marks {
        let (x, y) = ((*i as f32 + 0.5) * step, 3.0 + 4.0 * f32::from(*m as u8));
        sh = sh.fill(vec![[x - 1.5, y - 1.5], [x + 1.5, y - 1.5], [x + 1.5, y + 1.5], [x - 1.5, y + 1.5]], m.ink(k));
    }
    // the moment: a mark at its tick's end
    let x = ((at + 1) as f32 * step).min(w - 2.0);
    sh = sh.fill(vec![[x, 0.0], [x + 2.0, 0.0], [x + 2.0, h], [x, h]], k.c.text_accent);
    let strip = col()
        .key("time-strip")
        .w(0.0)
        .grow(1.0)
        .h(h)
        .cursor(rune::Cursor::Pointer)
        .child(canvas(sh).absolute(0.0, 0.0).size(w, h))
        .children(days)
        .on_layout(|s: &mut App, r| s.cloud.strip = r[2])
        .on_drag(|s: &mut App, d: &mut rune::Drag, tx: &mut rune::Tx| time_drag(s, d, tx))
        .on_wheel(|s: &mut App, e: rune::WheelEvent, _tx: &mut rune::Tx| {
            wheel_steps(s, &e);
            true
        });
    let said = match c.at {
        Some(t) => format!("{} · {} of {n}", crate::when::secs(t), at + 1),
        None => "now".to_string(),
    };
    let mut r = row().key("time-bar").w_full().gap(12.0).px(12.0).py(4.0).items_center().child(strip).child(text(said).font(k.ty.small_medium).color(k.c.text_base).single_line().key("time-said"));
    if c.at.is_some() {
        let now = Button::new("Now").variant(ButtonVariant::Ghost).help("The Map as it is now").on_press_tx(super::canvas::bar_press(|s: &mut App| s.cloud.at = None));
        r = r.child(now.build(k).key("time-now"));
    }
    r
}

/// What the page says while it has no cloud: being asked, or why there is none.
pub fn waiting(s: &App, k: &Kit) -> El {
    let why = match &s.cloud.base {
        Some(Err(e)) => e.clone(),
        _ => "Loading…".to_string(),
    };
    col().key("cloud-waiting").w_full().padding(24.0).child(text(why).font(k.ty.body).color(k.c.text_muted).wrap())
}

/// An asking post's dot in its card, across, in points (`card`; its ring round it, canvas.rs `mark_side`).
const MARK: f32 = 8.0;
/// The room a system prompt version's Edit takes at its card's top right ([`edit_of`]), wide and high, in points:
/// rune-kit's regular button, 24 high, "Edit" in it, and the card's inset around it.
const EDIT: [f32; 2] = [56.0, 32.0];
/// The room between an asking post's ring and its words (`card`), in points.
const MARK_GAP: f32 = 6.0;

/// Card I's Edit when it is a version of one agent's system prompt (lanes.rs `prompt_of`, read off the answers the
/// cloud holds, the slice around the focus first): the System prompt pane's own (prompt_edit.rs `edit_press`), its box
/// holding this version's text, the edit open apart over the Map (paper-research N-1446 p3041, 3; proto-window's walk,
/// p3038 3). None for any other card, and while its edit is open.
fn edit_of(s: &App, g: &Graph, i: u32, k: &Kit) -> Option<El> {
    let n = g.node(i);
    if n.kind != Kind::Node(NodeKind::Text) {
        return None;
    }
    let pi = super::canvas::project_at(s, g, i)?;
    let near = s.cloud.near.as_ref().and_then(|(_, a)| a.as_ref().ok());
    let base = s.cloud.base.as_ref().and_then(|a| a.as_ref().ok());
    let (agent, replace) = near.into_iter().chain(base).find_map(|a| super::lanes::prompt_of(a, &n.id))?;
    crate::prompt_edit::edit_press(s, pi, &agent, crate::prompt_edit::Start::Node(n.id.clone()), replace, k)
}

/// Node X of the cloud as a card at every size (p818: "bare nodes is bad"): its frame, filled, its edge lit when it is
/// selected or under the pointer, his posts' edge in the accent; inside, its words on as many lines as its height
/// takes, a thread's title and how many of his posts it holds in its head; open, its frame lighter, what it holds drawn
/// in it. The project is the canvas itself: nothing.
pub fn card(s: &App, g: &Graph, x: &super::lod::Shown, sel: bool, hover: bool, k: &Kit) -> El {
    let n = g.node(x.i);
    if n.kind == Kind::Project {
        return rune::el();
    }
    // what it says ([`says`]), and whether it stands for its siblings: a line is kept under its words for how many
    let standing = x.group.is_none() && x.many > 1 && !x.lit;
    let (label, note) = says(n, x, k.ty.small);
    // whose program or prompt template it is ([`whose`]), where it stands for itself: a run's after what it says of
    // itself, a template's first, as the editor says it ("agent-base's gather"), its one line cut at its end
    let note = match whose(s, g, x.i).filter(|_| x.group.is_none() && !standing) {
        Some(w) if n.kind == Kind::Text => format!("{w} {}", note.strip_prefix("the ").unwrap_or(&note)),
        Some(w) if note.is_empty() => w,
        Some(w) => format!("{note} · {w}"),
        None => note,
    };
    let a = x.alpha;
    let [cx, cy, cw, ch] = x.r;
    let (w, h) = (cw.max(4.0), ch.max(3.0));
    let open = x.look == super::lod::Look::Open;
    let faded = super::canvas::faded;
    // it came or changed in the time bar's tick (the owner, N-1791 p1: "a subtle accent border")
    let fresh = s.cloud.fresh(n);
    let edge = if sel {
        k.c.border_focus
    } else if hover {
        k.c.border_strong
    } else if fresh {
        faded(k.c.text_accent, 0.85)
    } else if n.his && n.kind == Kind::Post {
        faded(k.c.text_accent, 0.7)
    } else {
        k.c.border_muted
    };
    let fill = solid(g, x.i, open, k);
    let radius = k.radius.md.min(h / 3.0);
    let mut frame = rune::el().absolute(cx, cy).size(w, h).radius(radius).fill(faded(fill, a)).border(if sel || fresh { 2.0 } else { 1.0 }, faded(edge, a)).clip_rounded(radius);
    let line = |font| rune::text::measure("Ag", &[], font, None).1;
    let inner = w - 16.0;
    if inner < 28.0 {
        return frame;
    }
    // the part of it its words take: a thread's head once open, his post's words above its band once open, all else
    let world = n.rect;
    // the SHARE of it laid for its words over what it holds, at most their height: lod.rs `fit_to_shown` moves what it
    // shows up under them, and cuts it to them (p1839, 1)
    let laid = |share: f64| ((world.h * share) as f32 * cw / world.w as f32).min(words_height(g, x.i, cw)).min(h);
    let room = match n.kind {
        Kind::Thread if open => {
            let top = g.children(x.i).iter().map(|c| g.node(*c).rect.y).fold(world.bottom(), f64::min);
            (h * ((top - world.y) / world.h) as f32).clamp(0.0, h)
        }
        Kind::Post if open && (n.id.starts_with("answers:") || n.id.starts_with("requests:")) => laid(BUNDLE_HEAD),
        Kind::Post | Kind::Step if open && !g.children(x.i).is_empty() => laid(WORDS),
        // a run's head over its steps and a session's over its texts, laid as a bundle's ([`cells_in`])
        Kind::Run | Kind::Call if open && !g.children(x.i).is_empty() => laid(BUNDLE_HEAD),
        _ => h,
    };
    // its words' width: the card's inside, less an asking post's ring and the gap after it, which stand left of them
    // in one row (his posts' words ran that far past the card's right edge: "…its way" in cloud-2b of 8588ef17)
    let wide = words_wide(n, inner);
    // a system prompt version's Edit at its top right, where the card has the room, its words left of it
    let edit = (x.group.is_none() && !standing && room >= EDIT[1] && inner >= 3.0 * EDIT[0]).then(|| edit_of(s, g, x.i, k)).flatten();
    let wide = if edit.is_some() { wide - EDIT[0] } else { wide };
    // its history's chip, "◷ N" (ribbon.rs; the owner, N-1791 p1, 2), where it stands for itself and has two steps or
    // more by the time bar's moment: in its top row at its right, left of its Edit, its words narrowed by it so they
    // never run under it (p11, 1: half over the card's edge it covered the end of the first line); none where the card
    // has not the room for it beside 40 points of words
    let chip = (x.group.is_none() && !standing).then(|| super::ribbon::chip(g, x.i, s.map.moment())).flatten();
    let chip_w = chip.as_deref().map_or(0.0, |c| super::ribbon::chip_wide(c, k));
    let chip = chip.filter(|_| wide >= chip_w + CHIP_GAP + 40.0 && room >= super::ribbon::CHIP_HIGH + 8.0);
    let wide = if chip.is_some() { wide - chip_w - CHIP_GAP } else { wide };
    let title = title_type(n, x.group.is_some(), &label, !note.is_empty(), room, wide, k);
    let small = k.ty.small;
    let lines = ((room - 8.0) / line(title)).floor();
    if lines < 1.0 {
        return frame;
    }
    // a group's member's title on one line, how many more under it
    let most = lines.min(if x.group.is_some() {
        1.0
    } else if n.kind == Kind::Thread {
        2.0
    } else {
        CARD_LINES
    });
    // standing for its siblings, its words leave the line under them to how many more
    let kept = lines_over_note(room, title, k.ty.small);
    let most = if standing && kept >= 1.0 { most.min(kept) } else { most };
    // the lines its words take, wrapped as they are drawn, at most MOST: its note goes under them
    let took = (rune::text::measure(&label, &[], title, Some(wide)).1 / line(title) - 0.01).ceil().clamp(1.0, most);
    let words = text(label).font(title).color(faded(k.c.text_base, a)).max_lines(most as usize).w(wide);
    // standing for its siblings, its note directly under its words ([`lines_over_note`])
    let gap = if standing { 0.0 } else { 2.0 };
    let mut says = col().absolute(8.0, 4.0).w(inner).gap(gap).child(if n.ring.is_some() { row().gap(MARK_GAP).items_center().child(super::canvas::mark(n, x.i, MARK, a, k)).child(words) } else { words });
    // its note (how many of his posts, who and when) where a line is left for it under its words (a thread's title on
    // two lines once left its note half under the card's edge: "Kinds edited u… / 5 of your")
    // (a card cut to its words, [`words_height`], has exactly its note's line left: a hundredth's give for the sums)
    let used = took * line(title) + gap;
    if !note.is_empty() && room - 8.0 - used >= line(small) - 0.01 {
        // beside the chip where its words take less than the chip's height, narrowed by it too (p11, 1)
        let beside = chip.is_some() && used < super::ribbon::CHIP_HIGH;
        let room = if beside { inner - chip_w - CHIP_GAP } else { inner };
        says = says.child(text(note).font(small).color(faded(k.c.text_muted, a)).single_line().truncate().w(room));
    }
    frame = frame.child(says);
    if let Some(c) = chip {
        let x = w - 8.0 - chip_w - if edit.is_some() { EDIT[0] } else { 0.0 };
        let unfolded = s.cloud.unfolded.contains(&n.id);
        frame = frame.child(super::canvas::floating(super::ribbon::chip_el(&n.id, c, unfolded, k).absolute(x, 4.0)));
    }
    if let Some(e) = edit {
        frame = frame.child(super::canvas::floating(row().absolute(w - 8.0 - EDIT[0], 4.0).w(EDIT[0]).justify_end().child(e)));
    }
    frame
}

/// What node N's card X says (`card`), its title and its note, the note in SMALL: a group's heaviest member and how
/// many more (lod.rs `Shown::group`, its `i` that member); a card standing for its siblings too small for their words,
/// its own words and how many more as a group says it ([`group_words`]), first in its note, the kind's word left out
/// where the card's line holds no more, its own note after it where the line holds that too ("a system prompt block,
/// 9449 bytes" / "and 1 more" on still 4's card 75 points wide, "and 1 more text · carried" on a wider one): in still
/// 4 of his p815 a session's "a system prompt block, 94…" stood for the 57 bytes' block beside it on three lines of its
/// own words, and its "1 more here" had no line left (paper-research N-1446 p1791, 2; p1796); else its own.
fn says(n: &Node, x: &super::lod::Shown, small: TextStyle) -> (String, String) {
    match x.group {
        Some(_) => group_words(n.kind, x.many, &n.label),
        None if x.many > 1 && !x.lit => {
            let fits = |t: &str| rune::text::measure(t, &[], small, None).0 <= x.r[2].max(4.0) - 16.0;
            // as much as fits, the count the last to go: "and 2 more threads", "and 2 more", "+2" ("Kinds e… / and 2
            // m…" on standing threads about 70 points wide at the fit: paper-research N-1446 p1839, 2)
            let (_, more) = group_words(n.kind, x.many, "");
            let more = [more, format!("and {} more", x.many - 1)].into_iter().find(|t| fits(t)).unwrap_or_else(|| format!("+{}", x.many - 1));
            let with = format!("{more} · {}", n.note);
            (n.label.clone(), if !n.note.is_empty() && fits(&with) { with } else { more })
        }
        None => (n.label.clone(), n.note.clone()),
    }
}

/// The lines of TITLE a card's words ROOM high hold (`card`) leaving the line under them to its note, in SMALL,
/// directly under them: a card standing for its siblings keeps that line for how many more ([`says`]). (With a card's
/// gap of 2 between, still 4's text card, 45.6 high, kept one line of its words: "24 tools,…"; two take 46.)
fn lines_over_note(room: f32, title: TextStyle, small: TextStyle) -> f32 {
    let line = |font| rune::text::measure("Ag", &[], font, None).1;
    ((room - 8.0 - line(small) + 0.01) / line(title)).floor()
}

/// The type node N's title LABEL is drawn in (`card`), its card's words ROOM high and WIDE across: a thread's in its
/// neighbours' type, body_medium, where the room holds its words so with its note where it HAS one, as a card holding
/// none is cut to ([`words_height`]; a fixed 60 dropped a one-line title's 43 to the small type), else small_medium; a
/// group's (GROUPED) in small_medium, as before; any other's small.
fn title_type(n: &Node, grouped: bool, label: &str, has_note: bool, room: f32, wide: f32, k: &Kit) -> TextStyle {
    match n.kind {
        Kind::Thread if !grouped && room >= words_high(label, has_note, wide, k.ty.body_medium, k.ty.small, 2.0) => k.ty.body_medium,
        Kind::Thread => k.ty.small_medium,
        _ => k.ty.small,
    }
}

/// The width node N's words take in a card whose inside is INNER wide (`card`): an asking post's ring and the gap after
/// it stand left of them in one row.
fn words_wide(n: &Node, inner: f32) -> f32 {
    if n.ring.is_some() { inner - super::canvas::mark_side(n, MARK) - MARK_GAP } else { inner }
}

/// The height a card's words take on screen as `card` draws them: LABEL in TITLE wrapped at WIDE, on MOST lines at most
/// (a thread's two), and a line of SMALL under them where it HAS a note; 4 above, 2 between, 4 below.
fn words_high(label: &str, has_note: bool, wide: f32, title: TextStyle, small: TextStyle, most: f32) -> f32 {
    let line = |font| rune::text::measure("Ag", &[], font, None).1;
    let took = (rune::text::measure(label, &[], title, Some(wide)).1 / line(title) - 0.01).ceil().clamp(1.0, most);
    8.0 + took * line(title) + if has_note { 2.0 + line(small) } else { 0.0 }
}

/// How high node I's card is drawn on screen, W wide, while it holds no card (lod.rs `visible_worded`, which cuts it
/// to this; paper-research N-1446 p1731, docket-edge5 p1730; any card, p1796 1a): its title on the lines it wraps to
/// and its note's line, measured as `card` draws them, so a one-line title's card is lower than a two-line one's: a
/// thread's in its neighbours' type, body_medium, on two lines at most; any other's in the small type, on
/// [`CARD_LINES`] at most. Measured in the window's type scale (lib.rs draws with `Kit::neutral`, whose type is the
/// same dark or light). The project, or a card too narrow for words: lod.rs's [`super::lod::WORDS_HIGH`].
pub fn words_height(g: &Graph, i: u32, w: f32) -> f32 {
    words_of(g.node(i), w)
}

/// The side on screen at which card N opens its band, at least FROM: once [`WORDS`] of its height there holds its words
/// as [`words_height`] measures them at its width there, so a step whose words take two lines opens later than his
/// one-line post, from the measure and not a constant of its own (paper-research N-1446 p1796, 1b). The side grows by a
/// fiftieth till it does, at most to a hundred times FROM.
fn band_opens(n: &Node, from: f32) -> f32 {
    let (w, h) = (n.rect.w as f32, n.rect.h as f32);
    let side = (w * h).sqrt();
    if !(side > 0.0) {
        return from;
    }
    let mut s = from;
    while s < from * 100.0 && (WORDS as f32) * h * s / side < words_of(n, w * s / side) {
        s *= 1.02;
    }
    s
}

/// [`words_height`] of node N's card W wide on screen.
fn words_of(n: &Node, w: f32) -> f32 {
    static TYPE: std::sync::OnceLock<rune_kit::Type> = std::sync::OnceLock::new();
    let inner = w - 16.0;
    if n.kind == Kind::Project || inner < 28.0 {
        return super::lod::WORDS_HIGH;
    }
    let ty = TYPE.get_or_init(|| Kit::neutral(false).ty);
    let (title, most) = if n.kind == Kind::Thread { (ty.body_medium, 2.0) } else { (ty.small, CARD_LINES) };
    words_high(&n.label, !n.note.is_empty(), words_wide(n, inner), title, ty.small, most)
}

/// The most lines a card's words take in `card`, a thread's title aside (two).
const CARD_LINES: f32 = 12.0;

/// Where canvas.rs `board` draws each of DRAWN's edges in the cloud (lod.rs `edges`, of SHOWN): just over the deepest
/// node shown that holds both its ends (one end itself, when the other lies in it), and so under the two cards it joins
/// and what else that node holds. A link between two of his posts in one thread lies over the thread's card, a link
/// between two threads over the project's, each seen in the room between the cards it joins (paper-research N-1446
/// p1677: "the links aren't drawn", where every edge lay under every card). None: under all of them, as on the Map.
pub fn holders(g: &Graph, shown: &[super::lod::Shown], drawn: &[super::lod::Drawn]) -> Vec<Option<usize>> {
    let at: HashMap<u32, usize> = shown.iter().enumerate().filter(|(_, s)| !s.lit).map(|(k, s)| (s.i, k)).collect();
    let up = |i: u32| std::iter::successors(Some(i), |i| g.node(*i).parent);
    drawn
        .iter()
        .map(|e| {
            let from: HashSet<u32> = up(shown[e.from].i).collect();
            up(shown[e.to].i).filter(|i| from.contains(i)).find_map(|i| at.get(&i).copied())
        })
        .collect()
}

/// What a group's card says (lod.rs `Shown::group`; paper-research N-1446 p1715, 3: "Phone link, debug mo… and 4 more
/// threads", where "5 threads of one post" said nothing of what it holds): its heaviest member's title, MEMBER, and
/// how many more of KIND, MANY in all, in KIND's own words for one and more (canvas.rs `kind_words`: "and 3 more
/// branches", where an added "s" said "branchs", proto-window N-1446 p1951).
pub fn group_words(kind: Kind, many: u32, member: &str) -> (String, String) {
    let more = many.saturating_sub(1);
    let (one, more_of) = super::canvas::kind_words(kind);
    (member.to_string(), format!("and {more} more {}", if more == 1 { one } else { more_of }))
}

/// A link's width in the cloud, standing for COUNT of the graph's: thin, as Obsidian draws them, a little wider for
/// many.
pub fn link_width(count: u32) -> f32 {
    1.0 + (count as f32).log2().min(3.0) / 2.0
}

/// How much of the muted text's ink a link not lit takes in the cloud, and while another card is under the pointer.
const LINK_MUTED: f32 = 0.4;
const LINK_DIMMED: f32 = 0.18;

/// A link's ink in the cloud, ALPHA the less drawn of its ends': lit (an end under the pointer or selected), the text's;
/// else muted, the muted text's at [`LINK_MUTED`], grey against the canvas and the cards as Obsidian's lines are, dimmer
/// while another card is under the pointer (HOVERING). In the border's ink, a tenth of white, the lines of the first
/// stills with room were all but unseen (paper-research N-1446 p1677: "the links aren't drawn").
pub fn link_ink(lit: bool, hovering: bool, alpha: f32, k: &Kit) -> Color {
    if lit { k.c.text_base } else { super::canvas::faded(k.c.text_muted, if hovering { LINK_DIMMED } else { LINK_MUTED } * alpha) }
}

/// Card I's fill (OPEN: drawn open), opaque, as its layer looks over its parent's card and so on down to the canvas: a
/// link under a card is not seen through it, as one under a node of Obsidian's is not (the layers are a share of white:
/// a link to a post of his showed across it and across its band's card), and a card in a card still reads one layer
/// up. A thread's layer, open, a little of it.
fn solid(g: &Graph, i: u32, open: bool, k: &Kit) -> Color {
    let n = g.node(i);
    let layer = match n.kind {
        Kind::Thread if open => super::canvas::faded(k.c.layer_01, 0.6),
        Kind::Thread => k.c.layer_01,
        _ => k.c.layer_02,
    };
    let under = match n.parent {
        Some(p) if g.node(p).kind != Kind::Project => solid(g, p, true, k),
        _ => k.c.bg_deep,
    };
    over(layer, under)
}

/// TOP over the opaque UNDER, opaque, as the window blends them: in sRGB (rune `theme::blends_srgb`, its windows'
/// way), else in linear light. Mixed in linear light where the window blends in sRGB, the cards came out a flat light
/// grey.
fn over(top: Color, under: Color) -> Color {
    if !rune::theme::blends_srgb() {
        return under.mix(top.alpha(1.0), top.a).alpha(1.0);
    }
    let enc = |v: f32| if v <= 0.003_130_8 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    let dec = |v: f32| if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    let m = |t: f32, u: f32| dec(enc(u) + (enc(t) - enc(u)) * top.a);
    Color { r: m(top.r, under.r), g: m(top.g, under.g), b: m(top.b, under.b), a: 1.0 }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A made-up answer of his layer: his posts (`owner`) and the agents' that answer them, on threads N-1 to N-4, with
    /// the edges `posted_in`, `asks`, `closed_by`, `answered_upto` and `named`, and coordd's labels.
    pub(crate) fn base() -> GraphAnswer {
        let post = |id: &str, by: &str, when: &str| serde_json::json!({"id": id, "kind": "post", "line": 1, "when": when, "by": by});
        let ticket = |id: &str| serde_json::json!({"id": id, "kind": "ticket", "line": 1});
        let edge = |f: &str, t: &str, k: &str, when: &str| serde_json::json!({"from": f, "to": t, "kind": k, "line": 1, "when": when, "layer": "his"});
        let label = |id: &str, is: &str, l: &str| serde_json::json!({"id": id, "is": is, "label": l});
        let mut nodes = vec![ticket("N-1"), ticket("N-2"), ticket("N-3"), ticket("N-4")];
        let mut edges = Vec::new();
        let mut labels = vec![label("N-1", "ticket", "Your one group chat"), label("N-2", "ticket", "Codex and opencode logins"), label("N-3", "ticket", "The visual layer"), label("N-4", "ticket", "Spellcheck")];
        // his posts: five on N-1, two on N-2, one on N-3 and N-4
        let his = [("N-1.p1", "08:00"), ("N-1.p3", "08:10"), ("N-1.p5", "09:00"), ("N-1.p7", "09:30"), ("N-1.p9", "10:00"), ("N-2.p1", "08:05"), ("N-2.p4", "11:00"), ("N-3.p2", "12:00"), ("N-4.p1", "07:00")];
        for (id, hm) in his {
            let when = format!("2026-11-22T{hm}:00Z");
            nodes.push(post(id, "owner", &when));
            edges.push(edge(id, thread_of(id), "posted_in", &when));
            labels.push(label(id, "post", &format!("owner: his words in {id}")));
        }
        // agents' answers: N-1.p3 asks two agents, both answered; N-1.p9 asks one, open
        for (id, by, hm, to, kind) in [("N-1.p4", "paper-research", "08:12", "N-1.p3", "answered_upto"), ("N-1.p6", "docket-core", "09:05", "N-1.p3", "answered"), ("N-2.p2", "accounts-lead", "08:20", "N-2.p1", "answered_upto")] {
            let when = format!("2026-11-22T{hm}:00Z");
            nodes.push(post(id, by, &when));
            edges.push(edge(id, to, kind, &when));
            labels.push(label(id, "post", &format!("{by}: an answer in {id}")));
        }
        edges.push(edge("N-1.p3", "agent:paper-research", "asks", "2026-11-22T08:10:00Z"));
        edges.push(edge("N-1.p3", "N-1.p6", "closed_by", "2026-11-22T09:05:00Z"));
        // docket-core's answer in N-1 answers his first post in N-2 too: the two threads share it
        edges.push(edge("N-1.p6", "N-2.p1", "answered", "2026-11-22T09:05:00Z"));
        edges.push(edge("N-1.p9", "agent:docket-core", "asks", "2026-11-22T10:00:00Z"));
        // his posts name: N-1 → N-3 twice, N-2 → N-3 once
        edges.push(edge("N-1.p5", "N-3.p2", "named", "2026-11-22T09:00:00Z"));
        edges.push(edge("N-1.p7", "N-3", "named", "2026-11-22T09:30:00Z"));
        edges.push(edge("N-2.p4", "N-3.p2", "named", "2026-11-22T11:00:00Z"));
        // his words posted again on a handler's run, and his post with no words on a window's ticket: neither shown
        for (id, t, l) in [("O-1099511630505.p1", "O-1099511630505", "owner: his words in N-1.p9"), ("W-9.p2", "W-9", "owner")] {
            nodes.push(post(id, "owner", "2026-11-22T10:00:01Z"));
            nodes.push(ticket(t));
            edges.push(edge(id, t, "posted_in", "2026-11-22T10:00:01Z"));
            labels.push(label(id, "post", l));
        }
        serde_json::from_value(serde_json::json!({"slice": {"nodes": nodes, "edges": edges, "whole": {}, "shown": {}, "left": 0}, "labels": labels})).unwrap()
    }

    /// The slice around commit:3b2c4ac5 (templates.rs's made-up commit, his press on its row: p1885), its own layer
    /// and his: the commit, the job that ran on it, the build it went into, coordd's label for the commit.
    pub(crate) fn near_commit() -> GraphAnswer {
        let node = |id: &str, kind: &str, line: u32, hm: &str, what: &str| serde_json::json!({"id": id, "kind": kind, "line": line, "when": format!("2026-11-22T{hm}:00Z"), "what": what});
        let edge = |f: &str, t: &str, k: &str| serde_json::json!({"from": f, "to": t, "kind": k, "line": 1, "layer": "work"});
        let nodes = [node("commit:3b2c4ac5", "commit", 1, "10:00", "the read row, its last read's time"), node("job:7-2", "job", 2, "10:20", "test coordd: 252 passed"), node("build:69", "build", 3, "11:00", "")];
        let edges = [edge("job:7-2", "commit:3b2c4ac5", "ran"), edge("commit:3b2c4ac5", "build:69", "built_into")];
        let labels = [serde_json::json!({"id": "commit:3b2c4ac5", "is": "commit", "label": "the read row, its last read's time"})];
        serde_json::from_value(serde_json::json!({"slice": {"nodes": nodes, "edges": edges, "whole": {}, "shown": {}, "left": 0}, "labels": labels})).unwrap()
    }

    /// p1885 (p1903): any focus is a card. A commit his press on a template's row went to is asked in "Your messages"
    /// with its own layer, work, beside his, and in another style with that style's too; his post as the focus there
    /// asks nothing, nor a thread. The cloud draws the commit as an island of its own, as large as his post's card, its
    /// subject its words, "commit" and its time under them, holding a card for each node one hop from it: the job that
    /// ran on it, the build it went into, each its words; no thread's card meets it.
    /// The Map dated once, as built (the owner N-1791 p1, 1): each of his posts' cards at its post's time, a thread at
    /// its first post's, a link at the later of its ends'; a thread's history its posts coming after its first and the
    /// links made with it; the time bar's ticks each moment once, his posts counted in theirs.
    #[test]
    fn the_cloud_is_dated_once_and_its_ticks_are_its_moments() {
        let (g, _) = cloud(&base(), None, "project:research", "research", &mut BTreeMap::new()).unwrap();
        let at = |id: &str| g.roots().find_map(|p| g.find_in(p, id)).map(|i| g.node(i)).unwrap();
        let hm = |t: &str| docket_machine::machine::secs(&format!("2026-11-22T{t}:00Z")).unwrap();
        assert_eq!((at("N-1.p7").born, at("N-1").born, at("N-3").born, at("N-4").born), (hm("09:30"), hm("08:00"), hm("12:00"), hm("07:00")));
        let n1 = at("N-1");
        assert!(n1.events.windows(2).all(|w| w[0].at <= w[1].at), "oldest first");
        assert!(n1.events.iter().any(|e| e.at == hm("10:00") && matches!(e.what, super::super::Change::Child(_))), "p9 came into N-1 at 10:00: {:?}", n1.events);
        assert!(n1.moved_at(hm("09:30")) && !n1.moved_at(hm("11:00")));
        for e in g.edges() {
            assert_eq!(e.when, g.node(e.from).born.max(g.node(e.to).born), "a link at the later of its ends");
        }
        let t = ticks_of(&g);
        assert_eq!(t.len(), 12);
        assert_eq!(t.iter().map(|(_, h)| h).sum::<u32>(), 9, "his nine posts, each in its tick");
        assert_eq!(t[0], (hm("07:00"), 1));
    }

    /// The cost of the time bar's moment on a record the size of his (the owner N-1791 p1, 1: 60 frames a second, no
    /// build on a drag): 4,000 of his posts on 400 threads, each with an answer, made up. Built once at now; then the
    /// Map drawn at each of its 200 ticks, its whole in a 1600 by 1000 view and N-7 opened, by `visible_at` and
    /// `edges_at`, timed, against one build at a past moment the old way (`as_at`, then `cloud`). The times go to the
    /// test's output; what is asserted is that every tick draws only what was there by it.
    #[test]
    fn a_tick_is_drawn_in_the_frame_of_a_record_his_size() {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut labels = Vec::new();
        let t0 = docket_machine::machine::secs("2026-11-01T00:00:00Z").unwrap();
        for k in 0..4000i64 {
            let (thread, when, ans) = (format!("N-{}", k % 400 + 1), docket_machine::machine::at_secs(t0 + k * 600), docket_machine::machine::at_secs(t0 + k * 600 + 120));
            let (id, a) = (format!("{thread}.p{}", 2 * (k / 400) + 1), format!("{thread}.p{}", 2 * (k / 400) + 2));
            nodes.push(serde_json::json!({"id": id, "kind": "post", "line": 1, "when": when, "by": "owner"}));
            nodes.push(serde_json::json!({"id": a, "kind": "post", "line": 1, "when": ans, "by": "w1"}));
            edges.push(serde_json::json!({"from": id, "to": thread, "kind": "posted_in", "line": 1, "when": when, "layer": "his"}));
            edges.push(serde_json::json!({"from": a, "to": id, "kind": "answered", "line": 1, "when": ans, "layer": "his"}));
            labels.push(serde_json::json!({"id": id, "is": "post", "label": format!("owner: his words in {id}")}));
            labels.push(serde_json::json!({"id": a, "is": "post", "label": format!("w1: an answer in {a}")}));
        }
        for t in 1..=400 {
            nodes.push(serde_json::json!({"id": format!("N-{t}"), "kind": "ticket", "line": 1}));
        }
        let base: GraphAnswer = serde_json::from_value(serde_json::json!({"slice": {"nodes": nodes, "edges": edges, "whole": {}, "shown": {}, "left": 0}, "labels": labels})).unwrap();
        let began = std::time::Instant::now();
        let (g, _) = cloud(&base, None, "project:research", "research", &mut BTreeMap::new()).unwrap();
        let build = began.elapsed().as_secs_f64() * 1e3;
        let ticks = ticks_of(&g);
        assert_eq!(ticks.len(), TICKS);
        let view = [1600.0, 1000.0];
        let root = g.roots().next().unwrap();
        let whole = super::super::camera::Camera::fitting(&g.node(root).rect, view, 24.0);
        let n7 = g.find_in(root, "N-7").unwrap();
        let near = super::super::camera::Camera::fitting(&g.node(n7).rect, view, 24.0);
        let words = |i: u32, w: f32| words_height(&g, i, w);
        let mut worst = [0f64; 2];
        let mut sum = [0f64; 2];
        for (cam, k) in [(&whole, 0), (&near, 1)] {
            for (t, _) in &ticks {
                let began = std::time::Instant::now();
                let (shown, stand) = super::super::lod::visible_at(&g, cam, view, &[], &words, Some(*t));
                let drawn = super::super::lod::edges_at(&g, &shown, &stand, None, Some(*t));
                let ms = began.elapsed().as_secs_f64() * 1e3;
                worst[k] = worst[k].max(ms);
                sum[k] += ms;
                assert!(shown.iter().all(|x| g.node(x.i).born <= *t), "only what was there by the tick");
                std::hint::black_box(drawn);
            }
        }
        let began = std::time::Instant::now();
        let past = as_at(&base, ticks[TICKS / 2].0);
        let _ = cloud(&past, None, "project:research", "research", &mut BTreeMap::new()).unwrap();
        let old = began.elapsed().as_secs_f64() * 1e3;
        eprintln!(
            "time bar cost: {} nodes, {} links; built once {build:.1} ms; a tick drawn, whole: mean {:.2} ms, worst {:.2} ms; N-7 open: mean {:.2} ms, worst {:.2} ms; one build at a past moment the old way {old:.1} ms",
            g.nodes().len(),
            g.edges().len(),
            sum[0] / TICKS as f64,
            worst[0],
            sum[1] / TICKS as f64,
            worst[1]
        );
    }

    #[test]
    fn a_focus_that_is_none_of_his_posts_is_a_card_of_its_own() {
        let none = BTreeSet::new();
        let layers = |a: Option<Ask>| a.map(|a| a.layers.into_iter().collect::<Vec<_>>());
        assert_eq!(layers(near_ask(Some("commit:3b2c4ac5"), &none)), Some(vec![Layer::His, Layer::Work]), "Your messages: its own and his");
        let agents: BTreeSet<Layer> = PRESETS[1].1.iter().copied().collect();
        assert_eq!(layers(near_ask(Some("commit:3b2c4ac5"), &agents)), Some(vec![Layer::His, Layer::Agents, Layer::Work]), "With agents: its, its own and his");
        assert_eq!(near_ask(Some("N-1.p3"), &none), None, "his post: nothing in Your messages");
        assert_eq!(near_ask(Some("N-1"), &none), None, "a thread: nothing in Your messages");
        let (g, _) = cloud(&base(), Some(("commit:3b2c4ac5", &near_commit())), "project:research", "research", &mut BTreeMap::new()).unwrap();
        let p = g.project("project:research").unwrap();
        let c = g.find_in(p, "commit:3b2c4ac5").expect("the commit's card");
        let n = g.node(c);
        assert_eq!((super::super::canvas::kind_word(n.kind), n.label.as_str(), n.parent), ("commit", "the read row, its last read's time", Some(p)));
        assert!(n.note.starts_with("commit · ") && (n.rect.w, n.rect.h) == (CARD[0], CARD[1]), "{} {:?}", n.note, n.rect);
        let held: Vec<(&str, &str)> = g.children(c).iter().map(|k| (g.node(*k).id.as_str(), g.node(*k).label.as_str())).collect();
        assert_eq!(held, [("job:7-2", "test coordd: 252 passed"), ("build:69", "build:69")]);
        for t in g.children(p).iter().filter(|t| **t != c) {
            assert!(!g.node(*t).rect.meets(&n.rect), "{} meets it: {:?} {:?}", g.node(*t).id, g.node(*t).rect, n.rect);
        }
    }

    /// The slice around train 68's cut, commit:6467db1a, as coordd answered it on his record with work beside his
    /// (proto-window N-1446 p1912), cut to what is one hop from it: 40 nodes of nine kinds, its agent, its session, its
    /// worktree, its build, six commits (one it was merged into), eight branches, two threads named, two posts and 18
    /// jobs that ran on it, each also `ended` at it, four of them failed.
    pub(crate) fn near_cut() -> GraphAnswer {
        let f = "commit:6467db1a";
        let mut hops: Vec<(String, &str, &str, Option<bool>)> = vec![
            ("agent:docket-core".into(), "agent", "wrote", None),
            ("session:a056fb58".into(), "session", "came_from", None),
            ("worktree:/home/t/Documents/papers/soft-swarm-docket-train-37".into(), "worktree", "in", None),
            ("build:20261123-001147-6467db1a".into(), "build", "built_into", None),
            ("commit:2142baee".into(), "commit", "merged_into", None),
        ];
        hops.extend((1..=5).map(|k| (format!("commit:0c409ec{k}"), "commit", "came_from", None)));
        hops.extend(["main", "train-68", "core-actwait", "core-dups", "edge6-h5", "mind-h1", "mind-tmpl", "prod-client-docket-core"].map(|b| (format!("branch:soft-swarm/{b}"), "branch", "on", None)));
        hops.extend(["I-1509", "N-1446"].map(|t| (t.to_string(), "ticket", "named", None)));
        hops.extend(["N-1446.p862", "N-1446.p1341"].map(|p| (p.to_string(), "post", "named", None)));
        hops.extend((1..=18).map(|k| (format!("job:{k}"), "job", "ran", Some(![7, 8, 13, 14].contains(&k)))));
        let mut nodes = vec![serde_json::json!({"id": f, "kind": "commit", "line": 1, "when": "2026-11-22T23:10:00Z", "what": "yaaihh 0.3.10: the crate's version is the 0.3.x release this build is"})];
        let mut edges = Vec::new();
        for (k, (id, kind, edge, ok)) in hops.iter().enumerate() {
            let mut n = serde_json::json!({"id": id, "kind": kind, "line": k + 2, "when": "2026-11-22T23:46:00Z"});
            if let Some(ok) = ok {
                n["ok"] = serde_json::json!(ok);
                n["what"] = serde_json::json!(format!("test docket-machine: rc {}", if *ok { 0 } else { 101 }));
                edges.push(serde_json::json!({"from": id, "to": f, "kind": "ended", "line": 1, "layer": "work"}));
            }
            nodes.push(n);
            edges.push(serde_json::json!({"from": id, "to": f, "kind": edge, "line": 1, "layer": "work"}));
        }
        serde_json::from_value(serde_json::json!({"slice": {"nodes": nodes, "edges": edges, "whole": {}, "shown": {}, "left": 0}, "labels": []})).unwrap()
    }

    /// p1916: the focus's band holds its one-hop nodes grouped by kind, in the slice's order. A kind with one node is
    /// that node's own card; a kind with more is a card saying how many and how many failed, holding a card of each,
    /// opened as a card's band is. Train 68's cut 6467db1a, 40 nodes one hop from it on his record, is nine cards, not
    /// 40 flat; "18 jobs · 4 failed" holds its 18, each failed one saying so in its note.
    #[test]
    fn a_focus_band_holds_its_one_hop_nodes_grouped_by_kind() {
        let (g, _) = cloud(&base(), Some(("commit:6467db1a", &near_cut())), "project:research", "research", &mut BTreeMap::new()).unwrap();
        let c = g.find_in(g.project("project:research").unwrap(), "commit:6467db1a").expect("the cut's card");
        let held: Vec<&str> = g.children(c).iter().map(|k| g.node(*k).label.as_str()).collect();
        assert_eq!(held, ["agent:docket-core", "session:a056fb58", "worktree:/home/t/Documents/papers/soft-swarm-docket-train-37", "build:20261123-001147-6467db1a", "6 commits", "8 branches", "2 threads and items", "2 posts", "18 jobs · 4 failed"]);
        let jobs = *g.children(c).last().unwrap();
        assert!(g.node(jobs).opens.is_some(), "opened as a card's band is");
        let r = g.node(jobs).rect;
        let in_jobs: Vec<(&str, &str)> = g.children(jobs).iter().map(|k| (g.node(*k).id.as_str(), g.node(*k).note.as_str())).collect();
        assert_eq!(in_jobs.len(), 18, "{in_jobs:?}");
        let failed: Vec<&str> = in_jobs.iter().filter(|(_, n)| n.starts_with("job · failed · ")).map(|(i, _)| *i).collect();
        assert_eq!(failed, ["job:7", "job:8", "job:13", "job:14"]);
        for k in g.children(jobs) {
            let q = g.node(*k).rect;
            assert!(q.x >= r.x && q.y >= r.y && q.right() <= r.right() && q.bottom() <= r.bottom(), "inside its kind's card: {q:?} {r:?}");
        }
    }

    fn ids(g: &Graph, i: u32) -> Vec<&str> {
        g.children(i).iter().map(|c| g.node(*c).id.as_str()).collect()
    }

    /// The cloud opens on his posts alone: each in the thread it was posted in, oldest first, the thread titled and
    /// counting them; his ask's ring by the ledger; the agents' answers in a card beside his post, joined to it, each
    /// once, and a line from his other post an answer answers; the thread's order a path; his links; each post's
    /// weight its links (paper-research N-1446 p1677).
    /// paper-research N-1511 p42, 2: the answer as it was by 10:00 holds the edges by then and the nodes there by then;
    /// N-3.p2, written at 12:00, is not there though N-1.p5 named it at 09:00. The cloud of it: his chat with the four
    /// posts he had written, p3's ask closed at 09:05, p9's not yet asked; N-2 and N-4; no N-3.
    #[test]
    fn the_answer_as_at_a_moment_holds_what_was_there_by_then() {
        let to = "2026-11-22T09:59:59Z";
        let then = as_at(&base(), docket_machine::machine::secs(to).unwrap());
        assert!(then.slice.edges.iter().all(|e| e.edge.when.as_deref().is_some_and(|w| w <= to)), "the edges by then");
        let held: BTreeSet<&str> = then.slice.nodes.iter().map(|n| n.node.id.as_str()).collect();
        assert!(held.contains("N-1.p7") && !held.contains("N-1.p9") && !held.contains("N-3.p2"), "the nodes there by then: {held:?}");
        let (g, _) = cloud(&then, None, "project:research", "research", &mut BTreeMap::new()).unwrap();
        let p = g.project("project:research").unwrap();
        assert_eq!(ids(&g, p).into_iter().collect::<BTreeSet<_>>(), ["N-1", "N-2", "N-4"].into_iter().collect(), "no N-3 yet");
        let n1 = g.find_in(p, "N-1").unwrap();
        assert_eq!((ids(&g, n1), g.node(n1).note.as_str()), (vec!["N-1.p1", "N-1.p3", "answers:N-1.p3", "N-1.p5", "N-1.p7"], "4 of your posts"));
    }

    #[test]
    fn his_posts_are_cards_in_their_threads_each_joined_to_its_answers_card() {
        let mut kept = BTreeMap::new();
        let (g, changed) = cloud(&base(), None, "project:research", "research", &mut kept).unwrap();
        assert!(changed && kept.len() == 4, "each thread's place kept: {kept:?}");
        let p = g.project("project:research").unwrap();
        let threads: BTreeSet<&str> = ids(&g, p).into_iter().collect();
        assert_eq!(threads, ["N-1", "N-2", "N-3", "N-4"].into_iter().collect(), "not the run's copy, nor the post with no words");
        let n1 = g.find_in(p, "N-1").unwrap();
        assert_eq!(ids(&g, n1), ["N-1.p1", "N-1.p3", "answers:N-1.p3", "N-1.p5", "N-1.p7", "N-1.p9"], "his posts, oldest first, no agent's; after his post its answers' card");
        let t = g.node(n1);
        assert_eq!((t.kind, t.label.as_str(), t.note.as_str()), (Kind::Thread, "Your one group chat", "5 of your posts · 1 open"));
        let p3 = g.find_in(p, "N-1.p3").unwrap();
        let n = g.node(p3);
        assert_eq!((n.label.as_str(), n.his, n.ring), ("his words in N-1.p3", true, Some(Ring::Answered)));
        assert_eq!(g.node(g.find_in(p, "N-1.p9").unwrap()).ring, Some(Ring::Open));
        assert_eq!(g.node(g.find_in(p, "N-1.p1").unwrap()).ring, None);
        assert!(ids(&g, p3).is_empty(), "nothing inside his card");
        let bundle = g.find_in(p, "answers:N-1.p3").unwrap();
        assert_eq!(g.node(bundle).label, "2 answers: paper-research, docket-core");
        assert_eq!(ids(&g, bundle), ["N-1.p4", "N-1.p6"], "its two answers, oldest first");
        let a = g.node(g.find_in(p, "N-1.p6").unwrap());
        assert_eq!((a.label.as_str(), a.his), ("docket-core: an answer in N-1.p6", false));
        assert_eq!(a.note, crate::when::at("2026-11-22T09:05:00Z"), "who in its words, its note its time");
        // the bundle beside his card in the thread, apart from it; each answer inside the bundle
        let r = g.node(p3).rect;
        let br = g.node(bundle).rect;
        assert!(t.rect.holds(&br) && (br.x >= r.right() || br.right() <= r.x || br.y >= r.bottom() || br.bottom() <= r.y), "{br:?} beside {r:?}");
        for c in g.children(bundle) {
            assert!(br.holds(&g.node(*c).rect), "{:?} in the bundle {br:?}", g.node(*c).rect);
        }
        let links = |l: Link| -> BTreeSet<(&str, &str)> { g.edges().iter().filter(|e| e.link == l).map(|e| (g.node(e.from).id.as_str(), g.node(e.to).id.as_str())).collect() };
        // his links: N-1.p5 → N-3.p2, N-1.p7 → N-3 (the thread), N-2.p4 → N-3.p2
        assert_eq!(links(Link::Names), [("N-1.p5", "N-3.p2"), ("N-1.p7", "N-3"), ("N-2.p4", "N-3.p2")].into_iter().collect());
        // his post to its answers' card; N-2.p1 to docket-core's answer, under N-1.p3, that answers it too
        assert_eq!(links(Link::Answered), [("N-1.p3", "answers:N-1.p3"), ("N-2.p1", "answers:N-2.p1"), ("N-2.p1", "N-1.p6")].into_iter().collect());
        assert_eq!(ids(&g, g.find_in(p, "answers:N-2.p1").unwrap()), ["N-2.p2"], "the shared answer once, under N-1.p3");
        // each thread's order a path through his posts in it
        assert_eq!(links(Link::Next), [("N-1.p1", "N-1.p3"), ("N-1.p3", "N-1.p5"), ("N-1.p5", "N-1.p7"), ("N-1.p7", "N-1.p9"), ("N-2.p1", "N-2.p4")].into_iter().collect());
        // a post's weight its links: two answers; one answer and one shared; named twice; nothing
        let weight = |id: &str| g.node(g.find_in(p, id).unwrap()).weight;
        assert_eq!([weight("N-1.p3"), weight("N-2.p1"), weight("N-3.p2"), weight("N-1.p1")], [2.0, 2.0, 2.0, 0.0]);
        // a thread opens once its cards would read: a thread of five later than one of one, at its own side
        let one = g.node(g.find_in(p, "N-4").unwrap());
        assert!(one.opens.unwrap() < t.opens.unwrap(), "{:?} {:?}", one.opens, t.opens);
        // every card says the least its words read at, so none is drawn without them (lod.rs `READS`); the project not
        assert!(g.nodes().iter().all(|n| n.reads == if n.kind == Kind::Project { None } else { Some(READS) }), "reads");
    }

    /// A layer over the canvas, opaque, as the window paints it (sRGB): white at a tenth over black is a tenth of the way
    /// in sRGB, not in linear light (which came out a flat light grey); opaque on top stays itself.
    #[test]
    fn a_layer_over_the_canvas_is_as_the_window_blends_it() {
        let o = over(Color { r: 1.0, g: 1.0, b: 1.0, a: 0.1 }, Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 });
        let enc = |v: f32| 1.055 * v.powf(1.0 / 2.4) - 0.055;
        assert!(rune::theme::blends_srgb() && (enc(o.r) - 0.1).abs() < 1e-4 && o.a == 1.0, "{o:?}");
        let top = Color { r: 0.2, g: 0.3, b: 0.4, a: 1.0 };
        let same = over(top, Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 });
        assert!([same.r - top.r, same.g - top.g, same.b - top.b, same.a - top.a].iter().all(|d| d.abs() < 1e-5), "{same:?}");
    }

    /// A group's card (lod.rs `Shown::group`; paper-research N-1446 p1715, 3): its heaviest member's title, and how many
    /// more of its kind.
    #[test]
    fn a_groups_card_names_its_heaviest_member_and_how_many_more() {
        let w = |k: Kind, many: u32, member: &str| {
            let (a, b) = group_words(k, many, member);
            format!("{a} / {b}")
        };
        assert_eq!(w(Kind::Thread, 5, "Phone link, debug mode"), "Phone link, debug mode / and 4 more threads");
        assert_eq!(w(Kind::Thread, 2, "Spellcheck"), "Spellcheck / and 1 more thread");
        assert_eq!(w(Kind::Post, 9, "his words in N-1.p3"), "his words in N-1.p3 / and 8 more posts");
        // a kind's own word for more than one, not an "s" added: "and 3 more branchs" in p1885's still 1 (p1951)
        assert_eq!(w(Kind::Node(NodeKind::Branch), 4, "branch:soft-swarm/train-68"), "branch:soft-swarm/train-68 / and 3 more branches");
        assert_eq!(w(Kind::Node(NodeKind::Branch), 2, "branch:soft-swarm/main"), "branch:soft-swarm/main / and 1 more branch");
    }

    /// A card standing for its siblings too small for their words says how many more as a group's card does, first in
    /// its note and on a line kept for it under its words (paper-research N-1446 p1791, 2; p1796). Still 4 of his p815,
    /// lod's numbers there (proto-window's cloud_debug): a session's text card 75 by 45.6 points standing for one more,
    /// where "a system prompt block, 94…" took all three lines and said nothing of the 57 bytes' block.
    #[test]
    fn a_card_standing_for_its_siblings_says_and_1_more() {
        let k = Kit::neutral(false);
        let mut n = Node::new("text:b", Kind::Text, "a system prompt block, 9449 bytes", None, Rect::new(0.0, 0.0, 10.0, 10.0));
        n.note = "carried".into();
        let x = |w: f32, many: u32, lit: bool| super::super::lod::Shown { i: 0, r: [0.0, 0.0, w, 45.6], look: super::super::lod::Look::Card, alpha: 1.0, open: 0.0, lit, many, group: None };
        // at still 4's 75 points, "and 1 more text" is wider than its line: the count without the kind's word, and
        // without its own note, which would cut the count ("and 1 mo…" in the stills of e9905cb2)
        assert_eq!(says(&n, &x(75.0, 2, false), k.ty.small), ("a system prompt block, 9449 bytes".to_string(), "and 1 more".to_string()));
        assert_eq!(says(&n, &x(300.0, 3, false), k.ty.small).1, "and 2 more texts · carried");
        // alone, or a lit point standing for others (the Map's own words for those): its own
        assert_eq!(says(&n, &x(75.0, 1, false), k.ty.small).1, "carried");
        assert_eq!(says(&n, &x(75.0, 2, true), k.ty.small).1, "carried");
        // narrower than "and 2 more" (the fit's standing threads, about 70 points: "Kinds e… / and 2 m…"): the count
        // alone, the last to go (p1839, 2); a point wider, "and 2 more" again
        let wide = rune::text::measure("and 2 more", &[], k.ty.small, None).0 + 16.0;
        let with = if rune::text::measure("+2 · carried", &[], k.ty.small, None).0 + 16.0 <= wide - 1.0 { "+2 · carried" } else { "+2" };
        assert_eq!(says(&n, &x(wide - 1.0, 3, false), k.ty.small).1, with);
        assert_eq!(says(&n, &x(wide + 1.0, 3, false), k.ty.small).1, "and 2 more");
        // the line kept: its words' lines and the note's line directly under them fit the card's words' room, one more
        // would not
        let line = |f| rune::text::measure("Ag", &[], f, None).1;
        let s = line(k.ty.small);
        for room in [45.6, 46.0, 58.0, 70.0, 120.0] {
            let kept = lines_over_note(room, k.ty.small, k.ty.small);
            assert!(kept >= 1.0 && 8.0 + kept * s + s <= room + 0.01 && 8.0 + (kept + 1.0) * s + s > room + 0.01, "{room}: {kept} lines of {s}");
        }
        assert_eq!(lines_over_note(45.6, k.ty.small, k.ty.small), 2.0, "still 4's card: two lines of its words and the note's");
    }

    /// A thread's card holding no card yet is cut to its words as `card` draws them (lod.rs `visible_worded`, given
    /// `words_height` by canvas.rs `View::seen`; paper-research N-1446 p1731): its title in body_medium on the lines it
    /// wraps to and its note's line, so a one-line title's card is lower than a two-line one's; and `card` draws that
    /// title in body_medium at that height, its neighbours' type, where a fixed 60 dropped the one-line card's to the
    /// small type.
    #[test]
    fn a_thread_holding_none_is_its_words_height_in_its_neighbours_type() {
        let (g, _) = cloud(&base(), None, "project:research", "research", &mut Default::default()).unwrap();
        let k = Kit::neutral(false);
        let line = |f| rune::text::measure("Ag", &[], f, None).1;
        let (b, s) = (line(k.ty.body_medium), line(k.ty.small));
        let p = g.project("project:research").unwrap();
        let t = g.find_in(p, "N-1").unwrap();
        assert_eq!(g.node(t).label, "Your one group chat");
        let (one, two) = (words_height(&g, t, 400.0), words_height(&g, t, 100.0));
        assert!((one - (8.0 + b + 2.0 + s)).abs() < 1e-3 && (two - (8.0 + 2.0 * b + 2.0 + s)).abs() < 1e-3, "one line, then two: {one} {two} ({b} {s})");
        assert_eq!(words_height(&g, p, 400.0), super::super::lod::WORDS_HIGH, "the project: lod.rs's own");
        // its title's type at that height: body_medium; a point lower, small_medium; a group's, small_medium
        let n = g.node(t);
        for w in [400.0, 100.0] {
            let h = words_height(&g, t, w);
            let ty = |room: f32, grouped: bool| title_type(n, grouped, &n.label, true, room, w - 16.0, &k);
            assert_eq!((ty(h, false), ty(h - 1.0, false), ty(h, true)), (k.ty.body_medium, k.ty.small_medium, k.ty.small_medium), "{w} wide");
        }
        // drawn, through the cloud's view, zooming out from the whole: each thread holding none, at most its words'
        // height at the width it is drawn, grown or not (by the project's ladder, at least LADDER_CARD, or at its
        // `reads`: lod.rs `cut_to_words`, paper-research N-1446 p1742), and some cut to it
        let mut v = super::super::canvas::View::default();
        v.set_cloud(g.clone());
        let view = [1_400.0, 900.0];
        v.area = [0.0, 0.0, view[0], view[1]];
        let whole = super::super::camera::Camera::fitting(&g.node(p).rect, view, 0.0);
        let mut cut = Vec::new();
        for f in (0..16).map(|k| 0.85f64.powi(k)) {
            v.cam = super::super::camera::Camera { scale: whole.scale * f, ..whole };
            let shown = v.shown();
            for x in shown.iter().filter(|x| g.node(x.i).kind == Kind::Thread && x.group.is_none() && x.open <= 0.0) {
                if shown.iter().any(|y| g.node(y.i).parent == Some(x.i)) {
                    continue;
                }
                let h = words_height(&g, x.i, x.r[2]);
                assert!(x.r[3] <= h + 1e-3, "{} at {f}: {:?} over its words' {h}", g.node(x.i).id, x.r);
                if (x.r[3] - h).abs() < 1e-3 {
                    cut.push((g.node(x.i).id.clone(), x.r[3]));
                }
            }
        }
        assert!(!cut.is_empty(), "none cut to its words");
        assert!(cut.iter().any(|(_, h)| *h < super::super::lod::WORDS_HIGH), "measured, not lod.rs's 64: {cut:?}");
    }

    /// A card of words holding none is its words' height, as a thread holding none is (lod.rs `cut_to_words`, called
    /// for one drawn at its own size in `visible_worded`; paper-research N-1446 p1796, 1a): his posts in their threads,
    /// in the small type on the lines their words wrap to and their note's line; in the focus, the card of the agents
    /// his post reached (still 4 of his p815: "reached 21: …" on two lines over the top half of his post's band).
    #[test]
    fn a_card_holding_none_is_its_words_height() {
        let near = near_p3();
        let (g, _) = cloud(&base(), Some(("N-1.p3", &near)), "project:research", "research", &mut Default::default()).unwrap();
        let k = Kit::neutral(false);
        let s = rune::text::measure("Ag", &[], k.ty.small, None).1;
        let p = g.project("project:research").unwrap();
        let p5 = g.find_in(p, "N-1.p5").unwrap();
        assert_eq!((g.node(p5).label.as_str(), g.node(p5).note.is_empty()), ("his words in N-1.p5", false));
        let one = words_height(&g, p5, 400.0);
        assert!((one - (8.0 + s + 2.0 + s)).abs() < 1e-3, "his post's one line and its note's, measured: {one} ({s})");
        assert!(words_height(&g, p5, 60.0) > one, "on more lines when narrower");
        // drawn: in his thread, and at his post in the focus, zooming out from each; every card of words holding none
        // at most its words' height at the width it is drawn, and his posts and the reached card cut to it
        let mut v = super::super::canvas::View::default();
        v.set_cloud(g.clone());
        let view = [1_400.0, 900.0];
        v.area = [0.0, 0.0, view[0], view[1]];
        let mut cut: Vec<String> = Vec::new();
        for at in ["N-1", "N-1.p3"] {
            let fit = super::super::camera::Camera::fitting(&g.node(g.find_in(p, at).unwrap()).rect, view, 0.0);
            for f in (0..10).map(|k| 0.8f64.powi(k)) {
                v.cam = super::super::camera::Camera { scale: fit.scale * f, ..fit };
                for x in v.shown().iter().filter(|x| x.group.is_none() && !x.lit && g.node(x.i).reads.is_some() && g.children(x.i).is_empty()) {
                    let h = words_height(&g, x.i, x.r[2]);
                    assert!(x.r[3] <= h + 1e-3, "{} at {f} from {at}: {:?} over its words' {h}", g.node(x.i).id, x.r);
                    if (x.r[3] - h).abs() < 1e-3 {
                        cut.push(g.node(x.i).id.clone());
                    }
                }
            }
        }
        assert!(cut.iter().any(|c| c.starts_with("N-1.p") && g.node(g.find_in(p, c).unwrap()).his), "his posts cut to their words: {cut:?}");
        assert!(cut.iter().any(|c| c == "reached:N-1.p3"), "the reached card cut to its words: {cut:?}");
    }

    /// Every band takes the same share of its card, [`WORDS`], a quarter, and opens once that share holds the card's
    /// words as `words_height` measures them at its width there, not at a side of its own (paper-research N-1446
    /// p1796, 1b): his post, a step and an answer written in a turn; a step whose words take more lines than his post's
    /// opens later, by the measure.
    #[test]
    fn each_band_opens_once_its_share_holds_its_words() {
        let mut near = near_p3();
        near.slice.edges.push(serde_json::from_value(serde_json::json!({"from": "N-1.p4", "to": "turn:proto-mind@120", "kind": "written_in", "line": 1, "layer": "agents"})).unwrap());
        let (g, _) = cloud(&base(), Some(("N-1.p3", &near)), "project:research", "research", &mut Default::default()).unwrap();
        let p = g.project("project:research").unwrap();
        let mut need = Vec::new();
        for (id, floor) in [("N-1.p3", BAND_OPENS), ("O-1099511631050.s1", STEP_OPENS), ("O-1099511631050.s2", STEP_OPENS), ("N-1.p4", TURN_OPENS)] {
            let i = g.find_in(p, id).unwrap();
            let n = g.node(i);
            let o = n.opens.unwrap_or_else(|| panic!("{id} opens"));
            let (w, h) = (n.rect.w as f32, n.rect.h as f32);
            let side = (w * h).sqrt();
            // at side S on screen: the share's height and the words' there
            let at = |s: f32| ((WORDS as f32) * h * s / side, words_height(&g, i, w * s / side));
            let (room, words) = at(o);
            assert!(room >= words - 1e-3, "{id} opens at {o}: {room} for its words' {words}");
            let (room, words) = at(o / 1.03);
            assert!(o <= floor * 1.0001 || room < words, "{id} at {o}: a step less, {room} would hold its words' {words}");
            need.push((id, o, floor));
        }
        // his post and the first step open past their old sides (110, 120): the measure opens them, not a constant
        assert!(need.iter().filter(|(id, _, _)| ["N-1.p3", "O-1099511631050.s1"].contains(id)).all(|(_, o, floor)| *o > floor * 1.0001), "{need:?}");
        // the step's words ("his posts held together for a moment before any agent is given them") take more lines
        // than his post's ("his words in N-1.p3") at one width: it needs more of its share
        let (s1, p3) = (g.find_in(p, "O-1099511631050.s1").unwrap(), g.find_in(p, "N-1.p3").unwrap());
        assert!(words_height(&g, s1, 300.0) > words_height(&g, p3, 300.0), "{} {}", words_height(&g, s1, 300.0), words_height(&g, p3, 300.0));
        assert_eq!(WORDS, 0.25, "one share for every band");
    }

    /// p1839, 1: still 6 of his p815, each answer's card four times the turn card it holds. A card of words is drawn no
    /// larger than what it shows (lod.rs `fit_to_shown`): its words' height, and under it what it shows, moved up to a
    /// gap under its words, the card ending that gap under the lowest; the bundle and his post holding it so too.
    /// Closed, showing none of what it holds, it is its words' height; open with what it holds out of the view, it
    /// keeps room for that. What it shows is packed by column (p1883, 1).
    #[test]
    fn a_card_is_drawn_its_words_height_and_what_it_shows() {
        use super::super::lod;
        let mut near = near_p3();
        near.slice.edges.push(serde_json::from_value(serde_json::json!({"from": "N-1.p4", "to": "turn:proto-mind@120", "kind": "written_in", "line": 1, "layer": "agents"})).unwrap());
        let (g, _) = cloud(&base(), Some(("N-1.p3", &near)), "project:research", "research", &mut BTreeMap::new()).unwrap();
        let p = g.project("project:research").unwrap();
        let at = |id: &str| g.find_in(p, id).unwrap_or_else(|| panic!("{id} not laid"));
        let (p3, bundle, p4, turn) = (at("N-1.p3"), at("answers:N-1.p3"), at("N-1.p4"), at("turn:proto-mind@120"));
        let words = |i: u32, w: f32| words_height(&g, i, w);
        let view = [1400.0, 900.0];
        // the camera: N-1.p4's middle at the view's, its side SIDE on screen
        let a = g.node(p4).rect;
        let cam = |side: f32| {
            let scale = side as f64 / (a.w * a.h).sqrt();
            super::super::camera::Camera { at: [a.x + a.w / 2.0 - 700.0 / scale, a.y + a.h / 2.0 - 450.0 / scale], scale }
        };
        let drawn = |c: &super::super::camera::Camera, fit: bool| {
            let (mut s, stand) = lod::visible_worded(&g, c, view, &[], &words);
            if fit {
                lod::fit_to_shown(&g, c, &mut s, &stand, &words);
            }
            s
        };
        let r = |s: &[lod::Shown], i: u32| s.iter().find(|x| x.i == i).unwrap_or_else(|| panic!("{i} not drawn")).r;
        // open, its turn drawn: laid, the answer is three times the turn's card and more (still 6: four)
        let side = g.node(p4).opens.unwrap() * lod::FADE * 1.5;
        let c = cam(side);
        let (laid, s) = (drawn(&c, false), drawn(&c, true));
        assert!(r(&laid, p4)[3] > 3.0 * r(&laid, turn)[3], "laid: {:?} over {:?}", r(&laid, p4), r(&laid, turn));
        let (ar, tr) = (r(&s, p4), r(&s, turn));
        let gap = tr[0] - ar[0];
        let said = (r(&laid, p4)[3] * WORDS as f32).min(words(p4, ar[2]));
        assert!(gap > 0.0 && (tr[1] - (ar[1] + said + gap)).abs() < 0.01, "the turn a gap under its words: {tr:?} in {ar:?}, its words {said}");
        assert!((ar[1] + ar[3] - (tr[1] + tr[3] + gap)).abs() < 0.01, "the answer ends a gap under its turn: {ar:?} {tr:?}");
        assert!(ar[3] < 2.0 * tr[3] + 3.0 * gap + said, "{ar:?} {tr:?}");
        assert_eq!(tr[3], r(&laid, turn)[3], "the turn its words' height, as it was");
        // the bundle (his post's sibling in the thread, after it) and his post, each whole in the view at the fit of its
        // laid card, 0.92 of the view (a still's before p1894, 3): each ends a gap under the lowest of what it shows,
        // cut from what was laid for it
        let fit = |i: u32| {
            let q = g.node(i).rect;
            let scale = (1400.0 / q.w).min(780.0 / q.h) * 0.92;
            super::super::camera::Camera { at: [q.x + q.w / 2.0 - 700.0 / scale, q.y + q.h / 2.0 - 450.0 / scale], scale }
        };
        for (holder, what) in [(bundle, "the bundle"), (p3, "his post")] {
            let c = fit(holder);
            let (laid, s) = (drawn(&c, false), drawn(&c, true));
            let (h, kids): (_, Vec<_>) = (r(&s, holder), s.iter().filter(|x| g.node(x.i).parent == Some(holder)).map(|x| x.r).collect());
            let low = kids.iter().map(|k| k[1] + k[3]).fold(f32::MIN, f32::max);
            let gap = kids.iter().map(|k| k[0] - h[0]).fold(f32::MAX, f32::min);
            assert!(!kids.is_empty() && (h[1] + h[3] - (low + gap)).abs() < 0.01, "{what} {h:?} ends a gap {gap} under {low}");
            assert!(h[3] < r(&laid, holder)[3], "{what} cut: {h:?} of {:?}", r(&laid, holder));
        }
        // packed by column (p1883, 1): in his post's band the requests a gap under the agents it reached, not under the
        // run beside them, which is taller
        let s = drawn(&fit(p3), true);
        let (reached, run, requests) = (r(&s, at("reached:N-1.p3")), r(&s, at("run:O-1099511631050")), r(&s, at("requests:N-1.p3")));
        let gap = reached[0] - r(&s, p3)[0];
        assert!(run[0] > reached[0] && run[1] + run[3] > reached[1] + reached[3] + gap, "the run beside it, taller: {run:?} {reached:?}");
        assert!((requests[1] - (reached[1] + reached[3] + gap)).abs() < 0.01, "the requests a gap {gap} under the agents reached: {requests:?} under {reached:?}");
        // its top laid 50 points over the view's foot, its turn below the view: closed (its side under where it opens)
        // and showing none of what it holds, its words' height. Open, packed up in the bundle into the view, it holds
        // its turn as at any other view, a gap under its words, and ends a gap under it: laid below the view, the turn
        // is drawn all the same, as what a card that packs holds is (lod.rs `visible_worded`); left out, the answer
        // kept the turn's laid room for it, empty
        let foot = |side: f32| {
            let scale = side as f64 / (a.w * a.h).sqrt();
            super::super::camera::Camera { at: [a.x + a.w / 2.0 - 700.0 / scale, a.y - 850.0 / scale], scale }
        };
        let c = foot(g.node(p4).opens.unwrap() * 0.9);
        let s = drawn(&c, true);
        assert!(!s.iter().any(|x| x.i == turn), "its turn below the view");
        // closed it is a label, as it opens under a card's side: the cloud draws it as a card all the same
        let x = s.iter().find(|x| x.i == p4).unwrap();
        assert!(x.look == lod::Look::Label && g.node(p4).opens.unwrap() < lod::band_of(g.node(p4)).card, "{:?}", x.look);
        let (x, was) = (x.r, r(&drawn(&c, false), p4));
        assert!(x[3] == words(p4, x[2]) && x[3] < was[3], "closed: its words' height {}, {x:?}, laid {was:?}", words(p4, x[2]));
        let c = foot(side);
        let s = drawn(&c, true);
        let (ar, tr) = (r(&s, p4), r(&s, turn));
        let gap = tr[0] - ar[0];
        assert!(tr[1] < 900.0 && gap > 0.0 && (tr[1] - (ar[1] + said + gap)).abs() < 0.01, "its turn in the view, a gap under its words: {tr:?} in {ar:?}");
        assert!((ar[1] + ar[3] - (tr[1] + tr[3] + gap)).abs() < 0.01, "the answer ends a gap under its turn: {ar:?} {tr:?}");
    }

    /// The slice around his post N-1.p3, its focus: in its band, one card for the agents it reached, those it woke or
    /// handed it to first, then those told; inside its first answer's card, the turn it was written in.
    #[test]
    fn the_focus_holds_the_agents_it_reached_and_each_answers_turn() {
        let mut near = base();
        let edge = |f: &str, t: &str, k: &str| serde_json::from_value(serde_json::json!({"from": f, "to": t, "kind": k, "line": 1, "layer": "deliveries"})).unwrap();
        for (t, k) in [("agent:paper-research", "handed"), ("agent:docket-core", "woke"), ("agent:minder", "informed"), ("agent:docket-core", "informed")] {
            near.slice.edges.push(edge("N-1.p3", t, k));
        }
        near.slice.nodes.push(serde_json::from_value(serde_json::json!({"id": "turn:paper-research@100", "kind": "turn", "line": 100, "when": "2026-11-22T08:12:30Z", "by": "paper-research"})).unwrap());
        near.slice.edges.push(edge("N-1.p4", "turn:paper-research@100", "written_in"));
        // the requests sent in that turn, one of them twice as paper-research's session has p817's; the latest says
        // the model and effort it ran on
        for (r, at, what) in [("request:r1", "08:12:01", "claude-opus-5-5 high"), ("request:r3", "08:12:29", "claude-opus-5-5 xhigh"), ("request:r2", "08:12:10", "claude-opus-5-5 high")] {
            near.slice.nodes.push(serde_json::from_value(serde_json::json!({"id": r, "kind": "request", "session": ["s-pr", 10], "what": what, "when": format!("2026-11-22T{at}Z")})).unwrap());
            near.slice.edges.push(edge(r, "turn:paper-research@100", "sent_in"));
        }
        near.slice.edges.push(edge("request:r1", "turn:paper-research@100", "sent_in"));
        let (g, _) = cloud(&base(), Some(("N-1.p3", &near)), "project:research", "research", &mut BTreeMap::new()).unwrap();
        let p = g.project("project:research").unwrap();
        let p3 = g.find_in(p, "N-1.p3").unwrap();
        assert_eq!(ids(&g, p3), ["reached:N-1.p3"]);
        assert_eq!(ids(&g, g.find_in(p, "answers:N-1.p3").unwrap()), ["N-1.p4", "N-1.p6"]);
        let r = g.node(g.find_in(p, "reached:N-1.p3").unwrap());
        assert_eq!((r.label.as_str(), r.note.as_str()), ("reached 3: docket-core, paper-research, minder", "woke 1 · handed 1 · told 1"));
        let p4 = g.find_in(p, "N-1.p4").unwrap();
        assert_eq!(ids(&g, p4), ["turn:paper-research@100"]);
        let t = g.node(g.find_in(p, "turn:paper-research@100").unwrap());
        assert_eq!((t.kind, t.label.as_str()), (Kind::Turn, "paper-research's turn: 3 requests, claude-opus-5-5 xhigh"));
        assert!(t.note.starts_with("ended "), "{}", t.note);
        let a = g.node(p4).rect;
        assert!(a.holds(&t.rect) && t.rect.y >= a.y + a.h * WORDS, "{:?} in the lower part of {a:?}", t.rect);
        // the reached card in his card's band, below his words
        let (pr, rr) = (g.node(p3).rect, r.rect);
        assert!(pr.holds(&rr) && rr.y >= pr.y + pr.h * WORDS, "{rr:?} in the band of {pr:?}");
        // another post the focus: nothing of the slice in N-1.p3
        let (h, _) = cloud(&base(), Some(("N-1.p9", &near)), "project:research", "research", &mut BTreeMap::new()).unwrap();
        assert!(ids(&h, h.find_in(h.project("project:research").unwrap(), "N-1.p3").unwrap()).is_empty());
    }

    /// The slice around his post N-1.p3 as coordd gives his p815's (proto-window p1752), on [`base`]: the run of
    /// `owner_post` that handled it, its steps `gather` (no call) and `route` (gpt-6.2-sol), and three requests in
    /// proto-mind's and accounts-lead's sessions with the texts they carried and ran with (still 3).
    pub(crate) fn near_p3() -> GraphAnswer {
        let mut near = base();
        let node = |v: serde_json::Value| -> docket_machine::graph_slice::SliceNode { serde_json::from_value(v).unwrap() };
        let edge = |f: &str, t: &str, k: &str| -> docket_machine::graph_slice::SliceEdge { serde_json::from_value(serde_json::json!({"from": f, "to": t, "kind": k, "line": 1, "layer": "calls"})).unwrap() };
        let (run, s1, s2) = ("O-1099511631050", "O-1099511631050.s1", "O-1099511631050.s2");
        near.slice.edges.push(serde_json::from_value(serde_json::json!({"from": "N-1.p3", "to": "agent:accounts-lead", "kind": "handed", "line": 1, "layer": "deliveries"})).unwrap());
        for v in [
            serde_json::json!({"id": run, "kind": "ticket", "line": 2, "when": "2026-11-22T08:10:00Z"}),
            serde_json::json!({"id": "program:owner_post", "kind": "program", "line": 1}),
            serde_json::json!({"id": s2, "kind": "step", "line": 4}),
            serde_json::json!({"id": s1, "kind": "step", "line": 3}),
            serde_json::json!({"id": "call:gather", "kind": "call_file", "line": 3}),
            serde_json::json!({"id": "call:route", "kind": "call_file", "line": 4}),
            serde_json::json!({"id": "wire:O-1099511631050.s2@gpt-6.2-sol", "kind": "call", "line": 5}),
        ] {
            near.slice.nodes.push(node(v));
        }
        for (f, t, k) in [("N-1.p3", run, "handled_by"), (run, "program:owner_post", "came_from"), (s2, run, "step_of"), (s1, run, "step_of"), (s1, "call:gather", "came_from"), (s2, "call:route", "came_from"), (s2, "wire:O-1099511631050.s2@gpt-6.2-sol", "called")] {
            near.slice.edges.push(edge(f, t, k));
        }
        near.labels.push(serde_json::from_value(serde_json::json!({"id": s1, "is": "step", "label": "his posts held together for a moment before any agent is given them"})).unwrap());
        // proto-mind's session with two requests, accounts-lead's with one; their blocks, tools and a memory file
        let tokens = serde_json::json!({"input": 2, "output": 517, "cache_read": 130565, "cache_write": 1381});
        for (r, sid, at, turn) in [("request:p1", "s-proto", "08:11:00", "turn:proto-mind@120"), ("request:a1", "s-acc", "08:11:10", "turn:accounts-lead@110"), ("request:p2", "s-proto", "08:11:30", "turn:proto-mind@120")] {
            near.slice.nodes.push(node(serde_json::json!({"id": r, "kind": "request", "session": [sid, 10], "what": "claude-opus-5-5 xhigh", "when": format!("2026-11-22T{at}Z"), "tokens": tokens.clone()})));
            near.slice.edges.push(edge(r, "N-1.p3", "for"));
            near.slice.edges.push(edge(r, turn, "sent_in"));
        }
        for (id, what) in [("text:a", "a system prompt block, 57 bytes"), ("text:b", "a system prompt block, 9449 bytes"), ("tools:c", "22 tools, 31622 bytes"), ("text:m", "AutoMem /m/MEMORY.md, 9476 bytes"), ("text:d", "a system prompt block, 9460 bytes")] {
            let kind = if id.starts_with("tools") { "tools" } else { "text" };
            near.slice.nodes.push(node(serde_json::json!({"id": id, "kind": kind, "session": ["s-proto", 2], "what": what})));
        }
        for (by, line) in [("proto-mind", 120), ("accounts-lead", 110)] {
            near.slice.nodes.push(node(serde_json::json!({"id": format!("turn:{by}@{line}"), "kind": "turn", "line": line, "by": by})));
        }
        for (r, t, k) in [("request:p1", "text:a", "carried"), ("request:p1", "text:b", "carried"), ("request:p1", "tools:c", "carried"), ("request:p1", "text:m", "ran_with"), ("request:p2", "text:a", "carried"), ("request:p2", "text:b", "carried"), ("request:a1", "text:a", "carried"), ("request:a1", "text:d", "carried")] {
            near.slice.edges.push(edge(r, t, k));
        }
        // the record holds an edge twice, one per line of a message (proto-window N-1446 p1837): counted once
        near.slice.edges.push(edge("request:p2", "text:b", "carried"));
        near
    }

    /// Still 3 (paper-research N-1446 p1734; the owner, N-1104 p599: "i can dig down to the actual claude code layer"):
    /// the slice around his post N-1.p3 as coordd gives his p815's (proto-window p1752), and in its band, after whom it
    /// reached, the run of `owner_post` that handled it and the requests that carried it. The run holds its steps by
    /// number, each in coordd's words where it has some, else its template's name, each holding its prompt template and
    /// its calls on the wire (`gather` calls none, `route` gpt-6.2-sol); the requests card holds a card for each
    /// agent's session, the first first, and each session the texts its requests carried, the largest first, then
    /// those they ran with.
    #[test]
    fn the_focus_holds_the_run_that_handled_it_and_the_requests_that_carried_it() {
        let near = near_p3();
        let (s1, s2) = ("O-1099511631050.s1", "O-1099511631050.s2");
        let (g, _) = cloud(&base(), Some(("N-1.p3", &near)), "project:research", "research", &mut BTreeMap::new()).unwrap();
        let p = g.project("project:research").unwrap();
        let at = |id: &str| g.find_in(p, id).unwrap_or_else(|| panic!("{id} not drawn"));
        let says = |id: &str| (g.node(at(id)).kind, g.node(at(id)).label.clone(), g.node(at(id)).note.clone());
        let p3 = at("N-1.p3");
        assert_eq!(ids(&g, p3), ["reached:N-1.p3", "run:O-1099511631050", "requests:N-1.p3"], "whom it reached, its run, the requests");
        // each in its band, below his words
        let pr = g.node(p3).rect;
        for c in g.children(p3) {
            let r = g.node(*c).rect;
            assert!(pr.holds(&r) && r.y >= pr.y + pr.h * WORDS, "{r:?} in the band of {pr:?}");
        }
        // the run, its steps by number, each with its template and its calls
        let (k, label, note) = says("run:O-1099511631050");
        assert_eq!((k, label.as_str()), (Kind::Run, "handled by owner_post"));
        assert!(note.starts_with("2 steps · "), "{note}");
        assert_eq!(ids(&g, at("run:O-1099511631050")), [s1, s2]);
        assert_eq!(says(s1), (Kind::Step, "his posts held together for a moment before any agent is given them".into(), "step 1 · gather.call · no call on the wire".into()));
        assert_eq!(ids(&g, at(s1)), ["O-1099511631050.s1/call:gather"]);
        assert_eq!(says("O-1099511631050.s1/call:gather"), (Kind::Text, "its prompt template, gather.call".into(), "the call file it is formed from".into()));
        assert_eq!(says(s2), (Kind::Step, "route.call".into(), "step 2 · route.call · gpt-6.2-sol".into()));
        assert_eq!(ids(&g, at(s2)), ["O-1099511631050.s2/call:route", "wire:O-1099511631050.s2@gpt-6.2-sol"]);
        let (k, label, _) = says("wire:O-1099511631050.s2@gpt-6.2-sol");
        assert_eq!((k, label.as_str()), (Kind::Call, "on the wire: gpt-6.2-sol"));
        let sr = g.node(at(s2)).rect;
        assert!(g.children(at(s2)).iter().all(|c| g.node(*c).rect.y >= sr.y + sr.h * WORDS), "in the step's lower part");
        // the requests, by session, whose and how many, the latest's prompt; each session's texts, carried first, the
        // largest first, then what they ran with
        let (k, label, note) = says("requests:N-1.p3");
        assert_eq!((k, label.as_str()), (Kind::Post, "3 requests carried it: proto-mind, accounts-lead"));
        assert!(note.starts_with("2 sessions · the last "), "{note}");
        let (proto, acc) = ("requests:N-1.p3/session:s-proto", "requests:N-1.p3/session:s-acc");
        assert_eq!(ids(&g, at("requests:N-1.p3")), [proto, acc]);
        let (k, label, note) = says(proto);
        assert_eq!((k, label.as_str()), (Kind::Call, "proto-mind: 2 requests, claude-opus-5-5 xhigh"));
        assert!(note.starts_with("132k prompt tokens · the last "), "{note}");
        let texts: Vec<(String, String)> = g.children(at(proto)).iter().map(|c| (g.node(*c).label.clone(), g.node(*c).note.clone())).collect();
        let t = |a: &str, b: &str| (a.to_string(), b.to_string());
        // each says how many of his post's requests had it, of how many sessions, each request once (p1821, H3): the
        // 57-byte block in both sessions
        assert_eq!(texts, [t("22 tools, 31622 bytes", "carried by 1 request of 1 session"), t("a system prompt block, 9449 bytes", "carried by 2 requests of 1 session"), t("a system prompt block, 57 bytes", "carried by 3 requests of 2 sessions"), t("AutoMem /m/MEMORY.md, 9476 bytes", "ran with 1 request of 1 session")]);
        assert!(g.children(at(proto)).iter().all(|c| g.node(*c).kind == Kind::Text));
        // a block's card stands for its text, which his press opens apart; the tools, a file the requests ran with
        // (p1889, 1) and a prompt template do not
        let blocks: Vec<Option<&str>> = g.children(at(proto)).iter().map(|c| block_of(g.node(*c))).collect();
        assert_eq!(blocks, [None, Some("text:b"), Some("text:a"), None]);
        assert_eq!(block_of(g.node(at("O-1099511631050.s1/call:gather"))), None);
        assert_eq!(says(acc).1, "accounts-lead: 1 request, claude-opus-5-5 xhigh");
        assert_eq!(g.children(at(acc)).len(), 2);
        // each opens at its side on screen, its cards then drawn in it
        assert!(["run:O-1099511631050", s1, s2, proto, acc].iter().all(|id| g.node(at(id)).opens.is_some()), "each opens");
    }

    /// H4 (paper-research N-1446 p1821, p1844): whose a kind or a call is, in the editor's words, by where it is
    /// written: an agent's own, the project's library, the project's own, else agent-base's; nothing from a coordd
    /// that sends no places. A handler is found by its `when`, not by a comment naming it or a longer event.
    #[test]
    fn a_program_or_a_call_says_whose_it_is() {
        // agent-base's files where a release's coordd reads them, not under the checkout the window names (`s.base`)
        let (_held, mut s) = crate::view_cost::test_app();
        s.base = "/checkout/agent-base".into();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        assert_eq!(whose_words(&s, &p, "/ab/kinds/base.kind", None), None, "no places, nothing said");
        p.live.places = Some(coord_types::Places { coord: "/c".into(), definition: Some("/f/projects/research".into()), library: Some("/lib/tovrin".into()), ..Default::default() });
        let w = |f: &str, by: Option<&str>| whose_words(&s, &p, f, by);
        assert_eq!(w("/ab/kinds/base.kind", None).as_deref(), Some("agent-base's"));
        assert_eq!(w("/ab/calls/gather.call", None).as_deref(), Some("agent-base's"));
        assert_eq!(w("/f/projects/research/kinds/words.kind", None).as_deref(), Some("research's"));
        assert_eq!(w("/lib/tovrin/kinds/part.kind", None).as_deref(), Some("the library's"));
        assert_eq!(w("/f/projects/research/kinds/agents/proto-mind.kind", Some("proto-mind")).as_deref(), Some("proto-mind's own"));
        assert_eq!(w("/f/projects/research2/kinds/x.kind", None).as_deref(), Some("agent-base's"), "a longer folder's name is not under it");
        assert!(writes_when("kind base\n  when owner_post(p) do\n", "owner_post"));
        assert!(!writes_when("// - owner_post: the owner's words\n  when owner_posted(p) do\n", "owner_post"));
    }

    /// A band's cells: one row while they keep a shape at least half again as wide as tall, else a grid; all inside the
    /// band; and a small card deep in the focus keeps room for what it holds ([`gap_in`]).
    #[test]
    fn answers_take_one_row_then_a_grid_inside_the_band() {
        let r = Rect::new(0.0, 0.0, CARD[0], CARD[1]);
        let one = band_cells(r, 2);
        assert!(one.iter().all(|c| (c.y - one[0].y).abs() < 1e-9), "one row of two: {one:?}");
        let many = band_cells(r, 18);
        let rows: BTreeSet<i64> = many.iter().map(|c| (c.y * 1000.0) as i64).collect();
        assert!(rows.len() > 1, "eighteen take rows");
        for c in many.iter().chain(&one) {
            assert!(r.holds(c) && c.y >= r.h * WORDS, "{c:?}");
        }
        // inside a bundle: below its head, each at least half again as wide as tall, in a row where that keeps it so
        // (in the band's cell of CARD's card, 153 by 80 with the band three quarters of it, two answers one over the
        // other, 148 by 26: paper-research N-1446 p1796, 1b), else a grid
        let b = one[0];
        let two = cells_in(b, 2);
        assert!(two.iter().all(|c| b.holds(c) && c.y >= b.y + b.h * BUNDLE_HEAD && c.w >= 1.5 * c.h), "{two:?} in {b:?}");
        let wide = Rect::new(0.0, 0.0, 300.0, 60.0);
        let row = cells_in(wide, 2);
        assert!((row[0].y - row[1].y).abs() < 1e-9 && row.iter().all(|c| wide.holds(c) && c.w >= 1.5 * c.h), "{row:?} in {wide:?}");
        // still 3 of p815: a session about 13 by 4 holds its five texts, each a fifth of it across and high; a step
        // about 60 by 4 its prompt template, a third of it high (GAP's tenth and fifth left them 0.33 wide and no height)
        let session = Rect::new(0.0, 0.0, 13.3, 3.9);
        let texts = cells_in(session, 5);
        assert!(texts.iter().all(|c| session.holds(c) && c.w >= session.w / 5.0 && c.h >= session.h / 5.0), "{texts:?} in {session:?}");
        let step = Rect::new(0.0, 0.0, 60.5, 3.9);
        let template = band_cells(step, 1);
        assert!(step.holds(&template[0]) && template[0].h >= step.h / 3.0, "{template:?} in {step:?}");
    }

    /// Each link drawn over the deepest card that holds both its ends (canvas.rs `board`; paper-research N-1446 p1677):
    /// his links between threads over the project's, one between two of his posts in a thread over that thread's; with
    /// N-3 a card, its posts not shown, its links still over the project's, between the two cards; with N-1 a card, the
    /// link inside it gone.
    #[test]
    fn each_link_is_drawn_over_the_card_that_holds_both_its_ends() {
        use super::super::lod::{Look, Shown, edges};
        let (mut g, _) = cloud(&base(), None, "project:research", "research", &mut BTreeMap::new()).unwrap();
        let p = g.project("project:research").unwrap();
        let (p1, p5) = (g.find_in(p, "N-1.p1").unwrap(), g.find_in(p, "N-1.p5").unwrap());
        g.link(Edge::new(p1, p5, Link::Names)).unwrap();
        // shown as lod.rs `visible_with` lists them, parents before children; all but what a card in CLOSED holds
        let shown = |closed: &[&str]| -> Vec<Shown> {
            let mut out = Vec::new();
            let mut stack = vec![p];
            while let Some(i) = stack.pop() {
                out.push(Shown { i, r: [0.0; 4], look: Look::Open, alpha: 1.0, open: 1.0, lit: false, many: 1, group: None });
                if !closed.contains(&g.node(i).id.as_str()) {
                    stack.extend(g.children(i).iter().rev());
                }
            }
            out
        };
        let over = |closed: &[&str]| -> BTreeSet<(String, String, String)> {
            let s = shown(closed);
            let d = edges(&g, &s, &HashMap::new(), None);
            let id = |k: usize| g.node(s[k].i).id.clone();
            d.iter().zip(holders(&g, &s, &d)).filter(|(e, _)| e.link == Link::Names).map(|(e, h)| (id(e.from).min(id(e.to)), id(e.from).max(id(e.to)), h.map_or("none".to_string(), id))).collect()
        };
        let t = |a: &str, b: &str, h: &str| (a.to_string(), b.to_string(), h.to_string());
        let project = "project:research";
        assert_eq!(
            over(&[]),
            [t("N-1.p5", "N-3.p2", project), t("N-1.p7", "N-3", project), t("N-2.p4", "N-3.p2", project), t("N-1.p1", "N-1.p5", "N-1")].into_iter().collect()
        );
        assert_eq!(over(&["N-3"]), [t("N-1.p5", "N-3", project), t("N-1.p7", "N-3", project), t("N-2.p4", "N-3", project), t("N-1.p1", "N-1.p5", "N-1")].into_iter().collect());
        assert_eq!(over(&["N-1", "N-3"]), [t("N-1", "N-3", project), t("N-2.p4", "N-3", project)].into_iter().collect());
    }
}
