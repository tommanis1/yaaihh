//! The Map in the window, on the made-up record (made_up.rs; nothing of his is read: test_app's folder is its own):
//! p715's paths that the first Map draws, each as its presses and its end state (paper-research N-1479 p3: what is
//! selected, what the inspector says, the crumb trail); that a node keeps its place as the wheel zooms at it and
//! nothing out of view is built; what a frame builds at the record's size; and the stills at five scales.

use super::lod::{self, Look};
use super::{Graph, Kind, Node, Rect, canvas, made_up};
use crate::lens::{Depth, Filter, Lens};
use crate::{App, Sel};
use rune::runtime::Runtime;
use rune::{Tx, WheelEvent};

fn map_app() -> (std::sync::MutexGuard<'static, ()>, App) {
    let (held, mut s) = crate::view_cost::test_app();
    let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
    p.down = None;
    // a coordd before the graph's: the Map draws its own places, as it does until they go (lanes.rs `serves`)
    p.protocol = coord_types::GRAPH_PROTOCOL - 1;
    s.projects = vec![p];
    s.cur = 0;
    s.map.set_graph(made_up::record());
    s.sel = Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Cloud, at: None });
    (held, s)
}

pub(super) fn map_poll(s: &mut App, clock: f64, tx: &mut Tx) -> bool {
    canvas::poll(s, clock, tx)
}

pub(super) fn window(s: App) -> Runtime<App> {
    window_at(s, (1400.0, 900.0))
}

/// The window, SIZE points wide and high (the stills' 1400 by 900 is `window`'s).
fn window_at(s: App, size: (f32, f32)) -> Runtime<App> {
    let mut rt = Runtime::new(s, crate::view, size, rune::Env::new(false, 1.0));
    rt.poll = Some(map_poll);
    rt.on_key = Some(crate::on_key);
    rt.frame(0.0);
    rt.frame(0.016);
    rt
}

/// What the canvas draws now, and its edges lit as the selection lights them.
fn drawn_edges(rt: &Runtime<App>) -> (std::sync::Arc<Graph>, Vec<lod::Shown>, Vec<lod::Drawn>) {
    let v = &rt.state.map;
    let g = v.graph().unwrap().clone();
    let (shown, stand) = v.seen();
    let e = lod::edges(&g, &shown, &stand, v.selected());
    (g, shown, e)
}

/// Frames until a flight has landed.
pub(super) fn settle(rt: &mut Runtime<App>) {
    for _ in 0..120 {
        rt.frame(0.05);
        if !rt.state.map.flying() {
            rt.frame(0.016);
            return;
        }
    }
    panic!("the flight never landed");
}

/// Where node ID of research is drawn, its middle in window points, and how.
fn drawn(rt: &Runtime<App>, id: &str) -> Option<([f32; 2], Look)> {
    drawn_in(rt, "project:research", id)
}

/// Where node ID of project PROJECT (its node's id) is drawn, its middle in window points, and how.
fn drawn_in(rt: &Runtime<App>, project: &str, id: &str) -> Option<([f32; 2], Look)> {
    let v = &rt.state.map;
    let i = v.pick(&(project.into(), id.into()))?;
    let s = v.shown().into_iter().find(|x| x.i == i)?;
    Some(([v.area[0] + s.r[0] + s.r[2] / 2.0, v.area[1] + s.r[1] + s.r[3] / 2.0], s.look))
}

/// Where node ID of research is pressed: its dot when it is drawn as one (at a row's left), else its middle.
fn pressed_at(rt: &Runtime<App>, id: &str) -> Option<[f32; 2]> {
    let v = &rt.state.map;
    let i = v.pick(&("project:research".into(), id.into()))?;
    let s = v.shown().into_iter().find(|x| x.i == i)?;
    let d = if s.look == Look::Dot { lod::dot_at(&s.r) } else { [s.r[0] + s.r[2] / 2.0, s.r[1] + s.r[3] / 2.0] };
    Some([v.area[0] + d[0], v.area[1] + d[1]])
}

pub(super) fn press(rt: &mut Runtime<App>, at: [f32; 2]) {
    rt.pointer_move(at[0], at[1]);
    rt.frame(0.016);
    rt.pointer_down(at[0], at[1]);
    rt.pointer_up(at[0], at[1]);
    rt.frame(0.016);
}

/// The texts starting inside the element KEY, top to bottom, left to right; the floating sidebar's over it left out.
pub(super) fn texts_in(rt: &Runtime<App>, key: &str) -> Vec<String> {
    let Some((r, _)) = rt.presented(key) else { return Vec::new() };
    let mut t: Vec<([f32; 4], String)> = rt
        .elements()
        .into_iter()
        .filter_map(|e| {
            let y = e.rect[1] + e.rect[3] / 2.0;
            let inside = e.rect[0] >= r[0] - 0.5 && e.rect[0] <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3];
            // the sidebar's rows lie over the canvas's left edge, never in a card
            e.text.clone().filter(|_| inside && !e.path.contains("/sidebar/")).map(|t| (e.rect, t))
        })
        .collect();
    // top to bottom by lines (middles within 6 points are one line), each line left to right
    t.sort_by(|a, b| (a.0[1] + a.0[3] / 2.0).total_cmp(&(b.0[1] + b.0[3] / 2.0)));
    let mut lines: Vec<Vec<([f32; 4], String)>> = Vec::new();
    for x in t {
        let y = x.0[1] + x.0[3] / 2.0;
        match lines.last_mut() {
            Some(l) if (l[0].0[1] + l[0].0[3] / 2.0 - y).abs() < 6.0 => l.push(x),
            _ => lines.push(vec![x]),
        }
    }
    lines.into_iter().flat_map(|mut l| {
        l.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]));
        l.into_iter().map(|x| x.1)
    }).collect()
}

/// The middle of the element that says TEXT.
pub(super) fn says(rt: &Runtime<App>, text: &str) -> Option<[f32; 2]> {
    let e = rt.elements().into_iter().find(|e| e.text.as_deref() == Some(text))?;
    Some([e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0])
}

pub(super) fn selected(rt: &Runtime<App>) -> Option<String> {
    rt.state.map.sel.as_ref().map(|n| n.1.clone())
}

/// p715 path 1, "Did my ask land?" (his p678): the window opens on the record, his posts lit; a hover on p678 says its
/// two lines; a click selects it, the inspector shows its card and the crumb trail its place. The ring, its filling
/// and its trail into the build wait on the ledger (paper-research N-1479 p14).
#[test]
fn path_1_his_ask_is_lit_and_a_hover_and_a_click_say_where_it_is() {
    let (_held, s) = map_app();
    let mut rt = window(s);
    let (at, look) = drawn(&rt, "N-1104.p678").expect("p678 is drawn from the farthest zoom");
    assert_eq!(look, Look::Dot, "a lit point");
    // his posts lit across the record, each point standing for those that would meet it (none over a card's head)
    let lit: Vec<_> = rt.state.map.shown().into_iter().filter(|x| x.lit && rt.state.map.graph().unwrap().node(x.i).kind == Kind::Post).collect();
    let stand: u32 = lit.iter().map(|x| x.many).sum();
    assert!(stand >= 250 && lit.len() < stand as usize / 4, "{} points for {stand} posts", lit.len());
    // and no two of a kind touch, whatever grid cell they fall in (proto-window p48: two read as an 8)
    let all: Vec<_> = rt.state.map.shown().into_iter().filter(|x| x.lit).collect();
    let kind = |i: u32| rt.state.map.graph().unwrap().node(i).kind;
    for (k, a) in all.iter().enumerate() {
        for b in all[k + 1..].iter().filter(|b| kind(b.i) == kind(a.i)) {
            let (da, db) = (lod::dot_at(&a.r), lod::dot_at(&b.r));
            let e = (da[0] - db[0]).hypot(da[1] - db[1]);
            assert!(e >= lod::LIT_APART, "{} and {} are {e} apart", a.i, b.i);
        }
    }
    rt.pointer_move(at[0], at[1]);
    rt.frame(0.016);
    assert_eq!(texts_in(&rt, "map-preview"), ["d h m, please, on the Accounts tab", "post · d h m on the Accounts tab · in your window since 10:34 (build 59) · 20 more here"]);
    press(&mut rt, at);
    assert_eq!(selected(&rt).as_deref(), Some("N-1104.p678"));
    let ins = texts_in(&rt, "map-inspector");
    assert_eq!(ins[..4], ["post", "d h m, please, on the Accounts tab", "research › Your chat › N-1104 · his chat › d h m, please, on the Accounts tab", "d h m on the Accounts tab · in your window since 10:34 (build 59)"], "{ins:?}");
    assert_eq!(texts_in(&rt, "map-crumbs"), ["All", "›", "research", "›", "Your chat", "›", "N-1104 · his chat", "›", "d h m, please, on the Accounts tab"]);
    // marked: a ring around its point (p38 small 1)
    let ring = rt.presented("map-ring").expect("the selected point's ring").0;
    assert!((ring[0] + ring[2] / 2.0 - at[0]).abs() < 1.0 && (ring[1] + ring[3] / 2.0 - at[1]).abs() < 1.0 && ring[2] > lod::lit_side(1) + 2.0, "around it, off it: {ring:?}");
    // p678 names nothing: no edge lit, though Your chat's own go on to Window look and the gateway (p38 hold 1)
    let (g, shown, e) = drawn_edges(&rt);
    let chat = g.find_in(0, "topic:Your chat").unwrap();
    let of_chat = |x: &lod::Drawn| [x.from, x.to].iter().any(|k| g.path(shown[*k].i).contains(&chat));
    assert!(e.iter().filter(|x| of_chat(x)).count() >= 2, "Your chat's edges drawn: {e:?}");
    assert!(e.iter().all(|x| !x.lit), "none lit by p678: {:?}", e.iter().filter(|x| x.lit).map(|x| (&g.node(shown[x.from].i).id, &g.node(shown[x.to].i).id)).collect::<Vec<_>>());
}

/// p715 path 2, "Is everyone working?" (11:01): research's badge says 16 held; a double-click flies into research,
/// opening it whole, where the held agents' points point at one account; selecting it, the inspector offers Clear
/// cooldown. At the stills' 1400 by 900 and smaller, where two held agents fall on one point and its edges are both
/// theirs (proto-window p38 hold 2).
#[test]
fn path_2_the_held_agents_point_at_the_account_that_holds_them() {
    let mut merged = 0;
    for (size, whole) in [((1400.0, 900.0), true), ((1400.0, 860.0), true), ((1200.0, 800.0), true), ((1000.0, 700.0), false)] {
        merged += path_2_at(size, whole);
    }
    assert!(merged > 0, "at some size two held agents fell on one point");
}

/// Path 2 in a window SIZE points wide and high; WHOLE: research's fit opens it whole there. How many points stood
/// for more than one held agent.
fn path_2_at(size: (f32, f32), whole: bool) -> usize {
    let (_held, s) = map_app();
    let mut rt = window_at(s, size);
    let island = texts_in(&rt, "map-n-0");
    assert!(island.contains(&"16 held · limit".to_string()), "{size:?}: {island:?}");
    // a double-click on research, away from any dot: its corner band
    let r = rt.presented("map-n-0").unwrap().0;
    let away = [r[0] + r[2] * 0.6, r[1] + r[3] * 0.97];
    rt.pointer_move(away[0], away[1]);
    rt.pointer_down(away[0], away[1]);
    rt.pointer_up(away[0], away[1]);
    rt.pointer_down(away[0], away[1]);
    rt.pointer_up(away[0], away[1]);
    rt.frame(0.016);
    assert_eq!(selected(&rt).as_deref(), Some("project:research"));
    settle(&mut rt);
    let research = rt.state.map.shown().into_iter().find(|x| x.i == 0).unwrap();
    // fitted to the canvas it lands in, the inspector open beside it, not the one before the double-click opened it
    let (r, [w, h]) = (research.r, rt.state.map.size());
    assert!(r[0] >= 0.0 && r[1] >= 0.0 && r[0] + r[2] <= w + 0.5 && r[1] + r[3] <= h + 0.5, "{size:?}: research whole in the canvas {w}x{h}: {r:?}");
    if whole {
        assert_eq!(research.open, 1.0, "{size:?}: the fit opens it whole (p38 small 3), side {}", lod::side(&research.r));
    }
    let held = |rt: &Runtime<App>, when: &str| -> usize {
        let (g, shown, e) = drawn_edges(rt);
        let agent = |x: &lod::Shown| g.node(x.i).kind == Kind::Agent && g.node(x.i).state == super::State::Held;
        let points: Vec<&lod::Shown> = shown.iter().filter(|x| agent(x)).collect();
        let v = &rt.state.map;
        let stand = v.seen().1;
        // for the message: each agent neither drawn nor standing in another's point, where it is and what holds it
        let missing: Vec<String> = (0..g.nodes().len() as u32)
            .filter(|i| g.node(*i).kind == Kind::Agent && !shown.iter().any(|x| x.i == *i) && !stand.contains_key(i))
            .map(|i| {
                let t = g.node(i).parent.unwrap();
                let ts = shown.iter().find(|x| x.i == t);
                format!("{} at {:?} in {} {:?}", g.node(i).id, v.cam.on_screen(&g.node(i).rect), g.node(t).id, ts.map(|x| (x.look, x.r, x.open)))
            })
            .collect();
        assert_eq!(points.iter().map(|x| x.many).sum::<u32>(), 16, "{size:?} {when}: every agent held, a point each or one standing for its neighbours'; view {:?}, not drawn: {missing:?}", v.size());
        let gw = g.find_in(0, "account:gateway-opus").unwrap();
        let into: Vec<&lod::Drawn> = e.iter().filter(|x| shown[x.to].i == gw || shown[x.from].i == gw).collect();
        assert_eq!(into.iter().map(|x| x.count).sum::<u32>(), 16, "{size:?} {when}: each held agent's edge into the gateway");
        for x in into {
            let other = if shown[x.to].i == gw { &shown[x.from] } else { &shown[x.to] };
            assert!(agent(other), "{size:?} {when}: a gateway edge ends at {}, not at an agent's point", g.node(other.i).id);
            assert_eq!(x.count, other.many, "{size:?} {when}: as many edges at a point as the agents it stands for");
        }
        points.iter().filter(|x| x.many > 1).count()
    };
    let into = held(&rt, "in research");
    let gat = pressed_at(&rt, "account:gateway-opus").unwrap();
    press(&mut rt, gat);
    assert_eq!(selected(&rt).as_deref(), Some("account:gateway-opus"), "{size:?}");
    let ins = texts_in(&rt, "map-inspector");
    assert!(ins.contains(&"cooling until 13:31".to_string()) && ins.contains(&"Clear cooldown".to_string()), "{size:?}: {ins:?}");
    assert_eq!(texts_in(&rt, "map-crumbs"), ["All", "›", "research", "›", "Gateway · Opus"], "{size:?}");
    // as the still is taken: the inspector open on the gateway
    into + held(&rt, "the inspector open on the gateway")
}

/// A double-click's fit opens a topic whole, as it does a project (proto-window p38 small 3): what it holds is drawn
/// at its full ink, not at the fade's half.
#[test]
fn a_fit_opens_a_topic_whole() {
    for size in [(1400.0, 900.0), (1200.0, 800.0)] {
        for topic in ["topic:Your chat", "topic:Builds", "topic:Window look"] {
            let (_held, s) = map_app();
            let mut rt = window_at(s, size);
            assert!(canvas::fly_to_id(&mut rt.state, 0, topic), "{topic}");
            rt.invalidate();
            settle(&mut rt);
            let v = &rt.state.map;
            let i = v.pick(&("project:research".into(), topic.into())).unwrap();
            let t = v.shown().into_iter().find(|x| x.i == i).unwrap();
            assert_eq!(t.open, 1.0, "{size:?} {topic}: side {}", lod::side(&t.r));
        }
    }
}

/// One Map at a time (p38 small 5, consult-3 p39): with the Map focused and in a narrower pane beside it, one canvas
/// is drawn, the focused one, at its own size; the pane says where the Map is. Focusing another pane moves the canvas
/// to the first pane showing the Map.
#[test]
fn the_map_is_drawn_in_one_place() {
    let (_held, mut s) = map_app();
    let sel = s.sel.clone();
    s.panes = vec![(0, sel.clone()), (0, sel), (0, Sel::Tickets)];
    s.focus = 0;
    let mut rt = window(s);
    let canvases = |rt: &Runtime<App>| rt.elements().iter().filter(|e| e.key.as_deref() == Some("map-canvas")).count();
    let check = |rt: &Runtime<App>, when: &str| {
        assert_eq!(canvases(rt), 1, "{when}: one canvas");
        let r = rt.presented("map-canvas").unwrap().0;
        let v = &rt.state.map;
        // its own canvas's size, to the snap of a pixel
        assert!((v.area[2] - r[2]).abs() < 0.5 && (v.area[3] - r[3]).abs() < 0.5, "{when}: the Map culls and fits by its own canvas: {:?} {r:?}", v.area);
        assert!(rt.find("map-elsewhere").is_some(), "{when}: the other pane says where the Map is");
    };
    check(&rt, "focused");
    // the pane of Threads focused: the Map in the first pane that shows it
    rt.state.focus_pane(2);
    rt.invalidate();
    rt.frame(0.016);
    rt.frame(0.016);
    assert_eq!(canvas::drawn_in(&rt.state), Some(0));
    check(&rt, "another pane focused");
}

/// Two islands that both have an N-12 (consult-3, N-1479 p16): a double-click on each goes into its own, the crumbs
/// say whose, and its Open its page opens its own project, whichever was current, and asks that project's coordd for
/// the thread when its snapshot lacks it.
#[test]
fn each_island_s_n12_is_its_own() {
    let (_held, mut s) = map_app();
    let mut cav = crate::Proj::new(&crate::projects::ProjectConfig { name: "cav-paper".into(), id: "cav-paper".into(), boxed: false }, Vec::new());
    cav.down = None;
    cav.protocol = coord_types::GRAPH_PROTOCOL - 1;
    s.projects.push(cav);
    let names = ["research", "cav-paper"];
    let mut nodes = Vec::new();
    for (k, p) in names.iter().enumerate() {
        let x = k as f64 * 1_200.0;
        let r = nodes.len() as u32;
        nodes.push(Node::new(format!("project:{p}"), Kind::Project, *p, None, Rect::new(x, 0.0, 1_000.0, 1_000.0)));
        nodes.push(Node::new("topic:Paper", Kind::Topic, format!("{p}'s topic"), Some(r), Rect::new(x + 50.0, 50.0, 900.0, 900.0)));
        nodes.push(Node::new("N-12", Kind::Thread, format!("{p}'s N-12"), Some(r + 1), Rect::new(x + 300.0, 300.0, 400.0, 400.0)));
    }
    s.map.set_graph(Graph::new(nodes, vec![]).expect("an N-12 on each island is not Twice"));
    let mut rt = window(s);
    for (k, p) in names.iter().enumerate() {
        let island = format!("project:{p}");
        let g = rt.state.map.graph().unwrap().clone();
        rt.state.map.fly_to(&g.node(g.project(&island).unwrap()).rect, true);
        rt.invalidate();
        rt.frame(0.016);
        let (at, _) = drawn_in(&rt, &island, "N-12").expect("its N-12 drawn");
        rt.pointer_move(at[0], at[1]);
        for _ in 0..2 {
            rt.pointer_down(at[0], at[1]);
            rt.pointer_up(at[0], at[1]);
        }
        rt.frame(0.016);
        assert_eq!(rt.state.map.sel, Some((island.clone(), "N-12".to_string())), "its own N-12");
        settle(&mut rt);
        let (topic, thread) = (format!("{p}'s topic"), format!("{p}'s N-12"));
        assert_eq!(texts_in(&rt, "map-crumbs"), ["All", "›", p, "›", topic.as_str(), "›", thread.as_str()]);
        // the other project current: Open its page makes this one current
        rt.state.cur = 1 - k;
        let open = says(&rt, "Open its page").expect("the inspector offers its page");
        press(&mut rt, open);
        assert_eq!(rt.state.cur, k, "{p}'s page");
        // neither snapshot holds an N-12, so its page is fetched (lens.rs `zoom`, find.rs `number`), and from its own
        // project: this test gives neither a command, and the notice names the one asked
        assert_eq!(rt.state.notice.as_deref(), Some(format!("{p}: no reply").as_str()), "{p}'s N-12 asked of {p}");
    }
}

/// The wheel zooms at the pointer: the node under it stays under it at every scale, gaining detail in place; and what
/// is built is only what meets the view, never more than a few hundred nodes, however large the record.
#[test]
fn a_node_keeps_its_place_as_the_wheel_zooms_and_nothing_out_of_view_is_built() {
    let (_held, s) = map_app();
    let mut rt = window(s);
    let (q, _) = drawn(&rt, "N-1104.p678").unwrap();
    let area = rt.state.map.area;
    let mut looks = Vec::new();
    for _ in 0..34 {
        rt.wheel_event(WheelEvent::new(0.0, 40.0).at(q[0], q[1]));
        rt.frame(0.016);
        let (c, look) = drawn(&rt, "N-1104.p678").expect("still drawn");
        assert!((c[0] - q[0]).abs() < 0.5 && (c[1] - q[1]).abs() < 0.5, "it stays under the pointer: {c:?} {q:?}");
        looks.push(look);
        let built = rt.elements().iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("map-n-"))).count();
        let shown = rt.state.map.shown();
        assert_eq!(built, shown.len(), "a node built for each node shown, no more");
        assert!(built < 2_000, "only what is in view: {built}");
        for x in &shown {
            let r = x.r;
            assert!(r[0] < area[2] && r[1] < area[3] && r[0] + r[2] > 0.0 && r[1] + r[3] > 0.0, "drawn out of view: {r:?}");
        }
    }
    looks.dedup();
    assert_eq!(looks, [Look::Dot, Look::Label, Look::Card, Look::Open], "it gains detail in place");
    assert!(rt.state.map.cam.scale > 10.0, "the wheel went deep: {}", rt.state.map.cam.scale);
}

/// Esc flies out to what holds the place in view, and on out to the whole record.
#[test]
fn esc_flies_out_one_level_at_a_time() {
    let (_held, s) = map_app();
    let mut rt = window(s);
    let (q, _) = drawn(&rt, "N-1104.p678").unwrap();
    for _ in 0..30 {
        rt.wheel_event(WheelEvent::new(0.0, 40.0).at(q[0], q[1]));
    }
    rt.frame(0.016);
    let deep = rt.state.map.cam.scale;
    let esc = rune::KeyEvent { key: rune::Key::Escape, mods: Default::default() };
    rt.key(esc.clone());
    settle(&mut rt);
    let once = rt.state.map.cam.scale;
    assert!(once < deep, "out: {deep} to {once}");
    for _ in 0..6 {
        rt.key(esc.clone());
        settle(&mut rt);
    }
    let whole = rt.state.map.cam.scale;
    assert!(whole < once && (drawn(&rt, "project:research").unwrap().1 == Look::Open), "the whole record again: {whole}");
}

/// What a frame of the Map costs at the record's size (the made-up record's 4,432 nodes, research's about 3,300), at
/// five scales: each within one frame at 60 Hz. Ignored, as a timing is not for a loaded machine; run it alone:
/// `cargo test --release -p yaaihh map::paths::map_frame_cost -- --ignored --nocapture --test-threads 1` (the quick profile leaves this crate unoptimised).
#[test]
#[ignore]
fn map_frame_cost() {
    let (_held, s) = map_app();
    let g = s.map.graph().unwrap().clone();
    println!("record: {} nodes, {} edges", g.nodes().len(), g.edges().len());
    let mut rt = window(s);
    let (q, _) = drawn(&rt, "N-1104.p678").unwrap();
    let mut done = 0;
    for notches in [0usize, 6, 9, 15, 24] {
        for _ in done..notches {
            rt.wheel_event(WheelEvent::new(0.0, 40.0).at(q[0], q[1]));
        }
        done = notches;
        rt.frame(0.016);
        let t = std::time::Instant::now();
        for _ in 0..20 {
            rt.invalidate();
            rt.frame(0.016);
        }
        let ms = t.elapsed().as_secs_f64() * 1e3 / 20.0;
        println!("scale {:.3}: {} nodes drawn, {ms:.2} ms a frame", rt.state.map.cam.scale, rt.state.map.shown().len());
        assert!(ms < 1e3 / 60.0, "a frame at scale {} took {ms:.2} ms", rt.state.map.cam.scale);
    }
}

/// The scales of the stills, as camera scales at p678's middle: the whole record (0: fitted, as the Map opens), Your
/// chat among its neighbours, his chat open with its posts as points, its posts labelled, p678's card.
const SCALES: [f64; 5] = [0.0, 0.2, 0.5, 2.5, 9.0];

static SCALE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn at_scale() -> App {
    let (_held, mut s) = map_app();
    let g = s.map.graph().unwrap().clone();
    let p = g.node(g.find_in(0, "N-1104.p678").unwrap()).rect.mid();
    let scale = SCALES[SCALE.load(std::sync::atomic::Ordering::SeqCst)];
    if scale > 0.0 {
        s.map.aim(p, scale);
    }
    s
}

/// The stills: the same canvas, the camera moving in on p678 at five scales, in dark (his look).
#[test]
#[ignore]
fn map_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    for (k, _) in SCALES.iter().enumerate() {
        SCALE.store(k, std::sync::atomic::Ordering::SeqCst);
        let name = format!("map-{}.png", k + 1);
        let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1600x900".into(), "--at".into(), "1.0".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1600.0, 900.0), at_scale, crate::view) }).args(args).try_run().unwrap();
    }
}

/// The window as the Map opens, on the made-up record: the first step of paths 1 and 2.
fn opened() -> App {
    map_app().1
}

/// One still a step of paths 1 and 2 (paper-research N-1479 p3), each a run of the window playing the path's presses
/// up to that step as the tests above press them, in dark (his look). The points they go to are read from the same
/// window run headless first; the nodes are pressed by their keys.
#[test]
#[ignore]
fn map_path_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let (p678, gw, away) = {
        let (_held, s) = map_app();
        let rt = window(s);
        let g = rt.state.map.graph().unwrap().clone();
        let r = rt.presented("map-n-0").unwrap().0;
        (format!("map-n-{}", g.find_in(0, "N-1104.p678").unwrap()), format!("map-n-{}", g.find_in(0, "account:gateway-opus").unwrap()), [r[0] + r[2] * 0.6, r[1] + r[3] * 0.97])
    };
    let into = format!(r#"{{"t": 0.3, "type": "double-click", "x": {}, "y": {}}}"#, away[0], away[1]);
    let steps = [
        // both paths' first step: the record, his posts lit, research's badge
        ("path-1-2-open", String::new(), "0.5"),
        ("path-1-hover", format!(r#"[{{"t": 0.3, "type": "move", "key": "{p678}"}}]"#), "0.6"),
        ("path-1-click", format!(r#"[{{"t": 0.3, "type": "tap", "key": "{p678}"}}]"#), "0.8"),
        ("path-2-into", format!("[{into}]"), "2.0"),
        ("path-2-account", format!(r#"[{into}, {{"t": 1.8, "type": "tap", "key": "{gw}"}}]"#), "2.4"),
    ];
    for (name, script, at) in steps {
        let mut args = vec!["--snap".to_string(), out.join(format!("{name}.png")).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), at.into()];
        if !script.is_empty() {
            let f = out.join(format!("{name}.json"));
            std::fs::write(&f, script).unwrap();
            args.extend(["--script".into(), f.display().to_string()]);
        }
        let spec = rune::AppSpec { poll: Some(map_poll), on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), opened, crate::view) };
        rune::App::new(spec).args(args).try_run().unwrap();
    }
}

// ---- the Map laid from the window's projects (join.rs) ---------------------------------------------------------------

static KEPT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The window on the made-up snapshot (made_up.rs `research`, `cav`: Insight's rows, the snapshot's tickets and agents
/// at research's size), its islands laid as the window lays them (join.rs), the places kept in a file of its own.
fn laid_app() -> (std::sync::MutexGuard<'static, ()>, App) {
    let (held, mut s) = crate::view_cost::test_app();
    let now = chrono::Utc::now().timestamp();
    s.projects.clear();
    for (pi, (id, snap)) in [("research", made_up::research(now)), ("cav-paper", made_up::cav(now))].into_iter().enumerate() {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: id.into(), id: id.into(), boxed: false }, Vec::new());
        p.down = None;
        // a coordd before the graph's, as `map_app`'s
        p.protocol = coord_types::GRAPH_PROTOCOL - 1;
        p.live.tickets = snap.tickets;
        p.live.agents = snap.agents;
        s.insights.threads.insert(pi, (Ok(snap.rows), now));
        s.projects.push(p);
    }
    s.cur = 0;
    let f = std::env::temp_dir().join(format!("yaaihh-map-laid-{}-{}.json", std::process::id(), KEPT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
    let _ = std::fs::remove_file(&f);
    s.map.laying.at = Some(f);
    s.sel = Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Cloud, at: None });
    (held, s)
}

/// A point of node I's rect on the canvas, in view, where nothing but I is under it: where a press on it lands on it.
fn bare(rt: &Runtime<App>, i: u32) -> Option<[f32; 2]> {
    bare_within(rt, i, rt.state.map.size()[0])
}

/// Where a double-click into node I lands on it twice: a [`bare`] point left of the inspector its first click opens at
/// the canvas's right (canvas.rs `INSPECTOR`), which covers what was under the pointer before the second click comes
/// (w7's laid-2: the second click went to the inspector).
fn bare_for_two(rt: &Runtime<App>, i: u32) -> Option<[f32; 2]> {
    bare_within(rt, i, rt.state.map.size()[0] - canvas::INSPECTOR)
}

/// A [`bare`] point less than W points into the canvas.
fn bare_within(rt: &Runtime<App>, i: u32, w: f32) -> Option<[f32; 2]> {
    bare_in(rt, i, [0.0, w], rt.state.map.size()[1])
}

/// A [`bare`] point from X[0] to X[1] points into the canvas, and less than H down it.
fn bare_in(rt: &Runtime<App>, i: u32, x: [f32; 2], h: f32) -> Option<[f32; 2]> {
    let v = &rt.state.map;
    let shown = v.shown();
    let r = shown.iter().find(|s| s.i == i)?.r;
    (1..40)
        .flat_map(|y| (1..40).map(move |k| [r[0] + r[2] * k as f32 / 40.0, r[1] + r[3] * y as f32 / 40.0]))
        .filter(|q| q[0] >= x[0] && q[0] < x[1] && q[1] >= 0.0 && q[1] < h)
        .find(|q| lod::under(&shown, *q).map(|k| shown[k].i) == Some(i))
        .map(|q| [v.area[0] + q[0], v.area[1] + q[1]])
}

/// Where a double-click on cav-paper lands in the strip the inspector opens over: a [`bare`] point of its island in
/// the canvas's right [`canvas::INSPECTOR`] points, above the minimap.
fn in_the_strip(rt: &Runtime<App>) -> (u32, [f32; 2]) {
    let size = rt.state.map.size();
    let i = rt.state.map.graph().unwrap().project("project:cav-paper").unwrap();
    (i, bare_in(rt, i, [size[0] - canvas::INSPECTOR + 8.0, size[0] - 8.0], size[1] * 0.5).expect("cav-paper bare in the inspector's strip"))
}

/// A double-click on a node in the strip the inspector opens over flies into it (docket-edge6 N-1479 p176, proto-lead
/// p177). With the inspector closed, the first click selects cav-paper, whose island lies in the canvas's right
/// INSPECTOR points, and the inspector opens over it; the window lays out a frame between the two clicks, as his does,
/// and the second, on the inspector, is the canvas's (canvas.rs `pair_catcher`). Once the pair's time has passed, the
/// catcher is gone and a press there is the inspector's.
#[test]
fn a_double_click_where_the_inspector_opens_flies_in() {
    let (_held, s) = laid_app();
    let mut rt = window(s);
    settle(&mut rt);
    assert_eq!(selected(&rt), None, "the inspector closed");
    let (i, q) = in_the_strip(&rt);
    let click = |rt: &mut Runtime<App>| {
        rt.pointer_down(q[0], q[1]);
        rt.pointer_up(q[0], q[1]);
        rt.frame(0.016);
    };
    rt.pointer_move(q[0], q[1]);
    rt.frame(0.016);
    // one click: the inspector open over its point, the catcher at it, a frame asked for when the pair stops waiting
    let woke = rt.wake_deadline();
    click(&mut rt);
    assert_eq!(selected(&rt).as_deref(), Some("project:cav-paper"));
    let (ir, _) = rt.presented("map-inspector").expect("the inspector open");
    assert!(q[0] >= ir[0] && q[0] < ir[0] + ir[2] && q[1] >= ir[1] && q[1] < ir[1] + ir[3], "the inspector over the click: {q:?} in {ir:?}");
    let (pr, _) = rt.presented("map-pair").expect("the pair's catcher");
    assert!(q[0] >= pr[0] && q[0] <= pr[0] + pr[2] && q[1] >= pr[1] && q[1] <= pr[1] + pr[3], "the catcher at the click: {q:?} in {pr:?}");
    let due = rt.wake_deadline();
    assert!(woke.is_none_or(|w| w > rt.clock + 0.5) && due.is_some_and(|d| d <= rt.clock + 0.5), "a frame asked for when the pair stops waiting: before {woke:?}, now {due:?} at {}", rt.clock);
    // its time over: the catcher gone, so a press there is the inspector's
    rt.frame(0.3);
    rt.frame(0.3);
    assert!(rt.presented("map-pair").is_none(), "the catcher gone once the pair's time is over");
    // the inspector closed again; a double-click, the window laid out between its two clicks: flown into cav-paper
    canvas::select(&mut rt.state, None);
    rt.invalidate();
    rt.frame(0.016);
    assert!(rt.presented("map-inspector").is_none(), "the inspector closed");
    let scale = rt.state.map.cam.scale;
    click(&mut rt);
    click(&mut rt);
    assert_eq!(selected(&rt).as_deref(), Some("project:cav-paper"));
    assert!(rt.state.map.flying(), "flying into it");
    settle(&mut rt);
    let v = &rt.state.map;
    let (r, size) = (v.shown().into_iter().find(|x| x.i == i).unwrap().r, v.size());
    assert!(v.cam.scale > scale && (r[2] >= 0.9 * (size[0] - 56.0) || r[3] >= 0.9 * (size[1] - 56.0)), "cav-paper fitted: {r:?} in {size:?}");
}

/// An island's frame is fitted to what it holds, and the first view to the frames: the room the layout keeps below
/// its lowest node stays in its file but is not a blank third of his first view (paper-research N-1479 p65).
#[test]
fn the_island_is_framed_to_what_it_holds_and_the_layout_keeps_its_room() {
    let (_held, s) = laid_app();
    let f = s.map.laying.at.clone().unwrap();
    let mut rt = window(s);
    settle(&mut rt);
    let v = &rt.state.map;
    let g = v.graph().unwrap();
    let research = g.roots().find(|r| g.node(*r).label == "research").unwrap();
    let frame = g.node(research).rect;
    let inside: Vec<Rect> = (0..g.nodes().len() as u32).filter(|i| *i != research && g.root_of(*i) == research).map(|i| g.node(i).rect).collect();
    let lowest = inside.iter().map(|r| r.bottom()).fold(f64::MIN, f64::max);
    let left = inside.iter().map(|r| r.x).fold(f64::MAX, f64::min);
    assert!((frame.bottom() - lowest - (left - frame.x)).abs() < 1e-6, "as far under the lowest as the nodes start right of its left: {frame:?}, lowest {lowest}, left {left}");
    let kept = super::layout::read(&f).expect("the places written");
    let room = kept.islands.get("research").expect("research kept").rect;
    assert!(room[3] > frame.h * 1.15 && room[..3] == [frame.x, frame.y, frame.w], "the layout keeps its room: {room:?} against {frame:?}");
    // on the canvas as the Map opens: the frame ends just under the lowest node drawn in it, whole in the view
    let shown = v.shown();
    let fr = shown.iter().find(|x| x.i == research).unwrap().r;
    let low = shown.iter().filter(|x| x.i != research && g.root_of(x.i) == research).map(|x| x.r[1] + x.r[3]).fold(f32::MIN, f32::max);
    assert!(fr[1] + fr[3] - low < 0.15 * fr[3], "no blank third: the frame {fr:?} ends at {}, its lowest drawn at {low}", fr[1] + fr[3]);
    assert!(fr[1] + fr[3] <= v.size()[1], "the frame whole in the first view: {fr:?} in {:?}", v.size());
}

/// Each kind of point drawn as the Map opens says what it is under the pointer, without a click (paper-research N-1479
/// p65: the unlabelled points beside the grid said nothing): its label, then its kind and what its ink stands for, a
/// region how many items and threads it holds, a held agent its limit, a point standing for others how many.
#[test]
fn each_kind_of_point_says_what_it_is_under_the_pointer() {
    let (_held, s) = laid_app();
    let mut rt = window(s);
    settle(&mut rt);
    let g = rt.state.map.graph().unwrap().clone();
    let shown = rt.state.map.shown();
    let mut seen: Vec<Kind> = Vec::new();
    let mut said: Vec<String> = Vec::new();
    let mut missed: Vec<String> = Vec::new();
    // the points, and the topics in whatever look (too small for their label, the white points beside the grid)
    let points: Vec<&lod::Shown> = shown.iter().filter(|x| x.look == Look::Dot || x.lit || (g.node(x.i).kind == Kind::Topic && x.look != Look::Open)).collect();
    for x in points.iter().copied() {
        let n = g.node(x.i);
        // one of each kind, and a held agent and a point standing for others among them
        let held = n.kind == Kind::Agent && n.state == super::State::Held && !said.iter().any(|t| t.starts_with("agent · held ·"));
        let many = x.many > 1 && !said.iter().any(|t| t.contains("more here"));
        if seen.contains(&n.kind) && !held && !many {
            continue;
        }
        // at its own dot when that is where it is picked, else a bare place of it
        let local = lod::dot_at(&x.r);
        let own = shown.iter().position(|y| y.i == x.i);
        let area = rt.state.map.area;
        let q = if lod::under(&shown, local) == own { Some([area[0] + local[0], area[1] + local[1]]) } else { bare(&rt, x.i) };
        let Some(q) = q else {
            missed.push(format!("{} at {:?}: no place picks it", n.id, x.r));
            continue;
        };
        rt.pointer_move(q[0], q[1]);
        rt.frame(0.016);
        // the card's own texts, by the element tree (texts_in reads by place: the canvas's labels under the card too)
        let els = rt.elements();
        let card = els.iter().find(|e| e.key.as_deref() == Some("map-preview")).map(|e| e.path.clone()).expect("a preview");
        let mut own: Vec<([f32; 4], String)> = els
            .iter()
            .filter(|e| e.path.len() > card.len() && e.path.starts_with(&card) && !e.path[card.len()..].starts_with(|c: char| c.is_ascii_alphanumeric()))
            .filter_map(|e| Some((e.rect, e.text.clone()?)))
            .collect();
        own.sort_by(|a, b| a.0[1].total_cmp(&b.0[1]));
        let says: Vec<String> = own.into_iter().map(|(_, t)| t).collect();
        let Some(hovered) = rt.state.map.hover.as_ref().and_then(|h| rt.state.map.pick(h)) else { panic!("nothing hovered over {}", n.id) };
        let h = g.node(hovered);
        if hovered != x.i {
            missed.push(format!("{} at {:?}: {} hovered", n.id, x.r, h.id));
        }
        let at = shown.iter().find(|y| y.i == hovered).unwrap();
        assert_eq!(says.len(), 2, "two lines over {}: {says:?}", h.id);
        assert_eq!(says[0], h.label, "its label first");
        assert!(says[1].starts_with(canvas::kind_word(h.kind)), "{}'s kind first: {says:?}", h.id);
        if matches!(h.kind, Kind::Topic | Kind::Project) {
            assert!(says[1].contains(" thread") || says[1].contains(" item"), "a region says what it holds: {says:?}");
        }
        if h.kind == Kind::Agent && h.state == super::State::Held {
            assert!(says[1].starts_with("agent · held · claude-opus-4-5 until "), "a held agent its limit: {says:?}");
        }
        if at.lit && at.many > 1 {
            assert!(says[1].ends_with(&format!("{} more here", at.many - 1)), "how many it stands for: {says:?}");
        }
        seen.push(h.kind);
        said.push(says[1].clone());
    }
    for k in [Kind::Topic, Kind::Agent, Kind::Post, Kind::Account] {
        assert!(seen.contains(&k), "a {k:?} point hovered: {said:?}; missed {missed:?}");
    }
    for x in &points {
        assert!(seen.contains(&g.node(x.i).kind), "each kind drawn as a point hovered: {:?} not; missed {missed:?}", g.node(x.i).kind);
    }
    assert!(said.iter().any(|t| t.starts_with("agent · held ·")) && said.iter().any(|t| t.contains("more here")), "{said:?}");
}

/// An agent's event that changes nothing the layout reads lays nothing: coordd sends one at each tool's start and end
/// and at `set_tools`, several a second with twenty agents at work; nor does it key the tickets. One that changes its
/// state lays again, and one that changes only its wait for an account (proto-window N-1479 p68 hold 1, p116, p134).
#[test]
fn an_agents_tool_events_lay_nothing_and_its_state_lays_again() {
    let (_held, s) = laid_app();
    let mut rt = window(s);
    let lays = |rt: &Runtime<App>| rt.state.map.laying.lays;
    assert_eq!(lays(&rt), 1, "laid as the Map opens");
    let keyed = rt.state.map.laying.lines_keyed;
    let first = rt.state.projects[0].live.agents.iter().find(|a| a.state == "working").expect("an agent at work").clone();
    let send = |rt: &mut Runtime<App>, seq: u64, a: coord_types::AgentView| {
        rt.state.take(0, crate::link::Incoming::Event(coord_types::Event { seq, body: coord_types::Body::Agent(a) }));
        rt.frame(0.016);
    };
    for k in 0..40u64 {
        let mut a = first.clone();
        a.last_tool = Some(format!("Bash {k}"));
        a.tools = if k % 2 == 0 { Vec::new() } else { first.tools.clone() };
        send(&mut rt, 10 + k, a);
    }
    assert_eq!(lays(&rt), 1, "forty tool events: not laid again");
    assert_eq!(rt.state.map.laying.lines_keyed, keyed, "forty tool events: the lines and tickets keyed no time");
    let mut a = first.clone();
    a.state = "idle".into();
    send(&mut rt, 100, a.clone());
    assert_eq!(lays(&rt), 2, "its state changed: laid again");
    a.account_waiting = !a.account_waiting;
    send(&mut rt, 101, a);
    assert_eq!(lays(&rt), 3, "its wait for an account changed: laid again");
}

/// A ticket's event that changes nothing the Map reads of it lays nothing; one that changes a post drawn does, though
/// the layout reads none of it: its words coming (a part), or the post taken back (consult-3 N-1479 p133, proto-window
/// p116, p134).
#[test]
fn a_tickets_event_lays_again_only_for_what_is_read_of_it() {
    let (_held, s) = laid_app();
    let mut rt = window(s);
    let lays = |rt: &Runtime<App>| rt.state.map.laying.lays;
    assert_eq!(lays(&rt), 1, "laid as the Map opens");
    let event = |rt: &mut Runtime<App>, f: &dyn Fn(&mut coord_types::TicketView)| {
        let p = &mut rt.state.projects[0];
        f(p.live.tickets.get_mut(&1104).unwrap());
        p.live.tickets_rev += 1;
        for _ in 0..3 {
            rt.invalidate();
            rt.frame(0.016);
        }
    };
    let label = |rt: &Runtime<App>, id: &str| -> Option<String> {
        let g = rt.state.map.graph().unwrap();
        g.find_in(0, id).map(|i| g.node(i).label.clone())
    };
    // what it has spent: read by nothing here (layout.rs `lines_of`, `key`; join.rs `put`)
    event(&mut rt, &|t| t.spent += 1);
    assert_eq!(lays(&rt), 1, "a ticket's event that changes nothing read: not laid again");
    // post 3's words, as a part that came after it
    event(&mut rt, &|t| t.posts[2].text = docket_machine::Text::New("the words, come after the post".into()));
    assert_eq!(lays(&rt), 2, "a post's words came: laid again");
    assert_eq!(label(&rt, "N-1104.p3").as_deref(), Some("the words, come after the post"));
    // post 5 taken back
    let back = rt.state.projects[0].live.tickets[&1104].posts[4].clone();
    assert!(back.visible() && label(&rt, "N-1104.p5").is_some());
    event(&mut rt, &|t| t.posts[4].taken_back = true);
    assert_eq!(lays(&rt), 3, "a post taken back: laid again");
    assert_eq!(label(&rt, "N-1104.p5"), None, "and not drawn");
}

/// The pointer's hover card hides what lies under it: as his double-click into research leaves it (laid-2-into), over
/// the topic cards, its fill is opaque (consult-3 N-1479 p186: research's card read through, the white edges and
/// parity's "1 waits on you" across its words; proto-lead p193).
#[test]
fn the_hover_card_hides_what_lies_under_it() {
    let (_held, s) = laid_app();
    let mut rt = window(s);
    let at = bare_for_two(&rt, 0).unwrap();
    rt.pointer_move(at[0], at[1]);
    for _ in 0..2 {
        rt.pointer_down(at[0], at[1]);
        rt.pointer_up(at[0], at[1]);
    }
    rt.frame(0.016);
    settle(&mut rt);
    rt.pointer_move(at[0] + 1.0, at[1]);
    rt.frame(0.016);
    let els = rt.elements();
    let card = els.iter().find(|e| !e.exiting && e.key.as_deref() == Some("map-preview")).expect("the hover card");
    let r = card.rect;
    let meets = |q: [f32; 4]| q[0] < r[0] + r[2] && r[0] < q[0] + q[2] && q[1] < r[1] + r[3] && r[1] < q[1] + q[3];
    let under: Vec<&str> = els.iter().filter_map(|e| e.key.as_deref().filter(|k| k.starts_with("map-n-") && meets(e.rect))).collect();
    assert!(!under.is_empty(), "nodes under it: {r:?}");
    assert!(card.fill.a >= 0.999, "opaque over {under:?}: {:?}", card.fill);
}

/// A move over the Map with nothing under the pointer before or after it builds no view (docket-edge6 N-1446 p2356,
/// 3: on his Map every move built it, 24 ms a move, 45 at most): it is `live_only`, and rune's debug check, a quarter
/// second after the last commit, finds the screen as the view builds it. A move onto a node, along it (its card
/// follows the pointer) and off it is built.
#[test]
fn a_move_over_nothing_builds_no_view() {
    let (_held, s) = map_app();
    let mut rt = window(s);
    let ([w, h], shown, area) = (rt.state.map.size(), rt.state.map.shown(), rt.state.map.area);
    // off the canvas's floating controls, by 4 points
    let controls: Vec<[f32; 4]> = ["map-layers", "map-mini", "map-crumbs", "map-inspector"].iter().filter_map(|k| rt.presented(k).map(|x| x.0)).collect();
    let off = |q: &[f32; 2]| controls.iter().all(|r| q[0] < r[0] - 4.0 || q[0] > r[0] + r[2] + 4.0 || q[1] < r[1] - 4.0 || q[1] > r[1] + r[3] + 4.0);
    let grid = (1..40).flat_map(|y| (1..40).map(move |x| [w * x as f32 / 40.0, h * y as f32 / 40.0]));
    let bare: Vec<[f32; 2]> = grid.filter(|q| lod::under(&shown, *q).is_none()).map(|q| [area[0] + q[0], area[1] + q[1]]).filter(off).take(12).collect();
    assert!(bare.len() >= 6, "bare canvas to move over: {bare:?}");
    let skipped = rt.live_only_events();
    for q in &bare {
        rt.pointer_move(q[0], q[1]);
        rt.frame(0.016);
    }
    let moves = rt.live_only_events() - skipped;
    assert!(moves + 2 >= bare.len() as u64, "{moves} of {} moves over nothing built no view", bare.len());
    rt.frame(0.3);
    rt.pointer_move(bare[0][0], bare[0][1]);
    rt.frame(0.016);
    assert!(rt.live_only_warnings().is_empty(), "the screen as the view builds it: {:?}", rt.live_only_warnings());
    let (at, _) = drawn(&rt, "N-1104.p678").expect("p678 is drawn");
    let skipped = rt.live_only_events();
    rt.pointer_move(at[0], at[1]);
    rt.frame(0.016);
    assert!(!texts_in(&rt, "map-preview").is_empty(), "its card");
    rt.pointer_move(at[0] + 1.0, at[1]);
    rt.frame(0.016);
    rt.pointer_move(bare[0][0], bare[0][1]);
    rt.frame(0.016);
    assert_eq!((rt.live_only_events(), rt.state.map.hover.is_none()), (skipped, true), "onto it, along it and off it: each built");
}

/// His post goes out of the last day: the Map is laid once at that time, the post lit no more, and not again in the
/// frames after (proto-lead N-1479 p124; proto-window p122). No limit ends meanwhile: the last day's end is the only
/// time a state laid changes by itself but a midnight, which lays first where one comes before it (consult-3 p184).
#[test]
fn his_post_leaving_the_last_day_lays_once_and_the_frames_after_do_not() {
    let (_held, s) = laid_app();
    let mut rt = window(s);
    for p in &mut rt.state.projects {
        for a in &mut p.live.agents {
            a.limit_until = None;
        }
        p.live.agents_rev += 1;
    }
    rt.frame(0.016);
    let lays = |rt: &Runtime<App>| rt.state.map.laying.lays;
    let now = chrono::Utc::now().timestamp();
    let end = rt.state.projects.iter().filter_map(|p| super::layout::last_day_ends(&p.live.tickets, now)).min().expect("a post of his of the last day, not open");
    // the posts it ends the day of, lit until then
    let ending: Vec<String> = rt.state.projects[0]
        .live
        .tickets
        .values()
        .flat_map(|t| t.posts.iter().filter(move |p| p.by == "owner" && !t.unanswered_owner.contains(&p.n)).map(move |p| (t, p)))
        .filter(|(_, p)| chrono::DateTime::parse_from_rfc3339(&p.at).is_ok_and(|at| at.timestamp() + 86_400 + 1 == end))
        .map(|(t, p)| format!("{}.p{}", t.id, p.n))
        .collect();
    let lit = |rt: &Runtime<App>, id: &str| {
        let g = rt.state.map.graph().unwrap();
        g.find_in(0, id).map(|i| g.node(i).his)
    };
    let drawn: Vec<&String> = ending.iter().filter(|id| lit(&rt, id).is_some()).collect();
    assert!(!drawn.is_empty() && drawn.iter().all(|id| lit(&rt, id) == Some(true)), "lit until the day ends: {ending:?}");
    let midnight = crate::when::next_day(now).unwrap();
    if midnight < end {
        assert!(super::join::keep(&mut rt.state, midnight), "a midnight before it: laid");
    }
    let laid = lays(&rt);
    assert!(!super::join::keep(&mut rt.state, end - 1), "a second before: not laid");
    assert!(super::join::keep(&mut rt.state, end), "the day ended: laid");
    assert_eq!(lays(&rt), laid + 1);
    assert!(drawn.iter().all(|id| lit(&rt, id) == Some(false)), "lit no more: {drawn:?}");
    // up to the next post's end of day
    let next = rt.state.projects.iter().filter_map(|p| super::layout::last_day_ends(&p.live.tickets, end)).min().unwrap_or(end + 3600);
    let next = next.min(crate::when::next_day(end).unwrap());
    for t in [end + 1, (end + next) / 2, next - 1].into_iter().filter(|t| *t > end && *t < next) {
        assert!(!super::join::keep(&mut rt.state, t), "the frames after, at {t}: not laid");
    }
    assert_eq!(lays(&rt), laid + 1, "laid once");
}

/// A post's note says its time as of the lay (join.rs `put`: "p699 · paper-research · 23:30"), so the Map is laid again
/// when the day ends, the note then giving the day before's weekday, and not in the seconds either side of midnight
/// (consult-3 N-1479 p184: after midnight a quiet Map said yesterday's 17:55 as if it were today's).
#[test]
fn a_post_of_the_day_before_says_its_weekday_once_laid_at_midnight() {
    let (_held, s) = laid_app();
    let mut rt = window(s);
    // paper-research's answer 699 at 23:30 the day after tomorrow: no limit ends after it, no post of his leaves the
    // last day
    let day = crate::when::local(chrono::Utc::now().timestamp()).date_naive() + chrono::Days::new(2);
    let late = day.and_hms_opt(23, 30, 0).unwrap().and_local_timezone(chrono::Local).earliest().unwrap();
    let midnight = crate::when::next_day(late.timestamp()).unwrap();
    for p in &mut rt.state.projects {
        for a in &mut p.live.agents {
            a.limit_until = None;
        }
        p.live.agents_rev += 1;
    }
    let chat = &mut rt.state.projects[0].live;
    chat.tickets.get_mut(&1104).unwrap().posts[698].at = late.to_rfc3339();
    chat.tickets_rev += 1;
    let note = |rt: &Runtime<App>| {
        let g = rt.state.map.graph().unwrap();
        g.node(g.find_in(0, "N-1104.p699").expect("answer 699 laid")).note.clone()
    };
    assert!(super::join::keep(&mut rt.state, midnight - 60), "laid with its time");
    assert_eq!(note(&rt), "p699 · paper-research · 23:30", "that day");
    let laid = rt.state.map.laying.lays;
    assert!(!super::join::keep(&mut rt.state, midnight - 1), "a second before midnight: not laid");
    assert!(super::join::keep(&mut rt.state, midnight), "midnight: laid");
    assert_eq!(note(&rt), format!("p699 · paper-research · {} 23:30", late.format("%a")), "the day before: its weekday");
    assert!(!super::join::keep(&mut rt.state, midnight + 1), "a second after: not laid");
    assert_eq!(rt.state.map.laying.lays, laid + 1, "laid once");
}

/// Accounts answers whose meters are those the Map was laid from lay nothing (the poll asks every 30 s); one whose
/// meter moved lays again (proto-window N-1479 p68, hold 1).
#[test]
fn accounts_answers_with_the_same_meters_lay_nothing() {
    let (_held, s) = laid_app();
    let mut rt = window(s);
    let lays = |rt: &Runtime<App>| rt.state.map.laying.lays;
    let resets = chrono::Utc::now().timestamp() + 3600;
    let answer = |used: f64| {
        let mut n = coord_types::NativeAccounts { gateway: true, ..Default::default() };
        n.accounts.push(coord_types::NativeAccount { id: "work".into(), label: "Work".into(), tier: 1, five_hour: Some(coord_types::LimitWindow { used_percent: used, resets_at: resets }), ..Default::default() });
        Ok(serde_json::to_string(&n).unwrap())
    };
    crate::accounts::answered(&mut rt.state, 0, answer(40.0));
    rt.frame(0.016);
    let first = lays(&rt);
    assert_eq!(first, 2, "the first answer's meters: laid again");
    for _ in 0..2 {
        crate::accounts::answered(&mut rt.state, 0, answer(40.0));
        rt.frame(0.016);
    }
    assert_eq!(lays(&rt), first, "two answers with the same meters: not laid again");
    crate::accounts::answered(&mut rt.state, 0, answer(55.0));
    rt.frame(0.016);
    assert_eq!(lays(&rt), first + 1, "a meter moved: laid again");
}

/// A post drawn open keeps its steps over a relayout: they are placed again in the poll that lays, so the frame drawn
/// next has them, with no frame between (consult-3 N-1479 p69). On a laid graph (`set_laid`), as the window's is.
#[test]
fn an_open_posts_steps_are_drawn_in_the_frame_after_a_relayout() {
    // the runs: wire_app's under N-7's first post (picture/docket.rs `ran_app`), given to his chat's first post here
    let w = crate::picture::docket::tests::wire_app();
    let (_held, mut s) = laid_app();
    let wid = w.projects[0].id.clone();
    let ran = w.projects[0].live.tickets[&7].ran.clone();
    let fetched = w.tickets.find.fetched[&wid].clone();
    s.tickets.find.fetched.insert(s.projects[0].id.clone(), fetched);
    s.projects[0].live.tickets.values_mut().find(|t| t.id == "N-1104").expect("his chat").ran = ran;
    let mut rt = window(s);
    settle(&mut rt);
    assert!(!rt.state.map.fixed(), "laid from the window's projects");
    let post = {
        let g = rt.state.map.graph().unwrap();
        let root = g.project("project:research").expect("its island");
        let p = g.find_in(root, "N-1104.p1").expect("his chat's first post laid");
        g.node(p).rect
    };
    // near enough that the post is drawn open (lod.rs `band`: 560 points a side, a third past it), its first step's
    // slot (deep.rs: a 6 by 2 grid under the text of its card, at the row's start) in the middle, the camera moved as
    // his would be
    let view = rt.state.map.size();
    let scale = super::camera::Camera::fitting(&post, view, 0.0).scale.max(1.33 * 560.0 / (post.w * post.h).sqrt()).min(super::camera::NEAREST);
    let card = super::deep::card(post);
    let mid = [card.x + card.w / 12.0, card.y + card.h * 0.7];
    moved_to(&mut rt, super::camera::Camera { at: [mid[0] - view[0] as f64 / 2.0 / scale, mid[1] - view[1] as f64 / 2.0 / scale], scale });
    for _ in 0..3 {
        rt.invalidate();
        rt.frame(0.016);
    }
    let steps = |rt: &Runtime<App>| -> Vec<u32> {
        let g = rt.state.map.graph().unwrap();
        (0..g.nodes().len() as u32).filter(|i| g.node(*i).kind == Kind::Step && rt.presented(&format!("map-n-{i}")).is_some()).collect()
    };
    assert!(!steps(&rt).is_empty(), "the post drawn open, its steps placed and drawn");
    let lays = rt.state.map.laying.lays;
    // an event the layout reads: laid again, and the next frame drawn, nothing asked between
    relay(&mut rt);
    rt.frame(0.016);
    assert_eq!(rt.state.map.laying.lays, lays + 1, "laid again");
    assert!(!steps(&rt).is_empty(), "the frame after the relayout has the post's steps");
}

/// An agent's event the layout reads (its limit's why), changing no place: the islands laid again as they were.
fn relay(rt: &mut Runtime<App>) {
    let p = &mut rt.state.projects[0];
    let a = &mut p.live.agents[0];
    a.limit_why = Some(format!("{} again", a.limit_why.clone().unwrap_or_default()));
    p.live.agents_rev += 1;
}

/// The camera put at CAM as his zoom puts it (canvas.rs `zoom`, which marks it moved): a relayout leaves it there.
fn moved_to(rt: &mut Runtime<App>, cam: super::camera::Camera) {
    rt.state.map.cam = cam;
    canvas::zoom(&mut rt.state, 1.0, [0.0, 0.0]);
}

/// The labels drawn hold still over a relayout as they do over a pan (lod.rs `labels`, 1238b9ba): the set the last
/// frame drew is the set the frame after the relayout draws (consult-3 N-1479 p69).
#[test]
fn the_labels_drawn_are_the_same_over_a_relayout() {
    let (_held, s) = laid_app();
    let mut rt = window(s);
    settle(&mut rt);
    // from near the smallest topic out to the whole record, frame by frame: the labels drawn first are held as more
    // come, until a frame whose labels a pick with none held would not draw (the case a relayout must keep)
    let (topic, whole) = {
        let g = rt.state.map.graph().unwrap();
        let t = (0..g.nodes().len() as u32).filter(|i| g.node(*i).kind == Kind::Topic).min_by(|a, b| (g.node(*a).rect.w * g.node(*a).rect.h).total_cmp(&(g.node(*b).rect.w * g.node(*b).rect.h))).unwrap();
        (g.node(t).rect, rt.state.map.cam)
    };
    let size = rt.state.map.size();
    let near = super::camera::Camera::fitting(&topic, size, 0.0);
    let k = rune_kit::Kit::neutral(true);
    let fresh = |rt: &Runtime<App>| -> std::collections::BTreeSet<canvas::Name> {
        let v = &rt.state.map;
        let (g, shown) = (v.graph().unwrap(), v.shown());
        lod::labels(g, &shown, &Default::default(), |t| rune::text::measure(t, &[], k.ty.small, None).0, 16.0).into_iter().filter_map(|(x, _)| v.name(shown[x].i)).collect()
    };
    let mut held = false;
    for step in 0..=60 {
        let f = step as f64 / 60.0;
        let scale = (near.scale.ln() * (1.0 - f) + whole.scale.ln() * f).exp();
        let at = [near.at[0] * (1.0 - f) + whole.at[0] * f, near.at[1] * (1.0 - f) + whole.at[1] * f];
        moved_to(&mut rt, super::camera::Camera { at, scale });
        rt.invalidate();
        rt.frame(0.016);
        if rt.state.map.labelled() != fresh(&rt) {
            held = true;
            break;
        }
    }
    assert!(held, "the walk reaches a frame whose labels held are not a fresh pick's");
    let before = rt.state.map.labelled();
    let lays = rt.state.map.laying.lays;
    relay(&mut rt);
    rt.frame(0.016);
    assert_eq!(rt.state.map.laying.lays, lays + 1, "laid again");
    assert_eq!(rt.state.map.labelled(), before, "the same labels drawn");
}

/// A topic card's badge is drawn whole inside it at level 1, its words going to a second line where the card is too
/// narrow for one (proto-window N-1479 p68, small 2: "1 hel").
#[test]
fn a_cards_badge_is_drawn_whole_inside_it() {
    let (_held, s) = laid_app();
    // narrower than the stills' window, so the card is narrower than its badge on one line whatever the fonts
    let mut rt = window_at(s, (1180.0, 760.0));
    settle(&mut rt);
    let g = rt.state.map.graph().unwrap().clone();
    let card = rt.state.map.shown().into_iter().find(|x| x.look == Look::Card && g.node(x.i).kind == Kind::Topic && rt.state.map.counts(x.i).held > 0).expect("a topic card with a held agent");
    let (r, _) = rt.presented(&format!("map-n-{}", card.i)).unwrap();
    let held = rt.elements().into_iter().find(|e| e.text.as_deref().is_some_and(|t| t.ends_with("held · limit")) && e.rect[1] >= r[1] && e.rect[1] < r[1] + r[3] && e.rect[0] >= r[0] && e.rect[0] < r[0] + r[2]).expect("its badge's held words");
    let k = rune_kit::Kit::neutral(true);
    let whole = rune::text::measure(held.text.as_deref().unwrap(), &[], k.ty.small_medium, None).0;
    assert!(held.rect[2] + 0.5 >= whole, "{:?} drawn {} wide of its {whole}", held.text, held.rect[2]);
    assert!(held.rect[0] + held.rect[2] <= r[0] + r[2] + 0.5, "{:?} ends inside the card {r:?}", held.rect);
}

/// The selected node's edges are drawn over the cards: a held agent's edge into the Gateway reaches its point on a
/// topic card, not the card's edge (proto-window N-1479 p68, small 1).
#[test]
fn the_selected_nodes_edges_are_drawn_over_the_cards() {
    let (_held, s) = laid_app();
    let mut rt = window(s);
    settle(&mut rt);
    let gw = pressed_at(&rt, "account:gateway").unwrap();
    press(&mut rt, gw);
    let els = rt.elements();
    let at = |key: &str| els.iter().position(|e| e.key.as_deref() == Some(key));
    let lit = at("map-lit-edges").expect("the selected node's edges, drawn apart");
    let cards: Vec<usize> = rt.state.map.shown().iter().filter(|x| x.look == Look::Card).filter_map(|x| at(&format!("map-n-{}", x.i))).collect();
    assert!(!cards.is_empty() && cards.iter().all(|c| *c < lit), "over every card: {cards:?} under {lit}");
}

/// With research selected, as his first double-click into it leaves it, its edges are lit. An agent's edge to a thread
/// rolls up to its topic, drawn as a card, and ends at the card's middle: those stay under the cards, lit, and only
/// edges between two points are drawn over them. The Gateway's 16 edges to the held agents' points still are
/// (proto-window N-1479 p179).
#[test]
fn only_a_lit_edge_between_two_points_is_drawn_over_the_cards() {
    let (_held, s) = laid_app();
    let mut rt = window(s);
    settle(&mut rt);
    let away = bare_for_two(&rt, 0).unwrap();
    rt.pointer_move(away[0], away[1]);
    for _ in 0..2 {
        rt.pointer_down(away[0], away[1]);
        rt.pointer_up(away[0], away[1]);
    }
    rt.frame(0.016);
    settle(&mut rt);
    assert_eq!(selected(&rt).as_deref(), Some("project:research"));
    let (g, shown, e) = drawn_edges(&rt);
    let on_a_card = |e: &lod::Drawn| [e.from, e.to].iter().any(|k| matches!(shown[*k].look, Look::Card | Look::Open));
    let carded: Vec<String> = e.iter().filter(|e| e.lit && on_a_card(e)).map(|e| format!("{} - {}", g.node(shown[e.from].i).id, g.node(shown[e.to].i).id)).collect();
    assert!(carded.len() >= 7, "research's lit edges with an end on a card: {carded:?}");
    assert!(e.iter().filter(|e| canvas::over_the_cards(e, &shown)).all(|e| !on_a_card(e)), "none of them over the cards: {carded:?}");
    // the Gateway selected: its edges to the held agents' points, over the cards
    let gw = pressed_at(&rt, "account:gateway").unwrap();
    press(&mut rt, gw);
    let (g, shown, e) = drawn_edges(&rt);
    let i = g.find_in(0, "account:gateway").unwrap();
    let to_points: Vec<&lod::Drawn> = e.iter().filter(|e| e.lit && (shown[e.from].i == i || shown[e.to].i == i) && shown[e.from].look == Look::Dot && shown[e.to].look == Look::Dot).collect();
    assert!(to_points.iter().map(|e| e.count).sum::<u32>() >= 16, "the Gateway's edges to the held agents' points: {}", to_points.len());
    assert!(to_points.iter().all(|e| canvas::over_the_cards(e, &shown)), "drawn over the cards");
}

/// The window laid from its projects lays them once, and again only when what they are laid from changes: a post, an
/// agent's change, Insight's rows coming; frames with nothing new lay nothing (docket-edge6 N-1479 p35). The places are
/// kept in its file, read back as they were laid.
#[test]
fn the_islands_are_laid_only_when_what_they_are_laid_from_changes() {
    let (_held, s) = laid_app();
    let f = s.map.laying.at.clone().unwrap();
    let mut rt = window(s);
    let lays = |rt: &Runtime<App>| rt.state.map.laying.lays;
    assert_eq!(lays(&rt), 1, "laid as the Map opens");
    let kept = super::layout::read(&f).expect("the places written");
    assert_eq!(kept.islands.keys().collect::<Vec<_>>(), ["cav-paper", "research"]);
    for _ in 0..20 {
        rt.invalidate();
        rt.frame(0.016);
    }
    assert_eq!(lays(&rt), 1, "nothing new over twenty frames: not laid again");
    let nodes = |rt: &Runtime<App>| rt.state.map.graph().unwrap().nodes().len();
    let n = nodes(&rt);
    // a post on his chat
    {
        let p = &mut rt.state.projects[0];
        let v = p.live.tickets.get_mut(&1104).unwrap();
        let mut post = v.posts.last().unwrap().clone();
        post.n += 1;
        v.posts.push(post);
        p.live.tickets_rev += 1;
    }
    rt.frame(0.016);
    assert_eq!((lays(&rt), nodes(&rt)), (2, n + 1), "laid again for the post, which is its one new node");
    let g = rt.state.map.graph().unwrap().clone();
    assert!(g.find_in(0, "N-1104.p701").is_some_and(|i| g.node(i).parent == g.find_in(0, "N-1104")));
    // an agent at work again: its limit no longer holds it
    {
        let p = &mut rt.state.projects[0];
        p.live.agents[0].limit_until = None;
        p.live.agents[0].state = "working".into();
        p.live.agents_rev += 1;
    }
    for _ in 0..5 {
        rt.invalidate();
        rt.frame(0.016);
    }
    assert_eq!(lays(&rt), 3, "laid once for the agent");
    let i = rt.state.map.counts(0);
    assert_eq!(i.held, 15, "research's badge: one held fewer");
    // Insight's rows coming again, with a new line
    let mut rows = match rt.state.insights.threads.get(&0) {
        Some((Ok(r), _)) => r.clone(),
        _ => unreachable!(),
    };
    let mut x = rows[1].clone();
    x.id = "N-9000".into();
    rows.push(x);
    rt.state.insights.threads.insert(0, (Ok(rows), chrono::Utc::now().timestamp() + 1));
    rt.frame(0.016);
    rt.frame(0.016);
    assert_eq!(lays(&rt), 4);
    assert!(rt.state.map.graph().unwrap().find_in(0, "N-9000").is_some());
    let _ = std::fs::remove_file(&f);
}

/// A map.json this window cannot read (a later window's shape) is left byte for byte: the islands are laid all the
/// same, and again as the record changes, and nothing is written over it (docket-edge6 N-1479 p35).
#[test]
fn a_kept_file_of_a_later_shape_is_left_byte_for_byte() {
    let (_held, s) = laid_app();
    let f = s.map.laying.at.clone().unwrap();
    let later = br#"{"shape": 9, "islands": {"research": {"rect": [0, 0, 10, 10], "by": "a later window"}}}"#;
    std::fs::write(&f, later).unwrap();
    let mut rt = window(s);
    assert!(rt.state.map.graph().is_some_and(|g| g.project("project:research").is_some() && g.nodes().len() > 2_000), "laid all the same");
    relay(&mut rt);
    rt.frame(0.016);
    assert_eq!(rt.state.map.laying.lays, 2);
    assert_eq!(std::fs::read(&f).unwrap(), later, "left as it was");
    let _ = std::fs::remove_file(&f);
}

/// p715 path 2 on the record as the layout lays it: research's badge says 16 held; a double-click flies into research;
/// its sixteen held agents point at the gateway, each edge from an agent's point; a click on the gateway shows how
/// many, on which model, until when, and the press to its card on the Accounts tab, which opens it (the one press more
/// than p715: proto-lead N-1479 p55).
#[test]
fn path_2_through_the_layout_the_held_agents_point_at_the_gateway() {
    let (_held, s) = laid_app();
    let mut rt = window(s);
    let island = texts_in(&rt, "map-n-0");
    assert!(island.contains(&"16 held · limit".to_string()), "{island:?}");
    let at = bare_for_two(&rt, 0).expect("a bare place on research");
    rt.pointer_move(at[0], at[1]);
    for _ in 0..2 {
        rt.pointer_down(at[0], at[1]);
        rt.pointer_up(at[0], at[1]);
    }
    rt.frame(0.016);
    assert_eq!(selected(&rt).as_deref(), Some("project:research"));
    settle(&mut rt);
    let (g, shown, e) = drawn_edges(&rt);
    let gw = g.find_in(0, "account:gateway").expect("the gateway, in research's corner");
    // research's sixteen held agents, each a point or stood for by a near agent's (which may be one at work): each
    // held agent's edge ends at an agent's point, not at the topic it would roll up to were it not drawn
    let held = (0..g.nodes().len() as u32).filter(|i| g.node(*i).kind == Kind::Agent && g.node(*i).state == super::State::Held && g.root_of(*i) == 0).count();
    assert_eq!(held, 16);
    let agent = |x: &lod::Shown| g.node(x.i).kind == Kind::Agent;
    // (a drawn edge names its ends by their order among those shown, not by its direction)
    let into: Vec<&lod::Drawn> = e.iter().filter(|x| shown[x.to].i == gw || shown[x.from].i == gw).collect();
    assert_eq!(into.iter().map(|x| x.count).sum::<u32>(), 16, "each held agent's edge into the gateway");
    assert!(into.iter().all(|x| agent(&shown[if shown[x.to].i == gw { x.from } else { x.to }])), "from the agents' points");
    let p = pressed_at(&rt, "account:gateway").unwrap();
    press(&mut rt, p);
    assert_eq!(selected(&rt).as_deref(), Some("account:gateway"));
    let ins = texts_in(&rt, "map-inspector");
    assert!(ins.iter().any(|t| t.starts_with("16 held · claude-opus-4-5 until ")), "{ins:?}");
    assert!(ins.contains(&"Clear its cooldown in Accounts.".to_string()), "{ins:?}");
    assert_eq!(texts_in(&rt, "map-crumbs"), ["All", "›", "research", "›", "Gateway"]);
    let open = says(&rt, "Open the Accounts tab").expect("the press to its card");
    press(&mut rt, open);
    assert_eq!(rt.state.sel, Sel::Ext("accounts".into(), String::new()), "the Accounts tab, where the gateway's card is");
}

/// A double-click on a thread of one column taller than the view opens it at its top, as wide as it is, its first
/// posts open (his chat whole opened to a tenth of its fade: docket-edge6 N-1479 p47); a short one is fitted whole, as
/// before. His chat now takes columns, and opens at the column of its latest post (the test after this); the made-up
/// record's lines of one column are of 64 rows, so N-3009 is given 120 posts here: 256 rows, 960 tall.
#[test]
fn a_tall_thread_opens_at_its_top() {
    let (_held, mut s) = laid_app();
    let t = s.projects[0].live.tickets.values_mut().find(|t| t.id == "N-3009").expect("N-3009");
    let last = t.posts.last().unwrap().clone();
    for n in last.n + 1..=120 {
        t.posts.push(docket_machine::Post { n, ..last.clone() });
    }
    // its line is Insight's row (it has no outline: layout.rs `lines_of`), laid by its last post
    if let Some((Ok(rows), _)) = s.insights.threads.get_mut(&0) {
        rows.iter_mut().filter(|x| x.id == "N-3009").for_each(|x| x.last = Some(120));
    }
    let mut rt = window(s);
    let tall = "N-3009";
    {
        let g = rt.state.map.graph().unwrap();
        let r = g.node(g.find_in(0, tall).unwrap()).rect;
        let row = g.node(g.find_in(0, "N-3009.p1").unwrap()).rect;
        assert!(r.h >= 960.0 && (r.w - row.w).abs() < 1e-6, "a line of one column, 256 rows: {r:?}");
    }
    for id in [tall, "N-3711"] {
        assert!(canvas::fly_to_id(&mut rt.state, 0, id), "{id}");
        rt.invalidate();
        settle(&mut rt);
        let v = &rt.state.map;
        let g = v.graph().unwrap().clone();
        let i = g.find_in(0, id).unwrap();
        let t = v.shown().into_iter().find(|x| x.i == i).unwrap();
        assert_eq!(t.open, 1.0, "{id} open whole, side {}", lod::side(&t.r));
        let r = g.node(i).rect;
        // (the fit made as the double-click selects is made again in the canvas the inspector leaves: its top stays in
        // the upper part of the view)
        let top = v.cam.to_screen([r.x, r.y]);
        assert!(top[1] >= 0.0 && top[1] < v.size()[1] * 0.25, "{id}: its top in view: {top:?}");
        if id == tall {
            let first = g.find_in(0, &format!("{tall}.p1")).unwrap();
            assert!(v.shown().iter().any(|x| x.i == first), "its first post drawn");
        }
    }
}

/// A double-click on a line of more than one column fits the column holding its latest post, as wide as it is, its
/// foot at that post: his chat's 700 posts take three columns of 512 rows, and it opens on its last posts, not on its
/// whole card nor its first (docket-edge5 N-1479 p112, proto-lead p118).
#[test]
fn a_double_click_on_a_line_of_columns_fits_the_column_of_its_latest_post() {
    let (_held, s) = laid_app();
    let mut rt = window(s);
    assert!(canvas::fly_to_id(&mut rt.state, 0, "N-1104"));
    rt.invalidate();
    settle(&mut rt);
    let v = &rt.state.map;
    let g = v.graph().unwrap().clone();
    let i = g.find_in(0, "N-1104").unwrap();
    let line = g.node(i).rect;
    let posts: Vec<u32> = g.children(i).iter().copied().filter(|j| g.node(*j).kind == Kind::Post).collect();
    let last = *posts.iter().max_by_key(|j| crate::lens::parts(&g.node(**j).id).1.unwrap_or(0)).unwrap();
    assert_eq!(g.node(last).id, "N-1104.p700", "its latest post");
    let row = g.node(last).rect;
    assert!(line.w >= 3.0 * row.w - 1e-6 && row.x > line.x, "his chat in columns, its latest past the first: {line:?} {row:?}");
    let size = v.size();
    let (a, b) = (v.cam.to_screen([row.x, row.y]), v.cam.to_screen([row.right(), row.bottom()]));
    assert!(a[0] >= 0.0 && b[0] <= size[0] && b[0] - a[0] >= 0.9 * (size[0] - 2.0 * 28.0), "the column across the view: {a:?} {b:?} in {size:?}");
    assert!(b[1] <= size[1] && b[1] >= 0.5 * size[1], "its latest post in view, near its foot: {b:?} in {size:?}");
    let shown = v.shown();
    assert!(shown.iter().any(|x| x.i == last), "the latest post drawn");
    assert!(!shown.iter().any(|x| x.i == g.find_in(0, "N-1104.p1").unwrap()), "not the first column's top");
    assert_eq!(shown.iter().find(|x| x.i == i).unwrap().open, 1.0, "the line open");
}

/// A double-click anywhere on a post's row flies to its card (level 5, at the row's start: docket-edge8 N-1479
/// p71): its words in the card's top share as far as they fit, cut at a line, then "Open its page", which opens its
/// thread at it (proto-window p80); in the band below, where its steps go, why they are not drawn (deep.rs `note`).
#[test]
fn a_double_click_on_a_posts_row_flies_to_its_card_and_its_words_end_with_its_page() {
    // his chat's first post, long, with N-7's runs (wire_app's) made for it and not held; wire_app first, as it takes
    // the tests' lock that laid_app holds
    let w = crate::picture::docket::tests::wire_app();
    let ran = w.projects[0].live.tickets[&7].ran.clone();
    let (_held, mut s) = laid_app();
    let t = s.projects[0].live.tickets.get_mut(&1104).expect("his chat");
    t.posts[0].text = docket_machine::Text::New((1..=200).map(|k| format!("line {k} of a long post")).collect::<Vec<_>>().join("\n"));
    t.ran = ran;
    let mut rt = window(s);
    settle(&mut rt);
    let (i, row) = {
        let g = rt.state.map.graph().unwrap();
        let i = g.find_in(0, "N-1104.p1").expect("his chat's first post laid");
        (i, g.node(i).rect)
    };
    // the row across the view (level 4), double-clicked at its middle, far from its card
    let size = rt.state.map.size();
    moved_to(&mut rt, super::camera::Camera::fitting(&row, size, 0.0));
    rt.invalidate();
    rt.frame(0.016);
    let at = {
        let v = &rt.state.map;
        let shown = v.shown();
        let x = shown.iter().find(|x| x.i == i).expect("the row drawn");
        let q = [x.r[0] + x.r[2] * 0.5, x.r[1] + x.r[3] / 2.0];
        assert_eq!(lod::under(&shown, q).map(|k| shown[k].i), Some(i), "the row under its middle");
        assert!(q[0] > x.r[0] + 10.0 * x.r[3] * 1.5, "far from its card");
        [v.area[0] + q[0], v.area[1] + q[1]]
    };
    rt.pointer_move(at[0], at[1]);
    for _ in 0..2 {
        rt.pointer_down(at[0], at[1]);
        rt.pointer_up(at[0], at[1]);
    }
    rt.frame(0.016);
    assert_eq!(selected(&rt).as_deref(), Some("N-1104.p1"));
    settle(&mut rt);
    // its card fitted, whole in the canvas
    let v = &rt.state.map;
    let (c, size) = (v.cam.on_screen(&super::deep::card(row)), v.size());
    assert!(c[0] >= -0.5 && c[1] >= -0.5 && c[0] + c[2] <= size[0] + 0.5 && c[1] + c[3] <= size[1] + 0.5, "the card whole in view: {c:?} in {size:?}");
    assert!(c[2] >= 0.95 * (size[0] - 56.0) || c[3] >= 0.95 * (size[1] - 56.0), "fitted: {c:?} in {size:?}");
    // its words cut at a line, then Open its page, in its top share
    let (cr, _) = rt.presented(&format!("map-card-{i}")).expect("its card drawn");
    let top = cr[1] + cr[3] * 0.6;
    let (wr, _) = rt.presented(&format!("map-card-words-{i}")).expect("its words drawn");
    let line = rune::text::measure("Ag", &[], rune_kit::Kit::neutral(true).ty.body, None).1;
    assert!(wr[3] >= 10.0 * line && wr[3] < 100.0 * line, "cut at a line: {} tall, a line {line}", wr[3]);
    assert!(texts_in(&rt, &format!("map-card-words-{i}")).iter().any(|t| t.starts_with("line 1 of a long post")), "its words");
    let (open, _) = rt.presented(&format!("map-card-open-{i}")).expect("Open its page");
    assert!(open[1] >= wr[1] + wr[3] && open[1] + open[3] <= top + 0.5, "under its words, in the card's top: {open:?}, words {wr:?}, top {top}");
    // its steps' band says why they are not drawn
    let key = format!("map-card-note-{i}");
    let (note, _) = rt.presented(&key).expect("the band's note");
    assert!(note[1] >= top && note[1] + note[3] <= cr[1] + cr[3] + 0.5, "in the band: {note:?} in {cr:?}");
    assert!(texts_in(&rt, &key).iter().any(|t| t.starts_with("research disconnected; steps load")), "{:?}", texts_in(&rt, &key));
    // the press: its thread open at it
    press(&mut rt, [open[0] + open[2] / 2.0, open[1] + open[3] / 2.0]);
    assert_eq!(rt.state.tickets.shown, Some(1104), "its thread open");
}

/// A post held by reference (an agent's answer at its turn's end is a place in its transcript) says "…" on its card
/// until its words come, and the window asks for them through the post's own project's pipe, cav-paper's through
/// cav-paper's while research is the current project; once the answer comes the card draws them (consult-3 N-1479
/// p180, proto-lead p181). A pipe is a `cat` child, as view_cost's `linked_app`'s: not in the Windows test exe.
#[test]
#[cfg(not(windows))]
fn a_reference_posts_card_asks_for_its_words_and_draws_them_once_they_come() {
    let (_held, mut s) = laid_app();
    let mut cats = Vec::new();
    for p in &mut s.projects {
        let mut cat = std::process::Command::new("cat").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn().unwrap();
        p.link = Some(crate::link::Link::to_child(cat.stdin.take().unwrap()));
        cats.push(cat);
    }
    // paper-research's answer 699 and cav-writer's post on cav-paper's N-12, each a place in its coord's texts
    let place = |coord: &str, agent: &str| docket_machine::Loc { file: format!("/made-up/{coord}/tickets/texts/{agent}.jsonl"), offset: 4096, len: 812 };
    let (ours, theirs) = (place("research", "paper-research"), place("cav-paper", "cav-writer"));
    s.projects[0].live.tickets.get_mut(&1104).unwrap().posts[698].text = docket_machine::Text::Loc { loc: ours.clone() };
    s.projects[1].live.tickets.get_mut(&12).unwrap().posts[0].text = docket_machine::Text::Loc { loc: theirs.clone() };
    let mut rt = window(s);
    settle(&mut rt);
    let said = |rt: &Runtime<App>, key: &str| rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).and_then(|e| e.text);
    let asked = |rt: &Runtime<App>, pi: usize| -> Vec<String> {
        rt.state.projects[pi].pending.values().filter_map(|w| if let crate::Pending::TicketText { key } = w { Some(key.clone()) } else { None }).collect()
    };
    let card_of = |rt: &mut Runtime<App>, project: &str, id: &str| -> u32 {
        let (i, card) = {
            let g = rt.state.map.graph().unwrap();
            let i = g.project(project).and_then(|r| g.find_in(r, id)).expect("the post laid");
            (i, super::deep::card(g.node(i).rect))
        };
        let size = rt.state.map.size();
        moved_to(rt, super::camera::Camera::fitting(&card, size, 0.0));
        rt.invalidate();
        rt.frame(0.016);
        i
    };
    // research's: "…" on its card, asked once on research's pipe
    let i = card_of(&mut rt, "project:research", "N-1104.p699");
    let words = format!("map-card-words-{i}");
    assert_eq!(said(&rt, &words).as_deref(), Some("…"), "its words not read yet");
    assert!(asked(&rt, 0).is_empty() && asked(&rt, 1).is_empty(), "nothing asked before the window's poll");
    assert!(crate::texts::poll(&mut rt.state), "the poll asks");
    let key = crate::texts::key(&ours);
    assert_eq!(asked(&rt, 0), vec![key.clone()], "asked on research's pipe");
    assert!(asked(&rt, 1).is_empty(), "nothing on cav-paper's");
    rt.frame(0.016);
    crate::texts::poll(&mut rt.state);
    assert_eq!(asked(&rt, 0).len(), 1, "asked once while it is out");
    // the answer: its words on the card
    let id = *rt.state.projects[0].pending.iter().find(|(_, w)| matches!(w, crate::Pending::TicketText { key: k } if *k == key)).unwrap().0;
    rt.state.projects[0].pending.remove(&id);
    crate::threads::replied(&mut rt.state, crate::Pending::TicketText { key }, true, "Built and checked: the fix is on edge6-wire.".into());
    rt.invalidate();
    rt.frame(0.016);
    assert_eq!(said(&rt, &words).as_deref(), Some("Built and checked: the fix is on edge6-wire."), "its words once they came");
    // cav-paper's card, research still the current project: asked on cav-paper's pipe
    assert_eq!(rt.state.cur, 0);
    let i = card_of(&mut rt, "project:cav-paper", "N-12.p1");
    assert_eq!(said(&rt, &format!("map-card-words-{i}")).as_deref(), Some("…"), "cav-paper's not read yet");
    assert!(crate::texts::poll(&mut rt.state));
    assert_eq!(asked(&rt, 1), vec![crate::texts::key(&theirs)], "asked on cav-paper's pipe");
    assert!(asked(&rt, 0).is_empty(), "research's pipe asked nothing more");
    for p in &mut rt.state.projects {
        p.link = None;
    }
    for mut c in cats {
        let _ = c.wait();
    }
}

/// At level 5 a line's first post's card draws its words, and the line's name, which an open line draws at its top
/// left, is not drawn over them: row 1 starts at the line's corner (layout.rs `Rows::cell`), so "section 12" lay
/// across cav-paper's N-12.p1 words (docket-edge6 N-1479 p192, proto-lead p193). The whole line in view, its posts too
/// small for words, the name is drawn at its top.
#[test]
fn a_lines_name_is_not_drawn_over_its_first_posts_card_words() {
    let (_held, s) = laid_app();
    let mut rt = window(s);
    settle(&mut rt);
    let fit = |rt: &mut Runtime<App>, r: Rect| {
        let size = rt.state.map.size();
        moved_to(rt, super::camera::Camera::fitting(&r, size, 0.0));
        rt.invalidate();
        rt.frame(0.016);
    };
    let names_at = |rt: &Runtime<App>, name: &str| -> Vec<[f32; 4]> {
        rt.elements().into_iter().filter(|e| !e.exiting && e.text.as_deref() == Some(name)).map(|e| e.rect).collect()
    };
    let (i, card, name) = {
        let g = rt.state.map.graph().unwrap();
        let root = g.project("project:cav-paper").unwrap();
        let (i, line) = (g.find_in(root, "N-12.p1").expect("its first post"), g.find_in(root, "N-12").expect("the line"));
        (i, super::deep::card(g.node(i).rect), g.node(line).label.clone())
    };
    fit(&mut rt, card);
    let (words, _) = rt.presented(&format!("map-card-words-{i}")).expect("its words drawn");
    let over: Vec<[f32; 4]> = names_at(&rt, &name).into_iter().filter(|r| lod::overlap(r, &words)).collect();
    assert!(over.is_empty(), "the line's name {name:?} at {over:?}, over its first post's words at {words:?}");
    // his chat whole, 700 posts in its rows: its name at its top
    let (chat, rect, label) = {
        let g = rt.state.map.graph().unwrap();
        let chat = g.find_in(0, "N-1104").unwrap();
        (chat, g.node(chat).rect, g.node(chat).label.clone())
    };
    fit(&mut rt, rect);
    assert!(texts_in(&rt, &format!("map-n-{chat}")).contains(&label), "his chat's name drawn: {:?}", texts_in(&rt, &format!("map-n-{chat}")));
}

/// At level 4 a post's row draws its card at its start, 3:2, and says its words after it (docket-edge8 N-1479 p71,
/// proto-window p80).
#[test]
fn a_posts_row_says_its_words_after_its_card() {
    let (_held, s) = laid_app();
    let mut rt = window(s);
    settle(&mut rt);
    let (i, row, label) = {
        let g = rt.state.map.graph().unwrap();
        let i = g.find_in(0, "N-1104.p3").expect("a post of his chat");
        (i, g.node(i).rect, g.node(i).label.clone())
    };
    // near enough that the row is drawn as a card (lod.rs `band`: from 220 points a side), its start in view
    let scale = 1.2 * 220.0 * lod::FADE as f64 / (row.w * row.h).sqrt();
    moved_to(&mut rt, super::camera::Camera { at: [row.x - 40.0 / scale, row.y - 200.0 / scale], scale });
    for _ in 0..3 {
        rt.invalidate();
        rt.frame(0.016);
    }
    let x = rt.state.map.shown().into_iter().find(|x| x.i == i).expect("the row drawn");
    assert!(matches!(x.look, Look::Card | Look::Open), "{:?}", x.look);
    let (cr, _) = rt.presented(&format!("map-card-{i}")).expect("its card drawn");
    let (rr, _) = rt.presented(&format!("map-n-{i}")).unwrap();
    assert!((cr[0] - rr[0]).abs() < 0.5 && (cr[1] - rr[1]).abs() < 0.5 && (cr[2] - 1.5 * rr[3]).abs() < 0.5, "at its row's start, 3:2: {cr:?} in {rr:?}");
    let words = rt.elements().into_iter().find(|e| e.text.as_deref() == Some(label.as_str()) && e.rect[1] >= rr[1] && e.rect[1] < rr[1] + rr[3]).expect("its words");
    assert!(words.rect[0] >= cr[0] + cr[2], "after its card: {:?} against {cr:?}", words.rect);
}

/// A double-click at AT, its two presses as `canvas::drag` reads them.
fn twice(rt: &mut Runtime<App>, at: [f32; 2]) {
    rt.pointer_move(at[0], at[1]);
    for _ in 0..2 {
        rt.pointer_down(at[0], at[1]);
        rt.pointer_up(at[0], at[1]);
    }
    rt.frame(0.016);
}

/// Frames enough for the deep levels to place and draw what the frame before showed (canvas.rs `deep_step`, in the
/// poll after a frame).
fn deep_frames(rt: &mut Runtime<App>) {
    for _ in 0..4 {
        rt.invalidate();
        rt.frame(0.016);
    }
}

/// Where element KEY is drawn, found whole in the canvas and in node I as drawn: a press on it lands on it, and none
/// of it is cut off.
#[cfg(not(windows))]
fn whole_in(rt: &Runtime<App>, i: u32, key: &str) -> [f32; 4] {
    let (r, _) = rt.presented(key).unwrap_or_else(|| panic!("{key} drawn"));
    let (n, _) = rt.presented(&format!("map-n-{i}")).unwrap_or_else(|| panic!("node {i} drawn"));
    let a = rt.state.map.area;
    let within = |r: [f32; 4], o: [f32; 4]| r[0] >= o[0] - 0.5 && r[1] >= o[1] - 0.5 && r[0] + r[2] <= o[0] + o[2] + 0.5 && r[1] + r[3] <= o[1] + o[3] + 0.5;
    assert!(within(r, a) && within(r, n), "{key} whole in the canvas {a:?} and in its node {n:?}: {r:?}");
    r
}

/// Path 5's record: [`p700_ran`], and agent-base's calls from `live_folders`, in the folder returned.
fn path_5_app() -> (std::sync::MutexGuard<'static, ()>, App, std::path::PathBuf) {
    let (held, mut s) = p700_ran();
    let dir = crate::picture::docket::tests::live_folders(&mut s, &format!("map-path-5-{}", KEPT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
    (held, s, dir)
}

/// The window laid from the made-up snapshot (`laid_app`), his latest post, p700, given the run of docket.rs
/// `wire_app` that it holds (made up: pr's `answer`, standing for route, then the watcher's `next`), held as a run the
/// Map asked for is held.
fn p700_ran() -> (std::sync::MutexGuard<'static, ()>, App) {
    // wire_app first, as it takes the tests' lock that laid_app holds
    let w = crate::picture::docket::tests::wire_app();
    let (mut ran, runs) = (w.projects[0].live.tickets[&7].ran.clone(), w.tickets.find.fetched[&w.projects[0].id].clone());
    drop(w);
    let (held, mut s) = laid_app();
    // only the run held: N-7's others are not, and its card would say they are being read
    ran.retain(|r| runs.values().any(|v| v.id == r.id));
    for r in &mut ran {
        r.post = Some(700);
    }
    s.projects[0].live.tickets.get_mut(&1104).expect("his chat").ran = ran;
    let pid = s.projects[0].id.clone();
    s.tickets.find.fetched.insert(pid, runs);
    (held, s)
}

/// The key of the stage of path 5's step: the first of its run's (pr's `answer`).
fn path_5_stage() -> String {
    format!("{}.0", crate::picture::docket::tests::run_id(1))
}

/// p715 path 5, "Change the routing prompt" ("From any post of yours, zoom into route. The template is there … Click
/// Edit: it shows what it reaches … Save"), on its record ([`path_5_app`]) as the layout lays it, by presses at the
/// points drawn.
/// - His chat from the explorer opens at the column of his latest posts; a double-click on p700's row flies to its
///   card, where its steps lie in the band (level 5).
/// - A double-click on its first step flies into it: its stage, and its call's template (level 6).
/// - Edit opens the call's lines in place in the step, with what Save applies to and what it reaches.
/// - Save writes the project's own copy, the edit closes, and the step draws the template as saved.
///
/// At each double-click: what is selected, the inspector, the crumb trail. Not in the Windows test exe, as deep.rs's
/// prompt tests (31f2b5cb): the calls are read from this machine's files (his window's come served: the stills).
/// Not drawn: the template filled in for this post before it is sent (deep.rs; the wire's `model-for` line has it as
/// sent).
#[test]
#[cfg(not(windows))]
fn path_5_from_his_post_its_steps_prompt_is_edited_and_saved_on_the_map() {
    let (_held, s, dir) = path_5_app();
    let mut rt = window(s);
    settle(&mut rt);
    // his chat from the explorer, at the column of his latest posts; p700's row double-clicked: its card
    assert!(canvas::fly_to_id(&mut rt.state, 0, "N-1104"), "his chat on the Map");
    settle(&mut rt);
    let post = rt.state.map.graph().unwrap().find_in(0, "N-1104.p700").expect("his latest post laid");
    assert!(rt.state.map.graph().unwrap().node(post).his, "a post of his");
    let at = bare_for_two(&rt, post).expect("his latest post's row in view");
    twice(&mut rt, at);
    assert_eq!(selected(&rt).as_deref(), Some("N-1104.p700"));
    settle(&mut rt);
    deep_frames(&mut rt);
    let key = path_5_stage();
    let sid = super::deep::step_id(&key);
    let g = rt.state.map.graph().unwrap().clone();
    let step = g.find_in(0, &sid).expect("its steps placed once its card is open");
    assert_eq!(g.node(step).parent, Some(post));
    assert!(super::deep::card(g.node(post).rect).holds(&g.node(step).rect), "in its card");
    assert!(rt.find(&format!("map-card-note-{post}")).is_none(), "its run held: its band says nothing of reading it");
    let label = g.node(step).label.clone();
    let crumbs = |i: u32| -> Vec<String> { std::iter::once("All".to_string()).chain(g.path(i).iter().flat_map(|p| ["›".to_string(), g.node(*p).label.clone()])).collect() };
    assert_eq!(texts_in(&rt, "map-crumbs"), crumbs(post));
    let ins = texts_in(&rt, "map-inspector");
    assert!(ins.first().is_some_and(|t| t == "post") && ins.contains(&"Open its page".to_string()), "{ins:?}");
    // its first step double-clicked: its stage and its call's template, at the step's fit
    let at = bare_for_two(&rt, step).expect("its step in the card's band, apart from its wire");
    twice(&mut rt, at);
    assert_eq!(selected(&rt), Some(sid.clone()));
    settle(&mut rt);
    deep_frames(&mut rt);
    let x = rt.state.map.shown().into_iter().find(|x| x.i == step).expect("the step drawn");
    assert!(x.r[2] >= super::deep::PROMPT_PX, "wide enough to draw its prompt: {:?}", x.r);
    assert_eq!(texts_in(&rt, "map-crumbs"), crumbs(step));
    let ins = texts_in(&rt, "map-inspector");
    assert!(ins.first().is_some_and(|t| t == "step") && ins.contains(&label), "{ins:?}");
    let here = format!("map-step-{key}");
    assert!(texts_in(&rt, &here).iter().any(|t| t.contains("A question for you:")), "its template: {:?}", texts_in(&rt, &here));
    // Edit: the call's lines in place in the step, what Save applies to and what it reaches
    let edit = whole_in(&rt, step, &format!("map-edit-{key}"));
    press(&mut rt, [edit[0] + edit[2] / 2.0, edit[1] + edit[3] / 2.0]);
    assert_eq!(rt.state.tickets.edits.call_edit.as_ref().map(|e| e.call.as_str()), Some("answer"), "Edit pressed");
    assert_eq!(rt.state.deep.editing, Some((0, sid.clone())), "edited in this step");
    deep_frames(&mut rt);
    let ed = texts_in(&rt, "call-editor");
    assert!(ed.iter().any(|t| t.starts_with("Applies to every step after Save")), "{ed:?}");
    // the step drawn after its calls and their labels, so none lies over its editor (deep.rs `over_calls`: the Wire
    // call did, w1, then its label, w2)
    let els = rt.elements();
    let it = els.iter().position(|e| e.key.as_deref() == Some(format!("map-n-{step}").as_str())).expect("the step drawn");
    // the canvas's own: not the depth switch over it, whose words are not the calls'
    let switch = els.iter().find(|e| e.key.as_deref() == Some("map-layers")).map(|e| e.path.clone()).expect("the depth switch");
    let calls: Vec<usize> = (els.iter().enumerate())
        .filter(|(_, e)| !e.path.starts_with(&switch))
        .filter(|(_, e)| g.children(step).iter().any(|c| e.key.as_deref() == Some(format!("map-n-{c}").as_str()) || e.text.as_deref() == Some(g.node(*c).label.as_str())))
        .map(|(j, _)| j)
        .collect();
    assert!(!calls.is_empty() && calls.iter().all(|c| *c < it), "its calls and their labels drawn, under it: {calls:?}, the step {it}");
    assert!(ed.iter().any(|t| t == "Pause to see what it reaches."), "{ed:?}");
    // his change and why, then Save: the project's own copy, and the step's template as saved
    rt.update(|s: &mut App, _| {
        let e = s.tickets.edits.call_edit.as_mut().unwrap();
        e.text = rune::TextEdit::new(e.text.text.replacen("Answer it;", "Answer it, naming the file you read;", 1));
        s.tickets.edits.why = rune::TextEdit::new("name the file");
    });
    deep_frames(&mut rt);
    assert!(crate::edit::reach_of(&rt.state).is_some_and(|(f, t)| f == dir.join("proj/calls/answer.call") && t.contains("naming the file you read")), "what its reach asks of: the file Save writes");
    let save = whole_in(&rt, step, "edit-call-save");
    press(&mut rt, [save[0] + save[2] / 2.0, save[1] + save[3] / 2.0]);
    let copy = std::fs::read_to_string(dir.join("proj/calls/answer.call")).unwrap_or_else(|e| panic!("its copy written ({e}): {:?}", rt.state.tickets.edits.said));
    assert!(copy.contains("naming the file you read"), "{copy}");
    deep_frames(&mut rt);
    assert!(rt.state.tickets.edits.call_edit.is_none() && rt.find("call-editor").is_none(), "the edit closed");
    assert!(texts_in(&rt, &here).iter().any(|t| t.contains("naming the file you read")), "the template as saved: {:?}", texts_in(&rt, &here));
    let _ = std::fs::remove_dir_all(&dir);
}

/// p700's lines of the record from its post on, made up, as coordd gives them (lens.rs `came`): his post, the run it
/// opened, its step 1 pr's, handed to pr and answered Unclear, its step 2 the watcher's; then its deliveries to
/// twenty-four agents, so its rows run down its card past where its steps lie.
fn p700_lines() -> String {
    use serde_json::json;
    let run = crate::picture::docket::tests::run_id(1);
    let t: u64 = run[2..].parse().unwrap();
    let mut lines = vec![
        (201001, json!({"ev": "post", "at": "2026-11-21T09:00:00Z", "t": 1104, "by": "owner", "text": "which build has the fix?"})),
        (201002, json!({"ev": "opened", "at": "2026-11-21T09:00:00Z", "t": t, "kind": "owner_post", "parent": 1104, "program": "r <- answer"})),
        (201003, json!({"ev": "step", "at": "2026-11-21T09:00:01Z", "t": t, "step": 1, "worker": "pr", "call": "answer"})),
        (201004, json!({"ev": "given", "at": "2026-11-21T09:00:01Z", "t": t, "to": "pr", "delivery": format!("{run}.s1"), "upto": 0})),
        (201005, json!({"ev": "answer", "at": "2026-11-21T09:00:09Z", "t": t, "by": "pr", "head": {"head": "Unclear", "why": "Which of the two builds is meant is not said."}, "text": ""})),
        (201006, json!({"ev": "step", "at": "2026-11-21T09:00:09Z", "t": t, "step": 2, "worker": "watcher", "call": "next"})),
    ];
    lines.extend((1..=24).map(|k| (201010 + k, json!({"ev": "event", "at": "2026-11-21T09:00:10Z", "name": "delivered", "value": {"delivery": "N-1104.p700", "t": 1104, "to": format!("agent-{k}")}}))));
    lines.into_iter().map(|(line, v)| serde_json::to_string(&coord_types::RecordLine { line, text: v.to_string() }).unwrap()).collect::<Vec<_>>().join("\n")
}

/// The depth switch's record: his p700 with its run's steps ([`p700_ran`]) and its lines of the record held, made up
/// ([`p700_lines`]).
fn depths_app() -> (std::sync::MutexGuard<'static, ()>, App) {
    let (held, mut s) = p700_ran();
    crate::lens::came(&mut s, 0, "N-1104.p700".into(), None, true, p700_lines());
    (held, s)
}

/// [`depths_app`] for a still, with no lock held.
fn depths_shown() -> App {
    depths_app().1
}

/// The depth the Map's switch lights.
fn lit_depth(rt: &Runtime<App>) -> Option<String> {
    rt.elements().into_iter().find(|e| !e.exiting && e.role == "radio" && e.item_selected && e.path.split('/').any(|k| k == "map-layers")).and_then(|e| e.label)
}

/// The switch's segment WORD: where it is, and whether a press takes it.
fn depth_segment(rt: &Runtime<App>, word: &str) -> ([f32; 2], bool) {
    let e = rt.elements().into_iter().find(|e| !e.exiting && e.role == "radio" && e.label.as_deref() == Some(word) && e.path.split('/').any(|k| k == "map-layers"));
    let e = e.unwrap_or_else(|| panic!("the switch's {word}"));
    ([e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0], e.focusable)
}

/// The switch's segment WORD pressed, as his click does, and the flight it starts landed.
fn press_depth(rt: &mut Runtime<App>, word: &str) {
    let (at, on) = depth_segment(rt, word);
    assert!(on, "{word} is on");
    press(rt, at);
    settle(rt);
    deep_frames(rt);
}

/// A point over KEY, a control floating on the canvas, with a node of the Map under it, in window points.
fn over_a_node(rt: &Runtime<App>, key: &str) -> [f32; 2] {
    let (r, _) = rt.presented(key).unwrap_or_else(|| panic!("{key} drawn"));
    let (a, shown) = (rt.state.map.area, rt.state.map.shown());
    (1..10)
        .flat_map(|y| (1..10).map(move |x| [r[0] + r[2] * x as f32 / 10.0, r[1] + r[3] * y as f32 / 10.0]))
        .find(|q| lod::under(&shown, [q[0] - a[0], q[1] - a[1]]).is_some())
        .unwrap_or_else(|| panic!("a node under {key}"))
}

/// What the Map says of where it is: its crumbs, then its switch's help.
fn head_says(rt: &Runtime<App>) -> (Vec<String>, Option<String>) {
    (texts_in(rt, "map-crumbs"), rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some("map-layers")).and_then(|e| e.help))
}

/// structure.md §2 and §9 on the Map (paper-research N-1446 p1108; docket-edge6 p1125): the switch over the canvas's
/// top left lights the depth the Map is at, and each press goes there in place, on his p700 with its run's steps
/// ([`p700_ran`]) and its lines of the record made up ([`p700_lines`]).
/// - The whole record: Cloud, Events and Record off with why. His chat: Structure.
/// - p700 fitted and Events pressed: its card draws its events' rows in place of its words, none of its steps; the
///   crumbs end with Events.
/// - His Comment on an event's words: the box under them, in its room, to N-1104 (proto-window p1130).
/// - Where a step lay, a hover is the post's, and a press on the event's row there is its Record, in place, that line
///   whole (consult-3 p1129).
/// - Structure: its words and steps back, at the same camera. Events, then Esc: Structure, at the same camera.
/// - Events, then step 1's "Wire ›": its call, flown into, Wire lit. Esc: Structure.
/// - Its step double-clicked, then Wire: its call again. Over the switch and the minimap, nothing under them hovered.
///   Cloud: the whole record.
#[test]
fn the_depth_switch_goes_from_the_cloud_down_to_the_wire_and_back_up_a_press_each() {
    let (_held, s) = depths_app();
    let mut rt = window(s);
    settle(&mut rt);
    assert_eq!(lit_depth(&rt).as_deref(), Some("Cloud"), "the whole record");
    assert!(!depth_segment(&rt, "Events").1 && !depth_segment(&rt, "Record").1 && !depth_segment(&rt, "Wire").1, "nothing to draw them of");
    assert_eq!(head_says(&rt).1.as_deref(), Some("Select a thread, post or step"));
    assert!(canvas::fly_to_id(&mut rt.state, 0, "N-1104"), "his chat on the Map");
    settle(&mut rt);
    assert_eq!(lit_depth(&rt).as_deref(), Some("Structure"), "his chat");
    assert_eq!(head_says(&rt).1.as_deref(), Some("Wire: select a step or a call"), "his chat's Events and Record on");
    // p700's card, its steps in its band
    let post = rt.state.map.graph().unwrap().find_in(0, "N-1104.p700").expect("his latest post laid");
    let at = bare_for_two(&rt, post).expect("p700's row in view");
    twice(&mut rt, at);
    settle(&mut rt);
    deep_frames(&mut rt);
    let g = rt.state.map.graph().unwrap().clone();
    let sid = super::deep::step_id(&path_5_stage());
    let step = g.find_in(0, &sid).expect("its steps placed");
    let call = format!("{}.s1", crate::picture::docket::tests::run_id(1));
    let c = g.find_in(0, &call).expect("its step's call placed");
    assert_eq!(g.node(c).parent, Some(step));
    assert!(rt.presented(&format!("map-n-{step}")).is_some() && rt.presented(&format!("map-card-words-{post}")).is_some(), "its words and its step drawn");
    assert_eq!(lit_depth(&rt).as_deref(), Some("Structure"), "a post");
    let fitted = rt.state.map.cam;

    // Events: its events in its card, nothing of what it holds
    press_depth(&mut rt, "Events");
    assert_eq!(lit_depth(&rt).as_deref(), Some("Events"));
    assert_eq!(selected(&rt).as_deref(), Some("N-1104.p700"));
    let lines = canvas::lines_key(0, "N-1104.p700", Depth::Events);
    let rows = texts_in(&rt, &lines);
    assert!(rows.iter().any(|t| t == "pr answered Unclear: Which of the two builds is meant is not said.") && rows.iter().any(|t| t == "delivered N-1104.p700 to agent-1"), "its events: {rows:?}");
    assert!(rt.presented(&format!("map-card-words-{post}")).is_none() && rt.presented(&format!("map-n-{step}")).is_none(), "none of its words or steps");
    assert!(rt.state.map.shown().iter().all(|x| x.i == post || !g.path(x.i).contains(&post)), "nothing inside it drawn");
    let (crumbs, _) = head_says(&rt);
    assert_eq!(crumbs[crumbs.len() - 3..], [g.node(post).label.clone(), "›".into(), "Events".into()], "{crumbs:?}");
    // his Comment on an event's words: the box under them, to his chat
    let key = "event-N-1104.p700-201005-text";
    let sel = crate::ask::anchor_tests::select(&mut rt, key, "Which of the two builds is meant", "").expect("its words selectable");
    rt.update(move |s, _| crate::lens::comment(s, 0, "N-1104.p700", Depth::Events, &sel));
    crate::ask::anchor_tests::settle(&mut rt);
    let p = rt.state.notes.picked.clone().expect("a comment being written");
    assert_eq!((p.target.as_str(), p.label.as_str()), ("thread:N-1104", "the events of N-1104.p700"));
    crate::ask::anchor_tests::in_room(&rt, &lines, "step 2: next, by watcher", "the Map's Events");
    rt.update(|s, _| crate::ask::unwrite(s));
    crate::ask::anchor_tests::settle(&mut rt);
    // where its step lay: the hover is the post's; a press on the event row there is its Record, in place
    let a = rt.state.map.area;
    let sr = rt.state.map.cam.on_screen(&g.node(step).rect);
    let (line, q) = rt
        .elements()
        .into_iter()
        .filter(|e| !e.exiting && e.path.split('/').any(|k| k == lines))
        .filter_map(|e| Some((e.key.as_deref()?.strip_prefix("event-")?.parse::<u64>().ok()?, e.rect)))
        .find_map(|(n, r)| {
            let (x0, y0, x1, y1) = (r[0].max(a[0] + sr[0]), r[1].max(a[1] + sr[1]), (r[0] + r[2]).min(a[0] + sr[0] + sr[2]), (r[1] + r[3]).min(a[1] + sr[1] + sr[3]));
            (x1 - x0 > 8.0 && y1 - y0 > 8.0).then(|| (n, [(x0 + x1) / 2.0, (y0 + y1) / 2.0]))
        })
        .expect("an event's row over where its step lay");
    // the pointer moved there off the switch, whose press left it on its Events
    rt.pointer_move(q[0], q[1]);
    rt.frame(0.016);
    assert_eq!(rt.state.map.hover.as_ref().map(|n| n.1.as_str()), Some("N-1104.p700"), "the hover there is the post's");
    press(&mut rt, q);
    deep_frames(&mut rt);
    assert!(canvas::shown(&rt.state), "still the Map");
    assert_eq!(lit_depth(&rt).as_deref(), Some("Record"), "its Record");
    assert_eq!(selected(&rt).as_deref(), Some("N-1104.p700"), "the post's, not the step's");
    assert!(rt.state.records.whole.contains(&(0, line)), "line {line} whole");
    let record = canvas::lines_key(0, "N-1104.p700", Depth::Record);
    assert!(texts_in(&rt, &record).iter().any(|t| t.contains(r#""ev":"answer""#)), "the lines as written: {:?}", texts_in(&rt, &record));
    // Structure: its words and steps back, the camera kept
    let cam = rt.state.map.cam;
    press_depth(&mut rt, "Structure");
    assert_eq!(lit_depth(&rt).as_deref(), Some("Structure"));
    assert!(rt.presented(&format!("map-card-words-{post}")).is_some() && rt.presented(&format!("map-n-{step}")).is_some(), "its words and steps back");
    assert_eq!(rt.state.map.cam, cam, "the camera kept");
    // Events, then Esc: one press up, to Structure, the camera kept
    press_depth(&mut rt, "Events");
    let cam = rt.state.map.cam;
    let esc = rune::KeyEvent { key: rune::Key::Escape, mods: Default::default() };
    rt.key(esc.clone());
    deep_frames(&mut rt);
    assert_eq!(lit_depth(&rt).as_deref(), Some("Structure"), "Esc: up a depth");
    assert!(rt.presented(&format!("map-n-{step}")).is_some() && !rt.state.map.flying(), "its steps back, nothing flown");
    assert_eq!(rt.state.map.cam, cam, "the camera kept");

    // Events, then step 1's Wire ›: its call, in place
    press_depth(&mut rt, "Events");
    let (wr, _) = rt.presented("wire-201003").expect("step 1's Wire ›");
    press(&mut rt, [wr[0] + wr[2] / 2.0, wr[1] + wr[3] / 2.0]);
    settle(&mut rt);
    deep_frames(&mut rt);
    assert!(canvas::shown(&rt.state), "still the Map");
    assert_eq!(selected(&rt), Some(call.clone()), "its call");
    assert_eq!(lit_depth(&rt).as_deref(), Some("Wire"));
    rt.key(esc.clone());
    settle(&mut rt);
    assert_eq!(lit_depth(&rt).as_deref(), Some("Structure"), "Esc from the wire");
    // its step, then Wire: its call
    rt.state.map.cam = fitted;
    deep_frames(&mut rt);
    let at = bare_for_two(&rt, step).expect("its step in the card's band");
    twice(&mut rt, at);
    settle(&mut rt);
    deep_frames(&mut rt);
    assert_eq!(selected(&rt), Some(sid.clone()));
    assert_eq!(lit_depth(&rt).as_deref(), Some("Structure"), "a step");
    press_depth(&mut rt, "Wire");
    assert_eq!(selected(&rt), Some(call.clone()), "its call");
    assert_eq!(lit_depth(&rt).as_deref(), Some("Wire"));
    // over the switch, and over the minimap, nothing of the Map under them is hovered or previewed; off them, it is
    for key in ["map-layers", "map-mini"] {
        let q = over_a_node(&rt, key);
        rt.pointer_move(q[0], q[1]);
        rt.frame(0.016);
        assert!(rt.state.map.hover.is_none() && rt.presented("map-preview").is_none(), "over {key}: {:?} hovered", rt.state.map.hover);
    }
    let a = rt.state.map.area;
    rt.pointer_move(a[0] + a[2] / 2.0, a[1] + a[3] / 2.0);
    rt.frame(0.016);
    assert!(rt.state.map.hover.is_some(), "the canvas's middle hovered again");
    // Cloud: the whole record
    press_depth(&mut rt, "Cloud");
    assert_eq!(lit_depth(&rt).as_deref(), Some("Cloud"));
    assert_eq!(drawn(&rt, "project:research").map(|d| d.1), Some(Look::Open), "the whole record");
}

/// The depth switch's stills (N-1446 p1108, docket-edge6 p1125), 1400 × 900 on the made-up record ([`depths_app`]):
/// his p700 from the cloud down to the wire, one still a depth, by the presses his window takes: double-clicks into
/// research, his chat's topic, his chat and p700, then the switch's segments.
/// - layer-1-cloud: the record as the Map opens.
/// - layer-2-structure: p700's card, its words and its steps in its band.
/// - layer-3-events: Events pressed, its events in its card; layer-4-record: Record pressed, its lines as written.
/// - layer-5-wire: from depth 2, its first step double-clicked, then Wire: its call.
///
/// The points pressed are read from the same window run headless first.
/// `STILL_OUT=DIR yaaihh-tests map::paths::map_layer_stills --ignored --exact`.
#[test]
#[ignore]
fn map_layer_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let (into, events, record, step, wire) = {
        let (_held, s) = depths_app();
        let mut rt = window(s);
        let at = |rt: &Runtime<App>, id: &str| rt.state.map.graph().unwrap().find_in(0, id).unwrap_or_else(|| panic!("{id} on the Map"));
        let mut into = Vec::new();
        let mut go = |rt: &mut Runtime<App>, i: u32, what: &str| {
            let q = bare_for_two(rt, i).unwrap_or_else(|| panic!("{what} in view"));
            twice(rt, q);
            settle(rt);
            deep_frames(rt);
            into.push(q);
        };
        go(&mut rt, 0, "research");
        let chat = at(&rt, "N-1104");
        let topic = rt.state.map.graph().unwrap().node(chat).parent.unwrap();
        go(&mut rt, topic, "his chat's topic");
        go(&mut rt, chat, "his chat");
        let p700 = at(&rt, "N-1104.p700");
        go(&mut rt, p700, "his latest post");
        let (events, record, wire) = (depth_segment(&rt, "Events").0, depth_segment(&rt, "Record").0, depth_segment(&rt, "Wire").0);
        let step = bare_for_two(&rt, at(&rt, &super::deep::step_id(&path_5_stage()))).expect("its first step in its card");
        (into, events, record, step, wire)
    };
    let twice = |t: f32, q: [f32; 2]| format!(r#"{{"t": {t}, "type": "double-click", "x": {}, "y": {}}}"#, q[0], q[1]);
    let click = |t: f32, q: [f32; 2]| format!(r#"{{"t": {t}, "type": "click", "x": {}, "y": {}}}"#, q[0], q[1]);
    let to_post = into.iter().zip([0.3, 1.8, 3.4, 5.0]).map(|(q, t)| twice(t, *q)).collect::<Vec<_>>().join(", ");
    let stills: [(&str, String, &str); 5] = [
        ("layer-1-cloud", "[]".into(), "1"),
        ("layer-2-structure", format!("[{to_post}]"), "6.6"),
        ("layer-3-events", format!("[{to_post}, {}]", click(6.6, events)), "8.2"),
        ("layer-4-record", format!("[{to_post}, {}]", click(6.6, record)), "8.2"),
        ("layer-5-wire", format!("[{to_post}, {}, {}]", twice(6.6, step), click(8.2, wire)), "9.8"),
    ];
    for (name, script, at) in stills {
        let f = out.join(format!("{name}.json"));
        std::fs::write(&f, script).unwrap();
        let args = vec!["--snap".to_string(), out.join(format!("{name}.png")).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), at.into(), "--script".into(), f.display().to_string()];
        let spec = rune::AppSpec { poll: Some(map_poll), on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), depths_shown, crate::view) };
        rune::App::new(spec).args(args).try_run().unwrap();
    }
}

/// consult-3 N-1446 p1169, proto-window p1172 point 2: his chat at Events, dragged left until 190 points of it are in
/// view, less than its lines' room (canvas.rs `LAYER_ROOM`, 200 points, 8 a side): it says its name, and the Map is still
/// at Events.
#[test]
fn a_node_at_a_depth_with_no_room_for_its_lines_still_says_its_name() {
    let (_held, s) = depths_app();
    let mut rt = window(s);
    settle(&mut rt);
    assert!(canvas::fly_to_id(&mut rt.state, 0, "N-1104"), "his chat on the Map");
    settle(&mut rt);
    deep_frames(&mut rt);
    press_depth(&mut rt, "Events");
    assert_eq!(lit_depth(&rt).as_deref(), Some("Events"));
    let lines = canvas::lines_key(0, "N-1104", Depth::Events);
    assert!(rt.presented(&lines).is_some(), "its lines drawn at its fit");
    let g = rt.state.map.graph().unwrap().clone();
    let chat = g.find_in(0, "N-1104").expect("his chat laid");
    let x = rt.state.map.shown().into_iter().find(|x| x.i == chat).expect("his chat drawn");
    rt.state.map.cam.at[0] += (x.r[0] + x.r[2] - 190.0) as f64 / rt.state.map.cam.scale;
    deep_frames(&mut rt);
    let x = rt.state.map.shown().into_iter().find(|x| x.i == chat).expect("his chat drawn");
    assert!((x.r[0] + x.r[2] - 190.0).abs() < 1.0 && x.look != Look::Label, "190 points of it in view: {:?} {:?}", x.look, x.r);
    assert!(rt.presented(&lines).is_none(), "no room for its lines");
    assert_eq!(lit_depth(&rt).as_deref(), Some("Events"), "still at Events");
    // what its frame draws (its rect reaches past the canvas, over the sidebar)
    let frame = format!("map-n-{chat}");
    let says: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting && e.path.split('/').any(|k| k == frame)).filter_map(|e| e.text).collect();
    assert!(says.contains(&g.node(chat).label), "its name: {says:?}");
}

/// proto-window N-1446 p1172 point 3: p700 at Events, out to his chat's fit, a click on another post selects it, and the
/// Map is at Structure, as the inspector shows it: the switch lights Structure and the crumbs end at that post.
#[test]
fn a_click_on_another_node_takes_the_map_from_a_depth_to_structure() {
    let (_held, s) = depths_app();
    let mut rt = window(s);
    settle(&mut rt);
    assert!(canvas::fly_to_id(&mut rt.state, 0, "N-1104"), "his chat on the Map");
    settle(&mut rt);
    let g = rt.state.map.graph().unwrap().clone();
    let post = g.find_in(0, "N-1104.p700").expect("his latest post laid");
    let at = bare_for_two(&rt, post).expect("p700's row in view");
    press(&mut rt, at);
    settle(&mut rt);
    press_depth(&mut rt, "Events");
    assert_eq!(lit_depth(&rt).as_deref(), Some("Events"));
    // out to his chat's fit, as his wheel goes: p700 still at Events
    rt.state.map.cam = super::camera::Camera::fitting(&g.node(g.node(post).parent.expect("in his chat")).rect, rt.state.map.size(), 28.0);
    deep_frames(&mut rt);
    assert_eq!(lit_depth(&rt).as_deref(), Some("Events"), "the wheel keeps the depth");
    let other = rt.state.map.shown().iter().map(|x| x.i).find(|i| *i != post && g.node(*i).kind == Kind::Post && bare(&rt, *i).is_some()).expect("another post in view");
    let q = bare(&rt, other).unwrap();
    press(&mut rt, q);
    settle(&mut rt);
    assert_eq!(selected(&rt), Some(g.node(other).id.clone()), "the post clicked");
    assert_eq!(lit_depth(&rt).as_deref(), Some("Structure"), "the Map at Structure");
    let (crumbs, _) = head_says(&rt);
    assert!(crumbs.windows(2).any(|w| w[0] == "›" && w[1] == g.node(other).label) && !crumbs.iter().any(|t| t == "Events"), "{crumbs:?}");
}

/// proto-window N-1446 p1172 point 1 (p1175): his place in p700's events, scrolled down, stays when the Map is laid
/// again (join.rs `keep`) with a line before his chat, which moves p700's index in the graph: its lines are keyed by
/// its name ([`canvas::lines_key`]), not by its index.
#[test]
fn his_place_in_a_cards_lines_stays_when_the_map_is_laid_again() {
    let (_held, s) = depths_app();
    let mut rt = window(s);
    settle(&mut rt);
    assert!(canvas::fly_to_id(&mut rt.state, 0, "N-1104"), "his chat on the Map");
    settle(&mut rt);
    let post = rt.state.map.graph().unwrap().find_in(0, "N-1104.p700").expect("his latest post laid");
    let at = bare_for_two(&rt, post).expect("p700's row in view");
    twice(&mut rt, at);
    settle(&mut rt);
    deep_frames(&mut rt);
    press_depth(&mut rt, "Events");
    let lines = canvas::lines_key(0, "N-1104.p700", Depth::Events);
    // how far down its lines a row lies: the row's top under the lines' top
    let down = |rt: &Runtime<App>| {
        let (l, _) = rt.presented(&lines).expect("its lines drawn");
        let (r, _) = rt.presented("event-201020").expect("its row for agent-10 drawn");
        r[1] - l[1]
    };
    let (l, _) = rt.presented(&lines).expect("its lines drawn");
    let top = down(&rt);
    rt.wheel(l[0] + l[2] / 2.0, l[1] + l[3] / 2.0, -120.0);
    deep_frames(&mut rt);
    let read = down(&rt);
    assert!(read < top - 20.0, "scrolled by his wheel: {top} → {read}");
    // a line before his chat in Insight's rows, a number of its own, as a new chat comes: the Map laid again
    rt.update(|s, _| {
        let free = |s: &App, n: u64| !s.projects[0].live.tickets.contains_key(&n) && s.insights.threads[&0].0.as_ref().is_ok_and(|r| r.iter().all(|x| x.id != format!("N-{n}")));
        let n = (1u64..1104).find(|n| free(s, *n)).expect("a number before his chat's");
        let (rows, at) = s.insights.threads.get_mut(&0).expect("research's lines");
        let rows = rows.as_mut().expect("research's lines read");
        let mut x = rows.iter().find(|x| x.id == "N-1104").expect("his chat's line").clone();
        (x.id, x.label) = (format!("N-{n}"), "a chat before his".into());
        rows.push(x);
        *at += 1;
    });
    deep_frames(&mut rt);
    let moved = rt.state.map.graph().unwrap().find_in(0, "N-1104.p700").expect("p700 laid again");
    assert_ne!(moved, post, "p700's index moved");
    assert_eq!(lit_depth(&rt).as_deref(), Some("Events"), "still at Events");
    assert!((down(&rt) - read).abs() < 0.5, "his place kept: {read} → {}", down(&rt));
}

/// consult-3 N-1446 p1129, through the canvas: inside a post drawn at Events nothing is drawn, so nothing the Map works
/// out from what it draws reaches it. With the camera at its step's call, as near as the test above asks the call's
/// wire at, the call is not drawn and nothing is asked over five frames; at the canvas's middle, over the call, the
/// hover is the post's, a press is the post's and lands in its lines, and the crumbs end with the post and Events.
/// Back at Structure, the call is drawn and asks its wire, once. The pipe is `sh -c cat`, as the test above's: not in
/// the Windows test exe.
#[test]
#[cfg(not(windows))]
fn inside_a_post_drawn_at_a_depth_nothing_is_pressed_hovered_or_asked() {
    let mut s = crate::picture::docket::tests::wire_app();
    // a coordd before the graph's, as `map_app`'s: the Map's own places
    s.projects[0].protocol = coord_types::GRAPH_PROTOCOL - 1;
    let root = format!("project:{}", s.projects[0].id);
    let post = Rect::new(110.0, 120.0, 380.0, 60.0);
    let g = Graph::new(
        vec![
            Node::new(root.clone(), Kind::Project, "research", None, Rect::new(0.0, 0.0, 10_000.0, 8_000.0)),
            Node::new("N-7", Kind::Thread, "N-7", Some(0), Rect::new(100.0, 100.0, 400.0, 600.0)),
            Node::new("N-7.p1", Kind::Post, "p1", Some(1), post),
        ],
        vec![],
    )
    .unwrap();
    s.map.set_graph(g);
    s.map.aim(post.mid(), 4.0);
    s.sel = Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Cloud, at: None });
    // its lines held, made up: the depth asks nothing of its own
    let ev = serde_json::json!({"ev": "post", "at": "2026-11-21T09:00:00Z", "t": 7, "by": "owner", "text": "made up"});
    crate::lens::came(&mut s, 0, "N-7.p1".into(), None, true, serde_json::to_string(&coord_types::RecordLine { line: 9, text: ev.to_string() }).unwrap());
    let out = std::env::temp_dir().join(format!("yaaihh-map-layer-wire-{}.jsonl", std::process::id()));
    let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
    s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
    s.projects[0].down = None;
    let mut rt = window(s);
    deep_frames(&mut rt);
    let call = format!("{}.s1", crate::picture::docket::tests::run_id(1));
    let g = rt.state.map.graph().unwrap().clone();
    let c = g.find_in(0, &call).expect("the post drawn open: its steps and their calls placed");
    canvas::select(&mut rt.state, Some(2));
    canvas::to_depth(&mut rt.state, Depth::Events);
    settle(&mut rt);
    let size = rt.state.map.size();
    rt.state.map.cam = super::camera::Camera::fitting(&g.node(c).rect, size, 28.0);
    let raws = |rt: &Runtime<App>| rt.state.projects[0].pending.values().filter(|w| matches!(w, crate::Pending::Raw { .. })).count();
    deep_frames(&mut rt);
    deep_frames(&mut rt);
    assert_eq!(raws(&rt), 0, "nothing asked of its call over eight frames");
    assert!(rt.presented(&format!("map-n-{c}")).is_none(), "its call not drawn");
    let mid = [size[0] / 2.0, size[1] / 2.0];
    assert!(g.node(c).rect.holds(&Rect::new(rt.state.map.cam.to_world(mid)[0], rt.state.map.cam.to_world(mid)[1], 0.0, 0.0)), "the middle over its call");
    canvas::pointed(&mut rt.state, Some(mid));
    assert_eq!(rt.state.map.hover.as_ref().map(|n| n.1.as_str()), Some("N-7.p1"), "the hover is the post's");
    let shown = rt.state.map.shown();
    assert_eq!(lod::under(&shown, mid).map(|k| shown[k].i), Some(2), "a press is the post's");
    let (lr, _) = rt.presented(&canvas::lines_key(0, "N-7.p1", Depth::Events)).expect("its lines drawn in its card");
    let a = rt.state.map.area;
    assert!(lr[0] <= a[0] + mid[0] && a[0] + mid[0] <= lr[0] + lr[2] && lr[1] <= a[1] + mid[1] && a[1] + mid[1] <= lr[1] + lr[3], "the press lands in its lines: {lr:?}");
    let crumbs = texts_in(&rt, "map-crumbs");
    assert_eq!(crumbs[crumbs.len() - 3..], ["p1", "›", "Events"].map(String::from), "{crumbs:?}");
    // Structure: the call drawn, its wire asked once
    canvas::to_depth(&mut rt.state, Depth::Structure);
    deep_frames(&mut rt);
    deep_frames(&mut rt);
    assert!(rt.presented(&format!("map-n-{c}")).is_some(), "its call drawn");
    assert_eq!(raws(&rt), 1, "its wire asked once");
    rt.state.projects[0].link = None;
    let _ = child.wait();
    let _ = std::fs::remove_file(&out);
}

/// Path 3's record: the window laid from the made-up snapshot (`laid_app`), his chat given the runs docket.rs
/// `wire_app` lists under N-7 (made up: base's `owner_post` ran seven times, one for each of seven posts of his, p688
/// to p700, the last still going; and a stop's run, of no `when`), its first held as a run the Map asked for is held,
/// with its program's three steps (pr's `answer`, the watcher's `next`, a call of coordd's own, and `check`);
/// agent-base's calls from `live_folders`, in the folder returned; and the record's lines of `next`'s first round,
/// made up, held as its wire reads them.
fn path_3_app() -> (std::sync::MutexGuard<'static, ()>, App, std::path::PathBuf) {
    use crate::picture::docket::tests::{live_folders, run_id, wire_app};
    use serde_json::json;
    // wire_app first, as it takes the tests' lock that laid_app holds
    let w = wire_app();
    let (mut ran, runs) = (w.projects[0].live.tickets[&7].ran.clone(), w.tickets.find.fetched[&w.projects[0].id].clone());
    drop(w);
    let (held, mut s) = laid_app();
    let dir = live_folders(&mut s, &format!("map-path-3-{}", KEPT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
    for r in &mut ran {
        r.post = r.post.map(|n| 686 + 2 * n);
    }
    s.projects[0].live.tickets.get_mut(&1104).expect("his chat").ran = ran;
    let pid = s.projects[0].id.clone();
    s.tickets.find.fetched.insert(pid, runs);
    let t = docket_machine::HIDDEN + 1;
    let line = |line: u64, v: serde_json::Value| coord_types::RecordLine { line, text: v.to_string() };
    let lines = vec![line(501, json!({"ev": "step", "t": t, "step": 2, "call": "next", "worker": "watcher"})), line(502, json!({"ev": "answer", "t": t, "by": "watcher", "head": {"head": "keep"}}))];
    s.records.got.insert((0, format!("{}.s2", run_id(1))), Ok(lines));
    (held, s, dir)
}

/// p715 path 3, "Why is Codex burning?", from the handler node (proto-lead N-1479 p21: the first press, from the
/// account to coordd's own calls, waits on the own-step arm), on its record ([`path_3_app`]) as the layout lays it, by
/// presses at the points drawn.
/// - Programs from the explorer; a double-click on base's `owner_post` handler flies to it: its runs, each a row saying
///   what it came to, oldest first, the handler how many and what most came to.
/// - A double-click on its first run flies to its card, where its stages lie in the band, as a post's do (level 5).
/// - A double-click on its `next` step, coordd's own, flies into it: its stage, and its rounds as calls (level 6).
/// - A double-click on its first round flies into it: the wire, the record's lines of that round (level 7).
///
/// At each double-click: what is selected, the inspector, the crumb trail. Not in the Windows test exe, as path 5's:
/// the calls are read from this machine's files.
#[test]
#[cfg(not(windows))]
fn path_3_from_the_handler_to_its_runs_a_runs_steps_and_a_call_on_the_wire() {
    use crate::picture::docket::tests::run_id;
    let (_held, s, dir) = path_3_app();
    let mut rt = window(s);
    settle(&mut rt);
    let crumbs = |g: &Graph, i: u32| -> Vec<String> { std::iter::once("All".to_string()).chain(g.path(i).iter().flat_map(|p| ["›".to_string(), g.node(*p).label.clone()])).collect() };
    let opened = |rt: &mut Runtime<App>, i: u32, id: &str| {
        let at = bare_for_two(rt, i).unwrap_or_else(|| panic!("{id} in view, under nothing else"));
        twice(rt, at);
        assert_eq!(selected(rt).as_deref(), Some(id));
        settle(rt);
        deep_frames(rt);
    };
    // the handler, in Programs
    assert!(canvas::fly_to_id(&mut rt.state, 0, super::layout::PROGRAMS), "Programs on the Map");
    settle(&mut rt);
    let g = rt.state.map.graph().unwrap().clone();
    let handler = g.find_in(0, "handler:base:owner_post").expect("base's owner_post handler laid");
    assert_eq!(g.node(g.node(handler).parent.unwrap()).label, "Programs");
    opened(&mut rt, handler, "handler:base:owner_post");
    let g = rt.state.map.graph().unwrap().clone();
    assert_eq!(texts_in(&rt, "map-crumbs"), crumbs(&g, handler));
    let ins = texts_in(&rt, "map-inspector");
    assert!(ins.first().is_some_and(|t| t == "handler") && ins.iter().any(|t| t == "base · when your words land"), "{ins:?}");
    assert!(ins.iter().any(|t| t.starts_with("ran 7 · last ") && t.ends_with(" · most To([pr]) (6)")), "how many, and what most came to: {ins:?}");
    let runs: Vec<u32> = g.children(handler).to_vec();
    assert_eq!(runs.iter().map(|i| g.node(*i).id.clone()).collect::<Vec<_>>(), (1..=7).map(|i| format!("run:{}", run_id(i))).collect::<Vec<_>>(), "its runs, oldest first; the stop's is no handler's");
    assert_eq!((g.node(runs[0]).label.as_str(), g.node(runs[6]).label.as_str()), ("To([pr])", "running"));
    // its first run: its card, its stages in the band
    let run = runs[0];
    opened(&mut rt, run, &format!("run:{}", run_id(1)));
    let key = format!("{}.1", run_id(1));
    let sid = format!("run:{}/{}", run_id(1), super::deep::step_id(&key));
    let g = rt.state.map.graph().unwrap().clone();
    let step = g.find_in(0, &sid).expect("its stages placed once its card is open");
    assert_eq!(g.node(step).parent, Some(run));
    assert!(super::deep::card(g.node(run).rect).holds(&g.node(step).rect), "in its card");
    assert!(g.find_in(0, &super::deep::step_id(&key)).is_none(), "its own: the post it ran for is not open");
    assert!(rt.find(&format!("map-card-note-{run}")).is_none(), "held: its band says nothing of reading it");
    assert_eq!(texts_in(&rt, "map-crumbs"), crumbs(&g, run));
    let ins = texts_in(&rt, "map-inspector");
    assert!(ins.first().is_some_and(|t| t == "run") && ins.iter().any(|t| t == "To([pr])"), "{ins:?}");
    // its `next` step, coordd's own: its stage, and its two rounds as calls
    opened(&mut rt, step, &sid);
    let x = rt.state.map.shown().into_iter().find(|x| x.i == step).expect("the step drawn");
    assert!(x.r[2] >= super::deep::STEP_PX, "wide enough to draw its stage: {:?}", x.r);
    assert!(rt.find(&format!("map-step-{key}")).is_some(), "its stage drawn");
    let g = rt.state.map.graph().unwrap().clone();
    assert_eq!(texts_in(&rt, "map-crumbs"), crumbs(&g, step));
    assert!(texts_in(&rt, "map-inspector").first().is_some_and(|t| t == "step"));
    let cid = format!("run:{}/{}.s2", run_id(1), run_id(1));
    let call = g.find_in(0, &cid).expect("its first round, a call");
    assert_eq!(g.children(step).len(), 2, "a call for each round");
    // its first round: the wire, the record's lines of it
    opened(&mut rt, call, &cid);
    let wire = texts_in(&rt, &format!("map-call-{cid}"));
    assert!(wire.iter().any(|t| t == "Step 2: next, by watcher"), "the round's lines on the wire: {wire:?}");
    let g = rt.state.map.graph().unwrap().clone();
    assert_eq!(texts_in(&rt, "map-crumbs"), crumbs(&g, call));
    assert!(texts_in(&rt, "map-inspector").first().is_some_and(|t| t == "call"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The brain's record (paper-research N-1446 p1108, part 6): the window laid from the made-up snapshot (`laid_app`),
/// with deep.rs's `brain_passes` (made up: pr's pass that spoke, w2's that ended Quiet, pr's second still at work)
/// standing at his latest post, p700, as his chat's line in the outline has them, each pass in the snapshot; and the
/// record's lines of the round of pr's `attend`, made up, held as its wire reads them. The passes' ids, and that
/// round's node (`{pass}.s{step}`).
fn brain_app() -> (std::sync::MutexGuard<'static, ()>, App, Vec<String>, String) {
    use serde_json::json;
    let (line, passes) = super::deep::tests::brain_passes();
    let (held, mut s) = laid_app();
    let p = &mut s.projects[0];
    let chat = p.live.tickets.get_mut(&1104).expect("his chat");
    let children = line.children.iter().map(|c| docket_machine::ViewOutline { at_post: Some(700), ..c.clone() }).collect();
    chat.outline = Some(docket_machine::ViewOutline { id: chat.id.clone(), children, ..line });
    let ids: Vec<String> = passes.iter().map(|v| v.id.clone()).collect();
    let pr = &passes[0];
    let attend = crate::picture::reached(pr, &crate::picture::Labels::default()).into_iter().find(|r| r.call.as_deref() == Some("attend")).expect("pr's attend reached");
    let (t, step) = (pr.n, attend.rounds[0]);
    let wire = format!("{}.s{step}", pr.id);
    let at = |line: u64, v: serde_json::Value| coord_types::RecordLine { line, text: v.to_string() };
    let lines = vec![at(601, json!({"ev": "step", "t": t, "step": step, "call": "attend", "worker": "watcher"})), at(602, json!({"ev": "answer", "t": t, "by": "watcher", "head": {"head": "Read", "topic": "the counter"}}))];
    for v in passes {
        assert!(p.live.tickets.insert(v.n, v).is_none(), "a number of its own");
    }
    s.records.got.insert((0, wire.clone()), Ok(lines));
    (held, s, ids, wire)
}

/// The brain under the mark (paper-research N-1446 p1108, part 6; docket-edge6 p1125), on its record ([`brain_app`]) as
/// the layout lays it, by presses at the points drawn.
/// - His chat from the explorer; a double-click on p700's row flies to its card: each pass standing at it is a mark at
///   the right of its words, oldest first, and the post's words keep left of them.
/// - A double-click on pr's mark, the pass that spoke, flies to it: it says its agent and what its pass came to, and
///   its parts lie in its band, attend then elect; the inspector opens its pass's page.
/// - A double-click on its `attend` flies into it: its stage, and its round as a call.
/// - A double-click on that round flies into it: the wire, the record's lines of it.
///
/// At each double-click: what is selected, the inspector, the crumb trail.
#[test]
fn the_brain_from_his_post_to_a_passs_mark_its_parts_and_a_call_on_the_wire() {
    let (_held, s, ids, wire) = brain_app();
    let mut rt = window(s);
    settle(&mut rt);
    let crumbs = |g: &Graph, i: u32| -> Vec<String> { std::iter::once("All".to_string()).chain(g.path(i).iter().flat_map(|p| ["›".to_string(), g.node(*p).label.clone()])).collect() };
    let opened = |rt: &mut Runtime<App>, i: u32, id: &str| {
        let at = bare_for_two(rt, i).unwrap_or_else(|| panic!("{id} in view, under nothing else"));
        twice(rt, at);
        assert_eq!(selected(rt).as_deref(), Some(id));
        settle(rt);
        deep_frames(rt);
    };
    // his latest post: its card, the passes' marks at the right of its words
    assert!(canvas::fly_to_id(&mut rt.state, 0, "N-1104"), "his chat on the Map");
    settle(&mut rt);
    let post = rt.state.map.graph().unwrap().find_in(0, "N-1104.p700").expect("his latest post laid");
    opened(&mut rt, post, "N-1104.p700");
    let g = rt.state.map.graph().unwrap().clone();
    let marks: Vec<u32> = g.children(post).iter().copied().filter(|i| g.node(*i).kind == Kind::Brain).collect();
    assert_eq!(marks.iter().map(|i| g.node(*i).id.clone()).collect::<Vec<_>>(), ids.iter().map(|i| format!("brain:{i}")).collect::<Vec<_>>(), "a mark for each pass, oldest first, once its card is open");
    let room = super::deep::marks_room(super::deep::card(g.node(post).rect));
    let (words, _) = rt.presented(&format!("map-card-words-{post}")).expect("its words drawn");
    for m in &marks {
        assert!(room.holds(&g.node(*m).rect), "{} in the marks' room", g.node(*m).id);
        let (r, _) = rt.presented(&format!("map-n-{m}")).unwrap_or_else(|| panic!("mark {m} drawn"));
        assert!(words[0] + words[2] <= r[0] + 0.5, "the post's words {words:?} left of mark {m} {r:?}");
    }
    assert_eq!(texts_in(&rt, "map-crumbs"), crumbs(&g, post));
    // pr's mark, the pass that spoke: its parts in its band; its page from the inspector
    let pr = marks[0];
    opened(&mut rt, pr, &format!("brain:{}", ids[0]));
    let g = rt.state.map.graph().unwrap().clone();
    let says = texts_in(&rt, &format!("map-n-{pr}"));
    assert!(["pr", "Said"].iter().all(|w| says.iter().any(|t| t == w)), "its agent, and what its pass came to: {says:?}");
    assert_eq!(texts_in(&rt, "map-crumbs"), crumbs(&g, pr));
    let ins = texts_in(&rt, "map-inspector");
    assert!(ins.first().is_some_and(|t| t == "brain") && ins.contains(&"Open its page".to_string()), "{ins:?}");
    let parts: Vec<u32> = g.children(pr).iter().copied().filter(|i| g.node(*i).kind == Kind::Step).collect();
    let named: Vec<&str> = parts.iter().map(|i| g.node(*i).label.as_str()).collect();
    assert_eq!(named.get(..2), Some(&["attend", "elect"][..]), "attend, then elect: {named:?}");
    let (attend, sid) = (parts[0], g.node(parts[0]).id.clone());
    let key = super::deep::stage_of(&sid).expect("a stage").to_string();
    assert!(sid.starts_with(&format!("brain:{}/", ids[0])) && super::deep::card(g.node(pr).rect).holds(&g.node(attend).rect), "in its mark's card, carrying it: {sid}");
    // its attend: its stage, and its round as a call
    opened(&mut rt, attend, &sid);
    let x = rt.state.map.shown().into_iter().find(|x| x.i == attend).expect("the part drawn");
    assert!(x.r[2] >= super::deep::STEP_PX, "wide enough to draw its stage: {:?}", x.r);
    assert!(rt.find(&format!("map-step-{key}")).is_some(), "its stage drawn, from the pass the snapshot holds");
    let g = rt.state.map.graph().unwrap().clone();
    assert_eq!(texts_in(&rt, "map-crumbs"), crumbs(&g, attend));
    assert!(texts_in(&rt, "map-inspector").first().is_some_and(|t| t == "step"));
    let cid = format!("brain:{}/{wire}", ids[0]);
    let call = g.find_in(0, &cid).unwrap_or_else(|| panic!("its round, a call: {cid}"));
    assert_eq!(g.children(attend), [call], "one round");
    // that round: the wire, the record's lines of it
    opened(&mut rt, call, &cid);
    let step = wire.rsplit_once(".s").unwrap().1;
    let lines = texts_in(&rt, &format!("map-call-{cid}"));
    assert!(lines.iter().any(|t| *t == format!("Step {step}: attend, by watcher")), "the round's lines on the wire: {lines:?}");
    let g = rt.state.map.graph().unwrap().clone();
    assert_eq!(texts_in(&rt, "map-crumbs"), crumbs(&g, call));
    assert!(texts_in(&rt, "map-inspector").first().is_some_and(|t| t == "call"));
}

/// The brain's marks at his post's fit (paper-research N-1479 p323; proto-window N-1446 p1159 small 1, consult-3 p1160
/// point 2), on its record ([`brain_app`]) with the lines of pr's pass held, made up:
/// - p700 double-clicked: each mark, drawn at Label there, says on itself whose it is and what its pass came to.
/// - pr's mark pressed, then Events: its pass's events in its card (canvas.rs `record_of`, its pass's ticket).
#[test]
fn at_his_posts_fit_each_mark_says_whose_it_is_and_what_its_pass_came_to_and_its_events_are_its_passs() {
    use serde_json::json;
    let (_held, mut s, ids, wire) = brain_app();
    let step: u64 = wire.rsplit_once(".s").unwrap().1.parse().unwrap();
    let t = s.projects[0].live.tickets.values().find(|v| v.id == ids[0]).expect("pr's pass").n;
    let lines = [(601u64, json!({"ev": "step", "t": t, "step": step, "call": "attend", "worker": "watcher"})), (602, json!({"ev": "answer", "t": t, "by": "watcher", "head": {"head": "Read", "topic": "the counter"}}))]
        .map(|(line, v)| serde_json::to_string(&coord_types::RecordLine { line, text: v.to_string() }).unwrap())
        .join("\n");
    crate::lens::came(&mut s, 0, ids[0].clone(), None, true, lines);
    let mut rt = window(s);
    settle(&mut rt);
    assert!(canvas::fly_to_id(&mut rt.state, 0, "N-1104"), "his chat on the Map");
    settle(&mut rt);
    let post = rt.state.map.graph().unwrap().find_in(0, "N-1104.p700").expect("his latest post laid");
    let at = bare_for_two(&rt, post).expect("p700's row in view");
    twice(&mut rt, at);
    settle(&mut rt);
    deep_frames(&mut rt);
    let g = rt.state.map.graph().unwrap().clone();
    let marks: Vec<u32> = g.children(post).iter().copied().filter(|i| g.node(*i).kind == Kind::Brain).collect();
    assert_eq!(marks.len(), ids.len(), "a mark for each pass");
    let (shown, els) = (rt.state.map.shown(), rt.elements());
    let mut said = Vec::new();
    for m in &marks {
        let x = shown.iter().find(|x| x.i == *m).expect("the mark drawn");
        assert_eq!(x.look, Look::Label, "at his post's fit a mark is drawn at Label: {:?}", x.r);
        let n = g.node(*m);
        let words = format!("{} · {}", n.label, n.note);
        let (r, _) = rt.presented(&format!("map-n-{m}")).expect("its frame");
        let e = els.iter().find(|e| !e.exiting && e.text.as_deref() == Some(words.as_str())).unwrap_or_else(|| panic!("{words:?} drawn"));
        assert!(e.rect[0] >= r[0] && e.rect[0] + e.rect[2] <= r[0] + r[2] + 0.5 && e.rect[1] >= r[1] && e.rect[1] + e.rect[3] <= r[1] + r[3] + 0.5, "{words:?} {:?} on its mark {r:?}", e.rect);
        said.push(words);
    }
    assert_eq!(said, ["pr · Said", "w2 · Quiet", "pr · at work"], "whose, and what each came to");
    // pr's mark, then Events: its pass's lines
    let pr = marks[0];
    let q = bare_for_two(&rt, pr).expect("pr's mark in view");
    press(&mut rt, q);
    settle(&mut rt);
    assert_eq!(selected(&rt), Some(format!("brain:{}", ids[0])), "pr's mark");
    assert!(depth_segment(&rt, "Events").1, "Events on at a mark");
    press_depth(&mut rt, "Events");
    assert_eq!(lit_depth(&rt).as_deref(), Some("Events"));
    let rows = texts_in(&rt, &canvas::lines_key(0, &format!("brain:{}", ids[0]), Depth::Events));
    assert!(rows.iter().any(|t| *t == format!("step {step}: attend, by watcher")), "its pass's events: {rows:?}");
}

/// Path 5's record as his window (Windows) holds it: its calls served by research's coordd (edit.rs
/// `Asked::Prompts`, what a step drawing its prompt asks for there), made up from agent-base's in `live_folders`.
/// EDITING: the edit of `answer` started as well, as the answer to Edit starts it there (`Asked::Sources`), in p700's
/// first step.
fn path_5_served(editing: bool) -> (std::sync::MutexGuard<'static, ()>, App) {
    let (held, mut s, dir) = path_5_app();
    let call = std::fs::read_to_string(dir.join("base/calls/question.call")).unwrap();
    let served = serde_json::json!({"kinds": [], "calls": [{"path": "/base/calls/question.call", "text": call}]}).to_string();
    crate::edit::came(&mut s, 0, crate::edit::Asked::Prompts, true, served.clone());
    if editing {
        crate::edit::came(&mut s, 0, crate::edit::Asked::Sources { kind: false, name: "answer".into() }, true, served);
        s.deep.editing = Some((0, super::deep::step_id(&path_5_stage())));
    }
    (held, s)
}

fn path_5_shown() -> App {
    path_5_served(false).1
}

fn path_5_editing() -> App {
    path_5_served(true).1
}

/// Path 5's stills, from his window's build (Windows), on its record with the calls served ([`path_5_served`]), at
/// research's size, 1400 by 900, in dark (his look): after `map_laid_stills`'s double-clicks into research, his chat's
/// topic, his chat and his latest post, the post's card with its steps in the band; then its first step
/// double-clicked: its stage and template; then the same with the edit of its call under way there, as the answer to
/// Edit opens it (the press, and Save, are the test's above). The points pressed are read from the same window run
/// headless first.
#[test]
#[ignore]
fn map_path_5_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let (key, at) = (path_5_stage(), |rt: &Runtime<App>, id: &str| rt.state.map.graph().unwrap().find_in(0, id).unwrap_or_else(|| panic!("{id} on the Map")));
    let sid = super::deep::step_id(&key);
    let points = {
        let (_held, s) = path_5_served(false);
        let mut rt = window(s);
        let mut points = Vec::new();
        let mut into = |rt: &mut Runtime<App>, i: u32, what: &str| {
            let q = bare_for_two(rt, i).unwrap_or_else(|| panic!("{what} in view"));
            twice(rt, q);
            settle(rt);
            deep_frames(rt);
            points.push(q);
        };
        into(&mut rt, 0, "research");
        let chat = at(&rt, "N-1104");
        let topic = rt.state.map.graph().unwrap().node(chat).parent.unwrap();
        into(&mut rt, topic, "his chat's topic");
        into(&mut rt, chat, "his chat");
        let post = at(&rt, "N-1104.p700");
        into(&mut rt, post, "his latest post");
        let step = at(&rt, &sid);
        into(&mut rt, step, "its first step");
        assert_eq!(selected(&rt), Some(sid.clone()));
        let here = texts_in(&rt, &format!("map-step-{key}"));
        assert!(here.iter().any(|t| t.contains("A question for you:")) && here.iter().any(|t| t == "Edit the prompt"), "its template, served: {here:?}");
        points
    };
    let twice = |t: f32, q: [f32; 2]| format!(r#"{{"t": {t}, "type": "double-click", "x": {}, "y": {}}}"#, q[0], q[1]);
    let to_post: Vec<String> = points[..4].iter().zip([0.3, 1.8, 3.4, 5.0]).map(|(q, t)| twice(t, *q)).collect();
    let to_step = format!("{}, {}", to_post.join(", "), twice(6.6, points[4]));
    let stills: [(&str, String, &str, fn() -> App); 3] = [
        ("path-5-1-card", format!("[{}]", to_post.join(", ")), "6.6", path_5_shown),
        ("path-5-2-step", format!("[{to_step}]"), "8.4", path_5_shown),
        ("path-5-3-edit", format!("[{to_step}]"), "8.4", path_5_editing),
    ];
    for (name, script, at, init) in stills {
        let f = out.join(format!("{name}.json"));
        std::fs::write(&f, script).unwrap();
        let args = vec!["--snap".to_string(), out.join(format!("{name}.png")).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), at.into(), "--script".into(), f.display().to_string()];
        let spec = rune::AppSpec { poll: Some(map_poll), on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) };
        rune::App::new(spec).args(args).try_run().unwrap();
    }
}

/// Path 3's record (as on Linux: his window draws the same from its served answers) for a still, with no lock held.
fn path_3_shown() -> App {
    path_3_app().1
}

/// Path 3's record with its two oldest runs under N-3000 (a made-up thread of the snapshot), each come to Nothing, for
/// proto-lead's still of the rows (N-1479 p209); GONE: N-3000 left the snapshot, as a ticket does a day after it closed.
fn path_3_split(gone: bool) -> App {
    let (_held, mut s, _) = path_3_app();
    let p = &mut s.projects[0];
    let chat = p.live.tickets.get_mut(&1104).expect("his chat");
    let mut oldest: Vec<docket_machine::ViewRan> = chat.ran.drain(..2).collect();
    for r in &mut oldest {
        r.post = None;
        r.said = Some("Nothing".into());
    }
    let other = p.live.tickets.values_mut().find(|t| t.id == "N-3000").expect("N-3000 in the snapshot");
    other.ran = oldest;
    if gone {
        p.live.tickets.retain(|_, t| t.id != "N-3000");
    }
    s
}
fn path_3_before() -> App {
    path_3_split(false)
}
fn path_3_after() -> App {
    path_3_split(true)
}

/// Path 3's stills, from his window's build (Windows), on its record ([`path_3_app`]), at research's size, 1400 by
/// 900, in dark (his look): research, its Programs and base's `owner_post` handler double-clicked, its runs in its
/// rows (path-3-1-handler); its first run double-clicked, its card, its stages in the band (path-3-2-run); its `next`
/// step and that step's first round double-clicked, the wire (path-3-3-wire). And proto-lead's (N-1479 p209), the
/// handler's rows with its two oldest runs under N-3000 (path-3-4-rows-before), and once N-3000 has left the snapshot
/// (path-3-5-rows-after): the later runs two rows up. The points pressed are read from the same window run headless
/// first.
#[test]
#[ignore]
fn map_path_3_stills() {
    use crate::picture::docket::tests::run_id;
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let at = |rt: &Runtime<App>, id: &str| rt.state.map.graph().unwrap().find_in(0, id).unwrap_or_else(|| panic!("{id} on the Map"));
    let (run, step, call) = (format!("run:{}", run_id(1)), format!("run:{}/step:{}.1", run_id(1), run_id(1)), format!("run:{}/{}.s2", run_id(1), run_id(1)));
    let points = {
        let (_held, s, _) = path_3_app();
        let mut rt = window(s);
        let mut points = Vec::new();
        // from research's press on, the inspector is open and the canvas keeps its width: a node in the strip it opens
        // over is pressed where it is drawn (Programs, a point at research's top right, the room free when it came)
        let mut into = |rt: &mut Runtime<App>, i: u32, what: &str| {
            let q = if selected(rt).is_some() { bare_within(rt, i, rt.state.map.size()[0]) } else { bare_for_two(rt, i) };
            let q = q.unwrap_or_else(|| panic!("{what} in view"));
            twice(rt, q);
            settle(rt);
            deep_frames(rt);
            points.push(q);
        };
        into(&mut rt, 0, "research");
        let programs = at(&rt, super::layout::PROGRAMS);
        into(&mut rt, programs, "Programs");
        let handler = at(&rt, "handler:base:owner_post");
        into(&mut rt, handler, "base's owner_post handler");
        let i = at(&rt, &run);
        into(&mut rt, i, "its first run");
        let i = at(&rt, &step);
        into(&mut rt, i, "its next step");
        let i = at(&rt, &call);
        into(&mut rt, i, "its first round");
        assert_eq!(selected(&rt), Some(call.clone()));
        let wire = texts_in(&rt, &format!("map-call-{call}"));
        assert!(wire.iter().any(|t| t == "Step 2: next, by watcher"), "its wire: {wire:?}");
        points
    };
    let twice = |t: f32, q: [f32; 2]| format!(r#"{{"t": {t}, "type": "double-click", "x": {}, "y": {}}}"#, q[0], q[1]);
    let script = |n: usize| format!("[{}]", points[..n].iter().zip([0.3, 1.8, 3.4, 5.0, 6.6, 8.2]).map(|(q, t)| twice(t, *q)).collect::<Vec<_>>().join(", "));
    let stills: [(&str, String, &str, fn() -> App); 5] = [
        ("path-3-1-handler", script(3), "5.0", path_3_shown),
        ("path-3-2-run", script(4), "6.6", path_3_shown),
        ("path-3-3-wire", script(6), "9.8", path_3_shown),
        ("path-3-4-rows-before", script(3), "5.0", path_3_before),
        ("path-3-5-rows-after", script(3), "5.0", path_3_after),
    ];
    for (name, script, at, init) in stills {
        let f = out.join(format!("{name}.json"));
        std::fs::write(&f, script).unwrap();
        let args = vec!["--snap".to_string(), out.join(format!("{name}.png")).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), at.into(), "--script".into(), f.display().to_string()];
        let spec = rune::AppSpec { poll: Some(map_poll), on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) };
        rune::App::new(spec).args(args).try_run().unwrap();
    }
}

/// The brain's record ([`brain_app`]) for a still, with no lock held.
fn brain_shown() -> App {
    brain_app().1
}

/// The brain's stills (paper-research N-1446 p1108, part 6), from his window's build (Windows), on its made-up record
/// ([`brain_app`]), at research's size, 1400 by 900, in dark (his look): as path 5's, research, his chat's topic, his
/// chat and p700 double-clicked, its card with the three passes' marks at the right of its words (brain-1-post); pr's mark, the pass that spoke, double-clicked: it
/// says pr and Said, its parts attend and elect in its band (brain-2-mark); its attend double-clicked, its stage
/// (brain-3-part); that part's round double-clicked, the wire (brain-4-wire); and from p700's card, w2's mark, the pass
/// that ended Quiet with its attend alone (brain-5-quiet). The points pressed are read from the same window run
/// headless first.
#[test]
#[ignore]
fn map_brain_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let at = |rt: &Runtime<App>, id: &str| rt.state.map.graph().unwrap().find_in(0, id).unwrap_or_else(|| panic!("{id} on the Map"));
    let into = |rt: &mut Runtime<App>, i: u32, what: &str, points: &mut Vec<[f32; 2]>| {
        let q = bare_for_two(rt, i).unwrap_or_else(|| panic!("{what} in view"));
        twice(rt, q);
        settle(rt);
        deep_frames(rt);
        points.push(q);
    };
    // research, his chat's topic, his chat and p700, as path 5's stills go; then pr's mark, its attend and that round;
    // w2's mark's point read at p700's fit
    let (points, quiet) = {
        let (_held, s, ids, wire) = brain_app();
        let mut rt = window(s);
        let mut points = Vec::new();
        into(&mut rt, 0, "research", &mut points);
        let chat = at(&rt, "N-1104");
        let topic = rt.state.map.graph().unwrap().node(chat).parent.unwrap();
        into(&mut rt, topic, "his chat's topic", &mut points);
        into(&mut rt, chat, "his chat", &mut points);
        let post = at(&rt, "N-1104.p700");
        into(&mut rt, post, "his p700", &mut points);
        // p700 selected, the inspector is open and the canvas narrowed already: a click on w2's mark selects it and the
        // canvas keeps its width, so any point of it in view takes both clicks
        let quiet = bare(&rt, at(&rt, &format!("brain:{}", ids[1]))).expect("w2's mark in view");
        let mark = format!("brain:{}", ids[0]);
        let m = at(&rt, &mark);
        into(&mut rt, m, "pr's mark", &mut points);
        let part = rt.state.map.graph().unwrap().children(m).first().copied().expect("its first part");
        into(&mut rt, part, "its attend", &mut points);
        let call = format!("{mark}/{wire}");
        let c = at(&rt, &call);
        into(&mut rt, c, "its round", &mut points);
        assert_eq!(selected(&rt), Some(call));
        (points, quiet)
    };
    let twice = |t: f32, q: [f32; 2]| format!(r#"{{"t": {t}, "type": "double-click", "x": {}, "y": {}}}"#, q[0], q[1]);
    let script = |points: &[[f32; 2]]| format!("[{}]", points.iter().zip([0.3, 1.8, 3.4, 5.0, 6.6, 8.2, 9.8]).map(|(q, t)| twice(t, *q)).collect::<Vec<_>>().join(", "));
    let to_quiet: Vec<[f32; 2]> = points[..4].iter().copied().chain([quiet]).collect();
    let stills: [(&str, String, &str); 5] = [
        ("brain-1-post", script(&points[..4]), "6.6"),
        ("brain-2-mark", script(&points[..5]), "8.2"),
        ("brain-3-part", script(&points[..6]), "9.8"),
        ("brain-4-wire", script(&points[..7]), "11.4"),
        ("brain-5-quiet", script(&to_quiet), "8.2"),
    ];
    for (name, script, at) in stills {
        let f = out.join(format!("{name}.json"));
        std::fs::write(&f, script).unwrap();
        let args = vec!["--snap".to_string(), out.join(format!("{name}.png")).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), at.into(), "--script".into(), f.display().to_string()];
        let spec = rune::AppSpec { poll: Some(map_poll), on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), brain_shown, crate::view) };
        rune::App::new(spec).args(args).try_run().unwrap();
    }
}

/// The brain's satellites on the cloud (the owner, N-1791 p1, 3; brain.rs): research's cloud from cloud.rs's made-up
/// answer, its coordd serving the graph, with deep.rs's made-up passes (pr's that spoke, w2's that ended Quiet, pr's
/// second at work) and pr and w2 among its agents, each brain bound and on (brain.rs `tests::with_passes`). The passes.
pub(super) fn satellite_app() -> (std::sync::MutexGuard<'static, ()>, App, Vec<coord_types::TicketView>) {
    let (held, mut s) = laid_app();
    s.projects[0].protocol = coord_types::GRAPH_PAGE_PROTOCOL;
    if s.sidebar.is_open() {
        s.sidebar.toggle();
    }
    let passes = super::brain::tests::with_passes(&mut s);
    s.cloud.of = 0;
    s.cloud.kept_read = Some(Err("a test's places are not kept".into()));
    s.cloud.base = Some(Ok(super::cloud::tests::base()));
    (held, s, passes)
}

/// The brain's satellites at the cloud's whole, its overview: beside pr's node, its name, what its brain does now, the
/// step its pass at work waits on, and how many passes it has had; beside w2's, what its one pass came to. A press on
/// pr's selects that pass, and the inspector opens its page. The Map's bar has one Brain group with a switch for each,
/// named by the agent, on the bar's first line beside "Whole map", its last pass in its help and not on the bar. A pass
/// of w2's that comes in the snapshot then, at work, is what its satellite says at the next frame.
#[test]
fn each_bound_brain_has_a_satellite_at_the_clouds_whole_and_its_press_selects_its_pass() {
    let (_held, s, passes) = satellite_app();
    let mut rt = window(s);
    settle(&mut rt);
    rt.frame(0.016);
    let step = passes[2].waits_on.first().map(|w| w.call.clone()).expect("pr's second pass waits on a step");
    assert_eq!(texts_in(&rt, "map-brain-agent:pr"), ["pr".to_string(), format!("brain: {step}…"), "◷ 2".to_string()]);
    assert_eq!(texts_in(&rt, "map-brain-agent:w2"), ["w2", "brain: quiet"]);
    // at the whole: the agents' nodes are smaller on screen than their satellites, which read
    let g = rt.state.map.graph().unwrap().clone();
    let a = g.find_in(0, "agent:pr").expect("pr's node");
    let side = rt.state.map.cam.on_screen(&g.node(a).rect)[2];
    let (pill, _) = rt.presented("map-brain-agent:pr").expect("pr's satellite drawn");
    assert!(side < pill[3], "pr's node {side} points across, its satellite {} high", pill[3]);
    assert!(texts_in(&rt, "map-brain-switch-pr").contains(&"pr".to_string()) && texts_in(&rt, "map-brain-switch-w2").contains(&"w2".to_string()));
    assert_eq!(texts_in(&rt, "map-brain-switches").first().map(String::as_str), Some("Brain"));
    let (group, home) = (rt.presented("map-brain-switches").expect("the Brain group drawn").0, rt.presented("cloud-home").expect("Whole map drawn").0);
    assert!((group[1] + group[3] / 2.0 - home[1] - home[3] / 2.0).abs() < 4.0, "the Brain group {group:?} on Whole map's line {home:?}");
    assert!(!texts_in(&rt, "cloud-bar").iter().any(|t| t.starts_with("last pass")), "{:?}", texts_in(&rt, "cloud-bar"));
    press(&mut rt, [pill[0] + pill[2] / 2.0, pill[1] + pill[3] / 2.0]);
    assert_eq!(selected(&rt), Some(format!("brain:{}", passes[2].id)));
    assert!(says(&rt, "Open its page").is_some(), "the inspector opens the pass");
    // a pass of w2's comes, at work, as a snapshot's ticket event brings it (live.rs `apply`)
    let mut v = passes[2].clone();
    v.n = passes.iter().map(|p| p.n).max().unwrap() + 1;
    v.id = format!("B-{}", v.n);
    v.form["agent"] = serde_json::json!("w2");
    let p = &mut rt.state.projects[0];
    p.live.tickets.insert(v.n, v);
    p.live.tickets_rev += 1;
    rt.frame(0.016);
    rt.frame(0.016);
    assert_eq!(texts_in(&rt, "map-brain-agent:w2"), ["w2".to_string(), format!("brain: {step}…"), "◷ 2".to_string()]);
}

/// The owner's N-1791 p1, 2, through the window: at the cloud's whole his chat's card carries "◷ N", N the steps of its
/// history (ribbon.rs `count`); a press on the chip unfolds the rail under the card, its steps oldest first, the last
/// marked current, and no card moves; a press on a step selects the node it names; the chip again folds it, U on the
/// card selected unfolds it, Esc folds it. pr's satellite's chip unfolds pr's passes the same way. At a past moment
/// the chip counts the steps by then.
#[test]
fn a_cards_chip_unfolds_its_history_on_a_rail_that_moves_no_card_and_its_steps_select() {
    let (_held, s, _) = satellite_app();
    let mut rt = window(s);
    settle(&mut rt);
    rt.frame(0.016);
    let g = rt.state.map.graph().unwrap().clone();
    let chat = g.find_in(0, "N-1").expect("his chat");
    let n = super::ribbon::count(&g, chat, None);
    assert!(n >= 3, "his chat has {n} steps");
    assert_eq!(texts_in(&rt, "map-chip-N-1"), [format!("◷ {n}")]);
    // no card's words run under its chip (the owner, N-1791 p11, 1: half over the card's edge it covered the end of
    // their first line): every text that meets a chip is the chip's own
    let meets = |a: [f32; 4], b: [f32; 4]| a[0] < b[0] + b[2] - 0.5 && b[0] < a[0] + a[2] - 0.5 && a[1] < b[1] + b[3] - 0.5 && b[1] < a[1] + a[3] - 0.5;
    let mut chips = 0;
    for x in rt.state.map.shown() {
        let Some((c, _)) = rt.presented(&format!("map-chip-{}", g.node(x.i).id)) else { continue };
        chips += 1;
        for e in rt.elements().into_iter().filter(|e| e.text.is_some() && meets(e.rect, c)) {
            let inside = e.rect[0] >= c[0] - 0.5 && e.rect[0] + e.rect[2] <= c[0] + c[2] + 0.5;
            assert!(inside && e.text.as_deref().is_some_and(|t| t.starts_with('◷')), "{:?} runs under {}'s chip", e.text, g.node(x.i).id);
        }
    }
    assert!(chips >= 2, "{chips} chips drawn");
    let before: Vec<(u32, [f32; 4])> = rt.state.map.shown().iter().map(|x| (x.i, x.r)).collect();
    let (c, _) = rt.presented("map-chip-N-1").expect("the chip drawn");
    press(&mut rt, [c[0] + c[2] / 2.0, c[1] + c[3] / 2.0]);
    rt.frame(0.016);
    assert!(rt.state.cloud.unfolded.contains("N-1"), "the chip unfolds it");
    assert!(rt.presented("map-ribbon-N-1").is_some(), "its rail drawn");
    // on an opaque panel that holds all its steps (p11, 2: the rail's gaps showed the card under it through)
    let (panel, _) = rt.presented("map-ribbon-panel-N-1").expect("its rail's panel");
    for k in 1..=n {
        if let Some((b, _)) = rt.presented(&format!("map-step-N-1-{k}")) {
            assert!(b[0] >= panel[0] && b[1] >= panel[1] && b[0] + b[2] <= panel[0] + panel[2] && b[1] + b[3] <= panel[1] + panel[3], "step {k} {b:?} out of its panel {panel:?}");
        }
    }
    let after: Vec<(u32, [f32; 4])> = rt.state.map.shown().iter().map(|x| (x.i, x.r)).collect();
    assert_eq!(before, after, "no card moved or came or went");
    let steps = super::ribbon::steps(&g, chat, None);
    let last = texts_in(&rt, &format!("map-step-N-1-{n}"));
    assert!(last.len() == 1 && last[0].starts_with(&format!("#{n} ")) && last[0].ends_with(" · current"), "{last:?}");
    // the first step the rail draws (the earliest may stand behind "N earlier"), not the last
    let k = (1..n).find(|k| rt.presented(&format!("map-step-N-1-{k}")).is_some()).expect("a step before the last drawn");
    let (b, _) = rt.presented(&format!("map-step-N-1-{k}")).unwrap();
    press(&mut rt, [b[0] + b[2] / 2.0, b[1] + b[3] / 2.0]);
    assert_eq!(selected(&rt).as_deref(), Some(g.node(steps[k - 1].node).id.as_str()), "a step selects what it names");
    let (c, _) = rt.presented("map-chip-N-1").expect("the chip still drawn");
    press(&mut rt, [c[0] + c[2] / 2.0, c[1] + c[3] / 2.0]);
    rt.frame(0.016);
    assert!(!rt.state.cloud.unfolded.contains("N-1") && rt.presented("map-ribbon-N-1").is_none(), "the chip again folds it");
    // U on the card selected, Esc
    super::canvas::select(&mut rt.state, Some(chat));
    rt.key(rune::KeyEvent { key: rune::Key::Text("u".into()), mods: Default::default() });
    rt.frame(0.016);
    assert!(rt.state.cloud.unfolded.contains("N-1"), "U unfolds the card selected");
    rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: Default::default() });
    rt.frame(0.016);
    assert!(rt.state.cloud.unfolded.is_empty() && selected(&rt).as_deref() == Some("N-1"), "Esc folds it and goes nowhere");
    // pr's satellite: its chip unfolds pr's passes
    let (c, _) = rt.presented("map-chip-agent:pr").expect("pr's satellite's chip");
    press(&mut rt, [c[0] + c[2] / 2.0, c[1] + c[3] / 2.0]);
    rt.frame(0.016);
    assert!(rt.state.cloud.unfolded.contains("agent:pr"), "{:?}", rt.state.cloud.unfolded);
    assert!(rt.presented("map-ribbon-agent:pr").is_some(), "pr's passes on a rail");
    // at a past moment the chip counts the steps by then
    let past = steps[1].at;
    rt.state.cloud.at = Some(past);
    rt.state.map.set_moment(Some(past));
    rt.invalidate();
    rt.frame(0.016);
    let then = super::ribbon::count(&g, chat, Some(past));
    assert!(then < n, "{then} by then, {n} now");
    assert_eq!(texts_in(&rt, "map-chip-N-1"), [format!("◷ {then}")]);
}

/// The window of [`ribbon_stills`]: [`satellite_app`], his chat unfolded where [`RIBBON_SHOWN`] says so, at its moment.
fn ribbon_shown() -> App {
    let (_held, mut s, _) = satellite_app();
    let (unfold, at) = RIBBON_SHOWN.lock().unwrap().clone();
    s.cloud.unfolded = unfold.into_iter().map(String::from).collect();
    s.cloud.at = at;
    s
}

/// What the next still of [`ribbon_stills`] unfolds, and its moment.
static RIBBON_SHOWN: std::sync::Mutex<(Vec<&'static str>, Option<i64>)> = std::sync::Mutex::new((Vec::new(), None));

/// The owner's N-1791 p1, 2, as stills, on [`satellite_app`]'s made-up record, 1400 by 900: the cloud's whole with its
/// chips (ribbon-1-chips); his chat's chip pressed, its history on a rail under it (ribbon-2-unfolded); its step
/// before the last pressed, selected, the inspector open (ribbon-3-step); pr's satellite's chip pressed, its passes on a rail
/// (ribbon-4-brain); his chat unfolded at 09:30, the rail as far as then (ribbon-5-past).
/// `STILL_OUT=DIR yaaihh-tests map::paths::ribbon_stills --ignored --exact`.
#[test]
#[ignore]
fn ribbon_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let click = |t: f32, key: &str| format!(r#"{{"t": {t}, "type": "click", "key": "{key}"}}"#);
    let hm = |t: &str| docket_machine::machine::secs(&format!("2026-11-22T{t}:00Z"));
    // the step before the last, which the rail draws however many earlier ones stand behind "N earlier"
    let (g, _) = super::cloud::cloud(&super::cloud::tests::base(), None, "project:research", "research", &mut Default::default()).unwrap();
    let n = super::ribbon::count(&g, g.find_in(g.project("project:research").unwrap(), "N-1").unwrap(), None);
    let step = format!("map-step-N-1-{}", n - 1);
    let stills: [(&str, String, &str, Vec<&'static str>, Option<i64>); 5] = [
        ("ribbon-1-chips", "[]".into(), "1.0", vec![], None),
        ("ribbon-2-unfolded", format!("[{}]", click(1.0, "map-chip-N-1")), "1.6", vec![], None),
        ("ribbon-3-step", format!("[{}, {}]", click(1.0, "map-chip-N-1"), click(1.6, &step)), "2.2", vec![], None),
        ("ribbon-4-brain", format!("[{}]", click(1.0, "map-chip-agent:pr")), "1.6", vec![], None),
        ("ribbon-5-past", "[]".into(), "1.0", vec!["N-1"], hm("09:30")),
    ];
    for (name, script, at, unfold, moment) in stills {
        *RIBBON_SHOWN.lock().unwrap() = (unfold, moment);
        let f = out.join(format!("{name}.json"));
        std::fs::write(&f, script).unwrap();
        let args = vec!["--snap".to_string(), out.join(format!("{name}.png")).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), at.into(), "--script".into(), f.display().to_string()];
        let spec = rune::AppSpec { poll: Some(map_poll), on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), ribbon_shown, crate::view) };
        rune::App::new(spec).args(args).try_run().unwrap();
    }
}

/// The window of [`real_stills`]: [`satellite_app`]'s with no made-up tickets and no brain passes, its cloud's base the
/// answer [`REAL_SHOWN`] holds, what it unfolds unfolded and its camera where it aims.
fn real_shown() -> App {
    let (_held, mut s) = laid_app();
    s.projects[0].protocol = coord_types::GRAPH_PAGE_PROTOCOL;
    s.projects[0].live.tickets.clear();
    if s.sidebar.is_open() {
        s.sidebar.toggle();
    }
    let (answer, unfold, aim) = REAL_SHOWN.lock().unwrap().clone();
    s.cloud.of = 0;
    s.cloud.kept_read = Some(Err("a test's places are not kept".into()));
    s.cloud.base = answer.map(Ok);
    s.cloud.unfolded = unfold.into_iter().collect();
    if let Some((at, scale)) = aim {
        s.map.aim(at, scale);
    }
    s
}

/// What the next still of [`real_stills`] draws: its base answer, what it unfolds, where its camera aims.
#[allow(clippy::type_complexity)]
static REAL_SHOWN: std::sync::Mutex<(Option<coord_types::GraphAnswer>, Vec<String>, Option<([f64; 2], f64)>)> = std::sync::Mutex::new((None, Vec::new(), None));

/// The owner's N-1791 p10, 1: the Map and an unfolded rail on a real record, not a made-up one: coordd's answers to the
/// cloud's asks on a copy of a record (coordd's `a_coords_record_graph_answers`, its GRAPH_ANSWERS_OUT as REAL_ANSWERS:
/// `base.json`, his layer, as the cloud asks it, and `base-agents.json`, with the agents' layer on). 1400 by 900: the
/// cloud's whole (real-1-whole); the thread with the longest history at the whole unfolded, the camera on it
/// (real-2-unfolded); its step before the last pressed, the inspector open (real-3-step); with the agents' layer, the
/// same (real-4-agents). Beside each picture NAME.txt: the texts it draws, top to bottom, each with its place, so a
/// still is read as text too. `STILL_OUT=DIR REAL_ANSWERS=DIR yaaihh-tests map::paths::real_stills --ignored --exact`.
#[test]
#[ignore]
fn real_stills() {
    let (Ok(out), Ok(dir)) = (std::env::var("STILL_OUT"), std::env::var("REAL_ANSWERS")) else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let read = |name: &str| -> coord_types::GraphAnswer {
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(std::path::Path::new(&dir).join(format!("{name}.json"))).unwrap()).unwrap();
        serde_json::from_value(v["answer"].clone()).unwrap()
    };
    let click = |t: f32, key: &str| format!(r#"[{{"t": {t}, "type": "click", "key": "{key}"}}]"#);
    for (base, tag) in [("base", "real"), ("base-agents", "real-agents")] {
        let answer = read(base);
        let (g, _) = super::cloud::cloud(&answer, None, "project:research", "research", &mut Default::default()).unwrap();
        let p = g.project("project:research").unwrap();
        // the thread with the longest history, as its chip counts it now
        let (top, n) = g.children(p).iter().filter(|i| g.node(**i).kind == Kind::Thread).map(|i| (*i, super::ribbon::count(&g, *i, None))).max_by_key(|x| x.1).expect("a thread");
        let id = g.node(top).id.clone();
        println!("{tag}: {} nodes, {} edges; the longest history {id}, {n} steps", g.nodes.len(), g.edges.len());
        let aim = still_aim(&g, &id, 1.0);
        let mut stills: Vec<(String, String, Vec<String>, Option<([f64; 2], f64)>)> = Vec::new();
        if base == "base" {
            stills.push((format!("{tag}-1-whole"), "[]".into(), vec![], None));
            stills.push((format!("{tag}-2-unfolded"), "[]".into(), vec![id.clone()], Some(aim)));
            stills.push((format!("{tag}-3-step"), click(1.0, &format!("map-step-{id}-{}", n - 1)), vec![id.clone()], Some(aim)));
        } else {
            stills.push((format!("{tag}-4-whole"), "[]".into(), vec![], None));
            stills.push((format!("{tag}-5-unfolded"), "[]".into(), vec![id.clone()], Some(aim)));
        }
        for (name, script, unfold, aim) in stills {
            *REAL_SHOWN.lock().unwrap() = (Some(answer.clone()), unfold, aim);
            // its texts, as the window draws them at this size
            let mut rt = window(real_shown());
            settle(&mut rt);
            rt.frame(0.016);
            let mut said: Vec<([f32; 4], String)> = rt.elements().into_iter().filter_map(|e| e.text.clone().map(|t| (e.rect, t))).collect();
            said.sort_by(|a, b| (a.0[1], a.0[0]).partial_cmp(&(b.0[1], b.0[0])).unwrap());
            let text: String = said.iter().map(|(r, t)| format!("{:>5.0} {:>5.0} {:>4.0}x{:<3.0} {}\n", r[0], r[1], r[2], r[3], t.replace('\n', " / "))).collect();
            std::fs::write(out.join(format!("{name}.txt")), text).unwrap();
            drop(rt);
            let f = out.join(format!("{name}.json"));
            std::fs::write(&f, script).unwrap();
            let args = vec!["--snap".to_string(), out.join(format!("{name}.png")).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.6".into(), "--script".into(), f.display().to_string()];
            let spec = rune::AppSpec { poll: Some(map_poll), on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), real_shown, crate::view) };
            rune::App::new(spec).args(args).try_run().unwrap();
        }
    }
}

/// [`satellite_app`] seen from twice as far as its whole: its threads small cards, its agents' nodes points one over the
/// other, and their satellites under one another, each with a line back to its node. From four times as far the cloud
/// draws none of the threads (8e54b7dc's still), and the satellites alone show nothing they are beside.
fn satellite_far() -> App {
    let (_held, mut s, _) = satellite_app();
    let (g, _) = super::cloud::cloud(&super::cloud::tests::base(), None, "project:research", "research", &mut Default::default()).unwrap();
    let (at, scale) = still_aim(&g, "project:research", 0.5);
    s.map.aim(at, scale);
    s
}

/// The brain's satellites' stills (the owner, N-1791 p1, 3), on [`satellite_app`]'s record, 1400 by 900: the cloud at
/// its whole with pr's satellite at work and w2's quiet, and the Brain switches in the bar (satellite-1-whole); pr's
/// satellite pressed, its pass selected, the inspector open (satellite-2-pressed); twice as far out ([`satellite_far`],
/// satellite-3-far).
/// `STILL_OUT=DIR yaaihh-tests map::paths::map_satellite_stills --ignored --exact`.
#[test]
#[ignore]
fn map_satellite_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let press = r#"[{"t": 1.0, "type": "click", "key": "map-brain-agent:pr"}]"#;
    let whole: fn() -> App = || satellite_app().1;
    for (name, script, at, init) in [("satellite-1-whole", "[]", "1.0", whole), ("satellite-2-pressed", press, "1.6", whole), ("satellite-3-far", "[]", "1.0", satellite_far as fn() -> App)] {
        let f = out.join(format!("{name}.json"));
        std::fs::write(&f, script).unwrap();
        let args = vec!["--snap".to_string(), out.join(format!("{name}.png")).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), at.into(), "--script".into(), f.display().to_string()];
        let spec = rune::AppSpec { poll: Some(map_poll), on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) };
        rune::App::new(spec).args(args).try_run().unwrap();
    }
}

/// consult-3 N-1479 p33, docket-edge6 p35, through the canvas: a post drawn open has its steps placed in it; a call
/// drawn near enough asks its wire on the frame after, once; refused, it is drawn and asked nothing more over the
/// frames that follow. Not in the Windows test exe, as deep.rs's tests with a pipe (docket-edge8 31f2b5cb): the pipe
/// to coordd is `sh -c cat`, and Windows has no sh.
#[test]
#[cfg(not(windows))]
fn the_canvas_asks_a_calls_wire_once_and_a_refusal_not_again_over_the_frames() {
    let mut s = crate::picture::docket::tests::wire_app();
    // a coordd before the graph's, as `map_app`'s: the Map's own places
    s.projects[0].protocol = coord_types::GRAPH_PROTOCOL - 1;
    let root = format!("project:{}", s.projects[0].id);
    let post = Rect::new(110.0, 120.0, 380.0, 60.0);
    let g = Graph::new(
        vec![
            Node::new(root.clone(), Kind::Project, "research", None, Rect::new(0.0, 0.0, 10_000.0, 8_000.0)),
            Node::new("N-7", Kind::Thread, "N-7", Some(0), Rect::new(100.0, 100.0, 400.0, 600.0)),
            Node::new("N-7.p1", Kind::Post, "p1", Some(1), post),
        ],
        vec![],
    )
    .unwrap();
    s.map.set_graph(g);
    s.map.aim(post.mid(), 4.0);
    s.sel = Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Cloud, at: None });
    let out = std::env::temp_dir().join(format!("yaaihh-map-canvas-wire-{}.jsonl", std::process::id()));
    let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
    s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
    s.projects[0].down = None;
    let mut rt = window(s);
    for _ in 0..3 {
        rt.invalidate();
        rt.frame(0.016);
    }
    let call = format!("{}.s1", crate::picture::docket::tests::run_id(1));
    let g = rt.state.map.graph().unwrap().clone();
    let c = g.find_in(0, &call).expect("the post drawn open: its steps and their calls placed");
    assert_eq!(g.node(g.node(c).parent.unwrap()).parent, Some(2), "in its step, in the post");
    let size = rt.state.map.size();
    rt.state.map.cam = super::camera::Camera::fitting(&g.node(c).rect, size, 28.0);
    let raws = |rt: &Runtime<App>| rt.state.projects[0].pending.values().filter(|w| matches!(w, crate::Pending::Raw { .. })).count();
    for _ in 0..5 {
        rt.invalidate();
        rt.frame(0.016);
    }
    assert_eq!(raws(&rt), 1, "asked once over five frames");
    let id = *rt.state.projects[0].pending.iter().find(|(_, w)| matches!(w, crate::Pending::Raw { .. })).unwrap().0;
    let Some(crate::Pending::Raw { agent, older, page }) = rt.state.projects[0].pending.remove(&id) else { unreachable!() };
    crate::raw::replied(&mut rt.state, 0, agent, older, page, false, "no proxy log here\n".into());
    for _ in 0..8 {
        rt.invalidate();
        rt.frame(0.016);
    }
    assert_eq!(raws(&rt), 0, "the refusal is not asked again over eight frames");
    rt.state.projects[0].link = None;
    let _ = child.wait();
    let _ = std::fs::remove_file(&out);
}

/// The window as the Map opens on the made-up snapshot, laid (the first step of path 2, and the overview still), with
/// paper-research's answer 699 held by reference, a place in its coord's texts, as an agent's answer at its turn's
/// end is: its words already read, as the window has them once its ask was answered (the stills' window has no pipe).
fn laid_opened() -> App {
    let mut s = laid_app().1;
    let loc = docket_machine::Loc { file: "/made-up/research/tickets/texts/paper-research.jsonl".into(), offset: 4096, len: 812 };
    s.tickets.texts.insert(crate::texts::key(&loc), Some(Ok(ANSWER.into())));
    s.projects[0].live.tickets.get_mut(&1104).expect("his chat").posts[698].text = docket_machine::Text::Loc { loc };
    s
}

/// The words of paper-research's answer 699 in the stills (`laid_opened`), made up: his p678 asked for d h m.
const ANSWER: &str = "Done: the Accounts tab says each limit's reset in d h m now, as you asked in 678.\n\n- The gateway's \
    meters are read from its own page, so the tab and the corner agree.\n- A limit under an hour says its minutes alone.\n\n\
    Checked on the made-up record at research's size; the stills are in the folder.";

/// The stills of the Map laid from the made-up snapshot at research's size, 1400 by 900, in dark (his look): the
/// overview as it opens (level 1, consult-3 N-1479 p49) and the pointer over one of its unlabelled topics (p65), then
/// a still for each step of path 2 as the test above presses it: into research, the gateway selected with its press to
/// the Accounts tab. That tab's page in his window is the gateway's own (proxy/, `ext::tab_page`), which this crate's
/// tests do not have: no still of it. Then from research, a double-click on his chat's topic (at research's fit a
/// card, his chat not drawn), one on his chat, its column of his latest post (p112), and one on that post's row, its
/// card (level 5, p71 and p80). Last, from the first view, a double-click on cav-paper in the canvas's right
/// INSPECTOR points, which the inspector its first click opens covers (p177). The points pressed are read from the
/// same window run headless first.
#[test]
#[ignore]
fn map_laid_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let (away, gw, topic) = {
        let (_held, s) = laid_app();
        let mut rt = window(s);
        let away = bare_for_two(&rt, 0).unwrap();
        // an unlabelled topic's point, as the Map opens (p65: what it is, under the pointer)
        let g = rt.state.map.graph().unwrap().clone();
        let topic = rt.state.map.shown().iter().filter(|x| !x.lit && x.look != Look::Open && g.node(x.i).kind == Kind::Topic).min_by(|a, b| (a.r[2] * a.r[3]).total_cmp(&(b.r[2] * b.r[3]))).and_then(|x| bare(&rt, x.i)).unwrap();
        rt.pointer_move(away[0], away[1]);
        for _ in 0..2 {
            rt.pointer_down(away[0], away[1]);
            rt.pointer_up(away[0], away[1]);
        }
        rt.frame(0.016);
        settle(&mut rt);
        let gw = pressed_at(&rt, "account:gateway").unwrap();
        press(&mut rt, gw);
        assert!(says(&rt, "Open the Accounts tab").is_some());
        (away, gw, topic)
    };
    // into research, his chat's topic, his chat, its latest post: each point bare and left of the inspector the first
    // click opens
    let (holds, line, post, answer) = {
        let (_held, s) = laid_app();
        let mut rt = window(s);
        let twice = |rt: &mut Runtime<App>, at: [f32; 2]| {
            rt.pointer_move(at[0], at[1]);
            for _ in 0..2 {
                rt.pointer_down(at[0], at[1]);
                rt.pointer_up(at[0], at[1]);
            }
            rt.frame(0.016);
            settle(rt);
        };
        twice(&mut rt, away);
        let g = rt.state.map.graph().unwrap().clone();
        let chat = g.find_in(0, "N-1104").unwrap();
        let holds = bare_for_two(&rt, g.node(chat).parent.unwrap()).expect("his chat's topic in research's view");
        twice(&mut rt, holds);
        let line = bare_for_two(&rt, chat).expect("his chat in its topic's view");
        twice(&mut rt, line);
        let last = g.find_in(0, "N-1104.p700").unwrap();
        let post = bare_for_two(&rt, last).expect("his chat's latest post in its column's view");
        // paper-research's answer before it (`laid_opened`), level 5 of an agent's answer (p181)
        let answer = bare_for_two(&rt, g.find_in(0, "N-1104.p699").unwrap()).expect("answer 699 in the column's view");
        twice(&mut rt, post);
        assert_eq!(selected(&rt).as_deref(), Some("N-1104.p700"));
        (holds, line, post, answer)
    };
    let strip = {
        let (_held, s) = laid_app();
        let mut rt = window(s);
        settle(&mut rt);
        in_the_strip(&rt).1
    };
    let twice = |t: f32, q: [f32; 2]| format!(r#"{{"t": {t}, "type": "double-click", "x": {}, "y": {}}}"#, q[0], q[1]);
    let into = format!(r#"{{"t": 0.3, "type": "double-click", "x": {}, "y": {}}}"#, away[0], away[1]);
    let tap = |t: f32, q: [f32; 2]| format!(r#"{{"t": {t}, "type": "tap", "x": {}, "y": {}}}"#, q[0], q[1]);
    let steps = [
        ("laid-1-overview", String::new(), "0.5"),
        ("laid-1-hover", format!(r#"[{{"t": 0.2, "type": "move", "x": {}, "y": {}}}]"#, topic[0], topic[1]), "0.6"),
        ("laid-2-into", format!("[{into}]"), "2.0"),
        ("laid-3-gateway", format!("[{into}, {}]", tap(1.8, gw)), "2.4"),
        ("laid-4-column", format!("[{into}, {}, {}]", twice(1.8, holds), twice(3.4, line)), "5.0"),
        ("laid-5-answer", format!("[{into}, {}, {}, {}]", twice(1.8, holds), twice(3.4, line), twice(5.0, answer)), "6.6"),
        ("laid-5-card", format!("[{into}, {}, {}, {}]", twice(1.8, holds), twice(3.4, line), twice(5.0, post)), "6.6"),
        ("laid-6-strip", format!("[{}]", twice(0.3, strip)), "2.0"),
    ];
    for (name, script, at) in steps {
        let mut args = vec!["--snap".to_string(), out.join(format!("{name}.png")).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), at.into()];
        if !script.is_empty() {
            let f = out.join(format!("{name}.json"));
            std::fs::write(&f, script).unwrap();
            args.extend(["--script".into(), f.display().to_string()]);
        }
        let spec = rune::AppSpec { poll: Some(map_poll), on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), laid_opened, crate::view) };
        rune::App::new(spec).args(args).try_run().unwrap();
    }
    // what level 1 shows, for the head: each island's label and badge, its topics drawn and how
    let (_held, s) = laid_app();
    let rt = window(s);
    let v = &rt.state.map;
    let g = v.graph().unwrap().clone();
    let mut looks: std::collections::BTreeMap<String, usize> = Default::default();
    for x in v.shown() {
        *looks.entry(format!("{:?} {:?}{}", g.node(x.i).kind, x.look, if x.lit { " lit" } else { "" })).or_default() += x.many.max(1) as usize;
    }
    let says: Vec<String> = g.roots().map(|r| format!("{}: {:?}", g.node(r).label, texts_in(&rt, &format!("map-n-{r}")))).collect();
    std::fs::write(out.join("level-1.txt"), format!("scale {:.4}\n{says:#?}\n{looks:#?}\nnodes {}\n", v.cam.scale, g.nodes().len())).unwrap();
}

// ---- his asks' rings and their trail (layout.rs `Lit`, join.rs `put`, canvas.rs `mark`, lod.rs `edges`) ------------

/// The scales the ring's test and stills read his chat at, in points a unit ([`SCALES`]' third and fourth): its posts
/// points, each a mark; its posts labelled, a row each, the mark at its left. (Fitted to the whole record, as the Map
/// opens, p678's point lies under its topic card's head, and shows once that opens: lod.rs `visible_with`.)
const RING_POINTS: f64 = 0.5;
const RING_NEAR: f64 = 2.5;

/// [`laid_app`] with the asks on research's snapshot (made_up.rs `asks`), laid; the camera at node AT's middle at
/// SCALE, else the whole record as the Map opens.
fn ring_app(at: Option<(&str, f64)>) -> (std::sync::MutexGuard<'static, ()>, App) {
    let (held, mut s) = laid_app();
    let now = chrono::Utc::now().timestamp();
    made_up::asks(&mut s.projects[0].live.tickets, now);
    assert!(super::join::keep(&mut s, now), "laid");
    if let Some((id, scale)) = at {
        let g = s.map.graph().expect("laid").clone();
        s.map.aim(g.node(g.find_in(0, id).unwrap_or_else(|| panic!("{id} laid"))).rect.mid(), scale);
    }
    (held, s)
}

/// The trail's edges drawn now: each end's id, and whether lit.
fn trails(rt: &Runtime<App>) -> Vec<(String, String, bool)> {
    let (g, shown, e) = drawn_edges(rt);
    let mut t: Vec<_> = e.iter().filter(|x| x.link == super::Link::Trail).map(|x| (g.node(shown[x.from].i).id.clone(), g.node(shown[x.to].i).id.clone(), x.lit)).collect();
    t.sort();
    t
}

/// p715 path 1 with the ledger (docket-edge5 N-1479 p313; made_up.rs `asks`): his chat's posts as points, his open
/// p678 is a ring; near, a row each, p678 the ring and p676, which each it asked has answered, the ring with his dot,
/// and proto-window's ask of him on N-3018 the ring in the waits-on-you ink; with nothing selected no trail is drawn
/// (consult-3 p316 point 1); a hover on p676 says when each answered, and a click lights its trail to p677 beside it
/// and on to N-3009's answer, none to W-12's (point 2).
#[test]
fn path_1_his_ask_rings_and_a_click_lights_its_trail_to_each_answer() {
    for (id, scale, look, word) in [("N-1104.p678", RING_POINTS, Look::Dot, "open"), ("N-3018.p1", RING_NEAR, Look::Label, "to-him")] {
        let (_held, s) = ring_app(Some((id, scale)));
        let rt = window(s);
        let i = rt.state.map.graph().unwrap().find_in(0, id).unwrap();
        assert_eq!(drawn(&rt, id).map(|d| d.1), Some(look), "{id} at {scale}");
        assert!(rt.presented(&format!("map-ask-{word}-{i}")).is_some(), "{id}'s ring");
        assert!(trails(&rt).is_empty(), "nothing selected: no trail");
    }
    // in his chat
    let (_held, s) = ring_app(Some(("N-1104.p676", RING_NEAR)));
    let mut rt = window(s);
    let g = rt.state.map.graph().unwrap().clone();
    let i = |id: &str| g.find_in(0, id).unwrap();
    let (p676, p678) = (i("N-1104.p676"), i("N-1104.p678"));
    assert_eq!(drawn(&rt, "N-1104.p676").map(|d| d.1), Some(Look::Label), "a row, its mark at its left");
    let ring = rt.presented(&format!("map-ask-answered-{p676}")).expect("p676's ring with his dot").0;
    assert!(ring[2] > 6.0, "wider than a post's dot: {ring:?}");
    assert!(rt.presented(&format!("map-ask-open-{p678}")).is_some(), "p678's ring, empty");
    assert!(rt.presented(&format!("map-ask-answered-{p678}")).is_none());
    assert!(trails(&rt).is_empty(), "nothing selected: no trail");
    let at = pressed_at(&rt, "N-1104.p676").unwrap();
    rt.pointer_move(at[0], at[1]);
    rt.frame(0.016);
    let hover = texts_in(&rt, "map-preview");
    assert!(hover.iter().any(|t| t.starts_with("post · paper-research answered at ") && t.contains(", docket-edge8 at ") && t.contains(", docket-edge2 answered · p676 · you · ")), "{hover:?}");
    press(&mut rt, at);
    assert_eq!(selected(&rt).as_deref(), Some("N-1104.p676"));
    let t = trails(&rt);
    assert_eq!(t.len(), 2, "to p677 and on to N-3009's answer, none to W-12's: {t:?}");
    assert!(t.iter().all(|(a, b, lit)| *lit && (a == "N-1104.p676" || b == "N-1104.p676")), "{t:?}");
    assert!(t.iter().any(|(a, b, _)| a == "N-1104.p677" || b == "N-1104.p677"), "{t:?}");
}

/// The ring's stills (docket-edge5 N-1479 p313), on the made-up record with its asks, in dark (his look): his chat's
/// posts as points, p678 its ring; near, p678's ring and p676's with his dot; the hover on p676; the click, its trail
/// lit; proto-window's ask of him near; p678 selected as a point, its ring and the selection's apart, both of the
/// text's ink (docket-edge6 p388). And `ring.txt`: each asking post's mark and note.
#[test]
#[ignore]
fn map_ring_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let (key, open, words) = {
        let (_held, s) = ring_app(Some(("N-1104.p676", RING_NEAR)));
        let rt = window(s);
        let g = rt.state.map.graph().unwrap().clone();
        let words: Vec<String> = g.nodes().iter().filter(|n| n.ring.is_some()).map(|n| format!("{} | {:?} | {}", n.id, n.ring.unwrap(), n.note)).collect();
        (format!("map-n-{}", g.find_in(0, "N-1104.p676").unwrap()), format!("map-n-{}", g.find_in(0, "N-1104.p678").unwrap()), words)
    };
    std::fs::write(out.join("ring.txt"), words.join("\n") + "\n").unwrap();
    let steps: [(&str, fn() -> App, String, &str); 6] = [
        ("ring-1-points", || ring_app(Some(("N-1104.p678", RING_POINTS))).1, String::new(), "0.5"),
        ("ring-2-near", || ring_app(Some(("N-1104.p676", RING_NEAR))).1, String::new(), "0.5"),
        ("ring-3-hover", || ring_app(Some(("N-1104.p676", RING_NEAR))).1, format!(r#"[{{"t": 0.2, "type": "move", "key": "{key}"}}]"#), "0.7"),
        ("ring-4-trail", || ring_app(Some(("N-1104.p676", RING_NEAR))).1, format!(r#"[{{"t": 0.2, "type": "tap", "key": "{key}"}}]"#), "0.9"),
        ("ring-5-to-him", || ring_app(Some(("N-3018.p1", RING_NEAR))).1, String::new(), "0.5"),
        ("ring-6-points-selected", || ring_app(Some(("N-1104.p678", RING_POINTS))).1, format!(r#"[{{"t": 0.2, "type": "tap", "key": "{open}"}}]"#), "0.7"),
    ];
    for (name, init, script, t) in steps {
        let mut args = vec!["--snap".to_string(), out.join(format!("{name}.png")).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), t.into()];
        if !script.is_empty() {
            let f = out.join(format!("{name}.json"));
            std::fs::write(&f, script).unwrap();
            args.extend(["--script".into(), f.display().to_string()]);
        }
        let spec = rune::AppSpec { poll: Some(map_poll), on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) };
        rune::App::new(spec).args(args).try_run().unwrap();
    }
}

// ---- the graph page (lanes.rs): the Map of a coordd that serves the graph --------------------------------------------

/// The asks of the graph on their way to project 0's coordd: each its id and its ask.
fn graph_asked(rt: &Runtime<App>) -> Vec<(u64, docket_machine::graph_slice::Ask)> {
    let mut v: Vec<_> = rt.state.projects[0].pending.iter().filter_map(|(id, w)| if let crate::Pending::Graph(a) = w { Some((*id, a.clone())) } else { None }).collect();
    v.sort_by_key(|x| x.0);
    v
}

/// Coordd's answer to the graph's ask on its way: the made-up graph's (lanes.rs `tests::graph`), sliced by that ask,
/// taken as the window takes a reply, so the view runs again.
fn graph_answered(rt: &mut Runtime<App>) {
    let (id, ask) = graph_asked(rt).pop().expect("an ask of the graph on its way");
    let out = serde_json::to_string(&super::lanes::tests::answer(&ask)).unwrap();
    rt.update(|s: &mut App, _| {
        s.take(0, crate::link::Incoming::Reply(id, coord_types::Reply { ok: true, out, post: None }));
    });
    rt.frame(0.016);
    rt.frame(0.016);
}

/// The middle of element KEY, in window points.
fn middle_of(rt: &Runtime<App>, key: &str) -> [f32; 2] {
    let (r, _) = rt.presented(key).unwrap_or_else(|| panic!("{key} drawn"));
    [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0]
}

/// [`laid_app`] whose research's coordd serves the graph (`GRAPH_PROTOCOL`), its pipe a sink: the Map is the graph page.
#[cfg(not(windows))]
fn lanes_app() -> (std::sync::MutexGuard<'static, ()>, App, std::process::Child, std::path::PathBuf) {
    let (held, mut s) = laid_app();
    s.projects[0].protocol = coord_types::GRAPH_PAGE_PROTOCOL;
    // the timeline page as it was, in the cloud's place (cloud.rs `Cloud::lanes`)
    s.cloud.lanes = true;
    let (child, out) = super::deep::tests::piped(&mut s, 0, "lanes");
    (held, s, child, out)
}

/// paper-research N-1446 p1385 B, proto-window p1383 head 2: where research's coordd serves the graph, the Map is drawn
/// from it. It opens around his newest post (the made-up snapshot's N-1104.p700), asked of coordd once, with what is
/// worked on now, and draws the answer: the switches, the line saying what is on and the focus as id and label, his post
/// in his lane, its answer in the agents', the commit and its job in work, the delivery's mark, now's line and the row
/// of his asks nobody took, counted. A click on a card makes it the focus, asked of coordd in its turn; Esc goes back to the
/// focus before; a switch turns its layer off in the ask. The Map's own places are not laid.
#[test]
#[cfg(not(windows))]
fn the_graph_page_opens_around_his_newest_post_and_a_click_on_a_card_makes_it_the_focus() {
    let (_held, s, child, out) = lanes_app();
    let mut rt = window(s);
    rt.frame(0.016);
    let asked = graph_asked(&rt);
    assert_eq!(asked.len(), 1, "asked once over three frames: {asked:?}");
    assert_eq!(asked[0].1, super::lanes::tests::around("N-1104.p700"), "around his newest post, 3 hops, with now");
    assert!(rt.state.map.graph().is_none(), "the Map's places not laid");
    graph_answered(&mut rt);
    let line = texts_in(&rt, "lanes-line").join(" ");
    assert!(line.starts_with("On: his messages, agent to agent, deliveries, work, calls and prompts, files · 2 posts"), "{line}");
    assert!(line.contains("around N-1104.p700 “Make the graph page”, 3 hops"), "{line}");
    for key in ["lanes-card-N-1104.p700", "lanes-card-N-1446.p1400", "lanes-card-commit:0f40917b", "lanes-card-job:1-1", "lanes-now", "lanes-asks"] {
        assert!(rt.presented(key).is_some(), "{key} drawn");
    }
    assert_eq!(said_under(&rt, "lanes-asks-name"), ["his asks nobody took · 1"], "his ask nobody took");
    // his post's course under the line (p1429, 1): reached by docket-edge5 1 m after he asked, taken 8 m after that,
    // the longest, which stands out; 5 m so far since its green job, not in his window yet; not merged
    let course = texts_in(&rt, "lanes-course").join(" ");
    for said in ["Its course", "asked", "reached docket-edge5", "taken by docket-edge5", "first commit 0f40917b", "first green job 1-1", "5 m so far →", "not in your window yet", "not reached: merged"] {
        assert!(course.contains(said), "{said}: {course}");
    }
    assert_eq!(texts_in(&rt, "lanes-course-longest"), ["8 m →"]);
    assert!(rt.presented("lanes-course-line").is_some() && rt.presented("lanes-course-line-longest").is_some(), "along his post's row");
    // its answer's card, clicked: the focus, asked once it is
    let at = middle_of(&rt, "lanes-card-N-1446.p1400");
    press(&mut rt, at);
    assert_eq!(rt.state.lanes.ask.around.as_deref(), Some("N-1446.p1400"));
    assert_eq!(rt.state.lanes.back, ["N-1104.p700"]);
    rt.frame(0.016);
    assert_eq!(graph_asked(&rt).last().and_then(|(_, a)| a.around.clone()).as_deref(), Some("N-1446.p1400"), "the new focus asked");
    graph_answered(&mut rt);
    assert!(texts_in(&rt, "lanes-line").join(" ").contains("around N-1446.p1400 “The page, around his post”, 3 hops"));
    assert!(rt.presented("lanes-course").is_none() && rt.presented("lanes-course-line").is_none(), "an agent's post has no course");
    // Esc: back to his post
    crate::view_cost::press(&mut rt, rune::Key::Escape);
    assert_eq!((rt.state.lanes.ask.around.as_deref(), rt.state.lanes.back.len()), (Some("N-1104.p700"), 0));
    rt.frame(0.016);
    graph_answered(&mut rt);
    // the deliveries switched off: asked without them
    let at = middle_of(&rt, "lanes-switch-Deliveries");
    press(&mut rt, at);
    rt.frame(0.016);
    let (_, ask) = graph_asked(&rt).pop().expect("asked again");
    use docket_machine::graph_slice::Layer;
    assert_eq!(ask.layers, [Layer::His, Layer::Agents, Layer::Work, Layer::Calls, Layer::Files].into_iter().collect());
    rt.state.projects[0].link = None;
    let sent = super::deep::tests::sent(child, &out);
    assert!(sent.iter().all(|v| v["req"]["op"] == "graph"), "only the graph asked: {sent:?}");
    assert_eq!(sent.len(), 4);
}

/// The texts of the elements under the one keyed KEY in the tree, itself among them, in the order drawn: not what
/// lies under its rect, as [`texts_in`] reads, since a sheet over the lanes has the cards' words beneath it.
fn said_under(rt: &Runtime<App>, key: &str) -> Vec<String> {
    rt.elements().into_iter().filter(|e| !e.exiting && e.path.split('/').any(|k| k == key)).filter_map(|e| e.text).collect()
}

/// paper-research N-1446 p1510, 2: each row of his asks in now's lane says its count, "his asks nobody took · 1", and
/// across how long its ask has waited, not the newest ask's words (a bar with them read as a single ask). A click on
/// the row, its name or across, opens its list over the lanes, a line an ask: its id, its first words, how long it has
/// waited; a click on the row again shuts it. A click on a line makes that ask the focus, asked of coordd in its turn,
/// and shuts the list.
#[test]
#[cfg(not(windows))]
fn a_row_of_his_asks_says_its_count_and_a_click_opens_its_list() {
    let (_held, s, child, out) = lanes_app();
    let mut rt = window(s);
    rt.frame(0.016);
    graph_answered(&mut rt);
    assert_eq!(said_under(&rt, "lanes-asks-name"), ["his asks nobody took · 1"]);
    assert_eq!(said_under(&rt, "lanes-asks"), ["▸ it has waited 15 m"], "how long, not its words");
    assert!(rt.presented("lanes-asks-list").is_none(), "its list shut");
    // 19 of his open with no commit: their row's name whole with its count, wider than the other names' cut, and the
    // words across it start past it (p1532's stills: "his asks open, no commit…", its "· 19" cut off)
    rt.update(|s: &mut App, _| {
        if let Some((_, _, Ok(a))) = &mut s.lanes.got {
            a.now.as_mut().unwrap().open = (0..19).map(|i| format!("N-9.p{i}")).collect();
        }
    });
    rt.frame(0.016);
    assert_eq!(said_under(&rt, "lanes-open-name"), ["his asks open, no commit · 19"]);
    let rect = |key: &str| rt.elements().into_iter().find(|e| !e.exiting && e.text.is_some() && e.path.split('/').any(|k| k == key)).map(|e| e.rect).unwrap_or_else(|| panic!("{key} drawn"));
    let (name, words) = (rect("lanes-open-name"), rect("lanes-open"));
    assert!(name[2] > super::lanes::LEFT - 26.0, "whole, wider than the names' cut: {name:?}");
    assert!(name[0] + name[2] < words[0], "its words past it: {name:?}, {words:?}");
    // the row clicked at its name: its list, a line for N-1479.p10 asked at 08:05, at now 08:20
    let at = middle_of(&rt, "lanes-asks-name");
    press(&mut rt, at);
    assert_eq!(rt.state.lanes.listed.as_deref(), Some(""));
    assert!(rt.presented("lanes-asks-list").is_some(), "its list open");
    assert_eq!(said_under(&rt, "lanes-asks-line-N-1479.p10"), ["N-1479.p10", "And the stills", "15 m"], "its id, its first words, how long");
    assert_eq!(said_under(&rt, "lanes-asks"), ["▾ it has waited 15 m"]);
    // clicked again, across: shut
    let at = middle_of(&rt, "lanes-asks");
    press(&mut rt, at);
    assert!(rt.state.lanes.listed.is_none() && rt.presented("lanes-asks-list").is_none(), "shut again");
    // opened, and its line clicked: that ask the focus, the list shut
    press(&mut rt, at);
    let line = middle_of(&rt, "lanes-asks-line-N-1479.p10");
    press(&mut rt, line);
    assert_eq!(rt.state.lanes.ask.around.as_deref(), Some("N-1479.p10"));
    assert_eq!(rt.state.lanes.back, ["N-1104.p700"]);
    assert!(rt.state.lanes.listed.is_none() && rt.presented("lanes-asks-list").is_none(), "the list shut");
    rt.frame(0.016);
    assert_eq!(graph_asked(&rt).last().and_then(|(_, a)| a.around.clone()).as_deref(), Some("N-1479.p10"), "the new focus asked");
    rt.state.projects[0].link = None;
    let sent = super::deep::tests::sent(child, &out);
    assert!(sent.iter().all(|v| v["req"]["op"] == "graph"), "only the graph asked: {sent:?}");
}

/// The graph page's stills, 1400 × 900: `lanes-made-up` from the made-up graph (lanes.rs `tests::graph`) around his
/// p700; and from LANES_ANSWERS, a folder of coordd's answers (`NAME.json`, each `{"ask": …, "answer": …}`, as
/// coordd's `a_coords_record_graph_answers` writes them from his record), one still each, `lanes-NAME`. The window's
/// projects file is one of its own naming no project, so the bar says no error and nothing is asked of a coordd; the
/// sidebar is folded, since what it would list is the made-up snapshot's (made_up.rs `research`: its threads N-3000
/// and on), not his record's (paper-research N-1446 p1469, 7). An answer with asks of his nobody took has a second
/// still, `lanes-NAME-asks`, with their row's list open (p1510, 2).
/// `STILL_OUT=DIR [LANES_ANSWERS=DIR] yaaihh-tests map::paths::lanes_stills --ignored --exact`.
#[test]
#[ignore]
fn lanes_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    // view_cost's test_app's folder, whose projects.toml the window reads at its start
    let own = std::env::temp_dir().join(format!("yaaihh-view-cost-{}", std::process::id()));
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(own.join("projects.toml"), "# the graph page's stills: no project, nothing asked of a coordd\n").unwrap();
    let mut shown: Vec<(String, docket_machine::graph_slice::Ask, coord_types::GraphAnswer)> = Vec::new();
    let ask = super::lanes::tests::around("N-1104.p700");
    shown.push(("made-up".into(), ask.clone(), super::lanes::tests::answer(&ask)));
    if let Ok(dir) = std::env::var("LANES_ANSWERS") {
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
        files.sort();
        for f in files {
            let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
            let name = f.file_stem().unwrap().to_string_lossy().to_string();
            shown.push((name, serde_json::from_value(v["ask"].clone()).unwrap(), serde_json::from_value(v["answer"].clone()).unwrap()));
        }
    }
    let mut stills: Vec<(String, docket_machine::graph_slice::Ask, coord_types::GraphAnswer, Option<String>)> = Vec::new();
    for (name, ask, answer) in shown {
        if answer.now.as_ref().is_some_and(|n| !n.not_taken.is_empty()) {
            stills.push((format!("{name}-asks"), ask.clone(), answer.clone(), Some(super::lanes::NOT_TAKEN.to_string())));
        }
        stills.push((name, ask, answer, None));
    }
    for (name, ask, answer, listed) in stills {
        *LANES_SHOWN.lock().unwrap() = Some((ask, answer, listed));
        let args = vec!["--snap".to_string(), out.join(format!("lanes-{name}.png")).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1".into()];
        let spec = rune::AppSpec { poll: Some(map_poll), on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), lanes_shown, crate::view) };
        rune::App::new(spec).args(args).try_run().unwrap();
    }
}

/// The ask and the answer the next still of [`lanes_stills`] shows, and the row of his asks whose list is open.
type StillShown = (docket_machine::graph_slice::Ask, coord_types::GraphAnswer, Option<String>);
static LANES_SHOWN: std::sync::Mutex<Option<StillShown>> = std::sync::Mutex::new(None);

/// The window at the graph page, holding [`LANES_SHOWN`]'s answer to its ask, as coordd's answer is held once it has
/// come: research's snapshot made up, its coordd serving the graph.
fn lanes_shown() -> App {
    let (_held, mut s) = laid_app();
    s.projects[0].protocol = coord_types::GRAPH_PAGE_PROTOCOL;
    s.cloud.lanes = true;
    if s.sidebar.is_open() {
        s.sidebar.toggle();
    }
    let (ask, answer, listed) = LANES_SHOWN.lock().unwrap().clone().expect("an answer to show");
    s.lanes.listed = listed;
    s.lanes.ask = ask.clone();
    s.lanes.ask_of = 0;
    s.lanes.got = Some((0, ask, Ok(answer)));
    s
}

// ---- the cloud (cloud.rs): the Map of a coordd that serves the graph -------------------------------------------------

/// paper-research N-1446 p1656, on the study p1655: where research's coordd serves the graph, the Map is the cloud. It
/// asks coordd for his layer whole, once, and draws the answer: his threads as cards with their titles, his posts as
/// cards inside them, the presets and the switches over them; no timeline, no now rows, no counts. A click on a thread
/// goes in (p824): the camera flies into it and it is the focus; a switch on asks the slice around it with that layer.
#[test]
#[cfg(not(windows))]
fn the_cloud_opens_on_his_messages_and_a_click_goes_in() {
    let (_held, mut s) = laid_app();
    s.projects[0].protocol = coord_types::GRAPH_PAGE_PROTOCOL;
    let (child, out) = super::deep::tests::piped(&mut s, 0, "cloud");
    let mut rt = window(s);
    rt.frame(0.016);
    let asked = graph_asked(&rt);
    assert_eq!(asked.iter().map(|(_, a)| a.clone()).collect::<Vec<_>>(), [super::cloud::base_ask()], "his layer asked, once");
    assert!(rt.presented("cloud-waiting").is_some() && rt.presented("cloud-presets").is_some(), "the bar over its wait");
    // coordd's answer: his posts on four threads (cloud.rs `tests::base`)
    let base = serde_json::to_string(&super::cloud::tests::base()).unwrap();
    rt.update(|s: &mut App, _| {
        s.take(0, crate::link::Incoming::Reply(asked[0].0, coord_types::Reply { ok: true, out: base.clone(), post: None }));
    });
    rt.frame(0.016);
    rt.frame(0.016);
    assert!(rt.state.map.is_cloud() && rt.presented("cloud-waiting").is_none(), "the cloud built");
    let g = rt.state.map.graph().unwrap().clone();
    let p = g.project("project:research").unwrap();
    let n1 = g.find_in(p, "N-1").unwrap();
    let said = texts_in(&rt, &format!("map-n-{n1}"));
    assert!(said.iter().any(|t| t == "Your one group chat"), "the thread's title: {said:?}");
    assert!(said.iter().any(|t| t == "his words in N-1.p1"), "his post's words in it: {said:?}");
    // his asks' words, right of their ring, end where a card's words end, 8 points in from its right edge, as his
    // other posts' do: they ran the ring and its gap past it ("…its way" in cloud-2b of 8588ef17)
    for id in ["N-1.p1", "N-1.p3", "N-1.p9"] {
        let i = g.find_in(p, id).unwrap();
        assert_eq!(g.node(i).ring.is_some(), id != "N-1.p1", "{id}: its ring");
        let (r, _) = rt.presented(&format!("map-n-{i}")).expect(id);
        let said = format!("his words in {id}");
        let e = rt.elements().into_iter().find(|e| e.text.as_deref() == Some(said.as_str())).expect(id);
        // or 4 before its history's chip, where it has one, which they never run under (the owner, N-1791 p11, 1)
        let end = rt.presented(&format!("map-chip-{id}")).map_or(r[0] + r[2] - 8.0, |(c, _)| c[0] - 4.0);
        assert!((e.rect[0] + e.rect[2] - end).abs() < 0.5, "{id}: its words end at {}, its card at {}, they should at {end}", e.rect[0] + e.rect[2], r[0] + r[2]);
    }
    for gone in ["lanes-line", "lanes-now", "lanes-asks", "lanes-card-N-1.p1"] {
        assert!(rt.presented(gone).is_none(), "{gone}: not on the cloud");
    }
    // a click on the thread's head: it is the focus, flown into
    let at = bare(&rt, n1).expect("a bare point of N-1");
    press(&mut rt, at);
    assert_eq!(rt.state.cloud.focus.as_deref(), Some("N-1"));
    assert!(rt.state.map.flying(), "flying into it");
    // agent to agent switched on: the slice around N-1 asked with it
    let at = middle_of(&rt, "cloud-switch-Agents");
    press(&mut rt, at);
    rt.frame(0.016);
    use docket_machine::graph_slice::Layer;
    let want = super::cloud::near_ask(Some("N-1"), &[Layer::Agents].into_iter().collect()).unwrap();
    assert_eq!(graph_asked(&rt).last().map(|(_, a)| a.clone()), Some(want));
    rt.state.projects[0].link = None;
    let sent = super::deep::tests::sent(child, &out);
    assert!(sent.iter().all(|v| v["req"]["op"] == "graph"), "only the graph asked: {sent:?}");
    assert_eq!(sent.len(), 2);
}

/// Still 3 (paper-research N-1446 p1767: "Draw it as the record has it"): gone into his post, at its step's fit, the
/// cloud draws the step's card under its prompt template, as it draws every card under what it holds. The Map's own
/// step wide enough to draw its stage goes over its calls (deep.rs `over_calls`), unfilled; the cloud's is filled, and
/// drawn last it hid the template (still 3 of 4495ba98: "step 1 · gather.call" over an empty band).
#[test]
#[cfg(not(windows))]
fn the_clouds_step_is_drawn_under_its_prompt_template() {
    let (_held, mut s) = laid_app();
    let g = near_p3_cloud(&mut s);
    // the stills' fit ([`cloud_stills`])
    let (at, scale) = still_aim(&g, "O-1099511631050.s1", 1.0);
    s.map.aim(at, scale);
    let mut rt = window(s);
    settle(&mut rt);
    let g = rt.state.map.graph().unwrap().clone();
    let p = g.project("project:research").unwrap();
    let (step, template) = (g.find_in(p, "O-1099511631050.s1").unwrap(), g.find_in(p, "O-1099511631050.s1/call:gather").unwrap());
    let x = rt.state.map.shown().into_iter().find(|x| x.i == step).expect("the step drawn");
    assert!(super::deep::over_calls(Kind::Step, x.r[2]), "as wide as a Map's step drawn over its calls: {:?}", x.r);
    let els = rt.elements();
    let at = |i: u32| els.iter().position(|e| e.key.as_deref() == Some(format!("map-n-{i}").as_str()));
    let (a, b) = (at(step).expect("the step drawn"), at(template).expect("its template drawn"));
    assert!(a < b, "the step drawn at {a}, its template over it at {b}");
    let said = texts_in(&rt, &format!("map-n-{template}"));
    assert!(said.iter().any(|t| t == "its prompt template, gather.call"), "{said:?}");
}

/// p1839, 1: still 6's fit, an answer whole with the turn it was written in. It is drawn no larger than what it shows
/// (lod.rs `fit_to_shown`): its words whole over the turn's card, the turn's words whole, the answer ending a gap under
/// it, where it was laid with three quarters of it for the turn.
#[test]
#[cfg(not(windows))]
fn an_answer_is_drawn_its_words_and_the_turn_it_was_written_in() {
    let (_held, mut s) = laid_app();
    let g = near_p3_cloud(&mut s);
    let r = g.node(g.find_in(g.project("project:research").unwrap(), "N-1.p4").unwrap()).rect;
    // the stills' fit ([`cloud_stills`])
    let (at, scale) = still_aim(&g, "N-1.p4", 1.0);
    s.map.aim(at, scale);
    let mut rt = window(s);
    settle(&mut rt);
    let g = rt.state.map.graph().unwrap().clone();
    let p = g.project("project:research").unwrap();
    let (a, t) = (g.find_in(p, "N-1.p4").unwrap(), g.find_in(p, "turn:proto-mind@120").unwrap());
    let shown = rt.state.map.shown();
    let at = |i: u32| shown.iter().find(|x| x.i == i).unwrap_or_else(|| panic!("{i} drawn")).r;
    let (ar, tr) = (at(a), at(t));
    let gap = tr[0] - ar[0];
    assert!(gap > 0.0 && (ar[1] + ar[3] - (tr[1] + tr[3] + gap)).abs() < 0.5, "the answer ends a gap under its turn: {ar:?} {tr:?}");
    assert!(ar[3] < 0.5 * ar[2] * (r.h / r.w) as f32, "no longer the height laid for it: {ar:?}");
    let said = texts_in(&rt, &format!("map-n-{a}"));
    let (answer, turn) = (g.node(a), g.node(t));
    assert!(said.contains(&answer.label) && said.contains(&answer.note), "its words whole: {said:?}");
    assert!(said.contains(&turn.label) && turn.label.ends_with("2 requests, claude-opus-5-5 xhigh"), "the turn's: {said:?}");
}

/// [`laid_app`]'s window S at the cloud, the sidebar folded, his post N-1.p3 its focus with the slice around it (cloud.rs
/// `near_p3`), its first answer N-1.p4 written in proto-mind's turn, Everything on, as coordd's answers are held; the
/// graph the canvas lays from them.
fn near_p3_cloud(s: &mut App) -> Graph {
    s.projects[0].protocol = coord_types::GRAPH_PAGE_PROTOCOL;
    if s.sidebar.is_open() {
        s.sidebar.toggle();
    }
    let (base, mut near) = (super::cloud::tests::base(), super::cloud::tests::near_p3());
    near.slice.edges.push(serde_json::from_value(serde_json::json!({"from": "N-1.p4", "to": "turn:proto-mind@120", "kind": "written_in", "line": 1, "layer": "agents"})).unwrap());
    let (g, _) = super::cloud::cloud(&base, Some(("N-1.p3", &near)), "project:research", "research", &mut Default::default()).unwrap();
    s.cloud.of = 0;
    s.cloud.kept_read = Some(Err("a test's places are not kept".into()));
    s.cloud.base = Some(Ok(base));
    s.cloud.layers = super::cloud::PRESETS[2].1.iter().copied().collect();
    s.cloud.focus = Some("N-1.p3".into());
    let ask = super::cloud::near_ask(s.cloud.focus.as_deref(), &s.cloud.layers).unwrap();
    s.cloud.near = Some((ask, Ok(near)));
    g
}

/// The canvas a still of the cloud draws in, its window 1400 × 900 less the bar and the crumbs, nothing selected:
/// `window`'s, as [`a_still_aims_where_a_flight_lands`] measures it (a still fitted 1400 × 780 before p1894, 3).
const STILL_CANVAS: [f32; 2] = [1384.0, 804.0];

/// Where a still of the cloud G aims at node ID, ZOOM times nearer: where a flight into it lands in the still's canvas
/// (canvas.rs `go_in`), the card as drawn at that fit (`cloud_fit`; p1883, 2) fitted with the flight's own room
/// (`flown`), not at 0.92 of the view (paper-research N-1446 p1894, 3); its middle and its scale, as `aim` takes them.
fn still_aim(g: &Graph, id: &str, zoom: f64) -> ([f64; 2], f64) {
    let i = g.find_in(g.project("project:research").unwrap(), id).unwrap_or_else(|| panic!("{id} in the cloud"));
    let r = canvas::cloud_fit(g, i, STILL_CANVAS, &[]);
    (r.mid(), canvas::flown(&r, STILL_CANVAS).scale * zoom)
}

/// proto-window N-1446 p1951 (p1885's still 1): at a still's fit of train 68's cut, every card of its band is drawn
/// inside the card holding it, a group's card too: "18 jobs · 4 failed", 241 points wide, drew four groups' cards of
/// 140 side by side, 43 points past its edge over "2 posts", and "8 branches" spilled over its neighbours both ways.
/// And as far inside it as a plain card is (paper-research p1994): kept flush on the box's left and bottom edges, the
/// group cards had no margin where "6 commits" keeps its cards one.
#[test]
fn at_a_cuts_fit_every_card_of_its_band_is_drawn_inside_the_card_holding_it() {
    let (g, _) = super::cloud::cloud(&super::cloud::tests::base(), Some(("commit:6467db1a", &super::cloud::tests::near_cut())), "project:research", "research", &mut Default::default()).unwrap();
    let c = g.find_in(g.project("project:research").unwrap(), "commit:6467db1a").expect("the cut's card");
    let (at, scale) = still_aim(&g, "commit:6467db1a", 1.0);
    let cam = super::camera::Camera { at: [at[0] - STILL_CANVAS[0] as f64 / 2.0 / scale, at[1] - STILL_CANVAS[1] as f64 / 2.0 / scale], scale };
    let words = |i: u32, w: f32| super::cloud::words_height(&g, i, w);
    let (mut shown, stand) = lod::visible_worded(&g, &cam, STILL_CANVAS, &[], &words);
    lod::fit_to_shown(&g, &cam, &mut shown, &stand, &words);
    let drawn: std::collections::HashMap<u32, [f32; 4]> = shown.iter().filter(|s| !s.lit).map(|s| (s.i, s.r)).collect();
    let in_band = |i: u32| std::iter::successors(g.node(i).parent, |p| g.node(*p).parent).any(|p| p == c);
    let (mut held, mut groups) = (0, 0);
    for s in shown.iter().filter(|s| !s.lit && in_band(s.i)) {
        let p = g.node(s.i).parent.unwrap();
        let Some(o) = drawn.get(&p) else { continue };
        let r = s.r;
        held += 1;
        groups += usize::from(s.group.is_some());
        assert!(
            r[0] >= o[0] - 0.01 && r[1] >= o[1] - 0.01 && r[0] + r[2] <= o[0] + o[2] + 0.01 && r[1] + r[3] <= o[1] + o[3] + 0.01,
            "{} (and {} more) at {r:?} past {} at {o:?}",
            g.node(s.i).id,
            s.many.saturating_sub(1),
            g.node(p).label
        );
    }
    assert!(held >= 9 && groups > 0, "the band's nine cards, and its jobs too small for their words as groups at this fit: {held} held, {groups} groups");
    // the inset of a box's cards: how far its cards keep from its left edge, and its bottom from the lowest of them
    let boxed = |label: &str| {
        let b = *g.children(c).iter().find(|k| g.node(**k).label == label).unwrap_or_else(|| panic!("{label} in the band"));
        let o = drawn[&b];
        let kids: Vec<(&[f32; 4], bool)> = shown.iter().filter(|s| !s.lit && g.node(s.i).parent == Some(b)).map(|s| (&s.r, s.group.is_some())).collect();
        let left = kids.iter().map(|(r, _)| r[0] - o[0]).fold(f32::INFINITY, f32::min);
        let right = kids.iter().map(|(r, _)| o[0] + o[2] - (r[0] + r[2])).fold(f32::INFINITY, f32::min);
        let bottom = kids.iter().map(|(r, _)| o[1] + o[3] - (r[1] + r[3])).fold(f32::INFINITY, f32::min);
        (left, right, bottom, kids.iter().any(|(_, x)| *x))
    };
    let (plain, _, _, none) = boxed("6 commits");
    assert!(plain > 1.0 && !none, "6 commits' cards drawn as themselves, {plain} inside it");
    for label in ["18 jobs · 4 failed", "8 branches"] {
        let (left, right, bottom, grouped) = boxed(label);
        assert!(grouped, "{label} holds groups at this fit");
        assert!((left - plain).abs() < 0.01 && (bottom - plain).abs() < 0.01 && right >= plain - 0.01, "{label}: its cards {left} from its left, {right} from its right, {bottom} from its bottom; 6 commits' {plain}");
    }
}

/// p1894, 3: a still of the cloud aims at a card where a flight into it lands ([`still_aim`]): his post, proto-mind's
/// session in it, his post's first answer, each fitted as drawn with the flight's own room, the canvas's middle at
/// its middle; it aimed at 0.92 of the view. The window is one inspector wider than a still's: nothing selected, its
/// canvas is the still's and the inspector's room; once the first press has opened the inspector, every flight fits
/// the canvas it leaves, the still's.
#[test]
#[cfg(not(windows))]
fn a_still_aims_where_a_flight_lands() {
    let (_held, mut s) = laid_app();
    near_p3_cloud(&mut s);
    let mut rt = window_at(s, (1400.0 + canvas::INSPECTOR, 900.0));
    settle(&mut rt);
    assert_eq!(rt.state.map.size(), [STILL_CANVAS[0] + canvas::INSPECTOR, STILL_CANVAS[1]], "the still's canvas and the inspector's room");
    let g = rt.state.map.graph().unwrap().clone();
    let p3 = g.find_in(g.project("project:research").unwrap(), "N-1.p3").unwrap();
    rt.update(move |s, _| canvas::go_in(s, p3));
    settle(&mut rt);
    for id in ["N-1.p3", "requests:N-1.p3/session:s-proto", "N-1.p4"] {
        let g = rt.state.map.graph().unwrap().clone();
        let i = g.find_in(g.project("project:research").unwrap(), id).unwrap();
        rt.update(move |s, _| canvas::go_in(s, i));
        settle(&mut rt);
        assert_eq!(rt.state.map.size(), STILL_CANVAS, "{id}: the canvas the inspector leaves");
        let (at, scale) = still_aim(&g, id, 1.0);
        let cam = rt.state.map.cam;
        let mid = [cam.at[0] + STILL_CANVAS[0] as f64 / 2.0 / cam.scale, cam.at[1] + STILL_CANVAS[1] as f64 / 2.0 / cam.scale];
        assert!((cam.scale / scale - 1.0).abs() < 1e-4, "{id}: the flight landed at scale {}, the still aims at {scale}", cam.scale);
        assert!((mid[0] - at[0]).abs() * scale < 0.1 && (mid[1] - at[1]).abs() * scale < 0.1, "{id}: the flight's middle {mid:?}, the still's {at:?}");
    }
}

/// A flight into his post (a double-click: canvas.rs `go_in`) fits it as it is drawn there, cut to what it shows
/// (canvas.rs `cloud_fit`; paper-research N-1446 p1883, 2: still 6's bundle, framed at its laid size, took the top
/// third of the view, two thirds empty under it): whole in the canvas, its middle at the canvas's, as large as the
/// canvas takes it. And so a flight from there into a card deep in it, proto-mind's session among the requests that
/// carried it, packed up from where it was laid: drawn at its own fit, not left out as laid below that view (lod.rs
/// `visible_worded`; found merging pw-h34, whose press on a block in that session found the session not drawn).
#[test]
fn a_flight_into_his_post_fits_it_as_drawn() {
    let (_held, mut s) = laid_app();
    near_p3_cloud(&mut s);
    let mut rt = window(s);
    settle(&mut rt);
    let g = rt.state.map.graph().unwrap().clone();
    let at = |id: &str| g.find_in(g.project("project:research").unwrap(), id).unwrap();
    let (p3, session) = (at("N-1.p3"), at("requests:N-1.p3/session:s-proto"));
    for (i, what) in [(p3, "his post"), (session, "proto-mind's session in it")] {
        rt.update(move |s, _| canvas::go_in(s, i));
        settle(&mut rt);
        let [w, h] = rt.state.map.size();
        let scale = rt.state.map.cam.scale;
        let x = rt.state.map.shown().into_iter().find(|x| x.i == i && x.group.is_none() && !x.lit).unwrap_or_else(|| panic!("{what} drawn at its fit, the camera {:?}", rt.state.map.cam)).r;
        if i == p3 {
            assert!(x[3] < 0.9 * (g.node(p3).rect.h * scale) as f32, "cut from its laid height {}: {x:?}", g.node(p3).rect.h * scale);
        }
        assert!(x[0] >= 0.0 && x[1] >= 0.0 && x[0] + x[2] <= w && x[1] + x[3] <= h, "{what} whole in the canvas {w}×{h}: {x:?}");
        assert!((x[1] + x[3] / 2.0 - h / 2.0).abs() < 0.02 * h, "{what}'s middle at the canvas's {}: {x:?}", h / 2.0);
        assert!(x[2] >= 0.97 * (w - 56.0) || x[3] >= 0.97 * (h - 56.0), "{what} as large as the canvas takes it: {x:?} in {w}×{h}");
    }
}

/// p1821, H3: in the cloud, his press on a block's card in a session (still 4's "a system prompt block, 9449 bytes")
/// goes in, as every card's does, and opens the block's text apart, titled by the session's agent and the block, as a
/// folder's block does (p1642, 2). Its text is asked of coordd on that press only, once over the frames
/// (`Request::Raw` of its node, `text:b`), the popup saying so until it comes, then the text; nothing is asked before.
/// Never kept: closed, its text is let go, and a press again asks again (p1844), as a folder's block's (extends
/// [`a_blocks_version_opens_apart_with_its_text_asked_of_coordd_on_the_press`]).
#[test]
#[cfg(not(windows))]
fn a_block_pressed_in_the_cloud_opens_its_text_apart_asked_of_coordd_on_the_press() {
    let (_held, mut rt, child, out) = cloud_at_session("cloud-block");
    assert!(aparts_asked(&rt).is_empty() && rt.state.popups.is_empty(), "nothing asked before the press");
    let b = press_block(&mut rt, "text:b");
    assert_eq!(rt.state.map.graph().unwrap().node(b).note, "carried by 2 requests of 1 session");
    assert_eq!(rt.state.cloud.focus.as_deref(), Some("N-1.p3"), "gone into a card of the focus's: the focus kept");
    assert_eq!(rt.state.popups.len(), 1, "one popup");
    assert_eq!(rt.state.popups[0].what, crate::popup::What::Block { pi: 0, agent: "proto-mind".into(), id: "text:b".into() });
    assert_eq!(rt.state.popups[0].title, "proto-mind · a system prompt block, 9449 bytes · its text");
    assert_eq!(aparts_asked(&rt), ["text text:b"], "its text asked once over the frames");
    assert!(in_popup(&rt, "loading…"), "said until it comes");
    // coordd's answer: the block's words, under where they come from
    text_came(&mut rt, true, "You are proto-mind, an agent of research.");
    assert!(in_popup(&rt, "Its text:"));
    assert!(in_popup(&rt, "You are proto-mind, an agent of research."), "its text");
    assert!(aparts_asked(&rt).is_empty(), "nothing asked again while it is open");
    // closed: its text let go with its popup; pressed again, asked again
    rt.state.popups.clear();
    settle(&mut rt);
    assert!(rt.state.folders.texts.is_empty(), "its text let go with its popup");
    press_block(&mut rt, "text:b");
    assert_eq!(aparts_asked(&rt), ["text text:b"], "asked again on the next press");
    rt.state.projects[0].link = None;
    let sent = super::deep::tests::sent(child, &out);
    assert_eq!(sent.iter().filter(|v| v["req"]["id"] == "text:b").count(), 2, "asked once on each press: {sent:?}");
}

/// p1889, 1: a file the session ran with (its memory, under `~/.claude` on his record: "AutoMem …/MEMORY.md") offers
/// no press. coordd serves only a block a request carried (graph_kept.rs, its leg: it refuses one ran with), so a press
/// could only be refused, a window-high panel saying no (h34-5 of p1888). His press on its card goes in, as every
/// card's does, and opens nothing: nothing is asked of coordd, nothing holds its path. Its note says what it is.
#[test]
#[cfg(not(windows))]
fn a_file_the_session_ran_with_offers_no_press_and_nothing_reads_it() {
    let (_held, mut rt, child, out) = cloud_at_session("cloud-ran-with");
    let m = press_block(&mut rt, "text:m");
    assert_eq!(rt.state.map.graph().unwrap().node(m).note, "ran with 1 request of 1 session");
    assert_eq!(rt.state.cloud.focus.as_deref(), Some("N-1.p3"), "gone into a card of the focus's: the focus kept");
    assert!(rt.state.popups.is_empty(), "no popup: {:?}", rt.state.popups.iter().map(|p| &p.title).collect::<Vec<_>>());
    assert!(aparts_asked(&rt).is_empty(), "nothing asked over the frames");
    rt.state.projects[0].link = None;
    let sent = super::deep::tests::sent(child, &out);
    assert!(!sent.iter().any(|v| v["req"]["op"] == "raw"), "no text asked: {sent:?}");
    assert!(!sent.iter().any(|v| v.to_string().contains("MEMORY.md")), "its path asked of nobody: {sent:?}");
}

/// p1889, 2: a popup is as tall as what it holds, up to the window (popup.rs `layer`; it was the window's height
/// whatever it held): the block's popup, one line while its text is asked, is its head and that line; its text of 400
/// lines come, it is the window less the layer's margin (16 above, 16 below), its body scrolling, in the frame the
/// text is drawn in, not sprung there over the frames after it.
#[test]
#[cfg(not(windows))]
fn a_popup_is_as_tall_as_what_it_holds_up_to_the_window() {
    let (_held, mut rt, child, out) = cloud_at_session("cloud-popup-fit");
    press_block(&mut rt, "text:b");
    let rect = |rt: &Runtime<App>, key: &str| rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key} drawn")).rect;
    let window = rect(&rt, "kit-root");
    assert!(in_popup(&rt, "loading…"));
    let d = rect(&rt, "popup0-dialog");
    assert!(d[3] < 120.0, "its head and one line: {d:?} in {window:?}");
    let long = (1..=400).map(|n| format!("line {n} of the block")).collect::<Vec<_>>().join("\n");
    text_came(&mut rt, true, &long);
    let (d, body) = (rect(&rt, "popup0-dialog"), rect(&rt, "popup0-scroll"));
    assert!((d[3] - (window[3] - 32.0)).abs() < 1.0 && d[1] >= window[1] + 15.0, "the window less its margin: {d:?} in {window:?}");
    assert!(body[1] + body[3] <= d[1] + d[3] + 0.5, "its body inside it, scrolling: {body:?} in {d:?}");
    rt.state.projects[0].link = None;
    super::deep::tests::sent(child, &out);
}

/// The cloud gone into his post N-1.p3 ([`near_p3_cloud`]) at proto-mind's session's fit, as a still draws it
/// ([`still_aim`]), research's coordd serving blocks' texts, its pipe a sink named NAME; the popups' asks polled
/// ([`block_poll`]).
#[cfg(not(windows))]
fn cloud_at_session(name: &str) -> (std::sync::MutexGuard<'static, ()>, Runtime<App>, std::process::Child, std::path::PathBuf) {
    let (held, mut s) = laid_app();
    let g = near_p3_cloud(&mut s);
    s.projects[0].protocol = coord_types::TEXTS_PROTOCOL;
    let (child, out) = super::deep::tests::piped(&mut s, 0, name);
    let (at, scale) = still_aim(&g, "requests:N-1.p3/session:s-proto", 1.0);
    s.map.aim(at, scale);
    let mut rt = window(s);
    rt.poll = Some(block_poll);
    settle(&mut rt);
    (held, rt, child, out)
}

/// A press on a bare point of block ID's card in proto-mind's session, and the frames after it: its node.
fn press_block(rt: &mut Runtime<App>, id: &str) -> u32 {
    let g = rt.state.map.graph().unwrap().clone();
    let b = g.find_in(g.project("project:research").unwrap(), &format!("requests:N-1.p3/session:s-proto/{id}")).unwrap();
    let at = bare(rt, b).unwrap_or_else(|| panic!("a bare point of {id}'s card"));
    press(rt, at);
    for _ in 0..4 {
        rt.frame(0.016);
    }
    b
}

/// coordd's answer to the block's text asked, OK or not, OUT its words or why not.
fn text_came(rt: &mut Runtime<App>, ok: bool, out: &str) {
    let id = rt.state.projects[0].pending.iter().find(|(_, w)| matches!(w, crate::Pending::FolderText { .. })).map(|(id, _)| *id).expect("a block's text asked");
    let out = out.to_string();
    rt.update(move |s: &mut App, _| {
        s.take(0, crate::link::Incoming::Reply(id, coord_types::Reply { ok, out: out.clone(), post: None }));
    });
    rt.frame(0.016);
    rt.frame(0.016);
}

/// p1821, H4 (p1844): in the cloud, the run that handled his post says whose its program is, and each step's prompt
/// template whose its call is, first, from the Kinds answer the window holds (`s.tickets.kinds`): agent-base's `base`
/// and `gather.call`, research's own `route.call`. A handler two kinds have says nothing until the snapshot says which
/// kind its `when` is written in (`ViewRan::from`); an answer asked of another project says nothing here; while no
/// Kinds answer is held nothing is said, and the cloud asks nothing for it.
#[test]
#[cfg(not(windows))]
fn a_run_and_its_templates_say_whose_they_are() {
    let (_held, mut s) = laid_app();
    let g = near_p3_cloud(&mut s);
    let (child, out) = super::deep::tests::piped(&mut s, 0, "cloud-whose");
    s.projects[0].live.places = Some(coord_types::Places { coord: "/c".into(), definition: Some("/f/projects/research".into()), ..Default::default() });
    let kind = |name: &str, file: &str| -> crate::kinds::KindForm {
        let calls = serde_json::json!([{"name": "gather", "file": "/ab/calls/gather.call"}, {"name": "route", "file": "/f/projects/research/calls/route.call"}]);
        serde_json::from_value(serde_json::json!({"kind": name, "form": [], "file": file, "text": format!("kind {name}\n  when owner_post(p) do\n    p <- gather p;\n"), "calls": calls})).unwrap()
    };
    let (ab, own) = (kind("base", "/ab/kinds/base.kind"), kind("thread", "/f/projects/research/kinds/thread.kind"));
    s.tickets.kinds = Some(Ok(vec![ab.clone()]));
    s.tickets.kinds_of = Some("research".into());
    // still 2's fit, the run whole
    let (at, scale) = still_aim(&g, "run:O-1099511631050", 1.0);
    s.map.aim(at, scale);
    let mut rt = window(s);
    settle(&mut rt);
    let g = rt.state.map.graph().unwrap().clone();
    let p = g.project("project:research").unwrap();
    let said = |rt: &Runtime<App>, id: &str| texts_in(rt, &format!("map-n-{}", g.find_in(p, id).unwrap()));
    let run = |rt: &Runtime<App>| said(rt, "run:O-1099511631050").into_iter().find(|t| t.starts_with("2 steps · ")).expect("the run's note");
    assert!(run(&rt).ends_with(" · agent-base's"), "{}", run(&rt));
    assert!(said(&rt, "O-1099511631050.s1/call:gather").iter().any(|t| t == "agent-base's call file it is formed from"), "{:?}", said(&rt, "O-1099511631050.s1/call:gather"));
    assert!(said(&rt, "O-1099511631050.s2/call:route").iter().any(|t| t == "research's call file it is formed from"), "{:?}", said(&rt, "O-1099511631050.s2/call:route"));
    // the answer asked of cav-paper, shown before: nothing said of research's cards
    rt.update(|s: &mut App, _| s.tickets.kinds_of = Some("cav-paper".into()));
    rt.frame(0.016);
    assert!(!run(&rt).contains("'s"), "{}", run(&rt));
    assert!(said(&rt, "O-1099511631050.s1/call:gather").iter().any(|t| t == "the call file it is formed from"));
    rt.update(|s: &mut App, _| s.tickets.kinds_of = Some("research".into()));
    // two kinds with a `when owner_post`: which one ran it unknown, nothing said
    rt.update(move |s: &mut App, _| s.tickets.kinds = Some(Ok(vec![ab.clone(), own.clone()])));
    rt.frame(0.016);
    assert!(!run(&rt).contains("'s"), "{}", run(&rt));
    // the snapshot's run of it says its `when` is thread's: research's own
    rt.update(|s: &mut App, _| {
        let t = s.projects[0].live.tickets.values_mut().next().expect("a ticket in the snapshot");
        t.ran.push(serde_json::from_value(serde_json::json!({"id": "O-1099511631050", "event": "owner_post", "from": "thread", "line": 2, "post": 3, "opened_at": "2026-11-22T08:10:00Z", "said": "Send"})).unwrap());
    });
    rt.frame(0.016);
    assert!(run(&rt).ends_with(" · research's"), "{}", run(&rt));
    // no Kinds answer held: nothing said of whose, and nothing asked for it
    rt.update(|s: &mut App, _| s.tickets.kinds = None);
    rt.frame(0.016);
    rt.frame(0.016);
    assert!(!run(&rt).contains("'s"), "{}", run(&rt));
    assert!(said(&rt, "O-1099511631050.s1/call:gather").iter().any(|t| t == "the call file it is formed from"));
    rt.state.projects[0].link = None;
    let sent = super::deep::tests::sent(child, &out);
    assert!(sent.is_empty(), "nothing asked: {sent:?}");
}

/// The cloud's poll, then the popups' asks (sidebar.rs `apart_poll`), as lib.rs `poll` runs them.
fn block_poll(s: &mut App, clock: f64, tx: &mut Tx) -> bool {
    canvas::poll(s, clock, tx) | crate::sidebar::apart_poll(s)
}

/// The cloud's stills of his record (paper-research N-1446 p1656), 1400 × 900, from CLOUD_ANSWERS, a folder of
/// coordd's answers as its `a_coords_record_graph_answers` writes them (`NAME.json`, each `{"ask": …, "answer": …}`):
/// `cloud.json` his layer whole, the cloud's base. CLOUD_STILLS lists the stills, a JSON list of `{"name", "fit"?,
/// "zoom"?, "focus"?, "near"?}`: each drawn with the camera fitting node FIT (none: the whole cloud) and zoomed ZOOM
/// times past that fit about its middle; FOCUS gone into, with NEAR's answer (`NAME` of the folder) its slice, the
/// layers on that it was asked with. The places are those the cloud makes from nothing, never kept; the projects file
/// names no project, the sidebar is folded (as [`lanes_stills`]). A `kinds.json` beside the answers, when there is one
/// (`{"kinds": [KindForm…], "places": Places}`), is the Kinds answer held, research's (`Page::kinds_of`), and the places
/// its snapshot names, so a run and a prompt template say whose they are (H4, paper-research N-1446 p1859). A still's
/// "press" is the node his press is on once the cloud is laid (cloud.rs `pressed`: H3's popup), its coordd one that
/// gives a block's text; "refused", coordd's words refusing its text, as they come back for a file the session ran with.
/// `STILL_OUT=DIR CLOUD_ANSWERS=DIR CLOUD_STILLS=FILE yaaihh-tests map::paths::cloud_stills --ignored --exact`.
#[test]
#[ignore]
fn cloud_stills() {
    let (Ok(out), Ok(dir), Ok(list)) = (std::env::var("STILL_OUT"), std::env::var("CLOUD_ANSWERS"), std::env::var("CLOUD_STILLS")) else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let own = std::env::temp_dir().join(format!("yaaihh-view-cost-{}", std::process::id()));
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(own.join("projects.toml"), "# the cloud's stills: no project, nothing asked of a coordd\n").unwrap();
    let read = |name: &str| -> (docket_machine::graph_slice::Ask, coord_types::GraphAnswer) {
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(std::path::Path::new(&dir).join(format!("{name}.json"))).unwrap()).unwrap();
        (serde_json::from_value(v["ask"].clone()).unwrap(), serde_json::from_value(v["answer"].clone()).unwrap())
    };
    let (_, base) = read("cloud");
    let kinds = std::fs::read_to_string(std::path::Path::new(&dir).join("kinds.json")).ok().map(|t| {
        let v: serde_json::Value = serde_json::from_str(&t).unwrap();
        let kinds: Vec<crate::kinds::KindForm> = serde_json::from_value(v["kinds"].clone()).unwrap();
        (kinds, serde_json::from_value::<coord_types::Places>(v["places"].clone()).unwrap())
    });
    let stills: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(&list).unwrap()).unwrap();
    for still in stills {
        let name = still["name"].as_str().unwrap().to_string();
        let focus = still["focus"].as_str().map(String::from);
        let near = still["near"].as_str().map(read);
        let layers: std::collections::BTreeSet<docket_machine::graph_slice::Layer> = near.as_ref().map(|(a, _)| a.layers.iter().copied().filter(|l| *l != docket_machine::graph_slice::Layer::His).collect()).unwrap_or_default();
        // the camera: where a flight into FIT lands in the canvas under the bar and the crumbs ([`still_aim`]), ZOOM
        // times nearer
        let aim = still["fit"].as_str().map(|id| {
            let (g, _) = super::cloud::cloud(&base, focus.as_deref().zip(near.as_ref().map(|(_, a)| a)), "project:research", "research", &mut Default::default()).unwrap();
            still_aim(&g, id, still["zoom"].as_f64().unwrap_or(1.0))
        });
        let press = still["press"].as_str().map(|id| (id.to_string(), still["refused"].as_str().map(String::from)));
        *CLOUD_SHOWN.lock().unwrap() = Some(CloudShown { base: base.clone(), focus, layers, near: near.map(|(_, a)| a), aim, kinds: kinds.clone(), press });
        let args = vec!["--snap".to_string(), out.join(format!("{name}.png")).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1".into()];
        let spec = rune::AppSpec { poll: Some(still_poll), on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), cloud_shown, crate::view) };
        rune::App::new(spec).args(args).try_run().unwrap();
    }
}

/// What the next still of [`cloud_stills`] shows.
#[derive(Clone)]
struct CloudShown {
    base: coord_types::GraphAnswer,
    focus: Option<String>,
    layers: std::collections::BTreeSet<docket_machine::graph_slice::Layer>,
    near: Option<coord_types::GraphAnswer>,
    aim: Option<([f64; 2], f64)>,
    kinds: Option<(Vec<crate::kinds::KindForm>, coord_types::Places)>,
    /// The node pressed once the cloud is laid, and coordd's refusal of its text.
    press: Option<(String, Option<String>)>,
}
static CLOUD_SHOWN: std::sync::Mutex<Option<CloudShown>> = std::sync::Mutex::new(None);

/// The window at the cloud, holding [`CLOUD_SHOWN`]'s answers as coordd's are held once they have come: research's
/// snapshot made up, its coordd serving the graph, the places not kept.
fn cloud_shown() -> App {
    let (_held, mut s) = laid_app();
    s.projects[0].protocol = coord_types::GRAPH_PAGE_PROTOCOL;
    if s.sidebar.is_open() {
        s.sidebar.toggle();
    }
    let c = CLOUD_SHOWN.lock().unwrap().clone().expect("a cloud to show");
    s.cloud.of = 0;
    s.cloud.kept_read = Some(Err("a still's places are not kept".into()));
    s.cloud.base = Some(Ok(c.base));
    s.cloud.layers = c.layers;
    s.cloud.focus = c.focus;
    if let Some(near) = c.near
        && let Some(ask) = super::cloud::near_ask(s.cloud.focus.as_deref(), &s.cloud.layers)
    {
        s.cloud.near = Some((ask, Ok(near)));
    }
    if let Some((at, scale)) = c.aim {
        s.map.aim(at, scale);
    }
    if let Some((kinds, places)) = c.kinds {
        s.tickets.kinds = Some(Ok(kinds));
        s.tickets.kinds_of = Some(s.projects[0].id.clone());
        s.projects[0].live.places = Some(places);
    }
    if c.press.is_some() {
        s.projects[0].protocol = coord_types::TEXTS_PROTOCOL;
    }
    *STILL_PRESS.lock().unwrap() = c.press;
    s
}

/// The press of the still drawn, until it is made ([`still_poll`]).
static STILL_PRESS: std::sync::Mutex<Option<(String, Option<String>)>> = std::sync::Mutex::new(None);

/// The Map's poll, then once the cloud is laid [`STILL_PRESS`]'s press on its node (cloud.rs `pressed`, as canvas.rs
/// `drag` makes it), and coordd's refusal of what the popup it opens asks, held as `apart_poll`'s answer is.
fn still_poll(s: &mut App, clock: f64, tx: &mut Tx) -> bool {
    let changed = map_poll(s, clock, tx);
    let Some(g) = s.map.graph().cloned() else { return changed };
    let mut press = STILL_PRESS.lock().unwrap();
    // the node once the cloud holding it is laid
    let Some(i) = press.as_ref().and_then(|(id, _)| g.find_in(g.project("project:research")?, id)) else { return changed };
    let refused = press.take().unwrap().1;
    drop(press);
    let had = s.popups.len();
    super::cloud::pressed(s, i);
    // a card that offers no press opens nothing (cloud.rs `block_of`)
    if let (Some(why), Some(crate::popup::What::Block { pi, id, .. })) = (refused, s.popups.get(had).map(|p| &p.what)) {
        s.folders.texts.insert((*pi, id.clone()), Err(why));
    }
    true
}

/// The window at research's cloud, his layer alone (cloud.rs `tests::base`, as coordd's answer is held), the sidebar
/// folded, the places not kept: the Map as it first opens; and that camera.
fn his_cloud_opened() -> (std::sync::MutexGuard<'static, ()>, Runtime<App>, super::camera::Camera) {
    let (held, mut s) = laid_app();
    s.projects[0].protocol = coord_types::GRAPH_PAGE_PROTOCOL;
    if s.sidebar.is_open() {
        s.sidebar.toggle();
    }
    s.cloud.of = 0;
    s.cloud.kept_read = Some(Err("a test's places are not kept".into()));
    s.cloud.base = Some(Ok(super::cloud::tests::base()));
    let mut rt = window(s);
    settle(&mut rt);
    assert!(rt.state.map.is_cloud(), "the cloud drawn");
    let first = rt.state.map.cam;
    (held, rt, first)
}

/// Whether camera C is FIRST, as drawn: the same scale, and the same place to under half a point.
fn same_cam(c: super::camera::Camera, first: super::camera::Camera) -> bool {
    (c.scale / first.scale - 1.0).abs() < 1e-6 && (c.at[0] - first.at[0]).abs() * first.scale < 0.5 && (c.at[1] - first.at[1]).abs() * first.scale < 0.5
}

/// paper-research N-1446 p2013, 1 (his N-1104 p869: "i lost it i zoomed out i think all the way or all way in out but
/// now its borked"): from a camera far out or far in, Home and the bar's "Whole map" fly back to the Map as it first
/// opened.
#[test]
fn home_and_whole_map_fly_back_to_the_map_as_it_first_opened() {
    use super::camera::{Camera, FARTHEST, NEAREST};
    let (_held, mut rt, first) = his_cloud_opened();
    let home = rune::KeyEvent { key: rune::Key::Home, mods: Default::default() };
    let far = Camera { at: first.at, scale: FARTHEST };
    // in at the middle of what it first showed, the record's (looking away is p2013's 3, put back by itself)
    let size = rt.state.map.size();
    let middle = [first.at[0] + size[0] as f64 / 2.0 / first.scale, first.at[1] + size[1] as f64 / 2.0 / first.scale];
    let near = Camera { at: middle, scale: NEAREST };
    // Home first, then the bar's button (which gives the keys back: `a_press_on_the_bar_gives_the_keys_back_to_the_map`)
    for by in ["Home", "Whole map"] {
        for (what, cam) in [("far out", far), ("far in", near)] {
            rt.update(move |s, _| s.map.cam = cam);
            rt.frame(0.016);
            assert!(same_cam(rt.state.map.cam, cam), "{what}: the camera stays where it was put, {:?}", rt.state.map.cam);
            if by == "Home" {
                rt.key(home.clone());
            } else {
                let (x, y) = rt.find("cloud-home").expect("the bar's Whole map");
                rt.pointer_move(x, y);
                rt.frame(0.016);
                rt.pointer_down(x, y);
                rt.frame(0.016);
                rt.pointer_up(x, y);
            }
            settle(&mut rt);
            let c = rt.state.map.cam;
            assert!(same_cam(c, first), "{what}, by {by}: {c:?}, as it first opened {first:?}");
        }
    }
}

/// paper-research N-1446 p2033, 1: a press on a control of the cloud's bar gives the keys back to the Map (canvas.rs
/// `keys_back`; lib.rs `on_key` gives them to it only with nothing focused): after "Whole map", a switch, the switch
/// again and a preset, nothing is focused, Home flies back to the whole from far out and + zooms in a notch.
#[test]
fn a_press_on_the_bar_gives_the_keys_back_to_the_map() {
    use super::camera::{Camera, FARTHEST};
    let (_held, mut rt, first) = his_cloud_opened();
    let key = |k: rune::Key| rune::KeyEvent { key: k, mods: Default::default() };
    let switch = rt.elements().into_iter().filter_map(|e| e.key).find(|k| k.starts_with("cloud-switch-")).expect("a switch on the bar");
    for (what, on) in [("Whole map", "cloud-home".to_string()), ("a switch", switch.clone()), ("the switch again", switch), ("a preset", "cloud-presets".to_string())] {
        let (x, y) = rt.find(&on).unwrap_or_else(|| panic!("{on} drawn"));
        press(&mut rt, [x, y]);
        settle(&mut rt);
        let held: Vec<String> = rt.elements().into_iter().filter(|e| e.focused).map(|e| e.key.unwrap_or_default()).collect();
        assert!(held.is_empty(), "after {what}, nothing focused: {held:?}");
        rt.update(move |s, _| s.map.cam = Camera { at: first.at, scale: FARTHEST });
        rt.frame(0.016);
        rt.key(key(rune::Key::Home));
        settle(&mut rt);
        let whole = rt.state.map.bounds().expect("bounds").0 * canvas::PER_NOTCH;
        let c = rt.state.map.cam;
        assert!((c.scale / whole - 1.0).abs() < 1e-6, "after {what}, Home flies to the whole's fit {whole}: {c:?}");
        rt.key(key(rune::Key::Text("+".into())));
        rt.frame(0.016);
        let s = rt.state.map.cam.scale;
        assert!((s / (whole * canvas::PER_NOTCH) - 1.0).abs() < 1e-6, "after {what}, + zooms in a notch from {whole}: {s}");
    }
}

/// paper-research N-1446 p2013, 2: the wheel zooms out no farther than one notch past the whole record's fit, the
/// first open's, and in no nearer than where the smallest node fills the view, across and down; where it went from
/// 1e-5 to 1e4.
#[test]
fn the_wheel_stops_a_notch_past_the_whole_and_at_the_smallest_card() {
    let (_held, mut rt, first) = his_cloud_opened();
    let g = rt.state.map.graph().unwrap().clone();
    let size = rt.state.map.size();
    let far = first.scale / canvas::PER_NOTCH;
    let fills = |r: &Rect| (size[0] as f64 / r.w).max(size[1] as f64 / r.h);
    let near = g.nodes().iter().filter(|n| n.rect.w > 0.0 && n.rect.h > 0.0).map(|n| fills(&n.rect)).fold(far, f64::max).min(super::camera::NEAREST);
    assert!(near > 10.0 * first.scale, "in: where the smallest node fills the view {near}, the first open's {}", first.scale);
    let a = rt.state.map.area;
    let mid = [a[0] + a[2] / 2.0, a[1] + a[3] / 2.0];
    // up zooms in (canvas.rs `wheel`): 120 is three notches, 60 of them some 1e17 times
    for (dy, want, what) in [(-120.0, far, "out"), (120.0, near, "in"), (-120.0, far, "out again")] {
        for _ in 0..60 {
            rt.wheel_event(WheelEvent::new(0.0, dy).at(mid[0], mid[1]));
            rt.frame(0.016);
        }
        let s = rt.state.map.cam.scale;
        assert!((s / want - 1.0).abs() < 1e-6, "{what}: 60 wheel turns end at {s}, the bound {want}");
    }
}

/// paper-research N-1446 p2013, 3: a camera that draws nothing of the record (not a number, infinite, no scale, looking
/// away) is put back on the whole in the next frame, as the first open placed it. The camera is not kept across a
/// restart (ui.json holds the window's zoom, not the Map's camera), so the first open is that on a load.
#[test]
fn a_camera_that_draws_nothing_goes_back_on_the_whole_in_the_next_frame() {
    use super::camera::Camera;
    let (_held, mut rt, first) = his_cloud_opened();
    let away = Camera { at: [first.at[0] + 1.0e9, first.at[1] - 1.0e9], scale: first.scale };
    for (what, cam) in [
        ("not a number", Camera { at: [f64::NAN, first.at[1]], scale: f64::NAN }),
        ("infinite", Camera { at: first.at, scale: f64::INFINITY }),
        ("no scale", Camera { at: first.at, scale: 0.0 }),
        ("looking away", away),
    ] {
        rt.update(move |s, _| s.map.cam = cam);
        rt.frame(0.016);
        rt.frame(0.016);
        let c = rt.state.map.cam;
        assert!(same_cam(c, first), "{what}: {c:?}, as it first opened {first:?}");
    }
}

/// proto-window N-1446 p2419, 1: the Map built again before he moved it (a post of his came, as "Read again" brings
/// it): his press on a thread's card flies into it, and the camera stays there as the inspector it opens makes the
/// canvas narrower. It went back to the whole Map, the flight gone: the graph set again left the camera unplaced, and
/// the narrower canvas's layout placed it (canvas.rs `laid_out`).
#[test]
fn a_press_into_a_card_after_the_map_was_built_again_unmoved_stays_in_it() {
    let (_held, mut rt, first) = his_cloud_opened();
    rt.poll = Some(walk_poll);
    // his new post in N-2, as coordd's answer to "Read again" has it
    rt.update(|s: &mut App, _| {
        let mut b = super::cloud::tests::base();
        let more: coord_types::GraphAnswer = serde_json::from_value(serde_json::json!({"slice": {"nodes": [{"id": "N-2.p6", "kind": "post", "line": 1, "when": "2026-11-22T12:30:00Z", "by": "owner"}],
            "edges": [{"from": "N-2.p6", "to": "N-2", "kind": "posted_in", "line": 1, "when": "2026-11-22T12:30:00Z", "layer": "his"}], "whole": {}, "shown": {}, "left": 0},
            "labels": [{"id": "N-2.p6", "is": "post", "label": "owner: his words in N-2.p6"}]}))
        .unwrap();
        b.slice.nodes.extend(more.slice.nodes);
        b.slice.edges.extend(more.slice.edges);
        b.labels.extend(more.labels);
        s.cloud.base = Some(Ok(b));
    });
    rt.frame(0.016);
    settle(&mut rt);
    let g = rt.state.map.graph().unwrap().clone();
    let r = g.project("project:research").unwrap();
    assert!(g.find_in(r, "N-2.p6").is_some(), "built again with his new post");
    assert!(same_cam(rt.state.map.cam, first), "the camera where it first opened: {:?}, {first:?}", rt.state.map.cam);
    let t = g.find_in(r, "N-2").unwrap();
    let at = bare(&rt, t).expect("N-2's card in view");
    walk_at(&mut rt, at);
    assert_eq!(rt.state.cloud.focus.as_deref(), Some("N-2"), "gone into");
    // in it: nearer than the whole Map, N-2's card in the middle of what is left of the canvas beside the inspector
    let (c, size) = (rt.state.map.cam, rt.state.map.size());
    let x = g.node(t).rect;
    let mid = [((x.x + x.w / 2.0 - c.at[0]) * c.scale) as f32, ((x.y + x.h / 2.0 - c.at[1]) * c.scale) as f32];
    assert!(c.scale > 1.5 * first.scale && (0.0..size[0]).contains(&mid[0]) && (0.0..size[1]).contains(&mid[1]), "flown into N-2: the camera {c:?} (first {first:?}), its middle at {mid:?} in {size:?}");
}

/// The window's poll for his walks, in lib.rs `poll`'s order: the Map's, history.rs's (each place recorded), the
/// structure's reveal.
fn walk_poll(s: &mut App, clock: f64, tx: &mut Tx) -> bool {
    let map = canvas::poll(s, clock, tx);
    let placed = crate::history::poll(s);
    crate::sidebar::reveal(s, tx) | placed | map
}

/// [`his_cloud_opened`] with [`walk_poll`], his chat held in the snapshot under the cloud's id for it (N-1, cloud.rs
/// `tests::base`), so its page opens.
fn his_walk() -> (std::sync::MutexGuard<'static, ()>, Runtime<App>) {
    let (held, mut rt, _) = his_cloud_opened();
    rt.poll = Some(walk_poll);
    rt.update(|s, _| {
        let mut v = s.projects[0].live.tickets.get(&1104).cloned().expect("his chat in the snapshot");
        (v.n, v.id, v.title) = (1, "N-1".into(), "Your one group chat".into());
        s.projects[0].live.tickets.insert(1, v);
    });
    settle(&mut rt);
    (held, rt)
}

/// His press at AT, then frames until a flight it started has landed.
fn walk_at(rt: &mut Runtime<App>, at: [f32; 2]) {
    rt.pointer_move(at[0], at[1]);
    rt.frame(0.016);
    rt.pointer_down(at[0], at[1]);
    rt.frame(0.016);
    rt.pointer_up(at[0], at[1]);
    for _ in 0..3 {
        rt.frame(0.3);
    }
    settle(rt);
}

/// His press on the element KEY, as on a button (not one on its way out).
fn walk_press(rt: &mut Runtime<App>, key: &str) {
    let r = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).map(|e| e.rect).unwrap_or_else(|| panic!("{key} drawn"));
    walk_at(rt, [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0]);
}

/// His press on "Open its page" in the Map's inspector.
fn open_its_page(rt: &mut Runtime<App>) {
    let (r, _) = rt.presented("map-inspector").expect("the inspector open");
    let e = rt
        .elements()
        .into_iter()
        .find(|e| {
            let (x, y) = (e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0);
            e.text.as_deref() == Some("Open its page") && x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3]
        })
        .expect("Open its page in the inspector");
    walk_at(rt, [e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0]);
}

/// The wheel turned TURNS clicks of 120 (three notches each) at window point AT; up (more than none) zooms in.
fn wheel_at(rt: &mut Runtime<App>, at: [f32; 2], turns: i32) {
    for _ in 0..turns.abs() {
        rt.wheel_event(WheelEvent::new(0.0, 120.0 * turns.signum() as f32).at(at[0], at[1]));
        rt.frame(0.016);
    }
    settle(rt);
}

/// paper-research N-1446 p2101, the walk's 1–3 (p2116; his N-1104 p895: "navigation across the tools is very very
/// rough"): Back to the Map comes back to the camera he left it at and the node he had selected, wherever he was: zoomed
/// before going into any card, on a post he pressed inside his chat's card, and zoomed out past the card he went into;
/// from a card's page and from an overview; and Forward then Back again lands there too. Each flew to a card's fit or
/// the project's, and the post selected went.
#[test]
fn back_comes_back_to_the_map_as_he_left_it() {
    let (_held, mut rt) = his_walk();
    let map = rt.state.sel.clone();
    let back = |rt: &mut Runtime<App>| walk_press(rt, "history-back");
    let page = |rt: &mut Runtime<App>, sel: Sel| {
        rt.update(move |s, _| s.sel = sel);
        settle(rt);
    };
    let here = |rt: &Runtime<App>| (rt.state.sel.clone(), rt.state.map.cam, selected(rt));
    // zoomed in over his chat's card before going into any, then the Agents page and Back
    let n1 = drawn(&rt, "N-1").expect("his chat's card drawn").0;
    wheel_at(&mut rt, n1, 1);
    let zoomed = rt.state.map.cam;
    page(&mut rt, Sel::Agents);
    back(&mut rt);
    let (sel, c, _) = here(&rt);
    assert!(sel == map && same_cam(c, zoomed), "before any card: {sel:?} at {c:?}, he was at {zoomed:?}");
    // into his chat's card, a press on his post in it, its page, Back
    let at = pressed_at(&rt, "N-1").expect("his chat's card drawn");
    walk_at(&mut rt, at);
    let at = pressed_at(&rt, "N-1.p5").expect("his post drawn in it");
    walk_at(&mut rt, at);
    let post = rt.state.map.cam;
    assert_eq!((rt.state.cloud.focus.as_deref(), selected(&rt).as_deref()), (Some("N-1"), Some("N-1.p5")), "his chat gone into, his post selected");
    open_its_page(&mut rt);
    assert_eq!((&rt.state.sel, rt.state.tickets.shown), (&Sel::Tickets, Some(1)), "its page");
    back(&mut rt);
    let (sel, c, picked) = here(&rt);
    assert!(sel == map && same_cam(c, post) && picked.as_deref() == Some("N-1.p5"), "from its page: {sel:?} at {c:?} on {picked:?}, he was at {post:?} on N-1.p5");
    // out past the card he went into, then the Kinds page and Back; Forward, and Back again
    let a = rt.state.map.area;
    wheel_at(&mut rt, [a[0] + a[2] / 2.0, a[1] + a[3] / 2.0], -2);
    let out = rt.state.map.cam;
    page(&mut rt, Sel::Kinds);
    back(&mut rt);
    let (sel, c, picked) = here(&rt);
    assert!(sel == map && same_cam(c, out) && picked.as_deref() == Some("N-1.p5"), "past the card: {sel:?} at {c:?} on {picked:?}, he was at {out:?}");
    walk_press(&mut rt, "history-forward");
    assert_eq!(rt.state.sel, Sel::Kinds, "Forward");
    back(&mut rt);
    let (sel, c, _) = here(&rt);
    assert!(sel == map && same_cam(c, out), "Back again: {sel:?} at {c:?}, he was at {out:?}");
}

/// paper-research N-1446 p2101, the walk's 4 (p2116): the bar's Map, one press from every page. From his chat open on
/// the Threads page (its head far above, as N-1104's is), it goes into his chat's card on the Map, and Back returns to
/// the thread; from paper-research's chat with his chat beside it, the same, and Back to that chat; from the Agents
/// page, the Map as he left it. Before it, nothing on a thread or a chat led to its card, and with a thread open the
/// sidebar's Map row was scrolled out of sight (at y −1800 in the walk).
#[test]
fn the_bars_map_goes_to_the_thread_shown_and_back() {
    let (_held, mut rt) = his_walk();
    let back = |rt: &mut Runtime<App>| walk_press(rt, "history-back");
    let into = |rt: &Runtime<App>, from: &str| {
        assert!(canvas::shown(&rt.state), "{from}: the Map, {:?}", rt.state.sel);
        assert_eq!((rt.state.cloud.focus.as_deref(), selected(rt).as_deref()), (Some("N-1"), Some("N-1")), "{from}: his chat's card gone into");
        assert!(drawn(rt, "N-1").is_some(), "{from}: its card drawn");
    };
    let left = rt.state.map.cam;
    rt.update(|s, _| {
        s.sel = Sel::Tickets;
        s.tickets.shown = Some(1);
    });
    settle(&mut rt);
    walk_press(&mut rt, "map-mark");
    into(&rt, "his chat on Threads");
    back(&mut rt);
    assert_eq!((&rt.state.sel, rt.state.tickets.shown), (&Sel::Tickets, Some(1)), "Back: his chat again");
    let pr = "paper-research".to_string();
    let k = (0, pr.clone());
    rt.update(move |s, _| {
        s.sel = Sel::Agent(pr.clone());
        s.withyou.inside.insert(k.clone(), 1);
    });
    settle(&mut rt);
    walk_press(&mut rt, "map-mark");
    into(&rt, "beside paper-research's chat");
    let on = rt.state.map.cam;
    back(&mut rt);
    assert_eq!((&rt.state.sel, rt.state.withyou.inside.get(&(0, "paper-research".into()))), (&Sel::Agent("paper-research".into()), Some(&1)), "Back: the chat, his chat beside it");
    rt.update(|s, _| s.sel = Sel::Agents);
    settle(&mut rt);
    walk_press(&mut rt, "map-mark");
    assert!(canvas::shown(&rt.state) && same_cam(rt.state.map.cam, on), "from the Agents page, the Map as he left it: {:?} at {:?}, left at {on:?}", rt.state.sel, rt.state.map.cam);
    assert!(!same_cam(left, on), "and it had moved since it opened");
}

/// paper-research N-1511 p42, 2 (his N-1104 p849: "allow travel back over the timeline … the current map basically
/// then becomes one timestep"); the owner N-1791 p1, 1: the time bar by turns and events, the Map drawn at its tick from
/// the one graph built at now, nothing built again. Its ticks are the made-up record's twelve moments, spread evenly.
/// His press on the tick of 09:30 draws the Map as by then: N-1 and N-2, no N-3 (12:00) and no link to it, the graph
/// not built again (the same one, no build in his log), the camera and the places as they were, the time and "8 of 12"
/// beside the bar, N-1's card lit (his p7 came in that tick) and N-2's not. His drag back to 07:00 draws each tick as
/// it moves, N-4 alone while it is held. ← and → step a tick; Shift with the wheel over the canvas and the wheel over the
/// bar step a tick a notch. "Now" brings the Map as it is, nothing lit. His record over ten days: a day's date at its
/// first tick, the days with nothing in them taking no room, none past the bar's end.
#[test]
fn the_time_bar_steps_the_map_by_its_ticks_and_builds_nothing() {
    let (_held, mut rt, first) = his_cloud_opened();
    let kept = rt.state.cloud.kept.clone();
    let said = |rt: &Runtime<App>| rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some("time-said")).and_then(|e| e.text);
    let drawn = |rt: &Runtime<App>, id: &str| rt.state.map.graph().is_some_and(|g| rt.state.map.shown().iter().any(|x| g.node(x.i).id == id));
    let fresh = |rt: &Runtime<App>, id: &str| rt.state.map.graph().and_then(|g| g.roots().find_map(|p| g.find_in(p, id))).is_some_and(|i| rt.state.cloud.fresh(rt.state.map.graph().unwrap().node(i)));
    let strip = |rt: &Runtime<App>| rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some("time-strip")).map(|e| e.rect).expect("the time bar drawn");
    let hm = |t: &str| docket_machine::machine::secs(&format!("2026-11-22T{t}:00Z"));
    let ticks: Vec<i64> = rt.state.cloud.ticks.iter().map(|t| t.0).collect();
    let want: Vec<i64> = ["07:00", "08:00", "08:05", "08:10", "08:12", "08:20", "09:00", "09:05", "09:30", "10:00", "11:00", "12:00"].iter().filter_map(|t| hm(t)).collect();
    assert_eq!(ticks, want, "a tick for each moment the Map changed at");
    // his press on tick K, at its middle, as the bar is drawn then ("Now" beside it makes it narrower)
    let press = |rt: &mut Runtime<App>, k: usize| {
        let r = strip(rt);
        walk_at(rt, [r[0] + r[2] * (k as f32 + 0.5) / 12.0, r[1] + r[3] / 2.0]);
    };
    assert!(drawn(&rt, "N-1") && drawn(&rt, "N-3") && said(&rt).as_deref() == Some("now"), "now: the Map whole");
    assert!(!fresh(&rt, "N-1"), "now: nothing lit");
    let graph = rt.state.map.graph().cloned().unwrap();
    let builds = || crate::log::written().iter().filter(|l| l.starts_with("cloud: built as at ")).count();
    // (his log is read away as it is read: what is in it now goes, so what is read next is what came after)
    let _ = builds();
    press(&mut rt, 8);
    assert_eq!(rt.state.cloud.at, hm("09:30"), "the tick under his press");
    assert!(drawn(&rt, "N-1") && drawn(&rt, "N-2") && !drawn(&rt, "N-3"), "by 09:30: N-1 and N-2, N-3 not yet");
    let g = rt.state.map.graph().cloned().unwrap();
    assert!(std::sync::Arc::ptr_eq(&g, &graph), "drawn from the graph built at now");
    assert_eq!(builds(), 0, "nothing built");
    let n3 = g.roots().find_map(|p| g.find_in(p, "N-3")).unwrap();
    assert!(g.edges().iter().filter(|e| e.to == n3 || e.from == n3 || g.node(e.to).parent == Some(n3)).all(|e| !g.linked(e, rt.state.cloud.at)), "no link to N-3 by then");
    assert!(same_cam(rt.state.map.cam, first), "the camera where it was: {:?}, {first:?}", rt.state.map.cam);
    assert_eq!(rt.state.cloud.kept, kept, "the places kept as they were");
    assert_eq!(said(&rt), hm("09:30").map(|t| format!("{} · 9 of 12", crate::when::secs(t))), "its time and its tick beside the bar");
    assert!(fresh(&rt, "N-1") && !fresh(&rt, "N-2"), "his p7 came in that tick: N-1 lit, N-2 not");
    // his drag back to the first tick: the Map at each tick as it moves, held
    let r = strip(&rt);
    let (x, y) = (|k: f32| r[0] + r[2] * (k + 0.5) / 12.0, r[1] + r[3] / 2.0);
    rt.pointer_move(x(8.0), y);
    rt.frame(0.016);
    rt.pointer_down(x(8.0), y);
    rt.frame(0.016);
    for k in [6.0, 3.0, 0.0] {
        rt.pointer_move(x(k), y);
        rt.frame(0.016);
    }
    assert_eq!(rt.state.cloud.at, hm("07:00"), "held at the first tick: the Map there as he drags");
    assert!(drawn(&rt, "N-4") && !drawn(&rt, "N-1"), "held at 07:00: N-4 alone");
    rt.pointer_up(x(0.0), y);
    settle(&mut rt);
    assert_eq!(rt.state.cloud.at, hm("07:00"), "let go: where he let go");
    // → and ← a tick
    rt.key(rune::KeyEvent { key: rune::Key::Right, mods: Default::default() });
    settle(&mut rt);
    assert_eq!(rt.state.cloud.at, hm("08:00"), "→: the next tick");
    assert!(drawn(&rt, "N-1"), "by 08:00 N-1 is there");
    rt.key(rune::KeyEvent { key: rune::Key::Left, mods: Default::default() });
    settle(&mut rt);
    assert_eq!(rt.state.cloud.at, hm("07:00"), "←: back a tick");
    // Shift with the wheel over the canvas: a notch a tick, two notches two; the wheel over the bar likewise
    let c = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some("map-canvas")).map(|e| e.rect).unwrap();
    let shift = rune::Mods { shift: true, ..Default::default() };
    rt.wheel_event(rune::WheelEvent::new(0.0, -2.0 * super::canvas::NOTCH).at(c[0] + c[2] / 2.0, c[1] + c[3] / 2.0).mods(shift));
    settle(&mut rt);
    assert_eq!(rt.state.cloud.at, hm("08:05"), "Shift and two notches down: two ticks on");
    let cam = rt.state.map.cam;
    let r = strip(&rt);
    // (a wheel's gesture stays with what it began over until it is still for a tenth of a second: rune's WHEEL_IDLE)
    rt.frame(0.2);
    rt.wheel_event(rune::WheelEvent::new(0.0, super::canvas::NOTCH).at(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0));
    settle(&mut rt);
    assert_eq!(rt.state.cloud.at, hm("08:00"), "the wheel over the bar, a notch up: a tick back");
    assert!(same_cam(rt.state.map.cam, cam), "the wheel over the bar does not zoom the Map");
    walk_press(&mut rt, "time-now");
    assert!(rt.state.cloud.at.is_none() && drawn(&rt, "N-3") && said(&rt).as_deref() == Some("now") && !fresh(&rt, "N-1"), "Now: the Map whole again, nothing lit");
    assert!(same_cam(rt.state.map.cam, first), "the camera still where it was");
    // his record over ten days, his last post half an hour after his last midnight: a day's date at its first tick, the
    // quiet days taking no room, none running past the bar's end under the time beside it
    let end = crate::when::next_day(docket_machine::machine::secs("2026-11-22T12:59:59Z").unwrap()).unwrap() + 1800;
    rt.update(|s, _| {
        let mut b = super::cloud::tests::base();
        for (id, t) in [("N-4.p0", "2026-11-12T07:00:00Z".to_string()), ("N-4.p2", docket_machine::machine::at_secs(end))] {
            b.slice.nodes.push(serde_json::from_value(serde_json::json!({"id": id, "kind": "post", "line": 1, "when": t, "by": "owner"})).unwrap());
            b.slice.edges.push(serde_json::from_value(serde_json::json!({"from": id, "to": "N-4", "kind": "posted_in", "line": 1, "when": t, "layer": "his"})).unwrap());
        }
        s.cloud.base = Some(Ok(b));
    });
    settle(&mut rt);
    let els = rt.elements();
    let r = els.iter().find(|e| !e.exiting && e.key.as_deref() == Some("time-strip")).map(|e| e.rect).expect("the time bar drawn");
    let dates: Vec<_> = els.iter().filter(|e| !e.exiting && e.key.as_deref().is_some_and(|k| k.starts_with("time-day-"))).map(|e| (e.text.clone(), e.rect)).collect();
    assert_eq!(dates.len(), 2, "the 22nd and the day after, at their first ticks; the 12th is the bar's start: {dates:?}");
    for (date, d) in &dates {
        assert!(d[0] + d[2] <= r[0] + r[2] + 0.5, "{date:?} at {d:?} runs past the bar's end, {r:?}");
    }
}

/// The owner's N-1791 p1, 1, as stills: research's cloud of his layer (cloud.rs `tests::base`, made up) drawn at four
/// ticks of the time bar, 07:00 (N-4 alone), 08:10 (N-1 and N-2 begun), 09:30 (N-1 lit, its p7 came then) and now (N-3
/// in, linked from both), from the one graph built at now: `time-1-0700.png` … `time-4-now.png` in STILL_OUT. The
/// sidebar folded, the places not kept, no coordd asked.
/// `STILL_OUT=DIR yaaihh-tests map::paths::time_bar_stills --ignored --exact`.
#[test]
#[ignore]
fn time_bar_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let hm = |t: &str| docket_machine::machine::secs(&format!("2026-11-22T{t}:00Z"));
    for (name, at) in [("time-1-0700", hm("07:00")), ("time-2-0810", hm("08:10")), ("time-3-0930", hm("09:30")), ("time-4-now", None)] {
        *TIME_SHOWN.lock().unwrap() = at;
        let args = vec!["--snap".to_string(), out.join(format!("{name}.png")).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1".into()];
        let spec = rune::AppSpec { poll: Some(map_poll), on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), time_shown, crate::view) };
        rune::App::new(spec).args(args).try_run().unwrap();
    }
}

/// The moment of the next still of [`time_bar_stills`].
static TIME_SHOWN: std::sync::Mutex<Option<i64>> = std::sync::Mutex::new(None);

/// The window at research's cloud as [`his_cloud_opened`] opens it, the time bar at [`TIME_SHOWN`].
fn time_shown() -> App {
    let (_held, mut s) = laid_app();
    s.projects[0].protocol = coord_types::GRAPH_PAGE_PROTOCOL;
    if s.sidebar.is_open() {
        s.sidebar.toggle();
    }
    s.cloud.of = 0;
    s.cloud.kept_read = Some(Err("a still's places are not kept".into()));
    s.cloud.base = Some(Ok(super::cloud::tests::base()));
    s.cloud.at = *TIME_SHOWN.lock().unwrap();
    s
}

/// The window as [`window`] gives it, its poll the Map's and then the structure's reveal, in lib.rs `poll`'s order
/// (map_poll alone leaves sidebar.rs `reveal` out).
fn tree_window(s: App) -> Runtime<App> {
    let mut rt = window(s);
    rt.poll = Some(tree_poll);
    rt
}

fn tree_poll(s: &mut App, clock: f64, tx: &mut Tx) -> bool {
    let map = canvas::poll(s, clock, tx);
    crate::sidebar::reveal(s, tx) | map
}

/// Frames enough for the structure to reveal what the Map is at and the sidebar to bring its row into view.
fn tree_frames(rt: &mut Runtime<App>) {
    for _ in 0..20 {
        rt.invalidate();
        rt.frame(0.05);
    }
}

/// The structure's rows drawn, top to bottom: each row's key without its "tree-" (the Map's row "map", "insights",
/// then the Map's nodes' ids and the cuts' "cut-…"), no fold button's.
fn structure_rows(rt: &Runtime<App>) -> Vec<String> {
    let mut r: Vec<(f32, String)> = rt
        .elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| {
            let id = e.key.as_deref()?.strip_prefix("tree-")?;
            (!id.starts_with("fold-")).then(|| (e.rect[1], id.to_string()))
        })
        .collect();
    r.sort_by(|a, b| a.0.total_cmp(&b.0));
    r.into_iter().map(|x| x.1).collect()
}

/// Row ID of the structure: its label, what it says after it, whether it is lit, and whether any of it is in view in
/// the sidebar's scroll.
fn structure_row(rt: &Runtime<App>, id: &str) -> Option<(String, String, bool, bool)> {
    let els = rt.elements();
    let key = format!("tree-{id}");
    let row = els.iter().find(|e| !e.exiting && e.key.as_deref() == Some(key.as_str()))?;
    let inside = format!("/{key}/");
    let part = |name: &str| els.iter().find(|e| !e.exiting && e.key.as_deref() == Some(name) && e.path.contains(&inside)).and_then(|e| e.text.clone()).unwrap_or_default();
    Some((part("label"), part("detail"), row.item_selected, row.visible[3] > 0.0))
}

/// Row ROW of the structure brought into view by the sidebar's scroll.
fn bring_row(rt: &mut Runtime<App>, row: &str) {
    let to = format!("tree-{row}");
    rt.update(move |_, tx| tx.scroll_into_view(to));
    for _ in 0..20 {
        rt.frame(0.05);
    }
}

/// Element KEY of the structure pressed, as his click does, once the sidebar has brought row ROW into view.
fn press_in_structure(rt: &mut Runtime<App>, row: &str, key: &str) {
    bring_row(rt, row);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} drawn"));
    press(rt, [x, y]);
}

/// The structure's rows drawn and where each is, top to bottom (as [`structure_rows`]).
fn structure_places(rt: &Runtime<App>) -> Vec<(String, [f32; 4])> {
    let mut r: Vec<(String, [f32; 4])> = rt
        .elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| {
            let id = e.key.as_deref()?.strip_prefix("tree-")?;
            (!id.starts_with("fold-")).then(|| (id.to_string(), e.rect))
        })
        .collect();
    r.sort_by(|a, b| a.1[1].total_cmp(&b.1[1]));
    r
}

/// Row ID of the structure pressed.
fn press_row(rt: &mut Runtime<App>, id: &str) {
    press_in_structure(rt, id, &format!("tree-{id}"));
}

/// The ids of nodes IS of G.
fn ids_of(g: &Graph, is: &[u32]) -> Vec<String> {
    is.iter().map(|i| g.node(*i).id.clone()).collect()
}

/// The explorer (paper-research N-1446 p1126; docket-edge6 p1243): while a pane draws the Map, the structure's rows
/// are the Map's nodes of the chosen project's island, nested and in the order the Map has them (map.rs
/// `Graph::children`).
/// - Folded: the Map's row, Insights, then research's topics and accounts; nothing of cav-paper's island.
/// - A topic unfolded: its lines and agents, past 40 its first block of 40, and a row saying how many more.
/// - His chat unfolded: its latest block of 40 posts, as the Map opens it at its latest (canvas.rs `fit_of`), under a
///   row saying how many earlier.
#[test]
fn while_a_pane_draws_the_map_the_structures_rows_are_its_nodes_in_its_order() {
    let (_held, s) = map_app();
    let mut rt = tree_window(s);
    settle(&mut rt);
    let g = rt.state.map.graph().unwrap().clone();
    let island = g.project("project:research").unwrap();
    rt.update(|s, _| s.streams.open.retain(|k| !k.starts_with("tree|")));
    deep_frames(&mut rt);
    let rows = structure_rows(&rt);
    assert_eq!(rows[..2], ["map".to_string(), "insights".to_string()]);
    assert_eq!(rows[2..], ids_of(&g, g.children(island))[..], "research's nodes, in the Map's order");
    // a topic: its first 40 of 150 lines and 3 agents, and how many more
    press_in_structure(&mut rt, "topic:0.3 prototype", "tree-fold-topic:0.3 prototype");
    let topic = g.find_in(island, "topic:0.3 prototype").unwrap();
    let kids = ids_of(&g, g.children(topic));
    assert!(kids.len() > 40, "{} in it", kids.len());
    let rows = structure_rows(&rt);
    let at = rows.iter().position(|r| r == "topic:0.3 prototype").expect("topic:0.3 prototype's row listed");
    assert_eq!(rows[at + 1..at + 41], kids[..40], "its first 40, in the Map's order");
    assert_eq!(rows[at + 41], "cut-topic:0.3 prototype-down");
    assert_eq!(structure_row(&rt, "cut-topic:0.3 prototype-down").unwrap().0, format!("{} more", kids.len() - 40));
    assert_eq!(rows[at + 42], ids_of(&g, g.children(island))[2], "then the next topic");
    // his chat: its latest 40 of 700 posts
    rt.update(|s, _| s.streams.open.extend(["tree|0|topic:Your chat", "tree|0|N-1104"].map(String::from)));
    deep_frames(&mut rt);
    let rows = structure_rows(&rt);
    let at = rows.iter().position(|r| r == "N-1104").expect("N-1104's row listed");
    assert_eq!(rows[at + 1], "cut-N-1104-up");
    assert_eq!(structure_row(&rt, "cut-N-1104-up").unwrap().0, "660 earlier");
    let latest: Vec<String> = (661..=700).map(|n| format!("N-1104.p{n}")).collect();
    assert_eq!(rows[at + 2..at + 42], latest[..], "its latest 40");
    assert_eq!(structure_row(&rt, "N-1104.p700").unwrap().1, "p700", "a post's row says its number");
    assert!(!rows.iter().any(|r| r.starts_with("topic:Paper")), "nothing of cav-paper's island");
}

/// A press on a row of the structure flies the Map there and selects it (p1126: "A press in the tree flies the Map
/// there"), as a search's find does (canvas.rs `fly_to_id`): a topic, his chat (flown to at its latest column, p700 in
/// the canvas), his p678.
#[test]
fn a_press_on_a_row_of_the_structure_flies_the_map_there() {
    let (_held, s) = map_app();
    let mut rt = tree_window(s);
    settle(&mut rt);
    rt.update(|s, _| s.streams.open.extend(["tree|0|topic:Your chat"].map(String::from)));
    for id in ["topic:Builds", "N-1104", "N-1104.p678"] {
        let cam = rt.state.map.cam;
        press_row(&mut rt, id);
        settle(&mut rt);
        assert_eq!(selected(&rt).as_deref(), Some(id), "{id} selected");
        assert!(rt.state.map.cam != cam, "{id}: flown to");
        let v = &rt.state.map;
        let i = v.pick(&("project:research".into(), id.into())).unwrap();
        let r = v.shown().into_iter().find(|x| x.i == i).unwrap_or_else(|| panic!("{id} drawn")).r;
        let a = v.area;
        assert!(r[0] < a[2] && r[0] + r[2] > 0.0 && r[1] < a[3] && r[1] + r[3] > 0.0, "{id} in the canvas: {r:?} {a:?}");
        if id == "N-1104" {
            let (at, _) = drawn(&rt, "N-1104.p700").expect("p700 drawn");
            assert!(at[0] > a[0] && at[0] < a[0] + a[2] && at[1] > a[1] && at[1] < a[1] + a[3], "p700 in the canvas: {at:?} {a:?}");
        }
    }
}

/// Where the Map is lights its row in the structure and opens the rows above it, the row brought into view (p1126:
/// "Moving or selecting on the Map lights and opens that node in the tree"), and a line past 40 lists the fixed block
/// holding it (proto-window N-1446 p1249 point 1):
/// - p300 of his chat selected: his chat's topic and his chat unfolded, p300's row lit and in view, its block p261 to
///   p300, counted back from p700, with a row at each cut;
/// - the cut below pressed: the Map flown to p301, the first past it, and the next block listed, p301 to p340;
/// - p320 selected on the Map: its row lit, p301's not, the block as it was, and nothing unfolded (a post holds nothing
///   here);
/// - the block's 40th row pressed: p340 selected, and no row of the structure moves;
/// - nothing selected and the camera moved by hand onto a topic: that topic lit and unfolded.
#[test]
fn where_the_map_is_lights_its_row_in_the_structure_and_opens_the_rows_above_it() {
    let (_held, s) = map_app();
    let mut rt = tree_window(s);
    settle(&mut rt);
    rt.update(|s, _| s.streams.open.retain(|k| !k.starts_with("tree|")));
    deep_frames(&mut rt);
    let g = rt.state.map.graph().unwrap().clone();
    let post = |n: u32| g.find_in(0, &format!("N-1104.p{n}")).unwrap();
    let (p300, p320) = (post(300), post(320));
    let lit = |rt: &Runtime<App>, id: &str| structure_row(rt, id).map(|r| r.2);
    // his chat's rows between its two cuts
    let block = |rows: &[String]| {
        let a = rows.iter().position(|r| r == "cut-N-1104-up").expect("a cut above");
        let b = rows.iter().position(|r| r == "cut-N-1104-down").expect("a cut below");
        rows[a + 1..b.max(a + 1)].to_vec()
    };
    let posts = |a: u32, b: u32| (a..=b).map(|n| format!("N-1104.p{n}")).collect::<Vec<_>>();
    rt.update(|s, _| canvas::select(s, Some(p300)));
    tree_frames(&mut rt);
    for open in ["topic:Your chat", "N-1104"] {
        assert!(rt.state.streams.open.contains(&format!("tree|0|{open}")), "{open} unfolded");
    }
    assert_eq!(structure_row(&rt, "N-1104.p300").map(|r| (r.1, r.2, r.3)), Some(("p300".into(), true, true)), "its row lit, in view");
    assert_eq!(block(&structure_rows(&rt)), posts(261, 300), "its block");
    assert_eq!((structure_row(&rt, "cut-N-1104-up").unwrap().0, structure_row(&rt, "cut-N-1104-down").unwrap().0), ("260 earlier".into(), "400 later".into()));
    // the cut below: the Map at p301, and the next block
    press_in_structure(&mut rt, "cut-N-1104-down", "tree-cut-N-1104-down");
    settle(&mut rt);
    tree_frames(&mut rt);
    assert_eq!(selected(&rt).as_deref(), Some("N-1104.p301"), "flown past the cut");
    let rows = structure_rows(&rt);
    assert_eq!(block(&rows), posts(301, 340), "the block p301 to p340");
    assert_eq!(lit(&rt, "N-1104.p301"), Some(true));
    // another post of the block selected on the Map: its row lit, the block as it was, nothing unfolded
    let folds = rt.state.streams.open.clone();
    rt.update(|s, _| canvas::select(s, Some(p320)));
    tree_frames(&mut rt);
    assert_eq!(rt.state.streams.open, folds, "nothing unfolded");
    assert_eq!((lit(&rt, "N-1104.p320"), lit(&rt, "N-1104.p301")), (Some(true), Some(false)), "p320 lit, p301 not");
    assert_eq!(structure_rows(&rt), rows, "the block as it was");
    // the block's 40th row pressed: no row moves
    bring_row(&mut rt, "N-1104.p340");
    let before = structure_places(&rt);
    let (x, y) = rt.find("tree-N-1104.p340").unwrap();
    press(&mut rt, [x, y]);
    settle(&mut rt);
    tree_frames(&mut rt);
    assert_eq!(selected(&rt).as_deref(), Some("N-1104.p340"));
    assert_eq!(lit(&rt, "N-1104.p340"), Some(true));
    assert_eq!(structure_places(&rt), before, "no row moved");
    // moving: nothing selected, the camera on a topic
    let t = g.find_in(0, "topic:Builds").unwrap();
    rt.update(|s, _| {
        canvas::select(s, None);
        s.map.fly_to(&g.node(t).rect, true);
    });
    tree_frames(&mut rt);
    assert_eq!(rt.state.map.here().map(|h| (h.node, h.in_view)), Some((t, true)), "the Map at the topic, in view");
    assert!(rt.state.streams.open.contains("tree|0|topic:Builds"), "it unfolded");
    assert_eq!(lit(&rt, "topic:Builds"), Some(true), "its row lit");
    assert_ne!(lit(&rt, "N-1104.p340"), Some(true), "p340's not");
}

/// A flight over the record unfolds only the node it lands on (proto-window N-1446 p1249 point 4): the node only in
/// view is revealed once the flight has landed, so the topics it passes over, each the node in view for a while, are
/// not unfolded in his kept folds.
#[test]
fn a_flight_unfolds_in_the_structure_only_the_node_it_lands_on() {
    let (_held, s) = map_app();
    let mut rt = tree_window(s);
    settle(&mut rt);
    let g = rt.state.map.graph().unwrap().clone();
    let (from, to) = (g.find_in(0, "topic:Builds").unwrap(), g.find_in(0, "topic:Claude parity").unwrap());
    rt.update(|s, _| s.map.fly_to(&g.node(from).rect, true));
    tree_frames(&mut rt);
    assert_eq!(rt.state.map.here().map(|h| h.node), Some(from), "the Map at Builds");
    let folds = rt.state.streams.open.clone();
    rt.update(|s, _| s.map.fly_to(&g.node(to).rect, false));
    let mut passed = std::collections::BTreeSet::new();
    for _ in 0..400 {
        rt.invalidate();
        rt.frame(0.016);
        if let Some(h) = rt.state.map.here() {
            passed.insert(g.node(h.node).id.clone());
        }
        if !rt.state.map.flying() {
            break;
        }
    }
    assert!(!rt.state.map.flying(), "landed");
    tree_frames(&mut rt);
    let landed = rt.state.map.here().map(|h| h.node).unwrap();
    assert_eq!(g.node(landed).id, "topic:Claude parity", "landed at Claude parity");
    assert!(passed.len() > 2, "the flight passed over other nodes: {passed:?}");
    let chain: std::collections::BTreeSet<String> = g.path(landed).into_iter().map(|a| format!("tree|0|{}", g.node(a).id)).collect();
    let opened: Vec<&String> = rt.state.streams.open.difference(&folds).collect();
    assert!(!opened.is_empty() && opened.iter().all(|k| chain.contains(*k)), "only its chain unfolded: {opened:?}");
}

/// At a depth, the lit row says it in the crumbs' words, its label then the depth, as the crumbs end with them
/// (paper-research N-1446 p1244: "p700 · Events" at the end of the open path; proto-window p1249 point 3): p700 at
/// Events, then at Record, then at Structure again.
#[test]
fn at_a_depth_the_lit_row_of_the_structure_says_it_in_the_crumbs_words() {
    let (_held, s) = depths_app();
    let mut rt = tree_window(s);
    settle(&mut rt);
    assert!(canvas::fly_to_id(&mut rt.state, 0, "N-1104"), "his chat on the Map");
    settle(&mut rt);
    let post = rt.state.map.graph().unwrap().find_in(0, "N-1104.p700").expect("his latest post laid");
    let label = rt.state.map.graph().unwrap().node(post).label.clone();
    let at = bare_for_two(&rt, post).expect("p700's row in view");
    twice(&mut rt, at);
    settle(&mut rt);
    tree_frames(&mut rt);
    let says = |rt: &Runtime<App>| structure_row(rt, "N-1104.p700").map(|r| (r.0, r.1, r.2));
    assert_eq!(says(&rt), Some((label.clone(), "p700".into(), true)), "p700 lit");
    for word in ["Events", "Record"] {
        press_depth(&mut rt, word);
        tree_frames(&mut rt);
        assert_eq!(says(&rt), Some((format!("{label} · {word}"), "p700".into(), true)), "at {word}");
        let crumbs = head_says(&rt).0;
        assert_eq!(crumbs[crumbs.len() - 3..], [label.clone(), "›".into(), word.to_string()], "the crumbs end with the same words");
    }
    press_depth(&mut rt, "Structure");
    tree_frames(&mut rt);
    assert_eq!(says(&rt), Some((label, "p700".into(), true)), "at Structure");
}

/// The Map in a pane that is not focused (consult-3 N-1446 p1245): a row's press flies that pane's Map, and the
/// focused pane stays as it was; a thread's row the same. With the Map in two panes, the one canvas is `drawn_in`'s,
/// which flies; the other pane says where the Map is.
#[test]
fn a_row_flies_the_map_in_the_pane_that_draws_it_and_the_focused_pane_stays() {
    let (_held, mut s) = map_app();
    let map = s.sel.clone();
    s.panes = vec![(0, Sel::Tickets), (0, map.clone())];
    s.focus = 0;
    s.sel = Sel::Tickets;
    s.streams.open.insert("tree|0|topic:Your chat".into());
    let mut rt = tree_window(s);
    settle(&mut rt);
    assert_eq!(canvas::drawn_in(&rt.state), Some(1), "the Map in the pane not focused");
    let stays = |rt: &Runtime<App>, id: &str| assert_eq!((rt.state.focus, rt.state.sel.clone()), (0, Sel::Tickets), "{id}: the focused pane as it was");
    for id in ["topic:Builds", "N-1104"] {
        let cam = rt.state.map.cam;
        press_row(&mut rt, id);
        settle(&mut rt);
        assert_eq!(selected(&rt).as_deref(), Some(id), "{id} selected");
        assert!(rt.state.map.cam != cam, "{id}: the Map flown");
        stays(&rt, id);
    }
    // two panes at the Map
    rt.update(|s, _| s.panes.push((0, map.clone())));
    deep_frames(&mut rt);
    assert_eq!(canvas::drawn_in(&rt.state), Some(1));
    let cam = rt.state.map.cam;
    press_row(&mut rt, "topic:Builds");
    settle(&mut rt);
    assert_eq!(selected(&rt).as_deref(), Some("topic:Builds"));
    assert!(rt.state.map.cam != cam, "the Map flown");
    stays(&rt, "two panes");
    let canvases: Vec<[f32; 4]> = rt.elements().into_iter().filter(|e| !e.exiting && e.key.as_deref() == Some("map-canvas")).map(|e| e.rect).collect();
    let elsewhere = rt.presented("map-elsewhere").expect("the other pane says where the Map is").0;
    assert_eq!(canvases.len(), 1, "one canvas");
    assert!(canvases[0][0] < elsewhere[0], "the canvas in the first pane at the Map: {canvases:?} {elsewhere:?}");
}

/// A post's row in the structure is marked as one that waits on him by what marks it so in his thread (threads.rs
/// `open_to_owner`; proto-window N-1446 p1249 point 7), not by a second test: his chat's view with p698 and p699 asking
/// him and his p700 answering p698, his chat unfolded: p699's row marked, p698's and p700's not.
#[test]
fn a_post_that_asks_him_is_marked_in_the_structure_as_in_his_thread() {
    let (_held, mut s) = map_app();
    let v: coord_types::TicketView = serde_json::from_value(serde_json::json!({"id": "N-1104", "n": 1104, "kind": "thread", "opener": "owner",
        "ctx": {}, "form": {"to": "paper-research"}, "title": "his chat", "status": {"is": "resting"}, "children": [], "program": "", "bound": 3,
        "spent": 1, "opened_at": "2026-11-12T10:00:00Z", "given": [],
        "posts": [{"n": 698, "at": "2026-11-21T09:59:00Z", "by": "paper-research", "text": "Now?", "told": ["owner"], "asks": [{"at": [0, 4]}], "asks_open": true},
                  {"n": 699, "at": "2026-11-21T10:00:00Z", "by": "paper-research", "text": "Go?", "told": ["owner"], "asks": [{"at": [0, 3]}], "asks_open": true},
                  {"n": 700, "at": "2026-11-21T10:01:00Z", "by": "owner", "text": "Yes.", "told": ["paper-research"], "answers": 698}]}))
    .unwrap();
    let open: Vec<bool> = v.posts.iter().map(|q| crate::threads::open_to_owner(&v, q)).collect();
    assert_eq!(open, [false, true, false], "p699 open to him; p698 answered");
    assert!(v.posts[0].asks_open, "p698's asks open all the same");
    s.projects[0].live.tickets.insert(v.n, v);
    let mut rt = tree_window(s);
    settle(&mut rt);
    rt.update(|s, _| {
        s.streams.open.retain(|k| !k.starts_with("tree|"));
        s.streams.open.extend(["tree|0|topic:Your chat", "tree|0|N-1104"].map(String::from));
    });
    deep_frames(&mut rt);
    let marked = |id: &str| {
        let inside = format!("/tree-{id}/");
        assert!(structure_row(&rt, id).is_some(), "{id}'s row");
        rt.elements().into_iter().any(|e| !e.exiting && e.key.as_deref() == Some("status-mark") && e.path.contains(&inside))
    };
    assert_eq!(["N-1104.p698", "N-1104.p699", "N-1104.p700"].map(marked), [false, true, false], "p699 marked, p698 and p700 not");
}

/// The structure is the chosen project's island only, and the chosen project does not follow the Map's selection
/// (proto-window N-1446 p1249 point 2): with cav-paper a project of the window too, its topic selected on the Map
/// leaves research chosen, lights no row of research's nodes and unfolds nothing.
#[test]
fn a_node_of_another_island_lights_nothing_in_the_structure() {
    let (_held, mut s) = map_app();
    let mut cav = crate::Proj::new(&crate::projects::ProjectConfig { name: "cav-paper".into(), id: "cav-paper".into(), boxed: false }, Vec::new());
    cav.down = None;
    cav.protocol = coord_types::PROTOCOL;
    s.projects.push(cav);
    let mut rt = tree_window(s);
    settle(&mut rt);
    let g = rt.state.map.graph().unwrap().clone();
    let paper = g.project("project:cav-paper").and_then(|r| g.find_in(r, "topic:Paper")).expect("cav-paper's topic on the Map");
    tree_frames(&mut rt);
    let folds = rt.state.streams.open.clone();
    rt.update(|s, _| canvas::select(s, Some(paper)));
    tree_frames(&mut rt);
    assert_eq!(selected(&rt).as_deref(), Some("topic:Paper"), "cav-paper's topic selected");
    assert_eq!(rt.state.cur, 0, "research still chosen");
    assert_eq!(rt.state.streams.open, folds, "nothing unfolded");
    let research = g.project("project:research").unwrap();
    let nodes: Vec<String> = structure_rows(&rt).into_iter().filter(|id| g.find_in(research, id).is_some()).collect();
    assert!(nodes.len() > 6, "research's nodes listed: {nodes:?}");
    let lit: Vec<&String> = nodes.iter().filter(|id| structure_row(&rt, id).is_some_and(|r| r.2)).collect();
    assert!(lit.is_empty(), "no row of research's lit: {lit:?}");
}

/// The structure is built again when the Map's graph is set again, nothing else moved (proto-window N-1446 p1249 point
/// 5: the memo key's graph, canvas.rs `graph_rev`'s `shape`): his chat unfolded, to p700; the graph set again with a
/// p701: it is listed, and the cut above says one more.
#[test]
fn a_graph_set_again_is_listed_again_in_the_structure() {
    let (_held, s) = map_app();
    let mut rt = tree_window(s);
    settle(&mut rt);
    rt.update(|s, _| {
        s.streams.open.retain(|k| !k.starts_with("tree|"));
        s.streams.open.extend(["tree|0|topic:Your chat", "tree|0|N-1104"].map(String::from));
    });
    tree_frames(&mut rt);
    assert!(structure_row(&rt, "N-1104.p700").is_some() && structure_row(&rt, "N-1104.p701").is_none(), "his chat's latest block, to p700");
    assert_eq!(structure_row(&rt, "cut-N-1104-up").map(|r| r.0), Some("660 earlier".into()));
    let g = rt.state.map.graph().unwrap().clone();
    let (chat, p700) = (g.find_in(0, "N-1104").unwrap(), g.find_in(0, "N-1104.p700").unwrap());
    let mut more = (*g).clone();
    more.push(Node::new("N-1104.p701", Kind::Post, "post 701 of N-1104", Some(chat), g.node(p700).rect)).expect("p701 inside his chat");
    rt.update(move |s, _| s.map.set_graph(more));
    tree_frames(&mut rt);
    assert!(structure_row(&rt, "N-1104.p701").is_some(), "p701 listed");
    assert_eq!(structure_row(&rt, "cut-N-1104-up").map(|r| r.0), Some("661 earlier".into()), "the cut above says one more");
}

/// The structure is built again as the deep levels grow the Map's graph in place (canvas.rs `deep_step`,
/// `Arc::make_mut`; proto-window N-1446 p1249 point 5, `graph_rev`'s node count): his p700 lit and its row unfolded
/// while it holds nothing, then the Map flown into it as a double-click does (canvas.rs `go_in`), the selection as it
/// was: the steps its run places inside it are listed under its row.
#[test]
fn the_steps_the_deep_levels_add_in_place_are_listed_in_the_structure() {
    let (_held, s) = depths_app();
    let mut rt = tree_window(s);
    settle(&mut rt);
    let g = rt.state.map.graph().unwrap().clone();
    let post = g.find_in(0, "N-1104.p700").expect("his latest post laid");
    assert!(g.children(post).is_empty(), "nothing placed in p700 yet");
    let folds: Vec<String> = g.path(post).into_iter().skip(1).map(|a| format!("tree|0|{}", g.node(a).id)).collect();
    rt.update(move |s, _| {
        s.streams.open.extend(folds);
        canvas::select(s, Some(post));
    });
    tree_frames(&mut rt);
    assert_eq!(structure_row(&rt, "N-1104.p700").map(|r| r.2), Some(true), "p700 lit");
    rt.update(move |s, _| canvas::go_in(s, post));
    settle(&mut rt);
    deep_frames(&mut rt);
    tree_frames(&mut rt);
    let g = rt.state.map.graph().unwrap().clone();
    let inside = ids_of(&g, g.children(post));
    assert!(!inside.is_empty(), "steps placed in p700");
    let rows = structure_rows(&rt);
    let at = rows.iter().position(|r| r == "N-1104.p700").expect("p700's row listed");
    assert_eq!(rows[at + 1..(at + 1 + inside.len()).min(rows.len())], inside[..], "its steps listed under its row");
}

/// [`map_app`] with the Threads lens focused in the first pane and the Map in the second, his chat's topic unfolded in
/// the structure, for a still, with no lock held.
fn tree_two_panes() -> App {
    let (_held, mut s) = map_app();
    let map = s.sel.clone();
    s.panes = vec![(0, Sel::Tickets), (0, map)];
    s.focus = 0;
    s.sel = Sel::Tickets;
    s
}

/// The structure's stills, on the made-up record, in dark (his look), the sidebar revealing as the window's poll has
/// it ([`tree_poll`]): as the Map opens, the island's rows; his p700 selected, its chain unfolded and its row lit; at
/// Events, its row in the crumbs' words; the Threads lens focused and the Map in the other pane, flown there by a
/// press on Builds' row. The points pressed are read from the same window run headless first.
#[test]
#[ignore]
fn map_tree_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let (into, events) = {
        let (_held, s) = depths_app();
        let mut rt = tree_window(s);
        let at = |rt: &Runtime<App>, id: &str| rt.state.map.graph().unwrap().find_in(0, id).unwrap_or_else(|| panic!("{id} on the Map"));
        let mut into = Vec::new();
        let mut go = |rt: &mut Runtime<App>, i: u32, what: &str| {
            let q = bare_for_two(rt, i).unwrap_or_else(|| panic!("{what} in view"));
            twice(rt, q);
            settle(rt);
            deep_frames(rt);
            into.push(q);
        };
        go(&mut rt, 0, "research");
        let chat = at(&rt, "N-1104");
        let topic = rt.state.map.graph().unwrap().node(chat).parent.unwrap();
        go(&mut rt, topic, "his chat's topic");
        go(&mut rt, chat, "his chat");
        let p700 = at(&rt, "N-1104.p700");
        go(&mut rt, p700, "his latest post");
        (into, depth_segment(&rt, "Events").0)
    };
    let twice = |t: f32, q: [f32; 2]| format!(r#"{{"t": {t}, "type": "double-click", "x": {}, "y": {}}}"#, q[0], q[1]);
    let click = |t: f32, q: [f32; 2]| format!(r#"{{"t": {t}, "type": "click", "x": {}, "y": {}}}"#, q[0], q[1]);
    let to_post = into.iter().zip([0.3, 1.8, 3.4, 5.0]).map(|(q, t)| twice(t, *q)).collect::<Vec<_>>().join(", ");
    let stills: [(&str, fn() -> App, String, &str); 4] = [
        ("tree-1-open", depths_shown, "[]".into(), "1"),
        ("tree-2-p700", depths_shown, format!("[{to_post}]"), "6.6"),
        ("tree-3-events", depths_shown, format!("[{to_post}, {}]", click(6.6, events)), "8.2"),
        ("tree-4-two-panes", tree_two_panes, r#"[{"t": 0.5, "type": "tap", "key": "tree-topic:Builds"}]"#.into(), "2.5"),
    ];
    for (name, app, script, at) in stills {
        let f = out.join(format!("{name}.json"));
        std::fs::write(&f, script).unwrap();
        let args = vec!["--snap".to_string(), out.join(format!("{name}.png")).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), at.into(), "--script".into(), f.display().to_string()];
        let spec = rune::AppSpec { poll: Some(tree_poll), on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), app, crate::view) };
        rune::App::new(spec).args(args).try_run().unwrap();
    }
}

// ---- an agent's folder (sidebar.rs, the file view: paper-research N-1446 p1441; under its row in Agents, p1633) -----

use docket_machine::graph_slice::Row as FolderRow;

const EDGE5: &str = "docket-edge5";

/// Agent NAME of the made-up project, STATE since SINCE, after SESSIONS sessions, as coordd's events give it.
fn folder_agent(name: &str, state: &str, since: &str, sessions: usize) -> coord_types::AgentView {
    serde_json::from_value(serde_json::json!({"id": format!("AGT-{name}"), "name": name, "parent": "owner", "kind": "claude", "model": "m",
        "state": state, "since": since, "tools": [], "items": [], "sessions": sessions}))
    .unwrap()
}

/// A made-up folder as coordd's `Graph::folder` gives it (nothing of his: its rows are written here): its five groups;
/// a start briefed with N-1446.p1441; CLAUDE.md in VERSIONS versions, each after the first with the words that changed
/// into it; READS files read once each; one session of three requests and one system prompt block; a commit on its
/// branch, made in its worktree.
fn made_up_folder(versions: usize, reads: usize) -> Vec<FolderRow> {
    use docket_machine::graph::Source;
    let row = |text: &str, of: &str, source: Option<Source>, rows: Vec<FolderRow>| FolderRow { text: text.into(), of: of.into(), source, rows, ..Default::default() };
    let session = |line: usize| Some(Source::Session("5c6f1a2b-made-up".into(), line));
    let commit = || Some(Source::Commit("a98874df".into()));
    let texts = (1..=versions)
        .map(|v| {
            let changed = if v == 1 { String::new() } else { "; 1 word removed, 1 added".to_string() };
            let mut r = row(&format!("version {v}: {} requests ran with it{changed}", v + 1), &format!("text:v{v}"), session(100 + v), Vec::new());
            r.when = Some(format!("2026-11-22T10:0{v}:00Z"));
            if v > 1 {
                r.words = format!("1 word removed, 1 added: a b c d [-w{}-] {{+w{v}+}} f g", v - 1);
            }
            r
        })
        .collect();
    let reads: Vec<FolderRow> = (1..=reads)
        .map(|i| FolderRow { when: Some("2026-11-22T10:30:00Z".into()), ..row(&format!("src/f{i}.rs: read 1 time"), &format!("file:src/f{i}.rs"), session(200 + i), Vec::new()) })
        .collect();
    let block = row("block 1: 1 version", "text:b1", None, vec![row("version 1: carried by 3 requests", "text:b1", session(50), Vec::new())]);
    let requests = vec![
        row("claude-opus-5-5, effort high: 3 requests, the last at 2026-11-22T10:40:00Z; 30 prompt tokens, 6 out", "request:r1", session(50), Vec::new()),
        row("system prompt blocks: 1 block", "", None, vec![block]),
    ];
    let made = vec![
        row("on edge6-folder", "branch:edge6-folder", commit(), Vec::new()),
        row("made in soft-swarm-docket-edge6-graph", "worktree:soft-swarm-docket-edge6-graph", commit(), Vec::new()),
    ];
    vec![
        row("kind and brief: 1 start", "", None, vec![row("a start of docket-edge5", "start:docket-edge5@10", Some(Source::Line(10)), vec![row("brief: N-1446.p1441", "N-1446.p1441", Some(Source::Line(10)), Vec::new())])]),
        row(&format!("loaded texts: 1 file, {versions} versions"), "", None, vec![row(&format!("CLAUDE.md: {versions} versions"), "file:CLAUDE.md", None, texts)]),
        row(&format!("read: {reads} files, {reads} reads", reads = reads.len()), "", None, reads),
        row("sessions: 1, with 3 requests", "", None, vec![row("session 5c6f1a2b-made-up: 3 requests", "session:5c6f1a2b-made-up", session(1), requests)]),
        row(
            "commits and jobs: 1 commit, 0 jobs",
            "",
            None,
            vec![row("commits: 1", "", None, vec![row("a98874df merge train 67's cut", "commit:a98874df", commit(), made)]), row("jobs: 0", "", None, Vec::new())],
        ),
    ]
}

/// The made-up project research, its bridge at PROTOCOL, Agents open, its one agent docket-edge5 idle and pinned beneath
/// Agents, where its folder hangs (his N-1104 p863: the agents' tree is the Agents page's); its pipe a sink (map/deep.rs
/// `piped`), named NAME.
#[cfg(not(windows))]
fn folder_app(name: &str, protocol: u32) -> (std::sync::MutexGuard<'static, ()>, App, std::process::Child, std::path::PathBuf) {
    let (held, mut s) = crate::view_cost::test_app();
    let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
    p.protocol = protocol;
    p.live.agents = vec![folder_agent(EDGE5, "idle", "2026-11-22T10:00:00Z", 1)];
    s.projects = vec![p];
    s.cur = 0;
    s.sel = Sel::Agents;
    s.pins.list = vec![crate::sidebar::Pin { project: "research".into(), id: format!("AGT-{EDGE5}"), agent: EDGE5.into(), thread: false, lens: None }];
    let (child, out) = super::deep::tests::piped(&mut s, 0, name);
    (held, s, child, out)
}

/// The window as [`window`] gives it, its poll the folders' (sidebar.rs `folders_poll`) and their popups'
/// (`apart_poll`) from its first frame.
fn folder_window(s: App) -> Runtime<App> {
    let mut rt = Runtime::new(s, crate::view, (1400.0, 900.0), rune::Env::new(false, 1.0));
    rt.poll = Some(folder_poll);
    rt.on_key = Some(crate::on_key);
    rt.frame(0.0);
    rt.frame(0.016);
    rt
}

fn folder_poll(s: &mut App, _: f64, _: &mut Tx) -> bool {
    crate::sidebar::folders_poll(s) | crate::sidebar::apart_poll(s)
}

fn folder_frames(rt: &mut Runtime<App>) {
    for _ in 0..6 {
        rt.invalidate();
        rt.frame(0.05);
    }
}

/// The middle of the item WORD of an open menu, if one is drawn.
fn item_at(rt: &Runtime<App>, word: &str) -> Option<(f32, f32)> {
    let els = rt.elements();
    // right of the sidebar, whose Home row is the window's
    let edge = els.iter().find(|e| e.key.as_deref() == Some("global-home")).map_or(0.0, |e| e.rect[0] + e.rect[2]);
    let e = els.into_iter().find(|e| !e.exiting && e.rect[0] >= edge && e.text.as_deref() == Some(word))?;
    Some((e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0))
}

/// Where item WORD stands in agent NAME's ⋯ (chat.rs `more_menu`): its ⋯ pressed first, as he does, while it is shut.
fn behind_more(rt: &mut Runtime<App>, name: &str, word: &str) -> (f32, f32) {
    if item_at(rt, word).is_none() {
        let (x, y) = rt.find(&format!("agent-more-{name}")).unwrap_or_else(|| panic!("{name}'s ⋯"));
        press(rt, [x, y]);
        folder_frames(rt);
    }
    item_at(rt, word).unwrap_or_else(|| panic!("{word} behind {name}'s ⋯"))
}

/// The folders asked of project 0's coordd and on their way: each its id and its agent.
fn folders_asked(rt: &Runtime<App>) -> Vec<(u64, String)> {
    let mut v: Vec<_> = rt.state.projects[0].pending.iter().filter_map(|(id, w)| if let crate::Pending::Folder { agent } = w { Some((*id, agent.clone())) } else { None }).collect();
    v.sort();
    v
}

/// Coordd's answer ROWS to the folder asked last, taken as the window takes a reply.
fn folder_answered(rt: &mut Runtime<App>, rows: Vec<FolderRow>) {
    let (id, _) = folders_asked(rt).pop().expect("a folder asked");
    let out = serde_json::to_string(&coord_types::GraphAnswer { folder: Some(rows), ..Default::default() }).unwrap();
    rt.update(|s: &mut App, _| {
        s.take(0, crate::link::Incoming::Reply(id, coord_types::Reply { ok: true, out, post: None }));
    });
    folder_frames(rt);
}

/// The rows of agent NAME's folder drawn, top to bottom, by their ids after the agent's own; a cut's as `cut ID-up`.
fn folder_drawn(rt: &Runtime<App>, name: &str) -> Vec<String> {
    let own = format!("agent:{name}|");
    structure_rows(rt)
        .into_iter()
        .filter_map(|id| match id.strip_prefix("cut-") {
            Some(cut) => cut.strip_prefix(&own).map(|c| format!("cut {c}")),
            None => id.strip_prefix(&own).map(str::to_string),
        })
        .collect()
}

/// The words of the row keyed KEY, wherever it is.
fn row_label(rt: &Runtime<App>, key: &str) -> Option<String> {
    let inside = format!("/{key}/");
    rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some("label") && e.path.contains(&inside)).and_then(|e| e.text)
}

/// The folder asks among what was sent down the pipe of `deep::tests::piped` (the window asks other things on it too).
#[cfg(not(windows))]
fn folders_sent(child: std::process::Child, out: &std::path::Path) -> Vec<serde_json::Value> {
    super::deep::tests::sent(child, out).into_iter().filter(|v| v.to_string().contains(r#""folder":"#)).collect()
}

/// The text of the element keyed KEY.
fn keyed_text(rt: &Runtime<App>, key: &str) -> Option<String> {
    rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).and_then(|e| e.text)
}

/// Row ID of docket-edge5's folder unfolded by its fold button, once in view.
fn unfold_in_folder(rt: &mut Runtime<App>, id: &str) {
    let row = format!("agent:{EDGE5}|{id}");
    press_in_structure(rt, &row, &format!("tree-fold-{row}"));
    folder_frames(rt);
    let place = |key: String| rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key.as_str())).map(|e| (e.rect, e.visible));
    let (fold, own, side) = (place(format!("tree-fold-{row}")), place(format!("tree-{row}")), place("sidebar".to_string()));
    assert!(rt.state.streams.open.contains(&format!("tree|0|{row}")), "{id} unfolded: its fold at {fold:?}, its row at {own:?}, the sidebar at {side:?}");
}

/// docket-edge5's row unfolded, its folder answered with ROWS, and its loaded texts and CLAUDE.md unfolded.
fn folder_at_claude_md(rt: &mut Runtime<App>, rows: Vec<FolderRow>) {
    let (x, y) = rt.find("agent-fold-docket-edge5").expect("docket-edge5's row has a fold");
    press(rt, [x, y]);
    folder_frames(rt);
    folder_answered(rt, rows);
    unfold_in_folder(rt, "loaded texts");
    unfold_in_folder(rt, "loaded texts|file:CLAUDE.md");
}

const V2: &str = "agent:docket-edge5|loaded texts|file:CLAUDE.md|text:v2";

/// The file view's stills (paper-research N-1446 p1441; p1530: research's copy of the real docket-edge5, through a
/// stand-in coordd), 1400 × 900, the sidebar 460 wide, from FOLDER_ANSWER, coordd's answer to docket-edge5's folder
/// (`{"ask", "answer"}` as `a_coords_record_graph_answers` writes it): his row under Agents unfolded into its five
/// groups, its kind and brief unfolded (folder-groups); its loaded texts and the file of the most versions
/// (folder-texts); what it read (folder-read); its sessions, the first and its system prompt blocks, each unfolded
/// (folder-session); its commits and jobs, the first commit unfolded (folder-commits); and the latest version drawn in
/// view whose words both remove and add, pressed (folder-words: the script presses by key, and a row below the
/// sidebar's edge takes no press, p1681). With REQUESTS_ANSWER, coordd's answer to the requests of its first session
/// (`Ask::session`, p1642, 3): that session's requests apart (folder-requests), and its last block's latest version
/// apart, a made-up text in place of the block's, which is his, read on his press (folder-block). The project's
/// snapshot and the agent's row (idle) are written here; the rows under it are coordd's. Nothing is asked of a coordd:
/// the window's projects file names no project, and there is no poll.
/// `STILL_OUT=DIR FOLDER_ANSWER=FILE [REQUESTS_ANSWER=FILE] yaaihh-tests map::paths::folder_stills --ignored --exact
/// --nocapture`.
#[test]
#[ignore]
fn folder_stills() {
    let (Ok(out), Ok(answer)) = (std::env::var("STILL_OUT"), std::env::var("FOLDER_ANSWER")) else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    // view_cost's test_app's folder, whose projects.toml the window reads at its start
    let own = std::env::temp_dir().join(format!("yaaihh-view-cost-{}", std::process::id()));
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(own.join("projects.toml"), "# the file view's stills: no project, nothing asked of a coordd\n").unwrap();
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&answer).unwrap()).unwrap();
    let rows = serde_json::from_value::<coord_types::GraphAnswer>(v["answer"].clone()).unwrap().folder.expect("a folder in the answer");
    let top = docket_machine::graph::agent_id(EDGE5);
    // the rows under PARENT with their ids (sidebar.rs `row_ids`)
    let ids = |parent: &str, rows: &[FolderRow]| -> Vec<(String, FolderRow)> { crate::sidebar::row_ids(parent, rows).into_iter().zip(rows.iter().cloned()).collect() };
    let group = |head: &str| ids(&top, &rows).into_iter().find(|(_, r)| r.text.split(": ").next() == Some(head)).unwrap_or_else(|| panic!("a group {head}"));
    let fold = |id: &str| format!("tree|0|{id}");
    let (kind, texts, read, sessions, made) = (group("kind and brief"), group("loaded texts"), group("read"), group("sessions"), group("commits and jobs"));
    let mut under_kind = vec![fold(&kind.0)];
    under_kind.extend(ids(&kind.0, &kind.1.rows).iter().map(|(id, _)| fold(id)));
    let file = ids(&texts.0, &texts.1.rows).into_iter().max_by_key(|(_, r)| r.rows.len()).expect("a file loaded");
    let session = ids(&sessions.0, &sessions.1.rows).into_iter().next().expect("a session");
    let mut under_session = vec![fold(&sessions.0), fold(&session.0)];
    for (id, r) in ids(&session.0, &session.1.rows).into_iter().filter(|(_, r)| !r.rows.is_empty()) {
        under_session.push(fold(&id));
        under_session.extend(ids(&id, &r.rows).iter().filter(|(_, b)| !b.rows.is_empty()).map(|(id, _)| fold(id)));
    }
    let commits = ids(&made.0, &made.1.rows).into_iter().find(|(_, r)| r.text.starts_with("commits")).expect("its commits");
    let first = ids(&commits.0, &commits.1.rows).into_iter().next().expect("a commit");
    let listed: Vec<(String, FolderRow)> = ids(&file.0, &file.1.rows).into_iter().take(40).collect();
    let both = |r: &FolderRow| r.words.contains("[-") && r.words.contains("{+");
    let texts_open = vec![fold(&texts.0), fold(&file.0)];
    let shown = |open: &[String]| FolderShown { rows: rows.clone(), open: open.to_vec(), ..Default::default() };
    // the versions drawn in view with the file unfolded: the script presses by key, with no scroll
    *FOLDER_SHOWN.lock().unwrap() = Some(shown(&texts_open));
    let in_view = drawn_in_view(folder_shown());
    let drawn: Vec<&(String, FolderRow)> = listed.iter().filter(|(id, _)| in_view.contains(&format!("tree-{id}"))).collect();
    let version = drawn.iter().rev().find(|(_, r)| both(r)).or_else(|| drawn.iter().rev().find(|(_, r)| !r.words.is_empty())).expect("a version with words drawn in view").0.clone();
    let click = format!(r#"[{{"t": 0.3, "type": "click", "key": {}}}]"#, serde_json::to_string(&format!("tree-{version}")).unwrap());
    let script = out.join("folder-words.json");
    std::fs::write(&script, click).unwrap();
    let mut scenes: Vec<(&str, FolderShown, Option<String>)> = vec![
        ("folder-groups", shown(&under_kind), None),
        ("folder-texts", shown(&texts_open), None),
        ("folder-read", shown(&[fold(&read.0)]), None),
        ("folder-session", shown(&under_session), None),
        ("folder-commits", shown(&[fold(&made.0), fold(&commits.0), fold(&first.0)]), None),
        ("folder-words", shown(&texts_open), Some(script.display().to_string())),
    ];
    if let Ok(requests) = std::env::var("REQUESTS_ANSWER") {
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&requests).unwrap()).unwrap();
        let asked = v["ask"]["session"].as_str().expect("a session asked").to_string();
        let list = serde_json::from_value::<coord_types::GraphAnswer>(v["answer"].clone()).unwrap().folder.expect("its requests in the answer");
        let (sid, srow) = ids(&sessions.0, &sessions.1.rows).into_iter().find(|(_, r)| r.of == asked).expect("the session asked in the folder");
        let what = crate::popup::What::Requests { pi: 0, agent: EDGE5.into(), id: sid.clone(), session: asked.clone() };
        scenes.push(("folder-requests", FolderShown { popup: Some(what), requests: Some((asked.clone(), list)), ..shown(&under_session) }, None));
        // its last block's latest version
        let blocks = ids(&sid, &srow.rows).into_iter().find(|(_, r)| r.text.starts_with("system prompt blocks")).expect("its blocks");
        let block = ids(&blocks.0, &blocks.1.rows).into_iter().last().expect("a block");
        let (vid, latest) = ids(&block.0, &block.1.rows).into_iter().last().expect("a version");
        let what = crate::popup::What::Folder { pi: 0, agent: EDGE5.into(), id: vid };
        // the words that changed into it made up too: they are his block's words as much as its text is
        let mut held = shown(&under_session);
        made_up_words(&mut held.rows, &latest.of);
        scenes.push(("folder-block", FolderShown { popup: Some(what), text: Some((latest.of, MADE_UP_BLOCK.into())), ..held }, None));
    }
    println!("the file view's stills of {answer}: file {}, session {}, commit {}, version {version}", file.0, session.0, first.0);
    for (name, shown, script) in scenes {
        *FOLDER_SHOWN.lock().unwrap() = Some(shown);
        let mut args = vec!["--snap".to_string(), out.join(format!("{name}.png")).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1".into()];
        if let Some(s) = script {
            args.extend(["--script".to_string(), s]);
        }
        let spec = rune::AppSpec { on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), folder_shown, crate::view) };
        rune::App::new(spec).args(args).try_run().unwrap();
    }
}

/// The text folder-block shows in place of the block's, which is his (p1642, 2: read on his press, never kept).
const MADE_UP_BLOCK: &str = "Made up for the still: a system prompt block's text is his, read from its session's file on his press, never kept. In his window its words stand here, whole.";

/// The words that changed into the rows of ROWS (and those under them) of text node OF, made up for folder-block as
/// [`MADE_UP_BLOCK`] is, after coordd's counts (`words_head`), which are kept.
fn made_up_words(rows: &mut [FolderRow], of: &str) {
    for r in rows {
        if r.of == of && !r.words.is_empty() {
            // its counts, where it has them before its words (else `words_head` is the words themselves)
            let head = Some(docket_machine::graph_slice::words_head(&r.words)).filter(|h| h.len() < r.words.len()).unwrap_or("its words").to_string();
            r.words = format!("{head}: … the words before them {{+made up for the still: the words that changed in his block stand here+}} the words after them …");
        }
        made_up_words(&mut r.rows, of);
    }
}

/// The keys of what APP draws whole in view at the stills' size, after its first frames: a row its scroll cuts takes
/// the script's press at its middle, which may be out of view.
fn drawn_in_view(app: App) -> std::collections::HashSet<String> {
    let mut rt = Runtime::new(app, crate::view, (1400.0, 900.0), rune::Env::new(false, 1.0));
    rt.frame(0.0);
    rt.frame(0.016);
    rt.elements().into_iter().filter(|e| !e.exiting && e.rect[3] > 0.0 && e.visible[3] + 0.5 >= e.rect[3]).filter_map(|e| e.key).collect()
}

/// What the next still of [`folder_stills`] shows: the folder coordd gave, the folds unfolded in it, a popup open over
/// it, and what that popup shows that coordd gives apart: a block's text by its node, a session's requests.
#[derive(Clone, Default)]
struct FolderShown {
    rows: Vec<FolderRow>,
    open: Vec<String>,
    popup: Option<crate::popup::What>,
    text: Option<(String, String)>,
    requests: Option<(String, Vec<FolderRow>)>,
}
static FOLDER_SHOWN: std::sync::Mutex<Option<FolderShown>> = std::sync::Mutex::new(None);

/// The window at Agents, research's coordd serving the folder and what it holds apart (`TEXTS_PROTOCOL`),
/// docket-edge5 pinned, its row unfolded and its folder held as [`FOLDER_SHOWN`] gives it, as coordd's answers are held once
/// they have come, its popup open, titled as his press titles it (sidebar.rs `apart_title`).
fn folder_shown() -> App {
    let (_held, mut s) = crate::view_cost::test_app();
    let shown = FOLDER_SHOWN.lock().unwrap().clone().expect("a folder to show");
    let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
    p.protocol = coord_types::TEXTS_PROTOCOL;
    p.down = None;
    p.live.agents = vec![folder_agent(EDGE5, "idle", "2026-11-22T14:47:10Z", 1)];
    s.projects = vec![p];
    s.cur = 0;
    s.sel = Sel::Agents;
    s.pins.list = vec![crate::sidebar::Pin { project: "research".into(), id: format!("AGT-{EDGE5}"), agent: EDGE5.into(), thread: false, lens: None }];
    s.sidebar.set_width(460.0);
    s.streams.open.insert(format!("tree|0|{}", docket_machine::graph::agent_id(EDGE5)));
    s.streams.open.extend(shown.open);
    s.folders.got.insert((0, EDGE5.to_string()), (String::new(), Ok(shown.rows)));
    if let Some((session, list)) = shown.requests {
        s.folders.requests.insert((0, EDGE5.to_string(), session), (String::new(), Ok(list)));
    }
    if let Some((node, text)) = shown.text {
        s.folders.texts.insert((0, node), Ok(text));
    }
    if let Some(what) = shown.popup {
        let title = crate::sidebar::apart_title(&s, &what);
        crate::popup::open(&mut s, title, what);
    }
    s
}

/// paper-research N-1446 p1441, p1633 (docket-edge6's p1631, pick 1): where research's coordd gives an agent's folder,
/// docket-edge5's row under Agents has a fold, and nothing is asked while it is folded. Unfolded, its folder is asked
/// once over the frames, and a row says so until coordd answers; then its five groups lie under it, folded, each
/// saying its counts. A group unfolded lists its rows, each saying where it comes from: a version its session's line,
/// a commit its hash, the commit's worktree as made in it.
#[test]
#[cfg(not(windows))]
fn an_agents_row_unfolds_into_its_folder_asked_once_its_rows_saying_their_sources() {
    let (_held, s, child, out) = folder_app("folder-1", coord_types::FOLDER_PROTOCOL);
    let mut rt = folder_window(s);
    folder_frames(&mut rt);
    assert!(folders_asked(&rt).is_empty(), "nothing asked while it is folded");
    let (x, y) = rt.find("agent-fold-docket-edge5").expect("docket-edge5's row has a fold");
    press(&mut rt, [x, y]);
    folder_frames(&mut rt);
    assert_eq!(folders_asked(&rt).iter().map(|x| x.1.as_str()).collect::<Vec<_>>(), [EDGE5], "asked once over the frames");
    assert_eq!(row_label(&rt, "folder-note-docket-edge5").as_deref(), Some("loading…"));
    folder_answered(&mut rt, made_up_folder(2, 3));
    assert_eq!(folder_drawn(&rt, EDGE5), ["kind and brief", "loaded texts", "read", "sessions", "commits and jobs"], "the five groups, folded");
    assert_eq!(row_label(&rt, "folder-note-docket-edge5"), None, "the note gone with the answer");
    assert_eq!(structure_row(&rt, "agent:docket-edge5|read").map(|r| r.0), Some("read: 3 files, 3 reads".to_string()));
    unfold_in_folder(&mut rt, "loaded texts");
    unfold_in_folder(&mut rt, "loaded texts|file:CLAUDE.md");
    assert_eq!(structure_row(&rt, V2).map(|r| (r.0, r.1)), Some(("version 2: 3 requests ran with it; 1 word removed, 1 added".to_string(), "5c6f1a2b:102".to_string())));
    unfold_in_folder(&mut rt, "commits and jobs");
    unfold_in_folder(&mut rt, "commits and jobs|commits");
    unfold_in_folder(&mut rt, "commits and jobs|commits|commit:a98874df");
    let commit = "agent:docket-edge5|commits and jobs|commits|commit:a98874df";
    assert_eq!(structure_row(&rt, commit).map(|r| (r.0, r.1)), Some(("a98874df merge train 67's cut".to_string(), "commit a98874df".to_string())));
    let made = format!("{commit}|worktree:soft-swarm-docket-edge6-graph");
    assert_eq!(structure_row(&rt, &made).map(|r| r.0), Some("made in soft-swarm-docket-edge6-graph".to_string()), "{:?}", folder_drawn(&rt, EDGE5));
    assert!(folders_asked(&rt).is_empty(), "nothing more asked");
    rt.state.projects[0].link = None;
    let sent = folders_sent(child, &out);
    assert_eq!(sent.len(), 1, "one ask in all: {sent:?}");
    assert!(sent[0].to_string().contains(r#""folder":"docket-edge5""#), "{sent:?}");
}

/// p1682, 3 (a): a file's row cuts its path from its start by whole folders, keeping its end, so that its words are
/// drawn whole beside where they come from and one folder more would not be, at each width of the sidebar from 400 to
/// 520 in steps of 4 (each folder's cut falls within some step: a room off by a few points shows); where nothing more
/// than its name fits, its name is kept; its description says the whole path and its time (p1729); a short path is
/// whole.
#[test]
#[cfg(not(windows))]
fn a_files_path_is_cut_from_its_start_to_fit_its_row_and_said_whole() {
    let deep = "/home/t/Documents/papers/soft-swarm-docket-edge5-scale2/yaaihh/crates/yaaihh/src/picture/docket.rs";
    let mut rows = made_up_folder(2, 1);
    let mut far = rows[2].rows[0].clone();
    (far.text, far.of) = (format!("{deep}: read 9 times"), format!("file:{deep}"));
    rows[2].rows.insert(0, far);
    let (_held, s, _child, _out) = folder_app("folder-cut", coord_types::FOLDER_PROTOCOL);
    let mut rt = folder_window(s);
    folder_frames(&mut rt);
    let (x, y) = rt.find("agent-fold-docket-edge5").expect("docket-edge5's row has a fold");
    press(&mut rt, [x, y]);
    folder_frames(&mut rt);
    folder_answered(&mut rt, rows);
    unfold_in_folder(&mut rt, "read");
    let k = rune_kit::Kit::neutral(true);
    let row = format!("tree-agent:{EDGE5}|read|file:{deep}");
    for wide in (400..=520).step_by(4).map(|w| w as f32) {
        rt.update(move |s: &mut App, _| s.sidebar.set_width(wide));
        folder_frames(&mut rt);
        let inside = format!("/{row}/");
        let label = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some("label") && e.path.contains(&inside)).unwrap_or_else(|| panic!("its label at {wide}"));
        let text = label.text.clone().unwrap_or_default();
        let drawn = rune::text::measure(&text, &[], k.ty.small, None).0;
        assert!(text.starts_with("…/") && text.ends_with("/docket.rs: read 9 times"), "cut from its start, its name kept, at {wide}: {text}");
        let rects: Vec<(String, [f32; 4])> = rt.elements().into_iter().filter(|e| !e.exiting && (["sidebar", "sidebar-scroll", "sidebar-list", row.as_str()].contains(&e.key.as_deref().unwrap_or("")) || (e.path.contains(&inside) && e.key.as_deref().is_some_and(|k| ["label", "row-end", "detail"].contains(&k))))).map(|e| (e.key.unwrap_or_default(), e.rect)).collect();
        assert!(drawn <= label.rect[2] + 0.5, "drawn whole at {wide}: {text} is {drawn} wide, its label {}; {rects:?}", label.rect[2]);
        // the cut one folder longer would not be drawn whole, give or take the room's 4 points and as many again
        let kept = text.trim_start_matches('…').trim_end_matches(": read 9 times");
        let longer = deep[..deep.len() - kept.len()].rfind('/').map(|i| format!("…{}: read 9 times", &deep[i..]));
        assert!(longer.is_none_or(|t| rune::text::measure(&t, &[], k.ty.small, None).0 > label.rect[2] - 8.0), "no more cut than it needs at {wide}: {text}, its label {}", label.rect[2]);
        assert_eq!(said_of(&rt.accessibility_json(), &row).as_deref(), Some(format!("{deep}: read 9 times, 5c6f1a2b:201, at 2026-11-22T10:30:00Z").as_str()), "said whole, with its time, at {wide}");
    }
    // 300 wide and the project's own indent under its row in Projects (sidebar.rs `UNFOLD_PAD`)
    rt.update(|s: &mut App, _| s.sidebar.set_width(312.0));
    folder_frames(&mut rt);
    assert_eq!(row_label(&rt, &row).as_deref(), Some("…/docket.rs: read 9 times"), "its name kept where nothing more fits");
    assert_eq!(structure_row(&rt, "agent:docket-edge5|read|file:src/f1.rs").map(|r| r.0).as_deref(), Some("src/f1.rs: read 1 time"), "a short path whole");
    assert_eq!(said_of(&rt.accessibility_json(), "tree-agent:docket-edge5|read|file:src/f1.rs").as_deref(), Some("5c6f1a2b:201, at 2026-11-22T10:30:00Z"), "where it comes from and its time");
}

/// The description of the element keyed KEY in the window's accessibility tree V (rune `accessibility_json`).
fn said_of(v: &serde_json::Value, key: &str) -> Option<String> {
    if v["key"].as_str() == Some(key) {
        return v["description"].as_str().map(String::from);
    }
    v["children"].as_array()?.iter().find_map(|c| said_of(c, key))
}

/// p1633: the tree shows rows and counts only, and a version's words open apart: a press on version 2's row opens
/// popup.rs's panel, titled with the agent, the file and the version, the words without their marks under where they
/// come from; nothing of them is drawn in the sidebar.
#[test]
#[cfg(not(windows))]
fn a_versions_press_opens_its_words_apart() {
    let (_held, s, _child, _out) = folder_app("folder-2", coord_types::FOLDER_PROTOCOL);
    let mut rt = folder_window(s);
    folder_frames(&mut rt);
    folder_at_claude_md(&mut rt, made_up_folder(2, 3));
    assert!(rt.elements().iter().all(|e| e.text.as_deref() != Some("a b c d w1 w2 f g")), "no words in the tree");
    press_in_structure(&mut rt, V2, &format!("tree-{V2}"));
    folder_frames(&mut rt);
    assert_eq!(rt.state.popups.len(), 1, "one popup");
    assert_eq!(rt.state.popups[0].what, crate::popup::What::Folder { pi: 0, agent: EDGE5.into(), id: V2.into() });
    assert_eq!(rt.state.popups[0].title, "docket-edge5 · CLAUDE.md, version 2 · the words that changed");
    assert_eq!(keyed_text(&rt, &format!("folder-words-{V2}")).as_deref(), Some("a b c d w1 w2 f g"));
    assert!(in_popup(&rt, "from 5c6f1a2b:102, at 2026-11-22T10:02:00Z"), "where its words come from");
    assert!(in_popup(&rt, "version 2: 3 requests ran with it; 1 word removed, 1 added"), "its counts above them");
}

/// The folder is asked again when what its agent does moves (sidebar.rs `folder_mark`): a tool call asks nothing; its
/// turn's start asks once, and the rows held stay drawn until the answer comes; then version 3 lies under CLAUDE.md,
/// which stays unfolded, as do the groups he unfolded.
#[test]
#[cfg(not(windows))]
fn a_folder_is_asked_again_when_its_agent_moves_and_keeps_his_folds() {
    let (_held, s, child, out) = folder_app("folder-3", coord_types::FOLDER_PROTOCOL);
    let mut rt = folder_window(s);
    folder_frames(&mut rt);
    folder_at_claude_md(&mut rt, made_up_folder(2, 3));
    let versions = |rt: &Runtime<App>| folder_drawn(rt, EDGE5).into_iter().filter_map(|x| x.strip_prefix("loaded texts|file:CLAUDE.md|").map(str::to_string)).collect::<Vec<_>>();
    assert_eq!(versions(&rt), ["text:v1", "text:v2"]);
    rt.state.projects[0].live.agents[0].last_tool = Some("Read".into());
    folder_frames(&mut rt);
    assert!(folders_asked(&rt).is_empty(), "a tool call asks nothing");
    rt.state.projects[0].live.agents[0] = folder_agent(EDGE5, "working", "2026-11-22T10:50:00Z", 1);
    folder_frames(&mut rt);
    assert_eq!(folders_asked(&rt).len(), 1, "its turn's start asks once");
    assert_eq!(versions(&rt), ["text:v1", "text:v2"], "the rows held drawn meanwhile");
    folder_answered(&mut rt, made_up_folder(3, 3));
    assert_eq!(versions(&rt), ["text:v1", "text:v2", "text:v3"], "version 3 under CLAUDE.md, still unfolded");
    assert!(folders_asked(&rt).is_empty(), "nothing more asked");
    rt.state.projects[0].link = None;
    assert_eq!(folders_sent(child, &out).len(), 2, "two asks in all");
}

/// Below the folder's protocol an agent's row has no fold, and nothing is asked even with its fold kept unfolded from
/// a window before: the rows would never come.
#[test]
#[cfg(not(windows))]
fn below_the_folders_protocol_an_agents_row_has_no_fold_and_nothing_is_asked() {
    let (_held, mut s, child, out) = folder_app("folder-4", coord_types::FOLDER_PROTOCOL - 1);
    s.streams.open.insert(format!("tree|0|agent:{EDGE5}"));
    let mut rt = folder_window(s);
    folder_frames(&mut rt);
    assert!(rt.find("agent-fold-docket-edge5").is_none(), "no fold");
    assert!(rt.find("pin-research-AGT-docket-edge5").is_some(), "its row as before");
    assert!(folders_asked(&rt).is_empty() && folder_drawn(&rt, EDGE5).is_empty() && row_label(&rt, "folder-note-docket-edge5").is_none());
    rt.state.projects[0].link = None;
    assert!(folders_sent(child, &out).is_empty(), "no folder asked");
}

/// Fix 4's presses (accounts-lead N-1446 p2446; p2452, read right p2455): on an agent's page, the agent not pinned,
/// System prompt, Initial prompt and Home beside Raw log, from a coordd that gives an agent's folder. A press opens its
/// lens in a pane beside the page, focused, as Raw log opens its pane; the agent's folder is asked for the pane though no
/// row of it is unfolded; and each draws only the part of the folder that holds it, until docket-edge6's views take
/// them (p2496): each session's system prompt blocks; its starts with their briefs; the texts it loaded and the files
/// it read.
#[test]
#[cfg(not(windows))]
fn an_agents_system_prompt_initial_prompt_and_home_open_beside_its_page_with_no_pin() {
    let (_held, mut s, child, out) = folder_app("folder-views", coord_types::FOLDER_PROTOCOL - 1);
    s.pins.list.clear();
    s.sel = Sel::Agent(EDGE5.into());
    let mut rt = folder_window(s);
    folder_frames(&mut rt);
    let presses = ["System prompt", "Initial prompt", "Home"];
    // its menu opened behind its ⋯
    let (x, y) = rt.find(&format!("agent-more-{EDGE5}")).expect("its ⋯");
    press(&mut rt, [x, y]);
    folder_frames(&mut rt);
    assert!(item_at(&rt, "Raw log").is_some() && presses.iter().all(|w| item_at(&rt, w).is_none()), "a coordd before the folder's: none");
    rt.update(|s, _| s.projects[0].protocol = coord_types::FOLDER_PROTOCOL);
    folder_frames(&mut rt);
    assert!(presses.iter().all(|w| item_at(&rt, w).is_some()), "under Raw log, with no pin");
    assert!(folders_asked(&rt).is_empty(), "nothing asked before a press");
    let (x, y) = behind_more(&mut rt, EDGE5, "System prompt");
    press(&mut rt, [x, y]);
    folder_frames(&mut rt);
    let lens = |f: Filter| Sel::Lens(Lens::of(f));
    let prompt = lens(Filter::SystemPrompt { agent: EDGE5.into() });
    assert_eq!((rt.state.panes.len(), rt.state.focus, &rt.state.sel), (2, 1, &prompt), "in a pane beside, focused");
    assert_eq!(rt.state.panes[0], (0, Sel::Agent(EDGE5.into())), "the agent's page beside it");
    // the page is half the main area now: its ⋯ whole inside its pane, the menu shut by the pick
    let rect = |rt: &Runtime<App>, key: &str| rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).map(|e| e.rect);
    let (page, dots) = (rect(&rt, &crate::panes::key(0)).expect("the page's pane"), rect(&rt, &format!("agent-more-{EDGE5}")).expect("its ⋯"));
    assert!(dots[0] >= page[0] && dots[0] + dots[2] <= page[0] + page[2], "its ⋯ inside its pane: {dots:?} in {page:?}");
    assert!(item_at(&rt, "Raw log").is_none(), "the pick shut the menu");
    assert_eq!(folders_asked(&rt).iter().map(|x| x.1.as_str()).collect::<Vec<_>>(), [EDGE5], "its folder asked, no row of it unfolded");
    folder_answered(&mut rt, made_up_folder(2, 3));
    let session = "sessions|session:5c6f1a2b-made-up|system prompt blocks";
    assert_eq!(folder_drawn(&rt, EDGE5), [format!("{session}|text:b1")], "each session's system prompt blocks");
    assert_eq!(keyed_text(&rt, &format!("part-agent:{EDGE5}|{session}")).as_deref(), Some("session 5c6f1a2b-made-up: 3 requests · system prompt blocks: 1 block"));
    // the other two, shown in that pane
    rt.update(|s, _| s.show_sel(lens(Filter::InitialPrompt { agent: EDGE5.into() })));
    folder_frames(&mut rt);
    assert_eq!(folder_drawn(&rt, EDGE5), ["harness"], "its starts behind what the harness adds (p2987)");
    press_in_view(&mut rt, &format!("tree-fold-agent:{EDGE5}|harness"));
    assert_eq!(folder_drawn(&rt, EDGE5), ["harness", "kind and brief|start:docket-edge5@10"], "its starts");
    rt.update(|s, _| s.show_sel(lens(Filter::AgentHome { agent: EDGE5.into() })));
    folder_frames(&mut rt);
    assert_eq!(folder_drawn(&rt, EDGE5), ["loaded texts|file:CLAUDE.md", "read|file:src/f1.rs", "read|file:src/f2.rs", "read|file:src/f3.rs"], "the texts it loaded, the files it read");
    assert!(folders_asked(&rt).is_empty(), "the folder held is the three's");
    rt.state.projects[0].link = None;
    assert_eq!(folders_sent(child, &out).len(), 1, "one folder asked");
}

/// p3041, 5 (proto-window's walk p3038, 5): his path to the Map beside an agent's page. System prompt opened beside
/// the page, then the Map gone to from it in that pane, as its "› on the Map" goes; the Map's pane focused.
#[cfg(not(windows))]
fn map_beside_agent(name: &str) -> (std::sync::MutexGuard<'static, ()>, Runtime<App>, std::process::Child, std::path::PathBuf) {
    let (held, mut s, child, out) = folder_app(name, coord_types::FOLDER_PROTOCOL);
    s.pins.list.clear();
    s.sel = Sel::Agent(EDGE5.into());
    let mut rt = folder_window(s);
    folder_frames(&mut rt);
    let (x, y) = behind_more(&mut rt, EDGE5, "System prompt");
    press(&mut rt, [x, y]);
    folder_frames(&mut rt);
    assert_eq!((rt.state.panes.len(), rt.state.focus), (2, 1), "System prompt in a pane beside the page");
    rt.update(|s, _| s.show_sel(Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Cloud, at: None })));
    folder_frames(&mut rt);
    assert!(canvas::shown(&rt.state), "the Map in System prompt's pane");
    (held, rt, child, out)
}

/// p3041, 5: in half the window the Map's bar wraps, each of its presses whole inside the Map's pane; none is drawn
/// past its edge ([`map_beside_agent`]).
#[test]
#[cfg(not(windows))]
fn the_maps_bar_wraps_in_half_the_window_each_press_whole_inside_its_pane() {
    let (_held, rt, _child, _out) = map_beside_agent("folder-bar-wraps");
    let rect = |key: &str| rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).map(|e| e.rect);
    let pane = rect(&crate::panes::key(1)).expect("the Map's pane");
    let switches = docket_machine::graph_slice::Layer::ALL.into_iter().filter(|l| *l != docket_machine::graph_slice::Layer::His).map(|l| format!("cloud-switch-{l:?}"));
    let bar: Vec<String> = ["cloud-presets".to_string()].into_iter().chain(switches).chain(["cloud-home".into(), "cloud-again".into()]).collect();
    for key in &bar {
        let r = rect(key).unwrap_or_else(|| panic!("{key} drawn"));
        assert!(r[0] >= pane[0] && r[0] + r[2] <= pane[0] + pane[2] + 0.5, "{key} whole inside the Map's pane: {r:?} in {pane:?}");
    }
    let rows: std::collections::BTreeSet<i32> = bar.iter().filter_map(|k| rect(k)).map(|r| r[1].round() as i32).collect();
    assert!(rows.len() > 1, "in half the window the bar takes more than one line: {rows:?}");
}

/// p3041, 5: Initial prompt pressed on the agent's page, the Map beside it ([`map_beside_agent`]), takes the Map's
/// place, focused: two panes as before, not a third (panes.rs `open_pane_beside`).
#[test]
#[cfg(not(windows))]
fn initial_prompt_beside_the_map_takes_its_place_not_a_third_pane() {
    let (_held, mut rt, _child, _out) = map_beside_agent("folder-beside");
    rt.update(|s, _| s.focus_pane(0));
    folder_frames(&mut rt);
    let (x, y) = behind_more(&mut rt, EDGE5, "Initial prompt");
    press(&mut rt, [x, y]);
    folder_frames(&mut rt);
    let initial = Sel::Lens(Lens::of(Filter::InitialPrompt { agent: EDGE5.into() }));
    assert_eq!((rt.state.panes.len(), rt.state.focus, &rt.state.sel), (2, 1, &initial), "in the Map's place, focused, not a third pane");
    assert_eq!(rt.state.panes[0], (0, Sel::Agent(EDGE5.into())), "the agent's page beside it");
}

/// p2545: he presses System prompt to read it, so it opens at the newest session, each of its blocks with its newest
/// version's text under it, asked of coordd on the press (`Request::Raw` of its node, as a popup's is) and let go with
/// the pane; the older session under it, folded until he unfolds it, and then asking nothing. Initial prompt opens at
/// the newest start with the brief it was started with under it, as the window holds that post; the older start under
/// it, its brief not shown.
#[test]
#[cfg(not(windows))]
fn system_prompt_and_initial_prompt_open_at_the_newest_with_its_text() {
    use docket_machine::graph::Source;
    let (_held, mut s, child, out) = folder_app("folder-newest", coord_types::TEXTS_PROTOCOL);
    s.pins.list.clear();
    s.sel = Sel::Agent(EDGE5.into());
    // the post that briefed the newer start, as the window holds it (made up here)
    let brief: coord_types::TicketView = serde_json::from_value(serde_json::json!({"id": "N-1446", "n": 1446, "kind": "thread", "opener": "owner", "ctx": {},
        "form": {}, "title": "0.3", "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1,
        "opened_at": "2026-11-19T07:00:00Z", "given": [], "stars": [], "archived": false,
        "posts": [{"n": 1441, "at": "2026-11-19T07:01:00Z", "by": "paper-research", "text": "Your brief, made up here.", "names": [EDGE5]}]}))
    .unwrap();
    s.projects[0].live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(brief) });
    // two starts, the first briefed with a post the window does not hold; a newer session, its one block in two versions
    let mut folder = made_up_folder(2, 3);
    let start = |line: usize, brief: &str| FolderRow {
        text: "a start of docket-edge5".into(),
        of: format!("start:docket-edge5@{line}"),
        source: Some(Source::Line(line)),
        rows: vec![FolderRow { text: format!("brief: {brief}"), of: brief.into(), source: Some(Source::Line(line)), ..Default::default() }],
        ..Default::default()
    };
    folder[0] = FolderRow { text: "kind and brief: 2 starts".into(), rows: vec![start(10, "N-1446.p1000"), start(240, "N-1446.p1441")], ..Default::default() };
    let version = |v: usize| FolderRow { text: format!("version {v}: carried by 2 requests"), of: format!("text:c{v}"), source: Some(Source::Session("9e0d7c3a-made-up".into(), 10 * v)), ..Default::default() };
    let block = FolderRow { text: "block 1: 2 versions".into(), of: "text:c1".into(), rows: vec![version(1), version(2)], ..Default::default() };
    let blocks = FolderRow { text: "system prompt blocks: 1 block".into(), rows: vec![block], ..Default::default() };
    folder[3].rows.push(FolderRow { text: "session 9e0d7c3a-made-up: 2 requests".into(), of: "session:9e0d7c3a-made-up".into(), rows: vec![blocks], ..Default::default() });
    let mut rt = folder_window(s);
    folder_frames(&mut rt);
    let (x, y) = behind_more(&mut rt, EDGE5, "System prompt");
    press(&mut rt, [x, y]);
    folder_frames(&mut rt);
    folder_answered(&mut rt, folder);
    let (newer, older) = ("sessions|session:9e0d7c3a-made-up|system prompt blocks", "sessions|session:5c6f1a2b-made-up|system prompt blocks");
    assert_eq!(folder_drawn(&rt, EDGE5), [format!("{newer}|text:c1"), older.to_string()], "the newest session's block, then the older session folded");
    assert_eq!(aparts_asked(&rt), ["text text:c2"], "the newest version's text, asked on the press");
    apart_answered(&mut rt, "text text:c2", true, "Made-up block text.".into());
    assert_eq!(keyed_text(&rt, &format!("folder-text-agent:{EDGE5}|{newer}|text:c1|text:c2")).as_deref(), Some("Made-up block text."));
    // the older one unfolded: its block as the sidebar draws it, and nothing asked
    press_in_view(&mut rt, &format!("tree-fold-agent:{EDGE5}|{older}"));
    assert_eq!(folder_drawn(&rt, EDGE5), [format!("{newer}|text:c1"), older.to_string(), format!("{older}|text:b1")]);
    assert!(aparts_asked(&rt).is_empty(), "no text of the older session asked");
    // Initial prompt (p2987): its task, the newest start's brief first;
    // its starts behind what the harness adds, folded, counted; unfolded, the newest start first, the older under it
    rt.update(|s, _| s.show_sel(Sel::Lens(Lens::of(Filter::InitialPrompt { agent: EDGE5.into() }))));
    folder_frames(&mut rt);
    assert_eq!(folder_drawn(&rt, EDGE5), ["harness"], "the starts folded");
    let texts: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect();
    let at = |t: &str| texts.iter().position(|x| x == t).unwrap_or_else(|| panic!("{t}: {texts:?}"));

    assert!(at("Its task") < at("Brief, N-1446.p1441 by paper-research:"), "{texts:?}");
    assert!(at("Brief, N-1446.p1441 by paper-research:") < at("Your brief, made up here."), "its words: {texts:?}");
    assert_eq!(row_label(&rt, &format!("tree-agent:{EDGE5}|harness")).as_deref(), Some("what the harness adds · 3 lines"), "its line and two starts");
    press_in_view(&mut rt, &format!("tree-fold-agent:{EDGE5}|harness"));
    assert_eq!(folder_drawn(&rt, EDGE5), ["harness", "kind and brief|start:docket-edge5@240", "kind and brief|start:docket-edge5@10"], "the newest start first");
    assert!(rt.find("part-brief-N-1446.p1000").is_none(), "the older start's brief not shown");
    assert!(rt.state.folders.texts.is_empty(), "the block's text let go with its pane");
    rt.state.projects[0].link = None;
    let sent: Vec<_> = super::deep::tests::sent(child, &out).into_iter().filter(|v| v.to_string().contains("text:c")).collect();
    assert_eq!(sent.len(), 1, "one text asked in all: {sent:?}");
}

/// An agent's system prompt as coordd's `Graph::prompt` gives it (fix 4), made up: started with the standing text no
/// start line says, then its version 1 appended, then its version 2 in place of Claude Code's own; Claude Code's own
/// two blocks, the second in two versions, and the start that replaced them.
fn made_up_prompt() -> Vec<FolderRow> {
    use docket_machine::graph::Source;
    let start = |line: usize, what: &str| FolderRow { text: format!("2026-11-22T09:{line:02}:00Z: {what}"), of: format!("start:docket-edge5@{line}"), source: Some(Source::Line(line)), ..Default::default() };
    let given = |of: &str, text: &str, line: usize| FolderRow { text: text.into(), of: of.into(), source: Some(Source::Line(line)), rows: vec![start(line, "system: system prompt N, by owner")], ..Default::default() };
    let not_said = FolderRow { text: "the standing text, appended; no start line says which version: carried in 2 requests, 1 turn, 1 step".into(), of: "text:not-said".into(), ..Default::default() };
    let started = vec![not_said, given("text:s1", "version 1, appended: carried in 3 requests, 2 turns, 1 step", 10), given("text:s2", "version 2, in place of Claude Code's own: carried in 1 request, 1 turn, 0 steps", 40)];
    let version = |of: &str, text: &str, line: usize| FolderRow { text: text.into(), of: of.into(), source: Some(Source::Session("9e0d7c3a-made-up".into(), line)), ..Default::default() };
    let block = |text: &str, versions: Vec<FolderRow>| FolderRow { text: text.into(), of: versions[0].of.clone(), rows: versions, ..Default::default() };
    let own = vec![
        block("block 1: 1 version", vec![version("text:c1", "version 1: carried in 5 requests, 3 turns, 1 step", 3)]),
        block("block 2: 2 versions", vec![version("text:d1", "version 1: carried in 2 requests, 1 turn, 1 step", 3), version("text:d2", "version 2: carried in 3 requests, 2 turns, 1 step; 4 words", 20)]),
        start(40, "replaced by your version 2"),
    ];
    vec![
        FolderRow { text: "started with: 3 versions".into(), rows: started, ..Default::default() },
        FolderRow { text: "Claude Code's own: 2 blocks, 3 versions".into(), rows: own, ..Default::default() },
    ]
}

/// Fix 4's System prompt from a coordd that gives it (`PROMPT_PROTOCOL`; docket-machine `Graph::prompt`; docket-edge6
/// N-1446 p2496, accounts-lead p2504, paper-research p2545): the press asks its rows alone (`Ask::prompt`) and no
/// folder. Started with and Claude Code's own each under its line: started with's newest version first, the text it was
/// started with under it; each of Claude Code's own blocks, its newest version first with its text, no caption over
/// either (p2565); the start that replaced them. The newest texts are asked on the press (`Request::Raw` of each
/// node), drawn as the text it is sent, with no tree of its versions (N-1927.p1); all let go with the pane.
#[test]
#[cfg(not(windows))]
fn system_prompt_draws_each_version_counted_with_the_newest_texts_from_a_coordd_that_gives_it() {
    let (_held, mut s, child, out) = folder_app("prompt-node", coord_types::PROMPT_PROTOCOL);
    s.pins.list.clear();
    s.sel = Sel::Agent(EDGE5.into());
    let mut rt = folder_window(s);
    folder_frames(&mut rt);
    let (x, y) = behind_more(&mut rt, EDGE5, "System prompt");
    press(&mut rt, [x, y]);
    folder_frames(&mut rt);
    let answered = |rows: Vec<FolderRow>| serde_json::to_string(&coord_types::GraphAnswer { folder: Some(rows), ..Default::default() }).unwrap();
    let asked = format!("requests {EDGE5} prompt");
    assert!(folders_asked(&rt).is_empty(), "no folder asked for it");
    assert_eq!(aparts_asked(&rt), [asked.clone()], "its system prompt alone");
    apart_answered(&mut rt, &asked, true, answered(made_up_prompt()));
    // N-1864.p1, N-1927.p1: the text it is sent, whole, under where it came from; no tree of its versions
    assert!(folder_drawn(&rt, EDGE5).is_empty(), "no versions drawn");
    // and Claude Code's own main text as it first sent it, which the parts of a version in place of it are read against
    assert_eq!(aparts_asked(&rt), ["text text:c1", "text text:d1", "text text:d2", "text text:s2"], "the newest texts, asked on the press");
    apart_answered(&mut rt, "text text:s2", true, "Made-up version 2.".into());
    apart_answered(&mut rt, "text text:d2", true, "Made-up block 2, version 2.".into());
    let texts: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect();
    assert!(!texts.iter().any(|t| t.starts_with("A system prompt is what") || t.starts_with("It runs with")), "no lines about it: {texts:?}");
    assert_eq!(keyed_text(&rt, &format!("part-agent:{EDGE5}|system prompt|yours")).as_deref(), Some("docket-edge5's system prompt, as it is sent"));
    assert_eq!(keyed_text(&rt, "prompt-from-0").as_deref(), Some("agent override"), "his, in place of Claude Code's own");
    assert!(texts.iter().any(|t| t == "Made-up block 2, version 2."), "the text it is sent: {texts:?}");
    for gone in ["started with", "Claude Code's own", "versions", "› on the Map"] {
        assert!(!texts.iter().any(|t| t.starts_with(gone)), "{gone}: {texts:?}");
    }
    // the pane let go: its rows and its texts
    rt.update(|s, _| s.show_sel(Sel::Agent(EDGE5.into())));
    folder_frames(&mut rt);
    assert!(rt.state.folders.requests.is_empty() && rt.state.folders.texts.is_empty(), "let go with its pane");
    rt.state.projects[0].link = None;
    let sent: Vec<String> = super::deep::tests::sent(child, &out).into_iter().map(|v| v.to_string()).collect();
    let prompts: Vec<&String> = sent.iter().filter(|v| v.contains(r#""prompt":"docket-edge5""#)).collect();
    assert!(prompts.len() == 1 && !prompts[0].contains(r#""version""#), "its rows alone: {prompts:?}");
    assert!(!sent.iter().any(|v| v.contains(r#""folder":"#)), "no folder asked: {sent:?}");
}

/// His edit of an agent's system prompt as a branch (prompt_edit.rs; path fix 6, the owner N-1104 p996; the shape
/// accounts-lead N-1446 p2447, go p2456): Edit on Started with's line opens a box under it holding the newest version's
/// text, its switch in place of Claude Code's own as that version runs (consult-lead N-1446 p2829); his press puts it
/// after, a second back in place. He picks a turn by its time and first line, the newest first; a pick on a session
/// the agent has left goes back to Now, said (proto-mind p2830). Go on with it sends `Request::SystemPrompt` with the
/// byte where the next turn's prompt begins. An answer that can no longer come, and the engine's refusal, are said
/// under the edit, which stays open; its yes closes the edit and the pane says it was sent. The running turn is Now's
/// and is not listed. A Codex agent's Edit opens too: what its harness takes is the engine's to say (N-1790).
#[test]
#[cfg(not(windows))]
fn his_edit_of_a_system_prompt_goes_on_from_the_turn_he_picks() {
    let (_held, mut s, child, out) = folder_app("prompt-edit", coord_types::PROMPT_PROTOCOL);
    s.pins.list.clear();
    s.sel = Sel::Agent(EDGE5.into());
    // its conversation as the chat holds it, made up: two turns that ended and one running, merged as a page from
    // coordd is (live.rs `Conv::merge`, which keys its rows: with no thread between him and it, With you draws them)
    let entry = |offset: u64, at: &str, kind: &str, text: &str| coord_types::Entry { offset, at: format!("2026-11-24T{at}:00Z"), kind: kind.into(), tool: None, text: text.into(), patch: None, images: Vec::new() };
    let entries = vec![
        entry(0, "09:00", "user", "Read the parser"),
        entry(300, "09:01", "assistant", "Read."),
        entry(700, "09:30", "user", "Fix the lexer\nand its tests"),
        entry(900, "09:31", "tool", "cargo test"),
        entry(1100, "09:32", "result", "ok"),
        entry(1300, "09:33", "assistant", "Fixed."),
        entry(1500, "10:00", "user", "Now the docs"),
    ];
    s.projects[0].convs.entry(EDGE5.into()).or_default().merge(coord_types::Transcript { entries, start: 0, next: 1800, session: "/made-up/s1.jsonl".into(), ..Default::default() }, false);
    let mut rt = folder_window(s);
    folder_frames(&mut rt);
    let (x, y) = behind_more(&mut rt, EDGE5, "System prompt");
    press(&mut rt, [x, y]);
    folder_frames(&mut rt);
    let rows = serde_json::to_string(&coord_types::GraphAnswer { folder: Some(made_up_prompt()), ..Default::default() }).unwrap();
    apart_answered(&mut rt, &format!("requests {EDGE5} prompt"), true, rows);
    apart_answered(&mut rt, "text text:s2", true, "Made-up version 2.".into());
    assert!(rt.find(&format!("prompt-editor-{EDGE5}")).is_none(), "no edit open before his press");
    // Edit: the box holds version 2's text; Now is chosen, the newest turn that ended listed first
    press_in_view(&mut rt, &format!("prompt-edit-{EDGE5}"));
    assert_eq!(rt.state.prompt_edit.of, Some((0, EDGE5.to_string())));
    assert_eq!(rt.state.prompt_edit.text.text, "Made-up version 2.");
    // the box takes his first key, at its start, with no click into it first (proto-window's walk, N-1446 p3038 6)
    crate::view_cost::type_in(&mut rt, "Now ");
    assert_eq!(rt.state.prompt_edit.text.text, "Now Made-up version 2.");
    let listed: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.key.filter(|k| k.starts_with("prompt-edit-turn-"))).collect();
    assert_eq!(listed, ["prompt-edit-turn-1500", "prompt-edit-turn-700"], "the turns at 09:30 and 09:00, newest first; 10:00's is running");
    // each turn's time on its radio, its first line beside it, cut with "…" at the pane's edge (p3038 8)
    let mut words: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).filter(|t| t.starts_with("After ") || t.starts_with("· ")).collect();
    words.dedup();
    assert!(words.len() == 4 && words[0].starts_with("After ") && words[1] == "· Fix the lexer …" && words[2].starts_with("After ") && words[3] == "· Read the parser", "{words:?}");
    // the switch as version 2 runs, in place of Claude Code's own; his press puts it after, a second back in place
    assert!(rt.state.prompt_edit.replace, "version 2 runs in place of Claude Code's own");
    press_in_view(&mut rt, "prompt-edit-replace");
    assert!(!rt.state.prompt_edit.replace, "after Claude Code's own");
    press_in_view(&mut rt, "prompt-edit-replace");
    // from the turn at 09:00: the copy keeps it and ends where 09:30's begins; a pick on a session it has left goes
    // back to Now, said, and is picked again on its session
    press_in_view(&mut rt, "prompt-edit-turn-700");
    assert_eq!(rt.state.prompt_edit.after, Some(("/made-up/s1.jsonl".to_string(), 700)));
    let session = |rt: &mut Runtime<App>, to: &str| {
        let to = to.to_string();
        rt.update(move |s: &mut App, _| {
            s.projects[0].convs.get_mut(EDGE5).unwrap().session = to;
            crate::prompt_edit::update(s);
        });
        folder_frames(rt);
    };
    session(&mut rt, "/made-up/s2.jsonl");
    assert_eq!(rt.state.prompt_edit.after, None);
    assert!(keyed_text(&rt, "prompt-edit-error").is_some_and(|t| t.starts_with("New session")));
    session(&mut rt, "/made-up/s1.jsonl");
    press_in_view(&mut rt, "prompt-edit-turn-700");
    // an answer that can no longer come is said, the edit open
    rt.update(|s: &mut App, _| {
        let (tx, rx) = std::sync::mpsc::channel();
        drop(tx);
        s.prompt_edit.running = Some(rx);
        crate::prompt_edit::update(s);
    });
    folder_frames(&mut rt);
    assert!(keyed_text(&rt, "prompt-edit-error").is_some_and(|t| t.starts_with("No answer;")));
    assert!(rt.state.prompt_edit.of.is_some() && rt.state.prompt_edit.running.is_none());
    rt.update(|s: &mut App, _| s.prompt_edit.text = rune::TextEdit::new("Made-up version 3.\n"));
    let asked = |rt: &Runtime<App>| rt.state.projects[0].pending.iter().filter(|(_, w)| matches!(w, crate::Pending::Roster(_))).map(|(id, _)| *id).collect::<Vec<_>>();
    press_in_view(&mut rt, "prompt-edit-go");
    let first = asked(&rt);
    assert_eq!(first.len(), 1, "sent once");
    // the engine refuses it: said under the edit, which stays open with his text
    let refused = "coord system-prompt: byte 700 is not the end of a turn of /made-up/s1.jsonl";
    rt.update(|s: &mut App, _| {
        s.take(0, crate::link::Incoming::Reply(first[0], coord_types::Reply { ok: false, out: refused.into(), post: None }));
        crate::prompt_edit::update(s);
    });
    folder_frames(&mut rt);
    assert_eq!(keyed_text(&rt, "prompt-edit-error").as_deref(), Some(refused));
    assert!(rt.state.prompt_edit.of.is_some() && rt.state.prompt_edit.text.text == "Made-up version 3.\n");
    // sent again and taken: the edit closed, the pane says it was sent
    press_in_view(&mut rt, "prompt-edit-go");
    let second = asked(&rt);
    assert_eq!(second.len(), 1);
    rt.update(|s: &mut App, _| {
        s.take(0, crate::link::Incoming::Reply(second[0], coord_types::Reply { ok: true, out: format!("{EDGE5} goes on with system prompt 3 from a copy of its session cut after 700 bytes, once its turn ends"), post: None }));
        crate::prompt_edit::update(s);
    });
    folder_frames(&mut rt);
    assert!(rt.state.prompt_edit.of.is_none() && rt.find(&format!("prompt-editor-{EDGE5}")).is_none(), "closed");
    let said = keyed_text(&rt, &format!("prompt-edit-sent-{EDGE5}")).unwrap_or_default();
    assert!(said.starts_with("Sent at ") && said.contains("from the picked turn"), "{said}");
    // a Codex agent: Edit opens as for any harness; what it takes is the engine's to say, its refusal said under the
    // edit (the owner, N-1790 part 1: no kind gate in the window)
    rt.update(|s: &mut App, _| s.projects[0].live.agents[0].kind = "codex".into());
    folder_frames(&mut rt);
    press_in_view(&mut rt, &format!("prompt-edit-{EDGE5}"));
    assert!(rt.state.prompt_edit.of.is_some(), "opened for this harness too");
    rt.state.projects[0].link = None;
    let sent: Vec<serde_json::Value> = super::deep::tests::sent(child, &out).into_iter().filter(|v| v["req"]["op"] == "systemprompt").map(|v| v["req"].clone()).collect();
    let want = serde_json::json!({"op": "systemprompt", "name": EDGE5, "text": "Made-up version 3.", "replace": true, "after": 700});
    assert_eq!(sent, [want.clone(), want], "its text without the trailing newline, in place of Claude Code's own, after the turn at 09:00");
}

/// The slice around started with's version 2 as coordd would answer it with work beside his (made up): one hop from
/// it, the twelve requests that carried it.
fn near_text() -> coord_types::GraphAnswer {
    let f = "text:s2";
    let mut nodes = vec![serde_json::json!({"id": f, "kind": "text", "line": 1, "when": "2026-11-22T09:00:00Z", "what": "started with, version 2: 9,450 bytes"})];
    let mut edges = Vec::new();
    for k in 1..=12 {
        let id = format!("request:r{k}");
        nodes.push(serde_json::json!({"id": id, "kind": "request", "line": k + 1, "when": format!("2026-11-22T09:{:02}:00Z", 10 + k), "what": "claude-opus-5-5, effort high"}));
        edges.push(serde_json::json!({"from": id, "to": f, "kind": "carried", "line": k + 1, "layer": "work"}));
    }
    serde_json::from_value(serde_json::json!({"slice": {"nodes": nodes, "edges": edges, "whole": {}, "shown": {}, "left": 0}, "labels": []})).unwrap()
}

/// The Map calls a version of a system prompt one (proto-window's walk, N-1446 p3038 4): its card says "system
/// prompt", read off the start that was started with it (docket-machine graph.rs `StartedWith`), where it said "text a
/// session read" under "try-1's system prompt, version 1, appended"; a text no start was started with keeps its word.
#[test]
fn a_system_prompt_version_on_the_map_is_called_a_system_prompt() {
    let base = super::cloud::tests::base();
    let note = |near: &coord_types::GraphAnswer| {
        let (g, _) = super::cloud::cloud(&base, Some(("text:s2", near)), "project:research", "research", &mut Default::default()).unwrap();
        g.nodes().iter().find(|n| n.id == "text:s2").map(|n| n.note.clone()).expect("its card")
    };
    let carried = near_text();
    assert!(note(&carried).starts_with("text a session read · "), "{}", note(&carried));
    let mut started = serde_json::to_value(&carried).unwrap();
    started["slice"]["nodes"].as_array_mut().unwrap().push(serde_json::json!({"id": "start:docket-edge5@9", "kind": "start", "line": 14, "when": "2026-11-22T09:00:00Z", "what": "system: claude claude-opus-5-5, system prompt 2, by owner"}));
    started["slice"]["edges"].as_array_mut().unwrap().push(serde_json::json!({"from": "start:docket-edge5@9", "to": "text:s2", "kind": "started_with", "line": 14, "layer": "work", "what": "version 2, appended"}));
    let started: coord_types::GraphAnswer = serde_json::from_value(started).unwrap();
    assert!(note(&started).starts_with("system prompt · "), "{}", note(&started));
}

/// The slice around started with's version 2 as coordd gives it for ASK, made up, each edge in the layer docket-machine
/// graph_slice.rs `Layer::by` puts it in and given only when the ask has that layer on: the session that loaded it
/// (work), the start of docket-edge5's that was started with it, in place of Claude Code's own (a call's, beside what
/// its requests carried), and the twelve requests that carried it (calls).
fn near_started(ask: &docket_machine::graph_slice::Ask) -> coord_types::GraphAnswer {
    let f = "text:s2";
    let (start, session) = (format!("start:{EDGE5}@9"), "session:5c6f1a2b-made-up");
    let mut nodes = vec![
        serde_json::json!({"id": f, "kind": "text", "line": 1, "when": "2026-11-22T09:00:00Z", "what": format!("{EDGE5}'s system prompt, version 2, in place of Claude Code's own")}),
        serde_json::json!({"id": start, "kind": "start", "line": 14, "when": "2026-11-22T09:00:00Z", "what": "system: claude claude-opus-5-5, system prompt 2 in place of Claude Code's own, by owner"}),
        serde_json::json!({"id": session, "kind": "session", "line": 15, "when": "2026-11-22T09:00:00Z"}),
    ];
    let mut edges = vec![
        serde_json::json!({"from": session, "to": f, "kind": "loaded", "line": 15, "layer": "work"}),
        serde_json::json!({"from": start, "to": f, "kind": "started_with", "line": 14, "layer": "calls", "what": "version 2, in place of Claude Code's own"}),
    ];
    for k in 1..=12 {
        let id = format!("request:r{k}");
        nodes.push(serde_json::json!({"id": id, "kind": "request", "line": k + 15, "when": format!("2026-11-22T09:{:02}:00Z", 10 + k), "what": "claude-opus-5-5, effort high"}));
        edges.push(serde_json::json!({"from": id, "to": f, "kind": "carried", "line": k + 15, "layer": "calls"}));
    }
    let on = |e: &serde_json::Value| ask.layers.is_empty() || ask.layers.iter().any(|l| serde_json::to_value(l).unwrap() == e["layer"]);
    edges.retain(on);
    nodes.retain(|n| n["id"] == f || edges.iter().any(|e| e["from"] == n["id"] || e["to"] == n["id"]));
    serde_json::from_value(serde_json::json!({"slice": {"nodes": nodes, "edges": edges, "whole": {}, "shown": {}, "left": 0}, "labels": []})).unwrap()
}

/// Edit on the Map's card of a system prompt version is the pane's Edit, one component (paper-research N-1446 p3041,
/// 3; proto-window's walk, p3038 3: "On the Map he cannot change it"). From System prompt's "› on the Map" under his
/// words, the slice around the text is asked with calls on, where the start that was started with it is: a start's
/// `started_with` is a call's edge (docket-machine graph_slice.rs `Layer::by`), so with work alone coordd's slice holds
/// no start, and the card said "text a session read" with nothing to edit. The card says it is a system prompt and
/// has Edit; his press opens the pane's edit apart over the Map (popup.rs), its box holding the version's text, asked
/// again of coordd, and taking his first key, the switch as the version runs; the pane under it draws no second
/// editor; Go on with it sends it as the pane's does, and the panel says it was sent.
#[test]
#[cfg(not(windows))]
fn his_edit_from_a_system_prompt_card_on_the_map_is_the_panes_edit() {
    let (_held, mut s, child, out) = folder_app("prompt-map-edit", coord_types::PROMPT_PROTOCOL);
    s.pins.list.clear();
    s.sel = Sel::Agent(EDGE5.into());
    let mut rt = folder_window(s);
    rt.poll = Some(crate::poll);
    folder_frames(&mut rt);
    let (x, y) = behind_more(&mut rt, EDGE5, "System prompt");
    press(&mut rt, [x, y]);
    folder_frames(&mut rt);
    let answered = serde_json::to_string(&coord_types::GraphAnswer { folder: Some(made_up_prompt()), ..Default::default() }).unwrap();
    apart_answered(&mut rt, &format!("requests {EDGE5} prompt"), true, answered);
    apart_answered(&mut rt, "text text:s2", true, "Made-up version 2.".into());
    rt.update(|s: &mut App, _| crate::map::canvas::on_map(s, "text:s2"));
    assert_eq!(rt.state.cloud.focus.as_deref(), Some("text:s2"), "the Map's focus the text's node");
    // the Map's asks answered as coordd would: the slice around the text by the layers it asks, his layer for the rest
    let mut around = None;
    for _ in 0..20 {
        for (id, ask) in graph_asked(&rt) {
            let a = match ask.around.as_deref() {
                Some("text:s2") => {
                    around = Some(ask.layers.clone());
                    near_started(&ask)
                }
                _ => super::cloud::tests::base(),
            };
            let out = serde_json::to_string(&a).unwrap();
            rt.update(move |s: &mut App, _| _ = s.take(0, crate::link::Incoming::Reply(id, coord_types::Reply { ok: true, out, post: None })));
        }
        rt.frame(0.05);
    }
    // the flight to its card ended
    for _ in 0..60 {
        rt.frame(0.05);
    }
    let around = around.expect("the slice around the text asked");
    assert!(around.contains(&docket_machine::graph_slice::Layer::Calls), "asked with calls on, where its start is: {around:?}");
    let note = rt.state.map.graph().and_then(|g| g.nodes().iter().find(|n| n.id == "text:s2").map(|n| n.note.clone())).expect("its card");
    assert!(note.starts_with("system prompt"), "{note}");
    // and so does its row in the left bar's structure, which said "text a session read" (p3038 4)
    let said: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect();
    assert!(!said.iter().any(|t| t == "text a session read") && said.iter().any(|t| t == "system prompt"), "{said:?}");
    // Edit on its card; his press opens the pane's edit over the Map, the Map staying where it is
    let edit = format!("map-prompt-edit-{EDGE5}");
    let card = |rt: &Runtime<App>| rt.state.map.graph().map(|g| rt.state.map.shown().into_iter().filter(|x| g.node(x.i).id == "text:s2").map(|x| (x.r, x.look, x.group.is_some(), x.many)).collect::<Vec<_>>());
    // the Map flew into its card, where it stood a dot among the threads of his layer
    let drawn = card(&rt).unwrap_or_default();
    let all = rt.state.map.graph().map(|g| rt.state.map.shown().into_iter().map(|x| (g.node(x.i).id.clone(), x.r, x.look)).collect::<Vec<_>>());
    assert!(drawn.len() == 1 && drawn[0].1 != super::lod::Look::Dot && drawn[0].0[2] >= 0.75 * rt.state.map.size()[0], "flown into: {drawn:?}; view {:?}, flying {}, all {all:?}", rt.state.map.size(), rt.state.map.flying());
    let (x, y) = rt.find(&edit).unwrap_or_else(|| panic!("Edit on the card {:?}: {:?}", card(&rt), rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect::<Vec<_>>()));
    press(&mut rt, [x, y]);
    folder_frames(&mut rt);
    let text_asked = rt.state.projects[0].pending.iter().find(|(_, w)| apart_named(w).as_deref() == Some("text text:s2")).map(|(id, _)| *id);
    if let Some(id) = text_asked {
        rt.update(move |s: &mut App, _| _ = s.take(0, crate::link::Incoming::Reply(id, coord_types::Reply { ok: true, out: "Made-up version 2.".into(), post: None })));
        folder_frames(&mut rt);
    }
    assert_eq!(rt.state.cloud.focus.as_deref(), Some("text:s2"), "the press on Edit went into nothing else");
    assert_eq!(rt.state.prompt_edit.of, Some((0, EDGE5.to_string())));
    let editors = rt.elements().into_iter().filter(|e| !e.exiting && e.key.as_deref() == Some(&format!("prompt-editor-{EDGE5}"))).count();
    assert_eq!(editors, 1, "one editor, apart over the Map");
    assert!(rt.state.popups.iter().any(|p| p.title.contains(EDGE5)), "in a panel apart: {:?}", rt.state.popups.iter().map(|p| &p.title).collect::<Vec<_>>());
    assert_eq!(rt.state.prompt_edit.text.text, "Made-up version 2.");
    assert!(rt.state.prompt_edit.replace, "the switch as version 2 runs, in place of Claude Code's own");
    crate::view_cost::type_in(&mut rt, "Now ");
    assert_eq!(rt.state.prompt_edit.text.text, "Now Made-up version 2.", "the box takes his first key");
    press_in_view(&mut rt, "prompt-edit-go");
    let asked: Vec<u64> = rt.state.projects[0].pending.iter().filter(|(_, w)| matches!(w, crate::Pending::Roster(_))).map(|(id, _)| *id).collect();
    assert_eq!(asked.len(), 1, "sent once");
    let id = asked[0];
    rt.update(move |s: &mut App, _| _ = s.take(0, crate::link::Incoming::Reply(id, coord_types::Reply { ok: true, out: format!("{EDGE5} goes on with system prompt 3 on its conversation as it stands, once its turn ends"), post: None })));
    folder_frames(&mut rt);
    let said = keyed_text(&rt, &format!("prompt-edit-sent-{EDGE5}")).unwrap_or_default();
    assert!(said.starts_with("Sent at "), "the panel says it was sent: {said:?}");
    rt.state.projects[0].link = None;
    let sent: Vec<serde_json::Value> = super::deep::tests::sent(child, &out).into_iter().filter(|v| v["req"]["op"] == "systemprompt").map(|v| v["req"].clone()).collect();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!((&sent[0]["name"], &sent[0]["text"], &sent[0]["replace"]), (&serde_json::json!(EDGE5), &serde_json::json!("Now Made-up version 2."), &serde_json::json!(true)));
}

/// A list of more than forty rows lists forty (sidebar.rs `block_rows`, the structure's `LISTED`): 45 files read list
/// the first forty and a row saying 5 more; a press there lists the last five under a row saying 40 more, and a press
/// on that one the first forty again.
#[test]
#[cfg(not(windows))]
fn a_long_list_in_a_folder_lists_forty_with_a_row_saying_how_many_more() {
    let (_held, s, _child, _out) = folder_app("folder-5", coord_types::FOLDER_PROTOCOL);
    let mut rt = folder_window(s);
    folder_frames(&mut rt);
    folder_at_claude_md(&mut rt, made_up_folder(2, 45));
    unfold_in_folder(&mut rt, "read");
    let reads = |rt: &Runtime<App>| folder_drawn(rt, EDGE5).into_iter().filter(|x| x.starts_with("read|") || x.starts_with("cut read")).collect::<Vec<_>>();
    let first: Vec<String> = (1..=40).map(|i| format!("read|file:src/f{i}.rs")).chain(["cut read-down".to_string()]).collect();
    assert_eq!(reads(&rt), first);
    assert_eq!(row_label(&rt, "tree-cut-agent:docket-edge5|read-down").as_deref(), Some("5 more"));
    press_in_structure(&mut rt, "cut-agent:docket-edge5|read-down", "tree-cut-agent:docket-edge5|read-down");
    folder_frames(&mut rt);
    let last: Vec<String> = ["cut read-up".to_string()].into_iter().chain((41..=45).map(|i| format!("read|file:src/f{i}.rs"))).collect();
    assert_eq!(reads(&rt), last);
    assert_eq!(row_label(&rt, "tree-cut-agent:docket-edge5|read-up").as_deref(), Some("40 more"));
    press_in_structure(&mut rt, "cut-agent:docket-edge5|read-up", "tree-cut-agent:docket-edge5|read-up");
    folder_frames(&mut rt);
    assert_eq!(reads(&rt), first, "the first forty again");
}

/// A folder asked when the pipe ends is asked again, once, when the pipe is back (lib.rs's drain, sidebar.rs
/// `folder_lost`).
#[test]
#[cfg(not(windows))]
fn a_folder_asked_when_the_pipe_ended_is_asked_again_once_it_is_back() {
    let (_held, s, child, out) = folder_app("folder-6", coord_types::FOLDER_PROTOCOL);
    let mut rt = folder_window(s);
    folder_frames(&mut rt);
    let (x, y) = rt.find("agent-fold-docket-edge5").expect("docket-edge5's row has a fold");
    press(&mut rt, [x, y]);
    folder_frames(&mut rt);
    assert_eq!(folders_asked(&rt).len(), 1);
    rt.update(|s: &mut App, _| {
        s.take(0, crate::link::Incoming::Down("the bridge ended".into()));
    });
    folder_frames(&mut rt);
    assert!(folders_asked(&rt).is_empty(), "nothing on its way, nothing asked of a pipe that is down");
    let (child2, out2) = super::deep::tests::piped(&mut rt.state, 0, "folder-6b");
    rt.state.projects[0].protocol = coord_types::FOLDER_PROTOCOL;
    folder_frames(&mut rt);
    folder_frames(&mut rt);
    assert_eq!(folders_asked(&rt).len(), 1, "asked once more on the pipe that came back");
    rt.state.projects[0].link = None;
    assert_eq!(folders_sent(child, &out).len(), 1, "one ask on the pipe that ended");
    assert_eq!(folders_sent(child2, &out2).len(), 1, "one on the pipe that came back");
}

/// Whether text T is drawn in the first popup.
fn in_popup(rt: &Runtime<App>, t: &str) -> bool {
    rt.elements().iter().any(|e| !e.exiting && e.path.split('/').any(|k| k == "popup0-dialog") && e.text.as_deref() == Some(t))
}

const SESSION: &str = "agent:docket-edge5|sessions|session:5c6f1a2b-made-up";
const B1: &str = "agent:docket-edge5|sessions|session:5c6f1a2b-made-up|system prompt blocks|text:b1|text:b1";

/// What the folders' popups asked of project 0's coordd and is on its way (sidebar.rs `apart_poll`): `text ID` for a
/// block's text, `requests AGENT SESSION` for a session's requests.
fn aparts_asked(rt: &Runtime<App>) -> Vec<String> {
    let mut v: Vec<String> = rt.state.projects[0].pending.values().filter_map(apart_named).collect();
    v.sort();
    v
}

/// A folder's popup's ask W as [`aparts_asked`] names it.
fn apart_named(w: &crate::Pending) -> Option<String> {
    match w {
        crate::Pending::FolderText { id } => Some(format!("text {id}")),
        crate::Pending::Requests { agent, of } => Some(format!("requests {agent} {of}")),
        _ => None,
    }
}

/// Coordd's answer OUT (OK or not) to WHICH of [`aparts_asked`], taken as the window takes a reply.
fn apart_answered(rt: &mut Runtime<App>, which: &str, ok: bool, out: String) {
    let id = *rt.state.projects[0].pending.iter().find(|(_, w)| apart_named(w).as_deref() == Some(which)).unwrap_or_else(|| panic!("{which} asked")).0;
    rt.update(|s: &mut App, _| {
        s.take(0, crate::link::Incoming::Reply(id, coord_types::Reply { ok, out, post: None }));
    });
    folder_frames(rt);
}

/// Element KEY pressed, as his click does, once brought into view.
fn press_in_view(rt: &mut Runtime<App>, key: &str) {
    let to = key.to_string();
    rt.update(move |_, tx| tx.scroll_into_view(to));
    for _ in 0..20 {
        rt.frame(0.05);
    }
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} drawn"));
    press(rt, [x, y]);
    folder_frames(rt);
}

/// docket-edge5's row unfolded, its folder answered with ROWS, and its session's rows unfolded down to block 1's.
fn folder_at_block(rt: &mut Runtime<App>, rows: Vec<FolderRow>) {
    let (x, y) = rt.find("agent-fold-docket-edge5").expect("docket-edge5's row has a fold");
    press(rt, [x, y]);
    folder_frames(rt);
    folder_answered(rt, rows);
    unfold_in_folder(rt, "sessions");
    unfold_in_folder(rt, "sessions|session:5c6f1a2b-made-up");
    unfold_in_folder(rt, "sessions|session:5c6f1a2b-made-up|system prompt blocks");
    unfold_in_folder(rt, "sessions|session:5c6f1a2b-made-up|system prompt blocks|text:b1");
}

/// N requests of the made-up session as coordd's `Graph::session_requests` gives them (written here).
fn made_up_requests(n: usize) -> String {
    use docket_machine::graph::Source;
    let rows = (1..=n)
        .map(|i| FolderRow {
            text: format!("2026-11-22T10:{:02}:{:02}Z: claude-opus-5-5, effort high; 30 prompt tokens, 2 out; carried block 1 version 1", i / 60, i % 60),
            of: format!("request:r{i}"),
            source: Some(Source::Session("5c6f1a2b-made-up".into(), 300 + i)),
            ..Default::default()
        })
        .collect();
    serde_json::to_string(&coord_types::GraphAnswer { folder: Some(rows), ..Default::default() }).unwrap()
}

/// The requests listed in the popups, by their places in the session's list.
fn requests_listed(rt: &Runtime<App>) -> Vec<usize> {
    let mut v: Vec<usize> = rt
        .elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| {
            let key = e.key?;
            key.strip_prefix("folder-request-")?.parse().ok()
        })
        .collect();
    v.sort();
    v.dedup();
    v
}

/// p1642, 2: a press on a system prompt block's version opens it apart in popup.rs's panel, titled with the agent, the
/// block and the version, and asks coordd for its text once (`Request::Raw` of its node, nothing else), the panel
/// saying so under where it comes from until it comes; then the text lies there. Nothing more is asked while it is
/// open; closed, its text is let go, and a press again asks again.
#[test]
#[cfg(not(windows))]
fn a_blocks_version_opens_apart_with_its_text_asked_of_coordd_on_the_press() {
    let (_held, s, child, out) = folder_app("folder-7", coord_types::TEXTS_PROTOCOL);
    let mut rt = folder_window(s);
    folder_frames(&mut rt);
    folder_at_block(&mut rt, made_up_folder(2, 3));
    assert!(aparts_asked(&rt).is_empty(), "nothing asked before the press");
    press_in_structure(&mut rt, B1, &format!("tree-{B1}"));
    folder_frames(&mut rt);
    assert_eq!(rt.state.popups.len(), 1, "one popup");
    assert_eq!(rt.state.popups[0].what, crate::popup::What::Folder { pi: 0, agent: EDGE5.into(), id: B1.into() });
    assert_eq!(rt.state.popups[0].title, "docket-edge5 · block 1, version 1 · its text");
    assert_eq!(aparts_asked(&rt), ["text text:b1"], "its text asked once over the frames");
    assert!(in_popup(&rt, "from 5c6f1a2b:50"), "where it comes from");
    assert!(in_popup(&rt, "loading…"), "a note until it comes");
    apart_answered(&mut rt, "text text:b1", true, "You are Claude Code, made up here.".into());
    assert_eq!(keyed_text(&rt, &format!("folder-text-{B1}")).as_deref(), Some("You are Claude Code, made up here."));
    assert!(!in_popup(&rt, "loading…"), "the note gone with the text");
    assert!(aparts_asked(&rt).is_empty(), "nothing more asked");
    rt.state.popups.clear();
    folder_frames(&mut rt);
    assert!(rt.state.folders.texts.is_empty(), "its text let go with its popup");
    press_in_structure(&mut rt, B1, &format!("tree-{B1}"));
    folder_frames(&mut rt);
    assert_eq!(aparts_asked(&rt), ["text text:b1"], "asked again on the next press");
    rt.state.projects[0].link = None;
    let sent: Vec<_> = super::deep::tests::sent(child, &out).into_iter().filter(|v| v.to_string().contains("text:b1")).collect();
    assert_eq!(sent.len(), 2, "two asks in all: {sent:?}");
    assert!(sent.iter().all(|v| !v.to_string().contains("folder")), "asked by its node alone: {sent:?}");
}

/// Below TEXTS_PROTOCOL a block's version and a session open nothing apart, and nothing is asked of them: a version
/// with no words is a row, and a press on a session folds it, as before.
#[test]
#[cfg(not(windows))]
fn below_the_texts_protocol_a_blocks_version_and_a_session_open_nothing_apart() {
    let (_held, s, child, out) = folder_app("folder-8", coord_types::TEXTS_PROTOCOL - 1);
    let mut rt = folder_window(s);
    folder_frames(&mut rt);
    folder_at_block(&mut rt, made_up_folder(2, 3));
    press_in_structure(&mut rt, B1, &format!("tree-{B1}"));
    folder_frames(&mut rt);
    assert!(rt.state.popups.is_empty(), "a block's version opens nothing");
    press_in_structure(&mut rt, SESSION, &format!("tree-{SESSION}"));
    folder_frames(&mut rt);
    assert!(rt.state.popups.is_empty(), "a session opens nothing");
    assert!(!rt.state.streams.open.contains(&format!("tree|0|{SESSION}")), "the press folds the session");
    assert!(aparts_asked(&rt).is_empty(), "nothing asked");
    rt.state.projects[0].link = None;
    let sent = super::deep::tests::sent(child, &out);
    assert!(sent.iter().all(|v| !v.to_string().contains("text:b1") && !v.to_string().contains(r#""session":"#)), "{sent:?}");
}

/// p1642, 3: a press on a session opens its requests apart, asked of coordd once (`Ask::folder` with its session), a
/// note until they come; then each says what coordd's graph says of it, then where it comes from, forty at a time, a
/// row at each cut saying how many more. The session's fold unfolds it in the tree as before, and the press leaves it
/// so. When its agent moves they are asked again, once, those held drawn meanwhile.
#[test]
#[cfg(not(windows))]
fn a_sessions_press_opens_its_requests_apart_forty_at_a_time() {
    let (_held, s, child, out) = folder_app("folder-9", coord_types::TEXTS_PROTOCOL);
    let mut rt = folder_window(s);
    folder_frames(&mut rt);
    folder_at_block(&mut rt, made_up_folder(2, 3));
    press_in_structure(&mut rt, SESSION, &format!("tree-{SESSION}"));
    folder_frames(&mut rt);
    assert_eq!(rt.state.popups.len(), 1, "one popup");
    assert_eq!(rt.state.popups[0].what, crate::popup::What::Requests { pi: 0, agent: EDGE5.into(), id: SESSION.into(), session: "session:5c6f1a2b-made-up".into() });
    assert_eq!(rt.state.popups[0].title, "docket-edge5 · session 5c6f1a2b-made-up · its requests");
    assert!(rt.state.streams.open.contains(&format!("tree|0|{SESSION}")), "its fold as it was");
    let which = "requests docket-edge5 session:5c6f1a2b-made-up";
    assert_eq!(aparts_asked(&rt), [which], "asked once over the frames");
    assert!(in_popup(&rt, "loading…"), "a note until they come");
    apart_answered(&mut rt, which, true, made_up_requests(45));
    assert_eq!(requests_listed(&rt), (0..40).collect::<Vec<_>>(), "the first forty");
    assert_eq!(keyed_text(&rt, "folder-request-0-words").as_deref(), Some("2026-11-22T10:00:01Z: claude-opus-5-5, effort high; 30 prompt tokens, 2 out; carried block 1 version 1"));
    assert_eq!(keyed_text(&rt, "folder-request-0-from").as_deref(), Some("5c6f1a2b:301"), "where it comes from");
    assert_eq!(row_label(&rt, "folder-requests-cut-down").as_deref(), Some("5 more"));
    press_in_view(&mut rt, "folder-requests-cut-down");
    assert_eq!(requests_listed(&rt), (40..45).collect::<Vec<_>>(), "the last five");
    assert_eq!(row_label(&rt, "folder-requests-cut-up").as_deref(), Some("40 more"));
    assert!(aparts_asked(&rt).is_empty(), "nothing more asked");
    rt.state.projects[0].live.agents[0] = folder_agent(EDGE5, "working", "2026-11-22T10:50:00Z", 1);
    folder_frames(&mut rt);
    assert_eq!(aparts_asked(&rt), [which], "its turn's start asks them again, once");
    assert_eq!(requests_listed(&rt), (40..45).collect::<Vec<_>>(), "those held drawn meanwhile");
    rt.state.projects[0].link = None;
    let sent: Vec<_> = super::deep::tests::sent(child, &out).into_iter().filter(|v| v.to_string().contains(r#""session":"session:5c6f1a2b-made-up""#)).collect();
    assert_eq!(sent.len(), 2, "two asks in all: {sent:?}");
    assert!(sent[0].to_string().contains(r#""folder":"docket-edge5""#), "{sent:?}");
}

/// What the folders' popups asked when the pipe ends is asked again, once each, when it is back (lib.rs's drain,
/// sidebar.rs `apart_lost`): a block's text, its popup saying so meanwhile, and a session's requests.
#[test]
#[cfg(not(windows))]
fn a_blocks_text_and_a_sessions_requests_asked_when_the_pipe_ended_are_asked_again_once_it_is_back() {
    let (_held, s, child, out) = folder_app("folder-10", coord_types::TEXTS_PROTOCOL);
    let mut rt = folder_window(s);
    folder_frames(&mut rt);
    folder_at_block(&mut rt, made_up_folder(2, 3));
    crate::popup::open(&mut rt.state, "the block", crate::popup::What::Folder { pi: 0, agent: EDGE5.into(), id: B1.into() });
    crate::popup::open(&mut rt.state, "the session", crate::popup::What::Requests { pi: 0, agent: EDGE5.into(), id: SESSION.into(), session: "session:5c6f1a2b-made-up".into() });
    folder_frames(&mut rt);
    let both = ["requests docket-edge5 session:5c6f1a2b-made-up", "text text:b1"];
    assert_eq!(aparts_asked(&rt), both);
    rt.update(|s: &mut App, _| {
        s.take(0, crate::link::Incoming::Down("the bridge ended".into()));
    });
    folder_frames(&mut rt);
    assert!(aparts_asked(&rt).is_empty(), "nothing on its way, nothing asked of a pipe that is down");
    assert!(in_popup(&rt, "Disconnected; retries when back."));
    let (child2, out2) = super::deep::tests::piped(&mut rt.state, 0, "folder-10b");
    rt.state.projects[0].protocol = coord_types::TEXTS_PROTOCOL;
    folder_frames(&mut rt);
    folder_frames(&mut rt);
    assert_eq!(aparts_asked(&rt), both, "each asked once more on the pipe that came back");
    rt.state.projects[0].link = None;
    let ours = |v: &serde_json::Value| {
        let t = v.to_string();
        t.contains("text:b1") || t.contains(r#""session":"#)
    };
    assert_eq!(super::deep::tests::sent(child, &out).iter().filter(|v| ours(v)).count(), 2, "two on the pipe that ended");
    assert_eq!(super::deep::tests::sent(child2, &out2).iter().filter(|v| ours(v)).count(), 2, "two on the pipe that came back");
}

/// The window reads as a page (lib.rs `view`, paper-research N-1446 p2987, 3), but not the Map's canvases, whose presses
/// are the Map's (canvas.rs `board`, lanes.rs `board`): a drag over research's badge pans and selects no words, and a
/// double-click on a word drawn on either canvas lights no word. The inspector is the page's: a drag across one of its
/// lines selects that line.
#[test]
fn the_maps_words_keep_its_presses_and_the_inspectors_select() {
    let (_held, s) = map_app();
    let mut rt = window(s);
    let badge = "16 held · limit";
    let at = says(&rt, badge).expect("research's badge");
    let to = [at[0] + 60.0, at[1] + 40.0];
    rt.pointer_move(at[0], at[1]);
    rt.frame(0.016);
    rt.pointer_down(at[0], at[1]);
    rt.frame(0.016);
    for k in 1..=6 {
        let f = k as f32 / 6.0;
        rt.pointer_move(at[0] + (to[0] - at[0]) * f, at[1] + (to[1] - at[1]) * f);
        rt.frame(0.016);
    }
    rt.pointer_up(to[0], to[1]);
    rt.frame(0.016);
    let moved = says(&rt, badge).expect("the badge still drawn");
    assert!((moved[0] - at[0]).abs() > 30.0 && (moved[1] - at[1]).abs() > 20.0, "the Map panned with the hand: {at:?} to {moved:?}");
    assert_eq!(rt.selected_text(), None, "a drag on the Map selects no words");
    // research selected by a double-click on its corner band, as path 2 has it, the inspector open beside it
    let double = |rt: &mut Runtime<App>, q: [f32; 2]| {
        rt.pointer_move(q[0], q[1]);
        rt.frame(0.016);
        rt.pointer_down(q[0], q[1]);
        rt.pointer_up(q[0], q[1]);
        rt.pointer_down(q[0], q[1]);
        rt.pointer_up(q[0], q[1]);
        rt.frame(0.016);
    };
    let r = rt.presented("map-n-0").expect("research drawn").0;
    double(&mut rt, [r[0] + r[2] * 0.6, r[1] + r[3] * 0.97]);
    assert_eq!(selected(&rt).as_deref(), Some("project:research"));
    settle(&mut rt);
    // the inspector's longest line, dragged across from its left end to its right
    let (ir, _) = rt.presented("map-inspector").expect("the inspector open");
    let line = rt
        .elements()
        .into_iter()
        .filter(|e| !e.exiting && e.role != "button" && e.text.as_ref().is_some_and(|t| t.chars().count() > 4))
        .filter(|e| e.rect[0] >= ir[0] && e.rect[0] + e.rect[2] <= ir[0] + ir[2] + 0.5 && e.rect[1] >= ir[1] && e.rect[1] + e.rect[3] <= ir[1] + ir[3] + 0.5 && e.rect[3] < 30.0)
        .max_by_key(|e| e.text.as_ref().map_or(0, |t| t.chars().count()))
        .expect("a line in the inspector");
    let (r, words) = (line.rect, line.text.unwrap_or_default());
    let y = r[1] + r[3] / 2.0;
    rt.pointer_move(r[0] + 1.0, y);
    rt.frame(0.016);
    rt.pointer_down(r[0] + 1.0, y);
    rt.frame(0.016);
    for k in 1..=6 {
        rt.pointer_move(r[0] + 1.0 + (r[2] - 2.0) * k as f32 / 6.0, y);
        rt.frame(0.016);
    }
    rt.pointer_up(r[0] + r[2] - 1.0, y);
    rt.frame(0.016);
    assert_eq!(rt.selected_text().as_deref(), Some(words.as_str()), "the inspector's line selected");
    // a double-click on research's words where the canvas draws them (its card's, not the inspector's nor the left
    // bar's same words): the inspector's selection goes, and no word is lit
    let inside = |r: [f32; 4], p: [f32; 2]| p[0] >= r[0] && p[0] <= r[0] + r[2] && p[1] >= r[1] && p[1] <= r[1] + r[3];
    let (canvas, _) = rt.presented("map-canvas").expect("the canvas");
    let (card, _) = rt.presented("map-n-0").expect("research's card");
    let on_canvas = |p: [f32; 2]| inside(canvas, p) && inside(card, p) && !inside(ir, p);
    let (word, on) = rt
        .elements()
        .into_iter()
        .filter(|e| !e.exiting && e.role != "button" && e.text.as_ref().is_some_and(|t| t.chars().count() > 4))
        .map(|e| (e.text.unwrap_or_default(), [e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0]))
        .find(|(_, p)| on_canvas(*p))
        .expect("research's words on the canvas");
    double(&mut rt, on);
    let lit = rt.selected_text();
    drop((rt, _held));
    // the timeline's canvas (lanes.rs `board`) as the cloud's: a double-click on a lane's name lights no word
    #[cfg(windows)]
    let lane_lit: Option<String> = None;
    #[cfg(not(windows))]
    let lane_lit = {
        let (_held, s, child, out) = lanes_app();
        let mut rt = window(s);
        rt.frame(0.016);
        graph_answered(&mut rt);
        let ([x, y, w, h], _) = rt.presented("lanes-canvas").expect("the timeline's canvas");
        let on = rt
            .elements()
            .into_iter()
            .filter(|e| !e.exiting && e.text.as_deref() == Some("docket-core"))
            .map(|e| [e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0])
            .find(|p| p[0] >= x && p[0] <= x + w && p[1] >= y && p[1] <= y + h)
            .expect("docket-core's lane");
        double(&mut rt, on);
        let lit = rt.selected_text();
        rt.state.projects[0].link = None;
        let _ = super::deep::tests::sent(child, &out);
        lit
    };
    assert_eq!([lit, lane_lit], [None, None], "a double-click on the Map's words ({word:?}) and on a lane's name (docket-core) lights no word");
}
