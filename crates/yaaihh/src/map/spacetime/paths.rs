//! The time view on the window (docs/specs/spacetime-map.md): a press on a card on a plane behind selects it, the
//! switches are kept in ui.json, the causal rail's chips select, and the stills of Glass and Flat.

use super::*;
use crate::App;
use crate::map::paths::{map_poll, press, satellite_app, selected, settle, window};
use rune::Runtime;

/// The made-up record's last moment ([`satellite_app`]'s cloud).
fn last() -> i64 {
    let (g, _) = crate::map::cloud::cloud(&crate::map::cloud::tests::base(), None, "project:research", "research", &mut Default::default()).unwrap();
    *g.moments().last().unwrap()
}

/// [`satellite_app`]'s cloud drawn with SPACE, at the record's last moment.
fn at_last(space: Space) -> (std::sync::MutexGuard<'static, ()>, App) {
    let (held, mut s, _) = satellite_app();
    s.cloud.space = space;
    s.cloud.at = Some(last());
    (held, s)
}

/// Each node's rectangle as drawn now, by id.
fn rects(rt: &Runtime<App>) -> HashMap<String, [f32; 4]> {
    let g = rt.state.map.graph().unwrap().clone();
    rt.state.map.shown().iter().filter(|x| x.group.is_none()).map(|x| (g.node(x.i).id.clone(), x.r)).collect()
}

/// Glass draws its planes, and a thread older than the last hour on a plane behind, away from where one plane puts it:
/// a press there selects what is drawn there, inside that thread, where one plane has nothing of it.
#[test]
fn a_press_on_a_card_on_a_plane_behind_selects_that_card() {
    let (_held, s) = at_last(Space { wake: Wake::All, ..Space::default() });
    let mut rt = window(s);
    settle(&mut rt);
    let flat = {
        rt.update(|s, _| s.cloud.space.mode = Mode::Flat);
        settle(&mut rt);
        rects(&rt)
    };
    rt.update(|s, _| s.cloud.space.mode = Mode::Glass);
    settle(&mut rt);
    assert!(rt.presented("map-glass").is_some(), "the planes drawn");
    let g = rt.state.map.graph().unwrap().clone();
    let shown = rt.state.map.shown();
    let inside = |r: [f32; 4], q: [f32; 2]| q[0] >= r[0] && q[0] <= r[0] + r[2] && q[1] >= r[1] && q[1] <= r[1] + r[3];
    let (x, q) = shown
        .iter()
        .filter(|x| g.node(x.i).kind == Kind::Thread && x.group.is_none())
        .map(|x| (x, [x.r[0] + x.r[2] / 2.0, x.r[1] + 6.0]))
        .find(|(x, q)| flat.get(&g.node(x.i).id).is_some_and(|f| !inside(*f, *q)) && q[1] > 0.0)
        .expect("a thread drawn on a plane behind");
    let hit = crate::map::lod::under(&shown, q).map(|k| shown[k].i).expect("something drawn there");
    assert_eq!(anchor(&g, hit), x.i, "what is drawn there is of that thread");
    let a = rt.state.map.area;
    press(&mut rt, [a[0] + q[0], a[1] + q[1]]);
    assert_eq!(selected(&rt), Some(g.node(hit).id.clone()));
}

/// Flat and its wake, kept: written to ui.json and read back by a window that starts after; and the switches in the
/// cloud's row.
#[test]
fn the_time_switches_are_kept_and_read_back() {
    let (_held, s) = at_last(Space::default());
    let mut rt = window(s);
    assert_eq!(crate::map::paths::texts_in(&rt, "cloud-mode").iter().filter(|t| ["Glass", "Flat"].contains(&t.as_str())).count(), 2);
    assert!(crate::map::paths::texts_in(&rt, "cloud-wake").contains(&"24h".to_string()));
    let kept = Space { mode: Mode::Flat, wake: Wake::Hours2, ..Space::default() };
    rt.update(|s, _| s.cloud.space = kept);
    assert!(crate::state::write_now(&mut rt.state), "written");
    let mut back = crate::init();
    crate::state::restore(&mut back);
    assert_eq!(back.cloud.space, kept);
}

/// A press on Flat and on 2h, and Alt and the wheel's tilt, kept: the window's keeper (state.rs `keep`, each poll)
/// writes what [`crate::state::write_now`] writes.
#[test]
fn the_time_switches_pressed_are_kept() {
    let (_held, s) = at_last(Space::default());
    let mut rt = window(s);
    settle(&mut rt);
    for (key, words) in [("cloud-mode", "Flat"), ("cloud-wake", "2h")] {
        let (r, _) = rt.presented(key).expect(key);
        let at = rt.elements().into_iter().find(|e| e.text.as_deref() == Some(words) && e.rect[0] >= r[0] && e.rect[0] <= r[0] + r[2]).map(|e| [e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0]).expect(words);
        press(&mut rt, at);
    }
    rt.update(|s, _| s.cloud.space.turn(0.0, -100.0));
    let kept = rt.state.cloud.space;
    assert_eq!((kept.mode, kept.wake), (Mode::Flat, Wake::Hours2), "pressed");
    assert!(kept.tilt < Space::default().tilt, "tilted");
    assert!(crate::state::write_now(&mut rt.state), "written");
    let mut back = crate::init();
    crate::state::restore(&mut back);
    assert_eq!(back.cloud.space, kept);
}

/// A slice open whose nodes this graph has none of (another project, a record read again) draws no rail.
#[test]
fn a_slice_with_none_of_its_nodes_here_draws_no_rail() {
    let (_held, s) = at_last(Space { wake: Wake::All, ..Space::default() });
    let mut rt = window(s);
    rt.update(|s, _| s.cloud.causal = Some(("N-404.p1".into(), vec!["N-404.p1".into()])));
    settle(&mut rt);
    assert!(rt.presented("map-causal-panel").is_none() && rt.presented("map-causal").is_none(), "no rail");
}

/// The fold of questions and stops is worked out for a build only: frames with nothing new never do it, and a new
/// question does it once (consult-lead-c, N-1976 p8).
#[test]
fn frames_with_nothing_new_never_fold_the_tickets_again() {
    use crate::map::fold::CALLS;
    let (_held, s) = at_last(Space::default());
    let mut rt = window(s);
    settle(&mut rt);
    let before = CALLS.with(|c| c.get());
    for _ in 0..30 {
        rt.frame(0.016);
    }
    assert_eq!(CALLS.with(|c| c.get()), before, "30 frames, nothing new");
    rt.update(|s, _| {
        let live = &mut s.projects[0].live;
        let mut q = live.tickets.values().next().cloned().expect("a ticket");
        (q.id, q.n, q.parent, q.under) = ("Q-77".into(), 77, Some("N-1".into()), None);
        live.tickets.insert(77, q);
    });
    settle(&mut rt);
    assert_eq!(CALLS.with(|c| c.get()), before + 1, "a new question: one build");
}

/// The post with the longest causal past in the made-up record's cloud G at its last moment.
fn richest(g: &Graph) -> u32 {
    (0..g.nodes().len() as u32).filter(|i| g.node(*i).kind == Kind::Post).max_by_key(|i| (crate::map::causal::past(g, *i, last()).len(), *i)).expect("a post")
}

/// The causal slice of the post with the longest past: its past on a rail, oldest first, the post last; what is off
/// it faded; a chip's press selects its node; Esc closes it.
#[test]
fn the_causal_rail_shows_a_cards_past_and_its_chips_select() {
    let (_held, s) = at_last(Space { wake: Wake::All, ..Space::default() });
    let mut rt = window(s);
    settle(&mut rt);
    let g = rt.state.map.graph().unwrap().clone();
    let card = richest(&g);
    let id = g.node(card).id.clone();
    rt.update(|s, _| crate::map::causal::toggle(s, card));
    settle(&mut rt);
    let (of, ids) = rt.state.cloud.causal.clone().expect("open");
    assert_eq!(of, id);
    assert_eq!(ids.last(), Some(&id), "the card last");
    assert!(ids.len() >= 2, "{ids:?}");
    let shown = rt.state.map.shown();
    let on = crate::map::causal::held(&rt.state, &g).unwrap();
    assert!(shown.iter().filter(|x| !on.contains(&x.i)).all(|x| x.alpha <= crate::map::causal::OFF + 1e-4), "off the slice, faded");
    let (chip, _) = rt.presented("map-causal-0").expect("the oldest chip");
    let q = [chip[0] + chip[2] / 2.0, chip[1] + chip[3] / 2.0];
    let there = rt.element_at(q[0], q[1]).map(|p| p.path().to_string());
    press(&mut rt, q);
    assert_eq!(selected(&rt).as_deref(), Some(ids[0].as_str()), "the chip {chip:?}, at its middle {there:?}");
    rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: Default::default() });
    settle(&mut rt);
    assert!(rt.state.cloud.causal.is_none(), "Esc closes it");
}

/// What the next still of [`spacetime_stills`] draws: its space, its causal slice's card, its focus.
static SHOWN: std::sync::Mutex<Option<(Space, bool, Option<&'static str>)>> = std::sync::Mutex::new(None);

/// S's snapshot with item I-90 over his chat and the visual layer, and three agents on threads: one working, one
/// waiting on him, one idle (orbit.rs draws the outline and the chips).
fn with_item_and_agents(s: &mut App) {
    let live = &mut s.projects[0].live;
    let Some(like) = live.tickets.values().next().cloned() else { return };
    let mut put = |n: u64, id: &str, title: &str, under: Option<&str>| {
        let v = live.tickets.entry(n).or_insert_with(|| like.clone());
        if v.id != id {
            *v = like.clone();
            (v.id, v.n, v.title) = (id.to_string(), n, title.to_string());
        }
        v.under = under.map(String::from);
    };
    put(90, "I-90", "The Map in space and time", None);
    put(1, "N-1", "Your one group chat", Some("I-90"));
    put(3, "N-3", "The visual layer", Some("I-90"));
    let like = live.agents.first().cloned();
    for (n, (name, on, state)) in [("map-dev", "N-1.p3", "working"), ("reader", "N-3", "waiting on owner"), ("logins", "N-2", "idle")].into_iter().enumerate() {
        let Some(mut a) = live.agents.get(n).cloned().or_else(|| like.clone()) else { return };
        (a.name, a.on, a.state) = (name.to_string(), Some(on.to_string()), state.to_string());
        match live.agents.get_mut(n) {
            Some(x) => *x = a,
            None => live.agents.push(a),
        }
    }
}

fn shown_app() -> App {
    let (space, causal, focus) = SHOWN.lock().unwrap().unwrap_or((Space::default(), false, None));
    let (_held, mut s) = at_last(space);
    with_item_and_agents(&mut s);
    s.cloud.focus = focus.map(String::from);
    if causal {
        let (g, _) = crate::map::cloud::cloud(&crate::map::cloud::tests::base(), None, "project:research", "research", &mut Default::default()).unwrap();
        let i = richest(&g);
        s.cloud.causal = Some((g.node(i).id.clone(), crate::map::causal::past(&g, i, last()).into_iter().map(|n| g.node(n).id.clone()).collect()));
    }
    s
}

/// The owner's N-1976 p1 as stills, 1400 by 900, on the made-up record at its last moment: Glass (spacetime-1-glass),
/// Glass turned (spacetime-2-turned), Flat (spacetime-3-flat), Glass at 2h (spacetime-4-wake), Glass with the causal
/// slice of the post with the longest past open (spacetime-5-causal), Flat with it (spacetime-6-flat-causal), and his
/// post N-1.p3 the focus, the other items' threads small pills (spacetime-7-focus); an item over two threads and three
/// agents on threads in each ([`with_item_and_agents`]).
/// `STILL_OUT=DIR yaaihh-tests map::spacetime::paths::spacetime_stills --ignored --exact`.
#[test]
#[ignore]
fn spacetime_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let all = Space { wake: Wake::All, ..Space::default() };
    let stills: [(&str, Space, bool, Option<&'static str>); 7] = [
        ("spacetime-1-glass", all, false, None),
        ("spacetime-2-turned", Space { tilt: 48.0, yaw: 18.0, ..all }, false, None),
        ("spacetime-3-flat", Space { mode: Mode::Flat, ..all }, false, None),
        ("spacetime-4-wake", Space { wake: Wake::Hours2, ..Space::default() }, false, None),
        ("spacetime-5-causal", all, true, None),
        ("spacetime-6-flat-causal", Space { mode: Mode::Flat, ..all }, true, None),
        ("spacetime-7-focus", all, false, Some("N-1.p3")),
    ];
    for (name, space, causal, focus) in stills {
        *SHOWN.lock().unwrap() = Some((space, causal, focus));
        let f = out.join(format!("{name}.json"));
        std::fs::write(&f, "[]").unwrap();
        let args = vec!["--snap".to_string(), out.join(format!("{name}.png")).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into(), "--script".into(), f.display().to_string()];
        let spec = rune::AppSpec { poll: Some(map_poll), on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), shown_app, crate::view) };
        rune::App::new(spec).args(args).try_run().unwrap();
    }
}
