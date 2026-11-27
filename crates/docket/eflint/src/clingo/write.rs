//! The export's plain writing: names, strings and instances as Clingo reads them, a comment that stays one, and a
//! condition read with given fields in place of their variables.

use crate::ast::*;
use crate::compile::{Spec, free};
use std::collections::BTreeMap;

pub(super) type R<T> = Result<T, String>;

/// Literals that hold together; a list of them, the alternatives of a condition.
pub(super) type Body = Vec<String>;

/// Each alternative of A with each of B.
pub(super) fn cross(a: Vec<Body>, b: Vec<Body>) -> Vec<Body> {
    a.iter().flat_map(|x| b.iter().map(move |y| [x.clone(), y.clone()].concat())).collect()
}

/// T with each variable of GIVEN replaced by its term, a field left out of an application read as the variable of
/// its name; refused where a quantifier of T would bind a variable a term given reads.
pub(super) fn subst(spec: &Spec, t: &Term, given: &BTreeMap<Var, Term>) -> R<Term> {
    let s = |a: &Term| subst(spec, a, given).map(Box::new);
    Ok(match t {
        Term::Ref(v) => given.get(v).cloned().unwrap_or_else(|| t.clone()),
        Term::Q(q, xs, a) => {
            let inner: BTreeMap<Var, Term> = given.iter().filter(|(v, _)| !xs.contains(v)).map(|(v, a)| (v.clone(), a.clone())).collect();
            if inner.values().any(|a| free(spec, a).iter().any(|f| xs.contains(f))) {
                return Err("a domain constraint whose quantifier binds a variable a field given reads".into());
            }
            Term::Q(*q, xs.clone(), Box::new(subst(spec, a, &inner)?))
        }
        Term::Bin(o, a, b) => Term::Bin(*o, s(a)?, s(b)?),
        Term::Un(o, a) => Term::Un(*o, s(a)?),
        Term::Project(a, f) => Term::Project(s(a)?, f.clone()),
        Term::Tag(a, d) => Term::Tag(s(a)?, d.clone()),
        Term::App(d, Args::Pos(ts)) => Term::App(d.clone(), Args::Pos(ts.iter().map(|a| subst(spec, a, given)).collect::<R<_>>()?)),
        Term::App(d, Args::Mods(ms)) => {
            let mut out: Vec<(Var, Term)> = ms.iter().map(|(v, a)| Ok((v.clone(), subst(spec, a, given)?))).collect::<R<_>>()?;
            if let Some(Domain::Products(fs)) = spec.find(d).map(|t| &t.domain) {
                for f in fs.iter().filter(|f| !ms.iter().any(|(v, _)| v == *f)) {
                    if let Some(a) = given.get(f) {
                        out.push((f.clone(), a.clone()));
                    }
                }
            }
            Term::App(d.clone(), Args::Mods(out))
        }
        Term::BoolLit(_) | Term::IntLit(_) | Term::StringLit(_) | Term::CurrentTime => t.clone(),
    })
}

/// TEXT as a comment line: a line break in it would end the comment and start a rule.
pub(super) fn comment(text: &str) -> String {
    format!("% {}", text.replace(['\n', '\r'], " "))
}

/// The instance TE as a term, as Clingo writes it.
pub fn instance(te: &Tagged) -> String {
    let n = name(&te.dom);
    match &te.elem {
        Elem::Str(s) => format!("{n}({})", quote(s)),
        Elem::Int(i) => format!("{n}({i})"),
        Elem::Prod(args) if args.is_empty() => n,
        Elem::Prod(args) => format!("{n}({})", args.iter().map(instance).collect::<Vec<_>>().join(", ")),
    }
}

/// A type's name as a Clingo constant: `e_` before it, so no name is a keyword, and each character that cannot
/// stand in one written as `_` and a letter (`_` itself as `__`), so no two names meet.
pub(super) fn name(d: &str) -> String {
    format!("e_{}", escape(d))
}

pub(super) fn escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '_' => "__".to_string(),
            '-' => "_d".to_string(),
            '.' => "_o".to_string(),
            '\'' => "_q".to_string(),
            c if c.is_ascii_alphanumeric() => c.to_string(),
            c => format!("_x{:x}_", c as u32),
        })
        .collect()
}

pub(super) fn var(v: &Var) -> String {
    format!("V{}_{}", name(&v.dom), escape(&v.dec))
}

/// S as a Clingo string: a line break written `\n`, as Clingo reads it (another control character stands as itself).
pub(super) fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n"))
}
