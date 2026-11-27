//! A step whose answer is read as none of its outcomes twice (paper-research N-101 p757): it passes to the lead (an
//! agent with role `lead` that is not the one that answered), else to the owner, shown both answers whole with the
//! reading's reasons; the lead's own two pass it to the owner. A record from before it replays as recorded: with
//! the post to the lead (N-159.p16), or with nothing.

use super::*;

/// The first answer not read: a reminder. Then the second, ended; its reading is the caller's.
fn twice_unformed_by(r: &mut Run, c: u64, agent: &str, first: u64) {
    r.take(agent);
    r.go(Input::TurnEnded { agent: agent.into(), delivery: None, text: Run::loc(first), later: vec![], stood: None, go: None });
    let out = r.go(Input::Formed { t: c, step: 1, head: None, why: Some(format!("{agent}'s first is two outcomes.")), not_taken: None, call: None });
    assert!(out.lines.iter().any(|l| matches!(l, Line::Reminded { .. })));
    assert!(!out.lines.iter().any(|l| matches!(l, Line::Posted { .. } | Line::PassedUp { .. })), "{:?}", out.lines);
    r.take(agent);
    r.go(Input::TurnEnded { agent: agent.into(), delivery: None, text: Run::loc(first + 1), later: vec![], stood: None, go: None });
}

fn twice_unformed() -> (Run, u64) {
    let mut r = Run::new();
    r.go(Input::Role { agent: "paper-research".into(), role: "lead".into(), also: vec![] }).reply.unwrap();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    twice_unformed_by(&mut r, c, "orch", 2);
    (r, c)
}

fn second_none(r: &mut Run, c: u64, agent: &str) -> Out {
    let out = r.go(Input::Formed { t: c, step: 1, head: None, why: Some(format!("{agent}'s second gives no outcome.")), not_taken: None, call: None });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(!out.lines.iter().any(|l| matches!(l, Line::Posted { .. } | Line::Reminded { .. })), "no post, no third reminder: {:?}", out.lines);
    out
}

fn accepted(out: &Out) -> bool {
    out.effects.contains(&Effect::Ask { t: 201, step: 1, question: Question::Fills { role: "fixer".into(), call: "seek".into() } })
}

#[test]
fn a_second_answer_not_read_passes_the_step_to_the_lead_with_both_answers() {
    let (mut r, c) = twice_unformed();
    let out = second_none(&mut r, c, "orch");
    assert!(
        out.lines.iter().any(|l| matches!(l, Line::PassedUp { from, to, why, .. } if from == "orch" && to == "paper-research" && why.contains("orch answered twice") && why.contains("one of Accept") && why.ends_with("orch's second gives no outcome."))),
        "{:?}",
        out.lines
    );
    assert!(out.effects.contains(&Effect::Deliver { to: "paper-research".into() }), "{:?}", out.effects);
    let ViewStatus::Awaits { agent, step: 1, passed, .. } = r.status(c) else { panic!("{:?}", r.status(c)) };
    assert_eq!(agent, "paper-research");
    let shown: Vec<(String, Text, String)> = passed.into_iter().map(|n| (n.by, n.text, n.why)).collect();
    assert_eq!(
        shown,
        vec![
            ("orch".into(), Text::Loc { loc: Run::loc(2) }, "orch's first is two outcomes.".into()),
            ("orch".into(), Text::Loc { loc: Run::loc(3) }, "orch's second gives no outcome.".into()),
        ]
    );
    // the step the lead is given carries them
    let d = r.take("paper-research");
    assert!(matches!(&d.what, What::Step { step: 1, again: false, passed, .. } if passed.len() == 2), "{d:?}");
    // orch's later turn answers nothing
    let out = r.go(Input::TurnEnded { agent: "orch".into(), delivery: None, text: Run::loc(9), later: vec![], stood: None, go: None });
    assert!(!out.effects.iter().any(|e| matches!(e, Effect::Form { .. })), "{out:?}");
    // the lead's answer moves the ticket on
    r.go(Input::TurnEnded { agent: "paper-research".into(), delivery: None, text: Run::loc(4), later: vec![], stood: None, go: None });
    let out = r.go(Input::Formed { t: c, step: 1, head: Some(json!({"head": "Accept", "note": "see line 201"})), why: None, not_taken: None, call: None });
    assert!(accepted(&out), "{out:?}");
    r.replays();
}

#[test]
fn with_no_lead_it_passes_to_the_owner() {
    let mut r = Run::new();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    twice_unformed_by(&mut r, c, "orch", 2);
    let out = second_none(&mut r, c, "orch");
    assert!(out.lines.iter().any(|l| matches!(l, Line::PassedUp { from, to, .. } if from == "orch" && to == "owner")), "{:?}", out.lines);
    assert!(matches!(r.status(c), ViewStatus::Awaits { agent, passed, .. } if agent == "owner" && passed.len() == 2), "{:?}", r.status(c));
    // the owner's answer moves it on
    let out = r.go(Input::Answer { t: c, by: "owner".into(), head: json!({"head": "Accept", "note": "see line 201"}), text: Text::New("accept".into()) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(accepted(&out), "{out:?}");
    r.replays();
}

/// The agent that holds `lead` itself (here through a role line's `also`) and answered twice passes to the owner.
#[test]
fn the_lead_that_answered_passes_to_the_owner() {
    let mut r = Run::new();
    r.go(Input::Role { agent: "pr2".into(), role: "orchestrator".into(), also: vec!["lead".into()] }).reply.unwrap();
    let c = r.open_claim();
    r.decide(c, 1, "pr2");
    twice_unformed_by(&mut r, c, "pr2", 2);
    let out = second_none(&mut r, c, "pr2");
    assert!(out.lines.iter().any(|l| matches!(l, Line::PassedUp { from, to, .. } if from == "pr2" && to == "owner")), "{:?}", out.lines);
    r.replays();
}

/// The lead the policy gives (a role line's `also`: on Tovrin the lead is derived) is the lead the step passes to;
/// from docket-core2's efd358b (paper-research N-101).
#[test]
fn the_step_passes_to_a_lead_the_policy_gives() {
    let mut r = Run::new();
    r.go(Input::Role { agent: "pr2".into(), role: "orchestrator".into(), also: vec!["lead".into()] }).reply.unwrap();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    twice_unformed_by(&mut r, c, "orch", 2);
    let out = second_none(&mut r, c, "orch");
    assert!(out.lines.iter().any(|l| matches!(l, Line::PassedUp { from, to, .. } if from == "orch" && to == "pr2")), "{:?}", out.lines);
    assert!(matches!(r.status(c), ViewStatus::Awaits { agent, passed, .. } if agent == "pr2" && passed.len() == 2), "{:?}", r.status(c));
    r.replays();
}

#[test]
fn the_leads_two_answers_not_read_pass_it_to_the_owner_with_all_four() {
    let (mut r, c) = twice_unformed();
    second_none(&mut r, c, "orch");
    twice_unformed_by(&mut r, c, "paper-research", 4);
    let out = second_none(&mut r, c, "paper-research");
    assert!(out.lines.iter().any(|l| matches!(l, Line::PassedUp { from, to, .. } if from == "paper-research" && to == "owner")), "{:?}", out.lines);
    let ViewStatus::Awaits { agent, passed, .. } = r.status(c) else { panic!("{:?}", r.status(c)) };
    assert_eq!(agent, "owner");
    assert_eq!(passed.iter().map(|n| n.by.as_str()).collect::<Vec<_>>(), vec!["orch", "orch", "paper-research", "paper-research"]);
    let out = r.go(Input::Answer { t: c, by: "owner".into(), head: json!({"head": "Accept", "note": "see line 201"}), text: Text::New("accept".into()) });
    assert!(accepted(&out), "{out:?}");
    r.replays();
}

/// A record from before the pass-up: its second reading made the post to the lead, and the step stayed. It replays
/// so; one older still, with neither the post nor the pass-up, replays with nothing. Its `posted` line, from before
/// that line said whom its post names, replays as the post naming the lead does; written so, naming the lead, it
/// replays the same, and naming another is a problem line (proto-mind N-1446 p802).
#[test]
fn a_record_from_before_it_replays_as_recorded() {
    let (mut r, c) = twice_unformed();
    let n = r.m.view(c).unwrap().posts.last().map_or(1, |p| p.n + 1);
    let before = r.log.len();
    second_none(&mut r, c, "orch");
    let Line::Formed { at: when, .. } = r.log[before].clone() else { panic!("{:?}", r.log[before]) };
    let mut posted: Vec<Line> = r.log[..=before].to_vec();
    posted.push(Line::Posted { at: when, t: c, n, by: "C-200".into(), step: None, names: vec![], inform: vec![], into_turn: vec![] });
    let mut m2 = Machine::load(defs());
    assert_eq!(m2.replay(&posted), Vec::<String>::new());
    let v = m2.view(c).unwrap();
    let last = v.posts.last().unwrap();
    let Text::New(text) = &last.text else { panic!("{last:?}") };
    assert!(text.contains("orch answered twice") && text.contains("stays with orch"), "{text}");
    assert_eq!(last.names, vec!["paper-research".to_string()]);
    assert!(matches!(&v.status, ViewStatus::Awaits { agent, passed, .. } if agent == "orch" && passed.is_empty()), "{:?}", v.status);
    let named = |a: &str| -> Vec<Line> {
        let mut l = posted.clone();
        if let Some(Line::Posted { names, .. }) = l.last_mut() {
            *names = vec![a.to_string()];
        }
        l
    };
    assert_eq!(Machine::load(defs()).replay(&named("paper-research")), Vec::<String>::new());
    let problems = Machine::load(defs()).replay(&named("orch"));
    assert!(problems.len() == 1 && problems[0].starts_with(&format!("line {}: the machine now makes", before + 1)), "{problems:?}");
    let older: Vec<Line> = r.log[..=before].to_vec();
    let mut m3 = Machine::load(defs());
    assert_eq!(m3.replay(&older), Vec::<String>::new());
    assert!(m3.view(c).unwrap().posts.iter().all(|p| p.by != "C-200"));
    assert!(matches!(m3.view(c).unwrap().status, ViewStatus::Awaits { agent, .. } if agent == "orch"));
}

/// The reminder says why, in the forming call's words (paper-research N-101 p561); coordd's own reasons are not said.
#[test]
fn the_reminder_says_the_forming_calls_why() {
    let mut r = Run::new();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    r.take("orch");
    r.go(Input::TurnEnded { agent: "orch".into(), delivery: None, text: Run::loc(2), later: vec![], stood: None, go: None });
    let why = "It names no verdict, only that the claim was read.";
    r.go(Input::Formed { t: c, step: 1, head: None, why: Some(why.into()), not_taken: Some("not for the agent".into()), call: Some("tickets/forming/200.s1.1.json".into()) }).reply.unwrap();
    let due = r.m.due("orch");
    let again = due.iter().find(|d| d.id.ends_with(".s1.again")).expect("the reminder");
    assert!(matches!(&again.what, What::Step { again: true, why: Some(w), passed, .. } if w == why && passed.is_empty()), "{again:?}");
    assert!(r.log.iter().any(|l| matches!(l, Line::Formed { not_taken: Some(n), call: Some(_), .. } if n == "not for the agent")));
    // the view says the step's last reading read none, and why (the step card's chip, paper-research N-101 p638)
    assert!(matches!(&r.m.view(c).unwrap().status, crate::ViewStatus::Awaits { read_none: Some(w), passed, .. } if w == why && passed.is_empty()), "{:?}", r.m.view(c).unwrap().status);
}

/// The dry run's run 1 (paper-research N-101 p999, 2a; docket-try2 lines 72 to 78): dev's ask was answered later, so
/// its step was due again with the answer; dev's turn on the answer ended with an answer to the step. While that
/// answer is read nothing of the step goes to dev, and when it is not read that is its first: dev is reminded, the
/// step is not passed up.
#[test]
fn the_step_given_again_for_an_asks_answer_is_no_reminder_and_waits_for_the_reading() {
    let mut r = Run::new();
    r.dev_holds_the_seek();
    r.ask("dev", "which branch?").reply.unwrap();
    r.go(Input::AskLater { by: "dev".into() }).reply.unwrap();
    let q = r.question();
    r.decide(q, 1, "orch");
    r.take("orch");
    r.answer(q, "orch", json!({"head": "Answer", "text": "main"}));
    let due = r.m.due("dev");
    assert!(due.iter().any(|d| d.id.ends_with(".s1.again")), "{due:?}");
    // dev is given the answer and ends its turn on it, answering the step
    let answer = due.iter().find(|d| !d.id.ends_with(".s1.again")).expect("the ask's answer").clone();
    r.go(Input::Given { t: answer.t, to: "dev".into(), delivery: answer.id.clone(), upto: answer.upto, skipped: Default::default(), place: Some(Run::loc(1)) }).reply.unwrap();
    let out = r.go(Input::TurnEnded { agent: "dev".into(), delivery: Some(answer.id.clone()), text: Run::loc(5), later: vec![], stood: None, go: None });
    assert!(out.effects.iter().any(|e| matches!(e, Effect::Form { t: 201, .. })), "{out:?}");
    assert!(!r.m.due("dev").iter().any(|d| d.t == 201), "nothing of the step while it is read: {:?}", r.m.due("dev"));
    let out = r.go(Input::Formed { t: 201, step: 1, head: None, why: Some("It gives no outcome.".into()), not_taken: None, call: None });
    assert!(out.lines.iter().any(|l| matches!(l, Line::Reminded { t: 201, .. })), "{:?}", out.lines);
    assert!(!out.lines.iter().any(|l| matches!(l, Line::PassedUp { .. })), "{:?}", out.lines);
    let again = r.m.due("dev").into_iter().find(|d| d.id.ends_with(".s1.again")).expect("the reminder");
    assert!(matches!(&again.what, What::Step { again: true, why: Some(w), .. } if w == "It gives no outcome."), "{again:?}");
    r.replays();
}
