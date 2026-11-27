//! His sign-off of an ask with its word of worth, on the way back (paper-research N-1446 p1786, 1): his press writes
//! one `signed_off` line, an event of the machine's own as `brain_switch` is (docket-edge5 p1793). A record holding
//! such lines loads in an engine from before them as the same record without them: the same tickets, posts, labels and
//! asks, no problem line at its start and no `{"error": …}` row in `coord list` (p1794: the two places 68 would show
//! a line it cannot read), from the state saved before them and the lines after it, and replayed whole. The lines are
//! written here by their words alone, so this leg runs as it is on an engine that has never heard of them (train 68's)
//! and on the one that reads them.

use super::edit_tests::core;
use docket_machine::Input;
use serde_json::json;

/// His message to w2 opening a thread: its id and number, and the number of his post on it.
fn say(core: &mut super::Core, text: &str) -> (String, u64, u32) {
    core.ticket_input(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": "w2", "typed": ["w2"], "names": ["w2"], "text": text, "addressed": {"head": "Named"}}) }).unwrap();
    let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    let n = core.tickets.m.tickets[&t].posts.iter().find(|p| p.by == "owner").expect("his post").n;
    (core.tickets.m.id(t), t, n)
}

/// The `signed_off` line his press writes, whole: his word WORTH (`worth_it`, `fine`, `not_worth_it`; `none` takes it
/// back) on post N of T, at AT.
fn signed_off(at: &str, t: u64, n: u32, worth: &str) -> String {
    json!({"ev": "event", "at": at, "name": "signed_off", "value": {"by": "owner", "t": t, "n": n, "worth": worth}, "fired": []}).to_string()
}

/// What a load gives that the window and the agents read: every ticket's view, the labels of the threads and his
/// posts, whom each of his posts asks (the ASKED ledger), `coord list`'s rows (`Request::Tickets` with no filter, which
/// adds one `{"error": …}` row for each problem line), and the problem lines its start wrote.
#[allow(clippy::type_complexity)]
fn read(core: &super::Core, asks: &[(String, u64, u32)]) -> (serde_json::Value, Vec<coord_types::Label>, Vec<Vec<(String, Option<String>)>>, Vec<serde_json::Value>, Vec<String>) {
    let views = serde_json::to_value(core.ticket_views()).unwrap();
    let ids: Vec<String> = asks.iter().flat_map(|(id, _, n)| [id.clone(), format!("{id}.p{n}")]).collect();
    let (labels, _, _) = core.labels(&ids);
    let asked = asks.iter().map(|(_, t, n)| core.tickets.m.asked(*t, *n)).collect();
    let list = core.ticket_list(&crate::core::Who::Owner, &json!({}));
    assert!(list.ok, "{}", list.out);
    let rows = list.out.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    (views, labels, asked, rows, core.tickets.errors.clone())
}

/// LOAD's record is WITHOUT's, at a start with no problem line and a list with no error row.
#[allow(clippy::type_complexity)]
fn same(with: &(serde_json::Value, Vec<coord_types::Label>, Vec<Vec<(String, Option<String>)>>, Vec<serde_json::Value>, Vec<String>), without: &(serde_json::Value, Vec<coord_types::Label>, Vec<Vec<(String, Option<String>)>>, Vec<serde_json::Value>, Vec<String>), load: &str) {
    assert!(with.4.is_empty(), "{load}: no problem line at its start: {:?}", with.4);
    assert!(with.3.iter().all(|r| r.get("error").is_none()), "{load}: no error row in the list: {:?}", with.3);
    assert!(!with.3.is_empty(), "{load}: the list has its threads");
    assert_eq!(with.0, without.0, "{load}: the same tickets and posts");
    assert_eq!(with.1, without.1, "{load}: the same labels");
    assert_eq!(with.2, without.2, "{load}: the same asks, each of w2 and open");
    assert_eq!(with.3, without.3, "{load}: the same list");
}

#[test]
fn a_record_with_his_sign_offs_loads_as_the_same_record_without_them() {
    let (mut core, coord) = core("signed-off");
    let asks = vec![say(&mut core, "the parser"), say(&mut core, "the lexer")];
    for (id, t, n) in &asks {
        assert_eq!(core.tickets.m.asked(*t, *n), vec![("w2".to_string(), None)], "{id}: his post wakes w2, so the ledger counts it an ask of w2");
    }
    let load = || {
        let mut c = super::Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
        c.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        c.load_tickets();
        c
    };
    let without = read(&load(), &asks);
    assert!(without.4.is_empty(), "{:?}", without.4);
    assert!(without.3.iter().all(|r| r.get("error").is_none()), "{:?}", without.3);
    // his word on the first, worth it; on the second, fine, then taken back
    let log = coord.join("tickets/log.jsonl");
    let mut text = std::fs::read_to_string(&log).unwrap();
    let at = crate::core::now();
    let (a, b) = (&asks[0], &asks[1]);
    for l in [signed_off(&at, a.1, a.2, "worth_it"), signed_off(&at, b.1, b.2, "fine"), signed_off(&at, b.1, b.2, "none")] {
        text.push_str(&l);
        text.push('\n');
    }
    std::fs::write(&log, text).unwrap();
    // a start from the state the load before saved, and the three lines after it
    let saved = coord.join(super::replay_state::FOLDER);
    assert!(saved.is_dir(), "the load before saved its state");
    let again = load();
    same(&read(&again, &asks), &without, "from the state saved");
    // and a start that replays the record whole, as 68's coordd does on a record a later engine saved a state of
    std::fs::remove_dir_all(&saved).unwrap();
    let whole = load();
    same(&read(&whole, &asks), &without, "whole");
    drop(whole);
    // and the record goes on from them: his next message opens its thread, as before
    let mut again = again;
    let (_, t, n) = say(&mut again, "the printer");
    assert!(again.tickets.m.view(t).is_some_and(|v| v.kind == "thread"));
    assert_eq!(again.tickets.m.asked(t, n), vec![("w2".to_string(), None)]);
    let _ = std::fs::remove_dir_all(&coord);
}

/// His press through coordd (`Request::SignOff`, from ASKS_PROTOCOL): the owner's alone; one `signed_off` line for a
/// press that changes his word, none for one that does not; a post there is not is refused. Open → done's asks
/// (`Insight::Asks`, the owner's) give each of his asks with his post's words, its thread's label and his word.
#[test]
fn his_sign_off_through_coordd_is_one_line_a_change_and_the_asks_row_says_his_word() {
    use crate::core::Who;
    let (mut core, coord) = core("sign-off");
    let (id, _, n) = say(&mut core, "the parser");
    let post = format!("{id}.p{n}");
    let lines = || std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap().lines().filter(|l| l.contains(&format!("\"{}\"", docket_machine::SIGNED_OFF))).count();
    let row = |core: &super::Core| -> docket_machine::insight::Ask {
        let r = core.insight(&Who::Owner, coord_types::Insight::Asks);
        assert!(r.ok, "{}", r.out);
        r.out.lines().map(|l| serde_json::from_str::<docket_machine::insight::Ask>(l).unwrap()).find(|a| a.ask.times.post == post).unwrap_or_else(|| panic!("{post} not among {}", r.out))
    };
    assert!(!core.ticket_sign_off(&Who::Agent("w2".into()), &post, "fine").ok, "an agent's is refused");
    assert!(!core.insight(&Who::Agent("w2".into()), coord_types::Insight::Asks).ok, "the asks are his");
    assert_eq!(lines(), 0);
    let first = row(&core);
    assert_eq!((first.words.as_str(), first.on.is_empty(), first.ask.times.signed.clone()), ("the parser", false, None), "{first:?}");
    let r = core.ticket_sign_off(&Who::Owner, &post, "worth_it");
    assert!(r.ok, "{}", r.out);
    assert_eq!(lines(), 1);
    assert_eq!(row(&core).ask.times.signed.map(|s| s.worth), Some("worth_it".to_string()));
    let r = core.ticket_sign_off(&Who::Owner, &post, "worth_it");
    assert!(r.ok && r.out.contains("already"), "{}", r.out);
    assert_eq!(lines(), 1, "a press that changes nothing writes nothing");
    assert!(core.ticket_sign_off(&Who::Owner, &post, "none").ok);
    assert_eq!((lines(), row(&core).ask.times.signed), (2, None), "taken back");
    assert!(core.ticket_sign_off(&Who::Owner, &post, "none").out.contains("not signed off"));
    assert!(!core.ticket_sign_off(&Who::Owner, &format!("{id}.p99"), "fine").ok, "no such post");
    assert_eq!(lines(), 2);
    let _ = std::fs::remove_dir_all(&coord);
}
