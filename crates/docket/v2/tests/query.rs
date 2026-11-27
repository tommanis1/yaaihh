//! Reads from outside (`run::machine::query`): instance queries over joins, conditions, derived facts and duties;
//! whether given acts are enabled for an actor, and why not; violated duties; none of them leaves a read for a run.

use docket_v2::check::{load, resolve};
use docket_v2::norm::Core;
use docket_v2::run::machine::{Enabled, Holds, Machine};
use docket_v2::run::norms::Norms;
use docket_v2::run::value::{Instance, Value};
use docket_v2::syntax::{read, read_block};

const LENDING: &str = r#"
module t
Fact Person Identified by String
Fact Book Identified by String
Fact Shelf(book: Book, room: String)
Fact Lent(book: Book, to: Person, by: Person)
Fact Overdue(book: Book)
Fact Owes(debtor: Person, to: Person, book: Book)
  Holds when Lent(book, debtor, to)
Duty Give-Back(holder: Person, claimant: Person, book: Book)
  Holds when Lent(book, holder, claimant)
  Violated when Overdue(book)
Act Lend(by: Person, book: Book, to: Person)
  Conditioned by by != to && Not(Exists p, q : Lent(book, p, q))
  Creates Lent(book, to, by)
Act Mark(by: Person, book: Book)
  Creates Overdue(book)
+Person("ann"). +Person("bob"). +Person("cy").
+Shelf("b1", "hall"). +Shelf("b2", "attic"). +Shelf("b3", "hall").
"#;

fn inst(ty: &str, args: &[&str]) -> Instance {
    Instance { ty: format!("t.{ty}"), args: args.iter().map(|a| Value::text(a)).collect() }
}

fn lent() -> Machine<Core> {
    let mut m = Machine::new(load(vec![read(LENDING).unwrap()]).unwrap(), Core::default(), vec![]).unwrap();
    for (by, book, to) in [("cy", "b2", "ann"), ("ann", "b1", "bob"), ("ann", "b3", "cy")] {
        let s = m.norms.step(&inst("Lend", &[by, book, to]), Some(&Value::text(by))).unwrap();
        m.norms.commit(s);
    }
    m.norms.reads();
    m
}

/// The values query Q gives, read in module t's names.
fn query(m: &mut Machine<Core>, q: &str) -> Result<Vec<Value>, String> {
    query_within(m, q, u64::MAX)
}

/// [`query`], refused past STEPS bindings tried.
fn query_within(m: &mut Machine<Core>, q: &str, steps: u64) -> Result<Vec<Value>, String> {
    let mut e = read_block(&format!("{{ {q} }}")).map_err(|e| e.to_string())?;
    resolve(&m.loaded, "t", &[], &mut e).map_err(|e| e.to_string())?;
    m.inst_query(&e, steps)
}

fn texts(vs: &[&str]) -> Value {
    Value::Tuple(vs.iter().map(|v| Value::text(v)).collect())
}

#[test]
fn an_instance_query_joins_filters_and_reads_derived_facts_and_duties() {
    let mut m = lent();
    // a join with a condition: the books lent by ann that stand in the hall, with who has them
    let got = query(&mut m, r#"Foreach b, p : (b, p) When Lent(b, p, "ann") && Shelf(b, "hall")"#).unwrap();
    assert_eq!(got, [texts(&["b1", "bob"]), texts(&["b3", "cy"])]);
    // a derived fact, and a duty, as instances
    assert_eq!(query(&mut m, r#"Foreach d, c, b : Owes(d, c, b) When Owes(d, c, b) && c == "ann""#).unwrap(), [inst("Owes", &["bob", "ann", "b1"]), inst("Owes", &["cy", "ann", "b3"])].map(Value::Inst));
    let duties = query(&mut m, r#"Foreach h, c, b : Give-Back(h, c, b) When Give-Back(h, c, b) && c == "ann""#).unwrap();
    assert_eq!(duties, [inst("Give-Back", &["bob", "ann", "b1"]), inst("Give-Back", &["cy", "ann", "b3"])].map(Value::Inst));
    // a comparison and a negation over what holds
    let got = query(&mut m, r#"Foreach b : b When Shelf(b, "hall") && Not(Exists p, q : Lent(b, p, q) && p == "bob")"#).unwrap();
    assert_eq!(got, [Value::text("b3")]);
    assert!(query(&mut m, r#"Foreach b : b When Shelf(b, "cellar")"#).unwrap().is_empty());
    assert_eq!(m.norms.reads(), [0; 32], "no read is left for a run");
    assert!(query(&mut m, "Owes").unwrap_err().contains("a `Foreach`"), "only a Foreach is an instance query");
}

#[test]
fn an_instance_query_past_its_cap_of_bindings_is_refused_and_leaves_no_cap_after_it() {
    let mut m = lent();
    let cube = r#"Foreach a, b, c : (a, b, c) When Person(a) && Person(b) && Person(c)"#;
    assert_eq!(query_within(&mut m, cube, 1_000).unwrap().len(), 27);
    let why = query_within(&mut m, cube, 10).unwrap_err();
    assert!(why.contains("more than 10 bindings"), "{why}");
    // a count in the condition tries its bindings within the same cap
    let counted = r#"Foreach a : a When Person(a) && Count(Foreach b, c : b When Person(b) && Person(c)) == 9"#;
    assert!(query_within(&mut m, counted, 5).unwrap_err().contains("more than 5 bindings"));
    assert_eq!(query_within(&mut m, counted, 1_000).unwrap().len(), 3);
    assert_eq!(m.norms.reads(), [0; 32], "no read is left for a run");
    assert_eq!(query(&mut m, cube).unwrap().len(), 27, "the cap is the query's alone");
}

#[test]
fn enabled_for_answers_each_act_given_and_says_why_one_is_not() {
    let mut m = lent();
    let (ann, bob) = (Value::text("ann"), Value::text("bob"));
    let asked = [inst("Lend", &["bob", "b9", "cy"]), inst("Lend", &["bob", "b1", "cy"]), inst("Lend", &["bob", "b9", "bob"])];
    let got = m.enabled_for(&bob, &asked).unwrap();
    assert_eq!(got[0], Enabled::Yes);
    assert!(matches!(&got[1], Enabled::No(w) if w.contains("not enabled")), "b1 is lent already: {:?}", got[1]);
    assert!(matches!(&got[2], Enabled::No(_)), "to oneself: {:?}", got[2]);
    assert_eq!(m.norms.reads(), [0; 32], "no read is left for a run");
    assert_eq!(m.holding("t.Lent").unwrap().len(), 3, "nothing was committed");
    assert!(m.enabled_for(&ann, &[inst("Owes", &["ann", "cy"])]).unwrap_err().contains("no Act"));
}

#[test]
fn the_types_and_the_violated_duties() {
    let mut m = lent();
    let types = m.holding_types();
    assert!(types.contains(&("t.Lent".into(), Holds::Fact)) && types.contains(&("t.Give-Back".into(), Holds::Duty)));
    assert!(!types.iter().any(|(n, _)| n == "t.Lend"), "no Act");
    assert!(m.violated("t.Give-Back").unwrap().is_empty());
    let s = m.norms.step(&inst("Mark", &["ann", "b3"]), Some(&Value::text("ann"))).unwrap();
    m.norms.commit(s);
    m.norms.reads();
    assert_eq!(m.violated("t.Give-Back").unwrap(), [inst("Give-Back", &["cy", "ann", "b3"])]);
    assert_eq!(m.norms.reads(), [0; 32]);
    assert!(m.violated("t.Lent").is_err());
}

#[test]
fn moved_says_what_commits_changed_and_names_whole_only_a_type_no_step_keeps() {
    let counts = "Fact Loans(n: Int)\n  Holds when n == Count(Foreach b, p, q : b When Lent(b, p, q))\nFact Lendings(n: Int)\n  Holds when n == Count(Foreach b, p, q : b When Lent(b, p, q) && p != q)\n";
    let text = LENDING.replace("Act Lend(", &format!("{counts}Act Lend("));
    let mut m = Machine::new(load(vec![read(&text).unwrap()]).unwrap(), Core::default(), vec![]).unwrap();
    assert!(m.moved().all, "the first ask starts the count");
    let s = m.norms.step(&inst("Lend", &["cy", "b2", "ann"]), Some(&Value::text("cy"))).unwrap();
    m.norms.commit(s);
    let got = m.moved();
    let loans = |n: i64| Instance { ty: "t.Loans".into(), args: vec![Value::Int(n)] };
    assert!(!got.all && got.ended == [loans(0)], "a plain count is kept a step at a time: {got:?}");
    let made = [inst("Lent", &["b2", "ann", "cy"]), inst("Owes", &["ann", "cy", "b2"]), inst("Give-Back", &["ann", "cy", "b2"]), loans(1)];
    assert!(got.made.len() == 4 && made.iter().all(|i| got.made.contains(i)), "the stored fact, and what the steps keep of the derived: {got:?}");
    assert_eq!(got.whole, ["t.Lendings"], "a count with a condition, which no step keeps, is read whole; Shelf and Person are not");
    assert_eq!(got.duties, [(inst("Give-Back", &["ann", "cy", "b2"]), false)]);
    assert_eq!(m.enabled_for(&Value::text("bob"), &[inst("Lend", &["bob", "b9", "cy"])]).unwrap(), [Enabled::Yes]);
    let tried = m.moved();
    assert!(tried.made.is_empty() && tried.ended.is_empty() && tried.duties.is_empty(), "a step tried and taken back moves nothing: {tried:?}");
    let s = m.norms.step(&inst("Mark", &["ann", "b2"]), Some(&Value::text("ann"))).unwrap();
    m.norms.commit(s);
    let got = m.moved();
    assert_eq!((got.made, got.duties), (vec![inst("Overdue", &["b2"])], vec![(inst("Give-Back", &["ann", "cy", "b2"]), true)]));
    m.norms.unwatch();
    assert!(m.moved().all, "after unwatch the count starts again");
}

/// A count and a sum over stored facts kept a step at a time agree with the whole worked out after each step, an
/// instance made unknown (`Obfuscates`) included; neither is read whole.
#[test]
fn a_count_and_a_sum_kept_by_steps_agree_with_the_whole_when_an_instance_is_obfuscated() {
    let text = r#"
module t
Fact Person Identified by String
Fact Lent(book: String, to: Person)
Fact Loans(n: Int)
  Holds when n == Count(Foreach b, p : b When Lent(b, p))
Fact Fine(book: String, amount: Int)
Fact Fines(total: Int)
  Holds when total == Sum(Foreach b, k : k When Fine(b, k))
Act Lend(by: Person, book: String)
  Creates Lent(book, by)
Act Hide(by: Person, book: String)
  Obfuscates Lent(book, by)
Act Charge(by: Person, book: String, amount: Int)
  Creates Fine(book, amount)
Act Blur(by: Person, book: String, amount: Int)
  Obfuscates Fine(book, amount)
+Person("ann").
"#;
    let mut m = Machine::new(load(vec![read(text).unwrap()]).unwrap(), Core::checked_guided(), vec![]).unwrap();
    assert!(m.moved().all, "the first ask starts the count");
    let int = |ty: &str, n: i64| Instance { ty: format!("t.{ty}"), args: vec![Value::Int(n)] };
    let steps: [(&str, Vec<Value>, i64, i64); 6] = [
        ("Lend", vec![Value::text("b1")], 1, 0),
        ("Lend", vec![Value::text("b2")], 2, 0),
        ("Charge", vec![Value::text("b1"), Value::Int(5)], 2, 5),
        ("Charge", vec![Value::text("b2"), Value::Int(7)], 2, 12),
        ("Hide", vec![Value::text("b1")], 1, 12),
        ("Blur", vec![Value::text("b2"), Value::Int(7)], 1, 5),
    ];
    for (act, rest, loans, fines) in steps {
        let args: Vec<Value> = [Value::text("ann")].into_iter().chain(rest).collect();
        let s = m.norms.step(&Instance { ty: format!("t.{act}"), args }, Some(&Value::text("ann"))).unwrap();
        // the checked core panics here if what the steps keep is not the whole worked out again
        m.norms.commit(s);
        let got = m.moved();
        assert!(!got.all && got.whole.is_empty(), "{act}: {got:?}");
        assert!(m.norms.holds(&int("Loans", loans)) && m.norms.holds(&int("Fines", fines)), "{act}: {loans} loans, {fines} in fines");
    }
}
