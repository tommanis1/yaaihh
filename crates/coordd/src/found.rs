//! Edits made through Bash, found (N-101 p117, accepted p118). An agent that edits with a redirect, `sed` or a script
//! leaves no `act edit` (the post hook sees only Claude Code's own edit tools), so no duty to typecheck arises. In a
//! layer, all that its agents write is in the layer's upper folders (`layers/ID/up`, and `parts/NAME/up` for each other
//! repository; a deletion is a 0/0 character device); build outputs, `.git` and the conversations are bound from
//! outside them. So coordd keeps, per layer, what it last saw there (`layers/ID/found.json`), and looks again:
//!
//! - before it records a submit (`coord check`), so what was written before it is covered by it;
//! - before it takes an agent's answer (`coord answer`), so a hand-in's `answering` excuses it;
//! - at an agent's turn end, before the machine takes it.
//!
//! What changed since the last look, less what an `act edit` covered (the hook's and the symbols', `covered`: the
//! path's state then is taken as seen) and less what git ignores, is one `act edit` by the agent whose look it was, the
//! same act the hook writes, with `files`, `found` (where it was found) and `turns` (every agent that had a turn in the
//! layer since the last look: when there are two, which one wrote is not known); on the ticket the agent owes a step
//! on, else on the layer's ticket (a minder owes none). A commit is seen by what it writes in the view: one that
//! changes no file there is no edit. An agent outside any layer gets nothing from this: its tree is shared, and only
//! the hook's acts are its edits.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// How a path of a layer's upper folders stood: a deletion (`gone`), or its size, modification and change times.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seen {
    size: u64,
    mtime: i64,
    ctime: i64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    gone: bool,
}

/// What coordd last saw of a layer: each path (as its agents see it: the repository's path and the path in it), and
/// the agents that had a turn in it since.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snap {
    seen: BTreeMap<String, Seen>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    turns: BTreeSet<String>,
}

/// One look or cover at a time, over every layer.
static LOCK: Mutex<()> = Mutex::new(());

/// A layer as found needs it: its ID, its ticket, and each upper folder with the repository path it lies over.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub id: String,
    pub ticket: String,
    pub roots: Vec<(String, String)>,
}

/// The open layer agent NAME works in (`agents/NAME/layer`, written when it was started in the view).
pub fn layer_of(coord: &str, name: &str) -> Option<Place> {
    let id = std::fs::read_to_string(format!("{coord}/agents/{name}/layer")).ok()?.trim().to_string();
    crate::layer::load(coord).get(&id).filter(|l| l.state == "open").map(|l| place(coord, l))
}

fn place(coord: &str, l: &crate::layer::Layer) -> Place {
    let mut roots = vec![(l.up.clone(), l.repo.clone())];
    roots.extend(l.also.iter().map(|p| (format!("{coord}/layers/{}/parts/{}/up", l.id, p.name), p.repo.clone())));
    Place { id: l.id.clone(), ticket: l.ticket.clone(), roots }
}

fn snap_path(coord: &str, id: &str) -> String {
    format!("{coord}/layers/{id}/found.json")
}

/// Layer L's view is up (`layer::open`): its record of what was seen, if it has none yet, is what is in it now
/// (nothing, for a new layer).
pub fn opened(coord: &str, l: &crate::layer::Layer) {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !std::path::Path::new(&snap_path(coord, &l.id)).exists() {
        save(coord, &l.id, &Snap { seen: walk_all(&place(coord, l)), turns: BTreeSet::new() });
    }
}

fn save(coord: &str, id: &str, s: &Snap) {
    let p = snap_path(coord, id);
    let tmp = format!("{p}.tmp");
    if let Err(e) = std::fs::write(&tmp, serde_json::to_vec(s).unwrap_or_default()).and_then(|_| std::fs::rename(&tmp, &p)) {
        eprintln!("coordd: found: {p}: {e}");
    }
}

/// What was last seen of layer P; a layer opened before this was built has none, and is taken as it stands now.
fn load(coord: &str, p: &Place) -> Snap {
    match std::fs::read(snap_path(coord, &p.id)) {
        Ok(b) => serde_json::from_slice(&b).unwrap_or_default(),
        Err(_) => {
            eprintln!("coordd: found: layer {} has no record of what was seen in it (opened before edits were found); taken as it stands now", p.id);
            Snap { seen: walk_all(p), turns: BTreeSet::new() }
        }
    }
}

fn seen_of(m: &std::fs::Metadata) -> Seen {
    Seen { size: m.len(), mtime: m.mtime() * 1_000_000_000 + m.mtime_nsec(), ctime: m.ctime() * 1_000_000_000 + m.ctime_nsec(), gone: m.file_type().is_char_device() && m.rdev() == 0 }
}

/// Every path under upper folder UP but its folders, as at REPO, and how it stands. A folder that cannot be read is
/// skipped (said in the journal).
pub fn walk(up: &str, repo: &str) -> BTreeMap<String, Seen> {
    let mut out = BTreeMap::new();
    let mut stack = vec![String::new()];
    while let Some(rel) = stack.pop() {
        let dir = if rel.is_empty() { up.to_string() } else { format!("{up}/{rel}") };
        let rd = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(e) => {
                if rel.is_empty() && e.kind() == std::io::ErrorKind::NotFound {
                    continue;
                }
                eprintln!("coordd: found: {dir}: {e}");
                continue;
            }
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let r = if rel.is_empty() { name } else { format!("{rel}/{name}") };
            let Ok(m) = std::fs::symlink_metadata(e.path()) else { continue };
            if m.is_dir() {
                stack.push(r);
            } else {
                out.insert(format!("{}/{r}", repo.trim_end_matches('/')), seen_of(&m));
            }
        }
    }
    out
}

fn walk_all(p: &Place) -> BTreeMap<String, Seen> {
    p.roots.iter().flat_map(|(up, repo)| walk(up, repo)).collect()
}

/// The paths of what changed between WAS and NOW: new, changed, or no longer in the upper folders.
pub fn changed(was: &BTreeMap<String, Seen>, now: &BTreeMap<String, Seen>) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = now.iter().filter(|(k, v)| was.get(*k) != Some(v)).map(|(k, _)| k.clone()).collect();
    out.extend(was.keys().filter(|k| !now.contains_key(*k)).cloned());
    out
}

/// Of PATHS, those git ignores in the repository each lies in (`check-ignore`, asked once per repository that has
/// any).
fn ignored(roots: &[(String, String)], paths: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (_, repo) in roots {
        let prefix = format!("{}/", repo.trim_end_matches('/'));
        let rels: Vec<&str> = paths.iter().filter_map(|p| p.strip_prefix(&prefix)).collect();
        if rels.is_empty() {
            continue;
        }
        out.extend(check_ignore(repo, &rels).into_iter().map(|l| format!("{prefix}{l}")));
    }
    out
}

/// Of RELS (paths relative to REPO), those git ignores there. The paths are written to git from a thread of their own
/// while its answers are read: written first and read after, a list whose answers fill the pipe (64 KiB) stopped git
/// writing, so it stopped reading, and the write never returned, with the found lock held (Tovrin-dev's coordd frozen
/// at 10:42 on 17 Nov, N-101 p1705).
fn check_ignore(repo: &str, rels: &[&str]) -> Vec<String> {
    let child = Command::new("git").args(["-c", "safe.directory=*", "-C", repo, "check-ignore", "--stdin"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
    let Ok(mut child) = child else { return Vec::new() };
    let mut text = rels.join("\n");
    text.push('\n');
    let writer = child.stdin.take().map(|mut i| std::thread::spawn(move || {
        let _ = i.write_all(text.as_bytes());
    }));
    let o = child.wait_with_output();
    if let Some(w) = writer {
        let _ = w.join();
    }
    match o {
        Ok(o) => String::from_utf8_lossy(&o.stdout).lines().map(str::to_string).collect(),
        Err(_) => Vec::new(),
    }
}

/// Agent NAME began a turn: in a layer, it is among those who had a turn there since the last look.
pub fn turn_began(coord: &str, name: &str) {
    let Some(p) = layer_of(coord, name) else { return };
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = load(coord, &p);
    if s.turns.insert(name.to_string()) {
        save(coord, &p.id, &s);
    }
}

/// An `act edit` of agent NAME named FILES (the hook's, the symbols'): in a layer, how each stands now is taken as
/// seen, so it is not found again; a later write to it is.
pub fn covered(coord: &str, name: &str, files: &[String]) {
    let Some(p) = layer_of(coord, name) else { return };
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = load(coord, &p);
    for f in files {
        // a path in the repository, as the agent sees it; a relative one is in the project's own repository
        let abs = if f.starts_with('/') { f.clone() } else { format!("{}/{f}", p.roots[0].1.trim_end_matches('/')) };
        let Some((up, rel)) = p.roots.iter().find_map(|(up, repo)| abs.strip_prefix(&format!("{}/", repo.trim_end_matches('/'))).map(|r| (up, r))) else { continue };
        match std::fs::symlink_metadata(format!("{up}/{rel}")) {
            Ok(m) => s.seen.insert(abs, seen_of(&m)),
            Err(_) => s.seen.remove(&abs),
        };
    }
    save(coord, &p.id, &s);
}

/// Agent NAME's look AT (`submit`, `answer`, `turn_end`): in a layer, what changed there since the last look and no
/// `act edit` covered: that act (`{by, is: edit, text, files, found, turns}`) and the layer's ticket; None when nothing did, or
/// NAME is in no layer. STILL: the agents in a turn now, who count as having one at the next look as well.
pub fn look(coord: &str, name: &str, at: &str, still: &BTreeSet<String>) -> Option<(Value, String)> {
    let p = layer_of(coord, name)?;
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = load(coord, &p);
    let now = walk_all(&p);
    let mut files = changed(&s.seen, &now);
    if !files.is_empty() {
        let ig = ignored(&p.roots, &files);
        files.retain(|f| !ig.contains(f));
    }
    let mut turns = std::mem::take(&mut s.turns);
    turns.insert(name.to_string());
    s.turns = turns.iter().filter(|a| still.contains(*a)).cloned().collect();
    s.seen = now;
    save(coord, &p.id, &s);
    if files.is_empty() {
        return None;
    }
    let files: Vec<String> = files.into_iter().collect();
    let shown: Vec<&str> = files.iter().take(3).map(String::as_str).collect();
    let text = format!("found at {}: {}{}", at.replace('_', " "), shown.join(", "), if files.len() > 3 { format!(" and {} more", files.len() - 3) } else { String::new() });
    Some((json!({"by": name, "is": "edit", "text": text, "files": files, "found": at, "turns": turns}), p.ticket))
}

impl crate::core::Core {
    /// Agent NAME's look AT (`found::look`): an edit found is recorded now, before what follows it is taken.
    pub fn look_found(&mut self, name: &str, at: &str) {
        let still: BTreeSet<String> = self.agents.iter().filter(|(n, a)| a.running() && a.live.state == "working" && !(at == "turn_end" && n.as_str() == name)).map(|(n, _)| n.clone()).collect();
        if let Some((v, ticket)) = look(&self.coord, name, at, &still) {
            let (agent, is, text, more) = docket_machine::runner::act_of_event(&v);
            let t = docket_machine::parse_ticket(&ticket);
            if let Err(e) = self.ticket_input(docket_machine::Input::Act { agent, is, text, more, t }) {
                eprintln!("coordd: found: {name}'s edit found at {at}: {e}");
            }
        }
    }
}

#[cfg(test)]
mod pipe_tests {
    use std::collections::BTreeSet;

    /// A path list whose questions and answers are each far over a pipe (64 KiB): every ignored path is answered, and
    /// the call returns (written first and read after, it never did: N-101 p1705).
    #[test]
    fn a_list_larger_than_a_pipe_each_way_is_answered() {
        let repo = std::env::temp_dir().join(format!("coordd-found-pipe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).unwrap();
        let ok = std::process::Command::new("git").args(["init", "-q"]).current_dir(&repo).status().unwrap();
        assert!(ok.success());
        std::fs::write(repo.join(".gitignore"), "*.o\n").unwrap();
        let r = repo.to_str().unwrap().to_string();
        // 30000 paths of about 40 bytes: some 1.2 MB asked, the 20000 ignored ones some 0.8 MB answered
        let paths: BTreeSet<String> = (0..30000).map(|i| format!("{r}/build/objects/f{i:06}.{}", if i % 3 == 0 { "c" } else { "o" })).collect();
        let roots = vec![(r.clone(), r.clone())];
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(super::ignored(&roots, &paths));
        });
        let got = rx.recv_timeout(std::time::Duration::from_secs(120)).expect("ignored did not return: the pipes are stuck");
        assert_eq!(got.len(), 20000);
        assert!(got.iter().all(|p| p.ends_with(".o")));
        let _ = std::fs::remove_dir_all(&repo);
    }
}
