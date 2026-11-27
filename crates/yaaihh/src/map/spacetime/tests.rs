use super::*;
use crate::map::lod::Look;
use crate::map::{Edge, Link, Node, Rect};

const VIEW: [f32; 2] = [1200.0, 800.0];
const AT: i64 = 1_800_000_000;

/// A project, two threads in it, each with a post; thread A's post is TIME_A old, thread B's TIME_B, and B's post
/// names A's.
fn two(age_a: i64, age_b: i64) -> Graph {
    let mut g = Graph::default();
    let p = g.push(Node::new("project:research", Kind::Project, "research", None, Rect::new(0.0, 0.0, 1000.0, 600.0))).unwrap();
    let a = g.push(Node::new("N-1", Kind::Thread, "a", Some(p), Rect::new(100.0, 100.0, 300.0, 200.0))).unwrap();
    let pa = g.push(Node::new("N-1.p1", Kind::Post, "a's post", Some(a), Rect::new(120.0, 150.0, 100.0, 40.0))).unwrap();
    let b = g.push(Node::new("N-2", Kind::Thread, "b", Some(p), Rect::new(500.0, 100.0, 300.0, 200.0))).unwrap();
    let pb = g.push(Node::new("N-2.p1", Kind::Post, "b's post", Some(b), Rect::new(520.0, 150.0, 100.0, 40.0))).unwrap();
    g.link(Edge::new(pb, pa, Link::Names)).unwrap();
    g.date(|id| match id {
        "N-1.p1" => Some(AT - age_a),
        "N-2.p1" => Some(AT - age_b),
        _ => None,
    });
    g
}

fn shown(g: &Graph) -> Vec<Shown> {
    (0..g.nodes().len() as u32).map(|i| {
        let r = g.node(i).rect;
        Shown { i, r: [r.x as f32, r.y as f32, r.w as f32, r.h as f32], look: Look::Card, alpha: 1.0, open: 1.0, lit: false, many: 1, group: None }
    }).collect()
}

#[test]
fn flat_on_with_no_turn_the_now_plane_is_the_map_as_it_is() {
    let sp = Space { tilt: 90.0, ..Space::default() };
    for p in [[0.0, 0.0], [600.0, 400.0], [1100.0, 700.0]] {
        let (q, s) = sp.to_screen(p, 0, VIEW);
        assert!((q[0] - p[0]).abs() < 1e-3 && (q[1] - p[1]).abs() < 1e-3 && (s - 1.0).abs() < 1e-5, "{p:?} -> {q:?} {s}");
    }
}

#[test]
fn each_plane_behind_lies_smaller_and_higher_than_the_one_before() {
    let sp = Space::default();
    let mid = [VIEW[0] / 2.0, VIEW[1] / 2.0];
    let at: Vec<([f32; 2], f32)> = (0..PLANES).map(|k| sp.to_screen(mid, k, VIEW)).collect();
    for w in at.windows(2) {
        assert!(w[1].1 < w[0].1 && w[1].0[1] < w[0].0[1], "{at:?}");
    }
    // tilted, the Now plane's far edge is narrower than its near one: a plane seen in perspective
    let o = sp.outline(0, VIEW);
    assert!(o[1][0] - o[0][0] < o[2][0] - o[3][0], "{o:?}");
}

#[test]
fn the_wake_puts_each_age_on_its_plane_and_leaves_out_what_is_older() {
    assert_eq!(placed(60, false, Wake::Day, 0, AT), Some((0, 0.0)), "the last hour is now");
    assert_eq!(placed(30 * 3600, true, Wake::Now, 0, AT), Some((0, 0.0)), "what works or waits is now, whatever its age");
    assert_eq!(placed(2 * 3600, false, Wake::Now, 0, AT), None, "0h: the Now plane alone");
    let planes: Vec<usize> = [2, 8, 15, 24].iter().map(|h| placed(h * 3600, false, Wake::Day, 0, AT).unwrap().0).collect();
    assert_eq!(planes, [1, 1, 2, 3], "older, farther back");
    assert_eq!(placed(26 * 3600, false, Wake::Day, 0, AT), None);
    assert!(placed(26 * 3600, false, Wake::All, AT - 40 * 3600, AT).is_some(), "All: the whole record");
}

#[test]
fn flat_fades_what_is_older_and_moves_nothing() {
    let g = two(60, 20 * 3600);
    let mut s = shown(&g);
    let before = s.clone();
    let mut stand = HashMap::new();
    project(&g, &Space { mode: Mode::Flat, ..Space::default() }, AT, 0.3, VIEW, &mut s, &mut stand);
    assert_eq!(s.iter().map(|x| x.r).collect::<Vec<_>>(), before.iter().map(|x| x.r).collect::<Vec<_>>());
    let alpha = |id: &str| s.iter().find(|x| g.node(x.i).id == id).unwrap().alpha;
    assert_eq!(alpha("N-1.p1"), 1.0);
    assert!(alpha("N-2.p1") < 0.5 && alpha("N-2.p1") >= OLDEST, "{}", alpha("N-2.p1"));
    let mut s = shown(&g);
    project(&g, &Space { mode: Mode::Flat, wake: Wake::Now, ..Space::default() }, AT, 0.3, VIEW, &mut s, &mut stand);
    assert!(!s.iter().any(|x| g.node(x.i).id.starts_with("N-2")), "older than the wake: not drawn");
}

#[test]
fn glass_moves_a_thread_with_what_it_holds_and_draws_the_planes_behind_first() {
    let g = two(60, 10 * 3600);
    let mut s = shown(&g);
    let at = |s: &[Shown], id: &str| s.iter().position(|x| g.node(x.i).id == id).unwrap();
    let before = s.clone();
    let mut stand = HashMap::from([(4u32, 4usize)]);
    let used = project(&g, &Space::default(), AT, 0.3, VIEW, &mut s, &mut stand);
    assert_eq!(used, [0, 2]);
    let (t, p) = (s[at(&s, "N-2")].r, s[at(&s, "N-2.p1")].r);
    let (t0, p0) = (before[3].r, before[4].r);
    let k = t[2] / t0[2];
    assert!(k < 1.0, "smaller behind: {k}");
    assert!(((p[0] - t[0]) - (p0[0] - t0[0]) * k).abs() < 1e-3 && ((p[1] - t[1]) - (p0[1] - t0[1]) * k).abs() < 1e-3, "the post keeps its place in its thread");
    assert!(at(&s, "N-2") < at(&s, "N-1"), "the plane behind drawn first");
    assert_eq!(stand.get(&4), Some(&at(&s, "N-2.p1")), "what stands for a node follows its move");
}

#[test]
fn a_cards_causal_past_is_what_led_to_it_oldest_first() {
    let g = two(3 * 3600, 60);
    let ids = |v: Vec<u32>| v.into_iter().map(|i| g.node(i).id.clone()).collect::<Vec<_>>();
    let b = g.find_in(0, "N-2.p1").unwrap();
    assert_eq!(ids(crate::map::causal::past(&g, b, AT)), ["N-1.p1", "N-2.p1"], "the post it names, then it");
    let a = g.find_in(0, "N-1.p1").unwrap();
    assert_eq!(ids(crate::map::causal::past(&g, a, AT)), ["N-1.p1"], "what came later is not its past");
}

#[test]
fn the_space_is_kept_as_json_and_read_back() {
    let sp = Space { mode: Mode::Flat, wake: Wake::Hours2, tilt: 44.0, yaw: -10.0, pills: false };
    let back: Space = serde_json::from_str(&serde_json::to_string(&sp).unwrap()).unwrap();
    assert_eq!(back, sp);
    assert_eq!(serde_json::from_str::<Space>("{}").unwrap(), Space::default(), "an older file opens as Glass");
}

#[test]
fn zoomed_into_a_card_glass_gives_way_to_one_plane() {
    assert_eq!(tilted(0.3), 1.0);
    assert_eq!(tilted(FLATTENS[1]), 0.0);
    let g = two(60, 10 * 3600);
    let mut s = shown(&g);
    let before = s.clone();
    project(&g, &Space::default(), AT, FLATTENS[1], VIEW, &mut s, &mut HashMap::new());
    let r = |s: &[Shown], id: &str| s.iter().find(|x| g.node(x.i).id == id).unwrap().r;
    assert_eq!(r(&s, "N-2.p1"), r(&before, "N-2.p1"), "flat inside a card: a flight's fit lands where it aims");
}

/// The graph's first moment is kept as it is built and changed, and is always the first of its moments.
#[test]
fn the_graphs_origin_is_its_first_moment_kept_as_it_changes() {
    let mut g = two(10 * 3600, 3600);
    assert_eq!(g.origin(), g.moments().first().copied());
    assert_eq!(g.origin(), Some(AT - 10 * 3600));
    let older = g.push(Node { born: AT - 20 * 3600, ..Node::new("N-3", Kind::Thread, "c", Some(0), Rect::new(100.0, 400.0, 100.0, 100.0)) }).unwrap();
    assert_eq!(g.origin(), Some(AT - 20 * 3600), "an older node pushed");
    g.update(older, |n| n.born = AT);
    assert_eq!(g.origin(), g.moments().first().copied(), "the first gone later");
    assert_eq!(Graph::default().origin(), None);
}

/// A tilt or turn out of bounds, or not a number, as a damaged ui.json may hold, reads back within them.
#[test]
fn a_space_read_back_is_within_its_bounds() {
    let wild = Space { tilt: 400.0, yaw: f32::NAN, ..Space::default() }.clamped();
    assert_eq!((wild.tilt, wild.yaw), (TILT_BOUNDS[1], 0.0));
    let low = Space { tilt: f32::NEG_INFINITY, yaw: -90.0, ..Space::default() }.clamped();
    assert_eq!((low.tilt, low.yaw), (TILT, YAW_BOUNDS[0]));
    assert_eq!(Space::default().clamped(), Space::default());
}

/// At the tilt Glass opens at, the whole stack lies in the view: the Now plane's near edge and the farthest plane's
/// far edge.
#[test]
fn glass_opens_with_its_whole_stack_in_the_view() {
    let sp = Space::default();
    for k in 0..PLANES {
        for p in sp.outline(k, VIEW) {
            assert!(p[1] >= 0.0 && p[1] <= VIEW[1], "plane {k}: {p:?}");
        }
    }
}
