//! The machine's steps of meaning (spec 8) on small texts, with a stand-in normative core and a stand-in driver.

mod common;

use common::*;
use docket_v2::run::machine::{Machine, State};
use docket_v2::run::value::{Instance, Value};

#[test]
fn a_deep_handler_resumes_each_perform_and_its_return_clause_takes_the_result() {
    let (mut m, _) = machine("module t\nEffect Ask() -> Int", &["Returns Int Does { handle { x <- perform Ask(); y <- perform Ask(); return x + y } with { return v => return v * 10 | Ask(), k => resume k(2) } }"], Facts::default(), vec![]);
    let lines = run(&mut m, 1);
    assert_eq!(lines.last().unwrap(), "ended(r, 40)", "{lines:?}");
}

#[test]
fn an_arm_that_does_not_resume_gives_the_handlers_value_and_drops_the_rest() {
    let (mut m, asked) = machine("module t\nEffect Ask() -> Int\nEffect Out() -> Int", &["Returns Int Does { handle { x <- perform Ask(); perform Out() } with { | Ask(), k => return 7 } }"], Facts::default(), vec![("t.Out", Value::Int(1), vec![])]);
    let lines = run(&mut m, 1);
    assert_eq!(lines.last().unwrap(), "ended(r, 7)");
    assert!(asked.borrow().is_empty(), "what came after the dropped k was never carried out");
}

#[test]
fn a_perform_goes_to_a_root_arm_then_the_effects_does_then_a_driver_and_costs_reach_a_charge_arm() {
    let text = "module t\nEffect Pick(n: Int) -> Int Does { return 0 }\nEffect Work() -> Int\non Pick(1) do { return 100 }";
    let (mut m, _) = machine(text, &["Returns Int Does { a <- perform Pick(1); b <- perform Pick(2); handle { perform Work() } with { return w => return a + b + w | Charge(d, n), k => resume k(()) } }"], Facts::default(), vec![("t.Work", Value::Int(5), vec![("tokens".into(), 30)])]);
    let lines = run(&mut m, 1);
    assert_eq!(lines.last().unwrap(), "ended(r, 105)", "{lines:?}");
    assert!(lines.iter().any(|l| l.starts_with("effect(t.Work(), r,")) && lines.iter().any(|l| l.starts_with("answered(")) && lines.contains(&"charged(r, \"tokens\", 30)".to_string()), "{lines:?}");
}

#[test]
fn an_act_whose_actor_is_not_the_runs_party_is_refused_by_its_source_and_ends_the_run() {
    let mut f = Facts::default();
    f.acts.push(("t.Mark", |a, _| Ok((vec![inst("t.Marked", &[match &a.args[1] {
        Value::Text(s) => s,
        _ => "",
    }])], vec![]))));
    let (mut m, _) = machine("module t\nFact W Identified by String\nFact Marked Identified by String\nAct Mark(by: W, x: String)", &["{ Mark(\"w2\", \"a\") }", "Returns Bool Does { Mark(self, \"a\"); return Holds(Marked(\"a\")) }"], f, vec![]);
    let lines = run(&mut m, 1);
    assert!(lines.iter().any(|l| l.starts_with("refused(t.Mark(\"w2\", \"a\"), \"w1\", r, Source)")), "{lines:?}");
    assert!(lines.last().unwrap().starts_with("raised(r, Refused("), "{lines:?}");
    let lines = run(&mut m, 2);
    assert!(lines.iter().any(|l| l.starts_with("performed(t.Mark(\"w1\", \"a\")")), "{lines:?}");
}

#[test]
fn par_gives_its_branches_values_in_order_and_an_arm_outside_it_that_drops_k_cancels_them() {
    let (mut m, _) = machine("module t\nEffect Ask(n: Int) -> Int", &["Returns List<Int> Does { handle { par x in [1, 2, 3] up to 2 { perform Ask(x) } } with { | Ask(n), k => resume k(n * 2) } }", "Returns List<Int> Does { handle { par x in [1, 2, 3] up to 2 { perform Ask(x) } } with { | Ask(n), k where n == 2 => return [0] | Ask(n), k => resume k(n) } }"], Facts::default(), vec![]);
    let lines = run(&mut m, 1);
    assert_eq!(lines.last().unwrap(), "ended(r, [2, 4, 6])", "{lines:?}");
    let lines = run(&mut m, 2);
    assert_eq!(lines.last().unwrap(), "ended(r, [0])", "{lines:?}");
    assert!(lines.iter().any(|l| l.starts_with("cancelled(r.")), "the other branches are cancelled: {lines:?}");
}

#[test]
fn a_gap_is_filled_by_a_checked_text_and_a_filling_that_performs_charge_is_unfit() {
    let (mut m, _) = machine("module t\nfn f() -> Program<Bool> { x <- ...; return x }", &["Returns Bool Does { do f() }"], Facts::default(), vec![("Gap", Value::text("{ return true }"), vec![])]);
    let lines = run(&mut m, 1);
    assert_eq!(lines.last().unwrap(), "ended(r, true)", "{lines:?}");
    let (mut m, _) = machine("module t\nfn f() -> Program<Bool> { x <- ...; return x }", &["Returns Bool Does { do f() }"], Facts::default(), vec![("Gap", Value::text("{ perform Charge(\"tokens\", 0 - 100); return true }"), vec![])]);
    let lines = run(&mut m, 1);
    assert!(lines.last().unwrap().starts_with("raised(r, Unfit("), "{lines:?}");
    assert!(!lines.iter().any(|l| l.contains("-100")), "no charged line with the negative amount: {lines:?}");
}

#[test]
fn a_filling_has_the_type_its_place_needs_and_performs_no_effect_nobody_answers() {
    let text = "module t\nEffect Nobody() -> Bool\nfn f() -> Program<Bool> { x <- ...; return x }";
    for (filling, why) in [("{ return 1 }", "Bool wanted, Int given"), ("{ perform Nobody() }", "Nobody, which nobody answers")] {
        let (mut m, _) = machine(text, &["Returns Bool Does { do f() }"], Facts::default(), vec![("Gap", Value::text(filling), vec![])]);
        let lines = run(&mut m, 1);
        let last = lines.last().unwrap();
        assert!(last.starts_with("raised(r, Unfit(") && last.contains(why), "{lines:?}");
    }
}

#[test]
fn two_drivers_that_carry_one_effect_are_refused_in_either_order() {
    let table = |who: &str, effects: Vec<&'static str>| -> Box<dyn docket_v2::run::driver::Driver> {
        Box::new(Table { name: vec![Value::text(who)], answers: effects.into_iter().map(|e| (e, Value::Int(1), vec![])).collect(), asked: Default::default(), late: None })
    };
    let text = "module t\nEffect Deliver() -> Int\nEffect Notify() -> Int";
    for (a, b) in [("window", "turns"), ("turns", "window")] {
        let why = Machine::new(loaded(text, &[]), Facts::default(), vec![table(a, vec!["t.Deliver"]), table(b, vec!["t.Notify", "t.Deliver"])]).err().expect("refused");
        assert_eq!(why, format!("t.Deliver is carried by two drivers, \"{a}\" and \"{b}\": one Effect has one carrier"));
    }
    assert!(Machine::new(loaded(text, &[]), Facts::default(), vec![table("window", vec!["t.Deliver"]), table("turns", vec!["t.Notify"])]).is_ok());
}

#[test]
fn iterate_runs_again_until_done_and_past_its_bound_performs_exhausted() {
    let (mut m, _) = machine("module t\nEffect X() -> Int", &["Returns Int Does { iterate i = 1 up to 5 { if i == 3 { return Done(i) } else { return Again(i + 1) } } }", "Returns Int Does { iterate i = 1 up to 2 { return Again(i + 1) } }"], Facts::default(), vec![]);
    let lines = run(&mut m, 1);
    assert_eq!(lines.last().unwrap(), "ended(r, 3)", "{lines:?}");
    let lines = run(&mut m, 2);
    assert_eq!(lines.last().unwrap(), "raised(r, Exhausted())", "{lines:?}");
}

#[test]
fn a_guarded_value_whose_power_is_not_let_does_not_cross_and_the_driver_is_never_asked() {
    let text = "module t\nFact Point Identified by String\nFact Lets(h: String, v: String, p: Point, w: String)\n+Lets(\"stand-in\", \"1\", \"here\", \"Go\").\ntype V = | Go Needs \"Go\" | Stop(why: String) Needs \"Stop\"\nEffect Advise(p: Point) -> V\nEffect Carry(p: Point, v: V) -> Bool";
    let (mut m, asked) = machine(text, &["Returns Bool Does { v <- perform Advise(\"here\"); perform Carry(\"here\", v) }", "Returns Bool Does { perform Carry(\"here\", Go) }"], Facts::default(), vec![("t.Advise", Value::ctor("t.Stop", vec![Value::text("no")]), vec![]), ("t.Carry", Value::Bool(true), vec![])]);
    let lines = run(&mut m, 1);
    assert!(lines.last().unwrap().starts_with("raised(r, Fault("), "{lines:?}");
    assert_eq!(*asked.borrow(), ["t.Advise"], "Carry was never asked");
    let lines = run(&mut m, 2);
    assert_eq!(lines.last().unwrap(), "ended(r, true)", "{lines:?}");
    assert_eq!(m.state(1), State::Done);
}

#[test]
fn a_drivers_answer_not_of_the_effects_result_type_is_a_fault_and_is_never_answered() {
    let text = "module t\ntype V = | Go | Stop(why: String)\nEffect Work() -> Int\nEffect Pick() -> V";
    let (mut m, _) = machine(text, &["Returns Int Does { perform Work() }", "Returns V Does { perform Pick() }"], Facts::default(), vec![("t.Work", Value::text("five"), vec![]), ("t.Pick", Value::ctor("t.Stop", vec![Value::Int(1)]), vec![])]);
    let lines = run(&mut m, 1);
    assert!(lines.last().unwrap().starts_with("raised(r, Fault("), "{lines:?}");
    assert!(!lines.iter().any(|l| l.starts_with("answered(")), "{lines:?}");
    let lines = run(&mut m, 2);
    assert!(lines.last().unwrap().starts_with("raised(r, Fault("), "a constructor's field of the wrong type: {lines:?}");
}

#[test]
fn a_connection_starts_a_run_only_by_an_act_and_an_event_it_asks_for_is_refused_by_source() {
    let (mut m, _) = machine("module t\nEvent Tick()", &["Returns Int Does { return 3 }"], Facts::default(), vec![]);
    let lines = run(&mut m, 1);
    assert_eq!(lines, ["began(r, \"w1\")", "performed(t.P1(\"w1\"), \"w1\", r)", "ended(r, 3)"]);
    let from = m.record.len();
    m.start(Value::text("w1"), Some(0), "", Instance { ty: "t.Tick".into(), args: vec![] }).unwrap();
    m.run();
    let lines: Vec<String> = m.record[from..].iter().map(|l| l.to_string()).collect();
    assert!(lines.iter().any(|l| l.starts_with("refused(t.Tick(), \"w1\", r4, Source)")) && lines.last().unwrap().starts_with("raised(r4, Refused("), "{lines:?}");
}

#[test]
fn a_late_answer_comes_back_by_its_lines_place_and_a_failure_is_a_fault_with_no_answered_line() {
    let text = "module t\nEffect Work() -> Int\nEffect Use() -> Int";
    let l = docket_v2::check::load(vec![docket_v2::syntax::read(&format!("{text}\nAct P1(by: String) Returns Int Does {{ perform Work() }}\nAct P2(by: String) Returns Int Does {{ perform Use() }}")).unwrap()]).unwrap();
    // the daemon's frame would wait for the late answer; here the stand-in gives it on a channel of the test's
    let (tx, rx) = std::sync::mpsc::channel();
    let d = Table { name: vec![], answers: vec![("t.Work", Value::text("later"), vec![("tokens".into(), 4)]), ("t.Use", Value::text("fails"), vec![])], asked: Default::default(), late: Some(tx) };
    let mut m = Machine::new(l, Facts::default(), vec![Box::new(d)]).unwrap();
    let mut lines = run(&mut m, 1);
    assert!(lines.last().unwrap().starts_with("effect(t.Work(), r,"), "the run rests: {lines:?}");
    let (line, reply) = rx.try_recv().unwrap();
    let from = m.record.len();
    m.answer(line as usize, reply).unwrap();
    m.run();
    lines = m.record[from..].iter().map(|l| l.to_string()).collect();
    assert_eq!(lines, [format!("answered({line}, 9)"), "charged(r1, \"tokens\", 4)".to_string(), "ended(r1, 9)".to_string()], "a run under no other is named by its `began` line");
    let lines = run(&mut m, 2);
    assert!(lines.last().unwrap().starts_with("raised(r, Fault(\"no such tool\""), "{lines:?}");
    assert!(!lines.iter().any(|l| l.starts_with("answered(")), "{lines:?}");
}

fn lines_from(m: &Machine<Facts>, from: usize) -> Vec<String> {
    m.record[from..].iter().map(|l| l.to_string()).collect()
}

#[test]
fn a_failing_step_is_a_fault_in_its_run_and_the_other_runs_go_on() {
    let (mut m, _) = machine("module t", &["Returns Int Does { return 1 / 0 }", "Returns Int Does { return 2 }"], Facts::default(), vec![]);
    let from = m.record.len();
    m.start(Value::text("w1"), Some(0), "", Instance { ty: "t.P1".into(), args: vec![Value::text("w1")] }).unwrap();
    m.start(Value::text("w1"), Some(0), "", Instance { ty: "t.P2".into(), args: vec![Value::text("w1")] }).unwrap();
    m.run();
    let lines = lines_from(&m, from);
    assert!(lines.contains(&"raised(r1, Fault(\"a division by zero\"))".to_string()) && lines.contains(&"ended(r2, 2)".to_string()), "{lines:?}");
}

#[test]
fn a_driver_performs_only_the_events_it_named_and_a_where_that_fails_is_a_reactions_fault() {
    let (mut m, _) = machine("module t\nFact Broken(x: String)\nEvent Seen()\nEvent Other()\non Seen() where Broken(y) do { return 1 }", &[], Facts::default(), vec![]);
    let from = m.record.len();
    m.event(0, Instance { ty: "t.Other".into(), args: vec![] });
    m.event(0, Instance { ty: "t.Seen".into(), args: vec![] });
    m.run();
    let lines = lines_from(&m, from);
    assert_eq!(lines[0], "refused(t.Other(), a driver, -, Source)", "{lines:?}");
    assert!(lines[1].starts_with("performed(t.Seen()"), "{lines:?}");
    assert!(lines.iter().any(|l| l.starts_with("raised(r3,") && l.contains("Fault(\"Broken cannot be evaluated\")")), "{lines:?}");
}

#[test]
fn charge_reaches_every_arm_innermost_first_even_past_one_that_drops_k() {
    let text = "module t\nEffect Work() -> Int\nEffect Saw(n: Int) -> Int";
    let p = "Returns Int Does { handle { handle { perform Work() } with { | Charge(d, n), k => return 0 } } with { | Charge(d, n), k => { s <- perform Saw(n); resume k(()) } } }";
    let (mut m, asked) = machine(text, &[p], Facts::default(), vec![("t.Work", Value::Int(5), vec![("tokens".into(), 30)]), ("t.Saw", Value::Int(1), vec![])]);
    let lines = run(&mut m, 1);
    assert_eq!(*asked.borrow(), ["t.Work", "t.Saw"], "the outer arm saw the cost the inner one kept: {lines:?}");
    assert_eq!(lines.last().unwrap(), "ended(r, 0)", "{lines:?}");
}

#[test]
fn each_stated_dimension_is_charged_once_and_a_cost_in_another_is_a_fault() {
    let text = "module t\nEffect Work() -> Int\nEffect Odd() -> Int";
    let (mut m, _) = machine(text, &["Returns Int Does { perform Work() }", "Returns Int Does { perform Odd() }"], Facts::default(), vec![("t.Work", Value::Int(5), vec![]), ("t.Odd", Value::Int(1), vec![("watts".into(), 3)])]);
    let lines = run(&mut m, 1);
    assert!(lines.contains(&"charged(r, \"tokens\", 0)".to_string()), "{lines:?}");
    let lines = run(&mut m, 2);
    assert!(lines.last().unwrap().starts_with("raised(r, Fault(\"a cost in watts"), "{lines:?}");
}

#[test]
fn a_cancelled_branch_resting_on_a_driver_tells_it_the_line_is_no_longer_wanted() {
    let text = "module t\nEffect Work() -> Int\nEffect Stop() -> Int";
    let p = "Returns (Int, Int) Does { handle { par(perform Work(), perform Stop()) } with { | Stop(), k => return (0, 0) } }";
    let (mut m, asked) = machine(text, &[p], Facts::default(), vec![("t.Work", Value::text("later"), vec![])]);
    let lines = run(&mut m, 1);
    let line = lines.iter().position(|l| l.starts_with("effect(t.Work()")).map(|k| m.record.len() - lines.len() + k).unwrap();
    assert_eq!(*asked.borrow(), ["t.Work".to_string(), format!("dropped {line}")], "{lines:?}");
    assert_eq!(lines.last().unwrap(), "ended(r, (0, 0))", "{lines:?}");
}

const GUARDED: &str = "module t\nFact Point Identified by String\nFact Lets(h: String, v: String, p: Point, w: String)\n+Point(\"here\").\n+Point(\"there\").\n+Lets(\"stand-in\", \"1\", \"here\", \"Go\").\n+Lets(\"stand-in\", \"1\", \"there\", \"Go\").\n+Lets(\"stand-in\", \"1\", \"here\", \"Stop\").\ntype V = | Go Needs \"Go\" | Stop(why: String) Needs \"Stop\"\nEffect Advise(p: Point) -> V\nFact Can(p: Point, w: String) Holds when Lets(\"stand-in\", \"1\", p, w)\nfn stop() -> V { return Stop(\"f\") }\nEffect Carry(p: Point, v: V) -> Bool\nfn give(p: Point) -> Program<Bool> { perform Carry(p, Stop(\"g\")) }\nFact Any-Stop(w: String) Holds when Lets(h, v, p, w)\nEffect Advise2(p: Point) -> V Does { if Lets(\"stand-in\", \"1\", p, \"Stop\") { return Stop(\"d\") } else { return Go } }";

fn early(arm: &str) -> Result<usize, String> {
    let p = format!("Returns V Does {{ handle {{ perform Advise(\"there\") }} with {{ | {arm} }} }}");
    let (mut m, _) = machine(GUARDED, &[&p], Facts::default(), vec![("t.Advise", Value::ctor("t.Go", vec![]), vec![]), ("t.Carry", Value::Bool(true), vec![])]);
    let before = m.record.len();
    let r = m.start(Value::text("w1"), Some(0), "", Instance { ty: "t.P1".into(), args: vec![Value::text("w1")] });
    assert_eq!(m.record.len() == before, r.is_err(), "a run that does not start writes nothing");
    r
}

#[test]
fn a_run_whose_arm_writes_a_power_not_let_at_its_point_does_not_start() {
    let e = early("Advise(\"there\"), k => resume k(Stop(\"no\"))").unwrap_err();
    assert!(e.contains("t.Advise at ") && e.contains("Lets(\"stand-in\", \"1\", \"there\", \"Stop\")"), "{e}");
    assert!(early("Advise(\"here\"), k => resume k(Stop(\"no\"))").is_ok());
    let e = early("Advise(p), k => resume k(Stop(\"no\"))").unwrap_err();
    assert!(e.contains("\"there\"") && !e.contains("\"here\""), "a point bound by the pattern is every point that holds: {e}");
    assert!(early("Advise(p), k => resume k(Go)").is_ok(), "and `Advise2`'s `Does` is covered by its test on its own point field");
}

#[test]
fn a_power_under_a_test_of_lets_or_written_by_a_function_the_arm_calls_is_not_counted() {
    assert!(early("Advise(p), k where Lets(\"stand-in\", \"1\", p, \"Stop\") => resume k(Stop(\"no\"))").is_ok());
    assert!(early("Advise(p), k where Can(p, \"Stop\") => resume k(Stop(\"no\"))").is_ok(), "a fact whose every rule tests Lets");
    assert!(early("Advise(p), k where Can(p, \"Go\") => resume k(Stop(\"no\"))").is_err(), "a test of another power");
    assert!(early("Advise(\"there\"), k => resume k(stop())").is_ok(), "the boundary covers it");
}

#[test]
fn a_run_that_may_perform_an_effect_nobody_answers_does_not_start() {
    let text = "module t\nEffect Ask() -> Int\nEffect Out() -> Int";
    let (mut m, _) = machine(text, &["Returns Int Does { perform Ask() }", "Returns Int Does { handle { perform Ask() } with { | Ask(), k => resume k(1) } }"], Facts::default(), vec![("t.Out", Value::Int(1), vec![])]);
    let e = m.start(Value::text("w1"), Some(0), "", Instance { ty: "t.P1".into(), args: vec![Value::text("w1")] }).unwrap_err();
    assert!(e.contains("t.Ask"), "{e}");
    assert_eq!(run(&mut m, 2).last().unwrap(), "ended(r, 1)");
    let l = docket_v2::check::load(vec![docket_v2::syntax::read(&format!("{text}\non Out() do {{ perform Ask() }}")).unwrap()]).unwrap();
    let e = Machine::new(l, Facts::default(), vec![]).err().unwrap();
    assert!(e.contains("the reaction to t.Out may perform t.Ask"), "{e}");
}

#[test]
fn a_test_on_a_variable_covers_only_where_that_binding_of_it_stands() {
    let e = early("Advise(p), k where (Exists p : Lets(\"stand-in\", \"1\", p, \"Stop\")) => resume k(Stop(\"no\"))").unwrap_err();
    assert!(e.contains("\"there\", \"Stop\""), "a quantifier's own p: {e}");
    let e = early("Advise(p), k where Any-Stop(\"Stop\") => resume k(Stop(\"no\"))").unwrap_err();
    assert!(e.contains("\"there\", \"Stop\""), "a rule's own p: {e}");
    let e = early("Advise(p), k where Lets(\"stand-in\", \"1\", p, \"Stop\") => { b <- do give(p); resume k(Go) }").unwrap_err();
    assert!(e.contains("t.Carry at ") && e.contains("\"there\", \"Stop\""), "a function's own p: {e}");
}

#[test]
fn a_filling_that_writes_a_power_not_let_at_its_point_is_unfit() {
    let text = format!("{GUARDED}\nfn f() -> Program<V> {{ x <- ...; return x }}");
    let filling = "{ handle { perform Advise(\"there\") } with { | Advise(\"there\"), k => resume k(Stop(\"no\")) } }";
    let answers = vec![("Gap", Value::text(filling), vec![]), ("t.Advise", Value::ctor("t.Go", vec![]), vec![]), ("t.Carry", Value::Bool(true), vec![])];
    let (mut m, _) = machine(&text, &["Returns V Does { do f() }"], Facts::default(), answers);
    let lines = run(&mut m, 1);
    let last = lines.last().unwrap();
    assert!(last.starts_with("raised(r, Unfit(") && last.contains("there"), "{lines:?}");
}

#[test]
fn each_effect_line_keys_the_head_of_its_runs_folder_as_it_stands_then() {
    let (mut m, _) = machine("module t\nEffect Work() -> Int", &["Returns Int Does { a <- perform Work(); b <- perform Work(); return a + b }"], Facts::default(), vec![("t.Work", Value::Int(1), vec![])]);
    let edits = std::rc::Rc::new(std::cell::Cell::new(0));
    let seen = edits.clone();
    m.head = Box::new(move |folder| {
        seen.set(seen.get() + 1);
        format!("{folder}@{}", seen.get())
    });
    m.start(Value::text("w1"), Some(0), "/w", Instance { ty: "t.P1".into(), args: vec![Value::text("w1")] }).unwrap();
    m.run();
    let keys: Vec<_> = m.record.iter().filter_map(|l| if let docket_v2::run::record::Line::Effect { head, key, .. } = l { Some((head.clone(), *key)) } else { None }).collect();
    assert_eq!(edits.get(), 2, "the head is read at each effect line");
    assert_eq!((keys[0].0.as_str(), keys[1].0.as_str()), ("/w@1", "/w@2"));
    assert_ne!(keys[0].1, keys[1].1, "the same Effect after the tree moved has another key");
}

#[test]
fn a_program_builds_a_record_and_a_condition_reads_its_fields() {
    let text = "module t\ntype Pair { rel: String, n: Int }\nFact Rel(r: String)\nFact Kind Identified by String\n+Rel(\"means\").\n+Kind(\"means\").";
    let (mut m, _) = machine(text, &["Returns Int Does { p <- return Pair(\"means\", 2); if Rel(p.rel) && Holds(Rel(p.rel)) && Kind(p.rel) { return p.n } else { return 0 } }"], Facts::default(), vec![]);
    let lines = run(&mut m, 1);
    assert_eq!(lines.last().unwrap(), "ended(r, 2)", "{lines:?}");
}

#[test]
fn an_exact_replay_takes_whether_a_guarded_value_crossed_from_the_record_whichever_driver_is_linked() {
    let text = "module t\nFact Point Identified by String\nFact Lets(h: String, v: String, p: Point, w: String)\n+Lets(\"stand-in\", \"1\", \"here\", \"Stop\").\ntype V = | Go Needs \"Go\" | Stop(why: String) Needs \"Stop\"\nEffect Advise(p: Point) -> V\nEffect Carry(p: Point, v: V) -> Bool";
    let programs = ["Returns Bool Does { v <- perform Advise(\"here\"); perform Carry(\"here\", v) }"];
    let linked = |version: &str| {
        let d = Table { name: vec![Value::text("stand-in"), Value::text(version)], answers: vec![("t.Advise", Value::ctor("t.Stop", vec![Value::text("no")]), vec![]), ("t.Carry", Value::Bool(true), vec![])], asked: Default::default(), late: None };
        Machine::new(loaded(text, &programs), Facts::default(), vec![Box::new(d)]).unwrap()
    };
    // let to version 1 when written, not to version 2, and the other way round
    for (then, now) in [("1", "2"), ("2", "1")] {
        let mut old = linked(then);
        let lines = run(&mut old, 1);
        assert_eq!(lines.last().unwrap().starts_with("raised(r, Fault("), then == "2", "{lines:?}");
        let mut m = linked(now);
        m.restore(&old.record, &|_, _| (Some(0), String::new()), &mut |_| Err("one text".into())).unwrap();
        assert_eq!(m.record, old.record, "written with version {then}, restored with {now}");
    }
}
