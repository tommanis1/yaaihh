//! Lenses (the owner, N-1104 p599: "if you open vscode you just get the view at the root you navigate it and jsut
//! draga nd pin whatvervews yo uwant … the views the per agent etc are lenses on that sturucre jsut fitlers"; p610: "a2o
//! is different from a2a comms . i wnat ot osee them in different way / from different places"; the fundamentals §1–§2;
//! the split, proto-lead N-1446 p18 (3) on docket-edge6's p15 and p29).
//!
//! A lens is data: a filter over the project's one pile, the depth it is drawn at, and the node it is opened at. So it
//! is pinned in the sidebar, dropped into a pane (panes.rs) and kept in ui.json and a window's layout (tabs.rs) as any
//! page is: it is `Sel::Lens`. This file holds the type, the page a lens draws in a pane and how it keeps its node; the
//! filters other than Threads are drawn from the pages there are, never a copy, in docket-edge6's half:
//!
//! | filter | the list | drawn by |
//! |---|---|---|
//! | `Threads` | Threads' one list (`outline::outline`), the thread open beside it | `threads::page` |
//! | `WithYou { agent }` | the threads between him and the agent, its thread beside it | withyou.rs `view` |
//! | `A2o { agent }` | the agent's posts with him (streams.rs `with_owner`) | streams.rs `view`, `Which::You` |
//! | `A2a { agent }` | the agent's posts with the other agents (streams.rs `with_agents`) | streams.rs `view`, `Which::Agents` |
//! | `Find { words, titles, agent }` | a search's hits: over Threads' list, or within the agent's With you list | find.rs, the field holding its words |
//! | `Docket` | the project's programs, at a thread its own lit where it stands | kinds.rs `page_at` (docket-edge8) |
//! | `OpenToDone` | insight (3): every thread and item, opened → ended, his and the agents' apart, its numbers and the threads each counts | insight.rs `page` (S6) |
//! | `WorkTalk` | insight (1): every turn of the record, work or talk, by agent, model and hour, and the turns each counts | insight.rs `page` (S6) |
//! | `AgentsPerTopic` | insight (2): each thread with what is under it, and each item, with its agents and its time to done, and the lines each holds | insight.rs `page` (S6) |
//! | `SystemPrompt { agent }`, `InitialPrompt { agent }`, `AgentHome { agent }` | fix 4 (p2446): the agent's system prompt, its starts with their briefs, its texts and files; until docket-edge6's views (p2496), the part of its folder that holds each | sidebar.rs `folder_part` (docket-edge8) |
//!
//! With you and a2o are two lenses on him and the agent (his p610: "different way / from different places"): his
//! threads with it, each opened beside the list, against its posts with him in the order they came, whatever thread
//! they are on. a2a is its posts with the others: theirs, so no box (chat.rs `pane`).
//!
//! **Its node.** The lens on screen keeps no `at`: its thread is the page's (`tickets.shown` on Threads and a search
//! over it, `withyou.inside` on With you and a search within it), so going back and forth inside it moves history's
//! `Place.thread` and never makes a new place (docket-edge6's p15 point 2). `at` is written when the lens leaves the
//! focused pane (a pane beside, a window's layout, a pin: [`App::sel_kept`]) and read back when it returns
//! ([`App::show_sel`]). So two panes on one lens hold two threads (proto-lead N-1446 p72, p15 point 3). A With you
//! thread comes back beside its list only while it still stands there (withyou.rs `stands`, edge6-view's check of
//! ui.json's picks); else the list's first, a line in yaaihh.log. a2o and a2a are streams, at no thread.
//!
//! **In ui.json** a lens is written as the page it is drawn from (state.rs `kept_sel`), the lens beside it, so that a
//! window from before lenses (`release.sh back`) reads the file and opens those pages (consult-3, N-1396 p342).
//!
//! The depths (the fundamentals §2; consult-3's §2e: the raw pane is depth 5 only, depth 3 a filter of depth 4) are
//! named here so a lens keeps its depth. Below Structure a lens is at a node coordd reads (`N-1104`, `N-1104.p587`, a
//! run's step `O-1099511630339.s1`): Events and Record are its lines of the record (`Request::Record`), Wire a step's
//! call, what was sent and what came back: an agent's by its turn's requests (raw.rs `turn`), every other step (coordd's
//! own, `RunRound.own`; his; a box's; a policy's decide) by its lines of the record. Cloud is the Map (map/canvas.rs,
//! I-1478; S5's Cloud, handed to it: proto-window N-1479 p2).

use crate::{App, Proj};
use coord_types::RecordLine;
use rune::{El, ScrollSpec, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit, SegmentedControl};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// What a lens keeps of the pile (docket-edge6's cases, N-1446 p29).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Filter {
    /// Every thread of the project: the Threads page.
    Threads,
    /// The threads between him and AGENT: With you's list.
    WithYou { agent: String },
    /// AGENT's posts with him: a2o (his p610).
    A2o { agent: String },
    /// AGENT's posts with the other agents: a2a (his p610).
    A2a { agent: String },
    /// A search's hits: its WORDS, in the threads' titles only when TITLES; within AGENT's With you list when one is
    /// named (find.rs `agent_results`), else over Threads' list.
    Find {
        words: String,
        titles: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent: Option<String>,
    },
    /// The Docket (the Kinds page): the project's programs, and at a thread its own program, lit where it stands, with
    /// the runs of its handlers (0.3 W3). Shown, it is `Sel::Kinds`, the page's one value (proto-lead p120, as Threads
    /// is `Sel::Tickets`); a lens only while its pane is not focused, keeping its thread (N-1446 p113, p153).
    Docket,
    /// Insight (3), open → done (S6; his N-1002 "threads initial time -> completion", N-1020 "completion rate time it
    /// takes"): every line of the outline, when it opened and when it ended, the completion rate, the median time to
    /// end and the threads open for days (`docket_machine::insight`), each number opening the threads it counts. Its
    /// node is the number open (insight.rs `Number`), kept on screen too.
    OpenToDone,
    /// Insight (1), work and talk (S6; his p640 "track how many turs go into wokr", p641 "our unit is work x model"):
    /// every turn of the record, whether it worked or only talked as its line says, by agent, by model and by hour
    /// (`docket_machine::insight::work_talk`), each count opening the turns it counts. Its node is the count open
    /// (insight.rs `Count`), kept on screen too.
    WorkTalk,
    /// Insight (2), agents per topic (S6; his N-1104.p646 "agents per topic time to oc,mption"; proto-lead N-1446
    /// p653): each thread with what stands under it, and each item, with its agents and its time from open to done
    /// (`docket_machine::insight::per_topic`), over insight (3)'s rows. Its node is what it lists (insight.rs
    /// `Listing`: the threads' topics, the items', a topic's lines), kept on screen too.
    AgentsPerTopic,
    /// AGENT's system prompt (fix 4, accounts-lead N-1446 p2446; his N-1104 p767), opened by its press beside Raw log
    /// on the agent's page, in a pane beside it as Raw log's. Until docket-edge6's view takes it (p2496): each session's
    /// system prompt blocks of the agent's folder, the part of the folder view that holds it (p2452, read right p2455).
    SystemPrompt { agent: String },
    /// AGENT's initial prompt (fix 4; his p905): its starts with their briefs, the folder's "kind and brief", until
    /// docket-edge6's view.
    InitialPrompt { agent: String },
    /// AGENT's home (fix 4; his p851): the texts it loaded and the files it read, the folder's "loaded texts" and
    /// "read", until docket-edge6's view.
    AgentHome { agent: String },
}

/// The depth a lens draws (the fundamentals §2), top to bottom.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum Depth {
    /// Everything as a graph, his messages lit.
    Cloud,
    /// Threads, items and programs: the lists and the Docket picture.
    #[default]
    Structure,
    /// Turns, deliveries, decisions, tool calls: a filter of the record.
    Events,
    /// The record, line by line.
    Record,
    /// The raw calls, prompt and answer.
    Wire,
}

impl Depth {
    /// The five, top to bottom.
    pub const ALL: [Depth; 5] = [Depth::Cloud, Depth::Structure, Depth::Events, Depth::Record, Depth::Wire];

    /// Its word on a depth switch: a lens's below Structure ([`crumbs`]) and the Map's (map/canvas.rs `depth_switch`).
    /// Structure is the sidebar's word and his (p599 "the raw structure"; consult-3 N-1446 p1129).
    pub fn word(self) -> &'static str {
        match self {
            Depth::Cloud => "Cloud",
            Depth::Structure => "Structure",
            Depth::Events => "Events",
            Depth::Record => "Record",
            Depth::Wire => "Wire",
        }
    }
}

/// A filter over the pile, the depth it is drawn at, and the node it is opened at.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Lens {
    pub filter: Filter,
    /// How far down it is drawn (paper-research N-1446 p1385: "layers" are the switches by kind on the record's graph,
    /// how far down a lens goes its depth). Written `layer` in ui.json, as before, so a window from before reads it.
    #[serde(default, rename = "layer")]
    pub depth: Depth,
    /// The thread open in it (`N-1104`), none at its root; written only while it is not the page shown. An insight's
    /// is the number whose threads it lists (`days`), on screen too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
}

impl Filter {
    /// Its name in a row or a pane's head.
    pub fn name(&self) -> String {
        match self {
            Filter::Threads => "Threads".into(),
            Filter::WithYou { agent } => format!("With you · {agent}"),
            Filter::A2o { agent } => format!("{agent} ↔ you"),
            Filter::A2a { agent } => format!("{agent} ↔ agents"),
            Filter::Find { words, agent: None, .. } => format!("Find · {words}"),
            Filter::Find { words, agent: Some(a), .. } => format!("Find · {words} · with {a}"),
            // the page it is drawn as, as panes.rs `label` names it: its pane's head reads the same focused or not
            Filter::Docket => "Kinds".into(),
            Filter::OpenToDone => "Open → done".into(),
            Filter::WorkTalk => "Work and talk".into(),
            Filter::AgentsPerTopic => "Agents per topic".into(),
            Filter::SystemPrompt { agent } => format!("System prompt · {agent}"),
            Filter::InitialPrompt { agent } => format!("Initial prompt · {agent}"),
            Filter::AgentHome { agent } => format!("Home · {agent}"),
        }
    }

    /// A key for its elements, unique among a project's lenses.
    pub fn key(&self) -> String {
        match self {
            Filter::Threads => "threads".into(),
            Filter::WithYou { agent } => format!("with-{agent}"),
            Filter::A2o { agent } => format!("a2o-{agent}"),
            Filter::A2a { agent } => format!("a2a-{agent}"),
            Filter::Find { words, titles, agent } => format!("find-{}-{}-{words}", u8::from(*titles), agent.as_deref().unwrap_or("")),
            Filter::Docket => "docket".into(),
            Filter::OpenToDone => "open-to-done".into(),
            Filter::WorkTalk => "work-talk".into(),
            Filter::AgentsPerTopic => "agents-per-topic".into(),
            Filter::SystemPrompt { agent } => format!("system-prompt-{agent}"),
            Filter::InitialPrompt { agent } => format!("initial-prompt-{agent}"),
            Filter::AgentHome { agent } => format!("agent-home-{agent}"),
        }
    }

    /// The insight it is (S6): which rows it asks of coordd; none for a lens on the threads or an agent's page.
    pub fn insight(&self) -> Option<coord_types::Insight> {
        match self {
            Filter::OpenToDone | Filter::AgentsPerTopic => Some(coord_types::Insight::OpenToDone),
            Filter::WorkTalk => Some(coord_types::Insight::WorkTalk),
            Filter::Threads | Filter::WithYou { .. } | Filter::A2o { .. } | Filter::A2a { .. } | Filter::Find { .. } | Filter::Docket => None,
            Filter::SystemPrompt { .. } | Filter::InitialPrompt { .. } | Filter::AgentHome { .. } => None,
        }
    }

    /// The groups of its agent's folder a lens of fix 4 draws until docket-edge6's views take them (sidebar.rs
    /// `folder_part`), each a path of row heads from the folder's top, `*` any row; none for every other lens.
    pub fn folder_part(&self) -> &'static [&'static str] {
        match self {
            Filter::SystemPrompt { .. } => &["sessions|*|system prompt blocks"],
            Filter::InitialPrompt { .. } => &["kind and brief"],
            Filter::AgentHome { .. } => &["loaded texts", "read"],
            _ => &[],
        }
    }

    /// The insights it asks of coordd (S6): its own ([`Filter::insight`]) and, on Open → done, his asks, its third row
    /// (`Insight::Asks`, of a coordd from ASKS_PROTOCOL; paper-research N-1446 p1786).
    pub fn insights(&self) -> Vec<coord_types::Insight> {
        let mut v: Vec<coord_types::Insight> = self.insight().into_iter().collect();
        if *self == Filter::OpenToDone {
            v.push(coord_types::Insight::Asks);
        }
        v
    }
}

impl Lens {
    pub fn of(filter: Filter) -> Lens {
        Lens { filter, depth: Depth::Structure, at: None }
    }

    /// The Threads lens, open at thread ID.
    pub fn threads_at(id: &str) -> Lens {
        Lens { at: Some(id.to_string()), ..Lens::of(Filter::Threads) }
    }

    /// Its name: the filter's, and its depth below Structure ("Threads · Events").
    pub fn name(&self) -> String {
        match self.depth {
            Depth::Structure => self.filter.name(),
            // the Map is the whole pile, whatever it was opened from
            Depth::Cloud => "Map".into(),
            depth => format!("{} · {depth:?}", self.filter.name()),
        }
    }

    /// Whether it is drawn as the Threads page: its list (a search over it with the search's words in its field), and
    /// beside it the thread `tickets.shown`.
    pub fn shows_threads(&self) -> bool {
        matches!(self.filter, Filter::Threads | Filter::Find { agent: None, .. }) && self.depth == Depth::Structure
    }

    /// Whether it is drawn as the Docket: the Kinds page, at the thread it keeps.
    pub fn shows_docket(&self) -> bool {
        self.filter == Filter::Docket && self.depth == Depth::Structure
    }

    /// The agent whose With you it is drawn as (its list, a search within it with the search's words in its field, and
    /// beside it the thread `withyou.inside`), if it is.
    pub fn with_you(&self) -> Option<&str> {
        match &self.filter {
            Filter::WithYou { agent } | Filter::Find { agent: Some(agent), .. } if self.depth == Depth::Structure => Some(agent),
            _ => None,
        }
    }

    /// The agent whose page it is drawn from: With you's, a2o's or a2a's, a search's within With you.
    pub fn agent(&self) -> Option<&str> {
        match &self.filter {
            Filter::WithYou { agent } | Filter::A2o { agent } | Filter::A2a { agent } | Filter::Find { agent: Some(agent), .. } => Some(agent),
            Filter::SystemPrompt { agent } | Filter::InitialPrompt { agent } | Filter::AgentHome { agent } => Some(agent),
            Filter::Threads | Filter::Find { agent: None, .. } | Filter::Docket | Filter::OpenToDone | Filter::WorkTalk | Filter::AgentsPerTopic => None,
        }
    }

    /// The page it is on screen: the Threads lens at Structure is the Threads page itself, `Sel::Tickets`, so one page
    /// has one value in history, ui.json and a pin's light (proto-lead N-1446 p120); any other lens is itself, a search
    /// over Threads too, whose words are its own (docket-edge6, p124). The Docket lens at Structure is the Kinds page,
    /// `Sel::Kinds`, the same way (docket-edge8, p153).
    pub fn as_sel(&self) -> crate::Sel {
        match (&self.filter, self.depth) {
            (Filter::Threads, Depth::Structure) => crate::Sel::Tickets,
            (Filter::Docket, Depth::Structure) => crate::Sel::Kinds,
            _ => crate::Sel::Lens(self.clone()),
        }
    }
}

/// The lenses the bar's menu offers to pin from the page shown (zoom.rs `menu`; the lens shown itself is
/// `App::shown_pin`'s): on an agent's page its three, With you, its stream with him and its stream with the others
/// (his p610: a2o and a2a seen "from different places"); over a list of threads with words in the field, that search,
/// within the agent's With you when the list is its.
pub fn offers(s: &App) -> Vec<Lens> {
    let mut out = Vec::new();
    if let crate::Sel::Agent(a) = &s.sel {
        let agent = a.to_string();
        out.extend([Filter::WithYou { agent: agent.clone() }, Filter::A2o { agent: agent.clone() }, Filter::A2a { agent }].map(Lens::of));
    }
    let words = s.tickets.find.q.text.trim();
    if !words.is_empty() && crate::find::has_list(s) {
        let agent = match &s.sel {
            crate::Sel::Agent(a) => Some(a.clone()),
            crate::Sel::Lens(l) => l.with_you().map(str::to_string),
            _ => None,
        };
        let find = Lens::of(Filter::Find { words: words.to_string(), titles: s.tickets.find.titles, agent });
        if !matches!(&s.sel, crate::Sel::Lens(l) if l.filter == find.filter) {
            out.push(find);
        }
    }
    out
}

/// What a lens this head does not draw yet says in its pane.
fn not_drawn(l: &Lens, k: &Kit) -> El {
    col().padding(24.0).child(row().child(text(format!("{} · {:?}: not drawn yet", l.name(), l.depth)).font(k.ty.small).color(k.c.text_faint)))
}

/// Lens L's page in project P (the current one): Threads and a search over it are the Threads page, its list beside
/// the thread open in it; below Structure, the record's lines about its node; the others are drawn from the agent's
/// page ([`of_agent`]).
pub fn page(s: &App, p: &Proj, l: &Lens, k: &Kit) -> El {
    let pi = s.projects.iter().position(|q| q.id == p.id).unwrap_or(s.cur);
    match l.depth {
        Depth::Cloud => crate::map::canvas::page(s, k),
        _ if l.shows_threads() => crate::threads::page(s, p, k),
        _ if l.shows_docket() => crate::kinds::page(s, p, k),
        _ if l.filter.insight().is_some() => crate::insight::page(s, pi, l, k),
        Depth::Events | Depth::Record | Depth::Wire if l.filter == Filter::Threads => at_depth(s, pi, l, k),
        _ => of_agent(s, s.cur, l, true, k),
    }
}

/// Lens L of project PI drawn from its agent's page as chat.rs `pane` draws it, never a copy: With you and a search
/// within it by withyou.rs `view` (FOCUSED as it takes it: the field is the focused pane's), a2o and a2a by streams.rs
/// `view`, the agent's stream with him and with the others; fix 4's by the part of its folder each opens (sidebar.rs
/// `folder_part`). An agent no longer in the project says so.
fn of_agent(s: &App, pi: usize, l: &Lens, focused: bool, k: &Kit) -> El {
    let (Some(p), Some(name), Depth::Structure) = (s.projects.get(pi), l.agent(), l.depth) else { return not_drawn(l, k) };
    let Some(a) = p.live.agents.iter().find(|a| a.name == name) else {
        return col().padding(24.0).child(text(format!("{name} is not an agent of this box")).font(k.ty.body).color(k.c.text_muted));
    };
    let body = match &l.filter {
        Filter::A2o { .. } => crate::streams::view(s, pi, name, crate::streams::Which::You, k),
        Filter::A2a { .. } => crate::streams::view(s, pi, name, crate::streams::Which::Agents, k),
        f if !f.folder_part().is_empty() => crate::sidebar::folder_part(s, pi, name, f, k),
        _ => crate::withyou::view(s, pi, a, focused, k),
    };
    col().w_full().h_full().key(format!("lens-{pi}-{}", l.filter.key())).child(body)
}

/// Lens L of project PI in a pane that is not focused (panes.rs `area`): the thread it keeps, drawn whole as With you
/// draws the thread beside its list (withyou.rs `whole`); at its root, its list (With you's narrowed to its agent). A
/// stream, a2o or a2a, is drawn as on the focused page; below Structure, its lines as on the focused page. A press in
/// it focuses the pane first, so the press lands as on the focused page.
pub fn apart(s: &App, pi: usize, l: &Lens, k: &Kit) -> El {
    if l.shows_docket() {
        let Some(p) = s.projects.get(pi) else { return col() };
        return docket_apart(s, pi, p, l, k);
    }
    if l.filter.insight().is_some() {
        return crate::insight::page(s, pi, l, k);
    }
    if matches!(l.depth, Depth::Events | Depth::Record | Depth::Wire) && l.filter == Filter::Threads {
        return at_depth(s, pi, l, k);
    }
    if !l.shows_threads() && l.with_you().is_none() {
        return of_agent(s, pi, l, false, k);
    }
    let Some(p) = s.projects.get(pi) else { return col() };
    if let Some(v) = l.at.as_deref().and_then(|id| crate::find::held(s, p, id)).and_then(|n| crate::find::ticket(s, p, n)) {
        return crate::withyou::whole(s, p, v, k);
    }
    if let Some(a) = l.with_you() {
        return crate::threads::listing(s, p, Some(a), None, false, k);
    }
    let list = crate::outline::outline(s, pi, None, None, k).unwrap_or_else(|| col().child(text("No open threads.").font(k.ty.small).color(k.c.text_muted)));
    col().w_full().h_full().min_h(0.0).key(format!("lens-apart-{pi}-{}", l.filter.key())).scroll(ScrollSpec { stick_bottom: false }).px(12.0).py(10.0).child(list)
}

/// The record's lines asked of coordd (`Request::Record`) for the lenses at the Events and Record depths, by project
/// and node; what follows them; and the lines opened whole, by project and line number.
#[derive(Default)]
pub struct Records {
    pub got: HashMap<(usize, String), Result<Vec<RecordLine>, String>>,
    pub foot: HashMap<(usize, String), Foot>,
    pub whole: HashSet<(usize, u64)>,
}

/// What follows a node's lines when it is not their end: a page came back full, so more may follow (consult-3 N-1446
/// p174: 400 lines were a dead end); or why the next page did not come.
#[derive(Debug, Clone, PartialEq)]
pub enum Foot {
    More,
    Failed(String),
}

/// NODE's ticket (`N-1104`, a run's `O-1099511630339`) and its post or step: `N-1104.p587`, `O-1099511630339.s1`. The
/// ids coordd reads already (`Request::Label`). A node that names neither, as a question's delivery `Q-15.c`, is all
/// ticket.
pub(crate) fn parts(node: &str) -> (&str, Option<u32>, Option<u32>) {
    match node.rsplit_once('.') {
        Some((t, x)) if x.starts_with('p') => (t, x[1..].parse().ok(), None),
        Some((t, x)) if x.starts_with('s') => (t, None, x[1..].parse().ok()),
        _ => (node, None, None),
    }
}

/// What coordd is asked for NODE's lines: its ticket's record, from its post when it names one, after line AFTER for
/// the next page.
pub fn request(node: &str, after: Option<u64>) -> coord_types::Request {
    let (t, at, _) = parts(node);
    coord_types::Request::Record { t: t.to_string(), at, after, cap: 0 }
}

/// The node a zoom from thread ID (number N) starts at: the post sought or found on its page, else its last post, so
/// a long thread opens on what ran last and not on its first day (consult-3 N-1446 p174: N-1104 from 19 Nov); the
/// thread itself when it has no post.
pub fn node_of(s: &App, n: u64, id: &str) -> String {
    let last = s.proj().and_then(|p| crate::find::ticket(s, p, n)).and_then(|v| v.posts.last()).map(|q| q.n);
    match (s.tickets.find.found, last) {
        (Some((t, post)), _) if t == n => format!("{id}.p{post}"),
        (_, Some(post)) => format!("{id}.p{post}"),
        _ => id.to_string(),
    }
}

/// The page shown zoomed to DEPTH at NODE, one press per depth (proto-window N-1446 p10): at Structure the Threads lens,
/// its thread open at the post NODE names; below it the record's lines about NODE, asked of coordd once; at Wire NODE
/// is a run's step (`O-1099511630339.s1`), what its call was sent and what came back (proto-lead p158: a step's "› on
/// the wire" in the Docket is this). A depth is a place of its own, so Back comes up again as the crumb does
/// (history.rs). A thread the snapshot does not hold is fetched first, as a find's is; one coordd does not give is
/// said in the notice and nothing moves.
pub fn zoom(s: &mut App, depth: Depth, node: &str) {
    let (t, post, _) = parts(node);
    if depth == Depth::Structure {
        // most of the Map's lines closed more than a day ago, so the snapshot lacks them: its "Open its page" fetches
        // (proto-window N-1479 p36)
        let n = match crate::find::number(s, t) {
            Ok(n) => n,
            Err(e) => {
                s.notice = Some(e);
                return;
            }
        };
        s.show_sel(crate::Sel::Lens(Lens::of(Filter::Threads)));
        crate::find::open_at(s, n, node, post);
        return;
    }
    // a step's run is a hidden ticket the snapshot does not hold: fetched as a run pressed in the Docket is (find.rs
    // `number`), so its round says whom it went to; without it the step is read from the record
    if depth == Depth::Wire
        && let Err(e) = crate::find::number(s, t)
    {
        crate::log::line(&format!("lens: {node} on the wire: {e}"));
    }
    s.show_sel(crate::Sel::Lens(Lens { filter: Filter::Threads, depth, at: Some(node.to_string()) }));
}

/// [`zoom`] from a press on NODE's lines of project PI, wherever they are drawn: PI is made current first, so a call
/// on another island of the Map opens its own project's record, not the current one's (proto-window N-1479 p41), as
/// "Open its page" opens a node's own project's page (map/canvas.rs). In a pane it is already current: a press inside
/// a pane focuses it first (panes.rs `area`). Pressed in the lines the Map draws in a node's card, it is the Map's, in
/// place (map/canvas.rs `pressed_down`: an event's line to its Record, a step's "Wire ›" to its call).
fn zoom_in(s: &mut App, pi: usize, depth: Depth, node: &str) {
    if crate::map::canvas::pressed_down(s, pi, depth, node) {
        return;
    }
    s.cur = pi;
    zoom(s, depth, node);
}

/// NODE's lines asked of project PI's coordd, unless held or on their way; a coordd before RECORD_PROTOCOL is not
/// asked (its page says so).
fn ask(s: &mut App, pi: usize, node: &str) {
    let Some(p) = s.projects.get(pi) else { return };
    if on_way(p, node) || p.protocol < coord_types::RECORD_PROTOCOL || s.records.got.contains_key(&(pi, node.to_string())) {
        return;
    }
    s.request_in(pi, request(node, None), crate::Pending::Record { node: node.to_string(), after: None });
}

/// What NODE's lines at DEPTH in project PI are read from, asked once, as for a lens shown ([`App::show_sel`], the
/// current project) and for a call the Map draws in place on any project's island (map/deep.rs). On the wire an
/// agent's round is its turn's requests, read from the proxy's log (raw.rs `open_turn`); every other node is its lines
/// of the record ([`ask`]).
pub(crate) fn read(s: &mut App, pi: usize, depth: Depth, node: &str) {
    if let Some((to, since, until)) = turn_of(s, pi, depth, node) {
        return crate::raw::open_turn(s, pi, &to, node, since, until);
    }
    ask(s, pi, node)
}

/// The agent's turn NODE's lines at DEPTH in project PI are read from ([`read`]): on the wire an agent's round, its
/// agent and span ([`span`]), by the giving its answer came after (passed up, it is the new agent's); None for every
/// other node, whose lines are of the record.
pub(crate) fn turn_of(s: &App, pi: usize, depth: Depth, node: &str) -> Option<(String, u64, u64)> {
    s.projects.get(pi).and_then(|p| round(s, p, node)).filter(|_| depth == Depth::Wire).and_then(span)
}

/// Whether any of NODE's lines at DEPTH in project PI are held to draw ([`lines`]): on the wire an agent's round's
/// requests, else its lines of the record (or why they did not come).
pub(crate) fn held(s: &App, pi: usize, depth: Depth, node: &str) -> bool {
    match turn_of(s, pi, depth, node) {
        Some((to, _, _)) => crate::raw::turn_held(s, pi, &to, node),
        None => s.records.got.contains_key(&(pi, node.to_string())),
    }
}

/// Whether a page of NODE's lines is on its way from P's coordd.
fn on_way(p: &Proj, node: &str) -> bool {
    p.pending.values().any(|w| matches!(w, crate::Pending::Record { node: x, .. } if x == node))
}

/// The page of NODE's lines after the last one held, asked of project PI's coordd (the foot's press).
fn more(s: &mut App, pi: usize, node: &str) {
    let Some(last) = s.records.got.get(&(pi, node.to_string())).and_then(|g| g.as_ref().ok()).and_then(|l| l.last()).map(|l| l.line) else { return };
    if s.projects.get(pi).is_none_or(|p| on_way(p, node)) {
        return;
    }
    s.records.foot.insert((pi, node.to_string()), Foot::More);
    s.request_in(pi, request(node, Some(last)), crate::Pending::Record { node: node.to_string(), after: Some(last) });
}

/// coordd's answer for NODE of project PI: its lines, or why there are none. A page after line AFTER goes under the
/// lines held; a page that came back full says more may follow.
pub(crate) fn came(s: &mut App, pi: usize, node: String, after: Option<u64>, ok: bool, out: String) {
    let got: Result<Vec<RecordLine>, String> = if ok { out.lines().map(|l| serde_json::from_str(l).map_err(|e| format!("record failed: {e}"))).collect() } else { Err(out) };
    let key = (pi, node);
    let held = s.records.got.get_mut(&key).and_then(|g| g.as_mut().ok()).filter(|l| after.is_some() && l.last().map(|x| x.line) == after);
    match (got, held) {
        (Ok(page), Some(lines)) => {
            let full = page.len() >= coord_types::RECORD_CAP as usize;
            lines.extend(page);
            s.records.foot.remove(&key);
            if full {
                s.records.foot.insert(key, Foot::More);
            }
        }
        (Err(e), Some(_)) => {
            s.records.foot.insert(key, Foot::Failed(e));
        }
        (got, _) => {
            s.records.foot.remove(&key);
            if got.as_ref().is_ok_and(|p| p.len() >= coord_types::RECORD_CAP as usize) {
                s.records.foot.insert(key.clone(), Foot::More);
            }
            s.records.got.insert(key, got);
        }
    }
}

/// Lens L of project PI at the Events or Record depth: the crumbs back up and the depth's switch, then its node's lines
/// of the record as written (Record) or what they did, one line each (Events, a filter of them: consult-3's §2e).
fn at_depth(s: &App, pi: usize, l: &Lens, k: &Kit) -> El {
    let Some(p) = s.projects.get(pi) else { return col() };
    let node = l.at.clone().unwrap_or_default();
    let body = lines_commented(s, pi, l.depth, &node, k);
    col().w_full().h_full().min_h(0.0).key(format!("lens-layer-{pi}-{node}-{:?}", l.depth)).child(crumbs(s, p, pi, l, &node, k)).child(col().w_full().grow(1.0).min_h(0.0).key(format!("lens-lines-{pi}-{node}-{:?}", l.depth)).scroll(ScrollSpec { stick_bottom: false }).px(16.0).py(8.0).child(body))
}

/// NODE's lines at DEPTH in project PI: the lines of a lens below Structure under its crumbs ([`at_depth`]), and a call
/// on the Map drawn in place once it is near enough (map/deep.rs, p715 level 7). They are asked by [`read`]; until they
/// come, it says they are being read. Drawn by themselves, as on the Map, nothing in them is commented on: no passage
/// is lit and no box's room left, so a comment written on the same lines in a lens's pane stands in that pane alone,
/// its words kept while he is on the Map (I-1476).
pub(crate) fn lines(s: &App, pi: usize, depth: Depth, node: &str, k: &Kit) -> El {
    lines_as(s, pi, depth, node, None, k)
}

/// NODE's lines at DEPTH in project PI as he reads them: in a lens's pane ([`at_depth`]), and in a node's card on the
/// Map drawn at Events or Record (map/canvas.rs `depth_lines`; proto-window N-1446 p1130). Lit and commented on as
/// [`comment_source`] makes it, as a thread's are, the box under the words selected (I-1476), the comment going to the
/// thread [`comment_to`] gives; with none, no Comment.
pub(crate) fn lines_commented(s: &App, pi: usize, depth: Depth, node: &str, k: &Kit) -> El {
    let Some(p) = s.projects.get(pi) else { return col() };
    let body = lines_as(s, pi, depth, node, Some(&comment_source(node, depth)), k);
    if comment_to(s, p, node).is_none() {
        return body;
    }
    let node = node.to_string();
    crate::ask::commentable_apart(body, move |s: &mut App, sel| comment(s, pi, &node, depth, sel))
}

/// [`lines`] as a lens's pane draws them ([`lines_commented`]), SOURCE the comment on them is made as
/// ([`comment_source`]): its passage lit where it was selected, the box's room after it (ask.rs `lit`, `lit_room`,
/// `lit_words`).
fn lines_as(s: &App, pi: usize, depth: Depth, node: &str, source: Option<&str>, k: &Kit) -> El {
    let Some(p) = s.projects.get(pi) else { return col() };
    let key = (pi, node.to_string());
    let faint = |w: String| text(w).font(k.ty.small).color(k.c.text_muted).wrap();
    let mut body = col().w_full().gap(2.0);
    // on the wire, an agent's round is its turn's requests; every other round is its lines of the record (`span`)
    let turn = round(s, p, node).filter(|r| depth == Depth::Wire && span(r).is_some());
    body = match (turn, s.records.got.get(&key)) {
        (Some(r), _) => body.child(agents_wire(s, pi, r, node, k)),
        (_, None) if p.protocol < coord_types::RECORD_PROTOCOL => body.child(faint("Needs a newer engine.".into())),
        (_, None) => body.child(faint("Reading the record…".into())),
        (_, Some(Err(e))) => body.child(text(e.clone()).font(k.ty.small).color(k.c.danger.fg).wrap()),
        (_, Some(Ok(lines))) if depth == Depth::Record => body.child(faint(said(lines, "lines of the record"))).children(lines.iter().map(|x| record_row(s, pi, node, source, x, k))),
        (_, Some(Ok(lines))) if depth == Depth::Wire => body.children(coordds_wire(s, lines, pi, node, source, k)),
        (_, Some(Ok(lines))) => {
            let events: Vec<(&RecordLine, Value, String)> = lines.iter().filter_map(|x| {
                let v: Value = serde_json::from_str(&x.text).ok()?;
                let w = event_words(&v, parts(node).0)?;
                Some((x, v, w))
            }).collect();
            body.child(faint(format!("{} of {}", events.len(), said(lines, "lines of the record")))).children(events.into_iter().map(|(x, v, w)| event_row(s, x, &v, w, (pi, node, source), wire_at(s, p, node, &v), k)))
        }
    };
    body.children(foot(s, p, pi, node, k))
}

/// Where a comment on lines at NODE of project P goes: the thread its crumbs go back up to ([`up_to`]: a run's step's,
/// the thread it ran for; else NODE's own), when that is a thread. A lens below Structure has no thread page beside its
/// lines (history.rs `thread_shown`), so none is taken from a page (consult-3 N-1477 p64, proto-window p65): with no
/// thread to go up to, its lines offer no Comment. The thread is one the snapshot holds: a run's step of one only
/// fetched (the Map's lines closed more than a day ago) has its crumbs up to it, and offers no Comment, as before.
fn comment_to(s: &App, p: &Proj, node: &str) -> Option<String> {
    let thread = up_to(s, p, node).0;
    p.live.tickets.values().find(|v| v.id == thread && v.kind == "thread").map(crate::threads::target)
}

/// Comment on SEL, selected in DEPTH's lines at NODE in project PI (its menu's Comment): to NODE's thread
/// ([`comment_to`]), named by the depth and the node, its box under the words selected (ask.rs `comment_apart`).
pub(crate) fn comment(s: &mut App, pi: usize, node: &str, depth: Depth, sel: &rune::TextSelection) {
    let Some(target) = s.projects.get(pi).and_then(|p| comment_to(s, p, node)) else { return };
    let (source, label) = (comment_source(node, depth), format!("{} of {node}", depth_words(depth)));
    crate::ask::comment_apart(s, pi, &target, &source, &label, None, sel);
}

/// The source a comment on DEPTH's lines at NODE is made as, apart from the page's: its passage is lit in that pane only.
pub(crate) fn comment_source(node: &str, depth: Depth) -> String {
    format!("lens:{node}:{depth:?}")
}

/// What a comment on DEPTH's lines names them as, in its quote's "(from …)".
fn depth_words(depth: Depth) -> &'static str {
    match depth {
        Depth::Wire => "the wire",
        Depth::Record => "the record",
        _ => "the events",
    }
}

/// Under NODE's lines when they are not their end: the press for the next page ("Lines after 102803"), its reading, or
/// why it did not come with a press to ask again.
fn foot(s: &App, p: &Proj, pi: usize, node: &str, k: &Kit) -> Option<El> {
    let f = s.records.foot.get(&(pi, node.to_string()))?;
    let last = s.records.got.get(&(pi, node.to_string()))?.as_ref().ok()?.last()?.line;
    let r = row().w_full().pt(8.0).gap(8.0).center_y();
    if on_way(p, node) {
        return Some(r.child(text(format!("Reading the lines after {last}…")).font(k.ty.small).color(k.c.text_muted)));
    }
    let node = node.to_string();
    let (said, label) = match f {
        Foot::More => (None, format!("Lines after {last}")),
        Foot::Failed(e) => (Some(text(e.clone()).font(k.ty.small).color(k.c.danger.fg).wrap().shrink()), "Ask again".to_string()),
    };
    let press = Button::new(label).variant(ButtonVariant::Ghost).help("The record's next lines about it").on_press(move |s: &mut App| more(s, pi, &node)).build(k).key("lens-more");
    Some(r.children(said).child(press))
}

/// "N lines of the record, 101208–101265".
fn said(lines: &[RecordLine], what: &str) -> String {
    match (lines.first(), lines.last()) {
        (Some(a), Some(b)) => format!("{} {what}, {}–{}", lines.len(), a.line, b.line),
        _ => format!("No {what} here."),
    }
}

/// The thread NODE of project P goes back up to, and its post: a run's step's, the thread and the post it ran for
/// (`TicketView.ran`); else NODE's own ticket and post. Its crumbs lead up to it ([`crumbs`]); a comment on its lines
/// goes to it ([`comment_to`]). The thread is looked for among those a fetch holds too (`find::views`): the Map's lines
/// closed more than a day ago are not in the snapshot (proto-reader N-1479 p82).
fn up_to<'a>(s: &'a App, p: &'a Proj, node: &'a str) -> (&'a str, Option<u32>) {
    let (t, post, step) = parts(node);
    let home = step.and_then(|_| crate::find::views(s, p).find_map(|v| v.ran.iter().find(|r| r.id == t).map(|r| (v.id.as_str(), r.post))));
    home.unwrap_or((t, post))
}

/// The way back up from NODE (its thread's label and id, its post) and the depth's switch: Events · Record. The thread,
/// held live or fetched, is named by its label.
fn crumbs(s: &App, p: &Proj, pi: usize, l: &Lens, node: &str, k: &Kit) -> El {
    let (t, _, step) = parts(node);
    let (thread, post) = up_to(s, p, node);
    let label = crate::find::views(s, p).find(|v| v.id == thread).map(|v| crate::outline::label(p, v)).unwrap_or_else(|| thread.to_string());
    let up = post.map_or_else(|| thread.to_string(), |n| format!("{thread}.p{n}"));
    let (pick, again) = (node.to_string(), node.to_string());
    let back = Button::new(format!("‹ {}", crate::refs::cut(&label, 48))).variant(ButtonVariant::Ghost).help("Up to the thread").on_press(move |s: &mut App| zoom(s, Depth::Structure, &up)).build(k).key("lens-up");
    let mut r = row().w_full().px(12.0).pt(8.0).pb(6.0).gap(8.0).center_y().border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_weak).child(back).child(text(thread.to_string()).font(k.ty.small).color(k.c.text_faint));
    if let Some(n) = post {
        r = r.child(text(format!("› p{n}")).font(k.ty.small).color(k.c.text_muted));
    }
    if let Some(n) = step {
        r = r.child(text(format!("› {t} · step {n}")).font(k.ty.small).color(k.c.text_muted));
    }
    // Wire is a step's: offered at one
    let depths: &[Depth] = if step.is_some() { &[Depth::Events, Depth::Record, Depth::Wire] } else { &[Depth::Events, Depth::Record] };
    let names: Vec<&str> = depths.iter().map(|x| x.word()).collect();
    let at = depths.iter().position(|x| *x == l.depth);
    let picks: Vec<Depth> = depths.to_vec();
    let switch = SegmentedControl::new(&names, at).label("Depth").fit().on_pick(move |s: &mut App, i, _| zoom(s, picks[i], &pick)).build(k);
    let read = Button::new("Read again").variant(ButtonVariant::Ghost).help("The record as it is now").on_press(move |s: &mut App| {
        s.records.got.remove(&(pi, again.clone()));
        s.records.foot.remove(&(pi, again.clone()));
        ask(s, pi, &again);
    });
    r.child(rune::el().grow(1.0)).child(switch).child(read.build(k).key("lens-read-again"))
}

/// Text EL keyed KEY, selectable and lit as the comment SOURCE has it (ask.rs `lit`); with none (the Map's call), keyed
/// only, and not selectable, as there it would be a hit target in the call's card (proto-window N-1477 p63).
fn lit_as(s: &App, key: &str, source: Option<&str>, el: El, k: &Kit) -> El {
    match source {
        Some(x) => crate::ask::lit(s, key, x, el.selectable(true), k),
        None => el.key(key),
    }
}

/// The comment box's room after text KEY (ask.rs `lit_room`), in lines commented on as SOURCE; none without.
fn room_as(s: &App, key: &str, source: Option<&str>) -> Option<El> {
    source.and_then(|x| crate::ask::lit_room(s, key, x))
}

/// A line of the record as written: its number, then its text, cut at 220 characters until a press opens it whole.
fn record_row(s: &App, pi: usize, node: &str, source: Option<&str>, l: &RecordLine, k: &Kit) -> El {
    let key = (pi, l.line);
    let cut = !s.records.whole.contains(&key) && l.text.chars().count() > 220;
    let shown = if cut { format!("{}…", l.text.chars().take(220).collect::<String>()) } else { l.text.clone() };
    let at = format!("record-{node}-{}-text", l.line);
    let words = lit_as(s, &at, source, text(shown).font(k.ty.mono).color(k.c.text_base).wrap_anywhere().grow(1.0).shrink(), k);
    let line = row().key(format!("record-{}", l.line)).w_full().gap(10.0).py(2.0).child(text(l.line.to_string()).font(k.ty.mono).color(k.c.text_faint).w(64.0).min_w(64.0)).child(words);
    let line = k.interactive(line).on_press(move |s: &mut App| {
        if !s.records.whole.remove(&key) {
            s.records.whole.insert(key);
        }
    });
    // the comment box's room under the row, while it stands in the flow (I-1476 p9)
    col().w_full().child(line).children(room_as(s, &at, source))
}

/// An event of project PI's record: its time on his clock, to the second (`when::exact`), what it did, its line; a
/// press shows that line in the Record depth, whole.
/// A step's line has a press beside it down to the Wire depth at that step (WIRE), when its run is one the thread lists.
/// ON is (PI, NODE, the comment source of its lines: [`lines_as`]).
fn event_row(s: &App, l: &RecordLine, v: &Value, words: String, (pi, node, source): (usize, &str, Option<&str>), wire: Option<String>, k: &Kit) -> El {
    let at = v.get("at").and_then(Value::as_str).unwrap_or_default();
    let (node, line) = (node.to_string(), l.line);
    let at_key = format!("event-{node}-{line}-text");
    let room = room_as(s, &at_key, source);
    let words = lit_as(s, &at_key, source, text(words).font(k.ty.small).color(k.c.text_base).wrap().grow(1.0).shrink(), k);
    let r = row().key(format!("event-{line}")).w_full().gap(10.0).py(3.0).child(text(crate::when::exact(at)).help(crate::when::full(at)).font(k.ty.mono).color(k.c.text_faint).w(112.0).min_w(112.0)).child(words).child(text(line.to_string()).font(k.ty.small).color(k.c.text_faint));
    let r = k.interactive(r).on_press(move |s: &mut App| {
        zoom_in(s, pi, Depth::Record, &node);
        s.records.whole.insert((pi, line));
    });
    // the comment box's room under the row, while it stands in the flow (I-1476 p9)
    let Some(step) = wire else { return col().w_full().child(r).children(room) };
    let down = Button::new("Wire ›").variant(ButtonVariant::Ghost).control_size(rune_kit::ControlSize::Regular).help("What was sent and what came back").on_press(move |s: &mut App| zoom_in(s, pi, Depth::Wire, &step)).build(k).key(format!("wire-{line}"));
    col().w_full().child(row().w_full().gap(4.0).center_y().child(r.grow(1.0).shrink()).child(down)).children(room)
}

/// The step on the wire that record line V gives (`O-1099511630339.s1`), when it is a step of a run whose id coordd
/// reads: NODE's own ticket, or a run its thread lists (`TicketView.ran`), the thread held live or fetched.
fn wire_at(s: &App, p: &Proj, node: &str, v: &Value) -> Option<String> {
    let (t, n) = (v["t"].as_u64()?, v["step"].as_u64()?);
    if v["ev"] != "step" {
        return None;
    }
    let (me, end) = (parts(node).0, format!("-{t}"));
    let id = if me.ends_with(&end) { me.to_string() } else { crate::find::views(s, p).find(|x| x.id == me)?.ran.iter().find(|r| r.id.ends_with(&end))?.id.clone() };
    Some(format!("{id}.s{n}"))
}

/// Step NODE's round (`O-1099511630339.s1`: a run, and the number of its step on it) in the run's view, live or fetched
/// (find.rs `number`): whom it was given to, the giving its answer came after, its answer (`RunRound`, with
/// docket-edge5's `own` and `took`).
fn round<'a>(s: &'a App, p: &'a Proj, node: &str) -> Option<&'a docket_machine::RunRound> {
    fn of(ns: &[docket_machine::RunNode], step: u32) -> Option<&docket_machine::RunRound> {
        ns.iter().find_map(|n| n.rounds.iter().find(|r| r.step == step).or_else(|| of(&n.nodes, step)))
    }
    let (t, _, Some(step)) = parts(node) else { return None };
    of(&crate::find::views(s, p).find(|v| v.id == t)?.run, step)
}

/// An agent's round on the wire: its agent, and its requests' span as the proxy's log keys time (turn.rs `nanos`), from
/// the giving its answer came after (`took`, else the first) to its answer and a few seconds. None for a round that is
/// no agent's turn: coordd's own (`own`), and one never handed through a session, which his steps, a box's and a
/// policy's decide never are (consult-3 N-1446 p316): their wire is their lines of the record. None too until given.
fn span(r: &docket_machine::RunRound) -> Option<(String, u64, u64)> {
    if r.own {
        return None;
    }
    let (to, at) = match &r.took {
        Some(t) => (t.to.clone(), t.at.clone()),
        None => (r.to.clone()?, r.given.clone()?),
    };
    let until = r.answered.as_deref().and_then(crate::turn::nanos).map_or(u64::MAX, |t| t.saturating_add(crate::turn::SLACK_NS));
    Some((to, crate::turn::nanos(&at)?, until))
}

/// Round R of step NODE given to an agent, on the wire: whom and when, then its turn's requests, each folded to its head
/// until pressed, as a post's "what the model was sent" draws them (raw.rs `turn`).
fn agents_wire(s: &App, pi: usize, r: &docket_machine::RunRound, node: &str, k: &Kit) -> El {
    let Some((to, since, _)) = span(r) else { return text("Not given to anyone yet.").font(k.ty.small).color(k.c.text_muted) };
    let given = r.took.as_ref().map(|t| t.at.as_str()).or(r.given.as_deref()).unwrap_or_default();
    let answered = r.answered.as_deref().map_or_else(|| "not answered yet".to_string(), |a| format!("answered at {}", crate::when::exact(a)));
    let head = text(format!("Given to {to} at {}, {answered}:", crate::when::exact(given))).font(k.ty.small).color(k.c.text_muted).wrap().key(format!("wire-turn-{to}"));
    col().w_full().gap(6.0).child(head).child(crate::raw::turn(s, pi, &to, node, since, k))
}

/// Step NODE's lines among a run's LINES, oldest first: the step, what its call was made from, its giving, what was
/// asked and decided on it (a policy's decide has no step line, only these: consult-3 p397), its answer or its lapse.
/// Only the run's own lines: coordd's record of a run holds the tickets it opened too (consult-3 N-1446 p316). A line
/// names its step, a giving by its delivery, but an answer names only who gave it (`by`): it is the answer of the step
/// open then that it was given to or whose worker that is, the first opened of two (proto-reader p311). A step passed
/// up (E2's `pass_up`: `passed_up`, then the step's line again) is still the one step, and its new worker answers it
/// (proto-reader p426).
fn step_lines<'a>(lines: &'a [RecordLine], node: &str) -> Vec<(&'a RecordLine, Value)> {
    let (t, _, Some(step)) = parts(node) else { return Vec::new() };
    let (Some(run), step) = (docket_machine::parse_ticket(t), u64::from(step)) else { return Vec::new() };
    // the steps open, each with who may answer it; TO may answer step N too
    let mut open: Vec<(u64, Vec<String>)> = Vec::new();
    fn may(open: &mut Vec<(u64, Vec<String>)>, n: u64, to: Option<String>) {
        match open.iter_mut().find(|(m, _)| *m == n) {
            Some((_, by)) => by.extend(to),
            None => open.push((n, to.into_iter().collect())),
        }
    }
    let mut out = Vec::new();
    for l in lines {
        let Ok(v) = serde_json::from_str::<Value>(&l.text) else { continue };
        if v["t"].as_u64() != Some(run) {
            continue;
        }
        let who = |k: &str| v[k].as_str().map(str::to_string);
        let at = match v["ev"].as_str().unwrap_or_default() {
            // a step already open, written again, names its new worker (E2's `pass_up`)
            "step" => v["step"].as_u64().inspect(|n| may(&mut open, *n, who("worker"))),
            "given" => v["delivery"].as_str().and_then(docket_machine::parse_delivery).filter(|(x, _)| *x == run).and_then(|(_, s)| s.strip_prefix('s')?.parse().ok()).inspect(|n| may(&mut open, *n, who("to"))),
            "answer" => open.iter().position(|(_, by)| by.iter().any(|b| v["by"] == b.as_str())).map(|i| open.remove(i).0),
            "lapsed" => v["step"].as_u64().inspect(|n| open.retain(|(m, _)| m != n)),
            "step_args" | "asked" | "decided" | "passed_up" => v["step"].as_u64(),
            _ => None,
        };
        if at == Some(step) {
            out.push((l, v));
        }
    }
    out
}

/// What a step that was no agent's turn did on the wire (coordd's own, his, a box's, a policy's decide): its lines of
/// project PI's record, each in its parts (`wire_words`) with its line; a press shows the line in Record, whole.
fn coordds_wire(s: &App, lines: &[RecordLine], pi: usize, node: &str, source: Option<&str>, k: &Kit) -> Vec<El> {
    let picked = step_lines(lines, node);
    if picked.is_empty() {
        return vec![text("No lines of this step.").font(k.ty.small).color(k.c.text_muted)];
    }
    picked.iter().flat_map(|(l, v)| wire_words(v).into_iter().map(move |(part, title, body)| wire_part(s, l, part, title, body, (pi, node, source), k))).collect()
}

/// A step's line V of the record in words on the wire, as (part, title, body): the step; what its call was made from
/// (`step_args`); its giving, or its passing up to a new holder; what was decided on it, and for the model (`model-for`, the models tried under
/// `provenance`) the prompt as rendered when the record holds it; the answer, or that it lapsed. The prompt is read
/// where E1 is to write it, the `model-for` line's `prompt`; one the record does not hold is not drawn from a call file
/// read now: it says so (consult-3 N-1446 p34). Who fills a step (`fills`) and a policy's decide are no model: no prompt
/// part (p316).
fn wire_words(v: &Value) -> Vec<(&'static str, String, Option<String>)> {
    let pretty = |x: &Value| serde_json::to_string_pretty(x).unwrap_or_default();
    let f = |key: &str| v[key].as_str().unwrap_or_default().to_string();
    let answer = || v["answer"].as_str().map_or_else(|| pretty(&v["answer"]), str::to_string);
    vec![match (v["ev"].as_str().unwrap_or_default(), v["question"].as_str().unwrap_or_default()) {
        ("step", _) => ("step", format!("Step {}: {}, by {}", v["step"], f("call"), f("worker")), None),
        ("given", _) => ("given", format!("Given to {} at {}", f("to"), crate::when::exact(&f("at"))), None),
        ("step_args", _) => ("args", "What its call was made from".to_string(), Some(pretty(&v["args"]))),
        ("asked", q) => ("asked", format!("Asked of {}: {q}", f("role")), None),
        ("decided", "model-for") => {
            let tried: Vec<String> = v["provenance"]["attempts"].as_array().into_iter().flatten().filter_map(|a| a["model"].as_str().map(str::to_string)).collect();
            let model = format!("The model: {}{}", answer(), if tried.is_empty() { String::new() } else { format!(" (tried {})", tried.join(", ")) });
            let prompt = v["prompt"].as_str().map(str::to_string);
            let said = if prompt.is_some() { "The prompt as sent" } else { "Prompt not recorded; its sources are above." };
            return vec![("model", model, None), ("prompt", said.to_string(), prompt)];
        }
        ("decided", "fills") => ("decided", format!("Who fills it: {}", answer()), None),
        ("decided", "decide") => ("decided", format!("Decided: {}", answer()), None),
        ("decided", q) => ("decided", format!("{q}: {}", answer()), None),
        ("passed_up", _) => ("passed", format!("Passed up to {}: {}", f("to"), f("why")), None),
        ("lapsed", _) => ("lapsed", format!("No answer in {} s; went on as {}", v["after"], v["head"]["head"].as_str().unwrap_or("it")), None),
        _ => {
            let why = answer_why(&v["head"]).map(|w| format!(": {w}")).unwrap_or_default();
            let text = v["text"].as_str().filter(|t| !t.is_empty()).map(str::to_string);
            ("answer", format!("{} answered {}{why}", f("by"), v["head"]["head"].as_str().unwrap_or("it")), text.or_else(|| Some(pretty(&v["head"]))))
        }
    }]
}

/// PART of a step on the wire (`wire-args-101211`): TITLE, BODY under it as written, its line of project PI's record; a
/// press shows that line in Record, whole. ON is (PI, NODE, the comment source of its lines: [`lines_as`]).
fn wire_part(s: &App, l: &RecordLine, part: &str, title: String, body: Option<String>, (pi, node, source): (usize, &str, Option<&str>), k: &Kit) -> El {
    let (node, line) = (node.to_string(), l.line);
    let key = format!("wire-{part}-{line}");
    let at = format!("wire-{node}-{part}-{line}-title");
    let title = lit_as(s, &at, source, text(title).font(k.ty.small).color(k.c.text_base).wrap().grow(1.0).shrink(), k);
    // the comment box's room under the title, or in the body after its lines selected, while it stands in the flow
    let mut c = col().w_full().gap(4.0).py(4.0).child(row().w_full().gap(10.0).child(title).child(text(line.to_string()).font(k.ty.small).color(k.c.text_faint))).children(room_as(s, &at, source));
    if let Some(b) = body {
        let (at, make) = (format!("wire-{node}-{part}-{line}-body"), |b: String| text(b).font(k.ty.mono).color(k.c.text_muted).wrap_anywhere().selectable(source.is_some()));
        c = c.child(match source {
            Some(x) => crate::ask::lit_words(s, &at, x, &b, make, k),
            None => make(b).key(at),
        });
    }
    k.interactive(c.key(key)).on_press(move |s: &mut App| {
        zoom_in(s, pi, Depth::Record, &node);
        s.records.whole.insert((pi, line));
    })
}

/// What a line of the record did, in words, for the Events depth: every line says what it is, by its own words below
/// or by [`as_said`]; none for the lines that only carry data (a step's arguments, a reading and what it formed, an
/// agent's tool call): those are the Record depth's (consult-3 N-1446 p174: a reply, a stop, a pass-up dropped). The
/// posts a line lists are THREAD's (`N-1104`) when it names any of them (proto-reader p223: a turn's `keep` names 189).
pub(crate) fn event_words(v: &Value, thread: &str) -> Option<String> {
    let f = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default();
    let value = |x: &Value| x.as_str().map_or_else(|| x.to_string(), str::to_string);
    Some(match f("ev") {
        "step_args" | "formed" | "forming" | "act" => return None,
        "post" => format!("{} wrote “{}”", f("by"), short(f("text"))),
        // a handler's run is a hidden ticket (consult-3 p174: every `opened` line has a parent)
        "opened" if v["t"].as_u64().is_some_and(|t| t >= docket_machine::HIDDEN) => format!("{} run opened", f("kind")),
        "opened" => format!("{} opened", f("kind")),
        "step" => format!("step {}: {}, by {}", value(&v["step"]), f("call"), f("worker")),
        "given" => format!("handed to {}", f("to")),
        "decided" => format!("{}: {}", f("question"), value(&v["answer"])),
        "answer" => {
            let why = answer_why(&v["head"]).map(|w| format!(": {}", short(&w))).unwrap_or_default();
            format!("{} answered {}{why}", f("by"), v["head"]["head"].as_str().unwrap_or("it"))
        }
        "replied" => format!("{} replied: post {}", f("by"), value(&v["n"])),
        // whom his post went to (proto-reader N-1446 p163: "#587 told to paper-research")
        "told" => format!("#{} told to {}", value(&v["n"]), listed(&v["to"]).unwrap_or_default()),
        // the turns his posts went into (proto-reader N-1446 p128): the posts a turn took, and its end
        "turn_ended" if f("delivery").is_empty() => format!("{}'s turn ended", f("agent")),
        "turn_ended" => format!("{}'s turn on {} ended", f("agent"), f("delivery")),
        // a turn took its `items` only on a `deliver`: a `keep` left them waiting, a `report` reported them
        // (proto-reader N-1446 p223, p225; consult-3 p224: 105432, docket-core's keep over 189 ids)
        "event" if f("name") == "delivery_acted" => {
            let x = &v["value"];
            let all: Vec<&str> = x["items"].as_array().map(|xs| xs.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            let own: Vec<&str> = all.iter().copied().filter(|p| p.split_once(".p").is_some_and(|(t, _)| t == thread)).collect();
            let ids = few(&if own.is_empty() { all } else { own });
            let agent = x["agent"].as_str().unwrap_or_default();
            let why = x["why"].as_str().filter(|w| !w.is_empty()).map(|w| format!(": {}", short(w))).unwrap_or_default();
            match x["act"].as_str() {
                Some("deliver") => format!("{agent}'s turn took {ids}"),
                // part 4: the same message, to its session cut back to where AT was given
                Some("fork") => format!("{agent}'s fork from {} took {ids}", x["at"].as_str().unwrap_or("its work")),
                Some("keep") => format!("{agent} kept {ids}{why}"),
                Some("report") => format!("{agent}: {ids} reported{why}"),
                _ => as_said(f("name"), x),
            }
        }
        "event" if v["value"]["delivery"].is_string() => format!("{} {}{}", f("name"), v["value"]["delivery"].as_str().unwrap_or_default(), v["value"]["to"].as_str().map(|t| format!(" to {t}")).unwrap_or_default()),
        "event" => as_said(f("name"), &v["value"]),
        ev => as_said(ev, v),
    })
}

/// A line no words above are written for, in its own: WHAT it is (`passed_up` → "passed up"; an event's name), with
/// whose it is, who did it, from and to whom and why, where its fields X say: "stopped to owner", "consult-3: turn
/// cut", "codex-1: stopped: not running", "notified to paper-research".
fn as_said(what: &str, x: &Value) -> String {
    let f = |k: &str| listed(&x[k]);
    let what = if what.is_empty() { "a line".to_string() } else { what.replace('_', " ") };
    let mut w = match f("agent") {
        Some(a) => format!("{a}: {what}"),
        None => what,
    };
    for (k, said) in [("by", " by "), ("from", " from "), ("to", " to ")] {
        if let Some(n) = f(k) {
            w += &format!("{said}{n}");
        }
    }
    if let Some(y) = f("why") {
        w += &format!(": {}", short(&y));
    }
    w
}

/// Posts IDS, three at most by name: more are their count with the first and the last, "39 posts, N-1104.p560 …
/// N-1104.p599" (s4-p587-kept.png: docket-core's keep at 101553 named 39 of N-1104's posts, a third of the pane).
fn few(ids: &[&str]) -> String {
    match ids {
        [first, .., last] if ids.len() > 3 => format!("{} posts, {first} … {last}", ids.len()),
        _ => ids.join(", "),
    }
}

/// A field's words: a text as it is, a list joined ("consult-3, proto-reader"); none when empty or neither.
fn listed(x: &Value) -> Option<String> {
    match x {
        Value::String(w) if !w.is_empty() => Some(w.clone()),
        Value::Array(ws) if !ws.is_empty() => Some(ws.iter().map(|w| w.as_str().map_or_else(|| w.to_string(), str::to_string)).collect::<Vec<_>>().join(", ")),
        _ => None,
    }
}

/// The words beside answer HEAD's constructor: its `why` (an item closed by hand: `coord item close I WHY`), else the
/// lines that met it (`by`: a goal's Done, by its duty check; paper-research N-1446 p2255), each by its time and what
/// it is.
fn answer_why(head: &Value) -> Option<String> {
    head["why"].as_str().map(str::to_string).or_else(|| crate::texts::met_by(head))
}

/// T's first line, cut at 90 characters.
fn short(t: &str) -> String {
    let one = t.trim().lines().next().unwrap_or_default();
    if one.chars().count() > 90 { format!("{}…", one.chars().take(90).collect::<String>()) } else { one.to_string() }
}

/// The Docket lens L of project PI in a pane that is not focused: the Docket page's own drawing (kinds.rs `page_at`), at
/// the thread it keeps (a run of a handler too, once fetched), else at the scale the Docket shows, the Project scale
/// for a thread.
fn docket_apart(s: &App, pi: usize, p: &Proj, l: &Lens, k: &Kit) -> El {
    use crate::picture::docket::Scale;
    let v = l.at.as_deref().and_then(|id| crate::find::held(s, p, id)).and_then(|n| crate::find::ticket(s, p, n));
    let at = match (&v, crate::picture::docket::scale(&s.tickets.pic_open)) {
        (Some(_), _) => Scale::Thread,
        (None, Scale::Thread) => Scale::Project,
        (None, at) => at,
    };
    col().w_full().h_full().min_h(0.0).key(format!("lens-apart-{pi}-{}", l.filter.key())).scroll(rune::ScrollSpec { stick_bottom: false }).child(crate::kinds::page_at(s, p, at, v, k))
}

impl App {
    /// The page shown, with the thread it has open written into it (`Lens.at`): what a pane or a window keeps of it
    /// when it is left. Threads with a thread open is kept as the Threads lens at it, so a pane not focused holds its
    /// own thread; the page shown holds none, its thread being `tickets.shown`.
    pub fn sel_kept(&self) -> crate::Sel {
        let id = |n: u64| self.proj().and_then(|p| crate::find::ticket(self, p, n)).map(|v| v.id.clone());
        match &self.sel {
            crate::Sel::Tickets => self.tickets.shown.and_then(id).map_or(crate::Sel::Tickets, |at| crate::Sel::Lens(Lens::threads_at(&at))),
            crate::Sel::Lens(l) if l.shows_threads() => crate::Sel::Lens(Lens { at: self.tickets.shown.and_then(id), ..l.clone() }),
            crate::Sel::Lens(l) if l.with_you().is_some() => {
                let a = l.with_you().unwrap_or_default().to_string();
                crate::Sel::Lens(Lens { at: self.withyou.inside.get(&(self.cur, a)).copied().and_then(id), ..l.clone() })
            }
            // the Docket at a thread: a Docket lens keeping it, so a pane beside holds its own (p153)
            crate::Sel::Kinds if crate::picture::docket::scale(&self.tickets.pic_open) == crate::picture::docket::Scale::Thread => match self.tickets.shown.and_then(id) {
                Some(at) => crate::Sel::Lens(Lens { at: Some(at), ..Lens::of(Filter::Docket) }),
                None => crate::Sel::Kinds,
            },
            other => other.clone(),
        }
    }

    /// Show page SEL of the current project: a lens opens at the thread it kept (at its root when it kept none), the
    /// page holding none; the Threads lens is the Threads page ([`Lens::as_sel`]).
    /// A With you lens's thread comes back beside its list only while it stands there (withyou.rs `stands`); else the
    /// list's first, and a line in yaaihh.log, nothing shown to him (consult-3, N-1396 p331). A search's words go into
    /// the field, searched as typed words are (find.rs `typed`). The Docket lens shown is the Kinds page at the thread it
    /// kept, its one value (p120); a thread gone from the snapshot (a run fetched before a restart) opens it at Project,
    /// a line in yaaihh.log.
    pub fn show_sel(&mut self, sel: crate::Sel) {
        let crate::Sel::Lens(l) = sel else {
            self.sel = sel;
            return;
        };
        if l.shows_docket() {
            match l.at.as_deref().map(|id| (id, self.proj().and_then(|p| crate::find::held(self, p, id)))) {
                Some((_, Some(n))) => crate::picture::docket::open_thread(self, n),
                Some((id, None)) => {
                    crate::log::line(&format!("lens: the Docket's thread {id} is not in the snapshot; the Docket opens at Project"));
                    crate::picture::docket::set_scale(self, crate::picture::docket::Scale::Project);
                    self.sel = Lens::of(Filter::Docket).as_sel();
                }
                None => self.sel = Lens::of(Filter::Docket).as_sel(),
            }
            return;
        }
        // an insight keeps the number it lists on screen; its rows asked once, Open → done's asks with them
        if l.filter.insight().is_some() {
            let pi = self.cur;
            let asked = l.filter.insights();
            self.sel = crate::Sel::Lens(l);
            for which in asked {
                crate::insight::ask(self, pi, which);
            }
            return;
        }
        // below Structure its node's lines, asked once; the lens keeps its node on screen. On the wire an agent's
        // round is its turn's requests, read from the proxy's log (raw.rs `open_turn`)
        if let Some(node) = l.at.clone().filter(|_| matches!(l.depth, Depth::Events | Depth::Record | Depth::Wire)) {
            let depth = l.depth;
            self.sel = crate::Sel::Lens(l);
            return read(self, self.cur, depth, &node);
        }
        let n = l.at.as_deref().and_then(|id| self.proj().and_then(|p| crate::find::held(self, p, id)));
        if l.shows_threads() {
            self.tickets.shown = n;
        } else if let Some(a) = l.with_you() {
            let key = (self.cur, a.to_string());
            match n.filter(|n| self.proj().is_some_and(|p| crate::withyou::stands(p, a, *n))) {
                Some(n) => {
                    self.withyou.inside.insert(key, n);
                }
                None => {
                    if let (Some(id), Some(p)) = (&l.at, self.proj()) {
                        crate::log::line(&format!("lens: {} {id} beside {a}'s list in With you is no longer there; the list's first is shown", p.id));
                    }
                    self.withyou.inside.remove(&key);
                }
            }
        }
        if let Filter::Find { titles, .. } = &l.filter {
            self.tickets.find.titles = *titles;
        }
        self.find_words(&l);
        self.sel = Lens { at: None, ..l }.as_sel();
    }

    /// Search lens L's words into the one field, searched as typed words are (find.rs `typed`); within With you, in the
    /// agent's threads only (`all` off). At its show ([`App::show_sel`]), and at a start that restores it as the page
    /// shown (state.rs `restore`; titles only is ui.json's own there).
    pub(crate) fn find_words(&mut self, l: &Lens) {
        if let Filter::Find { words, .. } = &l.filter {
            let f = &mut self.tickets.find;
            (f.q, f.all) = (rune::TextEdit::new(words.clone()), false);
        }
    }
}

/// The lens a scene's word names: `threads`, `docket`, `open-to-done`, `work-talk`, `agents-per-topic`, `with:NAME`, `a2o:NAME`, `a2a:NAME`, `find:WORDS`
/// (`find:WORDS~NAME` within NAME's With you), each opened at a thread by `@ID` (`threads@N-1445`; an insight at the
/// number it lists, `open-to-done@days`); `events@NODE`, `record@NODE` and `wire@STEP`
/// the Threads lens at those depths (`events@N-1104.p587`, `wire@O-1099511630339.s1`).
fn named(w: &str) -> Option<Lens> {
    let (w, at) = w.split_once('@').map_or((w, None), |(w, id)| (w, Some(id.to_string())));
    let depth = match w {
        "events" => Depth::Events,
        "record" => Depth::Record,
        "wire" => Depth::Wire,
        _ => Depth::Structure,
    };
    if depth != Depth::Structure {
        return Some(Lens { filter: Filter::Threads, depth, at });
    }
    let filter = match w.split_once(':') {
        None if w == "threads" => Filter::Threads,
        None if w == "docket" => Filter::Docket,
        None if w == "open-to-done" => Filter::OpenToDone,
        None if w == "work-talk" => Filter::WorkTalk,
        None if w == "agents-per-topic" => Filter::AgentsPerTopic,
        Some(("with", a)) => Filter::WithYou { agent: a.into() },
        Some(("a2o", a)) => Filter::A2o { agent: a.into() },
        Some(("a2a", a)) => Filter::A2a { agent: a.into() },
        Some(("find", w)) => match w.split_once('~') {
            Some((w, a)) => Filter::Find { words: w.into(), titles: false, agent: Some(a.into()) },
            None => Filter::Find { words: w.into(), titles: false, agent: None },
        },
        _ => return None,
    };
    Some(Lens { at, ..Lens::of(filter) })
}

/// The scene `lens` (lib.rs SCENES), after the tickets scene has read the first project and picked YAAIHH_TICKET:
/// YAAIHH_LENS=WORD that lens shown at that thread, at post YAAIHH_LENS_POST=N when given (brought into view, as a
/// search's hit is); YAAIHH_LENS_BESIDE=WORD[,WORD] other lenses in panes beside it, the first keeping focus;
/// YAAIHH_TREE_OPEN=N-1[,N-2] those threads of the structure unfolded; YAAIHH_RECORD_LINES=FILE the record's lines a
/// depth's node is given, as coordd answers them (`RecordLine` JSON lines), for the node of the lens shown or beside;
/// YAAIHH_INSIGHT_ROWS=FILE the rows of insights (3) and (2) as coordd answers them (`docket_machine::insight::Thread` JSON lines);
/// YAAIHH_INSIGHT_TURNS=FILE insight (1)'s (`docket_machine::insight::Turn` JSON lines); YAAIHH_INSIGHT_ASKS=FILE
/// Open → done's asks (`docket_machine::insight::Ask` JSON lines).
pub fn scene(s: &mut App) {
    if let Some(l) = std::env::var("YAAIHH_LENS").ok().as_deref().and_then(named) {
        if l.shows_docket() || l.filter.insight().is_some() {
            s.show_sel(crate::Sel::Lens(l));
        } else {
            s.sel = Lens { at: None, ..l }.as_sel();
        }
    }
    let post = std::env::var("YAAIHH_LENS_POST").ok().and_then(|n| n.parse::<u32>().ok());
    if let (Some(n), Some(t)) = (post, s.tickets.shown) {
        let id = s.proj().and_then(|p| crate::find::ticket(s, p, t)).map(|v| v.id.clone()).unwrap_or_default();
        crate::find::open_at(s, t, &format!("{id}.p{n}"), Some(n));
    }
    let beside: Vec<Lens> = std::env::var("YAAIHH_LENS_BESIDE").unwrap_or_default().split(',').filter_map(named).collect();
    if !beside.is_empty() {
        s.panes = std::iter::once((0, s.sel_kept())).chain(beside.into_iter().map(|l| (0, crate::Sel::Lens(l)))).collect();
        s.focus = 0;
    }
    for id in std::env::var("YAAIHH_TREE_OPEN").unwrap_or_default().split(',').filter(|x| !x.is_empty()) {
        s.streams.open.insert(format!("tree|0|{id}"));
    }
    if let Ok(text) = std::env::var("YAAIHH_RECORD_LINES").and_then(|f| std::fs::read_to_string(f).map_err(|_| std::env::VarError::NotPresent)) {
        let nodes: Vec<String> = std::iter::once(&s.sel).chain(s.panes.iter().map(|(_, x)| x)).filter_map(|x| match x {
            crate::Sel::Lens(l) if !l.shows_threads() && l.filter.insight().is_none() => l.at.clone(),
            _ => None,
        }).collect();
        for node in nodes {
            came(s, 0, node, None, true, text.clone());
        }
    }
    // YAAIHH_FOLDER_ROWS=FILE: agent YAAIHH_AGENT's folder, its rows as coordd's `Graph::folder` gives them, held as if
    // answered, and its page shown, where fix 4's presses open the parts of it (sidebar.rs `folder_part`)
    if let (Ok(agent), Ok(file)) = (std::env::var("YAAIHH_AGENT"), std::env::var("YAAIHH_FOLDER_ROWS")) {
        match std::fs::read_to_string(&file).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<Vec<docket_machine::graph_slice::Row>>(&t).map_err(|e| e.to_string())) {
            Ok(rows) => {
                crate::sidebar::folder_held(s, 0, &agent, rows);
                s.sel = crate::Sel::Agent(agent);
            }
            Err(e) => eprintln!("YAAIHH_FOLDER_ROWS {file}: {e}"),
        }
    }
    // YAAIHH_FOLDER_PRESS=system-prompt|initial-prompt|home: that press on its page, as his click opens it (chat.rs);
    // YAAIHH_FOLDER_TEXTS=FILE: its blocks' versions' texts by node (`{"text:H": "…"}`), held as coordd's answers to
    // System prompt's press are (sidebar.rs `part_texts`), since a headless window has no live link to ask
    if let (Ok(agent), Ok(press)) = (std::env::var("YAAIHH_AGENT"), std::env::var("YAAIHH_FOLDER_PRESS")) {
        let f = match press.as_str() {
            "system-prompt" => Some(Filter::SystemPrompt { agent }),
            "initial-prompt" => Some(Filter::InitialPrompt { agent }),
            "home" => Some(Filter::AgentHome { agent }),
            _ => None,
        };
        match f {
            Some(f) => s.open_pane_beside(0, crate::Sel::Lens(Lens::of(f))),
            None => eprintln!("YAAIHH_FOLDER_PRESS {press}: system-prompt, initial-prompt or home"),
        }
    }
    // YAAIHH_PROMPT_ROWS=FILE: agent YAAIHH_AGENT's system prompt as coordd's `Graph::prompt` gives it, by what it is
    // of (`{"prompt": [rows], "text:H": [a version's requests]}`), held as if answered (sidebar.rs `apart_held`), where
    // System prompt draws it from a coordd at PROMPT_PROTOCOL (fix 4)
    if let (Ok(agent), Ok(file)) = (std::env::var("YAAIHH_AGENT"), std::env::var("YAAIHH_PROMPT_ROWS")) {
        type Held = std::collections::BTreeMap<String, Vec<docket_machine::graph_slice::Row>>;
        match std::fs::read_to_string(&file).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<Held>(&t).map_err(|e| e.to_string())) {
            Ok(held) => held.into_iter().for_each(|(of, rows)| crate::sidebar::apart_held(s, 0, &agent, &of, rows)),
            Err(e) => eprintln!("YAAIHH_PROMPT_ROWS {file}: {e}"),
        }
    }
    if let Ok(file) = std::env::var("YAAIHH_FOLDER_TEXTS") {
        match std::fs::read_to_string(&file).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<std::collections::BTreeMap<String, String>>(&t).map_err(|e| e.to_string())) {
            Ok(texts) => s.folders.texts.extend(texts.into_iter().map(|(node, text)| ((0, node), Ok(text)))),
            Err(e) => eprintln!("YAAIHH_FOLDER_TEXTS {file}: {e}"),
        }
    }
    for (var, which) in [("YAAIHH_INSIGHT_ROWS", coord_types::Insight::OpenToDone), ("YAAIHH_INSIGHT_TURNS", coord_types::Insight::WorkTalk), ("YAAIHH_INSIGHT_ASKS", coord_types::Insight::Asks)] {
        if let Ok(text) = std::env::var(var).and_then(|f| std::fs::read_to_string(f).map_err(|_| std::env::VarError::NotPresent)) {
            crate::insight::came(s, 0, which, true, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coord_types::TicketView;
    use serde_json::json;

    fn t(id: &str, n: u64, posts: serde_json::Value) -> TicketView {
        serde_json::from_value(json!({"id": id, "n": n, "kind": "thread", "opener": "paper-research", "ctx": {}, "form": {}, "title": id, "status": {"is": "resting"},
            "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": "2026-11-20T18:00:00Z", "given": [], "posts": posts, "waits_on": []}))
        .unwrap()
    }

    /// N-1104 with his p586, the agents' N-1444, and the brain talk N-1445 under N-1104 (his p627).
    fn project() -> crate::Proj {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        let his = t("N-1104", 1104, json!([{"n": 586, "at": "2026-11-20T18:22:55Z", "by": "owner", "text": "eh yeah but wait lets analyze"}]));
        let theirs = t("N-1444", 1444, json!([{"n": 1, "at": "2026-11-20T18:52:56Z", "by": "paper-research", "text": "The first brain", "names": ["docket-core2"]}]));
        let mut fork = t("N-1445", 1445, json!([{"n": 3, "at": "2026-11-20T19:09:24Z", "by": "owner", "text": "okay nice s ocan we run troiugh some hypothical"}]));
        fork.parent = Some("N-1104".into());
        for v in [his, theirs, fork] {
            p.live.tickets.insert(v.n, v);
        }
        p
    }

    fn frames(rt: &mut rune::runtime::Runtime<App>) {
        for _ in 0..4 {
            rt.frame(0.016);
        }
    }

    fn press(rt: &mut rune::runtime::Runtime<App>, key: &str) {
        let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is drawn"));
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        frames(rt);
    }

    /// A lens dropped beside a page is a pane of its own, opened at the thread it carried, and keeps that thread while
    /// the other pane has focus; dropped on a pane's middle it takes that pane's place, at its root. Two panes on one
    /// lens hold two threads.
    #[test]
    fn a_lens_dropped_beside_a_page_keeps_its_thread_while_the_other_pane_has_focus() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![project()];
        s.cur = 0;
        s.show_sel(crate::Sel::Lens(Lens::threads_at("N-1104")));
        let brain = Lens::threads_at("N-1445");
        s.drop_sel(0, crate::Sel::Lens(brain.clone()), 0, crate::panes::Zone::Right);
        assert_eq!((s.panes.len(), s.focus), (2, 1), "a pane of its own beside, focused");
        assert_eq!((s.sel.clone(), s.tickets.shown), (crate::Sel::Tickets, Some(1445)), "open at the thread it carried");
        assert_eq!(s.panes[0], (0, crate::Sel::Lens(Lens::threads_at("N-1104"))), "the first pane keeps its own");
        s.focus_pane(0);
        assert_eq!(s.tickets.shown, Some(1104), "the other pane back at its own thread");
        assert_eq!(s.panes[1], (0, crate::Sel::Lens(brain.clone())), "kept with its thread while the other pane has focus");
        assert_eq!(s.layout().panes[1], (0, crate::Sel::Lens(brain.clone())), "and so in the window's layout");
        s.drop_sel(0, crate::Sel::Lens(Lens::threads_at("N-1444")), 0, crate::panes::Zone::Center);
        assert_eq!((s.focus, s.tickets.shown), (0, Some(1444)), "the same lens in the first pane, at another thread");
        assert_eq!(s.panes[1], (0, crate::Sel::Lens(brain.clone())), "the second keeps its own");
        s.focus_pane(1);
        assert_eq!((s.sel.clone(), s.tickets.shown), (crate::Sel::Tickets, Some(1445)), "back at it");
        assert_eq!(s.panes[0], (0, crate::Sel::Lens(Lens::threads_at("N-1444"))));
        s.drop_sel(0, crate::Sel::Lens(Lens::of(Filter::Threads)), 0, crate::panes::Zone::Center);
        assert_eq!((s.focus, s.tickets.shown), (0, None), "dropped with no thread: at its root");
    }

    /// The pane that is not focused draws the thread its lens keeps, as With you draws the thread beside its list.
    #[test]
    fn a_pane_not_focused_draws_the_thread_its_lens_keeps() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![project()];
        s.cur = 0;
        s.show_sel(crate::Sel::Lens(Lens::threads_at("N-1104")));
        s.drop_sel(0, crate::Sel::Lens(Lens::threads_at("N-1445")), 0, crate::panes::Zone::Right);
        s.focus_pane(0);
        let mut rt = crate::view_cost::window(s);
        frames(&mut rt);
        assert!(rt.find("with-you-thread-N-1445").is_some(), "the brain talk drawn in the pane beside");
        assert_eq!(rt.state.tickets.shown, Some(1104), "N-1104 on the focused page");
    }

    /// One value for one page (proto-lead N-1446 p120): a pinned Threads lens pressed shows `Sel::Tickets`, and the
    /// pin is lit while Threads is shown from the sidebar's own row.
    #[test]
    fn a_pinned_threads_lens_is_lit_while_threads_is_shown() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![project()];
        s.cur = 0;
        s.sel = crate::Sel::Agents;
        let threads = Lens::of(Filter::Threads);
        s.pins.list.push(crate::sidebar::Pin { project: "research".into(), id: String::new(), agent: "Threads".into(), thread: false, lens: Some(threads.clone()) });
        let mut rt = crate::view_cost::window(s);
        frames(&mut rt);
        assert!(!crate::sidebar::lens_shown(&rt.state, &threads));
        press(&mut rt, "pin-research-lens-threads");
        assert_eq!(rt.state.sel, crate::Sel::Tickets, "the pin shows the Threads page itself");
        rt.update(|s, _| s.sel = crate::Sel::Agents);
        rt.invalidate();
        frames(&mut rt);
        assert!(!crate::sidebar::lens_shown(&rt.state, &threads));
        press(&mut rt, "tickets");
        assert_eq!(rt.state.sel, crate::Sel::Tickets);
        assert!(crate::sidebar::lens_shown(&rt.state, &threads), "the pin lit by the sidebar's Threads");
    }

    /// Going back and forth inside the lens on screen is history's thread, not a new page: the page shown keeps no
    /// `at` (docket-edge6's N-1446 p15 point 2).
    #[test]
    fn the_lens_on_screen_keeps_no_node_of_its_own() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![project()];
        s.cur = 0;
        s.show_sel(crate::Sel::Lens(Lens::threads_at("N-1104")));
        assert_eq!((s.sel.clone(), s.tickets.shown), (crate::Sel::Tickets, Some(1104)));
        crate::find::open_at(&mut s, 1445, "N-1445", None);
        assert_eq!(s.sel, crate::Sel::Tickets, "a thread opened in it leaves the page as it was");
        assert_eq!(s.sel_kept(), crate::Sel::Lens(Lens::threads_at("N-1445")), "what a pane keeps of it carries the thread");
    }

    /// The structure opens at the root: his threads that stand on their own, the agents' N-1444 behind Between agents
    /// (yaaihh/fresh N-1751 p6); a subthread stands folded under its thread until its ▸ is pressed, and a press on it
    /// opens it.
    #[test]
    fn the_structure_opens_at_the_root_and_a_thread_unfolds_to_its_subthreads() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![project()];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        let mut rt = crate::view_cost::window(s);
        frames(&mut rt);
        assert!(rt.find("tree-N-1104").is_some() && rt.find("tree-N-1444").is_none() && rt.find("tree-between-agents").is_some(), "his thread at the root, the agents' behind one row");
        assert!(rt.find("tree-N-1445").is_none(), "the brain talk folded under N-1104");
        press(&mut rt, "tree-fold-N-1104");
        assert!(rt.find("tree-N-1445").is_some(), "unfolded");
        press(&mut rt, "tree-N-1445");
        assert_eq!((rt.state.sel.clone(), rt.state.tickets.shown), (crate::Sel::Tickets, Some(1445)), "opened on the page");
    }

    /// A thread opened on the page is revealed in the structure, as VS Code's explorer reveals the file open: the
    /// threads above it unfolded, its row drawn and brought into view; a folded section is left folded.
    #[test]
    fn a_thread_opened_is_revealed_in_the_structure() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![project()];
        s.cur = 0;
        s.show_sel(crate::Sel::Lens(Lens::of(Filter::Threads)));
        crate::find::open_at(&mut s, 1445, "N-1445", None);
        assert_eq!(crate::sidebar::reveal_step(&mut s), (true, Some("tree-N-1445".into())));
        assert!(s.streams.open.contains("tree|0|N-1104"), "its thread unfolded");
        let mut rt = crate::view_cost::window(s);
        frames(&mut rt);
        assert!(rt.find("tree-N-1445").is_some(), "its row drawn");
        assert_eq!(rt.state.sel, crate::Sel::Tickets, "opened on the Threads page, beside its list");
        // folded, the section stays folded
        rt.update(|s, _| {
            s.streams.open.remove("tree|0|N-1104");
            s.streams.open.insert("tree|0|section".into());
            s.tickets.shown = Some(1104);
        });
        assert_eq!(crate::sidebar::reveal_step(&mut rt.state), (false, None));
    }

    /// LINES as coordd gives them, each at its line.
    fn record(lines: Vec<(u64, Value)>) -> String {
        lines.into_iter().map(|(line, v)| serde_json::to_string(&coord_types::RecordLine { line, text: v.to_string() }).unwrap()).collect::<Vec<_>>().join("\n")
    }

    /// p587's lines in small, at their lines in tonight's record, as it writes them (the post's words, the answer's why
    /// and the delivery of p586 before them made up): his post, the route run it opened, its step 1 the watcher's and
    /// step 2 his clarify, answered Send; then the turn that took it.
    fn tonight() -> String {
        let run = 1099511630339u64;
        record(vec![
            (101207, json!({"ev": "event", "at": "2026-11-20T18:23:02Z", "name": "delivered", "value": {"delivery": "N-1104.p586", "t": 1104, "to": "paper-research"}})),
            (101208, json!({"ev": "post", "at": "2026-11-20T18:23:17Z", "t": 1104, "by": "owner", "text": "and the reset could be a special action"})),
            (101209, json!({"ev": "opened", "at": "2026-11-20T18:23:17Z", "t": run, "kind": "owner_post", "parent": 1104, "program": "r <- match p.all {"})),
            (101210, json!({"ev": "step", "at": "2026-11-20T18:23:17Z", "t": run, "step": 1, "worker": "watcher", "call": "route"})),
            (101211, json!({"ev": "step_args", "at": "2026-11-20T18:23:17Z", "t": run, "step": 1, "args": {"others": ["paper-research", "consult-lead"], "text": {"post": {"n": 587, "t": 1104}}, "to": []}})),
            (101212, json!({"ev": "given", "at": "2026-11-20T18:23:18Z", "t": run, "to": "watcher", "delivery": "O-1099511630339.s1", "upto": 0})),
            (101215, json!({"ev": "decided", "at": "2026-11-20T18:23:20Z", "t": run, "step": 1, "question": "model-for", "answer": "gpt-6-luna", "provenance": {"at": "2026-11-20T18:23:18Z", "attempts": [{"model": "gpt-6-luna"}], "chosen": "gpt-6-luna"}})),
            (101216, json!({"ev": "answer", "at": "2026-11-20T18:23:20Z", "t": run, "by": "watcher", "head": {"head": "Unclear", "why": "No agent listed is the one his post answers."}, "text": ""})),
            (101217, json!({"ev": "step", "at": "2026-11-20T18:23:20Z", "t": run, "step": 2, "worker": "owner", "call": "clarify"})),
            (101218, json!({"ev": "step_args", "at": "2026-11-20T18:23:20Z", "t": run, "step": 2, "args": {"agents": ["paper-research", "consult-lead"], "text": {"post": {"n": 587, "t": 1104}}, "why": "No agent listed is the one his post answers."}})),
            (101230, json!({"ev": "turn_ended", "at": "2026-11-20T18:23:32Z", "agent": "paper-research", "delivery": "N-1104.p586", "stood": ""})),
            (101260, json!({"ev": "answer", "at": "2026-11-20T18:24:13Z", "t": run, "by": "owner", "head": {"head": "Send", "to": ["paper-research"]}, "text": ""})),
            (101262, json!({"ev": "told", "at": "2026-11-20T18:24:13Z", "t": 1104, "n": 587, "to": ["paper-research"], "inform": ["consult-lead"]})),
            (101265, json!({"ev": "event", "at": "2026-11-20T18:24:14Z", "name": "delivery_acted", "value": {"act": "deliver", "agent": "paper-research", "items": ["N-1104.p587"], "rest": ["N-1437.p2"]}, "fired": []})),
        ])
    }

    /// The route run O-1099511630339's own lines, as coordd's `Record` gives them for the run, at their lines in
    /// tonight's record: its step 1 the watcher's, step 2 his. Between step 1's giving and its answer, the lines of a
    /// ticket the run opened (made up: `ticket_record` gives a run's with its own), whose step 1 the watcher answers too.
    fn run_lines() -> String {
        let (run, opened) = (1099511630339u64, 1099511630341u64);
        let mut lines: Vec<(u64, Value)> = tonight().lines().map(|l| serde_json::from_str::<RecordLine>(l).unwrap()).map(|l| (l.line, serde_json::from_str::<Value>(&l.text).unwrap())).filter(|(_, v)| v["t"] == run).collect();
        lines.extend([
            (101213, json!({"ev": "step", "at": "2026-11-20T18:23:18Z", "t": opened, "step": 1, "worker": "watcher", "call": "asks"})),
            (101214, json!({"ev": "answer", "at": "2026-11-20T18:23:19Z", "t": opened, "by": "watcher", "head": {"head": "Nothing"}, "text": ""})),
            (101261, json!({"ev": "posted", "at": "2026-11-20T18:24:13Z", "t": run, "n": 1, "by": "owner", "step": 2})),
            (101263, json!({"ev": "closed", "at": "2026-11-20T18:24:13Z", "t": run, "result": []})),
        ]);
        lines.sort_by_key(|(n, _)| *n);
        record(lines)
    }

    /// A node is an id coordd reads: a thread, its post, a run's step; the request is its ticket's record from the post.
    #[test]
    fn a_node_asks_for_its_tickets_record_from_its_post() {
        assert!(matches!(request("N-1104.p587", None), coord_types::Request::Record { t, at: Some(587), after: None, cap: 0 } if t == "N-1104"));
        assert!(matches!(request("N-1104", Some(102803)), coord_types::Request::Record { t, at: None, after: Some(102803), .. } if t == "N-1104"));
        assert_eq!(parts("O-1099511630339.s1"), ("O-1099511630339", None, Some(1)));
    }

    /// Tonight's p587 at the Events depth: what ran, a line each, in words; a step's arguments are the Record depth's.
    #[test]
    fn tonights_p587_in_events_is_what_ran_in_words() {
        let words: Vec<Option<String>> = tonight().lines().map(|l| event_words(&serde_json::from_str(&serde_json::from_str::<RecordLine>(l).unwrap().text).unwrap(), "N-1104")).collect();
        assert_eq!(
            words,
            [
                Some("delivered N-1104.p586 to paper-research".to_string()),
                Some("owner wrote “and the reset could be a special action”".into()),
                Some("owner_post run opened".into()),
                Some("step 1: route, by watcher".into()),
                None,
                Some("handed to watcher".into()),
                Some("model-for: gpt-6-luna".into()),
                Some("watcher answered Unclear: No agent listed is the one his post answers.".into()),
                Some("step 2: clarify, by owner".into()),
                None,
                Some("paper-research's turn on N-1104.p586 ended".into()),
                Some("owner answered Send".into()),
                Some("#587 told to paper-research".into()),
                Some("paper-research's turn took N-1104.p587".into()),
            ]
        );
    }

    /// Every line of the record says what it is at the Events depth (consult-3 N-1446 p174): his p586's reply (101231),
    /// a turn ended, a stop, a pass-up, as the record writes them; only the lines that carry data say nothing.
    #[test]
    fn every_line_but_the_data_lines_is_an_event() {
        let words = |v: Value| event_words(&v, "N-1104");
        assert_eq!(words(json!({"ev": "replied", "at": "2026-11-20T18:23:32Z", "t": 1104, "n": 588, "by": "paper-research", "names": ["owner"]})).as_deref(), Some("paper-research replied: post 588"));
        assert_eq!(words(json!({"ev": "turn_ended", "at": "2026-11-20T20:13:52Z", "agent": "consult-3", "delivery": "N-1334.p14", "informs": ["N-1437.p12"], "stood": ""})).as_deref(), Some("consult-3's turn on N-1334.p14 ended"));
        assert_eq!(words(json!({"ev": "stopped", "at": "2026-11-18T06:14:00Z", "t": 409, "raised": {"exc": "Silent", "value": "minder"}, "to": "owner"})).as_deref(), Some("stopped to owner"));
        assert_eq!(words(json!({"ev": "passed_up", "at": "2026-11-20T21:00:00Z", "t": 1446, "step": 2, "from": "f1-1", "to": "paper-research", "why": "the fork ended with the join held"})).as_deref(), Some("passed up from f1-1 to paper-research: the fork ended with the join held"));
        assert_eq!(words(json!({"ev": "event", "at": "2026-11-20T13:56:36Z", "name": "stopped", "value": {"above": ["paper-research"], "agent": "codex-1", "item": "N-1127.p2", "why": "not running"}, "fired": []})).as_deref(), Some("codex-1: stopped: not running"));
        assert_eq!(words(json!({"ev": "notified", "at": "2026-11-18T17:37:18Z", "t": 569, "event": "closed", "to": ["paper-research"]})).as_deref(), Some("notified to paper-research"));
        assert_eq!(words(json!({"ev": "turn_cut", "at": "2026-11-12T19:38:29Z", "agent": "yaaihh"})).as_deref(), Some("yaaihh: turn cut"));
        // a shown ticket opened is no run, a hidden one is
        assert_eq!(words(json!({"ev": "opened", "at": "2026-11-20T19:07:16Z", "t": 1445, "kind": "thread", "parent": null})).as_deref(), Some("thread opened"));
        assert_eq!(words(json!({"ev": "opened", "at": "2026-11-20T18:23:17Z", "t": 1099511630339u64, "kind": "owner_post", "parent": 1104})).as_deref(), Some("owner_post run opened"));
        for data in ["step_args", "formed", "forming", "act"] {
            assert_eq!(words(json!({"ev": data, "at": "2026-11-20T18:23:17Z", "t": 1104})), None, "{data} is the Record depth's");
        }
    }

    /// A turn took a post only on a `deliver`: a `keep` left it waiting, a `report` reported it; each lists this
    /// thread's posts only (proto-reader N-1446 p223: 105432, docket-core's keep over 189 ids, N-1104.p631 among them).
    /// And whom his post was told to (p163).
    /// An item's close says its words: a hand's `why` as before, and a goal's Done by its duty check the lines that met
    /// it (`by`, `why` none), in the Events row and the wire's title alike (paper-research N-1446 p2255).
    #[test]
    fn an_items_close_says_a_hands_why_and_a_checks_lines() {
        let words = |v: Value| event_words(&v, "N-1104");
        let hand = json!({"ev": "answer", "at": "2026-11-23T10:00:00Z", "t": 12, "by": "lead", "head": {"head": "Done", "why": "landed in 71", "by": null}});
        assert_eq!(words(hand.clone()).as_deref(), Some("lead answered Done: landed in 71"));
        let by = ["2026-11-23T17:08:47Z job end 41-17", "2026-11-23T17:56:46Z window_release"];
        let check = json!({"ev": "answer", "at": "2026-11-23T17:56:46Z", "t": 12, "by": "I-12", "head": {"head": "Done", "why": null, "by": by}});
        assert_eq!(words(check.clone()).as_deref(), Some("I-12 answered Done: by 2026-11-23T17:08:47Z job end 41-17; 2026-11-23T17:56:46Z window_release"));
        let title = |v: &Value| wire_words(v).into_iter().find(|(part, ..)| *part == "answer").map(|(_, title, _)| title);
        assert_eq!(title(&hand).as_deref(), Some("lead answered Done: landed in 71"));
        assert_eq!(title(&check).as_deref(), Some("I-12 answered Done: by 2026-11-23T17:08:47Z job end 41-17; 2026-11-23T17:56:46Z window_release"));
        // a Dropped, and a constructor with neither, as before
        assert_eq!(words(json!({"ev": "answer", "t": 12, "by": "lead", "head": {"head": "Dropped", "why": "done elsewhere"}})).as_deref(), Some("lead answered Dropped: done elsewhere"));
        assert_eq!(words(json!({"ev": "answer", "t": 12, "by": "lead", "head": {"head": "Done", "by": []}})).as_deref(), Some("lead answered Done"));
    }

    #[test]
    fn a_turn_took_a_post_only_on_a_deliver_and_a_line_lists_this_threads_posts() {
        let acted = |act: &str, agent: &str, items: &[&str], why: Option<&str>| json!({"ev": "event", "at": "2026-11-20T19:44:04Z", "name": "delivery_acted", "value": {"act": act, "agent": agent, "items": items, "why": why}, "fired": []});
        let words = |v: Value| event_words(&v, "N-1104");
        assert_eq!(words(acted("deliver", "paper-research", &["N-412.p2198", "N-1104.p587"], None)).as_deref(), Some("paper-research's turn took N-1104.p587"));
        assert_eq!(words(acted("keep", "docket-core", &["N-101.p1885", "N-1104.p631", "N-412.p2198"], Some("Consult-3 approved docket-core2's follow-up"))).as_deref(), Some("docket-core kept N-1104.p631: Consult-3 approved docket-core2's follow-up"));
        assert_eq!(words(acted("report", "a2h-speed", &["N-1104.p443", "N-11040.p1"], Some("not running"))).as_deref(), Some("a2h-speed: N-1104.p443 reported: not running"));
        // a `fork` (part 4) took them as a deliver does, from where its work was given
        let mut fork = acted("fork", "w2", &["N-1104.p590"], None);
        fork["value"]["at"] = json!("N-1104.p585");
        assert_eq!(words(fork).as_deref(), Some("w2's fork from N-1104.p585 took N-1104.p590"));
        assert_eq!(words(json!({"ev": "told", "at": "2026-11-20T18:24:13Z", "t": 1104, "n": 587, "to": ["paper-research"], "inform": ["consult-lead", "here"]})).as_deref(), Some("#587 told to paper-research"));
        // more than three of this thread's: their count, the first and the last
        let kept = acted("keep", "docket-core", &["N-412.p2190", "N-1104.p560", "N-1104.p561", "N-1104.p562", "N-1104.p599"], Some("The latest review cleared docket-core2's item-step holds"));
        assert_eq!(words(kept).as_deref(), Some("docket-core kept 4 posts, N-1104.p560 … N-1104.p599: The latest review cleared docket-core2's item-step holds"));
        assert_eq!(few(&["N-1104.p1", "N-1104.p2", "N-1104.p3"]), "N-1104.p1, N-1104.p2, N-1104.p3");
    }

    /// A full page of 400 lines says more follow; the press at its foot asks for the page after its last line, and the
    /// 401st line comes under the 400 (consult-3 N-1446 p174). A thread zoomed with no post found starts at its last.
    #[test]
    fn the_401st_line_is_reached_by_the_press_at_the_foot() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = project();
        p.protocol = coord_types::RECORD_PROTOCOL;
        s.projects = vec![p];
        s.cur = 0;
        assert_eq!(node_of(&s, 1104, "N-1104"), "N-1104.p586", "no post found: its last post");
        let page = |from: u64, n: u64| (from..from + n).map(|i| serde_json::to_string(&RecordLine { line: i, text: json!({"ev": "given", "at": "2026-11-20T18:24:15Z", "t": 1104, "to": "paper-research"}).to_string() }).unwrap()).collect::<Vec<_>>().join("\n");
        let node = "N-1104.p586".to_string();
        s.show_sel(crate::Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Record, at: Some(node.clone()) }));
        came(&mut s, 0, node.clone(), None, true, page(101208, 400));
        let out = std::env::temp_dir().join(format!("yaaihh-record-more-{}.jsonl", std::process::id()));
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        let mut rt = crate::view_cost::window(s);
        frames(&mut rt);
        assert!(rt.find("record-101607").is_some() && rt.find("record-101608").is_none(), "the 400");
        // scrolled to their foot, as he reads down to it
        let (x, y) = rt.find("record-101208").unwrap();
        rt.wheel(x, y, -100_000.0);
        frames(&mut rt);
        press(&mut rt, "lens-more");
        rt.update(|s, _| s.projects[0].link = None);
        let _ = child.wait();
        let asked: Vec<Value> = std::fs::read_to_string(&out).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).map(|v| v["req"].clone()).collect();
        let _ = std::fs::remove_file(&out);
        assert_eq!(asked, [json!({"op": "record", "t": "N-1104", "at": 586, "after": 101607, "cap": 0})], "the page after the 400th");
        rt.update(|s, _| came(s, 0, "N-1104.p586".into(), Some(101607), true, page(101608, 1)));
        rt.invalidate();
        frames(&mut rt);
        assert!(rt.find("record-101208").is_some() && rt.find("record-101608").is_some(), "the 401st under the 400");
        assert!(rt.find("lens-more").is_none() && !rt.state.records.foot.contains_key(&(0, node)), "a page not full is the end");
    }

    /// From a post found on N-1104, Events in its head zooms to what ran from that post; a press on an event shows its
    /// line in the Record depth, whole; the crumb comes back up to the thread at that post.
    #[test]
    fn a_thread_zooms_down_to_its_events_and_record_and_comes_back_up() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = project();
        p.protocol = coord_types::RECORD_PROTOCOL;
        s.projects = vec![p];
        s.cur = 0;
        s.show_sel(crate::Sel::Lens(Lens::of(Filter::Threads)));
        crate::find::open_at(&mut s, 1104, "N-1104.p586", Some(586));
        let mut rt = crate::view_cost::window(s);
        frames(&mut rt);
        // Events is behind the thread's ⋯ (paper-research N-1446 p3296)
        press(&mut rt, "thread-more");
        press(&mut rt, "thread-events");
        let at = Lens { filter: Filter::Threads, depth: Depth::Events, at: Some("N-1104.p586".into()) };
        assert_eq!(rt.state.sel, crate::Sel::Lens(at.clone()), "at the post found");
        rt.update(|s, _| came(s, 0, "N-1104.p586".into(), None, true, tonight()));
        rt.invalidate();
        frames(&mut rt);
        assert!(rt.find("event-101208").is_some() && rt.find("event-101216").is_some(), "his post, the answer");
        assert!(rt.find("event-101211").is_none(), "the step's arguments are not an event");
        press(&mut rt, "event-101216");
        assert_eq!(rt.state.sel, crate::Sel::Lens(Lens { depth: Depth::Record, ..at }), "down to the record");
        assert!(rt.find("record-101211").is_some() && rt.state.records.whole.contains(&(0, 101216)), "every line, the one pressed whole");
        press(&mut rt, "lens-up");
        assert_eq!((rt.state.sel.clone(), rt.state.tickets.shown, rt.state.tickets.find.found), (crate::Sel::Tickets, Some(1104), Some((1104, 586))), "back up at the post");
    }

    /// Most of the Map's lines closed more than a day ago and are not in the snapshot: its "Open its page" zooms to
    /// Structure at one, and the thread is fetched as a find's is (find.rs `number`) and opened at its post. A pane
    /// left on it comes back to it and draws it while the other pane has focus. One coordd does not give is said in
    /// the notice and nothing moves; one held opens with nothing asked. Its stand-in coordd is `sh`, which the Windows
    /// test exe has not (docket-edge8's 31f2b5cb; proto-reader N-1479 p82).
    #[test]
    #[cfg(not(windows))]
    fn a_thread_the_snapshot_does_not_hold_is_fetched_when_zoomed_to() {
        let dir = std::env::temp_dir().join(format!("yaaihh-lens-zoom-fetch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // a stand-in coordd: it reads the hello and the one request and gives REPLY
        let coordd = |name: &str, reply: Value| -> Vec<String> {
            let f = dir.join(name);
            std::fs::write(&f, format!("{}\n", json!({"id": 1, "reply": reply}))).unwrap();
            vec!["sh".into(), "-c".into(), format!("cat >/dev/null; cat '{}'", f.display())]
        };
        let old = t("N-900", 900, json!([
            {"n": 1, "at": "2026-11-19T09:00:00Z", "by": "owner", "text": "the cloud of last week"},
            {"n": 2, "at": "2026-11-19T09:05:00Z", "by": "paper-research", "text": "closed: S5 goes to the Map"},
        ]));
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![project()];
        s.cur = 0;
        s.projects[0].cmd = coordd("given", json!({"ok": true, "out": serde_json::to_string(&old).unwrap()}));
        // the test's own folder has no projects.toml: its notice is not this test's (insight.rs)
        s.notice = None;
        assert_eq!(crate::find::held(&s, &s.projects[0], "N-900"), None, "not in the snapshot");
        zoom(&mut s, Depth::Structure, "N-900.p2");
        assert_eq!((s.tickets.shown, s.tickets.find.found, s.notice.clone()), (Some(900), Some((900, 2)), None), "fetched and open at p2");
        assert_eq!(crate::find::held(&s, &s.projects[0], "N-900"), Some(900), "kept as a find's fetch is");
        s.drop_sel(0, crate::Sel::Lens(Lens::threads_at("N-1104")), 0, crate::panes::Zone::Right);
        assert_eq!(s.panes[0], (0, crate::Sel::Lens(Lens::threads_at("N-900"))), "the first pane keeps it");
        s.focus_pane(0);
        assert_eq!(s.tickets.shown, Some(900), "and comes back to it");
        s.focus_pane(1);
        let mut rt = crate::view_cost::window(s);
        frames(&mut rt);
        assert!(rt.find("with-you-thread-N-900").is_some(), "drawn in the pane beside");
        assert_eq!(rt.state.tickets.shown, Some(1104), "N-1104 on the focused page");
        let s = &mut rt.state;
        s.projects[0].cmd = coordd("refused", json!({"ok": false, "out": "no ticket N-901"}));
        zoom(s, Depth::Structure, "N-901.p1");
        assert_eq!((s.notice.as_deref(), s.tickets.shown), (Some("no ticket N-901"), Some(1104)), "said, and nothing moves");
        s.notice = None;
        s.projects[0].cmd = vec!["false".into()];
        zoom(s, Depth::Structure, "N-1445.p3");
        assert_eq!((s.notice.as_deref(), s.tickets.shown, s.tickets.find.found), (None, Some(1445), Some((1445, 3))), "held: nothing asked");
    }

    /// A depth over a thread the snapshot does not hold (proto-reader N-1479 p82): N-900, closed last week, held as the
    /// Map's "Open its page" fetched it, with the route run his p2 opened, held as a run pressed in the Docket is. At
    /// Events on N-900.p2 the crumb names the thread by its label, as one the snapshot holds is named, and its run's
    /// step has its press down to the Wire; there "‹ Up to the thread" opens N-900 at p2, not the run's own ticket
    /// (`up_to`), and its lines offer no Comment, as on lead-64 (`comment_to`). Nothing is asked of coordd.
    #[test]
    fn a_fetched_threads_crumbs_name_it_and_lead_back_up_to_it() {
        type Rt = rune::runtime::Runtime<App>;
        let texts = |rt: &Rt| -> Vec<String> { rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect() };
        let run = "O-1099511630400";
        let mut old = t("N-900", 900, json!([
            {"n": 1, "at": "2026-11-19T09:00:00Z", "by": "paper-research", "text": "the cloud, as it stands"},
            {"n": 2, "at": "2026-11-19T09:05:00Z", "by": "owner", "text": "and S5 goes to the Map"},
        ]));
        old.title = "the cloud of last week".into();
        old.ran = serde_json::from_value(json!([{"id": run, "event": "owner_post", "from": "base", "line": 43, "post": 2, "opened_at": "2026-11-19T09:05:00Z", "closed_at": "2026-11-19T09:05:30Z", "said": "Send"}])).unwrap();
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = project();
        p.protocol = coord_types::RECORD_PROTOCOL;
        s.tickets.find.fetched.entry(p.id.clone()).or_default().insert(900, old);
        let rounds = json!([{"what": "step", "line": 1, "source": 0, "text": "r <- step watcher route(p)", "state": "done", "rounds": [{"step": 1, "to": "watcher", "given": "2026-11-19T09:05:01Z", "own": true}]}]);
        hold_run(&p, &mut s, run, "owner_post", "N-900", rounds);
        s.projects = vec![p];
        s.cur = 0;
        // the test's own folder has no projects.toml: its notice is not this test's (insight.rs)
        s.notice = None;
        let t = docket_machine::parse_ticket(run).unwrap();
        let step = || record(vec![(7, json!({"ev": "step", "at": "2026-11-19T09:05:01Z", "t": t, "step": 1, "worker": "watcher", "call": "route"}))]);
        // both pages held, so neither depth asks coordd (the test's project has no link)
        came(&mut s, 0, "N-900.p2".into(), None, true, step());
        came(&mut s, 0, format!("{run}.s1"), None, true, step());
        zoom(&mut s, Depth::Events, "N-900.p2");
        let mut rt = crate::view_cost::window(s);
        frames(&mut rt);
        let said = texts(&rt);
        assert!(said.iter().any(|x| x == "‹ the cloud of last week"), "named by its label, not its id: {said:?}");
        press(&mut rt, "wire-7");
        assert_eq!(rt.state.sel, crate::Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Wire, at: Some(format!("{run}.s1")) }), "its run's step, on the wire");
        let said = texts(&rt);
        assert!(said.iter().any(|x| x == "‹ the cloud of last week") && said.iter().any(|x| x == "› p2"), "up to N-900 at p2: {said:?}");
        // where a comment on its lines goes is as on lead-64 b5985c37: `up_to` reads N-900 fetched, `comment_to` finds
        // its thread among the snapshot's only, so a thread only fetched offers no Comment
        let node = format!("{run}.s1");
        assert_eq!((up_to(&rt.state, &rt.state.projects[0], &node), comment_to(&rt.state, &rt.state.projects[0], &node)), (("N-900", Some(2)), None), "up to N-900; no Comment on a thread only fetched");
        press(&mut rt, "lens-up");
        let s = &rt.state;
        assert_eq!((s.sel.clone(), s.tickets.shown, s.tickets.find.found, s.notice.clone()), (crate::Sel::Tickets, Some(900), Some((900, 2)), None), "back up at the post the run ran for, nothing asked");
    }

    /// N-1104's route run of tonight's p587, in the real run's shape, held as a run pressed in the Docket is held
    /// (find.rs `number`) and listed in N-1104's `ran` at p587. Step 1 is its watcher's (coordd's own). Step 2 is his
    /// `clarify`: never given, answered Send at 18:24:13Z, its answer the run's post 1.
    fn with_run(p: &mut crate::Proj, s: &mut App) {
        let run = "O-1099511630339";
        p.protocol = coord_types::RECORD_PROTOCOL;
        let his = p.live.tickets.get_mut(&1104).unwrap();
        his.posts.push(serde_json::from_value(json!({"n": 587, "at": "2026-11-20T18:23:17Z", "by": "owner", "text": "and the reset could be a special action"})).unwrap());
        his.ran = serde_json::from_value(json!([{"id": run, "event": "owner_post", "from": "base", "line": 43, "post": 587, "opened_at": "2026-11-20T18:23:17Z", "closed_at": "2026-11-20T18:24:13Z", "said": "Send"}])).unwrap();
        let rounds = json!([
            {"what": "step", "line": 1, "source": 0, "text": "r <- step watcher route(p)", "state": "done", "rounds": [{"step": 1, "to": "watcher", "given": "2026-11-20T18:23:18Z", "own": true}]},
            {"what": "step", "line": 12, "source": 0, "text": "c <- step owner clarify(p, r)", "state": "done", "rounds": [{"step": 2, "answered": "2026-11-20T18:24:13Z", "head": {"head": "Send"}, "post": 1}]},
        ]);
        hold_run(p, s, run, "owner_post", "N-1104", rounds);
    }

    /// Run ID of KIND under THREAD, done, with ROUNDS, held as a run pressed in the Docket is held (find.rs `number`).
    fn hold_run(p: &crate::Proj, s: &mut App, id: &str, kind: &str, thread: &str, rounds: Value) {
        let v: TicketView = serde_json::from_value(json!({"id": id, "n": docket_machine::parse_ticket(id).unwrap(), "kind": kind, "parent": thread, "under": thread, "opener": "coordd", "ctx": {}, "form": {}, "title": kind,
            "status": {"is": "done", "result": {"head": "Send"}}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": "2026-11-20T18:23:17Z", "given": [], "posts": [], "run": rounds})).unwrap();
        s.tickets.find.fetched.entry(p.id.clone()).or_default().insert(v.n, v);
    }

    /// A made-up run whose step goes to an agent: none of tonight's record does (its steps are the watcher's, the
    /// minder's, a box's and his). N-1444's p1 names docket-core2, and the run's step 1 asks docket-core2. It is given
    /// twice, and its answer comes after the second giving (`took`).
    fn with_agents_run(p: &mut crate::Proj, s: &mut App) {
        let run = "A-1099511630420";
        p.live.tickets.get_mut(&1444).unwrap().ran = serde_json::from_value(json!([{"id": run, "event": "agent_post", "from": "base", "line": 1, "post": 1, "opened_at": "2026-11-20T18:52:56Z", "closed_at": "2026-11-20T18:53:40Z", "said": "Send"}])).unwrap();
        let rounds = json!([{"what": "step", "line": 1, "source": 0, "text": "a <- step docket-core2 answer(p)", "state": "done",
            "rounds": [{"step": 1, "to": "docket-core2", "given": "2026-11-20T18:53:00Z", "answered": "2026-11-20T18:53:40Z", "took": {"to": "docket-core2", "at": "2026-11-20T18:53:20Z"}}]}]);
        hold_run(p, s, run, "agent_post", "N-1444", rounds);
    }

    /// A step's lines among its run's: the step, what its call was made from, its giving, the model, the answer. Only
    /// the run's own count, not those of a ticket it opened (consult-3 N-1446 p316). An answer goes to the open step it
    /// was given to (proto-reader p311).
    #[test]
    fn a_steps_lines_are_its_own_and_its_answer() {
        let parse = |text: String| text.lines().map(|l| serde_json::from_str(l).unwrap()).collect::<Vec<RecordLine>>();
        let at = |lines: &[RecordLine], node: &str| step_lines(lines, node).into_iter().map(|(l, _)| l.line).collect::<Vec<_>>();
        let lines = parse(run_lines());
        assert_eq!(at(&lines, "O-1099511630339.s1"), [101210, 101211, 101212, 101215, 101216], "not the step and answer of the ticket it opened");
        assert_eq!(at(&lines, "O-1099511630339.s2"), [101217, 101218, 101260], "his clarify and his Send");
        assert_eq!(at(&lines, "O-1099511630339.s3"), Vec::<u64>::new());
        assert_eq!(at(&lines, "N-1104.p587"), Vec::<u64>::new(), "a post is no step");
        // two steps open at once, answered the other way round, the second passed up to paper-research; a third lapses
        let run = 1099511630400u64;
        let fork = parse(record(vec![
            (1, json!({"ev": "step", "t": run, "step": 1, "worker": "watcher", "call": "route"})),
            (2, json!({"ev": "given", "t": run, "to": "watcher", "delivery": "O-1099511630400.s1"})),
            (3, json!({"ev": "step", "t": run, "step": 2, "worker": "minder", "call": "check"})),
            (4, json!({"ev": "given", "t": run, "to": "paper-research", "delivery": "O-1099511630400.s2"})),
            (5, json!({"ev": "answer", "t": run, "by": "paper-research", "head": {"head": "Done"}})),
            (6, json!({"ev": "answer", "t": run, "by": "watcher", "head": {"head": "Unclear"}})),
            (7, json!({"ev": "step", "t": run, "step": 3, "worker": "owner", "call": "look"})),
            (8, json!({"ev": "lapsed", "t": run, "step": 3, "head": {"head": "Go"}, "after": 30, "because": "time"})),
            // E2's held step passed up, as docket-core2's hand-passed.jsonl writes it (its lines 21, 22, 28, 29, 30): the
            // step's line again with its new worker, who answers it (proto-reader p426)
            (9, json!({"ev": "step", "t": run, "step": 4, "worker": "f201-1", "call": "join", "hold": true})),
            (10, json!({"ev": "step_args", "t": run, "step": 4, "args": {"label": "Brain fundamentals", "parent": "N-200", "sub": "N-202"}})),
            (11, json!({"ev": "passed_up", "t": run, "step": 4, "from": "f201-1", "to": "lead", "why": "the fork ended with the join held: f201-1 could not be started (roster fork failed)"})),
            (12, json!({"ev": "step", "t": run, "step": 4, "worker": "lead", "call": "join", "hold": true})),
            (13, json!({"ev": "step_args", "t": run, "step": 4, "args": {"label": "Brain fundamentals", "parent": "N-200", "sub": "N-202"}})),
            (14, json!({"ev": "answer", "t": run, "by": "lead", "head": {"head": "Settled"}})),
            // a policy's decide: asked and decided, no step line (the record's 94905, 94906; consult-3 p397)
            (15, json!({"ev": "asked", "t": run, "step": 5, "question": "decide", "role": "policy"})),
            (16, json!({"ev": "decided", "t": run, "step": 5, "question": "decide", "answer": "Clean"})),
            // lead's next step in the same run: its answer is that step's, step 4 being answered
            (17, json!({"ev": "step", "t": run, "step": 6, "worker": "lead", "call": "check"})),
            (18, json!({"ev": "answer", "t": run, "by": "lead", "head": {"head": "Done"}})),
        ]));
        assert_eq!(at(&fork, "O-1099511630400.s1"), [1, 2, 6]);
        assert_eq!(at(&fork, "O-1099511630400.s2"), [3, 4, 5]);
        assert_eq!(at(&fork, "O-1099511630400.s3"), [7, 8]);
        assert_eq!(at(&fork, "O-1099511630400.s4"), [9, 10, 11, 12, 13, 14], "passed up, and the new holder's answer");
        assert_eq!(at(&fork, "O-1099511630400.s5"), [15, 16], "what the policy was asked, and decided");
        assert_eq!(at(&fork, "O-1099511630400.s6"), [17, 18], "lead's next answer is its next step's");
    }

    /// What was decided on a step says what it was (consult-3 N-1446 p316). Only `model-for` is the model, with the
    /// models tried under `provenance`, and only it has a prompt part. Who fills a step and a policy's decide are said
    /// as they are, as is what was asked and a step that lapsed. These are the record's own shapes (lines 101215, 193,
    /// 194, 94905, 94906, 94915), and docket-core2's hand-passed.jsonl line 28 for a step passed up.
    #[test]
    fn what_was_decided_on_a_step_says_what_it_was() {
        let words = |v: Value| wire_words(&v).into_iter().map(|(part, title, body)| (part, title, body.is_some())).collect::<Vec<_>>();
        let model = json!({"ev": "decided", "t": 1099511630339u64, "step": 1, "question": "model-for", "answer": "gpt-6-luna", "provenance": {"attempts": [{"model": "gpt-6-luna"}], "chosen": "gpt-6-luna"}});
        assert_eq!(words(model.clone()), [("model", "The model: gpt-6-luna (tried gpt-6-luna)".to_string(), false), ("prompt", "Prompt not recorded; its sources are above.".into(), false)]);
        let mut sent = model;
        sent["prompt"] = json!("Whom does the owner's post answer?");
        assert_eq!(words(sent)[1], ("prompt", "The prompt as sent".to_string(), true), "where E1 is to write it");
        assert_eq!(words(json!({"ev": "decided", "t": 11, "step": 2, "question": "fills", "answer": "minder", "provenance": {"default": "coordd's own minder call"}})), [("decided", "Who fills it: minder".to_string(), false)]);
        assert_eq!(words(json!({"ev": "decided", "t": 1427, "step": 2, "question": "decide", "answer": "Clean", "provenance": {"attempts": [{"model": "Clean"}], "chosen": "Clean"}})), [("decided", "Decided: Clean".to_string(), false)]);
        assert_eq!(words(json!({"ev": "asked", "t": 1427, "step": 2, "question": "decide", "role": "policy"})), [("asked", "Asked of policy: decide".to_string(), false)]);
        assert_eq!(words(json!({"ev": "asked", "t": 11, "step": 2, "question": "fills", "role": "minder"})), [("asked", "Asked of minder: fills".to_string(), false)]);
        assert_eq!(words(json!({"ev": "lapsed", "t": 1427, "step": 5, "head": {"head": "Go"}, "after": 30, "because": "time"})), [("lapsed", "No answer in 30 s; went on as Go".to_string(), false)]);
        assert_eq!(words(json!({"ev": "passed_up", "t": 203, "step": 1, "from": "f201-1", "to": "lead", "why": "the fork ended with the join held: f201-1 could not be started (roster fork failed)"})), [("passed", "Passed up to lead: the fork ended with the join held: f201-1 could not be started (roster fork failed)".to_string(), false)]);
    }

    /// 0.3's Wire depth (his p599: "go down level llm calls on the wire"): from a step's line in Events, one press to
    /// what was sent and what came back. Step 1 is coordd's own (the watcher's route): its lines of the record, in words,
    /// the prompt said to be not in the record (consult-3 N-1446 p34); a press on a part opens its line in Record; the
    /// crumbs go back up to the thread at the post the run ran for, p587.
    #[test]
    fn a_step_coordd_answered_goes_down_to_its_lines_on_the_wire_and_back_up() {
        type Rt = rune::runtime::Runtime<App>;
        let texts = |rt: &Rt| -> Vec<String> { rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect() };
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = project();
        with_run(&mut p, &mut s);
        s.projects = vec![p];
        s.cur = 0;
        let node = "O-1099511630339.s1".to_string();
        s.show_sel(crate::Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Events, at: Some("N-1104.p587".into()) }));
        came(&mut s, 0, "N-1104.p587".into(), None, true, tonight());
        let mut rt = crate::view_cost::window(s);
        frames(&mut rt);
        assert!(rt.find("wire-101210").is_some() && rt.find("wire-101217").is_some() && rt.find("wire-101208").is_none(), "each step's line goes down to the wire, a post's does not");
        press(&mut rt, "wire-101210");
        assert_eq!(rt.state.sel, crate::Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Wire, at: Some(node.clone()) }));
        rt.update(|s, _| came(s, 0, "O-1099511630339.s1".into(), None, true, run_lines()));
        rt.invalidate();
        frames(&mut rt);
        for k in ["wire-step-101210", "wire-args-101211", "wire-given-101212", "wire-model-101215", "wire-prompt-101215", "wire-answer-101216"] {
            assert!(rt.find(k).is_some(), "{k} is drawn");
        }
        assert!(["wire-step-101213", "wire-answer-101214", "wire-step-101217", "event-101215"].iter().all(|k| rt.find(k).is_none()), "nothing of the ticket it opened, of step 2 or of the thread");
        let t = texts(&rt);
        for w in ["Step 1: route, by watcher", "The model: gpt-6-luna (tried gpt-6-luna)", "Prompt not recorded; its sources are above.", "watcher answered Unclear: No agent listed is the one his post answers.", "› O-1099511630339 · step 1", "› p587"] {
            assert!(t.iter().any(|x| x == w), "{w:?} in {t:?}");
        }
        press(&mut rt, "wire-args-101211");
        assert_eq!(rt.state.sel, crate::Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Record, at: Some(node.clone()) }));
        assert!(rt.state.records.whole.contains(&(0, 101211)), "its line whole");
        press(&mut rt, "lens-up");
        assert_eq!((rt.state.sel.clone(), rt.state.tickets.shown, rt.state.tickets.find.found), (crate::Sel::Tickets, Some(1104), Some((1104, 587))), "back up at the post the run ran for");
    }

    /// His step on the wire (step 2, his `clarify`) is its lines of the record: the step, what it was made from, his
    /// Send. It has no model, so no proxy log is asked for it (consult-3 N-1446 p316).
    #[test]
    fn his_step_on_the_wire_is_his_answer_and_asks_no_proxy_log() {
        type Rt = rune::runtime::Runtime<App>;
        let texts = |rt: &Rt| -> Vec<String> { rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect() };
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = project();
        with_run(&mut p, &mut s);
        s.projects = vec![p];
        s.cur = 0;
        let node = "O-1099511630339.s2";
        assert_eq!(span(round(&s, &s.projects[0], node).unwrap()), None, "never given: no agent's turn");
        zoom(&mut s, Depth::Wire, node);
        assert!(s.raw.logs.is_empty(), "no proxy log asked");
        came(&mut s, 0, node.into(), None, true, run_lines());
        let mut rt = crate::view_cost::window(s);
        frames(&mut rt);
        for k in ["wire-step-101217", "wire-args-101218", "wire-answer-101260"] {
            assert!(rt.find(k).is_some(), "{k} is drawn");
        }
        let t = texts(&rt);
        assert!(["Step 2: clarify, by owner", "owner answered Send"].iter().all(|w| t.iter().any(|x| x == w)), "{t:?}");
        assert!(!t.iter().any(|x| x.contains("prompt") || x.starts_with("Given to")), "no model, no giving: {t:?}");
        assert!(rt.state.raw.logs.is_empty() && rt.find("wire-turn-owner").is_none());
    }

    /// An agent's round on the wire (the made-up run's step to docket-core2) is its turn's requests from the giving its
    /// answer came after (`took`, 18:53:20, not the first giving at 18:53:00) to its answer, folded to their heads, as a
    /// post's "what the model was sent" draws them.
    #[test]
    fn an_agents_round_on_the_wire_is_its_turns_requests() {
        type Rt = rune::runtime::Runtime<App>;
        let texts = |rt: &Rt| -> Vec<String> { rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect() };
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = project();
        p.protocol = coord_types::RECORD_PROTOCOL;
        with_agents_run(&mut p, &mut s);
        s.projects = vec![p];
        s.cur = 0;
        let node = "A-1099511630420.s1";
        let n = |t: &str| crate::turn::nanos(t).unwrap();
        let (since, until) = (n("2026-11-20T18:53:20Z"), n("2026-11-20T18:53:40Z") + crate::turn::SLACK_NS);
        let r = round(&s, &s.projects[0], node).unwrap().clone();
        assert_eq!(span(&r), Some(("docket-core2".to_string(), since, until)), "from the giving it took");
        let early = ["══ 2026-11-20 19:53:05.000 · claude-opus-5-5 · status 200 ══", "[messages] 1", "[0 user text]\nan earlier giving"];
        let took = ["══ 2026-11-20 19:53:25.000 · claude-opus-5-5 · status 200 ══", "[messages] 1", "[0 user text]\nN-1444's p1", "[reply text]\nThe brain's first pass is right"];
        let lines: Vec<&str> = early.iter().chain(took.iter()).copied().collect();
        crate::raw::hold_turn(&mut s, 0, "docket-core2", node, since, until, &lines, vec![n("2026-11-20T18:53:05Z"), n("2026-11-20T18:53:25Z")]);
        zoom(&mut s, Depth::Wire, node);
        let mut rt = crate::view_cost::window(s);
        frames(&mut rt);
        assert!(rt.find("wire-turn-docket-core2").is_some());
        let t = texts(&rt);
        assert!(t.iter().any(|x| x.starts_with("Given to docket-core2 at ") && x.contains("answered at ")), "{t:?}");
        assert!(t.iter().any(|x| x.contains("19:53:25")) && !t.iter().any(|x| x.contains("19:53:05")), "the request after the giving it took, not the one before: {t:?}");
        assert!(t.iter().any(|x| x == "› p1") && t.iter().any(|x| x == "› A-1099511630420 · step 1"), "up to N-1444 at p1: {t:?}");
    }

    /// An agent's round pressed in a window with no link to its coordd says so, and nothing runs the bridge on the
    /// window's thread to wait up to 60 s (consult-3 N-1446 p386, docket-edge8 p345): no turn held, and no panic on
    /// test_app's empty command, which `link::once` would have run.
    #[test]
    fn an_agents_round_with_no_link_says_it_is_not_connected() {
        type Rt = rune::runtime::Runtime<App>;
        let texts = |rt: &Rt| -> Vec<String> { rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect() };
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = project();
        p.protocol = coord_types::RECORD_PROTOCOL;
        with_agents_run(&mut p, &mut s);
        s.projects = vec![p];
        s.cur = 0;
        assert!(s.projects[0].link.is_none());
        zoom(&mut s, Depth::Wire, "A-1099511630420.s1");
        let mut rt = crate::view_cost::window(s);
        frames(&mut rt);
        let t = texts(&rt);
        let said = crate::raw::cannot(&format!("{} is not connected", rt.state.projects[0].name));
        assert!(t.contains(&said), "{t:?}");
    }

    /// The Wire depth's stills, on made-up data only (`with_run`, `tonight`'s and `run_lines`' lines in small, a
    /// made-up run to an agent and its requests):
    /// - wire-own.png: Events at p587 on the left, each step's line with "Wire ›"; on the right step 1 on the wire,
    ///   the watcher's route call: what it was made from, the model, the prompt said to be not in the record, the answer.
    /// - wire-his.png: the same Events; on the right step 2 on the wire, his clarify and his Send.
    /// - wire-agent.png: the made-up run's step on the wire, given to docket-core2: its turn's request after the giving
    ///   it took, open.
    /// - wire-decide.png: a made-up run's step 1 on the wire, the policy's decide of who claims his p586: what it was
    ///   asked, then what it decided.
    /// - wire-record.png: the same Events on the left; on the right the Record depth at p587, its lines as written
    ///   (paper-research N-1446 p848: "Events · Record · Wire").
    ///
    /// `STILL_OUT=DIR yaaihh-tests lens::tests::wire_stills --ignored --exact`.
    #[test]
    #[ignore]
    fn wire_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn app() -> App {
            let (_held, mut s) = crate::view_cost::test_app();
            // the test's own folder has no projects.toml: none of that in a still
            s.notice = None;
            let mut p = project();
            with_run(&mut p, &mut s);
            with_agents_run(&mut p, &mut s);
            s.projects = vec![p];
            s.cur = 0;
            s
        }
        fn beside(step: &str) -> App {
            let mut s = app();
            came(&mut s, 0, "N-1104.p587".into(), None, true, tonight());
            came(&mut s, 0, step.into(), None, true, run_lines());
            let events = Lens { filter: Filter::Threads, depth: Depth::Events, at: Some("N-1104.p587".into()) };
            s.show_sel(crate::Sel::Lens(events.clone()));
            s.panes = vec![(0, crate::Sel::Lens(events)), (0, crate::Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Wire, at: Some(step.into()) }))];
            s
        }
        fn own() -> App {
            beside("O-1099511630339.s1")
        }
        fn his() -> App {
            beside("O-1099511630339.s2")
        }
        // the record beside its Events: one node's lines, so the Record pane reads what the Events pane filters
        fn record_beside() -> App {
            let mut s = app();
            let at = Some("N-1104.p587".to_string());
            came(&mut s, 0, "N-1104.p587".into(), None, true, tonight());
            let events = Lens { filter: Filter::Threads, depth: Depth::Events, at: at.clone() };
            s.show_sel(crate::Sel::Lens(events.clone()));
            s.panes = vec![(0, crate::Sel::Lens(events)), (0, crate::Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Record, at }))];
            s
        }
        fn agent() -> App {
            let mut s = app();
            let node = "A-1099511630420.s1";
            let n = |t: &str| crate::turn::nanos(t).unwrap();
            let lines = [
                "══ 2026-11-20 19:53:25.000 · claude-opus-5-5 · status 200 ══",
                "[system]\nYou are docket-core2, on the research project.",
                "[messages] 2",
                "[0 user text]\npaper-research on N-1444, #1: The first brain. @docket-core2, is its attention pass what the brain kind says?",
                "[1 assistant text]\nReading brain.kind's attend and elect first.",
                "[reply text]\nYes: attend then elect, as brain.kind writes them; one small point on the 12 h.",
            ];
            crate::raw::hold_turn(&mut s, 0, "docket-core2", node, n("2026-11-20T18:53:20Z"), n("2026-11-20T18:53:40Z") + crate::turn::SLACK_NS, &lines, vec![n("2026-11-20T18:53:25Z")]);
            crate::raw::unfold_turn(&mut s, 0, "docket-core2", node);
            zoom(&mut s, Depth::Wire, node);
            s
        }
        // a made-up run for his p586 whose step 1 is the policy's decide, in the record's shape (107000–107002,
        // owner_post's `taker <- decide claims(everyone, p.text)`): no step line, only what was asked and decided
        fn decide() -> App {
            let (_held, mut s) = crate::view_cost::test_app();
            s.notice = None;
            let mut p = project();
            with_run(&mut p, &mut s);
            with_agents_run(&mut p, &mut s);
            let run = "O-1099511630440";
            let t = docket_machine::parse_ticket(run).unwrap();
            p.live.tickets.get_mut(&1104).unwrap().ran.push(serde_json::from_value(json!({"id": run, "event": "owner_post", "from": "base", "line": 30, "post": 586, "opened_at": "2026-11-20T18:22:55Z", "closed_at": "2026-11-20T18:22:56Z", "said": "Send"})).unwrap());
            let rounds = json!([{"what": "step", "line": 30, "source": 0, "text": "taker <- decide claims(everyone, p.text)", "state": "done", "rounds": [{"step": 1}]}]);
            hold_run(&p, &mut s, run, "owner_post", "N-1104", rounds);
            s.projects = vec![p];
            s.cur = 0;
            let node = format!("{run}.s1");
            let why = "about the words' work, else free on the ticket, else the answerer, else anyone on the ticket, each one that can be woken";
            came(&mut s, 0, node.clone(), None, true, record(vec![
                (107001, json!({"ev": "asked", "at": "2026-11-20T18:22:55Z", "t": t, "step": 1, "question": "decide", "role": "policy"})),
                (107002, json!({"ev": "decided", "at": "2026-11-20T18:22:56Z", "t": t, "step": 1, "question": "decide", "answer": "paper-research", "provenance": {"chosen": "paper-research", "costs": [{"cost": 1, "why": why}]}})),
            ]));
            zoom(&mut s, Depth::Wire, &node);
            s
        }
        for (name, app) in [("wire-own", own as fn() -> App), ("wire-his", his), ("wire-agent", agent), ("wire-decide", decide), ("wire-record", record_beside)] {
            let script = out.join(format!("{name}.json"));
            std::fs::write(&script, "[]").unwrap();
            let snap = out.join(format!("{name}.png")).display().to_string();
            let script = script.display().to_string();
            let a: [&str; 8] = ["--snap", &snap, "--size", "1600x900", "--at", "1", "--script", &script];
            let spec = rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1600.0, 900.0), app, crate::view) };
            rune::App::new(spec).args(a.map(String::from).to_vec()).try_run().unwrap();
        }
    }

    /// A lens is kept as a page is: through `Sel` in ui.json and a window's layout, the depth and the node with it.
    #[test]
    fn a_lens_is_kept_as_a_page_is() {
        for f in [Filter::Threads, Filter::WithYou { agent: "paper-research".into() }, Filter::A2o { agent: "paper-research".into() }, Filter::A2a { agent: "paper-research".into() }, Filter::Find { words: "brain".into(), titles: true, agent: None }, Filter::Find { words: "brain".into(), titles: false, agent: Some("paper-research".into()) }, Filter::Docket] {
            let l = crate::Sel::Lens(Lens { filter: f, depth: Depth::Structure, at: Some("N-1104".into()) });
            let text = serde_json::to_string(&l).unwrap();
            assert_eq!(serde_json::from_str::<crate::Sel>(&text).unwrap(), l);
        }
        // a lens saved with no depth or node opens at the root of its structure
        let bare: crate::Sel = serde_json::from_value(json!({"Lens": {"filter": {"A2a": {"agent": "paper-research"}}}})).unwrap();
        assert_eq!(bare, crate::Sel::Lens(Lens::of(Filter::A2a { agent: "paper-research".into() })));
    }

    /// WORDS selected in the text keyed KEY, commented on as DEPTH's Comment at NODE makes it; the bytes selected.
    fn comment_on(rt: &mut rune::runtime::Runtime<App>, key: &'static str, words: &str, node: &'static str, depth: Depth) -> std::ops::Range<usize> {
        let t = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).and_then(|e| e.text).unwrap_or_else(|| panic!("{key} drawn"));
        let from = t.find(words).unwrap_or_else(|| panic!("{words:?} in {t:?}"));
        let range = from..from + words.len();
        let r = range.clone();
        rt.update(move |_, tx| tx.select_text(key, r));
        frames(rt);
        let sel = rt.selection().expect("selected");
        assert_eq!(sel.text(), words);
        rt.update(move |s, _| comment(s, 0, node, depth, &sel));
        // the frames as they are shown, none settled first (consult-3 N-1477 p25): none has text under the box
        crate::ask::anchor_tests::frames_shown(rt, &format!("the box on {key} opened"));
        // the box's motion settled: a box opened again comes from where the last one stood
        for _ in 0..40 {
            rt.frame(1.0 / 60.0);
        }
        range
    }

    /// The comment box on the lens's record, made up: words of line 101216 lit, the box right under them with his words
    /// in it, line 101217 on moved down under it (I-1476); narrow (1400) and wide (2200), the same.
    /// `STILL_OUT=DIR yaaihh-tests lens::tests::comment_stills --ignored --exact`.
    #[test]
    #[ignore]
    fn comment_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn record() -> App {
            let (_held, mut s) = crate::view_cost::test_app();
            let mut p = project();
            with_run(&mut p, &mut s);
            s.projects = vec![p];
            s.cur = 0;
            s.tickets.shown = Some(1104);
            s.show_sel(crate::Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Record, at: Some("N-1104.p587".into()) }));
            came(&mut s, 0, "N-1104.p587".into(), None, true, tonight());
            let line = s.records.got[&(0, "N-1104.p587".to_string())].as_ref().unwrap().iter().find(|l| l.line == 101216).unwrap().text.clone();
            let words = "No agent listed is the one his post answers.";
            let a = line.find(words).unwrap();
            let picked = crate::ask::Picked { target: "thread:N-1104".into(), source: "lens:N-1104.p587:Record".into(), label: "the record of N-1104.p587".into(),
                quote: words.into(), at: Vec::new(), pi: 0, anchor: None, block: None, pane: None, texts: vec![("record-N-1104.p587-101216-text".into(), a..a + words.len())] };
            crate::ask::write(&mut s, picked, "which agents were listed?\nand why was neither the one?");
            s.notice = None;
            s
        }
        let script = out.join("comment-record.json");
        std::fs::write(&script, "[]").unwrap();
        let script = script.display().to_string();
        for (w, size) in [(1400.0, "1400x900"), (2200.0, "2200x900")] {
            let snap = out.join(format!("comment-record-{size}.png")).display().to_string();
            let a: [&str; 8] = ["--snap", &snap, "--size", size, "--at", "1", "--script", &script];
            let spec = rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (w, 900.0), record, crate::view) };
            rune::App::new(spec).args(a.map(String::from).to_vec()).try_run().unwrap();
        }
    }

    /// I-1476 on the lens's lines (the owner, N-1104 p706; paper-research N-1477 p9, p18: covering none of his text,
    /// each surface narrow and wide): words of a record line selected and commented on are lit where they stand, and
    /// the box opens right under their line, the lines after it moved down under it; on the events, the same under the
    /// line's row in words. A lens's lines fill its pane, so
    /// neither size has a margin beside them: the box is in their room both times; no frame as it opens, nor as his
    /// five lines make it taller, has text under it. The comment goes to the node's thread, named by its depth and
    /// node. On the wire the same, in a part's body (what its call was made from), at a step whose run is no thread: it
    /// goes to the thread the page shows, the body's lines after the selected one moved down under the box.
    #[test]
    fn a_comment_on_the_records_and_the_wires_lines_opens_under_the_words_selected() {
        use crate::ask::anchor_tests::{NARROW, WIDE, drawn, in_room, lit, type_lines, under_box};
        let why = |rt: &rune::runtime::Runtime<App>, pane: &str| format!("box {:?}, room {:?}, lit {:?}, pane {:?}, under {:?}", drawn(rt, "cm-box"), drawn(rt, crate::ask::ROOM), lit(rt), drawn(rt, pane), under_box(rt));
        for size in [NARROW, WIDE] {
            let (_held, mut s) = crate::view_cost::test_app();
            let mut p = project();
            with_run(&mut p, &mut s);
            s.projects = vec![p];
            s.cur = 0;
            s.tickets.shown = Some(1104);
            s.show_sel(crate::Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Record, at: Some("N-1104.p587".into()) }));
            came(&mut s, 0, "N-1104.p587".into(), None, true, tonight());
            s.notice = None;
            let mut rt = crate::ask::anchor_tests::window(s, size);
            frames(&mut rt);
            let (key, pane) = ("record-N-1104.p587-101216-text", "lens-lines-0-N-1104.p587-Record");
            let at = comment_on(&mut rt, key, "No agent listed is the one his post answers.", "N-1104.p587", Depth::Record);
            let p = rt.state.notes.picked.clone().expect("a comment being written");
            assert_eq!((p.target.as_str(), p.label.as_str(), p.texts.clone()), ("thread:N-1104", "the record of N-1104.p587", vec![(key.to_string(), at)]));
            // line 101217, his clarify, the line after it
            in_room(&rt, pane, "clarify", &format!("{size:?}, the record: {}", why(&rt, pane)));
            type_lines(&mut rt, pane, "clarify");
            in_room(&rt, pane, "clarify", &format!("{size:?}, the record, his lines in: {}", why(&rt, pane)));
            rt.update(|s, _| crate::ask::unwrite(s));
            // the events: line 101216 in words, the box under its row, step 2's row (his clarify) after it moved down
            rt.update(|s, _| s.show_sel(crate::Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Events, at: Some("N-1104.p587".into()) })));
            rt.invalidate();
            frames(&mut rt);
            let (key, pane) = ("event-N-1104.p587-101216-text", "lens-lines-0-N-1104.p587-Events");
            let at = comment_on(&mut rt, key, "No agent listed is the one his post answers.", "N-1104.p587", Depth::Events);
            let p = rt.state.notes.picked.clone().expect("a comment being written");
            assert_eq!((p.target.as_str(), p.label.as_str(), p.texts.clone()), ("thread:N-1104", "the events of N-1104.p587", vec![(key.to_string(), at)]));
            in_room(&rt, pane, "step 2: clarify", &format!("{size:?}, the events: {}", why(&rt, pane)));
            rt.update(|s, _| crate::ask::unwrite(s));
            // on the wire: step 1 of the route run, its part "What its call was made from"
            let node = "O-1099511630339.s1";
            rt.update(move |s, _| {
                s.show_sel(crate::Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Wire, at: Some(node.into()) }));
                came(s, 0, node.into(), None, true, run_lines());
            });
            rt.invalidate();
            frames(&mut rt);
            let (key, pane) = ("wire-O-1099511630339.s1-args-101211-body", "lens-lines-0-O-1099511630339.s1-Wire");
            let at = comment_on(&mut rt, key, "consult-lead", node, Depth::Wire);
            let p = rt.state.notes.picked.clone().expect("a comment being written");
            assert_eq!((p.target.as_str(), p.label.as_str(), p.texts.clone()), ("thread:N-1104", "the wire of O-1099511630339.s1", vec![(key.to_string(), at)]));
            // the arguments' lines after "consult-lead"'s, its last `"to": []`
            in_room(&rt, pane, "\"to\": []", &format!("{size:?}, the wire: {}", why(&rt, pane)));
        }
    }

    /// consult-3 N-1477 p64, proto-window p65: a comment on a lens's lines goes to the thread its crumbs go up to, never
    /// to the one a page he left shows. N-1444 was left open on the Threads page (`tickets.shown`), and he is at the
    /// route run's step 1 on the wire: his comment on it goes to N-1104, which ran it. A second project holds threads
    /// of the same numbers; at a step of a run none of its threads ran, its lines offer no Comment, not its N-1444.
    #[test]
    fn a_comment_on_a_lens_goes_to_the_thread_its_crumbs_go_up_to_not_one_left_shown() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = project();
        with_run(&mut p, &mut s);
        let mut other = project();
        other.id = "other".into();
        s.projects = vec![p, other];
        s.cur = 0;
        s.tickets.shown = Some(1444);
        let node = "O-1099511630339.s1";
        s.show_sel(crate::Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Wire, at: Some(node.into()) }));
        came(&mut s, 0, node.into(), None, true, run_lines());
        s.notice = None;
        let mut rt = crate::ask::anchor_tests::window(s, crate::ask::anchor_tests::NARROW);
        frames(&mut rt);
        let key = "wire-O-1099511630339.s1-args-101211-body";
        let at = comment_on(&mut rt, key, "consult-lead", node, Depth::Wire);
        assert_eq!(rt.state.notes.picked.as_ref().map(|p| p.target.as_str()), Some("thread:N-1104"), "to the thread the run's crumbs go up to, not N-1444 left on the Threads page");
        // the second project's: a selection commented on as its pane's Comment would make it, at its step
        let step = "O-1099511630400.s1";
        assert_eq!(up_to(&rt.state, &rt.state.projects[1], step).0, "O-1099511630400", "no thread of the second project ran it");
        rt.update(|s, _| crate::ask::unwrite(s));
        frames(&mut rt);
        rt.update(move |_, tx| tx.select_text(key, at));
        frames(&mut rt);
        let sel = rt.selection().expect("selected again");
        rt.update(move |s, _| comment(s, 1, step, Depth::Wire, &sel));
        assert_eq!(rt.state.notes.picked.as_ref().map(|p| p.target.as_str()), None, "no Comment at it: not the second project's N-1444");
    }

    /// A lens's record drawn in two panes of the split, side by side on one node (proto-window N-1477 p29): words of a
    /// line selected in one pane and commented on are lit, their room drawn and the box stands in that pane alone, the
    /// focused one or the other; no text under the box, and the other pane's lines stay where they were.
    #[test]
    fn a_lens_drawn_in_two_panes_lights_one_passage_and_has_one_room_in_the_pane_he_selected_in() {
        use crate::ask::anchor_tests::{WIDE, select, settle, stands_in, text_at, window};
        const WORDS: &str = "No agent listed is the one his post answers.";
        for j in [0, 1] {
            let (_held, mut s) = crate::view_cost::test_app();
            let mut p = project();
            with_run(&mut p, &mut s);
            s.projects = vec![p];
            s.cur = 0;
            s.tickets.shown = Some(1104);
            let lens = Lens { filter: Filter::Threads, depth: Depth::Record, at: Some("N-1104.p587".into()) };
            s.show_sel(crate::Sel::Lens(lens.clone()));
            came(&mut s, 0, "N-1104.p587".into(), None, true, tonight());
            s.drop_sel(0, crate::Sel::Lens(lens), 0, crate::panes::Zone::Right);
            s.focus_pane(0);
            s.notice = None;
            let mut rt = window(s, WIDE);
            frames(&mut rt);
            // line 101217, his clarify, after the line commented on, in the other pane
            let other = |rt: &rune::runtime::Runtime<App>| text_at(rt, &crate::panes::key(1 - j), "clarify");
            let before = other(&rt);
            assert!(before.is_some(), "the record drawn in pane-{}", 1 - j);
            let sel = select(&mut rt, &crate::panes::key(j), WORDS, "").unwrap_or_else(|| panic!("the record's line drawn in pane-{j}"));
            rt.update(move |s, _| comment(s, 0, "N-1104.p587", Depth::Record, &sel));
            settle(&mut rt);
            stands_in(&rt, j);
            assert_eq!(other(&rt), before, "pane-{}'s lines where they were", 1 - j);
        }
    }
}
