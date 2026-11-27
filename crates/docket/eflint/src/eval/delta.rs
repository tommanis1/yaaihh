//! Derived types kept up to date a step at a time: after stored facts change, a type whose rules reach each instance
//! through its fields is worked out again for the instances the changes reach, a count or sum of one fact by the
//! parts of the instances that changed; any other type that reads a change goes stale and is worked out whole when read.

use super::subs::literal;
use super::*;

/// What a step changed: each instance whose truth it may have changed, with whether it held before.
pub type Delta = BTreeMap<Tagged, bool>;

/// A fact a rule reads, and for each of the fact's fields the field of the rule's head it is given, if any.
type Read = (Id, Vec<Option<usize>>);

/// How a step keeps a derived type up to date without working it out whole.
enum Upkeep {
    Keyed(Keyed),
    Tally(super::tally::Tally),
}

/// A derived type whose instances a change reaches through its fields.
struct Keyed {
    heads: Vec<Var>,
    /// Every fact its rules read, at each place they read it.
    reads: Vec<Read>,
    /// Each `Holds when`'s variables its opening facts bind (the head's and those of the `Exists` around it), and
    /// those facts.
    clauses: Vec<(Vec<Var>, Vec<Term>)>,
    /// The instances its `+` lines give.
    stated: Vec<Term>,
}

/// How a change to the stored facts reaches each derived type and each duty's violation, worked out once for a
/// specification read guided and ranging.
pub struct Plan {
    /// The specification it was worked out for.
    of: Spec,
    /// The derived types in the order they are worked out in, each with the types its rules name and, where a step
    /// keeps it up to date (through its fields, or as a count or sum), how.
    order: Vec<(Id, BTreeSet<Id>, Option<Upkeep>)>,
    /// The duty types whose violation a change reaches through their fields, with the facts it reads.
    violations: BTreeMap<Id, Vec<Read>>,
    /// Each type with the types whose rules or violations read it, directly or through derived types, itself among them.
    readers: BTreeMap<Id, BTreeSet<Id>>,
}

impl Plan {
    pub fn of(spec: &Spec) -> Plan {
        let types: Vec<(Id, Rc<TypeSpec>)> = spec
            .decls
            .iter()
            .filter(|(d, t)| !t.derivation.is_empty() && !spec.tests.contains(*d))
            .map(|(d, t)| (d.clone(), t.clone()))
            .collect();
        let empty = State::default();
        let ev = Ev::new(spec, &empty);
        let mut order = Vec::new();
        for Knot { members: knot, falling, .. } in graph(spec, &types).knots {
            // the types of a knot read each other, so each reads what all of them read and goes stale with them
            let mut reads: BTreeSet<Id> = knot.iter().flat_map(|&i| derivation_reads(spec, &types[i].0, &types[i].1)).map(|x| spec.chase(&x)).collect();
            // a type that reads itself through `Not` is worked out whole, by the alternating fixpoint
            if let ([i], true) = (&knot[..], falling.is_empty()) {
                let (d, t) = &types[*i];
                let upkeep = keyed(&ev, d, t).map(Upkeep::Keyed).or_else(|| super::tally::Tally::of(&ev, d, t).map(Upkeep::Tally));
                order.push((d.clone(), reads, upkeep));
                continue;
            }
            reads.extend(knot.iter().map(|&i| types[i].0.clone()));
            order.extend(knot.iter().map(|&i| (types[i].0.clone(), reads.clone(), None)));
        }
        let mut violations = BTreeMap::new();
        for (d, t) in &spec.decls {
            let Kind::Duty { violated_when, .. } = &t.kind else { continue };
            let (heads, mut reads) = (Spec::params(d, t), Vec::new());
            if violated_when.iter().all(|v| keyed_term(&ev, &heads, v, &mut reads)) {
                violations.insert(d.clone(), reads);
            }
        }
        let mut readers: BTreeMap<Id, BTreeSet<Id>> = BTreeMap::new();
        for d in spec.decls.keys().filter(|d| !spec.tests.contains(*d)) {
            for r in super::reads(spec, BTreeSet::from([d.clone()])) {
                readers.entry(r).or_default().insert(d.clone());
            }
        }
        Plan { of: spec.clone(), order, violations, readers }
    }

    /// The types a change to the types CHANGED may reach: those, and every type that reads one.
    pub fn reached(&self, changed: &BTreeSet<Id>) -> BTreeSet<Id> {
        changed.iter().flat_map(|c| self.readers.get(c).into_iter().flatten()).chain(changed).cloned().collect()
    }

    /// Whether SPEC is the one it was worked out for: the same declarations, each by pointer, and the same aliases,
    /// reading, host and tests.
    pub fn fits(&self, spec: &Spec) -> bool {
        let s = &self.of;
        s.decls.len() == spec.decls.len()
            && s.decls.iter().zip(&spec.decls).all(|((d, t), (e, u))| d == e && Rc::ptr_eq(t, u))
            && (&s.aliases, s.reading, &s.host, &s.tests) == (&spec.aliases, spec.reading, &spec.host, &spec.tests)
    }

    /// The derived types brought up to date after the stored facts in DELTA changed: a keyed type for the instances the
    /// changes reach, a count or sum by their parts; any other that reads a changed or stale type made stale, and one
    /// whose working out fails. DELTA gains every derived instance whose truth changed.
    pub fn propagate(&self, spec: &Spec, state: &mut State, stale: &mut BTreeSet<Id>, delta: &mut Delta) {
        let mut changed: BTreeSet<Id> = delta.keys().map(|te| spec.chase(&te.dom)).collect();
        for (d, reads, upkeep) in &self.order {
            if stale.contains(d) {
                continue;
            }
            if reads.iter().any(|r| stale.contains(r)) {
                stale.insert(d.clone());
                continue;
            }
            if !changed.contains(d) && reads.is_disjoint(&changed) {
                continue;
            }
            // an instance worked out one at a time reads two values, so one an unknown instance reaches goes whole
            if upkeep.is_some() && state.unknown().iter().any(|te| te.dom == *d || reads.contains(&te.dom)) {
                stale.insert(d.clone());
                continue;
            }
            let kept = match upkeep {
                Some(Upkeep::Keyed(k)) => Some(k.update(spec, state, d, delta)),
                Some(Upkeep::Tally(t)) => Some(t.update(spec, state, d, delta)),
                None => None,
            };
            match kept {
                Some(Ok(false)) => {}
                Some(Ok(true)) => {
                    changed.insert(d.clone());
                }
                Some(Err(_)) | None => {
                    stale.insert(d.clone());
                }
            }
        }
    }

    /// The duties of type D a change in DELTA may have ended or whose violation it may have changed: those in DELTA and
    /// those a change reaches through the violation's facts. `None` where the violation is not reached through D's
    /// fields, or D or a type the violation reads is stale.
    pub fn duties_reached(&self, spec: &Spec, state: &State, stale: &BTreeSet<Id>, d: &Id, delta: &Delta) -> Option<Vec<Tagged>> {
        let reads = self.violations.get(d)?;
        if stale.contains(d) || reads.iter().any(|(f, _)| stale.contains(f)) {
            return None;
        }
        let mut out = Vec::new();
        for te in delta.keys() {
            let ty = spec.chase(&te.dom);
            if ty == *d {
                out.push(te.clone());
            }
            for (_, key) in reads.iter().filter(|(f, _)| *f == ty) {
                if let Some(given) = partial(te, key) {
                    out.extend(matching(state, d, &given));
                }
            }
        }
        Some(out)
    }
}

impl Keyed {
    /// The instances of D the changes in DELTA reach, each worked out again on the state as it stands; DELTA gains those
    /// whose truth changed. Whether any did.
    fn update(&self, spec: &Spec, state: &mut State, d: &Id, delta: &mut Delta) -> R<bool> {
        let mut reached = BTreeSet::new();
        for te in delta.keys() {
            let ty = spec.chase(&te.dom);
            if ty == *d {
                reached.insert(te.clone());
            }
            for (_, key) in self.reads.iter().filter(|(f, _)| *f == ty) {
                let Some(given) = partial(te, key) else { continue };
                reached.extend(matching(state, d, &given));
                reached.extend(self.found(spec, state, d, &given)?);
            }
        }
        let mut any = false;
        for te in reached {
            let was = match state.contents().get(&te) {
                Some(i) if !i.derived => continue,
                i => i.is_some_and(|i| i.value),
            };
            let Some(now) = Ev::plain(spec, state).derives(&te)? else { return Err(RtErr::Internal(format!("{te} is not worked out alone"))) };
            if now != was {
                match now {
                    true => state.insert(te.clone(), Info { value: true, derived: true }),
                    false => state.remove(&te),
                }
                delta.entry(te).or_insert(was);
                any = true;
            }
        }
        Ok(any)
    }

    /// The instances of D with the fields GIVEN that each `Holds when` may give, from its opening facts' held instances,
    /// and those its `+` lines give.
    fn found(&self, spec: &Spec, state: &State, d: &Id, given: &[(usize, Elem)]) -> R<Vec<Tagged>> {
        let fixed: Vec<(Var, Tagged)> = given
            .iter()
            .map(|(n, e)| (self.heads[*n].clone(), Tagged { elem: e.clone(), dom: spec.type_of(&self.heads[*n]) }))
            .collect();
        let head = Term::App(d.clone(), Args::Mods(Vec::new()));
        let mut ev = Ev::plain(spec, state);
        let mut out = Vec::new();
        for (xs, apps) in &self.clauses {
            let open: Vec<Var> = xs.iter().filter(|x| !fixed.iter().any(|(v, _)| v == *x)).cloned().collect();
            let apps: Vec<&Term> = apps.iter().collect();
            ev.with(fixed.clone(), |ev| -> R<()> {
                let Some(found) = ev.held_bindings(&open, &apps)? else { return Err(RtErr::Internal(format!("{d} is not reached through its facts"))) };
                for b in found {
                    let heads = b.into_iter().filter(|(x, _)| self.heads.contains(x)).collect();
                    out.extend(taggeds(ev.with(heads, |ev| ev.eval(&head))?));
                }
                Ok(())
            })?;
        }
        for s in &self.stated {
            out.extend(taggeds(ev.eval(s)?).into_iter().filter(|te| fits(te, given)));
        }
        Ok(out)
    }
}

/// D's rules as a change reaches them, where it reaches each instance through its fields: a closed composite type read
/// guided and ranging, with no `Where`, no condition and no rule that can fail; each rule a `+` line, or a `Holds when`
/// whose opening facts bind the head's fields and those of the `Exists` around it ([`binds`]) and whose every fact is
/// given one of the head's fields ([`keyed_term`]).
fn keyed(ev: &Ev, d: &Id, t: &TypeSpec) -> Option<Keyed> {
    let spec = ev.spec;
    let Domain::Products(heads) = &t.domain else { return None };
    if !(spec.reading.guided && spec.reading.ranging) || !t.is_closed() || !t.conditions.is_empty() || !ev.unconstrained(d, 0) || ev.may_fail(d, t) {
        return None;
    }
    let mut k = Keyed { heads: heads.clone(), reads: Vec::new(), clauses: Vec::new(), stated: Vec::new() };
    for clause in &t.derivation {
        match clause {
            Derivation::Dv(xs, s) if xs.is_empty() && literal(s) => k.stated.push(s.clone()),
            Derivation::Dv(..) => return None,
            Derivation::HoldsWhen(c) => {
                let (outer, body) = match c {
                    Term::Q(Quant::Exists, vs, b) => (vs.clone(), &**b),
                    c => (Vec::new(), c),
                };
                let apps = opening(body);
                let xs: Vec<Var> = heads.iter().chain(&outer).cloned().collect();
                if !xs.iter().all(|x| apps.iter().any(|a| binds(ev, a, x))) || !keyed_term(ev, heads, c, &mut k.reads) {
                    return None;
                }
                k.clauses.push((xs, apps.into_iter().cloned().collect()));
            }
        }
    }
    (!k.reads.iter().any(|(f, _)| f == d)).then_some(k)
}

/// Whether every fact T reads (`Present`) is given one of HEADS as one of its fields of its type, and every variable a
/// quantifier in T binds ranges over a list its type gives or is a field of a fact the quantified condition opens with
/// ([`binds`]): then a change to an instance reaches only the heads it gives. READS gains each fact read, at each place.
fn keyed_term(ev: &Ev, heads: &[Var], t: &Term, reads: &mut Vec<Read>) -> bool {
    let spec = ev.spec;
    match t {
        Term::BoolLit(_) | Term::IntLit(_) | Term::StringLit(_) | Term::Ref(_) => true,
        Term::CurrentTime | Term::Un(UnOp::Violated | UnOp::Enabled, _) => false,
        Term::Un(UnOp::Present, a) => {
            let Term::App(f, args) = &**a else { return false };
            let Some(ft) = spec.find(f) else { return false };
            if spec.tests.contains(f) || !keyed_term(ev, heads, a, reads) {
                return false;
            }
            let params = Spec::params(f, ft);
            let key: Vec<Option<usize>> = placed(&params, args)
                .iter()
                .zip(&params)
                .map(|(t, p)| match t {
                    Term::Ref(v) if spec.type_of(v) == spec.type_of(p) => heads.iter().position(|h| h == v),
                    _ => None,
                })
                .collect();
            reads.push((spec.chase(f), key));
            reads.last().is_some_and(|(_, key)| key.iter().any(Option::is_some))
        }
        Term::Un(_, x) | Term::Project(x, _) | Term::Tag(x, _) => keyed_term(ev, heads, x, reads),
        Term::Bin(_, l, r) => keyed_term(ev, heads, l, reads) && keyed_term(ev, heads, r, reads),
        Term::Q(q, vs, body) => {
            let cond = match &**body {
                Term::Bin(BinOp::When, _, c) if *q != Quant::Exists => &**c,
                b => b,
            };
            let apps = opening(cond);
            let listed = |v: &Var| spec.find(&spec.type_of(v)).is_some_and(|t| spec.enumerable(&t.domain) && t.domain_constraint == Term::BoolLit(true));
            *q != Quant::Forall
                && vs.iter().all(|v| !heads.contains(v) && (listed(v) || apps.iter().any(|a| binds(ev, a, v))))
                && keyed_term(ev, heads, body, reads)
        }
        // an instance built, or a host function called: nothing read
        Term::App(f, args) => {
            let parts = match args {
                Args::Pos(ts) => ts.iter().all(|a| keyed_term(ev, heads, a, reads)),
                Args::Mods(ms) => ms.iter().all(|(_, a)| keyed_term(ev, heads, a, reads)),
            };
            parts && (spec.find(f).is_some() || spec.host.sig(f).is_some())
        }
    }
}

/// Whether APP, a fact of a closed composite type with no `Where`, gives X as one of its fields of X's type: under
/// ranging, X then ranges over a value that occurs wherever APP holds.
fn binds(ev: &Ev, app: &Term, x: &Var) -> bool {
    let spec = ev.spec;
    let Term::App(f, args) = app else { return false };
    let Some(ft) = spec.find(f) else { return false };
    let Domain::Products(params) = &ft.domain else { return false };
    if !ft.is_closed() || spec.enumerable(&ft.domain) || spec.tests.contains(f) || !ev.unconstrained(f, 0) {
        return false;
    }
    placed(params, args).iter().zip(params).any(|(t, p)| *t == Term::Ref(x.clone()) && spec.type_of(p) == spec.type_of(x))
}

/// The fields of the head an instance TE of a fact read with KEY gives; `None` where it gives one field two values.
fn partial(te: &Tagged, key: &[Option<usize>]) -> Option<Vec<(usize, Elem)>> {
    let fields: Vec<&Elem> = match &te.elem {
        Elem::Prod(args) => args.iter().map(|a| &a.elem).collect(),
        e => vec![e],
    };
    if fields.len() != key.len() {
        return None;
    }
    let mut given: Vec<(usize, Elem)> = Vec::new();
    for (e, k) in fields.into_iter().zip(key) {
        let Some(h) = k else { continue };
        match given.iter().find(|(g, _)| g == h) {
            Some((_, v)) if v != e => return None,
            Some(_) => {}
            None => given.push((*h, e.clone())),
        }
    }
    Some(given)
}

/// The instances of D, held or not, with the fields GIVEN.
fn matching(state: &State, d: &Id, given: &[(usize, Elem)]) -> Vec<Tagged> {
    let narrowest = given.iter().min_by_key(|(n, e)| state.by_field.get(&(d.clone(), *n, e.clone())).map_or(0, |es| es.len()));
    let Some((n, e)) = narrowest else { return Vec::new() };
    state.with_field(d, *n, e).filter(|te| fits(te, given)).collect()
}

/// Whether TE has the fields GIVEN.
fn fits(te: &Tagged, given: &[(usize, Elem)]) -> bool {
    let Elem::Prod(args) = &te.elem else { return false };
    given.iter().all(|(n, e)| args.get(*n).is_some_and(|a| a.elem == *e))
}
