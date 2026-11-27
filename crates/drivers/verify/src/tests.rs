//! std.verify run with this driver on a stand-in screen: Attest only with a sealed pass, and each way short of one refused.

use crate::{Builder, Config, Owner, Screen, Verify};
use docket_v2::norm::Core;
use docket_v2::run::machine::{Machine, State};
use docket_v2::run::norms::Norms;
use docket_v2::run::record::Line;
use docket_v2::run::value::{Instance, Value};
use std::path::{Path, PathBuf};
use yaaihh_e2e::{digest, digest_text, Look, Shown, Step, Target};

/// The bucket bob's runs are charged to.
const BUCKET: &str = "verify-head";

/// Each answer of the stand-in model costs this many tokens.
const TOKENS: i64 = 1000;

fn v(ty: &str, args: &[&str]) -> Instance {
    Instance { ty: format!("std.verify.{ty}"), args: args.iter().map(|a| Value::text(a)).collect() }
}

/// A window of two pages: Overview, and Settings once its row is pressed; each look's files written as the runner's are.
struct Stand {
    folder: PathBuf,
    settings: bool,
    n: usize,
}

impl Screen for Stand {
    fn step(&mut self, step: &Step, said: &str) -> Result<Look, String> {
        match step {
            Step::Tap(Target::Text(t)) if t == "Settings" || t == "This project's settings" => self.settings = true,
            Step::Tap(Target::Text(t)) if t == "Overview" => self.settings = false,
            Step::Tap(t) => return Err(format!("the window shows no {t:?} to press")),
            _ => {}
        }
        self.n += 1;
        let texts: &[&str] = if self.settings { &["Overview", "Settings", "Who may do what"] } else { &["Overview", "Settings", "No agent yet: New agent starts one", "This project's settings"] };
        let file = |ext: &str, body: &str| -> PathBuf {
            let p = self.folder.join(format!("{:02}.{ext}", self.n));
            std::fs::write(&p, body).unwrap();
            p
        };
        let shown = texts.iter().map(|t| Shown { text: t.to_string(), key: None, center: (1.0, 1.0) }).collect();
        Ok(Look { step: said.into(), png: file("png", &format!("picture {}", self.n)), tree: file("json", "{}"), log: file("log", ""), shown })
    }
    fn server_log(&self) -> String {
        "coordd: e2e on its socket".into()
    }
}

/// A folder of its own under the temp folder, with a one-commit repository and the planner's answers in it: before
/// each plan, the path it looks along.
fn place(name: &str, plans: &[&str]) -> PathBuf {
    let d = std::env::temp_dir().join(format!("verify-driver-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("repo")).unwrap();
    std::fs::write(d.join("repo/home.txt"), "the home lists projects\n").unwrap();
    let git = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(d.join("repo")).args(["-c", "user.name=t", "-c", "user.email=t@t"]).args(args).output().unwrap().status.success());
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&["commit", "-qm", "list projects on the home"]);
    for (n, p) in plans.iter().enumerate() {
        let said = |a: String| serde_json::json!({ "answer": a, "tokens": TOKENS }).to_string();
        std::fs::write(d.join(format!("answer{}.txt", 2 * n)), said("I would look at the settings:\n```\ntap Settings\n```\n".into())).unwrap();
        std::fs::write(d.join(format!("answer{}.txt", 2 * n + 1)), said(format!("Here is the plan:\n```\n{p}\n```\n"))).unwrap();
    }
    std::fs::write(d.join("window"), "the built window").unwrap();
    d
}

/// bob's run of verification of HEAD's intent, the model's Nth plan being PLANS[N], a run begun again after each
/// refused Seal, under a bucket of MOST tokens: the machine and the last run.
fn verified(name: &str, plans: &[&str]) -> (Machine<Core>, docket_v2::run::machine::RunId, PathBuf) {
    budgeted(name, plans, 100 * TOKENS)
}

fn budgeted(name: &str, plans: &[&str], most: i64) -> (Machine<Core>, docket_v2::run::machine::RunId, PathBuf) {
    let d = place(name, plans);
    let open_in = d.clone();
    let refused = crate::Refused::default();
    let cfg = Config {
        repo: d.join("repo"),
        planner: vec!["sh".into(), "-c".into(), format!("n=$(ls {0} | grep -c '^asked'); cat >{0}/asked$n.txt; cat {0}/answer$n.txt", d.display())],
        window: d.join("window"),
        evidence: d.join("evidence"),
        open: Box::new(move |folder: &Path| {
            let looks = folder.join("looks");
            std::fs::create_dir_all(&looks).unwrap();
            assert!(folder.starts_with(&open_in));
            Ok(Box::new(Stand { folder: looks, settings: false, n: 0 }) as Box<dyn Screen>)
        }),
        refused: refused.clone(),
    };
    let loaded = crate::load(&crate::limits(BUCKET, &[("tokens".into(), most)])).unwrap_or_else(|e| panic!("{e}"));
    let owner = Owner { why: "the test's bucket is spent".into() };
    let mut m = Machine::new(loaded, Core::checked(), vec![Box::new(Verify::new(cfg)), Box::new(Builder), Box::new(owner)]).unwrap();
    let claim = m.start(Value::text("ann"), Some(0), "", v("Claim", &["ann", "HEAD", "settings load", "bob"])).unwrap();
    m.run();
    assert_eq!(m.state(claim), State::Done, "{:?}", m.record);
    m.event(1, v("Built-From", &["HEAD", "sha256:the-built-window"]));
    let run = crate::verify(&mut m, &refused, "runner", "bob", "HEAD", "settings load", BUCKET).unwrap();
    (m, run, d)
}

const PASSES: &str = r#"{ seen <- perform Interact("HEAD", "tap Settings"); ok <- perform Expect("HEAD", seen, "Who may do what\n!Reading…"); if ok { return Delivered } else { return Falsified("settings never loaded") } }"#;

#[test]
fn a_run_that_passed_is_sealed_and_attested_and_the_duty_ends() {
    let (mut m, run, d) = verified("pass", &[PASSES]);
    assert_eq!(m.state(run), State::Done, "{:?}", m.record);
    let ok = Value::Ctor { name: "std.budgets.Ok".into(), args: vec![Value::Bool(true)] };
    assert!(m.record.iter().any(|l| matches!(l, Line::Ended { value, .. } if *value == ok)), "{:?}", m.record);
    assert!(!m.norms.holds(&v("Verify-Intent", &["bob", "ann", "HEAD", "settings load"])));
    let asked = std::fs::read_to_string(d.join("asked1.txt")).unwrap();
    assert!(asked.contains("settings load") && asked.contains("list projects on the home"), "the model is asked with the intent and the diff: {asked}");
    assert!(asked.contains("one text a line:\nOverview\nSettings\nNo agent yet"), "and with what the window shows at its start: {asked}");
    assert!(asked.contains("After tap Settings:\nOverview\nSettings\nWho may do what"), "and with what it showed along the path asked for: {asked}");
    let first = std::fs::read_to_string(d.join("asked0.txt")).unwrap();
    assert!(first.contains("a line for each path from the start") && first.contains("paid from the run's budget"), "first asked which paths to look along, with no count: {first}");
    assert!(asked.contains("no line that must be shown never holds"), "and told an expectation of absences alone never holds: {asked}");
    assert!(asked.contains("was not on the screen before the first step"), "and told a text shown at the start proves nothing: {asked}");
    let sealed: Vec<PathBuf> = std::fs::read_dir(d.join("evidence/sealed")).unwrap().map(|e| e.unwrap().path()).collect();
    let [manifest] = sealed.as_slice() else { panic!("one sealed run: {sealed:?}") };
    let hex = manifest.file_stem().unwrap().to_string_lossy().to_string();
    assert_eq!(digest(manifest).unwrap(), hex, "the evidence is named by its own digest");
    let plan_ref = format!("plan:{}", &digest_text(PASSES)[..16]);
    assert!(m.norms.holds(&Instance { ty: "std.verify.Attested".into(), args: vec![Value::text("HEAD"), Value::text("settings load"), Value::text(&plan_ref), Value::text(&format!("sha256:{hex}"))] }));
    let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(manifest).unwrap()).unwrap();
    assert_eq!(j["passed"], true);
    assert_eq!(j["window_sha256"], digest(&d.join("window")).unwrap());
    let pic = &j["looks"][0]["picture"];
    assert_eq!(pic["sha256"].as_str().unwrap(), digest(Path::new(pic["path"].as_str().unwrap())).unwrap());
    assert_eq!(j["expects"][0]["held"], true);
}

fn not_attested(name: &str, plan: &str) {
    let (mut m, run, _) = verified(name, &[plan]);
    assert_ne!(m.state(run), State::Done, "{:?}", m.record);
    assert!(m.norms.holds(&v("Verify-Intent", &["bob", "ann", "HEAD", "settings load"])), "the duty stays owed");
    assert!(!m.record.iter().any(|l| format!("{l:?}").contains("Attested")), "{:?}", m.record);
}

#[test]
fn a_delivered_with_nothing_expected_is_not_attested() {
    not_attested("noexpect", r#"{ seen <- perform Interact("HEAD", "tap Settings"); return Delivered }"#);
}

#[test]
fn a_delivered_after_a_failed_expectation_is_not_attested() {
    not_attested("ignored", r#"{ seen <- perform Interact("HEAD", "look"); ok <- perform Expect("HEAD", seen, "Who may do what"); return Delivered }"#);
}

#[test]
fn a_delivered_after_a_step_that_could_not_be_done_is_not_attested() {
    not_attested("nostep", r#"{ no <- perform Interact("HEAD", "tap Map"); seen <- perform Interact("HEAD", "look"); ok <- perform Expect("HEAD", seen, "Overview"); if ok { return Delivered } else { return Falsified("no map") } }"#);
}

#[test]
fn a_plan_is_the_fenced_block_and_braces_in_the_prose_around_it_are_left() {
    let answer = "The plan checks {the settings} page:\n```docket\n{ return Delivered }\n```\nIt uses {one} step.";
    assert_eq!(crate::plan::block(answer).as_deref(), Some("{ return Delivered }"));
    assert_eq!(crate::plan::block("  { return Delivered }\n").as_deref(), Some("{ return Delivered }"));
    assert_eq!(crate::plan::block("I would tap {Settings}."), None);
}

#[test]
fn paths_are_the_blocks_lines_split_on_semicolons_and_none_is_no_path() {
    let p = crate::plan::paths("Look:\n```\ntap Settings ; wait 5\ntap Overview\n```\n");
    assert_eq!(p, vec![vec!["tap Settings".to_string(), "wait 5".into()], vec!["tap Overview".to_string()]]);
    assert!(crate::plan::paths("```\nnone\n```").is_empty());
}

#[test]
fn a_falsified_run_is_sealed_reported_and_waits_for_the_fix() {
    let (mut m, run, _) = verified("falsified", &[r#"{ seen <- perform Interact("HEAD", "look"); ok <- perform Expect("HEAD", seen, "Who may do what"); if ok { return Delivered } else { return Falsified("settings never loaded") } }"#]);
    assert!(matches!(m.state(run), State::Resting(_)), "{:?}", m.record);
    assert!(m.norms.holds(&v("Resolve", &["ann", "bob", "HEAD", "settings never loaded"])));
}

/// The plan Sonnet 5.5 wrote for 3cf7d0ba7 (plan:85311f322acda8f1, N-1897.p17), its change named HEAD: every text it
/// requires is on the screen before its first step.
const SHOWN_BEFORE: &str = r#"{
  opened <- perform Interact("HEAD", "tap This project's settings");
  first <- perform Interact("HEAD", "wait 10");
  ok1 <- perform Expect("HEAD", first, "Settings\n!No agent yet: New agent starts one\n!Reading…\n!Reading...\n!connecting\n!Connecting");
  if ok1 {
    later <- perform Interact("HEAD", "wait 20");
    ok2 <- perform Expect("HEAD", later, "Settings\n!No agent yet: New agent starts one\n!Reading…\n!Reading...\n!connecting\n!Connecting");
    if ok2 {
      back <- perform Interact("HEAD", "tap Overview");
      home <- perform Interact("HEAD", "wait 2");
      ok3 <- perform Expect("HEAD", home, "No agent yet: New agent starts one\nThis project's settings");
      if ok3 {
        reopened <- perform Interact("HEAD", "tap Settings");
        again <- perform Interact("HEAD", "wait 5");
        ok4 <- perform Expect("HEAD", again, "Settings\n!No agent yet: New agent starts one\n!Reading…\n!Reading...\n!connecting\n!Connecting");
        if ok4 {
          return Delivered
        } else {
          return Falsified("Reopened from the sidebar, the Settings page did not open or still showed Reading… or connecting after 5 seconds")
        }
      } else {
        return Falsified("Could not get back from Settings to the project's Overview, so the window did not behave as a live project screen")
      }
    } else {
      return Falsified("30 seconds after opening, the Settings page showed Reading… or connecting, or was no longer shown")
    }
  } else {
    return Falsified("10 seconds after pressing This project's settings, the Settings page had not opened or still showed Reading… or connecting")
  }
}"#;

#[test]
fn a_delivered_whose_every_required_text_was_shown_before_its_first_step_is_not_attested() {
    not_attested("shownbefore", SHOWN_BEFORE);
}

#[test]
fn a_plan_refused_so_is_challenged_and_the_next_plan_is_asked_with_why() {
    let (mut m, run, d) = verified("challenged", &[SHOWN_BEFORE, PASSES]);
    assert_eq!(m.state(run), State::Done, "{:?}", m.record);
    assert!(!m.norms.holds(&v("Verify-Intent", &["bob", "ann", "HEAD", "settings load"])), "the second plan's run is attested");
    let second = std::fs::read_to_string(d.join("asked3.txt")).unwrap();
    assert!(second.contains("- every text it required (Settings, No agent yet: New agent starts one, This project's settings) was on the screen before its first step"), "{second}");
}

#[test]
fn rounds_of_refused_plans_end_when_the_bucket_is_spent_not_at_a_count() {
    let plans: Vec<String> = (1..=8).map(|n| SHOWN_BEFORE.replace("wait 10", &format!("wait {n}"))).collect();
    let plans: Vec<&str> = plans.iter().map(String::as_str).collect();
    // each round asks the model twice, 2000 tokens charged; the third passes 4500, and the owner's place denies it
    let (mut m, run, d) = budgeted("spent", &plans, 4500);
    let asked = std::fs::read_dir(&d).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().starts_with("asked")).count();
    assert_eq!(asked, 6, "three rounds, then the budget ends it: {:?}", m.record);
    let err = Value::Ctor { name: "std.budgets.Err".into(), args: vec![Value::text("the test's bucket is spent")] };
    assert!(matches!(m.record.iter().rev().find(|l| matches!(l, Line::Ended { .. })), Some(Line::Ended { value, .. }) if *value == err), "{:?}", m.record);
    assert_ne!(m.state(run), State::Raised, "the last run ended by its budget, not by a fault");
    assert!(m.norms.holds(&v("Verify-Intent", &["bob", "ann", "HEAD", "settings load"])), "nothing attested");
}
