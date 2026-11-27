//! docket/eval typed and run on the eFLINT core: the seal, the caps as std.budgets' limits, trials, and the front.

use docket_v2::check::load;
use docket_v2::norm::Core;
use docket_v2::run::norms::{Norms, Why};
use docket_v2::run::value::{Instance, Value};

fn text(rel: &str) -> String {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../docket/").to_string() + rel;
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

/// The project's own text: which seated role sets a suite's caps.
const PROJECT: &str = "module project\nimport eval as E\n+E.Caps-By(\"owner\").\n";

fn core() -> Core {
    let texts = ["std/hooks.docket", "std/comms.docket", "std/budgets.docket", "eval/eval.docket"];
    let mut modules: Vec<_> = texts.iter().map(|f| docket_v2::syntax::read(&text(f)).unwrap_or_else(|e| panic!("{f}: {e}"))).collect();
    modules.push(docket_v2::syntax::read(PROJECT).unwrap());
    let l = load(modules).unwrap_or_else(|e| panic!("{e}"));
    let mut core = Core::default();
    core.load(&l.norms).unwrap();
    core
}

fn t(s: &str) -> Value {
    Value::text(s)
}

fn n(x: i64) -> Value {
    Value::Int(x)
}

fn i(ty: &str, args: Vec<Value>) -> Instance {
    Instance { ty: ty.into(), args }
}

/// TY with ARGS done by PARTY, kept if allowed.
fn act(core: &mut Core, party: &str, ty: &str, args: Vec<Value>) -> Result<(), Why> {
    let s = core.step(&i(ty, args), Some(&t(party)))?;
    core.commit(s);
    Ok(())
}

fn event(core: &mut Core, ty: &str, args: Vec<Value>) {
    let s = core.step(&i(ty, args), None).unwrap_or_else(|e| panic!("{ty}: {e:?}"));
    core.commit(s);
}

fn refused() -> Result<(), Why> {
    Err(Why::Policy("not enabled".into()))
}

/// The owner, a loader, a runner, a grader and a proposer seated.
fn seated() -> Core {
    let mut c = core();
    event(&mut c, "std.comms.Owner-Seated", vec![t("him")]);
    for (who, role) in [("load", "loader"), ("run", "runner"), ("grade", "grader"), ("prop", "proposer")] {
        act(&mut c, "him", "std.comms.Seat", vec![t("him"), t(who), t(role)]).unwrap();
    }
    c
}

/// Suite `s` holding TASKS, each offering its instruction and hiding `tests/*`, capped, with K trials a task.
fn suite(tasks: &[&str], k: i64) -> Core {
    let mut c = seated();
    for task in tasks {
        act(&mut c, "load", "eval.Load-Task", vec![t("load"), t(task), t("s")]).unwrap();
        act(&mut c, "load", "eval.Offer", vec![t("load"), t(task), t("instruction.md")]).unwrap();
        act(&mut c, "load", "eval.Hide", vec![t("load"), t(task), t("tests/*")]).unwrap();
    }
    for (dim, max) in [("tokens", 1000), ("usd_cents", 50), ("wall_ms", 60_000)] {
        act(&mut c, "him", "eval.Cap", vec![t("him"), t("s"), t(dim), n(max)]).unwrap();
    }
    act(&mut c, "him", "eval.Set-Trials", vec![t("him"), t("s"), n(k)]).unwrap();
    c
}

fn start(c: &mut Core, cand: &str, task: &str, trial: i64, run: &str, sealed: bool) -> Result<(), Why> {
    act(c, "run", "eval.Start-Run", vec![t("run"), t("agent"), t(cand), t(task), n(trial), t(run), Value::Bool(sealed)])
}

fn result(c: &mut Core, run: &str, pass: bool, cents: i64, tokens: i64, ms: i64) -> Result<(), Why> {
    act(c, "grade", "eval.Record-Result", vec![t("grade"), t(run), Value::Bool(pass), n(cents), n(tokens), n(ms), t("eval-traces/x")])
}

fn holds(c: &mut Core, ty: &str, args: Vec<Value>) -> bool {
    c.holds(&i(ty, args))
}

#[test]
fn a_run_starts_only_sealed_and_a_broken_seal_fails_it() {
    let mut c = suite(&["a", "b", "leaky", "odd"], 1);
    act(&mut c, "load", "eval.Offer", vec![t("load"), t("leaky"), t("tests/test.sh")]).unwrap();
    act(&mut c, "load", "eval.Offer", vec![t("load"), t("odd"), t("src/../tests/test.sh")]).unwrap();
    assert_eq!(start(&mut c, "c1", "a", 1, "r1", false), refused(), "a job with network");
    assert_eq!(start(&mut c, "c1", "leaky", 1, "r1", true), refused(), "a tree with a grader file");
    assert_eq!(start(&mut c, "c1", "odd", 1, "r1", true), refused(), "a grader file named another way");
    assert_eq!(act(&mut c, "agent", "eval.Start-Run", vec![t("agent"), t("agent"), t("c1"), t("a"), n(1), t("r1"), Value::Bool(true)]), refused(), "a run started by the one it evaluates");
    start(&mut c, "c1", "a", 1, "r1", true).unwrap();
    assert_eq!(start(&mut c, "c2", "b", 1, "r1", true), refused(), "a run's bucket is its own");
    start(&mut c, "c1", "b", 1, "r2", true).unwrap();
    assert!(holds(&mut c, "eval.Evaluating", vec![t("agent"), t("a")]));
    assert_eq!(act(&mut c, "load", "eval.Load-Task", vec![t("load"), t("a"), t("other")]), refused(), "a task is of one suite");

    assert_eq!(act(&mut c, "agent", "eval.Record-Result", vec![t("agent"), t("r1"), Value::Bool(true), n(1), n(1), n(1), t("x")]), refused(), "the one evaluated records no result");
    event(&mut c, "eval.Opened", vec![t("r1"), t("src/lib.rs")]);
    event(&mut c, "eval.Opened", vec![t("r2"), t("tests/test.sh")]);
    result(&mut c, "r1", true, 5, 10, 100).unwrap();
    result(&mut c, "r2", true, 5, 10, 100).unwrap();
    assert_eq!(result(&mut c, "r1", false, 5, 10, 100), refused(), "one result for a run");
    assert!(holds(&mut c, "eval.Passed", vec![t("r1")]));
    assert!(!holds(&mut c, "eval.Passed", vec![t("r2")]) && holds(&mut c, "eval.Failed", vec![t("r2")]), "reading a grader file fails a passing run");
    assert!(!holds(&mut c, "eval.Evaluating", vec![t("agent"), t("a")]));

    // a path written another way counts as a grader file: `..`, `.`, a leading `/`, an empty part
    for (k, path) in ["src/../tests/test.sh", "./tests/test.sh", "/work/tests/test.sh", "tests//test.sh"].iter().enumerate() {
        let (cand, run) = (format!("o{k}"), format!("ro{k}"));
        start(&mut c, &cand, "a", 1, &run, true).unwrap();
        event(&mut c, "eval.Opened", vec![t(&run), t(path)]);
        result(&mut c, &run, true, 5, 10, 100).unwrap();
        assert!(holds(&mut c, "eval.Failed", vec![t(&run)]), "{path}");
    }

    start(&mut c, "c2", "a", 1, "r3", true).unwrap();
    event(&mut c, "eval.Egress", vec![t("r3"), t("example.org")]);
    result(&mut c, "r3", true, 5, 10, 100).unwrap();
    assert!(holds(&mut c, "eval.Failed", vec![t("r3")]), "a connection out fails a passing run");
}

#[test]
fn a_result_is_never_negative_and_a_pass_past_a_cap_is_a_fail() {
    let mut c = suite(&["a"], 1);
    for (k, (cents, tokens, ms)) in [(-1, 10, 100), (5, -1, 100), (5, 10, -1)].into_iter().enumerate() {
        let run = format!("n{k}");
        start(&mut c, &format!("cn{k}"), "a", 1, &run, true).unwrap();
        assert_eq!(result(&mut c, &run, true, cents, tokens, ms), refused(), "{cents} {tokens} {ms}");
    }
    for (k, (cents, tokens, ms)) in [(51, 10, 100), (5, 1001, 100), (5, 10, 60_001), (50, 1000, 60_000)].into_iter().enumerate() {
        let run = format!("p{k}");
        start(&mut c, &format!("cp{k}"), "a", 1, &run, true).unwrap();
        result(&mut c, &run, true, cents, tokens, ms).unwrap();
        assert_eq!(holds(&mut c, "eval.Passed", vec![t(&run)]), k == 3, "{cents} {tokens} {ms}: at a cap is within it");
    }
}

#[test]
fn a_runs_caps_are_its_limits_in_the_ledger_and_the_one_it_meters_writes_none() {
    let mut c = seated();
    act(&mut c, "load", "eval.Load-Task", vec![t("load"), t("a"), t("s")]).unwrap();
    act(&mut c, "him", "eval.Cap", vec![t("him"), t("s"), t("tokens"), n(1000)]).unwrap();
    act(&mut c, "him", "eval.Set-Trials", vec![t("him"), t("s"), n(1)]).unwrap();
    assert_eq!(start(&mut c, "c1", "a", 1, "r1", true), refused(), "a suite capped in one dimension of three");
    act(&mut c, "him", "eval.Cap", vec![t("him"), t("s"), t("usd_cents"), n(50)]).unwrap();
    assert_eq!(act(&mut c, "him", "eval.Cap", vec![t("him"), t("s"), t("wall_ms"), n(0)]), refused(), "a cap of nothing");
    assert_eq!(act(&mut c, "load", "eval.Cap", vec![t("load"), t("s"), t("wall_ms"), n(60_000)]), refused(), "caps are the project's role's");
    act(&mut c, "him", "eval.Cap", vec![t("him"), t("s"), t("wall_ms"), n(60_000)]).unwrap();
    start(&mut c, "c1", "a", 1, "r1", true).unwrap();
    assert!(holds(&mut c, "std.budgets.Cap", vec![t("r1"), t("usd_cents"), n(50)]));
    assert!(holds(&mut c, "std.budgets.Metered", vec![t("r1"), t("agent")]));

    let spend = |c: &mut Core, dim: &str, x: i64| act(c, "agent", "std.budgets.Spend", vec![t("agent"), t("r1"), t(dim), n(x)]);
    spend(&mut c, "tokens", 900).unwrap();
    assert_eq!(spend(&mut c, "tokens", 101), refused(), "a spend past the cap");
    assert_eq!(spend(&mut c, "tokens", -500), refused(), "a negative spend");
    spend(&mut c, "tokens", 100).unwrap();
    assert!(!holds(&mut c, "eval.Broke-Seal", vec![t("r1")]), "at the cap is within it");
    let record = |c: &mut Core, who: &str, x: i64| act(c, who, "std.budgets.Record-Spend", vec![t(who), t("r1"), t("usd_cents"), n(x)]);
    record(&mut c, "run", 51).unwrap();
    assert!(holds(&mut c, "std.budgets.Over-Budget", vec![t("r1"), t("usd_cents")]));
    assert!(holds(&mut c, "eval.Broke-Seal", vec![t("r1")]), "a measured cost past a cap breaks the seal");
    assert_eq!(record(&mut c, "agent", 1), refused(), "the run's agent writes nothing in its ledger");
    assert_eq!(record(&mut c, "run", -51), refused(), "and nobody takes a cost back out");
    assert!(holds(&mut c, "std.budgets.Over-Budget", vec![t("r1"), t("usd_cents")]));
}

#[test]
fn each_task_gets_k_trials_counted_once_and_no_run_twice() {
    let mut c = suite(&["a", "b"], 2);
    assert_eq!(start(&mut c, "c1", "a", 0, "x0", true), refused(), "trials count from 1");
    assert_eq!(start(&mut c, "c1", "a", 3, "x3", true), refused(), "two trials, not three");
    start(&mut c, "c1", "a", 1, "a1", true).unwrap();
    assert_eq!(start(&mut c, "c1", "a", 1, "a1b", true), refused(), "one run for a candidate, task and trial");
    // the easiest task run again and again scores nothing: every task's every trial is wanted
    result(&mut c, "a1", true, 1, 1, 1).unwrap();
    start(&mut c, "c1", "a", 2, "a2", true).unwrap();
    result(&mut c, "a2", false, 1, 1, 1).unwrap();
    assert!(!holds(&mut c, "eval.Score", vec![t("s"), t("c1"), n(1), n(2), n(2), n(2)]) && holds(&mut c, "eval.Graded", vec![t("s"), t("c1"), n(1)]), "a retry is the next trial, and b has none");
    start(&mut c, "c1", "b", 1, "b1", true).unwrap();
    result(&mut c, "b1", false, 1, 1, 1).unwrap();
    assert!(holds(&mut c, "eval.Graded", vec![t("s"), t("c1"), n(1)]), "b half done");
    start(&mut c, "c1", "b", 2, "b2", true).unwrap();
    result(&mut c, "b2", false, 1, 1, 1).unwrap();
    // a solved once in two (pass@2), b never; the cost is every trial's
    assert!(holds(&mut c, "eval.Score", vec![t("s"), t("c1"), n(1), n(4), n(4), n(4)]));
}

#[test]
fn the_archive_keeps_the_candidates_no_other_beats_on_passes_cents_tokens_and_time() {
    let mut c = suite(&["a", "b"], 1);
    // c2 beats c1; c3 is cheaper than c2 but passes less; c4 equals c2; c5 lacks a result; c6 is c2 with more tokens
    let runs = [("c1", [true, false], 20, 100, 500), ("c2", [true, true], 10, 100, 400), ("c3", [true, false], 4, 100, 400), ("c4", [true, true], 10, 100, 400), ("c5", [true, true], 1, 1, 1), ("c6", [true, true], 10, 200, 400)];
    for (cand, passes, cents, tokens, ms) in runs {
        for (j, task) in ["a", "b"].iter().enumerate() {
            let run = format!("{cand}-{task}");
            start(&mut c, cand, task, 1, &run, true).unwrap();
            if cand != "c5" || j == 0 {
                result(&mut c, &run, passes[j], cents / 2, tokens / 2, ms / 2).unwrap();
            }
        }
    }
    assert!(holds(&mut c, "eval.Score", vec![t("s"), t("c2"), n(2), n(10), n(100), n(400)]));
    let front: Vec<&str> = ["c1", "c2", "c3", "c4", "c5", "c6"].into_iter().filter(|x| holds(&mut c, "eval.On-Front", vec![t("s"), t(x)])).collect();
    assert_eq!(front, ["c2", "c3", "c4"], "c6 is beaten on tokens alone");

    let propose = |c: &mut Core, who: &str, parent: &str, child: &str| act(c, who, "eval.Propose", vec![t(who), t("s"), t(parent), t(child), t("say what was missed")]);
    assert_eq!(propose(&mut c, "agent", "c2", "c7"), refused(), "only a proposer proposes");
    assert_eq!(propose(&mut c, "prop", "c1", "c7"), refused(), "a dominated parent");
    assert_eq!(propose(&mut c, "prop", "c2", "c3"), refused(), "a child that is a candidate already");
    propose(&mut c, "prop", "c2", "c7").unwrap();
    assert!(holds(&mut c, "eval.Child-Of", vec![t("c7"), t("c2"), t("say what was missed")]));
}
