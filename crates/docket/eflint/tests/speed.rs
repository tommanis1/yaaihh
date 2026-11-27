//! A line costs about the same late in a long record as early in it: performing an act, adding a fact and asking
//! about one instance, a question that opens with two facts, the bindings of a question that opens with a fact given
//! one field, or a step checked by the closing rule. The made-up rules have the two shapes whose cost grows
//! with the record when a line is worked out over every instance: an act whose rule ranges over every pair of people
//! and every message, and a question whose rule asks whether any other message is a piece of this one.
use eflint::ast::{Elem, Phrase, Statement, Var, id};
use eflint::replay::{Event, Refusal, Replay, Value};
use std::time::{Duration, Instant};

const RULES: &str = r#"
Fact person Identified by String
Fact message Identified by Int
Fact said Identified by person * message
Fact answered Identified by message
Fact piece-of Identified by message * message'
Fact unanswered Identified by message
  Holds when (Exists person : said(person, message)) && Not(answered(message))
             && Not(Exists message' : piece-of(message', message))
Act answer
  Actor person
  Recipient person'
  Related to message
  Holds when said(person', message) && person != person'
  Creates answered(message).
+person("ann"). +person("bob"). +person("cy").
"#;

const PEOPLE: [&str; 3] = ["ann", "bob", "cy"];

fn s(x: &str) -> Value {
    Value::Str(x.to_string())
}

/// The record: each message held, said by someone and asked about; every second one answers the one before it, said by
/// someone else; every tenth is a piece of the one before it, which is then asked about again and is settled.
fn lines(n: usize) -> Vec<Event> {
    let mut out = Vec::new();
    let mut m = 0i64;
    while out.len() < n {
        m += 1;
        let (who, other) = (PEOPLE[m as usize % 3], PEOPLE[(m as usize + 1) % 3]);
        out.push(Event::Fact { name: "message".into(), objects: vec![Value::Int(m)], holds: Some(true) });
        out.push(Event::Fact { name: "said".into(), objects: vec![s(who), Value::Int(m)], holds: Some(true) });
        out.push(Event::Phrase(format!("?unanswered(message({m})).")));
        if m % 10 == 0 {
            out.push(Event::Fact { name: "piece-of".into(), objects: vec![Value::Int(m), Value::Int(m - 1)], holds: Some(true) });
            out.push(Event::Phrase(format!("?unanswered(message({})).", m - 1)));
        }
        if m % 2 == 0 {
            let said_by = PEOPLE[(m as usize - 1) % 3];
            out.push(Event::Act { name: "answer".into(), actor: s(other), objects: vec![s(said_by), Value::Int(m - 1)] });
        }
    }
    out.truncate(n);
    out
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

#[test]
fn a_line_late_in_a_long_record_costs_what_it_did_early() {
    const N: usize = 20_000;
    const WINDOW: usize = 1_000;
    // guided, as a record's lines are taken
    let mut r = Replay::from_text_as(RULES, eflint::compile::Reading { guided: true, ranging: false }).unwrap();
    let events = lines(N);
    let mut took = Vec::with_capacity(N);
    let (mut asked, mut yes) = (0, 0);
    for (n, e) in events.iter().enumerate() {
        let t = Instant::now();
        let a = r.perform(e);
        took.push(t.elapsed());
        assert!(a.ok, "line {} {e:?}: {:?}", n + 1, a.errors);
        asked += a.queries.len();
        yes += a.queries.iter().filter(|q| **q).count();
    }
    // the record does what it is for: questions asked, answered both ways
    assert!(asked > 4_000 && yes > 0 && yes < asked, "{yes} of {asked} questions answered yes");
    // each kind of line apart, so a cheap kind does not hide a dear one: a fact added, a question, an act performed
    let kind = |e: &Event| match e {
        Event::Fact { .. } => "a fact added",
        Event::Phrase(_) => "a question",
        _ => "an act performed",
    };
    for k in ["a fact added", "a question", "an act performed"] {
        let of = |from: usize, to: usize| {
            median((from..to).filter(|&i| kind(&events[i]) == k).map(|i| took[i]).collect())
        };
        let (early, late) = (of(200, 200 + WINDOW), of(N - WINDOW, N));
        println!("{k}: {early:?} at lines 200 to {}, {late:?} at the last {WINDOW} of {N}", 200 + WINDOW);
        // a cost that grows with the record is many times over this at line 20,000; the margin is for a busy machine
        assert!(late <= early * 3 + Duration::from_micros(100), "{k}: {late:?} late against {early:?} early");
    }
}

/// A question whose `Exists` or `Count` opens with two facts, the second given a person, costs what it did with a few
/// texts standing when many stand, guided: it goes over the held instances the facts give, not every pair of texts.
#[test]
fn a_question_opening_with_two_facts_costs_the_same_with_many_texts_standing() {
    const TEXTS: &str = r#"
Fact person Identified by String
Fact text Identified by String
Fact watch Identified by person * text
Fact changes Identified by person * text.
+watch(person("cy"), text("a1")). +watch(person("cy"), text("b9")).
"#;
    let mut r = Replay::from_text_as(TEXTS, eflint::compile::Reading { guided: true, ranging: true }).unwrap();
    let add = |r: &mut Replay, who: &str, path: String| {
        let a = r.perform(&Event::Fact { name: "changes".into(), objects: vec![s(who), s(&path)], holds: Some(true) });
        assert!(a.ok, "{:?}", a.errors);
    };
    for k in 0..3 {
        add(&mut r, "ann", format!("a{k}"));
    }
    let questions = [
        r#"?(Exists person, text, text' : watch(person, text) && changes(person("ann"), text') && text == text')."#,
        r#"?Count(Foreach text, text' : text When watch(person("cy"), text) && changes(person("ann"), text')) == 6."#,
    ];
    let ask = |r: &mut Replay| {
        let took = (0..20).map(|_| {
            let t = Instant::now();
            let answers: Vec<bool> = questions.iter().flat_map(|q| r.perform(&Event::Phrase(q.to_string())).queries).collect();
            let took = t.elapsed();
            assert_eq!(answers, [true, true]);
            took
        });
        median(took.collect())
    };
    let early = ask(&mut r);
    for k in 0..300 {
        add(&mut r, "bob", format!("b{k}"));
    }
    let late = ask(&mut r);
    println!("two questions: {early:?} with 4 texts standing, {late:?} with 303");
    // every pair of texts is thousands of times the pairs at first; the margin is for a busy machine
    assert!(late <= early * 3 + Duration::from_micros(100), "{late:?} late against {early:?} early");
}

/// The bindings of a question that opens with a fact given a shelf cost what they did with a few books standing when
/// thousands stand in other shelves, guided: they come from that shelf's instances through the index of instances by
/// field, not from every book.
#[test]
fn the_bindings_of_a_question_given_a_shelf_cost_the_same_with_many_books_standing() {
    const BOOKS: &str = r#"
Fact shelf Identified by String
Fact book Identified by String
Fact shelved Identified by book * shelf.
"#;
    let mut r = Replay::from_text_as(BOOKS, eflint::compile::Reading { guided: true, ranging: true }).unwrap();
    let shelve = |r: &mut Replay, n: String, t: String| {
        let a = r.perform(&Event::Fact { name: "shelved".into(), objects: vec![s(&n), s(&t)], holds: Some(true) });
        assert!(a.ok, "{:?}", a.errors);
    };
    for k in 0..5 {
        shelve(&mut r, format!("a{k}"), "t1".into());
    }
    let question = match &eflint::parser::parse_statements(r#"?shelved(book, shelf("t1"))."#).unwrap()[..] {
        [Statement::Phrase(Phrase::Query(t))] => t.clone(),
        other => panic!("not a query: {other:?}"),
    };
    let book = [Var::plain(&id("book"))];
    let ask = |r: &mut Replay| {
        let took = (0..20).map(|_| {
            let t = Instant::now();
            let rows = r.ways(&book, &question).unwrap();
            let took = t.elapsed();
            assert_eq!(rows.len(), 5);
            took
        });
        median(took.collect())
    };
    let early = ask(&mut r);
    for k in 0..3000 {
        shelve(&mut r, format!("b{k}"), format!("t{}", 2 + k % 50));
    }
    let late = ask(&mut r);
    println!("one shelf's books: {early:?} with 5 books standing, {late:?} with 3,005");
    // every book is hundreds of times the shelf's books; the margin is for a busy machine
    assert!(late <= early * 3 + Duration::from_micros(100), "{late:?} late against {early:?} early");
}

/// A step kept, taken back and made again, as the normative core keeps one, and a step the closing rule refuses, cost
/// about the same with 2,000 duties open as with 200, guided and ranging: a duty's rule and the fact its violation reads
/// are worked out for the instances a step's changes reach, and the closing rule reads the duties those changes reach.
#[test]
fn a_step_costs_the_same_with_many_duties_open() {
    const CHANGES: &str = r#"
Fact worker Identified by String
Fact change Identified by String
Fact submitted Identified by change * worker
Fact reviews Identified by change * worker
Fact landed Identified by change
Fact reported Identified by change
Fact green Identified by change
  Holds when (Exists worker : submitted(change, worker))
Duty owes
  Holder worker
  Claimant worker'
  Related to change
  Holds when submitted(change, worker) && reviews(change, worker') && Not(landed(change))
  Violated when reported(change) && Not(green(change))
Act submit
  Actor worker
  Recipient worker'
  Related to change
  Creates submitted(change, worker), reviews(change, worker')
Act withdraw
  Actor worker
  Related to change
  Conditioned by submitted(change, worker)
  Terminates submitted(change, worker).
+worker("w1"). +worker("w2").
"#;
    const N: usize = 2_000;
    let mut r = Replay::from_text_as(CHANGES, eflint::compile::Reading { guided: true, ranging: true }).unwrap();
    let w1 = Elem::Str(id("w1"));
    let act = |name: &str, objects: &[&str]| Event::Act { name: name.into(), actor: s("w1"), objects: objects.iter().map(|o| s(o)).collect() };
    let refused = |r: &mut Replay| {
        let took = (0..9).map(|_| {
            let t = Instant::now();
            let refusal = r.step(&act("withdraw", &["c0"]), Some(&w1));
            let took = t.elapsed();
            assert!(matches!(refusal, Err(Refusal::Closes(_))), "{refusal:?}");
            took
        });
        median(took.collect())
    };
    let (mut kept, mut refusals) = (Vec::with_capacity(N), Vec::new());
    for k in 0..N {
        let change = format!("c{k}");
        let t = Instant::now();
        let taken = r.step(&act("submit", &["w2", change.as_str()]), Some(&w1)).unwrap();
        r.undo(&taken);
        r.redo(&taken);
        kept.push(t.elapsed());
        if k + 1 == 200 || k + 1 == N {
            refusals.push(refused(&mut r));
        }
    }
    assert_eq!(r.duties_of(&w1).unwrap().len(), N);
    let (early, late) = (median(kept[100..200].to_vec()), median(kept[N - 100..].to_vec()));
    println!("a step kept, taken back and made again: {early:?} with 100 to 200 duties open, {late:?} with {} to {N}", N - 100);
    println!("a step refused by the closing rule: {:?} with 200 duties open, {:?} with {N}", refusals[0], refusals[1]);
    // a step that works every duty out again is ten times this at 2,000; the margin is for a busy machine
    assert!(late <= early * 3 + Duration::from_micros(100), "kept: {late:?} late against {early:?} early");
    assert!(refusals[1] <= refusals[0] * 3 + Duration::from_micros(100), "refused: {:?} late against {:?} early", refusals[1], refusals[0]);
}
