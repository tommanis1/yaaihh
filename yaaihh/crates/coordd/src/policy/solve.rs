//! Deciding (draft §4–§6, §8, §9): the compiled policy on one clingo control, held by a thread of its own (a control
//! cannot leave its thread). The model facts are its `base` part, grounded once and again only when they are stale
//! and no decision is open; each decision instance `dN` grounds `instance(dN)` and its own facts once, and is
//! switched on (the external `live(dN)`) only while it is solved, so instances decided at the same time do not mix.
//! A failed call adds the external `failed(dN, M)` and the instance is solved again (§5.4: the optimal order is the
//! fallback plan).

use super::compile::{Compiled, fact_line, quote};
pub use coord_types::{Cost, Exclusion, Given};
use clingo::{Control, Part, ShowType, SolveMode, SolverLiteral, Symbol, SymbolType, TruthValue};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

/// What one decision instance is about: its decision type, the call's fields (`call.task`, `call.agent`), the
/// caller's given fields.
#[derive(Clone, Debug, Serialize)]
pub struct Instance {
    pub kind: String,
    pub call: BTreeMap<String, String>,
    pub given: Vec<Given>,
}

/// The outcome of one solve of an instance.
#[derive(Clone, Debug, Serialize, Default)]
pub struct Outcome {
    pub chosen: Option<String>,
    /// Every optimal option (the judge's choice when more than one, §6).
    pub optimal: Vec<String>,
    pub excluded: Vec<Exclusion>,
    pub costs: Vec<Cost>,
    /// The best option other than the chosen one and its costs.
    pub runner_up: Option<(String, Vec<Cost>)>,
    /// The preference that decided between the chosen option and the runner-up: the highest level where they differ.
    pub decided_by: Option<Cost>,
    /// `no_route`: no option satisfies the hard rules (with the caller's given fields, the call faults, §14.4).
    pub fault: Option<String>,
}

/// The engine's state, owned by its thread.
pub struct Solver {
    pub compiled: Compiled,
    ctl: Option<Control>,
    facts: Vec<(String, Vec<Value>)>,
    next: usize,
    open: usize,
}

fn err(e: clingo::ClingoError) -> String {
    format!("clingo: {e}")
}

impl Solver {
    pub fn new(compiled: Compiled) -> Solver {
        Solver { compiled, ctl: None, facts: Vec::new(), next: 0, open: 0 }
    }

    /// Whether a new control may replace the current one (no instance is open on it).
    pub fn may_refresh(&self) -> bool {
        self.open == 0
    }

    /// Ground FACTS as the base part of a new control.
    /// The value of the system fact FACT for FIRST (`harness("gpt-6-luna")` is `codex`): its second argument.
    pub fn value(&self, fact: &str, first: &str) -> Option<String> {
        self.facts.iter().find(|(f, a)| f == fact && a.first().and_then(Value::as_str) == Some(first)).and_then(|(_, a)| a.get(1)?.as_str().map(String::from))
    }

    pub fn set_facts(&mut self, facts: Vec<(String, Vec<Value>)>) -> Result<(), String> {
        for (f, args) in &facts {
            let ids = self.compiled.facts.get(f).ok_or(format!("the system supplies `{f}` but the prelude declares no such fact type"))?;
            if ids.len() != args.len() {
                return Err(format!("the system's `{f}` facts have {} arguments; the prelude declares {}", args.len(), ids.len()));
            }
        }
        let mut ctl = clingo::control(vec!["--opt-mode=optN".into(), "0".into()]).map_err(err)?;
        let base: String = facts.iter().map(|(f, a)| fact_line(f, a) + "\n").collect();
        ctl.add("base", &[], &base).map_err(err)?;
        ctl.add("instance", &["d"], &self.compiled.instance).map_err(err)?;
        ctl.ground(&[Part::new("base", vec![]).map_err(err)?]).map_err(err)?;
        self.ctl = Some(ctl);
        self.facts = facts;
        Ok(())
    }

    /// Open an instance: its id.
    pub fn open(&mut self, inst: &Instance) -> Result<String, String> {
        if !self.compiled.decisions.contains_key(&inst.kind) {
            return Err(format!("no decision type `{}`", inst.kind));
        }
        let ctl = self.ctl.as_mut().ok_or("the policy has no facts yet")?;
        self.next += 1;
        let d = format!("d{}", self.next);
        let mut facts = format!("kind({d},{}).\n", quote(&inst.kind));
        for (k, v) in &inst.call {
            // a whole number is a number, so `call.nudged >= 3` compares numbers
            let term = if v.parse::<i64>().is_ok() { v.clone() } else { quote(v) };
            facts += &format!("call_field({d},{},{term}).\n", quote(k));
        }
        for g in &inst.given {
            facts += &format!("given({d},{},{}).\n", quote(&g.field), quote(&g.value));
        }
        let part = format!("facts_{d}");
        ctl.add(&part, &[], &facts).map_err(err)?;
        let id = Symbol::create_id(&d, true).map_err(err)?;
        ctl.ground(&[Part::new("instance", vec![id]).map_err(err)?, Part::new(&part, vec![]).map_err(err)?]).map_err(err)?;
        self.open += 1;
        Ok(d)
    }

    /// The solver literal of ATOM (as clingo prints it), if grounded.
    fn literal(ctl: &Control, name: &str, arity: u32, atom: &str) -> Result<Option<SolverLiteral>, String> {
        let sig = clingo::Signature::new(name, arity, true).map_err(err)?;
        for a in ctl.symbolic_atoms().map_err(err)?.iter_with_signature(sig).map_err(err)? {
            if a.symbol().map_err(err)?.to_string() == atom {
                return Ok(Some(a.literal().map_err(err)?));
            }
        }
        Ok(None)
    }

    fn set_external(&mut self, name: &str, arity: u32, atom: &str, value: TruthValue) -> Result<(), String> {
        let ctl = self.ctl.as_mut().ok_or("no control")?;
        match Self::literal(ctl, name, arity, atom)? {
            Some(l) => ctl.assign_external(l, value).map_err(err),
            None => Ok(()), // not grounded: an option that does not exist
        }
    }

    /// Exclude option M of D from now on (its call failed).
    pub fn fail(&mut self, d: &str, m: &str) -> Result<(), String> {
        self.set_external("failed", 2, &format!("failed({d},{})", quote(m)), TruthValue::True)
    }

    /// Close D: it is never solved again.
    pub fn close(&mut self, d: &str) -> Result<(), String> {
        self.open = self.open.saturating_sub(1);
        let ctl = self.ctl.as_mut().ok_or("no control")?;
        if let Some(l) = Self::literal(ctl, "live", 1, &format!("live({d})"))? {
            ctl.release_external(l).map_err(err)?;
        }
        Ok(())
    }

    /// The optimal models of D's instance (switched on for this solve), with ASSUMPTIONS: each model's shown atoms.
    fn optimal(&mut self, d: &str, assumptions: &[SolverLiteral]) -> Result<Vec<Vec<Symbol>>, String> {
        self.set_external("live", 1, &format!("live({d})"), TruthValue::True)?;
        let ctl = self.ctl.take().ok_or("no control")?;
        let mut h = ctl.solve(SolveMode::YIELD, assumptions).map_err(err)?;
        let mut out = Vec::new();
        let result = (|| -> Result<(), String> {
            while let Some(m) = h.model().map_err(err)? {
                // a model with nothing to optimise (no preference applies to this instance and none was grounded
                // before) is optimal as it is: clingo proves optimality only of an optimisation
                if m.cost().map_err(err)?.is_empty() || m.optimality_proven().map_err(err)? {
                    out.push(m.symbols(ShowType::SHOWN).map_err(err)?);
                }
                h.resume().map_err(err)?;
            }
            Ok(())
        })();
        self.ctl = Some(h.close().map_err(err)?);
        self.set_external("live", 1, &format!("live({d})"), TruthValue::False)?;
        result.map(|_| out)
    }

    /// Solve D: the chosen option and its provenance.
    pub fn solve(&mut self, d: &str, inst: &Instance) -> Result<Outcome, String> {
        let models = self.optimal(d, &[])?;
        let Some(first) = models.first() else {
            // unsatisfiable: no option meets the hard rules; the reasons are read from the instance switched off
            let excluded = self.exclusions_unchosen(d, inst)?;
            let fault = if inst.given.is_empty() { "no option satisfies the hard rules" } else { "no_route: no option has the fields the caller gave" };
            return Ok(Outcome { excluded, fault: Some(fault.into()), ..Default::default() });
        };
        let mut optimal: Vec<String> = models.iter().filter_map(|m| chosen(m, d)).collect();
        optimal.sort();
        optimal.dedup();
        // ties without a judge: the first by name, recorded as such (§6; the judge is not run in this slice)
        let winner = optimal[0].clone();
        let model = models.iter().find(|m| chosen(m, d).as_deref() == Some(winner.as_str())).unwrap_or(first);
        let excluded = self.exclusions(model, d, inst);
        let costs = self.costs(model, d, &inst.kind);
        // the runner-up: the best model with the winner excluded
        let runner_up = {
            let ctl = self.ctl.as_ref().ok_or("no control")?;
            match Self::literal(ctl, "chosen", 2, &format!("chosen({d},{})", quote(&winner)))? {
                Some(l) => self.optimal(d, &[l.negate()])?.first().and_then(|m| Some((chosen(m, d)?, self.costs(m, d, &inst.kind)))),
                None => None,
            }
        };
        let decided_by = runner_up.as_ref().and_then(|(_, rc)| {
            let mut levels: Vec<i64> = costs.iter().chain(rc).map(|c| c.level).collect();
            levels.sort_unstable_by(|a, b| b.cmp(a));
            levels.dedup();
            let at = |cs: &[Cost], l: i64| cs.iter().filter(|c| c.level == l).map(|c| c.cost).sum::<i64>();
            let l = levels.into_iter().find(|&l| at(&costs, l) != at(rc, l))?;
            let rule = self.compiled.rules.iter().find(|r| r.level == Some(l))?;
            Some(Cost { rule: format!("{} ({})", rule.text.replace('\n', " "), rule.at), level: l, cost: at(rc, l) - at(&costs, l) })
        });
        Ok(Outcome { chosen: Some(winner), optimal, excluded, costs, runner_up, decided_by, fault: None })
    }

    /// The exclusions read off MODEL for D.
    fn exclusions(&self, model: &[Symbol], d: &str, inst: &Instance) -> Vec<Exclusion> {
        let mut out: Vec<Exclusion> = model.iter().filter_map(|s| self.exclusion(*s, d, inst)).collect();
        out.sort_by(|a, b| (&a.option, &a.reason).cmp(&(&b.option, &b.reason)));
        out
    }

    /// When the instance has no model: every option's reasons, from a solve with the instance switched off (its
    /// `violates/3` atoms do not depend on the choice).
    fn exclusions_unchosen(&mut self, d: &str, inst: &Instance) -> Result<Vec<Exclusion>, String> {
        let ctl = self.ctl.take().ok_or("no control")?;
        let mut h = ctl.solve(SolveMode::YIELD, &[]).map_err(err)?;
        let syms = match h.model() {
            Ok(Some(m)) => m.symbols(ShowType::SHOWN).unwrap_or_default(),
            _ => Vec::new(),
        };
        self.ctl = Some(h.close().map_err(err)?);
        Ok(self.exclusions(&syms, d, inst))
    }

    fn exclusion(&self, s: Symbol, d: &str, inst: &Instance) -> Option<Exclusion> {
        if s.name().ok()? != "violates" {
            return None;
        }
        let a = s.arguments().ok()?;
        if a[0].to_string() != d {
            return None;
        }
        let option = a[1].string().ok()?.to_string();
        let r = a[2];
        let (reason, rule) = match r.name().unwrap_or("") {
            "failed" => ("failed".to_string(), "the call failed on this model".to_string()),
            "given" => {
                let f = r.arguments().ok()?[0].string().ok()?.to_string();
                let g = inst.given.iter().find(|g| g.field == f);
                ("given".into(), g.map_or(f.clone(), |g| format!("{} = {} (given by {} at {})", g.field, g.value, g.by, g.at)))
            }
            "r" => {
                let n = r.arguments().ok()?[0].number().ok()? as usize;
                let q = self.compiled.requirements.iter().find(|q| q.id == n)?;
                ("requires".into(), format!("{} ({})", q.text, q.at))
            }
            "only" => {
                let n = r.arguments().ok()?[0].arguments().ok()?[0].number().ok()? as usize;
                let q = self.compiled.rules.iter().find(|q| q.id == n)?;
                ("only".into(), format!("{} ({})", q.text.replace('\n', " "), q.at))
            }
            other => (other.to_string(), r.to_string()),
        };
        Some(Exclusion { option, reason, rule })
    }

    /// The cost per preference (of decision type KIND) of the option chosen in MODEL for D, highest level first.
    fn costs(&self, model: &[Symbol], d: &str, kind: &str) -> Vec<Cost> {
        let mut by: HashMap<usize, i64> = HashMap::new();
        for s in model {
            if s.name().ok() != Some("pen") {
                continue;
            }
            let Ok(a) = s.arguments() else { continue };
            if a[1].to_string() != d {
                continue;
            }
            if let (Ok(p), Ok(w)) = (a[0].arguments(), a[3].number()) {
                if let Ok(n) = p[0].number() {
                    *by.entry(n as usize).or_default() += w as i64;
                }
            }
        }
        let mut out: Vec<Cost> = self
            .compiled
            .rules
            .iter()
            .filter(|r| r.decision == kind)
            .filter_map(|r| Some(Cost { rule: format!("{} ({})", r.text.replace('\n', " "), r.at), level: r.level?, cost: *by.get(&r.id).unwrap_or(&0) }))
            .collect();
        out.sort_by(|a, b| b.level.cmp(&a.level));
        out
    }
}

/// The option MODEL chose for D.
fn chosen(model: &[Symbol], d: &str) -> Option<String> {
    model.iter().find_map(|s| {
        if s.name().ok()? != "chosen" {
            return None;
        }
        let a = s.arguments().ok()?;
        (a[0].to_string() == d && a[1].symbol_type().ok()? == SymbolType::String).then(|| a[1].string().ok().map(String::from)).flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::super::compile::{Scope, compile};
    use super::super::syntax::parse;
    use super::*;
    use serde_json::json;

    const PRELUDE: &str = "Fact model Identified by String\nFact family Identified by model * String\nFact price Identified by model * Int\nFact quota-left Identified by model * Int\nDecision model-for\n  Options model\n  Requires price(option) >= 0\n  Requires quota-left(option) > 0\n";
    const PROJECT: &str = "Prefer for model-for When call.task == \"recognise-question\":\n  family(option, \"codex\") > family(option, \"claude\")\nPrefer for model-for:\n  minimise price(option)\n";

    /// Example (a) of the draft (§10(a), checked by hand in §13): astra out of quota, luna the cheapest Codex model.
    fn solver() -> Solver {
        let c = compile(&[(Scope::Global, parse("prelude", PRELUDE).unwrap()), (Scope::Project, parse("decisions.eflint", PROJECT).unwrap())]).unwrap();
        let mut s = Solver::new(c);
        let mut facts = Vec::new();
        for (m, fam, q, p) in [("astra", "codex", 0, 1250), ("luna", "codex", 40, 1250), ("haiku", "claude", 90, 800), ("sonnet", "claude", 90, 3000)] {
            facts.push(("model".to_string(), vec![json!(m)]));
            facts.push(("family".to_string(), vec![json!(m), json!(fam)]));
            facts.push(("quota-left".to_string(), vec![json!(m), json!(q)]));
            facts.push(("price".to_string(), vec![json!(m), json!(p)]));
        }
        s.set_facts(facts).unwrap();
        s
    }

    fn recognise(given: Vec<Given>) -> Instance {
        Instance { kind: "model-for".into(), call: [("task".to_string(), "recognise-question".to_string())].into(), given }
    }

    #[test]
    fn codex_first_then_claude_after_a_failure_with_the_reasons() {
        let mut s = solver();
        let inst = recognise(vec![]);
        let d = s.open(&inst).unwrap();
        let o = s.solve(&d, &inst).unwrap();
        assert_eq!(o.chosen.as_deref(), Some("luna"));
        assert_eq!(o.excluded, vec![Exclusion { option: "astra".into(), reason: "requires".into(), rule: "Requires quota-left(option) > 0 (prelude:8)".into() }]);
        // the runner-up is astra-less codex-less: haiku, beaten at the codex-first preference's level
        assert_eq!(o.runner_up.as_ref().map(|r| r.0.as_str()), Some("haiku"));
        assert_eq!(o.decided_by.as_ref().map(|c| c.level), Some(2000));
        // luna's call failed: haiku next (level 2000 ties the Claude models, the price decides)
        s.fail(&d, "luna").unwrap();
        let o = s.solve(&d, &inst).unwrap();
        assert_eq!(o.chosen.as_deref(), Some("haiku"));
        assert!(o.excluded.iter().any(|e| e.option == "luna" && e.reason == "failed"));
        assert_eq!(o.decided_by.as_ref().map(|c| c.level), Some(1999));
        s.close(&d).unwrap();
    }

    #[test]
    fn a_given_field_is_kept_or_the_call_faults() {
        let mut s = solver();
        let g = |f: &str, v: &str| Given { field: f.into(), value: v.into(), by: "owner".into(), at: "2026-11-08T16:00".into() };
        let inst = recognise(vec![g("family", "claude")]);
        let d = s.open(&inst).unwrap();
        let o = s.solve(&d, &inst).unwrap();
        assert_eq!(o.chosen.as_deref(), Some("haiku"), "{o:?}");
        assert!(o.excluded.iter().any(|e| e.option == "luna" && e.reason == "given" && e.rule.contains("given by owner")));
        s.close(&d).unwrap();
        // an instance opened after a closed one is decided alone
        let inst = recognise(vec![g("model", "astra")]);
        let d = s.open(&inst).unwrap();
        let o = s.solve(&d, &inst).unwrap();
        assert_eq!(o.chosen, None);
        assert!(o.fault.as_deref().unwrap().starts_with("no_route"), "{o:?}");
        assert!(o.excluded.iter().any(|e| e.option == "astra" && e.reason == "requires"), "{o:?}");
    }

    /// The clerk's decisions (agent-base/policy/clerk.eflint): a turn that names its next step is nudged, until three
    /// nudges in a row (then the owner is told); a short reply gets no spell check.
    #[test]
    fn the_clerks_decisions() {
        let base = format!("{}/../../../agent-base/policy", env!("CARGO_MANIFEST_DIR"));
        let read = |f: &str| parse(f, &std::fs::read_to_string(format!("{base}/{f}")).unwrap()).unwrap();
        let mut decls = read("prelude.eflint");
        decls.extend(read("clerk.eflint"));
        let mut s = Solver::new(compile(&[(Scope::Global, decls)]).unwrap());
        s.set_facts(Vec::new()).unwrap();
        let mut decide = |kind: &str, fields: &[(&str, &str)]| {
            let inst = Instance { kind: kind.into(), call: fields.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), given: vec![] };
            let d = s.open(&inst).unwrap();
            let o = s.solve(&d, &inst).unwrap();
            s.close(&d).unwrap();
            (o.chosen, o.excluded)
        };
        // the first decision of a fresh solver, which no preference ranks: still decided (the hard rules leave accept)
        assert_eq!(decide("end-of-turn", &[("state", "Waiting"), ("nudged", "0"), ("said", "913")]).0.as_deref(), Some("accept"));
        assert_eq!(decide("end-of-turn", &[("state", "Next"), ("nudged", "0"), ("said", "0")]).0.as_deref(), Some("accept"));
        assert_eq!(decide("end-of-turn", &[("state", "Next"), ("nudged", "0"), ("said", "40")]).0.as_deref(), Some("nudge"));
        assert_eq!(decide("end-of-turn", &[("state", "Next"), ("nudged", "0")]).0.as_deref(), Some("nudge"));
        let (chosen, excluded) = decide("end-of-turn", &[("state", "Next"), ("nudged", "3")]);
        assert_eq!(chosen.as_deref(), Some("escalate"));
        assert!(excluded.iter().any(|e| e.option == "nudge" && e.reason == "only"), "{excluded:?}");
        assert_eq!(decide("end-of-turn", &[("state", "Done"), ("nudged", "0")]).0.as_deref(), Some("accept"));
        assert_eq!(decide("end-of-turn", &[("state", "Blocked"), ("nudged", "5")]).0.as_deref(), Some("accept"));
        assert_eq!(decide("needs-spellcheck", &[("length", "3")]).0.as_deref(), Some("no"));
        assert_eq!(decide("needs-spellcheck", &[("length", "140")]).0.as_deref(), Some("yes"));
    }
}
