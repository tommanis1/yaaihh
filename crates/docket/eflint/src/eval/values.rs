//! Ranging: the values fields take in instances that hold, kept as the state changes, and what a field ranges over.

use super::*;

impl State {
    /// Count the values TE's fields take in `values`, or take them out.
    pub(super) fn index_values(&mut self, te: &Tagged, add: bool) {
        let Elem::Prod(args) = &te.elem else { return };
        for a in args.iter() {
            let key = (a.dom.clone(), a.elem.clone());
            if add {
                *self.values.entry(key).or_default() += 1;
            } else if let Some(n) = self.values.get_mut(&key) {
                *n -= 1;
                if *n == 0 {
                    self.values.remove(&key);
                }
            }
        }
    }
    /// Whether some field of an instance that holds takes TE.
    pub fn occurs(&self, te: &Tagged) -> bool {
        self.values.contains_key(&(te.dom.clone(), te.elem.clone()))
    }
    /// The values fields of type D take in instances that hold, in the order of values.
    pub(super) fn values_of<'a>(&'a self, d: &'a Id) -> impl Iterator<Item = Elem> + 'a {
        self.values.range((d.clone(), Elem::Str(id("")))..).take_while(move |((k, _), _)| k == d).map(|((_, e), _)| e.clone())
    }
}

impl<'a> Ev<'a> {
    /// Whether ARG is one of the instances field X ranges over when a rule's fields are enumerated (`foreach`).
    pub(super) fn ranges_over(&mut self, x: &Var, arg: &Tagged) -> R<bool> {
        let d = self.spec.type_of(x);
        let t = self.tspec(&d)?;
        if self.spec.enumerable(&t.domain) {
            return Ok(self.every_valid_subs(x)?.contains(arg));
        }
        self.seen_type(&d);
        if self.spec.reading.ranging {
            return Ok(self.view().holds(arg) || self.view().occurs(arg));
        }
        if !t.is_closed() && self.view().instances_of(&d).is_empty() {
            return Err(RtErr::MissingInstances(d));
        }
        Ok(self.view().holds(arg))
    }
}
