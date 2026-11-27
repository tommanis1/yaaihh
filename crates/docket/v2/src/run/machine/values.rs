//! Values without effects (spec 6): constructors, fields, operators, truth, `Foreach`, patterns.

use super::*;

impl<N: Norms> Machine<N> {
    pub(super) fn foreach(&mut self, vs: &[String], v: &Expr, c: Option<&Expr>, env: &Env) -> Result<Vec<Value>, Fault> {
        let envs = match c {
            Some(c) => self.each(c, env)?,
            None => return Err("a `Foreach` with no `When` ranges over nothing that holds".into()),
        };
        let mut out = Vec::new();
        for e in envs {
            if vs.iter().all(|x| e.contains_key(x)) {
                out.push(self.pure(v, &e)?);
            }
        }
        Ok(out)
    }

    /// The value of an expression that performs nothing and reads no state: a `Foreach`'s value.
    pub(super) fn pure(&mut self, e: &Expr, env: &Env) -> Result<Value, Fault> {
        Ok(match &e.e {
            E::Lit(l) => lit(l),
            E::Var(x) => match env.get(x) {
                Some(v) => v.clone(),
                None => return Err(format!("{x} is no value here")),
            },
            E::Name(p) => self.construct(&p.to_string(), Vec::new())?,
            E::Tuple(xs) => Value::Tuple(xs.iter().map(|x| self.pure(x, env)).collect::<Result<_, _>>()?),
            E::Apply(p, xs) => {
                let vs: Vec<Value> = xs.iter().map(|x| self.pure(x, env)).collect::<Result<_, _>>()?;
                self.construct(&p.to_string(), vs)?
            }
            E::Field(x, f) => {
                let v = self.pure(x, env)?;
                self.field(v, f)?
            }
            E::Bin(op, a, b) => {
                let (a, b) = (self.pure(a, env)?, self.pure(b, env)?);
                self.bin(*op, a, b)?
            }
            _ => return Err("a `Foreach` gives a name, a tuple, an instance, a field or arithmetic".into()),
        })
    }

    /// A constructor, an instance, a prompt or a host function's value; none of these performs anything.
    pub(super) fn construct(&mut self, q: &str, vs: Vec<Value>) -> Result<Value, Fault> {
        Ok(match self.loaded.syms.get(q) {
            Some(Sym::Ctor { .. }) => Value::Ctor { name: q.to_string(), args: vs },
            // an instance of a type identified by a primitive is that primitive
            Some(Sym::Norm { kind: Norm::Fact | Norm::Duty, fields: Fields::By(_), .. }) => vs.into_iter().next().ok_or(format!("{q} takes its one value"))?,
            Some(Sym::Norm { kind: Norm::Fact | Norm::Duty, .. }) => Value::Inst(Instance { ty: q.to_string(), args: vs }),
            Some(Sym::Call { .. }) => Value::Prompt { name: q.to_string(), args: vs },
            Some(Sym::Record { fields, .. }) => Value::Record { ty: q.to_string(), fields: fields.iter().map(|f| f.name.clone()).zip(vs).collect() },
            Some(Sym::Builtin) if matches!(q, "Some" | "Again" | "Done") => Value::ctor(q, vs),
            Some(Sym::Builtin) if host::is_host(q) => host::apply(q, &vs)?,
            _ => return Err(format!("{q} builds no value")),
        })
    }

    pub(super) fn field(&self, v: Value, f: &str) -> Result<Value, Fault> {
        match &v {
            Value::Record { fields, .. } => fields.iter().find(|(n, _)| n == f).map(|(_, x)| x.clone()),
            Value::Inst(i) => match self.loaded.syms.get(&i.ty) {
                Some(Sym::Norm { fields: Fields::Named(fs), .. }) => fs.iter().position(|x| x.name == f).and_then(|k| i.args.get(k).cloned()),
                _ => None,
            },
            Value::Ctor { name, args, .. } => match self.loaded.syms.get(name) {
                Some(Sym::Ctor { fields, .. }) => fields.iter().position(|x| x.name == f).and_then(|k| args.get(k).cloned()),
                _ => None,
            },
            _ => None,
        }
        .ok_or(format!("{v} has no field {f}"))
    }

    pub(super) fn truth(&mut self, v: &Value) -> Result<bool, Fault> {
        match v {
            Value::Bool(b) => Ok(*b),
            Value::Inst(i) => Ok(self.norms.holds(i)),
            other => Err(format!("{other} is no condition")),
        }
    }

    pub(super) fn bin(&mut self, op: Op, a: Value, b: Value) -> Result<Value, Fault> {
        use Value::*;
        Ok(match (op, a, b) {
            (Op::And, a, b) => Bool(self.truth(&a)? && self.truth(&b)?),
            (Op::Or, a, b) => Bool(self.truth(&a)? || self.truth(&b)?),
            (Op::Eq, a, b) => Bool(a == b),
            (Op::Ne, a, b) => Bool(a != b),
            (Op::Lt, a, b) => Bool(a < b),
            (Op::Le, a, b) => Bool(a <= b),
            (Op::Gt, a, b) => Bool(a > b),
            (Op::Ge, a, b) => Bool(a >= b),
            (Op::Add, Int(x), Int(y)) => Int(x.checked_add(y).ok_or("a sum past the largest number")?),
            (Op::Add, Text(x), Text(y)) => Text(x + &y),
            (Op::Sub, Int(x), Int(y)) => Int(x.checked_sub(y).ok_or("a difference past the smallest number")?),
            (Op::Mul, Int(x), Int(y)) => Int(x.checked_mul(y).ok_or("a product past the largest number")?),
            (Op::Div, Int(_), Int(0)) | (Op::Rem, Int(_), Int(0)) => return Err("a division by zero".into()),
            (Op::Div, Int(x), Int(y)) => Int(x.div_euclid(y)),
            (Op::Rem, Int(x), Int(y)) => Int(x.rem_euclid(y)),
            (op, a, b) => return Err(format!("{op:?} of {a} and {b}")),
        })
    }

    pub(super) fn bind(&self, p: &Pat, v: &Value, env: &mut Env) -> bool {
        match (p, v) {
            (Pat::Any, _) => true,
            (Pat::Bind(n), v) => {
                env.insert(n.clone(), v.clone());
                true
            }
            (Pat::Lit(l), v) => lit(l) == *v,
            (Pat::Text(s), Value::Text(t)) => s == t,
            (Pat::Ctor(c, ps), Value::Ctor { name, args, .. }) => c.to_string() == *name && ps.len() == args.len() && ps.iter().zip(args).all(|(p, a)| self.bind(p, a, env)),
            (Pat::Ctor(c, ps), Value::Inst(i)) => c.to_string() == i.ty && ps.len() == i.args.len() && ps.iter().zip(&i.args).all(|(p, a)| self.bind(p, a, env)),
            // an instance of a type identified by a primitive is that primitive (5.4)
            (Pat::Ctor(c, ps), v) if ps.len() == 1 && matches!(self.loaded.syms.get(&c.to_string()), Some(Sym::Norm { fields: Fields::By(_), .. })) => self.bind(&ps[0], v, env),
            (Pat::Tuple(ps), Value::Tuple(vs)) => ps.len() == vs.len() && ps.iter().zip(vs).all(|(p, a)| self.bind(p, a, env)),
            (Pat::List(ps, rest), Value::List(vs)) => {
                let fits = if rest.is_some() { vs.len() >= ps.len() } else { vs.len() == ps.len() };
                if !fits || !ps.iter().zip(vs).all(|(p, a)| self.bind(p, a, env)) {
                    return false;
                }
                if let Some(Some(n)) = rest {
                    env.insert(n.clone(), Value::List(vs[ps.len()..].to_vec()));
                }
                true
            }
            _ => false,
        }
    }

    /// Whether value `v` is of type `t` (6.1); a driver's answer is checked by it where it crosses (12). A type
    /// parameter takes any value.
    pub(super) fn fits(&self, v: &Value, t: &Type) -> bool {
        let each = |vs: &[Value], ts: &[Field]| vs.len() == ts.len() && vs.iter().zip(ts).all(|(x, f)| self.fits(x, &f.ty));
        match t {
            Type::Tuple(ts) => matches!(v, Value::Tuple(vs) if vs.len() == ts.len() && vs.iter().zip(ts).all(|(x, t)| self.fits(x, t))),
            Type::Program(..) => matches!(v, Value::Program(_)),
            Type::Named(p, args) => {
                let of = |x: &Value| args.first().is_none_or(|t| self.fits(x, t));
                match (p.to_string().as_str(), v) {
                    ("Int", Value::Int(_)) | ("String", Value::Text(_)) | ("Bool", Value::Bool(_)) | ("Unit", Value::Unit) | ("Call", Value::Prompt { .. }) => true,
                    ("List", Value::List(vs)) => vs.iter().all(of),
                    ("Option", Value::Ctor { name, args: a }) => (name == "None" && a.is_empty()) || (name == "Some" && a.len() == 1 && of(&a[0])),
                    ("Loop", Value::Ctor { name, args: a }) => matches!(name.as_str(), "Again" | "Done") && a.len() == 1,
                    (q, v) => match self.loaded.syms.get(q) {
                        Some(Sym::Sum { .. }) => matches!(v, Value::Ctor { name, args: a } if matches!(self.loaded.syms.get(name), Some(Sym::Ctor { ty, fields, .. }) if ty == q && each(a, fields))),
                        Some(Sym::Record { fields, .. }) => matches!(v, Value::Record { ty, fields: fv } if ty == q && fv.len() == fields.len() && fv.iter().zip(fields).all(|((n, x), f)| *n == f.name && self.fits(x, &f.ty))),
                        Some(Sym::Norm { fields: Fields::By(t), .. }) => self.fits(v, t),
                        Some(Sym::Norm { fields: Fields::Named(fs), .. }) => matches!(v, Value::Inst(i) if i.ty == q && each(&i.args, fs)),
                        Some(_) => false,
                        None => true,
                    },
                }
            }
        }
    }

    /// Every way condition `c` holds with `env` (6.2), each field of a bound value read here first.
    pub(super) fn each(&mut self, c: &Expr, env: &Env) -> Result<Vec<Env>, Fault> {
        let (c, env) = self.closed(c, env);
        self.norms.each(&c, &env)
    }

    /// Whether condition `c` holds with `env`, each field of a bound value read here first.
    pub(super) fn holds(&mut self, c: &Expr, env: &Env) -> Result<bool, Fault> {
        let (c, env) = self.closed(c, env);
        self.norms.cond(&c, &env)
    }

    /// Condition `c` as the core reads it: each field of a value `env` binds is read here and given as a variable
    /// of its own, since the core reads facts and plain values, not records or constructors.
    fn closed(&self, c: &Expr, env: &Env) -> (Expr, Env) {
        let mut env = env.clone();
        let c = self.close(c, &mut env);
        (c, env)
    }

    fn close(&self, c: &Expr, env: &mut Env) -> Expr {
        let mut go = |x: &Expr| Box::new(self.close(x, env));
        let e = match &c.e {
            E::Field(..) => match self.read(c, env) {
                Some(v) => {
                    // a name no text can write
                    let n = format!(".{}", env.len());
                    env.insert(n.clone(), v);
                    E::Var(n)
                }
                None => c.e.clone(),
            },
            E::Apply(p, xs) => E::Apply(p.clone(), xs.iter().map(|x| *go(x)).collect()),
            E::Bin(op, a, b) => E::Bin(*op, go(a), go(b)),
            E::Not(x) => E::Not(go(x)),
            E::Neg(x) => E::Neg(go(x)),
            E::Holds(x) => E::Holds(go(x)),
            E::Violated(x) => E::Violated(go(x)),
            E::Enabled(x) => E::Enabled(go(x)),
            E::Quant(q, vs, b) => E::Quant(*q, vs.clone(), go(b)),
            E::Agg(a, x) => E::Agg(*a, go(x)),
            other => other.clone(),
        };
        Expr { at: c.at, e }
    }

    /// The value of `x.f…` with `x` bound in `env`, read without the core.
    fn read(&self, e: &Expr, env: &Env) -> Option<Value> {
        match &e.e {
            E::Var(n) => env.get(n).cloned(),
            E::Field(x, f) => self.field(self.read(x, env)?, f).ok(),
            _ => None,
        }
    }
}
