//! The graph's kept index (paper-research N-1446 p1828, (b)): what [`Graph::ask_times_at`], [`Graph::now`],
//! [`Graph::costs_at`] and [`Graph::his_after`] read, kept as the edges come, so that each reads only the nodes it
//! names and no read walks every edge. Each whole pass stays as the kept read's reference (`*_walked`), and each kept
//! read equals it row for row (tests/kept.rs; coordd's `a_coords_record_now` on a record's copy).
//!
//! The index catches up with the edges added since its last read when it is next read ([`Kept::catch_up`]), so what
//! adds edges (the record's builder, git's side, the sessions' side) calls nothing of its own; by then the edges are
//! dated (`Graph::date_from` runs as each group is made). An edge is never changed once in, and edges are taken out
//! only by [`Graph::retain_edges`] (the sessions' side makes a request's `for` again), which moves the index with
//! them. An edge list changed some other way under the index shows in its last edge, and the index is made again.
//!
//! What it keeps: each node's edges, and by kind those of a node with many ([`BIG`]); the edges at each node whose other
//! end is a commit; each session's lines with a time; each post's writers; by ticket and post number, the post's first
//! handing to an agent not his, and the posts answering it, and from each post number on the least of those (made when
//! first read, dropped when one of their inputs changes); the jobs, each job's ends and each commit's first branch; and
//! for the costs, what each request's step began
//! with ([`Made`]), made again only for a request whose own inputs changed: its `for`, its session or the session's
//! agent, a giving at or before it of its post to its agent, or one of the posts that giving brought turning out to
//! wake its agent, to be one of his asks, or to name one; and what his asks cost, summed as each step is made
//! ([`Sum`]): a request's part comes out when its step is made again or its node is about to change
//! ([`Kept::node_changing`]), and goes back in with what it holds then.

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::Hash;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::graph::{Cost, Course, Edge, EdgeKind, Graph, Node, NodeKind, Reached, Source, Spent, Tokens, by_then};
use crate::graph_slice::Layer;
use crate::line::At;

/// An id's number in the index.
type Id = u32;

/// His agent id.
const OWNER: &str = "agent:owner";

/// A time to the second, as `costs_at` compares a request's with a giving's.
fn second(t: &str) -> &str {
    t.get(..19).unwrap_or(t)
}

/// A node with more edges than this keeps them by kind as well ([`Kept::big`]), so a read of one kind visits only
/// those: his own node, a build's, an agent's (its `handed` most of all), a post given at many givings (paper-research
/// N-1446 p1896). Small in the crate's tests, so their graphs read both ways.
const BIG: usize = if cfg!(test) { 4 } else { 64 };

/// Edge I into GROUPS under its KIND, in order.
fn group_in(groups: &mut Vec<(EdgeKind, Vec<u32>)>, kind: EdgeKind, i: u32) {
    match groups.iter_mut().find(|(k, _)| *k == kind) {
        Some((_, l)) => l.push(i),
        None => groups.push((kind, vec![i])),
    }
}

/// A [`Shards`] map is split into two to this many tables.
const SHARD_BITS: u32 = 6;

/// A key's table in a [`Shards`] map: the top [`SHARD_BITS`] of its bits mixed, the same for a key and what it is
/// borrowed as (an `Arc<str>`'s is its `str`'s).
trait Shard {
    fn shard(&self) -> usize;
}

fn mix(x: u64) -> usize {
    (x.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> (64 - SHARD_BITS)) as usize
}

impl Shard for u32 {
    fn shard(&self) -> usize {
        mix(*self as u64)
    }
}

impl Shard for (u32, u32) {
    fn shard(&self) -> usize {
        mix(((self.0 as u64) << 32) | self.1 as u64)
    }
}

impl Shard for (u32, usize) {
    fn shard(&self) -> usize {
        mix(((self.0 as u64) << 32) ^ self.1 as u64)
    }
}

impl Shard for str {
    /// Its last eight bytes or fewer, and its length: where ids differ (a post's number, a hash, a uuid's end).
    fn shard(&self) -> usize {
        let b = self.as_bytes();
        mix(b[b.len().saturating_sub(8)..].iter().fold(b.len() as u64, |h, c| (h << 8) | *c as u64))
    }
}

impl Shard for Arc<str> {
    fn shard(&self) -> usize {
        (**self).shard()
    }
}

/// A map split by its keys ([`Shard`]) into 2^[`SHARD_BITS`] tables, each made bigger on its own as it fills. A table
/// made bigger moves all it holds at once: on research's graph, the index's ids or its steps made so moved, whole, were
/// most of each of its longest catch-ups (up to 64 ms under coordd's lock); split, each move is a sixty-fourth of
/// that (paper-research N-1446 p2072 1). Read and written as a `HashMap` is, by the calls the index makes.
#[derive(Clone)]
struct Shards<K, V>(Box<[HashMap<K, V>]>);

impl<K, V> Default for Shards<K, V> {
    fn default() -> Self {
        Shards((0..1 << SHARD_BITS).map(|_| HashMap::new()).collect())
    }
}

impl<K: Hash + Eq + Shard, V> Shards<K, V> {
    fn get<Q: Hash + Eq + Shard + ?Sized>(&self, k: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.0[k.shard()].get(k)
    }

    fn get_mut<Q: Hash + Eq + Shard + ?Sized>(&mut self, k: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
    {
        self.0[k.shard()].get_mut(k)
    }

    fn contains_key<Q: Hash + Eq + Shard + ?Sized>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.0[k.shard()].contains_key(k)
    }

    fn remove<Q: Hash + Eq + Shard + ?Sized>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        self.0[k.shard()].remove(k)
    }

    fn insert(&mut self, k: K, v: V) -> Option<V> {
        let i = k.shard();
        self.0[i].insert(k, v)
    }

    fn entry(&mut self, k: K) -> Entry<'_, K, V> {
        let i = k.shard();
        self.0[i].entry(k)
    }

    fn values(&self) -> impl Iterator<Item = &V> {
        self.0.iter().flat_map(|m| m.values())
    }

    fn capacity(&self) -> usize {
        self.0.iter().map(|m| m.capacity()).sum()
    }
}

/// The bits of a place within its chunk ([`Chunked`]): 4,096 to a chunk, in the crate's tests too, as the saved graph's
/// timed test is one; a leg of its own crosses chunks (`tests::kept`).
pub(crate) const CHUNK_BITS: u32 = 12;

/// A list kept in chunks of 2^[`CHUNK_BITS`], each its own allocation, so one pushed past the last makes a new chunk
/// and moves nothing. A `Vec` made bigger moves all it holds at once: on research's graph the index's lists by id
/// (each id's name, kind, node and edges) were made bigger in one catch-up, by 15 MB at once as the ids came to 2^18,
/// in the longest catch-up of the side's round off the lock; each such move is twice the last as the graph grows
/// (paper-research N-1446 p2072 1). Read by place as a slice is.
#[derive(Clone)]
pub(crate) struct Chunked<T>(Vec<Vec<T>>);

impl<T> Default for Chunked<T> {
    fn default() -> Self {
        Chunked(Vec::new())
    }
}

impl<T> Chunked<T> {
    pub(crate) fn len(&self) -> usize {
        self.0.last().map_or(0, |c| ((self.0.len() - 1) << CHUNK_BITS) + c.len())
    }

    pub(crate) fn push(&mut self, x: T) {
        if self.0.last().is_none_or(|c| c.len() == 1 << CHUNK_BITS) {
            self.0.push(Vec::with_capacity(1 << CHUNK_BITS));
        }
        self.0.last_mut().expect("a chunk").push(x);
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        self.0.iter().flatten()
    }

    /// The first N kept, the rest dropped (edges taken out of the graph, [`Kept::after_removing`]).
    pub(crate) fn truncate(&mut self, n: usize) {
        let chunks = n.div_ceil(1 << CHUNK_BITS);
        self.0.truncate(chunks);
        if let Some(c) = self.0.last_mut() {
            c.truncate(n - ((chunks - 1) << CHUNK_BITS));
        }
    }

    /// Its own heap: each chunk's room and the chunks' list ([`Kept::bytes`]).
    fn bytes(&self) -> usize {
        self.0.capacity() * std::mem::size_of::<Vec<T>>() + self.0.iter().map(|c| c.capacity() * std::mem::size_of::<T>()).sum::<usize>()
    }
}

impl<T> std::ops::Index<usize> for Chunked<T> {
    type Output = T;

    fn index(&self, i: usize) -> &T {
        &self.0[i >> CHUNK_BITS][i & ((1 << CHUNK_BITS) - 1)]
    }
}

impl<T> std::ops::IndexMut<usize> for Chunked<T> {
    fn index_mut(&mut self, i: usize) -> &mut T {
        &mut self.0[i >> CHUNK_BITS][i & ((1 << CHUNK_BITS) - 1)]
    }
}

/// The graph's parts the index reads: its nodes and its edges.
pub(crate) struct Parts<'g> {
    pub nodes: &'g [Node],
    pub edges: &'g [Edge],
}

/// The kept index of a graph, behind a lock of its own: a read through `&Graph` catches it up.
#[derive(Default)]
pub struct KeptCell(Mutex<Kept>);

impl Clone for KeptCell {
    fn clone(&self) -> KeptCell {
        KeptCell(Mutex::new(self.lock().clone()))
    }
}

impl std::fmt::Debug for KeptCell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KeptCell")
    }
}

impl KeptCell {
    /// The index, made again from nothing if a read panicked while holding it (it may be half caught up).
    pub(crate) fn lock(&self) -> MutexGuard<'_, Kept> {
        match self.0.lock() {
            Ok(k) => k,
            Err(e) => {
                let mut k = e.into_inner();
                *k = Kept::default();
                self.0.clear_poison();
                k
            }
        }
    }

    pub(crate) fn get_mut(&mut self) -> &mut Kept {
        if self.0.is_poisoned() {
            self.0.clear_poison();
            *self.0.get_mut().unwrap_or_else(|e| e.into_inner()) = Kept::default();
        }
        self.0.get_mut().unwrap_or_else(|e| e.into_inner())
    }
}

/// A session's lines with a time, each once: in the order they came, sorted when next read if one came out of order.
#[derive(Clone, Default)]
struct Lines {
    at: Vec<(Box<str>, usize)>,
    unsorted: bool,
}

impl Lines {
    fn push(&mut self, w: &str, n: usize) {
        if let Some((lw, ln)) = self.at.last() {
            if &**lw == w && *ln == n {
                return;
            }
            if (&**lw, *ln) > (w, n) {
                self.unsorted = true;
            }
        }
        self.at.push((w.into(), n));
    }

    /// The last line by UPTO (any, with none): the latest time, the last line at it.
    fn last(&mut self, upto: Option<&str>) -> Option<(At, usize)> {
        if self.unsorted {
            self.at.sort();
            self.at.dedup();
            self.unsorted = false;
        }
        let i = self.at.partition_point(|(w, _)| upto.is_none_or(|u| &**w <= u));
        i.checked_sub(1).map(|i| (self.at[i].0.to_string(), self.at[i].1))
    }
}

/// What a request's step began with, as last made ([`Kept::make`]): its agent, the giving lines it began with, the asks
/// it handed and those its posts name, its node's place, and whether its part is in the sums ([`tally`]).
#[derive(Clone)]
struct Made {
    agent: Id,
    lines: Vec<usize>,
    handed: BTreeSet<Id>,
    named: BTreeSet<Id>,
    node: Option<usize>,
    summed: bool,
}

/// What one of his asks cost as the made requests sum it ([`tally`]): how many of its `handed` requests count for
/// another ask too, and its rows, handed and named apart, each one agent's at one model and effort.
#[derive(Clone, Default)]
struct Sum {
    shared: usize,
    handed: Vec<Tally>,
    named: Vec<Tally>,
}

/// A row of a [`Sum`]: an agent's requests at one model and effort (the request node's `what`), how many carried
/// tokens, and theirs.
#[derive(Clone)]
struct Tally {
    agent: Id,
    what: String,
    requests: usize,
    carried: usize,
    tokens: Tokens,
}

/// Request M's part in SUMS, with what its node N holds (its model and effort, its tokens), put in (ADD) or taken out,
/// as `Graph::sum_costs` counts a request: for each ask its step handed (shared when it handed more than one), and apart
/// for each it only names. A row no request counts in any more goes.
fn tally(sums: &mut HashMap<Id, Sum>, m: &Made, n: &Node, add: bool) {
    let shared = m.handed.len() > 1;
    let named = m.named.iter().filter(|a| !m.handed.contains(a));
    for (ask, apart) in m.handed.iter().map(|a| (*a, false)).chain(named.map(|a| (*a, true))) {
        let s = sums.entry(ask).or_default();
        if shared && !apart {
            debug_assert!(add || s.shared > 0, "a request's part taken out of a sum it is not in");
            s.shared = if add { s.shared + 1 } else { s.shared.saturating_sub(1) };
        }
        let rows = if apart { &mut s.named } else { &mut s.handed };
        let i = rows.iter().position(|x| x.agent == m.agent && x.what == n.what).unwrap_or_else(|| {
            rows.push(Tally { agent: m.agent, what: n.what.clone(), requests: 0, carried: 0, tokens: Tokens::default() });
            rows.len() - 1
        });
        let row = &mut rows[i];
        if add {
            row.requests += 1;
            if let Some(t) = n.tokens {
                row.carried += 1;
                row.tokens += t;
            }
        } else {
            debug_assert!(row.requests > 0, "a request's part taken out of a row it is not in");
            row.requests = row.requests.saturating_sub(1);
            if let Some(t) = n.tokens {
                row.carried = row.carried.saturating_sub(1);
                row.tokens -= t;
            }
            if row.requests == 0 {
                rows.swap_remove(i);
            }
        }
    }
}

/// The costs' side of the index: what `costs_at` reads, by node.
#[derive(Clone, Default)]
struct Costs {
    /// Each session's agent (`session_of`), the last given.
    agent_of: HashMap<Id, Id>,
    /// Each request's sessions (`sent`), in order, and each session's requests.
    session: Shards<Id, Vec<Id>>,
    requests: HashMap<Id, Vec<Id>>,
    /// The posts each giving line handed each agent, in order; each post's givings to each agent, line and time; the
    /// agents each post was handed to.
    brought: Shards<(Id, usize), Vec<Id>>,
    givings: Shards<(Id, Id), Vec<(usize, Option<At>)>>,
    given_to: Shards<Id, Vec<Id>>,
    /// Each post and agent it woke or asks.
    woke: HashSet<(Id, Id)>,
    /// The posts with an `asks`, and those he wrote.
    asking: HashSet<Id>,
    his: HashSet<Id>,
    /// What each post names (`named`), and the posts naming each.
    names: Shards<Id, Vec<Id>>,
    named_by: Shards<Id, Vec<Id>>,
    /// Each request's `for`s (the post, when), and the requests for each post.
    fors: Shards<Id, Vec<(Id, Option<At>)>>,
    fors_into: Shards<Id, Vec<Id>>,
    /// Each request as last made, the requests that began with each agent's giving line, and those to make again.
    made: Shards<Id, Made>,
    chose: Shards<(Id, usize), HashSet<Id>>,
    dirty: HashSet<Id>,
    /// For each agent, its made requests' latest time to the second, and whether one of them has a `for` with no time:
    /// a giving later than that and none such begins no step already made.
    last_sent: HashMap<Id, String>,
    untimed: HashSet<Id>,
    /// What each of his asks cost, summed from the made requests ([`tally`]).
    sums: HashMap<Id, Sum>,
}

/// The kept index ([`KeptCell`]).
#[derive(Clone, Default)]
pub(crate) struct Kept {
    /// The edges and nodes in, and the last edge's ends and kind.
    upto: usize,
    nodes_upto: usize,
    last: Option<(String, String, EdgeKind)>,
    /// Each id by its number, each number by its id, and the kind and place of an id that is a node.
    ids: Shards<Arc<str>, Id>,
    names: Chunked<Arc<str>>,
    kinds: Chunked<Option<NodeKind>>,
    nodes: Chunked<Option<usize>>,
    /// Each id's edges by their place, either end (an edge from a node to itself twice, as `around` has it), in order;
    /// each edge's kind is the edge's own.
    by: Chunked<Vec<u32>>,
    /// The same by kind, each kind's in order, for an id with more than [`BIG`]: what a read of some kinds visits.
    big: Shards<Id, Vec<(EdgeKind, Vec<u32>)>>,
    /// The edges at each id whose other end is a commit (from X to X, X itself), in order: those of a commit whose node
    /// came after them put in when it came.
    commits: Shards<Id, Vec<u32>>,
    /// Each session's lines with a time (`Source::Session`), by the session as its edges' sources name it.
    sessions: HashMap<String, Lines>,
    /// The writers of each post or ticket (`wrote`), in order, with when.
    writers: Shards<Id, Vec<(Id, Option<At>)>>,
    /// By ticket and post number: the post's first handing to an agent not his (when, to, from), and each post
    /// answering it (`answered`, `answered_upto`; from, when).
    handed: HashMap<String, BTreeMap<u32, (At, Id, Id)>>,
    answered: HashMap<String, BTreeMap<u32, Vec<(Id, Option<At>)>>>,
    /// Of a ticket's first handings, from each post number on, the least by (when, to, from) (`Course::first_handed`),
    /// whatever the moment: made when first read, and dropped when one of the ticket's changes (paper-research N-1446
    /// p2072 2).
    handed_least: HashMap<String, Vec<(u32, (At, Id, Id))>>,
    /// Of a ticket's first answers by an agent's post, from each post number on, the least by (when, writer, post)
    /// (`Course::first_answered`): over the whole graph, and at the last moment read (`answered_at`; another moment's
    /// made again). Each made when first read, and dropped when one of the ticket's answers changes, or an answering
    /// post's writers or kind (paper-research N-1446 p2072 2).
    answered_least: HashMap<String, Vec<(u32, (At, Id, Id))>>,
    answered_at: Option<String>,
    answered_least_at: HashMap<String, Vec<(u32, (At, Id, Id))>>,
    /// The tickets each post answers (`answered`, `answered_upto`): whose first answers its writers and kind change.
    answers: Shards<Id, Vec<String>>,
    /// The jobs, in the order they came.
    jobs: Vec<Id>,
    /// Each id's `ended` edges from it, their places and other ends, in order (`Course::job_ends`: a job's).
    ended: Shards<Id, Vec<(u32, Id)>>,
    /// Each id's first `on` from it with a time, by time, the first of those at the least (`Course::first_on`: a
    /// commit's branch).
    first_on: Shards<Id, u32>,
    /// The agents' nodes by their places, in the graph's order (`Course::agents`).
    agents: Vec<usize>,
    /// The latest time of an edge in: a moment at or after it is one every edge is by, read as none
    /// ([`KeptView::new`]). After edges are taken out it may be later than any left, which costs only that.
    latest_when: Option<At>,
    costs: Costs,
    /// Each edge's layer by its place, as the slice reads it ([`Layer::by`] of its two ends' kinds and writers, as the
    /// whole graph has them now), and how many edges each layer has. A node's edges are read again when its node comes
    /// after them, or when he becomes or stops being its writer, so that a slice of some layers reads only their edges
    /// and works out no edge's layer (`Graph::slice`; his layer's slice held coordd's lock 0.39 s on research's record
    /// of 24 Nov, docket-edge5 N-1446 p2532). Beside the graph, never in it (p758, p760).
    layers: Chunked<Layer>,
    layer_count: BTreeMap<Layer, usize>,
    /// Each node's writer as the slice reads it (`wrote`): the agent end of the last `wrote`, `took` or `turn_of` at it
    /// whose agent end is an agent's ([`slice_writer`]).
    wrote_by: Shards<Id, Id>,
}

/// The writer edge E (from F to T) gives a node as the slice reads it (`Graph::slice_walked`'s `wrote`): a `wrote`'s
/// or a `took`'s from end, a `turn_of`'s to end, when that end is an agent's; (that agent, the node).
fn slice_writer(e: &Edge, f: Id, t: Id) -> Option<(Id, Id)> {
    let (agent, a, of) = match e.kind {
        EdgeKind::TurnOf => (&e.to, t, f),
        EdgeKind::Wrote | EdgeKind::Took => (&e.from, f, t),
        _ => return None,
    };
    agent.starts_with("agent:").then_some((a, of))
}

impl Kept {
    fn id(&mut self, s: &str) -> Id {
        if let Some(&i) = self.ids.get(s) {
            return i;
        }
        let i = self.names.len() as Id;
        let a: Arc<str> = Arc::from(s);
        self.ids.insert(a.clone(), i);
        self.names.push(a);
        self.kinds.push(None);
        self.nodes.push(None);
        self.by.push(Vec::new());
        i
    }

    pub(crate) fn name(&self, i: Id) -> &str {
        &self.names[i as usize]
    }

    fn kind(&self, i: Id) -> Option<NodeKind> {
        self.kinds[i as usize]
    }

    /// Edge E's layer (from F to T) as the slice reads it, from what the index holds of its ends now.
    fn layer_of(&self, e: &Edge, f: Id, t: Id) -> Layer {
        let owner = self.ids.get(OWNER).copied();
        let his = |x: Id| owner.is_some() && self.wrote_by.get(&x).copied() == owner;
        Layer::by(e, (self.kind(f), his(f)), (self.kind(t), his(t)))
    }

    /// Edge J's layer set to L, the layers' counts moved with it.
    fn set_layer(&mut self, j: usize, l: Layer) {
        let old = std::mem::replace(&mut self.layers[j], l);
        if old != l {
            if let Some(n) = self.layer_count.get_mut(&old) {
                *n -= 1;
            }
            *self.layer_count.entry(l).or_insert(0) += 1;
        }
    }

    /// The layers of X's edges read again: X's node came after them, or he became or stopped being its writer.
    fn relayer(&mut self, p: &Parts, x: Id) {
        for j in self.by[x as usize].clone() {
            let e = &p.edges[j as usize];
            let (Some(&f), Some(&t)) = (self.ids.get(e.from.as_str()), self.ids.get(e.to.as_str())) else { continue };
            let l = self.layer_of(e, f, t);
            self.set_layer(j as usize, l);
        }
    }

    /// Edge I's layer as the slice reads it (caught up).
    pub(crate) fn layer(&self, i: usize) -> Layer {
        self.layers[i]
    }

    /// How many edges each layer has in the whole graph (`Slice::whole`), the layers with none left out.
    pub(crate) fn layer_counts(&self) -> BTreeMap<Layer, usize> {
        self.layer_count.iter().filter(|(_, n)| **n > 0).map(|(l, n)| (*l, *n)).collect()
    }

    /// ID's writer as the slice reads it, by the agent's name ([`slice_writer`]).
    pub(crate) fn writer(&self, id: &str) -> Option<&str> {
        let x = self.ids.get(id)?;
        self.name(*self.wrote_by.get(x)?).strip_prefix("agent:")
    }

    /// The graph P's nodes and edges since the last catch-up, in; the index made again first when the edge list
    /// changed under it; then each request whose inputs changed made again.
    pub(crate) fn catch_up(&mut self, p: &Parts) {
        self.take_in(p);
        self.make_dirty(p);
    }

    /// [`Kept::catch_up`]'s first half: P's nodes and edges since the last, in (the index made again first when the
    /// edge list changed under it), each request whose inputs they change marked to be made again.
    pub(crate) fn take_in(&mut self, p: &Parts) {
        let changed = self.upto > p.edges.len()
            || self.nodes_upto > p.nodes.len()
            || (self.upto > 0 && {
                let e = &p.edges[self.upto - 1];
                self.last.as_ref().is_none_or(|(f, t, k)| *f != e.from || *t != e.to || *k != e.kind)
            });
        if changed {
            *self = Kept::default();
        }
        for i in self.nodes_upto..p.nodes.len() {
            self.node_in(p, i);
        }
        self.nodes_upto = p.nodes.len();
        for i in self.upto..p.edges.len() {
            self.edge_in(p, i);
        }
        self.upto = p.edges.len();
        self.last = p.edges.last().map(|e| (e.from.clone(), e.to.clone(), e.kind));
    }

    /// [`Kept::catch_up`]'s second half: each request marked, made again from P.
    pub(crate) fn make_dirty(&mut self, p: &Parts) {
        for r in self.take_dirty() {
            self.make(p, r);
        }
    }

    /// The requests marked to be made again, unmarked.
    pub(crate) fn take_dirty(&mut self) -> Vec<Id> {
        self.costs.dirty.drain().collect()
    }

    /// Edge I into X's edges, and into its kind's when X has more than [`BIG`] (all of them by kind once it has).
    fn by_in(&mut self, p: &Parts, x: Id, i: u32) {
        let list = &mut self.by[x as usize];
        list.push(i);
        if let Some(groups) = self.big.get_mut(&x) {
            group_in(groups, p.edges[i as usize].kind, i);
        } else if list.len() > BIG {
            let mut groups = Vec::new();
            for j in list.iter() {
                group_in(&mut groups, p.edges[*j as usize].kind, *j);
            }
            self.big.insert(x, groups);
        }
    }

    /// Edge J into the edges at Y whose other end is a commit, in order.
    fn commit_at(&mut self, y: Id, j: u32) {
        let l = self.commits.entry(y).or_default();
        let at = l.partition_point(|i| *i <= j);
        l.insert(at, j);
    }

    /// X's edges of KINDS by their places, in order: off its kinds' lists where it keeps them, else off all of its edges.
    fn of_kinds(&self, x: Id, kinds: &[EdgeKind], edges: &[Edge]) -> Vec<u32> {
        let Some(groups) = self.big.get(&x) else {
            return self.by[x as usize].iter().copied().filter(|j| kinds.contains(&edges[*j as usize].kind)).collect();
        };
        let mut lists = groups.iter().filter(|(k, _)| kinds.contains(k)).map(|(_, l)| l);
        match (lists.next(), lists.next()) {
            (None, _) => Vec::new(),
            (Some(a), None) => a.clone(),
            (Some(a), Some(b)) => {
                let mut all: Vec<u32> = a.iter().chain(b).chain(lists.flatten()).copied().collect();
                all.sort_unstable();
                all
            }
        }
    }

    /// Whether one of X's edges of KINDS is as ANY says: off its kinds' lists where it keeps them, else off all of its
    /// edges, each read where it is kept, nothing collected or sorted (`Course::has_turns`, paper-research N-1446 p2072
    /// 2).
    fn any_of_kinds(&self, x: Id, kinds: &[EdgeKind], edges: &[Edge], any: impl Fn(&Edge) -> bool) -> bool {
        match self.big.get(&x) {
            Some(groups) => groups.iter().filter(|(k, _)| kinds.contains(k)).flat_map(|(_, l)| l).any(|j| any(&edges[*j as usize])),
            None => self.by[x as usize].iter().map(|j| &edges[*j as usize]).filter(|e| kinds.contains(&e.kind)).any(any),
        }
    }

    /// Post X's writers or kind changed: the first answers kept for each ticket it answers dropped
    /// (`Course::first_answered`).
    fn answers_changed_by(&mut self, x: Id) {
        if let Some(tks) = self.answers.get(&x) {
            for tk in tks {
                self.answered_least.remove(tk.as_str());
                self.answered_least_at.remove(tk.as_str());
            }
        }
    }

    fn node_in(&mut self, p: &Parts, i: usize) {
        let n = &p.nodes[i];
        let x = self.id(&n.id);
        let first = self.kinds[x as usize].is_none();
        if self.kinds[x as usize] != Some(n.kind) {
            // whether an answer by it counts reads its kind
            self.answers_changed_by(x);
        }
        self.kinds[x as usize] = Some(n.kind);
        self.nodes[x as usize] = Some(i);
        match n.kind {
            // a post's kind decides whether it is an ask of his and whether what it names counts
            NodeKind::Post => self.dirty_post(x, true),
            NodeKind::Job => self.jobs.push(x),
            NodeKind::Agent => self.agents.push(i),
            // a commit whose edges came before it: each is at a commit at its other end from now on
            NodeKind::Commit if first => {
                for j in self.by[x as usize].clone() {
                    let e = &p.edges[j as usize];
                    let other = if e.from == self.name(x) { e.to.as_str() } else { e.from.as_str() };
                    let y = *self.ids.get(other).expect("an edge's ends have ids");
                    self.commit_at(y, j);
                }
            }
            _ => {}
        }
        // a request made before its node came: its node read from now on
        if self.costs.made.contains_key(&x) {
            self.costs.dirty.insert(x);
        }
        // edges that came before their node: each one's layer reads the node's kind from now on
        if first && !self.by[x as usize].is_empty() {
            self.relayer(p, x);
        }
    }

    fn edge_in(&mut self, p: &Parts, i: usize) {
        let e = &p.edges[i];
        if let Some(w) = &e.when
            && self.latest_when.as_ref().is_none_or(|l| w > l)
        {
            self.latest_when = Some(w.clone());
        }
        let (f, t) = (self.id(&e.from), self.id(&e.to));
        self.by_in(p, f, i as u32);
        self.by_in(p, t, i as u32);
        // at F its other end is T, at T it is F (T itself, from T to T): at a commit where that end is one
        if self.kind(t) == Some(NodeKind::Commit) {
            self.commit_at(f, i as u32);
        }
        if self.kind(f) == Some(NodeKind::Commit) {
            self.commit_at(t, i as u32);
        }
        if let (Source::Session(x, n), Some(w)) = (&e.source, &e.when) {
            self.sessions.entry(x.clone()).or_default().push(w, *n);
        }
        match e.kind {
            EdgeKind::Wrote => {
                self.writers.entry(t).or_default().push((f, e.when.clone()));
                // whether an answer by T counts reads its writer by then
                self.answers_changed_by(t);
            }
            EdgeKind::Handed if e.to != OWNER => {
                if let (Some((tk, Some(n))), Some(w)) = (crate::post_or_ticket(&e.from), &e.when) {
                    let better = match self.handed.get(tk).and_then(|b| b.get(&n)) {
                        Some((bw, bt, bf)) => (w.as_str(), e.to.as_str(), e.from.as_str()) < (bw.as_str(), self.name(*bt), self.name(*bf)),
                        None => true,
                    };
                    if better {
                        self.handed.entry(tk.to_string()).or_default().insert(n, (w.clone(), t, f));
                        self.handed_least.remove(tk);
                    }
                }
            }
            EdgeKind::Answered | EdgeKind::AnsweredUpto => {
                if let Some((tk, Some(n))) = crate::post_or_ticket(&e.to) {
                    self.answered.entry(tk.to_string()).or_default().entry(n).or_default().push((f, e.when.clone()));
                    self.answered_least.remove(tk);
                    self.answered_least_at.remove(tk);
                    let on = self.answers.entry(f).or_default();
                    if !on.iter().any(|x| x == tk) {
                        on.push(tk.to_string());
                    }
                }
            }
            EdgeKind::Ended => self.ended.entry(f).or_default().push((i as u32, t)),
            EdgeKind::On if e.when.is_some() => {
                // the first at the least time stays
                if self.first_on.get(&f).is_none_or(|j| e.when < p.edges[*j as usize].when) {
                    self.first_on.insert(f, i as u32);
                }
            }
            _ => {}
        }
        self.cost_in(e, f, t);
        // its layer as the slice reads it; then the writer it gives a node, if any: when that makes him that node's
        // writer, or no longer, the layers of the node's edges are read again, its own among them
        let l = self.layer_of(e, f, t);
        self.layers.push(l);
        *self.layer_count.entry(l).or_insert(0) += 1;
        if let Some((agent, of)) = slice_writer(e, f, t) {
            let owner = self.ids.get(OWNER).copied();
            let was = self.wrote_by.insert(of, agent);
            if (was.is_some() && was == owner) != (Some(agent) == owner) {
                self.relayer(p, of);
            }
        }
    }

    /// Edge E (from F to T) into the costs' side, each request whose step it may change marked to be made again.
    fn cost_in(&mut self, e: &Edge, f: Id, t: Id) {
        let c = &mut self.costs;
        match e.kind {
            EdgeKind::SessionOf => {
                c.agent_of.insert(f, t);
                c.dirty.extend(c.requests.get(&f).into_iter().flatten().copied());
            }
            EdgeKind::Sent => {
                c.session.entry(t).or_default().push(f);
                c.requests.entry(f).or_default().push(t);
                c.dirty.insert(t);
            }
            EdgeKind::For => {
                c.fors.entry(f).or_default().push((t, e.when.clone()));
                c.fors_into.entry(t).or_default().push(f);
                c.dirty.insert(f);
            }
            EdgeKind::Handed => {
                let Some(line) = e.line() else { return };
                c.brought.entry((t, line)).or_default().push(f);
                c.givings.entry((f, t)).or_default().push((line, e.when.clone()));
                let to = c.given_to.entry(f).or_default();
                if !to.contains(&t) {
                    to.push(t);
                }
                // the steps that began with this giving line, and those of its agent this post was for, when this
                // giving is at or before one of them: a later one begins none made already
                c.dirty.extend(c.chose.get(&(t, line)).into_iter().flatten().copied());
                let late = c.untimed.contains(&t)
                    || match (&e.when, c.last_sent.get(&t)) {
                        (None, _) => true,
                        (Some(w), Some(s)) => second(w) <= s.as_str(),
                        (Some(_), None) => false,
                    };
                if late {
                    let mine: Vec<Id> = c.fors_into.get(&f).into_iter().flatten().copied().filter(|r| c.made.get(r).is_some_and(|m| m.agent == t)).collect();
                    c.dirty.extend(mine);
                }
            }
            EdgeKind::Woke | EdgeKind::Asks => {
                c.woke.insert((f, t));
                let lines: Vec<usize> = c.givings.get(&(f, t)).into_iter().flatten().map(|(l, _)| *l).collect();
                for l in lines {
                    c.dirty.extend(c.chose.get(&(t, l)).into_iter().flatten().copied());
                }
                if e.kind == EdgeKind::Asks {
                    c.asking.insert(f);
                    self.dirty_post(f, true);
                }
            }
            EdgeKind::Wrote if e.from == OWNER => {
                c.his.insert(t);
                self.dirty_post(t, true);
            }
            EdgeKind::Named => {
                c.names.entry(f).or_default().push(t);
                c.named_by.entry(t).or_default().push(f);
                self.dirty_post(f, false);
            }
            _ => {}
        }
    }

    /// Each request whose step may read post P (its own post, or one a giving it began with brought), marked to be
    /// made again; with DEEP (P may have turned into an ask of his), those reading a post that names P too.
    fn dirty_post(&mut self, p: Id, deep: bool) {
        let c = &mut self.costs;
        c.dirty.extend(c.fors_into.get(&p).into_iter().flatten().copied());
        for a in c.given_to.get(&p).into_iter().flatten() {
            for (l, _) in c.givings.get(&(p, *a)).into_iter().flatten() {
                c.dirty.extend(c.chose.get(&(*a, *l)).into_iter().flatten().copied());
            }
        }
        if deep {
            let by: Vec<Id> = c.named_by.get(&p).cloned().unwrap_or_default();
            for x in by {
                self.dirty_post(x, false);
            }
        }
    }

    /// Whether X is one of his asks: a post he wrote with an `asks`.
    fn is_ask(&self, x: Id) -> bool {
        self.costs.his.contains(&x) && self.costs.asking.contains(&x) && self.kind(x) == Some(NodeKind::Post)
    }

    /// Request R's step made again ([`Made`]), as `costs_at` makes it: for each of its `for`s, the giving of that post to
    /// its agent it began with (the last at or before it, to the second), and of what that giving brought each post that
    /// woke its agent or asks it, and its own post; the asks among them, and the asks those posts name. Its part in the
    /// sums comes out as it went in, and goes in again with what its node (P's) holds now.
    pub(crate) fn make(&mut self, p: &Parts, r: Id) {
        if let Some(old) = self.costs.made.remove(&r) {
            for l in &old.lines {
                if let Some(s) = self.costs.chose.get_mut(&(old.agent, *l)) {
                    s.remove(&r);
                }
            }
            if old.summed && let Some(n) = old.node {
                tally(&mut self.costs.sums, &old, &p.nodes[n], false);
            }
        }
        let c = &self.costs;
        // its agent: the last of its sessions with one, as `costs_at`'s `sender` keeps the last
        let Some(agent) = c.session.get(&r).and_then(|ss| ss.iter().rev().find_map(|s| c.agent_of.get(s))).copied() else { return };
        let Some(fors) = c.fors.get(&r).filter(|f| !f.is_empty()) else { return };
        let mut made = Made { agent, lines: Vec::new(), handed: BTreeSet::new(), named: BTreeSet::new(), node: self.nodes[r as usize], summed: false };
        let (mut latest, mut untimed): (Option<&str>, bool) = (None, false);
        for (own, when) in fors {
            let sent = when.as_deref().map(second);
            match sent {
                Some(s) => latest = latest.max(Some(s)),
                None => untimed = true,
            }
            let line = c.givings.get(&(*own, agent)).and_then(|ls| ls.iter().filter(|(_, w)| sent.is_none() || w.as_deref().is_none_or(|w| Some(second(w)) <= sent)).map(|(l, _)| *l).max());
            if let Some(l) = line {
                made.lines.push(l);
            }
            let brought = line.and_then(|l| c.brought.get(&(agent, l)));
            for x in brought.into_iter().flatten().copied().filter(|x| c.woke.contains(&(*x, agent))).chain([*own]) {
                if self.is_ask(x) {
                    made.handed.insert(x);
                }
                if self.kind(x) == Some(NodeKind::Post) {
                    made.named.extend(c.names.get(&x).into_iter().flatten().copied().filter(|y| self.is_ask(*y)));
                }
            }
        }
        let latest = latest.map(str::to_string);
        let c = &mut self.costs;
        if let Some(s) = latest {
            let e = c.last_sent.entry(agent).or_default();
            if *e < s {
                *e = s;
            }
        }
        if untimed {
            c.untimed.insert(agent);
        }
        for l in &made.lines {
            c.chose.entry((agent, *l)).or_default().insert(r);
        }
        if let Some(n) = made.node {
            tally(&mut c.sums, &made, &p.nodes[n], true);
            made.summed = true;
        }
        c.made.insert(r, made);
    }

    /// Node N about to be changed (`Graph::get_mut`): a request made from it has its part in the sums taken out while
    /// the node still holds what went in, and is made again at the next catch-up with what it holds then.
    pub(crate) fn node_changing(&mut self, n: &Node) {
        let Some(&x) = self.ids.get(n.id.as_str()) else { return };
        let c = &mut self.costs;
        let Some(m) = c.made.get_mut(&x) else { return };
        if m.summed {
            tally(&mut c.sums, m, n, false);
            m.summed = false;
        }
        c.dirty.insert(x);
    }

    /// The index's heap by part, near enough (paper-research N-1446 p1865: its size beside the graph's; p1896: by
    /// part): each table at its capacity with a byte of control a slot, a BTreeMap's entries at one and a half times
    /// their size, each string's bytes (an `Arc`'s two counts with its own), a set inside a value at its capacity.
    pub(crate) fn bytes(&self) -> Vec<(&'static str, usize)> {
        use std::mem::size_of;
        fn vec<T>(v: &Vec<T>) -> usize {
            v.capacity() * size_of::<T>()
        }
        fn map<K, V>(m: &HashMap<K, V>) -> usize {
            m.capacity() * (size_of::<K>() + size_of::<V>() + 1)
        }
        fn shards<K: Hash + Eq + Shard, V>(m: &Shards<K, V>) -> usize {
            m.capacity() * (size_of::<K>() + size_of::<V>() + 1) + m.0.len() * size_of::<HashMap<K, V>>()
        }
        fn set<K>(m: &HashSet<K>) -> usize {
            m.capacity() * (size_of::<K>() + 1)
        }
        fn tree<K, V>(m: &BTreeMap<K, V>) -> usize {
            m.len() * (size_of::<K>() + size_of::<V>()) * 3 / 2
        }
        fn time(w: &Option<At>) -> usize {
            w.as_ref().map_or(0, String::capacity)
        }
        let c = &self.costs;
        let ids = shards(&self.ids) + self.names.bytes() + self.names.iter().map(|a| 16 + a.len()).sum::<usize>() + self.kinds.bytes() + self.nodes.bytes();
        let by = self.by.bytes() + self.by.iter().map(vec).sum::<usize>();
        let big = shards(&self.big) + self.big.values().map(|g| vec(g) + g.iter().map(|(_, l)| vec(l)).sum::<usize>()).sum::<usize>();
        let commits = shards(&self.commits) + self.commits.values().map(vec).sum::<usize>();
        let sessions = map(&self.sessions) + self.sessions.iter().map(|(k, l)| k.capacity() + vec(&l.at) + l.at.iter().map(|(w, _)| w.len()).sum::<usize>()).sum::<usize>();
        let writers = shards(&self.writers) + self.writers.values().map(|v| vec(v) + v.iter().map(|(_, w)| time(w)).sum::<usize>()).sum::<usize>();
        let handed = map(&self.handed)
            + self.handed.iter().map(|(k, b)| k.capacity() + tree(b) + b.values().map(|(w, _, _)| w.capacity()).sum::<usize>()).sum::<usize>()
            + map(&self.handed_least)
            + self.handed_least.iter().map(|(k, v)| k.capacity() + vec(v) + v.iter().map(|(_, (w, _, _))| w.capacity()).sum::<usize>()).sum::<usize>();
        let answered = map(&self.answered)
            + self.answered.iter().map(|(k, b)| k.capacity() + tree(b) + b.values().map(|v| vec(v) + v.iter().map(|(_, w)| time(w)).sum::<usize>()).sum::<usize>()).sum::<usize>()
            + [&self.answered_least, &self.answered_least_at].into_iter().map(|m| map(m) + m.iter().map(|(k, v)| k.capacity() + vec(v) + v.iter().map(|(_, (w, _, _))| w.capacity()).sum::<usize>()).sum::<usize>()).sum::<usize>()
            + time(&self.answered_at)
            + shards(&self.answers)
            + self.answers.values().map(|v| vec(v) + v.iter().map(String::capacity).sum::<usize>()).sum::<usize>();
        let inputs = map(&c.agent_of)
            + shards(&c.session)
            + c.session.values().map(vec).sum::<usize>()
            + map(&c.requests)
            + c.requests.values().map(vec).sum::<usize>()
            + shards(&c.brought)
            + c.brought.values().map(vec).sum::<usize>()
            + shards(&c.givings)
            + c.givings.values().map(|v| vec(v) + v.iter().map(|(_, w)| time(w)).sum::<usize>()).sum::<usize>()
            + shards(&c.given_to)
            + c.given_to.values().map(vec).sum::<usize>()
            + set(&c.woke)
            + set(&c.asking)
            + set(&c.his)
            + shards(&c.names)
            + c.names.values().map(vec).sum::<usize>()
            + shards(&c.named_by)
            + c.named_by.values().map(vec).sum::<usize>()
            + shards(&c.fors)
            + c.fors.values().map(|v| vec(v) + v.iter().map(|(_, w)| time(w)).sum::<usize>()).sum::<usize>()
            + shards(&c.fors_into)
            + c.fors_into.values().map(vec).sum::<usize>();
        let made = shards(&c.made)
            + c.made.values().map(|m| vec(&m.lines) + (m.handed.len() + m.named.len()) * size_of::<Id>() * 3 / 2).sum::<usize>()
            + shards(&c.chose)
            + c.chose.values().map(set).sum::<usize>()
            + set(&c.dirty)
            + map(&c.last_sent)
            + c.last_sent.values().map(String::capacity).sum::<usize>()
            + set(&c.untimed);
        let tallies = |rows: &Vec<Tally>| vec(rows) + rows.iter().map(|t| t.what.capacity()).sum::<usize>();
        let sums = map(&c.sums) + c.sums.values().map(|s| tallies(&s.handed) + tallies(&s.named)).sum::<usize>();
        vec![
            ("ids", ids),
            ("each id's edges", by),
            ("by kind", big),
            ("at a commit", commits),
            ("sessions' lines", sessions),
            ("writers", writers),
            ("first handings", handed),
            ("answers", answered),
            ("jobs", vec(&self.jobs) + shards(&self.ended) + self.ended.values().map(vec).sum::<usize>() + shards(&self.first_on)),
            ("agents", vec(&self.agents) + time(&self.latest_when)),
            ("the costs' inputs", inputs),
            ("the steps made", made),
            ("the sums", sums),
            ("layers", self.layers.bytes() + tree(&self.layer_count) + shards(&self.wrote_by)),
        ]
    }

    /// The edges from EDGES on given their times (`Graph::date_from`): the index made again if it holds one of them.
    pub(crate) fn dated_from(&mut self, edges: usize) {
        if edges < self.upto {
            *self = Kept::default();
        }
    }

    /// Before the edges REMOVED (their places in P, in order) are taken out: the index caught up, and their part taken
    /// out of it. Only a request's `for` is ever taken out (the sessions' side makes it again); the index is made again
    /// for any other. Gives the ids whose edges come at or after the first taken out, for [`Kept::after_removing`].
    pub(crate) fn before_removing(&mut self, p: &Parts, removed: &[usize]) -> Option<HashSet<Id>> {
        self.catch_up(p);
        // a session line its `for` came from stays held by another edge of the request from that line (its `sent`)
        let held = |k: &Kept, e: &Edge| {
            !matches!(e.source, Source::Session(..))
                || e.when.is_none()
                || k.ids.get(e.from.as_str()).is_some_and(|f| {
                    k.by[*f as usize].iter().any(|j| {
                        let o = &p.edges[*j as usize];
                        removed.binary_search(&(*j as usize)).is_err() && o.source == e.source && o.when == e.when
                    })
                })
        };
        if removed.iter().any(|i| p.edges[*i].kind != EdgeKind::For || !held(self, &p.edges[*i])) {
            *self = Kept::default();
            return None;
        }
        for i in removed {
            let e = &p.edges[*i];
            let (f, t) = (self.id(&e.from), self.id(&e.to));
            let c = &mut self.costs;
            if let Some(v) = c.fors.get_mut(&f)
                && let Some(k) = v.iter().position(|(x, w)| *x == t && *w == e.when)
            {
                v.remove(k);
            }
            if let Some(v) = c.fors_into.get_mut(&t)
                && let Some(k) = v.iter().position(|x| *x == f)
            {
                v.remove(k);
            }
            c.dirty.insert(f);
        }
        let mut touched = HashSet::new();
        for e in &p.edges[removed[0]..] {
            touched.insert(self.id(&e.from));
            touched.insert(self.id(&e.to));
        }
        Some(touched)
    }

    /// After the edges REMOVED (their old places, in order) were taken out of P: each place in the index moved down by
    /// how many were taken out before it, those taken out gone, in the edges of the ids TOUCHED.
    pub(crate) fn after_removing(&mut self, p: &Parts, removed: &[usize], touched: Option<HashSet<Id>>) {
        let Some(touched) = touched else { return };
        let first = removed[0] as u32;
        let shift = |list: &mut Vec<u32>| {
            let from = list.partition_point(|i| *i < first);
            let tail: Vec<u32> = list.drain(from..).collect();
            for i in tail {
                if let Err(before) = removed.binary_search(&(i as usize)) {
                    list.push(i - before as u32);
                }
            }
        };
        for x in touched {
            shift(&mut self.by[x as usize]);
            for (_, l) in self.big.get_mut(&x).into_iter().flatten() {
                shift(l);
            }
            if let Some(l) = self.commits.get_mut(&x) {
                shift(l);
            }
            // a request's `for` alone is taken out, never an `ended` or an `on`: each only moves down
            let down = |j: &mut u32| {
                let before = removed.partition_point(|r| *r < *j as usize) as u32;
                *j -= before;
            };
            for (j, _) in self.ended.get_mut(&x).into_iter().flatten() {
                down(j);
            }
            if let Some(j) = self.first_on.get_mut(&x) {
                down(j);
            }
        }
        // each edge's layer from the first taken out on moves down, those taken out gone from the counts; a `for` gives
        // no node a writer, so no other layer changes
        let tail: Vec<Layer> = (first as usize..self.layers.len()).map(|j| self.layers[j]).collect();
        self.layers.truncate(first as usize);
        for (k, l) in tail.into_iter().enumerate() {
            if removed.binary_search(&(first as usize + k)).is_ok() {
                if let Some(n) = self.layer_count.get_mut(&l) {
                    *n -= 1;
                }
            } else {
                self.layers.push(l);
            }
        }
        self.upto = p.edges.len();
        self.last = p.edges.last().map(|e| (e.from.clone(), e.to.clone(), e.kind));
    }
}

/// A read of the kept index at a moment UPTO (none: the whole graph), for [`Course`] and the costs: the index caught up,
/// held while the read lasts.
pub(crate) struct KeptView<'g> {
    g: &'g Graph,
    edges: &'g [Edge],
    upto: Option<String>,
    k: RefCell<MutexGuard<'g, Kept>>,
}

impl<'g> KeptView<'g> {
    pub(crate) fn new(g: &'g Graph, upto: Option<&str>) -> KeptView<'g> {
        let p = g.parts();
        let mut k = g.kept.lock();
        k.catch_up(&p);
        // a moment no edge is after is one every edge is by: each read here compares it with edges' times alone (the
        // sessions' lines and the first handings and answers are edges' too), so it reads as none, with no time
        // compared (paper-research N-1446 p1922 3: the canvas's ask, at his window's right edge)
        let upto = upto.filter(|u| k.latest_when.as_deref().is_some_and(|l| l > *u));
        KeptView { g, edges: p.edges, upto: upto.map(str::to_string), k: RefCell::new(k) }
    }

    fn upto(&self) -> Option<&str> {
        self.upto.as_deref()
    }

    /// The edges by then at X of KINDS, in order: one list made, off its kinds' lists where it keeps them (sorted only
    /// when two or more are read), else off all of its edges (paper-research N-1446 p2072 2).
    fn edges_of(&self, x: &str, kinds: &[EdgeKind]) -> Vec<&'g Edge> {
        let k = self.k.borrow();
        let Some(&i) = k.ids.get(x) else { return Vec::new() };
        let (edges, upto) = (self.edges, self.upto());
        let at = |j: &u32| &edges[*j as usize];
        let Some(groups) = k.big.get(&i) else {
            return k.by[i as usize].iter().map(at).filter(|e| kinds.contains(&e.kind) && by_then(&e.when, upto)).collect();
        };
        let mut lists = groups.iter().filter(|(g, _)| kinds.contains(g)).map(|(_, l)| l);
        match (lists.next(), lists.next()) {
            (None, _) => Vec::new(),
            (Some(a), None) => a.iter().map(at).filter(|e| by_then(&e.when, upto)).collect(),
            (Some(a), Some(b)) => {
                // the graph's order is its edges' places in it
                let mut all: Vec<&'g Edge> = a.iter().chain(b).chain(lists.flatten()).map(at).filter(|e| by_then(&e.when, upto)).collect();
                all.sort_unstable_by_key(|e| std::ptr::from_ref::<Edge>(e));
                all
            }
        }
    }

    /// Of ticket T's answers by UPTO by an agent's post, not his, each post's least by (when, writer, post), then from
    /// each number on the least of those ([`Course::first_answered`]): compared where they are kept, each copied once.
    fn answered_least_of(k: &Kept, t: &str, upto: Option<&str>) -> Vec<(u32, (At, Id, Id))> {
        let owner = k.ids.get(OWNER).copied();
        let mut v: Vec<(u32, (&At, Id, Id))> = Vec::new();
        for (m, answers) in k.answered.get(t).into_iter().flatten() {
            let best = answers
                .iter()
                .filter_map(|(f, w)| {
                    let w = w.as_ref().filter(|w| upto.is_none_or(|u| w.as_str() <= u))?;
                    let by = Self::writer_id(k, *f, upto).filter(|b| Some(*b) != owner)?;
                    Some((w, by, *f))
                })
                .min_by(|a, b| least_key(k, a).cmp(&least_key(k, b)));
            if let Some(b) = best {
                v.push((*m, b));
            }
        }
        for i in (0..v.len().saturating_sub(1)).rev() {
            if least_key(k, &v[i + 1].1) < least_key(k, &v[i].1) {
                v[i].1 = v[i + 1].1;
            }
        }
        v.into_iter().map(|(m, (w, by, f))| (m, (w.clone(), by, f))).collect()
    }

    /// The writer of X by then (`wrote`, the last), X a post or a ticket, by its number.
    fn writer_id(k: &Kept, x: Id, upto: Option<&str>) -> Option<Id> {
        if !matches!(k.kind(x), Some(NodeKind::Post | NodeKind::Ticket)) {
            return None;
        }
        k.writers.get(&x)?.iter().rev().find(|(_, w)| by_then(w, upto)).map(|(f, _)| *f)
    }

    /// What his asks cost over the whole graph ([`Graph::costs_at`] with no moment), from what each request's step began
    /// with as kept; read on a view of no moment (a past one's is the whole pass's, `Graph::costs_walked`).
    /// Each ask's rows as kept summed ([`Sum`]), in the order `Graph::sum_costs` gives them.
    pub(crate) fn costs(&self) -> Vec<Cost> {
        debug_assert!(self.upto.is_none(), "the kept costs are the whole graph's");
        let asks = self.his_asks();
        let k = self.k.borrow();
        let spent = |rows: &[Tally]| -> Vec<Spent> {
            let mut v: Vec<Spent> = rows.iter().map(|t| Spent { agent: k.name(t.agent).to_string(), what: t.what.clone(), requests: t.requests, carried: t.carried, tokens: t.tokens }).collect();
            v.sort_by(|a, b| (&a.agent, &a.what).cmp(&(&b.agent, &b.what)));
            v
        };
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for e in asks {
            let post = e.to.as_str();
            if !seen.insert(post) {
                continue;
            }
            let sum = k.ids.get(post).and_then(|x| k.costs.sums.get(x));
            out.push(Cost { post: post.to_string(), handed: sum.map(|s| spent(&s.handed)).unwrap_or_default(), named: sum.map(|s| spent(&s.named)).unwrap_or_default(), shared: sum.map_or(0, |s| s.shared) });
        }
        out
    }

    /// His writings of an ask by then ([`Graph::ask_times_at`]'s), in order.
    pub(crate) fn his_asks(&self) -> Vec<&'g Edge> {
        self.near(OWNER, &[EdgeKind::Wrote]).into_iter().filter(|e| e.from == OWNER && self.asking(&e.to) && self.kind_of(&e.to) == Some(NodeKind::Post)).collect()
    }

    /// For each of ASKS, his posts after it was in his window that answer it, or answer the post that took it or closed
    /// it in the ledger, or name it ([`Graph::his_after`]): from the edges at those posts alone, taken in the graph's
    /// order as the whole pass takes them.
    pub(crate) fn his_after(&self, asks: &[&crate::graph::AskTimes]) -> Vec<Vec<String>> {
        let k = self.k.borrow();
        // each edge into one of the posts, once, with the asks it may be on
        let mut on: BTreeMap<u32, Vec<(usize, &At)>> = BTreeMap::new();
        for (i, a) in asks.iter().enumerate() {
            let Some(landed) = a.in_window.as_ref() else { continue };
            let posts: BTreeSet<&str> = [Some(a.post.as_str()), a.taken.as_ref().and_then(|r| r.via.as_deref()), a.closed.as_ref().and_then(|r| r.via.as_deref())].into_iter().flatten().collect();
            for p in posts {
                let Some(&x) = k.ids.get(p) else { continue };
                for e in k.of_kinds(x, &[EdgeKind::Answered, EdgeKind::AnsweredUpto, EdgeKind::Named], self.edges) {
                    if self.edges[e as usize].to == p {
                        let v = on.entry(e).or_default();
                        if !v.iter().any(|(j, _)| *j == i) {
                            v.push((i, &landed.at));
                        }
                    }
                }
            }
        }
        // his posts: those a `wrote` from him reaches, at any time
        let his = |x: &str| -> bool {
            let Some(&i) = k.ids.get(x) else { return false };
            k.of_kinds(i, &[EdgeKind::Wrote], self.edges).iter().map(|e| &self.edges[*e as usize]).any(|e| e.from == OWNER && e.to == x)
        };
        let mut out: Vec<Vec<(&At, &str)>> = vec![Vec::new(); asks.len()];
        for (e, at) in on {
            let e = &self.edges[e as usize];
            let Some(when) = e.when.as_ref() else { continue };
            if !his(&e.from) || self.g.get(&e.from).is_none_or(|n| n.kind != NodeKind::Post) {
                continue;
            }
            for (i, landed) in at {
                if when > landed && e.from != asks[i].post && !out[i].iter().any(|(_, p)| *p == e.from) {
                    out[i].push((when, e.from.as_str()));
                }
            }
        }
        out.into_iter()
            .map(|mut v| {
                v.sort();
                v.into_iter().map(|(_, p)| p.to_string()).collect()
            })
            .collect()
    }
}

/// A first handing's or answer's order, X = (when, an id, an id): by the time, then the two ids' names, as
/// [`Reached`]'s (at, id, via) orders them.
fn least_key<'a>(k: &'a Kept, x: &(&'a At, Id, Id)) -> (&'a str, &'a str, &'a str) {
    (x.0.as_str(), k.name(x.1), k.name(x.2))
}

impl<'g> Course<'g> for KeptView<'g> {
    fn near(&self, x: &str, kinds: &[EdgeKind]) -> Vec<&'g Edge> {
        self.edges_of(x, kinds)
    }

    fn kind_of(&self, x: &str) -> Option<NodeKind> {
        let k = self.k.borrow();
        k.ids.get(x).and_then(|i| k.kind(*i))
    }

    fn commits_at(&self, x: &str) -> Vec<&'g Edge> {
        let k = self.k.borrow();
        let Some(i) = k.ids.get(x) else { return Vec::new() };
        k.commits.get(i).into_iter().flatten().map(|j| &self.edges[*j as usize]).filter(|e| by_then(&e.when, self.upto())).collect()
    }

    fn touched(&self, x: &str) -> bool {
        let k = self.k.borrow();
        let Some(&i) = k.ids.get(x) else { return false };
        k.by[i as usize].iter().any(|e| by_then(&self.edges[*e as usize].when, self.upto()))
    }

    fn writer(&self, x: &str) -> Option<String> {
        let k = self.k.borrow();
        let i = *k.ids.get(x)?;
        Self::writer_id(&k, i, self.upto()).map(|w| k.name(w).to_string())
    }

    fn asking(&self, x: &str) -> bool {
        if self.upto.is_none() {
            // at no moment, whether an `asks` is from it at all, as the costs' side keeps it
            let k = self.k.borrow();
            return k.ids.get(x).is_some_and(|i| k.costs.asking.contains(i));
        }
        self.near(x, &[EdgeKind::Asks]).iter().any(|e| e.from == x)
    }

    fn first_handed(&self, t: &str, n: u32) -> Option<Reached> {
        let mut guard = self.k.borrow_mut();
        let k: &mut Kept = &mut guard;
        if !k.handed_least.contains_key(t) {
            // each post's least, then from each number on the least of those, by (when, to, from): compared where they
            // are kept, each time copied once, kept until one of the ticket's changes
            let mut v: Vec<(u32, (&At, Id, Id))> = k.handed.get(t).into_iter().flatten().map(|(n, (w, to, from))| (*n, (w, *to, *from))).collect();
            for i in (0..v.len().saturating_sub(1)).rev() {
                if least_key(k, &v[i + 1].1) < least_key(k, &v[i].1) {
                    v[i].1 = v[i + 1].1;
                }
            }
            let least = v.into_iter().map(|(m, (w, to, from))| (m, (w.clone(), to, from))).collect();
            k.handed_least.insert(t.to_string(), least);
        }
        let k: &Kept = k;
        let mins = &k.handed_least[t];
        let i = mins.partition_point(|(m, _)| *m < n);
        let (at, to, from) = &mins.get(i)?.1;
        // the least of all is by then, or none is: each other is at or after it
        self.upto().is_none_or(|u| at.as_str() <= u).then(|| Reached { at: at.clone(), id: k.name(*to).to_string(), via: Some(k.name(*from).to_string()) })
    }

    fn first_answered(&self, t: &str, n: u32) -> Option<Reached> {
        let mut guard = self.k.borrow_mut();
        let k: &mut Kept = &mut guard;
        let upto = self.upto();
        // kept over the whole graph, and at the last moment read: another moment's made again
        if upto.is_some() && k.answered_at.as_deref() != upto {
            k.answered_at = upto.map(str::to_string);
            k.answered_least_at.clear();
        }
        let made = if upto.is_none() { k.answered_least.contains_key(t) } else { k.answered_least_at.contains_key(t) };
        if !made {
            let least = Self::answered_least_of(k, t, upto);
            let kept = if upto.is_none() { &mut k.answered_least } else { &mut k.answered_least_at };
            kept.insert(t.to_string(), least);
        }
        let k: &Kept = k;
        let mins = if upto.is_none() { &k.answered_least[t] } else { &k.answered_least_at[t] };
        let i = mins.partition_point(|(m, _)| *m < n);
        let (at, by, f) = &mins.get(i)?.1;
        Some(Reached { at: at.clone(), id: k.name(*by).to_string(), via: Some(k.name(*f).to_string()) })
    }

    fn has_turns(&self, x: &str) -> bool {
        let k = self.k.borrow();
        let Some(&i) = k.ids.get(x) else { return false };
        k.any_of_kinds(i, &[EdgeKind::TurnOf, EdgeKind::Prompted, EdgeKind::StartOf], self.edges, |e| e.to == x)
    }

    fn session_last(&self, x: &str) -> Option<(At, usize)> {
        let upto = self.upto.clone();
        self.k.borrow_mut().sessions.get_mut(x)?.last(upto.as_deref())
    }

    fn agents(&self) -> Vec<&'g Node> {
        let k = self.k.borrow();
        k.agents.iter().map(|i| &self.g.nodes[*i]).collect()
    }

    fn job_ends(&self) -> Vec<&'g Edge> {
        let k = self.k.borrow();
        // each job's `ended` as kept, its other end's kind read by its number: none of a job's other edges read
        k.jobs
            .iter()
            .flat_map(|j| k.ended.get(j).into_iter().flatten())
            .filter(|(_, to)| k.kind(*to) == Some(NodeKind::Commit))
            .map(|(i, _)| &self.edges[*i as usize])
            .filter(|e| by_then(&e.when, self.upto()))
            .collect()
    }

    fn first_on(&self, c: &str) -> Option<&'g Edge> {
        let k = self.k.borrow();
        let j = *k.first_on.get(k.ids.get(c)?)?;
        // the first at the least time of all, or none by then: each other is at or after it
        Some(&self.edges[j as usize]).filter(|e| by_then(&e.when, self.upto()))
    }
}
