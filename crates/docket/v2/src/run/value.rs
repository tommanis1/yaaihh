//! Values: what an expression gives, what a fact holds, what crosses to a driver (spec 6.1, 6.2).

use crate::syntax::tree::Expr;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

/// An instance of a declared Fact, Duty, Act or Event: its type's qualified name and its field values, first-order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instance {
    pub ty: String,
    pub args: Vec<Value>,
}

impl std::fmt::Display for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}(", self.ty)?;
        for (k, a) in self.args.iter().enumerate() {
            write!(f, "{}{a}", if k > 0 { ", " } else { "" })?;
        }
        write!(f, ")")
    }
}

/// One total order on values (6.2): by kind, then numbers by size, texts by code point, the rest field by field.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Value {
    Unit,
    Bool(bool),
    Int(i64),
    Text(String),
    Inst(Instance),
    /// A constructor of a declared `type` (or `Some`, `None`, `Again`, `Done`) with its fields. What power it
    /// needs (12) is not in the value: the loaded declaration says it where the value crosses.
    Ctor { name: String, args: Vec<Value> },
    Tuple(Vec<Value>),
    List(Vec<Value>),
    Record { ty: String, fields: Vec<(String, Value)> },
    /// A `Call<T>`: the prompt's name and its arguments; nothing happens until it is performed.
    Prompt { name: String, args: Vec<Value> },
    /// A `Program<T>`: a body and what it closes over (8.5); no continuation and no run.
    Program(Prog),
    /// An arm's `k`, by the machine's number for it (8.3): bound to the arm's name and only resumed, never a
    /// field, an argument or a result (7).
    Resumption(usize),
}

impl Value {
    pub fn text(s: &str) -> Value {
        Value::Text(s.to_string())
    }
    pub fn ctor(name: &str, args: Vec<Value>) -> Value {
        Value::Ctor { name: name.to_string(), args }
    }
    pub fn some(v: Value) -> Value {
        Value::ctor("Some", vec![v])
    }
    pub fn none() -> Value {
        Value::ctor("None", Vec::new())
    }
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }
    /// The powers of the guarded constructors in this value, its fields included (12), each as `needs` gives it
    /// for the constructor's qualified name.
    pub fn powers(&self, needs: &dyn Fn(&str) -> Option<String>, out: &mut Vec<String>) {
        match self {
            Value::Ctor { name, args } => {
                out.extend(needs(name));
                args.iter().for_each(|a| a.powers(needs, out));
            }
            Value::Tuple(vs) | Value::List(vs) => vs.iter().for_each(|a| a.powers(needs, out)),
            Value::Record { fields, .. } => fields.iter().for_each(|(_, a)| a.powers(needs, out)),
            Value::Prompt { args, .. } | Value::Inst(Instance { args, .. }) => args.iter().for_each(|a| a.powers(needs, out)),
            _ => {}
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let list = |f: &mut std::fmt::Formatter<'_>, open: &str, vs: &[Value], close: &str| {
            write!(f, "{open}")?;
            for (k, a) in vs.iter().enumerate() {
                write!(f, "{}{a}", if k > 0 { ", " } else { "" })?;
            }
            write!(f, "{close}")
        };
        match self {
            Value::Unit => write!(f, "()"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(n) => write!(f, "{n}"),
            Value::Text(s) => write!(f, "{s:?}"),
            Value::Inst(i) => write!(f, "{i}"),
            Value::Ctor { name, args } if args.is_empty() => write!(f, "{name}"),
            Value::Ctor { name, args } => list(f, &format!("{name}("), args, ")"),
            Value::Tuple(vs) => list(f, "(", vs, ")"),
            Value::List(vs) => list(f, "[", vs, "]"),
            Value::Record { ty, fields } => {
                write!(f, "{ty} {{")?;
                for (k, (n, v)) in fields.iter().enumerate() {
                    write!(f, "{}{n}: {v}", if k > 0 { ", " } else { " " })?;
                }
                write!(f, " }}")
            }
            Value::Prompt { name, args } => list(f, &format!("{name}("), args, ")"),
            Value::Program(Prog(c)) => {
                write!(f, "program({}, {}", c.module, crate::syntax::form::expr(&c.body))?;
                for (n, v) in &c.env {
                    write!(f, ", {n} = {v}")?;
                }
                write!(f, ")")
            }
            Value::Resumption(k) => write!(f, "k{k}"),
        }
    }
}

/// Names bound to values: a scope of a program, closed over by a program value.
pub type Env = BTreeMap<String, Value>;

/// A program value: its body, its scope, and the module whose text it is (8.2's source).
#[derive(Clone, Debug)]
pub struct Prog(pub Arc<Closure>);

#[derive(Debug)]
pub struct Closure {
    pub body: Expr,
    pub env: Env,
    pub module: String,
}

// A program is equal only to itself; programs are never fields of a fact (6.2), so this order is never a record's.
// A replay key (11) does not use it: the key hashes a program by its written form (`Display`): its module, its
// body as `syntax::form` writes it, and the values it closes over.
impl PartialEq for Prog {
    fn eq(&self, o: &Self) -> bool {
        Arc::ptr_eq(&self.0, &o.0)
    }
}
impl Eq for Prog {}
impl PartialOrd for Prog {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Prog {
    fn cmp(&self, o: &Self) -> Ordering {
        (Arc::as_ptr(&self.0) as usize).cmp(&(Arc::as_ptr(&o.0) as usize))
    }
}
impl std::hash::Hash for Prog {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        (Arc::as_ptr(&self.0) as usize).hash(h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_have_one_order_numbers_by_size_texts_by_code_point() {
        let mut v = vec![Value::text("b"), Value::Int(10), Value::text("B"), Value::Int(-2), Value::Bool(true)];
        v.sort();
        assert_eq!(v, vec![Value::Bool(true), Value::Int(-2), Value::Int(10), Value::text("B"), Value::text("b")]);
    }

    #[test]
    fn a_values_powers_are_those_of_its_guarded_constructors_at_any_depth() {
        let needs = |n: &str| (n == "Skip").then(|| n.to_string());
        let mut p = Vec::new();
        Value::List(vec![Value::Tuple(vec![Value::Int(1), Value::ctor("Skip", vec![Value::text("x")])]), Value::ctor("Go", vec![])]).powers(&needs, &mut p);
        assert_eq!(p, ["Skip"]);
    }
}
