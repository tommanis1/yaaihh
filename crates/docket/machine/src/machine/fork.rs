//! The fork (N-101 posts 11 and 12; the owner, N-121: "it should be like on question keep it one we move to impls fork
//! the question agent"): `step fork A c(...)` gives the step to a new agent made for it alone, and `let agent x = fork A
//! in {...}` names one that lives for the block; either starts from a copy of agent A's conversation as it stands at
//! that moment, and A goes on unchanged. The machine makes the agent and says whose conversation it copies
//! (Line::Forked, Effect::Fork); coordd copies the session and resumes the copy (coordd main.rs, Job::Start).

use super::*;

impl Machine {
    /// The effect that starts scoped agent AGENT of T for CALL, in T's layer: a fresh start, or with FROM a fork of
    /// FROM's conversation.
    pub(super) fn start_of(&mut self, agent: String, t: u64, call: String, from: Option<String>) -> Effect {
        // started again: an earlier failure to start it no longer stands
        self.unstarted.remove(&agent);
        let layer = self.layer_of(t);
        match from {
            Some(from) => Effect::Fork { agent, t, call, layer, from },
            None => Effect::Start { agent, t, call, layer },
        }
    }

    /// `step fork FROM c(...)`: as `fresh`, a short-lived agent made for this step alone and ended when it is answered,
    /// but started as a fork of FROM.
    pub(super) fn fork_step(&mut self, cx: &mut Cx, t: u64, from: String, owed: Owed, call: String) {
        let x = self.tickets.edit().get_mut(&t).unwrap();
        x.scoped += 1;
        let agent = format!("f{t}-{}", x.scoped);
        x.live.push(agent.clone());
        cx.line(Line::Forked { at: cx.now.clone(), t, agent: agent.clone(), from: from.clone() });
        self.await_step_of(cx, t, agent, owed, Some(call), Some(from));
    }
}

impl Machine {
    /// coordd could not start AGENT (Line::StartFailed, N-101: a fork, fresh, let agent or restart): every step owed to
    /// it now raises NoWorker {agent, why}; a later one raises when it is reached (`unstarted_raise`), unless AGENT is
    /// started again first.
    pub(super) fn start_failed(&mut self, cx: &mut Cx, agent: &str, why: &str) -> (bool, Result<String, String>) {
        self.unstarted.insert(agent.to_string(), why.to_string());
        // a copy (`fork … at`, N-1104 p1154) that never ran is nobody's copy: its agent's next post may make another
        self.drop_copy(agent);
        let owing: Vec<u64> = self.tickets.iter().filter(|(_, x)| x.is_open() && matches!(&x.status, Status::Awaits { agent: a, .. } if a == agent)).map(|(t, _)| *t).collect();
        let r = no_worker(agent, why);
        // it was never given the step, so nothing is withdrawn from it
        self.drive(cx, owing.iter().map(|t| Work::Throw(*t, r.clone())).collect());
        let on = owing.iter().map(|t| self.id(*t)).collect::<Vec<_>>().join(", ");
        (true, Ok(if on.is_empty() { format!("{agent} could not be started: {why}. Its next step raises NoWorker.") } else { format!("{agent} could not be started: {why}. Its step on {on} raises NoWorker.") }))
    }

    /// NoWorker for a step to AGENT, when coordd could not start it and nothing has started it since; an agent the
    /// program made whose scope has ended is started again by its step instead (`await_or_restart`), so not refused.
    pub(super) fn unstarted_raise(&self, agent: &str) -> Option<Raised> {
        let why = self.unstarted.get(agent)?;
        (!self.ended(agent)).then(|| no_worker(agent, why))
    }
}

fn no_worker(agent: &str, why: &str) -> Raised {
    Raised { exc: "NoWorker".into(), value: json!({"head": "NoWorker", "agent": agent, "why": why}) }
}

/// The agent `fork A` copies, A evaluated: its name; NoWorker when A names none (as a step's worker that is no agent).
pub(super) fn fork_from(a: &Expr, env: &Env, ctx: &Value) -> Result<String, Raised> {
    match eval(a, env, ctx) {
        Value::String(from) if !from.is_empty() => Ok(from),
        v => Err(Raised { exc: "NoWorker".into(), value: json!({"head": "NoWorker", "why": format!("`fork {}` is {v}, not an agent", crate::print::expr(a))}) }),
    }
}
