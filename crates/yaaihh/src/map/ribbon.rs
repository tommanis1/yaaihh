//! A node's history unfolded in place (the owner, N-1791 p1, 2): every card of the cloud with more than one step of
//! history (its coming, then each [`super::NodeEvent`] after it: a post into a thread, a pass of an agent's brain, a
//! link made) carries a chip in its corner, "◷ N", N its steps by the moment the time bar shows. A press on the chip,
//! or U on the card selected, unfolds it ([`toggle`]): a rail under the card, drawn over the Map and placing nothing,
//! so no card beside it moves (`cloud_place.rs` keeps its places), its steps oldest first from left to right, each
//! joined to the one before by a [`Link::History`], the step it is at last and marked current. A step that forked off
//! the node (a sub-thread from one of its posts, [`Link::Branch`] in the graph) is drawn a row under the step before
//! it, joined by a [`Link::Branched`]. A press on a step selects the node it names, so the inspector opens it, and on a
//! step an agent can be branched at ([`branch_of`]) opens its Rewind / Branch from here under it: a copy of that
//! agent's conversation cut at its last turn's end by the step's time, beside it, the agent going on as it is
//! (`Request::ForkAgent`'s `at`). The chip again, or Esc, folds it.
//!
//! What it reads is what edge8's [`super::Graph::date`] gave each node when the graph was built: nothing is asked of
//! coordd and nothing is built again on an unfold.

use super::{Change, Graph, Kind, Link, Node, TIMELESS};
use crate::App;
use rune::{El, row, text};
use rune_kit::{Button, ButtonVariant, Kit};

/// One step of a node's history: when (seconds), the node a press on it selects, its words, and whether it forked off
/// the node (drawn a row under the rail).
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub at: i64,
    pub node: u32,
    pub words: String,
    pub forked: bool,
}

/// Node I's history by moment AT (None: now), oldest first: its coming, then what came into it and what was linked
/// with it. Empty for a node with no time.
pub fn steps(g: &Graph, i: u32, at: Option<i64>) -> Vec<Step> {
    let n = g.node(i);
    let by = |t: i64| t != TIMELESS && at.is_none_or(|m| t <= m);
    let mut out = Vec::new();
    if by(n.born) {
        out.push(Step { at: n.born, node: i, words: came_words(n), forked: false });
    }
    for e in n.events.iter().filter(|e| by(e.at)) {
        let (j, words, forked) = match e.what {
            Change::Child(j) => (j, short(&g.node(j).label), false),
            Change::Linked(Link::Branch, j) => (j, format!("branched: {}", short(&g.node(j).label)), true),
            Change::Linked(l, j) => (j, format!("{}: {}", link_word(l), short(&g.node(j).label)), false),
        };
        out.push(Step { at: e.at, node: j, words, forked });
    }
    out
}

/// How many steps node I's history has by moment AT: its chip's N (none under two, [`chip`]).
pub fn count(g: &Graph, i: u32, at: Option<i64>) -> usize {
    let n = g.node(i);
    let by = |t: i64| t != TIMELESS && at.is_none_or(|m| t <= m);
    by(n.born) as usize + n.events.iter().filter(|e| by(e.at)).count()
}

/// The words of a node's coming, its first step.
fn came_words(n: &Node) -> String {
    match n.kind {
        Kind::Agent => format!("{} came", short(&n.label)),
        _ => format!("came: {}", short(&n.label)),
    }
}

/// A link's word in a step: what was made with the node.
fn link_word(l: Link) -> &'static str {
    match l {
        Link::Names => "named",
        Link::Branch | Link::Branched => "branched",
        Link::Worked => "worked on by",
        Link::Uses => "uses",
        Link::Trail | Link::Answered => "answered",
        Link::Next => "next",
        Link::History => "then",
    }
}

/// A label cut to one short line.
fn short(t: &str) -> String {
    let t = t.lines().next().unwrap_or("").trim();
    if t.chars().count() <= 40 { t.to_string() } else { format!("{}…", t.chars().take(39).collect::<String>()) }
}

/// The joins between STEPS (`steps`), by their indices: each step on the rail to the one before it on the rail, a
/// [`Link::History`]; a forked step to the step before it, a [`Link::Branched`].
pub fn joins(steps: &[Step]) -> Vec<(usize, usize, Link)> {
    let mut out = Vec::new();
    let mut last_rail: Option<usize> = None;
    for (k, s) in steps.iter().enumerate() {
        if s.forked {
            if k > 0 {
                out.push((k - 1, k, Link::Branched));
            }
        } else {
            if let Some(r) = last_rail {
                out.push((r, k, Link::History));
            }
            last_rail = Some(k);
        }
    }
    out
}

/// The chip's words for node I at moment AT: "◷ N" where its history has two steps or more.
pub fn chip(g: &Graph, i: u32, at: Option<i64>) -> Option<String> {
    let n = count(g, i, at);
    (n >= 2).then(|| format!("◷ {n}"))
}

/// The agent a step of node I's rail branches at STEP's time: the agent the rail is, else the agent the step names,
/// else the writer of the post it is; a Claude Code agent of the cloud's project, the current one, not boxed, its
/// coordd at [`coord_types::REWIND_PROTOCOL`] or later. None for a step of no time.
pub fn branch_of(s: &App, g: &Graph, i: u32, step: &Step) -> Option<String> {
    let p = s.projects.get(s.cloud.of).filter(|p| s.cloud.of == s.cur && !p.boxed && p.protocol >= coord_types::REWIND_PROTOCOL)?;
    if step.at == TIMELESS {
        return None;
    }
    let named = |id: &str| id.strip_prefix("agent:").map(String::from);
    let writer = |id: &str| {
        let (t, n) = id.split_once(".p")?;
        let n: u32 = n.parse().ok()?;
        p.live.tickets.values().find(|v| v.id == t)?.posts.iter().find(|q| q.n == n).map(|q| q.by.clone())
    };
    let at = &g.node(step.node).id;
    let name = named(&g.node(i).id).or_else(|| named(at)).or_else(|| writer(at))?;
    p.live.agents.iter().find(|a| a.name == name && (a.kind == "claude" || a.kind.is_empty())).map(|a| a.name.clone())
}

/// Unfold node ID's history, or fold it when it is unfolded, its step's Rewind / Branch from here closed. Whether it
/// is unfolded now.
pub fn toggle(s: &mut App, id: &str) -> bool {
    if s.cloud.rewind.as_ref().is_some_and(|(r, _)| r == id) {
        s.cloud.rewind = None;
    }
    let u = &mut s.cloud.unfolded;
    if u.remove(id) {
        false
    } else {
        u.insert(id.to_string());
        true
    }
}

/// The chip of node I's card, its words CHIP, as a press: the history unfolded or folded ([`toggle`]).
pub fn chip_el(id: &str, words: String, unfolded: bool, k: &Kit) -> El {
    let key = id.to_string();
    row()
        .key(format!("map-chip-{id}"))
        .h(CHIP_HIGH)
        .px(5.0)
        .items_center()
        .radius(CHIP_HIGH / 2.0)
        .fill(if unfolded { k.c.layer_02 } else { k.c.layer_01 })
        .border(1.0, if unfolded { k.c.border_focus } else { k.c.border_muted })
        .cursor(rune::Cursor::Pointer)
        .child(text(words).font(k.ty.small).color(if unfolded { k.c.text_base } else { k.c.text_muted }).single_line())
        .on_drag(move |s: &mut App, d: &mut rune::Drag, _tx: &mut rune::Tx| {
            if d.phase == rune::Phase::Up && !d.cancelled() {
                toggle(s, &key);
            }
        })
}

/// The width CHIP's chip takes ([`chip_el`]): its words, 5 points each side and its border.
pub fn chip_wide(chip: &str, k: &Kit) -> f32 {
    rune::text::measure(chip, &[], k.ty.small, None).0 + 12.0
}

/// The room between a rail's panel's edge and its steps.
const PANEL_PAD: f32 = 6.0;

/// The chip's height.
pub const CHIP_HIGH: f32 = 16.0;
/// A step's height on the rail, and the gap between two rows of it.
pub const STEP_HIGH: f32 = 22.0;
const ROW_GAP: f32 = 8.0;
/// The room between two steps, which their join crosses.
const STEP_GAP: f32 = 18.0;
/// The widest a step's words are drawn.
const STEP_WIDE: f32 = 220.0;
/// The height of a step's Rewind / Branch from here, a row under the rail.
const ACTION_HIGH: f32 = 28.0;

/// The rails of the unfolded nodes on screen, in the canvas's points (VIEW wide and high), over the cards: each under
/// its node's card (over it where the view's foot leaves no room), as many of its latest steps as the view's width
/// holds, "N earlier" before them where it does not hold all.
pub fn drawn(s: &App, g: &Graph, view: [f32; 2], k: &Kit) -> Vec<El> {
    let Some(root) = super::canvas::project_id(s, s.cloud.of).and_then(|id| g.project(&id)) else { return Vec::new() };
    let at = s.map.moment();
    let sel = s.map.selected();
    let wide = |t: &str| rune::text::measure(t, &[], k.ty.small, None).0;
    let faded = super::canvas::faded;
    let mut out = Vec::new();
    for id in &s.cloud.unfolded {
        let Some(i) = g.find_in(root, id) else { continue };
        if !g.there(i, at) {
            continue;
        }
        let Some(r) = s.map.on_screen(g, i) else { continue };
        if r[0] > view[0] || r[1] > view[1] || r[0] + r[2] < 0.0 || r[1] + r[3] < 0.0 {
            continue;
        }
        let all = steps(g, i, at);
        if all.len() < 2 {
            continue;
        }
        let rows = if all.iter().any(|s| s.forked) { 2.0 } else { 1.0 };
        // the step whose Rewind / Branch from here is open, and the agent it branches: a row under the rail
        let open = s.cloud.rewind.as_ref().filter(|(r, _)| r == id).and_then(|(_, n)| Some((*n, branch_of(s, g, i, all.get(*n)?)?)));
        let steps_high = rows * STEP_HIGH + (rows - 1.0) * ROW_GAP;
        let high = steps_high + if open.is_some() { ROW_GAP + ACTION_HIGH } else { 0.0 };
        // its panel's inside: under the card, else over it, PANEL_PAD in from the panel's edge
        let below = r[1] + r[3] + 6.0 + PANEL_PAD;
        let top = if below + high + PANEL_PAD <= view[1] { below } else { (r[1] - 6.0 - PANEL_PAD - high).max(PANEL_PAD) };
        let left = r[0].max(4.0 + PANEL_PAD);
        // the words and width of each step, the last the current one
        let last = all.len() - 1;
        let says: Vec<String> = all.iter().enumerate().map(|(n, x)| format!("#{} {} · {}{}", n + 1, crate::when::secs(x.at), x.words, if n == last { " · current" } else { "" })).collect();
        let widths: Vec<f32> = says.iter().map(|t| wide(t).min(STEP_WIDE) + 16.0).collect();
        // as many of the latest as fit from LEFT to the view's right edge, a pill for the earlier before them
        let room = view[0] - left - 4.0 - PANEL_PAD;
        let mut from = all.len();
        let mut used = 0.0;
        while from > 0 {
            let w = widths[from - 1] + if from < all.len() { STEP_GAP } else { 0.0 };
            let more = if from - 1 > 0 { wide(&format!("{} earlier", from - 1)) + 16.0 + STEP_GAP } else { 0.0 };
            if used + w + more > room && from < all.len() {
                break;
            }
            used += w;
            from -= 1;
        }
        let mut x = left;
        let mut boxes: Vec<Option<[f32; 4]>> = vec![None; all.len()];
        let mut el = rune::el().key(format!("map-ribbon-{id}")).absolute(0.0, 0.0).size(view[0], view[1]);
        let mut lines = rune::Shape::new();
        if from > 0 {
            let t = format!("{from} earlier");
            let w = wide(&t) + 16.0;
            el = el.child(text(t).font(k.ty.small).color(k.c.text_faint).single_line().absolute(x + 8.0, top + 4.0));
            lines = lines.line([x + w - 4.0, top + STEP_HIGH / 2.0], [x + w + STEP_GAP, top + STEP_HIGH / 2.0], 1.0, rune::LineCap::Round, faded(k.c.border_strong, 0.6));
            x += w + STEP_GAP;
        }
        // the rail's steps left to right; a forked one under the step before it, the rail going on after it
        for n in from..all.len() {
            let w = widths[n];
            let b = if all[n].forked { [x - STEP_GAP / 2.0, top + STEP_HIGH + ROW_GAP, w, STEP_HIGH] } else { [x, top, w, STEP_HIGH] };
            if !all[n].forked {
                x += w + STEP_GAP;
            }
            boxes[n] = Some(b);
        }
        for (a, b, l) in joins(&all) {
            let (Some(pa), Some(pb)) = (boxes[a], boxes[b]) else { continue };
            let ink = faded(k.c.border_strong, 0.9);
            match l {
                Link::Branched => {
                    let from = [pa[0] + pa[2] / 2.0, pa[1] + pa[3]];
                    let to = [pb[0], pb[1] + pb[3] / 2.0];
                    lines = lines.line(from, [from[0], to[1]], 1.0, rune::LineCap::Round, ink).line([from[0], to[1]], to, 1.0, rune::LineCap::Round, ink);
                }
                _ => {
                    lines = lines.line([pa[0] + pa[2], pa[1] + pa[3] / 2.0], [pb[0], pb[1] + pb[3] / 2.0], 1.0, rune::LineCap::Round, ink);
                }
            }
        }
        // an opaque raised panel under the rail ([`panel`])
        let action = open.as_ref().map(|(n, agent)| action_el(s, id, *n, agent, all[*n].at, [left, top + steps_high + ROW_GAP], k));
        let right = boxes.iter().flatten().map(|b| b[0] + b[2]).chain(action.as_ref().map(|(_, w)| left + w)).fold(left, f32::max);
        el = el.child(panel(format!("map-ribbon-panel-{id}"), [left - PANEL_PAD, top - PANEL_PAD, right - left + 2.0 * PANEL_PAD, high + 2.0 * PANEL_PAD], k));
        el = el.child(rune::canvas(lines).absolute(0.0, 0.0).size(view[0], view[1]));
        if let Some((a, _)) = action {
            el = el.child(super::canvas::floating(a));
        }
        for n in from..all.len() {
            let Some(b) = boxes[n] else { continue };
            let node = all[n].node;
            let current = n == last;
            let chosen = sel == Some(node) || open.as_ref().is_some_and(|(o, _)| *o == n);
            let branches = branch_of(s, g, i, &all[n]).is_some();
            let rail = id.clone();
            let step = pill(format!("map-step-{id}-{}", n + 1), b, says[n].clone(), current, chosen, k).on_drag(move |s: &mut App, d: &mut rune::Drag, _tx: &mut rune::Tx| {
                    if d.phase == rune::Phase::Up && !d.cancelled() {
                        super::canvas::select(s, Some(node));
                        let again = s.cloud.rewind.as_ref().is_some_and(|(r, o)| *r == rail && *o == n);
                        s.cloud.rewind = (branches && !again).then(|| (rail.clone(), n));
                    }
                });
            el = el.child(super::canvas::floating(step));
        }
        out.push(el);
    }
    out
}

/// A rail's opaque raised panel at R (x, y, w, h), a floating control, so a press on it between its steps goes to no
/// card under it (the owner, N-1791 p11, 2: over a card the rail's gaps showed it through). The causal rail's too.
pub fn panel(key: String, r: [f32; 4], k: &Kit) -> El {
    let p = rune::el()
        .key(key)
        .absolute(r[0], r[1])
        .size(r[2], r[3])
        .radius(k.radius.md)
        .fill(k.c.bg_base)
        .border(1.0, k.c.border_strong)
        .shadow(4.0, 12.0, super::canvas::faded(k.c.bg_inverse, 0.18))
        .on_drag(|_s: &mut App, _d: &mut rune::Drag, _tx: &mut rune::Tx| {});
    super::canvas::floating(p)
}

/// A rail's step at B saying WORDS, ringed when CURRENT or CHOSEN; its press is the caller's. The causal rail's too.
pub fn pill(key: String, b: [f32; 4], words: String, current: bool, chosen: bool, k: &Kit) -> El {
    let faded = super::canvas::faded;
    row()
        .key(key)
        .absolute(b[0], b[1])
        .size(b[2], b[3])
        .px(8.0)
        .items_center()
        .radius(STEP_HIGH / 2.0)
        .fill(k.c.layer_01)
        .border(if current || chosen { 2.0 } else { 1.0 }, if chosen { k.c.border_focus } else if current { faded(k.c.text_accent, 0.85) } else { k.c.border_muted })
        .cursor(rune::Cursor::Pointer)
        .child(text(words).font(k.ty.small).color(if current { k.c.text_base } else { k.c.text_muted }).single_line().truncate().w(b[2] - 16.0))
}

/// Step N of rail ID's Rewind / Branch from here, AGENT's copy cut by AT, its row's top-left at AT_XY: its press, what
/// it cuts at, and the row's width.
fn action_el(s: &App, id: &str, n: usize, agent: &str, at: i64, xy: [f32; 2], k: &Kit) -> (El, f32) {
    let forking = s.projects.get(s.cloud.of).is_some_and(|p| p.forking.is_some());
    let words = if forking { "Branching…" } else { "Rewind / Branch from here" };
    let when = crate::when::secs(at);
    let said = format!("{agent}'s conversation, up to {when}");
    let wide = rune::text::measure(words, &[], k.ty.small, None).0 + 32.0 + 8.0 + rune::text::measure(&said, &[], k.ty.small, None).0;
    let name = agent.to_string();
    let press = Button::new(words)
        .variant(ButtonVariant::Contrast)
        .disabled(forking)
        .on_press(move |s: &mut App| {
            s.cloud.rewind = None;
            crate::projects::fork(s, &name, Some(at));
        })
        .build(k)
        .key(format!("map-rewind-{id}-{}", n + 1))
        .help(format!("A copy of {said}; {agent} carries on"));
    let row = row().key(format!("map-rewind-row-{id}")).absolute(xy[0], xy[1]).h(ACTION_HIGH).gap(8.0).items_center().child(press).child(text(said).font(k.ty.small).color(k.c.text_muted).single_line());
    (row, wide)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::map::{Edge, Rect};

    /// A thread with three posts at 10, 20 and 30 s and a sub-thread branched off its second post at 25 s.
    pub fn threaded() -> Graph {
        let mut g = Graph::new(
            vec![
                Node::new("project:research", Kind::Project, "research", None, Rect::new(0.0, 0.0, 1000.0, 1000.0)),
                Node::new("N-1", Kind::Thread, "the fixture", Some(0), Rect::new(10.0, 10.0, 300.0, 300.0)),
                Node::new("N-1.p1", Kind::Post, "first", Some(1), Rect::new(20.0, 20.0, 100.0, 20.0)),
                Node::new("N-1.p2", Kind::Post, "second", Some(1), Rect::new(20.0, 50.0, 100.0, 20.0)),
                Node::new("N-1.p3", Kind::Post, "third", Some(1), Rect::new(20.0, 80.0, 100.0, 20.0)),
                Node::new("N-2", Kind::Thread, "the fork", Some(0), Rect::new(400.0, 10.0, 300.0, 300.0)),
            ],
            vec![Edge { from: 5, to: 1, link: Link::Branch, when: 25 }],
        )
        .unwrap();
        g.date(|id| match id {
            "N-1.p1" => Some(10),
            "N-1.p2" => Some(20),
            "N-1.p3" => Some(30),
            "N-2" => Some(25),
            _ => None,
        });
        g
    }

    /// The owner's N-1791 p1, 2: a thread's history is its coming and each post into it, oldest first; a sub-thread
    /// forked off it is a step of its own, under the rail, joined by Branched; at a past moment only the steps by
    /// then, and its chip counts them; a node with one step has no chip.
    #[test]
    fn a_nodes_history_is_its_coming_then_what_came_into_it_and_what_forked_off_it() {
        let g = threaded();
        let all = steps(&g, 1, None);
        let said: Vec<(i64, &str, bool)> = all.iter().map(|s| (s.at, g.node(s.node).id.as_str(), s.forked)).collect();
        assert_eq!(said, [(10, "N-1", false), (20, "N-1.p2", false), (25, "N-2", true), (30, "N-1.p3", false)]);
        assert_eq!(all[2].words, "branched: the fork");
        assert_eq!(joins(&all), [(0, 1, Link::History), (1, 2, Link::Branched), (1, 3, Link::History)]);
        assert_eq!(chip(&g, 1, None).as_deref(), Some("◷ 4"));
        assert_eq!(chip(&g, 1, Some(22)).as_deref(), Some("◷ 2"), "by 22 s: its coming and p2");
        assert_eq!(steps(&g, 1, Some(22)).len(), 2);
        assert_eq!(chip(&g, 1, Some(10)), None, "one step: no chip");
        assert_eq!(chip(&g, 2, None), None, "a post with nothing after it");
    }
}
