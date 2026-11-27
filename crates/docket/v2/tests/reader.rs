//! The reader on each written form: the tree it gives, and the texts it refuses (spec 2, 5.1).

use docket_v2::syntax::tree::*;
use docket_v2::syntax::{read, read_block};

fn decls(t: &str) -> Vec<DeclKind> {
    read(t).unwrap_or_else(|e| panic!("{e}\n{t}")).decls.into_iter().map(|d| d.what).collect()
}

fn block(t: &str) -> E {
    read_block(t).unwrap_or_else(|e| panic!("{e}\n{t}")).e
}

fn refused(t: &str, why: &str) {
    let e = read(t).err().unwrap_or_else(|| panic!("read, and should not be:\n{t}"));
    assert!(e.why.contains(why), "{e}\n{t}");
}

fn name(e: &Expr) -> String {
    match &e.e {
        E::Name(p) => p.to_string(),
        other => panic!("a name, not {other:?}"),
    }
}

#[test]
fn a_module_line_and_imports_with_and_without_an_alias() {
    let m = read("module a.b\nimport c.d as D\nimport e\nFact F Identified by String").unwrap();
    assert_eq!(m.name.unwrap().to_string(), "a.b");
    assert_eq!(m.imports.iter().map(|i| (i.path.to_string(), i.alias.clone())).collect::<Vec<_>>(), [("c.d".into(), Some("D".into())), ("e".into(), None)]);
}

#[test]
fn the_four_normative_kinds_their_fields_and_their_clauses() {
    let d = decls(
        "Fact W Identified by String\nFact P(w: W, n: Int) Holds when Q(w) && n > 1\nAct Go(by: W, x: W) Conditioned by P(x, 2) Creates P(x, 3), R(x) When S(x) Terminates P(x, 2) Does { return 1 }\nEvent Tick() Syncs with Go(\"a\", \"b\")\nDuty Pay(by: W, to: W) Violated when Late(by)\nExtend Act Go Conditioned by T(x)",
    );
    assert!(matches!(&d[0], DeclKind::Norm { kind: Norm::Fact, name, fields: Fields::By(Type::Named(t, _)), clauses } if name == "W" && t.to_string() == "String" && clauses.is_empty()));
    let DeclKind::Norm { kind: Norm::Fact, fields: Fields::Named(fs), clauses, .. } = &d[1] else { panic!("{:?}", d[1]) };
    assert_eq!(fs.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["w", "n"]);
    assert!(matches!(&clauses[..], [Clause::HoldsWhen(Expr { e: E::Bin(Op::And, _, _), .. })]));
    let DeclKind::Norm { kind: Norm::Act, clauses, .. } = &d[2] else { panic!() };
    assert!(matches!(&clauses[..], [Clause::ConditionedBy(_), Clause::Creates(c), Clause::Terminates(t), Clause::Does(_)] if c.len() == 2 && c[0].when.is_none() && c[1].when.is_some() && t.len() == 1), "{clauses:?}");
    assert!(matches!(&d[3], DeclKind::Norm { kind: Norm::Event, clauses, .. } if matches!(&clauses[..], [Clause::SyncsWith(_)])));
    assert!(matches!(&d[4], DeclKind::Norm { kind: Norm::Duty, clauses, .. } if matches!(&clauses[..], [Clause::ViolatedWhen(_)])));
    assert!(matches!(&d[5], DeclKind::Extend { kind: Norm::Act, name, clauses } if name.to_string() == "Go" && clauses.len() == 1));
}

#[test]
fn a_plus_line_is_a_fact_that_holds_and_it_ends_with_a_dot() {
    let d = decls("+P(\"a\", 2).\n+m.Q(1).");
    assert!(matches!(&d[0], DeclKind::Holds(Expr { e: E::Apply(p, xs), .. }) if p.to_string() == "P" && xs.len() == 2));
    assert!(matches!(&d[1], DeclKind::Holds(Expr { e: E::Apply(p, _), .. }) if p.to_string() == "m.Q"));
    refused("+P(\"a\", 2)\nFact W Identified by String", "`.`");
}

#[test]
fn effects_calls_types_and_fns() {
    let d = decls(
        "Effect Ask<T>(q: T) -> Option<T> Does { return None }\nCall Judge(x: String) -> Bool Template \"Is {{ x }} right?\"\ntype V = Go Needs \"Go\" | Stop(why: String)\ntype R { a: Int, b: List<String> }\nfn f(p: Program<Int ! {Ask, ..e}>) -> Program<Int> { do p }",
    );
    assert!(matches!(&d[0], DeclKind::Effect { name, params, fields, result: Type::Named(o, a), does: Some(_) } if name == "Ask" && params == &["T"] && fields.len() == 1 && o.to_string() == "Option" && a.len() == 1));
    let DeclKind::Call { template, .. } = &d[1] else { panic!() };
    assert!(matches!(&template[..], [Piece::Words(a), Piece::Hole(_), Piece::Words(b)] if a == "Is " && b == " right?"), "{template:?}");
    let DeclKind::Sum { variants, .. } = &d[2] else { panic!() };
    assert_eq!(variants.iter().map(|v| (v.name.as_str(), v.fields.len(), v.needs.as_deref())).collect::<Vec<_>>(), [("Go", 0, Some("Go")), ("Stop", 1, None)]);
    assert!(matches!(&d[3], DeclKind::Record { fields, .. } if fields.len() == 2));
    let DeclKind::Fn { fields, result, .. } = &d[4] else { panic!() };
    assert!(matches!(&fields[0].ty, Type::Program(_, Row { effects, rest: Some(e), implied: false }) if effects.len() == 1 && e == "e"));
    assert!(matches!(result, Type::Program(_, Row { implied: true, .. })));
}

#[test]
fn a_reaction_takes_its_patterns_in_parentheses_and_a_where() {
    let d = decls("on Go(w, _) where P(w) do { return () }");
    assert!(matches!(&d[0], DeclKind::On { target, pats, cond: Some(_), .. } if target.to_string() == "Go" && matches!(&pats[..], [Pat::Bind(w), Pat::Any] if w == "w")));
    refused("on Go do { return () }", "`(`");
}

#[test]
fn a_block_binds_the_rest_and_a_bind_cannot_end_it() {
    let E::Bind(x, p, rest) = block("{ x <- perform Ask(1); y <- f(x); return y }") else { panic!() };
    assert_eq!(x, "x");
    assert!(matches!(p.e, E::Perform(ref q, ref a) if q.to_string() == "Ask" && a.len() == 1));
    assert!(matches!(rest.e, E::Bind(ref y, _, ref r) if y == "y" && matches!(r.e, E::Return(_))));
    assert!(matches!(block("{ Go(); return 1 }"), E::Seq(..)));
    let e = read_block("{ x <- perform Ask(1) }").unwrap_err();
    assert!(e.why.contains("last statement"), "{e}");
    assert!(read_block("{ }").is_err(), "an empty block");
}

#[test]
fn handle_with_its_return_clause_and_arms_and_resume() {
    let E::Handle(_, ret, arms) = block("{ handle { perform A() } with { return v => return v | A(), k => resume k(1) | B(n), k where n > 1 => return 0 } }") else { panic!() };
    assert!(matches!(ret, Some((v, _)) if v == "v"));
    assert_eq!(arms.len(), 2);
    assert!(matches!(&arms[0].body.e, E::Resume(k, x) if k == "k" && matches!(x.e, E::Lit(Lit::Int(1)))));
    assert!(arms[1].cond.is_some() && arms[1].pats.len() == 1);
    assert!(read_block("{ handle { perform A() } with { | A(), k resume k(1) } }").unwrap_err().why.contains("`=>`"));
}

#[test]
fn match_with_patterns_and_no_empty_match() {
    let E::Match(_, cases) = block("{ match x { | Some(y) where y > 1 => return y | [a, ..rest] => return a | (1, \"t\") => return 0 | _ => return 1 } }") else { panic!() };
    assert!(matches!(&cases[0].pat, Pat::Ctor(p, ps) if p.to_string() == "Some" && matches!(&ps[..], [Pat::Bind(_)])) && cases[0].cond.is_some());
    assert!(matches!(&cases[1].pat, Pat::List(ps, Some(Some(r))) if ps.len() == 1 && r == "rest"));
    assert!(matches!(&cases[2].pat, Pat::Tuple(ps) if matches!(&ps[..], [Pat::Lit(Lit::Int(1)), Pat::Text(t)] if t == "t")));
    assert!(matches!(cases[3].pat, Pat::Any));
    assert!(read_block("{ match x { } }").unwrap_err().why.contains("no arm"));
}

#[test]
fn if_takes_a_block_after_else_and_nothing_else() {
    assert!(matches!(block("{ if x { return 1 } else { return 2 } }"), E::If(_, _, Some(_))));
    assert!(matches!(block("{ if x { return 1 } }"), E::If(_, _, None)));
    assert!(read_block("{ if x { return 1 } else if y { return 2 } }").is_err());
}

#[test]
fn par_iterate_do_and_the_gap() {
    assert!(matches!(block("{ par(f(), g()) }"), E::Par(xs) if xs.len() == 2));
    assert!(matches!(block("{ par x in l up to 3 { f(x) } }"), E::ParEach(x, _, Some(3), _) if x == "x"));
    assert!(matches!(block("{ par x in l { f(x) } }"), E::ParEach(_, _, None, _)));
    assert!(matches!(block("{ iterate s = 0 up to 5 { return Done(s) } }"), E::Iterate(s, _, Some(5), _) if s == "s"));
    assert!(matches!(block("{ do p }"), E::Do(_)));
    assert!(matches!(block("{ x <- ...; return x }"), E::Bind(_, p, _) if matches!(p.e, E::Gap)));
}

#[test]
fn conditions_by_precedence_and_the_eflint_forms() {
    let E::Return(x) = block("{ return a + b * c == d || Not(e) && f }") else { panic!() };
    let E::Bin(Op::Or, l, r) = &x.e else { panic!("{x:?}") };
    assert!(matches!(&l.e, E::Bin(Op::Eq, s, _) if matches!(&s.e, E::Bin(Op::Add, _, m) if matches!(m.e, E::Bin(Op::Mul, ..)))));
    assert!(matches!(&r.e, E::Bin(Op::And, n, _) if matches!(n.e, E::Not(_))));
    let E::Return(x) = block("{ return Holds(P(a)) && Violated(D(a)) && Enabled(Go(a)) }") else { panic!() };
    assert!(matches!(&x.e, E::Bin(Op::And, _, _)));
    assert!(matches!(block("{ return Exists g, t : P(g, t) }"), E::Return(x) if matches!(&x.e, E::Quant(Quant::Exists, vs, _) if vs.len() == 2)));
    assert!(matches!(block("{ return Forall g : P(g) }"), E::Return(x) if matches!(&x.e, E::Quant(Quant::Forall, ..))));
    let E::Return(x) = block("{ return Count(Foreach x : x When P(x)) }") else { panic!() };
    assert!(matches!(&x.e, E::Agg(Agg::Count, f) if matches!(&f.e, E::Foreach(vs, _, Some(_)) if vs == &["x"])));
    assert!(matches!(block("{ return (self, this) }"), E::Return(x) if matches!(&x.e, E::Tuple(t) if matches!(t[..], [Expr { e: E::SelfParty, .. }, Expr { e: E::This, .. }]))));
}

#[test]
fn names_paths_applications_and_texts_with_holes() {
    let E::Return(x) = block("{ return m.F(a.b, \"t {{ n }} u\", [1, -2], ()) }") else { panic!() };
    let E::Apply(p, xs) = &x.e else { panic!() };
    assert_eq!(p.to_string(), "m.F");
    assert_eq!(name(&xs[0]), "a.b");
    assert!(matches!(&xs[1].e, E::Text(ps) if ps.len() == 3));
    assert!(matches!(&xs[2].e, E::List(l) if matches!(l[1].e, E::Neg(_))));
    assert!(matches!(xs[3].e, E::Lit(Lit::Unit)));
}

#[test]
fn a_call_takes_no_type_parameters() {
    refused("Call J<T>(x: T) -> Bool Template \"{{ x }}\"", "");
}
