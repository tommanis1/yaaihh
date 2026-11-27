//! The split (owner 7 Nov night: "side by side, split into 4, like VS Code"): up to four panes side by side (two or
//! three in a row, four as two rows of two), each showing a page of a project; one has focus. The focused pane is the
//! app's `cur` and `sel`, so everything that acts on "the page shown" acts on it; the others are kept in
//! `App::panes` (the focused one's slot there is refreshed whenever focus moves). A pane on an agent shows its chat
//! or terminal and its own composer, whatever project it is from (chat.rs); a pane on another page names the page it
//! holds and shows it once its header is clicked (the pages read the app's current project and page).
//!
//! Split (the toolbar's button, or a double click on an agent in the sidebar: "open to the side") adds a pane; any
//! click inside a pane focuses it (before the click does what it does there), and its header's × closes it.
//!
//! **Drag and drop** (owner 8 Nov, "like VS Code"): an agent dragged from the sidebar (a pin or a running agent) onto
//! the main area shows where it would land: over a pane's left or right third, a new pane on that side of it; over
//! its middle (or anywhere once the split is full), in that pane. The drop does it; a drop over the sidebar does
//! nothing (a pin dropped there still reorders the pins). The split is panes in a row (four as two rows of two), so
//! there is no above/below.
//!
//! **Any page, not only an agent** (the owner, N-1104 p599: "jsut draga nd pin whatvervews yo uwant"; proto-window
//! N-1446 p10): the drag carries a `Sel`, so a pinned lens or a thread of the structure pane lands the same way as an
//! agent. A lens keeps the thread it has open (`Lens.at`) while its pane is not focused, and draws that thread there.

use crate::{App, Proj, Sel, agents, ext, views};
use rune::{Drag, El, Phase, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit};

/// The most panes the split shows.
pub const MAX: usize = 4;

/// The main area's key (lib.rs `view`): the split's panes stand in it, and with no split the page.
pub const MAIN: &str = "main";

/// The key of the split's pane J.
pub fn key(j: usize) -> String {
    format!("pane-{j}")
}

/// The split's pane keyed K ([`key`]): J.
pub fn keyed(k: &str) -> Option<usize> {
    k.strip_prefix("pane-")?.parse().ok()
}

/// A page being dragged from the sidebar: SEL of project PI (an agent, a lens, a thread as the Threads lens at it),
/// NAME by the pointer, the pointer AT (window points).
#[derive(Clone, Debug)]
pub struct AgentDrag {
    pub pi: usize,
    pub name: String,
    pub sel: Sel,
    pub at: [f32; 2],
}

/// Where a dragged agent lands on a pane: a new pane before it or after it, or in it; or on Agents in the sidebar,
/// which pins it (his N-1104 p863: "drag and pin"; paper-research N-1446 p2008).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zone {
    Left,
    Right,
    Center,
    Pin,
}

impl App {
    /// A sidebar row's drag of agent NAME of project PI: followed while it moves; on release, dropped where the view
    /// showed it would land ([`drop_zone`]). Whether the drop was taken (a pin then keeps its place in the list).
    pub fn drag_agent(&mut self, pi: usize, name: &str, d: &Drag) -> bool {
        self.drag_sel(pi, Sel::Agent(name.to_string()), name, d)
    }

    /// A sidebar row's drag of page SEL of project PI, NAME by the pointer: as [`drag_agent`], for any page.
    pub fn drag_sel(&mut self, pi: usize, sel: Sel, name: &str, d: &Drag) -> bool {
        match d.phase {
            Phase::Down => false,
            // a few points of travel first, so a click stays a click
            Phase::Move if self.dragging.is_none() && d.dx.hypot(d.dy) < 4.0 => false,
            Phase::Move => {
                self.dragging = Some(AgentDrag { pi, name: name.to_string(), sel: sel.clone(), at: d.window_point() });
                false
            }
            Phase::Up => {
                self.dragging = None;
                match self.drop_at.take() {
                    Some((j, zone)) if !d.cancelled() => {
                        self.drop_sel(pi, sel, j, zone);
                        true
                    }
                    _ => false,
                }
            }
        }
    }

    /// Page SEL of project PI dropped on pane J (the main area when there is no split) at ZONE; on Agents, pinned
    /// after his other pins (`App::pin_of`), one pinned already left where it is.
    pub(crate) fn drop_sel(&mut self, pi: usize, sel: Sel, j: usize, zone: Zone) {
        match zone {
            Zone::Pin => {
                if let Some(pin) = self.pin_of(pi, &sel).filter(|p| !self.pins.has(p)) {
                    self.pins.toggle(pin);
                }
            }
            Zone::Center => {
                self.focus_pane(j);
                self.cur = pi;
                match sel {
                    Sel::Agent(name) => self.select_agent(&name),
                    other => self.show_sel(other),
                }
            }
            Zone::Left | Zone::Right => self.insert_pane(if zone == Zone::Left { j } else { j + 1 }, pi, sel),
        }
    }

    /// Page SEL of project PI in a pane beside the focused one (the main area, when there is no split), focused: a pane
    /// that shows it already is focused instead, and with the split full the focused pane shows it. Raw log's pane
    /// (raw.rs `open_raw`), and fix 4's System prompt, Initial prompt and Home (chat.rs `pane`). A pane next to the
    /// focused one that holds a page opened this way, or the Map gone to from one ([`opened_beside`]), shows it in
    /// that page's place: Initial prompt pressed beside the Map takes the Map's place, not a third pane
    /// (paper-research N-1446 p3041, 5; proto-window's walk p3038, 5).
    pub fn open_pane_beside(&mut self, pi: usize, sel: Sel) {
        if (self.cur, &self.sel) == (pi, &sel) {
            return;
        }
        if let Some(j) = self.panes.iter().position(|p| (p.0, &p.1) == (pi, &sel)) {
            self.focus_pane(j);
            return;
        }
        let next = [self.focus + 1, self.focus.wrapping_sub(1)].into_iter().find(|&j| self.panes.get(j).is_some_and(|p| opened_beside(&p.1)));
        if let Some(j) = next {
            self.focus_pane(j);
        }
        if next.is_some() || self.panes.len() >= MAX {
            self.cur = pi;
            self.show_sel(sel);
            return;
        }
        self.insert_pane(self.focus + 1, pi, sel);
    }

    /// A new pane at place AT showing page SEL of project PI, focused (nothing when the split is full).
    fn insert_pane(&mut self, at: usize, pi: usize, sel: Sel) {
        if self.panes.is_empty() {
            self.panes.push((self.cur, self.sel_kept()));
            self.focus = 0;
        }
        if self.panes.len() >= MAX {
            return;
        }
        self.panes[self.focus] = (self.cur, self.sel_kept());
        self.panes.insert(at, (pi, sel));
        if at <= self.focus {
            self.focus += 1;
        }
        crate::ask::panes_moved(self, |k| Some(if k >= at { k + 1 } else { k }));
        self.focus_pane(at);
    }

    /// Add a pane showing what the focused one shows, and focus it.
    pub fn split(&mut self) {
        if self.panes.is_empty() {
            self.panes.push((self.cur, self.sel_kept()));
            self.focus = 0;
        }
        if self.panes.len() < MAX {
            self.panes.push((self.cur, self.sel_kept()));
            crate::ask::panes_moved(self, Some);
            self.focus_pane(self.panes.len() - 1);
        }
    }

    /// Focus pane J: its project and page become the app's.
    pub fn focus_pane(&mut self, j: usize) {
        if j == self.focus || j >= self.panes.len() {
            return;
        }
        self.panes[self.focus] = (self.cur, self.sel_kept());
        self.focus = j;
        let (p, sel) = self.panes[j].clone();
        self.cur = p;
        match sel {
            Sel::Agent(name) => self.select_agent(&name),
            other => self.show_sel(other),
        }
    }

    /// Focus the pane on agent NAME of project PI, when one shows it and is not focused.
    pub fn focus_on(&mut self, pi: usize, name: &str) {
        let want = (pi, Sel::Agent(name.to_string()));
        if (self.cur, &self.sel) == (want.0, &want.1) {
            return;
        }
        if let Some(j) = self.panes.iter().enumerate().position(|(i, p)| i != self.focus && *p == want) {
            self.focus_pane(j);
        }
    }

    /// Close pane J; one pane left ends the split.
    pub fn close_pane(&mut self, j: usize) {
        if j >= self.panes.len() {
            return;
        }
        if j == self.focus {
            self.focus_pane(if j == 0 { 1 } else { j - 1 });
        }
        self.panes.remove(j);
        if j < self.focus {
            self.focus -= 1;
        }
        if self.panes.len() == 1 {
            self.panes.clear();
            self.focus = 0;
        }
        crate::ask::panes_moved(self, |k| (k != j).then_some(if k > j { k - 1 } else { k }));
    }

    /// Agent NAME of project PI in a new pane beside the others (in the focused one when the split is full).
    pub fn open_to_side(&mut self, pi: usize, name: &str) {
        if self.panes.len() < MAX {
            self.split();
        }
        self.cur = pi;
        self.select_agent(name);
    }

    /// Whether the split has room for another pane.
    pub fn can_split(&self) -> bool {
        self.panes.len() < MAX
    }
}

/// Whether SEL is a page [`App::open_pane_beside`] opens, or the Map, which System prompt's "› on the Map" shows in
/// that page's pane (canvas.rs `on_map`): Raw log, System prompt, Initial prompt, Home, the Map. The next such page
/// takes its pane.
fn opened_beside(sel: &Sel) -> bool {
    match sel {
        Sel::Raw(_) | Sel::Code(_) => true,
        Sel::Lens(l) => !l.filter.folder_part().is_empty() || l.depth == crate::lens::Depth::Cloud,
        _ => false,
    }
}

/// The focused pane's title and content: the page SEL of project PI.
pub fn page(s: &App, k: &Kit) -> (String, El) {
    let (title, body) = page_of(s, k);
    (title, with_notices(s, s.cur, body, k))
}

/// BODY of a page of project PI under the instance's notices (paper-research N-8: coordd cannot write its log), one
/// red line each, on every page of that project until coordd clears them.
pub fn with_notices(s: &App, pi: usize, body: El, k: &Kit) -> El {
    let Some(p) = s.projects.get(pi) else { return body };
    let lines: Vec<El> = p
        .live
        .instance_notices()
        .map(|n| {
            row()
                .w_full()
                .px(24.0)
                .py(8.0)
                .fill(k.c.danger.bg)
                .border_sides([0.0, 0.0, 1.0, 0.0], k.c.danger.border)
                .child(text(format!("{} · {}", n.text, crate::when::at(&n.at))).font(k.ty.small_medium).color(k.c.danger.fg).wrap())
        })
        .collect();
    if lines.is_empty() {
        return body;
    }
    col().w_full().h_full().min_h(0.0).children(lines).child(col().w_full().grow(1.0).min_h(0.0).child(body))
}

fn page_of(s: &App, k: &Kit) -> (String, El) {
    let (title, body) = page_in(s, s.cur, &s.sel, k);
    let Some(p) = s.proj() else { return (title, body) };
    // the project's pages, the threads' filters and an extension's pages are entries of the sidebar (the owner, N-358:
    // "all of this in the left menu"): the page's whole height is its own
    if project_tab_open(p, &s.sel) {
        return ("Project".into(), ext_body(&s.sel, body, k));
    }
    if threads_open(p, &s.sel) {
        return ("Threads".into(), body);
    }
    (title, body)
}

fn project_tab_open(p: &Proj, sel: &Sel) -> bool {
    is_project_tab(p, sel)
}

fn threads_open(p: &Proj, sel: &Sel) -> bool {
    let sel = named(p, sel);
    matches!(sel, Sel::Tickets | Sel::Mail) || matches!(&sel, Sel::View(id) if boards(p).iter().any(|(b, _)| b == id))
}

/// An extension's page as its tab shows it: with its section beside it when it has pages of its own (the proxy's),
/// and a word to pick one while none is.
fn ext_body(sel: &Sel, body: El, k: &Kit) -> El {
    let Sel::Ext(name, page) = sel else { return body };
    let Some(e) = ext::get(name).filter(|e| ext::has_pages(e)) else { return body };
    if (e.title)(page).is_empty() || page.is_empty() { col().padding(24.0).child(text("Pick one of its pages on the left.").font(k.ty.body).color(k.c.text_muted)) } else { body }
}

/// The project file's boards (its views of kind `board`: Items), by id and title.
pub fn boards(p: &Proj) -> Vec<(String, String)> {
    p.live
        .view_specs()
        .into_iter()
        .filter(|v| v.get("kind").and_then(|x| x.as_str()) == Some("board"))
        .filter_map(|v| {
            let id = v.get("id")?.as_str()?.to_string();
            let title = v.get("title").and_then(|x| x.as_str()).unwrap_or(&id).to_string();
            Some((id, title))
        })
        .collect()
}

/// SEL with the project's home named by its id (`View("")` is the home).
fn named(p: &Proj, sel: &Sel) -> Sel {
    match sel {
        Sel::View(v) if v.is_empty() => Sel::View(p.live.home()),
        other => other.clone(),
    }
}

/// The Project page's tabs, each its label and page: Overview (home.rs), the file's home view, Kinds, the project
/// file's other views but its boards, Files, Settings (policy.rs), and the extensions that are tabs.
pub fn project_tabs(p: &Proj) -> Vec<(String, Sel)> {
    let home = p.live.home();
    let boards: Vec<String> = boards(p).into_iter().map(|(id, _)| id).collect();
    let views: Vec<(String, String)> = p
        .live
        .view_specs()
        .into_iter()
        .filter_map(|v| {
            let id = v.get("id")?.as_str()?.to_string();
            let title = v.get("title").and_then(|x| x.as_str()).unwrap_or(&id).to_string();
            Some((id, title))
        })
        .filter(|(id, _)| !boards.contains(id))
        .collect();
    let mut tabs = Vec::new();
    // Overview (home.rs), first in every project: its home when its file has no home view of its own
    tabs.push(("Overview".to_string(), crate::home::sel(p)));
    for (id, title) in views.iter().filter(|(id, _)| *id == home) {
        tabs.push((title.clone(), Sel::View(id.clone())));
    }
    tabs.push(("Kinds".to_string(), Sel::Kinds));
    for (id, title) in views.iter().filter(|(id, _)| *id != home) {
        tabs.push((title.clone(), Sel::View(id.clone())));
    }
    tabs.push(("Files".to_string(), Sel::Files));
    // Settings and Accounts are the window's, at the sidebar's foot (sidebar.rs `global_rows`), never a project's
    tabs.extend(ext::project_tabs());
    tabs
}

/// Whether A and B are the same tab: equal, or pages of one extension (its tab holds all of them).
fn same_tab(a: &Sel, b: &Sel) -> bool {
    match (a, b) {
        (Sel::Ext(x, _), Sel::Ext(y, _)) => x == y,
        _ => a == b,
    }
}

/// The entry of project P's pages that SEL is (an extension's page is its extension's entry).
pub fn tab_of(p: &Proj, sel: &Sel) -> Option<Sel> {
    let sel = named(p, sel);
    project_tabs(p).into_iter().map(|(_, t)| t).find(|t| same_tab(t, &sel))
}

/// Whether SEL is a tab of project P's Project page.
pub fn is_project_tab(p: &Proj, sel: &Sel) -> bool {
    let sel = named(p, sel);
    project_tabs(p).iter().any(|(_, t)| same_tab(t, &sel))
}

/// The page SEL alone, its title and content.
/// Page SEL of project PI, its title and body: the focused page's (`page`), and a pane's that is not focused (`area`),
/// so a pane keeps drawing the page it holds.
fn page_in(s: &App, pi: usize, sel: &Sel, k: &Kit) -> (String, El) {
    match (s.projects.get(pi), sel) {
        // the window's own pages stand with no project too (N-1934 p4): its Usage, and an extension's (the proxy's)
        (None, Sel::View(id)) if id == crate::global::USAGE => ("Usage".into(), crate::global::usage(s, k)),
        (None, Sel::View(id)) if id == crate::global::ID => ("Home".into(), crate::global::page(s, None, k)),
        (None, Sel::Ext(name, _)) if !ext::get(name).is_some_and(ext::global) => ("yaaihh".to_string(), first_step(s, k)),
        (_, Sel::Ext(name, page)) => match ext::get(name) {
            Some(e) => (e.page)(s, k, page),
            None => (name.clone(), col().padding(24.0).child(text(format!("{name} is not part of this yaaihh")).font(k.ty.body).color(k.c.text_muted))),
        },
        (None, _) => ("yaaihh".to_string(), first_step(s, k)),
        (Some(_), Sel::Agent(name)) => (name.clone(), crate::chat::pane(s, pi, name, true, k)),
        (Some(_), Sel::Raw(name)) => (format!("{name} · raw"), crate::raw::page(s, pi, name, k)),
        (Some(p), Sel::Agents) => ("Agents".into(), agents::page(s, p, k)),
        (Some(p), Sel::Kinds) => ("Kinds".into(), crate::kinds::page(s, p, k)),
        // a pane kept from when the code view had a pane of its own shows Files, which is it now
        (Some(p), Sel::Files | Sel::Code(_)) => ("Files".into(), crate::files::page(s, p, k)),
        (Some(p), Sel::Tickets) => ("Threads".into(), crate::threads::page(s, p, k)),
        (Some(p), Sel::Lens(l)) => (l.name(), crate::lens::page(s, p, l, k)),
        // the Notes page went into Threads (design part 9b); a saved layout naming it opens Threads
        (Some(p), Sel::Mail) => ("Threads".into(), crate::threads::page(s, p, k)),
        (Some(p), Sel::View(id)) => views::view(s, p, id, k),
    }
}

/// The page of a window with no project: what to do first, a project and then an agent in it, with New project
/// (paper-research N-1446 p2424, 0; the walk's empty window showed a blank page, p2423); a notice of the window's own
/// above it.
fn first_step(s: &App, k: &Kit) -> El {
    let notice = s.notice.as_ref().map(|n| text(crate::accounts::plain(n)).font(k.ty.body).color(k.c.text_muted).wrap());
    let new = Button::new("New project").variant(ButtonVariant::Contrast).on_press(|s: &mut App| s.new_project.open = true).build(k).key("first-new-project");
    col()
        .padding(24.0)
        .gap(12.0)
        .children(notice)
        .child(text("No project yet").font(k.ty.ui_medium).color(k.c.text_base))
        .child(text("Make a project, then an agent.").font(k.ty.body).color(k.c.text_muted).wrap())
        .child(row().child(new))
}

/// What an unfocused pane on page SEL of project PI is called.
/// The thread a pane on Threads has open, named as every ticket is (`refs::el`): the focused page's `tickets.shown`, a
/// kept lens's `at` (proto-reader N-1446 p118.1: the pane beside said no thread). The Docket at a thread the same way
/// (docket-edge8, p153), a run fetched too.
fn on_thread(s: &App, pi: usize, sel: &Sel, focused: bool, k: &Kit) -> Option<El> {
    let p = s.projects.get(pi)?;
    let v = match sel {
        Sel::Tickets if focused => s.tickets.shown.and_then(|n| crate::find::ticket(s, p, n))?,
        Sel::Lens(l) if l.shows_threads() => p.live.tickets.values().find(|v| Some(&v.id) == l.at.as_ref())?,
        Sel::Kinds if focused && crate::picture::docket::scale(&s.tickets.pic_open) == crate::picture::docket::Scale::Thread => s.tickets.shown.and_then(|n| crate::find::ticket(s, p, n))?,
        Sel::Lens(l) if l.shows_docket() => l.at.as_deref().and_then(|id| crate::find::held(s, p, id)).and_then(|n| crate::find::ticket(s, p, n))?,
        _ => return None,
    };
    Some(crate::refs::el(&crate::refs::ticket(Some(p), v), k.ty.small, if focused { k.c.text_base } else { k.c.text_muted }, k))
}

pub fn label(s: &App, pi: usize, sel: &Sel) -> String {
    let project = s.projects.get(pi).map_or("?", |p| p.name.as_str());
    format!("{} · {project}", page_name(sel))
}

/// The page SEL shows, by the name the sidebar gives it: "Home", "Threads", "Agents", an agent's name, "Map". The top
/// bar says it, so it says where he is (paper-research N-1446 p3207, 2).
pub fn page_name(sel: &Sel) -> String {
    match sel {
        Sel::Agent(name) => name.clone(),
        Sel::Raw(name) => format!("{name} · raw"),
        Sel::Agents => "Agents".into(),
        Sel::Kinds => "Kinds".into(),
        Sel::Files | Sel::Code(_) => "Files".into(),
        Sel::Tickets => "Threads".into(),
        Sel::Mail => "Threads".into(),
        Sel::Lens(l) => l.name(),
        Sel::Ext(name, page) => ext::get(name).map_or_else(|| name.clone(), |e| (e.title)(page)),
        Sel::View(v) if v == crate::global::ID => "Home".into(),
        Sel::View(v) if v == crate::global::USAGE => "Usage".into(),
        Sel::View(v) if v == crate::policy::ID => "Settings".into(),
        Sel::View(v) if v.is_empty() || v == crate::home::ID => "Overview".into(),
        Sel::View(v) => v.clone(),
    }
}

/// While an agent is dragged: where it would land under the pointer, shown in LAYER (the zone tinted, the agent's
/// name by the pointer, "Pin NAME" over Agents) and kept in `App::drop_at` for the release. Agents and the pins beneath
/// it take what is not pinned yet: a pin's own row dragged there reorders the pins.
pub fn drop_zone(s: &App, cx: &mut rune::Cx<App>, k: &Kit, layer: &mut Vec<El>) {
    let Some(d) = &s.dragging else {
        s.drop_at.set(None);
        return;
    };
    let [x, y] = d.at;
    let inside = |r: &[f32; 4]| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3];
    let pin = cx.layout(crate::sidebar::AGENTS_PINS).filter(|r| inside(r) && s.pin_of(d.pi, &d.sel).is_some_and(|p| !s.pins.has(&p)));
    let rects: Vec<Option<[f32; 4]>> = if s.panes.is_empty() { vec![cx.layout(MAIN)] } else { (0..s.panes.len()).map(|j| cx.layout(&key(j))).collect() };
    let room = s.panes.len() < MAX;
    let hit = rects.iter().enumerate().find_map(|(j, r)| r.filter(|r| inside(r)).map(|r| (j, r)));
    let target = pin.map(|r| (0, Zone::Pin, r)).or_else(|| {
        hit.map(|(j, r)| {
            let f = (x - r[0]) / r[2];
            let zone = if room && f < 0.3 { Zone::Left } else if room && f > 0.7 { Zone::Right } else { Zone::Center };
            (j, zone, r)
        })
    });
    s.drop_at.set(target.map(|(j, z, _)| (j, z)));
    if let Some((_, zone, r)) = target {
        let (zx, zw) = match zone {
            Zone::Left => (r[0], r[2] / 2.0),
            Zone::Right => (r[0] + r[2] / 2.0, r[2] / 2.0),
            Zone::Center | Zone::Pin => (r[0], r[2]),
        };
        layer.push(
            rune::el()
                .key("drop-zone")
                .pin(Some(zx), Some(r[1]), None, None)
                .w(zw)
                .h(r[3])
                .radius(k.radius.md)
                .fill(k.c.border_focus.alpha(0.18))
                .border(2.0, k.c.border_focus),
        );
    }
    layer.push(
        row()
            .key("drop-ghost")
            .pin(Some(x + 14.0), Some(y + 10.0), None, None)
            .px(8.0)
            .py(4.0)
            .radius(k.radius.sm)
            .fill(k.c.layer_02)
            .border(1.0, k.c.border_weak)
            .child(text(if pin.is_some() { format!("Pin {}", d.name) } else { d.name.clone() }).font(k.ty.small_medium).color(k.c.text_base)),
    );
}

/// The main area: the focused page alone, or the split with FOCUSED (the focused pane's content) in its place.
pub fn area(s: &App, focused: El, k: &Kit) -> El {
    if s.panes.is_empty() {
        return focused;
    }
    let mut focused = Some(focused);
    let panes: Vec<El> = (0..s.panes.len())
        .map(|j| {
            let is_focus = j == s.focus;
            let (pi, sel) = if is_focus { (s.cur, s.sel.clone()) } else { s.panes[j].clone() };
            let body = if is_focus {
                focused.take().unwrap_or_else(col)
            } else {
                // what is drawn in it knows its pane (the comment box's passage is lit in its own: ask.rs `ours`), and
                // the texts it draws unread are asked through its project's pipe (texts.rs `drawn_in`)
                s.notes.drawing.set(Some(j));
                crate::texts::drawn_in(s, pi, || match &sel {
                    Sel::Agent(name) => crate::chat::pane(s, pi, name, false, k),
                    // a raw pane follows its agent whether focused or not
                    Sel::Raw(name) => crate::raw::page(s, pi, name, k),
                    // the code view stays in sight beside the pane he works in
                    Sel::Files | Sel::Code(_) if crate::code::shows(s, pi) => crate::code::page(s, pi, k),
                    // the Map in one place only, the rest saying where it is (map/canvas.rs `apart`)
                    Sel::Lens(l) if l.depth == crate::lens::Depth::Cloud => crate::map::canvas::apart(s, j, k),
                    // a lens shows its list, the thread it keeps picked in it (lens.rs)
                    Sel::Lens(l) => crate::lens::apart(s, pi, l, k),
                    // every other page as it is drawn focused, of its own project (`page_in`)
                    _ => ext_body(&sel, page_in(s, pi, &sel, k).1, k),
                })
            };
            let head = k
                .interactive(
                    row()
                        .w_full()
                        .px(12.0)
                        .py(6.0)
                        .gap(8.0)
                        .center_y()
                        .fill(if is_focus { k.c.layer_02 } else { k.c.layer_01 })
                        .child(text(label(s, pi, &sel)).font(if is_focus { k.ty.small_medium } else { k.ty.small }).color(if is_focus { k.c.text_base } else { k.c.text_muted }).truncate().shrink())
                        .children(on_thread(s, pi, &sel, is_focus, k))
                        .child(rune::el().grow(1.0)),
                )
                .on_press(move |s: &mut App| s.focus_pane(j));
            let close = Button::new("×").variant(ButtonVariant::GhostMuted).on_press(move |s: &mut App| s.close_pane(j)).build(k);
            col()
                .key(key(j))
                // any click inside a pane focuses it, also on its buttons, fields and text (owner 8 Nov)
                .on_press_within(move |s: &mut App| s.focus_pane(j))
                .grow(1.0)
                .min_w(0.0)
                .min_h(0.0)
                .h_full()
                .radius(k.radius.md)
                .clip_rounded(k.radius.md)
                .border(1.0, if is_focus { k.c.border_focus } else { k.c.border_weak })
                .child(row().w_full().center_y().child(head.grow(1.0)).child(close))
                .child(col().w_full().grow(1.0).min_h(0.0).child(if is_focus { body } else { with_notices(s, pi, body, k) }))
        })
        .collect();
    let line = |els: Vec<El>| row().w_full().grow(1.0).min_h(0.0).gap(6.0).children(els);
    let mut panes = panes;
    let body = if panes.len() == 4 {
        let bottom = panes.split_off(2);
        col().w_full().h_full().gap(6.0).child(line(panes)).child(line(bottom))
    } else {
        col().w_full().h_full().child(line(panes))
    };
    body.padding(6.0)
}

#[cfg(test)]
mod bar_tests {
    use crate::{App, Sel};

    /// The top bar says where he is (paper-research N-1446 p3207, 2; docket-edge8's walk on build 89, p3205): the page's
    /// name after the arrows and find, "Home", "Threads", "Agents"; an agent's page its name (Rename's title, as before).
    /// Docket and Map stay where he knows them, right after it (p3226).
    #[test]
    fn the_top_bar_says_where_he_is_before_docket_and_map() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![crate::view_cost::project_for_tests()];
        s.cur = 0;
        let agent = s.projects[0].live.agents[0].name.clone();
        let mut rt = crate::view_cost::window(s);
        let title = |rt: &rune::runtime::Runtime<App>| rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some("bar-title")).map(|e| (e.text.unwrap_or_default(), e.rect));
        let at = |rt: &rune::runtime::Runtime<App>, key: &str| rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).map(|e| e.rect);
        for (sel, name) in [(Sel::View(String::new()), "Overview"), (Sel::View(crate::global::ID.into()), "Home"), (Sel::View(crate::policy::ID.into()), "Settings"), (Sel::Tickets, "Threads"), (Sel::Agents, "Agents")] {
            rt.update(move |s, _| s.sel = sel.clone());
            rt.invalidate();
            for _ in 0..3 {
                rt.frame(0.016);
            }
            let (t, r) = title(&rt).unwrap_or_else(|| panic!("{name}: the bar says the page"));
            assert_eq!(t, name);
            let docket = at(&rt, "docket-mark").expect("Docket on the bar");
            assert!(docket[0] >= r[0] + r[2] && docket[0] < r[0] + r[2] + 40.0, "{name}: Docket right after the page's name, title at {r:?}, Docket at {docket:?}");
        }
        rt.update(move |s, _| s.sel = Sel::Agent(agent.clone()));
        rt.invalidate();
        for _ in 0..3 {
            rt.frame(0.016);
        }
        assert!(title(&rt).is_none(), "an agent's page: its name, as Rename's title");
    }
}

#[cfg(test)]
#[path = "panes_tests.rs"]
mod tests;
