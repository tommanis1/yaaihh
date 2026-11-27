//! The evaluation of a term to its results, in the order they are reported.

use super::*;

impl<'a> Ev<'a> {
    pub fn eval(&mut self, t0: &Term) -> R<Vec<Value>> {
        match t0 {
            Term::Bin(BinOp::Eq | BinOp::Neq | BinOp::Leq | BinOp::Le | BinOp::Geq | BinOp::Ge, ..) if self.tracking() => self.compared(t0),
            _ => self.eval_here(t0),
        }
    }

    /// T0 evaluated, a comparison included.
    pub(super) fn eval_here(&mut self, t0: &Term) -> R<Vec<Value>> {
        use BinOp::*;
        match t0 {
            Term::CurrentTime => Ok(vec![Value::Int(self.state.time)]),
            Term::StringLit(s) => Ok(vec![Value::Str(s.clone())]),
            Term::BoolLit(b) => Ok(vec![Value::Bool(*b)]),
            Term::IntLit(i) => Ok(vec![Value::Int(*i)]),
            Term::Ref(x) => Ok(vec![Value::Tagged(self.lookup(x)?)]),
            Term::App(d, Args::Pos(ts)) if self.spec.find(d).is_none() && self.spec.host.sig(d).is_some() => {
                let given = self.sequence(ts.len(), &mut |ev, i| ev.eval(&ts[i]))?;
                given.iter().map(|vs| self.spec.host.call(d, vs).map_err(RtErr::Internal)).collect()
            }
            Term::App(d, params) => {
                let t = self.tspec(d)?;
                let Domain::Products(xs) = &t.domain else {
                    return Err(RtErr::Internal(format!("application of primitive type: {}", d)));
                };
                let replacements: Vec<(&Var, &Term)> = match params {
                    Args::Mods(ms) => ms.iter().map(|(v, t)| (v, t)).collect(),
                    Args::Pos(ts) => xs.iter().zip(ts.iter()).collect(),
                };
                let choices = self.sequence(replacements.len(), &mut |ev, i| {
                    let (x, term) = replacements[i];
                    Ok(taggeds(ev.eval(term)?).into_iter().map(|te| (x.clone(), te)).collect())
                })?;
                bind(choices, |given| {
                    let args: Vec<Tagged> =
                        self.with(given, |ev| xs.iter().map(|x| ev.lookup(x)).collect::<R<Vec<_>>>())?;
                    let b: Vec<(Var, Tagged)> = xs.iter().cloned().zip(args.iter().cloned()).collect();
                    let checks = self.with(b, |ev| ev.eval(&t.domain_constraint))?;
                    let te = Tagged { elem: Elem::Prod(args.into()), dom: d.clone() };
                    Ok(checks
                        .into_iter()
                        .filter(|v| *v == Value::Bool(true))
                        .map(|_| Value::Tagged(te.clone()))
                        .collect())
                })
            }
            Term::Tag(a, d) => {
                let vs = self.eval(a)?;
                bind(vs, |v| {
                    Ok(match v {
                        Value::Int(i) => vec![Value::Tagged(Tagged { elem: Elem::Int(i), dom: d.clone() })],
                        Value::Str(s) => vec![Value::Tagged(Tagged { elem: Elem::Str(s), dom: d.clone() })],
                        Value::Tagged(te) => vec![Value::Tagged(Tagged { elem: te.elem, dom: d.clone() })],
                        Value::Bool(_) => vec![],
                    })
                })
            }
            Term::Un(UnOp::Untag, a) => {
                let vs = self.eval(a)?;
                bind(vs, |v| {
                    Ok(match v {
                        Value::Tagged(Tagged { elem: Elem::Int(i), .. }) => vec![Value::Int(i)],
                        Value::Tagged(Tagged { elem: Elem::Str(s), .. }) => vec![Value::Str(s)],
                        _ => vec![],
                    })
                })
            }
            Term::Un(UnOp::Not, a) => Ok(bools(self.flipped(|ev| ev.eval(a))?).into_iter().map(|b| Value::Bool(!b)).collect()),
            Term::Bin(op @ (And | Or), a, b) => {
                let stop = *op == Or;
                let firsts = only_bools(self.eval(a)?);
                let mut second: Option<Vec<bool>> = None;
                bind(firsts, |x| {
                    if x == stop {
                        return Ok(vec![Value::Bool(stop)]);
                    }
                    if second.is_none() {
                        second = Some(bools(self.eval(b)?));
                    }
                    Ok(second.clone().unwrap().into_iter().map(Value::Bool).collect())
                })
            }
            Term::Bin(op @ (Leq | Le | Geq | Ge | Mult | Mod | Div | Sub | Add), a, b) => {
                let firsts = only_ints(self.eval(a)?);
                let mut second: Option<Vec<i64>> = None;
                bind(firsts, |x| {
                    if second.is_none() {
                        second = Some(ints(self.eval(b)?));
                    }
                    second.clone().unwrap().into_iter().map(|y| {
                        Ok(match op {
                            Leq => Value::Bool(x <= y),
                            Le => Value::Bool(x < y),
                            Geq => Value::Bool(x >= y),
                            Ge => Value::Bool(x > y),
                            Mult => Value::Int(x.wrapping_mul(y)),
                            Sub => Value::Int(x.wrapping_sub(y)),
                            Add => Value::Int(x.wrapping_add(y)),
                            _ => {
                                if y == 0 {
                                    return Err(RtErr::Internal("divide by zero".into()));
                                }
                                // rounding towards negative infinity
                                let (q, r) = (x.wrapping_div(y), x.wrapping_rem(y));
                                let adjust = r != 0 && ((r < 0) != (y < 0));
                                if *op == Div {
                                    Value::Int(if adjust { q - 1 } else { q })
                                } else {
                                    Value::Int(if adjust { r + y } else { r })
                                }
                            }
                        })
                    }).collect()
                })
            }
            Term::Bin(op @ (Eq | Neq), a, b) => {
                let firsts = fmap(self.eval(a)?, |v| v);
                if firsts.is_empty() {
                    return Ok(Vec::new());
                }
                let seconds = self.eval(b)?;
                bind(firsts, |x| bind(seconds.clone(), |y| Ok(vec![Value::Bool((x == y) == (*op == Eq))])))
            }
            Term::Bin(When, a, b) => {
                let firsts = self.eval(a)?;
                let mut second: Option<Vec<bool>> = None;
                bind(firsts, |v| {
                    if second.is_none() {
                        second = Some(bools(self.eval(b)?));
                    }
                    Ok(second.clone().unwrap().into_iter().filter(|c| *c).map(|_| v.clone()).collect())
                })
            }
            Term::Q(Quant::Sum, xs, a) => {
                let vs = ints(self.aggregated(|ev| ev.quantified(xs, a))?);
                Ok(vec![Value::Int(vs.into_iter().fold(0i64, |s, v| s.wrapping_add(v)))])
            }
            Term::Q(Quant::Count, xs, a) => {
                let vs = self.aggregated(|ev| ev.quantified(xs, a))?;
                Ok(vec![Value::Int(vs.len() as i64)])
            }
            Term::Q(Quant::Max, xs, a) => {
                let vs = ints(self.aggregated(|ev| ev.quantified(xs, a))?);
                Ok(vec![Value::Int(vs.into_iter().max().unwrap_or(0))])
            }
            Term::Q(Quant::Min, xs, a) => {
                let vs = ints(self.aggregated(|ev| ev.quantified(xs, a))?);
                Ok(vec![Value::Int(vs.into_iter().min().unwrap_or(0))])
            }
            Term::Q(Quant::Exists, xs, a) => {
                if self.spec.reading.guided {
                    if let Some(found) = self.exists_from_facts(xs, a)? {
                        return Ok(vec![Value::Bool(found)]);
                    }
                }
                let vs = self.foreach(xs, &mut |ev| {
                    Ok(ev.eval(a)?.into_iter().filter(|v| *v == Value::Bool(true)).collect())
                })?;
                Ok(vec![Value::Bool(!vs.is_empty())])
            }
            Term::Q(Quant::Forall, xs, a) => {
                // what is ranged over is read as under `Not`, each case as it stands: `Not(Exists x : Not(a))`
                let vs = self.flipped(|ev| ev.foreach(xs, &mut |ev| ev.flipped(|ev| Ok(bools(ev.eval(a)?)))))?;
                Ok(vec![Value::Bool(vs.into_iter().all(|b| b))])
            }
            Term::Un(op @ (UnOp::Present | UnOp::Violated | UnOp::Enabled), a) => {
                let tes = only_taggeds(self.eval(a)?);
                bind(tes, |te| {
                    let bs = match op {
                        UnOp::Present => self.is_holds(&te)?,
                        // read on this side: a `Not` inside the conditions turns the side itself
                        UnOp::Violated => self.is_violated(&te)?,
                        _ => self.is_enabled(&te)?,
                    };
                    Ok(fmap(bs, Value::Bool))
                })
            }
            Term::Project(a, x) => {
                let tes = only_taggeds(self.eval(a)?);
                bind(tes, |te| {
                    let t = self.tspec(&te.dom)?;
                    Ok(match (&te.elem, &t.domain) {
                        (Elem::Prod(args), Domain::Products(rs)) if args.len() == rs.len() => {
                            match rs.iter().position(|r| r == x) {
                                Some(j) => vec![Value::Tagged(args[j].clone())],
                                None => vec![],
                            }
                        }
                        _ => vec![],
                    })
                })
            }
        }
    }

    /// The values of A over every binding of XS: from the facts its condition opens with where the reading is guided
    /// ([`Ev::values_from_facts`]), else by trying every binding.
    fn quantified(&mut self, xs: &[Var], a: &Term) -> R<Vec<Value>> {
        if self.spec.reading.guided {
            if let Some(vs) = self.values_from_facts(xs, a)? {
                return Ok(vs);
            }
        }
        self.foreach(xs, &mut |ev| ev.eval(a))
    }
}
