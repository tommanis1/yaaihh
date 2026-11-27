//! What of the Map is drawn, and how much of each (the study, N-1479 p8): each node at the detail its size on screen
//! allows (its side: the square root of its area there, so a post, wide and low in its column, reads by what it holds), as a map draws a kind of feature only between its least and its most zoom; what lies out of view not at all
//! (a node out of view is never visited, nor what is inside it); labels put down in rank order where they meet no
//! label put down before, a label shown in the last frame first, so they hold still as he moves; an edge into a node
//! not open rolled up into one edge to it. Every fade is a function of the size alone, not of a clock: the change is
//! in place, follows his hand, and is the same in a still and a test. (Only a flight moves by the clock: camera.rs.)

use super::camera::Camera;
use super::{Graph, Kind, Link};
use std::collections::{HashMap, HashSet};

/// How a node is drawn, coarse to fine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Look {
    /// A dot at its middle.
    Dot,
    /// Its label beside the dot, its outline faint.
    Label,
    /// A card: its label, what it is now, its marks.
    Card,
    /// Open: a region with its name at its head and what is inside it drawn within.
    Open,
}

/// The sides on screen, in points, from which a kind takes each look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub dot: f32,
    pub label: f32,
    pub card: f32,
    pub open: f32,
}

/// Across how much of a zoom a look fades in: from its width to FADE times it.
pub const FADE: f32 = 1.6;

/// Kind K's band. A project and a topic open as a fit flies into them, so what they hold comes up whole; a thread
/// when it could hold a dozen labels; a post when its first lines could be read (p715's levels 1 to 4).
pub fn band(k: Kind) -> Band {
    let never = f32::INFINITY;
    match k {
        // a project and a topic open whole by 448 points (280 times FADE): the side a double-click's fit gives them,
        // the inspector open, from a window of 1200 by 800 (proto-window p38 small 3)
        Kind::Project => Band { dot: 2.0, label: 4.0, card: 140.0, open: 280.0 },
        Kind::Topic => Band { dot: 2.0, label: 36.0, card: 150.0, open: 280.0 },
        Kind::Item | Kind::Thread | Kind::Handler => Band { dot: 2.0, label: 40.0, card: 170.0, open: 420.0 },
        Kind::Agent | Kind::Build | Kind::Account => Band { dot: 1.0, label: 28.0, card: 120.0, open: never },
        Kind::Post | Kind::Run | Kind::Brain | Kind::Node(_) => Band { dot: 1.0, label: 60.0, card: 220.0, open: 560.0 },
        Kind::Step => Band { dot: 2.0, label: 48.0, card: 180.0, open: 440.0 },
        Kind::Call => Band { dot: 2.0, label: 48.0, card: 200.0, open: never },
        Kind::Turn | Kind::Text => Band { dot: 2.0, label: 48.0, card: 180.0, open: never },
    }
}

/// Node N's band: its kind's, opening where the node says it opens ([`super::Node::opens`]).
pub fn band_of(n: &super::Node) -> Band {
    let b = band(n.kind);
    Band { open: n.opens.unwrap_or(b.open), ..b }
}

/// How far W is into the fade that starts at FROM: 0 below it, 1 from FADE times it, smooth between.
pub fn fade(w: f32, from: f32) -> f32 {
    if !(from.is_finite()) || w <= from {
        return 0.0;
    }
    let t = ((w / from).ln() / FADE.ln()).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A node as this frame draws it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shown {
    pub i: u32,
    /// Its rectangle on screen, in the view's points.
    pub r: [f32; 4],
    pub look: Look,
    /// How much of it is drawn: its look's fade, times its parent's opening.
    pub alpha: f32,
    /// How far it is open: what is inside it is drawn this much.
    pub open: f32,
    /// A lit point: his, drawn as a dot though what holds it is not open.
    pub lit: bool,
    /// How many it stands for, itself among them: lit points that would meet it, nearer than [`LIT_APART`] points;
    /// the children of a group ([`Shown::group`]); or, for a card drawn as itself, its siblings too small for their
    /// words whose group's card would meet it ([`visible_with`]). 1 for a node standing for itself alone.
    pub many: u32,
    /// A group: one card, [`GROUP`] on screen at the middle of the first of them, kept inside the room its node lays
    /// them in ([`kept_in`]; proto-window N-1446 p1951), for MANY children of one node, of one kind, each too small on screen for its words ([`super::Node::reads`]). The first, its node `i`, is the one
    /// the label order puts first, his ask before the rest and then the heaviest, so the card names it and how many
    /// more ("Phone link, debug mo… and 4 more threads": paper-research N-1446 p1715, 3; p1677, 1); an edge to any of
    /// them ends at it.
    pub group: Option<Group>,
}

/// What a group's card says beside how many it stands for ([`Shown::many`]) and their kind (its node's).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Group {
    /// The least and the most weight among them (a thread's of the cloud: his posts in it; his post's: its links).
    pub weight: [f32; 2],
}

/// The least a card of the cloud is drawn at on screen, wide and high, in points, for [`super::Node::reads`]: a line
/// of its words, about nine letters of the small type (12 points a line), and its padding (cloud.rs `card` draws no
/// word under 44 wide or 20 high).
pub const READS: [f32; 2] = [72.0, 26.0];
/// A group's card on screen, wide and high, in points ([`Shown::group`]): "69 threads" and "of one post" on two
/// lines of the small type. Under three times as wide as high, so not a row: its edges end at its middle
/// ([`dot_at`]).
pub const GROUP: [f32; 2] = [140.0, 48.0];
/// The most children of a node that come in by the ladder ([`visible_with`]): the group chat's few most-linked posts
/// (proto-window N-1446 p1683).
pub const LADDER: usize = 5;
/// How many of them come in at once, the first (paper-research N-1446 p1715, 1: the ladder starts with 3 posts).
pub const LADDER_FIRST: usize = 3;
/// The least a child that comes in by the ladder is drawn at on screen, wide and high, in points, while what holds it
/// is a card, not open: four lines of its words in the small type, about thirty letters each, and its note (p1715, 1:
/// "each at a size whose words read, not 72 by 26"). His post's card at five eighths.
pub const LADDER_CARD: [f32; 2] = [200.0, 75.0];

/// The top of a card, where its name and a region's badge go (8 of padding, a name's line, a badge's), in points: no
/// lit point is drawn over it.
pub const CARD_HEAD: f32 = 52.0;
/// The height on screen, in points, a card that holds cards is cut to while it holds none, where its words are not
/// measured ([`visible_worded`]): the most a thread's words take in cloud.rs `card`, 4 above, two lines of its title's
/// type (body_medium, 21 a line), 2, its note's line (small, 12), 4 below. 60 or more, so its title keeps its
/// neighbours' type (proto-window N-1446 p1727).
pub const WORDS_HIGH: f32 = 64.0;
/// A lit point's side, in points, and the most it grows by standing for others.
pub const LIT: f32 = 8.0;
pub const LIT_GROW: f32 = 6.0;
/// The most a lit point reaches from its middle, in points (the largest, standing for many).
pub const LIT_REACH: f32 = (LIT + LIT_GROW) / 2.0;
/// How near two lit points of one kind may come, middle to middle, in points: two of the largest and 2 between. A
/// point nearer one already drawn is not drawn; the nearest stands for it, so a thread of hundreds of his posts far
/// out is a few points, not a bar, and no two points touch, whatever cell of a grid they fall in (proto-window
/// N-1479 p48: two points of neighbouring cells 6 apart read as an 8). The largest, so a point growing by those it
/// stands for never grows into another.
pub const LIT_APART: f32 = 2.0 * LIT_REACH + 2.0;

/// The side of a lit point standing for MANY (itself among them).
pub fn lit_side(many: u32) -> f32 {
    LIT + (many as f32).log2().min(LIT_GROW)
}

/// Where a node on screen at R has its dot: at its middle, or for a row much wider than it is high (a post on its
/// thread), at its left, its label beside it.
pub fn dot_at(r: &[f32; 4]) -> [f32; 2] {
    if is_row(r) { [r[0] + 8.0, r[1] + r[3] / 2.0] } else { [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0] }
}

/// Whether R is a row: at least three times as wide as it is high, and wide enough for its dot at the left.
pub fn is_row(r: &[f32; 4]) -> bool {
    r[2] >= 3.0 * r[3] && r[2] >= 24.0
}

/// The nodes of G that a view of VIEW points through CAM draws, parents before children: each that meets the view and
/// is wide enough for a dot, inside a node open enough to show it; and LIT's (his posts, p1's far level; the agents,
/// p715: "who's working or stopped" without a click) as dots where they are in view and what holds them is not open,
/// so they show from the farthest zoom.
pub fn visible(g: &Graph, cam: &Camera, view: [f32; 2], lit: &[u32]) -> Vec<Shown> {
    visible_with(g, cam, view, lit).0
}

/// [`visible`], and for each node not drawn as itself that another stands for (a lit point merged into another's, a
/// child too small for its words), the index in the first of the one that stands for it (so an edge to it ends
/// there: [`edges`]).
///
/// The children that say how large their words read ([`super::Node::reads`]) are drawn no smaller, at any zoom: no box
/// without words (paper-research N-1446 p1677, 1). Two rules:
/// - **The ladder** (`ladder`; proto-window p1683: the group chat's card shows its few most-linked posts before it
///   opens): a node shown at its card's side brings in its heaviest such children, in the label order, the first
///   [`LADDER_FIRST`] at once, then one more each time its side grows by [`FADE`], at most [`LADDER`], open or not.
///   Each is drawn about its middle, moved inside the node under its head ([`CARD_HEAD`]), meeting no other that came
///   in so: while the node is a card at least at [`LADDER_CARD`], so its words read; as the node opens, down to its
///   own size, never under its `reads`. A card that holds cards ([`holds_cards`]) and shows none of them is drawn only
///   its words' height ([`WORDS_HIGH`], or as [`visible_worded`] is told), so it is no larger than what it says, and
///   edges end at it so: not open, the ladder bringing none in; or drawn larger than itself, by its parent's ladder or
///   alone at its `reads`, which shows none (paper-research N-1446 p1726, p1739).
/// - **Groups** (`gather`): once the node opens, its other such children too small for their words are drawn as
///   groups, in the label order, each joining the first group of its kind whose card its own would meet ([`GROUP`] at
///   its middle, kept inside the node's room for them, [`kept_in`]: so no two groups' cards meet, and none lies past
///   its node's edge, p1951). One whose card would meet only a sibling drawn as itself, of its kind,
///   is stood for by it ([`Shown::many`]); a group of one is that child alone, at its `reads`. Those that read are
///   drawn as themselves.
///
/// A root is drawn as before, as are children that say nothing of their words (the Map's).
pub fn visible_with(g: &Graph, cam: &Camera, view: [f32; 2], lit: &[u32]) -> (Vec<Shown>, HashMap<u32, usize>) {
    visible_worded(g, cam, view, lit, &|_, _| WORDS_HIGH)
}

/// [`visible_with`], a card that holds cards and shows none cut to the height WORDS gives for its node and its width
/// on screen: its name's and its note's, measured where they are drawn, as lod.rs has no type to measure with (as
/// [`labels`] is given their widths; proto-window N-1446 p1727).
pub fn visible_worded(g: &Graph, cam: &Camera, view: [f32; 2], lit: &[u32], words: &dyn Fn(u32, f32) -> f32) -> (Vec<Shown>, HashMap<u32, usize>) {
    visible_at(g, cam, view, lit, words, None)
}

/// [`visible_worded`] at moment AT (seconds; None: now, the time bar's right end): only what was there by then
/// ([`Graph::there`]) is drawn, each where it lies now, so neighbours keep their places and what came later leaves its
/// room empty. Asked of what the walk reaches, its nodes and their children, so its cost is the view's, not the graph's.
pub fn visible_at(g: &Graph, cam: &Camera, view: [f32; 2], lit: &[u32], words: &dyn Fn(u32, f32) -> f32, at: Option<i64>) -> (Vec<Shown>, HashMap<u32, usize>) {
    let seen = cam.seen(view);
    // under a card that packs what it shows ([`packs`]) a node is drawn at its own x and no lower than it was laid, so
    // one laid below the view may be drawn in it: there only what lies beside or above the view is left out (p1889's
    // merge: a flight into a session in his post's band fitted it where it is drawn, packed up, and the session, laid
    // below that view, was not drawn there)
    let below = super::Rect::new(seen.x, seen.y, seen.w, f64::INFINITY);
    let mut out = Vec::new();
    let mut stand: HashMap<u32, usize> = HashMap::new();
    // the children drawn by another (a group's, or a card standing for them), never as lit points; and those stood
    // for by a sibling drawn as itself, by its node, as that is not drawn yet
    let mut gathered: HashSet<u32> = HashSet::new();
    let mut by_sibling: Vec<(u32, u32)> = Vec::new();
    // a node, how much of it is drawn, where when that is not its own rectangle (come in by the ladder, grown), and
    // whether a card over it packs what it shows
    let mut stack: Vec<(u32, f32, Option<[f32; 4]>, bool)> = g.roots().map(|i| (i, 1.0, None, false)).collect();
    stack.reverse();
    while let Some((i, parent, grown, up)) = stack.pop() {
        let n = g.node(i);
        if !g.there(i, at) || !n.rect.meets(if up { &below } else { &seen }) {
            continue;
        }
        let under = up || packs(n);
        let r = cam.on_screen(&n.rect);
        let b = band_of(n);
        let w = side(&r);
        if w < b.dot && grown.is_none() {
            continue;
        }
        let look = if w >= b.open {
            Look::Open
        } else if w >= b.card {
            Look::Card
        } else if w >= b.label {
            Look::Label
        } else {
            Look::Dot
        };
        let open = fade(w, b.open);
        // drawn larger than itself, at the least its words read at, it is a card
        let look = if grown.is_some() { look.max(Look::Card) } else { look };
        out.push(Shown { i, r: grown.unwrap_or(r), look, alpha: parent, open, lit: false, many: 1, group: None });
        // what it holds shows once it is drawn at its own size: laid in its own rectangle, it would not lie in the one
        // drawn; holding cards, it is its words' height, as one holding none yet (p1739)
        if grown.is_some() {
            cut_to_words(g, out.last_mut().unwrap(), words);
            continue;
        }
        let all = g.children(i);
        let then: Vec<u32>;
        let kids: &[u32] = if at.is_some() {
            then = all.iter().copied().filter(|k| g.there(*k, at)).collect();
            &then
        } else {
            all
        };
        if kids.iter().all(|k| g.node(*k).reads.is_none()) {
            // a card of words holding none, at its own size: its words' height (p1796, 1a); the Map's say nothing of
            // how large their words read, so stay
            if kids.is_empty() && n.reads.is_some() {
                cut_to_words(g, out.last_mut().unwrap(), words);
            }
            if open > 0.0 {
                stack.extend(kids.iter().rev().map(|k| (*k, parent * open, None, under)));
            }
            continue;
        }
        let came = ladder(g, cam, &seen, &r, w, b.card, open, kids);
        // a card that holds cards, holding none yet, takes only its words' height: its name and note, as its
        // neighbours; it grows to what it holds once the ladder's first fits (paper-research N-1446 p1726)
        if came.is_empty() && open <= 0.0 {
            cut_to_words(g, out.last_mut().unwrap(), words);
        }
        // each child: come in by the ladder, drawn as itself, too small (in a group), or not (the node not open)
        let (mut next, mut small, mut placed) = (Vec::new(), Vec::new(), Vec::new());
        for k in kids {
            let c = g.node(*k);
            if let Some((_, at, come)) = came.iter().find(|(p, _, _)| p == k) {
                next.push((*k, parent * come.max(open), *at, under));
                placed.push((*k, at.unwrap_or_else(|| cam.on_screen(&c.rect))));
                continue;
            }
            if open <= 0.0 {
                continue;
            }
            match c.reads {
                Some(m) if c.rect.meets(if under { &below } else { &seen }) => {
                    let at = cam.on_screen(&c.rect);
                    if reads_at(&at, m) {
                        next.push((*k, parent * open, None, under));
                        placed.push((*k, at));
                    } else {
                        small.push(*k);
                    }
                }
                _ => next.push((*k, parent * open, None, under)),
            }
        }
        // the room it lays its children in: inside its card, from the first's top down, and in a card that packs what it
        // shows ([`packs`]) by their own margin, the first's left from its left (the gap `fit_to_shown` keeps; a card
        // holding cards has none), so a group's card lies there as a card drawn as itself does, never over its words nor
        // past its edge (proto-window N-1446 p1951: a band's "18 jobs · 4 failed", 241 points wide, drew four groups'
        // cards of 140 side by side, 43 points past its edge over "2 posts"; paper-research p1994: kept flush on its
        // left and bottom edges, the same inset as a plain card's)
        let (left, top) = kids.iter().map(|k| cam.on_screen(&g.node(*k).rect)).fold((r[0] + r[2], r[1] + r[3]), |(x, y), q| (x.min(q[0]), y.min(q[1])));
        let (inset, top) = (if packs(n) { (left - r[0]).max(0.0) } else { 0.0 }, top.max(r[1]));
        let room = [r[0] + inset, top, (r[2] - 2.0 * inset).max(0.0), (r[1] + r[3] - inset - top).max(0.0)];
        for (k, stood) in gather(g, cam, &small, &placed, &room) {
            match stood {
                Stood::By(s) => by_sibling.push((k, s)),
                Stood::Group(members) => {
                    let c = g.node(k);
                    let own = cam.on_screen(&c.rect);
                    let (r, group) = if members.len() == 1 {
                        let m = c.reads.unwrap_or(READS);
                        (about(mid(&own), [own[2].max(m[0]), own[3].max(m[1])]), None)
                    } else {
                        let weight = |f: fn(f32, f32) -> f32, from: f32| members.iter().map(|x| g.node(*x).weight).fold(from, f);
                        (about(mid(&own), GROUP), Some(Group { weight: [weight(f32::min, f32::INFINITY), weight(f32::max, f32::NEG_INFINITY)] }))
                    };
                    let r = kept_in(r, &room);
                    for x in &members[1..] {
                        stand.insert(*x, out.len());
                    }
                    gathered.extend(members.iter().copied());
                    out.push(Shown { i: k, r, look: Look::Card, alpha: parent * open, open: 0.0, lit: false, many: members.len() as u32, group });
                    // alone at its reads it shows nothing it holds, as one grown (p1739)
                    if members.len() == 1 {
                        cut_to_words(g, out.last_mut().unwrap(), words);
                    }
                }
            }
        }
        stack.extend(next.into_iter().rev());
    }
    let at: HashMap<u32, usize> = out.iter().enumerate().map(|(k, s)| (s.i, k)).collect();
    for (k, s) in by_sibling {
        if let Some(&x) = at.get(&s) {
            out[x].many += 1;
            stand.insert(k, x);
            gathered.insert(k);
        }
    }
    let drawn: HashSet<u32> = out.iter().filter(|s| s.alpha > 0.0).map(|s| s.i).chain(gathered).collect();
    // his lit points, none nearer another of its kind than LIT_APART: the first in the label order stands for those
    // that would meet it
    let want = rank(g, lit.iter().copied().filter(|i| !drawn.contains(i) && g.node(*i).rect.meets(&seen)));
    // kept by kind, in a grid of LIT_APART (so any point near enough is in the 3 by 3 around): an agent never stands
    // for his post, nor his post for an agent
    let mut cells: HashMap<(Kind, i32, i32), Vec<usize>> = HashMap::new();
    for i in want {
        let mut r = cam.on_screen(&g.node(i).rect);
        let mut d = dot_at(&r);
        let mut up = g.node(i).parent;
        let holder = loop {
            match up {
                Some(p) if at.contains_key(&p) => break Some(&out[at[&p]]),
                Some(p) => up = g.node(p).parent,
                None => break None,
            }
        };
        let kind = g.node(i).kind;
        // over the head of the card it is drawn on: his post is not drawn, it shows once that opens; an agent is drawn
        // just under the head, as the agents on a topic stand in a row along it (layout.rs `DOTS_Y`) and would else
        // show only once it opens, a held one's edge drawn from the topic (path 2)
        if let Some(h) = holder.filter(|h| h.look == Look::Card && over_head(&h.r, d)) {
            let under = h.r[1] + CARD_HEAD + LIT_REACH;
            if kind != Kind::Agent || under + LIT_REACH > h.r[1] + h.r[3] {
                continue;
            }
            r[1] += under - d[1];
            d[1] = under;
        }
        let (cx, cy) = ((d[0] / LIT_APART).floor() as i32, (d[1] / LIT_APART).floor() as i32);
        let near = (cx - 1..=cx + 1)
            .flat_map(|x| (cy - 1..=cy + 1).map(move |y| (kind, x, y)))
            .filter_map(|c| cells.get(&c))
            .flatten()
            .map(|k| (*k, dist(dot_at(&out[*k].r), d)))
            .filter(|(_, e)| *e < LIT_APART)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match near {
            Some((k, _)) => {
                out[k].many += 1;
                stand.insert(i, k);
            }
            None => {
                cells.entry((kind, cx, cy)).or_default().push(out.len());
                out.push(Shown { i, r, look: Look::Dot, alpha: 1.0, open: 0.0, lit: true, many: 1, group: None });
            }
        }
    }
    (out, stand)
}

/// Whether a node of kind K is drawn as its name and note over what it holds (a project, a topic, an item, a thread),
/// not as words of its own (a post, a run, a step, a call).
fn holds_cards(k: Kind) -> bool {
    matches!(k, Kind::Project | Kind::Topic | Kind::Item | Kind::Thread)
}

/// Whether [`fit_to_shown`] packs what node N shows up under its words: a card of words of its own that says how large
/// they read (the cloud's), not one holding cards ([`holds_cards`]) nor the Map's, which say nothing of their words.
fn packs(n: &super::Node) -> bool {
    n.reads.is_some() && !holds_cards(n.kind)
}

/// SHOWN cut to its words' height, the one WORDS gives for its node and its width on screen, where that node holds
/// cards ([`holds_cards`]) or holds none: drawn showing none of them, or a card of words alone, it is no larger than
/// what it says (paper-research N-1446 p1726, p1739; p1796, 1a: the agents his p815 reached, two lines over the top
/// half of his post's band in still 4). A card holding others it shows stays.
fn cut_to_words(g: &Graph, shown: &mut Shown, words: &dyn Fn(u32, f32) -> f32) {
    if holds_cards(g.node(shown.i).kind) || g.children(shown.i).is_empty() {
        shown.r[3] = shown.r[3].min(words(shown.i, shown.r[2]));
    }
}

/// The cloud's cards of words in OUT ([`visible_worded`]'s, with STAND, drawn through CAM: those that say how large
/// their words read, [`super::Node::reads`], and hold no cards, [`holds_cards`], whose places lay what they hold) drawn
/// no larger than what they show: a card's words' height, as WORDS gives it for its node and its width on screen, and
/// under it what it shows of what it holds, each moved up to the gap it has at its sides under its words, or under the
/// lowest of those above it in its column, the card ending that gap under the lowest; showing none of it, its words'
/// height. Open, it keeps room for a child out of the view, where that lies (not drawn, nor stood for), so it is not
/// cut above what a pan brings in. By column, not by row: his post's band kept an empty row between "reached 21" and
/// the requests under it, the run beside it taller (paper-research N-1446 p1883, 1). The deepest first, so a
/// card holding one so cut is cut to it. Still 6 of his p815: each answer's card, laid with three quarters of it for
/// the turn it was written in, was four times the turn's card, cut to its words, and the bundle and his post holding
/// them kept that room (paper-research N-1446 p1839, 1). At every look, as the cloud draws each as a card (cloud.rs
/// `card`) and [`cut_to_words`] cuts it: an answer opens at a side under a card's, so closed it is a label, and was
/// drawn as laid for its turn. Lit points, a group's card, a card of the Map's: as they were.
pub fn fit_to_shown(g: &Graph, cam: &Camera, out: &mut [Shown], stand: &HashMap<u32, usize>, words: &dyn Fn(u32, f32) -> f32) {
    let drawn: HashSet<u32> = out.iter().map(|x| x.i).collect();
    for p in (0..out.len()).rev() {
        let (i, r) = (out[p].i, out[p].r);
        let n = g.node(i);
        if out[p].lit || out[p].group.is_some() || !packs(n) {
            continue;
        }
        // what it shows: drawn after it, parents before children, each node's together
        let inside = |j: usize| {
            let mut up = g.node(out[j].i).parent;
            while let Some(u) = up {
                if u == i {
                    return true;
                }
                up = g.node(u).parent;
            }
            false
        };
        let end = (p + 1..out.len()).find(|j| out[*j].lit || !inside(*j)).unwrap_or(out.len());
        let said = words(i, r[2]).min(r[3]);
        let kids: Vec<[f32; 4]> = (p + 1..end).filter(|j| g.node(out[*j].i).parent == Some(i)).map(|j| out[j].r).collect();
        let away: Vec<[f32; 4]> = if out[p].open > 0.0 {
            g.children(i).iter().filter(|c| !drawn.contains(*c) && !stand.contains_key(*c)).map(|c| cam.on_screen(&g.node(*c).rect)).collect()
        } else {
            Vec::new()
        };
        if kids.is_empty() && away.is_empty() {
            out[p].r[3] = said;
            continue;
        }
        let gap = kids.iter().chain(&away).map(|k| k[0] - r[0]).fold(f32::INFINITY, f32::min).max(0.0);
        // each child drawn with what it shows (from it to the next child), each out of the view as its room alone; by
        // its top, then its left
        let starts: Vec<usize> = (p + 1..end).filter(|j| g.node(out[*j].i).parent == Some(i)).collect();
        let mut each: Vec<([f32; 4], Option<(usize, usize)>)> = starts.iter().enumerate().map(|(k, j)| (out[*j].r, Some((*j, starts.get(k + 1).copied().unwrap_or(end))))).collect();
        each.extend(away.iter().map(|k| (*k, None)));
        each.sort_by(|a, b| (a.0[1], a.0[0]).partial_cmp(&(b.0[1], b.0[0])).unwrap_or(std::cmp::Ordering::Equal));
        let mut placed: Vec<[f32; 4]> = Vec::new();
        for (q, range) in each {
            let column = |a: &&[f32; 4]| a[0] < q[0] + q[2] - 0.5 && q[0] < a[0] + a[2] - 0.5;
            let to = placed.iter().filter(column).map(|a| a[1] + a[3] + gap).fold(r[1] + said + gap, f32::max);
            let up = (q[1] - to).max(0.0);
            if let Some((a, b)) = range {
                for x in &mut out[a..b] {
                    x.r[1] -= up;
                }
            }
            placed.push([q[0], q[1] - up, q[2], q[3]]);
        }
        let low = out[p + 1..end].iter().map(|x| x.r[1] + x.r[3]).chain(placed.iter().map(|k| k[1] + k[3])).fold(f32::NEG_INFINITY, f32::max);
        out[p].r[3] = r[3].min(low + gap - r[1]).max(said);
    }
}

/// How far apart two points on screen are.
fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// NODES in the label order of lit points, the ladder and groups: by tier (his asks first), the heavier, the first.
fn rank(g: &Graph, nodes: impl Iterator<Item = u32>) -> Vec<u32> {
    let mut v: Vec<u32> = nodes.collect();
    v.sort_by_key(|i| {
        let n = g.node(*i);
        (n.tier, std::cmp::Reverse(ordered(n.weight)), *i)
    });
    v
}

/// The ladder ([`visible_with`]): of KIDS, the children of a node shown at R with side W, its card's side CARD, OPEN
/// this far, those that come in by it, open or not. Of those that say how large their words read and meet SEEN, in
/// the label order ([`rank`]): the first [`LADDER_FIRST`] from CARD, each next from [`FADE`] times the last's, at most
/// [`LADDER`]. Each is drawn at a size from at least [`LADDER_CARD`] (the node not open) to its own, never under its
/// `reads` (the node open), by how far the node is open: at its own rectangle where that is the size (none), else
/// about its middle moved inside R under its head ([`CARD_HEAD`]). One is passed over where that does not fit or meets
/// one come in before. For each: where it is drawn, and how far it has come in.
fn ladder(g: &Graph, cam: &Camera, seen: &super::Rect, r: &[f32; 4], w: f32, card: f32, open: f32, kids: &[u32]) -> Vec<(u32, Option<[f32; 4]>, f32)> {
    let room = [r[0], r[1] + CARD_HEAD, r[2], r[3] - CARD_HEAD];
    let mut out: Vec<(u32, Option<[f32; 4]>, f32)> = Vec::new();
    let mut taken: Vec<[f32; 4]> = Vec::new();
    for k in rank(g, kids.iter().copied().filter(|k| g.node(*k).reads.is_some())) {
        let from = card * FADE.powi(out.len().saturating_sub(LADDER_FIRST - 1) as i32);
        if out.len() == LADDER || w <= from {
            break;
        }
        let n = g.node(k);
        let Some(m) = n.reads.filter(|_| n.rect.meets(seen)) else { continue };
        let own = cam.on_screen(&n.rect);
        let size = [0, 1].map(|d| {
            let (shut, opened) = (own[2 + d].max(m[d]).max(LADDER_CARD[d]), own[2 + d].max(m[d]));
            if open >= 1.0 { opened } else { shut + (opened - shut) * open }
        });
        let at = if size == [own[2], own[3]] {
            None
        } else {
            let Some(at) = moved_into(about(mid(&own), size), &room) else { continue };
            Some(at)
        };
        let drawn = at.unwrap_or(own);
        if taken.iter().any(|t| overlap(t, &drawn)) {
            continue;
        }
        taken.push(drawn);
        out.push((k, at, fade(w, from)));
    }
    out
}

/// How a child too small for its words is drawn ([`gather`]).
enum Stood {
    /// By a sibling drawn as itself.
    By(u32),
    /// As the first of a group: these, itself first.
    Group(Vec<u32>),
}

/// By what each of SMALL, children of one node too small for their words, is drawn ([`visible_with`]'s groups). In
/// the label order ([`rank`]), each joins the nearest group of its kind whose card its own would meet ([`GROUP`] at
/// its middle, kept in ROOM, the room their node lays them in: [`kept_in`]; p1951); else it is stood for by the nearest of PLACED, its siblings drawn as themselves (and where), of its
/// kind, that its card would meet; else it starts a group. So no two groups' cards of one kind meet, nor a group's a
/// sibling's of its kind.
fn gather(g: &Graph, cam: &Camera, small: &[u32], placed: &[(u32, [f32; 4])], room: &[f32; 4]) -> Vec<(u32, Stood)> {
    let mut groups: Vec<(u32, [f32; 4], Vec<u32>)> = Vec::new();
    // groups by kind, in a grid of GROUP (so any whose card meets one is in the 3 by 3 around)
    let mut cells: HashMap<(Kind, i32, i32), Vec<usize>> = HashMap::new();
    let mut out: Vec<(u32, Stood)> = Vec::new();
    for k in rank(g, small.iter().copied()) {
        let kind = g.node(k).kind;
        // its card as a group's is drawn: GROUP at its middle, kept in ROOM
        let card = kept_in(about(mid(&cam.on_screen(&g.node(k).rect)), GROUP), room);
        let m = mid(&card);
        let (cx, cy) = ((m[0] / GROUP[0]).floor() as i32, (m[1] / GROUP[1]).floor() as i32);
        let near = (cx - 1..=cx + 1)
            .flat_map(|x| (cy - 1..=cy + 1).map(move |y| (kind, x, y)))
            .filter_map(|c| cells.get(&c))
            .flatten()
            .filter(|j| overlap(&groups[**j].1, &card))
            .map(|j| (*j, dist(mid(&groups[*j].1), m)))
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        if let Some((j, _)) = near {
            groups[j].2.push(k);
            continue;
        }
        let by = placed.iter().filter(|(s, r)| g.node(*s).kind == kind && overlap(r, &card)).map(|(s, r)| (*s, to_rect(m, r))).min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((s, _)) = by {
            out.push((k, Stood::By(s)));
            continue;
        }
        cells.entry((kind, cx, cy)).or_default().push(groups.len());
        groups.push((k, card, vec![k]));
    }
    out.extend(groups.into_iter().map(|(k, _, v)| (k, Stood::Group(v))));
    out
}

/// R's middle, on screen.
fn mid(r: &[f32; 4]) -> [f32; 2] {
    [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0]
}

/// A rectangle SIZE wide and high on screen about M.
fn about(m: [f32; 2], size: [f32; 2]) -> [f32; 4] {
    [m[0] - size[0] / 2.0, m[1] - size[1] / 2.0, size[0], size[1]]
}

/// R moved inside ROOM, on screen; none where ROOM cannot hold it.
fn moved_into(r: [f32; 4], room: &[f32; 4]) -> Option<[f32; 4]> {
    if r[2] > room[2] || r[3] > room[3] {
        return None;
    }
    Some([r[0].clamp(room[0], room[0] + room[2] - r[2]), r[1].clamp(room[1], room[1] + room[3] - r[3]), r[2], r[3]])
}

/// R cut to ROOM where it is wider or higher, about its middle, then moved inside it ([`moved_into`]): a group's card
/// in the room its node lays its children in, never past the node's edge (proto-window N-1446 p1951).
fn kept_in(r: [f32; 4], room: &[f32; 4]) -> [f32; 4] {
    let cut = about(mid(&r), [r[2].min(room[2].max(0.0)), r[3].min(room[3].max(0.0))]);
    moved_into(cut, room).unwrap_or(cut)
}

/// Whether R, on screen, is at least READS wide and high: its words read there ([`super::Node::reads`]).
fn reads_at(r: &[f32; 4], reads: [f32; 2]) -> bool {
    r[2] >= reads[0] && r[3] >= reads[1]
}

/// How far point M is from rectangle R on screen: 0 inside it.
fn to_rect(m: [f32; 2], r: &[f32; 4]) -> f32 {
    let dx = (r[0] - m[0]).max(m[0] - (r[0] + r[2])).max(0.0);
    let dy = (r[1] - m[1]).max(m[1] - (r[1] + r[3])).max(0.0);
    dx.hypot(dy)
}

/// Whether a lit point at D would cover any of the head of a card at R: its name or its badge.
pub fn over_head(r: &[f32; 4], d: [f32; 2]) -> bool {
    d[1] - LIT_REACH < r[1] + CARD_HEAD
}

/// A rectangle's side on screen: the square root of its area.
pub fn side(r: &[f32; 4]) -> f32 {
    (r[2].max(0.0) * r[3].max(0.0)).sqrt()
}

/// The label order (p715): his asks, what waits on him, stopped or held, the rest; within a tier a label shown in the
/// last frame first (so labels hold still), then the heavier, then the nearer the root.
fn order(g: &Graph, shown: &[Shown], kept: &HashSet<u32>) -> Vec<usize> {
    let mut o: Vec<usize> = (0..shown.len()).collect();
    let key = |s: &Shown| {
        let n = g.node(s.i);
        (n.tier, !kept.contains(&s.i), std::cmp::Reverse(ordered(n.weight)), n.kind)
    };
    o.sort_by_key(|&k| key(&shown[k]));
    o
}

fn ordered(w: f32) -> u32 {
    (w.max(0.0) * 16.0).min(u32::MAX as f32) as u32
}

/// The size of a grid cell labels are kept in, in points.
const CELL: f32 = 64.0;

/// Where the labels of SHOWN go, those that meet no label put down before them, in the label order: for each, its
/// index in SHOWN and its box on screen. Nodes at Label take one beside their dot; a card or an open node carries its
/// label inside itself and takes none here (siblings do not overlap). WIDTH gives a label's width in points; HIGH is
/// its line's height. KEPT is the nodes labelled in the last frame.
pub fn labels(g: &Graph, shown: &[Shown], kept: &HashSet<u32>, width: impl Fn(&str) -> f32, high: f32) -> Vec<(usize, [f32; 4])> {
    let mut grid: HashMap<(i32, i32), Vec<[f32; 4]>> = HashMap::new();
    let mut out = Vec::new();
    for k in order(g, shown, kept) {
        let s = &shown[k];
        if s.look != Look::Label || s.alpha <= 0.0 || s.lit {
            continue;
        }
        let d = dot_at(&s.r);
        let b = if is_row(&s.r) {
            // beside its dot, inside its row
            let w = width(&label_words(g.node(s.i))).min(s.r[0] + s.r[2] - d[0] - 12.0).max(0.0);
            [d[0] + 8.0, d[1] - high / 2.0, w, high]
        } else {
            let w = width(&label_words(g.node(s.i))).min(s.r[2].max(140.0));
            [d[0] - w / 2.0, d[1] + 5.0, w, high]
        };
        if b[2] < 24.0 {
            continue;
        }
        let cells = cells_of(&b);
        if cells.iter().any(|c| grid.get(c).is_some_and(|v| v.iter().any(|o| overlap(o, &b)))) {
            continue;
        }
        for c in cells {
            grid.entry(c).or_default().push(b);
        }
        out.push((k, b));
    }
    out
}

/// What node N says under its dot drawn at Label ([`labels`]; canvas.rs `board` draws it): its label; a brain's mark,
/// its agent and what its pass came to, as its card says them on two lines ("pr · Said": paper-research N-1479 p323,
/// proto-window N-1446 p1159 small 1), where its agent's name alone, under 24 points, took no label and the mark said
/// nothing at his post's fit.
pub fn label_words(n: &super::Node) -> std::borrow::Cow<'_, str> {
    match n.kind {
        Kind::Brain if !n.note.is_empty() => format!("{} · {}", n.label, n.note).into(),
        _ => n.label.as_str().into(),
    }
}

fn cells_of(b: &[f32; 4]) -> Vec<(i32, i32)> {
    let (x0, y0) = ((b[0] / CELL).floor() as i32, (b[1] / CELL).floor() as i32);
    let (x1, y1) = (((b[0] + b[2]) / CELL).floor() as i32, ((b[1] + b[3]) / CELL).floor() as i32);
    (x0..=x1).flat_map(|x| (y0..=y1).map(move |y| (x, y))).collect()
}

pub(crate) fn overlap(a: &[f32; 4], b: &[f32; 4]) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
}

/// An edge as drawn: between two nodes of SHOWN (their indices), what it says, and how many edges of the graph it
/// stands for (those into what a node not open holds, rolled up into it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drawn {
    pub from: usize,
    pub to: usize,
    pub link: Link,
    pub count: u32,
    /// Lit: each edge it stands for has an end that is the selected node or inside it (a pair's lit edges and the
    /// others are two).
    pub lit: bool,
}

/// G's edges as SHOWN draws them: each end at the nearest node shown that holds it (itself, else what it lies in), the
/// edges between the same two shown nodes with the same link one edge with their count; none inside one node.
/// A node another stands for (a lit point merged into another's, a child too small for its words) ends its edges at
/// that one (STAND, from [`visible_with`]). An edge is lit by its own ends, before they roll up: one is SEL or inside
/// it, so selecting a post lights its edges and none of its thread's others. His ask's trail ([`Link::Trail`]) is
/// drawn only while one of its own ends is SEL: never faded, nor for an island or a line selected, or each answered ask
/// of his would cross it (consult-3 N-1479 p316).
pub fn edges(g: &Graph, shown: &[Shown], stand: &HashMap<u32, usize>, sel: Option<u32>) -> Vec<Drawn> {
    edges_at(g, shown, stand, sel, None)
}

/// [`edges`] at moment WHEN (None: now): a link made after it is not drawn ([`Graph::linked`]).
pub fn edges_at(g: &Graph, shown: &[Shown], stand: &HashMap<u32, usize>, sel: Option<u32>, when: Option<i64>) -> Vec<Drawn> {
    let mut at: HashMap<u32, usize> = shown.iter().enumerate().map(|(k, s)| (s.i, k)).collect();
    at.extend(stand.iter().map(|(i, k)| (*i, *k)));
    let within = |mut i: u32, sel: u32| loop {
        if i == sel {
            return true;
        }
        match g.node(i).parent {
            Some(p) => i = p,
            None => return false,
        }
    };
    let up = |mut i: u32| loop {
        if let Some(k) = at.get(&i) {
            return Some(*k);
        }
        i = g.node(i).parent?;
    };
    let mut rolled: HashMap<(usize, usize, Link, bool), u32> = HashMap::new();
    let mut order = Vec::new();
    for e in g.edges() {
        if !g.linked(e, when) || (e.link == Link::Trail && !sel.is_some_and(|s| e.from == s || e.to == s)) {
            continue;
        }
        let (Some(a), Some(b)) = (up(e.from), up(e.to)) else { continue };
        if a == b {
            continue;
        }
        let lit = sel.is_some_and(|s| within(e.from, s) || within(e.to, s));
        let key = (a.min(b), a.max(b), e.link, lit);
        let c = rolled.entry(key).or_insert(0);
        if *c == 0 {
            order.push(key);
        }
        *c += 1;
    }
    order.into_iter().map(|(a, b, link, lit)| Drawn { from: a, to: b, link, count: rolled[&(a, b, link, lit)], lit }).collect()
}

/// The node under Q, a point of the view: the dot nearest it within a few points (a dot is drawn over what holds it),
/// else the deepest shown whose rectangle holds it.
pub fn under(shown: &[Shown], q: [f32; 2]) -> Option<usize> {
    let inside = |r: &[f32; 4]| q[0] >= r[0] && q[0] <= r[0] + r[2] && q[1] >= r[1] && q[1] <= r[1] + r[3];
    let dot = shown
        .iter()
        .enumerate()
        .filter(|(_, s)| s.look == Look::Dot && s.alpha > 0.0)
        .map(|(k, s)| {
            let d = dot_at(&s.r);
            (k, (d[0] - q[0]).hypot(d[1] - q[1]))
        })
        .filter(|(_, d)| *d <= 6.0)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(k, _)| k);
    dot.or_else(|| shown.iter().enumerate().rev().find(|(_, s)| s.look != Look::Dot && s.alpha > 0.0 && inside(&s.r)).map(|(k, _)| k))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Edge, Node, Rect, Tier};

    /// A project of 4 topics of 6 threads of 8 posts, each inside its parent, and a thread naming an item in another
    /// topic.
    fn small() -> Graph {
        let mut nodes = vec![Node::new("project:research", Kind::Project, "research", None, Rect::new(0.0, 0.0, 10_000.0, 8_000.0))];
        for t in 0..4u32 {
            let tr = Rect::new(200.0 + 2_400.0 * t as f64, 400.0, 2_200.0, 7_000.0);
            let ti = nodes.len() as u32;
            nodes.push(Node::new(format!("topic:{t}"), Kind::Topic, format!("topic {t}"), Some(0), tr));
            for h in 0..6u32 {
                let hr = Rect::new(tr.x + 40.0, tr.y + 40.0 + 1_100.0 * h as f64, 2_100.0, 1_000.0);
                let hi = nodes.len() as u32;
                let kind = if h == 0 { Kind::Item } else { Kind::Thread };
                nodes.push(Node::new(format!("N-{t}{h}"), kind, format!("thread {t}.{h}"), Some(ti), hr));
                for p in 0..8u32 {
                    let pr = Rect::new(hr.x + 20.0, hr.y + 20.0 + 120.0 * p as f64, 2_000.0, 100.0);
                    let mut post = Node::new(format!("N-{t}{h}.p{p}"), Kind::Post, format!("post {p}"), Some(hi), pr);
                    if p == 3 {
                        post.tier = Tier::HisAsk;
                        post.his = true;
                    }
                    nodes.push(post);
                }
            }
        }
        let names = Edge::new(2 + 9 + 9 + 8, 2 + 54 + 1, Link::Names);
        Graph::new(nodes, vec![names]).unwrap()
    }

    fn lit(g: &Graph) -> Vec<u32> {
        (0..g.nodes().len() as u32).filter(|i| g.node(*i).his).collect()
    }

    #[test]
    fn far_out_the_project_and_his_posts_are_drawn_and_nothing_else() {
        let g = small();
        let view = [1200.0, 800.0];
        let cam = Camera::fitting(&g.node(0).rect, view, 300.0);
        let s = visible(&g, &cam, view, &lit(&g));
        let kinds: HashSet<Kind> = s.iter().filter(|x| !x.lit).map(|x| g.node(x.i).kind).collect();
        assert_eq!(kinds, HashSet::from([Kind::Project]), "{s:?}");
        // his 24 posts, lit, but for those whose point would reach into the project card's head (its name and badge)
        let project = s.iter().find(|x| x.i == 0).unwrap();
        assert_eq!(project.look, Look::Card);
        let head = project.r[1] + CARD_HEAD;
        let lit_posts = lit(&g);
        let clear: Vec<u32> = lit_posts.iter().copied().filter(|i| dot_at(&cam.on_screen(&g.node(*i).rect))[1] - LIT_REACH >= head).collect();
        assert!(clear.len() >= 12 && clear.len() < lit_posts.len(), "some under the head, most clear of it: {}", clear.len());
        let points: Vec<&Shown> = s.iter().filter(|x| x.lit).collect();
        assert_eq!(points.iter().map(|x| x.many).sum::<u32>() as usize, clear.len(), "his posts clear of the head, lit");
        assert!(points.iter().all(|x| dot_at(&x.r)[1] - lit_side(x.many) / 2.0 >= head), "none reaches into the head");
    }

    /// An agent on a topic drawn as a card, its dot in the row along the topic's head, is drawn just under the head, not
    /// hidden by it (path 2 through the layout: a held agent hidden there had its edge drawn from its topic); his post
    /// at the same place is not drawn until the card opens.
    #[test]
    fn an_agent_over_a_cards_head_is_drawn_just_under_it() {
        let g = Graph::new(
            vec![
                Node::new("project:p", Kind::Project, "p", None, Rect::new(0.0, 0.0, 1_000.0, 1_000.0)),
                Node::new("topic:t", Kind::Topic, "t", Some(0), Rect::new(100.0, 100.0, 400.0, 300.0)),
                Node::new("agent:a", Kind::Agent, "a", Some(1), Rect::new(110.0, 110.0, 24.0, 24.0)),
                Node::new("N-1.p1", Kind::Post, "p1", Some(1), Rect::new(302.0, 112.0, 36.0, 20.0)),
            ],
            vec![],
        )
        .unwrap();
        let cam = Camera { at: [0.0, 0.0], scale: 0.6 };
        let mut g = g;
        g.update(3, |n| n.his = true);
        let s = visible(&g, &cam, [800.0, 800.0], &[2, 3]);
        let topic = s.iter().find(|x| x.i == 1).unwrap();
        assert_eq!(topic.look, Look::Card);
        let a = s.iter().find(|x| x.i == 2).expect("the agent drawn");
        assert!(a.lit && (dot_at(&a.r)[1] - (topic.r[1] + CARD_HEAD + LIT_REACH)).abs() < 1e-3, "just under the head: {:?} {:?}", a.r, topic.r);
        assert!(!s.iter().any(|x| x.i == 3), "his post under the head waits for the card to open");
    }

    #[test]
    fn his_posts_that_would_fall_on_one_point_are_one_point() {
        let g = small();
        // the whole project in a view so small that his 24 posts crowd
        let view = [160.0, 120.0];
        let cam = Camera::fitting(&g.node(0).rect, view, 10.0);
        let s = visible(&g, &cam, view, &lit(&g));
        let lit: Vec<&Shown> = s.iter().filter(|x| x.lit).collect();
        assert_eq!(lit.iter().map(|x| x.many).sum::<u32>(), 24, "each stands for those that would meet it");
        assert!(lit.len() < 24, "fewer points than posts: {}", lit.len());
        apart(&lit);
    }

    /// No two lit points of one kind come nearer than LIT_APART: drawn at their largest, they never touch.
    fn apart(lit: &[&Shown]) {
        for (k, a) in lit.iter().enumerate() {
            for b in &lit[k + 1..] {
                let e = dist(dot_at(&a.r), dot_at(&b.r));
                assert!(e >= LIT_APART, "{} and {} are {e} apart", a.i, b.i);
                assert!(e - lit_side(a.many) / 2.0 - lit_side(b.many) / 2.0 >= 2.0, "{} and {} touch", a.i, b.i);
            }
        }
    }

    /// proto-window N-1479 p48: two of his posts 6 points apart, either side of a grid cell's edge (18 points, the
    /// rule before), are one point; a third 26 away is its own.
    #[test]
    fn his_posts_that_would_meet_across_a_cells_edge_are_one_point() {
        let mut nodes = vec![Node::new("project:research", Kind::Project, "research", None, Rect::new(0.0, 0.0, 1_000.0, 1_000.0))];
        for (k, x) in [34.0, 40.0, 60.0].into_iter().enumerate() {
            let mut post = Node::new(format!("N-1.p{k}"), Kind::Post, format!("post {k}"), Some(0), Rect::new(x - 0.25, 99.75, 0.5, 0.5));
            post.his = true;
            nodes.push(post);
        }
        let g = Graph::new(nodes, vec![]).unwrap();
        let view = [400.0, 400.0];
        let cam = Camera { at: [0.0, 0.0], scale: 1.0 };
        let (s, stand) = visible_with(&g, &cam, view, &lit(&g));
        let lit: Vec<&Shown> = s.iter().filter(|x| x.lit).collect();
        let drawn: Vec<(u32, u32)> = lit.iter().map(|x| (x.i, x.many)).collect();
        assert_eq!(drawn, [(1, 2), (3, 1)], "p0 stands for p1, 6 away; p2, 26 away, is its own");
        assert_eq!(stand.get(&2).map(|k| s[*k].i), Some(1), "p1's edges end at p0");
        apart(&lit);
    }

    #[test]
    fn nothing_out_of_view_is_drawn_and_nothing_in_a_closed_node() {
        let g = small();
        let view = [1200.0, 800.0];
        // into the second topic's third thread, near enough for its posts
        let at = g.find_in(0, "N-12").unwrap();
        let cam = Camera::fitting(&g.node(at).rect, view, 40.0);
        let s = visible(&g, &cam, view, &lit(&g));
        let seen = cam.seen(view);
        for x in &s {
            assert!(g.node(x.i).rect.meets(&seen), "{} drawn out of view", g.node(x.i).id);
            if let Some(p) = g.node(x.i).parent
                && !x.lit
            {
                let ps = s.iter().find(|y| y.i == p).expect("its parent is drawn");
                assert!(ps.open > 0.0, "{} drawn inside {} not open", g.node(x.i).id, g.node(p).id);
            }
        }
        assert!(s.iter().any(|x| g.node(x.i).id == "N-12.p0"), "its posts are drawn");
        assert!(!s.iter().any(|x| g.node(x.i).id.starts_with("N-3")), "nothing of a topic out of view");
        assert!(s.len() < 40, "only what is in view: {}", s.len());
    }

    #[test]
    fn a_node_keeps_its_place_across_scales() {
        let g = small();
        let view = [1200.0, 800.0];
        let p = g.find_in(0, "N-12.p3").unwrap();
        let mid = g.node(p).rect.mid();
        let mut cam = Camera::fitting(&g.node(0).rect, view, 0.0);
        let q = cam.to_screen(mid);
        let mut looks = Vec::new();
        for _ in 0..40 {
            cam.zoom_about(1.25, q);
            let s = visible(&g, &cam, view, &lit(&g));
            let Some(x) = s.iter().find(|x| x.i == p) else { continue };
            let c = [x.r[0] + x.r[2] / 2.0, x.r[1] + x.r[3] / 2.0];
            assert!((c[0] - q[0]).abs() < 0.05 && (c[1] - q[1]).abs() < 0.05, "it stays under the pointer: {c:?} {q:?}");
            looks.push(x.look);
        }
        looks.dedup();
        assert_eq!(looks, [Look::Dot, Look::Label, Look::Card, Look::Open], "it gains detail in place");
    }

    #[test]
    fn labels_go_on_by_rank_and_never_overlap() {
        let g = small();
        let view = [1200.0, 800.0];
        // a thread open, its posts at Label, 42 points apart: labels 60 high collide with their neighbours'
        let t = g.find_in(0, "N-02").unwrap();
        let mut cam = Camera::fitting(&g.node(t).rect, view, 0.0);
        let q = cam.to_screen(g.node(t).rect.mid());
        cam.zoom_about(0.35 / cam.scale, q);
        let s = visible(&g, &cam, view, &[]);
        let at_label: Vec<usize> = (0..s.len()).filter(|k| s[*k].look == Look::Label && g.node(s[*k].i).id.starts_with("N-02.")).collect();
        assert_eq!(at_label.len(), 8, "{s:?}");
        let placed = labels(&g, &s, &HashSet::new(), |t| t.len() as f32 * 7.0, 60.0);
        for (a, (_, x)) in placed.iter().enumerate() {
            for (_, y) in &placed[a + 1..] {
                assert!(!overlap(x, y), "{x:?} {y:?}");
            }
        }
        let ids: Vec<&str> = placed.iter().map(|(k, _)| g.node(s[*k].i).id.as_str()).filter(|i| i.starts_with("N-02.")).collect();
        assert_eq!(ids, ["N-02.p3", "N-02.p0", "N-02.p5", "N-02.p7"], "his first, then the rest where they fit");
        // a label shown in the last frame keeps its place before one that was not
        let kept = HashSet::from([g.find_in(0, "N-02.p1").unwrap()]);
        let again = labels(&g, &s, &kept, |t| t.len() as f32 * 7.0, 60.0);
        let ids: Vec<&str> = again.iter().map(|(k, _)| g.node(s[*k].i).id.as_str()).filter(|i| i.starts_with("N-02.")).collect();
        assert_eq!(ids, ["N-02.p3", "N-02.p1", "N-02.p5", "N-02.p7"]);
    }

    #[test]
    fn an_edge_into_a_node_not_open_rolls_up_into_it() {
        let g = small();
        let view = [1200.0, 800.0];
        let mut cam = Camera::fitting(&g.node(0).rect, view, 0.0);
        cam.zoom_about(0.6, [600.0, 400.0]);
        let s = visible(&g, &cam, view, &[]);
        let e = edges(&g, &s, &HashMap::new(), None);
        assert_eq!(e.len(), 1, "{e:?}");
        let (a, b) = (g.node(s[e[0].from].i).id.as_str(), g.node(s[e[0].to].i).id.as_str());
        assert_eq!((a, b, e[0].count), ("topic:0", "topic:1", 1), "a post's edge to an item, between the topics that hold them");
        // nearer, the threads open: it runs between the thread and the item
        let cam = Camera::fitting(&g.node(0).rect, view, 0.0);
        let s = visible(&g, &cam, view, &[]);
        let e = edges(&g, &s, &HashMap::new(), None);
        let (a, b) = (g.node(s[e[0].from].i).id.as_str(), g.node(s[e[0].to].i).id.as_str());
        assert_eq!((a, b), ("N-02", "N-10"));
    }

    /// proto-window's p38 hold 1: lit by each edge's own ends, before they roll up. Two posts of one thread each name
    /// an item of the next topic; far out, both edges roll up between the two topics.
    #[test]
    fn an_edge_is_lit_by_its_own_ends_not_by_what_they_roll_up_to() {
        let g0 = small();
        let nodes = g0.nodes().to_vec();
        let (p7, p6) = (g0.find_in(0, "N-02.p7").unwrap(), g0.find_in(0, "N-02.p6").unwrap());
        let (n10, n11) = (g0.find_in(0, "N-10").unwrap(), g0.find_in(0, "N-11").unwrap());
        let edges_ = vec![Edge::new(p7, n10, Link::Names), Edge::new(p6, n11, Link::Names)];
        let g = Graph::new(nodes, edges_).unwrap();
        let view = [1200.0, 800.0];
        let mut cam = Camera::fitting(&g.node(0).rect, view, 0.0);
        cam.zoom_about(0.6, [600.0, 400.0]);
        let s = visible(&g, &cam, view, &[]);
        let lit_of = |sel: Option<u32>| -> Vec<(String, String, u32, bool)> {
            let mut e: Vec<_> = edges(&g, &s, &HashMap::new(), sel).iter().map(|d| (g.node(s[d.from].i).id.clone(), g.node(s[d.to].i).id.clone(), d.count, d.lit)).collect();
            e.sort();
            e
        };
        let t = |a: &str, b: &str, c: u32, l: bool| (a.to_string(), b.to_string(), c, l);
        assert_eq!(lit_of(None), [t("topic:0", "topic:1", 2, false)], "both rolled into one between the topics");
        // p7 selected: its edge alone lit, the thread's other one not, though both run between the same two topics
        assert_eq!(lit_of(Some(p7)), [t("topic:0", "topic:1", 1, false), t("topic:0", "topic:1", 1, true)]);
        // the thread holding both, or the topic the items are in: both
        assert_eq!(lit_of(g.find_in(0, "N-02")), [t("topic:0", "topic:1", 2, true)]);
        assert_eq!(lit_of(g.find_in(0, "topic:1")), [t("topic:0", "topic:1", 2, true)]);
        // another post of the thread, with no edge of its own: none
        assert_eq!(lit_of(g.find_in(0, "N-02.p3")), [t("topic:0", "topic:1", 2, false)]);
    }

    /// His ask's trail (map.rs `Link::Trail`; consult-3 N-1479 p316 point 1): drawn only while one of its own ends is
    /// selected, his post or the answer it goes to; with nothing selected, or the thread, the topic or the island that
    /// holds it, none, faded or lit, where an edge that names an item is drawn all the same.
    #[test]
    fn his_asks_trail_is_drawn_only_while_its_post_or_its_answer_is_selected() {
        let g0 = small();
        let (his, answer, other) = (g0.find_in(0, "N-02.p3").unwrap(), g0.find_in(0, "N-11.p5").unwrap(), g0.find_in(0, "N-02.p6").unwrap());
        let n10 = g0.find_in(0, "N-10").unwrap();
        let g = Graph::new(g0.nodes().to_vec(), vec![Edge::new(his, answer, Link::Trail), Edge::new(other, n10, Link::Names)]).unwrap();
        let view = [1200.0, 800.0];
        let cam = Camera::fitting(&g.node(0).rect, view, 0.0);
        let s = visible(&g, &cam, view, &[]);
        let trails = |sel: Option<u32>| -> Vec<bool> { edges(&g, &s, &HashMap::new(), sel).iter().filter(|d| d.link == Link::Trail).map(|d| d.lit).collect() };
        assert_eq!(trails(None), Vec::<bool>::new(), "nothing selected: no trail, faded or lit");
        for holder in ["N-02", "topic:0", "project:research"] {
            assert_eq!(trails(g.find_in(0, holder)), Vec::<bool>::new(), "{holder} selected: none");
        }
        assert_eq!(trails(g.find_in(0, "N-02.p6")), Vec::<bool>::new(), "another post of the thread: none");
        assert_eq!(trails(Some(his)), [true], "his post selected: its trail, lit");
        assert_eq!(trails(Some(answer)), [true], "the answer selected: the same");
        assert_eq!(edges(&g, &s, &HashMap::new(), None).iter().filter(|d| d.link == Link::Names).count(), 1, "an item's edge drawn as before");
    }

    /// proto-window's p38 hold 2: a lit point merged into another ends its edges at the point that stands for it, not
    /// at what holds it.
    #[test]
    fn a_merged_points_edges_end_at_the_point_standing_for_it() {
        let g0 = small();
        let nodes = g0.nodes().to_vec();
        // every one of his posts names the project's last item (topic 3's)
        let item = g0.find_in(0, "N-30").unwrap();
        let his: Vec<u32> = lit(&g0);
        let e: Vec<Edge> = his.iter().map(|p| Edge::new(*p, item, Link::Names)).collect();
        let g = Graph::new(nodes, e).unwrap();
        let view = [160.0, 120.0];
        let cam = Camera::fitting(&g.node(0).rect, view, 10.0);
        let (s, stand) = visible_with(&g, &cam, view, &his);
        assert!(!stand.is_empty(), "some merged at this size");
        for (i, k) in &stand {
            assert!(s[*k].lit && s[*k].many > 1 && g.node(s[*k].i).kind == g.node(*i).kind, "{i} stands in {k}");
        }
        let d = edges(&g, &s, &stand, None);
        // each edge from a lit point: its count is the posts it stands for, and none ends at a topic or the project
        let from_points: u32 = d.iter().filter(|x| s[x.from].lit || s[x.to].lit).map(|x| x.count).sum();
        assert_eq!(from_points, his.len() as u32, "every one of his edges drawn, at a point: {d:?}");
        for x in &d {
            let (a, b) = (g.node(s[x.from].i).kind, g.node(s[x.to].i).kind);
            assert!(a == Kind::Post || b == Kind::Post, "an edge of his ends at a point: {a:?} {b:?}");
            let point = if s[x.from].lit { &s[x.from] } else { &s[x.to] };
            assert_eq!(x.count, point.many, "a point's edges are as many as the posts it stands for");
        }
    }

    #[test]
    fn a_fade_follows_the_width_alone() {
        assert_eq!(fade(10.0, 20.0), 0.0);
        assert_eq!(fade(32.0, 20.0), 1.0);
        let half = fade(20.0 * FADE.sqrt(), 20.0);
        assert!((half - 0.5).abs() < 1e-4, "{half}");
        assert_eq!(fade(1e9, f32::INFINITY), 0.0);
    }

    /// A cloud's project: two large threads, and forty small ones of one post or a few, ten across and four down, the
    /// heavier the later in the last row, each saying its words read from READS on screen; the first large one names the
    /// sixth small one.
    fn threads_of_one_post() -> Graph {
        let mut nodes = vec![Node::new("project:cloud", Kind::Project, "cloud", None, Rect::new(0.0, 0.0, 10_000.0, 10_000.0))];
        let mut thread = |id: String, r: Rect, weight: f32| {
            let mut t = Node::new(id.clone(), Kind::Thread, id, Some(0), r);
            t.weight = weight;
            t.reads = Some(READS);
            nodes.push(t);
        };
        thread("N-1".into(), Rect::new(200.0, 200.0, 2_000.0, 1_000.0), 50.0);
        thread("N-2".into(), Rect::new(5_000.0, 200.0, 2_000.0, 1_000.0), 40.0);
        for k in 0..40u32 {
            let r = Rect::new(200.0 + 240.0 * (k % 10) as f64, 4_000.0 + 400.0 * (k / 10) as f64, 100.0, 40.0);
            thread(format!("N-{}", 10 + k), r, if k < 30 { 1.0 } else { (k - 28) as f32 });
        }
        Graph::new(nodes, vec![Edge::new(1, 3 + 5, Link::Names)]).unwrap()
    }

    /// paper-research N-1446 p1677, 1: no box without words at any zoom. Far out the small threads are groups that say
    /// how many and of what weight; the heaviest come in by the ladder at the least their words read at; one whose
    /// group's card would meet a thread drawn as itself is stood for by it. Nearer, each is itself where it is.
    #[test]
    fn a_card_too_small_for_its_words_joins_a_group_until_he_zooms_in() {
        let g = threads_of_one_post();
        let id = |i: u32| g.node(i).id.as_str();
        let cam = Camera { at: [0.0, 0.0], scale: 0.25 };
        let (s, stand) = visible_with(&g, &cam, [2_500.0, 2_500.0], &[]);
        for x in &s {
            let m = g.node(x.i).reads.unwrap_or_default();
            assert!(x.r[2] >= m[0] && x.r[3] >= m[1], "{} drawn at {:?}, under its words' {m:?}", id(x.i), x.r);
        }
        // each thread drawn once or stood for, never both; together they stand for all 42
        let threads: Vec<&Shown> = s.iter().filter(|x| g.node(x.i).kind == Kind::Thread).collect();
        for t in 1..g.nodes().len() as u32 {
            let drawn = threads.iter().filter(|x| x.i == t).count();
            assert_eq!(drawn + usize::from(stand.contains_key(&t)), 1, "{}", id(t));
        }
        assert_eq!(threads.iter().map(|x| x.many).sum::<u32>(), 42);
        let groups: Vec<(&str, u32, [f32; 2])> = threads.iter().filter_map(|x| x.group.map(|k| (id(x.i), x.many, k.weight))).collect();
        let ones = [1.0, 1.0];
        let rows = [("N-10", 3, ones), ("N-13", 3, ones), ("N-16", 3, ones), ("N-20", 3, ones), ("N-23", 3, ones), ("N-26", 3, ones), ("N-30", 3, ones), ("N-33", 3, ones), ("N-36", 3, ones)];
        assert_eq!(groups[0], ("N-43", 3, [3.0, 5.0]), "the last row's, of weights 3 to 5");
        assert_eq!(groups[1..], rows, "the first three rows' of one post, three a group");
        assert!(threads.iter().filter(|x| x.group.is_some()).all(|x| (x.r[2], x.r[3]) == (GROUP[0], GROUP[1])));
        // the heaviest by the ladder: the two large as themselves, three small at their words' size (of the five
        // heaviest, two would meet a heavier one), standing for the neighbours whose group's card would meet them; at
        // each row's end one alone, at its words' size
        let alone: Vec<(&str, u32)> = threads.iter().filter(|x| x.group.is_none()).map(|x| (id(x.i), x.many)).collect();
        assert_eq!(alone, [("N-40", 1), ("N-19", 1), ("N-29", 1), ("N-39", 1), ("N-1", 1), ("N-2", 1), ("N-45", 3), ("N-47", 2), ("N-49", 1)]);
        for t in ["N-40", "N-19", "N-45", "N-49"] {
            let x = threads.iter().find(|x| id(x.i) == t).unwrap();
            assert_eq!((x.r[2], x.r[3]), (READS[0], READS[1]), "{t}");
        }
        // no two of the cards made up here meet: groups, those alone, those come in by the ladder
        // (and the two large, holding none, at their words' height: p1796, 1a)
        let made: Vec<&&Shown> = threads.iter().filter(|x| x.r != cam.on_screen(&g.node(x.i).rect)).collect();
        assert_eq!(made.len(), 19);
        for (k, a) in made.iter().enumerate() {
            for b in &made[k + 1..] {
                assert!(!overlap(&a.r, &b.r), "{} and {} meet", id(a.i), id(b.i));
            }
        }
        let at = |t: &str| s.iter().position(|x| id(x.i) == t).unwrap();
        let of = |t: &str| stand[&g.find_in(0, t).unwrap()];
        assert_eq!([of("N-48"), of("N-46"), of("N-44"), of("N-42"), of("N-41"), of("N-15")], [at("N-47"), at("N-45"), at("N-45"), at("N-43"), at("N-43"), at("N-13")]);
        // an edge to a thread in a group ends at the group
        let e = edges(&g, &s, &stand, None);
        assert_eq!(e.iter().map(|d| (id(s[d.from].i), id(s[d.to].i), d.count)).collect::<Vec<_>>(), [("N-13", "N-1", 1)]);
        // nearer, the small threads read: each drawn as itself where it is, none in a group
        let near = Camera { at: [0.0, 3_900.0], scale: 1.0 };
        let (s, stand) = visible_with(&g, &near, [2_500.0, 1_400.0], &[]);
        assert!(stand.is_empty() && s.iter().all(|x| x.group.is_none() && x.many == 1), "{s:?}");
        for k in 3..43u32 {
            let x = s.iter().find(|x| x.i == k).unwrap_or_else(|| panic!("{} drawn", id(k)));
            assert_eq!(x.r, near.on_screen(&g.node(k).rect), "{} where it is", id(k));
        }
    }

    /// A project of one thread, tall, of six posts one under another, the heavier every other one from the top (p0, p2,
    /// p4, then p1, p3, p5), each saying its words read from READS; the thread opens from 900 (cloud.rs: once its
    /// posts' cards would read). The project is large, so it is open whole at every zoom the legs look at but the
    /// farthest (0.02, 200 wide), where it is a card.
    fn group_chat() -> Graph {
        let mut nodes = vec![Node::new("project:cloud", Kind::Project, "cloud", None, Rect::new(0.0, 0.0, 10_000.0, 10_000.0))];
        let mut t = Node::new("N-1", Kind::Thread, "Your one group chat", Some(0), Rect::new(0.0, 0.0, 1_600.0, 2_400.0));
        t.opens = Some(900.0);
        t.reads = Some(READS);
        t.weight = 6.0;
        nodes.push(t);
        for (p, weight) in [6.0, 3.0, 5.0, 2.0, 4.0, 1.0].into_iter().enumerate() {
            let r = Rect::new(100.0, 800.0 + 300.0 * p as f64, 1_400.0, 100.0);
            let mut n = Node::new(format!("N-1.p{p}"), Kind::Post, format!("post {p}"), Some(1), r);
            n.weight = weight;
            n.reads = Some(READS);
            nodes.push(n);
        }
        Graph::new(nodes, vec![]).unwrap()
    }

    /// proto-window N-1446 p1683, paper-research p1715 (1): the group chat's card shows its three most-linked posts as
    /// soon as it is a card, each at LADDER_CARD so its words read, inside the card under its head, meeting no other;
    /// then one more each time it grows by FADE; while it opens they go down to their own size and the rest come in;
    /// open, each is itself where it is. Showing none, grown or not, it is its words' height (p1726, p1739); a post,
    /// holding none, is at most its words' height as it comes in and where it is (p1796, 1a: here lod.rs's WORDS_HIGH).
    #[test]
    fn a_threads_three_heaviest_posts_come_in_at_once_before_it_opens_at_a_size_their_words_read() {
        let g = group_chat();
        let view = [2_000.0, 2_500.0];
        let at = |scale: f64| visible(&g, &Camera { at: [0.0, 0.0], scale }, view, &[]);
        let posts = |scale: f64| -> Vec<(u32, [f32; 4], f32)> {
            let mut v: Vec<_> = at(scale).iter().filter(|x| g.node(x.i).kind == Kind::Post).map(|x| (x.i - 2, x.r, x.alpha)).collect();
            v.sort_by_key(|x| x.0);
            v
        };
        let thread = |scale: f64| at(scale).into_iter().find(|x| x.i == 1).unwrap();
        let near = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3);
        // holding none, under its card's side or too narrow for the ladder's first, it is its words' height (p1726)
        assert!(posts(0.08).is_empty(), "under its card's side, none");
        assert!(near(thread(0.08).r, [0.0, 0.0, 128.0, WORDS_HIGH]), "{:?}", thread(0.08).r);
        let s = at(0.08);
        assert_eq!(under(&s, [64.0, 150.0]).map(|k| s[k].i), Some(0), "under its words, the project, not an empty card");
        assert!(posts(0.1).is_empty() && near(thread(0.1).r, [0.0, 0.0, 160.0, WORDS_HIGH]), "160 wide, under LADDER_CARD's 200");
        // its words measured where they are drawn: a title on two lines under 150 wide, on one from there (p1727)
        let words = |i: u32, w: f32| if i == 1 && w >= 150.0 { 43.0 } else { WORDS_HIGH };
        let worded = |scale: f64| visible_worded(&g, &Camera { at: [0.0, 0.0], scale }, view, &[], &words).0.into_iter().find(|x| x.i == 1).unwrap().r;
        assert!(near(worded(0.08), [0.0, 0.0, 128.0, WORDS_HIGH]) && near(worded(0.1), [0.0, 0.0, 160.0, 43.0]), "{:?} {:?}", worded(0.08), worded(0.1));
        assert!(near(worded(0.135), [0.0, 0.0, 216.0, 324.0]), "holding the three, its own size");
        // zoomed out until the project closes: the thread a card grown by the project's ladder, showing none of its
        // posts, is its words' height too, not LADDER_CARD's 200 by 75 round them (p1739)
        let s = visible_worded(&g, &Camera { at: [0.0, 0.0], scale: 0.02 }, view, &[], &words).0;
        let t = s.iter().find(|x| x.i == 1).unwrap();
        assert!(s[0].i == 0 && s[0].open == 0.0 && s[0].look == Look::Card && s.len() == 2, "the project a card holding it: {s:?}");
        assert!(near(t.r, [0.0, CARD_HEAD, LADDER_CARD[0], 43.0]), "grown, cut to its words: {t:?}");
        assert_eq!(under(&s, [100.0, CARD_HEAD + 60.0]).map(|k| s[k].i), Some(0), "under its words, the project, not an empty area");
        assert!(posts(0.02).is_empty() && near(thread(0.02).r, [0.0, CARD_HEAD, LADDER_CARD[0], WORDS_HIGH]), "{:?}", thread(0.02).r);
        // a card, under FADE times its card's side: the three heaviest at once, at LADDER_CARD, and the card its own
        // size round them
        let t = thread(0.135);
        assert!(t.open == 0.0 && near(t.r, [0.0, 0.0, 216.0, 324.0]) && side(&t.r) > 170.0 && side(&t.r) < 170.0 * FADE, "{t:?}");
        let three = posts(0.135);
        assert_eq!(three.iter().map(|x| x.0).collect::<Vec<_>>(), [0, 2, 4]);
        let cut = LADDER_CARD[1].min(WORDS_HIGH);
        for (k, (p, r, a)) in three.iter().enumerate() {
            let want = [8.0, [77.25, 158.25, 239.25][k], LADDER_CARD[0], cut];
            assert!(r.iter().zip(want).all(|(x, y)| (x - y).abs() < 1e-3), "p{p} at {r:?}, not {want:?}");
            assert!(r[1] >= t.r[1] + CARD_HEAD && r[1] + r[3] <= t.r[1] + t.r[3], "p{p} inside, under the head");
            assert!(*a > 0.0 && *a < 1.0 && *a == three[0].2, "p{p} coming in with the others: {a}");
            for (_, o, _) in &three[k + 1..] {
                assert!(!overlap(r, o), "{r:?} {o:?}");
            }
        }
        // larger, a fourth, then a fifth coming in: as wide as they are, as high as LADDER_CARD, cut to their words
        let five = posts(0.3);
        assert_eq!(five.iter().map(|x| x.0).collect::<Vec<_>>(), [0, 1, 2, 3, 4]);
        assert!(five.iter().all(|x| (x.1[2], x.1[3]) == (420.0, cut)), "{five:?}");
        let alpha: Vec<f32> = five.iter().map(|x| x.2).collect();
        assert!(alpha[3] > 0.0 && alpha[3] < 1.0 && [0, 1, 2, 4].iter().all(|k| alpha[*k] == 1.0), "{alpha:?}");
        // opening: the five going down to their own height, the sixth come in as itself
        let cam = Camera { at: [0.0, 0.0], scale: 0.6 };
        let t = thread(0.6);
        assert!(t.open > 0.0 && t.open < 1.0, "{}", t.open);
        let six = posts(0.6);
        assert_eq!(six.len(), 6);
        for (p, r, a) in &six {
            let own = cam.on_screen(&g.node(p + 2).rect);
            if *p == 5 {
                assert_eq!((*r, *a), (own, t.open), "p5 as itself, at its opening");
            } else {
                assert!(r[3] > own[3] && r[3] < LADDER_CARD[1] && r[2] == own[2] && *a >= t.open, "p{p} going down to its own: {r:?} {a}");
            }
        }
        // open: each where it is
        let cam = Camera { at: [0.0, 0.0], scale: 1.0 };
        let (s, stand) = visible_with(&g, &cam, view, &[]);
        assert!(stand.is_empty() && s.iter().all(|x| x.group.is_none()));
        let all = posts(1.0);
        assert_eq!(all.len(), 6);
        for (p, r, a) in all {
            let mut own = cam.on_screen(&g.node(p + 2).rect);
            own[3] = own[3].min(WORDS_HIGH);
            assert_eq!((r, a), (own, 1.0), "p{p}");
        }
    }
}
