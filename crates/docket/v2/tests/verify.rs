//! std.verify on the eFLINT core: what refuses a verification or a release, and a synthesized E2E plan run to release.

use docket_v2::norm::Core;
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::machine::{Machine, State};
use docket_v2::run::norms::{Norms, Why};
use docket_v2::run::record::Line;
use docket_v2::run::value::{Instance, Value};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

fn loaded() -> docket_v2::check::names::Loaded {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../docket/std");
    let read = |f: &str| docket_v2::syntax::read(&std::fs::read_to_string(dir.join(f)).unwrap()).unwrap_or_else(|e| panic!("{f}: {e}"));
    let texts = ["verify.docket", "budgets.docket", "comms.docket", "hooks.docket"].map(read).to_vec();
    let mut l = docket_v2::check::names::load(texts).unwrap_or_else(|e| panic!("{e}"));
    l.rows = docket_v2::check::typing::check(&l).unwrap_or_else(|es| panic!("{}", es.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n")));
    l
}

fn v(ty: &str, args: &[&str]) -> Instance {
    Instance { ty: format!("std.verify.{ty}"), args: args.iter().map(|a| Value::text(a)).collect() }
}

/// Performs TY as BY (none for a driver's Event) and commits it, or says why it is refused.
fn act(core: &mut Core, by: Option<&str>, ty: &str, args: &[&str]) -> Result<(), Why> {
    let s = core.step(&v(ty, args), by.map(Value::text).as_ref())?;
    core.commit(s);
    Ok(())
}

fn ok(core: &mut Core, by: Option<&str>, ty: &str, args: &[&str]) {
    act(core, by, ty, args).unwrap_or_else(|e| panic!("{ty}{args:?} by {by:?}: {e:?}"));
}

fn refused(core: &mut Core, by: Option<&str>, ty: &str, args: &[&str]) {
    assert!(act(core, by, ty, args).is_err(), "{ty}{args:?} by {by:?} was let");
}

/// The runner's two lines: PLAN ran clean for VERIFIER's INTENT with evidence EV, and that run is attested.
fn attest(core: &mut Core, verifier: &str, intent: &str, plan: &str, ev: &str) {
    ok(core, None, "Ran-Clean-On", &[verifier, "c1", intent, plan, ev]);
    ok(core, None, "Attest", &[verifier, "c1", intent, plan, ev]);
}

/// Change c1 by ann, built, with intent i1 claimed and bob seated to verify it.
fn claimed() -> Core {
    let mut core = Core::checked();
    core.load(&loaded().norms).unwrap();
    ok(&mut core, Some("ann"), "Claim", &["ann", "c1", "i1", "bob"]);
    ok(&mut core, None, "Built-From", &["c1", "c1.exe"]);
    core
}

#[test]
fn self_verification_is_refused() {
    let mut core = Core::checked();
    core.load(&loaded().norms).unwrap();
    refused(&mut core, Some("ann"), "Claim", &["ann", "c1", "i1", "ann"]);
    ok(&mut core, Some("ann"), "Claim", &["ann", "c1", "i1", "bob"]);
    refused(&mut core, Some("cy"), "Claim", &["cy", "c1", "i2", "ann"]);
    refused(&mut core, Some("ann"), "Submit-Plan", &["ann", "c1", "i1", "p1"]);
    ok(&mut core, None, "Built-From", &["c1", "c1.exe"]);
    refused(&mut core, None, "Attest", &["ann", "c1", "i1", "p1", "e1"]);
    // a verifier who commits to the change is its author: it can no longer plan or be attested
    ok(&mut core, None, "Committed-By", &["c1", "bob"]);
    refused(&mut core, Some("bob"), "Submit-Plan", &["bob", "c1", "i1", "p1"]);
    assert!(core.holds(&v("Verify-Intent", &["bob", "ann", "c1", "i1"])));
    refused(&mut core, Some("ann"), "Release", &["ann", "c1"]);
}

#[test]
fn attesting_without_a_test_plan_first_is_refused() {
    let mut core = claimed();
    refused(&mut core, None, "Ran-Clean-On", &["bob", "c1", "i1", "p1", "e1"]);
    refused(&mut core, None, "Attest", &["bob", "c1", "i1", "p1", "e1"]);
    refused(&mut core, Some("bob"), "Report-Failure", &["bob", "c1", "i1", "p1", "f1", "e1"]);
    refused(&mut core, Some("bob"), "Submit-Plan", &["bob", "c1", "i1", ""]);
    ok(&mut core, Some("bob"), "Submit-Plan", &["bob", "c1", "i1", "p1"]);
    refused(&mut core, Some("bob"), "Submit-Plan", &["bob", "c1", "i1", "p2"]);
    refused(&mut core, None, "Attest", &["bob", "c1", "i1", "p2", "e1"]);
    refused(&mut core, None, "Ran-Clean-On", &["bob", "c1", "i1", "p1", ""]);
    refused(&mut core, None, "Attest", &["bob", "c1", "i1", "p1", "e1"]);
    let mut unbuilt = Core::checked();
    unbuilt.load(&loaded().norms).unwrap();
    ok(&mut unbuilt, Some("ann"), "Claim", &["ann", "c1", "i1", "bob"]);
    ok(&mut unbuilt, Some("bob"), "Submit-Plan", &["bob", "c1", "i1", "p1"]);
    refused(&mut unbuilt, None, "Ran-Clean-On", &["bob", "c1", "i1", "p1", "e1"]);
    attest(&mut core, "bob", "i1", "p1", "e1");
    assert!(!core.holds(&v("Verify-Intent", &["bob", "ann", "c1", "i1"])));
}

#[test]
fn releasing_while_verify_intent_or_resolve_is_undischarged_is_refused() {
    let mut core = Core::checked();
    core.load(&loaded().norms).unwrap();
    refused(&mut core, Some("ann"), "Release", &["ann", "c1"]);
    let mut core = claimed();
    ok(&mut core, Some("ann"), "Claim", &["ann", "c1", "i2", "cy"]);
    ok(&mut core, Some("bob"), "Submit-Plan", &["bob", "c1", "i1", "p1"]);
    attest(&mut core, "bob", "i1", "p1", "e1");
    assert!(core.holds(&v("Verify-Intent", &["cy", "ann", "c1", "i2"])));
    refused(&mut core, Some("ann"), "Release", &["ann", "c1"]);
    ok(&mut core, Some("cy"), "Submit-Plan", &["cy", "c1", "i2", "p2"]);
    ok(&mut core, Some("cy"), "Report-Failure", &["cy", "c1", "i2", "p2", "f1", "e2"]);
    assert!(core.holds(&v("Resolve", &["ann", "cy", "c1", "f1"])));
    ok(&mut core, None, "Ran-Clean-On", &["cy", "c1", "i2", "p2", "e3"]);
    refused(&mut core, None, "Attest", &["cy", "c1", "i2", "p2", "e3"]);
    refused(&mut core, Some("ann"), "Release", &["ann", "c1"]);
    refused(&mut core, Some("cy"), "Clear-Failure", &["cy", "c1", "f1"]);
    ok(&mut core, Some("ann"), "Claim-Fix", &["ann", "c1", "f1"]);
    refused(&mut core, Some("cy"), "Clear-Failure", &["cy", "c1", "f1"]);
    ok(&mut core, None, "Ran-Clean-On", &["cy", "c1", "i2", "p2", "e4"]);
    refused(&mut core, Some("ann"), "Clear-Failure", &["ann", "c1", "f1"]);
    refused(&mut core, Some("bob"), "Clear-Failure", &["bob", "c1", "f1"]);
    refused(&mut core, Some("ann"), "Release", &["ann", "c1"]);
    ok(&mut core, Some("cy"), "Clear-Failure", &["cy", "c1", "f1"]);
    assert!(!core.holds(&v("Resolve", &["ann", "cy", "c1", "f1"])));
    refused(&mut core, Some("ann"), "Release", &["ann", "c1"]);
    ok(&mut core, None, "Attest", &["cy", "c1", "i2", "p2", "e4"]);
    ok(&mut core, Some("ann"), "Release", &["ann", "c1"]);
    refused(&mut core, Some("ann"), "Claim", &["ann", "c1", "i3", "bob"]);
}

#[test]
fn a_challenged_plan_must_be_replaced_before_the_duty_is_discharged() {
    let mut core = claimed();
    ok(&mut core, Some("bob"), "Submit-Plan", &["bob", "c1", "i1", "p1"]);
    refused(&mut core, Some("bob"), "Challenge-Plan", &["bob", "c1", "i1", "p1", "mine"]);
    refused(&mut core, Some("cy"), "Challenge-Plan", &["cy", "c1", "i1", "p1", ""]);
    ok(&mut core, Some("cy"), "Challenge-Plan", &["cy", "c1", "i1", "p1", "only checks the window opens"]);
    refused(&mut core, None, "Attest", &["bob", "c1", "i1", "p1", "e1"]);
    refused(&mut core, Some("bob"), "Submit-Plan", &["bob", "c1", "i1", "p1"]);
    ok(&mut core, Some("bob"), "Submit-Plan", &["bob", "c1", "i1", "p2"]);
    attest(&mut core, "bob", "i1", "p2", "e2");
    assert!(!core.holds(&v("Verify-Intent", &["bob", "ann", "c1", "i1"])));
    // a challenge after the attestation opens the duty again
    ok(&mut core, Some("ann"), "Challenge-Plan", &["ann", "c1", "i1", "p2", "never opens the new page"]);
    assert!(core.holds(&v("Verify-Intent", &["bob", "ann", "c1", "i1"])));
    refused(&mut core, Some("ann"), "Release", &["ann", "c1"]);
    ok(&mut core, Some("bob"), "Submit-Plan", &["bob", "c1", "i1", "p3"]);
    attest(&mut core, "bob", "i1", "p3", "e3");
    ok(&mut core, Some("ann"), "Release", &["ann", "c1"]);
}

#[test]
fn clearing_a_failure_without_a_clean_run_after_the_fix_is_refused() {
    let mut core = claimed();
    ok(&mut core, Some("bob"), "Submit-Plan", &["bob", "c1", "i1", "p1"]);
    ok(&mut core, None, "Ran-Clean-On", &["bob", "c1", "i1", "p1", "e0"]);
    ok(&mut core, Some("bob"), "Report-Failure", &["bob", "c1", "i1", "p1", "f1", "e1"]);
    ok(&mut core, Some("ann"), "Claim-Fix", &["ann", "c1", "f1"]);
    assert!(!core.holds(&v("Ran-Clean", &["c1", "i1", "p1", "e0"])), "a clean run before the fix is stale");
    refused(&mut core, Some("bob"), "Clear-Failure", &["bob", "c1", "f1"]);
    ok(&mut core, None, "Ran-Clean-On", &["bob", "c1", "i1", "p1", "e2"]);
    ok(&mut core, Some("bob"), "Clear-Failure", &["bob", "c1", "f1"]);
    attest(&mut core, "bob", "i1", "p1", "e3");
    ok(&mut core, Some("ann"), "Release", &["ann", "c1"]);
}

type Asked = Rc<RefCell<Vec<(String, Vec<Value>)>>>;

/// The verifier's model and the E2E runner: fixed answers, and a queue for the plans, scenarios, seals and waits.
struct Runner(VecDeque<(&'static str, Reply)>, Asked);

impl Driver for Runner {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("e2e"), Value::text("1")]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        let mut e: Vec<(String, Vec<String>)> = ["Diff", "Synthesize", "Interact", "Expect", "Seal", "Confirm", "Await-Fix", "Seat-Verifier", "Start-Verify"].iter().map(|e| (format!("std.verify.{e}"), Vec::new())).collect();
        e.push(("Gap".into(), Vec::new()));
        // the imported texts' own Effects, so their reactions start; none is asked here
        let imported = ["std.hooks.Start-Session", "std.hooks.Tool-Use", "std.hooks.Next-Moment", "std.hooks.Apply", "std.comms.Call-Text", "std.comms.Model", "std.comms.Read-As", "std.comms.Pick", "std.comms.TextOf", "std.budgets.Request-Budget", "std.budgets.Command", "std.budgets.Measure"];
        e.extend(imported.iter().map(|n| (n.to_string(), Vec::new())));
        e
    }
    fn events(&self) -> Vec<String> {
        ["Ran-Clean-On", "Attest", "Built-From", "Committed-By"].iter().map(|e| format!("std.verify.{e}")).collect()
    }
    fn carry(&mut self, op: &str, args: &[Value], _: u64) -> Reply {
        self.1.borrow_mut().push((op.to_string(), args.to_vec()));
        match op.trim_start_matches("std.verify.") {
            "Diff" => answer(Value::text("+ list new projects on the home"), vec![]),
            "Seat-Verifier" => answer(Value::text("bob"), vec![]),
            "Start-Verify" => answer(Value::Unit, vec![]),
            "Interact" => answer(Value::text("the home with c1's project"), vec![]),
            "Expect" => answer(Value::Bool(true), vec![]),
            op => {
                let (want, reply) = self.0.pop_front().unwrap_or_else(|| panic!("{op} was not expected"));
                assert_eq!(op, want);
                reply
            }
        }
    }
    fn dropped(&mut self, _: u64) {}
}

fn answer(value: Value, events: Vec<Instance>) -> Reply {
    Reply::Answer { value, costs: Vec::new(), events }
}

/// The emergent run: a plan synthesized and run as a filled program, a failure, a challenge, a stronger plan, release.
#[test]
fn verify_synthesizes_runs_and_replans_until_the_intent_is_attested() {
    let weak = "{ seen <- perform Interact(\"c1\", \"open the window\"); return Falsified(\"the new project is missing\") }";
    let strong = "{ seen <- perform Interact(\"c1\", \"make a project, open the home\"); ok <- perform Expect(\"c1\", seen, \"the project is listed\"); if ok { return Delivered } else { return Falsified(\"not listed\") } }";
    let mut script: VecDeque<(&'static str, Reply)> = VecDeque::new();
    script.push_back(("Synthesize", answer(Value::text("plan-1"), vec![])));
    script.push_back(("Gap", answer(Value::text(weak), vec![])));
    script.push_back(("Seal", answer(Value::text("run-1.webm"), vec![])));
    script.push_back(("Await-Fix", Reply::Later));
    script.push_back(("Synthesize", answer(Value::text("plan-2"), vec![])));
    script.push_back(("Gap", answer(Value::text(strong), vec![])));
    script.push_back(("Seal", answer(Value::text("run-2.webm"), vec![v("Ran-Clean-On", &["bob", "c1", "i1", "plan-2", "run-2.webm"])])));
    script.push_back(("Confirm", answer(Value::Unit, vec![v("Attest", &["bob", "c1", "i1", "plan-2", "run-2.webm"])])));
    let asked: Asked = Default::default();
    let mut m = Machine::new(loaded(), Core::checked(), vec![Box::new(Runner(script, asked.clone()))]).unwrap();
    let start = |m: &mut Machine<Core>, by: &str, i: Instance| {
        let r = m.start(Value::text(by), Some(0), "", i).unwrap_or_else(|e| panic!("{e}"));
        m.run();
        r
    };
    let ask = start(&mut m, "ann", v("Request-Verification", &["ann", "c1", "i1"]));
    assert_eq!(m.state(ask), State::Done, "{:?}", m.record);
    assert!(m.norms.holds(&v("Verify-Intent", &["bob", "ann", "c1", "i1"])));
    m.event(0, v("Built-From", &["c1", "c1.exe"]));
    let run = start(&mut m, "bob", v("Run-Verification", &["bob", "c1", "i1", "verify-c1"]));
    let State::Resting(wait) = m.state(run) else { panic!("the run waits for the fix: {:?}", m.record) };
    assert!(m.norms.holds(&v("Resolve", &["ann", "bob", "c1", "the new project is missing"])));
    let early = start(&mut m, "ann", v("Release", &["ann", "c1"]));
    assert_eq!(m.state(early), State::Raised, "a release with duties owed is refused");
    start(&mut m, "ann", v("Claim-Fix", &["ann", "c1", "the new project is missing"]));
    start(&mut m, "cy", v("Challenge-Plan", &["cy", "c1", "i1", "plan-1", "never makes a project"]));
    m.answer(wait, answer(Value::Unit, vec![])).unwrap();
    m.run();
    assert_eq!(m.state(run), State::Done, "{:?}", m.record);
    assert!(m.record.iter().any(|l| matches!(l, Line::Ended { value: Value::Ctor { name, args }, .. } if name == "std.budgets.Ok" && args == &[Value::Bool(true)])));
    let synth: Vec<Vec<Value>> = asked.borrow().iter().filter(|(o, _)| o == "std.verify.Synthesize").map(|(_, a)| a.clone()).collect();
    assert_eq!(synth.len(), 2);
    assert_eq!(synth[1][4], Value::List(vec![Value::text("never makes a project")]), "the second plan is asked to answer the challenge");
    let release = start(&mut m, "ann", v("Release", &["ann", "c1"]));
    assert_eq!(m.state(release), State::Done, "{:?}", m.record);
    assert!(m.norms.holds(&v("Released", &["c1"])));
}
