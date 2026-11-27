//! `verify-run --release DIR --window EXE --repo DIR --change REV --intent TEXT --author A --verifier V --evidence DIR -- PLANNER...`:
//! std.verify run whole on the built window: A claims the intent with V seated, the build's line, then V's run.
//! PLANNER, the verifier's model, is the caller's, printing `{"answer", "tokens"}` as `coordd call` does; the daemon
//! that links this driver names it. `--bucket B --limit DIM=MOST...` bound the run's cost; nothing else bounds it.

use docket_driver_verify::{limits, live, load, verify, Builder, Config, Owner, Verify};
use docket_v2::norm::Core;
use docket_v2::run::machine::{Machine, State};
use docket_v2::run::norms::Norms;
use docket_v2::run::record::Line;
use docket_v2::run::value::{Instance, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use yaaihh_e2e::{digest, Release};

/// The party that challenges a plan whose Seal attested nothing: the runner, never the verifier.
const RUNNER: &str = "yaaihh-e2e";

fn main() {
    match run() {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(e) => {
            eprintln!("verify-run: {e}");
            std::process::exit(2)
        }
    }
}

fn v(ty: &str, args: &[&str]) -> Instance {
    Instance { ty: format!("std.verify.{ty}"), args: args.iter().map(|a| Value::text(a)).collect() }
}

/// Whether the intent ended attested.
fn run() -> Result<bool, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cut = args.iter().position(|a| a == "--").ok_or("the planner's command goes after --")?;
    let (mut f, mut most): (HashMap<&str, &str>, Vec<(String, i64)>) = (HashMap::new(), Vec::new());
    for pair in args[..cut].chunks(2) {
        let [k, val] = pair else { return Err(format!("{} needs a value", pair[0])) };
        if k == "--limit" {
            let (dim, n) = val.split_once('=').ok_or(format!("--limit {val}: DIM=MOST"))?;
            most.push((dim.into(), n.parse().map_err(|e| format!("--limit {val}: {e}"))?));
        } else {
            f.insert(k.trim_start_matches("--"), val);
        }
    }
    if !most.iter().any(|(d, _)| d == "tokens") {
        return Err("--limit tokens=MOST is needed: with no limit nothing would end the rounds".into());
    }
    let get = |k: &str| f.get(k).copied().ok_or(format!("--{k} is needed"));
    let (change, intent, author, verifier) = (get("change")?, get("intent")?, get("author")?, get("verifier")?);
    let window = PathBuf::from(get("window")?);
    let open = live(Release::folder(&PathBuf::from(get("release")?)), window.clone());
    let refused = Default::default();
    let cfg = Config { repo: get("repo")?.into(), planner: args[cut + 1..].to_vec(), window: window.clone(), evidence: get("evidence")?.into(), open, refused: std::rc::Rc::clone(&refused) };
    let bucket = get("bucket")?;
    let owner = Owner { why: format!("verify-run asks nobody: {bucket}'s limits are its caller's --limit") };
    let mut m = Machine::new(load(&limits(bucket, &most))?, Core::checked(), vec![Box::new(Verify::new(cfg)), Box::new(Builder), Box::new(owner)])?;
    let claim = m.start(Value::text(author), Some(0), "", v("Claim", &[author, change, intent, verifier]))?;
    m.run();
    if m.state(claim) != State::Done {
        return Err(format!("the claim was refused: {:?}", m.record.last()));
    }
    m.event(1, v("Built-From", &[change, &format!("sha256:{}", digest(&window)?)]));
    let r = verify(&mut m, &refused, RUNNER, verifier, change, intent, bucket)?;
    for line in &m.record {
        println!("{line:?}");
    }
    let attested = !m.norms.holds(&v("Verify-Intent", &[verifier, author, change, intent]));
    let ok = Value::Ctor { name: "std.budgets.Ok".into(), args: vec![Value::Bool(true)] };
    let ended = m.record.iter().any(|l| matches!(l, Line::Ended { value, .. } if *value == ok));
    println!("run {:?}; intent attested: {attested}", m.state(r));
    Ok(attested && ended)
}
