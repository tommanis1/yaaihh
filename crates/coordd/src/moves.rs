//! coordd's own moves of an agent between views (N-101 p406, C; the joint proposal with docket-core2, N-229). An
//! agent stands in one view at a time, at the project's path: a layer's (`agents/NAME/layer` names it), the line's
//! read-only view (`line@SHA`, layer.rs `line_view`), or none (the project's own folder). coordd moves it, once out of
//! its turn and never in one:
//!
//! - to the layer of a step it is given, when it has no open step where it stands (one in that view; in the project's
//!   folder, one in no layer); while it has, the step is handed with the note line (`note`) and it stays;
//! - out of a layer that has ended, to the line's view, so the layer's end is not held by it;
//! - out of the line's view of an older line, when the line has moved (a part landed), to its view as it is now;
//! - after coordd restarts, an agent that ran in a view is started again in it, or in the line's view if it has gone.
//!
//! - to the layer it asked for with `coord go ID` (N-101 p414), when the turn that asked has ended: that turn answers
//!   no step (its last message is a post), and its choice is kept as `agents/NAME/go`, then `agents/NAME/going` once
//!   the turn's end is taken, so a restart between them keeps it.
//!
//! A move: what it leaves uncommitted listed (`layer::uncommitted`, E), its session closed (`roster down`), what it
//! left running ended (`cgroup::kill_left`), and started again on its own conversation in the new view and its box
//! (`main::launch`, which readies the box: `sandbox::ready`); recorded as a `moved` line; in a layer, told where it is
//! and what it holds there (`arrived`). A turn's end answers only a step in the layer the agent stood in, or in none
//! (the machine's `stood`); another layer's step is answered with `coord answer`.

use crate::core::{Core, Job};

/// What `launch` takes for the line's view as it is now (`layer::line_view` makes or finds it).
pub const LINE: &str = "line@";

/// Where agent NAME stands: the view it was last started in (`agents/NAME/layer`), none for the project's own folder.
pub fn stand(coord: &str, name: &str) -> Option<String> {
    std::fs::read_to_string(format!("{coord}/agents/{name}/layer")).ok().map(|l| l.trim().to_string()).filter(|l| !l.is_empty())
}

/// How a place reads in a line: a layer by its id, the line's view as that, none as the project's own folder.
pub fn shown(at: Option<&str>) -> String {
    match at {
        None => String::from("the project's own folder"),
        Some(l) if crate::layer::is_line(l) => format!("the line's read-only view ({l})"),
        Some(l) => format!("layer {l}"),
    }
}

/// The note line of a step in layer L handed to an agent standing at STAND, another place (paper-research N-101 p410,
/// p414), `note.tmpl` of PLACES: where the step is, where it stands (`own`, `line` or `at`), and whether `coord go L`
/// moves it there (MOVED: an agent coordd moves, Claude Code or Codex). None when its renderings have no note.
pub fn note(places: &crate::policy::Places, l: &str, stand: Option<&str>, moved: bool) -> Option<String> {
    let line = stand.filter(|s| crate::layer::is_line(s));
    let at = stand.filter(|s| !crate::layer::is_line(s));
    let env = serde_json::json!({"layer": l, "own": stand.is_none(), "line": line, "at": at, "moved": moved});
    crate::render::template_in(places, "note", &env).map_err(|e| eprintln!("coordd: moves: rendering the note of layer {l}: {e}")).ok().flatten()
}

/// A move coordd is to make now: AGENT to TO (a layer's id, or `LINE`), for the step of ticket T if it is one, WHY;
/// RESUME: the agent does not run (coordd restarted), so there is no session to close.
#[derive(Clone, Debug, PartialEq)]
pub struct Move {
    pub agent: String,
    pub to: String,
    pub t: Option<u64>,
    pub why: String,
    pub resume: bool,
}

/// What `move_due` reads of the machine, the same for every agent: the layers active now, and each agent's steps owed
/// or held, in ticket order, with the layer each runs in. Read once a round by `to_move` for all its agents, and only
/// once one of them may be moved: each agent's `move_due` walked every ticket three times for it, every second under
/// the lock (docket-edge5, N-1446 p2899, p2903).
pub struct Reads {
    active: Vec<String>,
    steps: std::collections::HashMap<String, Vec<(u64, Option<String>)>>,
}

impl Reads {
    /// M's reads: one walk for the active layers, one for the steps.
    pub fn of(m: &docket_machine::Machine) -> Reads {
        let active = m.active_layers().into_iter().map(|(l, _, _)| l).collect();
        let mut steps: std::collections::HashMap<String, Vec<(u64, Option<String>)>> = std::collections::HashMap::new();
        for (agent, t, _) in m.steps_awaited() {
            steps.entry(agent.to_string()).or_default().push((t, m.layer_of(t)));
        }
        Reads { active, steps }
    }
}

impl Core {
    /// Where agent NAME is to be moved now, if anywhere: a Claude Code agent of the registry (a step's agent only to its
    /// step's layer), running, out of its turn, nothing on its way to it,
    /// no move of it running.
    pub fn move_due(&self, name: &str) -> Option<Move> {
        self.move_due_in(name, &std::cell::OnceCell::new())
    }

    /// `move_due` with the machine's READS shared, read the first time an agent may be moved.
    fn move_due_in(&self, name: &str, reads: &std::cell::OnceCell<Reads>) -> Option<Move> {
        let a = self.agents.get(name)?;
        // started again on its own conversation: one with no session kept is not moved
        let kept = std::path::Path::new(&format!("{}/agents/{name}/session.json", self.coord)).is_file();
        // a Claude Code or Codex agent: roster ends its session and resumes the same conversation in the new view
        // (a Codex agent's app-server is started in the view, since it runs the agent's commands)
        if !matches!(a.kind.as_str(), "claude" | "codex") || !kept || !a.running() || self.moving.contains(name) || self.inflight.contains_key(name) || self.tickets.stopping.contains(name) {
            return None;
        }
        if a.live.state == "working" && a.ended_turn != a.live.turn {
            return None;
        }
        let m = &self.tickets.m;
        let stand = stand(&self.coord, name);
        let Reads { active, steps } = reads.get_or_init(|| Reads::of(m));
        // its own choice (`coord go`), once the turn that asked has ended
        if let Some((l, t)) = going(&self.coord, name, "going").filter(|(l, _)| active.contains(l) && stand.as_ref() != Some(l)) {
            return Some(Move { agent: name.into(), to: l, t: Some(t), why: String::from("coord go"), resume: false });
        }
        let steps: &[(u64, Option<String>)] = steps.get(name).map_or(&[], Vec::as_slice);
        // open work where it stands: it stays, and is handed what else it is given with the note line
        if steps.iter().any(|(_, l)| *l == stand) {
            return None;
        }
        if let Some((t, l)) = steps.iter().find_map(|(t, l)| l.as_ref().filter(|l| active.contains(l)).map(|l| (*t, l.clone()))) {
            return Some(Move { agent: name.into(), why: format!("given a step of {} in it", m.id(t)), to: l, t: Some(t), resume: false });
        }
        // a step's agent is moved only to the layer of a step it owes: a piece's minder lives on between its steps, and
        // one started where its step is not (the owner's `coord start`, N-101 p1778) goes to it; it is never sent to
        // the line's view, its layer ending with its block
        if crate::core::is_fresh(name) {
            return None;
        }
        match stand {
            // its layer has ended: out to the line's view
            Some(s) if !crate::layer::is_line(&s) && !active.contains(&s) => Some(Move { agent: name.into(), to: LINE.into(), t: None, why: format!("layer {s} ended"), resume: false }),
            // it stands in the line's view of an older line (a part landed since it was started there): to the line's
            // view as the line is now, so it reads and plans on what the line holds (paper-research N-101 p1528)
            Some(s) if crate::layer::is_line(&s) && self.line_now.as_ref().is_some_and(|now| *now != s) => Some(Move { agent: name.into(), to: LINE.into(), t: None, why: String::from("the line moved"), resume: false }),
            _ => None,
        }
    }

    /// The moves to make now, each marked as running (`moving`) so it is asked once: the agents to resume after a
    /// restart (`resuming`), then every agent with a move due.
    pub fn to_move(&mut self) -> Vec<Move> {
        let mut out = Vec::new();
        // the machine's reads, once for the round, when something needs them (`Reads`)
        let reads = std::cell::OnceCell::new();
        let m = &self.tickets.m;
        let active = || &reads.get_or_init(|| Reads::of(m)).active;
        for name in std::mem::take(&mut self.resuming) {
            let Some(s) = stand(&self.coord, &name) else { continue };
            let (to, why) = if active().contains(&s) { (s.clone(), String::from("coordd restarted")) } else { (LINE.into(), format!("coordd restarted; {} is gone", shown(Some(&s)))) };
            out.push(Move { agent: name, to, t: None, why, resume: true });
        }
        let names: Vec<String> = self.agents.keys().cloned().collect();
        // a `coord go` to a layer that has ended meanwhile: dropped
        for n in &names {
            if going(&self.coord, n, "going").is_some_and(|(l, _)| !active().contains(&l)) {
                let _ = std::fs::remove_file(format!("{}/agents/{n}/going", self.coord));
            }
        }
        out.extend(names.iter().filter_map(|n| self.move_due_in(n, &reads)));
        for m in &out {
            self.moving.insert(m.agent.clone());
            // a move starts it again: not started for a step as well while it comes up (`to_start_owing`)
            self.started_owing.insert(m.agent.clone(), (std::time::Instant::now(), None));
        }
        out
    }

    /// Send each move due now to the senders (`main::senders`; no jobs, as at the load and in tests: none).
    pub fn send_moves(&mut self) {
        let Some(jobs) = self.jobs.clone() else { return };
        for m in self.to_move() {
            let _ = jobs.send(Job::Move(m));
        }
    }
}

/// What a move did, for its line: what was left uncommitted where it stood, and what it left running, ended.
#[derive(Default)]
pub struct Done {
    pub left: Vec<String>,
    pub killed: Vec<String>,
}

/// Make move M (Job::Move): see the module's words. Where the agent stood is read now; a move no longer due (it began
/// a turn meanwhile, or was given work where it stands) is not made. A failure is coordd's log's, and the agent is
/// started again where it stood.
pub async fn run(shared: &crate::Shared, m: Move) {
    let (coord, base, from, model, dir) = {
        let core = shared.lock().await;
        let still = m.resume || core.agents.get(&m.agent).is_some_and(|a| a.running() && !(a.live.state == "working" && a.ended_turn != a.live.turn) && !core.inflight.contains_key(&m.agent));
        if !still {
            drop(core);
            shared.lock().await.moving.remove(&m.agent);
            return;
        }
        let a = &core.agents[&m.agent];
        let dir = a.session.get("cwd").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
        (core.coord.clone(), core.base.clone(), stand(&core.coord, &m.agent), a.model.clone(), dir)
    };
    let mut done = Done::default();
    // E: what it leaves uncommitted where it stood (an open layer's view; the line's is read-only)
    if let Some(f) = from.clone().filter(|f| !crate::layer::is_line(f)) {
        let c2 = coord.clone();
        done.left = tokio::task::spawn_blocking(move || crate::layer::uncommitted(&c2, &f)).await.unwrap_or_else(|e| Err(e.to_string())).unwrap_or_else(|e| vec![format!("(not read: {e})")]);
        // the warning for a turn's end that answered there is given at the next delivery, when the layer may have
        // ended and its view gone (an answer that ends its ticket ends the layer: moves-try run 5): it says what this
        // read
        keep_left(&coord, &m.agent, from.as_deref().unwrap_or_default(), &done.left);
    }
    if !m.resume {
        match tokio::process::Command::new(format!("{base}/bin/roster")).args(["down", &m.agent]).stdin(std::process::Stdio::null()).output().await {
            Ok(o) if o.status.success() => {}
            Ok(o) => return failed(shared, &m, &format!("roster down failed: {}", String::from_utf8_lossy(&o.stderr).trim()), None).await,
            Err(e) => return failed(shared, &m, &format!("roster down: {e}"), None).await,
        }
    }
    // what it started and left behind would hold the old view open, and a layer's end would wait on it
    let a2 = m.agent.clone();
    done.killed = tokio::task::spawn_blocking(move || crate::cgroup::kill_left(&a2)).await.unwrap_or_default();
    let what = format!("moved to {} ({})", shown(Some(&m.to)), m.why);
    let started = crate::launch(shared, crate::Launch { agent: m.agent.clone(), model: model.clone(), dir: dir.clone(), prompt: String::new(), layer: Some(m.to.clone()), from: None, cut: None, what, force: false }).await;
    if let Err(e) = started {
        return failed(shared, &m, &e, Some((from, model, dir))).await;
    }
    let to = stand(&coord, &m.agent);
    // into the line's view now: the older ones nobody stands in go, and their bases with them (the current one is
    // kept, so a move into it meanwhile does not lose it)
    if let Some(keep) = to.clone().filter(|l| crate::layer::is_line(l)) {
        let c2 = coord.clone();
        for said in tokio::task::spawn_blocking(move || crate::layer::line_tidy(&c2, &keep)).await.unwrap_or_default() {
            eprintln!("coordd: layers: {said}");
        }
    }
    let _ = std::fs::remove_file(format!("{coord}/agents/{}/going", m.agent));
    let mut core = shared.lock().await;
    let said = to.clone().filter(|l| !crate::layer::is_line(l)).and_then(|l| core.arrived(&m.agent, &l, &dir));
    let input = docket_machine::Input::Moved { t: m.t, agent: m.agent.clone(), from, to, why: m.why.clone(), killed: done.killed, left: done.left };
    if let Err(e) = core.ticket_input(input) {
        eprintln!("coordd: moves: {}: its move not recorded: {e}", m.agent);
    }
    // where it now is: told, unless its step there, given before, is handed again (`sN.back`), which begins with it
    // (main's `rendered_each`; paper-research N-101 p629)
    let back = core.ticket_due(&m.agent).iter().any(|d| d.id.ends_with(".back"));
    if let Some(said) = said.filter(|_| !back) {
        core.note_from_coord(&m.agent, &said);
    }
    core.moving.remove(&m.agent);
    core.changed(&m.agent);
}

/// Move M failed for WHY: said in coordd's log; when its session was closed, started again where it stood (BACK: that
/// view, its model and folder), so it is not left down.
async fn failed(shared: &crate::Shared, m: &Move, why: &str, back: Option<(Option<String>, String, String)>) {
    eprintln!("coordd: moves: {} not moved to {}: {why}", m.agent, shown(Some(&m.to)));
    if let Some((f, model, dir)) = back {
        let layer = f.map(|f| if crate::layer::is_line(&f) { LINE.to_string() } else { f });
        let what = format!("again where it stood, its move to {} failed", shown(Some(&m.to)));
        if let Err(e) = crate::launch(shared, crate::Launch { agent: m.agent.clone(), model, dir, prompt: String::new(), layer, from: None, cut: None, what, force: false }).await {
            eprintln!("coordd: moves: {} not started again where it stood: {e}", m.agent);
        }
    }
    shared.lock().await.moving.remove(&m.agent);
}

// ---- the commit check (N-101 p406) -------------------------------------------------------------------------------

/// Whether AGENT is the program's own worker (the runner, coordd's watcher and minder), not an agent: its answers'
/// commits are the line's (the runner's `OnLine(top)`), so neither the layer's check nor its completion applies.
fn programs_own(agent: &str) -> bool {
    agent == docket_machine::runner::RUNNER || docket_machine::coordds_own(agent)
}

/// Whether commit SHA of the repository at PATH is on BRANCH (it or one of its ancestors); else why not, or none when
/// BRANCH is not there to look at, which is coordd's to log and not the agent's fault.
fn on_branch(path: &str, sha: &str, branch: &str) -> Result<(), Option<String>> {
    let git = |args: &[&str]| std::process::Command::new("git").args(["-c", "safe.directory=*", "-C", path]).args(args).stderr(std::process::Stdio::null()).output();
    let full = format!("refs/heads/{branch}");
    if !git(&["rev-parse", "--verify", "-q", &full]).is_ok_and(|o| o.status.success()) {
        return Err(None);
    }
    if !git(&["rev-parse", "--verify", "-q", &format!("{sha}^{{commit}}")]).is_ok_and(|o| o.status.success()) {
        return Err(Some(format!("{sha} is no commit of {path}")));
    }
    match git(&["merge-base", "--is-ancestor", sha, &full]) {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) if o.status.code() == Some(1) => Err(Some(format!("{sha} is not on {branch}, the branch of the step's layer (commit in the layer's view, where you stand when you work on it)"))),
        Ok(o) => Err(Some(format!("git merge-base --is-ancestor {sha} {branch} in {path}: {}", String::from_utf8_lossy(&o.stderr).trim()))),
        Err(e) => Err(Some(format!("git in {path}: {e}"))),
    }
}

impl Core {
    /// INPUT, or, when it is an answer (given or formed) to a step run in a layer one of whose commits (a `Ref` field
    /// of its constructor: a commit, or commits by repository) is not on the layer's branch in its repository, its
    /// refusal for that (`Input::Unfit`; the third makes the step Malformed). A commit made in a boxed layer reaches
    /// the project's repositories first (`layer::sync`).
    pub fn off_layer(&self, input: docket_machine::Input) -> docket_machine::Input {
        use docket_machine::{Input, Status, Type};
        let (t, head, by, step, call) = match &input {
            Input::Answer { t, head, by, .. } => (*t, head, Some(by), None, None),
            Input::Formed { t, head: Some(head), step, call, .. } => (*t, head, None, Some(*step), call.clone()),
            _ => return input,
        };
        let Some(x) = self.tickets.m.tickets.get(&t).filter(|x| x.is_open()) else { return input };
        let Status::Awaits { owed, agent, forming, .. } = &x.status else { return input };
        let takes = match (by, step) {
            (Some(by), _) => by == agent,
            (None, Some(step)) => step == owed.step && forming.is_some() && x.owner_words.is_none(),
            _ => false,
        };
        // a step the program holds (the runner's hand_in, accepted, cut; coordd's own) is not checked: what it answers
        // is the line's, never a commit of the layer's branch (docket-core3, N-101: run 1's hand-in refused)
        if owed.hold || !takes || programs_own(agent) {
            return input;
        }
        let Some(layer) = self.tickets.m.layer_of(t) else { return input };
        let Type::Variant(cs) = &owed.head else { return input };
        let Some(fields) = head.get("head").and_then(serde_json::Value::as_str).and_then(|h| cs.iter().find(|(c, _)| c == h)).map(|(_, f)| f) else { return input };
        let Some(p) = self.project.as_ref() else { return input };
        let main = p.repo().unwrap_or_default();
        let repos = p.repos();
        let path_of = |name: &str| repos.iter().find(|(n, _)| n == name).map(|(_, r)| r.clone()).or_else(|| (repos.is_empty() || p.layer_repos().0 == name).then(|| main.clone()));
        // each commit an answer names, with the repository it is of
        let mut commits: Vec<(String, String)> = Vec::new();
        for (f, ty) in fields {
            if !matches!(ty, Type::Ref) && !matches!(ty, Type::Opt(t) if **t == Type::Ref) {
                continue;
            }
            match head.get(f) {
                Some(serde_json::Value::String(sha)) if !main.is_empty() => commits.push((main.clone(), sha.clone())),
                Some(serde_json::Value::Object(o)) => commits.extend(o.iter().filter_map(|(n, v)| Some((path_of(n)?, v.as_str()?.to_string())))),
                _ => {}
            }
        }
        if commits.is_empty() {
            return input;
        }
        if let Err(e) = crate::layer::sync(&self.coord, &layer) {
            eprintln!("coordd: layers: {layer}: {e}");
        }
        let branch = crate::runner::layer_branch(&layer);
        for (path, sha) in commits {
            match on_branch(&path, &sha, &branch) {
                Ok(()) => {}
                Err(None) => eprintln!("coordd: layers: {path} has no branch {branch}; {sha} not checked"),
                Err(Some(why)) => return Input::Unfit { t, step: owed.step, why, call },
            }
        }
        input
    }

    /// W1 (paper-research N-101 p466): INPUT, or, when it is an answer (given or formed) to a step run in a layer, with
    /// each commit it gives (a `Ref` field of its constructor: a bare commit, or commits by repository) completed
    /// against the layer, as `completed_on_layer` lays one over `layer on X`: every repository of the layer whose layer
    /// branch moved past the layer's base there, and that the answer does not name, goes in with that branch's head.
    /// The record keeps the whole set; the commit check (`off_layer`) reads it after. A commit made in a boxed layer
    /// reaches the project's repositories first (`layer::sync`).
    pub fn completed_in_layer(&self, input: docket_machine::Input) -> docket_machine::Input {
        use docket_machine::{Input, Status, Type};
        let (t, head) = match &input {
            Input::Answer { t, head, .. } => (*t, head),
            Input::Formed { t, head: Some(head), .. } => (*t, head),
            _ => return input,
        };
        let Some(layer) = self.tickets.m.layer_of(t) else { return input };
        let Some(Status::Awaits { owed, agent, .. }) = self.tickets.m.tickets.get(&t).filter(|x| x.is_open()).map(|x| &x.status) else { return input };
        // the program's own step answers the line's commits, not the layer's: nothing of the layer goes in
        if programs_own(agent) {
            return input;
        }
        let Type::Variant(cs) = &owed.head else { return input };
        let Some(fields) = head.get("head").and_then(serde_json::Value::as_str).and_then(|h| cs.iter().find(|(c, _)| c == h)).map(|(_, f)| f) else { return input };
        let refs: Vec<&String> = fields.iter().filter(|(f, ty)| (matches!(ty, Type::Ref) || matches!(ty, Type::Opt(t) if **t == Type::Ref)) && head.get(f).is_some_and(|v| v.is_string() || v.is_object())).map(|(f, _)| f).collect();
        if refs.is_empty() {
            return input;
        }
        if let Err(e) = crate::layer::sync(&self.coord, &layer) {
            eprintln!("coordd: layers: {layer}: {e}");
        }
        let place = self.runner_place(t);
        let moved = moved_in(&self.coord, &layer, place.main_repo.as_deref().unwrap_or_default());
        if moved.is_empty() {
            return input;
        }
        let mut out = head.clone();
        for f in refs {
            let given = match &out[f.as_str()] {
                serde_json::Value::String(sha) => place.keyed(sha).and_then(|v| v.as_object().cloned()),
                serde_json::Value::Object(o) => Some(o.clone()),
                _ => None,
            };
            // a bare commit no repository can be found for is left as it is (the commit check says so)
            let Some(mut given) = given else { continue };
            let n = given.len();
            for (name, sha) in &moved {
                given.entry(name.clone()).or_insert_with(|| serde_json::Value::String(sha.clone()));
            }
            if given.len() > n {
                out[f.as_str()] = serde_json::Value::Object(given);
            }
        }
        match input {
            Input::Answer { t, by, text, .. } => Input::Answer { t, by, head: out, text },
            Input::Formed { t, step, why, not_taken, call, .. } => Input::Formed { t, step, head: Some(out), why, not_taken, call },
            i => i,
        }
    }

    /// E: the warning an answer by agent ME gets when it leaves uncommitted changes in the layer it stands in (read
    /// off the lock: `layer::uncommitted` runs git in the view). Empty when it stands in none, or leaves nothing.
    pub fn uncommitted_warning(coord: &str, me: &str) -> String {
        let Some(l) = stand(coord, me).filter(|l| !crate::layer::is_line(l)) else { return String::new() };
        match crate::layer::uncommitted(coord, &l) {
            Ok(left) if !left.is_empty() => format!(
                "\nWarning: you leave changes uncommitted in layer {l}: {}. They stay in the layer's view, where nobody else reads them; commit them (or remove them) if they are part of this work.",
                left.join(", ")
            ),
            Ok(_) => String::new(),
            Err(e) => {
                eprintln!("coordd: layers: {l}: what {me} leaves uncommitted not read: {e}");
                String::new()
            }
        }
    }
}

/// The warning for an answer given by ending the turn (paper-research N-101 p466, 2): the turn's end of NAME answered
/// a step while it stood in layer L, so `agents/NAME/answered` keeps L until its next delivery, whose first line the
/// warning is (`turn_warning`). Not kept for the line's view (read-only) or the project's folder.
pub fn mark_answered(coord: &str, name: &str, stood: Option<&str>) {
    let Some(l) = stood.filter(|l| !l.is_empty() && !crate::layer::is_line(l)) else { return };
    let d = format!("{coord}/agents/{name}");
    let _ = std::fs::create_dir_all(&d);
    if let Err(e) = std::fs::write(format!("{d}/answered"), format!("{l}\n")) {
        eprintln!("coordd: moves: {name}: the layer its turn's end answered in not kept: {e}");
    }
}

/// NAME's turn given its deliveries ended: the warning they began with is said (`turn_warning`).
pub fn warned(coord: &str, name: &str) {
    let _ = std::fs::remove_file(format!("{coord}/agents/{name}/answered"));
}

/// A move of NAME out of layer FROM read LEFT there: kept with the layer its turn's end answered in, when that is FROM,
/// for the warning (`turn_warning`).
pub fn keep_left(coord: &str, name: &str, from: &str, left: &[String]) {
    let file = format!("{coord}/agents/{name}/answered");
    let Ok(kept) = std::fs::read_to_string(&file) else { return };
    if left.is_empty() || kept.lines().next().map(str::trim) != Some(from) {
        return;
    }
    if let Err(e) = std::fs::write(&file, format!("{from}\n{}\n", left.join("\n"))) {
        eprintln!("coordd: moves: {name}: what it left in {from} not kept for its warning: {e}");
    }
}

/// The warning NAME's next delivery begins with, when a turn's end of it answered a step in a layer (`mark_answered`)
/// and left changes uncommitted there: as a move out of it read them, else as they are now; and the file it came
/// from, which the end of the next turn given something removes (`warned`). Runs git in the view: off the lock.
pub fn turn_warning(places: &crate::policy::Places, name: &str) -> Option<(String, String)> {
    let coord = places.coord.as_str();
    let file = format!("{coord}/agents/{name}/answered");
    let kept = std::fs::read_to_string(&file).ok()?;
    let mut lines = kept.lines();
    let l = lines.next().unwrap_or_default().trim().to_string();
    // as a move out of it read them (`keep_left`), else as they are now
    let read: Vec<String> = lines.map(str::to_string).filter(|f| !f.is_empty()).collect();
    let left = if !read.is_empty() {
        read
    } else {
        match crate::layer::uncommitted(coord, &l) {
            Ok(left) => left,
            Err(e) => {
                // a layer that has ended has no view: nothing is left to warn about
                eprintln!("coordd: layers: {l}: what {name}'s turn left uncommitted not read: {e}");
                vec![]
            }
        }
    };
    let said = if left.is_empty() {
        String::new()
    } else {
        let env = serde_json::json!({"layer": l, "left": left.join(", ")});
        crate::render::template_in(places, "warning", &env).map_err(|e| eprintln!("coordd: moves: {name}: rendering its warning: {e}")).ok().flatten().unwrap_or_default()
    };
    Some((said, file))
}

#[cfg(test)]
mod tests;

/// Of layer L's repositories (its own, named MAIN, and its parts by their names), those whose layer branch moved past
/// the layer's base there, by name, with the branch's head. None for a layer not in the registry.
pub fn moved_in(coord: &str, l: &str, main: &str) -> Vec<(String, String)> {
    let Some(x) = crate::layer::load(coord).remove(l) else { return vec![] };
    let mut each = vec![(main.to_string(), x.repo.clone(), x.base.clone(), x.branch.clone())];
    each.extend(x.also.iter().map(|p| (p.name.clone(), p.repo.clone(), p.base.clone(), p.branch.clone())));
    let full = |repo: &str, what: &str| -> Option<String> {
        let o = std::process::Command::new("git").args(["-c", "safe.directory=*", "-C", repo, "rev-parse", "--verify", "-q", &format!("{what}^{{commit}}")]).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok()?;
        o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|s| !s.is_empty())
    };
    each.into_iter()
        .filter(|(name, ..)| !name.is_empty())
        .filter_map(|(name, repo, base, branch)| {
            let head = full(&repo, &format!("refs/heads/{branch}"))?;
            (Some(&head) != full(&repo, &base).as_ref()).then_some((name, head))
        })
        .collect()
}

// ---- coord go (N-101 p414) ---------------------------------------------------------------------------------------

/// The layer and ticket agent NAME asked to go to, kept in `agents/NAME/WHICH`: `go` while the turn that asked runs,
/// `going` once its end is taken.
pub fn going(coord: &str, name: &str, which: &str) -> Option<(String, u64)> {
    let s = std::fs::read_to_string(format!("{coord}/agents/{name}/{which}")).ok()?;
    let (l, t) = s.trim().split_once(' ')?;
    Some((l.to_string(), t.parse().ok()?))
}

/// The turn of NAME has ended: the layer it asked to go to in it, if any (its `go` becomes `going`, for the move).
pub fn turn_went(coord: &str, name: &str) -> Option<String> {
    let (l, _) = going(coord, name, "go")?;
    let d = format!("{coord}/agents/{name}");
    if let Err(e) = std::fs::rename(format!("{d}/go"), format!("{d}/going")) {
        eprintln!("coordd: moves: {name}: {d}/go: {e}");
    }
    Some(l)
}

impl Core {
    /// What AGENT holds, by layer: each step owed or held, (ticket, its id, call, layer), oldest first.
    fn steps_by_layer(&self, agent: &str) -> Vec<(u64, String, String, Option<String>)> {
        let m = &self.tickets.m;
        let mut out: Vec<_> = m.owes(agent).into_iter().chain(m.holds(agent)).map(|(t, _, call)| (t, m.id(t), call, m.layer_of(t))).collect();
        out.sort();
        out
    }

    /// `coord go` with no argument: where AGENT stands, and each step it holds with its layer.
    pub fn go_list(&self, agent: &str) -> String {
        let stand = stand(&self.coord, agent);
        let mut out = format!("You stand in {}.", shown(stand.as_deref()));
        if let Some((l, _)) = going(&self.coord, agent, "go").or_else(|| going(&self.coord, agent, "going")) {
            out.push_str(&format!(" You go to layer {l} when this turn ends."));
        }
        let steps = self.steps_by_layer(agent);
        if steps.is_empty() {
            out.push_str("\nYou hold no step.");
        }
        for (_, id, call, l) in steps {
            let at = match &l {
                None => String::from("no layer (answered from anywhere)"),
                Some(l) if Some(l) == stand.as_ref() => format!("layer {l}, where you stand"),
                Some(l) => format!("layer {l} (coord go {l})"),
            };
            out.push_str(&format!("\n  {id} {call}: {at}"));
        }
        out
    }

    /// `coord go [ID]` by WHO: the list, or the move to layer ID (a layer's id, or a ticket's, whose layer it is) at the
    /// end of the running turn. Refused, with the list, for a layer where it holds no step.
    pub fn go(&mut self, who: &crate::core::Who, to: Option<String>) -> coord_types::Reply {
        use crate::core::{err, ok};
        let crate::core::Who::Agent(me) = who else { return err("coord go: an agent's; it moves the agent that runs it".into()) };
        if self.agents.get(me).is_none_or(|a| !matches!(a.kind.as_str(), "claude" | "codex")) {
            return err("coord go: only a Claude Code or Codex agent is moved between views".into());
        }
        let Some(to) = to else { return ok(self.go_list(me)) };
        let m = &self.tickets.m;
        let active: Vec<String> = m.active_layers().into_iter().map(|(l, _, _)| l).collect();
        let layer = if active.contains(&to) { Some(to.clone()) } else { docket_machine::parse_ticket(&to).filter(|t| m.tickets.get(t).is_some_and(|x| m.id(x.t) == to)).and_then(|t| m.layer_of(t)) };
        let Some(l) = layer.filter(|l| active.contains(l)) else { return err(format!("coord go: {to} is no open layer, nor a ticket in one.\n{}", self.go_list(me))) };
        let stand = stand(&self.coord, me);
        if stand.as_ref() == Some(&l) {
            return err(format!("coord go: you stand in {l} already.\n{}", self.go_list(me)));
        }
        let Some((t, id, call, _)) = self.steps_by_layer(me).into_iter().find(|(_, _, _, x)| x.as_ref() == Some(&l)) else {
            return err(format!("coord go: you hold no step in layer {l}, so you are not moved there.\n{}", self.go_list(me)));
        };
        let d = format!("{}/agents/{me}", self.coord);
        // the agent runs it, so in a turn: the move is made when that turn ends (or is cut), never at once. Not by the
        // state coordd follows: a turn a channel message began may not read as working yet, and a move then cut the
        // turn that asked, before its last words (moves-try run 5, docket-try2)
        if let Err(e) = std::fs::create_dir_all(&d).and_then(|_| std::fs::write(format!("{d}/go"), format!("{l} {t}\n"))) {
            return err(format!("coord go: {d}/go: {e}"));
        }
        let _ = std::fs::remove_file(format!("{d}/going"));
        let here = match stand.as_deref() {
            None => String::from("the project's own folder"),
            Some(s) => s.to_string(),
        };
        ok(format!(
            "Going to layer {l} ({id}, your step {call}). The move is made when this turn ends: end your turn now. Nothing more you do in this turn is in {l}. Your work in {here} stays there as you leave it, uncommitted edits included."
        ))
    }

    /// What AGENT is told after a move into layer L at DIR (the project's path), `arrived.tmpl` of its renderings: where
    /// it is, its step there, and its other steps with their layers. None when its renderings have none.
    pub fn arrived(&self, agent: &str, l: &str, dir: &str) -> Option<String> {
        let m = &self.tickets.m;
        let steps = self.steps_by_layer(agent);
        let ticket = m.active_layers().into_iter().find(|(x, _, _)| x == l).map(|(_, t, _)| m.id(t)).unwrap_or_default();
        let gist = |t: u64| -> String {
            let Some(docket_machine::Status::Awaits { owed, .. }) = m.tickets.get(&t).map(|x| &x.status) else { return String::new() };
            let args = crate::render::resolved(&owed.args);
            let first = args.as_object().and_then(|o| o.values().find_map(|v| v.as_str().map(str::to_string))).unwrap_or_default();
            let first = first.lines().next().unwrap_or_default().to_string();
            if first.is_empty() {
                String::new()
            } else if first.chars().count() > 60 {
                format!(", \"{}…\"", first.chars().take(60).collect::<String>())
            } else {
                format!(", \"{first}\"")
            }
        };
        let here: Vec<String> = steps.iter().filter(|(_, _, _, x)| x.as_deref() == Some(l)).map(|(t, id, call, _)| format!("{call} on {id}{}", gist(*t))).collect();
        let other: Vec<String> = steps
            .iter()
            .filter(|(_, _, _, x)| x.as_deref() != Some(l))
            .map(|(_, id, call, x)| format!("{} ({id}, {call})", x.as_deref().unwrap_or("no layer")))
            .collect();
        let phrase = |xs: Vec<String>| Some(xs.join("; ")).filter(|x| !x.is_empty());
        let env = serde_json::json!({"layer": l, "ticket": ticket, "dir": dir, "no_here": here.is_empty(), "here": phrase(here), "no_other": other.is_empty(), "other": phrase(other)});
        crate::render::template_in(&self.places_for(agent), "arrived", &env).map_err(|e| eprintln!("coordd: moves: {agent}: rendering where it arrived: {e}")).ok().flatten()
    }
}
