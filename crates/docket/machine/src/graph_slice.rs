//! The one graph as the window asks for it (paper-research N-1446 p1376, part 5; proto-window p1383, head 1): the
//! layers he switches on, like his "google earth layers" (the owner, N-1104 p767: "you can filter by what stuff you
//! want to see … searching filtering is then … filtering these items or edges"), and a part of it around a node, by
//! who and when, as a [`Slice`]. What a node or an edge is stays [`crate::graph`]'s; which layer it is in is a way of
//! showing it, so it is here, beside the graph, and asked of it, never stored in it (p758, p760).
//!
//! - **The layers** ([`Layer`]): his messages (his posts and every edge at one), agent to agent (the agents' posts
//!   and theirs), deliveries (what woke an agent, what it was handed, its turns), work (commits, builds and his
//!   window, a read of a head), calls and prompts (steps, the calls on the wire, the `.call` files, templates,
//!   programs and handlers), files (an agent's edits, a forming call's kept file). An edge is in one layer
//!   ([`Layer::of`]); a node is in the layers of its edges.
//! - **Around a node** ([`Ask::around`]): its edges, then theirs, HOPS out, along the layers on ([`Graph::walk_by`]);
//!   past an agent or a ticket the walk counts what is there but goes no further (paper-research p1366: else two hops from any post are
//!   everything he wrote and everything on its thread, edge6's run2: 2017 of p745's 2019 edges), but a run reached
//!   from the post it handled (`handled_by`) it goes into: the run's steps, their calls and its posts (p1677). The
//!   same run reached from its thread (`ran`) stops it, so a thread's walk does not go into each of its runs.
//! - **Who and when**: an edge at an agent, or at a post or a turn of its; an edge whose time ([`Edge::when`]) is in
//!   the span. An edge with no time is in no span: with either end asked, it is left out.
//! - **The limit**: the nearest hops whole; in the farthest hop it reaches, each of the focus's neighbours its share,
//!   the newest first ([`nearest`], p1753; a slice of the whole graph is one hop, each edge its own: the newest first);
//!   what it left out is counted ([`Slice::left`]), never dropped unsaid.
//! - **An agent's folder** ([`Ask::folder`], [`Graph::folder`]; paper-research p1441, p1530): its starts, its loaded
//!   texts by version, what it read, its sessions, and its commits and jobs with their worktrees, as [`Row`]s, each
//!   saying where it comes from; counted from the whole graph, so no limit cuts it.
//!
//! A slice's nodes and edges are the graph's own ([`Node`], [`Edge`]: where each comes from and when), with what the
//! window needs beside them: an edge's layer, a post's writer.
//!
//! Not here: words (a post's text is in session files, which coordd's find reads off the lock), and labels (coordd's
//! `labels`, the one source of what an id reads as).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::graph::{Edge, EdgeKind, Graph, Node, NodeKind, Source};
use crate::line::At;

/// A layer he switches on: what its nodes and edges are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    /// His posts, and every edge at one: what he wrote, where, what answered it, whom it named.
    His,
    /// The agents' posts to each other, and theirs.
    Agents,
    /// What woke or informed an agent, what it was handed (or the slice of its session that took it), and its turns.
    Deliveries,
    /// Commits, their sessions and merges, the runner's jobs, builds and his window, a read of a head; an item noted on
    /// a post or a ticket, and an agent's start of an item (how his ask is taken, docket-edge5 21fed2f4).
    Work,
    /// Steps, the calls on the wire, their `.call` files and templates, programs and the handlers they ran; a session's
    /// requests, the system prompt and tools they carried and the posts they were for.
    Calls,
    /// An agent's edits of a file, a forming call's kept prompt and reply; a text that stood as a file.
    Files,
    /// A layer this build does not know: a newer coordd's, as this build decodes it (`#[serde(other)]`; paper-research
    /// N-1446 p1574). It has no switch ([`Layer::ALL`] leaves it out), so no lane draws its edges; and an edge of a kind
    /// this build does not know.
    #[serde(other)]
    Other,
}

impl Layer {
    pub const ALL: [Layer; 6] = [Layer::His, Layer::Agents, Layer::Deliveries, Layer::Work, Layer::Calls, Layer::Files];

    /// The layer edge E is in, in G whose posts' writers are WROTE (post → its writer's name). An edge at a commit, a
    /// session, a job, a build or a branch is work, whatever its kind: git's and the runner's (`graph_git`: an agent
    /// wrote the commit, a job ran on it, a build came from the one before, a branch was made on it).
    pub fn of(e: &Edge, g: &Graph, wrote: &HashMap<&str, &str>) -> Layer {
        let end = |id: &str| (g.get(id).map(|n| n.kind), wrote.get(id).is_some_and(|w| *w == OWNER));
        Layer::by(e, end(&e.from), end(&e.to))
    }

    /// [`Layer::of`] from what it reads of edge E's two ends, FROM and TO: each one's node kind (none: the graph has no
    /// node of that id) and whether he is its writer. The graph's kept index keeps each edge's layer so, beside the graph
    /// and never in it (`graph_kept`; docket-edge5 N-1446 p2532).
    pub(crate) fn by(e: &Edge, from: (Option<NodeKind>, bool), to: (Option<NodeKind>, bool)) -> Layer {
        let work = |k: Option<NodeKind>| matches!(k, Some(NodeKind::Commit | NodeKind::Session | NodeKind::Job | NodeKind::Build | NodeKind::Branch));
        match e.kind {
            _ if work(from.0) || work(to.0) => Layer::Work,
            EdgeKind::Wrote if to.0 == Some(NodeKind::File) => Layer::Files,
            // his ask → whom the ASKED ledger says it asks, and → the answer that closed it there, his as the ask is
            EdgeKind::Wrote | EdgeKind::PostedIn | EdgeKind::Answered | EdgeKind::AnsweredUpto | EdgeKind::Named | EdgeKind::Asks | EdgeKind::ClosedBy => {
                if from.1 || to.1 || e.from == crate::graph::agent_id(OWNER) {
                    Layer::His
                } else {
                    Layer::Agents
                }
            }
            // a delivery that began a turn, and an agent's start, beside its turns' ends; the slice of a session an agent
            // took, what it was told without being woken, and the turn a post was written in
            EdgeKind::Woke | EdgeKind::Handed | EdgeKind::TurnOf | EdgeKind::Prompted | EdgeKind::StartOf | EdgeKind::Took | EdgeKind::Informed | EdgeKind::WrittenIn | EdgeKind::MadeIn => Layer::Deliveries,
            // a start's brief, beside the start (a session's is work by the first arm)
            EdgeKind::Briefed if from.0 == Some(NodeKind::Start) => Layer::Deliveries,
            EdgeKind::Read | EdgeKind::BuiltInto | EdgeKind::Installed | EdgeKind::MergedInto | EdgeKind::Started | EdgeKind::Closed | EdgeKind::Ended | EdgeKind::On | EdgeKind::In => Layer::Work,
            // an item → the post or ticket its note names (`open`'s note); a ticket → its program stays a call's
            EdgeKind::CameFrom if from.0 == Some(NodeKind::Ticket) && matches!(to.0, Some(NodeKind::Post | NodeKind::Ticket)) => Layer::Work,
            EdgeKind::Edited if to.0 == Some(NodeKind::File) => Layer::Files,
            // an agent's side (follow::side_into): a session's own edges are work by the first arm
            EdgeKind::SessionOf | EdgeKind::Loaded | EdgeKind::Briefed | EdgeKind::Sent => Layer::Work,
            // a text's next version beside the file it stood as; a request's turn and the texts it ran with beside what
            // it carried
            EdgeKind::VersionOf | EdgeKind::Became => Layer::Files,
            // his sign-off of his ask, his as the ask is
            EdgeKind::SignedOff => Layer::His,
            // his post → the run that handled it, beside the thread's `ran` of it; a start's system prompt text beside
            // what its requests carried
            EdgeKind::Ran | EdgeKind::HandledBy | EdgeKind::StepOf | EdgeKind::Called | EdgeKind::CameFrom | EdgeKind::Edited | EdgeKind::Carried | EdgeKind::For | EdgeKind::SentIn | EdgeKind::RanWith | EdgeKind::StartedWith => Layer::Calls,
            EdgeKind::Other => Layer::Other,
        }
    }
}

/// The owner, as the record names him.
const OWNER: &str = "owner";

/// What the window asks of the graph. Empty LAYERS: all of them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ask {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub layers: BTreeSet<Layer>,
    /// The node it is around (`N-1104.p678`), with HOPS: none, the whole graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub around: Option<String>,
    #[serde(default)]
    pub hops: u32,
    /// Agents by name: an edge at one, or at a post or a turn of one. None: everyone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub who: Vec<String>,
    /// Items and threads by id (`I-1469`): an edge at one, or at a post of one, as `who` is at an agent. None: every one
    /// (proto-window N-1446 p1383, head 2: "search and filter by kind, who, when and item").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
    /// The span its edges happened in, either end open (record times, `2026-11-21T08:19:19Z`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<At>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<At>,
    /// The most edges it gives, the nearest hops first ([`nearest`]); 0: [`LIMIT`].
    #[serde(default)]
    pub limit: usize,
    /// An agent by its name: its folder ([`Graph::folder`]; the file view, paper-research N-1446 p1441), given alone,
    /// with no slice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// With `folder`, a session of that agent's by its node (`session:ID`): its requests alone
    /// ([`Graph::session_requests`]; paper-research N-1446 p1642, 3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// An agent by its name: its system prompt ([`Graph::prompt`]; fix 4, the node his System prompt press opens,
    /// docket-edge6 N-1446 p2496, read right p2504), given alone, with no slice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// With `prompt`, one of its versions by its text node (`text:SHA12`, or [`NOT_SAID`]): the requests that carried
    /// it alone, under their turns under their steps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// With what is worked on now and what is not, the whole graph's ([`Graph::now`], coordd's `GraphAnswer::now`): the
    /// canvas's right edge (paper-research N-1446 p1385: "The right edge of the canvas is now").
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub now: bool,
}

/// The edges a slice gives when the ask names no limit.
pub const LIMIT: usize = 2000;

/// A node of a slice: the graph's, and a post's writer, a turn's agent, or the agent whose session a slice is (`took`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SliceNode {
    #[serde(flatten)]
    pub node: Node,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
}

/// An edge of a slice: the graph's, and its layer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SliceEdge {
    #[serde(flatten)]
    pub edge: Edge,
    pub layer: Layer,
}

/// What the graph gives for an [`Ask`]: the edges that answer it and the nodes at their ends, oldest first, and how
/// many edges each layer has in the whole graph and in the slice.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slice {
    pub nodes: Vec<SliceNode>,
    pub edges: Vec<SliceEdge>,
    /// Each layer's edges in the whole graph.
    pub whole: BTreeMap<Layer, usize>,
    /// Each layer's edges in this slice.
    pub shown: BTreeMap<Layer, usize>,
    /// The edges that answered the ask but the limit left out.
    pub left: usize,
    /// Every edge that answered the ask, the limit's left out among them, by layer and the hour it happened in
    /// (`2026-11-21T08`, a record time's first 13 characters): the canvas's summary far out, where an edge each is too
    /// many (paper-research N-1446 p1385: "Far out, the counts per hour per lane stay as the summary"). An edge with no
    /// time is in no hour, and only in `whole`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hours: BTreeMap<Layer, BTreeMap<String, usize>>,
    /// The node it is around, when the graph has none of that id.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unknown: bool,
}

impl Graph {
    /// The part of the graph that answers ASK, each edge's layer, each layer's count and each node's writer read off the
    /// kept index (`graph_kept`), so an ask of some layers reads only their edges and works out no edge's layer: his
    /// layer's ask held coordd's lock 0.57 s on research's record of 24 Nov, 0.39 s of it the slice working out the
    /// layers of all 626k edges to keep 7,477 (docket-edge5 N-1446 p2532). [`Graph::slice_walked`] gives the same from
    /// one pass over every edge, as each slice once did.
    pub fn slice(&self, ask: &Ask) -> Slice {
        let p = self.parts();
        let mut k = self.kept.lock();
        k.catch_up(&p);
        let k = &*k;
        self.slice_with(ask, &|i| k.layer(i), &|id| k.writer(id), k.layer_counts())
    }

    /// [`Graph::slice`] from one pass over every edge: each node's writer, then each edge's layer ([`Layer::of`]) and
    /// each layer's count worked out again. The kept read's reference, which it equals (tests/kept.rs).
    pub fn slice_walked(&self, ask: &Ask) -> Slice {
        let wrote: HashMap<&str, &str> = self.edges.iter().filter(|e| matches!(e.kind, EdgeKind::Wrote | EdgeKind::TurnOf | EdgeKind::Took)).filter_map(|e| {
            let (agent, of) = if e.kind == EdgeKind::TurnOf { (&e.to, &e.from) } else { (&e.from, &e.to) };
            agent.strip_prefix("agent:").map(|a| (of.as_str(), a))
        }).collect();
        let layer: Vec<Layer> = self.edges.iter().map(|e| Layer::of(e, self, &wrote)).collect();
        let mut whole = BTreeMap::new();
        for l in &layer {
            *whole.entry(*l).or_insert(0) += 1;
        }
        self.slice_with(ask, &|i| layer[i], &|id| wrote.get(id).copied(), whole)
    }

    /// The part of the graph that answers ASK, edge I's layer LAYER(I), a node's writer WROTE(ID), each layer's count
    /// in the whole graph WHOLE.
    fn slice_with<'w>(&self, ask: &Ask, layer: &dyn Fn(usize) -> Layer, wrote: &dyn Fn(&str) -> Option<&'w str>, whole: BTreeMap<Layer, usize>) -> Slice {
        let on = |l: Layer| ask.layers.is_empty() || ask.layers.contains(&l);
        let unknown = ask.around.as_ref().is_some_and(|a| self.get(a).is_none());
        // the edges in reach, each with its hop and its branch: all (each at hop 0, its own branch), or a walk around the
        // node along the layers on that stops at agents and tickets, a run reached from the post it handled aside
        let reach: Vec<(usize, usize, usize)> = match &ask.around {
            None => (0..self.edges.len()).filter(|&i| on(layer(i))).map(|i| (0, i, i)).collect(),
            Some(from) => {
                let hub = |n: &Node, by: &Edge| match n.kind {
                    NodeKind::Agent => true,
                    NodeKind::Ticket => !(by.kind == EdgeKind::HandledBy && by.to == n.id),
                    _ => false,
                };
                self.walk_by(from, ask.hops.max(1) as usize, &|i| on(layer(i)), &hub)
            }
        };
        let who = |e: &Edge| {
            ask.who.is_empty()
                || [&e.from, &e.to].iter().any(|id| ask.who.iter().any(|a| id.strip_prefix("agent:") == Some(a.as_str()) || wrote(id.as_str()) == Some(a.as_str())))
        };
        let item = |e: &Edge| ask.items.is_empty() || [&e.from, &e.to].iter().any(|id| ask.items.iter().any(|t| id.as_str() == t || crate::post_ref(id).is_some_and(|(tk, _)| tk == t)));
        let when = |e: &Edge| match e.when.as_deref() {
            Some(at) => ask.from.as_deref().is_none_or(|f| at >= f) && ask.to.as_deref().is_none_or(|t| at <= t),
            None => ask.from.is_none() && ask.to.is_none(),
        };
        let mut hits: Vec<(usize, usize, usize)> = reach.into_iter().filter(|&(_, i, _)| who(&self.edges[i]) && item(&self.edges[i]) && when(&self.edges[i])).collect();
        // oldest first, by time, then where each comes from (the record's lines in their order)
        hits.sort_by(|&(_, a, _), &(_, b, _)| (&self.edges[a].when, &self.edges[a].source, a).cmp(&(&self.edges[b].when, &self.edges[b].source, b)));
        let limit = if ask.limit == 0 { LIMIT } else { ask.limit };
        let left = hits.len().saturating_sub(limit);
        let mut hours: BTreeMap<Layer, BTreeMap<String, usize>> = BTreeMap::new();
        for &(_, i, _) in &hits {
            if let Some(hour) = self.edges[i].when.as_deref().and_then(|w| w.get(..13)) {
                *hours.entry(layer(i)).or_default().entry(hour.to_string()).or_insert(0) += 1;
            }
        }
        let hits: Vec<usize> = nearest(&hits, hits.len() - left).into_iter().map(|k| hits[k].1).collect();
        let mut shown = BTreeMap::new();
        let mut ids = BTreeSet::new();
        let edges: Vec<SliceEdge> = hits
            .iter()
            .map(|&i| {
                let e = &self.edges[i];
                *shown.entry(layer(i)).or_insert(0) += 1;
                ids.insert(e.from.as_str());
                ids.insert(e.to.as_str());
                SliceEdge { edge: e.clone(), layer: layer(i) }
            })
            .collect();
        if let Some(a) = ask.around.as_deref().filter(|_| !unknown) {
            ids.insert(a);
        }
        let mut nodes: Vec<SliceNode> = ids
            .into_iter()
            .filter_map(|id| self.get(id))
            .map(|n| SliceNode { node: n.clone(), by: wrote(n.id.as_str()).map(str::to_string) })
            .collect();
        nodes.sort_by(|a, b| (&a.node.when, &a.node.source, &a.node.id).cmp(&(&b.node.when, &b.node.source, &b.node.id)));
        Slice { nodes, edges, whole, shown, left, hours, unknown }
    }
}

/// Which of HITS (each its hop, its edge and its branch, oldest first) the limit keeps, ROOM of them, by their places in
/// HITS, oldest first: the nearest hops whole, so what is near the focus is never cut for what is far but new
/// (paper-research N-1446 p1753; around his post p815 the newest 6000 were its neighbours' requests and none of its own
/// run, proto-window p1752). In the farthest hop it reaches, each branch (the edge at the focus it was walked from,
/// [`Graph::walk_by`]) its share: those with fewer whole, the rest at one level and one more each for those with the
/// newest edges while there is room, the newest first within each; so one neighbour's many newer edges do not cut
/// another's few. Around p815 on research's record of 22 Nov 21:07Z, at three hops and 6000: hops 1 and 2 are 440
/// edges, hop 3 302190, 247652 of them one request's, through the system prompt block every request shares; the
/// newest first within hop 3 cut p815's step's call file, its prompt template, which its run's share keeps. A slice of
/// the whole graph is one hop, each edge its own branch: the newest first.
fn nearest(hits: &[(usize, usize, usize)], room: usize) -> Vec<usize> {
    let mut by_hop: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (k, h) in hits.iter().enumerate() {
        by_hop.entry(h.0).or_default().push(k);
    }
    let (mut kept, mut room) = (Vec::new(), room);
    for ks in by_hop.into_values() {
        if ks.len() <= room {
            room -= ks.len();
            kept.extend(ks);
            continue;
        }
        // the hop it cuts: each branch's edges, the newest first
        let mut branches: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for &k in ks.iter().rev() {
            branches.entry(hits[k].2).or_default().push(k);
        }
        // the level: the fewest whole while what is left shares the rest evenly; the branches above it are cut to it
        let mut sizes: Vec<usize> = branches.values().map(Vec::len).collect();
        sizes.sort_unstable();
        let (mut level, mut free) = (0, room);
        for (j, &n) in sizes.iter().enumerate() {
            let share = free / (sizes.len() - j);
            if n > share {
                level = share;
                break;
            }
            free -= n;
        }
        let mut cut: Vec<&Vec<usize>> = branches.values().filter(|b| b.len() > level).collect();
        let extra = room - branches.values().map(|b| b.len().min(level)).sum::<usize>();
        cut.sort_by_key(|b| std::cmp::Reverse(b[0]));
        for b in branches.values() {
            kept.extend(b.iter().take(level));
        }
        kept.extend(cut.iter().take(extra).map(|b| b[level]));
        break;
    }
    kept.sort_unstable();
    kept
}

/// A row of an agent's folder ([`Graph::folder`]; the file view, paper-research N-1446 p1441, plan p1529): what it
/// says; the node it is of, which a press opens; where that comes from and when, as the graph says; the words that
/// changed into it (a `became` edge's), behind a press; and the rows under it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    pub text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub of: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<At>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub words: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<Row>,
}

impl Row {
    /// A row of node ID saying TEXT, from where G says ID comes from, at its time.
    fn of_node(g: &Graph, id: &str, text: String) -> Row {
        let n = g.get(id);
        Row { text, of: id.to_string(), source: n.map(|n| n.source.clone()), when: n.and_then(|n| n.when.clone()), ..Default::default() }
    }

    /// A row of node ID saying TEXT, from where edge E comes from, at its time.
    fn of_edge(e: &Edge, id: &str, text: String) -> Row {
        Row { text, of: id.to_string(), source: Some(e.source.clone()), when: e.when.clone(), ..Default::default() }
    }

    /// A row saying TEXT, holding ROWS.
    fn group(text: String, rows: Vec<Row>) -> Row {
        Row { text, rows, ..Default::default() }
    }
}

/// A request's model and effort as its node says them (`claude-opus-5-5 high`), as a folder's row writes them.
fn effort_label(how: &str) -> String {
    match how.rsplit_once(' ') {
        Some((model, effort)) => format!("{model}, effort {effort}"),
        None if how.is_empty() => "model not said".to_string(),
        None => how.to_string(),
    }
}

/// A session's requests ([`Graph::session_requests`]): one row for each request SENT (the session's `sent` edges, in
/// time, each once), saying when, its model and effort, its tokens, and each system prompt block of CARRIED (the
/// folder's `carried` edges) by its number among BLOCKS, the session's blocks as its folder numbers them.
fn session_rows(g: &Graph, sent: &[&Edge], carried: &[&Edge], blocks: &[Row]) -> Vec<Row> {
    let mut number: HashMap<&str, (usize, usize)> = HashMap::new();
    for (k, b) in blocks.iter().enumerate() {
        for (v, r) in b.rows.iter().enumerate() {
            number.entry(r.of.as_str()).or_insert((k + 1, v + 1));
        }
    }
    let mut by_request: HashMap<&str, Vec<&str>> = HashMap::new();
    for c in carried {
        by_request.entry(c.from.as_str()).or_default().push(c.to.as_str());
    }
    sent.iter()
        .map(|e| {
            let mut placed: Vec<(usize, usize)> = by_request.get(e.to.as_str()).into_iter().flatten().filter_map(|t| number.get(t).copied()).collect();
            placed.sort();
            placed.dedup();
            let carried = if placed.is_empty() { "no system prompt block".to_string() } else { placed.iter().map(|(k, v)| format!("block {k} version {v}")).collect::<Vec<_>>().join(", ") };
            Row::of_edge(e, &e.to, format!("{}; carried {carried}", request_text(g, e)))
        })
        .collect()
}

/// A request's row's words ([`Graph::session_requests`], [`Graph::prompt`]): when E (its `sent` edge) sent it, its
/// model and effort, and its tokens.
fn request_text(g: &Graph, e: &Edge) -> String {
    let n = g.get(&e.to);
    let label = effort_label(n.map_or("", |n| n.what.trim()));
    let tokens = match n.and_then(|n| n.tokens) {
        Some(t) => format!("{} prompt tokens, {} out", t.prompt(), t.output),
        None => "no tokens: the session holds no response to it".to_string(),
    };
    format!("{}: {label}; {tokens}", e.when.as_deref().unwrap_or("?"))
}

/// The node of an agent's started-with version that no start line names ([`Graph::prompt`]): the requests before its
/// first start line, and those of a start that says none (a record from before `system_sha`), the standing text's.
/// No text's id: those are hex.
pub const NOT_SAID: &str = "text:not-said";

/// What REQUESTS add up to, each once, as his System prompt node counts a version (accounts-lead N-1446 p2504:
/// "carried in 212 requests, 31 turns, 9 steps"): the turns TURN says they were sent in, and the steps STEP says they
/// worked for, each step by the posts handed in it.
fn counted(requests: &BTreeSet<&str>, turn: &HashMap<&str, &str>, step: &HashMap<&str, BTreeSet<&str>>) -> String {
    let turns: BTreeSet<&str> = requests.iter().filter_map(|r| turn.get(r).copied()).collect();
    let steps: BTreeSet<&BTreeSet<&str>> = requests.iter().filter_map(|r| step.get(r)).collect();
    format!("carried in {}, {}, {}", many(requests.len(), "request", "requests"), many(turns.len(), "turn", "turns"), many(steps.len(), "step", "steps"))
}

/// REQUESTS (their `sent` edges, in time) under their turns (TURN) under their steps (STEP, each by the posts handed
/// in it), each step and each turn where its first request was sent; a request in no step, or in no turn yet, under a
/// row that says so ([`Graph::prompt`] with a version).
fn by_step(g: &Graph, requests: &[&Edge], turn: &HashMap<&str, &str>, step: &HashMap<&str, BTreeSet<&str>>) -> Vec<Row> {
    let none = BTreeSet::new();
    let mut at: HashMap<&BTreeSet<&str>, usize> = HashMap::new();
    let mut steps: Vec<(&BTreeSet<&str>, Vec<(Option<&str>, Vec<&Edge>)>)> = Vec::new();
    for &e in requests {
        let s = step.get(e.to.as_str()).unwrap_or(&none);
        let k = *at.entry(s).or_insert_with(|| {
            steps.push((s, Vec::new()));
            steps.len() - 1
        });
        let t = turn.get(e.to.as_str()).copied();
        let turns = &mut steps[k].1;
        match turns.iter_mut().find(|(x, _)| *x == t) {
            Some((_, es)) => es.push(e),
            None => turns.push((t, vec![e])),
        }
    }
    steps
        .into_iter()
        .map(|(posts, turns)| {
            let n: usize = turns.iter().map(|(_, es)| es.len()).sum();
            let ended = turns.iter().filter(|(t, _)| t.is_some()).count();
            let rows: Vec<Row> = turns
                .into_iter()
                .map(|(t, es)| {
                    let rows: Vec<Row> = es.iter().map(|e| Row::of_edge(e, &e.to, request_text(g, e))).collect();
                    let text = many(rows.len(), "request", "requests");
                    let mut r = match t {
                        Some(t) => Row::of_node(g, t, format!("turn ended {}: {text}", g.get(t).and_then(|n| n.when.as_deref()).unwrap_or("?"))),
                        None => Row { text: format!("no turn end yet: {text}"), ..Default::default() },
                    };
                    r.rows = rows;
                    r
                })
                .collect();
            let count = if ended == 0 { many(n, "request", "requests") } else { format!("{} in {}", many(n, "request", "requests"), many(ended, "turn", "turns")) };
            let mut r = match posts.first() {
                Some(p) => Row { text: format!("step for {}: {count}", posts.iter().copied().collect::<Vec<_>>().join(", ")), of: p.to_string(), ..Default::default() },
                None => Row { text: format!("no step: {count}"), ..Default::default() },
            };
            r.rows = rows;
            r
        })
        .collect()
}

/// N of ONE or MORE: `1 request`, `3 requests`.
fn many(n: usize, one: &str, more: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {more}") }
}

/// The head of a `became` edge's words (coordd's `follow::difference`): how many words were removed and added, and
/// whether they were too many to compare, before the words themselves; words with no count before them, whole (`the
/// same words: …`). The window marks the words after it (yaaihh sidebar.rs `words_marked`).
pub fn words_head(what: &str) -> &str {
    if !what.starts_with(|c: char| c.is_ascii_digit()) {
        return what;
    }
    let end = match what.find(" (too many") {
        Some(i) => what[i..].find("):").map(|j| i + j + 1),
        None => what.find(':'),
    };
    end.map_or(what, |k| &what[..k])
}

/// ES in the order they happened: by time, then by where they come from.
fn in_time(es: &mut [&Edge]) {
    es.sort_by(|x, y| (&x.when, &x.source).cmp(&(&y.when, &y.source)));
}

/// TEXTS, in the order they first stood, as versions, each with the `became` edge into it: from the earliest along
/// BECAME (in time order) to each next version, a text again where it came back; a text no edge reaches (first
/// stood in another session) starts again, with none.
fn versions<'a>(texts: &[&'a str], became: &[&'a Edge]) -> Vec<(&'a str, Option<&'a Edge>)> {
    let (mut out, mut placed) = (Vec::new(), BTreeSet::new());
    for &t in texts {
        if !placed.insert(t) {
            continue;
        }
        out.push((t, None));
        let (mut at, mut k) = (t, 0);
        while let Some(j) = became[k..].iter().position(|b| b.from == at) {
            let b = became[k + j];
            placed.insert(b.to.as_str());
            out.push((b.to.as_str(), Some(b)));
            (at, k) = (b.to.as_str(), k + j + 1);
        }
    }
    out
}

/// The starts of the ids an agent's folder holds as its own ([`Graph::folder`]): its starts, sessions, commits and
/// jobs. It hashes only ids that start so (run 12: docket-edge5's folder took 0.465 s under coordd's lock when it
/// hashed every edge's), and reads the whole graph once, each edge's kind first (run 13: 0.15 to 0.25 s for every
/// agent, however few its rows, when it read it three times).
const OWN: [&str; 4] = ["start:", "session:", "commit:", "job:"];

impl Graph {
    /// AGENT's folder, the file view (paper-research N-1446 p1441, plan p1529, p1530): five rows in his order, each
    /// holding its own:
    /// - its starts (roster's `began`), each saying its phase, kind, model and effort, its folder and who started it,
    ///   with its brief and its session; before a start line, its sessions' briefs;
    /// - its loaded texts by file, each version with how many of its requests ran with it and the words that changed
    ///   into it in its sessions;
    /// - what it read, the most read first;
    /// - its sessions, each with its requests by model and effort and the system prompt blocks they carried, each
    ///   block version by version; each request counted once in all, though two of its session files carry it;
    /// - its commits and its jobs, the newest first, with their worktrees (`in`): a commit with none is "worktree not
    ///   known" (p1530), a job with none "worktree not said".
    ///
    /// A row of a node says where that comes from and when, as the graph does. Counted here from the whole graph, so
    /// no limit cuts it; none for an agent the graph has not.
    pub fn folder(&self, agent: &str) -> Vec<Row> {
        self.folder_of(agent, None)
    }

    /// The requests of AGENT's session SESSION (its node, `session:ID`), as its folder counts them, each a row: when it
    /// was sent, its model and effort, its tokens, and the system prompt blocks it carried by their numbers in the
    /// folder's session (`block 1 version 2`); in the order they were sent, each once. A session's request list apart
    /// (paper-research N-1446 p1642, 3); none for a session that is not the agent's.
    pub fn session_requests(&self, agent: &str, session: &str) -> Vec<Row> {
        self.folder_of(agent, Some(session))
    }

    /// AGENT's folder ([`Graph::folder`]), or with ONLY one of its sessions, that session's requests alone
    /// ([`Graph::session_requests`]).
    fn folder_of(&self, agent: &str, only: Option<&str>) -> Vec<Row> {
        let a = crate::graph::agent_id(agent);
        if self.get(&a).is_none() {
            return Vec::new();
        }
        // one pass over the whole graph, by each edge's kind, reading its ids only where that kind can matter: the
        // agent's own, in the graph's order (its starts, sessions, commits and jobs), every text's files and next
        // versions, and the edges of the kinds that can leave its own and its requests
        let (mut starts, mut sessions, mut commits, mut jobs) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut own: HashSet<&str> = HashSet::new();
        // each session's first `session_of` edge: where the record names it, its row's source
        let mut named: HashMap<&str, &Edge> = HashMap::new();
        let (mut stood, mut became): (HashMap<&str, BTreeSet<&str>>, Vec<&Edge>) = Default::default();
        let (mut leaving, mut of_requests): (Vec<&Edge>, Vec<&Edge>) = Default::default();
        for e in &self.edges {
            let (list, id) = match e.kind {
                EdgeKind::StartOf if e.to == a && e.from.starts_with(OWN[0]) => (&mut starts, &e.from),
                EdgeKind::SessionOf if e.to == a && e.from.starts_with(OWN[1]) => (&mut sessions, &e.from),
                EdgeKind::Wrote if e.from == a && e.to.starts_with(OWN[2]) => (&mut commits, &e.to),
                EdgeKind::Ran if e.from == a && e.to.starts_with(OWN[3]) => (&mut jobs, &e.to),
                EdgeKind::VersionOf => {
                    stood.entry(e.from.as_str()).or_default().insert(e.to.as_str());
                    continue;
                }
                EdgeKind::Became => {
                    became.push(e);
                    continue;
                }
                EdgeKind::Briefed | EdgeKind::CameFrom | EdgeKind::Loaded | EdgeKind::Read | EdgeKind::Sent | EdgeKind::On | EdgeKind::In | EdgeKind::Ran => {
                    leaving.push(e);
                    continue;
                }
                EdgeKind::Carried | EdgeKind::RanWith => {
                    of_requests.push(e);
                    continue;
                }
                _ => continue,
            };
            if e.kind == EdgeKind::SessionOf {
                named.entry(id.as_str()).or_insert(e);
            }
            if own.insert(id.as_str()) {
                list.push(id.as_str());
            }
        }
        // what leaves its own
        let mut out: HashMap<&str, Vec<&Edge>> = HashMap::new();
        for e in leaving {
            if OWN.iter().any(|p| e.from.starts_with(p)) && own.contains(e.from.as_str()) {
                out.entry(e.from.as_str()).or_default().push(e);
            }
        }
        in_time(&mut became);
        let from = |id: &str, k: EdgeKind| out.get(id).map(|es| es.iter().copied().filter(|e| e.kind == k).collect::<Vec<&Edge>>()).unwrap_or_default();
        let stems: HashSet<&str> = sessions.iter().map(|s| s.trim_start_matches("session:")).collect();
        let in_session = |e: &Edge, of: &dyn Fn(&str) -> bool| matches!(&e.source, Source::Session(f, _) if of(f.as_str()));
        // what leaves its requests: the texts they carried and ran with
        let requests: HashSet<&str> = sessions.iter().flat_map(|&s| from(s, EdgeKind::Sent)).map(|e| e.to.as_str()).collect();
        let (mut carried, mut ran_with): (Vec<&Edge>, HashMap<&str, BTreeSet<&str>>) = Default::default();
        for e in of_requests.into_iter().filter(|e| requests.contains(e.from.as_str())) {
            match e.kind {
                EdgeKind::Carried if self.get(&e.to).is_some_and(|n| n.kind == NodeKind::Text) => carried.push(e),
                EdgeKind::RanWith => {
                    ran_with.entry(e.to.as_str()).or_default().insert(e.from.as_str());
                }
                _ => {}
            }
        }
        in_time(&mut carried);

        // 1. its starts, else its sessions' briefs
        let mut rows = Vec::new();
        for &s in &starts {
            let mut r = Row::of_node(self, s, self.get(s).map_or(String::new(), |n| n.what.clone()));
            // a post or ticket it names; or, when it named none, the words it was started with, whole (graph.rs
            // `began`'s `prompt`; paper-research N-1446 p3041)
            r.rows.extend(from(s, EdgeKind::Briefed).into_iter().map(|e| {
                if self.get(&e.to).is_some_and(|n| n.kind == NodeKind::Text) {
                    let mut w = Row::of_edge(e, &e.to, format!("brief: the words it was started with, {} bytes", e.what.len()));
                    w.words = e.what.clone();
                    w
                } else {
                    Row::of_edge(e, &e.to, format!("brief: {}", e.to))
                }
            }));
            r.rows.extend(from(s, EdgeKind::CameFrom).into_iter().map(|e| Row::of_edge(e, &e.to, format!("session {}", e.to.trim_start_matches("session:")))));
            rows.push(r);
        }
        let head = if starts.is_empty() {
            for &s in &sessions {
                let stem = s.trim_start_matches("session:");
                rows.extend(from(s, EdgeKind::Briefed).into_iter().map(|e| Row::of_edge(e, &e.to, format!("brief of session {stem}: {}", self.get(&e.to).map_or("", |n| n.what.as_str())))));
            }
            "kind and brief: no start line of it yet, so its sessions' briefs".to_string()
        } else {
            format!("kind and brief: {}", many(starts.len(), "start", "starts"))
        };
        let starts_row = Row::group(head, rows);

        // 2. its loaded texts by file, each version where its sessions first loaded it
        let mut loads: Vec<&Edge> = sessions.iter().flat_map(|&s| from(s, EdgeKind::Loaded)).collect();
        in_time(&mut loads);
        let (mut by_file, mut loaded_at): (BTreeMap<&str, Vec<&str>>, HashMap<&str, &Edge>) = Default::default();
        for e in &loads {
            if loaded_at.contains_key(e.to.as_str()) {
                continue;
            }
            loaded_at.insert(e.to.as_str(), *e);
            for f in stood.get(e.to.as_str()).into_iter().flatten() {
                by_file.entry(*f).or_default().push(e.to.as_str());
            }
        }
        let (mut files, mut all) = (Vec::new(), 0);
        for (f, texts) in &by_file {
            let set: BTreeSet<&str> = texts.iter().copied().collect();
            let bs: Vec<&Edge> = became.iter().copied().filter(|b| in_session(*b, &|f: &str| stems.contains(f)) && set.contains(b.from.as_str()) && set.contains(b.to.as_str())).collect();
            let vs = versions(texts, &bs);
            all += vs.len();
            let mut rows = Vec::new();
            for (k, (t, b)) in vs.iter().enumerate() {
                let n = ran_with.get(t).map_or(0, BTreeSet::len);
                let text = format!("version {}: {}{}", k + 1, many(n, "request ran with it", "requests ran with it"), b.map(|b| format!("; {}", words_head(&b.what))).unwrap_or_default());
                let mut r = match (b, loaded_at.get(t)) {
                    (Some(b), _) | (None, Some(b)) => Row::of_edge(b, t, text),
                    (None, None) => Row { text, of: t.to_string(), ..Default::default() },
                };
                r.words = b.map(|b| b.what.clone()).unwrap_or_default();
                rows.push(r);
            }
            let mut r = Row::group(format!("{}: {}", f.trim_start_matches("file:"), many(vs.len(), "version", "versions")), rows);
            r.of = f.to_string();
            files.push(r);
        }
        let texts_row = Row::group(format!("loaded texts: {}, {}", many(files.len(), "file", "files"), many(all, "version", "versions")), files);

        // 3. what it read, the most read first
        let mut reads: BTreeMap<&str, Vec<&Edge>> = BTreeMap::new();
        for e in sessions.iter().flat_map(|&s| from(s, EdgeKind::Read)) {
            reads.entry(e.to.as_str()).or_default().push(e);
        }
        let total: usize = reads.values().map(Vec::len).sum();
        let mut read: Vec<(&str, Vec<&Edge>)> = reads.into_iter().collect();
        read.iter_mut().for_each(|(_, es)| in_time(es));
        read.sort_by(|x, y| y.1.len().cmp(&x.1.len()).then(x.0.cmp(y.0)));
        let rows = read
            .iter()
            .filter_map(|(f, es)| {
                // the last read's row, its source and its time (`when`), out of its words, which leave the path the
                // room in the window's row (paper-research N-1446 p1729)
                Some(Row::of_edge(es.last()?, f, format!("{}: read {}", f.trim_start_matches("file:"), many(es.len(), "time", "times"))))
            })
            .collect();
        let read_row = Row::group(format!("read: {}, {}", many(read.len(), "file", "files"), many(total, "read", "reads")), rows);

        // 4. its sessions: their requests by model and effort, and the system prompt blocks they carried
        let (mut rows, mut asked) = (Vec::new(), HashSet::new());
        for &s in &sessions {
            let stem = s.trim_start_matches("session:");
            let mut sent = from(s, EdgeKind::Sent);
            in_time(&mut sent);
            let mut seen = HashSet::new();
            sent.retain(|e| seen.insert(e.to.as_str()));
            asked.extend(sent.iter().map(|e| e.to.as_str()));
            let mut by: BTreeMap<&str, Vec<&Edge>> = BTreeMap::new();
            for e in &sent {
                by.entry(self.get(&e.to).map_or("", |n| n.what.trim())).or_default().push(*e);
            }
            // where the record names it, not where its node was first made (git's round reads a commit that came
            // from it before the side reads it: docket-edge5's row said "commit 330a5a9b", p1681)
            let text = format!("session {stem}: {}", many(sent.len(), "request", "requests"));
            let mut r = match named.get(s) {
                Some(e) => Row::of_edge(e, s, text),
                None => Row::of_node(self, s, text),
            };
            for (how, es) in &by {
                let (Some(first), Some(last)) = (es.first(), es.last()) else { continue };
                let label = effort_label(how);
                // tokens from the request nodes, each once (a request's node holds its response's; none, none came)
                let kept: Vec<_> = es.iter().filter_map(|e| self.get(&e.to).and_then(|n| n.tokens)).collect();
                let (prompt, output) = kept.iter().fold((0, 0), |(p, o), t| (p + t.prompt(), o + t.output));
                let tokens = match kept.len() {
                    0 => "no tokens: the session holds no response to them".to_string(),
                    k if k == es.len() => format!("{prompt} prompt tokens, {output} out"),
                    k => format!("{prompt} prompt tokens, {output} out, of the {k} with a response"),
                };
                let text = format!("{label}: {}, the last at {}; {tokens}", many(es.len(), "request", "requests"), last.when.as_deref().unwrap_or("?"));
                r.rows.push(Row::of_edge(first, &first.to, text));
            }
            // each block: its versions along the session's `became` edges, by the requests that carried each
            let mine = |f: &str| f == stem;
            let (mut texts, mut carriers): (Vec<&str>, HashMap<&str, (BTreeSet<&str>, &Edge)>) = Default::default();
            for e in carried.iter().copied().filter(|e| in_session(*e, &mine)) {
                carriers
                    .entry(e.to.as_str())
                    .or_insert_with(|| {
                        texts.push(e.to.as_str());
                        (BTreeSet::new(), e)
                    })
                    .0
                    .insert(e.from.as_str());
            }
            let set: BTreeSet<&str> = texts.iter().copied().collect();
            let bs: Vec<&Edge> = became.iter().copied().filter(|b| in_session(*b, &mine) && set.contains(b.from.as_str()) && set.contains(b.to.as_str())).collect();
            let mut blocks: Vec<Row> = Vec::new();
            for (t, b) in versions(&texts, &bs) {
                let Some((who, first)) = carriers.get(t) else { continue };
                if b.is_none() {
                    blocks.push(Row { of: t.to_string(), ..Default::default() });
                }
                let Some(block) = blocks.last_mut() else { continue };
                let text = format!("version {}: carried by {}{}", block.rows.len() + 1, many(who.len(), "request", "requests"), b.map(|b| format!("; {}", words_head(&b.what))).unwrap_or_default());
                let mut v = Row::of_edge(b.unwrap_or(*first), t, text);
                v.words = b.map(|b| b.what.clone()).unwrap_or_default();
                block.rows.push(v);
            }
            for (k, b) in blocks.iter_mut().enumerate() {
                b.text = format!("block {}: {}", k + 1, many(b.rows.len(), "version", "versions"));
            }
            if only == Some(s) {
                return session_rows(self, &sent, &carried, &blocks);
            }
            r.rows.push(Row::group(format!("system prompt blocks: {}", many(blocks.len(), "block", "blocks")), blocks));
            rows.push(r);
        }
        if only.is_some() {
            return Vec::new();
        }
        let sessions_row = Row::group(format!("sessions: {}, with {}", sessions.len(), many(asked.len(), "request", "requests")), rows);

        // 5. its commits and its jobs, the newest first, with their worktrees
        let newest_first = |ids: &mut Vec<&str>| ids.sort_by(|x, y| self.get(y).and_then(|n| n.when.as_ref()).cmp(&self.get(x).and_then(|n| n.when.as_ref())));
        newest_first(&mut commits);
        newest_first(&mut jobs);
        let commit_rows = commits
            .iter()
            .map(|&c| {
                let subject = self.get(c).map_or("", |n| n.what.as_str());
                let mut r = Row::of_node(self, c, format!("{} {subject}", c.trim_start_matches("commit:")).trim_end().to_string());
                let mut on = BTreeSet::new();
                r.rows.extend(from(c, EdgeKind::On).into_iter().filter(|e| on.insert(e.to.as_str())).map(|e| Row::of_edge(e, &e.to, format!("on {}", e.to.trim_start_matches("branch:")))));
                r.rows.push(match from(c, EdgeKind::In).first() {
                    Some(e) => Row::of_edge(e, &e.to, format!("made in {}", e.to.trim_start_matches("worktree:"))),
                    None => Row { text: "worktree not known".to_string(), ..Default::default() },
                });
                r
            })
            .collect();
        let job_rows = jobs
            .iter()
            .map(|&j| {
                let what = self.get(j).map_or("", |n| n.what.as_str());
                let mut r = Row::of_node(self, j, format!("job {}: {what}", j.trim_start_matches("job:")));
                r.rows.push(match from(j, EdgeKind::In).first() {
                    Some(e) => Row::of_edge(e, &e.to, format!("in {}", e.to.trim_start_matches("worktree:"))),
                    None => Row { text: "worktree not said".to_string(), ..Default::default() },
                });
                r.rows.extend(from(j, EdgeKind::Ran).into_iter().map(|e| Row::of_edge(e, &e.to, format!("on commit {}", e.to.trim_start_matches("commit:")))));
                r
            })
            .collect();
        let work_row = Row::group(
            format!("commits and jobs: {}, {}", many(commits.len(), "commit", "commits"), many(jobs.len(), "job", "jobs")),
            vec![Row::group(format!("commits: {}", commits.len()), commit_rows), Row::group(format!("jobs: {}", jobs.len()), job_rows)],
        );
        vec![starts_row, texts_row, read_row, sessions_row, work_row]
    }

    /// AGENT's system prompt, the node his System prompt press opens (fix 4; the owner N-1104 p767: "linked to most
    /// every claude code step"; docket-edge6 N-1446 p2496, accounts-lead p2504, p2529), two rows:
    /// - started with: the text each of its starts gave Claude Code beside or in place of its own (`started_with`),
    ///   each text once, where it first stood, with its starts; the requests before its first start line and a start
    ///   that names none (a record from before `system_sha`) under the standing text no line says ([`NOT_SAID`]);
    /// - Claude Code's own: the system prompt blocks its requests carried, version by version along its sessions'
    ///   `became` edges, as its folder's sessions have them but over all its sessions; the requests a start in place of
    ///   it sent are none of its, and each such start says it was replaced. Claude Code joins an appended text onto
    ///   its last block (walk-a's session, p2529), so that block takes a new version with each started-with text too.
    ///
    /// Each version counted: the requests that carried it (a started-with text: those sent from each of its starts to
    /// the agent's next, to the second, as `sent_in` joins them; a block: its `carried` edges), the turns they were
    /// sent in (`sent_in`) and the steps they worked for (`for`). With VERSION (a text node either row holds), that
    /// version's requests alone, under their turns under their steps. None for an agent the graph has not, or a
    /// version it has not. A start that began a line of its own, his edit from a turn, says which line and from
    /// which turn of which (its session's `came_from` to that turn, made at the start's own line; path fix 6), so the
    /// System prompt and Initial prompt panes show the branch as they show the start; a later start on that line
    /// (a restart) does not.
    pub fn prompt(&self, agent: &str, version: Option<&str>) -> Vec<Row> {
        let a = crate::graph::agent_id(agent);
        if self.get(&a).is_none() {
            return Vec::new();
        }
        // one pass over the whole graph, by each edge's kind, as the folder's (`folder_of`)
        let (mut starts, mut sessions, mut sent, mut carried) = (Vec::new(), HashSet::new(), Vec::new(), Vec::new());
        let (mut sent_in, mut fors, mut became, mut given) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut came = Vec::new();
        for e in &self.edges {
            match e.kind {
                EdgeKind::StartOf if e.to == a => starts.push(e),
                EdgeKind::CameFrom if e.from.starts_with("start:") || e.from.starts_with("session:") => came.push(e),
                EdgeKind::SessionOf if e.to == a => {
                    sessions.insert(e.from.as_str());
                }
                EdgeKind::Sent => sent.push(e),
                EdgeKind::Carried => carried.push(e),
                EdgeKind::SentIn => sent_in.push(e),
                EdgeKind::For => fors.push(e),
                EdgeKind::Became => became.push(e),
                EdgeKind::StartedWith => given.push(e),
                _ => {}
            }
        }
        // its requests in time, each once (two of its session files may carry one), each one's turn and step
        sent.retain(|e| sessions.contains(e.from.as_str()));
        in_time(&mut sent);
        let mut seen = HashSet::new();
        sent.retain(|e| seen.insert(e.to.as_str()));
        let requests: HashSet<&str> = sent.iter().map(|e| e.to.as_str()).collect();
        let mut turn: HashMap<&str, &str> = HashMap::new();
        for e in sent_in.iter().filter(|e| requests.contains(e.from.as_str())) {
            turn.entry(e.from.as_str()).or_insert(e.to.as_str());
        }
        let mut step: HashMap<&str, BTreeSet<&str>> = HashMap::new();
        for e in fors.iter().filter(|e| requests.contains(e.from.as_str())) {
            step.entry(e.from.as_str()).or_default().insert(e.to.as_str());
        }

        // 1. started with: its starts in time, each request in the latest's at or before it, to the second
        in_time(&mut starts);
        let gave: HashMap<&str, &Edge> = given.iter().map(|e| (e.from.as_str(), *e)).collect();
        let key = |s: &Edge| gave.get(s.from.as_str()).map(|g| g.to.as_str());
        fn second(e: &Edge) -> &str {
            e.when.as_deref().and_then(|w| w.get(..19)).unwrap_or("")
        }
        let seconds: Vec<&str> = starts.iter().map(|s| second(s)).collect();
        let (mut order, mut spans): (Vec<Option<&str>>, HashMap<Option<&str>, (Vec<&Edge>, BTreeSet<&str>)>) = Default::default();
        let mut replaced: HashSet<&str> = HashSet::new();
        for r in &sent {
            let at = second(r);
            let by = seconds.partition_point(|s| *s <= at).checked_sub(1).map(|i| starts[i]);
            let k = by.and_then(key);
            if !spans.contains_key(&k) {
                order.push(k);
            }
            spans.entry(k).or_default().1.insert(r.to.as_str());
            if by.and_then(|s| gave.get(s.from.as_str())).is_some_and(|g| g.what.contains(crate::graph::IN_PLACE)) {
                replaced.insert(r.to.as_str());
            }
        }
        for s in &starts {
            let k = key(s);
            if !spans.contains_key(&k) {
                order.push(k);
            }
            spans.entry(k).or_default().0.push(s);
        }
        // in the order each first stood: its first start or its first request, whichever came first
        order.sort_by_cached_key(|k| {
            let (ss, rs) = &spans[k];
            let s = ss.first().map(|s| second(s));
            let r = sent.iter().find(|e| rs.contains(e.to.as_str())).map(|e| second(e));
            s.into_iter().chain(r).min().unwrap_or_default()
        });

        // 2. Claude Code's own: the blocks its requests carried, but those a start in place of it sent
        let stems: HashSet<&str> = sessions.iter().map(|s| s.trim_start_matches("session:")).collect();
        carried.retain(|e| requests.contains(e.from.as_str()) && !replaced.contains(e.from.as_str()) && self.get(&e.to).is_some_and(|n| n.kind == NodeKind::Text));
        in_time(&mut carried);
        let (mut texts, mut carriers): (Vec<&str>, HashMap<&str, (BTreeSet<&str>, &Edge)>) = Default::default();
        for &e in &carried {
            carriers
                .entry(e.to.as_str())
                .or_insert_with(|| {
                    texts.push(e.to.as_str());
                    (BTreeSet::new(), e)
                })
                .0
                .insert(e.from.as_str());
        }

        // with a version, its requests alone
        if let Some(v) = version {
            let k = if v == NOT_SAID { None } else { Some(v) };
            let Some(rs) = spans.get(&k).map(|(_, rs)| rs).or_else(|| carriers.get(v).map(|c| &c.0)) else {
                return Vec::new();
            };
            let es: Vec<&Edge> = sent.iter().copied().filter(|e| rs.contains(e.to.as_str())).collect();
            return by_step(self, &es, &turn, &step);
        }

        // the line a start began (path fix 6): its session's `came_from` to a turn, made at the start's own line
        let session_of: HashMap<&str, &str> = came.iter().filter(|e| e.to.starts_with("session:")).map(|e| (e.from.as_str(), e.to.as_str())).collect();
        let line_of: HashMap<(&str, Option<usize>), &str> =
            came.iter().filter(|e| self.get(&e.to).is_some_and(|n| n.kind == NodeKind::Turn)).map(|e| ((e.from.as_str(), e.line()), e.what.as_str())).collect();
        let began = |s: &Edge| session_of.get(s.from.as_str()).and_then(|x| line_of.get(&(*x, s.line()))).map(|w| format!("; {w}")).unwrap_or_default();
        let mut rows = Vec::new();
        for k in &order {
            let (ss, rs) = &spans[k];
            let gv = k.and(ss.iter().find_map(|s| gave.get(s.from.as_str())));
            let what = gv.map_or("the standing text, appended; no start line says which version", |g| g.what.as_str());
            let text = format!("{what}: {}", counted(rs, &turn, &step));
            let mut r = match (k, gv) {
                (Some(t), Some(g)) => Row::of_edge(g, t, text),
                _ => Row { text, of: NOT_SAID.to_string(), ..Default::default() },
            };
            r.rows = ss.iter().map(|s| Row::of_node(self, &s.from, format!("{}: {}{}", s.when.as_deref().unwrap_or("?"), self.get(&s.from).map_or("", |n| n.what.as_str()), began(s)))).collect();
            rows.push(r);
        }
        let started = Row::group(format!("started with: {}", many(rows.len(), "version", "versions")), rows);

        let set: BTreeSet<&str> = texts.iter().copied().collect();
        in_time(&mut became);
        let bs: Vec<&Edge> = became.into_iter().filter(|b| matches!(&b.source, Source::Session(f, _) if stems.contains(f.as_str())) && set.contains(b.from.as_str()) && set.contains(b.to.as_str())).collect();
        let mut blocks: Vec<Row> = Vec::new();
        for (t, b) in versions(&texts, &bs) {
            let Some((who, first)) = carriers.get(t) else { continue };
            if b.is_none() {
                blocks.push(Row { of: t.to_string(), ..Default::default() });
            }
            let Some(block) = blocks.last_mut() else { continue };
            let text = format!("version {}: {}{}", block.rows.len() + 1, counted(who, &turn, &step), b.map(|b| format!("; {}", words_head(&b.what))).unwrap_or_default());
            let mut v = Row::of_edge(b.unwrap_or(*first), t, text);
            v.words = b.map(|b| b.what.clone()).unwrap_or_default();
            block.rows.push(v);
        }
        let all: usize = blocks.iter().map(|b| b.rows.len()).sum();
        let n = blocks.len();
        for (k, b) in blocks.iter_mut().enumerate() {
            b.text = format!("block {}: {}", k + 1, many(b.rows.len(), "version", "versions"));
        }
        // each start in place of it: from then its requests carried his text, and none of Claude Code's own
        for s in &starts {
            if let Some(g) = gave.get(s.from.as_str()).filter(|g| g.what.contains(crate::graph::IN_PLACE)) {
                let v = g.what.split(',').next().unwrap_or_default();
                blocks.push(Row::of_node(self, &s.from, format!("{}: replaced by your {v}", s.when.as_deref().unwrap_or("?"))));
            }
        }
        let own = Row::group(format!("Claude Code's own: {}, {}", many(n, "block", "blocks"), many(all, "version", "versions")), blocks);
        vec![started, own]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::agent_id;

    /// His p678's way, as research's record has it (edge6's run2, proto-window p1363), and the hubs it touches: his
    /// post on N-1104, which holds his other post; accounts-2's answer on I-1469; the build and his window.
    fn p678() -> Graph {
        let mut g = Graph::default();
        let (owner, acc) = (g.node(agent_id("owner"), NodeKind::Agent, 1), g.node(agent_id("accounts-2"), NodeKind::Agent, 2));
        let chat = g.node("N-1104", NodeKind::Ticket, 3);
        let item = g.node("I-1469", NodeKind::Ticket, 4);
        let his = g.node("N-1104.p678", NodeKind::Post, 10);
        let other = g.node("N-1104.p745", NodeKind::Post, 40);
        let fix = g.node("I-1469.p26", NodeKind::Post, 20);
        let build = g.node("build:20261121-101928-4ee5c14e", NodeKind::Build, 30);
        let merge = g.node("commit:4ee5c14e", NodeKind::Commit, 30);
        g.edge(&owner, &his, EdgeKind::Wrote, 10);
        g.edge(&his, &chat, EdgeKind::PostedIn, 10);
        g.edge(&his, &acc, EdgeKind::Woke, 10);
        g.edge(&acc, &fix, EdgeKind::Wrote, 20);
        g.edge(&fix, &item, EdgeKind::PostedIn, 20);
        g.edge(&fix, &his, EdgeKind::Answered, 20);
        g.edge(&merge, &build, EdgeKind::BuiltInto, 30);
        g.edge(&build, &owner, EdgeKind::Installed, 30);
        g.edge(&owner, &other, EdgeKind::Wrote, 40);
        g.edge(&other, &chat, EdgeKind::PostedIn, 40);
        // each line's time, as `Graph::date` gives it: the lines from 10, 20, 30 and 40 at these, those before at none
        let times = [(10, "2026-11-21T08:19:19Z"), (20, "2026-11-21T08:43:19Z"), (30, "2026-11-21T09:39:20Z"), (40, "2026-11-22T07:17:15Z")];
        let time = |n: Option<usize>| times.iter().rev().find(|(f, _)| Some(*f) <= n).map(|(_, a)| a.to_string());
        g.nodes.iter_mut().for_each(|x| x.when = time(x.line()));
        g.edges.iter_mut().for_each(|e| e.when = time(e.line()));
        g
    }

    /// A kind or a layer this build does not know, a newer coordd's (paper-research N-1446 p1574), reads as `Other`, so
    /// a slice that holds one still decodes, and an ask from a newer window too; what this build makes reads back as it
    /// was.
    #[test]
    fn a_kind_or_a_layer_this_build_does_not_know_reads_as_other() {
        let s = p678().slice(&Ask::default());
        let mut v = serde_json::to_value(&s).unwrap();
        assert_eq!(serde_json::from_value::<Slice>(v.clone()).unwrap(), s);
        v["nodes"][0]["kind"] = "made_up".into();
        v["edges"][0]["kind"] = "made_up".into();
        v["edges"][0]["layer"] = "made_up".into();
        v["whole"]["made_up"] = 1.into();
        let read: Slice = serde_json::from_value(v).unwrap();
        assert_eq!((read.nodes[0].node.kind, read.edges[0].edge.kind, read.edges[0].layer), (NodeKind::Other, EdgeKind::Other, Layer::Other));
        assert_eq!(read.whole.get(&Layer::Other), Some(&1));
        assert_eq!((&read.nodes[1..], &read.edges[1..]), (&s.nodes[1..], &s.edges[1..]), "the rest as it was");
        let ask: Ask = serde_json::from_str(r#"{"layers": ["work", "made_up"]}"#).unwrap();
        assert_eq!(ask.layers, BTreeSet::from([Layer::Work, Layer::Other]));
        assert_eq!([NodeKind::Other, NodeKind::Worktree].map(|k| serde_json::to_value(k).unwrap()), ["other", "worktree"]);
        assert!(!Layer::ALL.contains(&Layer::Other), "no switch of its own");
    }

    fn edges(s: &Slice) -> Vec<(String, EdgeKind, String)> {
        s.edges.iter().map(|e| (e.edge.from.clone(), e.edge.kind, e.edge.to.clone())).collect()
    }

    fn lines(s: &Slice) -> Vec<usize> {
        s.edges.iter().map(|e| e.edge.line().unwrap()).collect()
    }

    /// Each edge is in one layer: his post's are his, the agent's answer to it too, the agent's post on its item the
    /// agents', what woke the agent a delivery, the build and his window work; counted in the whole graph.
    #[test]
    fn each_edge_is_in_one_layer_and_the_whole_graph_counts_them() {
        let g = p678();
        let s = g.slice(&Ask::default());
        let by: Vec<(EdgeKind, Layer)> = s.edges.iter().map(|e| (e.edge.kind, e.layer)).collect();
        assert_eq!(
            by,
            [
                (EdgeKind::Wrote, Layer::His),
                (EdgeKind::PostedIn, Layer::His),
                (EdgeKind::Woke, Layer::Deliveries),
                (EdgeKind::Wrote, Layer::Agents),
                (EdgeKind::PostedIn, Layer::Agents),
                (EdgeKind::Answered, Layer::His),
                (EdgeKind::BuiltInto, Layer::Work),
                (EdgeKind::Installed, Layer::Work),
                (EdgeKind::Wrote, Layer::His),
                (EdgeKind::PostedIn, Layer::His),
            ]
        );
        assert_eq!(s.whole, BTreeMap::from([(Layer::His, 5), (Layer::Agents, 2), (Layer::Deliveries, 1), (Layer::Work, 2)]));
        assert_eq!((s.shown.clone(), s.left), (s.whole.clone(), 0));
        let post = s.nodes.iter().find(|n| n.node.id == "N-1104.p678").unwrap();
        assert_eq!((post.by.as_deref(), post.node.when.as_deref()), (Some("owner"), Some("2026-11-21T08:19:19Z")));
        assert_eq!(s.nodes.iter().find(|n| n.node.id == "I-1469.p26").and_then(|n| n.by.as_deref()), Some("accounts-2"));
    }

    /// Around his post the walk takes its edges and theirs, but at his agent node and at his thread it stops: his
    /// other post on N-1104, two hops out through either, is not reached; the agent's answer is, through the post.
    #[test]
    fn a_walk_around_a_post_stops_at_agents_and_tickets() {
        let g = p678();
        let s = g.slice(&Ask { around: Some("N-1104.p678".into()), hops: 2, ..Ask::default() });
        let got = edges(&s);
        assert!(got.contains(&("I-1469.p26".into(), EdgeKind::Answered, "N-1104.p678".into())), "{got:?}");
        assert!(got.contains(&("agent:accounts-2".into(), EdgeKind::Wrote, "I-1469.p26".into())), "the answer's own edges, hop 2: {got:?}");
        assert!(!s.nodes.iter().any(|n| n.node.id == "N-1104.p745"), "{:?}", s.nodes);
        assert!(!got.iter().any(|(_, k, _)| *k == EdgeKind::Installed), "past agent:owner: {got:?}");
        assert!(!s.unknown);
        assert!(g.slice(&Ask { around: Some("N-9.p1".into()), hops: 2, ..Ask::default() }).unknown);
    }

    /// His post p815 in the shape research's record gives it (proto-window N-1446 p1752): the run of `owner_post` that
    /// handled it at 16:49, its step, the step's call file (its prompt template) and a call on the wire (the record's
    /// run of p815 has one step, `gather`, which calls no model; the asks runs after it each call gpt-6.2-sol); and p808,
    /// an older post of his it names, which 40 of the agents' requests were for after it, each carrying the one system
    /// prompt block. Around p815 at three hops, the run is hop 1, its step hop 2, the call file and the call hop 3;
    /// p808's requests hop 2, their blocks hop 3.
    fn p815() -> Graph {
        let mut g = Graph::default();
        let at = |m: u32, s: u32| Some(format!("2026-11-22T{:02}:{:02}:{s:02}Z", 16 + m / 60, m % 60));
        let line = |n: usize| Source::Line(n);
        let p = g.node_from("N-1104.p815", NodeKind::Post, line(164337), at(49, 23));
        let older = g.node_from("N-1104.p808", NodeKind::Post, line(162093), Some("2026-11-22T13:50:40Z".into()));
        let run = g.node_from("O-1099511631050", NodeKind::Ticket, line(164338), at(49, 42));
        let step = g.node_from("O-1099511631050.s1", NodeKind::Step, line(164340), at(49, 43));
        let call = g.node_from("call:route", NodeKind::CallFile, line(164340), at(49, 43));
        let wire = g.node_from("wire:O-1099511631050.s1@gpt-6.2-sol", NodeKind::Call, line(164341), at(49, 44));
        g.edge_from(&p, &older, EdgeKind::Named, line(164337), at(49, 23));
        g.edge_from(&p, &run, EdgeKind::HandledBy, line(164338), at(49, 42));
        g.edge_from(&step, &run, EdgeKind::StepOf, line(164340), at(49, 43));
        g.edge_from(&step, &call, EdgeKind::CameFrom, line(164340), at(49, 43));
        g.edge_from(&step, &wire, EdgeKind::Called, line(164341), at(49, 44));
        let block = g.node_from("text:2719b7a469d9", NodeKind::Text, Source::Session("a056fb58".into(), 2), at(60, 0));
        for k in 0..40u32 {
            let source = Source::Session("a056fb58".into(), 10 + k as usize);
            let r = g.node_from(format!("request:r{k}"), NodeKind::Request, source.clone(), at(60 + k, 5));
            g.edge_from(&r, &older, EdgeKind::For, source.clone(), at(60 + k, 5));
            g.edge_from(&r, &block, EdgeKind::Carried, source, at(60 + k, 5));
        }
        g
    }

    /// The limit keeps what is near the focus before what is far but new (paper-research N-1446 p1753): around p815 at
    /// three hops and 48 edges of its 85, its `handled_by` (hop 1) and its step (hop 2), each of p808's 40 requests
    /// (hop 2), and of hop 3's 42 edges the 5 it has room for: the run's 2 whole, its call file and its call on the wire,
    /// though older than every request of p808's, and p808's newest 3. Newest first, the limit kept p808's requests and
    /// their blocks alone, as the window's ask around p815 did on research's record (p1752); the newest first within hop
    /// 3, their blocks and neither the call file nor the call.
    #[test]
    fn the_limit_keeps_a_posts_own_run_before_a_neighbours_newer_requests() {
        let g = p815();
        let ask = |limit: usize| Ask { around: Some("N-1104.p815".into()), hops: 3, limit, ..Ask::default() };
        let all = g.slice(&ask(0));
        assert_eq!((all.edges.len(), all.left), (85, 0), "every edge in reach");
        let s = g.slice(&ask(48));
        assert_eq!((s.edges.len(), s.left), (48, 37));
        let got = edges(&s);
        let has = |f: &str, k: EdgeKind, t: &str| got.contains(&(f.to_string(), k, t.to_string()));
        assert!(has("N-1104.p815", EdgeKind::HandledBy, "O-1099511631050"), "its run: {got:?}");
        assert!(has("O-1099511631050.s1", EdgeKind::StepOf, "O-1099511631050"), "its step: {got:?}");
        assert!(has("O-1099511631050.s1", EdgeKind::CameFrom, "call:route"), "its call file: {got:?}");
        assert!(has("O-1099511631050.s1", EdgeKind::Called, "wire:O-1099511631050.s1@gpt-6.2-sol"), "its call on the wire: {got:?}");
        assert_eq!(got.iter().filter(|(_, k, _)| *k == EdgeKind::For).count(), 40, "p808's requests, hop 2, whole");
        let blocks: Vec<&str> = got.iter().filter(|(_, k, _)| *k == EdgeKind::Carried).map(|(f, _, _)| f.as_str()).collect();
        assert_eq!(blocks, ["request:r37", "request:r38", "request:r39"], "hop 3's room past the run's, p808's newest");
        assert!(s.nodes.iter().any(|n| n.node.id == "wire:O-1099511631050.s1@gpt-6.2-sol"), "the call's node with its edge");
        // still oldest first, and the hours count what the limit left too
        assert!(s.edges.windows(2).all(|w| w[0].edge.when <= w[1].edge.when), "{got:?}");
        assert_eq!(s.hours.values().flat_map(|x| x.values()).sum::<usize>(), 85);
        // a slice of the whole graph is one hop, each edge its own: the newest first, as before
        let whole = g.slice(&Ask { limit: 3, ..Ask::default() });
        assert_eq!(edges(&whole).iter().map(|(f, k, _)| (f.as_str(), *k)).collect::<Vec<_>>(), [("request:r38", EdgeKind::Carried), ("request:r39", EdgeKind::For), ("request:r39", EdgeKind::Carried)]);
    }

    /// The layers on, who, the item and when narrow it; the limit keeps the newest and counts the rest.
    #[test]
    fn layers_who_items_when_and_the_limit_narrow_it_and_say_what_they_left() {
        let g = p678();
        let work = g.slice(&Ask { layers: BTreeSet::from([Layer::Work]), ..Ask::default() });
        assert_eq!(edges(&work), [("commit:4ee5c14e".into(), EdgeKind::BuiltInto, "build:20261121-101928-4ee5c14e".into()), ("build:20261121-101928-4ee5c14e".into(), EdgeKind::Installed, "agent:owner".into())]);
        assert_eq!(work.shown, BTreeMap::from([(Layer::Work, 2)]));
        assert_eq!(work.whole.values().sum::<usize>(), 10, "the whole graph's counts whatever is on");
        let acc = g.slice(&Ask { who: vec!["accounts-2".into()], ..Ask::default() });
        assert_eq!(lines(&acc), [10, 20, 20, 20], "what woke it, and its post's three");
        let item = g.slice(&Ask { items: vec!["I-1469".into()], ..Ask::default() });
        assert_eq!(edges(&item), [("agent:accounts-2".into(), EdgeKind::Wrote, "I-1469.p26".into()), ("I-1469.p26".into(), EdgeKind::PostedIn, "I-1469".into()), ("I-1469.p26".into(), EdgeKind::Answered, "N-1104.p678".into())], "its post's three, at the post and at the item");
        assert!(g.slice(&Ask { items: vec!["I-14".into()], ..Ask::default() }).edges.is_empty(), "an id is the item's whole");
        let day = g.slice(&Ask { from: Some("2026-11-21T09:00:00Z".into()), to: Some("2026-11-21T23:59:59Z".into()), ..Ask::default() });
        assert_eq!(lines(&day), [30, 30]);
        let two = g.slice(&Ask { limit: 2, ..Ask::default() });
        assert_eq!((lines(&two), two.left), (vec![40, 40], 8));
        // the hours count what the limit left out too, by layer: his edges at 08 and 07 the next day, the agents' at 08,
        // the work at 09
        let h = |l: Layer, hs: &[(&str, usize)]| (l, hs.iter().map(|(k, n)| (k.to_string(), *n)).collect::<BTreeMap<_, _>>());
        assert_eq!(
            two.hours,
            BTreeMap::from([
                h(Layer::His, &[("2026-11-21T08", 3), ("2026-11-22T07", 2)]),
                h(Layer::Agents, &[("2026-11-21T08", 2)]),
                h(Layer::Deliveries, &[("2026-11-21T08", 1)]),
                h(Layer::Work, &[("2026-11-21T09", 2)]),
            ])
        );
        assert_eq!(two.hours.values().flat_map(|x| x.values()).sum::<usize>(), two.edges.len() + two.left);
    }

    /// git's and the runner's edges are work, whatever their kind (an agent wrote the commit, a job ran on it, the
    /// commit's session and parent); an edge with no time is in the whole graph but in no span; and a slice reads back
    /// from its JSON as it was, each node's and edge's source one field (`"line"`, `"commit"`), as the window reads it. An
    /// item noted on his post, and an agent's start of it, are work too.
    #[test]
    fn gits_and_the_runners_edges_are_work_and_a_slice_reads_back_from_its_json() {
        use crate::graph::Source;
        let mut g = p678();
        let at = Some("2026-11-21T09:30:00Z".to_string());
        let c = g.node_from("commit:de447f86", NodeKind::Commit, Source::Commit("de447f86".into()), at.clone());
        let job = g.node_from("job:41234-1763717400", NodeKind::Job, Source::Job("41234-1763717400".into()), at.clone());
        let session = g.node_from("session:97a64ad1", NodeKind::Session, Source::Commit("de447f86".into()), at.clone());
        g.edge_from("agent:accounts-2", &c, EdgeKind::Wrote, Source::Commit("de447f86".into()), at.clone());
        g.edge_from(&c, &session, EdgeKind::CameFrom, Source::Commit("de447f86".into()), at.clone());
        g.edge_from(&c, "commit:4ee5c14e", EdgeKind::MergedInto, Source::Commit("4ee5c14e".into()), at.clone());
        g.edge_from("agent:accounts-2", &job, EdgeKind::Ran, Source::Job("41234-1763717400".into()), at.clone());
        g.edge_from(&job, &c, EdgeKind::Ran, Source::Job("41234-1763717400".into()), None);
        g.get_mut(&job).unwrap().ok = Some(true);
        let work = g.slice(&Ask { layers: BTreeSet::from([Layer::Work]), ..Ask::default() });
        assert_eq!(work.shown, BTreeMap::from([(Layer::Work, 7)]), "{:?}", edges(&work));
        let agents = g.slice(&Ask { layers: BTreeSet::from([Layer::Agents]), ..Ask::default() });
        assert!(!edges(&agents).iter().any(|(_, _, to)| to.starts_with("commit:")), "{:?}", edges(&agents));
        // the job's end has no time: in the whole graph, in no span
        let spanned = g.slice(&Ask { layers: BTreeSet::from([Layer::Work]), from: Some("2026-11-21T00:00:00Z".into()), ..Ask::default() });
        assert_eq!(spanned.edges.len(), 6);
        // oldest first, by time: the git edges at 09:30 between the line at 08:43 and the build's at 09:39
        let kinds: Vec<EdgeKind> = work.edges.iter().map(|e| e.edge.kind).collect();
        assert_eq!(kinds.first(), Some(&EdgeKind::Ran), "no time comes first: {kinds:?}");
        assert_eq!(kinds.last(), Some(&EdgeKind::Installed), "{kinds:?}");
        let text = serde_json::to_string(&work).unwrap();
        assert!(text.contains(r#""commit":"de447f86""#) && text.contains(r#""line":30"#) && text.contains(r#""job":"41234-1763717400""#), "{text}");
        assert_eq!(serde_json::from_str::<Slice>(&text).unwrap(), work);
        let all = g.slice(&Ask::default());
        assert_eq!(serde_json::from_str::<Slice>(&serde_json::to_string(&all).unwrap()).unwrap(), all);
        // an item noted on his post, and an agent's start of it, are work; a ticket's program stays a call's
        let mut h = p678();
        let item = h.node("I-1480", NodeKind::Ticket, 50);
        let program = h.node("program:item", NodeKind::Program, 50);
        h.edge(&item, "N-1104.p678", EdgeKind::CameFrom, 50);
        h.edge(&item, &program, EdgeKind::CameFrom, 50);
        h.edge("agent:accounts-2", &item, EdgeKind::Started, 51);
        let got = edges(&h.slice(&Ask { layers: BTreeSet::from([Layer::Work]), ..Ask::default() }));
        assert!(got.len() == 4 && got.contains(&(item.clone(), EdgeKind::CameFrom, "N-1104.p678".into())) && got.contains(&("agent:accounts-2".into(), EdgeKind::Started, item.clone())), "{got:?}");
        let calls = edges(&h.slice(&Ask { layers: BTreeSet::from([Layer::Calls]), ..Ask::default() }));
        assert_eq!(calls, [(item, EdgeKind::CameFrom, program)]);
    }

    /// EDGE between FROM and TO, from SOURCE at WHEN (`09:00:01`, today's), each end made once, as the graph's own:
    /// its index among G's edges.
    fn put(g: &mut Graph, from: (&str, NodeKind), to: (&str, NodeKind), kind: EdgeKind, source: Source, when: &str) -> usize {
        let at = (!when.is_empty()).then(|| format!("2026-11-22T{when}Z"));
        let a = g.node_from(from.0, from.1, source.clone(), at.clone());
        let b = g.node_from(to.0, to.1, source.clone(), at.clone());
        g.edge_from(&a, &b, kind, source, at);
        g.edges.len() - 1
    }

    /// w's and x's work (made up, in the shapes coordd's side, git's round and the record's lines put): w's start,
    /// briefed by N-9.p2; its two sessions, s2 carrying one of s1's requests again, as one agent's two session files
    /// can; its memory file in three versions, the third first loaded in s2, and its CLAUDE.md in one; three reads of
    /// two files; four requests of two models, one with no response; a system block in two versions beside one that
    /// stays; two commits, one with the worktree it was made in, and two jobs, one with its worktree. x has no start
    /// line; its session takes w's memory file from its second version to its third, and runs a request with it.
    fn folders() -> Graph {
        use crate::graph::Tokens;
        let mut g = Graph::default();
        let ss = |f: &str, n: usize| Source::Session(f.to_string(), n);
        let (w, x) = (("agent:w", NodeKind::Agent), ("agent:x", NodeKind::Agent));
        let (s1, s2, sx) = (("session:s1", NodeKind::Session), ("session:s2", NodeKind::Session), ("session:sx", NodeKind::Session));
        let text = |id| (id, NodeKind::Text);
        let file = |id| (id, NodeKind::File);
        let rq = |id| (id, NodeKind::Request);
        let commit = |id| (id, NodeKind::Commit);
        let job = |id| (id, NodeKind::Job);
        let (start, tree) = (("start:w@3", NodeKind::Start), ("worktree:/w/t", NodeKind::Worktree));
        put(&mut g, s1, w, EdgeKind::SessionOf, Source::Line(2), "09:00:00");
        put(&mut g, start, w, EdgeKind::StartOf, Source::Line(3), "09:00:00");
        g.get_mut("start:w@3").unwrap().what = "start: claude claude-opus-5-5, effort high, in /d/w, by lead".into();
        put(&mut g, start, ("N-9.p2", NodeKind::Post), EdgeKind::Briefed, Source::Line(3), "09:00:00");
        put(&mut g, start, s1, EdgeKind::CameFrom, Source::Line(3), "09:00:00");
        put(&mut g, s2, w, EdgeKind::SessionOf, Source::Line(5), "10:00:00");
        put(&mut g, sx, x, EdgeKind::SessionOf, Source::Line(6), "09:30:00");
        put(&mut g, s1, text("text:bw"), EdgeKind::Briefed, ss("s1", 1), "09:00:01");
        put(&mut g, sx, text("text:bx"), EdgeKind::Briefed, ss("sx", 1), "09:30:01");
        g.get_mut("text:bx").unwrap().what = "the first user message, 120 bytes".into();
        // the loads: the memory file and CLAUDE.md, the memory file again, its second version, its third in s2
        let (memory, claude) = (file("file:/m/MEMORY.md"), file("file:/w/CLAUDE.md"));
        put(&mut g, s1, text("text:m1"), EdgeKind::Loaded, ss("s1", 2), "09:00:02");
        put(&mut g, text("text:m1"), memory, EdgeKind::VersionOf, ss("s1", 2), "09:00:02");
        put(&mut g, s1, text("text:c1"), EdgeKind::Loaded, ss("s1", 3), "09:00:02");
        put(&mut g, text("text:c1"), claude, EdgeKind::VersionOf, ss("s1", 3), "09:00:02");
        put(&mut g, s1, text("text:m1"), EdgeKind::Loaded, ss("s1", 10), "09:00:05");
        put(&mut g, s1, text("text:m2"), EdgeKind::Loaded, ss("s1", 20), "09:00:06");
        put(&mut g, text("text:m2"), memory, EdgeKind::VersionOf, ss("s1", 20), "09:00:06");
        let b = put(&mut g, text("text:m1"), text("text:m2"), EdgeKind::Became, ss("s1", 20), "09:00:06");
        g.edges[b].what = "2 words added: - one {+- two+}".into();
        put(&mut g, s2, text("text:m3"), EdgeKind::Loaded, ss("s2", 2), "10:00:01");
        put(&mut g, text("text:m3"), memory, EdgeKind::VersionOf, ss("s2", 2), "10:00:01");
        put(&mut g, s2, text("text:c1"), EdgeKind::Loaded, ss("s2", 3), "10:00:01");
        put(&mut g, sx, text("text:m2"), EdgeKind::Loaded, ss("sx", 2), "09:30:02");
        put(&mut g, sx, text("text:m3"), EdgeKind::Loaded, ss("sx", 8), "09:30:05");
        let b = put(&mut g, text("text:m2"), text("text:m3"), EdgeKind::Became, ss("sx", 8), "09:30:05");
        g.edges[b].what = "1 word removed: - one [-- two-]".into();
        // the requests: what each ran with and carried, as its session's line says
        let request = |g: &mut Graph, s: (&str, NodeKind), id: &'static str, n: usize, when: &str, what: &str, tokens: Option<(u64, u64)>| {
            put(g, s, rq(id), EdgeKind::Sent, ss(s.0.trim_start_matches("session:"), n), when);
            let r = g.get_mut(id).unwrap();
            r.what = what.into();
            r.tokens = tokens.map(|(input, output)| Tokens { input, output, cache_read: 80, cache_write: 10, reasoning: 0 });
        };
        let with = |g: &mut Graph, id: &'static str, k: EdgeKind, to: (&str, NodeKind), s: &str, n: usize, when: &str| {
            put(g, rq(id), to, k, ss(s, n), when);
        };
        request(&mut g, s1, "request:r1", 4, "09:00:03", "claude-opus-5-5 high", Some((10, 5)));
        with(&mut g, "request:r1", EdgeKind::RanWith, text("text:m1"), "s1", 4, "09:00:03");
        with(&mut g, "request:r1", EdgeKind::RanWith, text("text:c1"), "s1", 4, "09:00:03");
        with(&mut g, "request:r1", EdgeKind::Carried, text("text:sys1"), "s1", 4, "09:00:03");
        with(&mut g, "request:r1", EdgeKind::Carried, text("text:sysB"), "s1", 4, "09:00:03");
        with(&mut g, "request:r1", EdgeKind::Carried, ("tools:t1", NodeKind::Tools), "s1", 4, "09:00:03");
        request(&mut g, s1, "request:r2", 21, "09:00:07", "claude-opus-5-5 high", Some((10, 2)));
        with(&mut g, "request:r2", EdgeKind::RanWith, text("text:m2"), "s1", 21, "09:00:07");
        with(&mut g, "request:r2", EdgeKind::RanWith, text("text:c1"), "s1", 21, "09:00:07");
        with(&mut g, "request:r2", EdgeKind::Carried, text("text:sys2"), "s1", 21, "09:00:07");
        with(&mut g, "request:r2", EdgeKind::Carried, text("text:sysB"), "s1", 21, "09:00:07");
        let b = put(&mut g, text("text:sys1"), text("text:sys2"), EdgeKind::Became, ss("s1", 21), "09:00:07");
        g.edges[b].what = "1 word removed, 1 added: standing, [-first-] {+second+}".into();
        request(&mut g, s1, "request:r3", 22, "09:00:08", "claude-sonnet-5-5 medium", None);
        with(&mut g, "request:r3", EdgeKind::RanWith, text("text:m2"), "s1", 22, "09:00:08");
        with(&mut g, "request:r3", EdgeKind::RanWith, text("text:c1"), "s1", 22, "09:00:08");
        with(&mut g, "request:r3", EdgeKind::Carried, text("text:sys2"), "s1", 22, "09:00:08");
        with(&mut g, "request:r3", EdgeKind::Carried, text("text:sysB"), "s1", 22, "09:00:08");
        put(&mut g, s2, rq("request:r2"), EdgeKind::Sent, ss("s2", 5), "09:00:07");
        request(&mut g, s2, "request:r4", 6, "10:00:02", "claude-opus-5-5 high", Some((5, 1)));
        with(&mut g, "request:r4", EdgeKind::RanWith, text("text:m3"), "s2", 6, "10:00:02");
        with(&mut g, "request:r4", EdgeKind::RanWith, text("text:c1"), "s2", 6, "10:00:02");
        with(&mut g, "request:r4", EdgeKind::Carried, text("text:sys2"), "s2", 6, "10:00:02");
        with(&mut g, "request:r4", EdgeKind::Carried, text("text:sysB"), "s2", 6, "10:00:02");
        request(&mut g, sx, "request:rx", 9, "09:30:06", "claude-opus-5-5 low", None);
        with(&mut g, "request:rx", EdgeKind::RanWith, text("text:m3"), "sx", 9, "09:30:06");
        with(&mut g, "request:rx", EdgeKind::Carried, text("text:sys2"), "sx", 9, "09:30:06");
        // the reads
        put(&mut g, s1, file("file:/w/a.rs"), EdgeKind::Read, ss("s1", 30), "09:00:09");
        put(&mut g, s2, file("file:/w/a.rs"), EdgeKind::Read, ss("s2", 7), "10:00:03");
        put(&mut g, s1, file("file:/w/b.rs"), EdgeKind::Read, ss("s1", 31), "09:00:10");
        put(&mut g, sx, file("file:/w/c.rs"), EdgeKind::Read, ss("sx", 10), "09:30:07");
        // git's: the commits, their branches and the worktree the first was made in; a post of w's, which is no commit
        let (c1, c2) = (commit("commit:c1aaaaaa"), commit("commit:c2bbbbbb"));
        let (edge, main) = (("branch:repo/edge", NodeKind::Branch), ("branch:repo/main", NodeKind::Branch));
        let git = |c: &str| Source::Commit(c.to_string());
        put(&mut g, w, c1, EdgeKind::Wrote, git("c1aaaaaa"), "09:30:00");
        g.get_mut(c1.0).unwrap().what = "point 3: the words".into();
        put(&mut g, c1, edge, EdgeKind::On, git("c1aaaaaa"), "09:30:00");
        put(&mut g, c1, tree, EdgeKind::In, git("c1aaaaaa"), "09:30:00");
        put(&mut g, c1, s1, EdgeKind::CameFrom, git("c1aaaaaa"), "09:30:00");
        put(&mut g, w, c2, EdgeKind::Wrote, git("c2bbbbbb"), "09:40:00");
        g.get_mut(c2.0).unwrap().what = "the fix".into();
        put(&mut g, c2, edge, EdgeKind::On, git("c2bbbbbb"), "09:40:00");
        put(&mut g, c2, main, EdgeKind::On, git("c2bbbbbb"), "09:41:00");
        put(&mut g, w, ("N-9.p3", NodeKind::Post), EdgeKind::Wrote, Source::Line(50), "09:50:00");
        put(&mut g, x, commit("commit:c3cccccc"), EdgeKind::Wrote, git("c3cccccc"), "09:45:00");
        // the runner's: two jobs of w's, the first ended, with the worktree it sent
        put(&mut g, w, job("job:9-1"), EdgeKind::Ran, Source::Line(40), "09:35:00");
        g.get_mut("job:9-1").unwrap().what = "test docket-machine: rc 0, 33 passed, 0 failed".into();
        put(&mut g, job("job:9-1"), c1, EdgeKind::Ran, Source::Line(40), "09:35:00");
        put(&mut g, job("job:9-1"), tree, EdgeKind::In, Source::Line(40), "09:35:00");
        put(&mut g, job("job:9-1"), c1, EdgeKind::Ended, Source::Line(41), "09:36:00");
        put(&mut g, w, job("job:9-2"), EdgeKind::Ran, Source::Line(45), "09:45:00");
        g.get_mut("job:9-2").unwrap().what = "test coordd".into();
        put(&mut g, job("job:9-2"), c2, EdgeKind::Ran, Source::Line(45), "09:45:00");
        g
    }

    /// orch's system prompt (made up, in the shapes roster's `began` lines and coordd's side put): a request before its
    /// first start line; its start with the standing text and two requests in one turn and one step, the second
    /// carrying block 2's second version; his version 2 appended, its request carrying block 2's third; his version 3
    /// in place of Claude Code's own, its request carrying his text alone, in no turn yet. One session; x's request
    /// carries block 1 too, and counts for none of orch's.
    fn prompts() -> Graph {
        let mut g = Graph::default();
        let ss = |n: usize| Source::Session("s1".to_string(), n);
        let (orch, s1) = (("agent:orch", NodeKind::Agent), ("session:s1", NodeKind::Session));
        let text = |id: &'static str| (id, NodeKind::Text);
        let rq = |id: &'static str| (id, NodeKind::Request);
        put(&mut g, s1, orch, EdgeKind::SessionOf, Source::Line(1), "08:59:00");
        // its starts, each with the text it was started with, and its turns' ends
        let starts = [
            (3, "09:00:00", "start: claude claude-opus-5-5, by lead", "text:std", "the standing text, appended"),
            (4, "09:10:00", "system: claude claude-opus-5-5, system prompt 2", "text:v2", "version 2, appended"),
            (5, "09:20:00", "system: claude claude-opus-5-5, system prompt 3 in place of Claude Code's own", "text:v3", "version 3, in place of Claude Code's own"),
        ];
        for (n, when, what, t, words) in starts {
            let id = format!("start:orch@{n}");
            put(&mut g, (id.as_str(), NodeKind::Start), orch, EdgeKind::StartOf, Source::Line(n), when);
            g.get_mut(&id).unwrap().what = what.into();
            let e = put(&mut g, (id.as_str(), NodeKind::Start), text(t), EdgeKind::StartedWith, Source::Line(n), when);
            g.edges[e].what = words.into();
        }
        for (n, when) in [(2, "08:59:50"), (10, "09:05:00"), (20, "09:15:00")] {
            put(&mut g, (format!("turn:orch@{n}").as_str(), NodeKind::Turn), orch, EdgeKind::TurnOf, Source::Line(n), when);
        }
        // its requests: the turn each was sent in, the step it worked for, the blocks it carried
        let request = |g: &mut Graph, id: &'static str, n: usize, when: &str, turn: Option<&str>, posts: &[&str], blocks: &[&'static str]| {
            put(g, s1, rq(id), EdgeKind::Sent, ss(n), when);
            g.get_mut(id).unwrap().what = "claude-opus-5-5 high".into();
            if let Some(t) = turn {
                put(g, rq(id), (t, NodeKind::Turn), EdgeKind::SentIn, ss(n), when);
            }
            for &p in posts {
                put(g, rq(id), (p, NodeKind::Post), EdgeKind::For, ss(n), when);
            }
            for &b in blocks {
                put(g, rq(id), text(b), EdgeKind::Carried, ss(n), when);
            }
            put(g, rq(id), ("tools:t1", NodeKind::Tools), EdgeKind::Carried, ss(n), when);
        };
        request(&mut g, "request:r0", 2, "08:59:30", Some("turn:orch@2"), &[], &["text:cc1", "text:cc2a"]);
        request(&mut g, "request:r1", 5, "09:00:05", Some("turn:orch@10"), &["N-1.p1"], &["text:cc1", "text:cc2a"]);
        request(&mut g, "request:r2", 8, "09:00:30", Some("turn:orch@10"), &["N-1.p1"], &["text:cc1", "text:cc2b"]);
        let b = put(&mut g, text("text:cc2a"), text("text:cc2b"), EdgeKind::Became, ss(8), "09:00:30");
        g.edges[b].what = "1 word removed, 1 added: [-a-] {+b+}".into();
        request(&mut g, "request:r3", 12, "09:10:05", Some("turn:orch@20"), &["N-1.p2"], &["text:cc1", "text:cc2c"]);
        let b = put(&mut g, text("text:cc2b"), text("text:cc2c"), EdgeKind::Became, ss(12), "09:10:05");
        g.edges[b].what = "1 word added: {+c+}".into();
        request(&mut g, "request:r4", 15, "09:20:05", None, &["N-1.p3"], &["text:v3"]);
        // x's
        put(&mut g, ("session:sx", NodeKind::Session), ("agent:x", NodeKind::Agent), EdgeKind::SessionOf, Source::Line(6), "09:00:00");
        put(&mut g, ("session:sx", NodeKind::Session), rq("request:rx"), EdgeKind::Sent, Source::Session("sx".into(), 1), "09:00:06");
        put(&mut g, rq("request:rx"), text("text:cc1"), EdgeKind::Carried, Source::Session("sx".into(), 1), "09:00:06");
        g
    }

    /// ROWS as lines, each indented by its depth, with where it comes from; onto OUT.
    fn shown(rows: &[Row], depth: usize, out: &mut Vec<String>) {
        for r in rows {
            let from = match &r.source {
                None => String::new(),
                Some(Source::Line(n)) => format!(" · line {n}"),
                Some(Source::Session(f, n)) => format!(" · {f}:{n}"),
                Some(Source::Commit(c)) => format!(" · commit {c}"),
                Some(Source::Job(j)) => format!(" · job {j}"),
                Some(Source::Release(x)) => format!(" · release {x}"),
            };
            out.push(format!("{}{}{from}", "  ".repeat(depth), r.text));
            shown(&r.rows, depth + 1, out);
        }
    }

    /// ROWS' nodes, each row's that has one, in the order the rows are shown; onto OUT.
    fn ofs<'a>(rows: &'a [Row], out: &mut Vec<&'a str>) {
        for r in rows {
            if !r.of.is_empty() {
                out.push(&r.of);
            }
            ofs(&r.rows, out);
        }
    }

    /// ROWS' words (a `became` edge's), each with its row's text; onto OUT.
    fn words<'a>(rows: &'a [Row], out: &mut Vec<(&'a str, &'a str)>) {
        for r in rows {
            if !r.words.is_empty() {
                out.push((&r.text, &r.words));
            }
            words(&r.rows, out);
        }
    }

    /// A start that began a line of its own, his edit from a turn, says in its row which line and from which turn of
    /// which, read from the graph the record makes (path fix 6). The start before it, a restart on the new line, and
    /// an edit with Now say none. So his System prompt and Initial prompt panes show the branch where its start is.
    #[test]
    fn a_start_that_began_a_line_says_from_which_turn_in_its_row() {
        use serde_json::json;
        let m = crate::Machine::load(Default::default());
        let l = |v: serde_json::Value| -> crate::Line { serde_json::from_value(v).unwrap() };
        let began = |at: &str, more: serde_json::Value| {
            let mut v = json!({"ev": "act", "at": at, "agent": "orch", "is": "began", "text": "start", "kind": "claude", "model": "claude-opus-5-5"});
            v.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
            l(v)
        };
        let ended = |at: &str, session: &str, end: u64| l(json!({"ev": "turn_ended", "at": at, "agent": "orch", "text": {"file": format!("/h/{session}.jsonl"), "offset": end - 10, "len": 10}}));
        let sha = |c: &str| c.repeat(64);
        let mut b = crate::graph::Builder::default();
        let lines = [
            began("2026-11-24T10:00:00Z", json!({"phase": "start", "session": "s1", "system_sha": sha("a")})),
            ended("2026-11-24T10:05:00Z", "s1", 1000),
            ended("2026-11-24T10:10:00Z", "s1", 2000),
            began("2026-11-24T10:12:00Z", json!({"phase": "system", "session": "c0ff", "from": "s1", "cut": "1100", "system": "2", "replace": "", "system_sha": sha("b")})),
            began("2026-11-24T10:20:00Z", json!({"phase": "restart", "session": "c0ff", "system": "2", "replace": "", "system_sha": sha("b")})),
            began("2026-11-24T10:30:00Z", json!({"phase": "system", "session": "c0ff", "system": "3", "replace": "", "system_sha": sha("c")})),
        ];
        for (i, x) in lines.into_iter().enumerate() {
            b.after(&m, i, &[x]);
        }
        let rows = b.graph.prompt("orch", None);
        let starts: Vec<&str> = rows[0].rows.iter().flat_map(|v| v.rows.iter().map(|s| s.text.as_str())).collect();
        assert_eq!(
            starts,
            [
                "2026-11-24T10:00:00Z: start: claude claude-opus-5-5",
                "2026-11-24T10:12:00Z: system: claude claude-opus-5-5, system prompt 2; branch 2, from turn 1 of branch 1, after byte 1100 of its session",
                "2026-11-24T10:20:00Z: restart: claude claude-opus-5-5, system prompt 2",
                "2026-11-24T10:30:00Z: system: claude claude-opus-5-5, system prompt 3",
            ]
        );
    }

    /// An agent's system prompt, the node his System prompt press opens (fix 4; accounts-lead N-1446 p2504): what its
    /// starts gave Claude Code, each text once with its starts, the requests before its first start under the standing
    /// text no line says; Claude Code's own blocks version by version, but what a start in place of them sent, which
    /// says it replaced them; each version counted in requests, turns and steps; and a version's requests alone, under
    /// their turns under their steps. Of orch and x on made-up edges, the rows written out apart from the code.
    #[test]
    fn a_system_prompt_counts_each_version_in_requests_turns_and_steps_and_gives_one_versions_requests_under_them() {
        let g = prompts();
        let lines = |rows: &[Row]| {
            let mut out = Vec::new();
            shown(rows, 0, &mut out);
            out
        };
        let orch = g.prompt("orch", None);
        assert_eq!(
            lines(&orch),
            [
                "started with: 4 versions",
                "  the standing text, appended; no start line says which version: carried in 1 request, 1 turn, 0 steps",
                "  the standing text, appended: carried in 2 requests, 1 turn, 1 step · line 3",
                "    2026-11-22T09:00:00Z: start: claude claude-opus-5-5, by lead · line 3",
                "  version 2, appended: carried in 1 request, 1 turn, 1 step · line 4",
                "    2026-11-22T09:10:00Z: system: claude claude-opus-5-5, system prompt 2 · line 4",
                "  version 3, in place of Claude Code's own: carried in 1 request, 0 turns, 1 step · line 5",
                "    2026-11-22T09:20:00Z: system: claude claude-opus-5-5, system prompt 3 in place of Claude Code's own · line 5",
                "Claude Code's own: 2 blocks, 4 versions",
                "  block 1: 1 version",
                "    version 1: carried in 4 requests, 3 turns, 2 steps · s1:2",
                "  block 2: 3 versions",
                "    version 1: carried in 2 requests, 2 turns, 1 step · s1:2",
                "    version 2: carried in 1 request, 1 turn, 1 step; 1 word removed, 1 added · s1:8",
                "    version 3: carried in 1 request, 1 turn, 1 step; 1 word added · s1:12",
                "  2026-11-22T09:20:00Z: replaced by your version 3 · line 5",
            ]
        );
        let mut of = Vec::new();
        ofs(&orch, &mut of);
        assert_eq!(
            of,
            [NOT_SAID, "text:std", "start:orch@3", "text:v2", "start:orch@4", "text:v3", "start:orch@5", "text:cc1", "text:cc1", "text:cc2a", "text:cc2a", "text:cc2b", "text:cc2c", "start:orch@5"],
            "each row opens its text or its start"
        );
        let r = |when: &str, n: usize| format!("    2026-11-22T{when}Z: claude-opus-5-5, effort high; no tokens: the session holds no response to it · s1:{n}");
        assert_eq!(
            lines(&g.prompt("orch", Some("text:std"))),
            ["step for N-1.p1: 2 requests in 1 turn".to_string(), "  turn ended 2026-11-22T09:05:00Z: 2 requests · line 10".into(), r("09:00:05", 5), r("09:00:30", 8)]
        );
        assert_eq!(
            lines(&g.prompt("orch", Some("text:cc1"))),
            [
                "no step: 1 request in 1 turn".to_string(),
                "  turn ended 2026-11-22T08:59:50Z: 1 request · line 2".into(),
                r("08:59:30", 2),
                "step for N-1.p1: 2 requests in 1 turn".into(),
                "  turn ended 2026-11-22T09:05:00Z: 2 requests · line 10".into(),
                r("09:00:05", 5),
                r("09:00:30", 8),
                "step for N-1.p2: 1 request in 1 turn".into(),
                "  turn ended 2026-11-22T09:15:00Z: 1 request · line 20".into(),
                r("09:10:05", 12),
            ],
            "block 1 in each request but the one his version 3 replaced it in"
        );
        assert_eq!(lines(&g.prompt("orch", Some("text:v3"))), ["step for N-1.p3: 1 request".to_string(), "  no turn end yet: 1 request".into(), r("09:20:05", 15)]);
        assert_eq!(lines(&g.prompt("orch", Some(NOT_SAID))), ["no step: 1 request in 1 turn".to_string(), "  turn ended 2026-11-22T08:59:50Z: 1 request · line 2".into(), r("08:59:30", 2)]);
        assert_eq!(
            lines(&g.prompt("x", None)),
            [
                "started with: 1 version",
                "  the standing text, appended; no start line says which version: carried in 1 request, 0 turns, 0 steps",
                "Claude Code's own: 1 block, 1 version",
                "  block 1: 1 version",
                "    version 1: carried in 1 request, 0 turns, 0 steps · sx:1",
            ],
            "x's own, none of orch's"
        );
        assert_eq!((g.prompt("orch", Some("text:never")), g.prompt("nobody", None)), (Vec::new(), Vec::new()), "a version or an agent it has not");
    }

    /// An agent's folder (the file view, paper-research N-1446 p1441, p1530): its starts, its loaded texts version by
    /// version with the words that changed into each in its own sessions, its reads, its sessions' requests by model
    /// and effort and their system blocks, its commits and jobs, the newest first, each with its worktree or the words
    /// that there is none; each row saying where it comes from, and opening its node. Of w and x on made-up edges, the
    /// rows written out apart from the code.
    #[test]
    fn an_agents_folder_holds_its_starts_texts_reads_sessions_commits_and_jobs_each_with_its_source() {
        let g = folders();
        let lines = |agent: &str| {
            let mut out = Vec::new();
            shown(&g.folder(agent), 0, &mut out);
            out
        };
        let w = [
            "kind and brief: 1 start",
            "  start: claude claude-opus-5-5, effort high, in /d/w, by lead · line 3",
            "    brief: N-9.p2 · line 3",
            "    session s1 · line 3",
            "loaded texts: 2 files, 4 versions",
            "  /m/MEMORY.md: 3 versions",
            "    version 1: 1 request ran with it · s1:2",
            "    version 2: 2 requests ran with it; 2 words added · s1:20",
            "    version 3: 1 request ran with it · s2:2",
            "  /w/CLAUDE.md: 1 version",
            "    version 1: 4 requests ran with it · s1:3",
            "read: 2 files, 3 reads",
            "  /w/a.rs: read 2 times · s2:7",
            "  /w/b.rs: read 1 time · s1:31",
            "sessions: 2, with 4 requests",
            "  session s1: 3 requests · line 2",
            "    claude-opus-5-5, effort high: 2 requests, the last at 2026-11-22T09:00:07Z; 200 prompt tokens, 7 out · s1:4",
            "    claude-sonnet-5-5, effort medium: 1 request, the last at 2026-11-22T09:00:08Z; no tokens: the session holds no response to them · s1:22",
            "    system prompt blocks: 2 blocks",
            "      block 1: 2 versions",
            "        version 1: carried by 1 request · s1:4",
            "        version 2: carried by 2 requests; 1 word removed, 1 added · s1:21",
            "      block 2: 1 version",
            "        version 1: carried by 3 requests · s1:4",
            "  session s2: 2 requests · line 5",
            "    claude-opus-5-5, effort high: 2 requests, the last at 2026-11-22T10:00:02Z; 195 prompt tokens, 3 out · s2:5",
            "    system prompt blocks: 2 blocks",
            "      block 1: 1 version",
            "        version 1: carried by 1 request · s2:6",
            "      block 2: 1 version",
            "        version 1: carried by 1 request · s2:6",
            "commits and jobs: 2 commits, 2 jobs",
            "  commits: 2",
            "    c2bbbbbb the fix · commit c2bbbbbb",
            "      on repo/edge · commit c2bbbbbb",
            "      on repo/main · commit c2bbbbbb",
            "      worktree not known",
            "    c1aaaaaa point 3: the words · commit c1aaaaaa",
            "      on repo/edge · commit c1aaaaaa",
            "      made in /w/t · commit c1aaaaaa",
            "  jobs: 2",
            "    job 9-2: test coordd · line 45",
            "      worktree not said",
            "      on commit c2bbbbbb · line 45",
            "    job 9-1: test docket-machine: rc 0, 33 passed, 0 failed · line 40",
            "      in /w/t · line 40",
            "      on commit c1aaaaaa · line 40",
        ];
        assert_eq!(lines("w"), w, "w's folder");
        let folder = g.folder("w");
        let at = |path: &str| folder[2].rows.iter().find(|r| r.of == path).and_then(|r| r.when.clone());
        assert_eq!((at("file:/w/a.rs"), at("file:/w/b.rs")), (Some("2026-11-22T10:00:03Z".into()), Some("2026-11-22T09:00:10Z".into())), "a read row's time is its last read's");
        let mut of = Vec::new();
        ofs(&folder, &mut of);
        assert_eq!(
            of,
            [
                "start:w@3", "N-9.p2", "session:s1",
                "file:/m/MEMORY.md", "text:m1", "text:m2", "text:m3", "file:/w/CLAUDE.md", "text:c1",
                "file:/w/a.rs", "file:/w/b.rs",
                "session:s1", "request:r1", "request:r3", "text:sys1", "text:sys1", "text:sys2", "text:sysB", "text:sysB",
                "session:s2", "request:r2", "text:sys2", "text:sys2", "text:sysB", "text:sysB",
                "commit:c2bbbbbb", "branch:repo/edge", "branch:repo/main", "commit:c1aaaaaa", "branch:repo/edge", "worktree:/w/t",
                "job:9-2", "commit:c2bbbbbb", "job:9-1", "worktree:/w/t", "commit:c1aaaaaa",
            ],
            "each row opens its node: a version its text, a model's row its first request, a block its first version"
        );
        let mut said = Vec::new();
        words(&folder, &mut said);
        assert_eq!(
            said,
            [
                ("version 2: 2 requests ran with it; 2 words added", "2 words added: - one {+- two+}"),
                ("version 2: carried by 2 requests; 1 word removed, 1 added", "1 word removed, 1 added: standing, [-first-] {+second+}"),
            ],
            "the words behind a version are its own sessions' `became`: x's next version of the memory file is not w's"
        );
        let memory = &folder[1].rows[0].rows;
        let when: Vec<Option<&str>> = memory.iter().map(|v| v.when.as_deref()).collect();
        assert_eq!(when, [Some("2026-11-22T09:00:02Z"), Some("2026-11-22T09:00:06Z"), Some("2026-11-22T10:00:01Z")], "each version at its first load");
        let x = [
            "kind and brief: no start line of it yet, so its sessions' briefs",
            "  brief of session sx: the first user message, 120 bytes · sx:1",
            "loaded texts: 1 file, 2 versions",
            "  /m/MEMORY.md: 2 versions",
            "    version 1: 0 requests ran with it · sx:2",
            "    version 2: 1 request ran with it; 1 word removed · sx:8",
            "read: 1 file, 1 read",
            "  /w/c.rs: read 1 time · sx:10",
            "sessions: 1, with 1 request",
            "  session sx: 1 request · line 6",
            "    claude-opus-5-5, effort low: 1 request, the last at 2026-11-22T09:30:06Z; no tokens: the session holds no response to them · sx:9",
            "    system prompt blocks: 1 block",
            "      block 1: 1 version",
            "        version 1: carried by 1 request · sx:9",
            "commits and jobs: 1 commit, 0 jobs",
            "  commits: 1",
            "    c3cccccc · commit c3cccccc",
            "      worktree not known",
            "  jobs: 0",
        ];
        assert_eq!(lines("x"), x, "x's folder: no start line, so its session's brief");
        assert!(g.folder("nobody").is_empty(), "an agent the graph has not");
        let long = "2102 words removed, 2102 added (too many to compare word by word: written as removed, then added): [-p q0";
        assert_eq!(words_head(long), "2102 words removed, 2102 added (too many to compare word by word: written as removed, then added)");
        let same = "the same words: only their spacing or line breaks changed";
        assert_eq!(words_head(same), same, "no count, so the words whole");
    }

    /// A session's row says where the record names it, its `session_of` edge's line, though a commit that came from it
    /// made its node first (git's round reads before the side; docket-edge5's row said "commit 330a5a9b", p1681).
    #[test]
    fn a_sessions_row_says_where_the_record_names_it() {
        let mut g = Graph::default();
        put(&mut g, ("commit:c9", NodeKind::Commit), ("session:s9", NodeKind::Session), EdgeKind::CameFrom, Source::Commit("c9".into()), "09:00:00");
        put(&mut g, ("session:s9", NodeKind::Session), ("agent:w", NodeKind::Agent), EdgeKind::SessionOf, Source::Line(7), "09:00:01");
        assert_eq!(g.get("session:s9").map(|n| n.source.clone()), Some(Source::Commit("c9".into())), "its node made by the commit");
        let mut out = Vec::new();
        shown(&g.folder("w"), 0, &mut out);
        assert!(out.contains(&"  session s9: 0 requests · line 7".to_string()), "where the record names it: {out:#?}");
    }

    /// A session's requests alone (paper-research N-1446 p1642, 3): each sent, in time, once, saying when, its model
    /// and effort, its tokens and the blocks it carried by their numbers in that session's folder, from where its
    /// session's line says; r2, which both of w's session files carry, in each, its blocks numbered as each numbers
    /// them; none of a session that is not the agent's, or that is no session.
    #[test]
    fn a_sessions_requests_are_given_alone_each_with_the_blocks_it_carried() {
        let g = folders();
        let lines = |agent: &str, session: &str| {
            let mut out = Vec::new();
            shown(&g.session_requests(agent, session), 0, &mut out);
            out
        };
        assert_eq!(
            lines("w", "session:s1"),
            [
                "2026-11-22T09:00:03Z: claude-opus-5-5, effort high; 100 prompt tokens, 5 out; carried block 1 version 1, block 2 version 1 · s1:4",
                "2026-11-22T09:00:07Z: claude-opus-5-5, effort high; 100 prompt tokens, 2 out; carried block 1 version 2, block 2 version 1 · s1:21",
                "2026-11-22T09:00:08Z: claude-sonnet-5-5, effort medium; no tokens: the session holds no response to it; carried block 1 version 2, block 2 version 1 · s1:22",
            ]
        );
        assert_eq!(
            lines("w", "session:s2"),
            [
                "2026-11-22T09:00:07Z: claude-opus-5-5, effort high; 100 prompt tokens, 2 out; carried block 1 version 1, block 2 version 1 · s2:5",
                "2026-11-22T10:00:02Z: claude-opus-5-5, effort high; 95 prompt tokens, 1 out; carried block 1 version 1, block 2 version 1 · s2:6",
            ]
        );
        let of: Vec<String> = g.session_requests("w", "session:s1").into_iter().map(|r| r.of).collect();
        assert_eq!(of, ["request:r1", "request:r2", "request:r3"], "each row opens its request");
        assert!(g.session_requests("x", "session:s1").is_empty(), "s1 is not x's");
        assert!(g.session_requests("w", "N-9.p2").is_empty(), "no session");
        assert_eq!(g.folder("w").len(), 5, "the folder as before");
    }
}
