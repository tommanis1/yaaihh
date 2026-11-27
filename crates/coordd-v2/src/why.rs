//! docket-v2's justifications (run::explain) as JSON, the wire's form: an `explain` reply and a refused act's `why`.

use crate::codec::of_value;
use docket_v2::run::explain::{Done, Explain, Holds, Link, Premise, Rule, WhyFailed};
use docket_v2::run::value::Env;
use serde_json::{json, Map, Value as Json};

fn bindings(env: &Env) -> Json {
    Json::Object(env.iter().map(|(k, v)| (k.clone(), of_value(v))).collect::<Map<_, _>>())
}

fn done(d: &Done) -> Json {
    json!({"step": d.step, "act": d.act.as_ref().map(|a| a.to_string())})
}

fn rule(r: &Rule) -> Json {
    let mut o = json!({"at": r.at.to_string(), "rule": r.text, "bindings": bindings(&r.env), "premises": r.premises.iter().map(premise).collect::<Vec<_>>()});
    if r.cut {
        o["cut"] = json!(true);
    }
    o
}

fn holds(h: &Holds) -> Json {
    match h {
        Holds::Stated(d) => json!({"stated": done(d)}),
        Holds::Postulated { at } => json!({"postulated": at.to_string()}),
        Holds::Rule(r) => json!({"rule": rule(r)}),
        Holds::Unproved { cut: true, .. } => json!({"unproved": "no proof found before the explanation's budget ran out", "cut": true}),
        Holds::Unproved { tries, .. } => json!({"unproved": format!("no proof found in {tries} bindings per rule that does not stand on itself")}),
    }
}

fn premise(p: &Premise) -> Json {
    let mut o = json!({"at": p.at.to_string(), "text": p.text, "bindings": bindings(&p.env), "holds": p.holds});
    if p.cut {
        o["cut"] = json!(true);
    }
    if !p.parts.is_empty() {
        o["parts"] = p.parts.iter().map(premise).collect();
    }
    if !p.under.is_empty() {
        o["facts"] = p.under.iter().map(explain).collect();
    }
    o
}

fn link(l: &Link) -> Json {
    json!({"inst": l.inst.to_string(), "at": l.at.to_string(), "rule": l.text, "not": l.negated})
}

/// Why an instance holds (`holds` true), does not (false), or has no single answer (null), whole.
pub fn explain(e: &Explain) -> Json {
    let mut o = json!({"inst": e.inst().to_string(), "holds": e.holds()});
    match e {
        Explain::True { by, .. } => o["by"] = holds(by),
        Explain::False { ended, overridden, rules, .. } => {
            if let Some(d) = ended {
                o["ended"] = done(d);
            }
            if let Some(h) = overridden {
                o["overridden"] = holds(h);
            }
            o["rules"] = rules.iter().map(rule).collect();
        }
        Explain::OnPath { .. } => o["on_path"] = json!(true),
        Explain::Unknown { cycle, .. } => o["cycle"] = cycle.iter().map(link).collect(),
    }
    o
}

/// Why an Act or Event is not enabled: its failing conditions and the syncs that are not enabled; both empty when it is.
/// A synced act explained above is `"again": true`; one the budget stopped is `"cut": true`.
pub fn refused(w: &WhyFailed) -> Json {
    let mut o = json!({"act": w.act.to_string(), "failing": w.failing.iter().map(premise).collect::<Vec<_>>(), "syncs": w.syncs.iter().map(refused).collect::<Vec<_>>()});
    for (k, on) in [("again", w.again), ("cut", w.cut)] {
        if on {
            o[k] = json!(true);
        }
    }
    o
}
