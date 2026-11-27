//! Whether an instance holds, is violated or is enabled, the transition an act or event makes, and its effects.

use super::*;

impl<'a> Ev<'a> {
    pub fn is_holds(&mut self, te: &Tagged) -> R<Vec<bool>> {
        let valid = self.is_valid_instance(te)?;
        bind(valid, |v| {
            if !v {
                return Ok(vec![false]);
            }
            // ranging: an act's or event's instance is there for any valid fields; its conditions say if it is enabled
            if self.spec.reading.ranging && self.spec.triggerable(&te.dom) {
                return Ok(vec![true]);
            }
            if self.spec.tests.contains(&te.dom) {
                return self.test_holds(te);
            }
            self.seen(te);
            match self.view().value(te) {
                Some(v) => Ok(vec![v]),
                None => {
                    let t = self.tspec(&te.dom)?;
                    if t.is_closed() {
                        Ok(vec![false])
                    } else {
                        let (trues, falses) = self.view().assigned(&te.dom);
                        let mut all: Vec<(Tagged, Assign)> = Vec::new();
                        all.extend(trues.into_iter().map(|t| (t, Assign::True)));
                        all.extend(falses.into_iter().map(|t| (t, Assign::False)));
                        all.push((te.clone(), Assign::Unknown));
                        all.sort();
                        Err(RtErr::MissingAssignments(all))
                    }
                }
            }
        })
    }

    /// Whether a rule of test TE's type holds of TE's fields, or derives TE, its conditions met.
    fn test_holds(&mut self, te: &Tagged) -> R<Vec<bool>> {
        let t = self.tspec(&te.dom)?;
        let b = self.bindings(&t, te);
        let mut any = false;
        for d in &t.derivation {
            any |= match d {
                Derivation::HoldsWhen(c) => bools(self.with(b.clone(), |ev| ev.eval(c))?).contains(&true),
                Derivation::Dv(xs, term) => self.foreach(xs, &mut |ev| Ok(taggeds(ev.eval(term)?)))?.contains(te),
            };
        }
        if !any {
            return Ok(vec![false]);
        }
        Ok(vec![self.sat_conditions(te)?.into_iter().any(|v| v)])
    }

    pub fn sat_conditions(&mut self, te: &Tagged) -> R<Vec<bool>> {
        let valid = self.is_valid_instance(te)?;
        bind(valid, |v| {
            if !v {
                return Ok(vec![false]);
            }
            let t = self.tspec(&te.dom)?;
            let b = self.bindings(&t, te);
            self.with(b, |ev| {
                let lists =
                    ev.sequence(t.conditions.len(), &mut |ev, i| Ok(bools(ev.eval(&t.conditions[i])?)))?;
                Ok(fmap(lists, |bs| bs.into_iter().all(|b| b)))
            })
        })
    }

    pub fn violation_condition(&mut self, te: &Tagged) -> R<Vec<bool>> {
        let t = self.tspec(&te.dom)?;
        let conds: &[Term] = match &t.kind {
            Kind::Duty { violated_when, .. } => violated_when,
            _ => &[],
        };
        let term = conds
            .iter()
            .rev()
            .fold(Term::BoolLit(false), |acc, c| Term::Bin(BinOp::Or, Box::new(c.clone()), Box::new(acc)));
        let b = self.product_bindings(&t, te)?;
        Ok(bools(self.with(b, |ev| ev.eval(&term))?))
    }

    pub fn is_violated(&mut self, te: &Tagged) -> R<Vec<bool>> {
        let holds = fmap(self.is_holds(te)?, |h| h);
        if holds.is_empty() {
            return Ok(Vec::new());
        }
        let viol = self.violation_condition(te)?;
        bind(holds, |h| bind(viol.clone(), |v| Ok(vec![h && v])))
    }

    pub fn is_enabled(&mut self, te: &Tagged) -> R<Vec<bool>> {
        let holds = self.is_holds(te)?;
        bind(holds, |h| {
            if !h {
                return Ok(vec![false]);
            }
            let sat = self.sat_conditions(te)?;
            bind(sat, |ok| {
                if !ok {
                    return Ok(vec![false]);
                }
                let t = self.tspec(&te.dom)?;
                match &t.kind {
                    Kind::Act { syncs, .. } => {
                        let b = self.product_bindings(&t, te)?;
                        self.with(b, |ev| {
                            let mut none_forced = true;
                            for s in syncs {
                                for info in ev.eval_sync(&BTreeSet::new(), s)? {
                                    none_forced &= !info.forced;
                                }
                            }
                            Ok(vec![none_forced])
                        })
                    }
                    _ => Ok(vec![true]),
                }
            })
        })
    }

    /// Perform an act or event: its effects, and the transitions it synchronises with.
    pub fn instantiate_trans(&mut self, done: &BTreeSet<Tagged>, te: &Tagged) -> R<Vec<TransInfo>> {
        if done.contains(te) {
            return Ok(Vec::new());
        }
        let t = self.tspec(&te.dom)?;
        let enabled = self.is_enabled(te)?;
        let mut done2 = done.clone();
        done2.insert(te.clone());
        bind(enabled, |is_enabled| {
            let (effects, syncs, physical, actor) = match &t.kind {
                Kind::Fact { .. } | Kind::Duty { .. } => return Ok(Vec::new()),
                Kind::Act { effects, syncs, physical } => {
                    (effects, syncs, *physical, prod_args(te).and_then(|a| a.first().cloned()))
                }
                Kind::Event { effects, syncs } => (effects, syncs, true, None),
            };
            let b = self.product_bindings(&t, te)?;
            self.with(b, |ev| {
                let mut sync_infos = Vec::new();
                for s in syncs {
                    sync_infos.extend(ev.eval_sync(&done2, s)?);
                }
                let mut any_forced = false;
                let mut sync_store = Store::new();
                for info in sync_infos.iter().rev() {
                    any_forced |= info.forced;
                    sync_store = store_union(info.assignments.clone(), sync_store);
                }
                let mut own = Store::new();
                for e in effects.iter().rev() {
                    own = store_union(ev.eval_effect(e)?, own);
                }
                Ok(vec![TransInfo {
                    tagged: te.clone(),
                    assignments: store_union(own, sync_store),
                    forced: any_forced || (!is_enabled && !physical),
                    disabled: any_forced || !is_enabled,
                    actor: actor.clone(),
                    syncs: sync_infos,
                }])
            })
        })
    }

    pub(super) fn eval_sync(&mut self, done: &BTreeSet<Tagged>, s: &Sync) -> R<Vec<TransInfo>> {
        self.foreach(&s.vars, &mut |ev| {
            let tes = only_taggeds(ev.eval(&s.term)?);
            bind(tes, |te| ev.instantiate_trans(done, &te))
        })
    }

    pub fn eval_effect(&mut self, e: &Effect) -> R<Store> {
        let guided = if self.spec.reading.guided { self.effect_from_facts(e)? } else { None };
        let tes = match guided {
            Some(tes) => tes,
            None => self.foreach(&e.vars, &mut |ev| Ok(taggeds(ev.eval(&e.term)?)))?,
        };
        let a = match e.kind {
            EffKind::Create => Assign::True,
            EffKind::Terminate => Assign::False,
            EffKind::Obfuscate => Assign::Unknown,
        };
        Ok(tes.into_iter().map(|te| (te, a)).collect())
    }

    /// The results of `m` for every combination of instances of the variables.
    pub fn foreach<T>(&mut self, xs: &[Var], m: &mut dyn FnMut(&mut Self) -> R<Vec<T>>) -> R<Vec<T>> {
        match xs.split_first() {
            None => m(self),
            Some((x, rest)) => {
                let tes = self.every_valid_subs(x)?;
                bind(tes, |te| {
                    self.tick()?;
                    self.with(vec![(x.clone(), te)], |ev| ev.foreach(rest, m))
                })
            }
        }
    }
}
