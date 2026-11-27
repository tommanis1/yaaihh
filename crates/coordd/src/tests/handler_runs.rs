//! `TicketView::ran` in coordd (0.3's W3 and W4, N-1446 p34, p37): a thread's view carries every run of a handler under
//! it, in the event the window reads and in the snapshot, a restart lists the same, and `Request::Ticket` on a run's
//! id opens that run.

use super::edit_tests::core;
use super::Said;
use crate::core::Who;
use docket_machine::{Input, OWNER_POST};
use serde_json::json;

#[test]
fn a_threads_view_lists_its_handlers_runs_and_a_runs_id_opens_it() {
    let (mut core, coord) = core("ran");
    core.ticket_input(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": "w2", "typed": ["w2"], "names": ["w2"], "text": "the parser", "addressed": {"head": "Named"}}) }).unwrap();
    let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    let id = core.tickets.m.id(t);
    let r = core.ticket_post(&Who::Owner, &id, "and the lexer".into(), None, None, Said::default(), None, false);
    assert!(r.ok, "{}", r.out);
    let his = core.tickets.m.tickets[&t].last();
    let r = core.ticket_post(&Who::Agent("w2".into()), &id, "seen".into(), None, None, Said::default(), None, false);
    assert!(r.ok, "{}", r.out);
    let v = core.ticket_shown(t).unwrap();
    let posts = |ev: &str| v.ran.iter().filter(|x| x.event == ev).map(|x| x.post).collect::<Vec<_>>();
    assert_eq!(posts(OWNER_POST), [Some(1), Some(his)], "{:?}", v.ran);
    // w2's post runs no handler: thread.kind's `when agent_post` went with the watcher's `asks` (proto-lead N-1479
    // p247); a thread opened before keeps it, and its runs are listed so (`ran` reads the runs, not the kind)
    assert_eq!(posts("agent_post"), Vec::<Option<u32>>::new(), "{:?}", v.ran);
    assert!(v.ran.iter().all(|x| x.from.is_some() && x.line.is_some()), "{:?}", v.ran);
    // the event the window reads, and the snapshot, carry it as it is
    let read = |e: &coord_types::Event| serde_json::from_str::<coord_types::Event>(&serde_json::to_string(e).unwrap()).unwrap();
    let one = coord_types::Event { seq: 9, body: coord_types::Body::Ticket(v.clone()) };
    assert_eq!(read(&one), one);
    let coord_types::Body::Snapshot(snap) = read(&core.snapshot()).body else { panic!("not a snapshot") };
    assert_eq!(snap.tickets.iter().find(|x| x.id == id).map(|x| &x.ran), Some(&v.ran));
    assert!(snap.tickets.iter().all(|x| !v.ran.iter().any(|r| r.id == x.id)), "a run is not a ticket of its own there");
    // a run's id opens that run
    let shown = core.ticket_show(&v.ran[0].id);
    assert!(shown.ok, "{}", shown.out);
    let opened: serde_json::Value = serde_json::from_str(&shown.out).unwrap();
    assert_eq!(opened["id"], json!(v.ran[0].id));
    // a restart lists the same
    let mut again = super::Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
    again.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
    again.load_tickets();
    assert!(again.tickets.errors.is_empty(), "{:?}", again.tickets.errors);
    assert_eq!(again.ticket_shown(t).unwrap().ran, v.ran);
    let _ = std::fs::remove_dir_all(&coord);
}
