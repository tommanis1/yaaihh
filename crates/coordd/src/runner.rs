//! coordd's half of the runner's contract (plan/tovrin-program/runner-contract.md, section "Agreed with the runner's
//! builder"; N-101, N-97): what a step owed to the runner goes out as, beyond the call's own arguments.
//! `project::to_runner` runs the command; this file says what its JSON is.

use crate::core::{Core, Who, err, ok};
use coord_types::Reply;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

/// Where a step owed to the runner stands, as coordd knows it beside the step's arguments: the layer branch of its
/// ticket (`layer/ID`, the layer of the step's scope) and the name among `repos` of the project's own repository.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Place {
    pub branch: Option<String>,
    pub main_repo: Option<String>,
    /// The project's `root`, the agents' line: every step coordd gives the runner carries it as `line`, so the program
    /// names no line and the runner cuts this one, refusing a step whose line is not its own base (N-159 posts 25, 27).
    pub line: Option<String>,
    /// The project's repositories, each NAME and path (`repos`): in a project of several, a bare commit is the one
    /// repository's that holds it (N-159 post 62).
    pub repos: Vec<(String, String)>,
    /// The work the step is for (N-101 p166): the nearest ticket, the step's own or one it is under, of a kind the
    /// project wrote, never a par's branch, which is the engine's own; every step coordd gives the runner carries it
    /// as `of` (`Machine::work_of`).
    pub of: Option<String>,
}


impl Place {
    /// Bare commit SHA by repository: with several repositories, `{REPO: FULL}` for the one repository that holds it,
    /// FULL its full name there (`git rev-parse --verify`: a short prefix may become ambiguous later in the record); none
    /// or several, it stays a string, which the runner refuses. With one, the project's own repository's, as given.
    /// None: no name known (it stays a string).
    pub fn keyed(&self, sha: &str) -> Option<Value> {
        if self.repos.len() > 1 {
            let full = |path: &str| {
                let o = std::process::Command::new("git").args(["-c", "safe.directory=*", "-C", path, "rev-parse", "--verify", "-q", &format!("{sha}^{{commit}}")]).stderr(std::process::Stdio::null()).output().ok()?;
                o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|f| !f.is_empty())
            };
            let holders: Vec<(&String, String)> = self.repos.iter().filter_map(|(n, path)| full(path).map(|f| (n, f))).collect();
            return match holders.as_slice() {
                [(one, f)] => Some(json!({ (*one).clone(): f })),
                _ => None,
            };
        }
        self.main_repo.as_ref().filter(|m| !m.is_empty()).map(|m| json!({ m.clone(): sha }))
    }
}

/// The branch of layer ID, as layer.rs names it in every repository.
pub fn layer_branch(id: &str) -> String {
    format!("layer/{id}")
}

impl crate::core::Core {
    /// Where ticket T's step owed to the runner stands (`Place`): the layer of its scope, and the project's own
    /// repository's name: its name among `repos`, else (a project of one repository that names none) its folder's
    /// last part. The runner keys commits by its own names for the repositories (yaaihh, N-97 post 35), so a project
    /// whose folder is named otherwise says the name in `repos = { NAME = PATH }`.
    pub fn runner_place(&self, t: u64) -> Place {
        let branch = self.tickets.m.layer_of(t).map(|id| layer_branch(&id));
        let main_repo = self.project.as_ref().and_then(|p| {
            let named = p.layer_repos().0;
            if !named.is_empty() {
                return Some(named);
            }
            p.repo().and_then(|r| std::path::Path::new(&r).file_name().map(|n| n.to_string_lossy().to_string()))
        });
        let line = self.project.as_ref().and_then(|p| p.project.root.clone());
        let repos = self.project.as_ref().map(|p| p.repos()).unwrap_or_default();
        let of = self.tickets.m.work_of(t);
        Place { branch, main_repo, line, repos, of }
    }
}

/// The JSON of step CALL with ARGS (by the call's parameter names, texts resolved) as the runner takes it:
/// - `hand_in(commit)`: `{"commits": {REPO: SHA, ...}, "branch": B}`, always by repository, in a project of one
///   repository too (paper-research and yaaihh, N-101 post 28, N-97 post 35). A commit set (an object) goes as it is;
///   a bare commit is the project's own repository's (`{MAIN: SHA}`), or in a project of several repositories the one
///   that holds it (`Place::keyed`); with no name known, or none or several holding it, it stays a string, which the
///   runner refuses (its refusal fails the hand to the runner, as any).
///   B is the layer branch of the step's ticket, left out when it is in no layer.
/// - every other call: its arguments as they are.
pub fn step_json(call: &str, args: &Value, place: &Place) -> Value {
    // built_green(commit) names the commits of a hand-in on the line, as the hand-in gave them (contract point h): a
    // bare commit is the project's own repository's there as well
    if call == "built_green" {
        let mut out = args.as_object().cloned().unwrap_or_default();
        if let Some(keyed) = args.get("commit").and_then(Value::as_str).and_then(|sha| place.keyed(sha)) {
            out.insert("commit".into(), keyed);
        }
        return Value::Object(out);
    }
    // accepted(piece) names the piece's commits by repository as well (N-159): a bare `piece.commit` is the
    // project's own repository's, as for built_green
    if call == "accepted" {
        let mut out = args.clone();
        if let Some(keyed) = args.pointer("/piece/commit").and_then(Value::as_str).and_then(|sha| place.keyed(sha)) {
            if let Some(piece) = out.get_mut("piece").and_then(Value::as_object_mut) {
                piece.insert("commit".into(), keyed);
            }
        }
        return out;
    }
    if call != "hand_in" {
        return args.clone();
    }
    let mut out = Map::new();
    let commit = args.get("commits").or_else(|| args.get("commit")).cloned().unwrap_or(Value::Null);
    let commits = match commit {
        Value::String(sha) => place.keyed(&sha).unwrap_or(Value::String(sha)),
        c => c,
    };
    out.insert("commits".into(), commits);
    // anything else the call carries goes along (a call of a project may add arguments)
    if let Value::Object(o) = args {
        for (k, v) in o.iter().filter(|(k, _)| !matches!(k.as_str(), "commit" | "commits" | "branch")) {
            out.insert(k.clone(), v.clone());
        }
    }
    match (args.get("branch"), &place.branch) {
        (Some(b), _) if !b.is_null() => {
            out.insert("branch".into(), b.clone());
        }
        (_, Some(b)) => {
            out.insert("branch".into(), json!(b));
        }
        _ => {}
    }
    Value::Object(out)
}

// ---- an answer in `layer on X` (N-159 posts 119, 125) ----------------------------------------------------------

/// The commits by repository X of the innermost layer T's step runs in, when that layer is `layer on X` with X commits
/// by repository (its frame keeps X as JSON text); None for a layer on a branch or the root, or none.
pub fn layer_on(m: &docket_machine::Machine, t: u64) -> Option<(String, Map<String, Value>)> {
    let mut u = t;
    loop {
        let x = m.tickets.get(&u)?;
        if let Some((id, on)) = x.k.iter().rev().find_map(|f| if let docket_machine::machine::Frame::Layer { id, on } = f { Some((id.clone(), on.clone())) } else { None }) {
            return on.and_then(|o| serde_json::from_str::<Value>(&o).ok()).and_then(|v| v.as_object().cloned()).map(|x| (id, x));
        }
        u = x.parent?;
    }
}

/// HEAD (an answer `{head, FIELD...}`) with each `Ref` field of its constructor among CS that names only some of X's
/// repositories, or is a bare commit KEY puts by repository, laid over X: each repository it names gets its commit, every
/// other keeps X's (tovrin-writer and paper-research, N-159 post 125: the fixer's commit on the build's cut). A field that
/// names a repository X does not, or a bare commit KEY cannot place, is left as it is.
pub fn complete_refs(head: &Value, cs: &[(String, Vec<(String, docket_machine::Type)>)], x: &Map<String, Value>, key: impl Fn(&str) -> Option<Value>) -> Value {
    use docket_machine::Type;
    let Some(fields) = head.get("head").and_then(Value::as_str).and_then(|h| cs.iter().find(|(c, _)| c == h)).map(|(_, f)| f) else { return head.clone() };
    let mut out = head.clone();
    for (f, ty) in fields {
        if !matches!(ty, Type::Ref) && !matches!(ty, Type::Opt(t) if **t == Type::Ref) {
            continue;
        }
        let given = match out.get(f) {
            Some(Value::String(sha)) => key(sha).and_then(|v| v.as_object().cloned()),
            Some(Value::Object(o)) => Some(o.clone()),
            _ => None,
        };
        let Some(given) = given.filter(|g| !g.is_empty() && g.keys().all(|k| x.contains_key(k))) else { continue };
        let mut whole = x.clone();
        whole.extend(given);
        out[f.as_str()] = Value::Object(whole);
    }
    out
}

impl Core {
    /// An answer to a step run in `layer on X` (`Input::Answer`, `Input::Formed`) with its commits laid over X
    /// (`complete_refs`), before the machine takes it: the record keeps the whole set. Any other input as it is.
    pub fn completed_on_layer(&self, input: docket_machine::Input) -> docket_machine::Input {
        use docket_machine::Input;
        match input {
            Input::Answer { t, by, head, text } => Input::Answer { t, by, head: self.complete_on_layer(t, head), text },
            Input::Formed { t, step, head: Some(head), why, not_taken, call } => Input::Formed { t, step, head: Some(self.complete_on_layer(t, head)), why, not_taken, call },
            i => i,
        }
    }

    fn complete_on_layer(&self, t: u64, head: Value) -> Value {
        let Some((layer, x)) = layer_on(&self.tickets.m, t) else { return head };
        let Some(docket_machine::Status::Awaits { owed, .. }) = self.tickets.m.tickets.get(&t).map(|x| &x.status) else { return head };
        let docket_machine::Type::Variant(cs) = &owed.head else { return head };
        let place = self.runner_place(t);
        let coord = self.coord.clone();
        // a commit made in a boxed layer reaches the project's repositories when the layer is brought in
        let key = |sha: &str| {
            place.keyed(sha).or_else(|| {
                if let Err(e) = crate::layer::sync(&coord, &layer) {
                    eprintln!("coordd: layers: {layer}: {e}");
                }
                place.keyed(sha)
            })
        };
        complete_refs(&head, cs, &x, key)
    }
}

impl Core {
    /// An answer whose commit is a bare hash, in a project of several repositories (paper-research N-101 p953, 4): the
    /// hash is looked up in each of them; found in exactly one, the Ref is that repository's (its full hash), and the
    /// answer goes on so; in none or in several, the answer is not read (Unfit: the reminder says which), as a model
    /// cannot tell. Only a hex hash of 7 to 40 characters: a branch's name stays as it is. Before a layer's commits
    /// are looked for, the layer is brought in, as `complete_on_layer` does.
    pub fn bare_refs(&self, input: docket_machine::Input) -> docket_machine::Input {
        use docket_machine::{Input, Status, Type};
        let (t, head, by, step, call) = match &input {
            Input::Answer { t, head, by, .. } => (*t, head.clone(), Some(by.clone()), None, None),
            Input::Formed { t, head: Some(head), step, call, .. } => (*t, head.clone(), None, Some(*step), call.clone()),
            _ => return input,
        };
        let Some(p) = self.project.as_ref() else { return input };
        let repos = p.repos();
        if repos.len() < 2 {
            return input;
        }
        let Some(x) = self.tickets.m.tickets.get(&t).filter(|x| x.is_open()) else { return input };
        let Status::Awaits { owed, agent, forming, .. } = &x.status else { return input };
        let takes = match (&by, step) {
            (Some(by), _) => by == agent,
            (None, Some(step)) => step == owed.step && forming.is_some() && x.owner_words.is_none(),
            _ => false,
        };
        // the program's own workers (the runner, coordd's) answer the line's commits keyed already
        if owed.hold || !takes || agent == docket_machine::runner::RUNNER || docket_machine::coordds_own(agent) {
            return input;
        }
        let Type::Variant(cs) = &owed.head else { return input };
        let Some(fields) = head.get("head").and_then(Value::as_str).and_then(|h| cs.iter().find(|(c, _)| c == h)).map(|(_, f)| f.clone()) else { return input };
        let hex = |s: &str| (7..=40).contains(&s.len()) && s.chars().all(|c| c.is_ascii_hexdigit());
        let holders = |sha: &str| -> Vec<(String, String)> {
            repos
                .iter()
                .filter_map(|(n, path)| {
                    let o = std::process::Command::new("git").args(["-c", "safe.directory=*", "-C", path, "rev-parse", "--verify", "-q", &format!("{sha}^{{commit}}")]).stderr(std::process::Stdio::null()).output().ok()?;
                    let full = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    (o.status.success() && !full.is_empty()).then(|| (n.clone(), full))
                })
                .collect()
        };
        let layer = self.tickets.m.layer_of(t);
        let mut out = head.clone();
        let names = repos.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ");
        for (f, ty) in &fields {
            if !matches!(ty, Type::Ref) && !matches!(ty, Type::Opt(t) if **t == Type::Ref) {
                continue;
            }
            let Some(sha) = out.get(f).and_then(Value::as_str).map(str::to_string).filter(|s| hex(s)) else { continue };
            let mut found = holders(&sha);
            if found.is_empty() {
                if let Some(l) = &layer {
                    if let Err(e) = crate::layer::sync(&self.coord, l) {
                        eprintln!("coordd: layers: {l}: {e}");
                    }
                    found = holders(&sha);
                }
            }
            match found.as_slice() {
                [(n, full)] => out[f.as_str()] = json!({ n.clone(): full }),
                [] => return Input::Unfit { t, step: owed.step, why: format!("{f} is the commit {sha}, which is in none of the project's repositories ({names}); give it with its repository, as `a {sha}`"), call },
                several => {
                    let which = several.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(" and ");
                    return Input::Unfit { t, step: owed.step, why: format!("{f} is the commit {sha}, which is in {which}; say which repository it is of, as `a {sha}`"), call };
                }
            }
        }
        match input {
            Input::Answer { t, by, text, .. } => Input::Answer { t, by, head: out, text },
            Input::Formed { t, step, why, not_taken, call, .. } => Input::Formed { t, step, head: Some(out), why, not_taken, call },
            i => i,
        }
    }
}

// ---- acts (N-101 post 27, B) ---------------------------------------------------------------------------------

/// The tools of Claude Code that change a file: their answered call is the act `edit` (paper-research, N-101 post 28:
/// the duty arises from any edit, not only the symbol tools').
const EDITS: [&str; 4] = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// The acts coordd writes itself, which no agent may send.
const COORDDS_ACTS: [&str; 2] = ["submit", "check"];

/// The act `edit` a PostToolUse hook INPUT stands for, if its tool changes a file: `{is, text, tool, files}`.
pub fn edit_of_hook(input: &Value) -> Option<Value> {
    let tool = input.get("tool_name").and_then(Value::as_str).filter(|t| EDITS.contains(t))?;
    let i = input.get("tool_input").cloned().unwrap_or(Value::Null);
    let file = i.get("file_path").or_else(|| i.get("notebook_path")).and_then(Value::as_str).unwrap_or_default();
    Some(json!({"is": "edit", "text": format!("{tool} {file}"), "tool": tool, "files": [file]}))
}

/// The files an act V names (`files`).
fn files_of(v: &Value) -> Vec<String> {
    v.get("files").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

impl Core {
    /// Agent NAME's PostToolUse hook with INPUT: an edit by one of Claude Code's own tools is its act `edit`.
    pub fn act_of_hook(&self, name: &str, input: &Value) {
        if let Some(mut v) = edit_of_hook(input) {
            v["by"] = json!(name);
            // what it wrote is not found again (found.rs)
            crate::found::covered(&self.coord, name, &files_of(&v));
            self.ticket_event("act", &v);
        }
    }

    /// `Request::Act`: an act of the calling agent (the symbols server's `edit`), recorded as the act line.
    pub fn request_act(&self, who: &Who, is: &str, text: &str, more: &Map<String, Value>) -> Reply {
        let Who::Agent(me) = who else { return err("coord act: an agent's act; the owner's are its requests".into()) };
        if is.is_empty() || !is.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return err(format!("coord act: not an act: {is:?} (a word: edit, ...)"));
        }
        if COORDDS_ACTS.contains(&is) {
            return err(format!("coord act: `{is}` is coordd's own act (coord check writes it)"));
        }
        let mut v = Value::Object(more.clone());
        v["by"] = json!(me);
        v["is"] = json!(is);
        v["text"] = json!(text);
        if is == "edit" {
            crate::found::covered(&self.coord, me, &files_of(&v));
        }
        self.ticket_event("act", &v);
        ok(String::new())
    }
}

// ---- role lines (N-101 post 27, F) --------------------------------------------------------------------------------

/// The roles the record should say: the runner's when the project file names one, and each registered agent's role
/// with those HELD says the policy has it hold besides (`ticket::roles_held`; an agent with no role of its own but one
/// the policy gives is a `member` first, the policy's word for none).
pub fn roles_now(runner: bool, agents: impl Iterator<Item = (String, String)>, held: &BTreeMap<String, BTreeSet<String>>) -> Vec<(String, String, Vec<String>)> {
    let mut out = Vec::new();
    for (name, role) in agents {
        let own = if role.is_empty() { "member".to_string() } else { role };
        let also: Vec<String> = held.get(&name).into_iter().flatten().filter(|r| **r != own && *r != "member").cloned().collect();
        if own != "member" || !also.is_empty() {
            out.push((name, own, also));
        }
    }
    if runner {
        out.push(("runner".into(), "runner".into(), Vec::new()));
    }
    out
}

/// The registered agents of NAMES that WANT gives no line, whose last role line in M says a role other than `member` or
/// one held besides: each set back to `member`, with nothing besides.
pub fn back_to_member<'a>(m: &docket_machine::Machine, names: impl Iterator<Item = &'a String>, want: &[(String, String, Vec<String>)]) -> Vec<(String, String, Vec<String>)> {
    names
        .filter(|n| !want.iter().any(|(a, _, _)| a == *n))
        .filter(|n| m.roles.get(*n).is_some_and(|r| r != "member") || m.also.get(*n).is_some_and(|x| !x.is_empty()))
        .map(|n| (n.clone(), "member".to_string(), Vec::new()))
        .collect()
}

impl Core {
    /// The `role` lines the record lacks: each role in `roles_now` the record's last line for that agent does not say.
    /// The policy is asked who holds what (a solve, under the lock: at start and when a role is set); when it does not
    /// compile, the registry's roles alone, and the journal says why.
    pub fn record_roles(&mut self) {
        let runner = self.project.as_ref().is_some_and(|p| p.project.runner.is_some());
        let held = crate::ticket::roles_held(&self.places(), self.norm_facts(&[])).unwrap_or_else(|e| {
            eprintln!("coordd: who holds which role, by the policy: {e}");
            BTreeMap::new()
        });
        // an agent the owner ended holds no role: it is never started again, so a step the lead is given must not go
        // to it (Tovrin-dev, 17 Nov: S-1099511627813 went to the ended orch, the first of two leads by name; N-101 p1449)
        let mut want = roles_now(runner, self.agents.iter().filter(|(_, a)| a.ended.is_none()).map(|(n, a)| (n.clone(), a.role.clone())), &held);
        // an agent set back to none: `member`, when the record's last line for it says otherwise (docket-core3's
        // finding, paper-research N-101 p579: an old `orchestrator` line never cleared, and the policy then saw two
        // orchestrators, and with the derived fact two leads)
        want.extend(back_to_member(&self.tickets.m, self.agents.keys(), &want));
        for (agent, role, also) in want {
            let m = &self.tickets.m;
            if m.roles.get(&agent) != Some(&role) || m.also.get(&agent).map_or(&[][..], Vec::as_slice) != also.as_slice() {
                if let Err(e) = self.ticket_input(docket_machine::Input::Role { agent: agent.clone(), role, also }) {
                    eprintln!("coordd: the role line of {agent}: {e}");
                }
            }
        }
    }
}

// ---- an agent's own check through coordd (N-101 post 27, A) --------------------------------------------------

/// The checks asked and not answered yet, by name: who asked and what it ran (said again in its answer). A coordd
/// restarted meanwhile answers from the name alone.
static CHECKS: Mutex<BTreeMap<String, (String, String)>> = Mutex::new(BTreeMap::new());

/// The modes `coord check` takes, as the runner's flags.
pub const MODES: [&str; 3] = ["quick", "typecheck", "proofs"];

/// The name of agent ME's check asked at AT (`2026-11-15T20:15:02Z`): `ME.cYYYYMMDDHHMMSS`, a second one in the same
/// second `-2` after it, and so on (no two of one agent share a runner file: checks/NAME.json).
pub fn check_name(me: &str, at: &str, taken: impl Fn(&str) -> bool) -> String {
    let digits: String = at.chars().filter(char::is_ascii_digit).collect();
    let base = format!("{me}.c{digits}");
    let mut name = base.clone();
    let mut k = 1;
    while taken(&name) {
        k += 1;
        name = format!("{base}-{k}");
    }
    name
}

/// The agent whose check NAME is (`A.cN...`), if it is one of AGENTS.
pub fn check_agent<'a>(name: &str, agents: impl Fn(&str) -> bool) -> Option<String> {
    let (a, rest) = name.rsplit_once(".c")?;
    (rest.chars().next().is_some_and(|c| c.is_ascii_digit()) && agents(a)).then(|| a.to_string())
}

/// What `coord check` runs: `RUNNER check NAME --to ME [--MODE] REF...`.
pub fn check_command(runner: &str, name: &str, me: &str, mode: Option<&str>, flip: &[String], refs: &[String]) -> String {
    let q = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    let mode = mode.map(|m| format!(" --{m}")).unwrap_or_default();
    let flips: String = flip.iter().map(|s| format!(" --flip {}", q(s))).collect();
    let refs: Vec<String> = refs.iter().map(|r| q(r)).collect();
    format!("{runner} check {} --to {}{mode}{flips} {}", q(name), q(me), refs.join(" "))
}

/// `Request::Check` by WHO: the layers synced, the runner asked, the act `submit` recorded. Off the lock but for the
/// look-ups: the runner's command runs blocking, beside it.
pub async fn check(shared: &crate::Shared, who: &Who, mode: Option<String>, refs: Vec<String>, flip: Vec<String>) -> Reply {
    let Who::Agent(me) = who else { return err("coord check: an agent's own check; the owner asks the runner by a ticket".into()) };
    if let Some(m) = mode.as_deref().filter(|m| !MODES.contains(m)) {
        return err(format!("coord check: no mode {m} (--quick, --typecheck or --proofs; none: the whole check)"));
    }
    let (dir, runner, refs, coord) = {
        let core = shared.lock().await;
        let Some((dir, runner)) = core.project.as_ref().and_then(|p| Some((p.dir.clone(), p.project.runner.clone()?))) else {
            return err("coord check: the project file names no runner".into());
        };
        let refs = if refs.is_empty() {
            match core.own_layer_refs(me) {
                Ok(r) => r,
                Err(why) => return err(format!("coord check: {why}; name what to check as REPO:BRANCH")),
            }
        } else {
            refs
        };
        (dir, runner, refs, core.places().coord.clone())
    };
    // a boxed layer's commits reach the project's repositories, where the runner reads them, before it is asked
    for e in tokio::task::spawn_blocking(move || crate::layer::sync_open(&coord)).await.unwrap_or_default() {
        eprintln!("coordd: layers: {e}");
    }
    let at = crate::core::now();
    let name = {
        let checks = CHECKS.lock().unwrap_or_else(|e| e.into_inner());
        check_name(me, &at, |n| checks.contains_key(n))
    };
    let cmd = check_command(&runner, &name, me, mode.as_deref(), &flip, &refs);
    let c2 = cmd.clone();
    let out = tokio::task::spawn_blocking(move || std::process::Command::new("sh").arg("-c").arg(&c2).current_dir(&dir).output()).await;
    match out {
        Ok(Ok(o)) if o.status.success() => {}
        Ok(Ok(o)) => return err(format!("coord check: the runner refused it ({}): {}", o.status, String::from_utf8_lossy(&o.stderr).trim())),
        Ok(Err(e)) => return err(format!("coord check: {cmd}: {e}")),
        Err(e) => return err(format!("coord check: {e}")),
    }
    let flips: String = flip.iter().map(|s| format!("flip {s} ")).collect();
    let what = format!("{}{flips}{}", mode.as_deref().map(|m| format!("{m} ")).unwrap_or_default(), refs.join(" "));
    CHECKS.lock().unwrap_or_else(|e| e.into_inner()).insert(name.clone(), (me.clone(), what.clone()));
    let mut v = json!({"by": me, "is": "submit", "text": what, "check": name, "refs": refs});
    if let Some(m) = &mode {
        v["mode"] = json!(m);
    }
    if !flip.is_empty() {
        v["flip"] = json!(flip);
    }
    let mut core = shared.lock().await;
    // an edit made through Bash before this submit is covered by it: found and recorded first (found.rs)
    core.look_found(me, "submit");
    core.ticket_event("act", &v);
    drop(core);
    ok(format!("check {name} queued: {what}. Its answer comes to you as a message (and as the act `check` on the ticket you owe a step on)."))
}

impl Core {
    /// The refs of agent ME's layer, `REPO:layer/ID` in each repository of the project: the layer of the step it owes.
    pub fn own_layer_refs(&self, me: &str) -> Result<Vec<String>, String> {
        let id = self.tickets.m.owing(me).ok_or("you owe no step, so no layer is yours")?;
        let t = docket_machine::parse_ticket(&id).ok_or("no ticket")?;
        let layer = self.tickets.m.layer_of(t).ok_or_else(|| format!("{id}'s step is in no layer"))?;
        let p = self.project.as_ref().ok_or("no project file")?;
        let mut names: Vec<String> = p.repos().into_iter().map(|(n, _)| n).collect();
        if names.is_empty() {
            names.extend(self.runner_place(t).main_repo);
        }
        if names.is_empty() {
            return Err("the project names no repository".into());
        }
        Ok(names.into_iter().map(|n| format!("{n}:{}", layer_branch(&layer))).collect())
    }

    /// The runner's `check_end` V for a check an agent asked through coordd (`A.cN`): its act `check` on the ticket
    /// the agent owes a step on, and a notice to it. False when V names no such check (another check, or a ticket's).
    pub fn check_answered(&mut self, v: &Value) -> bool {
        let Some(name) = v.get("name").and_then(Value::as_str) else { return false };
        let asked = CHECKS.lock().unwrap_or_else(|e| e.into_inner()).remove(name);
        let Some((agent, what)) = asked.or_else(|| check_agent(name, |a| self.agents.contains_key(a)).map(|a| (a, String::new()))) else { return false };
        let result = v.get("result").and_then(Value::as_str).unwrap_or("red").to_string();
        let reason = v.get("reason").and_then(Value::as_str).unwrap_or_default().to_string();
        let text = format!("{}{result}{}", if what.is_empty() { String::new() } else { format!("{what}: ") }, if reason.is_empty() { String::new() } else { format!(": {reason}") });
        self.ticket_event("act", &json!({"by": agent, "is": "check", "text": text, "check": name, "result": result}));
        self.note_from_coord(&agent, &format!("Your check {name}: {}\n\n{text}", result.to_uppercase()));
        true
    }
}

// ---- the runner's jobs (N-101 post 27, D) ----------------------------------------------------------------------

/// The runner's jobs running now, by id, and whose they are (its `name`): from its job_start and job_end lines, and
/// each answer of `RUNNER jobs --json`. Empty, nothing is asked.
static RUNNING: Mutex<BTreeMap<String, Value>> = Mutex::new(BTreeMap::new());

/// One ask of `RUNNER jobs --json` at a time; events meanwhile are seen by it or the next.
static ASKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The jobs table after the runner's event V: a job_start adds its job, a job_end takes it away. Whether one runs.
pub fn note_job(v: &Value) -> bool {
    let mut running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    let id = v.get("id").or_else(|| v.get("job")).map(|i| i.as_str().map_or_else(|| i.to_string(), str::to_string));
    match (v.get("event").and_then(Value::as_str), id) {
        (Some("job_start"), Some(id)) => {
            running.insert(id, v.get("name").cloned().unwrap_or(Value::Null));
        }
        (Some("job_end"), Some(id)) => {
            running.remove(&id);
        }
        _ => {}
    }
    !running.is_empty()
}

/// After the runner's event V (coordd's project source; `main::project_event`): while a job of the runner runs,
/// `RUNNER jobs --json` is asked, off the lock (paper-research N-101 post 28: on every runner event, no clock of
/// coordd's own). Nothing asked while nothing runs. The runner writes what it answered as one `job` line of phase
/// `measure` per running job (runner-v2 1e7c2c5), which comes back through the project's source as a `job_measure`
/// event and is recorded on its ticket; so coordd records no copy of its own (it did, as `jobs` events, before).
/// A `job_measure` asks nothing: it is the answer of an ask.
pub async fn after_event(shared: &crate::Shared, v: &Value) {
    let runner_event = matches!(v.get("event").and_then(Value::as_str), Some("outcome" | "step_end" | "job_start" | "job_end" | "check_end"));
    if !runner_event || !note_job(v) || ASKING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let answer = runner_command(shared, "jobs --json".into()).await;
    ASKING.store(false, std::sync::atomic::Ordering::SeqCst);
    let list = match answer.and_then(|out| serde_json::from_str::<Value>(&out).map_err(|e| format!("not JSON: {e}"))) {
        Ok(l) => l,
        Err(e) => return eprintln!("coordd: runner jobs: {e}"),
    };
    // the answer is what runs: the table follows it
    {
        let mut running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
        running.clear();
        for j in list.as_array().into_iter().flatten() {
            if let Some(id) = j.get("id").map(|i| i.as_str().map_or_else(|| i.to_string(), str::to_string)) {
                running.insert(id, j.get("name").cloned().unwrap_or(Value::Null));
            }
        }
    }
}

/// `RUNNER ARGS` run in the project file's folder, off the lock: its output, or why not.
async fn runner_command(shared: &crate::Shared, args: String) -> Result<String, String> {
    let (dir, runner) = {
        let core = shared.lock().await;
        core.project.as_ref().and_then(|p| Some((p.dir.clone(), p.project.runner.clone()?))).ok_or("the project file names no runner")?
    };
    let cmd = format!("{runner} {args}");
    let c2 = cmd.clone();
    match tokio::task::spawn_blocking(move || std::process::Command::new("sh").arg("-c").arg(&c2).current_dir(&dir).output()).await {
        Ok(Ok(o)) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).into_owned()),
        Ok(Ok(o)) => Err(format!("{cmd}: {}: {}", o.status, String::from_utf8_lossy(&o.stderr).trim())),
        Ok(Err(e)) => Err(format!("{cmd}: {e}")),
        Err(e) => Err(e.to_string()),
    }
}

/// `Request::Jobs`: the runner's running jobs as it says them.
pub async fn jobs(shared: &crate::Shared) -> Reply {
    match runner_command(shared, "jobs".into()).await {
        Ok(out) => ok(out),
        Err(e) => err(format!("coord jobs: {e}")),
    }
}

/// The runner's service socket, beside the job files jobs.rs reads.
pub fn runner_socket() -> std::path::PathBuf {
    machine_state::store::socket()
}

/// `Request::Store`: the store's rows, or a page of ROW's entries from FROM, as the runner's service gives them.
pub async fn store(row: Option<String>, from: Option<u64>) -> Reply {
    let sock = runner_socket();
    match tokio::task::spawn_blocking(move || machine_state::store::rows(&sock, row.as_deref(), from.unwrap_or(0))).await {
        Ok(Ok(line)) => ok(line),
        Ok(Err(why)) => err(format!("coord store: {why}")),
        Err(e) => err(format!("coord store: {e}")),
    }
}

/// `Request::StoreFree` by WHO: the owner's press on one finding of the store page's walk, which the runner deletes only
/// when its last walk found it and it still is one; what it freed, in words.
pub async fn free(who: &Who, path: &str) -> Reply {
    free_at(&runner_socket(), who, path).await
}

/// [`free`] on the socket SOCK.
pub async fn free_at(sock: &std::path::Path, who: &Who, path: &str) -> Reply {
    if !matches!(who, Who::Owner) {
        return err(format!("coord store: only the owner deletes what the store page's walk found, not {}", who.name()));
    }
    let (at, q) = (sock.to_path_buf(), json!({"free": path}));
    match tokio::task::spawn_blocking(move || machine_state::store::line(&at, &q)).await.unwrap_or_else(|e| Err(std::io::Error::other(e))) {
        Err(e) => err(format!("coord store: the runner's service at {}: {e}", sock.display())),
        Ok(line) => match serde_json::from_str::<Value>(line.trim()) {
            Ok(v) if v.get("error").is_some() => err(format!("coord store: {}", v["error"].as_str().unwrap_or("the runner refused it"))),
            Ok(v) if v.get("freed").and_then(Value::as_u64).is_some() => ok(format!("{path}: {} bytes freed", v["freed"].as_u64().unwrap_or(0))),
            _ => err("coord store: the runner answered nothing freed (a runner from before Home's delete?)".into()),
        },
    }
}

/// `Request::End` by WHO: the owner, or an agent whose role is lead, ends the runner's job JOB for WHY.
pub async fn end(shared: &crate::Shared, who: &Who, job: &str, why: &str) -> Reply {
    let by = who.name();
    let may = matches!(who, Who::Owner) || shared.lock().await.tickets.m.holders("lead").contains(&by);
    if !may {
        return err(format!("coord end: {by} may not end a job (the owner or a lead does)"));
    }
    if why.trim().is_empty() {
        return err("coord end: say why (coord end JOB WHY...)".into());
    }
    let q = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    match runner_command(shared, format!("end {} --by {} {}", q(job), q(&by), q(why))).await {
        Ok(out) => ok(out),
        Err(e) => err(format!("coord end: {e}")),
    }
}

/// What `coord open` refuses in FORM for KIND's form FIELDS: a field of one value (a Ref, a text, a ticket, an agent,
/// a number or a yes/no) given an object or a list (`--cut '{"a": SHA, "b": SHA}'`). The machine would turn it into
/// one text, and the runner would then refuse the step (B-4, N-97 post 48); coordd says so at once (paper-research
/// N-97 post 49).
pub fn one_value_fields(fields: &[(String, docket_machine::types::Type)], form: &Value) -> Result<(), String> {
    use docket_machine::types::Type;
    for (name, ty) in fields {
        let one = match ty {
            Type::Opt(t) => t.as_ref(),
            t => t,
        };
        if !matches!(one, Type::Text | Type::Ref | Type::Ticket | Type::Agent | Type::Int | Type::Bool) {
            continue;
        }
        // the command line gives every field as a text: one that reads as JSON is an object or list all the same, for
        // all but a Text (whose text may well start with a brace)
        let read = match form.get(name) {
            Some(Value::String(v)) if !matches!(one, Type::Text) && v.trim_start().starts_with(['{', '[']) => serde_json::from_str::<Value>(v).ok(),
            v => v.cloned(),
        };
        let what = match read {
            // commits by repository is what a Ref is in a project of several (`{"a": SHA, "b": SHA}`): the machine
            // reads it so, from a text as from an object (docket-machine types.rs coerce; docket-core, N-101.p64)
            Some(o @ Value::Object(_)) if matches!(one, Type::Ref) && Type::Ref.conforms(&o).is_ok() => continue,
            Some(Value::Object(_)) => "an object",
            Some(Value::Array(_)) => "a list",
            _ => continue,
        };
        return Err(format!(
            "coord open: --{name} is one {one}, and what was given is {what}. A Ref of several repositories is written {{\"REPO\": \"SHA\", ...}} or REPO=SHA,REPO=SHA."
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_object_for_a_field_of_one_value_is_refused_at_open() {
        use docket_machine::types::Type;
        let form = vec![("cut".to_string(), Type::Ref), ("note".to_string(), Type::Opt(Box::new(Type::Text))), ("carries".to_string(), Type::List(Box::new(Type::Ticket)))];
        // commits by repository is a Ref, as an object or as its text; an object of anything else is not
        assert!(super::one_value_fields(&form, &json!({"cut": {"a": "1", "b": "2"}})).is_ok());
        let e = super::one_value_fields(&form, &json!({"cut": {"a": 1}})).unwrap_err();
        assert!(e.starts_with("coord open: --cut is one Ref, and what was given is an object."), "{e}");
        assert!(super::one_value_fields(&form, &json!({"note": ["x"]})).unwrap_err().contains("--note is one Text, and what was given is a list"));
        assert!(super::one_value_fields(&form, &json!({"cut": "abc", "carries": ["T-1"]})).is_ok());
        // as the command line gives it: a text that reads as an object; a Text may start with a brace
        assert!(super::one_value_fields(&form, &json!({"cut": "{\"a\": \"1\", \"b\": \"2\"}"})).is_ok());
        assert!(super::one_value_fields(&form, &json!({"cut": "{\"a\": [\"1\"]}"})).unwrap_err().contains("--cut is one Ref, and what was given is an object"));
        assert!(super::one_value_fields(&form, &json!({"note": "{not json"})).is_ok());
        assert!(super::one_value_fields(&form, &json!({"note": "[1]"})).is_ok());
    }

    use super::*;

    fn place(branch: Option<&str>, main: Option<&str>) -> Place {
        Place { branch: branch.map(String::from), main_repo: main.map(String::from), line: None, repos: Vec::new(), of: None }
    }

    #[test]
    fn a_hand_in_goes_out_as_commits_by_repository_with_the_layer_branch() {
        let args = json!({"commit": {"tovrin": "a1b2c3d", "p4": "9f8e7d6"}});
        let out = step_json("hand_in", &args, &place(Some("layer/E-12.l1"), Some("tovrin")));
        assert_eq!(out, json!({"commits": {"tovrin": "a1b2c3d", "p4": "9f8e7d6"}, "branch": "layer/E-12.l1"}));
    }

    #[test]
    fn a_bare_commit_is_the_projects_own_repositorys() {
        let out = step_json("hand_in", &json!({"commit": "a1b2c3d"}), &place(Some("layer/E-3.l1"), Some("a")));
        assert_eq!(out, json!({"commits": {"a": "a1b2c3d"}, "branch": "layer/E-3.l1"}));
        // no name known (no project folder): the string as it is; no layer: no branch
        let out = step_json("hand_in", &json!({"commit": "a1b2c3d"}), &place(None, None));
        assert_eq!(out, json!({"commits": "a1b2c3d"}));
    }

    #[test]
    fn an_edit_tool_is_an_edit_act_and_another_tool_none() {
        let v = edit_of_hook(&json!({"tool_name": "Edit", "tool_input": {"file_path": "/w/src/a.rs", "old_string": "x"}})).unwrap();
        assert_eq!(v, json!({"is": "edit", "text": "Edit /w/src/a.rs", "tool": "Edit", "files": ["/w/src/a.rs"]}));
        assert!(edit_of_hook(&json!({"tool_name": "Read", "tool_input": {"file_path": "/w/a"}})).is_none());
        assert!(edit_of_hook(&json!({"tool_name": "mcp__symbols__rename"})).is_none(), "the symbols server sends its own");
    }

    #[test]
    fn a_checks_name_is_its_agents_and_unique() {
        assert_eq!(check_name("f12-1", "2026-11-15T20:15:02Z", |_| false), "f12-1.c20261115201502");
        assert_eq!(check_name("f12-1", "2026-11-15T20:15:02Z", |n| n == "f12-1.c20261115201502"), "f12-1.c20261115201502-2");
        assert_eq!(check_agent("f12-1.c20261115201502-2", |a| a == "f12-1"), Some("f12-1".into()));
        assert_eq!(check_agent("printer-12", |_| true), None);
        assert_eq!(check_agent("x.c2", |a| a == "y"), None);
        assert_eq!(check_command("RUN", "a.c1", "a", Some("quick"), &[], &["p4:layer/E-1.l1".into(), "tovrin:layer/E-1.l1".into()]), "RUN check 'a.c1' --to 'a' --quick 'p4:layer/E-1.l1' 'tovrin:layer/E-1.l1'");
        assert_eq!(check_command("RUN", "a.c2", "a", None, &["proofs-gen".into()], &["p4:x".into()]), "RUN check 'a.c2' --to 'a' --flip 'proofs-gen' 'p4:x'");
    }

    /// `coord check` and `coord act` through coordd (N-101 post 27, A and B): the runner asked with the check's name, the
    /// act `submit` recorded; the runner's check_end for that name is the act `check` and a notice to the agent; an
    /// edit through the post hook and the symbols server's `edit` are act lines; coordd's own acts are refused.
    #[test]
    fn an_agents_check_goes_through_coordd_and_its_answer_comes_back_as_an_act() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-runner-check-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        let project = coord.join("files");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("runner.sh"), "#!/bin/sh\nprintf '%s\\n' \"$@\" > args.txt\n").unwrap();
        std::fs::write(project.join("project.toml"), "[project]\nname = \"c\"\nrunner = \"sh runner.sh\"\n").unwrap();
        std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"impl\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        core.load_tickets();
        let log = coord.join("tickets/log.jsonl");
        let acts = move |is: &str| -> Vec<Value> {
            std::fs::read_to_string(&log).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "act" && v["is"] == is).collect()
        };
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
        let shared: crate::Shared = crate::Shared::new(core);
        rt.block_on(async {
            crate::ticket::start(shared.0.clone());
            let wait = |is: &'static str, n: usize| {
                let acts = acts.clone();
                async move {
                    for _ in 0..500 {
                        if acts(is).len() >= n {
                            return;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    }
                }
            };
            let who = Who::Agent("impl".into());
            // the owner has no own check; a mode the runner has not is refused; no layer and no refs: said why
            assert!(!check(&shared, &Who::Owner, None, vec!["a:x".into()], vec![]).await.ok);
            assert!(!check(&shared, &who, Some("fast".into()), vec!["a:x".into()], vec![]).await.ok);
            let r = check(&shared, &who, None, vec![], vec![]).await;
            assert!(!r.ok && r.out.contains("owe no step"), "{}", r.out);
            // asked: the runner's command, and the act submit
            let r = check(&shared, &who, Some("quick".into()), vec!["a:feature".into(), "b:feature".into()], vec!["proofs-gen".into()]).await;
            assert!(r.ok, "{}", r.out);
            let args = std::fs::read_to_string(project.join("args.txt")).unwrap();
            let args: Vec<&str> = args.lines().collect();
            assert_eq!((args[0], &args[2..]), ("check", &["--to", "impl", "--quick", "--flip", "proofs-gen", "a:feature", "b:feature"][..]), "{args:?}");
            let name = args[1].to_string();
            assert!(name.starts_with("impl.c"), "{name}");
            wait("submit", 1).await;
            let s = acts("submit");
            assert_eq!((s.len(), s[0]["agent"].clone(), s[0]["check"].clone(), s[0]["text"].clone(), s[0].get("t")), (1, json!("impl"), json!(name), json!("quick flip proofs-gen a:feature b:feature"), None), "{s:?}");
            // answered: the act check with its result, and a notice to the agent
            shared.lock().await.project_event(json!({"event": "check_end", "name": name, "result": "red", "reason": "case 3"})).await;
            wait("check", 1).await;
            let c = acts("check");
            assert_eq!((c.len(), c[0]["result"].clone(), c[0]["text"].clone()), (1, json!("red"), json!("quick flip proofs-gen a:feature b:feature: red: case 3")), "{c:?}");
            let core = shared.lock().await;
            assert!(core.tickets.m.tickets.values().any(|x| x.kind == "thread" && x.posts.iter().any(|p| matches!(&p.text, docket_machine::Text::New(t) if t.contains(&format!("Your check {name}: RED"))))), "a notice to impl");
            // edits: Claude Code's Edit through the post hook, and the symbols server's act
            core.act_of_hook("impl", &json!({"tool_name": "Write", "tool_input": {"file_path": "/w/a.rs"}}));
            let more: Map<String, Value> = [("tool".to_string(), json!("rename")), ("symbol".to_string(), json!("report::twice"))].into_iter().collect();
            assert!(core.request_act(&who, "edit", "rename report::twice -> report::double (3 places)", &more).ok);
            assert!(!core.request_act(&who, "check", "x", &Map::new()).ok, "coordd's own act");
            assert!(!core.request_act(&Who::Owner, "edit", "x", &Map::new()).ok);
            drop(core);
            wait("edit", 2).await;
            let e = acts("edit");
            // each task runs on its own, so the two may land in either order
            let mut tools: Vec<String> = e.iter().map(|v| v["tool"].as_str().unwrap_or_default().to_string()).collect();
            tools.sort();
            assert_eq!(tools, vec!["Write", "rename"], "{e:?}");
        });
        rt.shutdown_background();
        let _ = std::fs::remove_dir_all(&coord);
    }

    #[test]
    fn the_jobs_table_follows_the_job_lines() {
        assert!(note_job(&json!({"event": "job_start", "id": "t-1", "name": "B-3"})));
        assert!(note_job(&json!({"event": "outcome", "name": "B-3"})), "still runs");
        assert!(!note_job(&json!({"event": "job_end", "id": "t-1", "name": "B-3"})));
    }

    #[test]
    fn the_record_gets_a_role_line_for_each_role_it_does_not_say_and_no_other() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-runner-roles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        let project = coord.join("files");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("project.toml"), "[project]\nname = \"r\"\nrunner = \"true\"\n").unwrap();
        std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"lead\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n{\"ev\":\"role\",\"name\":\"lead\",\"role\":\"lead\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        core.load_tickets();
        let roles = |coord: &std::path::Path| -> Vec<(String, String)> {
            std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "role").map(|v| (v["agent"].as_str().unwrap().to_string(), v["role"].as_str().unwrap().to_string())).collect()
        };
        core.record_roles();
        let mut got = roles(&coord);
        got.sort();
        assert_eq!(got, vec![("lead".to_string(), "lead".to_string()), ("runner".to_string(), "runner".to_string())]);
        // said already: nothing more, also after a restart (replayed from the log)
        core.record_roles();
        let mut again = Core::load(&coord.display().to_string(), &base);
        again.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        again.load_tickets();
        again.record_roles();
        assert_eq!(roles(&coord).len(), 2);
        // the owner sets a role: a line
        assert!(again.set_role(&Who::Owner, "lead", "consultant").ok);
        assert_eq!(roles(&coord).last().unwrap(), &("lead".to_string(), "consultant".to_string()));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// Two agents hold `lead` and the owner ends one: from then on the running one alone is the lead the machine
    /// answers with (a stop's step, a step passed up), and `--undo` gives the ended one its role back (paper-research
    /// N-101 p1449, p1456; Tovrin-dev's orch and orchestrator).
    #[test]
    fn an_ended_agent_holds_no_role() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-ended-role-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"orch\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n{\"ev\":\"role\",\"name\":\"orch\",\"role\":\"lead\",\"at\":\"x\"}\n{\"ev\":\"add\",\"id\":\"AGT-002\",\"name\":\"orchestrator\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n{\"ev\":\"role\",\"name\":\"orchestrator\",\"role\":\"lead\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        core.record_roles();
        let both = core.tickets.m.holders("lead");
        assert!(core.set_ended(&Who::Owner, "orch", false).ok);
        let after = core.tickets.m.holders("lead");
        let line = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap().lines().last().unwrap().to_string();
        // a restart replays the line and writes none again
        let n = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap().lines().count();
        let mut again = Core::load(&coord.display().to_string(), &base);
        again.load_tickets();
        again.record_roles();
        let replayed = again.tickets.m.holders("lead");
        let m = std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap().lines().count();
        assert!(again.set_ended(&Who::Owner, "orch", true).ok);
        let undone = again.tickets.m.holders("lead");
        let _ = std::fs::remove_dir_all(&coord);
        assert_eq!(both, vec!["orch".to_string(), "orchestrator".to_string()]);
        assert_eq!(after, vec!["orchestrator".to_string()], "{line}");
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!((v["ev"].as_str(), v["agent"].as_str(), v["role"].as_str()), (Some("role"), Some("orch"), Some("member")));
        assert_eq!(replayed, vec!["orchestrator".to_string()]);
        assert_eq!(n, m, "no line again after a restart");
        assert_eq!(undone, vec!["orch".to_string(), "orchestrator".to_string()]);
    }

    #[test]
    fn built_green_names_the_hand_ins_commits_by_repository() {
        assert_eq!(step_json("built_green", &json!({"commit": "abc123"}), &place(None, Some("a"))), json!({"commit": {"a": "abc123"}}));
        assert_eq!(step_json("built_green", &json!({"commit": {"a": "1", "b": "2"}}), &place(None, Some("a"))), json!({"commit": {"a": "1", "b": "2"}}));
    }

    #[test]
    fn accepted_names_the_pieces_commit_by_repository() {
        let bare = json!({"piece": {"by": "f76-1", "commit": "165d44e", "head": "Handed", "top": {"a": "6d97", "b": "09a4"}}});
        assert_eq!(
            step_json("accepted", &bare, &place(None, Some("a"))),
            json!({"piece": {"by": "f76-1", "commit": {"a": "165d44e"}, "head": "Handed", "top": {"a": "6d97", "b": "09a4"}}})
        );
        // without a main repository it goes as it is
        assert_eq!(step_json("accepted", &bare, &place(None, None)), bare);
    }

    #[test]
    fn the_other_calls_go_out_as_they_are() {
        for (call, args) in [
            ("cut", json!({"branch": "line"})),
            ("cases", json!({"cut": {"a": "1", "b": "2"}})),
            ("take_out", json!({"cut": {"a": "1"}, "piece": "E-12"})),
            ("merge_back", json!({"cut": {"a": "1"}})),
            ("accepted", json!({"piece": {"commit": {"a": "1"}, "top": {"a": "2", "b": "3"}, "by": "f12-1"}})),
        ] {
            assert_eq!(step_json(call, &args, &place(Some("layer/X"), Some("a"))), args, "{call}");
        }
    }

    /// A runner step whose command could not run (N-159 post 48): not there (127), then killed by a signal; held back,
    /// not raised; when the runner is up (`runner_up`) it goes at the next hand-over, and once given it goes no more. A
    /// runner that ran and refused is the other side of the line: `ran`, so it raises (main.rs, 9d8741e).
    #[test]
    fn a_step_whose_runner_could_not_run_goes_once_the_runner_is_up() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-runner-down-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        let project = coord.join("files");
        std::fs::create_dir_all(project.join("kinds")).unwrap();
        std::fs::create_dir_all(project.join("calls")).unwrap();
        std::fs::write(project.join("calls/h.call"), "call hand_in(commit: Ref) -> OnLine(top: Ref) | Red(errors: Text)\n  template\n    Check {{ commit }}.\n").unwrap();
        std::fs::write(project.join("kinds/h.kind"), "kind piece\n  letter H\n  form c: Ref\n  result Ref\n  pattern\n    v <- step runner hand_in(c);\n    match v {\n      OnLine(top) => return top\n      Red(e) => return c\n    }\n").unwrap();
        std::fs::write(project.join("project.toml"), "[project]\nname = \"h\"\nrunner = \"sh runner.sh\"\n").unwrap();
        std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        core.load_tickets();
        core.ticket_input(docket_machine::Input::Open { kind: "piece".into(), under: None, opener: "owner".into(), form: json!({"c": "abc123"}), text: None, names: vec![], told: vec![] }).unwrap();
        let k = core.tickets.m.tickets.values().find(|x| x.kind == "piece").map(|x| x.t).unwrap();
        let dir = project.display().to_string();
        let d = core.due_now("runner");
        assert_eq!(d.len(), 1, "{d:?}");
        // down: the command is not there, then it is killed before it answers; each time held back as main.rs does
        let n = crate::project::to_runner(&dir, "./no-such-runner", &d[0], &core.runner_place(k)).unwrap_err();
        assert!(!n.ran, "127: {n:?}");
        std::fs::write(project.join("runner.sh"), "#!/bin/sh\nkill -9 $$\n").unwrap();
        let n = crate::project::to_runner(&dir, "sh runner.sh", &d[0], &core.runner_place(k)).unwrap_err();
        assert!(!n.ran, "a signal: {n:?}");
        core.runner_held("runner", &d[0], &n.why);
        assert!(core.due_now("runner").is_empty(), "held back");
        // another project event is no sign of the runner: still held
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(core.project_event(json!({"event": "job_end", "id": "1"})));
        assert!(core.due_now("runner").is_empty());
        // up: it goes, once
        std::fs::write(project.join("runner.sh"), "#!/bin/sh\nprintf '%s\\n' \"$@\" >> args.txt\n").unwrap();
        rt.block_on(core.project_event(json!({"event": "runner_up", "at": "x"})));
        let d = core.due_now("runner");
        assert_eq!(d.len(), 1, "{d:?}");
        crate::project::to_runner(&dir, "sh runner.sh", &d[0], &core.runner_place(k)).unwrap();
        core.ticket_given("runner", &d[0]);
        assert!(core.due_now("runner").is_empty(), "given: not again");
        rt.block_on(core.project_event(json!({"event": "runner_up", "at": "y"})));
        assert!(core.due_now("runner").is_empty(), "a later start sends nothing again");
        assert_eq!(std::fs::read_to_string(project.join("args.txt")).unwrap().lines().filter(|l| *l == "step").count(), 1, "the step went out once");
        // the other side of the line: a runner that ran and refused
        std::fs::write(project.join("runner.sh"), "#!/bin/sh\necho 'coord-runner step: refused: no' >&2\nexit 1\n").unwrap();
        let n = crate::project::to_runner(&dir, "sh runner.sh", &d[0], &core.runner_place(k)).unwrap_err();
        assert!(n.ran, "{n:?}");
        let _ = std::fs::remove_dir_all(&coord);
    }


    /// A runner step whose command cannot start (its execute bit gone: 126, Permission denied) is held back and shown so
    /// on its step, then tried again on coordd's own clock: once the command can start it goes out, with no coordd
    /// restart and no event from the runner (paper-research N-101 p448; the 27 minutes of B-273's cut, p443). The same
    /// error again is one journal line, not one per try; and Core::hand_over, which the clock loop calls each second,
    /// offers it again only once its minute (here a second) is up.
    #[test]
    fn a_held_runner_step_goes_out_on_the_clock_once_its_command_can_start() {
        use std::os::unix::fs::PermissionsExt;
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-runner-again-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        let project = coord.join("files");
        std::fs::create_dir_all(project.join("kinds")).unwrap();
        std::fs::create_dir_all(project.join("calls")).unwrap();
        std::fs::write(project.join("calls/h.call"), "call hand_in(commit: Ref) -> OnLine(top: Ref) | Red(errors: Text)\n  template\n    Check {{ commit }}.\n").unwrap();
        std::fs::write(project.join("kinds/h.kind"), "kind piece\n  letter H\n  form c: Ref\n  result Ref\n  pattern\n    v <- step runner hand_in(c);\n    match v {\n      OnLine(top) => return top\n      Red(e) => return c\n    }\n").unwrap();
        std::fs::write(project.join("project.toml"), "[project]\nname = \"h\"\nrunner = \"./runner.sh\"\n").unwrap();
        std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        core.load_tickets();
        assert!(core.add(&crate::core::Who::Owner, "runner", "program", "none").ok);
        core.agents.get_mut("runner").unwrap().live.pid = Some(std::process::id() as i32);
        core.runner_after = std::time::Duration::from_secs(1);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        core.jobs = Some(tx);
        core.ticket_input(docket_machine::Input::Open { kind: "piece".into(), under: None, opener: "owner".into(), form: json!({"c": "abc123"}), text: None, names: vec![], told: vec![] }).unwrap();
        let k = core.tickets.m.tickets.values().find(|x| x.kind == "piece").map(|x| x.t).unwrap();
        let id = core.tickets.m.id(k);
        let dir = project.display().to_string();
        // the runner's program without its execute bit
        let runner = project.join("runner.sh");
        crate::accounts::testing::program(&runner, "#!/bin/sh\nprintf '%s\\n' \"$@\" >> args.txt\n");
        std::fs::set_permissions(&runner, std::fs::Permissions::from_mode(0o644)).unwrap();
        // what main.rs's Job::Program does with each delivery the hand-over gives it
        let send = |core: &mut Core, rx: &mut tokio::sync::mpsc::UnboundedReceiver<crate::core::Job>| -> Option<bool> {
            // other jobs (none for the runner's program) are not this one's
            while rx.try_recv().is_ok() {}
            core.hand_over("runner");
            let Ok(crate::core::Job::Program { name, deliveries }) = rx.try_recv() else { return None };
            assert_eq!(deliveries.len(), 1);
            let d = deliveries[0].0.clone();
            let sent = crate::project::to_runner(&dir, "./runner.sh", &d, &core.runner_place(k));
            let given = core.runner_outcome(&name, &d, &sent);
            if given {
                core.ticket_given(&name, &d);
            }
            Some(given)
        };
        assert_eq!(core.due_now("runner").len(), 1, "due: {:?}", core.tickets.m.due("runner"));
        assert!(core.to_hand_over().contains(&"runner".to_string()), "the clock loop's look offers it");
        assert_eq!(send(&mut core, &mut rx), Some(false), "cannot start: held back, not given");
        let u = core.runner_unsent.values().next().cloned().unwrap();
        assert!(u.why.contains("Permission denied"), "{u:?}");
        // shown on its step, for coord show and the app
        let shown = core.ticket_shown(k).unwrap().unsent;
        assert_eq!(shown.len(), 1, "{shown:?}");
        assert_eq!((shown[0].step, shown[0].call.as_str(), shown[0].worker.as_str(), shown[0].tries), (1, "hand_in", "runner", Some(1)));
        let why = shown[0].why.clone().unwrap_or_default();
        assert!(why.contains("Permission denied") && !why.starts_with("not sent") && shown[0].since.as_deref() == Some(u.since.as_str()), "{shown:?}");
        let r = core.ticket_show(&id);
        assert!(r.ok && serde_json::to_string(&r).unwrap().contains("unsent"), "coord show has it too");
        // within its minute it is not offered again; the same error once more is no new journal line, another is
        assert!(core.due_now("runner").is_empty());
        assert_eq!(send(&mut core, &mut rx), None, "not offered within its minute");
        let d = core.tickets.m.due("runner")[0].clone();
        assert!(!core.runner_held("runner", &d, &u.why), "the same error: no new journal line");
        assert!(core.runner_held("runner", &d, "another error"), "another error: a journal line");
        assert!(core.runner_held("runner", &d, &u.why));
        // its minute up and still no execute bit: tried, held again, its first failure's time kept
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert_eq!(send(&mut core, &mut rx), Some(false), "tried again on the clock, still cannot start");
        assert_eq!(core.runner_unsent.values().next().unwrap().since, u.since);
        assert_eq!(core.ticket_shown(k).unwrap().unsent[0].tries, Some(5), "tried five times: two sends, three helds");
        // the execute bit put back: no event, no restart; at its next try it goes out, once
        std::fs::set_permissions(&runner, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(send(&mut core, &mut rx), None, "still within its minute");
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert_eq!(send(&mut core, &mut rx), Some(true), "sent on the clock");
        assert!(core.runner_unsent.is_empty());
        assert!(core.ticket_shown(k).unwrap().unsent.is_empty());
        assert!(core.due_now("runner").is_empty(), "given: not again");
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert_eq!(send(&mut core, &mut rx), None, "nothing more goes");
        assert_eq!(std::fs::read_to_string(project.join("args.txt")).unwrap().lines().filter(|l| *l == "step").count(), 1, "the step went out once");
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// A bare commit in a project of several repositories (N-159 post 62: P-124's 562e0f4 was in b, keyed to a): the one
    /// repository that holds it, by its full name; none or several, it stays a string. For hand_in, built_green and accepted alike.
    #[test]
    fn a_bare_commit_goes_to_the_one_repository_that_holds_it() {
        let dir = std::env::temp_dir().join(format!("docket-keyed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let git = |repo: &std::path::Path, args: &[&str]| {
            let o = std::process::Command::new("git").args(["-c", "user.name=t", "-c", "user.email=t@t"]).args(args).current_dir(repo).output().unwrap();
            assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        };
        let mut shas = Vec::new();
        for r in ["a", "b"] {
            let p = dir.join(r);
            std::fs::create_dir_all(&p).unwrap();
            git(&p, &["init", "-q"]);
            std::fs::write(p.join("f"), r).unwrap();
            git(&p, &["add", "f"]);
            git(&p, &["commit", "-q", "-m", r]);
            shas.push(git(&p, &["rev-parse", "HEAD"]));
        }
        let place = Place { branch: Some("layer/P-123.l1".into()), main_repo: Some("a".into()), line: None, of: None, repos: ["a", "b"].iter().map(|r| (r.to_string(), dir.join(r).display().to_string())).collect() };
        let b = &shas[1][..7];
        // by its full name there
        assert_eq!(step_json("hand_in", &json!({"commit": b}), &place), json!({"commits": {"b": shas[1]}, "branch": "layer/P-123.l1"}));
        assert_eq!(step_json("built_green", &json!({"commit": b}), &place), json!({"commit": {"b": shas[1]}}));
        assert_eq!(step_json("accepted", &json!({"piece": {"commit": shas[0], "by": "f1"}}), &place), json!({"piece": {"commit": {"a": shas[0]}, "by": "f1"}}));
        // held by none: a string, refused by the runner
        assert_eq!(step_json("hand_in", &json!({"commit": "0000000"}), &place)["commits"], json!("0000000"));
        // held by both (b's commit fetched into a): a string
        git(&dir.join("a"), &["fetch", "-q", &dir.join("b").display().to_string(), "HEAD"]);
        assert_eq!(step_json("hand_in", &json!({"commit": b}), &place)["commits"], json!(b));
        // one repository: the project's own, as before
        let one = Place { repos: vec![("a".into(), dir.join("a").display().to_string())], ..place.clone() };
        assert_eq!(step_json("hand_in", &json!({"commit": b}), &one)["commits"], json!({"a": b}));
        let _ = std::fs::remove_dir_all(&dir);
    }


    /// An answer in `layer on X` (N-159 posts 119, 125: the fixer's bare 2add7bf in a, on the cut {a, b}): laid over X.
    #[test]
    fn an_answer_in_a_layer_on_a_cut_is_laid_over_the_cut() {
        use docket_machine::Type;
        let cs = vec![("Commit".to_string(), vec![("commit".to_string(), Type::Ref)]), ("NotMine".to_string(), vec![("why".to_string(), Type::Text)])];
        let x: Map<String, Value> = serde_json::from_value(json!({"a": "923dc65", "b": "977b48e"})).unwrap();
        // the bare commit, which KEY places in a (as Place::keyed does by the repository holding it)
        let key = |sha: &str| (sha == "2add7bf").then(|| json!({"a": "2add7bf0000"}));
        assert_eq!(complete_refs(&json!({"head": "Commit", "commit": "2add7bf"}), &cs, &x, key), json!({"head": "Commit", "commit": {"a": "2add7bf0000", "b": "977b48e"}}));
        // the fixer committed in both: as it is
        let both = json!({"head": "Commit", "commit": {"a": "1", "b": "2"}});
        assert_eq!(complete_refs(&both, &cs, &x, key), both);
        // one named by repository
        assert_eq!(complete_refs(&json!({"head": "Commit", "commit": {"b": "2"}}), &cs, &x, key), json!({"head": "Commit", "commit": {"a": "923dc65", "b": "2"}}));
        // a commit nothing places, a repository X does not name, a text field: as they are
        for h in [json!({"head": "Commit", "commit": "0000000"}), json!({"head": "Commit", "commit": {"c": "3"}}), json!({"head": "NotMine", "why": "2add7bf"})] {
            assert_eq!(complete_refs(&h, &cs, &x, key), h);
        }
    }


    /// Through coordd (N-159 post 125): a step in `layer on c`, c a cut of two repositories, answered with a bare commit
    /// of a: the ticket's result, and the record, carry the whole cut with a's commit replaced; the record replays.
    #[test]
    fn a_fixers_bare_commit_in_a_layer_on_the_cut_comes_back_as_the_whole_cut() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-layer-on-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        let git = |repo: &std::path::Path, args: &[&str]| {
            let o = std::process::Command::new("git").args(["-c", "user.name=t", "-c", "user.email=t@t"]).args(args).current_dir(repo).output().unwrap();
            assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        };
        let mut cut = Map::new();
        for r in ["a", "b"] {
            let p = coord.join(r);
            std::fs::create_dir_all(&p).unwrap();
            git(&p, &["init", "-q"]);
            std::fs::write(p.join("f"), r).unwrap();
            git(&p, &["add", "f"]);
            git(&p, &["commit", "-q", "-m", r]);
            cut.insert(r.into(), json!(git(&p, &["rev-parse", "HEAD"])));
        }
        // the fix, on top of the cut's a
        std::fs::write(coord.join("a/f"), "fixed").unwrap();
        git(&coord.join("a"), &["commit", "-q", "-am", "fix"]);
        let fix = git(&coord.join("a"), &["rev-parse", "HEAD"]);
        let project = coord.join("files");
        std::fs::create_dir_all(project.join("kinds")).unwrap();
        std::fs::create_dir_all(project.join("calls")).unwrap();
        std::fs::write(project.join("calls/m.call"), "call mend(cut: Ref) -> Commit(commit: Ref) | Cannot(why: Text)\n  template\n    Mend {{ cut }}.\n").unwrap();
        std::fs::write(project.join("kinds/m.kind"), "kind mending\n  letter M\n  form c: Ref\n  result Ref\n  pattern\n    layer on c {\n      r <- step dev mend(c);\n      match r {\n        Commit(n) => return n\n        Cannot(w) => return c\n      }\n    }\n").unwrap();
        let (a, b) = (coord.join("a").display().to_string(), coord.join("b").display().to_string());
        std::fs::write(project.join("project.toml"), format!("[project]\nname = \"m\"\nrepo = \"{a}\"\nrepos = {{ a = \"{a}\", b = \"{b}\" }}\n")).unwrap();
        std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"dev\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        core.load_tickets();
        assert!(core.tickets.errors.is_empty(), "{:?}", core.tickets.errors);
        core.ticket_input(docket_machine::Input::Open { kind: "mending".into(), under: None, opener: "owner".into(), form: json!({"c": cut}), text: None, names: vec![], told: vec![] }).unwrap();
        let t = core.tickets.m.tickets.values().find(|x| x.kind == "mending").map(|x| x.t).unwrap();
        assert!(layer_on(&core.tickets.m, t).is_some(), "in layer on c: {:?}", core.tickets.m.tickets[&t].status);
        core.ticket_input(docket_machine::Input::Answer { t, by: "dev".into(), head: json!({"head": "Commit", "commit": &fix[..7]}), text: docket_machine::Text::New("fixed".into()) }).unwrap();
        let mut whole = cut.clone();
        whole.insert("a".into(), json!(fix));
        assert!(matches!(&core.tickets.m.tickets[&t].status, docket_machine::Status::Done(v) if *v == Value::Object(whole.clone())), "{:?}", core.tickets.m.tickets[&t].status);
        let again = crate::ticket::Tickets::load(&coord.display().to_string(), &base, core.project.as_ref().map(|p| p.dir.as_str()), 1);
        assert!(again.errors.is_empty(), "{:#?}", again.errors);
        assert_eq!(again.m.view(t), core.tickets.m.view(t));
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// Home's press on a finding of the walk: coordd asks the runner's service `{"free": PATH}` for the owner alone, and
    /// says what was freed, or the runner's refusal, in words.
    #[tokio::test]
    async fn a_finding_is_freed_by_the_runners_service_for_the_owner_alone() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        let d = std::env::temp_dir().join(format!("coordd-free-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let sock = d.join("sock");
        let l = tokio::net::UnixListener::bind(&sock).unwrap();
        let answers = [r#"{"freed": 3221225472, "path": "/w/target"}"#, r#"{"error": "/w/x is not one of the last walk's findings; walk again first"}"#];
        let serve = tokio::spawn(async move {
            let mut asked = Vec::new();
            for a in answers {
                let (s, _) = l.accept().await.unwrap();
                let (r, mut w) = s.into_split();
                let mut q = String::new();
                tokio::io::BufReader::new(r).read_line(&mut q).await.unwrap();
                asked.push(serde_json::from_str::<Value>(&q).unwrap());
                w.write_all(format!("{a}\n").as_bytes()).await.unwrap();
            }
            asked
        });
        let agent = free_at(&sock, &Who::Agent("w3".into()), "/w/target").await;
        assert!(!agent.ok && agent.out.contains("only the owner"), "{agent:?}");
        let r = free_at(&sock, &Who::Owner, "/w/target").await;
        assert!(r.ok && r.out == "/w/target: 3221225472 bytes freed", "{r:?}");
        let r = free_at(&sock, &Who::Owner, "/w/x").await;
        assert!(!r.ok && r.out == "coord store: /w/x is not one of the last walk's findings; walk again first", "{r:?}");
        assert_eq!(serve.await.unwrap(), vec![json!({"free": "/w/target"}), json!({"free": "/w/x"})], "the agent's press never reached the runner");
        assert_eq!(runner_socket(), crate::jobs::dir().with_file_name("sock"), "the socket beside the job files");
        let _ = std::fs::remove_dir_all(&d);
    }
}
