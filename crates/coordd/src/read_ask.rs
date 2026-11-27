//! A Safe agent's read outside its folders, which Claude Code would ask a person about: its permission hook asks coordd
//! (`claude/hook`), which lets it through when the project's read folders hold the path, refuses it at once when it is
//! a place no agent reads, and otherwise puts one card in the agent's chat and holds the call until the owner answers
//! or [`WAIT`] runs out. No question ticket and no message: his answer is the call's.

use coord_types::{ReadAnswer, ReadAsk};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use tokio::sync::oneshot;

/// How long a read waits on him: below Claude Code's 600 s bound on the hook (settings.json).
pub const WAIT: std::time::Duration = std::time::Duration::from_secs(480);

/// Claude Code's tools that only read, and where each names its path.
const TOOLS: [(&str, &str); 5] = [("Read", "file_path"), ("NotebookRead", "notebook_path"), ("Glob", "path"), ("Grep", "path"), ("LS", "path")];

/// Credentials beside what coordd never serves, by a file's or folder's name: no agent reads them, whatever he allows.
const SECRETS: [&str; 12] = ["oauth_creds.json", "google_accounts.json", ".credentials.json", "credentials.json", "auth.json", ".netrc", ".gnupg", ".aws", ".kube", ".docker", ".password-store", ".pgpass"];

/// What coordd says to a read.
#[derive(Debug)]
pub enum Judged {
    Allow,
    Refuse(String),
    /// Ask ID waits on him; the answer comes on the receiver.
    Wait(u64, oneshot::Receiver<Result<(), String>>),
    /// Not a read this decides: the hook's own refusal stands.
    Pass,
}

type Waiting = (String, ReadAsk, Vec<oneshot::Sender<Result<(), String>>>);

/// The reads waiting on him, and his answers that are not kept in the project file.
#[derive(Default)]
pub struct Asks {
    next: u64,
    open: BTreeMap<u64, Waiting>,
    once: BTreeSet<(String, PathBuf)>,
    denied: BTreeSet<(String, PathBuf)>,
}

/// The path a permission request INPUT reads and whether it names a folder; none for another tool, a path that is not
/// whole, or a session that bypasses prompts (its safety check is not about reads).
pub fn asked(input: &Value) -> Option<(String, bool)> {
    if input["permission_mode"].as_str() == Some("bypassPermissions") {
        return None;
    }
    let tool = input["tool_name"].as_str()?;
    let (_, key) = TOOLS.iter().find(|(t, _)| *t == tool)?;
    let ti = &input["tool_input"];
    let named = ti[*key].as_str().filter(|p| !p.is_empty()).map(str::to_string);
    let path = match ti["pattern"].as_str().filter(|g| tool == "Glob" && g.starts_with('/')) {
        Some(g) => Some(named.map_or_else(|| glob_root(g), |n| above_both(&n, &glob_root(g)))),
        None => named,
    }?;
    path.starts_with('/').then_some((path, !matches!(tool, "Read" | "NotebookRead")))
}

/// Whether INPUT is a Glob whose pattern climbs with `..`, which no folder named bounds.
fn climbs(input: &Value) -> bool {
    input["tool_name"].as_str() == Some("Glob") && input["tool_input"]["pattern"].as_str().is_some_and(|g| g.split('/').any(|s| s == ".."))
}

/// The folder an absolute Glob pattern G reads: its whole part, up to the folder its first wildcard is in.
fn glob_root(g: &str) -> String {
    let lit = g.split(['*', '?', '[', '{']).next().unwrap_or("");
    let dir = if lit.len() < g.len() { lit.rsplit_once('/').map_or("", |(d, _)| d) } else { lit.trim_end_matches('/') };
    if dir.is_empty() { "/".into() } else { dir.into() }
}

/// The folder above both A and B, by their parts.
fn above_both(a: &str, b: &str) -> String {
    let same: PathBuf = Path::new(a).components().zip(Path::new(b).components()).take_while(|(x, y)| x == y).map(|(x, _)| x).collect();
    same.display().to_string()
}

/// RAW as the kernel walks it: part by part, each link followed before a `..` after it; parts not there taken as
/// written. None for a link that leads nowhere.
pub fn resolve(raw: &str) -> Option<PathBuf> {
    if !raw.starts_with('/') {
        return None;
    }
    let mut p = PathBuf::from("/");
    let mut there = true;
    for c in Path::new(raw).components() {
        match c {
            Component::Normal(n) => {
                p.push(n);
                if there {
                    match std::fs::canonicalize(&p) {
                        Ok(real) => p = real,
                        Err(_) if std::fs::symlink_metadata(&p).is_ok() => return None,
                        Err(_) => there = false,
                    }
                }
            }
            Component::ParentDir => {
                p.pop();
                there = there || std::fs::symlink_metadata(&p).is_ok();
            }
            _ => {}
        }
    }
    Some(p)
}

/// Why P is a place no agent reads, whatever he allows: what coordd never serves, and credentials by their names.
pub fn closed(home: &Path, p: &Path) -> Option<String> {
    if let Some(why) = crate::files::never_served(home, p) {
        return Some(why);
    }
    let secret = |n: &str| SECRETS.contains(&n) || n.starts_with("id_rsa") || n.starts_with("id_ed25519") || n.starts_with("id_ecdsa");
    p.components().any(|c| matches!(c, Component::Normal(n) if secret(&n.to_string_lossy()))).then(|| "a credential".into())
}

/// Why folder P, read whole by a tool that walks it, would reach a place no agent reads: the home or a folder above it,
/// a folder of logins or above one, a coord's own folder, or one with a credential directly in it. No walk: a lookup
/// of each name.
pub fn held(home: &Path, p: &Path) -> Option<String> {
    if home.starts_with(p) {
        return Some("the owner's home or a folder above it".into());
    }
    let logins = [".ssh", ".codex", ".config", ".local/opt", "agent-coord/accounts", ".claude", ".gemini", ".aws", ".gnupg", ".kube", ".docker", ".password-store"];
    if logins.iter().any(|l| home.join(l).starts_with(p)) {
        return Some("a folder of the owner's logins, or above one".into());
    }
    if p.join("registry.jsonl").exists() {
        return Some("a coord's own folder".into());
    }
    SECRETS.iter().find(|s| p.join(s).symlink_metadata().is_ok()).map(|s| format!("a folder holding {s}"))
}

/// The folder "Allow read-only for this folder" offers for P (FOLDER: P names one): P's own, or the one it is in; none
/// at the top of the machine or of the home, or above the home, where only P itself may be allowed.
pub fn offered(home: &Path, p: &Path, folder: bool) -> Option<PathBuf> {
    let f = if folder { p.to_path_buf() } else { p.parent()?.to_path_buf() };
    let wide = f.components().count() < 3 || home.starts_with(&f);
    (!wide && closed(home, &f).is_none() && held(home, &f).is_none()).then_some(f)
}

/// Whether P is in one of FOLDERS, each taken as the machine has it.
pub fn within(p: &Path, folders: &[String]) -> bool {
    folders.iter().filter_map(|f| resolve(f)).any(|f| p.starts_with(&f))
}

/// Claude Code's answer to a permission request: let through, or refused with WHY for the agent to read.
pub fn hook_answer(allow: Result<(), String>) -> String {
    let decision = match allow {
        Ok(()) => json!({"behavior": "allow"}),
        Err(why) => json!({"behavior": "deny", "message": why}),
    };
    json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": decision}}).to_string()
}

/// The words a read the owner has not answered in [`WAIT`] is refused with.
pub const UNANSWERED: &str = "denied for now: the owner has not answered yet. His card stays in your chat; go on with other work and read it again later, when an answer of his lets it through. Do not ask him another way.";

impl Asks {
    /// Agent AGENT's read INPUT, its read folders FOLDERS (the project file's `[read.NAME]` and `box_read`).
    pub fn judge(&mut self, agent: &str, input: &Value, home: &Path, folders: &[String], now: &str) -> Judged {
        let Some((raw, folder)) = asked(input) else { return Judged::Pass };
        if climbs(input) {
            return Judged::Refuse("denied: a Glob pattern that climbs with `..` is not judged. Give the folder it reads whole as its path.".into());
        }
        let Some(p) = resolve(&raw) else { return Judged::Pass };
        if let Some(why) = closed(home, &p) {
            return Judged::Refuse(format!("denied: {} is {why}, which no agent reads, whatever is allowed. Work from what you have, or ask your lead.", p.display()));
        }
        if within(&p, folders) {
            return Judged::Allow;
        }
        if let Some(why) = folder.then(|| held(home, &p)).flatten() {
            return Judged::Refuse(format!("denied: {} is {why}, which no agent reads whole. Name a folder or file inside it.", p.display()));
        }
        if self.once.remove(&(agent.to_string(), p.clone())) {
            return Judged::Allow;
        }
        if let Some((_, said)) = self.denied.iter().find(|(a, d)| a == agent && p.starts_with(d)) {
            return Judged::Refuse(format!("denied: the owner said no to reading {}. Do the work without it; do not try it another way.", said.display()));
        }
        let shown = p.display().to_string();
        let (tx, rx) = oneshot::channel();
        // one card for a path that waits already: this call waits on it too
        if let Some((id, (_, _, txs))) = self.open.iter_mut().find(|(_, (a, k, _))| a == agent && k.path == shown) {
            txs.retain(|t| !t.is_closed());
            txs.push(tx);
            return Judged::Wait(*id, rx);
        }
        self.next += 1;
        let ask = ReadAsk { id: self.next, path: shown, folder: offered(home, &p, folder).map(|f| f.display().to_string()), at: now.to_string() };
        self.open.insert(self.next, (agent.to_string(), ask, vec![tx]));
        Judged::Wait(self.next, rx)
    }

    /// Agent AGENT's reads that wait on him, oldest first.
    pub fn of(&self, agent: &str) -> Vec<ReadAsk> {
        self.open.values().filter(|(a, _, _)| a == agent).map(|(_, k, _)| k.clone()).collect()
    }

    /// His ANSWER to AGENT's ask ID: its waiting calls answered, and the folder to keep in the project file for
    /// [`ReadAnswer::Folder`], every other ask of AGENT's inside it let through as well.
    pub fn answer(&mut self, agent: &str, id: u64, answer: ReadAnswer) -> Result<(ReadAsk, Option<String>), String> {
        let Some((_, k, _)) = self.open.get(&id).filter(|(a, _, _)| a == agent) else { return Err(format!("{agent} has no read waiting as {id}")) };
        if answer == ReadAnswer::Folder && k.folder.is_none() {
            return Err(format!("{} may be allowed once only: its folder is too wide", k.path));
        }
        let Some((a, ask, txs)) = self.open.remove(&id) else { return Err(format!("{agent} has no read waiting as {id}")) };
        let path = PathBuf::from(&ask.path);
        let send = |txs: Vec<oneshot::Sender<Result<(), String>>>, r: Result<(), String>| txs.into_iter().filter(|t| !t.is_closed()).map(|t| t.send(r.clone()).is_ok()).fold(false, |x, y| x | y);
        let kept = match answer {
            ReadAnswer::Folder => {
                let f = ask.folder.clone().unwrap_or_default();
                send(txs, Ok(()));
                let inside: Vec<u64> = self.open.iter().filter(|(_, (b, k, _))| *b == a && Path::new(&k.path).starts_with(&f)).map(|(n, _)| *n).collect();
                for n in inside {
                    if let Some((_, _, t)) = self.open.remove(&n) {
                        send(t, Ok(()));
                    }
                }
                Some(f)
            }
            ReadAnswer::Once => {
                if !send(txs, Ok(())) {
                    self.once.insert((a, path));
                }
                None
            }
            ReadAnswer::Deny => {
                let at = ask.folder.clone().map(PathBuf::from).unwrap_or(path);
                send(txs, Err(format!("denied: the owner said no to reading {}. Do the work without it; do not try it another way.", at.display())));
                self.denied.insert((a, at));
                None
            }
        };
        Ok((ask, kept))
    }
}

/// FOLDER kept read-only for AGENT in the project file `$COORD/project` names, as `project::add_repo` writes it.
pub fn keep(at: &crate::files::Places, agent: &str, folder: &str) -> Result<String, String> {
    let change = crate::project::Change { done: format!("{agent} reads {folder} from now on"), there: "a read folder is added there", message: format!("{agent} reads {folder} read-only, allowed by the owner in its chat") };
    crate::project::edit_project_file(at, &change, |old, _| with_read(old, agent, folder))
}

/// TEXT, a project file, with FOLDER among `[read.AGENT] folders`: added through toml_edit, his other lines kept.
pub fn with_read(text: &str, agent: &str, folder: &str) -> Result<String, String> {
    use toml_edit::{Array, DocumentMut, Item, Table, value};
    let mut doc: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| format!("the project file does not read as TOML: {e}"))?;
    let root = doc.as_table_mut();
    if !root.contains_key("read") {
        let mut t = Table::new();
        t.set_implicit(true);
        root.insert("read", Item::Table(t));
    }
    let read = root.get_mut("read").and_then(Item::as_table_like_mut).ok_or("the project file's `read` is no table")?;
    if !read.contains_key(agent) {
        read.insert(agent, Item::Table(Table::new()));
    }
    let own = read.get_mut(agent).and_then(Item::as_table_like_mut).ok_or_else(|| format!("the project file's `read.{agent}` is no table"))?;
    if !own.contains_key("folders") {
        own.insert("folders", value(Array::new()));
    }
    let folders = own.get_mut("folders").and_then(Item::as_array_mut).ok_or_else(|| format!("the project file's `read.{agent}.folders` is no list"))?;
    if !folders.iter().any(|f| f.as_str() == Some(folder)) {
        folders.push(folder);
    }
    Ok(doc.to_string())
}

#[cfg(test)]
#[path = "read_ask_tests.rs"]
mod tests;
