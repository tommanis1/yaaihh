//! The instances a type's fields give: which are valid, which a rule derives, and which rules may fail on some.

use super::*;

impl<'a> Ev<'a> {
    pub(super) fn instantiate_domain(&mut self, d: &Id, dom: &Domain) -> R<Vec<Elem>> {
        match dom {
            Domain::Time => Ok((0..=self.state.time).map(Elem::Int).collect()),
            Domain::AnyString => Err(RtErr::Internal(format!("cannot enumerate all strings of type: {}", d))),
            Domain::AnyInt => Err(RtErr::Internal(format!("cannot enumerate all integers of type: {}", d))),
            Domain::Strings(ss) => Ok(ss.iter().map(|s| Elem::Str(s.clone())).collect()),
            Domain::Ints(is) => Ok(is.iter().map(|i| Elem::Int(*i)).collect()),
            Domain::Products(xs) => {
                let lists = self.sequence(xs.len(), &mut |ev, i| ev.every_valid_subs(&xs[i]))?;
                Ok(fmap(lists, |args| Elem::Prod(args.into())))
            }
        }
    }

    /// Every instance of a variable's type: all valid instances when the type can be enumerated,
    /// the instances that hold otherwise.
    pub fn every_valid_subs(&mut self, x: &Var) -> R<Vec<Tagged>> {
        let d = self.spec.type_of(x);
        let t = self.tspec(&d)?;
        if self.spec.enumerable(&t.domain) {
            let elems = self.instantiate_domain(&d, &t.domain)?;
            bind(elems, |e| {
                let te = Tagged { elem: e, dom: d.clone() };
                let b = self.bindings(&t, &te);
                let checks = self.with(b, |ev| ev.eval(&t.domain_constraint))?;
                Ok(checks.into_iter().filter(|v| *v == Value::Bool(true)).map(|_| te.clone()).collect())
            })
        } else {
            self.seen_type(&d);
            let (trues, falses) = self.view().assigned(&d);
            if self.spec.reading.ranging {
                let mut all: BTreeSet<Elem> = trues.into_iter().map(|te| te.elem).collect();
                all.extend(self.view().values_of(&d));
                return Ok(all.into_iter().map(|elem| Tagged { elem, dom: d.clone() }).collect());
            }
            if !t.is_closed() && trues.is_empty() && falses.is_empty() {
                return Err(RtErr::MissingInstances(d));
            }
            Ok(trues)
        }
    }

    /// Every instance an act or event type could be triggered with: as [`Ev::every_valid_subs`], but a
    /// composite type that cannot be enumerated is built from the possible instances of its fields.
    pub fn every_possible_subs(&mut self, x: &Var) -> R<Vec<Tagged>> {
        let d = self.spec.type_of(x);
        let t = self.tspec(&d)?;
        if self.spec.enumerable(&t.domain) {
            return self.every_valid_subs(x);
        }
        match &t.domain {
            Domain::Products(xs) => {
                let lists = self.sequence(xs.len(), &mut |ev, i| ev.every_possible_subs(&xs[i]))?;
                bind(lists, |args| {
                    let b: Vec<(Var, Tagged)> = xs.iter().cloned().zip(args.iter().cloned()).collect();
                    let checks = self.with(b, |ev| ev.eval(&t.domain_constraint))?;
                    let te = Tagged { elem: Elem::Prod(args.into()), dom: d.clone() };
                    Ok(checks.into_iter().filter(|v| *v == Value::Bool(true)).map(|_| te.clone()).collect())
                })
            }
            _ => {
                self.seen_type(&d);
                Ok(self.view().holding_of(&d))
            }
        }
    }

    pub fn is_valid_instance(&mut self, te: &Tagged) -> R<Vec<bool>> {
        let t = self.tspec(&te.dom)?;
        let shape_ok = match (&te.elem, &t.domain) {
            (Elem::Int(i), Domain::Ints(is)) => is.contains(i),
            (Elem::Int(_), Domain::AnyInt) => true,
            (Elem::Str(s), Domain::Strings(ss)) => ss.contains(s),
            (Elem::Str(_), Domain::AnyString) => true,
            (Elem::Prod(args), Domain::Products(params)) => args.len() == params.len(),
            _ => false,
        };
        if !shape_ok {
            return Ok(vec![false]);
        }
        let check = |ev: &mut Self| -> R<Vec<bool>> {
            let b = ev.bindings(&t, te);
            Ok(bools(ev.with(b, |ev| ev.eval(&t.domain_constraint))?))
        };
        match &te.elem {
            Elem::Prod(args) => {
                let lists = self.sequence(args.len(), &mut |ev, i| ev.is_valid_instance(&args[i]))?;
                let all = fmap(lists, |bs| bs.into_iter().all(|b| b));
                bind(all, |ok| if ok { check(self) } else { Ok(vec![false]) })
            }
            _ => check(self),
        }
    }

    /// Whether T's rules work out the one instance TE of a composite type on the state as it stands, as `saturate`
    /// would: a `+` line gives it, or each field is among the instances its type ranges over and a `Holds when` is true
    /// of the fields; its conditions met. `None` where the whole type is worked out instead: a `Derived from` rule, a
    /// rule that reads T itself, or a type that is not composite.
    pub fn derives(&mut self, te: &Tagged) -> R<Option<bool>> {
        let t = self.tspec(&te.dom)?;
        if !matches!(t.domain, Domain::Products(_)) {
            return Ok(None);
        }
        let (mut rules, mut stated) = (Vec::new(), Vec::new());
        for clause in &t.derivation {
            match clause {
                Derivation::HoldsWhen(c) => rules.push(c.clone()),
                Derivation::Dv(xs, s) if xs.is_empty() && literal(s) => stated.push(s),
                Derivation::Dv(..) => return Ok(None),
            }
        }
        let mut read = BTreeSet::new();
        rules.iter().chain(&t.conditions).for_each(|c| mentions(c, &mut read));
        if read.iter().any(|d| self.spec.chase(d) == te.dom) || self.may_fail(&te.dom, &t) {
            return Ok(None);
        }
        let mut any = false;
        for s in stated {
            any |= taggeds(self.eval(s)?).contains(te);
        }
        let b = self.bindings(&t, te);
        if !any {
            for (x, arg) in &b {
                if !self.ranges_over(x, arg)? {
                    return Ok(Some(false));
                }
            }
        }
        for c in &rules {
            any |= bools(self.with(b.clone(), |ev| ev.eval(c))?).into_iter().any(|v| v);
        }
        if !any {
            return Ok(Some(false));
        }
        Ok(Some(self.sat_conditions(te)?.into_iter().any(|v| v)))
    }

    /// Whether working out D's instances can fail on one instance and not on another, so that one instance worked out
    /// alone, or a rule tried only on the combinations its facts give, could answer where the whole type fails: a type
    /// its rules, conditions or fields read, or a `Where` of one of those reads, that is open (an instance not assigned
    /// is an error), or a division (by zero).
    pub(super) fn may_fail(&self, d: &Id, t: &TypeSpec) -> bool {
        let mut terms: Vec<&Term> = t.conditions.iter().chain([&t.domain_constraint]).collect();
        for clause in &t.derivation {
            match clause {
                Derivation::HoldsWhen(c) => terms.push(c),
                Derivation::Dv(_, c) => terms.push(c),
            }
        }
        self.reads_may_fail(Spec::params(d, t).iter().map(|v| v.dom.clone()).collect(), &terms)
    }

    /// Whether TERMS, read with fields of the types in TODO, can fail on one binding and not on another: they divide,
    /// or read a type, or a `Where` of one, that is open.
    pub(super) fn reads_may_fail(&self, mut todo: Vec<Id>, terms: &[&Term]) -> bool {
        if terms.iter().any(|c| divides(c)) {
            return true;
        }
        terms.iter().for_each(|c| {
            let mut m = BTreeSet::new();
            mentions(c, &mut m);
            todo.extend(m);
        });
        let mut seen = BTreeSet::new();
        while let Some(x) = todo.pop() {
            let x = self.spec.chase(&x);
            if !seen.insert(x.clone()) {
                continue;
            }
            let Some(xt) = self.spec.find(&x) else { return true };
            if !xt.is_closed() || divides(&xt.domain_constraint) {
                return true;
            }
            let mut m = BTreeSet::new();
            mentions(&xt.domain_constraint, &mut m);
            todo.extend(m);
            todo.extend(Spec::params(&x, xt).iter().map(|v| v.dom.clone()));
        }
        false
    }

    /// Whether D and the types of its fields, all the way down, have no `Where`: an instance of it is valid by its
    /// shape alone, and checking one cannot fail.
    pub(super) fn unconstrained(&self, d: &Id, depth: usize) -> bool {
        let Some(t) = self.spec.find(d) else { return false };
        if depth > 16 || t.domain_constraint != Term::BoolLit(true) {
            return false;
        }
        match &t.domain {
            Domain::Products(ps) => ps.iter().all(|p| self.unconstrained(&self.spec.type_of(p), depth + 1)),
            _ => true,
        }
    }
}

/// Whether T is an instance written out whole, reading nothing: what a `+` line gives.
pub(super) fn literal(t: &Term) -> bool {
    match t {
        Term::StringLit(_) | Term::IntLit(_) | Term::BoolLit(_) => true,
        Term::Tag(x, _) => literal(x),
        Term::App(_, Args::Pos(ts)) => ts.iter().all(literal),
        Term::App(_, Args::Mods(ms)) => ms.iter().all(|(_, t)| literal(t)),
        _ => false,
    }
}
