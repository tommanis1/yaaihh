//! A call's question (`asks C`, N-101; the owner on the designer's question: "3 no same session").
//!
//! `call design(...) -> Design(...) | Ask(question: Text)` with the line `asks Ask`: an answer formed to Ask is not
//! taken. The step stays owed by the agent that answered it, and the question becomes its ask on the ticket, as a
//! `coord ask` past its wait is: an `ask` ticket under the nearest ticket whose kind has `when ask`, or, with none, under
//! the ticket itself, where the owner answers it (`OWNER_ASKED`). The reply reaches the asker as words with the step's
//! line again, in its own session, and its next answer answers the step. Nothing new is written: the answer's line and
//! the ask ticket's `opened` line are the record, and replay makes the ask again from the answer.

use super::{AskOf, Cx, Env, Machine, Status, Work, loose_kind, ASK};
use crate::line::{Effect, Line};
use crate::syntax::{self, Kind};
use crate::types::{Text, Type};
use serde_json::{Value, json};

/// What runs a question no kind above takes: the owner answers it, and the answer goes back to the asker.
const OWNER_ASKED: &str = "a <- step owner \"{{ q.by }} asks: {{ q.text }}\" -> Answer(text: Text);\nreply q a";

impl Machine {
    /// The answer HEAD (from TEXT) by BY to STEP of T, if it is the call's question: asked, not taken. Whether it was.
    pub(super) fn asked_by_head(&mut self, cx: &mut Cx, t: u64, by: &str, head: &Value, text: &Text, step: u32) -> bool {
        let Status::Awaits { agent, owed, .. } = &self.tickets[&t].status else { return false };
        let Some(c) = self.call_of(t, &owed.call).and_then(|s| s.asks) else { return false };
        if head.get("head").and_then(Value::as_str) != Some(c.as_str()) {
            return false;
        }
        let agent = agent.clone();
        // the agent asked this already, with `coord ask`, while it held the step: the step's Ask is that question, not
        // another (paper-research N-101 p524, 2). The answer is posted; the step waits on the question, as it did
        if let Some(q) = self.tickets.values().find(|x| x.is_open() && x.ask.as_ref().is_some_and(|a| a.own && a.by == agent && a.held == t && !a.replied)).map(|x| x.t) {
            self.post_answer(cx, t, by, head, text, step);
            if let Status::Awaits { forming, refused, .. } = &mut self.tickets.edit().get_mut(&t).unwrap().status {
                *forming = None;
                *refused = None;
            }
            cx.effect(Effect::Changed { t });
            cx.effect(Effect::Changed { t: q });
            return true;
        }
        // one at a time, as a `coord ask` is: a second question while the first is open is refused
        if self.tickets.values().any(|x| x.ask.as_ref().is_some_and(|a| a.by == agent && a.held == t && !a.replied) && x.is_open()) || self.held_asks().iter().any(|(_, q)| q.ask.by == agent) {
            self.refuse(cx, t, step, &format!("your question on {} is not answered yet: answer the step, or wait for that answer", self.id(t)));
            return true;
        }
        let question = head.as_object().and_then(|o| o.iter().find(|(k, _)| *k != "head")).map(|(_, v)| v.clone()).unwrap_or(Value::Null);
        // its words where the machine has them; a place (the agent's answer) goes on as it is, for coordd to read: never
        // printed as a record (N-101 p524)
        let words = question;
        let question = match &words {
            Value::String(s) => s.clone(),
            v => self.readable(v).as_str().map(str::to_string).unwrap_or_default(),
        };
        // the answer is posted, as any answer is (the question shows on the ticket); the step stays owed
        self.post_answer(cx, t, by, head, text, step);
        let x = self.tickets.edit().get_mut(&t).unwrap();
        if let Status::Awaits { forming, refused, .. } = &mut x.status {
            *forming = None;
            *refused = None;
        }
        cx.effect(Effect::Changed { t });
        // the `during` around the step looks at it first, if one takes it (ask_minded.rs)
        let q = super::HeldAsk { ask: AskOf { by: agent.clone(), text: question.clone(), words: words.clone(), waiting: false, replied: false, held: t, own: false }, context: None, head: true, stopped: false };
        if let Err(q) = self.ask_minded(cx, q) {
            self.open_ask(cx, t, &agent, &q.ask.words);
        }
        true
    }

    /// BY's answer HEAD (from TEXT) to STEP of T posted on T, as `accept` posts an answer, without taking it.
    pub(super) fn post_answer(&mut self, cx: &mut Cx, t: u64, by: &str, head: &Value, text: &Text, step: u32) {
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let n = x.last() + 1;
        x.posts.push(crate::line::Post { n, at: cx.now.clone(), by: by.to_string(), text: text.clone(), head: Some(head.clone()), step: Some(step), names: Vec::new(), told: Vec::new(), auto: false, moved: None, from: None, held: None, halted: false, dropped: false, routed: false, asks: Vec::new(), asks_open: false, asks_closed: None, answers: None, inform: Vec::new(), typed: None, cleaned_from: None, split_from: None, also: Vec::new(), on: Vec::new(), would_close: None, taken_back: false, joins: None, passes: None, comments: Vec::new(), into_turn: Vec::new(), kind: None, answering: Vec::new() });
        x.read_own(by.to_string(), n, self.era, &mut self.passed_his);
        cx.line(x.posts.last().unwrap().posted(t));
    }

    /// AGENT's QUESTION on T, whose step it holds, past its wait: run by the nearest `when ask` above T, else by the
    /// owner (`OWNER_ASKED`) under T.
    pub(super) fn open_ask(&mut self, cx: &mut Cx, t: u64, agent: &str, question: &Value) {
        let q = json!({"by": agent, "text": question, "context": null});
        let mut u = t;
        let found = loop {
            let x = &self.tickets[&u];
            if let Some(h) = x.handlers.iter().find(|h| h.ev == "ask") {
                let mut env: Env = x.form.as_object().map(|o| o.iter().map(|(a, b)| (a.clone(), b.clone())).collect()).unwrap_or_default();
                env.insert("this".into(), json!(self.id(u)));
                env.insert(h.x.clone(), q.clone());
                if h.cond.as_ref().is_none_or(|c| super::eval(c, &env, &x.ctx) == json!(true)) {
                    break Some((h.clone(), env, u));
                }
            }
            match x.parent.or(x.under) {
                Some(p) => u = p,
                None => break None,
            }
        };
        let (h, env, u) = found.unwrap_or_else(|| {
            let x = &self.tickets[&t];
            let body = syntax::parse_prog(OWNER_ASKED, 1).expect("the owner's answer to a question parses");
            let h = syntax::When { ev: "ask".into(), x: "q".into(), cond: None, body, line: 0, from: x.kind.clone() };
            let mut env: Env = x.form.as_object().map(|o| o.iter().map(|(a, b)| (a.clone(), b.clone())).collect()).unwrap_or_default();
            env.insert("this".into(), json!(self.id(t)));
            env.insert("q".into(), q.clone());
            (h, env, t)
        });
        let c = self.next.max(self.tickets.range(..super::HIDDEN).next_back().map_or(0, |(m, _)| m + 1));
        let (body, program) = self.handler_program(cx, c, u, &h, None);
        let k = Kind { pattern: body.clone(), result: Type::Any, ..loose_kind(ASK) };
        let mut x = self.new_ticket(c, ASK, None, None, q, self.child_ctx(u), String::new(), &k, &cx.now);
        x.under = Some(u);
        x.raised_by = Some(agent.to_string());
        x.ask = Some(AskOf { by: agent.to_string(), text: question.as_str().unwrap_or_default().to_string(), words: question.clone(), waiting: false, replied: false, held: t, own: false });
        self.tickets.edit().insert(c, x);
        self.next = c + 1;
        cx.line(Line::Opened { at: cx.now.clone(), t: c, kind: ASK.into(), parent: u, program, calls: Default::default() });
        cx.effect(Effect::Changed { t: c });
        self.drive(cx, vec![Work::Run(c, body, env)]);
    }
}
