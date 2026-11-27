//! Archive through coordd (the owner, N-1073; N-867 posts 21, 25, 27 and 29). `coord archive T` and the window's
//! `{"op": "archive"}` are the owner's alone, and taken live from this release; the release before read one only in
//! a record. His post on an archived thread writes its unarchive first, one line before the post. The policy is given
//! `archived`, and `works-under` when he archives.

use super::edit_tests::{core, log};
use super::{Said, Task, expired, run_task};
use crate::core::Who;
use docket_machine::{Input, Line};
use serde_json::json;

fn say(core: &mut super::Core, text: &str) -> (u64, String) {
    core.ticket_input(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": "w2", "typed": ["w2"], "names": ["w2"], "text": text, "addressed": {"head": "Named"}}) }).unwrap();
    let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    (t, core.tickets.m.id(t))
}

/// The record with his `archive` of T after its lines, as this release writes it, loaded again as coordd loads it at
/// a start.
fn with_his_archive_line(coord: &std::path::Path, t: u64) -> super::Core {
    let file = coord.join("tickets/log.jsonl");
    let mut text = std::fs::read_to_string(&file).unwrap();
    let last: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    text.push_str(&serde_json::to_string(&json!({"ev": "archive", "at": last["at"], "t": t, "on": true, "by": "owner"})).unwrap());
    text.push('\n');
    std::fs::write(&file, text).unwrap();
    load(coord)
}

fn load(coord: &std::path::Path) -> super::Core {
    let mut core = super::Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
    core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
    core.load_tickets();
    assert!(core.tickets.errors.is_empty(), "{:?}", core.tickets.errors);
    core
}

#[test]
fn his_archive_is_his_alone_reads_back_from_the_record_and_his_post_unarchives_it_first() {
    let (mut core, coord) = core("archive");
    let (t, a) = say(&mut core, "the parser");
    assert!(core.ticket_due("w2").iter().any(|d| d.t == t), "the thread is w2's to read");
    // an agent's, in words, whatever the policy says; nothing written
    let r = core.ticket_archive(&Who::Agent("w2".into()), &a, true);
    assert!(!r.ok && r.out.contains("Only the owner archives a ticket, or unarchives one: it says what is of importance to him."), "{}", r.out);
    assert!(!log(&coord).iter().any(|l| matches!(l, Line::Archive { .. })));
    // the window's request reads as the request
    let req: coord_types::Request = serde_json::from_str(&format!("{{\"op\":\"archive\",\"t\":\"{a}\",\"on\":true}}")).unwrap();
    assert!(matches!(req, coord_types::Request::Archive { t: ref u, on: true } if *u == a), "{req:?}");

    // a record holding his archive, loaded: the thread archived, in the snapshot as the window decodes it
    let mut core = with_his_archive_line(&coord, t);
    let read = |e: &coord_types::Event| serde_json::from_str::<coord_types::Event>(&serde_json::to_string(e).unwrap()).unwrap();
    let coord_types::Body::Snapshot(snap) = read(&core.snapshot()).body else { panic!("not a snapshot") };
    let v = snap.tickets.iter().find(|v| v.id == a).expect("the thread");
    assert!(v.archived && v.archived_under.is_none(), "{v:?}");
    assert!(core.ticket_due("w2").iter().all(|d| d.t != t), "nothing of it is handed: {:?}", core.ticket_due("w2"));
    // the policy is given `archived` for it
    let facts = core.ticket_facts(Some(t)).0;
    assert!(facts.iter().any(|(f, v)| f == "archived" && *v == vec![a.clone()]), "{facts:?}");
    // a step held on it does not expire (tickets.eflint's `expired`); one on a ticket not archived does
    let owed = vec![(t, 1, "w2".to_string(), a.clone(), 999)];
    let mut held = core.norm_facts(&[]);
    held.push(("archived".into(), vec![a.clone()]));
    assert_eq!(expired(&core.places(), held, &owed), Ok(Vec::new()));
    assert_eq!(expired(&core.places(), core.norm_facts(&[]), &owed).map(|e| e.len()), Ok(1));
    // whoever holds work under it is told once, quietly (notify.eflint's `inform`, event "archived")
    let mut facts = core.ticket_facts(Some(t)).0;
    facts.push(("works-under".into(), vec!["w2".into(), a.clone()]));
    let out = run_task(&crate::policy::Engine::start(core.places()), Task::Notify { t, event: "archived".into(), actor: "owner".into(), post: None, ticket: a.clone(), facts, places: core.places() });
    assert!(matches!(out.as_slice(), [Input::Notified { to, inform, .. }] if to.is_empty() && *inform == vec!["w2".to_string()]), "{out:?}");

    // his post there: its unarchive is written first, one line before the post, and his words go
    let r = core.ticket_post(&Who::Owner, &a, "and the lexer too".into(), None, None, Said::default(), None, false);
    assert!(r.ok, "{}", r.out);
    let lines = log(&coord);
    let at = lines.iter().rposition(|l| matches!(l, Line::Archive { on: false, .. })).expect("an unarchive");
    assert!(matches!(&lines[at], Line::Archive { t: u, by, .. } if *u == t && by == "owner"), "{:?}", lines[at]);
    assert!(matches!(lines.get(at + 1), Some(Line::Post { t: u, by, .. }) if *u == t && by == "owner"), "{:?}", lines.get(at + 1));
    assert!(!core.tickets.m.view(t).unwrap().archived);
    assert!(core.ticket_due("w2").iter().any(|d| d.t == t), "{:?}", core.ticket_due("w2"));
    // and the record loads again as it ran
    let again = load(&coord);
    assert!(!again.tickets.m.view(t).unwrap().archived);
    let _ = std::fs::remove_dir_all(&coord);
}

/// His archive, live: one line, and the `notify` question it asks is given who holds work under the thread
/// (`works-under`: w2, awaited by a question under it), so that the library's `inform` tells w2 once.
#[test]
fn his_archive_live_asks_who_is_told_with_who_works_under_it() {
    let (mut core, coord) = core("archive-live");
    let (t, a) = say(&mut core, "the parser");
    core.ticket_input(Input::Open { kind: "question".into(), under: Some(t), opener: "owner".into(), form: json!({"ask": "which median?"}), text: None, names: vec![], told: vec![] }).unwrap();
    let q = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    let step = match &core.tickets.m.tickets[&q].status {
        docket_machine::Status::Asking { owed, .. } => owed.step,
        s => panic!("{s:?}"),
    };
    core.ticket_input(Input::Decided { t: q, step, question: "fills".into(), answer: json!("w2"), provenance: json!({}) }).unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    core.tickets.tasks = Some(tx);
    let r = core.ticket_archive(&Who::Owner, &a, true);
    assert!(r.ok, "{}", r.out);
    assert!(log(&coord).iter().any(|l| matches!(l, Line::Archive { t: u, on: true, by, .. } if *u == t && by == "owner")));
    assert_eq!(core.tickets.m.view(q).unwrap().archived_under, Some(a.clone()));
    let task = std::iter::from_fn(|| rx.try_recv().ok()).find(|t| matches!(t, Task::Notify { .. })).expect("who is told is asked");
    let Task::Notify { t: u, event, actor, post, ticket, facts, places } = task else { unreachable!() };
    assert_eq!((u, event.as_str(), actor.as_str()), (t, "archived", "owner"));
    assert!(facts.iter().any(|(f, v)| f == "works-under" && *v == vec!["w2".to_string(), a.clone()]), "{facts:?}");
    let out = run_task(&crate::policy::Engine::start(places.clone()), Task::Notify { t: u, event, actor, post, ticket, facts, places });
    assert!(matches!(out.as_slice(), [Input::Notified { inform, .. }] if *inform == vec!["w2".to_string()]), "{out:?}");
    for i in out {
        core.ticket_input(i).unwrap();
    }
    assert!(core.ticket_due("w2").iter().any(|d| d.t == t && d.id.ends_with(".archived") && d.inform), "told once, quietly: {:?}", core.ticket_due("w2"));
    assert!(core.ticket_due("w2").iter().all(|d| d.t != q), "its step under it is held");
    let _ = std::fs::remove_dir_all(&coord);
}
