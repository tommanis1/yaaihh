//! The recogniser: every end position of each grammar symbol from each start, and the reading built from those sets.

use super::*;

pub(super) const BIN_OPS: [(BinOp, bool); 14] = [
    (BinOp::When, false),
    (BinOp::Or, false),
    (BinOp::And, false),
    (BinOp::Eq, false),
    (BinOp::Neq, false),
    (BinOp::Leq, false),
    (BinOp::Geq, false),
    (BinOp::Le, false),
    (BinOp::Ge, false),
    (BinOp::Sub, true),
    (BinOp::Add, true),
    (BinOp::Mult, true),
    (BinOp::Mod, true),
    (BinOp::Div, true),
];

pub(super) fn bin_op(t: &Tok) -> Option<BinOp> {
    Some(match t {
        Tok::Kw("When") | Tok::Kw("Where") => BinOp::When,
        Tok::Kw("||") => BinOp::Or,
        Tok::Kw("&&") => BinOp::And,
        Tok::Kw("==") => BinOp::Eq,
        Tok::Kw("!=") => BinOp::Neq,
        Tok::Kw("<=") => BinOp::Leq,
        Tok::Kw(">=") => BinOp::Geq,
        Tok::Ch('<') => BinOp::Le,
        Tok::Ch('>') => BinOp::Ge,
        Tok::Ch('-') => BinOp::Sub,
        Tok::Ch('+') => BinOp::Add,
        Tok::Ch('*') => BinOp::Mult,
        Tok::Ch('%') => BinOp::Mod,
        Tok::Ch('/') => BinOp::Div,
        _ => return None,
    })
}

pub(super) fn prefix_op(t: &Tok) -> Option<UnOp> {
    Some(match t {
        Tok::Ch('!') | Tok::Kw("Not") => UnOp::Not,
        Tok::Kw("Present") | Tok::Kw("Holds") => UnOp::Present,
        Tok::Kw("Violated") => UnOp::Violated,
        Tok::Kw("Enabled") => UnOp::Enabled,
        _ => return None,
    })
}

pub(super) fn quant_kw(t: &Tok) -> Option<Quant> {
    Some(match t {
        Tok::Kw("Sum") => Quant::Sum,
        Tok::Kw("Count") => Quant::Count,
        Tok::Kw("Max") => Quant::Max,
        Tok::Kw("Min") => Quant::Min,
        _ => return None,
    })
}

impl<'a> Parser<'a> {
    pub(super) fn id(&self, name: &str) -> usize {
        self.g.names[name]
    }

    pub(super) fn miss(&mut self, l: usize, what: impl FnOnce() -> String) {
        if l > self.miss_at {
            self.miss_at = l;
            self.missed.clear();
        }
        if l == self.miss_at {
            self.missed.insert(what());
        }
    }

    pub(super) fn tok_matches(&mut self, s: &Sym, l: usize) -> bool {
        let Some(t) = self.toks.get(l) else {
            self.miss(l, || describe_sym(s));
            return false;
        };
        let ok = match (s, t) {
            (Sym::Ch(c), Tok::Ch(d)) => c == d,
            (Sym::Kw(k), Tok::Kw(d)) => k == d,
            (Sym::Id, Tok::Id(_)) | (Sym::AltId, Tok::Alt(_)) | (Sym::Int, Tok::Int(_)) => true,
            (Sym::Str, Tok::Str(_)) | (Sym::Chr, Tok::Chr(_)) => true,
            _ => false,
        };
        if !ok {
            self.miss(l, || describe_sym(s));
        }
        ok
    }

    pub(super) fn ends_sym(&mut self, s: &Sym, l: usize) -> Ends {
        match s {
            Sym::Expr => self.expr_ends(l),
            Sym::Nt(n) => self.ends_nt(*n, l),
            _ => Rc::new(if self.tok_matches(s, l) { vec![l + 1] } else { vec![] }),
        }
    }

    pub(super) fn ends_nt(&mut self, n: usize, l: usize) -> Ends {
        if let Some(e) = self.nt_memo.get(&(n, l)) {
            return e.clone();
        }
        let g = self.g;
        let mut all = Vec::new();
        for (a, alt) in g.rules[n].alts.iter().enumerate() {
            all.extend(self.ends_prefix(n, a, alt.syms.len(), l).iter());
        }
        let e = norm(all);
        self.nt_memo.insert((n, l), e.clone());
        e
    }

    /// The positions reached after the first `j` symbols of an alternative, starting at `l`.
    pub(super) fn ends_prefix(&mut self, n: usize, a: usize, j: usize, l: usize) -> Ends {
        if j == 0 {
            return Rc::new(vec![l]);
        }
        if let Some(e) = self.pre_memo.get(&(n, a, j, l)) {
            return e.clone();
        }
        let g = self.g;
        let before = self.ends_prefix(n, a, j - 1, l);
        let sym = &g.rules[n].alts[a].syms[j - 1];
        let mut all = Vec::new();
        for k in before.iter() {
            all.extend(self.ends_sym(sym, *k).iter());
        }
        let e = norm(all);
        self.pre_memo.insert((n, a, j, l), e.clone());
        e
    }

    pub(super) fn expr_ends(&mut self, l: usize) -> Ends {
        if let Some(e) = self.expr_memo.get(&l) {
            return e.clone();
        }
        let mut set: Vec<usize> = Vec::new();
        if let Some(t) = self.toks.get(l).cloned() {
            match &t {
                Tok::Kw("True") | Tok::Kw("False") | Tok::Kw("Current Time") => set.push(l + 1),
                Tok::Int(_) | Tok::Str(_) | Tok::Alt(_) => set.push(l + 1),
                Tok::Id(_) => {
                    let v = self.id("var");
                    set.extend(self.ends_nt(v, l).iter());
                    let a = self.id("arguments");
                    set.extend(self.ends_nt(a, l + 1).iter());
                }
                Tok::Ch('(') => {
                    for n in ["quantified", "parens"] {
                        let n = self.id(n);
                        set.extend(self.ends_nt(n, l).iter());
                    }
                }
                _ => {}
            }
            if prefix_op(&t).is_some() {
                set.extend(self.expr_ends(l + 1).iter());
            }
            if quant_kw(&t).is_some() {
                let f = self.id("foreach");
                set.extend(self.ends_nt(f, l + 1).iter());
            }
        }
        if set.is_empty() {
            self.miss(l, || "an expression".to_string());
        }
        // extend to the right: binary operators, projection, tagging
        let mut seen: std::collections::BTreeSet<usize> = set.iter().copied().collect();
        let mut work: Vec<usize> = seen.iter().copied().collect();
        while let Some(k) = work.pop() {
            let mut more: Vec<usize> = Vec::new();
            match self.toks.get(k).cloned() {
                Some(t) if bin_op(&t).is_some() => more.extend(self.expr_ends(k + 1).iter()),
                Some(Tok::Ch('.')) => {
                    let p = self.id("pvar");
                    more.extend(self.ends_nt(p, k + 1).iter())
                }
                Some(Tok::Ch(':')) => {
                    if let Some(Tok::Id(_)) = self.toks.get(k + 1) {
                        more.push(k + 2)
                    }
                }
                _ => self.miss(k, || "an operator".to_string()),
            }
            for m in more {
                if seen.insert(m) {
                    work.push(m);
                }
            }
        }
        let e: Ends = Rc::new(seen.into_iter().collect());
        self.expr_memo.insert(l, e.clone());
        e
    }

    // ---- building the reading ---------------------------------------------------------------

    pub(super) fn build_sym(&mut self, s: &Sym, l: usize, r: usize) -> Tree {
        match s {
            Sym::Expr => Tree::Expr(self.build_expr(l, r)),
            Sym::Nt(n) => self.build_nt(*n, l, r),
            _ => Tree::Tok(l),
        }
    }

    pub(super) fn build_nt(&mut self, n: usize, l: usize, r: usize) -> Tree {
        let g = self.g;
        for (a, alt) in g.rules[n].alts.iter().enumerate() {
            let len = alt.syms.len();
            if !has(&self.ends_prefix(n, a, len, l), r) {
                continue;
            }
            let mut kids = Vec::with_capacity(len);
            let mut right = r;
            for j in (0..len).rev() {
                let starts = self.ends_prefix(n, a, j, l);
                let fits = |p: &mut Self, k: usize| has(&p.ends_sym(&alt.syms[j], k), right);
                let k = if alt.longest_left[j] {
                    starts.iter().rev().copied().find(|k| fits(self, *k))
                } else {
                    starts.iter().copied().find(|k| fits(self, *k))
                }
                .expect("a recognised span has a split");
                kids.push(self.build_sym(&alt.syms[j], k, right));
                right = k;
            }
            kids.reverse();
            return Tree::Node(n, a, kids);
        }
        unreachable!("build_nt called on a span that was not recognised")
    }

    pub(super) fn build_expr(&mut self, l: usize, r: usize) -> Term {
        let toks = self.toks;
        let first = &toks[l];
        if r == l + 1 {
            match first {
                Tok::Kw("True") => return Term::BoolLit(true),
                Tok::Kw("False") => return Term::BoolLit(false),
                _ => {}
            }
        }
        let left = self.expr_ends(l);
        for (op, last) in BIN_OPS {
            let fits = |p: &mut Self, k: usize| {
                bin_op(&toks[k]) == Some(op) && has(&left, k) && has(&p.expr_ends(k + 1), r)
            };
            let k = if last {
                (l + 1..r.saturating_sub(1)).rev().find(|k| fits(self, *k))
            } else {
                (l + 1..r.saturating_sub(1)).find(|k| fits(self, *k))
            };
            if let Some(k) = k {
                return Term::Bin(op, Box::new(self.build_expr(l, k)), Box::new(self.build_expr(k + 1, r)));
            }
        }
        if prefix_op(first) == Some(UnOp::Not) && has(&self.expr_ends(l + 1), r) {
            return Term::Un(UnOp::Not, Box::new(self.build_expr(l + 1, r)));
        }
        if let Some(q) = quant_kw(first) {
            let f = self.id("foreach");
            if has(&self.ends_nt(f, l + 1), r) {
                let t = self.build_nt(f, l + 1, r);
                let (vs, body) = self.foreach(t);
                return Term::Q(q, vs, Box::new(body));
            }
        }
        if r == l + 1 {
            match first {
                Tok::Int(i) => return Term::IntLit(*i),
                Tok::Str(s) | Tok::Alt(s) => return Term::StringLit(s.clone()),
                _ => {}
            }
        }
        if let Tok::Id(name) = first {
            let v = self.id("var");
            if has(&self.ends_nt(v, l), r) {
                let t = self.build_nt(v, l, r);
                return Term::Ref(self.var(&t));
            }
            let a = self.id("arguments");
            if has(&self.ends_nt(a, l + 1), r) {
                let t = self.build_nt(a, l + 1, r);
                return Term::App(name.clone(), self.arguments(t));
            }
        }
        let pvar = self.id("pvar");
        for k in l + 2..r {
            if toks[k - 1] == Tok::Ch('.') && has(&left, k - 1) && has(&self.ends_nt(pvar, k), r) {
                let t = self.build_nt(pvar, k, r);
                let Tree::Node(_, alt, kids) = &t else { unreachable!() };
                let v = self.var(&kids[if *alt == 0 { 0 } else { 1 }]);
                return Term::Project(Box::new(self.build_expr(l, k - 1)), v);
            }
        }
        if r >= l + 3 && toks[r - 2] == Tok::Ch(':') && has(&left, r - 2) {
            if let Tok::Id(d) = &toks[r - 1] {
                return Term::Tag(Box::new(self.build_expr(l, r - 2)), d.clone());
            }
        }
        if *first == Tok::Ch('(') {
            let q = self.id("quantified");
            if has(&self.ends_nt(q, l), r) {
                let Tree::Node(_, alt, mut kids) = self.build_nt(q, l, r) else { unreachable!() };
                let Tree::Expr(body) = kids.remove(4) else { unreachable!() };
                let vs = self.vars1(&kids[2]);
                return Term::Q(if alt == 0 { Quant::Exists } else { Quant::Forall }, vs, Box::new(body));
            }
        }
        if let Some(op) = prefix_op(first) {
            if op != UnOp::Not && has(&self.expr_ends(l + 1), r) {
                return Term::Un(op, Box::new(self.build_expr(l + 1, r)));
            }
        }
        if *first == Tok::Ch('(') {
            let p = self.id("parens");
            if has(&self.ends_nt(p, l), r) {
                return self.build_expr(l + 1, r - 1);
            }
        }
        if *first == Tok::Kw("Current Time") && r == l + 1 {
            return Term::CurrentTime;
        }
        unreachable!("build_expr called on a span that was not recognised")
    }

    // ---- from trees to syntax -----------------------------------------------------------------
}
