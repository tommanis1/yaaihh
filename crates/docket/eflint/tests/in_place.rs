//! A line taken in place (`Replay::perform`: the state changed where it stands, a derived type worked out only when
//! something reads it) answers as the same line taken by working every derived type out again from the stored facts
//! (`Replay::perform_from_nothing`), after every line of a stream; and the two states, worked out whole, are the same
//! fact for fact, with the same duties. And the guided way (rules started from the facts they name) answers as every
//! binding tried.

use eflint::ast::{Elem, id};
use eflint::eval::Info;
use eflint::replay::{Event, Refusal, Replay, Taken, Value};

/// Rules that read derived facts positively, under `Not`, through `Exists` and a count, a derived act, a duty with a
/// violation, a `Var` and a `Function` type, whose new instances end others, an act whose effect reads another
/// instance of it, and an `Exists` given a literal.
const RULES: &str = r#"
Fact person Identified by String
Fact thing Identified by String
Fact owns Identified by person * thing
Fact lent Identified by person * person' * thing
Fact away Identified by person
Var weather Identified by String
Function holder Identified by thing * person
Fact trusted Identified by person
  Holds when (Exists thing : owns(person, thing))
Fact lonely Identified by person
  Holds when person(person) && Not(trusted(person)) && Not(away(person))
Fact busy Identified by person
  Holds when Count(Foreach thing : thing When (Exists person' : lent(person, person', thing))) > 1
Fact first-lender Identified by person
  Holds when busy(person) && Not(Exists person' : busy(person') && person' != person && lonely(person'))
Duty owes-back
  Holder person
  Claimant person'
  Related to thing
  Violated when lonely(person') || away(person)
Act give
  Actor person
  Recipient person'
  Related to thing
  Holds when owns(person, thing) && person != person'
  Creates owes-back(person', person, thing), lent(person, person', thing), holder(thing, person')
  Terminates owns(person, thing)
Act take-back
  Actor person
  Recipient person'
  Related to thing
  Conditioned by lent(person, person', thing)
  Terminates owes-back(person', person, thing), lent(person, person', thing)
  Creates owns(person, thing), holder(thing, person)
Event leaves
  Related to person
  Creates away(person)
Event comes-back
  Related to person
  Terminates away(person)
Fact lends-pen Identified by person
  Holds when (Exists person' : lent(person, person', thing("pen")))
Fact nudged Identified by person * person'
Act nudge
  Actor person
  Recipient person'
  Holds when person != person' && Not(away(person'))
  Creates nudged(person, person') When nudge(person', person).
+person("ann"). +person("bob"). +person("cy"). +person("di").
+thing("pen"). +thing("cup"). +thing("hat"). +thing("key").
+owns(person("ann"), thing("pen")). +owns(person("bob"), thing("cup")).
"#;

/// A duty its rule works out: held by the borrower while the lender is not away, ended by the lender's taking back or
/// leaving, violated while its holder is away or nudged.
const MINDS: &str = r#"
Duty minds
  Holder person
  Claimant person'
  Related to thing
  Holds when lent(person', person, thing) && Not(away(person'))
  Violated when away(person) || nudged(person', person).
"#;

const PEOPLE: [&str; 4] = ["ann", "bob", "cy", "di"];
const THINGS: [&str; 4] = ["pen", "cup", "hat", "key"];

fn s(x: &str) -> Value {
    Value::Str(x.to_string())
}

/// N lines drawn from every kind the rules take, the same each run.
fn lines(n: usize) -> Vec<Event> {
    let mut seed: u64 = 7;
    let mut next = |m: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % m
    };
    let mut out = Vec::new();
    while out.len() < n {
        let (a, b, i) = (PEOPLE[next(4)], PEOPLE[next(4)], THINGS[next(4)]);
        out.push(match next(13) {
            0 | 1 => Event::Act { name: "give".into(), actor: s(a), objects: vec![s(b), s(i)] },
            2 => Event::Act { name: "take-back".into(), actor: s(a), objects: vec![s(b), s(i)] },
            3 => Event::Fact { name: "owns".into(), objects: vec![s(a), s(i)], holds: Some(true) },
            4 => Event::Fact { name: "owns".into(), objects: vec![s(a), s(i)], holds: Some(false) },
            5 => Event::Event { name: "leaves".into(), objects: vec![s(a)] },
            6 => Event::Event { name: "comes-back".into(), objects: vec![s(a)] },
            7 => Event::Fact { name: "weather".into(), objects: vec![s(["dry", "wet", "cold"][next(3)])], holds: Some(true) },
            8 => Event::Phrase(format!("?lonely(person(\"{a}\")).")),
            9 => Event::Act { name: "nudge".into(), actor: s(a), objects: vec![s(b)] },
            10 => Event::Phrase(format!("?lends-pen(person(\"{a}\")).")),
            11 => Event::Phrase(format!("?trusted(person(\"{a}\")).")),
            _ => Event::Phrase(format!("?first-lender(person(\"{a}\")) || (Exists person : busy(person) && person != person(\"{b}\")).")),
        });
    }
    out
}

fn duties(r: &mut Replay) -> Vec<String> {
    r.duties().iter().map(|d| format!("{} {}", d.duty, d.violated)).collect()
}

fn answer(x: &eflint::replay::Answer) -> impl PartialEq + std::fmt::Debug {
    (x.ok, x.errors.clone(), x.executed.clone(), x.disabled.clone(), x.queries.clone(), x.instances.clone())
}

/// The stream taken in place, guided or not, against the same worked out from nothing, guided or not.
fn same_after_every_line(fast_guided: bool, slow_guided: bool) {
    let read = |guided| eflint::compile::Reading { guided, ranging: false };
    let mut fast = Replay::from_text_as(RULES, read(fast_guided)).unwrap();
    let mut slow = Replay::from_text_as(RULES, read(slow_guided)).unwrap();
    let (mut performed, mut asked) = (0, 0);
    for (n, e) in lines(600).iter().enumerate() {
        let a = fast.perform(e);
        let b = slow.perform_from_nothing(e);
        assert_eq!(answer(&a), answer(&b), "line {} {e:?}", n + 1);
        performed += a.ok as usize;
        asked += a.queries.len();
        // a question now and then on the line's own state, so some types are worked out between lines and some not
        if n % 7 == 3 {
            let seen = duties(&mut fast);
            assert_eq!(seen, duties(&mut slow.clone()), "line {} {e:?}", n + 1);
        }
        let (mut seen, mut whole) = (fast.clone(), slow.clone());
        let (facts, owed) = (seen.state().unwrap().contents().clone(), duties(&mut seen));
        assert_eq!(&facts, whole.state().unwrap().contents(), "line {} {e:?}", n + 1);
        assert_eq!(owed, duties(&mut whole), "line {} {e:?}", n + 1);
    }
    // the stream does what it is for: most lines performed, questions asked
    assert!(performed > 400 && asked > 100, "{performed} performed, {asked} asked");
}

/// Guided and ranging, as the normative core reads, the state agrees with every derived type worked out whole after
/// every line, and a derived instance turned false by hand is the one named.
#[test]
fn a_state_worked_out_line_by_line_agrees_with_the_whole_until_one_instance_is_changed() {
    let mut r = Replay::from_text_as(RULES, eflint::compile::Reading { guided: true, ranging: true }).unwrap();
    for (n, e) in lines(200).iter().enumerate() {
        r.perform(e);
        assert_eq!(r.unlike_whole(false), Ok(None), "line {} {e:?}", n + 1);
    }
    let held = r.state().unwrap().contents().iter().find(|(te, i)| te.dom == id("trusted") && i.derived && i.value);
    let te = held.expect("someone trusted").0.clone();
    r.cfg.state.insert(te.clone(), Info { value: false, derived: true });
    assert_eq!((r.unlike_whole(false), r.unlike_whole(true)), (Ok(Some(te.clone())), Ok(Some(te))));
}

/// Steps kept or refused by the closing rule, guided and ranging with the rules a step reaches worked out for the
/// instances it reaches, answer as the same steps with every derived type worked out whole; each kept step is taken
/// back and made again, as the normative core does, and the state agrees with the whole after every line. The holder
/// is the actor or the one acted on by turns, or an event's person; the first four steps end a duty and clear a
/// violation, both refused.
#[test]
fn a_step_worked_out_where_it_reaches_answers_as_one_worked_out_whole() {
    let text = format!("{RULES}{MINDS}");
    let read = |guided| eflint::compile::Reading { guided, ranging: true };
    let (mut fast, mut slow) = (Replay::from_text_as(&text, read(true)).unwrap(), Replay::from_text_as(&text, read(false)).unwrap());
    let act = |name: &str, [a, b, i]: [&str; 3]| Event::Act { name: name.into(), actor: s(a), objects: vec![s(b), s(i)] };
    let event = |name: &str| Event::Event { name: name.into(), objects: vec![s("bob")] };
    let first = [act("give", ["ann", "bob", "pen"]), act("take-back", ["ann", "bob", "pen"]), event("leaves"), event("comes-back")];
    let seen = |r: &Result<Taken, Refusal>| r.as_ref().map(|t| (t.executed.clone(), t.created.clone(), t.terminated.clone())).map_err(Clone::clone);
    let (mut kept, mut refused) = (0, Vec::new());
    for (n, e) in first.into_iter().chain(lines(600)).enumerate() {
        let holder = match &e {
            Event::Act { actor, objects, .. } => [actor, &objects[0]][n % 2],
            Event::Event { objects, .. } => &objects[0],
            _ => {
                let (a, b) = (fast.perform(&e), slow.perform(&e));
                assert_eq!(answer(&a), answer(&b), "line {} {e:?}", n + 1);
                continue;
            }
        };
        let Value::Str(h) = holder else { unreachable!() };
        let h = Elem::Str(id(h));
        let (a, b) = (fast.step(&e, Some(&h)), slow.step(&e, Some(&h)));
        assert_eq!(seen(&a), seen(&b), "line {} {e:?}", n + 1);
        match a {
            Ok(t) => {
                kept += 1;
                fast.undo(&t);
                fast.redo(&t);
            }
            Err(Refusal::Closes(_)) => refused.push(n),
            Err(_) => {}
        }
        if n % 7 == 3 {
            for p in PEOPLE.map(|p| Elem::Str(id(p))) {
                assert_eq!(fast.duties_of(&p), slow.clone().duties_of(&p), "line {} {e:?}", n + 1);
            }
        }
        let (mut now, mut whole) = (fast.clone(), slow.clone());
        assert_eq!(now.unlike_whole(false), Ok(None), "line {} {e:?}", n + 1);
        assert_eq!(now.state().unwrap().contents(), whole.state().unwrap().contents(), "line {} {e:?}", n + 1);
    }
    assert!(refused.starts_with(&[1, 3]) && kept > 50, "{kept} kept, refused at lines {refused:?}");
}

#[test]
fn a_line_taken_in_place_answers_as_one_worked_out_from_nothing_guided() {
    same_after_every_line(true, true);
}

#[test]
fn a_line_taken_in_place_answers_as_one_worked_out_from_nothing_unguided() {
    same_after_every_line(false, false);
}

#[test]
fn a_line_taken_in_place_guided_answers_as_one_worked_out_from_nothing_unguided() {
    same_after_every_line(true, false);
}

/// An act whose type fails to be worked out for some instance (an open fact not assigned) fails as the whole type
/// does, though the instance performed alone would not fail. The open fact is given for the two people the rules
/// load with; the person added after them is the one not given. Guided, as a record's lines are taken.
#[test]
fn an_act_that_fails_elsewhere_fails_as_the_whole_type() {
    const OPEN: &str = r#"
Fact person Identified by String
Open Fact vouched Identified by person
Act lend
  Actor person
  Recipient person'
  Holds when vouched(person').
+vouched(person("ann")). +vouched(person("bob")).
+person("ann"). +person("bob").
"#;
    let guided = eflint::compile::Reading { guided: true, ranging: false };
    let (mut fast, mut slow) = (Replay::from_text_as(OPEN, guided).unwrap(), Replay::from_text_as(OPEN, guided).unwrap());
    let cy = Event::Fact { name: "person".into(), objects: vec![s("cy")], holds: Some(true) };
    let (a, b) = (fast.perform(&cy), slow.perform_from_nothing(&cy));
    assert_eq!(answer(&a), answer(&b));
    assert!(b.ok, "{:?}", b.errors);
    let e = Event::Act { name: "lend".into(), actor: s("ann"), objects: vec![s("bob")] };
    let (a, b) = (fast.perform(&e), slow.perform_from_nothing(&e));
    assert_eq!(answer(&a), answer(&b));
    assert!(!b.ok, "{:?}", b.errors);
}

/// A line that fails changes nothing: the act refused for an undeclared type leaves the state as it was. Guided, as a
/// record's lines are taken.
#[test]
fn a_line_that_fails_leaves_the_state_as_it_was() {
    let mut r = Replay::from_text_as(RULES, eflint::compile::Reading { guided: true, ranging: false }).unwrap();
    r.perform(&Event::Act { name: "give".into(), actor: s("ann"), objects: vec![s("bob"), s("pen")] });
    let before = r.state().unwrap().contents().clone();
    let a = r.perform(&Event::Fact { name: "owns".into(), objects: vec![s("ann")], holds: Some(true) });
    assert!(!a.ok);
    assert_eq!(r.state().unwrap().contents(), &before);
}
