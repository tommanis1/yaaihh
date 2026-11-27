//! The record graph's edges from posts and deliveries (graph_posts.rs; paper-research N-1446 p1376, 3) on a run's own
//! log, replayed as a record is (`graph::of`).

use super::*;
use crate::graph::{EdgeKind, NodeKind, Way, way};

/// Two on a thread (`two_on_a_thread`: his words to lead, lead's reply to him, lead's `@fixer`, fixer's reply), then
/// fixer's post, which informs lead. Each delivery hands the posts that giving brought to its agent, which took the
/// slice of its session it landed in; whose message to whom is read off the edges. Made up.
#[test]
fn a_delivery_is_the_slice_its_agent_took_with_the_posts_it_brought() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.post_on(t, "fixer", "the median is fixed", &[]).reply.unwrap();
    let (g, problems) = crate::graph::of(&mut Machine::load(defs()), &r.log);
    assert!(problems.is_empty(), "{problems:?}");
    let id = r.m.id(t);
    // fixer's delivery: every post up to its UPTO that is not fixer's own handed to fixer, and the slice fixer took
    let (i, upto, delivery) = r.log.iter().enumerate().find_map(|(i, l)| match l {
        Line::Given { to, upto, delivery, .. } if to == "fixer" => Some((i, *upto, delivery.clone())),
        _ => None,
    }).unwrap();
    let s = crate::graph::slice_id("fixer", &delivery, i + 1);
    assert_eq!(g.get(&s).map(|n| n.kind), Some(NodeKind::Slice));
    let into = |kind: EdgeKind| -> Vec<String> { g.edges.iter().filter(|e| e.to == s && e.kind == kind).map(|e| e.from.clone()).collect() };
    assert_eq!(into(EdgeKind::Took), ["agent:fixer"]);
    let brought: Vec<String> = r.m.tickets[&t].posts.iter().filter(|p| p.n <= upto && p.by != "fixer").map(|p| format!("{id}.p{}", p.n)).collect();
    assert!(brought.len() >= 2, "{brought:?}");
    assert_eq!(g.edges.iter().filter(|e| e.to == "agent:fixer" && e.kind == EdgeKind::Handed && e.line() == Some(i + 1)).map(|e| e.from.clone()).collect::<Vec<_>>(), brought);
    assert!(into(EdgeKind::Handed).is_empty(), "nothing is handed to a slice");
    // whose to whom: his words handed to lead's session; lead's `@fixer` woke fixer; lead's reply answered him;
    // fixer's post informed lead
    let ways = |kind: EdgeKind, to: &str| -> Vec<Option<Way>> { g.edges.iter().filter(|e| e.kind == kind && e.to.starts_with(to)).map(|e| way(&g, e)).collect() };
    assert!(ways(EdgeKind::Handed, "agent:lead").contains(&Some(Way::OwnerToAgent)), "{:?}", ways(EdgeKind::Handed, "agent:lead"));
    assert_eq!(ways(EdgeKind::Woke, "agent:fixer"), [Some(Way::AgentToAgent)]);
    assert!(ways(EdgeKind::Answered, "agent:owner").contains(&Some(Way::AgentToOwner)), "{:?}", ways(EdgeKind::Answered, "agent:owner"));
    assert!(ways(EdgeKind::Informed, "agent:lead").contains(&Some(Way::AgentToAgent)), "{:?}", ways(EdgeKind::Informed, "agent:lead"));
    // his post reached lead
    let his = g.ask_times().into_iter().find(|a| a.post == format!("{}.p1", r.m.id(t))).expect("his post's course");
    assert!(his.reached.as_ref().is_some_and(|x| x.id.starts_with("agent:")), "{:?}", his.reached);
    // lead's turn-end reply answers what its turn held (docket-edge5 N-1446 p1396), as the machine counts it, a kind
    // apart from the reply tool's target (p1409, 1): it → his post, so lead took it
    let reply = r.log.iter().find_map(|l| match l {
        Line::Replied { by, n, .. } if by == "lead" => Some(format!("{id}.p{n}")),
        _ => None,
    }).expect("lead's reply");
    assert!(g.edges.iter().any(|e| e.from == reply && e.to == format!("{id}.p1") && e.kind == EdgeKind::AnsweredUpto), "{reply}");
    assert!(!g.edges.iter().any(|e| e.from == reply && e.to == format!("{id}.p1") && e.kind == EdgeKind::Answered), "{reply}");
    // fixer's post, which names no post it answers, answers by the machine the last post of lead's it was handed
    let fixed = format!("{id}.p{}", r.m.tickets[&t].posts.iter().rev().find(|p| p.by == "fixer").unwrap().n);
    let upto: Vec<&str> = g.edges.iter().filter(|e| e.from == fixed && e.kind == EdgeKind::AnsweredUpto).map(|e| e.to.as_str()).collect();
    assert_eq!(upto.len(), 1, "{fixed}: {upto:?}");
    assert!(g.edges.iter().any(|e| e.kind == EdgeKind::Wrote && e.from == "agent:lead" && e.to == upto[0]), "{upto:?}");
    assert!(his.taken.as_ref().is_some_and(|x| x.id == "agent:lead"), "{:?}", his.taken);
    // a slice is in deliveries and is its agent's: fixer's deliveries hold what was handed to fixer, and its slice
    let ask = crate::graph_slice::Ask { who: vec!["fixer".into()], layers: std::collections::BTreeSet::from([crate::graph_slice::Layer::Deliveries]), ..Default::default() };
    let part = g.slice(&ask);
    assert!(part.edges.iter().any(|e| e.edge.to == "agent:fixer" && e.edge.kind == EdgeKind::Handed), "{:?}", part.shown);
    assert!(part.edges.iter().any(|e| e.edge.to == s && e.edge.kind == EdgeKind::Took && e.layer == crate::graph_slice::Layer::Deliveries));
    assert_eq!(part.nodes.iter().find(|n| n.node.id == s).and_then(|n| n.by.as_deref()), Some("fixer"));
    // nothing of a slice's place is in the graph: no node's id holds the session's file
    assert!(!g.nodes.iter().any(|n| n.id.contains("transcript.jsonl")));
}

/// A delivery that began a turn (`prompted`, docket-edge5's, from coordd's `delivered` line, which a run's own log does
/// not have) is told apart as a handed post is: his post that began lead's turn is his to an agent. Made up.
#[test]
fn a_post_that_began_a_turn_is_told_apart_as_a_handed_one_is() {
    let mut g = crate::graph::Graph::default();
    let post = g.node("N-1.p1", NodeKind::Post, 1);
    let (owner, lead) = (g.node(crate::graph::agent_id("owner"), NodeKind::Agent, 1), g.node(crate::graph::agent_id("lead"), NodeKind::Agent, 1));
    g.edge(&owner, &post, EdgeKind::Wrote, 1);
    g.edge(&post, &lead, EdgeKind::Prompted, 2);
    let prompted = g.edges.iter().find(|e| e.kind == EdgeKind::Prompted).unwrap();
    assert_eq!(way(&g, prompted), Some(Way::OwnerToAgent));
}

/// One giving, one `handed` per post (paper-research N-1446 p1409, on docket-edge6's run 4): coordd's two lines for a
/// delivery, handed and then landed in the session (`where`), hand its posts once, at the first; the second is the
/// slice, which the agent took. A giving recorded with nothing handed (upto 0, read after a restart) hands its posts at
/// the line that gives them. Made up.
#[test]
fn a_giving_hands_its_posts_once_and_its_landing_is_the_slice() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.post_on(t, "lead", "@fixer and the mean", &["fixer"]).reply.unwrap();
    let d = r.m.due("fixer").into_iter().find(|d| d.t == t).expect("fixer owed lead's post");
    let given = |r: &mut Run, upto: u32, place: Option<Loc>| r.go(Input::Given { t, to: "fixer".into(), delivery: d.id.clone(), upto, skipped: Default::default(), place }).reply.unwrap();
    given(&mut r, d.upto, None);
    given(&mut r, d.upto, Some(Run::loc(40)));
    let (g, problems) = crate::graph::of(&mut Machine::load(defs()), &r.log);
    assert!(problems.is_empty(), "{problems:?}");
    let lines: Vec<usize> = r.log.iter().enumerate().filter(|(_, l)| matches!(l, Line::Given { delivery, .. } if *delivery == d.id)).map(|(i, _)| i + 1).collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    let post = format!("{}.p{}", r.m.id(t), d.upto);
    let handed: Vec<usize> = g.edges.iter().filter(|e| e.from == post && e.to == "agent:fixer" && e.kind == EdgeKind::Handed).filter_map(|e| e.line()).collect();
    assert_eq!(handed, [lines[0]], "handed once, by the first line");
    let s = crate::graph::slice_id("fixer", &d.id, lines[1]);
    assert_eq!(g.edges.iter().filter(|e| e.to == s).map(|e| (e.from.as_str(), e.kind)).collect::<Vec<_>>(), [("agent:fixer", EdgeKind::Took)]);
    // read after a restart: recorded with nothing handed, then found in the session, given up to its post
    r.post_on(t, "lead", "@fixer and the mode", &["fixer"]).reply.unwrap();
    let e = r.m.due("fixer").into_iter().find(|e| e.t == t).expect("fixer owed lead's next post");
    r.go(Input::Given { t, to: "fixer".into(), delivery: e.id.clone(), upto: 0, skipped: Default::default(), place: None }).reply.unwrap();
    r.go(Input::Given { t, to: "fixer".into(), delivery: e.id.clone(), upto: e.upto, skipped: Default::default(), place: Some(Run::loc(50)) }).reply.unwrap();
    let (g, _) = crate::graph::of(&mut Machine::load(defs()), &r.log);
    let last = r.log.iter().rposition(|l| matches!(l, Line::Given { delivery, .. } if *delivery == e.id)).unwrap() + 1;
    let post = format!("{}.p{}", r.m.id(t), e.upto);
    let handed: Vec<usize> = g.edges.iter().filter(|x| x.from == post && x.to == "agent:fixer" && x.kind == EdgeKind::Handed).filter_map(|x| x.line()).collect();
    assert_eq!(handed, [last], "handed by the line that gave it");
}

/// One wake, one `woke` (p1409): his post a handler takes wakes by the handler's decision alone. Told at once (his
/// `@fixer` on a thread): one woke, at the `told` line. Dropped: none. Released as policy said: one each to its names
/// and told, at the `released` line. Made up, as the machine's own tests of each.
#[test]
fn his_post_a_handler_takes_wakes_once_as_the_handler_decided() {
    let woke = |g: &crate::graph::Graph, post: &str| -> Vec<(String, usize)> { g.edges.iter().filter(|e| e.from == post && e.kind == EdgeKind::Woke).map(|e| (e.to.clone(), e.line().unwrap())).collect() };
    let at = |r: &Run, f: &dyn Fn(&Line) -> bool| -> usize { r.log.iter().position(|l| f(l)).expect("its line") + 1 };
    // told
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.owner_says(t, &["fixer"], "@fixer use the sorted copy");
    let n = r.m.tickets[&t].last();
    let (g, problems) = crate::graph::of(&mut Machine::load(defs()), &r.log);
    assert!(problems.is_empty(), "{problems:?}");
    let told = at(&r, &|l| matches!(l, Line::Told { t: u, n: k, .. } if *u == t && *k == n));
    let post = format!("{}.p{n}", r.m.id(t));
    assert_eq!(woke(&g, &post), [("agent:fixer".to_string(), told)]);
    assert_eq!(g.edges.iter().filter(|e| e.from == post && e.kind == EdgeKind::Informed).map(|e| e.to.as_str()).collect::<Vec<_>>(), ["agent:lead"]);
    // dropped
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.owner_says(t, &["lead"], "hmm");
    let n = r.m.tickets[&t].last();
    r.route(json!({"head": "Unclear", "why": "it asks nothing"}));
    let h = r.handler().unwrap();
    r.go(Input::Answer { t: h, by: "owner".into(), head: json!({"head": "Drop"}), text: Text::New(String::new()) }).reply.unwrap();
    let (g, _) = crate::graph::of(&mut Machine::load(defs()), &r.log);
    assert!(woke(&g, &format!("{}.p{n}", r.m.id(t))).is_empty());
    // released
    let k = "kind lax\n  form w: Agent\n  result ()\n  pattern\n    ...\n  when owner_post(p) do\n    return ()\n";
    let (d, errors) = defs_with(&[("lax.kind", k)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    r.go(Input::Open { kind: "lax".into(), under: None, opener: "lead".into(), form: json!({"w": "lead"}), text: None, names: vec!["lead".into()], told: vec![] }).reply.unwrap();
    let t = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    r.go(Input::Post { t, by: "owner".into(), text: Text::New("note this".into()), names: vec!["fixer".into()], told: vec!["lead".into()], inform: vec![], answers: None, kind: None }).reply.unwrap();
    let (g, _) = crate::graph::of(&mut Machine::load(defs_with(&[("lax.kind", k)]).0), &r.log);
    let released = at(&r, &|l| matches!(l, Line::Released { t: u, n: 1, .. } if *u == t));
    assert_eq!(woke(&g, &format!("{}.p1", r.m.id(t))), [("agent:fixer".to_string(), released), ("agent:lead".to_string(), released)]);
}

/// A post halted for his approval on a slow ticket reaches nobody until he lets it go (p1419, 1): it wakes at the line
/// that lets it go, his `approve` or slow switched off, whom the machine's `approve` delivers it to (as he left them:
/// bob, when his edit says so), and nobody at its `post` line. Made up, as the machine's own
/// `slow_a_post_delivered_halts_and_whom_it_wakes_is_edited_but_not_the_owners`.
#[test]
fn a_post_halted_on_a_slow_ticket_wakes_at_the_line_that_lets_it_go() {
    for (edit, off, wakes) in [(false, false, "agent:ann"), (true, false, "agent:bob"), (false, true, "agent:ann")] {
        let mut r = Run::slow_run();
        let (j, w) = r.slow_job();
        r.go(Input::Post { t: w, by: "owner".into(), text: Text::New("start small".into()), names: vec!["lead".into()], told: vec![], inform: vec![], answers: None, kind: None });
        r.take("lead");
        let out = r.go(Input::Post { t: w, by: "lead".into(), text: Text::New("ann, look".into()), names: vec!["ann".into()], told: vec![], inform: vec![], answers: None, kind: None });
        assert_eq!(Run::halted_lines(&out), vec![(w, "deliver".to_string())]);
        let n = r.m.tickets[&w].last();
        let h = r.m.tickets[&w].halts.iter().find(|h| h.what.name() == "deliver").unwrap().n;
        if edit {
            r.go(Input::HaltEdit { t: w, n: h, by: "owner".into(), value: json!({"wakes": ["bob"]}) }).reply.unwrap();
        }
        if off {
            r.go(Input::Slow { t: j, on: false, by: "owner".into() }).reply.unwrap();
        } else {
            r.approve(w, h);
        }
        let go = r.log.iter().rposition(|l| matches!(l, Line::Approve { .. } | Line::Slow { on: false, .. })).unwrap() + 1;
        let (g, problems) = crate::graph::of(&mut Machine::load(crate::machine::Defs::clone(&r.m.defs)), &r.log);
        assert!(problems.is_empty(), "{problems:?}");
        let post = format!("{}.p{n}", r.m.id(w));
        let woke: Vec<(String, usize)> = g.edges.iter().filter(|e| e.from == post && e.kind == EdgeKind::Woke).map(|e| (e.to.clone(), e.line().unwrap())).collect();
        assert_eq!(woke, [(wakes.to_string(), go)], "edit {edit}, off {off}");
    }
}

/// A program's own post, by its ticket: `post` naming one agent and informing another.
const SAY_TO: &str = "\
/// a program's own post, for the graph's tests
kind say_to
  form a: Agent, b: Agent
  result ()
  pattern
    _ <- post \"The build is green.\" naming a informing b;
    return ()
";

/// A program's post (`posted`, by its ticket's id): its writer is that ticket, the program run, and no agent is named
/// after it; it wakes whom it names and informs whom it informs at its own line, as the engine (paper-research N-1446
/// p1439). Made up.
#[test]
fn a_programs_post_is_written_by_its_ticket_and_wakes_whom_it_names_at_its_line() {
    let mut r = Run::with(&[("say_to.kind", SAY_TO)]);
    // it closes at once, so it is opened naming someone, as `note_to` is (tests/delivery.rs)
    r.go(Input::Open { kind: "say_to".into(), under: None, opener: "owner".into(), form: json!({"a": "ann", "b": "bob"}), text: None, names: vec!["lead".into()], told: vec![] }).reply.unwrap();
    let (line, t, n, by) = r.log.iter().enumerate().find_map(|(i, l)| match l {
        Line::Posted { t, n, by, .. } => Some((i + 1, *t, *n, by.clone())),
        _ => None,
    }).expect("the program's post");
    let id = r.m.id(t);
    assert_eq!(by, id, "posted by its ticket");
    let (g, problems) = crate::graph::of(&mut Machine::load(crate::machine::Defs::clone(&r.m.defs)), &r.log);
    assert!(problems.is_empty(), "{problems:?}");
    let post = format!("{id}.p{n}");
    let wrote: Vec<(&str, Option<NodeKind>)> = g.edges.iter().filter(|e| e.to == post && e.kind == EdgeKind::Wrote).map(|e| (e.from.as_str(), g.get(&e.from).map(|x| x.kind))).collect();
    assert_eq!(wrote, [(id.as_str(), Some(NodeKind::Ticket))]);
    assert!(g.get(&format!("agent:{id}")).is_none(), "no agent named after its ticket");
    let out: Vec<(&str, EdgeKind, usize, Option<Way>)> = g.edges.iter().filter(|e| e.from == post && matches!(e.kind, EdgeKind::Woke | EdgeKind::Informed)).map(|e| (e.to.as_str(), e.kind, e.line().unwrap(), way(&g, e))).collect();
    assert_eq!(out, [("agent:ann", EdgeKind::Woke, line, Some(Way::Engine)), ("agent:bob", EdgeKind::Informed, line, Some(Way::Engine))]);
}

/// His post put into a turn that ends without answering it is made due again and given again (`his_at_end`, as
/// `his_post_left_unanswered_in_two_turns_is_owed_until_answered` runs it). Both givings hand it, the second's `handed`
/// says which line made it due again, the turn's end, and the first's says none; `reached` is still the first
/// (paper-research N-1446 p1449). Made up.
#[test]
fn a_post_given_again_after_a_turn_left_it_unanswered_says_which_turn_end() {
    let mut r = Run::new();
    r.go(Input::Event { name: ERA.into(), value: json!({"era": ERA_PUT_IN}) }).reply.unwrap();
    let a = r.note("lead", "research", "the stills?");
    let d1 = r.give("research", a);
    let h = r.note("owner", "research", "and the release notes?");
    let d = r.m.due("research").into_iter().find(|d| d.t == h).unwrap();
    r.go(Input::GivenInto { t: h, to: "research".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).reply.unwrap();
    r.turn("research", Some(&d1), 1);
    assert!(r.m.handed_before("research", &d.id), "due again");
    assert_eq!(r.give("research", h), d.id);
    let end = r.log.iter().position(|l| matches!(l, Line::TurnEnded { agent, .. } if agent == "research")).unwrap() + 1;
    let given: Vec<usize> = r.log.iter().enumerate().filter(|(_, l)| matches!(l, Line::Given { t, to, .. } if *t == h && to == "research")).map(|(i, _)| i + 1).collect();
    assert_eq!(given.len(), 2, "{given:?}");
    let (g, problems) = crate::graph::of(&mut Machine::load(crate::machine::Defs::clone(&r.m.defs)), &r.log);
    assert!(problems.is_empty(), "{problems:?}");
    let post = format!("{}.p1", r.m.id(h));
    let handed: Vec<(usize, Option<usize>)> = g.edges.iter().filter(|e| e.from == post && e.to == "agent:research" && e.kind == EdgeKind::Handed).map(|e| (e.line().unwrap(), e.again)).collect();
    assert_eq!(handed, [(given[0], None), (given[1], Some(end))]);
    let first = g.edges.iter().find(|e| e.from == post && e.kind == EdgeKind::Handed).unwrap();
    let his = g.ask_times().into_iter().find(|x| x.post == post).expect("his post's course");
    assert_eq!(his.reached.as_ref().map(|x| &x.at), first.when.as_ref(), "reached is still the first");
}

/// Each post an agent wrote is in the turn it was written in (paper-research N-1446 p1458, point 3): its writer's next
/// turn end by record line, its own when the turn's end made it (`replied`); his posts are in none, nor is fixer's
/// last, with no turn end after it. In deliveries, with no words of its own. Made up.
#[test]
fn an_agents_post_is_in_the_turn_it_was_written_in() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.post_on(t, "fixer", "the median is fixed", &[]).reply.unwrap();
    let (g, problems) = crate::graph::of(&mut Machine::load(defs()), &r.log);
    assert!(problems.is_empty(), "{problems:?}");
    // each agent's turn ends by line, and each post's writer and line as `wrote` has it: a post is in its writer's
    // first turn end at or after it
    let ends: Vec<(String, usize)> = r.log.iter().enumerate().filter_map(|(i, l)| match l {
        Line::TurnEnded { agent, .. } | Line::TurnCut { agent, .. } => Some((agent.clone(), i + 1)),
        _ => None,
    }).collect();
    let wrote: Vec<(&str, &str, usize)> = g.edges.iter().filter(|e| e.kind == EdgeKind::Wrote && g.get(&e.to).is_some_and(|n| n.kind == NodeKind::Post)).map(|e| (e.from.as_str(), e.to.as_str(), e.line().unwrap())).collect();
    let (mut want, mut own, mut later) = (Vec::new(), 0, 0);
    for (by, post, line) in &wrote {
        let Some(agent) = by.strip_prefix("agent:") else { continue };
        if let Some((_, end)) = ends.iter().find(|(a, end)| a == agent && end >= line) {
            want.push((post.to_string(), crate::graph::turn_id(agent, *end)));
            if end == line { own += 1 } else { later += 1 }
        }
    }
    let mut got: Vec<(String, String)> = g.edges.iter().filter(|e| e.kind == EdgeKind::WrittenIn).map(|e| (e.from.clone(), e.to.clone())).collect();
    want.sort();
    got.sort();
    assert_eq!(got, want);
    assert!(own >= 1 && later >= 1, "a turn-end reply in its own turn ({own}) and a post between turns in the next ({later})");
    let id = r.m.id(t);
    let fixed = r.m.tickets[&t].posts.iter().rev().find(|p| p.by == "fixer").unwrap().n;
    let in_one = |n: u32| got.iter().any(|(p, _)| *p == format!("{id}.p{n}"));
    assert!(!in_one(1) && !in_one(fixed), "his post, and fixer's last with no turn end after it");
    for e in g.edges.iter().filter(|e| e.kind == EdgeKind::WrittenIn) {
        assert_eq!((e.line(), e.what.as_str()), (e.to.rsplit_once('@').and_then(|(_, n)| n.parse().ok()), ""), "at its turn's line");
        assert_eq!(crate::graph_slice::Layer::of(e, &g, &Default::default()), crate::graph_slice::Layer::Deliveries);
    }
}
