//! The Policy page on the real engine: the window's link asks each question of coordd-v2 and the page draws its answers.

use super::page::tests::{pick, picked, plain_words, press, under};
use super::page::At;
use crate::v2::boot::{boot, texts, Seen, OPEN, SEATED};
use crate::App;
use rune::runtime::Runtime;
use rune::{col, Cx, El};
use rune_kit::Kit;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The longest the page waits for its answers before the test fails.
const WAIT: Duration = Duration::from_secs(20);

fn view(s: &App, _: &mut Cx<App>) -> El {
    let k = Kit::neutral(false);
    col().w_full().h_full().fill(k.c.bg_base).child((super::extension().page)(s, &k, "").1)
}

/// The row of the package's one fact, and the line under it that says the engine's own word at a party.
const ROW: &str = "policy-row-Desk.Mode-Of";
const NOW: &str = "policy-now-Desk.Mode-Of";

/// How long it took until every answer drawn has come and `done` holds of the page.
fn until(rt: &mut Runtime<App>, done: impl Fn(&Runtime<App>) -> bool) -> Duration {
    let (v2, t0) = (crate::v2::extension(), Instant::now());
    loop {
        rt.update(|s, tx| {
            (v2.poll)(s, tx);
        });
        rt.invalidate();
        rt.frame(0.016);
        if !under(&*rt, "policy-v2", "…") && done(&*rt) {
            return t0.elapsed();
        }
        let t: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect();
        assert!(t0.elapsed() < WAIT, "the page did not come to it: {t:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The engine booted in DIR, the owner (o, connected first) seated and the project set to yolo, w1 (second) having
/// opened a thread and set to safe; the projects file that names it, its texts, its top module and the window's party
/// (o1: every later connection), as the daemon has them.
pub(crate) fn engine(dir: &Path) -> (PathBuf, Seen, Seen) {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    let socket = boot(dir);
    let mut o = Seen::open(&socket);
    o.act(SEATED, json!(["o1"]));
    o.act("yaaihh.desk.Set-Project-Mode", json!(["o1", "yolo"]));
    let mut w = Seen::open(&socket);
    w.act(OPEN, json!(["w1", "t1", "A thread of its own"]));
    o.act("yaaihh.desk.Set-Agent-Mode", json!(["o1", "w1", "safe"]));
    let texts: Vec<String> = texts().iter().map(|p| p.display().to_string()).collect();
    let config = dir.join("projects.toml");
    std::fs::write(&config, format!("[v2]\nname = \"yaaihh\"\nsocket = \"{}\"\ntop = \"yaaihh.project\"\nparty = \"o1\"\ntexts = {texts:?}\n", socket.display())).unwrap();
    (config, o, w)
}

/// The owner sets the project to yolo and w1 to safe; the page reads each level from the engine, then follows a change.
#[test]
fn the_page_draws_each_level_as_the_engine_answers_it_and_follows_a_change() {
    let (config, mut o, w) = engine(&std::env::temp_dir().join(format!("yaaihh-v2-policy-{}", std::process::id())));
    let (held, mut rt) = opened(&config);
    // the window's one load, at setup: the texts read, loaded and checked, and the page's rows read from them
    let c = crate::v2::config().expect("the projects file's [v2]");
    let t0 = Instant::now();
    let (modules, loaded, _) = crate::v2::plan(&c);
    let rows = super::plan::plan(&modules, &loaded.expect("the package loads"), &c.top).rows.len();
    let load = t0.elapsed();

    // the project's level first: its own mode, set by its act, drawn picked
    let first = until(&mut rt, |rt| under(rt, "policy-text", "The text the engine runs") && picked(rt, ROW).is_some());
    assert_eq!(picked(&rt, ROW), Some(2), "the project's own: yolo");
    press(&mut rt, "policy-at-Desk.Install-Mode", None);
    until(&mut rt, |rt| under(rt, ROW, "Inherited from the default"));
    press(&mut rt, "policy-at-party-w1", None);
    until(&mut rt, |rt| picked(rt, ROW) == Some(0) && under(rt, NOW, "Now: Safe, set for the agent"));
    // o1 runs nowhere, so Mode-Of holds of it for no mode; its level inherits the project's
    press(&mut rt, "policy-at-party-o1", None);
    until(&mut rt, |rt| under(rt, ROW, "Inherited from the project") && under(rt, NOW, "Not running now; it would be Yolo"));

    // a performed line: the answers are asked again, and the page draws the project's new mode
    o.act("yaaihh.desk.Set-Project-Mode", json!(["o1", "safe"]));
    let again = until(&mut rt, |rt| under(rt, NOW, "it would be Safe, from the project"));
    plain_words(&rt, ROW);
    // two values each: at the project's level Install-Mode and Project-Mode; at o1's those, Agent-Mode and Mode-Of
    eprintln!("policy page on the engine: plan of {} texts ({rows} row) loaded in {load:?}; 4 questions answered in {first:?} at first, 8 again in {again:?} after one line", texts().len());
    drop((held, o, w));
}

/// The window acting as o1 sets the project's mode and w1's by the package's acts, and gives o1 a mode of its own by
/// Override; w1's own try is refused, as its record says.
#[test]
fn the_window_sets_a_level_by_the_package_s_act_and_draws_a_refusal_from_the_record() {
    let (config, o, mut w) = engine(&std::env::temp_dir().join(format!("yaaihh-v2-policy-acts-{}", std::process::id())));
    let (held, mut rt) = opened(&config);
    until(&mut rt, |rt| picked(rt, ROW) == Some(2));
    // w1 is not the owner, so the act's condition fails and the engine writes its refusal
    w.act("yaaihh.desk.Set-Project-Mode", json!(["w1", "safe"]));
    until(&mut rt, |rt| under(rt, ROW, "w1's change was refused"));
    pick(&mut rt, ROW, 0);
    until(&mut rt, |rt| picked(rt, ROW) == Some(0) && !under(rt, "policy-refused-Desk.Mode-Of", ""));
    press(&mut rt, "policy-at-party-w1", None);
    until(&mut rt, |rt| picked(rt, ROW) == Some(0));
    pick(&mut rt, ROW, 1);
    until(&mut rt, |rt| picked(rt, ROW) == Some(1) && under(rt, NOW, "Now: Guarded, set for the agent"));
    plain_words(&rt, ROW);
    // o1 has no mode of its own: Override writes the project's, safe now, as its own
    press(&mut rt, "policy-at-party-o1", None);
    until(&mut rt, |rt| under(rt, ROW, "Inherited from the project"));
    press(&mut rt, "policy-override-Desk.Mode-Of", None);
    until(&mut rt, |rt| picked(rt, ROW) == Some(0) && under(rt, NOW, "Now: Safe, set for the agent"));
    drop((held, o, w));
}

/// The window as it opens on the projects file CONFIG, the engine's extension and the Policy page's set up.
fn opened(config: &Path) -> (std::sync::MutexGuard<'static, ()>, Runtime<App>) {
    let (held, _) = crate::view_cost::test_app();
    // SAFETY: under the tests' lock, held
    unsafe { std::env::set_var("YAAIHH_CONFIG", config) };
    (held, Runtime::new(window(), view, (1100.0, 760.0), rune::Env::new(false, 1.0)))
}

/// The window with the engine's extension and the Policy page's set up on the projects file the environment names.
pub(crate) fn window() -> App {
    let mut s = crate::init();
    let (v2, policy) = (crate::v2::extension(), super::extension());
    (v2.init)(&mut s);
    (policy.init)(&mut s);
    (v2.setup)(&mut s, std::sync::Arc::new(|| {}));
    s
}

/// Whether every question the page asks of S's engine is answered, for both parties, its loaded line come.
fn answered(s: &App) -> bool {
    let (v, p) = (s.ext.get::<crate::v2::V2>(), s.ext.get::<super::Policy>());
    let Some(plan) = p.plan.get() else { return false };
    let mut parties = v.desk.parties();
    let two = parties.len() == 2 && v.desk.loaded_line().is_some();
    parties.push(String::new());
    let ask = |q: &str| crate::v2::query(s, q);
    let waits = |r: &super::ask::Resolved| r.holds == super::ask::Said::Waiting || r.levels.contains(&super::ask::Said::Waiting);
    two && plan.rows.iter().all(|r| parties.iter().all(|party| !waits(&super::ask::resolve(&ask, r, party))))
}

/// The window as it opens on the engine, its answers come, the engine's project shown on its Policy tab.
fn live_window() -> App {
    let (v2, policy) = (crate::v2::extension(), super::extension());
    let mut s = window();
    let (k, end) = (Kit::neutral(false), Instant::now() + WAIT);
    while !answered(&s) && Instant::now() < end {
        (v2.poll)(&mut s, &mut rune::Tx::default());
        // drawn, the page asks what it has not yet
        drop((policy.page)(&s, &k, ""));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(answered(&s), "the page's answers did not come in {WAIT:?}: no still of a page still asking");
    // o1's level: what it inherits, Override, and the engine's own word
    s.ext.get_mut::<super::Policy>().at = Some(At::Party("o1".into()));
    s.cur = s.ext.get::<crate::v2::V2>().at.expect("the engine's project");
    s.sel = crate::Sel::Ext(super::NAME.into(), String::new());
    s
}

/// The window itself on the real engine, its Policy tab open: `STILL_OUT=DIR`, alone in its process (it registers the extensions).
#[test]
#[ignore]
fn policy_live_still() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let (config, o, w) = engine(&std::env::temp_dir().join(format!("yaaihh-v2-policy-still-{}", std::process::id())));
    let (held, _) = crate::view_cost::test_app();
    // SAFETY: under the tests' lock, held
    unsafe { std::env::set_var("YAAIHH_CONFIG", &config) };
    crate::ext::register(vec![crate::v2::extension(), super::extension()]);
    std::fs::create_dir_all(&out).unwrap();
    let png = PathBuf::from(out).join("policy-live.png").display().to_string();
    let args = ["--snap", &png, "--size", "1400x900", "--at", "1.0"].map(str::to_string).to_vec();
    rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), live_window, crate::view) }).args(args).try_run().unwrap();
    drop((held, o, w));
}
