//! Parts by dependency (N-101; the owner, 15 Nov: "each part names the parts it stands on; the program starts every
//! part whose parts-below are on the line, in parallel": "yes this is the right stuructre").
//!
//! `par x in e up to n as K after A [reach KIND] { p }`: one part per element, as a `par` over a list makes; a part
//! whose A is empty runs at once, every other one is opened and held (`Line::Holds`) until each part it stands on has
//! reached the state, then runs (`Line::Freed`). A part reaches the state when it closed Done, or, with `reach KIND`,
//! when the latest ticket of kind KIND under it closed Done. A held part whose part below failed never runs: it closes
//! Cancelled, and the par raises the first failure in the order written that is not such a cancel. Names twice, a
//! name in A that is no element's, or a circle: nothing is opened, NoOrder is raised.

use super::{Cx, Env, Machine, Status, TraceKey, Work, eval};
use crate::line::{Line, Raised};
use crate::syntax::{Each, Prog};
use crate::types::Type;
use crate::line::Effect;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// A part that has not started: the parts it stands on (ticket numbers), the state they must reach, and its run.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Held {
    pub on: Vec<u64>,
    pub reach: Option<String>,
    pub prog: Prog,
    pub env: Env,
}

/// The field a cancel for a part below that failed carries: the par skips such a failure when it raises.
const STANDS_ON: &str = "stands_on";

/// The first failure among a par's closed branches, in their order; one that never ran because a part it stood on
/// failed comes only when there is no other.
pub(super) fn first_failure<'a>(sts: impl Iterator<Item = &'a Status>) -> Option<Raised> {
    let failed: Vec<&Raised> = sts.filter_map(|s| if let Status::Failed(r) = s { Some(r) } else { None }).collect();
    failed.iter().find(|r| r.value.get(STANDS_ON).is_none()).or(failed.first()).map(|r| (*r).clone())
}

/// The order of the elements, by index: for each, the indices of those it stands on. Or why there is none.
fn order(keys: &[Value], afters: &[Value]) -> Result<Vec<Vec<usize>>, String> {
    let mut at = BTreeMap::new();
    let mut names = Vec::new();
    for (i, k) in keys.iter().enumerate() {
        let Some(k) = k.as_str() else { return Err(format!("the name of element {} is {k}, not a text", i + 1)) };
        if at.insert(k.to_string(), i).is_some() {
            return Err(format!("two parts are named \"{k}\""));
        }
        names.push(k.to_string());
    }
    let mut on = Vec::new();
    for (i, a) in afters.iter().enumerate() {
        let mut mine = Vec::new();
        for n in a.as_array().map(Vec::as_slice).unwrap_or_default() {
            let n = n.as_str().unwrap_or_default();
            match at.get(n) {
                Some(&j) if j == i => return Err(format!("\"{}\" stands on itself", names[i])),
                Some(&j) => {
                    if !mine.contains(&j) {
                        mine.push(j);
                    }
                }
                None => return Err(format!("\"{}\" stands on \"{n}\", which is no part", names[i])),
            }
        }
        on.push(mine);
    }
    // a circle: some part can never start
    let mut state = vec![0u8; on.len()];
    fn visit(i: usize, on: &[Vec<usize>], state: &mut [u8], path: &mut Vec<usize>) -> Option<Vec<usize>> {
        match state[i] {
            2 => return None,
            1 => return Some(path[path.iter().position(|&p| p == i).unwrap()..].to_vec()),
            _ => {}
        }
        state[i] = 1;
        path.push(i);
        for &j in &on[i] {
            if let Some(c) = visit(j, on, state, path) {
                return Some(c);
            }
        }
        path.pop();
        state[i] = 2;
        None
    }
    for i in 0..on.len() {
        if let Some(c) = visit(i, &on, &mut state, &mut Vec::new()) {
            let mut ns: Vec<String> = c.iter().map(|&j| format!("\"{}\"", names[j])).collect();
            ns.push(ns[0].clone());
            return Err(format!("the parts stand on each other in a circle: {}", ns.join(" on ")));
        }
    }
    Ok(on)
}

impl Machine {
    /// `par x in e up to n as K after A [reach KIND] { b }` reached on T.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn ordered_par(&mut self, cx: &mut Cx, t: u64, mut branches: Vec<Prog>, tys: Vec<Type>, each: Each, env: &Env, ctx: &Value, work: &mut Vec<Work>, key: Option<TraceKey>) {
        let o = each.order.clone().expect("a par in order");
        let xs = match eval(&each.e, env, ctx) {
            Value::Array(xs) => xs,
            _ => Vec::new(),
        };
        if xs.len() > each.n as usize {
            work.push(Work::Throw(t, Raised { exc: "Exhausted".into(), value: json!({"branches": xs.len(), "most": each.n}) }));
            return;
        }
        if xs.is_empty() {
            work.push(Work::Resume(t, json!([])));
            return;
        }
        let envs: Vec<Env> = xs
            .into_iter()
            .map(|v| {
                let mut benv = env.clone();
                benv.insert(each.x.clone(), v);
                benv
            })
            .collect();
        let keys: Vec<Value> = envs.iter().map(|e| eval(&o.key, e, ctx)).collect();
        let afters: Vec<Value> = envs.iter().map(|e| eval(&o.after, e, ctx)).collect();
        let holds = match order(&keys, &afters) {
            Ok(h) => h,
            Err(why) => {
                work.push(Work::Throw(t, Raised { exc: "NoOrder".into(), value: json!({"head": "NoOrder", "why": why}) }));
                return;
            }
        };
        let b = branches.remove(0);
        let ty = tys.into_iter().next();
        let runs = envs.into_iter().map(|e| (b.clone(), e, ty.clone())).collect();
        self.parts_held(cx, t, runs, work, key, Some((holds, o.reach)));
    }

    /// The parts CHILDREN just opened, with their RUNS in the same order: those that stand on others are held, and
    /// written so; the runs of those that start at once are given back.
    pub(super) fn hold_parts(&mut self, cx: &mut Cx, children: &[u64], runs: Vec<Work>, holds: Vec<Vec<usize>>, reach: Option<String>) -> Vec<Work> {
        let mut now = Vec::new();
        for ((c, run), on) in children.iter().zip(runs).zip(holds) {
            if on.is_empty() {
                now.push(run);
                continue;
            }
            let Work::Run(_, prog, env) = run else { unreachable!() };
            let on: Vec<u64> = on.into_iter().map(|j| children[j]).collect();
            cx.line(Line::Holds { at: cx.now.clone(), t: *c, on: on.clone(), reach: reach.clone() });
            self.tickets.edit().get_mut(c).unwrap().held = Some(Held { on, reach: reach.clone(), prog, env });
        }
        now
    }

    /// Whether part P has reached the state REACH: closed Done, or the latest ticket of kind REACH under it closed Done.
    fn reached(&self, p: u64, reach: Option<&str>) -> bool {
        let Some(x) = self.tickets.get(&p) else { return false };
        if matches!(x.status, Status::Done(_)) {
            return true;
        }
        let Some(k) = reach else { return false };
        let mut latest: Option<u64> = None;
        let mut stack = x.children.clone();
        while let Some(c) = stack.pop() {
            let Some(y) = self.tickets.get(&c) else { continue };
            if y.kind == k && latest.is_none_or(|l| c > l) {
                latest = Some(c);
            }
            stack.extend(y.children.iter().copied());
        }
        latest.is_some_and(|l| matches!(self.tickets[&l].status, Status::Done(_)))
    }

    /// T has closed: for each part above it (T included) of a par in order that still waits, its held siblings that
    /// stand on it start once all they stand on have reached the state; if T is such a part and failed, those never
    /// start and close Cancelled. Whether any was cancelled (their closes then join the par, so T's does not).
    pub(super) fn part_closed(&mut self, cx: &mut Cx, t: u64, work: &mut Vec<Work>) -> bool {
        let failed = matches!(self.tickets[&t].status, Status::Failed(_));
        let mut cancelled = false;
        let mut a = Some(t);
        while let Some(p) = a {
            a = self.tickets.get(&p).and_then(|x| x.parent);
            let Some(par) = a else { break };
            let Some(Status::Waits { children, par: true }) = self.tickets.get(&par).map(|x| x.status.clone()) else { continue };
            let held: Vec<u64> = children.iter().copied().filter(|c| self.tickets[c].held.as_ref().is_some_and(|h| h.on.contains(&p))).collect();
            for s in held {
                if failed && p == t {
                    let (by, why) = (self.id(t), format!("stands on {}, which failed", self.id(t)));
                    let r = Raised { exc: "Cancelled".into(), value: json!({"head": "Cancelled", "by": by, "why": why, STANDS_ON: by}) };
                    self.cancel(cx, s, &by, &why, &r, work);
                    cancelled = true;
                    continue;
                }
                let h = self.tickets[&s].held.clone().unwrap();
                if h.on.iter().all(|&q| self.reached(q, h.reach.as_deref())) {
                    self.tickets.edit().get_mut(&s).unwrap().held = None;
                    cx.line(Line::Freed { at: cx.now.clone(), t: s });
                    cx.effect(Effect::Changed { t: s });
                    work.push(Work::Run(s, h.prog, h.env));
                }
            }
        }
        cancelled
    }
}
