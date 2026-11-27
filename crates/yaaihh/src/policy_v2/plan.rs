//! The Policy page's plan, read from the package's own text: each fact that resolves level by level, its values and setters.

use docket_v2::check::Loaded;
use docket_v2::syntax::tree::{Clause, DeclKind, Expr, Field, Fields, Module, Norm, Op, Piece, Quant, Type, E};
use std::collections::HashMap;

/// A fact the package resolves level by level, named as the top module names it.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub fact: String,
    /// Whether it holds for each party (its fields before the value).
    pub per_party: bool,
    /// The instances the package declares of its value's type.
    pub values: Vec<String>,
    /// The facts it falls back through, most general first.
    pub levels: Vec<Level>,
    /// What holds with no level set, when the package says.
    pub default: Option<String>,
}

/// One level of a row: the fact that sets it, whether it is per party, and the acts that create it; a level has at most
/// the row's two fields, so its last argument is the value and the one before it, if any, the party.
#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    pub fact: String,
    pub per_party: bool,
    pub set_by: Vec<Setter>,
}

/// What fills one field of a setting act: the party performing it, the level's party, or the level's value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fill {
    Actor,
    Party,
    Value,
}

/// An act whose `Creates` makes a level, named in full as an act request names it, and how the page fills its fields
/// (`None` when it cannot, so the page only names it).
#[derive(Clone, Debug, PartialEq)]
pub struct Setter {
    pub act: String,
    pub fills: Option<Vec<Fill>>,
}

/// The rows of MODULES whose facts module TOP can name, each named as TOP's imports name it.
pub fn rows(modules: &[Module], top: &str) -> Vec<Row> {
    let top_module = modules.iter().find(|m| named(m) == top);
    modules
        .iter()
        .filter_map(|m| Some((m, prefix(top_module?, m)?)))
        .flat_map(|(m, prefix)| of_module(m, &prefix))
        .collect()
}

/// The page's plan of a text: its rows, and the hash the engine writes in its `loaded` line when it loads that text.
pub struct Plan {
    pub rows: Vec<Row>,
    pub hash: String,
}

/// The plan of the engine's texts: MODULES as read and LOADED from them, its questions read in module TOP.
pub fn plan(modules: &[Module], loaded: &Loaded, top: &str) -> Plan {
    Plan { rows: rows(modules, top), hash: loaded.hash.iter().map(|b| format!("{b:02x}")).collect() }
}

fn named(m: &Module) -> String {
    m.name.as_ref().map(ToString::to_string).unwrap_or_default()
}

/// How TOP names M's declarations: nothing when M is TOP, its import's alias, else its path; None when TOP does not import M.
pub(crate) fn prefix(top: &Module, m: &Module) -> Option<String> {
    if named(m) == named(top) {
        return Some(String::new());
    }
    let i = top.imports.iter().find(|i| i.path.to_string() == named(m))?;
    Some(format!("{}.", i.alias.clone().unwrap_or_else(|| i.path.to_string())))
}

fn of_module(m: &Module, prefix: &str) -> Vec<Row> {
    let facts: HashMap<&str, &Fields> = m
        .decls
        .iter()
        .filter_map(|d| match &d.what {
            DeclKind::Norm { kind: Norm::Fact, name, fields, .. } => Some((name.as_str(), fields)),
            _ => None,
        })
        .collect();
    let value_type = |name: &str| facts.get(name).and_then(|f| last_type(f));
    m.decls
        .iter()
        .filter_map(|d| {
            let DeclKind::Norm { kind: Norm::Fact, name, fields: Fields::Named(fs), clauses } = &d.what else { return None };
            if fs.len() > 2 {
                return None;
            }
            let ty = named_type(&fs.last()?.ty)?;
            // a level the page can ask as it asks the row: its value alone, or the row's party and its value
            let fits = |l: &str| matches!(facts.get(l), Some(Fields::Named(lf)) if lf.len() <= fs.len() && (lf.len() < 2 || lf[0].ty == fs[0].ty));
            let values = instances(m, ty);
            if values.is_empty() {
                return None;
            }
            let var = &fs.last()?.name;
            let (mut levels, mut default) = (Vec::new(), None);
            for c in clauses {
                let Clause::HoldsWhen(e) = c else { continue };
                let leaves = conjuncts(e);
                let level = leaves.iter().find_map(|x| match &x.e {
                    E::Apply(p, _) if p.0.len() == 1 && p.last() != name && value_type(p.last()) == Some(ty) && fits(p.last()) => Some(p.last()),
                    _ => None,
                });
                match level {
                    Some(l) => levels.push((l, ruled_out(&leaves))),
                    None => default = default.or_else(|| leaves.iter().find_map(|x| literal_for(x, var))),
                }
            }
            if levels.len() + usize::from(default.is_some()) < 2 {
                return None;
            }
            // most general first: a level's clause rules out with `Not(Exists …)` each level more specific than it
            let named: Vec<&str> = levels.iter().map(|(l, _)| *l).collect();
            levels.sort_by_key(|(_, ruled)| std::cmp::Reverse(ruled.iter().filter(|r| named.contains(r)).count()));
            let levels = levels
                .into_iter()
                .map(|(l, _)| Level { fact: format!("{prefix}{l}"), per_party: matches!(facts.get(l), Some(Fields::Named(f)) if f.len() > 1), set_by: setters(m, l) })
                .collect();
            Some(Row { fact: format!("{prefix}{name}"), per_party: fs.len() > 1, values, levels, default })
        })
        .collect()
}

fn last_type(f: &Fields) -> Option<&str> {
    match f {
        Fields::Named(fs) => fs.last().and_then(|f: &Field| named_type(&f.ty)),
        Fields::By(_) => None,
    }
}

/// A type named in its own module, by its name.
fn named_type(t: &Type) -> Option<&str> {
    match t {
        Type::Named(p, args) if p.0.len() == 1 && args.is_empty() => Some(p.last()),
        _ => None,
    }
}

/// The values M declares of type TY: each `+TY("word")`.
fn instances(m: &Module, ty: &str) -> Vec<String> {
    m.decls
        .iter()
        .filter_map(|d| match &d.what {
            DeclKind::Holds(Expr { e: E::Apply(p, args), .. }) if p.0.len() == 1 && p.last() == ty && args.len() == 1 => words(&args[0]),
            _ => None,
        })
        .collect()
}

fn words(e: &Expr) -> Option<String> {
    let E::Text(pieces) = &e.e else { return None };
    pieces.iter().map(|p| if let Piece::Words(w) = p { Some(w.as_str()) } else { None }).collect()
}

/// The facts LEAVES rule out: each `Not(Exists … : F(…))` among them.
fn ruled_out<'a>(leaves: &[&'a Expr]) -> Vec<&'a str> {
    leaves
        .iter()
        .filter_map(|x| match &x.e {
            E::Not(q) => match &q.e {
                E::Quant(Quant::Exists, _, body) => match &body.e {
                    E::Apply(p, _) if p.0.len() == 1 => Some(p.last()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn conjuncts(e: &Expr) -> Vec<&Expr> {
    match &e.e {
        E::Bin(Op::And, a, b) => conjuncts(a).into_iter().chain(conjuncts(b)).collect(),
        _ => vec![e],
    }
}

/// The variable E is, if it is one.
fn var(e: &Expr) -> Option<&str> {
    match &e.e {
        E::Var(v) => Some(v),
        E::Name(p) if p.0.len() == 1 => Some(p.last()),
        _ => None,
    }
}

/// The word VAR is compared with in E (`mode == "safe"`), either side.
fn literal_for(e: &Expr, v: &str) -> Option<String> {
    let E::Bin(Op::Eq, a, b) = &e.e else { return None };
    if var(a) == Some(v) { words(b) } else if var(b) == Some(v) { words(a) } else { None }
}

/// The acts in M whose `Creates` makes fact L.
fn setters(m: &Module, l: &str) -> Vec<Setter> {
    m.decls
        .iter()
        .filter_map(|d| {
            let DeclKind::Norm { kind: Norm::Act, name, fields, clauses } = &d.what else { return None };
            let made = clauses.iter().find_map(|c| match c {
                Clause::Creates(es) => es.iter().find_map(|x| match &x.inst.e {
                    E::Apply(p, args) if p.0.len() == 1 && p.last() == l => Some(args),
                    _ => None,
                }),
                _ => None,
            })?;
            Some(Setter { act: format!("{}.{name}", named(m)), fills: fills(fields, made) })
        })
        .collect()
}

/// An act's fields filled for the level it creates as MADE: the first is its actor (5.3), each other one of the
/// level's arguments, its party or its value; `None` when the actor is the level's own or an argument no field gives.
fn fills(fields: &Fields, made: &[Expr]) -> Option<Vec<Fill>> {
    let Fields::Named(fs) = fields else { return None };
    let given = |a: &Expr| var(a).is_some_and(|v| fs.iter().skip(1).any(|f| f.name == v));
    if !made.iter().all(given) {
        return None;
    }
    let at = |f: &str| made.iter().position(|a| var(a) == Some(f));
    fs.iter()
        .enumerate()
        .map(|(i, f)| match (i, at(f.name.as_str())) {
            (0, None) => Some(Fill::Actor),
            (0, Some(_)) | (_, None) => None,
            (_, Some(j)) if j + 1 == made.len() => Some(Fill::Value),
            (_, Some(_)) => Some(Fill::Party),
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{rows, Fill, Level, Row, Setter};

    /// A made-up package in the shape of a desk's modes: a top module importing it, and facts that are not levels.
    pub(crate) const DESK: &str = r#"module t.desk

import std.hooks as H

Fact Mode Identified by String
+Mode("safe"). +Mode("yolo").
Fact Agent-Mode(agent: H.Party, mode: Mode)
Fact Project-Mode(mode: Mode)
Fact Install-Mode(mode: Mode)

Fact Mode-Of(agent: H.Party, mode: Mode)
  Holds when Agent-Mode(agent, mode)
  Holds when H.Running-On(agent, h, v) && Project-Mode(mode) && Not(Exists m : Agent-Mode(agent, m))
  Holds when H.Running-On(agent, h, v) && Install-Mode(mode) && Not(Exists m : Agent-Mode(agent, m)) && Not(Exists m : Project-Mode(m))
  Holds when H.Running-On(agent, h, v) && Mode(mode) && mode == "safe" && Not(Exists m : Agent-Mode(agent, m))
             && Not(Exists m : Project-Mode(m)) && Not(Exists m : Install-Mode(m))

Act Set-Agent-Mode(owner: H.Party, agent: H.Party, mode: Mode)
  Terminates     Agent-Mode(agent, m) When Agent-Mode(agent, m)
  Creates        Agent-Mode(agent, mode)
Act Set-Project-Mode(owner: H.Party, mode: Mode)
  Terminates     Project-Mode(m) When Project-Mode(m)
  Creates        Project-Mode(mode)
Act Set-Own-Mode(agent: H.Party, mode: Mode)
  Creates        Agent-Mode(agent, mode)

Fact Thread Identified by String
Fact Title(thread: Thread, words: String)
Fact Unboxed(agent: H.Party, enabled: Bool)
  Holds when Mode-Of(agent, "yolo") && enabled == false
"#;

    pub(crate) const TOP: &str = "module t.top\n\nimport std.hooks as H\nimport t.desk as D\n";

    pub(crate) fn read(texts: &[&str]) -> Vec<docket_v2::syntax::tree::Module> {
        texts.iter().map(|t| docket_v2::syntax::read(t).expect("the made-up package reads")).collect()
    }

    /// The fallback is read from the clauses alone: three levels most general first, the default, the values and setters,
    /// each setter's fields filled by its actor and the level's arguments, and one whose actor is the level's party not filled.
    #[test]
    fn a_fact_that_falls_back_level_by_level_is_one_row_named_as_the_top_module_names_it() {
        let got = rows(&read(&[DESK, TOP]), "t.top");
        let level = |fact: &str, per_party, set_by: &[(&str, Option<&[Fill]>)]| Level {
            fact: fact.into(),
            per_party,
            set_by: set_by.iter().map(|(a, f)| Setter { act: a.to_string(), fills: f.map(<[Fill]>::to_vec) }).collect(),
        };
        let (actor, party, value) = (Fill::Actor, Fill::Party, Fill::Value);
        assert_eq!(
            got,
            vec![Row {
                fact: "D.Mode-Of".into(),
                per_party: true,
                values: vec!["safe".into(), "yolo".into()],
                levels: vec![
                    level("D.Install-Mode", false, &[]),
                    level("D.Project-Mode", false, &[("t.desk.Set-Project-Mode", Some(&[actor, value][..]))]),
                    level("D.Agent-Mode", true, &[("t.desk.Set-Agent-Mode", Some(&[actor, party, value][..])), ("t.desk.Set-Own-Mode", None)]),
                ],
                default: Some("safe".into()),
            }]
        );
    }

    /// The order comes from the guards, not from the order the clauses are written in.
    #[test]
    fn the_most_general_level_is_the_one_whose_clause_rules_out_the_others_whatever_their_order() {
        let text = r#"module t.flip

Fact Level Identified by String
+Level("low"). +Level("high").
Fact Mine(who: String, level: Level)
Fact Ours(level: Level)
Fact Level-Of(who: String, level: Level)
  Holds when Ours(level) && Not(Exists l : Mine(who, l))
  Holds when Mine(who, level)
"#;
        let got = rows(&read(&[text]), "t.flip");
        let levels: Vec<&str> = got.iter().flat_map(|r| r.levels.iter().map(|l| l.fact.as_str())).collect();
        assert_eq!(levels, ["Ours", "Mine"], "Ours rules Mine out, so it is the more general");
        assert_eq!(got[0].default, None, "no clause gives a word of its own");
    }

    /// A module the top does not import gives no row, and the top module itself is named with no prefix.
    #[test]
    fn a_module_the_top_cannot_name_gives_no_row_and_the_top_itself_needs_no_prefix() {
        assert!(rows(&read(&[DESK, "module t.top\n"]), "t.top").is_empty(), "not imported: nothing the engine could be asked");
        let own = rows(&read(&[DESK]), "t.desk");
        assert_eq!(own.iter().map(|r| r.fact.as_str()).collect::<Vec<_>>(), ["Mode-Of"]);
    }
}
