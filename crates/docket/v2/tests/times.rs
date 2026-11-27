//! The normative core's times early and late in a long record, run by hand:
//! `cargo test --profile quick -p docket-v2 --test times -- --ignored --nocapture`.

use docket_v2::check::{Loaded, load};
use docket_v2::norm::Core;
use docket_v2::run::norms::Norms;
use docket_v2::run::value::{Env, Instance, Value};
use std::time::{Duration, Instant};

/// Paths changed and noted, the glob condition of a reaction, and an act that ends every note of its party.
const PATHS: &str = r#"
module t
Fact Party Identified by String
Fact Tree(glob: String)
Fact Changes(agent: Party, path: String)
Fact Noted(agent: Party, kind: String, path: String)
Act Touch(by: Party, path: String)
  Creates Changes(by, path)
Act Note(by: Party, kind: String, path: String)
  Creates Noted(by, kind, path)
Act Forget(by: Party)
  Terminates Noted(by, kind, path) When Noted(by, kind, path)
on Touch(by, path) where Exists g, p : Tree(g) && Changes(by, p) && matches_glob(p, g) do { return 1 }
+Party("w1"). +Party("w2"). +Tree("crates/engine/**"). +Tree("crates/core/**"). +Tree("agent-base/**").
"#;

fn core_of(text: &str) -> (Loaded, Core) {
    let l = load(vec![docket_v2::syntax::read(text).unwrap()]).unwrap();
    let mut core = Core::default();
    core.load(&l.norms).unwrap();
    (l, core)
}

fn t(s: &str) -> Value {
    Value::text(s)
}

fn at(ty: &str, args: Vec<Value>) -> Instance {
    Instance { ty: ty.into(), args }
}

fn perform(core: &mut Core, i: &Instance, by: &str) {
    let s = core.step(i, Some(&t(by))).unwrap();
    core.commit(s);
}

fn timed(f: impl FnOnce()) -> Duration {
    let s = Instant::now();
    f();
    s.elapsed()
}

/// Nine tries of F, each giving the time of what it measures: the median and the largest.
fn times(f: impl FnMut(usize) -> Duration) -> String {
    let mut ds: Vec<Duration> = (0..9).map(f).collect();
    ds.sort();
    format!("median {:?}, largest {:?}", ds[4], ds[8])
}

#[test]
#[ignore = "a probe of times, run by hand"]
fn the_cores_times_early_and_late_in_a_long_record() {
    let library = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../docket/gate/library-a.docket");
    let (l, mut core) = core_of(&std::fs::read_to_string(library).unwrap());
    let a = |ty: &str, args: Vec<Value>| at(&format!("gate.a.{ty}"), args);
    let submit = |k: usize| a("Submit", vec![t("w1"), t(&format!("c{k}")), t("w2")]);
    let cond = l.ons.iter().find(|o| o.target.ends_with("Report")).and_then(|o| o.cond.clone()).unwrap();
    let env: Env = [("by".to_string(), t("w1")), ("change".to_string(), t("c0"))].into_iter().collect();
    let mut n = 0;
    for open in [500, 2000] {
        for k in n..open {
            perform(&mut core, &submit(k), "w1");
        }
        n = open + 9;
        let line = |what: &str, took: String| println!("{what}, {open} changes open: {took}");
        line("a Submit, step and commit", times(|k| timed(|| perform(&mut core, &submit(open + k), "w1"))));
        let withdraw = a("Withdraw", vec![t("w1"), t("c0")]);
        let refused = |core: &mut Core| assert!(core.step(&withdraw, Some(&t("w1"))).is_err());
        line("a Withdraw refused by the closing rule", times(|_| timed(|| refused(&mut core))));
        let green = a("Green", vec![t("c0")]);
        line("holds Green", times(|_| timed(|| _ = core.holds(&green))));
        line("a reaction's where", times(|_| timed(|| _ = core.cond(&cond, &env).unwrap())));
    }

    let (l, mut core) = core_of(PATHS);
    let cond = l.ons[0].cond.clone().unwrap();
    let env: Env = [("by".to_string(), t("w1")), ("path".to_string(), t("x"))].into_iter().collect();
    let mut n = 0;
    for standing in [10, 100, 1000] {
        for k in n..standing {
            perform(&mut core, &at("t.Touch", vec![t("w1"), t(&format!("docs/f{k}.md"))]), "w1");
        }
        n = standing;
        let glob = times(|_| timed(|| assert!(!core.cond(&cond, &env).unwrap())));
        println!("the glob where, false, {standing} paths: {glob}");
        let forget = times(|_| {
            for k in 0..3 {
                perform(&mut core, &at("t.Note", vec![t("w2"), t("edit"), t(&format!("w2/f{k}"))]), "w2");
            }
            timed(|| perform(&mut core, &at("t.Forget", vec![t("w2")]), "w2"))
        });
        println!("a Forget of 3 notes, two fields open, {standing} paths: {forget}");
    }
    assert!(!core.holds(&at("t.Noted", vec![t("w2"), t("edit"), t("w2/f0")])));
    assert!(core.holds(&at("t.Changes", vec![t("w1"), t("docs/f0.md")])));
}
