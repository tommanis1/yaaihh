//! A state read with a few instances taken to hold or to fail other than it has them: the unknown instances taken to
//! hold (the upper bound of the truth), or a knot's own types read as one side of the alternating fixpoint has them.
//! What is read costs what the state's own reads cost, plus the few instances changed; nothing is copied.

use super::*;

/// What a reading takes otherwise than the state: PLUS hold, MINUS fail, and the derived instances of the types OWN
/// names are those of its set, whatever the state holds of them.
#[derive(Clone, Copy)]
pub struct Over<'a> {
    pub plus: Option<&'a BTreeSet<Tagged>>,
    pub minus: Option<&'a BTreeSet<Tagged>>,
    pub own: Option<(&'a BTreeSet<Id>, &'a BTreeSet<Tagged>)>,
}

impl<'a> Over<'a> {
    /// The instances of PLUS taken to hold.
    pub fn plus(plus: &'a BTreeSet<Tagged>) -> Over<'a> {
        Over { plus: Some(plus), minus: None, own: None }
    }
}

/// A state, with what a reading takes otherwise where it is given.
#[derive(Clone, Copy)]
pub struct View<'a> {
    pub base: &'a State,
    pub over: Option<Over<'a>>,
}

impl<'a> View<'a> {
    /// What the reading takes of TE where it decides: a derived instance of a replaced type is in its set or not.
    fn forced(&self, te: &Tagged) -> Option<bool> {
        let o = self.over.as_ref()?;
        if let Some((own, set)) = o.own {
            if own.contains(&te.dom) && !self.base.contents.get(te).is_some_and(|i| !i.derived) {
                return Some(set.contains(te));
            }
        }
        if o.plus.is_some_and(|p| p.contains(te)) {
            return Some(true);
        }
        if o.minus.is_some_and(|m| m.contains(te)) {
            return Some(false);
        }
        None
    }

    /// TE's truth as an assignment: `None` where it is not assigned (a derived instance taken to fail is not).
    pub fn value(&self, te: &Tagged) -> Option<bool> {
        match self.forced(te) {
            Some(true) => Some(true),
            Some(false) => None,
            None => self.base.contents.get(te).map(|i| i.value),
        }
    }

    pub fn holds(&self, te: &Tagged) -> bool {
        self.value(te) == Some(true)
    }

    /// The instances the reading may take otherwise, of type D or of any: a knot's and the unknown ones, few.
    fn changed(&self, d: Option<&Id>) -> Vec<Tagged> {
        let Some(o) = &self.over else { return Vec::new() };
        let of = |te: &&Tagged| d.is_none_or(|d| te.dom == *d);
        let mut out: Vec<Tagged> = Vec::new();
        if let Some((own, set)) = o.own {
            out.extend(set.iter().filter(of).cloned());
            for t in own.iter().filter(|t| d.is_none_or(|d| d == *t)) {
                out.extend(self.base.instances_of(t));
            }
        }
        for s in [o.plus, o.minus].into_iter().flatten() {
            out.extend(s.iter().filter(of).cloned());
        }
        out.sort();
        out.dedup();
        out
    }

    /// The instances of type D, held or not, in the order of values.
    pub fn instances_of(&self, d: &Id) -> Vec<Tagged> {
        let mut all: Vec<Tagged> = self.base.instances_of(d).collect();
        let changed = self.changed(Some(d));
        if changed.is_empty() {
            return all;
        }
        all.extend(changed);
        all.sort();
        all.dedup();
        all.retain(|te| self.value(te).is_some());
        all
    }

    /// The instances of type D that hold and those assigned to fail.
    pub fn assigned(&self, d: &Id) -> (Vec<Tagged>, Vec<Tagged>) {
        self.instances_of(d).into_iter().partition(|te| self.holds(te))
    }

    /// The instances of type D that hold.
    pub fn holding_of(&self, d: &Id) -> Vec<Tagged> {
        self.instances_of(d).into_iter().filter(|te| self.holds(te)).collect()
    }

    /// How many more (or fewer) fields of type D take each value than in the state: from the instances taken otherwise.
    fn value_change(&self, d: &Id) -> BTreeMap<Elem, isize> {
        let mut by = BTreeMap::new();
        for te in self.changed(None) {
            let (was, is) = (self.base.holds(&te), self.holds(&te));
            let (Elem::Prod(args), true) = (&te.elem, was != is) else { continue };
            for a in args.iter().filter(|a| a.dom == *d) {
                *by.entry(a.elem.clone()).or_default() += if is { 1 } else { -1 };
            }
        }
        by
    }

    /// The values fields of type D take in instances that hold, in the order of values.
    pub fn values_of(&self, d: &Id) -> Vec<Elem> {
        let change = self.value_change(d);
        if change.is_empty() {
            return self.base.values_of(d).collect();
        }
        let mut all: BTreeSet<Elem> = self.base.values_of(d).collect();
        all.extend(change.keys().cloned());
        let count = |e: &Elem| self.base.values.get(&(d.clone(), e.clone())).map_or(0, |n| *n as isize) + change.get(e).copied().unwrap_or(0);
        all.into_iter().filter(|e| count(e) > 0).collect()
    }

    /// Whether TE stands in a field of an instance that holds.
    pub fn occurs(&self, te: &Tagged) -> bool {
        let base = self.base.values.get(&(te.dom.clone(), te.elem.clone())).map_or(0, |n| *n as isize);
        if self.over.is_none() {
            return base > 0;
        }
        base + self.value_change(&te.dom).get(&te.elem).copied().unwrap_or(0) > 0
    }

    /// How many instances of type D have field N equal to V, about: what a guided reading starts from the least of.
    pub fn with_field_count(&self, d: &Id, n: usize, v: &Elem) -> usize {
        self.base.by_field.get(&(d.clone(), n, v.clone())).map_or(0, |es| es.len())
    }

    /// The instances of type D whose field N is V, held or not.
    pub fn with_field(&self, d: &Id, n: usize, v: &Elem) -> Vec<Tagged> {
        let mut all: Vec<Tagged> = self.base.with_field(d, n, v).collect();
        let field = |te: &Tagged| matches!(&te.elem, Elem::Prod(args) if args.get(n).is_some_and(|a| a.elem == *v));
        let changed: Vec<Tagged> = self.changed(Some(d)).into_iter().filter(field).collect();
        if changed.is_empty() {
            return all;
        }
        all.extend(changed);
        all.sort();
        all.dedup();
        all.retain(|te| self.value(te).is_some());
        all
    }
}
