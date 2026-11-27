//! A reading's leaves and small parts turned into names, numbers, variables, arguments, types and effects.

use super::*;

impl<'a> Parser<'a> {
    pub(super) fn name(&self, t: &Tree) -> &str {
        match t {
            Tree::Node(n, _, _) => &self.g.rules[*n].name,
            _ => "",
        }
    }

    pub(super) fn ident(&self, t: &Tree) -> Id {
        match t {
            Tree::Tok(i) => match &self.toks[*i] {
                Tok::Id(s) | Tok::Alt(s) | Tok::Str(s) => s.clone(),
                other => panic!("identifier expected, found {:?}", other),
            },
            // atom
            Tree::Node(_, _, kids) => self.ident(&kids[0]),
            _ => unreachable!(),
        }
    }

    pub(super) fn int(&self, t: &Tree) -> i64 {
        match t {
            Tree::Tok(i) => match &self.toks[*i] {
                Tok::Int(v) => *v,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }

    pub(super) fn chr(&self, t: &Tree) -> char {
        match t {
            Tree::Tok(i) => match &self.toks[*i] {
                Tok::Chr(c) => *c,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }

    /// The elements of a `x*` / `x+` list, or of `x?`.
    pub(super) fn members<'t>(&self, t: &'t Tree) -> Vec<&'t Tree> {
        let mut out = Vec::new();
        let mut cur = t;
        loop {
            let Tree::Node(_, _, kids) = cur else { unreachable!() };
            match kids.len() {
                0 => return out,
                1 => {
                    out.push(&kids[0]);
                    return out;
                }
                _ => {
                    out.push(&kids[0]);
                    cur = &kids[1];
                }
            }
        }
    }

    pub(super) fn kids<'t>(&self, t: &'t Tree) -> (usize, &'t [Tree]) {
        match t {
            Tree::Node(_, a, kids) => (*a, kids),
            _ => unreachable!(),
        }
    }

    pub(super) fn expr(&self, t: &Tree) -> Term {
        match t {
            Tree::Expr(e) => e.clone(),
            _ => unreachable!(),
        }
    }

    pub(super) fn var(&self, t: &Tree) -> Var {
        let (_, k) = self.kids(t);
        let (_, d) = self.kids(&k[1]);
        let mut dec = String::new();
        if let Some(i) = self.members(&d[0]).first() {
            dec.push_str(&self.int(i).to_string());
        }
        for _ in self.members(&d[1]) {
            dec.push('\'');
        }
        Var { dom: self.ident(&k[0]), dec: id(&dec) }
    }

    /// `head tail*` where each tail element is `separator member`.
    pub(super) fn sep_list<'t>(&self, head: &'t Tree, tail: &'t Tree) -> Vec<&'t Tree> {
        let mut out = vec![head];
        for t in self.members(tail) {
            out.push(&self.kids(t).1[1]);
        }
        out
    }

    pub(super) fn vars1(&self, t: &Tree) -> Vec<Var> {
        let (_, k) = self.kids(t);
        self.sep_list(&k[0], &k[1]).into_iter().map(|v| self.var(v)).collect()
    }

    pub(super) fn exprs(&self, t: &Tree) -> Vec<Term> {
        let (_, k) = self.kids(t);
        if k.is_empty() {
            return Vec::new();
        }
        self.sep_list(&k[0], &k[1]).into_iter().map(|e| self.expr(e)).collect()
    }

    pub(super) fn foreach(&self, t: Tree) -> (Vec<Var>, Term) {
        let (_, k) = self.kids(&t);
        (self.vars1(&k[2]), self.expr(&k[4]))
    }

    pub(super) fn optforeach(&self, t: &Tree) -> (Vec<Var>, Term) {
        let (a, k) = self.kids(t);
        if a == 0 {
            (Vec::new(), self.expr(&k[0]))
        } else {
            let (_, f) = self.kids(&k[0]);
            (self.vars1(&f[2]), self.expr(&f[4]))
        }
    }

    pub(super) fn optforeach_list(&self, t: &Tree) -> Vec<(Vec<Var>, Term)> {
        let (_, k) = self.kids(t);
        self.sep_list(&k[0], &k[1]).into_iter().map(|e| self.optforeach(e)).collect()
    }

    pub(super) fn arguments(&self, t: Tree) -> Args {
        let (a, k) = self.kids(&t);
        if a == 0 {
            let (_, m) = self.kids(&k[1]);
            if m.is_empty() {
                return Args::Mods(Vec::new());
            }
            Args::Mods(
                self.sep_list(&m[0], &m[1])
                    .into_iter()
                    .map(|md| {
                        let (_, p) = self.kids(md);
                        (self.var(&p[0]), self.expr(&p[2]))
                    })
                    .collect(),
            )
        } else {
            Args::Pos(self.exprs(&k[1]))
        }
    }

    pub(super) fn atoms1(&self, t: &Tree) -> Vec<Id> {
        let (_, k) = self.kids(t);
        self.sep_list(&k[0], &k[1]).into_iter().map(|a| self.ident(a)).collect()
    }

    pub(super) fn ints1(&self, t: &Tree) -> Vec<i64> {
        let (_, k) = self.kids(t);
        self.sep_list(&k[0], &k[1]).into_iter().map(|a| self.int(a)).collect()
    }

    pub(super) fn char_range(&self, a: &Tree, b: &Tree) -> Vec<Id> {
        (self.chr(a)..=self.chr(b)).map(|c| id(&c.to_string())).collect()
    }

    pub(super) fn type_expr(&self, t: &Tree) -> Domain {
        let (a, k) = self.kids(t);
        let product = |first: &Tree, rest: &Tree| {
            let mut vs = vec![self.var(first)];
            let (_, r) = self.kids(rest);
            vs.extend(self.sep_list(&r[0], &r[1]).into_iter().map(|v| self.var(v)));
            Domain::Products(vs)
        };
        match a {
            0 => Domain::Products(vec![self.var(&k[0])]),
            1 => {
                let mut ss = vec![self.ident(&k[0])];
                ss.extend(self.atoms1(&k[2]));
                Domain::Strings(ss)
            }
            2 => {
                let mut is = vec![self.int(&k[0])];
                is.extend(self.ints1(&k[2]));
                Domain::Ints(is)
            }
            3 => product(&k[0], &k[2]),
            4 => product(&k[1], &k[3]),
            5 => Domain::Ints(vec![self.int(&k[0])]),
            6 => Domain::Ints((self.int(&k[0])..=self.int(&k[2])).collect()),
            7 => Domain::Strings(vec![self.ident(&k[0])]),
            8 => Domain::Strings(self.char_range(&k[0], &k[2])),
            9 | 10 => Domain::AnyString,
            11 => Domain::AnyInt,
            _ => Domain::Time,
        }
    }

    pub(super) fn closed(&self, t: &Tree) -> Option<bool> {
        // `None` is the closed default, `Some(false)` an explicitly open type
        if self.kids(t).0 == 1 {
            Some(false)
        } else {
            None
        }
    }

    pub(super) fn domcon(&self, t: &Tree) -> Term {
        let (a, k) = self.kids(t);
        if a == 0 {
            self.expr(&k[1])
        } else {
            Term::BoolLit(true)
        }
    }

    pub(super) fn objects(&self, t: &Tree) -> Vec<Var> {
        let (a, k) = self.kids(t);
        if a == 0 {
            self.vars1(&k[1])
        } else {
            Vec::new()
        }
    }

    pub(super) fn opt_var(&self, t: &Tree) -> Option<Var> {
        self.members(t).first().map(|a| self.var(&self.kids(a).1[1]))
    }

    pub(super) fn effects(&self, kind: EffKind, t: &Tree) -> Clause {
        Clause::Effects(
            self.optforeach_list(t).into_iter().map(|(vars, term)| Effect { kind, vars, term }).collect(),
        )
    }

    pub(super) fn ids1(&self, t: &Tree) -> Vec<Id> {
        let (_, k) = self.kids(t);
        self.sep_list(&k[0], &k[1]).into_iter().map(|a| self.ident(a)).collect()
    }
}
