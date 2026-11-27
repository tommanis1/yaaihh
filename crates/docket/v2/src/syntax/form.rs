//! The written form of a tree: Docket text, bracketed, without positions; what hashes and keys read (spec 11).

use super::tree::*;

/// A declaration in its written form.
pub fn decl(d: &DeclKind) -> String {
    let mut w = W(String::new());
    w.decl(d);
    w.0
}

/// An expression in its written form.
pub fn expr(e: &Expr) -> String {
    let mut w = W(String::new());
    w.expr(e);
    w.0
}

struct W(String);

impl W {
    fn s(&mut self, t: &str) {
        self.0.push_str(t);
    }
    fn text(&mut self, t: &str) {
        self.0.push_str(&format!("{t:?}"));
    }
    fn list<T>(&mut self, open: &str, xs: &[T], close: &str, mut one: impl FnMut(&mut Self, &T)) {
        self.s(open);
        for (k, x) in xs.iter().enumerate() {
            if k > 0 {
                self.s(", ");
            }
            one(self, x);
        }
        self.s(close);
    }
    fn params(&mut self, ps: &[String]) {
        if !ps.is_empty() {
            self.list("<", ps, ">", |w, p| w.s(p));
        }
    }
    fn fields(&mut self, fs: &[Field]) {
        self.list("(", fs, ")", |w, f| {
            w.s(&f.name);
            w.s(": ");
            w.ty(&f.ty);
        });
    }
    fn ty(&mut self, t: &Type) {
        match t {
            Type::Named(p, args) => {
                self.s(&p.to_string());
                if !args.is_empty() {
                    self.list("<", args, ">", Self::ty);
                }
            }
            Type::Tuple(ts) => self.list("(", ts, ")", Self::ty),
            Type::Program(t, row) => {
                self.s("Program<");
                self.ty(t);
                if !row.implied {
                    self.s(" ! {");
                    self.list("", &row.effects, "", |w, e| w.s(&e.to_string()));
                    if let Some(r) = &row.rest {
                        self.s(if row.effects.is_empty() { ".." } else { ", .." });
                        self.s(r);
                    }
                    self.s("}");
                }
                self.s(">");
            }
        }
    }
    fn norm(&mut self, k: Norm) {
        self.s(match k {
            Norm::Fact => "Fact",
            Norm::Act => "Act",
            Norm::Event => "Event",
            Norm::Duty => "Duty",
        });
    }
    fn decl(&mut self, d: &DeclKind) {
        match d {
            DeclKind::Norm { kind, name, fields, clauses } => {
                self.norm(*kind);
                self.s(" ");
                self.s(name);
                match fields {
                    Fields::By(t) => {
                        self.s(" Identified by ");
                        self.ty(t);
                    }
                    Fields::Named(fs) => self.fields(fs),
                }
                self.clauses(clauses);
            }
            DeclKind::Extend { kind, name, clauses } => {
                self.s("Extend ");
                self.norm(*kind);
                self.s(" ");
                self.s(&name.to_string());
                self.clauses(clauses);
            }
            DeclKind::Holds(e) => {
                self.s("+");
                self.expr(e);
                self.s(".");
            }
            DeclKind::Effect { name, params, fields, result, does } => {
                self.s("Effect ");
                self.s(name);
                self.params(params);
                self.fields(fields);
                self.s(" -> ");
                self.ty(result);
                if let Some(b) = does {
                    self.s(" Does ");
                    self.block(b);
                }
            }
            DeclKind::Call { name, fields, result, template } => {
                self.s("Call ");
                self.s(name);
                self.fields(fields);
                self.s(" -> ");
                self.ty(result);
                self.s(" Template ");
                self.pieces(template);
            }
            DeclKind::Sum { name, params, variants } => {
                self.s("type ");
                self.s(name);
                self.params(params);
                self.s(" =");
                for v in variants {
                    self.s(" | ");
                    self.s(&v.name);
                    self.fields(&v.fields);
                    if let Some(n) = &v.needs {
                        self.s(" Needs ");
                        self.text(n);
                    }
                }
            }
            DeclKind::Record { name, params, fields } => {
                self.s("type ");
                self.s(name);
                self.params(params);
                self.list(" { ", fields, " }", |w, f| {
                    w.s(&f.name);
                    w.s(": ");
                    w.ty(&f.ty);
                });
            }
            DeclKind::Fn { name, params, fields, result, body } => {
                self.s("fn ");
                self.s(name);
                self.params(params);
                self.fields(fields);
                self.s(" -> ");
                self.ty(result);
                self.s(" ");
                self.block(body);
            }
            DeclKind::On { target, pats, cond, body } => {
                self.s("on ");
                self.s(&target.to_string());
                self.list("(", pats, ")", Self::pat);
                self.cond(cond.as_ref());
                self.s(" do ");
                self.block(body);
            }
        }
    }
    fn clauses(&mut self, cs: &[Clause]) {
        for c in cs {
            let (word, e) = match c {
                Clause::HoldsWhen(e) => ("Holds when", Some(e)),
                Clause::ViolatedWhen(e) => ("Violated when", Some(e)),
                Clause::ConditionedBy(e) => ("Conditioned by", Some(e)),
                Clause::DerivedFrom(e) => ("Derived from", Some(e)),
                Clause::Prefer(e) => ("Prefer", Some(e)),
                Clause::Does(e) => {
                    self.s(" Does ");
                    self.block(e);
                    continue;
                }
                Clause::Returns(t) => {
                    self.s(" Returns ");
                    self.ty(t);
                    continue;
                }
                Clause::Creates(l) => ("Creates", self.effectful("Creates", l)),
                Clause::Terminates(l) => ("Terminates", self.effectful("Terminates", l)),
                Clause::Obfuscates(l) => ("Obfuscates", self.effectful("Obfuscates", l)),
                Clause::SyncsWith(l) => ("Syncs with", self.effectful("Syncs with", l)),
            };
            if let Some(e) = e {
                self.s(" ");
                self.s(word);
                self.s(" ");
                self.expr(e);
            }
        }
    }
    fn effectful<'e>(&mut self, word: &str, l: &[Effectful]) -> Option<&'e Expr> {
        self.s(" ");
        self.s(word);
        self.list(" ", l, "", |w, x| {
            w.expr(&x.inst);
            if let Some(c) = &x.when {
                w.s(" When ");
                w.expr(c);
            }
        });
        None
    }
    fn cond(&mut self, c: Option<&Expr>) {
        if let Some(c) = c {
            self.s(" where ");
            self.expr(c);
        }
    }
    fn pieces(&mut self, ps: &[Piece]) {
        self.s("\"");
        for p in ps {
            match p {
                Piece::Words(t) => {
                    let q = format!("{t:?}");
                    self.s(&q[1..q.len() - 1]);
                }
                Piece::Hole(e) => {
                    self.s("{{ ");
                    self.expr(e);
                    self.s(" }}");
                }
            }
        }
        self.s("\"");
    }
    fn lit(&mut self, l: &Lit) {
        match l {
            Lit::Int(n) => self.s(&n.to_string()),
            Lit::Bool(b) => self.s(&b.to_string()),
            Lit::Unit => self.s("()"),
        }
    }
    fn pat(&mut self, p: &Pat) {
        match p {
            Pat::Any => self.s("_"),
            Pat::Bind(n) => self.s(n),
            Pat::Lit(l) => self.lit(l),
            Pat::Text(t) => self.text(t),
            Pat::Ctor(c, ps) => {
                self.s(&c.to_string());
                if !ps.is_empty() {
                    self.list("(", ps, ")", Self::pat);
                }
            }
            Pat::Tuple(ps) => self.list("(", ps, ")", Self::pat),
            Pat::List(ps, rest) => {
                self.list("[", ps, "", Self::pat);
                if let Some(r) = rest {
                    self.s(if ps.is_empty() { ".." } else { ", .." });
                    self.s(r.as_deref().unwrap_or("_"));
                }
                self.s("]");
            }
        }
    }
    /// A body where the grammar wants a block: a block as it is, anything else in braces.
    fn block(&mut self, e: &Expr) {
        if matches!(e.e, E::Bind(..) | E::Seq(..)) {
            self.expr(e);
        } else {
            self.s("{ ");
            self.expr(e);
            self.s(" }");
        }
    }
    fn one(&mut self, word: &str, x: &Expr) {
        self.s(word);
        self.s("(");
        self.expr(x);
        self.s(")");
    }
    fn up_to(&mut self, n: &Option<i64>) {
        if let Some(n) = n {
            self.s(&format!(" up to {n}"));
        }
    }
    fn expr(&mut self, e: &Expr) {
        match &e.e {
            E::Lit(l) => self.lit(l),
            E::Text(ps) => self.pieces(ps),
            E::Name(p) => self.s(&p.to_string()),
            E::Var(x) => self.s(x),
            E::Apply(p, xs) | E::Perform(p, xs) => {
                if matches!(e.e, E::Perform(..)) {
                    self.s("perform ");
                }
                self.s(&p.to_string());
                self.list("(", xs, ")", Self::expr);
            }
            E::Field(x, f) => {
                self.expr(x);
                self.s(".");
                self.s(f);
            }
            E::Bin(op, a, b) => {
                self.s("(");
                self.expr(a);
                self.s(match op {
                    Op::And => " && ",
                    Op::Or => " || ",
                    Op::Eq => " == ",
                    Op::Ne => " != ",
                    Op::Lt => " < ",
                    Op::Le => " <= ",
                    Op::Gt => " > ",
                    Op::Ge => " >= ",
                    Op::Add => " + ",
                    Op::Sub => " - ",
                    Op::Mul => " * ",
                    Op::Div => " / ",
                    Op::Rem => " % ",
                });
                self.expr(b);
                self.s(")");
            }
            E::Neg(x) => {
                self.s("-");
                self.expr(x);
            }
            E::Not(x) => self.one("Not", x),
            E::Holds(x) => self.one("Holds", x),
            E::Violated(x) => self.one("Violated", x),
            E::Enabled(x) => self.one("Enabled", x),
            E::Quant(q, vs, x) => {
                self.s(match q {
                    Quant::Exists => "(Exists ",
                    Quant::Forall => "(Forall ",
                });
                self.s(&vs.join(", "));
                self.s(" : ");
                self.expr(x);
                self.s(")");
            }
            E::Foreach(vs, v, c) => {
                self.s("(Foreach ");
                self.s(&vs.join(", "));
                self.s(" : ");
                self.expr(v);
                if let Some(c) = c {
                    self.s(" When ");
                    self.expr(c);
                }
                self.s(")");
            }
            E::Agg(a, x) => self.one(
                match a {
                    Agg::Count => "Count",
                    Agg::Sum => "Sum",
                    Agg::Max => "Max",
                    Agg::Min => "Min",
                },
                x,
            ),
            E::Tuple(xs) => self.list("(", xs, ")", Self::expr),
            E::List(xs) => self.list("[", xs, "]", Self::expr),
            E::SelfParty => self.s("self"),
            E::This => self.s("this"),
            E::Bind(..) | E::Seq(..) => {
                self.s("{ ");
                let mut at = e;
                loop {
                    match &at.e {
                        E::Bind(x, p, rest) => {
                            self.s(x);
                            self.s(" <- ");
                            self.expr(p);
                            self.s("; ");
                            at = rest;
                        }
                        E::Seq(p, rest) => {
                            self.expr(p);
                            self.s("; ");
                            at = rest;
                        }
                        _ => {
                            self.expr(at);
                            break;
                        }
                    }
                }
                self.s(" }");
            }
            E::Return(x) => {
                self.s("return ");
                self.expr(x);
            }
            E::Handle(body, ret, arms) => {
                self.s("handle ");
                self.block(body);
                self.s(" with {");
                if let Some((x, r)) = ret {
                    self.s(" return ");
                    self.s(x);
                    self.s(" => ");
                    self.expr(r);
                }
                for a in arms {
                    self.s(" | ");
                    self.s(&a.effect.to_string());
                    self.list("(", &a.pats, ")", Self::pat);
                    self.s(", ");
                    self.s(&a.k);
                    self.cond(a.cond.as_ref());
                    self.s(" => ");
                    self.expr(&a.body);
                }
                self.s(" }");
            }
            E::Resume(k, x) => {
                self.s("resume ");
                self.one(k, x);
            }
            E::Match(on, cases) => {
                self.s("match ");
                self.expr(on);
                self.s(" {");
                for c in cases {
                    self.s(" | ");
                    self.pat(&c.pat);
                    self.cond(c.cond.as_ref());
                    self.s(" => ");
                    self.expr(&c.body);
                }
                self.s(" }");
            }
            E::If(c, a, b) => {
                self.s("if ");
                self.expr(c);
                self.s(" ");
                self.block(a);
                if let Some(b) = b {
                    self.s(" else ");
                    self.block(b);
                }
            }
            E::Par(xs) => self.list("par(", xs, ")", Self::expr),
            E::ParEach(x, l, n, body) => {
                self.s(&format!("par {x} in "));
                self.expr(l);
                self.up_to(n);
                self.s(" ");
                self.block(body);
            }
            E::Iterate(x, init, n, body) => {
                self.s(&format!("iterate {x} = "));
                self.expr(init);
                self.up_to(n);
                self.s(" ");
                self.block(body);
            }
            E::Do(x) => {
                self.s("do ");
                self.expr(x);
            }
            E::Gap => self.s("..."),
        }
    }
}
