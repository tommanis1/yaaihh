//! The time to run and restore one long run of single Effects, and research's words import; run alone.

mod common;

use common::*;
use docket_v2::norm::Core;
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::machine::Machine;
use docket_v2::run::norms::{NormDecl, Norms, Step, Why};
use docket_v2::run::value::{Env, Instance, Value};
use docket_v2::syntax::form;
use docket_v2::syntax::tree::Expr;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

const TEXT: &str = "module t\nEffect TextOf(n: Int) -> String\nAct Take(by: String, l: List<Int>) Does { xs <- par n in l up to 1 { perform TextOf(n) }; return () }";

fn linked() -> Machine<Facts> {
    let d = Table { name: vec![Value::text("stand-in"), Value::text("1")], answers: vec![("t.TextOf", Value::text("words"), vec![])], asked: Default::default(), late: None };
    Machine::new(loaded(TEXT, &[]), Facts::default(), vec![Box::new(d)]).unwrap()
}

/// Runs of the given lengths one after the other, timed; then the record restored on a fresh machine, timed.
fn measure(lengths: &[usize]) {
    let mut m = linked();
    let t = Instant::now();
    for n in lengths {
        let l = Value::List((0..*n as i64).map(Value::Int).collect());
        m.start(Value::text("w1"), Some(0), "", Instance { ty: "t.Take".into(), args: vec![Value::text("w1"), l] }).unwrap();
        m.run();
    }
    let ran = t.elapsed();
    let answers = m.record.iter().filter(|l| l.to_string().starts_with("answered(")).count();
    assert_eq!(answers, lengths.iter().sum::<usize>());
    let mut fresh = linked();
    let t = Instant::now();
    fresh.restore(&m.record, &|_, _| (Some(0), String::new()), &mut |_| Err("one text".into())).unwrap();
    let restored = t.elapsed();
    assert_eq!(fresh.record, m.record);
    eprintln!("cost: {} runs, {answers} answers, {} lines: run {:.1} ms, restore {:.1} ms", lengths.len(), m.record.len(), ran.as_secs_f64() * 1e3, restored.as_secs_f64() * 1e3);
}

/// With the stand-in core and a driver answering at once; `--ignored --nocapture` prints the figures.
#[test]
#[ignore]
fn cost_of_one_long_run_and_of_research_words_import() {
    measure(&[2344]);
    // 666 runs, 12,366 answers: one of 2,344, and 665 sharing the rest
    let rest = 12366 - 2344;
    let mut lengths = vec![2344];
    lengths.extend((0..665).map(|k| rest / 665 + usize::from(k < rest % 665)));
    measure(&lengths);
}

/// A turn's usage priced as std.budgets' reaction to Usage reads it (budgets.docket:124), and a thread's posts listed as
/// v1's `Foreach n : n When Desk.Posted(n, thread)` reads them (v1.docket:86).
const PRICED: &str = r#"
module t
Fact Party Identified by String
Fact Node Identified by String
Fact Thread Identified by String
Fact Kind Identified by String
Fact Bucket Identified by String
Fact Used(turn: Node, agent: Party, kind: Kind, tokens: Int, minute: Int)
Fact On-Model(agent: Party, model: String)
Fact Rate(model: String, kind: Kind, per_thousand: Int)
Fact Bucket-Of(agent: Party, bucket: Bucket)
Fact Posted(n: Node, thread: Thread)
Event Joined(agent: Party, model: String, bucket: Bucket)
  Creates On-Model(agent, model), Bucket-Of(agent, bucket)
Event Priced(model: String, kind: Kind, per_thousand: Int)
  Creates Rate(model, kind, per_thousand)
Event Usage(agent: Party, turn: Node, kind: Kind, tokens: Int, minute: Int)
  Creates Used(turn, agent, kind, tokens, minute)
Event Post(n: Node, thread: Thread)
  Creates Posted(n, thread)
Event Read(thread: Thread)
on Usage(agent, _, kind, tokens, _) where Bucket-Of(agent, b) && On-Model(agent, m) && Rate(m, kind, r) && r > 0 do { return tokens * r }
on Read(thread) where Posted(n, thread) do { return n }
"#;

/// Performs PRICED's Events.
struct Events;

impl Driver for Events {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("events"), Value::text("1")]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        Vec::new()
    }
    fn events(&self) -> Vec<String> {
        ["Joined", "Priced", "Usage", "Post", "Read"].map(|e| format!("t.{e}")).to_vec()
    }
    fn carry(&mut self, op: &str, _: &[Value], _: u64) -> Reply {
        Reply::Failed(format!("no driver for {op}"))
    }
    fn dropped(&mut self, _: u64) {}
}

/// Each `each` the daemon's core answers: its condition's written form, the bindings tried, the rows answered.
type Eaches = Rc<RefCell<Vec<(String, u64, usize)>>>;

/// The daemon's core, `Core::default()`, each `each` kept in its [`Eaches`].
struct Counted(Core, Eaches);

impl Norms for Counted {
    fn load(&mut self, decls: &[NormDecl]) -> Result<(), String> {
        self.0.load(decls)
    }
    fn holds(&mut self, i: &Instance) -> bool {
        self.0.holds(i)
    }
    fn cond(&mut self, e: &Expr, env: &Env) -> Result<bool, String> {
        self.0.cond(e, env)
    }
    fn each(&mut self, e: &Expr, env: &Env) -> Result<Vec<Env>, String> {
        let before = self.0.tried();
        let rows = self.0.each(e, env)?;
        self.1.borrow_mut().push((form::expr(e).to_string(), self.0.tried() - before, rows.len()));
        Ok(rows)
    }
    fn step(&mut self, act: &Instance, party: Option<&Value>) -> Result<Step, Why> {
        self.0.step(act, party)
    }
    fn commit(&mut self, step: Step) {
        self.0.commit(step)
    }
    fn view(&mut self, k: Option<usize>) {
        self.0.view(k)
    }
    fn reads(&mut self) -> [u8; 32] {
        self.0.reads()
    }
}

fn counted(eaches: &Eaches) -> Machine<Counted> {
    Machine::new(loaded(PRICED, &[]), Counted(Core::default(), eaches.clone()), vec![Box::new(Events)]).unwrap()
}

/// TURNS turns of four agents, each on its own model and bucket, with a rate for each kind: a turn posts in one of ten
/// threads and reports four kinds, each with its own count of tokens as a real turn's are; then one thread is read.
/// Each `each`, as run and as restored: the condition, the bindings tried, the rows.
fn priced(turns: usize) -> (Vec<(String, u64, usize)>, Vec<(String, u64, usize)>) {
    let ran: Eaches = Default::default();
    let mut m = counted(&ran);
    let ev = |ty: &str, args: Vec<Value>| Instance { ty: format!("t.{ty}"), args };
    let mut events = Vec::new();
    for a in 0..4 {
        events.push(ev("Joined", vec![Value::text(&format!("a{a}")), Value::text(&format!("m{a}")), Value::text(&format!("b{a}"))]));
        for (j, kind) in ["input", "cached_read", "cache_write", "output"].iter().enumerate() {
            events.push(ev("Priced", vec![Value::text(&format!("m{a}")), Value::text(kind), Value::Int(10 + 4 * a + j as i64)]));
        }
    }
    for k in 0..turns {
        let (agent, node) = (Value::text(&format!("a{}", k % 4)), Value::text(&format!("n{k}")));
        events.push(ev("Post", vec![node.clone(), Value::text(&format!("t{}", k % 10))]));
        for (j, kind) in ["input", "cached_read", "cache_write", "output"].iter().enumerate() {
            events.push(ev("Usage", vec![agent.clone(), node.clone(), Value::text(kind), Value::Int((4 * k + j + 1) as i64), Value::Int(k as i64)]));
        }
    }
    events.push(ev("Read", vec![Value::text("t3")]));
    for e in events {
        m.event(0, e);
        m.run();
    }
    let restored: Eaches = Default::default();
    let mut fresh = counted(&restored);
    fresh.restore(&m.record, &|_, _| (Some(0), String::new()), &mut |_| Err("one text".into())).unwrap();
    assert_eq!(fresh.record, m.record);
    (ran.take(), restored.take())
}

/// N-1832 p305: `each` tries no more bindings than the rows its condition's facts match, whatever else the state
/// holds, as run and in the restore's replay. Pricing a Usage tries the one row of its agent's bucket, model and rate for
/// its kind, as many Ints and Strings as the turns have put in the state; reading a thread tries that thread's posts,
/// not every Node. Every row the facts match is one `r > 0` keeps, so the rows answered are the rows matched.
#[test]
fn each_tries_the_rows_its_facts_match_whatever_the_state_holds() {
    for turns in [20, 100] {
        let (ran, restored) = priced(turns);
        for (eaches, how) in [(&ran, "run"), (&restored, "restored")] {
            assert_eq!(eaches.len(), 4 * turns + 1, "{how}: an `each` for each Usage and the one Read");
            let usage = |c: &str| ["Bucket-Of(agent, b)", "On-Model(agent, m)", "Rate(m, kind, r)", "r > 0"].iter().all(|f| c.contains(f));
            assert!(eaches[..4 * turns].iter().all(|(c, tried, rows)| usage(c) && (*tried, *rows) == (1, 1)), "{turns} turns, {how}: {:?}", &eaches[..4]);
            assert_eq!(eaches[4 * turns], ("t.Posted(n, thread)".to_string(), turns as u64 / 10, turns / 10), "{turns} turns, {how}");
        }
    }
}
