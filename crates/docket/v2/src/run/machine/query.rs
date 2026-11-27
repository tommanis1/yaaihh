//! Reads from outside, as no run reads: instance queries, whether given acts are enabled, and violated duties.

use super::*;
use crate::run::norms::Moved;
use crate::syntax::lex::At;

/// A Fact or a Duty, the kinds whose instances hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holds {
    Fact,
    Duty,
}

/// Whether an act was enabled, and why not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Enabled {
    Yes,
    No(String),
}

impl<N: Norms> Machine<N> {
    /// The values of `Foreach` query `q`, in the order of values, refused past STEPS bindings tried; its read is
    /// dropped, so no run's key holds it.
    pub fn inst_query(&mut self, q: &Expr, steps: u64) -> Result<Vec<Value>, Fault> {
        let E::Foreach(vs, v, c) = &q.e else { return Err("an instance query is a `Foreach`".into()) };
        self.norms.cap(Some(steps));
        let got = self.foreach(vs, v, c.as_deref(), &Env::new());
        self.norms.cap(None);
        self.norms.reads();
        got
    }

    /// For each act in ACTS, whether ACTOR may perform it now, worked out for that instance alone and taken back.
    pub fn enabled_for(&mut self, actor: &Value, acts: &[Instance]) -> Result<Vec<Enabled>, Fault> {
        let mut out = Vec::new();
        for a in acts {
            if !matches!(self.loaded.syms.get(&a.ty), Some(Sym::Norm { kind: Norm::Act, .. })) {
                self.norms.reads();
                return Err(format!("{} is no Act of the loaded text", a.ty));
            }
            out.push(match self.norms.step(a, Some(actor)) {
                Ok(_) => Enabled::Yes,
                Err(Why::Policy(w)) => Enabled::No(w),
                Err(Why::Closes(d)) => Enabled::No(format!("it would close {d}")),
            });
        }
        self.norms.reads();
        Ok(out)
    }

    /// What moved since this was last asked, of Fact and Duty types only ([`Moved`]); the first ask starts the count.
    pub fn moved(&mut self) -> Moved {
        let mut m = self.norms.moved();
        let held = |ty: &str| matches!(self.loaded.syms.get(ty), Some(Sym::Norm { kind: Norm::Fact | Norm::Duty, .. }));
        m.made.retain(|i| held(&i.ty));
        m.ended.retain(|i| held(&i.ty));
        m.whole.retain(|t| held(t));
        m
    }

    /// Every Fact and Duty type of the loaded text, by name.
    pub fn holding_types(&self) -> Vec<(String, Holds)> {
        let kind = |s: &Sym| match s {
            Sym::Norm { kind: Norm::Fact, .. } => Some(Holds::Fact),
            Sym::Norm { kind: Norm::Duty, .. } => Some(Holds::Duty),
            _ => None,
        };
        self.loaded.syms.iter().filter_map(|(n, s)| kind(s).map(|k| (n.clone(), k))).collect()
    }

    /// The instances of Duty `ty` that hold and are violated now, in the order of values, as no run reads it.
    pub fn violated(&mut self, ty: &str) -> Result<Vec<Instance>, String> {
        let n = match self.loaded.syms.get(ty) {
            Some(Sym::Norm { kind: Norm::Duty, fields: Fields::By(_), .. }) => 1,
            Some(Sym::Norm { kind: Norm::Duty, fields: Fields::Named(fs), .. }) => fs.len(),
            _ => return Err(format!("{ty} is no Duty of the loaded text")),
        };
        let xs: Vec<String> = (0..n).map(|k| format!("x{k}")).collect();
        let var = |x: &String| Expr { at: At::default(), e: E::Var(x.clone()) };
        let duty = Expr { at: At::default(), e: E::Apply(Path::one(ty), xs.iter().map(var).collect()) };
        let c = Expr { at: At::default(), e: E::Bin(Op::And, Box::new(duty.clone()), Box::new(Expr { at: At::default(), e: E::Violated(Box::new(duty)) })) };
        let rows = self.norms.each(&c, &Env::new());
        self.norms.reads();
        Ok(rows?.into_iter().map(|mut env| Instance { ty: ty.to_string(), args: xs.iter().filter_map(|x| env.remove(x)).collect() }).collect())
    }
}
