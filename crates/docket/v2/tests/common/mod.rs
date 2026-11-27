//! What the machine's tests share: a stand-in normative core, a stand-in driver, a machine on a small text.
// each test file uses a part of it
#![allow(dead_code)]

use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::machine::Machine;
use docket_v2::run::norms::{NormDecl, Norms, Step, Why};
use docket_v2::run::value::{Env, Instance, Value};
use docket_v2::syntax::tree::{E, Expr, Op, Path};
use std::collections::BTreeSet;

/// Stored facts and a table of acts: what each act creates, if its condition holds. Conditions it can read: one
/// atom `F(…)`, `Holds(atom)`, `Not(atom)`, `a && b`, comparisons of bound values; enough for these texts, and no more.
#[derive(Default)]
pub struct Facts {
    pub held: BTreeSet<Instance>,
    /// Σ after each committed step, the first as loaded; and the one read now, if a past one (11).
    pub log: Vec<BTreeSet<Instance>>,
    pub viewing: Option<usize>,
    /// What was read since the core was last asked, each read with its answer.
    pub read: Vec<String>,
    pub acts: Vec<(&'static str, fn(&Instance, &BTreeSet<Instance>) -> Result<(Vec<Instance>, Vec<Instance>), Why>)>,
}

pub fn inst(ty: &str, args: &[&str]) -> Instance {
    Instance { ty: ty.into(), args: args.iter().map(|a| Value::text(a)).collect() }
}

impl Facts {
    fn now(&self) -> &BTreeSet<Instance> {
        self.viewing.map_or(&self.held, |k| &self.log[k])
    }
    fn val(&self, e: &Expr, env: &Env) -> Option<Value> {
        match &e.e {
            E::Var(x) => env.get(x).cloned(),
            E::Text(ps) => match ps.as_slice() {
                [docket_v2::syntax::tree::Piece::Words(w)] => Some(Value::text(w)),
                _ => None,
            },
            E::Lit(docket_v2::syntax::tree::Lit::Int(n)) => Some(Value::Int(*n)),
            _ => None,
        }
    }
    fn ways(&self, c: &Expr, env: &Env) -> Vec<Env> {
        match &c.e {
            E::Apply(Path(p), args) => {
                let ty = &p[0];
                let mut out = Vec::new();
                for h in self.now().iter().filter(|h| h.ty == *ty && h.args.len() == args.len()) {
                    let mut e = env.clone();
                    let fits = args.iter().zip(&h.args).all(|(a, v)| match self.val(a, &e) {
                        Some(x) => x == *v,
                        None => match &a.e {
                            E::Var(n) => {
                                e.insert(n.clone(), v.clone());
                                true
                            }
                            _ => false,
                        },
                    });
                    if fits {
                        out.push(e);
                    }
                }
                out
            }
            E::Not(x) => {
                if self.ways(x, env).is_empty() {
                    vec![env.clone()]
                } else {
                    vec![]
                }
            }
            E::Holds(x) => self.ways(x, env),
            E::Bin(Op::And, a, b) => self.ways(a, env).into_iter().flat_map(|e| self.ways(b, &e)).collect(),
            E::Bin(op, a, b) => {
                let (x, y) = (self.val(a, env), self.val(b, env));
                let ok = match op {
                    Op::Eq => x == y,
                    Op::Ne => x != y,
                    Op::Gt => x > y,
                    _ => false,
                };
                if ok { vec![env.clone()] } else { vec![] }
            }
            _ => vec![],
        }
    }
}

impl Norms for Facts {
    fn load(&mut self, decls: &[NormDecl]) -> Result<(), String> {
        for d in decls {
            if let docket_v2::syntax::tree::DeclKind::Holds(Expr { e: E::Apply(p, args), .. }) = &d.decl.what {
                let args = args.iter().map(|a| self.val(a, &Env::new()).ok_or("a literal")).collect::<Result<_, _>>()?;
                self.held.insert(Instance { ty: p.to_string(), args });
            }
        }
        // a reload keeps Σ and its past
        if self.log.is_empty() {
            self.log = vec![self.held.clone()];
        }
        Ok(())
    }
    fn holds(&mut self, i: &Instance) -> bool {
        let h = self.now().contains(i);
        self.read.push(format!("{i} {h}"));
        h
    }
    fn cond(&mut self, c: &Expr, env: &Env) -> Result<bool, String> {
        let h = !self.ways(c, env).is_empty();
        self.read.push(format!("{c:?} {env:?} {h}"));
        Ok(h)
    }
    fn each(&mut self, c: &Expr, env: &Env) -> Result<Vec<Env>, String> {
        if matches!(&c.e, E::Apply(p, _) if p.to_string() == "t.Broken") {
            return Err("Broken cannot be evaluated".into());
        }
        Ok(self.ways(c, env))
    }
    fn step(&mut self, act: &Instance, _party: Option<&Value>) -> Result<Step, Why> {
        // an Act with no row of the table changes nothing and is let
        let (created, terminated) = match self.acts.iter().find(|(n, _)| *n == act.ty) {
            Some((_, f)) => f(act, self.now())?,
            None => (vec![], vec![]),
        };
        Ok(Step { act: act.clone(), created, terminated })
    }
    fn commit(&mut self, s: Step) {
        assert!(self.viewing.is_none(), "a commit while a past state is viewed");
        for t in s.terminated {
            self.held.remove(&t);
        }
        self.held.extend(s.created);
        self.log.push(self.held.clone());
    }
    fn view(&mut self, k: Option<usize>) {
        self.viewing = k;
    }
    fn reads(&mut self) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        if self.read.is_empty() {
            return [0; 32];
        }
        Sha256::digest(std::mem::take(&mut self.read).join("\n")).into()
    }
}

/// Answers from a table: each Effect it names, the answer, the costs; and how often it was asked.
pub struct Table {
    pub name: Vec<Value>,
    pub answers: Vec<(&'static str, Value, Vec<(String, i64)>)>,
    pub asked: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    pub late: Option<std::sync::mpsc::Sender<(u64, Reply)>>,
}

impl Driver for Table {
    fn name(&self) -> Vec<Value> {
        self.name.clone()
    }
    fn dropped(&mut self, line: u64) {
        self.asked.borrow_mut().push(format!("dropped {line}"));
    }
    fn events(&self) -> Vec<String> {
        vec!["t.Seen".to_string()]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        self.answers.iter().map(|(e, _, _)| (e.to_string(), vec!["tokens".to_string()])).collect()
    }
    fn carry(&mut self, op: &str, _args: &[Value], line: u64) -> Reply {
        self.asked.borrow_mut().push(op.to_string());
        let (_, v, c) = self.answers.iter().find(|(e, _, _)| *e == op).unwrap();
        match v {
            Value::Text(t) if t == "later" => {
                if let Some(late) = &self.late {
                    late.send((line, Reply::Answer { value: Value::Int(9), costs: c.clone(), events: vec![] })).unwrap();
                }
                Reply::Later
            }
            Value::Text(t) if t == "fails" => Reply::Failed("no such tool".into()),
            _ => Reply::Answer { value: v.clone(), costs: c.clone(), events: vec![] },
        }
    }
}

pub type Asked = std::rc::Rc<std::cell::RefCell<Vec<String>>>;

/// A machine on module `t` of `text`, with each of `programs` the `Does` of an Act `P1`, `P2`, … of it.
pub fn machine(text: &str, programs: &[&str], facts: Facts, answers: Vec<(&'static str, Value, Vec<(String, i64)>)>) -> (Machine<Facts>, Asked) {
    let l = loaded(text, programs);
    let asked: Asked = Default::default();
    let d = Table { name: vec![Value::text("stand-in"), Value::text("1")], answers, asked: asked.clone(), late: None };
    (Machine::new(l, facts, vec![Box::new(d)]).unwrap(), asked)
}

/// Module `t` of `text`, with each of `programs` the `Does` of an Act `P1`, `P2`, … of it, loaded.
pub fn loaded(text: &str, programs: &[&str]) -> docket_v2::check::names::Loaded {
    // a program written as a block is an Act's `Does`, with no result (5.2); one that gives a value says its type
    let acts: String = programs.iter().enumerate().map(|(k, p)| format!("\nAct P{}(by: String) {}{p}", k + 1, if p.starts_with('{') { "Does " } else { "" })).collect();
    docket_v2::check::load(vec![docket_v2::syntax::read(&format!("{text}{acts}")).unwrap()]).unwrap()
}

/// Run Act `Pk` as party `w1` (8.8), to its end; its record's lines as written.
pub fn run(m: &mut Machine<Facts>, k: usize) -> Vec<String> {
    let from = m.record.len();
    let r = m.start(Value::text("w1"), Some(0), "", Instance { ty: format!("t.P{k}"), args: vec![Value::text("w1")] }).unwrap();
    m.run();
    labelled(m, from, r, "r")
}

/// The lines from `from` on, run `r`'s name and its branches' written `label`.
pub fn labelled(m: &Machine<Facts>, from: usize, r: usize, label: &str) -> Vec<String> {
    let name = m.run_name(r);
    m.record[from..]
        .iter()
        .map(|l| {
            let s = l.to_string();
            let mut out = String::new();
            let mut rest = s.as_str();
            while let Some(at) = rest.find(name) {
                let before = rest[..at].chars().last();
                let after = rest[at + name.len()..].chars().next();
                out.push_str(&rest[..at]);
                let whole = !before.is_some_and(|c| c.is_alphanumeric()) && !after.is_some_and(|c| c.is_ascii_digit());
                out.push_str(if whole { label } else { name });
                rest = &rest[at + name.len()..];
            }
            out + rest
        })
        .collect()
}
