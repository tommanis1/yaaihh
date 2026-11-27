//! A stop passed to the owner on an agent's question (the owner, N-1980.p2: clerk-paper-setup's Q-1 hit NoWorker and
//! the sidebar and Needs you showed four cards): one item, its question in his words, the stop said inline in plain
//! English, no hidden ticket's id and no exception's name.
use crate::{App, Sel};
use docket_machine::TicketView;
use serde_json::json;

const ASKED: &str = "May I read the shared drive's setup notes?";

/// Q-1 of clerk-paper-setup: its answer step owed to the lead, whom nobody fills, raised NoWorker; stopped, the owner
/// asked in S-1099511627777 whether it runs again.
pub(crate) fn stopped_question() -> App {
    let (_held, mut s) = crate::view_cost::test_app();
    let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "ai-detection".into(), id: "ai-detection".into(), boxed: false }, Vec::new());
    (p.down, p.protocol) = (None, coord_types::PROTOCOL);
    p.live.agents = vec![serde_json::from_value(json!({"id": "AGT-1", "name": "clerk-paper-setup", "parent": "owner", "kind": "claude", "model": "claude-opus-5-5", "state": "idle", "since": "2026-11-27T09:00:00Z", "tools": [], "items": []})).unwrap()];
    let hidden = docket_machine::HIDDEN + 1;
    let stop = json!({"id": format!("S-{hidden}"), "n": hidden, "kind": "stopped", "opener": "clerk-paper-setup", "ctx": {}, "form": {}, "title": "", "children": [], "program": "",
        "bound": 1, "spent": 0, "opened_at": "2026-11-27T09:01:00Z", "given": [], "posts": [], "parent": "Q-1",
        "status": {"is": "awaits", "agent": "owner", "call": "stopped", "step": 1, "since": "2026-11-27T09:01:00Z", "taken": false, "refusals": 0, "forming": false,
            "head": {"Variant": [["Again", []], ["Ended", [["why", "Text"]]]]}, "args": {}}});
    let q: TicketView = serde_json::from_value(json!({"id": "Q-1", "n": 1, "kind": "question", "opener": "clerk-paper-setup", "ctx": {}, "form": {"ask": ASKED},
        "title": ASKED, "children": [], "program": "", "bound": 4, "spent": 1, "opened_at": "2026-11-27T09:00:00Z", "given": [], "posts": [],
        "status": {"is": "stopped", "raised": {"exc": "NoWorker", "value": {"role": "lead", "call": "answer"}}, "to": "owner", "by": format!("S-{hidden}")},
        "waits_on": [{"ticket": format!("S-{hidden}"), "agent": "owner", "call": "stopped"}], "owed": true,
        "handlers": [stop]}))
    .unwrap();
    p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(q) });
    s.projects = vec![p];
    s.cur = 0;
    s.sel = Sel::Tickets;
    s
}

fn texts(rt: &rune::runtime::Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text.or(e.label)).collect()
}

fn settle(rt: &mut rune::runtime::Runtime<App>) {
    rt.invalidate();
    for _ in 0..3 {
        rt.frame(0.016);
    }
}

/// Needs you holds it once, as his question; its page says why it stopped in words, never the hidden ticket's id
/// nor the exception's name; the box's line says what his answer decides.
#[test]
fn a_stopped_question_is_one_item_said_in_plain_words() {
    let s = stopped_question();
    assert_eq!(crate::waits::on_owner(&s.projects[0]).len(), 1, "one thing waits on him");
    let mut rt = crate::view_cost::window(s);
    settle(&mut rt);
    let leaks = |t: &[String]| t.iter().filter(|x| x.contains("S-1099511627777") || x.contains("NoWorker")).cloned().collect::<Vec<_>>();
    let t = texts(&rt);
    assert_eq!(t.iter().filter(|x| x.contains(ASKED)).count(), 1, "his question once in the list: {t:?}");
    assert!(leaks(&t).is_empty(), "{:?}", leaks(&t));
    rt.update(|s, _| s.tickets.shown = Some(1));
    settle(&mut rt);
    let t = texts(&rt);
    assert!(leaks(&t).is_empty(), "{:?}", leaks(&t));
    assert!(t.iter().any(|x| x.contains("nobody could take it: no agent is the lead here")), "{t:?}");
}

#[test]
fn coordds_answer_to_a_step_is_said_without_its_ids() {
    use crate::threads::answered_words;
    assert_eq!(answered_words("Answered step 1 of S-1099511627777."), "Answered.");
    assert_eq!(answered_words("Answered step 2 of N-450; it waits for the owner's approval (slow mode)."), "Answered; it waits for the owner's approval (slow mode).");
    assert_eq!(answered_words("Not awaiting."), "Not awaiting.");
}

/// Stills of what he sees: the threads' list with Needs you, and the question's page. `STILL_OUT=DIR …
/// one_stop_tests::one_stop_stills --ignored --exact`.
#[test]
#[ignore]
fn one_stop_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    fn page() -> App {
        let mut s = stopped_question();
        s.tickets.shown = Some(1);
        s
    }
    let shots: [(&str, fn() -> App); 2] = [("stop-list.png", stopped_question), ("stop-page.png", page)];
    for (name, init) in shots {
        let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) }).args(args).try_run().unwrap();
    }
}
