//! An agent made from an agent's turn (item 11; the owner on N-199, paper-research N-101 p271): `coord new NAME TEXT`,
//! a fresh agent, and `coord fork [FROM] --name NEW TEXT`, a copy of FROM's conversation (the caller's when FROM is not
//! given). The orchestrator makes the agent itself before it answers, and answers with its name.
//!
//! The project's policy decides, by the act `make-agent(actor, recipient, source, thread)`: ACTOR makes RECIPIENT
//! copying SOURCE ("none": a fresh one) in THREAD (the thread of the step the actor owes, asked as "t"; "none" when it
//! owes none). With the act shown the facts `made(agent, by, from, thread)` for every agent made so far (the machine's
//! `made`: the `made` lines and a program's `let agent`), a thread being "t" when it is the one asked about.
//! agent-base's `policy/assign.eflint` lets an agent copy itself and nothing more (paper-research N-101 p344); a
//! project's own `assign.eflint` replaces it. A policy without the act makes no agent, as `read-session` reads no
//! session. A copy of another agent reads its session, so it also needs what `coord log` needs
//! (`Core::may_read_session`).
//!
//! What is made is recorded as a line `made` (T, the ticket of the step the actor owes; AGENT; BY; FROM: paper-research
//! N-101 p272), and the registry's `add` names the actor as its parent and FROM. Then it is started as the project
//! starts a step's agent (`crate::launch`): in the actor's layer if it runs in one, as a user of its own there where the
//! project is boxed, with the project's limits, path and box_setup; else in FROM's folder (a copy) or the actor's (a
//! fresh one). TEXT is its first words, after roster's fork note for a copy.

use crate::Shared;
use crate::core::{Core, Who, err, ok};
use coord_types::Reply;
use serde_json::Value;

/// What was asked (`Request::Make`).
pub struct Asked {
    pub new: String,
    pub from: Option<String>,
    pub model: Option<String>,
    pub text: String,
    pub fork: bool,
}

/// An agent to make, checked but for the policy.
#[derive(Debug)]
pub struct Plan {
    pub what: &'static str,
    pub actor: String,
    pub new: String,
    /// The agent it copies: none for a fresh one.
    pub source: Option<String>,
    /// Its model: a copy's, the one it copies; a fresh one's, `--model` when it names a model, else chosen by
    /// `model_of` (empty until then).
    pub model: String,
    /// `--model` as given: a model or a class name (the model classes, models.rs).
    pub asked_model: Option<String>,
    /// The folder it starts in when it is in no layer: the one it copies's, else the actor's.
    pub dir: String,
    /// The layer the actor runs in, if it is active: the new one starts in its view, boxed where the project is.
    pub layer: Option<String>,
    /// The ticket of the step the actor owes, if it owes one, and its thread.
    pub t: Option<u64>,
    pub thread: Option<String>,
    pub text: String,
    places: crate::policy::Places,
    facts: Vec<(String, Vec<String>)>,
}

/// WHO's request A, checked against what coordd knows (the names, the copy's session and who may read it), with the
/// policy's question made: the plan, or why not.
pub fn plan(core: &Core, who: &Who, a: Asked) -> Result<Plan, String> {
    let what = if a.fork { "coord fork" } else { "coord new" };
    let Who::Agent(actor) = who else { return Err(format!("{what}: run inside an agent's session; the owner makes agents with roster add or roster fork")) };
    let actor = actor.clone();
    let source = match (a.from.filter(|f| !f.is_empty()), a.fork) {
        (Some(f), true) => Some(f),
        (None, true) => Some(actor.clone()),
        (_, false) => None,
    };
    let new = if !a.new.is_empty() {
        a.new
    } else if let (true, Some(f)) = (a.fork, &source) {
        let mut n = format!("{f}-fork");
        let mut k = 2;
        while core.agents.contains_key(&n) {
            n = format!("{f}-fork{k}");
            k += 1;
        }
        n
    } else {
        return Err(format!("{what}: name the new agent"));
    };
    if core.agents.contains_key(&new) {
        return Err(format!("{what}: {new} is an agent already"));
    }
    if let Some(f) = &source {
        let Some(s) = core.agents.get(f) else { return Err(format!("{what}: no agent {f}")) };
        if s.kind != "claude" {
            return Err(format!("{what}: {f} is not a Claude Code agent; only those are copied"));
        }
        if f != &actor && !core.may_read_session(who, f) {
            return Err(format!("{what}: a copy of {f} reads its session, and {actor} may not (only the agents above {f}, and those the norms let: read-session)"));
        }
    }
    let m = &core.tickets.m;
    let t = m.owing(&actor).and_then(|id| docket_machine::parse_ticket(&id));
    let thread = t.map(|t| m.thread_of(t));
    // a copy keeps the model of what it copies; a fresh one's is said by `--model` or the policy (`model_of`): none
    // is taken by default (the model classes)
    let asked_model = a.model.filter(|m| !m.trim().is_empty());
    let model = source.as_ref().and_then(|f| core.agents.get(f)).map(|s| s.model.clone()).unwrap_or_default();
    let cwd = |n: &str| core.agents.get(n).and_then(|s| s.session.get("cwd").and_then(Value::as_str).map(str::to_string));
    let dir = source.as_deref().and_then(cwd).or_else(|| cwd(&actor)).unwrap_or_else(|| core.places().coord.clone());
    let layer = std::fs::read_to_string(format!("{}/agents/{actor}/layer", core.places().coord)).ok().map(|l| l.trim().to_string()).filter(|l| m.active_layers().iter().any(|(a, _, _)| a == l));
    let mut facts = core.norm_facts(&[]);
    facts.push(("agent".into(), vec![new.clone()]));
    facts.push(("source".into(), vec![source.clone().unwrap_or_else(|| "none".into())]));
    for (agent, by, from, th) in m.made() {
        let th = match (th, &thread) {
            (Some(x), Some(y)) if &x == y => "t".to_string(),
            (Some(x), _) => x,
            (None, _) => "none".to_string(),
        };
        facts.push(("made".into(), vec![agent, by, from.unwrap_or_else(|| "none".into()), th]));
    }
    Ok(Plan { what, actor, new, source, model, asked_model, dir, layer, t, thread, text: a.text, places: core.places(), facts })
}

/// Whether the policy lets P be made (`make-agent`; blocking: clingo), or why not.
pub fn allowed(p: &Plan) -> Result<(), String> {
    let norms = crate::policy::interp::acts(&p.places).map_err(|e| format!("{}: the policy could not be read: {e}", p.what))?;
    let shown = norms.solve_for(&["make-agent"], &p.facts).map_err(|e| format!("{}: the policy could not be solved: {e}", p.what))?;
    let src = p.source.clone().unwrap_or_else(|| "none".into());
    let thread = if p.thread.is_some() { "t" } else { "none" };
    match norms.verdict(&shown, "make-agent", &[&p.actor, &p.new, &src, thread]) {
        Ok(Some(v)) if v.enabled => Ok(()),
        Ok(Some(v)) => Err(format!("{}: the policy does not let {} make {}: {}", p.what, p.actor, p.new, v.why)),
        Ok(None) => Err(format!("{}: the project's policy has no make-agent, so no agent makes agents", p.what)),
        Err(e) => Err(format!("{}: the policy could not be asked: {e}", p.what)),
    }
}

/// P's model settled (the model classes, models.rs; blocking: the proxy, clingo): a copy keeps the model of what it
/// copies, and `--model` naming another model or class is refused, not taken; a fresh one runs the model `--model`
/// names, or, for a class, the policy's `model-for` among the models of that class; with no `--model`, the
/// project's model-for rule for agents decides, and with none it is refused. The decision, when one was made.
pub fn model_of(p: &mut Plan) -> Result<Option<coord_types::Decision>, String> {
    use crate::models::{Asked, Order};
    let scopes = p.places.sources().map_err(|e| format!("{}: the policy could not be read: {e}", p.what))?;
    let served = || crate::policy::Engine::start(p.places.clone()).served().map_err(|e| format!("{}: {e}", p.what));
    match (&p.source, p.asked_model.clone()) {
        (Some(_), None) => Ok(None),
        (Some(f), Some(w)) => {
            let order = Order::of(&scopes, &served()?);
            let bare = |m: &str| m.split(':').next().unwrap_or(m).to_string();
            let same = match order.asked(&w).map_err(|e| format!("{}: --model {e}", p.what))? {
                Asked::Model(m) => bare(&m) == bare(&p.model),
                Asked::Class(_, n) => order.tier.get(&bare(&p.model)) == Some(&n),
            };
            if same {
                Ok(None)
            } else {
                Err(format!("{}: a copy keeps the model of what it copies: {f} runs {}, and --model {w} is another", p.what, order.show(&bare(&p.model))))
            }
        }
        (None, Some(w)) => {
            let order = Order::of(&scopes, &served()?);
            match order.asked(&w).map_err(|e| format!("{}: --model {e}", p.what))? {
                Asked::Model(m) => {
                    p.model = m;
                    Ok(None)
                }
                Asked::Class(c, _) => {
                    let d = crate::models::decide(&p.places, &p.new, "coord new", Some(&format!("model.class == {c}"))).map_err(|e| format!("{}: {e}", p.what))?;
                    p.model = d.chosen.clone().ok_or_else(|| format!("{}: no model of class {c}: {}", p.what, crate::models::why_none(&d)))?;
                    Ok(Some(d))
                }
            }
        }
        (None, None) => {
            if crate::models::project_rule(&scopes).is_none() {
                let classes = Order::of(&scopes, &[]).class.keys().cloned().collect::<Vec<_>>().join(", ");
                return Err(format!("{}: say its model with --model, a model or a class ({classes}); the project's policy has no model-for rule for agents to choose one", p.what));
            }
            let d = crate::models::decide(&p.places, &p.new, "coord new", None).map_err(|e| format!("{}: {e}", p.what))?;
            p.model = d.chosen.clone().ok_or_else(|| format!("{}: no model: {}", p.what, crate::models::why_none(&d)))?;
            Ok(Some(d))
        }
    }
}

/// P registered (its maker its parent, what it copies on the registry's `add`) and recorded as a `made` line.
pub fn register(core: &mut Core, p: &Plan) -> Result<(), String> {
    core.add_made(&p.actor, &p.new, &p.model, p.source.as_deref()).map_err(|e| format!("{}: {e}", p.what))?;
    if let Err(e) = core.ticket_input(docket_machine::Input::Made { t: p.t, agent: p.new.clone(), by: p.actor.clone(), from: p.source.clone() }) {
        eprintln!("coordd: make: {}, made by {}: not recorded: {e}", p.new, p.actor);
    }
    Ok(())
}

/// `Request::Make` from WHO: planned, asked of the policy (off the lock), registered and recorded, then started.
pub async fn make(shared: &Shared, who: &Who, a: Asked) -> Reply {
    let p = match plan(&*shared.lock().await, who, a) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    let (p, decision) = match tokio::task::spawn_blocking(move || {
        allowed(&p)?;
        let mut p = p;
        let d = model_of(&mut p)?;
        Ok::<_, String>((p, d))
    })
    .await
    {
        Ok(Ok(x)) => x,
        Ok(Err(e)) => return err(e),
        Err(e) => return err(format!("coordd: {e}")),
    };
    let coord = {
        let mut core = shared.lock().await;
        if let Err(e) = register(&mut core, &p) {
            return err(e);
        }
        // the model chosen, on the ticket of the step the maker owes
        if let (Some(d), Some(t)) = (&decision, p.t) {
            let step = core.tickets.m.tickets.get(&t).map_or(0, |x| x.steps);
            core.ticket_decision(t, step, d);
        }
        core.places().coord.clone()
    };
    // its first words: TEXT, after roster's fork note for a copy
    let a = format!("{coord}/agents/{}", p.new);
    let prompt = format!("{a}/first.md");
    if let Err(e) = std::fs::create_dir_all(&a).and_then(|_| std::fs::write(&prompt, format!("{}\n", p.text.trim_end()))) {
        return err(format!("{}: {} is registered, but its first words could not be written ({prompt}: {e})", p.what, p.new));
    }
    let how = p.source.as_ref().map_or_else(|| String::from("a new agent"), |f| format!("a copy of {f}"));
    let what = format!("by {}'s {}", p.actor, p.what);
    match crate::launch(shared, crate::Launch { agent: p.new.clone(), model: p.model.clone(), dir: p.dir.clone(), prompt, layer: p.layer.clone(), from: p.source.clone(), cut: None, what, force: false }).await {
        Ok(()) => {
            let inside = p.layer.as_ref().map_or_else(String::new, |l| format!(", in layer {l}"));
            ok(format!("{}: made, {how}, on {}{inside}. Message it with coord msg {}, or answer with its name.", p.new, p.model, p.new))
        }
        Err(e) => err(format!("{}: {} is registered and recorded, but was not started: {e}", p.what, p.new)),
    }
}

#[cfg(test)]
mod tests;
