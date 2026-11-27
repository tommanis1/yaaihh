//! Modules, imports and names (spec 5.2, 5.6; load check 2): every name made to stand for one declaration.

use super::resolve::Scope;
use crate::run::driver::Lets;
use crate::run::norms::NormDecl;
use crate::run::value::Value;
use crate::syntax::form;
use crate::syntax::lex::At;
use crate::syntax::tree::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// A refusal of a text at load, with where and why (14).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadError {
    pub module: String,
    pub at: At,
    pub why: String,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.module, self.at, self.why)
    }
}

/// A declaration as the loaded text knows it, by its qualified name (`gate.a.Submitted`).
#[derive(Clone, Debug, PartialEq)]
pub enum Sym {
    Norm { kind: Norm, fields: Fields, module: String },
    Effect { params: Vec<String>, fields: Vec<Field>, result: Type, does: Option<Expr>, module: String },
    Call { fields: Vec<Field>, result: Type, template: Vec<Piece> },
    /// A constructor: of the sum `ty`, its fields, the power it needs (12).
    Ctor { ty: String, fields: Vec<Field>, needs: Option<String> },
    Sum { params: Vec<String>, ctors: Vec<String> },
    Record { params: Vec<String>, fields: Vec<Field> },
    Fn { params: Vec<String>, fields: Vec<Field>, result: Type, body: Expr, module: String },
    /// A type, constructor, Effect or host function of the language itself (13, 6.3).
    Builtin,
}

/// A reaction (8.7), its names resolved: `on target(pats) where cond do body`, written in `module`.
#[derive(Clone, Debug, PartialEq)]
pub struct On {
    pub module: String,
    pub at: At,
    pub target: String,
    pub pats: Vec<Pat>,
    pub cond: Option<Expr>,
    pub body: Expr,
}

/// A text loaded whole: its modules in load order, every declaration by its qualified name, its normative part
/// for the evaluator, its reactions in load order and then the order written, and its `Lets` lines.
#[derive(Clone, Debug, Default)]
pub struct Loaded {
    pub modules: Vec<String>,
    pub syms: BTreeMap<String, Sym>,
    pub norms: Vec<NormDecl>,
    pub ons: Vec<On>,
    pub lets: Lets,
    /// The `Lets` fact (12, 13), and the type of its point field: the one fact the engine reads by name.
    pub lets_fact: Option<(String, String)>,
    /// Each module's imports, to resolve a text given later in its scope (a gap's filling).
    pub imports: BTreeMap<String, Vec<Import>>,
    /// What typing found: the rows of the Acts' bodies and of the reactions (7).
    pub rows: super::typing::Rows,
    /// The hash of the text as loaded, for the `loaded` line: each module's name and its declarations in their
    /// written form, resolved, in load order.
    pub hash: [u8; 32],
}

/// The language's own names (13, 6.3); a module's name of the same spelling stands before it.
pub const BUILTINS: [&str; 30] = [
    "Int", "String", "Bool", "Unit", "Never", "List", "Option", "Call", "Program", "Loop", "Some", "None", "Again", "Done", "Gap", "Charge", "Now", "Refused", "Unfit", "Exhausted", "Fault", "matches_glob", "diff_lines", "added_lines", "removed_lines", "str_len", "contains",
    "is_subsequence", "first", "to_int",
];

/// Load modules as one text: order them by imports and then by path (5.6), resolve every name, and refuse the text
/// on a name declared twice, a name that stands for nothing, or an import cycle (14, check 2).
pub fn load(texts: Vec<Module>) -> Result<Loaded, LoadError> {
    let err = |m: &str, at: At, why: String| LoadError { module: m.to_string(), at, why };
    let mut by: BTreeMap<String, Module> = BTreeMap::new();
    for m in texts {
        let name = m.name.as_ref().map(|p| p.to_string()).unwrap_or_default();
        if by.contains_key(&name) {
            return Err(err(&name, At::default(), format!("module {name} is given twice")));
        }
        by.insert(name, m);
    }
    // load order: imports first, then by path; a cycle is refused
    let mut order: Vec<String> = Vec::new();
    let mut on_path: Vec<String> = Vec::new();
    fn visit(n: &str, by: &BTreeMap<String, Module>, order: &mut Vec<String>, on_path: &mut Vec<String>) -> Result<(), LoadError> {
        if order.iter().any(|o| o == n) {
            return Ok(());
        }
        if on_path.iter().any(|o| o == n) {
            return Err(LoadError { module: n.to_string(), at: At::default(), why: format!("imports form a cycle: {} → {n}", on_path.join(" → ")) });
        }
        let m = &by[n];
        on_path.push(n.to_string());
        for i in &m.imports {
            let p = i.path.to_string();
            if !by.contains_key(&p) {
                return Err(LoadError { module: n.to_string(), at: i.at, why: format!("{p} is imported and not given") });
            }
            visit(&p, by, order, on_path)?;
        }
        on_path.pop();
        order.push(n.to_string());
        Ok(())
    }
    for n in by.keys() {
        visit(n, &by, &mut order, &mut on_path)?;
    }
    let mut out = Loaded { modules: order.clone(), imports: by.iter().map(|(n, m)| (n.clone(), m.imports.clone())).collect(), ..Loaded::default() };
    for b in BUILTINS {
        out.syms.insert(b.to_string(), Sym::Builtin);
    }
    // every module's declarations first, so a name may be used above where it is declared
    for mn in &order {
        for d in &by[mn].decls {
            let q = |n: &str| qualify(mn, n);
            let mut put = |name: String, s: Sym| -> Result<(), LoadError> {
                if out.syms.get(&name).is_some_and(|x| *x != Sym::Builtin) {
                    return Err(err(mn, d.at, format!("{name} is declared twice")));
                }
                out.syms.insert(name, s);
                Ok(())
            };
            match &d.what {
                DeclKind::Norm { kind, name, fields, .. } => put(q(name), Sym::Norm { kind: *kind, fields: fields.clone(), module: mn.clone() })?,
                DeclKind::Effect { name, params, fields, result, does } => put(q(name), Sym::Effect { params: params.clone(), fields: fields.clone(), result: result.clone(), does: does.clone(), module: mn.clone() })?,
                DeclKind::Call { name, fields, result, template } => put(q(name), Sym::Call { fields: fields.clone(), result: result.clone(), template: template.clone() })?,
                DeclKind::Sum { name, params, variants } => {
                    for v in variants {
                        put(q(&v.name), Sym::Ctor { ty: q(name), fields: v.fields.clone(), needs: v.needs.clone() })?;
                    }
                    put(q(name), Sym::Sum { params: params.clone(), ctors: variants.iter().map(|v| q(&v.name)).collect() })?;
                }
                DeclKind::Record { name, params, fields } => put(q(name), Sym::Record { params: params.clone(), fields: fields.clone() })?,
                DeclKind::Fn { name, params, fields, result, body } => put(q(name), Sym::Fn { params: params.clone(), fields: fields.clone(), result: result.clone(), body: body.clone(), module: mn.clone() })?,
                DeclKind::Extend { .. } | DeclKind::Holds(_) | DeclKind::On { .. } => {}
            }
        }
    }
    let named: Vec<String> = out.syms.iter().filter(|(n, s)| (*n == "Lets" || n.ends_with(".Lets")) && matches!(s, Sym::Norm { kind: Norm::Fact, .. })).map(|(n, _)| n.clone()).collect();
    if named.len() > 1 {
        return Err(err(&named[1], At::default(), format!("`Lets` is declared by one module of a text, and here by two: {}", named.join(", "))));
    }
    let lets_name = named.into_iter().next();
    // then every name in every declaration resolved, in load order; the table's bodies take the resolved form
    let mut fixed: Vec<(String, DeclKind)> = Vec::new();
    let mut hash = Sha256::new();
    for mn in &order {
        hash.update(format!("module {mn}\n"));
        let m = &by[mn];
        let sc = Scope { module: mn, imports: &m.imports, syms: &out.syms };
        for d in &m.decls {
            let mut d = d.clone();
            sc.decl(&mut d).map_err(|(at, why)| err(mn, at, why))?;
            hash.update(format!("{}\n", form::decl(&d.what)));
            match d.what {
                DeclKind::On { target, pats, cond, body } => out.ons.push(On { module: mn.clone(), at: d.at, target: target.to_string(), pats, cond, body }),
                DeclKind::Norm { .. } | DeclKind::Extend { .. } | DeclKind::Holds(_) => {
                    if let DeclKind::Holds(Expr { e: E::Apply(p, args), .. }) = &d.what
                        && Some(p.to_string()) == lets_name
                    {
                        let vs: Option<Vec<Value>> = args.iter().map(literal).collect();
                        out.lets.0.push(vs.ok_or_else(|| err(mn, d.at, "a `Lets` line's fields are literals".into()))?);
                    }
                    if let Some(n) = d.what.name() {
                        fixed.push((qualify(mn, n), d.what.clone()));
                    }
                    out.norms.push(NormDecl { module: mn.clone(), decl: d });
                }
                what => {
                    if let Some(n) = what.name() {
                        fixed.push((qualify(mn, n), what));
                    }
                }
            }
        }
    }
    for (q, what) in fixed {
        let module = q.rsplit_once('.').map(|(m, _)| m.to_string()).unwrap_or_default();
        match (out.syms.get_mut(&q), what) {
            (Some(Sym::Effect { does, result, fields, .. }), DeclKind::Effect { does: d2, result: r2, fields: f2, .. }) => (*does, *result, *fields) = (d2, r2, f2),
            (Some(Sym::Fn { body, result, fields, .. }), DeclKind::Fn { body: b2, result: r2, fields: f2, .. }) => (*body, *result, *fields) = (b2, r2, f2),
            (Some(Sym::Call { template, result, fields, .. }), DeclKind::Call { template: t2, result: r2, fields: f2, .. }) => (*template, *result, *fields) = (t2, r2, f2),
            (Some(Sym::Norm { fields, .. }), DeclKind::Norm { fields: f2, .. }) => *fields = f2,
            (Some(Sym::Record { fields, .. }), DeclKind::Record { fields: f2, .. }) => *fields = f2,
            (Some(Sym::Sum { .. }), DeclKind::Sum { variants, .. }) => {
                for v in variants {
                    if let Some(Sym::Ctor { fields, .. }) = out.syms.get_mut(&qualify(&module, &v.name)) {
                        *fields = v.fields;
                    }
                }
            }
            _ => {}
        }
    }
    out.lets_fact = lets_name.and_then(|n| match out.syms.get(&n) {
        Some(Sym::Norm { fields: Fields::Named(fs), .. }) if fs.len() >= 2 => match &fs[fs.len() - 2].ty {
            Type::Named(p, _) => Some((n, p.to_string())),
            _ => None,
        },
        _ => None,
    });
    out.hash = hash.finalize().into();
    Ok(out)
}

/// `module.name`, or the name alone for a text with no `module` line.
pub fn qualify(module: &str, name: &str) -> String {
    if module.is_empty() { name.to_string() } else { format!("{module}.{name}") }
}

/// A literal's value: a number, a text with no hole, a truth value, `()`.
pub fn literal(e: &Expr) -> Option<Value> {
    match &e.e {
        E::Lit(Lit::Int(n)) => Some(Value::Int(*n)),
        E::Lit(Lit::Bool(b)) => Some(Value::Bool(*b)),
        E::Lit(Lit::Unit) => Some(Value::Unit),
        E::Neg(x) => literal(x).and_then(|v| v.as_int()).map(|n| Value::Int(-n)),
        E::Text(ps) => match ps.as_slice() {
            [Piece::Words(w)] => Some(Value::Text(w.clone())),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::read;

    fn load_texts(ts: &[&str]) -> Result<Loaded, LoadError> {
        load(ts.iter().map(|t| read(t).unwrap()).collect())
    }

    #[test]
    fn a_name_declared_twice_is_refused() {
        let e = load_texts(&["module m\nFact A Identified by String\nEffect A() -> Unit"]).unwrap_err();
        assert!(e.why.contains("m.A is declared twice"), "{e}");
    }

    #[test]
    fn a_name_that_stands_for_nothing_binds_by_enumeration_only_in_a_clause_or_a_where() {
        let e = load_texts(&["module m\nfn f() -> Int { return Missing(1) }"]).unwrap_err();
        assert!(e.why.contains("Missing stands for no declaration"), "{e}");
        let e = load_texts(&["module m\nfn f() -> Int { return misspelt }"]).unwrap_err();
        assert!(e.why.contains("misspelt stands for no declaration and no variable"), "{e}");
        let l = load_texts(&["module m\nFact P(x: Int)\nFact Q(x: Int) Holds when P(y) && x == y\nEffect E() -> Int\non E() where P(z) do { return z }"]).unwrap();
        assert_eq!(l.norms.len(), 2);
        let Some(DeclKind::Norm { clauses, .. }) = l.norms.iter().map(|d| &d.decl.what).find(|w| w.name() == Some("Q")) else { panic!() };
        let Clause::HoldsWhen(c) = &clauses[0] else { panic!() };
        assert!(matches!(&c.e, E::Bin(_, a, _) if matches!(&a.e, E::Apply(p, xs) if p.to_string() == "m.P" && matches!(&xs[0].e, E::Var(y) if y == "y"))), "{c:?}");
        assert!(matches!(&l.ons[0].body.e, E::Return(x) if matches!(&x.e, E::Var(z) if z == "z")));
    }

    #[test]
    fn imports_load_first_and_a_cycle_is_refused() {
        let l = load_texts(&["module b\nimport a as A\nFact Q(x: A.P)", "module a\nFact P Identified by String"]).unwrap();
        assert_eq!(l.modules, ["a", "b"]);
        assert!(matches!(&l.syms["b.Q"], Sym::Norm { fields: Fields::Named(f), .. } if f[0].ty == Type::Named(Path::one("a.P"), vec![])));
        let e = load_texts(&["module a\nimport b\nFact P Identified by String", "module b\nimport a\nFact Q Identified by String"]).unwrap_err();
        assert!(e.why.contains("cycle"), "{e}");
    }

    #[test]
    fn a_variables_field_is_a_field_and_a_lets_line_is_kept() {
        let l = load_texts(&["module m\nFact Lets(h: String, p: String, w: String)\n+Lets(\"d\", \"p\", \"w\").\ntype R { a: Int }\nEffect Get() -> R\nfn f() -> Program<Int> { r <- perform Get(); return r.a }"]).unwrap();
        assert_eq!(l.lets.0, vec![vec![Value::text("d"), Value::text("p"), Value::text("w")]]);
        let Sym::Fn { body, .. } = &l.syms["m.f"] else { panic!() };
        let E::Bind(_, _, rest) = &body.e else { panic!("{body:?}") };
        assert!(matches!(&rest.e, E::Return(x) if matches!(&x.e, E::Field(_, f) if f == "a")), "{rest:?}");
    }
}
