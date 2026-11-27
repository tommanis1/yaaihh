//! A reading's clauses, declarations, phrases and files turned into the syntax tree.

use super::*;

impl<'a> Parser<'a> {
    pub(super) fn clause(&self, t: &Tree) -> Clause {
        // fact-clause / event-clause / duty-clause wrap one specific clause
        let inner = &self.kids(t).1[0];
        let (a, k) = self.kids(inner);
        match self.name(inner) {
            "precond" => Clause::ConditionedBy(self.exprs(&k[1])),
            "derivation" => {
                if a == 0 {
                    Clause::Derivation(
                        self.optforeach_list(&k[1]).into_iter().map(|(vs, t)| Derivation::Dv(vs, t)).collect(),
                    )
                } else {
                    Clause::Derivation(self.exprs(&k[1]).into_iter().map(Derivation::HoldsWhen).collect())
                }
            }
            "open-close" => {
                if a == 0 {
                    Clause::Close
                } else {
                    Clause::Open
                }
            }
            "domain-ext" => {
                let (b, d) = self.kids(&k[1]);
                match b {
                    0 => Clause::Strings(self.atoms1(&d[0])),
                    1 => Clause::Ints(self.ints1(&d[0])),
                    2 => Clause::Ints((self.int(&d[0])..=self.int(&d[2])).collect()),
                    _ => Clause::Strings(self.char_range(&d[0], &d[2])),
                }
            }
            "creates" => self.effects(EffKind::Create, &k[1]),
            "terminates" => self.effects(EffKind::Terminate, &k[1]),
            "obfuscates" => self.effects(EffKind::Obfuscate, &k[1]),
            "syncs" => Clause::Sync(
                self.optforeach_list(&k[1]).into_iter().map(|(vars, term)| Sync { vars, term }).collect(),
            ),
            "violation" => Clause::Violation(self.exprs(&k[1])),
            "enforced" => Clause::Enforcing(self.ids1(&k[1])),
            "terminated-by" => Clause::TerminatedBy(self.ids1(&k[1])),
            "created-by" => Clause::CreatedBy(self.ids1(&k[1])),
            other => unreachable!("clause {}", other),
        }
    }

    pub(super) fn clauses(&self, t: &Tree) -> Vec<Clause> {
        let at = |c: &Tree, made: Clause| match self.first_line(c) {
            Some(l) => Clause::At(Rc::from(""), l, Box::new(made)),
            None => made,
        };
        self.members(t).into_iter().map(|c| at(c, self.clause(c))).collect()
    }

    /// The line of the first token of T.
    fn first_line(&self, t: &Tree) -> Option<usize> {
        match t {
            Tree::Tok(i) => self.lines.get(*i).copied(),
            Tree::Expr(_) => None,
            Tree::Node(_, _, kids) => kids.iter().find_map(|k| self.first_line(k)),
        }
    }

    pub(super) fn frame(&self, t: &Tree) -> Vec<Decl> {
        let inner = &self.kids(t).1[0];
        let (_, k) = self.kids(inner);
        let spec = |kind: Kind, domain: Domain, constraint: Term, restriction, closed| TypeSpec {
            kind,
            domain,
            domain_constraint: constraint,
            restriction,
            derivation: Vec::new(),
            closed,
            conditions: Vec::new(),
        };
        let fact = Kind::Fact { invariant: false, actor: false };
        let act = |physical| Kind::Act { effects: Vec::new(), syncs: Vec::new(), physical };
        let actor_default = Var::plain(&id("actor"));
        match self.name(inner) {
            "fact-decl" => {
                let restriction = match self.kids(&k[1]).0 {
                    0 => None,
                    1 => Some(Restriction::Var),
                    _ => Some(Restriction::Function),
                };
                let (ia, ik) = self.kids(&k[3]);
                let domain = if ia == 0 { self.type_expr(&ik[1]) } else { Domain::AnyString };
                let name = self.ident(&k[2]);
                vec![
                    Decl::Type(name.clone(), spec(fact, domain, self.domcon(&k[4]), restriction, self.closed(&k[0]))),
                    Decl::Ext(name, self.clauses(&k[5])),
                ]
            }
            "bool-decl" => {
                let name = self.ident(&k[2]);
                vec![
                    Decl::Type(
                        name.clone(),
                        spec(fact, Domain::Products(Vec::new()), Term::BoolLit(true), None, self.closed(&k[0])),
                    ),
                    Decl::Ext(name, self.clauses(&k[3])),
                ]
            }
            n @ ("pred-decl" | "inv-decl") => {
                let name = self.ident(&k[1]);
                let kind = Kind::Fact { invariant: n == "inv-decl", actor: false };
                vec![
                    Decl::Type(name.clone(), spec(kind, Domain::Products(Vec::new()), Term::BoolLit(true), None, None)),
                    Decl::Ext(name, vec![Clause::Derivation(vec![Derivation::HoldsWhen(self.expr(&k[3]))])]),
                ]
            }
            "duty-decl" => {
                let name = self.ident(&k[2]);
                let mut dom = vec![self.var(&k[5]), self.var(&k[7])];
                dom.extend(self.objects(&k[8]));
                let kind = Kind::Duty {
                    enforcing: Vec::new(),
                    terminating: Vec::new(),
                    creating: Vec::new(),
                    violated_when: Vec::new(),
                };
                vec![
                    Decl::Type(
                        name.clone(),
                        spec(kind, Domain::Products(dom), self.domcon(&k[9]), None, self.closed(&k[0])),
                    ),
                    Decl::Ext(name, self.clauses(&k[10])),
                ]
            }
            "act-decl" => {
                let name = self.ident(&k[2]);
                let mut dom = vec![self.opt_var(&k[4]).unwrap_or(actor_default)];
                dom.extend(self.opt_var(&k[5]));
                dom.extend(self.objects(&k[6]));
                vec![
                    Decl::Type(
                        name.clone(),
                        spec(act(false), Domain::Products(dom), self.domcon(&k[7]), None, self.closed(&k[0])),
                    ),
                    Decl::Ext(name, self.clauses(&k[8])),
                ]
            }
            "physical-decl" => {
                let name = self.ident(&k[1]);
                let mut dom = vec![self.opt_var(&k[3]).unwrap_or(actor_default)];
                dom.extend(self.objects(&k[4]));
                let mut clauses = vec![Clause::Derivation(vec![Derivation::HoldsWhen(Term::BoolLit(true))])];
                clauses.extend(self.clauses(&k[6]));
                vec![
                    Decl::Type(name.clone(), spec(act(true), Domain::Products(dom), self.domcon(&k[5]), None, None)),
                    Decl::Ext(name, clauses),
                ]
            }
            "event-decl" => {
                let name = self.ident(&k[2]);
                let kind = Kind::Event { effects: Vec::new(), syncs: Vec::new() };
                vec![
                    Decl::Type(
                        name.clone(),
                        spec(kind, Domain::Products(self.objects(&k[4])), self.domcon(&k[5]), None, self.closed(&k[0])),
                    ),
                    Decl::Ext(name, self.clauses(&k[6])),
                ]
            }
            "ext" => {
                let (_, b) = self.kids(&k[1]);
                vec![Decl::Ext(self.ident(&b[1]), self.clauses(&b[2]))]
            }
            "placeholder" => vec![Decl::Placeholder(self.ident(&k[1]), self.ident(&k[3]))],
            other => unreachable!("frame {}", other),
        }
    }

    pub(super) fn frames(&self, t: &Tree) -> Vec<Decl> {
        self.members(t).into_iter().flat_map(|f| self.frame(f)).collect()
    }

    pub(super) fn phrase(&self, t: &Tree) -> Phrase {
        let (a, k) = self.kids(t);
        match a {
            0 | 1 | 2 | 3 | 6 | 7 => {
                let (vs, term) = self.optforeach(&k[1]);
                match a {
                    0 => Phrase::Trigger(vs, term),
                    1 => Phrase::Create(vs, term),
                    2 => Phrase::Terminate(vs, term),
                    3 => Phrase::Obfuscate(vs, term),
                    6 => Phrase::InstQuery(false, vs, term),
                    _ => Phrase::InstQuery(true, vs, term),
                }
            }
            4 => Phrase::Query(self.expr(&k[1])),
            5 => Phrase::Query(Term::Un(UnOp::Not, Box::new(self.expr(&k[1])))),
            _ => Phrase::Decls(self.frames(&k[0])),
        }
    }

    pub(super) fn phrase_or_comp(&self, t: &Tree) -> Phrase {
        let (a, k) = self.kids(t);
        if a == 1 {
            return self.phrase(&k[0]);
        }
        let (b, body) = self.kids(&k[1]);
        if b == 1 {
            return Phrase::Par(Vec::new());
        }
        let (_, pd) = self.kids(&body[0]);
        Phrase::Par(self.sep_list(&pd[0], &pd[1]).into_iter().map(|p| self.phrase(p)).collect())
    }

    pub(super) fn top(&self, t: &Tree) -> Vec<Statement> {
        let (_, k) = self.kids(t);
        let mut out = Vec::new();
        let (a, statements) = self.kids(&k[0]);
        if a == 0 {
            for statement in self.members(&statements[0]) {
                let (ia, ik) = self.kids(statement);
                if ia == 0 {
                    let (da, dk) = self.kids(&ik[0]);
                    let path = self.ident(&dk[1]).to_string();
                    out.push(if da == 0 { Statement::Include(path) } else { Statement::Require(path) });
                } else {
                    out.push(Statement::Phrase(self.phrase_or_comp(&ik[0])));
                }
            }
        }
        if let Some(p) = self.members(&k[1]).first() {
            out.push(Statement::Phrase(self.phrase(p)));
        }
        out
    }

    pub(super) fn flint(&self, t: &Tree) -> OldFile {
        let (_, k) = self.kids(t);
        let decls = self.frames(&k[1]);
        let part = |t| self.part(t);
        let refiner = part(&k[2])
            .into_iter()
            .map(|r| {
                let (_, rk) = self.kids(r);
                (self.ident(&rk[1]), self.type_expr(&rk[3]))
            })
            .collect();
        let initial = part(&k[3])
            .into_iter()
            .map(|i| {
                let (a, ik) = self.kids(i);
                let (vars, term) = if a == 0 {
                    let (_, f) = self.kids(&ik[0]);
                    (self.vars1(&f[2]), self.expr(&f[4]))
                } else {
                    (Vec::new(), self.expr(&ik[0]))
                };
                Effect { kind: EffKind::Create, vars, term }
            })
            .collect();
        let scenario = part(&k[4])
            .into_iter()
            .map(|s| {
                let (a, sk) = self.kids(s);
                match a {
                    1 => Phrase::Query(self.expr(&sk[1])),
                    2 => Phrase::Query(Term::Un(UnOp::Not, Box::new(self.expr(&sk[1])))),
                    _ => {
                        let (aa, ak) = self.kids(&sk[1]);
                        let app = |name: &Tree, args: &Tree| {
                            let (n, alt, kids) = match args {
                                Tree::Node(n, alt, kids) => (*n, *alt, kids),
                                _ => unreachable!(),
                            };
                            // rebuild an owned tree for `arguments`
                            let owned = Tree::Node(n, alt, kids.iter().map(|t| self.clone_tree(t)).collect());
                            Term::App(self.ident(name), self.arguments(owned))
                        };
                        let (vs, term) =
                            if aa == 0 { (Vec::new(), app(&ak[0], &ak[1])) } else { (self.vars1(&ak[2]), app(&ak[4], &ak[5])) };
                        match self.kids(&sk[0]).0 {
                            0 => Phrase::Trigger(vs, term),
                            1 => Phrase::Create(vs, term),
                            _ => Phrase::Terminate(vs, term),
                        }
                    }
                }
            })
            .collect();
        OldFile { decls, refiner, initial, scenario }
    }

    /// The elements of an optional part of the four-part format.
    pub(super) fn part<'t>(&self, t: &'t Tree) -> Vec<&'t Tree> {
        let (a, p) = self.kids(t);
        if a == 0 {
            self.members(&p[1])
        } else {
            Vec::new()
        }
    }

    pub(super) fn clone_tree(&self, t: &Tree) -> Tree {
        match t {
            Tree::Tok(i) => Tree::Tok(*i),
            Tree::Expr(e) => Tree::Expr(e.clone()),
            Tree::Node(n, a, k) => Tree::Node(*n, *a, k.iter().map(|t| self.clone_tree(t)).collect()),
        }
    }
}
