//! A subthread opened at a post through coordd (paper-research N-447, piece 2): the owner's `say` with `under`, and
//! `coord msg --under`, open a thread tied at that post from its open, with the label given or cut from the first
//! line; a place that is not there, and `under` with `follows` or `--re`, are refused before anything is recorded.

use super::edit_tests::core;
use super::{Said, Task};
use crate::core::Who;
use docket_machine::Input;

#[test]
fn the_owners_words_and_an_agents_message_open_a_thread_at_a_post() {
    let (mut core, coord) = core("subthread");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    core.tickets.tasks = Some(tx);
    let pump = |core: &mut super::Core, rx: &mut tokio::sync::mpsc::UnboundedReceiver<Task>| {
        while let Ok(t) = rx.try_recv() {
            if let Task::Event { name, value } = t {
                core.ticket_input(Input::Event { name, value }).unwrap();
            }
        }
    };
    let last = |core: &super::Core| *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    assert!(core.ticket_say(&Who::Owner, "w2".into(), vec!["w2".into()], vec!["w2".into()], "the parser".into(), None).ok);
    pump(&mut core, &mut rx);
    let a = last(&core);
    let ia = core.tickets.m.id(a);
    // refused before anything is recorded: a place not there, under with follows
    let r = core.ticket_say_under(&Who::Owner, "w2".into(), vec!["w2".into()], vec![], "x".into(), None, Some(format!("{ia}.p7")), None, false);
    assert!(!r.ok && r.out.contains("no ticket or post"), "{}", r.out);
    let r = core.ticket_say_under(&Who::Owner, "w2".into(), vec!["w2".into()], vec![], "x".into(), Some(a), Some(ia.clone()), None, false);
    assert!(!r.ok, "{}", r.out);
    pump(&mut core, &mut rx);
    assert_eq!(last(&core), a);
    // the owner's, in a subthread at post 1, labelled: no intake, tied from its open
    let r = core.ticket_say_under(&Who::Owner, "w2".into(), vec!["w2".into()], vec![], "keep the comments\nas they are".into(), None, Some(format!("{ia}.p1")), Some(" comments ".into()), false);
    assert_eq!(r.out, format!("Taken: to w2, under {ia} at post 1."));
    pump(&mut core, &mut rx);
    let b = last(&core);
    assert_eq!(core.tickets.m.outline_parent(b), Some((a, 1)));
    assert!(matches!(core.tickets.m.tickets[&b].status, docket_machine::Status::Rests { .. }), "{:?}", core.tickets.m.tickets[&b].status);
    assert_eq!(docket_machine::ViewOutline::of(&core.tickets.m, &core.tickets.m.tickets[&b], false).label, "comments");
    // a message at the ticket (its last post), with --re refused
    let r = core.ticket_msg_under(&Who::Owner, None, vec!["w2".into()], "and the printer", false, Some(ia.clone()), None, Vec::new(), Said::default(), Some(ia.clone()), None, None);
    assert!(!r.ok, "{}", r.out);
    let r = core.ticket_msg_under(&Who::Owner, None, vec!["w2".into()], "and the printer", false, None, None, Vec::new(), Said::default(), Some(ia.clone()), None, None);
    assert!(r.ok, "{}", r.out);
    let c = last(&core);
    let n = core.tickets.m.tickets[&a].posts.last().unwrap().n;
    assert_eq!(core.tickets.m.outline_parent(c), Some((a, n)));
    assert!(docket_machine::ViewOutline::of(&core.tickets.m, &core.tickets.m.tickets[&c], false).label.contains("printer"));
    // a new top thread for sure: no intake, no tie
    let r = core.ticket_say_under(&Who::Owner, "w2".into(), vec!["w2".into()], vec![], "a topic of its own".into(), None, None, None, true);
    assert_eq!(r.out, "Taken: to w2.");
    pump(&mut core, &mut rx);
    let d = last(&core);
    assert_eq!(core.tickets.m.outline_parent(d), None);
    assert!(matches!(core.tickets.m.tickets[&d].status, docket_machine::Status::Rests { .. }), "no intake: {:?}", core.tickets.m.tickets[&d].status);
    // a restart reads the ties back from the open lines
    let mut again = super::Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
    again.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
    again.load_tickets();
    assert!(again.tickets.errors.is_empty(), "{:?}", again.tickets.errors);
    assert_eq!((again.tickets.m.outline_parent(b), again.tickets.m.outline_parent(c)), (Some((a, 1)), Some((a, n))));
    let _ = std::fs::remove_dir_all(&coord);
}

/// The quoted words under a subthread (paper-research N-447 p71): `under` T.pN:FROM+LEN checks the span against the
/// post's words and keeps those words in the thread's form, which the outline line shows; a span past the words, one
/// on a ticket with no post named, and one that is no FROM+LEN are refused before anything is recorded.
#[test]
fn a_subthread_keeps_the_words_of_the_post_it_was_opened_on() {
    let (mut core, coord) = core("subthread-quote");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    core.tickets.tasks = Some(tx);
    let pump = |core: &mut super::Core, rx: &mut tokio::sync::mpsc::UnboundedReceiver<Task>| {
        while let Ok(t) = rx.try_recv() {
            if let Task::Event { name, value } = t {
                core.ticket_input(Input::Event { name, value }).unwrap();
            }
        }
    };
    let last = |core: &super::Core| *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    let quote = |core: &super::Core, t: u64| docket_machine::ViewOutline::of(&core.tickets.m, &core.tickets.m.tickets[&t], false).quote;
    assert!(core.ticket_say(&Who::Owner, "w2".into(), vec!["w2".into()], vec!["w2".into()], "the parser and the lexer".into(), None).ok);
    pump(&mut core, &mut rx);
    let a = last(&core);
    let ia = core.tickets.m.id(a);
    for (under, says) in [(format!("{ia}.p1:4+60"), "runs past them"), (format!("{ia}:4+6"), "words are of one post"), (format!("{ia}.p1:x"), "FROM+LEN"), (format!("{ia}.p1:4+0"), "FROM+LEN"), (format!("{ia}.p1:{}+2", usize::MAX), "FROM+LEN"), (format!("{ia}.p1:2+{}", usize::MAX), "FROM+LEN"), (format!("{ia}.p1:{}+1", usize::MAX - 1), "runs past them")] {
        let r = core.ticket_say_under(&Who::Owner, "w2".into(), vec!["w2".into()], vec![], "x".into(), None, Some(under.clone()), None, false);
        assert!(!r.ok && r.out.contains(says), "{under}: {}", r.out);
    }
    pump(&mut core, &mut rx);
    assert_eq!(last(&core), a, "nothing recorded");
    // the owner's, on "parser"
    let r = core.ticket_say_under(&Who::Owner, "w2".into(), vec!["w2".into()], vec![], "keep its comments".into(), None, Some(format!("{ia}.p1:4+6")), None, false);
    assert_eq!(r.out, format!("Taken: to w2, under {ia} at post 1."));
    pump(&mut core, &mut rx);
    let b = last(&core);
    assert_eq!(core.tickets.m.outline_parent(b), Some((a, 1)));
    assert_eq!(quote(&core, b).as_deref(), Some("parser"));
    assert_eq!(core.tickets.m.tickets[&b].form["quote"], serde_json::json!({"from": 4, "len": 6, "words": "parser"}));
    // an agent's message, on "the lexer"
    let r = core.ticket_msg_under(&Who::Owner, None, vec!["w2".into()], "the lexer next", false, None, None, Vec::new(), Said::default(), Some(format!("{ia}.p1:15+9")), None, None);
    assert!(r.ok, "{}", r.out);
    let c = last(&core);
    assert_eq!(quote(&core, c).as_deref(), Some("the lexer"));
    // the outline of A has both lines, each with its words; a restart reads them back
    let kids = docket_machine::ViewOutline::of(&core.tickets.m, &core.tickets.m.tickets[&a], true).children;
    assert_eq!(kids.iter().map(|k| k.quote.clone()).collect::<Vec<_>>(), vec![Some("parser".to_string()), Some("the lexer".to_string())]);
    // the window reads A's view, its outline lines with their words, back in a whole event (N-542)
    let one = coord_types::Event { seq: 5, body: coord_types::Body::Ticket(core.ticket_shown(a).unwrap()) };
    let back: coord_types::Event = serde_json::from_str(&serde_json::to_string(&one).unwrap()).unwrap();
    assert_eq!(back, one);
    let coord_types::Body::Ticket(v) = back.body else { panic!() };
    assert_eq!(v.outline.map(|o| o.children.into_iter().map(|k| k.quote).collect::<Vec<_>>()), Some(vec![Some("parser".to_string()), Some("the lexer".to_string())]));
    let mut again = super::Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
    again.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
    again.load_tickets();
    assert!(again.tickets.errors.is_empty(), "{:?}", again.tickets.errors);
    assert_eq!((quote(&again, b), quote(&again, c)), (Some("parser".to_string()), Some("the lexer".to_string())));
    let _ = std::fs::remove_dir_all(&coord);
}
