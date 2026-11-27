//! The sidebar (owner 7 Nov night), top to bottom:
//!
//! - **Home**: the window's own Home over every project (global.rs).
//! - **Projects +**: the projects; "+" starts a new one.
//! - **the current project's entries**: Map · Threads · Agents · Project (paper-research N-1446 p1938), beneath each
//!   only while it is open what it holds, but for Agents. Agents says how many of its agents run, in words ("9
//!   running"), and opens the Agents page, which holds its agents by their teams (agents.rs). Beneath it, on every page,
//!   only what he pinned in this project (his N-1104 p863: "under agents i just want to be able to drag and pin";
//!   paper-research N-1446 p2008, p2016), in his order:
//!   - agents' chats (the agent page's Pin button): a press opens the agent's page, his chat with it;
//!   - threads (Pin behind a thread's `⋯`): a row opens the thread, its label with its id quiet; `thread` is set and
//!     `id` is the ticket's;
//!   - lenses.
//!   Anything a row drags (a thread of the structure, the Map's row, an insight, an agent's tile on the Agents page)
//!   is pinned by a drop on Agents (panes.rs `drop_zone`). A row's close button unpins it; dragging a row reorders
//!   them, or drops it into a pane. Kept in `~/.config/yaaihh/pins.json` (`[{"project": ID, "id": AGT-NNN, "agent":
//!   NAME}]`): a pin is the agent's id, like every other reference to an agent, so it follows a rename; the name is
//!   the last one seen, shown while the project is down. A pin written before ids (no `id`) takes its agent's id when
//!   the project is first seen. A pin stands under its own project's Agents, his pins of other projects under theirs.
//! - each extension's section (ext.rs).
//! - **Structure** (the owner, N-1104 p599: "here just the raw structure like a if you open vscode you just get the
//!   view at the root you navigate it"; proto-window N-1446 p10): the project's open threads as they are, newest first,
//!   each unfolding to its subthreads, folded as he left them, the section itself folding at its head. A row opens its
//!   thread on the page shown (a lens keeps it beside its list); dragged onto the main area it opens in a pane there.
//!   Above the threads, Insights (S6), folded until he unfolds it: each insight a row, opened as any lens is.
//!   While a pane draws the Map, the rows under Insights are the Map's nodes instead, of the island of the project
//!   chosen, nested and in the order the Map has them; a press flies the Map there, and where the Map is lights its
//!   row and opens the rows above it (paper-research N-1446 p1126: "a way to move around the Map, never the structure
//!   itself").
//! - **an agent's folder** (the file view, paper-research N-1446 p1441): from a coordd that gives it, a pinned agent's
//!   row under Agents folds; unfolded, its folder's five groups under it (its kind and brief, its loaded texts by
//!   version, what it read, its sessions, its commits and jobs), each row saying where it comes from and folding in
//!   turn. The tree shows rows and counts only: a version's words open apart (popup.rs; p1633).
//!
//! A pinned lens (lens.rs) is a row under Agents: a press shows it, a drag reorders the pins or drops it into a pane as
//! an agent is. It is kept in `pins-lenses.json` beside pins.json with its place in the list, so a window from before
//! lenses reads pins.json whole and shows no row that opens nothing ([`joined`], [`split`]). Which lenses are offered to pin, and where (a2o and a2a beside each agent's With you), is docket-edge6's
//! half (N-1446 p18 (3)).

use crate::{App, Sel, agents, ext};
use rune::{El, Icon};
use rune_kit::shell::{Reorder, RowStatus, Sidebar, SidebarRow, SidebarSection};
use rune_kit::{ControlSize, IconButton, IconButtonVariant, Kit};
use coord_types::AgentView;
use docket_machine::graph_slice::Row as FolderRow;
use serde::{Deserialize, Serialize};

/// The sidebar's width closed to its icons (the owner N-284.p1: narrow, and it can be closed to icons).
pub const RAIL_W: f32 = 56.0;
/// The points between the sidebar's edges and its rows (lib.rs's `sidebar_padding`): at the window's edge, then at the
/// main pane.
pub const SIDE_PAD: [f32; 2] = [14.0, 10.0];
static THREADS: Icon = Icon { name: "yaaihh-threads", view: 16.0, body: r#"<path d="M3 3.5H13V10.5H7L4.5 12.5V10.5H3Z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>"# };
static AGENTS: Icon = Icon { name: "yaaihh-agents", view: 16.0, body: r#"<circle cx="8" cy="5.5" r="2.5" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M3 13C3.6 10.6 5.6 9.5 8 9.5S12.4 10.6 13 13" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"# };
static PROJECT: Icon = Icon { name: "yaaihh-project", view: 16.0, body: r#"<path d="M2.5 4.5H6.5L7.5 5.5H13.5V12.5H2.5Z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>"# };
static NARROW: Icon = Icon { name: "yaaihh-narrow", view: 16.0, body: r#"<path d="M9.5 4L5.5 8L9.5 12" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>"# };
static HOME: Icon = Icon { name: "yaaihh-home", view: 16.0, body: r#"<path d="M2.5 7.5L8 3L13.5 7.5V13H2.5Z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>"# };
static USAGE: Icon = Icon { name: "yaaihh-usage", view: 16.0, body: r#"<path d="M3.5 13V9M8 13V4M12.5 13V7" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>"# };
static ACCOUNTS: Icon = Icon { name: "yaaihh-accounts", view: 16.0, body: r#"<circle cx="5.5" cy="8" r="2.5" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M8 8H13.5M11.5 8V10M13.5 8V10" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"# };
static SETTINGS: Icon = Icon { name: "yaaihh-settings", view: 16.0, body: r#"<path d="M2.5 4.5H13.5M2.5 8H13.5M2.5 11.5H13.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/><circle cx="10" cy="4.5" r="1.4" fill="currentColor"/><circle cx="5.5" cy="8" r="1.4" fill="currentColor"/><circle cx="11" cy="11.5" r="1.4" fill="currentColor"/>"# };
static WIDEN: Icon = Icon { name: "yaaihh-widen", view: 16.0, body: r#"<path d="M6.5 4L10.5 8L6.5 12" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>"# };
static UNPIN: Icon = Icon { name: "yaaihh-unpin", view: 16.0, body: r#"<path d="M4.5 8H11.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"# };
static PLUS: Icon = Icon { name: "yaaihh-plus", view: 16.0, body: r#"<path d="M8 3.5V12.5M3.5 8H12.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"# };

/// A pinned chat: the agent whose id is ID (last seen named AGENT) of the project whose id is PROJECT.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pin {
    pub project: String,
    #[serde(default)]
    pub id: String,
    /// The agent's name, or for a thread the label last seen.
    pub agent: String,
    /// A pinned thread (its ticket id in `id`), not an agent's chat.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub thread: bool,
    /// A pinned lens (lens.rs), its name in `agent`: not an agent's chat nor a thread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lens: Option<crate::lens::Lens>,
}

impl Pin {
    /// Whether PIN is the same chat: same project and agent id (by name only while either has no id yet).
    fn same(&self, pin: &Pin) -> bool {
        if self.lens.is_some() || pin.lens.is_some() {
            return self.project == pin.project && self.lens.as_ref().map(|l| &l.filter) == pin.lens.as_ref().map(|l| &l.filter);
        }
        self.project == pin.project && self.thread == pin.thread && if self.id.is_empty() || pin.id.is_empty() { self.agent == pin.agent } else { self.id == pin.id }
    }

    /// The pinned agent among AGENTS: by id, or by name for a pin without one.
    fn find<'a>(&self, agents: &'a [AgentView]) -> Option<&'a AgentView> {
        agents.iter().filter(|_| !self.thread && self.lens.is_none()).find(|a| if self.id.is_empty() { a.name == self.agent } else { a.id == self.id })
    }
}

/// The pinned chats and the drag that reorders them.
#[derive(Default)]
pub struct Pins {
    pub list: Vec<Pin>,
    /// The lens pins this window cannot read (a newer window's), kept as written for the window that wrote them.
    unread: Vec<serde_json::Value>,
    /// The lens pins' file was there and not a list this window reads (a newer window's shape, or one cut off by a
    /// window from before `write_whole`): it is left as it is, never written over or removed (proto-reader N-1446
    /// p280), and the lens pins stay in pins.json (`write`).
    kept: bool,
    pub drag: Reorder,
}

fn pins_path() -> std::path::PathBuf {
    std::path::Path::new(&crate::projects::config_path()).with_file_name("pins.json")
}

/// The lens pins' file beside pins.json (proto-lead N-1446 p158; docket-edge6 p166). pins.json is a bare list read
/// whole, so a lens cannot be a field of it as ui.json's `lenses` is (state.rs `lensed`): an older window would show
/// its pin as a row that opens nothing, and one that met a new top level would show no pins and write over them.
fn lens_pins_path(pins: &std::path::Path) -> std::path::PathBuf {
    pins.with_file_name("pins-lenses.json")
}

/// A pin of the lens pins' file: its place in the whole list, and the pin.
#[derive(Serialize, Deserialize)]
struct Placed {
    at: usize,
    pin: Pin,
}

/// pins.json's text PINS and the lens pins' file's LENSES (none: no file) as one list, each lens pin back at its place,
/// or after the others when the list is shorter (an older window took pins out); a pins.json written with lens pins in
/// it (S1, or while `Pins::kept`) reads as it is, and a lens pin of the file it already holds is not read twice. Each
/// lens pin is read on its own: one this window cannot read is kept as written, a line in yaaihh.log. A file that is
/// not a list this window reads gives no lens pins and no `unread` at all: it is not this window's to write
/// (`Pins::kept`).
fn joined(pins: &str, lenses: Option<&[u8]>) -> (Vec<Pin>, Option<Vec<serde_json::Value>>) {
    let mut list: Vec<Pin> = serde_json::from_str(pins).unwrap_or_default();
    let all = match lenses.map(serde_json::from_slice::<Vec<serde_json::Value>>) {
        None => Vec::new(),
        Some(Ok(all)) => all,
        Some(Err(err)) => {
            crate::log::line(&format!("pins-lenses.json: not a list this window reads ({err}); left as it is"));
            return (list, None);
        }
    };
    let (mut placed, mut unread) = (Vec::new(), Vec::new());
    for e in all {
        match serde_json::from_value::<Placed>(e.clone()) {
            Ok(p) => placed.push(p),
            Err(err) => {
                crate::log::line(&format!("pins-lenses.json: a pin this window does not know ({err}); kept as written"));
                unread.push(e);
            }
        }
    }
    placed.sort_by_key(|p| p.at);
    for p in placed {
        if !list.iter().any(|q| q.same(&p.pin)) {
            list.insert(p.at.min(list.len()), p.pin);
        }
    }
    (list, Some(unread))
}

/// LIST as the two files write it: pins.json, the agents' and threads' pins, which a window from before lenses reads
/// whole; the lens pins with their places, then UNREAD as they came (none: no file).
fn split(list: &[Pin], unread: &[serde_json::Value]) -> (String, Option<String>) {
    let pins: Vec<&Pin> = list.iter().filter(|p| p.lens.is_none()).collect();
    let mut lenses: Vec<serde_json::Value> = list.iter().enumerate().filter(|(_, p)| p.lens.is_some()).map(|(at, p)| serde_json::to_value(Placed { at, pin: p.clone() }).unwrap()).collect();
    lenses.extend(unread.iter().cloned());
    (serde_json::to_string_pretty(&pins).unwrap(), (!lenses.is_empty()).then(|| serde_json::to_string_pretty(&lenses).unwrap()))
}

impl Pins {
    pub fn load() -> Pins {
        Pins::read(&pins_path())
    }

    /// The pins of pins.json at PINS and the lens pins' file beside it.
    fn read(pins: &std::path::Path) -> Pins {
        let text = std::fs::read_to_string(pins).unwrap_or_default();
        let (list, unread) = match std::fs::read(lens_pins_path(pins)) {
            Ok(lenses) => joined(&text, Some(&lenses)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => joined(&text, None),
            // there and not read: left as it is, as one that is not a list (consult-3 N-1446 p326: its own error)
            Err(e) => {
                crate::log::line(&format!("pins-lenses.json: not read ({e}); left as it is"));
                (joined(&text, None).0, None)
            }
        };
        Pins { list, kept: unread.is_none(), unread: unread.unwrap_or_default(), drag: Reorder::default() }
    }

    fn save(&self) {
        self.write(&pins_path());
    }

    /// Write the pins to pins.json at PINS and the lens pins' file beside it, each whole or not at all (state.rs
    /// `write_whole`, as ui.json is: consult-3 N-1446 p326). While the lens pins' file is `kept` it is left as it is,
    /// and pins.json holds the lens pins inline as S1 wrote them, those read and those pinned since (proto-reader p322):
    /// none is lost by this window. A window from before lenses (38's `Pin` has no `lens`) shows them as rows that open
    /// nothing, and its next pin change writes them back without their lens: back here they are agent rows, and the
    /// lens pins are gone (consult-3 p439). Only in `kept`, which since `write_whole` comes from a newer window's shape.
    fn write(&self, pins: &std::path::Path) {
        if self.kept {
            crate::log::line("pins-lenses.json: left as it is, as this window could not read it; the lens pins are kept in pins.json");
            let _ = crate::state::write_whole(pins, &serde_json::to_string_pretty(&self.list).unwrap());
            return;
        }
        let (text, lenses) = split(&self.list, &self.unread);
        let _ = crate::state::write_whole(pins, &text);
        let _ = match lenses {
            Some(l) => crate::state::write_whole(&lens_pins_path(pins), &l),
            None => std::fs::remove_file(lens_pins_path(pins)),
        };
    }

    pub fn has(&self, pin: &Pin) -> bool {
        self.list.iter().any(|p| p.same(pin))
    }

    /// His pins as LIST, a state of his window had them (state.rs `go_back_to`): saved when they differ.
    pub(crate) fn put_back(&mut self, list: Vec<Pin>) {
        if serde_json::to_value(&self.list).ok() != serde_json::to_value(&list).ok() {
            self.list = list;
            self.save();
        }
    }

    /// Pin PIN at AT among his pins (none: last), unless it is pinned: his new chat where the old one's pin stands
    /// (archive.rs `step`).
    pub(crate) fn put(&mut self, pin: Pin, at: Option<usize>) {
        if self.has(&pin) {
            return;
        }
        let i = at.unwrap_or(self.list.len()).min(self.list.len());
        self.list.insert(i, pin);
        self.save();
    }

    /// A spin-off NEW of agent FROM of project PROJECT pinned right under FROM's pin, so it stands by the agent it came
    /// from (his walk item 19), else last; nothing when it is pinned. Its id comes with the next agents (`follow`).
    pub fn spun_off(&mut self, project: &str, from: &str, new: &str) {
        let at = self.list.iter().rposition(|p| p.project == project && !p.thread && p.lens.is_none() && p.agent == from).map(|i| i + 1);
        self.put(Pin { project: project.into(), id: String::new(), agent: new.into(), thread: false, lens: None }, at);
    }

    /// Agent NAME's pin of project PROJECT gone, if it has one: a spin-off joined back or deleted.
    pub fn unpin_agent(&mut self, project: &str, name: &str) {
        let before = self.list.len();
        self.list.retain(|p| !(p.project == project && !p.thread && p.lens.is_none() && p.agent == name));
        if self.list.len() != before {
            self.save();
        }
    }

    /// Pin PIN, or unpin it when pinned.
    pub fn toggle(&mut self, pin: Pin) {
        match self.list.iter().position(|p| p.same(&pin)) {
            Some(i) => _ = self.list.remove(i),
            None => self.list.push(pin),
        }
        self.save();
    }

    /// Bring the pins of project PROJECT up to its AGENTS: an id for a pin without one, the current name of a renamed
    /// agent; saved when something changed.
    pub fn follow(&mut self, project: &str, agents: &[AgentView]) {
        let mut changed = false;
        for pin in self.list.iter_mut().filter(|p| p.project == project && !p.thread && p.lens.is_none()) {
            if let Some(a) = pin.find(agents)
                && (pin.id != a.id || pin.agent != a.name)
            {
                (pin.id, pin.agent) = (a.id.clone(), a.name.clone());
                changed = true;
            }
        }
        if changed {
            self.save();
        }
    }

    /// The FROM-th of the pins shown under a project's Agents put where the TO-th is, SHOWN their places in the list:
    /// his pins of other projects between them stay where they are.
    fn moved(&mut self, shown: &[usize], from: usize, to: usize) {
        let (Some(&from), Some(&to)) = (shown.get(from), shown.get(to)) else { return };
        if from < self.list.len() && to < self.list.len() {
            let p = self.list.remove(from);
            self.list.insert(to, p);
            self.save();
        }
    }
}

impl App {
    /// The pin for the agent or the lens shown, if one is.
    pub fn shown_pin(&self) -> Option<Pin> {
        match &self.sel {
            Sel::Agent(_) => self.pin_of(self.cur, &self.sel),
            Sel::Lens(l) => self.lens_pin(l),
            _ => None,
        }
    }

    /// The pin for page SEL of project PI, as a drop on Agents makes it (his N-1104 p863; paper-research N-1446 p2008):
    /// an agent's chat; the thread a structure's row drags (Threads' lens open at it, `Lens::threads_at`), by its label
    /// now; any other lens at its root, as [`Self::lens_pin`]; none for another page.
    pub fn pin_of(&self, pi: usize, sel: &Sel) -> Option<Pin> {
        let p = self.projects.get(pi)?;
        let project = p.id.clone();
        match sel {
            Sel::Agent(name) => {
                let id = p.live.agents.iter().find(|a| &a.name == name).map_or(String::new(), |a| a.id.clone());
                Some(Pin { project, id, agent: name.clone(), thread: false, lens: None })
            }
            Sel::Lens(l) => match &l.at {
                Some(id) if l.filter == crate::lens::Filter::Threads && l.depth == crate::lens::Depth::Structure => {
                    let pin = Pin { project, id: id.clone(), agent: id.clone(), thread: true, lens: None };
                    Some(Pin { agent: pin_label(p, &pin), ..pin })
                }
                _ => Some(Pin { project, id: String::new(), agent: l.name(), thread: false, lens: Some(crate::lens::Lens { at: None, ..l.clone() }) }),
            },
            _ => None,
        }
    }

    /// The pin for lens L of the current project, by its name: pinned at its root, so a press on its row opens it
    /// there (`show_lens`) whatever thread it had open when pinned.
    pub fn lens_pin(&self, l: &crate::lens::Lens) -> Option<Pin> {
        self.pin_of(self.cur, &Sel::Lens(crate::lens::Lens { at: None, ..l.clone() }))
    }

    /// The pin for thread V of the current project: its ticket id, and its label as it reads now.
    pub fn thread_pin(&self, v: &coord_types::TicketView) -> Option<Pin> {
        let p = self.proj()?;
        Some(Pin { project: p.id.clone(), id: v.id.clone(), agent: p.labels.get(&v.id).map(str::to_string).unwrap_or_else(|| v.title.trim().to_string()), thread: true, lens: None })
    }

    /// Show agent AGENT of project I.
    pub fn open_in(&mut self, i: usize, agent: &str) {
        self.cur = i;
        self.select_agent(agent);
    }
}

pub fn sidebar(s: &App, k: &Kit) -> El {
    // built again only when something it shows changes, not for a keystroke or a frame elsewhere, or the look
    let mut v = crate::version::Version::default();
    v.add(k.dark);
    if s.pins.list.iter().any(|p| p.thread) {
        // a pinned thread's row reads its label and whether it is the one shown
        let labels: Vec<String> = s.pins.list.iter().filter(|p| p.thread).filter_map(|p| s.projects.iter().find(|q| q.id == p.project).map(|q| pin_label(q, p))).collect();
        let ended: Vec<bool> = s.pins.list.iter().filter(|p| p.thread).map(|p| s.projects.iter().find(|q| q.id == p.project).is_some_and(|q| q.live.tickets.values().find(|t| t.id == p.id).is_none_or(closed))).collect();
        v.add(s.tickets.shown).debug(&labels).debug(&ended);
    }
    v.debug(&s.pins.list).debug(&s.pins.drag).debug(&s.pin_hover).add(s.cur).debug(&s.sel).add(s.rail).add(s.narrow.get());
    // the structure's folds, and while it is open what its rows read: the tickets and the thread shown
    let mut folds: Vec<&String> = s.streams.open.iter().filter(|x| x.starts_with("tree|")).collect();
    folds.sort();
    v.debug(&folds);
    // his Archive (archive.rs): its moment and whether it is shown move Structure, his pins and its own row
    v.debug(&s.archive);
    if let Some(p) = s.proj().filter(|p| !s.streams.open.contains(&section_key(s.cur)) && p.down.is_none()) {
        v.add((p.live.tickets_rev, s.tickets.shown));
        // while they are the Map's ([`map_island`]): its graph, the node lit and the depth the Map is at
        let drawn = crate::map::canvas::drawn_in(s).is_some();
        v.add(drawn);
        if drawn {
            v.add(s.map.graph_rev()).debug(&lit_name(s)).debug(&s.map.depth_at().map(|x| x.1));
        }
    }
    // what a pinned agent's row says beneath its name (agents.rs `side_line`): the thread it works on, idle since
    // when, that it waits on him
    if let Some(p) = s.proj() {
        for a in s.pins.list.iter().filter(|pin| pin.project == p.id).filter_map(|pin| pin.find(&p.live.agents)) {
            v.debug(&agents::side_line(p, a));
        }
    }
    // the agents' folders under their rows: what came of them, the blocks he moved to, whether they are given, and
    // whether a press on a block's version or a session opens it apart; while one is held, the sidebar's width (a
    // file's path is cut to fit it, [`path_cut`])
    v.add(s.folders.rev).add(s.proj().is_some_and(folder_served)).add(s.proj().is_some_and(texts_served));
    v.add((!s.folders.got.is_empty()).then(|| s.sidebar.width() as i32));
    for e in ext::all() {
        v.add((e.nav_key)(s));
    }
    for q in &s.projects {
        v.add((&q.id, &q.name, &q.down, q.live.slow_project(), q.live.mail_hold.is_some()));
        // what a row reads of an agent, never its counters (memory, CPU, tokens move twice a second while it works,
        // and the whole sidebar was built again each time, 11 Nov)
        for a in &q.live.agents {
            v.add((&a.id, &a.name, &a.parent, &a.role, &a.kind, &a.model, &a.state, &a.part, &a.dialog));
        }
        v.debug(&q.live.parts);
    }
    if let Some(p) = s.proj() {
        // what the rows read, not the event count (main cde92cf: built again on every event otherwise)
        let notes = p.live.tickets.values().filter(|v| v.kind == "note" && v.posts.iter().any(|x| x.names.iter().chain(x.told.iter()).any(|n| n == "owner"))).count();
        v.add((crate::stars::count(p), crate::stars::any(p, s.tickets.find.owned.get(&p.id).map_or(&[][..], |v| v)))).add(s.tickets.find.shown == Some(crate::find::Shown::ForOwner));
        // the search field: whether it stands here, and only then its words, caret and focus, `titles only`, the line
        // under it and what it says it searches (typing in a list's field, the sidebar folded away, leaves the sidebar
        // as it was)
        let q = &s.tickets.find.q;
        let field = crate::find::in_sidebar(s);
        v.add((field, field.then(|| (&q.text, q.anchor, q.focus, q.focused, s.tickets.find.titles, crate::find::searching(s), crate::find::says(s, true)))));
        v.add(crate::outline::active(s, p));
        v.add((p.live.mail.len(), crate::waits::on_owner_drawn(p).len() + crate::read_ask::asks(p).len(), p.live.tickets.len(), p.live.tickets.values().filter(|t| t.archived).count(), notes, p.live.refused().count(), p.live.project_name(), p.live.home()));
        v.debug(&s.project_tab).add(p.live.views.values().filter(|d| d.error.is_some()).count());
        for spec in p.live.view_specs() {
            v.debug(spec);
        }
        for spec in p.live.view_specs() {
            let id = spec.get("id").and_then(|x| x.as_str()).unwrap_or("");
            v.add((id, spec.get("title").and_then(|x| x.as_str()), p.live.views.get(id).and_then(|d| d.error.as_deref())));
        }
    }
    // the root keeps the key the Sidebar gives it
    rune::memo::memo_with_hash("sidebar", v.finish(), || built(s, k))
}

/// A pinned thread's words: the project's label for it, else its title, else the one last seen.
fn pin_label(p: &crate::Proj, pin: &Pin) -> String {
    let title = p.live.tickets.values().find(|t| t.id == pin.id).map(|t| t.title.trim().to_string()).filter(|t| !t.is_empty());
    p.labels.get(&pin.id).map(str::to_string).or(title).unwrap_or_else(|| pin.agent.clone())
}

/// Whether thread V has ended (done or failed); one at rest is still open.
fn closed(v: &coord_types::TicketView) -> bool {
    matches!(v.status, docket_machine::ViewStatus::Done { .. } | docket_machine::ViewStatus::Failed { .. })
}

/// A pinned thread's row text: the label cut with `…` when it is long, never the id (a reference is label and id).
fn row_text(label: &str, id: &str) -> String {
    const LONGEST: usize = 22;
    let cut: String = if label.chars().count() > LONGEST { label.chars().take(LONGEST - 1).collect::<String>().trim_end().to_string() + "…" } else { label.to_string() };
    format!("{cut} · {id}")
}

/// Row AT of the pins SHOWN under a project's Agents (their places in the list): it reorders them by a drag, the pins of
/// his other projects between them left in place ([`Pins::moved`]); dragged out of the sidebar, OUT takes it.
fn reorder_pin(row: SidebarRow, s: &App, at: usize, shown: &[usize], out: impl Fn(&mut App, &rune::Drag, &mut rune::Tx) -> bool + 'static) -> SidebarRow {
    let order = shown.to_vec();
    row.reorder_or_out(&s.pins.drag, at, shown.len(), |s: &mut App| &mut s.pins.drag, move |s: &mut App, from, to| s.pins.moved(&order, from, to), out)
}

/// The row of pinned thread PIN of the project at PI, row AT of the pins SHOWN under its Agents: its label now (else
/// the last seen) and its id quiet, "closed" beneath once it has ended; a click opens it, Unpin unpins it, a drag
/// reorders.
fn thread_row(s: &App, k: &Kit, pi: usize, at: usize, shown: &[usize], pin: &Pin) -> El {
    let p = &s.projects[pi];
    let ticket = p.live.tickets.values().find(|t| t.id == pin.id);
    let label = pin_label(p, pin);
    // a thread that has ended (or that the record no longer holds live) stays pinned and says so; a project that is
    // down says nothing of its threads
    let ended = p.down.is_none() && ticket.is_none_or(closed);
    let selected = pi == s.cur && s.sel == Sel::Tickets && ticket.is_some_and(|t| s.tickets.shown == Some(t.n));
    let id = pin.id.clone();
    // the pins' one height, as their reorder takes them (rune-kit `Reorder`)
    let mut row = SidebarRow::new(row_text(&label, &pin.id)).depth(1).height(40.0).selected(selected);
    if ended {
        row = row.subtitle("closed");
    }
    let key = format!("pin-{}-{}", pin.project, pin.id);
    if let Some(u) = unpin(s, k, &key, pin) {
        row = row.trailing(u);
    }
    let row = reorder_pin(row, s, at, shown, |_: &mut App, _, _| false)
        .on_select(move |s: &mut App, _| {
            s.cur = pi;
            // a thread closed more than a day ago is not in the snapshot: fetched, as Find does
            match crate::find::number(s, &id) {
                Ok(t) => crate::find::open_at(s, t, &id, None),
                Err(e) => s.notice = Some(e),
            }
        });
    hovered(row.build(k), key)
}

/// A pinned thread the owner archived, or one in the Archive (archive.rs): out of every list, so out of his pins too
/// (Unarchive, or Bring it all back, puts it back, pin kept).
fn archived_pin(s: &App, pin: &Pin) -> bool {
    s.projects.iter().find(|q| q.id == pin.project).is_some_and(|q| q.live.tickets.values().any(|t| t.id == pin.id && t.archived)) || crate::archive::pin_hidden(s, pin)
}

/// Whether lens L is the page shown (whatever thread it has open): the Threads lens is lit by the Threads page itself.
pub(crate) fn lens_shown(s: &App, l: &crate::lens::Lens) -> bool {
    match l.as_sel() {
        Sel::Lens(_) => matches!(&s.sel, Sel::Lens(x) if x.filter == l.filter && x.depth == l.depth),
        page => s.sel == page,
    }
}

/// Show lens L of project I: at its root, unless it is the page shown already (a press on its row keeps his thread).
fn show_lens(s: &mut App, i: usize, l: &crate::lens::Lens) {
    if s.cur == i && lens_shown(s, l) {
        return;
    }
    s.cur = i;
    s.show_sel(Sel::Lens(l.clone()));
}

/// The row of pinned lens L of the project at PI, row AT of the pins SHOWN under its Agents: its name; a click shows it,
/// a drag reorders the pins or, out of the sidebar, drops it into a pane (panes.rs).
fn lens_row(s: &App, k: &Kit, pi: usize, at: usize, shown: &[usize], pin: &Pin, l: &crate::lens::Lens) -> El {
    let selected = pi == s.cur && lens_shown(s, l);
    let name = l.name();
    let mut row = SidebarRow::new(name.clone()).depth(1).height(40.0).selected(selected);
    let (dragged, shown_l) = (l.clone(), l.clone());
    let key = format!("pin-{}-lens-{}", pin.project, l.filter.key());
    if let Some(u) = unpin(s, k, &key, pin) {
        row = row.trailing(u);
    }
    let row = reorder_pin(row, s, at, shown, move |s: &mut App, d, _| s.drag_sel(pi, Sel::Lens(dragged.clone()), &name, d))
        .on_select(move |s: &mut App, _| show_lens(s, pi, &shown_l));
    hovered(row.build(k), key)
}

/// The row of pinned agent PIN of the project at PI, row AT of the pins SHOWN under its Agents: its name now (a rename
/// since the pin was saved shows at once; else the last one seen), the one line the Agents page gives it (agents.rs
/// `side_line`) or that it stopped, busy while it works, its folder's fold where coordd gives it; a press opens its
/// page, his chat with it, a double-click beside the page shown; its close unpins it; a drag reorders the pins or drops
/// it into a pane.
fn agent_pin_row(s: &App, k: &Kit, pi: usize, at: usize, shown: &[usize], pin: &Pin) -> El {
    let p = &s.projects[pi];
    let agent = pin.find(&p.live.agents);
    let name = agent.map_or(pin.agent.clone(), |a| a.name.clone());
    let mut row = SidebarRow::new(name.clone()).depth(1).height(40.0).selected(pi == s.cur && s.sel == Sel::Agent(name.clone()));
    // a spin-off says what it is a copy of (his walk item 19) first, before its state, which a narrow row cuts
    let line = agent.and_then(|a| agents::side_line(p, a).or_else(|| (a.state == "stopped").then(|| "stopped".to_string())));
    let spun = agent.and_then(|a| a.from.as_ref()).map(|f| format!("spin-off of {f}"));
    if let Some(line) = match (line, spun) {
        (Some(l), Some(f)) => Some(format!("{f} · {l}")),
        (l, f) => l.or(f),
    } {
        row = row.subtitle(line);
    }
    let key = format!("pin-{}-{}", pin.project, if pin.id.is_empty() { &pin.agent } else { &pin.id });
    let (dragged, open, side) = (name.clone(), name.clone(), name.clone());
    let mut row = reorder_pin(row, s, at, shown, move |s: &mut App, d, _| s.drag_agent(pi, &dragged, d))
        .on_select(move |s: &mut App, _| s.open_in(pi, &open))
        .on_double_click(move |s: &mut App, _| s.open_to_side(pi, &side));
    if agent.is_some_and(|a| a.dialog.is_some()) {
        row = row.status(RowStatus::Attention);
    } else if agent.is_some_and(|a| a.state == "working" || a.compacting.is_some()) {
        row = row.status(RowStatus::Busy);
    }
    // its folder under it (the file view, paper-research N-1446 p1633), from a coordd that gives it; Unpin before it
    let fold = (agent.is_some() && folder_served(p)).then(|| fold_button(s, tree_key(pi, &folder_id(&name)), format!("agent-fold-{name}"), k));
    let end: Vec<El> = unpin(s, k, &key, pin).into_iter().chain(fold).collect();
    if !end.is_empty() {
        row = row.trailing(rune::row().key("row-end").gap(2.0).center_y().children(end));
    }
    hovered(row.build(k), key)
}

/// The Unpin of pin PIN, its row keyed KEY, while the pointer is on that row (`App::pin_hover`), else none: at rest its
/// name has the row's width (at his sidebar's 220 points, an Unpin beside an agent's fold cut "paper-research" to
/// "paper-rese…"). It goes before whatever else ends the row, so the fold he aims at stays under his pointer. It says
/// Unpin, not Close, which on a thread would read as closing the thread.
fn unpin(s: &App, k: &Kit, key: &str, pin: &Pin) -> Option<El> {
    (s.pin_hover.as_deref() == Some(key)).then(|| {
        let gone = pin.clone();
        let button = IconButton::new(&UNPIN).control_size(ControlSize::Small).variant(IconButtonVariant::GhostMuted).label("Unpin").help("Unpin");
        button.build(k).key("unpin").on_press(move |s: &mut App| s.pins.toggle(gone.clone()))
    })
}

/// Pin row EL keyed KEY, followed by the pointer for [`unpin`].
fn hovered(el: El, key: String) -> El {
    el.key(key.clone()).on_hover_change(move |s: &mut App, inside: bool, _: &mut rune::Tx| {
        if inside {
            s.pin_hover = Some(key.clone());
        } else if s.pin_hover.as_deref() == Some(key.as_str()) {
            s.pin_hover = None;
        }
    })
}

/// The fold key of node ID in the structure of the project at PI ("section": the section itself, folded when held); of
/// an agent's folder and its rows, under Agents, too ([`folder_id`], [`row_ids`]).
fn tree_key(pi: usize, id: &str) -> String {
    format!("tree|{pi}|{id}")
}

/// The fold key of the structure of the project at PI as a whole ([`tree_key`]'s "section"): held while it is folded,
/// as at a window's first start (state.rs `put_folds`).
pub(crate) fn section_key(pi: usize) -> String {
    tree_key(pi, "section")
}

/// The key of Agents and his pins beneath it: a drag dropped there is pinned (panes.rs `drop_zone`).
pub(crate) const AGENTS_PINS: &str = "agents-pins";

/// Each poll: the thread opened on the page shown, revealed in the structure as VS Code's explorer reveals the file
/// open: the threads above it unfolded, its row brought into view over a few frames while they open. Only when the
/// thread shown changes, so his own scrolling of the pane is left alone; a folded section stays folded.
pub fn reveal(s: &mut App, tx: &mut rune::Tx) -> bool {
    let (changed, key) = reveal_step(s);
    if let Some(key) = key {
        tx.scroll_into_view(key);
    }
    changed
}

/// What the structure last revealed ([`reveal_step`]): the thread open on the page shown, by its number; or, while the
/// rows are the Map's, where the Map is (canvas.rs `View::here`), by its node's name, as `View::sel` keeps a node,
/// since a lay moves the indices (proto-window N-1446 p1249 points 4 and 5).
#[derive(Clone, Debug, PartialEq)]
pub enum Revealed {
    Thread(u64),
    Node(crate::map::canvas::Name),
}

/// [`reveal`]'s step: whether anything changed, and the row to bring into view. While the structure's rows are the
/// Map's ([`map_island`]), [`reveal_node`].
pub(crate) fn reveal_step(s: &mut App) -> (bool, Option<String>) {
    if map_island(s, s.cur).is_some() {
        return reveal_node(s);
    }
    let shown = s.tickets.shown.filter(|_| s.sel.shows_threads()).map(|n| (s.cur, n));
    let row = |s: &App, pi: usize, n: u64| s.projects.get(pi).and_then(|p| crate::find::ticket(s, p, n)).map(|v| format!("tree-{}", v.id));
    match s.revealed {
        Some((pi, Revealed::Thread(n), left)) if shown == Some((pi, n)) => {
            if left == 0 {
                return (false, None);
            }
            s.revealed = Some((pi, Revealed::Thread(n), left - 1));
            return (true, row(s, pi, n));
        }
        _ => {}
    }
    s.revealed = shown.map(|(pi, n)| (pi, Revealed::Thread(n), 3));
    let Some((pi, n)) = shown else { return (false, None) };
    if s.streams.open.contains(&section_key(pi)) {
        return (false, None);
    }
    let Some(p) = s.projects.get(pi) else { return (false, None) };
    let Some(v) = crate::find::ticket(s, p, n) else { return (false, None) };
    let mut above = Vec::new();
    let mut x = v;
    while let Some(q) = crate::outline::parent(p, x) {
        above.push(q.id.clone());
        x = q;
    }
    for id in above {
        s.streams.open.insert(tree_key(pi, &id));
    }
    let key = row(s, pi, n);
    (true, key)
}

/// [`reveal_step`] while the structure's rows are the Map's: where the Map is (canvas.rs `View::here`), if it is of the
/// chosen project's island, lit (`Revealed::Node`), it and the rows above it unfolded (those that hold anything) and
/// its row brought into view over a few frames while they open; only when that node changes, so his own scrolling and
/// folding of the pane are left alone (p1126: "Moving or selecting on the Map lights and opens that node in the
/// tree"). A node selected or drawn at a depth at once; one that is only in view once a flight has landed, so the
/// nodes a flight passes over are not unfolded in his kept folds (proto-window N-1446 p1249 point 4). A folded section
/// stays folded.
fn reveal_node(s: &mut App) -> (bool, Option<String>) {
    let pi = s.cur;
    let Some((g, island)) = map_island(s, pi) else { return (false, None) };
    let here = s.map.here_kept().filter(|h| g.root_of(h.node) == island);
    if here.is_some_and(|h| h.in_view) && s.map.flying() {
        return (false, None);
    }
    let now = here.and_then(|h| s.map.name(h.node)).map(Revealed::Node);
    match &s.revealed {
        Some((p, was, left)) if *p == pi && now.as_ref() == Some(was) => {
            if *left == 0 {
                return (false, None);
            }
            let left = left - 1;
            if let Some(x) = s.revealed.as_mut() {
                x.2 = left;
            }
            return (true, here.map(|h| format!("tree-{}", g.node(h.node).id)));
        }
        None if now.is_none() => return (false, None),
        _ => {}
    }
    s.revealed = now.map(|r| (pi, r, 3));
    let Some(h) = here.filter(|_| !s.streams.open.contains(&section_key(pi))) else { return (true, None) };
    for a in g.path(h.node).into_iter().skip(1).filter(|a| !g.children(*a).is_empty()) {
        s.streams.open.insert(tree_key(pi, &g.node(a).id));
    }
    (true, Some(format!("tree-{}", g.node(h.node).id)))
}

/// The Map's node the structure lights, by its name ([`reveal_node`]).
fn lit_name(s: &App) -> Option<&crate::map::canvas::Name> {
    match &s.revealed {
        Some((pi, Revealed::Node(n), _)) if *pi == s.cur => Some(n),
        _ => None,
    }
}

/// The Map's graph and project PI's island in it, while a pane draws the Map (canvas.rs `drawn_in`): the structure's
/// rows are then the Map's nodes ([`node_rows`]; paper-research N-1446 p1126: "the same nodes as the Map, in the same
/// order"). None while no pane draws it: the graph is laid only while one does (canvas.rs `poll`), so its rows would
/// not follow the record, and the rows are the outline's ([`tree_rows`]).
fn map_island(s: &App, pi: usize) -> Option<(std::sync::Arc<crate::map::Graph>, u32)> {
    crate::map::canvas::drawn_in(s)?;
    let g = s.map.graph()?.clone();
    let island = g.project(&crate::map::canvas::project_id(s, pi)?)?;
    Some((g, island))
}

/// Project P's structure as it is (the owner, N-1104 p599): its open threads that stand on their own, newest started
/// first, so that a row keeps its place (his N-281.p9: "order them by date started so they are always in the same
/// order"), each unfolding to its subthreads; no groups, no order by what waits (those are the Threads lens's). Folded
/// at a window's first start, and after it as he left it, over every start and swap (state.rs `structure_open`;
/// paper-research N-1446 p1938, 2; his N-282.p3: "yo udont have ot revela everytihng at onc"). None while the project has no
/// thread at all, open or ended: a new project's left bar shows only what exists, so no "Structure 0" and no Insights of nothing (docket-edge7's
/// walk, N-1446 p3050 6; paper-research p3051).
///
/// His threads are its rows, as on Threads: the threads he takes no part in, with their subthreads (outline.rs
/// `apart`), stand behind one row at its foot, "Between agents", folded until he unfolds it, and its count is his
/// alone (yaaihh/fresh N-1751 p6: "the left bar still shows all four threads under Structure ... beside a list of
/// two"). While the Map is drawn its rows are the Map's nodes, as they were.
fn structure(s: &App, p: &crate::Proj, k: &Kit) -> Option<El> {
    let pi = s.cur;
    let island = map_island(s, pi);
    let mut top: Vec<&coord_types::TicketView> = Vec::new();
    let mut others: Vec<&coord_types::TicketView> = Vec::new();
    if island.is_none() {
        // what his Archive holds out (archive.rs): now the threads opened before its moment, there those after
        top = p.live.tickets.values().filter(|t| crate::outline::outlined(t) && crate::outline::open(t) && crate::outline::parent(p, t).is_none() && !crate::archive::hides(s, p, t)).collect();
        top.sort_by_key(|t| std::cmp::Reverse(t.n));
        (top, others) = top.into_iter().partition(|t| !crate::outline::apart(t, &crate::outline::subthreads(p, t)));
    }
    let count = island.as_ref().map_or(top.len(), |(g, i)| g.children(*i).len());
    if count == 0 && p.live.tickets.is_empty() {
        return None;
    }
    let section = section_key(pi);
    let folded = s.streams.open.contains(&section);
    // a row of the project's fold (N-1934 p4: nothing of a project's outside it), its rows beneath it while unfolded
    let mark = if folded { "▸" } else { "▾" };
    let head = SidebarRow::new("Structure").badge(count.to_string()).detail(mark.to_string()).height(32.0).on_select(move |s: &mut App, _| {
        if !s.streams.open.remove(&section) {
            s.streams.open.insert(section.clone());
        }
    });
    let mut sec = rune::col().key("structure").role("group").label("Structure").w_full().gap(2.0).child(head.build(k).key("section-header"));
    if !folded {
        let mut rows = Vec::new();
        insight_rows(s, pi, &mut rows, k);
        match &island {
            Some((g, i)) => inside_rows(s, pi, g, *i, 0, &Lit::of(s, g), &mut rows, k),
            None => {
                for t in top {
                    tree_rows(s, p, pi, t, 0, &mut rows, k);
                }
                between_rows(s, p, pi, &others, &mut rows, k);
            }
        }
        sec = sec.children(rows);
    }
    Some(sec)
}

/// The row "Between agents" at the foot of the structure of the project at PI, with how many it holds after it, and,
/// while he has it unfolded, OTHERS under it, the threads he takes no part in, each as [`tree_rows`] draws it; no row
/// when there are none ([`structure`]).
fn between_rows(s: &App, p: &crate::Proj, pi: usize, others: &[&coord_types::TicketView], rows: &mut Vec<El>, k: &Kit) {
    if others.is_empty() {
        return;
    }
    let key = tree_key(pi, "between-agents");
    let button = fold_button(s, key.clone(), "tree-fold-between-agents".into(), k);
    let n = others.len();
    let words = rune::text(n.to_string()).key("detail").font(k.ty.small).color(k.c.text_faint).a11y_hidden();
    let press = key.clone();
    let end = rune::row().key("row-end").gap(6.0).center_y().shrink().child(words).child(button);
    let said = format!("{n} thread{}", if n == 1 { "" } else { "s" });
    rows.push(SidebarRow::new("Between agents").title_style(k.ty.small).depth(1).height(28.0).on_select(move |s: &mut App, _| toggle_fold(s, &press)).trailing(end).build(k).key("tree-between-agents").description(said));
    if s.streams.open.contains(&key) {
        for t in others {
            tree_rows(s, p, pi, t, 1, rows, k);
        }
    }
}

/// The Map's row, the first of the entries of the project at PI (I-1478; N-1104 p715: the Map is the main view, opened
/// from here; paper-research N-1446 p1938, 1: Map · Threads · Agents · Project), into ROWS: a press shows it, a drag
/// drops it into a pane. Keyed as when it stood at the structure's head. While it is shown, a thread's row in the
/// structure flies the Map there ([`tree_rows`]). Offered where PI's coordd serves the graph (map/lanes.rs
/// `serves_in`), which the cloud asks once the Map is drawn, or once the Map has a graph to draw (docket-edge5's
/// map/layout.rs gives an older coordd's project one): a page that would never fill is not offered. Build 68 waited
/// for a graph alone, which came only once the Map was drawn, opened only from this row: he never saw it (his N-1104
/// p835; paper-research N-1446 p1928).
fn map_row(s: &App, pi: usize, rows: &mut Vec<El>, k: &Kit) {
    if s.map.graph().is_none() && !crate::map::lanes::serves_in(s, pi) {
        return;
    }
    let l = crate::lens::Lens { filter: crate::lens::Filter::Threads, depth: crate::lens::Depth::Cloud, at: None };
    let (shown, dragged) = (l.clone(), l);
    let row = SidebarRow::new("Map")
        .height(32.0)
        .selected(s.cur == pi && crate::map::canvas::shown(s))
        .on_select(move |s: &mut App, _| show_lens(s, pi, &shown))
        .on_drag(move |s: &mut App, d, _| _ = s.drag_sel(pi, Sel::Lens(dragged.clone()), "Map", d));
    rows.push(row.build(k).key("tree-map"));
}

/// The Insights node of the structure of the project at PI (S6, proto-lead N-1446 p388: each insight "opened from the
/// structure pane like any lens"), folded until he unfolds it, into ROWS; under it each insight's row: a press shows its
/// lens (at the number it kept, else its first), a drag drops it into a pane.
fn insight_rows(s: &App, pi: usize, rows: &mut Vec<El>, k: &Kit) {
    let key = tree_key(pi, "insights");
    let open = s.streams.open.contains(&key);
    let button = fold_button(s, key.clone(), "tree-fold-insights".into(), k);
    rows.push(SidebarRow::new("Insights").title_style(k.ty.small).depth(1).height(28.0).on_select(move |s: &mut App, _| toggle_fold(s, &key)).trailing(button).build(k).key("tree-insights"));
    if !open {
        return;
    }
    for l in crate::insight::lenses() {
        let (name, key) = (l.name(), format!("tree-insight-{}", l.filter.key()));
        let (shown, dragged) = (l.clone(), l.clone());
        let row = SidebarRow::new(name.clone())
            .title_style(k.ty.small)
            .depth(2)
            .height(28.0)
            .selected(s.cur == pi && lens_shown(s, &l))
            .on_select(move |s: &mut App, _| show_lens(s, pi, &shown))
            .on_drag(move |s: &mut App, d, _| _ = s.drag_sel(pi, Sel::Lens(dragged.clone()), &name, d));
        rows.push(row.build(k).key(key));
    }
}

/// One row of the structure's tree, the outline's ([`tree_rows`]) or the Map's ([`node_rows`]): the id its key and
/// fold are named by, its label and what it says after it, at DEPTH; lit; marked when it waits on him; a fold button
/// when anything lies under it; a drag dropping it into a pane as the lens DRAG.
struct TreeRow {
    id: String,
    label: String,
    detail: String,
    depth: u8,
    selected: bool,
    waits: bool,
    under: bool,
    drag: Option<crate::lens::Lens>,
}

/// Row R of the structure of the project at PI, a press doing PRESS. What it says after its label and its fold lie at
/// its end together, its words cut before the fold is (rune-kit's `SidebarRow::detail` keeps its words whole, so a deep
/// row's pushed its fold past the sidebar's edge: a commit in an agent's folder at the sidebar's 196 points); they are
/// said as its description, then its mark's words, as `SidebarRow` says them.
fn tree_row(s: &App, pi: usize, r: TreeRow, press: impl Fn(&mut App) + 'static, k: &Kit) -> El {
    let key = tree_key(pi, &r.id);
    let mut row = SidebarRow::new(r.label.clone())
        .title_style(k.ty.small)
        .depth(r.depth + 1)
        .height(28.0)
        .selected(r.selected)
        .on_select(move |s: &mut App, _| press(s));
    if let Some(lens) = r.drag {
        let name = r.label;
        row = row.on_drag(move |s: &mut App, d, _| _ = s.drag_sel(pi, Sel::Lens(lens.clone()), &name, d));
    }
    if r.waits {
        row = row.status(RowStatus::Attention);
    }
    let words = (!r.detail.is_empty()).then(|| rune::text(r.detail.clone()).key("detail").font(k.ty.small).color(k.c.text_faint).truncate().shrink().a11y_hidden());
    let fold = r.under.then(|| fold_button(s, key, format!("tree-fold-{}", r.id), k));
    if words.is_some() || fold.is_some() {
        row = row.trailing(rune::row().key("row-end").gap(6.0).center_y().shrink().children(words).children(fold));
    }
    let said: Vec<&str> = [r.detail.as_str(), if r.waits { "Needs attention" } else { "" }].into_iter().filter(|w| !w.is_empty()).collect();
    row.build(k).key(format!("tree-{}", r.id)).description(said.join(", "))
}

/// The fold button at the end of a row, keyed EL: ▸ while what lies under it is folded, ▾ while it is open; a press
/// folds or unfolds it, kept under KEY among the folds he left ([`toggle_fold`]).
fn fold_button(s: &App, key: String, el: String, k: &Kit) -> El {
    let open = s.streams.open.contains(&key);
    rune_kit::Button::new(if open { "▾" } else { "▸" }).variant(rune_kit::ButtonVariant::GhostMuted).on_press(move |s: &mut App| toggle_fold(s, &key)).build(k).key(el)
}

/// What lies under the row whose fold key is KEY unfolded if it was folded, else folded.
fn toggle_fold(s: &mut App, key: &str) {
    if !s.streams.open.remove(key) {
        s.streams.open.insert(key.to_string());
    }
}

/// Thread T's row in the structure at DEPTH, and its subthreads' under it while it is unfolded, into ROWS. A click
/// opens it on the page shown; a drag drops it into a pane as the Threads lens at it.
fn tree_rows(s: &App, p: &crate::Proj, pi: usize, t: &coord_types::TicketView, depth: u8, rows: &mut Vec<El>, k: &Kit) {
    let under = crate::outline::subthreads(p, t);
    let (n, id) = (t.n, t.id.clone());
    let r = TreeRow {
        id: t.id.clone(),
        label: crate::outline::label(p, t),
        detail: t.id.clone(),
        depth,
        selected: s.sel.shows_threads() && s.tickets.shown == Some(t.n),
        waits: crate::outline::state(t).1,
        under: !under.is_empty(),
        drag: Some(crate::lens::Lens::threads_at(&t.id)),
    };
    rows.push(tree_row(
        s,
        pi,
        r,
        move |s: &mut App| {
            if !crate::map::canvas::fly_to_id(s, pi, &id) {
                crate::find::open_at(s, n, &id, None)
            }
        },
        k,
    ));
    if s.streams.open.contains(&tree_key(pi, &t.id)) {
        for c in under {
            tree_rows(s, p, pi, c, depth + 1, rows, k);
        }
    }
}

/// At most how many of what lies inside a node the structure's tree lists ([`listed`]), and of a list in an agent's
/// folder ([`block_rows`]): in the made-up record his chat holds 700 posts and a topic up to 150 lines, docket-edge5's
/// folder 140 commits (run 14), and a section builds every row it is given.
const LISTED: usize = 40;

/// What of the Map the structure's tree lights: where the Map is as [`reveal_node`] last revealed it, its chain of
/// parents, and the depth it is drawn at (canvas.rs `View::here`, which the crumbs end with).
struct Lit {
    here: Option<u32>,
    path: Vec<u32>,
    depth: Option<crate::lens::Depth>,
}

impl Lit {
    fn of(s: &App, g: &crate::map::Graph) -> Lit {
        let here = lit_name(s).and_then(|n| s.map.pick(n));
        let depth = s.map.here().filter(|h| Some(h.node) == here).and_then(|h| h.depth);
        Lit { here, path: here.map(|i| g.path(i)).unwrap_or_default(), depth }
    }
}

/// Which of what lies inside node I of G the tree lists, as a range of its children: all of them up to [`LISTED`];
/// past it, the fixed block of LISTED holding the one on the chain PATH (proto-window N-1446 p1249 point 1: a press on
/// a row lights its node, and a block never moves under his pointer). A line's blocks are counted back from its latest
/// post, so its latest block is whole and, with none of it lit, the one listed (the Map opens a line at its latest
/// posts, canvas.rs `fit_of`); any other node's from its first.
fn listed(g: &crate::map::Graph, i: u32, path: &[u32]) -> (usize, usize) {
    let kids = g.children(i);
    let line = line_kind(g.node(i).kind);
    let at = kids.iter().position(|c| path.contains(c)).unwrap_or(if line { kids.len().saturating_sub(1) } else { 0 });
    block(kids.len(), at, line)
}

/// The range a tree lists of a list of N ([`listed`], [`block_rows`]): all of it up to [`LISTED`]; past it, the fixed
/// block of LISTED that holds the one at AT (the last, when the list has shrunk to it or before it), counted back from
/// the last when LATEST, else from the first.
fn block(n: usize, at: usize, latest: bool) -> (usize, usize) {
    if n <= LISTED {
        return (0, n);
    }
    let at = at.min(n - 1);
    if latest {
        let end = n - (n - 1 - at) / LISTED * LISTED;
        return (end.saturating_sub(LISTED), end);
    }
    let a = at / LISTED * LISTED;
    (a, (a + LISTED).min(n))
}

/// Whether a node of kind K is a line, whose posts or runs lie inside it oldest first.
fn line_kind(k: crate::map::Kind) -> bool {
    matches!(k, crate::map::Kind::Thread | crate::map::Kind::Item | crate::map::Kind::Handler)
}

/// The rows of what lies inside node I of the Map's graph G, at DEPTH, each with what lies inside it while it is
/// unfolded ([`node_rows`]), as many as [`listed`] gives, with a row at each cut ([`cut_row`]).
#[allow(clippy::too_many_arguments)]
fn inside_rows(s: &App, pi: usize, g: &crate::map::Graph, i: u32, depth: u8, lit: &Lit, rows: &mut Vec<El>, k: &Kit) {
    let kids = g.children(i);
    let (a, b) = listed(g, i, &lit.path);
    if a > 0 {
        rows.push(cut_row(pi, g, i, kids[a - 1], a, true, depth, k));
    }
    // a line's ticket, once, for its posts' marks
    let ticket = s.projects.get(pi).filter(|_| line_kind(g.node(i).kind)).and_then(|p| p.live.tickets.values().find(|t| t.id == g.node(i).id));
    for c in &kids[a..b] {
        node_rows(s, pi, g, *c, ticket, depth, lit, rows, k);
    }
    if b < kids.len() {
        rows.push(cut_row(pi, g, i, kids[b], kids.len() - b, false, depth, k));
    }
}

/// The row at a cut of what node I of G lists ([`listed`]), at DEPTH: how many it leaves out, above it (UP) or below;
/// a press flies the Map to TO, the first past the cut, which the tree then lights, so the rows listed move to it.
#[allow(clippy::too_many_arguments)]
fn cut_row(pi: usize, g: &crate::map::Graph, i: u32, to: u32, left: usize, up: bool, depth: u8, k: &Kit) -> El {
    let words = match (line_kind(g.node(i).kind), up) {
        (true, true) => "earlier",
        (true, false) => "later",
        (false, _) => "more",
    };
    let id = g.node(to).id.clone();
    let key = format!("tree-cut-{}-{}", g.node(i).id, if up { "up" } else { "down" });
    cut_el(key, format!("{left} {words}"), depth, move |s: &mut App| _ = crate::map::canvas::fly_to_id(s, pi, &id), k)
}

/// A row at a cut of a list the tree lists, keyed KEY, at DEPTH: WORDS, how many it leaves out; a press does PRESS.
fn cut_el(key: String, words: String, depth: u8, press: impl Fn(&mut App) + 'static, k: &Kit) -> El {
    SidebarRow::new(words).title_style(k.ty.small).depth(depth + 1).height(28.0).on_select(move |s: &mut App, _| press(s)).build(k).key(key)
}

/// What a node's row in the structure says after its label: a thread's or an item's id, as the outline's rows do, a
/// post's number, else what it is as the Map's card says it (canvas.rs `said_kind`): a system prompt where it said "text
/// a session read" under "docket-edge5's system prompt, version 2" (proto-window's walk, N-1446 p3038 4).
fn node_detail(n: &crate::map::Node) -> String {
    match n.kind {
        crate::map::Kind::Thread | crate::map::Kind::Item => n.id.clone(),
        crate::map::Kind::Post => crate::lens::parts(&n.id).1.map_or_else(|| "post".into(), |p| format!("p{p}")),
        _ => crate::map::canvas::said_kind(n).into(),
    }
}

/// Node I of the Map's graph G as a row of the structure at DEPTH, and while it is unfolded what lies inside it
/// ([`inside_rows`]), into ROWS: its label as the Map has it; lit where the Map is (LIT), its label then the depth it
/// is drawn at, in the crumbs' words ("his post 700 · Events" where they end "› his post 700 › Events"; paper-research
/// N-1446 p1244, proto-window p1249 point 3); marked as its thread's row or its post's ring is (TICKET: the line it
/// lies in, if it is a post; point 7); a press flies the Map there (canvas.rs `fly_to_id`); a thread's or an item's
/// drag drops the Threads lens at it, as the outline's.
#[allow(clippy::too_many_arguments)]
fn node_rows(s: &App, pi: usize, g: &crate::map::Graph, i: u32, ticket: Option<&coord_types::TicketView>, depth: u8, lit: &Lit, rows: &mut Vec<El>, k: &Kit) {
    let n = g.node(i);
    let here = lit.here == Some(i);
    let label = match lit.depth.filter(|_| here) {
        Some(l) => format!("{} · {}", n.label, l.word()),
        None => n.label.clone(),
    };
    let waits = match n.kind {
        crate::map::Kind::Post => ticket.zip(crate::lens::parts(&n.id).1).and_then(|(v, p)| v.posts.iter().find(|q| q.n == p).map(|q| crate::threads::open_to_owner(v, q))).unwrap_or(false),
        kind if line_kind(kind) => s.projects.get(pi).and_then(|p| p.live.tickets.values().find(|t| t.id == n.id)).is_some_and(|t| crate::outline::state(t).1),
        _ => false,
    };
    let under = !g.children(i).is_empty();
    let r = TreeRow {
        id: n.id.clone(),
        label,
        detail: node_detail(n),
        depth,
        selected: here,
        waits,
        under,
        drag: matches!(n.kind, crate::map::Kind::Thread | crate::map::Kind::Item).then(|| crate::lens::Lens::threads_at(&n.id)),
    };
    let id = n.id.clone();
    rows.push(tree_row(s, pi, r, move |s: &mut App| _ = crate::map::canvas::fly_to_id(s, pi, &id), k));
    if under && s.streams.open.contains(&tree_key(pi, &n.id)) {
        inside_rows(s, pi, g, i, depth + 1, lit, rows, k);
    }
}

fn built(s: &App, k: &Kit) -> El {
    if s.rail || s.narrow.get() {
        return rail(s, k);
    }
    let plus = IconButton::new(&PLUS).control_size(ControlSize::Small).variant(IconButtonVariant::GhostMuted).label("New project").help("New project…");
    let plus = plus.build(k).on_press(|s: &mut App| s.new_project.open = true);
    let mut sb = Sidebar::new().label("yaaihh");
    // the projects, the current one unfolded beneath its row
    let projects: Vec<El> = s.projects.iter().enumerate().map(|(i, q)| {
        let link = match &q.down {
            // slow over the whole project says so where the project is chosen (N-412 ask 5)
            None if agents::waiting(q) > 0 => format!("{} · {} running", agents::waiting_words(agents::waiting(q)), agents::active(q).count()),
            None if q.live.slow_project() => format!("{} running · slow", agents::active(q).count()),
            // the unfolded one's count is its Agents row's, said once
            None if i == s.cur => String::new(),
            None => format!("{} running", agents::active(q).count()),
            Some(why) => first_line(why),
        };
        let row = SidebarRow::new(q.name.clone());
        let mut row = if link.is_empty() { row } else { row.subtitle(link) }.height(40.0).selected(i == s.cur).on_select(move |s: &mut App, _| s.cur = i);
        if q.down.is_some() || agents::waiting(q) > 0 {
            row = row.status(RowStatus::Attention);
        }
        // agents' mail held by him (line 327; consult-lead, N-1324 post 7): said where the project is chosen, so nobody
        // wonders why its agents are quiet
        if q.down.is_none() && q.live.mail_hold.is_some() {
            row = row.badge("held".to_string());
        }
        // its menu at a secondary press (projects.rs `menu`)
        row.build(k).key(format!("project-{i}")).on_secondary_press(move |s: &mut App, at, _| s.project_menu = Some((i, at)))
    }).collect();
    let narrow = IconButton::new(&NARROW).control_size(ControlSize::Small).variant(IconButtonVariant::GhostMuted).label("Sidebar to icons").help("Sidebar to icons");
    let narrow = narrow.build(k).on_press(to_rail);
    let Some(p) = s.proj() else {
        let foot = rune::row().key("link").w_full().h(32.0).center_y().child(rune::el().grow(1.0)).child(narrow);
        return sb.section(SidebarSection::new("Projects").key("projects").action(plus).rows(projects)).footer(global_rows(s, k, foot)).build(k);
    };
    // the search field, on every page: at the sidebar's top, fixed; over a list of threads it searches that list
    // (find.rs `in_sidebar`, paper-research N-1446 p2121)
    if let Some(f) = crate::find::sidebar_field(s, k) {
        sb = sb.header(f);
    }
    // the current project's entries: the Map's (paper-research N-1446 p1938, 1), then his three (the owner,
    // paper-research N-101 p758): Threads (its boards, Items, a filter in it), Agents, and Project (the home, kinds,
    // decisions, files, harnesses as its tabs: panes.rs project_tabs); beneath each only while it is open what it holds
    let mut rows = Vec::new();
    map_row(s, s.cur, &mut rows, k);
    let boards: Vec<String> = crate::panes::boards(p).into_iter().map(|(id, _)| id).collect();
    let in_threads = match &s.sel {
        Sel::Tickets | Sel::Mail => true,
        Sel::View(v) => boards.contains(v) || (v.is_empty() && boards.contains(&p.live.home())),
        _ => false,
    };
    let owed = crate::waits::count(p);
    // its count the open threads, as the Threads page's All active counts them (outline.rs `active`; docket-edge7's
    // walk N-1446 p3050, 7: "Threads 0" with its one thread open, where it counted what waits on him, paper-research
    // p3051); what waits on him marks the row, and Threads lists it first under Waits on you
    let mut tickets = SidebarRow::new("Threads")
        .badge(crate::outline::active(s, p).to_string())
        .height(32.0)
        .selected(in_threads)
        .on_select(|s: &mut App, _| s.sel = Sel::Tickets);
    if owed > 0 {
        tickets = tickets.status(RowStatus::Attention);
    }
    rows.push(tickets.build(k).key("tickets"));
    // the agents' stars for him (the owner, N-361): a filter of Threads with its count, beneath it on every page while
    // there is one (paper-research N-1446 p2121; until then only while Threads was open, as its other filters are)
    let (stars, any_star) = (crate::stars::count(p), crate::stars::any(p, s.tickets.find.owned.get(&p.id).map_or(&[][..], |v| v)));
    let star_list = in_threads && s.tickets.find.shown == Some(crate::find::Shown::ForOwner);
    if any_star || star_list {
        // the number is what waits for him (an agent's star not opened yet), never his own stars
        let mut row = SidebarRow::new("Starred").depth(1).height(30.0).selected(star_list).on_select(|s: &mut App, _| {
            s.sel = Sel::Tickets;
            s.tickets.shown = None;
            s.tickets.find.shown = Some(crate::find::Shown::ForOwner);
            if let Some(p) = s.proj() {
                let (id, mine) = (p.id.clone(), crate::stars::ask_own(p));
                s.tickets.find.owned.insert(id, mine);
            }
        });
        if stars > 0 {
            row = row.badge(stars.to_string());
        }
        rows.push(row.build(k).key("threads-starred"));
    }
    // its filters beneath it while it is open (the owner, N-358: one menu, on the left): all threads, a board's items
    let filters = crate::panes::boards(p);
    if in_threads && !filters.is_empty() {
        let all = matches!(s.sel, Sel::Tickets | Sel::Mail);
        rows.push(SidebarRow::new("All threads").depth(1).height(30.0).selected(all).on_select(|s: &mut App, _| s.sel = Sel::Tickets).build(k).key("threads-all"));
        for (id, title) in filters {
            let open = Sel::View(id.clone());
            let on = s.sel == open || (matches!(&s.sel, Sel::View(v) if v.is_empty()) && p.live.home() == id);
            rows.push(SidebarRow::new(title).depth(1).height(30.0).selected(on).on_select(move |s: &mut App, _| s.sel = open.clone()).build(k).key(format!("threads-{id}")));
        }
    }
    // Agents, how many of its agents run said in words (his N-1104 p863: "so x running"); beneath it, on every page,
    // only what he pinned in this project, in his order: the agents by their teams are the Agents page's (agents.rs
    // `grid`; paper-research N-1446 p2008, p2016). Agents and its pins are where a drag is dropped to pin (panes.rs
    // `drop_zone`). Agents is lit on its page, and on an agent's page (its chat, its session) unless that agent's own
    // pin below is lit: one entry says where he is, as the rail's icon does (`rail`; the walk's 2, N-1446 p2120)
    let pin_lit = |a: &str| s.pins.list.iter().any(|pin| pin.project == p.id && !pin.thread && pin.lens.is_none() && pin.find(&p.live.agents).map_or(pin.agent.as_str(), |x| x.name.as_str()) == a);
    let agents_lit = match &s.sel {
        Sel::Agents | Sel::Raw(_) => true,
        Sel::Agent(a) => !pin_lit(a),
        _ => false,
    };
    let mut pinned = vec![
        SidebarRow::new("Agents")
            .detail(match agents::waiting(p) {
                0 => format!("{} running", agents::active(p).count()),
                n => agents::waiting_words(n),
            })
            .height(32.0)
            .selected(agents_lit)
            .on_select(|s: &mut App, _| s.sel = Sel::Agents)
            .build(k)
            .key("agents"),
    ];
    let shown: Vec<usize> = s.pins.list.iter().enumerate().filter(|(_, pin)| pin.project == p.id && !(pin.thread && archived_pin(s, pin))).map(|(at, _)| at).collect();
    for (at, pin) in shown.iter().map(|&i| &s.pins.list[i]).enumerate() {
        match &pin.lens {
            Some(l) => pinned.push(lens_row(s, k, s.cur, at, &shown, pin, l)),
            None if pin.thread => pinned.push(thread_row(s, k, s.cur, at, &shown, pin)),
            None => {
                pinned.push(agent_pin_row(s, k, s.cur, at, &shown, pin));
                if let Some(a) = pin.find(&p.live.agents) {
                    folder_rows(s, s.cur, &a.name, 1, &mut pinned, k);
                }
            }
        }
    }
    rows.push(rune::col().key(AGENTS_PINS).w_full().gap(2.0).children(pinned));
    // the project's other pages: one entry, opening the tab last shown; a kind's or the policy's refusals, a look that
    // was not taken and a view's error stand out on it
    let tabs = crate::panes::project_tabs(p);
    let open_at = project_open_at(s, p, &tabs);
    let mut project = SidebarRow::new("Project").height(32.0).selected(crate::panes::is_project_tab(p, &s.sel)).on_select(move |s: &mut App, _| s.sel = open_at.clone());
    let faults = project_faults(p);
    if faults > 0 {
        project = project.badge(faults.to_string()).status(RowStatus::Attention);
    }
    rows.push(project.build(k).key("project-pages"));
    // its pages beneath it while one of them is open: what the bar across the page's top held (the owner, N-358)
    if crate::panes::is_project_tab(p, &s.sel) {
        let shown = crate::panes::tab_of(p, &s.sel);
        for (label, page) in tabs {
            let on = shown.as_ref() == Some(&page);
            let go = page.clone();
            rows.push(
                SidebarRow::new(label.clone())
                    .depth(1)
                    .height(30.0)
                    .selected(on)
                    .on_select(move |s: &mut App, _| {
                        // an extension's entry opens on the page of it last shown
                        let t = match &go {
                            Sel::Ext(n, _) => s.ext_last.get(n).cloned().unwrap_or_else(|| go.clone()),
                            _ => go.clone(),
                        };
                        s.sel = t.clone();
                        s.project_tab = Some(t);
                    })
                    .build(k)
                    .key(format!("project-tab-{label}")),
            );
            // what it opens to, beneath it while it is open: an extension's pages (the proxy's), a step further in
            if on
                && let Sel::Ext(name, _) = &page
                && let Some(e) = ext::get(name).filter(|e| ext::has_pages(e))
            {
                rows.extend((e.rows)(s, k, 2));
            }
        }
    }
    // everything up to his Archive's moment, after Project, while there is one (archive.rs)
    rows.extend(crate::archive::sidebar_row(s, k));
    rows.extend(structure(s, p, k));
    // beneath the project's own row, no second row of its name (N-1446 p1938)
    let title = p.live.project_name().unwrap_or(&p.name).to_string();
    let unfolded = rune::col().key("project").role("group").label(title).w_full().gap(2.0).pl(UNFOLD_PAD).children(rows);
    let mut listed = projects;
    listed.insert((s.cur + 1).min(listed.len()), unfolded);
    sb = sb.section(SidebarSection::new("Projects").key("projects").action(plus).rows(listed));
    // the link said once: while it is live the project lit above says so and the footer holds its button alone; the
    // project and why there only while it is down (N-1446 p1938)
    let footer = match &p.down {
        None => rune::row().key("link").w_full().h(32.0).center_y().child(rune::el().grow(1.0)).child(narrow),
        Some(why) => SidebarRow::new(p.name.clone()).subtitle(first_line(why)).height(40.0).trailing(narrow).build(k).key("link"),
    };
    sb.footer(global_rows(s, k, footer)).build(k)
}

/// The window's own pages, in the sidebar's foot and the rail's: each one's words, key, icon, whether it is shown, and
/// the press that opens it. Settings is a project's engine's to answer, so none while there is no project.
fn global_pages(s: &App) -> Vec<(&'static str, &'static str, &'static Icon, bool, fn(&mut App))> {
    let view = |id: &str| matches!(&s.sel, Sel::View(v) if v == id);
    let usage = s.sel == crate::global::usage_page();
    let machine = matches!(&s.sel, Sel::Ext(n, _) if n == crate::harness::NAME);
    let accounts = !usage && !machine && matches!(&s.sel, Sel::Ext(n, _) if ext::get(n).is_some_and(ext::global));
    let mut pages: Vec<(&'static str, &'static str, &'static Icon, bool, fn(&mut App))> = vec![
        ("Home", "global-home", &HOME, view(crate::global::ID), |s: &mut App| s.sel = Sel::View(crate::global::ID.into())),
        ("Usage", "global-usage", &USAGE, usage, |s: &mut App| s.sel = crate::global::usage_page()),
        ("Accounts", "global-accounts", &ACCOUNTS, accounts, |s: &mut App| s.sel = crate::ext::tab_page(crate::accounts::NAME)),
    ];
    if !s.projects.is_empty() {
        pages.push(("Settings", "global-settings", &SETTINGS, view(crate::policy::ID) || machine, |s: &mut App| crate::policy::open(s, crate::policy::Scope::Global)));
    }
    pages
}

/// The window's own pages, at the sidebar's foot over LINK and never under a project: Home (the machines), Usage,
/// Accounts and Settings.
fn global_rows(s: &App, k: &Kit, link: El) -> El {
    let accounts = global_pages(s).iter().any(|&(_, key, _, on, _)| key == "global-accounts" && on);
    let mut c = rune::col().key("global").role("group").label("yaaihh").w_full().gap(2.0).pt(8.0);
    for (words, key, _, on, go) in global_pages(s) {
        c = c.child(SidebarRow::new(words).height(30.0).selected(on).on_select(move |s: &mut App, _| go(s)).build(k).key(key));
        // the pages of an extension that took the accounts' tab (the proxy's), beneath it while it is open; its Usage is
        // the foot's own row, so its rows leave it out (`Extension::window_pages`)
        if key == "global-accounts" && accounts
            && let Sel::Ext(name, _) = &s.sel
            && let Some(e) = ext::get(name).filter(|e| ext::has_pages(e))
        {
            c = c.children((e.rows)(s, k, 1));
        }
        // this machine's harnesses, beneath Settings while it is open: the window's, never a project's tab
        if key == "global-settings" && on {
            let here = matches!(&s.sel, Sel::Ext(n, _) if n == crate::harness::NAME);
            let row = SidebarRow::new("Harnesses").depth(1).height(30.0).selected(here);
            c = c.child(row.on_select(|s: &mut App, _| s.sel = Sel::Ext(crate::harness::NAME.into(), String::new())).build(k).key("global-harnesses"));
        }
    }
    c.child(link)
}

// ---- an agent's folder (the file view) ----------------------------------------------------------------------------

/// The agents' folders (the file view, paper-research N-1446 p1441; under their rows in Agents, p1633): each agent's
/// rows as coordd last gave them (docket-machine `Graph::folder`), asked while its row is unfolded ([`folders_poll`]).
#[derive(Default)]
pub struct Folders {
    /// Each folder by project and agent: the mark it was asked at ([`folder_mark`]), its rows or why there are none.
    pub(crate) got: std::collections::HashMap<(usize, String), (String, Result<Vec<FolderRow>, String>)>,
    /// The asks on their way, by project and agent, each with the mark it was asked at.
    pub(crate) asked: std::collections::HashMap<(usize, String), String>,
    /// Where each list of more than [`LISTED`] rows is, by the fold key of the row it lies under: the row its block
    /// holds, as he moved it at a cut ([`block_rows`]).
    pub(crate) at: std::collections::HashMap<String, usize>,
    /// Moved by each answer and each block moved, for the sidebar's cache.
    pub(crate) rev: u64,
    /// What follows is drawn in the popups alone, which are drawn each frame: it moves no cache.
    /// A system prompt block's text by project and node while a popup shows it ([`apart_poll`]; paper-research N-1446
    /// p1642, 2): it, or why coordd gave none. Let go once no popup shows it: read on his click, never kept.
    pub(crate) texts: std::collections::HashMap<(usize, String), Result<String, String>>,
    /// The blocks' texts asked and on their way, by project and node.
    pub(crate) texts_asked: std::collections::HashSet<(usize, String)>,
    /// What an agent's graph gives apart from its folder, by project, agent and what it is of, while a popup or a pane
    /// shows it ([`apart_poll`]): a session's requests (its node; p1642, 3), its system prompt's rows ([`PROMPT`]) and
    /// a version's requests under their turns and steps (its text node; fix 4, accounts-lead N-1446 p2504). The mark
    /// they were asked at ([`folder_mark`]), and them or why there are none. Let go once nothing shows them.
    pub(crate) requests: std::collections::HashMap<(usize, String, String), (String, Result<Vec<FolderRow>, String>)>,
    /// Those asked and on their way, each with the mark it was asked at.
    pub(crate) requests_asked: std::collections::HashMap<(usize, String, String), String>,
}

/// Whether project P's coordd gives an agent's folder (`FOLDER_PROTOCOL`): only then has an agent's row a fold, and
/// an agent's page its System prompt, Initial prompt and Home (chat.rs `pane`).
pub(crate) fn folder_served(p: &crate::Proj) -> bool {
    p.down.is_none() && p.protocol >= coord_types::FOLDER_PROTOCOL
}

/// Whether project P's coordd gives a system prompt block's text and a session's requests (`TEXTS_PROTOCOL`): only
/// then does a press on a block's version or on a session open them apart.
fn texts_served(p: &crate::Proj) -> bool {
    p.down.is_none() && p.protocol >= coord_types::TEXTS_PROTOCOL
}

/// Whether project P's coordd gives an agent's system prompt (`PROMPT_PROTOCOL`, docket-machine `Graph::prompt`; fix
/// 4): then System prompt draws it ([`prompt_part`]) and asks no folder for it; else its folder's sessions' blocks
/// ([`sessions_part`]).
fn prompt_served(p: &crate::Proj) -> bool {
    p.down.is_none() && p.protocol >= coord_types::PROMPT_PROTOCOL
}

/// What an agent's system prompt's rows are held and asked under among [`Folders::requests`] (`Ask::prompt`; fix 4),
/// beside a session's node and a version's text node.
pub(crate) const PROMPT: &str = "prompt";

/// Whether row R of a folder, its id ID ([`row_ids`]), is a version of a system prompt block: a text's row with none
/// under it, under a session's `system prompt blocks`.
fn block_version(id: &str, r: &FolderRow) -> bool {
    r.rows.is_empty() && r.of.starts_with("text:") && id.contains("|system prompt blocks|")
}

/// The agents whose rows are listed under Agents, by their names now: the ones he pinned in the project chosen (his
/// N-1104 p863; paper-research N-1446 p2008), on every page. Only their folders are asked for.
fn agents_listed(s: &App) -> Vec<&str> {
    let Some(p) = s.proj() else { return Vec::new() };
    s.pins.list.iter().filter(|pin| pin.project == p.id).filter_map(|pin| pin.find(&p.live.agents)).map(|a| a.name.as_str()).collect()
}

/// What of agent A moves its folder: its state and since when, and how many sessions it has had. A turn's start and
/// end move it, so its folder is asked again with what the turn made; its tool calls do not.
fn folder_mark(a: &AgentView) -> String {
    format!("{}|{}|{}", a.state, a.since, a.sessions)
}

/// The id of agent NAME's folder, its fold's ([`tree_key`]) and the start of its rows' ([`row_ids`]): the agent's node.
fn folder_id(name: &str) -> String {
    docket_machine::graph::agent_id(name)
}

/// The agent whose folder page SEL draws a part of, when it is a lens of fix 4 ([`folder_part`]).
fn paned_agent(sel: &Sel) -> Option<&str> {
    match sel {
        Sel::Lens(l) if !l.filter.folder_part().is_empty() => l.agent(),
        _ => None,
    }
}

/// The pages of the chosen project on screen: the page shown and the split's others.
fn shown_sels(s: &App) -> impl Iterator<Item = &Sel> {
    let others = s.panes.iter().enumerate().filter(|(j, (pi, _))| *j != s.focus && *pi == s.cur).map(|(_, (_, sel))| sel);
    std::iter::once(&s.sel).chain(others)
}

/// The agents of the chosen project whose folder a pane on screen draws a part of ([`folder_part`]): the page shown
/// and the split's others, pinned or not (p2452: none of fix 4's presses needs a pin); System prompt's not, from a
/// coordd that gives it apart ([`prompt_served`]).
fn agents_paned(s: &App) -> Vec<String> {
    let apart = s.projects.get(s.cur).is_some_and(prompt_served);
    let mut out: Vec<String> = shown_sels(s).filter(|sel| !(apart && prompt_agent(sel).is_some())).filter_map(paned_agent).map(str::to_string).collect();
    // an Inspect drawer draws its agent's System prompt (inspect.rs)
    if !apart {
        out.extend(inspected_here(s));
    }
    out
}

/// The agents of the chosen project whose Inspect drawer is on screen (drawer.rs `inspected`).
fn inspected_here(s: &App) -> Vec<String> {
    crate::drawer::inspected(s).into_iter().filter(|(pi, _)| *pi == s.cur).map(|(_, n)| n).collect()
}

/// The agents of the chosen project whose System prompt is on screen: a page of it, or its Inspect drawer.
fn prompts_shown(s: &App) -> Vec<String> {
    let mut out: Vec<String> = shown_sels(s).filter_map(prompt_agent).map(str::to_string).collect();
    for n in inspected_here(s) {
        if !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

/// Every System prompt on screen, by its project and agent: the page shown, the split's other panes, an Inspect drawer.
pub(crate) fn prompt_targets(s: &App) -> Vec<(usize, String)> {
    let others = s.panes.iter().enumerate().filter(|(j, _)| *j != s.focus).map(|(_, (pi, sel))| (*pi, sel));
    let mut out: Vec<(usize, String)> = std::iter::once((s.cur, &s.sel)).chain(others).filter_map(|(pi, sel)| prompt_agent(sel).map(|n| (pi, n.to_string()))).collect();
    for x in crate::drawer::inspected(s) {
        if !out.contains(&x) {
            out.push(x);
        }
    }
    out
}

/// The agent whose System prompt page SEL is (lens.rs `Filter::SystemPrompt`).
fn prompt_agent(sel: &Sel) -> Option<&str> {
    match sel {
        Sel::Lens(l) => match &l.filter {
            crate::lens::Filter::SystemPrompt { agent } => Some(agent),
            _ => None,
        },
        _ => None,
    }
}

/// The texts the panes on screen show of their agents' folders ([`sessions_part`]), each by its project and node: for
/// each System prompt, its newest session's blocks' newest versions; from a coordd that gives its system prompt, the
/// versions whose text [`prompt_part`] shows ([`PromptVersion::text_shown`]).
fn part_texts(s: &App) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let apart = s.projects.get(s.cur).is_some_and(prompt_served);
    for agent in &prompts_shown(s) {
        let filter = crate::lens::Filter::SystemPrompt { agent: agent.clone() };
        if apart {
            // no request yet: no text of it is in the record's graph to read (`Request::Raw` would be refused)
            let sent = |list: &[FolderRow]| list.get(1).is_some_and(|g| g.rows.iter().any(|b| !b.rows.is_empty()));
            if let Some((_, Ok(list))) = s.folders.requests.get(&(s.cur, agent.clone(), PROMPT.to_string())).filter(|(_, r)| r.as_deref().is_ok_and(sent)) {
                out.extend(prompt_versions(agent, list).iter().filter(|v| v.text_shown()).map(|v| (s.cur, v.row.of.clone())));
                // the standing text beside a version of his, which System prompt leaves out of his words (p2987)
                out.extend(standing_of(list).filter(|_| list.first().is_some_and(|g| g.rows.iter().any(his_version))).map(|of| (s.cur, of)));
                // Claude Code's own main text as it first sent it, which the parts of a version of his in place of it are
                // read against (DRAFT, p3164)
                let in_place = list.first().is_some_and(|g| g.rows.iter().any(|r| r.text.contains(docket_machine::graph::IN_PLACE)));
                out.extend(list.get(1).filter(|_| in_place).and_then(|g| g.rows.iter().rev().find(|b| !b.rows.is_empty())).and_then(|b| b.rows.first()).map(|v| (s.cur, v.of.clone())));
            }
            continue;
        }
        let Some((_, Ok(list))) = s.folders.got.get(&(s.cur, agent.clone())) else { continue };
        let mut found = Vec::new();
        for path in filter.folder_part() {
            groups_at(&folder_id(agent), list, path, "", &mut found);
        }
        out.extend(found.last().into_iter().flat_map(|(id, r, _)| newest_versions(id, r)).map(|(_, _, v)| (s.cur, v.of.clone())));
    }
    out
}

/// What the System prompt panes on screen show apart from any folder, from a coordd that gives it ([`prompt_part`]),
/// each by its project, agent and what it is of: its system prompt's rows ([`PROMPT`]).
fn part_asks(s: &App) -> Vec<(usize, String, String)> {
    let mut out = Vec::new();
    if !s.projects.get(s.cur).is_some_and(prompt_served) {
        return out;
    }
    for agent in &prompts_shown(s) {
        out.push((s.cur, agent.to_string(), PROMPT.to_string()));
    }
    out
}

/// Each frame: the folder of each agent of the chosen project whose row is unfolded, or whose folder a pane on screen
/// draws a part of ([`agents_paned`]), asked of its coordd when none is held or its mark moved since
/// ([`folder_mark`]), one ask on its way for each; whether one was asked. The rows held stay drawn until the answer
/// comes.
pub fn folders_poll(s: &mut App) -> bool {
    let pi = s.cur;
    let listed = agents_listed(s);
    let paned = agents_paned(s);
    let Some(p) = s.projects.get(pi).filter(|p| folder_served(p) && !(listed.is_empty() && paned.is_empty())) else { return false };
    let wanted: Vec<(String, String)> = p
        .live
        .agents
        .iter()
        .filter(|a| (listed.contains(&a.name.as_str()) && s.streams.open.contains(&tree_key(pi, &folder_id(&a.name)))) || paned.contains(&a.name))
        .map(|a| (a.name.clone(), folder_mark(a)))
        .filter(|(name, mark)| {
            let key = (pi, name.clone());
            !s.folders.asked.contains_key(&key) && s.folders.got.get(&key).is_none_or(|g| g.0 != *mark)
        })
        .collect();
    let mut asked = false;
    for (name, mark) in wanted {
        let ask = docket_machine::graph_slice::Ask { folder: Some(name.clone()), ..Default::default() };
        if s.request_in(pi, coord_types::Request::Graph { ask }, crate::Pending::Folder { agent: name.clone() }) {
            s.folders.asked.insert((pi, name), mark);
            asked = true;
        }
    }
    asked
}

/// Project PI's answer to agent AGENT's folder: its rows held at the mark it was asked at, or why there are none.
pub fn folder_came(s: &mut App, pi: usize, agent: String, ok: bool, out: String) {
    let key = (pi, agent);
    let Some(mark) = s.folders.asked.remove(&key) else { return };
    let got = if ok {
        serde_json::from_str::<coord_types::GraphAnswer>(&out).map_err(|e| format!("unreadable folder: {e}")).and_then(|a| a.folder.ok_or_else(|| "no folder".to_string()))
    } else {
        Err(out.trim().to_string())
    };
    s.folders.got.insert(key, (mark, got));
    s.folders.rev += 1;
}

/// For a scene (lens.rs `scene`): ROWS, agent AGENT's folder in project PI, held as coordd's answer is, at the agent's
/// mark now ([`folder_mark`]), so nothing is asked again until it moves.
pub(crate) fn folder_held(s: &mut App, pi: usize, agent: &str, rows: Vec<FolderRow>) {
    let mark = s.projects.get(pi).and_then(|p| p.live.agents.iter().find(|a| a.name == agent)).map(folder_mark).unwrap_or_default();
    s.folders.got.insert((pi, agent.to_string()), (mark, Ok(rows)));
    s.folders.rev += 1;
}

/// For a scene (lens.rs `scene`): ROWS, what agent AGENT's graph gives apart under OF in project PI
/// ([`Folders::requests`]: its system prompt's under [`PROMPT`], a version's requests under its text node), held as
/// coordd's answer is, at the agent's mark now.
pub(crate) fn apart_held(s: &mut App, pi: usize, agent: &str, of: &str, rows: Vec<FolderRow>) {
    let mark = s.projects.get(pi).and_then(|p| p.live.agents.iter().find(|a| a.name == agent)).map(folder_mark).unwrap_or_default();
    s.folders.requests.insert((pi, agent.to_string(), of.to_string()), (mark, Ok(rows)));
}

/// Agent AGENT's folder asked of project PI's coordd, its pipe ended before the answer: asked again once it is back.
pub fn folder_lost(f: &mut Folders, pi: usize, agent: &str) {
    f.asked.remove(&(pi, agent.to_string()));
}

/// Each frame: what the folders' popups show that coordd has not given yet, asked of a coordd that gives it
/// ([`texts_served`]), one ask on its way for each (paper-research N-1446 p1642, 2 and 3): a system prompt block's
/// text (`Request::Raw` of its node), and a session's requests (`Ask::folder` with its `session`), these again when the
/// agent's mark moves ([`folder_mark`]); and the block texts a System prompt pane shows ([`part_texts`], p2545), and
/// from a coordd that gives it, its system prompt's rows and the requests of the versions he unfolded ([`part_asks`];
/// fix 4); whether one was asked. What no popup or pane shows any more is let go: a block's text is read on his click
/// (on its version, or on System prompt) and never kept.
pub fn apart_poll(s: &mut App) -> bool {
    let (mut texts, mut requests) = (Vec::new(), Vec::new());
    for p in &s.popups {
        match &p.what {
            crate::popup::What::Folder { pi, agent, id } => {
                if let Some(r) = folder_row_of(s, *pi, agent, id).filter(|r| block_version(id, r)) {
                    texts.push((*pi, r.of.clone()));
                }
            }
            crate::popup::What::Requests { pi, agent, session, .. } => requests.push((*pi, agent.clone(), session.clone())),
            // a block pressed in the cloud: its node is the text's (H3, p1821)
            crate::popup::What::Block { pi, id, .. } => texts.push((*pi, id.clone())),
            _ => {}
        }
    }
    // the version's text an edit opened from its card on the Map waits for (prompt_edit.rs `PromptEdit::reading`)
    if let (Some((pi, _)), Some(id)) = (&s.prompt_edit.of, &s.prompt_edit.reading) {
        texts.push((*pi, id.clone()));
    }
    texts.extend(part_texts(s));
    requests.extend(part_asks(s));
    s.folders.texts.retain(|key, _| texts.contains(key));
    s.folders.requests.retain(|key, _| requests.contains(key));
    let mut asked = false;
    for key in texts {
        if !s.projects.get(key.0).is_some_and(texts_served) || s.folders.texts.contains_key(&key) || s.folders.texts_asked.contains(&key) {
            continue;
        }
        if s.request_in(key.0, coord_types::Request::Raw { id: key.1.clone() }, crate::Pending::FolderText { id: key.1.clone() }) {
            s.folders.texts_asked.insert(key);
            asked = true;
        }
    }
    for key in requests {
        // a session's requests from TEXTS_PROTOCOL on; an agent's system prompt and a version's from PROMPT_PROTOCOL
        let prompt = key.2 == PROMPT || key.2.starts_with("text:");
        let Some(p) = s.projects.get(key.0).filter(|p| if prompt { prompt_served(p) } else { texts_served(p) }) else { continue };
        let mark = p.live.agents.iter().find(|a| a.name == key.1).map(folder_mark).unwrap_or_default();
        if s.folders.requests_asked.contains_key(&key) || s.folders.requests.get(&key).is_some_and(|g| g.0 == mark) {
            continue;
        }
        let mut ask = docket_machine::graph_slice::Ask::default();
        match key.2.as_str() {
            PROMPT => ask.prompt = Some(key.1.clone()),
            version if prompt => {
                (ask.prompt, ask.version) = (Some(key.1.clone()), Some(version.to_string()));
            }
            session => {
                (ask.folder, ask.session) = (Some(key.1.clone()), Some(session.to_string()));
            }
        }
        if s.request_in(key.0, coord_types::Request::Graph { ask }, crate::Pending::Requests { agent: key.1.clone(), of: key.2.clone() }) {
            s.folders.requests_asked.insert(key, mark);
            asked = true;
        }
    }
    asked
}

/// Project PI's answer to the text of block ID (its node): the text, or why there is none.
pub fn folder_text_came(s: &mut App, pi: usize, id: String, ok: bool, out: String) {
    let key = (pi, id);
    if !s.folders.texts_asked.remove(&key) {
        return;
    }
    s.folders.texts.insert(key, if ok { Ok(out) } else { Err(out.trim().to_string()) });
}

/// Project PI's answer to what agent AGENT's graph gives apart under OF ([`Folders::requests`]: a session's requests,
/// its system prompt, a version's requests): held at the mark they were asked at, or why there are none.
pub fn requests_came(s: &mut App, pi: usize, agent: String, of: String, ok: bool, out: String) {
    let key = (pi, agent, of);
    let Some(mark) = s.folders.requests_asked.remove(&key) else { return };
    let got = if ok {
        serde_json::from_str::<coord_types::GraphAnswer>(&out).map_err(|e| format!("unreadable requests: {e}")).and_then(|a| a.folder.ok_or_else(|| "no requests".to_string()))
    } else {
        Err(out.trim().to_string())
    };
    s.folders.requests.insert(key, (mark, got));
}

/// What W, a folder's popup's ask of project PI's coordd ([`apart_poll`]), its pipe ended before the answer: asked
/// again once it is back, while a popup shows it.
pub fn apart_lost(f: &mut Folders, pi: usize, w: &crate::Pending) {
    match w {
        crate::Pending::FolderText { id } => {
            f.texts_asked.remove(&(pi, id.clone()));
        }
        crate::Pending::Requests { agent, of } => {
            f.requests_asked.remove(&(pi, agent.clone(), of.clone()));
        }
        _ => {}
    }
}

/// The ids of ROWS under the row PARENT: PARENT, `|`, then the node each is of, else its words before their first `: `
/// (a group's: `sessions`, `commits`), a second of the same among them numbered (`#2`). Its fold and its popup are
/// named by it, so a folder given again keeps what he unfolded.
pub(crate) fn row_ids(parent: &str, rows: &[FolderRow]) -> Vec<String> {
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    rows.iter()
        .map(|r| {
            let own = if r.of.is_empty() { row_head(&r.text) } else { r.of.as_str() };
            let n = seen.entry(own).or_insert(0);
            *n += 1;
            if *n == 1 { format!("{parent}|{own}") } else { format!("{parent}|{own}#{n}") }
        })
        .collect()
}

/// A row's words before their first `: `: a group's name (`read`), a file's path, `version 2`.
fn row_head(text: &str) -> &str {
    text.split(": ").next().unwrap_or(text)
}

/// The row of agent AGENT's folder in project PI whose id is ID ([`row_ids`]), as coordd last gave it.
fn folder_row_of<'a>(s: &'a App, pi: usize, agent: &str, id: &str) -> Option<&'a FolderRow> {
    let (_, Ok(rows)) = s.folders.got.get(&(pi, agent.to_string()))? else { return None };
    let (mut rows, mut parent) = (rows.as_slice(), folder_id(agent));
    loop {
        let ids = row_ids(&parent, rows);
        let i = ids.iter().position(|x| x == id || id.strip_prefix(x.as_str()).is_some_and(|rest| rest.starts_with('|')))?;
        if ids[i] == id {
            return Some(&rows[i]);
        }
        (rows, parent) = (rows[i].rows.as_slice(), ids[i].clone());
    }
}

/// Where row R comes from, after its words (p1441: "each row says its source"): the record's line, a session's line
/// after its id's first eight characters, a commit, a job or a release; nothing for a group.
fn source_words(r: &FolderRow) -> String {
    use docket_machine::graph::Source;
    match &r.source {
        None => String::new(),
        Some(Source::Line(n)) => format!("line {n}"),
        Some(Source::Session(f, n)) => format!("{}:{n}", f.chars().take(8).collect::<String>()),
        Some(Source::Commit(c)) => format!("commit {c}"),
        Some(Source::Job(j)) => format!("job {j}"),
        Some(Source::Release(r)) => format!("release {r}"),
    }
}

/// Agent NAME's folder in a pane, as lens F of fix 4 draws it until docket-edge6's views take its place (lens.rs
/// `Filter::folder_part`; p2452, read right p2455; p2496): only the groups its paths name, each a path of row heads from
/// the folder's top, `*` any row (`sessions|*|system prompt blocks`). Each is under a line saying where it stands and is
/// drawn with its rows as the sidebar draws them ([`block_rows`]), so his folds and presses are theirs; else a line
/// saying why there are none yet. He presses it to read it (paper-research N-1446 p2545), so System prompt opens at the
/// newest session with its blocks' texts ([`sessions_part`]) and Initial prompt at the newest start with its brief
/// ([`starts_part`]); Home is its lists as they are. From a coordd that gives an agent's system prompt
/// ([`prompt_served`]), System prompt draws that, with no folder ([`prompt_part`]; fix 4, docket-edge6 N-1446 p2496).
pub(crate) fn folder_part(s: &App, pi: usize, name: &str, f: &crate::lens::Filter, k: &Kit) -> El {
    folder_part_in(s, pi, name, f, false, k)
}

/// [`folder_part`], FOOTER: drawn in the Inspect drawer (inspect.rs), whose footer sends the parts' draft, so the parts
/// editor draws no Go on with it of its own.
pub(crate) fn folder_part_in(s: &App, pi: usize, name: &str, f: &crate::lens::Filter, footer: bool, k: &Kit) -> El {
    let said = |t: String| rune::text(t).font(k.ty.body).color(k.c.text_muted).key("part-note");
    let mut rows: Vec<El> = Vec::new();
    // what the pane is, in one line, before anything of it (his N-1104 words in paper-research N-1446 p2987: "what is
    // intial promt vs sytem promt")
    let what = match f {
        crate::lens::Filter::InitialPrompt { .. } => Some(format!("An initial prompt is the first message {name} got when it started: its task. It is given once; everything after it is the conversation.")),
        _ => None,
    };
    rows.extend(what.map(|t| what_it_is(t, k)));
    match s.folders.got.get(&(pi, name.to_string())).map(|g| &g.1) {
        _ if matches!(f, crate::lens::Filter::SystemPrompt { .. }) && s.projects.get(pi).is_some_and(prompt_served) => prompt_part(s, pi, name, &mut rows, footer, k),
        _ if !s.projects.get(pi).is_some_and(folder_served) => rows.push(said("Needs a newer engine.".into())),
        None => rows.push(said("Loading…".into())),
        Some(Err(why)) => rows.push(said(format!("Not read: {}", first_line(why)))),
        Some(Ok(list)) => {
            let mut found = Vec::new();
            for path in f.folder_part() {
                groups_at(&folder_id(name), list, path, "", &mut found);
            }
            if found.is_empty() {
                rows.push(said("Nothing of it in the record's graph yet.".into()));
            }
            match f {
                crate::lens::Filter::SystemPrompt { .. } => sessions_part(s, pi, name, &found, &mut rows, k),
                crate::lens::Filter::InitialPrompt { .. } => starts_part(s, pi, name, &found, &mut rows, k),
                _ => {
                    for (id, r, place) in &found {
                        rows.push(part_head(place, id, k));
                        block_rows(s, pi, name, id, &r.rows, 0, &mut rows, k);
                    }
                }
            }
        }
    }
    rune::col().w_full().h_full().min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(rune::col().w_full().px(16.0).py(12.0).gap(2.0).children(rows))
}


/// The line at the top of Initial prompt saying what the pane is ([`folder_part`]; p2987).
fn what_it_is(t: String, k: &Kit) -> El {
    // small, at the body's line height for its size, so two lines of it do not crowd (paper-research N-1446 p3083)
    let style = rune::TextStyle { line_height: (k.ty.small.size * k.ty.body.line_height / k.ty.body.size * 2.0).round() / 2.0, ..k.ty.small };
    rune::text(t).font(style).color(k.c.text_muted).wrap().key("part-what")
}

/// The one folded row System prompt and Initial prompt keep what the harness adds behind, whose id is ID, into ROWS
/// (his N-1104 words in paper-research N-1446 p2987: "makee it more paple lie kamgic", "what the harness adds · N
/// lines"): LINES counted when known. A press folds or unfolds it, kept
/// among his folds; whether it is open.
fn harness_row(s: &App, pi: usize, id: &str, lines: Option<usize>, rows: &mut Vec<El>, k: &Kit) -> bool {
    let label = harness_words(lines);
    let fold = tree_key(pi, id);
    let open = s.streams.open.contains(&fold);
    let t = TreeRow { id: id.to_string(), label, detail: String::new(), depth: 0, selected: false, waits: false, under: true, drag: None };
    rows.push(rune::col().w_full().pt(14.0).child(tree_row(s, pi, t, move |s: &mut App| toggle_fold(s, &fold), k)));
    open
}

/// Part PART of agent NAME's system prompt in project PI unfolded, as a press on its row leaves it (a test's).
#[cfg(test)]
pub(crate) fn open_part(s: &mut App, pi: usize, name: &str, part: &str) {
    s.streams.open.insert(tree_key(pi, &format!("{}|by part", prompt_id(name))));
    s.streams.open.insert(tree_key(pi, &format!("{}|parts|{part}", prompt_id(name))));
}

/// [`harness_row`]'s words: what the harness adds, LINES counted when known.
fn harness_words(lines: Option<usize>) -> String {
    match lines {
        Some(0) => "what the harness adds · nothing".to_string(),
        Some(1) => "what the harness adds · 1 line".to_string(),
        Some(n) => format!("what the harness adds · {n} lines"),
        None => "what the harness adds".to_string(),
    }
}

/// A group's line in a part ([`folder_part`]): where it stands (PLACE), keyed by its id.
fn part_head(place: &str, id: &str, k: &Kit) -> El {
    rune::text(place.to_string()).font(k.ty.small_medium).color(k.c.text_muted).truncate().key(format!("part-{id}"))
}

/// What a part draws under a row of it ([`sessions_part`], [`starts_part`]): in from the row's words.
fn part_under(child: El) -> El {
    rune::col().w_full().pl(rune_kit::shell::INDENT * 2.0 + 8.0).pr(8.0).pt(2.0).pb(10.0).child(child)
}

/// System prompt's groups FOUND, each a session's system prompt blocks in the folder's order, into ROWS (p2545): the
/// newest session first, each of its blocks with its newest version's text under it ([`block_text_el`]: asked of
/// coordd on his press as a popup's is, and let go with the pane, [`apart_poll`]); the older ones under it, newest
/// first, each a row folded until he unfolds it, then drawn as the sidebar draws it.
fn sessions_part(s: &App, pi: usize, name: &str, found: &[(String, &FolderRow, String)], rows: &mut Vec<El>, k: &Kit) {
    let mut newest_first = found.iter().rev();
    if let Some((id, r, place)) = newest_first.next() {
        rows.push(part_head(place, id, k));
        let ids = row_ids(id, &r.rows);
        let texts = newest_versions(id, r);
        for (b, bid) in r.rows.iter().zip(&ids) {
            folder_row(s, pi, name, bid, b, 0, rows, k);
            if let Some((_, vid, v)) = texts.iter().find(|t| &t.0 == bid) {
                rows.push(part_under(block_text_el(s, pi, vid, &v.of, k)));
            }
        }
    }
    for (id, r, place) in newest_first {
        let fold = tree_key(pi, id);
        let t = TreeRow { id: id.clone(), label: place.clone(), detail: String::new(), depth: 0, selected: false, waits: false, under: true, drag: None };
        rows.push(tree_row(s, pi, t, move |s: &mut App| toggle_fold(s, &fold), k));
        if s.streams.open.contains(&tree_key(pi, id)) {
            block_rows(s, pi, name, id, &r.rows, 1, rows, k);
        }
    }
}

/// The blocks of R, a session's system prompt blocks whose id is ID, each with its newest version: the block's id, the
/// version's id and row, for a block whose last row is a version ([`block_version`]).
fn newest_versions<'a>(id: &str, r: &'a FolderRow) -> Vec<(String, String, &'a FolderRow)> {
    r.rows
        .iter()
        .zip(row_ids(id, &r.rows))
        .filter_map(|(b, bid)| {
            let (v, vid) = (b.rows.last()?, row_ids(&bid, &b.rows).pop()?);
            block_version(&vid, v).then_some((bid, vid, v))
        })
        .collect()
}

/// The id of agent NAME's system prompt's rows ([`row_ids`], [`prompt_part`]), beside its folder's groups, so a fold he
/// leaves in it is kept as the folder's are.
fn prompt_id(name: &str) -> String {
    format!("{}|system prompt", folder_id(name))
}

/// A version of an agent's system prompt as System prompt draws it ([`prompt_versions`]): its row's id ([`row_ids`]),
/// its row, and whether it is the newest of its text's (started with's, or its block's).
struct PromptVersion<'a> {
    id: String,
    row: &'a FolderRow,
    newest: bool,
}

impl PromptVersion<'_> {
    /// Whether its text is read: the newest's (p2545); none for the standing text no start line says
    /// (`graph_slice::NOT_SAID`), which is no node of a text.
    fn text_shown(&self) -> bool {
        self.newest && self.row.of != docket_machine::graph_slice::NOT_SAID
    }
}

/// The versions of LIST, an agent's system prompt as coordd's `Graph::prompt` gives it (started with, whose rows are
/// its versions; then Claude Code's own, whose rows are its blocks, each holding its versions, and the starts that
/// replaced it), in the order System prompt draws them ([`prompt_part`]): started with's newest first, then each
/// block's newest first.
fn prompt_versions<'a>(name: &str, list: &'a [FolderRow]) -> Vec<PromptVersion<'a>> {
    fn newest_first<'a>(parent: &str, rows: &'a [FolderRow], out: &mut Vec<PromptVersion<'a>>) {
        let ids = row_ids(parent, rows);
        for (n, i) in (0..rows.len()).rev().enumerate() {
            out.push(PromptVersion { id: ids[i].clone(), row: &rows[i], newest: n == 0 });
        }
    }
    let mut out = Vec::new();
    let ids = row_ids(&prompt_id(name), list);
    if let (Some(started), Some(id)) = (list.first(), ids.first()) {
        newest_first(id, &started.rows, &mut out);
    }
    if let (Some(own), Some(id)) = (list.get(1), ids.get(1)) {
        for (b, bid) in own.rows.iter().zip(row_ids(id, &own.rows)).filter(|(b, _)| !b.rows.is_empty()) {
            newest_first(&bid, &b.rows, &mut out);
        }
    }
    out
}

/// Agent NAME's system prompt into ROWS, as coordd's `Graph::prompt` gives it: the text it is sent now, whole, each
/// section under a quiet marker of where it came from (prompt_view.rs; the owner's live trial, N-1864.p1); Edit and
/// Edit by part beside its head (prompt_edit.rs, [`parts_rows`]); before its first request, the file it was given
/// ([`fresh`]).
fn prompt_part(s: &App, pi: usize, name: &str, rows: &mut Vec<El>, footer: bool, k: &Kit) {
    let said = |t: String| rune::text(t).font(k.ty.body).color(k.c.text_muted).key("part-note");
    let note = |t: String| rune::text(t).font(k.ty.small).color(k.c.text_faint).wrap();
    let given = match s.projects.get(pi).filter(|p| p.protocol >= coord_types::GIVEN_PROTOCOL).map(|p| s.standing.given(&p.id, name)) {
        Some(crate::prompt_view::Given::Got(r)) => Some(r),
        _ => None,
    };
    let file = given.and_then(Result::ok);
    // the file it was given (or that there is none) stands in for the record's rows until they come
    let none: Vec<FolderRow> = Vec::new();
    let list = match s.folders.requests.get(&(pi, name.to_string(), PROMPT.to_string())) {
        Some((_, Ok(list))) => list,
        _ if given.is_some() => &none,
        None => {
            rows.push(said("Loading…".into()));
            return;
        }
        Some((_, Err(why))) => {
            rows.push(said(format!("Not read: {}", plain_why(why))));
            return;
        }
    };
    let versions = prompt_versions(name, list);
    let ids = row_ids(&prompt_id(name), list);
    let under = |parent: &str| versions.iter().filter(|v| v.id.rsplit_once('|').map(|x| x.0) == Some(parent)).collect::<Vec<_>>();
    let held = |of: &str| match s.folders.texts.get(&(pi, of.to_string())) {
        Some(Ok(t)) => Some(t.as_str()),
        _ => None,
    };
    // started with's versions, newest first: the newest is what it runs with now; his are those not the standing text
    let started = ids.first().map(|id| under(id)).unwrap_or_default();
    let now = started.iter().find(|v| v.newest).copied();
    let blocks = ids.get(1).map(|own| own_blocks(list, own)).unwrap_or_default();
    // before its first request the file it was given says how it was given, in place of the harness's own or after it
    let replace = match file.filter(|_| blocks.is_empty()) {
        Some(g) => g.replace,
        None => now.is_some_and(|v| v.row.text.contains(docket_machine::graph::IN_PLACE)),
    };
    let his = started.iter().find(|v| his_version(v.row)).copied();
    let his_now = his.is_some() && his.map(|v| &v.id) == now.map(|v| &v.id);
    let standing = standing_of(list).and_then(|of| held(&of));
    let own: Vec<Option<&str>> = blocks.iter().flat_map(|b| under(b)).filter(|v| v.newest).map(|v| held(&v.row.of)).collect();
    let his_text = his.filter(|_| his_now).and_then(|v| held(&v.row.of));
    // the parts (the parts editor's; the shape accounts-lead N-1446 p3065 with docket-edge8's p3067): Claude Code's own
    // main text as it first sent it, and his version's in its place, each split by `prompt_parts::parts`, the
    // appended text at the end ours: in place of it, the main text before his version (its oldest); appended to it, the
    // newest, his words after it ours
    let appended = if replace { standing } else { his_text.or(standing) };
    let main_text = blocks.last().map(|b| under(b)).and_then(|vs| if replace { vs.last().copied() } else { vs.first().copied() }).and_then(|v| held(&v.row.of));
    let stock = main_text.map(|t| crate::prompt_parts::parts(t, appended));
    let his_parts = his_text.filter(|_| replace).map(|t| named_as(stock.as_deref().unwrap_or_default(), crate::prompt_parts::parts(t, standing)));
    let changed = changed_parts(stock.as_deref(), his_parts.as_deref());
    // the head: whose it is; Edit by part (open while a draft of his parts stands) and Edit, quiet beside it
    let by_part = tree_key(pi, &format!("{}|by part", prompt_id(name)));
    let parts_open = stock.is_some() && (s.streams.open.contains(&by_part) || crate::prompt_edit::draft_of(s, pi, name).is_some_and(|d| !d.is_empty()));
    let head = rune::text(format!("{name}'s system prompt, as it is sent")).font(k.ty.small_medium).color(k.c.text_base).key(format!("part-{}|yours", prompt_id(name)));
    let by = stock.is_some().then(|| {
        let words = if parts_open { "Done by part" } else { "Edit by part" };
        rune_kit::Button::new(words).variant(rune_kit::ButtonVariant::Ghost).on_press(move |s: &mut App| toggle_fold(s, &by_part)).build(k).key(format!("part-by-part-{name}")).help("Parts switched or changed")
    });
    let start = crate::prompt_edit::Start::Text(now.and_then(|v| held(&v.row.of)).or(file.map(|g| g.text.as_str())).unwrap_or_default().to_string());
    let edit = crate::prompt_edit::edit_press(s, pi, name, start, replace, k);
    rows.push(rune::row().w_full().gap(8.0).center_y().child(rune::col().grow(1.0).shrink().min_w(0.0).child(head)).children(by).children(edit));
    // opened from its card on the Map, the edit is drawn apart over it, not a second time here (popup.rs `PromptEdit`)
    let apart = s.prompt_edit.apart && s.prompt_edit.of.as_ref().is_some_and(|(p, n)| *p == pi && n == name);
    if !apart {
        rows.extend(crate::prompt_edit::editor(s, pi, name, k));
    }
    // the text it is sent now, each section marked by where it came from (prompt_view.rs)
    let newest = blocks.last().and_then(|b| under(b).first().copied());
    let now_text = newest.and_then(|v| held(&v.row.of));
    let not_read = newest.and_then(|v| match s.folders.texts.get(&(pi, v.row.of.clone())) {
        Some(Err(why)) => Some(why),
        _ => None,
    });
    let recipe = file.and_then(|g| g.diff.as_ref());
    match now_text {
        // a prompt recipe made it: drawn from its own account of what it kept, cut and added (prompt_diff.rs `of_file`)
        _ if recipe.is_some() => rows.extend(recipe.map(|d| crate::prompt_diff::drawn(s, pi, name, &crate::prompt_diff::of_file_kept(pi, name, d), k)).unwrap_or_default()),
        // no request yet: the file it was given, else what the record holds of it
        None if blocks.is_empty() => rows.extend(fresh(s, pi, name, his.and_then(|v| held(&v.row.of)).or(standing).map(|t| (t, his.is_some(), replace)), k)),
        None => rows.push(note(not_read.map_or("Loading…".into(), |why| format!("Not read: {}", plain_why(why)))).key("prompt-none")),
        Some(t) if stock.is_some() => {
            let stock = stock.as_deref().unwrap_or_default();
            let identity = if own.len() > 1 { own[0] } else { None };
            let files = s.projects.get(pi).map_or(&[][..], |p| s.standing.of(&p.id));
            let his = his_now && !replace;
            // read against Claude Code's own: what is left out struck, what is added marked; worked out again only when
            // what it reads changes
            let key = crate::prompt_diff::key_of(&(t, appended, replace, his, identity, stock.iter().map(|p| (&p.name, &p.text, p.ours)).collect::<Vec<_>>(), files.iter().map(|f| (&f.from, &f.text)).collect::<Vec<_>>()));
            let d = crate::prompt_diff::kept(pi, name, key, || {
                let parts = crate::prompt_parts::parts(t, appended);
                let parts = if replace { named_as(stock, parts) } else { parts };
                crate::prompt_diff::diff(identity, stock, &parts, files, his)
            });
            rows.extend(crate::prompt_diff::drawn(s, pi, name, &d, k));
        }
        Some(t) => {
            let parts = crate::prompt_parts::parts(t, appended);
            let parts = if replace { named_as(stock.as_deref().unwrap_or_default(), parts) } else { parts };
            // in place of Claude Code's own, a part is his unless Claude Code sent it so
            let stock_has = |q: &crate::prompt_parts::Part| stock.iter().flatten().any(|o| o.name == q.name && o.text.trim() == q.text.trim());
            let overridden: Vec<String> = parts.iter().filter(|q| replace && !q.ours && !stock_has(q)).map(|q| q.name.clone()).collect();
            let identity = if own.len() > 1 { own[0] } else { None };
            let files = s.projects.get(pi).map_or(&[][..], |p| s.standing.of(&p.id));
            rows.extend(crate::prompt_view::drawn(&crate::prompt_view::sections(identity, &parts, &overridden, his_now && !replace, files), k));
        }
    }
    if his.is_some() && !his_now {
        rows.push(note(format!("Restarted since; your version is not running.")));
    }
    if let Some(stock) = stock.as_ref().filter(|_| parts_open) {
        let cc1 = if own.len() > 1 { own[0] } else { None };
        parts_rows(s, pi, name, cc1, stock, his_parts.as_deref(), &changed, standing, footer, rows, k);
    }
}

/// Agent NAME's system prompt before its first request: the file coordd says it was given (`Request::Given`), or that
/// there is none; from a coordd before `GIVEN_PROTOCOL`, RECORDED (its text, his, in place of the harness's own), else
/// the standing files joined. Cut by those files under their markers, one muted line where the harness's own goes;
/// "Loading…" only while an answer is on its way.
fn fresh(s: &App, pi: usize, name: &str, recorded: Option<(&str, bool, bool)>, k: &Kit) -> Vec<El> {
    let plain = |t: &str| vec![rune::text(t.to_string()).font(k.ty.body).color(k.c.text_muted).wrap().key("part-note")];
    let Some(p) = s.projects.get(pi) else { return Vec::new() };
    let files = s.standing.of(&p.id);
    let waiting = s.standing.waiting(&p.id);
    let chosen = if p.protocol >= coord_types::GIVEN_PROTOCOL {
        use crate::prompt_view::Given;
        match s.standing.given(&p.id, name) {
            Given::Got(Ok(g)) => Some((g.text.clone(), g.his, g.replace)),
            Given::Got(Err(why)) if why.contains(coord_types::NO_GIVEN_FILE) => return plain(NO_FILE),
            Given::Got(Err(why)) => return plain(&format!("Not read: {}", plain_why(why))),
            // not asked yet: the next poll asks it, while it is one of the project's agents
            Given::Waiting | Given::NotAsked if p.live.agents.iter().any(|a| a.name == name) => return plain("Loading…"),
            Given::Waiting | Given::NotAsked => return plain(&format!("{name} is not one of this project's agents now.")),
        }
    } else {
        recorded.map(|(t, h, r)| (t.to_string(), h, r)).or_else(|| crate::prompt_view::joined(files).map(|t| (t, false, false)))
    };
    let Some((text, his, replace)) = chosen else {
        return if waiting { plain("Loading…") } else { vec![crate::prompt_view::to_come(k)] };
    };
    let ours = crate::prompt_parts::Part { name: "ours".into(), text, ours: true };
    let mut out: Vec<El> = (!replace).then(|| crate::prompt_view::to_come(k)).into_iter().collect();
    out.extend(crate::prompt_view::drawn(&crate::prompt_view::sections(None, std::slice::from_ref(&ours), &[], his, files), k));
    out
}

/// WHY's first line without the engine's own name before it, for a line on screen.
fn plain_why(why: &str) -> String {
    first_line(why).trim_start_matches("coordd: ").to_string()
}

/// What System prompt says of an agent with no system prompt file, from a coordd that would give it.
pub(crate) const NO_FILE: &str = "No system prompt file yet: it is written when the agent starts.";

/// The ids of the rows of Claude Code's own (LIST's second group, whose id is OWN) that hold versions: its blocks
/// ([`prompt_part`]).
fn own_blocks(list: &[FolderRow], own: &str) -> Vec<String> {
    list.get(1).map(|g| g.rows.iter().zip(row_ids(own, &g.rows)).filter(|(r, _)| !r.rows.is_empty()).map(|(_, id)| id).collect()).unwrap_or_default()
}

/// HIS parts, his version in place of Claude Code's own, each named as STOCK's part at its place where his own words
/// opening a paragraph would name it otherwise (his Pronouns paragraph rewritten is still Pronouns), when both have as
/// many parts; else as they are.
fn named_as(stock: &[crate::prompt_parts::Part], mut his: Vec<crate::prompt_parts::Part>) -> Vec<crate::prompt_parts::Part> {
    if his.len() == stock.len() {
        for (p, q) in his.iter_mut().zip(stock).filter(|(p, q)| !p.ours && !q.ours && !p.text.starts_with("# ")) {
            p.name = q.name.clone();
        }
    }
    his
}

/// The parts of Claude Code's own he changed in HIS, his version in place of it, by name: those whose text is not STOCK's
/// part of that name, or that STOCK has none of, and STOCK's parts HIS leaves out (switched off); ours (the standing
/// text appended) is never one. Computed against the stock text, so no record line says it (accounts-lead N-1446 p3065).
fn changed_parts(stock: Option<&[crate::prompt_parts::Part]>, his: Option<&[crate::prompt_parts::Part]>) -> Vec<String> {
    let (Some(stock), Some(his)) = (stock, his) else { return Vec::new() };
    let mut out: Vec<String> = his.iter().filter(|p| !p.ours && stock.iter().find(|q| q.name == p.name).is_none_or(|q| q.text != p.text)).map(|p| p.name.clone()).collect();
    out.extend(off_parts(stock, his));
    out
}

/// The names of STOCK's parts (Claude Code's own) that HIS, his version in place of it, leaves out: switched off.
fn off_parts(stock: &[crate::prompt_parts::Part], his: &[crate::prompt_parts::Part]) -> Vec<String> {
    stock.iter().filter(|q| !q.ours && !his.iter().any(|p| p.name == q.name)).map(|q| q.name.clone()).collect()
}

/// The parts editor (paper-research N-1446 p3164, its still; p3354 4: "build the parts editor (rows and switches as in
/// the still)"): opened by Edit by part, Claude Code's own parts (the shape accounts-lead p3065) into ROWS: its line, what it sends at every start and he cannot change
/// (CC1, its identity block), then each part of HIS version in place of it, else of STOCK (Claude Code's own as it first
/// sent it), by name, a folded row: a changed one marked, one off marked so, one he switched marked so, ours said. Opened,
/// Stock · Modified · Custom (p2995) · Off (p1158's "●off", his yes p1160) and its text, Off none: Stock, Claude Code's own (for ours the
/// harness's STANDING text); Modified, its text in a box, the stock text beside it; Custom, a box of his own (for ours,
/// what Edit writes). Once he switched one, what they add up to goes with Go on with it (prompt_parts.rs `compose`):
/// every part stock, ours after Claude Code's own; else the parts as one system prompt in place of it. Then what goes with the first message instead.
#[allow(clippy::too_many_arguments)]
fn parts_rows(s: &App, pi: usize, name: &str, cc1: Option<&str>, stock: &[crate::prompt_parts::Part], his: Option<&[crate::prompt_parts::Part]>, changed: &[String], standing: Option<&str>, footer: bool, rows: &mut Vec<El>, k: &Kit) {
    use crate::prompt_parts::{Going, Pick};
    let note = |t: String| rune::text(t).font(k.ty.small).color(k.c.text_faint).wrap();
    let body = |t: &str| rune::text(t.to_string()).font(k.ty.small).color(k.c.text_base).wrap();
    let id = format!("{}|parts", prompt_id(name));
    rows.push(rune::col().w_full().pt(8.0).child(part_head("Claude Code's own, read from its newest request", &id, k)));
    let first = cc1.and_then(|t| t.lines().next()).unwrap_or("its identity line");
    rows.push(note(format!("Billing line, fixed · \"{first}\"")));
    let e = &s.prompt_edit;
    // his draft of this agent's parts, open or kept for it (prompt_edit.rs `kept`)
    let none = crate::prompt_edit::Picks::new();
    let picks = crate::prompt_edit::draft_of(s, pi, name).unwrap_or(&none);
    let drafted = !picks.is_empty();
    // his version's parts, with Claude Code's own he left out among them at their place, off; else Claude Code's own
    let mut list: Vec<crate::prompt_parts::Part> = his.unwrap_or(stock).to_vec();
    if let Some(h) = his {
        for (i, q) in stock.iter().enumerate().filter(|(_, q)| !q.ours && !h.iter().any(|p| p.name == q.name)) {
            list.insert(i.min(list.len()), crate::prompt_parts::Part { name: q.name.clone(), text: String::new(), ours: false });
        }
    }
    let off = his.map(|h| off_parts(stock, h)).unwrap_or_default();
    // each part: its name, its stock text (ours: the standing text), how it goes on now, and the text it goes on with
    let now: Vec<(String, Option<String>, Pick, String, bool)> = list
        .iter()
        .map(|p| {
            let stock_text = match p.ours {
                true => standing.map(str::to_string),
                false => stock.iter().find(|q| q.name == p.name).map(|q| q.text.clone()),
            };
            let pick = match (p.ours, changed.contains(&p.name)) {
                (false, _) if off.contains(&p.name) => Pick::Off,
                (true, _) if stock_text.as_deref().map(str::trim) == Some(p.text.trim()) => Pick::Stock,
                (true, _) => Pick::Custom,
                (false, true) => Pick::Modified,
                (false, false) => Pick::Stock,
            };
            (p.name.clone(), stock_text, pick, p.text.clone(), p.ours)
        })
        .collect();
    for (pname, stock_text, at, text, ours) in &now {
        let pid = format!("{id}|{pname}");
        let draft = picks.get(pname);
        let pick = draft.map_or(*at, |d| d.0);
        // Off, or a Custom left empty: the same state, "● off" (p1158's word)
        let empty = !*ours && match draft {
            Some((Pick::Off, _)) => true,
            Some((Pick::Custom, t)) => t.text.trim().is_empty(),
            Some(_) => false,
            None => *at == Pick::Off,
        };
        let detail = match (*ours, draft.is_some(), changed.contains(pname)) {
            _ if empty => "● off".to_string(),
            (_, true, _) => "● switched".to_string(),
            (true, _, _) => "ours".to_string(),
            (_, _, true) => "● changed".to_string(),
            _ => String::new(),
        };
        let fold = tree_key(pi, &pid);
        let open = s.streams.open.contains(&fold);
        let t = TreeRow { id: pid.clone(), label: pname.clone(), detail, depth: 0, selected: false, waits: false, under: true, drag: None };
        rows.push(tree_row(s, pi, t, move |s: &mut App| toggle_fold(s, &fold), k));
        if !open {
            continue;
        }
        let (n, part, mine, at) = (name.to_string(), pname.clone(), text.clone(), *at);
        // On or Off, and his own text by a click on it, Modified · Diff · Reset once it differs (the owner, N-1790 part
        // 1: no Stock · Modified · Custom · Off); ours, the appended text, has no Off: an empty one appends nothing
        let on = !empty;
        let mut chips = rune::row().w_full().gap(8.0).center_y();
        if !*ours {
            let (n, part, mine) = (n.clone(), part.clone(), mine.clone());
            let toggle = rune_kit::Switch::new(on).label(if on { "On" } else { "Off" }).on_toggle(move |s: &mut App| match on {
                true => crate::prompt_edit::pick_part(s, pi, &n, &part, Pick::Off, "", at),
                false if at == Pick::Off => crate::prompt_edit::pick_part(s, pi, &n, &part, Pick::Stock, &mine, at),
                false => crate::prompt_edit::reset_part(s, pi, &n, &part),
            });
            chips = chips.child(toggle.build(k).key(format!("part-on-{pid}")));
        }
        let editing = on && matches!(pick, Pick::Modified | Pick::Custom);
        let differs = on && match draft {
            Some(d) => d.0 != Pick::Stock,
            None => matches!(at, Pick::Modified | Pick::Custom),
        };
        let diffed = e.diff.contains(&(pi, name.to_string(), pname.clone()));
        if differs {
            let key = (pi, name.to_string(), pname.clone());
            let diff = rune_kit::Button::new("Diff").variant(if diffed { rune_kit::ButtonVariant::Outline } else { rune_kit::ButtonVariant::Ghost }).help("Claude Code's own text beside yours").on_press(move |s: &mut App| {
                if !s.prompt_edit.diff.remove(&key) {
                    s.prompt_edit.diff.insert(key.clone());
                }
            });
            let (n2, part2, back) = (n.clone(), part.clone(), stock_text.clone().unwrap_or_default());
            let drafted_here = draft.is_some();
            let reset = rune_kit::Button::new("Reset").variant(rune_kit::ButtonVariant::Ghost).help("Back to what runs now").on_press(move |s: &mut App| match drafted_here {
                true => crate::prompt_edit::reset_part(s, pi, &n2, &part2),
                false => crate::prompt_edit::pick_part(s, pi, &n2, &part2, Pick::Stock, &back, at),
            });
            chips = chips.child(rune::text("Modified").font(k.ty.small_medium).color(k.c.text_accent).key(format!("part-modified-{pid}"))).child(diff.build(k).key(format!("part-diff-{pid}"))).child(reset.build(k).key(format!("part-reset-{pid}")));
        }
        let stock_el = || body(stock_text.as_deref().unwrap_or(if *ours { "No standing text read." } else { "Not in Claude Code's own." }));
        let fresh;
        let shown = match draft {
            Some(d) => &d.1,
            None => {
                fresh = rune::TextEdit::new(text);
                &fresh
            }
        };
        let area = {
            let (n, part, first) = (name.to_string(), pname.clone(), text.clone());
            rune_kit::TextArea::new(shown).width(None).spellcheck(false).auto_grow(3, 18).disabled(e.running.is_some()).on_edit(move |s: &mut App| crate::prompt_edit::part_text(s, pi, &n, &part, pick, &first)).build(k).key(format!("part-text-{pid}"))
        };
        // its text as it goes on: a click opens it to edit, his own from its words now
        let shown_el = || {
            // back to Claude Code's own (Reset), or never his: its stock words; else the words it runs with
            let first = match pick == Pick::Stock || text.trim().is_empty() {
                true => stock_text.clone().unwrap_or_default(),
                false => text.clone(),
            };
            let words = if first.trim().is_empty() { stock_el() } else { body(&first) };
            let (n, part) = (n.clone(), part.clone());
            let to = if *ours { Pick::Custom } else { Pick::Modified };
            k.interactive(rune::col().w_full().radius(k.radius.sm).child(words)).on_press(move |s: &mut App| crate::prompt_edit::pick_part(s, pi, &n, &part, to, &first, at)).help("Click to edit it").key(format!("part-edit-{pid}"))
        };
        let mine_el = if !on {
            note(format!("Off: nothing of {pname} is sent."))
        } else if editing {
            area
        } else {
            shown_el()
        };
        // Diff: Claude Code's own beside his
        let text_el = match differs && diffed && !*ours {
            true => rune::row()
                .w_full()
                .gap(16.0)
                .child(rune::col().grow(1.0).shrink().min_w(0.0).gap(4.0).child(note("Yours".into())).child(mine_el))
                .child(rune::col().grow(1.0).shrink().min_w(0.0).gap(4.0).child(note("Claude Code's own".into())).child(stock_el())),
            false => mine_el,
        };
        rows.push(part_under(rune::col().w_full().gap(8.0).child(chips).child(text_el)));
    }
    // what the switches add up to, sent once he switched one
    if drafted && !picks.is_empty() {
        let texts: Vec<(Option<String>, String, bool)> = now
            .iter()
            .map(|(pname, stock_text, _, text, ours)| {
                let going = match picks.get(pname) {
                    Some((Pick::Stock, _)) => stock_text.clone().unwrap_or_default(),
                    Some((Pick::Off, _)) => String::new(),
                    Some((_, t)) => t.text.clone(),
                    None => text.clone(),
                };
                (stock_text.clone().filter(|_| !ours), going, *ours)
            })
            .collect();
        let going: Vec<Going> = texts.iter().map(|(st, t, ours)| Going { stock: st.as_deref(), text: t, ours: *ours }).collect();
        let made = crate::prompt_parts::compose(&going);
        let what = match &made {
            None => "Nothing to send: every part is empty.".to_string(),
            Some((_, false)) => "Claude Code's own, then the appended text.".to_string(),
            Some((_, true)) => "These parts, in place of Claude Code's.".to_string(),
        };
        rows.push(note(what).key("parts-what"));
        // the drawer's footer sends it (inspect.rs): what it makes, and how much longer it is than what it runs with
        if footer {
            let runs: usize = now.iter().map(|(_, _, _, t, _)| t.chars().count()).sum();
            let grows = made.as_ref().map_or(0, |(t, _)| t.chars().count()) as i64 - runs as i64;
            *e.made.borrow_mut() = Some(crate::prompt_edit::Made { pi, name: name.to_string(), version: made, grows });
            return rows.push(rune::col().w_full().pt(6.0).child(note("CLAUDE.md, memory, git status and date come later.".into())));
        }
        if let Some(why) = e.error.as_ref().filter(|_| e.of.is_none()) {
            rows.push(rune::text(first_line(why)).font(k.ty.small).color(k.c.danger.fg).wrap().key("parts-error"));
        }
        let running = e.running.is_some();
        let why = crate::prompt_edit::why_not(s, pi, name);
        let n = name.to_string();
        let go = rune_kit::Button::new(if running { "Sending…" } else { "Go on with it" }).variant(rune_kit::ButtonVariant::Contrast).disabled(running || made.is_none() || why.is_some());
        let go = go.on_press(move |s: &mut App| {
            if let Some((t, replace)) = made.clone() {
                crate::prompt_edit::send_parts(s, pi, &n, t, replace);
            }
        });
        let go = match why {
            Some(w) => go.build(k).help(w),
            None => go.build(k).help("Applies after its turn"),
        };
        let cancel = rune_kit::Button::new("Cancel").variant(rune_kit::ButtonVariant::Ghost).disabled(running).on_press(crate::prompt_edit::drop_parts).build(k);
        rows.push(rune::row().w_full().justify_end().gap(8.0).pt(4.0).child(cancel.key("parts-cancel")).child(go.key("parts-go")));
    }
    rows.push(rune::col().w_full().pt(6.0).child(note("CLAUDE.md, memory, git status and date come later.".into())));
}

/// The text node of the newest standing text among LIST's started with (an agent's system prompt as `Graph::prompt`
/// gives it), which a version of his is read against (prompt_view.rs; asked by [`part_texts`]
/// beside a version of his though not shown); none for the standing text no start line says, which has no node.
fn standing_of(list: &[FolderRow]) -> Option<String> {
    list.first()?.rows.iter().rev().find(|r| !his_version(r) && r.of != docket_machine::graph_slice::NOT_SAID).map(|r| r.of.clone())
}

/// Whether R, a version of started with ([`prompt_part`]), is his: one he gave (`version N, appended`, `version N, in
/// place of Claude Code's own`), not the standing text roster gives with none of his (graph.rs's words for it, "the
/// standing text, appended", with or without a start line saying it).
fn his_version(r: &FolderRow) -> bool {
    r.of != docket_machine::graph_slice::NOT_SAID && !r.text.starts_with("the standing text")
}

/// Initial prompt's groups FOUND into ROWS (p2545), its task first and plain, the harness's behind one fold (his
/// N-1104 words in paper-research N-1446 p2987): Its task, the brief its newest start was started with
/// ([`brief_el`]), or that it was started with none; then what the harness adds · N lines ([`harness_row`]), N the
/// rows under it, folded; unfolded, each group under its line, its starts newest first as the sidebar folds them. With
/// no start line yet, its sessions' briefs as they are, under the fold.
fn starts_part(s: &App, pi: usize, name: &str, found: &[(String, &FolderRow, String)], rows: &mut Vec<El>, k: &Kit) {
    let note = |t: String| rune::text(t).font(k.ty.small).color(k.c.text_faint).wrap();
    let task = format!("{}|task", folder_id(name));
    rows.push(rune::col().w_full().pt(10.0).child(rune::text("Its task").font(k.ty.small_medium).color(k.c.text_base).key(format!("part-{task}"))));
    let newest = found.iter().rev().find_map(|(_, r, _)| r.rows.iter().rev().find(|x| x.of.starts_with("start:")));
    match newest.map(|st| st.rows.iter().find(|b| b.text.starts_with("brief: "))) {
        Some(Some(b)) => rows.push(brief_el(s, pi, &b.of, k)),
        Some(None) => rows.push(note(format!("None: no task given.")).key(format!("part-{task}-none"))),
        None => rows.push(note("No start recorded yet.".into()).key(format!("part-{task}-none"))),
    }
    let lines = found.iter().map(|(_, r, _)| 1 + r.rows.len()).sum();
    if !harness_row(s, pi, &format!("{}|harness", folder_id(name)), Some(lines), rows, k) {
        return;
    }
    for (id, r, place) in found {
        rows.push(rune::col().w_full().pt(8.0).child(part_head(place, id, k)));
        if !r.rows.iter().any(|x| x.of.starts_with("start:")) {
            block_rows(s, pi, name, id, &r.rows, 0, rows, k);
            continue;
        }
        let ids = row_ids(id, &r.rows);
        for i in (0..r.rows.len()).rev() {
            folder_row(s, pi, name, &ids[i], &r.rows[i], 0, rows, k);
        }
    }
}

/// The brief OF names, a post (`N-12.p3`) or a thread or item (`I-4`: its first post), as the window holds it, under
/// the start it briefed (p2545): who wrote it, then its words as a thread draws them (threads.rs `words`, asked as any
/// post's are while not read); else that this window does not hold it.
fn brief_el(s: &App, pi: usize, of: &str, k: &Kit) -> El {
    let note = |t: String| rune::text(t).font(k.ty.small).color(k.c.text_faint).wrap();
    let (t, n, _) = crate::lens::parts(of);
    let post = s.projects.get(pi).and_then(|p| crate::find::views(s, p).find(|v| v.id == t)).and_then(|v| v.posts.iter().find(|x| x.n == n.unwrap_or(1)));
    match post {
        None => note(format!("{of} is not held in this window.")).key(format!("part-brief-{of}")),
        Some(x) => rune::col()
            .w_full()
            .gap(4.0)
            .child(note(format!("Brief, {of} by {}:", x.by)))
            .child(rune_kit::markdown::Markdown::new(crate::threads::state(&s.tickets, x).shown()).selectable(true).build(k))
            .key(format!("part-brief-{of}")),
    }
}

/// The groups among LIST, under the row PARENT, that PATH names ([`folder_part`]), into OUT: each with its id, its row,
/// and where it stands, the words of the rows `*` passed (ABOVE) and its own, ` · ` between.
fn groups_at<'a>(parent: &str, list: &'a [FolderRow], path: &str, above: &str, out: &mut Vec<(String, &'a FolderRow, String)>) {
    let (head, rest) = path.split_once('|').unwrap_or((path, ""));
    for (r, id) in list.iter().zip(row_ids(parent, list)) {
        if head != "*" && row_head(&r.text) != head {
            continue;
        }
        let place = match (head == "*" || rest.is_empty(), above.is_empty()) {
            (false, _) => above.to_string(),
            (true, true) => r.text.clone(),
            (true, false) => format!("{above} · {}", r.text),
        };
        if rest.is_empty() {
            out.push((id, r, place));
        } else {
            groups_at(&id, &r.rows, rest, &place, out);
        }
    }
}

/// Agent NAME's folder under its row at DEPTH while it is unfolded, from a coordd that gives it, into ROWS: its five
/// groups ([`block_rows`]), or a row saying why there are none yet.
fn folder_rows(s: &App, pi: usize, name: &str, depth: u8, rows: &mut Vec<El>, k: &Kit) {
    let id = folder_id(name);
    if !s.projects.get(pi).is_some_and(folder_served) || !s.streams.open.contains(&tree_key(pi, &id)) {
        return;
    }
    let note = |t: String| SidebarRow::new(t).title_style(k.ty.small).depth(depth + 1).height(26.0).build(k).key(format!("folder-note-{name}"));
    match s.folders.got.get(&(pi, name.to_string())).map(|g| &g.1) {
        None => rows.push(note("loading…".into())),
        Some(Err(why)) => rows.push(note(format!("not read: {}", first_line(why)))),
        Some(Ok(list)) if list.is_empty() => rows.push(note("nothing of it in the record's graph yet".into())),
        Some(Ok(list)) => block_rows(s, pi, name, &id, list, depth, rows, k),
    }
}

/// LIST, the rows of agent AGENT's folder under the row PARENT, at DEPTH, each with those under it while it is
/// unfolded ([`folder_row`]): all of them up to [`LISTED`]; past it the block he moved to ([`Folders::at`]), with a row
/// at each cut saying how many it leaves out, a press there listing the block past it.
#[allow(clippy::too_many_arguments)]
fn block_rows(s: &App, pi: usize, agent: &str, parent: &str, list: &[FolderRow], depth: u8, rows: &mut Vec<El>, k: &Kit) {
    let ids = row_ids(parent, list);
    let key = tree_key(pi, parent);
    let (a, b) = block(list.len(), s.folders.at.get(&key).copied().unwrap_or(0), false);
    let to = |at: usize, key: String| {
        move |s: &mut App| {
            s.folders.at.insert(key.clone(), at);
            s.folders.rev += 1;
        }
    };
    if a > 0 {
        rows.push(cut_el(format!("tree-cut-{parent}-up"), format!("{a} more"), depth, to(a - 1, key.clone()), k));
    }
    for (r, id) in list[a..b].iter().zip(&ids[a..b]) {
        folder_row(s, pi, agent, id, r, depth, rows, k);
    }
    if b < list.len() {
        rows.push(cut_el(format!("tree-cut-{parent}-down"), format!("{} more", list.len() - b), depth, to(b, key), k));
    }
}

/// Row R of agent AGENT's folder, whose id is ID, at DEPTH, and those under it while it is unfolded, into ROWS: its
/// words and counts, then where it comes from ([`source_words`]). A press on a version opens the words that changed
/// into it apart (popup.rs; p1633: the tree shows rows and counts only), on a system prompt block's version its text
/// too, and on a session its requests (p1642, 2 and 3; from [`texts_served`]); on a post or a thread the window holds,
/// opens it there; on any other row that holds rows, folds or unfolds it.
#[allow(clippy::too_many_arguments)]
fn folder_row(s: &App, pi: usize, agent: &str, id: &str, r: &FolderRow, depth: u8, rows: &mut Vec<El>, k: &Kit) {
    let under = !r.rows.is_empty();
    let words = !r.words.is_empty();
    let served = s.projects.get(pi).is_some_and(texts_served);
    let block = served && block_version(id, r);
    let session = (served && r.of.starts_with("session:")).then(|| r.of.clone());
    let held = s.projects.get(pi).filter(|_| !r.of.contains(':')).and_then(|p| {
        let (t, post, _) = crate::lens::parts(&r.of);
        crate::find::held(s, p, t).map(|n| (n, t.to_string(), post))
    });
    let detail = source_words(r);
    // a file's path cut from its start to fit beside where it comes from, whole in the row's description (p1682, 3)
    let label = match r.of.strip_prefix("file:") {
        Some(path) => path_cut(&r.text, path, label_room(s.sidebar.width(), depth, &detail, under, k), k.ty.small),
        None => r.text.clone(),
    };
    // its description: its whole words where they are cut, where it comes from, and its time (a read row's last
    // read's, out of its words: p1729)
    let (cut, at) = (label != r.text, r.when.as_ref().map(|w| format!("at {w}")));
    let said = (cut || at.is_some()).then(|| [cut.then_some(r.text.as_str()), Some(detail.as_str()), at.as_deref()].into_iter().flatten().filter(|w| !w.is_empty()).collect::<Vec<_>>().join(", "));
    let t = TreeRow { id: id.to_string(), label, detail, depth, selected: false, waits: false, under, drag: None };
    let (agent_of, id_of, fold) = (agent.to_string(), id.to_string(), tree_key(pi, id));
    let press = move |s: &mut App| {
        if words || block {
            let what = crate::popup::What::Folder { pi, agent: agent_of.clone(), id: id_of.clone() };
            let title = apart_title(s, &what);
            crate::popup::open(s, title, what);
        } else if let Some(session) = &session {
            let what = crate::popup::What::Requests { pi, agent: agent_of.clone(), id: id_of.clone(), session: session.clone() };
            let title = apart_title(s, &what);
            crate::popup::open(s, title, what);
        } else if let Some((n, t, post)) = &held {
            crate::find::open_at(s, *n, t, *post);
        } else if under {
            toggle_fold(s, &fold);
        }
    };
    let row = tree_row(s, pi, t, press, k);
    rows.push(match said {
        Some(whole) => row.description(whole),
        None => row,
    });
    if under && s.streams.open.contains(&tree_key(pi, id)) {
        block_rows(s, pi, agent, id, &r.rows, depth + 1, rows, k);
    }
}

/// LABEL, a file row's words beginning with its PATH, cut from the path's start by whole folders until it fits ROOM
/// points in STYLE ("…/src/picture/docket.rs: read 9 times"), the file's own name always kept; whole when it fits
/// (paper-research N-1446 p1682, 3 (a): a worktree's paths begin alike, and their ends tell its files apart).
fn path_cut(label: &str, path: &str, room: f32, style: rune::TextStyle) -> String {
    let fits = |t: &str| rune::text::measure(t, &[], style, None).0 <= room;
    let Some(after) = label.strip_prefix(path).filter(|_| !fits(label)) else { return label.to_string() };
    let cuts: Vec<String> = path.match_indices('/').filter(|(i, _)| *i > 0).map(|(i, _)| format!("…{}{after}", &path[i..])).collect();
    cuts.iter().find(|t| fits(t.as_str())).or(cuts.last()).cloned().unwrap_or_else(|| label.to_string())
}

/// The points a folder row's label is drawn in at DEPTH in a sidebar WIDE points wide, beside DETAIL and, when UNDER,
/// its fold ([`tree_row`]: the sidebar's [`SIDE_PAD`]; rune-kit `SidebarRow`: the list's inset and the row's, 8 on
/// each side, 16 a depth, and 6 between its parts), less 4 for a cut that rounds up.
fn label_room(wide: f32, depth: u8, detail: &str, under: bool, k: &Kit) -> f32 {
    let parts = [(!detail.is_empty()).then(|| rune::text::measure(detail, &[], k.ty.small, None).0), under.then_some(FOLD_ROOM)];
    let end: f32 = parts.iter().flatten().map(|w| w + 6.0).sum();
    wide - SIDE_PAD[0] - SIDE_PAD[1] - UNFOLD_PAD - 4.0 * 8.0 - f32::from(depth + 1) * rune_kit::shell::INDENT - end - 4.0
}

/// How far the project shown stands in under its own row in Projects ([`built`]).
const UNFOLD_PAD: f32 = 12.0;

/// A row's fold button's width ([`fold_button`]: a ghost button, "▸" between its padding).
const FOLD_ROOM: f32 = 32.0;

/// The title of WHAT, a folder's row apart, as his press on it titles it ([`folder_row`]): a version's, the words that
/// changed into it, and a system prompt block's, its text too ([`folder_title`]); a session's, its requests.
pub(crate) fn apart_title(s: &App, what: &crate::popup::What) -> String {
    match what {
        crate::popup::What::Folder { pi, agent, id } => {
            let r = folder_row_of(s, *pi, agent, id);
            let block = s.projects.get(*pi).is_some_and(texts_served) && r.is_some_and(|r| block_version(id, r));
            let words = r.is_some_and(|r| !r.words.is_empty());
            let what = match (block, words) {
                (true, true) => "its text and the words that changed",
                (true, false) => "its text",
                _ => "the words that changed",
            };
            folder_title(s, *pi, agent, id, what)
        }
        crate::popup::What::Requests { pi, agent, id, .. } => crate::popup::title(agent, folder_row_of(s, *pi, agent, id).map_or("", |r| row_head(&r.text)), "its requests"),
        _ => String::new(),
    }
}

/// The title of row ID apart, WHAT it shows: the agent, then the row above it and the row itself by their heads (a
/// file's path or a block, the version), as `popup::title` writes them: "docket-edge5 · CLAUDE.md, version 2 · the
/// words that changed", "docket-edge5 · block 1, version 1 · its text".
fn folder_title(s: &App, pi: usize, agent: &str, id: &str, what: &str) -> String {
    let own = folder_row_of(s, pi, agent, id).map_or("", |r| row_head(&r.text));
    let above = id.rsplit_once('|').and_then(|(up, _)| folder_row_of(s, pi, agent, up)).map(|r| row_head(&r.text));
    crate::popup::title(agent, &above.map_or(own.to_string(), |a| format!("{a}, {own}")), what)
}

/// Row ID of agent AGENT's folder in project PI apart (popup.rs `What::Folder`): its words and counts and where they
/// come from, then the words that changed into it, those removed struck through and faint, those added bold in the
/// text's own colour, those around them quiet ([`words_marked`]); a system prompt block's version, then its text
/// ([`block_text_el`]). None when coordd's folder no longer holds it.
pub(crate) fn folder_words(s: &App, pi: usize, agent: &str, id: &str, k: &Kit) -> Option<El> {
    let r = folder_row_of(s, pi, agent, id)?;
    let from = match (source_words(r), &r.when) {
        (w, Some(at)) if !w.is_empty() => format!("from {w}, at {at}"),
        (w, _) if !w.is_empty() => format!("from {w}"),
        _ => String::new(),
    };
    let marked = (!r.words.is_empty()).then(|| {
        let (shown, gone, came) = words_marked(&r.words);
        let mut spans: Vec<rune::Span> = came.iter().map(|&(a, b)| rune::Span::new(a, b).bold().color(k.c.text_base)).chain(gone.iter().map(|&(a, b)| rune::Span::new(a, b).color(k.c.text_faint))).collect();
        spans.sort_by_key(|x| x.start);
        rune::rich(shown, spans).font(k.ty.body).color(k.c.text_muted).strikes(gone.iter().map(|&(a, b)| (a, b, None))).key(format!("folder-words-{id}"))
    });
    let text = block_version(id, r).then(|| block_text_el(s, pi, id, &r.of, k));
    let head = rune::text(r.text.clone()).font(k.ty.small).color(k.c.text_base).wrap();
    Some(rune::col().w_full().gap(6.0).child(head).child(rune::text(from).font(k.ty.small).color(k.c.text_faint).wrap()).children(marked).children(text))
}

/// The text of node OF, the version of a system prompt block whose row is ID, apart under its words (p1642, 2), or a
/// block pressed in the cloud, ID its node too (popup.rs `What::Block`, p1821 H3): as coordd read it from the session
/// on his press ([`apart_poll`]), or a note until it comes, or why it is not there.
pub(crate) fn block_text_el(s: &App, pi: usize, id: &str, of: &str, k: &Kit) -> El {
    text_el(s, pi, id, of, true, k)
}

/// The text of node OF as [`block_text_el`] draws a block's: CAPTIONED under a line saying it is the block's, read on
/// this press; else alone, where the row above it says whose it is (System prompt's pane, fix 4; p2565).
fn text_el(s: &App, pi: usize, id: &str, of: &str, captioned: bool, k: &Kit) -> El {
    let note = |t: String| rune::text(t).font(k.ty.small).color(k.c.text_faint).wrap();
    let Some(p) = s.projects.get(pi) else { return note("Its project is gone.".into()) };
    if p.down.is_some() {
        return note("Disconnected; retries when back.".into());
    }
    if !texts_served(p) {
        return note(format!("Needs a newer engine (protocol {}).", coord_types::TEXTS_PROTOCOL));
    }
    match s.folders.texts.get(&(pi, of.to_string())) {
        None => note("loading…".into()),
        Some(Err(why)) => note(format!("its text not read: {}", first_line(why))),
        Some(Ok(t)) => rune::col()
            .w_full()
            .gap(4.0)
            .children(captioned.then(|| note("Its text:".into())))
            .child(rune::text(t.clone()).font(k.ty.body).color(k.c.text_base).wrap().key(format!("folder-text-{id}"))),
    }
}

/// Session SESSION's requests of agent AGENT's folder in project PI apart (popup.rs `What::Requests`; p1642, 3): each
/// as coordd's graph gives it (docket-machine `Graph::session_requests`: when, its model and effort, its tokens, the
/// blocks it carried) and where it comes from, forty at a time ([`LISTED`]) with a row at each cut saying how many
/// more, a press there listing the block past it; a note until they come, or why there are none.
pub(crate) fn requests_apart(s: &App, pi: usize, agent: &str, session: &str, k: &Kit) -> El {
    let note = |t: String| rune::text(t).font(k.ty.small).color(k.c.text_faint).wrap();
    let list = match s.folders.requests.get(&(pi, agent.to_string(), session.to_string())) {
        None => return note("loading…".into()),
        Some((_, Err(why))) => return note(format!("not read: {}", first_line(why))),
        Some((_, Ok(list))) if list.is_empty() => return note("The record's graph holds no request of it.".into()),
        Some((_, Ok(list))) => list,
    };
    let key = format!("requests|{pi}|{agent}|{session}");
    let (a, b) = block(list.len(), s.folders.at.get(&key).copied().unwrap_or(0), false);
    let to = |at: usize| {
        let key = key.clone();
        move |s: &mut App| {
            s.folders.at.insert(key.clone(), at);
        }
    };
    let n = list.len();
    let said = format!("{n} {}, as sent.", if n == 1 { "request" } else { "requests" });
    let mut c = rune::col().w_full().gap(4.0).child(note(said));
    if a > 0 {
        c = c.child(cut_el("folder-requests-cut-up".into(), format!("{a} more"), 0, to(a - 1), k));
    }
    for (j, r) in list[a..b].iter().enumerate() {
        let at = a + j;
        let words = rune::text(r.text.clone()).font(k.ty.small).color(k.c.text_base).wrap().grow(1.0).shrink().key(format!("folder-request-{at}-words"));
        let from = rune::text(source_words(r)).font(k.ty.small).color(k.c.text_faint).key(format!("folder-request-{at}-from"));
        c = c.child(rune::row().key(format!("folder-request-{at}")).w_full().gap(8.0).child(words).child(from));
    }
    if b < n {
        c = c.child(cut_el("folder-requests-cut-down".into(), format!("{} more", n - b), 0, to(b), k));
    }
    c
}

/// The words of a `became` edge (coordd's `follow::difference`: how many changed, `:`, then the words, `[-removed-]`
/// and `{+added+}` among those that stay) as drawn apart: the words after the counts without their marks, and the byte
/// ranges in them removed and added; a run its cut left open marked to the end. Words with no counts before them (`the
/// same words: …`) whole, none marked.
fn words_marked(words: &str) -> (String, Vec<(usize, usize)>, Vec<(usize, usize)>) {
    let head = docket_machine::graph_slice::words_head(words);
    let Some(body) = words.get(head.len()..).and_then(|b| b.strip_prefix(':')) else { return (words.to_string(), Vec::new(), Vec::new()) };
    let (mut out, mut gone, mut came) = (String::new(), Vec::new(), Vec::new());
    // the run of marked words open: whether it is of words added, and where in OUT it began
    let mut open: Option<(bool, usize)> = None;
    for w in body.split(' ').filter(|w| !w.is_empty()) {
        if !out.is_empty() {
            out.push(' ');
        }
        let mut w = w;
        if open.is_none() {
            if let Some(x) = w.strip_prefix("[-") {
                (open, w) = (Some((false, out.len())), x);
            } else if let Some(x) = w.strip_prefix("{+") {
                (open, w) = (Some((true, out.len())), x);
            }
        }
        let close = open.and_then(|(added, _)| w.strip_suffix(if added { "+}" } else { "-]" }));
        out.push_str(close.unwrap_or(w));
        if close.is_some()
            && let Some((added, from)) = open.take()
        {
            (if added { &mut came } else { &mut gone }).push((from, out.len()));
        }
    }
    if let Some((added, from)) = open {
        (if added { &mut came } else { &mut gone }).push((from, out.len()));
    }
    (out, gone, came)
}

/// The sidebar closed to its icons, or opened again to the width it had.
fn to_rail(s: &mut App) {
    s.rail = !s.rail;
    if s.rail {
        s.rail_from = s.sidebar.width();
        s.sidebar.set_width(RAIL_W);
    } else {
        let w = if s.rail_from > RAIL_W { s.rail_from } else { 220.0 };
        s.sidebar.set_width(w);
    }
}

/// The sidebar as its icons: the current project's three entries (a mark on one that waits on the owner), the
/// window's own pages, and one to open it again.
fn rail(s: &App, k: &Kit) -> El {
    let icon = |i: &'static Icon, label: &str, on: bool| IconButton::new(i).variant(if on { IconButtonVariant::Neutral } else { IconButtonVariant::GhostMuted }).label(label.to_string()).help(label.to_string()).build(k);
    let mut c = rune::col().w_full().gap(8.0).pt(8.0).items_center();
    if let Some(p) = s.proj() {
        let owed = crate::waits::count(p);
        let threads = matches!(s.sel, Sel::Tickets | Sel::Mail) || crate::panes::boards(p).iter().any(|(id, _)| s.sel == Sel::View(id.clone()));
        let label = if owed > 0 { format!("Threads, {owed} waiting on you") } else { "Threads".to_string() };
        let t = icon(&THREADS, &label, threads).on_press(|s: &mut App| s.sel = Sel::Tickets);
        c = c.child(if owed > 0 { rune::row().child(t).child(rune::el().w(6.0).h(6.0).radius(3.0).fill(k.c.danger.fg)) } else { t });
        c = c.child(icon(&AGENTS, "Agents", matches!(s.sel, Sel::Agents | Sel::Agent(_) | Sel::Raw(_))).on_press(|s: &mut App| s.sel = Sel::Agents));
        let open_at = project_open_at(s, p, &crate::panes::project_tabs(p));
        c = c.child(icon(&PROJECT, "Project", crate::panes::is_project_tab(p, &s.sel)).on_press(move |s: &mut App| s.sel = open_at.clone()));
    }
    // the window's own pages, at its foot as in the open sidebar (N-1934 p12)
    c = c.child(rune::el().grow(1.0));
    for (words, key, i, on, go) in global_pages(s) {
        c = c.child(icon(i, words, on).on_press(move |s: &mut App| go(s)).key(format!("rail-{key}")));
    }
    c = c.child(icon(&WIDEN, "Open the sidebar", false).on_press(to_rail));
    Sidebar::new().label("yaaihh").child(c.h_full()).build(k)
}

/// The page the Project entry opens in P, of its TABS: the tab last shown, else the project's home (its file's home
/// view, else Home, home.rs; Home is the first tab in every project), else the first.
fn project_open_at(s: &App, p: &crate::Proj, tabs: &[(String, Sel)]) -> Sel {
    let home = crate::panes::tab_of(p, &Sel::View(String::new()));
    s.project_tab.clone().filter(|t| tabs.iter().any(|(_, x)| x == t)).or(home).or_else(|| tabs.first().map(|(_, t)| t.clone())).unwrap_or(Sel::Kinds)
}

/// What the Project row's badge counts: a kind's or the policy's refusals, a look that was not taken (look.rs, read on
/// the Kinds page with them) and each view whose source failed.
fn project_faults(p: &crate::Proj) -> usize {
    p.live.refused().count() + p.live.views.values().filter(|d| d.error.is_some()).count()
}

/// T's first non-empty line: a row's subtitle is one line, and a multi-line one (a view source's stderr) would push its
/// text out of the row over the rows above.
pub fn first_line(t: &str) -> String {
    t.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A part of Claude Code's own his version leaves out is off, counted with the changed ones; one he rewrote is
    /// changed; ours is neither.
    #[test]
    fn a_part_his_version_leaves_out_is_off_and_counted_changed() {
        use crate::prompt_parts::Part;
        let part = |n: &str, t: &str, ours: bool| Part { name: n.into(), text: t.into(), ours };
        let stock = [part("Agent line", "A.", false), part("Pronouns", "P.", false), part("Care", "C.", false), part("Appended: ours", "S.", true)];
        let his = [part("Agent line", "A.", false), part("Care", "C, mine.", false), part("Appended: ours", "S.", true)];
        assert_eq!(off_parts(&stock, &his), ["Pronouns"]);
        assert_eq!(changed_parts(Some(&stock), Some(&his)), ["Care", "Pronouns"]);
        assert!(changed_parts(Some(&stock), None).is_empty());
    }

    /// p1928 (his N-1104 p835: "i dont see this map"): a fresh window on research's coordd at GRAPH_PAGE_PROTOCOL shows
    /// the Map's row in research's tree, with no graph held and no graph asked: in build 68 the row waited for a graph,
    /// which came only once the Map was drawn, and the Map opened only from the row. A coordd before it, with no graph
    /// held, offers none, as before.
    #[test]
    fn the_maps_row_is_offered_where_coordd_serves_the_graph_before_any_graph_is_held() {
        for (protocol, offered) in [(coord_types::GRAPH_PAGE_PROTOCOL, true), (coord_types::GRAPH_PAGE_PROTOCOL - 1, false)] {
            let (_held, mut s) = crate::view_cost::test_app();
            let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
            p.down = None;
            p.protocol = protocol;
            s.projects = vec![p];
            s.cur = 0;
            s.sel = crate::Sel::Tickets;
            let mut rt = crate::view_cost::window(s);
            for _ in 0..4 {
                rt.frame(0.016);
            }
            assert!(rt.find("tickets").is_some() && rt.find("tree-insights").is_none(), "research's entries drawn, no structure of no thread (protocol {protocol})");
            assert!(rt.state.map.graph().is_none(), "no graph held (protocol {protocol})");
            assert!(!rt.state.projects[0].pending.values().any(|w| matches!(w, crate::Pending::Graph(_))), "no graph asked (protocol {protocol})");
            assert_eq!(rt.find("tree-map").is_some(), offered, "the Map's row (protocol {protocol})");
        }
    }

    /// His sidebar cut (his N-1104 p835: "the sidebad is still looking pretty clutered"; paper-research N-1446 p1929,
    /// p1938): the Map's row first of the project's entries, then his three (Threads, Agents, Project); no row of the
    /// project's name over them; Starred beneath Threads on every page (N-1446 p2121); no agent beneath Agents that he did
    /// not pin, on any page, the Agents page holding them by their teams (his N-1104 p863; N-1446 p2008); the footer its
    /// button alone while the link is live, the project and why while it is down.
    #[test]
    fn the_sidebar_holds_the_maps_row_and_his_three_entries_and_beneath_them_only_what_is_open() {
        let agent = |name: &str| -> AgentView {
            serde_json::from_value(serde_json::json!({"id": name, "name": name, "parent": "owner", "kind": "claude", "model": "m", "state": "idle", "since": "",
                "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null}))
            .unwrap()
        };
        let starred: coord_types::TicketView = serde_json::from_value(serde_json::json!({"id": "N-122", "n": 122, "kind": "thread", "opener": "owner", "ctx": {},
            "form": {"to": "lead"}, "title": "Hacker News scan", "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1,
            "opened_at": "2026-11-19T07:00:00Z", "given": [], "stars": [{"by": "owner", "at": "2026-11-19T07:05:00Z"}], "archived": false,
            "posts": [{"n": 1, "at": "2026-11-19T07:01:00Z", "by": "owner", "text": "scan", "names": ["lead"]}]}))
        .unwrap();
        let why = "coordd is not answering";
        for (sel, down) in [(Sel::Tickets, None), (Sel::Agents, None), (Sel::Agent("lead".into()), None), (Sel::Tickets, Some(why))] {
            let (_held, mut s) = crate::view_cost::test_app();
            let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
            p.down = down.map(String::from);
            p.protocol = coord_types::PROTOCOL;
            p.live.agents = vec![agent("lead"), agent("fixer")];
            p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(starred.clone()) });
            s.projects = vec![p];
            s.cur = 0;
            s.sel = sel.clone();
            let mut rt = crate::view_cost::window(s);
            crate::picture::docket::tests::frames(&mut rt);
            let on = format!("{sel:?}, down {down:?}");
            let els: Vec<_> = rt.elements().into_iter().filter(|e| !e.exiting && e.path.contains("/sidebar/")).collect();
            let y = |key: &str| els.iter().find(|e| e.key.as_deref() == Some(key)).map(|e| e.rect[1]);
            let texts = |key: &str| -> Vec<String> {
                let inside = format!("/{key}/");
                els.iter().filter(|e| e.path.contains(&inside)).filter_map(|e| e.text.clone()).collect()
            };
            let entries: Vec<Option<f32>> = ["tree-map", "tickets", "agents", "project-pages"].into_iter().map(|k| y(k)).collect();
            assert!(entries.iter().all(Option::is_some) && entries.windows(2).all(|w| w[0] < w[1]), "Map · Threads · Agents · Project, in that order ({on}): {entries:?}");
            assert!(!texts("project").iter().any(|t| t == "research"), "no row of the project's name over its entries ({on})");
            let starred = y("threads-starred");
            assert!(starred.is_some() && y("tickets") < starred && starred < y("agents"), "Starred beneath Threads on every page ({on}): {starred:?}");
            let listed: Vec<&str> = ["lead", "fixer"].into_iter().filter(|a| y(&format!("agent-{a}")).is_some()).collect();
            assert!(listed.is_empty(), "no agent beneath Agents that he did not pin ({on}): {listed:?}");
            let footer = texts("link");
            assert!(y("link").is_some(), "the footer drawn ({on})");
            match down {
                None => assert!(!footer.iter().any(|t| t == "research" || t == "live"), "the footer: its button alone while live ({on}): {footer:?}"),
                Some(why) => assert!(footer.iter().any(|t| t == "research") && footer.iter().any(|t| t == why), "the footer: the project and why while down ({on}): {footer:?}"),
            }
        }
    }

    /// N-1934 (his live trial on v0.4): the window's Home, Usage, Accounts and Settings stand at the sidebar's foot, in
    /// that order, below every project and never inside one; the project shown unfolds beneath its own row, to its Map,
    /// Threads, Agents and Project, above the next project's row; search stands over them; Settings and Accounts are no
    /// project's tabs; and each foot row opens its page.
    #[test]
    fn the_windows_own_pages_stand_at_the_foot_and_a_project_unfolds_beneath_its_row() {
        let global = ["global-home", "global-usage", "global-accounts", "global-settings"];
        for (cur, sel) in [(0, Sel::Tickets), (1, Sel::Agents), (0, Sel::Kinds), (0, Sel::View(crate::global::USAGE.into())), (1, Sel::View(crate::policy::ID.into()))] {
            let (_held, mut rt) = two_projects(Sel::Tickets, Vec::new());
            rt.update(move |s, _| {
                s.cur = cur;
                s.sel = sel.clone();
            });
            crate::picture::docket::tests::frames(&mut rt);
            let on = format!("project {cur}, {:?}", rt.state.sel);
            let els: Vec<_> = rt.elements().into_iter().filter(|e| !e.exiting && e.path.contains("/sidebar/")).collect();
            let el = |key: &str| els.iter().find(|e| e.key.as_deref() == Some(key));
            let y = |key: &str| el(key).map(|e| e.rect[1]);
            let foot: Vec<f32> = global.iter().map(|g| y(g).unwrap_or_else(|| panic!("{g} drawn ({on})"))).collect();
            assert!(foot.windows(2).all(|w| w[0] < w[1]), "Home · Usage · Accounts · Settings ({on}): {foot:?}");
            for g in global {
                let path = &el(g).unwrap().path;
                assert!(path.contains("/global/") && !path.contains("/project/"), "{g} is the window's, under no project ({on}): {path}");
            }
            let lowest = els.iter().filter(|e| e.path.contains("/project/") || e.key.as_deref().is_some_and(|k| k.starts_with("project-"))).map(|e| e.rect[1] + e.rect[3]).fold(0.0, f32::max);
            assert!(foot[0] >= lowest, "the foot below every project's row ({on}): {} over {lowest}", foot[0]);
            let entries: Vec<Option<f32>> = [format!("project-{cur}"), "tree-map".into(), "tickets".into(), "agents".into(), "project-pages".into()].iter().map(|k| y(k)).collect();
            assert!(entries.iter().all(Option::is_some) && entries.windows(2).all(|w| w[0] < w[1]), "the project's row, then its Map, Threads, Agents, Project ({on}): {entries:?}");
            if let Some(next) = y(&format!("project-{}", cur + 1)) {
                assert!(y("project-pages") < Some(next), "unfolded above the next project's row ({on})");
            }
            assert!(el("agents").unwrap().path.contains("/project/"), "Agents is the project's ({on})");
            for e in els.iter().filter(|e| e.key.as_deref().is_some_and(|k| k == "structure" || k.starts_with("tree-") || k.starts_with("insight"))) {
                assert!(e.path.contains("/project/"), "the project's structure is inside its fold ({on}): {}", e.path);
            }
            let structure = els.iter().find(|e| e.key.as_deref() == Some("section-header") && e.path.contains("/structure/")).map(|e| e.rect[1]);
            // cav has no thread, so no Structure (`structure`)
            let threads = !rt.state.projects[cur].live.tickets.is_empty();
            assert!(structure.is_some_and(|h| h > y("project-pages").unwrap()) == threads, "Structure beneath Project, in the fold ({on}): {structure:?}");
            assert!(y("find-bar-sidebar").is_some_and(|f| f < y("project-0").unwrap()), "search over the projects ({on})");
            for tab in ["project-tab-Settings", "project-tab-Accounts"] {
                assert!(el(tab).is_none(), "{tab}: no project's tab ({on})");
            }
            if rt.state.sel == Sel::Kinds {
                assert!(el("project-tab-Kinds").is_some(), "the project's own tabs beneath Project ({on})");
            }
        }
        let (_held, mut rt) = two_projects(Sel::Tickets, Vec::new());
        for (key, want) in [("global-usage", crate::global::usage_page()), ("global-settings", Sel::View(crate::policy::ID.into())), ("global-accounts", crate::ext::tab_page(crate::accounts::NAME)), ("global-home", Sel::View(crate::global::ID.into()))] {
            crate::home::tests::keyed(&mut rt, key);
            crate::picture::docket::tests::frames(&mut rt);
            assert_eq!(rt.state.sel, want, "{key} opens its page");
        }
        assert_eq!(rt.state.settings.scope, crate::policy::Scope::Global, "Settings opens on every project");
    }

    /// Harnesses is this machine's, not a project's: a row beneath the foot's Settings while Settings or it
    /// is open, which opens it with Settings lit, and no tab of any project's.
    #[test]
    fn harnesses_stands_beneath_the_windows_settings() {
        let (_held, mut rt) = two_projects(Sel::Tickets, Vec::new());
        crate::picture::docket::tests::frames(&mut rt);
        assert!(rt.find("global-harnesses").is_none(), "folded while Settings is not open");
        crate::home::tests::keyed(&mut rt, "global-settings");
        crate::picture::docket::tests::frames(&mut rt);
        crate::home::tests::keyed(&mut rt, "global-harnesses");
        crate::picture::docket::tests::frames(&mut rt);
        assert_eq!(rt.state.sel, Sel::Ext(crate::harness::NAME.into(), String::new()));
        let els: Vec<_> = rt.elements().into_iter().filter(|e| !e.exiting && e.path.contains("/sidebar/")).collect();
        let at = |key: &str| els.iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key} drawn"));
        let row = at("global-harnesses");
        assert!(row.path.contains("/global/") && !row.path.contains("/project/"), "the window's: {}", row.path);
        assert!(row.rect[1] > at("global-settings").rect[1], "beneath Settings");
        assert!(els.iter().all(|e| e.key.as_deref() != Some("project-tab-Harnesses")), "no project's tab");
        assert!(!crate::panes::is_project_tab(&rt.state.projects[rt.state.cur], &rt.state.sel), "not a tab of the Project page");
    }

    /// With no project yet, the foot's pages still open: Usage draws its page, never the first step's (N-1934 p4).
    #[test]
    fn the_windows_usage_opens_with_no_project() {
        // Usage and Home draw their pages; Settings, a project's engine's to answer, is no row
        for (sel, says) in [(crate::global::usage_page(), "All projects"), (Sel::View(crate::global::ID.into()), "Projects")] {
            let (_held, mut s) = crate::view_cost::test_app();
            s.projects.clear();
            s.sel = sel.clone();
            let mut rt = crate::view_cost::window(s);
            crate::picture::docket::tests::frames(&mut rt);
            let texts: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect();
            assert!(texts.iter().any(|t| t == says) && !texts.iter().any(|t| t == "No project yet"), "{sel:?}: {texts:?}");
            assert!(rt.find("global-home").is_some() && rt.find("global-settings").is_none(), "{sel:?}: no Settings row with no project");
        }
    }

    /// The sidebar closed to its icons keeps the window's own pages at its foot (N-1934 p12), each opening its page.
    #[test]
    fn the_rail_keeps_the_windows_own_pages() {
        let (_held, mut rt) = two_projects(Sel::Tickets, Vec::new());
        rt.update(|s, _| to_rail(s));
        crate::picture::docket::tests::frames(&mut rt);
        let y = |k: &str| rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(k)).map(|e| e.rect[1]);
        let foot: Vec<Option<f32>> = ["rail-global-home", "rail-global-usage", "rail-global-accounts", "rail-global-settings"].iter().map(|k| y(k)).collect();
        assert!(foot.iter().all(Option::is_some) && foot.windows(2).all(|w| w[0] < w[1]), "{foot:?}");
        for (key, want) in [("rail-global-usage", crate::global::usage_page()), ("rail-global-home", Sel::View(crate::global::ID.into())), ("rail-global-settings", Sel::View(crate::policy::ID.into()))] {
            crate::home::tests::keyed(&mut rt, key);
            crate::picture::docket::tests::frames(&mut rt);
            assert_eq!(rt.state.sel, want, "{key} opens its page");
        }
    }

    /// No page of the window says how it is built or how to use it: no engine's name, no command, no tutorial
    /// (N-1934 3), in its texts and its labels, at each project's pages and the window's own.
    #[test]
    fn no_page_speaks_of_the_engine_or_teaches() {
        const JARGON: [&str; 10] = ["coordd", "/autocompact", "/usage", "four at a time", "before anything else", "both measured", "he says", "clerk", "@name", "(@"];
        let mut long = Vec::new();
        let pages = [Sel::Tickets, Sel::Agents, Sel::Kinds, Sel::View(crate::global::ID.into()), Sel::View(crate::global::USAGE.into()), Sel::View(crate::policy::ID.into()), crate::ext::tab_page(crate::accounts::NAME)];
        for cur in [0, 1] {
            for sel in pages.clone() {
                // a projects.toml read at its start, as the window has one: no notice of the test's own folder
                let dir = std::env::temp_dir().join(format!("yaaihh-view-cost-{}", std::process::id()));
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join("projects.toml"), "").unwrap();
                let (_held, mut rt) = two_projects(Sel::Tickets, Vec::new());
                let _ = std::fs::remove_file(dir.join("projects.toml"));
                rt.update(move |s, _| {
                    s.cur = cur;
                    s.sel = sel.clone();
                });
                crate::picture::docket::tests::frames(&mut rt);
                for e in rt.elements().into_iter().filter(|e| !e.exiting) {
                    // the Docket's programs name the engine that runs them, as the owner asked of them (N-1988 p1)
                    let engine_named = e.path.split('/').any(|k| k == "programs-v1" || (k.starts_with("programs-") && k.ends_with("-stack")) || k.starts_with("inspect-docket-"));
                    for said in [&e.text, &e.label, &e.help].into_iter().flatten() {
                        let low = said.to_lowercase();
                        assert!(!JARGON.iter().filter(|j| !(engine_named && **j == "coordd")).any(|j| low.contains(j)), "project {cur}, {:?}: {said:?} at {}", rt.state.sel, e.path);
                    }
                    // a tooltip says it in a few words (N-1934 1)
                    // a tooltip that says a text whole, cut where it is drawn, is the text's own
                    if let Some(help) = e.help.as_ref().filter(|h| h.split_whitespace().count() > 8 && e.text.as_ref().is_none_or(|t| !h.starts_with(t.trim_end_matches('…')))) {
                        long.push(format!("project {cur}, {:?}: {help:?} at {}", rt.state.sel, e.path));
                    }
                }
            }
        }
        long.dedup();
        assert!(long.is_empty(), "tooltips over 8 words: {long:#?}");
    }

    /// The window of the next two legs: project research with lead working, fixer idle and old stopped, its thread
    /// N-122 "Hacker News scan" open; project cav with writer; PINS his pins; SEL the page shown in research.
    fn two_projects_app(sel: Sel, pins: Vec<Pin>) -> (std::sync::MutexGuard<'static, ()>, App) {
        let agent = |name: &str, state: &str| -> AgentView {
            serde_json::from_value(serde_json::json!({"id": format!("AGT-{name}"), "name": name, "parent": "owner", "kind": "claude", "model": "m", "state": state,
                "since": "", "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null}))
            .unwrap()
        };
        let thread: coord_types::TicketView = serde_json::from_value(serde_json::json!({"id": "N-122", "n": 122, "kind": "thread", "opener": "owner", "ctx": {},
            "form": {"to": "lead"}, "title": "Hacker News scan", "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1,
            "opened_at": "2026-11-19T07:00:00Z", "given": [], "stars": [], "archived": false,
            "posts": [{"n": 1, "at": "2026-11-19T07:01:00Z", "by": "owner", "text": "scan", "names": ["lead"]}]}))
        .unwrap();
        let project = |id: &str, agents: Vec<AgentView>| {
            let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: id.into(), id: id.into(), boxed: false }, Vec::new());
            p.down = None;
            p.protocol = coord_types::PROTOCOL;
            p.live.agents = agents;
            p
        };
        let (held, mut s) = crate::view_cost::test_app();
        let mut research = project("research", vec![agent("lead", "working"), agent("fixer", "idle"), agent("old", "stopped")]);
        research.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(thread) });
        s.projects = vec![research, project("cav", vec![agent("writer", "idle")])];
        s.cur = 0;
        s.sel = sel;
        s.pins.list = pins;
        (held, s)
    }

    /// [`two_projects`]'s window, drawn.
    fn two_projects(sel: Sel, pins: Vec<Pin>) -> (std::sync::MutexGuard<'static, ()>, rune::runtime::Runtime<App>) {
        let (held, s) = two_projects_app(sel, pins);
        let mut rt = crate::view_cost::window(s);
        crate::picture::docket::tests::frames(&mut rt);
        (held, rt)
    }

    /// The stills of N-1934: `STILL_OUT=DIR` and `--ignored --exact sidebar::tests::sidebar_ia_stills`. The two
    /// projects' window: research unfolded under its row at Threads, then the window's own Usage open from the foot.
    #[test]
    #[ignore]
    fn sidebar_ia_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        // read once at the start, so the still shows no missing file (docket-edge8, N-1952 p5); the folder kept as found
        fn made(sel: Sel) -> App {
            let dir = std::env::temp_dir().join(format!("yaaihh-view-cost-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("projects.toml"), "").unwrap();
            let s = two_projects_app(sel, Vec::new()).1;
            let _ = std::fs::remove_file(dir.join("projects.toml"));
            s
        }
        fn threads() -> App {
            made(Sel::Tickets)
        }
        fn usage() -> App {
            made(crate::global::usage_page())
        }
        let shots: [(&str, fn() -> App); 2] = [("sidebar-threads.png", threads), ("sidebar-usage.png", usage)];
        for (name, start) in shots {
            let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), start, crate::view) }).args(args).try_run().unwrap();
        }
    }

    /// The rect of the element keyed KEY, if it is drawn.
    fn rect(rt: &rune::runtime::Runtime<App>, key: &str) -> Option<[f32; 4]> {
        rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).map(|e| e.rect)
    }

    /// A press at X, Y, as his pointer gives it.
    fn press_at(rt: &mut rune::runtime::Runtime<App>, x: f32, y: f32) {
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        rt.frame(0.016);
    }

    /// A drag from FROM to TO in eight steps, a frame each; BEFORE_UP reads the window before the release.
    fn drag(rt: &mut rune::runtime::Runtime<App>, from: [f32; 2], to: [f32; 2], before_up: impl Fn(&rune::runtime::Runtime<App>)) {
        rt.pointer_move(from[0], from[1]);
        rt.frame(0.016);
        rt.pointer_down(from[0], from[1]);
        rt.frame(0.016);
        for i in 1..=8 {
            let f = i as f32 / 8.0;
            rt.pointer_move(from[0] + (to[0] - from[0]) * f, from[1] + (to[1] - from[1]) * f);
            rt.frame(0.016);
        }
        before_up(rt);
        rt.pointer_up(to[0], to[1]);
        rt.frame(0.016);
        rt.frame(0.016);
    }

    /// His Agents ask (his N-1104 p863: "where now it has a circle with the number of agents you need to add running so
    /// x running then under agents i jsut wantto be able to drag and pin"; paper-research N-1446 p2008, p2016): Agents
    /// says in words how many of its agents run, no count in a circle; beneath it, on every page, only what he pinned
    /// in this project (an agent, a thread, a lens), in his order; his pin of another project under that one's Agents;
    /// no Pinned section over the projects. A press on a pinned agent opens its page, his chat with it; a drag of a pin
    /// reorders this project's pins, the other project's left where it was among them; a pin's Unpin shows on the row
    /// under the pointer alone, and unpins it.
    #[test]
    fn his_pins_stand_under_agents_on_every_page_and_agents_says_in_words_how_many_run() {
        use crate::lens::{Filter, Lens};
        let agent = |project: &str, name: &str| Pin { project: project.into(), id: format!("AGT-{name}"), agent: name.into(), thread: false, lens: None };
        let with_lead = Lens::of(Filter::WithYou { agent: "lead".into() });
        let pins = vec![
            agent("research", "fixer"),
            agent("cav", "writer"),
            Pin { project: "research".into(), id: "N-122".into(), agent: "Hacker News scan".into(), thread: true, lens: None },
            Pin { project: "research".into(), id: String::new(), agent: with_lead.name(), thread: false, lens: Some(with_lead) },
        ];
        for sel in [Sel::Tickets, Sel::Agents, Sel::Agent("lead".into()), Sel::Files] {
            let (_held, rt) = two_projects(sel.clone(), pins.clone());
            let els: Vec<_> = rt.elements().into_iter().filter(|e| !e.exiting && e.path.contains("/sidebar/")).collect();
            let y = |key: &str| els.iter().find(|e| e.key.as_deref() == Some(key)).map(|e| e.rect[1]);
            let words: Vec<String> = els.iter().filter(|e| e.path.contains("/agents/")).filter_map(|e| e.text.clone()).collect();
            assert!(words.iter().any(|w| w == "2 running") && !words.iter().any(|w| w == "2"), "Agents says how many run, in words ({sel:?}): {words:?}");
            assert!(y("pinned").is_none(), "no Pinned section ({sel:?})");
            let order: Vec<Option<f32>> = ["agents", "pin-research-AGT-fixer", "pin-research-N-122", "pin-research-lens-with-lead", "project-pages"].into_iter().map(|k| y(k)).collect();
            assert!(order.iter().all(Option::is_some) && order.windows(2).all(|w| w[0] < w[1]), "beneath Agents his pins of research in his order, then Project ({sel:?}): {order:?}");
            assert!(y("pin-cav-AGT-writer").is_none(), "his pin of cav is not research's ({sel:?})");
        }
        let (_held, mut rt) = two_projects(Sel::Tickets, pins.clone());
        rt.update(|s, _| s.cur = 1);
        rt.invalidate();
        crate::picture::docket::tests::frames(&mut rt);
        assert!(rect(&rt, "pin-cav-AGT-writer").is_some() && rect(&rt, "pin-research-AGT-fixer").is_none(), "in cav, under its Agents its pin alone");
        rt.update(|s, _| s.cur = 0);
        rt.invalidate();
        crate::picture::docket::tests::frames(&mut rt);
        let r = rect(&rt, "pin-research-AGT-fixer").expect("fixer's pin");
        press_at(&mut rt, r[0] + 30.0, r[1] + r[3] / 2.0);
        assert_eq!(rt.state.sel, Sel::Agent("fixer".into()), "a press on a pinned agent opens its page, his chat with it");
        crate::picture::docket::tests::frames(&mut rt);
        let (from, to) = (rect(&rt, "pin-research-lens-with-lead").expect("the lens's pin"), rect(&rt, "pin-research-AGT-fixer").expect("fixer's pin"));
        drag(&mut rt, [from[0] + 30.0, from[1] + from[3] / 2.0], [from[0] + 30.0, to[1] + to[3] / 2.0], |_| {});
        let names: Vec<&str> = rt.state.pins.list.iter().map(|p| p.agent.as_str()).collect();
        assert_eq!(names, ["With you · lead", "fixer", "writer", "Hacker News scan"], "the lens up to the first of research's pins; cav's writer left among them");
        // his Unpin on the pin under the pointer alone: at rest a name has the row's width
        crate::picture::docket::tests::frames(&mut rt);
        let control = |rt: &rune::runtime::Runtime<App>, row: &str, key: &str| {
            let inside = format!("/{row}/");
            rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key) && e.path.contains(&inside)).map(|e| e.rect)
        };
        rt.pointer_move(1.0, 1.0);
        crate::picture::docket::tests::frames(&mut rt);
        assert!(control(&rt, "pin-research-AGT-fixer", "unpin").is_none() && control(&rt, "pin-research-N-122", "unpin").is_none(), "no Unpin at rest");
        let r = rect(&rt, "pin-research-N-122").expect("N-122's pin");
        rt.pointer_move(r[0] + 30.0, r[1] + r[3] / 2.0);
        crate::picture::docket::tests::frames(&mut rt);
        assert!(control(&rt, "pin-research-N-122", "unpin").is_some(), "Unpin on the thread's pin under the pointer");
        let r = rect(&rt, "pin-research-AGT-fixer").expect("fixer's pin");
        let fold = control(&rt, "pin-research-AGT-fixer", "agent-fold-fixer").expect("its folder's fold");
        rt.pointer_move(r[0] + 30.0, r[1] + r[3] / 2.0);
        crate::picture::docket::tests::frames(&mut rt);
        assert_eq!(control(&rt, "pin-research-AGT-fixer", "agent-fold-fixer"), Some(fold), "its fold where it was at rest, Unpin before it");
        let c = control(&rt, "pin-research-AGT-fixer", "unpin").expect("its Unpin under the pointer");
        press_at(&mut rt, c[0] + c[2] / 2.0, c[1] + c[3] / 2.0);
        let names: Vec<&str> = rt.state.pins.list.iter().map(|p| p.agent.as_str()).collect();
        assert_eq!(names, ["With you · lead", "writer", "Hacker News scan"], "its Unpin unpins fixer");
    }

    /// One entry is lit wherever he is (the walk's 2, paper-research N-1446 p2101, p2120): on the page of an agent he did
    /// not pin, reached from a tile, All agents or a mention, Agents is lit, as on the Agents page and as the rail's icon
    /// is; on a pinned agent's page its pin is lit and Agents is not; on Threads and Files neither is.
    #[test]
    fn on_an_agents_page_agents_is_lit_unless_its_own_pin_is() {
        let pins = vec![Pin { project: "research".into(), id: "AGT-fixer".into(), agent: "fixer".into(), thread: false, lens: None }];
        for (sel, agents, pin) in [
            (Sel::Tickets, false, false),
            (Sel::Agents, true, false),
            (Sel::Agent("lead".into()), true, false),
            (Sel::Agent("fixer".into()), false, true),
            (Sel::Files, false, false),
        ] {
            let (_held, rt) = two_projects(sel.clone(), pins.clone());
            let els: Vec<_> = rt.elements().into_iter().filter(|e| !e.exiting && e.path.contains("/sidebar/")).collect();
            let lit = |key: &str| els.iter().find(|e| e.key.as_deref() == Some(key)).map(|e| e.item_selected);
            assert_eq!((lit("agents"), lit("pin-research-AGT-fixer")), (Some(agents), Some(pin)), "(Agents lit, fixer's pin lit) on {sel:?}");
        }
    }

    /// A drop on Agents pins (his N-1104 p863: "drag and pin deffo chats / threads ... and mayeb aslo specici agetns";
    /// paper-research N-1446 p2008, p2016): a thread of the structure and an agent's tile on the Agents page, dragged
    /// onto Agents, are pinned there after his other pins, "Pin NAME" by the pointer on the way; dragged there again
    /// they stay pinned, and where they were.
    #[test]
    fn a_thread_and_an_agents_tile_dropped_on_agents_are_pinned_there_and_a_second_drop_leaves_them() {
        let (_held, mut rt) = two_projects(Sel::Tickets, Vec::new());
        let ghost = |rt: &rune::runtime::Runtime<App>| -> Vec<String> { rt.elements().into_iter().filter(|e| e.path.contains("/drop-ghost/")).filter_map(|e| e.text).collect() };
        let center = |r: [f32; 4]| [r[0] + 30.0, r[1] + r[3] / 2.0];
        for round in 0..2 {
            let (from, to) = (rect(&rt, "tree-N-122").expect("N-122's row in the structure"), rect(&rt, "agents").expect("Agents"));
            drag(&mut rt, center(from), center(to), |rt| {
                let said = ghost(rt);
                assert_eq!(said.iter().any(|t| t == "Pin Hacker News scan"), round == 0, "\"Pin NAME\" by the pointer while it is not pinned (round {round}): {said:?}");
            });
            let pins: Vec<(String, String, bool)> = rt.state.pins.list.iter().map(|p| (p.project.clone(), p.id.clone(), p.thread)).collect();
            assert_eq!(pins, [("research".to_string(), "N-122".to_string(), true)], "the thread pinned, once (round {round})");
            crate::picture::docket::tests::frames(&mut rt);
            assert!(rect(&rt, "pin-research-N-122").is_some(), "its row beneath Agents (round {round})");
        }
        rt.update(|s, _| s.sel = Sel::Agents);
        rt.invalidate();
        crate::picture::docket::tests::frames(&mut rt);
        for round in 0..2 {
            let (tile, to) = (rect(&rt, "tile-lead").expect("lead's tile"), rect(&rt, "agents").expect("Agents"));
            // the tile's corner, inside its padding: no control of its own there
            drag(&mut rt, [tile[0] + 4.0, tile[1] + 4.0], center(to), |_| {});
            let pins: Vec<(String, String, bool)> = rt.state.pins.list.iter().map(|p| (p.id.clone(), p.agent.clone(), p.thread)).collect();
            assert_eq!(pins, [("N-122".to_string(), "Hacker News scan".to_string(), true), ("AGT-lead".to_string(), "lead".to_string(), false)], "lead pinned after the thread, once (round {round})");
            assert_eq!(rt.state.sel, Sel::Agents, "the drop on Agents opened nothing (round {round})");
            crate::picture::docket::tests::frames(&mut rt);
        }
    }

    fn pin(thread: bool, id: &str) -> Pin {
        Pin { project: "p".into(), id: id.into(), agent: "x".into(), thread, lens: None }
    }

    /// A lens pin is kept beside pins.json (proto-lead N-1446 p158; docket-edge6 p166): pins.json holds only what a
    /// window from before lenses reads, so after a `back` it shows no row that opens nothing; this window puts each lens
    /// pin back at its place, keeps one it cannot read as written, and reads an S1 pins.json with lens pins in it.
    #[test]
    fn a_lens_pin_is_kept_beside_pins_json_at_its_place() {
        use crate::lens::{Filter, Lens};
        let agent = |n: &str| Pin { project: "p".into(), id: format!("AGT-{n}"), agent: n.into(), thread: false, lens: None };
        let lens = |filter: Filter| {
            let l = Lens { filter, depth: Default::default(), at: None };
            Pin { project: "p".into(), id: String::new(), agent: l.name(), thread: false, lens: Some(l) }
        };
        let list = vec![agent("pr"), lens(Filter::Threads), agent("qa"), lens(Filter::WithYou { agent: "pr".into() })];
        let names = |l: &[Pin]| l.iter().map(|p| p.agent.clone()).collect::<Vec<_>>();
        let newer = serde_json::json!({"at": 1, "pin": {"project": "p", "agent": "Mentions · N-7", "lens": {"filter": {"Mentions": {"of": "N-7"}}, "layer": "Structure", "at": null}}});
        let (pins, lenses) = split(&list, std::slice::from_ref(&newer));
        // what an older window reads: the agents, nothing of a lens
        let old: Vec<serde_json::Value> = serde_json::from_str(&pins).unwrap();
        assert_eq!(old.iter().map(|p| p["agent"].as_str().unwrap()).collect::<Vec<_>>(), ["pr", "qa"]);
        assert!(!pins.contains("lens"), "{pins}");
        // this window: the list as it was, the newer window's pin kept as written
        let lenses = lenses.expect("a file beside it");
        let (back, unread) = joined(&pins, Some(lenses.as_bytes()));
        let unread = unread.expect("a list");
        assert_eq!(names(&back), ["pr", "Threads", "qa", "With you · pr"]);
        assert!(back.iter().zip(&list).all(|(a, b)| a.same(b) && a.lens == b.lens));
        assert_eq!(unread, [newer.clone()]);
        assert_eq!(split(&back, &unread), (pins.clone(), Some(lenses.clone())), "written again as read");
        // an older window took qa out: each lens pin at its place, or after what is left
        let (fewer, _) = joined(r#"[{"project":"p","id":"AGT-pr","agent":"pr"}]"#, Some(lenses.as_bytes()));
        assert_eq!(names(&fewer), ["pr", "Threads", "With you · pr"]);
        // no lens pins: no file; an S1 pins.json with its lens pins inline reads as it is
        assert_eq!(split(&[agent("pr")], &[]).1, None);
        let (s1, _) = joined(&serde_json::to_string(&list).unwrap(), None);
        assert_eq!(names(&s1), names(&list));
        // a file that is not a list (cut off half-way, a newer shape) survives a save, and pins.json is written
        let dir = std::env::temp_dir().join(format!("yaaihh-pins-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pins.json");
        std::fs::write(&path, &pins).unwrap();
        std::fs::write(lens_pins_path(&path), "{").unwrap();
        let mut read = Pins::read(&path);
        assert_eq!(names(&read.list), ["pr", "qa"]);
        read.list.push(lens(Filter::Threads));
        read.list.remove(0);
        read.write(&path);
        assert_eq!(std::fs::read_to_string(lens_pins_path(&path)).unwrap(), "{");
        assert_eq!(names(&Pins::read(&path).list), ["qa", "Threads"], "the lens pinned since is kept in pins.json");
        // an S1 pins.json with its lens pins inline beside it: a write keeps them inline (proto-reader p322)
        std::fs::write(&path, serde_json::to_string(&list).unwrap()).unwrap();
        let mut s1 = Pins::read(&path);
        s1.list.remove(2);
        s1.write(&path);
        assert_eq!(std::fs::read_to_string(lens_pins_path(&path)).unwrap(), "{");
        assert_eq!(names(&Pins::read(&path).list), ["pr", "Threads", "With you · pr"]);
        // the file mended (here taken away): the inline pins go out to it at the next write, each read once
        std::fs::remove_file(lens_pins_path(&path)).unwrap();
        let mended = Pins::read(&path);
        assert!(!mended.kept);
        mended.write(&path);
        assert!(!std::fs::read_to_string(&path).unwrap().contains("lens"));
        std::fs::write(&path, serde_json::to_string(&mended.list).unwrap()).unwrap();
        assert_eq!(names(&Pins::read(&path).list), ["pr", "Threads", "With you · pr"], "inline and in the file: once");
        // one that cannot be read at all (here a folder) is kept as well, and nothing is left beside pins.json
        std::fs::remove_file(lens_pins_path(&path)).unwrap();
        std::fs::create_dir(lens_pins_path(&path)).unwrap();
        let folder = Pins::read(&path);
        assert!(folder.kept);
        folder.write(&path);
        assert!(lens_pins_path(&path).is_dir() && std::fs::read_dir(&dir).unwrap().count() == 2, "no .tmp left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// consult-5's hold on 02de3136 (N-412 post 2013): a look that does not check is a refusal like a kind's, so the
    /// Project row shows it in its badge.
    #[test]
    fn a_look_that_does_not_check_gives_the_project_row_its_badge() {
        let _one = crate::view_cost::one();
        let mut p = crate::view_cost::project_for_tests();
        assert_eq!(project_faults(&p), 0);
        p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Look(serde_json::json!({"context": {"mark": {"shape": "star"}}})) });
        assert_eq!(p.live.refused().count(), 1, "{:?}", p.live.look_refused);
        assert_eq!(project_faults(&p), 1, "the look's line is counted");
        p.live.refusals = vec!["a kind's refusal".into()];
        assert_eq!(project_faults(&p), 2);
    }

    #[test]
    fn a_thread_pin_is_not_an_agents_and_an_old_file_reads() {
        assert!(!pin(true, "N-1").same(&pin(false, "N-1")), "same id, different kind");
        assert!(pin(true, "N-1").same(&pin(true, "N-1")));
        assert!(!pin(true, "N-1").same(&pin(true, "N-2")));
        let old: Vec<Pin> = serde_json::from_str(r#"[{"project":"p","id":"AGT-1","agent":"a"}]"#).unwrap();
        assert!(!old[0].thread, "a pins.json from before thread pins reads as agents");
        let text = serde_json::to_string(&pin(false, "AGT-1")).unwrap();
        assert!(!text.contains("thread"), "an agent's pin is written as before: {text}");
        assert!(serde_json::to_string(&pin(true, "N-1")).unwrap().contains("\"thread\":true"));
    }

    #[test]
    fn a_pinned_row_reads_its_title_and_only_an_ended_thread_is_closed() {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        let mk = |n: u64, status: serde_json::Value| -> coord_types::TicketView {
            serde_json::from_value(serde_json::json!({"id": format!("N-{n}"), "n": n, "kind": "thread", "opener": "owner", "ctx": {}, "form": {}, "title": format!("title {n}"),
                "status": status, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": "2026-11-19T07:00:00Z", "given": [], "posts": []})).unwrap()
        };
        let (rest, done) = (mk(1, serde_json::json!({"is": "resting"})), mk(2, serde_json::json!({"is": "done", "result": {"head": "OneOff"}})));
        assert!(!closed(&rest) && closed(&done), "a thread at rest is still open");
        p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(rest) });
        assert_eq!(pin_label(&p, &pin(true, "N-1")), "title 1", "no label from the project: the title");
        assert_eq!(pin_label(&p, &pin(true, "N-9")), "x", "not held at all: the one last seen");
    }

    #[test]
    fn a_long_title_is_cut_before_the_id_is() {
        let t = row_text("Hacker News scan of everything he asked for on the night", "N-1104");
        assert!(t.ends_with("… · N-1104") && t.chars().count() < 36, "{t}");
        assert_eq!(row_text("Short", "N-9"), "Short · N-9");
    }

    #[test]
    fn what_a_pin_fetched_for_one_project_stays_and_is_not_another_projects() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mk = |id: &str| { let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: id.into(), id: id.into(), boxed: false }, Vec::new()); p.down = None; p };
        s.projects = vec![mk("a"), mk("b")];
        let v = |title: &str| -> coord_types::TicketView { serde_json::from_value(serde_json::json!({"id": "N-122", "n": 122, "kind": "thread", "opener": "owner", "ctx": {}, "form": {},
            "title": title, "status": {"is": "done", "result": {"head": "OneOff"}}, "children": [], "program": "", "bound": 3, "spent": 1,
            "opened_at": "2026-11-16T21:00:00Z", "given": [], "posts": []})).unwrap() };
        s.tickets.find.fetched.entry("a".into()).or_default().insert(122, v("in a"));
        s.tickets.find.fetched.entry("b".into()).or_default().insert(122, v("in b"));
        s.cur = 1;
        let t = crate::find::number(&mut s, "N-122").unwrap();
        crate::find::open_at(&mut s, t, "N-122", None);
        // nothing drops what was fetched (poll no longer clears it), and B draws its own thread 122
        let p = s.proj().unwrap();
        assert_eq!(crate::find::ticket(&s, p, 122).map(|v| v.title.clone()), Some("in b".into()));
        s.cur = 0;
        let p = s.proj().unwrap();
        assert_eq!(crate::find::ticket(&s, p, 122).map(|v| v.title.clone()), Some("in a".into()), "A's is still there");
    }

    #[test]
    fn a_pinned_thread_out_of_the_snapshot_opens_from_what_was_fetched() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        s.projects = vec![p];
        s.cur = 0;
        let v: coord_types::TicketView = serde_json::from_value(serde_json::json!({"id": "N-122", "n": 122, "kind": "thread", "opener": "owner", "ctx": {}, "form": {},
            "title": "Hacker News scan", "status": {"is": "done", "result": {"head": "OneOff"}}, "children": [], "program": "", "bound": 3, "spent": 1,
            "opened_at": "2026-11-16T21:00:00Z", "given": [], "posts": []})).unwrap();
        assert!(!s.proj().unwrap().live.tickets.contains_key(&122), "closed long ago: not in the snapshot");
        s.tickets.find.fetched.entry("research".into()).or_default().insert(122, v);
        let t = crate::find::number(&mut s, "N-122").expect("found among the fetched, no coordd asked");
        crate::find::open_at(&mut s, t, "N-122", None);
        assert_eq!((s.sel.clone(), s.tickets.shown), (Sel::Tickets, Some(122)));
    }

    /// The stills of N-891: `STILL_OUT=DIR cargo test thread_pin_stills -- --ignored`. Made-up project: a pinned open
    /// thread and a pinned closed one in the sidebar with the `⋯` open on the open one; then N-122's case, closed as
    /// `OneOff` by the minder, with Reopen behind its `⋯`.
    #[test]
    #[ignore]
    fn thread_pin_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn init(shown: u64) -> App {
            let (_held, mut s) = crate::view_cost::test_app();
            let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
            p.down = None;
            p.protocol = coord_types::PROTOCOL;
            let at = |m: u32| format!("2026-11-19T07:{m:02}:00Z");
            let mk = |v: serde_json::Value| -> coord_types::TicketView { serde_json::from_value(v).unwrap() };
            let open = mk(serde_json::json!({"id": "N-541", "n": 541, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
                "title": "With you empty", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(1), "given": [], "posts": [
                {"n": 1, "at": at(4), "by": "owner", "text": "the red bar for the compaction mark and being able to drag it", "names": ["paper-research"]},
                {"n": 2, "at": at(9), "by": "paper-research", "text": "The circle in the header is the control: drag the red mark on it.", "told": ["owner"]}]}));
            let closed = mk(serde_json::json!({"id": "N-122", "n": 122, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
                "title": "Hacker News scan", "status": {"is": "done", "result": {"head": "OneOff"}}, "children": [], "program": "", "bound": 3, "spent": 1,
                "opened_at": at(2), "given": [], "shaped": [{"n": 1, "at": at(20), "answer": {"head": "OneOff"}, "undone": false}], "posts": [
                {"n": 1, "at": at(5), "by": "owner", "text": "scan Hacker News for anything on P4 verification", "names": ["paper-research"]},
                {"n": 2, "at": at(15), "by": "paper-research", "text": "Nothing new on P4 today.", "told": ["owner"]}]}));
            for (i, v) in [open, closed].into_iter().enumerate() {
                p.live.apply(coord_types::Event { seq: i as u64 + 1, body: coord_types::Body::Ticket(v) });
            }
            s.projects = vec![p];
            s.cur = 0;
            for n in [541u64, 122] {
                let v = s.projects[0].live.tickets[&n].clone();
                let pin = s.thread_pin(&v).unwrap();
                s.pins.list.push(pin);
            }
            crate::find::open_at(&mut s, shown, &format!("N-{shown}"), None);
            s.tickets.more.insert(shown);
            s
        }
        fn pinned_and_menu() -> App {
            init(541)
        }
        fn minder_closed() -> App {
            init(122)
        }
        let shots: [(&str, fn() -> App); 2] = [("pinned-thread-and-menu.png", pinned_and_menu), ("reopen-n122.png", minder_closed)];
        for (name, start) in shots {
            let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), start, crate::view) }).args(args).try_run().unwrap();
        }
    }

    /// An archived thread (N-1073) leaves the Threads list, Starred and Pinned, and stays in the snapshot.
    #[test]
    fn an_archived_thread_leaves_every_list_and_stays_in_the_snapshot() {
        let mk = |archived: bool| -> coord_types::TicketView {
            serde_json::from_value(serde_json::json!({"id": "N-122", "n": 122, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "lead"}, "title": "Hacker News scan",
                "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1, "opened_at": "2026-11-19T07:00:00Z", "given": [],
                "stars": [{"by": "owner", "at": "2026-11-19T07:05:00Z"}], "archived": archived,
                "posts": [{"n": 1, "at": "2026-11-19T07:01:00Z", "by": "owner", "text": "scan", "names": ["lead"]}]})).unwrap()
        };
        let (on, off) = (mk(true), mk(false));
        assert!(on.archived && !off.archived);
        assert!(crate::threads::listed(&off) && !crate::threads::listed(&on), "the Threads list");
        let p = |v: coord_types::TicketView| {
            let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
            p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v) });
            p
        };
        assert_eq!(crate::stars::own(&p(off), &[]).len(), 1, "his star is listed");
        assert!(crate::stars::own(&p(on.clone()), &[]).is_empty(), "an archived thread's star leaves Starred");
        let pin = Pin { project: "research".into(), id: "N-122".into(), agent: "Hacker News scan".into(), thread: true, lens: None };
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![p(on)];
        assert!(archived_pin(&s, &pin), "and the pinned row");
        let wire = serde_json::to_string(&coord_types::Request::Archive { t: "N-122".into(), on: true }).unwrap();
        assert!(wire.contains("archive") && wire.contains("N-122"), "{wire}");
    }

    /// A `became` edge's words as drawn apart (`words_marked`): without their marks, with the byte ranges of the words
    /// removed and of those added; a run too many to compare, removed then added; a cut run's mark closed after its
    /// `…`, the cut said after it; a run whose mark never closes marked to the end; the same words whole.
    #[test]
    fn a_versions_words_are_drawn_without_their_marks_the_changed_ones_marked() {
        assert_eq!(words_marked("1 word removed, 1 added: a b c d [-e-] {+x+} f g"), ("a b c d e x f g".into(), vec![(8, 9)], vec![(10, 11)]));
        assert_eq!(words_marked("2 words added: - one {+- two+}"), ("- one - two".into(), vec![], vec![(6, 11)]));
        let long = "2 words removed, 1 word added (too many to compare word by word: written as removed, then added): [-a b-] {+c+}";
        assert_eq!(words_marked(long), ("a b c".into(), vec![(0, 3)], vec![(4, 5)]));
        let cut = "60 words removed: x [-a b …-] … (cut there: at most 50 words removed and 50 added are written)";
        assert_eq!(words_marked(cut), ("x a b … … (cut there: at most 50 words removed and 50 added are written)".into(), vec![(2, 9)], vec![]));
        assert_eq!(words_marked("1 word added: a {+b"), ("a b".into(), vec![], vec![(2, 3)]));
        let same = "the same words: only their spacing or line breaks changed";
        assert_eq!(words_marked(same), (same.into(), vec![], vec![]));
    }

    /// The block of a list the tree lists (`block`): all of it up to forty; else the forty holding the row at AT, from
    /// the first, or back from the last for a line; AT past a list that shrank, its last block.
    #[test]
    fn a_tree_lists_forty_at_a_time() {
        assert_eq!(block(0, 0, true), (0, 0));
        assert_eq!(block(40, 39, false), (0, 40));
        assert_eq!(block(45, 0, false), (0, 40));
        assert_eq!(block(45, 41, false), (40, 45));
        assert_eq!(block(45, 44, true), (5, 45));
        assert_eq!(block(45, 2, true), (0, 5));
        assert_eq!(block(45, 80, false), (40, 45), "a list that shrank under where he was: its last block");
    }

    /// The standing text a version of his is read against is the newest with a node, beside his versions.
    #[test]
    fn the_standing_text_is_the_newest_with_a_node() {
        let row = |text: &str, of: &str| FolderRow { text: text.into(), of: of.into(), ..Default::default() };
        let group = |rows: Vec<FolderRow>| vec![FolderRow { text: "started with".into(), rows, ..Default::default() }];
        let not_said = row("the standing text, appended; no start line says which version", docket_machine::graph_slice::NOT_SAID);
        let list = group(vec![not_said.clone(), row("the standing text, appended", "text:std"), row("version 1, appended", "text:v1")]);
        assert_eq!(standing_of(&list).as_deref(), Some("text:std"));
        assert!(his_version(&list[0].rows[2]) && !his_version(&list[0].rows[1]) && !his_version(&list[0].rows[0]));
        assert_eq!(standing_of(&group(vec![not_said, row("version 1, appended", "text:v1")])), None, "no node to hold it by");
    }
}
