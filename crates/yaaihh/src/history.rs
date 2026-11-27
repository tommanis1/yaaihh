//! Back, forward and History: the window remembers where he was, so a click does not lose his place (the owner,
//! N-890.p1: "ah the ui need a back and a history button so i dont lost my place in the ui if i click something").
//!
//! A place is the focused pane's project and page (`Sel`) and the thread open on it (the Threads page's
//! `tickets.shown`; an agent's With you thread, `withyou.inside`). It carries the agent whose detail panel is open (the
//! circle's, `circle.open`) and the row at the top of its view when he left it; both come back with it, and neither
//! makes a place (consult-3, N-412 post 920: a panel opened and closed made two places for one page). That row is read_place.rs's own: the row it keeps for the view
//! (`Places::kept`). Going back hands the row back to read_place.rs (`Places::bring`), and it is scrolled there the same
//! way a place comes back after a swap. There is no second scroll memory.
//!
//! Nothing that opens a page tells this module about it. At the start of each frame (`poll`) the place shown is compared
//! with the one recorded. When the project, page or open thread has changed, the place he left goes on the back
//! list with its row, and the forward list is cleared. So a link followed in a message, a reference pressed, a row
//! opened from a list, a sidebar entry and a question's post each make a place, wherever their code sets the
//! selection. Scrolling only moves the row of the place he is on; it makes no place. Moving the focus between the
//! split's panes, or opening or closing a pane, makes none either: the place recorded is then the newly focused pane's.
//!
//! On the Map's cloud (cloud.rs), the card he went into, its focus, is part of the place: each card he goes into (a
//! click, a double-click, Enter, a search's find: cloud.rs `went_in`) is a place, as Sourcetrail's back and forward
//! walk the symbols he made active (paper-research N-1446 p1758; the study p1655, 3(c)). Here too nothing tells this
//! module: its poll reads the cloud's focus. A place on the cloud also keeps the camera he last rested on there and the
//! node selected, as a thread's keeps its row; neither makes a place. Going back to one makes it the focus again, its
//! node selected again, and flies to that camera: where he was, zoomed or not, whatever card he had gone into (p2116).
//! A place kept without a camera (by an older window, in ui.json) flies into its card, as his click did (canvas.rs
//! `fly_to_id`); to the cloud before any, the focus goes and the camera flies out to the project's card whole. A focus
//! whose card is not in the cloud drawn (an agent's post of a slice since asked again) comes back as the focus alone,
//! its slice asked again, the camera where it is.
//!
//! `Back` and forward step through the lists as a browser does: Alt+Left and Alt+Right (when no text field holds the
//! keys: in a field Alt+arrows move the caret by words, rune's editor), the mouse's back and forward buttons over the
//! page, and the two arrows at the start of the bar. A press on the quiet mark between them (`⌄`) opens History: the
//! last places he left, newest first, each as its reference (label, the id quiet). A press on one goes there, and the
//! places between move to the forward list, as in a browser's history menu. The lists go into ui.json with the rest of
//! the window's state (state.rs), so a swap keeps them.
//!
//! Under the places, "Your window": the earlier states of his whole window, newest first (state.rs's deltas: the
//! panes, the tabs, the pins, the lenses, the Map's moment), each in words and its time. A press puts his window back
//! as it was then, what he typed and his mail as they are; that is a state too, so nothing after it is lost
//! (paper-research N-1446 p2207; his N-687 p8). A place is where he was; a state is how his window was.

use crate::{App, Sel};
use rune::{El, KeyEvent, Tx};
use rune_kit::Kit;
use serde::{Deserialize, Serialize};

/// The most places kept on each list: the oldest drop off.
pub const KEEP: usize = 50;

/// The most places History shows.
pub const SHOWN: usize = 15;

/// Where he was: what is restored when he goes back to it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Place {
    /// The project, by id.
    pub project: String,
    pub sel: Sel,
    /// The thread open on the page: the Threads page's, or an agent's With you thread.
    #[serde(default)]
    pub thread: Option<u64>,
    /// The agent whose detail panel (the circle's) was open when he left: restored with the place, never a place.
    #[serde(default)]
    pub panel: Option<String>,
    /// The key of the row at the top of the view when he left it ("" at a chat's end); none on a page without one.
    #[serde(default)]
    pub top: Option<String>,
    /// On the Map's cloud, the card he went into (`N-1104.p815`, cloud.rs `went_in`): none before any, and on every
    /// other page.
    #[serde(default)]
    pub focus: Option<String>,
    /// On the Map's cloud, the camera he last rested on there (`[x, y, scale]`, canvas.rs `View::cam`; not taken while
    /// a flight is on its way) and the node selected: both come back with the place, as its row does, and neither makes
    /// one (a wheel's zoom is no place). paper-research N-1446 p2101, the walk p2116 1–3: Back flew to the card's fit or
    /// the project's, not to where he was, and a post he had selected was lost.
    #[serde(default)]
    pub cam: Option<[f64; 3]>,
    #[serde(default)]
    pub picked: Option<(String, String)>,
    /// On the Agents page, its tab (0 Active, 1 All agents; agents.rs `agents_tab`): each a place of its own, so Back
    /// brings Active again after the "stopped: …" line opened All agents (paper-research N-1446 p2120, the walk's 4).
    /// None on every other page, and in a place kept before it was kept.
    #[serde(default)]
    pub tab: Option<usize>,
    /// On the Kinds page, its Docket as he left it ([`board`]): the scale, the agent picked, the program card open and
    /// what a press opened under the board, each change a place of its own, so Back returns to the scale and the card
    /// he came from (the walk's 5). Empty on every other page, and in a place kept before it was kept.
    #[serde(default)]
    pub board: Vec<String>,
    /// Its reference when he was there, for History: the label and the id (empty for a page).
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub id: String,
}

impl Place {
    /// The same place, whatever its panel, row and label.
    fn same(&self, o: &Place) -> bool {
        (&self.project, &self.sel, self.thread, &self.focus, self.tab, &self.board) == (&o.project, &o.sel, o.thread, &o.focus, o.tab, &o.board)
    }
}

#[derive(Default)]
pub struct History {
    /// The places he left, the last one last.
    pub back: Vec<Place>,
    /// The places gone back from, the next one last.
    pub ahead: Vec<Place>,
    /// The place shown at the last poll.
    now: Option<Place>,
    /// The focused pane and the number of panes at the last poll.
    panes: (usize, usize),
    /// History open, from a press on its mark.
    pub open: bool,
}

impl History {
    /// HERE is shown, in pane state PANES: true when it is a new place (the one left went on the back list). The same
    /// place takes HERE's label, its node selected and its camera (the one it last rested on while a flight is on its
    /// way); a change of pane only records HERE.
    fn arrive(&mut self, here: Place, panes: (usize, usize), top: impl FnOnce(&Place) -> Option<String>) -> bool {
        let moved_pane = std::mem::replace(&mut self.panes, panes) != panes;
        match self.now.take() {
            Some(mut was) if !moved_pane && !was.same(&here) => {
                was.top = top(&was);
                push(&mut self.back, was);
                self.ahead.clear();
                self.now = Some(here);
                true
            }
            Some(was) if was.same(&here) => {
                let cam = here.cam.or(was.cam);
                self.now = Some(Place { top: was.top, cam, ..here });
                false
            }
            _ => {
                self.now = Some(here);
                false
            }
        }
    }

    /// Steps N places back (or ahead), the place left taking row TOP: the place reached, none when the list is
    /// shorter than N.
    fn walk(&mut self, back: bool, n: usize, top: Option<String>) -> Option<Place> {
        let len = if back { self.back.len() } else { self.ahead.len() };
        if n == 0 || len < n {
            return None;
        }
        let (from, to) = if back { (&mut self.back, &mut self.ahead) } else { (&mut self.ahead, &mut self.back) };
        if let Some(mut now) = self.now.take() {
            now.top = top;
            push(to, now);
        }
        for _ in 1..n {
            push(to, from.pop()?);
        }
        self.now = from.pop();
        self.now.clone()
    }
}

/// P put last on list L, the oldest dropped past [`KEEP`].
fn push(l: &mut Vec<Place>, p: Place) {
    l.push(p);
    if l.len() > KEEP {
        l.remove(0);
    }
}

/// The thread page SEL of project PI shows: the Threads page's (`tickets.shown`), or the one beside an agent's With
/// you (`withyou.inside`, kept for each project); none on every other page, a lens below Structure among them. The
/// place kept ([`here`]) and where a popup's passage is commented on to (popup.rs `goes_to`; consult-3 N-1477 p64).
pub(crate) fn thread_shown(s: &App, pi: usize, sel: &Sel) -> Option<u64> {
    match sel {
        sel if sel.shows_threads() => crate::threads::picked(s, pi, None),
        sel => sel.with_you().and_then(|a| crate::threads::picked(s, pi, Some(a))),
    }
}

/// Whether the focused pane draws the cloud of project PI, whose focus is `cloud.focus` (cloud.rs).
fn on_cloud(s: &App, pi: usize) -> bool {
    crate::map::canvas::shown(s) && s.map.is_cloud() && !s.cloud.lanes && s.cloud.of == pi
}

/// The label of the cloud's card ID in project PI, as the canvas draws it; none when it is not drawn.
fn card_label(s: &App, pi: usize, id: &str) -> Option<String> {
    let g = s.map.graph()?;
    let i = g.project(&crate::map::canvas::project_id(s, pi)?).and_then(|r| g.find_in(r, id))?;
    Some(g.node(i).label.clone())
}

/// What History calls page SEL of the focused pane's project when no thread or card is in it: its pane's name
/// (panes.rs `label`) with, after the page's name, on the Agents page the tab shown, on the Kinds page the scale its
/// Docket is drawn at, the agent picked and the program card open, so the places of one page read apart in History's
/// list ("Kinds · research" three times in a row, paper-research N-1446 p2188). Before the project's name: the list is
/// narrow and cuts a row's end ("Kinds · Research (ho…" in his window).
fn page_label(s: &App, sel: &Sel) -> String {
    use crate::picture::docket::{FOCUS, picked, scale, shown_kind};
    let l = crate::panes::label(s, s.cur, sel);
    let more = match sel {
        Sel::Agents => vec![crate::agents::TABS.get(s.agents_tab).unwrap_or(&crate::agents::TABS[0]).to_string()],
        Sel::Kinds => {
            let open = &s.tickets.pic_open;
            let mut m = vec![format!("{} scale", scale(open).label())];
            m.extend([picked(open, FOCUS), shown_kind(open)].into_iter().flatten().map(String::from));
            m
        }
        _ => return l,
    };
    l.replacen(" · ", &format!(" · {} · ", more.join(" · ")), 1)
}

/// The label History gives the place shown in the focused pane ([`here`]): the words of a state of his window
/// (state.rs `said`).
pub(crate) fn shown_label(s: &App) -> Option<String> {
    here(s).map(|p| p.label).filter(|l| !l.is_empty())
}

/// The place shown in the focused pane, none without a project.
fn here(s: &App) -> Option<Place> {
    let p = s.projects.get(s.cur)?;
    let thread = thread_shown(s, s.cur, &s.sel);
    let panel = s.circle.open.as_ref().filter(|(i, _)| *i == s.cur).map(|(_, a)| a.clone());
    let cloud = on_cloud(s, s.cur);
    let focus = s.cloud.focus.clone().filter(|_| cloud);
    // where the camera rests, and what is selected: a flight's camera is on its way, so the one before it stays
    let c = s.map.cam;
    let cam = [c.at[0], c.at[1], c.scale];
    let cam = (cloud && !s.map.flying() && cam.iter().all(|x| x.is_finite()) && c.scale > 0.0).then_some(cam);
    let picked = s.map.sel.clone().filter(|_| cloud);
    let v = thread.and_then(|t| crate::find::ticket(s, p, t));
    let r = match (&s.sel, v, &focus) {
        // a card he went into on the cloud: its label, its id
        (_, _, Some(f)) => crate::refs::Ref { label: card_label(s, s.cur, f).unwrap_or_else(|| f.clone()), id: f.clone() },
        (Sel::Tickets | Sel::Lens(_) | Sel::Agent(_), Some(v), _) => crate::refs::ticket(Some(p), v),
        (Sel::Agent(a), None, _) => crate::refs::who(Some(p), a),
        (sel, _, _) => crate::refs::Ref { label: page_label(s, sel), id: String::new() },
    };
    let tab = (s.sel == Sel::Agents).then_some(s.agents_tab);
    Some(Place { project: p.id.clone(), sel: s.sel.clone(), thread, panel, top: None, focus, cam, picked, tab, board: board(s), label: r.label, id: r.id })
}

/// The keys of the Kinds page's Docket in `pic_open` that make a place (picture/docket.rs): the agent picked at the
/// Agent scale, the program card open under the board, what a press opened under it. The scale is kept apart, by its
/// label ([`board`]).
const BOARD: [&str; 3] = [crate::picture::docket::FOCUS, crate::picture::docket::SHOWN, crate::picture::docket::PANEL];

/// What of the Kinds page's Docket makes a place, sorted: the scale it is drawn at, by its label (Project when none is
/// picked, so the first look and a press on Project are one place), and the [`BOARD`] keys open; none on any other
/// page.
fn board(s: &App) -> Vec<String> {
    use crate::picture::docket::{SCALE, scale};
    if s.sel != Sel::Kinds {
        return Vec::new();
    }
    let open = &s.tickets.pic_open;
    let mut b = vec![format!("{SCALE}{}", scale(open).label())];
    b.extend(open.iter().filter(|k| BOARD.iter().any(|p| k.starts_with(p))).cloned());
    b.sort();
    b
}

/// The view (read_place.rs) whose top row place P keeps: an open thread's, else an agent's chat.
fn view(s: &App, pl: &Place) -> Option<String> {
    let p = s.projects.iter().find(|p| p.id == pl.project)?;
    match (&pl.sel, pl.thread.and_then(|t| crate::find::ticket(s, p, t))) {
        (Sel::Tickets | Sel::Lens(_) | Sel::Agent(_), Some(v)) => Some(crate::read_place::thread_view(&p.id, &v.id)),
        (Sel::Agent(a), None) => Some(crate::read_place::chat_view(&p.id, a)),
        _ => None,
    }
}

/// The row at the top of place P's view now.
fn top(s: &App, p: &Place) -> Option<String> {
    view(s, p).and_then(|v| s.read_places.kept.get(&v).cloned())
}

/// Each frame: the place shown, recorded. True when a place was made; then the popups over the page left close, so a
/// press in one that went somewhere shows where it went (popup.rs).
pub fn poll(s: &mut App) -> bool {
    let Some(here) = here(s) else { return false };
    let panes = (s.focus, s.panes.len());
    let mut h = std::mem::take(&mut s.history);
    let made = h.arrive(here, panes, |p| top(s, p));
    // the thread left opens where he read it when he comes back to it (read_place.rs `left`)
    if let Some(v) = made.then(|| h.back.last().and_then(|was| view(s, was))).flatten() {
        s.read_places.left(&v);
    }
    s.history = h;
    if made {
        crate::popup::close(s);
        // a picture open over the page left closes with it, as the popups do (the walk's 7)
        s.viewing = None;
    }
    made
}

/// One step back or ahead: the arrows, Alt+Left and Alt+Right, the mouse's buttons. A picture open over the page
/// (viewer.rs) is what Back closes first, and it goes no further; Forward goes on, and the picture closes with the page
/// it was over (`go`). The walk's 7, paper-research N-1446 p2120.
fn step(s: &mut App, back: bool) {
    if back && s.viewing.take().is_some() {
        return;
    }
    go(s, back, 1);
}

/// He goes N places back (or ahead).
pub fn go(s: &mut App, back: bool, n: usize) {
    // a place in a project this window no longer has cannot be reached
    let ids: Vec<String> = s.projects.iter().map(|p| p.id.clone()).collect();
    s.history.back.retain(|p| ids.contains(&p.project));
    s.history.ahead.retain(|p| ids.contains(&p.project));
    let row = here(s).and_then(|p| top(s, &p));
    let Some(to) = s.history.walk(back, n, row) else { return };
    let Some(i) = s.projects.iter().position(|p| p.id == to.project) else { return };
    // a picture open over the page left closes with it: a step makes no place, so `poll` does not close it here
    s.viewing = None;
    s.cur = i;
    s.sel = to.sel.clone();
    match &to.sel {
        sel if sel.shows_threads() => s.tickets.shown = to.thread,
        sel => {
            if let Some(a) = sel.with_you() {
                let k = (i, a.to_string());
                match to.thread {
                    Some(t) => s.withyou.inside.insert(k, t),
                    None => s.withyou.inside.remove(&k),
                };
            }
        }
    }
    s.circle.open = to.panel.clone().map(|a| (i, a));
    // the Agents page's tab and the Kinds page's Docket as he left them (the walk's 4 and 5); a place kept without
    // them leaves both as they are
    if let Some(t) = to.tab {
        s.agents_tab = t;
    }
    if !to.board.is_empty() {
        use crate::picture::docket::{SCALE, SHOWN};
        s.tickets.pic_open.retain(|k| !k.starts_with(SCALE) && !BOARD.iter().any(|p| k.starts_with(p)));
        s.tickets.pic_open.extend(to.board.iter().cloned());
        // a card open under the board is open on the Kinds page too, as its press opened it (docket.rs `show_program`)
        s.kinds_open.extend(to.board.iter().filter_map(|k| k.strip_prefix(SHOWN)).map(String::from));
    }
    if let (Some(v), Some(row)) = (view(s, &to), &to.top) {
        s.read_places.bring(&v, row);
    }
    // on the cloud: its focus again, its node selected again and the camera he left it at flown to; a place kept
    // without a camera, flown into its card as his click did, or before any, out to the project's card whole
    if on_cloud(s, i) {
        s.cloud.focus = to.focus.clone();
        match (to.cam, &to.focus) {
            (Some([x, y, scale]), _) => {
                let picked = to.picked.as_ref().and_then(|n| s.map.pick(n));
                crate::map::canvas::select(s, picked);
                s.map.fly_cam(crate::map::camera::Camera { at: [x, y], scale });
            }
            (None, Some(f)) => {
                crate::map::canvas::fly_to_id(s, i, f);
            }
            (None, None) => {
                crate::map::canvas::select(s, None);
                let whole = crate::map::canvas::project_id(s, i).and_then(|id| s.map.graph().and_then(|g| g.project(&id).map(|r| g.node(r).rect)));
                if let Some(r) = whole {
                    s.map.fly_to(&r, false);
                }
            }
        }
    }
    s.history.open = false;
}

/// Whether one Back goes to a place WHICH names, in the project shown. A page's own way back (the bar's "‹ Agents",
/// Kinds' "‹ the thread") is then that Back, never a place of its own, so Back after it does not come to the page it
/// left again: one back and forward across everything (the walk's 3 and 6, paper-research N-1446 p2101, p2120).
pub fn back_is(s: &App, which: impl Fn(&Place) -> bool) -> bool {
    let Some(p) = s.proj() else { return false };
    s.history.back.last().is_some_and(|b| b.project == p.id && which(b))
}

/// Alt+Left and Alt+Right: whether the key was one of them.
pub fn key(s: &mut App, ev: &KeyEvent) -> bool {
    let back = match ev.key {
        rune::Key::Left => true,
        rune::Key::Right => false,
        _ => return false,
    };
    if !ev.mods.alt || ev.mods.cmd || ev.mods.ctrl || ev.mods.shift {
        return false;
    }
    step(s, back);
    true
}

/// BODY taking the mouse's back and forward buttons: they go back and ahead. Its primary press does nothing.
pub fn buttons(body: El) -> El {
    body.pointer_buttons([rune::PointerButton::Back, rune::PointerButton::Forward]).on_press_tx(|s: &mut App, tx: &mut Tx| match tx.button() {
        rune::PointerButton::Back => step(s, true),
        rune::PointerButton::Forward => step(s, false),
        _ => {}
    })
}

/// The bar's start, the same on every page (docs/specs/window-rules.md §10): the two arrows with History's mark between
/// them, each a quiet glyph (`small`, `text_faint`; the disabled ink when there is nothing to go to). History opens
/// from a press on the mark, into LAYER: the places he left, then "Your window", the earlier states of his window
/// (state.rs `window_states`), each its words and its time; a press puts his window back as it was then (state.rs
/// `go_back_to`; paper-research N-1446 p2207).
pub fn arrows(s: &App, k: &Kit, layer: &mut Vec<El>) -> El {
    use rune_kit::{Menu, MenuItem};
    let arrow = |glyph, help, back: bool, none| {
        quiet(k, glyph, help, none, if back { "history-back" } else { "history-forward" }, move |s: &mut App, _: &mut Tx| step(s, back))
    };
    let none = s.history.back.is_empty() && !crate::state::has_states(s);
    let pair = rune::row()
        // lit while a picture is open over the page, which Back closes (`step`), with no place behind it too
        .child(arrow("‹", "Back (Alt+Left)", true, s.history.back.is_empty() && s.viewing.is_none()))
        // History by a plain press too, so nothing in it waits on a gesture he has to know (paper-research N-893.p3);
        // his window's states read as it opens
        .child(quiet(k, "⌄", "History", none, "history-open", |s: &mut App, _: &mut Tx| {
            s.history.open = !s.history.open;
            if s.history.open {
                crate::state::read_deltas(s);
            }
        }))
        .child(arrow("›", "Forward (Alt+Right)", false, s.history.ahead.is_empty()))
        .key("history-arrows");
    let mut menu = Menu::new("history").trigger(pair).open(s.history.open).on_close(|s: &mut App| s.history.open = false);
    for (n, p) in s.history.back.iter().rev().take(SHOWN).enumerate() {
        let mut item = MenuItem::new(p.label.clone()).on_select(move |s: &mut App| go(s, true, n + 1));
        if !p.id.is_empty() {
            item = item.shortcut(p.id.clone());
        }
        menu = menu.item(item);
    }
    if s.history.back.is_empty() {
        menu = menu.item(MenuItem::new("Nowhere yet").disabled(true));
    }
    let states = crate::state::window_states(s, SHOWN);
    if !states.is_empty() {
        // wide enough for a state's words and its time beside them, as "pr · the Map at 19 Nov 11:00" is
        menu = menu.min_width(320.0).separator().group_label("Your window");
        for (n, said, at) in states {
            menu = menu.item(MenuItem::new(said).shortcut(at).on_select(move |s: &mut App| {
                s.history.open = false;
                crate::state::go_back_to(s, n);
            }));
        }
    }
    menu.build(k, layer)
}

/// One of the bar's quiet marks (the arrows, History's, find's): GLYPH in `small`, `text_faint`, the disabled ink when
/// NONE; HELP on hover; KEY its key. The press is the button's own, so a dimmed one takes none.
pub(crate) fn quiet(k: &Kit, glyph: &'static str, help: &'static str, none: bool, key: &'static str, press: impl Fn(&mut App, &mut Tx) + 'static) -> El {
    use rune_kit::{Button, ButtonVariant};
    let (small, faint) = (k.ty.small, k.c.text_faint);
    Button::new(glyph)
        .variant(ButtonVariant::Ghost)
        .disabled(none)
        .help(help)
        .label_with(move |_, ink| rune::text(glyph).font(small).color(if none { ink } else { faint }))
        .on_press_tx(press)
        .build(k)
        .key(key)
}

/// The `history` scene: on the Threads page's scene, the places of a walk through the project made as he would make
/// them (its newest threads, the Agents page, an agent's chat), then History open over the arrows.
pub fn scene(s: &mut App) {
    let Some(p) = s.projects.get(s.cur) else { return };
    let mut threads: Vec<u64> = p.live.tickets.keys().copied().collect();
    threads.sort_unstable_by(|a, b| b.cmp(a));
    let mut walk: Vec<(Sel, Option<u64>)> = threads.into_iter().take(4).map(|t| (Sel::Tickets, Some(t))).collect();
    walk.insert(walk.len().min(1), (Sel::Agents, None));
    if let Some(a) = p.live.agents.iter().find(|a| a.kind != "program") {
        walk.insert(walk.len().min(2), (Sel::Agent(a.name.clone()), None));
    }
    let (sel, shown) = (s.sel.clone(), s.tickets.shown);
    for (sel, t) in walk.into_iter().chain([(sel, shown)]) {
        s.sel = sel;
        s.tickets.shown = t;
        poll(s);
    }
    s.history.open = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(sel: Sel, thread: Option<u64>) -> Place {
        Place { project: "b".into(), sel, thread, panel: None, top: None, focus: None, cam: None, picked: None, tab: None, board: Vec::new(), label: String::new(), id: String::new() }
    }

    /// The Map's cloud page (sidebar.rs's Map entry).
    fn cloud_page() -> Sel {
        Sel::Lens(crate::lens::Lens { filter: crate::lens::Filter::Threads, depth: crate::lens::Depth::Cloud, at: None })
    }

    #[test]
    fn a_new_page_makes_a_place_and_its_row_is_kept() {
        let mut h = History::default();
        assert!(!h.arrive(at(Sel::Tickets, Some(12)), (0, 0), |_| None), "the first place is only recorded");
        assert!(!h.arrive(at(Sel::Tickets, Some(12)), (0, 0), |_| Some("never".into())), "the same place: nothing");
        assert!(h.arrive(at(Sel::Agent("orch".into()), None), (0, 0), |_| Some("N-12-p7".into())));
        assert_eq!(h.back.len(), 1);
        assert_eq!((h.back[0].thread, h.back[0].top.as_deref()), (Some(12), Some("N-12-p7")), "the thread and the row he left");
    }

    #[test]
    fn another_thread_on_the_same_page_is_a_place() {
        let mut h = History::default();
        h.arrive(at(Sel::Tickets, Some(12)), (0, 0), |_| None);
        assert!(h.arrive(at(Sel::Tickets, Some(13)), (0, 0), |_| None));
        assert!(h.arrive(at(Sel::Tickets, None), (0, 0), |_| None), "back to the list");
        assert_eq!(h.back.iter().map(|p| p.thread).collect::<Vec<_>>(), vec![Some(12), Some(13)]);
    }

    #[test]
    fn back_then_forward_comes_back_to_the_same_place_and_row() {
        let mut h = History::default();
        h.arrive(at(Sel::Tickets, Some(12)), (0, 0), |_| None);
        h.arrive(at(Sel::Agents, None), (0, 0), |_| Some("N-12-p7".into()));
        let to = h.walk(true, 1, None).unwrap();
        assert_eq!((to.thread, to.top.as_deref()), (Some(12), Some("N-12-p7")));
        assert!(!h.arrive(to, (0, 0), |_| None), "arriving where it went makes no place");
        let ahead = h.walk(false, 1, Some("N-12-p9".into())).unwrap();
        assert_eq!(ahead.sel, Sel::Agents);
        assert_eq!(h.back[0].top.as_deref(), Some("N-12-p9"), "the row he had scrolled to before going forward");
        assert!(h.walk(false, 1, None).is_none(), "nothing further ahead");
    }

    #[test]
    fn a_new_place_after_back_clears_forward() {
        let mut h = History::default();
        h.arrive(at(Sel::Tickets, Some(12)), (0, 0), |_| None);
        h.arrive(at(Sel::Agents, None), (0, 0), |_| None);
        let to = h.walk(true, 1, None).unwrap();
        h.arrive(to, (0, 0), |_| None);
        assert_eq!(h.ahead.len(), 1);
        h.arrive(at(Sel::Files, None), (0, 0), |_| None);
        assert!(h.ahead.is_empty());
    }

    #[test]
    fn history_jumps_several_places_and_keeps_those_between_ahead() {
        let mut h = History::default();
        for t in 1..=4 {
            h.arrive(at(Sel::Tickets, Some(t)), (0, 0), |_| None);
        }
        let to = h.walk(true, 3, None).unwrap();
        assert_eq!(to.thread, Some(1));
        assert_eq!(h.ahead.iter().map(|p| p.thread).collect::<Vec<_>>(), vec![Some(4), Some(3), Some(2)], "forward goes 2, 3, 4");
        assert!(h.back.is_empty());
    }

    #[test]
    fn a_panel_travels_with_its_place_and_makes_none() {
        let mut h = History::default();
        let panel = |a: Option<&str>| Place { panel: a.map(Into::into), ..at(Sel::Agent("pr".into()), None) };
        h.arrive(at(Sel::Tickets, Some(12)), (0, 0), |_| None);
        h.arrive(panel(None), (0, 0), |_| None);
        assert!(!h.arrive(panel(Some("pr")), (0, 0), |_| None), "opening the panel");
        assert!(!h.arrive(panel(None), (0, 0), |_| None), "closing it");
        assert!(!h.arrive(panel(Some("pr")), (0, 0), |_| None), "opening it again");
        h.arrive(at(Sel::Files, None), (0, 0), |_| None);
        assert_eq!(h.back.len(), 2, "the thread and the chat, once");
        assert_eq!(h.walk(true, 1, None).unwrap().panel.as_deref(), Some("pr"), "the chat comes back with its panel open");
    }

    #[test]
    fn a_change_of_pane_makes_no_place() {
        let mut h = History::default();
        h.arrive(at(Sel::Tickets, Some(12)), (0, 0), |_| None);
        assert!(!h.arrive(at(Sel::Agent("orch".into()), None), (0, 1), |_| None), "a split opened");
        assert!(!h.arrive(at(Sel::Agents, None), (1, 1), |_| None), "the other pane focused");
        assert!(h.back.is_empty());
        assert!(h.arrive(at(Sel::Files, None), (1, 1), |_| None));
    }

    /// On the cloud, each card he goes into is a place, the cloud before any one too; the same card again is none; and
    /// back and forward walk them as they walk pages (paper-research N-1446 p1758).
    #[test]
    fn each_card_gone_into_on_the_cloud_is_a_place() {
        let mut h = History::default();
        let into = |f: Option<&str>| Place { focus: f.map(Into::into), ..at(cloud_page(), None) };
        h.arrive(into(None), (0, 0), |_| None);
        assert!(h.arrive(into(Some("N-1")), (0, 0), |_| None), "the first card gone into");
        assert!(!h.arrive(into(Some("N-1")), (0, 0), |_| None), "the same card: nothing");
        assert!(h.arrive(into(Some("N-1.p3")), (0, 0), |_| None), "a post in it");
        assert_eq!(h.back.iter().map(|p| p.focus.as_deref()).collect::<Vec<_>>(), vec![None, Some("N-1")]);
        assert_eq!(h.walk(true, 1, None).unwrap().focus.as_deref(), Some("N-1"));
        assert_eq!(h.walk(true, 1, None).unwrap().focus, None, "the cloud before any card");
        assert_eq!(h.walk(false, 2, None).unwrap().focus.as_deref(), Some("N-1.p3"));
    }

    #[test]
    fn the_lists_keep_the_last_places() {
        let mut h = History::default();
        for t in 0..(KEEP as u64 + 10) {
            h.arrive(at(Sel::Tickets, Some(t)), (0, 0), |_| None);
        }
        assert_eq!(h.back.len(), KEEP);
        assert_eq!(h.back.last().unwrap().thread, Some(KEEP as u64 + 8));
    }

    /// History calls a page with no thread in it by its pane's name with, after the page's name, on the Agents page the
    /// tab shown; on the Kinds page, the scale its Docket is drawn at, the agent picked and the program card open
    /// (paper-research N-1446 p2188: "Kinds · research" three times in a row read alike), before the project's name,
    /// which the narrow list cuts first. A page with none of them keeps its pane's name.
    #[test]
    fn history_names_the_agents_tab_and_the_kinds_board() {
        use crate::picture::docket::{FOCUS, SHOWN, Scale, set_scale};
        let (_held, mut rt) = window();
        let label = |rt: &rune::runtime::Runtime<App>| here(&rt.state).expect("a place").label;
        rt.state.sel = Sel::Agents;
        assert_eq!(label(&rt), "Agents · Active · research");
        rt.state.agents_tab = 1;
        assert_eq!(label(&rt), "Agents · All agents · research");
        rt.state.sel = Sel::Kinds;
        assert_eq!(label(&rt), "Kinds · Project scale · research", "as first seen");
        set_scale(&mut rt.state, Scale::Thread);
        assert_eq!(label(&rt), "Kinds · Thread scale · research");
        set_scale(&mut rt.state, Scale::Agent);
        rt.state.tickets.pic_open.insert(format!("{FOCUS}pr"));
        rt.state.tickets.pic_open.insert(format!("{SHOWN}answer"));
        assert_eq!(label(&rt), "Kinds · Agent scale · pr · answer · research");
        frames(&mut rt);
        rt.state.sel = Sel::Agents;
        frames(&mut rt);
        assert_eq!(rt.state.history.back.last().map(|p| p.label.as_str()), Some("Kinds · Agent scale · pr · answer · research"), "History's row of the Kinds page he left");
        rt.state.sel = Sel::Files;
        assert_eq!(label(&rt), "Files · research", "a page with none of them: its pane's name");
    }

    /// The window: agent pr's chat with its threads (N-7 waits on him, N-8 has subthread N-9).
    fn window() -> (std::sync::MutexGuard<'static, ()>, rune::runtime::Runtime<App>) {
        let (held, s) = made_up();
        let mut rt = crate::view_cost::window(s);
        // the window's own key routing, and this module's part of its poll (the rest reads pipes these projects lack)
        fn poll_places(s: &mut App, _: f64, _: &mut Tx) -> bool {
            poll(s)
        }
        rt.poll = Some(poll_places);
        rt.on_key = Some(crate::on_key);
        rt.frame(0.016);
        (held, rt)
    }

    /// [`window`]'s App: project research, agent pr, thread N-8 and its subthread N-9, on N-8.
    fn made_up() -> (std::sync::MutexGuard<'static, ()>, App) {
        use serde_json::json;
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.live.agents = vec![serde_json::from_value(json!({"id": "AGT-1", "name": "pr", "parent": "owner", "kind": "claude", "model": "m",
            "state": "idle", "since": "2026-11-19T08:00:00Z", "tools": [], "items": []})).unwrap()];
        let thread = |n: u64, extra: serde_json::Value| -> coord_types::TicketView {
            let mut v = json!({"id": format!("N-{n}"), "n": n, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "pr"},
                "title": format!("thread {n}"), "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1,
                "opened_at": "2026-11-19T08:00:00Z", "given": [],
                "posts": [{"n": 1, "at": "2026-11-19T07:00:00Z", "by": "owner", "text": "is it in?", "names": ["pr"], "told": ["pr"]}]});
            v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            serde_json::from_value(v).unwrap()
        };
        p.live.tickets.insert(8, thread(8, json!({})));
        p.live.tickets.insert(9, thread(9, json!({"under": "N-8", "opener": "pr", "form": {}, "posts": []})));
        s.projects = vec![p];
        s.cur = 0;
        s.sel = Sel::Tickets;
        s.tickets.shown = Some(8);
        (held, s)
    }

    /// The stills of "Your window" (paper-research N-1446 p2207): `STILL_OUT=DIR cargo test
    /// history::tests::window_states_stills -- --ignored`. On [`made_up`]'s window he goes from thread N-8 to pr's
    /// chat, opens a pane beside it, pins pr, shows the Map as at noon on the 19th, and back on the Threads page closes
    /// the pane; each a state written as `keep` writes it. Then History open over the arrows: the places he left, and
    /// under them his window's states, the one he is in left out.
    #[test]
    #[ignore]
    fn window_states_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn start() -> App {
            let (_held, mut s) = made_up();
            // as his window starts: its state as restored is the baseline, so the made-up project's own set-up is
            // no state of his
            crate::state::restore(&mut s);
            let settle = |s: &mut App| {
                poll(s);
                crate::state::write_now(s);
            };
            settle(&mut s);
            s.sel = Sel::Agent("pr".into());
            settle(&mut s);
            s.panes.push((0, Sel::Tickets));
            settle(&mut s);
            s.pins.toggle(crate::sidebar::Pin { project: "research".into(), id: "AGT-1".into(), agent: "pr".into(), thread: false, lens: None });
            settle(&mut s);
            s.cloud.at = docket_machine::machine::secs("2026-11-19T10:59:59Z");
            settle(&mut s);
            (s.sel, s.tickets.shown) = (Sel::Tickets, Some(9));
            s.panes.clear();
            settle(&mut s);
            crate::state::read_deltas(&mut s);
            s.history.open = true;
            s
        }
        let args = vec!["--snap".to_string(), out.join("history-your-window.png").display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), start, crate::view) }).args(args).try_run().unwrap();
    }

    fn frames(rt: &mut rune::runtime::Runtime<App>) {
        for _ in 0..3 {
            rt.frame(0.016);
        }
    }

    /// Thread N-8, then pr's chat (as a reference pressed in a message opens it), then the mouse's back button over
    /// the page: N-8 again; its forward button: the chat; Alt+Left with nothing focused: N-8; the arrows are drawn,
    /// and a press on History's mark opens it.
    #[test]
    fn the_mouse_buttons_and_alt_arrows_go_back_and_forward_in_the_window() {
        let (_held, mut rt) = window();
        assert!(rt.find("history-arrows").is_some(), "the arrows on the bar");
        rt.state.sel = Sel::Agent("pr".into());
        frames(&mut rt);
        assert_eq!(rt.state.history.back.len(), 1, "the thread he left is a place");
        assert_eq!((rt.state.history.back[0].label.as_str(), rt.state.history.back[0].id.as_str()), ("thread 8", "N-8"), "its reference: label, id; an unlabelled thread by its title, never its kind (0.2 C16)");
        let (x, y) = rt.find("main").expect("the page is drawn");
        rt.button_down(x, y, rune::PointerButton::Back);
        rt.button_up(x, y, rune::PointerButton::Back);
        frames(&mut rt);
        assert_eq!((&rt.state.sel, rt.state.tickets.shown), (&Sel::Tickets, Some(8)), "the mouse's back button");
        rt.button_down(x, y, rune::PointerButton::Forward);
        rt.button_up(x, y, rune::PointerButton::Forward);
        frames(&mut rt);
        assert_eq!(rt.state.sel, Sel::Agent("pr".into()), "its forward button");
        rt.key(KeyEvent { key: rune::Key::Left, mods: rune::Mods { alt: true, ..Default::default() } });
        frames(&mut rt);
        assert_eq!(rt.state.sel, Sel::Tickets, "Alt+Left");
        assert_eq!(rt.state.history.ahead.len(), 1);
        rt.state.sel = Sel::Agent("pr".into());
        frames(&mut rt);
        let (mx, my) = rt.find("history-open").unwrap();
        rt.pointer_move(mx, my);
        rt.frame(0.016);
        rt.pointer_down(mx, my);
        rt.frame(0.016);
        rt.pointer_up(mx, my);
        frames(&mut rt);
        assert!(rt.state.history.open, "History opens by a press on its mark");
    }

    /// paper-research N-1446 p2207 (his N-687 p8): his window's states, and going back to one. On thread N-8 he opens
    /// a pane on pr's chat, pins pr, closes the pane and shows the Map as at noon on the 19th; then types in pr's box.
    /// Each change is a line of ui-deltas.jsonl, but his typing. History's "Your window" lists them newest first, the
    /// one he is in left out; a press on the pane's puts his window back as it was then: the pane, pr not pinned, the
    /// Map now, and his words still in the box. Going back is a line of its own, and the pin's state is still there to
    /// go to.
    #[test]
    fn his_window_goes_back_to_an_earlier_state_and_keeps_what_he_typed() {
        let (_held, mut rt) = window();
        let lines = || -> Vec<serde_json::Value> { std::fs::read_to_string(crate::state::deltas_path()).unwrap_or_default().lines().map(|l| serde_json::from_str(l).unwrap()).collect() };
        let said = |v: &serde_json::Value| v["said"].as_str().unwrap_or_default().to_string();
        // each change settled and written, as `keep` writes it (a test's window writes nothing by itself)
        let settle = |rt: &mut rune::runtime::Runtime<App>| {
            frames(rt);
            crate::state::write_now(&mut rt.state);
        };
        // the run's first line, his window whole as it started, and what the test's window changed since
        settle(&mut rt);
        let first = lines();
        assert!(first[0]["start"] == true && said(&first[0]) == "opened" && first[0]["now"].get("panes").is_some(), "the run's first line: his window whole: {first:?}");
        let base = first.len();
        rt.state.panes.push((0, Sel::Agent("pr".into())));
        settle(&mut rt);
        rt.state.pins.toggle(crate::sidebar::Pin { project: "research".into(), id: "AGT-1".into(), agent: "pr".into(), thread: false, lens: None });
        settle(&mut rt);
        rt.state.panes.clear();
        settle(&mut rt);
        let noon = docket_machine::machine::secs("2026-11-19T10:59:59Z");
        rt.state.cloud.at = noon;
        settle(&mut rt);
        rt.state.composers.insert((0, "pr".into()), rune::TextEdit::new("is it in yet?".to_string()));
        settle(&mut rt);
        let map = format!("thread 8 · the Map at {}", crate::when::secs(noon.unwrap() + 1));
        assert_eq!(lines()[base..].iter().map(said).collect::<Vec<_>>(), vec!["thread 8 · 2 panes".to_string(), "thread 8 · pinned pr".into(), "thread 8 · one pane".into(), map.clone()], "a line for each change, none for his typing");
        assert!(lines().iter().all(|l| l["now"].get("composers").is_none()), "what he typed is in none");
        press(&mut rt, "history-open");
        let texts: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect();
        let at = |t: &str| texts.iter().position(|x| x == t);
        let order: Vec<Option<usize>> = ["Your window", "thread 8 · one pane", "thread 8 · pinned pr", "thread 8 · 2 panes", "opened"].into_iter().map(at).collect();
        assert!(order.iter().all(Option::is_some) && order.windows(2).all(|w| w[0] < w[1]), "Your window, newest first: {texts:?}");
        assert_eq!(at(&map), None, "the state he is in is not listed");
        let go_to = |rt: &mut rune::runtime::Runtime<App>, said: &str| {
            let r = rt.elements().into_iter().find(|e| !e.exiting && e.text.as_deref() == Some(said)).map(|e| e.rect).unwrap_or_else(|| panic!("{said} listed"));
            let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
            rt.pointer_move(x, y);
            rt.frame(0.016);
            rt.pointer_down(x, y);
            rt.frame(0.016);
            rt.pointer_up(x, y);
            frames(rt);
        };
        go_to(&mut rt, "thread 8 · 2 panes");
        assert_eq!((&rt.state.panes, rt.state.pins.list.len(), rt.state.cloud.at), (&vec![(0, Sel::Agent("pr".into()))], 0, None), "his window as then: the pane, pr not pinned, the Map now");
        assert_eq!(rt.state.composers.get(&(0, "pr".into())).map(|t| t.text.as_str()), Some("is it in yet?"), "his words still in the box");
        let l = lines();
        let back = l.last().unwrap();
        assert_eq!(l.len(), base + 5, "going back is a line of its own");
        assert!(said(back).starts_with("back to ") && ["2 panes", "unpinned pr", "the Map now"].iter().all(|w| said(back).contains(w)), "its words: {}", said(back));
        assert_eq!((&back["was"]["pins"][0]["agent"], &back["was"]["map_at"], &back["now"]["map_at"]), (&serde_json::json!("pr"), &serde_json::json!(noon), &serde_json::Value::Null), "what it undid, whole");
        press(&mut rt, "history-open");
        go_to(&mut rt, "thread 8 · pinned pr");
        assert_eq!((rt.state.pins.list.iter().map(|p| p.agent.as_str()).collect::<Vec<_>>(), rt.state.panes.len()), (vec!["pr"], 1), "nothing lost: the pin's state, gone forward to");
    }

    /// On pr's chat from thread N-8, his panel opened and closed: one press of Back is the thread again, and History
    /// holds pr once.
    #[test]
    fn opening_and_closing_a_panel_on_a_page_makes_no_place() {
        let (_held, mut rt) = window();
        rt.state.sel = Sel::Agent("pr".into());
        frames(&mut rt);
        let cur = rt.state.cur;
        rt.state.circle.open = Some((cur, "pr".into()));
        frames(&mut rt);
        rt.state.circle.open = None;
        frames(&mut rt);
        assert_eq!(rt.state.history.back.len(), 1, "only the thread he left");
        let (bx, by) = rt.find("history-back").unwrap();
        rt.pointer_move(bx, by);
        rt.frame(0.016);
        rt.pointer_down(bx, by);
        rt.frame(0.016);
        rt.pointer_up(bx, by);
        frames(&mut rt);
        assert_eq!((&rt.state.sel, rt.state.tickets.shown), (&Sel::Tickets, Some(8)), "one press of Back");
        let pr = |l: &[Place]| l.iter().filter(|p| p.sel == Sel::Agent("pr".into())).count();
        assert_eq!(pr(&rt.state.history.back) + pr(&rt.state.history.ahead), 1, "pr once");
    }

    /// The bar's "‹ Agents" is Back and makes no place (the walk's 3, paper-research N-1446 p2101, p2120). On thread
    /// N-8, then the Agents page, a press on pr's tile opens pr's chat with "‹ Agents" on the bar; a press on it is the
    /// Agents page, and Back after it is N-8, not pr's chat again. Forward twice is pr's chat, with "‹ Agents" again,
    /// and the Agents page is in the lists once. Reached from N-8, pr's chat has no "‹ Agents".
    #[test]
    fn the_bars_way_back_to_the_agents_page_is_back_and_makes_no_place() {
        let (_held, mut rt) = window();
        let agents_back = |rt: &rune::runtime::Runtime<App>| rt.elements().into_iter().find(|e| !e.exiting && e.text.as_deref() == Some("‹ Agents")).map(|e| e.rect);
        rt.state.sel = Sel::Agents;
        frames(&mut rt);
        press(&mut rt, "tile-pr");
        assert_eq!(rt.state.sel, Sel::Agent("pr".into()), "a press on pr's tile: pr's chat");
        let r = agents_back(&rt).expect("‹ Agents on pr's chat opened from the Agents page");
        press_at(&mut rt, r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        assert_eq!(rt.state.sel, Sel::Agents, "‹ Agents");
        press(&mut rt, "history-back");
        assert_eq!((&rt.state.sel, rt.state.tickets.shown), (&Sel::Tickets, Some(8)), "Back after ‹ Agents: the thread before the Agents page, not pr's chat");
        press(&mut rt, "history-forward");
        press(&mut rt, "history-forward");
        assert_eq!(rt.state.sel, Sel::Agent("pr".into()), "Forward twice: pr's chat");
        assert!(agents_back(&rt).is_some(), "‹ Agents again: one Back is the Agents page");
        let agents = |l: &[Place]| l.iter().filter(|p| p.sel == Sel::Agents).count();
        assert_eq!(agents(&rt.state.history.back) + agents(&rt.state.history.ahead), 1, "the Agents page once in the lists");
        rt.state.sel = Sel::Tickets;
        frames(&mut rt);
        rt.state.select_agent("pr");
        frames(&mut rt);
        assert!(agents_back(&rt).is_none(), "no ‹ Agents on pr's chat reached from N-8: Back is N-8");
    }

    /// Kinds' "‹ the thread" is Back and makes no place (the walk's 6, paper-research N-1446 p2101, p2120). From thread
    /// N-8 the bar's Docket opens Kinds at N-8's program; "‹ the thread" is N-8 with Kinds ahead, so Back after it goes
    /// on to the page before N-8, not to Kinds again.
    #[test]
    fn kinds_way_back_to_the_thread_is_back_and_makes_no_place() {
        let (_held, mut rt) = window();
        rt.state.sel = Sel::Agent("pr".into());
        frames(&mut rt);
        rt.state.sel = Sel::Tickets;
        frames(&mut rt);
        press(&mut rt, "docket-mark");
        assert_eq!(rt.state.sel, Sel::Kinds, "the bar's Docket: Kinds, at N-8's program");
        press(&mut rt, "docket-back");
        assert_eq!((&rt.state.sel, rt.state.tickets.shown), (&Sel::Tickets, Some(8)), "‹ the thread");
        assert_eq!(rt.state.history.ahead.last().map(|p| &p.sel), Some(&Sel::Kinds), "Kinds ahead: ‹ the thread was Back");
        press(&mut rt, "history-back");
        assert_eq!(rt.state.sel, Sel::Agent("pr".into()), "Back after ‹ the thread: pr's chat, before N-8, not Kinds again");
    }

    fn press_at(rt: &mut rune::runtime::Runtime<App>, x: f32, y: f32) {
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        frames(rt);
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

    /// Find from a page with no list of threads, the Agents page (N-925), the sidebar folded away (open, the ⌕ is its
    /// field's, find.rs): the ⌕ opens the Threads page's box over it; Esc closes it and he stays; the ⌕ opens it again,
    /// a hit opens its thread at that post, and Back returns him there.
    #[test]
    fn find_opens_over_any_page_and_a_hit_is_a_place_back_returns_from() {
        let (_held, mut rt) = window();
        rt.state.sidebar.set_open(false);
        rt.state.sel = Sel::Agents;
        frames(&mut rt);
        press(&mut rt, "find-mark");
        assert!(rt.state.tickets.find.over && rt.find("find-over").is_some(), "the ⌕: the box over the page");
        assert!(rt.find(crate::find::INPUT).is_some(), "the Threads page's own box");
        rt.key(KeyEvent { key: rune::Key::Escape, mods: Default::default() });
        frames(&mut rt);
        assert!(!rt.state.tickets.find.over && rt.find("find-over").is_none(), "Esc closes it");
        assert_eq!(rt.state.sel, Sel::Agents, "and leaves him where he was");
        let hit: coord_types::FindHit = serde_json::from_value(serde_json::json!({"id": "N-8", "t": 8, "post": 1, "title": "thread 8",
            "by": "owner", "at": "2026-11-19T07:00:00Z", "words": "is it in?"})).unwrap();
        rt.state.tickets.find.shown = Some(crate::find::Shown::Hits { q: "in".into(), hits: Ok(vec![hit]) });
        press(&mut rt, "find-mark");
        assert!(rt.find("find-over").is_some() && rt.find("hit-N-8").is_some(), "the ⌕: the box and the same results");
        press(&mut rt, "hit-N-8");
        assert_eq!((&rt.state.sel, rt.state.tickets.shown, rt.state.tickets.find.found), (&Sel::Tickets, Some(8), Some((8, 1))), "the thread at that post");
        assert!(!rt.state.tickets.find.over, "the box closed");
        go(&mut rt.state, true, 1);
        assert_eq!(rt.state.sel, Sel::Agents, "Back: the page he searched from");
    }

    /// The cloud of research's made-up answer (cloud.rs's `base`) drawn on the Map's page, nothing gone into.
    fn on_the_cloud() -> (std::sync::MutexGuard<'static, ()>, App) {
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        s.projects = vec![p];
        s.cur = 0;
        s.sel = cloud_page();
        s.cloud.of = 0;
        let (g, _) = crate::map::cloud::cloud(&crate::map::cloud::tests::base(), None, "project:research", "research", &mut Default::default()).expect("the cloud");
        s.map.set_cloud(g);
        (held, s)
    }

    /// On the cloud, threads N-1 and N-3 gone into as a click goes in (canvas.rs `fly_to_id`): the poll makes each a
    /// place, labelled as its card; Back makes N-1 the focus again and flies into it; Back again, the cloud before any,
    /// flown out to the project; Forward twice, N-3; and arriving where it went makes no place.
    #[test]
    fn back_and_forward_on_the_cloud_walk_the_cards_he_went_into() {
        let (_held, mut s) = on_the_cloud();
        assert!(!poll(&mut s), "the cloud is only recorded");
        for id in ["N-1", "N-3"] {
            assert!(crate::map::canvas::fly_to_id(&mut s, 0, id), "{id} drawn");
            assert_eq!(s.cloud.focus.as_deref(), Some(id));
            assert!(poll(&mut s), "{id}: a place");
        }
        let n1 = s.history.back.last().unwrap().clone();
        assert_eq!((n1.focus.as_deref(), n1.id.as_str()), (Some("N-1"), "N-1"));
        assert_eq!(Some(n1.label), card_label(&s, 0, "N-1"), "History shows it as its card");
        assert_eq!(s.history.back[0].focus, None, "the cloud before any");
        // the flight under way landed, so a flight after each step is that step's
        let land = |s: &mut App| {
            let r = s.map.graph().and_then(|g| g.project("project:research").map(|i| g.node(i).rect)).unwrap();
            s.map.fly_to(&r, true);
            assert!(!s.map.flying());
        };
        land(&mut s);
        go(&mut s, true, 1);
        assert_eq!(s.cloud.focus.as_deref(), Some("N-1"), "Back: N-1 the focus");
        assert!(s.map.flying(), "flown into");
        assert!(!poll(&mut s), "arriving where it went makes no place");
        land(&mut s);
        go(&mut s, true, 1);
        assert_eq!(s.cloud.focus, None, "Back again: the cloud before any");
        assert!(s.map.flying() && s.map.selected().is_none(), "flown out, nothing selected");
        go(&mut s, false, 2);
        assert_eq!(s.cloud.focus.as_deref(), Some("N-3"), "Forward twice");
        assert!(!poll(&mut s));
        assert_eq!((s.history.back.len(), s.history.ahead.len()), (2, 0));
    }

    /// The Agents page's tab and the Kinds page's Docket are places (paper-research N-1446 p2120, the walk's 4 and 5).
    /// On the Agents page, All agents (as the "stopped: …" line opens it) then Back is Active again, and Forward All
    /// agents. Kinds as first seen and a press on Project are one place. At the Thread scale, then the Project scale,
    /// then a program's card open under the board, then what a count opened: Back walks them back one at a time to the
    /// Thread scale, and Forward walks them on again.
    #[test]
    fn the_agents_tab_and_the_kinds_board_are_places_back_returns_to() {
        use crate::picture::docket::{PANEL, SHOWN, Scale, scale, set_scale, show_panel, shown_kind};
        let (_held, mut rt) = window();
        rt.state.sel = Sel::Agents;
        frames(&mut rt);
        rt.state.agents_tab = 1;
        frames(&mut rt);
        go(&mut rt.state, true, 1);
        assert_eq!((&rt.state.sel, rt.state.agents_tab), (&Sel::Agents, 0), "Back: Active again");
        go(&mut rt.state, false, 1);
        assert_eq!((&rt.state.sel, rt.state.agents_tab), (&Sel::Agents, 1), "Forward: All agents again");
        rt.state.sel = Sel::Kinds;
        frames(&mut rt);
        let n = rt.state.history.back.len();
        set_scale(&mut rt.state, Scale::Project);
        frames(&mut rt);
        assert_eq!(rt.state.history.back.len(), n, "Project pressed on Kinds as first seen: the same place");
        set_scale(&mut rt.state, Scale::Thread);
        frames(&mut rt);
        set_scale(&mut rt.state, Scale::Project);
        frames(&mut rt);
        rt.state.tickets.pic_open.insert(format!("{SHOWN}answer"));
        frames(&mut rt);
        show_panel(&mut rt.state, "count:answer:");
        frames(&mut rt);
        let board = |rt: &rune::runtime::Runtime<App>| {
            let open = &rt.state.tickets.pic_open;
            (scale(open), shown_kind(open).map(String::from), open.iter().any(|k| k.starts_with(PANEL)))
        };
        assert_eq!(board(&rt), (Scale::Project, Some("answer".into()), true), "the board as he made it");
        go(&mut rt.state, true, 1);
        assert_eq!(board(&rt), (Scale::Project, Some("answer".into()), false), "Back: what the count opened is put away");
        go(&mut rt.state, true, 1);
        assert_eq!(board(&rt), (Scale::Project, None, false), "Back: the card is put away");
        go(&mut rt.state, true, 1);
        assert_eq!((&rt.state.sel, board(&rt)), (&Sel::Kinds, (Scale::Thread, None, false)), "Back: the Thread scale he came from");
        go(&mut rt.state, false, 3);
        assert_eq!(board(&rt), (Scale::Project, Some("answer".into()), true), "Forward: all three again");
        assert!(rt.state.kinds_open.contains("answer"), "the card open on the Kinds page too");
    }

    /// Back to a thread hands its row to read_place.rs, which brings the view back to it as after a swap.
    #[test]
    fn going_back_hands_the_row_to_read_place() {
        let (_held, mut rt) = window();
        let view = crate::read_place::thread_view("research", "N-8");
        rt.state.read_places.kept.insert(view.clone(), "N-8-p1".into());
        rt.state.sel = Sel::Agent("pr".into());
        frames(&mut rt);
        assert_eq!(rt.state.history.back[0].top.as_deref(), Some("N-8-p1"));
        rt.state.read_places.kept.insert(view.clone(), "elsewhere".into());
        go(&mut rt.state, true, 1);
        assert_eq!(rt.state.read_places.kept.get(&view).map(String::as_str), Some("N-8-p1"), "the row he left, to be brought back");
    }

    /// A picture open over the page is closed by Back, and the bar stays in reach (the walk's 7, paper-research N-1446
    /// p2101, p2120). On pr's chat from N-8, with a picture of its open: a press on the bar's Back reaches it and closes
    /// the picture, pr's chat still shown; so do Alt+Left and the mouse's back button over the picture. Back with no
    /// picture open is N-8. With a picture open over N-8, Forward goes on to pr's chat and the picture closes with N-8.
    /// First, on N-8 with no place behind it, Back is lit while a picture is open, and its press closes it.
    #[test]
    fn back_closes_a_picture_open_over_the_page_and_the_bar_stays_in_reach() {
        let (_held, mut rt) = window();
        let open = |rt: &mut rune::runtime::Runtime<App>| {
            rt.update(|s, _| s.viewing = Some("pr|1".into()));
            rt.invalidate();
            frames(rt);
            assert!(rt.find("image-viewer").is_some(), "the picture open");
        };
        let shown = |rt: &rune::runtime::Runtime<App>| (rt.state.viewing.clone(), rt.state.sel.clone());
        assert!(rt.state.history.back.is_empty(), "nothing behind N-8");
        open(&mut rt);
        press(&mut rt, "history-back");
        assert_eq!(shown(&rt), (None, Sel::Tickets), "with nothing behind, the press on Back still closes the picture");
        rt.state.sel = Sel::Agent("pr".into());
        frames(&mut rt);
        let pr = (None, Sel::Agent("pr".into()));
        open(&mut rt);
        press(&mut rt, "history-back");
        assert_eq!(shown(&rt), pr, "a press on the bar's Back reaches it, closes the picture and stays on pr's chat");
        open(&mut rt);
        rt.key(KeyEvent { key: rune::Key::Left, mods: rune::Mods { alt: true, ..Default::default() } });
        frames(&mut rt);
        assert_eq!(shown(&rt), pr, "Alt+Left closes it and stays");
        open(&mut rt);
        let (x, y) = rt.find("image-viewer-area").expect("the picture's area");
        rt.button_down(x, y, rune::PointerButton::Back);
        rt.button_up(x, y, rune::PointerButton::Back);
        frames(&mut rt);
        assert_eq!(shown(&rt), pr, "the mouse's back button over it closes it and stays");
        press(&mut rt, "history-back");
        assert_eq!((&rt.state.sel, rt.state.tickets.shown), (&Sel::Tickets, Some(8)), "Back with no picture open: N-8");
        open(&mut rt);
        press(&mut rt, "history-forward");
        assert_eq!(shown(&rt), pr, "Forward goes on to pr's chat, and the picture closes with N-8");
    }
}
