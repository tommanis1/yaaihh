//! His sign-off of an ask with his word of worth (`SIGNED_OFF`; paper-research N-1446 p1786; consult-2's design, N-1002
//! p6: utility is his word at sign-off, with what it took beside it), and Open → done's asks row (`insight::asks`):
//! on a run's own log, replayed as a record is, and on made-up rows. Made up.

use super::*;
use crate::graph::{AskNow, AskTimes, Point, Reached, Signed};
use crate::insight::{Ask, ask_took, asks};
use crate::machine::{SIGNED_OFF, WORTHS};
use serde_json::json;

/// His press: post N of T signed off WORTH, by BY.
fn sign(r: &mut Run, by: &str, t: u64, n: u32, worth: &str) -> Out {
    r.go(Input::Event { name: SIGNED_OFF.into(), value: json!({"by": by, "t": t, "n": n, "worth": worth}) })
}

/// His ask's sign-off as the graph of the run's log reads it, at UPTO.
fn signed_at(r: &Run, post: &str, upto: Option<&str>) -> Option<Signed> {
    let (g, problems) = crate::graph::of(&mut Machine::load(defs()), &r.log);
    assert!(problems.is_empty(), "{problems:?}");
    g.ask_times_at(upto).into_iter().find(|a| a.post == post).unwrap_or_else(|| panic!("{post} is no ask of his")).signed
}

/// Two on a thread (his words to lead, lead's `@fixer`): his first post asks lead, so it is an ask of his. He signs
/// it off worth it: one `signed_off` line, his word kept, and the graph's course of it has it, at that line's time
/// and not before. Fine changes his word; none takes it back, and the course has none again. The run replays with the
/// same word, and a press that changes nothing still writes its line (coordd writes none then: `ticket_sign_off`).
#[test]
fn his_sign_off_is_on_his_asks_course_and_taking_it_back_clears_it() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    let n = r.m.tickets[&t].posts.iter().find(|p| p.by == "owner").unwrap().n;
    let post = format!("{}.p{n}", r.m.id(t));
    assert!(!r.m.asked(t, n).is_empty(), "his post asks lead: {:?}", r.m.asked(t, n));
    assert_eq!((r.m.signed_off(t, n), signed_at(&r, &post, None)), (None, None));
    let before = r.log.len();
    // the run's time before his press, as `Run::go` stamps it
    let then = format!("2026-11-10T12:00:{:02}Z", r.clock);
    let out = sign(&mut r, "owner", t, n, "worth_it");
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert_eq!(r.log.len(), before + 1, "one line");
    let Line::Event { at, name, fired, on, .. } = &r.log[before] else { panic!("{:?}", r.log[before]) };
    assert_eq!((name.as_str(), fired.is_empty(), on.is_none()), (SIGNED_OFF, true, true), "a record line of the machine's own");
    let when = at.clone();
    assert_eq!(r.m.signed_off(t, n), Some("worth_it"));
    assert_eq!(signed_at(&r, &post, None), Some(Signed { at: when.clone(), worth: "worth_it".into() }));
    assert!(r.clock < 60 && then < when);
    assert_eq!(signed_at(&r, &post, Some(&then)), None, "not before its line");
    sign(&mut r, "owner", t, n, "fine").reply.unwrap();
    assert_eq!(r.m.signed_off(t, n), Some("fine"));
    assert_eq!(signed_at(&r, &post, None).map(|s| s.worth), Some("fine".into()));
    assert_eq!(signed_at(&r, &post, Some(&when)).map(|s| s.worth), Some("worth_it".into()), "what he had said by then");
    sign(&mut r, "owner", t, n, "none").reply.unwrap();
    assert_eq!((r.m.signed_off(t, n), signed_at(&r, &post, None)), (None, None), "taken back");
    sign(&mut r, "owner", t, n, "not_worth_it").reply.unwrap();
    r.replays();
    let mut m2 = Machine::load(defs());
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.signed_off, r.m.signed_off, "a replay keeps his last word");
    assert_eq!(m2.signed_off(t, n), Some("not_worth_it"));
    assert_eq!(WORTHS, ["worth_it", "fine", "not_worth_it"], "his three words, in his press's order");
}

/// The machine refuses a sign-off by anyone but him, one with a word that is none of his, and one on a post that is
/// no ask of his (lead's post, which he did not write); none writes a line, and his word stays as it was.
#[test]
fn a_sign_off_by_an_agent_with_another_word_or_on_no_ask_of_his_is_refused() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    let his = r.m.tickets[&t].posts.iter().find(|p| p.by == "owner").unwrap().n;
    let leads = r.m.tickets[&t].posts.iter().find(|p| p.by == "lead").unwrap().n;
    sign(&mut r, "owner", t, his, "fine").reply.unwrap();
    let before = r.log.len();
    for (by, n, worth, says) in [("lead", his, "worth_it", "Only the owner"), ("owner", his, "great", "his word"), ("owner", leads, "worth_it", "no ask of his")] {
        let e = sign(&mut r, by, t, n, worth).reply.expect_err("refused");
        assert!(e.contains(says), "{by} {n} {worth}: {e}");
    }
    assert_eq!(r.log.len(), before, "no line");
    assert_eq!(r.m.signed_off(t, his), Some("fine"));
}

/// A made-up ask on N-1104: POST asked at ASKED (minutes past 10:00), with the course POINTS reached, each at its own
/// minute, signed off at SIGNED when some.
fn ask(post: &str, asked: u32, points: &[(Point, u32)], signed: Option<(u32, &str)>) -> Ask {
    let at = |m: u32| format!("2026-11-22T{:02}:{:02}:00Z", 10 + m / 60, m % 60);
    let r = |p: Point, who: &str| points.iter().find(|(q, _)| *q == p).map(|(_, m)| Reached { at: at(*m), id: who.into(), via: None });
    let times = AskTimes {
        post: post.into(),
        asked: Some(at(asked)),
        reached: r(Point::Reached, "agent:dev"),
        taken: r(Point::Taken, "agent:dev"),
        first_commit: r(Point::FirstCommit, "commit:0f40917b"),
        merged: r(Point::Merged, "commit:9a3c1e22"),
        first_green: r(Point::FirstGreen, "job:3-1"),
        in_window: r(Point::InWindow, "build:20261122-120000-9a3c1e22"),
        asks: vec!["agent:dev".into()],
        closed: r(Point::Closed, "agent:dev"),
        signed: signed.map(|(m, w)| Signed { at: at(m), worth: w.into() }),
    };
    let (point, last) = times.points().into_iter().rev().find_map(|(p, _, r)| Some((p, r?.clone()))).map_or((Point::Asked, None), |(p, r)| (p, Some(r)));
    Ask { ask: AskNow { point, last, who: Some("agent:dev".into()), times }, words: format!("the words of {post}"), on: "Your one group chat".into(), cost: None, after: Vec::new() }
}

/// Open → done's asks row on made-up asks, as coordd gives them, in the order he asked them: nobody moves holds the
/// one never taken and the one taken and open in the ledger with no commit, the oldest first, and neither the one at
/// work (a commit, not in his window) nor the one he signed off before anyone took it; answered in words, the one
/// closed with no commit; in his window, both installed, the latest first; signed off, the two he signed; and his
/// only, the one in his window he has not signed off. Open runs from asked to his sign-off, else to now.
#[test]
fn the_asks_row_counts_nobody_moves_answered_in_words_in_his_window_and_signed_off() {
    use Point::*;
    let rows = vec![
        ask("N-1104.p1", 0, &[(Reached, 1)], None),
        ask("N-1104.p2", 5, &[(Reached, 6), (Taken, 7)], None),
        ask("N-1104.p3", 10, &[(Reached, 11), (Taken, 12), (Closed, 12)], None),
        ask("N-1104.p4", 15, &[(Reached, 16), (Taken, 17), (FirstCommit, 30)], None),
        ask("N-1104.p5", 20, &[(Reached, 21), (Taken, 22), (FirstCommit, 40), (Merged, 50), (FirstGreen, 45), (InWindow, 90)], None),
        ask("N-1104.p6", 25, &[(Reached, 26), (Taken, 27), (FirstCommit, 41), (InWindow, 120)], Some((130, "worth_it"))),
        ask("N-1104.p7", 30, &[(Reached, 31)], Some((35, "not_worth_it"))),
    ];
    let n = asks(&rows);
    let ids = |v: &[&Ask]| v.iter().map(|a| a.ask.times.post.trim_start_matches("N-1104.").to_string()).collect::<Vec<_>>();
    assert_eq!(ids(&n.asked), ["p7", "p6", "p5", "p4", "p3", "p2", "p1"], "every one, the newest first");
    assert_eq!(ids(&n.stuck), ["p1", "p2"], "never taken; taken, open in the ledger, nothing done: the oldest first");
    assert_eq!(ids(&n.words), ["p3"]);
    assert_eq!(ids(&n.window), ["p6", "p5"], "the latest in his window first");
    assert_eq!(ids(&n.signed), ["p6", "p7"], "the latest sign-off first");
    assert_eq!(ids(&n.yours), ["p5"], "in his window, not signed off: the only one that waits for him");
    let now = docket_machine_secs("2026-11-22T13:00:00Z");
    assert_eq!(ask_took(&rows[5], now), Some(105 * 60), "asked 10:25, signed off 12:10");
    assert_eq!(ask_took(&rows[4], now), Some(160 * 60), "asked 10:20, open at 13:00");
    let mut bad = rows[0].clone();
    bad.ask.times.asked = Some("not a time".into());
    assert_eq!(ask_took(&bad, now), None, "never counted as zero");
    // a row reads back as coordd writes it, and one from before the sign-off (no `signed`) reads as not signed off
    let line = serde_json::to_string(&rows[5]).unwrap();
    assert_eq!(serde_json::from_str::<Ask>(&line).unwrap(), rows[5]);
    let mut old = serde_json::to_value(&rows[4]).unwrap();
    assert!(old["ask"]["times"].get("signed").is_none(), "none is left out");
    old["ask"]["times"].as_object_mut().unwrap().remove("signed");
    assert_eq!(serde_json::from_value::<Ask>(old).unwrap(), rows[4]);
}

fn docket_machine_secs(at: &str) -> i64 {
    crate::machine::secs(at).unwrap()
}
