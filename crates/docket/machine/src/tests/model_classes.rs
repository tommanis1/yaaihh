//! The model classes (paper-research N-101): a call's `where` over `model`, read with the calls; the orders `<`, `<=`,
//! `>`, `>=` beside `==` and `!=`; a step refused to an agent whose model does not meet its call (coordd's
//! `ModelRefused`) raises NoWorker with the model and the condition; the record replays to the same.

use crate::*;
use serde_json::json;
use std::collections::BTreeMap;

const CALLS: &str = "\
call design(words: Text) where model >= medium -> Designed(how: Text)
  template
    Design {{ words }}.

call decomp(words: Text)
    where model.class == small and model.family == claude
    -> Parts(parts: [Text])
  template
    Split {{ words }}.

call plain(words: Text) -> Done
  template
    Do {{ words }}.
";

const KINDS: &str = "\
kind change
  letter C
  form words: Text
  result Designed(how: Text) | NotMade(why: Text)
  pattern
    try { step ann design(words) } handle { NoWorker(e) => return NotMade(e.why) }

kind count
  letter K
  form n: Int
  result Text
  pattern
    match n {
      k where k >= 3 => return \"many\"
      k where k < 3 and k > 0 => return \"few\"
      _ => return \"none\"
    }
";

fn calls() -> (BTreeMap<String, CallSig>, syntax::Named) {
    let mut named = BTreeMap::new();
    let calls = syntax::parse_calls("m.call", CALLS, &mut named).unwrap().into_iter().map(|c| (c.name.clone(), c)).collect();
    (calls, named)
}

fn machine() -> Machine {
    let (calls, named) = calls();
    let (d, errors) = Defs::build(&[("m.kind".to_string(), KINDS.to_string())], calls, named, 300);
    assert!(errors.is_empty(), "{errors:#?}");
    Machine::load(d)
}

fn open(m: &mut Machine, log: &mut Vec<Line>, kind: &str, form: serde_json::Value) -> u64 {
    let out = m.apply(Input::Open { kind: kind.into(), under: None, opener: "owner".into(), form, text: None, names: vec!["ann".into()], told: vec![] }, "2026-11-16T10:00:01Z".into());
    assert!(out.reply.is_ok(), "{:?}", out.reply);
    log.extend(out.lines);
    *m.tickets.keys().max().unwrap()
}

#[test]
fn a_call_carries_its_where_as_written() {
    let (calls, _) = calls();
    assert_eq!(calls["design"].cond.as_deref(), Some("model >= medium"));
    assert_eq!(calls["decomp"].cond.as_deref(), Some("model.class == small and model.family == claude"));
    assert_eq!(calls["plain"].cond, None);
    // its answer read as before
    assert_eq!(calls["decomp"].head.to_string(), "Parts(parts: [Text])");
    // read back into the one expression coordd holds models to
    let e = syntax::model_cond("model >= medium", 1).unwrap();
    assert_eq!(print::expr(&e), "model >= medium");
    // what a `where` may not say, each refused with the call's place
    for (w, said) in [
        ("model", "is no comparison"),
        ("model.size == 2", "`model.size`"),
        // `or` reads in a handler's `where` (N-101 p392), and a call's `where` still joins by `and` only
        ("model >= medium or model == x", "is no comparison"),
        ("result of model == 2", "no model, class"),
    ] {
        let text = format!("call c(x: Text) where {w} -> Done\n  template\n    x\n");
        let e = syntax::parse_calls("w.call", &text, &mut BTreeMap::new()).unwrap_err();
        assert!(e.starts_with("w.call:1:") && e.contains(said), "{w}: {e}");
    }
}

#[test]
fn numbers_are_ordered_in_a_where() {
    let mut m = machine();
    let mut log = Vec::new();
    for (n, want) in [(5, "many"), (3, "many"), (2, "few"), (0, "none")] {
        let t = open(&mut m, &mut log, "count", json!({"n": n}));
        assert_eq!(m.tickets[&t].status, Status::Done(json!(want)), "{n}");
    }
    // an order of what is no number is refused when the kind is checked
    let (calls, named) = calls();
    let bad = "kind odd\n  form w: Text\n  result Text\n  pattern\n    match w {\n      k where k > 2 => return k\n      _ => return w\n    }\n";
    let (_, errors) = Defs::build(&[("o.kind".to_string(), bad.to_string())], calls, named, 300);
    assert!(errors.iter().any(|e| e.contains("`>` orders numbers")), "{errors:#?}");
}

#[test]
fn a_step_refused_for_its_agents_model_raises_no_worker_and_replays() {
    let mut m = machine();
    let mut log = Vec::new();
    let t = open(&mut m, &mut log, "change", json!({"words": "the cache"}));
    let why = "ann runs claude-haiku-5-5 (1, small), and call design needs `where model >= medium`";
    // not the agent owed: refused, nothing written
    let out = m.apply(Input::ModelRefused { t, step: 1, agent: "bob".into(), why: why.into() }, "2026-11-16T10:00:02Z".into());
    assert!(out.reply.is_err() && out.lines.is_empty(), "{:?}", out.reply);
    let out = m.apply(Input::ModelRefused { t, step: 1, agent: "ann".into(), why: why.into() }, "2026-11-16T10:00:03Z".into());
    assert!(out.reply.as_ref().is_ok_and(|r| r.contains("raises NoWorker")), "{:?}", out.reply);
    assert!(out.lines.iter().any(|l| matches!(l, Line::ModelRefused { agent, .. } if agent == "ann")), "{:?}", out.lines);
    log.extend(out.lines);
    // the handler has the model and the condition
    assert_eq!(m.tickets[&t].status, Status::Done(json!({"head": "NotMade", "why": why})));
    // the line as the record has it, and the record replayed to the same
    let line = log.iter().find(|l| matches!(l, Line::ModelRefused { .. })).unwrap();
    let s = serde_json::to_string(line).unwrap();
    assert!(s.starts_with("{\"ev\":\"model_refused\"") && s.contains("\"agent\":\"ann\""), "{s}");
    let mut m2 = machine();
    assert_eq!(m2.replay(&log), Vec::<String>::new());
    assert_eq!(m2.tickets[&t].status, m.tickets[&t].status);
    // kept on the step, as the picture shows it (docket-edge N-235), and so after a replay
    fn refused(ns: &[RunNode]) -> Option<ViewRefused> {
        ns.iter().find_map(|n| n.refused.clone().or_else(|| refused(&n.nodes)))
    }
    let r = refused(&m.view(t).unwrap().run);
    assert_eq!(r, Some(ViewRefused { agent: "ann".into(), why: why.into(), at: "2026-11-16T10:00:03Z".into() }));
    assert_eq!(refused(&m2.view(t).unwrap().run), r);
}
