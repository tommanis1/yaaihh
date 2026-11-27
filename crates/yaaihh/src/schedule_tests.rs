//! Schedule beside Send: a time or thread read as he types it; a thread's box held, not sent, and drawn as a queued
//! card whose Edit, Send now and Delete ask coordd; an agent's chat sent for its turn's end; nothing below the protocol.
use super::*;
use crate::Sel;
use crate::home::tests::{keyed, texts};
use coord_types::TicketView;
use serde_json::json;

#[test]
fn a_time_or_a_thread_is_read_as_he_types_it() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-11-27T10:30:00+01:00").unwrap();
    let at = |t: &str| When::At { at: t.into() };
    assert_eq!(when_of("+15m", now), Ok(at("2026-11-27T09:45:00Z")));
    assert_eq!(when_of("in 2h", now), Ok(at("2026-11-27T11:30:00Z")));
    assert_eq!(when_of("+1d", now), Ok(at("2026-11-28T09:30:00Z")));
    assert_eq!(when_of("11:00", now), Ok(at("2026-11-27T10:00:00Z")), "later today");
    assert_eq!(when_of("09:00", now), Ok(at("2026-11-28T08:00:00Z")), "gone today: tomorrow");
    assert_eq!(when_of("tomorrow 09:00", now), Ok(at("2026-11-28T08:00:00Z")));
    assert_eq!(when_of("Tomorrow", now), Ok(at("2026-11-28T08:00:00Z")));
    assert_eq!(when_of("when N-1980 closes", now), Ok(When::Closes { t: "N-1980".into() }));
    assert_eq!(when_of("n-12", now), Ok(When::Closes { t: "N-12".into() }));
    assert!(when_of("soon", now).is_err());
}

/// Project "research" linked to a pipe whose lines go to a file, a thread N-450 open; the file and the pipe's child.
#[cfg(unix)]
fn linked(protocol: u32) -> (std::sync::MutexGuard<'static, ()>, App, std::path::PathBuf, std::process::Child) {
    let (held, mut s) = crate::view_cost::test_app();
    let out = std::env::temp_dir().join(format!("yaaihh-schedule-{}-{protocol}", std::process::id()));
    let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
    let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
    (p.down, p.protocol) = (None, protocol);
    p.link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
    let v: TicketView = serde_json::from_value(json!({"id": "N-450", "n": 450, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
        "title": "Windows window", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": "2026-11-27T08:00:00Z", "given": [],
        "posts": [{"n": 1, "at": "2026-11-27T08:00:00Z", "by": "owner", "text": "the box", "names": ["paper-research"]}]}))
    .unwrap();
    p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v) });
    s.projects = vec![p];
    s.cur = 0;
    s.sel = Sel::Tickets;
    s.tickets.shown = Some(450);
    (held, s, out, child)
}

/// The requests sent so far, as JSON, once they are at least N or a second has passed.
fn lines(out: &std::path::Path, n: usize) -> Vec<serde_json::Value> {
    let until = Instant::now() + std::time::Duration::from_secs(1);
    loop {
        let v: Vec<serde_json::Value> = std::fs::read_to_string(out).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()).map(|v| v["req"].clone()).collect();
        if v.len() >= n || Instant::now() > until {
            return v;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

const BOX: &str = "thread:N-450";

fn settle(rt: &mut rune::runtime::Runtime<App>) {
    rt.invalidate();
    for _ in 0..3 {
        rt.frame(0.016);
    }
}

/// Schedule's press holds the box's words with coordd (no post made), the box emptied; the list's answer draws them
/// above the box as a queued card; Send now and Delete ask coordd, Edit puts the words back in the box and drops them.
#[cfg(unix)]
#[test]
fn a_thread_box_held_is_a_queued_card_its_edit_send_now_and_delete_ask_coordd() {
    let (_held, mut s, out, mut child) = linked(coord_types::SCHEDULE_PROTOCOL);
    s.composers.insert((0, BOX.into()), TextEdit::new("after the cut, merge it"));
    poll(&mut s);
    came(&mut s, "research".into(), None, None, Ok("[]".into()));
    let mut rt = crate::view_cost::window(s);
    settle(&mut rt);
    keyed(&mut rt, &format!("schedule-0-{BOX}"));
    let sent = lines(&out, 2);
    assert_eq!(sent.len(), 2, "the list, then the hold: no post {sent:?}");
    let add = &sent[1];
    assert_eq!((add["op"].as_str(), add["act"]["do"].as_str(), add["act"]["when"]["is"].as_str(), add["act"]["place"].as_str()), (Some("schedule"), Some("add"), Some("hold"), Some(BOX)));
    assert_eq!((add["act"]["req"]["op"].as_str(), add["act"]["req"]["t"].as_str(), add["act"]["req"]["text"].as_str()), (Some("post"), Some("N-450"), Some("after the cut, merge it")));
    assert_eq!(rt.state.composers[&(0, BOX.into())].text, "after the cut, merge it", "kept until coordd has it");
    let req = serde_json::from_value(add["act"]["req"].clone()).unwrap();
    let held = Held { id: 1, made: "2026-11-27T10:00:00Z".into(), when: When::Hold, place: BOX.into(), req, failed: None };
    let words = Some("after the cut, merge it".to_string());
    rt.update(|s, _| came(s, "research".into(), Some((0, BOX.into())), words, Ok(serde_json::to_string(&[held]).unwrap())));
    assert!(rt.state.composers.get(&(0, BOX.into())).is_none_or(|e| e.text.is_empty()), "held: the box emptied");
    settle(&mut rt);
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "Queued (Hold)") && t.iter().any(|x| x == "after the cut, merge it"), "{t:?}");
    keyed(&mut rt, "held-send-0-1");
    keyed(&mut rt, "held-drop-0-1");
    let sent = lines(&out, 4);
    assert_eq!(sent[2]["act"], json!({"do": "send", "id": 1}));
    assert_eq!(sent[3]["act"], json!({"do": "drop", "id": 1}));
    keyed(&mut rt, "held-edit-0-1");
    assert_eq!(rt.state.composers[&(0, BOX.into())].text, "after the cut, merge it", "its words back in the box");
    assert_eq!(lines(&out, 5)[4]["act"], json!({"do": "drop", "id": 1}));
    let _ = child.kill();
    let _ = std::fs::remove_file(&out);
}

/// A hold coordd refuses leaves his words in the box and says why; a Send now it refuses asks for the list again,
/// whose card then says it was not sent and why.
#[cfg(unix)]
#[test]
fn a_refused_hold_keeps_his_words_and_a_refused_send_now_shows_on_its_card() {
    let (_held, mut s, out, mut child) = linked(coord_types::SCHEDULE_PROTOCOL);
    poll(&mut s);
    came(&mut s, "research".into(), None, None, Ok("[]".into()));
    s.composers.insert((0, BOX.into()), TextEdit::new("keep these words"));
    let target = crate::message_box::Target::Thread { project: 0, t: "N-450".into(), post: None, way: crate::message_box::Way::Here };
    hold(&mut s, &target, When::Hold);
    came(&mut s, "research".into(), Some((0, BOX.into())), Some("keep these words".into()), Err("coord schedule: 256 are held already".into()));
    assert_eq!(s.composers[&(0, BOX.into())].text, "keep these words", "his words stay");
    assert_eq!(s.schedule.refused.get(&(0, BOX.into())).map(String::as_str), Some("coord schedule: 256 are held already"));
    // Send now refused: the list asked again, and its card says why
    came(&mut s, "research".into(), Some((0, BOX.into())), None, Err("There is no ticket N-999.".into()));
    assert!(poll(&mut s), "the list asked again");
    assert_eq!(lines(&out, 3)[2]["act"], json!({"do": "list"}));
    let req = Request::Post { kind: None, t: "N-999".into(), text: "nowhere".into(), to: None, answers: None, wait: false, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), comments: Vec::new() };
    let failed = Held { id: 4, made: "2026-11-27T10:00:00Z".into(), when: When::Hold, place: BOX.into(), req, failed: Some("There is no ticket N-999.".into()) };
    came(&mut s, "research".into(), None, None, Ok(serde_json::to_string(&[failed]).unwrap()));
    let mut rt = crate::view_cost::window(s);
    settle(&mut rt);
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "Not sent: There is no ticket N-999.") && !t.iter().any(|x| x == "Queued (Hold)"), "{t:?}");
    let _ = child.kill();
    let _ = std::fs::remove_file(&out);
}

/// A menu's "When its turn ends" sends the box at once with `wait`; "When N-450 closes" holds it on the thread; a
/// coordd below the protocol draws no Schedule.
#[cfg(unix)]
#[test]
fn the_turns_end_is_sent_waiting_and_a_close_holds_it_and_an_old_coordd_has_no_schedule() {
    let (held, mut s, out, mut child) = linked(coord_types::SCHEDULE_PROTOCOL);
    s.composers.insert((0, BOX.into()), TextEdit::new("when you are done"));
    let target = crate::message_box::Target::Thread { project: 0, t: "N-450".into(), post: None, way: crate::message_box::Way::Here };
    after_turn(&mut s, &target);
    let sent = lines(&out, 1);
    assert_eq!((sent[0]["op"].as_str(), sent[0]["wait"].as_bool(), sent[0]["text"].as_str()), (Some("post"), Some(true), Some("when you are done")));
    s.composers.insert((0, BOX.into()), TextEdit::new("once it closes"));
    hold(&mut s, &target, When::Closes { t: "N-450".into() });
    assert_eq!(lines(&out, 2)[1]["act"]["when"], json!({"is": "closes", "t": "N-450"}));
    let _ = child.kill();
    let _ = std::fs::remove_file(&out);
    drop(s);
    drop(held);
    let (_held, s, out, mut child) = linked(coord_types::SCHEDULE_PROTOCOL - 1);
    assert!(!offered(&s, &target));
    let mut rt = crate::view_cost::window(s);
    settle(&mut rt);
    assert!(rt.elements().iter().all(|e| e.key.as_deref() != Some(&format!("schedule-0-{BOX}"))), "no Schedule");
    let _ = child.kill();
    let _ = std::fs::remove_file(&out);
}

/// The stills of Schedule (N-1980): a thread's box with two held cards and Schedule's menu open, and an agent's chat
/// with Schedule beside Send and one held. `STILL_OUT=DIR … schedule::tests::schedule_stills --ignored --exact`.
#[test]
#[ignore]
fn schedule_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    fn held(id: u64, when: When, place: &str, text: &str) -> Held {
        let req = Request::Post { kind: None, t: "N-450".into(), text: text.into(), to: None, answers: None, wait: false, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), comments: Vec::new() };
        Held { id, made: "2026-11-27T09:00:00Z".into(), when, place: place.into(), req, failed: None }
    }
    fn base() -> App {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        (p.down, p.protocol) = (None, coord_types::PROTOCOL);
        p.live.agents = vec![serde_json::from_value(json!({"id": "AGT-1", "name": "paper-research", "parent": "owner", "kind": "claude", "model": "claude-opus-5-5", "state": "working", "since": "2026-11-27T08:00:00Z", "tools": [], "items": []})).unwrap()];
        let v: TicketView = serde_json::from_value(json!({"id": "N-450", "n": 450, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
            "title": "Windows window", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": "2026-11-27T08:00:00Z", "given": [],
            "posts": [{"n": 1, "at": "2026-11-27T08:00:00Z", "by": "owner", "text": "the box, one for every place", "names": ["paper-research"], "told": ["paper-research"]},
                      {"n": 2, "at": "2026-11-27T08:05:00Z", "by": "paper-research", "text": "Done: the box is in every place now.", "told": ["owner"]}]}))
        .unwrap();
        p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v) });
        s.projects = vec![p];
        s.cur = 0;
        let tomorrow = (Local::now() + Duration::days(1)).date_naive().and_hms_opt(9, 0, 0).and_then(|t| Local.from_local_datetime(&t).earliest()).unwrap();
        let at = tomorrow.with_timezone(&chrono::Utc).format("%Y-%m-%dT%H:%M:%SZ").to_string();
        s.schedule.held.insert("research".into(), vec![
            held(1, When::Hold, BOX, "Merge it after the cut, and post the new tip here."),
            held(2, When::At { at }, BOX, "Morning check: is the Windows build green?"),
            held(3, When::Hold, "paper-research", "When you're done: the stills of both features, please."),
        ]);
        s.schedule.asked.insert("research".into(), Instant::now());
        s
    }
    fn thread() -> App {
        let mut s = base();
        s.sel = Sel::Tickets;
        s.tickets.shown = Some(450);
        s.composers.insert((0, BOX.into()), TextEdit::new("and the release notes after that"));
        s.schedule.menu = Some(crate::message_box::Target::Thread { project: 0, t: "N-450".into(), post: None, way: crate::message_box::Way::Here });
        s
    }
    fn agent() -> App {
        let mut s = base();
        s.sel = Sel::Agent("paper-research".into());
        s.streams.which.insert((0, "paper-research".into()), crate::streams::Which::Work);
        s.composers.insert((0, "paper-research".into()), TextEdit::new("thanks, the box looks right"));
        s
    }
    let shots: [(&str, fn() -> App); 2] = [("schedule-thread.png", thread), ("schedule-agent.png", agent)];
    for (name, init) in shots {
        let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) }).args(args).try_run().unwrap();
    }
}
