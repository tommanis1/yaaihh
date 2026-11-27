//! His held messages through coordd (schedule.rs): held in the coord folder and read back after a restart, sent by
//! his Send now, at their time, once the thread they wait on closes; one refused when due is kept with its refusal.

use super::edit_tests::core;
use super::{Core, Said};
use crate::core::Who;
use coord_types::Request;
use coord_types::schedule::{Held, Sched, When};
use docket_machine::Input;
use serde_json::json;

fn say(core: &mut Core, text: &str) -> String {
    core.ticket_input(Input::Event { name: "owner_message".into(), value: json!({"from": "owner", "to": "w2", "typed": ["w2"], "names": ["w2"], "text": text, "addressed": {"head": "Named"}}) }).unwrap();
    let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
    core.tickets.m.id(t)
}

fn post(t: &str, text: &str) -> Request {
    Request::Post { kind: None, t: t.into(), text: text.into(), to: None, answers: None, wait: false, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), comments: Vec::new() }
}

/// How many posts thread T has.
fn posts(core: &Core, t: &str) -> usize {
    let n = core.tickets.m.ticket_named(t).unwrap();
    core.tickets.m.tickets[&n].posts.len()
}

/// The held messages as the file in COORD keeps them.
fn file(coord: &std::path::Path) -> Vec<Held> {
    let book: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(coord.join("scheduled.json")).unwrap()).unwrap();
    serde_json::from_value(book["held"].clone()).unwrap()
}

async fn act(shared: &crate::Shared, a: Sched) -> Result<Vec<Held>, String> {
    let r = crate::schedule::act(shared, &Who::Owner, a).await;
    if r.ok { Ok(serde_json::from_str(&r.out).unwrap()) } else { Err(r.out) }
}

#[test]
fn a_held_message_stays_over_a_restart_and_goes_when_he_says_its_time_comes_or_its_thread_closes() {
    let (mut core, coord) = core("schedule");
    let (a, b) = (say(&mut core, "the parser"), say(&mut core, "the lexer"));
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let shared = crate::Shared::new(core);
    rt.block_on(async {
        let add = |req: Request, when: When| Sched::Add { req: Box::new(req), when, place: format!("thread:{a}") };
        // only his, and only a post or a message
        assert!(!crate::schedule::act(&shared, &Who::Agent("w2".into()), Sched::List).await.ok);
        assert!(act(&shared, add(Request::Standing, When::Hold)).await.is_err());
        let held = act(&shared, add(post(&a, "held until I say"), When::Hold)).await.unwrap();
        assert_eq!(held.iter().map(|h| (h.id, h.text())).collect::<Vec<_>>(), [(1, "held until I say")]);
        act(&shared, add(post(&a, "at nine"), When::At { at: "2999-01-01T09:00:00Z".into() })).await.unwrap();
        act(&shared, add(post(&a, "once the lexer is done"), When::Closes { t: b.clone() })).await.unwrap();
        // nothing is due: nothing posted
        crate::schedule::send_due(&shared).await;
        let first = posts(&*shared.lock().await, &a);
        // a restart: the file read again holds all three
        let again = file(&coord);
        assert_eq!(again.iter().map(|h| h.text()).collect::<Vec<_>>(), ["held until I say", "at nine", "once the lexer is done"]);
        // Send now: posted, gone from the list
        let left = act(&shared, Sched::Send { id: 1 }).await.unwrap();
        assert_eq!(left.iter().map(|h| h.id).collect::<Vec<_>>(), [2, 3]);
        assert_eq!(posts(&*shared.lock().await, &a), first + 1, "posted");
        // its time come
        let mut book: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(coord.join("scheduled.json")).unwrap()).unwrap();
        book["held"][0]["when"] = json!({"is": "at", "at": "2000-01-01T00:00:00Z"});
        std::fs::write(coord.join("scheduled.json"), book.to_string()).unwrap();
        crate::schedule::send_due(&shared).await;
        assert_eq!(posts(&*shared.lock().await, &a), first + 2, "its time come: posted");
        // its thread closed
        assert!(shared.lock().await.ticket_end(&Who::Owner, &b, "done", "merged", Said::default()).ok);
        crate::schedule::send_due(&shared).await;
        assert_eq!(posts(&*shared.lock().await, &a), first + 3, "its thread closed: posted");
        assert!(act(&shared, Sched::List).await.unwrap().is_empty());
        // refused when due: kept, its refusal on it, and not tried again; dropped by him
        act(&shared, add(post("N-999", "nowhere"), When::At { at: "2000-01-01T00:00:00Z".into() })).await.unwrap();
        crate::schedule::send_due(&shared).await;
        let kept = act(&shared, Sched::List).await.unwrap();
        assert_eq!(kept.len(), 1);
        assert!(kept[0].failed.as_deref().is_some_and(|f| f.contains("N-999")), "{:?}", kept[0].failed);
        assert!(act(&shared, Sched::Drop { id: kept[0].id }).await.unwrap().is_empty());
    });
    let _ = std::fs::remove_dir_all(&coord);
}

/// An id is never given twice: not after the newest is deleted, nor while one is out being sent and comes back
/// refused; a time is kept in UTC as coordd writes it, and one that does not read is refused; the file is his alone,
/// with nothing left beside it; and no more than `MOST` are held.
#[test]
fn ids_only_grow_times_are_kept_in_utc_and_the_file_is_his_alone() {
    let (mut core, coord) = core("schedule-ids");
    let a = say(&mut core, "the parser");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let shared = crate::Shared::new(core);
    rt.block_on(async {
        let add = |text: &str, when: When| Sched::Add { req: Box::new(post(&a, text)), when, place: format!("thread:{a}") };
        act(&shared, add("one", When::Hold)).await.unwrap();
        act(&shared, add("two", When::Hold)).await.unwrap();
        act(&shared, Sched::Drop { id: 2 }).await.unwrap();
        let ids = |h: &[Held]| h.iter().map(|h| h.id).collect::<Vec<_>>();
        assert_eq!(ids(&act(&shared, add("three", When::Hold)).await.unwrap()), [1, 3], "2 is not given again");
        // one refused when sent comes back beside one held while it was out: two ids, so Delete takes one
        let out = act(&shared, Sched::Add { req: Box::new(post("N-999", "nowhere")), when: When::Hold, place: format!("thread:{a}") }).await.unwrap();
        let nowhere = *ids(&out).last().unwrap();
        assert!(act(&shared, Sched::Send { id: nowhere }).await.is_err());
        let back = act(&shared, Sched::List).await.unwrap();
        let mut seen = ids(&back);
        seen.dedup();
        assert_eq!(seen, ids(&back), "no id twice: {back:?}");
        act(&shared, Sched::Drop { id: nowhere }).await.unwrap();
        // a time with an offset kept in UTC; one that does not read refused
        let at = act(&shared, add("at ten in Amsterdam", When::At { at: "2999-01-01T10:00:00.5+02:00".into() })).await.unwrap();
        assert_eq!(at.last().unwrap().when, When::At { at: "2999-01-01T08:00:00Z".into() });
        assert!(act(&shared, add("whenever", When::At { at: "tomorrow".into() })).await.unwrap_err().contains("not a time"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(coord.join("scheduled.json")).unwrap().permissions().mode() & 0o777, 0o600);
        }
        assert!(!coord.join("scheduled.json.part").exists());
        let held = act(&shared, Sched::List).await.unwrap().len();
        for i in held..crate::schedule::MOST {
            act(&shared, add(&format!("n{i}"), When::Hold)).await.unwrap();
        }
        assert!(act(&shared, add("one too many", When::Hold)).await.unwrap_err().contains("held already"));
    });
    let _ = std::fs::remove_dir_all(&coord);
}

/// A message out being sent stays in the file until its answer: when coordd stopped before that, it is found again
/// at the start, held, saying it may have gone, and is not sent again unasked; one sent leaves nothing behind.
#[test]
fn a_message_out_being_sent_when_coordd_stops_is_found_again_not_lost() {
    let (mut core, coord) = core("schedule-crash");
    let a = say(&mut core, "the parser");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let shared = crate::Shared::new(core);
    rt.block_on(async {
        let add = |text: &str| Sched::Add { req: Box::new(post(&a, text)), when: When::Hold, place: format!("thread:{a}") };
        act(&shared, add("sent")).await.unwrap();
        act(&shared, Sched::Send { id: 1 }).await.unwrap();
        let book = |c: &std::path::Path| -> serde_json::Value { serde_json::from_str(&std::fs::read_to_string(c.join("scheduled.json")).unwrap()).unwrap() };
        assert!(book(&coord).get("sending").is_none(), "sent: nothing left being sent");
        // coordd stopped with message 2 out being sent: the file as it was then
        act(&shared, add("in flight")).await.unwrap();
        let mut b = book(&coord);
        b["sending"] = b["held"].clone();
        b["held"] = json!([]);
        std::fs::write(coord.join("scheduled.json"), b.to_string()).unwrap();
        let before = posts(&*shared.lock().await, &a);
        crate::schedule::found_again(&shared.lock().await.coord.clone()).await;
        let back = act(&shared, Sched::List).await.unwrap();
        assert_eq!(back.iter().map(|h| (h.id, h.text(), h.failed.as_deref())).collect::<Vec<_>>(), [(2, "in flight", Some(crate::schedule::MAY_HAVE_GONE))]);
        crate::schedule::send_due(&shared).await;
        assert_eq!(posts(&*shared.lock().await, &a), before, "not sent again unasked");
    });
    let _ = std::fs::remove_dir_all(&coord);
}
