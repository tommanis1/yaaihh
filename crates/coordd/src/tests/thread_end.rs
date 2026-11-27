//! A thread ends through coordd (paper-research N-447, piece 1): `coord end` asks the policy's `edit` as a cancel
//! does (its opener, an agent above it, a lead; the owner is not bound), and `coord outline` then leaves the thread
//! out, but keeps the line of one that ended under a thread still open, marked so.

use super::edit_tests::{core, log};
use super::{Said, may, outline_rows};
use crate::core::Who;
use docket_machine::{Input, Line};
use serde_json::json;

fn say(core: &mut super::Core, text: &str) -> String {
    core.ticket_input(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": "w2", "typed": ["w2"], "names": ["w2"], "text": text, "addressed": {"head": "Named"}}) }).unwrap();
    let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    core.tickets.m.id(t)
}

#[test]
fn an_ended_thread_leaves_the_outline_and_its_parents_line_for_it_stays() {
    let (mut core, coord) = core("end");
    let (a, b, c) = (say(&mut core, "the parser"), say(&mut core, "the lexer"), say(&mut core, "the printer"));
    assert!(core.ticket_tie(&Who::Owner, &b, &a).ok);
    let rows = |core: &super::Core| outline_rows(&core.tickets.m).into_iter().map(|o| (o.id, o.state, o.why)).collect::<Vec<_>>();
    assert_eq!(rows(&core).len(), 3);
    // w2 opened none of them and leads nothing: the policy's `edit` refuses it, as it would a cancel
    let ask = core.ticket_may_edit(&Who::Agent("w2".into()), &b).expect("an agent is asked");
    assert_eq!(ask.act, "edit");
    assert!(may(ask).is_err(), "w2 may not end the owner's thread");
    assert!(core.ticket_may_edit(&Who::Owner, &b).is_none(), "the owner is not bound");
    let r = core.ticket_end(&Who::Owner, &b, "done", "the lexer is merged", Said::default());
    assert!(r.ok, "{}", r.out);
    let r = core.ticket_end(&Who::Owner, &c, "dropped", "not needed", Said::default());
    assert!(r.ok, "{}", r.out);
    // B ended under A, open: its line stays, marked; C ended at the top: gone
    let now = rows(&core);
    assert_eq!(now.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), vec![a.as_str(), b.as_str()]);
    assert_eq!((now[1].1.as_str(), now[1].2.as_deref()), ("done", Some("the lexer is merged")));
    // the window reads a whole event in one go (N-541): a snapshot holding the ended thread's view and A's outline,
    // whose line for B carries how and why, decodes back through coord_types::Event as coordd writes it
    let read = |e: &coord_types::Event| serde_json::from_str::<coord_types::Event>(&serde_json::to_string(e).unwrap()).unwrap();
    let coord_types::Body::Snapshot(snap) = read(&core.snapshot()).body else { panic!("not a snapshot") };
    let vb = snap.tickets.iter().find(|v| v.id == b).expect("B, closed today, is in the snapshot");
    assert_eq!(vb.ended.as_ref().map(|e| (e.how.as_str(), e.why.as_str(), e.by.as_str())), Some(("done", "the lexer is merged", "owner")));
    let va = snap.tickets.iter().find(|v| v.id == a).expect("A");
    let kid = va.outline.as_ref().and_then(|o| o.children.iter().find(|k| k.id == b)).expect("A's line for B");
    assert_eq!((kid.state.as_str(), kid.why.as_deref(), kid.open), ("done", Some("the lexer is merged"), false));
    let t = core.tickets.m.tickets.range(..docket_machine::HIDDEN).find(|(_, x)| core.tickets.m.id(x.t) == b).map(|(t, _)| *t).unwrap();
    let one = coord_types::Event { seq: 9, body: coord_types::Body::Ticket(core.ticket_shown(t).unwrap()) };
    assert_eq!(read(&one), one, "a ticket's event with `ended` reads back the same");
    let r = core.ticket_end(&Who::Owner, &a, "kept", "over", Said::default());
    assert!(r.ok, "{}", r.out);
    assert!(rows(&core).is_empty(), "A ended: neither is a row; B is behind A's click");
    // the record holds one `ended` line each, and a restart reads them back
    let ended: Vec<String> = log(&coord).into_iter().filter_map(|l| if let Line::Ended { how, .. } = l { Some(how) } else { None }).collect();
    assert_eq!(ended, vec!["done", "dropped", "kept"]);
    let mut again = super::Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
    again.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
    again.load_tickets();
    assert!(again.tickets.errors.is_empty(), "{:?}", again.tickets.errors);
    assert!(rows(&again).is_empty());
    let _ = std::fs::remove_dir_all(&coord);
}

/// The post-level fields of the PROTOCOL 6 line read back in a whole event, as the window reads it (docket-core3,
/// N-447 p116; N-541): a ticket's view from the machine whose posts carry `would_close` and an `asks_closed` of each
/// new way (`Withdrawn`, `Subthread`), written and read back through `coord_types::Event`, equal whole.
#[test]
fn a_posts_would_close_and_its_new_closing_ways_read_back_in_an_event() {
    use docket_machine::line::{AsksClosed, Closes, WouldClose};
    let (mut core, coord) = core("end-decode");
    let a = say(&mut core, "the parser");
    let t = core.tickets.m.tickets.range(..docket_machine::HIDDEN).find(|(_, x)| core.tickets.m.id(x.t) == a).map(|(t, _)| *t).unwrap();
    let mut v = core.ticket_shown(t).unwrap();
    let mut p = v.posts[0].clone();
    p.asks_open = true;
    p.would_close = Some(WouldClose { foot: Closes::Routed, reply: Closes::Yes, subthread: Closes::No });
    let mut q = p.clone();
    q.n = p.n + 1;
    q.asks_open = false;
    q.would_close = None;
    q.asks_closed = Some(AsksClosed::Withdrawn { by: "w2".into(), why: "overtaken".into() });
    let mut r = q.clone();
    r.n = q.n + 1;
    r.asks_closed = Some(AsksClosed::Subthread { ticket: "N-9".into() });
    v.posts = vec![p, q, r];
    let one = coord_types::Event { seq: 3, body: coord_types::Body::Ticket(v) };
    let line = serde_json::to_string(&one).unwrap();
    assert!(line.contains("\"would_close\"") && line.contains("\"Withdrawn\"") && line.contains("\"Subthread\""), "{line}");
    let back: coord_types::Event = serde_json::from_str(&line).unwrap();
    assert_eq!(back, one);
    // and inside a snapshot, as the window's first event
    let coord_types::Body::Ticket(v) = back.body else { panic!() };
    let snap = serde_json::json!({"seq": 4, "snapshot": {"agents": [], "mail": [], "views": [], "refusals": [], "tickets": [v.clone()], "notices": []}});
    let coord_types::Body::Snapshot(s) = serde_json::from_str::<coord_types::Event>(&snap.to_string()).unwrap().body else { panic!() };
    assert_eq!(s.tickets[0], v);
    let _ = std::fs::remove_dir_all(&coord);
}

/// `Request::Insight` open → done (S6, proto-lead N-1446 p388): the owner's, every line of the outline with when it
/// ended, those `coord outline` leaves out too (ended at the top); an agent is refused.
#[test]
fn an_insight_holds_every_thread_open_or_ended_and_is_the_owners() {
    let (mut core, coord) = core("insight");
    let (a, b) = (say(&mut core, "the parser"), say(&mut core, "the lexer"));
    let r = core.ticket_end(&Who::Owner, &b, "done", "the lexer is merged", Said::default());
    assert!(r.ok, "{}", r.out);
    assert!(outline_rows(&core.tickets.m).iter().all(|o| o.id != b), "the outline leaves it out");
    assert!(!core.insight(&Who::Agent("w2".into()), coord_types::Insight::OpenToDone).ok, "an agent is refused");
    let r = core.insight(&Who::Owner, coord_types::Insight::OpenToDone);
    assert!(r.ok, "{}", r.out);
    let rows: Vec<docket_machine::insight::Thread> = r.out.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(rows.iter().map(|x| (x.id.as_str(), x.ended.is_some())).collect::<Vec<_>>(), vec![(a.as_str(), false), (b.as_str(), true)]);
    assert_eq!(rows[1].state, "done");
    assert!(rows.iter().all(|x| x.by.is_none()), "his message's threads are his");
    let _ = std::fs::remove_dir_all(&coord);
}
