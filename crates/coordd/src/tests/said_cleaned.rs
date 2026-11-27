//! His cleaned words sent as a say (docket-edge2 N-564, N-487): the `cleaned` line goes on the post of his that the
//! say's own thread makes, at once when named, when the intake answers however late, or where the intake moved them;
//! never by the clock or by matching text (docket-core3 N-487 p73). A words ticket not his ended in Send is refused.

use super::{Core, Task};
use crate::core::Who;
use docket_machine::{Input, Text};
use serde_json::json;

const CALL: &str = "call look(text: Text) -> Go | AsTyped\n  task look\n  default Go\n";
const KIND: &str = "kind words\n  form text: Text\n  result Send(text: Text) | Back(why: Text)\n  pattern\n    o <- step owner look(text);\n    match o {\n      Go => return Send(\"the words\")\n      AsTyped => return Send(text)\n    }\n";

fn core(tag: &str) -> (Core, std::path::PathBuf) {
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let coord = std::env::temp_dir().join(format!("docket-said-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&coord);
    let project = coord.join("files");
    std::fs::create_dir_all(project.join("kinds")).unwrap();
    std::fs::create_dir_all(project.join("calls")).unwrap();
    std::fs::write(project.join("calls/words.call"), CALL).unwrap();
    std::fs::write(project.join("kinds/words.kind"), KIND).unwrap();
    std::fs::write(project.join("project.toml"), "[project]\nname = \"w\"\n").unwrap();
    std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
    std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
    let mut core = Core::load(&coord.display().to_string(), &base);
    core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
    core.load_tickets();
    assert!(core.tickets.errors.is_empty(), "{:?}", core.tickets.errors);
    (core, coord)
}

/// A words ticket of his, ended in Send.
fn words(core: &mut Core) -> u64 {
    let r = core.ticket_input(Input::Open { kind: "words".into(), under: None, opener: "owner".into(), form: json!({"text": "teh words"}), text: None, names: vec![], told: vec![] }).unwrap();
    let w = docket_machine::parse_ticket(r.trim_start_matches("Opened ").trim_end_matches('.')).unwrap();
    core.ticket_input(Input::Lapsed { t: w, step: 1, head: json!({"head": "Go"}), after: 30, because: "time".into() }).unwrap();
    assert!(core.tickets.m.cleaned_words(w).is_ok());
    w
}

#[test]
fn his_say_of_cleaned_words_names_the_words_ticket_on_the_post_that_carries_them() {
    let (mut core, coord) = core("said");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    core.tickets.tasks = Some(tx);
    let pump = |core: &mut Core, rx: &mut tokio::sync::mpsc::UnboundedReceiver<Task>| {
        while let Ok(t) = rx.try_recv() {
            if let Task::Event { name, value } = t {
                core.ticket_input(Input::Event { name, value }).unwrap();
            }
        }
    };
    let w = words(&mut core);
    let cleaned = |core: &Core| -> Vec<(u64, u32)> { core.tickets.m.tickets.values().flat_map(|x| x.posts.iter().filter(|p| p.cleaned_from.is_some()).map(move |p| (x.t, p.n))).collect() };
    // refused before anything is sent: a words ticket not ended in Send
    let open = core.ticket_input(Input::Open { kind: "words".into(), under: None, opener: "owner".into(), form: json!({"text": "x"}), text: None, names: vec![], told: vec![] }).unwrap();
    let open = docket_machine::parse_ticket(open.trim_start_matches("Opened ").trim_end_matches('.')).unwrap();
    assert!(core.ticket_cleaned_words(&Who::Owner, &core.tickets.m.id(open)).is_err());
    assert!(core.ticket_cleaned_words(&Who::Agent("w2".into()), &core.tickets.m.id(w)).is_err(), "only his");
    // the event names the words ticket, so the record says it
    // named: the thread posts his words when the event opens it
    let text = "the words, cleaned";
    let r = core.ticket_say_cleaned(&Who::Owner, "w2".into(), vec!["w2".into()], vec![], text.into(), None, None, None, true, Some(w));
    assert!(r.ok, "{}", r.out);
    pump(&mut core, &mut rx);
    let got = cleaned(&core);
    assert_eq!(got.len(), 1, "{got:?}");
    let (t, n) = got[0];
    let p = core.tickets.m.tickets[&t].posts.iter().find(|p| p.n == n).unwrap();
    assert_eq!((p.by.as_str(), p.cleaned_from.clone()), ("owner", Some(core.tickets.m.id(w))));
    assert!(core.cleaned_pending.is_empty());
    // through the intake: nothing until the watcher answers, however long that takes; then on the thread's post
    let r = core.ticket_say_cleaned(&Who::Owner, "w2".into(), vec!["w2".into()], vec![], text.into(), None, None, None, false, Some(w));
    assert!(r.ok, "{}", r.out);
    pump(&mut core, &mut rx);
    assert_eq!(core.cleaned_pending.len(), 1, "waits on the intake");
    let u = core.cleaned_pending[0].0;
    core.ticket_input(Input::Answer { t: u, by: docket_machine::WATCHER.into(), head: json!({"head": "Thread"}), text: Text::New(String::new()) }).unwrap();
    assert!(core.cleaned_pending.is_empty());
    assert_eq!(cleaned(&core).len(), 2);
    assert!(cleaned(&core).iter().any(|(x, _)| *x == u));
    // moved by the intake to the first thread: on the post of his the move made there
    let r = core.ticket_say_cleaned(&Who::Owner, "w2".into(), vec!["w2".into()], vec![], "and one more".into(), None, None, None, false, Some(w));
    assert!(r.ok, "{}", r.out);
    pump(&mut core, &mut rx);
    let v = core.cleaned_pending[0].0;
    let out = core.ticket_input(Input::Answer { t: v, by: docket_machine::WATCHER.into(), head: json!({"head": "Place", "ticket": core.tickets.m.id(t)}), text: Text::New(String::new()) });
    assert!(out.is_ok(), "{out:?}");
    assert!(core.cleaned_pending.is_empty());
    let last = core.tickets.m.tickets[&t].posts.iter().rev().find(|p| p.by == "owner").unwrap();
    assert_eq!((&last.text, last.cleaned_from.clone()), (&Text::New("and one more".into()), Some(core.tickets.m.id(w))));
    // a restart reads the lines back
    let mut again = Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
    again.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
    again.load_tickets();
    assert!(again.tickets.errors.is_empty(), "{:?}", again.tickets.errors);
    assert_eq!(cleaned(&again), cleaned(&core));
    let _ = std::fs::remove_dir_all(&coord);
}

/// His message split by point (N-696, paper-research N-590 p12): each point he sends is his own post, and the `split`
/// line names the message on it, whether said to a new thread (on the post its thread makes) or posted on a ticket;
/// a point that is no point of a split, or anyone's but his, is refused before anything is sent; a restart reads it
/// back.
#[test]
fn each_point_of_his_split_message_names_the_message_on_his_own_post_and_replays() {
    let (mut core, coord) = core("split");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    core.tickets.tasks = Some(tx);
    let pump = |core: &mut Core, rx: &mut tokio::sync::mpsc::UnboundedReceiver<Task>| {
        while let Ok(t) = rx.try_recv() {
            if let Task::Event { name, value } = t {
                core.ticket_input(Input::Event { name, value }).unwrap();
            }
        }
    };
    let from = |point| docket_machine::SplitFrom { message: "m-1763500000000".into(), point, of: 2 };
    let split = |core: &Core| -> Vec<(u64, u32, u32)> { core.tickets.m.tickets.values().flat_map(|x| x.posts.iter().filter_map(move |p| Some((x.t, p.n, p.split_from.as_ref()?.point)))).collect() };
    assert!(super::split_refused(&Who::Agent("w2".into()), &from(1)).is_some(), "only his");
    assert!(super::split_refused(&Who::Owner, &docket_machine::SplitFrom { of: 1, ..from(1) }).is_some(), "one point is no split");
    assert!(super::split_refused(&Who::Owner, &from(3)).is_some(), "past its points");
    assert!(super::split_refused(&Who::Owner, &from(2)).is_none());
    // point 1 said to a new thread: on the post of his it makes
    let r = core.ticket_say_split(&Who::Owner, "w2".into(), vec!["w2".into()], vec![], "agda-dev is still not in the window.".into(), None, None, true, None, Some(from(1)));
    assert!(r.ok, "{}", r.out);
    pump(&mut core, &mut rx);
    assert!(core.split_pending.is_empty());
    let got = split(&core);
    assert_eq!(got.len(), 1, "{got:?}");
    let (t, n, point) = got[0];
    assert_eq!(point, 1);
    assert_eq!(core.tickets.m.tickets[&t].posts.iter().find(|p| p.n == n).unwrap().by, "owner");
    // point 2 posted on that ticket: on the post just made
    let before = core.ticket_last_post(&core.tickets.m.id(t));
    core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("runner rows: yes, do them.".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let r = core.ticket_split_post(&Who::Owner, &core.tickets.m.id(t), before, from(2), crate::core::ok(String::new()));
    assert!(r.ok && r.out.is_empty(), "{}", r.out);
    assert_eq!(split(&core).iter().map(|x| x.2).collect::<Vec<_>>(), [1, 2]);
    // not on a post of an agent's
    core.ticket_input(Input::Post { t, by: "w2".into(), text: Text::New("on it".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let n3 = core.tickets.m.tickets[&t].last();
    assert!(core.ticket_input(Input::Split { t, n: n3, from: from(2) }).is_err());
    // read back as the window reads a line (N-541; docket-core3, N-412 p441): a ticket's event, its change, and a
    // snapshot holding it, each with both points' `split_from` as recorded
    use coord_types::Body;
    let view = core.ticket_shown(t).unwrap();
    let marks = |v: &docket_machine::TicketView| -> Vec<(u32, Option<docket_machine::SplitFrom>)> { v.posts.iter().map(|p| (p.n, p.split_from.clone())).collect() };
    assert_eq!(marks(&view).iter().filter_map(|(_, f)| f.clone()).collect::<Vec<_>>(), [from(1), from(2)]);
    let bodies = [Body::Ticket(view.clone()), Body::TicketChange { view: view.clone(), posts: view.posts.len() as u32 }, Body::Snapshot(coord_types::Snapshot { tickets: vec![view.clone()], ..Default::default() })];
    for body in bodies {
        let line = serde_json::to_string(&coord_types::Event { seq: 7, body }).unwrap();
        assert!(line.contains("\"split_from\":{\"message\":\"m-1763500000000\""), "{line}");
        let back: coord_types::Event = serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line}"));
        let v = match back.body { Body::Ticket(v) | Body::TicketChange { view: v, .. } => v, Body::Snapshot(s) => s.tickets[0].clone(), _ => panic!("{line}") };
        assert_eq!(marks(&v), marks(&view));
    }
    // a restart reads the lines back
    let mut again = Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
    again.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
    again.load_tickets();
    assert!(again.tickets.errors.is_empty(), "{:?}", again.tickets.errors);
    assert_eq!(split(&again), split(&core));
    let _ = std::fs::remove_dir_all(&coord);
}
