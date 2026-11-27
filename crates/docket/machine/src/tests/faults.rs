//! The engine faults N-101's stand-in run on docket-try found (docket-core, N-135.p3), each with its test.

use super::*;

/// On a slow thread: a `during`'s look halts on its hidden ticket, and the parent's view lists that ticket with its
/// halts (fault 1); `stop` writes the agent's line `stopped` (fault 5); a reminder halted for the implementer's step
/// goes once that step is answered (fault 2).
#[test]
fn on_a_slow_thread_a_look_shows_under_its_ticket_stop_is_recorded_and_a_stale_reminder_goes() {
    let (d, errors) = defs_with(&DURING_KINDS);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d.clone()), log: Vec::new(), clock: 0 };
    r.open_form("piece", json!({"task": "median"}));
    r.go(Input::Slow { t: 200, on: true, by: "owner".into() }).reply.unwrap();
    // the implementer's step went out at the open, before the switch
    r.take("f200-1");
    // its own check: the look's step halts on the hidden ticket, and the piece's view shows it
    act(&mut r, "f200-1", "check", "case 3 red").reply.unwrap();
    let v = r.m.view(200).unwrap();
    let look = v.handlers.iter().find(|h| h.kind == "act").unwrap_or_else(|| panic!("{:?}", v.handlers));
    assert!(matches!(&look.halts[..], [ViewHalt::Give { .. }]), "{:?}", look.halts);
    assert!(v.halted.contains(&look.id), "{:?}", v.halted);
    let lt = parse_ticket(&look.id).unwrap();
    assert!(lt >= HIDDEN);
    r.approve(lt, 1);
    r.take("f200-2");
    let out = r.go(Input::Answer { t: lt, by: "f200-2".into(), head: json!({"head": "Say", "text": "the median of an empty list"}), text: Text::New("as said".into()) });
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    let n = match r.halts(lt).as_slice() { [ViewHalt::Take { n, .. }] => *n, h => panic!("{h:?}") };
    let out = r.approve(lt, n);
    assert!(out.lines.iter().any(|l| matches!(l, Line::Agent { agent, what, .. } if agent == "f200-1" && what == "stopped")), "{:?}", out.lines);
    assert!(out.effects.contains(&Effect::Interrupt { agent: "f200-1".into() }));
    // the word, delivered; then a reminder of the implementer's step halts as a notice
    // (the post is the piece's now, so its delivery halts there: N-101.p64)
    let n = r.halts(200).iter().find_map(|h| if let ViewHalt::Deliver { n, .. } = h { Some(*n) } else { None }).unwrap_or_else(|| panic!("{:?} {:?}", r.halts(200), r.halts(lt)));
    r.approve(200, n);
    // the look's post is on the piece, by the piece, not on its hidden ticket (N-101.p64)
    let last = r.m.view(200).unwrap().posts.last().cloned().unwrap();
    assert_eq!((last.text.clone(), last.by.as_str()), (Text::New("the median of an empty list".into()), "P-200"), "{last:?}");
    r.take("f200-1");
    // its turn ends in words that answer nothing: a reminder of its step, halted as a notice
    r.go(Input::TurnEnded { agent: "f200-1".into(), delivery: None, text: Run::loc(2), later: vec![], stood: None, go: None });
    let out = r.go(Input::Formed { t: 200, step: 1, head: None, why: None, not_taken: None, call: None });
    assert!(out.lines.iter().any(|l| matches!(l, Line::Reminded { .. })), "{:?}", out.lines);
    assert!(r.halts(200).iter().any(|h| matches!(h, ViewHalt::Notice { delivery, .. } if delivery == "P-200.s1.again")), "{:?}", r.halts(200));
    // the implementer answers: once the answer is taken, no notice for its step stands
    r.go(Input::Answer { t: 200, by: "f200-1".into(), head: json!({"head": "Done", "c": "abc123"}), text: Text::New("done".into()) }).reply.unwrap();
    let take = r.halts(200).iter().find_map(|h| if let ViewHalt::Take { n, .. } = h { Some(*n) } else { None }).expect("the answer halts");
    r.approve(200, take);
    assert!(!r.halts(200).iter().any(|h| matches!(h, ViewHalt::Notice { delivery, .. } if delivery.starts_with("P-200.s"))), "{:?}", r.halts(200));
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.view(200), r.m.view(200));
}

const FIRED_KINDS: [(&str, &str); 1] = [(
    "cut.kind",
    "kind bit\n  letter Z\n  form task: Text\n  result Text\n  raises Nope\n  pattern\n    v <- step dev \"Do {{ task }}\" -> Done | Fail;\n    match v {\n      Done => return task\n      Fail => raise Nope(\"n\")\n    }\n\nkind batch\n  letter Y\n  form cut: Text\n  result Text\n  raises Stalled\n  pattern\n    v <- step runner \"Cases on {{ cut }}\" -> Green;\n    return cut\n\nwhen cut-build is build open batch(\"line\")\n",
)];

/// The `when` facts count closes that ended done, and failed ones apart (fault 3); a ticket a binding opens is slow
/// when what made it fire came from a slow thread (fault 4).
#[test]
fn when_counts_closes_done_and_a_binding_opened_from_a_slow_thread_halts() {
    let (d, errors) = defs_with(&FIRED_KINDS);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d.clone()), log: Vec::new(), clock: 0 };
    let open = |r: &mut Run, task: &str| r.go(Input::Open { kind: "bit".into(), under: None, opener: "owner".into(), form: json!({"task": task}), text: None, names: vec![], told: vec![] }).reply.unwrap();
    // a bit that fails: not counted as closed
    open(&mut r, "a");
    r.take("dev");
    r.answer(200, "dev", json!({"head": "Fail"}));
    let f = &r.m.when_asks("2026-11-10T12:05:00Z")[0].3;
    assert!(!f.contains_key("closed-bit") && f.get("failed-bit").map(String::as_str) == Some("1"), "{f:?}");
    // a bit on a slow thread that ends done
    open(&mut r, "b");
    r.go(Input::Slow { t: 201, on: true, by: "owner".into() }).reply.unwrap();
    r.take("dev");
    r.go(Input::Answer { t: 201, by: "dev".into(), head: json!({"head": "Done"}), text: Text::New(String::new()) }).reply.unwrap();
    let n = match r.halts(201).as_slice() { [ViewHalt::Take { n, .. }] => *n, h => panic!("{h:?}") };
    r.approve(201, n);
    let (text, f) = { let a = &r.m.when_asks("2026-11-10T12:06:00Z")[0]; (a.0.clone(), a.3.clone()) };
    assert_eq!(f.get("closed-bit").map(String::as_str), Some("1"), "{f:?}");
    // the batch it fires is slow: its first step halts
    let out = r.go(Input::Event { name: "when".into(), value: json!({"binding": text, "answer": "build"}) });
    assert_eq!(out.reply, Ok("Opened Y-202.".into()));
    assert!(r.m.slow(202));
    assert!(matches!(r.halts(202).as_slice(), [ViewHalt::Give { .. }]), "{:?}", r.halts(202));
    // and a batch fired by closes on no slow thread is not
    open(&mut r, "c");
    r.take("dev");
    r.answer(203, "dev", json!({"head": "Done"}));
    let out = r.go(Input::Event { name: "when".into(), value: json!({"binding": text, "answer": "build"}) });
    assert_eq!(out.reply, Ok("Opened Y-204.".into()));
    assert!(!r.m.slow(204));
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.view(202), r.m.view(202));
}

/// A Ref typed on a command line as JSON keeps its commits by repository (N-101.p64, docket-core5's step 9 run).
#[test]
fn a_ref_typed_as_json_stays_commits_by_repository() {
    let v = crate::types::Type::Ref.coerce(&json!("{\"a\": \"8f28eb2\", \"b\": \"2bcc31b\"}")).unwrap();
    assert_eq!(v, json!({"a": "8f28eb2", "b": "2bcc31b"}));
    assert!(crate::types::Type::Ref.coerce(&json!("{\"a\": 3}")).is_err());
    assert_eq!(crate::types::Type::Ref.coerce(&json!("a=8f28eb2,b=2bcc31b")).unwrap(), v);
}

const LOOK_POSTS: [(&str, &str); 1] = [(
    "watch.kind",
    "kind watch\n  letter P\n  form task: Text\n  result Text\n  pattern\n    let agent a in {\n      r <- during {\n        step a \"Do {{ task }}\" -> Done(c: Text)\n      } when act(u) where u.by == a do {\n        post u.text\n      };\n      match r { Done(c) => return c }\n    }\n",
)];

/// A look's post is on the ticket it belongs to (N-101.p64); a record made before that, with the post on the look's
/// hidden ticket by that ticket, replays as it was, with nothing broken (N-170).
#[test]
fn a_look_post_from_an_old_record_replays_where_the_record_has_it() {
    let (d, errors) = defs_with(&LOOK_POSTS);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d.clone()), log: Vec::new(), clock: 0 };
    r.open_form("watch", json!({"task": "t"}));
    r.take("f200-1");
    act(&mut r, "f200-1", "edit", "wrote x").reply.unwrap();
    let posted: Vec<(u64, String)> = r.log.iter().filter_map(|l| if let Line::Posted { t, by, .. } = l { Some((*t, by.clone())) } else { None }).collect();
    assert_eq!(posted, vec![(200, "P-200".to_string())], "now: on the watch, by the watch");
    // the same record as the machine before N-101.p64 wrote it: the post on the hidden ticket, by it
    let hidden = r.log.iter().find_map(|l| if let Line::Opened { t, parent: 200, .. } = l { Some(*t) } else { None }).expect("the look's ticket");
    let old: Vec<Line> = r.log.iter().cloned().map(|l| match l {
        Line::Posted { at, t: 200, n: _, by: _, step, .. } => Line::Posted { at, t: hidden, n: 1, by: format!("A-{hidden}"), step, names: vec![], inform: vec![], into_turn: vec![] },
        l => l,
    }).collect();
    let mut m2 = Machine::load(d.clone());
    assert_eq!(m2.replay(&old), Vec::<String>::new());
    assert!(m2.view(200).unwrap().broken.is_none());
    let mut m3 = Machine::load(d);
    assert_eq!(m3.replay(&r.log), Vec::<String>::new());
}
