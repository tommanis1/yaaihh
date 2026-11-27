//! A specification given as syntax with a caller's functions, questions put to it as terms (`Replay::declare`, `ask`,
//! `ways`), and a kept step taken back and made again (`Replay::undo`, `redo`).

use eflint::ast::{Decl, Elem, Phrase, Statement, Tagged, Term, Var, id};
use eflint::compile::{Host, HostTable, Reading, Spec, Ty};
use eflint::eval::Value as Got;
use eflint::parser::parse_statements;
use eflint::replay::{Event, Refusal, Replay, Value};
use std::collections::BTreeSet;
use std::rc::Rc;

const NOTES: &str = r#"
Fact person Identified by String.
Fact note Identified by String.
Fact noted Identified by person * note.
Act write Actor person Related to note
  Conditioned by str_len(note) > 2
  Creates noted(person, note).
Fact wordy Identified by note * person Holds when str_len(note) > 3.
Extend Fact wordy Derived from wordy("abc", person) When noted(person, "abc").
"#;

/// The caller's one function: the length of a string.
struct Lengths;

impl HostTable for Lengths {
    fn sig(&self, name: &str) -> Option<(Vec<Ty>, Ty)> {
        (name == "str_len").then(|| (vec![Ty::Strings], Ty::Ints))
    }

    fn call(&self, name: &str, args: &[Got]) -> Result<Got, String> {
        match (name, args) {
            ("str_len", [Got::Str(s)]) => Ok(Got::Int(s.chars().count() as i64)),
            _ => Err(format!("{name}: {args:?}")),
        }
    }
}

fn decls(text: &str) -> Vec<Decl> {
    let each = |s| match s {
        Statement::Phrase(Phrase::Decls(ds)) => ds,
        other => panic!("not a declaration: {other:?}"),
    };
    parse_statements(text).unwrap().into_iter().flat_map(each).collect()
}

fn term(text: &str) -> Term {
    match &parse_statements(&format!("?{text}.")).unwrap()[..] {
        [Statement::Phrase(Phrase::Query(t))] => t.clone(),
        other => panic!("not a query: {other:?}"),
    }
}

fn of(dom: &str, v: &str) -> Tagged {
    Tagged { elem: Elem::Str(id(v)), dom: id(dom) }
}

fn write(who: &str, what: &str) -> Event {
    Event::Act { name: "write".into(), actor: Value::Str(who.into()), objects: vec![Value::Str(what.into())] }
}

/// A condition and a question apply the caller's function; a binding is listed once whatever else holds with it; a
/// test holds of fields that stand in no fact, or when a rule derives it; a kept step taken back and made again leaves
/// what it made.
#[test]
fn a_term_is_asked_with_the_callers_functions() {
    let tests = BTreeSet::from([id("wordy")]);
    let spec = Spec { reading: Reading { guided: false, ranging: true }, host: Host(Some(Rc::new(Lengths))), tests, ..Spec::new() };
    let mut r = Replay::declare(decls(NOTES), spec).unwrap();
    let ann = Elem::Str(id("ann"));
    assert_eq!(r.step(&write("ann", "no"), Some(&ann)).map(|_| ()), Err(Refusal::Policy));
    let kept = r.step(&write("ann", "long"), Some(&ann)).unwrap();
    let noted = |p: &str, n: &str| Tagged {
        elem: Elem::Prod(Rc::from([of("person", p), of("note", n)])),
        dom: id("noted"),
    };
    assert_eq!(kept.created, [noted("ann", "long")]);
    r.step(&write("bob", "long"), None).unwrap();
    r.step(&write("bob", "brief"), None).unwrap();

    assert!(r.ask(&term("noted(person, note) && str_len(note) > 4")).unwrap());
    assert!(!r.ask(&term("noted(person, note) && str_len(note) > 5")).unwrap());
    let rows = r.ways(&[Var::plain(&id("note"))], &term("noted(person, note)")).unwrap();
    assert_eq!(rows, [[of("note", "brief")], [of("note", "long")]]);
    assert!(r.ask(&term(r#"wordy("abcd", "cy")"#)).unwrap());
    assert!(!r.ask(&term(r#"wordy("abc", "cy")"#)).unwrap());
    r.step(&write("cy", "abc"), None).unwrap();
    assert!(r.ask(&term(r#"wordy("abc", "cy")"#)).unwrap() && !r.ask(&term(r#"wordy("abc", "bob")"#)).unwrap());

    r.undo(&kept);
    assert!(!r.holds(&noted("ann", "long")).unwrap());
    r.redo(&kept);
    assert!(r.holds(&noted("ann", "long")).unwrap());
}

const BOOKS: &str = r#"
Fact shelf Identified by String.
Fact book Identified by String.
Fact shelved Identified by book * shelf.
Fact seen Identified by book.
Fact other Identified by book Holds when book != book("n1").
"#;

/// Read guided, the bindings of a question that opens with a fact are what trying every binding gives: given a field,
/// with a further condition, with another free variable, and opening with a test.
#[test]
fn ways_read_guided_are_what_every_binding_gives() {
    let questions = [
        r#"shelved(book, shelf("t1"))"#,
        r#"shelved(book, shelf("t1")) && Not(seen(book))"#,
        r#"shelved(book, shelf)"#,
        r#"other(book) && shelved(book, shelf("t1"))"#,
    ];
    let rows = |guided| {
        let spec = Spec { reading: Reading { guided, ranging: true }, tests: BTreeSet::from([id("other")]), ..Spec::new() };
        let mut r = Replay::declare(decls(BOOKS), spec).unwrap();
        for (n, t) in [("n1", "t1"), ("n2", "t1"), ("n3", "t2"), ("n4", "t1")] {
            let a = r.perform(&Event::Fact { name: "shelved".into(), objects: vec![Value::Str(n.into()), Value::Str(t.into())], holds: Some(true) });
            assert!(a.ok, "{:?}", a.errors);
        }
        assert!(r.perform(&Event::Fact { name: "seen".into(), objects: vec![Value::Str("n1".into())], holds: Some(true) }).ok);
        questions.iter().map(|q| r.ways(&[Var::plain(&id("book"))], &term(q)).unwrap()).collect::<Vec<_>>()
    };
    let guided = rows(true);
    assert_eq!(guided, rows(false));
    let books = |ns: &[&str]| ns.iter().map(|n| vec![of("book", n)]).collect::<Vec<_>>();
    assert_eq!(guided, [books(&["n1", "n2", "n4"]), books(&["n2", "n4"]), books(&["n1", "n2", "n3", "n4"]), books(&["n2", "n4"])]);
}

/// `ways_upto` gives N of the rows `ways` gives, in their order, read guided or not; all of them when N is more.
#[test]
fn ways_upto_n_are_n_of_the_ways() {
    for guided in [true, false] {
        let spec = Spec { reading: Reading { guided, ranging: true }, tests: BTreeSet::from([id("other")]), ..Spec::new() };
        let mut r = Replay::declare(decls(BOOKS), spec).unwrap();
        for (n, t) in [("n1", "t1"), ("n2", "t1"), ("n3", "t2"), ("n4", "t1")] {
            assert!(r.perform(&Event::Fact { name: "shelved".into(), objects: vec![Value::Str(n.into()), Value::Str(t.into())], holds: Some(true) }).ok);
        }
        let (book, q) = ([Var::plain(&id("book"))], term(r#"shelved(book, shelf)"#));
        let all = r.ways(&book, &q).unwrap();
        assert_eq!(all.len(), 4);
        for n in 0..6 {
            let some = r.ways_upto(&book, &q, n).unwrap();
            assert!(some.len() == n.min(4) && some.iter().all(|row| all.contains(row)) && some.windows(2).all(|w| w[0] < w[1]), "guided {guided}, {n}: {some:?}");
        }
        assert_eq!(r.ways_upto(&book, &q, 9).unwrap(), all);
    }
}

const PRICED: &str = r#"
Fact agent Identified by String.
Fact bucket Identified by String.
Fact kind Identified by String.
Fact model Identified by String.
Fact node Identified by String.
Fact thread Identified by String.
Fact bucket-of Identified by agent * bucket.
Fact on-model Identified by agent * model.
Fact rate Identified by model * kind * int.
Fact used Identified by node * agent * kind * int.
Fact posted Identified by node * thread.
"#;

/// PRICED read guided or not, ranging: five agents each on its own bucket and model, one of them priced for two kinds,
/// and TURNS turns of it, each posting in one of five threads and using its own count of tokens.
fn priced(guided: bool, turns: usize) -> Replay {
    let mut r = Replay::from_text_as(PRICED, Reading { guided, ranging: true }).unwrap();
    let mut said: Vec<String> = (1..=5).flat_map(|a| [format!("+bucket-of(\"a{a}\", \"b{a}\")."), format!("+on-model(\"a{a}\", \"m{a}\").")]).collect();
    said.extend(["input", "output"].iter().enumerate().map(|(j, kind)| format!("+rate(\"m1\", \"{kind}\", {}).", 10 + j)));
    for k in 0..turns {
        said.push(format!("+posted(\"n{k}\", \"t{}\").", k % 5));
        said.push(format!("+used(\"n{k}\", \"a1\", \"input\", {}).", 100 + k));
    }
    for s in said {
        let a = r.perform(&Event::Phrase(s.clone()));
        assert!(a.ok, "{s}: {:?}", a.errors);
    }
    r
}

/// N-1832 p305: guided, `ways` binds a variable from the facts its term names, looked up by the fields given or bound
/// so far, so it tries the rows they match whatever else the state holds, and answers as trying every binding does. An
/// Int a fact gives is bound from it; a conjunct that cannot fail is passed over to the fact after it, as is a fact
/// naming a variable of the `Exists` around the term; a variable no fact gives ranges over its type.
#[test]
fn ways_binds_from_the_facts_and_answers_as_trying_every_binding() {
    let v = |d: &str| Var::plain(&id(d));
    for turns in [40, 200] {
        let (mut guided, mut every) = (priced(true, turns), priced(false, turns));
        let posts = turns as u64 / 5;
        let cases: [(Vec<Var>, &str, u64); 5] = [
            (vec![v("bucket"), v("model"), v("int")], r#"bucket-of("a1", bucket) && on-model("a1", model) && rate(model, "input", int) && int > 0"#, 1),
            (vec![v("node")], r#"posted(node, "t1")"#, posts),
            (vec![v("node")], r#"node != "n1" && posted(node, "t1")"#, posts),
            (vec![v("model")], r#"bucket-of("a1", bucket) && on-model("a1", model)"#, 1),
            (vec![v("node"), v("int")], r#"posted(node, "t1") && int > 105"#, posts * (2 + turns as u64)),
        ];
        for (k, (xs, q, tries)) in cases.iter().enumerate() {
            let (g, e) = (guided.tried(), every.tried());
            let rows = guided.ways(xs, &term(q)).unwrap();
            assert_eq!(rows, every.ways(xs, &term(q)).unwrap(), "{turns} turns: {q}");
            assert_eq!(guided.tried() - g, *tries, "{turns} turns: {q}");
            assert!(k == 4 || every.tried() - e > *tries, "{turns} turns: trying every binding of {q} tries {}", every.tried() - e);
        }
    }
}
