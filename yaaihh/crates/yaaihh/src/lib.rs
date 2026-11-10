//! yaaihh: the owner's window onto the projects (vault designs/yaaihh-2026-11-07.md). A project is one coordd, in a box
//! or on the host; each is read through its coordd's stream (`coordc bridge`, link.rs) and drawn from the project
//! file's views.
//!
//!   yaaihh                     the window, with the projects of ~/.config/yaaihh/projects.toml (YAAIHH_CONFIG: another)
//!   yaaihh --background        the tray icon only (login autostart); closing the window keeps yaaihh running, a
//!                              second start opens the running one's window (tray.rs)
//!   yaaihh --snap F --scene S  a still of the first project's real state (scenes: overview, items, items-board,
//!                              item-detail, runner, runner-detail, commits, decisions, mail, questions, recognised, agent,
//!                              agents, agents-all, split, new-project, …: SCENES below; the agent scene shows
//!                              YAAIHH_AGENT, else the last agent added)
//!   YAAIHH_VIEW_DATA=ID=FILE   with --snap: view ID shows FILE's data instead of its source's (a source not yet live)
//!   YAAIHH_TERM_TEST=COORD [YAAIHH_TERM_AGENT=NAME] yaaihh --snap F --scene term
//!                              a still of agent NAME's (t1's) terminal on its agent page, through `coordc bridge` run
//!                              here on the coord folder COORD (its coordd at COORD/coordd.sock, NAME's holder at
//!                              COORD/pty/NAME.sock), as an unboxed project's pipe is
//!
//! projects.toml (projects.rs):
//!   agent_base = "/path/to/agent-base"     (default $AGENT_BASE)
//!   search = ["~/src"]                       (where a relative file path in a chat is looked for; default nowhere)
//!   [[project]]
//!   name = "My box"
//!   box  = "my-box"
//!   [[project]]
//!   name  = "Scratch"
//!   id    = "scratch"
//!   boxed = false

mod agents;
mod ask;
mod chat;
mod clerk;
mod domain;
pub mod ext;
mod link;
mod log;
mod live;
mod netdraw;
mod panes;
mod paths;
mod projects;
mod sidebar;
pub mod spend;
mod state;
mod term;
mod tray;
mod version;
mod views;
mod work;
mod zoom;

use coord_types::{Request, Transcript};
use link::{Incoming, Link};
use live::{Conv, Live};
use projects::{NewProject, ProjectConfig};
use rune::{AppSpec, Chrome, Cx, El, KeyEvent, TextEdit, Tx, text};
use rune_kit::shell::{self, PaneState, SidebarToggle, SplitView, Toolbar};
use rune_kit::{Button, ButtonVariant, Kit, SortDir, TableState};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// What the main pane shows.
#[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Sel {
    /// A view of the project file, by id; empty: the project's home.
    View(String),
    Mail,
    /// The project's questions to the owner (ask.rs).
    Questions,
    /// The agents page (agents.rs).
    Agents,
    /// The nets' runs (clerk.rs).
    Clerk,
    Agent(String),
    /// Extension NAME's page PAGE (ext.rs).
    Ext(String, String),
}

/// What a request was for, so its reply lands in the right place.
pub(crate) enum Pending {
    Page { agent: String, older: bool },
    Sent,
    /// Mail from a chat's composer: what was typed and the comments it carried, kept by the mail's id so Edit can
    /// put them back (chat.rs `queued`).
    Mailed { typed: String, comments: Vec<ask::Comment> },
    /// A queued mail taken back (`Unsend`); EDIT: its text goes back into AGENT's composer.
    Unsent { id: String, agent: String, edit: bool },
    /// A terminal request (term.rs): a refusal is shown in its view.
    Term { agent: String },
    /// A request extension EXT made ([`Proj::ask`]), with the data it asked with: the reply goes to it.
    Ext { ext: &'static str, data: Box<dyn std::any::Any> },
    /// A decision asked for its why (ask.rs `toggle_why`), by its key in `Notes::why`.
    Decision { key: String },
    /// The nets' files and nets (clerk.rs `load`).
    Nets,
    /// A net tried (clerk.rs `try_it`): EDITED, or the current version.
    NetTried { edited: bool },
    /// An edit saved, or refused (clerk.rs `save`).
    NetSaved,
    /// A run's part marked (clerk.rs `mark`): the run comes back by the live stream; the nets view is read again
    /// for the counts.
    NetMarked,
    /// The marked runs replayed (clerk.rs `replay`).
    NetReplayed,
    /// File PATH's text TEXT drawn, or its parse error (clerk.rs `ask_drawing`).
    NetGraph { path: String, text: String },
    /// A chat's image (`chat::image_key`).
    Image { key: String },
    /// The commits before the oldest held (domain.rs "Show older").
    Older,
}

pub struct Proj {
    pub name: String,
    /// The box's name, or the host coordd's.
    pub id: String,
    cmd: Vec<String>,
    link: Option<Link>,
    pub live: Live,
    /// Why the pipe is down, while it is.
    pub down: Option<String>,
    /// In a box (`box.sh`), else on the host (`host-coordd`): how it is brought up (projects.rs `bring_up`).
    pub boxed: bool,
    /// Its bring-up running (at yaaihh's start, or the owner's Start): what it ended with comes here.
    pub starting: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    /// A fork of one of its agents running (the agent page's Fork): the new agent's name or why it failed.
    pub forking: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    /// Why the last bring-up failed.
    pub start_error: Option<String>,
    pub convs: HashMap<String, Conv>,
    pending: HashMap<u64, Pending>,
    /// The `commits` view's data, read once per change, and after it the older ones the page asked for.
    pub commits: Vec<Commit>,
    /// The older commits asked for (`Request::Older`), oldest last; FROM: where the commits held begin (the next
    /// ask's before); PENDING: an ask is out.
    pub older: Vec<Commit>,
    pub older_from: Option<String>,
    pub older_pending: bool,
}

pub struct App {
    pub projects: Vec<Proj>,
    /// The agent-base checkout (box.sh, host-coordd) and the config file (projects.rs).
    pub base: String,
    pub config_path: String,
    /// Wakes the window from other threads (the pipes, a project's start); none headless.
    pub wake: Option<rune::Waker>,
    /// The agents page: its tab (0 Active, 1 All agents), All agents' table and the sort it shows.
    pub agents_tab: usize,
    pub all_table: TableState,
    pub all_sort: Option<(String, SortDir)>,
    /// The agent shown was opened from the grid: its toolbar leads back there.
    pub back_to_grid: bool,
    /// The model price table the agents' cost is read from: an extension's ([`ext::Extension::prices`]), read again
    /// while the agents page shows, at most once a minute; else the bundled catalog.
    pub prices: Result<prices::PriceTable, String>,
    prices_at: Instant,
    pub new_project: NewProject,
    /// The extensions' states (ext.rs).
    pub ext: ext::States,
    pub cur: usize,
    sidebar: PaneState,
    pub sel: Sel,
    /// The mail being written to each agent, by project and agent (chat.rs: each chat pane has its own).
    pub composers: HashMap<(usize, String), TextEdit>,
    /// What the owner's mails from a composer were made of (the typed text, the comments), by project and mail id,
    /// so a queued one that is edited goes back into the composer as it was written.
    pub drafts: HashMap<(usize, String), (String, Vec<ask::Comment>)>,
    /// The split's other panes (the focused one is `cur` and `sel`), each a project and what it shows: at most three,
    /// so four in all (view; owner 7 Nov night, "side by side, split into 4, like VS Code").
    pub panes: Vec<(usize, Sel)>,
    /// The focused pane's place in `panes` (panes.rs).
    pub focus: usize,
    /// The pinned chats (sidebar.rs).
    pub pins: sidebar::Pins,
    /// An agent being dragged from the sidebar onto the split, and where it would land if released now (the view
    /// works that out from the layout, panes.rs `drop_zone`; a cell, since the view only reads the state).
    pub dragging: Option<panes::AgentDrag>,
    pub drop_at: std::cell::Cell<Option<(usize, panes::Zone)>>,
    pub show_closed: bool,
    pub open_mail: Option<String>,
    /// Entries whose long text is shown whole.
    pub expanded: HashSet<String>,
    /// The chats' images by `chat::image_key`, asked of coordd as their entries arrive (chat.rs `ask_images`).
    pub images: HashMap<String, chat::Img>,
    /// The chat image shown over the whole window (`chat::image_key`), opened by clicking it; a click or Escape closes it.
    pub viewing: Option<String>,
    /// The pointer is over an element that takes Ctrl+wheel itself (the usage chart's zoom), so the window does not zoom.
    pub wheel_taken: bool,
    /// Decodes them off the frame; set up with the window's waker (a headless scene makes its own).
    pub image_cache: Option<rune::ImageCache>,
    /// The last thing that went wrong (a refused send, say).
    pub notice: Option<String>,
    /// A commit whose place in its agent's session is being looked for (earlier pages are read until it is found).
    pub seek: Option<Commit>,
    /// The entry a commit was found at, marked in the conversation.
    pub found: Option<String>,
    /// The agents' terminals attached (term.rs).
    pub term: term::Terms,
    /// Comments on selected text and answers being written (ask.rs).
    pub notes: ask::Notes,
    /// The nets' runs seen on their page (clerk.rs).
    pub clerk_seen: clerk::Seen,
    /// The Clerk page's nets: shown, edited and tried.
    pub clerk_ed: clerk::Editor,
    /// The domain views' detail panel, filters and layouts (domain.rs).
    pub domain: domain::State,
    /// The window's state as last written to ui.json (state.rs).
    keeper: state::Keeper,
    /// The whole window's zoom (zoom.rs).
    pub zoom: zoom::Zoom,
}

/// An agent's commit, from the project's `commits` view (its Agent, Session and Item trailers).
#[derive(Clone, Debug, Deserialize)]
pub struct Commit {
    pub hash: String,
    pub at: String,
    pub subject: String,
    pub agent: String,
    #[serde(default)]
    pub session: String,
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default)]
    pub refs: String,
    #[serde(default)]
    pub repo: String,
    /// landed, in the runner or not yet.
    #[serde(default)]
    pub state: String,
}

pub(crate) fn init() -> App {
    let path = projects::config_path();
    let (cfg, notice) = match std::fs::read_to_string(&path) {
        Ok(t) => match toml::from_str::<projects::Config>(&t) {
            Ok(c) => (c, None),
            Err(e) => (projects::Config::default(), Some(format!("{path}: {e}"))),
        },
        Err(e) => (projects::Config::default(), Some(format!("{path}: {e}"))),
    };
    let base = cfg.agent_base.or_else(|| std::env::var("AGENT_BASE").ok()).unwrap_or_default();
    let _ = paths::SEARCH.set(cfg.search.clone());
    let projects = cfg.project.iter().map(|p| Proj::new(p, projects::pipe(&base, p))).collect();
    let (all_table, all_sort) = agents::table_state();
    let mut s = App {
        projects,
        base,
        config_path: path,
        pins: sidebar::Pins::load(),
        wake: None,
        agents_tab: 0,
        all_table,
        all_sort,
        back_to_grid: false,
        images: HashMap::new(),
        viewing: None,
        wheel_taken: false,
        image_cache: None,
        prices: prices::PriceTable::bundled(),
        prices_at: Instant::now(),
        new_project: NewProject::new(),
        ext: ext::States::default(),
        cur: 0,
        sidebar: PaneState::new(260.0).range(180.0, 520.0),
        sel: Sel::View(String::new()),
        composers: HashMap::new(),
        drafts: HashMap::new(),
        panes: Vec::new(),
        dragging: None,
        drop_at: std::cell::Cell::new(None),
        focus: 0,
        show_closed: false,
        open_mail: None,
        expanded: HashSet::new(),
        notice,
        seek: None,
        found: None,
        term: term::Terms::default(),
        notes: ask::Notes::load(),
        clerk_seen: clerk::Seen::default(),
        clerk_ed: clerk::Editor::default(),
        domain: domain::State::default(),
        keeper: state::Keeper::default(),
        zoom: zoom::Zoom::default(),
    };
    for e in ext::all() {
        (e.init)(&mut s);
    }
    s.read_prices();
    if !rune::is_headless() {
        state::restore(&mut s);
    }
    s
}

fn setup(s: &mut App, wake: rune::Waker) {
    if rune::is_headless() {
        return; // the scenes read the box once, synchronously
    }
    s.image_cache = Some(rune::ImageCache::new(wake.clone()));
    s.wake = Some(wake.clone());
    // each project brought up (after a reboot nothing else starts a box's steps, its coordd or its agents, 9 Nov)
    for i in 0..s.projects.len() {
        projects::bring_up(s, i);
    }
    for p in &mut s.projects {
        p.link = Some(Link::start(p.cmd.clone(), wake.clone()));
    }
    for e in ext::all() {
        (e.setup)(s, wake.clone());
    }
}

fn poll(s: &mut App, _: f64, tx: &mut Tx) -> bool {
    let began = Instant::now();
    let mut changed = false;
    for i in 0..s.projects.len() {
        let incoming = s.projects[i].link.as_ref().map(Link::drain).unwrap_or_default();
        for m in incoming {
            changed = true;
            s.take(i, m);
        }
    }
    changed |= s.new_project_done();
    changed |= projects::brought_up(s);
    // the threads kept in the Questions list go when the owner leaves the page (ask.rs `Notes::kept`)
    if s.sel != Sel::Questions && !s.notes.kept.is_empty() {
        s.notes.kept.clear();
    }
    // the agents page: prices as the usage page last saved them, the tiles' chats, and the "ago" times
    if s.sel == Sel::Agents {
        if s.prices_at.elapsed() > Duration::from_secs(60) {
            changed |= s.read_prices();
        }
        if s.agents_tab == 0 {
            s.follow_tiles();
        }
        tx.wake_after(Duration::from_secs(10));
    }
    for e in ext::all() {
        changed |= (e.poll)(s, tx);
    }
    changed |= term::follow_page(s);
    if let Some(key) = s.seek_step() {
        tx.scroll_into_view(key.clone());
        s.found = Some(key);
        changed = true;
    }
    s.zoom.show(tx);
    state::keep(s, tx);
    if began.elapsed() > Duration::from_millis(100) {
        log::line(&format!("slow update: {} ms", began.elapsed().as_millis()));
    }
    changed
}

impl App {
    pub fn proj(&self) -> Option<&Proj> {
        self.projects.get(self.cur)
    }

    /// The price table again from the extension that keeps one (the first), else the bundled catalog; whether one was
    /// read.
    fn read_prices(&mut self) -> bool {
        self.prices_at = Instant::now();
        let Some(f) = ext::all().iter().find_map(|e| e.prices) else { return false };
        self.prices = f(self);
        true
    }

    fn take(&mut self, i: usize, m: Incoming) {
        let p = &mut self.projects[i];
        match m {
            Incoming::Event(ev) => {
                p.down = None;
                let snapshot = matches!(ev.body, coord_types::Body::Snapshot(_));
                // an agent's chat is read on only when its transcript grew (its latest activity moved), not for every
                // change of its view (tokens, memory, state): those came twice a second while agents worked, each a
                // page asked of coordd and the window rebuilt (9 Nov, m151 fix 4)
                let active_before = match &ev.body {
                    coord_types::Body::Agent(a) => p.live.agents.iter().find(|x| x.name == a.name).map(|x| x.last_active.clone()),
                    _ => None,
                };
                let agent = p.apply(ev);
                if snapshot {
                    // a new stream: what was read of each session may be behind; read on from where it ends
                    let names: Vec<String> = p.convs.keys().cloned().collect();
                    names.iter().for_each(|n| p.read_newer(n));
                }
                let grew = |a: &String| p.live.agents.iter().find(|x| &x.name == a).map(|x| x.last_active.clone()) != active_before;
                if let Some(a) = agent.filter(|a| p.convs.contains_key(a) && grew(a)) {
                    p.read_newer(&a);
                }
                if snapshot {
                    term::relink(self, i);
                }
                if i == self.cur && self.sel == Sel::Clerk {
                    clerk::mark_seen(self);
                }
                self.pins.follow(&self.projects[i].id, &self.projects[i].live.agents);
            }
            Incoming::Pty(ev) => term::take(self, i, ev),
            Incoming::Reply(id, r) => match p.pending.remove(&id) {
                Some(Pending::Page { agent, older }) => {
                    let c = p.convs.entry(agent.clone()).or_default();
                    if older { c.older_pending = false } else { c.newer_pending = false }
                    match serde_json::from_str::<Transcript>(&r.out) {
                        Ok(page) if r.ok => {
                            let wanted = chat::image_requests(&agent, &page, &self.images);
                            if !c.merge(page, older) {
                                p.read_newer(&agent);
                            }
                            for (key, req) in wanted {
                                if let Some(id) = p.link.as_mut().and_then(|l| l.ask(&req)) {
                                    p.pending.insert(id, Pending::Image { key: key.clone() });
                                    self.images.insert(key, chat::Img::Asked);
                                }
                            }
                        }
                        _ => c.refused = Some(r.out),
                    }
                }
                Some(Pending::Sent) if !r.ok => self.notice = Some(r.out),
                Some(Pending::Mailed { typed, comments }) => match r.out.split(';').next().filter(|_| r.ok) {
                    Some(id) => {
                        self.drafts.insert((i, id.trim().to_string()), (typed, comments));
                    }
                    None => self.notice = Some(r.out),
                },
                Some(Pending::Unsent { id, agent, edit }) => {
                    if !r.ok {
                        self.notice = Some(r.out);
                    } else if edit {
                        let (typed, comments) = self.drafts.remove(&(i, id.clone())).unwrap_or_else(|| {
                            let body = p.live.mail.get(&id).map(|m| m.body.clone()).unwrap_or_default();
                            (body, Vec::new())
                        });
                        let c = self.composers.entry((i, agent.clone())).or_insert_with(|| TextEdit::new(""));
                        // what is in the box already stays, after the mail taken back
                        let text = if c.text.trim().is_empty() { typed } else { format!("{typed}\n\n{}", c.text) };
                        let mut t = TextEdit::new(text);
                        t.focused = true;
                        *c = t;
                        if !comments.is_empty() {
                            let mut all = comments;
                            all.extend(self.notes.comments.remove(&agent).unwrap_or_default());
                            self.notes.comments.insert(agent, all);
                        }
                    } else {
                        self.drafts.remove(&(i, id));
                    }
                }
                Some(Pending::Term { agent }) if !r.ok => term::refused(&mut self.term, i, &agent, r.out),
                Some(Pending::Nets) => self.clerk_ed.view = Some(if r.ok { serde_json::from_str(&r.out).map_err(|e| format!("the nets: {e}")) } else { Err(r.out) }),
                Some(Pending::NetTried { edited }) => {
                    self.clerk_ed.tried[usize::from(!edited)] = Some(if r.ok { serde_json::from_str(&r.out).map_err(|e| format!("the run: {e}")) } else { Err(r.out) })
                }
                Some(Pending::Image { key }) => {
                    let img = chat::take_image(self.image_cache.as_ref(), r.ok, &r.out);
                    self.images.insert(key, img);
                }
                Some(Pending::NetGraph { path, text }) => {
                    // a reply for a text since edited is dropped: the newer one is on its way
                    if self.clerk_ed.drawn.get(&path) == Some(&text) {
                        let slot = self.clerk_ed.graphs.entry(path).or_default();
                        match if r.ok { serde_json::from_str(&r.out).map_err(|e| format!("the drawing: {e}")) } else { Err(r.out) } {
                            Ok(g) => *slot = (Some(g), None),
                            Err(e) => slot.1 = Some(e),
                        }
                    }
                }
                Some(Pending::NetMarked) => {
                    self.clerk_ed.mark_error = (!r.ok).then_some(r.out);
                    clerk::load(self);
                }
                Some(Pending::NetReplayed) => {
                    self.clerk_ed.replayed = Some(Some(if r.ok { serde_json::from_str(&r.out).map_err(|e| format!("the replay: {e}")) } else { Err(r.out) }))
                }
                Some(Pending::NetSaved) => {
                    self.clerk_ed.saved.push(if r.ok { Ok(r.out) } else { Err(r.out) });
                    if r.ok {
                        clerk::load(self);
                    }
                }
                Some(Pending::Decision { key }) => {
                    let d = if r.ok { serde_json::from_str(&r.out).map_err(|e| format!("the decision: {e}")) } else { Err(r.out) };
                    if let Some(slot) = self.notes.why.get_mut(&key) {
                        *slot = Some(d);
                    }
                }
                Some(Pending::Older) => {
                    p.older_pending = false;
                    let page = if r.ok { serde_json::from_str::<serde_json::Value>(&r.out).map_err(|e| format!("older commits: {e}")) } else { Err(r.out) };
                    match page {
                        Ok(v) => {
                            p.older.extend(v.get("commits").and_then(|c| serde_json::from_value::<Vec<Commit>>(c.clone()).ok()).unwrap_or_default());
                            p.older_from = v.get("from").and_then(serde_json::Value::as_str).map(String::from);
                            p.join_older();
                        }
                        Err(e) => self.notice = Some(e),
                    }
                }
                Some(Pending::Ext { ext, data }) => {
                    if let Some(e) = ext::get(ext) {
                        (e.reply)(self, i, data, if r.ok { Ok(r.out) } else { Err(r.out) });
                    }
                }
                _ => {}
            },
            Incoming::Down(why) => {
                p.down = Some(why);
                p.pending.clear();
                p.convs.values_mut().for_each(|c| (c.older_pending, c.newer_pending) = (false, false));
                term::down(&mut self.term, i);
            }
        }
    }

    /// Show agent NAME, reading its session the first time.
    pub fn select_agent(&mut self, name: &str) {
        self.sel = Sel::Agent(name.into());
        if let Some(p) = self.projects.get_mut(self.cur) {
            if !p.convs.get(name).is_some_and(|c| c.loaded || c.newer_pending) {
                p.read_newer(name);
            }
        }
    }

    /// Show COMMIT's agent at the commit: the entry that names its hash (a `git commit` and its result do), else the
    /// first entry after the commit's time.
    pub fn jump(&mut self, c: Commit) {
        let Some(p) = self.proj() else { return };
        let Some(a) = p.live.agents.iter().find(|a| a.name == c.agent) else {
            self.notice = Some(format!("{} is not an agent of this box", c.agent));
            return;
        };
        if !c.session.is_empty() && !a.transcript.as_deref().is_some_and(|t| t.contains(&c.session)) {
            self.notice = Some(format!("{} was made in an earlier session of {} (session {})", &c.hash[..9.min(c.hash.len())], c.agent, c.session));
        }
        self.found = None;
        self.select_agent(&c.agent.clone());
        self.seek = Some(c);
    }

    /// One step of a seek: the key once the commit's entry is held; else read an earlier page if there is one.
    fn seek_step(&mut self) -> Option<String> {
        let c = self.seek.clone()?;
        let p = self.projects.get_mut(self.cur)?;
        let conv = p.convs.get(&c.agent)?;
        if !conv.loaded {
            return None;
        }
        let short = &c.hash[..10.min(c.hash.len())];
        let by_hash = conv.entries.iter().position(|e| e.text.contains(short) || e.text.contains(&c.hash[..7.min(c.hash.len())]));
        let at = match by_hash {
            Some(i) => Some(i),
            None if conv.start > 0 => {
                if !conv.older_pending {
                    p.read_older(&c.agent);
                }
                return None;
            }
            None => conv.entries.iter().position(|e| e.at >= c.at).or(conv.entries.len().checked_sub(1)),
        };
        self.seek = None;
        let i = at?;
        // a call is folded into its run: open the run and the call (work.rs)
        let open = work::reveal(conv, i, |t| ask::asked(p, t).is_some());
        let key = conv.keys[i].clone();
        self.expanded.extend(open);
        Some(key)
    }

    /// The project's agent commits (its `commits` view), newest first.
    pub fn commits(&self) -> &[Commit] {
        self.proj().map_or(&[], |p| &p.commits)
    }

    /// Send the text written to agent NAME of project PROJECT, after the comments made on its chat; ESC stops its
    /// turn first.
    /// The composer's mail to NAME (and those it tags): held until its turn ends; SOON in after its next tool call; ESC
    /// stops its turn first.
    pub fn send(&mut self, project: usize, name: &str, soon: bool, esc: bool) {
        let key = (project, name.to_string());
        let written = self.composers.get(&key).map(|t| t.text.clone()).unwrap_or_default();
        let comments = self.notes.comments.get(name).cloned().unwrap_or_default();
        if written.trim().is_empty() && comments.is_empty() {
            return;
        }
        // the chat's agent, and each agent tagged @NAME in the text or its comments (owner m118)
        let notes: Vec<&str> = std::iter::once(written.as_str()).chain(comments.iter().map(|c| c.note.as_str())).collect();
        let mut to = vec![name.to_string()];
        match self.projects.get(project).map(|p| ask::tagged(p, &notes)) {
            Some(Ok(names)) => to.extend(names.into_iter().filter(|n| n != name)),
            Some(Err(e)) => {
                self.notice = Some(e);
                return;
            }
            None => return,
        }
        let (body, quotes) = ask::compose(&comments, &written);
        let req = Request::Msg { to, body, esc, soon, as_host: None, re: None, quotes };
        if self.request_in(project, req, Pending::Mailed { typed: written, comments }) {
            self.composers.remove(&key);
            self.notes.comments.remove(name);
        }
    }

    /// Take queued mail ID to AGENT back (chat.rs `queued`); EDIT puts it back into the composer.
    pub fn unsend(&mut self, project: usize, agent: &str, id: &str, edit: bool) {
        self.request_in(project, Request::Unsend { id: id.into() }, Pending::Unsent { id: id.into(), agent: agent.into(), edit });
    }

    /// Deliver queued mail ID now: SOON after its recipients' next tool calls, else stopping their turns.
    pub fn send_now(&mut self, project: usize, id: &str, soon: bool) {
        self.request_in(project, Request::Now { id: id.into(), soon }, Pending::Sent);
    }

    /// Send a mail request through the current project's pipe; false (and a notice) when it is down.
    pub fn mail(&mut self, req: Request) -> bool {
        self.mail_in(self.cur, req)
    }

    /// Send a mail request through project PROJECT's pipe; false (and a notice) when it is down.
    pub fn mail_in(&mut self, project: usize, req: Request) -> bool {
        self.request_in(project, req, Pending::Sent)
    }

    /// Send REQ through project PROJECT's pipe, its reply going to WHAT; false (and a notice) when the pipe is down.
    pub(crate) fn request_in(&mut self, project: usize, req: Request, what: Pending) -> bool {
        let Some(p) = self.projects.get_mut(project) else { return false };
        match p.link.as_mut().and_then(|l| l.ask(&req)) {
            Some(id) => {
                p.pending.insert(id, what);
                self.notice = None;
                true
            }
            None => {
                self.notice = Some("not sent: the box is not connected".into());
                false
            }
        }
    }
}

impl Proj {
    pub fn new(cfg: &ProjectConfig, cmd: Vec<String>) -> Proj {
        Proj {
            name: cfg.name.clone(),
            id: cfg.id.clone(),
            cmd,
            link: None,
            live: Live::default(),
            down: Some("connecting".into()),
            boxed: cfg.boxed,
            starting: None,
            forking: None,
            start_error: None,
            convs: HashMap::new(),
            pending: HashMap::new(),
            commits: Vec::new(),
            older: Vec::new(),
            older_from: None,
            older_pending: false,
        }
    }

    /// Apply EV to the live state, and read the commits again when their view changed.
    fn apply(&mut self, ev: coord_types::Event) -> Option<String> {
        let commits = match &ev.body {
            coord_types::Body::Snapshot(_) => true,
            coord_types::Body::View(v) => v.id == "commits",
            _ => false,
        };
        let agent = self.live.apply(ev);
        if commits {
            let data = self.live.views.get("commits").and_then(|v| v.data.as_ref());
            if self.older.is_empty() {
                self.older_from = data.and_then(|d| d.get("from")?.as_str()).map(String::from);
            }
            self.commits = data.and_then(|d| d.get("commits")).and_then(|c| serde_json::from_value(c.clone()).ok()).unwrap_or_default();
            self.join_older();
        }
        agent
    }

    /// The commits held: the view's, then the older ones asked for (none twice).
    fn join_older(&mut self) {
        let seen: HashSet<String> = self.commits.iter().map(|c| c.hash.clone()).collect();
        let older: Vec<Commit> = self.older.iter().filter(|c| !seen.contains(&c.hash)).cloned().collect();
        self.commits.extend(older);
    }

    /// Ask this project's coordd REQ for extension EXT, with DATA kept for its reply ([`ext::Extension::reply`]);
    /// false when the pipe is down.
    pub fn ask(&mut self, req: &Request, ext: &'static str, data: Box<dyn std::any::Any>) -> bool {
        let Some(id) = self.link.as_mut().and_then(|l| l.ask(req)) else { return false };
        self.pending.insert(id, Pending::Ext { ext, data });
        true
    }

    /// Ask this project's coordd REQ once and wait for the reply (Err: coordd's refusal, or no reply): for a scene,
    /// which reads before its one frame.
    pub fn ask_once(&self, req: &Request) -> Result<String, String> {
        for m in link::once(&self.cmd, std::slice::from_ref(req)) {
            if let Incoming::Reply(_, r) = m {
                return if r.ok { Ok(r.out) } else { Err(r.out) };
            }
        }
        Err(format!("{}: no reply", self.name))
    }

    /// Ask coordd for the week of commits before those held.
    pub fn ask_older(&mut self) {
        let Some(before) = self.older_from.clone().filter(|_| !self.older_pending) else { return };
        if let Some(id) = self.link.as_mut().and_then(|l| l.ask(&Request::Older { id: "commits".into(), before })) {
            self.pending.insert(id, Pending::Older);
            self.older_pending = true;
        }
    }

    /// Ask for the part of NAME's session after what is held (all of its last part, the first time).
    fn read_newer(&mut self, name: &str) {
        let c = self.convs.entry(name.into()).or_default();
        if c.newer_pending {
            return;
        }
        let req = Request::Transcript { name: name.into(), before: None, after: c.loaded.then_some(c.next) };
        if let Some(id) = self.link.as_mut().and_then(|l| l.ask(&req)) {
            c.newer_pending = true;
            self.pending.insert(id, Pending::Page { agent: name.into(), older: false });
        }
    }

    /// Ask for the page before what is held of NAME's session.
    pub fn read_older(&mut self, name: &str) {
        let Some(c) = self.convs.get_mut(name) else { return };
        if c.older_pending || !c.loaded || c.start == 0 {
            return;
        }
        let req = Request::Transcript { name: name.into(), before: Some(c.start), after: None };
        if let Some(id) = self.link.as_mut().and_then(|l| l.ask(&req)) {
            c.older_pending = true;
            self.pending.insert(id, Pending::Page { agent: name.into(), older: true });
        }
    }
}

fn view(s: &App, cx: &mut Cx<App>) -> El {
    let k = Kit::neutral(cx.system_dark());
    let (title, main) = panes::page(s, &k);
    let main = panes::area(s, main, &k);
    let mut bar = Toolbar::new().title(title);
    if let Sel::Agent(name) = &s.sel {
        let name = name.clone();
        let forking = s.proj().is_some_and(|p| p.forking.is_some());
        bar = bar.trailing(
            Button::new(if forking { "Forking…" } else { "Fork" })
                .variant(ButtonVariant::Ghost)
                .disabled(forking || name.is_empty())
                .on_press(move |s: &mut App| projects::fork(s, &name))
                .build(&k),
        );
        if s.back_to_grid {
            bar = bar.leading(
                Button::new("‹ Agents")
                    .variant(ButtonVariant::Ghost)
                    .on_press(|s: &mut App| {
                        s.sel = Sel::Agents;
                        s.back_to_grid = false;
                    })
                    .build(&k),
            );
        }
    }
    for b in zoom::toolbar(s, &k) {
        bar = bar.trailing(b);
    }
    bar = bar.trailing(Button::new("Split").variant(ButtonVariant::Ghost).disabled(!s.can_split()).on_press(|s: &mut App| s.split()).build(&k));
    if let Some(pin) = s.shown_pin() {
        let on = s.pins.has(&pin);
        bar = bar.trailing(Button::new(if on { "Unpin" } else { "Pin" }).variant(ButtonVariant::Ghost).on_press(move |s: &mut App| s.pins.toggle(pin.clone())).build(&k));
    }
    if let Some(p) = s.proj().filter(|p| p.down.is_some() && p.down.as_deref() != Some("connecting")) {
        // its coordd is not reached: why, and Start (its box's start steps or its host coordd, then its agents)
        let why = p.start_error.clone().or_else(|| p.down.clone()).unwrap_or_default();
        let i = s.cur;
        bar = bar.trailing(text(format!("{}: {}", p.name, sidebar::first_line(&why))).font(k.ty.small).color(k.c.danger.fg).truncate().max_w(420.0));
        bar = bar.trailing(
            Button::new(if p.starting.is_some() { "Starting…" } else { "Start" })
                .variant(ButtonVariant::Neutral)
                .disabled(p.starting.is_some())
                .on_press(move |s: &mut App| projects::bring_up(s, i))
                .build(&k),
        );
    }
    if let Some(n) = &s.notice {
        bar = bar.trailing(text(n.clone()).font(k.ty.small).color(k.c.danger.fg).truncate().max_w(420.0));
    }
    let mut layer = Vec::new();
    projects::dialog(s, &k, &mut layer);
    ask::mentions(s, cx, &k, &mut layer);
    ask::layer(s, cx, &k, &mut layer);
    panes::drop_zone(s, cx, &k, &mut layer);
    chat::viewer(s, &k, &mut layer);
    let split = SplitView::new(main.key("main"))
        .sidebar(&s.sidebar, |s: &mut App| &mut s.sidebar, sidebar::sidebar(s, &k).cache())
        .toolbar(bar)
        .titlebar_inset(cx.titlebar_inset())
        .frame_inset(8.0)
        .toolbar_in_panel()
        .sidebar_padding(14.0, 10.0)
        .sidebar_toggle(SidebarToggle::new().in_header())
        .reduced_motion(cx.is_reduced_motion())
        .build(&k);
    rune_kit::root(split, layer)
}

fn on_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if zoom::key(s, ev, tx) {
        return true;
    }
    if s.viewing.is_some() && matches!(ev.key, rune::Key::Escape) {
        s.viewing = None;
        return true;
    }
    if shell::is_sidebar_toggle(ev) {
        s.sidebar.toggle();
        return true;
    }
    tx.focused().is_none() && term::page_key(s, ev, tx)
}

/// A scene: the first project's real state, read once.
fn read_once(s: &mut App, sel: Sel) {
    let Some(p) = s.projects.first_mut() else { return };
    for m in link::once(&p.cmd, &[]) {
        if let Incoming::Event(ev) = m {
            p.apply(ev);
        }
    }
    p.down = None;
    // YAAIHH_VIEW_DATA=ID=FILE: view ID shows FILE's data (a source's output not yet live in coordd)
    if let Some((id, file)) = std::env::var("YAAIHH_VIEW_DATA").ok().as_deref().and_then(|v| v.split_once('='))
        && let (Some(v), Ok(text)) = (p.live.views.get_mut(id), std::fs::read_to_string(file))
    {
        v.data = serde_json::from_str(&text).ok();
    }
    // an agent scene without a name shows YAAIHH_AGENT, else the last agent added
    let sel = match sel {
        Sel::Agent(n) if n.is_empty() => {
            Sel::Agent(std::env::var("YAAIHH_AGENT").ok().or_else(|| p.live.agents.last().map(|a| a.name.clone())).unwrap_or_default())
        }
        x => x,
    };
    // the sessions the scene shows: the agent's, or the tiles'
    let names: Vec<String> = match &sel {
        Sel::Agent(name) => vec![name.clone()],
        Sel::Agents => agents::active(p).map(|a| a.name.clone()).collect(),
        _ => Vec::new(),
    };
    let reqs: Vec<Request> = names.iter().map(|n| Request::Transcript { name: n.clone(), before: None, after: None }).collect();
    if !reqs.is_empty() {
        for m in link::once(&p.cmd, &reqs) {
            if let Incoming::Reply(id, r) = m {
                let Some(name) = names.get((id as usize).wrapping_sub(1)) else { continue };
                if let Ok(page) = serde_json::from_str::<Transcript>(&r.out) {
                    p.convs.entry(name.clone()).or_default().merge(page, false);
                }
            }
        }
    }
    // the shown sessions' images, read and decoded before the frame
    let wanted: Vec<(String, Request)> =
        names.iter().filter_map(|n| Some((n, p.convs.get(n)?))).flat_map(|(n, c)| chat::conv_image_requests(n, c)).collect();
    if !wanted.is_empty() {
        let cache = s.image_cache.get_or_insert_with(|| rune::ImageCache::new(std::sync::Arc::new(|| {})));
        let reqs: Vec<Request> = wanted.iter().map(|(_, r)| r.clone()).collect();
        for m in link::once(&p.cmd, &reqs) {
            if let Incoming::Reply(id, r) = m
                && let Some((key, _)) = wanted.get((id as usize).wrapping_sub(1))
            {
                let img = chat::take_image(Some(cache), r.ok, &r.out);
                if let chat::Img::Ready(i) = &img {
                    let _ = i.wait();
                }
                s.images.insert(key.clone(), img);
            }
        }
    }
    s.sel = sel;
}

/// The terminal scene: agent NAME's page switched to Terminal, its screen read once. With YAAIHH_TERM_TEST (see the
/// top) the one project is a local coord folder whose pipe is `coordc bridge` run here (the coordc beside this
/// binary); else the first configured project.
fn term_scene(s: &mut App) {
    let name = std::env::var("YAAIHH_TERM_AGENT").unwrap_or_else(|_| "t1".into());
    if let Ok(coord) = std::env::var("YAAIHH_TERM_TEST") {
        let coordc = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join("coordc").display().to_string())).unwrap_or_else(|| "coordc".into());
        let cmd = vec!["env".into(), format!("COORD={coord}"), format!("COORDD_SOCK={coord}/coordd.sock"), coordc, "bridge".into()];
        s.projects = vec![Proj {
            name: "terminal test".into(),
            id: "term-test".into(),
            cmd,
            link: None,
            live: Live::default(),
            down: None,
            boxed: false,
            starting: None,
            forking: None,
            start_error: None,
            convs: HashMap::new(),
            pending: HashMap::new(),
            commits: Vec::new(),
            older: Vec::new(),
            older_from: None,
            older_pending: false,
        }];
    }
    if s.projects.is_empty() {
        s.notice = Some("no project: the term scene needs YAAIHH_TERM_TEST=COORD or a configured project".into());
        return;
    }
    s.cur = 0;
    read_once(s, Sel::Agent(name.clone()));
    s.term.pages.insert((0, name.clone()));
    term::attach(s, 0, &name);
    for m in link::once(&s.projects[0].cmd, &[Request::Attach { name: name.clone(), rows: 0, cols: 0 }]) {
        match m {
            Incoming::Pty(ev) => term::take(s, 0, ev),
            Incoming::Reply(_, r) if !r.ok => term::refused(&mut s.term, 0, &name, r.out),
            _ => {}
        }
    }
}

/// The core's scenes and the extensions' (`--snap F --scene NAME`).
fn scenes() -> &'static [(&'static str, fn(&mut App))] {
    let all: Vec<(&'static str, fn(&mut App))> = SCENES.iter().chain(ext::all().iter().flat_map(|e| e.scenes)).copied().collect();
    Box::leak(all.into_boxed_slice())
}

const SCENES: &[(&str, fn(&mut App))] = &[
    ("items", |s| read_once(s, Sel::View("items".into()))),
    ("closed", |s| {
        read_once(s, Sel::View("items".into()));
        s.show_closed = true;
    }),
    ("runner", |s| read_once(s, Sel::View("runner".into()))),
    ("clerk", clerk_scene),
    ("clerk-filed", |s| {
        clerk_scene(s);
        s.clerk_ed.filed_only = true;
    }),
    // the first filing run's details open
    ("clerk-details", |s| {
        clerk_scene(s);
        s.clerk_ed.filed_only = true;
        let first = s.projects.first().and_then(|p| p.live.nets.iter().rev().find(|r| r.effects.iter().any(|e| e.live)).map(|r| r.id.clone()));
        s.clerk_ed.details.extend(first);
    }),
    // the clerk's turn-end net open for editing, tried beside its last run (the files read here, not from coordd)
    ("clerk-edit", |s| {
        read_once(s, Sel::Clerk);
        let base = format!("{}/../../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let src = |kind: &str, path: String| coord_types::NetSource { text: std::fs::read_to_string(&path).unwrap_or_default(), target: path.replace("agent-base", "PROJECT"), path, scope: "global".into(), kind: kind.into() };
        let net = src("net", format!("{base}/nets/clerk.net"));
        let line = net.text.lines().position(|l| l.starts_with("net turn_end")).unwrap_or(0) + 1;
        let view = coord_types::NetsView {
            sources: vec![net.clone(), src("call", format!("{base}/calls/clerk.call")), src("live", format!("{base}/nets/live"))],
            nets: vec![
                coord_types::NetInfo { name: "owner_message".into(), event: "owner_message".into(), file: net.path.clone(), line: 4, calls: vec!["spellcheck".into(), "clarity".into(), "split".into()], bound: vec!["global".into()] },
                coord_types::NetInfo { name: "turn_end".into(), event: "turn_end".into(), file: net.path.clone(), line, calls: vec!["turn".into()], bound: vec!["global".into()] },
            ],
            errors: Vec::new(),
            marks: Vec::new(),
        };
        for x in &view.sources {
            s.clerk_ed.texts.insert(x.path.clone(), TextEdit::new(&x.text.replace("Go on with your work.", "Carry on.")));
        }
        s.clerk_ed.view = Some(Ok(view));
        s.clerk_ed.open = Some("turn_end".into());
        draw_once(s, &net.path);
        let last = s.projects.get(s.cur).and_then(|p| p.live.nets.iter().rev().find(|r| r.event == "turn_end").cloned());
        if let Some(r) = last {
            s.clerk_ed.picked = Some(r.id.clone());
            s.clerk_ed.tried = [Some(Ok(coord_types::NetRun { id: "try".into(), ..r })), None];
        }
    }),
    // the owner's-message net open: its own lines and drawing, the calls it uses (owner m130)
    ("clerk-owner", |s| {
        SCENES.iter().find(|(n, _)| *n == "clerk-edit").unwrap().1(s);
        s.clerk_ed.tried = [None, None];
        s.clerk_ed.picked = None;
        s.clerk_ed.open = Some("owner_message".into());
    }),
    // the drawing with the caret in the text: its step outlined (owner m127)
    ("clerk-caret", |s| {
        SCENES.iter().find(|(n, _)| *n == "clerk-edit").unwrap().1(s);
        s.clerk_ed.tried = [None, None];
        s.clerk_ed.picked = None;
        caret_on(s, "match decide end-of-turn");
    }),
    // the text broken mid-edit: the last good drawing stays under the parse error
    ("clerk-broken", |s| {
        SCENES.iter().find(|(n, _)| *n == "clerk-edit").unwrap().1(s);
        let Some(path) = s.clerk_ed.texts.keys().find(|p| p.ends_with(".net")).cloned() else { return };
        let t = s.clerk_ed.texts.get_mut(&path).unwrap();
        let broken = t.text.replacen("for p in pieces {", "for p in pieces", 1);
        t.replace_text(broken);
        draw_once(s, &path);
    }),
    // the project's home (its overview)
    ("overview", |s| read_once(s, Sel::View(String::new()))),
    ("items-board", |s| {
        read_once(s, Sel::View("items".into()));
        s.domain.board = true;
    }),
    ("decisions", |s| read_once(s, Sel::View("decisions".into()))),
    // the detail panel: the first item an agent works on (else the first item), the first card in a lane
    ("item-detail", |s| {
        read_once(s, Sel::View("items".into()));
        let Some(p) = s.projects.first() else { return };
        let id = p.live.agents.iter().flat_map(|a| a.items.first()).next().cloned().or_else(|| p.live.views.get("items")?.data.as_ref()?.pointer("/items/0/id")?.as_str().map(String::from));
        s.domain.detail = id.map(domain::Detail::Item);
    }),
    ("runner-detail", |s| {
        read_once(s, Sel::View("runner".into()));
        let Some(cards) = s.projects.first().and_then(|p| p.live.views.get("runner")?.data.as_ref()?.get("cards")?.as_array().cloned()) else { return };
        let card = cards.iter().find(|c| c.get("lane").is_some_and(|l| !l.is_null())).or(cards.first());
        s.domain.detail = card.and_then(|c| c.get("id")?.as_str()).map(|id| domain::Detail::Card(id.into()));
    }),
    ("mail", |s| {
        read_once(s, Sel::Mail);
        s.open_mail = s.projects.first().and_then(|p| p.live.mail.last()).map(|m| m.id.clone());
    }),
    ("agent", |s| read_once(s, Sel::Agent(String::new()))),
    ("agents", |s| read_once(s, Sel::Agents)),
    // the agent scene with its first image opened over the window (a click on it in a chat)
    ("agent-image", |s| {
        read_once(s, Sel::Agent(String::new()));
        // YAAIHH_IMAGE=FILE: that picture stands in when the chat holds none
        if let Ok(bytes) = std::env::var("YAAIHH_IMAGE").map(std::fs::read).unwrap_or(Ok(Vec::new())) && !bytes.is_empty() {
            let cache = s.image_cache.get_or_insert_with(|| rune::ImageCache::new(std::sync::Arc::new(|| {})));
            let img = cache.load(bytes);
            let _ = img.wait();
            s.images.insert("scene".into(), chat::Img::Ready(img));
        }
        s.viewing = s.images.iter().find(|(_, i)| matches!(i, chat::Img::Ready(_))).map(|(k, _)| k.clone());
    }),
    // the split: orch's chat beside LAZY's (panes.rs)
    ("split", |s| {
        read_once(s, Sel::Agent("LAZY".into()));
        s.split();
        read_once(s, Sel::Agent("orch".into()));
    }),
    ("questions", |s| read_once(s, Sel::Questions)),
    // a question coordd's recogniser found in an agent's plain mail, its decision's why open (policy slice)
    ("recognised", |s| {
        read_once(s, Sel::Questions);
        let Some(p) = s.projects.first_mut() else { return };
        let cost = |rule: &str, level, cost| coord_types::Cost { rule: rule.into(), level, cost };
        let (fam, price) = ("Prefer for model-for When call.task == \"recognise-question\": family(option, \"codex\") > family(option, \"claude\") (agent-base/policy/decisions.eflint:3)",
            "Prefer for model-for: minimise price(option) (agent-base/policy/decisions.eflint:5)");
        let d = coord_types::Decision {
            id: "D1".into(), at: "2026-11-08T15:20:00Z".into(), decision: "model-for".into(),
            call: [("agent", "orch"), ("call", "recognise-question"), ("task", "recognise-question")].map(|(f, v)| (f.to_string(), v.to_string())).into(),
            chosen: Some("gpt-6-luna".into()), optimal: vec!["gpt-6-luna".into()],
            excluded: vec![coord_types::Exclusion { option: "gpt-image-2".into(), reason: "requires".into(), rule: "Requires price(option) >= 0 (agent-base/policy/prelude.eflint:12)".into() }],
            costs: vec![cost(fam, 1000, 0), cost(price, 999, 250000)],
            runner_up: Some(("gpt-6-mini".into(), vec![cost(fam, 1000, 0), cost(price, 999, 400000)])),
            decided_by: Some(cost(price, 999, 150000)),
            attempts: vec![coord_types::Attempt { model: "gpt-6-luna".into(), failure: None }],
            ..Default::default()
        };
        p.live.mail.put(coord_types::MailView {
            id: "m9998".into(), at: "2026-11-08T15:19:00Z".into(), from: "orch".into(), to: vec!["owner".into()], subject: "runner green".into(),
            body: "runner green\nThe runner is green again. Should I merge core/l1 into dev now, or wait for the proof check?".into(),
            esc: false, soon: false, delivered: vec!["owner".into()], withdrawn: Vec::new(), kind: String::new(), re: None, quotes: Vec::new(), state: Some("open".into()),
            recognised: Some(coord_types::Recognised { question: "Merge core/l1 into dev now, or wait for the proof check?".into(),
                quote: "Should I merge core/l1 into dev now, or wait for the proof check?".into(), decision: "D1".into(), model: "gpt-6-luna".into() }),
            filed: None, unfiled: false, clerk: None,
        });
        let key = format!("{}/D1", p.id);
        s.notes.why.insert(key, Some(Ok(d)));
    }),
    // the comment bar with a note being written and one comment waiting (YAAIHH_AGENT's chat)
    ("comment", comment_scene),
    // an @ being typed in an agent chat's composer: the agents offered at the caret (owner m118)
    ("mention", |s| {
        read_once(s, Sel::Agent(String::new()));
        let Sel::Agent(a) = s.sel.clone() else { return };
        let mut w = TextEdit::new("merge it and ask @");
        w.set_caret(w.text.len());
        w.focused = true;
        s.composers.insert((s.cur, a.clone()), w);
        s.notes.mention = Some(ask::Mention { field: ask::Field::Composer(s.cur, a), at: 17, query: String::new(), active: 0 });
    }),
    // a waiting comment opened from its highlight
    ("comment-open", |s| {
        comment_scene(s);
        let Sel::Agent(a) = s.sel.clone() else { return };
        (s.notes.picked, s.notes.writing, s.notes.open) = (None, None, Some(ask::waiting_key(&a, 0)));
    }),
    // a mail the owner sent that has not reached the agent yet, queued at the chat's foot (chat.rs `queued`)
    ("queued", |s| {
        read_once(s, Sel::Agent(String::new()));
        let Sel::Agent(a) = s.sel.clone() else { return };
        let Some(p) = s.projects.first_mut() else { return };
        if let Some(x) = p.live.agents.iter_mut().find(|x| x.name == a) {
            x.state = "working".into();
        }
        p.live.mail.put(coord_types::MailView {
            id: "m9999".into(), at: "2026-11-08T14:02:00Z".into(), from: "owner".into(), to: vec![a], subject: String::new(),
            body: "once the runner is green, merge it and tell me".into(), esc: false, soon: false, delivered: Vec::new(), withdrawn: Vec::new(),
            kind: String::new(), re: None, quotes: Vec::new(), state: None, recognised: None, filed: None, unfiled: false, clerk: None,
        });
    }),
    // a mail of the owner's the clerk sent back, with Deliver anyway (owner m116)
    ("queued-clerk", |s| {
        read_once(s, Sel::Agent(String::new()));
        let Sel::Agent(a) = s.sel.clone() else { return };
        let Some(p) = s.projects.first_mut() else { return };
        p.live.mail.put(coord_types::MailView {
            id: "m9999".into(), at: "2026-11-08T14:02:00Z".into(), from: "owner".into(), to: vec![a], subject: String::new(),
            body: "merge it once its green".into(), esc: false, soon: false, delivered: Vec::new(), withdrawn: Vec::new(),
            kind: String::new(), re: None, quotes: Vec::new(), state: None, recognised: None, filed: None, unfiled: false,
            clerk: Some("sent back: which branch: core/l1 or lazy/portfolio?".into()),
        });
    }),
    // a placed comment beside a queued mail: the same card (owner m132)
    ("comment-queued", |s| {
        SCENES.iter().find(|(n, _)| *n == "queued").unwrap().1(s);
        let mails = s.projects.first().map(|p| p.live.mail.clone()).unwrap_or_default();
        comment_scene(s);
        if let Some(p) = s.projects.first_mut() {
            p.live.mail = mails;
        }
        (s.notes.picked, s.notes.writing) = (None, None);
        let Sel::Agent(a) = s.sel.clone() else { return };
        if let Some(c) = s.notes.comments.get_mut(&a).and_then(|v| v.first_mut()) {
            c.note = "yes, and show the age too; @DEBUG check it".into();
        }
    }),
    ("agents-all", |s| {
        read_once(s, Sel::Agents);
        s.agents_tab = 1;
    }),
    ("new-project", |s| {
        s.new_project.open = true;
        s.new_project.name = TextEdit::new("Scratch");
        s.new_project.dir = TextEdit::new("/tmp/scratch");
        s.new_project.boxed = false;
    }),
    ("term", term_scene),
    ("commits", |s| read_once(s, Sel::View("commits".into()))),
    ("jump", |s| {
        read_once(s, Sel::View("commits".into()));
        if let Some(c) = s.commits().iter().cloned().find(|c| s.projects[0].live.agents.iter().any(|a| a.name == c.agent)) {
            let req = Request::Transcript { name: c.agent.clone(), before: None, after: None };
            for m in link::once(&s.projects[0].cmd, &[req]) {
                if let Incoming::Reply(1, r) = m {
                    if let Ok(page) = serde_json::from_str::<Transcript>(&r.out) {
                        s.projects[0].convs.entry(c.agent.clone()).or_default().merge(page, false);
                    }
                }
            }
            s.sel = Sel::Agent(c.agent.clone());
            s.seek = Some(c); // the first frame's poll finds it and scrolls there
        }
    }),
];

/// Whether this is a dev instance (`--dev`): beside prod, its own socket and tray icon (tray.rs); an extension leaves
/// to prod what only one instance should do.
static DEV: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn dev() -> bool {
    DEV.load(std::sync::atomic::Ordering::Relaxed)
}

/// File PATH's text in the Clerk editor drawn by coordd (`Request::NetGraph`), as typing in it does.
fn draw_once(s: &mut App, path: &str) {
    let (Some(p), Some(t)) = (s.projects.first(), s.clerk_ed.texts.get(path)) else { return };
    let text = t.text.clone();
    for m in link::once(&p.cmd, &[Request::NetGraph { file: path.into(), text: text.clone() }]) {
        if let Incoming::Reply(1, r) = m {
            let slot = s.clerk_ed.graphs.entry(path.to_string()).or_default();
            match if r.ok { serde_json::from_str(&r.out).map_err(|e| e.to_string()) } else { Err(r.out) } {
                Ok(g) => *slot = (Some(g), None),
                Err(e) => slot.1 = Some(e),
            }
        }
    }
    s.clerk_ed.drawn.insert(path.to_string(), text);
}

/// The Clerk editor scene with the caret on the first line holding NEEDLE.
fn caret_on(s: &mut App, needle: &str) {
    let Some(path) = s.clerk_ed.texts.keys().find(|p| p.ends_with(".net")).cloned() else { return };
    let Some(name) = s.clerk_ed.open.clone() else { return };
    let t = &mut clerk::piece_mut(s, &path, &name).edit;
    if let Some(at) = t.text.find(needle) {
        t.set_caret(at);
        t.focused = true;
    }
}

/// The Clerk page with the nets read from coordd, as the sidebar's Clerk row does.
fn clerk_scene(s: &mut App) {
    read_once(s, Sel::Clerk);
    let Some(p) = s.projects.first() else { return };
    for m in link::once(&p.cmd, &[Request::Nets]) {
        if let Incoming::Reply(1, r) = m {
            s.clerk_ed.view = Some(if r.ok { serde_json::from_str(&r.out).map_err(|e| format!("the nets: {e}")) } else { Err(r.out) });
        }
    }
}

/// The comment scene: one comment waiting and one being written, on the agent's latest messages.
fn comment_scene(s: &mut App) {
    read_once(s, Sel::Agent(String::new()));
    let Sel::Agent(a) = s.sel.clone() else { return };
    // two passages of the agent's messages: one commented on and waiting, one being commented on
    let said: Vec<(String, String)> = s.projects[s.cur].convs.get(&a).map_or(Vec::new(), |c| {
        c.entries.iter().filter(|e| e.kind == "assistant").map(|e| (format!("{a}@{}", e.offset), crate::paths::linkify(&e.text))).collect()
    });
    let passage = |(source, src): &(String, String)| {
        let plain = |w: &&str| w.chars().all(|c| c.is_alphanumeric() || ",.'".contains(c));
        let words: Vec<&str> = src.lines().map(|l| l.split_whitespace().skip_while(|w| !plain(w)).take_while(plain).take(5).collect::<Vec<_>>()).find(|w| w.len() >= 4)?;
        let quote = words.join(" ");
        let at = rune_kit::markdown::find(src, &quote, rune_kit::markdown::Case::Sensitive).into_iter().next()?;
        Some((source.clone(), quote, at))
    };
    // the latest messages, on screen
    let mut found = said.iter().rev().filter_map(passage);
    if let Some((source, quote, at)) = found.next() {
        s.notes.comments.insert(a.clone(), vec![ask::Comment { source, label: "your message".into(), quote, note: "yes, and show the age too".into(), at }]);
    }
    if let Some((source, quote, at)) = found.next() {
        s.notes.picked = Some(ask::Picked { target: a, source, label: "your message".into(), quote, at, pi: s.cur });
        s.notes.writing = Some(TextEdit::new("good, that is how it should work"));
    }
}

/// Run yaaihh with the extensions EXTS (ext.rs; none: the core alone): the window, or with `--background` the tray
/// only, or with `--snap` one scene's still.
pub fn run(exts: Vec<ext::Extension>) {
    ext::register(exts);
    // `--background` (login autostart): the tray only, no window. `--replace`: the running yaaihh quits first (a
    // release going live). `--dev`: beside prod (tray.rs). Every other argument is Rune's (the scenes).
    let ours = ["--background", "--replace", "--dev"];
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let (background, still) = (flag("--background"), flag("--snap"));
    DEV.store(flag("--dev"), std::sync::atomic::Ordering::Relaxed);
    if flag("--replace") && !tray::replace() {
        eprintln!("yaaihh --replace: the running yaaihh did not quit; it keeps running");
        std::process::exit(1);
    }
    if !still && tray::hand_over() {
        return; // a yaaihh runs: it opened its window
    }
    if !still && !rune::is_headless() {
        log::open();
    }
    let chrome = Chrome { full_size_content: true, traffic_lights: Some((16.0, 25.0)), drag_regions: true };
    let app = rune::App::new(AppSpec {
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(on_key),
        on_wheel: Some(zoom::wheel),
        scenes: scenes(),
        min_size: Some((720.0, 480.0)),
        // mono text (tool calls, results) in the system's monospace face
        families: ("Inter", "ui-monospace"),
        ..AppSpec::new("yaaihh", (1280.0, 800.0), init, view)
    })
    .chrome(chrome)
    .args(args.into_iter().filter(|a| !ours.contains(&a.as_str())));
    if still {
        return app.run();
    }
    // closing the window hides it: the usage collector and the boxes' links go on (tray.rs)
    let app = if background { app.windowless() } else { app.keep_running() };
    let _tray = tray::start(app.handle());
    app.run();
}
