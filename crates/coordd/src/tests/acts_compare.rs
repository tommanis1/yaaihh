//! The same answers on acts (paper-research N-568, part 1): eflint-rs against policy/norms.rs on every act of a
//! project's policy files, in the situations coordd asks its norms in, built from a copy of a coord (its tickets
//! record and its registry journal). A harness and a table: nothing is switched.
//!
//! The policy's declarations are written out as eFLINT (`to_eflint`), as one would write them in the language whole:
//! a field's value is an instance of its type (`agent("orch")`, where norms.rs has the bare `"orch"`), each field is
//! named by its type (`agent1`, `agent2`: eFLINT has no `actor`/`recipient` names), and a fact read as a function
//! (`waited(ticket)`, `family(runs(actor))`, coordd's own) is an `Exists` over the fact's last identifier. Two writings
//! of the literals: as written (`actor != "owner"`), and typed (`agent1 != agent("owner")`, where the other side is of
//! a fact type), so the first departure of norms.rs (a field's value is the identifier) shows where it changes an
//! answer.

use super::*;
use crate::policy::norms;
use crate::policy::syntax::{Cond, DeclKind, Expr, Lit, Op};
use eflint::ast::{Elem, Tagged};
use std::collections::HashSet;
use eflint::replay::{Event, Replay};
use std::fmt::Write as _;

type Facts = Vec<(String, Vec<String>)>;
/// An act instance: the act's name and its fields' identifiers.
type Inst = (String, Vec<String>);
use crate::policy::interp::{facts_text, leaf, norms_enabled, to_eflint, Spec};
/// The act instances eflint-rs has enabled after FACTS are stated in BASE's state; or its errors.
fn eflint_enabled(base: &Replay, spec: &Spec, facts: &Facts) -> Result<BTreeSet<Inst>, String> {
    let mut r = Replay { cfg: base.cfg.clone() };
    let a = r.apply(&Event::Phrase(facts_text(spec, facts)));
    if !a.ok || !a.errors.is_empty() {
        return Err(a.errors.join("; "));
    }
    Ok(a.enabled.iter().filter(|e| spec.acts.contains_key(&*e.act.dom)).map(|e| (e.act.dom.to_string(), leaf(&e.act))).collect())
}

/// How many instances of each act there are to ask in FACTS (the product of its fields' instances that hold, as both
/// engines enumerate them).
fn asked(spec: &Spec, facts: &Facts) -> usize {
    let mut of: BTreeMap<&str, BTreeSet<&Vec<String>>> = BTreeMap::new();
    for (f, ids) in facts {
        of.entry(f.as_str()).or_default().insert(ids);
    }
    spec.acts.values().map(|fields| fields.iter().map(|(_, t)| of.get(t.as_str()).map_or(0, |s| s.len())).product::<usize>()).sum()
}

/// The situations coordd asks its norms in, from CORE as loaded from a copy: (what it is, the facts).
fn situations(core: &Core) -> Vec<(String, Facts)> {
    let mut out: Vec<(String, Facts)> = Vec::new();
    out.push(("the team (mail, esc, models, teams)".into(), core.norm_facts(&[])));
    let m = &core.tickets.m;
    // a thread or ticket asked as "t", with those taking part (`norm_verdict`)
    let mut broughts: BTreeSet<Vec<String>> = BTreeSet::new();
    for x in m.tickets.values() {
        let mut p: Vec<String> = x.given.iter().map(|g| g.to.clone()).chain(core.takers(x.t)).collect();
        p.sort();
        p.dedup();
        broughts.insert(p);
    }
    for b in broughts {
        out.push((format!("a ticket asked as t, taking part: {}", b.join(" ")), core.norm_facts(&b)));
    }
    // each ticket as a post, an edit or a close on it is asked about (`ticket_facts`, `with_notify_facts`); with
    // ACTS_EVERY=N every open ticket and every Nth of the rest
    let every: usize = std::env::var("ACTS_EVERY").ok().and_then(|n| n.parse().ok()).unwrap_or(1).max(1);
    for (i, x) in m.tickets.values().enumerate() {
        if !x.is_open() && i % every != 0 {
            continue;
        }
        let id = m.id(x.t);
        let facts = core.ticket_facts(Some(x.t)).0;
        out.push((format!("{id}: a post on it"), with_notify_facts(facts.clone(), "post")));
        let mut closed = with_notify_facts(facts, "closed");
        if matches!(x.status, docket_machine::Status::Failed(_)) {
            closed.push(("closed-failed".into(), vec![id.clone()]));
        }
        out.push((format!("{id}: its close"), closed));
    }
    // a ticket opened, of each kind there is (`may`, asked as `new`)
    let kinds: BTreeSet<String> = m.tickets.values().map(|x| x.kind.clone()).chain(["item", "claim", "fix", "check", "decision"].map(String::from)).collect();
    for k in kinds {
        let mut f = with_notify_facts(core.ticket_facts(None).0, "post");
        f.push(("ticket".into(), vec!["new".into()]));
        f.push(("ticket-kind".into(), vec!["new".into(), k.clone()]));
        out.push((format!("a {k} opened"), f));
    }
    // the steps owed at the record's end, as `expiry_question` asks them, and two hours later
    let end = m.tickets.values().flat_map(|x| x.posts.last().map(|p| p.at.clone())).max().and_then(|a| chrono::DateTime::parse_from_rfc3339(&a).ok()).map(|d| d.with_timezone(&chrono::Utc)).unwrap_or_else(chrono::Utc::now);
    for (label, now) in [("at the record's end", end), ("two hours after the record's end", end + chrono::Duration::hours(2))] {
        let mut f = core.norm_facts(&[]);
        for (t, _, agent, since) in m.awaiting() {
            if !f.iter().any(|(n, v)| n == "agent" && v[0] == agent) {
                f.push(("agent".into(), vec![agent.clone()]));
            }
            let id = m.id(t);
            f.push(("ticket".into(), vec![id.clone()]));
            f.push(("awaits".into(), vec![id.clone(), agent.clone()]));
            f.push(("waited".into(), vec![id, core.silent_minutes(t, &agent, &since, now).to_string()]));
        }
        out.push((format!("the steps owed, {label}"), f));
    }
    out
}

/// A copy of a coord for the harness: its tickets record, its project file and its registry journal without the
/// `session` lines (a session's id is not wanted here).
fn copy_coord(from: &str, to: &std::path::Path) {
    let _ = std::fs::remove_dir_all(to);
    std::fs::create_dir_all(to.join("tickets")).unwrap();
    let whole = |b: Vec<u8>| b[..b.iter().rposition(|c| *c == b'\n').map_or(0, |i| i + 1)].to_vec();
    std::fs::write(to.join("tickets/log.jsonl"), whole(std::fs::read(format!("{from}/tickets/log.jsonl")).unwrap())).unwrap();
    std::fs::copy(format!("{from}/project"), to.join("project")).unwrap();
    let reg = String::from_utf8(whole(std::fs::read(format!("{from}/registry.jsonl")).unwrap_or_default())).unwrap();
    let kept: String = reg.lines().filter(|l| serde_json::from_str::<Value>(l).ok().and_then(|v| v.get("ev").and_then(Value::as_str).map(|e| e != "session")).unwrap_or(false)).map(|l| format!("{l}\n")).collect();
    std::fs::write(to.join("registry.jsonl"), kept).unwrap();
}

/// The models the norms are given: the proxy's, in production; here the policy's `tier` instances, each in the family
/// the proxy would give it (claude, codex).
fn models(places: &crate::policy::Places) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (_, decls) in places.sources().unwrap_or_default() {
        for d in decls {
            if let DeclKind::Instance { fact, args } = &d.kind {
                if let (true, Some(Expr::Str(m))) = (fact == "tier", args.first()) {
                    out.push((m.clone(), if m.starts_with("claude") { "claude".into() } else { "codex".into() }));
                }
            }
        }
    }
    out
}

/// ACTS_COORD: a coord (read only; copied to ACTS_OUT/coord first), ACTS_BASE: agent-base, ACTS_OUT: where the table
/// and the eFLINT texts are written.
#[test]
#[ignore]
fn eflint_rs_answers_the_acts_as_norms_rs() {
    let (Ok(from), Ok(base), Ok(out)) = (std::env::var("ACTS_COORD"), std::env::var("ACTS_BASE"), std::env::var("ACTS_OUT")) else { return };
    let out = std::path::PathBuf::from(out);
    let coord = out.join("coord");
    copy_coord(&from, &coord);
    let coord = coord.display().to_string();
    let mut core = crate::core::Core::load(&coord, &base);
    core.project = crate::project::Project::load(&coord).unwrap();
    core.load_tickets();
    let places = core.places();
    core.set_models(models(&places));
    let n = norms::load(&places).expect("norms.rs loads the policy");
    let sits = situations(&core);
    let mut report = String::new();
    writeln!(report, "coord {from}: {} record lines, {} tickets, {} agents; policy {} acts: {}", std::fs::read_to_string(format!("{coord}/tickets/log.jsonl")).unwrap().lines().count(), core.tickets.m.tickets.len(), core.agents.len(), n.acts.len(), n.acts.keys().cloned().collect::<Vec<_>>().join(", ")).unwrap();
    // the same situations once: many tickets give the same facts
    let mut seen: BTreeMap<BTreeSet<(String, Vec<String>)>, Vec<String>> = BTreeMap::new();
    for (what, f) in &sits {
        seen.entry(f.iter().filter(|(k, _)| n.facts.contains_key(k)).cloned().collect()).or_default().push(what.clone());
    }
    writeln!(report, "{} situations, {} different (ACTS_EVERY={})", sits.len(), seen.len(), std::env::var("ACTS_EVERY").unwrap_or("1".into())).unwrap();
    // the norms.rs answers, once per situation
    // ACTS_BUDGET seconds for the whole (default 1200: the runner stops a test at 30 minutes); the literals as written
    // for the first ACTS_WRITTEN situations (default 30), typed for every one
    let budget = std::time::Duration::from_secs(std::env::var("ACTS_BUDGET").ok().and_then(|n| n.parse().ok()).unwrap_or(1200));
    let written: usize = std::env::var("ACTS_WRITTEN").ok().and_then(|n| n.parse().ok()).unwrap_or(30);
    let mut questions = 0usize;
    let mut specs = Vec::new();
    for typed in [false, true] {
        let label = if typed { "typed literals" } else { "literals as written" };
        match to_eflint(&places, typed).and_then(|spec| Replay::from_text(&spec.text).map(|r| (spec, r))) {
            Ok((spec, r)) => {
                std::fs::write(out.join(format!("policy-{}.eflint", if typed { "typed" } else { "as-written" })), &spec.text).unwrap();
                specs.push((label, spec, r));
            }
            Err(e) => writeln!(report, "\n[{label}] eflint-rs does not load it: {e}").unwrap(),
        }
    }
    #[derive(Default)]
    struct Tally {
        same: usize,
        differ: usize,
        errors: usize,
        asked: usize,
        secs: f64,
        by_act: BTreeMap<String, (usize, usize)>,
        lines: Vec<String>,
    }
    let mut tallies: Vec<Tally> = specs.iter().map(|_| Tally::default()).collect();
    let (t0, mut t_ours, mut done) = (std::time::Instant::now(), 0f64, 0usize);
    // in the order the situations were made (the team, the threads, the tickets...), each once
    let mut order: Vec<(&BTreeSet<(String, Vec<String>)>, &Vec<String>)> = Vec::new();
    let mut put: BTreeSet<&BTreeSet<(String, Vec<String>)>> = BTreeSet::new();
    for (_, f) in &sits {
        let key: BTreeSet<(String, Vec<String>)> = f.iter().filter(|(k, _)| n.facts.contains_key(k)).cloned().collect();
        if let Some((k, w)) = seen.get_key_value(&key) {
            if put.insert(k) {
                order.push((k, w));
            }
        }
    }
    for (k, (facts, whats)) in order.into_iter().enumerate() {
        if t0.elapsed() > budget {
            break;
        }
        if k % 50 == 0 {
            eprintln!("situation {k} of {}, {:.0} s", seen.len(), t0.elapsed().as_secs_f64());
        }
        let facts: Facts = facts.iter().cloned().collect();
        let t = std::time::Instant::now();
        let ours = n.solve(&facts).map(|s| norms_enabled(&n, &s));
        t_ours += t.elapsed().as_secs_f64();
        done += 1;
        for ((label, spec, replay), tally) in specs.iter().zip(tallies.iter_mut()) {
            if *label == "literals as written" && k >= written {
                continue;
            }
            tally.asked += 1;
            if *label == "typed literals" {
                questions += asked(spec, &facts);
            }
            let t = std::time::Instant::now();
            let theirs = eflint_enabled(replay, spec, &facts);
            tally.secs += t.elapsed().as_secs_f64();
            match (&ours, theirs) {
                (Ok(a), Ok(b)) if a == &b => {
                    tally.same += 1;
                    for (act, _) in a {
                        tally.by_act.entry(act.clone()).or_default().0 += 1;
                    }
                }
                (Ok(a), Ok(b)) => {
                    tally.differ += 1;
                    let only_ours: Vec<&Inst> = a.difference(&b).collect();
                    let only_theirs: Vec<&Inst> = b.difference(a).collect();
                    for (act, _) in only_ours.iter().chain(&only_theirs) {
                        tally.by_act.entry(act.clone()).or_default().1 += 1;
                    }
                    tally.lines.push(format!("  {} (and {} more like it): enabled by norms.rs only: {}; by eflint-rs only: {}", whats[0], whats.len() - 1, show(&only_ours), show(&only_theirs)));
                }
                (a, b) => {
                    tally.errors += 1;
                    tally.lines.push(format!("  {}: norms.rs {}; eflint-rs {}", whats[0], a.as_ref().map(|s| format!("{} enabled", s.len())).unwrap_or_else(|e| e.clone()), b.map(|s| format!("{} enabled", s.len())).unwrap_or_else(|e| e.chars().take(300).collect())));
                }
            }
        }
    }
    writeln!(report, "compared {done} of the {} different situations in {:.0} s (budget {} s); norms.rs {:.3} s a situation", seen.len(), t0.elapsed().as_secs_f64(), budget.as_secs(), t_ours / done.max(1) as f64).unwrap();
    for ((label, _, _), t) in specs.iter().zip(&tallies) {
        writeln!(report, "\n[{label}] {} situations asked: {} the same, {} differ, {} with an error; eflint-rs {:.3} s a situation", t.asked, t.same, t.differ, t.errors, t.secs / t.asked.max(1) as f64).unwrap();
        for (act, (s, d)) in &t.by_act {
            writeln!(report, "  {act}: enabled instances in agreeing situations {s}, instances differing {d}").unwrap();
        }
        for l in t.lines.iter().take(40) {
            writeln!(report, "{l}").unwrap();
        }
        if t.lines.len() > 40 {
            writeln!(report, "  ... {} more", t.lines.len() - 40).unwrap();
        }
    }
    writeln!(report, "\nact instances asked (over the different situations): {questions}").unwrap();
    std::fs::write(out.join("table.txt"), &report).unwrap();
    println!("{report}");
}

fn show(xs: &[&Inst]) -> String {
    if xs.is_empty() {
        return "none".into();
    }
    let s: Vec<String> = xs.iter().take(6).map(|(a, v)| format!("{a}({})", v.join(", "))).collect();
    format!("{}{}", s.join(" "), if xs.len() > 6 { format!(" ... ({} in all)", xs.len()) } else { String::new() })
}

/// The reference's conditioned-by tests (eflint-rs `conformance/reference/tests/conditioned-by/`), and the same
/// tables written as acts in coordd's dialect, through both engines (paper-research N-568 p8, from consult-lead
/// N-566 p6: `Conditioned by` becomes the most used clause once `Only for` is `Extend Act pick-D Conditioned by`).
const CONDITIONED: &[(&str, &str, &[(&str, &[(&str, &[&str])], &[&str])])] = &[
    (
        "jackpot as an act (jackpot.eflint, jackpot2.eflint: Holds when input < 10, Conditioned by input > 5)",
        "Fact player Identified by String\nFact input Identified by Int\nAct jackpot\n  Actor player\n  Related to input\n  Holds when input < 10\n  Conditioned by input > 5\n",
        &[
            ("input 3", &[("player", &["P"]), ("input", &["3"])], &[]),
            ("input 6", &[("player", &["P"]), ("input", &["6"])], &["jackpot(P, 6)"]),
            ("input 10", &[("player", &["P"]), ("input", &["10"])], &[]),
        ],
    ),
    (
        "test1 as an act (int 0..5, Holds when even, Conditioned by > 0)",
        "Fact agent Identified by String\nFact num Identified by Int\nFact even Identified by num\nAct take\n  Actor agent\n  Related to num\n  Holds when even(num)\n  Conditioned by num > 0\n",
        &[("0..5", &[("agent", &["a"]), ("num", &["0"]), ("num", &["1"]), ("num", &["2"]), ("num", &["3"]), ("num", &["4"]), ("num", &["5"]), ("even", &["0"]), ("even", &["2"]), ("even", &["4"])], &["take(a, 2)", "take(a, 4)"])],
    ),
    (
        "two Conditioned by lines (norms.rs's departure 2: one condition atom per line)",
        "Fact agent Identified by String\nFact num Identified by Int\nFact even Identified by num\nAct take\n  Actor agent\n  Related to num\n  Holds when even(num)\n  Conditioned by num > 0\n  Conditioned by num < 4 and not even(3)\n",
        &[("0..5", &[("agent", &["a"]), ("num", &["0"]), ("num", &["1"]), ("num", &["2"]), ("num", &["3"]), ("num", &["4"]), ("num", &["5"]), ("even", &["0"]), ("even", &["2"]), ("even", &["4"])], &["take(a, 2)"])],
    ),
];

fn show_set(s: &BTreeSet<Inst>) -> String {
    let v: Vec<String> = s.iter().map(|(a, xs)| format!("{a}({})", xs.join(", "))).collect();
    if v.is_empty() { "none".into() } else { v.join(" ") }
}

/// `model-for` in the new form (one-interpreter.md §4.2): an act `pick-model-for` whose enabled instances are the
/// allowed options, each `Only for` an `Extend Act pick-model-for Conditioned by !c || e`, the call's fields facts
/// (`call-place`, ...), the preference a fact per option (`cost`, the price: `minimise price(option)`). Written by
/// hand from agent-base's prelude.eflint, decisions.eflint and teams.eflint (prod's line).
const PICK_MODEL_FOR: &str = "Fact call-place Identified by String.
Fact call-call Identified by String.
Fact call-task Identified by String.
Fact call-agent Identified by String.
Fact cost Identified by model1 * int2.
Act pick-model-for
  Actor agent
  Related to model
  Holds when (Exists price : price && price.model1 == model && price.int2 >= 0).
Extend Act pick-model-for Conditioned by Not(call-place(\"agent\")) || harness(model, \"claude\").
Extend Act pick-model-for Conditioned by Not(call-place(\"session\")) || (Exists harness : harness && harness.model1 == model).
Extend Act pick-model-for Conditioned by Not(Exists call-place : call-place && call-place != call-place(\"agent\")) || family(model, \"codex\").
Extend Act pick-model-for Conditioned by Not(call-call(\"form-head\")) || (Exists tier : tier && tier.model1 == model && tier.int2 >= 3).
Extend Act pick-model-for Conditioned by Not(call-call(\"form-parts\")) || (Exists tier : tier && tier.model1 == model && tier.int2 >= 3).
Extend Act pick-model-for Conditioned by Not(call-task(\"clean\")) || (Exists tier : tier && tier.model1 == model && tier.int2 >= 2).
Extend Act pick-model-for Conditioned by Not(replaced(model)).
";

/// ACTS_OUT, ACTS_BASE, ACTS_COORD (its record names the models the proxy served and the calls model-for was asked for).
#[test]
#[ignore]
fn conditioned_by_rows() {
    let (Ok(out), Ok(base), Ok(from)) = (std::env::var("ACTS_OUT"), std::env::var("ACTS_BASE"), std::env::var("ACTS_COORD")) else { return };
    let out = std::path::PathBuf::from(out);
    let mut report = String::new();
    // (1a) the reference's files, as the reference runs them (test mode: only failed queries and errors print)
    let dir = std::env::var("ACTS_CONFORMANCE").unwrap_or_else(|_| "/home/t/Documents/papers/eflint-rs/conformance/reference/tests/conditioned-by".into());
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).collect();
    files.sort();
    for f in &files {
        let mut printed: Vec<u8> = Vec::new();
        let read = eflint::session::Session::new(&mut printed, true).run_file(f);
        let text = std::fs::read_to_string(f).unwrap();
        let ours = crate::policy::syntax::parse(&f.display().to_string(), &text).map(|d| format!("parses ({} declarations)", d.len())).unwrap_or_else(|e| format!("refuses it: {}", e.lines().next().unwrap_or("")));
        writeln!(report, "{}: eflint-rs {}; norms.rs's parser {ours}", f.file_name().unwrap().to_string_lossy(), if read && printed.is_empty() { "every query as the file expects".to_string() } else { format!("printed: {}", String::from_utf8_lossy(&printed).trim()) }).unwrap();
    }
    // (1b) the same tables as acts, through both
    for (i, (what, policy, cases)) in CONDITIONED.iter().enumerate() {
        let d = out.join(format!("cond{i}"));
        std::fs::create_dir_all(d.join("policy")).unwrap();
        std::fs::write(d.join("policy/t.eflint"), policy).unwrap();
        let places = crate::policy::Places { base: d.display().to_string(), ..Default::default() };
        let n = norms::load(&places);
        let spec = to_eflint(&places, false);
        writeln!(report, "\n{what}").unwrap();
        for (case, facts, want) in cases.iter() {
            let facts: Facts = facts.iter().map(|(f, v)| (f.to_string(), v.iter().map(|s| s.to_string()).collect())).collect();
            let want: BTreeSet<Inst> = want.iter().map(|w| { let (a, r) = w.split_once('(').unwrap(); (a.to_string(), r.trim_end_matches(')').split(", ").map(String::from).collect()) }).collect();
            let a = n.as_ref().map_err(|e| e.clone()).and_then(|n| n.solve(&facts).map(|s| norms_enabled(n, &s)));
            let b = spec.as_ref().map_err(|e| e.clone()).and_then(|s| { let r = Replay::from_text(&s.text)?; eflint_enabled(&r, s, &facts) });
            let say = |x: &Result<BTreeSet<Inst>, String>| match x { Ok(s) if *s == want => format!("{} (as the table)", show_set(s)), Ok(s) => format!("{} (NOT as the table)", show_set(s)), Err(e) => format!("error: {}", e.chars().take(200).collect::<String>()) };
            writeln!(report, "  {case}: the table {}; norms.rs {}; eflint-rs {}", show_set(&want), say(&a), say(&b)).unwrap();
        }
    }
    // (2) model-for in the new form, against today's decision (clingo) on the same model facts and calls
    let places = crate::policy::Places { base: base.clone(), ..Default::default() };
    let parsed = crate::policy::syntax::parse("pick-model-for.eflint", PICK_MODEL_FOR).map(|d| format!("parses ({} declarations)", d.len())).unwrap_or_else(|e| format!("refuses it: {}", e.lines().next().unwrap_or("")));
    writeln!(report, "\nmodel-for in the new form: norms.rs's parser {parsed}").unwrap();
    let log = std::fs::read_to_string(format!("{from}/tickets/log.jsonl")).unwrap();
    let (mut served, mut calls): (BTreeSet<String>, BTreeMap<String, BTreeMap<String, String>>) = Default::default();
    for l in log.lines() {
        let Ok(v) = serde_json::from_str::<Value>(l) else { continue };
        if v["ev"] != "decided" || v["question"] != "model-for" {
            continue;
        }
        let p = &v["provenance"];
        for e in p["excluded"].as_array().into_iter().flatten() {
            served.extend(coord_types::excluded_options(e).into_iter().map(String::from));
        }
        served.extend(p["chosen"].as_str().map(String::from));
        if let Some(c) = p["call"].as_object() {
            let c: BTreeMap<String, String> = c.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect();
            calls.insert(serde_json::to_string(&c).unwrap(), c);
        }
    }
    // the calls the policy names that the record has not: the clean task, form-parts, an agent's model, a session's
    for extra in [r#"{"agent":"watcher","call":"clean","place":"call","task":"clean"}"#, r#"{"agent":"owner","call":"form-parts","place":"call","task":"form"}"#, r#"{"agent":"dev","call":"coord new","place":"agent"}"#, r#"{"agent":"dev","call":"net","place":"session"}"#] {
        let c: BTreeMap<String, String> = serde_json::from_str(extra).unwrap();
        calls.insert(extra.to_string(), c);
    }
    let provider = |m: &str| if m.starts_with("claude") { "anthropic" } else if m.starts_with("kimi") { "moonshot" } else { "openai" };
    let model_facts = crate::policy::facts::model_facts(&served.iter().map(|m| (m.clone(), provider(m).to_string())).collect::<Vec<_>>());
    let compiled = places.sources().and_then(|s| crate::policy::compile::compile(&s)).expect("the policy compiles");
    let mut solver = crate::policy::solve::Solver::new(compiled);
    solver.set_facts(model_facts.clone()).expect("clingo takes the model facts");
    // eflint-rs: the policy's facts and instances (its acts left out) and the new form
    let mut spec = to_eflint(&places, true).expect("the policy written out");
    let facts_only: String = spec.text.split("\nAct ").next().unwrap().to_string() + "\n";
    spec.text = facts_only + PICK_MODEL_FOR;
    spec.acts = BTreeMap::from([("pick-model-for".to_string(), vec![("actor".to_string(), "agent".to_string()), ("model".to_string(), "model".to_string())])]);
    spec.facts.extend([("call-place", "String"), ("call-call", "String"), ("call-task", "String"), ("call-agent", "String")].map(|(f, t)| (f.to_string(), vec![t.to_string()])));
    spec.facts.insert("cost".into(), vec!["model".into(), "Int".into()]);
    std::fs::write(out.join("pick-model-for.eflint"), &spec.text).unwrap();
    let replay = match Replay::from_text(&spec.text) {
        Ok(r) => r,
        Err(e) => {
            writeln!(report, "eflint-rs does not load the new form: {e}").unwrap();
            std::fs::write(out.join("conditioned.txt"), &report).unwrap();
            println!("{report}");
            return;
        }
    };
    let price: BTreeMap<String, i64> = model_facts.iter().filter(|(f, _)| f == "price").filter_map(|(_, a)| Some((a[0].as_str()?.to_string(), a[1].as_i64()?))).collect();
    let (mut same, mut differ) = (0, 0);
    for c in calls.values() {
        let inst = crate::policy::solve::Instance { kind: "model-for".into(), call: c.clone(), given: vec![], unmet: vec![] };
        let d = solver.open(&inst).expect("the decision opens");
        let o = solver.solve(&d, &inst);
        let _ = solver.close(&d);
        let o = match o {
            Ok(o) => o,
            Err(e) => {
                writeln!(report, "  {c:?}: clingo: {e}").unwrap();
                continue;
            }
        };
        let excluded: BTreeSet<String> = o.excluded.iter().map(|e| e.option.clone()).collect();
        let allowed: BTreeSet<String> = served.iter().filter(|m| !excluded.contains(*m)).cloned().collect();
        let mut facts: Facts = model_facts.iter().map(|(f, a)| (f.clone(), a.iter().map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string())).collect())).collect();
        facts.push(("agent".into(), vec!["coordd".into()]));
        for (k, v) in c {
            facts.push((format!("call-{k}"), vec![v.clone()]));
        }
        for (m, p) in &price {
            facts.push(("cost".into(), vec![m.clone(), p.to_string()]));
        }
        let theirs = eflint_enabled(&replay, &spec, &facts);
        let line = match theirs {
            Ok(set) => {
                let picked: BTreeSet<String> = set.iter().map(|(_, v)| v[1].clone()).collect();
                let low = picked.iter().filter_map(|m| price.get(m)).min();
                let best: Vec<&String> = picked.iter().filter(|m| price.get(*m) == low).collect();
                let ok = picked == allowed && o.chosen.as_ref().is_some_and(|ch| best.contains(&ch));
                if ok { same += 1 } else { differ += 1 }
                format!("{}: allowed {} by clingo, {} by eflint-rs{}; chosen {:?} by clingo, lowest cost {:?} among eflint-rs's{}", serde_json::to_string(c).unwrap(), allowed.len(), picked.len(), if picked == allowed { " (the same)" } else { "" }, o.chosen, best, if ok { String::new() } else { format!(" DIFFER: clingo only {:?}, eflint-rs only {:?}", allowed.difference(&picked).collect::<Vec<_>>(), picked.difference(&allowed).collect::<Vec<_>>()) }.as_str())
            }
            Err(e) => {
                differ += 1;
                format!("{c:?}: eflint-rs error: {}", e.chars().take(300).collect::<String>())
            }
        };
        writeln!(report, "  {line}").unwrap();
    }
    writeln!(report, "model-for, {} models served, {} calls: {same} the same, {differ} differ", served.len(), calls.len()).unwrap();
    std::fs::write(out.join("conditioned.txt"), &report).unwrap();
    println!("{report}");
}

/// What one question costs eflint-rs on a coord's facts (paper-research N-582.p17): loading the policy text, stating
/// the facts and deriving on a copy of the loaded state, and `Spec::answers` as coordd asks it (it loads each time);
/// norms.rs beside it. ACTS_COORD, ACTS_BASE, ACTS_OUT as above; ACTS_COST_N situations (default 20), spread evenly
/// over the different ones. Writes ACTS_OUT/cost.txt.
#[test]
#[ignore]
fn eflint_rs_cost_per_question() {
    let (Ok(from), Ok(base), Ok(out)) = (std::env::var("ACTS_COORD"), std::env::var("ACTS_BASE"), std::env::var("ACTS_OUT")) else { return };
    let out = std::path::PathBuf::from(out);
    let coord = out.join("coord");
    copy_coord(&from, &coord);
    let coord = coord.display().to_string();
    let started = std::time::Instant::now();
    let mut core = crate::core::Core::load(&coord, &base);
    core.project = crate::project::Project::load(&coord).unwrap();
    core.load_tickets();
    let places = core.places();
    core.set_models(models(&places));
    eprintln!("cost: the record replayed in {:.1} s", started.elapsed().as_secs_f64());
    let n = norms::load(&places).expect("norms.rs loads the policy");
    let spec = crate::policy::interp::load(&places).expect("eflint-rs loads the policy");
    let started = std::time::Instant::now();
    let sits = situations(&core);
    eprintln!("cost: {} situations made in {:.1} s", sits.len(), started.elapsed().as_secs_f64());
    let mut seen: BTreeSet<BTreeSet<(String, Vec<String>)>> = BTreeSet::new();
    for (_, f) in &sits {
        seen.insert(f.iter().filter(|(k, _)| n.facts.contains_key(k)).cloned().collect());
    }
    let want: usize = std::env::var("ACTS_COST_N").ok().and_then(|n| n.parse().ok()).unwrap_or(20);
    // the largest situations first (research's team, 293 facts, is the one the 692 s was measured on)
    let mut picked: Vec<Facts> = seen.iter().map(|s| s.iter().cloned().collect()).collect();
    picked.sort_by_key(|f: &Facts| std::cmp::Reverse(f.len()));
    picked.truncate(want);
    let t = std::time::Instant::now();
    let loaded = Replay::from_text(&spec.text).expect("loads");
    let first_load = t.elapsed().as_secs_f64();
    let mut cols: [Vec<f64>; 5] = Default::default();
    let mut report = String::new();
    writeln!(report, "coord {from}: {} situations, {} different (the table's size, ACTS_EVERY={}); policy loaded once in {:.3} s", sits.len(), seen.len(), std::env::var("ACTS_EVERY").unwrap_or("1".into()), first_load).unwrap();
    writeln!(report, "per question, s: load = Replay::from_text; copy = the loaded state cloned; state+derive = the facts stated and everything derived on the copy; answers = Spec::answers (load, state, derive, refused rows); norms.rs = solve").unwrap();
    writeln!(report, "facts\tload\tcopy\tstate+derive\tanswers\tnorms.rs").unwrap();
    // the old path (`apply`: every act derived after each fact's line) only with ACTS_COST_OLD=1: 692 s a situation
    let old = std::env::var("ACTS_COST_OLD").is_ok_and(|v| v == "1");
    for facts in &picked {
        let t = std::time::Instant::now();
        let _ = Replay::from_text(&spec.text);
        let load = t.elapsed().as_secs_f64();
        let t = std::time::Instant::now();
        let copy = Replay { cfg: loaded.cfg.clone() };
        let copied = t.elapsed().as_secs_f64();
        drop(copy);
        let t = std::time::Instant::now();
        if old {
            let _ = eflint_enabled(&loaded, &spec, facts);
        }
        let derive = if old { t.elapsed().as_secs_f64() - copied } else { f64::NAN };
        let t = std::time::Instant::now();
        let _ = spec.answers(facts, None);
        let answers = t.elapsed().as_secs_f64();
        // what the `both` comparison and a `verdict` ask: the acts asked only, no refused rows
        for act in ["start-subagent", "name", "esc"] {
            let t = std::time::Instant::now();
            let _ = spec.answers_for(facts, Some(&[act.to_string()]), None, false);
            let line = format!("{act} alone: {:.3} s", t.elapsed().as_secs_f64());
            eprintln!("cost: {line}");
            writeln!(report, "  {line}").unwrap();
        }
        let t = std::time::Instant::now();
        let _ = n.solve(facts);
        let ours = t.elapsed().as_secs_f64();
        writeln!(report, "{}\t{load:.3}\t{copied:.4}\t{derive:.3}\t{answers:.3}\t{ours:.3}", facts.len()).unwrap();
        eprintln!("cost: {} facts: load {load:.3} s, copy {copied:.4} s, state+derive {derive:.3} s, answers {answers:.3} s, norms.rs {ours:.3} s", facts.len());
        for (c, v) in cols.iter_mut().zip([load, copied, derive, answers, ours]) {
            c.push(v);
        }
    }
    for (name, c) in ["load", "copy", "state+derive", "answers", "norms.rs"].iter().zip(cols.iter_mut()) {
        c.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = c.get(c.len() / 2).copied().unwrap_or(0.0);
        writeln!(report, "{name}: median {:.0} ms, worst {:.0} ms, {} of {} over 50 ms", median * 1000.0, c.last().copied().unwrap_or(0.0) * 1000.0, c.iter().filter(|v| **v > 0.05).count(), c.len()).unwrap();
    }
    std::fs::write(out.join("cost.txt"), &report).unwrap();
    eprintln!("{report}");
}

/// Which acts cost eflint-rs the time (paper-research N-582.p34): in ACTS_COST_N situations (default 1; the largest
/// first), the facts stated with every act left underived (as `Replay::perform` steps), then each act of the policy
/// derived alone (`saturate_only` with what it reads), timed, with how many instances it enumerates (the product of
/// its fields' instances) and how many hold. One line per act as it ends, so a run cut short still says which.
#[test]
#[ignore]
fn eflint_rs_cost_per_act() {
    use eflint::eval::{lazy_types, needed_lazy, saturate_only};
    let (Ok(from), Ok(base), Ok(out)) = (std::env::var("ACTS_COORD"), std::env::var("ACTS_BASE"), std::env::var("ACTS_OUT")) else { return };
    let out = std::path::PathBuf::from(out);
    let coord = out.join("coord");
    copy_coord(&from, &coord);
    let coord = coord.display().to_string();
    let mut core = crate::core::Core::load(&coord, &base);
    core.project = crate::project::Project::load(&coord).unwrap();
    core.load_tickets();
    let places = core.places();
    core.set_models(models(&places));
    let n = norms::load(&places).expect("norms.rs loads the policy");
    let spec = crate::policy::interp::load(&places).expect("eflint-rs loads the policy");
    let mut seen: BTreeSet<BTreeSet<(String, Vec<String>)>> = BTreeSet::new();
    for (_, f) in situations(&core) {
        seen.insert(f.iter().filter(|(k, _)| n.facts.contains_key(k)).cloned().collect());
    }
    let mut by_size: Vec<Facts> = seen.into_iter().map(|s| s.into_iter().collect()).collect();
    by_size.sort_by_key(|f: &Facts| std::cmp::Reverse(f.len()));
    let want: usize = std::env::var("ACTS_COST_N").ok().and_then(|n| n.parse().ok()).unwrap_or(1);
    let loaded = Replay::from_text(&spec.text).expect("loads");
    let mut report = String::new();
    for facts in by_size.iter().take(want) {
        let mut r = Replay { cfg: loaded.cfg.clone() };
        let t = std::time::Instant::now();
        let a = r.perform(&Event::Phrase(facts_text(&spec, facts)));
        let line = format!("situation of {} facts: stated, acts underived, in {:.3} s ({} errors)", facts.len(), t.elapsed().as_secs_f64(), a.errors.len());
        eprintln!("cost: {line}");
        writeln!(report, "{line}").unwrap();
        let lazy = lazy_types(&r.cfg.spec);
        let mut of: BTreeMap<&str, usize> = BTreeMap::new();
        for (f, _) in facts {
            *of.entry(f.as_str()).or_default() += 1;
        }
        for (act, fields) in &spec.acts {
            let enumerated: usize = fields.iter().map(|(_, t)| of.get(t.as_str()).copied().unwrap_or(0)).product();
            let only = needed_lazy(&r.cfg.spec, &lazy, std::iter::once(eflint::ast::id(act)).collect());
            let t = std::time::Instant::now();
            let held = match saturate_only(&r.cfg.spec, r.cfg.state.clone(), &only) {
                Ok(s) => s.holding().filter(|te| &*te.dom == act.as_str()).count().to_string(),
                Err(e) => format!("error: {}", e.message()),
            };
            let line = format!("  {act}: {:.3} s, fields {}, {enumerated} combinations of stated instances, {held} hold", t.elapsed().as_secs_f64(), fields.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join(" × "));
            eprintln!("cost: {line}");
            writeln!(report, "{line}").unwrap();
        }
    }
    std::fs::write(out.join("cost-per-act.txt"), &report).unwrap();
}
