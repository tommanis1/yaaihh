//! The programs drawn on the Docket page and in the Inspect drawer: a coordd (v1) project's check and the release's
//! texts on a press, a rule or a thing pressed, a finding pressed, an agent's harness moment by moment, and the engine's
//! own page on a real coordd-v2. Made up: the projects, kinds, parties and record.

use super::{Built, Origin, BUILT, BUILT_IN, PICK, TOP};
use crate::kinds::KindForm;
use crate::policy_v2::page::tests::{press, under};
use crate::App;
use rune::runtime::Runtime;
use rune::{col, Cx, El};
use rune_kit::Kit;
use serde_json::json;

/// Whether an element keyed KEY is drawn under the one keyed WITHIN.
fn drawn_under(rt: &Runtime<App>, key: &str, within: &str) -> bool {
    rt.elements().into_iter().any(|e| !e.exiting && e.key.as_deref() == Some(key) && e.path.split('/').any(|k| k == within))
}

fn frames(rt: &mut Runtime<App>) {
    for _ in 0..4 {
        rt.frame(0.016);
    }
}

/// A coordd project on its Kinds page, at the Project scale: two kinds, one that does not parse, and REFUSALS.
fn coordd_project(refusals: Vec<String>) -> (std::sync::MutexGuard<'static, ()>, App) {
    let (held, mut s) = crate::view_cost::test_app();
    let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
    p.down = None;
    p.live.refusals = refusals;
    s.projects = vec![p];
    s.cur = 0;
    s.sel = crate::Sel::Kinds;
    let f = |kind: &str, text: &str, file: &str| -> KindForm { serde_json::from_value(json!({"kind": kind, "form": [], "text": text, "file": file})).unwrap() };
    s.tickets.kinds = Some(Ok(vec![
        f("checkup", "kind checkup\n  form due: Text\n  result Text\n  pattern\n    return due\n", "/p/kinds/checkup.kind"),
        f("broken", "this is not a kind", "/p/kinds/broken.kind"),
    ]));
    (held, s)
}

/// KEY, in the drawing's own view, brought to its middle and pressed.
fn press_drawn(rt: &mut Runtime<App>, key: &str) {
    let to = key.to_string();
    rt.update(move |_, tx| tx.scroll_center(to));
    for _ in 0..120 {
        let was = rt.find(key);
        rt.frame(0.016);
        if rt.find(key) == was {
            break;
        }
    }
    press(rt, key, None);
}

fn window(s: App) -> Runtime<App> {
    let mut rt = Runtime::new(s, crate::view, (1800.0, 1100.0), rune::Env::new(false, 1.0));
    frames(&mut rt);
    rt
}

/// The texts built in are the ones the release's engine loads, in its order, read in its top module.
#[test]
fn the_built_in_texts_are_the_release_s_in_its_order() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sh = std::fs::read_to_string(root.join("ops/release/cutover.sh")).unwrap();
    let list = |v: &str| sh.lines().find_map(|l| l.strip_prefix(&format!("{v}=\""))).unwrap().trim_end_matches('"').split(' ').map(str::to_string).collect::<Vec<_>>();
    let want: Vec<String> = list("STD").iter().map(|t| format!("std/{t}.docket")).chain(list("PKG").iter().map(|t| format!("defaults/{t}.docket"))).collect();
    assert_eq!(BUILT_IN.map(|(p, _)| p.to_string()).to_vec(), want);
    assert!(sh.contains(&format!("--top {TOP}")));
    #[cfg(unix)]
    assert_eq!(crate::v2::boot::TEXTS, BUILT_IN.map(|(p, _)| p));
    for (p, t) in BUILT_IN {
        assert_eq!(std::fs::read_to_string(root.join("docket").join(p)).unwrap(), t, "{p}");
    }
}

/// A coordd project's page names its engine and kinds, says each refusal and each kind that does not parse at its file,
/// and on a press draws the release's texts, which have no error.
#[test]
fn a_coordd_project_names_its_engine_and_its_findings_and_draws_the_release_s_on_a_press() {
    let (held, s) = coordd_project(vec!["/p/kinds/x.kind:3: step look names no call".into()]);
    let mut rt = window(s);
    assert!(under(&rt, "programs-v1", "coordd (v1) · 2 kinds in 2 files"));
    assert!(under(&rt, "programs-errors", "2 errors"));
    assert!(under(&rt, "programs-v1", "/p/kinds/x.kind:3") && under(&rt, "programs-v1", "step look names no call"));
    assert!(under(&rt, "programs-v1", "/p/kinds/broken.kind") && under(&rt, "programs-v1", "kind broken does not parse here"));
    assert!(rt.find("programs-board").is_none(), "the release's texts only on a press");
    press(&mut rt, "programs-built-in", None);
    frames(&mut rt);
    assert!(under(&rt, "programs-built-in-stack", "coordd-v2 in the release") && drawn_under(&rt, "programs-board", "programs-built-in-stack"));
    assert!(!drawn_under(&rt, "programs-errors", "programs-built-in-stack"), "the release's texts read, load and answer no moment with a power a version lacks");
    for (i, (p, _)) in BUILT_IN.iter().enumerate() {
        assert!(under(&rt, &format!("programs-text-{i}"), p));
    }
    // with no refusal and every kind parsing, the coordd project's badge says so (the tests' lock let go first: it is
    // taken again)
    drop((rt, held));
    let (_held, mut s) = coordd_project(Vec::new());
    s.tickets.kinds.as_mut().unwrap().as_mut().unwrap().pop();
    let rt = window(s);
    assert!(drawn_under(&rt, "programs-validated", "programs-v1"));
}

/// A rule pressed lights it and shows its declaration beside the drawing, the whole text on a press, its declaration
/// lit in it; a moment pressed shows where std.hooks states it; Put away closes the text and puts the pick away.
#[test]
fn a_rule_or_a_moment_pressed_shows_its_text_beside_the_drawing() {
    let (_held, mut s) = coordd_project(Vec::new());
    super::set(&mut s, BUILT, Some(String::new()));
    let mut rt = window(s);
    press(&mut rt, "programs-row-std.hooks-236", None);
    frames(&mut rt);
    assert!(rt.state.tickets.pic_open.contains(&format!("{PICK}row:std.hooks:236")));
    assert!(under(&rt, "programs-source", "std/hooks.docket:236") && under(&rt, "programs-source", "on Hook(agent, \"permission_asked\", payload)"));
    assert!(!under(&rt, "programs-source", "module std.hooks"), "its declaration alone");
    press(&mut rt, "programs-whole", None);
    frames(&mut rt);
    assert!(under(&rt, "programs-source", "module std.hooks") && under(&rt, "programs-source", "Its declaration"));
    press_drawn(&mut rt, "programs-node-moment:permission_asked");
    frames(&mut rt);
    assert!(under(&rt, "programs-source", "+Moment(\"permission_asked\")"));
    press(&mut rt, "programs-source-close", None);
    frames(&mut rt);
    assert!(rt.find("programs-source").is_none());
    // pressed again, a moment puts its pick away
    press_drawn(&mut rt, "programs-node-moment:permission_asked");
    frames(&mut rt);
    assert!(!rt.state.tickets.pic_open.iter().any(|k| k.starts_with(PICK)) && rt.find("programs-source").is_none());
}

fn finding_view(s: &App, _: &mut Cx<App>) -> El {
    let k = Kit::neutral(false);
    let b = Built::of(Origin::BuiltIn, vec![("w/a.docket".into(), Ok("module a\nFact P(x: Int))\n".into())), ("w/b.docket".into(), Err("cannot be read: gone".into()))], "a");
    col().w_full().children(super::diag_rows(&b.checked.diags, Some(&b), &k)).children(super::source::panel(s, &b, None, &k))
}

/// A finding pressed shows its text at its line, the finding under that line; a text that cannot be read is a finding
/// with no text to show.
#[test]
fn a_finding_pressed_shows_its_text_at_its_line() {
    let (_held, s) = crate::view_cost::test_app();
    let mut rt = Runtime::new(s, finding_view, (900.0, 600.0), rune::Env::new(false, 1.0));
    frames(&mut rt);
    assert!(rt.find("programs-diag-1").is_none(), "nothing to show of a text that cannot be read");
    press(&mut rt, "programs-diag-0", None);
    frames(&mut rt);
    // named without the folder both texts are in
    assert!(under(&rt, "programs-source", "a.docket:2") && !under(&rt, "programs-source", "w/a.docket") && under(&rt, "programs-source", "▲ does not read"));
    assert!(under(&rt, "programs-source", "Fact P(x: Int"));
}

/// The Docket tab of an agent's drawer: its harness's bridge, each version's moments with its own hook, what comes and
/// what is let, each rule there and whether it runs, and how his words reach it: by its bridge's delivery section and
/// its rules (Claude Code pushed, Codex in turns, agy pulled and kicked), else by what std.comms answers at a turn's
/// start (opencode, Modify). A rule pressed opens the drawing on it; its bridge pressed opens the Docket on it.
#[test]
fn the_inspect_drawer_s_docket_tab_says_each_moment_of_its_harness() {
    let (_held, s) = crate::inspect::tests::orch(crate::inspect::Tab::Docket);
    let mut rt = window(s);
    let tab = "inspect-docket-orch";
    for w in ["Its harness, claude-code, under coordd-v2", "claude-code 2.1.332", "UserPromptSubmit", "comes always", "lets deny, inject"] {
        assert!(under(&rt, tab, w), "{w}");
    }
    assert!(under(&rt, tab, "runs when its condition holds") && under(&rt, tab, "skipped: needs modify, which it tests for"));
    assert!(under(&rt, tab, "How his words reach it: pushed") && under(&rt, tab, "does Push-Channel"));
    let kinds = [
        ("codex", "codex 0.160.0", ["How his words reach it: turns", "Codex-Start-Turn"]),
        ("opencode", "opencode 1.18.34", ["std.comms answers turn_begins with Modify", "opencode 1.18.34"]),
        ("agy", "agy agy-1.5.18", ["How his words reach it: pulled", "does Agy-Kick"]),
    ];
    for (kind, version, reach) in kinds {
        rt.update(|s, _| s.projects[0].live.agents[0].kind = kind.into());
        rt.invalidate();
        frames(&mut rt);
        assert!(under(&rt, tab, version) && reach.iter().all(|w| under(&rt, tab, w)), "{kind}");
    }
    assert!(under(&rt, tab, "comes only in mode strict"), "agy's tool_called");
    assert!(under(&rt, tab, "· agy-1.5.18 — agy 1.5.18 itself, over ACP"), "an item of its bridge's list, a line of its own");
    // an item's line indented deeper goes on with it
    assert!(under(&rt, tab, "· kicked off — by the project when its session starts, or by him (agy-rpc's line); its first pull is due within the grace"));
    // a rule pressed opens the drawing on it, lit and brought into view though it is far down
    press(&mut rt, "inspect-docket-rule-agy-1.5.18-turn_begins-std.comms-161", None);
    for _ in 0..60 {
        rt.frame(0.016);
    }
    assert!(matches!(rt.state.sel, crate::Sel::Kinds) && super::picked(&rt.state, PICK) == Some("row:std.comms:161"));
    let (_, y) = rt.find("programs-row-std.comms-161").expect("the rule drawn");
    assert!((0.0..1100.0).contains(&y), "the rule in view, at {y}");
    rt.update(|s, _| s.sel = crate::Sel::Agent("orch".into()));
    frames(&mut rt);
    press(&mut rt, "inspect-docket-bridge", None);
    frames(&mut rt);
    assert!(matches!(rt.state.sel, crate::Sel::Kinds));
    assert_eq!(super::shown(&rt.state), Some(("std/bridges/agy.docket".to_string(), 0)));
    assert!(super::picked(&rt.state, BUILT).is_some() && under(&rt, "programs-source", "module std.bridges.agy"));
}

/// The stills, all made up: `STILL_OUT=DIR yaaihh-tests programs::tests::NAME --ignored --exact`.
#[test]
#[ignore = "a still: STILL_OUT"]
fn built_in_still() {
    crate::inspect::tests::shoot_at("programs-built-in.png", (1800.0, 1100.0), || {
        let (held, mut s) = coordd_project(vec!["/p/kinds/x.kind:3: step look names no call".into()]);
        drop(held);
        s.notice = None;
        super::set(&mut s, BUILT, Some(String::new()));
        // near the drawing's top: a still is not scrolled
        super::pick(&mut s, "row:std.hooks:236", "std/hooks.docket", 236);
        s
    });
}

#[test]
#[ignore = "a still: STILL_OUT"]
fn inspect_docket_still() {
    crate::inspect::tests::shoot("inspect-docket.png", || crate::inspect::tests::orch(crate::inspect::Tab::Docket).1);
}

#[test]
#[ignore = "a still: STILL_OUT"]
fn inspect_docket_agy_still() {
    crate::inspect::tests::shoot("inspect-docket-agy.png", || {
        let mut s = crate::inspect::tests::orch(crate::inspect::Tab::Docket).1;
        s.projects[0].live.agents[0].kind = "agy".into();
        s
    });
}

#[cfg(unix)]
mod live {
    use super::super::check::Level;
    use super::*;
    use crate::v2::boot::root;
    use std::time::{Duration, Instant};

    const WAIT: Duration = Duration::from_secs(20);

    /// Polled and drawn until DONE holds of the page.
    fn until(rt: &mut Runtime<App>, what: &str, done: impl Fn(&Runtime<App>) -> bool) {
        let (v2, t0) = (crate::v2::extension(), Instant::now());
        while !done(rt) {
            assert!(t0.elapsed() < WAIT, "{what}");
            rt.update(|s, tx| {
                (v2.poll)(s, tx);
            });
            rt.invalidate();
            rt.frame(0.016);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// The window opened on the engine in DIR, after w1 tried to set the project's mode, which is not its to set.
    fn opened(dir: &std::path::Path) -> (std::sync::MutexGuard<'static, ()>, App, [crate::v2::boot::Seen; 2]) {
        let (config, o, mut w) = crate::policy_v2::live::engine(dir);
        w.act("std.hooks.Reach", json!(["w1", "turn_begins"]));
        w.act("yaaihh.desk.Set-Project-Mode", json!(["w1", "safe"]));
        let (held, _) = crate::view_cost::test_app();
        // SAFETY: under the tests' lock, held
        unsafe { std::env::set_var("YAAIHH_CONFIG", &config) };
        let mut s = crate::policy_v2::live::window();
        (s.cur, s.sel) = (crate::v2::at(&s).expect("the engine's project"), crate::Sel::Kinds);
        (held, s, [o, w])
    }

    /// The engine's page counts its record (a thread opened, a refusal), checks the texts it loaded with no error, says
    /// why the refused act is not enabled, lists what holds of a Fact and why one of them holds, each from the engine.
    #[test]
    fn the_engine_s_page_counts_its_record_and_explains_a_refusal_and_a_fact() {
        let dir = std::env::temp_dir().join(format!("yaaihh-programs-{}", std::process::id()));
        let (held, s, parties) = opened(&dir);
        let mut rt = window(s);
        until(&mut rt, "the refusal drawn", |rt| under(rt, "programs-refusals", "Set-Project-Mode"));
        let tally = &rt.state.ext.get::<crate::v2::V2>().desk.tally;
        assert_eq!(tally.performed.get("yaaihh.desk.Open-Thread").map(|p| p.0), Some(1));
        assert_eq!(tally.refused.get("yaaihh.desk.Set-Project-Mode"), Some(&1));
        let b = super::super::engine(&rt.state).unwrap();
        assert!(b.checked.loaded.is_some() && b.checked.diags.iter().all(|d| d.level == Level::Note), "{:?}", b.checked.diags);
        assert!(drawn_under(&rt, "programs-board", "programs-engine-stack") && !drawn_under(&rt, "programs-errors", "programs-engine-stack"));
        let refused = rt.elements().into_iter().filter_map(|e| e.key).find(|k| k.starts_with("programs-refused-")).expect("its Why?");
        press(&mut rt, &refused, None);
        until(&mut rt, "why it is not enabled", |rt| under(rt, "programs-why", "is not enabled"));
        assert!(under(&rt, "programs-source", "Act Set-Project-Mode"), "its declaration beside it");
        // Opened-By at its line: one holds, w1's thread; why, from the engine: the act that stated it
        let desk = root().join("defaults/desk.docket").display().to_string();
        rt.update(|s, _| super::super::show(s, Some((&desk, 22))));
        until(&mut rt, "Opened-By's count", |rt| under(rt, "programs-held-yaaihh.desk.Opened-By", "1 hold"));
        press(&mut rt, "programs-held-yaaihh.desk.Opened-By", None);
        until(&mut rt, "its instances", |rt| rt.find("programs-why-yaaihh.desk.Opened-By-0").is_some());
        press(&mut rt, "programs-why-yaaihh.desk.Opened-By-0", None);
        until(&mut rt, "why it holds", |rt| under(rt, "programs-why", "holds") && under(rt, "programs-why", "stated at step"));
        drop((held, parties));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The window itself on the real engine, its Docket page open, a rule picked: `STILL_OUT=DIR`, alone in its process
    /// (it registers the extensions).
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn engine_still() {
        let dir = std::env::temp_dir().join(format!("yaaihh-programs-still-{}", std::process::id()));
        let (held, s, parties) = opened(&dir);
        drop(s);
        crate::ext::register(vec![crate::v2::extension(), crate::policy_v2::extension()]);
        crate::inspect::tests::shoot_at("programs-engine.png", (1800.0, 1100.0), || {
            let (v2, k) = (crate::v2::extension(), Kit::neutral(false));
            let mut s = crate::policy_v2::live::window();
            (s.cur, s.sel) = (crate::v2::at(&s).unwrap(), crate::Sel::Kinds);
            let hooks = root().join("std/hooks.docket").display().to_string();
            super::super::pick(&mut s, "row:std.hooks:236", &hooks, 236);
            let end = Instant::now() + WAIT;
            // drawn until the engine has answered what the page asks (the Duties' counts, the facts beside it)
            while Instant::now() < end {
                (v2.poll)(&mut s, &mut rune::Tx::default());
                drop(crate::kinds::page(&s, &s.projects[s.cur], &k));
                let b = super::super::engine(&s).unwrap();
                let asked = b.checked.drawing.rows.iter().filter(|r| r.rule == super::super::read::Rule::Duty).all(|r| crate::v2::holding(&s, &r.name).is_some());
                if asked && !s.ext.get::<crate::v2::V2>().desk.tally.refusals.is_empty() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            s
        });
        drop((held, parties));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
