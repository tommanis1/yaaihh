//! One message on several threads (the owner's N-354.p1, N-355.p1; paper-research N-695.p1, 2; docket-edge4 N-725):
//! a post goes on further threads too, the same words by the same writer, each a post of its own; each says the
//! others (`Post::on`, a `posted_on` line each), a restart reads them back, and the window reads them in an event.

use super::edit_tests::{core, log};
use super::Said;
use crate::core::Who;
use docket_machine::{Input, Line};
use serde_json::json;

fn say(core: &mut super::Core, text: &str) -> String {
    core.ticket_input(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": "w2", "typed": ["w2"], "names": ["w2"], "text": text, "addressed": {"head": "Named"}}) }).unwrap();
    let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    core.tickets.m.id(t)
}

/// w2 posts on thread ID.
fn w2_post(core: &mut super::Core, id: &str) {
    let r = core.ticket_post(&Who::Agent("w2".into()), id, "seen".into(), None, None, Said::default(), None, false);
    assert!(r.ok, "{}", r.out);
}

/// The owner's unnamed words TEXT to w2, and the job of the watcher's intake step they open.
fn intake(core: &mut super::Core, text: &str) -> crate::clerk::Job {
    let v = json!({"from": "owner", "to": "w2", "typed": ["w2"], "names": [], "text": text, "addressed": {"head": "Unnamed"}});
    core.ticket_input(Input::Event { name: "owner_message".into(), value: v }).unwrap();
    let d = core.ticket_due(docket_machine::WATCHER).into_iter().find(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "intake")).expect("the intake step");
    core.clerk_job(&d).expect("intake's job")
}

/// edit_tests' coord, its project with one kind more: `o`, whose one step the owner owes.
fn core_owing(tag: &str) -> (super::Core, std::path::PathBuf) {
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let coord = std::env::temp_dir().join(format!("docket-on-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&coord);
    let project = coord.join("files");
    std::fs::create_dir_all(project.join("kinds")).unwrap();
    std::fs::create_dir_all(project.join("calls")).unwrap();
    std::fs::write(project.join("calls/a.call"), "call ask(q: Text) -> Said(text: Text)\n  task t\n  template\n    Q: {{ q }}\n").unwrap();
    std::fs::write(project.join("kinds/o.kind"), "kind o\n  letter O\n  form q: Text\n  result Text\n  pattern\n    a <- step owner ask(q);\n    return q\n").unwrap();
    std::fs::write(project.join("project.toml"), "[project]\nname = \"e\"\n").unwrap();
    std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
    std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
    let mut core = super::Core::load(&coord.display().to_string(), &base);
    core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
    core.load_tickets();
    assert!(core.tickets.errors.is_empty(), "{:?}", core.tickets.errors);
    (core, coord)
}

/// w2 asks the owner something on thread ID, still open: the thread waits on his answer.
fn w2_asks(core: &mut super::Core, id: &str) {
    w2_post(core, id);
    let t = super::ticket_of(&core.tickets.m, id).unwrap();
    core.tickets.m.tickets.edit().get_mut(&t).unwrap().posts.last_mut().unwrap().asks_open = true;
    assert!(!core.tickets.m.view(t).unwrap().waits_on.iter().any(|w| w.agent == "owner"), "{id}: through the marked ask alone");
    assert!(core.awaits_owner(id));
}

fn last_post(core: &super::Core, id: &str) -> docket_machine::Post {
    let t = super::ticket_of(&core.tickets.m, id).unwrap();
    core.tickets.m.tickets[&t].posts.last().unwrap().clone()
}

#[test]
fn one_message_goes_on_each_thread_and_each_post_says_the_others() {
    let (mut core, coord) = core("posted-on");
    let (a, b, c) = (say(&mut core, "the parser"), say(&mut core, "the lexer"), say(&mut core, "the printer"));
    let before = core.ticket_last_post(&a);
    let r = core.ticket_post(&Who::Owner, &a, "both need the new tokens".into(), None, None, Said::default(), None, false);
    assert!(r.ok, "{}", r.out);
    let on = vec![(b.clone(), Said::default()), (c.clone(), Said::default()), ("N-999".to_string(), Said::default())];
    let r = core.ticket_post_on(&Who::Owner, &a, before, "both need the new tokens", None, None, None, on, vec![("N-998".into(), "You may not post on N-998.".into())], r);
    assert!(r.ok, "{}", r.out);
    assert!(r.out.contains("Not posted on N-999") && r.out.contains("Not posted on N-998: You may not post on N-998."), "{}", r.out);
    let (pa, pb, pc) = (last_post(&core, &a), last_post(&core, &b), last_post(&core, &c));
    let id = |t: &str, p: &docket_machine::Post| format!("{t}.p{}", p.n);
    for p in [&pa, &pb, &pc] {
        assert_eq!((p.by.as_str(), &p.text), ("owner", &docket_machine::Text::New("both need the new tokens".into())));
    }
    assert_eq!(pa.on, vec![id(&b, &pb), id(&c, &pc)]);
    assert_eq!(pb.on, vec![id(&a, &pa), id(&c, &pc)]);
    assert_eq!(pc.on, vec![id(&a, &pa), id(&b, &pb)]);
    // one `posted_on` line each; a restart reads them back
    let lines: Vec<(u32, Vec<String>)> = log(&coord).into_iter().filter_map(|l| if let Line::PostedOn { n, on, .. } = l { Some((n, on)) } else { None }).collect();
    assert_eq!(lines.len(), 3);
    let mut again = super::Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
    again.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
    again.load_tickets();
    assert!(again.tickets.errors.is_empty(), "{:?}", again.tickets.errors);
    assert_eq!(last_post(&again, &b).on, pb.on);
    // the window reads the field in a ticket's event and a snapshot (N-541)
    let read = |e: &coord_types::Event| serde_json::from_str::<coord_types::Event>(&serde_json::to_string(e).unwrap()).unwrap();
    let t = super::ticket_of(&core.tickets.m, &b).unwrap();
    let one = coord_types::Event { seq: 9, body: coord_types::Body::Ticket(core.ticket_shown(t).unwrap()) };
    assert_eq!(read(&one), one);
    let coord_types::Body::Snapshot(snap) = read(&core.snapshot()).body else { panic!("not a snapshot") };
    let vb = snap.tickets.iter().find(|v| v.id == b).expect("B");
    assert_eq!(vb.posts.iter().find(|p| p.n == pb.n).map(|p| p.on.clone()), Some(pb.on.clone()));
    // a post with nothing further says nothing; a `posted_on` naming a post of another writer, or its own thread, is refused
    let before = core.ticket_last_post(&a);
    let plain = core.ticket_post(&Who::Owner, &a, "only here".into(), None, None, Said::default(), None, false);
    let plain = core.ticket_post_on(&Who::Owner, &a, before, "only here", None, None, None, vec![], vec![], plain);
    assert!(plain.ok && last_post(&core, &a).on.is_empty(), "{}", plain.out);
    let ta = super::ticket_of(&core.tickets.m, &a).unwrap();
    let n = last_post(&core, &a).n;
    assert!(core.ticket_input(Input::PostedOn { t: ta, n, on: vec![format!("{a}.p1")] }).is_err(), "its own thread");
    assert!(core.ticket_input(Input::PostedOn { t: ta, n, on: vec![format!("{b}.p99")] }).is_err(), "no such post");
    w2_post(&mut core, &b);
    assert!(core.ticket_input(Input::PostedOn { t: ta, n, on: vec![id(&b, &last_post(&core, &b))] }).is_err(), "another writer's post");
    let _ = std::fs::remove_dir_all(&coord);
}

/// A thread among them that is not open (consult-3, N-581.p35): the owner's words open a new thread following it (his
/// `say`), which is named in the reply and tied to nothing; the posts made on the open ones say each other, and an
/// older post of his on the closed thread is never taken for the new one.
#[test]
fn a_closed_thread_among_them_is_named_and_the_open_ones_say_each_other() {
    let (mut core, coord) = core("posted-on-closed");
    let (d, e, f) = (say(&mut core, "the parser"), say(&mut core, "the lexer"), say(&mut core, "the printer"));
    let old = core.ticket_post(&Who::Owner, &d, "an older word".into(), Some("w2".into()), None, Said::default(), None, false);
    assert!(old.ok, "{}", old.out);
    let older = last_post(&core, &d);
    assert!(core.ticket_end(&Who::Owner, &d, "done", "merged", Said::default()).ok);
    // the first thread closed
    let before = core.ticket_last_post(&d);
    let r = core.ticket_post(&Who::Owner, &d, "all three again".into(), Some("w2".into()), None, Said::default(), None, false);
    assert!(r.ok, "{}", r.out);
    let on = vec![(e.clone(), Said::default()), (f.clone(), Said::default())];
    let r = core.ticket_post_on(&Who::Owner, &d, before, "all three again", Some("w2".into()), None, None, on, vec![], r);
    assert!(r.ok && r.out.contains(&format!("({d} is not open")), "{}", r.out);
    let (pe, pf) = (last_post(&core, &e), last_post(&core, &f));
    assert_eq!(pe.text, docket_machine::Text::New("all three again".into()));
    assert_eq!(pe.on, vec![format!("{f}.p{}", pf.n)]);
    assert_eq!(pf.on, vec![format!("{e}.p{}", pe.n)]);
    assert!(last_post(&core, &d).on.is_empty() && last_post(&core, &d).n == older.n, "the older post on the closed thread is not tied");
    // a further thread closed: named, and the one post made says nothing
    let before = core.ticket_last_post(&e);
    let r = core.ticket_post(&Who::Owner, &e, "just these".into(), Some("w2".into()), None, Said::default(), None, false);
    let r = core.ticket_post_on(&Who::Owner, &e, before, "just these", Some("w2".into()), None, None, vec![(d.clone(), Said::default())], vec![], r);
    assert!(r.ok && r.out.contains(&format!("({d} is not open")), "{}", r.out);
    assert!(last_post(&core, &e).on.is_empty());
    let _ = std::fs::remove_dir_all(&coord);
}

/// His message fitting two open threads (the intake's Unsure) stands on both, and nothing is asked of him (the owner,
/// N-354.p1/N-355.p1: "psot on both for all i care"; paper-research N-586.p10): his post on each, each saying the
/// other; the thread his words opened holds nothing (Moved to the first).
#[test]
fn a_message_fitting_two_threads_stands_on_both_and_asks_nothing() {
    let (mut core, coord) = core("posted-on-fits-two");
    let (a, b, c) = (say(&mut core, "the parser"), say(&mut core, "the lexer"), say(&mut core, "the printer"));
    // one of them waits on his answer, the other does not: still both
    w2_asks(&mut core, &a);
    let c_before = core.ticket_last_post(&c);
    let job = intake(&mut core, "both need the new tokens");
    assert!(job.open.contains(&a) && job.open.contains(&b), "{}", job.open);
    // the intake names three: his words go on the first two, and the third is named to him on the first
    core.clerk_answered(&job, json!({"head": "Unsure", "tickets": [a, b, c]}), None, None);
    let (pa, pb) = (last_post(&core, &a), last_post(&core, &b));
    for p in [&pa, &pb] {
        assert_eq!((p.by.as_str(), &p.text), ("owner", &docket_machine::Text::New("both need the new tokens".into())));
    }
    assert_eq!(pa.on, vec![format!("{b}.p{}", pb.n)]);
    assert_eq!(pb.on, vec![format!("{a}.p{}", pa.n)]);
    assert_eq!(core.ticket_last_post(&c), c_before, "no post on the third");
    let fits: Vec<_> = core.notices().into_iter().filter(|n| n.ticket.as_deref() == Some(a.as_str())).collect();
    assert!(fits.len() == 1 && fits[0].text.contains(&format!("({c})")) && fits[0].text.contains("went on two"), "{fits:?}");
    assert!(core.notices().iter().all(|n| n.ticket.as_deref() != Some(b.as_str())), "nothing said on the second");
    // nothing asked of him: no step of his owed (w2's post on the first is a reply to him), and the thread his words
    // opened holds no post of them
    assert!(!core.ticket_due("owner").iter().any(|d| matches!(&d.what, docket_machine::What::Step { .. })));
    let opened = super::ticket_of(&core.tickets.m, &job.ticket).unwrap();
    assert!(core.tickets.m.tickets[&opened].posts.iter().all(|p| p.text != docket_machine::Text::New("both need the new tokens".into())), "moved, not kept");
    let _ = std::fs::remove_dir_all(&coord);
}

#[test]
fn words_cancelled_while_the_intake_ran_go_on_no_thread() {
    let (mut core, coord) = core("posted-on-cancelled");
    let (a, b) = (say(&mut core, "the parser"), say(&mut core, "the lexer"));
    let (a_before, b_before) = (core.ticket_last_post(&a), core.ticket_last_post(&b));
    let job = intake(&mut core, "both need the new tokens");
    // he cancels the thread his words opened while the call runs off the lock (consult-3, N-581.p48)
    let r = core.ticket_cancel(&Who::Owner, &job.ticket, "not now".into(), Said::default());
    assert!(r.ok, "{}", r.out);
    core.clerk_answered(&job, json!({"head": "Unsure", "tickets": [a, b]}), None, None);
    assert_eq!((core.ticket_last_post(&a), core.ticket_last_post(&b)), (a_before, b_before), "nothing posted");
    let _ = std::fs::remove_dir_all(&coord);
}

#[test]
fn a_short_answer_fitting_two_threads_that_wait_on_him_stands_on_neither() {
    let (mut core, coord) = core_owing("posted-on-two-waiting");
    // one waits through w2's marked ask on its post, the other through a step he owes (kind `o`, opened by w2)
    let a = say(&mut core, "the old tokens");
    w2_asks(&mut core, &a);
    let r = core.ticket_open(&Who::Agent("w2".into()), "o".into(), None, json!({"q": "merge now?"}), None, Said::default());
    assert!(r.ok, "{}", r.out);
    let b = core.tickets.m.id(*core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0);
    let bt = super::ticket_of(&core.tickets.m, &b).unwrap();
    assert!(core.tickets.m.view(bt).unwrap().waits_on.iter().any(|w| w.agent == "owner"), "{b} waits through waits_on: {:?}", core.tickets.m.tickets[&bt].status);
    assert!(!core.tickets.m.tickets[&bt].posts.iter().any(|p| core.tickets.m.asks_open(&core.tickets.m.tickets[&bt], p.n)), "{b}: no marked ask");
    let (a_before, b_before) = (core.ticket_last_post(&a), core.ticket_last_post(&b));
    let job = intake(&mut core, "yes");
    core.clerk_answered(&job, json!({"head": "Unsure", "tickets": [a, b]}), None, None);
    assert_eq!((core.ticket_last_post(&a), core.ticket_last_post(&b)), (a_before, b_before), "a yes answers neither question");
    // a thread of his own, his words its post, with both tickets beside it in the answer
    let opened = super::ticket_of(&core.tickets.m, &job.ticket).unwrap();
    assert!(core.tickets.m.tickets[&opened].posts.iter().any(|p| p.by == "owner" && p.text == docket_machine::Text::New("yes".into())), "his own thread");
    let unsure = log(&coord).into_iter().map(|l| serde_json::to_string(&l).unwrap()).any(|l| l.contains("Unsure") && l.contains(&a) && l.contains(&b));
    assert!(unsure, "the Unsure answer, both tickets in it, is recorded");
    let _ = std::fs::remove_dir_all(&coord);
}

#[test]
fn a_short_answer_fitting_two_threads_where_an_agent_merely_spoke_last_stands_on_both() {
    let (mut core, coord) = core("posted-on-spoke-last");
    let (a, b) = (say(&mut core, "the old tokens"), say(&mut core, "the merge"));
    // w2 spoke last on both and asked nothing: the list says "waits on the owner's answer", the guard does not
    w2_post(&mut core, &a);
    w2_post(&mut core, &b);
    for id in [&a, &b] {
        let t = super::ticket_of(&core.tickets.m, id).unwrap();
        let v = core.tickets.m.view(t).unwrap();
        assert!(v.owner.as_ref().is_some_and(|o| !o.unanswered.is_empty()) && !v.owed, "{id}: an agent spoke last, nothing owed");
        assert!(!core.awaits_owner(id), "{id}");
    }
    let job = intake(&mut core, "yes, both look good");
    core.clerk_answered(&job, json!({"head": "Unsure", "tickets": [a, b]}), None, None);
    let (pa, pb) = (last_post(&core, &a), last_post(&core, &b));
    for p in [&pa, &pb] {
        assert_eq!((p.by.as_str(), &p.text), ("owner", &docket_machine::Text::New("yes, both look good".into())));
    }
    assert_eq!(pa.on, vec![format!("{b}.p{}", pb.n)]);
    let _ = std::fs::remove_dir_all(&coord);
}
