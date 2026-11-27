//! States, the evaluation's types and its core: bindings and how results are sequenced.
//!
//! A term evaluates to a list of results under an environment of variable bindings: zero results
//! when a filter fails, several when a quantified variable ranges over instances. The order of
//! the results is part of the behaviour (it is the order in which instances are reported and
//! transitions are performed), so sequencing goes through [`bind`], which fixes that order.

use crate::ast::*;
use crate::compile::Spec;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

mod delta;
mod graph;
mod view;
mod guided;
mod holds;
mod reads;
mod saturate;
mod subs;
mod tally;
mod term;
#[cfg(test)]
mod tests;
mod values;

pub use self::delta::{Delta, Plan};
pub use self::graph::*;
pub use self::view::*;
use self::guided::*;
pub use self::reads::*;
pub use self::saturate::*;

#[derive(Clone, PartialEq, Debug)]
pub struct Info {
    pub value: bool,
    /// Whether the fact was derived from others, not stored: it is dropped and worked out again when they change.
    pub derived: bool,
}

#[derive(Clone, PartialEq, Default)]
pub struct State {
    /// Every assigned instance with its truth. Written only through `insert`, `remove` and the
    /// `retain_underived` functions, which keep `by_type` and `by_field` with it.
    contents: BTreeMap<Tagged, Info>,
    /// The instances of `contents` by type, in `contents`' order (by value): what `assigned` and a
    /// `Var` or `Function` assignment read, so neither scans every instance of every type.
    by_type: BTreeMap<Id, BTreeSet<Elem>>,
    /// The instances of composite types, stored or derived, by type, field and the field's value: what a quantifier or
    /// an ending effect started from the facts it names reads ([`Ev::held_bindings`]), and a rule kept up to date.
    by_field: BTreeMap<(Id, usize, Elem), BTreeSet<Elem>>,
    /// The values the fields of instances that hold take, stored or derived, by the field's type, each with how many
    /// such fields take it: what a variable ranges over when ranging (`Reading::ranging`), beside its type's instances that hold.
    values: BTreeMap<(Id, Elem), usize>,
    /// Derived instances that neither hold nor fail: what the well-founded model leaves open where rules read each
    /// other through `Not`, `Forall`, a count, `Violated` or `Enabled` (`saturate::alternate`). Not in `contents`: a
    /// reading that decides ([`Ev::new`]) takes them to hold or fail whichever is safer; a query says they are unknown.
    unknown: BTreeSet<Tagged>,
    pub time: i64,
}

impl State {
    pub fn holding(&self) -> impl Iterator<Item = &Tagged> {
        self.contents.iter().filter(|(_, i)| i.value).map(|(t, _)| t)
    }
    pub fn not_holding(&self) -> impl Iterator<Item = &Tagged> {
        self.contents.iter().filter(|(_, i)| !i.value).map(|(t, _)| t)
    }
    pub fn holds(&self, te: &Tagged) -> bool {
        self.contents.get(te).map_or(false, |i| i.value)
    }
    pub fn contents(&self) -> &BTreeMap<Tagged, Info> {
        &self.contents
    }
    pub fn unknown(&self) -> &BTreeSet<Tagged> {
        &self.unknown
    }
    pub(super) fn leave_unknown(&mut self, te: Tagged) {
        self.unknown.insert(te);
    }
    /// TE no longer in `contents`, unknown if it was.
    pub(super) fn take_out(&mut self, te: &Tagged) {
        let unknown = self.unknown.contains(te);
        self.remove(te);
        if unknown {
            self.unknown.insert(te.clone());
        }
    }
    pub fn insert(&mut self, te: Tagged, i: Info) {
        if !i.derived {
            self.unknown.remove(&te);
        }
        self.by_type.entry(te.dom.clone()).or_default().insert(te.elem.clone());
        let holding = i.value;
        let old = self.contents.insert(te.clone(), i);
        if old.is_none() {
            self.index_fields(&te, true);
        }
        if holding != old.is_some_and(|o| o.value) {
            self.index_values(&te, holding);
        }
    }
    pub fn remove(&mut self, te: &Tagged) {
        self.unknown.remove(te);
        if let Some(i) = self.contents.remove(te) {
            if let Some(es) = self.by_type.get_mut(&te.dom) {
                es.remove(&te.elem);
                if es.is_empty() {
                    self.by_type.remove(&te.dom);
                }
            }
            self.index_fields(te, false);
            if i.value {
                self.index_values(te, false);
            }
        }
    }
    /// Add TE to `by_field`, or take it out.
    fn index_fields(&mut self, te: &Tagged, add: bool) {
        let Elem::Prod(args) = &te.elem else { return };
        for (n, a) in args.iter().enumerate() {
            let key = (te.dom.clone(), n, a.elem.clone());
            if add {
                self.by_field.entry(key).or_default().insert(te.elem.clone());
            } else if let Some(es) = self.by_field.get_mut(&key) {
                es.remove(&te.elem);
                if es.is_empty() {
                    self.by_field.remove(&key);
                }
            }
        }
    }
    /// The instances of type D whose field N is V, held or not, stored or derived, in `contents`' order.
    fn with_field<'a>(&'a self, d: &'a Id, n: usize, v: &Elem) -> impl Iterator<Item = Tagged> + 'a {
        self.by_field.get(&(d.clone(), n, v.clone())).into_iter().flatten().map(move |e| Tagged { elem: e.clone(), dom: d.clone() })
    }
    /// Drop every derived instance.
    pub fn retain_underived(&mut self) {
        let by_type = &mut self.by_type;
        let mut gone = Vec::new();
        self.contents.retain(|te, i| {
            if i.derived {
                if let Some(es) = by_type.get_mut(&te.dom) {
                    es.remove(&te.elem);
                }
                gone.push((te.clone(), i.value));
            }
            !i.derived
        });
        by_type.retain(|_, es| !es.is_empty());
        self.unknown.clear();
        for (te, held) in gone {
            self.index_fields(&te, false);
            if held {
                self.index_values(&te, false);
            }
        }
    }
    /// Drop the derived instances of the types of TYPES, and nothing else.
    pub fn retain_underived_of(&mut self, types: &BTreeSet<Id>) {
        self.unknown.retain(|te| !types.contains(&te.dom));
        let (by_type, contents) = (&mut self.by_type, &mut self.contents);
        let mut gone = Vec::new();
        for d in types {
            let Some(es) = by_type.get_mut(d) else { continue };
            es.retain(|e| {
                let te = Tagged { elem: e.clone(), dom: d.clone() };
                let derived = contents.get(&te).is_some_and(|i| i.derived);
                if derived {
                    let held = contents.remove(&te).is_some_and(|i| i.value);
                    gone.push((te, held));
                }
                !derived
            });
            if es.is_empty() {
                by_type.remove(d);
            }
        }
        for (te, held) in gone {
            self.index_fields(&te, false);
            if held {
                self.index_values(&te, false);
            }
        }
    }
    /// The instances of type D, held or not, in `contents`' order.
    pub fn instances_of<'a>(&'a self, d: &'a Id) -> impl Iterator<Item = Tagged> + 'a {
        self.by_type.get(d).into_iter().flatten().map(move |e| Tagged { elem: e.clone(), dom: d.clone() })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Assign {
    True,
    False,
    Unknown,
}

/// The assignments a phrase or transition makes. Where two assignments meet, creating wins over
/// terminating, which wins over obfuscating.
pub type Store = BTreeMap<Tagged, Assign>;

pub fn store_union(mut a: Store, b: Store) -> Store {
    for (k, v) in b {
        let e = a.entry(k).or_insert(v);
        if v < *e {
            *e = v;
        }
    }
    a
}

#[derive(Clone, PartialEq, Debug)]
pub enum RtErr {
    /// An open type without any assignment is enumerated.
    MissingInstances(Id),
    /// The truth of an instance of an open type is not known.
    MissingAssignments(Vec<(Tagged, Assign)>),
    Internal(String),
    /// A question tried more bindings than its cap ([`Ev::within`]).
    TooMany(u64),
}

impl RtErr {
    pub fn is_missing_input(&self) -> bool {
        !matches!(self, RtErr::Internal(_) | RtErr::TooMany(_))
    }
    pub fn message(&self) -> String {
        match self {
            RtErr::MissingInstances(d) => format!("missing input assignments for type: {}", d),
            RtErr::MissingAssignments(tes) => format!(
                "missing input assignments for: [{}]",
                tes.iter().map(|(t, _)| show_string(&t.to_string())).collect::<Vec<_>>().join(",")
            ),
            RtErr::Internal(s) => format!("INTERNAL ERROR {}", s),
            RtErr::TooMany(n) => format!("the question tries more than {n} bindings, its cap; ask a narrower one"),
        }
    }
}

pub type R<T> = Result<T, RtErr>;

#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    Bool(bool),
    Str(Id),
    Int(i64),
    Tagged(Tagged),
}

/// A performed act or event with the transitions it synchronises with.
#[derive(Clone, PartialEq, Debug)]
pub struct TransInfo {
    pub tagged: Tagged,
    /// Its effects, including those of synchronised transitions.
    pub assignments: Store,
    /// Whether performing it is a violation (a disabled act, or one synchronising with one).
    pub forced: bool,
    /// Whether it, or a transition it synchronises with, is not enabled.
    pub disabled: bool,
    pub actor: Option<Tagged>,
    pub syncs: Vec<TransInfo>,
}

impl TransInfo {
    pub fn is_action(&self) -> bool {
        self.actor.is_some()
    }
}

/// Sequencing: apply `f` to each result; the results of later elements come first.
pub fn bind<A, B>(xs: Vec<A>, mut f: impl FnMut(A) -> R<Vec<B>>) -> R<Vec<B>> {
    let mut chunks = Vec::with_capacity(xs.len());
    for x in xs {
        chunks.push(f(x)?);
    }
    if chunks.len() == 1 {
        return Ok(chunks.pop().unwrap());
    }
    let mut out = Vec::with_capacity(chunks.iter().map(Vec::len).sum());
    for c in chunks.into_iter().rev() {
        out.extend(c);
    }
    Ok(out)
}

/// Mapping a pure function over results is a `bind`, so it reverses them.
fn fmap<A, B>(xs: Vec<A>, f: impl FnMut(A) -> B) -> Vec<B> {
    xs.into_iter().rev().map(f).collect()
}

fn bools(vs: Vec<Value>) -> Vec<bool> {
    // a `bind` over the results that keeps the booleans
    vs.into_iter().rev().filter_map(|v| if let Value::Bool(b) = v { Some(b) } else { None }).collect()
}

fn ints(vs: Vec<Value>) -> Vec<i64> {
    vs.into_iter().rev().filter_map(|v| if let Value::Int(i) = v { Some(i) } else { None }).collect()
}

fn taggeds(vs: Vec<Value>) -> Vec<Tagged> {
    vs.into_iter().rev().filter_map(|v| if let Value::Tagged(t) = v { Some(t) } else { None }).collect()
}

/// The instances among results, in order (for use under [`bind`]).
fn only_taggeds(vs: Vec<Value>) -> Vec<Tagged> {
    vs.into_iter().filter_map(|v| if let Value::Tagged(t) = v { Some(t) } else { None }).collect()
}

fn only_ints(vs: Vec<Value>) -> Vec<i64> {
    vs.into_iter().filter_map(|v| if let Value::Int(i) = v { Some(i) } else { None }).collect()
}

fn only_bools(vs: Vec<Value>) -> Vec<bool> {
    vs.into_iter().filter_map(|v| if let Value::Bool(b) = v { Some(b) } else { None }).collect()
}

fn prod_args(te: &Tagged) -> Option<&[Tagged]> {
    match &te.elem {
        Elem::Prod(args) => Some(args),
        _ => None,
    }
}

pub struct Ev<'a> {
    pub spec: &'a Spec,
    pub state: &'a State,
    /// What this reading takes otherwise than STATE, where anything.
    over: Option<Over<'a>>,
    /// The other bound of the truth, where there are two: what a read that can turn false as more holds reads.
    other: Option<(&'a State, Option<Over<'a>>)>,
    /// Whether this side is the upper bound: what a comparison an unknown instance could turn answers on it.
    upper: bool,
    /// The types whose instances are not settled while a knot is worked out, and the types of their fields.
    open: Option<&'a BTreeSet<Id>>,
    /// Whether what is being compared read an unknown or unsettled instance.
    touched: bool,
    /// Whether a value being worked out (a count, sum, max or min) read one: no instance made of it is sure.
    valued: bool,
    env: Vec<(Var, Tagged)>,
    /// The bindings it may try ([`Ev::within`]), and how many more.
    cap: u64,
    left: u64,
}

impl<'a> Ev<'a> {
    /// An evaluation that decides only what is sure: where STATE leaves instances unknown, a plain read takes them as
    /// failing and a read through `Not`, `Forall`, a count, `Violated` or `Enabled` as holding, so an act, a violation
    /// or a condition that an unknown instance could turn is not taken to hold.
    pub fn new(spec: &'a Spec, state: &'a State) -> Ev<'a> {
        let other = (!state.unknown.is_empty()).then(|| (state, Some(Over::plus(&state.unknown))));
        Ev { spec, state, over: None, other, upper: false, open: None, touched: false, valued: false, env: Vec::new(), cap: u64::MAX, left: u64::MAX }
    }

    /// An evaluation of two values, unknown instances failing: how rules are worked out, knot by knot.
    pub fn plain(spec: &'a Spec, state: &'a State) -> Ev<'a> {
        Ev { spec, state, over: None, other: None, upper: false, open: None, touched: false, valued: false, env: Vec::new(), cap: u64::MAX, left: u64::MAX }
    }

    /// The reading of what may hold: [`Ev::new`]'s two sides the other way round, unknown instances holding for a
    /// plain read and failing for one through `Not`.
    pub fn upper(spec: &'a Spec, state: &'a State) -> Ev<'a> {
        let over = (!state.unknown.is_empty()).then(|| Over::plus(&state.unknown));
        Ev { spec, state, over, other: over.map(|_| (state, None)), upper: true, open: None, touched: false, valued: false, env: Vec::new(), cap: u64::MAX, left: u64::MAX }
    }

    /// One side of the alternating fixpoint: plain reads read STATE (the upper bound where UPPER), reads through `Not`
    /// or `Forall` read it as OTHER takes it, and so on alternately inside them; the instances of OPEN are unsettled.
    pub(super) fn sides(spec: &'a Spec, state: &'a State, other: Over<'a>, upper: bool, open: &'a BTreeSet<Id>) -> Ev<'a> {
        Ev { spec, state, over: None, other: Some((state, Some(other))), upper, open: Some(open), touched: false, valued: false, env: Vec::new(), cap: u64::MAX, left: u64::MAX }
    }

    /// Whether any read can meet an instance that is not settled.
    fn tracking(&self) -> bool {
        !self.state.unknown.is_empty() || self.open.is_some()
    }

    /// TE read: what is being compared is touched where TE is unknown or of a knot being worked out.
    pub(super) fn seen(&mut self, te: &Tagged) {
        if self.tracking() && (self.state.unknown.contains(te) || self.open.is_some_and(|o| o.contains(&te.dom))) {
            self.touched = true;
        }
    }

    /// The instances of D, or the values its fields take, enumerated: touched where one unsettled could be among them.
    pub(super) fn seen_type(&mut self, d: &Id) {
        if !self.tracking() {
            return;
        }
        let of = |te: &Tagged| te.dom == *d || matches!(&te.elem, Elem::Prod(a) if a.iter().any(|x| x.dom == *d));
        if self.open.is_some_and(|o| o.contains(d)) || self.state.unknown.iter().any(of) {
            self.touched = true;
        }
    }

    /// T, compared: on either side an answer an unsettled instance could turn is the side's own bound.
    fn compared(&mut self, t: &Term) -> R<Vec<Value>> {
        let (touched, valued) = (std::mem::take(&mut self.touched), std::mem::take(&mut self.valued));
        let got = self.eval_here(t);
        let open = self.touched || self.valued;
        (self.touched, self.valued) = (touched || self.touched, valued);
        let upper = self.upper;
        Ok(got?.into_iter().map(|v| if open && matches!(v, Value::Bool(_)) { Value::Bool(upper) } else { v }).collect())
    }

    /// An aggregate's value: marked where it read an unsettled instance.
    fn aggregated(&mut self, m: impl FnOnce(&mut Self) -> R<Vec<Value>>) -> R<Vec<Value>> {
        let touched = std::mem::take(&mut self.touched);
        let got = m(self);
        self.valued |= self.touched;
        self.touched |= touched;
        got
    }

    /// This evaluator, refusing once it has tried more than STEPS bindings: a question that joins too much ends.
    pub fn within(mut self, steps: u64) -> Self {
        (self.cap, self.left) = (steps, steps);
        self
    }

    /// One binding more tried; refused past the cap.
    fn tick(&mut self) -> R<()> {
        let Some(n) = self.left.checked_sub(1) else { return Err(RtErr::TooMany(self.cap)) };
        self.left = n;
        Ok(())
    }

    /// The state as this side of the reading takes it.
    fn view(&self) -> View<'a> {
        View { base: self.state, over: self.over }
    }

    /// M with the two bounds swapped, where there are two.
    fn flipped<T>(&mut self, m: impl FnOnce(&mut Self) -> T) -> T {
        let Some((s, o)) = self.other else { return m(self) };
        let back = (std::mem::replace(&mut self.state, s), std::mem::replace(&mut self.over, o));
        self.other = Some(back);
        self.upper = !self.upper;
        let r = m(self);
        self.upper = !self.upper;
        let (s, o) = self.other.expect("swapped back by its own flip");
        self.other = Some((std::mem::replace(&mut self.state, s), std::mem::replace(&mut self.over, o)));
        r
    }

    fn tspec(&self, d: &Id) -> R<Rc<TypeSpec>> {
        self.spec.find(d).cloned().ok_or_else(|| RtErr::Internal(format!("undeclared type: {}", d)))
    }

    /// Run `m` with extra bindings; later bindings shadow earlier ones.
    fn with<T>(&mut self, bindings: Vec<(Var, Tagged)>, m: impl FnOnce(&mut Self) -> T) -> T {
        let mark = self.env.len();
        self.env.extend(bindings);
        let r = m(self);
        self.env.truncate(mark);
        r
    }

    fn lookup(&self, x: &Var) -> R<Tagged> {
        match self.env.iter().rev().find(|(v, _)| v == x) {
            Some((_, te)) => Ok(Tagged { elem: te.elem.clone(), dom: self.spec.type_of(x) }),
            None => Err(RtErr::Internal(format!("variable {}{} not bound", x.dom, x.dec))),
        }
    }

    /// The bindings an instance gives to the parameters of its type.
    fn bindings(&self, t: &TypeSpec, te: &Tagged) -> Vec<(Var, Tagged)> {
        match (&t.domain, &te.elem) {
            (Domain::Products(xs), Elem::Prod(args)) => xs.iter().cloned().zip(args.iter().cloned()).collect(),
            _ => vec![(Var::plain(&te.dom), te.clone())],
        }
    }

    fn product_bindings(&self, t: &TypeSpec, te: &Tagged) -> R<Vec<(Var, Tagged)>> {
        match (&t.domain, &te.elem) {
            (Domain::Products(xs), Elem::Prod(args)) => Ok(xs.iter().cloned().zip(args.iter().cloned()).collect()),
            _ => Err(RtErr::Internal(format!("{} is not an instance of a composite type", te))),
        }
    }

    /// The results of several independent computations combined into lists, one per choice.
    fn sequence<T: Clone>(
        &mut self,
        n: usize,
        get: &mut dyn FnMut(&mut Self, usize) -> R<Vec<T>>,
    ) -> R<Vec<Vec<T>>> {
        fn go<'a, T: Clone>(
            ev: &mut Ev<'a>,
            i: usize,
            n: usize,
            get: &mut dyn FnMut(&mut Ev<'a>, usize) -> R<Vec<T>>,
        ) -> R<Vec<Vec<T>>> {
            if i == n {
                return Ok(vec![Vec::new()]);
            }
            let here = get(ev, i)?;
            if here.is_empty() {
                return Ok(Vec::new());
            }
            let rest = go(ev, i + 1, n, get)?;
            let mut out = Vec::with_capacity(here.len() * rest.len());
            for a in &here {
                for r in rest.iter().rev() {
                    let mut v = Vec::with_capacity(r.len() + 1);
                    v.push(a.clone());
                    v.extend(r.iter().cloned());
                    out.push(v);
                }
            }
            Ok(out)
        }
        go(self, 0, n, get)
    }}
