//! What a change costs on a large state, the act and its delta timed apart, p50 and p99: thousands of facts the act
//! does not touch, none of them read; and thousands in the very type it changes, with a count and a sum over it.

use coordd_v2::docket::{Bind, Docket, Texts};
use coordd_v2::machine::Machine;
use coordd_v2::peer::Party;
use coordd_v2_wire::StepDelta;
use docket_v2::norm::Core;
use docket_v2::run::norms::{Moved, NormDecl, Norms, Step, Why};
use docket_v2::run::value::{Env, Instance, Value};
use docket_v2::syntax::tree::Expr;
use serde_json::{json, Value as Json};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const LENDING: &str = r#"
module t
Fact Person Identified by String
Fact Book Identified by String
Fact Shelf(book: Book, room: String)
Fact Note Identified by String
Fact Housed(book: Book) Holds when Shelf(book, "hall")
Fact Kept(keeper: Person, room: String)
Duty Dust(keeper: Person, room: String) Holds when Kept(keeper, room)
Fact Lent(book: Book, to: Person, by: Person)
Fact Overdue(book: Book)
Duty Give-Back(holder: Person, claimant: Person, book: Book)
  Holds when Lent(book, holder, claimant)
  Violated when Overdue(book)
Act Lend(by: Person, book: Book, to: Person)
  Conditioned by by != to && Not(Exists p, q : Lent(book, p, q))
  Creates Lent(book, to, by)
Act Take-Back(by: Person, book: Book)
  Conditioned by Exists p : Lent(book, p, by)
  Terminates Lent(book, p, by)
Act Mark(by: Person, book: Book)
  Creates Overdue(book)
Act Forgive(by: Person, book: Book)
  Terminates Overdue(book)
+Person("w1"). +Person("bob").
"#;

/// What the core was asked while `on`: every read of Σ, and every type it would read whole.
#[derive(Default)]
struct Probe {
    on: AtomicBool,
    asked: Mutex<Vec<String>>,
}

impl Probe {
    fn note(&self, what: String) {
        if self.on.load(Ordering::SeqCst) {
            self.asked.lock().unwrap().push(what);
        }
    }
}

/// The core, with a probe on what it is asked.
struct Watched(Core, Arc<Probe>);

impl Norms for Watched {
    fn load(&mut self, d: &[NormDecl]) -> Result<(), String> {
        self.0.load(d)
    }
    fn holds(&mut self, i: &Instance) -> bool {
        self.1.note(format!("holds {i}"));
        self.0.holds(i)
    }
    fn cond(&mut self, c: &Expr, env: &Env) -> Result<bool, String> {
        self.1.note(format!("cond {c:?}"));
        self.0.cond(c, env)
    }
    fn each(&mut self, c: &Expr, env: &Env) -> Result<Vec<Env>, String> {
        self.1.note(format!("each {c:?}"));
        self.0.each(c, env)
    }
    fn step(&mut self, a: &Instance, p: Option<&Value>) -> Result<Step, Why> {
        self.1.note(format!("step {a}"));
        self.0.step(a, p)
    }
    fn commit(&mut self, s: Step) {
        self.0.commit(s)
    }
    fn view(&mut self, k: Option<usize>) {
        self.0.view(k)
    }
    fn reads(&mut self) -> [u8; 32] {
        self.0.reads()
    }
    fn moved(&mut self) -> Moved {
        let m = self.0.moved();
        m.whole.iter().for_each(|t| self.1.note(format!("whole {t}")));
        m
    }
    fn unwatch(&mut self) {
        self.0.unwatch()
    }
    fn cap(&mut self, steps: Option<u64>) {
        self.0.cap(steps)
    }
}

/// LENDING with N facts besides, of types no act here touches: shelves, notes, a derived type and a duty over them.
fn background(n: usize) -> String {
    let mut text = LENDING.to_string();
    for k in 0..n / 2 {
        text.push_str(&format!("+Shelf(\"s{k}\", \"hall\"). +Note(\"n{k}\").\n"));
    }
    for k in 0..100 {
        text.push_str(&format!("+Kept(\"w1\", \"r{k}\").\n"));
    }
    text
}

/// The machine on TEXT, its core probed.
fn machine(text: &str, probe: Arc<Probe>) -> Docket<Watched> {
    let loaded = docket_v2::check::load(vec![docket_v2::syntax::read(text).unwrap()]).unwrap();
    let runs = docket_v2::run::machine::Machine::new(loaded, Watched(Core::default(), probe), vec![]).unwrap();
    let (bind, texts): (Bind, Texts) = (Box::new(|_, _| (None, String::new())), Box::new(|_| Err("this record starts empty".into())));
    Docket::new(runs, "t", vec![], bind, texts)
}

fn lent(book: &str, to: &str, by: &str) -> Json {
    json!({"inst": "t.Lent", "args": [book, to, by]})
}

fn give_back(holder: &str, claimant: &str, book: &str) -> Json {
    json!({"inst": "t.Give-Back", "args": [holder, claimant, book]})
}

/// Act NAME with ARGS as w1, then its delta at SEQ: what the core was asked while the delta was made, and how long
/// the act and the delta each took.
fn acted(m: &mut Docket<Watched>, probe: &Probe, view: &mut coordd_v2::machine::View, seq: u64, name: &str, args: &[Json]) -> (StepDelta, Vec<String>, [Duration; 2]) {
    let began = Instant::now();
    m.act(&Party::of("w1", None), name, args).unwrap();
    let act = began.elapsed();
    probe.on.store(true, Ordering::SeqCst);
    let began = Instant::now();
    let d = m.delta(view, seq).unwrap();
    let delta = began.elapsed();
    probe.on.store(false, Ordering::SeqCst);
    (d, std::mem::take(&mut *probe.asked.lock().unwrap()), [act, delta])
}

/// Every act is timed this many times on the untouched state, and on the crowded one; p50 and p99 are given, never the best.
const ROUNDS: usize = 200;
const CROWDED_ROUNDS: usize = 50;

/// The p50 and p99 of TS.
fn pct(ts: &[Duration]) -> [Duration; 2] {
    let mut ts = ts.to_vec();
    ts.sort();
    [ts[ts.len() / 2], ts[(ts.len() * 99 / 100).min(ts.len() - 1)]]
}

fn texts(xs: &[&str]) -> Vec<Json> {
    xs.iter().map(|x| json!(x)).collect()
}

#[test]
fn a_delta_reads_only_what_the_act_moved_however_much_else_holds() {
    for n in [5_000, 20_000] {
        let probe = Arc::new(Probe::default());
        let mut m = machine(&background(n), probe.clone());
        let mut view = m.view(None).unwrap();
        assert_eq!((view.facts["t.Shelf"].len(), view.facts["t.Note"].len(), view.facts["t.Housed"].len()), (n / 2, n / 2, n / 2));
        assert_eq!(view.duties["t.Dust"].len(), 100);
        let (gb, overdue) = (give_back("bob", "w1", "b1"), json!({"inst": "t.Overdue", "args": ["b1"]}));
        let acts: [(&str, Vec<Json>, StepDelta); 4] = [
            ("t.Lend", texts(&["w1", "b1", "bob"]), StepDelta { created_facts: vec![lent("b1", "bob", "w1")], new_duties: vec![gb.clone()], ..StepDelta::default() }),
            ("t.Mark", texts(&["w1", "b1"]), StepDelta { created_facts: vec![overdue.clone()], violations: vec![gb.clone()], ..StepDelta::default() }),
            ("t.Forgive", texts(&["w1", "b1"]), StepDelta { terminated_facts: vec![overdue], cleared_violations: vec![gb.clone()], ..StepDelta::default() }),
            ("t.Take-Back", texts(&["w1", "b1"]), StepDelta { terminated_facts: vec![lent("b1", "bob", "w1")], terminated_duties: vec![gb], ..StepDelta::default() }),
        ];
        let mut took = vec![[Vec::new(), Vec::new()]; acts.len()];
        let mut seq = 0;
        for _ in 0..ROUNDS {
            for (k, (name, args, want)) in acts.iter().enumerate() {
                seq += 1;
                let (d, asked, t) = acted(&mut m, &probe, &mut view, seq, name, args);
                assert_eq!(d, StepDelta { seq, ..want.clone() }, "n = {n}, {name}");
                assert!(asked.is_empty(), "n = {n}, {name}: the delta asked {asked:?}");
                (0..2).for_each(|w| took[k][w].push(t[w]));
            }
            assert_eq!(view, m.view(None).unwrap(), "the view kept is the view read whole");
        }
        let deltas = said(n, "besides", &acts.map(|a| a.0), &took);
        // the delta reads nothing of the state, so its time does not grow with it; the margin is the tower's load
        assert!(deltas.iter().all(|[p50, p99]| *p50 < Duration::from_millis(1) && *p99 < Duration::from_millis(25)), "n = {n}: {deltas:?}");
    }
}

/// p50 and p99 of each act and each delta, printed; the deltas' returned.
fn said(n: usize, how: &str, names: &[&str], took: &[[Vec<Duration>; 2]]) -> Vec<[Duration; 2]> {
    for (name, [act, delta]) in names.iter().zip(took) {
        // the first round also works out what the load left for the first read
        let later = |xs: &[Duration]| pct(&xs[1..])[1];
        eprintln!("{n} facts {how}: {name}: act p50 {:?} p99 {:?} (after the first round {:?}); delta p50 {:?} p99 {:?} (after the first round {:?})", pct(act)[0], pct(act)[1], later(act), pct(delta)[0], pct(delta)[1], later(delta));
    }
    took.iter().map(|[_, d]| pct(d)).collect()
}

/// N books lent and fined already, so each act changes a type with N instances, and a count and a sum over it.
fn crowded(n: usize) -> String {
    let mut text = LENDING.to_string();
    text.push_str("Fact Fine(book: Book, amount: Int)\n");
    text.push_str("Fact Loans(n: Int)\n  Holds when n == Count(Foreach b, p, q : b When Lent(b, p, q))\n");
    text.push_str("Fact Fines(total: Int)\n  Holds when total == Sum(Foreach b, k : k When Fine(b, k))\n");
    text.push_str("Act Charge(by: Person, book: Book, amount: Int)\n  Creates Fine(book, amount)\n");
    text.push_str("Act Pay(by: Person, book: Book, amount: Int)\n  Terminates Fine(book, amount)\n");
    for k in 0..n {
        text.push_str(&format!("+Lent(\"x{k}\", \"bob\", \"w1\"). +Fine(\"x{k}\", 1).\n"));
    }
    text
}

#[test]
fn a_delta_in_a_crowded_type_with_a_count_and_a_sum_over_it() {
    let mut p50s: Vec<Vec<Duration>> = Vec::new();
    for n in [5_000, 20_000] {
        let probe = Arc::new(Probe::default());
        let mut m = machine(&crowded(n), probe.clone());
        let mut view = m.view(None).unwrap();
        assert_eq!((view.facts["t.Lent"].len(), view.duties["t.Give-Back"].len(), view.facts["t.Fine"].len()), (n, n, n));
        let book = |extra: &[Json]| [json!("w1"), json!("b1")].into_iter().chain(extra.iter().cloned()).collect::<Vec<_>>();
        let acts = [("t.Lend", book(&[json!("bob")])), ("t.Mark", book(&[])), ("t.Charge", book(&[json!(5)])), ("t.Pay", book(&[json!(5)])), ("t.Forgive", book(&[])), ("t.Take-Back", book(&[]))];
        let mut client = view.clone();
        let mut took = vec![[Vec::new(), Vec::new()]; acts.len()];
        let mut seq = 0;
        for round in 0..CROWDED_ROUNDS {
            for (k, (name, args)) in acts.iter().enumerate() {
                seq += 1;
                let (d, asked, t) = acted(&mut m, &probe, &mut view, seq, name, args);
                assert!(!d.is_empty(), "n = {n}, {name} moved something");
                // the count and the sum are kept a step at a time, so nothing is read whole
                assert!(asked.is_empty(), "n = {n}, {name}: {asked:?}");
                coordd_v2::delta::apply(&mut client, &d);
                (0..2).for_each(|w| took[k][w].push(t[w]));
                if round == 0 {
                    assert_eq!(client, m.view(None).unwrap(), "n = {n}, {name}: a client taking the deltas has the whole");
                }
            }
            assert_eq!((&view, &client), (&m.view(None).unwrap(), &view), "n = {n}: the views kept are the whole");
        }
        p50s.push(said(n, "in the changed type", &acts.map(|a| a.0), &took).iter().map(|[p50, _]| *p50).collect());
    }
    // four times the state, about the same delta: a millisecond besides for the tower's load
    for (k, (small, big)) in p50s[0].iter().zip(&p50s[1]).enumerate() {
        assert!(*big < *small * 3 + Duration::from_millis(1), "act {k}: p50 {small:?} at 5,000, {big:?} at 20,000");
    }
}
