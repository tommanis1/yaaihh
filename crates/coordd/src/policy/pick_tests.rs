//! The picker against clingo's solver (paper-research N-568, part 2): the same instance, the same facts, every field
//! of the outcome compared. `differences` is the comparison; the unit tests run it on a policy that uses each thing
//! the language has; `the_recorded_decisions_picked_as_clingo_solved_them` runs it on a coord's recorded `decided`
//! lines (ignored: it reads a copy given by PICK_COORD).

use super::super::compile::{Scope, compile};
use super::super::solve::{Given, Instance, Outcome, Solver};
use super::super::syntax::{Decl, parse};
use super::{Picker, differences};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// What the solver gives for INST with FAILED failed one after another (each solve, then its fail, as `decided`
/// runs them), the last solve's outcome.
fn solved(scopes: &[(Scope, Vec<Decl>)], facts: &[(String, Vec<Value>)], inst: &Instance, failed: &[String]) -> Result<Outcome, String> {
    let mut s = Solver::new(compile(scopes)?);
    s.set_facts(facts.to_vec())?;
    let d = s.open(inst)?;
    for f in failed {
        s.solve(&d, inst)?;
        s.fail(&d, f)?;
    }
    let o = s.solve(&d, inst);
    s.close(&d)?;
    o
}

fn both(scopes: &[(Scope, Vec<Decl>)], facts: &[(String, Vec<Value>)], inst: &Instance, failed: &[String]) -> (Outcome, Outcome, Vec<String>) {
    let s = solved(scopes, facts, inst, failed).unwrap();
    let (p, ties) = Picker::new(scopes, facts).unwrap().pick(inst, failed).unwrap();
    let d = differences(&s, &p, &ties);
    assert!(d.is_empty(), "{inst:?} failed {failed:?}:\n{}", d.join("\n"));
    (s, p, ties)
}

/// A policy with each thing the language has: listed and fact options, Requires, Only with a `When` on the call and on
/// the option, an agent's file, A > B > C, minimise and maximise, a fact as a value with several values, `_`, `not`,
/// a derived fact through `not` of another, a stated instance, a number compared with a string.
const PRELUDE: &str = "\
Fact model Identified by String
Fact family Identified by model * String
Fact price Identified by model * Int
Fact speed Identified by model * Int
Fact harness Identified by model * String
Fact tier Identified by model * Int
Fact banned Identified by model
Fact usable Identified by model
Fact cheap Identified by model
  Derived from cheap(m) When price(m, p) and p < 1000
Fact pricey Identified by model
  Derived from pricey(m) When model(m) and not cheap(m)
+tier(\"haiku\", 1).
+tier(\"sonnet\", 2).
+tier(\"opus\", 3).
+tier(\"luna\", 1).
+banned(\"astra\").
Decision model-for
  Options model
  Requires price(option) >= 0
  Requires not banned(option)
Decision how
  Options fast | careful | skip
";

const PROJECT: &str = "\
Only for model-for When call.place == \"agent\":
  harness(option, _)
Only for model-for When call.task == \"big\":
  tier(option) >= 2
Only for model-for When tier(option) == 3:
  call.task != \"small\"
Prefer for model-for When call.task == \"recognise\":
  family(option, \"codex\") > family(option, \"claude\") > family(option, \"kimi\")
Prefer for model-for:
  minimise price(option)
Prefer for model-for When call.task == \"race\":
  maximise speed(option)
Prefer for model-for:
  pricey(option) > cheap(option)
Prefer for how When call.mood == \"calm\":
  option == \"careful\" > option == \"fast\"
Only for how When call.n >= 3:
  option != \"skip\"
";

const AGENT: &str = "\
Prefer for model-for:
  family(option, \"claude\")
";

fn scopes() -> Vec<(Scope, Vec<Decl>)> {
    vec![(Scope::Global, parse("prelude", PRELUDE).unwrap()), (Scope::Project, parse("decisions", PROJECT).unwrap()), (Scope::Agent("w2".into()), parse("w2", AGENT).unwrap())]
}

fn facts() -> Vec<(String, Vec<Value>)> {
    let mut f = Vec::new();
    for (m, fam, p, h) in [("astra", "codex", 1250, Some("codex")), ("luna", "codex", 1250, Some("codex")), ("haiku", "claude", 800, Some("claude")), ("sonnet", "claude", 3000, Some("claude")), ("opus", "claude", 15000, Some("claude")), ("k2", "kimi", 600, None), ("free", "kimi", -1, None)] {
        f.push(("model".to_string(), vec![json!(m)]));
        f.push(("family".to_string(), vec![json!(m), json!(fam)]));
        f.push(("price".to_string(), vec![json!(m), json!(p)]));
        if let Some(h) = h {
            f.push(("harness".to_string(), vec![json!(m), json!(h)]));
        }
    }
    // two speeds for luna: each is a weight of its own
    for (m, s) in [("luna", 5), ("luna", 9), ("haiku", 7), ("k2", 7)] {
        f.push(("speed".to_string(), vec![json!(m), json!(s)]));
    }
    f
}

fn inst(kind: &str, call: &[(&str, &str)], given: Vec<Given>) -> Instance {
    Instance { kind: kind.into(), call: call.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), given, unmet: Vec::new() }
}

#[test]
fn the_picker_gives_what_clingo_gives_on_each_part_of_the_language() {
    let (sc, f) = (scopes(), facts());
    let calls: Vec<Vec<(&str, &str)>> = vec![
        vec![],
        vec![("task", "recognise")],
        vec![("task", "recognise"), ("place", "agent")],
        vec![("task", "big")],
        vec![("task", "small")],
        vec![("task", "race")],
        vec![("task", "race"), ("agent", "w2")],
        vec![("task", "recognise"), ("agent", "w2"), ("place", "agent")],
        vec![("place", "agent"), ("task", "big"), ("agent", "w3")],
    ];
    for c in &calls {
        let i = inst("model-for", c, Vec::new());
        let (s, ..) = both(&sc, &f, &i, &[]);
        // the next best when the chosen one fails, twice
        if let Some(first) = s.chosen.clone() {
            let (s2, ..) = both(&sc, &f, &i, std::slice::from_ref(&first));
            if let Some(second) = s2.chosen {
                both(&sc, &f, &i, &[first, second]);
            }
        }
    }
    // a given field kept, one no option has, and a number given as text
    let g = |field: &str, value: &str| Given { field: field.into(), value: value.into(), by: "owner".into(), at: "2026-11-18T19:00".into() };
    both(&sc, &f, &inst("model-for", &[("task", "recognise")], vec![g("family", "claude")]), &[]);
    let (s, ..) = both(&sc, &f, &inst("model-for", &[], vec![g("family", "gemini")]), &[]);
    assert!(s.fault.as_deref().is_some_and(|f| f.starts_with("no_route")), "{s:?}");
    both(&sc, &f, &inst("model-for", &[], vec![g("price", "800")]), &[]);
    // the call's `where`, on an option and on a name that is none
    let mut w = inst("model-for", &[("where", "large")], Vec::new());
    w.unmet = vec!["haiku".into(), "nothing".into()];
    both(&sc, &f, &w, &[]);
    // listed options: a tie, an order, a number field compared, nothing left
    for c in [vec![], vec![("mood", "calm")], vec![("n", "3")], vec![("n", "2"), ("mood", "calm")]] {
        both(&sc, &f, &inst("how", &c, Vec::new()), &[]);
    }
    both(&sc, &f, &inst("how", &[("n", "5")], Vec::new()), &["fast".into(), "careful".into()]);
    // the derived facts hold as clingo derives them
    let p = Picker::new(&sc, &f).unwrap();
    let mut cheap = p.holding("cheap");
    cheap.sort();
    assert_eq!(cheap, vec![vec!["free".to_string()], vec!["haiku".into()], vec!["k2".into()]]);
    let mut s = Solver::new(compile(&sc).unwrap());
    s.set_facts(f.clone()).unwrap();
    let mut pricey = s.holding("pricey").unwrap();
    pricey.sort();
    let mut mine = p.holding("pricey");
    mine.sort();
    assert_eq!(mine, pricey);
}

/// solve.rs's own example (draft §10(a)): the same outcome, reasons and levels.
#[test]
fn the_drafts_example_picked() {
    let pre = "Fact model Identified by String\nFact family Identified by model * String\nFact price Identified by model * Int\nFact quota-left Identified by model * Int\nDecision model-for\n  Options model\n  Requires price(option) >= 0\n  Requires quota-left(option) > 0\n";
    let proj = "Prefer for model-for When call.task == \"recognise-question\":\n  family(option, \"codex\") > family(option, \"claude\")\nPrefer for model-for:\n  minimise price(option)\n";
    let sc = vec![(Scope::Global, parse("prelude", pre).unwrap()), (Scope::Project, parse("decisions.eflint", proj).unwrap())];
    let mut f = Vec::new();
    for (m, fam, q, p) in [("astra", "codex", 0, 1250), ("luna", "codex", 40, 1250), ("haiku", "claude", 90, 800), ("sonnet", "claude", 90, 3000)] {
        f.push(("model".to_string(), vec![json!(m)]));
        f.push(("family".to_string(), vec![json!(m), json!(fam)]));
        f.push(("quota-left".to_string(), vec![json!(m), json!(q)]));
        f.push(("price".to_string(), vec![json!(m), json!(p)]));
    }
    let i = inst("model-for", &[("task", "recognise-question")], Vec::new());
    let (_, p, _) = both(&sc, &f, &i, &[]);
    assert_eq!(p.chosen.as_deref(), Some("luna"));
    assert_eq!(p.decided_by.map(|c| c.level), Some(2000));
    let (_, p, _) = both(&sc, &f, &i, &["luna".into()]);
    assert_eq!((p.chosen.as_deref(), p.decided_by.map(|c| c.level)), (Some("haiku"), Some(1999)));
}

/// The provider of a model as the proxy names it (`owned_by`), from its name: what the recorded lines name.
fn provider(m: &str) -> &'static str {
    if m.starts_with("claude") {
        "anthropic"
    } else if m.starts_with("gpt") || m.starts_with('o') {
        "openai"
    } else if m.starts_with("kimi") {
        "moonshot"
    } else if m.starts_with("gemini") {
        "google"
    } else {
        ""
    }
}

/// N-568 part 2: every recorded `decided` line of the coord copy PICK_COORD (its `tickets/log.jsonl`, or `log.jsonl`
/// beside `project`), decided again by clingo and by the picker on the same facts and today's policy (this checkout's
/// agent-base and the project PICK_PROJECT, else the copy's own project file's folder), and each compared; printed:
/// the counts, every difference, and how each engine's choice stands against the recorded one.
///
/// The record keeps the instance and the outcome, not the facts: for `model-for` the served models are every model the
/// recorded lines name (their family and price as `facts::model_facts` gives them today); for `fills` each line's
/// options are its own, holding the role and `running` unless the line excluded them for it (fills has no preference,
/// so every allowed option is among the line's optimal ones). A failed call is failed again in its order.
#[test]
#[ignore]
fn the_recorded_decisions_picked_as_clingo_solved_them() {
    let dir = std::env::var("PICK_COORD").expect("PICK_COORD: a copy of a coord's record");
    let log = [format!("{dir}/tickets/log.jsonl"), format!("{dir}/log.jsonl")].into_iter().find(|p| std::path::Path::new(p).exists()).expect("a log.jsonl");
    let repo = format!("{}/../../..", env!("CARGO_MANIFEST_DIR"));
    let project = std::env::var("PICK_PROJECT").ok().or_else(|| {
        let p = std::fs::read_to_string(format!("{dir}/project")).ok()?;
        Some(std::path::Path::new(p.trim()).parent()?.display().to_string())
    });
    let places = super::super::Places { base: format!("{repo}/agent-base"), project: project.clone(), coord: dir.clone(), ..Default::default() };
    let sc = places.sources().unwrap();
    compile(&sc).unwrap();
    let lines: Vec<Value> = std::fs::read_to_string(&log).unwrap().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "decided").collect();
    // the served models: every model a recorded model-for line names
    let mut served: Vec<String> = Vec::new();
    for v in lines.iter().filter(|v| v["question"] == "model-for") {
        let p = &v["provenance"];
        let others = p["optimal"].as_array().into_iter().flatten().chain([&p["chosen"], &p["runner_up"][0]]).chain(p["attempts"].as_array().into_iter().flatten().map(|a| &a["model"]));
        let named = p["excluded"].as_array().into_iter().flatten().flat_map(coord_types::excluded_options).chain(others.filter_map(Value::as_str));
        for m in named {
            if !served.iter().any(|s| s == m) {
                served.push(m.to_string());
            }
        }
    }
    served.sort();
    let model_facts = super::super::facts::model_facts(&served.iter().map(|m| (m.clone(), provider(m).to_string())).collect::<Vec<_>>());
    let types = compile(&sc).unwrap().facts;
    let declared = |facts: Vec<(String, Vec<Value>)>| -> Vec<(String, Vec<Value>)> { facts.into_iter().filter(|(f, a)| types.get(f).is_some_and(|t| t.len() == a.len())).collect() };
    let model_facts = declared(model_facts);
    let model_picker = Picker::new(&sc, &model_facts).unwrap();
    let mut model_solver = Solver::new(compile(&sc).unwrap());
    model_solver.set_facts(model_facts.clone()).unwrap();
    let (mut n, mut same, mut skipped) = (BTreeMap::<String, usize>::new(), BTreeMap::<String, usize>::new(), Vec::new());
    let (mut vs_record_clingo, mut vs_record_picker, mut runner_ties) = (BTreeMap::<String, usize>::new(), BTreeMap::<String, usize>::new(), 0usize);
    let mut diffs: Vec<String> = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let (mut t_clingo, mut t_picker) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
    for v in &lines {
        let q = v["question"].as_str().unwrap_or("").to_string();
        let p = if q == "fills" { v["provenance"].get("policy").unwrap_or(&v["provenance"]).clone() } else { v["provenance"].clone() };
        let at = format!("{} t{} step {}", v["at"].as_str().unwrap_or(""), v["t"], v["step"]);
        let call: BTreeMap<String, String> = match q.as_str() {
            "model-for" => serde_json::from_value(p["call"].clone()).unwrap_or_default(),
            _ => BTreeMap::new(),
        };
        if q == "fills" {
            // the role the line was for is not in its provenance; the options' role facts stand for it: each option holds
            // `role(option, R)` for one R the call names unless excluded for it
            let options: Vec<String> = {
                let mut o: Vec<String> = p["excluded"].as_array().into_iter().flatten().filter_map(|e| e["option"].as_str().map(String::from)).chain(p["optimal"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from))).collect();
                o.sort();
                o.dedup();
                o
            };
            let out_for = |o: &str, what: &str| p["excluded"].as_array().into_iter().flatten().any(|e| e["option"] == o && e["rule"].as_str().is_some_and(|r| r.contains(what)));
            let mut facts: Vec<(String, Vec<Value>)> = Vec::new();
            for o in &options {
                facts.push(("agent".into(), vec![json!(o)]));
                if !out_for(o, "role(option, call.role)") {
                    facts.push(("role".into(), vec![json!(o), json!("the-role")]));
                }
                if !out_for(o, "running(option)") {
                    facts.push(("running".into(), vec![json!(o)]));
                }
            }
            let facts = declared(facts);
            let i = Instance { kind: "fills".into(), call: BTreeMap::from([("role".to_string(), "the-role".to_string()), ("call".to_string(), String::new()), ("ticket".to_string(), String::new())]), given: Vec::new(), unmet: Vec::new() };
            *n.entry(q.clone()).or_default() += 1;
            let t0 = std::time::Instant::now();
            let s = solved(&sc, &facts, &i, &[]);
            t_clingo += t0.elapsed();
            let t0 = std::time::Instant::now();
            let pk = Picker::new(&sc, &facts).and_then(|pk| pk.pick(&i, &[]));
            t_picker += t0.elapsed();
            match (s, pk) {
                (Ok(s), Ok((pk, ties))) => {
                    let d = differences(&s, &pk, &ties);
                    if d.is_empty() {
                        *same.entry(q.clone()).or_default() += 1;
                    } else {
                        diffs.push(format!("{q} {at}:\n  {}", d.join("\n  ")));
                    }
                    let rec = p["chosen"].as_str().map(String::from);
                    *vs_record_clingo.entry(format!("{q} {}", if s.chosen == rec { "same" } else { "differs" })).or_default() += 1;
                    *vs_record_picker.entry(format!("{q} {}", if pk.chosen == rec { "same" } else { "differs" })).or_default() += 1;
                }
                (s, pk) => diffs.push(format!("{q} {at}: clingo {:?}, picker {:?}", s.err(), pk.err())),
            }
            continue;
        }
        if q != "model-for" {
            skipped.push(format!("{q} {at}"));
            continue;
        }
        let given: Vec<Given> = serde_json::from_value(p["given"].clone()).unwrap_or_default();
        let unmet: Vec<String> = p["excluded"].as_array().into_iter().flatten().filter(|e| e["reason"] == "where").filter_map(|e| e["option"].as_str().map(String::from)).collect();
        let failed: Vec<String> = p["attempts"].as_array().into_iter().flatten().filter(|a| !a["failure"].is_null()).filter_map(|a| a["model"].as_str().map(String::from)).collect();
        let i = Instance { kind: q.clone(), call, given, unmet };
        *n.entry(q.clone()).or_default() += 1;
        // one solver for every model-for line, as coordd's engine holds one (open, solve, fail, close)
        let t0 = std::time::Instant::now();
        let s = (|| -> Result<Outcome, String> {
            let d = model_solver.open(&i)?;
            for f in &failed {
                model_solver.solve(&d, &i)?;
                model_solver.fail(&d, f)?;
            }
            let o = model_solver.solve(&d, &i);
            model_solver.close(&d)?;
            o
        })();
        t_clingo += t0.elapsed();
        let t0 = std::time::Instant::now();
        let pk = model_picker.pick(&i, &failed);
        t_picker += t0.elapsed();
        match (s, pk) {
            (Ok(s), Ok((pk, ties))) => {
                if ties.len() > 1 {
                    runner_ties += 1;
                }
                let d = differences(&s, &pk, &ties);
                let key = serde_json::to_string(&(&i.call, &i.given, &i.unmet, &failed)).unwrap();
                if d.is_empty() {
                    *same.entry(q.clone()).or_default() += 1;
                } else if seen.insert(key, 1).is_none() {
                    diffs.push(format!("{q} {at} {:?}:\n  {}", i.call, d.join("\n  ")));
                }
                let rec = p["chosen"].as_str().map(String::from);
                *vs_record_clingo.entry(format!("{q} {}", if s.chosen == rec { "same" } else { "differs" })).or_default() += 1;
                *vs_record_picker.entry(format!("{q} {}", if pk.chosen == rec { "same" } else { "differs" })).or_default() += 1;
            }
            (s, pk) => diffs.push(format!("{q} {at}: clingo {:?}, picker {:?}", s.err(), pk.err())),
        }
    }
    println!("record {log}: {} decided lines; policy {}/agent-base + {project:?}", lines.len(), repo);
    println!("served models (from the lines): {}", served.len());
    for (q, k) in &n {
        println!("{q}: {k} replayed, {} the same in every field, {} differ", same.get(q).copied().unwrap_or(0), k - same.get(q).copied().unwrap_or(0));
    }
    println!("runner-up tied between several options (any is clingo's): {runner_ties}");
    println!("against the recorded choice, clingo today: {vs_record_clingo:?}");
    println!("against the recorded choice, picker: {vs_record_picker:?}");
    println!("not replayed: {} {skipped:?}", skipped.len());
    println!("time (unoptimised build): clingo {:.1} s, picker {:.1} s (fills: each with its own solver or picker, as coordd makes one per question)", t_clingo.as_secs_f64(), t_picker.as_secs_f64());
    println!("distinct differences: {}", diffs.len());
    for d in &diffs {
        println!("{d}");
    }
}

/// In `both` (paper-research N-568 p14): the picker's answer set beside the solver's gives no line when they agree; a
/// line naming the decision, the instance and the difference when they do not; the picker's fault and a panic in it
/// each as a line, never as the decision's fault (the solver's answer is the caller's either way).
#[test]
fn beside_the_solver_the_picker_journals_only_a_difference() {
    use super::{beside, guarded, holding_beside};
    let pre = "Fact model Identified by String\nFact price Identified by model * Int\nDecision model-for\n  Options model\n";
    let sc = vec![(Scope::Global, parse("prelude", pre).unwrap()), (Scope::Project, parse("decisions.eflint", "Prefer for model-for:\n  minimise price(option)\n").unwrap())];
    let facts = |cheap: &str| vec![("model".to_string(), vec![json!("a")]), ("model".to_string(), vec![json!("b")]), ("price".to_string(), vec![json!("a"), json!(if cheap == "a" { 1 } else { 9 })]), ("price".to_string(), vec![json!("b"), json!(if cheap == "b" { 1 } else { 9 })])];
    let inst = Instance { kind: "model-for".into(), call: BTreeMap::from([("task".to_string(), "t".to_string())]), given: Vec::new(), unmet: Vec::new() };
    let s = solved(&sc, &facts("a"), &inst, &[]).unwrap();
    assert_eq!(s.chosen.as_deref(), Some("a"));
    let same = Picker::new(&sc, &facts("a")).unwrap();
    assert_eq!(beside(Ok(&same), &inst, &[], Ok(&s)), None);
    let failed = vec!["a".to_string()];
    let s2 = solved(&sc, &facts("a"), &inst, &failed).unwrap();
    assert_eq!(beside(Ok(&same), &inst, &failed, Ok(&s2)), None);
    let other = Picker::new(&sc, &facts("b")).unwrap();
    let line = beside(Ok(&other), &inst, &[], Ok(&s)).unwrap();
    assert!(line.starts_with("coordd: decider both: model-for {"), "{line}");
    assert!(line.contains("chosen: clingo Some(\"a\"), picker Some(\"b\")"), "{line}");
    let line = beside(Err("the policy: no"), &inst, &[], Ok(&s)).unwrap();
    assert!(line.ends_with("the picker faulted: the policy: no"), "{line}");
    let other_kind = Instance { kind: "fills".into(), ..inst.clone() };
    assert_eq!(beside(Ok(&same), &other_kind, &[], Err("no decision type")), None);
    assert!(guarded::<()>(|| panic!("inside")).unwrap_err().contains("the picker panicked: inside"));
    let held = vec![vec!["a".to_string()], vec!["b".to_string()]];
    assert_eq!(holding_beside(Ok(&same), "model", &held), None);
    let line = holding_beside(Ok(&same), "model", &held[..1]).unwrap();
    assert!(line.contains("picker only [[\"b\"]]"), "{line}");
}

/// What `both` adds to a decision (paper-research N-568 p14: say it in the hand-over), on the system's policy
/// (agent-base) with twelve served models and a team of thirty: for `model-for`, one pick on the picker made with the
/// facts (made again only when they are, every `FACTS_FOR`); for `fills` and `roles_held`, the picker made for the
/// question and its pick. Beside each, the solver's own time for the same. Ignored: a timing, not a check.
#[test]
#[ignore]
fn what_both_adds_to_a_decision() {
    use std::time::Instant;
    let places = super::super::Places { base: format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), ..Default::default() };
    let sc = places.sources().unwrap();
    let served: Vec<(String, String)> = [("gpt-6-luna", "openai"), ("gpt-6-sol", "openai"), ("gpt-6.1-sol", "openai"), ("gpt-5.6-luna", "openai"), ("gpt-5.5", "openai"), ("claude-opus-5-5", "anthropic"), ("claude-sonnet-5-5", "anthropic"), ("claude-haiku-5-5", "anthropic"), ("claude-fable-5-2", "anthropic"), ("kimi-k2", "moonshot"), ("glm-5", "zhipu"), ("qwen-4", "alibaba")].iter().map(|(m, p)| (m.to_string(), p.to_string())).collect();
    let mf = super::super::facts::model_facts(&served);
    let n = 200u32;
    let t0 = Instant::now();
    for _ in 0..n {
        Picker::new(&sc, &mf).unwrap();
    }
    let new_models = t0.elapsed() / n;
    let p = Picker::new(&sc, &mf).unwrap();
    let mut solver = Solver::new(compile(&sc).unwrap());
    solver.set_facts(mf.clone()).unwrap();
    let mut out = Vec::new();
    for (task, place) in [("intake", "call"), ("route", "call"), ("step", "agent")] {
        let inst = Instance { kind: "model-for".into(), call: BTreeMap::from([("task".to_string(), task.to_string()), ("agent".to_string(), "w1".to_string()), ("place".to_string(), place.to_string())]), given: Vec::new(), unmet: Vec::new() };
        let t0 = Instant::now();
        for _ in 0..n {
            p.pick(&inst, &[]).unwrap();
        }
        let picked = t0.elapsed() / n;
        let t0 = Instant::now();
        for _ in 0..n {
            let d = solver.open(&inst).unwrap();
            solver.solve(&d, &inst).unwrap();
            solver.close(&d).unwrap();
        }
        let solved = t0.elapsed() / n;
        let s = { let d = solver.open(&inst).unwrap(); let o = solver.solve(&d, &inst).unwrap(); solver.close(&d).unwrap(); o };
        assert_eq!(super::beside(Ok(&p), &inst, &[], Ok(&s)), None);
        out.push(format!("model-for {task}: pick {picked:?} (solver open+solve+close {solved:?}); chose {:?}", s.chosen));
    }
    let types = compile(&sc).unwrap().facts;
    let mut team: Vec<(String, Vec<Value>)> = Vec::new();
    for k in 0..30 {
        let a = format!("a{k}");
        team.push(("agent".into(), vec![json!(a)]));
        team.push(("role".into(), vec![json!(a), json!(["lead", "orchestrator", "member", "fixer", "reviewer"][k % 5])]));
        if k % 3 != 0 {
            team.push(("running".into(), vec![json!(a)]));
        }
    }
    team.push(("ticket".into(), vec![json!("C-1")]));
    let team: Vec<(String, Vec<Value>)> = team.into_iter().filter(|(f, a)| types.get(f).is_some_and(|t| t.len() == a.len())).collect();
    let inst = Instance { kind: "fills".into(), call: BTreeMap::from([("role".to_string(), "fixer".to_string()), ("call".to_string(), "triage".to_string()), ("ticket".to_string(), "C-1".to_string())]), given: Vec::new(), unmet: Vec::new() };
    let t0 = Instant::now();
    for _ in 0..n {
        Picker::new(&sc, &team).unwrap().pick(&inst, &[]).unwrap();
    }
    let picked = t0.elapsed() / n;
    let t0 = Instant::now();
    for _ in 0..n {
        Picker::new(&sc, &team).unwrap().holding("role");
    }
    let held = t0.elapsed() / n;
    let t0 = Instant::now();
    for _ in 0..n {
        solved(&sc, &team, &inst, &[]).unwrap();
    }
    let solved_fills = t0.elapsed() / n;
    let t0 = Instant::now();
    for _ in 0..n {
        let mut s = Solver::new(compile(&sc).unwrap());
        s.set_facts(team.clone()).unwrap();
        s.holding("role").unwrap();
    }
    let solved_held = t0.elapsed() / n;
    out.push(format!("model-for: the picker made with the model facts {new_models:?} (once per FACTS_FOR)"));
    out.push(format!("fills: picker made + pick {picked:?} (solver compile+ground+solve {solved_fills:?})"));
    out.push(format!("roles_held: picker made + holding {held:?} (solver {solved_held:?})"));
    println!("{}", out.join("\n"));
}
