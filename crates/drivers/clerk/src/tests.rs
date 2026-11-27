use super::*;
use docket_v2::check::names::Loaded;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// A made-up module with a call, a sum with a key-typed field, and a record.
const TEXT: &str = r#"module t

Fact Fault Identified by String
type Verdict = Clear | Faulty(fault: Fault, remedy: String)
type Mark { node: String, n: Int }

Call Check(thing: String, cond: String, marks: List<Mark>) -> Verdict
  Template """
  Look at {{ thing }} for "{{ cond }}"; marks so far: {{ marks }}
  """
Call Marks(thing: String) -> List<Mark>
  Template """
  Mark {{ thing }}.
  """
"#;

fn loaded(texts: &[(&str, String)]) -> Loaded {
    let modules = texts.iter().map(|(p, t)| docket_v2::syntax::read(t).unwrap_or_else(|e| panic!("{p}: {e}"))).collect();
    docket_v2::check::load(modules).unwrap()
}

fn clerk(syms: &BTreeMap<String, Sym>, command: &str, seconds: u64) -> Clerk {
    Clerk::new(Config { command: vec![command.into()], at_once: 2, seconds }, syms).unwrap()
}

fn prompt(name: &str, args: Vec<Value>) -> Value {
    Value::Prompt { name: name.into(), args }
}

/// A stand-in model: a script in a scratch folder of its own, never a model of his.
fn script(name: &str, body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("clerk-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("model");
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

/// The answers the clerk gives within `secs`, waiting for `n` of them.
fn answers(c: &mut Clerk, n: usize, secs: u64) -> Vec<(u64, Reply)> {
    let (end, mut got) = (Instant::now() + Duration::from_secs(secs), Vec::new());
    while got.len() < n && Instant::now() < end {
        got.extend(c.ready().0);
        std::thread::sleep(Duration::from_millis(20));
    }
    got
}

fn text_of(r: Reply) -> String {
    match r {
        Reply::Answer { value: Value::Text(t), .. } => t,
        r => panic!("{r:?}"),
    }
}

fn read(c: &mut Clerk, call: &str, text: &str) -> Reply {
    c.carry(READ_AS, &[prompt(call, Vec::new()), Value::text(text)], 1)
}

fn mark(node: &str, n: i64) -> Value {
    Value::Record { ty: "t.Mark".into(), fields: vec![("node".into(), Value::text(node)), ("n".into(), Value::Int(n))] }
}

#[test]
fn a_calls_template_is_filled_from_its_fields() {
    let l = loaded(&[("t", TEXT.into())]);
    let mut c = clerk(&l.syms, "/bin/true", 5);
    let call = prompt("t.Check", vec![Value::text("the map"), Value::text("two levels"), Value::List(vec![mark("a", 1)])]);
    let t = text_of(c.carry(CALL_TEXT, &[call], 1));
    assert!(t.contains(r#"Look at the map for "two levels"; marks so far: "#), "{t}");
    assert!(t.contains(&Value::List(vec![mark("a", 1)]).to_string()), "a value that is no text as the engine writes it: {t}");
    let Reply::Failed(why) = c.carry(CALL_TEXT, &[prompt("t.Check", vec![Value::text("x")])], 2) else { panic!("one value for three fields") };
    assert!(why.contains("has 3 fields and was given 1"), "{why}");
    assert!(matches!(c.carry(CALL_TEXT, &[prompt("t.Nothing", vec![])], 3), Reply::Failed(w) if w.contains("no call")));
}

#[test]
fn a_models_text_is_read_as_the_calls_type_or_says_why_not() {
    let l = loaded(&[("t", TEXT.into())]);
    let mut c = clerk(&l.syms, "/bin/true", 5);
    let value = |r: Reply| match r {
        Reply::Answer { value, .. } => value,
        r => panic!("{r:?}"),
    };
    assert_eq!(value(read(&mut c, "t.Check", " Clear\n")), Value::ctor("t.Clear", vec![]));
    let faulty = Value::ctor("t.Faulty", vec![Value::text("more"), Value::text("say less")]);
    assert_eq!(value(read(&mut c, "t.Check", r#"Faulty(more, "say less")"#)), faulty, "a key written bare is its text");
    assert_eq!(value(read(&mut c, "t.Check", "```\nFaulty(\"more\", \"say less\")\n```")), faulty, "alone in one fence");
    assert_eq!(value(read(&mut c, "t.Marks", r#"[Mark("a", 2), Mark("b", -1)]"#)), Value::List(vec![mark("a", 2), mark("b", -1)]));
    for (text, says) in [
        ("Maybe", "Maybe is no alternative of t.Verdict: t.Clear, t.Faulty"),
        ("It looks clear to me.", "not one value as the texts write it"),
        ("Clear; Clear", "names no alternative of t.Verdict"),
        (r#"Faulty(more)"#, "t.Faulty has 2 fields and was written with 1"),
        (r#"[Mark("a", "two")]"#, "is not written as a value of Int"),
        (r#"Faulty(more, "{{ x }}")"#, "has a hole"),
    ] {
        let Reply::Failed(why) = read(&mut c, if text.starts_with('[') { "t.Marks" } else { "t.Check" }, text) else { panic!("{text} read") };
        assert!(why.contains(says), "{text}: {why}");
    }
}

#[test]
fn a_model_is_asked_by_the_command_with_the_partys_name_and_answers_later() {
    let l = loaded(&[("t", TEXT.into())]);
    let ok = script("ok", r#"printf '{"answer": "%s:%s", "tokens": 12}' "$1" "$(cat)""#);
    let mut c = clerk(&l.syms, ok.to_str().unwrap(), 5);
    let party = Value::Inst(Instance { ty: "std.hooks.Party".into(), args: vec![Value::text("w1")] });
    assert_eq!(c.carry(MODEL, &[party, Value::text("Mark the map.")], 7), Reply::Later);
    assert_eq!(c.carry(MODEL, &[Value::text("w2"), Value::text("And this.")], 8), Reply::Later, "a program's party is its text");
    let mut got = answers(&mut c, 2, 10);
    got.sort_by_key(|(l, _)| *l);
    let said = |r: &Reply| match r {
        Reply::Answer { value: Value::Text(t), costs, .. } => (t.clone(), costs.clone()),
        r => panic!("{r:?}"),
    };
    assert_eq!(got.iter().map(|(l, r)| (*l, said(r).0)).collect::<Vec<_>>(), [(7, "w1:Mark the map.".into()), (8, "w2:And this.".into())]);
    for (_, r) in &got {
        let costs = said(r).1;
        assert_eq!((&costs[0], &costs[1].0), (&("tokens".to_string(), 12), &"wall_ms".to_string()), "both measures, on every answer");
    }
    let _ = std::fs::remove_dir_all(ok.parent().unwrap());
}

#[test]
fn a_model_that_fails_is_too_slow_or_is_no_longer_wanted_gives_no_answer() {
    let l = loaded(&[("t", TEXT.into())]);
    let bad = script("bad", "echo 'the route refused it' >&2; exit 3");
    let mut c = clerk(&l.syms, bad.to_str().unwrap(), 5);
    c.carry(MODEL, &[Value::text("w1"), Value::text("x")], 1);
    let got = answers(&mut c, 1, 10);
    assert!(matches!(&got[..], [(1, Reply::Failed(w))] if w.contains("w1's model") && w.ends_with("the route refused it")), "{got:?}");
    let slow = script("slow", "sleep 30 & wait");
    let mut c = clerk(&l.syms, slow.to_str().unwrap(), 1);
    let t0 = Instant::now();
    c.carry(MODEL, &[Value::text("w1"), Value::text("x")], 2);
    let got = answers(&mut c, 1, 10);
    assert!(matches!(&got[..], [(2, Reply::Failed(w))] if w.contains("no answer from w1's model in 1 s")), "{got:?}");
    assert!(t0.elapsed() < Duration::from_secs(10), "the sleep it left ended with it, so its pipes closed");
    c.carry(MODEL, &[Value::text("w1"), Value::text("x")], 3);
    c.dropped(3);
    assert!(answers(&mut c, 1, 3).is_empty(), "a dropped line gets no answer");
    let plain = script("plain", "echo Clear");
    let mut c = clerk(&l.syms, plain.to_str().unwrap(), 5);
    c.carry(MODEL, &[Value::text("w1"), Value::text("x")], 4);
    let got = answers(&mut c, 1, 10);
    assert!(matches!(&got[..], [(4, Reply::Failed(w))] if w.contains("printed no answer with its count")), "no count, no answer: {got:?}");
    for p in [bad, slow, plain] {
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }
}

#[test]
fn the_clerk_carries_the_three_effects_and_wants_a_command() {
    let l = loaded(&[("t", TEXT.into())]);
    let c = clerk(&l.syms, "/bin/true", 5);
    let effects = c.effects();
    assert_eq!(effects.iter().map(|(e, _)| e.as_str()).collect::<Vec<_>>(), [CALL_TEXT, MODEL, READ_AS]);
    assert_eq!((effects[0].1.len(), &effects[1].1[..], effects[2].1.len()), (0, &["tokens".to_string(), "wall_ms".to_string()][..], 0));
    assert!(c.events().is_empty());
    assert!(Clerk::new(Config { command: vec![], at_once: 1, seconds: 5 }, &l.syms).is_err());
    assert!(Clerk::new(Config { command: vec!["/bin/true".into()], at_once: 0, seconds: 5 }, &l.syms).is_err());
}

/// The package's own calls: each template filled, and each answer's type read as its templates ask for it.
#[test]
fn the_packages_calls_are_filled_and_their_answers_read() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../docket/");
    let names = ["std/hooks", "std/bridges/claude-code", "std/bridges/codex", "std/bridges/opencode", "std/bridges/agy", "std/delivery", "std/comms", "std/rails", "std/budgets", "std/control", "std/adapt", "defaults/audit", "defaults/train", "defaults/hygiene", "defaults/brain", "defaults/desk", "defaults/project", "defaults/v1"];
    let texts: Vec<(&str, String)> = names.iter().map(|n| (*n, std::fs::read_to_string(format!("{root}{n}.docket")).unwrap())).collect();
    let l = loaded(&texts);
    let mut c = clerk(&l.syms, "/bin/true", 5);
    let calls: Vec<(&String, usize)> = l.syms.iter().filter_map(|(n, s)| if let Sym::Call { fields, .. } = s { Some((n, fields.len())) } else { None }).collect();
    assert!(calls.len() >= 4, "{calls:?}");
    for (name, n) in &calls {
        let t = text_of(c.carry(CALL_TEXT, &[prompt(name, vec![Value::text("x"); *n])], 1));
        assert!(!t.contains("{{"), "{name}: {t}");
    }
    let q = |end: &str| calls.iter().map(|(n, _)| n.as_str()).find(|n| n.ends_with(end)).unwrap().to_string();
    for (call, text) in [("Check-Proof", r#"Unmet("the picture is of another screen")"#), ("Check-Condition", "Met"), ("Trace-Normative-Claim", r#"Faulty(more, "say only what he said")"#), ("Annotate-Turn", r#"[MarkProposal("t:/a:0:9", "for", "k1")]"#)] {
        assert!(matches!(read(&mut c, &q(call), text), Reply::Answer { .. }), "{call}: {:?}", read(&mut c, &q(call), text));
    }
}
