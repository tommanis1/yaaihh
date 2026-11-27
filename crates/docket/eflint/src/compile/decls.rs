//! The static check of clauses, declarations and phrases, and declarations' changes to the specification.

use super::*;

impl<'a> Static<'a> {
    pub(super) fn effect(&self, bound: &[Var], e: &Effect) -> S<Effect> {
        let (t2, _) = self.term(&e.term)?;
        let fs = self.free_vars(&t2)?;
        let mut vars = e.vars.clone();
        let mut all_bound = bound.to_vec();
        all_bound.extend(e.vars.iter().cloned());
        vars.extend(without(fs, &all_bound));
        Ok(Effect { kind: e.kind, vars, term: t2 })
    }

    pub(super) fn sync(&self, bound: &[Var], s: &Sync) -> S<Sync> {
        let (t2, ty) = self.term(&s.term)?;
        match ty {
            Ty::Tagged(_) => {
                let fs = self.free_vars(&t2)?;
                let mut all_bound = s.vars.clone();
                all_bound.extend(bound.iter().cloned());
                let mut vars = s.vars.clone();
                vars.extend(without(fs, &all_bound));
                Ok(Sync { vars, term: t2 })
            }
            _ => err("sync clause is not an instance expression".into()),
        }
    }

    /// A condition: boolean, with unbound variables existentially quantified.
    pub(super) fn condition(&self, bound: &[Var], t: &Term) -> S<Term> {
        let t2 = self.convert(t, &Ty::Bool)?;
        let fs = self.free_vars(&t2)?;
        Ok(exists(without(fs, bound), t2))
    }

    pub(super) fn derivation(&self, bound: &[Var], d: &Id, dv: &Derivation) -> S<Derivation> {
        match dv {
            Derivation::HoldsWhen(t) => Ok(Derivation::HoldsWhen(self.condition(bound, t)?)),
            Derivation::Dv(xs, t) => {
                let t2 = self.convert(t, &Ty::Tagged(d.clone()))?;
                let fs = self.free_vars(&t2)?;
                let mut vars = xs.clone();
                vars.extend(without(fs, xs));
                Ok(Derivation::Dv(vars, t2))
            }
        }
    }

    /// Compile the clauses of an extension into single-clause extensions, possibly of other types
    /// (a duty's `Terminated by` and `Created by` extend the acts they name).
    pub(super) fn extension(&self, ty: &Id, clauses: &[Clause]) -> S<Vec<(Id, Clause)>> {
        let Some(tspec) = self.spec.decls.get(ty).cloned() else {
            return err(format!("cannot find the type {} to extend or {} of the wrong kind", ty, ty));
        };
        let xs = Spec::params(ty, &tspec);
        let is_fact = matches!(tspec.kind, Kind::Fact { .. });
        let general = || {
            err(format!(
                "cannot extend type {}; only primitive types of kind Fact can have their domain extended (Identified by)",
                ty
            ))
        };
        let mut out: Vec<(Id, Clause)> = Vec::new();
        for clause in clauses {
            match clause {
                Clause::At(file, line, c) => {
                    for (target, done) in self.extension(ty, std::slice::from_ref(&**c))? {
                        out.push((target, Clause::At(file.clone(), *line, Box::new(done))));
                    }
                }
                Clause::ConditionedBy(conds) => out.push((
                    ty.clone(),
                    Clause::ConditionedBy(conds.iter().map(|c| self.condition(&xs, c)).collect::<S<_>>()?),
                )),
                Clause::Derivation(dvs) => out.push((
                    ty.clone(),
                    Clause::Derivation(dvs.iter().map(|d| self.derivation(&xs, ty, d)).collect::<S<_>>()?),
                )),
                Clause::Effects(effs) => out.push((
                    ty.clone(),
                    Clause::Effects(effs.iter().map(|e| self.effect(&xs, e)).collect::<S<_>>()?),
                )),
                Clause::Sync(ss) => {
                    out.push((ty.clone(), Clause::Sync(ss.iter().map(|s| self.sync(&xs, s)).collect::<S<_>>()?)))
                }
                Clause::Violation(vs) => out.push((
                    ty.clone(),
                    Clause::Violation(vs.iter().map(|v| self.violation_condition(ty, v)).collect::<S<_>>()?),
                )),
                Clause::Enforcing(acts) => {
                    // a duty is violated when an act that enforces it is enabled
                    let mut viols = Vec::new();
                    for a in acts {
                        let cond = Term::Un(UnOp::Enabled, bx(Term::App(a.clone(), Args::Mods(Vec::new()))));
                        viols.push((ty.clone(), Clause::Violation(vec![self.term(&cond)?.0])));
                    }
                    out.push((ty.clone(), Clause::Enforcing(acts.clone())));
                    out.extend(viols);
                }
                Clause::TerminatedBy(acts) => {
                    // the act terminates the duty, and holds when the duty holds
                    let duty = Term::App(ty.clone(), Args::Mods(Vec::new()));
                    let mut on_acts = Vec::new();
                    let mut holds = Vec::new();
                    for a in acts {
                        let e = Effect { kind: EffKind::Terminate, vars: Vec::new(), term: duty.clone() };
                        on_acts.push((a.clone(), Clause::Effects(vec![self.effect(&xs, &e)?])));
                        let h = Derivation::HoldsWhen(Term::Un(UnOp::Present, bx(duty.clone())));
                        holds.push((a.clone(), Clause::Derivation(vec![self.derivation(&xs, a, &h)?])));
                    }
                    out.push((ty.clone(), Clause::TerminatedBy(acts.clone())));
                    out.extend(on_acts);
                    out.extend(holds);
                }
                Clause::CreatedBy(acts) => {
                    let duty = Term::App(ty.clone(), Args::Mods(Vec::new()));
                    let mut on_acts = Vec::new();
                    for a in acts {
                        let e = Effect { kind: EffKind::Create, vars: Vec::new(), term: duty.clone() };
                        on_acts.push((a.clone(), Clause::Effects(vec![self.effect(&xs, &e)?])));
                    }
                    out.push((ty.clone(), Clause::CreatedBy(acts.clone())));
                    out.extend(on_acts);
                }
                Clause::Strings(_) => {
                    if !is_fact {
                        return general();
                    }
                    match tspec.domain {
                        Domain::AnyString | Domain::Strings(_) => out.push((ty.clone(), clause.clone())),
                        _ => {
                            return err(format!(
                                "cannot extend type {}; only string domains can be extended by string literals (Identified by)",
                                ty
                            ))
                        }
                    }
                }
                Clause::Ints(_) => {
                    if !is_fact {
                        return general();
                    }
                    match tspec.domain {
                        Domain::AnyInt | Domain::Ints(_) => out.push((ty.clone(), clause.clone())),
                        _ => {
                            return err(format!(
                                "cannot extend type {}; only integer domains can be extended by integer literals (Identified by)",
                                ty
                            ))
                        }
                    }
                }
                Clause::Close | Clause::Open => out.push((ty.clone(), clause.clone())),
            }
        }
        Ok(out)
    }

    /// Keep where the terms of compiled clause C of type TY are written.
    fn place(&mut self, ty: &Id, file: Rc<str>, line: usize, c: &Clause) {
        let file = if file.is_empty() { self.spec.file.clone() } else { file };
        let terms: Vec<Term> = match c {
            Clause::Derivation(ds) => ds.iter().map(|d| match d { Derivation::HoldsWhen(x) | Derivation::Dv(_, x) => x.clone() }).collect(),
            Clause::ConditionedBy(ts) | Clause::Violation(ts) => ts.clone(),
            _ => Vec::new(),
        };
        let lines = Rc::make_mut(&mut self.spec.lines);
        lines.extend(terms.into_iter().map(|term| Line { ty: ty.clone(), term, file: file.clone(), line }));
    }

    pub(super) fn violation_condition(&self, duty: &Id, t: &Term) -> S<Term> {
        let xs = match self.dom(duty)? {
            Domain::Products(xs) => xs,
            _ => return err(format!("the duty {} is not a composite type", duty)),
        };
        self.condition(&xs, t)
    }

    /// Add one compiled clause to a type; later clauses go in front of earlier ones.
    pub(super) fn apply_clause(&mut self, ty: &Id, clause: Clause) {
        if let Clause::At(file, line, c) = clause {
            self.place(ty, file, line, &c);
            return self.apply_clause(ty, *c);
        }
        let Some(rc) = self.spec.decls.get_mut(ty) else { return };
        let t = Rc::make_mut(rc);
        fn prepend<T>(new: Vec<T>, old: &mut Vec<T>) {
            let mut v = new;
            v.append(old);
            *old = v;
        }
        match clause {
            Clause::At(..) => {}
            Clause::Close => {
                if t.closed.is_some() {
                    t.closed = Some(true)
                }
            }
            Clause::Open => {
                if t.closed.is_none() {
                    t.closed = Some(false)
                }
            }
            Clause::ConditionedBy(c) => prepend(c, &mut t.conditions),
            Clause::Derivation(d) => prepend(d, &mut t.derivation),
            Clause::Effects(e) => match &mut t.kind {
                Kind::Act { effects, .. } | Kind::Event { effects, .. } => prepend(e, effects),
                _ => {}
            },
            Clause::Sync(s) => match &mut t.kind {
                Kind::Act { syncs, .. } | Kind::Event { syncs, .. } => prepend(s, syncs),
                _ => {}
            },
            Clause::Violation(v) => {
                if let Kind::Duty { violated_when, .. } = &mut t.kind {
                    prepend(v, violated_when)
                }
            }
            Clause::Enforcing(a) => {
                if let Kind::Duty { enforcing, .. } = &mut t.kind {
                    prepend(a, enforcing)
                }
            }
            Clause::TerminatedBy(a) => {
                if let Kind::Duty { terminating, .. } = &mut t.kind {
                    prepend(a, terminating)
                }
            }
            Clause::CreatedBy(a) => {
                if let Kind::Duty { creating, .. } = &mut t.kind {
                    prepend(a, creating)
                }
            }
            Clause::Strings(ss) => match &mut t.domain {
                Domain::AnyString => t.domain = Domain::Strings(ss),
                Domain::Strings(old) => old.extend(ss),
                _ => {}
            },
            Clause::Ints(is) => match &mut t.domain {
                Domain::AnyInt => t.domain = Domain::Ints(is),
                Domain::Ints(old) => old.extend(is),
                _ => {}
            },
        }
    }

    pub(super) fn decl_block(&mut self, ds: &[Decl]) -> S<()> {
        // new types and placeholders first, so that declarations in one block can refer to each other
        for d in ds {
            match d {
                Decl::Type(name, tspec) => {
                    self.spec.decls.insert(name.clone(), Rc::new(tspec.clone()));
                }
                Decl::Placeholder(from, to) => {
                    self.spec.aliases.insert(from.clone(), to.clone());
                }
                Decl::Ext(_, _) => {}
            }
        }
        for d in ds {
            if let Decl::Ext(ty, clauses) = d {
                for (target, clause) in self.extension(ty, clauses)? {
                    self.apply_clause(&target, clause);
                }
            }
        }
        Ok(())
    }

    pub(super) fn query(&self, t0: &Term) -> S<Term> {
        let unbound: Vec<Var> = self.free_vars(t0)?.into_iter().collect();
        let t1 = if unbound.is_empty() {
            t0.clone()
        } else if let Term::Un(UnOp::Not, inner) = t0 {
            Term::Un(UnOp::Not, bx(Term::Q(Quant::Exists, unbound, inner.clone())))
        } else {
            Term::Q(Quant::Exists, unbound, bx(t0.clone()))
        };
        self.convert(&t1, &Ty::Bool)
    }

    pub(super) fn quantified(&self, vs: &[Var], t: &Term) -> S<(Vec<Var>, Term)> {
        let fs = self.free_vars(t)?;
        let mut vars = vs.to_vec();
        vars.extend(without(fs, vs));
        Ok((vars, self.term(t)?.0))
    }

    pub fn phrase(&mut self, p: &Phrase) -> S<CPhrase> {
        match p {
            Phrase::Trigger(vs, t) => self.quantified(vs, t).map(|(v, t)| CPhrase::Trigger(v, t)),
            Phrase::Create(vs, t) => self.quantified(vs, t).map(|(v, t)| CPhrase::Create(v, t)),
            Phrase::Terminate(vs, t) => self.quantified(vs, t).map(|(v, t)| CPhrase::Terminate(v, t)),
            Phrase::Obfuscate(vs, t) => self.quantified(vs, t).map(|(v, t)| CPhrase::Obfuscate(v, t)),
            Phrase::Query(t) => self.query(t).map(CPhrase::Query),
            Phrase::InstQuery(b, vs, t) => self.quantified(vs, t).map(|(v, t)| CPhrase::InstQuery(*b, v, t)),
            Phrase::Decls(ds) => {
                self.decl_block(ds)?;
                Ok(CPhrase::OnlyDecls)
            }
            Phrase::Par(ps) => {
                // declarations of a block take effect before its other phrases, which run side by side
                let mut decls = Vec::new();
                let mut rest = Vec::new();
                for p in ps {
                    match p {
                        Phrase::Decls(ds) => decls.extend(ds.iter().cloned()),
                        other => rest.push(other),
                    }
                }
                self.decl_block(&decls)?;
                let compiled: Vec<CPhrase> = rest.into_iter().map(|p| self.phrase(p)).collect::<S<_>>()?;
                Ok(compiled
                    .into_iter()
                    .rev()
                    .fold(CPhrase::OnlyDecls, |acc, p| CPhrase::Par(Box::new(p), Box::new(acc))))
            }
        }
    }

    /// The static phase of the four-part file format: every declared type as a whole.
    pub fn old_file(&mut self, f: &OldFile) -> S<(Vec<Effect>, Vec<CPhrase>)> {
        for d in f.decls.iter().rev() {
            match d {
                Decl::Type(name, tspec) => {
                    self.spec.decls.insert(name.clone(), Rc::new(tspec.clone()));
                }
                Decl::Placeholder(from, to) => {
                    self.spec.aliases.insert(from.clone(), to.clone());
                }
                Decl::Ext(_, _) => {}
            }
        }
        let names: Vec<Id> = self.spec.decls.keys().cloned().collect();
        for name in names {
            let tspec = self.spec.decls[&name].clone();
            let xs = Spec::params(&name, &tspec);
            if let Domain::Products(vs) = &tspec.domain {
                self.check_known(vs)?;
            }
            let constraint = self.condition(&xs, &tspec.domain_constraint)?;
            let rc = self.spec.decls.get_mut(&name).unwrap();
            Rc::make_mut(rc).domain_constraint = constraint;
        }
        let initial = f.initial.iter().map(|e| self.effect(&[], e)).collect::<S<Vec<_>>>()?;
        let mut scenario = Vec::new();
        for p in &f.scenario {
            let mut copy = self.spec.clone();
            scenario.push(Static { spec: &mut copy }.phrase(p)?);
        }
        Ok((initial, scenario))
    }
}
