//! The split: up to four panes side by side (two or
//! three in a row, four as two rows of two), each showing a page of a project; one has focus. The focused pane is the
//! app's `cur` and `sel`, so everything that acts on "the page shown" acts on it; the others are kept in
//! `App::panes` (the focused one's slot there is refreshed whenever focus moves). A pane on an agent shows its chat
//! or terminal and its own composer, whatever project it is from (chat.rs); a pane on another page names the page it
//! holds and shows it once its header is clicked (the pages read the app's current project and page).
//!
//! Split (the toolbar's button, or a double click on an agent in the sidebar: "open to the side") adds a pane; any
//! click inside a pane focuses it (before the click does what it does there), and its header's × closes it.
//!
//! **Drag and drop**: an agent dragged from the sidebar (a pin or a running agent) onto
//! the main area shows where it would land: over a pane's left or right third, a new pane on that side of it; over
//! its middle (or anywhere once the split is full), in that pane. The drop does it; a drop over the sidebar does
//! nothing (a pin dropped there still reorders the pins). The split is panes in a row (four as two rows of two), so
//! there is no above/below.

use crate::{App, Sel, agents, ask, ext, views};
use rune::{Drag, El, Phase, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit};

/// The most panes the split shows.
pub const MAX: usize = 4;

/// An agent being dragged from the sidebar: agent NAME of project PI, the pointer AT (window points).
#[derive(Clone, Debug)]
pub struct AgentDrag {
    pub pi: usize,
    pub name: String,
    pub at: [f32; 2],
}

/// Where a dragged agent lands on a pane: a new pane before it or after it, or in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zone {
    Left,
    Right,
    Center,
}

impl App {
    /// A sidebar row's drag of agent NAME of project PI: followed while it moves; on release, dropped where the view
    /// showed it would land ([`drop_zone`]). Whether the drop was taken (a pin then keeps its place in the list).
    pub fn drag_agent(&mut self, pi: usize, name: &str, d: &Drag) -> bool {
        match d.phase {
            Phase::Down => false,
            // a few points of travel first, so a click stays a click
            Phase::Move if self.dragging.is_none() && d.dx.hypot(d.dy) < 4.0 => false,
            Phase::Move => {
                self.dragging = Some(AgentDrag { pi, name: name.to_string(), at: d.window_point() });
                false
            }
            Phase::Up => {
                self.dragging = None;
                match self.drop_at.take() {
                    Some((j, zone)) if !d.cancelled() => {
                        self.drop_agent(pi, name, j, zone);
                        true
                    }
                    _ => false,
                }
            }
        }
    }

    /// Agent NAME of project PI dropped on pane J (the main area when there is no split) at ZONE.
    fn drop_agent(&mut self, pi: usize, name: &str, j: usize, zone: Zone) {
        match zone {
            Zone::Center => {
                self.focus_pane(j);
                self.cur = pi;
                self.open_agent(name, false);
            }
            Zone::Left | Zone::Right => self.insert_pane(if zone == Zone::Left { j } else { j + 1 }, pi, name),
        }
    }

    /// A new pane at place AT showing agent NAME of project PI, focused (nothing when the split is full).
    fn insert_pane(&mut self, at: usize, pi: usize, name: &str) {
        if self.panes.is_empty() {
            self.panes.push((self.cur, self.sel.clone()));
            self.focus = 0;
        }
        if self.panes.len() >= MAX {
            return;
        }
        self.panes[self.focus] = (self.cur, self.sel.clone());
        self.panes.insert(at, (pi, Sel::Agent(name.to_string())));
        if at <= self.focus {
            self.focus += 1;
        }
        self.focus_pane(at);
    }

    /// Add a pane showing what the focused one shows, and focus it.
    pub fn split(&mut self) {
        if self.panes.is_empty() {
            self.panes.push((self.cur, self.sel.clone()));
            self.focus = 0;
        }
        if self.panes.len() < MAX {
            self.panes.push((self.cur, self.sel.clone()));
            self.focus_pane(self.panes.len() - 1);
        }
    }

    /// Focus pane J: its project and page become the app's.
    pub fn focus_pane(&mut self, j: usize) {
        if j == self.focus || j >= self.panes.len() {
            return;
        }
        self.panes[self.focus] = (self.cur, self.sel.clone());
        self.focus = j;
        let (p, sel) = self.panes[j].clone();
        self.cur = p;
        match sel {
            Sel::Agent(name) => self.select_agent(&name),
            other => self.sel = other,
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
    }

    /// Agent NAME of project PI in a new pane beside the others (in the focused one when the split is full).
    pub fn open_to_side(&mut self, pi: usize, name: &str) {
        if self.panes.len() < MAX {
            self.split();
        }
        self.cur = pi;
        self.open_agent(name, false);
    }

    /// Whether the split has room for another pane.
    pub fn can_split(&self) -> bool {
        self.panes.len() < MAX
    }
}

/// The focused pane's title and content: the page SEL of project PI.
pub fn page(s: &App, k: &Kit) -> (String, El) {
    match (s.proj(), &s.sel) {
        (None, _) => ("yaaihh".to_string(), col().padding(24.0).child(text(s.notice.clone().unwrap_or_default()).font(k.ty.body).color(k.c.text_muted).wrap())),
        (Some(_), Sel::Agent(name)) => (name.clone(), crate::chat::pane(s, s.cur, name, k)),
        (Some(p), Sel::Agents) => ("Agents".into(), agents::page(s, p, k)),
        (Some(p), Sel::Clerk) => ("Clerk".into(), crate::clerk::page(s, p, k)),
        (_, Sel::Ext(name, page)) => match ext::get(name) {
            Some(e) => (e.page)(s, k, page),
            None => (name.clone(), col().padding(24.0).child(text(format!("{name} is not part of this yaaihh")).font(k.ty.body).color(k.c.text_muted))),
        },
        (Some(p), Sel::Mail) => ("Mail".into(), views::mail(s, p, k)),
        (Some(p), Sel::Questions) => ("Questions".into(), ask::inbox(s, p, k)),
        (Some(p), Sel::View(id)) => views::view(s, p, id, k),
    }
}

/// What an unfocused pane on page SEL of project PI is called.
fn label(s: &App, pi: usize, sel: &Sel) -> String {
    let project = s.projects.get(pi).map_or("?", |p| p.name.as_str());
    let page = match sel {
        Sel::Agent(name) => name.clone(),
        Sel::Agents => "Agents".into(),
        Sel::Clerk => "Clerk".into(),
        Sel::Mail => "Mail".into(),
        Sel::Questions => "Questions".into(),
        Sel::Ext(name, page) => return ext::get(name).map_or_else(|| name.clone(), |e| (e.title)(page)),
        Sel::View(v) if v.is_empty() => "Home".into(),
        Sel::View(v) => v.clone(),
    };
    format!("{page} · {project}")
}

/// While an agent is dragged: where it would land under the pointer, shown in LAYER (the zone tinted, the agent's
/// name by the pointer) and kept in `App::drop_at` for the release.
pub fn drop_zone(s: &App, cx: &mut rune::Cx<App>, k: &Kit, layer: &mut Vec<El>) {
    let Some(d) = &s.dragging else {
        s.drop_at.set(None);
        return;
    };
    let [x, y] = d.at;
    let rects: Vec<Option<[f32; 4]>> = if s.panes.is_empty() { vec![cx.layout("main")] } else { (0..s.panes.len()).map(|j| cx.layout(&format!("pane-{j}"))).collect() };
    let room = s.panes.len() < MAX;
    let hit = rects.iter().enumerate().find_map(|(j, r)| r.filter(|r| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]).map(|r| (j, r)));
    let target = hit.map(|(j, r)| {
        let f = (x - r[0]) / r[2];
        let zone = if room && f < 0.3 { Zone::Left } else if room && f > 0.7 { Zone::Right } else { Zone::Center };
        (j, zone, r)
    });
    s.drop_at.set(target.map(|(j, z, _)| (j, z)));
    if let Some((_, zone, r)) = target {
        let (zx, zw) = match zone {
            Zone::Left => (r[0], r[2] / 2.0),
            Zone::Right => (r[0] + r[2] / 2.0, r[2] / 2.0),
            Zone::Center => (r[0], r[2]),
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
            .child(text(d.name.clone()).font(k.ty.small_medium).color(k.c.text_base)),
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
                match &sel {
                    Sel::Agent(name) => crate::chat::pane(s, pi, name, k),
                    _ => col().w_full().padding(24.0).child(text("Click the header to show this page here").font(k.ty.small).color(k.c.text_faint)),
                }
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
                        .child(text(label(s, pi, &sel)).font(if is_focus { k.ty.small_medium } else { k.ty.small }).color(if is_focus { k.c.text_base } else { k.c.text_muted }).truncate().grow(1.0).shrink()),
                )
                .on_press(move |s: &mut App| s.focus_pane(j));
            let close = Button::new("×").variant(ButtonVariant::GhostMuted).on_press(move |s: &mut App| s.close_pane(j)).build(k);
            col()
                .key(format!("pane-{j}"))
                // any click inside a pane focuses it, also on its buttons, fields and text
                .on_press_within(move |s: &mut App| s.focus_pane(j))
                .grow(1.0)
                .min_w(0.0)
                .min_h(0.0)
                .h_full()
                .radius(k.radius.md)
                .clip_rounded(k.radius.md)
                .border(1.0, if is_focus { k.c.border_focus } else { k.c.border_weak })
                .child(row().w_full().center_y().child(head.grow(1.0)).child(close))
                .child(col().w_full().grow(1.0).min_h(0.0).child(body))
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
