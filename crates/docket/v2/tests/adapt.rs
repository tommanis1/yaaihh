//! std.adapt and its packs on the normative core: a model's profile alone selects its program and its effort, and
//! the project's start of a session sends that effort; a project's `Extend` stands over the packs; a review goes to
//! a seated party of a paired family; a brief is shown once or every turn, by the hooks themselves.

use docket_v2::check::load;
use docket_v2::check::names::Sym;
use docket_v2::norm::Core;
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::machine::Machine;
use docket_v2::run::norms::{Norms, Why};
use docket_v2::run::value::{Instance, Value};
use docket_v2::syntax::read;
use std::cell::RefCell;
use std::rc::Rc;

/// Three agents on three models, ann's model left to each test and ann seated; tom gives out the work.
const WORLD: &str = r#"
module t
import std.adapt as A
import std.budgets as B
import std.rails as R
import std.comms as C
import std.programs.adversarial_reviewer as V
import std.hooks as H
+H.Party("ann"). +C.Role("tom", "owner").
+A.Task-Class("feature", "scoped"). +A.Task-Class("design", "deep"). +A.Task-Class("review", "review").
+A.Task-Class("proof", "deep"). +A.Task-Class("lint", "scoped").
+B.On-Model("ann", "m1"). +B.On-Model("rev", "m2"). +B.On-Model("gem", "m3").
+A.Model-Profile("m2", "workhorse", "long"). +A.Model-Profile("m3", "fast-small", "short").
+R.Of-Family("ann", "anthropic"). +R.Of-Family("gem", "google").
+A.Cross-Family-Pair("anthropic", "openai"). +A.Cross-Family-Pair("openai", "google").
+A.Takes-Effort("m1", "low"). +A.Takes-Effort("m1", "medium"). +A.Takes-Effort("m1", "high").
+R.Assigned("ann", "w1"). +A.Of-Kind("w1", "feature").
+R.Assigned("ann", "w2"). +A.Of-Kind("w2", "design").
+R.Assigned("ann", "w3"). +A.Of-Kind("w3", "proof").
+R.Assigned("ann", "w4"). +A.Of-Kind("w4", "lint").
+B.On-Model("bo", "m1"). +R.Assigned("bo", "w5"). +A.Of-Kind("w5", "feature").
+B.On-Model("cy", "m1:xhigh"). +R.Assigned("cy", "w6"). +A.Of-Kind("w6", "feature").
+A.Of-Kind("r1", "review"). +V.Review-Of("r1", "w1").
"#;

const FRONTIER: &str = r#"+A.Model-Profile("m1", "frontier", "long")."#;
const WORKHORSE: &str = r#"+A.Model-Profile("m1", "workhorse", "short")."#;
const REV_OPENAI: &str = r#"+R.Of-Family("rev", "openai")."#;

fn text(path: &str) -> String {
    let p = format!("{}/../../../docket/{path}.docket", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

/// The texts of PATHS, then the world with LINES added, checked whole (14).
fn loaded(paths: &[String], lines: &[&str]) -> docket_v2::check::names::Loaded {
    let mut ms: Vec<_> = paths.iter().map(|p| read(&text(p)).unwrap_or_else(|e| panic!("{p}: {e}"))).collect();
    ms.push(read(&format!("{WORLD}{}\n", lines.join("\n"))).unwrap());
    load(ms).unwrap_or_else(|e| panic!("{e}"))
}

/// std.adapt, the three packs and what they import, and Tovrin's choices when TOVRIN.
fn texts(tovrin: bool) -> Vec<String> {
    let mut paths: Vec<String> = ["hooks", "delivery", "comms", "budgets", "rails", "control", "adapt"].map(|p| format!("std/{p}")).into();
    paths.extend(["lean-frontier", "guided-worker", "adversarial-reviewer"].map(|p| format!("std/programs/{p}")));
    if tovrin {
        paths.push("examples/tovrin/proofs".into());
    }
    paths
}

fn core(lines: &[&str], tovrin: bool) -> Core {
    let mut core = Core::default();
    core.load(&loaded(&texts(tovrin), lines).norms).unwrap();
    core
}

fn a(ty: &str, args: &[&str]) -> Instance {
    Instance { ty: format!("std.adapt.{ty}"), args: args.iter().map(|s| Value::text(s)).collect() }
}

fn act(core: &mut Core, ty: &str, args: &[&str], by: &str) -> Result<(), Why> {
    let i = Instance { ty: ty.into(), args: args.iter().map(|s| Value::text(s)).collect() };
    let s = core.step(&i, Some(&Value::text(by)))?;
    core.commit(s);
    Ok(())
}

/// The one template among the three packs' that is selected for ANN's KIND, or none.
fn selected(core: &mut Core, kind: &str) -> Option<&'static str> {
    let all = ["lean-frontier", "guided-worker", "adversarial-reviewer", "proof-checker"];
    let got: Vec<_> = all.into_iter().filter(|t| core.holds(&a("Selected-Program", &["ann", kind, t]))).collect();
    assert!(got.len() <= 1, "{kind}: {got:?}");
    got.first().copied()
}

fn effort(core: &mut Core, agent: &str, kind: &str) -> Option<&'static str> {
    let got: Vec<_> = ["low", "medium", "high", "xhigh", "max"].into_iter().filter(|e| core.holds(&a("Effort-For", &[agent, kind, e]))).collect();
    assert!(got.len() <= 1, "{agent} {kind}: {got:?}");
    got.first().copied()
}

fn asks_own_check(core: &mut Core, target: &str) -> bool {
    core.holds(&Instance { ty: "std.rails.Required".into(), args: vec![Value::text(target), Value::text("does_what_was_asked")] })
}

/// A driver carrying every Effect the text leaves to one (14, check 4), seen only by the sessions it starts.
struct Sessions(Vec<String>, Rc<RefCell<Vec<String>>>);

impl Driver for Sessions {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("sessions"), Value::text("1")]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        self.0.iter().map(|e| (e.clone(), Vec::new())).collect()
    }
    fn events(&self) -> Vec<String> {
        Vec::new()
    }
    fn carry(&mut self, op: &str, args: &[Value], _: u64) -> Reply {
        assert_eq!(op, "std.hooks.Start-Session", "nothing else is performed");
        self.1.borrow_mut().push(args[4].to_string());
        Reply::Answer { value: Value::Unit, costs: Vec::new(), events: Vec::new() }
    }
    fn dropped(&mut self, _: u64) {}
}

/// The model texts yaaihh.project's Start-Agent, run by tom, starts AGENT's session on, in the world with LINES.
fn started_on(agent: &str, lines: &[&str]) -> Vec<String> {
    let mut paths = texts(false);
    paths.extend(["bridges/claude-code", "bridges/codex", "bridges/opencode", "bridges/agy"].map(|p| format!("std/{p}")));
    paths.extend(["audit", "train", "hygiene", "brain", "desk", "project"].map(|p| format!("defaults/{p}")));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let l = loaded(&paths, lines);
    let carried = l.syms.iter().filter(|(_, s)| matches!(s, Sym::Effect { does: None, .. })).map(|(n, _)| n.clone()).collect();
    let mut m = Machine::new(l, Core::default(), vec![Box::new(Sessions(carried, seen.clone()))]).unwrap();
    let start = Instance { ty: "yaaihh.project.Start-Agent".into(), args: vec![Value::text("tom"), Value::text(agent)] };
    m.start(Value::text("tom"), None, "", start).unwrap();
    m.run();
    seen.take()
}

/// (1) One line differs between the two worlds, m1's profile: the program, its scaffolding, its effort, the model
/// text the project's start sends, and the end check all follow it.
#[test]
fn a_profile_alone_switches_program_and_effort_and_the_session_gets_it() {
    let mut top = core(&[FRONTIER], false);
    assert_eq!(selected(&mut top, "feature"), Some("lean-frontier"));
    assert!(top.holds(&a("Program-Template", &["lean-frontier", "direct", "minimal", "lean", "medium"])));
    assert_eq!(effort(&mut top, "bo", "feature"), Some("medium"), "scoped work at the bottom of the U-curve");
    assert!(asks_own_check(&mut top, "w1"), "lean work owes that it does what was asked");
    assert_eq!(started_on("bo", &[FRONTIER]), ["\"m1:medium\""]);

    let mut mid = core(&[WORKHORSE], false);
    assert_eq!(selected(&mut mid, "feature"), Some("guided-worker"));
    assert!(mid.holds(&a("Program-Template", &["guided-worker", "hinted", "stepwise", "guided", "high"])));
    assert_eq!(effort(&mut mid, "bo", "feature"), Some("high"));
    assert!(!asks_own_check(&mut mid, "w1"));
    assert_eq!(started_on("bo", &[WORKHORSE]), ["\"m1:high\""]);

    let short = r#"+A.Model-Profile("m1", "frontier", "short")."#;
    assert_eq!(selected(&mut core(&[short], false), "feature"), Some("guided-worker"), "lean needs a long context");
    assert_eq!(selected(&mut core(&[], false), "feature"), None, "an unprofiled model has no program");
    assert_eq!(started_on("bo", &[]), ["\"m1\""], "and its session is started as before");
}

/// (2) The U-curve: deep work is raised, and work refuted below the top goes one step up for each refutation, to
/// what its model takes; a session is started at the highest effort its work runs at, each ask brought down to it.
#[test]
fn deep_and_refuted_work_raise_effort_within_what_the_model_takes() {
    let mut c = core(&[FRONTIER], false);
    assert_eq!(effort(&mut c, "ann", "design"), Some("high"));
    assert!(c.holds(&a("Session-Effort", &["ann", "high"])));
    assert!(c.holds(&a("At-Top", &["ann", "w2"])) && !c.holds(&a("At-Top", &["ann", "w1"])), "m1 takes nothing above high");
    let raised = r#"+A.Raised("ann", "w1", "medium")."#;
    let mut c = core(&[FRONTIER, raised], false);
    assert_eq!(effort(&mut c, "ann", "feature"), Some("high"), "refuted once at medium");
    assert!(c.holds(&a("At-Top", &["ann", "w1"])), "a second refutation finds it at the top");
    let takes = r#"+A.Takes-Effort("m1", "max")."#;
    let twice = r#"+A.Raised("ann", "w2", "high")."#;
    let mut c = core(&[FRONTIER, takes, twice], false);
    assert_eq!(effort(&mut c, "ann", "design"), Some("max"), "the next step m1 takes, past the xhigh it does not");
    assert!(c.holds(&a("Session-Effort", &["ann", "max"])) && c.holds(&a("At-Top", &["ann", "w2"])));
}

/// (2b) One piece whose ask is above all its model takes is run at the highest the model does take, never at none.
#[test]
fn a_session_on_one_piece_is_clamped_to_its_model() {
    let dee = r#"+H.Party("dee"). +B.On-Model("dee", "m4"). +A.Model-Profile("m4", "frontier", "long").
+A.Takes-Effort("m4", "low"). +A.Takes-Effort("m4", "medium"). +R.Assigned("dee", "w7"). +A.Of-Kind("w7", "design")."#;
    let mut c = core(&[FRONTIER, dee], false);
    assert_eq!(effort(&mut c, "dee", "design"), Some("high"), "deep work asks high");
    assert!(c.holds(&a("Session-Effort", &["dee", "medium"])) && c.holds(&a("At-Top", &["dee", "w7"])));

    let fay = r#"+H.Party("fay"). +B.On-Model("fay", "m1"). +R.Assigned("fay", "w10"). +A.Of-Kind("w10", "design").
+A.Chosen-Effort("fay", "design", "xhigh")."#;
    let mut c = core(&[FRONTIER, fay], false);
    assert_eq!(effort(&mut c, "fay", "design"), Some("xhigh"), "the project asks xhigh of m1, which takes up to high");
    assert!(c.holds(&a("Session-Effort", &["fay", "high"])) && c.holds(&a("At-Top", &["fay", "w10"])));

    let ed = r#"+H.Party("ed"). +B.On-Model("ed", "m3"). +A.Takes-Effort("m3", "low"). +A.Takes-Effort("m3", "medium").
+R.Assigned("ed", "w8"). +A.Of-Kind("w8", "feature")."#;
    let mut c = core(&[FRONTIER, ed], false);
    assert!(c.holds(&a("Selected-Program", &["ed", "feature", "guided-worker"])) && c.holds(&a("Effort-For", &["ed", "feature", "high"])));
    assert!(c.holds(&a("Session-Effort", &["ed", "medium"])), "guided-worker's high, on a model that takes up to medium");
    assert_eq!(started_on("ed", &[FRONTIER, ed]), ["\"m3:medium\""]);
}

/// (3) Tovrin's `Extend` over the packs: a proof under its proof checker at high, lint on the guided worker at low,
/// on the same frontier model that the packs alone give the lean program.
#[test]
fn a_projects_extend_stands_over_the_packs() {
    let mut packs = core(&[FRONTIER], false);
    assert_eq!(selected(&mut packs, "proof"), Some("lean-frontier"));
    assert_eq!(selected(&mut packs, "lint"), Some("lean-frontier"));
    assert_eq!(effort(&mut packs, "ann", "lint"), Some("medium"));

    let mut tovrin = core(&[FRONTIER], true);
    assert_eq!(selected(&mut tovrin, "proof"), Some("proof-checker"));
    assert_eq!(effort(&mut tovrin, "ann", "proof"), Some("high"));
    assert_eq!(selected(&mut tovrin, "lint"), Some("guided-worker"));
    assert_eq!(effort(&mut tovrin, "ann", "lint"), Some("low"));
    assert_eq!(selected(&mut tovrin, "feature"), Some("lean-frontier"), "a kind Tovrin does not choose for");
}

/// (4) A review of ann's work never goes to ann; it goes to the paired family while a party of it is seated, to any
/// other seated family when none is, and runs under the adversarial reviewer. The family is the one std.rails holds.
#[test]
fn a_review_goes_to_a_seated_party_of_a_paired_family() {
    let assign = "std.rails.Assign";
    let refused = || Err(Why::Policy("not enabled".into()));
    let mut c = core(&[FRONTIER, REV_OPENAI], false);
    assert!(c.holds(&a("Reviewer-Family", &["ann", "openai"])) && !c.holds(&a("Reviewer-Family", &["ann", "google"])));
    assert_eq!(act(&mut c, assign, &["tom", "gem", "r1"], "tom"), refused(), "openai is paired and seated");
    assert_eq!(act(&mut c, assign, &["tom", "ann", "r1"], "tom"), refused(), "the author");
    act(&mut c, assign, &["tom", "rev", "r1"], "tom").unwrap();
    let as_rev = |t| a("Selected-Program", &["rev", "review", t]);
    assert!(c.holds(&as_rev("adversarial-reviewer")) && !c.holds(&as_rev("guided-worker")));
    assert_eq!(effort(&mut c, "rev", "review"), Some("high"));

    // rev runs m2, profiled, but no family was stated for it: openai has nobody seated
    let mut c = core(&[FRONTIER], false);
    assert!(c.holds(&a("Reviewer-Family", &["ann", "google"])));
    act(&mut c, assign, &["tom", "gem", "r1"], "tom").unwrap();
}

/// (5) The lean brief is shown once and again after a compaction; the guided one at every turn.
#[test]
fn a_brief_is_shown_once_or_every_turn() {
    let mut c = core(&[FRONTIER], false);
    let owed = a("Brief-Owed", &["ann", "lean-frontier"]);
    assert!(c.holds(&owed));
    act(&mut c, "std.adapt.Take-Brief", &["ann", "lean-frontier"], "ann").unwrap();
    assert!(!c.holds(&owed));
    act(&mut c, "std.hooks.Reach", &["ann", "compacting"], "ann").unwrap();
    assert!(c.holds(&owed), "a compaction drops it");

    let mut c = core(&[WORKHORSE], false);
    act(&mut c, "std.adapt.Take-Brief", &["ann", "guided-worker"], "ann").unwrap();
    assert!(c.holds(&a("Brief-Owed", &["ann", "guided-worker"])));
}

/// (6) A model text that names its effort keeps it: unprofiled it has no program, profiled it has one, and either way
/// its session starts on the text as it was, never with a second effort.
#[test]
fn a_model_that_names_its_effort_keeps_it() {
    let cy = |t| a("Selected-Program", &["cy", "feature", t]);
    let mut c = core(&[FRONTIER], false);
    assert!(!c.holds(&cy("lean-frontier")) && !c.holds(&cy("guided-worker")));
    assert_eq!(started_on("cy", &[FRONTIER]), ["\"m1:xhigh\""]);

    let own = r#"+A.Model-Profile("m1:xhigh", "frontier", "long"). +A.Takes-Effort("m1:xhigh", "medium")."#;
    let mut c = core(&[FRONTIER, own], false);
    assert!(c.holds(&cy("lean-frontier")));
    assert!(c.holds(&a("Effort-For", &["cy", "feature", "medium"])) && !c.holds(&a("Session-Effort", &["cy", "medium"])));
    assert_eq!(started_on("cy", &[FRONTIER, own]), ["\"m1:xhigh\""]);
}


/// A harness session for dan, on claude-code 2.1.332: it reaches the moments it is given, in turn, and keeps each
/// answer applied; a tool call changes the files it is given.
struct Harness(Vec<String>, Vec<(&'static str, Vec<&'static str>)>, Rc<RefCell<Vec<(String, Value)>>>);

impl Driver for Harness {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("claude-code"), Value::text("2.1.332")]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        self.0.iter().map(|e| (e.clone(), Vec::new())).collect()
    }
    fn events(&self) -> Vec<String> {
        Vec::new()
    }
    fn carry(&mut self, op: &str, args: &[Value], _: u64) -> Reply {
        let value = match op {
            "std.hooks.Next-Moment" if self.1.is_empty() => return Reply::Later,
            "std.hooks.Next-Moment" => {
                let (moment, files) = self.1.remove(0);
                Value::Record { ty: "std.hooks.Reached".into(), fields: vec![("moment".into(), Value::text(moment)), ("payload".into(), Value::text(&files.join(" ")))] }
            }
            "std.hooks.Tool-Use" => {
                let change = |p: &str| Value::Record {
                    ty: "std.hooks.FileChange".into(),
                    fields: ["kind", "path", "from", "patch", "added"].iter().map(|f| (f.to_string(), Value::text(match *f { "kind" => "edit", "path" => p, _ => "" }))).collect(),
                };
                let Value::Text(files) = &args[1] else { panic!("{args:?}") };
                let changes = files.split_whitespace().map(change).collect();
                Value::Record { ty: "std.hooks.ToolUse".into(), fields: vec![("tool".into(), Value::text("edit")), ("command".into(), Value::text("")), ("changes".into(), Value::List(changes))] }
            }
            "std.hooks.Apply" => {
                self.2.borrow_mut().push((args[1].to_string(), args[2].clone()));
                Value::Unit
            }
            _ => panic!("{op} is not a harness's"),
        };
        Reply::Answer { value, costs: Vec::new(), events: Vec::new() }
    }
    fn dropped(&mut self, _: u64) {}
}

const DAN: &str = r#"+H.Party("dan"). +B.On-Model("dan", "m1"). +R.Assigned("dan", "w9"). +A.Of-Kind("w9", "feature").
+H.Running-On("dan", "claude-code", "2.1.332").
Act Serve-Here(agent: H.Party) Does { do H.serve(agent) }"#;

/// What dan's harness was answered at each of MOMENTS, served under std.adapt's arms in the world with LINES.
fn answers(lines: &[&str], moments: Vec<(&'static str, Vec<&'static str>)>) -> Vec<(String, String)> {
    let mut paths = texts(false);
    paths.push("std/bridges/claude-code".into());
    let l = loaded(&paths, &[lines, &[DAN][..]].concat());
    let carried = l.syms.iter().filter(|(_, s)| matches!(s, Sym::Effect { does: None, .. })).map(|(n, _)| n.clone()).collect();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut m = Machine::new(l, Core::default(), vec![Box::new(Harness(carried, moments, seen.clone()))]).unwrap();
    m.start(Value::text("dan"), Some(0), "", Instance { ty: "t.Serve-Here".into(), args: vec![Value::text("dan")] }).unwrap();
    m.run();
    let said = |v: &Value| match v {
        Value::Ctor { name, args } if name == "std.hooks.Inject" => format!("Inject {}", args[0].to_string().split_whitespace().take(3).collect::<Vec<_>>().join(" ")),
        Value::Ctor { name, .. } => name.trim_start_matches("std.hooks.").to_string(),
        v => panic!("{v:?}"),
    };
    seen.take().iter().map(|(m, v)| (m.trim_matches('"').to_string(), said(v))).collect()
}

/// (7) Through the machine: the lean brief at the first turn and again after a compaction, the guided one at every
/// turn, and the guided hint after a call that changes a file, not after one that changes none.
#[test]
fn the_hooks_show_the_brief_and_the_hint_as_the_pack_says() {
    let moment = |m: &'static str| (m, Vec::new());
    let got = answers(&[FRONTIER], vec![moment("turn_begins"), moment("turn_begins"), moment("compacting"), moment("turn_begins")]);
    let lean = "Inject \"Your task, its";
    assert_eq!(got.iter().map(|(_, a)| a.as_str()).collect::<Vec<_>>(), [lean, "Allow", "Allow", lean]);

    let got = answers(&[WORKHORSE], vec![moment("turn_begins"), moment("turn_begins"), ("tool_called", vec!["src/a.rs"]), moment("tool_called")]);
    let guided = "Inject \"Work in steps.";
    assert_eq!(got.iter().map(|(_, a)| a.as_str()).collect::<Vec<_>>(), [guided, guided, "Inject \"You changed a", "Allow"]);
}
