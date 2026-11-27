//! A question to the owner looked at first (paper-research N-101 p384; the owner on 12 Nov: the minder "shoudl ofc be
//! there first eh with hte is htis even a relvant question is this not elary answered").
//!
//! When the step a question is asked on stands in a `during` whose `when act(u)` takes `{by, is: "ask", text,
//! ticket}` (its `where` holds), the `during`'s look runs before the question is asked. Both ways an agent asks go
//! through here: an answer to a call's `asks` head (asks_head.rs) and a waiting `coord ask` (`Line::Ask`). The
//! question is held on the look's hidden ticket (`Ticket::held_ask`), or, while another look runs, on the `during`
//! (its `asks`), for the next look. When the look closes, the question is asked as it would have been at once, unless
//! the look stopped the asker (`stop`: the look's post is what the agent gets), or the step it was asked on is no
//! longer owed by the asker. A look that fails lets the question through, so a broken minder swallows nothing.
//!
//! Nothing new is written: the answer's or the ask's line is the input, the look's `opened` line is its output, and
//! the ask's `opened` line is an output of the input that closes the look, so replay makes them again.

use super::{logged_run, AskOf, Cx, Frame, Machine, Status, Ticket, Work};
use crate::syntax;
use crate::line::Effect;
use serde_json::{Value, json};

/// A question held while a look runs on it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HeldAsk {
    pub ask: AskOf,
    pub context: Option<String>,
    /// Asked by an answer to a call's `asks` head (asks_head.rs), not by a waiting `coord ask`.
    pub head: bool,
    /// The look stopped the asker: the question is not asked.
    pub stopped: bool,
}

impl Machine {
    /// Hold Q while the `during` around the step it is asked on looks at it: Ok with what the asker is told, or Q
    /// back when no `during` takes it (none, its `where` does not hold, or, on replay, the log has no look for it).
    pub(super) fn ask_minded(&mut self, cx: &mut Cx, q: HeldAsk) -> Result<String, HeldAsk> {
        let t = q.ask.held;
        let Some((u, i)) = self.during_on(t, "act") else { return Err(q) };
        let Some((when, env, busy)) = self.ask_env(u, i, &q) else { return Err(q) };
        if busy {
            if let Some(Frame::During { asks, .. }) = self.tickets.edit().get_mut(&u).unwrap().k.get_mut(i) {
                asks.push(q);
            }
            return Ok(format!("Held: the look running on {} looks at your question next; it reaches the owner after that.", self.id(u)));
        }
        self.ask_look(cx, u, i, when, q, env)
    }

    /// The `during` at frame I of U on question Q: the handler that takes it and its environment, and whether a look
    /// runs now; None when no `where` holds.
    fn ask_env(&self, u: u64, i: usize, q: &HeldAsk) -> Option<(syntax::When, super::Env, bool)> {
        let Some(Frame::During { busy, .. }) = self.tickets[&u].k.get(i) else { return None };
        let busy = busy.is_some_and(|c| self.tickets.get(&c).is_some_and(Ticket::is_open));
        let (when, env) = self.during_when(u, i, "act", &ask_act(self, q))?;
        Some((when, env, busy))
    }

    /// The look of handler WHEN of the `during` at frame I of U on question Q, in ENV, with Q held on it.
    fn ask_look(&mut self, cx: &mut Cx, u: u64, i: usize, when: syntax::When, q: HeldAsk, env: super::Env) -> Result<String, HeldAsk> {
        if !logged_run(cx, "act", u) {
            return Err(q);
        }
        let Some(Frame::During { scope, .. }) = self.tickets[&u].k.get(i).cloned() else { return Err(q) };
        let value = ask_act(self, &q);
        let by = q.ask.by.clone();
        let Some((c, body)) = self.handler_ticket(cx, u, "act", &when, value, &by, Some(&scope)) else { return Err(q) };
        self.tickets.edit().get_mut(&c).unwrap().held_ask = Some(q);
        if let Some(Frame::During { busy, .. }) = self.tickets.edit().get_mut(&u).unwrap().k.get_mut(i) {
            *busy = Some(c);
        }
        cx.effect(Effect::Changed { t: u });
        let said = format!("Held: {} looks at your question first; it reaches the owner after that.", self.id(u));
        self.drive(cx, vec![Work::Run(c, body, env)]);
        Ok(said)
    }

    /// Look C closed: the question it held is asked, or not.
    pub(super) fn ask_looked(&mut self, cx: &mut Cx, c: u64) {
        if let Some(q) = self.tickets.edit().get_mut(&c).and_then(|x| x.held_ask.take()) {
            self.ask_after_look(cx, q);
        }
    }

    /// The next question the `during` at frame I of U holds, looked at now that no look runs; true if one was.
    pub(super) fn ask_next(&mut self, cx: &mut Cx, u: u64, i: usize) -> bool {
        let q = match self.tickets.edit().get_mut(&u).and_then(|x| x.k.get_mut(i)) {
            Some(Frame::During { asks, .. }) if !asks.is_empty() => asks.remove(0),
            _ => return false,
        };
        let looked = match self.ask_env(u, i, &q) {
            Some((when, env, _)) => self.ask_look(cx, u, i, when, q, env),
            None => Err(q),
        };
        match looked {
            Ok(_) => true,
            Err(q) => {
                self.ask_after_look(cx, q);
                false
            }
        }
    }

    /// Question Q, looked at (or no longer to be): asked unless the look stopped the asker or the step it was asked
    /// on is no longer the asker's; a waiting asker not asked for is told so.
    pub(super) fn ask_after_look(&mut self, cx: &mut Cx, q: HeldAsk) {
        let owed = matches!(self.tickets.get(&q.ask.held).map(|x| &x.status), Some(Status::Awaits { agent, .. }) if *agent == q.ask.by);
        let by = q.ask.by.clone();
        let not = if q.stopped {
            Some("the look at it stopped you; its words come to you as a post")
        } else if !owed {
            Some("the step it was asked on is no longer yours")
        } else {
            None
        };
        if let Some(why) = not {
            if q.ask.waiting {
                cx.effect(Effect::Answered { agent: by.clone(), text: format!("Not sent to the owner: {why}.") });
                cx.effect(Effect::Deliver { to: by });
            }
            return;
        }
        if q.head {
            self.open_ask(cx, q.ask.held, &by, &q.ask.words);
            return;
        }
        if let Err(e) = self.ask_waiting(cx, &by, &q.ask.text, q.context.clone(), q.ask.held, q.ask.waiting, false) {
            if q.ask.waiting {
                cx.effect(Effect::Answered { agent: by.clone(), text: format!("Not sent to the owner: {e}") });
                cx.effect(Effect::Deliver { to: by });
            }
        }
    }

    /// The questions held now: where (the look's ticket, or the `during`'s while it waits for a look), and each.
    pub(super) fn held_asks(&self) -> Vec<(u64, &HeldAsk)> {
        let mut out = Vec::new();
        for x in self.tickets.values() {
            out.extend(x.held_ask.iter().map(|q| (x.t, q)));
            for f in &x.k {
                if let Frame::During { asks, .. } = f {
                    out.extend(asks.iter().map(|q| (x.t, q)));
                }
            }
        }
        out
    }

    /// AGENT's held question, to change.
    pub(super) fn held_ask_mut(&mut self, agent: &str) -> Option<&mut HeldAsk> {
        for x in self.tickets.edit().values_mut() {
            if let Some(q) = x.held_ask.as_mut().filter(|q| q.ask.by == agent) {
                return Some(q);
            }
            for f in &mut x.k {
                if let Frame::During { asks, .. } = f {
                    if let Some(q) = asks.iter_mut().find(|q| q.ask.by == agent) {
                        return Some(q);
                    }
                }
            }
        }
        None
    }

    /// AGENT was stopped by ticket T's program: a question of its held by the look T runs in is not asked.
    pub(super) fn ask_stopped(&mut self, t: u64, agent: &str) {
        let mut u = Some(t);
        while let Some(v) = u {
            let Some(x) = self.tickets.edit().get_mut(&v) else { return };
            if let Some(q) = x.held_ask.as_mut().filter(|q| q.ask.by == agent) {
                q.stopped = true;
                return;
            }
            u = x.parent.or(x.under);
        }
    }
}

/// The act a question is to a `during`: `{by, is: "ask", text, ticket}`, as coordd's acts are.
fn ask_act(m: &Machine, q: &HeldAsk) -> Value {
    json!({"by": q.ask.by, "is": "ask", "text": q.ask.words, "ticket": m.id(q.ask.held)})
}
