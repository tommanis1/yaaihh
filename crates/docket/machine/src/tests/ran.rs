//! `TicketView::ran` (docket-edge8 N-1446 p21; consult-3 p34; proto-lead p37): every run of a handler under a ticket,
//! open and closed, in one list that W3's handler rows and W4's post marks both read; and what a step's round says of
//! its wire (`RunRound::own`, `took`).

use super::*;
use crate::machine::AGENT_POST;
use serde_json::json;

/// A thread whose owner's posts each run `when owner_post` and whose agent's post runs `when agent_post`: each run is
/// listed in the order it opened, with its event, the `when` it ran (the kind it was written in and its line), the
/// post it ran for, its times and what it came to. One still held is open, and in `handlers` as before; answered, it
/// says the watcher's answer. A replay lists the same.
#[test]
fn every_run_of_a_handler_under_a_thread_is_listed_with_its_post_its_when_and_what_it_came_to() {
    let mut r = Run::before_asks();
    let t = r.two_on_a_thread();
    // the owner's first words, which opened the thread, ran it too
    let opening = r.m.tickets[&t].posts[0].n;
    r.owner_says(t, &["fixer"], "@fixer use the sorted copy");
    let told = r.m.tickets[&t].last();
    r.owner_says(t, &["lead"], "go ahead");
    let held = r.m.tickets[&t].last();
    let h = r.handler().unwrap();
    let when = |ev: &str| r.m.tickets[&t].handlers.iter().find(|w| w.ev == ev).map(|w| (Some(w.from.clone()), Some(w.line as u32))).unwrap();
    let v = r.m.view(t).unwrap();
    let owner: Vec<&ViewRan> = v.ran.iter().filter(|x| x.event == OWNER_POST).collect();
    assert_eq!(owner.iter().map(|x| x.post).collect::<Vec<_>>(), [Some(opening), Some(told), Some(held)], "{:?}", v.ran);
    assert!(owner.iter().all(|x| (x.from.clone(), x.line) == when(OWNER_POST)), "{:?}", v.ran);
    // the lead's post, read by the thread's `when agent_post` (the owner is on it)
    let lead = v.ran.iter().find(|x| x.event == AGENT_POST).unwrap_or_else(|| panic!("{:?}", v.ran));
    let first = r.m.tickets[&t].posts.iter().find(|p| p.by == "lead").unwrap().n;
    assert_eq!((lead.post, (lead.from.clone(), lead.line)), (Some(first), when(AGENT_POST)));
    // in the order they opened
    let ids: Vec<u64> = v.ran.iter().map(|x| r.m.tickets.values().find(|y| r.m.id(y.t) == x.id).unwrap().t).collect();
    assert!(ids.windows(2).all(|w| w[0] < w[1]), "{ids:?}");
    // told at once: closed, its result (what `tell` gave) saying nothing; held: open, nothing said yet, and still in
    // `handlers`
    assert!(owner[1].closed_at.is_some() && owner[1].said.is_none(), "{:?}", owner[1]);
    assert_eq!((owner[2].id.clone(), owner[2].closed_at.clone(), owner[2].said.clone()), (r.m.id(h), None, None));
    assert_eq!(v.handlers.iter().map(|x| x.id.clone()).collect::<Vec<_>>(), [r.m.id(h)]);
    r.route(json!({"head": "To", "agents": ["fixer"]}));
    let v = r.m.view(t).unwrap();
    let routed = v.ran.iter().find(|x| x.id == r.m.id(h)).unwrap();
    assert!(routed.closed_at.is_some(), "{routed:?}");
    assert_eq!(routed.said.as_deref(), Some(r#"To(["fixer"])"#), "the watcher's answer, as a value prints");
    assert!(v.handlers.is_empty());
    // its own view, as `Request::Ticket` opens it
    assert_eq!(r.m.view(h).map(|x| x.id), Some(r.m.id(h)));
    r.replays();
}

/// A run keeps the `when` it ran: a reload that moves the handler's line leaves the old run's line as it was, and a
/// run after it has the new one.
#[test]
fn a_run_keeps_the_line_of_the_when_it_ran_after_a_reload_moves_it() {
    const K: &str = "kind talk\n  form q: Text\n  result Text\n  raises Exhausted, Stalled\n  pattern\n    a <- step lead answer(q, none);\n    return q\n  when owner_post(p) do\n    return ()\n";
    let moved = K.replace("  when owner_post", "  when agent_post(q) do\n    return ()\n  when owner_post");
    let (d, errors) = defs_with(&[("talk.kind", K)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    r.go(Input::Open { kind: "talk".into(), under: None, opener: "lead".into(), form: json!({"q": "why"}), text: None, names: vec!["lead".into()], told: vec![] }).reply.unwrap();
    let t = *r.m.tickets.range(..HIDDEN).next_back().unwrap().0;
    r.owner_says(t, &["lead"], "one");
    let before = r.m.view(t).unwrap().ran;
    assert_eq!(before.iter().map(|x| x.line).collect::<Vec<_>>(), [Some(8)], "{before:?}");
    let line = 8;
    let (d, errors) = defs_with(&[("talk.kind", &moved)]);
    assert!(errors.is_empty(), "{errors:?}");
    *r.m.defs.edit() = d;
    r.go(Input::Reload { by: "owner".into(), why: "moved".into(), commit: None, kinds: vec!["talk".into()], calls: vec![] }).reply.unwrap();
    r.go(Input::Retext { t, by: "owner".into(), why: "moved".into() }).reply.unwrap();
    assert_eq!(r.m.tickets[&t].handlers.iter().find(|w| w.ev == OWNER_POST).map(|w| w.line), Some(10), "it took the new text");
    r.owner_says(t, &["lead"], "two");
    let after = r.m.view(t).unwrap().ran;
    assert_eq!(after.iter().map(|x| x.line).collect::<Vec<_>>(), [Some(line), Some(line + 2)], "{after:?}");
    assert!(after.iter().all(|x| x.from.as_deref() == Some("talk")), "{after:?}");
}

/// A step's round says whose its wire is: the watcher's round is coordd's own (`own`), an agent's is not. A step
/// given, never read, and given again before its answer says the giving its answer came after (`took`); one given
/// once has none.
#[test]
fn a_round_says_whether_coordd_answered_it_and_which_giving_its_answer_came_after() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.owner_says(t, &["lead"], "go ahead");
    let h = r.handler().unwrap();
    r.route(json!({"head": "To", "agents": ["fixer"]}));
    let rounds = |m: &Machine, t: u64| -> Vec<RunRound> {
        fn all(ns: &[RunNode], out: &mut Vec<RunRound>) {
            for n in ns {
                out.extend(n.rounds.iter().cloned());
                all(&n.nodes, out);
            }
        }
        let mut out = Vec::new();
        all(&m.view(t).unwrap().run, &mut out);
        out
    };
    let watcher = rounds(&r.m, h);
    assert!(!watcher.is_empty() && watcher.iter().all(|x| x.own && x.took.is_none()), "{watcher:?}");
    // a claim's step, handed and never read, then handed again and answered
    let mut r = Run::new();
    let c = r.open_claim();
    r.decide(c, 1, "orch");
    r.take("orch");
    let first = rounds(&r.m, c);
    assert!(first.iter().all(|x| !x.own && x.took.is_none()), "{first:?}");
    r.go(Input::Unread { agent: "orch".into() }).reply.unwrap();
    r.give("orch", c);
    let again = r.m.tickets[&c].given.last().unwrap().at.clone();
    r.answer(c, "orch", json!({"head": "Accept"}));
    let step = rounds(&r.m, c).into_iter().find(|x| x.step == 1).unwrap();
    assert_eq!(step.took.as_ref().map(|x| (x.to.as_str(), x.at.clone())), Some(("orch", again)), "{step:?}");
    assert!(step.given.as_ref().is_some_and(|g| *g < step.took.as_ref().unwrap().at), "{step:?}");
}

/// The record graph's handlers are the Map's (paper-research N-1446 p1375): a run of a `when` is a `ran` edge from its
/// ticket to `handler:KIND:EVENT`, KIND the kind that `when` was written in as `ViewRan::from` gives it (the key of
/// yaaihh map/layout.rs `handled`), and one to the run's own ticket; a hole's handler, which every kind takes from
/// base, is `handler:base:hole`, not the ticket's kind. Every handler in the graph is a `when` a ticket holds, and a
/// `handling` line for one its ticket does not hold is a problem at its line, with no handler (p1378).
#[test]
fn the_graphs_handlers_are_the_whens_in_the_kinds_they_were_written_in_as_the_map_keys_them() {
    use crate::graph::{EdgeKind, NodeKind};
    use std::collections::BTreeSet;
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.owner_says(t, &["fixer"], "@fixer use the sorted copy");
    let s = r.say("lead", &["lead"], "the parser");
    r.settle(10);
    assert!(r.log.iter().any(|l| matches!(l, Line::Handling { t: u, event, .. } if *u == s && event == "hole")), "the said ticket ran its hole");
    let (g, problems) = graph::of(&mut Machine::load(defs()), &r.log);
    assert!(problems.is_empty(), "{problems:?}");
    let ran = |from: &str| -> (BTreeSet<String>, BTreeSet<String>) {
        g.edges.iter().filter(|e| e.kind == EdgeKind::Ran && e.from == from).map(|e| e.to.clone()).partition(|to| to.starts_with("handler:"))
    };
    // the thread's runs: the Map's handlers for it, and each run's own ticket
    let v = r.m.view(t).unwrap();
    let map: BTreeSet<String> = v.ran.iter().filter_map(|x| x.from.as_ref().map(|k| format!("handler:{k}:{}", x.event))).collect();
    assert!(v.ran.len() >= 2 && map.contains("handler:base:owner_post"), "base's owner_post runs: {:?}", v.ran);
    let (handlers, runs) = ran(&r.m.id(t));
    // a hole the thread ran is no run of a `when` the view lists: it is the next check's
    assert_eq!(handlers.into_iter().filter(|h| !h.ends_with(":hole")).collect::<BTreeSet<_>>(), map);
    assert_eq!(runs, v.ran.iter().map(|x| x.id.clone()).collect());
    // the hole's, from base, as `a_hole_handlers_run_replays_under_the_program_it_ran` finds its program there
    assert!(r.m.tickets[&s].kind != "base");
    assert!(ran(&r.m.id(s)).0.contains("handler:base:hole"), "{:?}", ran(&r.m.id(s)));
    // and no handler named by anything but a `when` some ticket holds
    let whens: BTreeSet<String> = r.m.tickets.values().flat_map(|y| y.handlers.iter().map(|w| format!("handler:{}:{}", w.from, w.ev))).collect();
    let handlers: BTreeSet<&str> = g.nodes.iter().filter(|n| n.kind == NodeKind::Handler).map(|n| n.id.as_str()).collect();
    assert!(handlers.iter().all(|h| whens.contains(*h)), "{handlers:?} against {whens:?}");
    // a hole handled by a `when` the ticket does not hold: the graph says so at its line, and makes no handler for it
    let at = r.log.iter().rev().find_map(|l| if let Line::Handling { at, .. } = l { Some(at.clone()) } else { None }).unwrap();
    let mut b = graph::Builder::default();
    b.after(&r.m, 41, &[Line::Handling { at, t: s, event: "gone".into(), program: String::new() }]);
    assert_eq!(b.problems, [format!("line 42: {} handled `gone` with a `when gone` it does not hold, so the graph has no handler for it", r.m.id(s))]);
    assert!(!b.graph.nodes.iter().any(|n| n.kind == NodeKind::Handler) && !b.graph.edges.iter().any(|e| e.kind == EdgeKind::Ran), "{:?}", b.graph);
}

/// Each node and edge of the record graph has its line's own time (`Graph::date`, docket-edge5 62b3d7ee), given as the
/// Builder makes it (`Graph::date_from`), so coordd's graph, kept along each input, has it without the record read
/// again (proto-window N-1446 p1383: the window's "when" and its x axis): the whole graph undated and dated again
/// afterwards is the same, and a slice gives each edge as the graph has it.
#[test]
fn each_node_and_edge_of_the_record_graph_has_its_lines_time_as_it_is_made() {
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.owner_says(t, &["fixer"], "@fixer use the sorted copy");
    let (g, _) = graph::of(&mut Machine::load(defs()), &r.log);
    let at = |n: Option<usize>| serde_json::to_value(&r.log[n.unwrap() - 1]).unwrap()["at"].as_str().filter(|a| !a.is_empty()).map(str::to_string);
    assert!(g.edges.len() > 4 && g.edges.iter().all(|e| e.when.is_some() && e.when == at(e.line())), "{:?}", g.edges);
    assert!(g.nodes.iter().all(|n| n.when.is_some() && n.when == at(n.line())), "{:?}", g.nodes);
    let mut again = g.clone();
    again.nodes.iter_mut().for_each(|n| n.when = None);
    again.edges.iter_mut().for_each(|e| e.when = None);
    again.date(&r.log);
    assert_eq!((&again.nodes, &again.edges), (&g.nodes, &g.edges));
    let s = g.slice(&crate::graph_slice::Ask { around: Some(r.m.id(t)), hops: 1, ..Default::default() });
    assert!(!s.edges.is_empty() && s.edges.iter().all(|e| g.edges.contains(&e.edge)), "{:?}", s.edges);
}

/// His post reaches the run that handled it (paper-research N-1446 p1677, still 3, pick 1): each run of `when
/// owner_post` is a `handled_by` edge from the post it was opened for to its ticket, at its `opened` line, as
/// `TicketView::ran` lists the run with its post; his post that joined the burst a run gathers, from it to that run, at
/// its own `post` line. The edge is a call's, and the window's walk around his post goes into the run, to its steps
/// and their call files; the run reached from its thread (`ran`) stops the thread's walk, so a thread's slice does not
/// go into each of its runs. Made up.
#[test]
fn his_post_reaches_the_run_that_handled_it_and_the_walk_around_it_goes_into_the_run() {
    use crate::graph::EdgeKind;
    use crate::graph_slice::{Ask, Layer};
    use std::collections::BTreeSet;
    let mut r = Run::new();
    let t = r.two_on_a_thread();
    r.owner_says(t, &["fixer"], "@fixer use the sorted copy");
    r.owner_says(t, &["lead"], "go ahead");
    let held = r.m.tickets[&t].last();
    let h = r.handler().unwrap();
    r.route(json!({"head": "To", "agents": ["fixer"]}));
    // a burst: his second post joins the run that gathers his first, and has no run of its own
    r.m.owner_hold = Some(15);
    r.owner_says(t, &[], "the build: 36 fails on the runner");
    let a = r.m.tickets[&t].last();
    r.owner_says(t, &[], "and the brain: the ledger should mark p614 kept");
    let b = r.m.tickets[&t].last();
    let burst = r.handler_of(t, a);
    assert_eq!(r.m.tickets[&burst].post_of.as_ref().map(|p| p.posts()), Some(vec![a, b]));
    let Status::Awaits { owed, .. } = &r.m.tickets[&burst].status else { panic!("{:?}", r.m.tickets[&burst].status) };
    let step = owed.step;
    r.go(Input::Lapsed { t: burst, step, head: json!({"head": "Send"}), after: 15, because: "time".into() }).reply.unwrap();
    r.route_of(t, a, json!({"head": "To", "agents": ["lead"]}));
    let (g, problems) = graph::of(&mut Machine::load(defs()), &r.log);
    assert!(problems.is_empty(), "{problems:?}");
    let id = r.m.id(t);
    let post = |n: u32| format!("{id}.p{n}");
    let handled: BTreeSet<(String, String, usize)> = g.edges.iter().filter(|e| e.kind == EdgeKind::HandledBy).map(|e| (e.from.clone(), e.to.clone(), e.line().unwrap())).collect();
    // each run the thread's view lists, from its post, at the line that opened it
    let opened = |run: &str| r.log.iter().position(|l| matches!(l, Line::Opened { t: c, .. } if r.m.id(*c) == run)).expect("its opened line") + 1;
    let v = r.m.view(t).unwrap();
    let mut runs: BTreeSet<(String, String, usize)> = v.ran.iter().filter(|x| x.event == OWNER_POST).map(|x| (post(x.post.expect("its post")), x.id.clone(), opened(&x.id))).collect();
    assert_eq!(runs.len(), 4, "the thread's opening words, told, held, the burst: {:?}", v.ran);
    // and the burst's second post, at its own line
    let joined = r.log.iter().position(|l| matches!(l, Line::Post { t: u, text: Text::New(w), .. } if *u == t && w.starts_with("and the brain"))).unwrap() + 1;
    runs.insert((post(b), r.m.id(burst), joined));
    assert_eq!(handled, runs);
    // around the held post: the run that handled it, a call's, and past it its steps and their call files (`route`)
    let layers = BTreeSet::from([Layer::His, Layer::Calls]);
    let run = r.m.id(h);
    let s = g.slice(&Ask { around: Some(post(held)), hops: 3, layers: layers.clone(), ..Ask::default() });
    let got: BTreeSet<(String, EdgeKind, String)> = s.edges.iter().map(|e| (e.edge.from.clone(), e.edge.kind, e.edge.to.clone())).collect();
    assert!(s.edges.iter().any(|e| (e.edge.from == post(held), e.edge.kind, e.edge.to == run, e.layer) == (true, EdgeKind::HandledBy, true, Layer::Calls)), "{got:?}");
    let steps: Vec<String> = g.edges.iter().filter(|e| e.kind == EdgeKind::StepOf && e.to == run).map(|e| e.from.clone()).collect();
    assert!(!steps.is_empty() && steps.iter().all(|st| got.contains(&(st.clone(), EdgeKind::StepOf, run.clone()))), "{steps:?}: {got:?}");
    assert!(got.iter().any(|(f, k, to)| steps.contains(f) && *k == EdgeKind::CameFrom && to == "call:route"), "{got:?}");
    // around the thread: it ran the run, and the walk stops there
    let thread = g.slice(&Ask { around: Some(id.clone()), hops: 3, layers, ..Ask::default() });
    assert!(thread.edges.iter().any(|e| e.edge.kind == EdgeKind::Ran && e.edge.from == id && e.edge.to == run), "the thread ran it");
    let into: Vec<&str> = thread.edges.iter().filter(|e| e.edge.kind == EdgeKind::StepOf && e.edge.to == run).map(|e| e.edge.from.as_str()).collect();
    assert!(into.is_empty(), "{into:?}");
    r.replays();
}
