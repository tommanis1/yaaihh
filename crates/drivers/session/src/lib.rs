//! The session driver: `Start-Session` carried by the agent's harness's own start, then its session's Events.
//!
//! What an agent runs on and how is its profile, which the daemon keeps and gives here by the agent's party; how a
//! started harness gets its box and its socket is the daemon's too, given as `spawn`. This driver joins the two:
//! the harness's start turns the profile's switches into its arguments, environment and files, and once the
//! harness is spawned the session states what it runs on (`Session-Started`) and, where the profile names one, the
//! mode it was started in (`Mode-Stated`). A tool asked off that the harness would keep on fails the start; each other
//! switch it cannot take goes to `spawn` in `Launch.not_taken`, and the daemon says it where it says its own.

use docket_driver_agy as agy;
use docket_driver_claude as claude;
use docket_driver_codex as codex;
use docket_driver_opencode as opencode;
use docket_hooks::start::{Launch, Prompt, Switches};
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::{Instance, Value};

pub const START: &str = "std.hooks.Start-Session";
pub const STARTED: &str = "std.hooks.Session-Started";
pub const MODE: &str = "std.hooks.Mode-Stated";

/// What one agent runs on and how: harness, version, mode, switches, system text, and the model the program passed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    pub harness: String,
    pub version: String,
    pub mode: Option<String>,
    pub switches: Switches,
    pub prompt: Option<(String, Prompt)>,
    /// The model the start passes on unread; empty: the harness's own choice.
    pub model: String,
}

/// The start of harness `p.harness` for profile `p`, its hooks run by the relay at `relay`; none if a tool asked off
/// would stay on.
pub fn launch(p: &Profile, relay: &str) -> Result<Launch, String> {
    let prompt = p.prompt.as_ref().map(|(f, how)| (f.as_str(), *how));
    let mut l = match p.harness.as_str() {
        claude::HARNESS => {
            let mut l = claude::start::launch(&p.switches, prompt);
            l.files.push(("settings.json".into(), claude::hooks_settings(relay).to_string()));
            l
        }
        codex::HARNESS => {
            let mut l = codex::start::launch(&p.switches, prompt)?;
            l.files.push(("hooks.json".into(), codex::start::hooks_file(relay).to_string()));
            l
        }
        opencode::HARNESS => opencode::start::launch(&p.switches, prompt),
        agy::HARNESS => agy::start::launch(&p.switches, prompt)?,
        h => return Err(format!("no start for harness {h}")),
    };
    if !l.tools_on.is_empty() {
        return Err(l.tools_on.join("; "));
    }
    l.env.push(("DOCKET_HOOK_RELAY".into(), relay.into()));
    Ok(l)
}

/// The driver, given the relay's path, the daemon's profiles by party, and its way to spawn a started harness.
pub struct Session<P, S> {
    pub relay: String,
    pub profile: P,
    pub spawn: S,
}

impl<P, S> Driver for Session<P, S>
where
    P: FnMut(&Value) -> Option<Profile>,
    S: FnMut(&Value, &Profile, Launch) -> Result<(), String>,
{
    fn name(&self) -> Vec<Value> {
        vec![Value::text("session")]
    }

    fn effects(&self) -> Vec<(String, Vec<String>)> {
        vec![(START.into(), vec![])]
    }

    fn events(&self) -> Vec<String> {
        vec![STARTED.into(), MODE.into()]
    }

    fn carry(&mut self, op: &str, args: &[Value], _line: u64) -> Reply {
        let (START, [agent, Value::Bool(boxed), Value::Bool(unasked), Value::Bool(check), Value::Text(model)]) = (op, args) else { return Reply::Failed(format!("{op} is not carried here as given")) };
        if *boxed {
            return Reply::Failed("a boxed start: Docket v2 has no box yet, and it is not started outside one".into());
        }
        let Some(mut p) = (self.profile)(agent) else { return Reply::Failed(format!("no profile for {agent}")) };
        (p.switches.unasked, p.switches.safety_check, p.model) = (*unasked, *check, model.clone());
        if let Err(why) = launch(&p, &self.relay).and_then(|l| (self.spawn)(agent, &p, l)) {
            return Reply::Failed(why);
        }
        let event = |ty: &str, args| Instance { ty: ty.into(), args };
        // Harness, Version and Mode are each `Identified by String`: an event's field holds the string itself
        let mut events = vec![event(STARTED, vec![agent.clone(), Value::text(&p.harness), Value::text(&p.version)])];
        events.extend(p.mode.iter().map(|m| event(MODE, vec![agent.clone(), Value::text(m)])));
        Reply::Answer { value: Value::Unit, costs: vec![], events }
    }

    /// Every start is answered at once, so no run rests here.
    fn dropped(&mut self, _line: u64) {}
}

#[cfg(test)]
mod tests;
