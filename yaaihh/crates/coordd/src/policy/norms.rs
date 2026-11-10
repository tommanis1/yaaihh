//! eFLINT's norms compiled to answer set programs (draft §3): the translation of Esterhuyse, Müller and van Binsbergen
//! ("A Stable Model Semantics for eFLINT Norm Specifications", GPCE 2025; their Haskell compiler,
//! `Language/EFLINT/Clingo.hs`, Zenodo 15470286; copy in agent-coord/research/notes/eflint-clingo). An instance's
//! standing in state `S` is `in((ATTR, X), S)`: `holds`, `enabled`, `derived`, `suppressed`, `enum`, and the
//! transition atoms (`create`, `terminate`, `add`, `rem`, `actTrigger`). [`LIB`] is their state-trace and eFLINT
//! libraries verbatim; the rules per declaration follow their abstraction (`Abstract.hs`, `abstractifyTypeSpec`):
//!
//! ```text
//! in((holds,role(X1,X2)),S) :- role(X1,X2), state(S).          % a fact coordd supplies holds in every state
//! in((enum,role(X1,X2)),S) :- in((holds,role(X1,X2)),S).        % Infinite: the instances that hold are enumerable
//! in((derived,send_mail(F1,F2,F3)),S) :- state(S),              % Holds when C: Foreach field (enum): act Where C
//!     in((enum,agent(F1)),S), in((enum,agent(F2)),S), in((enum,thread(F3)),S), not in((holds,role(F2,"consultant")),S).
//! cond(c(1),send_mail(F1,F2,F3),S) :- state(S), <the fields' enums>, F1 != F2.          % Conditioned by C
//! in((suppressed,send_mail(F1,F2,F3)),S) :- state(S), <the fields' enums>, not cond(c(1),send_mail(F1,F2,F3),S).
//! ```
//!
//! Two departures, both deliberate:
//! - A field's value is the identifier of its type's instance, not the instance (`role("orch","lead")`, not
//!   `role(agent("orch"),"lead")`): coordd's facts are flat (the decisions read them as `family(O,"codex")`), so a
//!   field's type must be a fact type with one identifier, whose instances are `T(V)` with field value `V`.
//! - `Conditioned by` is one condition atom per line, negated in the suppression rule, and suppression ranges over the
//!   fields' enumerations rather than the act's own: the Haskell compiler negates the condition's conjunction literal
//!   by literal (`not a ; b`) and enumerates over `holds` of the act, which `suppressed` itself defeats (no stable
//!   model once a condition fails).
//!
//! The current state is state 1 (the library's `state(1)`); the facts are the situation then. One solve gives every
//! act instance's standing in that situation ([`Norms::solve`], [`Norms::verdict`]); coordd keeps the solved
//! situations until the policy, an agent or a role changes (core.rs `NormCache`), so a mail's check is a lookup.

use super::compile::{pred, quote};
use super::syntax::{At, Cond, DeclKind, Expr, Lit, Op};
use super::Places;
use clingo::{ShowType, SolveMode};
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write;

/// The state-trace and eFLINT libraries of the GPCE translation, as `Clingo.hs` writes them.
const LIB: &str = "state(1).
state(S) :- in(F, S).
state(S - 1) :- state(S), 1 < S.
:- state(S), not 0 < S.
in(F, S + 1) :- in((add, F), S), not in((rem, F), S), state(S + 1).
in((add, F), S + 1) :- in((add, F), S), not in((rem, F), S), state(S + 1).
in((add, (created, X)), S) :- in((create, X), S).
in((rem, (terminated, X)), S) :- in((create, X), S).
in((rem, (created, X)), S) :- in((terminate, X), S), not in((create, X), S).
in((add, (terminated, X)), S) :- in((terminate, X), S), not in((create, X), S).
in((rem, (created, X)), S) :- in((obfuscate, X), S), not in((terminate, X), S), not in((create, X), S).
in((rem, (terminated, X)), S) :- in((obfuscate, X), S), not in((terminate, X), S), not in((create, X), S).
in((holds, X), S) :- in((created, X), S).
in((holds, X), S) :- in((derived, X), S), not in((suppressed, X), S), not in((terminated, X), S).
in((enabled, X), S) :- in((holds, X), S), not in((suppressed, X), S).
in((dutyViolation, X), S) :- in((violated, X), S), in((enabled, X), S).
in((actViolation, X), S) :- in((actTrigger, X), S), not in((enabled, X), S).
";

/// A compiled act: its fields and the lines provenance cites.
#[derive(Clone, Debug)]
pub struct Act {
    pub fields: Vec<(String, String)>,
    pub holds: Vec<(At, String)>,
    /// Each `Conditioned by` line with its number `K` in the program (`cond(c(K),...)`).
    pub conditions: Vec<(At, String, usize)>,
}

/// The norms of a policy, compiled.
#[derive(Clone, Debug, Default)]
pub struct Norms {
    pub program: String,
    pub facts: BTreeMap<String, Vec<String>>,
    pub acts: BTreeMap<String, Act>,
}

/// Whether an act instance is enabled now, and why not when it is not.
#[derive(Clone, Debug)]
pub struct Verdict {
    pub enabled: bool,
    /// The policy lines that decide it: the `Holds when` lines that hold, or (not enabled) that none does, and the
    /// `Conditioned by` lines that fail.
    pub why: String,
}

/// Compile the norms of the policy files of PLACES.
pub fn load(places: &Places) -> Result<Norms, String> {
    let scopes = places.sources()?;
    let mut n = Norms::default();
    for (_, decls) in &scopes {
        for d in decls {
            if let DeclKind::Fact { name, ids } = &d.kind {
                n.facts.insert(name.clone(), ids.clone());
            }
        }
    }
    let mut out = String::from(LIB);
    for (name, ids) in &n.facts {
        let xs: Vec<String> = (1..=ids.len()).map(|i| format!("X{i}")).collect();
        let atom = format!("{}({})", pred(name), xs.join(","));
        writeln!(out, "#defined {}/{}.", pred(name), ids.len()).unwrap();
        writeln!(out, "in((holds,{atom}),S) :- {atom}, state(S).").unwrap();
        writeln!(out, "in((enum,{atom}),S) :- in((holds,{atom}),S).").unwrap();
    }
    // the installer's instances (`+tier("m", 1).`): facts in every state, as coordd's are
    for (_, decls) in &scopes {
        for d in decls {
            if let DeclKind::Instance { fact, args } = &d.kind {
                writeln!(out, "{}.", super::compile::instance_atom(&n.facts, fact, args).map_err(|e| format!("{}: {e}", d.at))?).unwrap();
            }
        }
    }
    let mut ccount = 0;
    for (_, decls) in &scopes {
        for d in decls {
            let DeclKind::Act { name, fields, holds, conditions } = &d.kind else { continue };
            if n.facts.contains_key(name) || n.acts.contains_key(name) {
                return Err(format!("{}: `{name}` is declared twice", d.at));
            }
            let vars: BTreeMap<&str, String> = fields.iter().enumerate().map(|(i, (f, _))| (f.as_str(), format!("F{}", i + 1))).collect();
            let mut domain = Vec::new();
            for (f, ty) in fields {
                match n.facts.get(ty) {
                    Some(ids) if ids.len() == 1 => domain.push(format!("in((enum,{}({})),S)", pred(ty), vars[f.as_str()])),
                    Some(_) => return Err(format!("{}: field `{f}`: its type `{ty}` must be a fact type with one identifier", d.at)),
                    None => return Err(format!("{}: field `{f}`: no fact type `{ty}`", d.at)),
                }
            }
            let inst = format!("{}({})", pred(name), fields.iter().map(|(f, _)| vars[f.as_str()].clone()).collect::<Vec<_>>().join(","));
            let head = |body: &[String]| -> String { std::iter::once("state(S)".to_string()).chain(domain.iter().cloned()).chain(body.iter().cloned()).collect::<Vec<_>>().join(", ") };
            for (at, c, _) in holds {
                let body = Body::new(&n, &vars).cond(c).map_err(|e| format!("{at}: {e}"))?;
                writeln!(out, "in((derived,{inst}),S) :- {}.", head(&body)).unwrap();
            }
            let mut numbered = Vec::new();
            for (at, c, text) in conditions {
                ccount += 1;
                numbered.push((at.clone(), text.clone(), ccount));
                let body = Body::new(&n, &vars).cond(c).map_err(|e| format!("{at}: {e}"))?;
                writeln!(out, "cond(c({ccount}),{inst},S) :- {}.", head(&body)).unwrap();
                writeln!(out, "in((suppressed,{inst}),S) :- {}.", head(&[format!("not cond(c({ccount}),{inst},S)")])).unwrap();
            }
            let holds = holds.iter().map(|(a, _, t)| (a.clone(), t.clone())).collect();
            n.acts.insert(name.clone(), Act { fields: fields.clone(), holds, conditions: numbered });
        }
    }
    out += "#show in/2.\n#show cond/3.\n";
    n.program = out;
    Ok(n)
}

/// The literals of a norm's condition over an act's fields, in state `S`.
struct Body<'a> {
    norms: &'a Norms,
    vars: &'a BTreeMap<&'a str, String>,
    n: usize,
}

impl<'a> Body<'a> {
    fn new(norms: &'a Norms, vars: &'a BTreeMap<&'a str, String>) -> Self {
        Body { norms, vars, n: 0 }
    }

    /// A term for E and the literals that bind it.
    fn expr(&mut self, e: &Expr) -> Result<(String, Vec<String>), String> {
        Ok(match e {
            Expr::Field(f) => (self.vars.get(f.as_str()).cloned().ok_or(format!("`{f}` is not a field of the act ({})", self.vars.keys().cloned().collect::<Vec<_>>().join(", ")))?, vec![]),
            Expr::Str(s) => (quote(s), vec![]),
            Expr::Int(i) => (i.to_string(), vec![]),
            Expr::Any => ("_".into(), vec![]),
            Expr::App(f, args) => {
                // a fact read as a function: its last identifier
                let arity = self.norms.facts.get(f).ok_or(format!("no fact type `{f}`"))?.len();
                if arity != args.len() + 1 {
                    return Err(format!("`{f}` has {arity} identifiers: as a value it takes {} arguments", arity - 1));
                }
                let (mut terms, mut body) = self.args(args)?;
                self.n += 1;
                let v = format!("V{}", self.n);
                terms.push(v.clone());
                body.push(format!("in((holds,{}({})),S)", pred(f), terms.join(",")));
                (v, body)
            }
            Expr::Option | Expr::Call(_) => return Err("`option` and `call.FIELD` belong to decisions, not norms".into()),
        })
    }

    fn args(&mut self, args: &[Expr]) -> Result<(Vec<String>, Vec<String>), String> {
        let (mut terms, mut body) = (Vec::new(), Vec::new());
        for a in args {
            let (t, b) = self.expr(a)?;
            terms.push(t);
            body.extend(b);
        }
        Ok((terms, body))
    }

    /// The conjunction's literals (bindings first, negations last, so every variable is bound).
    fn cond(&mut self, c: &Cond) -> Result<Vec<String>, String> {
        let (mut pos, mut neg) = (Vec::new(), Vec::new());
        for l in c {
            match l {
                Lit::Holds(f, args) | Lit::Not(f, args) => {
                    let arity = self.norms.facts.get(f).ok_or(format!("no fact type `{f}`"))?.len();
                    if arity != args.len() {
                        return Err(format!("`{f}` has {arity} identifiers: as a condition it takes {arity} arguments"));
                    }
                    let (terms, body) = self.args(args)?;
                    pos.extend(body);
                    let atom = format!("in((holds,{}({})),S)", pred(f), terms.join(","));
                    if matches!(l, Lit::Not(..)) { neg.push(format!("not {atom}")) } else { pos.push(atom) }
                }
                Lit::Cmp(a, op, b) => {
                    let (ta, ba) = self.expr(a)?;
                    let (tb, bb) = self.expr(b)?;
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

impl Norms {
    /// The norms solved in the situation FACTS: the atoms shown at state 1 (`in/2`, `cond/3`), for [`Norms::verdict`]
    /// to read any act instance from. One solve answers every instance (design coordd-core-2026-11-10 §5).
    pub fn solve(&self, facts: &[(String, Vec<String>)]) -> Result<HashSet<String>, String> {
        let mut program = self.program.clone();
        for (f, ids) in facts {
            let Some(types) = self.facts.get(f).filter(|d| d.len() == ids.len()) else {
                return Err(format!("coordd supplies `{f}` with {} identifiers, which the policy does not declare", ids.len()));
            };
            // an `Int` identifier is a number (`used(actor) < granted(actor)` compares numbers), the rest strings
            let vals: Vec<String> = ids.iter().zip(types).map(|(i, t)| if t == "Int" && i.parse::<i64>().is_ok() { i.clone() } else { quote(i) }).collect();
            writeln!(program, "{}({}).", pred(f), vals.join(",")).unwrap();
        }
        let e = |e: clingo::ClingoError| format!("clingo: {e}");
        let mut ctl = clingo::control(vec![]).map_err(e)?;
        ctl.add("base", &[], &program).map_err(e)?;
        ctl.ground(&[clingo::Part::new("base", vec![]).map_err(e)?]).map_err(e)?;
        let mut h = ctl.solve(SolveMode::YIELD, &[]).map_err(e)?;
        let shown: HashSet<String> = match h.model().map_err(e)? {
            Some(m) => m.symbols(ShowType::SHOWN).map_err(e)?.iter().map(|s| s.to_string()).collect(),
            None => return Err("the norms have no stable model in this situation".into()),
        };
        h.close().map_err(e)?;
        Ok(shown)
    }

    /// Act ACT with field values ARGS as SHOWN ([`Norms::solve`]) has it: enabled or not, and why; None when the
    /// policy has no such act.
    pub fn verdict(&self, shown: &HashSet<String>, act: &str, args: &[&str]) -> Result<Option<Verdict>, String> {
        let Some(a) = self.acts.get(act) else { return Ok(None) };
        if a.fields.len() != args.len() {
            return Err(format!("act {act} has {} fields, the query gives {}", a.fields.len(), args.len()));
        }
        let inst = format!("{}({})", pred(act), args.iter().map(|v| quote(v)).collect::<Vec<_>>().join(","));
        let has = |attr: &str| shown.contains(&format!("in(({attr},{inst}),1)"));
        let enabled = has("enabled");
        let mut why = Vec::new();
        if !has("derived") {
            why.push(format!("no `Holds when` of {act} holds ({})", a.holds.iter().map(|(at, t)| format!("{t} at {at}")).collect::<Vec<_>>().join("; ")));
        }
        for (at, t, k) in &a.conditions {
            if !shown.contains(&format!("cond(c({k}),{inst},1)")) {
                why.push(format!("`{t}` fails ({at})"));
            }
        }
        if enabled {
            why.push(format!("{act} holds and is not suppressed"));
        }
        Ok(Some(Verdict { enabled, why: why.join("; ") }))
    }
}
