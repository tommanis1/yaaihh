//! The static check of a term: names resolved and literals and instances converted to the type expected.

use super::*;

impl<'a> Static<'a> {
    pub(super) fn dom(&self, d: &Id) -> S<Domain> {
        match self.spec.find(d) {
            Some(t) => Ok(t.domain.clone()),
            None => err(format!("undeclared type: {}", d)),
        }
    }

    pub(super) fn check_known(&self, xs: &[Var]) -> S<()> {
        for x in xs {
            if self.spec.find(&self.spec.type_of(x)).is_none() {
                return err(format!("Undeclared type or placeholder `{}{}`", x.dom, x.dec));
            }
        }
        Ok(())
    }

    pub(super) fn free_vars(&self, t: &Term) -> S<BTreeSet<Var>> {
        let xs = free(self.spec, t);
        self.check_known(&xs.iter().cloned().collect::<Vec<_>>())?;
        Ok(xs)
    }

    pub(super) fn match_type(&self, from: &Ty, to: &Domain) -> S<bool> {
        Ok(match from {
            Ty::Tagged(d) => match_domain(&self.dom(d)?, to),
            Ty::Ints => matches!(to, Domain::AnyInt | Domain::Ints(_) | Domain::Time),
            Ty::Strings => matches!(to, Domain::AnyString | Domain::Strings(_)),
            Ty::Bool => false,
        })
    }

    /// Check a term against an expected type, inserting the conversion if there is one.
    pub fn convert(&self, t: &Term, want: &Ty) -> S<Term> {
        let (t2, got) = self.term(t)?;
        if got == *want {
            return Ok(t2);
        }
        let fail = || err(format!("cannot convert {} to {}", show_ty(&got), show_ty(want)));
        match want {
            Ty::Strings => match &got {
                Ty::Tagged(d) if match_domain(&self.dom(d)?, &Domain::AnyString) => Ok(Term::Un(UnOp::Untag, bx(t2))),
                _ => fail(),
            },
            Ty::Ints => match &got {
                Ty::Tagged(d) if match_domain(&self.dom(d)?, &Domain::AnyInt) => Ok(Term::Un(UnOp::Untag, bx(t2))),
                _ => fail(),
            },
            Ty::Bool => match &got {
                Ty::Ints => Ok(Term::Bin(BinOp::Geq, bx(t2), bx(Term::IntLit(1)))),
                Ty::Tagged(_) => Ok(Term::Un(UnOp::Present, bx(t2))),
                _ => fail(),
            },
            Ty::Tagged(d) => {
                if self.match_type(&got, &self.dom(d)?)? {
                    Ok(Term::Tag(bx(t2), d.clone()))
                } else {
                    fail()
                }
            }
        }
    }

    /// A function of the caller's table applied: each argument converted to the type the table gives.
    fn host_application(&self, d: &Id, args: &Args) -> S<(Term, Ty)> {
        let (Some((want, got)), Args::Pos(ts)) = (self.spec.host.sig(d), args) else {
            return err(format!("{d} takes its arguments in order"));
        };
        if ts.len() != want.len() {
            return err(format!("{d} takes {} arguments, {} given", want.len(), ts.len()));
        }
        let ts = ts.iter().zip(&want).map(|(t, w)| self.convert(t, w)).collect::<S<Vec<_>>>()?;
        Ok((Term::App(d.clone(), Args::Pos(ts)), got))
    }

    pub(super) fn primitive_application(&self, d: &Id, args: &Args) -> S<(Term, Ty)> {
        let msg = || {
            err(format!(
                "The constructor {} is primitive, and should receive exactly one argument in functional style",
                d
            ))
        };
        let dom = self.dom(d)?;
        let tagged = |t: Term| Ok((Term::Tag(bx(t), d.clone()), Ty::Tagged(d.clone())));
        match (&dom, args) {
            (_, Args::Pos(ts)) if ts.len() > 1 => msg(),
            (Domain::AnyString, Args::Pos(ts)) if ts.len() == 1 => tagged(self.convert(&ts[0], &Ty::Strings)?),
            (Domain::Strings(ss), Args::Pos(ts)) if ss.len() == 1 && ts.is_empty() => {
                tagged(Term::StringLit(ss[0].clone()))
            }
            (Domain::Strings(ss), Args::Mods(ms)) if ss.len() == 1 && ms.is_empty() => {
                tagged(Term::StringLit(ss[0].clone()))
            }
            (Domain::Strings(_), Args::Pos(ts)) if ts.len() == 1 => tagged(self.convert(&ts[0], &Ty::Strings)?),
            (Domain::AnyInt, Args::Pos(ts)) if ts.len() == 1 => tagged(self.convert(&ts[0], &Ty::Ints)?),
            (Domain::Ints(is), Args::Mods(ms)) if is.len() == 1 && ms.is_empty() => tagged(Term::IntLit(is[0])),
            (Domain::Ints(is), Args::Pos(ts)) if is.len() == 1 && ts.is_empty() => tagged(Term::IntLit(is[0])),
            (Domain::Ints(_), Args::Pos(ts)) if ts.len() == 1 => tagged(self.convert(&ts[0], &Ty::Ints)?),
            _ => msg(),
        }
    }

    pub(super) fn modifiers(&self, ms: &[(Var, Term)]) -> S<Vec<(Var, Term)>> {
        ms.iter()
            .map(|(x, t)| Ok((x.clone(), self.convert(t, &Ty::Tagged(self.spec.type_of(x)))?)))
            .collect()
    }

    pub(super) fn arguments(&self, d: &Id, args: &Args) -> S<Args> {
        match args {
            Args::Mods(ms) => Ok(Args::Mods(self.modifiers(ms)?)),
            Args::Pos(ts) => match self.dom(d)? {
                Domain::Products(xs) => {
                    if xs.len() == ts.len() {
                        let ms: Vec<(Var, Term)> = xs.into_iter().zip(ts.iter().cloned()).collect();
                        Ok(Args::Mods(self.modifiers(&ms)?))
                    } else {
                        err(format!("elements of {} have {} components, {} given", d, xs.len(), ts.len()))
                    }
                }
                _ => err(format!("non-composite {} used in application", d)),
            },
        }
    }

    /// Check a term and give its type.
    pub fn term(&self, t0: &Term) -> S<(Term, Ty)> {
        use BinOp::*;
        match t0 {
            Term::CurrentTime => Ok((Term::CurrentTime, Ty::Ints)),
            Term::StringLit(_) => Ok((t0.clone(), Ty::Strings)),
            Term::IntLit(_) => Ok((t0.clone(), Ty::Ints)),
            Term::BoolLit(_) => Ok((t0.clone(), Ty::Bool)),
            Term::Ref(x) => {
                self.check_known(std::slice::from_ref(x))?;
                Ok((t0.clone(), Ty::Tagged(self.spec.type_of(x))))
            }
            Term::App(d, args) if self.spec.find(d).is_none() && self.spec.host.sig(d).is_some() => self.host_application(d, args),
            Term::App(d, args) => {
                self.check_known(&[Var::plain(d)])?;
                match self.spec.find(d).map(|t| &t.domain) {
                    Some(Domain::Products(_)) => {
                        Ok((Term::App(d.clone(), self.arguments(d, args)?), Ty::Tagged(d.clone())))
                    }
                    _ => self.primitive_application(d, args),
                }
            }
            Term::Tag(_, _) => err("tagging is an auxiliary operation".into()),
            Term::Un(UnOp::Untag, _) => err("untagging is an auxiliary operation".into()),
            Term::Un(UnOp::Not, a) => Ok((Term::Un(UnOp::Not, bx(self.convert(a, &Ty::Bool)?)), Ty::Bool)),
            Term::Un(op, a) => {
                let (a2, ty) = self.term(a)?;
                match ty {
                    Ty::Tagged(_) => Ok((Term::Un(*op, bx(a2)), Ty::Bool)),
                    _ => err(match op {
                        UnOp::Present => "Holds(t) requires t to evaluate to a an instance, not a literal".into(),
                        UnOp::Violated => "Violated(t) requires t to evaluate to an instance, not a literal".into(),
                        _ => "Enabled(t) requires t to evaluate to an instance, not a literal".into(),
                    }),
                }
            }
            Term::Bin(op @ (And | Or), a, b) => {
                let a2 = self.convert(a, &Ty::Bool)?;
                let b2 = self.convert(b, &Ty::Bool)?;
                Ok((Term::Bin(*op, bx(a2), bx(b2)), Ty::Bool))
            }
            Term::Bin(op @ (Geq | Leq | Le | Ge), a, b) => {
                let a2 = self.convert(a, &Ty::Ints)?;
                let b2 = self.convert(b, &Ty::Ints)?;
                Ok((Term::Bin(*op, bx(a2), bx(b2)), Ty::Bool))
            }
            Term::Bin(op @ (Mult | Mod | Div | Sub | Add), a, b) => {
                let a2 = self.convert(a, &Ty::Ints)?;
                let b2 = self.convert(b, &Ty::Ints)?;
                Ok((Term::Bin(*op, bx(a2), bx(b2)), Ty::Ints))
            }
            Term::Bin(op @ (Eq | Neq), a, b) => {
                let left_first = || -> S<(Term, Ty)> {
                    let (a2, ty) = self.term(a)?;
                    let b2 = self.convert(b, &ty)?;
                    Ok((Term::Bin(*op, bx(a2), bx(b2)), Ty::Bool))
                };
                let right_first = || -> S<(Term, Ty)> {
                    let (b2, ty) = self.term(b)?;
                    let a2 = self.convert(a, &ty)?;
                    Ok((Term::Bin(*op, bx(a2), bx(b2)), Ty::Bool))
                };
                match left_first() {
                    Ok(r) => Ok(r),
                    Err(mut e1) => match right_first() {
                        Ok(r) => Ok(r),
                        Err(e2) => {
                            e1.extend(e2);
                            Err(e1)
                        }
                    },
                }
            }
            Term::Bin(When, a, b) => {
                let (a2, ty) = self.term(a)?;
                let b2 = self.convert(b, &Ty::Bool)?;
                Ok((Term::Bin(When, bx(a2), bx(b2)), ty))
            }
            Term::Q(q @ (Quant::Sum | Quant::Max | Quant::Min), xs, a) => {
                Ok((Term::Q(*q, xs.clone(), bx(self.convert(a, &Ty::Ints)?)), Ty::Ints))
            }
            Term::Q(Quant::Count, xs, a) => {
                let (a2, _) = self.term(a)?;
                Ok((Term::Q(Quant::Count, xs.clone(), bx(a2)), Ty::Ints))
            }
            Term::Q(q, xs, a) => Ok((Term::Q(*q, xs.clone(), bx(self.convert(a, &Ty::Bool)?)), Ty::Bool)),
            Term::Project(a, x) => {
                let (a2, ty) = self.term(a)?;
                let what = format!("project({},{})", dump::term(a), dump::var(x));
                match &ty {
                    Ty::Tagged(d) => match self.dom(d)? {
                        Domain::Products(xs) => {
                            if xs.contains(x) {
                                Ok((Term::Project(bx(a2), x.clone()), Ty::Tagged(self.spec.type_of(x))))
                            } else {
                                err(format!(
                                    "{} expects t to evaluate to a value of a product-type containing a reference to {} instead got: [{}]",
                                    what,
                                    dump::var(x),
                                    xs.iter().map(dump::var).collect::<Vec<_>>().join(",")
                                ))
                            }
                        }
                        dom => err(format!(
                            "{} expects t to evaluate to a value of a product-type, instead got: {}",
                            what,
                            dump::domain(&dom)
                        )),
                    },
                    _ => err(format!(
                        "{} expects t to evaluate to a tagged-element, instead got: {}",
                        what,
                        show_ty(&ty)
                    )),
                }
            }
        }
    }
}
