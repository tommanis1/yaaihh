//! Guided derivation: the bindings of a rule, a quantifier or an ending effect found from the instances of the facts
//! it names.

use super::*;

impl<'a> Ev<'a> {
    /// `Exists XS : C` answered from the held instances of the facts C opens with ([`Ev::bound_by_facts`]). `None`
    /// where this does not apply: every binding is tried then.
    pub(super) fn exists_from_facts(&mut self, xs: &[Var], body: &Term) -> R<Option<bool>> {
        let Some(found) = self.bound_by_facts(xs, body)? else { return Ok(None) };
        let mut any = false;
        for binding in found {
            any |= self.with(binding, |ev| ev.eval(body))?.iter().any(|v| *v == Value::Bool(true));
        }
        Ok(Some(any))
    }

    /// The values of `V When C` over every binding of XS, as many as trying every binding gives, from the held
    /// instances of the facts C opens with ([`Ev::bound_by_facts`]), where V cannot fail: a binding they skip gives no
    /// value. `None` where this does not apply.
    pub(super) fn values_from_facts(&mut self, xs: &[Var], a: &Term) -> R<Option<Vec<Value>>> {
        let Term::Bin(BinOp::When, v, c) = a else { return Ok(None) };
        if self.reads_may_fail(Vec::new(), &[v]) {
            return Ok(None);
        }
        let Some(found) = self.bound_by_facts(xs, c)? else { return Ok(None) };
        let mut out = Vec::new();
        for binding in found {
            out.extend(self.with(binding, |ev| ev.eval(a))?);
        }
        Ok(Some(out))
    }

    /// What `Terminates` or `Obfuscates F(..) [When C]` ends, from the held instances of F alone
    /// ([`Ev::held_bindings`]): ending an instance of a closed F no rule works out that does not hold changes no
    /// answer, and C, which cannot fail, is then evaluated on no other binding. `None` where this does not apply. A
    /// later `Open` on F leaves those instances unknown where the reference has them false; Docket opens no type.
    pub(super) fn effect_from_facts(&mut self, e: &Effect) -> R<Option<Vec<Tagged>>> {
        let (app, cond) = match &e.term {
            Term::Bin(BinOp::When, a, c) => (&**a, Some(&**c)),
            t => (t, None),
        };
        let derived = matches!(app, Term::App(f, _) if self.spec.find(f).is_some_and(|t| !t.derivation.is_empty()));
        let cond_may_fail = cond.is_some_and(|c| self.reads_may_fail(Vec::new(), &[c]));
        if e.kind == EffKind::Create || e.vars.is_empty() || derived || cond_may_fail {
            return Ok(None);
        }
        let Some(found) = self.held_bindings(&e.vars, &[app])? else { return Ok(None) };
        let mut out = Vec::new();
        for binding in found {
            out.extend(self.with(binding, |ev| Ok(taggeds(ev.eval(&e.term)?)))?);
        }
        Ok(Some(out))
    }

    /// The bindings of XS under which the facts C opens with, its first conjuncts `F(..)`, all hold
    /// ([`Ev::held_bindings`]): under any other binding one of them is false, those before it cannot fail, and `&&`
    /// evaluates nothing after it.
    fn bound_by_facts(&mut self, xs: &[Var], c: &Term) -> R<Option<Vec<Vec<(Var, Tagged)>>>> {
        self.held_bindings(xs, &opening(c))
    }

    /// The bindings of XS under which the first of APPS that [`Ev::fact_fields`] takes are all held instances, in the
    /// order every binding is tried in ([`Ev::joined`]). `None` unless each of XS is a field of one of them.
    pub(super) fn held_bindings(&mut self, xs: &[Var], apps: &[&Term]) -> R<Option<Vec<Vec<(Var, Tagged)>>>> {
        let mut facts = Vec::new();
        for app in apps {
            let Some(fact) = self.fact_fields(xs, app)? else { break };
            facts.push(fact);
        }
        if (0..xs.len()).any(|j| !gives(&facts, j)) {
            return Ok(None);
        }
        Ok(self.joined(xs, &facts)?.map(|(found, _)| found))
    }

    /// The results of M for every binding of XS under which C may hold, in the order they are found, not
    /// [`Ev::foreach`]'s: those of XS that C's facts give ([`Ev::bindings_where`]) are bound from their held instances,
    /// the others range over their types. Every binding is tried where this does not apply.
    pub fn foreach_where<T>(&mut self, xs: &[Var], c: &Term, m: &mut dyn FnMut(&mut Self) -> R<Vec<T>>) -> R<Vec<T>> {
        let Some((found, rest)) = self.bindings_where(xs, c)? else { return self.foreach(xs, m) };
        let mut out = Vec::new();
        for binding in found {
            out.extend(self.with(binding, |ev| ev.foreach(&rest, m))?);
        }
        Ok(out)
    }

    /// The bindings of those of XS that fields of C's facts give, under which those facts all hold ([`Ev::joined`]),
    /// and the rest of XS. C's facts are its conjuncts `F(..)` that [`Ev::fact_fields`] takes, each after conjuncts
    /// that cannot fail, in C or under an `Exists` around it whose variables range without failing. Under any binding
    /// they skip one of them is false, so C is, and nothing evaluated before it can fail; no range of XS can fail either
    /// ([`Ev::ranges_safely`]): trying every binding gives no other answer and no error. `None` where a range can fail,
    /// or C has no such fact.
    fn bindings_where(&mut self, xs: &[Var], c: &Term) -> R<Option<(Vec<Vec<(Var, Tagged)>>, Vec<Var>)>> {
        let body = match c {
            Term::Q(Quant::Exists, ys, body) if ys.iter().all(|y| self.ranges_safely(y)) => &**body,
            c => c,
        };
        if !xs.iter().all(|x| self.ranges_safely(x)) {
            return Ok(None);
        }
        let mut cs = Vec::new();
        conjuncts(body, &mut cs);
        let mut facts = Vec::new();
        for c in cs {
            let fact = match c {
                Term::Un(UnOp::Present, app) => self.fact_fields(xs, app)?,
                _ => None,
            };
            match fact {
                Some(fact) => facts.push(fact),
                None if self.reads_may_fail(Vec::new(), &[c]) => break,
                None => {}
            }
        }
        if facts.is_empty() {
            return Ok(None);
        }
        self.joined(xs, &facts)
    }

    /// Whether X's range, what [`Ev::foreach`] binds it to, cannot fail: a closed type's instances that hold and,
    /// ranging, any type's with the values fields take; an enumerable type's instances where no `Where` is on it or on
    /// its fields' types.
    fn ranges_safely(&self, x: &Var) -> bool {
        let d = self.spec.type_of(x);
        let Some(t) = self.spec.find(&d) else { return false };
        match self.spec.enumerable(&t.domain) {
            true => self.unconstrained(&d, 0),
            false => self.spec.reading.ranging || t.is_closed(),
        }
    }

    /// The bindings of those of XS that FACTS' fields give under which FACTS all hold, in the order every binding is
    /// tried in, joined one after the other through the index of instances by field; and the rest of XS. `None` where
    /// one that FACTS give is of an open type and the reading is not ranging, or its type's instances cannot be listed.
    fn joined(&mut self, xs: &[Var], facts: &[(Id, Vec<Result<usize, Elem>>)]) -> R<Option<(Vec<Vec<(Var, Tagged)>>, Vec<Var>)>> {
        // each of XS that FACTS give: the instances it ranges over, in the order they are tried (`None`: those that
        // hold and, ranging, the values fields take)
        let mut ranges: Vec<Option<Vec<Elem>>> = Vec::new();
        let mut rest = Vec::new();
        for (j, x) in xs.iter().enumerate() {
            let Some(xt) = self.spec.find(&self.spec.type_of(x)).cloned() else { return Ok(None) };
            if !gives(facts, j) {
                rest.push(x.clone());
                ranges.push(None);
            } else if self.spec.enumerable(&xt.domain) {
                match self.every_valid_subs(x) {
                    Ok(tes) => ranges.push(Some(tes.into_iter().map(|te| te.elem).collect())),
                    Err(_) => return Ok(None),
                }
            } else if xt.is_closed() || self.spec.reading.ranging {
                ranges.push(None);
            } else {
                return Ok(None);
            }
        }
        // the bindings each fact in turn allows, from its held instances with the fields given or bound so far
        let state = self.view();
        let mut partial: Vec<Vec<Option<Elem>>> = vec![vec![None; xs.len()]];
        for (f, fields) in facts {
            let mut next = Vec::new();
            for b in &partial {
                let given = |n: usize| match &fields[n] {
                    Err(e) => Some(e),
                    Ok(j) => b[*j].as_ref(),
                };
                let narrowest = (0..fields.len())
                    .filter_map(|n| given(n).map(|e| (n, e)))
                    .min_by_key(|(n, e)| state.with_field_count(f, *n, e));
                self.seen_type(f);
                let candidates: Vec<Tagged> = match narrowest {
                    Some((n, e)) => state.with_field(f, n, e),
                    None => state.instances_of(f),
                };
                'cand: for te in candidates {
                    self.tick()?;
                    let Elem::Prod(args) = &te.elem else { continue };
                    if args.len() != fields.len() || !state.holds(&te) {
                        continue;
                    }
                    let mut bound = b.clone();
                    for (a, field) in args.iter().zip(fields) {
                        match field {
                            Err(e) if a.elem != *e => continue 'cand,
                            Err(_) => {}
                            Ok(j) => {
                                if bound[*j].as_ref().is_some_and(|e| *e != a.elem) {
                                    continue 'cand;
                                }
                                bound[*j] = Some(a.elem.clone());
                            }
                        }
                    }
                    next.push(bound);
                }
            }
            partial = next;
        }
        let mut found: Vec<(Vec<(usize, Option<Elem>)>, Vec<(Var, Tagged)>)> = Vec::new();
        'b: for bound in partial {
            let mut key = Vec::new();
            let mut binding = Vec::new();
            for ((x, e), range) in xs.iter().zip(bound).zip(&ranges) {
                // one of the rest, which no fact gives, is bound by none
                let Some(e) = e else { continue };
                let v = Tagged { elem: e.clone(), dom: self.spec.type_of(x) };
                match range {
                    Some(r) => match r.iter().position(|y| *y == e) {
                        Some(i) => key.push((i, None)),
                        None => continue 'b,
                    },
                    None if state.holds(&v) || (self.spec.reading.ranging && state.occurs(&v)) => {
                        self.seen_type(&v.dom);
                        key.push((0, Some(e)))
                    }
                    None => continue 'b,
                }
                binding.push((x.clone(), v));
            }
            found.push((key, binding));
        }
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found.dedup_by(|a, b| a.0 == b.0);
        Ok(Some((found.into_iter().map(|(_, binding)| binding).collect(), rest)))
    }

    /// APP's type F, and each of its fields' terms as one of XS, by itself, or the one instance it is given. `None`
    /// where F is no closed composite type, is a test or has a `Where` on it or its fields' types, or a field's term
    /// names one of XS otherwise, or is not one instance: working out `F(..)` cannot fail where this is given.
    fn fact_fields(&mut self, xs: &[Var], app: &Term) -> R<Option<(Id, Vec<Result<usize, Elem>>)>> {
        let Term::App(f, args) = app else { return Ok(None) };
        let Some(t) = self.spec.find(f).cloned() else { return Ok(None) };
        let Domain::Products(params) = &t.domain else { return Ok(None) };
        let test = self.spec.tests.contains(f);
        if !t.is_closed() || self.spec.enumerable(&t.domain) || test || !self.unconstrained(f, 0) {
            return Ok(None);
        }
        if let Args::Mods(ms) = args {
            if ms.iter().enumerate().any(|(i, (v, _))| !params.contains(v) || ms[..i].iter().any(|(w, _)| w == v)) {
                return Ok(None);
            }
        }
        let mut fields: Vec<Result<usize, Elem>> = Vec::new();
        for term in &placed(params, args) {
            match term {
                Term::Ref(v) if xs.contains(v) => fields.push(Ok(xs.iter().position(|x| x == v).unwrap())),
                term if names_any(term, xs) => return Ok(None),
                term => match self.eval(term).map(taggeds) {
                    Ok(tes) if tes.len() == 1 => fields.push(Err(tes[0].elem.clone())),
                    _ => return Ok(None),
                },
            }
        }
        Ok(Some((f.clone(), fields)))
    }
}

/// Whether field J of the variables is a field of one of FACTS.
fn gives(facts: &[(Id, Vec<Result<usize, Elem>>)], j: usize) -> bool {
    facts.iter().any(|(_, fields)| fields.contains(&Ok(j)))
}

/// The facts T opens with, its first conjuncts `F(..)`.
pub(super) fn opening(t: &Term) -> Vec<&Term> {
    let mut cs = Vec::new();
    conjuncts(t, &mut cs);
    cs.into_iter()
        .map_while(|c| match c {
            Term::Un(UnOp::Present, a) => Some(&**a),
            _ => None,
        })
        .collect()
}

/// The term each of PARAMS is given in an application with ARGS: the given one, the others by name.
pub(super) fn placed(params: &[Var], args: &Args) -> Vec<Term> {
    let given: Vec<(&Var, &Term)> = match args {
        Args::Mods(ms) => ms.iter().map(|(v, t)| (v, t)).collect(),
        Args::Pos(ts) => params.iter().zip(ts.iter()).collect(),
    };
    params.iter().map(|p| given.iter().rev().find(|(v, _)| *v == p).map_or_else(|| Term::Ref(p.clone()), |(_, t)| (*t).clone())).collect()
}

/// A partial binding of a rule's fields: `None` where a condition says nothing about the field.
pub(super) type Partial = Vec<Option<Tagged>>;

/// The terms T joins with `&&`, in order, into OUT.
pub(super) fn conjuncts<'t>(t: &'t Term, out: &mut Vec<&'t Term>) {
    match t {
        Term::Bin(BinOp::And, l, r) => {
            conjuncts(l, out);
            conjuncts(r, out);
        }
        t => out.push(t),
    }
}

/// For a rule `T(x1..xn) Holds when c1 && c2 && ...`: bindings of some of the fields, taken from
/// the instances that hold of the facts the conditions name, and the fields left to enumerate.
/// A condition is used when it is, for a closed fact type `F` of several fields whose instances
/// cannot be listed from its fields' types alone,
///
/// - `F(a1..am)`, each argument a field of `T`, a literal, or a variable of an `Exists` around it;
/// - `(Exists f : f.p == x && ...)` for `f` of type `F` and a field `x` of `T`;
/// - `c || c'` where both are of these forms and say something about the same fields.
///
/// The rule's whole condition is still evaluated for each binding, so the instances found are
/// those of the full enumeration: a combination that no instance of such a fact matches makes
/// the conjunction false there. A runtime error on such a combination would not be met, so a rule
/// that can fail on one combination and not on another (`Ev::may_fail`: it divides, or reads an
/// open type) is not given to this. `None` when no condition has these shapes, or a field has no
/// instance (the full enumeration then reports what it must).
pub(super) fn guided_bindings(spec: &Spec, s: &State, xs: &[Var], c: &Term) -> Option<(Vec<Vec<(Var, Tagged)>>, Vec<Var>)> {
    let mut cs = Vec::new();
    conjuncts(c, &mut cs);
    let mut valid: Vec<BTreeSet<Tagged>> = Vec::new();
    for x in xs {
        let subs = Ev::plain(spec, s).every_valid_subs(x).ok()?;
        if subs.is_empty() {
            // a closed type without instances: no combination exists, nothing is derived
            let closed = spec.find(&spec.type_of(x)).map_or(false, |t| t.is_closed());
            return if closed { Some((Vec::new(), Vec::new())) } else { None };
        }
        valid.push(subs.into_iter().collect());
    }
    let mut starts: BTreeSet<Partial> = BTreeSet::new();
    starts.insert(vec![None; xs.len()]);
    let mut used = false;
    for c in cs {
        let Some(matches) = guide(spec, s, xs, &valid, c) else { continue };
        let mut next: BTreeSet<Partial> = BTreeSet::new();
        let mut too_many = false;
        'join: for a in &starts {
            for b in &matches {
                if a.iter().zip(b.iter()).any(|(x, y)| matches!((x, y), (Some(x), Some(y)) if x != y)) {
                    continue;
                }
                next.insert(a.iter().zip(b.iter()).map(|(x, y)| x.clone().or_else(|| y.clone())).collect());
                if next.len() > 100_000 {
                    too_many = true;
                    break 'join;
                }
            }
        }
        if !too_many {
            starts = next;
            used = true;
        }
    }
    if !used {
        return None;
    }
    // every start binds the same fields: each condition used does
    let fields: Vec<bool> = match starts.iter().next() {
        Some(st) => st.iter().map(|b| b.is_some()).collect(),
        None => vec![true; xs.len()],
    };
    let rest = xs.iter().zip(fields.iter()).filter(|(_, b)| !**b).map(|(x, _)| x.clone()).collect();
    let starts =
        starts.into_iter().map(|st| xs.iter().cloned().zip(st).filter_map(|(x, b)| b.map(|b| (x, b))).collect()).collect();
    Some((starts, rest))
}

/// The partial bindings one condition allows (see [`guided_bindings`]); all bind the same fields.
pub(super) fn guide(spec: &Spec, s: &State, xs: &[Var], valid: &[BTreeSet<Tagged>], c: &Term) -> Option<BTreeSet<Partial>> {
    pub(super) enum Pat {
        Field(usize),
        Any,
        Str(Id),
        Int(i64),
    }
    let field = |v: &Var, of: &Var| -> Option<usize> {
        let j = xs.iter().position(|x| x == v)?;
        (spec.type_of(v) == spec.type_of(of)).then_some(j)
    };
    let collect = |fdom: &Id, pats: &[Pat]| -> Option<BTreeSet<Partial>> {
        if !pats.iter().any(|p| matches!(p, Pat::Field(_))) {
            return None;
        }
        let mut out = BTreeSet::new();
        'cand: for te in s.instances_of(fdom).filter(|te| s.holds(te)) {
            let Elem::Prod(args) = &te.elem else { continue };
            if args.len() != pats.len() {
                continue;
            }
            let mut bound: Partial = vec![None; xs.len()];
            for (a, p) in args.iter().zip(pats.iter()) {
                match p {
                    Pat::Any => {}
                    Pat::Str(l) => {
                        if a.elem != Elem::Str(l.clone()) {
                            continue 'cand;
                        }
                    }
                    Pat::Int(i) => {
                        if a.elem != Elem::Int(*i) {
                            continue 'cand;
                        }
                    }
                    Pat::Field(j) => {
                        let v = Tagged { elem: a.elem.clone(), dom: spec.type_of(&xs[*j]) };
                        if !valid[*j].contains(&v) || bound[*j].as_ref().map_or(false, |b| *b != v) {
                            continue 'cand;
                        }
                        bound[*j] = Some(v);
                    }
                }
            }
            out.insert(bound);
        }
        Some(out)
    };
    let closed_product = |d: &Id| -> Option<(Id, Vec<Var>)> {
        let t = spec.find(d)?;
        match &t.domain {
            Domain::Products(params) if t.is_closed() && !spec.enumerable(&t.domain) => Some((spec.chase(d), params.clone())),
            _ => None,
        }
    };
    match c {
        Term::Bin(BinOp::Or, l, r) => {
            let (mut a, b) = (guide(spec, s, xs, valid, l)?, guide(spec, s, xs, valid, r)?);
            let shape = |m: &BTreeSet<Partial>| m.iter().next().map(|p| p.iter().map(|b| b.is_some()).collect::<Vec<_>>());
            match (shape(&a), shape(&b)) {
                (Some(x), Some(y)) if x != y => None,
                _ => {
                    a.extend(b);
                    Some(a)
                }
            }
        }
        // (Exists f : f.p == x && ...), f a closed fact type
        Term::Q(Quant::Exists, qs, body) if qs.len() == 1 && closed_product(&spec.type_of(&qs[0])).is_some() => {
            let f = &qs[0];
            let (fdom, params) = closed_product(&spec.type_of(f))?;
            let mut pats: Vec<Pat> = params.iter().map(|_| Pat::Any).collect();
            let mut cs = Vec::new();
            conjuncts(body, &mut cs);
            for c in cs {
                let Term::Bin(BinOp::Eq, l, r) = c else { continue };
                for (a, b) in [(&**l, &**r), (&**r, &**l)] {
                    if let (Term::Project(of, p), Term::Ref(x)) = (a, b) {
                        if **of == Term::Ref(f.clone()) {
                            if let (Some(i), Some(j)) = (params.iter().position(|q| q == p), field(x, p)) {
                                if matches!(pats[i], Pat::Any) {
                                    pats[i] = Pat::Field(j);
                                }
                            }
                        }
                    }
                }
            }
            collect(&fdom, &pats)
        }
        _ => {
            // [Exists qs :] Present(F(param := term, ..))
            let (quantified, inner): (&[Var], &Term) = match c {
                Term::Q(Quant::Exists, qs, inner) => (qs, inner),
                c => (&[], c),
            };
            let Term::Un(UnOp::Present, app) = inner else { return None };
            let Term::App(f, args) = &**app else { return None };
            let (fdom, params) = closed_product(f)?;
            let ts: Vec<&Term> = match args {
                Args::Pos(ts) if ts.len() == params.len() => ts.iter().collect(),
                Args::Mods(ms) if ms.len() == params.len() && ms.iter().zip(params.iter()).all(|((v, _), p)| v == p) => {
                    ms.iter().map(|(_, t)| t).collect()
                }
                _ => return None,
            };
            let mut pats = Vec::new();
            for (t, p) in ts.iter().zip(params.iter()) {
                let t = match t {
                    Term::Tag(inner, _) if matches!(**inner, Term::StringLit(_) | Term::IntLit(_)) => &**inner,
                    t => *t,
                };
                pats.push(match t {
                    Term::Ref(v) if quantified.contains(v) => Pat::Any,
                    Term::Ref(v) => Pat::Field(field(v, p)?),
                    Term::StringLit(l) => Pat::Str(l.clone()),
                    Term::IntLit(i) => Pat::Int(*i),
                    _ => return None,
                });
            }
            collect(&fdom, &pats)
        }
    }
}
