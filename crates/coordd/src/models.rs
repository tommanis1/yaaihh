//! The model classes (paper-research N-101, the form the owner agreed over N-188, N-199 and N-201: "the model classes
//! you showed are fine"). The project's policy orders the models with one number each, within and across families
//! (`+tier("claude-sonnet-5-5", 2).`, teams.eflint), and names numbers as classes (`+class("medium", 2).`); the
//! proxy says each model's family (`family`, policy/facts.rs). A call may carry `where COND` over `model`, the model
//! of the agent its step goes to (`call design(words: Text) where model >= medium -> ...`;
//! docket_machine::syntax::model_cond reads it). coordd holds it:
//!
//! - when a step is given to an agent that exists: one whose model does not meet the call's `where` is refused the
//!   step, which raises NoWorker with the model and the condition said (`Input::ModelRefused`);
//! - when it makes an agent for a step (`step fresh`, `let agent`): the policy's `model-for` decides among the models
//!   that meet it (the others excluded by `where`, policy/solve.rs); a copy (`fork`) keeps the model of what it
//!   copies, and is refused if that does not meet the call;
//! - `coord new` and `coord fork` take `--model` as a model or a class name (make.rs);
//! - at load, a step that would make an agent with no model said (no `where` on its call and no `model-for` rule of
//!   the project for agents) refuses its kind, naming the call (ticket/mod.rs).
//!
//! No model is taken by default: where none is said, nothing is made.

use crate::policy::compile::Scope;
use crate::policy::syntax::{Decl, DeclKind, Expr as PExpr};
use docket_machine::syntax::{Expr, Rel};
use std::collections::BTreeMap;

/// What a `where` is held to: each model's number, each class's, each model's family.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Order {
    /// `tier(MODEL, N)`: the model's number, one order within and across families.
    pub tier: BTreeMap<String, i64>,
    /// `class(NAME, N)`: the number the class names.
    pub class: BTreeMap<String, i64>,
    /// The proxy's `family(MODEL, F)`.
    pub family: BTreeMap<String, String>,
}

/// A value in a `where`: a model, a number, a name.
#[derive(Clone, Debug, PartialEq)]
enum V {
    Model(String),
    Num(i64),
    Name(String),
}

impl Order {
    /// The order the policy's files SCOPES state (their `+tier` and `+class` instances), with the families of MODELS
    /// (model, family: what the proxy serves).
    pub fn of(scopes: &[(Scope, Vec<Decl>)], models: &[(String, String)]) -> Order {
        let mut o = Order { family: models.iter().cloned().collect(), ..Default::default() };
        for d in scopes.iter().flat_map(|(_, ds)| ds) {
            if let DeclKind::Instance { fact, args } = &d.kind {
                if let [PExpr::Str(n), PExpr::Int(k)] = args.as_slice() {
                    match fact.as_str() {
                        "tier" => o.tier.insert(n.clone(), *k),
                        "class" => o.class.insert(n.clone(), *k),
                        _ => None,
                    };
                }
            }
        }
        o
    }

    /// The order of the policy at PLACES, with the families of MODELS; why it could not be read.
    pub fn read(places: &crate::policy::Places, models: &[(String, String)]) -> Result<Order, String> {
        Ok(Order::of(&places.sources()?, models))
    }

    /// The number of MODEL.
    fn number(&self, model: &str) -> Result<i64, String> {
        self.tier.get(model).copied().ok_or_else(|| format!("{model} has no number in the policy (no `+tier(\"{model}\", N).`)"))
    }

    /// The class names of number N, for a reader: `medium`, or `2` when no class names it.
    fn classes_of(&self, n: i64) -> String {
        let cs: Vec<&str> = self.class.iter().filter(|(_, k)| **k == n).map(|(c, _)| c.as_str()).collect();
        if cs.is_empty() { n.to_string() } else { cs.join("/") }
    }

    /// MODEL as a reader is shown it: its name, number and class (`claude-haiku-5-5 (1, small)`).
    pub fn show(&self, model: &str) -> String {
        match self.tier.get(model) {
            Some(n) => format!("{model} ({n}, {})", self.classes_of(*n)),
            None => format!("{model} (no number in the policy)"),
        }
    }

    /// What the bare name W stands for: a class (its number), a model the policy orders or the proxy serves, else a
    /// name (a family's: `claude`).
    fn word(&self, w: &str) -> V {
        if let Some(n) = self.class.get(w) {
            return V::Num(*n);
        }
        if self.tier.contains_key(w) || self.family.contains_key(w) {
            return V::Model(w.to_string());
        }
        V::Name(w.to_string())
    }

    fn term(&self, e: &Expr, model: Option<&str>) -> Result<V, String> {
        let m = || model.map(str::to_string).ok_or_else(|| String::from("`model` has no value here"));
        Ok(match e {
            Expr::Var(v) if v == "model" => V::Model(model.unwrap_or_default().to_string()),
            Expr::Var(w) => self.word(w),
            Expr::Str(s) => V::Name(s.clone()),
            Expr::Int(n) => V::Num(*n),
            Expr::Field(_, f) if f == "class" && model.is_none() => V::Num(0),
            Expr::Field(_, f) if f == "class" => V::Num(self.number(&m()?)?),
            Expr::Field(_, f) if f == "family" && model.is_none() => V::Name(String::new()),
            Expr::Field(_, f) if f == "family" => {
                let m = m()?;
                V::Name(self.family.get(&m).cloned().ok_or_else(|| format!("{m} has no family the proxy says"))?)
            }
            Expr::Field(_, f) if f == "name" => V::Name(model.unwrap_or_default().to_string()),
            e => return Err(format!("`{}` is no model, class, family, name or number", docket_machine::print::expr(e))),
        })
    }

    fn num(&self, v: &V) -> Result<i64, String> {
        match v {
            V::Num(n) => Ok(*n),
            V::Model(m) => self.number(m),
            V::Name(s) => Err(format!("\"{s}\" is a name, not ordered")),
        }
    }

    /// Whether COND holds of MODEL (none: only its names checked, every comparison taken to hold); why it cannot be
    /// told (a name the policy does not know, a model with no number).
    fn holds(&self, cond: &Expr, model: Option<&str>) -> Result<bool, String> {
        match cond {
            Expr::And(a, b) => Ok(self.holds(a, model)? & self.holds(b, model)?),
            Expr::Cmp { l, r, eq } => {
                let (a, b) = (self.term(l, model)?, self.term(r, model)?);
                if model.is_none() {
                    // `model == W`: W a class, a model or a number; a family's name is compared with `model.family`
                    let is_model = |e: &Expr| matches!(e, Expr::Var(v) if v == "model");
                    for (e, v) in [(&**l, &b), (&**r, &a)] {
                        if let (true, V::Name(w)) = (is_model(e), v) {
                            return Err(format!("`{}`: `{w}` is no class ({}) and no model the policy orders", docket_machine::print::expr(cond), self.class.keys().cloned().collect::<Vec<_>>().join(", ")));
                        }
                    }
                    return Ok(true);
                }
                let same = match (&a, &b) {
                    (V::Model(x), V::Model(y)) | (V::Model(x), V::Name(y)) | (V::Name(y), V::Model(x)) | (V::Name(x), V::Name(y)) => x == y,
                    (V::Model(_), V::Num(_)) | (V::Num(_), V::Model(_)) | (V::Num(_), V::Num(_)) => self.num(&a)? == self.num(&b)?,
                    _ => return Err(format!("`{}` compares a name with a number", docket_machine::print::expr(cond))),
                };
                Ok(same == *eq)
            }
            Expr::Rel { l, r, rel } => {
                let (a, b) = (self.term(l, model)?, self.term(r, model)?);
                if model.is_none() {
                    // each side ordered: a class, a model, a number
                    for v in [&a, &b] {
                        if let V::Name(s) = v {
                            return Err(format!("`{}`: `{s}` is no class ({}) and no model the policy orders", docket_machine::print::expr(cond), self.class.keys().cloned().collect::<Vec<_>>().join(", ")));
                        }
                    }
                    return Ok(true);
                }
                Ok(Rel::holds(*rel, self.num(&a)?, self.num(&b)?))
            }
            e => Err(format!("`{}` is no comparison", docket_machine::print::expr(e))),
        }
    }

    /// Whether MODEL meets COND; why it cannot be told.
    pub fn meets(&self, cond: &Expr, model: &str) -> Result<bool, String> {
        self.holds(cond, Some(model))
    }

    /// Why COND names something the policy does not know, if it does.
    pub fn check(&self, cond: &Expr) -> Result<(), String> {
        self.holds(cond, None).map(|_| ())
    }

    /// The models of MODELS that do not meet COND.
    pub fn unmet(&self, cond: &Expr, models: &[String]) -> Vec<String> {
        models.iter().filter(|m| !self.meets(cond, m).unwrap_or(false)).cloned().collect()
    }

    /// What `--model W` names: a model (itself), or a class (its number); why neither.
    pub fn asked(&self, w: &str) -> Result<Asked, String> {
        if let Some(n) = self.class.get(w) {
            return Ok(Asked::Class(w.to_string(), *n));
        }
        let m = w.split(':').next().unwrap_or(w);
        if self.tier.contains_key(m) || self.family.contains_key(m) {
            return Ok(Asked::Model(w.to_string()));
        }
        Err(format!("{w} is no model the proxy serves or the policy orders, and no class ({})", self.class.keys().cloned().collect::<Vec<_>>().join(", ")))
    }
}

/// A model asked for by name or by class (`--model`).
#[derive(Clone, Debug, PartialEq)]
pub enum Asked {
    Model(String),
    Class(String, i64),
}

/// The call's `where` (its text, as written) read; none when it has none.
pub fn cond_of(sig: &docket_machine::check::CallSig) -> Option<Result<Expr, String>> {
    sig.cond.as_deref().map(|c| docket_machine::syntax::model_cond(c, 1))
}

/// Whether SCOPES have a `model-for` rule of the project (or one of its agents) for the agents' models: a `Prefer` or
/// `Only` for `model-for` that is not only for another place than `agent` (`When call.place == "session"` is not).
pub fn project_rule(scopes: &[(Scope, Vec<Decl>)]) -> Option<String> {
    use crate::policy::syntax::{Lit, Op};
    scopes.iter().filter(|(s, _)| *s != Scope::Global).flat_map(|(_, ds)| ds).find_map(|d| {
        let when = match &d.kind {
            DeclKind::Prefer { decision, when, .. } | DeclKind::Only { decision, when, .. } if decision == "model-for" => when,
            _ => return None,
        };
        let elsewhere = when.iter().any(|l| match l {
            Lit::Cmp(PExpr::Call(f), Op::Eq, PExpr::Str(v)) | Lit::Cmp(PExpr::Str(v), Op::Eq, PExpr::Call(f)) => f == "place" && v != "agent",
            Lit::Cmp(PExpr::Call(f), Op::Ne, PExpr::Str(v)) | Lit::Cmp(PExpr::Str(v), Op::Ne, PExpr::Call(f)) => f == "place" && v == "agent",
            _ => false,
        });
        (!elsewhere).then(|| format!("{} ({})", d.text.lines().next().unwrap_or("").trim(), d.at))
    })
}

/// The model of AGENT, made for a step of CALL (or by `coord new`, CALL `coord new`) under COND (the call's `where`,
/// or a class asked for: `model.class == medium`): the policy's `model-for` with `place` = `agent`, the models the
/// proxy serves that do not meet COND left out (`where`). Refused when nothing says the model: no COND and no
/// model-for rule of the project for agents. Blocking (the proxy, clingo).
pub fn decide(places: &crate::policy::Places, agent: &str, call: &str, cond: Option<&str>) -> Result<coord_types::Decision, String> {
    let scopes = places.sources()?;
    let rule = project_rule(&scopes);
    let engine = crate::policy::Engine::start(places.clone());
    let served = engine.served()?;
    let order = Order::of(&scopes, &served);
    let mut fields = BTreeMap::from([("place".to_string(), "agent".to_string()), ("agent".to_string(), agent.to_string()), ("call".to_string(), call.to_string())]);
    let unmet = match cond {
        Some(c) => {
            let e = docket_machine::syntax::model_cond(c, 1)?;
            order.check(&e)?;
            fields.insert("where".into(), c.to_string());
            order.unmet(&e, &served.iter().map(|(m, _)| m.clone()).collect::<Vec<_>>())
        }
        None if rule.is_some() => Vec::new(),
        None => return Err(format!("{call} says no model (no `where`), and the project's policy has no model-for rule for agents")),
    };
    engine.decide_model(fields, unmet)
}

/// The effort the policy gives CALL (`effort-for(CALL, LEVEL)`, the same fact an LLM call's task reads, policy/mod.rs),
/// for an agent made for a step of CALL (paper-research N-101 p673, p677: Tovrin's `+effort-for("mind", "medium").`).
/// The key is the call's name; the code's own tasks that read the fact (intake, route, asks, shape) are each named as
/// the call they run, so one fact means one piece of work either way.
pub fn effort_of(places: &crate::policy::Places, call: &str) -> Result<Option<String>, String> {
    Ok(crate::policy::effort_for(&places.sources()?, call))
}

/// MODEL (of FAMILY, as the proxy says) at LEVEL, as roster takes it (`MODEL:LEVEL`), or why MODEL does not take LEVEL:
/// a Claude Code agent takes Claude Code's levels (`core::CLAUDE_EFFORTS`), a Codex agent its model's in Codex's catalog
/// (CODEX, `codex debug models`). A level the model does not take is refused, never dropped for a default.
pub fn at_effort(call: &str, model: &str, family: Option<&str>, level: &str, codex: Option<Result<Vec<String>, String>>) -> Result<String, String> {
    let levels: Vec<String> = match (family, codex) {
        (Some("claude"), _) => crate::core::CLAUDE_EFFORTS.iter().map(|l| l.to_string()).collect(),
        (Some("codex"), Some(Ok(l))) => l,
        (Some("codex"), Some(Err(why))) => return Err(format!("the policy gives call {call} effort {level}, and the levels {model} takes are not known: {why}")),
        (f, _) => return Err(format!("the policy gives call {call} effort {level}, and {model} ({}) takes no effort", f.unwrap_or("no family the proxy names"))),
    };
    if !levels.iter().any(|l| l == level) {
        return Err(format!("the policy gives call {call} effort {level}, which {model} does not take ({})", levels.join(", ")));
    }
    Ok(format!("{model}:{level}"))
}

/// Why decision D chose no model: its fault, and each model left out with why.
pub fn why_none(d: &coord_types::Decision) -> String {
    let mut by: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for x in &d.excluded {
        by.entry(x.option.as_str()).or_default().push(if x.reason == "where" { x.rule.clone() } else { format!("{}: {}", x.reason, x.rule) });
    }
    let out: Vec<String> = by.into_iter().map(|(m, rs)| format!("{m} ({})", rs.join("; "))).collect();
    format!("{}; left out: {}", d.fault.as_deref().unwrap_or("no option left"), if out.is_empty() { String::from("none") } else { out.join(", ") })
}

/// Whether a copy of FROM, which runs MODEL, may take a step of CALL, whose `where` is COND: a copy keeps the model of
/// what it copies, so one that does not meet the call is refused, not raised. SERVED: the proxy's models and families.
pub fn copy_meets(places: &crate::policy::Places, served: &[(String, String)], from: &str, model: &str, call: &str, cond: &str) -> Result<(), String> {
    let order = Order::read(places, served)?;
    let e = docket_machine::syntax::model_cond(cond, 1)?;
    let bare = model.split(':').next().unwrap_or(model);
    match order.meets(&e, bare) {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!("a copy keeps the model of what it copies: {from} runs {}, and call {call} needs `where {cond}`", order.show(bare))),
        Err(e) => Err(format!("a copy keeps {from}'s model {model}, and call {call}'s `where {cond}` cannot be told of it: {e}")),
    }
}
