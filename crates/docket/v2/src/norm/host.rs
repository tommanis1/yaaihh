//! The host table (6.3) as eFLINT's terms apply it: its functions on texts and numbers, since no fact holds a list.

use crate::run::host;
use crate::run::value::Value;
use eflint::ast::{Id, id};
use eflint::compile::{HostTable, Ty};
use eflint::eval::Value as Got;

/// The functions of `run::host` a condition may apply.
pub struct Table;

/// The types of NAME's arguments and its result, if a condition may apply it.
fn sig(name: &str) -> Option<(Vec<Ty>, Ty)> {
    let (s, i) = (Ty::Strings, Ty::Ints);
    Some(match name {
        "matches_glob" | "contains" => (vec![s.clone(), s], Ty::Bool),
        "diff_lines" => (vec![s.clone(), s], i),
        "added_lines" | "removed_lines" | "str_len" => (vec![s], i),
        _ => return None,
    })
}

/// eFLINT's names of the types of NAME's arguments.
pub fn arg_types(name: &str) -> Option<Vec<Id>> {
    let (args, _) = sig(name)?;
    Some(args.iter().map(|t| id(if *t == Ty::Ints { "int" } else { "string" })).collect())
}

impl HostTable for Table {
    fn sig(&self, name: &str) -> Option<(Vec<Ty>, Ty)> {
        sig(name)
    }

    fn call(&self, name: &str, args: &[Got]) -> Result<Got, String> {
        let given = args
            .iter()
            .map(|a| match a {
                Got::Str(s) => Ok(Value::Text(s.to_string())),
                Got::Int(n) => Ok(Value::Int(*n)),
                other => Err(format!("{name}: {other:?} is no text or number")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        match host::apply(name, &given)? {
            Value::Bool(b) => Ok(Got::Bool(b)),
            Value::Int(n) => Ok(Got::Int(n)),
            other => Err(format!("{name} gave {other}, which no condition reads")),
        }
    }
}
