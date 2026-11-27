//! What the record's own lines have shown, for the steps' `on`s (`syntax::On`; N-1511 p43; paper-research N-1446
//! p2139, p2152, p2162): the lines of the system events a step given after them is answered by, and the facts a
//! `where` reads (`syntax::fact_sig`), each from the lines alone, so a replay gives the same answers and the machine
//! runs no git.

use super::{Env, MERGED, eval};
use crate::line::{At, Line};
use crate::syntax::Expr;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// The record's lines as the steps' `on`s read them (`Machine::on_answer`), kept as each is carried out
/// (`Machine::line_answers`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Seen {
    /// Each `job_ended` line, by its tree, in the record's order: a step given after its tree's job ended finds that
    /// end, whatever other jobs ended since (p2162).
    pub ends: BTreeMap<String, Vec<(At, Value)>>,
    /// Each `cut` line, by its commit's first 8.
    pub cuts: BTreeMap<String, (At, Value)>,
    /// Each `merged` line ([`MERGED`]), by the first 8 of the tip it brought its branch to.
    pub merges: BTreeMap<String, (At, Value)>,
    /// Each train branch's tip as its last `merged` line says, by `REPO/BRANCH`: what coordd's git round compares.
    pub tips: BTreeMap<String, String>,
    /// The last `installed` line: what a coord runs now, a state as his window's release is.
    pub installed: Option<(At, Value)>,
    /// Each commit a `merged` line listed → its parents, each by its first 8: what `reaches` walks.
    pub parents: BTreeMap<String, Vec<String>>,
    /// Each commit's tree, by the commit's first 8: a `merged` line's, a cut's, and a job's run on the commit unchanged
    /// (`changed` 0; a job on changes runs another tree on the same commit).
    pub trees: BTreeMap<String, String>,
    /// The jobs started and not ended, by id → (its worktree, its tree's first 8). A start in a worktree ends the one
    /// before it there, whose end the record never had (a job per worktree at a time, the runner's rule).
    pub running: BTreeMap<String, (String, String)>,
}

/// The first 8 of commit or tree C, as `graph_git::commit_id` names a commit, without its `commit:`.
pub fn short(c: &str) -> String {
    c.strip_prefix("commit:").unwrap_or(c).chars().take(8).collect()
}

impl Seen {
    /// LINE kept, read at AT; EV its system event and value when it is one (`Machine::event_of`). Whether it is one
    /// the facts are read from: a job's start or end, a cut, a merged line.
    pub fn keep(&mut self, at: &At, line: &Line, ev: Option<(&str, &Value)>) -> bool {
        let mut facts = false;
        if let Line::Act { is, more, .. } = line {
            let f = |k: &str| more.get(k).and_then(Value::as_str).unwrap_or_default();
            let unchanged = more.get("changed").is_some_and(|c| c.as_str() == Some("0") || c.as_u64() == Some(0));
            facts = (is == "job" && ["start", "end"].contains(&f("phase"))) || (is == "release" && f("phase") == "cut");
            let kept = (is == "job" && unchanged) || (is == "release" && f("phase") == "cut");
            if kept && !f("commit").is_empty() && !f("tree").is_empty() {
                self.trees.insert(short(f("commit")), f("tree").to_string());
            }
            match (is.as_str(), f("phase"), f("id")) {
                ("job", "start", id) if !id.is_empty() => {
                    let wt = f("wt");
                    self.running.retain(|_, (w, _)| wt.is_empty() || w != wt);
                    self.running.insert(id.to_string(), (wt.to_string(), short(f("tree"))));
                }
                ("job", "end", id) => {
                    self.running.remove(id);
                }
                _ => {}
            }
        }
        let Some((ev, v)) = ev else { return facts };
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default();
        match ev {
            "job_ended" => self.ends.entry(s("tree").to_string()).or_default().push((at.clone(), v.clone())),
            "cut" => {
                self.cuts.insert(short(s("commit")), (at.clone(), v.clone()));
            }
            "installed" => self.installed = Some((at.clone(), v.clone())),
            e if e == MERGED => {
                for c in v.get("commits").and_then(Value::as_array).into_iter().flatten() {
                    let id = short(c.get("commit").and_then(Value::as_str).unwrap_or_default());
                    let ps = c.get("parents").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(short).collect();
                    if let Some(t) = c.get("tree").and_then(Value::as_str) {
                        self.trees.insert(id.clone(), t.to_string());
                    }
                    self.parents.insert(id, ps);
                }
                self.trees.insert(short(s("tip")), s("tree").to_string());
                self.tips.insert(format!("{}/{}", s("repo"), s("branch")), s("tip").to_string());
                self.merges.insert(short(s("tip")), (at.clone(), v.clone()));
                facts = true;
            }
            _ => {}
        }
        facts
    }

    /// The lines of system event EV kept, the newest first: what a step given now reads. His window's is the
    /// machine's own (`Machine::window_release`), not here.
    pub fn lines(&self, ev: &str) -> Vec<(&At, &Value)> {
        let mut out: Vec<(&At, &Value)> = match ev {
            "job_ended" => self.ends.values().flatten().map(|(a, v)| (a, v)).collect(),
            "cut" => self.cuts.values().map(|(a, v)| (a, v)).collect(),
            "installed" => self.installed.iter().map(|(a, v)| (a, v)).collect(),
            e if e == MERGED => self.merges.values().map(|(a, v)| (a, v)).collect(),
            _ => Vec::new(),
        };
        // the record's order reversed, then by time: of one second's lines, the later first
        out.reverse();
        out.sort_by(|a, b| b.0.cmp(a.0));
        out
    }

    /// Fact NAME of ARGS (`syntax::fact_sig`), as the lines kept say; a commit or tree is named by its first 8.
    pub fn fact(&self, name: &str, args: &[Value]) -> Value {
        let arg = |i: usize| args.get(i).and_then(Value::as_str).map(short);
        match name {
            "reaches" => json!(matches!((arg(0), arg(1)), (Some(a), Some(b)) if self.reaches(&a, &b))),
            "tree_of" => arg(0).and_then(|c| self.trees.get(&c)).map_or(Value::Null, |t| json!(t)),
            "running" => json!(arg(0).is_some_and(|t| self.running.values().any(|(_, r)| *r == t))),
            _ => Value::Null,
        }
    }

    /// Whether commit A is B or has B under it, by the parents the `merged` lines listed.
    fn reaches(&self, a: &str, b: &str) -> bool {
        let (mut seen, mut next) = (BTreeSet::new(), vec![a.to_string()]);
        while let Some(c) = next.pop() {
            if c == b {
                return true;
            }
            if seen.insert(c.clone()) {
                next.extend(self.parents.get(&c).into_iter().flatten().cloned());
            }
        }
        false
    }

    /// E with each fact in it read ([`Expr::Fact`], its arguments in ENV and CTX first) into the value it has: what
    /// `Machine::on_answer` evaluates. The checker keeps facts out of a `find`'s `where`, whose element it has not.
    pub fn read(&self, e: &mut Expr, env: &Env, ctx: &Value) {
        match e {
            Expr::Fact { name, args } => {
                args.iter_mut().for_each(|a| self.read(a, env, ctx));
                let vs: Vec<Value> = args.iter().map(|a| eval(a, env, ctx)).collect();
                let v = self.fact(name, &vs);
                *e = match v {
                    Value::Bool(b) => Expr::Bool(b),
                    Value::String(s) => Expr::Str(s),
                    _ => Expr::None,
                };
            }
            Expr::Field(x, _) | Expr::ResultOf { e: x, .. } | Expr::TextOf { e: x, .. } | Expr::ByOf(x) | Expr::Not(x) | Expr::Find { e: x, .. } | Expr::Quant { e: x, .. } => self.read(x, env, ctx),
            Expr::Con { args: xs, .. } | Expr::List(xs) | Expr::Tuple(xs) => xs.iter_mut().for_each(|x| self.read(x, env, ctx)),
            Expr::Record(fs) => fs.iter_mut().for_each(|(_, x)| self.read(x, env, ctx)),
            Expr::Cmp { l, r, .. } | Expr::Rel { l, r, .. } | Expr::And(l, r) | Expr::Or(l, r) | Expr::Append(l, r) => {
                self.read(l, env, ctx);
                self.read(r, env, ctx);
            }
            _ => {}
        }
    }
}
