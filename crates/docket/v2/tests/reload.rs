//! A reload (spec 11) on small texts: each run that has not ended is replaced by a run of the new text replayed
//! against its lines, which takes over the Effect the old run rests on, or parts and drops it.

mod common;

use common::*;
use docket_v2::run::driver::Reply;
use docket_v2::run::machine::Machine;
use docket_v2::run::value::{Instance, Value};
use std::sync::mpsc::Receiver;

const TEXT: &str = "module t\nEvent Seen()\nEffect Work(n: Int) -> Int\nEffect Slow() -> Int\nEffect Other() -> Int\nEffect Nobody() -> Int\non Seen() do { y <- perform Slow(); return () }";
const P1: &str = "Returns Int Does { a <- perform Work(1); b <- perform Slow(); return a + b }";

/// A machine on `programs` whose `Slow` is answered late, on the channel given back; a connection's run and a
/// reaction's each resting on it.
fn resting(programs: &[&str]) -> (Machine<Facts>, Asked, Receiver<(u64, Reply)>) {
    let (mut m, asked, rx) = linked(programs);
    m.start(Value::text("w1"), Some(0), "", Instance { ty: "t.P1".into(), args: vec![Value::text("w1")] }).unwrap();
    m.run();
    m.event(0, Instance { ty: "t.Seen".into(), args: vec![] });
    m.run();
    assert_eq!(*asked.borrow(), ["t.Work", "t.Slow", "t.Slow"]);
    asked.borrow_mut().clear();
    (m, asked, rx)
}

/// A machine on `programs` with a driver that answers `Slow` late, on the channel given back.
fn linked(programs: &[&str]) -> (Machine<Facts>, Asked, Receiver<(u64, Reply)>) {
    linked_on(TEXT, programs, Facts::default())
}

fn linked_on(text: &str, programs: &[&str], facts: Facts) -> (Machine<Facts>, Asked, Receiver<(u64, Reply)>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let asked: Asked = Default::default();
    let answers = vec![("t.Work", Value::Int(1), vec![]), ("t.Slow", Value::text("later"), vec![]), ("t.Other", Value::Int(2), vec![])];
    let d = Table { name: vec![Value::text("stand-in"), Value::text("1")], answers, asked: asked.clone(), late: Some(tx) };
    (Machine::new(loaded(text, programs), facts, vec![Box::new(d)]).unwrap(), asked, rx)
}

/// Act `Pk` started as party `w1` and run.
fn start(m: &mut Machine<Facts>, k: usize) {
    m.start(Value::text("w1"), Some(0), "", Instance { ty: format!("t.P{k}"), args: vec![Value::text("w1")] }).unwrap();
    m.run();
}

/// Reload with `programs`; the lines written.
fn reload(m: &mut Machine<Facts>, programs: &[&str]) -> Vec<String> {
    reload_on(m, TEXT, programs)
}

fn reload_on(m: &mut Machine<Facts>, text: &str, programs: &[&str]) -> Vec<String> {
    let from = m.record.len();
    m.reload(loaded(text, programs)).unwrap();
    m.record[from..].iter().map(|l| l.to_string()).collect()
}

/// The late answers given, each as the daemon gives it, then the machine run; one whose run is gone is refused.
/// The lines written.
fn answer(m: &mut Machine<Facts>, rx: &Receiver<(u64, Reply)>) -> Vec<String> {
    let from = m.record.len();
    for (line, reply) in rx.try_iter() {
        if m.answer(line as usize, reply).is_ok() {
            m.run();
        }
    }
    m.record[from..].iter().map(|l| l.to_string()).collect()
}

#[test]
fn an_unchanged_text_reloaded_asks_no_driver_again_and_the_answer_reaches_the_new_run() {
    let (mut m, asked, rx) = resting(&[P1]);
    let lines = reload(&mut m, &[P1]);
    assert_eq!(lines[1..], ["cancelled(r1, reload)", "began(r12, \"w1\")", "cancelled(r8, reload)", "began(r14, \"t\")"], "{lines:?}");
    assert!(asked.borrow().is_empty(), "{:?}", asked.borrow());
    let lines = answer(&mut m, &rx);
    assert!(lines.contains(&"ended(r12, 10)".to_string()) && lines.contains(&"ended(r14, ())".to_string()), "{lines:?}");
    assert!(asked.borrow().is_empty(), "{:?}", asked.borrow());
}

#[test]
fn a_text_changed_before_where_a_run_stands_parts_there_and_drops_its_line_and_one_changed_after_changes_nothing() {
    let (mut m, asked, _rx) = resting(&[P1]);
    reload(&mut m, &["Returns Int Does { a <- perform Work(2); b <- perform Slow(); return a + b }"]);
    assert_eq!(*asked.borrow(), ["dropped 6", "t.Work", "t.Slow"], "the reaction, unchanged, takes its line over");
    let (mut m, asked, rx) = resting(&[P1]);
    reload(&mut m, &["Returns Int Does { a <- perform Work(1); b <- perform Slow(); c <- perform Other(); return a + b + c }"]);
    assert!(asked.borrow().is_empty(), "{:?}", asked.borrow());
    let lines = answer(&mut m, &rx);
    assert_eq!(*asked.borrow(), ["t.Other"]);
    assert!(lines.contains(&"ended(r12, 12)".to_string()), "{lines:?}");
}

#[test]
fn a_replacement_that_cannot_start_leaves_its_run_cancelled_with_why() {
    let (mut m, asked, _rx) = resting(&[P1]);
    let lines = reload(&mut m, &["Returns Int Does { perform Nobody() }"]);
    assert!(lines[1].starts_with("cancelled(r1, reload: t.P1 may perform t.Nobody"), "{lines:?}");
    assert!(!lines.iter().any(|l| l.starts_with("began(r12")), "{lines:?}");
    assert_eq!(*asked.borrow(), ["dropped 6"]);
}

#[test]
fn a_record_with_reloads_restores_line_for_line() {
    let v2 = ["Returns Int Does { a <- perform Work(2); b <- perform Slow(); return a + b }"];
    let (mut old, _, rx) = resting(&[P1]);
    reload(&mut old, &v2);
    answer(&mut old, &rx);
    let (mut m, asked, _) = linked(&v2);
    let first = loaded(TEXT, &[P1]);
    m.restore(&old.record, &|_, _| (Some(0), String::new()), &mut |h| if *h == first.hash { Ok(first.clone()) } else { Err("no text kept".into()) }).unwrap();
    assert_eq!(m.record, old.record);
    assert!(asked.borrow().is_empty(), "{:?}", asked.borrow());
}

#[test]
fn a_reactions_replacement_binds_in_the_state_as_it_stood_at_its_performed_line() {
    let text = format!("{TEXT}\nFact Done(w: String)\nAct Mark(by: String)\nAct Unmark(by: String)\non Seen() where Done(w) do {{ y <- perform Slow(); return () }}");
    let facts = Facts { acts: vec![("t.Mark", |a, _| Ok((vec![Instance { ty: "t.Done".into(), args: a.args.clone() }], vec![]))), ("t.Unmark", |a, _| Ok((vec![], vec![Instance { ty: "t.Done".into(), args: a.args.clone() }])))], ..Facts::default() };
    let programs = ["{ Mark(by) }", "{ Unmark(by) }"];
    let (mut m, asked, rx) = linked_on(&text, &programs, facts);
    start(&mut m, 1);
    m.event(0, Instance { ty: "t.Seen".into(), args: vec![] });
    m.run();
    // what bound it holds no longer
    start(&mut m, 2);
    assert_eq!(*asked.borrow(), ["t.Slow", "t.Slow"]);
    asked.borrow_mut().clear();
    let lines = reload_on(&mut m, &text, &programs);
    assert!(asked.borrow().is_empty(), "{:?} {lines:?}", asked.borrow());
    let lines = answer(&mut m, &rx);
    assert_eq!(lines.iter().filter(|l| l.starts_with("ended(") && l.ends_with(", ())")).count(), 2, "{lines:?}");
}

#[test]
fn a_reactions_condition_that_cannot_be_evaluated_takes_its_place_among_the_modules_runs() {
    let then = format!("{TEXT}\nFact Broken(x: String)\non Seen() do {{ return () }}\non Seen() do {{ y <- perform Slow(); return () }}");
    let now = format!("{TEXT}\nFact Broken(x: String)\non Seen() where Broken(x) do {{ return () }}\non Seen() do {{ y <- perform Slow(); return () }}");
    let (mut m, asked, rx) = linked_on(&then, &[], Facts::default());
    m.event(0, Instance { ty: "t.Seen".into(), args: vec![] });
    m.run();
    assert_eq!(*asked.borrow(), ["t.Slow", "t.Slow"]);
    asked.borrow_mut().clear();
    let lines = reload_on(&mut m, &now, &[]);
    assert!(asked.borrow().is_empty(), "the second reaction's run is replaced and takes its line over: {:?} {lines:?}", asked.borrow());
    let lines = answer(&mut m, &rx);
    assert_eq!(lines.iter().filter(|l| l.starts_with("ended(") && l.ends_with(", ())")).count(), 2, "{lines:?}");
}
