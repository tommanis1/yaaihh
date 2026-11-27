//! The explainer's limits: the budget marks what it did not ask and a cut or circular proof is never given, a synced act
//! is explained once, the history is read once, a `Forall` reads its body as it stands, and an act's refusal is told whole.

use super::why::{Asker, BUDGET, TRIES};
use super::{Core, cycle};
use crate::run::explain::{Done, Explain, Holds, Premise, WhyFailed};
use crate::run::norms::{NormDecl, Norms};
use crate::run::value::{Instance, Value};
use std::time::{Duration, Instant};

fn decls(text: &str) -> Vec<NormDecl> {
    crate::check::load(vec![crate::syntax::read(text).unwrap()]).unwrap().norms
}

fn loaded(text: &str) -> Core {
    let mut core = Core::default();
    core.load(&decls(text)).unwrap();
    core
}

fn inst(ty: &str, args: &[&str]) -> Instance {
    Instance { ty: format!("t.{ty}"), args: args.iter().map(|a| Value::text(a)).collect() }
}

fn by(core: &mut Core, ty: &str, args: &[&str]) {
    let s = core.step(&inst(ty, args), Some(&Value::text(args[0]))).unwrap();
    core.commit(s);
}

/// Every instance the tree names is said to hold exactly when it holds.
fn truthful(core: &mut Core, e: &Explain) {
    if let Some(h) = e.holds() {
        assert_eq!(core.holds(e.inst()), h, "{} is said to hold: {h}", e.inst());
    }
    match e {
        Explain::True { by, .. } => holds(core, by),
        Explain::False { overridden, rules, .. } => {
            overridden.iter().for_each(|h| holds(core, h));
            rules.iter().flat_map(|r| &r.premises).for_each(|p| premise(core, p));
        }
        Explain::OnPath { .. } | Explain::Unknown { .. } => {}
    }
}

fn holds(core: &mut Core, h: &Holds) {
    if let Holds::Rule(r) = h {
        r.premises.iter().for_each(|p| premise(core, p));
    }
}

fn premise(core: &mut Core, p: &Premise) {
    p.parts.iter().for_each(|q| premise(core, q));
    p.under.iter().for_each(|e| truthful(core, e));
}

/// Nothing in the tree is cut or stands on its path.
fn clean(e: &Explain) -> bool {
    let rule = |r: &crate::run::explain::Rule| !r.cut && r.premises.iter().all(clean_premise);
    match e {
        Explain::True { by: Holds::Rule(r), .. } => rule(r),
        Explain::True { by, .. } => !matches!(by, Holds::Unproved { .. }),
        Explain::False { overridden, rules, .. } => overridden.iter().all(|h| clean(&Explain::True { inst: e.inst().clone(), by: (**h).clone() })) && rules.iter().all(rule),
        Explain::OnPath { .. } => false,
        Explain::Unknown { .. } => true,
    }
}

fn clean_premise(p: &Premise) -> bool {
    !p.cut && p.parts.iter().all(clean_premise) && p.under.iter().all(clean)
}

const LOOP: &str = r#"module t
Fact Node Identified by String
Fact Edge(a: Node, b: Node)
Fact Base(x: Node)
Fact R(x: Node)
  Holds when Base(x)
  Derived from Foreach a, b : R(a) When Edge(a, b) && R(b)
Act Link(by: Node, a: Node, b: Node) Creates Edge(a, b)
Act Ground(by: Node, x: Node) Creates Base(x)
+Node("n1"). +Node("n2").
"#;

/// R(n1) holds through n2's base fact, its loop through itself never taken; with one try only the loop is found, and
/// the answer says no proof stands apart; without the base fact it does not hold, and nothing in either says otherwise.
#[test]
fn a_fact_that_holds_through_a_loop_and_a_base_fact_is_proved_by_the_base_fact() {
    let mut core = loaded(LOOP);
    by(&mut core, "Link", &["n1", "n1", "n1"]);
    by(&mut core, "Link", &["n1", "n1", "n2"]);
    let r1 = inst("R", &["n1"]);
    let none = Asker::of(&mut core, BUDGET, TRIES).unwrap().explain(&r1).unwrap();
    assert!(matches!(none, Explain::False { .. }), "no base fact: {none:?}");
    truthful(&mut core, &none);

    by(&mut core, "Ground", &["n1", "n2"]);
    let e = Asker::of(&mut core, BUDGET, TRIES).unwrap().explain(&r1).unwrap();
    truthful(&mut core, &e);
    assert!(clean(&e), "{e:?}");
    let Explain::True { by: Holds::Rule(rule), .. } = &e else { panic!("{e:?}") };
    let [Explain::True { by: Holds::Rule(r2), .. }] = rule.premises[1].under.as_slice() else { panic!("R(n2) holds: {rule:?}") };
    assert_eq!((rule.env.get("b"), r2.premises[0].text.as_str()), (Some(&Value::text("n2")), "t.Base(x)"));

    let one = Asker::of(&mut core, BUDGET, 1).unwrap().explain(&r1).unwrap();
    assert_eq!(one, Explain::True { inst: r1, by: Holds::Unproved { tries: 1, cut: false } });
}

/// At every budget R(n1) is proved whole, through n2's base fact, or the answer says the budget ran out first: the loop
/// through itself, cut before it reaches R(n1) again, is never given as its proof.
#[test]
fn a_proof_the_budget_cuts_is_never_given_as_one() {
    let mut core = loaded(LOOP);
    by(&mut core, "Link", &["n1", "n1", "n1"]);
    by(&mut core, "Link", &["n1", "n1", "n2"]);
    by(&mut core, "Ground", &["n1", "n2"]);
    let r1 = inst("R", &["n1"]);
    let mut proved = 0;
    for budget in 0..64 {
        let e = Asker::of(&mut core, budget, TRIES).unwrap().explain(&r1).unwrap();
        truthful(&mut core, &e);
        match &e {
            Explain::True { by: Holds::Rule(r), .. } => {
                assert!(clean(&e) && r.env.get("b") == Some(&Value::text("n2")), "budget {budget}: {e:?}");
                proved += 1;
            }
            Explain::True { by: Holds::Unproved { cut: true, .. }, .. } => {}
            other => panic!("budget {budget}: {other:?}"),
        }
    }
    assert!(proved > 0, "a budget of 63 proves it");
}

/// Past the budget a premise is marked cut, with nothing looked at beneath it, and the refusal says it was cut; with
/// budget enough all three failing conjuncts are told whole.
#[test]
fn a_condition_wider_than_the_budget_marks_the_premises_it_did_not_expand() {
    let text = "module t\nFact Person Identified by String\nFact Member(p: Person)\nFact Paid(p: Person)\nFact Signed(p: Person)\n\
                Act Enter(by: Person) Conditioned by Member(by) && Paid(by) && Signed(by)\n+Person(\"ann\").\n";
    let mut core = loaded(text);
    let enter = inst("Enter", &["ann"]);
    for budget in 0..16 {
        let w = Asker::of(&mut core, budget, TRIES).unwrap().refused(&enter).unwrap();
        assert!(w.refuses(), "budget {budget}");
        assert!(w.failing.iter().all(|p| !p.cut || (p.parts.is_empty() && p.under.is_empty())), "budget {budget}: {w:?}");
        assert!(w.cut || (w.failing.len() == 3 && w.failing.iter().all(|p| !p.cut)), "budget {budget}: {w:?}");
    }
    assert!(Asker::of(&mut core, 2, TRIES).unwrap().refused(&enter).unwrap().cut);
    let whole = Asker::of(&mut core, BUDGET, TRIES).unwrap().refused(&enter).unwrap();
    assert!(!whole.cut && whole.failing.len() == 3 && whole.failing.iter().all(|p| !p.cut && p.under.len() == 1), "{whole:?}");
}

/// S0 syncs with P0 and Q0, each of which syncs with S1, and so on N times: 2^N paths to S_N, whose condition fails.
fn diamonds(n: usize) -> String {
    let mut text = "module t\nFact Person Identified by String\nFact Member(p: Person)\n".to_string();
    for k in 0..n {
        let next = k + 1;
        text += &format!("Act S{k}(by: Person) Syncs with P{k}(by), Q{k}(by)\nAct P{k}(by: Person) Syncs with S{next}(by)\nAct Q{k}(by: Person) Syncs with S{next}(by)\n");
    }
    text + &format!("Act S{n}(by: Person) Conditioned by Member(by)\n+Person(\"ann\").\n")
}

fn size(w: &WhyFailed) -> usize {
    1 + w.syncs.iter().map(size).sum::<usize>()
}

/// Each act of a chain of diamonds is explained once: the second way to S_k+1 names it again, not its tree.
#[test]
fn a_diamond_chain_of_synced_acts_is_explained_once_per_act() {
    let mut small = loaded(&diamonds(3));
    let s0 = inst("S0", &["ann"]);
    let w = Asker::of(&mut small, BUDGET, TRIES).unwrap().refused(&s0).unwrap();
    assert!(small.step(&s0, Some(&Value::text("ann"))).is_err() && w.refuses());
    let [p0, q0] = w.syncs.as_slice() else { panic!("{w:?}") };
    assert_eq!((p0.again, &q0.syncs[0].act, q0.syncs[0].again), (false, &inst("S1", &["ann"]), true));

    const N: usize = 24;
    let mut core = loaded(&diamonds(N));
    let w = Asker::of(&mut core, BUDGET, TRIES).unwrap().refused(&s0).unwrap();
    assert!(w.refuses() && !w.cut);
    assert_eq!(size(&w), 4 * N + 1, "S0..S{N}, each P and Q, and each Q's S again");
}

/// Over 50 000 committed steps, a thousand calls of the core's `explain` take well under two seconds, each naming the
/// step that made its fact, and a viewed past state names the step that ended it then.
#[test]
fn a_long_history_is_not_read_again_by_each_explanation() {
    let text = "module t\nFact Person Identified by String\nFact Member(p: Person)\nAct Join(by: Person) Creates Member(by)\n\
                Act Leave(by: Person) Terminates Member(by)\n";
    let mut core = loaded(text);
    by(&mut core, "Join", &["ann"]);
    for _ in 0..25_000 {
        by(&mut core, "Join", &["bob"]);
        by(&mut core, "Leave", &["bob"]);
    }
    by(&mut core, "Join", &["bob"]);
    let (ann, bob) = (inst("Member", &["ann"]), inst("Member", &["bob"]));
    let t = Instant::now();
    for _ in 0..1000 {
        let e = core.explain(&ann).unwrap();
        assert_eq!(e, Explain::True { inst: ann.clone(), by: Holds::Stated(Done { step: 1, act: Some(inst("Join", &["ann"])) }) });
    }
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
    let Explain::True { by: Holds::Stated(d), .. } = core.explain(&bob).unwrap() else { panic!() };
    assert_eq!(d.step, 50_002);
    core.view(Some(50_001));
    let Explain::False { ended: Some(d), .. } = core.explain(&bob).unwrap() else { panic!() };
    assert_eq!((d.step, d.act), (50_001, Some(inst("Leave", &["bob"]))));
    core.view(None);
}

/// An act named with a field too few, too many or of the wrong type is refused an explanation, not said to be enabled.
#[test]
fn an_act_without_its_fields_is_no_act_to_explain() {
    let text = "module t\nFact Person Identified by String\nFact Book Identified by String\nFact Shelved(b: Book)\n\
                Act Borrow(by: Person, b: Book) Conditioned by Shelved(b)\n";
    let mut core = loaded(text);
    for args in [vec![Value::text("ann")], vec![Value::text("ann"), Value::text("b1"), Value::text("x")], vec![Value::text("ann"), Value::Int(1)]] {
        let i = Instance { ty: "t.Borrow".into(), args };
        assert!(core.why_refused(&i).is_err(), "{i}: {:?}", core.why_refused(&i));
    }
    let w = core.why_refused(&inst("Borrow", &["ann", "b1"])).unwrap();
    assert!(w.refuses(), "{w:?}");
}

/// `Forall y : Not(Q(x)) || …` reads Q(x) under `Not`, `Forall y : Q(x) || …` does not, and `Forall q : Rank(q)` with
/// q a Q reads every Q falling.
#[test]
fn a_forall_reads_its_body_as_it_stands_and_its_domain_falling() {
    let base = "module t\nFact Person Identified by String\nFact Seen(p: Person)\nFact Rank(q: Q)\n";
    let q = inst("Q", &["ann"]);
    let neg = cycle(&decls(&format!("{base}Fact Q(x: Person) Holds when Forall y : Not(Q(x)) || Seen(y)\n")), &q);
    assert!(neg.as_ref().is_some_and(|c| c.iter().any(|l| l.negated)), "{neg:?}");
    let pos = cycle(&decls(&format!("{base}Fact Q(x: Person) Holds when Forall y : Q(x) || Seen(y)\n")), &q);
    assert_eq!(pos, None);
    let dom = cycle(&decls(&format!("{base}Fact Q(x: Person) Holds when Forall r : Rank(r)\n")), &q);
    assert!(dom.as_ref().is_some_and(|c| c.last().is_some_and(|l| l.negated && l.inst == q)), "{dom:?}");
}

/// An act is refused exactly when its refusal says why, before and after the fact its conditions read is made: its
/// own `Holds when` is not read for whether it is enabled, as eFLINT's ranging reading has it.
#[test]
fn an_act_with_holds_when_is_refused_exactly_when_its_refusal_says_why() {
    let text = "module t\nFact Person Identified by String\nFact Member(p: Person)\nAct Join(by: Person) Creates Member(by)\n\
                Act Enter(by: Person) Holds when Member(by)\nAct Leave(by: Person) Conditioned by Member(by) Syncs with Enter(by)\n\
                Act Wave(by: Person) Syncs with Leave(by)\n+Person(\"ann\").\n";
    let mut core = loaded(text);
    let mut seen = Vec::new();
    for joined in [false, true] {
        if joined {
            by(&mut core, "Join", &["ann"]);
        }
        for act in ["Enter", "Leave", "Wave"] {
            let i = inst(act, &["ann"]);
            let w = Asker::of(&mut core, BUDGET, TRIES).unwrap().refused(&i).unwrap();
            let refused = core.step(&i, Some(&Value::text("ann"))).is_err();
            assert_eq!(w.refuses(), refused, "{act} joined {joined}: {w:?}");
            seen.push(refused);
        }
    }
    assert_eq!(seen, [false, true, true, false, false, false]);
}
