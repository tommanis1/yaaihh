//! Tabs and windows (owner, 10 Nov): two menus that open when the pointer rests at an edge of the window. The top
//! edge opens the **tabs**: whole workspaces, side by side. The right edge opens the current tab's **windows**: each
//! window is a layout of its own (a page, or the split of up to four panes, panes.rs), a quick way between views of
//! what the agents do. The window shown is the app's own state (`cur`, `sel`, `panes`, `focus`); the others are kept
//! here as [`Layout`]s, and switching saves the one shown into its slot first.
//!
//! A menu opens after the pointer rests [`DWELL`] on its edge strip (the top one [`EDGE_TOP`] points high, the side one
//! [`EDGE`] wide) and closes when the pointer leaves its band. Each tab or window is a small circle with its number,
//! centred on its edge and growing outward as more come (owner 10 Nov): the tabs in a row at the top centre, + (a new
//! one) at its right; the windows in a column at the middle of the right edge, + at its top. No click switches: the
//! pointer on a circle switches to it, and the wheel steps through them, so moving along them shows each in turn.
//! No card names them (owner m530, m539): the circles alone, and × after + closes the tab or window shown (one is
//! always left).

use crate::{App, Sel};
use rune::{El, Tx, WheelEvent, col, el, row, text};
use rune_kit::Kit;
use std::time::{Duration, Instant};

/// How long the pointer rests at an edge before its menu opens.
const DWELL: Duration = Duration::from_millis(250);
/// The width of the side edge strip that opens the windows.
const EDGE: f32 = 4.0;
/// The height of the top edge strip that opens the tabs: 85% of the open menu's band (owner m536), so the pointer
/// need not reach the very edge; it takes no presses, so the toolbar under it still works.
const EDGE_TOP: f32 = 0.85 * (2.0 * BAND_PAD + DOT + 2.0 * PILL_PAD);
/// The open tabs menu's band: its padding around the pill, and the pill's around the circles.
const BAND_PAD: f32 = 8.0;
const PILL_PAD: f32 = 6.0;
/// A tab's or window's circle.
const DOT: f32 = 36.0;
/// The wheel's points per step from one circle to the next.
const STEP: f32 = 24.0;

/// What one window shows: the focused project and page, the split's panes and which one has focus (as `App` holds them).
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub cur: usize,
    pub sel: Sel,
    pub panes: Vec<(usize, Sel)>,
    pub focus: usize,
}

/// A tab: its windows and the one shown.
#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    pub windows: Vec<Layout>,
    pub cur: usize,
}

/// The edge whose menu is meant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edge {
    Top,
    Side,
}

/// The tabs, the one shown, and the menus' state.
#[derive(Default)]
pub struct Tabs {
    /// Empty until the first menu or switch: then one tab with the window shown.
    pub tabs: Vec<Tab>,
    pub cur: usize,
    /// The edge strip the pointer rests on, since when.
    pub resting: Option<(Edge, Instant)>,
    /// The menu open.
    pub open: Option<Edge>,
    /// The wheel's points not yet a step ([`STEP`]).
    pub wheel: f32,
}

impl App {
    /// The window shown, as a [`Layout`].
    pub fn layout(&self) -> Layout {
        Layout { cur: self.cur, sel: self.sel_kept(), panes: self.panes.clone(), focus: self.focus }
    }

    /// Show layout L: its project and page, its panes; every agent it shows reads its session.
    fn show_layout(&mut self, l: Layout) {
        self.cur = l.cur.min(self.projects.len().saturating_sub(1));
        self.panes = l.panes;
        self.focus = if self.panes.is_empty() { 0 } else { l.focus.min(self.panes.len() - 1) };
        match l.sel {
            Sel::Agent(name) => self.select_agent(&name),
            other => self.show_sel(other),
        }
        for (i, sel) in self.panes.clone() {
            if let (Sel::Agent(name), Some(p)) = (sel, self.projects.get_mut(i)) {
                p.open_conv(&name);
            }
        }
    }

    /// One tab holding the window shown, when there is none; the window shown saved into its slot.
    fn keep_shown(&mut self) {
        let l = self.layout();
        let t = &mut self.tabs;
        if t.tabs.is_empty() {
            t.tabs.push(Tab { windows: vec![l], cur: 0 });
            t.cur = 0;
            return;
        }
        let tab = &mut t.tabs[t.cur];
        tab.windows[tab.cur] = l;
    }

    /// A copy of the page shown, without the split: what + adds.
    fn fresh(&self) -> Layout {
        Layout { cur: self.cur, sel: self.sel_kept(), panes: Vec::new(), focus: 0 }
    }

    pub fn switch_tab(&mut self, t: usize) {
        self.keep_shown();
        if t >= self.tabs.tabs.len() || t == self.tabs.cur {
            return;
        }
        self.tabs.cur = t;
        let tab = &self.tabs.tabs[t];
        self.show_layout(tab.windows[tab.cur].clone());
    }

    pub fn switch_window(&mut self, w: usize) {
        self.keep_shown();
        let tab = &mut self.tabs.tabs[self.tabs.cur];
        if w >= tab.windows.len() || w == tab.cur {
            return;
        }
        tab.cur = w;
        let l = tab.windows[w].clone();
        self.show_layout(l);
    }

    pub fn new_tab(&mut self) {
        self.keep_shown();
        let l = self.fresh();
        self.tabs.tabs.push(Tab { windows: vec![l.clone()], cur: 0 });
        self.tabs.cur = self.tabs.tabs.len() - 1;
        self.show_layout(l);
    }

    pub fn new_window(&mut self) {
        self.keep_shown();
        let l = self.fresh();
        let tab = &mut self.tabs.tabs[self.tabs.cur];
        tab.windows.push(l.clone());
        tab.cur = tab.windows.len() - 1;
        self.show_layout(l);
    }

    pub fn close_tab(&mut self, t: usize) {
        self.keep_shown();
        let tabs = &mut self.tabs;
        if tabs.tabs.len() < 2 || t >= tabs.tabs.len() {
            return;
        }
        tabs.tabs.remove(t);
        if t < tabs.cur || tabs.cur >= tabs.tabs.len() {
            tabs.cur = tabs.cur.saturating_sub(1);
        }
        if t == tabs.cur || t == tabs.cur + 1 {
            let tab = &tabs.tabs[tabs.cur];
            let l = tab.windows[tab.cur].clone();
            self.show_layout(l);
        }
    }

    pub fn close_window(&mut self, w: usize) {
        self.keep_shown();
        let tab = &mut self.tabs.tabs[self.tabs.cur];
        if tab.windows.len() < 2 || w >= tab.windows.len() {
            return;
        }
        let shown = tab.cur == w;
        tab.windows.remove(w);
        if w < tab.cur || tab.cur >= tab.windows.len() {
            tab.cur = tab.cur.saturating_sub(1);
        }
        if shown {
            let l = tab.windows[tab.cur].clone();
            self.show_layout(l);
        }
    }
}

/// Each poll: a pointer that has rested on an edge long enough opens its menu.
pub fn poll(s: &mut App) -> bool {
    match s.tabs.resting {
        Some((edge, at)) if s.tabs.open.is_none() && at.elapsed() >= DWELL => {
            s.keep_shown();
            s.tabs.open = Some(edge);
            true
        }
        _ => false,
    }
}

/// The tabs and windows as the menus show them: the saved ones, with the one shown as it is now.
pub(crate) fn current(s: &App) -> (Vec<Tab>, usize) {
    let mut tabs = s.tabs.tabs.clone();
    if tabs.is_empty() {
        return (vec![Tab { windows: vec![s.layout()], cur: 0 }], 0);
    }
    let t = s.tabs.cur.min(tabs.len() - 1);
    let w = tabs[t].cur;
    tabs[t].windows[w] = s.layout();
    (tabs, t)
}

/// A tab's or window's circle: its number, filled when it is the one shown; the pointer on it runs GO (switches).
fn dot(key: String, n: usize, shown: bool, k: &Kit, go: impl Fn(&mut App) + 'static) -> El {
    el().key(key)
        .w(DOT)
        .h(DOT)
        .radius(DOT / 2.0)
        .center()
        .fill(if shown { k.c.text_accent } else { k.c.surface_raised })
        .border(1.0, if shown { k.c.border_focus } else { k.c.border_weak })
        .child(text(n.to_string()).font(k.ty.ui_medium).color(if shown { k.c.bg_base } else { k.c.text_muted }))
        .on_hover_change(move |s: &mut App, inside: bool, _: &mut Tx| {
            if inside {
                go(s)
            }
        })
}

/// A circle that wants a click: + (a new tab or window) or × (close the tab shown); a click runs ACT.
fn plus(key: &str, mark: &str, k: &Kit, act: impl Fn(&mut App) + 'static) -> El {
    el().key(key.to_string())
        .w(DOT)
        .h(DOT)
        .radius(DOT / 2.0)
        .center()
        .border(1.0, k.c.border_weak)
        .cursor(rune::Cursor::Pointer)
        .child(text(mark.to_string()).font(k.ty.ui_medium).color(k.c.text_muted))
        .on_press(move |s: &mut App| act(s))
}

/// The wheel over EDGE's menu: one step to the next or the previous tab or window per [`STEP`] points (down or right:
/// the next).
fn wheel(edge: Edge) -> impl Fn(&mut App, WheelEvent, &mut Tx) -> bool {
    move |s: &mut App, e: WheelEvent, _: &mut Tx| {
        s.tabs.wheel += if edge == Edge::Top { e.dx + e.dy } else { e.dy };
        if s.tabs.wheel.abs() < STEP {
            return true;
        }
        let d: isize = if s.tabs.wheel < 0.0 { 1 } else { -1 };
        s.tabs.wheel = 0.0;
        let (tabs, t) = current(s);
        let (cur, n) = if edge == Edge::Top { (t, tabs.len()) } else { (tabs[t].cur, tabs[t].windows.len()) };
        let to = cur.saturating_add_signed(d).min(n - 1);
        if to != cur {
            if edge == Edge::Top { s.switch_tab(to) } else { s.switch_window(to) }
        }
        true
    }
}

/// The edge strips, and the menu open, into LAYER.
pub fn layer(s: &App, k: &Kit, layer: &mut Vec<El>) {
    let strip = |edge: Edge| {
        move |s: &mut App, inside: bool, tx: &mut Tx| {
            if inside {
                s.tabs.resting = Some((edge, Instant::now()));
                tx.wake_after(DWELL);
            } else if s.tabs.resting.is_some_and(|(e, _)| e == edge) {
                s.tabs.resting = None;
            }
        }
    };
    if s.tabs.open.is_none() {
        layer.push(el().key("edge-top").pin(Some(0.0), Some(0.0), Some(0.0), None).h(EDGE_TOP).on_hover_change(strip(Edge::Top)));
        layer.push(el().key("edge-side").pin(None, Some(0.0), Some(0.0), Some(0.0)).w(EDGE).on_hover_change(strip(Edge::Side)));
        return;
    }
    let (tabs, t) = current(s);
    let leave = |s: &mut App, inside: bool, _: &mut Tx| {
        if !inside {
            s.tabs.open = None;
            s.tabs.resting = None;
            s.tabs.wheel = 0.0;
        }
    };
    let pill = |c: El| c.gap(8.0).padding(PILL_PAD).radius(DOT).fill(k.c.bg_base).border(1.0, k.c.border_weak);
    match s.tabs.open {
        Some(Edge::Top) => {
            let mut dots: Vec<El> = (0..tabs.len()).map(|i| dot(format!("tab-{i}"), i + 1, i == t, k, move |s: &mut App| s.switch_tab(i))).collect();
            dots.push(plus("tab-new", "+", k, |s: &mut App| s.new_tab()));
            if tabs.len() > 1 {
                dots.push(plus("tab-close", "×", k, move |s: &mut App| s.close_tab(t)));
            }
            // the band: the whole width, the circles centred in it; leaving it closes the menu (no card: owner 10 Nov)
            layer.push(
                col()
                    .key("tabs-menu")
                    .pin(Some(0.0), Some(0.0), Some(0.0), None)
                    .padding(BAND_PAD)
                    .items_center()
                    .on_hover_change(leave)
                    .on_wheel(wheel(Edge::Top))
                    .child(pill(row().center_y()).children(dots)),
            );
        }
        Some(Edge::Side) => {
            let tab = &tabs[t];
            let n = tab.windows.len();
            let mut dots = vec![plus("window-new", "+", k, |s: &mut App| s.new_window())];
            dots.extend((0..n).map(|i| dot(format!("window-{i}"), i + 1, i == tab.cur, k, move |s: &mut App| s.switch_window(i))));
            if n > 1 {
                let w = tab.cur;
                dots.push(plus("window-close", "×", k, move |s: &mut App| s.close_window(w)));
            }
            // the band: the whole height at the right, the circles at its middle (no card: owner m539)
            layer.push(
                col()
                    .key("windows-menu")
                    .pin(None, Some(0.0), Some(0.0), Some(0.0))
                    .padding(BAND_PAD)
                    .justify_center()
                    .on_hover_change(leave)
                    .on_wheel(wheel(Edge::Side))
                    .child(pill(col().items_center()).children(dots)),
            );
        }
        None => {}
    }
}
