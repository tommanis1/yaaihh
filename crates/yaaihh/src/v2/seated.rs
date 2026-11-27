//! His window seated at the real window driver: his words on an engine thread reach it on the thread's own name and to
//! its agent, the box's message is answered, and what waits for him is decided by its buttons and ended by the record.

// Made up: the parties, the thread, the words and the record's lines; the driver is yaaihh's, read from its file.

use super::asked::{APPROVAL, BUDGET};
use super::seat::Seat;
use super::{boot, plan, show, Config, V2};
use crate::message_box::{thread_place, Target, Way};
use crate::outgoing::Is;
use crate::policy_v2::page::tests::{press, under};
use docket_driver_window::{Window, SAYS, SEATED};
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::{Instance, Value};
use rune::runtime::Runtime;
use rune::TextEdit;
use serde_json::{json, Value as Json};
use std::path::{Path, PathBuf};
use std::sync::MutexGuard;
use std::time::{Duration, Instant};

const HIS: &str = "Keep the outline to two levels.";

fn act(ty: &str, args: Json) -> Json {
    json!({"performed": {"act": {"inst": ty, "args": args}, "party": null, "run": null}})
}

fn ask(op: &str, args: Json, run: &str) -> Json {
    json!({"effect": {"op": op, "args": args, "run": run, "head": "", "key": ""}})
}

/// Until `done`, or the test fails as `what`.
fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < end, "{what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// What the driver has ready, once it has `n` answers and Events.
fn ready(w: &mut Window, n: usize) -> (Vec<(u64, Reply)>, Vec<Instance>) {
    let (mut answers, mut events) = (Vec::new(), Vec::new());
    until("the driver has what was sent", || {
        let (a, e) = w.ready();
        answers.extend(a);
        events.extend(e);
        answers.len() + events.len() >= n
    });
    (answers, events)
}

/// His window on a record where w1 opened a thread and two questions wait for him, seated at the socket in DIR.
fn window(dir: &Path) -> (MutexGuard<'static, ()>, crate::App) {
    let (held, mut s) = crate::view_cost::test_app();
    let texts = boot::texts().iter().map(|p| p.display().to_string()).collect();
    let threads = Some(boot::root().join("defaults/threads.toml").display().to_string());
    let config = Config { texts, threads, socket: Some(dir.join("engine.sock").display().to_string()), ..Config::default() };
    let map = plan(&config).2.expect("yaaihh's threads file");
    let mut v = V2 { config: Some(config), ..V2::default() };
    let lines = [
        act("std.comms.Owner-Seated", json!(["o1"])),
        act("yaaihh.desk.Open-Thread", json!(["w1", "t1", "The outline's depth"])),
        ask(APPROVAL, json!(["w2", "A note for w2."]), "r1"),
        ask(BUDGET, json!(["front", "wall_ms", 60000]), "r2"),
        act("yaaihh.desk.Open-Thread", json!(["o1", "t2", "His own"])),
    ];
    for (at, l) in lines.into_iter().enumerate() {
        v.desk.take(at as u64, l);
    }
    let i = super::add_project(&mut s, "yaaihh");
    show(&mut s, i, v.desk.tickets(&map));
    v.at = Some(i);
    v.seat = Some(Seat::new(crate::engine_v2::Reach::Socket(dir.join("window.sock")).connect(), std::sync::Arc::new(|| {})));
    s.ext.insert(v);
    (held, s)
}

#[test]
fn his_words_and_decisions_go_through_his_seat_to_the_window_driver() {
    let dir = std::env::temp_dir().join(format!("yaaihh-v2-seated-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut file: Json = serde_json::from_str(&std::fs::read_to_string(boot::root().join("defaults/window.json")).unwrap()).unwrap();
    let paths = [("owner", json!("o1")), ("socket", json!(dir.join("window.sock"))), ("words", json!(dir.join("his-words")))];
    file.as_object_mut().unwrap().extend(paths.map(|(k, v)| (k.to_string(), v)));
    let mut w = Window::new(docket_driver_window::Config::read(&file.to_string()).unwrap(), Box::new(|_: &std::os::unix::net::UnixStream| Some("o1".into()))).unwrap();

    // the questions carried by the driver as the engine carries them
    let (_held, mut s) = window(&dir);
    let i = super::at(&s).expect("the engine's project");
    assert_eq!((w.carry(APPROVAL, &[], 2), w.carry(BUDGET, &[], 3)), (Reply::Later, Reply::Later));
    assert_eq!(ready(&mut w, 1).1, [Instance { ty: SEATED.into(), args: vec![Value::text("o1")] }], "seated as his window starts");

    // his words in the thread's box: to w1, who opened it, on the engine's name for the thread
    s.composers.insert((i, thread_place("N-1")), TextEdit::new(HIS));
    crate::message_box::send(&mut s, &Target::Thread { project: i, t: "N-1".into(), post: None, way: Way::Here });
    assert_eq!(s.notice, None);
    let events = ready(&mut w, 3).1;
    assert_eq!(std::fs::read_to_string(dir.join("his-words")).unwrap(), format!("{HIS}\n"), "his words kept by the driver");
    let got: Vec<(&str, &Value)> = events.iter().map(|e| (e.ty.as_str(), &e.args[1])).collect();
    assert_eq!(&got[..2], [(SAYS, &Value::text("w1")), ("yaaihh.desk.Owner-Posts", &Value::text("t1"))]);
    assert_eq!(events[2].ty, "yaaihh.desk.Written");
    until("the box's answer", || super::seat::drain(&mut s) && s.outgoing.list.last().is_some_and(|o| o.is != Is::Sending));
    assert_eq!(s.outgoing.list.last().unwrap().is, Is::Sent, "taken: his message waits for its copy in the record");

    // on a thread he opened, with no agent on it yet, his words would reach no one: kept, and why
    s.composers.insert((i, thread_place("N-2")), TextEdit::new(HIS));
    crate::message_box::send(&mut s, &Target::Thread { project: i, t: "N-2".into(), post: None, way: Way::Here });
    assert!(matches!(&s.outgoing.list.last().unwrap().is, Is::NotSent(why) if why.starts_with("no agent here yet")), "{:?}", s.outgoing.list.last().unwrap().is);
    assert_eq!(w.ready(), (vec![], vec![]), "nothing went to the driver");

    // the two questions, decided by their buttons; a grant needs its new limit first
    let view: fn(&crate::App, &mut rune::Cx<crate::App>) -> rune::El = |s, _| super::asked::el(s, &rune_kit::Kit::neutral(false));
    let mut rt = Runtime::new(s, view, (900.0, 700.0), rune::Env::new(false, 1.0));
    rt.frame(0.0);
    assert!(under(&rt, "v2-asked-head", "2 waiting for you") && under(&rt, "v2-ask-2", "Words to w2 wait for your approval"));
    assert!(under(&rt, "v2-ask-3", "front over budget wall_ms, needs 60000 more"));
    press(&mut rt, "v2-grant-3", None);
    assert!(under(&rt, "v2-not-3", "The new limit is a whole number"), "nothing sent without it");
    rt.update(|s, _| drop(s.ext.get_mut::<V2>().edits.insert((3, "limit"), TextEdit::new("120000"))));
    rt.frame(0.016);
    press(&mut rt, "v2-approve-2", None);
    press(&mut rt, "v2-grant-3", None);
    let approved = Value::ctor("std.control.Approved", vec![Value::text("A note for w2.")]);
    let granted = Value::ctor("std.budgets.Granted", vec![Value::Int(120_000)]);
    let answer = |value| Reply::Answer { value, costs: vec![], events: vec![] };
    assert_eq!(ready(&mut w, 2).0, [(2, answer(approved)), (3, answer(granted))]);
    assert!(under(&rt, "v2-sent-2", "Sent") && under(&rt, "v2-sent-3", "Sent"));

    // the record's answer ends the question
    rt.update(|s, _| assert!(s.ext.get_mut::<V2>().desk.take(5, json!({"answered": {"line": 2, "value": "x"}}))));
    rt.frame(0.016);
    assert!(under(&rt, "v2-asked-head", "1 waiting for you") && !rt.elements().iter().any(|e| !e.exiting && e.key.as_deref() == Some("v2-ask-2")));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The Engine page with what waits for him, as an image: `STILL_OUT=DIR`, one process.
#[test]
#[ignore]
fn asked_still() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let _ = std::fs::remove_file(out.join("window.sock"));
    // something listens where the driver would, so his window is connected
    let _listening = std::os::unix::net::UnixListener::bind(out.join("window.sock")).unwrap();
    let args = ["--snap", &out.join("v2-asked.png").display().to_string(), "--size", "1000x720", "--at", "1.0"].map(str::to_string).to_vec();
    let init = || window(&PathBuf::from(std::env::var("STILL_OUT").unwrap())).1;
    let view: fn(&crate::App, &mut rune::Cx<crate::App>) -> rune::El = |s, _| {
        let k = rune_kit::Kit::neutral(false);
        rune::col().w_full().h_full().fill(k.c.bg_base).child(super::page(s, &k, "").1)
    };
    rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1000.0, 720.0), init, view) }).args(args).try_run().unwrap();
    let _ = std::fs::remove_file(out.join("window.sock"));
}
