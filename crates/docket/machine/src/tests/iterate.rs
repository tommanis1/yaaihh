//! An `iterate` with no `up to` (N-101.p33, tovrin-writer on N-143): as many rounds as it takes, each waiting on a
//! step; never Exhausted; its bound is per round.

use super::*;
use crate::syntax::Prog;

const WAIT: &str = "kind waiting\n  letter W\n  form task: Text\n  result Text\n  raises Stalled\n  pattern\n    iterate seen = \"none\" {\n      g <- step runner \"Built green? {{ seen }}\" -> Green(top: Text) | Red(why: Text);\n      match g {\n        Green(top) => return Done(top)\n        Red(why) => return Again(why)\n      }\n    }\n";

/// It parses and prints without `up to`, checks with no Exhausted to raise, its bound is one round's, and it goes round
/// more often than any count would allow; replay gives the same view.
#[test]
fn an_iterate_with_no_bound_goes_round_until_done_and_is_never_exhausted() {
    let (d, errors) = defs_with(&[("waiting.kind", WAIT)]);
    assert!(errors.is_empty(), "{errors:?}");
    let k = &d.kinds["waiting"];
    assert!(matches!(&k.pattern, Prog::Iterate { n, .. } if *n == syntax::UNBOUNDED));
    let text = print::prog(&k.pattern, 0);
    assert!(text.starts_with("iterate seen = \"none\" {"), "{text}");
    assert_eq!(check::bound(&k.pattern, &d.kinds), 1);
    assert!(check::rounds_unbounded(&k.pattern));
    let mut r = Run { m: Machine::load(d.clone()), log: Vec::new(), clock: 0 };
    r.open_form("waiting", json!({"task": "t"}));
    for i in 0..1200 {
        let out = r.go(Input::Answer { t: 200, by: "runner".into(), head: json!({"head": "Red", "why": format!("red {i}")}), text: Text::New(String::new()) });
        assert!(out.reply.is_ok(), "{:?}", out.reply);
    }
    assert!(!matches!(r.m.view(200).unwrap().status, ViewStatus::Failed { .. } | ViewStatus::Done { .. }));
    r.go(Input::Answer { t: 200, by: "runner".into(), head: json!({"head": "Green", "top": "abc"}), text: Text::New(String::new()) }).reply.unwrap();
    assert!(matches!(r.m.view(200).unwrap().status, ViewStatus::Done { .. }), "{:?}", r.m.view(200).unwrap().status);
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.view(200), r.m.view(200));
}

/// A round that can go Again before any step, decision or open is refused; one that only ends Done without waiting
/// is not. A kind whose
/// `raises` leaves out Exhausted is accepted (WAIT above).
#[test]
fn an_iterate_with_no_bound_must_wait_before_each_again() {
    let refused = |src: &str| {
        let (_, errors) = defs_with(&[("w.kind", src)]);
        errors
    };
    let no_wait = "kind w\n  result Text\n  raises Stalled\n  pattern\n    iterate s = \"a\" {\n      return Again(s)\n    }\n";
    let e = refused(no_wait);
    assert!(e.iter().any(|e| e.contains("must wait on a step in each round")), "{e:?}");
    let late = "kind w\n  result Text\n  raises Stalled\n  pattern\n    v <- step runner \"First\" -> A | B;\n    iterate s = \"a\" {\n      match v {\n        A => return Again(s)\n        B => (g <- step runner \"Go\" -> Ok; return Done(s))\n      }\n    }\n";
    let e = refused(late);
    assert!(e.iter().any(|e| e.contains("must wait on a step in each round")), "{e:?}");
    let fine = "kind w\n  result Text\n  raises Stalled\n  pattern\n    v <- step runner \"First\" -> A | B;\n    iterate s = \"a\" {\n      match v {\n        A => return Done(s)\n        B => (g <- step runner \"Go\" -> Ok; return Again(s))\n      }\n    }\n";
    assert!(refused(fine).is_empty(), "{:?}", refused(fine));
}

/// Defs from agent-base with the call file EXTRA (its `type` lines) and the kind file KIND.
fn defs_typed(extra: &str, kind: &str) -> (Defs, Vec<String>) {
    let mut named = BTreeMap::new();
    let extra = ("w.call", extra);
    let calls: BTreeMap<String, CallSig> = CALLS.iter().chain([&extra]).flat_map(|(f, t)| syntax::parse_calls(f, t, &mut named).unwrap()).map(|c| (c.name.clone(), c)).collect();
    let mut files: Vec<(String, String)> = KINDS.iter().map(|(f, t)| (f.to_string(), t.to_string())).collect();
    files.push(("w.kind".into(), kind.into()));
    Defs::build(&files, calls, named, 200)
}

const KEPT: &str = "kind kept\n  letter K\n  form c: Text\n  result Text\n  raises Stalled\n  pattern\n    w <- iterate seen = \"none\" {\n      g <- step runner \"Built green? {{ seen }}\" -> Green(top: Text) | Red(why: Text);\n      match g {\n        Green(top) => return Done(OnLine(top))\n        Red(why) => return Again(why)\n      }\n    };\n    match w {\n      OnLine(top) => return top\n      Own(why) => return why\n    }\n";

/// A loop whose value is kept in a name (`w <- iterate ...`) takes its type from the declared type with the head it
/// answers `Done(..)` with, as a match does (N-101.p51); with no such type, or the head in two, it is refused and says
/// which.
#[test]
fn a_kept_iterate_takes_its_type_from_the_declared_type_of_its_done_head() {
    let (d, errors) = defs_typed("type Waited = OnLine(top: Text) | Own(why: Text)\n", KEPT);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d.clone()), log: Vec::new(), clock: 0 };
    r.open_form("kept", json!({"c": "abc"}));
    r.answer(200, "runner", json!({"head": "Red", "why": "another part"}));
    r.answer(200, "runner", json!({"head": "Green", "top": "t9"}));
    assert!(matches!(r.m.view(200).unwrap().status, ViewStatus::Done { .. }), "{:?}", r.m.view(200).unwrap().status);
    let mut m2 = Machine::load(d);
    assert_eq!(m2.replay(&r.log), Vec::<String>::new());
    assert_eq!(m2.view(200), r.m.view(200));
    let (_, errors) = defs_typed("", KEPT);
    assert!(errors.iter().any(|e| e.contains("no declared type has OnLine")), "{errors:?}");
    let (_, errors) = defs_typed("type Waited = OnLine(top: Text) | Own(why: Text)\ntype Other = OnLine(top: Text) | Gone\n", KEPT);
    assert!(errors.iter().any(|e| e.contains("more than one declared type") && e.contains("Waited") && e.contains("Other")), "{errors:?}");
}
