//! An agy session that pulls (pull.rs, N-1998): its first message is pushed as any; once it has pulled, only the
//! owner's words are pushed, and only while no turn runs. `read-brain` answers std.bridges.agy's `Briefing` with what
//! it handed; `report-work` answers those messages; a pull missed past its wake and grace is told once.

use crate::cgroup::Caller;
use crate::core::{Core, Job, Who};
use crate::core::delivery_tests::{coord, end_turn, item, open, t_of};
use docket_machine::Input;
use crate::pull::{GRACE_MINS, SLEEP_MINS, WORK_MINS};
use coord_types::Request;
use serde_json::Value;
use tokio::sync::mpsc::UnboundedReceiver;

/// The ids w2's hand-over pushes now, if anything.
fn pushed(core: &mut Core, rx: &mut UnboundedReceiver<Job>) -> Option<Vec<String>> {
    core.hand_over("w2");
    let mut got = None;
    while let Ok(j) = rx.try_recv() {
        if let Job::Codex { ids, .. } = j {
            got = Some(ids);
        }
    }
    got
}

/// w2's pull through coordd's socket, as `coord read-brain NOTE` sends it: its briefing.
async fn pull(shared: &crate::Shared, note: &str) -> Value {
    let r = crate::handle(shared, &Caller::Agent("w2".into()), 0, Request::ReadBrain { note: note.into() }).await;
    assert!(r.ok, "{}", r.out);
    serde_json::from_str(&r.out).unwrap()
}

fn agy(tag: &str) -> (std::path::PathBuf, Core, UnboundedReceiver<Job>) {
    let (dir, _, mut core, rx) = coord(tag, "idle");
    core.agents.get_mut("w2").unwrap().kind = "agy".into();
    (dir, core, rx)
}

/// Before its first pull a session is pushed to as before (its kick-off), and what comes in its turn at the turn's end.
/// After it, a lead's post is never pushed, not at a turn's end either, and the owner's only while no turn runs.
/// `to_hand_over` names it only for what is pushed.
#[tokio::test]
async fn once_it_has_pulled_only_the_owners_words_are_pushed_and_only_between_turns() {
    let (dir, mut core, mut rx) = agy("pull-gate");
    let first = open(&mut core, "lead", &["w2"], "kick off: map the store");
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert_eq!(pushed(&mut core, &mut rx), Some(vec![first.clone()]), "its first message is pushed");
    core.handed("w2", &[first], true, "codex");
    core.begin_turn("w2");
    let before = open(&mut core, "lead", &["w2"], "and the index");
    end_turn(&mut core, &dir, "Nothing");
    assert_eq!(core.to_hand_over(), ["w2"], "not yet pulled: pushed at its turn's end, as any session");
    assert_eq!(pushed(&mut core, &mut rx), Some(vec![before.clone()]));
    core.handed("w2", &[before], true, "codex");
    let shared = crate::Shared::new(core);
    pull(&shared, "starting").await;
    let mut core = shared.lock().await;
    core.begin_turn("w2");
    let lead = open(&mut core, "lead", &["w2"], "also the tests");
    end_turn(&mut core, &dir, "Nothing");
    assert!(core.to_hand_over().is_empty(), "a lead's post waits for its pull, past the turn's end");
    assert_eq!(pushed(&mut core, &mut rx), None);
    core.begin_turn("w2");
    let his = open(&mut core, "owner", &["w2"], "stop and look at N-12");
    assert!(core.to_hand_over().is_empty() && pushed(&mut core, &mut rx).is_none(), "nothing into its running turn, not even his");
    end_turn(&mut core, &dir, "Nothing");
    assert_eq!(core.to_hand_over(), ["w2"]);
    assert_eq!(pushed(&mut core, &mut rx), Some(vec![his.clone()]), "his words, alone, between turns");
    core.handed("w2", &[his], true, "codex");
    assert!(core.to_hand_over().is_empty() && pushed(&mut core, &mut rx).is_none(), "taken: not pushed again");
    assert!(core.tickets.m.due("w2").iter().any(|d| d.id == lead), "the lead's still waits for the pull");
    drop(core);
    let _ = std::fs::remove_dir_all(&dir);
}

/// std.bridges.agy's `Briefing`: its fields' names, as the docket declares them.
fn briefing_fields() -> Vec<String> {
    let text = include_str!("../../../../docket/std/bridges/agy.docket");
    let decl = text.split("type Briefing {").nth(1).and_then(|r| r.split('}').next()).expect("agy.docket declares Briefing");
    let mut names: Vec<String> = decl.split(',').map(|f| f.split(':').next().unwrap().trim().to_string()).collect();
    names.sort();
    names
}

/// A pull answers exactly the briefing's fields, as agy.docket declares them: what waited, handed now and not again, "work" while anything
/// was handed, "sleep" with the sleep wake when nothing waits; the owner's words handed are its directives.
#[tokio::test]
async fn a_pull_hands_what_waits_once_and_answers_the_briefing() {
    let (dir, mut core, _rx) = agy("pull-brief");
    let lead = open(&mut core, "lead", &["w2"], "the store's index, please");
    let his = open(&mut core, "owner", &["w2"], "the index first");
    let shared = crate::Shared::new(core);
    let b = pull(&shared, "on the store").await;
    let mut keys: Vec<&str> = b.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, briefing_fields(), "{b}");
    assert_eq!(keys.len(), 7);
    assert_eq!((b["mode"].as_str(), b["suggested_wake_mins"].as_i64()), (Some("work"), Some(WORK_MINS)), "{b}");
    let said = b["unread_messages"].as_array().unwrap().iter().map(|m| m.as_str().unwrap()).collect::<Vec<_>>().join("\n");
    assert!(said.contains("the store's index, please") && said.contains("the index first"), "{said}");
    let directive = b["priority_directive"].as_str().unwrap();
    assert!(directive.starts_with(his.split('.').next().unwrap()), "{b}");
    assert_eq!(b["new_directives"].as_array().unwrap().len(), 1, "{b}");
    {
        let core = shared.lock().await;
        let p = core.agents["w2"].live.pull.clone().expect("its pull noted");
        assert_eq!((p.note.as_str(), p.wake), ("on the store", WORK_MINS));
        assert!(p.handed.contains(&lead) && p.handed.contains(&his), "{p:?}");
        assert!(core.tickets.m.due("w2").is_empty(), "handed");
    }
    let again = pull(&shared, "").await;
    assert_eq!((again["mode"].as_str(), again["suggested_wake_mins"].as_i64()), (Some("work"), Some(WORK_MINS)), "unanswered: work still {again}");
    assert_eq!((again["unread_messages"].as_array().unwrap().len(), again["new_directives"].as_array().unwrap().len()), (0, 0), "nothing handed twice");
    assert_eq!(again["priority_directive"].as_str(), Some(directive), "his word stands until a report answers it");
    let r = crate::handle(&shared, &Caller::Agent("w2".into()), 0, Request::ReportWork { summary: "indexed".into(), refs: vec![], item: None }).await;
    assert!(r.ok, "{}", r.out);
    let after = pull(&shared, "").await;
    assert_eq!((after["mode"].as_str(), after["suggested_wake_mins"].as_i64(), after["priority_directive"].as_str()), (Some("sleep"), Some(SLEEP_MINS), Some("")), "answered: {after}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `report-work` posts the summary and its refs as w2's answer on the first message its pulls handed, the others
/// answered with it, and clears them: a second report with nothing to answer and no item is refused.
#[tokio::test]
async fn a_report_answers_what_its_pulls_handed() {
    let (dir, mut core, _rx) = agy("pull-report");
    let lead = open(&mut core, "lead", &["w2"], "the store's index, please");
    let shared = crate::Shared::new(core);
    pull(&shared, "").await;
    let report = |summary: &str| Request::ReportWork { summary: summary.into(), refs: vec!["abc1234".into()], item: None };
    let r = crate::handle(&shared, &Caller::Agent("w2".into()), 0, report("indexed the store")).await;
    assert!(r.ok, "{}", r.out);
    {
        let core = shared.lock().await;
        let x = &core.tickets.m.tickets[&t_of(&lead)];
        let p = x.posts.last().unwrap();
        assert_eq!((p.by.as_str(), p.answers), ("w2", Some(1)));
        let docket_machine::Text::New(text) = &p.text else { panic!("its words as posted") };
        assert!(text.contains("indexed the store") && text.contains("Shown by: abc1234"), "{text}");
        assert!(core.agents["w2"].live.pull.as_ref().unwrap().handed.is_empty());
    }
    let again = crate::handle(&shared, &Caller::Agent("w2".into()), 0, report("more")).await;
    assert!(!again.ok && again.out.contains("no message your pulls handed"), "{}", again.out);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A pull missed past its wake and the grace is told once for its due time; a session of another harness never pulls.
#[tokio::test]
async fn a_missed_pull_is_told_once_and_only_agy_pulls() {
    let (dir, core, _rx) = agy("pull-missed");
    let shared = crate::Shared::new(core);
    pull(&shared, "").await;
    let mut core = shared.lock().await;
    assert!(core.pulls_missed().is_empty(), "not due yet");
    let p = core.agents.get_mut("w2").unwrap().live.pull.as_mut().unwrap();
    p.at -= (p.wake + GRACE_MINS) * 60 + 1;
    assert_eq!(core.pulls_missed(), ["w2"]);
    assert!(core.pulls_missed().is_empty(), "told once");
    assert!(core.agents["w2"].live.pull.as_ref().unwrap().missed.is_some());
    core.agents.get_mut("w2").unwrap().kind = "claude".into();
    drop(core);
    let r = crate::handle(&shared, &Caller::Agent("w2".into()), 0, Request::ReadBrain { note: String::new() }).await;
    assert!(!r.ok && r.out.contains("only an agy agent pulls"), "{}", r.out);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A pull reads only its agent's open items and checks (`Holds`): its own closed items, 10 or 100, are not touched; an
/// open one is held and is its assigned item; closed it is held no more, and reopened held again (consult-lead-c N-1895
/// p331).
#[tokio::test]
async fn a_pull_reads_only_its_agents_open_items_however_many_are_closed() {
    let (dir, mut core, _rx) = agy("pull-holds");
    let w2 = Who::Agent("w2".into());
    let held = |core: &Core| core.tickets.holds.of_agent("w2").count();
    let mut closed = 0;
    for n in [10, 100] {
        while closed < n {
            let (id, t) = item(&mut core, "an old one");
            assert!(core.item_start(&w2, &id).ok);
            core.ticket_input(Input::Cancel { t, by: "owner".into(), why: "done with".into() }).unwrap();
            closed += 1;
        }
        assert_eq!(held(&core), 0, "{n} closed items of its own: none held");
    }
    // their closes, due to it, taken as a pull takes them: the pull below renders only the open one's
    let due = core.due_now("w2");
    let told: Vec<String> = core.ticket_handing("w2", due).into_iter().map(|(d, _)| d.id).collect();
    core.handed("w2", &told, true, "pull");
    assert!(core.due_now("w2").is_empty());
    let (id, t) = item(&mut core, "the index");
    assert!(core.item_start(&w2, &id).ok);
    assert_eq!(held(&core), 1);
    let shared = crate::Shared::new(core);
    let b = pull(&shared, "").await;
    assert_eq!((b["mode"].as_str(), &b["assigned_items"]), (Some("work"), &serde_json::json!([format!("{id}: the index")])), "{b}");
    let mut core = shared.lock().await;
    core.ticket_input(Input::Cancel { t, by: "owner".into(), why: "not now".into() }).unwrap();
    assert_eq!(held(&core), 0, "closed: held no more");
    core.ticket_input(Input::Reopen { t, by: "owner".into() }).unwrap();
    assert_eq!(held(&core), 1, "reopened, its start kept: held again");
    drop(core);
    let _ = std::fs::remove_dir_all(&dir);
}

/// His message pushed to an idle agy session, as its kick-off or after it has pulled (N-1998 p7, 1): its pull says
/// "work", his words its directive and the message named to answer, and `report-work` answers it on his thread.
#[tokio::test]
async fn his_pushed_message_is_its_pulls_work_and_its_reports_answer() {
    for pulled_first in [false, true] {
        let (dir, core, mut rx) = agy(if pulled_first { "pull-pushed-after" } else { "pull-pushed-kick" });
        let shared = crate::Shared::new(core);
        if pulled_first {
            pull(&shared, "").await;
        }
        let mut core = shared.lock().await;
        let his = open(&mut core, "owner", &["w2"], "@w2 write the intro");
        assert_eq!(core.to_hand_over(), ["w2"]);
        assert_eq!(pushed(&mut core, &mut rx), Some(vec![his.clone()]), "pulled first: {pulled_first}");
        core.handed("w2", &[his.clone()], true, "codex");
        drop(core);
        let b = pull(&shared, "").await;
        assert_eq!(b["mode"].as_str(), Some("work"), "pulled first: {pulled_first}: {b}");
        assert!(b["priority_directive"].as_str().unwrap().starts_with(his.split('.').next().unwrap()), "{b}");
        assert!(b["unread_messages"].as_array().unwrap().iter().any(|m| m.as_str().unwrap().contains(&his)), "{b}");
        let r = crate::handle(&shared, &Caller::Agent("w2".into()), 0, Request::ReportWork { summary: "the intro is drafted".into(), refs: vec![], item: None }).await;
        assert!(r.ok, "pulled first: {pulled_first}: {}", r.out);
        let core = shared.lock().await;
        let p = core.tickets.m.tickets[&t_of(&his)].posts.last().unwrap();
        assert_eq!((p.by.as_str(), p.answers), ("w2", Some(1)), "on his thread, answering him");
        drop(core);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A pull while a push is on its way (N-1998 p7, 2): the push's entry stays as it was, what it carries is not handed
/// again by the pull, and the push's own end is taken as ever.
#[tokio::test]
async fn a_pull_leaves_a_push_on_its_way_alone() {
    let (dir, core, mut rx) = agy("pull-inflight");
    let shared = crate::Shared::new(core);
    pull(&shared, "").await;
    let mut core = shared.lock().await;
    let his = open(&mut core, "owner", &["w2"], "the figures too");
    assert_eq!(pushed(&mut core, &mut rx), Some(vec![his.clone()]));
    let lead = open(&mut core, "lead", &["w2"], "and the table");
    drop(core);
    let b = pull(&shared, "").await;
    let said = b["unread_messages"].as_array().unwrap().iter().map(|m| m.as_str().unwrap()).collect::<Vec<_>>().join("\n");
    assert!(said.contains("and the table") && !said.contains("the figures too"), "the push's own not pulled: {said}");
    let mut core = shared.lock().await;
    assert_eq!(core.inflight.get("w2").map(|g| g.ids.clone()), Some(vec![his.clone()]), "the push's entry kept");
    core.handed("w2", &[his.clone()], true, "codex");
    assert!(core.inflight.get("w2").is_none(), "the push's end taken");
    let p = core.agents["w2"].live.pull.clone().unwrap();
    assert!(p.handed.contains(&lead), "{p:?}");
    drop(core);
    let _ = std::fs::remove_dir_all(&dir);
}
