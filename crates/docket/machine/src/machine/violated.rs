//! A duty broken at a step's answer: `Violated` (N-101 post 57; the owner: "waht a violation end up with in genral in a
//! pl term is an exeection hdnler that for exmaple steers. or escaltes whatver the user want ot write").
//!
//! When the project's policy declares a duty that can be violated (`Defs.duties`), an agent's answer to a step is put
//! to the policy before it is taken: `asked` with question `violated`, the ticket Asking meanwhile. The policy answers
//! the duties the answer breaks (`decided`, `violated`). None: the answer is taken as it would have been. Some: the
//! answer is posted, the step is answered for its delivery, and Violated {duty, holder, claimant, work, answered} is
//! raised at the step, where a `try` may handle it. The machine names no duty and no call: which duties there are,
//! and which answer breaks one, is the policy's.

use super::{Cx, Machine, Status, Work, NOBODY, POLICY, coordds_own};
use crate::line::{Effect, Line, Question, Raised};
use crate::types::Text;
use serde_json::{Value, json};

/// The question, as `asked` and `decided` write it.
pub const QUESTION: &str = "violated";
/// The role a ticket is Asking while an answer is put to the policy.
pub const ROLE: &str = "violated";

/// An answer put to the policy: by whom, what, to which step, and the ticket's state before.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pending {
    pub by: String,
    pub head: Value,
    pub text: Text,
    pub step: u32,
    pub before: Status,
}

impl Machine {
    /// Whether answers by AGENT are put to the policy: not the owner's, nobody's or the policy's, not coordd's own
    /// agents' (the watcher, the minder), and not a program agent's (the runner: one coordd says reads nothing).
    fn asks_about(&self, agent: &str) -> bool {
        agent != "owner" && agent != NOBODY && agent != POLICY && !coordds_own(agent) && self.readers.as_ref().is_none_or(|r| r.contains(agent))
    }

    /// BY's answer HEAD (from TEXT) to STEP of T: put to the policy, if it is asked about. Whether it was.
    /// On replay, where the log asked it.
    pub(super) fn ask_violated(&mut self, cx: &mut Cx, t: u64, by: &str, head: &Value, text: &Text, step: u32) -> bool {
        if !self.defs.duties {
            return false;
        }
        let Status::Awaits { owed, .. } = &self.tickets[&t].status else { return false };
        // a call's question is not taken, so it breaks nothing (asks_head.rs)
        if self.call_of(t, &owed.call).and_then(|c| c.asks).as_deref().is_some_and(|c| head.get("head").and_then(Value::as_str) == Some(c)) {
            return false;
        }
        let asked = match &cx.recorded {
            Some(rec) => rec.iter().any(|l| matches!(l, Line::Asked { t: u, step: s, question, .. } if *u == t && *s == step && question == QUESTION)),
            None => self.asks_about(by),
        };
        if !asked {
            return false;
        }
        let x = self.tickets.edit().get_mut(&t).unwrap();
        let before = x.status.clone();
        let Status::Awaits { owed, .. } = &before else { unreachable!() };
        x.status = Status::Asking { owed: owed.clone(), role: ROLE.into() };
        x.pending = Some(Pending { by: by.to_string(), head: head.clone(), text: text.clone(), step, before });
        cx.line(Line::Asked { at: cx.now.clone(), t, step, question: QUESTION.into(), role: by.to_string() });
        cx.effects.extend(self.violated_question(t, step));
        cx.effect(Effect::Changed { t });
        true
    }

    /// Whether T is asking the policy whether its answer to STEP breaks a duty (an ask of it still stands).
    pub fn asks_violated(&self, t: u64, step: u32) -> bool {
        self.tickets.get(&t).is_some_and(|x| matches!(&x.status, Status::Asking { role, .. } if role == ROLE) && x.pending.as_ref().is_some_and(|p| p.step == step))
    }

    /// The question T's answer put to the policy asks (to ask again after a replay).
    pub(super) fn violated_question(&self, t: u64, step: u32) -> Option<Effect> {
        let x = self.tickets.get(&t)?;
        let p = x.pending.as_ref()?;
        let Status::Asking { owed, .. } = &x.status else { return None };
        Some(Effect::Ask { t, step, question: Question::Violated { agent: p.by.clone(), call: owed.call.clone(), answered: p.head.clone() } })
    }

    /// The policy's ANSWER to whether T's answer to STEP breaks a duty: [] takes it, else Violated is raised there.
    pub(super) fn decided_violated(&mut self, cx: &mut Cx, t: u64, step: u32, answer: &Value) -> Result<String, String> {
        let id = self.id(t);
        // an ask that ended unanswered (null): coordd gave it up because the ticket no longer asks it (closed, or its
        // answer gone another way); a record of that only, never taken while the ticket still asks it
        if answer.is_null() {
            if self.asks_violated(t, step) {
                return Err(format!("{id} still asks whether its answer to step {step} breaks a duty: an ask that ended unanswered is no answer to it."));
            }
            return Ok(format!("The ask whether the answer to step {step} of {id} breaks a duty ended unanswered: {id} no longer asks it."));
        }
        let x = self.tickets.edit().get_mut(&t).ok_or_else(|| format!("There is no ticket {t}."))?;
        if !matches!(&x.status, Status::Asking { role, .. } if role == ROLE) || x.pending.as_ref().is_none_or(|p| p.step != step) {
            return Err(format!("{id} is not asking whether an answer to step {step} breaks a duty."));
        }
        let Some(broken) = answer.as_array().filter(|a| a.iter().all(|d| d.get("duty").and_then(Value::as_str).is_some())) else {
            return Err(String::from("That answer does not fit `violated`: the list of the duties broken, each with its `duty`."));
        };
        let broken = broken.clone();
        let p = x.pending.take().unwrap();
        x.status = p.before.clone();
        if let Status::Awaits { forming, .. } = &mut x.status {
            *forming = None;
        }
        cx.effect(Effect::Changed { t });
        let Some(first) = broken.first() else {
            self.take_answer_now(cx, t, &p.by, p.head, p.text, step);
            return Ok(format!("{}'s answer to step {step} of {id} breaks no duty.", p.by));
        };
        // the answer is given: posted, the step answered for its delivery, and the program raises
        self.post_answer(cx, t, &p.by, &p.head, &p.text, step);
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.last_by = Some(p.by.clone());
        x.status = Status::Waits { children: Vec::new(), par: false };
        self.end_scoped(cx, t, Some(&p.by));
        let mut v = json!({"head": "Violated", "answered": p.head});
        for f in ["duty", "holder", "claimant", "work"] {
            if let Some(a) = first.get(f) {
                v[f] = a.clone();
            }
        }
        if broken.len() > 1 {
            v["also"] = Value::Array(broken[1..].to_vec());
        }
        let duty = first["duty"].as_str().unwrap_or_default().to_string();
        self.drive(cx, vec![Work::Throw(t, Raised { exc: "Violated".into(), value: v })]);
        Ok(format!("{}'s answer to step {step} of {id} breaks {duty}: Violated raised there.", p.by))
    }

    /// AGENT's turn was stopped (`stop`): an answer of its being put to the policy counts as never given; its step is
    /// owed again as before.
    pub(super) fn drop_violated(&mut self, cx: &mut Cx, agent: &str) {
        let ts: Vec<u64> = self.tickets.values().filter(|x| x.pending.as_ref().is_some_and(|p| p.by == agent) && matches!(&x.status, Status::Asking { role, .. } if role == ROLE)).map(|x| x.t).collect();
        for t in ts {
            let x = self.tickets.edit().get_mut(&t).unwrap();
            let p = x.pending.take().unwrap();
            x.status = p.before;
            if let Status::Awaits { forming, .. } = &mut x.status {
                *forming = None;
            }
            cx.effect(Effect::Changed { t });
        }
    }
}
