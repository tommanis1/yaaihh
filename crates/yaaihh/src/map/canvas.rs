//! The Map's page (I-1478; the owner, N-1104 p715): the crumb trail at its head, the canvas under it, and at its right
//! the inspector once something is selected. It is what a lens at `Depth::Cloud` draws (lens.rs), opened from the
//! Structure pane.
//!
//! - **The canvas** draws what [`lod::visible`] gives at the camera, each node at its look, the labels [`lod::labels`]
//!   puts down, the edges [`lod::edges`] rolls up, his posts lit; a minimap in its corner.
//! - **Moves** (p715's clicks): a drag pans; the wheel and a pinch zoom at the pointer (it takes the wheel, so Ctrl and
//!   a pinch zoom the Map, not the window: zoom.rs `wheel`); a hover shows a node's two lines; a click selects, the
//!   inspector showing its card; a double-click flies in to fit the node (a post its card, a line of columns the
//!   column of its latest post: [`fit_of`]), its second click the canvas's even where the inspector its first opened
//!   now is ([`pair_catcher`]); Esc flies out to what holds the place in view; a crumb flies to its node; a press on
//!   the minimap flies there.
//! - **The depth switch** over the canvas's top left ([`depth_switch`]; structure.md §2 and §9): Cloud · Structure ·
//!   Events · Record · Wire, lit where the Map is, each one press away, in place: Events and Record draw a node's lines
//!   of the record in its card ([`depth_lines`]), Wire flies into a call, which draws its wire. Esc from a node drawn
//!   at a depth comes back up to Structure first.
//! - **Nothing on the canvas is a button**: what a node does is in the inspector (p715: "the Map stays clean"). Past
//!   level 4 the deep levels draw what their pages do, presses with them (deep.rs `inside`), and a post's card ends
//!   its words with "Open its page" ([`post_card`]).

use super::camera::{Camera, FARTHEST, Flight, NEAREST};
use super::lod::{self, Look, Shown};
use super::{Graph, Kind, Node, Rect, Ring, State};
use crate::App;
use crate::lens::Depth;
use rune::{Color, El, LineCap, MagnifyEvent, PointerEvent, ScrollSpec, Shape, Tx, WheelEvent, canvas, col, el, row, text};
use rune_kit::Kit;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Room kept round a node a flight lands on, in points.
const ROOM: f32 = 28.0;
/// Wheel points to a notch (rune's 40 a line), and a notch's zoom: the graph page's too (lanes.rs).
pub(super) const NOTCH: f32 = 40.0;
pub(super) const PER_NOTCH: f64 = 1.25;
/// Travel past which a press is a drag, not a click (rune ends a run of clicks at 4).
pub(super) const CLICK: f32 = 4.0;
/// How long a click that selected a node waits for its second, in seconds: rune's `double_click_interval` headless and
/// on Linux, and the system's default on Windows and macOS (a handler cannot read the window's own).
const PAIR: f64 = 0.5;
/// A dot, in points (a lit one's: lod::lit_side).
const DOT: f32 = 6.0;
/// Between a selected dot and its ring, in points.
const RING_GAP: f32 = 2.0;
/// The line of an asking post's ring ([`mark`]), in points.
const ASK_W: f32 = 2.0;
/// A label's line, in points.
const LINE: f32 = 16.0;
/// An open region's head over its edge, in points.
const HEAD: f32 = 22.0;
/// The minimap.
const MINI: [f32; 2] = [168.0, 112.0];
/// The inspector's width.
pub(crate) const INSPECTOR: f32 = 320.0;
/// The least of a node in view, wide and high in points, that its lines at a depth are drawn in ([`depth_lines`]).
const LAYER_ROOM: [f32; 2] = [200.0, 96.0];

/// A node named across graphs: its project's id and its own (the graph is built again as the record changes, and its
/// indices with it).
pub type Name = (String, String);

/// The Map's own state on the App.
#[derive(Default)]
pub struct View {
    pub cam: Camera,
    /// The canvas's place in the window, as last laid out.
    pub area: [f32; 4],
    /// The camera has been placed on the whole record once the canvas had a size.
    placed: bool,
    pub sel: Option<Name>,
    pub hover: Option<Name>,
    /// The pointer over the canvas, in its points.
    pointer: Option<[f32; 2]>,
    /// The pointer is over a control floating on the canvas, the depth switch or the minimap ([`floating`]): no node
    /// under it is hovered or previewed ([`pointed`]).
    on_control: bool,
    /// The press under way: where the pointer was last, and whether it has moved past a click.
    press: Option<([f32; 2], bool)>,
    /// A click that selected a node, waiting for its second: the node, the click's window point and its time on the
    /// window's clock ([`pair_catcher`]). The next press on the canvas drops it, and so does the poll once [`PAIR`] has
    /// passed.
    pair: Option<(Name, [f32; 2], f64)>,
    /// The page's place in the window (the crumbs and the canvas, the inspector), as last laid out.
    page: [f32; 4],
    flight: Option<(Flight, Option<f64>)>,
    /// What the flight under way fits: a change of the canvas's size while it flies (the inspector opening as a
    /// double-click selects) fits it again, so it lands whole in the canvas it lands in.
    fitting: Option<Rect>,
    /// What the last frame labelled, so labels hold still (lod.rs `labels`).
    kept: RefCell<HashSet<u32>>,
    graph: Option<Arc<Graph>>,
    /// What is drawn from the farthest zoom: his posts, lit, an agent's post that asks him, and the agents.
    lit: Vec<u32>,
    /// Where the camera goes when the canvas is first laid out: a world point at its middle and a scale (a still's).
    aim: Option<([f64; 2], f64)>,
    /// What waits on him, and what is held or stopped, under each node.
    counts: Vec<Counts>,
    /// What the graph is laid from, and when it was laid (join.rs).
    pub laying: super::join::Laying,
    /// The graph was set by hand (`set_graph`: the made-up record, a test's), not laid from the window's projects.
    fixed: bool,
    /// The graph is the cloud's (cloud.rs, `set_cloud`): its cards drawn as cloud.rs draws them, none lit as a point,
    /// a click going in.
    cloud: bool,
    /// He has moved the camera since it was placed: a lay that changes the record's size leaves it where it is.
    touched: bool,
    /// Counts the graphs set: what the last frame drew is of the graph it was drawn from.
    shape: u64,
    /// What the last frame drew, each node with its width on screen and look, and the graph's `shape` then: what the
    /// deep levels' step reads (`deep_step`).
    drawn: RefCell<(u64, Vec<(u32, f32, Look)>)>,
    /// The places of the words the last frame's cards drew, each with its post's project: what `texts::poll` asks for
    /// while the Map shows ([`worded`]; consult-3 N-1479 p180, proto-lead p181).
    worded: RefCell<Vec<(usize, docket_machine::Loc)>>,
    /// The posts' cards the frame draws with their words, on screen, and how strongly ([`word_card`]): set before any
    /// node is drawn, so the head of a node holding one gives way to it where they meet (`node`; proto-lead p193).
    word_cards: RefCell<Vec<(u32, [f32; 4], f32)>>,
    /// The node drawn at Events or Record, and that depth: its lines in its place ([`depth_lines`]), what it holds not
    /// drawn ([`View::seen`]). Set by the depth switch ([`to_depth`]); Structure, Cloud, Esc, a crumb and a flight into
    /// another node clear it.
    at_depth: Option<(Name, Depth)>,
    /// [`View::here`] as last worked out, and what it was worked out from ([`View::here_kept`]).
    here_from: Option<(HereFrom, Option<Here>)>,
    /// What the brain's step last read ([`brain_step`]): the cloud's project, its tickets' and agents' revisions, how
    /// many of its tickets a fetch holds, the moment shown and the graph's `shape`. While they stay, it reads nothing.
    brain_from: Option<(usize, u64, u64, usize, Option<i64>, u64)>,
    /// The moment the cloud is drawn at (cloud.rs `Cloud::at`, the time bar's; None: now): only what was there by then
    /// is drawn ([`lod::visible_at`]), from the one graph built at now, so a drag along the bar draws each frame with
    /// nothing built. Set from the App as each frame is drawn ([`page`]) and as the bar's drag moves it.
    moment: std::cell::Cell<Option<i64>>,
    /// How T is drawn and the moment ages are counted from (spacetime.rs), set from the App as each frame is drawn.
    pub space: std::cell::Cell<Option<(super::spacetime::Space, i64)>>,
    /// The time planes the last frame drew a card on (Glass).
    planes: RefCell<Vec<usize>>,
    /// The causal slice open (causal.rs), by node and with what holds each: drawn in full, the rest faded. Set from the
    /// App as each frame is drawn.
    pub chain: RefCell<Option<HashSet<u32>>>,
    /// The threads of other items than the one he has gone into (orbit.rs `peripheral`): drawn as small faint pills.
    /// Set from the App as each frame is drawn.
    pub side: RefCell<Option<HashSet<u32>>>,
}

/// Where the Map is ([`View::here`]): the node the crumb trail ends at, the depth it is drawn at, and whether it is
/// only the one in view (none drawn at a depth, none selected), which a flight passes over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Here {
    pub node: u32,
    pub depth: Option<Depth>,
    pub in_view: bool,
}

/// What [`View::here`] reads: the camera, the canvas's size, the selection, the node at a depth and the graph's shape.
type HereFrom = (Camera, [f32; 2], Option<Name>, Option<(Name, Depth)>, (u64, usize));

/// What is under a node, by state: p715's badges ("what waits on you, and anything stopped or broken").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub waits: u32,
    pub held: u32,
    pub stopped: u32,
}

impl View {
    /// The Map's graph from now on, set by hand (the made-up record's, a test's): the window's projects are not laid over
    /// it. The camera stays where it is.
    pub fn set_graph(&mut self, g: Graph) {
        self.fixed = true;
        self.cloud = false;
        self.put(g);
    }

    /// The cloud's graph (cloud.rs `build`): nothing lit as a point, his posts being cards in their threads. The camera
    /// stays where it is, or, until he has moved it, shows the whole cloud again.
    pub fn set_cloud(&mut self, g: Graph) {
        self.fixed = false;
        self.cloud = true;
        self.put(g);
        self.lit.clear();
        if !self.touched {
            self.placed = false;
        }
    }

    /// The camera stays where it is through the graphs set next, as though he had moved it ([`View::set_cloud`]): the
    /// time bar's moment (cloud.rs `time_bar`), whose graph is the smaller the further back it is, so the picture stays
    /// still and what came later leaves its room empty.
    pub fn stay(&mut self) {
        self.touched = true;
    }

    /// The moment the cloud is drawn at ([`View::moment`]).
    pub fn moment(&self) -> Option<i64> {
        self.moment.get()
    }

    /// Draw the cloud at moment AT from now on (None: now).
    pub fn set_moment(&self, at: Option<i64>) {
        self.moment.set(at);
    }

    /// Whether the graph is the cloud's ([`View::set_cloud`]).
    pub fn is_cloud(&self) -> bool {
        self.cloud && self.graph.is_some()
    }

    /// The graph laid from the window's projects (join.rs), and each project node's badge as its layout counted it
    /// (BADGES). The camera stays where it is, or, until he has moved it, shows the whole record again.
    pub fn set_laid(&mut self, g: Graph, badges: Vec<(u32, Counts)>) {
        self.cloud = false;
        self.put(g);
        for (i, b) in badges {
            if let Some(c) = self.counts.get_mut(i as usize) {
                *c = b;
            }
        }
        if !self.touched {
            self.placed = false;
        }
    }

    /// Whether the graph was set by hand ([`View::set_graph`]).
    pub fn fixed(&self) -> bool {
        self.fixed
    }

    /// The graph grew by nodes inside what was drawn (the deep levels' steps and calls): nothing under them waits.
    fn grew(&mut self) {
        let n = self.graph.as_ref().map_or(0, |g| g.nodes().len());
        self.counts.resize(n, Counts::default());
    }

    fn put(&mut self, g: Graph) {
        // what the last frame held, by name: a relayout keeps the labels drawn, and what was drawn, so the poll that
        // lays places an open post's steps again (`deep_step`) and the frame after it draws them (consult-3 N-1479 p69)
        let kept: Vec<Name> = self.kept.borrow().iter().filter_map(|i| self.name(*i)).collect();
        let drawn: Vec<(Name, f32, Look)> = {
            let d = self.drawn.borrow();
            if d.0 == self.shape { d.1.iter().filter_map(|(i, w, l)| Some((self.name(*i)?, *w, *l))).collect() } else { Vec::new() }
        };
        self.lit = (0..g.nodes().len() as u32)
            .filter(|i| {
                let n = g.node(*i);
                (n.kind == Kind::Post && (n.his || n.ring.is_some())) || n.kind == Kind::Agent
            })
            .collect();
        let mut c = vec![Counts::default(); g.nodes().len()];
        for i in (0..g.nodes().len()).rev() {
            let n = &g.nodes()[i];
            // a region's own state is its lines': only what lies in it is counted
            let region = matches!(n.kind, Kind::Project | Kind::Topic);
            match n.state {
                _ if region => {}
                State::WaitsOnHim => c[i].waits += 1,
                // the badge's "16 held · limit" counts agents (an account held is what holds them)
                State::Held if n.kind == Kind::Agent => c[i].held += 1,
                State::Stopped => c[i].stopped += 1,
                _ => {}
            }
            if let Some(p) = n.parent {
                let x = c[i];
                let up = &mut c[p as usize];
                up.waits += x.waits;
                up.held += x.held;
                up.stopped += x.stopped;
            }
        }
        self.counts = c;
        self.graph = Some(Arc::new(g));
        self.shape += 1;
        *self.kept.borrow_mut() = kept.iter().filter_map(|n| self.pick(n)).collect();
        *self.drawn.borrow_mut() = (self.shape, drawn.into_iter().filter_map(|(n, w, l)| Some((self.pick(&n)?, w, l))).collect());
    }

    pub fn graph(&self) -> Option<&Arc<Graph>> {
        self.graph.as_ref()
    }

    /// What the canvas shows, in its points.
    pub fn size(&self) -> [f32; 2] {
        [self.area[2].max(1.0), self.area[3].max(1.0)]
    }

    /// The nodes the canvas draws now.
    pub fn shown(&self) -> Vec<Shown> {
        self.seen().0
    }

    /// The nodes the canvas draws now, and for each lit point merged into another's, the one standing for it. Nothing
    /// inside the node drawn at a depth ([`View::at_depth`]) is: its lines are drawn in its place, so no press, hover,
    /// crumb, edge or read of the deep levels (`deep_step`) reaches what it holds, its steps and their calls' wires
    /// (consult-3 N-1446 p1129).
    pub fn seen(&self) -> (Vec<Shown>, HashMap<u32, usize>) {
        let Some(g) = &self.graph else { return (Vec::new(), HashMap::new()) };
        let (shown, stand) = if self.cloud {
            let (mut shown, mut stand) = cloud_seen(g, &self.cam, self.size(), &self.lit, self.moment.get());
            if let Some((space, at)) = self.space.get() {
                *self.planes.borrow_mut() = super::spacetime::project(g, &space, at, self.cam.scale, self.size(), &mut shown, &mut stand);
            }
            if let Some(side) = self.side.borrow().as_ref() {
                super::orbit::compact(g, side, &mut shown, &mut stand);
            }
            if let Some(on) = self.chain.borrow().as_ref() {
                for x in shown.iter_mut().filter(|x| !on.contains(&x.i)) {
                    x.alpha *= super::causal::OFF;
                }
            }
            (shown, stand)
        } else {
            lod::visible_with(g, &self.cam, self.size(), &self.lit)
        };
        let Some((l, _)) = self.depth_at() else { return (shown, stand) };
        let inside = |mut i: u32| {
            while let Some(up) = g.node(i).parent {
                if up == l {
                    return true;
                }
                i = up;
            }
            false
        };
        // each one's place in what is left
        let (mut at, mut left) = (Vec::with_capacity(shown.len()), 0);
        for x in &shown {
            at.push(if inside(x.i) { None } else { left += 1; Some(left - 1) });
        }
        let stand = stand.into_iter().filter(|(i, _)| !inside(*i)).filter_map(|(i, k)| Some((i, at[k]?))).collect();
        (shown.into_iter().zip(&at).filter(|(_, k)| k.is_some()).map(|(x, _)| x).collect(), stand)
    }

    /// Where node I of G lies on screen as the canvas draws it: on one plane, the camera's; in the time view, on its
    /// plane (spacetime.rs `place`). None: older than the wake.
    pub fn on_screen(&self, g: &Graph, i: u32) -> Option<[f32; 4]> {
        let flat = self.cam.on_screen(&g.node(i).rect);
        let Some((space, at)) = self.space.get().filter(|_| self.cloud) else { return Some(flat) };
        let a = self.cam.on_screen(&g.node(super::spacetime::anchor(g, i)).rect);
        super::spacetime::place(g, &space, at, self.cam.scale, self.size(), i, flat, a)
    }

    /// The node drawn at a depth, in the graph now, and that depth.
    pub fn depth_at(&self) -> Option<(u32, Depth)> {
        self.at_depth.as_ref().and_then(|(n, l)| Some((self.pick(n)?, *l)))
    }

    /// Where the Map is: the node drawn at a depth and that depth, else the one selected, else the one whose place is
    /// in view ([`View::focus`]). The crumb trail ends at it ([`crumbs`]), and the structure's tree lights it, saying
    /// the depth in the crumbs' words (sidebar.rs `reveal_step`, `node_rows`; paper-research N-1446 p1126: "Moving or
    /// selecting on the Map lights and opens that node in the tree"; proto-window p1249 point 3).
    pub fn here(&self) -> Option<Here> {
        if let Some((node, l)) = self.depth_at() {
            return Some(Here { node, depth: Some(l), in_view: false });
        }
        if let Some(node) = self.selected() {
            return Some(Here { node, depth: None, in_view: false });
        }
        self.focus().map(|node| Here { node, depth: None, in_view: true })
    }

    /// [`View::here`], worked out again only when what it reads has moved since it last was: the tree asks on every
    /// poll while a pane draws the Map, and finding the node in view is a pass over what the canvas draws.
    pub(crate) fn here_kept(&mut self) -> Option<Here> {
        let from = (self.cam, self.size(), self.sel.clone(), self.at_depth.clone(), self.graph_rev());
        if let Some((f, i)) = &self.here_from
            && *f == from
        {
            return *i;
        }
        let i = self.here();
        self.here_from = Some((from, i));
        i
    }

    /// Which graph is set, and how many nodes it has grown to: what the structure's tree is built again on (sidebar.rs
    /// `sidebar`). `shape` moves on every graph set ([`View::put`]), as the graph's `Arc` is a new one each time, and
    /// is never the same twice, as an address the allocator gives the next graph once the last is freed may be; the
    /// deep levels' steps and calls grow the graph in place (`Arc::make_mut` in `deep_step`), so the count too.
    pub fn graph_rev(&self) -> (u64, usize) {
        (self.shape, self.graph.as_ref().map_or(0, |g| g.nodes().len()))
    }

    /// Node I's name.
    pub fn name(&self, i: u32) -> Option<Name> {
        let g = self.graph.as_ref()?;
        Some((g.node(g.root_of(i)).id.clone(), g.node(i).id.clone()))
    }

    /// The node NAME is, in the graph now.
    pub fn pick(&self, name: &Name) -> Option<u32> {
        let g = self.graph.as_ref()?;
        g.find_in(g.project(&name.0)?, &name.1)
    }

    /// The nodes the last frame labelled, by name (a test's look at what lod.rs `labels` held).
    #[cfg(test)]
    pub(crate) fn labelled(&self) -> std::collections::BTreeSet<Name> {
        self.kept.borrow().iter().filter_map(|i| self.name(*i)).collect()
    }

    pub fn selected(&self) -> Option<u32> {
        self.sel.as_ref().and_then(|n| self.pick(n))
    }

    pub fn counts(&self, i: u32) -> Counts {
        self.counts.get(i as usize).copied().unwrap_or_default()
    }

    /// The node whose place is in view: the deepest one drawn open that holds the middle of the canvas, else the
    /// selected one; what the crumb trail names and Esc leaves.
    pub fn focus(&self) -> Option<u32> {
        self.middle_open(&self.shown()).or_else(|| self.selected())
    }

    /// The deepest node of SHOWN (what the canvas draws now) drawn open that holds the middle of the canvas: the place
    /// in view, by which the depth switch is lit ([`depth_now`]).
    fn middle_open(&self, shown: &[Shown]) -> Option<u32> {
        let g = self.graph.as_ref()?;
        let [w, h] = self.size();
        let mid = self.cam.to_world([w / 2.0, h / 2.0]);
        let holds = |r: &Rect| mid[0] >= r.x && mid[0] <= r.right() && mid[1] >= r.y && mid[1] <= r.bottom();
        shown.iter().rev().find(|s| s.look == Look::Open && !s.lit && holds(&g.node(s.i).rect)).map(|s| s.i)
    }

    /// Fly to show R whole; at once when NOW is false (a test, a first placing).
    pub fn fly_to(&mut self, r: &Rect, now: bool) {
        let to = flown(r, self.size());
        if now {
            self.cam = to;
            self.flight = None;
        } else {
            self.fly_cam(to);
            self.fitting = Some(*r);
        }
    }

    /// Fly to camera TO: a change of the canvas's size while it flies fits nothing again (history.rs: the camera a
    /// place on the cloud was left at, paper-research N-1446 p2116).
    pub fn fly_cam(&mut self, to: Camera) {
        self.flight = Some((Flight::new(self.cam, to, self.size()), None));
        self.fitting = None;
    }

    /// The farthest and the nearest the camera zooms on this graph, as scales: out, one wheel notch past the whole
    /// record's fit, the first open's ([`View::whole`], [`laid_out`]); in, to where the smallest of its nodes fills the
    /// view, across and down, so drawn at least as large as the view every look lod.rs gives it is reached (a wide post
    /// fitted across is still a card: it opens at 560 points a side, lod.rs `band`). One rule from the fit, no tuned
    /// number (paper-research N-1446 p2013, 2; his N-1104 p869 "i zoomed out i think all the way or all way in": the
    /// camera went from 1e-5 to 1e4, nine orders of magnitude, most of them drawing nothing of his); within those two.
    /// None without a graph or a size.
    pub fn bounds(&self) -> Option<(f64, f64)> {
        let g = self.graph.as_ref()?;
        let size = self.size();
        if size[0] <= 1.0 || size[1] <= 1.0 {
            return None;
        }
        let far = (Camera::fitting(&self.whole()?, size, ROOM).scale / PER_NOTCH).max(FARTHEST);
        let fills = |r: &Rect| (size[0] as f64 / r.w).max(size[1] as f64 / r.h);
        let near = g.nodes().iter().filter(|n| n.rect.w > 0.0 && n.rect.h > 0.0).map(|n| fills(&n.rect)).fold(far, f64::max);
        Some((far, near.min(NEAREST)))
    }

    /// Whether the camera is a camera: a finite scale above none, at a finite place.
    fn finite(&self) -> bool {
        let c = &self.cam;
        c.scale.is_finite() && c.scale > 0.0 && c.at[0].is_finite() && c.at[1].is_finite()
    }

    /// Whether the camera draws any of the record: [`View::finite`], looking at some of the whole ([`View::whole`]). One
    /// that does not is put back on the whole in the frame ([`poll`]; paper-research N-1446 p2013, 3).
    fn drawable(&self) -> bool {
        self.finite() && self.whole().is_none_or(|w| self.cam.seen(self.size()).meets(&w))
    }

    /// Put the camera, once the canvas is laid out, with world point AT at its middle and SCALE points a unit.
    pub fn aim(&mut self, at: [f64; 2], scale: f64) {
        self.aim = Some((at, scale));
        // placed as asked: a lay after it leaves it there
        self.touched = true;
    }

    /// Whether a flight is on its way.
    pub fn flying(&self) -> bool {
        self.flight.is_some()
    }

    /// The whole record's rectangle: every project's.
    fn whole(&self) -> Option<Rect> {
        let g = self.graph.as_ref()?;
        let mut roots = g.roots().map(|i| g.node(i).rect);
        let first = roots.next()?;
        Some(roots.fold(first, |a, b| {
            let (x, y) = (a.x.min(b.x), a.y.min(b.y));
            Rect::new(x, y, a.right().max(b.right()) - x, a.bottom().max(b.bottom()) - y)
        }))
    }
}

/// Whether the page shown is the Map: a lens at the Cloud depth.
pub fn shown(s: &App) -> bool {
    matches!(&s.sel, crate::Sel::Lens(l) if l.depth == crate::lens::Depth::Cloud)
}

/// The one pane the Map is drawn in (panes.rs `area`): the focused one if it shows the Map, else the first that does.
/// One `View` holds one canvas's size, camera and hit tests, so a canvas in a second place would cull, fit and hit by
/// the other's size (proto-window p38 small 5, consult-3 p39). None: no pane shows it.
pub fn drawn_in(s: &App) -> Option<usize> {
    if shown(s) {
        return Some(s.focus);
    }
    let cloud = |sel: &crate::Sel| matches!(sel, crate::Sel::Lens(l) if l.depth == crate::lens::Depth::Cloud);
    (0..s.panes.len()).find(|j| *j != s.focus && cloud(&s.panes[*j].1))
}

/// The places of the words the Map's cards draw (`post_card`), each with its post's project, while the Map shows: the
/// texts held by reference that `texts::poll` asks for through that project's pipe (texts.rs `ask_each`), so which
/// cards draw words is decided here alone (proto-lead N-1479 p183).
pub(crate) fn worded(s: &App) -> Vec<(usize, docket_machine::Loc)> {
    if drawn_in(s).is_none() {
        return Vec::new();
    }
    s.map.worded.borrow().clone()
}

/// An unfocused pane J at the Map: the canvas if this is the Map's one place, else where it is.
pub fn apart(s: &App, j: usize, k: &Kit) -> El {
    if drawn_in(s) == Some(j) {
        return page(s, k);
    }
    let there = if shown(s) { "The Map is open in the focused pane." } else { "The Map is open in another pane." };
    col().key("map-elsewhere").w_full().padding(24.0).child(text(there).font(k.ty.small).color(k.c.text_faint))
}

/// The Map's frame step, from the window's poll: a flight's camera at CLOCK, and the next frame asked while it flies.
/// Whether the Map changed.
pub fn poll(s: &mut App, clock: f64, tx: &mut Tx) -> bool {
    let mut changed = false;
    // a pair's time over: its catcher goes, and a press where it was is the inspector's
    if s.map.pair.as_ref().is_some_and(|p| clock >= p.2 + PAIR) {
        s.map.pair = None;
        changed = true;
    }
    if drawn_in(s).is_some() && super::lanes::serves(s) {
        // a coordd that serves the graph: the cloud asks for its slices and builds the graph (cloud.rs); the timeline
        // page, where it is drawn still, its own (lanes.rs)
        changed |= if s.cloud.lanes { super::lanes::poll(s) } else { super::cloud::poll(s) | brain_step(s) };
        // the card "› on the Map" went to, flown into once a build drew it ([`on_map`]), while it is the focus
        if let Some(id) = s.cloud.fly.clone().filter(|_| !s.cloud.lanes) {
            if s.cloud.focus.as_deref() != Some(id.as_str()) {
                s.cloud.fly = None;
            } else if s.map.size()[0] > 1.0 && fly_to_id(s, s.cloud.of, &id) {
                s.cloud.fly = None;
                changed = true;
            }
        }
    } else if drawn_in(s).is_some() {
        // the corner's meters from the Accounts page's last answer; the islands laid again when what they are laid from
        // moved
        changed |= super::join::keep(s, chrono::Utc::now().timestamp());
        changed |= deep_step(s);
        // the lines of the node drawn at a depth, asked once (lens.rs `read`: not while held or on their way)
        if let Some((i, depth)) = s.map.depth_at()
            && let Some(g) = s.map.graph.clone()
            && let (Some(pi), Some(node)) = (project_at(s, &g, i), record_of(&g, i))
        {
            crate::lens::read(s, pi, depth, &node);
        }
    }
    let drawn = drawn_in(s).is_some();
    let v = &mut s.map;
    // a camera that draws nothing of the record (not a number, no scale, looking away) goes back on the whole, as the
    // first open placed it, so no frame shows an empty canvas with no way back (paper-research N-1446 p2013, 3); not
    // while he drags or a flight is on its way, unless it is not a camera ([`View::finite`]), which no flight leaves
    if drawn
        && v.placed
        && v.press.is_none()
        && (!v.flying() || !v.finite())
        && !v.drawable()
        && let Some(w) = v.whole()
    {
        v.fly_to(&w, true);
        changed = true;
    }
    let Some((f, from)) = v.flight.as_mut() else { return changed };
    let from = *from.get_or_insert(clock);
    let t = ((clock - from) / f.seconds()).clamp(0.0, 1.0);
    v.cam = f.at(t);
    if t >= 1.0 {
        v.flight = None;
    } else {
        tx.wake_after(std::time::Duration::ZERO);
    }
    true
}

/// The deep levels' frame step (deep.rs), from what the last frame drew: each post drawn open asks for its runs
/// (`deep::want`, once each) and holds its steps and their calls (`deep::place`, which places only what is new) and
/// its brain's marks with their parts (`deep::place_passes`, from the snapshot), and so does each handler's run drawn
/// open, of itself (`deep::want_run`); each step and call drawn asks what it reads
/// (`deep::near`, on every frame it is near: each read once a pipe). Whether the graph grew.
fn deep_step(s: &mut App) -> bool {
    let (shape, drawn) = s.map.drawn.borrow().clone();
    let Some(g) = s.map.graph.clone().filter(|_| shape == s.map.shape) else { return false };
    let mut open = Vec::new();
    let mut runs_open = Vec::new();
    for (i, px, look) in drawn {
        let n = g.node(i);
        let deep = matches!(n.kind, Kind::Step | Kind::Call);
        if !(deep || (card_kind(n.kind) && look == Look::Open)) {
            continue;
        }
        let Some(pi) = project_at(s, &g, i) else { continue };
        if deep {
            super::deep::near(s, pi, &g, i, px);
            continue;
        }
        if let Some(run) = super::deep::run_node(&n.id).filter(|_| n.kind == Kind::Run) {
            runs_open.push((pi, run.to_string(), i));
            continue;
        }
        let (Some(line), Some(post)) = (n.parent.map(|p| g.node(p).id.as_str()), n.id.rsplit_once(".p").and_then(|(_, x)| x.parse::<u32>().ok())) else { continue };
        if let Some(t) = s.projects[pi].live.tickets.values().find(|t| t.id == line) {
            open.push((pi, t.n, post, i));
        }
    }
    drop(g);
    let mut grew = false;
    for (pi, t, post, i) in open {
        super::deep::want(s, pi, t, post);
        let Some(mut g) = s.map.graph.take() else { break };
        let before = g.nodes().len();
        let p = &s.projects[pi];
        let v = &p.live.tickets[&t];
        // its runs' steps in its band, and its brain's marks, their parts inside them, at the right of its words
        let (runs, passes) = (super::deep::runs(s, pi, v, post), super::deep::passes(s, pi, v, post));
        if !(runs.is_empty() && passes.is_empty()) {
            let (gm, labels) = (Arc::make_mut(&mut g), crate::picture::labels(s, p));
            let placed = if runs.is_empty() { Ok(()) } else { super::deep::place(gm, i, &runs, &labels) };
            if let Err(e) = placed.and_then(|()| if passes.is_empty() { Ok(()) } else { super::deep::place_passes(gm, i, &passes, &labels) }) {
                crate::log::line(&format!("map: a step of {} not placed: {e:?}", g.node(i).id));
            }
        }
        grew |= g.nodes().len() > before;
        s.map.graph = Some(g);
    }
    for (pi, run, i) in runs_open {
        super::deep::want_run(s, pi, &run);
        let Some(mut g) = s.map.graph.take() else { break };
        let before = g.nodes().len();
        let p = &s.projects[pi];
        if let Some(v) = super::deep::held_run(s, pi, &run)
            && let Err(e) = super::deep::place(Arc::make_mut(&mut g), i, &[v], &crate::picture::labels(s, p))
        {
            crate::log::line(&format!("map: a step of {} not placed: {e:?}", g.node(i).id));
        }
        grew |= g.nodes().len() > before;
        s.map.graph = Some(g);
    }
    if grew {
        s.map.grew();
    }
    grew
}

/// The brain's step in the cloud (brain.rs): each agent of the cloud's project whose brain is bound has its node left of
/// the threads, and inside it its passes, each saying what it does now. It reads the snapshot only when it changed, or
/// the moment shown or the graph did (`View::brain_from`); the graph is copied and the view built again only when one of
/// its nodes changed (`brain::changes`). Whether one did.
fn brain_step(s: &mut App) -> bool {
    if !s.map.is_cloud() {
        return false;
    }
    let pi = s.cloud.of;
    let Some(p) = s.projects.get(pi) else { return false };
    let from = (pi, p.live.tickets_rev, p.live.agents_rev, s.tickets.find.fetched.get(&p.id).map_or(0, |m| m.len()), s.cloud.at, s.map.shape);
    if s.map.brain_from == Some(from) {
        return false;
    }
    s.map.brain_from = Some(from);
    let Some(mut g) = s.map.graph.take() else { return false };
    // the satellites borrow the snapshot; the graph, taken out, is placed in beside them
    let (changed, grew) = {
        let sats = super::brain::satellites(s, s.cloud.of);
        match project_id(s, s.cloud.of).and_then(|id| g.project(&id)) {
            Some(root) if !sats.is_empty() && super::brain::changes(&g, root, &sats) => {
                let grew = super::brain::place(Arc::make_mut(&mut g), root, &sats).unwrap_or_else(|e| {
                    crate::log::line(&format!("map: a brain's node not placed: {e:?}"));
                    false
                });
                (true, grew)
            }
            _ => (false, false),
        }
    };
    s.map.graph = Some(g);
    if grew {
        s.map.grew();
    }
    changed
}

/// The canvas laid out at R: the first time it has a size, the camera shows the whole record.
pub fn laid_out(s: &mut App, r: [f32; 4]) {
    let v = &mut s.map;
    let resized = (v.area[2] - r[2]).abs() > 0.5 || (v.area[3] - r[3]).abs() > 0.5;
    v.area = r;
    // flying to fit a node as the canvas changes size (the inspector opened by the double-click): it fits the new one
    if resized
        && v.flying()
        && let Some(f) = v.fitting
    {
        v.fly_to(&f, false);
    }
    if v.placed || r[2] <= 1.0 {
        return;
    }
    if let Some((at, scale)) = v.aim.take() {
        let [w, h] = v.size();
        v.cam = Camera { at: [at[0] - w as f64 / 2.0 / scale, at[1] - h as f64 / 2.0 / scale], scale };
        v.placed = true;
    } else if v.touched {
        // moved by him since the graph was set again unplaced (a post of his came, "Read again"): where he put it
        // stands. His press, flying into a card, opens the inspector, and the narrower canvas's layout put the camera
        // back on the whole Map, the flight gone (proto-window N-1446 p2419, 1)
        v.placed = true;
    } else if let Some(w) = v.whole() {
        v.fly_to(&w, true);
        v.placed = true;
    }
}

/// A press, a drag and their end on the canvas: a drag pans; a press that did not move selects what is under it (or
/// nothing), the second of a double-click flies into it. A click that selects a node waits for its second, which may
/// land on the inspector it opens ([`pair_catcher`]): a frame is asked for when it stops waiting.
pub fn drag(s: &mut App, d: &rune::Drag, tx: &mut Tx) {
    let at = [d.x, d.y];
    let v = &mut s.map;
    match d.phase {
        rune::Phase::Down => {
            v.flight = None;
            v.touched = true;
            v.press = Some((at, false));
            v.pair = None;
        }
        rune::Phase::Move => {
            let Some((last, moved)) = v.press else { return };
            let moved = moved || d.dx.hypot(d.dy) > CLICK;
            if moved {
                v.cam.shift([at[0] - last[0], at[1] - last[1]]);
                v.press = Some((at, true));
            }
        }
        rune::Phase::Up => {
            let moved = v.press.take().is_some_and(|(_, m)| m);
            if moved || d.cancelled() {
                return;
            }
            let shown = v.shown();
            let hit = lod::under(&shown, at).map(|k| shown[k].i);
            // Shift and a click: the card's causal slice (causal.rs)
            if let Some(i) = hit.filter(|_| s.map.cloud && tx.mods().shift) {
                super::causal::toggle(s, i);
                return;
            }
            if d.clicks >= 2 || (s.map.cloud && hit.is_some()) {
                // in the cloud a click goes in, as zooming in on it does (paper-research N-1104 p824); a block's opens
                // its text apart too (cloud.rs `pressed`, p1821 H3)
                match hit {
                    Some(i) => {
                        go_in(s, i);
                        if s.map.cloud {
                            super::cloud::pressed(s, i);
                        }
                    }
                    None => zoom(s, PER_NOTCH * PER_NOTCH, at),
                }
            } else {
                select(s, hit);
                s.map.pair = hit.and_then(|i| s.map.name(i)).map(|n| (n, d.window_point(), tx.clock()));
                if s.map.pair.is_some() {
                    tx.wake_at(tx.clock() + PAIR);
                }
            }
        }
    }
}

/// The id of project PI's node.
pub fn project_id(s: &App, pi: usize) -> Option<String> {
    s.projects.get(pi).map(|p| format!("project:{}", p.id))
}

/// The index in the window's projects of node I's project.
pub fn project_at(s: &App, g: &Graph, i: u32) -> Option<usize> {
    let root = &g.node(g.root_of(i)).id;
    (0..s.projects.len()).find(|pi| project_id(s, *pi).as_deref() == Some(root.as_str()))
}

/// While a pane draws the Map ([`drawn_in`]): select node ID of project PI and fly to it there (a row of the explorer,
/// a search's find), the focused pane left as it is when the Map is in another. Whether the Map took it; false when no
/// pane draws the Map or it has no such node.
pub fn fly_to_id(s: &mut App, pi: usize, id: &str) -> bool {
    if drawn_in(s).is_none() {
        return false;
    }
    if super::lanes::serves(s) && s.cloud.lanes {
        // the graph page: the row's thread is its focus, when it is the project it shows
        if pi == s.cur {
            super::lanes::focus(s, id);
        }
        return pi == s.cur;
    }
    let (Some(g), Some(p)) = (s.map.graph.clone(), project_id(s, pi)) else { return false };
    let Some(i) = g.project(&p).and_then(|r| g.find_in(r, id)) else { return false };
    go_in(s, i);
    true
}

/// Node ID on the Map, its focus: flown into through [`fly_to_id`] where the Map draws its card; else the cloud's focus
/// all the same, taken as a card he goes into takes it (cloud.rs `went_in`, which history.rs's `Place::focus` reads),
/// its slice asked at the cloud's next poll (paper-research N-1446 p1818, 2). A row of the prompt templates
/// (templates.rs) and the bar's Map from a thread ([`to_map`]) take him there.
pub fn on_map(s: &mut App, id: &str) {
    let pi = s.cur;
    s.show_sel(crate::Sel::Lens(crate::lens::Lens { filter: crate::lens::Filter::Threads, depth: Depth::Cloud, at: None }));
    if !fly_to_id(s, pi, id) {
        super::cloud::of_project(s, pi);
        super::cloud::went_in(s, id);
        s.cloud.fly = Some(id.to_string());
    }
}

/// The Map from where he is, as the bar's Docket opens at the scale of where he is (picture/docket.rs `open`): the
/// thread the page shows (the Threads page's, the one beside an agent's With you: history.rs `thread_shown`) is its card,
/// gone into ([`on_map`]); from any other page, the Map as he left it. On the Map already, it stays as it is.
pub fn to_map(s: &mut App) {
    if shown(s) {
        return;
    }
    let thread = s.projects.get(s.cur).and_then(|p| crate::history::thread_shown(s, s.cur, &s.sel).and_then(|t| crate::find::ticket(s, p, t))).map(|v| v.id.clone());
    match thread {
        Some(id) => on_map(s, &id),
        None => s.show_sel(crate::Sel::Lens(crate::lens::Lens { filter: crate::lens::Filter::Threads, depth: Depth::Cloud, at: None })),
    }
}

/// The bar's `Map`, after the Docket's (lib.rs): [`to_map`], the keys then the Map's ([`bar_press`]). Offered where the
/// project's coordd serves the graph, as the sidebar's Map row is (lanes.rs `serves`). paper-research N-1446 p2116, 4:
/// nothing on a thread or a chat led to its card, and with a thread open the sidebar's Map row was scrolled out of sight.
pub fn bar_mark(s: &App, k: &Kit) -> Option<El> {
    super::lanes::serves(s).then(|| rune_kit::Button::new("Map").variant(rune_kit::ButtonVariant::Ghost).help("The Map, at the thread shown").on_press_tx(bar_press(to_map)).build(k).key("map-mark"))
}

/// Select node I (none: nothing). Another node than the one drawn at a depth takes the Map back to Structure, as the
/// inspector, which shows the node selected, has it (proto-window N-1446 p1172 point 3: the crumbs and the switch stayed
/// on the depth); none selected leaves it there.
pub fn select(s: &mut App, i: Option<u32>) {
    if let (Some(i), Some((l, _))) = (i, s.map.depth_at())
        && l != i
    {
        s.map.at_depth = None;
    }
    s.map.sel = i.and_then(|i| s.map.name(i));
}

/// Fly into node I, selected, to fit it (p715: "goes in one level, centred on it").
pub fn go_in(s: &mut App, i: u32) {
    let Some(g) = s.map.graph.clone() else { return };
    // flown into another node than the one drawn at a depth, the Map is at Structure ([`select`])
    select(s, Some(i));
    s.map.touched = true;
    // the cloud's card whole, and it the focus whose slice is asked (cloud.rs `went_in`)
    let r = if s.map.cloud { cloud_fit(&g, i, s.map.size(), &s.map.lit) } else { fit_of(&g, i, s.map.size()) };
    s.map.fly_to(&r, false);
    if s.map.cloud {
        let id = g.node(i).id.clone();
        super::cloud::went_in(s, &id);
    }
}

/// Whether a node of kind K is drawn as a card at level 5 (deep.rs `card`): a post, a handler's run and a brain's
/// mark, whose cards are a post's (`post_card`). The one test of it, which `deep_step`, [`fit_of`], [`latest_column`],
/// `node` and [`word_card`] read (docket-edge6 N-1479 p219, N-1446 p1125).
fn card_kind(k: Kind) -> bool {
    matches!(k, Kind::Post | Kind::Run | Kind::Brain)
}

/// The cloud's nodes drawn through CAM in a view VIEW, LIT its lit points, as [`View::seen`] draws them: a card holding
/// none cut to its words as cloud.rs `card` draws them (paper-research N-1446 p1731), and each drawn no larger than
/// what it shows (lod.rs `fit_to_shown`, paper-research N-1446 p1839, 1).
fn cloud_seen(g: &Graph, cam: &Camera, view: [f32; 2], lit: &[u32], at: Option<i64>) -> (Vec<Shown>, HashMap<u32, usize>) {
    let words = |i: u32, w: f32| super::cloud::words_height(g, i, w);
    let (mut shown, stand) = lod::visible_at(g, cam, view, lit, &words, at);
    lod::fit_to_shown(g, cam, &mut shown, &stand, &words);
    (shown, stand)
}

/// What a flight into the cloud's card I of G fits in a view VIEW, LIT its lit points: the card where and as large as
/// it is drawn at that fit ([`cloud_seen`]), not as laid, so it fills the view as drawn (paper-research N-1446 p1883, 2:
/// still 6's bundle, framed at its laid size, took the top third of the view, two thirds empty under it). From the fit
/// of the laid card, the card as drawn at the fit before, until a fit draws it as it fits (to a hundredth of its
/// height), at most four times.
pub fn cloud_fit(g: &Graph, i: u32, view: [f32; 2], lit: &[u32]) -> Rect {
    let mut r = g.node(i).rect;
    for _ in 0..4 {
        let cam = Camera::fitting(&r, view, ROOM);
        let Some(x) = cloud_seen(g, &cam, view, lit, None).0.into_iter().find(|x| x.i == i && x.group.is_none() && !x.lit) else { break };
        let at = Rect::new(cam.at[0] + x.r[0] as f64 / cam.scale, cam.at[1] + x.r[1] as f64 / cam.scale, x.r[2] as f64 / cam.scale, x.r[3] as f64 / cam.scale);
        let near = |a: f64, b: f64| (a - b).abs() <= r.h * 0.01;
        if near(at.x, r.x) && near(at.y, r.y) && near(at.w, r.w) && near(at.h, r.h) {
            break;
        }
        r = at;
    }
    r
}

/// The camera a flight to show R whole lands at in a view VIEW ([`View::fly_to`]): R fitted in it with the flight's
/// room ([`ROOM`]) at each side. A still of the cloud aims there too (paths.rs `still_aim`; paper-research N-1446
/// p1894, 3: not at 0.92 of the view).
pub fn flown(r: &Rect, view: [f32; 2]) -> Camera {
    Camera::fitting(r, view, ROOM)
}

/// What a flight into node I of G fits in a view VIEW points wide and high:
/// - a post, its card (deep.rs `card`, level 5: its words and its steps), wherever on its row the double-click came
///   (docket-edge8 N-1479 p71, proto-window p80: the card is 5.6 of the row's 180 units);
/// - a line of more than one column (layout.rs `Rows`: past 256 posts, columns of 512 rows), the column holding its
///   latest post, as wide as it is, in the view's shape, its foot at that post's row: his latest posts, open, not the
///   line's whole card (docket-edge5 p112, proto-lead p118);
/// - any other node whole; or, where it would not open whole (a tall thread: N-1104 whole opened to 0.1 of its fade,
///   docket-edge6 p47), its top, as wide as it is, in the view's shape: its first posts, open.
fn fit_of(g: &Graph, i: u32, view: [f32; 2]) -> Rect {
    let n = g.node(i);
    let r = n.rect;
    let (vw, vh) = ((view[0] - 2.0 * ROOM).max(1.0) as f64, (view[1] - 2.0 * ROOM).max(1.0) as f64);
    if card_kind(n.kind) {
        return super::deep::card(r);
    }
    if let Some((c, last)) = latest_column(g, i) {
        let h = (c.w * vh / vw).min(c.h);
        return Rect::new(c.x, (last.bottom() - h).clamp(c.y, c.bottom() - h), c.w, h);
    }
    let open = lod::band_of(n).open;
    let side = lod::side(&Camera::fitting(&r, view, ROOM).on_screen(&r));
    if !open.is_finite() || side >= open * lod::FADE {
        return r;
    }
    Rect::new(r.x, r.y, r.w, (r.w * vh / vw).min(r.h))
}

/// Line I's column holding its latest post (the highest number), and that post's row; a handler's, its latest run's
/// (the last put: join.rs puts them oldest first); none for one of one column or of none drawn. A line's rect is its
/// rows (layout.rs `Rows::cell`: top-down, then the next column), so its columns are as wide as a row.
fn latest_column(g: &Graph, i: u32) -> Option<(Rect, Rect)> {
    let r = g.node(i).rect;
    let rows = g.children(i).iter().copied().filter(|j| card_kind(g.node(*j).kind));
    let last = rows.max_by_key(|j| (crate::lens::parts(&g.node(*j).id).1.unwrap_or(0), *j))?;
    let row = g.node(last).rect;
    let cols = (r.w / row.w).round();
    if !(cols >= 2.0) {
        return None;
    }
    let c = ((row.x - r.x) / row.w).floor().clamp(0.0, cols - 1.0);
    Some((Rect::new(r.x + c * row.w, r.y, row.w, r.h), row))
}

/// Fly out to what holds the place in view (Esc), the whole record from a project.
pub fn go_out(s: &mut App) {
    let Some(g) = s.map.graph.clone() else { return };
    s.map.touched = true;
    let up = s.map.focus().and_then(|f| g.node(f).parent);
    match up {
        Some(p) => s.map.fly_to(&g.node(p).rect, false),
        None => {
            if let Some(w) = s.map.whole() {
                s.map.fly_to(&w, false);
            }
        }
    }
}

/// Back to the whole record, as the Map first opened on it: Home, and the cloud's bar's "Whole map" (paper-research
/// N-1446 p2013, 1; his N-1104 p869: "i lost it … now its borked"). At once from a camera that is not one
/// ([`View::finite`]), which a flight from it would never leave.
pub fn go_home(s: &mut App) {
    let v = &mut s.map;
    let Some(w) = v.whole() else { return };
    v.touched = true;
    v.at_depth = None;
    let now = !v.finite();
    v.fly_to(&w, now);
}

/// After a press on a control of one of the Map's bars (the cloud's, the depth switch, the timeline's), the keys are
/// the Map's again: a press by the pointer leaves nothing focused, as a click on the canvas does, so Home, Esc, Enter,
/// + and - reach [`key`] (lib.rs `on_key` gives them to the Map only with nothing focused; paper-research N-1446 p2033,
/// 1: Home and Esc after "Whole map"). A control the keyboard drove, its ring showing, keeps the focus, so Tab and
/// Space go on along the bar.
pub fn keys_back(tx: &mut Tx) {
    if !tx.focus_visible() {
        tx.blur();
    }
}

/// A bar control's press F, then [`keys_back`]: the one rule every bar's controls are built with.
pub fn bar_press(f: impl Fn(&mut App) + 'static) -> impl Fn(&mut App, &mut Tx) + 'static {
    move |s: &mut App, tx: &mut Tx| {
        f(s);
        keys_back(tx);
    }
}

/// Zoom by FACTOR at Q, a point of the canvas, no farther out and no nearer in than [`View::bounds`]: a camera a flight
/// left beyond one zooms back toward them as before, never by a jump.
pub fn zoom(s: &mut App, factor: f64, q: [f32; 2]) {
    let v = &mut s.map;
    v.flight = None;
    v.touched = true;
    if !v.finite()
        && let Some(w) = v.whole()
    {
        v.fly_to(&w, true);
    }
    let (now, to) = (v.cam.scale, v.cam.scale * factor);
    let to = match v.bounds() {
        Some((_, near)) if factor > 1.0 => to.min(near.max(now)),
        Some((far, _)) => to.max(far.min(now)),
        None => to,
    };
    v.cam.zoom_about(to / now, q);
}

/// The wheel over the canvas: up zooms in at the pointer, a notch a step, a trackpad's small deltas adding up to it;
/// with Shift, over the cloud, it steps the time bar's ticks instead (cloud.rs `wheel_steps`; the owner N-1791 p1).
/// Always taken, so nothing under it scrolls and Ctrl does not zoom the window.
pub fn wheel(s: &mut App, e: &WheelEvent) -> bool {
    if e.mods.shift && s.map.is_cloud() {
        super::cloud::wheel_steps(s, e);
        return true;
    }
    if e.mods.alt && s.map.is_cloud() && s.cloud.space.mode == super::spacetime::Mode::Glass {
        s.cloud.space.turn(e.dx, e.dy);
        return true;
    }
    let d = if e.dy != 0.0 { e.dy } else { e.dx };
    let f = PER_NOTCH.powf((d / NOTCH) as f64);
    zoom(s, f, [e.x, e.y]);
    true
}

/// A pinch over the canvas.
pub fn magnify(s: &mut App, e: &MagnifyEvent) -> bool {
    zoom(s, (1.0 + e.delta as f64).max(0.05), [e.x, e.y]);
    true
}

/// The pointer moved over the canvas: what is under it is the one hovered. Whether the view must be built again: the
/// hovered one changed, or one is hovered, whose card follows the pointer; else the move is `live_only`.
pub fn pointed(s: &mut App, q: Option<[f32; 2]>) -> bool {
    let v = &mut s.map;
    v.pointer = q;
    let hover = q.filter(|_| !v.on_control).and_then(|q| {
        let shown = v.shown();
        lod::under(&shown, q).and_then(|k| v.name(shown[k].i))
    });
    let changed = hover != v.hover;
    v.hover = hover;
    changed || v.hover.is_some()
}

/// E, a control floating on the canvas (the depth switch, the minimap): while the pointer is over it, the canvas,
/// which hears the pointer over its children too, hovers nothing under it ([`View::on_control`]), and as it leaves,
/// what is under the pointer is hovered again, whichever of the two the window tells first.
pub(super) fn floating(e: El) -> El {
    e.on_hover_change(|s: &mut App, inside: bool, _tx: &mut Tx| {
        s.map.on_control = inside;
        let q = s.map.pointer;
        pointed(s, q);
    })
}

/// A key while the Map is shown: Esc flies out (from a node drawn at a depth, back up to Structure first, the camera
/// kept), Enter into the selected node, + and - zoom at the middle, Home back to the whole ([`go_home`]). Whether it
/// was the Map's.
pub fn key(s: &mut App, name: &rune::Key) -> bool {
    if super::lanes::serves(s) && s.cloud.lanes {
        return super::lanes::key(s, name);
    }
    let mid = { let [w, h] = s.map.size(); [w / 2.0, h / 2.0] };
    match name {
        // a step's open Rewind / Branch from here closes first, then an unfolded history folds (ribbon.rs), the
        // selected node's, else every one
        rune::Key::Escape if s.map.is_cloud() && s.cloud.rewind.is_some() => s.cloud.rewind = None,
        rune::Key::Escape if s.map.is_cloud() && s.cloud.causal.is_some() => s.cloud.causal = None,
        rune::Key::Escape if s.map.is_cloud() && !s.cloud.unfolded.is_empty() => {
            let id = s.map.selected().and_then(|i| s.map.graph().map(|g| g.node(i).id.clone()));
            if !id.is_some_and(|id| s.cloud.unfolded.remove(&id)) {
                s.cloud.unfolded.clear();
            }
        }
        rune::Key::Escape => {
            if s.map.at_depth.take().is_none() {
                go_out(s)
            }
        }
        // U on the selected card: its history unfolded or folded (ribbon.rs; the owner, N-1791 p1, 2)
        rune::Key::Text(c) if (c == "u" || c == "U") && s.map.is_cloud() => {
            let Some(id) = s.map.selected().and_then(|i| s.map.graph().filter(|g| super::ribbon::count(g, i, s.map.moment()) >= 2).map(|g| g.node(i).id.clone())) else { return false };
            super::ribbon::toggle(s, &id);
        }
        rune::Key::Enter => match s.map.selected() {
            Some(i) => go_in(s, i),
            None => return false,
        },
        rune::Key::Text(c) if c == "+" || c == "=" => zoom(s, PER_NOTCH, mid),
        rune::Key::Text(c) if c == "-" => zoom(s, 1.0 / PER_NOTCH, mid),
        rune::Key::Home => go_home(s),
        // over the cloud, ← → a tick of the time bar back and on (the owner N-1791 p1)
        rune::Key::Left | rune::Key::Right if s.map.is_cloud() => {
            if s.cloud.step(if matches!(name, rune::Key::Left) { -1 } else { 1 }) {
                s.map.set_moment(s.cloud.at);
                s.map.stay();
            }
        }
        _ => return false,
    }
    true
}

// ---- the drawing ---------------------------------------------------------------------------------------------------

/// The Map's page: the graph's (lanes.rs) where the project's coordd serves it, else the places'.
pub fn page(s: &App, k: &Kit) -> El {
    if super::lanes::serves(s) && s.cloud.lanes {
        return super::lanes::page(s, k);
    }
    let v = &s.map;
    v.set_moment(s.cloud.at);
    v.space.set(Some((s.cloud.space, s.cloud.at.unwrap_or_else(super::spacetime::clock))));
    *v.chain.borrow_mut() = v.graph.as_ref().and_then(|g| super::causal::held(s, g));
    *v.side.borrow_mut() = s.cloud.side.clone().filter(|_| s.cloud.space.pills);
    // where a coordd serves the graph, the cloud (cloud.rs): its bar over the Map's page, once it is built, and its time
    // bar under that
    let cloud = super::lanes::serves(s);
    if cloud && !v.is_cloud() {
        return col().w_full().h_full().child(super::cloud::bar(s, k)).child(super::cloud::waiting(s, k));
    }
    let Some(g) = v.graph.clone() else {
        return col().padding(24.0).child(text("The Map is placing the record.").font(k.ty.body).color(k.c.text_muted));
    };
    let sel = v.selected();
    let mut page = row().w_full().h_full().child(col().w(0.0).grow(1.0).h_full().child(crumbs(s, &g, k)).child(board(s, &g, k)));
    if let Some(i) = sel {
        page = page.child(inspector(s, &g, i, k));
    }
    if let Some(c) = pair_catcher(v) {
        page = page.child(c);
    }
    let page = page.on_layout(|s: &mut App, r| s.map.page = r);
    if cloud { col().w_full().h_full().child(super::cloud::bar(s, k)).child(super::cloud::time_bar(s, k)).child(page) } else { page }
}

/// Where a pair's second click is kept for the canvas. A click that selects a node while the inspector is closed opens
/// it at the canvas's right, over its last [`INSPECTOR`] points (the inspector takes its place in the page's row, and
/// the canvas narrows), so a second click there landed on the inspector, perhaps on a press of it, and nothing flew
/// (docket-edge6 N-1479 p176, proto-lead p177). While the pair waits, a catcher the size of a click's travel at its
/// first point, over the inspector, takes the second and flies into the node the first selected, as the canvas's
/// double-click does ([`drag`]). The poll drops the pair once [`PAIR`] has passed, and the catcher with it: a press
/// there after that is the inspector's.
fn pair_catcher(v: &View) -> Option<El> {
    let (_, at, _) = v.pair.as_ref()?;
    let [x, y, w, _] = v.page;
    (w > INSPECTOR && at[0] >= x + w - INSPECTOR && at[0] < x + w).then(|| {
        el().key("map-pair").absolute(at[0] - x - CLICK, at[1] - y - CLICK).size(2.0 * CLICK, 2.0 * CLICK).on_drag(|s: &mut App, d: &mut rune::Drag, tx: &mut Tx| {
            if matches!(d.phase, rune::Phase::Up) {
                second(s, d, tx.clock());
            }
        })
    })
}

/// A pair's second click, on its catcher at NOW: a click within [`PAIR`] of the first flies into the node it selected.
fn second(s: &mut App, d: &rune::Drag, now: f64) {
    let Some((n, _, t)) = s.map.pair.take() else { return };
    if d.cancelled() || d.dx.hypot(d.dy) > CLICK || now - t > PAIR {
        return;
    }
    if let Some(i) = s.map.pick(&n) {
        go_in(s, i);
    }
}

/// Ink of a node in state ST: the one accent for what waits on him (and his own, lit); the Kit's state colours for
/// held and stopped; the page's inks by how much it says.
fn ink(st: State, k: &Kit) -> Color {
    match st {
        State::WaitsOnHim => k.c.text_accent,
        State::Held => k.c.warning.fg,
        State::Stopped => k.c.danger.fg,
        State::Working => k.c.text_base,
        State::Plain => k.c.text_muted,
        State::Idle | State::Ended => k.c.text_faint,
    }
}

/// Node N's own ink: a post of his, lit, the accent; else its state's ([`ink`]).
fn own_ink(n: &Node, k: &Kit) -> Color {
    if n.his && n.kind == Kind::Post { k.c.text_accent } else { ink(n.state, k) }
}

pub(super) fn faded(c: Color, a: f32) -> Color {
    c.alpha(c.a * a.clamp(0.0, 1.0))
}

/// A label's width at the label's type.
fn width(t: &str, k: &Kit) -> f32 {
    rune::text::measure(t, &[], k.ty.small, None).0
}

/// The canvas.
fn board(s: &App, g: &Graph, k: &Kit) -> El {
    let v = &s.map;
    let view = v.size();
    let (shown, stand) = v.seen();
    let placed = lod::labels(g, &shown, &v.kept.borrow(), |t| width(t, k), LINE);
    *v.kept.borrow_mut() = placed.iter().map(|(x, _)| shown[*x].i).collect();
    *v.drawn.borrow_mut() = (v.shape, shown.iter().map(|x| (x.i, x.r[2], x.look)).collect());
    v.worded.borrow_mut().clear();
    *v.word_cards.borrow_mut() = shown.iter().filter_map(|x| word_card(g, x)).collect();
    let sel = v.selected();
    let hover = v.hover.as_ref().and_then(|n| v.pick(n));
    // not the window's page (lib.rs `view`): every press here is the Map's, a drag pans, a click selects or goes in, a
    // double-click flies in, so a card's words are no text to select here (a double-click would light a word as it
    // flew); they select in the inspector beside it and in the text a click opens
    let mut c = col().key("map-canvas").w_full().grow(1.0).h(0.0).clip_rounded(0.0).fill(k.c.bg_deep).cursor(rune::Cursor::Grab).select_texts(false);
    // edges under the nodes, one shape; the selected node's between two points over them ([`over_the_cards`])
    let (mut lines, mut over, mut lit) = (Shape::new(), Shape::new(), false);
    // in the cloud the card under the pointer lights its links as the selected one does, the rest dimmer (Obsidian's
    // hover, the study's 1c: N-1446 p1655; cloud.rs `link_ink`)
    let lights = if v.cloud { hover.or(sel) } else { sel };
    // in the cloud each edge lies just over the card that holds both its ends, under the cards it joins (cloud.rs
    // `holders`; paper-research N-1446 p1677: drawn under every card, the cloud's links were not seen), thin
    let drawn = lod::edges_at(g, &shown, &stand, lights, v.moment());
    let held = if v.cloud { super::cloud::holders(g, &shown, &drawn) } else { vec![None; drawn.len()] };
    let mut held_lines: HashMap<usize, Shape> = HashMap::new();
    for (e, h) in drawn.iter().zip(held) {
        let (a, b) = (&shown[e.from], &shown[e.to]);
        let w = if v.cloud { super::cloud::link_width(e.count) } else { 1.0 + (e.count as f32).log2().min(3.0) };
        if over_the_cards(e, &shown) {
            over = over.line(lod::dot_at(&a.r), lod::dot_at(&b.r), w, LineCap::Round, k.c.text_base);
            lit = true;
        } else {
            let ink = if v.cloud {
                super::cloud::link_ink(e.lit, hover.is_some(), a.alpha.min(b.alpha), k)
            } else if e.lit {
                k.c.text_base
            } else {
                faded(k.c.border_base, 0.7 * a.alpha.min(b.alpha))
            };
            let l = match h {
                Some(h) => held_lines.entry(h).or_default(),
                None => &mut lines,
            };
            *l = std::mem::take(l).line(lod::dot_at(&a.r), lod::dot_at(&b.r), w, LineCap::Round, ink);
        }
    }
    if v.cloud {
        c = c.children(v.space.get().and_then(|(sp, _)| super::spacetime::glass(&sp, &v.planes.borrow(), v.cam.scale, view, k)));
        // each item's threads in one outline, under the links and the cards (orbit.rs)
        c = c.children(super::orbit::hulls(s, g, &shown, view, k));
    }
    c = c.child(canvas(lines).absolute(0.0, 0.0).size(view[0], view[1]));
    // not in the cloud, whose step is a filled card (cloud.rs `card`) drawn under what it holds as every card is: drawn
    // last, it hid its prompt template (still 3 of his p815, paper-research N-1446 p1767)
    let above = |x: &Shown| !v.cloud && super::deep::over_calls(g.node(x.i).kind, x.r[2]);
    let draw = |c: El, x: &Shown| c.child(node(s, g, x, sel == Some(x.i), hover == Some(x.i), view, k).key(format!("map-n-{}", x.i)));
    // the links a card holds, over it and under what it holds
    let links_of = |c: El, at: usize, held_lines: &mut HashMap<usize, Shape>| match held_lines.remove(&at) {
        Some(l) => c.child(canvas(l).absolute(0.0, 0.0).size(view[0], view[1]).key(format!("map-links-{}", shown[at].i))),
        None => c,
    };
    for (at, x) in shown.iter().enumerate().filter(|(_, x)| !above(x)) {
        c = links_of(draw(c, x), at, &mut held_lines);
    }
    if lit {
        c = c.child(canvas(over).absolute(0.0, 0.0).size(view[0], view[1]).key("map-lit-edges"));
    }
    for (x, b) in placed.iter().filter(|_| !v.cloud) {
        let n = g.node(shown[*x].i);
        let a = shown[*x].alpha * lod::fade(lod::side(&shown[*x].r), lod::band(n.kind).label);
        c = c.child(text(lod::label_words(n).into_owned()).font(k.ty.small).color(faded(k.c.text_base, a)).single_line().truncate().absolute(b[0], b[1]).w(b[2]));
    }
    // a step drawing its stage over its calls, their labels and the lit edges, under the pointer's preview (deep.rs
    // `over_calls`; the Wire call's label lay over the editor, w2)
    for (at, x) in shown.iter().enumerate().filter(|(_, x)| above(x)) {
        c = links_of(draw(c, x), at, &mut held_lines);
    }
    // the node drawn at a depth: its lines over it, keyed by its name, not inside its frame, which is keyed by its index
    // in the graph ([`lines_key`])
    if let Some((x, l)) = v.depth_at().and_then(|(i, l)| shown.iter().find(|x| x.i == i).map(|x| (x, l))) {
        c = c.children(depth_lines(s, g, x, l, view, k));
    }
    // over the canvas's top left, as a map's layer control floats over it, under the preview that follows the hand: the
    // head is the crumbs', whose trail is as long as the node is deep (in a window 1,000 points wide with the inspector
    // open, the switch beside them left them about 20 points). At the left it stays where it is when the inspector
    // opens and the canvas narrows from the right, away from the minimap's corner; on the top edge it lies in the ROOM
    // every fit leaves above what it flies to (28 points, the switch's height), so nothing a fit shows lies under it
    if !v.cloud {
        c = c.child(floating(depth_switch(s, g, &shown, k).pin(Some(12.0), Some(0.0), None, None)));
    }
    // the brain's satellites beside its agents' nodes, at a size that reads at any zoom (brain.rs)
    if v.cloud {
        c = c.children(super::brain::drawn(s, g, view, k));
        // the unfolded histories' rails, over the cards and placing none of them (ribbon.rs)
        c = c.children(super::ribbon::drawn(s, g, view, k));
        // the causal slice's rail over the canvas's head (causal.rs)
        c = c.children(super::causal::drawn(s, g, view, k));
        // the live agents beside the threads they are on (orbit.rs)
        c = c.children(super::orbit::agents(s, g, &shown, k));
    }
    if let (Some(x), Some(q)) = (hover.and_then(|i| shown.iter().find(|x| x.i == i)), v.pointer) {
        c = c.child(preview(s, g, x, q, view, k));
    }
    c = c.child(floating(minimap(s, g, k)));
    c.on_layout(|s: &mut App, r| laid_out(s, r))
        .on_drag(|s: &mut App, d: &mut rune::Drag, tx: &mut Tx| drag(s, d, tx))
        .on_wheel(|s: &mut App, e: WheelEvent, _tx: &mut Tx| wheel(s, &e))
        .on_magnify(|s: &mut App, e: MagnifyEvent, _tx: &mut Tx| magnify(s, &e))
        .on_pointer_move_local(|s: &mut App, p: PointerEvent, tx: &mut Tx| {
            // nothing hovered before or now: the view reads nothing that moved, so it is not built again (every move
            // was: 24 ms each on his Map, docket-edge6 N-1446 p2356, 3)
            if !pointed(s, Some([p.x, p.y])) {
                tx.live_only();
            }
        })
        .on_hover_change(|s: &mut App, inside: bool, _tx: &mut Tx| {
            s.wheel_taken = inside;
            if !inside {
                pointed(s, None);
            }
        })
}

/// Whether edge E of SHOWN is drawn over the nodes: a lit one between two points. The Gateway's edge to a held agent's
/// point on its topic card then reaches the point rather than stopping at the card's edge under its fill (proto-window
/// N-1479 p68, small 1). An end on a card or an open node is drawn at its middle (lod.rs `dot_at`), where over the card
/// the edge would end at nothing: with research selected, seven of its edges did (p179). Those stay under the nodes,
/// lit, as before small 1.
pub(crate) fn over_the_cards(e: &lod::Drawn, shown: &[Shown]) -> bool {
    e.lit && shown[e.from].look == Look::Dot && shown[e.to].look == Look::Dot
}

/// Node X as its look draws it, in a view VIEW points wide and high.
fn node(s: &App, g: &Graph, x: &Shown, sel: bool, hover: bool, view: [f32; 2], k: &Kit) -> El {
    if s.map.cloud {
        return super::cloud::card(s, g, x, sel, hover, k);
    }
    let n = g.node(x.i);
    let st_ink = own_ink(n, k);
    let [dx, dy] = lod::dot_at(&x.r);
    if x.look == Look::Dot || x.lit {
        // a lit point standing for others near it is larger by how many; an asking post's is its ring
        let d = if x.lit { lod::lit_side(x.many) } else { DOT };
        let m = mark_side(n, d);
        let dot = mark(n, x.i, d, x.alpha, k).absolute(dx - m / 2.0, dy - m / 2.0);
        if !sel {
            return dot;
        }
        // selected: a ring around the dot, off it, in the text's ink (a lit dot's own ink is the accent)
        let o = m + 2.0 * RING_GAP + 4.0;
        return el()
            .absolute(dx - o / 2.0, dy - o / 2.0)
            .size(o, o)
            .child(el().absolute(0.0, 0.0).size(o, o).radius(o / 2.0).border(2.0, k.c.text_base).key("map-ring"))
            .child(dot.absolute(o / 2.0 - m / 2.0, o / 2.0 - m / 2.0));
    }
    let edge = if sel {
        k.c.border_focus
    } else if hover {
        k.c.border_strong
    } else {
        k.c.border_muted
    };
    let wide = if sel { 2.0 } else { 1.0 };
    let dot = |a: f32| el().size(DOT, DOT).radius(DOT / 2.0).fill(faded(st_ink, a));
    match x.look {
        Look::Label => el()
            .absolute(x.r[0], x.r[1])
            .size(x.r[2], x.r[3])
            .radius(k.radius.sm)
            .border(wide, faded(edge, x.alpha))
            .child(mark(n, x.i, DOT, x.alpha, k).absolute(dx - x.r[0] - mark_side(n, DOT) / 2.0, dy - x.r[1] - mark_side(n, DOT) / 2.0)),
        Look::Card | Look::Open => {
            let open = x.open;
            // a step drawn over its calls is unfilled, its panel filled (deep.rs `over_calls`)
            let fill = if super::deep::over_calls(n.kind, x.r[2]) {
                Color::TRANSPARENT
            } else if x.look == Look::Open {
                faded(k.c.layer_01, 0.55)
            } else {
                k.c.layer_01
            };
            let frame = el().size(x.r[2], x.r[3]).radius(k.radius.md).fill(faded(fill, x.alpha)).border(wide, faded(edge, x.alpha)).clip_rounded(k.radius.md);
            // a post's card at its row's start (deep.rs `card`, level 5), on screen as in the world: the scale is one; a
            // handler's run's the same (its stages in its band)
            let card = card_kind(n.kind).then(|| card_of(x));
            // what it says stays in the part of it in view: a post wider than the view still shows its line, after its
            // card (proto-window N-1479 p80: level 4's words lose the card's share of the row)
            let ox = (-x.r[0]).clamp(0.0, (x.r[2] - 160.0).max(0.0)).max(card.map_or(0.0, |c| c.w as f32));
            // (down too for a card; an open node's head stays at its top, over none of what it holds)
            let oy = if x.look == Look::Card { (-x.r[1]).clamp(0.0, (x.r[3] - 40.0).max(0.0)) } else { 0.0 };
            let inner = (x.r[2] - ox).min(view[0] - (x.r[0] + ox).max(0.0)).max(0.0) - 16.0;
            let region = x.look == Look::Open && matches!(n.kind, Kind::Project | Kind::Topic);
            let name = |font, a| text(n.label.clone()).font(font).color(faded(k.c.text_base, a)).single_line().truncate();
            if region {
                // a region's name and badge above its edge, out of the way of what it holds
                let head = row().absolute(ox, 0.0).w(inner.max(0.0)).h(HEAD).gap(8.0).items_center().child(dot(x.alpha)).child(name(k.ty.small_medium, x.alpha)).children(badge(s.map.counts(x.i), n.kind, x.alpha, f32::INFINITY, k));
                return el().absolute(x.r[0], x.r[1] - HEAD).size(x.r[2], x.r[3] + HEAD).child(head).child(frame.absolute(0.0, HEAD));
            }
            // what it says gives way to the card of a post it holds drawing its words under its name, as they fade in:
            // a line's first post at level 5, whose row starts at the line's corner (layout.rs `Rows::cell`; proto-lead
            // N-1479 p193); not a card it lies in, as a step does in its post's (deep.rs `inside`, path 5)
            let font = if x.look == Look::Open { k.ty.small_medium } else { k.ty.body_medium };
            let head = [x.r[0] + ox + 8.0, x.r[1] + oy + 8.0, inner.max(0.0), rune::text::measure("Ag", &[], font, None).1];
            let under = s.map.word_cards.borrow().iter().filter(|(i, r, _)| *i != x.i && lod::overlap(r, &head) && g.path(*i).contains(&x.i)).map(|(.., a)| *a).fold(0.0, f32::max);
            let ha = x.alpha * (1.0 - under);
            // drawn at a depth: its lines over it, in its card for a post or a run ([`depth_lines`], drawn by the board),
            // none of its words or of what it holds ([`View::seen`])
            let depth = s.map.depth_at().filter(|(l, _)| *l == x.i).map(|(_, l)| l);
            let mut says = col().absolute(ox + 8.0, oy + 8.0).w(inner.max(0.0)).gap(2.0).child(row().gap(6.0).items_center().child(mark(n, x.i, DOT, ha, k)).child(name(font, ha)));
            // its words on as many lines as the card's width takes, none cut (p68 small 2: "1 hel")
            says = says.children(badge(s.map.counts(x.i), n.kind, ha, inner, k));
            // a step's stage and prompt, a call's wire, once near enough (deep.rs `inside`; its label else); not at a
            // depth, where a call with no room for its lines says its name and does not ask for its wire (proto-window
            // N-1446 p1175)
            if depth.is_none()
                && matches!(n.kind, Kind::Step | Kind::Call)
                && let Some(pi) = project_at(s, g, x.i)
            {
                says = says.children(super::deep::inside(s, pi, g, x.i, x.r[2], k));
            }
            // what the card says fades as it opens, what it holds taking its place
            if !n.note.is_empty() && open < 1.0 {
                says = says.child(text(n.note.clone()).font(k.ty.small).color(faded(k.c.text_muted, ha * (1.0 - open))).max_lines(2).truncate());
            }
            let frame = frame.absolute(x.r[0], x.r[1]);
            let frame = match (card, depth) {
                (Some(c), None) => frame.child(post_card(s, g, x, [c.w as f32, c.h as f32], k)),
                (Some(c), Some(_)) => frame.child(card_edge(x, [c.w as f32, c.h as f32], k)),
                (None, _) => frame,
            };
            // at a depth its lines' head names it, and a card's row still says its line beside the card; a node with no
            // room for its lines ([`lines_room`]) says what it says, its name first (consult-3 N-1446 p1169: an empty
            // box, with no name, once the wheel or a pinch made it too small)
            let lined = depth.is_some() && lines_room(s, g, x, view).is_some();
            if ha > 0.0 && (card.is_some() || !lined) { frame.child(says) } else { frame }
        }
        Look::Dot => unreachable!(),
    }
}

/// Node N's (I's) mark, D points across as a dot, in A of its ink: a node's dot, or an asking post's ring ([`Ring`],
/// [`mark_side`] across), at every look its dot is drawn at. His ask open: a ring of the text's ink, empty; answered
/// by each agent it asks: his dot, his ink, in the ring. An agent's ask of him: the ring in the waits-on-you ink around
/// the post's own dot, in the page's. The accent's one role is "this needs you" (docs/specs/window-rules.md §2), so only the
/// ring that needs him takes it; his own asks wait on agents, or are done (proto-window N-1479 p315, p371). The fill
/// once his window holds what it asked waits for a typed fact (p9).
pub(super) fn mark(n: &Node, i: u32, d: f32, a: f32, k: &Kit) -> El {
    let own = own_ink(n, k);
    let Some(ring) = n.ring else { return el().size(d, d).radius(d / 2.0).fill(faded(own, a)) };
    let (o, inner) = (mark_side(n, d), (d - 2.0 * ASK_W).max(3.0));
    let (word, line) = match ring {
        Ring::Open => ("open", k.c.text_base),
        Ring::Answered => ("answered", k.c.text_base),
        Ring::ToHim => ("to-him", ink(State::WaitsOnHim, k)),
    };
    let m = el().absolute(0.0, 0.0).size(o, o).radius(o / 2.0).border(ASK_W, faded(line, a)).key(format!("map-ask-{word}-{i}"));
    let m = match ring {
        Ring::Open => m,
        Ring::Answered | Ring::ToHim => m.child(el().absolute((o - inner) / 2.0, (o - inner) / 2.0).size(inner, inner).radius(inner / 2.0).fill(faded(own, a))),
    };
    // held in one of its size, which a point's look keys as the node (`board`), the ring keeping its own key
    el().size(o, o).child(m)
}

/// The side of node N's [`mark`] for a dot D points across: an asking post's ring takes its line round the dot.
pub(super) fn mark_side(n: &Node, d: f32) -> f32 {
    if n.ring.is_some() { d + 2.0 * ASK_W } else { d }
}

/// Post X's card in X's own points (deep.rs `card`): at its row's start, on screen as in the world.
fn card_of(x: &Shown) -> Rect {
    super::deep::card(Rect::new(0.0, 0.0, x.r[2] as f64, x.r[3] as f64))
}

/// How strongly a post's card SIZE points on screen draws its words: from [`CARD_WORDS`] tall, faded in as it grows,
/// and 120 points wide; 0 when it draws none. `post_card` draws by it and `node`'s heads give way by it.
fn card_words(x: &Shown, size: [f32; 2]) -> f32 {
    let a = x.alpha * lod::fade(size[1], CARD_WORDS);
    if a > 0.0 && size[0] >= 120.0 { a } else { 0.0 }
}

/// Post X's card on screen where the frame draws it with its words ([`card_words`]), with how strongly: none for
/// another node, or a post drawn as a point. A handler's run's card too ([`card_kind`]), so a handler's name gives way
/// to its first run's words as a line's does to its first post's.
fn word_card(g: &Graph, x: &Shown) -> Option<(u32, [f32; 4], f32)> {
    let drawn = matches!(x.look, Look::Card | Look::Open) && !x.lit && card_kind(g.node(x.i).kind);
    let c = card_of(x);
    let a = card_words(x, [c.w as f32, c.h as f32]);
    (drawn && a > 0.0).then(|| (x.i, [x.r[0] + c.x as f32, x.r[1] + c.y as f32, c.w as f32, c.h as f32], a))
}

/// How tall on screen a post's card is when its words start to show: they fade in from here to [`lod::FADE`] times it.
const CARD_WORDS: f32 = 90.0;

/// Post X's card, SIZE points on screen at its row's start (deep.rs `card`; docket-edge8 N-1479 p71): its edge; once
/// tall enough ([`CARD_WORDS`]), its words in its top share (deep.rs `POST_TEXT`) as far as they fit, cut at a line,
/// then "Open its page", the whole post one press away and the card never scrolling (proto-window p80: about 25 lines
/// of 1,270 × 500 at the fit); in its band below, where its steps go, deep.rs `note` while they are being read or did
/// not come. Words held elsewhere say "…" until they are read: the card notes their places ([`worded`]), and the
/// window asks for them (texts.rs `ask_each`). A handler's run's card is this card (docket-edge8 N-1479 p208): in its
/// top share what the run came to and what it was for (its label and note: join.rs), in its band deep.rs `note_run`.
/// So is a brain's mark's (N-1446 p1125): its agent and what its pass came to (deep.rs `place_passes`), its parts in its
/// band. A post with a mark keeps its words left of its marks (deep.rs `marks_room`).
fn post_card(s: &App, g: &Graph, x: &Shown, size: [f32; 2], k: &Kit) -> El {
    let n = g.node(x.i);
    let [w, h] = size;
    let mut c = card_edge(x, size, k);
    // its words, press and note on a card they fit, checked before its post is looked up (proto-window p179 small 2)
    let a = card_words(x, size);
    if a <= 0.0 {
        return c;
    }
    let at = project_at(s, g, x.i);
    let top = h * super::deep::POST_TEXT as f32;
    let (line, small) = (rune::text::measure("Ag", &[], k.ty.body, None).1, rune::text::measure("Ag", &[], k.ty.small, None).1);
    // in its band, what is said of its steps while they are being read or did not come
    let band = |c: El, why: Option<String>| match why.filter(|_| h - top - 16.0 >= small) {
        Some(why) => {
            let fit = ((h - top - 16.0) / small).floor() as usize;
            c.child(text(why).font(k.ty.small).color(faded(k.c.text_muted, a)).max_lines(fit).absolute(8.0, top + 8.0).w(w - 16.0).key(format!("map-card-note-{}", x.i)))
        }
        None => c,
    };
    if matches!(n.kind, Kind::Run | Kind::Brain) {
        let lines = ((top - 16.0 - small - 4.0) / line).floor();
        if lines >= 1.0 {
            let said = text(n.label.clone()).font(k.ty.body).color(faded(k.c.text_base, a)).max_lines(lines as usize).key(format!("map-card-words-{}", x.i));
            let what = text(n.note.clone()).font(k.ty.small).color(faded(k.c.text_muted, a)).single_line().truncate();
            c = c.child(col().absolute(8.0, 8.0).w(w - 16.0).gap(4.0).child(said).child(what));
        }
        return band(c, at.zip(super::deep::run_node(&n.id)).and_then(|(pi, run)| super::deep::note_run(s, pi, run)));
    }
    let (t, post, _) = crate::lens::parts(&n.id);
    let found = at.and_then(|pi| Some((pi, s.projects.get(pi)?.live.tickets.values().find(|v| v.id == t)?))).and_then(|(pi, v)| Some((pi, v, v.posts.iter().find(|p| Some(p.n) == post)?)));
    let Some((pi, v, p)) = found else { return c };
    // its words as far as they fit above the press, whole lines; left of its brain's marks once it has one
    let lines = ((top - 16.0 - 24.0 - 8.0) / line).floor();
    if lines >= 1.0 {
        let mut held = Vec::new();
        crate::texts::locs_in(&serde_json::to_value(&p.text).unwrap_or_default(), &mut held);
        s.map.worded.borrow_mut().extend(held.into_iter().map(|l| (pi, l)));
        let id = n.id.clone();
        let marked = g.children(x.i).iter().any(|j| g.node(*j).kind == Kind::Brain);
        c = c.child(
            col()
                .absolute(8.0, 8.0)
                .w((if marked { w * super::deep::MARKS_FROM as f32 } else { w }) - 16.0)
                .gap(8.0)
                .child(text(crate::texts::drawn_in(s, pi, || crate::threads::words(&s.tickets, p))).font(k.ty.body).color(faded(k.c.text_base, a)).max_lines(lines as usize).key(format!("map-card-words-{}", x.i)))
                .child(rune_kit::Button::new("Open its page").variant(rune_kit::ButtonVariant::Ghost).on_press(move |s: &mut App| open_page(s, at, &id)).build(k).key(format!("map-card-open-{}", x.i))),
        );
    }
    band(c, super::deep::note(s, pi, v, p.n))
}

/// Post X's card, SIZE points on screen, as its edge: what [`post_card`] draws in, and the lines of a card drawn at a
/// depth ([`depth_lines`]).
fn card_edge(x: &Shown, size: [f32; 2], k: &Kit) -> El {
    el().key(format!("map-card-{}", x.i)).absolute(0.0, 0.0).size(size[0], size[1]).radius(k.radius.md).border(1.0, faded(k.c.border_muted, x.alpha))
}

/// Node X drawn at DEPTH (Events or Record) in a view VIEW points wide and high: its lines of the record
/// ([`record_of`]) as a lens's pane draws them, his Comment on them as there (lens.rs `lines_commented`; proto-window
/// N-1446 p1130), under a head naming them, over the part of it in view ([`lines_room`]), scrolling under the wheel:
/// rune gives the wheel to the scroll under the pointer before the canvas zooms. A child of the canvas ([`board`]),
/// keyed by X's name ([`lines_key`]). None where [`lines_room`] is.
fn depth_lines(s: &App, g: &Graph, x: &Shown, depth: Depth, view: [f32; 2], k: &Kit) -> Option<El> {
    let (pi, node, b) = lines_room(s, g, x, view)?;
    let n = g.node(x.i);
    let head = row()
        .gap(8.0)
        .items_center()
        .child(text(n.label.clone()).font(k.ty.small_medium).color(k.c.text_base).single_line().truncate().shrink())
        .child(text(depth.word()).font(k.ty.small).color(k.c.text_muted).single_line());
    let lines = col().key(lines_key(pi, &n.id, depth)).w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(crate::lens::lines_commented(s, pi, depth, &node, k));
    Some(col().key(format!("map-layer-{pi}-{}-{depth:?}", n.id)).absolute(b[0], b[1]).size(b[2], b[3]).gap(6.0).child(head).child(lines))
}

/// Where node X, drawn at a depth in a view VIEW points wide and high, draws its lines ([`depth_lines`]): its project,
/// its lines' name ([`record_of`]) and, in the canvas's points, the part of it in view (a post's or a run's, of its
/// card), 8 points in. None for a node with no lines, or where that part is smaller than [`LAYER_ROOM`]: the node then
/// says what it says, its name first ([`node`]).
fn lines_room(s: &App, g: &Graph, x: &Shown, view: [f32; 2]) -> Option<(usize, String, [f32; 4])> {
    let n = g.node(x.i);
    let (pi, node) = (project_at(s, g, x.i)?, record_of(g, x.i)?);
    let r = if card_kind(n.kind) { card_of(x) } else { Rect::new(0.0, 0.0, x.r[2] as f64, x.r[3] as f64) };
    // in X's points: the part of R in view, 8 points in
    let (x0, y0) = ((r.x as f32).max(-x.r[0]) + 8.0, (r.y as f32).max(-x.r[1]) + 8.0);
    let (x1, y1) = ((r.right() as f32).min(view[0] - x.r[0]) - 8.0, (r.bottom() as f32).min(view[1] - x.r[1]) - 8.0);
    if x1 - x0 < LAYER_ROOM[0] || y1 - y0 < LAYER_ROOM[1] {
        return None;
    }
    Some((pi, node, [x.r[0] + x0, x.r[1] + y0, x1 - x0, y1 - y0]))
}

/// The key of the scroll node ID of project PI draws its lines at DEPTH in ([`depth_lines`]): by its name, as a lens
/// pane's `lens-lines-{pi}-{node}-{depth}` is (lens.rs), so his place in them stays when the Map is laid again (join.rs
/// `keep`) and the node's index moves (proto-window N-1446 p1172 point 1). rune keeps a scroll's offset by its path.
pub(crate) fn lines_key(pi: usize, id: &str, depth: Depth) -> String {
    format!("map-layer-lines-{pi}-{id}-{depth:?}")
}

/// Node I's lines of the record, as lens.rs names a node ([`crate::lens::request`]): a thread's, an item's or a post's
/// by its id (the post's from it on); a handler's run's, its run's; a brain's mark's, its pass's ticket's (deep.rs
/// `brain_node`); a step's, its run's (a mark's part's, its pass's); a call's, its round's (`O-1099511630339.s1`, as
/// its wire is read: deep.rs `own`). None for any other node: coordd's record is per ticket.
fn record_of(g: &Graph, i: u32) -> Option<String> {
    let n = g.node(i);
    match n.kind {
        Kind::Thread | Kind::Item | Kind::Post => Some(n.id.clone()),
        Kind::Run => super::deep::run_node(&n.id).map(str::to_string),
        Kind::Brain => super::deep::brain_node(&n.id).map(str::to_string),
        Kind::Step => super::deep::stage_of(&n.id).map(|key| super::deep::run_of(key).to_string()),
        Kind::Call => Some(super::deep::own(&n.id).to_string()),
        _ => None,
    }
}

/// The call Wire flies into from node I: I itself, a call; a step's first; none for any other node.
fn wire_call(g: &Graph, i: u32) -> Option<u32> {
    match g.node(i).kind {
        Kind::Call => Some(i),
        Kind::Step => g.children(i).iter().copied().find(|c| g.node(*c).kind == Kind::Call),
        _ => None,
    }
}

/// The node a press on depth L applies to, of SHOWN (what the canvas draws now): the selected node, else the one
/// drawn at a depth, else the place in view ([`View::middle_open`]). For Events and Record, when it has lines of the
/// record ([`record_of`]); for Wire, the call it flies into ([`wire_call`]). None when there is none.
fn depth_target(s: &App, g: &Graph, shown: &[Shown], l: Depth) -> Option<u32> {
    let v = &s.map;
    let i = v.selected().or_else(|| v.depth_at().map(|x| x.0)).or_else(|| v.middle_open(shown))?;
    match l {
        Depth::Events | Depth::Record => record_of(g, i).map(|_| i),
        Depth::Wire => wire_call(g, i),
        Depth::Cloud | Depth::Structure => Some(i),
    }
}

/// The depth the Map is at, of SHOWN, which its switch lights: the node's drawn at a depth; else, by the place in view,
/// Wire where a call holding the canvas's middle draws its wire (level 7, deep.rs `CALL_PX`), Cloud over islands and
/// regions (levels 1–2: nothing open there, or a project), Structure between (a topic's threads down to a step).
fn depth_now(s: &App, g: &Graph, shown: &[Shown]) -> Depth {
    let v = &s.map;
    if let Some((_, l)) = v.depth_at() {
        return l;
    }
    let [w, h] = v.size();
    let mid = [w / 2.0, h / 2.0];
    let holds = |r: &[f32; 4]| mid[0] >= r[0] && mid[0] <= r[0] + r[2] && mid[1] >= r[1] && mid[1] <= r[1] + r[3];
    if shown.iter().any(|x| g.node(x.i).kind == Kind::Call && x.r[2] >= super::deep::CALL_PX && holds(&x.r)) {
        return Depth::Wire;
    }
    match v.middle_open(shown).map(|i| g.node(i).kind) {
        None | Some(Kind::Project) => Depth::Cloud,
        Some(_) => Depth::Structure,
    }
}

/// The depth switch's press: the Map at depth L, one press, in place on the same canvas (structure.md §2: "Going down
/// is one press, and coming back up is one press"; docket-edge6 N-1446 p1125):
/// - Cloud flies out to the whole record, as the "All" crumb does.
/// - Structure brings the node drawn at a depth back to its words and what it holds, the camera kept; from a call's
///   wire it goes up to the call's step, as Esc does; from the cloud into the selected node, as a double-click does.
/// - Events and Record draw the lines of the node they apply to ([`depth_target`]) in its place, fitted ([`fit_of`]).
/// - Wire flies into the call ([`wire_call`]), which draws its wire (deep.rs `inside`).
pub(crate) fn to_depth(s: &mut App, l: Depth) {
    let Some(g) = s.map.graph.clone() else { return };
    let shown = s.map.shown();
    let now = depth_now(s, &g, &shown);
    s.map.touched = true;
    match l {
        Depth::Cloud => {
            s.map.at_depth = None;
            if let Some(w) = s.map.whole() {
                s.map.fly_to(&w, false);
            }
        }
        Depth::Structure => match now {
            Depth::Events | Depth::Record => s.map.at_depth = None,
            Depth::Wire => go_out(s),
            Depth::Cloud => {
                if let Some(i) = s.map.selected().or_else(|| s.map.middle_open(&shown)) {
                    go_in(s, i);
                }
            }
            Depth::Structure => {}
        },
        Depth::Events | Depth::Record => {
            let Some(i) = depth_target(s, &g, &shown, l) else { return };
            s.map.at_depth = s.map.name(i).map(|n| (n, l));
            select(s, Some(i));
            let r = fit_of(&g, i, s.map.size());
            s.map.fly_to(&r, false);
        }
        Depth::Wire => {
            let Some(c) = depth_target(s, &g, &shown, l) else { return };
            s.map.at_depth = None;
            go_in(s, c);
        }
    }
}

/// A press in the lines the Map draws in a node's card ([`depth_lines`]) down to DEPTH at NODE of project PI (lens.rs
/// `zoom_in`: an event's line to Record, its step's "Wire ›" to the call), taken in place: the card then draws its
/// Record, or the call is flown into and draws its wire. Whether the Map took it: not when the Map is not the page
/// shown (the lines pressed were a lens pane's: a press in a pane focuses it first), nor for a call not placed in the
/// node, which opens in the lens as a call's lines pressed on the Map do.
pub(crate) fn pressed_down(s: &mut App, pi: usize, depth: Depth, node: &str) -> bool {
    let (Some(g), Some((i, _))) = (s.map.graph.clone(), s.map.depth_at()) else { return false };
    if !shown(s) || project_at(s, &g, i) != Some(pi) {
        return false;
    }
    if depth == Depth::Record && record_of(&g, i).as_deref() == Some(node) {
        s.map.at_depth = s.map.name(i).map(|n| (n, Depth::Record));
        return true;
    }
    if depth != Depth::Wire {
        return false;
    }
    let mut under = vec![i];
    while let Some(j) = under.pop() {
        let n = g.node(j);
        if n.kind == Kind::Call && super::deep::own(&n.id) == node {
            s.map.at_depth = None;
            go_in(s, j);
            return true;
        }
        under.extend_from_slice(g.children(j));
    }
    false
}

/// Node ID's own page (Open its page, in the inspector and on a post's card): the Threads lens at Structure, the
/// thread open at the post ID names; its project AT made current first, so an N-12 of cav-paper opens cav-paper's,
/// not research's (consult-3 N-1479 p16).
fn open_page(s: &mut App, at: Option<usize>, id: &str) {
    if let Some(pi) = at {
        s.cur = pi;
    }
    crate::lens::zoom(s, crate::lens::Depth::Structure, id)
}

/// A project's or a topic's badge: what waits on him, what is held, what is stopped under it (p715 level 1). Its words
/// go on as many lines as W points take, each whole (proto-window N-1479 p68 small 2: a card's read "1 hel"); a
/// wrapping row did not wrap in a card (line32), so the words are measured, as the labels are (`width`).
fn badge(c: Counts, kind: Kind, a: f32, w: f32, k: &Kit) -> Option<El> {
    if !matches!(kind, Kind::Project | Kind::Topic) {
        return None;
    }
    let mut words = Vec::new();
    if c.waits > 0 {
        words.push((format!("{} waits on you", c.waits), k.c.text_accent));
    }
    if c.held > 0 {
        words.push((format!("{} held · limit", c.held), k.c.warning.fg));
    }
    if c.stopped > 0 {
        words.push((format!("{} stopped", c.stopped), k.c.danger.fg));
    }
    if words.is_empty() {
        return None;
    }
    const GAP: f32 = 8.0;
    let mut lines: Vec<Vec<(String, Color)>> = Vec::new();
    let mut used = 0.0;
    for (t, c) in words {
        let tw = rune::text::measure(&t, &[], k.ty.small_medium, None).0;
        match lines.last_mut() {
            Some(l) if used + GAP + tw <= w => {
                l.push((t, c));
                used += GAP + tw;
            }
            _ => {
                lines.push(vec![(t, c)]);
                used = tw;
            }
        }
    }
    let line = |l: Vec<(String, Color)>| row().gap(GAP).children(l.into_iter().map(|(t, c)| text(t).font(k.ty.small_medium).color(faded(c, a)).single_line()));
    Some(match lines.len() {
        1 => line(lines.pop().unwrap()),
        _ => col().gap(2.0).children(lines.into_iter().map(line)),
    })
}

/// The two lines a hover shows beside the pointer Q (p715: "a two-line preview, no click needed"): the point's label,
/// then what it is ([`hover_says`]). On rune-kit's popover surface, [`rune_kit::popover::raised`], as find's, the
/// circle's and the message box's panels are: opaque, so the edges and the cards under it do not read through its
/// words, as they did through `layer_02`, the neutral kit's `control_track`, white at 5% in dark (consult-3 N-1479
/// p186, proto-lead p193, proto-window p242).
fn preview(s: &App, g: &Graph, at: &Shown, q: [f32; 2], view: [f32; 2], k: &Kit) -> El {
    let n = g.node(at.i);
    let (w, h) = (280.0, 52.0);
    let x = if q[0] + 14.0 + w > view[0] { q[0] - 14.0 - w } else { q[0] + 14.0 };
    let y = if q[1] + 14.0 + h > view[1] { q[1] - 14.0 - h } else { q[1] + 14.0 };
    let second = hover_says(s, g, at);
    let card = col()
        .key("map-preview")
        .absolute(x.max(0.0), y.max(0.0))
        .w(w)
        .padding(8.0)
        .gap(2.0)
        .child(text(n.label.clone()).font(k.ty.small_medium).color(k.c.text_base).single_line().truncate())
        .child(text(second).font(k.ty.small).color(k.c.text_muted).single_line().truncate());
    rune_kit::popover::raised(k, card, k.radius.md)
}

/// What a hover says of the point AT under its label (paper-research N-1479 p65: an unlabelled point says nothing
/// at level 1 else): its kind first; then its own note where it has one (a post's number, by whom and when; an
/// account's meter; a held agent's limit); a region what its ink stands for, how many items and threads it holds and
/// its badge; any other its ink in words. A lit point standing for others of its kind near it says how many. A node of
/// a slice says its kind in its note already ([`said_kind`]).
fn hover_says(s: &App, g: &Graph, at: &Shown) -> String {
    let n = g.node(at.i);
    let mut w = match n.kind {
        Kind::Node(_) if !n.note.is_empty() => Vec::new(),
        _ => vec![kind_word(n.kind).to_string()],
    };
    if matches!(n.kind, Kind::Project | Kind::Topic) {
        // the badge says what waits on him
        w.extend(state_word(n.state).filter(|_| n.state != State::WaitsOnHim).map(str::to_string));
        let (items, threads) = lines_in(g, at.i);
        let count = |n: usize, one: &str| match n {
            0 => None,
            1 => Some(format!("1 {one}")),
            n => Some(format!("{n} {one}s")),
        };
        w.extend(count(items, "item"));
        w.extend(count(threads, "thread"));
        let c = s.map.counts(at.i);
        w.extend((c.waits > 0).then(|| format!("{} waits on you", c.waits)));
        w.extend((c.held > 0).then(|| format!("{} held", c.held)));
        w.extend((c.stopped > 0).then(|| format!("{} stopped", c.stopped)));
    } else if !n.note.is_empty() {
        w.push(n.note.clone());
    } else {
        w.extend(state_word(n.state).map(str::to_string));
    }
    if at.lit && at.many > 1 {
        w.push(format!("{} more here", at.many - 1));
    }
    w.join(" · ")
}

/// The items and threads inside region I (a project's through its topics).
fn lines_in(g: &Graph, i: u32) -> (usize, usize) {
    let mut c = (0, 0);
    for &j in g.children(i) {
        match g.node(j).kind {
            Kind::Item => c.0 += 1,
            Kind::Thread => c.1 += 1,
            Kind::Topic => {
                let (a, b) = lines_in(g, j);
                c = (c.0 + a, c.1 + b);
            }
            _ => {}
        }
    }
    c
}

/// What a state's ink ([`ink`]) stands for, in words; none for the page's own.
fn state_word(st: State) -> Option<&'static str> {
    match st {
        State::Plain => None,
        State::WaitsOnHim => Some("waits on you"),
        State::Working => Some("at work"),
        State::Idle => Some("idle"),
        State::Held => Some("held"),
        State::Stopped => Some("stopped"),
        State::Ended => Some("ended"),
    }
}

/// What a kind is called in words.
pub fn kind_word(k: Kind) -> &'static str {
    kind_words(k).0
}

/// What node N is called: a slice's node as its note begins, which reads its edges (cloud.rs `focus_card`, lanes.rs
/// `node_word`: a text a start was started with is a system prompt); any other its kind's word. The inspector says it,
/// and so does the left bar's row of it in the structure (sidebar.rs `node_detail`).
pub(crate) fn said_kind(n: &Node) -> &str {
    match n.kind {
        Kind::Node(_) if !n.note.is_empty() => n.note.split(" · ").next().unwrap_or_default(),
        _ => kind_word(n.kind),
    }
}

/// What a kind is called in words, one of it and more: coordd's kinds as lanes.rs `kind_words` says them ("branch",
/// "branches": a group's card said "and 3 more branchs" in p1885's still 1, proto-window N-1446 p1951).
pub fn kind_words(k: Kind) -> (&'static str, &'static str) {
    match k {
        Kind::Project => ("project", "projects"),
        Kind::Topic => ("topic", "topics"),
        Kind::Build => ("build", "builds"),
        Kind::Account => ("account", "accounts"),
        Kind::Agent => ("agent", "agents"),
        Kind::Item => ("item", "items"),
        Kind::Thread => ("thread", "threads"),
        Kind::Handler => ("handler", "handlers"),
        Kind::Run => ("run", "runs"),
        Kind::Post => ("post", "posts"),
        Kind::Brain => ("brain", "brains"),
        Kind::Step => ("step", "steps"),
        Kind::Call => ("call", "calls"),
        Kind::Turn => ("turn", "turns"),
        Kind::Text => ("text", "texts"),
        Kind::Node(k) => super::lanes::kind_words(k),
    }
}

/// The crumb trail over the canvas: the chain of parents of the node drawn at a depth, then that depth ("… › p586 ›
/// Events"); else of the selected node, else of the one whose place is in view; each flying to its node.
fn crumbs(s: &App, g: &Graph, k: &Kit) -> El {
    let mut r = row().key("map-crumbs").w_full().h(32.0).px(12.0).gap(4.0).items_center();
    let here = s.map.here();
    let depth = here.and_then(|h| h.depth);
    let trail = here.map(|h| g.path(h.node)).unwrap_or_default();
    r = r.child(crumb("All", None, trail.is_empty(), k));
    for (n, i) in trail.iter().enumerate() {
        r = r.child(text("›").font(k.ty.small).color(k.c.text_faint));
        r = r.child(crumb(&g.node(*i).label, Some(*i), n + 1 == trail.len() && depth.is_none(), k));
    }
    if let Some(l) = depth {
        r = r.child(text("›").font(k.ty.small).color(k.c.text_faint));
        r = r.child(text(l.word()).font(k.ty.small_medium).color(k.c.text_base).single_line().key("map-crumb-layer"));
    }
    r
}

/// The depth switch (structure.md §2: "A lens chooses its layer and its filter. Going down is one press, and coming
/// back up is one press"): Cloud · Structure · Events · Record · Wire, lens.rs's five, the depth the Map is at lit
/// ([`depth_now`]) from SHOWN (what the canvas draws), a press going there ([`to_depth`]). Events and Record are a
/// thread's, an item's, a post's, a run's, a step's or a call's, Wire a step's or a call's ([`depth_target`]): off
/// with nothing such to apply to, and its help says why, naming them (it is the whole control's, shown over any of
/// its segments).
fn depth_switch(s: &App, g: &Graph, shown: &[Shown], k: &Kit) -> El {
    let at = depth_now(s, g, shown);
    let names: Vec<&str> = Depth::ALL.iter().map(|l| l.word()).collect();
    let mut sw = rune_kit::SegmentedControl::new(&names, Depth::ALL.iter().position(|l| *l == at)).label("Depth").fit();
    let off = |l: Depth| depth_target(s, g, shown, l).is_none();
    let (events, wire) = (off(Depth::Events), off(Depth::Wire));
    for (i, l) in Depth::ALL.iter().enumerate() {
        if (matches!(l, Depth::Events | Depth::Record) && events) || (*l == Depth::Wire && wire) {
            sw = sw.segment_state(i, rune_kit::State::Disabled);
        }
    }
    // a press gives the keys back ([`keys_back`]); Esc while a segment holds the focus the keyboard gave it is still
    // the Map's, one depth up ([`key`]): the window's own keys are the Map's only with nothing focused (lib.rs `on_key`)
    let sw = sw
        .on_pick(|s: &mut App, i, tx: &mut Tx| {
            to_depth(s, Depth::ALL[i]);
            keys_back(tx);
        })
        .build(k)
        .key("map-layers");
    let sw = sw.on_key(|s: &mut App, e: &rune::KeyEvent, _: &mut Tx| matches!(e.key, rune::Key::Escape) && key(s, &e.key));
    match (events, wire) {
        (true, _) => sw.help("Select a thread, post or step"),
        (false, true) => sw.help("Wire: select a step or a call"),
        (false, false) => sw,
    }
}

fn crumb(label: &str, i: Option<u32>, last: bool, k: &Kit) -> El {
    let t = text(label.to_string()).font(if last { k.ty.small_medium } else { k.ty.small }).color(if last { k.c.text_base } else { k.c.text_muted }).single_line();
    k.interactive(el().px(6.0).py(2.0).radius(k.radius.sm).child(t)).cursor(rune::Cursor::Pointer).on_press(move |s: &mut App| {
        s.map.touched = true;
        s.map.at_depth = None;
        match i {
            Some(i) => {
                if let Some(g) = s.map.graph.clone() {
                    s.map.fly_to(&g.node(i).rect, false);
                }
            }
            None => {
                if let Some(w) = s.map.whole() {
                    s.map.fly_to(&w, false);
                }
            }
        }
    })
}

/// The minimap in the canvas's corner: the projects and their topics, the view's rectangle over them; a press flies
/// the middle of the view there.
fn minimap(s: &App, g: &Graph, k: &Kit) -> El {
    let v = &s.map;
    let Some(whole) = v.whole() else { return el() };
    let view = v.size();
    let m = Camera::fitting(&whole, MINI, 6.0);
    let mut sh = Shape::new();
    let rect = |sh: Shape, r: [f32; 4], ink: Color| {
        let (x0, y0, x1, y1) = (r[0], r[1], r[0] + r[2], r[1] + r[3]);
        sh.fill(vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]], ink)
    };
    for p in g.roots() {
        sh = rect(sh, m.on_screen(&g.node(p).rect), k.c.layer_02);
        for t in g.children(p) {
            sh = rect(sh, m.on_screen(&g.node(*t).rect), k.c.layer_03);
        }
    }
    let seen = m.on_screen(&v.cam.seen(view));
    let (x0, y0, x1, y1) = (seen[0], seen[1], seen[0] + seen[2], seen[1] + seen[3]);
    sh = sh.stroke_with(vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]], true, 1.5, LineCap::Round, k.c.text_base);
    let (x, y) = ((view[0] - MINI[0] - 12.0).max(0.0), (view[1] - MINI[1] - 12.0).max(0.0));
    el().key("map-mini")
        .absolute(x, y)
        .size(MINI[0], MINI[1])
        .radius(k.radius.md)
        .fill(faded(k.c.layer_01, 0.94))
        .border(1.0, k.c.border_base)
        .clip_rounded(k.radius.md)
        .cursor(rune::Cursor::Pointer)
        .child(canvas(sh).absolute(0.0, 0.0).size(MINI[0], MINI[1]))
        .on_drag(move |s: &mut App, d: &mut rune::Drag, _tx: &mut Tx| {
            if d.phase == rune::Phase::Up && !d.cancelled() {
                let to = m.to_world([d.x, d.y]);
                let v = &mut s.map;
                v.touched = true;
                let [w, h] = v.size();
                let mut c = v.cam;
                c.at = [to[0] - w as f64 / 2.0 / c.scale, to[1] - h as f64 / 2.0 / c.scale];
                v.flight = Some((Flight::new(v.cam, c, [w, h]), None));
                v.fitting = None;
            }
        })
}

/// The inspector at the Map's right: node I's card, where it is, what is under it, and what it does.
fn inspector(s: &App, g: &Graph, i: u32, k: &Kit) -> El {
    let n = g.node(i);
    let trail: Vec<String> = g.path(i).iter().map(|p| g.node(*p).label.clone()).collect();
    let c = s.map.counts(i);
    let mut col_ = col()
        .key("map-inspector")
        .w(INSPECTOR)
        .min_w(INSPECTOR)
        .h_full()
        .padding(16.0)
        .gap(8.0)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_muted)
        .child(text(said_kind(n).to_string()).font(k.ty.small).color(k.c.text_faint))
        .child(text(n.label.clone()).font(k.ty.body_medium).color(k.c.text_base).wrap())
        .child(text(trail.join(" › ")).font(k.ty.small).color(k.c.text_muted).wrap());
    if !n.note.is_empty() {
        col_ = col_.child(text(n.note.clone()).font(k.ty.body).color(k.c.text_base).wrap());
    }
    if c != Counts::default() {
        col_ = col_.children(badge(c, Kind::Topic, 1.0, INSPECTOR - 32.0, k));
    }
    let (id, kind) = (n.id.clone(), n.kind);
    // the gateway's cooldown is cleared on its card, which the Accounts tab draws: one press more than p715's (the
    // window does not hold the gateway's own clear; proto-lead N-1479 p55), the press the message box's Accounts makes:
    // in a build with the gateway that is its Accounts page, in its tab (`ext::tab_page`)
    if kind == Kind::Account && id == "account:gateway" {
        col_ = col_.child(text("Clear its cooldown in Accounts.").font(k.ty.small).color(k.c.text_muted).wrap());
        col_ = col_.child(rune_kit::Button::new("Open the Accounts tab").on_press(|s: &mut App| s.sel = crate::ext::tab_page("accounts")).build(k));
    } else if kind == Kind::Account && n.state == State::Held {
        // a held account's limit, cleared as its card on the Accounts tab clears it (p715 path 2)
        let account = id.strip_prefix("account:").unwrap_or(&id).to_string();
        // as the card's own press: not again while one is out, and a refusal said under it
        let (asking, refused) = crate::accounts::clearing(s, &account);
        col_ = col_.child(rune_kit::Button::new("Clear cooldown").disabled(asking).on_press(move |s: &mut App| crate::accounts::clear(s, &account)).build(k));
        if let Some(why) = refused {
            col_ = col_.child(crate::accounts::needs_you(k, why).key("map-refused"));
        }
    }
    // a brain's mark opens its pass's page, the page its mark in his thread opens (outline.rs `mark_line`)
    let page = if kind == Kind::Brain { super::deep::brain_node(&id).map(str::to_string) } else { matches!(kind, Kind::Thread | Kind::Item | Kind::Post).then_some(id) };
    if let Some(id) = page {
        let at = project_at(s, g, i);
        col_ = col_.child(rune_kit::Button::new("Open its page").on_press(move |s: &mut App| open_page(s, at, &id)).build(k));
    }
    col_
}
