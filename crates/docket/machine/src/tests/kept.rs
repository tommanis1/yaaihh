//! The graph's kept index (paper-research N-1446 p1828, (b)): each kept read equals its whole pass (`*_walked`) row for
//! row. The graph grows one edge at a time in a made-up order: a node before its edges, after them or never, some edges
//! with no time and some to the half second, a request's `for` taken out and made again as the sessions' side does it
//! (`Graph::retain_edges`), any other edge taken out (the index made again), a request's tokens coming later, the graph
//! cloned on the way. Each read is checked with no moment and at each moment a time of the graph names.

use std::collections::{BTreeMap, BTreeSet};

use crate::graph::{AskTimes, Course, Edge, EdgeKind, Graph, Node, NodeKind, Reached, Source, Tokens};
use crate::graph_slice::{Ask, Layer};
use crate::line::At;

/// A small seeded stream of numbers (an LCG, no crate): the same graph for the same seed.
struct Seq(u64);

impl Seq {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len())]
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

/// The graph's times: whole seconds and fractions of one, as the record's lines and the sessions' write them.
const TIMES: [&str; 9] = [
    "2026-11-23T10:00:00Z",
    "2026-11-23T10:00:01.500Z",
    "2026-11-23T10:00:01Z",
    "2026-11-23T10:00:02Z",
    "2026-11-23T10:00:05.250Z",
    "2026-11-23T10:00:05Z",
    "2026-11-23T10:01:00Z",
    "2026-11-23T10:02:00Z",
    "2026-11-23T10:03:00Z",
];

const OWNER: &str = "agent:owner";
const AGENTS: [&str; 4] = ["agent:a1", "agent:a2", "agent:a3", "agent:far/x"];
const POSTS: [&str; 9] = ["N-1.p1", "N-1.p2", "N-1.p3", "N-1.p4", "N-1.p5", "N-2.p1", "N-2.p2", "N-2.p3", "N-2.p4"];
const ITEMS: [&str; 2] = ["I-7", "I-8"];
const COMMITS: [&str; 4] = ["commit:c1", "commit:c2", "commit:c3", "commit:c4"];
const BRANCHES: [&str; 2] = ["branch:b1", "branch:b2"];
const JOBS: [&str; 3] = ["job:j1", "job:j2", "job:j3"];
const BUILDS: [&str; 2] = ["build:B1", "build:B2"];
const SESSIONS: [&str; 2] = ["session:s1", "session:s2"];
const REQUESTS: [&str; 6] = ["request:r1", "request:r2", "request:r3", "request:r4", "request:r5", "request:r6"];
const TURNS: [&str; 2] = ["turn:t1", "turn:t2"];
const STARTS: [&str; 2] = ["start:x1", "start:x2"];

/// A node's kind by its id's form.
fn kind_of(id: &str) -> NodeKind {
    match id.split_once(':').map_or(id, |(k, _)| k) {
        "agent" => NodeKind::Agent,
        "commit" => NodeKind::Commit,
        "branch" => NodeKind::Branch,
        "job" => NodeKind::Job,
        "build" => NodeKind::Build,
        "session" => NodeKind::Session,
        "request" => NodeKind::Request,
        "turn" => NodeKind::Turn,
        "start" => NodeKind::Start,
        "file" => NodeKind::File,
        _ if id.contains(".p") => NodeKind::Post,
        _ => NodeKind::Ticket,
    }
}

/// What the reads found over every seed and step: each kind of row the leg must have reached at least once.
#[derive(Default, Debug)]
struct Found {
    asks: usize,
    handed: usize,
    taken: usize,
    closed: usize,
    in_window: usize,
    signed: usize,
    commits: usize,
    costs: usize,
    named: usize,
    after: usize,
    sessions: usize,
    reds: usize,
    removed_for: usize,
    removed_other: usize,
}

/// Each kept read of G equals its whole pass, at every moment; what they found counted in SEEN.
fn same_reads(g: &Graph, step: &str, seen: &mut Found) {
    let moments: Vec<Option<&str>> = std::iter::once(None).chain(TIMES.iter().map(|t| Some(*t))).collect();
    for u in &moments {
        assert_eq!(g.ask_times_at(*u), g.ask_times_walked(*u), "{step}: ask_times_at {u:?}");
        assert_eq!(g.asks_now(*u), g.asks_now_walked(*u), "{step}: asks_now {u:?}");
    }
    for at in TIMES {
        let now = g.now(at);
        assert_eq!(now, g.now_walked(at), "{step}: now {at}");
        seen.sessions += now.agents.iter().filter(|a| a.session.is_some()).count();
        seen.reds += now.reds.len();
    }
    let costs = g.costs_at(None);
    assert_eq!(costs, g.costs_walked(None), "{step}: costs_at");
    seen.costs += costs.iter().filter(|c| !c.handed.is_empty()).count();
    seen.named += costs.iter().filter(|c| !c.named.is_empty()).count();
    let asks = g.ask_times_at(None);
    let all: Vec<&AskTimes> = asks.iter().collect();
    let after = g.his_after(&all);
    assert_eq!(after, g.his_after_walked(&all), "{step}: his_after");
    seen.after += after.iter().filter(|a| !a.is_empty()).count();
    seen.asks += asks.len();
    seen.handed += asks.iter().filter(|a| a.reached.is_some()).count();
    seen.taken += asks.iter().filter(|a| a.taken.is_some()).count();
    seen.closed += asks.iter().filter(|a| a.closed.is_some()).count();
    seen.in_window += asks.iter().filter(|a| a.in_window.is_some()).count();
    seen.signed += asks.iter().filter(|a| a.signed.is_some()).count();
    seen.commits += asks.iter().filter(|a| a.first_commit.is_some()).count();
}

/// One made-up edge into G, its ends' nodes made now, later (PENDING) or never.
fn grow(g: &mut Graph, s: &mut Seq, pending: &mut Vec<String>) {
    let time = |s: &mut Seq| if s.chance(12) { None } else { Some(TIMES[s.below(TIMES.len())].to_string()) };
    let line = |s: &mut Seq| Source::Line(1 + s.below(6));
    let session = |s: &mut Seq| Source::Session(["s1", "s2"][s.below(2)].to_string(), 1 + s.below(8));
    let posts_items: Vec<&str> = POSTS.iter().chain(&ITEMS).copied().collect();
    let posts_commits: Vec<&str> = POSTS.iter().chain(&COMMITS).copied().collect();
    let (kind, from, to, source, what): (EdgeKind, &str, &str, Source, &str) = match s.below(40) {
        0..=3 => (EdgeKind::Wrote, OWNER, s.pick(&POSTS), line(s), ""),
        4 | 5 => (EdgeKind::Wrote, s.pick(&AGENTS), s.pick(&POSTS), line(s), ""),
        6 => (EdgeKind::Wrote, s.pick(&AGENTS), s.pick(&COMMITS), line(s), ""),
        7 => (EdgeKind::Wrote, s.pick(&AGENTS), s.pick(&ITEMS), line(s), ""),
        8 | 9 => (EdgeKind::Asks, s.pick(&POSTS), s.pick(&AGENTS), line(s), ""),
        10 => (EdgeKind::Woke, s.pick(&POSTS), s.pick(&AGENTS), line(s), ""),
        11..=13 => (EdgeKind::Handed, s.pick(&POSTS), if s.chance(10) { OWNER } else { s.pick(&AGENTS) }, line(s), ""),
        14 => (EdgeKind::Answered, s.pick(&POSTS), s.pick(&POSTS), line(s), ""),
        15 => (EdgeKind::AnsweredUpto, s.pick(&POSTS), s.pick(&POSTS), line(s), ""),
        16 => (EdgeKind::Named, s.pick(&POSTS), s.pick(&POSTS), line(s), ""),
        17 => (EdgeKind::Named, s.pick(&JOBS), s.pick(&posts_items), line(s), ""),
        18 => (EdgeKind::CameFrom, s.pick(&ITEMS), s.pick(&POSTS), line(s), ""),
        19 => (EdgeKind::Started, s.pick(&AGENTS), s.pick(&ITEMS), line(s), ""),
        20 => (EdgeKind::Closed, s.pick(&ITEMS), s.pick(&AGENTS), line(s), ""),
        21 => (EdgeKind::ClosedBy, s.pick(&POSTS), s.pick(&posts_items), line(s), ""),
        22 => (EdgeKind::SignedOff, OWNER, s.pick(&POSTS), line(s), ["worth_it", "fine", "none"][s.below(3)]),
        23 => (EdgeKind::Named, s.pick(&posts_commits), s.pick(&posts_commits), line(s), ""),
        24 => (EdgeKind::MergedInto, s.pick(&COMMITS), s.pick(&COMMITS), line(s), ""),
        25 => (EdgeKind::Ended, s.pick(&JOBS), s.pick(&COMMITS), line(s), ""),
        26 => (EdgeKind::Ran, s.pick(&AGENTS), s.pick(&JOBS), line(s), ""),
        27 => (EdgeKind::Ran, s.pick(&JOBS), s.pick(&COMMITS), line(s), ""),
        28 => (EdgeKind::BuiltInto, s.pick(&COMMITS), s.pick(&BUILDS), line(s), ""),
        29 => (EdgeKind::Installed, s.pick(&BUILDS), OWNER, line(s), ""),
        30 => (EdgeKind::On, s.pick(&COMMITS), s.pick(&BRANCHES), line(s), ""),
        31 => (EdgeKind::TurnOf, s.pick(&TURNS), s.pick(&AGENTS), line(s), ""),
        32 => (EdgeKind::Prompted, s.pick(&POSTS), s.pick(&AGENTS), line(s), ""),
        33 => (EdgeKind::StartOf, s.pick(&STARTS), s.pick(&AGENTS), line(s), ""),
        34 => (EdgeKind::CameFrom, s.pick(&STARTS), s.pick(&SESSIONS), line(s), ""),
        35 => (EdgeKind::SessionOf, s.pick(&SESSIONS), s.pick(&AGENTS), line(s), ""),
        36 => (EdgeKind::Sent, s.pick(&SESSIONS), s.pick(&REQUESTS), session(s), ""),
        37 | 38 => (EdgeKind::For, s.pick(&REQUESTS), s.pick(&POSTS), session(s), ""),
        _ => (EdgeKind::Edited, s.pick(&AGENTS), "file:/w/a.rs", line(s), ""),
    };
    for id in [from, to] {
        if g.get(id).is_some() || pending.iter().any(|p| p == id) {
            continue;
        }
        match s.below(10) {
            0..=6 => node(g, s, id),
            7 | 8 => pending.push(id.to_string()),
            _ => {}
        }
    }
    let when = time(s);
    g.edge_from(from, to, kind, source, when);
    g.edges.last_mut().unwrap().what = what.to_string();
}

/// The node ID, of its kind, with a time, a job's end, a request's model and tokens.
fn node(g: &mut Graph, s: &mut Seq, id: &str) {
    let when = if s.chance(30) { None } else { Some(TIMES[s.below(TIMES.len())].to_string()) };
    g.node_from(id, kind_of(id), Source::Line(1 + s.below(6)), when);
    let n = g.get_mut(id).unwrap();
    match n.kind {
        NodeKind::Job => n.ok = [Some(true), Some(false), None][s.below(3)],
        NodeKind::Request => {
            n.what = ["m xhigh", "m low"][s.below(2)].to_string();
            n.tokens = s.chance(60).then(|| Tokens { input: 3, output: 1 + s.below(50) as u64, cache_read: 900, cache_write: 60, reasoning: 0 });
        }
        _ => {}
    }
}

/// Each of a request's `for`s taken out and made again from the same lines, as the sessions' side makes them again
/// when a step began at one of its requests (follow.rs); some go to another post.
fn for_again(g: &mut Graph, s: &mut Seq) -> bool {
    let r = s.pick(&REQUESTS);
    let old: Vec<(String, Source, Option<String>)> = g.edges.iter().filter(|e| e.kind == EdgeKind::For && e.from == r).map(|e| (e.to.clone(), e.source.clone(), e.when.clone())).collect();
    if old.is_empty() {
        return false;
    }
    g.retain_edges(|e| !(e.kind == EdgeKind::For && e.from == r));
    for (to, source, when) in old {
        let to = if s.chance(30) { s.pick(&POSTS).to_string() } else { to };
        if s.chance(80) {
            g.edge_from(r, &to, EdgeKind::For, source, when);
        }
    }
    true
}

/// Every kept read equals its whole pass after each edge in, over many made-up graphs (paper-research N-1446 p1828:
/// each kept twin equals the whole pass row for row), and the graphs reach every kind of row the reads give.
#[test]
fn each_kept_read_is_its_whole_pass_as_the_graph_grows() {
    let mut seen = Found::default();
    for seed in 0..14u64 {
        let mut s = Seq(seed.wrapping_mul(0x9E3779B97F4A7C15) ^ 0x5DEECE66D);
        let mut g = Graph::default();
        let mut pending: Vec<String> = Vec::new();
        for step in 0..170 {
            grow(&mut g, &mut s, &mut pending);
            if !pending.is_empty() && s.chance(15) {
                let id = pending.remove(s.below(pending.len()));
                node(&mut g, &mut s, &id);
            }
            if s.chance(5) && for_again(&mut g, &mut s) {
                seen.removed_for += 1;
            }
            if s.chance(2) && !g.edges.is_empty() {
                // an edge of another kind taken out: the index is made again
                let i = s.below(g.edges.len());
                if g.edges[i].kind != EdgeKind::For {
                    let mut k = 0;
                    g.retain_edges(|_| {
                        k += 1;
                        k - 1 != i
                    });
                    seen.removed_other += 1;
                }
            }
            if s.chance(4) {
                // a request's tokens, come with its response's later line
                let r = s.pick(&REQUESTS);
                if let Some(n) = g.get_mut(r) {
                    n.tokens = Some(Tokens { input: 1, output: 7, cache_read: 0, cache_write: 0, reasoning: 0 });
                }
            }
            if s.chance(3) {
                g = g.clone();
            }
            // each step read for the first seeds, every third after, so a catch-up takes several edges at once too
            if seed < 3 || step % 3 == 0 {
                same_reads(&g, &format!("seed {seed} step {step}"), &mut seen);
            }
        }
        same_reads(&g, &format!("seed {seed} at its end"), &mut seen);
        // a graph read back (its kept index made from nothing) reads as the kept one did
        let back: Graph = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert_eq!(back.ask_times_at(None), g.ask_times_at(None), "seed {seed}: read back");
        assert_eq!(back.costs_at(None), g.costs_at(None), "seed {seed}: read back, costs");
    }
    eprintln!("kept reads reached: {seen:?}");
    let Found { asks, handed, taken, closed, in_window, signed, commits, costs, named, after, sessions, reds, removed_for, removed_other } = seen;
    for (what, n) in [("asks", asks), ("handed", handed), ("taken", taken), ("closed", closed), ("in his window", in_window), ("signed", signed), ("commits", commits), ("costs", costs), ("named costs", named), ("his posts after", after), ("sessions' last lines", sessions), ("reds", reds), ("fors made again", removed_for), ("other edges taken out", removed_other)] {
        assert!(n > 0, "no {what} reached: the made-up graphs miss a part of the reads");
    }
}

/// The asks each slice leg reads a graph with: every layer, the whole graph cut by a limit, each layer alone, his with
/// a writer, by who, by item, in a span, and around a post and around his node.
fn slice_asks() -> Vec<Ask> {
    let mut asks = vec![Ask::default(), Ask { limit: 5, ..Ask::default() }];
    for l in Layer::ALL {
        asks.push(Ask { layers: BTreeSet::from([l]), limit: 40_000, ..Ask::default() });
    }
    asks.push(Ask { layers: BTreeSet::from([Layer::His]), who: vec!["a1".into()], ..Ask::default() });
    asks.push(Ask { who: vec!["owner".into()], ..Ask::default() });
    asks.push(Ask { items: vec!["I-7".into()], ..Ask::default() });
    asks.push(Ask { from: Some(TIMES[2].into()), to: Some(TIMES[6].into()), ..Ask::default() });
    asks.push(Ask { around: Some("N-1.p1".into()), hops: 2, layers: BTreeSet::from([Layer::His, Layer::Agents]), ..Ask::default() });
    asks.push(Ask { around: Some(OWNER.into()), hops: 1, ..Ask::default() });
    asks
}

/// What the slice legs reached over every seed and step: his edges, writers given to nodes, a post of his written
/// again by an agent, a node come after its edges, a `for` made again, another edge taken out.
#[derive(Default, Debug)]
struct Slices {
    his: usize,
    by: usize,
    unhis: usize,
    late: usize,
    removed_for: usize,
    removed_other: usize,
}

/// Each slice of G read off the kept layers equals its whole pass's; what they found counted in SEEN.
fn same_slices(g: &Graph, step: &str, seen: &mut Slices) {
    for (k, ask) in slice_asks().iter().enumerate() {
        let s = g.slice(ask);
        assert_eq!(s, g.slice_walked(ask), "{step}: slice {k} {ask:?}");
        seen.his += s.edges.iter().filter(|e| e.layer == Layer::His).count();
        seen.by += s.nodes.iter().filter(|n| n.by.is_some()).count();
    }
}

/// An edge that gives a node a writer as the slice reads it (`wrote`), or looks as if it does: a post or an item
/// written again by him or an agent, a commit or a file he wrote, a session or a slice of one he or an agent took, a
/// turn of his or an agent's, a post naming a turn and a slice answering a post (so their writers decide a layer), and
/// a `wrote` whose writer end is no agent's.
fn writer_edge(g: &mut Graph, s: &mut Seq, pending: &mut Vec<String>, seen: &mut Slices) {
    let who = |s: &mut Seq| if s.chance(50) { OWNER } else { s.pick(&AGENTS) };
    let posts_items: Vec<&str> = POSTS.iter().chain(&ITEMS).copied().collect();
    let (kind, from, to): (EdgeKind, &str, &str) = match s.below(9) {
        0..=2 => (EdgeKind::Wrote, who(s), s.pick(&posts_items)),
        3 => (EdgeKind::Wrote, OWNER, if s.chance(50) { s.pick(&COMMITS) } else { "file:/w/b.rs" }),
        4 => (EdgeKind::Took, who(s), if s.chance(50) { s.pick(&SESSIONS) } else { "slice:s1.4" }),
        5 => (EdgeKind::TurnOf, s.pick(&TURNS), who(s)),
        6 => (EdgeKind::Named, s.pick(&POSTS), s.pick(&TURNS)),
        7 => (EdgeKind::Answered, "slice:s1.4", s.pick(&POSTS)),
        _ => (EdgeKind::Wrote, s.pick(&JOBS), s.pick(&POSTS)),
    };
    // a post of his (his the last writer edge at it) written again by an agent
    let last = g
        .edges
        .iter()
        .rev()
        .filter_map(|e| match e.kind {
            EdgeKind::TurnOf => Some((e.from.as_str(), e.to.as_str())),
            EdgeKind::Wrote | EdgeKind::Took => Some((e.to.as_str(), e.from.as_str())),
            _ => None,
        })
        .find(|(of, a)| *of == to && a.starts_with("agent:"))
        .map(|(_, a)| a == OWNER);
    if kind == EdgeKind::Wrote && last == Some(true) && from != OWNER && from.starts_with("agent:") {
        seen.unhis += 1;
    }
    for id in [from, to] {
        if g.get(id).is_none() && !pending.iter().any(|p| p == id) {
            if s.chance(70) {
                node(g, s, id);
            } else {
                pending.push(id.to_string());
            }
        }
    }
    let when = if s.chance(12) { None } else { Some(TIMES[s.below(TIMES.len())].to_string()) };
    g.edge_from(from, to, kind, Source::Line(1 + s.below(6)), when);
}

/// Each slice read off the kept index (each edge's layer, each layer's count and each node's writer, docket-edge5
/// N-1446 p2532) equals its whole pass's after each edge in, over made-up graphs where a post of his is written again
/// by an agent and back, nodes come after their edges (a commit's turning its edges to work), a `took` and a `turn_of`
/// give writers, a writer end that is no agent's gives none, a `for` is made again and another edge taken out, and the
/// graph is cloned and read back.
#[test]
fn each_slice_off_the_kept_layers_is_its_whole_pass_as_the_graph_grows() {
    let mut seen = Slices::default();
    for seed in 0..12u64 {
        let mut s = Seq(seed.wrapping_mul(0xD1B5_4A32_D192_ED03) ^ 0x2545_F491_4F6C_DD1D);
        let mut g = Graph::default();
        let mut pending: Vec<String> = Vec::new();
        for step in 0..150 {
            grow(&mut g, &mut s, &mut pending);
            if s.chance(30) {
                writer_edge(&mut g, &mut s, &mut pending, &mut seen);
            }
            if !pending.is_empty() && s.chance(15) {
                let id = pending.remove(s.below(pending.len()));
                if g.edges.iter().any(|e| e.from == id || e.to == id) {
                    seen.late += 1;
                }
                node(&mut g, &mut s, &id);
            }
            if s.chance(5) && for_again(&mut g, &mut s) {
                seen.removed_for += 1;
            }
            if s.chance(2) && !g.edges.is_empty() {
                // an edge of another kind taken out: the index is made again
                let i = s.below(g.edges.len());
                if g.edges[i].kind != EdgeKind::For {
                    let mut k = 0;
                    g.retain_edges(|_| {
                        k += 1;
                        k - 1 != i
                    });
                    seen.removed_other += 1;
                }
            }
            if s.chance(3) {
                g = g.clone();
            }
            if seed < 3 || step % 3 == 0 {
                same_slices(&g, &format!("seed {seed} step {step}"), &mut seen);
            }
        }
        same_slices(&g, &format!("seed {seed} at its end"), &mut seen);
        let back: Graph = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        for ask in slice_asks() {
            assert_eq!(back.slice(&ask), g.slice(&ask), "seed {seed}: read back, {ask:?}");
        }
    }
    eprintln!("slices reached: {seen:?}");
    let Slices { his, by, unhis, late, removed_for, removed_other } = seen;
    for (what, n) in [("his edges", his), ("writers", by), ("posts of his written again by an agent", unhis), ("nodes after their edges", late), ("fors made again", removed_for), ("other edges taken out", removed_other)] {
        assert!(n > 0, "no {what} reached: the made-up graphs miss a part of the slices");
    }
}

/// A post's edges leave his layer when an agent writes it after him and come back when he writes it again; an edge at a
/// node that comes later reads its kind once it comes (a commit's: work); a `for` taken out leaves the others' layers
/// where they were. Each step's kept slice is its whole pass's.
#[test]
fn a_post_written_again_by_an_agent_leaves_his_layer_and_comes_back() {
    let all = Ask { limit: 40_000, ..Ask::default() };
    let layer_of = |g: &Graph, kind: EdgeKind, from: &str, line: usize| {
        let s = g.slice(&all);
        assert_eq!(s, g.slice_walked(&all), "the kept slice is its whole pass's");
        s.edges.iter().find(|e| e.edge.kind == kind && e.edge.from == from && e.edge.source == Source::Line(line)).map(|e| e.layer)
    };
    let mut g = Graph::default();
    g.node_from(OWNER, NodeKind::Agent, Source::Line(1), None);
    g.node_from("agent:a1", NodeKind::Agent, Source::Line(1), None);
    // its thread's edge before its node and before any writer: the agents'
    g.edge_from("N-1.p1", "N-1", EdgeKind::PostedIn, Source::Line(1), None);
    assert_eq!(layer_of(&g, EdgeKind::PostedIn, "N-1.p1", 1), Some(Layer::Agents));
    g.node_from("N-1.p1", NodeKind::Post, Source::Line(1), None);
    g.edge_from(OWNER, "N-1.p1", EdgeKind::Wrote, Source::Line(2), None);
    assert_eq!(layer_of(&g, EdgeKind::PostedIn, "N-1.p1", 1), Some(Layer::His), "his post: its edges are his");
    g.edge_from("agent:a1", "N-1.p1", EdgeKind::Wrote, Source::Line(3), None);
    assert_eq!(layer_of(&g, EdgeKind::PostedIn, "N-1.p1", 1), Some(Layer::Agents), "an agent wrote it after him");
    assert_eq!(layer_of(&g, EdgeKind::Wrote, OWNER, 2), Some(Layer::His), "his own wrote stays his");
    assert_eq!(g.slice(&all).nodes.iter().find(|n| n.node.id == "N-1.p1").and_then(|n| n.by.clone()), Some("a1".to_string()));
    g.edge_from("N-1.p1", "commit:c1", EdgeKind::Named, Source::Line(4), None);
    assert_eq!(layer_of(&g, EdgeKind::Named, "N-1.p1", 4), Some(Layer::Agents));
    g.node_from("commit:c1", NodeKind::Commit, Source::Line(4), None);
    assert_eq!(layer_of(&g, EdgeKind::Named, "N-1.p1", 4), Some(Layer::Work), "its commit came: work");
    g.edge_from(OWNER, "N-1.p1", EdgeKind::Wrote, Source::Line(5), None);
    assert_eq!(layer_of(&g, EdgeKind::PostedIn, "N-1.p1", 1), Some(Layer::His), "he wrote it again");
    g.edge_from("request:r1", "N-1.p1", EdgeKind::For, Source::Session("s1".into(), 1), None);
    g.edge_from("N-1.p1", "agent:a1", EdgeKind::Asks, Source::Line(6), None);
    g.retain_edges(|e| e.kind != EdgeKind::For);
    assert_eq!(layer_of(&g, EdgeKind::Asks, "N-1.p1", 6), Some(Layer::His), "a for taken out moves the layers after it down");
    assert_eq!(g.slice(&all).whole, BTreeMap::from([(Layer::His, 5), (Layer::Work, 1)]));
}

/// A taken-out `for` moves the index along without making it again, and an edge added after reads as from nothing: the
/// kept read after `retain_edges` is the read of a graph with the same edges whose index was never kept.
#[test]
fn a_for_made_again_moves_the_kept_index_along() {
    let mut g = Graph::default();
    let at = |t: &str| Some(t.to_string());
    for (id, kind) in [(OWNER, NodeKind::Agent), ("agent:a1", NodeKind::Agent), ("N-1.p1", NodeKind::Post), ("N-1.p2", NodeKind::Post), ("session:s1", NodeKind::Session), ("request:r1", NodeKind::Request), ("request:r2", NodeKind::Request)] {
        g.node_from(id, kind, Source::Line(1), at("2026-11-23T10:00:00Z"));
    }
    g.edge_from(OWNER, "N-1.p1", EdgeKind::Wrote, Source::Line(1), at("2026-11-23T10:00:00Z"));
    g.edge_from("N-1.p1", "agent:a1", EdgeKind::Asks, Source::Line(1), at("2026-11-23T10:00:00Z"));
    g.edge_from(OWNER, "N-1.p2", EdgeKind::Wrote, Source::Line(2), at("2026-11-23T10:00:01Z"));
    g.edge_from("N-1.p2", "agent:a1", EdgeKind::Asks, Source::Line(2), at("2026-11-23T10:00:01Z"));
    g.edge_from("N-1.p1", "agent:a1", EdgeKind::Handed, Source::Line(3), at("2026-11-23T10:00:02Z"));
    g.edge_from("N-1.p2", "agent:a1", EdgeKind::Handed, Source::Line(4), at("2026-11-23T10:00:03Z"));
    g.edge_from("session:s1", "agent:a1", EdgeKind::SessionOf, Source::Line(3), at("2026-11-23T10:00:02Z"));
    let s = |n| Source::Session("s1".into(), n);
    g.edge_from("session:s1", "request:r1", EdgeKind::Sent, s(5), at("2026-11-23T10:00:05Z"));
    g.edge_from("request:r1", "N-1.p1", EdgeKind::For, s(5), at("2026-11-23T10:00:05Z"));
    g.edge_from("session:s1", "request:r2", EdgeKind::Sent, s(6), at("2026-11-23T10:00:06Z"));
    g.edge_from("request:r2", "N-1.p1", EdgeKind::For, s(6), at("2026-11-23T10:00:06Z"));
    let before = g.costs_at(None);
    assert_eq!(before.iter().map(|c| (c.post.as_str(), c.handed.iter().map(|x| x.requests).sum::<usize>())).collect::<Vec<_>>(), vec![("N-1.p1", 2), ("N-1.p2", 0)]);
    // a step began at r2 with p2: r2's `for` made again, for p2
    g.retain_edges(|e| !(e.kind == EdgeKind::For && e.from == "request:r2"));
    g.edge_from("request:r2", "N-1.p2", EdgeKind::For, s(6), at("2026-11-23T10:00:06Z"));
    let after = g.costs_at(None);
    assert_eq!(after, g.costs_walked(None));
    assert_eq!(after.iter().map(|c| (c.post.as_str(), c.handed.iter().map(|x| x.requests).sum::<usize>())).collect::<Vec<_>>(), vec![("N-1.p1", 1), ("N-1.p2", 1)]);
    assert_eq!(g.ask_times_at(None), g.ask_times_walked(None));
    let back: Graph = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
    assert_eq!(back.costs_at(None), after, "the index made from nothing reads the same");
}

/// A ticket's first answers kept (`Kept::answered_least`) are dropped when an answering post's node comes: an answer by
/// a post whose node comes after it counts once it comes, as a post's kind decides (paper-research N-1446 p2072 2).
#[test]
fn an_answer_by_a_post_whose_node_comes_later_counts_once_it_comes() {
    let mut g = Graph::default();
    g.node_from("agent:a1", NodeKind::Agent, Source::Line(1), None);
    g.node_from("N-1.p1", NodeKind::Post, Source::Line(1), None);
    g.edge_from("agent:a1", "N-2.p1", EdgeKind::Wrote, Source::Line(2), Some("2026-11-23T10:00:00Z".to_string()));
    g.edge_from("N-2.p1", "N-1.p1", EdgeKind::Answered, Source::Line(2), Some("2026-11-23T10:00:01Z".to_string()));
    let first = |g: &Graph| crate::graph_kept::KeptView::new(g, None).first_answered("N-1", 1).map(|r| (r.id, r.via));
    assert_eq!(first(&g), None, "no node, no answer");
    g.node_from("N-2.p1", NodeKind::Post, Source::Line(3), None);
    assert_eq!(first(&g), Some(("agent:a1".to_string(), Some("N-2.p1".to_string()))), "its node come, its answer counts");
}

/// A commit's first `on` kept (`Kept::first_on`) is the first of those at the least time, as `min_by` gives it in the
/// one pass, and none at a moment before that time (paper-research N-1446 p2072 2).
#[test]
fn a_commits_first_on_is_the_first_at_the_least_time() {
    let mut g = Graph::default();
    g.node_from("commit:c1", NodeKind::Commit, Source::Line(1), None);
    let at = |m: u32| Some(format!("2026-11-23T10:0{m}:00Z"));
    g.edge_from("commit:c1", "branch:b2", EdgeKind::On, Source::Line(2), at(2));
    g.edge_from("commit:c1", "branch:b1", EdgeKind::On, Source::Line(3), at(1));
    g.edge_from("commit:c1", "branch:b2", EdgeKind::On, Source::Line(4), at(1));
    g.edge_from("commit:c1", "branch:b2", EdgeKind::On, Source::Line(5), None);
    let on = |u: Option<&str>| crate::graph_kept::KeptView::new(&g, u).first_on("commit:c1").map(|e| e as *const Edge);
    let want = Some(&g.edges[1] as *const Edge);
    assert_eq!(on(None), want, "the first at the least time");
    assert_eq!(on(Some("2026-11-23T10:01:30Z")), want, "by a moment after it");
    assert_eq!(on(Some("2026-11-23T10:00:30Z")), None, "none by a moment before it");
}

/// The first answers kept at a moment (`Kept::answered_least_at`) move with the graph: `asks_now` at one moment and with
/// none, read after each edge in as the graph grows, is its whole pass each time, the moment's kept least read again
/// across catch-ups where [`each_kept_read_is_its_whole_pass_as_the_graph_grows`] reads each moment in turn
/// (paper-research N-1446 p2072 2).
#[test]
fn the_first_answers_kept_at_a_moment_move_with_the_graph() {
    for seed in 0..6u64 {
        let mut s = Seq(seed.wrapping_mul(0x9E3779B97F4A7C15) ^ 0x2545F4914F6CDD1D);
        let mut g = Graph::default();
        let mut pending: Vec<String> = Vec::new();
        let mut answered = 0;
        for step in 0..170 {
            grow(&mut g, &mut s, &mut pending);
            if !pending.is_empty() && s.chance(15) {
                let id = pending.remove(s.below(pending.len()));
                node(&mut g, &mut s, &id);
            }
            for u in [None, Some(TIMES[6])] {
                let kept = g.asks_now(u);
                assert_eq!(kept, g.asks_now_walked(u), "seed {seed} step {step}: asks_now {u:?}");
                // taken by an answer: by a post, not an item's start
                answered += kept.iter().filter(|a| a.times.taken.as_ref().is_some_and(|r| r.via.as_deref().is_some_and(|v| v.contains(".p")))).count();
            }
        }
        assert!(answered > 0, "seed {seed}: no ask taken by an answer");
    }
}

/// A read of two kinds or more at an id with more than `BIG` edges, whose kinds the index keeps apart, gives them in the
/// graph's order, as the one pass does, not each kind's after the other (`KeptView::edges_of`; paper-research N-1446
/// p2072 2).
#[test]
fn a_read_of_two_kinds_at_a_big_id_is_in_the_graphs_order() {
    let mut g = Graph::default();
    g.node_from("agent:a", NodeKind::Agent, Source::Line(1), None);
    for i in 0..12 {
        let kind = if i % 3 == 0 { EdgeKind::Ran } else { EdgeKind::Wrote };
        g.edge_from("agent:a", &format!("x{i}"), kind, Source::Line(i + 2), Some(format!("2026-11-23T10:{i:02}:00Z")));
    }
    let kinds = [EdgeKind::Wrote, EdgeKind::Ran];
    let want: Vec<*const Edge> = g.edges.iter().map(|e| e as *const Edge).collect();
    let v = crate::graph_kept::KeptView::new(&g, None);
    let got: Vec<*const Edge> = v.near("agent:a", &kinds).into_iter().map(|e| e as *const Edge).collect();
    assert_eq!(got, want, "in the graph's order");
}

/// The index's lists by id in chunks (`graph_kept::Chunked`; paper-research N-1446 p2072 1) read as one list: each
/// place what was pushed there or written over it, the length and the order the pushes', across three chunks and into a
/// fourth, at each chunk's last place and the next's first.
#[test]
fn a_chunked_list_reads_as_one_list_across_its_chunks() {
    use crate::graph_kept::{CHUNK_BITS, Chunked};
    let size = 1 << CHUNK_BITS;
    let n = 3 * size + 5;
    let mut c: Chunked<usize> = Chunked::default();
    assert_eq!(c.len(), 0);
    for i in 0..n {
        c.push(i * 7);
        assert_eq!(c.len(), i + 1, "the length after {i} pushed");
    }
    for i in (0..n).step_by(97).chain([size - 1, size, 2 * size - 1, 2 * size, 3 * size, n - 1]) {
        assert_eq!(c[i], i * 7, "place {i}");
    }
    c[size] = 1;
    assert_eq!((c[size - 1], c[size], c[size + 1]), ((size - 1) * 7, 1, (size + 1) * 7));
    let all: Vec<usize> = c.iter().copied().collect();
    assert_eq!(all.len(), n);
    assert!(all.iter().enumerate().all(|(i, x)| *x == if i == size { 1 } else { i * 7 }), "in the pushes' order");
}

/// A job's end at a commit whose node has not come is no red yet, on the index as on the one pass, and once the
/// commit's node comes it is: each course gives the ends from a job to a commit alone (`Course::job_ends`; the one pass
/// checks the kinds, paper-research N-1446 p1922 3).
#[test]
fn a_jobs_end_at_a_commit_not_come_yet_is_no_red_until_it_comes() {
    let mut g = Graph::default();
    let at = |m: &str| Some(format!("2026-11-23T10:0{m}:00Z"));
    g.node_from("agent:a", NodeKind::Agent, Source::Line(1), at("0"));
    g.node_from("job:1", NodeKind::Job, Source::Line(2), at("1"));
    g.get_mut("job:1").unwrap().ok = Some(false);
    for (from, to, kind, m) in [("turn:a:1", "agent:a", EdgeKind::TurnOf, "0"), ("agent:a", "job:1", EdgeKind::Ran, "1"), ("job:1", "c1", EdgeKind::Ran, "1"), ("c1", "branch:b", EdgeKind::On, "1"), ("job:1", "c1", EdgeKind::Ended, "2")] {
        g.edge_from(from, to, kind, Source::Line(2), at(m));
    }
    let end = "2026-11-23T10:05:00Z";
    let now = g.now(end);
    assert_eq!(now, g.now_walked(end), "before the commit's node");
    assert!(now.reds.is_empty() && now.old_reds == 0, "no red at a commit with no node: {:?}, {} old", now.reds, now.old_reds);
    g.node_from("c1", NodeKind::Commit, Source::Line(3), at("1"));
    let now = g.now(end);
    assert_eq!(now, g.now_walked(end), "once the commit's node came");
    assert_eq!(now.reds.iter().map(|r| r.branch.as_str()).collect::<Vec<_>>(), ["b"], "red once the commit's node came");
}

/// A commit whose node comes after its edge to one of his asks, the index read in between: once its node is in, it is
/// the ask's first commit, read off the kept index as off the whole pass (an edge at a commit is put at its other end
/// when the commit's node comes; paper-research N-1446 p1896).
#[test]
fn a_commit_whose_node_comes_after_its_edges_reaches_his_ask() {
    let mut g = Graph::default();
    let at = |t: &str| Some(t.to_string());
    g.node_from("N-1.p1", NodeKind::Post, Source::Line(1), at("2026-11-23T10:00:00Z"));
    g.edge_from(OWNER, "N-1.p1", EdgeKind::Wrote, Source::Line(1), at("2026-11-23T10:00:00Z"));
    g.edge_from("N-1.p1", "agent:a1", EdgeKind::Asks, Source::Line(1), at("2026-11-23T10:00:00Z"));
    g.edge_from("commit:c1", "N-1.p1", EdgeKind::Named, Source::Line(2), at("2026-11-23T10:01:00Z"));
    let before = g.ask_times_at(None);
    assert_eq!(before, g.ask_times_walked(None));
    assert_eq!(before.iter().map(|a| a.first_commit.is_some()).collect::<Vec<_>>(), vec![false], "no commit's node yet");
    g.node_from("commit:c1", NodeKind::Commit, Source::Line(2), at("2026-11-23T10:01:00Z"));
    let after = g.ask_times_at(None);
    assert_eq!(after, g.ask_times_walked(None));
    assert_eq!(after.iter().map(|a| a.first_commit.as_ref().map(|r| r.id.as_str())).collect::<Vec<_>>(), vec![Some("commit:c1")]);
}

/// A course that times each call of the one it wraps, by its name (`near`'s by its first kind): the calls, their time
/// and the edges they gave.
struct Timed<'a, 'g> {
    v: &'a dyn Course<'g>,
    spent: std::cell::RefCell<std::collections::BTreeMap<(&'static str, Option<EdgeKind>), (usize, std::time::Duration, usize)>>,
}

impl<'a, 'g> Timed<'a, 'g> {
    fn new(v: &'a dyn Course<'g>) -> Timed<'a, 'g> {
        Timed { v, spent: Default::default() }
    }

    fn time<T>(&self, name: &'static str, kind: Option<EdgeKind>, gave: impl Fn(&T) -> usize, f: impl FnOnce() -> T) -> T {
        let t = std::time::Instant::now();
        let r = f();
        let d = t.elapsed();
        let mut s = self.spent.borrow_mut();
        let x = s.entry((name, kind)).or_default();
        x.0 += 1;
        x.1 += d;
        x.2 += gave(&r);
        r
    }

    /// The read NAME's WHOLE time through this course, the part of it in the calls, and each call's line, the longest
    /// first.
    fn report(&self, name: &str, whole: std::time::Duration) {
        let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
        let s = self.spent.borrow();
        let calls: std::time::Duration = s.values().map(|x| x.1).sum();
        println!("{name}, timed: {:.1} ms, {:.1} ms of it in {} calls of the course, so {:.1} ms its own", ms(whole), ms(calls), s.values().map(|x| x.0).sum::<usize>(), ms(whole.saturating_sub(calls)));
        let mut rows: Vec<_> = s.iter().collect();
        rows.sort_by(|a, b| b.1.1.cmp(&a.1.1));
        for ((m, k), (n, d, gave)) in rows {
            println!("  {m}{}: {n} calls, {:.2} ms, {gave} edges given", k.map_or(String::new(), |k| format!(" {k:?}")), ms(*d));
        }
    }
}

impl<'g> Course<'g> for Timed<'_, 'g> {
    fn near(&self, x: &str, kinds: &[EdgeKind]) -> Vec<&'g Edge> {
        self.time("near", kinds.first().copied(), Vec::len, || self.v.near(x, kinds))
    }

    fn kind_of(&self, x: &str) -> Option<NodeKind> {
        self.time("kind_of", None, |_| 0, || self.v.kind_of(x))
    }

    fn commits_at(&self, x: &str) -> Vec<&'g Edge> {
        self.time("commits_at", None, Vec::len, || self.v.commits_at(x))
    }

    fn touched(&self, x: &str) -> bool {
        self.time("touched", None, |_| 0, || self.v.touched(x))
    }

    fn writer(&self, x: &str) -> Option<String> {
        self.time("writer", None, |_| 0, || self.v.writer(x))
    }

    fn asking(&self, x: &str) -> bool {
        self.time("asking", None, |_| 0, || self.v.asking(x))
    }

    fn first_handed(&self, t: &str, n: u32) -> Option<Reached> {
        self.time("first_handed", None, |_| 0, || self.v.first_handed(t, n))
    }

    fn first_answered(&self, t: &str, n: u32) -> Option<Reached> {
        self.time("first_answered", None, |_| 0, || self.v.first_answered(t, n))
    }

    fn has_turns(&self, x: &str) -> bool {
        self.time("has_turns", None, |_| 0, || self.v.has_turns(x))
    }

    fn session_last(&self, x: &str) -> Option<(At, usize)> {
        self.time("session_last", None, |_| 0, || self.v.session_last(x))
    }

    fn job_ends(&self) -> Vec<&'g Edge> {
        self.time("job_ends", None, Vec::len, || self.v.job_ends())
    }

    fn first_on(&self, c: &str) -> Option<&'g Edge> {
        self.time("first_on", None, |_| 0, || self.v.first_on(c))
    }

    fn agents(&self) -> Vec<&'g Node> {
        self.time("agents", None, |_| 0, || self.v.agents())
    }
}

/// `now` at the canvas's ask on a saved graph, read again and again so its own code can be sampled from outside the
/// process (paper-research N-1446 p2072 2): SAVED_GRAPH=DIR as [`a_saved_graph_s_reads_timed`] takes it,
/// SAVED_GRAPH_NOW_TIMES reads (else 300), each the same as the first. It says when its reads begin and when they end,
/// so stacks taken between the two lines are of them alone. Ignored, as the timed test is.
#[test]
#[ignore]
fn a_saved_graph_s_now_read_again_and_again() {
    use std::time::Instant;
    let Ok(dir) = std::env::var("SAVED_GRAPH") else { return };
    let times: usize = std::env::var("SAVED_GRAPH_NOW_TIMES").ok().and_then(|n| n.parse().ok()).unwrap_or(300).max(1);
    let g: Graph = serde_json::from_reader(std::io::BufReader::with_capacity(1 << 20, std::fs::File::open(format!("{dir}/graph.json")).unwrap())).unwrap();
    let moments: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(format!("{dir}/moments.json")).unwrap()).unwrap();
    let canvas = moments["canvas"].as_str().unwrap().to_string();
    g.keep();
    let first = g.now(&canvas);
    println!("now's reads begin: {times} at the canvas's ask {canvas}, {} agents", first.agents.len());
    let t = Instant::now();
    for _ in 0..times {
        assert!(std::hint::black_box(g.now(&canvas)) == first, "each read the same as the first");
    }
    let all = t.elapsed().as_secs_f64() * 1000.0;
    println!("now's reads end: {times} in {all:.0} ms, {:.2} ms each", all / times as f64);
}

/// The kept reads timed, and their parts, on a graph a copy of a record saved (paper-research N-1446 p1865: each read
/// under coordd's lock under a frame; ignored, as coordd's copy is): SAVED_GRAPH=DIR, as coordd's
/// `a_coords_record_now` wrote it with SAVE_GRAPH=DIR, DIR/graph.json the graph after its reads and DIR/moments.json the
/// canvas's ask, AT and the graph's length (nodes, edges) before the sessions' side and after each put (a session's, or
/// a part of one). The graph is grown again in that order, the index caught up after each put as coordd's side round
/// does, each catch-up timed; then each read five times on the whole (its least and its middle), and once through a
/// course that times each call ([`Timed`]), so what a read spends in the index's calls and in its own code is apart.
#[test]
#[ignore]
fn a_saved_graph_s_reads_timed() {
    use std::time::{Duration, Instant};
    let Ok(dir) = std::env::var("SAVED_GRAPH") else { return };
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let t = Instant::now();
    let g: Graph = serde_json::from_reader(std::io::BufReader::with_capacity(1 << 20, std::fs::File::open(format!("{dir}/graph.json")).unwrap())).unwrap();
    let moments: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(format!("{dir}/moments.json")).unwrap()).unwrap();
    let (canvas, at) = (moments["canvas"].as_str().unwrap().to_string(), moments["at"].as_str().unwrap().to_string());
    let grew: Vec<(usize, usize)> = serde_json::from_value(moments["grew"].clone()).unwrap();
    println!("saved graph {dir}: {} nodes, {} edges, read in {:.1} s; the canvas's ask {canvas}, AT {at}; {} puts", g.nodes.len(), g.edges.len(), t.elapsed().as_secs_f64(), grew.len().saturating_sub(1));
    // the side's catch-ups, the graph grown again as it grew, each in its two halves: the edges in, then each request
    // marked made again, each of those timed
    let mut grown: Option<Graph> = None;
    if grew.windows(2).all(|w| w[0].0 <= w[1].0 && w[0].1 <= w[1].1) && grew.last().is_some_and(|l| l.0 <= g.nodes.len() && l.1 <= g.edges.len()) {
        let (n0, e0) = grew[0];
        // a catch-up: its time, its edges and nodes, its edges' time in, and the requests made again (time, name)
        struct Up {
            all: f64,
            edges: usize,
            nodes: usize,
            took_in: f64,
            made: Vec<(f64, String)>,
            grew: String,
            // of `took_in`, the piece's nodes alone, before its edges
            nodes_in: f64,
        }
        // the index's parts that grew by 2 MB or more in a half (a table made bigger at once: a catch-up's own growth
        // is some hundred bytes an edge)
        let grew_in = |half: &str, a: &[(&str, usize)], b: &[(&str, usize)]| -> Vec<String> {
            let mb = |x: usize| x as f64 / 1e6;
            a.iter().zip(b).filter(|(x, y)| y.1 >= x.1 + 2_000_000).map(|(x, y)| format!("{half}: {} {:.1} → {:.1} MB", x.0, mb(x.1), mb(y.1))).collect()
        };
        // the side's round grown again ROUNDS times (SAVED_GRAPH_ROUNDS, else 3), each catch-up kept at its least of
        // them, as the machine's other work makes one round's times wander (one part's by three times, 23 Nov)
        let rounds: usize = std::env::var("SAVED_GRAPH_ROUNDS").ok().and_then(|r| r.parse().ok()).unwrap_or(3).max(1);
        // each put as it went in (nodes from, to; edges from, to), or with SAVED_GRAPH_MOST one over that many edges in
        // pieces of that many, its nodes with the first: coordd's parts with a session's whole turn of `sent_in` put
        // about SIDE_PART at a time, as the saved lengths, from before, do not show
        let most: Option<usize> = std::env::var("SAVED_GRAPH_MOST").ok().and_then(|m| m.parse().ok()).filter(|m| *m > 0);
        let mut puts: Vec<(usize, usize, usize, usize)> = Vec::new();
        for w in grew.windows(2) {
            let ((n, e), (n2, e2)) = (w[0], w[1]);
            let step = most.unwrap_or(usize::MAX);
            let mut at = e;
            loop {
                let to = e2.min(at.saturating_add(step));
                puts.push((if at == e { n } else { n2 }, n2, at, to));
                at = to;
                if at >= e2 {
                    break;
                }
            }
        }
        if let Some(m) = most {
            println!("catch-up: the {} puts in {} pieces of at most {m} edges", grew.len().saturating_sub(1), puts.len());
        }
        let mut ups: Vec<Up> = Vec::new();
        let mut first: Vec<f64> = Vec::new();
        for round in 0..rounds {
            let mut h = Graph::of_parts(g.nodes[..n0].to_vec(), g.edges[..e0].to_vec());
            let t = Instant::now();
            h.keep();
            println!("catch-up, round {}: the index made from nothing at the side's start, {n0} nodes and {e0} edges, in {:.1} ms", round + 1, ms(t.elapsed()));
            for (i, &(n, n2, e, e2)) in puts.iter().enumerate() {
                h.put_parts(g.nodes[n..n2].iter().cloned(), g.edges[e..e2].iter().cloned());
                let p = h.parts();
                let mut k = h.kept.lock();
                let b0 = k.bytes();
                // its nodes first, against the edges before it (as `take_in` takes nodes before edges), then its edges
                let t = Instant::now();
                k.take_in(&crate::graph_kept::Parts { nodes: p.nodes, edges: &p.edges[..e] });
                let nodes_in = ms(t.elapsed());
                k.take_in(&p);
                let took_in = ms(t.elapsed());
                let b1 = k.bytes();
                let t = Instant::now();
                let mut made: Vec<(f64, u32)> = Vec::new();
                for r in k.take_dirty() {
                    let t = Instant::now();
                    k.make(&p, r);
                    made.push((ms(t.elapsed()), r));
                }
                let all = took_in + ms(t.elapsed());
                let grew = [grew_in("in", &b0, &b1), grew_in("made again", &b1, &k.bytes())].concat().join(", ");
                made.sort_by(|a, b| b.0.total_cmp(&a.0));
                let made: Vec<(f64, String)> = made.iter().map(|(d, r)| (*d, if *d > 0.1 { k.name(*r).to_string() } else { String::new() })).collect();
                let up = Up { all, edges: e2 - e, nodes: n2 - n, took_in, made, grew, nodes_in };
                if round == 0 {
                    first.push(all);
                }
                match ups.get_mut(i) {
                    None => ups.push(up),
                    Some(u) if up.all < u.all => *u = up,
                    Some(_) => {}
                }
            }
            if round + 1 == rounds {
                grown = Some(h);
            }
        }
        let all: f64 = ups.iter().map(|u| u.all).sum();
        let edges: usize = ups.iter().map(|u| u.edges).sum();
        let making: f64 = ups.iter().flat_map(|u| &u.made).map(|m| m.0).sum();
        let made: usize = ups.iter().map(|u| u.made.len()).sum();
        first.sort_by(|a, b| b.total_cmp(a));
        println!("catch-up, the first round alone (its memory new to the process, as coordd's first round after a start): {} over 16 ms; the longest five {}", first.iter().filter(|x| **x > 16.0).count(), first.iter().take(5).map(|x| format!("{x:.1} ms")).collect::<Vec<_>>().join(", "));
        println!("catch-up, each at its least of {rounds} rounds: {} puts' edges, {edges} in all, caught up in {all:.1} ms ({:.2} µs an edge); {} over 16 ms; of it {made} requests made again in {making:.1} ms", ups.len(), 1000.0 * all / edges.max(1) as f64, ups.iter().filter(|u| u.all > 16.0).count());
        ups.sort_by(|a, b| b.all.total_cmp(&a.all));
        println!("catch-up, the longest ten (ms, edges, nodes; in: its nodes, then its edges; the requests made again): {}", ups.iter().take(10).map(|u| format!("{:.1} ms {} edges {} nodes ({:.2} µs an edge): in {:.1} ms ({:.1} + {:.1}), {} made again in {:.1} ms", u.all, u.edges, u.nodes, 1000.0 * u.all / u.edges.max(1) as f64, u.took_in, u.nodes_in, u.took_in - u.nodes_in, u.made.len(), u.made.iter().map(|m| m.0).sum::<f64>())).collect::<Vec<_>>().join("; "));
        for u in ups.iter().take(3) {
            println!("  the catch-up of {:.1} ms, its costliest five made again: {}", u.all, u.made.iter().take(5).map(|(d, r)| format!("{r} {:.0} µs", 1000.0 * d)).collect::<Vec<_>>().join(", "));
        }
        for u in ups.iter().take(10) {
            println!("  the catch-up of {:.1} ms, the index's parts grown at once: {}", u.all, if u.grew.is_empty() { "none" } else { u.grew.as_str() });
        }
    } else {
        println!("catch-up: not timed: the lengths saved do not grow to the graph's");
    }
    let t = Instant::now();
    g.keep();
    println!("the index made from nothing on the whole graph in {:.1} ms", ms(t.elapsed()));
    // the index caught up as the side's round grew it reads as the one made from nothing
    if let Some(h) = grown.filter(|h| h.nodes.len() == g.nodes.len() && h.edges.len() == g.edges.len()) {
        let same = |a: String, b: String| if a == b { "the same" } else { "NOT the same" };
        let his = Ask { layers: BTreeSet::from([Layer::His]), limit: 40_000, ..Ask::default() };
        println!("the graph grown again, its index caught up put by put: costs_at {}, asks_now {}, now at the canvas's ask {}, his layer's slice {}, as on the index made from nothing", same(json(&h.costs_at(None)), json(&g.costs_at(None))), same(json(&h.asks_now(None)), json(&g.asks_now(None))), same(json(&h.now(&canvas)), json(&g.now(&canvas))), same(json(&h.slice(&his)), json(&g.slice(&his))));
    }
    // his layer's slice as the Map asks it (limit 40,000): off the kept layers, and from one pass over every edge
    // (docket-edge5 N-1446 p2532)
    let his = Ask { layers: BTreeSet::from([Layer::His]), limit: 40_000, ..Ask::default() };
    // each read five times: its least and its middle
    let five = |name: &str, f: &dyn Fn() -> usize| {
        let mut ts: Vec<f64> = Vec::new();
        let mut n = 0;
        for _ in 0..5 {
            let t = Instant::now();
            n = f();
            ts.push(ms(t.elapsed()));
        }
        ts.sort_by(f64::total_cmp);
        println!("read {name}: {n} rows; least {:.1} ms, middle {:.1} ms, most {:.1} ms", ts[0], ts[2], ts[4]);
    };
    let asks = g.asks_now(None);
    let times: Vec<&AskTimes> = asks.iter().map(|a| &a.times).collect();
    five("costs_at, the whole graph", &|| g.costs_at(None).len());
    five("asks_now, the whole graph", &|| g.asks_now(None).len());
    five("his_after", &|| g.his_after(&times).len());
    five("ask_times_at, the whole graph", &|| g.ask_times_at(None).len());
    five("now at the canvas's ask", &|| g.now(&canvas).agents.len());
    five("now at AT", &|| g.now(&at).agents.len());
    five("asks_now at the canvas's ask", &|| g.asks_now(Some(&canvas)).len());
    // the whole graph's again, read after the others as the canvas's ask is: whether the canvas's costs more for its
    // moment or for its place in the order (paper-research N-1446 p2072 3)
    five("asks_now, the whole graph, again", &|| g.asks_now(None).len());
    // each read's rows as JSON, by a digest (FNV-1a, 64 bits) to set beside another build's on the same graph, and
    // beside its twin's from one pass over every edge
    let digest = |name: &str, kept: String, walked: String| {
        let fnv = |s: &str| s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
        println!("rows of {name}: {} bytes, digest {:016x}; its twin from one pass {}", kept.len(), fnv(&kept), if kept == walked { "the same" } else { "NOT the same" });
    };
    fn json<T: serde::Serialize>(x: &T) -> String {
        serde_json::to_string(x).unwrap()
    }
    digest("costs_at, the whole graph", json(&g.costs_at(None)), json(&g.costs_walked(None)));
    digest("asks_now, the whole graph", json(&asks), json(&g.asks_now_walked(None)));
    digest("his_after", json(&g.his_after(&times)), json(&g.his_after_walked(&times)));
    digest("ask_times_at, the whole graph", json(&g.ask_times_at(None)), json(&g.ask_times_walked(None)));
    digest("now at the canvas's ask", json(&g.now(&canvas)), json(&g.now_walked(&canvas)));
    digest("now at AT", json(&g.now(&at)), json(&g.now_walked(&at)));
    digest("asks_now at the canvas's ask", json(&g.asks_now(Some(&canvas))), json(&g.asks_now_walked(Some(&canvas))));
    five("slice, his layer", &|| g.slice(&his).edges.len());
    five("slice, his layer, from one pass", &|| g.slice_walked(&his).edges.len());
    digest("slice, his layer", json(&g.slice(&his)), json(&g.slice_walked(&his)));
    digest("slice, the whole graph", json(&g.slice(&Ask::default())), json(&g.slice_walked(&Ask::default())));
    // the latest edge's time: a moment at or after it reads as none in the index
    let latest = g.edges.iter().filter_map(|e| e.when.as_deref()).max().unwrap_or("");
    println!("the latest edge's time {latest}: the canvas's ask {canvas} is {} it, AT {at} {} it", if canvas.as_str() >= latest { "at or after" } else { "before" }, if at.as_str() >= latest { "at or after" } else { "before" });
    // each through the timed course, on a view of its own (a view holds the index's lock)
    {
        let t = Instant::now();
        let v = crate::graph_kept::KeptView::new(&g, None);
        println!("a view of the whole graph made (its catch-up, nothing new) in {:.3} ms", ms(t.elapsed()));
        let t = Instant::now();
        let his = v.his_asks();
        println!("costs_at's own: his asks ({}) in {:.1} ms", his.len(), ms(t.elapsed()));
        let timed = Timed::new(&v);
        let t = Instant::now();
        let n = g.asks_by(None, &timed).len();
        timed.report(&format!("asks_now, the whole graph ({n} asks)"), t.elapsed());
    }
    {
        let v = crate::graph_kept::KeptView::new(&g, Some(&canvas));
        let timed = Timed::new(&v);
        let t = Instant::now();
        let now = g.now_with(&canvas, &timed);
        timed.report(&format!("now at the canvas's ask ({} agents, {} asks, {} reds)", now.agents.len(), now.asks.len(), now.reds.len()), t.elapsed());
        // what `now` reads that is no call of the course: a job's node by its id; and every node read for the agents,
        // as it read them before the index kept them
        let t = Instant::now();
        let agents = g.nodes.iter().filter(|n| n.kind == NodeKind::Agent).count();
        println!("every node read for its agents ({} nodes, {agents} agents), as `now` read them before, in {:.2} ms", g.nodes.len(), ms(t.elapsed()));
        let ends = v.job_ends();
        let t = Instant::now();
        let ok = ends.iter().filter_map(|e| g.get(&e.from)).filter(|j| j.ok == Some(true)).count();
        println!("now's own: each job end's job by its id (`Graph::get`, {} ends, {ok} green) in {:.2} ms", ends.len(), ms(t.elapsed()));
    }
    {
        let v = crate::graph_kept::KeptView::new(&g, Some(&canvas));
        let timed = Timed::new(&v);
        let t = Instant::now();
        let n = g.asks_by(Some(&canvas), &timed).len();
        timed.report(&format!("asks_now at the canvas's ask ({n} asks)"), t.elapsed());
    }
}
