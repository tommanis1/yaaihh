//! Archive (the owner, N-1073; paper-research N-867.p21; docket-core7's plan N-867.p25, docket-core3's N-867.p27): the
//! owner marks a ticket as no longer of importance to him. It is not an end. Nothing at or under it is handed while it
//! is archived, its hole is put to no minder, and Unarchive hands what was held as it would have been. This release
//! takes an archive live; the release before read one only in a record it replays (`archiving_live` stands as it was
//! there, a pass-through here).

use super::*;
use crate::machine::{ARCHIVED_NOTICE, archiving_live};

fn archive(r: &mut Run, t: u64, on: bool, by: &str) -> Out {
    r.go(Input::Archive { t, on, by: by.into() })
}

/// His archive of a thread two agents are on, live: an agent's is refused in words
/// and writes nothing; his writes one line, and the thread stays as it was, not ended. While it is archived an
/// agent's post on it wakes nobody, its `asks` step and the minder are not handed anything, and whoever policy names
/// is told once, quietly. His unarchive writes one line and hands what was held, and the minder is asked then. This
/// release replays the record to the same views.
#[test]
fn his_archive_holds_what_is_on_it_and_unarchive_hands_it_as_it_was() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    archiving_live(|| {
        let out = archive(&mut r, t, true, "lead");
        assert_eq!(out.reply, Err("Only the owner archives a ticket, or unarchives one: it says what is of importance to him.".into()));
        assert!(out.lines.is_empty(), "{:?}", out.lines);
        let out = archive(&mut r, t, true, "owner");
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        assert!(matches!(out.lines.as_slice(), [Line::Archive { on: true, by, .. }] if by == "owner"), "{:?}", out.lines);
        assert!(out.effects.contains(&Effect::Notify { t, event: "archived".into(), actor: "owner".into(), post: None }), "{:?}", out.effects);
        let v = r.m.view(t).unwrap();
        assert!(v.archived && v.archived_under.is_none());
        assert_eq!(v.status, ViewStatus::Resting, "not an end");
        assert!(v.outline.as_ref().is_none_or(|o| o.archived));
        assert_eq!(archive(&mut r, t, true, "owner").reply, Ok(format!("{} is archived already.", r.m.id(t))));
        // an agent's post on it: nothing is handed, not the watcher's step, not its wake
        r.post_on(t, "fixer", "@lead the median is fixed", &["lead"]).reply.unwrap();
        assert!(!r.woken("lead", t) && !r.informed("lead", t), "{:?}", r.m.due("lead"));
        // a step under it (the watcher's `asks` on the post, where it runs) is kept and not handed
        for (h, _) in r.converting() {
            assert!(r.m.due("watcher").iter().all(|d| d.t != h), "{:?}", r.m.due("watcher"));
        }
        assert!(!r.asking_minder(t));
        // whoever policy names is told once, quietly: the one thing handed from it
        let out = r.go(Input::Notified { t, event: "archived".into(), to: vec![], inform: vec!["fixer".into(), "owner".into()], post: None, provenance: json!({}) });
        assert!(out.reply.is_ok() && out.lines.len() == 1, "{out:?}");
        let told = r.m.due("fixer");
        assert_eq!(told.len(), 1, "{told:?}");
        assert!(told[0].inform && told[0].id == format!("{}.{ARCHIVED_NOTICE}", r.m.id(t)), "{:?}", told[0]);
        assert!(matches!(&told[0].what, What::Words { text: Text::New(w), from } if from == "owner" && w.contains("archived")), "{:?}", told[0].what);
        r.go(Input::Given { t, to: "fixer".into(), delivery: told[0].id.clone(), upto: told[0].upto, skipped: Default::default(), place: Some(Run::loc(60)) }).reply.unwrap();
        assert!(r.m.due("fixer").is_empty(), "told once");
        // his unarchive: one line, and what was held is handed as it would have been
        let out = archive(&mut r, t, false, "lead");
        assert!(out.reply.is_err() && out.lines.is_empty(), "{out:?}");
        let out = archive(&mut r, t, false, "owner");
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        assert!(matches!(out.lines.as_slice(), [Line::Archive { on: false, .. }]), "{:?}", out.lines);
        assert!(!r.m.view(t).unwrap().archived);
        assert!(r.woken("lead", t), "{:?}", r.m.due("lead"));
        // the minder is asked once it is quiet again, as something happened on it while archived
        r.nothing_asked();
        r.settle(70);
        assert!(r.asking_minder(t), "{:?}", r.m.tickets[&t].status);
    });
    r.replays();
}

/// A ticket opened under an archived thread is held too, and `archived_under` names the thread: worked out from its
/// parents at each view, never written, so his unarchive brings it back with no line of its own (docket-core3,
/// N-867.p27, 2). Who holds work under it is the step's agent and the ticket's opener.
#[test]
fn a_ticket_under_an_archived_one_is_held_and_archived_under_is_worked_out() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    archiving_live(|| {
        archive(&mut r, t, true, "owner").reply.unwrap();
        r.go(Input::Open { kind: "question".into(), under: Some(t), opener: "fixer".into(), form: json!({"ask": "which median?"}), text: None, names: vec![], told: vec![] }).reply.unwrap();
        let q = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
        assert_eq!(r.m.tickets[&q].under, Some(t));
        r.decide(q, 1, "lead");
        let v = r.m.view(q).unwrap();
        assert!(!v.archived);
        assert_eq!(v.archived_under, Some(r.m.id(t)));
        assert!(r.m.due("lead").iter().all(|d| d.t != q), "held: {:?}", r.m.due("lead"));
        assert_eq!(r.m.works_under(t), std::collections::BTreeSet::from(["fixer".to_string(), "lead".to_string()]));
        let out = archive(&mut r, t, false, "owner");
        assert!(matches!(out.lines.as_slice(), [Line::Archive { t: u, on: false, .. }] if *u == t), "{:?}", out.lines);
        assert!(out.effects.contains(&Effect::Deliver { to: "lead".into() }), "{:?}", out.effects);
        assert_eq!(r.m.view(q).unwrap().archived_under, None);
        assert!(r.m.due("lead").iter().any(|d| d.t == q && d.id.ends_with(".s1")), "{:?}", r.m.due("lead"));
    });
    r.replays();
}

/// This release takes his archive live, and a machine replaying the record holds the thread archived. His own post
/// there unarchives it, so his words are never held (docket-core3's condition, N-867.p27), and that record replays.
/// An archive line not by him is refused on replay too, in the load's words (docket-core3, N-867.p27, 1).
#[test]
fn his_archive_is_taken_live_and_his_post_there_unarchives_it() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    let out = archive(&mut r, t, true, "owner");
    assert!(out.reply.is_ok() && matches!(out.lines.as_slice(), [Line::Archive { on: true, .. }]), "{out:?}");
    let text: Vec<String> = r.log.iter().map(|l| serde_json::to_string(l).unwrap()).collect();
    let lines: Vec<Line> = text.iter().map(|s| serde_json::from_str(s).unwrap()).collect();
    let mut back = Machine::load(defs());
    assert_eq!(back.replay(&lines), Vec::<String>::new());
    assert!(back.tickets[&t].archived && back.view(t).unwrap().archived);
    // his post on it, live in this release: it is unarchived and his words go
    let out = r.owner_says(t, &["lead"], "one more: the mode too");
    assert!(out.reply.is_ok());
    assert!(!r.m.tickets[&t].archived);
    // routed as any post of his on a thread of two is (an_owners_post_routed_to_one_of_two_informs_the_other), not held
    // by the archive: the thread is unarchived before his words go
    r.route(json!({"head": "To", "agents": ["lead"]}));
    assert!(r.woken("lead", t), "{:?}", r.m.due("lead"));
    r.replays();
    // a line not by him: refused on replay as live
    let mut forged = lines.clone();
    if let Some(Line::Archive { by, .. }) = forged.iter_mut().rev().find(|l| matches!(l, Line::Archive { .. })) {
        *by = "lead".into();
    }
    let problems = Machine::load(defs()).replay(&forged);
    assert!(problems.iter().any(|p| p.contains("Only the owner archives a ticket")), "{problems:?}");
}
