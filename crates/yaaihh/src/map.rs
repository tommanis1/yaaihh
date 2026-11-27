//! The Map (I-1478; the owner, N-1104 p599: "one big pile sort of unsorted but structured … the views … are lenses on
//! that structure"; p606: "Zoom into a thread and you see its program … detail appears by depth": "yes yes this";
//! p714/p715, the spec): one canvas over his record, panned and zoomed continuously, each node drawn at the detail its
//! size on screen allows, in place. This module is what the Map draws: its nodes, each in a rectangle of one world, and
//! the links between them. What places them (the layout, docket-edge5's part), what draws them (the canvas, its
//! camera and its levels of detail) and the deep levels drawn inside a step (docket-edge8's) all share these types.
//!
//! - **One world.** A node's [`Rect`] is in world units and does not depend on the zoom: going in is only the camera
//!   moving, so a node keeps its place at every level (p715: "the same places, more detail as you go in"). The world
//!   is f64, so a call on the wire, tens of thousands of times smaller than a project, is still placed exactly.
//! - **Nesting.** Every node but a project lies inside its parent's rectangle ([`Graph::push`] checks it): a post inside
//!   its thread, the thread inside its topic, the topic inside its project; a project lies inside nothing. The crumb trail is the chain of parents
//!   ([`Graph::path`]).
//! - **Ids are lens nodes.** A node's id is the string `lens::zoom` opens (`N-1104`, `N-1104.p587`, `I-1469`, a run's
//!   step `A-….s1`), so a double-click opens what it names; a node coordd has no id for is named by its kind
//!   (`topic:I-1469`, `agent:docket-edge6`, `build:60`, `account:codex`, `project:research`). An id is one project's:
//!   research's N-1 and cav-paper's are two nodes ([`Graph::find_in`]).
//! - **Rank** decides which labels go on first when they would collide (p715: his asks, then what waits on him, then
//!   stopped or held, then the rest).
//!
//! Not drawn in the first Map (paper-research N-1479 p9): heads and their reads (until I-1480's `read`), what a build
//! carried (only the window-release lines: which build was in his window, from when), a topic tied when a thread opens
//! (his to decide).

use std::collections::HashMap;

pub mod brain;
pub mod camera;
pub mod canvas;
pub mod causal;
pub mod fold;
pub mod cloud;
pub mod cloud_place;
pub mod deep;
pub mod join;
pub mod lanes;
pub mod layout;
pub mod lod;
pub mod orbit;
pub mod ribbon;
pub mod spacetime;
pub mod topic;
#[cfg(test)]
mod made_up;
#[cfg(test)]
mod act_paths;
#[cfg(test)]
mod paths;

/// A rectangle in world units: its top-left and its size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> f64 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
    pub fn mid(&self) -> [f64; 2] {
        [self.x + self.w / 2.0, self.y + self.h / 2.0]
    }
    /// Whether O lies wholly inside it (its edges included).
    pub fn holds(&self, o: &Rect) -> bool {
        o.x >= self.x && o.y >= self.y && o.right() <= self.right() && o.bottom() <= self.bottom()
    }
    /// Whether it and O share any point: a node meets the view.
    pub fn meets(&self, o: &Rect) -> bool {
        self.x <= o.right() && o.x <= self.right() && self.y <= o.bottom() && o.y <= self.bottom()
    }
}

/// What a node is, by p715's levels, far to near.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// Level 1: a project, an island (research, cav-paper).
    Project,
    /// Level 2: a topic, a region of its project: an item with the threads that name it or were opened from it, else
    /// threads that share their agents (paper-research N-1479 p9: the record's links, then communities).
    Topic,
    /// Level 2: a build in the project's band, from the window-release lines.
    Build,
    /// Level 2: an account's meter, in the project's corner.
    Account,
    /// Level 2–3: an agent, a dot on the topic it works on.
    Agent,
    /// Level 3: an item (`I-…`).
    Item,
    /// Level 3: a thread, a line of the outline.
    Thread,
    /// Level 3: a handler, a kind's `when` on one event (`handler:{kind}:{event}`, "thread · when an agent posts": not
    /// by its line, which an edit of its kind moves, docket-edge5 N-1479 p211), in its project's Programs region, the
    /// steps of its runs inside it (docket-edge8's, N-1479 p17; p715 path 3 starts at it).
    Handler,
    /// Level 4: one run of a handler (`run:{run}`), labelled by what it came to (keep, deliver), its steps inside it as
    /// `run:{run}/…`, so they are not the same nodes as that run's steps inside the post it was for (docket-edge8's,
    /// N-1479 p24: path 3 comes to them through the handler, path 5 through the post).
    Run,
    /// Level 4: a post, on its thread.
    Post,
    /// Level 5: the brain's mark on a post, one per pass; its parts inside it.
    Brain,
    /// Level 5–6: a step that handled a post (a run's step, `A-….s1`), its program and template inside it.
    Step,
    /// Level 7: a call on the wire, what was sent and what came back.
    Call,
    /// The cloud's (cloud.rs): an agent's turn an answer was written in (coordd's `written_in`), inside the answer's card.
    Turn,
    /// The cloud's (cloud.rs): a text a request carried, by its size, not its words: a system prompt block (coordd's
    /// `text`) or the request's tools (`tools`), inside the card of the request (a [`Kind::Call`] on the wire), as his
    /// p1104 digs "down to the actual claude code layer" (proto-window N-1446 p1755). It holds nothing.
    Text,
    /// The cloud's (cloud.rs `focus_card`): a node of the graph's own kind KIND, none of the above (a commit, a job, a
    /// build of the record), a card of its words: the focus that is none of his posts' cards, and each node one hop
    /// from it in its slice (paper-research N-1446 p1885, p1903: any focus is a card).
    Node(docket_machine::graph::NodeKind),
}

/// A node's state, as its colour says it (p715: agents "working, idle, held on a limit, stopped"; his asks "a ring
/// while open, filled once in your window").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum State {
    /// Nothing to say: drawn in the page's own ink.
    #[default]
    Plain,
    /// It waits on him: the window's one accent.
    WaitsOnHim,
    /// At work now.
    Working,
    Idle,
    /// Held on a limit (a gateway cooling down).
    Held,
    /// Stopped, or broken.
    Stopped,
    /// Ended: done, dropped, kept.
    Ended,
}

/// Which labels go on first: lower first (p715). Within a tier, the larger weight first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tier {
    /// His ask that is still open, or a post of his.
    HisAsk,
    /// What waits on him.
    WaitsOnHim,
    /// Stopped or held.
    Stopped,
    #[default]
    Rest,
}

/// One node of the Map.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// The lens node it opens (`N-1104.p587`), else its kind's name for it (`agent:docket-edge6`): see the module's doc.
    pub id: String,
    pub kind: Kind,
    /// Its label, one line.
    pub label: String,
    /// What it is now, one line, for its card, its hover and the inspector ("waits on you", "accounts-2 · building the
    /// reset fix", "cooling until 13:31"); empty when there is nothing to say.
    pub note: String,
    /// The node it lies inside, by its index in [`Graph::nodes`]; none for a project.
    pub parent: Option<u32>,
    /// Where it lies, in world units, inside its parent's.
    pub rect: Rect,
    pub state: State,
    pub tier: Tier,
    /// The measure its size carries, the work in it (lines given, posts): the tie-break of its tier.
    pub weight: f32,
    /// His: he wrote it or opened it.
    pub his: bool,
    /// A post that asks: his, of agents, or an agent's, of him ([`Ring`]); none for every other node.
    pub ring: Option<Ring>,
    /// The side on screen, in points, from which it opens, where that is not its kind's (lod.rs `band`): a thread of
    /// the cloud's once its posts' cards would read, whatever how many it holds (cloud.rs).
    pub opens: Option<f32>,
    /// The least it is drawn at on screen, wide and high, in points, its words reading there (lod.rs `READS`): a card
    /// of the cloud's. Smaller, it is not drawn as a box with no words (paper-research N-1446 p1677, 1): it comes in as
    /// one of its parent's few heaviest, at this size, or it joins a group of its siblings that says how many they are
    /// (lod.rs `visible_with`, `Group`). None: drawn at any size, as the Map's nodes are.
    pub reads: Option<[f32; 2]>,
    /// When it came, in seconds ([`Graph::date`]): its own time, else its first child's; [`TIMELESS`], there at every
    /// moment. A moment before it does not draw it ([`Graph::there`]).
    pub born: i64,
    /// What happened to it after it came, oldest first ([`Graph::date`]): a child came into it, a link of its was made.
    /// Its history's steps: it and these.
    pub events: Vec<NodeEvent>,
}

/// A node there at every moment: no time of its own nor of anything it holds.
pub const TIMELESS: i64 = i64::MIN;

/// One step of a node's history ([`Node::events`]): at AT, WHAT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeEvent {
    pub at: i64,
    pub what: Change,
}

/// What changed a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// A node came into it (by its index).
    Child(u32),
    /// A link of the kind was made with the node (by its index).
    Linked(Link, u32),
}

impl Node {
    pub fn new(id: impl Into<String>, kind: Kind, label: impl Into<String>, parent: Option<u32>, rect: Rect) -> Node {
        Node { id: id.into(), kind, label: label.into(), note: String::new(), parent, rect, state: State::Plain, tier: Tier::Rest, weight: 0.0, his: false, ring: None, opens: None, reads: None, born: TIMELESS, events: Vec::new() }
    }

    /// Whether it changed at T: it came then, or something happened to it then.
    pub fn moved_at(&self, t: i64) -> bool {
        self.born == t || self.events.iter().any(|e| e.at == t)
    }
}

/// How a post that asks is drawn (p715 level 2 and path 1: "a ring while open, filled once in your window"): a ring,
/// apart from the dot of a post that asks nothing. Two marks from two readings, kept apart (proto-window N-1479
/// p315): his ask of agents reads (B)'s ledger (`TicketView::asked`), an agent's ask of him threads.rs
/// `open_to_owner`. The fill waits for a typed fact of what a build carried (paper-research p9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ring {
    /// His post asks agents, and one has not answered: a ring, empty, in his ink.
    Open,
    /// Each agent it asks has answered: the ring with his dot in it.
    Answered,
    /// An agent's post asks him and waits on him: the ring in the waits-on-you ink around the post's own dot. It goes
    /// once its asks close (his answer, his "nothing to answer", the asker's drop, a move, a cancel: p315).
    ToHim,
}

/// What a link says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Link {
    /// A thread names an item, or was opened from it (topics' way 1).
    Names,
    /// A sub-thread off the post it came from.
    Branch,
    /// An agent worked on it (given anything on it).
    Worked,
    /// An agent runs on an account (p715 path 2: the agents held on a limit point at it).
    Uses,
    /// The way his ask went (p715 path 1: the post → the item → the build in his window): his post to each answer's
    /// post, or its line where the post is not placed. Drawn only while one of its own ends is selected, never faded
    /// (lod.rs `edges`; consult-3 N-1479 p316 point 1: else every answered ask of his crosses the island).
    Trail,
    /// In the cloud, his post to its answers' card, or to an answer of it placed under another post of his, so two
    /// threads that share an answer are joined (paper-research N-1446 p1677, 1 and 2).
    Answered,
    /// In the cloud, his post to his next one in the same thread: the thread's order as a path (p1677, 2).
    Next,
    /// A step of a node's history to the step after it, on its unfolded rail (ribbon.rs; the owner, N-1791 p1, 2).
    History,
    /// A step of a node's history to a step that forked off it (a sub-thread from one of its posts), a row under the
    /// rail (ribbon.rs).
    Branched,
}

/// A link from one node to another, by their indices, made at WHEN (seconds; [`TIMELESS`]: there at every moment,
/// [`Graph::date`] gives it the later of its ends' births).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edge {
    pub from: u32,
    pub to: u32,
    pub link: Link,
    pub when: i64,
}

impl Edge {
    /// A link of LINK from FROM to TO, with no time of its own.
    pub fn new(from: u32, to: u32, link: Link) -> Edge {
        Edge { from, to, link, when: TIMELESS }
    }
}

/// N's first moment: its birth, or its first event.
fn first_of(n: &Node) -> Option<i64> {
    std::iter::once(n.born).chain(n.events.iter().map(|e| e.at)).filter(|t| *t != TIMELESS).min()
}

fn min_of(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// The Map's nodes and links. Built once from what the window holds; a node's children may be added later, when it is
/// first drawn near enough to show them ([`Graph::push`]), each inside its parent.
#[derive(Clone, Debug, Default)]
pub struct Graph {
    /// Added only through [`Graph::push`], so each is checked against its parent.
    nodes: Vec<Node>,
    /// Added only through [`Graph::link`], so each joins two nodes.
    edges: Vec<Edge>,
    by_id: HashMap<(u32, String), u32>,
    kids: Vec<Vec<u32>>,
    /// Each node's project.
    root: Vec<u32>,
    /// The projects, in the order they were added.
    projects: Vec<u32>,
    /// The first of [`Graph::moments`], kept as nodes come and change: read each frame by the time view.
    first: Option<i64>,
}

/// Why a node or a link was refused.
#[derive(Clone, Debug, PartialEq)]
pub enum Refused {
    /// The node does not lie inside its parent's rectangle.
    Outside { node: String, parent: String },
    /// Its parent is not added yet.
    ParentAfter { node: String },
    /// Its project has a node of that id already (a second project of the same id is refused too).
    Twice { node: String },
    /// A node with no parent that is not a project.
    Rootless { node: String },
    /// A project inside another node.
    Nested { node: String },
    /// A link from or to a node not added: the EDGE-th link given.
    NoNode { edge: usize },
}

impl Graph {
    /// NODES, each parent before its children, and EDGES between them; refused when a node does not lie inside its
    /// parent (the Map's one promise: a node keeps its place at every level, and going in finds it where it was).
    pub fn new(nodes: Vec<Node>, edges: Vec<Edge>) -> Result<Graph, Refused> {
        let mut g = Graph { nodes: Vec::with_capacity(nodes.len()), edges: Vec::new(), by_id: HashMap::new(), kids: Vec::new(), root: Vec::new(), projects: Vec::new(), first: None };
        for n in nodes {
            g.push(n)?;
        }
        for (i, e) in edges.into_iter().enumerate() {
            g.link(e).map_err(|_| Refused::NoNode { edge: i })?;
        }
        Ok(g)
    }

    /// Add N, inside its parent (a project inside nothing); its index.
    pub fn push(&mut self, n: Node) -> Result<u32, Refused> {
        let i = self.nodes.len() as u32;
        match (n.kind, n.parent) {
            (Kind::Project, Some(_)) => return Err(Refused::Nested { node: n.id }),
            (k, None) if k != Kind::Project => return Err(Refused::Rootless { node: n.id }),
            _ => {}
        }
        let root = match n.parent {
            None => i,
            Some(p) => {
                let Some(pn) = self.nodes.get(p as usize) else { return Err(Refused::ParentAfter { node: n.id }) };
                if !pn.rect.holds(&n.rect) {
                    return Err(Refused::Outside { node: n.id, parent: pn.id.clone() });
                }
                self.root[p as usize]
            }
        };
        let key = (root, n.id.clone());
        if self.by_id.contains_key(&key) || (n.parent.is_none() && self.project(&n.id).is_some()) {
            return Err(Refused::Twice { node: n.id });
        }
        match n.parent {
            Some(p) => self.kids[p as usize].push(i),
            None => self.projects.push(i),
        }
        self.by_id.insert(key, i);
        self.first = min_of(self.first, first_of(&n));
        self.nodes.push(n);
        self.kids.push(Vec::new());
        self.root.push(root);
        Ok(i)
    }

    /// Add link E, between two nodes added; refused (as the next link, `NoNode`'s EDGE) when either is not.
    pub fn link(&mut self, e: Edge) -> Result<(), Refused> {
        let n = self.nodes.len();
        if e.from as usize >= n || e.to as usize >= n {
            return Err(Refused::NoNode { edge: self.edges.len() });
        }
        self.edges.push(e);
        Ok(())
    }

    /// The nodes, by index.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// The links.
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// The project whose node is named ID (`project:research`). Any other node is found in its own project:
    /// [`Graph::find_in`] (two islands' N-12 are two nodes; consult-3 N-1479 p16, p39).
    pub fn project(&self, id: &str) -> Option<u32> {
        self.roots().find(|r| self.nodes[*r as usize].id == id)
    }

    /// The node named ID in project ROOT.
    pub fn find_in(&self, root: u32, id: &str) -> Option<u32> {
        self.by_id.get(&(root, id.to_string())).copied()
    }

    /// I's project.
    pub fn root_of(&self, i: u32) -> u32 {
        self.root[i as usize]
    }

    pub fn node(&self, i: u32) -> &Node {
        &self.nodes[i as usize]
    }

    /// Change what node I says once it is placed (its label, note, state, tier, weight, his) through F; its id, kind,
    /// parent and place stay as they were checked when it was added.
    pub fn update(&mut self, i: u32, f: impl FnOnce(&mut Node)) {
        let n = &mut self.nodes[i as usize];
        let (id, kind, parent, rect) = (n.id.clone(), n.kind, n.parent, n.rect);
        let was = first_of(n);
        f(n);
        (n.id, n.kind, n.parent, n.rect) = (id, kind, parent, rect);
        let now = first_of(n);
        // the first moment may have been this node's and gone later
        self.first = if was.is_some() && was == self.first && now != was { self.nodes.iter().fold(None, |m, n| min_of(m, first_of(n))) } else { min_of(self.first, now) };
    }

    /// The nodes inside I, in the order they were added.
    pub fn children(&self, i: u32) -> &[u32] {
        &self.kids[i as usize]
    }

    /// The projects, in the order they were added.
    pub fn roots(&self) -> impl Iterator<Item = u32> + '_ {
        self.projects.iter().copied()
    }

    /// Each node's and link's time, once the graph is whole: a node's own (OWN, by its id), else its first child's; a
    /// link's own, else the later of its ends'. Each node's events: its children's births after its own, and its links
    /// made after it came, oldest first. Children are added after their parents ([`Graph::push`]), so one pass up from
    /// the last gives every node its children's first.
    pub fn date(&mut self, own: impl Fn(&str) -> Option<i64>) {
        for n in &mut self.nodes {
            n.born = own(&n.id).unwrap_or(TIMELESS);
            n.events.clear();
        }
        for i in (0..self.nodes.len()).rev() {
            let (b, p) = (self.nodes[i].born, self.nodes[i].parent);
            if let Some(p) = p.map(|p| p as usize)
                && b != TIMELESS
                && own(&self.nodes[p].id).is_none()
                && (self.nodes[p].born == TIMELESS || b < self.nodes[p].born)
            {
                self.nodes[p].born = b;
            }
        }
        for i in 0..self.nodes.len() {
            if let Some(p) = self.nodes[i].parent.map(|p| p as usize) {
                let b = self.nodes[i].born;
                if b != TIMELESS && b > self.nodes[p].born {
                    self.nodes[p].events.push(NodeEvent { at: b, what: Change::Child(i as u32) });
                }
            }
        }
        for e in &mut self.edges {
            if e.when == TIMELESS {
                e.when = self.nodes[e.from as usize].born.max(self.nodes[e.to as usize].born);
            }
            for (a, b) in [(e.from, e.to), (e.to, e.from)] {
                let n = &mut self.nodes[a as usize];
                if e.when != TIMELESS && e.when > n.born {
                    n.events.push(NodeEvent { at: e.when, what: Change::Linked(e.link, b) });
                }
            }
        }
        for n in &mut self.nodes {
            n.events.sort_by_key(|e| e.at);
        }
        self.first = self.nodes.iter().fold(None, |m, n| min_of(m, first_of(n)));
    }

    /// The first moment the graph changed at (the first of [`Graph::moments`]), in constant time; None: it has none.
    pub fn origin(&self) -> Option<i64> {
        self.first
    }

    /// Whether node I is there at moment AT (None: now): it came by then.
    pub fn there(&self, i: u32, at: Option<i64>) -> bool {
        at.is_none_or(|t| self.nodes[i as usize].born <= t)
    }

    /// Whether link E is there at moment AT (None: now): made by then, and both its ends there.
    pub fn linked(&self, e: &Edge, at: Option<i64>) -> bool {
        at.is_none_or(|t| e.when <= t) && self.there(e.from, at) && self.there(e.to, at)
    }

    /// The moments the graph changed at, oldest first, each once: every node's birth and every event.
    pub fn moments(&self) -> Vec<i64> {
        let mut t: Vec<i64> = self.nodes.iter().flat_map(|n| std::iter::once(n.born).chain(n.events.iter().map(|e| e.at))).filter(|t| *t != TIMELESS).collect();
        t.sort_unstable();
        t.dedup();
        t
    }

    /// I's chain of parents, its project first and I last: the crumb trail.
    pub fn path(&self, i: u32) -> Vec<u32> {
        let mut p = vec![i];
        let mut at = i;
        while let Some(up) = self.nodes[at as usize].parent {
            p.push(up);
            at = up;
        }
        p.reverse();
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(id: &str, kind: Kind, parent: Option<u32>, r: Rect) -> Node {
        Node::new(id, kind, id, parent, r)
    }

    #[test]
    fn a_node_lies_inside_its_parent_and_its_path_is_the_crumb_trail() {
        let g = Graph::new(
            vec![
                n("project:research", Kind::Project, None, Rect::new(0.0, 0.0, 10_000.0, 8_000.0)),
                n("topic:I-1469", Kind::Topic, Some(0), Rect::new(100.0, 100.0, 2_000.0, 1_500.0)),
                n("I-1469", Kind::Item, Some(1), Rect::new(120.0, 120.0, 200.0, 120.0)),
                n("N-1470", Kind::Thread, Some(1), Rect::new(400.0, 120.0, 200.0, 300.0)),
                n("N-1470.p3", Kind::Post, Some(3), Rect::new(405.0, 130.0, 190.0, 12.0)),
            ],
            vec![Edge::new(3, 2, Link::Names)],
        )
        .unwrap();
        let p = g.find_in(0, "N-1470.p3").unwrap();
        let trail: Vec<&str> = g.path(p).iter().map(|i| g.node(*i).id.as_str()).collect();
        assert_eq!(trail, ["project:research", "topic:I-1469", "N-1470", "N-1470.p3"]);
        assert_eq!(g.children(1), [2, 3]);
        assert_eq!(g.roots().collect::<Vec<_>>(), [0]);
    }

    #[test]
    fn a_node_out_of_its_parent_or_named_twice_is_refused() {
        let project = n("project:research", Kind::Project, None, Rect::new(0.0, 0.0, 100.0, 100.0));
        let out = n("N-1", Kind::Thread, Some(0), Rect::new(90.0, 90.0, 20.0, 5.0));
        assert_eq!(Graph::new(vec![project.clone(), out], vec![]).unwrap_err(), Refused::Outside { node: "N-1".into(), parent: "project:research".into() });
        let twice = n("project:research", Kind::Project, None, Rect::new(0.0, 0.0, 1.0, 1.0));
        assert_eq!(Graph::new(vec![project.clone(), twice], vec![]).unwrap_err(), Refused::Twice { node: "project:research".into() });
        // a project's N-1 and another's are two nodes
        let cav = n("project:cav-paper", Kind::Project, None, Rect::new(200.0, 0.0, 100.0, 100.0));
        let (a, b) = (n("N-1", Kind::Thread, Some(0), Rect::new(1.0, 1.0, 5.0, 5.0)), n("N-1", Kind::Thread, Some(1), Rect::new(201.0, 1.0, 5.0, 5.0)));
        let g = Graph::new(vec![project.clone(), cav, a, b], vec![]).unwrap();
        assert_eq!((g.find_in(0, "N-1"), g.find_in(1, "N-1"), g.root_of(3), g.project("project:cav-paper"), g.project("N-1")), (Some(2), Some(3), 1, Some(1), None));
        let before = n("N-2", Kind::Thread, Some(5), Rect::new(1.0, 1.0, 1.0, 1.0));
        assert_eq!(Graph::new(vec![project.clone(), before], vec![]).unwrap_err(), Refused::ParentAfter { node: "N-2".into() });
        assert_eq!(Graph::new(vec![project.clone()], vec![Edge::new(0, 3, Link::Worked)]).unwrap_err(), Refused::NoNode { edge: 0 });
        // a thread inside nothing, a project inside a project
        let rootless = n("N-3", Kind::Thread, None, Rect::new(1.0, 1.0, 1.0, 1.0));
        assert_eq!(Graph::new(vec![project.clone(), rootless], vec![]).unwrap_err(), Refused::Rootless { node: "N-3".into() });
        let nested = n("project:inner", Kind::Project, Some(0), Rect::new(1.0, 1.0, 1.0, 1.0));
        assert_eq!(Graph::new(vec![project.clone(), nested], vec![]).unwrap_err(), Refused::Nested { node: "project:inner".into() });
        // a link added later is checked as one given at first
        let mut g = Graph::new(vec![project, n("N-4", Kind::Thread, Some(0), Rect::new(1.0, 1.0, 1.0, 1.0))], vec![]).unwrap();
        assert_eq!(g.link(Edge::new(1, 0, Link::Names)), Ok(()));
        assert_eq!(g.link(Edge::new(1, 2, Link::Names)), Err(Refused::NoNode { edge: 1 }));
        assert_eq!(g.edges().len(), 1);
        // an update changes what it says, never where it is
        g.update(1, |n| {
            n.state = State::Working;
            n.rect = Rect::new(500.0, 500.0, 1.0, 1.0);
            n.id = "N-5".into();
        });
        assert_eq!((g.node(1).state, g.node(1).rect, g.node(1).id.as_str()), (State::Working, Rect::new(1.0, 1.0, 1.0, 1.0), "N-4"));
    }
}
