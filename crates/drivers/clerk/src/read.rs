//! `Read-As`: a model's text read as a call's declared type, written as the texts write a value, or why it is not one.

use docket_v2::check::names::Sym;
use docket_v2::run::value::{Instance, Value};
use docket_v2::syntax::read::read_block;
use docket_v2::syntax::tree::{Expr, Field, Fields, Lit, Norm, Piece, Type, E};
use std::collections::BTreeMap;

/// `text` as one value of type `ty`: alone, with only spaces around it, or alone in one fenced block.
pub fn read_as(syms: &BTreeMap<String, Sym>, ty: &Type, text: &str) -> Result<Value, String> {
    let bare = unfenced(text.trim());
    let e = read_block(&format!("{{ {bare} }}")).map_err(|e| format!("not one value as the texts write it ({e}): {}", start(bare)))?;
    value(syms, ty, &e)
}

/// The text inside a fence that holds the whole answer, else the text itself.
fn unfenced(t: &str) -> &str {
    let Some(rest) = t.strip_prefix("```") else { return t };
    match (rest.split_once('\n'), rest.strip_suffix("```")) {
        (Some((_, body)), Some(_)) => body.strip_suffix("```").unwrap_or(body).trim(),
        _ => t,
    }
}

/// The text's first 80 characters, to say what was read.
fn start(t: &str) -> String {
    let s: String = t.chars().take(80).collect();
    if s.len() < t.len() { format!("{s}…") } else { s }
}

/// Expression `e` as a value of type `ty`, built by its loaded declaration as the engine builds one (values.rs).
fn value(syms: &BTreeMap<String, Sym>, ty: &Type, e: &Expr) -> Result<Value, String> {
    let no = || Err(format!("{} is not written as a value of {}", start(&written(e)), shown(ty)));
    let Type::Named(p, args) = ty else {
        return match (ty, &e.e) {
            (Type::Tuple(ts), E::Tuple(xs)) if ts.len() == xs.len() => ts.iter().zip(xs).map(|(t, x)| value(syms, t, x)).collect::<Result<_, _>>().map(Value::Tuple),
            (Type::Tuple(ts), E::Lit(Lit::Unit)) if ts.is_empty() => Ok(Value::Unit),
            _ => no(),
        };
    };
    let q = p.to_string();
    let arg = |k: usize| args.get(k).ok_or(format!("{q} is given no type to read its values by"));
    match (q.as_str(), &e.e) {
        ("Int", E::Lit(Lit::Int(n))) => Ok(Value::Int(*n)),
        ("Int", E::Neg(x)) if matches!(x.e, E::Lit(Lit::Int(_))) => value(syms, ty, x).map(|v| Value::Int(-v.as_int().unwrap_or_default())),
        ("Bool", E::Lit(Lit::Bool(b))) => Ok(Value::Bool(*b)),
        ("Unit", E::Lit(Lit::Unit)) => Ok(Value::Unit),
        ("String", E::Text(ps)) => words(ps).map(Value::Text).ok_or(format!("{} has a hole: a value is written without one", start(&written(e)))),
        ("List", E::List(xs)) => xs.iter().map(|x| value(syms, arg(0)?, x)).collect::<Result<_, _>>().map(Value::List),
        ("Option", E::Name(n)) if n.last() == "None" => Ok(Value::none()),
        ("Option", E::Apply(n, xs)) if n.last() == "Some" && xs.len() == 1 => value(syms, arg(0)?, &xs[0]).map(Value::some),
        (_, _) => match syms.get(&q) {
            Some(Sym::Sum { ctors, .. }) => {
                let (n, xs) = applied(e).ok_or(format!("{} names no alternative of {q}", start(&written(e))))?;
                let c = ctors.iter().find(|c| c.rsplit('.').next() == Some(n)).ok_or(format!("{n} is no alternative of {q}: {}", ctors.join(", ")))?;
                let Some(Sym::Ctor { fields, .. }) = syms.get(c) else { return Err(format!("{c} is declared as no constructor")) };
                each(syms, c, fields, xs).map(|args| Value::Ctor { name: c.clone(), args })
            }
            Some(Sym::Record { fields, .. }) => match applied(e) {
                Some((n, xs)) if q.rsplit('.').next() == Some(n) => each(syms, &q, fields, xs).map(|vs| Value::Record { ty: q.clone(), fields: fields.iter().map(|f| f.name.clone()).zip(vs).collect() }),
                _ => no(),
            },
            Some(Sym::Norm { kind: Norm::Fact | Norm::Duty, fields: Fields::By(t), .. }) => match (&e.e, applied(e)) {
                (E::Name(n), _) if n.0.len() == 1 && matches!(t, Type::Named(s, _) if s.to_string() == "String") => Ok(Value::text(n.last())),
                (_, Some((n, [x]))) if q.rsplit('.').next() == Some(n) => value(syms, t, x),
                _ => value(syms, t, e),
            },
            Some(Sym::Norm { kind: Norm::Fact | Norm::Duty, fields: Fields::Named(fs), .. }) => match applied(e) {
                Some((n, xs)) if q.rsplit('.').next() == Some(n) => each(syms, &q, fs, xs).map(|args| Value::Inst(Instance { ty: q.clone(), args })),
                _ => no(),
            },
            _ => no(),
        },
    }
}

/// `Name` or `Name(args)`: the name as written, its last part, and the arguments.
fn applied(e: &Expr) -> Option<(&str, &[Expr])> {
    match &e.e {
        E::Name(n) => Some((n.last(), &[])),
        E::Apply(n, xs) => Some((n.last(), xs.as_slice())),
        _ => None,
    }
}

/// Each argument read by its field's type, one for each field.
fn each(syms: &BTreeMap<String, Sym>, of: &str, fields: &[Field], xs: &[Expr]) -> Result<Vec<Value>, String> {
    if fields.len() != xs.len() {
        return Err(format!("{of} has {} fields and was written with {}", fields.len(), xs.len()));
    }
    fields.iter().zip(xs).map(|(f, x)| value(syms, &f.ty, x)).collect()
}

/// A text literal's words, if it has no hole.
fn words(ps: &[Piece]) -> Option<String> {
    ps.iter().map(|p| if let Piece::Words(w) = p { Some(w.as_str()) } else { None }).collect()
}

fn written(e: &Expr) -> String {
    docket_v2::syntax::form::expr(e)
}

fn shown(t: &Type) -> String {
    match t {
        Type::Named(p, args) if args.is_empty() => p.to_string(),
        Type::Named(p, args) => format!("{p}<{}>", args.iter().map(shown).collect::<Vec<_>>().join(", ")),
        Type::Tuple(ts) => format!("({})", ts.iter().map(shown).collect::<Vec<_>>().join(", ")),
        Type::Program(..) => "a program".into(),
    }
}
