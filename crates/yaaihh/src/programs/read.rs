//! The harness's programs as the Docket draws them (N-1988), read from the texts' syntax: what starts each rule (a
//! harness moment, an Event, an Act or an Effect performed), the rules (each reaction and each Duty, by module), and
//! what each does (an Effect performed, an Act done, an answer to a moment and the power it needs).

use docket_v2::syntax::tree::{Clause, DeclKind, Expr, Module, Norm, Pat, Piece, E};
use std::collections::{BTreeMap, BTreeSet};

/// The Effect every harness moment is (std.hooks): a reaction to it with its moment written starts at that moment.
pub const HOOK: &str = "std.hooks.Hook";
/// What a bridge lets at a moment (std.hooks): the engine's check reads it and nothing else.
pub const LETS: &str = "std.hooks.Lets";
/// A harness moment, as std.hooks states each.
pub const MOMENT: &str = "std.hooks.Moment";

/// The words of a text literal with no holes.
pub fn text_of(e: &Expr) -> Option<String> {
    match &e.e {
        E::Text(ps) => match ps.as_slice() {
            [Piece::Words(w)] => Some(w.clone()),
            [] => Some(String::new()),
            _ => None,
        },
        _ => None,
    }
}

/// What a thing at a side of the drawing is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Is {
    Moment,
    Event,
    Act,
    Effect,
    /// An answer to a moment (a constructor of std.hooks' HookAction), with the power it needs.
    Answer,
}

/// A thing at a side: its key (a full name, or `moment:NAME`), its words, where it is declared (its module and line;
/// line 0 where no module declares it), the power an answer needs.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub key: String,
    pub is: Is,
    pub words: String,
    pub module: String,
    pub line: u32,
    pub needs: Option<String>,
}

/// A rule: a reaction (`on`) or a Duty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    On,
    Duty,
}

/// A rule's row: its module and line, its words, the moment a reaction to a harness moment names, the keys of what
/// starts it and of what it does, and the powers it tests std.hooks' `Lets` for.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub module: String,
    pub line: u32,
    pub rule: Rule,
    pub words: String,
    /// A Duty's full name, or what a reaction reacts to.
    pub name: String,
    pub moment: Option<String>,
    pub from: Vec<String>,
    pub to: Vec<String>,
    pub tests: Vec<String>,
}

/// A module's own fact as written (`+F("a", "b").`): its module and line, F's full name, its texts.
#[derive(Clone, Debug, PartialEq)]
pub struct Stated {
    pub module: String,
    pub line: u32,
    pub fact: String,
    pub args: Vec<String>,
}

/// The drawing: what starts the rules, the rules in load order, what they do.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawing {
    pub from: Vec<Node>,
    pub rows: Vec<Row>,
    pub to: Vec<Node>,
}

/// What a declaration is, as the drawing reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Sym<'a> {
    Norm(Norm),
    Effect,
    Fn(&'a Expr),
    Ctor(Option<&'a str>),
    Other,
}

/// Every declaration of the modules by full name, with its module and line, and how each module names the others.
struct Names<'a> {
    by: BTreeMap<String, (Sym<'a>, &'a str, u32)>,
    alias: BTreeMap<&'a str, BTreeMap<String, String>>,
}

/// A module's name as written (`std.hooks`), empty for one that names none.
pub fn name_of(m: &Module) -> String {
    m.name.as_ref().map(ToString::to_string).unwrap_or_default()
}

impl<'a> Names<'a> {
    fn of(mods: &'a [(String, Module)]) -> Names<'a> {
        let (mut by, mut alias) = (BTreeMap::new(), BTreeMap::new());
        for (name, m) in mods {
            let a: &mut BTreeMap<String, String> = alias.entry(name.as_str()).or_default();
            for i in &m.imports {
                a.insert(i.alias.clone().unwrap_or_else(|| i.path.to_string()), i.path.to_string());
            }
            for d in &m.decls {
                let sym = match &d.what {
                    DeclKind::Norm { kind, .. } => Sym::Norm(*kind),
                    DeclKind::Effect { .. } => Sym::Effect,
                    DeclKind::Fn { body, .. } => Sym::Fn(body),
                    DeclKind::Sum { variants, .. } => {
                        for v in variants {
                            by.insert(format!("{name}.{}", v.name), (Sym::Ctor(v.needs.as_deref()), name.as_str(), d.at.line));
                        }
                        Sym::Other
                    }
                    _ => Sym::Other,
                };
                if let Some(n) = d.what.name() {
                    by.insert(format!("{name}.{n}"), (sym, name.as_str(), d.at.line));
                }
            }
        }
        Names { by, alias }
    }

    /// The full name PATH stands for in module M, when it is a declaration of the modules.
    fn full(&self, m: &str, path: &[String]) -> Option<String> {
        let (last, head) = path.split_last()?;
        let module = if head.is_empty() {
            m.to_string()
        } else {
            let h = head.join(".");
            self.alias.get(m).and_then(|a| a.get(&h)).cloned().unwrap_or(h)
        };
        let q = format!("{module}.{last}");
        self.by.contains_key(&q).then_some(q)
    }

    fn sym(&self, q: &str) -> Sym<'a> {
        self.by.get(q).map_or(Sym::Other, |s| s.0)
    }

    /// What a program written in module M does, the functions it calls followed: the Effects it performs (but the
    /// moment itself, which an arm that watches performs again), the Acts it does, and, in its own text (TOP), the
    /// answers that need a power: a function such as std.hooks' `combine` builds every answer it is given.
    fn does(&self, m: &str, e: &Expr, top: bool, seen: &mut BTreeSet<String>, out: &mut Vec<String>) {
        each(e, &mut |x| {
            let (path, performed) = match &x.e {
                E::Perform(p, _) => (p, true),
                E::Apply(p, _) | E::Name(p) => (p, false),
                _ => return,
            };
            let Some(q) = self.full(m, &path.0) else { return };
            let key = match self.sym(&q) {
                Sym::Effect if performed && q != HOOK => q,
                Sym::Norm(Norm::Act) => q,
                Sym::Ctor(Some(_)) if top => q,
                Sym::Fn(body) if seen.insert(q.clone()) => {
                    let fm = self.by[&q].1;
                    self.does(fm, body, false, seen, out);
                    return;
                }
                _ => return,
            };
            if !out.contains(&key) {
                out.push(key);
            }
        });
    }

    /// The powers condition E of module M tests std.hooks' `Lets` for, as written there.
    fn tests(&self, m: &str, e: &Expr) -> Vec<String> {
        let mut out = Vec::new();
        each(e, &mut |x| {
            if let E::Apply(p, args) = &x.e {
                if self.full(m, &p.0).as_deref() == Some(LETS) {
                    out.extend(args.get(3).and_then(text_of));
                }
            }
        });
        out
    }

    /// The facts and duties condition E of module M names, in the order written.
    fn reads(&self, m: &str, e: &Expr, out: &mut Vec<String>) {
        each(e, &mut |x| {
            if let E::Apply(p, _) | E::Name(p) = &x.e {
                if let Some(q) = self.full(m, &p.0).filter(|q| matches!(self.sym(q), Sym::Norm(Norm::Fact | Norm::Duty))) {
                    if !out.contains(&q) {
                        out.push(q);
                    }
                }
            }
        });
    }
}

/// Every expression in E, E first.
pub fn each<'e>(e: &'e Expr, f: &mut impl FnMut(&'e Expr)) {
    f(e);
    let mut go = |x: &'e Expr| each(x, &mut *f);
    match &e.e {
        E::Lit(_) | E::Name(_) | E::Var(_) | E::SelfParty | E::This | E::Gap => {}
        E::Text(ps) => ps.iter().for_each(|p| if let Piece::Hole(x) = p { go(x) }),
        E::Apply(_, xs) | E::Perform(_, xs) | E::Tuple(xs) | E::List(xs) | E::Par(xs) => xs.iter().for_each(go),
        E::Field(x, _) | E::Neg(x) | E::Not(x) | E::Holds(x) | E::Violated(x) | E::Enabled(x) | E::Quant(_, _, x) | E::Agg(_, x) | E::Return(x) | E::Resume(_, x) | E::Do(x) => go(x),
        E::Bin(_, a, b) | E::Bind(_, a, b) | E::Seq(a, b) | E::ParEach(_, a, _, b) | E::Iterate(_, a, _, b) => {
            go(a);
            go(b);
        }
        E::Foreach(_, a, b) => {
            go(a);
            if let Some(b) = b {
                go(b);
            }
        }
        E::If(c, a, b) => {
            go(c);
            go(a);
            if let Some(b) = b {
                go(b);
            }
        }
        E::Handle(x, ret, arms) => {
            go(x);
            if let Some((_, r)) = ret {
                go(r);
            }
            for a in arms {
                a.cond.iter().for_each(&mut go);
                go(&a.body);
            }
        }
        E::Match(x, cases) => {
            go(x);
            for c in cases {
                c.cond.iter().for_each(&mut go);
                go(&c.body);
            }
        }
    }
}

/// The last part of a full name: `std.hooks.Hook` is `Hook`.
pub fn short(q: &str) -> &str {
    q.rsplit('.').next().unwrap_or(q)
}

/// Each of MODS' own facts whose arguments are all texts with no holes, in the order written.
pub fn stated(mods: &[(String, Module)]) -> Vec<Stated> {
    stated_in(&Names::of(mods), mods)
}

fn stated_in(names: &Names<'_>, mods: &[(String, Module)]) -> Vec<Stated> {
    let mut out = Vec::new();
    for (m, module) in mods {
        for d in &module.decls {
            let DeclKind::Holds(Expr { e: E::Apply(p, args), .. }) = &d.what else { continue };
            let (Some(fact), Some(args)) = (names.full(m, &p.0), args.iter().map(text_of).collect::<Option<Vec<_>>>()) else { continue };
            out.push(Stated { module: m.clone(), line: d.at.line, fact, args });
        }
    }
    out
}

/// The drawing of MODS, each a module with its name, in the order the engine loaded them.
pub fn drawing(mods: &[(String, Module)]) -> Drawing {
    let names = Names::of(mods);
    let mut d = Drawing::default();
    // what each Event and Act makes or ends, and what each derived fact reads: a Duty starts from what makes the
    // facts it reads, through the facts they are derived from
    let (mut makes, mut derived): (BTreeMap<String, Vec<String>>, BTreeMap<String, Vec<String>>) = Default::default();
    let moments: Vec<(String, u32)> = stated_in(&names, mods).into_iter().filter(|s| s.fact == MOMENT).filter_map(|s| Some((s.args.first()?.clone(), s.line))).collect();
    for (m, module) in mods {
        for decl in &module.decls {
            let DeclKind::Norm { kind, name, clauses, .. } = &decl.what else { continue };
            let q = format!("{m}.{name}");
            for c in clauses {
                match (kind, c) {
                    (Norm::Event | Norm::Act, Clause::Creates(es) | Clause::Terminates(es)) => {
                        for e in es {
                            let mut fs = Vec::new();
                            names.reads(m, &e.inst, &mut fs);
                            for f in fs {
                                makes.entry(f).or_default().push(q.clone());
                            }
                        }
                    }
                    (Norm::Fact, Clause::HoldsWhen(e)) => names.reads(m, e, derived.entry(q.clone()).or_default()),
                    _ => {}
                }
            }
        }
    }
    let starts = |reads: &[String]| {
        let (mut out, mut seen, mut todo): (Vec<String>, BTreeSet<String>, Vec<String>) = (Vec::new(), BTreeSet::new(), reads.to_vec());
        while let Some(f) = todo.pop() {
            if !seen.insert(f.clone()) {
                continue;
            }
            for a in makes.get(&f).into_iter().flatten() {
                if !out.contains(a) {
                    out.push(a.clone());
                }
            }
            todo.extend(derived.get(&f).into_iter().flatten().cloned());
        }
        out
    };
    for (m, module) in mods {
        for decl in &module.decls {
            let line = decl.at.line;
            match &decl.what {
                DeclKind::On { target, pats, cond, body } => {
                    let Some(t) = names.full(m, &target.0) else { continue };
                    let moment = (t == HOOK).then(|| match pats.get(1) {
                        Some(Pat::Text(w)) => w.clone(),
                        _ => "*".into(),
                    });
                    let mut tests = cond.as_ref().map(|c| names.tests(m, c)).unwrap_or_default();
                    // a test in the body guards what stands under it as one in the `where` does (spec 12)
                    each(body, &mut |x| {
                        if let E::If(c, _, _) = &x.e {
                            tests.extend(names.tests(m, c));
                        }
                    });
                    let mut to = Vec::new();
                    names.does(m, body, true, &mut BTreeSet::new(), &mut to);
                    let from = vec![moment.as_ref().map_or(t.clone(), |w| format!("moment:{w}"))];
                    let words = moment.as_ref().map_or_else(|| format!("on {}", short(&t)), |w| format!("on Hook · {}", if w == "*" { "any moment" } else { w.as_str() }));
                    d.rows.push(Row { module: m.clone(), line, rule: Rule::On, words, name: t, moment, from, to, tests });
                }
                DeclKind::Norm { kind: Norm::Duty, name, clauses, .. } => {
                    let mut reads = Vec::new();
                    for c in clauses {
                        if let Clause::HoldsWhen(e) | Clause::ViolatedWhen(e) = c {
                            names.reads(m, e, &mut reads);
                        }
                    }
                    let q = format!("{m}.{name}");
                    reads.retain(|r| *r != q);
                    let from = starts(&reads);
                    d.rows.push(Row { module: m.clone(), line, rule: Rule::Duty, words: format!("Duty {name}"), name: q, moment: None, from, to: Vec::new(), tests: Vec::new() });
                }
                _ => {}
            }
        }
    }
    // the sides: the moments in the order std.hooks declares them, then what starts a rule in the order first named;
    // what the rules do in that order, the answers last
    let hooks = mods.iter().find(|(m, _)| m == "std.hooks").map_or("", |(m, _)| m.as_str());
    let node = |key: &str| -> Node {
        if let Some(w) = key.strip_prefix("moment:") {
            let line = moments.iter().find(|(m, _)| m == w).map_or(0, |(_, l)| *l);
            let words = if w == "*" { "any moment".to_string() } else { w.to_string() };
            return Node { key: key.into(), is: Is::Moment, words, module: hooks.into(), line, needs: None };
        }
        let (sym, module, line) = names.by.get(key).map_or((Sym::Other, "", 0), |s| *s);
        let (is, needs) = match sym {
            Sym::Norm(Norm::Event) => (Is::Event, None),
            Sym::Norm(Norm::Act) => (Is::Act, None),
            Sym::Ctor(n) => (Is::Answer, n.map(str::to_string)),
            _ => (Is::Effect, None),
        };
        let words = match is {
            Is::Answer => format!("answers {}", short(key)),
            _ => short(key).to_string(),
        };
        Node { key: key.into(), is, words, module: module.into(), line, needs }
    };
    let mut from: Vec<String> = moments.iter().map(|(m, _)| format!("moment:{m}")).filter(|k| d.rows.iter().any(|r| r.from.contains(k))).collect();
    let mut to: Vec<String> = Vec::new();
    for r in &d.rows {
        from.extend(r.from.iter().filter(|k| !from.contains(*k)).cloned().collect::<Vec<_>>());
        to.extend(r.to.iter().filter(|k| !to.contains(*k)).cloned().collect::<Vec<_>>());
    }
    d.from = from.iter().map(|k| node(k)).collect();
    let mut to: Vec<Node> = to.iter().map(|k| node(k)).collect();
    to.sort_by_key(|n| n.is == Is::Answer);
    d.to = to;
    d
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The texts the release's engine loads, built into the window, each with its module.
    pub(crate) fn std_modules() -> Vec<(String, Module)> {
        modules(&super::super::BUILT_IN.map(|(_, t)| t))
    }

    pub(crate) fn modules(texts: &[&str]) -> Vec<(String, Module)> {
        texts.iter().map(|t| docket_v2::syntax::read(t).unwrap()).map(|m| (name_of(&m), m)).collect()
    }

    /// A reaction starts at the moment it names and goes to what its body performs, does and answers, the functions it
    /// calls followed; a Duty starts at the Events that make the facts it reads, through a derived fact.
    #[test]
    fn a_reaction_starts_at_its_moment_and_a_duty_at_what_makes_its_facts() {
        let hooks = "module std.hooks\nFact Party Identified by String\nFact Moment Identified by String\n+Moment(\"turn_begins\").\n+Moment(\"turn_end\").\n\
                     type HookAction =\n  | Allow\n  | Inject(text: String)  Needs \"inject\"\n\
                     Effect Hook(agent: Party, moment: Moment, payload: String) -> HookAction Does { return Allow }\n\
                     fn combine(a: HookAction, b: HookAction) -> HookAction {\n  match (a, b) {\n    | (Inject(x), _) => return Inject(x)\n    | _ => return b\n  }\n}";
        let w = "module w\nimport std.hooks as H\nFact Unheard(who: H.Party)\nFact Late(who: H.Party)\n  Holds when Unheard(who)\n\
                 Event Said(who: H.Party)\n  Creates Unheard(who)\nAct Hear(who: H.Party)\nEffect TextOf(id: String) -> String\n\
                 fn news(a: H.Party) -> Program<String> { t <- perform TextOf(\"n\"); Hear(a); return t }\n\
                 on H.Hook(agent, \"turn_begins\", p) where Unheard(agent) do { t <- do news(agent); return H.combine(H.Inject(t), H.Allow) }\n\
                 Duty Answer(holder: H.Party, claimant: H.Party)\n  Holds when Late(holder)";
        let d = drawing(&modules(&[hooks, w]));
        let on = &d.rows[0];
        assert_eq!((on.words.as_str(), on.from.as_slice()), ("on Hook · turn_begins", &["moment:turn_begins".to_string()][..]));
        assert_eq!(on.to, ["w.TextOf", "w.Hear", "std.hooks.Inject"]);
        let duty = &d.rows[1];
        assert_eq!((duty.rule, duty.from.as_slice()), (Rule::Duty, &["w.Said".to_string()][..]), "through Late, derived from Unheard, which Said makes");
        // the moments no rule names are not drawn; the answer is last, with its power
        let keys = |ns: &[Node]| ns.iter().map(|n| (n.key.clone(), n.is)).collect::<Vec<_>>();
        assert_eq!(keys(&d.from), [("moment:turn_begins".into(), Is::Moment), ("w.Said".into(), Is::Event)]);
        assert_eq!(keys(&d.to), [("w.TextOf".into(), Is::Effect), ("w.Hear".into(), Is::Act), ("std.hooks.Inject".into(), Is::Answer)]);
        assert_eq!((d.to[2].words.as_str(), d.to[2].needs.as_deref(), d.from[0].line), ("answers Inject", Some("inject"), 4));
    }

    /// Every reaction and Duty of the release's texts is a row, and each starts somewhere.
    #[test]
    fn the_release_texts_draw_every_reaction_and_duty() {
        let mods = std_modules();
        let d = drawing(&mods);
        let count = |pre: &str| mods.iter().map(|(_, m)| m.decls.iter().filter(|x| matches!((&x.what, pre), (DeclKind::On { .. }, "on") | (DeclKind::Norm { kind: Norm::Duty, .. }, "duty"))).count()).sum::<usize>();
        assert_eq!(d.rows.iter().filter(|r| r.rule == Rule::On).count(), count("on"));
        assert_eq!(d.rows.iter().filter(|r| r.rule == Rule::Duty).count(), count("duty"));
        assert!(d.rows.iter().filter(|r| r.rule == Rule::On).all(|r| !r.from.is_empty()));
        assert!(d.from.iter().any(|n| n.key == "moment:turn_begins") && d.to.iter().any(|n| n.is == Is::Answer && n.needs.as_deref() == Some("inject")), "{:?}", d.to);
    }
}
