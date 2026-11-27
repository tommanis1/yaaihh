//! Ranging (`compile::Reading::ranging`): a value stands in a fact without being declared, and an act is there for any
//! valid fields, so its conditions alone say whether it is enabled.

use eflint::compile::Reading;
use eflint::replay::{Event, Refusal, Replay, Value};

const SHELVES: &str = r#"
Fact person Identified by String.
Fact shelf Identified by String.
Fact book Identified by String.
Fact on Identified by book * shelf.
Fact like Identified by book * person.
Fact likes Identified by book * int
  Derived from (Foreach book, shelf : likes(book, Count(Foreach person : person When like(book, person))) When on(book, shelf)).
Act shelve Actor person Related to book, shelf
  Conditioned by Not(Exists shelf' : on(book, shelf'))
  Creates on(book, shelf).
Act like-it Actor person Related to book
  Conditioned by (Exists shelf : on(book, shelf))
  Creates like(book, person).
+person("ann"). +person("bob"). +person("cy").
+shelf("s1").
"#;

fn act(name: &str, actor: &str, objects: &[&str]) -> Event {
    let s = |v: &str| Value::Str(v.to_string());
    Event::Act { name: name.to_string(), actor: s(actor), objects: objects.iter().map(|o| s(o)).collect() }
}

fn holds(r: &mut Replay, q: &str) -> bool {
    let a = r.perform(&Event::Phrase(format!("?{q}.")));
    assert!(a.ok && a.queries.len() == 1, "{q}: {:?}", a.errors);
    a.queries[0]
}

/// `book("b1")` is never stated: ranging, it stands in `on` once shelved, a count ranges over it, and a second
/// shelving or a like of a book on no shelf is refused by its condition. Ranging off, as the reference reads, the
/// first act is not there at all.
#[test]
fn a_value_stands_in_a_fact_without_being_declared() {
    let mut r = Replay::from_text_as(SHELVES, Reading { guided: false, ranging: true }).unwrap();
    assert!(r.step(&act("shelve", "ann", &["b1", "s1"]), None).is_ok());
    assert_eq!(r.step(&act("shelve", "bob", &["b1", "s1"]), None).map(|_| ()), Err(Refusal::Policy));
    for p in ["ann", "bob", "cy"] {
        assert!(r.step(&act("like-it", p, &["b1"]), None).is_ok(), "{p}");
    }
    assert!(holds(&mut r, r#"likes(book("b1"), 3)"#));
    assert_eq!(r.step(&act("like-it", "ann", &["b2"]), None).map(|_| ()), Err(Refusal::Policy));

    let mut plain = Replay::from_text(SHELVES).unwrap();
    assert_eq!(plain.step(&act("shelve", "ann", &["b1", "s1"]), None).map(|_| ()), Err(Refusal::Policy));
}
