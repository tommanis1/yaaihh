//! A session's start on the daemon's side: each party's profile, its hook socket, and the command that spawns its harness.

// The session driver carries `Start-Session` and is given the profile by party and the way to spawn. The spawn
// command starts the harness detached, in its terminal as the launcher makes it, and prints the cgroup it started it
// in; the parties' table then vouches for that cgroup as the agent's party, its runs bound to the hooks driver and
// to the agent's folder. What the program decides (the box, asking, the safety check) comes in `Start-Session`, not here.

use crate::link::Sessions;
use crate::peer::{written, Party, Table};
use docket_driver_session::{Profile, Session};
use docket_hooks::start::{Launch, Prompt, Switches};
use docket_v2::run::value::Value;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;

/// A profile as the profiles file writes it, by party: `{"w1": {"harness": "codex", "version": "0.1", "folder": "/w"}}`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Written {
    harness: String,
    version: String,
    /// The agent's working folder, which its runs are bound to.
    folder: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    tools_off: Vec<String>,
    #[serde(default)]
    parts_off: Vec<String>,
    /// The system text's file, and `append` or `replace`.
    #[serde(default)]
    prompt: Option<(String, String)>,
}

/// The profiles in `text`, each party's with its folder.
pub fn profiles(text: &str) -> Result<BTreeMap<String, (Profile, String)>, String> {
    let all: BTreeMap<String, Written> = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let profile = |(party, w): (String, Written)| {
        let prompt = match w.prompt {
            None => None,
            Some((file, how)) => Some((file, match how.as_str() {
                "append" => Prompt::Append,
                "replace" => Prompt::Replace,
                h => return Err(format!("{party}: a system text is given by append or replace, not {h}")),
            })),
        };
        let switches = Switches { tools_off: w.tools_off, parts_off: w.parts_off, ..Switches::default() };
        Ok((party, (Profile { harness: w.harness, version: w.version, mode: w.mode, switches, prompt, model: String::new() }, w.folder)))
    };
    all.into_iter().map(profile).collect()
}

/// Where a start puts an agent's hook socket and its harness's home, and the command that spawns the harness:
/// `SPAWN PARTY HARNESS HOME FOLDER MODEL -- ARGS...` (MODEL empty: the harness's own), the start's environment
/// given, its last line out the cgroup. It returns once the harness is started: the daemon serves nobody until it has.
pub struct Places {
    pub hooks: PathBuf,
    pub homes: PathBuf,
    pub spawn: PathBuf,
    /// The machine's place of the hooks driver, which carries every frame's Effects: an agent's runs are bound to it.
    pub carrier: usize,
}

/// The linked driver for profile `p`: the one named `HARNESS/VERSION`, or else the only one of its harness.
fn driver_for(names: &[String], p: &Profile) -> Result<usize, String> {
    let want = format!("{}/{}", p.harness, p.version);
    let mut same = names.iter().enumerate().filter(|(_, n)| n.split('/').next() == Some(p.harness.as_str()));
    names.iter().position(|n| *n == want).or(match (same.next(), same.next()) {
        (Some((k, _)), None) => Some(k),
        _ => None,
    })
    .ok_or(format!("no one linked driver for {want}"))
}

/// The harness's arguments, before and after its start's own, that start a copy of the session whose transcript is at
/// `transcript`: the file's name ends with the session's id.
pub fn copy_args(harness: &str, transcript: &str) -> Result<(Vec<String>, Vec<String>), String> {
    let stem = std::path::Path::new(transcript).file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    let id = stem.get(stem.len().saturating_sub(36)..).filter(|id| id.len() == 36 && id.char_indices().all(|(i, c)| if [8, 13, 18, 23].contains(&i) { c == '-' } else { c.is_ascii_hexdigit() }));
    let id = id.ok_or(format!("{transcript} names no session id"))?.to_string();
    match harness {
        docket_driver_claude::HARNESS => Ok((vec![], vec!["--resume".into(), id, "--fork-session".into()])),
        docket_driver_codex::HARNESS => Ok((vec!["fork".into()], vec![id])),
        h => Err(format!("no copy of a {h} session is started")),
    }
}

/// A start of a copy of a party's session: given the party and its transcript's path, the copy's party.
pub type Copy = Box<dyn FnMut(&Value, &str) -> Result<Value, String>>;

/// The file in a started party's home that keeps what the program chose at its start: its model, asking and safety check.
pub const STARTED: &str = "started.json";

/// The session driver: profiles by party from `profiles`, hook sockets from the linked `frames` (each by its written
/// name, at its place among the machine's drivers), each spawned harness vouched for in `table`; and the start of a
/// copy of a session it started, by the profile of the party (of a copy: the party it was copied from) and what its
/// start kept ([`STARTED`]), named `PARTY.copy-N` for the first N with no home yet, in the party's folder.
pub fn session(
    relay: String,
    profiles: BTreeMap<String, (Profile, String)>,
    frames: Vec<(String, Rc<RefCell<dyn Sessions>>)>,
    table: Rc<Table>,
    places: Places,
    reads: Option<fn(&str) -> bool>,
) -> (Session<impl FnMut(&Value) -> Option<Profile>, impl FnMut(&Value, &Profile, Launch) -> Result<(), String>>, Copy) {
    let names: Vec<String> = frames.iter().map(|(n, _)| n.clone()).collect();
    let folders: Rc<RefCell<BTreeMap<String, String>>> = Rc::new(RefCell::new(profiles.iter().map(|(n, (_, f))| (n.clone(), f.clone())).collect()));
    let profiles = Rc::new(profiles);
    let (copy_profiles, copy_folders, copy_relay, homes) = (profiles.clone(), folders.clone(), relay.clone(), places.homes.clone());
    let profile = move |agent: &Value| profiles.get(&written(std::slice::from_ref(agent))).map(|(p, _)| p.clone());
    let spawn = Rc::new(move |agent: &Value, p: &Profile, l: Launch| {
        let party = written(std::slice::from_ref(agent));
        if reads.is_some_and(|r| !r(&p.harness)) {
            return Err(format!("{party} is not started: the turns driver reads no transcript of {}, so it would run deaf", p.harness));
        }
        let folder = folders.borrow().get(&party).cloned().unwrap_or_default();
        let d = driver_for(&names, p)?;
        let frame = &frames[d].1;
        let socket = frame.borrow_mut().open(&party, &places.hooks).map_err(|e| format!("{party}'s hook socket: {e}"))?;
        let started = (|| {
            let home = places.homes.join(&party);
            std::fs::create_dir_all(&home).map_err(|e| format!("{}: {e}", home.display()))?;
            for (name, text) in &l.files {
                std::fs::write(home.join(name), text).map_err(|e| format!("{}: {e}", home.join(name).display()))?;
            }
            for n in &l.not_taken {
                eprintln!("coordd-v2: {party}'s start does not take: {n}");
            }
            let mut c = Command::new(&places.spawn);
            c.arg(&party).arg(&p.harness).arg(&home).arg(&folder).arg(&p.model).arg("--").args(&l.args);
            let out = c.envs(l.env.iter().cloned()).env("DOCKET_HOOK_SOCKET", &socket).output().map_err(|e| format!("{}: {e}", places.spawn.display()))?;
            let last = |b: &[u8]| String::from_utf8_lossy(b).lines().last().unwrap_or_default().trim().to_string();
            if !out.status.success() {
                return Err(format!("{} for {party}: {}", places.spawn.display(), last(&out.stderr)));
            }
            Some(last(&out.stdout)).filter(|c| c.starts_with('/')).ok_or(format!("{} printed no cgroup for {party}", places.spawn.display()))
        })();
        let added = started.and_then(|cgroup| table.add(cgroup, Party { name: party.clone(), driver: Some(places.carrier), folder }));
        if added.is_ok() {
            // kept for a copy of the session, after a restart too; without it a copy fails, saying so
            let (kept, chosen) = (places.homes.join(&party).join(STARTED), serde_json::json!({"harness": p.harness, "model": p.model, "unasked": p.switches.unasked, "safety_check": p.switches.safety_check}));
            if let Err(e) = std::fs::write(&kept, chosen.to_string()) {
                eprintln!("coordd-v2: {party}'s start is not kept at {}: {e}", kept.display());
            }
        }
        added.inspect_err(|_| frame.borrow_mut().close(&party))
    });
    let copy_spawn = spawn.clone();
    let copy: Copy = Box::new(move |parent, transcript| {
        let name = written(std::slice::from_ref(parent));
        // a copy of a copy runs on the profile of the party it was copied from
        let mut base = name.as_str();
        while let Some((b, _)) = base.rsplit_once(".copy-").filter(|(_, n)| n.parse::<u32>().is_ok()) {
            base = b;
        }
        let (mut p, folder) = copy_profiles.get(base).cloned().ok_or(format!("{base} has no profile, so no copy of {name} is"))?;
        let kept = homes.join(&name).join(STARTED);
        let chosen: serde_json::Value = std::fs::read_to_string(&kept).ok().and_then(|t| serde_json::from_str(&t).ok()).ok_or(format!("{name}'s start is not kept at {}, so no copy of it is", kept.display()))?;
        let (Some(model), Some(check)) = (chosen["model"].as_str(), chosen["safety_check"].as_bool()) else { return Err(format!("{} is not a start's", kept.display())) };
        (p.model, p.switches.safety_check, p.switches.unasked) = (model.to_string(), check, chosen["unasked"].as_bool().unwrap_or(false));
        let (before, after) = copy_args(&p.harness, transcript)?;
        let mut l = docket_driver_session::launch(&p, &copy_relay)?;
        l.args = before.into_iter().chain(l.args).chain(after).collect();
        let fork = (1..).map(|n| format!("{name}.copy-{n}")).find(|f| !homes.join(f).exists()).unwrap_or_default();
        copy_folders.borrow_mut().insert(fork.clone(), folder);
        copy_spawn(&Value::text(&fork), &p, l).map(|()| Value::text(&fork))
    });
    (Session { relay, profile, spawn: move |a: &Value, p: &Profile, l: Launch| spawn(a, p, l) }, copy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_is_read_by_party_and_finds_its_harnesss_driver() {
        let ps = profiles(r#"{"w1": {"harness": "agy", "version": "agy-1.5.18", "folder": "/w", "mode": "plan", "tools_off": ["Bash"], "prompt": ["s.md", "append"]}, "w2": {"harness": "codex", "version": "9", "folder": "/v"}}"#).unwrap();
        let (w1, folder) = &ps["w1"];
        assert_eq!((w1.mode.as_deref(), w1.switches.tools_off.len(), w1.prompt.clone(), folder.as_str()), (Some("plan"), 1, Some(("s.md".into(), Prompt::Append)), "/w"));
        let names = ["claude-code/1".to_string(), "codex/0.1".into(), "agy/t3code-0.1.11".into(), "agy/agy-1.5.18".into()];
        assert_eq!(driver_for(&names, &ps["w1"].0), Ok(3), "by its version");
        assert_eq!(driver_for(&names, &ps["w2"].0), Ok(1), "the only one of its harness");
        assert!(driver_for(&names, &Profile { harness: "agy".into(), version: "2".into(), ..Default::default() }).is_err(), "two of its harness, neither its version");
        assert!(profiles(r#"{"w1": {"harness": "codex", "version": "1", "folder": "/w", "prompt": ["s.md", "after"]}}"#).is_err());
        for what in ["boxed\": true", "safety_check\": false"] {
            assert!(profiles(&format!(r#"{{"w1": {{"harness": "codex", "version": "1", "folder": "/w", "{what}}}}}"#)).is_err(), "the program decides {what}");
        }
    }

    /// A start writes the harness's files in its home, opens the agent's hook socket, gives the spawn command its
    /// path, and vouches for the cgroup printed; a spawn that fails, or prints another party's cgroup, closes the
    /// socket again and vouches for nothing.
    #[test]
    fn a_start_spawns_the_harness_and_vouches_for_its_cgroup() {
        use crate::peer::Vouch;
        use docket_v2::run::driver::{Driver, Reply};
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("start-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("hooks")).unwrap();
        let spawn = dir.join("spawn");
        // a copy is put in its cgroup's parent: the table vouches for one party per cgroup
        std::fs::write(&spawn, "#!/bin/sh\n[ \"$1\" = w9 ] && { echo cannot >&2; exit 1; }\necho \"$1 $2 $4 $5 $DOCKET_HOOK_SOCKET\" > \"$3/seen\"\nh=$3; shift 6; echo \"$*\" > \"$h/args\"\nc=$(sed -n s/^0:://p /proc/self/cgroup)\ncase $h in *.copy-2.copy-1) dirname \"$(dirname \"$(dirname \"$c\")\")\" ;; *.copy-1) dirname \"$c\" ;; *.copy-2) dirname \"$(dirname \"$c\")\" ;; *) echo \"$c\" ;; esac\n").unwrap();
        std::fs::set_permissions(&spawn, std::fs::Permissions::from_mode(0o755)).unwrap();
        let codex = Rc::new(RefCell::new(docket_hooks::Frame::new(docket_driver_codex::Codex)));
        let frames: Vec<(String, Rc<RefCell<dyn Sessions>>)> = vec![("codex/1".into(), codex)];
        let table = Rc::new(Table::read("", &[]).unwrap());
        let ps = profiles(r#"{"w1": {"harness": "codex", "version": "1", "folder": "/w"}, "w2": {"harness": "codex", "version": "1", "folder": "/v"}, "w9": {"harness": "codex", "version": "1", "folder": "/u"}}"#).unwrap();
        let places = Places { hooks: dir.join("hooks"), homes: dir.join("homes"), spawn, carrier: 4 };
        let (mut s, mut copy) = session("/relay".into(), ps, frames, table.clone(), places, Some(|h: &str| h == "codex"));
        assert!(matches!(s.carry(docket_driver_session::START, &[Value::text("w1"), Value::Bool(false), Value::Bool(false), Value::Bool(true), Value::text("m-1")], 0), Reply::Answer { .. }));
        let socket = dir.join("hooks/w1.hooks");
        assert_eq!(std::fs::read_to_string(dir.join("homes/w1/seen")).unwrap().trim(), format!("w1 codex /w m-1 {}", socket.display()), "the model the program passed, unread");
        assert!(dir.join("homes/w1/hooks.json").exists() && socket.exists());
        let kept: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("homes/w1").join(STARTED)).unwrap()).unwrap();
        assert_eq!((&kept["unasked"], &kept["safety_check"]), (&false.into(), &true.into()), "what the program chose, kept for its copies");
        assert_eq!(kept["harness"], "codex", "and its harness, by which the turns driver hands it words");
        let mine = crate::peer::cgroup_of(std::process::id() as i32).unwrap();
        assert_eq!(table.party(&mine), Some(Party { folder: "/w".into(), ..Party::of("w1", Some(4)) }), "its runs bound to its folder and to the frames' carrier");
        let Reply::Failed(why) = s.carry(docket_driver_session::START, &[Value::text("w9"), Value::Bool(false), Value::Bool(false), Value::Bool(true), Value::text("")], 1) else { panic!("w9's spawn fails") };
        assert!(why.ends_with("for w9: cannot"), "{why}");
        assert!(!dir.join("hooks/w9.hooks").exists(), "its socket closed again");
        let Reply::Failed(why) = s.carry(docket_driver_session::START, &[Value::text("w2"), Value::Bool(false), Value::Bool(false), Value::Bool(true), Value::text("")], 2) else { panic!("w2 is given w1's cgroup") };
        assert!(why.contains("vouched for as w1") && !dir.join("hooks/w2.hooks").exists(), "{why}");
        assert_eq!(table.party(&mine).map(|p| p.name), Some("w1".into()));
        let id = "019a6b2c-1d2e-7f30-8a41-b52c63d74e85";
        assert!(copy(&Value::text("w2"), "/t/x.jsonl").is_err_and(|e| e.contains("is not kept")), "w2's start failed");
        assert!(copy(&Value::text("w1"), "/t/no-id.jsonl").is_err_and(|e| e.contains("names no session id")));
        assert_eq!(copy(&Value::text("w1"), &format!("/t/rollout-2026-11-26T10-00-00-{id}.jsonl")), Ok(Value::text("w1.copy-1")));
        let args = std::fs::read_to_string(dir.join("homes/w1.copy-1/args")).unwrap();
        assert!(args.starts_with("fork ") && args.trim().ends_with(id), "codex fork, its start's own args between: {args}");
        assert_eq!(std::fs::read_to_string(dir.join("homes/w1.copy-1/seen")).unwrap().split(' ').nth(2), Some("/w"), "in w1's folder");
        let parent = std::path::Path::new(&mine).parent().unwrap().display().to_string();
        assert_eq!(table.party(&parent).map(|p| (p.name, p.folder)), Some(("w1.copy-1".into(), "/w".into())));
        assert_eq!(copy_args("claude-code", &format!("/p/{id}.jsonl")), Ok((vec![], vec!["--resume".into(), id.into(), "--fork-session".into()])));
        // after a restart: the profile by name, the model w1's start kept, the next free name
        let codex = Rc::new(RefCell::new(docket_hooks::Frame::new(docket_driver_codex::Codex)));
        let ps = profiles(r#"{"w1": {"harness": "codex", "version": "1", "folder": "/w"}}"#).unwrap();
        let places = Places { hooks: dir.join("hooks"), homes: dir.join("homes"), spawn: dir.join("spawn"), carrier: 4 };
        let (_, mut copy) = session("/relay".into(), ps, vec![("codex/1".into(), codex)], table.clone(), places, None);
        assert_eq!(copy(&Value::text("w1"), &format!("/t/rollout-x-{id}.jsonl")), Ok(Value::text("w1.copy-2")));
        assert_eq!(std::fs::read_to_string(dir.join("homes/w1.copy-2/seen")).unwrap().split(' ').nth(3), Some("m-1"), "the model w1 was started with");
        // a copy of a copy: w1's profile and folder, the copy's kept start
        assert_eq!(copy(&Value::text("w1.copy-2"), &format!("/t/rollout-y-{id}.jsonl")), Ok(Value::text("w1.copy-2.copy-1")));
        let seen: Vec<String> = std::fs::read_to_string(dir.join("homes/w1.copy-2.copy-1/seen")).unwrap().split(' ').map(String::from).collect();
        assert_eq!((seen[1].as_str(), seen[2].as_str(), seen[3].as_str()), ("codex", "/w", "m-1"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
