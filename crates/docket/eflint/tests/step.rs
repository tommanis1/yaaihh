//! A step is kept only if it is enabled and closes no duty of its holder (`Replay::step`), a refusal leaves the state
//! as it was, and what a step ends is gone over as its type allows.

use eflint::ast::{Elem, Tagged, id};
use eflint::compile::Reading;
use eflint::replay::{Event, Refusal, Replay, Value};

/// `mind` is derived and ends when its holder lets go of the box; `tidy` is stored, made by the claimant's request, and
/// violated through the derived `messy`.
const BOXES: &str = r#"
Fact person Identified by String.
Fact box Identified by String.
Fact thing Identified by String.
Placeholder holder For person.
Placeholder claimant For person.
Fact has Identified by box * holder.
Fact asked Identified by box * claimant.
Fact inside Identified by thing * box.
Fact messy Identified by box Holds when (Exists thing : inside(thing, box)).
Duty mind Holder holder Claimant claimant Related to box
  Holds when has(box, holder) && asked(box, claimant).
Duty tidy Holder holder Claimant claimant Related to box
  Violated when messy(box).
Act take Actor holder Related to box
  Conditioned by Not(Exists holder' : has(box, holder'))
  Creates has(box, holder).
Act drop Actor holder Related to box
  Conditioned by has(box, holder)
  Terminates has(box, holder).
Act ask Actor claimant Related to box, holder
  Creates asked(box, claimant), tidy(holder, claimant, box).
Act release Actor claimant Related to box
  Terminates asked(box, claimant).
Act put Actor holder Related to thing, box
  Creates inside(thing, box).
Act empty Actor holder Related to box
  Terminates inside(thing, box).
Event cleared Related to box
  Terminates inside(thing, box).
+person("ann"). +person("bob").
"#;

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

fn act(name: &str, actor: &str, objects: &[&str]) -> Event {
    Event::Act { name: name.to_string(), actor: s(actor), objects: objects.iter().map(|o| s(o)).collect() }
}

fn person(v: &str) -> Tagged {
    Tagged { elem: Elem::Str(id(v)), dom: id("person") }
}

fn holds(r: &mut Replay, q: &str) -> bool {
    let a = r.perform(&Event::Phrase(format!("?{q}.")));
    assert!(a.ok && a.queries.len() == 1, "{q}: {:?}", a.errors);
    a.queries[0]
}

/// Each refusal by its reason: a condition that fails, a derived duty its holder would end, a stored duty's violation
/// its holder would clear through a derived fact; and what neither holder nor rule limits, kept: the claimant ending a
/// duty, a violation arising, an event with no holder clearing it.
#[test]
fn a_step_is_refused_when_it_closes_a_duty_of_its_holder() {
    let mut r = Replay::from_text_as(BOXES, Reading { guided: false, ranging: true }).unwrap();
    let (ann, bob) = (person("ann"), person("bob"));
    let step = |r: &mut Replay, e: Event, by: &Tagged| r.step(&e, Some(&by.elem)).map(|_| ());
    let violated = |r: &mut Replay| r.duties_of(&person("ann").elem).unwrap().iter().map(|d| d.violated).collect::<Vec<_>>();

    assert_eq!(step(&mut r, act("take", "ann", &["x"]), &ann), Ok(()));
    assert_eq!(step(&mut r, act("take", "bob", &["x"]), &bob), Err(Refusal::Policy));
    assert_eq!(step(&mut r, act("ask", "bob", &["x", "ann"]), &bob), Ok(()));
    assert!(holds(&mut r, r#"mind("ann", "bob", "x") && tidy("ann", "bob", "x")"#));
    assert_eq!(violated(&mut r), [false, false]);

    match step(&mut r, act("drop", "ann", &["x"]), &ann) {
        Err(Refusal::Closes(d)) => assert_eq!(d.dom, id("mind")),
        other => panic!("drop: {other:?}"),
    }
    assert!(holds(&mut r, r#"has("x", "ann") && mind("ann", "bob", "x")"#));

    assert_eq!(step(&mut r, act("put", "ann", &["t1", "x"]), &ann), Ok(()));
    assert_eq!(violated(&mut r), [false, true]);
    match step(&mut r, act("empty", "ann", &["x"]), &ann) {
        Err(Refusal::Closes(d)) => assert_eq!(d.dom, id("tidy")),
        other => panic!("empty: {other:?}"),
    }
    assert!(holds(&mut r, r#"inside("t1", "x") && messy("x")"#));
    let cleared = Event::Event { name: "cleared".into(), objects: vec![s("x")] };
    assert!(r.step(&cleared, None).is_ok());
    assert_eq!(violated(&mut r), [false, false]);

    assert_eq!(step(&mut r, act("release", "bob", &["x"]), &bob), Ok(()));
    assert_eq!(step(&mut r, act("drop", "ann", &["x"]), &ann), Ok(()));
    assert_eq!(r.duties_of(&ann.elem).unwrap().iter().map(|d| d.duty.dom.to_string()).collect::<Vec<_>>(), ["tidy"]);
}

/// The holder is a value: a duty whose holder field is of another type than the actor's, with the same value, is the
/// actor's, and the closing rule keeps it.
#[test]
fn a_holder_of_another_type_with_the_actors_value_is_the_actor() {
    let text = r#"
Fact person Identified by String.
Fact member Identified by String.
Fact box Identified by String.
Placeholder claimant For person.
Fact has Identified by box * member.
Duty keep Holder member Claimant claimant Related to box Holds when has(box, member).
Act take Actor person Related to box Creates has(box, member(person)).
Act drop Actor person Related to box Terminates has(box, member(person)).
+person("ann"). +person("bob").
"#;
    let mut r = Replay::from_text_as(text, Reading { guided: false, ranging: true }).unwrap();
    let (ann, bob) = (person("ann"), person("bob"));
    assert!(r.step(&act("take", "ann", &["x"]), Some(&ann.elem)).is_ok());
    match r.step(&act("drop", "ann", &["x"]), Some(&ann.elem)) {
        Err(Refusal::Closes(d)) => assert_eq!((d.dom, holds(&mut r, r#"has("x", member("ann"))"#)), (id("keep"), true)),
        other => panic!("drop: {other:?}"),
    }
    assert!(r.step(&act("drop", "bob", &["x"]), Some(&bob.elem)).is_ok(), "bob holds no duty and ends nothing");
}

/// Guided, ending a fact with open fields goes over the instances that hold where its type is closed, so a combination
/// never held is not stated; where the type is open it goes over every combination, and one never held, unknown
/// before, is stated false.
#[test]
fn ending_goes_over_the_held_instances_alone_on_a_closed_type() {
    for open in [false, true] {
        let text = r#"
Fact person Identified by String.
Fact text Identified by String.
OPEN Fact changing Identified by person * text * text'.
Act note Actor person Related to text, text' Creates changing(person, text, text').
Act forget Actor person Terminates changing(person, text, text') When text != text'.
+person("ann").
"#
        .replace("OPEN ", if open { "Open " } else { "" });
        let mut r = Replay::from_text_as(&text, Reading { guided: true, ranging: true }).unwrap();
        for objects in [["a", "b"], ["c", "d"]] {
            assert!(r.step(&act("note", "ann", &objects), None).is_ok());
        }
        let taken = r.step(&act("forget", "ann", &[]), None).unwrap();
        assert_eq!(taken.terminated.len(), 2, "open: {open}");
        let word = |v: &str| Tagged { elem: Elem::Str(id(v)), dom: id("text") };
        let never = Tagged { elem: Elem::Prod(vec![person("ann"), word("a"), word("d")].into()), dom: id("changing") };
        let stated = r.state().unwrap().contents().get(&never).map(|i| i.value);
        assert_eq!(stated, open.then_some(false), "open: {open}");
        assert!(holds(&mut r, r#"Not(changing("ann", "a", "b")) && Not(changing("ann", "a", "d"))"#));
    }
}
