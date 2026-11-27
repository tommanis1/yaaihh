//! docket/eval's programs run on the machine over the eFLINT core: `with_seal` around a served agent, `capped` around
//! work that costs, and `mutate` from a front candidate, each with a stand-in driver answering their Effects.

use docket_v2::norm::Core;
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::machine::Machine;
use docket_v2::run::norms::Norms;
use docket_v2::run::record::Line;
use docket_v2::run::value::{Instance, Value};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

fn text(rel: &str) -> String {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../docket/").to_string() + rel;
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

/// The project: who caps, what the stand-in harness lets, and an Act whose program is each of the three.
const PROJECT: &str = r#"module project
import std.hooks as H
import std.budgets as B
import eval as E
+E.Caps-By("owner").
+H.Party("agent").
+H.Lets("stand-in", "1", "tool_called", "deny").
Effect Work() -> Unit
Act Sealed(by: H.Party) Does { do E.with_seal(by, H.serve(by)) }
Act Capped(by: H.Party, run: B.Bucket) Does { result <- do E.capped(run, { perform Work(); perform Work(); perform Work() }); return () }
Act Mutate(by: H.Party, parent: E.Candidate) Does { do E.mutate("s", parent) }
"#;

type Asked = Rc<RefCell<Vec<(String, Vec<Value>)>>>;

/// Every Effect of the texts with no `Does`: a driver carries them all, or a reaction that may perform one is lost.
const CARRIED: [&str; 14] = [
    "std.hooks.Start-Session", "std.hooks.Tool-Use", "std.hooks.Next-Moment", "std.hooks.Apply", "std.comms.Call-Text", "std.comms.Model", "std.comms.Read-As",
    "std.comms.Pick", "std.comms.TextOf", "std.budgets.Request-Budget", "std.budgets.Command", "std.budgets.Measure", "eval.Source-Of", "eval.Write-Candidate",
];

/// Answers each Effect it carries from a queue of answers for it, and writes down what it was asked.
struct Stand {
    answers: Vec<(&'static str, VecDeque<Reply>)>,
    asked: Asked,
}

impl Driver for Stand {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("stand-in"), Value::text("1")]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        let mut all: Vec<&str> = CARRIED.to_vec();
        all.extend(self.answers.iter().map(|(e, _)| *e).filter(|e| !CARRIED.contains(e)));
        all.into_iter().map(|e| (e.to_string(), vec!["tokens".to_string()])).collect()
    }
    fn events(&self) -> Vec<String> {
        vec![]
    }
    fn carry(&mut self, op: &str, args: &[Value], _: u64) -> Reply {
        self.asked.borrow_mut().push((op.to_string(), args.to_vec()));
        let q = self.answers.iter_mut().find(|(e, _)| *e == op).map(|(_, q)| q);
        q.and_then(VecDeque::pop_front).unwrap_or_else(|| Reply::Failed(format!("{op}: no answer left")))
    }
    fn dropped(&mut self, _: u64) {}
}

fn t(s: &str) -> Value {
    Value::text(s)
}

fn n(x: i64) -> Value {
    Value::Int(x)
}

fn answer(v: Value) -> Reply {
    Reply::Answer { value: v, costs: vec![], events: vec![] }
}

fn record(ty: &str, fields: Vec<(&str, Value)>) -> Value {
    Value::Record { ty: ty.into(), fields: fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect() }
}

/// A machine on std, eval and the project, its driver answering ANSWERS.
fn machine(answers: Vec<(&'static str, Vec<Reply>)>) -> (Machine<Core>, Asked) {
    let texts = ["std/hooks.docket", "std/comms.docket", "std/budgets.docket", "eval/eval.docket"];
    let mut modules: Vec<_> = texts.iter().map(|f| docket_v2::syntax::read(&text(f)).unwrap_or_else(|e| panic!("{f}: {e}"))).collect();
    modules.push(docket_v2::syntax::read(PROJECT).unwrap_or_else(|e| panic!("project: {e}")));
    let l = docket_v2::check::load(modules).unwrap_or_else(|e| panic!("{e}"));
    let asked: Asked = Default::default();
    let d = Stand { answers: answers.into_iter().map(|(e, v)| (e, v.into())).collect(), asked: asked.clone() };
    (Machine::new(l, Core::default(), vec![Box::new(d)]).unwrap_or_else(|e| panic!("{e}")), asked)
}

/// TY with ARGS done by PARTY on the machine's core, outside any run.
fn act(m: &mut Machine<Core>, party: &str, ty: &str, args: Vec<Value>) {
    let s = m.norms.step(&Instance { ty: ty.into(), args }, Some(&t(party))).unwrap_or_else(|e| panic!("{ty}: {e:?}"));
    m.norms.commit(s);
}

/// Seated parties, suite `s` of tasks a and b hiding `tests/*`, capped, one trial each.
fn suite(m: &mut Machine<Core>) {
    let s = m.norms.step(&Instance { ty: "std.comms.Owner-Seated".into(), args: vec![t("him")] }, None).unwrap();
    m.norms.commit(s);
    for (who, role) in [("load", "loader"), ("run", "runner"), ("grade", "grader"), ("prop", "proposer")] {
        act(m, "him", "std.comms.Seat", vec![t("him"), t(who), t(role)]);
    }
    for task in ["a", "b"] {
        act(m, "load", "eval.Load-Task", vec![t("load"), t(task), t("s")]);
        act(m, "load", "eval.Hide", vec![t("load"), t(task), t("tests/*")]);
    }
    for (dim, max) in [("tokens", 10), ("usd_cents", 50), ("wall_ms", 60_000)] {
        act(m, "him", "eval.Cap", vec![t("him"), t("s"), t(dim), n(max)]);
    }
    act(m, "him", "eval.Set-Trials", vec![t("him"), t("s"), n(1)]);
}

/// Act TY with ARGS started as PARTY and run to its end; the record's lines it wrote.
fn run(m: &mut Machine<Core>, party: &str, ty: &str, args: Vec<Value>) -> Vec<String> {
    let from = m.record.len();
    m.start(t(party), Some(0), "", Instance { ty: ty.into(), args }).unwrap_or_else(|e| panic!("{ty}: {e}"));
    m.run();
    m.record[from..].iter().map(Line::to_string).collect()
}

fn holds(m: &mut Machine<Core>, ty: &str, args: Vec<Value>) -> bool {
    m.norms.holds(&Instance { ty: ty.into(), args })
}

/// A tool call changing PATH, as the harness reports it.
fn changing(path: &str) -> Value {
    let change = record("std.hooks.FileChange", vec![("kind", t("edit")), ("path", t(path)), ("from", t("")), ("patch", t("")), ("added", t(""))]);
    record("std.hooks.ToolUse", vec![("tool", t("edit")), ("command", t("")), ("changes", Value::List(vec![change]))])
}

#[test]
fn with_seal_denies_a_change_to_a_grader_file_and_passes_the_rest_on() {
    let moment = || answer(record("std.hooks.Reached", vec![("moment", t("tool_called")), ("payload", t("{}"))]));
    let (mut m, asked) = machine(vec![
        ("std.hooks.Next-Moment", vec![moment(), moment(), moment()]),
        ("std.hooks.Tool-Use", vec![answer(changing("tests/test.sh")), answer(changing("src/lib.rs")), answer(changing("src/../tests/x"))]),
        ("std.hooks.Apply", vec![answer(Value::Unit), answer(Value::Unit), answer(Value::Unit)]),
    ]);
    suite(&mut m);
    act(&mut m, "run", "eval.Start-Run", vec![t("run"), t("agent"), t("c1"), t("a"), n(1), t("r1"), Value::Bool(true)]);
    let lines = run(&mut m, "agent", "project.Sealed", vec![t("agent")]);
    let applied: Vec<Value> = asked.borrow().iter().filter(|(op, _)| op == "std.hooks.Apply").map(|(_, a)| a[2].clone()).collect();
    assert_eq!(applied.len(), 3, "{lines:#?}");
    let denied = |v: &Value| matches!(v, Value::Ctor { name, .. } if name.ends_with("Deny"));
    assert!(denied(&applied[0]) && !denied(&applied[1]) && denied(&applied[2]), "the grader's file and the one named another way: {applied:?}");
    assert!(!holds(&mut m, "std.hooks.Changes", vec![t("agent"), t("tests/test.sh")]), "the call's files forgotten after it");
}

#[test]
fn capped_charges_the_runners_ledger_and_stops_the_work_at_the_cap() {
    let costs = |k: i64| Reply::Answer { value: Value::Unit, costs: vec![("tokens".into(), k)], events: vec![] };
    let (mut m, _) = machine(vec![("project.Work", vec![costs(6), costs(6), costs(6), costs(6)])]);
    suite(&mut m);
    act(&mut m, "run", "eval.Start-Run", vec![t("run"), t("agent"), t("c1"), t("a"), n(1), t("r1"), Value::Bool(true)]);
    let lines = run(&mut m, "run", "project.Capped", vec![t("run"), t("r1")]);
    assert!(holds(&mut m, "std.budgets.Total", vec![t("r1"), t("tokens"), n(12)]), "both costs recorded: {lines:#?}");
    assert!(holds(&mut m, "eval.Broke-Seal", vec![t("r1")]), "past its cap");
    assert_eq!(lines.iter().filter(|l| l.starts_with("effect(project.Work")).count(), 2, "the third never runs: {lines:#?}");
    assert!(!lines.iter().any(|l| l.contains("Request-Budget")), "denied in the arm, nobody asked for more: {lines:#?}");
    // the same program run as the agent the run meters: its first cost is refused, never written
    act(&mut m, "run", "eval.Start-Run", vec![t("run"), t("agent"), t("c1"), t("b"), n(1), t("r2"), Value::Bool(true)]);
    let lines = run(&mut m, "agent", "project.Capped", vec![t("agent"), t("r2")]);
    assert!(lines.iter().any(|l| l.starts_with("refused(std.budgets.Record-Spend")), "{lines:#?}");
    assert!(holds(&mut m, "std.budgets.Total", vec![t("r2"), t("tokens"), n(0)]));
}

#[test]
fn mutate_gives_the_reflector_the_parents_text_and_proposes_its_change() {
    let source = "module candidate\n// the harness text itself, not its hash\n";
    let (mut m, asked) = machine(vec![
        ("std.comms.Pick", vec![answer(t("refl"))]),
        ("eval.Source-Of", vec![answer(t(source))]),
        ("std.comms.TextOf", vec![answer(t("the trace of b"))]),
        ("std.comms.Call-Text", vec![answer(t("the prompt"))]),
        ("std.comms.Model", vec![answer(t("change: read the tests' names"))]),
        ("std.comms.Read-As", vec![answer(Value::ctor("eval.Change", vec![t("read the tests' names")]))]),
        ("eval.Write-Candidate", vec![answer(t("c7"))]),
    ]);
    suite(&mut m);
    for (task, run, pass) in [("a", "ra", true), ("b", "rb", false)] {
        act(&mut m, "run", "eval.Start-Run", vec![t("run"), t("agent"), t("c2"), t(task), n(1), t(run), Value::Bool(true)]);
        act(&mut m, "grade", "eval.Record-Result", vec![t("grade"), t(run), Value::Bool(pass), n(1), n(1), n(1), t(&format!("eval-traces/{run}"))]);
    }
    assert!(holds(&mut m, "eval.On-Front", vec![t("s"), t("c2")]));
    let lines = run(&mut m, "prop", "project.Mutate", vec![t("prop"), t("c2")]);
    let asked = asked.borrow();
    let arg = |op: &str| asked.iter().find(|(o, _)| o == op).map(|(_, a)| a.clone()).unwrap_or_else(|| panic!("{op} never asked: {lines:#?}"));
    assert_eq!(arg("eval.Source-Of"), [t("c2")]);
    assert_eq!(arg("std.comms.TextOf"), [t("eval-traces/rb")], "only the failed run's trace");
    match &arg("std.comms.Call-Text")[0] {
        Value::Prompt { name, args } => assert!(name.ends_with("Reflect") && args[0] == t(source) && args[1] == Value::List(vec![t("the trace of b")]), "{args:?}"),
        other => panic!("{other:?}"),
    }
    assert!(holds(&mut m, "eval.Child-Of", vec![t("c7"), t("c2"), t("read the tests' names")]), "{lines:#?}");
}
