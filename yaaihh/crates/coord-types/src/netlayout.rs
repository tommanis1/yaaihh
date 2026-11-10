//! The geometry of a drawn net (owner m127, m130): where each step, arm label and line of a [`NetGraph`] goes, in
//! points, so the Clerk page draws it with absolute positions and a test can walk the lines. Text sizes come from the
//! caller ([`Measure`]: the UI's text engine, a test's estimate).
//!
//! Every block has one axis: the line comes in at its top and leaves at its bottom on it, so a sequence lines its
//! blocks up on a shared axis with a line between each two. A branch puts its arms side by side under a bar from its
//! head's axis and joins the open ones (those that do not end in a fault) under another; when that is wider than the
//! room given, the arms stack instead, each joined to a rail on the left by a line to its label and, when open, by a
//! line back from its end, the rail joined to the head above and to the axis below. A loop frames its body, the line
//! running through the frame's top and bottom. Lines are horizontal or vertical segments; [`unreached`] lists what no
//! line joins to the event.

use crate::{FlowNode, NetFlow, NetGraph};
use std::collections::HashSet;

/// The width the Clerk page gives a drawing; a branch wider stacks its arms.
pub const ROOM: f32 = 560.0;
/// Space between two steps, and around a label on a line.
pub const GAP: f32 = 16.0;
/// Space between two arms side by side.
pub const ARM_GAP: f32 = 20.0;
/// A stacked branch's rail, from its block's left edge; arms start this far right of it.
pub const RAIL: f32 = 10.0;
pub const TICK: f32 = 16.0;
/// A loop's frame: inside space at its sides, top and bottom.
pub const FRAME: f32 = 12.0;
/// A step's padding (sides, top and bottom), its texts' widest measure, a call's session bar (with its gap), an arc
/// out's line before its label, a label's padding.
pub const PAD_X: f32 = 10.0;
pub const PAD_Y: f32 = 6.0;
pub const TEXT_W: f32 = 230.0;
pub const BAR: f32 = 9.0;
pub const OUT_LINE: f32 = 12.0;
pub const CHIP_X: f32 = 8.0;
pub const CHIP_Y: f32 = 2.0;
pub const OUT_W: f32 = 150.0;

/// The kinds of text a drawing sets: a step's label (an effect's, smaller), a step's detail, a label on a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Font {
    Label,
    Small,
}

/// The size in points of text set in a font, wrapped at a width.
pub type Measure<'a> = &'a dyn Fn(&str, Font, f32) -> (f32, f32);

/// What a placed box is.
#[derive(Clone, Debug, PartialEq)]
pub enum What {
    /// A step: its node, whether it heads a branch or loop (then it is marked from its own line), its label's and
    /// detail's fonts and sizes, the arcs out's label sizes.
    Step { node: FlowNode, head: bool, shape_w: f32, label: (f32, f32), detail: Option<(f32, f32)>, outs: Vec<(f32, f32)> },
    /// An arm's label on its line.
    Arm(String),
    /// Where the net ends.
    Done,
    /// A loop's frame (drawn behind; not a step).
    Frame,
}

/// A box at (X, Y) of size (W, H). LIT: a run went through it.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub what: What,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub lit: bool,
}

/// A line from (X0, Y0) to (X1, Y1), horizontal or vertical.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seg {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub lit: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub w: f32,
    pub h: f32,
    pub items: Vec<Placed>,
    pub segs: Vec<Seg>,
}

/// A laid-out block: its size, its axis, whether a line leaves its bottom.
struct Block {
    w: f32,
    h: f32,
    ax: f32,
    open: bool,
    items: Vec<Placed>,
    segs: Vec<Seg>,
}

impl Block {
    fn empty() -> Block {
        Block { w: 0.0, h: 0.0, ax: 0.0, open: true, items: Vec::new(), segs: Vec::new() }
    }

    fn put(&mut self, b: Block, dx: f32, dy: f32) {
        self.items.extend(b.items.into_iter().map(|mut p| {
            p.x += dx;
            p.y += dy;
            p
        }));
        self.segs.extend(b.segs.into_iter().map(|s| Seg { x0: s.x0 + dx, x1: s.x1 + dx, y0: s.y0 + dy, y1: s.y1 + dy, ..s }));
    }

    fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, lit: bool) {
        if (x0 - x1).abs() > 0.01 || (y0 - y1).abs() > 0.01 {
            self.segs.push(Seg { x0, y0, x1, y1, lit });
        }
    }

    /// Moved so nothing lies left of 0; the width taken from what it holds.
    fn normalised(mut self) -> Block {
        let left = self.items.iter().map(|p| p.x).chain(self.segs.iter().map(|s| s.x0.min(s.x1))).fold(f32::INFINITY, f32::min);
        if left.is_finite() && left.abs() > 0.01 {
            let mut b = Block { w: 0.0, h: self.h, ax: self.ax - left, open: self.open, items: Vec::new(), segs: Vec::new() };
            b.put(Block { items: std::mem::take(&mut self.items), segs: std::mem::take(&mut self.segs), ..Block::empty() }, -left, 0.0);
            self = b;
        }
        self.w = self.items.iter().map(|p| p.x + p.w).chain(self.segs.iter().map(|s| s.x0.max(s.x1))).fold(0.0, f32::max);
        self
    }
}

/// The detail line under step N's label: its own detail and, for a call, its session (or "fresh").
pub fn detail(n: &FlowNode) -> String {
    let tag = match n.kind.as_str() {
        "call" => Some(n.session.as_ref().map_or("fresh".to_string(), |s| format!("session {s}"))),
        _ => None,
    };
    [n.detail.as_str(), tag.as_deref().unwrap_or("")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ")
}

/// The font of step N's label.
pub fn label_font(n: &FlowNode) -> Font {
    if matches!(n.kind.as_str(), "data" | "effect" | "fault" | "raise") { Font::Small } else { Font::Label }
}

struct Ctx<'a> {
    measure: Measure<'a>,
    run: Option<&'a HashSet<usize>>,
}

impl Ctx<'_> {
    fn went(&self, f: &NetFlow) -> bool {
        let Some(r) = self.run else { return false };
        fn w(f: &NetFlow, r: &HashSet<usize>) -> bool {
            match f {
                NetFlow::Seq(xs) => xs.iter().any(|x| w(x, r)),
                NetFlow::Node(n) => r.contains(&n.lines.0),
                NetFlow::Branch { head, arms } => r.contains(&head.lines.0) || arms.iter().any(|a| w(&a.body, r)),
                NetFlow::Each { head, body } => r.contains(&head.lines.0) || w(body, r),
            }
        }
        w(f, r)
    }

    fn step(&self, n: &FlowNode, head: bool) -> Block {
        let label = (self.measure)(&n.label, label_font(n), TEXT_W);
        let d = detail(n);
        let detail = (!d.is_empty()).then(|| (self.measure)(&d, Font::Small, TEXT_W));
        let bar = if n.kind == "call" && n.session.is_some() { BAR } else { 0.0 };
        let shape_w = 2.0 * PAD_X + bar + label.0.max(detail.map_or(0.0, |d| d.0));
        let shape_h = 2.0 * PAD_Y + label.1 + detail.map_or(0.0, |d| d.1 + 2.0);
        let outs: Vec<(f32, f32)> = n.out.iter().map(|o| {
            let (w, h) = (self.measure)(&format!("→ {o}"), Font::Small, OUT_W);
            (w + 2.0 * 6.0, h + 2.0 * CHIP_Y)
        }).collect();
        let w = shape_w + outs.iter().map(|o| OUT_LINE + o.0).sum::<f32>();
        let h = outs.iter().map(|o| o.1).fold(shape_h, f32::max);
        let lit = self.run.is_some_and(|r| r.contains(&n.lines.0));
        let what = What::Step { node: n.clone(), head, shape_w, label, detail, outs };
        Block { w, h, ax: shape_w / 2.0, open: !n.terminal, items: vec![Placed { what, x: 0.0, y: 0.0, w, h, lit }], segs: Vec::new() }
    }

    fn chip(&self, label: &str) -> (f32, f32) {
        let (w, h) = (self.measure)(label, Font::Small, TEXT_W);
        (w + 2.0 * CHIP_X, h + 2.0 * CHIP_Y)
    }

    /// FLOW laid out in ROOM points of width: a branch whose arms side by side are wider stacks them.
    fn flow(&self, f: &NetFlow, room: f32) -> Block {
        match f {
            NetFlow::Node(n) => self.step(n, false),
            // blocks on a shared axis can together be wider than each alone: their branches get less room until they fit
            NetFlow::Seq(xs) => {
                let mut r = room;
                loop {
                    let b = self.seq(xs.iter().map(|x| (self.flow(x, r), self.went(x))).collect());
                    if b.w <= room || r <= TEXT_W {
                        break b;
                    }
                    r -= (b.w - room).max(GAP);
                }
            }
            NetFlow::Each { head, body } => self.each(head, body, self.went(f), room),
            NetFlow::Branch { head, arms } => {
                let side = self.side(head, arms, self.went(f), room);
                if side.w <= room || arms.len() < 2 { side } else { self.stacked(head, arms, self.went(f), room) }
            }
        }
    }

    /// BLOCKS one under another on a shared axis, each with whether a run reached it.
    fn seq(&self, blocks: Vec<(Block, bool)>) -> Block {
        if blocks.is_empty() {
            return Block::empty();
        }
        let ax = blocks.iter().map(|(b, _)| b.ax).fold(0.0, f32::max);
        let mut out = Block { ax, ..Block::empty() };
        let mut y = 0.0;
        for (i, (b, lit)) in blocks.into_iter().enumerate() {
            if i > 0 {
                out.line(ax, y, ax, y + GAP, lit);
                y += GAP;
            }
            let (h, open, bax) = (b.h, b.open, b.ax);
            out.put(b, ax - bax, y);
            y += h;
            out.open = open;
        }
        out.h = y;
        out.normalised()
    }

    fn each(&self, head: &FlowNode, body: &NetFlow, lit: bool, room: f32) -> Block {
        let hb = self.step(head, true);
        let bb = self.flow(body, room - 2.0 * FRAME);
        let body_lit = self.went(body);
        let ax = hb.ax.max(FRAME + bb.ax);
        let (fx, fw) = (ax - FRAME - bb.ax, bb.w + 2.0 * FRAME);
        let mut out = Block { ax, ..Block::empty() };
        let hh = hb.h;
        let hax = hb.ax;
        out.put(hb, ax - hax, 0.0);
        let fy = hh + GAP;
        out.line(ax, hh, ax, fy, lit);
        out.line(ax, fy, ax, fy + FRAME, body_lit);
        let by = fy + FRAME;
        let bh = bb.h;
        out.put(bb, fx + FRAME, by);
        let fb = by + bh + FRAME;
        out.line(ax, by + bh, ax, fb, body_lit);
        out.items.insert(0, Placed { what: What::Frame, x: fx, y: fy, w: fw, h: fb - fy, lit: false });
        out.h = fb;
        out.normalised()
    }

    /// A branch with its arms side by side.
    fn side(&self, head: &FlowNode, arms: &[crate::FlowArm], lit: bool, room: f32) -> Block {
        let hb = self.step(head, true);
        let mut cols = Vec::new();
        let mut x = 0.0;
        for a in arms {
            let (cw, ch) = self.chip(&a.label);
            let bb = self.flow(&a.body, room);
            let left = (cw / 2.0).max(bb.ax);
            let right = (cw / 2.0).max(bb.w - bb.ax);
            cols.push((x + left, cw, ch, bb, self.went(&a.body), &a.label));
            x += left + right + ARM_GAP;
        }
        let first = cols.first().map_or(0.0, |c| c.0);
        let last = cols.last().map_or(0.0, |c| c.0);
        let ax = (first + last) / 2.0;
        let mut out = Block { ax, ..Block::empty() };
        let hh = hb.h;
        let hax = hb.ax;
        out.put(hb, ax - hax, 0.0);
        let by = hh + GAP / 2.0;
        out.line(ax, hh, ax, by, lit);
        let mut ends = Vec::new();
        for (cx, cw, ch, bb, arm_lit, label) in cols {
            out.line(ax.min(cx), by, ax.max(cx), by, arm_lit);
            let cy = by + GAP / 2.0;
            out.line(cx, by, cx, cy, arm_lit);
            out.items.push(Placed { what: What::Arm(label.clone()), x: cx - cw / 2.0, y: cy, w: cw, h: ch, lit: arm_lit });
            let top = cy + ch + GAP / 2.0;
            out.line(cx, cy + ch, cx, top, arm_lit);
            let (bh, bax, open) = (bb.h, bb.ax, bb.open);
            out.put(bb, cx - bax, top);
            ends.push((cx, top + bh, open, arm_lit));
        }
        let open = ends.iter().any(|e| e.2);
        let jy = ends.iter().map(|e| e.1).fold(0.0, f32::max) + GAP / 2.0;
        for &(cx, ey, o, arm_lit) in &ends {
            if o {
                out.line(cx, ey, cx, jy, arm_lit);
                out.line(ax.min(cx), jy, ax.max(cx), jy, arm_lit);
            }
        }
        out.open = open;
        out.h = if open { jy } else { jy - GAP / 2.0 };
        out.normalised()
    }

    /// A branch with its arms one under another on a rail.
    fn stacked(&self, head: &FlowNode, arms: &[crate::FlowArm], lit: bool, room: f32) -> Block {
        let hb = self.step(head, true);
        let ax = hb.ax.max(RAIL + TICK);
        let mut out = Block { ax, ..Block::empty() };
        let hh = hb.h;
        let hax = hb.ax;
        out.put(hb, ax - hax, 0.0);
        let y1 = hh + GAP / 2.0;
        out.line(ax, hh, ax, y1, lit);
        out.line(RAIL, y1, ax, y1, lit);
        let mut y = y1 + GAP / 2.0;
        let mut rail_end = y1;
        let mut open = false;
        for a in arms {
            let arm_lit = self.went(&a.body);
            let (cw, ch) = self.chip(&a.label);
            let bb = self.flow(&a.body, room - RAIL - TICK);
            let cx = RAIL + TICK + (cw / 2.0).max(bb.ax);
            out.items.push(Placed { what: What::Arm(a.label.clone()), x: cx - cw / 2.0, y, w: cw, h: ch, lit: arm_lit });
            let tick = y + ch / 2.0;
            out.line(RAIL, tick, cx - cw / 2.0, tick, arm_lit);
            rail_end = rail_end.max(tick);
            let top = y + ch + GAP / 2.0;
            out.line(cx, y + ch, cx, top, arm_lit);
            let (bh, bax, bopen) = (bb.h, bb.ax, bb.open);
            out.put(bb, cx - bax, top);
            y = top + bh;
            if bopen {
                open = true;
                out.line(cx, y, cx, y + GAP / 2.0, arm_lit);
                y += GAP / 2.0;
                out.line(RAIL, y, cx, y, arm_lit);
                rail_end = y;
            }
            y += GAP;
        }
        if open {
            let exit = y;
            out.line(RAIL, rail_end, RAIL, exit, lit);
            out.line(RAIL, exit, ax, exit, lit);
            rail_end = exit;
            out.h = exit;
        } else {
            out.h = y - GAP;
        }
        out.line(RAIL, y1, RAIL, rail_end, lit);
        out.open = open;
        out.normalised()
    }
}

/// Net G laid out in ROOM points of width (a branch wider stacks its arms), its run's path lit when RUN gives the
/// source lines the run went through.
pub fn layout(g: &NetGraph, measure: Measure, room: f32, run: Option<&HashSet<usize>>) -> Layout {
    let cx = Ctx { measure, run };
    let start = cx.step(&g.start, false);
    let body = cx.flow(&g.body, room);
    let body_lit = cx.went(&g.body) || run.is_some_and(|r| r.contains(&g.start.lines.0));
    let mut blocks = vec![(start, true), (body, body_lit)];
    if blocks[1].0.open {
        let (w, h) = measure("done", Font::Small, TEXT_W);
        let (w, h) = (w + 2.0 * PAD_X, h + 2.0 * PAD_Y);
        blocks.push((Block { w, h, ax: w / 2.0, open: false, items: vec![Placed { what: What::Done, x: 0.0, y: 0.0, w, h, lit: body_lit }], segs: Vec::new() }, body_lit));
    }
    // an empty body is no block: the event's line goes straight on to the end
    blocks.retain(|(b, _)| !b.items.is_empty() || !b.segs.is_empty());
    let b = cx.seq(blocks);
    Layout { w: b.w, h: b.h, items: b.items, segs: b.segs }
}

/// The boxes no line joins to the event (each by its label), empty when the drawing is connected.
pub fn unreached(l: &Layout) -> Vec<String> {
    const E: f32 = 0.5;
    let boxes: Vec<&Placed> = l.items.iter().filter(|p| p.what != What::Frame).collect();
    let n = boxes.len() + l.segs.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut Vec<usize>, i: usize) -> usize {
        if p[i] != i {
            let r = find(p, p[i]);
            p[i] = r;
        }
        p[i]
    }
    let join = |p: &mut Vec<usize>, a: usize, b: usize| {
        let (a, b) = (find(p, a), find(p, b));
        p[a] = b;
    };
    let on_seg = |s: &Seg, x: f32, y: f32| x >= s.x0.min(s.x1) - E && x <= s.x0.max(s.x1) + E && y >= s.y0.min(s.y1) - E && y <= s.y0.max(s.y1) + E;
    // the part of a box a line touches: its edge (a step's shape, not its arcs out)
    let touches = |p: &Placed, x: f32, y: f32| {
        let w = match &p.what {
            What::Step { shape_w, .. } => *shape_w,
            _ => p.w,
        };
        x >= p.x - E && x <= p.x + w + E && y >= p.y - E && y <= p.y + p.h + E && ((y - p.y).abs() <= E || (y - p.y - p.h).abs() <= E || (x - p.x).abs() <= E || (x - p.x - w).abs() <= E)
    };
    for (i, s) in l.segs.iter().enumerate() {
        let si = boxes.len() + i;
        for (j, t) in l.segs.iter().enumerate().skip(i + 1) {
            if on_seg(t, s.x0, s.y0) || on_seg(t, s.x1, s.y1) || on_seg(s, t.x0, t.y0) || on_seg(s, t.x1, t.y1) {
                join(&mut parent, si, boxes.len() + j);
            }
        }
        for (b, p) in boxes.iter().enumerate() {
            if touches(p, s.x0, s.y0) || touches(p, s.x1, s.y1) {
                join(&mut parent, si, b);
            }
        }
    }
    let Some(start) = boxes.iter().position(|p| matches!(&p.what, What::Step { node, .. } if node.kind == "start")) else { return vec!["no event".into()] };
    let root = find(&mut parent, start);
    let mut out = Vec::new();
    for (b, p) in boxes.iter().enumerate() {
        if find(&mut parent, b) != root {
            out.push(match &p.what {
                What::Step { node, .. } => node.label.clone(),
                What::Arm(l) => format!("arm {l}"),
                What::Done => "done".into(),
                What::Frame => "frame".into(),
            });
        }
    }
    out
}
