//! Each normative type's kind and fields as eFLINT reads them, and values carried between the machine's form and eFLINT's.

use crate::check::names::qualify;
use crate::run::norms::NormDecl;
use crate::run::value::{Instance, Value};
use crate::syntax::tree::{DeclKind, Fields, Norm, Type};
use eflint::ast::{Elem, Id, Tagged, Var, id};
use eflint::replay::Value as Given;
use std::collections::{BTreeMap, BTreeSet};

/// eFLINT's name for Docket's `Bool`: a type of the two texts `false` and `true`.
pub const BOOL: &str = "Bool";

/// A normative type: its kind, and its fields, each a variable of its written name typed by eFLINT's name of its type;
/// for a type identified by a primitive, that primitive's eFLINT name and no fields.
pub struct Shape {
    pub kind: Norm,
    pub fields: Vec<Var>,
    pub by: Option<Id>,
}

/// Every normative type of a loaded text by its qualified name, and those whose rule makes them tests (6.2).
#[derive(Default)]
pub struct Shapes {
    pub by: BTreeMap<String, Shape>,
    pub tests: BTreeSet<String>,
}

impl Shapes {
    /// The shapes of DECLS' types; refused where a field is not first-order or a Duty lacks a holder and a claimant
    /// (14, check 5), or an Act has no actor (5.3).
    pub fn of(decls: &[NormDecl]) -> Result<Shapes, String> {
        let facts: BTreeSet<String> = decls
            .iter()
            .filter_map(|d| match &d.decl.what {
                DeclKind::Norm { kind: Norm::Fact, name, .. } => Some(qualify(&d.module, name)),
                _ => None,
            })
            .collect();
        let mut out = Shapes::default();
        for d in decls {
            let DeclKind::Norm { kind, name, fields, .. } = &d.decl.what else { continue };
            let q = qualify(&d.module, name);
            let at = |why: String| format!("{}:{}: {q}: {why}", d.module, d.decl.at);
            let ty = |t: &Type| match t {
                Type::Named(p, a) if a.is_empty() => match p.to_string().as_str() {
                    "Int" => Some(id("int")),
                    "String" => Some(id("string")),
                    "Bool" => Some(id(BOOL)),
                    n if facts.contains(n) => Some(id(n)),
                    _ => None,
                },
                _ => None,
            };
            let shape = match fields {
                Fields::By(t) => match ty(t) {
                    Some(p) if (&*p == "string" || &*p == "int") && *kind != Norm::Duty => Shape { kind: *kind, fields: Vec::new(), by: Some(p) },
                    _ => return Err(at("a type is identified by String or Int, and a Duty by its fields".into())),
                },
                Fields::Named(fs) => {
                    let mut vars = Vec::new();
                    for f in fs {
                        let dom = ty(&f.ty).ok_or_else(|| at(format!("field {} is no fact type, Int, String or Bool (6.2)", f.name)))?;
                        vars.push(Var { dom, dec: id(&f.name) });
                    }
                    match kind {
                        Norm::Duty if vars.len() < 2 => return Err(at("a Duty has a holder and a claimant (5.3)".into())),
                        Norm::Act if vars.is_empty() => return Err(at("an Act's first field is its actor (5.3)".into())),
                        _ => Shape { kind: *kind, fields: vars, by: None },
                    }
                }
            };
            out.by.insert(q, shape);
        }
        Ok(out)
    }

    pub fn get(&self, ty: &str) -> Result<&Shape, String> {
        self.by.get(ty).ok_or_else(|| format!("{ty} is no Fact, Act, Event or Duty"))
    }

    /// The instance of eFLINT type TY that V is.
    pub fn tagged(&self, ty: &Id, v: &Value) -> Result<Tagged, String> {
        let elem = match v {
            Value::Text(s) => Elem::Str(id(s)),
            Value::Int(n) => Elem::Int(*n),
            Value::Bool(b) => return Ok(Tagged { elem: Elem::Str(id(&b.to_string())), dom: id(BOOL) }),
            Value::Inst(i) => return self.instance(i),
            other => return Err(format!("{other} is no value a fact holds (6.2)")),
        };
        Ok(Tagged { elem, dom: ty.clone() })
    }

    /// The eFLINT instance of a machine's instance.
    pub fn instance(&self, i: &Instance) -> Result<Tagged, String> {
        let s = self.get(&i.ty)?;
        let ty = id(&i.ty);
        match (&s.by, i.args.as_slice()) {
            (Some(_), [v]) => self.tagged(&ty, v),
            (None, args) if args.len() == s.fields.len() => {
                let fs = s.fields.iter().zip(args).map(|(f, v)| self.tagged(&f.dom, v)).collect::<Result<Vec<_>, _>>()?;
                Ok(Tagged { elem: Elem::Prod(fs.into()), dom: ty })
            }
            _ => Err(format!("{i} does not have the fields of {}", i.ty)),
        }
    }

    /// The act or event of a machine's instance, as eFLINT's reader performs it.
    pub fn event(&self, i: &Instance) -> Result<eflint::replay::Event, String> {
        let mut args = i.args.iter().map(given).collect::<Result<Vec<_>, _>>()?;
        match self.get(&i.ty)?.kind {
            Norm::Act if !args.is_empty() => {
                let actor = args.remove(0);
                Ok(eflint::replay::Event::Act { name: i.ty.clone(), actor, objects: args })
            }
            Norm::Event => Ok(eflint::replay::Event::Event { name: i.ty.clone(), objects: args }),
            _ => Err(format!("{} is no Act or Event", i.ty)),
        }
    }
}

/// A field's value as eFLINT's reader is given it.
fn given(v: &Value) -> Result<Given, String> {
    Ok(match v {
        Value::Text(s) => Given::Str(s.clone()),
        Value::Int(n) => Given::Int(*n),
        Value::Bool(b) => Given::Str(b.to_string()),
        Value::Inst(i) => Given::List(i.args.iter().map(given).collect::<Result<_, _>>()?),
        other => return Err(format!("{other} is no value a fact holds (6.2)")),
    })
}

/// A value of a party as a duty's holder field holds it.
pub fn elem(shapes: &Shapes, v: &Value) -> Result<Elem, String> {
    Ok(shapes.tagged(&id("string"), v)?.elem)
}

/// The machine's value of an eFLINT instance.
pub fn value(te: &Tagged) -> Value {
    match &te.elem {
        Elem::Str(s) if &*te.dom == BOOL => Value::Bool(&**s == "true"),
        Elem::Str(s) => Value::Text(s.to_string()),
        Elem::Int(n) => Value::Int(*n),
        Elem::Prod(fs) => Value::Inst(Instance { ty: te.dom.to_string(), args: fs.iter().map(value).collect() }),
    }
}

/// An instance as a step lists it: of a type identified by a primitive, with that primitive its one field.
pub fn listed(te: &Tagged) -> Instance {
    match value(te) {
        Value::Inst(i) => i,
        v => Instance { ty: te.dom.to_string(), args: vec![v] },
    }
}
