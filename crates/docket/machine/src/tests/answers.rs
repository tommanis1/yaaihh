//! docket-core3's items (N-101): the library and a project's program loaded with every failing line reported;
//! answers in parts, extra parts, `text of` and `by of`.

use super::*;
use std::collections::BTreeMap;

const CALLS: &str = "call design(words: Text) -> Design(asked: Text, today: Text) | Ask(question: Text)\n  task design\n";

fn build(kinds: &str) -> (Defs, Vec<String>) {
    let mut named = BTreeMap::new();
    let calls: BTreeMap<String, CallSig> = syntax::parse_calls("t.call", CALLS, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
    Defs::build(&[("t.kind".to_string(), kinds.to_string())], calls, named, 1)
}

/// Each error of a kind is its own line, and checking goes on past it: two unknown calls in a row, and a bad arm
/// beside a good one, are three lines; what only follows from an error is not reported again.
#[test]
fn a_kind_gives_every_error_it_has() {
    let k = "kind k\n  form w: Text\n  result Text\n  pattern\n    a <- step fresh nowhere(w);\n    b <- step fresh elsewhere(a);\n    d <- step fresh design(w);\n    match d {\n      Design(x, y) => step fresh third(x)\n      Ask(q) => return q\n    }\n";
    let (defs, errors) = build(k);
    assert!(!defs.kinds.contains_key("k"));
    assert_eq!(errors.len(), 3, "{errors:#?}");
    assert!(errors[0].contains("line 5") && errors[0].contains("no call `nowhere`"), "{errors:#?}");
    assert!(errors[1].contains("line 6") && errors[1].contains("no call `elsewhere`"), "{errors:#?}");
    assert!(errors[2].contains("line 9") && errors[2].contains("no call `third`"), "{errors:#?}");
}

/// A name bound by a failed statement is unknown, and matching on it reports nothing more; the arms are still checked.
#[test]
fn an_error_does_not_cascade() {
    let k = "kind k\n  form w: Text\n  result Text\n  pattern\n    d <- step fresh nowhere(w);\n    match d {\n      Design(x, y) => return x\n      Other(q) => step fresh third(q)\n    }\n";
    let (_, errors) = build(k);
    assert_eq!(errors.len(), 2, "{errors:#?}");
    assert!(errors[0].contains("no call `nowhere`") && errors[1].contains("no call `third`"), "{errors:#?}");
}

/// A kind that does not parse costs only itself: the next kind in the file loads.
#[test]
fn a_kind_that_does_not_parse_costs_only_itself() {
    let k = "kind bad\n  result Text\n  pattern\n    x <- once upon a time;\n    return x\n\nkind good\n  form w: Text\n  result Text\n  pattern\n    return w\n";
    let (defs, errors) = build(k);
    assert!(defs.kinds.contains_key("good"), "{errors:#?}");
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].contains("t.kind"), "{errors:#?}");
}

/// A kind with no error loads as before, and a kind's first error is still what `check_program` gives.
#[test]
fn a_good_kind_still_loads() {
    let k = "kind k\n  form w: Text\n  result Text\n  pattern\n    d <- step fresh design(w);\n    match d {\n      Design(x, y) => return x\n      Ask(q) => return q\n    }\n";
    let (defs, errors) = build(k);
    assert!(errors.is_empty(), "{errors:#?}");
    assert!(defs.kinds.contains_key("k"));
}

/// A later folder's call replaces an earlier one's only with the same parameters and an answer that extends it:
/// more parts after the earlier's are allowed, fewer constructors too; another parameter list is refused with its
/// line, and the earlier call stands.
#[test]
fn a_call_is_replaced_by_one_that_extends_it() {
    let lib = ("lib/lib.call".to_string(), "call design(words: Text) -> Design(asked: Text, today: Text) | Ask(question: Text)\n  task design\ncall plan(p: Text) -> Plan(how: Text)\n  task plan\n".to_string());
    let own = ("own/own.call".to_string(), "call design(words: Text) -> Design(asked: Text, today: Text, meaning: Text)\n  task design\ncall plan(p: Text, part: Text) -> Plan(how: Text)\n  task plan\n".to_string());
    let (calls, errors) = syntax::parse_call_files(&[lib, own], &mut BTreeMap::new());
    assert_eq!(calls["design"].file, "own/own.call", "{errors:#?}");
    assert_eq!(calls["plan"].file, "lib/lib.call");
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].starts_with("own/own.call:3: call plan does not replace lib/lib.call:3's: it takes (p: Text, part: Text), that one (p: Text)"), "{errors:#?}");
    let narrow = ("own/n.call".to_string(), "call design(words: Text) -> Design(asked: Text) | Ask(question: Text)\n  task design\n".to_string());
    let lib = ("lib/lib.call".to_string(), "call design(words: Text) -> Design(asked: Text, today: Text) | Ask(question: Text)\n  task design\n".to_string());
    let (calls, errors) = syntax::parse_call_files(&[lib, narrow], &mut BTreeMap::new());
    assert_eq!(calls["design"].file, "lib/lib.call");
    assert!(errors[0].contains("its Design(asked: Text) does not start with that one's Design(asked: Text, today: Text)"), "{errors:#?}");
}

/// The library's kind matches the first parts of a replaced call's answer; the parts after are not bound, and a
/// value carrying them matches.
#[test]
fn a_kind_matches_the_parts_it_names() {
    let lib = ("lib/lib.call".to_string(), "call design(words: Text) -> Design(asked: Text, today: Text) | Ask(question: Text)\n  task design\n".to_string());
    let own = ("own/own.call".to_string(), "call design(words: Text) -> Design(asked: Text, today: Text, meaning: Text) | Ask(question: Text)\n  task design\n".to_string());
    let (calls, errors) = syntax::parse_call_files(&[lib, own], &mut BTreeMap::new());
    assert!(errors.is_empty(), "{errors:#?}");
    let calls: BTreeMap<String, CallSig> = calls.into_iter().map(|(n, c)| (n, c.sig)).collect();
    let k = "kind change\n  form w: Text\n  result Text\n  pattern\n    d <- step fresh design(w);\n    match d {\n      Design(asked, today) => return today\n      Ask(q) => return q\n    }\n";
    let (defs, errors) = Defs::build(&[("lib/lib.kind".to_string(), k.to_string())], calls.clone(), BTreeMap::new(), 1);
    assert!(errors.is_empty(), "{errors:#?}");
    let syntax::Prog::Bind { rest, .. } = &defs.kinds["change"].pattern else { panic!() };
    let syntax::Prog::Match { arms, .. } = rest.as_ref() else { panic!() };
    let mut env = BTreeMap::new();
    let v = serde_json::json!({"head": "Design", "asked": "a", "today": "t", "meaning": "m"});
    assert!(machine::tests_matches(&arms[0].0, &v, &mut env));
    assert_eq!(env.get("today"), Some(&serde_json::json!("t")));
    assert!(!env.contains_key("meaning"));
    // more names than fields is still refused
    let k = k.replace("Design(asked, today)", "Design(asked, today, meaning, more)");
    let (_, errors) = Defs::build(&[("lib/lib.kind".to_string(), k)], calls, BTreeMap::new(), 1);
    assert!(errors.iter().any(|e| e.contains("Design has 3 fields, the pattern names 4")), "{errors:#?}");
}

/// A call that does not parse costs only itself: the file's other calls load, their lines as written.
#[test]
fn a_call_that_does_not_parse_costs_only_itself() {
    let f = ("c.call".to_string(), "type P {\n  a: Text\n}\n\ncall one(x: Text) -> A | B\n  task t\n  wonders yes\n  template\n    {{ x }}\n\n// between\ncall two(p: P) -> C(why: Text)\n  task t\n".to_string());
    let (calls, errors) = syntax::parse_call_files(&[f], &mut BTreeMap::new());
    assert_eq!(calls.keys().collect::<Vec<_>>(), vec!["two"], "{errors:#?}");
    assert_eq!(calls["two"].line, 12);
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].starts_with("c.call:7: a call's lines after its signature"), "{errors:#?}");
}

/// `text of d` and `by of d` check on an answer, not on a text; they print as written.
#[test]
fn text_of_and_by_of_check_on_answers() {
    let k = "kind k\n  form w: Text\n  result Text\n  pattern\n    d <- step fresh design(w);\n    a <- step by of d design(text of d);\n    return text of a\n";
    let (defs, errors) = build(k);
    assert!(errors.is_empty(), "{errors:#?}");
    assert!(print::prog(&defs.kinds["k"].pattern, 0).contains("step by of d design(text of d)"), "{}", print::prog(&defs.kinds["k"].pattern, 0));
    let (_, errors) = build(&k.replace("design(text of d)", "design(text of w)"));
    assert!(errors.iter().any(|e| e.contains("`text of` takes an answer (a record or variant), and this is a Text")), "{errors:#?}");
}

/// The value the program goes on with carries who answered and the whole answer; what leaves the engine does not.
#[test]
fn an_answer_carries_its_agent_and_its_text() {
    let text = Text::Loc { loc: Loc { file: "/t.jsonl".into(), offset: 3, len: 40 } };
    let v = crate::answers::carried(serde_json::json!({"head": "Design", "asked": "a", "today": "t"}), "f7-1", &text);
    assert_eq!(crate::answers::by_of(&v), serde_json::json!("f7-1"));
    assert_eq!(crate::answers::text_of(&v, None), text.value());
    assert_eq!(crate::answers::plain(serde_json::json!([v.clone(), {"x": v}])), serde_json::json!([{"head": "Design", "asked": "a", "today": "t"}, {"x": {"head": "Design", "asked": "a", "today": "t"}}]));
    // a value no step gave: rendered from its fields in order, under their headings; who gave it: none
    let ty = Type::Variant(vec![("Design".into(), vec![("asked".into(), Type::Text), ("stands_on".into(), Type::List(Box::new(Type::Text)))])]);
    let built = serde_json::json!({"head": "Design", "stands_on": ["A", "B"], "asked": "do x"});
    assert_eq!(crate::answers::text_of(&built, Some(&ty)), serde_json::json!("Asked\ndo x\n\nStands on\nA, B"));
    assert_eq!(crate::answers::by_of(&built), serde_json::Value::Null);
    // a text held by reference stays where it is: the whole is the new words and that place, joined
    let held = serde_json::json!({"head": "Design", "asked": {"loc": {"file": "/t", "offset": 0, "len": 1}}, "stands_on": []});
    let place = Text::Loc { loc: Loc { file: "/t".into(), offset: 0, len: 1 } };
    assert_eq!(crate::answers::text_of(&held, Some(&ty)), Text::Joined { joined: vec![Text::New("Asked\n".into()), place, Text::New("\n\nStands on".into())] }.value());
    // a step that answers a text (`-> Text`): the reference to the output stays one, nothing added to it
    assert_eq!(crate::answers::carried(text.value(), "f7-2", &text), text.value());
}

/// End to end: the designer's answer goes on with who gave it and the whole of it; the next step goes to that agent
/// (`step by of d`), and the ticket closes with the answer's text; the record carries neither.
#[test]
fn the_next_step_goes_to_the_agent_that_answered() {
    let k = "kind t\n  form w: Text\n  result Text\n  pattern\n    d <- step fresh \"Design {{ w }}\" -> Design(asked: Text, today: Text);\n    e <- step by of d \"Again: {{ w }}\" -> Fine(why: Text);\n    return text of d\n";
    let (d, errors) = defs_with(&[("t.kind", k)]);
    assert!(errors.is_empty(), "{errors:?}");
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    r.open_form("t", serde_json::json!({"w": "x"}));
    let designer = match r.status(200) { ViewStatus::Awaits { agent, .. } => agent, s => panic!("{s:?}") };
    r.go(Input::Answer { t: 200, by: designer.clone(), head: serde_json::json!({"head": "Design", "asked": "a", "today": "t"}), text: Text::New("Asked\na\nToday\nt".into()) }).reply.unwrap();
    match r.status(200) {
        ViewStatus::Awaits { agent, .. } => assert_eq!(agent, designer),
        s => panic!("{s:?}"),
    }
    r.go(Input::Answer { t: 200, by: designer.clone(), head: serde_json::json!({"head": "Fine", "why": "ok"}), text: Text::New("Fine".into()) }).reply.unwrap();
    assert_eq!(r.status(200), ViewStatus::Done { result: serde_json::json!("Asked\na\nToday\nt") });
    let record = serde_json::to_string(&r.log).unwrap();
    assert!(!record.contains("@by") && !record.contains("@text"), "{record}");
    // replay makes them again in accept: the same outputs
    let (d, _) = defs_with(&[("t.kind", k)]);
    assert_eq!(Machine::load(d).replay(&r.log), Vec::<String>::new());
}

/// A hand-up's `text of {log = log, found = found, ...}` where `found` is an agent's answer held in its session
/// (docket-core3, N-101: run 1's H-14 raised NoResult through C-13, M-10 and B-9): the text goes on, its headings new
/// words and `found` where it is held; nothing is raised. A record from before, which has the NoResult, replays so.
#[test]
fn text_of_a_value_holding_a_text_by_reference_goes_on_and_an_old_noresult_replays() {
    let k = "kind h\n  form log: Text, found: Text\n  result Text\n  pattern\n    return text of {log = log, found = found}\n";
    let (d, errors) = defs_with(&[("h.kind", k)]);
    assert!(errors.is_empty(), "{errors:?}");
    let place = Text::Loc { loc: Loc { file: "/session.jsonl".into(), offset: 122187, len: 2848 } };
    let mut r = Run { m: Machine::load(d), log: Vec::new(), clock: 0 };
    // it closes at once, so it names who reads it
    let form = serde_json::json!({"log": "/builds/case-4.log", "found": place.value()});
    r.go(Input::Open { kind: "h".into(), under: None, opener: "owner".into(), form, text: None, names: vec!["orch".into()], told: vec![] }).reply.unwrap();
    let joined = Text::Joined { joined: vec![Text::New("Log\n/builds/case-4.log\n\nFound\n".into()), place.clone()] };
    assert_eq!(r.status(200), ViewStatus::Done { result: joined.value() });
    let (d, _) = defs_with(&[("h.kind", k)]);
    assert_eq!(Machine::load(d).replay(&r.log), Vec::<String>::new());
    // the record as the engine wrote it before: the same open, closed with NoResult
    let old: Vec<Line> = r
        .log
        .iter()
        .map(|l| match l {
            Line::Closed { at, t, .. } => Line::Closed { at: at.clone(), t: *t, result: None, raised: Some(Raised { exc: "NoResult".into(), value: serde_json::json!({"head": "NoResult", "why": "`text of {log = log, found = found}`: no step gave this value"}) }) },
            l => l.clone(),
        })
        .collect();
    let (d, _) = defs_with(&[("h.kind", k)]);
    let mut m = Machine::load(d);
    assert_eq!(m.replay(&old), Vec::<String>::new());
    assert!(matches!(m.view(200).unwrap().status, ViewStatus::Failed { .. }), "{:?}", m.view(200).unwrap().status);
}

/// An answer given with its fields and no text (the owner's, in the app: the dry run's design, paper-research N-101
/// p1008, 1): `text of` it is its fields, as the step would render them, not the empty text; and a post with no words
/// is not shown in a step as "NAME wrote:" over nothing.
#[test]
fn text_of_an_answer_with_fields_and_no_text_is_its_fields() {
    let ty = Type::Variant(vec![("Design".into(), vec![("asked".into(), Type::Text), ("stands_on".into(), Type::List(Box::new(Type::Text)))])]);
    let v = crate::answers::carried(serde_json::json!({"head": "Design", "asked": "do x", "stands_on": ["A"]}), "owner", &Text::New(String::new()));
    assert_eq!(crate::answers::text_of(&v, Some(&ty)), serde_json::json!("Asked\ndo x\n\nStands on\nA"));
    let mut r = super::Run::new();
    let c = r.open_claim();
    r.go(Input::Post { t: c, by: "owner".into(), text: Text::New(String::new()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None });
    r.go(Input::Post { t: c, by: "owner".into(), text: Text::New("see line 201".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).reply.unwrap();
    r.decide(c, 1, "orch");
    let d = r.take("orch");
    let What::Step { unread, .. } = &d.what else { panic!("{d:?}") };
    assert_eq!(unread.iter().map(|p| p.text.clone()).collect::<Vec<_>>(), vec![Text::New("see line 201".into())]);
}
