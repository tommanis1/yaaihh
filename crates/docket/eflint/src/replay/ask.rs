//! A specification given as syntax, and questions put to its state as terms: whether one holds, and every binding
//! under which it does.

use super::*;
use crate::compile::{Spec, Static, Ty, free};
use crate::eval::{Assign, RtErr, Value as Got, mentions, reads, truth, undecided_why};

impl Replay {
    /// SPEC with DECLS declared in it; refused if a declaration does not check or its derived types have no single
    /// answer. SPEC says how types are worked out, its host table and its tests.
    pub fn declare(decls: Vec<Decl>, spec: Spec) -> Result<Replay, String> {
        let mut r = Replay { cfg: Config::default(), stale: BTreeSet::new(), plan: None, tried: 0 };
        r.cfg.spec = spec;
        let (outs, done) = step_in_place(&mut r.cfg, &mut r.stale, &Phrase::Decls(decls));
        let mut a = Answer::default();
        take_outputs(outs, &mut a);
        if !done || !a.errors.is_empty() {
            return Err(a.errors.join("\n"));
        }
        freshen(&mut r.cfg, &mut r.stale, None).map_err(|e| e.message())?;
        Ok(r)
    }

    /// The stored facts of FROM in place of this one's, those of a type this one does not declare left out: a
    /// specification declared again keeps its state. What was left out, as a step [`Replay::undo`] takes back.
    pub fn keep_stored(&mut self, from: &Replay) -> Option<Taken> {
        let mut was = Vec::new();
        for (te, i) in from.cfg.state.contents().iter().filter(|(_, i)| !i.derived) {
            match self.cfg.spec.decls.contains_key(&te.dom) {
                true => self.cfg.state.insert(te.clone(), i.clone()),
                false => was.push((te.clone(), Some(i.clone()))),
            }
        }
        self.stale.extend(derived_types(&self.cfg.spec));
        let store = was.iter().map(|(te, _)| (te.clone(), Assign::Unknown)).collect();
        (!was.is_empty()).then(|| Taken { executed: Vec::new(), created: Vec::new(), terminated: Vec::new(), store, was })
    }

    /// Whether T holds, its free variables read as `Exists`; an error where it turns on unknown instances.
    pub fn ask(&mut self, t: &Term) -> Result<bool, String> {
        self.truth(t)?.ok_or_else(|| undecided_why(&self.cfg.spec, &self.cfg.state))
    }

    /// Whether T holds, its free variables read as `Exists`; `None` where it turns on unknown instances.
    pub fn truth(&mut self, t: &Term) -> Result<Option<bool>, String> {
        let t = self.compiled(t, &[])?;
        let mut named = BTreeSet::new();
        mentions(&t, &mut named);
        self.read(named)?;
        truth(&self.cfg.spec, &self.cfg.state, &t).map_err(|e| e.message())
    }

    /// Every binding of XS under which T holds, in the order of values; T's other free variables read as `Exists`.
    /// Guided, those of XS that T's facts give are bound from their held instances, looked up by the fields given or
    /// bound so far, and only the others range over their types ([`Ev::foreach_where`]); else every binding is tried.
    pub fn ways(&mut self, xs: &[Var], t: &Term) -> Result<Vec<Vec<Tagged>>, String> {
        self.ways_bounded(xs, t, usize::MAX, u64::MAX)
    }

    /// At most N of the bindings [`Replay::ways`] gives, in the order of values: none is tried once N are found.
    pub fn ways_upto(&mut self, xs: &[Var], t: &Term, n: usize) -> Result<Vec<Vec<Tagged>>, String> {
        self.ways_bounded(xs, t, n, u64::MAX)
    }

    /// [`Replay::ways`], refused once it has tried more than STEPS bindings, counted in its counts too.
    pub fn ways_within(&mut self, xs: &[Var], t: &Term, steps: u64) -> Result<Vec<Vec<Tagged>>, String> {
        self.ways_bounded(xs, t, usize::MAX, steps)
    }

    /// At most N of the bindings [`Replay::ways`] gives, refused past STEPS bindings tried.
    pub fn ways_bounded(&mut self, xs: &[Var], t: &Term, n: usize, steps: u64) -> Result<Vec<Vec<Tagged>>, String> {
        let t = self.compiled(t, xs)?;
        let mut named: BTreeSet<Id> = xs.iter().map(|x| x.dom.clone()).collect();
        mentions(&t, &mut named);
        self.read(named)?;
        let mut ev = Ev::new(&self.cfg.spec, &self.cfg.state).within(steps);
        let (mut out, mut tried) = (BTreeSet::new(), 0u64);
        let mut row = |ev: &mut Ev| {
            if out.len() >= n {
                return Ok(Vec::<()>::new());
            }
            tried += 1;
            if !ev.eval(&t)?.contains(&Got::Bool(true)) {
                return Ok(Vec::new());
            }
            let mut row = Vec::with_capacity(xs.len());
            for x in xs {
                match ev.eval(&Term::Ref(x.clone()))?.pop() {
                    Some(Got::Tagged(te)) => row.push(te),
                    other => return Err(RtErr::Internal(format!("{} is bound to {other:?}", x.dom))),
                }
            }
            out.insert(row);
            Ok(Vec::new())
        };
        let got = match self.cfg.spec.reading.guided {
            true => ev.foreach_where(xs, &t, &mut row),
            false => ev.foreach(xs, &mut row),
        };
        self.tried += tried;
        got.map_err(|e| e.message())?;
        Ok(out.into_iter().collect())
    }

    /// How many bindings [`Replay::ways`] has tried T under, all its calls together: what its cost goes with, held to
    /// the rows T's facts match (N-1832 p305).
    pub fn tried(&self) -> u64 {
        self.tried
    }

    /// Whether instance TE holds, what its type reads worked out first.
    pub fn holds(&mut self, te: &Tagged) -> Result<bool, String> {
        self.read(BTreeSet::from([te.dom.clone()]))?;
        Ok(self.cfg.state.holds(te))
    }

    /// T checked and converted to a condition, its free variables but BOUND quantified by `Exists`.
    fn compiled(&mut self, t: &Term, bound: &[Var]) -> Result<Term, String> {
        let others: Vec<Var> = free(&self.cfg.spec, t).into_iter().filter(|v| !bound.contains(v)).collect();
        let t = if others.is_empty() { t.clone() } else { Term::Q(Quant::Exists, others, Box::new(t.clone())) };
        Static { spec: &mut self.cfg.spec }.convert(&t, &Ty::Bool).map_err(|e| e.join("; "))
    }

    /// The types NAMED and what they read worked out, where stale.
    fn read(&mut self, named: BTreeSet<Id>) -> Result<(), String> {
        let read = reads(&self.cfg.spec, named);
        freshen(&mut self.cfg, &mut self.stale, Some(&read)).map_err(|e| e.message())
    }
}
