//! The stop (paper-research N-101 p934): a failure of the engine's that nothing above the ticket handles, and that it
//! can run again from, stops the ticket where it was raised instead of unwinding. Its stack stays as it was and every
//! ticket above it still waits on it. The lead (else the owner) is given one step of agent-base's call `stopped`, in a
//! hidden ticket under it: Again runs the node it stopped at again; Ended unwinds the failure as a raise does. A second
//! stop at the same place goes to the owner.
//!
//! An `iterate` whose rounds ran out (Exhausted) stops too (paper-research N-101 p1141, 1: run 4's change closed failed
//! and nobody had a step): Again is one round more, from the value the last round gave.
//!
//! Not stopped, as before: Cancelled and Ended (someone ended it), the program's own `raise`, Stalled (the program's
//! limit), Unplaced (a filing's, which goes back to the owner), and a failure a ticket is given by one below
//! it (that one did not stop). A record from before this, whose lines close the ticket with that failure, replays as it
//! was recorded: it unwinds.

use super::{Cx, Frame, Kind, Machine, Status, Work, loose_kind};
use crate::line::{Line, Raised};
use crate::syntax::{self, Prog};
use crate::types::Type;
use crate::{check, print};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The call the lead is given (agent-base's `calls/stopped.call`). With no such call nothing can be asked, and the
/// failure unwinds as before.
pub const CALL: &str = "stopped";
/// The hidden ticket's kind, and what it handles.
pub const KIND: &str = "stopped";
/// The failures that stop: those that can be run again from where they were raised (N-101 post 933, point 2).
pub const STOPS: [&str; 11] = ["NoResult", "NotFound", "Silent", "Malformed", "NoWorker", "RunnerRefused", "NoLayer", "Violated", "NoCall", "Stuck", "Exhausted"];

impl Machine {
    /// Whether R, raised at T, stops T rather than unwinding (see the module's doc). Takes T's `passing`.
    pub(super) fn stops_at(&mut self, cx: &Cx, t: u64, r: &Raised) -> bool {
        let x = self.tickets.edit().get_mut(&t).unwrap();
        if std::mem::take(&mut x.passing) || !STOPS.contains(&r.exc.as_str()) || x.handling.is_some() || x.stop_of.is_some() {
            return false;
        }
        if !matches!(&x.last_node, Some((p, ..)) if !matches!(p, Prog::Raise { .. })) || !self.defs.calls.contains_key(CALL) {
            return false;
        }
        // Exhausted stops only where its rounds ran out (an `iterate`); a `for` or a branch count over its most does not
        if r.exc == "Exhausted" && !matches!(&x.last_node, Some((Prog::Iterate { n: 1, .. }, ..))) {
            return false;
        }
        // a record whose lines close it with this failure: it unwound then, and does now
        if cx.recorded.as_ref().is_some_and(|rec| rec.iter().any(|l| matches!(l, Line::Closed { t: c, raised: Some(q), .. } if *c == t && q.exc == r.exc))) {
            return false;
        }
        !self.taken_above(t, &r.exc)
    }

    /// Whether a `try` on T's frames, or on a ticket above it that waits on the one below, has an arm for EXC; an `any`
    /// above takes what its branches raise.
    fn taken_above(&self, t: u64, exc: &str) -> bool {
        let mut u = t;
        loop {
            let x = &self.tickets[&u];
            if x.k.iter().any(|f| matches!(f, Frame::Try { arms, .. } if arms.iter().any(|h| h.exc == exc))) {
                return true;
            }
            let Some(p) = x.parent else { return false };
            let Some(y) = self.tickets.get(&p) else { return false };
            if !matches!(&y.status, Status::Waits { children, .. } if children.contains(&u)) {
                return false;
            }
            if y.any.is_some() {
                return true;
            }
            u = p;
        }
    }

    /// T stops at R: `Stopped` is written, and TO (OWNER when it is given to the owner) is given the step `stopped` in a
    /// hidden ticket under T.
    pub(super) fn stop(&mut self, cx: &mut Cx, t: u64, r: Raised, owner: bool, work: &mut Vec<Work>) {
        let x = &self.tickets[&t];
        let (node, env, steps) = x.last_node.clone().unwrap();
        let key = format!("{} {} {}", r.exc, node.at(), print::node(&node).1);
        let agent = match &x.status {
            Status::Awaits { agent, .. } => agent.clone(),
            _ => String::new(),
        };
        // as a step passed up: the lead, unless it is the one whose step failed; else the owner
        let lead = self.holders("lead").into_iter().find(|l| *l != agent).filter(|_| agent != "owner");
        let to = match lead {
            Some(l) if !owner && !x.stops.contains(&key) => l,
            _ => String::from("owner"),
        };
        let place = format!("{} stopped at {}", self.labelled(t, true), where_in(&node, steps));
        let failure = self.failure(&r);
        let from = self.came_from(t);
        let c = self.next_hidden();
        // its program: as recorded on replay, else made here
        let recorded = cx.recorded.as_ref().and_then(|rec| {
            rec.iter().find_map(|l| match l {
                Line::Opened { t: ct, program, .. } if *ct == c && !program.is_empty() => Some(program.clone()),
                _ => None,
            })
        });
        let text = recorded.unwrap_or_else(|| if to == "owner" { format!("step owner {CALL}(place, failure, from)") } else { format!("step who {CALL}(place, failure, from)") });
        let scope: BTreeMap<String, Type> = [("who", Type::Agent), ("place", Type::Text), ("failure", Type::Text), ("from", Type::Text)].into_iter().map(|(n, t)| (n.to_string(), t)).collect();
        let mut body = match syntax::parse_prog(&text, 1) {
            Ok(p) => p,
            Err(e) => Prog::Raise { exc: "NoCall".into(), e: Some(syntax::Expr::Str(format!("the stop's recorded program no longer parses: {e}"))), at: 0 },
        };
        let _ = check::check_in_with(&mut body, &Type::Any, &scope, &[], &self.defs.calls, &self.defs.kinds, Some(&self.defs.named), false);
        let k = Kind { pattern: body.clone(), result: Type::Any, ..loose_kind(KIND) };
        let mut y = self.new_ticket(c, KIND, None, None, Value::Null, self.child_ctx(t), String::new(), &k, &cx.now);
        y.under = Some(t);
        y.handling = Some(KIND.into());
        y.stop_of = Some(t);
        self.tickets.edit().insert(c, y);
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.stops.push(key);
        x.status = Status::Stopped { raised: r.clone(), node, env, to: to.clone(), by: c };
        cx.line(Line::Stopped { at: cx.now.clone(), t, raised: r, to: to.clone() });
        cx.line(Line::Opened { at: cx.now.clone(), t: c, kind: KIND.into(), parent: t, program: text, calls: BTreeMap::new() });
        cx.effect(crate::line::Effect::Changed { t });
        let env: super::Env = [("who", json!(to)), ("place", json!(place)), ("failure", json!(failure)), ("from", json!(from))].into_iter().map(|(n, v)| (n.to_string(), v)).collect();
        work.push(Work::Run(c, body, env));
    }

    /// The hidden ticket C, which asked what the stopped ticket S does, closed: Again runs S's node again; Ended
    /// unwinds S's failure; a stop step that failed is given to the owner, and when the owner's fails, S unwinds.
    pub(super) fn stop_answered(&mut self, cx: &mut Cx, s: u64, c: u64, work: &mut Vec<Work>) {
        let Some(Status::Stopped { raised, node, env, to, by }) = self.tickets.get(&s).map(|x| x.status.clone()) else { return };
        if by != c {
            return;
        }
        let head = match &self.tickets[&c].status {
            Status::Done(v) => v.get("head").and_then(Value::as_str).map(str::to_string),
            _ => None,
        };
        let x = self.tickets.edit().get_mut(&s).unwrap();
        x.status = Status::Waits { children: Vec::new(), par: false };
        cx.effect(crate::line::Effect::Changed { t: s });
        match head.as_deref() {
            Some("Again") => work.push(Work::Run(s, node, env)),
            None if to != "owner" => {
                x.status = Status::Stopped { raised: raised.clone(), node, env, to, by };
                self.stop(cx, s, raised, true, work);
            }
            _ => {
                x.passing = true;
                work.push(Work::Throw(s, raised));
            }
        }
    }

    /// T's label and id, as `label (T-1)` (PARENS) or `label T-1`.
    fn labelled(&self, t: u64, parens: bool) -> String {
        let label = crate::label::ticket(self, t, &crate::label::own_words);
        let id = self.id(t);
        match (label.is_empty(), parens) {
            (true, _) => id,
            (false, true) => format!("{label} ({id})"),
            (false, false) => format!("{label} {id}"),
        }
    }

    /// The tickets above T, nearest first: "It came from: culprit C-13, from mend M-10, from build B-9."
    fn came_from(&self, t: u64) -> String {
        let mut above = Vec::new();
        let mut u = self.tickets.get(&t).and_then(|x| x.parent);
        while let Some(p) = u {
            above.push(self.labelled(p, false));
            u = self.tickets.get(&p).and_then(|x| x.parent);
        }
        if above.is_empty() { String::from("No ticket is above it.") } else { format!("It came from: {}.", above.join(", from ")) }
    }
}

/// Where in its program NODE is, its ticket having given STEPS steps before it: "its first step", "its `match`".
fn where_in(node: &Prog, steps: u32) -> String {
    match node {
        Prog::Bind { first, .. } => where_in(first, steps),
        Prog::Step { .. } => format!("its {} step", ordinal(steps + 1)),
        p => format!("its `{}`", print::node(p).0),
    }
}

impl Machine {
    /// The failure's words: its why; a duty broken (Violated) by its fields (docket-core3, N-101 p1047, 2: the lead was
    /// shown `Violated(Commit({…}), "runner", "owes-typecheck", "about-note", "T-10")`); else its name and value.
    pub(crate) fn failure(&self, r: &Raised) -> String {
        let v = &r.value;
        if let Some(w) = v.get("why").and_then(Value::as_str) {
            return w.to_string();
        }
        if r.exc == "Violated" && let Some(duty) = v.get("duty").and_then(Value::as_str) {
            let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
            let (holder, claimant, work) = (s("holder"), s("claimant"), s("work"));
            // the work by its ticket: `T-10` is ticket 10's work (eflint-rs record.rs names it LETTER-T)
            let of = work.rsplit_once('-').and_then(|(_, n)| n.parse::<u64>().ok()).filter(|n| self.tickets.contains_key(n)).map(|n| format!(" ({})", self.id(n))).unwrap_or_default();
            let answered = v.get("answered").map(answer_words).unwrap_or_default();
            let also = v.get("also").and_then(Value::as_array).map_or(0, Vec::len);
            let more = match also {
                0 => String::new(),
                1 => String::from(" One more duty is broken by it."),
                n => format!(" {n} more duties are broken by it."),
            };
            return format!("{holder} owes {claimant} `{duty}` on the work {work}{of}, and answered {answered} while it was still owed.{more}");
        }
        if v.is_null() { r.exc.clone() } else { format!("{}: {}", r.exc, print::value(v)) }
    }
}

/// An answer's head in words: its outcome, then each field, a commit by repository as `a 22329bb`.
fn answer_words(h: &Value) -> String {
    let Some(name) = h.get("head").and_then(Value::as_str) else { return print::value(h) };
    let mut out = vec![name.to_string()];
    for (k, x) in h.as_object().into_iter().flatten().filter(|(k, _)| k.as_str() != "head" && !k.starts_with('@')) {
        match x {
            Value::Object(o) if o.values().all(Value::is_string) => out.extend(o.iter().map(|(r, c)| format!("{r} {}", c.as_str().unwrap_or_default().chars().take(7).collect::<String>()))),
            Value::String(c) if c.len() <= 40 => out.push(c.clone()),
            Value::String(_) | Value::Null => {}
            x => out.push(format!("{k} {}", print::value(x))),
        }
    }
    out.join(" ")
}

fn ordinal(n: u32) -> String {
    const WORDS: [&str; 10] = ["first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth", "tenth"];
    match n {
        1..=10 => WORDS[n as usize - 1].to_string(),
        _ => {
            let s = match (n % 10, n % 100) {
                (_, 11..=13) => "th",
                (1, _) => "st",
                (2, _) => "nd",
                (3, _) => "rd",
                _ => "th",
            };
            format!("{n}{s}")
        }
    }
}
