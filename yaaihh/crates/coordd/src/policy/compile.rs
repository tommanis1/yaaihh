//! The policy compiled to answer set programming for clingo (draft §4.2, §5, §13, §14.4). One program part,
//! `#program instance(d).`, holds every rule about one decision instance `d`; the facts coordd supplies (models,
//! prices) are the `base` part. The shapes are those checked by hand in clingo (research notes `policy-check/*.lp`):
//!
//! ```text
//! option(d,O) :- kind(d,"model-for"), model(O).                    % Options model
//! 1 { chosen(d,O) : option(d,O) } 1 :- decide(d).                  % one option, while the instance is live
//! ok(r(1),d,O) :- option(d,O), price(O,V1), V1 >= 0.               % Requires price(option) >= 0
//! violates(d,O,r(1)) :- option(d,O), kind(d,"model-for"), not ok(r(1),d,O).
//! violates(d,O,failed) :- failed(d,O).                             % the call failed on O (fallback)
//! violates(d,O,given("family")) :- option(d,O), given(d,"family",V), not field(d,O,"family",V).
//! :- chosen(d,O), violates(d,O,_).
//! c(p(1),0,d,O) :- option(d,O), family(O,"codex").                 % A > B: the conditions
//! pen(p(1),d,O,1) :- chosen(d,O), kind(d,"model-for"), call_field(d,"task","recognise-question"),
//!                    c(p(1),1,d,O), not c(p(1),0,d,O).
//! :~ pen(p(1),d,O,W). [W@1999, p(1), d, O]
//! ```
//!
//! Every exclusion is a named `violates/3` atom derived for every option, chosen or not, so provenance reads each
//! option's reasons off one model (§13.5); every preference's cost is a `pen/4` atom, so the cost per preference of
//! the winner and of the runner-up are read off their models, not decoded from clingo's cost vector.
//!
//! Levels (§5.2, §14.1): `scope_rank * 1000 + (1000 - position)`, scope ranks global 0, project 1, agent 2 (call 3 is
//! the caller's given fields, which are hard). clingo optimises the highest level first.

use super::syntax::{At, Cond, Decl, DeclKind, Expr, Lit, Op, Rank};
use std::collections::BTreeMap;
use std::fmt::Write;

/// Where a policy file's declarations apply, narrowest last.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    /// The system's prelude and global policy (agent-base/policy).
    Global,
    /// The project's policy (PROJECT/policy/*.eflint).
    Project,
    /// One agent's (PROJECT/policy/agents/NAME.eflint): its declarations hold only for calls of that agent.
    Agent(String),
}

impl Scope {
    fn rank(&self) -> i64 {
        match self {
            Scope::Global => 0,
            Scope::Project => 1,
            Scope::Agent(_) => 2,
        }
    }
}

/// A preference or hard rule as compiled: what provenance cites.
#[derive(Clone, Debug)]
pub struct Rule {
    pub id: usize,
    pub decision: String,
    /// The optimisation level; None for a hard rule (`Only`).
    pub level: Option<i64>,
    pub at: At,
    pub text: String,
}

/// A `Requires` line as compiled.
#[derive(Clone, Debug)]
pub struct Requirement {
    pub id: usize,
    pub at: At,
    pub text: String,
}

/// A decision type: its options' fact and the facts its options have fields in (for caller-given fields).
#[derive(Clone, Debug)]
pub struct DecisionType {
    pub name: String,
    pub options: String,
    /// Options listed in place (`Options accept | nudge | escalate`) rather than a fact type's instances.
    pub listed: Vec<String>,
    pub at: At,
    /// Judge declared for ties, with its guideline file.
    pub judge: Option<String>,
}

/// The compiled policy.
#[derive(Clone, Debug, Default)]
pub struct Compiled {
    /// The `#program instance(d).` part.
    pub instance: String,
    /// Fact types: name → identifier types.
    pub facts: BTreeMap<String, Vec<String>>,
    pub decisions: BTreeMap<String, DecisionType>,
    pub rules: Vec<Rule>,
    pub requirements: Vec<Requirement>,
}

/// Names the compiled program uses itself; a fact type may not take one.
const RESERVED: &[&str] = &["option", "chosen", "decide", "kind", "live", "failed", "given", "field", "violates", "ok", "c", "pen", "call_field", "in", "state", "cond"];

/// A name as an ASP predicate (`quota-left` → `quota_left`).
pub fn pred(name: &str) -> String {
    name.replace('-', "_")
}

/// A string as an ASP string term.
pub fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n"))
}

/// The atom of instance `+FACT(ARGS).` of a declared fact type (FACTS: the declared ones and their identifiers).
pub fn instance_atom(facts: &BTreeMap<String, Vec<String>>, fact: &str, args: &[Expr]) -> Result<String, String> {
    let ids = facts.get(fact).ok_or(format!("+{fact}: no fact type `{fact}`"))?;
    if ids.len() != args.len() {
        return Err(format!("+{fact}: `{fact}` has {} identifiers, the instance gives {}", ids.len(), args.len()));
    }
    let vals: Vec<String> = args.iter().map(|a| match a {
        Expr::Str(s) => quote(s),
        Expr::Int(i) => i.to_string(),
        _ => unreachable!("syntax.rs: an instance's values are strings and integers"),
    }).collect();
    Ok(format!("{}({})", pred(fact), vals.join(",")))
}

/// Compile the declarations of each scope, in order (the prelude and global files first).
pub fn compile(scopes: &[(Scope, Vec<Decl>)]) -> Result<Compiled, String> {
    let mut c = Compiled::default();
    // declarations of facts and decisions first, wherever they are
    for (_, decls) in scopes {
        for d in decls {
            match &d.kind {
                DeclKind::Fact { name, ids } => {
                    if RESERVED.contains(&pred(name).as_str()) {
                        return Err(format!("{}: `{name}` is a name the compiled policy uses itself", d.at));
                    }
                    c.facts.insert(name.clone(), ids.clone());
                }
                DeclKind::Decision { name, options, .. } => {
                    let listed: Vec<String> = if options.contains('|') { options.split('|').map(|o| o.trim().to_string()).collect() } else { Vec::new() };
                    c.decisions.insert(name.clone(), DecisionType { name: name.clone(), options: options.clone(), listed, at: d.at.clone(), judge: None });
                }
                _ => {}
            }
        }
    }
    let mut out = String::from("#program instance(d).\n#external live(d).\ndecide(d) :- live(d).\n");
    out += "1 { chosen(d,O) : option(d,O) } 1 :- decide(d).\n";
    out += "#external failed(d,O) : option(d,O).\nviolates(d,O,failed) :- failed(d,O).\n";
    out += "violates(d,O,given(F)) :- option(d,O), given(d,F,V), not field(d,O,F,V).\n";
    out += ":- chosen(d,O), violates(d,O,_).\n";
    for t in c.decisions.values() {
        if !t.listed.is_empty() {
            // options listed in place: each is an option, and its own field `option` (a caller may give it)
            let k = quote(&t.name);
            for o in &t.listed {
                writeln!(out, "option(d,{}) :- kind(d,{k}).", quote(o)).unwrap();
            }
            writeln!(out, "field(d,O,\"option\",O) :- option(d,O), kind(d,{k}).").unwrap();
            continue;
        }
        let ids = c.facts.get(&t.options).ok_or(format!("{}: Options {}: no such fact type", t.at, t.options))?;
        if ids.len() != 1 {
            return Err(format!("{}: Options {}: the options' fact type must have one identifier", t.at, t.options));
        }
        let k = quote(&t.name);
        writeln!(out, "option(d,O) :- kind(d,{k}), {}(O).", pred(&t.options)).unwrap();
        // the caller-given fields an option has: itself, and each fact about it with one value
        writeln!(out, "field(d,O,{},O) :- option(d,O), kind(d,{k}).", quote(&t.options)).unwrap();
        for (f, ids) in &c.facts {
            if ids.len() == 2 && ids[0] == t.options {
                writeln!(out, "field(d,O,{},V) :- option(d,O), kind(d,{k}), {}(O,V).", quote(f), pred(f)).unwrap();
            }
        }
    }
    let mut position: BTreeMap<i64, i64> = BTreeMap::new();
    for (scope, decls) in scopes {
        for d in decls {
            match &d.kind {
                DeclKind::Decision { name, requires, .. } => {
                    for (at, cond, text) in requires {
                        let id = c.requirements.len() + 1;
                        let mut v = Vars::default();
                        let body = v.cond(&c, cond).map_err(|e| format!("{at}: {e}"))?;
                        writeln!(out, "ok(r({id}),d,O) :- option(d,O), kind(d,{}){}.", quote(name), comma(&body)).unwrap();
                        writeln!(out, "violates(d,O,r({id})) :- option(d,O), kind(d,{}), not ok(r({id}),d,O).", quote(name)).unwrap();
                        c.requirements.push(Requirement { id, at: at.clone(), text: text.clone() });
                    }
                }
                DeclKind::Prefer { decision, when, rank } => {
                    let level = scope.rank() * 1000 + (1000 - *position.entry(scope.rank()).and_modify(|p| *p += 1).or_insert(0));
                    let id = c.rules.len() + 1;
                    let guard = c.guard(scope, decision, when).map_err(|e| format!("{}: {e}", d.at))?;
                    match rank {
                        Rank::Order(conds) => {
                            for (k, cond) in conds.iter().enumerate() {
                                let mut v = Vars::default();
                                let body = v.cond(&c, cond).map_err(|e| format!("{}: {e}", d.at))?;
                                writeln!(out, "c(p({id}),{k},d,O) :- option(d,O){}.", comma(&body)).unwrap();
                            }
                            // rank k: the first condition it satisfies; conds.len(): none (a single condition: 0 or 1)
                            for k in 0..=conds.len() {
                                let mut lits: Vec<String> = Vec::new();
                                if k < conds.len() {
                                    lits.push(format!("c(p({id}),{k},d,O)"));
                                }
                                lits.extend((0..k).map(|j| format!("not c(p({id}),{j},d,O)")));
                                if k == 0 {
                                    continue; // the best rank costs nothing
                                }
                                writeln!(out, "pen(p({id}),d,O,{k}) :- chosen(d,O){}{}.", comma(&guard), comma(&lits)).unwrap();
                            }
                        }
                        Rank::Minimise(e) | Rank::Maximise(e) => {
                            let mut v = Vars::default();
                            let (term, body) = v.expr(&c, e).map_err(|e| format!("{}: {e}", d.at))?;
                            let w = if matches!(rank, Rank::Maximise(_)) { format!("-{term}") } else { term };
                            let mut lits = guard.clone();
                            lits.extend(body);
                            writeln!(out, "pen(p({id}),d,O,{w}) :- chosen(d,O){}.", comma(&lits)).unwrap();
                        }
                    }
                    writeln!(out, ":~ pen(p({id}),d,O,W). [W@{level}, p({id}), d, O]").unwrap();
                    c.rules.push(Rule { id, decision: decision.clone(), level: Some(level), at: d.at.clone(), text: d.text.clone() });
                }
                DeclKind::Only { decision, when, cond } => {
                    let id = c.rules.len() + 1;
                    let guard = c.guard(scope, decision, when).map_err(|e| format!("{}: {e}", d.at))?;
                    let mut v = Vars::default();
                    let body = v.cond(&c, cond).map_err(|e| format!("{}: {e}", d.at))?;
                    writeln!(out, "c(p({id}),0,d,O) :- option(d,O){}.", comma(&body)).unwrap();
                    writeln!(out, "violates(d,O,only(p({id}))) :- option(d,O){}, not c(p({id}),0,d,O).", comma(&guard)).unwrap();
                    c.rules.push(Rule { id, decision: decision.clone(), level: None, at: d.at.clone(), text: d.text.clone() });
                }
                DeclKind::Judge { decision, using } => match c.decisions.get_mut(decision) {
                    Some(t) => t.judge = Some(using.clone()),
                    None => return Err(format!("{}: Judge {decision}: no such decision", d.at)),
                },
                DeclKind::Instance { fact, args } => {
                    out += &instance_atom(&c.facts, fact, args).map_err(|e| format!("{}: {e}", d.at))?;
                    out += ".\n";
                }
                // norms are compiled on their own (norms.rs)
                DeclKind::Fact { .. } | DeclKind::Act { .. } => {}
            }
        }
    }
    out += "#show chosen/2.\n#show violates/3.\n#show pen/4.\n";
    c.instance = out;
    Ok(c)
}

fn comma(lits: &[String]) -> String {
    lits.iter().map(|l| format!(", {l}")).collect()
}

impl Compiled {
    /// The body literals that make a rule of SCOPE about DECISION apply: the decision's kind, the agent of an agent's
    /// file, and its `When` condition.
    fn guard(&self, scope: &Scope, decision: &str, when: &Cond) -> Result<Vec<String>, String> {
        if !self.decisions.contains_key(decision) {
            return Err(format!("no decision `{decision}`"));
        }
        let mut lits = vec![format!("kind(d,{})", quote(decision))];
        if let Scope::Agent(a) = scope {
            lits.push(format!("call_field(d,\"agent\",{})", quote(a)));
        }
        lits.extend(Vars { n: 100 }.cond(self, when)?);
        Ok(lits)
    }
}

/// Fresh variables for one rule body.
#[derive(Default)]
struct Vars {
    n: usize,
}

impl Vars {
    fn fresh(&mut self) -> String {
        self.n += 1;
        format!("V{}", self.n)
    }

    /// A term for E and the literals that bind it.
    fn expr(&mut self, c: &Compiled, e: &Expr) -> Result<(String, Vec<String>), String> {
        Ok(match e {
            Expr::Option => ("O".into(), vec![]),
            Expr::Any => return Err("`_` stands only for a fact's argument (`harness(option, _)`)".into()),
            Expr::Field(f) => return Err(format!("`{f}`: a bare name is an act's field, in a norm only")),
            Expr::Str(s) => (quote(s), vec![]),
            Expr::Int(n) => (n.to_string(), vec![]),
            Expr::Call(f) => {
                let v = self.fresh();
                (v.clone(), vec![format!("call_field(d,{},{v})", quote(f))])
            }
            Expr::App(f, args) => {
                let arity = c.facts.get(f).ok_or(format!("no fact type `{f}`"))?.len();
                if arity != args.len() + 1 {
                    return Err(format!("`{f}` has {arity} identifiers: as a value it takes {} arguments", arity - 1));
                }
                let (mut terms, mut body) = self.args(c, args)?;
                let v = self.fresh();
                terms.push(v.clone());
                body.push(format!("{}({})", pred(f), terms.join(",")));
                (v, body)
            }
        })
    }

    fn args(&mut self, c: &Compiled, args: &[Expr]) -> Result<(Vec<String>, Vec<String>), String> {
        let (mut terms, mut body) = (Vec::new(), Vec::new());
        for a in args {
            if *a == Expr::Any {
                terms.push("_".into());
                continue;
            }
            let (t, b) = self.expr(c, a)?;
            terms.push(t);
            body.extend(b);
        }
        Ok((terms, body))
    }

    /// The body literals of a condition (bindings first, negations last, so every variable is bound).
    fn cond(&mut self, c: &Compiled, cond: &Cond) -> Result<Vec<String>, String> {
        let (mut pos, mut neg) = (Vec::new(), Vec::new());
        for l in cond {
            match l {
                Lit::Holds(f, args) | Lit::Not(f, args) => {
                    let arity = c.facts.get(f).ok_or(format!("no fact type `{f}`"))?.len();
                    if arity != args.len() {
                        return Err(format!("`{f}` has {arity} identifiers: as a condition it takes {arity} arguments"));
                    }
                    let (terms, body) = self.args(c, args)?;
                    pos.extend(body);
                    let atom = format!("{}({})", pred(f), terms.join(","));
                    if matches!(l, Lit::Not(..)) { neg.push(format!("not {atom}")) } else { pos.push(atom) }
                }
                Lit::Cmp(a, op, b) => {
                    let (ta, ba) = self.expr(c, a)?;
                    let (tb, bb) = self.expr(c, b)?;
                    pos.extend(ba);
                    pos.extend(bb);
                    let op = match op {
                        Op::Eq => "=",
                        Op::Ne => "!=",
                        Op::Lt => "<",
                        Op::Le => "<=",
                        Op::Gt => ">",
                        Op::Ge => ">=",
                    };
                    pos.push(format!("{ta} {op} {tb}"));
                }
            }
        }
        pos.extend(neg);
        Ok(pos)
    }
}

/// The base part's fact line for fact NAME with ARGS (strings or numbers, as JSON values).
pub fn fact_line(name: &str, args: &[serde_json::Value]) -> String {
    let terms: Vec<String> = args.iter().map(|a| match a {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => quote(s),
        other => quote(&other.to_string()),
    }).collect();
    format!("{}({}).", pred(name), terms.join(","))
}

#[cfg(test)]
mod tests {
    use super::super::syntax::parse;
    use super::*;

    pub const PRELUDE: &str = "Fact model Identified by String\nFact family Identified by model * String\nFact price Identified by model * Int\nDecision model-for\n  Options model\n  Requires price(option) >= 0\n";
    pub const PROJECT: &str = "Prefer for model-for When call.task == \"recognise-question\":\n  family(option, \"codex\") > family(option, \"claude\")\nPrefer for model-for:\n  minimise price(option)\n";

    #[test]
    fn example_a_compiles_to_the_checked_shapes() {
        let c = compile(&[(Scope::Global, parse("prelude", PRELUDE).unwrap()), (Scope::Project, parse("decisions", PROJECT).unwrap())]).unwrap();
        let p = &c.instance;
        for want in [
            "option(d,O) :- kind(d,\"model-for\"), model(O).",
            "1 { chosen(d,O) : option(d,O) } 1 :- decide(d).",
            "ok(r(1),d,O) :- option(d,O), kind(d,\"model-for\"), price(O,V1), V1 >= 0.",
            "violates(d,O,r(1)) :- option(d,O), kind(d,\"model-for\"), not ok(r(1),d,O).",
            "field(d,O,\"family\",V) :- option(d,O), kind(d,\"model-for\"), family(O,V).",
            "c(p(1),0,d,O) :- option(d,O), family(O,\"codex\").",
            "pen(p(1),d,O,1) :- chosen(d,O), kind(d,\"model-for\"), call_field(d,\"task\",V101), V101 = \"recognise-question\", c(p(1),1,d,O), not c(p(1),0,d,O).",
            "pen(p(1),d,O,2) :- chosen(d,O), kind(d,\"model-for\"), call_field(d,\"task\",V101), V101 = \"recognise-question\", not c(p(1),0,d,O), not c(p(1),1,d,O).",
            ":~ pen(p(1),d,O,W). [W@2000, p(1), d, O]",
            "pen(p(2),d,O,V1) :- chosen(d,O), kind(d,\"model-for\"), price(O,V1).",
            ":~ pen(p(2),d,O,W). [W@1999, p(2), d, O]",
        ] {
            assert!(p.contains(want), "missing {want}\nin\n{p}");
        }
        assert_eq!(c.rules[0].at.to_string(), "decisions:1");
    }

    #[test]
    fn agent_files_rank_above_the_project_and_unknown_facts_are_errors() {
        let c = compile(&[
            (Scope::Global, parse("prelude", PRELUDE).unwrap()),
            (Scope::Project, parse("decisions", PROJECT).unwrap()),
            (Scope::Agent("orch".into()), parse("orch", "Only for model-for:\n  family(option, \"claude\")\n").unwrap()),
        ]).unwrap();
        assert!(c.instance.contains("violates(d,O,only(p(3))) :- option(d,O), kind(d,\"model-for\"), call_field(d,\"agent\",\"orch\"), not c(p(3),0,d,O)."), "{}", c.instance);
        let e = compile(&[(Scope::Global, parse("prelude", PRELUDE).unwrap()), (Scope::Project, parse("x", "Prefer for model-for:\n  minimise speed(option)\n").unwrap())]);
        assert!(e.unwrap_err().contains("no fact type `speed`"));
    }
}
