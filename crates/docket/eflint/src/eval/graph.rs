//! The derived types as a graph of what each one's rules read, each read marked monotone or not with where it is
//! written, cut into knots (Tarjan's strongly connected components) in the order to work them out, each with its
//! stratum.

use super::*;

/// What a read of a type goes through: one through `Not`, `Forall`, a count, `Violated` or `Enabled` can turn false
/// when more instances hold; a plain one (an application, `Present`, `Exists`) cannot.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Through {
    Plain,
    Not,
    Forall,
    Count,
    Sum,
    Max,
    Min,
    Violated,
    Enabled,
}

impl Through {
    /// Whether more instances of the type read can turn the reader false.
    pub fn falls(self) -> bool {
        self != Through::Plain
    }

    pub fn word(self) -> &'static str {
        match self {
            Through::Plain => "a plain read",
            Through::Not => "Not",
            Through::Forall => "Forall",
            Through::Count => "Count",
            Through::Sum => "Sum",
            Through::Max => "Max",
            Through::Min => "Min",
            Through::Violated => "Violated",
            Through::Enabled => "Enabled",
        }
    }

    fn of(q: Quant) -> Through {
        match q {
            Quant::Exists => Through::Plain,
            Quant::Forall => Through::Forall,
            Quant::Count => Through::Count,
            Quant::Sum => Through::Sum,
            Quant::Max => Through::Max,
            Quant::Min => Through::Min,
        }
    }
}

/// The part of a type a read is in.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Part {
    Rule,
    Condition,
    Constraint,
    Field,
}

/// READER's PART reads the derived type READ through THROUGH, written at AT (`file:line`) where the clause was read
/// with its place.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Read {
    pub reader: Id,
    pub read: Id,
    pub through: Through,
    pub part: Part,
    pub at: Option<String>,
}

impl Read {
    pub fn describe(&self) -> String {
        let part = match self.part {
            Part::Rule => "rule",
            Part::Condition => "Conditioned by",
            Part::Constraint => "domain constraint",
            Part::Field => "field",
        };
        let at = self.at.as_deref().map(|a| format!(" ({a})")).unwrap_or_default();
        format!("{}'s {part}{at} reads {} through {}", self.reader, self.read, self.through.word())
    }
}

/// Derived types whose rules read each other, as indexes into [`Graph::types`]; its stratum, one more than any knot
/// it reads through a read that can turn false; and those reads among its own members (none: it is stratified).
#[derive(Clone, PartialEq, Debug)]
pub struct Knot {
    pub members: Vec<usize>,
    pub stratum: usize,
    pub falling: Vec<Read>,
}

/// The reads among a specification's derived types, and its knots, each after every knot it reads.
#[derive(Clone, PartialEq, Debug)]
pub struct Graph {
    pub types: Vec<Id>,
    pub reads: Vec<Read>,
    pub knots: Vec<Knot>,
}

/// The graph of every derived type of SPEC, tests aside.
pub fn polarity(spec: &Spec) -> Rc<Graph> {
    known(spec).1
}

/// Every derived type of SPEC, tests aside, and their graph: made once for the text and kept on it, with the
/// declarations it was made for, each by pointer, and the tests and aliases; made again when they are not the same.
pub fn known(spec: &Spec) -> (Rc<Vec<(Id, Rc<TypeSpec>)>>, Rc<Graph>) {
    let mut kept = spec.graph.0.borrow_mut();
    if let Some(k) = kept.as_ref().filter(|k| k.fits(spec)) {
        return (k.types.clone(), k.graph.clone());
    }
    let types: Vec<(Id, Rc<TypeSpec>)> = spec
        .decls
        .iter()
        .filter(|(d, t)| !t.derivation.is_empty() && !spec.tests.contains(*d))
        .map(|(d, t)| (d.clone(), t.clone()))
        .collect();
    let g = Rc::new(graph(spec, &types));
    let decls = spec.decls.iter().map(|(d, t)| (d.clone(), t.clone())).collect();
    let k = Kept { decls, tests: spec.tests.clone(), aliases: spec.aliases.clone(), types: Rc::new(types), graph: g };
    let out = (k.types.clone(), k.graph.clone());
    *kept = Some(k);
    out
}

thread_local! {
    static BUILT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many graphs this thread has made: a step on a text already worked out makes none.
pub fn graphs_built() -> usize {
    BUILT.with(|b| b.get())
}

/// A specification's graph as [`known`] keeps it; two specifications compare equal whatever they keep.
#[derive(Clone, Default)]
pub struct KnownGraph(std::cell::RefCell<Option<Kept>>);

impl PartialEq for KnownGraph {
    fn eq(&self, _: &KnownGraph) -> bool {
        true
    }
}

#[derive(Clone)]
struct Kept {
    decls: Vec<(Id, Rc<TypeSpec>)>,
    tests: BTreeSet<Id>,
    aliases: BTreeMap<Id, Id>,
    types: Rc<Vec<(Id, Rc<TypeSpec>)>>,
    graph: Rc<Graph>,
}

impl Kept {
    fn fits(&self, spec: &Spec) -> bool {
        self.decls.len() == spec.decls.len()
            && self.decls.iter().zip(&spec.decls).all(|((d, t), (e, u))| d == e && Rc::ptr_eq(t, u))
            && (&self.tests, &self.aliases) == (&spec.tests, &spec.aliases)
    }
}

/// The graph of the derived types TYPES: what working each out reads (its rules, conditions, domain constraint and
/// fields), with `Violated` and `Enabled` followed into the violation conditions and conditions of the type named.
pub fn graph(spec: &Spec, types: &[(Id, Rc<TypeSpec>)]) -> Graph {
    BUILT.with(|b| b.set(b.get() + 1));
    let index: BTreeMap<Id, usize> = types.iter().enumerate().map(|(i, (d, _))| (d.clone(), i)).collect();
    let mut reads = Vec::new();
    for (d, t) in types {
        let mut add = |term: &Term, vars: &[Var], part: Part| {
            let at = spec.place(d, term);
            let mut got: Vec<(Id, Through)> = vars.iter().map(|v| (v.dom.clone(), Through::Plain)).collect();
            walk(spec, &mut BTreeSet::new(), term, Through::Plain, &mut got);
            for (r, through) in got {
                let read = spec.chase(&r);
                if index.contains_key(&read) {
                    reads.push(Read { reader: d.clone(), read, through, part, at: at.clone() });
                }
            }
        };
        for c in &t.derivation {
            match c {
                Derivation::HoldsWhen(x) => add(x, &[], Part::Rule),
                Derivation::Dv(xs, x) => add(x, xs, Part::Rule),
            }
        }
        t.conditions.iter().for_each(|c| add(c, &[], Part::Condition));
        add(&t.domain_constraint, &[], Part::Constraint);
        for v in Spec::params(d, t) {
            let read = spec.chase(&v.dom);
            if index.contains_key(&read) {
                reads.push(Read { reader: d.clone(), read, through: Through::Plain, part: Part::Field, at: None });
            }
        }
    }
    reads.sort();
    reads.dedup();
    let mut adj = vec![Vec::new(); types.len()];
    for r in &reads {
        adj[index[&r.reader]].push(index[&r.read]);
    }
    let mut t = Tarjan { adj: &adj, index: vec![None; types.len()], low: vec![0; types.len()], on: vec![false; types.len()], stack: Vec::new(), next: 0, out: Vec::new() };
    for v in 0..types.len() {
        if t.index[v].is_none() {
            t.visit(v);
        }
    }
    let comps = t.out;
    let mut knot_of = vec![0; types.len()];
    for (k, c) in comps.iter().enumerate() {
        c.iter().for_each(|&v| knot_of[v] = k);
    }
    let mut knots: Vec<Knot> = Vec::with_capacity(comps.len());
    for (k, members) in comps.into_iter().enumerate() {
        let mut stratum = 0;
        let mut falling = Vec::new();
        for r in reads.iter().filter(|r| knot_of[index[&r.reader]] == k) {
            let other = knot_of[index[&r.read]];
            if other == k {
                if r.through.falls() {
                    falling.push(r.clone());
                }
            } else {
                stratum = stratum.max(knots[other].stratum + usize::from(r.through.falls()));
            }
        }
        knots.push(Knot { members, stratum, falling });
    }
    Graph { types: types.iter().map(|(d, _)| d.clone()).collect(), reads, knots }
}

/// The types T reads, each through the outermost read that can turn false around it, else THROUGH.
fn walk(spec: &Spec, seen: &mut BTreeSet<(Id, bool)>, t: &Term, through: Through, out: &mut Vec<(Id, Through)>) {
    let inner = |w: Through| if through.falls() { through } else { w };
    match t {
        Term::BoolLit(_) | Term::IntLit(_) | Term::StringLit(_) | Term::CurrentTime => {}
        Term::Ref(v) => out.push((v.dom.clone(), through)),
        Term::Bin(_, l, r) => {
            walk(spec, seen, l, through, out);
            walk(spec, seen, r, through, out);
        }
        Term::Un(UnOp::Not, x) => walk(spec, seen, x, inner(Through::Not), out),
        Term::Un(op @ (UnOp::Violated | UnOp::Enabled), x) => {
            let w = inner(if *op == UnOp::Violated { Through::Violated } else { Through::Enabled });
            walk(spec, seen, x, w, out);
            let mut named = BTreeSet::new();
            mentions(x, &mut named);
            // each type named followed once into what decides it, so a type that names itself ends
            for d in named {
                let Some(t) = spec.find(&d) else { continue };
                if !seen.insert((d.clone(), *op == UnOp::Violated)) {
                    continue;
                }
                let followed: Vec<Term> = match (&t.kind, op) {
                    (Kind::Duty { violated_when, .. }, UnOp::Violated) => violated_when.clone(),
                    (Kind::Act { syncs, .. } | Kind::Event { syncs, .. }, UnOp::Enabled) => {
                        out.extend(syncs.iter().flat_map(|s| s.vars.iter().map(|v| (v.dom.clone(), w))));
                        let synced = syncs.iter().map(|s| Term::Un(UnOp::Enabled, Box::new(s.term.clone())));
                        t.conditions.iter().cloned().chain(synced).collect()
                    }
                    (_, UnOp::Enabled) => t.conditions.clone(),
                    _ => Vec::new(),
                };
                followed.iter().for_each(|c| walk(spec, seen, c, w, out));
            }
        }
        Term::Un(_, x) => walk(spec, seen, x, through, out),
        Term::Tag(x, d) => {
            out.push((d.clone(), through));
            walk(spec, seen, x, through, out);
        }
        Term::Project(x, v) => {
            out.push((v.dom.clone(), through));
            walk(spec, seen, x, through, out);
        }
        Term::Q(q, xs, x) => {
            let w = inner(Through::of(*q));
            out.extend(xs.iter().map(|v| (v.dom.clone(), w)));
            walk(spec, seen, x, w, out);
        }
        Term::App(d, args) => {
            out.push((d.clone(), through));
            // a variable given as a field is a value already bound, not a read of its type
            let given: Vec<&Term> = match args {
                Args::Pos(ts) => ts.iter().collect(),
                Args::Mods(ms) => ms.iter().map(|(_, a)| a).collect(),
            };
            for a in given.into_iter().filter(|a| !matches!(a, Term::Ref(_))) {
                walk(spec, seen, a, through, out);
            }
        }
    }
}

/// A knot with instances the well-founded model leaves unknown: its types, the reads that close its cycle (none: it
/// only reads an instance another knot left unknown), and those instances.
#[derive(Clone, PartialEq, Debug)]
pub struct Undecided {
    pub types: Vec<Id>,
    pub cycle: Vec<Read>,
    pub unknown: Vec<Tagged>,
}

impl Undecided {
    pub fn message(&self) -> String {
        let types = self.types.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(", ");
        let why = match self.cycle.is_empty() {
            true => "read an instance left unknown".to_string(),
            false => format!(
                "read each other in a cycle through a read that can turn false ({})",
                self.cycle.iter().map(Read::describe).collect::<Vec<_>>().join("; ")
            ),
        };
        let open = self.unknown.iter().map(|te| te.to_string()).collect::<Vec<_>>().join(", ");
        format!("unknown: [{types}] {why}; neither holding nor failing: {open}")
    }
}

/// What [`undecided`] says, each knot's message joined: the why of a refusal on an unknown instance.
pub fn undecided_why(spec: &Spec, state: &State) -> String {
    let all: Vec<String> = undecided(spec, state).iter().map(Undecided::message).collect();
    format!("undecided, {}", all.join("; "))
}

/// The knots of SPEC with instances STATE leaves unknown.
pub fn undecided(spec: &Spec, state: &State) -> Vec<Undecided> {
    if state.unknown().is_empty() {
        return Vec::new();
    }
    let g = polarity(spec);
    let mut out = Vec::new();
    for k in &g.knots {
        let types: Vec<Id> = k.members.iter().map(|&i| g.types[i].clone()).collect();
        let unknown: Vec<Tagged> = state.unknown().iter().filter(|te| types.contains(&te.dom)).cloned().collect();
        if !unknown.is_empty() {
            out.push(Undecided { types, cycle: k.falling.clone(), unknown });
        }
    }
    out
}

/// Whether T holds on STATE, `None` where it holds at the upper bound of the truth (unknown instances holding) and
/// not at its lower one (none holding): Kleene's third value.
pub fn truth(spec: &Spec, state: &State, t: &Term) -> R<Option<bool>> {
    let all = |vs: Vec<Value>| vs.iter().all(|v| *v == Value::Bool(true));
    let sure = all(Ev::new(spec, state).eval(t)?);
    if state.unknown().is_empty() {
        return Ok(Some(sure));
    }
    let may = all(Ev::upper(spec, state).eval(t)?);
    Ok(if sure { Some(true) } else if may { None } else { Some(false) })
}

/// Tarjan's strongly connected components of a graph given by its edges from each node, each component sorted and
/// given after every component its nodes reach.
struct Tarjan<'g> {
    adj: &'g [Vec<usize>],
    index: Vec<Option<usize>>,
    low: Vec<usize>,
    on: Vec<bool>,
    stack: Vec<usize>,
    next: usize,
    out: Vec<Vec<usize>>,
}

impl Tarjan<'_> {
    fn visit(&mut self, v: usize) {
        self.index[v] = Some(self.next);
        self.low[v] = self.next;
        self.next += 1;
        self.stack.push(v);
        self.on[v] = true;
        let adj = self.adj;
        for &w in &adj[v] {
            match self.index[w] {
                None => {
                    self.visit(w);
                    self.low[v] = self.low[v].min(self.low[w]);
                }
                Some(i) if self.on[w] => self.low[v] = self.low[v].min(i),
                Some(_) => {}
            }
        }
        if Some(self.low[v]) == self.index[v] {
            let mut comp = Vec::new();
            while let Some(w) = self.stack.pop() {
                self.on[w] = false;
                comp.push(w);
                if w == v {
                    break;
                }
            }
            comp.sort_unstable();
            self.out.push(comp);
        }
    }
}
