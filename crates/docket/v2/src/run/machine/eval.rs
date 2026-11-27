//! Stepping an expression (spec 7, 8.1): each form evaluated, its parts gathered, then used.

use super::*;

impl<N: Norms> Machine<N> {
    pub(super) fn gather(&mut self, r: RunId, xs: Vec<Expr>, env: Env, m: Arc<str>, then: Then) -> Result<(), Fault> {
        let mut todo: VecDeque<Expr> = xs.into();
        match todo.pop_front() {
            None => self.finish_gather(r, Vec::new(), env, m, then),
            Some(first) => {
                self.runs[r].frames.push(Frame::Gather { done: Vec::new(), todo, env: env.clone(), m: m.clone(), then });
                self.set(r, Ctl::Eval(first, env, m))
            }
        }
    }

    pub(super) fn eval(&mut self, r: RunId, e: Expr, env: Env, m: Arc<str>) -> Result<(), Fault> {
        let give = |me: &mut Self, v: Value| me.set(r, Ctl::Give(v));
        match e.e {
            E::Lit(l) => give(self, lit(&l)),
            E::Text(ps) => {
                let holes: Vec<Expr> = ps.iter().filter_map(|p| if let Piece::Hole(x) = p { Some(x.clone()) } else { None }).collect();
                self.gather(r, holes, env, m, Then::Text(ps))
            }
            E::Var(x) => match env.get(&x) {
                Some(v) => give(self, v.clone()),
                None => Err(format!("{x} is not bound in run {}", self.runs[r].name)),
            },
            E::Name(p) => {
                let v = self.name(r, &p)?;
                match v {
                    Named::Value(v) => give(self, v),
                    Named::Act(i) => {
                        let m2 = m.clone();
                        self.perform_act(r, i, m2)
                    }
                }
            }
            E::Apply(p, args) => {
                let q = p.to_string();
                // a program-typed argument of a `fn` is a program value, not run (8.5)
                if let Some(Sym::Fn { fields, .. }) = self.loaded.syms.get(&q) {
                    let progs: Vec<bool> = fields.iter().map(|f| matches!(f.ty, Type::Program(..))).collect();
                    let mut slots = Vec::new();
                    let mut todo = Vec::new();
                    for (k, a) in args.into_iter().enumerate() {
                        if progs.get(k) == Some(&true) && !self.gives_program(&a) {
                            slots.push(Some(Value::Program(Prog(Arc::new(Closure { body: a, env: env.clone(), module: m.to_string() })))));
                        } else {
                            slots.push(None);
                            todo.push(a);
                        }
                    }
                    return self.gather(r, todo, env, m, Then::ApplyFn(q, slots));
                }
                self.gather(r, args, env, m, Then::Apply(q))
            }
            E::Perform(p, args) => self.gather(r, args, env, m, Then::Perform(p.to_string())),
            E::Field(x, f) => self.gather(r, vec![*x], env, m, Then::Field(f)),
            E::Bin(op, a, b) => self.gather(r, vec![*a, *b], env, m, Then::Bin(op)),
            E::Neg(x) => self.gather(r, vec![*x], env, m, Then::Neg),
            E::Not(x) => self.gather(r, vec![*x], env, m, Then::Not),
            E::Holds(_) | E::Violated(_) | E::Enabled(_) | E::Quant(..) => {
                let b = self.holds(&e_of(e.at, e.e), &env)?;
                give(self, Value::Bool(b))
            }
            E::Foreach(vs, v, c) => {
                let l = self.foreach(&vs, &v, c.as_deref(), &env)?;
                give(self, Value::List(l))
            }
            E::Agg(a, x) => {
                let E::Foreach(vs, v, c) = &x.e else { return Err("`Count` and its kind take a `Foreach`".into()) };
                let l = self.foreach(vs, v, c.as_deref(), &env)?;
                let ns = || l.iter().filter_map(Value::as_int);
                give(
                    self,
                    Value::Int(match a {
                        Agg::Count => l.len() as i64,
                        Agg::Sum => ns().sum(),
                        Agg::Max => ns().max().unwrap_or(0),
                        Agg::Min => ns().min().unwrap_or(0),
                    }),
                )
            }
            E::Tuple(xs) => self.gather(r, xs, env, m, Then::Tuple),
            E::List(xs) => self.gather(r, xs, env, m, Then::List),
            E::SelfParty => {
                let p = self.runs[r].party.clone();
                give(self, p)
            }
            E::This => {
                let n = Value::Text(self.runs[r].name.clone());
                give(self, n)
            }
            E::Bind(x, p, rest) => {
                self.runs[r].frames.push(Frame::Bind(x, *rest, env.clone(), m.clone()));
                self.set(r, Ctl::Eval(*p, env, m))
            }
            E::Seq(p, rest) => {
                self.runs[r].frames.push(Frame::Seq(*rest, env.clone(), m.clone()));
                self.set(r, Ctl::Eval(*p, env, m))
            }
            E::Return(x) => self.set(r, Ctl::Eval(*x, env, m)),
            E::Handle(body, ret, arms) => {
                self.runs[r].frames.push(Frame::Handle(ret.map(|(x, b)| (x, *b)), arms, env.clone(), m.clone()));
                self.set(r, Ctl::Eval(*body, env, m))
            }
            E::Resume(k, x) => self.gather(r, vec![*x], env, m, Then::Resume(k)),
            E::Match(on, cases) => {
                self.runs[r].frames.push(Frame::Match(cases, env.clone(), m.clone()));
                self.set(r, Ctl::Eval(*on, env, m))
            }
            E::If(c, a, b) => {
                self.runs[r].frames.push(Frame::If(*a, b.map(|b| *b), env.clone(), m.clone()));
                if crate::check::typing::reads_facts(&self.loaded.syms, &c) {
                    let v = self.holds(&c, &env)?;
                    return self.set(r, Ctl::Give(Value::Bool(v)));
                }
                self.set(r, Ctl::Eval(*c, env, m))
            }
            E::Par(xs) => {
                let rest = xs.into_iter().enumerate().map(|(k, x)| (k, x, env.clone())).collect::<VecDeque<_>>();
                let n = rest.len();
                self.par(r, rest, n, false, m)
            }
            E::ParEach(x, l, n, body) => self.gather(r, vec![*l], env, m, Then::ParList(x, n, *body)),
            E::Iterate(x, init, n, body) => self.gather(r, vec![*init], env, m, Then::Iter(x, n, *body)),
            E::Do(x) => self.gather(r, vec![*x], env, m, Then::Do),
            E::Gap => {
                let site = format!("{m}:{}", e.at);
                self.runs[r].frames.push(Frame::Fill(m, site.clone()));
                self.set(r, Ctl::Perform("Gap".into(), vec![Value::Text(site)]))
            }
        }
    }

    /// Whether an argument gives a program value when evaluated (a variable, or a `fn` whose result is a program),
    /// so that it is not made into one.
    pub(super) fn gives_program(&self, a: &Expr) -> bool {
        match &a.e {
            E::Var(_) => true,
            E::Name(p) | E::Apply(p, _) => matches!(self.loaded.syms.get(&p.to_string()), Some(Sym::Fn { result: Type::Program(..), .. })),
            _ => false,
        }
    }

    /// What a declaration's name standing alone is: a constructor or instance with no fields, or an Act.
    pub(super) fn name(&mut self, r: RunId, p: &Path) -> Result<Named, Fault> {
        let q = p.to_string();
        match self.loaded.syms.get(&q) {
            Some(Sym::Ctor { .. }) => Ok(Named::Value(Value::Ctor { name: q, args: Vec::new() })),
            Some(Sym::Builtin) if q == "None" => Ok(Named::Value(Value::none())),
            Some(Sym::Norm { kind: Norm::Act | Norm::Event, .. }) => Ok(Named::Act(Instance { ty: q, args: Vec::new() })),
            Some(Sym::Norm { .. }) => Ok(Named::Value(Value::Inst(Instance { ty: q, args: Vec::new() }))),
            _ => Err(format!("{q} has no value in run {}", self.runs[r].name)),
        }
    }

    pub(super) fn finish_gather(&mut self, r: RunId, vs: Vec<Value>, env: Env, m: Arc<str>, then: Then) -> Result<(), Fault> {
        let give = |me: &mut Self, v: Value| me.set(r, Ctl::Give(v));
        match then {
            Then::Apply(q) => self.apply(r, q, vs, m),
            Then::ApplyFn(q, slots) => {
                let mut it = vs.into_iter();
                let all: Vec<Value> = slots.into_iter().map(|s| s.or_else(|| it.next()).unwrap_or(Value::Unit)).collect();
                self.apply(r, q, all, m)
            }
            Then::Perform(q) => {
                if q == "Charge" {
                    return Err("`perform Charge` in a program: only the engine performs it (10)".into());
                }
                self.set(r, Ctl::Perform(q, vs))
            }
            Then::Tuple => give(self, Value::Tuple(vs)),
            Then::List => give(self, Value::List(vs)),
            Then::Bin(op) => {
                let mut it = vs.into_iter();
                let (a, b) = (it.next().unwrap(), it.next().unwrap());
                let v = self.bin(op, a, b)?;
                give(self, v)
            }
            Then::Neg => match &vs[0] {
                Value::Int(n) => give(self, Value::Int(-n)),
                v => Err(format!("-{v}")),
            },
            Then::Not => {
                let b = self.truth(&vs[0])?;
                give(self, Value::Bool(!b))
            }
            Then::Field(f) => {
                let v = self.field(vs.into_iter().next().unwrap(), &f)?;
                give(self, v)
            }
            Then::Text(ps) => {
                let mut it = vs.into_iter();
                let s: String = ps
                    .iter()
                    .map(|p| match p {
                        Piece::Words(w) => w.clone(),
                        Piece::Hole(_) => match it.next() {
                            Some(Value::Text(t)) => t,
                            Some(v) => v.to_string(),
                            None => String::new(),
                        },
                    })
                    .collect();
                give(self, Value::Text(s))
            }
            Then::Resume(k) => self.resume(r, &k, vs.into_iter().next().unwrap(), &env),
            Then::Do => match vs.into_iter().next().unwrap() {
                Value::Program(Prog(c)) => self.set(r, Ctl::Eval(c.body.clone(), c.env.clone(), c.module.as_str().into())),
                v => Err(format!("`do` of {v}, which is no program")),
            },
            Then::ParList(x, n, body) => {
                let Value::List(elems) = vs.into_iter().next().unwrap() else { return Err("`par … in` of what is no list".into()) };
                let rest = elems
                    .into_iter()
                    .enumerate()
                    .map(|(k, it)| {
                        let mut e = env.clone();
                        e.insert(x.clone(), it);
                        (k, body.clone(), e)
                    })
                    .collect::<VecDeque<_>>();
                let up = n.map_or(rest.len(), |n| n.max(1) as usize);
                self.par(r, rest, up, true, m)
            }
            Then::Iter(x, n, body) => {
                let s = vs.into_iter().next().unwrap();
                let mut e = env.clone();
                e.insert(x.clone(), s);
                self.runs[r].frames.push(Frame::Loop { x, body: body.clone(), n, round: 1, env, m: m.clone() });
                self.set(r, Ctl::Eval(body, e, m))
            }
        }
    }

    pub(super) fn apply(&mut self, r: RunId, q: String, vs: Vec<Value>, m: Arc<str>) -> Result<(), Fault> {
        match self.loaded.syms.get(&q).cloned() {
            Some(Sym::Norm { kind: Norm::Act | Norm::Event, .. }) => self.perform_act(r, Instance { ty: q, args: vs }, m),
            Some(Sym::Fn { fields, result, body, module, .. }) => {
                let env: Env = fields.iter().map(|f| f.name.clone()).zip(vs).collect();
                if matches!(result, Type::Program(..)) {
                    // a fn whose result is a program builds it and runs nothing (8.5)
                    return self.set(r, Ctl::Give(Value::Program(Prog(Arc::new(Closure { body, env, module })))));
                }
                self.set(r, Ctl::Eval(body, env, module.as_str().into()))
            }
            Some(Sym::Effect { .. }) => Err(format!("{q} is an Effect: it is performed with `perform`")),
            _ => {
                let v = self.construct(&q, vs)?;
                self.set(r, Ctl::Give(v))
            }
        }
    }
}
