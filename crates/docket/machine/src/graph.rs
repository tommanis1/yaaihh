//! The record as one graph (paper-research N-1446 p1337; the owner, N-1104 p757: "its noded and edges", and p758: what
//! a thing is, apart from how it is shown). Made once, by the replay: [`Builder::after`] is `Machine::replay_with`'s
//! AFTER, and reads only the input just replayed, with its recorded outputs, and the tickets they name. Beside the
//! record, git's commits and the runner's jobs (`graph_git`, p1364, p1372; the owner, p764). So every node and edge
//! says where it comes from ([`Source`]: the record's line, a commit, a job, a release or a session's line) and
//! when, and nothing is kept beside the machine but the graph itself. Every view reads this; nothing here is drawn.
//! An agent's side, from the sessions the record's `given` lines name, is coordd's (`follow::side_into`, p1376 4),
//! with the requests the sessions sent (p1398 5).
//!
//! Ids: a post `N-1104.p745` and a ticket `N-1104` as [`Machine::id`] writes them; a step `N-1104.s3` as the delivery's
//! step ids are; everything else `KIND:NAME` (`agent:paper-research`, `program:question`, `call:route`), a handler
//! `handler:KIND:EVENT` (the kind its `when` was written in and its event, the Map's id), a turn `turn:AGENT@LINE`, at
//! the line that ended it, a commit `commit:HASH8` (8 characters, as a release's name has it), a session `session:ID`,
//! a job `job:FILE` (its job file's name), a build `build:RELEASE`, an agent's start `start:AGENT@LINE`, at roster's
//! line, a branch `branch:REPO/NAME`, a text `text:SHA12` (its sha256's first 12 hex), a request `request:ID` (its
//! `api-request` line's id) and the tools a request carried `tools:SHA12`.
//!
//! [`Graph::ask_times`] gives each of his asks its course, and [`Graph::now`] what is worked on at a moment and what is
//! not: both read the edges and their times alone, so either at a past time gives that time as it was.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::line::{At, Line};
use crate::machine::Machine;

/// What a node is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Post,
    Ticket,
    Agent,
    /// An agent's turn, at the `turn_ended` line that closed it (or the `turn_cut` one, when its process went first).
    Turn,
    Step,
    /// A step's call as it went on the wire.
    Call,
    /// A call as its `.call` file declares it, by name.
    CallFile,
    /// The owner's edit of a call's prompt on a ticket (`Line::Template`).
    Template,
    /// A ticket's program, by its kind.
    Program,
    /// A `when` a ticket ran (`Line::Opened` of its run, `Line::Handling` of its hole), by its kind and event.
    Handler,
    File,
    /// A commit, from git, or named by the record (a read's head, a release's).
    Commit,
    /// An agent's Claude Code session: the one a commit came from (its `Session:` trailer), a start runs, or a file the
    /// record's `given` lines name.
    Session,
    /// An agent's start in a session (roster's `began` line, docket-core N-1446 p1397): start, restart, fork or rename,
    /// its `what` the phase, the model and the effort.
    Start,
    /// A text as it stood, kept once by its hash: a file a session's instructions loaded, its first user message, or a
    /// block of the system prompt its requests carried.
    Text,
    /// A request a session sent, from its `api-request` line: its model and effort, at its time (paper-research N-1446
    /// p1398 5).
    Request,
    /// The tools a session's requests carried, kept once by the hash of their JSON (`tools:SHA12`).
    Tools,
    /// A runner job.
    Job,
    /// A branch of a repository, `branch:REPO/NAME` (git's reflog of it).
    Branch,
    Build,
    Account,
    /// The slice of an agent's session a `given` says it took (`where`): the giving, not its bytes, which coordd reads
    /// asked by the giving, never by a place ([`slice_id`]; paper-research N-1446 p1376, 3, p1380).
    Slice,
    /// A worktree of a repository, by its top as git gives it (`worktree:TOP`, [`crate::graph_git::worktree_id`]):
    /// where a job ran and a commit was made (paper-research N-1446 p1530).
    Worktree,
    /// A kind this build does not know: a newer coordd's, as this build decodes it (`#[serde(other)]`; paper-research
    /// N-1446 p1574), so a slice that holds one still reads, its card "other". Never made here.
    #[serde(other)]
    Other,
}

/// What happened between two nodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// agent → post (a program's post: its ticket → it, p1439); agent → commit (its `Agent:`); agent → a ticket it
    /// opened to hand in for a post (its form's `for`, `Machine::handed_in_for`), at its open
    Wrote,
    /// post → ticket
    PostedIn,
    Answered,
    /// post → the last post of another's it answers as the machine counts it (`Ticket::answers`: up to what its writer
    /// had been handed, less what was kept from it), apart from the reply tool's target (`answered`, drawn under that
    /// post; paper-research N-1446 p1409, 1)
    AnsweredUpto,
    Named,
    Handed,
    Woke,
    /// turn → agent, at the turn's end
    TurnOf,
    /// the post (or step) whose delivery was the prompt of an agent's turn → the agent (`delivered`, `began`): the
    /// turn began then, as its session wrote it
    Prompted,
    /// start → agent (roster's `began`)
    StartOf,
    /// ticket → handler; agent → job, at its start; job → the commit it ran on, at its start
    Ran,
    /// job → the commit it ran on, as it ended (its `ok` says how)
    Ended,
    /// agent → the item it works on (`started`: `coord item start`)
    Started,
    /// item → the agent that ended it (`ended`), or its program, which closed it (`closed`)
    Closed,
    /// his post → each agent the ASKED ledger says it asks to act (`Machine::asked`), at the line it first does
    /// (paper-research N-1446 p1446)
    Asks,
    /// his post → the answer that closed it in the ASKED ledger (a post, or a hand-in's ticket), every agent it asks then
    /// having one (`Machine::asked`), at the line it first does; again after it asks one more
    ClosedBy,
    /// step → ticket
    StepOf,
    Called,
    /// ticket → program; item → its note; step → call file; commit → its session, and its first parent; build → the
    /// build it follows; start → the session it runs (a fork's, whose own is not named: the session it forks)
    CameFrom,
    Edited,
    /// a typed read → its head's commit; session → a file its Read calls read
    Read,
    /// a merge's other parent → the merge
    MergedInto,
    /// commit → build
    BuiltInto,
    /// commit → each branch that was made on it or came to it (git's reflog), the first time
    On,
    Installed,
    /// session → its agent, at the record's first `given` that names the session's file
    SessionOf,
    /// session → a text its instructions loaded, at the start and at each compaction
    Loaded,
    /// session → its first user message
    Briefed,
    /// text → the file it stood as
    VersionOf,
    /// session → a request it sent
    Sent,
    /// request → each block of its system prompt and its tools, from its shape
    Carried,
    /// request → each post handed in its step: the step its request began (`follow::side`'s join, by hash or by order)
    /// and the requests after it up to the next step's
    For,
    /// agent → the slice of its session that took a delivery (`given`)
    Took,
    /// post (or a notice's ticket) → agent, seen at its next turn, which it does not start (`inform`)
    Informed,
    /// post → the turn it was written in: its writer's next turn end or cut by record line, its own when its turn's end
    /// made it (`replied`); his posts and a program's are in none (paper-research N-1446 p1458, point 3)
    WrittenIn,
    /// request → the turn it was sent in: its session's agent's first turn end or cut at or after the request's time,
    /// to the whole second, as the record writes its times (a request in the same second as a turn's end is that
    /// turn's)
    SentIn,
    /// request → the text each file its session's instructions loaded stood as at it: that file's latest load before it
    /// in the session (`loaded`)
    RanWith,
    /// text → the text it became next in a session: a loaded file's next text at its path, a system prompt block's next
    /// text at its index in the session's requests, at the line where the new one first stands; its `what` the words
    /// that changed (coordd's `follow::difference`)
    Became,
    /// job → the worktree it sent (its line's `wt`); commit → the worktree it was made in, the first whose HEAD reflog
    /// has it as a commit (docket-edge8's git round; paper-research N-1446 p1530)
    In,
    /// his post → the run its ticket's `when owner_post` opened for it (a ticket of its own, numbered from HIDDEN), at
    /// the line that opened the run; a post of his that joined the burst a run gathers (`gather`), at its own line
    /// (paper-research N-1446 p1677: his post reaches the run that handled it, and through it the run's steps and calls)
    HandledBy,
    /// him → his post, his sign-off of it, his word of worth in its `what` (`worth_it`, `fine`, `not_worth_it`; `none`
    /// takes it back), at its `signed_off` line (`machine::SIGNED_OFF`; paper-research N-1446 p1786): the last by then
    /// holds ([`AskTimes::signed`])
    SignedOff,
    /// start → the turn it was made in: its starter's next turn end or cut by record line, as `WrittenIn` joins a post
    /// to its writer's turn (roster's `starter`; fix 4's Initial prompt, docket-edge6 N-1446 p2496: where a start was
    /// written); a start of his, or one no agent ran roster for, is in none
    MadeIn,
    /// start → the text it gave Claude Code beside or in place of Claude Code's own system prompt (roster's
    /// `system_sha`: his version N, `$COORD/prompts/NAME.system/N.md`, or the standing text; path-system), one node
    /// with any text of the same words (`text:SHA12`, coordd's `follow::text_id`); its `what` the version, and whether
    /// it replaced Claude Code's own ([`IN_PLACE`]) (accounts-lead N-1446 p2504 2, p2524 2; docket-edge6 p2529)
    StartedWith,
    /// a kind this build does not know: a newer coordd's, as this build decodes it (`#[serde(other)]`; paper-research
    /// N-1446 p1574). Never made here.
    #[serde(other)]
    Other,
}

/// A `started_with` edge's words for a start whose text replaced Claude Code's own system prompt (roster's `replace`):
/// `version 3, in place of Claude Code's own`.
pub const IN_PLACE: &str = "in place of Claude Code's own";

/// Whose message to whom an edge carries, read off the graph (paper-research N-1446 p1376, 3: told apart by kind):
/// his to an agent, an agent's to an agent, an agent's to him, or the engine's own: a call on the wire, or a program's
/// post (p1439). Nothing marks it; [`way`] reads it from the post's writer and where the edge goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Way {
    OwnerToAgent,
    AgentToAgent,
    AgentToOwner,
    Engine,
}

/// Where a node or an edge comes from (paper-research N-1446 p1364: "Each edge says its source: the record's line,
/// the commit hash, or the release name"). As JSON, one field: `"line": 128271`, `"commit": "de447f86"`; read back so
/// too, as the window reads a slice of the graph (`graph_slice`, proto-window N-1446 p1383).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// The record's line, counted from 1 over the replayed lines: log.jsonl's own.
    Line(usize),
    /// A commit, by its first 8 characters: its message, trailers and parents, as git gives them.
    Commit(String),
    /// A runner job, by its job file's name (build-runner's `jobs/PID-SINCE.json`, without `.json`).
    Job(String),
    /// A release, by its name: what it carries, from git and the release it follows.
    Release(String),
    /// A session's line, counted from 1: the session `session:ID`'s file, as the record's `given` lines name it.
    Session(String, usize),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    /// Where it first came from.
    #[serde(flatten)]
    pub source: Source,
    /// When it came to be, as the record writes a time (`2026-11-21T08:19:19Z`): its line's, its commit's, its job's
    /// start. None where its source gives none (a release's name gives its time in no zone it names).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<At>,
    /// What its source says it is, in a line: a commit's subject; a job's verb, packages and totals. Empty for most.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub what: String,
    /// Whether it went right, where its source says so: a job that ended with rc 0, or not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    /// A request's tokens, from its session's response to it (paper-research N-1446 p1429 2): None where the
    /// session holds no response, and for every other kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<Tokens>,
}

/// Token counts of requests. `input` is the prompt not read from or written to the cache, so the prompt is
/// `input + cache_read + cache_write`; `reasoning` is the part of `output` the model spent thinking. A request
/// node's ([`Node::tokens`]), and the usage rows' (coord-types names it too).
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    #[serde(default)]
    pub reasoning: u64,
}

impl Tokens {
    pub fn prompt(&self) -> u64 {
        self.input + self.cache_read + self.cache_write
    }
    pub fn total(&self) -> u64 {
        self.prompt() + self.output
    }
}

impl std::ops::AddAssign for Tokens {
    fn add_assign(&mut self, o: Tokens) {
        self.input += o.input;
        self.output += o.output;
        self.cache_read += o.cache_read;
        self.cache_write += o.cache_write;
        self.reasoning += o.reasoning;
    }
}

impl std::ops::SubAssign for Tokens {
    fn sub_assign(&mut self, o: Tokens) {
        self.input = self.input.saturating_sub(o.input);
        self.output = self.output.saturating_sub(o.output);
        self.cache_read = self.cache_read.saturating_sub(o.cache_read);
        self.cache_write = self.cache_write.saturating_sub(o.cache_write);
        self.reasoning = self.reasoning.saturating_sub(o.reasoning);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
    #[serde(flatten)]
    pub source: Source,
    /// When it happened: its line's time, its commit's, a job's start or end. None where its source gives none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<At>,
    /// A `handed` edge of a giving made anew after the machine made its delivery due again: the record line that made
    /// it so, a turn's end or cut that left his post unanswered (`GivenRec::left`), a line that put it back unread
    /// (`unread`, `due_again`), or an era's (paper-research N-1446 p1449). None for every other edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub again: Option<usize>,
    /// What changed along it, in words: a `became` edge's difference between its two texts, which the window is not
    /// served itself (p1438's guard: no text at a place the record gives; p1458). Empty for every other edge.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub what: String,
}

impl Node {
    /// The record line it first came from, if it came from the record.
    pub fn line(&self) -> Option<usize> {
        if let Source::Line(n) = self.source { Some(n) } else { None }
    }
}

impl Edge {
    /// The record line it comes from, if it comes from the record.
    pub fn line(&self) -> Option<usize> {
        if let Source::Line(n) = self.source { Some(n) } else { None }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(from = "GraphRead")]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    #[serde(skip)]
    at: BTreeMap<String, usize>,
    /// The runner's jobs by id as their lines and files say them, the last line's, an end never undone by its start
    /// (`graph_git::job`): what a decision's facts read of them (`graph_git::facts`; proto-mind N-1446 p1502 (1)).
    #[serde(skip)]
    pub jobs: BTreeMap<String, crate::graph_git::Job>,
    /// The release acts in the record's order (`graph_git::release_line`): which commit the coord's coordd runs.
    #[serde(skip)]
    pub releases: Vec<crate::graph_git::ReleaseAct>,
    /// The kept index of what [`Graph::ask_times_at`], [`Graph::now`], [`Graph::costs_at`] and [`Graph::his_after`]
    /// read (`graph_kept`, paper-research N-1446 p1828): caught up with the edges when next read, never written.
    #[serde(skip)]
    pub(crate) kept: crate::graph_kept::KeptCell,
}

/// A graph as written (its nodes and edges), read back with its index of nodes made again: each node's place by its id,
/// as `node_from` keeps it (a saved replay, I-1509). Its jobs and release acts are not written with it: a saved replay
/// keeps them beside it and puts them back (coordd's replay_state.rs).
#[derive(Deserialize)]
struct GraphRead {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

impl From<GraphRead> for Graph {
    fn from(g: GraphRead) -> Graph {
        let at = g.nodes.iter().enumerate().map(|(i, n)| (n.id.clone(), i)).collect();
        Graph { nodes: g.nodes, edges: g.edges, at, jobs: BTreeMap::new(), releases: Vec::new(), kept: Default::default() }
    }
}

#[cfg(test)]
impl Graph {
    /// The graph of NODES and EDGES, each node's place by its id made as a saved graph's is (a test's: tests/kept.rs
    /// grows a saved graph again as it grew).
    pub(crate) fn of_parts(nodes: Vec<Node>, edges: Vec<Edge>) -> Graph {
        Graph::from(GraphRead { nodes, edges })
    }

    /// NODES and EDGES after the graph's own, each node with its place by its id (a test's, as [`Graph::of_parts`]).
    pub(crate) fn put_parts(&mut self, nodes: impl IntoIterator<Item = Node>, edges: impl IntoIterator<Item = Edge>) {
        for n in nodes {
            self.at.insert(n.id.clone(), self.nodes.len());
            self.nodes.push(n);
        }
        self.edges.extend(edges);
    }
}

impl Graph {
    /// The node ID, of KIND, made at record line LINE unless it is there already; its id.
    pub fn node(&mut self, id: impl Into<String>, kind: NodeKind, line: usize) -> String {
        self.node_from(id, kind, Source::Line(line), None)
    }

    /// The node ID, of KIND, from SOURCE at WHEN unless it is there already; its id.
    pub fn node_from(&mut self, id: impl Into<String>, kind: NodeKind, source: Source, when: Option<At>) -> String {
        let id = id.into();
        if !self.at.contains_key(&id) {
            self.at.insert(id.clone(), self.nodes.len());
            self.nodes.push(Node { id: id.clone(), kind, source, when, what: String::new(), ok: None, tokens: None });
        }
        id
    }

    /// An edge from record line LINE; its time is the line's, given by [`Graph::date`].
    pub fn edge(&mut self, from: &str, to: &str, kind: EdgeKind, line: usize) {
        self.edge_from(from, to, kind, Source::Line(line), None);
    }

    pub fn edge_from(&mut self, from: &str, to: &str, kind: EdgeKind, source: Source, when: Option<At>) {
        self.edges.push(Edge { from: from.to_string(), to: to.to_string(), kind, source, when, again: None, what: String::new() });
    }

    pub fn get(&self, id: &str) -> Option<&Node> {
        self.at.get(id).map(|&i| &self.nodes[i])
    }

    /// Node ID, to change: the kept index told first, while the node still holds what it read (a request's model and
    /// tokens in his asks' costs, `graph_kept`).
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Node> {
        let &i = self.at.get(id)?;
        self.kept.get_mut().node_changing(&self.nodes[i]);
        Some(&mut self.nodes[i])
    }

    /// The graph's parts the kept index reads.
    pub(crate) fn parts(&self) -> crate::graph_kept::Parts<'_> {
        crate::graph_kept::Parts { nodes: &self.nodes, edges: &self.edges }
    }

    /// The kept index caught up with the edges in now (`graph_kept`): what the next read would do first, done where the
    /// graph grows by much at once (coordd's start, its sessions' and git's rounds), so no read waits on it.
    pub fn keep(&self) {
        self.kept.lock().catch_up(&self.parts());
    }

    /// The graph's heap and its kept index's, near enough, in bytes (paper-research N-1446 p1865: the index beside the
    /// graph): each node and edge at its size with its strings' bytes, each node's place at one and a half times its
    /// entry with its id's bytes; the index as `graph_kept` counts it. The jobs and release acts are not counted.
    pub fn heap_bytes(&self) -> (usize, Vec<(&'static str, usize)>) {
        use std::mem::size_of;
        let source = |s: &Source| match s {
            Source::Commit(x) | Source::Job(x) | Source::Release(x) | Source::Session(x, _) => x.capacity(),
            _ => 0,
        };
        let when = |w: &Option<At>| w.as_ref().map_or(0, String::capacity);
        let nodes = self.nodes.capacity() * size_of::<Node>() + self.nodes.iter().map(|n| n.id.capacity() + n.what.capacity() + when(&n.when) + source(&n.source)).sum::<usize>();
        let edges = self.edges.capacity() * size_of::<Edge>() + self.edges.iter().map(|e| e.from.capacity() + e.to.capacity() + e.what.capacity() + when(&e.when) + source(&e.source)).sum::<usize>();
        let at = self.at.len() * (size_of::<String>() + size_of::<usize>()) * 3 / 2 + self.at.keys().map(String::capacity).sum::<usize>();
        (nodes + edges + at, self.kept.lock().bytes())
    }

    /// The edges HOLD holds, the others taken out, in order as `Vec::retain` keeps them, with the kept index moved along:
    /// what takes edges out of the graph goes through here (the sessions' side, making a request's `for` again).
    pub fn retain_edges(&mut self, mut hold: impl FnMut(&Edge) -> bool) {
        let removed: Vec<usize> = self.edges.iter().enumerate().filter(|(_, e)| !hold(*e)).map(|(i, _)| i).collect();
        if removed.is_empty() {
            return;
        }
        let Graph { nodes, edges, kept, .. } = self;
        let kept = kept.get_mut();
        let touched = kept.before_removing(&crate::graph_kept::Parts { nodes: nodes.as_slice(), edges: edges.as_slice() }, &removed);
        let mut i = 0;
        edges.retain(|_| {
            i += 1;
            removed.binary_search(&(i - 1)).is_err()
        });
        kept.after_removing(&crate::graph_kept::Parts { nodes: nodes.as_slice(), edges: edges.as_slice() }, &removed, touched);
    }

    /// Each node's and edge's time from its record line, where it has none: the line's own `at`. What is added from
    /// the record after [`of`] (coordd's `named`) is dated by calling it again; a time already there stays.
    pub fn date(&mut self, lines: &[Line]) {
        self.date_from(0, 0, 1, lines);
    }

    /// [`Graph::date`] for the nodes from index NODES on and the edges from EDGES on, LINES being the record's from line
    /// FIRST: what [`Builder::after`] has just made of a group, dated as it is made, so coordd's graph, kept along each
    /// input recorded, has its times without the record read again (proto-window N-1446 p1383).
    pub fn date_from(&mut self, nodes: usize, edges: usize, first: usize, lines: &[Line]) {
        // a time given to an edge the kept index holds already: it is made again
        self.kept.get_mut().dated_from(edges);
        let mut at: Vec<Option<At>> = vec![None; lines.len()];
        let mut time = |n: usize| -> Option<At> {
            let k = n.checked_sub(first)?;
            let l = lines.get(k)?;
            Some(at[k].get_or_insert_with(|| crate::machine::line_at(l)).clone()).filter(|a| !a.is_empty())
        };
        for x in self.nodes.iter_mut().skip(nodes) {
            if let (None, Source::Line(n)) = (&x.when, &x.source) {
                x.when = time(*n);
            }
        }
        for e in self.edges.iter_mut().skip(edges) {
            if let (None, Source::Line(n)) = (&e.when, &e.source) {
                e.when = time(*n);
            }
        }
    }

    /// How many nodes of each kind, and edges of each kind.
    pub fn counts(&self) -> (BTreeMap<NodeKind, usize>, BTreeMap<EdgeKind, usize>) {
        let mut n = BTreeMap::new();
        for x in &self.nodes {
            *n.entry(x.kind).or_insert(0) += 1;
        }
        let mut e = BTreeMap::new();
        for x in &self.edges {
            *e.entry(x.kind).or_insert(0) += 1;
        }
        (n, e)
    }

    /// The edges within HOPS of FROM, either way along each, each once: with the hop it was reached at (1 for an edge
    /// at FROM), in hop order, then in source order (the record's lines first, in their order).
    pub fn walk(&self, from: &str, hops: usize) -> Vec<(usize, &Edge)> {
        self.walk_by(from, hops, &|_| true, &|_, _| false).into_iter().map(|(h, i, _)| (h, &self.edges[i])).collect()
    }

    /// [`Graph::walk`] along the edges TAKE keeps (by their index), and on from no node but FROM that STOP holds of,
    /// with the edge the walk first reached it by: an edge at such a node is reached, what lies past it is not
    /// (paper-research N-1446 p1366: past an agent or a ticket a walk counts what is there but does not follow it;
    /// p1677: a run reached from the post it handled it follows). Each edge by its index, with its hop and its branch:
    /// the edge at FROM it was walked from (itself at hop 1), so what one neighbour brings is told from another's
    /// (`graph_slice`'s limit, paper-research N-1446 p1753). The window's slice around a node walks so, along the layers
    /// on (`graph_slice`, proto-window p1383).
    pub fn walk_by(&self, from: &str, hops: usize, take: &dyn Fn(usize) -> bool, stop: &dyn Fn(&Node, &Edge) -> bool) -> Vec<(usize, usize, usize)> {
        let mut by: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (i, e) in self.edges.iter().enumerate().filter(|(i, _)| take(*i)) {
            by.entry(e.from.as_str()).or_default().push(i);
            by.entry(e.to.as_str()).or_default().push(i);
        }
        let mut seen_nodes = BTreeSet::from([from]);
        let mut seen_edges = BTreeSet::new();
        let mut out = Vec::new();
        // each node with its hop, the edge it was reached by and that edge's branch (none for FROM)
        let mut front = VecDeque::from([(from, 0usize, None, 0usize)]);
        while let Some((at, hop, came, branch)) = front.pop_front() {
            if hop == hops || came.is_some_and(|i: usize| self.get(at).is_some_and(|n| stop(n, &self.edges[i]))) {
                continue;
            }
            let mut here: Vec<usize> = by.get(at).into_iter().flatten().copied().filter(|i| !seen_edges.contains(i)).collect();
            here.sort_by(|a, b| self.edges[*a].source.cmp(&self.edges[*b].source));
            for i in here {
                seen_edges.insert(i);
                let e = &self.edges[i];
                let branch = if came.is_none() { i } else { branch };
                out.push((hop + 1, i, branch));
                let other = if e.from == at { e.to.as_str() } else { e.from.as_str() };
                if seen_nodes.insert(other) {
                    front.push_back((other, hop + 1, Some(i), branch));
                }
            }
        }
        out.sort_by(|(h, a, _), (k, b, _)| (h, &self.edges[*a].source).cmp(&(k, &self.edges[*b].source)));
        out
    }

    /// The course of each of his asks (a post of his the ASKED ledger says asks an agent, p1561), in the order they came
    /// (paper-research N-1446 p1376, 1; the owner, N-1104 p767: "competion time what is worked or not worked on now"):
    /// [`AskTimes`], derived from the edges alone.
    pub fn ask_times(&self) -> Vec<AskTimes> {
        self.ask_times_at(None)
    }

    /// [`Graph::ask_times`] as it was at UPTO: from the edges at or before it (and those with no time, as what a
    /// release carries), each point one reached by then. Read off the kept index (`graph_kept`, paper-research N-1446
    /// p1828), each ask from the edges at its own nodes, so no read walks the whole graph; [`Graph::ask_times_walked`]
    /// gives the same from one pass over every edge.
    pub fn ask_times_at(&self, upto: Option<&str>) -> Vec<AskTimes> {
        self.ask_times_with(upto, &crate::graph_kept::KeptView::new(self, upto))
    }

    /// [`Graph::ask_times_at`] from one pass over every edge by UPTO, as each read once worked it out: the kept read's
    /// reference, which it equals row for row (tests/kept.rs; coordd's copy of a record).
    pub fn ask_times_walked(&self, upto: Option<&str>) -> Vec<AskTimes> {
        self.ask_times_with(upto, &Walked::new(self, upto))
    }

    /// [`Graph::ask_times_at`] from V: the edges by UPTO as the kept index or the one pass gives them ([`Course`]).
    pub(crate) fn ask_times_with<'g>(&'g self, upto: Option<&str>, v: &dyn Course<'g>) -> Vec<AskTimes> {
        let owner = agent_id("owner");
        let point = |at: &Option<At>, id: &str, via: Option<&str>| at.clone().filter(|a| upto.is_none_or(|u| a.as_str() <= u)).map(|at| Reached { at, id: id.to_string(), via: via.map(str::to_string) });
        let first = |xs: &mut dyn Iterator<Item = Reached>| xs.min_by(|a, b| (&a.at, &a.id, &a.via).cmp(&(&b.at, &b.id, &b.via)));
        let kind = |id: &str| v.kind_of(id);
        let near = |x: &str, k: EdgeKind| -> Vec<&'g Edge> { v.near(x, &[k]) };
        let mut out = Vec::new();
        // his writings of an ask ([`Graph::his_ask`]): `wrote` from him to a post that asks an agent by then
        for w in near(&owner, EdgeKind::Wrote).into_iter().filter(|e| e.from == owner && v.asking(&e.to) && kind(&e.to) == Some(NodeKind::Post)) {
            let post = w.to.as_str();
            let on = crate::post_or_ticket(post).and_then(|(t, n)| Some((t, n?)));
            // the first post of its ticket from it on that a session took, to an agent not his
            let reached = on.and_then(|(t, n)| v.first_handed(t, n));
            // the items whose note it is (`came_from`), each started by an agent (`started`)
            let items: BTreeSet<&str> = near(post, EdgeKind::CameFrom).into_iter().filter(|e| e.to == post && kind(&e.from) == Some(NodeKind::Ticket)).map(|e| e.from.as_str()).collect();
            // the first answer by an agent's post (his own left out) to one of its ticket from it on, or the first start
            // of an item whose note it is
            let taken = first(
                &mut on
                    .and_then(|(t, n)| v.first_answered(t, n))
                    .into_iter()
                    .chain(items.iter().flat_map(|i| near(i, EdgeKind::Started)).filter(|e| items.contains(e.to.as_str()) && e.from != owner).filter_map(|e| point(&e.when, &e.from, Some(e.to.as_str())))),
            );
            // the commits that name it, by any edge between the two (its subject, its `Item:`)
            let commits: BTreeSet<&str> = v.commits_at(post).into_iter().map(|e| if e.from == post { e.to.as_str() } else { e.from.as_str() }).collect();
            let when_of = |x: &str| self.get(x).and_then(|n| point(&n.when, x, None));
            let first_commit = first(&mut commits.iter().filter_map(|c| when_of(c)));
            let merges: BTreeSet<&str> = commits.iter().flat_map(|c| near(c, EdgeKind::MergedInto)).filter(|e| commits.contains(e.from.as_str())).map(|e| e.to.as_str()).collect();
            let merged = first(&mut merges.iter().filter_map(|c| when_of(c)));
            // the jobs that were for it: those that name it, or an item whose note it is (`named`, the runner's `items`;
            // paper-research N-1446 p1422, 4)
            let for_it: BTreeSet<&str> = std::iter::once(post)
                .chain(items.iter().copied())
                .flat_map(|x| near(x, EdgeKind::Named))
                .filter(|e| (e.to == post || items.contains(e.to.as_str())) && kind(&e.from) == Some(NodeKind::Job))
                .map(|e| e.from.as_str())
                .collect();
            let first_green = first(
                &mut commits
                    .iter()
                    .flat_map(|c| near(c, EdgeKind::Ended))
                    .filter(|e| commits.contains(e.to.as_str()))
                    .chain(for_it.iter().flat_map(|j| near(j, EdgeKind::Ended)).filter(|e| for_it.contains(e.from.as_str())))
                    .filter(|e| self.get(&e.from).is_some_and(|j| j.kind == NodeKind::Job && j.ok == Some(true)))
                    .filter_map(|e| point(&e.when, &e.from, None)),
            );
            let builds: BTreeSet<&str> = commits.iter().chain(&merges).flat_map(|c| near(c, EdgeKind::BuiltInto)).filter(|e| commits.contains(e.from.as_str()) || merges.contains(e.from.as_str())).map(|e| e.to.as_str()).collect();
            let in_window = first(&mut builds.iter().flat_map(|b| near(b, EdgeKind::Installed)).filter(|e| builds.contains(e.from.as_str()) && e.to == owner).filter_map(|e| point(&e.when, &e.from, None)));
            // the ledger's: whom it asks, and its close after the last of those
            let asks_e: Vec<&Edge> = near(post, EdgeKind::Asks).into_iter().filter(|e| e.from == post).collect();
            let asks: Vec<String> = asks_e.iter().map(|e| e.to.clone()).collect::<BTreeSet<_>>().into_iter().collect();
            let last_asked = asks_e.iter().filter_map(|e| e.when.as_ref()).max();
            let closed = latest(near(post, EdgeKind::ClosedBy).into_iter().filter(|e| e.from == post).collect())
                .filter(|e| last_asked.is_none_or(|a| e.when.as_ref().is_some_and(|w| w >= a)))
                .and_then(|e| point(&e.when, &v.writer(&e.to).unwrap_or_else(|| e.to.clone()), Some(e.to.as_str())));
            // his sign-off: the last by then, its word; none when that one took it back
            let signed = latest(near(post, EdgeKind::SignedOff).into_iter().filter(|e| e.to == post && e.from == owner).collect())
                .filter(|e| e.what != "none")
                .and_then(|e| Some(Signed { at: e.when.clone().filter(|a| upto.is_none_or(|u| a.as_str() <= u))?, worth: e.what.clone() }));
            out.push(AskTimes { post: post.to_string(), asked: w.when.clone(), reached, taken, first_commit, merged, first_green, in_window, asks, closed, signed });
        }
        out
    }

    /// Whether E is his writing of an ask: `wrote` from him to a post of ASKING, those the ASKED ledger says ask an
    /// agent by then (an `asks` edge from it; paper-research N-1446 p1561). So his answer to a step, a question's
    /// (Q-22.p1) or a request's (R-413.p1), which closes what it answers, is none, nor a post of his that wakes no one,
    /// nor one on a handler's run (a ticket from HIDDEN on, which the ledger never reads: `owner_post`'s gathers his
    /// posts, each an ask on its own ticket, as its p1 by him).
    fn his_ask(&self, e: &Edge, asking: &BTreeSet<&str>) -> bool {
        e.kind == EdgeKind::Wrote && e.from == agent_id("owner") && asking.contains(e.to.as_str()) && self.get(&e.to).map(|n| n.kind) == Some(NodeKind::Post)
    }

    /// What his ask ASK cost ([`Graph::costs_at`] over the whole graph): nothing counted where no request reaches it.
    pub fn cost(&self, ask: &str) -> Cost {
        self.costs_at(None).into_iter().find(|c| c.post == ask).unwrap_or_else(|| Cost { post: ask.to_string(), ..Default::default() })
    }

    /// What each of his asks cost, in the order he asked them, from the edges at or before UPTO (paper-research N-1446
    /// p1429 2; docket-edge6 p1457): each request a session sent (`sent`, its agent the session's, `session_of`) counts
    /// for each ask its step handed, and apart for each ask a post its step handed names (`named`, one hop: a relay of
    /// the ask, a post on its work), by its agent and its model and effort (the request's `what`), with its tokens where
    /// the session holds its response ([`Cost`]). A step's `for` names the last post of each giving it began with
    /// (`follow::side`'s join, a given's `upto`); what that giving brought is the `handed` edges to the same agent at
    /// that post's giving's line, its last at or before the request (to the second). Of those the step is for its own
    /// post and each that woke its agent or asks it (`woke`, `asks`), not one that only came along: a new giving brings
    /// a thread's whole past, and a step that one post of it woke is no step for each ask before it.
    ///
    /// For the whole graph it is read off the kept index: what each request's step began with, kept as its inputs come
    /// and made again only when one of them changes (`graph_kept`, paper-research N-1446 p1828). At a past moment UPTO it
    /// is one pass over every edge by then ([`Graph::costs_walked`]); nothing in coordd reads that under its lock.
    pub fn costs_at(&self, upto: Option<&str>) -> Vec<Cost> {
        match upto {
            None => crate::graph_kept::KeptView::new(self, None).costs(),
            Some(_) => self.costs_walked(upto),
        }
    }

    /// [`Graph::costs_at`] from one pass over every edge by UPTO: the kept read's reference, which it equals row for row.
    pub fn costs_walked(&self, upto: Option<&str>) -> Vec<Cost> {
        let edges: Vec<&Edge> = self.edges.iter().filter(|e| by_then(&e.when, upto)).collect();
        let asking: BTreeSet<&str> = edges.iter().filter(|e| e.kind == EdgeKind::Asks).map(|e| e.from.as_str()).collect();
        let asks: Vec<&str> = edges.iter().filter(|e| self.his_ask(e, &asking)).map(|e| e.to.as_str()).collect();
        let is_ask: BTreeSet<&str> = asks.iter().copied().collect();
        let agent_of: BTreeMap<&str, &str> = edges.iter().filter(|e| e.kind == EdgeKind::SessionOf).map(|e| (e.from.as_str(), e.to.as_str())).collect();
        let sender: BTreeMap<&str, &str> = edges.iter().filter(|e| e.kind == EdgeKind::Sent).filter_map(|e| Some((e.to.as_str(), *agent_of.get(e.from.as_str())?))).collect();
        // what each giving's line handed each agent; the lines each post was handed to each agent at, with their times
        let mut brought: BTreeMap<(&str, usize), Vec<&str>> = BTreeMap::new();
        let mut givings: BTreeMap<(&str, &str), Vec<(usize, Option<&str>)>> = BTreeMap::new();
        for e in edges.iter().filter(|e| e.kind == EdgeKind::Handed) {
            let Some(line) = e.line() else { continue };
            brought.entry((e.to.as_str(), line)).or_default().push(e.from.as_str());
            givings.entry((e.from.as_str(), e.to.as_str())).or_default().push((line, e.when.as_deref()));
        }
        // each post → each agent it woke or asks
        let woke: BTreeSet<(&str, &str)> = edges.iter().filter(|e| matches!(e.kind, EdgeKind::Woke | EdgeKind::Asks)).map(|e| (e.from.as_str(), e.to.as_str())).collect();
        // the asks each post names
        let mut names: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for e in edges.iter().filter(|e| e.kind == EdgeKind::Named && is_ask.contains(e.to.as_str()) && self.get(&e.from).is_some_and(|n| n.kind == NodeKind::Post)) {
            names.entry(e.from.as_str()).or_default().insert(e.to.as_str());
        }
        // each request: the asks its step handed, and those a post its step handed names
        let second = |t: Option<&str>| t.map(|t| t.get(..19).unwrap_or(t).to_string());
        let mut of: BTreeMap<&str, (BTreeSet<&str>, BTreeSet<&str>)> = BTreeMap::new();
        for e in edges.iter().filter(|e| e.kind == EdgeKind::For) {
            let Some(agent) = sender.get(e.from.as_str()).copied() else { continue };
            let sent = second(e.when.as_deref());
            let line = givings.get(&(e.to.as_str(), agent)).and_then(|ls| ls.iter().filter(|(_, w)| sent.is_none() || w.is_none() || second(*w) <= sent).map(|(l, _)| *l).max());
            let x = of.entry(e.from.as_str()).or_default();
            let own = e.to.as_str();
            for post in line.and_then(|l| brought.get(&(agent, l))).into_iter().flatten().copied().filter(|p| woke.contains(&(*p, agent))).chain([own]) {
                if is_ask.contains(post) {
                    x.0.insert(post);
                }
                x.1.extend(names.get(post).into_iter().flatten().copied());
            }
        }
        let rows = of.iter().filter_map(|(r, (handed, named))| Some((self.get(r)?, sender[r], handed.iter().copied().collect::<Vec<_>>(), named.iter().copied().collect::<Vec<_>>())));
        Graph::sum_costs(&asks, rows)
    }

    /// What ASKS (his, in the order he asked them) cost, from ROWS: each request's node, its agent, the asks its step
    /// handed and those its posts name ([`Graph::costs_at`]). A request counts for each ask it handed, shared when it
    /// handed more than one, and apart for each it only names (`named` never adds into a cost, p1535); by its agent and
    /// its model and effort, with its tokens where the session holds its response.
    pub(crate) fn sum_costs<'a>(asks: &[&'a str], rows: impl Iterator<Item = (&'a Node, &'a str, Vec<&'a str>, Vec<&'a str>)>) -> Vec<Cost> {
        let mut out: BTreeMap<&str, Cost> = asks.iter().map(|a| (*a, Cost { post: a.to_string(), ..Default::default() })).collect();
        for (node, agent, handed, named) in rows {
            let handed: BTreeSet<&str> = handed.into_iter().collect();
            let named: BTreeSet<&str> = named.into_iter().filter(|a| !handed.contains(a)).collect();
            // shared: handed for another of his asks too
            let shared = handed.len() > 1;
            for (ask, apart) in handed.iter().map(|a| (*a, false)).chain(named.iter().map(|a| (*a, true))) {
                let Some(c) = out.get_mut(ask) else { continue };
                c.shared += usize::from(shared && !apart);
                let rows = if apart { &mut c.named } else { &mut c.handed };
                let i = rows.iter().position(|x| x.agent == agent && x.what == node.what).unwrap_or_else(|| {
                    rows.push(Spent { agent: agent.to_string(), what: node.what.clone(), ..Default::default() });
                    rows.len() - 1
                });
                rows[i].requests += 1;
                if let Some(t) = node.tokens {
                    rows[i].carried += 1;
                    rows[i].tokens += t;
                }
            }
        }
        asks.iter()
            .filter_map(|a| out.remove(a))
            .map(|mut c| {
                c.handed.sort_by(|a, b| (&a.agent, &a.what).cmp(&(&b.agent, &b.what)));
                c.named.sort_by(|a, b| (&a.agent, &a.what).cmp(&(&b.agent, &b.what)));
                c
            })
            .collect()
    }

    /// Each of his asks at UPTO (none: the whole graph), in the order he asked them, at the last point of its course
    /// it has reached, and who that was ([`AskNow`]): what [`Graph::now`] reads for those not in his window yet, and
    /// Open → done's asks for all of them (paper-research N-1446 p1786).
    /// Read off the kept index, as [`Graph::ask_times_at`] is.
    pub fn asks_now(&self, upto: Option<&str>) -> Vec<AskNow> {
        self.asks_by(upto, &crate::graph_kept::KeptView::new(self, upto))
    }

    /// [`Graph::asks_now`] from one pass over every edge by UPTO: the kept read's reference.
    pub fn asks_now_walked(&self, upto: Option<&str>) -> Vec<AskNow> {
        self.asks_by(upto, &Walked::new(self, upto))
    }

    /// [`Graph::asks_now`] from V, the edges by UPTO ([`Course`]): the last point reached, and who was behind it, the
    /// agent, or the one behind the commit (its `wrote`) or the job (its `ran`); his own at `asked`.
    pub(crate) fn asks_by<'g>(&'g self, upto: Option<&str>, v: &dyn Course<'g>) -> Vec<AskNow> {
        let owner = agent_id("owner");
        let kind = |id: &str| v.kind_of(id);
        let behind = |x: &str| -> Option<String> {
            if x.starts_with("agent:") {
                return Some(x.to_string());
            }
            v.near(x, &[EdgeKind::Wrote, EdgeKind::Ran]).into_iter().find(|e| e.to == x && kind(&e.from) == Some(NodeKind::Agent)).map(|e| e.from.clone())
        };
        let mut asks = Vec::new();
        for t in self.ask_times_with(upto, v) {
            let (point, last) = t.points().into_iter().rev().find_map(|(p, _, r)| Some((p, r?.clone()))).map_or((Point::Asked, None), |(p, r)| (p, Some(r)));
            let who = last.as_ref().map_or(Some(owner.clone()), |r| behind(&r.id));
            asks.push(AskNow { point, last, who, times: t });
        }
        asks
    }

    /// For each of ASKS, his posts after it was in his window that answer it, or answer the post that took it or closed
    /// it in the ledger, or name it (`answered`, `answered_upto`, `named`), each once, in the order written: what he said
    /// of it once it had landed, his corrections among them (consult-2's design, N-1002 p6, B; paper-research N-1446
    /// p1786). None for one not in his window. The edges are read once for all of them.
    /// Read off the kept index: the edges at those posts alone, in the graph's order.
    pub fn his_after(&self, asks: &[&AskTimes]) -> Vec<Vec<String>> {
        crate::graph_kept::KeptView::new(self, None).his_after(asks)
    }

    /// [`Graph::his_after`] from one pass over every edge: the kept read's reference.
    pub fn his_after_walked(&self, asks: &[&AskTimes]) -> Vec<Vec<String>> {
        let owner = agent_id("owner");
        // each post an edge may reach → the asks it is on, with when each was in his window
        let mut on: BTreeMap<&str, Vec<(usize, &At)>> = BTreeMap::new();
        for (i, a) in asks.iter().enumerate() {
            let Some(landed) = a.in_window.as_ref() else { continue };
            let posts: BTreeSet<&str> = [Some(a.post.as_str()), a.taken.as_ref().and_then(|r| r.via.as_deref()), a.closed.as_ref().and_then(|r| r.via.as_deref())].into_iter().flatten().collect();
            for p in posts {
                on.entry(p).or_default().push((i, &landed.at));
            }
        }
        let mut out: Vec<Vec<(&At, &str)>> = vec![Vec::new(); asks.len()];
        if on.is_empty() {
            return vec![Vec::new(); asks.len()];
        }
        let his: BTreeSet<&str> = self.edges.iter().filter(|e| e.kind == EdgeKind::Wrote && e.from == owner).map(|e| e.to.as_str()).collect();
        for e in self.edges.iter().filter(|e| matches!(e.kind, EdgeKind::Answered | EdgeKind::AnsweredUpto | EdgeKind::Named)) {
            let (Some(at), Some(when)) = (on.get(e.to.as_str()), e.when.as_ref()) else { continue };
            if !his.contains(e.from.as_str()) || self.get(&e.from).is_none_or(|n| n.kind != NodeKind::Post) {
                continue;
            }
            for (i, landed) in at {
                if when > *landed && e.from != asks[*i].post && !out[*i].iter().any(|(_, p)| *p == e.from) {
                    out[*i].push((when, e.from.as_str()));
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

    /// What is worked on at AT, and what is not (paper-research N-1446 p1399; the owner, N-1104 p767: "what is worked
    /// or not worked on now"): each agent's state, and his asks not yet in his window, from the edges at or before AT
    /// alone ([`Now`]). Nothing is read from a live coordd, so `now` of a past time is that time as it was; the
    /// canvas's right edge is `now` of the latest.
    /// Read off the kept index, each agent from the edges at its own nodes; [`Graph::now_walked`] gives the same from one
    /// pass over every edge.
    pub fn now(&self, at: &str) -> Now {
        self.now_with(at, &crate::graph_kept::KeptView::new(self, Some(at)))
    }

    /// [`Graph::now`] from one pass over every edge by AT: the kept read's reference.
    pub fn now_walked(&self, at: &str) -> Now {
        self.now_with(at, &Walked::new(self, Some(at)))
    }

    /// [`Graph::now`] from V, the edges by AT ([`Course`]).
    pub(crate) fn now_with<'g>(&'g self, at: &str, v: &dyn Course<'g>) -> Now {
        let owner = agent_id("owner");
        let upto = Some(at);
        let kind = |id: &str| v.kind_of(id);
        let near = |x: &str, k: EdgeKind| -> Vec<&'g Edge> { v.near(x, &[k]) };
        // a job by then: its start (the agent's `ran`), its commit, its end and whether it went right
        let job_now = |j: &str| -> JobNow {
            let on = |k: EdgeKind| near(j, k).into_iter().find(|x| x.from == j && kind(&x.to) == Some(NodeKind::Commit));
            let end = on(EdgeKind::Ended);
            let start = near(j, EdgeKind::Ran).into_iter().find(|x| x.to == j && kind(&x.from) == Some(NodeKind::Agent));
            JobNow {
                at: start.and_then(|x| x.when.clone()).unwrap_or_default(),
                id: j.to_string(),
                what: self.get(j).map(|n| n.what.clone()).unwrap_or_default(),
                commit: on(EdgeKind::Ran).or(end).map(|x| x.to.clone()),
                ended: end.and_then(|x| x.when.clone()),
                ok: end.and_then(|_| self.get(j)?.ok),
            }
        };
        // the agents `now` lists: this coord's own that the graph ever shows a turn or a start of, a turn's end, a
        // delivery that began one, a start of it; the whole graph's, so a first turn that ends later is one already.
        // The engine's own writers (coord, coordd, box) have none; another project's agents (PROJECT/NAME) are not
        // this coord's, though research's record cut two turns of tovrin-dev's on 13 Nov: no row (N-1446 p1481, p1510)
        let own = |id: &str| !id.trim_start_matches("agent:").contains('/');
        let mut agents = Vec::new();
        for a in v.agents().into_iter().filter(|n| n.id != owner && v.touched(&n.id) && v.has_turns(&n.id) && own(&n.id)) {
            let id = a.id.as_str();
            // its edges of each kind from it, each kind read once
            let from_it = |k: EdgeKind| -> Vec<&'g Edge> { near(id, k).into_iter().filter(|e| e.from == id).collect() };
            let (wrote, ran, started) = (from_it(EdgeKind::Wrote), from_it(EdgeKind::Ran), from_it(EdgeKind::Started));
            let start_of: Vec<&'g Edge> = near(id, EdgeKind::StartOf).into_iter().filter(|e| e.to == id).collect();
            let ended = latest(near(id, EdgeKind::TurnOf).into_iter().filter(|e| e.to == id).collect()).and_then(|e| e.when.clone());
            let after = |e: &&Edge| e.when.is_some() && ended.as_ref().is_none_or(|t| e.when.as_ref() > Some(t));
            // in a turn: since the delivery that began it, or else since its first work after its last turn's end
            let prompted = latest(near(id, EdgeKind::Prompted).into_iter().filter(|e| e.to == id).filter(after).collect());
            let work = || {
                let edited = from_it(EdgeKind::Edited);
                wrote.iter().chain(&edited).chain(&ran).chain(&started).chain(&start_of).copied().filter(after).min_by(|a, b| a.when.cmp(&b.when))
            };
            let turn = prompted.or_else(work).map(|e| TurnNow { since: e.when.clone().unwrap_or_default(), by: if e.from == id { e.to.clone() } else { e.from.clone() }, kind: e.kind });
            let idle_since = if turn.is_none() { ended.clone() } else { None };
            // its edges whose other end is a commit (`commits_at`), of no kind read: no post among them
            let commits = v.commits_at(id);
            let last_post = latest_to(&wrote, NodeKind::Post, &commits, v).map(|e| Reached { at: e.when.clone().unwrap_or_default(), id: e.to.clone(), via: None });
            let mut items: Vec<Reached> = Vec::new();
            for e in started.iter().copied().filter(|e| !v.near(&e.to, &[EdgeKind::Closed]).iter().any(|c| c.from == e.to)) {
                if !items.iter().any(|i| i.id == e.to) {
                    items.push(Reached { at: e.when.clone().unwrap_or_default(), id: e.to.clone(), via: None });
                }
            }
            // its last commit, off its edges whose other end is a commit (`commits_at`): no kind read
            let last_commit = latest(commits.iter().copied().filter(|e| e.kind == EdgeKind::Wrote && e.from == id).collect()).map(|e| {
                let on = v.first_on(&e.to);
                Work { at: e.when.clone().unwrap_or_default(), id: e.to.clone(), what: self.get(&e.to).map(|n| n.what.clone()).unwrap_or_default(), on: on.map(|o| o.to.trim_start_matches("branch:").to_string()) }
            });
            let last_job = latest_to(&ran, NodeKind::Job, &[], v).map(|e| job_now(&e.to));
            let began = latest(start_of).map(|e| {
                let session = near(&e.from, EdgeKind::CameFrom).into_iter().find(|s| s.from == e.from).map(|s| s.to.clone());
                Work { at: e.when.clone().unwrap_or_default(), id: e.from.clone(), what: self.get(&e.from).map(|n| n.what.clone()).unwrap_or_default(), on: session }
            });
            // its sessions' (`session_of`) last line by then, as the graph holds their lines (paper-research N-1446
            // p1429 4)
            let session = near(id, EdgeKind::SessionOf)
                .into_iter()
                .filter(|e| e.to == id)
                .filter_map(|e| Some((e.from.as_str(), v.session_last(e.from.trim_start_matches("session:"))?)))
                .max_by(|a, b| a.1.cmp(&b.1))
                .map(|(x, (at, line))| SessionNow { session: x.to_string(), line, at });
            agents.push(AgentNow { agent: id.trim_start_matches("agent:").to_string(), turn, idle_since, last_post, items, last_commit, last_job, began, session });
        }
        // the agents in a turn whose session has a line by then, the one written nothing in for longest first
        let mut quiet: Vec<(&At, &str)> = agents.iter().filter(|a| a.turn.is_some()).filter_map(|a| Some((&a.session.as_ref()?.at, a.agent.as_str()))).collect();
        quiet.sort();
        let quiet = quiet.into_iter().map(|(_, a)| a.to_string()).collect();
        // each branch an agent works on by then, its last commit's or its last job's, whose last job to end by then
        // ended red: that job, and the first red since its last green; a job's branch is the one its commit was made
        // on (its first `on`); a red branch no agent works on any more is counted, not listed: old work, not stuck
        let made_on = |c: &str| v.first_on(c).map(|o| o.to.as_str());
        let working: BTreeSet<String> = agents
            .iter()
            .flat_map(|a| [a.last_commit.as_ref().and_then(|c| c.on.clone()), a.last_job.as_ref().and_then(|j| j.commit.as_deref()).and_then(made_on).map(|b| b.trim_start_matches("branch:").to_string())])
            .flatten()
            .collect();
        let mut old_reds = 0;
        let mut ends: BTreeMap<&str, Vec<(&At, &str, bool)>> = BTreeMap::new();
        for e in v.job_ends() {
            let (Some(when), Some(ok), Some(branch)) = (e.when.as_ref(), self.get(&e.from).and_then(|j| j.ok), made_on(&e.to)) else { continue };
            ends.entry(branch).or_default().push((when, e.from.as_str(), ok));
        }
        let mut reds = Vec::new();
        for (branch, mut xs) in ends {
            xs.sort();
            let Some(&(_, last, false)) = xs.last() else { continue };
            let since = xs.iter().rev().take_while(|x| !x.2).last().map(|x| x.0.clone()).unwrap_or_default();
            let branch = branch.trim_start_matches("branch:").to_string();
            if !working.contains(&branch) {
                old_reds += 1;
                continue;
            }
            reds.push(Red { branch, job: job_now(last), since });
        }
        reds.sort_by(|a, b| (&a.since, &a.branch).cmp(&(&b.since, &b.branch)));
        // his asks not yet in his window, each at its last point, and who that was
        let asks: Vec<AskNow> = self.asks_by(upto, v).into_iter().filter(|a| a.times.in_window.is_none()).collect();
        let not_taken = asks.iter().filter(|a| a.times.reached.is_some() && a.times.taken.is_none()).map(|a| a.times.post.clone()).collect();
        let rows = |f: &dyn Fn(&AskTimes) -> bool| asks.iter().filter(|a| f(&a.times)).map(|a| a.times.post.clone()).collect::<Vec<_>>();
        let no_commit = rows(&|t| t.taken.is_some() && t.first_commit.is_none());
        let open = rows(&|t| t.taken.is_some() && t.first_commit.is_none() && t.closed.is_none());
        let answered = rows(&|t| t.closed.is_some() && t.first_commit.is_none());
        Now { at: at.to_string(), agents, asks, not_taken, no_commit, open, answered, quiet, reds, old_reds }
    }
}

/// What a read of his asks' courses and of [`Graph::now`] takes of the edges by a moment: from the kept index
/// (`graph_kept::KeptView`) or from one pass over every edge ([`Walked`]), each the same (paper-research N-1446 p1828).
pub(crate) trait Course<'g> {
    /// The edges by then at X (either end) of one of KINDS, in the graph's order.
    fn near(&self, x: &str, kinds: &[EdgeKind]) -> Vec<&'g Edge>;
    /// The kind of node X; none where X is no node.
    fn kind_of(&self, x: &str) -> Option<NodeKind>;
    /// The edges by then at X whose other end (X itself, for an edge from X to X) is a commit, in the graph's order.
    fn commits_at(&self, x: &str) -> Vec<&'g Edge>;
    /// Whether an edge by then is at X.
    fn touched(&self, x: &str) -> bool;
    /// Who wrote X by then (its last `wrote`), X a post or a ticket (a hand-in).
    fn writer(&self, x: &str) -> Option<String>;
    /// Whether post X asks an agent by then (an `asks` from it, the ASKED ledger's).
    fn asking(&self, x: &str) -> bool;
    /// Of ticket T's posts from number N on, the first taken by a session by then, to an agent not his: when, the agent
    /// and the post (a `handed`).
    fn first_handed(&self, t: &str, n: u32) -> Option<Reached>;
    /// Of the answers by then to ticket T's posts from number N on (`answered`, `answered_upto`) by a post an agent wrote,
    /// not he, the first: when, its writer and the answering post.
    fn first_answered(&self, t: &str, n: u32) -> Option<Reached>;
    /// Whether the whole graph, at any time, shows a turn of agent X, a delivery that began one or a start of it
    /// (`turn_of`, `prompted`, `start_of` into it).
    fn has_turns(&self, x: &str) -> bool;
    /// Session X's last line by then, as the graph's edges from its lines hold them: the latest time, the last line at it.
    fn session_last(&self, x: &str) -> Option<(At, usize)>;
    /// The `ended` edges by then from a job to a commit.
    fn job_ends(&self) -> Vec<&'g Edge>;
    /// The first `on` from C by then with a time, by time, the first of those at the least: the branch commit C was made
    /// on.
    fn first_on(&self, c: &str) -> Option<&'g Edge>;
    /// The agents' nodes, at any time, in the graph's order.
    fn agents(&self) -> Vec<&'g Node>;
}

/// One pass over every edge by a moment, as each of the graph's reads once made it: the kept index's reference
/// ([`Course`]; [`Graph::ask_times_walked`] and the other `*_walked`).
pub(crate) struct Walked<'g> {
    g: &'g Graph,
    upto: Option<String>,
    edges: Vec<&'g Edge>,
    by: BTreeMap<&'g str, Vec<&'g Edge>>,
    // a post's writer, and a hand-in's: the ledger's answer is either (`closed`)
    writer: BTreeMap<&'g str, &'g str>,
    // by ticket, with the post's number: each post a session took (`handed`, to an agent not his), and each post an
    // agent's post answered (`answered`, his own answers left out)
    handed: BTreeMap<&'g str, Vec<(u32, &'g Edge)>>,
    answered: BTreeMap<&'g str, Vec<(u32, &'g Edge)>>,
    // what only `now` reads, made when it first does
    has_turns: std::cell::OnceCell<BTreeSet<&'g str>>,
    session_last: std::cell::OnceCell<BTreeMap<&'g str, (&'g At, usize)>>,
}

impl<'g> Walked<'g> {
    fn new(g: &'g Graph, upto: Option<&str>) -> Walked<'g> {
        let owner = agent_id("owner");
        let edges: Vec<&Edge> = g.edges.iter().filter(|e| by_then(&e.when, upto)).collect();
        let by = around(&edges);
        let kind = |id: &str| g.get(id).map(|n| n.kind);
        let writer: BTreeMap<&str, &str> = edges.iter().filter(|e| e.kind == EdgeKind::Wrote && matches!(kind(&e.to), Some(NodeKind::Post | NodeKind::Ticket))).map(|e| (e.to.as_str(), e.from.as_str())).collect();
        let mut handed: BTreeMap<&str, Vec<(u32, &Edge)>> = BTreeMap::new();
        let mut answered: BTreeMap<&str, Vec<(u32, &Edge)>> = BTreeMap::new();
        for e in edges.iter().copied() {
            let post = match e.kind {
                EdgeKind::Handed if e.to != owner => &e.from,
                EdgeKind::Answered | EdgeKind::AnsweredUpto if writer.get(e.from.as_str()).is_some_and(|a| *a != owner) => &e.to,
                _ => continue,
            };
            let Some((t, Some(n))) = crate::post_or_ticket(post) else { continue };
            (if e.kind == EdgeKind::Handed { &mut handed } else { &mut answered }).entry(t).or_default().push((n, e));
        }
        Walked { g, upto: upto.map(str::to_string), edges, by, writer, handed, answered, has_turns: Default::default(), session_last: Default::default() }
    }

    /// The first of XS: the least by (when, id, via).
    fn first(xs: impl Iterator<Item = Reached>) -> Option<Reached> {
        xs.min_by(|a, b| (&a.at, &a.id, &a.via).cmp(&(&b.at, &b.id, &b.via)))
    }

    /// E's time as a point reached by then, at ID through VIA.
    fn point(&self, e: &Edge, id: &str, via: &str) -> Option<Reached> {
        e.when.clone().filter(|a| self.upto.as_deref().is_none_or(|u| a.as_str() <= u)).map(|at| Reached { at, id: id.to_string(), via: Some(via.to_string()) })
    }
}

impl<'g> Course<'g> for Walked<'g> {
    fn near(&self, x: &str, kinds: &[EdgeKind]) -> Vec<&'g Edge> {
        self.by.get(x).into_iter().flatten().copied().filter(|e| kinds.contains(&e.kind)).collect()
    }

    fn kind_of(&self, x: &str) -> Option<NodeKind> {
        self.g.get(x).map(|n| n.kind)
    }

    fn commits_at(&self, x: &str) -> Vec<&'g Edge> {
        self.by.get(x).into_iter().flatten().copied().filter(|e| self.kind_of(if e.from == x { &e.to } else { &e.from }) == Some(NodeKind::Commit)).collect()
    }

    fn touched(&self, x: &str) -> bool {
        self.by.contains_key(x)
    }

    fn writer(&self, x: &str) -> Option<String> {
        self.writer.get(x).map(|w| w.to_string())
    }

    fn asking(&self, x: &str) -> bool {
        self.near(x, &[EdgeKind::Asks]).iter().any(|e| e.from == x)
    }

    fn first_handed(&self, t: &str, n: u32) -> Option<Reached> {
        Self::first(from_post(&self.handed, Some((t, n))).into_iter().filter_map(|e| self.point(e, &e.to, &e.from)))
    }

    fn first_answered(&self, t: &str, n: u32) -> Option<Reached> {
        Self::first(from_post(&self.answered, Some((t, n))).into_iter().filter_map(|e| self.point(e, self.writer.get(e.from.as_str())?, &e.from)))
    }

    fn has_turns(&self, x: &str) -> bool {
        self.has_turns.get_or_init(|| self.g.edges.iter().filter(|e| matches!(e.kind, EdgeKind::TurnOf | EdgeKind::Prompted | EdgeKind::StartOf)).map(|e| e.to.as_str()).collect()).contains(x)
    }

    fn session_last(&self, x: &str) -> Option<(At, usize)> {
        let lasts = self.session_last.get_or_init(|| {
            let mut m: BTreeMap<&str, (&At, usize)> = BTreeMap::new();
            for e in self.edges.iter().copied() {
                if let (Source::Session(x, n), Some(w)) = (&e.source, &e.when) {
                    let last = m.entry(x.as_str()).or_insert((w, *n));
                    *last = (*last).max((w, *n));
                }
            }
            m
        });
        lasts.get(x).map(|(w, n)| ((*w).clone(), *n))
    }

    fn job_ends(&self) -> Vec<&'g Edge> {
        self.edges.iter().copied().filter(|e| e.kind == EdgeKind::Ended && self.kind_of(&e.from) == Some(NodeKind::Job) && self.kind_of(&e.to) == Some(NodeKind::Commit)).collect()
    }

    fn first_on(&self, c: &str) -> Option<&'g Edge> {
        self.near(c, &[EdgeKind::On]).into_iter().filter(|o| o.from == c && o.when.is_some()).min_by(|a, b| a.when.cmp(&b.when))
    }

    fn agents(&self) -> Vec<&'g Node> {
        self.g.nodes.iter().filter(|n| n.kind == NodeKind::Agent).collect()
    }
}

/// Whether a time WHEN is at or before UPTO (any is, with no UPTO); a node or an edge with no time is there at every
/// time, as what a release carries.
pub(crate) fn by_then(when: &Option<At>, upto: Option<&str>) -> bool {
    match (when, upto) {
        (Some(w), Some(u)) => w.as_str() <= u,
        _ => true,
    }
}

/// The last of EDGES by time, the last given of those at one time; none with no time.
fn latest<'a>(edges: Vec<&'a Edge>) -> Option<&'a Edge> {
    edges.into_iter().filter(|e| e.when.is_some()).max_by(|a, b| a.when.cmp(&b.when))
}

/// [`latest`] of those of EDGES whose end `to` is of KIND, as V reads kinds: the last of them at the latest time. Read
/// from the last on, an end's kind is read only for an edge later than the latest found yet, and not for one of NOT,
/// edges known to end at another kind, in the graph's order as EDGES are (an agent's `wrote` to its commits, for its
/// last post): not for each edge (`Graph::now`'s agents' `wrote` and `ran`; paper-research N-1446 p1922 3, p2072 2).
fn latest_to<'a>(edges: &[&'a Edge], kind: NodeKind, not: &[&Edge], v: &dyn Course<'_>) -> Option<&'a Edge> {
    let mut best: Option<&'a Edge> = None;
    // the graph's order is its edges' places in it: NOT's after E's place are passed as E goes back
    let place = |e: &Edge| std::ptr::from_ref(e);
    let mut not = not.iter().rev().copied().peekable();
    for e in edges.iter().rev().copied().filter(|e| e.when.is_some()) {
        while not.next_if(|n| place(*n) > place(e)).is_some() {}
        if not.peek().is_some_and(|n| std::ptr::eq(*n, e)) {
            continue;
        }
        if best.is_none_or(|b| e.when > b.when) && v.kind_of(&e.to) == Some(kind) {
            best = Some(e);
        }
    }
    best
}

/// EDGES by each node they touch, either end.
fn around<'a>(edges: &[&'a Edge]) -> BTreeMap<&'a str, Vec<&'a Edge>> {
    let mut by: BTreeMap<&str, Vec<&Edge>> = BTreeMap::new();
    for e in edges.iter().copied() {
        by.entry(e.from.as_str()).or_default().push(e);
        by.entry(e.to.as_str()).or_default().push(e);
    }
    by
}

/// What is worked on at a moment, and what is not ([`Graph::now`]): coordd's `GraphAnswer::now`, read back by the window
/// for its canvas's right edge (proto-window N-1446 p1383, head 2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Now {
    pub at: At,
    /// Each of this coord's agents the graph has an edge of by then and ever shows a turn or a start of (its turn's end,
    /// a delivery that began one, a start of it), his own node aside, by its id. The engine's own writers (coord,
    /// coordd, box) have none, and another project's agents (PROJECT/NAME) are not this coord's: no row, so their work
    /// opens no turn here (N-1446 p1481, p1510).
    pub agents: Vec<AgentNow>,
    /// His asks not yet in his window, in the order he asked them.
    pub asks: Vec<AskNow>,
    /// Of those, the ones reached and never taken: the ones he needs to see (paper-research N-1446 p1392).
    pub not_taken: Vec<String>,
    /// And the ones taken with no commit naming them yet.
    pub no_commit: Vec<String>,
    /// By the ASKED ledger (paper-research N-1446 p1446): of those, the ones still open in it, each asking an agent, as
    /// every ask does (p1561);
    pub open: Vec<String>,
    /// and of all of them, the ones it holds closed with no commit: closed in words, the ones answered.
    pub answered: Vec<String>,
    /// The agents in a turn whose session has a line by then ([`AgentNow::session`]), the one whose session has written
    /// nothing for longest first (paper-research N-1446 p1429 4): how long is shown, not judged.
    pub quiet: Vec<String>,
    /// Each branch an agent works on by then (its last commit's or its last job's) whose last job to end by then ended
    /// red, the one red for longest first ([`Red`]; p1429 4).
    pub reds: Vec<Red>,
    /// How many branches no agent works on by then end red too: old work, a red test's branch among them.
    pub old_reds: usize,
}

/// A branch whose last job to end by a moment ended red, with no green since ([`Now::reds`]): that job, and SINCE, when
/// the first red after the branch's last green ended. A job's branch is the one its commit was made on (its first `on`,
/// git's reflog); a job on a commit with no branch in the graph is on none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Red {
    pub branch: String,
    pub job: JobNow,
    pub since: At,
}

/// An agent's session's last line by a moment, as the graph holds its lines (a request sent, a text loaded, a file
/// read; `follow::side`): the session, the line, its time ([`AgentNow::session`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionNow {
    pub session: String,
    pub line: usize,
    pub at: At,
}

/// An agent at a moment: its turn, and the last of each kind of its work.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentNow {
    /// Its name, as its node's id has it after `agent:`.
    pub agent: String,
    /// In a turn: since when, and what shows it. None: not in one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn: Option<TurnNow>,
    /// Not in a turn: since its last turn ended.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_since: Option<At>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_post: Option<Reached>,
    /// The items it started that are not closed, each when it started it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Reached>,
    /// Its last commit: its subject, and the branch it was made on (or the first that came to it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_commit: Option<Work>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_job: Option<JobNow>,
    /// Its last start (roster's `began`): phase, model and effort, and its session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub began: Option<Work>,
    /// The last line by then of its sessions the record names (`session_of`), the latest of them; none where the graph
    /// holds no line of one (paper-research N-1446 p1429 4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionNow>,
}

/// An agent's turn running: since SINCE, which edge of KIND to or from BY shows. A delivery that began a turn (its
/// `prompted`) says when it began; with none since the last turn's end, its first work after it (a post it wrote, a
/// file it edited, a job it ran, an item it started, a start of it): the turn began by then.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnNow {
    pub since: At,
    pub by: String,
    pub kind: EdgeKind,
}

/// A piece of work at a moment: when, its node, what its source says it is, and where it is: a commit's branch, a
/// start's session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Work {
    pub at: At,
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub what: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
}

/// A job at a moment: started at AT, on COMMIT; running while ENDED is none, OK once it has ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobNow {
    pub at: At,
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub what: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended: Option<At>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
}

/// One of his asks at a moment: the last point of its course it has reached, and who that was ([`Graph::asks_now`];
/// [`Now::asks`] holds those not in his window yet).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskNow {
    pub point: Point,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last: Option<Reached>,
    /// The agent behind it: who was handed it or took it, who wrote the commit, who ran the job; his own, at `asked`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub who: Option<String>,
    pub times: AskTimes,
}

/// A point of an ask's course ([`AskTimes`]), in its order ([`AskTimes::points`]). `InWindow` is never an ask's last
/// point in [`Now::asks`], which holds only those not in his window yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Point {
    Asked,
    Reached,
    Taken,
    /// closed in the ASKED ledger
    Closed,
    FirstCommit,
    Merged,
    FirstGreen,
    InWindow,
}

/// The edges of BY (by ticket, with their post's number) of ticket ON's posts from its number on.
fn from_post<'a>(by: &BTreeMap<&str, Vec<(u32, &'a Edge)>>, on: Option<(&str, u32)>) -> Vec<&'a Edge> {
    let Some((t, n)) = on else { return Vec::new() };
    by.get(t).into_iter().flatten().filter(|(k, _)| *k >= n).map(|(_, e)| *e).collect()
}

/// A point of an ask's course: when the graph first reached it, the node that reached it (the agent, the commit, the
/// job, the build), and what it came through (the post handed or answered, the item started).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reached {
    pub at: At,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
}

/// The course of one of his posts, each point the first time the graph's edges reach it: derived from them, never
/// written by hand. None where nothing reaches it yet, or where its source gives no time. An ask reached and never
/// taken is the one he needs to see (paper-research N-1446 p1392): a delivery is not someone taking the work.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskTimes {
    pub post: String,
    /// His post's `wrote`.
    pub asked: Option<At>,
    /// An agent's session was handed it: a `handed` from it, or from a later post of its ticket (a `given` takes each
    /// post up to the one it names).
    pub reached: Option<Reached>,
    /// An agent took it up: its post `answered` it, or a later post of its ticket; or it `started` an item whose note
    /// it is.
    pub taken: Option<Reached>,
    /// The first commit an edge joins to it (its subject naming it, its `Item:`).
    pub first_commit: Option<Reached>,
    /// The first merge of one of those commits (`merged_into`).
    pub merged: Option<Reached>,
    /// The first job to end green (`ended`, its job `ok`) on one of those commits, or for the post or an item whose note
    /// it is (the job's `named`, its `items`; paper-research N-1446 p1422, 4): when it ended.
    pub first_green: Option<Reached>,
    /// The first build carrying one of those commits or merges (`built_into`) to be installed in his window
    /// (`installed`): when his window says it had it (the `window_release` line's `since`, by his window's clock), or
    /// on a line that says none, the line's own time, the record's, when coordd heard of it (graph_runs; the build's
    /// `what` says which; paper-research N-1446 p1392).
    pub in_window: Option<Reached>,
    /// The agents the ASKED ledger says it asks to act (`asks`), by their ids: none where the ledger says nothing of it
    /// (paper-research N-1446 p1446).
    pub asks: Vec<String>,
    /// Its close in that ledger (`closed_by`), each agent it asks having its answer: when, the agent whose answer
    /// closed it, `via` that answer. None while one has none, or after it asks one more.
    pub closed: Option<Reached>,
    /// His sign-off, his word of worth (`signed_off`; paper-research N-1446 p1786): the last by then, none when he has
    /// not signed it off or took it back. Not a [`Point`]: he may sign off at any point of the course, and a window
    /// before it reads every course as it did (left out when none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed: Option<Signed>,
}

/// His sign-off of one of his asks ([`AskTimes::signed`]): when, and his WORTH, one of `machine::WORTHS`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signed {
    pub at: At,
    pub worth: String,
}

/// What one of his asks cost ([`Graph::costs_at`]; paper-research N-1446 p1429 2): the requests sent for it, by agent
/// and by model and effort (`handed`), its cost; beside it, never added in, those of steps on posts that name it
/// (`named`). A request counts whole for each ask it is for, so a sum over his asks counts a shared one once for each
/// (`shared`). What the sessions' lines do not carry is not here: a request with no response (`Spent`'s `requests` less
/// its `carried`), and the sessions of agents the record names no `given` place for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cost {
    pub post: String,
    /// The requests sent in a step that handed it (a giving that brought it; the request's `for`).
    pub handed: Vec<Spent>,
    /// Those sent in a step that handed a post naming it (`named`), and not it: a relay of it, a post on its work, a
    /// test that took it for its example. Named by, shown beside its cost and never added into it, nor into a total or
    /// a ranking (paper-research N-1446 p1535).
    pub named: Vec<Spent>,
    /// How many of its `handed` requests also count for another of his asks: their step handed more than one.
    pub shared: usize,
}

/// Requests of one agent at one model and effort, for one ask ([`Cost`]): how many, how many of them the session holds
/// a response to, and those responses' tokens (`Node::tokens`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spent {
    pub agent: String,
    /// The requests' model and effort, as their node says (`what`).
    pub what: String,
    pub requests: usize,
    pub carried: usize,
    pub tokens: Tokens,
}

impl AskTimes {
    /// Each point of the course in its order, with its time and the node that reached it (none at `Asked`: his own
    /// post), both none where the graph has not reached it: the order `Graph::now`'s last point and the window's spans
    /// between them read (paper-research N-1446 p1429, 1).
    pub fn points(&self) -> [(Point, Option<&At>, Option<&Reached>); 8] {
        // a function, not a closure: what it gives borrows from X (a closure's would be a lifetime of its own)
        fn r(p: Point, x: &Option<Reached>) -> (Point, Option<&At>, Option<&Reached>) {
            (p, x.as_ref().map(|x| &x.at), x.as_ref())
        }
        [
            (Point::Asked, self.asked.as_ref(), None),
            r(Point::Reached, &self.reached),
            r(Point::Taken, &self.taken),
            r(Point::Closed, &self.closed),
            r(Point::FirstCommit, &self.first_commit),
            r(Point::Merged, &self.merged),
            r(Point::FirstGreen, &self.first_green),
            r(Point::InWindow, &self.in_window),
        ]
    }
}

/// The way edge E carries a message ([`Way`]): a step's call on the wire is the engine's; an edge from a post that
/// woke, informed, answered (`answered`, `answered_upto`), began a turn of (`prompted`) or was handed to an agent is by
/// the post's writer (its `wrote`) to that agent, `agent:owner` being him; a program's post, whose writer is its ticket
/// (the program run, `posted`), is the engine's (paper-research N-1446 p1439). None for any other edge, or a post with
/// no writer in G.
pub fn way(g: &Graph, e: &Edge) -> Option<Way> {
    if e.kind == EdgeKind::Called {
        return Some(Way::Engine);
    }
    if !matches!(e.kind, EdgeKind::Woke | EdgeKind::Informed | EdgeKind::Answered | EdgeKind::AnsweredUpto | EdgeKind::Handed | EdgeKind::Prompted) || g.get(&e.from)?.kind != NodeKind::Post {
        return None;
    }
    let to = match g.get(&e.to)?.kind {
        NodeKind::Agent => e.to.as_str(),
        _ => return None,
    };
    let by = g.edges.iter().find(|x| x.kind == EdgeKind::Wrote && x.to == e.from)?.from.as_str();
    // a program's post, by its ticket (the program run): the engine's, no agent's
    if g.get(by).is_some_and(|n| n.kind == NodeKind::Ticket) {
        return Some(Way::Engine);
    }
    let owner = agent_id("owner");
    Some(match (by == owner, to == owner) {
        (true, _) => Way::OwnerToAgent,
        (false, true) => Way::AgentToOwner,
        (false, false) => Way::AgentToAgent,
    })
}

pub fn agent_id(name: &str) -> String {
    format!("agent:{name}")
}

pub fn post_id(m: &Machine, t: u64, n: u32) -> String {
    format!("{}.p{n}", m.id(t))
}

pub fn step_id(m: &Machine, t: u64, step: u32) -> String {
    format!("{}.s{step}", m.id(t))
}

/// The node of AGENT's turn that ended (or was cut) at record line LINE.
pub fn turn_id(agent: &str, line: usize) -> String {
    format!("turn:{agent}@{line}")
}

/// The node of the slice of AGENT's session that took DELIVERY at record line LINE (`given`'s `where`): the giving,
/// never its place, which coordd looks up in the record when the window asks by this id (paper-research N-1446 p1380).
pub fn slice_id(agent: &str, delivery: &str, line: usize) -> String {
    format!("slice:{agent}:{delivery}@{line}")
}

/// AGENT, DELIVERY and LINE of a slice's id ([`slice_id`]); None for any other id. No agent's name holds a `:`, nor a
/// delivery's id a `:` or an `@`.
pub fn slice_of(id: &str) -> Option<(&str, &str, usize)> {
    let (agent, rest) = id.strip_prefix("slice:")?.split_once(':')?;
    let (delivery, line) = rest.rsplit_once('@')?;
    Some((agent, delivery, line.parse().ok()?)).filter(|(a, d, _)| !a.is_empty() && !d.is_empty() && !d.contains(':'))
}

/// A `when EVENT` handler as the Map keys it (yaaihh map/layout.rs `handled`): `handler:KIND:EVENT`, KIND the kind the
/// `when` was written in (`syntax::When::from`, a run's `Ticket::when`), not the ticket's, which takes base's `when`s
/// for the events it has none of (`check::inherit`), so one handler is one node (paper-research N-1446 p1375). Never
/// by its program's text, which is the whole handler (docket-edge6's first run, p1362).
pub fn handler_id(kind: &str, event: &str) -> String {
    format!("handler:{kind}:{event}")
}

/// Makes the graph along a replay: [`Builder::after`] as `Machine::replay_with`'s AFTER, then [`Builder::graph`].
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Builder {
    pub graph: Graph,
    /// Where the record and the graph disagree, as the replay's problems are written (`line N: ...`): a line the
    /// machine wrote that names what the machine, replayed, does not hold (paper-research N-1446 p1378).
    pub problems: Vec<String>,
    /// For each ticket, how many of its posts are in the graph.
    posts: BTreeMap<u64, usize>,
    /// The items an agent started (`started`), whose end is an edge.
    started: BTreeSet<u64>,
    /// What each giving has handed, and the posts halted until a line lets them go (`graph_posts::Delivering`).
    delivering: crate::graph_posts::Delivering,
    /// Each agent's posts since its last turn end or cut, for the turn they were written in (`written_in`).
    unturned: BTreeMap<String, Vec<String>>,
    /// The starts each agent ran roster for since its last turn end or cut, for the turn they were made in (`made_in`),
    /// by the starter's name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    made: BTreeMap<String, Vec<String>>,
    /// Each agent's turn ends by the session its last message is in (the file's name, the session's id): where each
    /// turn's last message ends, in bytes, and the turn's record line, in the record's order. A start on a cut copy of
    /// one of those sessions goes off the last of them that ends at or before its cut (path fix 6).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    ends: BTreeMap<String, BTreeMap<String, Vec<(u64, usize)>>>,
    /// The sessions that starts on cut copies began (path fix 6), each with its agent, its line's number among the
    /// agent's lines, and how many turns that ended the line it went off had up to the cut (a cut turn has no place,
    /// so it is not counted: `ends`), so that its own turns are counted on from there. Line 1 is every session no cut
    /// made, line 2 the agent's first cut copy, and so on.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    lines: BTreeMap<String, (String, u32, usize)>,
    /// The byte each of those sessions was cut after: where its own part of its transcript begins.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    cuts: BTreeMap<String, u64>,
    /// His system-prompt edits by agent and session, each where it falls in the session's transcript ([`Restarted`]),
    /// for the window's history of the agent (path fix 6 point 3, accounts-lead N-1446 p2524): a session cut from
    /// another holds the other's edits before the cut, then its own.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    restarted: BTreeMap<String, BTreeMap<String, Vec<Restarted>>>,
    /// His posts by ticket (the hidden ones aside), each with what the ASKED ledger last said of it ([`Asked`]).
    his: BTreeMap<u64, BTreeMap<u32, Asked>>,
    /// His posts the first head was handed in for at the inputs taken since coordd last took them (a ticket opened
    /// with its `for`, `Machine::handed_in_for`), by ticket and post: where coordd raises `asked` (`machine::ASKED`;
    /// N-1511 p47), so his goal opens at its first head, not at his post, and a post no work answers opens none
    /// (paper-research N-1446 p2354): `Machine::first_head_for`. Never saved.
    #[serde(skip)]
    pub first_handed: Vec<(u64, u32)>,
}

/// His edit of an agent's system prompt (roster's `began`, phase `system`) as the agent's history shows it (path fix 6
/// point 3, accounts-lead N-1446 p2524): where in a session's transcript it falls, when, his version, and, made from a
/// turn, the line it began ([`Builder::restarted`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Restarted {
    /// The byte of the transcript it falls at. Made from a turn, its cut. With Now, where the session's last turn that
    /// ended ends, or else where its own part begins (its cut, or 0): a turn cut after that is not placed.
    pub offset: u64,
    /// The `began` line's time.
    pub at: At,
    /// His version of the system prompt it started with (`system`), empty when the line names none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub system: String,
    /// Made from a turn, the line it began; None with Now, which begins none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<Branched>,
}

/// The line his edit from a turn began (`Builder::lines`): its number among the agent's lines, the session it went off
/// and that one's number, and how many turns that ended the old line had up to the cut (0 when the record holds none of
/// its turns).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Branched {
    pub k: u32,
    /// The session it went off, by id (its transcript's file name). coordd's `Transcript` gives it as that transcript's
    /// path, beside the session's own, which a window's `Request::Transcript` names to read the old line on from the
    /// cut.
    pub from: String,
    pub from_line: u32,
    pub turns: usize,
}

/// What the ASKED ledger last said of one of his posts (`Machine::asked`), as the Builder read it: the agents it has
/// asked, whether one of them has no answer, and whether it is closed, each it asks having one.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Asked {
    who: BTreeSet<String>,
    open: bool,
    closed: bool,
}

impl Builder {
    /// His system-prompt edits that fall in AGENT's session SESSION (its id, its transcript's file name), in the
    /// transcript's order (path fix 6 point 3).
    pub fn restarted(&self, agent: &str, session: &str) -> &[Restarted] {
        self.restarted.get(agent).and_then(|s| s.get(session)).map_or(&[], Vec::as_slice)
    }

    /// After the input at index I of the record (GROUP: it and its recorded outputs), with M as it stands then. A post
    /// is the line that made it appear on its ticket; a ticket the line that first named it. What it makes is dated
    /// from the group's lines ([`Graph::date_from`]).
    pub fn after(&mut self, m: &Machine, i: usize, group: &[Line]) {
        let made = (self.graph.nodes.len(), self.graph.edges.len());
        let g = &mut self.graph;
        let tickets: BTreeSet<u64> = group.iter().flat_map(crate::machine::line_tickets).collect();
        for (k, l) in group.iter().enumerate() {
            let line = i + k + 1;
            crate::graph_runs::add(g, &mut self.problems, m, line, l);
            crate::graph_posts::add(g, &mut self.delivering, m, line, l);
            match l {
                // a turn's end, or its cut (its process went, or its session started again, with no end)
                Line::TurnEnded { agent, .. } | Line::TurnCut { agent, .. } => {
                    let a = g.node(agent_id(agent), NodeKind::Agent, line);
                    let turn = g.node(turn_id(agent, line), NodeKind::Turn, line);
                    g.edge(&turn, &a, EdgeKind::TurnOf, line);
                    // where its last message ends in its session, for a start on a copy cut from it later (path fix
                    // 6); a cut turn has no place
                    if let Line::TurnEnded { text, .. } = l
                        && let Some(session) = std::path::Path::new(&text.file).file_stem().and_then(|s| s.to_str()).filter(|s| !s.is_empty())
                    {
                        self.ends.entry(agent.clone()).or_default().entry(session.to_string()).or_default().push((text.offset + text.len, line));
                    }
                }
                Line::Step { t, step, call, .. } => {
                    let tid = g.node(m.id(*t), NodeKind::Ticket, line);
                    let s = g.node(step_id(m, *t, *step), NodeKind::Step, line);
                    g.edge(&s, &tid, EdgeKind::StepOf, line);
                    let c = g.node(format!("call:{call}"), NodeKind::CallFile, line);
                    g.edge(&s, &c, EdgeKind::CameFrom, line);
                }
                // an item → its note, his post or a ticket, as its opening line's form has it
                Line::Open { t, kind, form, .. } if kind == "item" => {
                    if let Some((tk, n)) = form.get("note").and_then(|v| v.as_str()).and_then(crate::post_or_ticket) {
                        let item = g.node(m.id(*t), NodeKind::Ticket, line);
                        let note = match n {
                            Some(n) => g.node(format!("{tk}.p{n}"), NodeKind::Post, line),
                            None => g.node(tk, NodeKind::Ticket, line),
                        };
                        g.edge(&item, &note, EdgeKind::CameFrom, line);
                    }
                }
                Line::Started { t, by, .. } => {
                    let a = g.node(agent_id(by), NodeKind::Agent, line);
                    let item = g.node(m.id(*t), NodeKind::Ticket, line);
                    g.edge(&a, &item, EdgeKind::Started, line);
                    self.started.insert(*t);
                }
                // a started item's end: by the agent that ended it (`coord end`), or by its program (`closed`)
                Line::Ended { t, by, .. } if self.started.contains(t) => {
                    let item = g.node(m.id(*t), NodeKind::Ticket, line);
                    let a = g.node(agent_id(by), NodeKind::Agent, line);
                    g.edge(&item, &a, EdgeKind::Closed, line);
                }
                Line::Closed { t, .. } if self.started.contains(t) => {
                    let item = g.node(m.id(*t), NodeKind::Ticket, line);
                    let p = g.node("program:item", NodeKind::Program, line);
                    g.edge(&item, &p, EdgeKind::Closed, line);
                }
                // a delivery that was the prompt of a turn (`delivered`, `began`): the turn began then, as its session
                // wrote it; its post, its ticket, or its step
                Line::Event { name, value, .. } if name == crate::machine::DELIVERED && value.get("began").and_then(|v| v.as_bool()) == Some(true) => {
                    let (Some(to), Some(d)) = (value.get("to").and_then(|v| v.as_str()), value.get("delivery").and_then(|v| v.as_str())) else { continue };
                    let from = match (crate::post_or_ticket(d), crate::machine::step_id(d)) {
                        (Some((tk, Some(n))), _) => g.node(format!("{tk}.p{n}"), NodeKind::Post, line),
                        (Some((tk, None)), _) => g.node(tk, NodeKind::Ticket, line),
                        (None, Some((t, n, _))) => g.node(step_id(m, t, n), NodeKind::Step, line),
                        (None, None) => continue,
                    };
                    let a = g.node(agent_id(to), NodeKind::Agent, line);
                    let at = value.get("at").and_then(|v| v.as_str()).filter(|a| !a.is_empty()).map(str::to_string);
                    g.edge_from(&from, &a, EdgeKind::Prompted, Source::Line(line), at);
                }
                // his sign-off of an ask (`signed_off`, paper-research N-1446 p1786): him → his post, his word in the
                // edge's `what`, `none` when he took it back
                Line::Event { name, value, .. } if name == crate::machine::SIGNED_OFF => {
                    let num = |k: &str| value.get(k).and_then(|v| v.as_u64());
                    let (Some(t), Some(n), Some(worth)) = (num("t"), num("n").and_then(|n| u32::try_from(n).ok()), value.get("worth").and_then(|v| v.as_str())) else { continue };
                    let me = g.node(agent_id("owner"), NodeKind::Agent, line);
                    let post = g.node(post_id(m, t, n), NodeKind::Post, line);
                    g.edge(&me, &post, EdgeKind::SignedOff, line);
                    if let Some(e) = g.edges.last_mut() {
                        e.what = worth.to_string();
                    }
                }
                // a runner job's start or end (`act` `job`, build-runner's line, docket-core N-1446 p1397)
                Line::Act { at, agent, is, more, .. } if is == "job" => crate::graph_git::job_line(g, line, at, agent, more),
                // a release act (`act` `release`, release.sh's, docket-core N-1446 p1539), wherever it lands (p1541)
                Line::Act { at, is, more, .. } if is == "release" => crate::graph_git::release_line(g, at, more),
                // an agent's start (roster's `began`, docket-core N-1446 p1397): its phase, kind, model and effort,
                // the folder it starts in and who started it; the session it runs (a fork's own is not named: then
                // the one it forks); and the post or ticket its first prompt names, its brief (the file view, p1441)
                Line::Act { at, agent, is, more, .. } if is == "began" => {
                    let field = |k: &str| more.get(k).and_then(|v| v.as_str()).filter(|v| !v.is_empty());
                    // his system-prompt edit, for where it falls in the agent's history (`restarted`)
                    let his_edit = field("phase") == Some("system");
                    let s = g.node(format!("start:{agent}@{line}"), NodeKind::Start, line);
                    let who = [field("kind"), field("model")].into_iter().flatten().collect::<Vec<_>>().join(" ");
                    // his version of its system prompt it starts with (path-system's `system`, `replace`)
                    let replace = field("replace").is_some();
                    let system = field("system").map(|n| if replace { format!("system prompt {n} {IN_PLACE}") } else { format!("system prompt {n}") });
                    let how: Vec<String> = [Some(who).filter(|w| !w.is_empty()), field("effort").map(|e| format!("effort {e}")), field("dir").map(|d| format!("in {d}")), system, field("starter").map(|b| format!("by {b}"))]
                        .into_iter()
                        .flatten()
                        .collect();
                    if let Some(n) = g.get_mut(&s) {
                        n.what = format!("{}: {}", field("phase").unwrap_or("start"), how.join(", ")).trim_end_matches([':', ' ']).to_string();
                    }
                    let a = g.node(agent_id(agent), NodeKind::Agent, line);
                    g.edge(&s, &a, EdgeKind::StartOf, line);
                    if let Some(session) = field("session").or(field("from")) {
                        let x = g.node(format!("session:{session}"), NodeKind::Session, line);
                        g.edge(&s, &x, EdgeKind::CameFrom, line);
                    }
                    // a start on a copy of session FROM cut after byte CUT (`cut`: his system-prompt edit from a turn,
                    // or a fork's part 4; path fix 6, accounts-lead N-1446 p2524): its session is a line of its own,
                    // going off FROM's line at the turn it was cut after. That turn is the last of FROM's whose message
                    // ends at or before CUT: the copy is cut after a turn's end mark (follow.rs `turn_end`), and the
                    // turn's message comes before its mark. Line 1 stays as it happened. No cut, no line: a start on
                    // its session as it stands (his edit with Now), a fork's agent (`--fork-session`, another agent's
                    // line), and every start recorded before 726dc806
                    if let (Some(session), Some(from), Some(cut)) = (field("session"), field("from"), field("cut").and_then(|c| c.parse::<u64>().ok())) {
                        let ends = self.ends.get(agent).and_then(|s| s.get(from));
                        let off = ends.and_then(|e| e.iter().rposition(|(end, _)| *end <= cut).map(|i| (i, e[i].1)));
                        let (from_line, before) = self.lines.get(from).map_or((1, 0), |(_, k, n)| (*k, *n));
                        let k = match self.lines.get(session) {
                            Some((_, k, _)) => *k,
                            None => 2 + self.lines.values().filter(|(a, ..)| a == agent).count() as u32,
                        };
                        let turns = before + off.map_or(0, |(i, _)| i + 1);
                        self.lines.insert(session.to_string(), (agent.clone(), k, turns));
                        if let Some((_, at)) = off {
                            let x = g.node(format!("session:{session}"), NodeKind::Session, line);
                            let turn = g.node(turn_id(agent, at), NodeKind::Turn, line);
                            g.edge(&x, &turn, EdgeKind::CameFrom, line);
                            if let Some(e) = g.edges.last_mut() {
                                e.what = format!("branch {k}, from turn {turns} of branch {from_line}, after byte {cut} of its session");
                            }
                        }
                        // the copy holds FROM's edits before the cut, then, his, its own at the cut; a fork's part 4
                        // is drawn as before, with none
                        self.cuts.insert(session.to_string(), cut);
                        let before = self.restarted.get(agent).and_then(|s| s.get(from)).into_iter().flatten();
                        let mut marks: Vec<Restarted> = before.filter(|r| r.offset < cut).cloned().collect();
                        if his_edit {
                            let line = Some(Branched { k, from: from.to_string(), from_line, turns });
                            marks.push(Restarted { offset: cut, at: at.clone(), system: field("system").unwrap_or_default().to_string(), line });
                        }
                        if !marks.is_empty() {
                            self.restarted.entry(agent.clone()).or_default().insert(session.to_string(), marks);
                        }
                    } else if his_edit && let Some(session) = field("session") {
                        // with Now: its session as it stands, where its last turn that ended ends, or else where its own
                        // part begins
                        let last = self.ends.get(agent).and_then(|s| s.get(session)).and_then(|e| e.last()).map(|(end, _)| *end);
                        let offset = last.or(self.cuts.get(session).copied()).unwrap_or(0);
                        let mark = Restarted { offset, at: at.clone(), system: field("system").unwrap_or_default().to_string(), line: None };
                        self.restarted.entry(agent.clone()).or_default().entry(session.to_string()).or_default().push(mark);
                    }
                    let brief = match field("brief").and_then(crate::post_or_ticket) {
                        Some((tk, Some(n))) => Some(g.node(format!("{tk}.p{n}"), NodeKind::Post, line)),
                        Some((tk, None)) if crate::machine::parse_ticket(tk).is_some() => Some(g.node(tk, NodeKind::Ticket, line)),
                        _ => None,
                    };
                    // the words it was started with when they name no brief (`prompt`, which roster gives such a start
                    // whole, with `prompt_sha`; paper-research N-1446 p3041): they came from outside, so the record
                    // holds them, and they are its brief: on a fresh start the text node its session's first user message has
                    // (coordd follow.rs `text_id`, the sha256's first 12 hex), the brief edge holding the words
                    let mut words = None;
                    let brief = brief.or_else(|| {
                        let p = field("prompt")?;
                        let sha = field("prompt_sha").filter(|h| h.len() >= 12 && h.as_bytes()[..12].iter().all(u8::is_ascii_hexdigit))?;
                        let id = format!("text:{}", sha[..12].to_ascii_lowercase());
                        let new = g.get(&id).is_none();
                        let t = g.node(id, NodeKind::Text, line);
                        if new && let Some(n) = g.get_mut(&t) {
                            n.what = format!("the words it was started with, {} bytes", p.len());
                        }
                        words = Some(p);
                        Some(t)
                    });
                    if let Some(b) = brief {
                        g.edge(&s, &b, EdgeKind::Briefed, line);
                        if let Some(w) = words
                            && let Some(e) = g.edges.last_mut()
                        {
                            e.what = w.to_string();
                        }
                    }
                    // the text it gave Claude Code beside or in place of its own (`system_sha`; a hash that is none
                    // names none), one node with any text of its words
                    if let Some(sha) = field("system_sha").filter(|h| h.len() >= 12 && h.as_bytes()[..12].iter().all(u8::is_ascii_hexdigit)) {
                        let id = format!("text:{}", sha[..12].to_ascii_lowercase());
                        let what = match field("system") {
                            Some(n) if replace => format!("version {n}, {IN_PLACE}"),
                            Some(n) => format!("version {n}, appended"),
                            None => "the standing text, appended".to_string(),
                        };
                        let new = g.get(&id).is_none();
                        let t = g.node(id, NodeKind::Text, line);
                        if new && let Some(n) = g.get_mut(&t) {
                            n.what = format!("{agent}'s system prompt, {what}");
                        }
                        g.edge(&s, &t, EdgeKind::StartedWith, line);
                        if let Some(e) = g.edges.last_mut() {
                            e.what = what;
                        }
                    }
                    // the turn of its starter's it was made in, at the starter's next turn end or cut (`made_in`); he
                    // ends no turn
                    if let Some(by) = field("starter").filter(|b| *b != "owner") {
                        self.made.entry(by.to_string()).or_default().push(s.clone());
                    }
                }
                _ => {}
            }
        }
        let line = i + 1;
        // the tickets the ledger is read on: where an agent's post landed, and his posts that did
        let (mut to_read, mut his_new) = (BTreeSet::new(), Vec::new());
        for t in tickets {
            let Some(y) = m.tickets.get(&t) else { continue };
            let fresh = !self.posts.contains_key(&t);
            let tid = g.node(m.id(t), NodeKind::Ticket, line);
            if fresh {
                let p = g.node(format!("program:{}", y.kind), NodeKind::Program, line);
                g.edge(&tid, &p, EdgeKind::CameFrom, line);
            }
            let seen = self.posts.entry(t).or_insert(0);
            for p in y.posts.iter().skip(*seen) {
                // its writer: an agent, him, or the program run whose post it is, by its ticket's id (`posted`): that
                // ticket's node, never an agent named after it (paper-research N-1446 p1439)
                let a = match m.ticket_named(&p.by) {
                    Some(_) => g.node(p.by.clone(), NodeKind::Ticket, line),
                    None => g.node(agent_id(&p.by), NodeKind::Agent, line),
                };
                let pid = g.node(post_id(m, t, p.n), NodeKind::Post, line);
                g.edge(&a, &pid, EdgeKind::Wrote, line);
                g.edge(&pid, &tid, EdgeKind::PostedIn, line);
                // an agent's post waits for its writer's next turn end, the turn it was written in (`written_in`); his
                // and a program's are in none (he ends no turn, nor does a program)
                if m.ticket_named(&p.by).is_none() && p.by != "owner" {
                    self.unturned.entry(p.by.clone()).or_default().push(pid.clone());
                }
                if p.by != "owner" {
                    to_read.insert(t);
                } else if t < crate::HIDDEN {
                    his_new.push((t, p.n));
                }
            }
            *seen = (*seen).max(y.posts.len());
        }
        // each agent's posts since its last turn end, written in the turn this group ends or cuts, at the turn's line:
        // a post its turn's end made (`replied`) is its own turn's; and the starts it ran roster for since (`made_in`)
        for (k, l) in group.iter().enumerate() {
            if let Line::TurnEnded { agent, .. } | Line::TurnCut { agent, .. } = l {
                let line = i + k + 1;
                for p in self.unturned.remove(agent).unwrap_or_default() {
                    g.edge(&p, &turn_id(agent, line), EdgeKind::WrittenIn, line);
                }
                for s in self.made.remove(agent).unwrap_or_default() {
                    g.edge(&s, &turn_id(agent, line), EdgeKind::MadeIn, line);
                }
            }
        }
        // the ASKED ledger's close (paper-research N-1446 p1446): his post → each agent it asks (`asks`), and → the answer
        // that closed it, each it asks having one (`closed_by`), read off the ledger's one rule (`Machine::asked`) where a
        // line may change it: his post as it lands; one open where an agent's post lands on its ticket, or a reply-tool
        // answer from another ticket answers a post of it (`answers_also`), or a ticket opens to hand in for a post of it
        // (its `for`, `Machine::handed_in_for`: its opener wrote it); any, at a line that may widen whom it asks or take an
        // answer back (a `told`, a release, a post taken back, dropped, halted or moved)
        for (t, n) in his_new {
            self.his.entry(t).or_default().insert(n, Asked { open: true, ..Default::default() });
            to_read.insert(t);
        }
        for (k, l) in group.iter().enumerate() {
            match l {
                Line::AnswersAlso { also, .. } => to_read.extend(also.iter().filter_map(|a| crate::post_id(a)).map(|(u, _)| u)),
                Line::Open { t, opener, form, .. } => {
                    if let Some((u, _)) = m.handed_in_for(form) {
                        let (a, x) = (g.node(agent_id(opener), NodeKind::Agent, i + k + 1), g.node(m.id(*t), NodeKind::Ticket, i + k + 1));
                        g.edge(&a, &x, EdgeKind::Wrote, i + k + 1);
                        to_read.insert(u);
                    }
                    // the first head handed in for his post: where his goal opens (`first_handed`)
                    if let Some(p) = m.first_head_for(*t, form) {
                        self.first_handed.push(p);
                    }
                }
                _ => {}
            }
        }
        let widened: BTreeSet<u64> = group
            .iter()
            .filter(|l| matches!(l, Line::Told { .. } | Line::Released { .. } | Line::TakenBack { .. } | Line::Dropped { .. } | Line::Halted { .. } | Line::HaltEdit { .. } | Line::Moved { .. }))
            .flat_map(crate::machine::line_tickets)
            .collect();
        for t in to_read.iter().chain(&widened).collect::<BTreeSet<_>>() {
            let Some(asks) = self.his.get_mut(t) else { continue };
            let widens = widened.contains(t);
            for (n, was) in asks.iter_mut().filter(|(_, a)| a.open || widens) {
                let ledger = m.asked(*t, *n);
                let ask = post_id(m, *t, *n);
                for (a, _) in &ledger {
                    if was.who.insert(a.clone()) {
                        let to = g.node(agent_id(a), NodeKind::Agent, line);
                        g.edge(&ask, &to, EdgeKind::Asks, line);
                        was.closed = false;
                    }
                }
                let closed = !ledger.is_empty() && ledger.iter().all(|(_, answer)| answer.is_some());
                // the last of its answers is the one that closed it
                if let Some(by) = ledger.iter().filter_map(|(_, a)| a.as_deref()).max_by_key(|a| g.get(a).and_then(Node::line)).filter(|_| closed && !was.closed) {
                    // a post, or a hand-in's ticket
                    let to = g.node(by.to_string(), if crate::post_id(by).is_some() { NodeKind::Post } else { NodeKind::Ticket }, line);
                    g.edge(&ask, &to, EdgeKind::ClosedBy, line);
                }
                was.closed = closed;
                was.open = !ledger.is_empty() && !closed;
            }
        }
        self.graph.date_from(made.0, made.1, i + 1, group);
    }
}

/// The graph of the record LINES, replayed on M, each node and edge dated as it is made: with what the replay says
/// diverged, then where the graph and the record disagree ([`Builder::problems`]).
pub fn of(m: &mut Machine, lines: &[Line]) -> (Graph, Vec<String>) {
    let mut b = Builder::default();
    let mut problems = m.replay_with(lines, &mut |m, i, group| b.after(m, i, group));
    problems.append(&mut b.problems);
    (b.graph, problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_walk_goes_both_ways_along_each_edge_once_in_hop_order() {
        let mut g = Graph::default();
        let a = g.node("agent:x", NodeKind::Agent, 1);
        let p = g.node("N-1.p1", NodeKind::Post, 1);
        let t = g.node("N-1", NodeKind::Ticket, 1);
        let q = g.node("N-1.p2", NodeKind::Post, 2);
        g.edge(&a, &p, EdgeKind::Wrote, 1);
        g.edge(&p, &t, EdgeKind::PostedIn, 1);
        g.edge(&a, &q, EdgeKind::Wrote, 2);
        g.edge(&q, &t, EdgeKind::PostedIn, 2);
        assert_eq!(g.node("N-1", NodeKind::Ticket, 9), "N-1");
        assert_eq!((g.nodes.len(), g.get("N-1").and_then(Node::line)), (4, Some(1)));
        let w: Vec<(usize, &str, &str)> = g.walk("N-1.p1", 2).iter().map(|(h, e)| (*h, e.from.as_str(), e.to.as_str())).collect();
        assert_eq!(w, vec![(1, "agent:x", "N-1.p1"), (1, "N-1.p1", "N-1"), (2, "agent:x", "N-1.p2"), (2, "N-1.p2", "N-1")]);
        assert_eq!(g.walk("N-1.p1", 1).len(), 2);
        // along the edges it takes, and on from no node it stops at but the first: past the ticket nothing, past the
        // agent its other post; and only `wrote`
        let ends = |w: Vec<(usize, usize, usize)>| w.into_iter().map(|(h, i, _)| (h, g.edges[i].from.clone(), g.edges[i].to.clone())).collect::<Vec<_>>();
        let s = |a: &str, b: &str| (a.to_string(), b.to_string());
        let (wrote_p1, p1_in, wrote_p2, p2_in) = (s("agent:x", "N-1.p1"), s("N-1.p1", "N-1"), s("agent:x", "N-1.p2"), s("N-1.p2", "N-1"));
        let at = |h: usize, (a, b): &(String, String)| (h, a.clone(), b.clone());
        assert_eq!(ends(g.walk_by("N-1.p1", 2, &|_| true, &|n, _| n.kind == NodeKind::Ticket)), vec![at(1, &wrote_p1), at(1, &p1_in), at(2, &wrote_p2)]);
        assert_eq!(ends(g.walk_by("N-1.p1", 2, &|_| true, &|n, _| n.kind == NodeKind::Agent)), vec![at(1, &wrote_p1), at(1, &p1_in), at(2, &p2_in)]);
        assert_eq!(ends(g.walk_by("N-1", 2, &|_| true, &|n, _| n.kind == NodeKind::Ticket)).len(), 4, "the node it starts at is walked from");
        assert_eq!(ends(g.walk_by("N-1.p1", 2, &|i| g.edges[i].kind == EdgeKind::Wrote, &|_, _| false)), vec![at(1, &wrote_p1), at(2, &wrote_p2)]);
        // a stop by the edge it was reached by too: a stop that lets the ticket through by p1's `posted_in` walks on
        // from it, to p2's `posted_in`, as no stop does
        let through = |n: &Node, e: &Edge| n.kind == NodeKind::Ticket && e.kind != EdgeKind::PostedIn;
        assert_eq!(ends(g.walk_by("N-1.p1", 2, &|_| true, &through)), vec![at(1, &wrote_p1), at(1, &p1_in), at(2, &wrote_p2), at(2, &p2_in)]);
        // each edge's branch, the edge at p1 it was walked from: its `wrote` for the agent's other post, its `posted_in`
        // for the ticket's
        let branches: Vec<(usize, usize)> = g.walk_by("N-1.p1", 2, &|_| true, &through).into_iter().map(|(_, i, b)| (i, b)).collect();
        assert_eq!(branches, [(0, 0), (1, 1), (2, 0), (3, 1)]);
        let (n, e) = g.counts();
        assert_eq!((n[&NodeKind::Post], e[&EdgeKind::Wrote], e[&EdgeKind::PostedIn]), (2, 2, 2));
    }

    /// An item opened from his post comes from it, and an agent's start is its edge to the item; an item with no note,
    /// or one not an item, has none. Made up.
    #[test]
    fn an_item_comes_from_its_note_and_its_starters_started_it() {
        let m = Machine::load(Default::default());
        let l = |v: serde_json::Value| -> Line { serde_json::from_value(v).unwrap() };
        let at = "2026-11-21T08:51:00Z";
        let open = |t: u64, kind: &str, form: serde_json::Value| l(serde_json::json!({"ev": "open", "at": at, "t": t, "kind": kind, "opener": "paper-research", "form": form, "program": ""}));
        let mut b = Builder::default();
        b.after(&m, 0, &[open(5, "item", serde_json::json!({"note": "N-1104.p690", "title": "x"}))]);
        b.after(&m, 1, &[l(serde_json::json!({"ev": "started", "at": "2026-11-21T09:00:00Z", "t": 5, "by": "accounts-2"}))]);
        b.after(&m, 2, &[open(6, "item", serde_json::json!({"title": "no note"}))]);
        b.after(&m, 3, &[open(7, "thread", serde_json::json!({"note": "N-1104.p690"}))]);
        let e: Vec<(String, String, EdgeKind, Option<usize>)> = b.graph.edges.iter().map(|e| (e.from.clone(), e.to.clone(), e.kind, e.line())).collect();
        assert_eq!(e, vec![(m.id(5), "N-1104.p690".into(), EdgeKind::CameFrom, Some(1)), ("agent:accounts-2".into(), m.id(5), EdgeKind::Started, Some(2))]);
    }

    /// The lines `now` reads a turn, an item's end, a job and a start from (made up, in the record's shapes).
    #[test]
    fn a_turn_begins_at_its_prompt_and_ends_or_is_cut_an_item_closes_and_a_start_says_its_model() {
        use serde_json::json;
        let m = Machine::load(Default::default());
        let l = |v: serde_json::Value| -> Line { serde_json::from_value(v).unwrap() };
        let at = "2026-11-22T07:00:00Z";
        let delivered = |d: &str, began: bool| l(json!({"ev": "event", "at": "2026-11-22T07:00:02Z", "name": "delivered", "value": {"t": 1104, "to": "docket-edge5", "delivery": d, "at": "2026-11-22T07:00:01Z", "began": began, "place": {"file": "f", "offset": 0, "len": 1}}, "fired": []}));
        let mut b = Builder::default();
        b.after(&m, 0, &[delivered("N-1104.p767", true)]);
        b.after(&m, 1, &[delivered("N-1104.p768", false)]);
        b.after(&m, 2, &[l(json!({"ev": "turn_cut", "at": at, "agent": "docket-edge5"}))]);
        b.after(&m, 3, &[l(json!({"ev": "started", "at": at, "t": 5, "by": "accounts-2"}))]);
        b.after(&m, 4, &[l(json!({"ev": "closed", "at": at, "t": 5}))]);
        b.after(&m, 5, &[l(json!({"ev": "closed", "at": at, "t": 6}))]);
        b.after(&m, 6, &[l(json!({"ev": "act", "at": at, "agent": "docket-core", "is": "began", "text": "restart", "phase": "restart", "model": "claude-opus-5-5", "effort": "xhigh", "session": "97a6"}))]);
        let job = |phase: &str, at: &str, end: serde_json::Value| {
            let mut v = json!({"ev": "act", "at": at, "agent": "docket-core", "is": "job", "text": "job", "phase": phase, "id": "9-1795330000", "since": "2026-11-22T08:01:00+0100", "verb": "test", "packages": ["coordd"], "commit": "a2c839a6", "branch": "core-hook"});
            v.as_object_mut().unwrap().extend(end.as_object().unwrap().clone());
            l(v)
        };
        b.after(&m, 7, &[job("start", "2026-11-22T07:01:00Z", json!({}))]);
        b.after(&m, 8, &[job("end", "2026-11-22T07:02:00Z", json!({"rc": "0", "passed": "36", "failed": "0"}))]);
        let e: Vec<(String, String, EdgeKind, Option<usize>, Option<String>)> = b.graph.edges.iter().map(|e| (e.from.clone(), e.to.clone(), e.kind, e.line(), e.when.clone())).collect();
        let s = |x: &str| x.to_string();
        let w = |x: &str| Some(x.to_string());
        assert_eq!(
            e,
            vec![
                (s("N-1104.p767"), s("agent:docket-edge5"), EdgeKind::Prompted, Some(1), w("2026-11-22T07:00:01Z")),
                (s("turn:docket-edge5@3"), s("agent:docket-edge5"), EdgeKind::TurnOf, Some(3), w(at)),
                (s("agent:accounts-2"), m.id(5), EdgeKind::Started, Some(4), w(at)),
                (m.id(5), s("program:item"), EdgeKind::Closed, Some(5), w(at)),
                (s("start:docket-core@7"), s("agent:docket-core"), EdgeKind::StartOf, Some(7), w(at)),
                (s("start:docket-core@7"), s("session:97a6"), EdgeKind::CameFrom, Some(7), w(at)),
                (s("agent:docket-core"), s("job:9-1795330000"), EdgeKind::Ran, Some(8), w("2026-11-22T07:01:00Z")),
                (s("job:9-1795330000"), s("commit:a2c839a6"), EdgeKind::Ran, Some(8), w("2026-11-22T07:01:00Z")),
                (s("job:9-1795330000"), s("commit:a2c839a6"), EdgeKind::Ended, Some(9), w("2026-11-22T07:02:00Z")),
            ],
            "a delivery into a running turn begins none; a ticket nobody started has no end edge; a job's end line adds its end alone"
        );
        let n = |id: &str| b.graph.get(id).cloned().unwrap();
        assert_eq!((n("start:docket-core@7").what, n("job:9-1795330000").what, n("job:9-1795330000").ok), (s("restart: claude-opus-5-5, effort xhigh"), s("test coordd: rc 0, 36 passed, 0 failed"), Some(true)));
    }

    /// The lines an agent's folder reads its starts and its jobs' worktrees from (made up, in roster's and the runner's
    /// shapes): a start says its kind, model and effort, the folder it starts in and who started it, and is briefed by
    /// the post or ticket its first prompt names, a brief that names neither none; a job is in the worktree its line
    /// names, `in`, in the work layer; a start's brief is a delivery's (the file view, paper-research N-1446 p1441,
    /// p1530).
    #[test]
    fn a_start_says_its_kind_folder_and_starter_with_its_brief_and_a_job_is_in_its_worktree() {
        use serde_json::json;
        let m = Machine::load(Default::default());
        let l = |v: serde_json::Value| -> Line { serde_json::from_value(v).unwrap() };
        let at = "2026-11-22T07:00:00Z";
        let began = |more: serde_json::Value| {
            let mut v = json!({"ev": "act", "at": at, "agent": "agy-study", "is": "began", "text": "start"});
            v.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
            l(v)
        };
        let mut b = Builder::default();
        b.after(&m, 0, &[began(json!({"phase": "start", "kind": "claude", "model": "claude-opus-5-5", "effort": "high", "dir": "/d/agy-study", "session": "cac2", "brief": "N-1104.p797", "prompt_sha": "ab12", "starter": "paper-research"}))]);
        b.after(&m, 1, &[began(json!({"phase": "restart", "kind": "claude", "model": "claude-opus-5-5", "brief": "I-1478"}))]);
        b.after(&m, 2, &[began(json!({"phase": "fork", "brief": "the fixture"}))]);
        let job = json!({"ev": "act", "at": "2026-11-22T07:01:00Z", "agent": "agy-study", "is": "job", "text": "job", "phase": "start", "id": "9-1", "since": "2026-11-22T08:01:00+0100", "verb": "test", "packages": ["coordd"], "commit": "a2c839a6", "branch": "x", "wt": "/w/soft-swarm-x"});
        b.after(&m, 3, &[l(job)]);
        let g = &b.graph;
        let e: Vec<(String, String, EdgeKind, Option<usize>)> = g.edges.iter().map(|e| (e.from.clone(), e.to.clone(), e.kind, e.line())).collect();
        let s = |x: &str| x.to_string();
        assert_eq!(
            e,
            vec![
                (s("start:agy-study@1"), s("agent:agy-study"), EdgeKind::StartOf, Some(1)),
                (s("start:agy-study@1"), s("session:cac2"), EdgeKind::CameFrom, Some(1)),
                (s("start:agy-study@1"), s("N-1104.p797"), EdgeKind::Briefed, Some(1)),
                (s("start:agy-study@2"), s("agent:agy-study"), EdgeKind::StartOf, Some(2)),
                (s("start:agy-study@2"), s("I-1478"), EdgeKind::Briefed, Some(2)),
                (s("start:agy-study@3"), s("agent:agy-study"), EdgeKind::StartOf, Some(3)),
                (s("agent:agy-study"), s("job:9-1"), EdgeKind::Ran, Some(4)),
                (s("job:9-1"), s("commit:a2c839a6"), EdgeKind::Ran, Some(4)),
                (s("job:9-1"), s("worktree:/w/soft-swarm-x"), EdgeKind::In, Some(4)),
            ],
            "a brief that names no post or ticket has no edge"
        );
        let what = |id: &str| g.get(id).map(|n| n.what.clone());
        assert_eq!(
            [what("start:agy-study@1"), what("start:agy-study@2"), what("start:agy-study@3")],
            [Some(s("start: claude claude-opus-5-5, effort high, in /d/agy-study, by paper-research")), Some(s("restart: claude claude-opus-5-5")), Some(s("fork"))]
        );
        let kind = |id: &str| g.get(id).map(|n| n.kind);
        assert_eq!(
            [kind("N-1104.p797"), kind("I-1478"), kind("worktree:/w/soft-swarm-x")],
            [Some(NodeKind::Post), Some(NodeKind::Ticket), Some(NodeKind::Worktree)]
        );
        let layer = |from: &str, k: EdgeKind| g.edges.iter().find(|e| e.from == from && e.kind == k).map(|e| crate::graph_slice::Layer::of(e, g, &std::collections::HashMap::new()));
        assert_eq!(
            [layer("start:agy-study@1", EdgeKind::Briefed), layer("job:9-1", EdgeKind::In)],
            [Some(crate::graph_slice::Layer::Deliveries), Some(crate::graph_slice::Layer::Work)],
            "a start's brief beside the start, a job's worktree with the job"
        );
    }

    /// A start whose words name no brief holds them whole (roster's `prompt`, beside `prompt_sha`): its brief is the
    /// text node of its session's first user message, by the hash's first 12 hex, and the brief edge holds the words;
    /// one that names a brief, or holds no hash, makes no text node (paper-research N-1446 p3041). The folder's start
    /// row gives the words whole (the Initial prompt pane's "kind and brief").
    #[test]
    fn a_start_with_no_brief_holds_its_words_whole_as_its_brief() {
        use serde_json::json;
        let m = Machine::load(Default::default());
        let l = |v: serde_json::Value| -> Line { serde_json::from_value(v).unwrap() };
        let began = |more: serde_json::Value| {
            let mut v = json!({"ev": "act", "at": "2026-11-24T21:10:00Z", "agent": "try-1", "is": "began", "text": "start", "phase": "start", "kind": "claude", "starter": "owner"});
            v.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
            l(v)
        };
        let words = "You are a standing agent, started by the owner from yaaihh's New agent to try things out.\nThere is no task yet.";
        let mut b = Builder::default();
        b.after(&m, 0, &[began(json!({"prompt_sha": "0A1B2C3D4E5F67890000", "prompt": words}))]);
        b.after(&m, 1, &[began(json!({"prompt_sha": "ffffffffffff1111", "prompt": "Read N-1446 p3041.", "brief": "N-1446.p3041"}))]);
        b.after(&m, 2, &[began(json!({"prompt_sha": "zz", "prompt": "no hash"}))]);
        let g = &b.graph;
        let briefs: Vec<(String, String, String)> = g.edges.iter().filter(|e| e.kind == EdgeKind::Briefed).map(|e| (e.from.clone(), e.to.clone(), e.what.clone())).collect();
        let s = |x: &str| x.to_string();
        assert_eq!(briefs, vec![(s("start:try-1@1"), s("text:0a1b2c3d4e5f"), s(words)), (s("start:try-1@2"), s("N-1446.p3041"), String::new())]);
        let n = g.get("text:0a1b2c3d4e5f").unwrap();
        assert_eq!((n.kind, n.what.as_str()), (NodeKind::Text, format!("the words it was started with, {} bytes", words.len()).as_str()));
        assert!(g.get("text:ffffffffffff").is_none(), "a named brief: no text node");
        let folder = g.folder("try-1");
        let starts = folder.iter().find(|r| r.text.starts_with("kind and brief")).unwrap();
        let first = &starts.rows[0].rows[0];
        assert_eq!((first.text.as_str(), first.of.as_str(), first.words.as_str()), (format!("brief: the words it was started with, {} bytes", words.len()).as_str(), "text:0a1b2c3d4e5f", words));
        assert_eq!(starts.rows[1].rows[0].text, "brief: N-1446.p3041");
    }

    /// His system-prompt edit from a turn starts the agent on a copy of its session cut after that turn (roster's `from`
    /// and `cut`, 726dc806): the copy's session goes off line 1 at the turn it was cut after, the last whose message
    /// ends at or before the cut, a turn that said nothing too, a cut turn never. Line 2's own turns are counted on from
    /// there, so a second edit from line 2 makes line 3 from line 2's turn. An edit with Now, a fork's agent, and a cut
    /// of a session whose turns the record does not hold go off no line (path fix 6, accounts-lead N-1446 p2524). Each
    /// edit of his falls in its session's history (`Builder::restarted`): at the cut, with the line it began; with Now,
    /// where the session's last turn ended or its own part begins. A copy holds the edits before its cut of the session
    /// it was cut from; a fork's part 4 and a plain restart add none (point 3).
    #[test]
    fn an_edit_from_a_turn_starts_a_line_off_the_old_one_at_the_turn_it_was_cut_after() {
        use serde_json::json;
        let m = Machine::load(Default::default());
        let at = "2026-11-24T10:00:00Z";
        let l = |v: serde_json::Value| -> Line { serde_json::from_value(v).unwrap() };
        let began = |more: serde_json::Value| -> Line {
            let mut v = json!({"ev": "act", "at": at, "agent": "orch", "is": "began", "text": "start", "kind": "claude", "model": "claude-opus-5-5"});
            v.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
            l(v)
        };
        let ended = |agent: &str, session: &str, offset: u64, len: u64| l(json!({"ev": "turn_ended", "at": at, "agent": agent, "text": {"file": format!("/h/.claude/projects/-w/{session}.jsonl"), "offset": offset, "len": len}}));
        let mut b = Builder::default();
        let lines = [
            began(json!({"phase": "start", "session": "s1"})),
            ended("orch", "s1", 900, 100),
            l(json!({"ev": "turn_cut", "at": at, "agent": "orch"})),
            ended("orch", "s1", 1800, 200),
            // another agent's turn in a file of the same name is not orch's
            ended("lead", "s1", 2050, 10),
            // a turn that said nothing: its place is where its message would begin
            ended("orch", "s1", 2950, 0),
            // from turn 2 (line 4), cut after its end mark at 2100
            began(json!({"phase": "system", "session": "c0ff", "from": "s1", "cut": "2100", "system": "2", "replace": ""})),
            // line 2: the copy holds s1 up to 2100, and its own turns end after it
            ended("orch", "c0ff", 2400, 100),
            ended("orch", "c0ff", 2800, 100),
            // with Now: its session as it stands, no line
            began(json!({"phase": "system", "session": "c0ff", "system": "3", "replace": ""})),
            // from line 2's first turn of its own (line 8), cut at its end
            began(json!({"phase": "system", "session": "d00d", "from": "c0ff", "cut": "2500", "system": "4", "replace": ""})),
            // a fork's agent: its parent's session named, nothing cut
            began(json!({"phase": "fork", "from": "s1"})),
            // a session whose turns the record does not hold: a line of its own, off no turn
            began(json!({"phase": "system", "session": "e1", "from": "zz", "cut": "10", "system": "5", "replace": ""})),
            // with Now on a copy none of whose own turns has ended: where its own part begins
            began(json!({"phase": "system", "session": "e1", "system": "6", "replace": "", "at": "2026-11-24T10:42:00Z"})),
            // a fork's part 4 on a copy of line 3: line 3's edits before its cut, none of its own
            began(json!({"phase": "fork", "session": "f1", "from": "d00d", "cut": "2600"})),
            // a restart that is no edit of his
            began(json!({"phase": "restart", "session": "c0ff"})),
        ];
        for (i, x) in lines.into_iter().enumerate() {
            b.after(&m, i, &[x]);
        }
        let g = &b.graph;
        let s = |x: &str| x.to_string();
        let e: Vec<(String, String, Option<usize>, String)> =
            g.edges.iter().filter(|e| e.kind == EdgeKind::CameFrom).map(|e| (e.from.clone(), e.to.clone(), e.line(), e.what.clone())).collect();
        assert_eq!(
            e,
            vec![
                (s("start:orch@1"), s("session:s1"), Some(1), s("")),
                (s("start:orch@7"), s("session:c0ff"), Some(7), s("")),
                (s("session:c0ff"), s("turn:orch@4"), Some(7), s("branch 2, from turn 2 of branch 1, after byte 2100 of its session")),
                (s("start:orch@10"), s("session:c0ff"), Some(10), s("")),
                (s("start:orch@11"), s("session:d00d"), Some(11), s("")),
                (s("session:d00d"), s("turn:orch@8"), Some(11), s("branch 3, from turn 3 of branch 2, after byte 2500 of its session")),
                (s("start:orch@12"), s("session:s1"), Some(12), s("")),
                (s("start:orch@13"), s("session:e1"), Some(13), s("")),
                (s("start:orch@14"), s("session:e1"), Some(14), s("")),
                (s("start:orch@15"), s("session:f1"), Some(15), s("")),
                (s("start:orch@16"), s("session:c0ff"), Some(16), s("")),
            ],
            "line 1 stays as it was; Now, the fork's agent and a session the record holds no turns of go off no turn"
        );
        assert_eq!(g.get("turn:orch@4").map(|n| n.kind), Some(NodeKind::Turn), "the turn it goes off is the turn's own node");
        let layer = g.edges.iter().find(|e| e.from == "session:c0ff").map(|e| crate::graph_slice::Layer::of(e, g, &std::collections::HashMap::new()));
        assert_eq!(layer, Some(crate::graph_slice::Layer::Work), "beside the sessions");
        // where each edit falls in the agent's history, by session (path fix 6 point 3)
        let mark = |offset: u64, at: &str, system: &str, line: Option<(u32, &str, u32, usize)>| Restarted {
            offset,
            at: s(at),
            system: s(system),
            line: line.map(|(k, from, from_line, turns)| Branched { k, from: s(from), from_line, turns }),
        };
        let line2 = mark(2100, at, "2", Some((2, "s1", 1, 2)));
        assert!(b.restarted("orch", "s1").is_empty(), "line 1 never changes");
        assert_eq!(b.restarted("orch", "c0ff"), &[line2.clone(), mark(2900, at, "3", None)], "the cut, then Now where its last turn ends");
        let line3 = mark(2500, at, "4", Some((3, "c0ff", 2, 3)));
        assert_eq!(b.restarted("orch", "d00d"), &[line2.clone(), line3.clone()], "line 2's edits before its cut, not the later Now, then its own");
        assert_eq!(b.restarted("orch", "e1"), &[mark(10, at, "5", Some((4, "zz", 1, 0))), mark(10, "2026-11-24T10:42:00Z", "6", None)]);
        assert_eq!(b.restarted("orch", "f1"), &[line2, line3], "a fork's cut copy keeps them and adds none");
        assert!(b.restarted("lead", "s1").is_empty(), "by agent");
    }

    /// A start names the text it gave Claude Code beside or in place of its own (roster's `system_sha`, path-system):
    /// the standing text, his version appended, his version in place of Claude Code's own, one node by the hash's first
    /// 12 hex, its `what` saying which; a hash that is none names none. And the turn of its starter's it was made in,
    /// the starter's next turn end or cut by record line (`made_in`), as a post's `written_in`: none for a start of
    /// his, nor in a later turn (fix 4; docket-edge6 N-1446 p2496, p2529).
    #[test]
    fn a_start_names_the_text_it_was_started_with_and_the_turn_its_starter_made_it_in() {
        use serde_json::json;
        let m = Machine::load(Default::default());
        let at = "2026-11-24T10:00:00Z";
        let began = |more: serde_json::Value| -> Line {
            let mut v = json!({"ev": "act", "at": at, "agent": "orch", "is": "began", "text": "start", "kind": "claude", "model": "claude-opus-5-5"});
            v.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
            serde_json::from_value(v).unwrap()
        };
        let cut = |agent: &str| Line::TurnCut { at: at.into(), agent: agent.into() };
        let (standing, v2, v3) = ("AB12cd34ef56".repeat(5) + "abcd", "0123456789ab".repeat(5) + "cdef", "fedcba987654".repeat(5) + "3210");
        let mut b = Builder::default();
        b.after(&m, 0, &[began(json!({"phase": "start", "system_sha": standing, "starter": "lead"}))]);
        b.after(&m, 1, &[began(json!({"phase": "system", "system": "2", "system_sha": v2, "starter": "lead"}))]);
        b.after(&m, 2, &[cut("lead")]);
        b.after(&m, 3, &[began(json!({"phase": "system", "system": "3", "replace": "1", "system_sha": v3, "starter": "owner"}))]);
        b.after(&m, 4, &[began(json!({"phase": "restart", "system": "3", "replace": "1", "system_sha": "not a hash", "starter": "lead"}))]);
        b.after(&m, 5, &[cut("lead")]);
        b.after(&m, 6, &[cut("lead")]);
        let g = &b.graph;
        let s = |x: &str| x.to_string();
        let e: Vec<(String, String, EdgeKind, Option<usize>, String)> =
            g.edges.iter().filter(|e| matches!(e.kind, EdgeKind::StartedWith | EdgeKind::MadeIn)).map(|e| (e.from.clone(), e.to.clone(), e.kind, e.line(), e.what.clone())).collect();
        assert_eq!(
            e,
            vec![
                (s("start:orch@1"), s("text:ab12cd34ef56"), EdgeKind::StartedWith, Some(1), s("the standing text, appended")),
                (s("start:orch@2"), s("text:0123456789ab"), EdgeKind::StartedWith, Some(2), s("version 2, appended")),
                (s("start:orch@1"), s("turn:lead@3"), EdgeKind::MadeIn, Some(3), s("")),
                (s("start:orch@2"), s("turn:lead@3"), EdgeKind::MadeIn, Some(3), s("")),
                (s("start:orch@4"), s("text:fedcba987654"), EdgeKind::StartedWith, Some(4), s("version 3, in place of Claude Code's own")),
                (s("start:orch@5"), s("turn:lead@6"), EdgeKind::MadeIn, Some(6), s("")),
            ],
            "his start in no turn; a hash that is none names no text; lead's second cut made none"
        );
        assert!(b.made.is_empty(), "each start in the turn it was made in");
        let what = |id: &str| g.get(id).map(|n| (n.kind, n.what.clone()));
        assert_eq!(
            [what("text:ab12cd34ef56"), what("text:fedcba987654"), what("start:orch@2"), what("start:orch@4"), what("start:orch@5")],
            [
                Some((NodeKind::Text, s("orch's system prompt, the standing text, appended"))),
                Some((NodeKind::Text, s("orch's system prompt, version 3, in place of Claude Code's own"))),
                Some((NodeKind::Start, s("system: claude claude-opus-5-5, system prompt 2, by lead"))),
                Some((NodeKind::Start, s("system: claude claude-opus-5-5, system prompt 3 in place of Claude Code's own, by owner"))),
                Some((NodeKind::Start, s("restart: claude claude-opus-5-5, system prompt 3 in place of Claude Code's own, by lead"))),
            ]
        );
        let layer = |k: EdgeKind| g.edges.iter().find(|e| e.kind == k).map(|e| crate::graph_slice::Layer::of(e, g, &std::collections::HashMap::new()));
        assert_eq!(
            [layer(EdgeKind::MadeIn), layer(EdgeKind::StartedWith)],
            [Some(crate::graph_slice::Layer::Deliveries), Some(crate::graph_slice::Layer::Calls)],
            "where a start was made beside the start, what it was started with beside what its requests carried"
        );
    }

    /// `cost` on made-up edges: one giving brings his p0, p1 and p2 to docket-edge5, whose two requests in its step count
    /// for p1, which woke it, and p2, which asks it, not p0, which woke docket-core and came along (one with no
    /// response, so not carried); docket-core's request in a step handed docket-edge5's post naming p1 counts for p1
    /// apart (`named`), not for p2, which a post that came along names; a later giving of p2 with a post naming p1 makes
    /// docket-edge5's next request p2's only, and p1's named by, which makes it shared for neither (p1535); a request no
    /// session of an agent sent counts for nothing; and by a time before docket-core's request, p1 has no `named` yet.
    #[test]
    fn cost_counts_each_request_for_the_asks_its_steps_giving_brought_and_apart_for_those_its_posts_name() {
        fn put(g: &mut Graph, from: (&str, NodeKind), to: (&str, NodeKind), k: EdgeKind, n: usize, hms: &str) {
            let at = Some(format!("2026-11-22T{hms}"));
            let source = if n == 0 { Source::Session("s".into(), 1) } else { Source::Line(n) };
            let a = g.node_from(from.0, from.1, source.clone(), at.clone());
            let b = g.node_from(to.0, to.1, source.clone(), at.clone());
            g.edge_from(&a, &b, k, source, at);
        }
        let mut g = Graph::default();
        let (owner, e5, core) = (("agent:owner", NodeKind::Agent), ("agent:docket-edge5", NodeKind::Agent), ("agent:docket-core", NodeKind::Agent));
        let (post, request) = (|id| (id, NodeKind::Post), |id| (id, NodeKind::Request));
        let (s1, s2) = (("session:s1", NodeKind::Session), ("session:s2", NodeKind::Session));
        let tokens = |input, output| Tokens { input, output, cache_read: 100 * input, cache_write: 10 * input, reasoning: output / 2 };
        let req = |g: &mut Graph, id: &str, what: &str, t: Option<Tokens>| {
            let x = g.get_mut(id).unwrap();
            (x.what, x.tokens) = (what.to_string(), t);
        };
        // his p0 wakes docket-core and p1 docket-edge5, so the ledger says each asks it (p1561)
        put(&mut g, owner, post("N-1104.p0"), EdgeKind::Wrote, 1, "07:59:00Z");
        put(&mut g, post("N-1104.p0"), core, EdgeKind::Woke, 1, "07:59:00Z");
        put(&mut g, post("N-1104.p0"), core, EdgeKind::Asks, 1, "07:59:00Z");
        put(&mut g, owner, post("N-1104.p1"), EdgeKind::Wrote, 1, "08:00:00Z");
        put(&mut g, post("N-1104.p1"), e5, EdgeKind::Woke, 1, "08:00:00Z");
        put(&mut g, post("N-1104.p1"), e5, EdgeKind::Asks, 1, "08:00:00Z");
        put(&mut g, owner, post("N-1104.p2"), EdgeKind::Wrote, 2, "08:00:30Z");
        put(&mut g, post("N-1104.p2"), e5, EdgeKind::Asks, 2, "08:00:30Z");
        put(&mut g, s1, e5, EdgeKind::SessionOf, 3, "08:01:00Z");
        put(&mut g, s2, core, EdgeKind::SessionOf, 3, "08:01:00Z");
        // one giving, line 4, brings p0, p1 and p2; the step's `for` names p2, its upto; p0 woke docket-core, not it
        put(&mut g, post("N-1104.p0"), e5, EdgeKind::Handed, 4, "08:01:00Z");
        put(&mut g, post("N-1104.p1"), e5, EdgeKind::Handed, 4, "08:01:00Z");
        put(&mut g, post("N-1104.p2"), e5, EdgeKind::Handed, 4, "08:01:00Z");
        for (r, at) in [("request:r1", "08:01:00.400Z"), ("request:r2", "08:01:05.000Z")] {
            put(&mut g, s1, request(r), EdgeKind::Sent, 0, at);
            put(&mut g, request(r), post("N-1104.p2"), EdgeKind::For, 0, at);
        }
        req(&mut g, "request:r1", "claude-opus-5-5 xhigh", Some(tokens(3, 40)));
        req(&mut g, "request:r2", "claude-opus-5-5 xhigh", None);
        // docket-edge5's post naming p1, handed to docket-core at line 6, its step one request at medium
        put(&mut g, e5, post("N-1446.p10"), EdgeKind::Wrote, 5, "08:02:00Z");
        put(&mut g, post("N-1446.p10"), post("N-1104.p1"), EdgeKind::Named, 5, "08:02:00Z");
        put(&mut g, post("N-1446.p10"), core, EdgeKind::Handed, 6, "08:02:10Z");
        // brought with it, its p9 naming p2, which woke nobody: no step of docket-core's is for it
        put(&mut g, e5, post("N-1446.p9"), EdgeKind::Wrote, 5, "08:01:50Z");
        put(&mut g, post("N-1446.p9"), post("N-1104.p2"), EdgeKind::Named, 5, "08:01:50Z");
        put(&mut g, post("N-1446.p10"), core, EdgeKind::Woke, 6, "08:02:10Z");
        put(&mut g, post("N-1446.p9"), core, EdgeKind::Handed, 6, "08:02:10Z");
        put(&mut g, s2, request("request:r3"), EdgeKind::Sent, 0, "08:02:11Z");
        put(&mut g, request("request:r3"), post("N-1446.p10"), EdgeKind::For, 0, "08:02:11Z");
        req(&mut g, "request:r3", "claude-opus-5-5 medium", Some(tokens(5, 7)));
        // p2 given anew at line 7, with docket-core's p11 naming p1, which woke docket-edge5: docket-edge5's next request
        // is p2's alone, and p1's named by, not shared for it
        put(&mut g, post("N-1104.p2"), e5, EdgeKind::Handed, 7, "08:03:00Z");
        put(&mut g, core, post("N-1446.p11"), EdgeKind::Wrote, 7, "08:02:50Z");
        put(&mut g, post("N-1446.p11"), post("N-1104.p1"), EdgeKind::Named, 7, "08:02:50Z");
        put(&mut g, post("N-1446.p11"), e5, EdgeKind::Woke, 7, "08:03:00Z");
        put(&mut g, post("N-1446.p11"), e5, EdgeKind::Handed, 7, "08:03:00Z");
        put(&mut g, s1, request("request:r4"), EdgeKind::Sent, 0, "08:03:01Z");
        put(&mut g, request("request:r4"), post("N-1104.p2"), EdgeKind::For, 0, "08:03:01Z");
        req(&mut g, "request:r4", "claude-opus-5-5 xhigh", Some(tokens(2, 9)));
        // a request of no agent's session
        put(&mut g, request("request:r5"), post("N-1104.p1"), EdgeKind::For, 0, "08:03:30Z");
        req(&mut g, "request:r5", "claude-opus-5-5 xhigh", Some(tokens(1, 1)));
        let spent = |agent: &str, what: &str, requests, carried, t: Tokens| Spent { agent: agent.into(), what: what.into(), requests, carried, tokens: t };
        let mut both = tokens(3, 40);
        both += tokens(2, 9);
        assert_eq!(
            g.costs_at(None),
            vec![
                Cost { post: "N-1104.p0".into(), ..Default::default() },
                Cost {
                    post: "N-1104.p1".into(),
                    handed: vec![spent("agent:docket-edge5", "claude-opus-5-5 xhigh", 2, 1, tokens(3, 40))],
                    named: vec![spent("agent:docket-core", "claude-opus-5-5 medium", 1, 1, tokens(5, 7)), spent("agent:docket-edge5", "claude-opus-5-5 xhigh", 1, 1, tokens(2, 9))],
                    shared: 2,
                },
                Cost { post: "N-1104.p2".into(), handed: vec![spent("agent:docket-edge5", "claude-opus-5-5 xhigh", 3, 2, both)], named: vec![], shared: 2 },
            ],
            "r1 and r2 for p1 and p2, not p0, which came along; r2 not carried; r3 p1's apart, not p2's through p9; r4 p2's alone, p1's named by through p11, so shared for neither; r5 no agent's"
        );
        assert_eq!(g.cost("N-1104.p2"), g.costs_at(None).remove(2));
        assert_eq!((g.cost("N-1104.p9").handed, g.cost("N-1104.p9").named), (vec![], vec![]), "not his ask: nothing");
        let before = g.costs_at(Some("2026-11-22T08:02:10Z"));
        assert_eq!((before[1].named.clone(), before[2].handed[0].requests), (vec![], 2), "by 08:02:10, before r3 and r4");
    }

    /// `now`'s latest of an agent's edges to a node of a kind ([`latest_to`], read from the last on, a kind read only
    /// where it may change what was found) is [`latest`] of those of the kind: the last of them at the latest time, with
    /// ties, an edge with no time and ends of other kinds among them, out of time order (paper-research N-1446 p1922 3).
    #[test]
    fn the_latest_of_a_kind_read_from_the_last_is_latest_of_those_of_the_kind() {
        let mut g = Graph::default();
        let a = g.node("agent:x", NodeKind::Agent, 1);
        let at = |hm: &str| Some(format!("2026-11-23T{hm}:00Z"));
        let ends = [
            ("N-1.p1", NodeKind::Post, at("10:01")),
            ("c1", NodeKind::Commit, at("10:03")),
            ("N-1.p2", NodeKind::Post, at("10:03")),
            ("c2", NodeKind::Commit, at("10:03")),
            ("N-1.p3", NodeKind::Post, None),
            ("N-1", NodeKind::Ticket, at("10:09")),
            ("N-1.p4", NodeKind::Post, at("10:02")),
            ("c3", NodeKind::Commit, at("10:01")),
        ];
        for (i, (id, kind, when)) in ends.iter().enumerate() {
            g.node(*id, *kind, i + 1);
            g.edge_from(&a, id, EdgeKind::Wrote, Source::Line(i + 1), when.clone());
        }
        let v = Walked::new(&g, None);
        let wrote: Vec<&Edge> = g.edges.iter().collect();
        for kind in [NodeKind::Post, NodeKind::Commit, NodeKind::Ticket, NodeKind::Job] {
            let of_kind: Vec<&Edge> = wrote.iter().copied().filter(|e| v.kind_of(&e.to) == Some(kind)).collect();
            let (want, got) = (latest(of_kind), latest_to(&wrote, kind, &[], &v));
            assert_eq!(got.map(|e| e as *const Edge), want.map(|e| e as *const Edge), "{kind:?}: {got:?}, not {want:?}");
        }
        let to = |kind: NodeKind| latest_to(&wrote, kind, &[], &v).map(|e| e.to.as_str());
        assert_eq!((to(NodeKind::Post), to(NodeKind::Commit), to(NodeKind::Job)), (Some("N-1.p2"), Some("c2"), None), "of two at the latest time, the last");
        // the edges to its commits passed as of another kind (`commits_at`'s, for its last post): the same latest of
        // each other kind
        let commits: Vec<&Edge> = wrote.iter().copied().filter(|e| v.kind_of(&e.to) == Some(NodeKind::Commit)).collect();
        for kind in [NodeKind::Post, NodeKind::Ticket, NodeKind::Job] {
            let (with, without) = (latest_to(&wrote, kind, &commits, &v), latest_to(&wrote, kind, &[], &v));
            assert_eq!(with.map(|e| e as *const Edge), without.map(|e| e as *const Edge), "{kind:?} with the commits passed over");
        }
        // and an edge passed so is not read: with N-1.p2's, the latest post is N-1.p4, the next at 10:02
        let p2: Vec<&Edge> = wrote.iter().copied().filter(|e| e.to == "N-1.p2").collect();
        assert_eq!(latest_to(&wrote, NodeKind::Post, &p2, &v).map(|e| e.to.as_str()), Some("N-1.p4"), "an edge passed over is not read");
    }

    /// `now` on made-up edges at two moments: an agent mid-turn, one idle with a red last job, one in its first turn
    /// since its start with a job still running and an item open; his asks reached and never taken, taken with no
    /// commit, with a commit, and one in his window, which is not among them; and his posts the ASKED ledger says ask no
    /// one, his answer to a question among them, which are no asks (paper-research N-1446 p1561).
    #[test]
    fn now_gives_each_agents_turn_and_last_work_and_his_asks_by_their_last_point() {
        fn put(g: &mut Graph, from: (&str, NodeKind), to: (&str, NodeKind), k: EdgeKind, n: usize, hm: &str) {
            let at = (!hm.is_empty()).then(|| format!("2026-11-22T{hm}Z"));
            let a = g.node_from(from.0, from.1, Source::Line(n), at.clone());
            let b = g.node_from(to.0, to.1, Source::Line(n), at.clone());
            g.edge_from(&a, &b, k, Source::Line(n), at);
        }
        let mut g = Graph::default();
        let (owner, e5, core, pw) = (("agent:owner", NodeKind::Agent), ("agent:docket-edge5", NodeKind::Agent), ("agent:docket-core", NodeKind::Agent), ("agent:proto-window", NodeKind::Agent));
        let post = |id| (id, NodeKind::Post);
        let t = |hm: &str| format!("2026-11-22T{hm}Z");
        // docket-edge5: a turn ended 08:00; a post 08:06; a delivery began a turn 08:10; a post 08:15
        put(&mut g, ("turn:docket-edge5@1", NodeKind::Turn), e5, EdgeKind::TurnOf, 1, "08:00:00");
        put(&mut g, e5, post("N-1104.p702"), EdgeKind::Wrote, 2, "08:06:00");
        put(&mut g, post("N-1104.p767"), e5, EdgeKind::Prompted, 3, "08:10:00");
        put(&mut g, e5, post("N-1446.p1400"), EdgeKind::Wrote, 4, "08:15:00");
        // docket-core: a job from 08:05 on a2c839a6, red at 08:09; its commit 0f40917b at 08:08 on core-hook, later on
        // main; its turn's end at 08:12
        put(&mut g, core, ("job:1-1", NodeKind::Job), EdgeKind::Ran, 5, "08:05:00");
        put(&mut g, ("job:1-1", NodeKind::Job), ("commit:a2c839a6", NodeKind::Commit), EdgeKind::Ran, 5, "08:05:00");
        put(&mut g, ("job:1-1", NodeKind::Job), ("commit:a2c839a6", NodeKind::Commit), EdgeKind::Ended, 6, "08:09:00");
        g.get_mut("job:1-1").unwrap().ok = Some(false);
        put(&mut g, core, ("commit:0f40917b", NodeKind::Commit), EdgeKind::Wrote, 7, "08:08:00");
        g.get_mut("commit:0f40917b").unwrap().what = "the hook's fix".into();
        put(&mut g, ("commit:0f40917b", NodeKind::Commit), ("branch:soft-swarm/core-hook", NodeKind::Branch), EdgeKind::On, 7, "08:08:00");
        put(&mut g, ("commit:0f40917b", NodeKind::Commit), ("branch:soft-swarm/main", NodeKind::Branch), EdgeKind::On, 8, "08:11:00");
        put(&mut g, ("turn:docket-core@9", NodeKind::Turn), core, EdgeKind::TurnOf, 9, "08:12:00");
        // proto-window: started 07:40 in session abcd; an item started 07:50 and closed 08:00; another started 08:16; a
        // job from 08:18, still running
        put(&mut g, ("start:proto-window@10", NodeKind::Start), pw, EdgeKind::StartOf, 10, "07:40:00");
        g.get_mut("start:proto-window@10").unwrap().what = "restart: claude-opus-5-5, effort xhigh".into();
        put(&mut g, ("start:proto-window@10", NodeKind::Start), ("session:abcd", NodeKind::Session), EdgeKind::CameFrom, 10, "07:40:00");
        put(&mut g, pw, ("I-1500", NodeKind::Ticket), EdgeKind::Started, 11, "07:50:00");
        put(&mut g, ("I-1500", NodeKind::Ticket), ("program:item", NodeKind::Program), EdgeKind::Closed, 12, "08:00:00");
        put(&mut g, pw, ("I-1501", NodeKind::Ticket), EdgeKind::Started, 13, "08:16:00");
        put(&mut g, pw, ("job:2-2", NodeKind::Job), EdgeKind::Ran, 14, "08:18:00");
        put(&mut g, ("job:2-2", NodeKind::Job), ("commit:5d6c850c", NodeKind::Commit), EdgeKind::Ran, 14, "08:18:00");
        // his asks: p690, asking docket-edge5, in his window by 07:50; N-1479.p10 handed to docket-edge5 and never
        // taken; p701 taken by its answer p702, no commit; p703 named by 0f40917b
        put(&mut g, owner, post("N-1104.p690"), EdgeKind::Wrote, 15, "07:30:00");
        put(&mut g, post("N-1104.p690"), e5, EdgeKind::Asks, 15, "07:30:00");
        put(&mut g, ("commit:1111aaaa", NodeKind::Commit), post("N-1104.p690"), EdgeKind::Named, 16, "07:35:00");
        put(&mut g, ("commit:1111aaaa", NodeKind::Commit), ("build:x", NodeKind::Build), EdgeKind::BuiltInto, 17, "");
        put(&mut g, ("build:x", NodeKind::Build), owner, EdgeKind::Installed, 17, "07:50:00");
        put(&mut g, owner, post("N-1479.p10"), EdgeKind::Wrote, 18, "08:01:00");
        put(&mut g, post("N-1479.p10"), e5, EdgeKind::Handed, 19, "08:02:00");
        put(&mut g, owner, post("N-1104.p701"), EdgeKind::Wrote, 20, "08:03:00");
        put(&mut g, post("N-1104.p702"), post("N-1104.p701"), EdgeKind::Answered, 2, "08:06:00");
        put(&mut g, owner, post("N-1104.p703"), EdgeKind::Wrote, 21, "08:07:00");
        put(&mut g, ("commit:0f40917b", NodeKind::Commit), post("N-1104.p703"), EdgeKind::Named, 7, "08:08:00");
        // the ledger (p1446): p701 asks docket-edge5, closed by its p702; N-1479.p10 and p703 ask one each, never
        // closed; N-1480.p4 asks docket-edge5 and docket-core, taken by docket-edge5's p5, still open for docket-core;
        // its p6, taken by docket-core's p7, asks no one, so it is no ask (p1561)
        put(&mut g, post("N-1104.p701"), e5, EdgeKind::Asks, 20, "08:03:00");
        put(&mut g, post("N-1104.p701"), post("N-1104.p702"), EdgeKind::ClosedBy, 2, "08:06:00");
        put(&mut g, post("N-1479.p10"), e5, EdgeKind::Asks, 18, "08:01:00");
        put(&mut g, post("N-1104.p703"), core, EdgeKind::Asks, 21, "08:07:00");
        put(&mut g, owner, post("N-1480.p4"), EdgeKind::Wrote, 23, "08:04:00");
        put(&mut g, post("N-1480.p4"), e5, EdgeKind::Asks, 23, "08:04:00");
        put(&mut g, post("N-1480.p4"), core, EdgeKind::Asks, 23, "08:04:00");
        put(&mut g, e5, post("N-1480.p5"), EdgeKind::Wrote, 24, "08:06:30");
        put(&mut g, post("N-1480.p5"), post("N-1480.p4"), EdgeKind::Answered, 24, "08:06:30");
        put(&mut g, owner, post("N-1480.p6"), EdgeKind::Wrote, 25, "08:09:00");
        put(&mut g, core, post("N-1480.p7"), EdgeKind::Wrote, 26, "08:10:00");
        put(&mut g, post("N-1480.p7"), post("N-1480.p6"), EdgeKind::Answered, 26, "08:10:00");
        // p1561: his Q-22.p1, his answer to docket-core's question, handed to it with the question's close and never
        // answered, asks no one; his N-1434.p20 asks docket-edge5, which is handed it and never takes it
        put(&mut g, owner, post("Q-22.p1"), EdgeKind::Wrote, 46, "08:09:30");
        put(&mut g, post("Q-22.p1"), core, EdgeKind::Handed, 47, "08:09:40");
        put(&mut g, owner, post("N-1434.p20"), EdgeKind::Wrote, 48, "08:11:00");
        put(&mut g, post("N-1434.p20"), e5, EdgeKind::Asks, 48, "08:11:00");
        put(&mut g, post("N-1434.p20"), e5, EdgeKind::Handed, 49, "08:11:10");
        // point 4 (p1429): docket-edge5's session s5, its lines 40 to 42 its requests at 08:12, 08:14:30 and 08:21;
        // a2c839a6 made on core-hook; docket-edge5's job:3-3 on 0f40917b, also core-hook's, red at 08:13, after
        // job:1-1's red; its job:4-4 green at 08:14 on edge5-graph's 21fed2f4; job:5-5 green on core-hook at 08:25
        put(&mut g, ("session:s5", NodeKind::Session), e5, EdgeKind::SessionOf, 30, "08:00:00");
        for (r, n, hm) in [("request:q1", 40, "08:12:00"), ("request:q2", 41, "08:14:30"), ("request:q3", 42, "08:21:00")] {
            let (x, at) = (Source::Session("s5".into(), n), Some(t(hm)));
            let q = g.node_from(r, NodeKind::Request, x.clone(), at.clone());
            g.edge_from("session:s5", &q, EdgeKind::Sent, x, at);
        }
        put(&mut g, ("commit:a2c839a6", NodeKind::Commit), ("branch:soft-swarm/core-hook", NodeKind::Branch), EdgeKind::On, 31, "08:04:00");
        for (j, c, n, start, end, ok) in [("job:3-3", "commit:0f40917b", 32, "08:12:30", "08:13:00", false), ("job:4-4", "commit:21fed2f4", 34, "08:13:30", "08:14:00", true), ("job:5-5", "commit:0f40917b", 36, "08:24:00", "08:25:00", true)] {
            put(&mut g, e5, (j, NodeKind::Job), EdgeKind::Ran, n, start);
            put(&mut g, (j, NodeKind::Job), (c, NodeKind::Commit), EdgeKind::Ran, n, start);
            put(&mut g, (j, NodeKind::Job), (c, NodeKind::Commit), EdgeKind::Ended, n + 1, end);
            g.get_mut(j).unwrap().ok = Some(ok);
        }
        put(&mut g, ("commit:21fed2f4", NodeKind::Commit), ("branch:soft-swarm/edge5-graph", NodeKind::Branch), EdgeKind::On, 33, "08:11:00");
        // docket-edge5's job:6-6 at 07:30 on old-red's dead0001, red at 07:31: its last job until 08:12:30
        put(&mut g, ("commit:dead0001", NodeKind::Commit), ("branch:soft-swarm/old-red", NodeKind::Branch), EdgeKind::On, 38, "07:29:00");
        put(&mut g, e5, ("job:6-6", NodeKind::Job), EdgeKind::Ran, 38, "07:30:00");
        put(&mut g, ("job:6-6", NodeKind::Job), ("commit:dead0001", NodeKind::Commit), EdgeKind::Ran, 38, "07:30:00");
        put(&mut g, ("job:6-6", NodeKind::Job), ("commit:dead0001", NodeKind::Commit), EdgeKind::Ended, 39, "07:31:00");
        g.get_mut("job:6-6").unwrap().ok = Some(false);
        // owner_post's run gathering p701, its p1 by him, handed to the watcher: the run's, no ask
        put(&mut g, owner, post("O-1099511627777.p1"), EdgeKind::Wrote, 22, "08:03:15");
        put(&mut g, post("O-1099511627777.p1"), ("agent:watcher", NodeKind::Agent), EdgeKind::Handed, 22, "08:03:15");
        // p1481, p1510: coordd edits N-345 at 07:00, with no turn's end, delivery or start ever; another project's
        // tovrin-dev/DESIGN, a turn of which this record cut at 06:00, writes N-69.p2 at 07:05
        put(&mut g, ("agent:coordd", NodeKind::Agent), ("N-345", NodeKind::Ticket), EdgeKind::Edited, 43, "07:00:00");
        put(&mut g, ("turn:tovrin-dev/DESIGN@45", NodeKind::Turn), ("agent:tovrin-dev/DESIGN", NodeKind::Agent), EdgeKind::TurnOf, 45, "06:00:00");
        put(&mut g, ("agent:tovrin-dev/DESIGN", NodeKind::Agent), post("N-69.p2"), EdgeKind::Wrote, 44, "07:05:00");

        let now = g.now(&t("08:20:00"));
        let agent = |n: &Now, a: &str| n.agents.iter().find(|x| x.agent == a).cloned().unwrap();
        let r = |hm: &str, id: &str| Reached { at: t(hm), id: id.into(), via: None };
        let e5n = agent(&now, "docket-edge5");
        assert_eq!(e5n.turn, Some(TurnNow { since: t("08:10:00"), by: "N-1104.p767".into(), kind: EdgeKind::Prompted }), "since the delivery that began it, not its post before");
        assert_eq!((e5n.idle_since, e5n.last_post), (None, Some(r("08:15:00", "N-1446.p1400"))));
        let core_n = agent(&now, "docket-core");
        assert_eq!((core_n.turn, core_n.idle_since.as_deref()), (None, Some(t("08:12:00").as_str())));
        assert_eq!(core_n.last_commit, Some(Work { at: t("08:08:00"), id: "commit:0f40917b".into(), what: "the hook's fix".into(), on: Some("soft-swarm/core-hook".into()) }), "the branch it was made on, not the one it came to");
        assert_eq!(core_n.last_job, Some(JobNow { at: t("08:05:00"), id: "job:1-1".into(), what: String::new(), commit: Some("commit:a2c839a6".into()), ended: Some(t("08:09:00")), ok: Some(false) }));
        let pw_n = agent(&now, "proto-window");
        assert_eq!(pw_n.turn, Some(TurnNow { since: t("07:40:00"), by: "start:proto-window@10".into(), kind: EdgeKind::StartOf }), "no turn's end yet: since its first work, its start");
        assert_eq!(
            now.agents.iter().map(|a| a.agent.as_str()).collect::<BTreeSet<_>>(),
            BTreeSet::from(["docket-core", "docket-edge5", "proto-window"]),
            "coordd has work but no turn or start ever, tovrin-dev/DESIGN is another project's: no row; the watcher only an edge to it"
        );
        assert_eq!(pw_n.items, vec![r("08:16:00", "I-1501")], "the closed item is not its item now");
        assert_eq!(pw_n.last_job.as_ref().map(|j| (j.id.as_str(), j.ended.clone(), j.ok)), Some(("job:2-2", None, None)), "still running");
        assert_eq!(pw_n.began, Some(Work { at: t("07:40:00"), id: "start:proto-window@10".into(), what: "restart: claude-opus-5-5, effort xhigh".into(), on: Some("session:abcd".into()) }));
        let asks: Vec<(&str, Point, Option<&str>)> = now.asks.iter().map(|a| (a.times.post.as_str(), a.point, a.who.as_deref())).collect();
        assert_eq!(
            asks,
            vec![
                ("N-1479.p10", Point::Reached, Some("agent:docket-edge5")),
                ("N-1104.p701", Point::Closed, Some("agent:docket-edge5")),
                ("N-1104.p703", Point::FirstCommit, Some("agent:docket-core")),
                ("N-1480.p4", Point::Taken, Some("agent:docket-edge5")),
                ("N-1434.p20", Point::Reached, Some("agent:docket-edge5")),
            ],
            "p690 is in his window; O-1099511627777.p1 is a run's, and N-1480.p6 and Q-22.p1 ask no one: no ask"
        );
        let ids = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(
            (now.not_taken.clone(), now.no_commit.clone()),
            (ids(&["N-1479.p10", "N-1434.p20"]), ids(&["N-1104.p701", "N-1480.p4"])),
            "his answer Q-22.p1, handed and never answered, is no ask nobody took; N-1434.p20 asks, so it is one"
        );
        assert_eq!(
            (now.open.clone(), now.answered.clone()),
            (ids(&["N-1480.p4"]), ids(&["N-1104.p701"])),
            "N-1480.p4 open for docket-core, p701 closed in words; N-1479.p10 untaken, p703 has its commit"
        );
        let p701 = now.asks.iter().find(|a| a.times.post == "N-1104.p701").unwrap();
        assert_eq!((p701.times.asks.clone(), p701.times.closed.clone()), (ids(&["agent:docket-edge5"]), Some(Reached { at: t("08:06:00"), id: "agent:docket-edge5".into(), via: Some("N-1104.p702".into()) })));
        // point 4: docket-edge5 in a turn, its session's last line by then 41, at 08:14:30; proto-window in a turn with
        // no session line; core-hook red since job:1-1's 08:09, its last red job:3-3; edge5-graph green
        assert_eq!(e5n.session, Some(SessionNow { session: "session:s5".into(), line: 41, at: t("08:14:30") }), "not 08:21's line 42");
        assert_eq!((now.quiet.clone(), core_n.session), (ids(&["docket-edge5"]), None), "proto-window's session has no line; docket-core is in none");
        let red = |n: &Now| n.reds.iter().map(|r| (r.branch.clone(), r.job.id.clone(), r.job.ended.clone(), r.since.clone())).collect::<Vec<_>>();
        assert_eq!((red(&now), now.old_reds), (vec![("soft-swarm/core-hook".to_string(), "job:3-3".to_string(), Some(t("08:13:00")), t("08:09:00"))], 1), "edge5-graph's last is green; job:2-2 runs; old-red is no one's work by then");
        let branches = |n: &Now| n.reds.iter().map(|r| r.branch.clone()).collect::<Vec<_>>();
        let (at10, at30) = (g.now(&t("08:10:00")), g.now(&t("08:30:00")));
        assert_eq!(branches(&at10), vec!["soft-swarm/old-red".to_string(), "soft-swarm/core-hook".into()], "at 08:10 old-red is docket-edge5's last job's, red longest; core-hook job:1-1's");
        assert_eq!((red(&at30), at30.old_reds), (vec![], 1), "job:5-5's green at 08:25 ends core-hook's");
        // the course in its order, each point at its time where the graph reached it: p690's ends in his window
        let p690 = g.ask_times().into_iter().find(|t| t.post == "N-1104.p690").expect("his post");
        let points: Vec<(Point, bool)> = p690.points().iter().map(|(p, at, _)| (*p, at.is_some())).collect();
        assert_eq!((points.len(), points.first(), points.last()), (8, Some(&(Point::Asked, true)), Some(&(Point::InWindow, true))), "{points:?}");
        assert!(points.windows(2).all(|w| w[0].0 < w[1].0), "in its order: {points:?}");

        // at 08:07:30, as it was then: docket-core in a turn since its job's start, the job running, p703 just asked
        let then = g.now(&t("08:07:30"));
        let core_then = agent(&then, "docket-core");
        assert_eq!(core_then.turn, Some(TurnNow { since: t("08:05:00"), by: "job:1-1".into(), kind: EdgeKind::Ran }));
        assert_eq!((core_then.last_job.as_ref().map(|j| (j.ended.clone(), j.ok)), core_then.last_commit), (Some((None, None)), None));
        let e5_then = agent(&then, "docket-edge5");
        assert_eq!(e5_then.turn, Some(TurnNow { since: t("08:06:00"), by: "N-1104.p702".into(), kind: EdgeKind::Wrote }), "no delivery began it yet: since its post");
        assert_eq!((agent(&then, "proto-window").items, agent(&g.now(&t("07:55:00")), "proto-window").items), (vec![], vec![r("07:50:00", "I-1500")]), "I-1500 closed at 08:00, I-1501 not started yet; at 07:55 I-1500 open");
        let asks: Vec<(&str, Point, Option<&str>)> = then.asks.iter().map(|a| (a.times.post.as_str(), a.point, a.who.as_deref())).collect();
        assert_eq!(
            asks,
            vec![
                ("N-1479.p10", Point::Reached, Some("agent:docket-edge5")),
                ("N-1104.p701", Point::Closed, Some("agent:docket-edge5")),
                ("N-1104.p703", Point::Asked, Some("agent:owner")),
                ("N-1480.p4", Point::Taken, Some("agent:docket-edge5")),
            ]
        );
        assert_eq!(then.open, ids(&["N-1480.p4"]), "open for docket-core");
        // at 08:05:30, before docket-edge5's p702: p701 taken by nobody yet, its close to come
        let early = g.now(&t("08:05:30"));
        let p701 = early.asks.iter().find(|a| a.times.post == "N-1104.p701").unwrap();
        assert_eq!((p701.point, p701.times.closed.clone()), (Point::Asked, None));
    }
}
