//! A made-up record for the Map's tests and stills, at research's size on 21 Nov (docket-edge5, N-1479 p11: 951 lines
//! of the outline; the snapshot's posts about 2,300, N-1104 alone 700): two projects, research's six topics with their
//! threads and items, its sixteen agents held on a limit as at 11:01 (p715 path 2), its accounts, his posts lit, his
//! p678 (path 1). Every name and word here is made up; nothing is read. Placed simply, each node inside its parent, so
//! the tests check the canvas and not a layout (the layout is docket-edge5's, map/layout.rs).
//!
//! And the same record as the window holds it ([`research`], [`cav`]: Insight's rows, the snapshot's tickets and
//! agents), for the layout to lay as the window does (map/join.rs): path 2 through the layout, and the overview.

use super::{Edge, Graph, Kind, Link, Node, Rect, State, Tier};

/// The topics of research, with how many lines each holds (Your chat's first is N-1104, his chat, larger than the
/// rest).
pub const TOPICS: [(&str, usize); 6] = [("Your chat", 70), ("0.3 prototype", 150), ("Claude parity", 140), ("Builds", 150), ("AI-use detection", 130), ("Window look", 140)];
/// The agents, by name.
pub const AGENTS: [&str; 16] = [
    "paper-research", "proto-lead", "proto-window", "proto-reader", "consult-3", "accounts-lead", "accounts-2", "docket-core", "docket-core2", "docket-core4", "docket-core5", "docket-edge2", "docket-edge5",
    "docket-edge6", "docket-edge7", "docket-edge8",
];

const MARGIN: f64 = 300.0;
const CELL: f64 = 240.0;
const THREAD: f64 = 210.0;
const BAND: f64 = 420.0;

struct B {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    n: u64,
}

impl B {
    fn push(&mut self, mut n: Node, f: impl FnOnce(&mut Node)) -> u32 {
        f(&mut n);
        self.nodes.push(n);
        (self.nodes.len() - 1) as u32
    }

    /// Thread or item N at R in PARENT, with POSTS posts in COLS columns, every EVERY-th his (none for 0).
    fn thread(&mut self, parent: u32, r: Rect, kind: Kind, label: String, posts: usize, cols: usize, every: usize) -> u32 {
        self.n += 1;
        let n = self.n;
        let id = if kind == Kind::Item { format!("I-{n}") } else { format!("N-{n}") };
        let st = match (n % 29, n % 9) {
            (0, _) => State::WaitsOnHim,
            (_, 1 | 2) => State::Ended,
            (_, 3) => State::Working,
            _ => State::Plain,
        };
        let t = self.push(Node::new(id.clone(), kind, label, Some(parent), r), |x| {
            x.state = st;
            x.note = match st {
                State::WaitsOnHim => "waits on you".into(),
                State::Working => "at work".into(),
                State::Ended => "done".into(),
                _ => String::new(),
            };
            x.tier = if st == State::WaitsOnHim { Tier::WaitsOnHim } else { Tier::Rest };
            x.weight = posts as f32;
        });
        self.posts(t, &id, r, posts, cols, every);
        t
    }

    /// POSTS posts of thread T (ID) at R under its head, every EVERY-th his (none for 0): top-down by number at a fixed
    /// row height, a column filled before the next beside it, the rect with rows for twice its posts (at least 16), so
    /// a post added goes below the last and none moves (docket-edge5's rule for map/layout.rs, N-1479 p18 and p23).
    fn posts(&mut self, t: u32, id: &str, r: Rect, posts: usize, cols: usize, every: usize) {
        if posts == 0 {
            return;
        }
        let (pad, head) = (r.w * 0.03, r.w.min(r.h) * 0.1);
        let rows = (2 * posts).div_ceil(cols).max(16);
        let w = (r.w - pad * (cols as f64 + 1.0)) / cols as f64;
        let step = (r.h - head - pad) / rows as f64;
        for p in 0..posts {
            let (c, k) = (p / rows, p % rows);
            let pr = Rect::new(r.x + pad + c as f64 * (w + pad), r.y + head + k as f64 * step, w, step * 0.82);
            let his = every > 0 && p % every == 0;
            let n = p + 1;
            self.push(Node::new(format!("{id}.p{n}"), Kind::Post, format!("post {n} of {id}"), Some(t), pr), |x| {
                x.his = his;
                x.tier = if his { Tier::HisAsk } else { Tier::Rest };
            });
        }
    }
}

/// The record: research and cav-paper.
pub fn record() -> Graph {
    let mut b = B { nodes: Vec::new(), edges: Vec::new(), n: 1000 };
    let research = Rect::new(0.0, 0.0, 12_000.0, 8_000.0);
    let r = b.push(Node::new("project:research", Kind::Project, "research", None, research), |_| {});
    let (tw, th) = ((research.w - 4.0 * MARGIN) / 3.0, (research.h - 3.0 * MARGIN - BAND) / 2.0);
    let mut agent = 0;
    for (ti, (name, lines)) in TOPICS.iter().enumerate() {
        let tr = Rect::new(MARGIN + (ti % 3) as f64 * (tw + MARGIN), MARGIN + (ti / 3) as f64 * (th + MARGIN), tw, th);
        let t = b.push(Node::new(format!("topic:{name}"), Kind::Topic, *name, Some(r), tr), |_| {});
        let cols = ((tr.w - 80.0) / CELL).floor() as usize;
        let mut slot = 0usize;
        let mut left = *lines;
        if ti == 0 {
            // his chat: N-1104, 700 posts in ten columns (room for twice as many), every other his, p678 his ask
            let big = Rect::new(tr.x + 40.0, tr.y + 120.0, 1_000.0, 2_900.0);
            b.n = 1103;
            b.thread(t, big, Kind::Thread, "N-1104 · his chat".into(), 700, 10, 2);
            // the other lines after his chat's number, so none is named twice
            b.n = 2000;
            left -= 1;
        }
        while left > 0 {
            let (c, row) = (slot % cols, slot / cols);
            slot += 1;
            if ti == 0 && c < 5 {
                continue;
            }
            let x = tr.x + 40.0 + c as f64 * CELL;
            let y = tr.y + 120.0 + row as f64 * CELL;
            if y + THREAD > tr.bottom() - 260.0 {
                break;
            }
            let kind = if slot % 17 == 1 { Kind::Item } else { Kind::Thread };
            let posts = 1 + (slot * 7) % 6;
            let label = format!("{} {}", if kind == Kind::Item { "item" } else { "thread" }, slot);
            // a thread he opened: its first post his
            let every = if slot % 11 == 3 { posts + 1 } else { 0 };
            b.thread(t, Rect::new(x, y, THREAD, THREAD), kind, label, posts, 1, every);
            left -= 1;
        }
        // its agents along its foot
        for k in 0..3 {
            if agent >= AGENTS.len() || (ti == 5 && k > 0) {
                break;
            }
            let ar = Rect::new(tr.x + 60.0 + k as f64 * 200.0, tr.bottom() - 200.0, 80.0, 80.0);
            let name = AGENTS[agent];
            b.push(Node::new(format!("agent:{name}"), Kind::Agent, name, Some(t), ar), |x| {
                x.state = State::Held;
                x.note = "held · Opus cooling until 13:31".into();
                x.tier = Tier::Stopped;
            });
            agent += 1;
        }
    }
    assert_eq!(agent, AGENTS.len(), "every agent on a topic");
    // the accounts, in research's corner band
    let band_y = research.bottom() - BAND + 60.0;
    let gw = b.push(Node::new("account:gateway-opus", Kind::Account, "Gateway · Opus", Some(r), Rect::new(MARGIN, band_y, 900.0, 300.0)), |x| {
        x.state = State::Held;
        x.note = "cooling until 13:31".into();
        x.tier = Tier::Stopped;
    });
    b.push(Node::new("account:codex", Kind::Account, "Codex", Some(r), Rect::new(MARGIN + 1_100.0, band_y, 900.0, 300.0)), |x| x.note = "526 calls in 2 h".into());
    for (i, n) in b.nodes.iter().enumerate() {
        if n.kind == Kind::Agent {
            b.edges.push(Edge::new(i as u32, gw, Link::Uses));
        }
    }
    // his p678 (path 1)
    if let Some(p) = b.nodes.iter_mut().find(|n| n.id == "N-1104.p678") {
        p.label = "d h m, please, on the Accounts tab".into();
        p.note = "d h m on the Accounts tab · in your window since 10:34 (build 59)".into();
        p.his = true;
        p.tier = Tier::HisAsk;
        // his ask still open: before his other posts where they would fall on one point
        p.weight = 1.0;
    }
    // a thread naming an item in another topic
    let (a, i) = (b.nodes.iter().position(|n| n.kind == Kind::Thread && n.id != "N-1104"), b.nodes.iter().rposition(|n| n.kind == Kind::Item));
    if let (Some(a), Some(i)) = (a, i) {
        b.edges.push(Edge::new(a as u32, i as u32, Link::Names));
    }
    // cav-paper, smaller, to the right
    let cav = Rect::new(13_000.0, 2_000.0, 4_000.0, 3_000.0);
    let c = b.push(Node::new("project:cav-paper", Kind::Project, "cav-paper", None, cav), |_| {});
    let t = b.push(Node::new("topic:Paper", Kind::Topic, "Paper", Some(c), Rect::new(cav.x + 200.0, cav.y + 200.0, 3_600.0, 2_600.0)), |_| {});
    b.n = 1;
    for k in 0..60usize {
        let x = cav.x + 240.0 + (k % 13) as f64 * CELL;
        let y = cav.y + 320.0 + (k / 13) as f64 * CELL;
        b.thread(t, Rect::new(x, y, THREAD, THREAD), Kind::Thread, format!("section {k}"), 1 + k % 4, 1, if k % 9 == 0 { 8 } else { 0 });
    }
    Graph::new(b.nodes, b.edges).expect("each node inside its parent")
}

/// A time for the made-up snapshot when no clock is read: 21 Nov 2026, 10:00 UTC.
pub const NOW: i64 = 1_795_255_200;

/// A project as the window holds it, made up, for the layout to lay (map/join.rs): Insight's rows of its outline, its
/// snapshot's tickets and agents.
pub struct Snapshot {
    pub rows: Vec<docket_machine::insight::Thread>,
    pub tickets: std::collections::BTreeMap<u64, coord_types::TicketView>,
    pub agents: Vec<coord_types::AgentView>,
}

impl Snapshot {
    /// The island it is, named ID.
    pub fn island<'a>(&'a self, id: &'a str) -> super::layout::Island<'a> {
        super::layout::Island { id, name: id, lines: &self.rows, tickets: &self.tickets, agents: &self.agents, accounts: None }
    }
}

/// Words for the made-up lines' labels.
const WORDS: [&str; 24] = [
    "accounts tab", "the Map", "builds", "window look", "his chat", "release train", "proxy log", "spinner", "typing", "message box", "popup", "outline",
    "insight", "brain", "gateway", "AI-use detection", "parity", "stills", "runner", "comment box", "lenses", "push and pull", "archive", "search",
];

/// A line of the outline, made up: ID, its KIND, its LABEL, the line it stands under, its agents, its STATE, its last
/// post.
fn row(id: String, kind: &str, label: String, parent: Option<String>, agents: &[&str], state: &str, last: u32) -> docket_machine::insight::Thread {
    docket_machine::insight::Thread {
        id,
        label,
        kind: kind.into(),
        opened: "2026-11-10T10:00:00Z".into(),
        by: None,
        ended: None,
        state: state.into(),
        archived: false,
        last: Some(last),
        parent,
        agents: agents.iter().map(|a| a.to_string()).collect(),
        started: Vec::new(),
    }
}

/// Ticket ID's view in the snapshot, made up: its POSTS, each (by, seconds before NOW, words), and the posts of his no
/// later post answers.
fn view(id: &str, posts: &[(String, i64, String)], unanswered: &[u32], now: i64) -> coord_types::TicketView {
    let n: u64 = id.trim_start_matches(|c: char| !c.is_ascii_digit()).parse().unwrap();
    let at = |ago: i64| chrono::DateTime::from_timestamp(now - ago, 0).unwrap().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let posts: Vec<_> = posts.iter().enumerate().map(|(i, (by, ago, text))| serde_json::json!({"n": i + 1, "at": at(*ago), "by": by, "text": text, "names": []})).collect();
    serde_json::from_value(serde_json::json!({"id": id, "n": n, "kind": "thread", "opener": "owner", "ctx": {}, "form": {}, "title": id,
        "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(86_400 * 10),
        "given": [], "posts": posts, "unanswered_owner": unanswered, "mentions": {}}))
    .unwrap()
}

fn agent(name: &str, model: &str, state: &str, on: Option<String>) -> coord_types::AgentView {
    let mut a: coord_types::AgentView = serde_json::from_value(serde_json::json!({
        "id": "", "name": name, "parent": "paper-research", "kind": "claude", "model": model, "state": state, "since": "2026-11-21T09:00:00Z",
        "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null
    }))
    .unwrap();
    a.on = on;
    a
}

/// Research as the window holds it at NOW, made up at its size on 21 Nov (docket-edge5 N-1479 p11: 951 lines of the
/// outline; the snapshot's posts about 2,300, N-1104 alone 700):
/// - 951 lines: N-1104 his chat; 40 items with four threads under each; 750 threads at the root, given to the agents
///   in pairs, so the rare ones gather them into topics (way 2); every 37th waits on him, every 5th at work, every
///   101st failed, the rest quiet;
/// - the snapshot's views: N-1104 with 700 posts, every other his (his p678 still open, his of the last hour lit), and
///   80 more open threads with a few posts each, his first post on every 7th, a day old or less;
/// - the sixteen agents of [`AGENTS`] held on the gateway's Opus limit until NOW + 2 h 30 m (p715 path 2, as at 11:01),
///   each on a line, and four more at work.
pub fn research(now: i64) -> Snapshot {
    let mut rows = vec![row("N-1104".into(), "thread", "his chat".into(), None, &["paper-research"], "waits on you", 700)];
    let pool: Vec<String> = AGENTS.iter().map(|a| a.to_string()).chain((0..8).map(|i| format!("worker-{i}"))).collect();
    let state = |k: usize| if k % 37 == 0 { "waits on you" } else if k % 101 == 0 { "failed" } else if k % 5 == 0 { "at work" } else { "quiet" };
    for i in 0..40 {
        let item = format!("I-{}", 1400 + i);
        let ag = [pool[i % pool.len()].as_str(), pool[(i * 5 + 1) % pool.len()].as_str()];
        rows.push(row(item.clone(), "item", format!("{} {}", WORDS[i % WORDS.len()], i), None, &ag, state(i + 1), 3));
        for k in 0..4 {
            let n = 2000 + i * 4 + k;
            rows.push(row(format!("N-{n}"), "thread", format!("{} check {k}", WORDS[i % WORDS.len()]), Some(item.clone()), &ag[..1 + k % 2], state(n), (1 + k * 3) as u32));
        }
    }
    for i in 0..750 {
        let n = 3000 + i;
        let ag = [pool[(i / 30) % pool.len()].as_str(), pool[(i / 30 + 7) % pool.len()].as_str()];
        rows.push(row(format!("N-{n}"), "thread", format!("{} {}", WORDS[(i / 30) % WORDS.len()], i % 30), None, &ag, state(n), (1 + i % 9) as u32));
    }
    rows.sort_by_key(|x| x.id.trim_start_matches(|c: char| !c.is_ascii_digit()).parse::<u64>().unwrap());
    let mut tickets = std::collections::BTreeMap::new();
    let chat: Vec<(String, i64, String)> = (1..=700)
        .map(|n| {
            let his = n % 2 == 0;
            let words = if n == 678 { "d h m, please, on the Accounts tab".to_string() } else if his { format!("his post {n}") } else { format!("paper-research's answer {n}") };
            (if his { "owner".to_string() } else { "paper-research".to_string() }, (701 - n as i64) * 120, words)
        })
        .collect();
    let v = view("N-1104", &chat, &[678], now);
    tickets.insert(v.n, v);
    for i in 0..80 {
        let id = format!("N-{}", 3000 + i * 9);
        let posts: Vec<(String, i64, String)> = (0..1 + i % 6).map(|k| (if k == 0 && i % 7 == 0 { "owner".into() } else { pool[i % pool.len()].clone() }, 3_600 * (6 - k as i64), format!("post {} of {id}", k + 1))).collect();
        let v = view(&id, &posts, &[], now);
        tickets.insert(v.n, v);
    }
    let mut agents: Vec<coord_types::AgentView> = AGENTS
        .iter()
        .enumerate()
        .map(|(i, a)| coord_types::AgentView { limit_until: Some(now + 9_000), limit_why: Some("429 rate_limit_error: Opus".into()), ..agent(a, "claude-opus-4-5", "idle", Some(format!("N-{}", 3000 + i * 45))) })
        .collect();
    agents.extend((0..4).map(|i| agent(&format!("worker-{i}"), "claude-sonnet-4-5", "working", Some(format!("N-{}", 2000 + i * 4)))));
    Snapshot { rows, tickets, agents }
}

/// The asks on research's snapshot (its TICKETS) at NOW, made up (I-1455 (B)'s ledger and threads.rs `open_to_owner`; docket-edge5
/// N-1479 p313), for the ring's tests and stills; not in [`research`], whose posts the other tests count:
/// - his p678 asks paper-research, who has not answered: the ring;
/// - his p676 asked three, each answered: paper-research in his chat at p677, docket-edge8 by the reply tool on N-3009
///   (its p3, posted here for it), docket-edge2 on W-12, a ticket the island does not lay: the ring with his dot, its
///   trail to the two the island lays;
/// - proto-window's N-3018.p1 asks him and waits on him: the ring in the waits-on-you ink.
pub fn asks(tickets: &mut std::collections::BTreeMap<u64, coord_types::TicketView>, now: i64) {
    use serde_json::json;
    let at = |ago: i64| chrono::DateTime::from_timestamp(now - ago, 0).unwrap().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    fn on<'a>(t: &'a mut std::collections::BTreeMap<u64, coord_types::TicketView>, id: &str) -> &'a mut coord_types::TicketView {
        t.values_mut().find(|v| v.id == id).unwrap_or_else(|| panic!("{id} in the snapshot"))
    }
    on(tickets, "N-1104").asked = serde_json::from_value(json!([
        {"post": 676, "agent": "paper-research", "answer": "N-1104.p677"},
        {"post": 676, "agent": "docket-edge8", "answer": "N-3009.p3"},
        {"post": 676, "agent": "docket-edge2", "answer": "W-12.p1"},
        {"post": 678, "agent": "paper-research"},
    ]))
    .unwrap();
    on(tickets, "N-3009").posts.push(serde_json::from_value(json!({"n": 3, "at": at(2_700), "by": "docket-edge8", "text": "answered by the reply tool", "names": []})).unwrap());
    let ask = &mut on(tickets, "N-3018").posts[0];
    ask.asks = serde_json::from_value(json!([{"at": [0, 4]}])).unwrap();
    ask.asks_open = true;
    ask.told = vec!["owner".into()];
}

/// cav-paper at NOW, made up: 60 sections at its outline's root, one agent at work; its N-12 is its own, not
/// research's (consult-3 N-1479 p16).
pub fn cav(now: i64) -> Snapshot {
    let rows = (1..=60).map(|n| row(format!("N-{n}"), "thread", format!("section {n}"), None, &["cav-writer"], if n % 9 == 0 { "at work" } else { "quiet" }, 2)).collect();
    let mut tickets = std::collections::BTreeMap::new();
    let v = view("N-12", &[("cav-writer".into(), 600, "section 12, drafted".into())], &[], now);
    tickets.insert(v.n, v);
    Snapshot { rows, tickets, agents: vec![agent("cav-writer", "claude-sonnet-4-5", "working", Some("N-12".into()))] }
}
