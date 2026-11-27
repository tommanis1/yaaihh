//! `Call-Text`: a call's template with each hole filled from the call's own fields, as the engine fills a text's.

use docket_v2::check::names::Sym;
use docket_v2::run::value::Value;
use docket_v2::syntax::tree::{Expr, Fields, Piece, E};
use std::collections::BTreeMap;

/// The template of `call` with its holes filled: a text as it stands, any other value as the engine writes it.
pub fn fill(syms: &BTreeMap<String, Sym>, call: &Value) -> Result<String, String> {
    let Value::Prompt { name, args } = call else { return Err(format!("{call} is no call")) };
    let Some(Sym::Call { fields, template, .. }) = syms.get(name) else { return Err(format!("{name} is no call of the loaded text")) };
    if fields.len() != args.len() {
        return Err(format!("{name} has {} fields and was given {} values", fields.len(), args.len()));
    }
    let by: BTreeMap<&str, &Value> = fields.iter().map(|f| f.name.as_str()).zip(args).collect();
    template
        .iter()
        .map(|p| match p {
            Piece::Words(w) => Ok(w.clone()),
            Piece::Hole(h) => hole(syms, &by, h).map(|v| match v {
                Value::Text(t) => t,
                v => v.to_string(),
            }),
        })
        .collect()
}

/// A hole's value: a field of the call, or a field of such a value; a hole that computes is refused.
fn hole(syms: &BTreeMap<String, Sym>, by: &BTreeMap<&str, &Value>, h: &Expr) -> Result<Value, String> {
    let of = |n: &str| by.get(n).map(|v| (*v).clone()).ok_or(format!("the hole {n} names no field of the call"));
    match &h.e {
        E::Var(n) => of(n),
        E::Name(p) if p.0.len() == 1 => of(p.last()),
        E::Field(x, f) => {
            let v = hole(syms, by, x)?;
            field(syms, &v, f).ok_or(format!("{v} has no field {f}"))
        }
        _ => Err(format!("the hole at {} is more than a field of the call", h.at)),
    }
}

/// Field `f` of a record, a constructor's value or an instance, by the declaration's order of fields.
fn field(syms: &BTreeMap<String, Sym>, v: &Value, f: &str) -> Option<Value> {
    let at = |names: Vec<&str>, args: &[Value]| names.iter().position(|n| *n == f).and_then(|k| args.get(k).cloned());
    match v {
        Value::Record { fields, .. } => fields.iter().find(|(n, _)| n == f).map(|(_, x)| x.clone()),
        Value::Ctor { name, args } => match syms.get(name) {
            Some(Sym::Ctor { fields, .. }) => at(fields.iter().map(|x| x.name.as_str()).collect(), args),
            _ => None,
        },
        Value::Inst(i) => match syms.get(&i.ty) {
            Some(Sym::Norm { fields: Fields::Named(fs), .. }) => at(fs.iter().map(|x| x.name.as_str()).collect(), &i.args),
            _ => None,
        },
        _ => None,
    }
}
