//! A client's view made from the deltas alone, one by one or merged, is the view read whole after every act: first
//! a violated duty that ends and comes back in one frame, then random acts.

use coordd_v2::delta::{apply, Frame};
use coordd_v2::docket::{Bind, Docket, Texts};
use coordd_v2::machine::{Machine, View};
use coordd_v2::peer::Party;
use docket_v2::norm::Core;
use serde_json::json;

const TEXT: &str = r#"
module t
Fact Person Identified by String
Fact Book Identified by String
Fact Lent(book: Book, to: Person, by: Person)
Fact Overdue(book: Book)
Fact Owes(debtor: Person, to: Person, book: Book)
  Holds when Lent(book, debtor, to)
Fact Loans(n: Int)
  Holds when n == Count(Foreach b, p, q : b When Lent(b, p, q))
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
+Person("ann"). +Person("bob"). +Person("cy").
"#;

/// A fixed-seed draw, so a failing run comes back the same.
struct Draw(u64);

impl Draw {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) % n as u64) as usize
    }
}

fn machine() -> Docket<Core> {
    let loaded = docket_v2::check::load(vec![docket_v2::syntax::read(TEXT).unwrap()]).unwrap();
    let runs = docket_v2::run::machine::Machine::new(loaded, Core::default(), vec![]).unwrap();
    let (bind, texts): (Bind, Texts) = (Box::new(|_, _| (None, String::new())), Box::new(|_| Err("this record starts empty".into())));
    Docket::new(runs, "t", vec![], bind, texts)
}

/// Every type of V read whole, as no count sees it.
fn whole(m: &mut Docket<Core>, v: &View) -> View {
    let names: Vec<String> = v.facts.keys().chain(v.duties.keys()).cloned().collect();
    m.view(Some(&names)).unwrap()
}

#[test]
fn a_view_from_the_deltas_alone_is_the_whole_after_every_random_act() {
    let (people, books) = (["ann", "bob", "cy"], ["b1", "b2", "b3", "b4"]);
    let mut r = Draw(1915);
    let mut m = machine();
    let mut kept = m.view(None).unwrap();
    let (mut one, mut merged, mut pending) = (kept.clone(), kept.clone(), Frame::default());
    // first a violated duty that ends and comes back unviolated in one frame, then one cleared before it ends
    let script = [("t.Lend", "b1", true), ("t.Mark", "b1", false), ("t.Take-Back", "b1", false), ("t.Forgive", "b1", false), ("t.Lend", "b1", true)];
    let script = script.iter().chain(&[("t.Lend", "b2", true), ("t.Mark", "b2", false), ("t.Forgive", "b2", false), ("t.Take-Back", "b2", false), ("t.Lend", "b2", true)]);
    let script: Vec<_> = script.collect();
    let fixed = [2, 5, 7, 10];
    let (mut flush, mut seen) = (2, [0usize; 5]);
    for step in 1..=400u64 {
        let (by, book, to) = (people[r.below(3)], books[r.below(4)], people[r.below(3)]);
        if let Some(&&(name, book, lend)) = script.get(step as usize - 1) {
            let args: &[&str] = if lend { &["ann", book, "bob"] } else { &["ann", book] };
            m.act(&Party::of("ann", None), name, &args.iter().map(|a| json!(a)).collect::<Vec<_>>()).unwrap();
        } else {
            match (step, r.below(10)) {
                (200, _) => m.reload(docket_v2::check::load(vec![docket_v2::syntax::read(TEXT).unwrap()]).unwrap()).unwrap(),
                (_, 0) => drop(m.enabled_for(&Party::of(by, None), &json!(by), &json!({"inst": "t.Lend", "args": [by, book, to]})).unwrap()),
                (_, k) => {
                    let (name, args) = match k % 4 {
                        0 => ("t.Lend", vec![by, book, to]),
                        1 => ("t.Take-Back", vec![by, book]),
                        2 => ("t.Mark", vec![by, book]),
                        _ => ("t.Forgive", vec![by, book]),
                    };
                    drop(m.act(&Party::of(by, None), name, &args.iter().map(|a| json!(a)).collect::<Vec<_>>()));
                }
            }
        }
        let d = m.delta(&mut kept, step).unwrap();
        let loans = d.created_facts.iter().any(|f| f["inst"] == "t.Loans");
        for (k, moved) in [!d.violations.is_empty(), !d.cleared_violations.is_empty(), !d.terminated_duties.is_empty(), loans, d.is_empty()].into_iter().enumerate() {
            seen[k] += moved as usize;
        }
        apply(&mut one, &d);
        pending.push(&d);
        let truth = whole(&mut m, &kept);
        assert_eq!(kept, truth, "step {step}: the engine's own view");
        assert_eq!(one, truth, "step {step}: a client taking each delta, after {d:?}");
        if step >= flush {
            let frame = std::mem::take(&mut pending).delta();
            apply(&mut merged, &frame);
            assert_eq!(merged, truth, "step {step}: a client taking merged frames, after {frame:?}");
            if step == 5 || step == 10 {
                assert_eq!(frame.cleared_violations.len(), 1, "step {step}: the duty back unviolated is cleared: {frame:?}");
            }
            flush = fixed.into_iter().find(|f| *f > step).unwrap_or(step + 1 + r.below(5) as u64);
        }
    }
    eprintln!("400 random steps: violated {}, cleared {}, duties ended {}, count changed {}, nothing moved {}", seen[0], seen[1], seen[2], seen[3], seen[4]);
    assert!(seen.iter().all(|n| *n > 0), "every kind of move came up: {seen:?}");
}
