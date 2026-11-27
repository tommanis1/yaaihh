//! Assignments made to a state, and the derived types worked out after them, knot by knot.

use super::*;

/// Apply assignments to a state. Creating an instance of a `Var` type ends the other instances
/// of the type; creating one of a `Function` type ends the others with the same first field.
pub fn make_assignments(spec: &Spec, store: &Store, state: &State) -> State {
    let mut s = state.clone();
    assign(spec, store, &mut s);
    s
}

/// `make_assignments` on the state itself.
pub fn assign(spec: &Spec, store: &Store, s: &mut State) {
    for (te, a) in store.iter().rev() {
        match a {
            Assign::True => {
                match spec.decls.get(&te.dom).and_then(|t| t.restriction) {
                    Some(Restriction::Var) => {
                        for k in s.instances_of(&te.dom).collect::<Vec<_>>() {
                            s.insert(k, Info { value: false, derived: false });
                        }
                    }
                    Some(Restriction::Function) => {
                        if let Elem::Prod(args) = &te.elem {
                            if args.len() == 2 {
                                for k in s.instances_of(&te.dom).collect::<Vec<_>>() {
                                    if let Elem::Prod(other) = &k.elem {
                                        if other.len() == 2 && other[0] == args[0] {
                                            s.insert(k, Info { value: false, derived: false });
                                        }
                                    }
                                }
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }
                    None => {}
                }
                s.insert(te.clone(), Info { value: true, derived: false });
            }
            Assign::False => {
                s.insert(te.clone(), Info { value: false, derived: false });
            }
            Assign::Unknown => {
                s.remove(te);
            }
        }
    }
}

/// Drop every derived fact, then apply the derivation rules until nothing is added.
pub fn rebase_and_saturate(spec: &Spec, state: State) -> R<State> {
    rebase_and_saturate_but(spec, state, &BTreeSet::new())
}

/// `rebase_and_saturate`, leaving the types of LAZY underived (`lazy_types`: no other type's rule reads them).
pub fn rebase_and_saturate_but(spec: &Spec, state: State, lazy: &BTreeSet<Id>) -> R<State> {
    let mut s = state;
    s.retain_underived();
    saturate(spec, s, |d| !lazy.contains(d))
}

/// Derive the types of ONLY on top of STATE, whose other derived facts stand: for an act or event of `lazy_types`
/// about to be triggered, with the lazy types its own rules read (`needed_lazy`).
pub fn saturate_only(spec: &Spec, state: State, only: &BTreeSet<Id>) -> R<State> {
    saturate(spec, state, |d| only.contains(d))
}

/// `saturate_only` on the state itself. On an error the state holds part of ONLY's instances.
pub fn saturate_only_in(spec: &Spec, state: &mut State, only: &BTreeSet<Id>) -> R<()> {
    saturate_in(spec, state, |d| only.contains(d))
}

pub(super) fn saturate(spec: &Spec, state: State, keep: impl Fn(&Id) -> bool) -> R<State> {
    let mut s = state;
    saturate_in(spec, &mut s, keep)?;
    Ok(s)
}

/// Work the derived types KEEP accepts out on S, knot by knot in the order of [`graph`], so that a read through
/// `Not`, `Forall` or a count of another knot's type sees it complete, whatever the types are called. A knot that
/// reads itself so, or reads an unknown instance, is worked out by [`alternate`].
/// The graph is the text's, made once ([`known`]); of a knot only the types KEEP accepts are worked out, the others
/// standing as they are.
pub(super) fn saturate_in(spec: &Spec, s: &mut State, keep: impl Fn(&Id) -> bool) -> R<()> {
    let (derived_types, g) = known(spec);
    for knot in &g.knots {
        let members: Vec<usize> = knot.members.iter().copied().filter(|&i| keep(&derived_types[i].0)).collect();
        if members.is_empty() {
            continue;
        }
        let open: BTreeSet<&Id> = s.unknown().iter().map(|te| &te.dom).collect();
        let reads_open = !open.is_empty()
            && members.iter().any(|&i| derivation_reads(spec, &derived_types[i].0, &derived_types[i].1).iter().any(|r| open.contains(r)));
        if knot.falling.is_empty() && !reads_open {
            // a type alone in its knot whose rules do not read it is whole after one pass
            let once = match members[..] {
                [i] => !derivation_reads(spec, &derived_types[i].0, &derived_types[i].1).contains(&derived_types[i].0),
                _ => false,
            };
            round(spec, s, None, &derived_types, &members, once)?;
        } else {
            alternate(spec, s, &derived_types, &members)?;
        }
    }
    Ok(())
}

/// The alternating fixpoint of Van Gelder, Ross and Schlipf for one knot: from nothing sure, what may hold given what
/// is sure, then what is sure given what may, until what is sure stops growing. What is sure holds, what may and is
/// not sure is unknown, the rest fails: the well-founded model. A knot whose instances do not read themselves through
/// `Not` (a strict `<` down a chain) leaves none unknown.
fn alternate(spec: &Spec, s: &mut State, types: &[(Id, Rc<TypeSpec>)], knot: &[usize]) -> R<()> {
    let own: BTreeSet<Id> = knot.iter().map(|&i| types[i].0.clone()).collect();
    // the types not settled while it is worked out: its own, and those of their fields, whose values they add to
    let mut open = own.clone();
    for &i in knot {
        open.extend(Spec::params(&types[i].0, &types[i].1).iter().map(|v| spec.chase(&v.dom)));
    }
    let none = BTreeSet::new();
    // both sides are S, nothing copied: the other knots' unknown instances are put in while what may hold is worked
    // out, and each side's read through `Not` takes the knot's own instances as the other side last found them
    let others: BTreeSet<Tagged> = s.unknown().iter().filter(|te| !own.contains(&te.dom)).cloned().collect();
    let mut sure: BTreeSet<Tagged> = BTreeSet::new();
    loop {
        put(s, &own, &none);
        others.iter().for_each(|te| s.insert(te.clone(), Info { value: true, derived: true }));
        let lower = Over { plus: None, minus: Some(&others), own: Some((&own, &sure)) };
        let may = round(spec, s, Some((lower, true, &open)), types, knot, false);
        // taken out again whether or not the round could be worked out
        others.iter().for_each(|te| s.take_out(te));
        let may: BTreeSet<Tagged> = may?.into_iter().collect();
        debug_assert!(sure.is_subset(&may), "what is sure of a knot is among what may hold of it");
        put(s, &own, &none);
        let upper = Over { plus: Some(&others), minus: None, own: Some((&own, &may)) };
        let next: BTreeSet<Tagged> = round(spec, s, Some((upper, false, &open)), types, knot, false)?.into_iter().collect();
        if next == sure {
            may.difference(&sure).for_each(|te| s.leave_unknown(te.clone()));
            return Ok(());
        }
        if next.is_superset(&sure) {
            sure = next;
            continue;
        }
        // a count or `Enabled` inside the knot can make what is sure shrink: then nothing of the knot is sure
        put(s, &own, &none);
        may.into_iter().chain(sure).chain(next).for_each(|te| s.leave_unknown(te));
        return Ok(());
    }
}

/// S with the derived instances of the types OWN replaced by TES, holding.
fn put(s: &mut State, own: &BTreeSet<Id>, tes: &BTreeSet<Tagged>) {
    s.retain_underived_of(own);
    for te in tes {
        s.insert(te.clone(), Info { value: true, derived: true });
    }
}

/// One side of the fixpoint, or the reading of two values.
type Side<'a> = Option<(Over<'a>, bool, &'a BTreeSet<Id>)>;

/// An evaluation on S, its reads through `Not` taking S as OTHER does where it is given, on the upper side where so.
fn ev_on<'a>(spec: &'a Spec, s: &'a State, side: Side<'a>) -> Ev<'a> {
    match side {
        Some((o, upper, open)) => Ev::sides(spec, s, o, upper, open),
        None => Ev::plain(spec, s),
    }
}

/// The rules of KNOT applied on S until nothing is added, or ONCE, reads that can turn false reading OTHER where it
/// is given; the instances added.
fn round(spec: &Spec, s: &mut State, other: Side<'_>, types: &[(Id, Rc<TypeSpec>)], knot: &[usize], once: bool) -> R<Vec<Tagged>> {
    let mut added = Vec::new();
    loop {
        let mut changed = false;
        for &i in knot {
            let (d, t) = &types[i];
            for clause in &t.derivation {
                let (xs, term) = match clause {
                    Derivation::HoldsWhen(c) => {
                        let (xs, instance) = match &t.domain {
                            Domain::Products(xs) => (xs.clone(), Term::App(d.clone(), Args::Mods(Vec::new()))),
                            _ => (vec![Var::plain(d)], Term::Ref(Var::plain(d))),
                        };
                        (xs, Term::Bin(BinOp::When, Box::new(instance), Box::new(c.clone())))
                    }
                    Derivation::Dv(xs, term) => (xs.clone(), term.clone()),
                };
                let mut body = |ev: &mut Ev| -> R<Vec<Tagged>> {
                    let valued = std::mem::take(&mut ev.valued);
                    let tes = taggeds(ev.eval(&term)?);
                    let found = bind(tes, |te| Ok(ev.sat_conditions(&te)?.into_iter().filter(|b| *b).map(|_| te.clone()).collect()))?;
                    // an instance made of a count an unsettled instance could change is never sure
                    let open = std::mem::replace(&mut ev.valued, valued);
                    Ok(if open && !ev.upper { Vec::new() } else { found })
                };
                // a rule that can fail on one combination and not on another is tried on every one, guided or
                // not, so it meets the errors the reference meets
                let guided = match clause {
                    Derivation::HoldsWhen(c) if spec.reading.guided && !Ev::plain(spec, s).may_fail(d, t) => {
                        guided_bindings(spec, s, &xs, c)
                    }
                    _ => None,
                };
                let found = match guided {
                    Some((starts, rest)) => {
                        let mut out = Vec::new();
                        for start in starts {
                            out.extend(ev_on(spec, s, other).with(start, |ev| ev.foreach(&rest, &mut body))?);
                        }
                        out
                    }
                    None => ev_on(spec, s, other).foreach(&xs, &mut body)?,
                };
                for te in found {
                    if !s.contents.contains_key(&te) {
                        s.insert(te.clone(), Info { value: true, derived: true });
                        added.push(te);
                        changed = true;
                    }
                }
            }
        }
        if once || !changed {
            return Ok(added);
        }
    }
}
