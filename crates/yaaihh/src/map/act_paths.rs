//! The Map's two acts on what it draws: a rail step's Rewind / Branch from here (ribbon.rs), and a brain
//! satellite's suggestions ready for his Approve or Reject (brain.rs). Each through the window, to the request it sends.

use super::paths::{map_poll, press, satellite_app, settle, texts_in, window};
use crate::App;
use coord_types::Request;
use rune::runtime::Runtime;

/// The requests project 0 sends, written by a child to a file; the file read back.
struct Asked {
    _child: std::process::Child,
    out: std::path::PathBuf,
}

impl Asked {
    fn on(s: &mut App, name: &str) -> Asked {
        let out = std::env::temp_dir().join(format!("yaaihh-{name}-{}.jsonl", std::process::id()));
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        Asked { _child: child, out }
    }

    /// The requests sent by now, waiting for the Nth to be written.
    fn sent(&self, n: usize) -> Vec<Request> {
        let read = || std::fs::read_to_string(&self.out).unwrap_or_default().lines().filter_map(|l| serde_json::from_value(serde_json::from_str::<serde_json::Value>(l).ok()?["req"].take()).ok()).collect::<Vec<Request>>();
        for _ in 0..200 {
            if read().len() >= n {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        read()
    }
}

impl Drop for Asked {
    fn drop(&mut self) {
        _ = std::fs::remove_file(&self.out);
    }
}

/// The rail of [`act_stills`] and its test: pr's, its brain's satellite's history; pr a Claude Code agent.
const RAIL: &str = "agent:pr";

fn pr_is_claude(s: &mut App) {
    let a = s.projects[0].live.agents.iter_mut().find(|a| a.name == "pr").expect("pr");
    a.kind = "claude".into();
}

/// The middle of what KEY is drawn at, pressed.
fn press_key(rt: &mut Runtime<App>, key: &str) {
    let (b, _) = rt.presented(key).unwrap_or_else(|| panic!("{key} not drawn"));
    press(rt, [b[0] + b[2] / 2.0, b[1] + b[3] / 2.0]);
    rt.frame(0.016);
}

/// [`RAIL`] unfolded, and the first step drawn that an agent branches at: its number on the rail (from 1), its time
/// and the agent.
fn branching_step(rt: &mut Runtime<App>) -> (usize, i64, String) {
    press_key(rt, &format!("map-chip-{RAIL}"));
    let g = rt.state.map.graph().unwrap().clone();
    let chat = g.find_in(0, RAIL).expect("pr's node");
    let steps = super::ribbon::steps(&g, chat, None);
    (1..=steps.len())
        .filter(|k| rt.presented(&format!("map-step-{RAIL}-{k}")).is_some())
        .find_map(|k| super::ribbon::branch_of(&rt.state, &g, chat, &steps[k - 1]).map(|a| (k, steps[k - 1].at, a)))
        .expect("a step of pr's rail it branches at")
}

/// pr's rail unfolded at the whole: a press on a step it branches at selects it and opens "Rewind / Branch
/// from here" under the rail, saying whose conversation it cuts and by when; its press asks coordd for a fork of that
/// agent at the step's time (`ForkAgent`'s `at`), and the action closes. Esc closes it before it folds the rail; the
/// step again closes it; a coordd before `REWIND_PROTOCOL` shows none.
#[test]
fn a_rail_step_opens_rewind_and_its_press_asks_for_a_fork_cut_at_the_steps_time() {
    let (_held, mut s, _) = satellite_app();
    s.projects[0].protocol = coord_types::PROTOCOL;
    pr_is_claude(&mut s);
    let asked = Asked::on(&mut s, "rewind");
    let mut rt = window(s);
    settle(&mut rt);
    rt.frame(0.016);
    let (k, at, agent) = branching_step(&mut rt);
    press_key(&mut rt, &format!("map-step-{RAIL}-{k}"));
    assert_eq!(rt.state.cloud.rewind, Some((RAIL.to_string(), k - 1)));
    let row = texts_in(&rt, &format!("map-rewind-row-{RAIL}"));
    assert!(row.iter().any(|t| t == "Rewind / Branch from here"), "{row:?}");
    assert!(row.iter().any(|t| t.starts_with(&format!("{agent}'s conversation, up to "))), "{row:?}");
    // the step again closes it; once more opens it; Esc closes it and leaves the rail unfolded
    press_key(&mut rt, &format!("map-step-{RAIL}-{k}"));
    assert!(rt.state.cloud.rewind.is_none() && rt.presented(&format!("map-rewind-row-{RAIL}")).is_none());
    press_key(&mut rt, &format!("map-step-{RAIL}-{k}"));
    rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: Default::default() });
    rt.frame(0.016);
    assert!(rt.state.cloud.rewind.is_none() && rt.state.cloud.unfolded.contains(RAIL), "Esc closes the action first");
    press_key(&mut rt, &format!("map-step-{RAIL}-{k}"));
    press_key(&mut rt, &format!("map-rewind-{RAIL}-{k}"));
    assert!(rt.state.cloud.rewind.is_none() && rt.state.projects[0].forking.is_some(), "asked, the action closed");
    let sent = asked.sent(1);
    assert!(matches!(sent.last(), Some(Request::ForkAgent { from, at: Some(t), new: None, .. }) if *from == agent && *t == at), "{sent:?}");
    // an older coordd: no step branches
    rt.state.projects[0].protocol = coord_types::REWIND_PROTOCOL - 1;
    let g = rt.state.map.graph().unwrap().clone();
    let chat = g.find_in(0, RAIL).unwrap();
    assert!(super::ribbon::steps(&g, chat, None).iter().all(|x| super::ribbon::branch_of(&rt.state, &g, chat, x).is_none()));
}

/// Two of pr's suggestions waiting for his approval, as coordd's `BrainView::ready` says them.
fn ready(s: &mut App, passes: &[coord_types::TicketView]) {
    let pr = passes.iter().find(|v| v.form["agent"] == "pr").expect("a pass of pr's");
    let one = |halt, post, words: &str| coord_types::BrainSuggestion { ticket: pr.n, id: pr.id.clone(), halt, post, words: words.into() };
    let a = s.projects[0].live.agents.iter_mut().find(|a| a.name == "pr").unwrap();
    a.brain.as_mut().unwrap().ready = vec![one(1, 3, "Read the lock's holder before you retry"), one(2, 4, "The test you skipped covers this")];
}

/// pr's brain with two posts halted for him: its satellite says "brain: ready (2 suggestions)", the oldest's words
/// beside it; Approve lets that halt go (`Approve`), Reject drops it (`HaltReject`). At a past moment, or from a coordd
/// before `BRAIN_READY_PROTOCOL`, it says what its passes came to.
#[test]
fn a_brains_ready_suggestions_stand_on_its_satellite_with_approve_and_reject() {
    let (held, mut s, passes) = satellite_app();
    s.projects[0].protocol = coord_types::PROTOCOL;
    ready(&mut s, &passes);
    let asked = Asked::on(&mut s, "ready");
    let mut rt = window(s);
    settle(&mut rt);
    rt.frame(0.016);
    let said = texts_in(&rt, "map-brain-agent:pr");
    assert!(said.windows(2).any(|w| w == ["pr", "brain: ready (2 suggestions)"]), "{said:?}");
    assert!(said.iter().any(|t| t.starts_with("“Read the lock")), "{said:?}");
    // its suggestion on a second line, cut to the room left of its node: the satellite covers no card
    let g = rt.state.map.graph().unwrap().clone();
    let node = rt.state.map.cam.on_screen(&g.node(g.find_in(0, "agent:pr").unwrap()).rect);
    let (pill, _) = rt.presented("map-brain-agent:pr").expect("pr's satellite drawn");
    assert!(pill[0] + pill[2] <= node[0], "{pill:?} ends left of its node {node:?}");
    assert!(texts_in(&rt, "map-brain-agent:w2").iter().all(|t| !t.contains("ready")), "w2 has none");
    let (id, tn) = rt.state.projects[0].live.agents.iter().find(|a| a.name == "pr").and_then(|a| a.brain.as_ref()).map(|b| (b.ready[0].id.clone(), b.ready[0].ticket)).unwrap();
    press_key(&mut rt, "map-brain-approve-pr");
    press_key(&mut rt, "map-brain-reject-pr");
    let sent = asked.sent(2);
    assert!(matches!(&sent[..], [Request::Approve { t, n: 1 }, Request::HaltReject { ticket, halt: 1, reason: None }] if *t == id && *ticket == tn), "{sent:?}");
    // an older coordd's window
    // the test lock let go before the next window takes it
    drop((rt, held));
    let (_held, mut s, passes) = satellite_app();
    s.projects[0].protocol = coord_types::BRAIN_READY_PROTOCOL - 1;
    ready(&mut s, &passes);
    let mut rt = window(s);
    settle(&mut rt);
    rt.frame(0.016);
    assert!(texts_in(&rt, "map-brain-agent:pr").iter().all(|t| !t.contains("ready")), "an older coordd: no ready state");
}

/// The window of [`act_stills`]: [`satellite_app`] at this coordd's protocol, pr's two suggestions ready, and with
/// `BRANCHING` its fork asked, its action open at [`RAIL`]'s step [`BRANCH_AT`].
fn act_shown() -> App {
    let (_held, mut s, passes) = satellite_app();
    s.projects[0].protocol = coord_types::PROTOCOL;
    ready(&mut s, &passes);
    pr_is_claude(&mut s);
    if let Some(k) = *BRANCH_AT.lock().unwrap() {
        s.cloud.unfolded.insert(RAIL.into());
        s.cloud.rewind = Some((RAIL.into(), k - 1));
        let (_tx, rx) = std::sync::mpsc::channel();
        s.projects[0].forking = Some(rx);
    }
    s
}

/// The step of [`act_shown`]'s open action, from 1; none for a still with none open.
static BRANCH_AT: std::sync::Mutex<Option<usize>> = std::sync::Mutex::new(None);

/// The two acts as stills, 1400 by 900: pr's rail unfolded, a step pressed, its Rewind / Branch from here open
/// (rewind-1-action); its fork asked, "Branching…" (rewind-2-branching); pr's brain ready with two suggestions,
/// Approve and Reject on its satellite (brain-ready).
/// `STILL_OUT=DIR yaaihh-tests map::act_paths::act_stills --ignored --exact`.
#[test]
#[ignore]
fn act_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let (k, _, _) = {
        let mut rt = window(act_shown());
        settle(&mut rt);
        rt.frame(0.016);
        branching_step(&mut rt)
    };
    let click = |t: f32, key: &str| format!(r#"{{"t": {t}, "type": "click", "key": "{key}"}}"#);
    let step = format!("map-step-{RAIL}-{k}");
    let stills = [
        ("rewind-1-action", format!("[{}, {}]", click(1.0, &format!("map-chip-{RAIL}")), click(1.6, &step)), "2.2", None),
        ("rewind-2-branching", "[]".to_string(), "1.0", Some(k)),
        ("brain-ready", "[]".to_string(), "1.0", None),
    ];
    for (name, script, at, branch) in stills {
        *BRANCH_AT.lock().unwrap() = branch;
        let f = out.join(format!("{name}.json"));
        std::fs::write(&f, script).unwrap();
        let args = vec!["--snap".to_string(), out.join(format!("{name}.png")).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), at.into(), "--script".into(), f.display().to_string()];
        let spec = rune::AppSpec { poll: Some(map_poll), on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), act_shown, crate::view) };
        rune::App::new(spec).args(args).try_run().unwrap();
    }
}
