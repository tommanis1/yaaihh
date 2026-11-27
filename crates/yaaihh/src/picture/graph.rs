//! The board the project's Docket is drawn on (0.2 line 26; the owner, N-1104 p496: "a visual representation of docket
//! programs … where are the arrows"): boxes placed where the drawing puts them, and arrows between them on a canvas. A
//! program's frame lies under the arrows and what stands in it (a step, a name, a count) over them, so an arrow meets a
//! step at its edge and runs over the frame it leaves. What rune lacks, only: its curves are rune's `Path`; its dashes
//! and heads are the picture's wires' own (picture/wire.rs: `wire::dashed`, `wire::head`), so the page has one dash
//! and one head.

use super::wire;
use rune::{Color, El, LineCap, Path, Shape, TextStyle, col, text};

pub type P = [f32; 2];

/// A placed rectangle, in the board's points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct R {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl R {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> R {
        R { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn mid_x(&self) -> f32 {
        self.x + self.w / 2.0
    }
    pub fn mid_y(&self) -> f32 {
        self.y + self.h / 2.0
    }
    pub fn left_mid(&self) -> P {
        [self.x, self.mid_y()]
    }
    pub fn right_mid(&self) -> P {
        [self.right(), self.mid_y()]
    }
    pub fn bottom_mid(&self) -> P {
        [self.mid_x(), self.bottom()]
    }
}

/// One piece of a way: to its point, straight, round one control point, or round two.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Seg {
    Line(P),
    Quad(P, P),
    Cubic(P, P, P),
}

impl Seg {
    fn end(&self) -> P {
        match *self {
            Seg::Line(p) | Seg::Quad(_, p) | Seg::Cubic(_, _, p) => p,
        }
    }
}

/// Where a wire runs: from its start through its pieces; the direction it arrives in is its last piece's.
#[derive(Clone, Debug, PartialEq)]
pub struct Way {
    start: P,
    segs: Vec<Seg>,
}

impl Way {
    /// Straight through PTS.
    pub fn line(pts: &[P]) -> Way {
        Way { start: pts[0], segs: pts[1..].iter().map(|&p| Seg::Line(p)).collect() }
    }

    /// Through PTS, each corner rounded by up to RADIUS.
    pub fn elbow(pts: &[P], radius: f32) -> Way {
        let mut segs = Vec::new();
        for i in 1..pts.len() {
            if i + 1 == pts.len() {
                segs.push(Seg::Line(pts[i]));
                break;
            }
            let (a, c, b) = (pts[i - 1], pts[i], pts[i + 1]);
            let r = radius.min(dist(a, c) / 2.0).min(dist(c, b) / 2.0);
            let (da, db) = (dir(c, a), dir(c, b));
            segs.push(Seg::Line([c[0] + da[0] * r, c[1] + da[1] * r]));
            segs.push(Seg::Quad(c, [c[0] + db[0] * r, c[1] + db[1] * r]));
        }
        Way { start: pts[0], segs }
    }

    /// From A to B leaving and arriving level (left to right).
    pub fn across(a: P, b: P) -> Way {
        Way { start: a, segs: Vec::new() }.across_to(b)
    }

    /// From A to B leaving and arriving upright.
    pub fn down(a: P, b: P) -> Way {
        let k = ((b[1] - a[1]).abs() * 0.5).max(16.0) * if b[1] >= a[1] { 1.0 } else { -1.0 };
        Way { start: a, segs: vec![Seg::Cubic([a[0], a[1] + k], [b[0], b[1] - k], b)] }
    }

    /// This way, then on to B leaving and arriving level.
    pub fn across_to(mut self, b: P) -> Way {
        let a = self.end();
        let k = ((b[0] - a[0]).abs() * 0.5).max(24.0);
        self.segs.push(Seg::Cubic([a[0] + k, a[1]], [b[0] - k, b[1]], b));
        self
    }

    pub fn end(&self) -> P {
        self.segs.last().map_or(self.start, Seg::end)
    }

    /// The direction it arrives in: from its last piece's last control point (or start) to its end.
    fn arrives(&self) -> P {
        let from = match self.segs.last() {
            Some(Seg::Cubic(_, c, _)) | Some(Seg::Quad(c, _)) => *c,
            _ => self.segs.len().checked_sub(2).map_or(self.start, |i| self.segs[i].end()),
        };
        dir(from, self.end())
    }

    /// This way ending L short of its end, along the way it arrives (an arrow's line stops under its head).
    fn short(&self, l: f32) -> Way {
        let mut w = self.clone();
        let d = self.arrives();
        let back = |p: P| [p[0] - d[0] * l, p[1] - d[1] * l];
        if let Some(last) = w.segs.last_mut() {
            *last = match *last {
                Seg::Line(p) => Seg::Line(back(p)),
                Seg::Quad(c, p) => Seg::Quad(c, back(p)),
                Seg::Cubic(c1, c2, p) => Seg::Cubic(c1, back(c2), back(p)),
            };
        }
        w
    }

    fn path(&self) -> Path {
        let mut p = Path::new().move_to(self.start);
        for s in &self.segs {
            p = match *s {
                Seg::Line(x) => p.line_to(x),
                Seg::Quad(c, x) => p.quad_to(c, x),
                Seg::Cubic(c1, c2, x) => p.cubic_to(c1, c2, x),
            };
        }
        p
    }

    /// The board's points it reaches, for the board's size (its control points bound the curve).
    fn reach(&self) -> impl Iterator<Item = P> + '_ {
        std::iter::once(self.start).chain(self.segs.iter().flat_map(|s| match *s {
            Seg::Line(p) => vec![p],
            Seg::Quad(c, p) => vec![c, p],
            Seg::Cubic(c1, c2, p) => vec![c1, c2, p],
        }))
    }
}

/// An arrow, or a line without a head.
#[derive(Clone, Debug, PartialEq)]
pub struct Wire {
    pub way: Way,
    pub ink: Color,
    pub width: f32,
    pub dash: bool,
    pub head: bool,
}

impl Wire {
    pub fn new(way: Way, ink: Color, width: f32) -> Wire {
        Wire { way, ink, width, dash: false, head: true }
    }
    /// Drawn in dashes: what is held, or still to come.
    pub fn dashed(mut self) -> Wire {
        self.dash = true;
        self
    }
}

/// What a drawing is made of: frames under the arrows, the arrows, and what stands over them.
#[derive(Default)]
pub struct Board {
    pub w: f32,
    pub h: f32,
    under: Vec<El>,
    wires: Vec<Wire>,
    over: Vec<El>,
}

impl Board {
    pub fn new() -> Board {
        Board::default()
    }

    fn grow(&mut self, r: R) {
        self.w = self.w.max(r.right());
        self.h = self.h.max(r.bottom());
    }

    /// EL as a frame at R, under the arrows.
    pub fn frame(&mut self, r: R, el: El) {
        self.grow(r);
        self.under.push(el.absolute(r.x, r.y).size(r.w, r.h));
    }

    /// EL at R, over the arrows.
    pub fn put(&mut self, r: R, el: El) {
        self.grow(r);
        self.over.push(el.absolute(r.x, r.y).size(r.w, r.h));
    }

    /// Words at X, Y (their top left), over the arrows; the rectangle they take.
    pub fn words(&mut self, x: f32, y: f32, words: &str, style: TextStyle, ink: Color) -> R {
        let (w, h) = measure(words, style);
        let r = R::new(x, y, w, h);
        self.grow(r);
        self.over.push(text(words.to_string()).font(style).color(ink).absolute(x, y));
        r
    }

    pub fn wire(&mut self, w: Wire) {
        for p in w.way.reach() {
            self.w = self.w.max(p[0]);
            self.h = self.h.max(p[1]);
        }
        self.wires.push(w);
    }

    /// The board as one element, KEY on the page: its frames, its arrows (the dashed on a canvas of their own, cut by
    /// `Shape::dashed`, their heads whole), what stands over.
    pub fn el(self, key: &str) -> El {
        let (w, h) = (self.w.ceil() + 4.0, self.h.ceil() + 4.0);
        let (mut solid, mut dashed) = (Shape::new(), Shape::new());
        for wire in &self.wires {
            if wire.dash {
                dashed = draw(dashed, wire);
            } else {
                solid = draw(solid, wire);
            }
        }
        let mut c = col().key(key.to_string()).size(w, h).children(self.under).child(rune::canvas(solid).absolute(0.0, 0.0).size(w, h));
        if !dashed.is_empty() {
            c = c.child(rune::canvas(wire::dashed(dashed, true)).absolute(0.0, 0.0).size(w, h));
        }
        c.children(self.over)
    }
}

/// WORDS' width and height in STYLE: the window's one measure of a text (the board's words, a match's arms).
pub fn measure(words: &str, style: TextStyle) -> (f32, f32) {
    rune::text::measure(words, &[], style, None)
}

/// WIRE added to S: its line, stopped under its head, and its head (`wire::head`, a fill on the shape: `drawn`'s doc,
/// it lands as its line arrives).
fn draw(s: Shape, w: &Wire) -> Shape {
    if !w.head {
        return s.stroke_path(&w.way.path(), w.width, LineCap::Butt, w.ink);
    }
    let s = s.stroke_path(&w.way.short(wire::head_len(w.width) * 0.7).path(), w.width, LineCap::Butt, w.ink);
    wire::head(s, w.way.end(), w.way.arrives(), w.width, w.ink)
}

fn dist(a: P, b: P) -> f32 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
}

fn dir(a: P, b: P) -> P {
    let d = dist(a, b).max(0.0001);
    [(b[0] - a[0]) / d, (b[1] - a[1]) / d]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_way_arrives_along_its_last_piece_and_an_arrow_stops_under_its_head() {
        let w = Way::line(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]);
        assert_eq!((w.end(), w.arrives()), ([10.0, 10.0], [0.0, 1.0]));
        assert_eq!(w.short(4.0).end(), [10.0, 6.0]);
        // a curve across leaves and arrives level, and keeps arriving level when stopped short
        let c = Way::across([0.0, 0.0], [100.0, 50.0]);
        assert_eq!((c.end(), c.arrives()), ([100.0, 50.0], [1.0, 0.0]));
        assert_eq!(c.short(5.0).end(), [95.0, 50.0]);
        assert_eq!(c.short(5.0).arrives(), [1.0, 0.0]);
        // a rounded corner arrives along the way out of it
        let e = Way::elbow(&[[0.0, 0.0], [0.0, 20.0], [30.0, 20.0]], 6.0);
        assert_eq!((e.end(), e.arrives()), ([30.0, 20.0], [1.0, 0.0]));
        assert_eq!(e.segs[1], Seg::Quad([0.0, 20.0], [6.0, 20.0]));
    }

    #[test]
    fn an_arrow_is_its_line_stopped_under_rune_s_arrowhead_and_a_dashed_one_keeps_its_head_whole() {
        use rune::shape::arrowhead;
        let w = Wire::new(Way::line(&[[0.0, 10.0], [40.0, 10.0]]), Color::WHITE, 2.0);
        let l = wire::head_len(2.0);
        assert_eq!(draw(Shape::new(), &w), Shape::new().stroke_path(&Way::line(&[[0.0, 10.0], [40.0 - l * 0.7, 10.0]]).path(), 2.0, LineCap::Butt, Color::WHITE).fill(arrowhead([40.0, 10.0], [1.0, 0.0], l, l * 0.9), Color::WHITE));
        let mut b = Board::new();
        b.wire(w.clone().dashed());
        assert_eq!((b.w, b.h), (40.0, 10.0));
    }
}
