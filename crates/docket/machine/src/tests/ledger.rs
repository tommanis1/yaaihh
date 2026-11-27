//! The commitment ledger (I-1455 (B); proto-mind N-1479 p85, its rule paper-research's p94, p98 and p105 on
//! consult-3's p84 and proto-reader's p88): `Machine::ledger`, `Machine::asked`, and from them `TicketView::asked` and
//! `unanswered_owner`. Every record here is made up.

use super::*;

/// His thread to TO, his words TEXT its post 1, as his window opens one: its number.
fn his(r: &mut Run, to: &[&str], text: &str) -> u64 {
    let out = r.go(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null}), text: Some(Text::New(text.into())), names: to.iter().map(|a| a.to_string()).collect(), told: vec![] });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    *r.m.tickets.range(..HIDDEN).next_back().unwrap().0
}

/// His post on T to TO, routed to them when the thread's handler asks: its number.
fn his_post(r: &mut Run, t: u64, to: &[&str], text: &str) -> u32 {
    r.owner_says(t, to, text);
    if !to.iter().all(|a| r.woken(a, t)) {
        r.route(json!({"head": "To", "agents": to}));
    }
    r.m.tickets[&t].posts.iter().rev().find(|p| p.by == "owner").unwrap().n
}

/// BY's post on T typing NAMES (none: the rule names whom it answers), the reply tool's answer to post ANSWERS, typed
/// KIND: its number.
fn says(r: &mut Run, t: u64, by: &str, names: &[&str], answers: Option<u32>, kind: Option<PostKind>, text: &str) -> u32 {
    let out = r.go(Input::Post { t, by: by.into(), text: Text::New(text.into()), names: names.iter().map(|a| a.to_string()).collect(), told: vec![], inform: vec![], answers, kind });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    r.m.tickets[&t].last()
}

/// A start reads the same ledger for each of AGENTS: the record replayed into a new machine.
fn a_start_reads_the_same(r: &Run, agents: &[&str]) {
    let mut again = Machine::load(defs());
    assert_eq!(again.replay(&r.log), Vec::<String>::new());
    for a in agents {
        assert_eq!(again.ledger(a), r.m.ledger(a), "{a}'s ledger after a start");
    }
    r.replays();
}

/// His post to w2 and w3 (p85 points 1 and 2): w2's post naming nobody, which the rule names him on, answers it for
/// w2. w3's ask stays open in the ledger, but p1 leaves `unanswered_owner`: answered by one it asks, it is answered
/// for all (`Machine::his_answered`; paper-research N-1446 p2355, which turns this leg's "w3 still owes p1" round).
/// His p2 to w2 came after w2 was handed p1 and before it wrote: w2's post answers p1, not p2, though it names him.
/// w3's reply-tool answer closes p1 for w3, and not his p2, which w3 was handed and not asked: the reply tool's `to`
/// names p1. Each ledger holds each ask with its answer, the owner's none, and a start reads the same.
#[test]
fn his_post_to_two_is_answered_for_both_once_one_has_and_an_answer_is_to_what_was_handed() {
    let mut r = Run::new();
    let t = his(&mut r, &["w2", "w3"], "is the build green?");
    let id = r.m.id(t);
    let p = |n: u32| Some(format!("{id}.p{n}"));
    assert_eq!(r.m.asked(t, 1), vec![("w2".to_string(), None), ("w3".to_string(), None)]);
    r.give("w2", t);
    let more = his_post(&mut r, t, &["w2"], "and the stills?");
    let green = says(&mut r, t, "w2", &[], None, None, "green");
    assert_eq!(r.m.tickets[&t].posts.last().unwrap().answering, ["owner"], "the rule named him");
    assert_eq!(r.m.asked(t, 1), vec![("w2".to_string(), p(green)), ("w3".to_string(), None)]);
    assert_eq!(r.m.asked(t, more), vec![("w2".to_string(), None)], "p{more} came after w2 was handed p1, before it wrote p{green}");
    let v = r.m.view(t).unwrap();
    assert_eq!(v.unanswered_owner, vec![more], "w2 answered p1, so it is answered for w3 too (p2355); p{more} is w2's alone and open");
    assert_eq!(
        v.asked,
        vec![ViewAsked { post: 1, agent: "w2".into(), answer: p(green) }, ViewAsked { post: 1, agent: "w3".into(), answer: None }, ViewAsked { post: more, agent: "w2".into(), answer: None }]
    );
    r.give("w3", t);
    let yes = says(&mut r, t, "w3", &["owner"], Some(1), None, "yes, green");
    assert_eq!(r.m.view(t).unwrap().unanswered_owner, vec![more], "w3's answer is to p1 alone");
    let at = r.m.tickets[&t].posts[0].at.clone();
    assert_eq!(r.m.ledger("w3"), vec![Commitment { post: format!("{id}.p1"), by: "owner".into(), at, why: "owner".into(), head: None, answer: p(yes) }]);
    assert_eq!(r.m.ledger("w2").into_iter().map(|o| (o.post, o.answer)).collect::<Vec<_>>(), vec![(format!("{id}.p1"), p(green)), (format!("{id}.p{more}"), None)]);
    assert!(r.m.ledger("owner").is_empty(), "what agents ask him is their posts' asks");
    a_start_reads_the_same(&r, &["w2", "w3"]);
}

/// His post told to one agent and handed to another on the thread, which answers it (docket-edge8 N-1446 p2353, 8:
/// N-1104.p315, told to consult-lead and answered by here's p316 and paper-research's p317, stood in his "Asked, no
/// answer yet"; paper-research p2355): answered for all, so it leaves `unanswered_owner`, the Threads page's list, by
/// the rule the foot of each delivery reads (`Machine::his_answered`, `owed_posts`). The watcher routes his words for
/// the lead to the fixer, so they ask the fixer alone and reach the lead as one on the thread. A post by w3, neither
/// asked nor handed them, answers nothing; the lead's does. The fixer's own ask stays open in its ledger.
#[test]
fn his_post_told_to_one_and_answered_by_another_it_was_handed_is_no_longer_asked_with_no_answer() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.owner_says(t, &["lead"], "go ahead");
    let n = r.m.tickets[&t].last();
    r.route(json!({"head": "To", "agents": ["fixer"]}));
    assert_eq!(r.m.asked(t, n), vec![("fixer".to_string(), None)], "told to the fixer alone");
    assert_eq!(r.m.view(t).unwrap().unanswered_owner, vec![n]);
    says(&mut r, t, "w3", &["owner"], None, None, "green here");
    assert!(!r.m.his_answered(t, n), "w3 was neither asked nor handed p{n}");
    assert_eq!(r.m.view(t).unwrap().unanswered_owner, vec![n]);
    r.give("lead", t);
    let ok = says(&mut r, t, "lead", &["owner"], None, None, "done, green");
    assert!(r.m.his_answered(t, n), "the lead was handed p{n} and answered it in p{ok}");
    assert!(r.m.view(t).unwrap().unanswered_owner.is_empty(), "answered for all: not asked with no answer");
    assert_eq!(r.m.asked(t, n), vec![("fixer".to_string(), None)], "the fixer's own ask stays as the ledger holds it");
    assert_eq!(r.m.ledger("fixer").into_iter().filter(|o| o.post == format!("{}.p{n}", r.m.id(t))).map(|o| o.answer).collect::<Vec<_>>(), vec![None]);
    a_start_reads_the_same(&r, &["fixer", "lead"]);
}

/// A result answers only what its writer was handed (p94's point 2 in each way; consult-3's N-1479 p127, proto-reader's
/// p128): w2, handed his p1, writes a `result` after his p2 came, which it was not handed. The result answers p1, and p2
/// stays open for w2 and in `unanswered_owner`. Nothing on the thread is due to w2 then (`read_own` takes his p2, not
/// sent to wait, as read by w2's post). The second half pins how the ledger reads a giving's range, not what w2 saw:
/// his p4's delivery shows w2 p4 alone (`unread`: p2 is behind its mark), yet its GivenInto, up to p4, hands p2 by
/// `Ticket::hands` (n <= upto, not skipped), so w2's next result closes both. That p2 is the post `read_own` passed
/// before any round raised it, the gap docket-core2's head on lead-64 closes (N-1479 p152 to p156, proto-lead p187):
/// from the era it begins (`ERA_HIS_HANDED` at 9e89f36c, begun by coordd), w2's mark stays below p2 until p2 is
/// handed. `Run` writes no era line, so this test runs in era 0 and reads as here with that head too (proto-reader p221).
#[test]
fn a_result_answers_only_what_its_writer_was_handed() {
    let mut r = Run::new();
    let t = his(&mut r, &["w2"], "is the build green?");
    let id = r.m.id(t);
    let p = |n: u32| Some(format!("{id}.p{n}"));
    r.give("w2", t);
    let more = his_post(&mut r, t, &["w2"], "and the stills?");
    let green = says(&mut r, t, "w2", &[], None, Some(PostKind::Result), "green on 1a2b");
    assert_eq!(r.m.asked(t, 1), vec![("w2".to_string(), p(green))]);
    assert_eq!(r.m.asked(t, more), vec![("w2".to_string(), None)], "w2 was not handed p{more} when it wrote p{green}");
    assert_eq!(r.m.view(t).unwrap().unanswered_owner, vec![more]);
    assert!(r.m.due("w2").iter().all(|d| d.t != t), "p{more} reads as read");
    let log = his_post(&mut r, t, &["w2"], "and the log?");
    let d = r.m.due("w2").into_iter().find(|d| d.t == t).unwrap();
    let What::Posts { unread } = &d.what else { panic!("{d:?}") };
    assert_eq!(unread.iter().map(|p| p.n).collect::<Vec<u32>>(), vec![log], "the delivery shows w2 p{log} alone: p{more} is behind its mark");
    r.go(Input::GivenInto { t, to: "w2".into(), delivery: d.id, upto: d.upto, skipped: Default::default(), place: None }).reply.unwrap();
    let up = says(&mut r, t, "w2", &[], None, Some(PostKind::Result), "the stills and the log are up");
    assert_eq!((r.m.asked(t, more), r.m.asked(t, log)), (vec![("w2".to_string(), p(up))], vec![("w2".to_string(), p(up))]), "p{log}'s giving, up to p{log}, hands p{more} (`hands`)");
    assert!(r.m.view(t).unwrap().unanswered_owner.is_empty());
    a_start_reads_the_same(&r, &["w2"]);
}

/// Which posts answer (the scan before's filter; consult-3's N-1479 p136 point 3, proto-lead p142): on a slowed thread,
/// w2's "@owner green, and @w3 …" wakes w3 too, so it is halted for his approval, and it answers his p1 only once he
/// approves it. w2's answer that he moves to another thread answers nothing where it was written.
#[test]
fn an_answer_halted_for_him_or_moved_away_answers_nothing() {
    let mut r = Run::new();
    let t = his(&mut r, &["w2"], "is the build green?");
    let id = r.m.id(t);
    r.go(Input::Slow { t, on: true, by: "owner".into() }).reply.unwrap();
    r.give("w2", t);
    let green = says(&mut r, t, "w2", &["owner", "w3"], None, None, "@owner green, and @w3 the log is yours");
    assert!(r.m.tickets[&t].posts.iter().any(|p| p.n == green && p.halted), "halted for him");
    assert_eq!(r.m.asked(t, 1), vec![("w2".to_string(), None)], "p{green} waits for his approval");
    let card = r.m.tickets[&t].halts.last().unwrap().n;
    r.go(Input::Approve { t, n: card, by: "owner".into() }).reply.unwrap();
    assert_eq!(r.m.asked(t, 1), vec![("w2".to_string(), Some(format!("{id}.p{green}")))], "approved");
    let u = his(&mut r, &["w2"], "and the stills?");
    let elsewhere = his(&mut r, &["w2"], "the release notes");
    r.give("w2", u);
    let up = says(&mut r, u, "w2", &["owner"], None, None, "@owner the stills are up");
    assert_eq!(r.m.asked(u, 1), vec![("w2".to_string(), Some(format!("{}.p{up}", r.m.id(u))))]);
    r.go(Input::Move { t: u, n: up, to: Some(elsewhere), by: "owner".into() }).reply.unwrap();
    assert_eq!(r.m.asked(u, 1), vec![("w2".to_string(), None)], "p{up} moved to {}", r.m.id(elsewhere));
    a_start_reads_the_same(&r, &["w2", "w3"]);
}

/// His ask passed on (consult-3's N-1479 p84 case, proto-reader's N-9, paper-research p94): paper-research, handed
/// his p1, types only `@accounts-2` (p2). That answers him nothing: p1 stays open in `asked`, in `unanswered_owner`
/// and in paper-research's ledger, and accounts-2 owes p2. Its next word to him is its turn's last message: the rule
/// Reply takes what the turn held as answered by the ledger's rule too (`reply_names_of`, paper-research p136), so the
/// Reply goes on his thread, names him, and closes p1. The rule before took p2, written with p1 handed, as having
/// answered what the turn held, and the turn's end posted nowhere, leaving p1 open until a typed `@owner` (p98).
#[test]
fn his_ask_passed_on_stays_open_until_the_agents_next_word_to_him() {
    let mut r = Run::new();
    let t = his(&mut r, &["paper-research"], "can we keep the d h m breakdown on the accounts tab everywhere?");
    let id = r.m.id(t);
    let d = r.give("paper-research", t);
    let pass = says(&mut r, t, "paper-research", &["accounts-2"], None, None, "@accounts-2, please make every duration d h m");
    let open = |r: &Run| (r.m.asked(t, 1), r.m.view(t).unwrap().unanswered_owner, r.m.ledger("paper-research").into_iter().map(|o| (o.post, o.answer)).collect::<Vec<_>>());
    let still = (vec![("paper-research".to_string(), None)], vec![1], vec![(format!("{id}.p1"), None)]);
    assert_eq!(open(&r), still, "p{pass} passes it on");
    assert_eq!(r.m.ledger("accounts-2").into_iter().map(|o| (o.post, o.why)).collect::<Vec<_>>(), vec![(format!("{id}.p{pass}"), "named".to_string())]);
    // the turn ends: "Yes: every duration on the tab will be d h m"
    let out = r.turn("paper-research", Some(&d), 3);
    let replied = out.lines.iter().find_map(|l| match l {
        Line::Replied { t: u, n, names, .. } if *u == t => Some((*n, names.clone())),
        _ => None,
    });
    let yes = pass + 1;
    assert_eq!(replied, Some((yes, vec!["owner".to_string()])), "its turn's end is its answer to him: {:?}", out.lines);
    assert_eq!(open(&r), (vec![("paper-research".to_string(), Some(format!("{id}.p{yes}")))], vec![], vec![(format!("{id}.p1"), Some(format!("{id}.p{yes}")))]));
    a_start_reads_the_same(&r, &["paper-research", "accounts-2"]);
}

/// A reply-tool answer to another post (proto-reader's N-1479 p88 widening, paper-research p94): w2, handed his p1
/// and consult-3's p2, answers p2 by the tool, typing consult-3 only, and his p1 stays open. One naming him too
/// answers it.
#[test]
fn a_reply_tool_answer_to_another_post_leaves_his_open_unless_it_names_him() {
    let mut r = Run::new();
    let t = his(&mut r, &["w2"], "the stills?");
    let id = r.m.id(t);
    let red = says(&mut r, t, "consult-3", &["w2"], None, None, "@w2 and the red?");
    r.give("w2", t);
    let to_c3 = says(&mut r, t, "w2", &["consult-3"], Some(red), None, "the red is af04d0ca");
    assert_eq!(r.m.asked(t, 1), vec![("w2".to_string(), None)], "p{to_c3} answers p{red} only");
    assert_eq!(r.m.asked(t, red), vec![("w2".to_string(), Some(format!("{id}.p{to_c3}")))]);
    let both = says(&mut r, t, "w2", &["consult-3", "owner"], Some(red), None, "and @owner: the stills are up");
    assert_eq!(r.m.asked(t, 1), vec![("w2".to_string(), Some(format!("{id}.p{both}")))]);
    a_start_reads_the_same(&r, &["w2"]);
}

/// Agents' asks (p98): w3, handed w2's "@w3 build 1a2b", answers it with a typed "@w2 done, see 1a2b". lead's typed
/// read of 1a2b to w3 is owed with its head until w3's result answers it; lead's fyi asks nothing, though it names
/// w3. The thread w2 opened is at rest (closed Between), so each post names whom it is for.
#[test]
fn an_agents_ask_is_answered_by_a_typed_name_and_a_read_by_a_result() {
    let mut r = Run::new();
    let t = r.note("w2", "w3", "@w3 build 1a2b");
    let id = r.m.id(t);
    let read = says(&mut r, t, "lead", &["w3"], None, Some(PostKind::Read { to: vec!["w3".into()], head: "1a2b".into() }), "read 1a2b");
    let fyi = says(&mut r, t, "lead", &["w3"], None, Some(PostKind::Fyi), "fyi @w3: the tower is free");
    let owed = |r: &Run| r.m.ledger("w3").into_iter().map(|o| (o.post, o.why, o.head, o.answer)).collect::<Vec<_>>();
    let (build, reading) = ((format!("{id}.p1"), "named".to_string(), None), (format!("{id}.p{read}"), "read".to_string(), Some("1a2b".to_string())));
    assert_eq!(owed(&r), vec![(build.0.clone(), build.1.clone(), build.2.clone(), None), (reading.0.clone(), reading.1.clone(), reading.2.clone(), None)], "p{fyi} asks nothing");
    r.give("w3", t);
    let done = says(&mut r, t, "w3", &["w2"], None, None, "@w2 done, see 1a2b");
    assert_eq!(owed(&r)[1].3, None, "p{done} names w2, not lead");
    let result = says(&mut r, t, "w3", &["w2"], None, Some(PostKind::Result), "@w2 1a2b is read right");
    assert_eq!(owed(&r), vec![(build.0, build.1, build.2, Some(format!("{id}.p{done}"))), (reading.0, reading.1, reading.2, Some(format!("{id}.p{result}")))]);
    a_start_reads_the_same(&r, &["w3"]);
}

/// His `@all` (`all_of`): owed by the agent it is handed to, here the one the policy's `claims` gives it to, until
/// that agent's claim; the agent only informed of it owes nothing; passed on, the agent it is passed to, and the post
/// that passes it, though it names that agent, asks it nothing of its own, so the claim leaves it owing nothing.
#[test]
fn his_all_is_owed_by_its_holder_until_its_claim() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    let id = r.m.id(t);
    let (c, _) = r.owner_posts(t, &[], "@all is the runner back?");
    let h = r.handler_of(t, c);
    let Status::Asking { owed, .. } = &r.m.tickets[&h].status else { panic!("{:?}", r.m.tickets[&h].status) };
    let step = owed.step;
    r.go(Input::Decided { t: h, step, question: "decide".into(), answer: json!("lead"), provenance: json!({"decision": "claims"}) }).reply.unwrap();
    assert_eq!(r.m.asked(t, c), vec![("lead".to_string(), None)], "fixer is only informed");
    assert!(r.m.ledger("fixer").iter().all(|o| o.post != format!("{id}.p{c}")));
    r.give("lead", t);
    let claim = says(&mut r, t, "lead", &["owner"], Some(c), None, "yes, since 12:00");
    assert_eq!(r.m.asked(t, c), vec![("lead".to_string(), Some(format!("{id}.p{claim}")))]);
    let o = r.m.ledger("lead").into_iter().find(|o| o.post == format!("{id}.p{c}")).unwrap();
    assert_eq!((o.why.as_str(), o.answer), ("all", Some(format!("{id}.p{claim}"))));
    // passed on (`Input::Pass`, `all_of`'s `passed`): the agent it is passed to owes it, though his post did not wake
    // it; the one that passed it owes nothing; its claim answers it
    let (d, _) = r.owner_posts(t, &[], "@all is the disk full?");
    let h = r.handler_of(t, d);
    let Status::Asking { owed, .. } = &r.m.tickets[&h].status else { panic!("{:?}", r.m.tickets[&h].status) };
    let step = owed.step;
    r.go(Input::Decided { t: h, step, question: "decide".into(), answer: json!("lead"), provenance: json!({"decision": "claims"}) }).reply.unwrap();
    r.give("lead", t);
    r.go(Input::Pass { t, by: "lead".into(), n: d, to: "fixer".into(), text: "the disk is yours".into() }).reply.unwrap();
    assert_eq!(r.m.asked(t, d), vec![("fixer".to_string(), None)], "lead passed p{d} on to fixer");
    assert!(r.m.ledger("lead").iter().all(|o| o.post != format!("{id}.p{d}")));
    // the pass names fixer, and asks it nothing of its own: his p{d} is the ask (N-1104.p1014, paper-research N-1446
    // p2973)
    let pass = r.m.tickets[&t].last();
    assert_eq!(r.m.tickets[&t].posts.iter().find(|p| p.n == pass).map(|p| (p.names.clone(), p.passes)), Some((vec!["fixer".to_string()], Some(d))));
    assert!(r.m.asked(t, pass).is_empty(), "the pass asks nobody: {:?}", r.m.asked(t, pass));
    assert!(r.m.ledger("fixer").iter().all(|o| o.post != format!("{id}.p{pass}")), "{:?}", r.m.ledger("fixer"));
    r.give("fixer", t);
    let took = says(&mut r, t, "fixer", &["owner"], Some(d), None, "it is, at 92%");
    assert_eq!(r.m.asked(t, d), vec![("fixer".to_string(), Some(format!("{id}.p{took}")))]);
    let o = r.m.ledger("fixer").into_iter().find(|o| o.post == format!("{id}.p{d}")).unwrap();
    assert_eq!((o.why.as_str(), o.answer), ("all", Some(format!("{id}.p{took}"))));
    assert!(r.m.ledger("fixer").iter().all(|o| o.answer.is_some()), "its claim leaves it owing nothing: {:?}", r.m.ledger("fixer"));
    a_start_reads_the_same(&r, &["lead", "fixer"]);
}

/// The ledger in the record's graph (paper-research N-1446 p1446): his p1 asks w2 and w3 (`asks`), stays open past
/// w2's answer and closes at w3's (`closed_by`, at that line); his p2 to w2 closes by w2's reply-tool answer from
/// another thread (`answers_also`), at that line. `ask_times` reads whom each asks and its close, and before w3's
/// answer p1 has none.
#[test]
fn his_asks_and_their_close_are_edges_of_the_graph_at_the_lines_the_ledger_says_so() {
    use crate::graph::EdgeKind;
    let mut r = Run::new();
    let t = his(&mut r, &["w2", "w3"], "is the build green?");
    let id = r.m.id(t);
    let p = |n: u32| format!("{id}.p{n}");
    r.give("w2", t);
    let more = his_post(&mut r, t, &["w2"], "and the stills?");
    let green = says(&mut r, t, "w2", &[], None, None, "green");
    r.give("w3", t);
    let yes = says(&mut r, t, "w3", &["owner"], Some(1), None, "yes, green");
    let o = his(&mut r, &["w2"], "the other thread");
    r.give("w2", o);
    let done = says(&mut r, o, "w2", &["owner"], Some(1), None, "done here, and the stills are up");
    let there = format!("{}.p{done}", r.m.id(o));
    r.go(Input::AnswersAlso { t: o, n: done, also: vec![p(more)] }).reply.unwrap();
    assert_eq!(r.m.asked(t, more), vec![("w2".to_string(), Some(there.clone()))]);
    let (g, problems) = crate::graph::of(&mut Machine::load(defs()), &r.log);
    assert!(problems.is_empty(), "{problems:?}");
    let line_of = |post: &str| g.get(post).and_then(|n| n.line()).unwrap();
    let also = r.log.iter().position(|l| matches!(l, Line::AnswersAlso { .. })).unwrap() + 1;
    let edges = |k: EdgeKind| g.edges.iter().filter(|e| e.kind == k && (e.from == p(1) || e.from == p(more))).map(|e| (e.from.clone(), e.to.clone(), e.line().unwrap())).collect::<Vec<_>>();
    let asks = edges(EdgeKind::Asks);
    assert_eq!(asks.iter().map(|(f, t, _)| (f.clone(), t.clone())).collect::<Vec<_>>(), vec![(p(1), "agent:w2".into()), (p(1), "agent:w3".into()), (p(more), "agent:w2".into())]);
    assert!(asks.iter().all(|(_, _, l)| *l < line_of(&p(green))), "each asked before w2 answered: {asks:?}");
    assert_eq!(edges(EdgeKind::ClosedBy), vec![(p(1), p(yes), line_of(&p(yes))), (p(more), there.clone(), also)], "p1 closed at w3's answer, not w2's; p{more} at the reply tool's line");
    let times = g.ask_times();
    let of = |post: &str| times.iter().find(|x| x.post == post).cloned().unwrap();
    assert_eq!(of(&p(1)).asks, vec!["agent:w2".to_string(), "agent:w3".to_string()]);
    assert_eq!(of(&p(1)).closed.map(|c| (c.id, c.via)), Some(("agent:w3".to_string(), Some(p(yes)))));
    assert_eq!(of(&p(more)).closed.map(|c| (c.id, c.via)), Some(("agent:w2".to_string(), Some(there))));
    let before = g.get(&p(green)).and_then(|n| n.when.clone()).unwrap();
    assert_eq!(g.ask_times_at(Some(&before)).into_iter().find(|x| x.post == p(1)).unwrap().closed, None, "open until w3's answer");
}

/// A head kind as the dev program's hands one in (proto-mind's dev.kind `head`, cut down): a commit, and the ask or
/// item it is `for`.
const HEAD_KINDS: [(&str, &str); 1] = [(
    "head.kind",
    "kind head\n  letter H\n  form commit: Text, for: Text\n  result Text\n  pattern\n    r <- step dev \"Read {{ commit }}\" -> Right(why: Text) | Wrong(why: Text);\n    match r { Right(why) => return why, Wrong(why) => return why }\n",
)];

/// BY's hand-in for FOR (`coord open head --for FOR`), with no words: its id.
fn hand_in(r: &mut Run, by: &str, for_: &str) -> String {
    let out = r.go(Input::Open { kind: "head".into(), under: None, opener: by.into(), form: json!({"commit": "1a2b3c4d", "for": for_}), text: None, names: vec![], told: vec![] });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let t = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    r.m.id(t)
}

/// A hand-in answers its ask (lack 8 of the dev program, docket-edge5 N-1446 p1494 point 8, paper-research p1495 and
/// p1591): a ticket the asked agent opens whose form's `for` names his post answers it at its open, for that agent
/// alone in the ledger, and for all in `unanswered_owner` (paper-research N-1446 p2355); one `for` an item answers the post the item's note names. A hand-in by an agent not asked, or `for` a post
/// not there at its open, a thread, or an item with no note, answers nothing. One `for` his goal's item answers the
/// post its `ask` names (research's goals.kind; p2483). A start reads the same.
#[test]
fn a_hand_in_answers_the_ask_its_for_names_at_its_open() {
    let mut r = Run::with(&HEAD_KINDS);
    let t = his(&mut r, &["w2", "w3"], "the head for the stills?");
    let id = r.m.id(t);
    let p = |n: u32| format!("{id}.p{n}");
    hand_in(&mut r, "lead", &p(1));
    hand_in(&mut r, "w2", &id);
    assert_eq!(r.m.asked(t, 1), vec![("w2".to_string(), None), ("w3".to_string(), None)], "lead is not asked; {id} is no item");
    let w2 = hand_in(&mut r, "w2", &p(1));
    assert_eq!(r.m.asked(t, 1), vec![("w2".to_string(), Some(w2.clone())), ("w3".to_string(), None)], "w3's own ask stays open");
    assert!(r.m.view(t).unwrap().unanswered_owner.is_empty(), "w2's hand-in answers p1 for all (`his_answered`, paper-research N-1446 p2355)");
    assert_eq!(r.m.ledger("w2").into_iter().map(|o| (o.post, o.answer)).collect::<Vec<_>>(), vec![(p(1), Some(w2.clone()))]);
    // w3's hand-in for his next post, before it is there; then his p2 to w3, an item whose note it is, and one with none
    r.give("w3", t);
    let next = r.m.tickets[&t].last() + 1;
    let early = hand_in(&mut r, "w3", &p(next));
    let more = his_post(&mut r, t, &["w3"], "and the log?");
    assert_eq!(more, next);
    assert_eq!(r.m.asked(t, more), vec![("w3".to_string(), None)], "p{more} was not there at {early}'s open");
    let item = |r: &mut Run, form: Value| {
        r.go(Input::Open { kind: "item".into(), under: None, opener: "lead".into(), form, text: None, names: vec![], told: vec![] }).reply.unwrap();
        r.m.id(*r.m.tickets.range(..HIDDEN).next_back().unwrap().0)
    };
    let noted = item(&mut r, json!({"title": "the log", "note": p(more)}));
    let bare = item(&mut r, json!({"title": "the log, again"}));
    hand_in(&mut r, "w3", &bare);
    assert_eq!(r.m.asked(t, more), vec![("w3".to_string(), None)], "{bare} has no note");
    let w3 = hand_in(&mut r, "w3", &noted);
    assert_eq!(r.m.asked(t, more), vec![("w3".to_string(), Some(w3.clone()))], "{noted}'s note is p{more}");
    assert_eq!(r.m.asked(t, 1)[1], ("w3".to_string(), None), "a hand-in for {noted} is for p{more} alone");
    let w3_p1 = hand_in(&mut r, "w3", &p(1));
    assert_eq!(r.m.asked(t, 1), vec![("w2".to_string(), Some(w2)), ("w3".to_string(), Some(w3_p1))]);
    assert!(r.m.view(t).unwrap().unanswered_owner.is_empty());
    // his next post to w3 and its goal, an item with the post as its `ask` and no note, as research's binding opens it
    let stills = his_post(&mut r, t, &["w3"], "and the stills?");
    let goal = item(&mut r, json!({"title": "the stills", "note": null, "ask": p(stills)}));
    let w3_goal = hand_in(&mut r, "w3", &goal);
    assert_eq!(r.m.asked(t, stills), vec![("w3".to_string(), Some(w3_goal))], "{goal}'s ask is p{stills}");
    let mut again = Machine::load(defs_with(&HEAD_KINDS).0);
    assert_eq!(again.replay(&r.log), Vec::<String>::new());
    for a in ["w2", "w3"] {
        assert_eq!(again.ledger(a), r.m.ledger(a), "{a}'s ledger after a start");
    }
    r.replays_with(&HEAD_KINDS);
}

/// The hand-in in the record's graph: his p1 to w2 closes at w2's hand-in's open (`closed_by`, at that line, to the
/// hand-in's ticket), which w2 `wrote`; `ask_times` reads the close as w2's, via the hand-in.
#[test]
fn a_hand_in_closes_his_ask_in_the_graph_at_its_open() {
    use crate::graph::{EdgeKind, NodeKind};
    let mut r = Run::with(&HEAD_KINDS);
    let t = his(&mut r, &["w2"], "the head for the stills?");
    let ask = format!("{}.p1", r.m.id(t));
    let head = hand_in(&mut r, "w2", &ask);
    let (g, problems) = crate::graph::of(&mut Machine::load(defs_with(&HEAD_KINDS).0), &r.log);
    assert!(problems.is_empty(), "{problems:?}");
    let open = r.log.iter().position(|l| matches!(l, Line::Open { kind, .. } if kind == "head")).unwrap() + 1;
    let edges = |k: EdgeKind| g.edges.iter().filter(|e| e.kind == k && (e.from == ask || e.to == head)).map(|e| (e.from.clone(), e.to.clone(), e.line().unwrap())).collect::<Vec<_>>();
    assert_eq!(edges(EdgeKind::ClosedBy), vec![(ask.clone(), head.clone(), open)]);
    assert_eq!(g.get(&head).map(|n| n.kind), Some(NodeKind::Ticket));
    assert_eq!(edges(EdgeKind::Wrote), vec![("agent:w2".to_string(), head.clone(), open)]);
    let times = g.ask_times().into_iter().find(|x| x.post == ask).unwrap();
    assert_eq!(times.closed.map(|c| (c.id, c.via)), Some(("agent:w2".to_string(), Some(head))));
}

/// A head that stands at named steps (the dev program's `head`, cut down further): the cutter's `merge` of its commit,
/// then `dev`'s hold on it at the cut, `cut_of(commit, merge)`, as dev.kind's (proto-mind N-1446 p1493).
const STAND_KINDS: [(&str, &str); 1] = [(
    "head.kind",
    "kind head\n  letter H\n  form commit: Ref, for: Text\n  result Text\n  pattern\n    m <- step role cutter merge(commit);\n    match m {\n      Merged(onto) =>\n        k <- hold dev cut_of(commit, onto);\n        match k { In(release) => return release }\n    }\n",
)];
const STAND_CALLS: &str = "call merge(commit: Ref) -> Merged(merge: Ref)\n  template\n    Merge {{ commit }} onto the train's tip.\n\ncall cut_of(commit: Ref, merge: Ref) -> In(release: Text)\n  template\n    {{ commit }} stands on the train as {{ merge }}.\n";

/// agent-base's kinds and calls, with `STAND_KINDS` and its calls.
fn stand_defs() -> Defs {
    stand_defs_with(&STAND_KINDS)
}

/// agent-base's kinds and calls, with KINDS and `STAND_CALLS`.
fn stand_defs_with(kinds: &[(&str, &str)]) -> Defs {
    let (d, errors) = stand_build(kinds);
    assert!(errors.is_empty(), "{errors:?}");
    d
}

/// `stand_defs_with`, and what it refused.
fn stand_build(kinds: &[(&str, &str)]) -> (Defs, Vec<String>) {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = CALLS.iter().copied().chain([("stand.call", STAND_CALLS)]).flat_map(|(f, t)| syntax::parse_calls(f, t, &mut named).unwrap()).map(|c| (c.name.clone(), c)).collect();
    let files: Vec<(String, String)> = KINDS.iter().chain(kinds.iter()).map(|(f, t)| (f.to_string(), t.to_string())).collect();
    Defs::build(&files, calls, named, 200)
}

/// Where the open tickets stand, the facts a `when` decision is asked in (`Machine::standing`; the dev program's lack
/// 10, proto-mind N-1446 p1502, p1505, on lack 8's hand-in): w2's head for his p1 stands at the cutter's `merge`
/// while the policy fills the role and once it is given, then at `dev`'s hold `cut_of` with the merge, each with its
/// string arguments, and hands in for his p1, by him. w2's head for w3's post hands in for it, by w3; a head for no
/// post stands and hands in for nothing. Once closed, a head stands nowhere. A start reads the same.
#[test]
fn where_the_open_tickets_stand_is_stated_for_a_when_decision() {
    let mut r = Run { m: Machine::load(stand_defs()), log: Vec::new(), clock: 0 };
    let t = his(&mut r, &["w2"], "the head for the stills?");
    let (id, his_p1) = (r.m.id(t), format!("{}.p1", r.m.id(t)));
    let open = |r: &mut Run, by: &str, commit: &str, for_: &str| {
        let out = r.go(Input::Open { kind: "head".into(), under: None, opener: by.into(), form: json!({"commit": commit, "for": for_}), text: None, names: vec![], told: vec![] });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        *r.m.tickets.range(..HIDDEN).next_back().unwrap().0
    };
    let fact = |f: &str, args: &[&str]| (f.to_string(), args.iter().map(|a| a.to_string()).collect::<Vec<String>>());
    let of = |r: &Run, h: &str| r.m.standing().into_iter().filter(|(_, args)| args[0] == h).collect::<Vec<_>>();
    let a = open(&mut r, "w2", "aaaa1111", &his_p1);
    let ha = r.m.id(a);
    assert!(matches!(r.m.tickets[&a].status, Status::Asking { .. }), "the cutter's role is being filled");
    let merging = vec![fact("stands", &[&ha, "merge"]), fact("stands-with", &[&ha, "commit", "aaaa1111"]), fact("hands-in", &[&ha, &his_p1, "owner"])];
    assert_eq!(of(&r, &ha), merging);
    r.decide(a, 1, "w3");
    assert_eq!(of(&r, &ha), merging, "given to the cutter, at the same step");
    r.answer(a, "w3", json!({"head": "Merged", "merge": "bbbb2222"}));
    let at_cut = vec![fact("stands", &[&ha, "cut_of"]), fact("stands-with", &[&ha, "commit", "aaaa1111"]), fact("stands-with", &[&ha, "merge", "bbbb2222"]), fact("hands-in", &[&ha, &his_p1, "owner"])];
    assert_eq!(of(&r, &ha), at_cut);
    // an agent's post, and a head for it; a head for no post (a thread)
    let w3 = says(&mut r, t, "w3", &["w2"], None, None, "and one for the log");
    let b = open(&mut r, "w2", "cccc3333", &format!("{id}.p{w3}"));
    let hb = r.m.id(b);
    assert_eq!(of(&r, &hb), vec![fact("stands", &[&hb, "merge"]), fact("stands-with", &[&hb, "commit", "cccc3333"]), fact("hands-in", &[&hb, &format!("{id}.p{w3}"), "w3"])]);
    let c = open(&mut r, "w2", "dddd4444", &id);
    let hc = r.m.id(c);
    assert_eq!(of(&r, &hc), vec![fact("stands", &[&hc, "merge"]), fact("stands-with", &[&hc, "commit", "dddd4444"])], "{id} is no post");
    // the start reads the same; then A carried by a cut: closed, it stands nowhere
    let mut again = Machine::load(stand_defs());
    assert_eq!(again.replay(&r.log), Vec::<String>::new());
    assert_eq!(again.standing(), r.m.standing());
    r.answer(a, "dev", json!({"head": "In", "release": "20261122-170000-aaaa1111"}));
    assert!(matches!(r.m.tickets[&a].status, Status::Done(_)));
    assert!(of(&r, &ha).is_empty(), "{:?}", r.m.standing());
    assert_eq!(r.m.standing().iter().filter(|(f, _)| f == "stands").count(), 2, "B and C stand");
}

/// `STAND_KINDS`' head, and the cut a `when` binding opens when the policy answers go, handed the heads that stand at
/// the cut's hold (dev.kind's `when cut-due is go open cut(stands.cut_of)`).
const CUT_KINDS: [(&str, &str); 2] = [
    STAND_KINDS[0],
    ("cut.kind", "kind cut\n  letter U\n  form heads: [Ticket]\n  result ()\n  pattern\n    return ()\n\nwhen cut-due is go open cut(stands.cut_of)\n"),
];

/// A `when` binding hands the kind it opens where the open tickets stand (`syntax::STANDS`; the dev program's lack 5,
/// proto-mind N-1446 p1502, on lack 10's `standing`): the cut opened when the policy answers go is handed the heads at
/// `cut_of`, by number, not the one still in its merge; once those are carried, none. The form is on the record's
/// line, so a start reads the same. `stands` names only calls there are, and only a decision's binding reads it.
#[test]
fn a_when_binding_hands_the_kind_it_opens_the_tickets_that_stand_at_a_call() {
    let mut r = Run { m: Machine::load(stand_defs_with(&CUT_KINDS)), log: Vec::new(), clock: 0 };
    let text = r.m.defs.bindings.iter().find(|b| b.kind == "cut").unwrap().text.clone();
    assert_eq!(text, "when cut-due is go open cut(stands.cut_of)");
    let open = |r: &mut Run, commit: &str| {
        let out = r.go(Input::Open { kind: "head".into(), under: None, opener: "w2".into(), form: json!({"commit": commit, "for": "I-1"}), text: None, names: vec![], told: vec![] });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        *r.m.tickets.range(..HIDDEN).next_back().unwrap().0
    };
    let merged = |r: &mut Run, h: u64, onto: &str| {
        r.decide(h, 1, "w3");
        r.answer(h, "w3", json!({"head": "Merged", "merge": onto}));
    };
    let a = open(&mut r, "aaaa1111");
    merged(&mut r, a, "aaaa0000");
    let b = open(&mut r, "cccc3333");
    let c = open(&mut r, "dddd4444");
    merged(&mut r, c, "dddd0000");
    let fire = |r: &mut Run| {
        let out = r.go(Input::Event { name: "when".into(), value: json!({"binding": text, "answer": "go", "decision": {"chosen": "go"}}) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        let (_, x) = r.m.tickets.range(..HIDDEN).next_back().unwrap();
        assert_eq!(x.kind, "cut");
        x.form.clone()
    };
    let (ha, hb, hc) = (r.m.id(a), r.m.id(b), r.m.id(c));
    assert_eq!(fire(&mut r), json!({"heads": [ha, hc]}), "{hb} is in its merge");
    let fired = r.log.iter().rev().find_map(|l| match l { Line::Event { fired, .. } => fired.first().map(|f| f.form.clone()), _ => None });
    assert_eq!(fired, Some(json!({"heads": [ha, hc]})), "on the record's line");
    for h in [a, c] {
        r.answer(h, "dev", json!({"head": "In", "release": "20261122-180000-aaaa1111"}));
    }
    assert_eq!(fire(&mut r), json!({"heads": []}), "carried: none stands at cut_of");
    let mut again = Machine::load(stand_defs_with(&CUT_KINDS));
    assert_eq!(again.replay(&r.log), Vec::<String>::new());
    for (t, x) in r.m.tickets.range(..HIDDEN) {
        assert_eq!(again.tickets[t].form, x.form, "{t}");
    }
    // a call there is not, and a binding on an event or a clock: refused
    for (binding, why) in [("when cut-due is go open cut(stands.cut_off)", "cut_off"), ("every 1h open cut(stands.cut_of)", "stands")] {
        let cut = format!("kind cut\n  letter U\n  form heads: [Ticket]\n  result ()\n  pattern\n    return ()\n\n{binding}\n");
        let (_, errors) = stand_build(&[STAND_KINDS[0], ("cut.kind", cut.as_str())]);
        assert!(errors.iter().any(|e| e.contains(why)), "{binding}: {errors:?}");
    }
}

/// The kinds of the `asked` tests: a goal for each ask of his, opened by a binding on `asked` (N-1511 p47), and the
/// head handed in for it, at whose first `asked` is raised (paper-research N-1446 p2354).
const GOALS: &[(&str, &str)] = &[
    ("goal.kind", "kind goal\n  form post: Ref, title: Text\n  result ()\n  pattern\n    return ()\n"),
    ("asks.kind", "on asked(e) where e.by == \"owner\" open goal(e.post, e.title)\n"),
    HEAD_KINDS[0],
];

/// The posts coordd raises `asked` for along R's record replayed under EXTRA (`graph::Builder::first_handed`), each
/// with the opener of the head handed in for it in the input it is raised at.
fn first_handed(r: &Run, extra: &[(&str, &str)]) -> Vec<(String, Option<String>)> {
    let mut m = Machine::load(defs_with(extra).0);
    let mut b = crate::graph::Builder::default();
    let mut out = Vec::new();
    let problems = m.replay_with(&r.log, &mut |m, i, group| {
        b.after(m, i, group);
        for (t, n) in b.first_handed.drain(..) {
            let post = crate::graph::post_id(m, t, n);
            let by = group.iter().find_map(|l| match l {
                Line::Open { opener, form, .. } if form["for"] == json!(post) => Some(opener.clone()),
                _ => None,
            });
            out.push((post, by));
        }
    });
    assert_eq!(problems, Vec::<String>::new());
    out
}

/// The goals R's machine holds, by the post each is for.
fn goals(r: &Run) -> Vec<Value> {
    r.m.tickets.values().filter(|x| x.kind == "goal").map(|x| x.form["post"].clone()).collect()
}

/// His ask raises `asked` once, at the first head handed in for it (paper-research N-1446 p2354; N-1511 p47, p2204):
/// nothing at his post, at w2's answer or at w2's ask of w3, so a post no work answers ("jeez", p2352) opens no goal;
/// w2's head for p1 raises it, w3's second head for p1 nothing more, a head for w2's own ask nothing. Sent back naming
/// the post, it is a line with the ledger's value (whom it asks, its first line) whose binding opens one goal for it;
/// a second for the post, an agent's post and a post not there are refused and open nothing. A start reads the lines:
/// the same goals, and none raised again.
#[test]
fn his_ask_is_raised_once_at_its_first_head_and_its_asked_line_opens_its_goal() {
    let mut r = Run::with(GOALS);
    let t = his(&mut r, &["w2", "w3"], "**is the build green?**\nsay so here");
    let id = r.m.id(t);
    let p1 = format!("{id}.p1");
    assert_eq!(r.m.asked_value(t, 1), Some(json!({"post": p1, "by": "owner", "to": ["w2", "w3"], "title": "is the build green?"})));
    r.give("w2", t);
    says(&mut r, t, "w2", &[], None, None, "green");
    let more = his_post(&mut r, t, &["w2"], "and the stills?");
    let ask = says(&mut r, t, "w2", &["w3"], None, Some(PostKind::Ask { to: vec!["w3".into()] }), "w3, the stills?");
    let (pm, pa) = (format!("{id}.p{more}"), format!("{id}.p{ask}"));
    assert!(first_handed(&r, GOALS).is_empty(), "no head yet: nothing raised");
    hand_in(&mut r, "w2", &p1);
    hand_in(&mut r, "w3", &p1);
    hand_in(&mut r, "w3", &pa);
    hand_in(&mut r, "w2", &pm);
    let firsts = first_handed(&r, GOALS);
    assert_eq!(firsts, [(p1.clone(), Some("w2".to_string())), (pm.clone(), Some("w2".to_string()))]);
    for (p, _) in &firsts {
        let out = r.go(Input::Event { name: ASKED.into(), value: json!({"post": p}) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
        let line = |l: &Line| matches!(l, Line::Event { name, value, fired, .. } if name == ASKED && value["post"] == json!(p) && fired.len() == 1);
        assert!(out.lines.iter().any(line), "{:?}", out.lines);
    }
    assert_eq!(goals(&r), [json!(p1), json!(pm)]);
    for p in [p1.clone(), pa.clone(), format!("{id}.p99")] {
        let out = r.go(Input::Event { name: ASKED.into(), value: json!({"post": p}) });
        assert!(out.reply.is_err() && out.lines.is_empty(), "{p}: {:?}", out.reply);
    }
    assert_eq!(goals(&r).len(), 2);
    let mut again = Machine::load(defs_with(GOALS).0);
    assert_eq!(again.replay(&r.log), Vec::<String>::new());
    assert_eq!((again.asks_raised.len(), again.asked_value(t, 1)), (2, None));
    r.replays_with(GOALS);
}

/// His `@all` raises nothing at the hand that first has it asking an agent (the watcher's decision), nor when its
/// holder passes it on: `asked` comes once, at the first head handed in for it, the fixer's (paper-research N-1446
/// p2354); with no binding on `asked` the machine writes the same lines, and a binding's `when asked(e)` handler is
/// refused at load.
#[test]
fn his_all_is_raised_once_at_its_first_head_not_at_its_hand_nor_when_passed() {
    let mut r = Run::with(GOALS);
    let t = r.two_on_a_thread();
    let (d, _) = r.owner_posts(t, &[], "@all is the disk full?");
    let h = r.handler_of(t, d);
    let Status::Asking { owed, .. } = &r.m.tickets[&h].status else { panic!("{:?}", r.m.tickets[&h].status) };
    let step = owed.step;
    r.go(Input::Decided { t: h, step, question: "decide".into(), answer: json!("lead"), provenance: json!({"decision": "claims"}) }).reply.unwrap();
    r.give("lead", t);
    r.go(Input::Pass { t, by: "lead".into(), n: d, to: "fixer".into(), text: "the disk is yours".into() }).reply.unwrap();
    assert_eq!(r.m.asked(t, d), vec![("fixer".to_string(), None)]);
    let all = format!("{}.p{d}", r.m.id(t));
    assert!(first_handed(&r, GOALS).is_empty(), "no head yet: nothing raised");
    hand_in(&mut r, "fixer", &all);
    hand_in(&mut r, "lead", &all);
    let firsts = first_handed(&r, GOALS);
    assert_eq!(firsts, [(all.clone(), Some("fixer".to_string()))]);
    assert_eq!(first_handed(&r, &HEAD_KINDS), firsts, "the binding changes no line");
    let k = "kind k\n  form a: Text\n  result ()\n  pattern\n    return ()\n  when asked(e) do\n    post \"asked\" on this\n";
    let (_, errors) = defs_with(&[("k.kind", k)]);
    assert!(errors.iter().any(|e| e.contains("asked opens tickets by a binding")), "{errors:?}");
}

/// A goal for each ask of his whose hold reads its duties (N-1511 p47; paper-research N-1446 p2139, 2): Done with the
/// lines that met them when none is owed, Broken at the first broken one; and the heads handed in for an ask.
const DUTY_GOALS: &[(&str, &str)] = &[
    ("goal.kind", "kind goal\n  form post: Ref, title: Text\n  result Done(by: [Ref]) | Broken(duty: Text?, line: Ref?) | Dropped(why: Text)\n  pattern\n    hold w3 \"Close {{ title }}\" -> Done(by: [Ref]) | Broken(duty: Text?, line: Ref?) | Dropped(why: Text) on duties(e) where e.goal == this and e.owed == [] => Done(e.by) on duties(e) where e.goal == this and e.broken != none => Broken(e.broken.duty, e.broken.line)\n"),
    ("asks.kind", "on asked(e) where e.by == \"owner\" open goal(e.post, e.title)\n"),
    HEAD_KINDS[0],
];

/// research's binding (its kinds/goals.kind, made up here the same): his ask opens an item with the post as its `ask`
/// (paper-research N-1446 p2255, 1); and the head handed in for it.
const ITEM_GOALS: &[(&str, &str)] = &[("goals.kind", "on asked(e) where e.by == \"owner\" open item(e.title, none, e.post)\n"), HEAD_KINDS[0]];

/// The `duties` questions OUT puts: each goal's id and the lines it is read over.
fn duties_asked(out: &Out) -> Vec<(String, Vec<Value>)> {
    out.effects.iter().filter_map(|e| match e {
        Effect::Ask { question: Question::Duties { goal, lines }, .. } => Some((goal.clone(), lines.clone())),
        _ => None,
    }).collect()
}

/// The policy's ANSWER to goal G's ask that stands: taken, or why not.
fn duties(r: &mut Run, g: u64, answer: Value) -> (Out, Result<String, String>) {
    let step = r.m.awaits_duties(g).unwrap_or_default();
    let out = r.go(Input::Decided { t: g, step, question: "duties".into(), answer, provenance: json!({"asked": "duties"}) });
    let reply = out.reply.clone();
    (out, reply)
}

/// A runner's job line, PHASE start or end, of job ID on COMMIT with tree TREE (a start on the commit unchanged).
fn job(phase: &str, id: &str, commit: &str, tree: &str) -> Input {
    let more = match phase {
        "start" => json!({"phase": "start", "id": id, "commit": commit, "tree": tree, "changed": 0, "wt": "w"}),
        _ => json!({"phase": "end", "id": id, "machine": "tower", "rc": 0, "verb": "test", "commit": commit, "tree": tree, "branch": "b", "packages": [], "items": [], "passed": 12, "failed": 0}),
    };
    Input::Act { agent: "runner".into(), is: "job".into(), text: format!("job {phase}"), more: more.as_object().cloned().unwrap(), t: None }
}

/// A goal's duties (N-1511 p47; paper-research N-1446 p2139, 2, p2204): the goal his ask's `asked` line opens awaits
/// them at its hold and is asked at once, over that line and its step's. One ask at a time: the head handed in for the
/// ask, opened while the first stands, asks again at its answer, with the head's lines. A line that touches no open
/// goal asks nothing: w2's post, a job on another commit and tree, a job's end on a tree not yet known as the head's,
/// his window on another release. A job start that names the head's tree asks, and that earlier end goes in as its
/// act line; his window on the head's commit is one of the goal's lines. Each answer is the `duties` line its hold
/// reads by `this`: an answer that does not fit (its holders or met too), is for another goal or comes unasked is
/// refused; the second goal's Broken leaves the first owed, and none owed is Done with the lines that met them. A
/// start reads the same.
#[test]
fn a_goals_duties_are_asked_at_the_lines_that_touch_it_and_its_hold_reads_the_answer() {
    let mut r = Run::with(DUTY_GOALS);
    let t = his(&mut r, &["w2"], "**the page**\nmake it");
    let p1 = format!("{}.p1", r.m.id(t));
    let out = r.go(Input::Event { name: ASKED.into(), value: json!({"post": p1}) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let g = r.m.tickets.values().find(|x| x.kind == "goal").unwrap().t;
    let gid = r.m.id(g);
    assert!(r.m.awaits_duties(g).is_some(), "{:?}", r.m.tickets[&g].status);
    let asked = duties_asked(&out);
    assert_eq!((asked.len(), asked[0].0.as_str()), (1, gid.as_str()), "{asked:?}");
    assert_eq!(asked[0].1[0]["name"], json!(ASKED));
    assert!(asked[0].1.iter().any(|l| l["ev"] == "step" && l["t"] == json!(g)), "{:?}", asked[0].1);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Asked { t, question, .. } if *t == g && question == "duties")));
    // the head, opened while that ask stands: asked again at its answer
    let out = r.go(Input::Open { kind: "head".into(), under: None, opener: "w2".into(), form: json!({"commit": "1a2b3c4d", "for": p1}), text: None, names: vec![], told: vec![] });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let h = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    assert!(duties_asked(&out).is_empty());
    let owed = json!({"goal": gid, "owed": ["owes-read"], "by": [], "broken": null});
    // with who holds the read and which line met the checks (p2421): on its view with the line's time (p2328, 2)
    let line = "2026-11-23T17:08:47Z job end 41-17";
    let rich = json!({"goal": gid, "owed": ["owes-read"], "holders": [{"duty": "owes-read", "holder": "w3"}], "by": [line], "met": [{"duty": "owes-checks", "holder": "w2", "line": line}], "broken": null});
    let (out, reply) = duties(&mut r, g, rich.clone());
    assert!(reply.is_ok(), "{reply:?}");
    let at = r.log.iter().rev().find_map(|l| match l {
        Line::Decided { at, t, question, .. } if *t == g && question == "duties" => Some(json!(at)),
        _ => None,
    });
    let seen = r.m.view(g).and_then(|v| v.duties).expect("its latest duties");
    assert_eq!((Some(&seen["at"]), &seen["owed"], &seen["holders"], &seen["met"]), (at.as_ref(), &rich["owed"], &rich["holders"], &rich["met"]), "{seen}");
    let asked = duties_asked(&out);
    assert_eq!(asked.len(), 1, "asked again: the head's open touched it");
    assert!(asked[0].1.iter().any(|l| l["ev"] == "open" && l["t"] == json!(h)), "{:?}", asked[0].1);
    assert!(r.m.awaits_duties(g).is_some(), "still owed: at its hold");
    // and one whose holders name no duty, or whose met names no line (the `duties` type's; p2421)
    let no_duty = json!({"goal": gid, "owed": ["owes-read"], "holders": [{"holder": "w3"}], "by": [], "broken": null});
    let no_line = json!({"goal": gid, "owed": [], "by": [line], "met": [{"duty": "owes-read", "holder": "w2"}], "broken": null});
    for bad in [json!({"goal": "N-999", "owed": [], "by": [], "broken": null}), json!({"goal": gid, "owed": "none"}), no_duty, no_line] {
        assert!(duties(&mut r, g, bad.clone()).1.is_err(), "{bad}");
    }
    duties(&mut r, g, owed.clone()).1.unwrap();
    assert!(duties(&mut r, g, owed.clone()).1.is_err(), "no ask stands");
    // lines that touch no goal ask nothing
    let before = r.m.duties_asked;
    r.give("w2", t);
    says(&mut r, t, "w2", &[], None, None, "on it");
    for i in [job("end", "41-16", "ffff0000", "t-other"), job("end", "41-17", "0000aaaa", "t-head")] {
        assert!(r.go(i).reply.is_ok());
    }
    assert!(r.go(Input::Event { name: WINDOW_RELEASE.into(), value: json!({"release": "20261123-170000-ffff0000", "contains": []}) }).reply.is_ok());
    assert_eq!(r.m.duties_asked, before);
    // a job run on the head's commit unchanged names its tree: the end before it on that tree is read
    let out = r.go(job("start", "41-18", "1a2b3c4d", "t-head"));
    let asked = duties_asked(&out);
    assert_eq!(asked.len(), 1);
    let end = asked[0].1.iter().find(|l| l["ev"] == "act" && l["phase"] == "end").expect("the job's end on the head's tree");
    assert_eq!((&end["id"], &end["agent"], &end["machine"], end.get("ticket")), (&json!("41-17"), &json!("runner"), &json!("tower"), None));
    duties(&mut r, g, owed.clone()).1.unwrap();
    let out = r.go(Input::Event { name: WINDOW_RELEASE.into(), value: json!({"release": "20261123-180857-1a2b3c4d", "contains": []}) });
    let asked = duties_asked(&out);
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].1.last().unwrap()["name"], json!(WINDOW_RELEASE));
    duties(&mut r, g, owed.clone()).1.unwrap();
    // his second ask, its own goal, opened at its first head as coordd raises it (paper-research N-1446 p2354): that
    // head's open and its `read` step, an input before, are the goal's first lines; the head's read asks the first only
    let p2 = format!("{}.p{}", r.m.id(t), his_post(&mut r, t, &["w2"], "and the stills?"));
    let h2 = hand_in(&mut r, "w2", &p2);
    let h2 = r.m.ticket_named(&h2).unwrap();
    assert_eq!(r.m.first_heads.keys().collect::<Vec<_>>(), [&p2]);
    let out = r.go(Input::Event { name: ASKED.into(), value: json!({"post": p2}) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let g2 = r.m.tickets.values().filter(|x| x.kind == "goal").map(|x| x.t).max().unwrap();
    assert_ne!(g2, g);
    assert!(r.m.first_heads.is_empty(), "taken by its goal");
    let asked = duties_asked(&out);
    let lines = asked.iter().find(|(id, _)| *id == r.m.id(g2)).map(|(_, ls)| ls.clone()).unwrap_or_default();
    let at = |f: &dyn Fn(&Value) -> bool| lines.iter().position(f);
    let (open, step) = (at(&|l| l["ev"] == "open" && l["t"] == json!(h2)), at(&|l| l["ev"] == "step" && l["t"] == json!(h2)));
    let raised = at(&|l| l["ev"] == "event" && l["name"] == json!(ASKED));
    assert!(open.is_some() && step.is_some() && open < raised && step < raised, "{lines:?}");
    let out = r.go(Input::Answer { t: h, by: "dev".into(), head: json!({"head": "Right", "why": "fine"}), text: Text::New("right".into()) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let asked = duties_asked(&out);
    assert_eq!(asked.iter().map(|(g, _)| g.as_str()).collect::<Vec<_>>(), [gid.as_str()]);
    // the second's broken read: Broken there, and the first still owed
    let broken = json!({"goal": r.m.id(g2), "owed": ["owes-read"], "by": [], "broken": {"duty": "owes-read", "line": "2026-11-22T15:00:30Z window_release"}});
    duties(&mut r, g2, broken).1.unwrap();
    assert!(matches!(&r.m.tickets[&g2].status, Status::Done(v) if v["head"] == "Broken" && v["duty"] == "owes-read"), "{:?}", r.m.tickets[&g2].status);
    assert!(r.m.awaits_duties(g).is_some());
    let by = json!(["2026-11-22T15:00:20Z job end 41-17", "2026-11-22T15:00:33Z answer by dev"]);
    duties(&mut r, g, json!({"goal": gid, "owed": [], "by": by, "broken": null})).1.unwrap();
    assert!(matches!(&r.m.tickets[&g].status, Status::Done(v) if v["head"] == "Done" && v["by"] == by), "{:?}", r.m.tickets[&g].status);
    let asks = r.log.iter().filter(|l| matches!(l, Line::Asked { question, .. } if question == "duties")).count();
    assert_eq!((asks, r.m.duties_asked), (6, 6));
    assert!(r.m.goal_lines.is_empty(), "both closed");
    assert!(r.m.view(g).and_then(|v| v.duties).is_some_and(|d| d["owed"] == json!([]) && d["by"] == by), "kept once closed");
    let mut again = Machine::load(defs_with(DUTY_GOALS).0);
    assert_eq!(again.replay(&r.log), Vec::<String>::new());
    assert_eq!((again.duties_asked, &again.goal_lines, &again.duties_seen), (6, &r.m.goal_lines, &r.m.duties_seen));
    r.replays_with(DUTY_GOALS);
}

/// His ask's item is his goal (N-1511 p47; paper-research N-1446 p2255): research's binding opens an item with the
/// post as its `ask`, which its lead holds at `goal` and its duties close, never a hand. The lead's `coord item close`
/// (Done with words) and a Broken by hand are refused, saying the way out, and nothing is written; a head handed in for
/// the item by its id is a head for his post (p2483): it answers the post, and the item's duties are asked again over
/// its lines; a second `asked` for the post opens no second item; none owed is Done by the lines that met them, `why`
/// none. His second ask's item is
/// dropped by its lead: Dropped is a hand's. An item opened by hand is held at `close` and closes with its words as
/// before, `by` none. A start reads the same.
#[test]
fn his_asks_item_is_closed_by_its_duties_and_only_dropped_by_hand() {
    let mut r = Run::with(ITEM_GOALS);
    let t = his(&mut r, &["w2"], "**the page**\nmake it");
    let p1 = format!("{}.p1", r.m.id(t));
    r.go(Input::Event { name: ASKED.into(), value: json!({"post": p1}) }).reply.unwrap();
    let items = |r: &Run| r.m.tickets.values().filter(|x| x.kind == "item").map(|x| x.t).collect::<Vec<_>>();
    let [g] = items(&r)[..] else { panic!("{:?}", items(&r)) };
    assert_eq!(r.m.tickets[&g].form, json!({"title": "the page", "note": null, "ask": p1}));
    // its lead holds it at `goal`, and its duties are asked there
    let out = r.go(Input::Decided { t: g, step: 1, question: "fills".into(), answer: json!("orch"), provenance: json!({}) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    assert!(r.m.holds("orch").contains(&(g, 1, "goal".to_string())), "{:?}", r.m.holds("orch"));
    let gid = r.m.id(g);
    let asked = duties_asked(&out);
    assert_eq!(asked.iter().map(|(g, _)| g.clone()).collect::<Vec<_>>(), [gid.clone()]);
    // read from the `asked` line that opened it, an input before its lead was filled: the policy's `take`
    let first = &asked[0].1[0];
    assert_eq!((&first["ev"], &first["name"], &first["fired"][0]["t"]), (&json!("event"), &json!(ASKED), &json!(g)), "{:?}", asked[0].1);
    // a head handed in `--for` the item: a head for his post, read at the duties' next ask
    let h = hand_in(&mut r, "w2", &gid);
    assert_eq!(r.m.asked(t, 1), vec![("w2".to_string(), Some(h.clone()))], "{h} answers p1");
    let (out, reply) = duties(&mut r, g, json!({"goal": gid, "owed": ["owes-read"], "by": [], "broken": null}));
    assert!(reply.is_ok(), "{reply:?}");
    let ht = r.m.ticket_named(&h).unwrap();
    let asked = duties_asked(&out);
    assert!(asked.len() == 1 && asked[0].1.iter().any(|l| l["ev"] == "open" && l["t"] == json!(ht)), "asked again over {h}'s open: {asked:?}");
    // by hand: refused, saying the way out; nothing written
    let way = format!("at step 1 of {gid} comes only from its `duties` line, never by hand: when that line will not come, its lead cancels, withdraws or repairs {gid}, or drops it (`coord item drop`).");
    let n = r.log.len();
    let close = r.go(Input::Answer { t: g, by: "orch".into(), head: json!("Done"), text: Text::New("landed in 72".into()) });
    assert_eq!(close.reply, Err(format!("Done {way}")));
    let broken = r.go(Input::Answer { t: g, by: "orch".into(), head: json!({"head": "Broken", "duty": "owes-read", "line": "2026-11-23T17:12:34Z answer by w3"}), text: Text::New("broken".into()) });
    assert_eq!(broken.reply, Err(format!("Broken {way}")));
    assert_eq!(r.log.len(), n);
    // a second `asked` for the post: refused, no second item
    let again = r.go(Input::Event { name: ASKED.into(), value: json!({"post": p1}) });
    assert!(again.reply.is_err() && again.lines.is_empty(), "{:?}", again.reply);
    assert_eq!(items(&r), [g]);
    // none owed: Done by the lines that met them
    let by = json!(["2026-11-23T17:08:47Z job end 41-17", "2026-11-23T17:56:46Z window_release"]);
    duties(&mut r, g, json!({"goal": gid, "owed": [], "by": by, "broken": null})).1.unwrap();
    assert_eq!(r.status(g), ViewStatus::Done { result: json!({"head": "Done", "why": null, "by": by}) });
    // his second ask's item, dropped by its lead
    let p2 = format!("{}.p{}", r.m.id(t), his_post(&mut r, t, &["w2"], "and the stills?"));
    r.go(Input::Event { name: ASKED.into(), value: json!({"post": p2}) }).reply.unwrap();
    let g2 = *items(&r).last().unwrap();
    assert_ne!(g2, g);
    r.go(Input::Decided { t: g2, step: 1, question: "fills".into(), answer: json!("orch"), provenance: json!({}) }).reply.unwrap();
    r.go(Input::Answer { t: g2, by: "orch".into(), head: json!("Dropped"), text: Text::New("he took it back".into()) }).reply.unwrap();
    assert_eq!(r.status(g2), ViewStatus::Done { result: json!({"head": "Dropped", "why": "he took it back"}) });
    // an item opened by hand: held at `close`, closed with its words
    let i = r.open_item("faster replay");
    r.go(Input::Decided { t: i, step: 1, question: "fills".into(), answer: json!("orch"), provenance: json!({}) }).reply.unwrap();
    assert!(r.m.holds("orch").contains(&(i, 1, "close".to_string())), "{:?}", r.m.holds("orch"));
    r.go(Input::Answer { t: i, by: "orch".into(), head: json!("Done"), text: Text::New("landed in 72".into()) }).reply.unwrap();
    assert_eq!(r.status(i), ViewStatus::Done { result: json!({"head": "Done", "why": "landed in 72", "by": null}) });
    let mut again = Machine::load(defs_with(ITEM_GOALS).0);
    assert_eq!(again.replay(&r.log), Vec::<String>::new());
    r.replays_with(ITEM_GOALS);
}
