//! A count or a sum over the instances of one fact, kept up to date a step at a time: each instance a step changes
//! adds its part or takes it away, where the type would otherwise be worked out again over every instance.

use super::delta::Delta;
use super::subs::literal;
use super::*;

/// One field of the fact counted: a variable the binding gives it, or a value it must have.
enum Field {
    Var(Var),
    Is(Tagged),
}

/// A derived type of one field set to `Count` or `Sum` of `V When F(..)` over F's variables: each binding is one
/// instance of F, and V reads nothing but the binding.
pub(super) struct Tally {
    sum: bool,
    v: Term,
    fact: Id,
    fields: Vec<Field>,
}

impl Tally {
    /// D's rule as a tally, where it is one: one rule `D(Q(xs, V When F(..)))`, Q a count or a sum, every one of XS
    /// a field of F of its type, F's other fields values, V one of XS or a value; no condition or constraint on D. F
    /// is stored, or derived only as values put in, so each instance of it holds or does not: none is left unknown.
    pub(super) fn of(ev: &Ev, d: &Id, t: &TypeSpec) -> Option<Tally> {
        let spec = ev.spec;
        let Domain::Products(heads) = &t.domain else { return None };
        if heads.len() != 1 || !t.conditions.is_empty() || t.domain_constraint != Term::BoolLit(true) {
            return None;
        }
        let [Derivation::Dv(vs, term)] = &t.derivation[..] else { return None };
        let head = match term {
            Term::Bin(BinOp::When, h, c) if **c == Term::BoolLit(true) => &**h,
            h => h,
        };
        let Term::App(h, hargs) = head else { return None };
        let given_head = placed(heads, hargs);
        // the total is tagged with its field's type, a sum's body untagged to numbers
        let agg = match &given_head[..] {
            [Term::Tag(x, _)] => &**x,
            [x] => x,
            _ => return None,
        };
        let Term::Q(q, xs, body) = agg else { return None };
        let (body, untag) = match &**body {
            Term::Un(UnOp::Untag, b) => (&**b, true),
            b => (b, false),
        };
        let sum = match q {
            Quant::Count => false,
            Quant::Sum => true,
            _ => return None,
        };
        let Term::Bin(BinOp::When, v, c) = body else { return None };
        let app = match &**c {
            Term::Un(UnOp::Present, a) => &**a,
            a => a,
        };
        let Term::App(f, fargs) = app else { return None };
        let ft = spec.find(f)?;
        let Domain::Products(params) = &ft.domain else { return None };
        if !vs.is_empty() || spec.chase(h) != *d || !ft.derivation.iter().all(put_in) || spec.tests.contains(f) || spec.chase(f) == *d {
            return None;
        }
        let mut empty = Ev::new(spec, ev.state);
        let mut fields = Vec::new();
        for (a, p) in placed(params, fargs).iter().zip(params) {
            fields.push(match a {
                Term::Ref(x) if xs.contains(x) && spec.type_of(x) == spec.type_of(p) => Field::Var(x.clone()),
                a if literal(a) => Field::Is(taggeds(empty.eval(a).ok()?).pop()?),
                _ => return None,
            });
        }
        let given = |x: &Var| fields.iter().any(|f| matches!(f, Field::Var(y) if y == x));
        let reads_binding_only = matches!(&**v, Term::Ref(x) if given(x)) || literal(v);
        let v = if untag { Term::Un(UnOp::Untag, v.clone()) } else { (**v).clone() };
        (xs.iter().all(given) && reads_binding_only).then(|| Tally { sum, v, fact: spec.chase(f), fields })
    }

    /// The binding of the counted variables instance TE of the fact gives; none where TE is not one counted.
    fn binding(&self, te: &Tagged) -> Option<Vec<(Var, Tagged)>> {
        let Elem::Prod(args) = &te.elem else { return None };
        if args.len() != self.fields.len() {
            return None;
        }
        let mut b: Vec<(Var, Tagged)> = Vec::new();
        for (a, f) in args.iter().zip(&self.fields) {
            match f {
                Field::Is(want) if want.elem != a.elem => return None,
                Field::Is(_) => {}
                Field::Var(x) => match b.iter().find(|(y, _)| y == x) {
                    Some((_, got)) if got.elem != a.elem => return None,
                    Some(_) => {}
                    None => b.push((x.clone(), a.clone())),
                },
            }
        }
        Some(b)
    }

    /// D brought up to date after the changes in DELTA: each instance of the fact that came adds its part, each that
    /// went takes it away; DELTA gains the total before and the total now. Whether the total moved.
    pub(super) fn update(&self, spec: &Spec, state: &mut State, d: &Id, delta: &mut Delta) -> R<bool> {
        let mut by = 0i64;
        let mut ev = Ev::new(spec, state);
        for (te, was) in delta.iter() {
            if spec.chase(&te.dom) != self.fact || state.holds(te) == *was {
                continue;
            }
            let Some(b) = self.binding(te) else { continue };
            let vs = ev.with(b, |ev| ev.eval(&self.v))?;
            let part = match self.sum {
                false => vs.len() as i64,
                true => vs.iter().try_fold(0i64, |s, v| match v {
                    Value::Int(k) | Value::Tagged(Tagged { elem: Elem::Int(k), .. }) => Ok(s.wrapping_add(*k)),
                    v => Err(RtErr::Internal(format!("{d} sums {v:?}, no number"))),
                })?,
            };
            by = if *was { by.wrapping_sub(part) } else { by.wrapping_add(part) };
        }
        if by == 0 {
            return Ok(false);
        }
        let held: Vec<Tagged> = state.instances_of(d).filter(|te| state.holds(te)).collect();
        let [old] = &held[..] else { return Err(RtErr::Internal(format!("{d} holds {} totals, not one", held.len()))) };
        let Elem::Prod(fs) = &old.elem else { return Err(RtErr::Internal(format!("{old} is no total"))) };
        let [Tagged { elem: Elem::Int(n), dom }] = &fs[..] else { return Err(RtErr::Internal(format!("{old} is no total"))) };
        let now = Tagged { elem: Elem::Prod(Rc::from([Tagged { elem: Elem::Int(n.wrapping_add(by)), dom: dom.clone() }])), dom: old.dom.clone() };
        let old = old.clone();
        state.remove(&old);
        state.insert(now.clone(), Info { value: true, derived: true });
        delta.entry(old).or_insert(true);
        delta.entry(now).or_insert(false);
        Ok(true)
    }
}

/// Whether rule C only puts in a value: one that reads nothing, so it holds whatever else does.
fn put_in(c: &Derivation) -> bool {
    match c {
        Derivation::Dv(xs, Term::Bin(BinOp::When, t, w)) => xs.is_empty() && **w == Term::BoolLit(true) && literal(t),
        Derivation::Dv(xs, t) => xs.is_empty() && literal(t),
        Derivation::HoldsWhen(_) => false,
    }
}
