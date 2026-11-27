//! Structural prefix replay (spec 11) on small texts: what a run following an old run's lines reuses, what it
//! reads, and where it goes live.

mod common;

use common::*;
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::machine::Machine;
use docket_v2::run::norms::Why;
use docket_v2::run::record::Line;
use docket_v2::run::value::{Instance, Value};
use std::cell::RefCell;
use std::rc::Rc;

const TEXT: &str = "module t\nFact Done(w: String)\nAct Mark(by: String)\nAct Deny(by: String)\nEvent Seen()\nEvent Gone()\nEffect Work(n: Int) -> Int\nEffect Other() -> Int";

fn facts() -> Facts {
    Facts { acts: vec![("t.Mark", |a, _| Ok((vec![Instance { ty: "t.Done".into(), args: a.args.clone() }], vec![]))), ("t.Deny", |_, _| Err(Why::Policy("not let".into())))], ..Facts::default() }
}

fn answers() -> Vec<(&'static str, Value, Vec<(String, i64)>)> {
    vec![("t.Work", Value::Int(1), vec![("tokens".into(), 5)]), ("t.Other", Value::Int(2), vec![])]
}

/// A machine on the text with `programs` changed, over the same record and core: a reload (5.5).
fn reloaded(old: &mut Machine<Facts>, programs: &[&str]) -> (Machine<Facts>, Asked) {
    let (mut m, asked) = machine(TEXT, programs, std::mem::take(&mut old.norms), answers());
    m.record = std::mem::take(&mut old.record);
    (m, asked)
}

/// Replay Act `Pk` against the run that began last, to its end; the lines it writes, its own name written `r2`.
fn replay(m: &mut Machine<Facts>, k: usize, folder: &str) -> Vec<String> {
    replay_as(m, "r2", k, folder)
}

fn replay_as(m: &mut Machine<Facts>, label: &str, k: usize, folder: &str) -> Vec<String> {
    let old = m.record.iter().rev().find_map(|l| match l {
        Line::Began { run, .. } => Some(run.clone()),
        _ => None,
    });
    let old = old.unwrap();
    let from = m.record.len();
    let r = m.replay(&old, Value::text("w1"), Some(0), folder, Instance { ty: format!("t.P{k}"), args: vec![Value::text("w1")] }).unwrap();
    m.run();
    labelled(m, from, r, label)
}

#[test]
fn a_replay_reuses_each_recorded_answer_and_act_and_goes_live_at_the_first_that_differs() {
    let p1 = "Returns Int Does { a <- perform Work(1); Mark(by); b <- perform Work(2); return a + b }";
    let p2 = "Returns Int Does { a <- perform Work(1); Mark(by); b <- perform Work(2); c <- perform Other(); return a + b + c }";
    let (mut old, _) = machine(TEXT, &[p1], facts(), answers());
    run(&mut old, 1);
    let (mut m, asked) = reloaded(&mut old, &[p2]);
    let lines = replay(&mut m, 1, "");
    assert_eq!(*asked.borrow(), ["t.Other"], "only the Effect past the prefix is carried out");
    assert!(!lines.iter().any(|l| l.starts_with("performed(") || l.contains("\"tokens\", 5")), "the prefix writes no act and no charge again: {lines:?}");
    assert_eq!(lines.last().unwrap(), "ended(r2, 4)", "{lines:?}");
}

#[test]
fn a_replay_reads_the_state_as_it_stood_at_each_old_line() {
    // the run tests a fact and then sets it: read as it stands now, the test would part from the record at once
    let (mut m, asked) = machine(TEXT, &["{ if Not(Done(by)) { Mark(by) }; x <- perform Work(1); return () }"], facts(), answers());
    run(&mut m, 1);
    asked.borrow_mut().clear();
    let lines = replay(&mut m, 1, "");
    assert!(asked.borrow().is_empty(), "the whole run matched: {:?} {lines:?}", asked.borrow());
    assert_eq!(lines.last().unwrap(), "ended(r2, ())", "{lines:?}");
}

#[test]
fn a_replay_keys_by_the_recorded_head_and_puts_the_tree_where_it_parts() {
    let p1 = "Returns Int Does { a <- perform Work(1); b <- perform Work(2); return a + b }";
    let p2 = "Returns Int Does { a <- perform Work(1); b <- perform Work(3); return a + b }";
    let (mut m, _) = machine(TEXT, &[p1], facts(), answers());
    let (reads, puts) = (Rc::new(RefCell::new(0)), Rc::new(RefCell::new(Vec::new())));
    let (n, p) = (reads.clone(), puts.clone());
    m.head = Box::new(move |f| {
        *n.borrow_mut() += 1;
        format!("{f}@{}", n.borrow())
    });
    m.put = Box::new(move |f, h| {
        p.borrow_mut().push(format!("{f} {h}"));
        Ok(())
    });
    m.start(Value::text("w1"), Some(0), "/w", Instance { ty: "t.P1".into(), args: vec![Value::text("w1")] }).unwrap();
    m.run();
    let (mut m2, asked) = reloaded(&mut m, &[p2]);
    (m2.head, m2.put) = (std::mem::replace(&mut m.head, Box::new(|_| String::new())), std::mem::replace(&mut m.put, Box::new(|_, _| Ok(()))));
    replay(&mut m2, 1, "/w");
    assert_eq!(*puts.borrow(), ["/w /w@2"], "the tree as it stood at the old line where the runs part");
    assert_eq!(*reads.borrow(), 3, "the tree is not read in the prefix, and read live after it");
    assert_eq!(*asked.borrow(), ["t.Work"]);
}

#[test]
fn each_branch_of_a_par_follows_the_old_branch_at_its_place() {
    let (mut m, asked) = machine(TEXT, &["Returns (Int, Int) Does { par(perform Work(1), perform Work(2)) }"], facts(), answers());
    run(&mut m, 1);
    asked.borrow_mut().clear();
    let lines = replay(&mut m, 1, "");
    assert!(asked.borrow().is_empty(), "{:?}", asked.borrow());
    assert_eq!(lines.last().unwrap(), "ended(r2, (1, 1))", "{lines:?}");
}

#[test]
fn a_replay_that_ends_before_its_old_lines_leaves_the_state_as_it_stands() {
    let (mut old, _) = machine(TEXT, &["{ Mark(by); x <- perform Work(1); return () }"], facts(), answers());
    run(&mut old, 1);
    let (mut m, _) = reloaded(&mut old, &["{ Mark(by); return () }"]);
    replay(&mut m, 1, "");
    m.event(0, Instance { ty: "t.Seen".into(), args: vec![] });
    assert!(m.record.last().unwrap().to_string().starts_with("performed(t.Seen()"), "a driver's Event after it is committed as Σ stands");
}

#[test]
fn the_first_live_effect_line_keeps_what_the_run_read_and_a_replay_of_a_replay_follows_both() {
    let read = "{ if Not(Done(by)) { Mark(by) }; x <- perform Work(1); return () }";
    let changed = "{ if Not(Done(by)) { Mark(by) }; x <- perform Work(2); return () }";
    let (mut old, _) = machine(TEXT, &[read], facts(), answers());
    run(&mut old, 1);
    let (mut m, asked) = reloaded(&mut old, &[changed]);
    replay(&mut m, 1, "");
    assert_eq!(*asked.borrow(), ["t.Work"]);
    let (mut again, asked) = reloaded(&mut m, &[changed]);
    let lines = replay_as(&mut again, "r3", 1, "");
    assert!(asked.borrow().is_empty(), "r3 follows r's prefix and r2's live line: {:?} {lines:?}", asked.borrow());
    assert_eq!(lines.last().unwrap(), "ended(r3, ())", "{lines:?}");
}

#[test]
fn a_run_raised_inside_its_prefix_writes_where_it_ends_and_its_replay_follows_it() {
    let p = "{ x <- perform Work(1); Deny(by) }";
    let (mut old, _) = machine(TEXT, &[p], facts(), answers());
    run(&mut old, 1);
    let (mut m, _) = reloaded(&mut old, &[p]);
    let lines = replay(&mut m, 1, "");
    assert!(lines.iter().any(|l| l.starts_with("prefix(r2, r1, ")) && lines.last().unwrap().starts_with("raised(r2, Refused"), "{lines:?}");
    let (mut again, asked) = reloaded(&mut m, &[p]);
    let lines = replay_as(&mut again, "r3", 1, "");
    assert!(asked.borrow().is_empty(), "{:?} {lines:?}", asked.borrow());
}

#[test]
fn a_record_replayed_exactly_on_a_fresh_machine_is_written_again_and_its_resting_run_asks_again() {
    let text = format!("{TEXT}\nEffect Slow() -> Int\non Seen() do {{ y <- perform Work(5); return () }}");
    let programs = ["Returns (Int, Int) Does { n <- perform Now(); Mark(by); par(perform Work(1), perform Work(2)) }", "{ x <- perform Other(); Deny(by) }", "Returns Int Does { perform Slow() }"];
    let mut with_slow = answers();
    with_slow.push(("t.Slow", Value::text("later"), vec![]));
    let start = |m: &mut Machine<Facts>, k: usize| {
        let r = m.start(Value::text("w1"), Some(0), "", Instance { ty: format!("t.P{k}"), args: vec![Value::text("w1")] }).unwrap();
        m.run();
        m.run_name(r).to_string()
    };
    let (mut old, _) = machine(&text, &programs, facts(), with_slow.clone());
    start(&mut old, 1);
    old.event(0, Instance { ty: "t.Seen".into(), args: vec![] });
    old.run();
    let b = start(&mut old, 2);
    start(&mut old, 3);
    // a late answer, with its cost, comes between other lines
    let line = old.record.iter().rposition(|l| l.to_string().starts_with("effect(t.Slow()")).unwrap();
    old.answer(line, Reply::Answer { value: Value::Int(7), costs: vec![("tokens".into(), 3)], events: vec![] }).unwrap();
    old.run();
    old.replay(&b, Value::text("w1"), Some(0), "", Instance { ty: "t.P2".into(), args: vec![Value::text("w1")] }).unwrap();
    old.run();
    start(&mut old, 3);
    let (mut m, asked) = machine(&text, &programs, facts(), with_slow);
    m.restore(&old.record, &|_, _| (Some(0), String::new()), &mut |_| Err("the text is the machine's own".into())).unwrap();
    assert_eq!(m.record, old.record);
    assert_eq!(m.norms.held, old.norms.held, "Σ is rebuilt");
    assert_eq!(*asked.borrow(), ["t.Slow"], "no answered Effect is carried out again; the resting one is asked again");
}

#[test]
fn an_exact_replay_loads_each_text_the_record_names_reads_no_tree_and_puts_none() {
    let v1 = ["Returns Int Does { a <- perform Work(1); b <- perform Work(2); return a + b }"];
    let v2 = ["Returns Int Does { a <- perform Work(1); b <- perform Work(3); return a + b }"];
    let refusing = || Facts { acts: vec![("t.Seen", |_, _| Err(Why::Policy("not now".into())))], ..Facts::default() };
    let puts = Rc::new(RefCell::new(Vec::new()));
    let (mut old, _) = machine(TEXT, &v1, refusing(), answers());
    old.head = Box::new(|f| format!("{f}@then"));
    old.put = putting(&puts);
    let a = old.start(Value::text("w1"), Some(0), "/w", Instance { ty: "t.P1".into(), args: vec![Value::text("w1")] }).unwrap();
    let a = old.run_name(a).to_string();
    old.run();
    old.event(0, Instance { ty: "t.Seen".into(), args: vec![] });
    old.reload(loaded(TEXT, &v2)).unwrap();
    old.replay(&a, Value::text("w1"), Some(0), "/w", Instance { ty: "t.P1".into(), args: vec![Value::text("w1")] }).unwrap();
    old.run();
    assert_eq!(puts.borrow().len(), 1, "the replay that parted put the tree once");
    let (mut m, asked) = machine(TEXT, &v2, refusing(), answers());
    m.head = Box::new(|f| format!("{f}@now"));
    m.put = putting(&puts);
    let first = loaded(TEXT, &v1);
    m.restore(&old.record, &|_, _| (Some(0), "/w".into()), &mut |h| if *h == first.hash { Ok(first.clone()) } else { Err("no text kept".into()) }).unwrap();
    assert_eq!(m.record, old.record, "the heads are the record's, and the refused Event is refused again");
    assert_eq!(puts.borrow().len(), 1, "no tree is put");
    assert!(asked.borrow().is_empty());
}

/// A `put` that only writes down where it was asked to put a tree.
fn putting(puts: &Rc<RefCell<Vec<String>>>) -> Box<dyn FnMut(&str, &str) -> Result<(), String>> {
    let p = puts.clone();
    Box::new(move |f, h| {
        p.borrow_mut().push(format!("{f} {h}"));
        Ok(())
    })
}

#[test]
fn an_exact_replay_takes_each_drivers_event_as_its_line_has_it_and_checks_no_old_text_against_todays_drivers() {
    let then = format!("{TEXT}\non Seen() do {{ y <- perform Work(5); return () }}");
    let (mut old, _) = machine(&then, &[], facts(), answers());
    old.event(0, Instance { ty: "t.Seen".into(), args: vec![] });
    old.run();
    // an Event its driver does not name: refused at its source
    old.event(0, Instance { ty: "t.Gone".into(), args: vec![] });
    assert!(old.record.last().unwrap().to_string().starts_with("refused(t.Gone()"));
    let first = loaded(&then, &[]);
    let mut m = Machine::new(loaded(TEXT, &[]), facts(), Vec::new()).unwrap();
    m.restore(&old.record, &|_, _| (None, String::new()), &mut |h| if *h == first.hash { Ok(first.clone()) } else { Err("no text kept".into()) }).unwrap();
    assert_eq!(m.record[..old.record.len()], old.record[..], "with no driver linked, and the old reaction's Effect answered by none now");
}

/// A driver whose `Work` costs in two dimensions and whose `Log` costs nothing.
struct Costly;

impl Driver for Costly {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("stand-in"), Value::text("1")]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        vec![("t.Work".into(), vec!["tokens".into(), "watts".into()]), ("t.Log".into(), vec![])]
    }
    fn events(&self) -> Vec<String> {
        Vec::new()
    }
    fn carry(&mut self, op: &str, _args: &[Value], _line: u64) -> Reply {
        let costs = if op == "t.Work" { vec![("tokens".into(), 2), ("watts".into(), 3)] } else { Vec::new() };
        Reply::Answer { value: Value::Int(5), costs, events: Vec::new() }
    }
    fn dropped(&mut self, _line: u64) {}
}

#[test]
fn an_answers_costs_stand_right_after_it_and_restore_whole_when_a_charge_handler_performs_an_effect() {
    let text = "module t\nEffect Work(n: Int) -> Int\nEffect Log(n: Int) -> Int";
    let arm = "with { | Charge(d, n), k => { y <- perform Log(n); resume k(()) } }";
    // the handler in the run that performed, and in the run a `par` branch rests under
    let programs = [format!("Returns Int Does {{ handle {{ perform Work(1) }} {arm} }}"), format!("Returns List<Int> Does {{ handle {{ par x in [1] up to 1 {{ perform Work(x) }} }} {arm} }}")];
    let programs: Vec<&str> = programs.iter().map(String::as_str).collect();
    let fresh = || Machine::new(loaded(text, &programs), Facts::default(), vec![Box::new(Costly)]).unwrap();
    for k in [1, 2] {
        let mut old = fresh();
        let lines = run(&mut old, k);
        let at = lines.iter().position(|l| l.starts_with("answered(")).unwrap();
        assert!(lines[at + 1].contains("\"tokens\", 2)") && lines[at + 2].contains("\"watts\", 3)"), "{lines:?}");
        assert!(lines[at + 3..].iter().any(|l| l.starts_with("effect(t.Log(")), "{lines:?}");
        let mut m = fresh();
        m.restore(&old.record, &|_, _| (Some(0), String::new()), &mut |_| Err("one text".into())).unwrap();
        assert_eq!(m.record, old.record, "program {k}");
    }
}
