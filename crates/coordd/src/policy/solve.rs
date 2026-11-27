//! Deciding (draft §4–§6, §8, §9): the compiled policy on one clingo control, held by a thread of its own (a control
//! cannot leave its thread). The model facts are its `base` part, grounded once and again only when they are stale
//! and no decision is open; each decision instance `dN` grounds `instance(dN)` and its own facts once, and is
//! switched on (the external `live(dN)`) only while it is solved, so instances decided at the same time do not mix.
//! A failed call adds the external `failed(dN, M)` and the instance is solved again (§5.4: the optimal order is the
//! fallback plan).

use super::compile::{Compiled, fact_line, pred, quote};
pub use coord_types::{Cost, Exclusion, Given, Grounds};
use clingo::{Control, Part, ShowType, SolveMode, SolverLiteral, Symbol, SymbolType, TruthValue};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

/// What one decision instance is about: its decision type, the call's fields (`call.task`, `call.agent`), the
/// caller's given fields.
#[derive(Clone, Debug, Serialize)]
pub struct Instance {
    pub kind: String,
    pub call: BTreeMap<String, String>,
    pub given: Vec<Given>,
    /// The options a call's `where` leaves out (models.rs: the model classes), excluded as `where`, the condition
    /// being the call field `where`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unmet: Vec<String>,
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
    /// What made each of its rules hold or not, when asked ([`Solver::explain`]; proto-mind N-1446 p1502 (9)).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub grounds: Vec<Grounds>,
}

/// Whether the policy declares each supplied fact of FACTS with its number of arguments (TYPES: compile.rs's
/// `facts`); the picker checks the same before it reads them.
pub fn supplied(types: &BTreeMap<String, Vec<String>>, facts: &[(String, Vec<Value>)]) -> Result<(), String> {
    for (f, args) in facts {
        let ids = types.get(f).ok_or(format!("the system supplies `{f}` but the prelude declares no such fact type"))?;
        if ids.len() != args.len() {
            return Err(format!("the system's `{f}` facts have {} arguments; the prelude declares {}", args.len(), ids.len()));
        }
    }
    Ok(())
}

/// Its control is freed under the clingo lock too.
impl Drop for Solver {
    fn drop(&mut self) {
        let _clingo = super::clingo();
        self.ctl = None;
    }
}

/// The engine's state, owned by its thread.
pub struct Solver {
    pub compiled: Compiled,
    ctl: Option<Control>,
    facts: Vec<(String, Vec<Value>)>,
    next: usize,
    open: usize,
    /// Whether the policy's `why` parts are on the control ([`Solver::explain`]).
    why: bool,
}

fn err(e: clingo::ClingoError) -> String {
    format!("clingo: {e}")
}

impl Solver {
    pub fn new(compiled: Compiled) -> Solver {
        Solver { compiled, ctl: None, facts: Vec::new(), next: 0, open: 0, why: false }
    }

    /// Whether a new control may replace the current one (no instance is open on it).
    pub fn may_refresh(&self) -> bool {
        self.open == 0
    }

    pub fn set_facts(&mut self, facts: Vec<(String, Vec<Value>)>) -> Result<(), String> {
        let _clingo = super::clingo();
        supplied(&self.compiled.facts, &facts)?;
        let mut ctl = clingo::control(vec!["--opt-mode=optN".into(), "0".into()]).map_err(err)?;
        // the supplied facts, then the policy's own instances and derived ones (compile.rs `base`)
        let base: String = facts.iter().map(|(f, a)| fact_line(f, a) + "\n").collect::<String>() + &self.compiled.base;
        ctl.add("base", &[], &base).map_err(err)?;
        ctl.add("instance", &["d"], &self.compiled.instance).map_err(err)?;
        ctl.ground(&[Part::new("base", vec![]).map_err(err)?]).map_err(err)?;
        self.ctl = Some(ctl);
        self.facts = facts;
        Ok(())
    }

    /// Every atom of fact FACT that holds with the supplied facts, the policy's own (`+FACT(...)`) and those it derives
    /// (`Derived from`), no decision live: each one's arguments as text.
    pub fn holding(&mut self, fact: &str) -> Result<Vec<Vec<String>>, String> {
        let _clingo = super::clingo();
        let ctl = self.ctl.as_mut().ok_or("the policy has no facts yet")?;
        self.next += 1;
        let id = Symbol::create_id(&format!("d{}", self.next), true).map_err(err)?;
        ctl.ground(&[Part::new("instance", vec![id]).map_err(err)?]).map_err(err)?;
        let ctl = self.ctl.take().ok_or("no control")?;
        let mut h = ctl.solve(SolveMode::YIELD, &[]).map_err(err)?;
        let syms = match h.model() {
            Ok(Some(m)) => m.symbols(ShowType::ATOMS).map_err(err),
            Ok(None) => Ok(Vec::new()),
            Err(e) => Err(err(e)),
        };
        self.ctl = Some(h.close().map_err(err)?);
        let name = super::compile::pred(fact);
        Ok(syms?
            .into_iter()
            .filter(|s| s.name().is_ok_and(|n| n == name))
            .filter_map(|s| s.arguments().ok()?.iter().map(|a| a.string().ok().map(str::to_string)).collect::<Option<Vec<String>>>())
            .collect())
    }

    /// Open an instance: its id.
    pub fn open(&mut self, inst: &Instance) -> Result<String, String> {
        let _clingo = super::clingo();
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
        for m in &inst.unmet {
            facts += &format!("violates({d},{},where).\n", quote(m));
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
        let _clingo = super::clingo();
        self.set_external("failed", 2, &format!("failed({d},{})", quote(m)), TruthValue::True)
    }

    /// Close D: it is never solved again.
    pub fn close(&mut self, d: &str) -> Result<(), String> {
        let _clingo = super::clingo();
        self.open = self.open.saturating_sub(1);
        let ctl = self.ctl.as_mut().ok_or("no control")?;
        if let Some(l) = Self::literal(ctl, "live", 1, &format!("live({d})"))? {
            ctl.release_external(l).map_err(err)?;
        }
        Ok(())
    }

    /// What made each rule of D's decision hold, or not, in the facts this solver was given (proto-mind N-1446 p1502
    /// (9)): the policy's `why` parts (compile.rs `why_base`, `why`) grounded for D, read off one solve with D switched
    /// off, as its exclusions are when it has no model. Each rule whose `When` held, with the facts it read, and each
    /// whose `When` did not for a fact it says `not` of that held, with that fact; each derived fact with the facts it
    /// came down to. A solver of its own asks it (`ticket::solved`): the parts stay on its control.
    pub fn explain(&mut self, d: &str) -> Result<Vec<Grounds>, String> {
        let _clingo = super::clingo();
        let ctl = self.ctl.as_mut().ok_or("the policy has no facts yet")?;
        if !self.why {
            ctl.add("why_base", &[], &self.compiled.why_base).map_err(err)?;
            ctl.add("why", &["d"], &self.compiled.why).map_err(err)?;
            ctl.ground(&[Part::new("why_base", vec![]).map_err(err)?]).map_err(err)?;
            self.why = true;
        }
        let id = Symbol::create_id(d, true).map_err(err)?;
        ctl.ground(&[Part::new("why", vec![id]).map_err(err)?]).map_err(err)?;
        let ctl = self.ctl.take().ok_or("no control")?;
        let mut h = ctl.solve(SolveMode::YIELD, &[]).map_err(err)?;
        let syms = match h.model() {
            Ok(Some(m)) => m.symbols(ShowType::SHOWN).map_err(err),
            Ok(None) => Ok(Vec::new()),
            Err(e) => Err(err(e)),
        };
        self.ctl = Some(h.close().map_err(err)?);
        Ok(self.grounds(&syms?, d))
    }

    /// The grounds of D's rules in the atoms SYMS of the `why` parts (`explain`).
    fn grounds(&self, syms: &[Symbol], d: &str) -> Vec<Grounds> {
        let names: HashMap<String, &str> = self.compiled.facts.keys().map(|f| (pred(f), f.as_str())).collect();
        let handed: HashSet<String> = self.facts.iter().map(|(f, a)| fact_line(f, a).trim_end_matches('.').to_string()).collect();
        let mut derived: HashMap<String, Vec<Vec<Symbol>>> = HashMap::new();
        // by rule: whether its When held, the facts each instance of it read, the options it bars
        let mut by: BTreeMap<usize, (bool, Vec<Vec<Symbol>>, Vec<String>)> = BTreeMap::new();
        for s in syms {
            let (Ok(name), Ok(a)) = (s.name(), s.arguments()) else { continue };
            if name == "why_derived" && a.len() == 2 {
                derived.entry(a[0].to_string()).or_default().push(a[1].arguments().unwrap_or_default());
                continue;
            }
            if a.len() != 3 || a[1].to_string() != d {
                continue;
            }
            let Some(id) = a[0].arguments().ok().and_then(|p| p.first()?.number().ok()) else { continue };
            let e = by.entry(id as usize).or_default();
            match name {
                "why_held" => {
                    e.0 = true;
                    e.1.push(a[2].arguments().unwrap_or_default());
                }
                "why_unless" => e.1.push(vec![a[2]]),
                "why_bars" => e.2.extend(a[2].string().ok().map(str::to_string)),
                _ => {}
            }
        }
        // one order, whatever order the model shows them in: each way and each instance by what it read
        let key = |fs: &Vec<Symbol>| fs.iter().map(Symbol::to_string).collect::<Vec<_>>();
        derived.values_mut().for_each(|ways| ways.sort_by_key(key));
        let mut out = Vec::new();
        for (id, (held, mut read, mut bars)) in by {
            read.sort_by_key(key);
            let facts: Vec<Symbol> = read.into_iter().flatten().collect();
            // a rule whose When did not hold for want of a fact has no grounds to say
            let Some(r) = self.compiled.rules.iter().find(|r| r.id == id).filter(|_| held || !facts.is_empty()) else { continue };
            let (mut said, mut from, mut seen) = (Vec::new(), Vec::new(), HashSet::new());
            for f in &facts {
                let s = written(f, &names);
                if !said.contains(&s) {
                    said.push(s);
                }
                if derived.contains_key(&f.to_string()) && !handed.contains(&f.to_string()) {
                    came_from(f, &derived, &handed, &names, &mut seen, &mut from);
                }
            }
            bars.sort();
            let file = std::path::Path::new(&r.at.file).file_name().map_or_else(|| r.at.file.clone(), |f| f.to_string_lossy().into_owned());
            out.push(Grounds { rule: format!("{} ({})", r.text.replace('\n', " "), r.at), at: format!("{file}:{}", r.at.line), why: r.why.clone(), held, bars, facts: said, from });
        }
        out
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
                // before) is optimal as it is: clingo proves optimality only of an optimisation (9 Nov: an
                // end-of-turn on a Waiting turn, which no preference ranks, came out as no option at all)
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
        let _clingo = super::clingo();
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
            Some(Cost { rule: format!("{} ({})", rule.text.replace('\n', " "), rule.at), level: l, cost: at(rc, l) - at(&costs, l), why: rule.why.clone() })
        });
        Ok(Outcome { chosen: Some(winner), optimal, excluded, costs, runner_up, decided_by, fault: None, grounds: Vec::new() })
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
        let mut why = None;
        let (reason, rule) = match r.name().unwrap_or("") {
            "failed" => ("failed".to_string(), "the call failed on this model".to_string()),
            "where" => ("where".to_string(), format!("where {}", inst.call.get("where").map_or("", String::as_str))),
            "given" => {
                let f = r.arguments().ok()?[0].string().ok()?.to_string();
                let g = inst.given.iter().find(|g| g.field == f);
                ("given".into(), g.map_or(f.clone(), |g| format!("{} = {} (given by {} at {})", g.field, g.value, g.by, g.at)))
            }
            "r" => {
                let n = r.arguments().ok()?[0].number().ok()? as usize;
                let q = self.compiled.requirements.iter().find(|q| q.id == n)?;
                why = q.why.clone();
                ("requires".into(), format!("{} ({})", q.text, q.at))
            }
            "only" => {
                let n = r.arguments().ok()?[0].arguments().ok()?[0].number().ok()? as usize;
                let q = self.compiled.rules.iter().find(|q| q.id == n)?;
                why = q.why.clone();
                ("only".into(), format!("{} ({})", q.text.replace('\n', " "), q.at))
            }
            other => (other.to_string(), r.to_string()),
        };
        Some(Exclusion { option, reason, rule, why })
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
            .filter_map(|r| Some(Cost { rule: format!("{} ({})", r.text.replace('\n', " "), r.at), level: r.level?, cost: *by.get(&r.id).unwrap_or(&0), why: r.why.clone() }))
            .collect();
        out.sort_by(|a, b| b.level.cmp(&a.level));
        out
    }
}

/// Fact F as the policy writes it (`red-on("6298415b")`): NAMES, each fact's name by its ASP predicate; `no(G)` is
/// `not G`, and `any` a `_`.
fn written(f: &Symbol, names: &HashMap<String, &str>) -> String {
    if f.symbol_type().ok() != Some(SymbolType::Function) {
        return f.to_string();
    }
    let (Ok(name), Ok(args)) = (f.name(), f.arguments()) else { return f.to_string() };
    match (name, &args[..]) {
        ("no", [g]) => format!("not {}", written(g, names)),
        ("any", []) => "_".into(),
        (n, []) => names.get(n).copied().unwrap_or(n).to_string(),
        (n, _) => format!("{}({})", names.get(n).copied().unwrap_or(n), args.iter().map(|a| written(a, names)).collect::<Vec<_>>().join(", ")),
    }
}

/// Into OUT, the facts derived fact F came down to through DERIVED (`why_derived`: each way it was derived, the facts
/// that way read), each written once: those HANDED the solver, the policy's own and absences (`not G`) as they are,
/// a derived one through its own ways, once (SEEN).
fn came_from(f: &Symbol, derived: &HashMap<String, Vec<Vec<Symbol>>>, handed: &HashSet<String>, names: &HashMap<String, &str>, seen: &mut HashSet<String>, out: &mut Vec<String>) {
    let key = f.to_string();
    match derived.get(&key) {
        Some(ways) if !handed.contains(&key) => {
            if seen.insert(key) {
                for g in ways.iter().flatten() {
                    came_from(g, derived, handed, names, seen, out);
                }
            }
        }
        _ => {
            let s = written(f, names);
            if !out.contains(&s) {
                out.push(s);
            }
        }
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
        Instance { kind: "model-for".into(), call: [("task".to_string(), "recognise-question".to_string())].into(), given, unmet: Vec::new() }
    }

    #[test]
    fn codex_first_then_claude_after_a_failure_with_the_reasons() {
        let mut s = solver();
        let inst = recognise(vec![]);
        let d = s.open(&inst).unwrap();
        let o = s.solve(&d, &inst).unwrap();
        assert_eq!(o.chosen.as_deref(), Some("luna"));
        assert_eq!(o.excluded, vec![Exclusion { option: "astra".into(), reason: "requires".into(), rule: "Requires quota-left(option) > 0 (prelude:8)".into(), why: None }]);
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

    /// What made each rule of a decision hold or not (proto-mind N-1446 p1502 (9); paper-research p1617): a commit
    /// whose tree's job failed a test is Red by the rule that reads it, whose grounds are the job, its count and its
    /// failing test, through both of `red-on`'s ways; the trailer that kept the item's rule from holding, and the
    /// Green it would bar; for another item, the trailer's absence, which held. The preference reads no fact.
    #[test]
    fn a_decisions_grounds_are_the_facts_that_made_its_rules_hold() {
        let policy = "Fact job Identified by String * String\nFact job-failed Identified by String * Int\nFact job-failing Identified by String * String\nFact tree-of Identified by String * String\nFact names-item Identified by String * String\nFact red-on Identified by String\n  Derived from red-on(commit) When tree-of(commit, tree) and job(id, tree) and job-failed(id, n) and n > 0\n  Derived from red-on(commit) When tree-of(commit, tree) and job(id, tree) and job-failing(id, test)\nDecision green\n  Options Green | Red\n// not this ask's\nOnly for green When not names-item(call.commit, call.for):\n  option == \"Red\"\n// a job failed\nOnly for green When red-on(call.commit):\n  option == \"Red\"\nPrefer for green:\n  option == \"Green\" > option == \"Red\"\n";
        let mut s = Solver::new(compile(&[(Scope::Project, parse("/p/policy/green.eflint", policy).unwrap())]).unwrap());
        let f = |n: &str, a: Value| (n.to_string(), a.as_array().unwrap().clone());
        s.set_facts(vec![f("tree-of", json!(["c1", "t1"])), f("job", json!(["j1", "t1"])), f("job-failed", json!(["j1", 2])), f("job-failing", json!(["j1", "x::y"])), f("names-item", json!(["c1", "I-7"]))]).unwrap();
        let ask = |s: &mut Solver, item: &str| {
            let inst = Instance { kind: "green".into(), call: [("commit".to_string(), "c1".to_string()), ("for".to_string(), item.to_string())].into(), given: Vec::new(), unmet: Vec::new() };
            let d = s.open(&inst).unwrap();
            let o = s.solve(&d, &inst).unwrap();
            let g = s.explain(&d).unwrap();
            s.close(&d).unwrap();
            (o.chosen.unwrap_or_default(), g.into_iter().map(|g| (g.at, g.why, g.held, g.bars, g.facts, g.from)).collect::<Vec<_>>())
        };
        let v = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let failed = (
            "green.eflint:15".to_string(),
            Some("a job failed".to_string()),
            true,
            v(&["Green"]),
            v(&["red-on(\"c1\")"]),
            v(&["tree-of(\"c1\", \"t1\")", "job(\"j1\", \"t1\")", "job-failed(\"j1\", 2)", "job-failing(\"j1\", \"x::y\")"]),
        );
        let prefer = ("green.eflint:17".to_string(), None, true, v(&[]), v(&[]), v(&[]));
        let item = |held: bool, fact: &str| ("green.eflint:12".to_string(), Some("not this ask's".to_string()), held, v(&["Green"]), v(&[fact]), v(&[]));
        assert_eq!(ask(&mut s, "I-7"), ("Red".to_string(), vec![item(false, "names-item(\"c1\", \"I-7\")"), failed.clone(), prefer.clone()]));
        assert_eq!(ask(&mut s, "I-8"), ("Red".to_string(), vec![item(true, "not names-item(\"c1\", \"I-8\")"), failed, prefer]), "a second instance on the same solver, alone");
        let mut s = solver();
        let inst = recognise(vec![]);
        let d = s.open(&inst).unwrap();
        assert!(s.explain(&d).unwrap().iter().all(|g| g.facts.is_empty() && g.from.is_empty()), "no `When` of model-for's reads a fact");
    }
}
