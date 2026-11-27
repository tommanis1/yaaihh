//! The text-change kind's three steps (agent-base kinds/text-change.kind, paper-research N-865), which coordd answers
//! itself as the worker `coord`, a name no agent may take (`core::reserved`): a project's own text changed live, by an
//! agent or by the owner from his window, committed and loaded together, or nothing changed; and after the load, the
//! picture the agent that made it looks at.
//!
//! `check_text` places each file (`place`: only the project's text, or the flavour's look; never agent-base, coordd's
//! or the window's code, the record, a path with `..` or a link, nor anything `files::never_served` names), applies
//! the change to a copy of the project's text and reads the copy as a reload reads it (`Tickets::read_defs_in`, off
//! the lock), then holds what it read to the open tickets as a reload does (`Core::reload_checked`, under the lock,
//! which changes nothing).
//! `load_text`, one step under the lock, holds the files to what the check read, writes them, reads them again,
//! commits them in their repository only (coordd's own commit: hooks off, never a push) and takes the definitions with
//! the commit on the reload's line, as the owner's Reload takes them (`Core::reload_take`); on any refusal every file has its old bytes back, the index is as it was for its
//! paths, and no commit stays. A journal in the coord's folder carries a load across a stop: rolled back at the next
//! start when it had not committed, its line written then when it had (`text_recover_files`, `text_recover_line`).
//!
//! `still_text` (consult-lead N-412 p1203) starts a stand-in window, the window's own build beside coordd, with only
//! its own environment, connected to this coordd as his window is (`coordc bridge` on this coord), on the page the
//! change touches (`still_page`); for the look it waits for the look on its link, which coordd sends again once that
//! link is up (`text_still_look`), so the picture shows the change as his window took it, never a file read by
//! itself. The picture goes where coordd says, the coord's `stills` folder by ticket and step, never a path of the
//! request's. The agent's `look_at` then answers LooksRight, or Wrong, which the kind takes back by the change's own
//! commit (`way_back`).
//!
//! Only a ticket whose program is agent-base's text-change kind as its file has it is answered (docket-core3, N-874
//! condition 1), and no project's text may declare the names it stands on: so the decision and the read come before
//! every load. Who may open one and what is read first are the project's policy (`Act open`, the decision
//! `text-read`), and the look's the system's (`look-read`), never here.

use crate::core::Core;
use docket_machine::{Defs, Delivery, Text};
use serde_json::{Value, json};
use std::path::{Component, Path, PathBuf};

/// The worker coordd answers the text-change kind's steps as.
pub const COORD: &str = "coord";
pub const CHECK: &str = "check_text";
pub const LOAD: &str = "load_text";
pub const STILL: &str = "still_text";
/// The kind whose steps `coord` answers, as agent-base has it.
pub const KIND: &str = "text-change";
/// The names agent-base's text-change kind stands on: no project or library text declares them (N-874 condition 1).
const OWN: [(&str, &str); 9] = [
    ("kind", "text-change"),
    ("call", "check_text"),
    ("call", "load_text"),
    ("call", "read_text"),
    ("call", "still_text"),
    ("call", "look_at"),
    ("decision", "text-read"),
    ("decision", "look-read"),
    ("type", "TextFile"),
];
/// The flavour's look: its one file's name, and how long it may be (docket-edge6, N-865 post 7).
pub const LOOK: &str = "look.toml";
pub const LOOK_CAP: usize = 8192;
/// The journal of a load under way, in the coord's folder.
const JOURNAL: &str = "text-change.journal";
/// The stand-in window's program, beside coordd: the window's own build for this machine (consult-lead N-412 p1203).
const STAND_IN: &str = "window-stand-in";
/// In the coord's folder: the stand-in's own folder (its projects.toml, its ui.json, its HOME), and the stills, each
/// named by its ticket and step here, never by a request (docket-core3 reads this, N-412 p1203).
const STAND_IN_DIR: &str = "stand-in";
const STILLS: &str = "stills";

/// Where a change is read and written, taken under the lock: the coord's folder, agent-base, the project's folder,
/// its library when its file names one, the flavour's look file when the project is one of a flavour's
/// (`FLAVOUR/projects/NAME`), and the owner's home (for `never_served`).
#[derive(Clone, Debug)]
pub struct At {
    pub coord: String,
    pub base: String,
    pub project: String,
    pub library: Option<String>,
    pub look: Option<String>,
    pub home: String,
}

/// One file of a change as given: its path, and its whole text (none: removed).
#[derive(Clone, Debug, PartialEq)]
pub struct File {
    pub path: String,
    pub text: Option<String>,
}

/// A step due to `coord`, taken under the lock: what its call needs.
#[derive(Clone, Debug)]
pub struct Job {
    pub t: u64,
    pub step: u32,
    pub ticket: String,
    pub call: String,
    pub files: Vec<File>,
    pub back: Option<String>,
    /// the still's: the commit the load made
    pub commit: Option<String>,
    pub read: String,
    pub why: String,
    /// who opened the change: the check's answer and the reload's line name them
    pub by: String,
    /// None: the coord has no project, so no text of its own
    pub at: Option<At>,
    pub first: u64,
    /// why the ticket is not answered, when it is not agent-base's text-change as its file has it
    pub not_ours: Option<String>,
}

/// What the check found off the lock: refused already, the look checked, or the copy read as a reload reads it (its
/// definitions, what their reading refused, said of the real files), with the answer to give when the open tickets
/// let them.
pub enum Checked {
    Refused(Value),
    Look(Value),
    Read { read: Box<crate::ticket::DefsRead>, head: Value },
}

/// What a file touches, for the decisions `text-read` and `look-read`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Touch {
    Kinds,
    Calls,
    /// the project's prompt texts: renderings/NAME.tmpl, read at each render, and standing.md, every agent's standing
    /// text, read at its start (agent-base bin/roster `standing()`; paper-research N-1446 p2884)
    Renderings,
    Views,
    Policy,
    Library,
    Look,
}

/// A file of a change placed: where it is on the host, the root it is under (the project's folder, its library, or
/// the flavour's), its name from that root, what it touches, and its text to be.
#[derive(Clone, Debug)]
struct Placed {
    real: PathBuf,
    root: PathBuf,
    rel: PathBuf,
    touch: Touch,
    text: Option<String>,
}

fn refused(why: impl Into<String>, at: Vec<String>) -> Value {
    json!({"head": "Refused", "why": why.into(), "at": at})
}

fn unseen(why: impl Into<String>) -> Value {
    json!({"head": "Unseen", "why": why.into()})
}

fn canon(p: &str) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p))
}

/// The files of a step's FILES argument.
fn files_of(v: &Value) -> Vec<File> {
    v.as_array().into_iter().flatten().map(|f| File { path: f["path"].as_str().unwrap_or_default().to_string(), text: f["text"].as_str().map(String::from) }).collect()
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let o = std::process::Command::new("git").arg("-C").arg(dir).args(args).stdin(std::process::Stdio::null()).output().map_err(|e| format!("git: {e}"))?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).into_owned())
    } else {
        Err(format!("git {}: {}", args.first().copied().unwrap_or_default(), String::from_utf8_lossy(&o.stderr).trim()))
    }
}

/// The repository folder DIR is in, as git has it (also project.rs `add_repo`'s).
pub(crate) fn repository(dir: &Path) -> Option<PathBuf> {
    git(dir, &["rev-parse", "--show-toplevel"]).ok().map(|s| PathBuf::from(s.trim())).map(|p| std::fs::canonicalize(&p).unwrap_or(p))
}

/// What PATH is in the project's text or the flavour's look, or why it never changes this way.
fn place(at: &At, path: &str, text: Option<String>) -> Result<Placed, String> {
    if path.trim().is_empty() || path.contains('\0') {
        return Err(format!("`{path}` is no path"));
    }
    let p = Path::new(path);
    // as written, before anything is joined or followed
    if p.components().any(|c| matches!(c, Component::ParentDir | Component::CurDir)) {
        return Err(format!("{path}: a path with `..` or `.` in it never changes this way"));
    }
    let project = canon(&at.project);
    let library = at.library.as_deref().map(canon);
    let base = canon(&at.base);
    let coord = canon(&at.coord);
    // the flavour's look: `look.toml` as the name alone, or its path as the host has it
    if let Some(look) = at.look.as_deref().map(PathBuf::from) {
        let dir = look.parent().map(Path::to_path_buf).unwrap_or_default();
        let is_look = path == LOOK || (p.is_absolute() && p.file_name().is_some_and(|n| n == LOOK) && p.parent().is_some_and(|d| d == dir || canon(&d.display().to_string()) == dir));
        if is_look {
            if std::fs::symlink_metadata(&look).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(format!("{path}: {} is a link, and a link never changes this way", look.display()));
            }
            if let Some(t) = text.as_ref().filter(|t| t.len() > LOOK_CAP) {
                return Err(format!("{path}: the look is at most {} KB, and this one is {} bytes", LOOK_CAP / 1024, t.len()));
            }
            if text.is_none() && !look.exists() {
                return Err(format!("{path} is to be removed, and there is no such file"));
            }
            return Ok(Placed { real: look, root: dir, rel: PathBuf::from(LOOK), touch: Touch::Look, text });
        }
    }
    // an absolute path under a root as written (the project file's folder, the library's) is under it as the host
    // has it, links above the root followed; under no root, it is refused below
    let mut full = if p.is_absolute() { p.to_path_buf() } else { project.join(p) };
    for (given, real) in [(Some(at.project.as_str()), Some(&project)), (at.library.as_deref(), library.as_ref())] {
        if let (Some(g), Some(r)) = (given, real) {
            if let Ok(rest) = full.strip_prefix(g) {
                if !full.starts_with(r) {
                    full = r.join(rest);
                }
            }
        }
    }
    if full.starts_with(&base) {
        return Err(format!("{path} is agent-base's: the system's own text changes on the trains, never this way"));
    }
    // the coord's folder (the record, the registry, its keys), unless the project's folder is inside it
    if full.starts_with(&coord) && !full.starts_with(&project) {
        return Err(format!("{path} is in the coord's folder, the record's: it never changes this way"));
    }
    // a library inside the project's folder is the library's
    let (root, lib) = match &library {
        Some(l) if full.starts_with(l) => (l.clone(), true),
        _ if full.starts_with(&project) => (project.clone(), false),
        _ => return Err(format!("{path} is not in the project's text: only its folder {}{} changes this way", project.display(), library.as_ref().map_or(String::new(), |l| format!(" and its library {}", l.display())))),
    };
    let rel = full.strip_prefix(&root).unwrap().to_path_buf();
    let names: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    if names.is_empty() || names.iter().any(|n| n.starts_with('.') || !n.chars().all(|c| c.is_alphanumeric() || "-_.".contains(c))) {
        return Err(format!("{path}: a name of letters, digits, `-`, `_` and `.` not starting with `.` is wanted at each part"));
    }
    let ext = rel.extension().and_then(|e| e.to_str()).unwrap_or_default();
    let flat = |r: &Path| !r.join("kinds").is_dir() && !r.join("calls").is_dir();
    let n: Vec<&str> = names.iter().map(String::as_str).collect();
    let touch = match (lib, n.as_slice(), ext) {
        (true, ["kinds", _] | ["calls", _], "kind" | "call") => Touch::Library,
        (true, [_], "kind" | "call") if flat(&root) => Touch::Library,
        (false, ["kinds", _], "kind") => Touch::Kinds,
        (false, ["calls", _], "call") => Touch::Calls,
        (false, ["renderings", _], "tmpl") => Touch::Renderings,
        (false, [_], "kind") if flat(&root) => Touch::Kinds,
        (false, [_], "call") if flat(&root) => Touch::Calls,
        (false, ["policy", _] | ["policy", "agents" | "eflint", _], "eflint") => Touch::Policy,
        (false, ["project.toml"], _) => Touch::Views,
        (false, ["standing.md"], _) => Touch::Renderings,
        _ => {
            return Err(format!(
                "{path} is not the project's text: what changes this way is kinds/NAME.kind, calls/NAME.call, renderings/NAME.tmpl, policy/NAME.eflint (and policy/agents/, policy/eflint/), the views of project.toml, standing.md, the library's kinds and calls, and the flavour's look.toml"
            ));
        }
    };
    // no link anywhere on the way: written through, a link would put the text wherever it points (files.rs, p135)
    let mut at_part = root.clone();
    for c in rel.components() {
        at_part.push(c);
        if std::fs::symlink_metadata(&at_part).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(format!("{path}: {} is a link, and a link never changes this way", at_part.display()));
        }
    }
    if let Some(why) = crate::files::never_served(Path::new(&at.home), &full) {
        return Err(format!("{path} is never written ({why})"));
    }
    if std::fs::metadata(&full).is_ok_and(|m| m.is_dir()) {
        return Err(format!("{path} is a folder"));
    }
    if text.is_none() && !full.exists() {
        return Err(format!("{path} is to be removed, and there is no such file"));
    }
    Ok(Placed { real: full, root, rel, touch, text })
}

/// FILES placed, all of them in one repository that is not the system's own, the look alone or the project's text
/// alone: the files and that repository.
fn place_all(at: &At, files: &[File]) -> Result<(Vec<Placed>, PathBuf), (String, Vec<String>)> {
    if files.is_empty() {
        return Err(("the change names no file".into(), Vec::new()));
    }
    let mut placed: Vec<Placed> = Vec::new();
    let mut why = Vec::new();
    for f in files {
        match place(at, &f.path, f.text.clone()) {
            Ok(p) if placed.iter().any(|q| q.real == p.real) => why.push(format!("{} is named twice", f.path)),
            Ok(p) => placed.push(p),
            Err(e) => why.push(e),
        }
    }
    if !why.is_empty() {
        return Err((why.join("; "), Vec::new()));
    }
    if placed.iter().any(|p| p.touch == Touch::Look) && placed.iter().any(|p| p.touch != Touch::Look) {
        return Err(("the look and the project's text are two changes: open one for each".into(), Vec::new()));
    }
    let system = repository(&canon(&at.base));
    let mut repos: Vec<PathBuf> = Vec::new();
    for p in &placed {
        let folder = p.real.ancestors().find(|a| a.is_dir()).unwrap_or(&p.root).to_path_buf();
        match repository(&folder) {
            None => return Err((format!("{} is in no git repository: a change is committed as it is loaded, so it has nowhere to go", p.real.display()), Vec::new())),
            Some(r) if Some(&r) == system.as_ref() => return Err((format!("{} is in the system's own repository ({}): coordd, the window and agent-base change on the trains, never this way", p.real.display(), r.display()), Vec::new())),
            Some(r) if !repos.contains(&r) => repos.push(r),
            Some(_) => {}
        }
    }
    if repos.len() > 1 {
        return Err((format!("the files are in {} repositories ({}): one change is one commit, so change them apart", repos.len(), repos.iter().map(|r| r.display().to_string()).collect::<Vec<_>>().join(", ")), Vec::new()));
    }
    Ok((placed, repos.remove(0)))
}

/// What the files are now and are to be, as one word (FNV-1a over each path, whether it is there, its bytes, and its
/// text to be): what the load holds them to, so that it writes only what its check saw, over only what its check read
/// (docket-core3, N-874 condition 1).
fn read_of(placed: &[Placed]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut eat = |b: &[u8]| {
        for x in b {
            h ^= *x as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    let mut ps: Vec<&Placed> = placed.iter().collect();
    ps.sort_by(|a, b| a.real.cmp(&b.real));
    for p in ps {
        eat(p.real.as_os_str().as_encoded_bytes());
        match std::fs::read(&p.real) {
            Ok(b) => {
                eat(&[1]);
                eat(&(b.len() as u64).to_le_bytes());
                eat(&b);
            }
            Err(_) => eat(&[0]),
        }
        match &p.text {
            Some(t) => {
                eat(&[1]);
                eat(&(t.len() as u64).to_le_bytes());
                eat(t.as_bytes());
            }
            None => eat(&[0]),
        }
    }
    format!("{h:016x}")
}

/// The project file's change: only its `[[view]]` entries, and of those only the ones with no `source` or `older`
/// (a command coordd runs on the host, not text; docket-core3, N-874 condition 2), each template among them (`kind =
/// "lens"`) one the window takes (`coord_types::template`). The ids of the views it changes, or why not.
fn views_changed(old: &str, new: &str) -> Result<Vec<String>, String> {
    let parse = |t: &str, which: &str| toml::from_str::<toml::Value>(t).map_err(|e| format!("project.toml {which}: {e}"));
    let (mut o, mut n) = (parse(old, "as it is")?, parse(new, "as it would be")?);
    let take = |v: &mut toml::Value| v.as_table_mut().and_then(|t| t.remove("view")).and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let (ov, nv) = (take(&mut o), take(&mut n));
    if o != n {
        return Err("project.toml: only its [[view]] entries change this way; the rest of it (the project, its limits, servers, norms, library) is the owner's, by his own hand".into());
    }
    let id = |v: &toml::Value| v.get("id").and_then(|i| i.as_str()).map(String::from);
    let mut ids: Vec<String> = ov.iter().chain(&nv).filter_map(id).collect();
    ids.sort();
    ids.dedup();
    for v in &nv {
        if id(v).is_none() || v.get("kind").and_then(|k| k.as_str()).is_none() {
            return Err("project.toml: each [[view]] has an `id` and a `kind`".into());
        }
    }
    let mut changed = Vec::new();
    for i in ids {
        let (a, b) = (ov.iter().find(|v| id(v).as_deref() == Some(&i)), nv.iter().find(|v| id(v).as_deref() == Some(&i)));
        if a == b {
            continue;
        }
        if [a, b].iter().flatten().any(|v| v.get("source").is_some() || v.get("older").is_some()) {
            return Err(format!("project.toml: the view {i} has a source, a command coordd runs on the host, which is not text: a view with a command rides a train"));
        }
        // the window's own check (coord_types::template): a template it would not draw, or one that reaches past what a
        // template may (an agent's folder or session, more than 3 hops), is refused here, before anything is written,
        // at the line that names it (paper-research N-1446 p1789, 2)
        if let Some(v) = b.filter(|v| v.get("kind").and_then(|k| k.as_str()) == Some("lens")) {
            let json = serde_json::to_value(v).map_err(|e| format!("project.toml: the view {i} does not read: {e}"))?;
            coord_types::template::check(&json).map_err(|faults| format!("project.toml:{}: the view {i} is not a template the window takes: {}", view_line(new, &i), faults.join("; ")))?;
        }
        changed.push(i);
    }
    Ok(changed)
}

/// The line of project.toml's TEXT that names the view ID (`id = "ID"`), else its first.
fn view_line(text: &str, id: &str) -> usize {
    let named = format!("id=\"{id}\"");
    text.lines().position(|l| l.split('#').next().unwrap_or_default().replace([' ', '\t'], "") == named).map_or(1, |n| n + 1)
}

/// The paths each refusal names with a line, for the window to show at the line: `path:line`, or a kind's refusal as
/// docket_machine's check words it, `path: kind K refused: line N: …` (check.rs), as `path:N`.
fn places_in(refusals: &[String]) -> Vec<String> {
    let text_file = |p: &str| [".kind", ".call", ".eflint", ".tmpl", ".toml"].iter().any(|x| p.ends_with(x));
    let digits = |l: &str| !l.is_empty() && l.chars().all(|c| c.is_ascii_digit());
    let mut out = Vec::new();
    for r in refusals {
        let words: Vec<&str> = r.split_whitespace().collect();
        for (i, w) in words.iter().enumerate() {
            let w = w.trim_end_matches([':', ',', ';', '.', ')', '`']).trim_start_matches(['(', '`']);
            let place = match w.rsplit_once(':') {
                Some((p, l)) if digits(l) && text_file(p) => Some(w.to_string()),
                _ if text_file(w) => words[i + 1..].iter().take(6).collect::<Vec<_>>().windows(2).find(|x| *x[0] == "line" && digits(x[1].trim_end_matches(':'))).map(|x| format!("{w}:{}", x[1].trim_end_matches(':'))),
                _ => None,
            };
            if let Some(p) = place.filter(|p| !out.contains(p)) {
                out.push(p);
            }
        }
    }
    out
}

/// Each text file under FROM (the project's or library's text: its kinds, calls, renderings and policy folders, and
/// the files at its top) copied to TO, a link as the reload reads it (what it points to); no dot-name is copied.
fn copy_text(from: &Path, to: &Path) -> std::io::Result<()> {
    fn tree(from: &Path, to: &Path, depth: u32) -> std::io::Result<()> {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)?.flatten() {
            let name = e.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let path = e.path();
            if path.is_file() {
                std::fs::copy(&path, to.join(&name))?;
            } else if path.is_dir() && depth > 0 {
                tree(&path, &to.join(&name), depth - 1)?;
            }
        }
        Ok(())
    }
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() {
        let name = e.file_name();
        let s = name.to_string_lossy();
        let path = e.path();
        if path.is_file() && [".kind", ".call", ".toml", ".tmpl"].iter().any(|x| s.ends_with(x)) {
            std::fs::copy(&path, to.join(&name))?;
        } else if path.is_dir() && ["kinds", "calls", "renderings", "policy"].contains(&s.as_ref()) {
            tree(&path, &to.join(&name), 2)?;
        }
    }
    Ok(())
}

/// The files of BACK, a commit in REPO: each file it changed, as it was before it; refused when the commit is not
/// there, is the first, or a file of it changed since.
fn way_back(repo: &Path, back: &str) -> Result<Vec<File>, String> {
    let c = git(repo, &["rev-parse", "--verify", "--quiet", &format!("{back}^{{commit}}")]).map_err(|_| format!("there is no commit {back} in {}", repo.display()))?;
    let c = c.trim().to_string();
    git(repo, &["rev-parse", "--verify", "--quiet", &format!("{c}^")]).map_err(|_| format!("{back} is the repository's first commit: there is nothing before it to go back to"))?;
    let names = git(repo, &["diff-tree", "--no-commit-id", "--name-only", "-r", "-z", &format!("{c}^"), &c])?;
    let mut files = Vec::new();
    for name in names.split('\0').filter(|n| !n.is_empty()) {
        let path = repo.join(name);
        let then = git(repo, &["show", &format!("{c}:{name}")]).ok();
        let now = std::fs::read_to_string(&path).ok();
        if now != then {
            return Err(format!("{} changed since {back}: going back would undo more than that change", path.display()));
        }
        files.push(File { path: path.display().to_string(), text: git(repo, &["show", &format!("{c}^:{name}")]).ok() });
    }
    if files.is_empty() {
        return Err(format!("{back} changed no file"));
    }
    Ok(files)
}

/// The project's text, copied to a scratch folder with PLACED applied: the folders to read (agent-base, the library,
/// the project) and how to say a scratch path as the real one. Only the project's text and its library are copied,
/// never the coord's folder (N-874 remark 4). The scratch folder is in the coord's folder, named by the ticket and
/// step: the owner's, and no other coord's check meets it (a name from the process alone put two coords' copies of
/// one ticket and step in one folder).
struct Scratch {
    dir: PathBuf,
    dirs: Vec<String>,
    project: String,
    library: Option<String>,
    swaps: Vec<(String, String)>,
}

impl Scratch {
    fn make(at: &At, placed: &[Placed], tag: &str) -> Result<Scratch, String> {
        let dir = Path::new(&at.coord).join(format!("text-check-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        let project = canon(&at.project);
        let library = at.library.as_deref().map(canon);
        let (p, l) = (dir.join("project"), dir.join("library"));
        copy_text(&project, &p).map_err(|e| format!("the project's text could not be copied to check: {e}"))?;
        let mut dirs = vec![at.base.clone()];
        let mut swaps = vec![(p.display().to_string(), project.display().to_string())];
        if let Some(lib) = &library {
            copy_text(lib, &l).map_err(|e| format!("the library could not be copied to check: {e}"))?;
            dirs.push(l.display().to_string());
            swaps.insert(0, (l.display().to_string(), lib.display().to_string()));
        }
        dirs.push(p.display().to_string());
        for f in placed {
            let to = if Some(&f.root) == library.as_ref() { l.join(&f.rel) } else { p.join(&f.rel) };
            match &f.text {
                Some(t) => {
                    if let Some(d) = to.parent() {
                        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", f.rel.display()))?;
                    }
                    std::fs::write(&to, t).map_err(|e| format!("{}: {e}", f.rel.display()))?;
                }
                None => {
                    let _ = std::fs::remove_file(&to);
                }
            }
        }
        Ok(Scratch { dir, dirs, project: p.display().to_string(), library: library.map(|_| l.display().to_string()), swaps })
    }

    fn real(&self, s: &str) -> String {
        self.swaps.iter().fold(s.to_string(), |s, (a, b)| s.replace(a.as_str(), b))
    }

    /// Where the copy's text declares a name agent-base's text-change stands on (`OWN`): `file:line: kind X`, each.
    fn declares_own(&self) -> Vec<String> {
        let mut out = Vec::new();
        let roots: Vec<PathBuf> = std::iter::once(PathBuf::from(&self.project)).chain(self.library.iter().map(PathBuf::from)).collect();
        for root in roots {
            let mut files: Vec<PathBuf> = Vec::new();
            for sub in ["", "kinds", "calls"] {
                if let Ok(rd) = std::fs::read_dir(root.join(sub)) {
                    files.extend(rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "kind" || x == "call")));
                }
            }
            files.sort();
            for f in files {
                let Ok(text) = std::fs::read_to_string(&f) else { continue };
                for (i, line) in text.lines().enumerate() {
                    let mut w = line.split(|c: char| c.is_whitespace() || c == '(' || c == '{');
                    let (Some(word), Some(name)) = (w.next(), w.next()) else { continue };
                    if OWN.iter().any(|(k, n)| *k == word && *n == name) {
                        out.push(format!("{}:{}: {word} {name} is agent-base's text-change's own, and no project's text declares it", self.real(&f.display().to_string()), i + 1));
                    }
                }
            }
        }
        out
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// What each placed file touches, by name: the lists the decision `text-read` is given; the project file's views
/// changed, or why the project file may not change so.
fn touched(placed: &[Placed]) -> Result<Value, String> {
    let of = |t: Touch| -> Vec<String> { placed.iter().filter(|p| p.touch == t).map(|p| p.rel.display().to_string()).collect() };
    let mut views = Vec::new();
    for p in placed.iter().filter(|p| p.touch == Touch::Views) {
        let old = std::fs::read_to_string(&p.real).unwrap_or_default();
        let new = p.text.clone().ok_or("project.toml is never removed this way")?;
        // a template's fault names its line in the real file, as a kind's does (`places_in`)
        let real = |e: String| {
            let at = e.strip_prefix("project.toml:").filter(|r| r.starts_with(|c: char| c.is_ascii_digit())).map(|rest| format!("{}:{rest}", p.real.display()));
            at.unwrap_or(e)
        };
        views.extend(views_changed(&old, &new).map_err(real)?);
    }
    Ok(json!({"kinds": of(Touch::Kinds), "calls": of(Touch::Calls), "renderings": of(Touch::Renderings), "views": views, "policy": of(Touch::Policy), "library": of(Touch::Library)}))
}

/// The kinds the touched kind files declare under a name agent-base has a kind of (consult-2, N-875 point 3).
fn shadows(at: &At, scratch: &Scratch, defs: &Defs, placed: &[Placed]) -> Vec<String> {
    let base: Vec<String> = std::fs::read_dir(Path::new(&at.base).join("kinds"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .flat_map(|t| t.lines().filter_map(|l| l.strip_prefix("kind ").map(|r| r.split_whitespace().next().unwrap_or_default().to_string())).collect::<Vec<_>>())
        .collect();
    let touched: Vec<String> = placed.iter().filter(|p| matches!(p.touch, Touch::Kinds | Touch::Library)).map(|p| p.real.display().to_string()).collect();
    let base_dir = canon(&at.base);
    let mut out: Vec<String> = defs
        .kinds
        .values()
        .filter(|k| base.contains(&k.name) && !canon(&k.file).starts_with(&base_dir) && touched.contains(&scratch.real(&k.file)))
        .map(|k| k.name.clone())
        .collect();
    out.sort();
    out
}

/// A kind's own handlers of the owner's events (`when owner_...`), as its text has them, each section's lines trimmed:
/// what is compared, so that a line moved above them changes nothing.
fn owner_handlers(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut open: Option<(usize, String)> = None;
    for line in text.lines() {
        let indent = line.len() - line.trim_start().len();
        if let Some((at, body)) = open.as_mut() {
            if line.trim().is_empty() || indent > *at {
                body.push_str(line.trim());
                body.push('\n');
                continue;
            }
            out.push(std::mem::take(body));
            open = None;
        }
        if line.trim_start().strip_prefix("when ").is_some_and(|r| r.trim_start().starts_with("owner")) {
            open = Some((indent, format!("{}\n", line.trim())));
        }
    }
    out.extend(open.map(|(_, b)| b));
    out
}

/// The bindings the change adds, alters or takes away, and the kinds whose own handlers of the owner's events it
/// changes: DEFS as the copy read them against OLD, the definitions now (consult-2, N-875 point 3).
fn bindings_and_handlers(old: &Defs, defs: &Defs) -> (Vec<String>, Vec<String>) {
    let mut bindings: Vec<String> = defs.bindings.iter().filter(|b| !old.bindings.iter().any(|o| o.text == b.text)).map(|b| b.text.clone()).collect();
    bindings.extend(old.bindings.iter().filter(|o| !defs.bindings.iter().any(|b| b.text == o.text)).map(|o| format!("gone: {}", o.text)));
    let owners = |d: &Defs, name: &str| d.kinds.get(name).map(|k| owner_handlers(&k.text)).unwrap_or_default();
    let mut names: Vec<&String> = defs.kinds.keys().chain(old.kinds.keys()).collect();
    names.sort();
    names.dedup();
    let handlers = names.into_iter().filter(|n| owners(defs, n.as_str()) != owners(old, n.as_str())).cloned().collect();
    (bindings, handlers)
}

/// A load's journal (`JOURNAL`): what it was about to write, and the repository's head before it.
#[derive(serde::Serialize, serde::Deserialize)]
struct Journal {
    ticket: String,
    t: u64,
    repo: String,
    head: String,
    /// each file's path and its text before the load (none: not there)
    files: Vec<(String, Option<String>)>,
    made: Vec<String>,
    by: String,
    why: String,
}

/// JOURNAL written to PATH, readable by the owner only, and on the disk before the first file of the load is written
/// (docket-core3, N-874 post 4).
fn write_journal(path: &Path, journal: &Journal) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
    f.write_all(serde_json::to_string(journal).unwrap_or_default().as_bytes())?;
    f.sync_all()?;
    if let Some(d) = path.parent() {
        std::fs::File::open(d)?.sync_all()?;
    }
    Ok(())
}

/// Tests failing first (N-865 item 6): with TEXT_CHANGE_FAILING_FIRST set, a test's check and load answer as if they
/// were not built, so that each test is seen to fail without them.
#[cfg(test)]
fn not_built() -> Option<Value> {
    std::env::var_os("TEXT_CHANGE_FAILING_FIRST").map(|_| refused("not built", Vec::new()))
}

#[cfg(not(test))]
fn not_built() -> Option<Value> {
    None
}

/// The check, off the lock (blocking: files copied and read, git asked): refused, the look checked, or the copy's
/// definitions read.
pub fn check(job: &Job) -> Checked {
    if let Some(v) = not_built() {
        return Checked::Refused(v);
    }
    if let Some(why) = &job.not_ours {
        return Checked::Refused(refused(why.clone(), Vec::new()));
    }
    let Some(at) = &job.at else { return Checked::Refused(refused("this coord has no project: the only text it reads is agent-base's, which changes on the trains", Vec::new())) };
    let files = match (&job.back, job.files.is_empty()) {
        (Some(_), false) => return Checked::Refused(refused("a way back names its commit (back) and no files", Vec::new())),
        (Some(b), true) => {
            // the commit is the project's repository's, or the look's
            let dirs = [Some(canon(&at.project)), at.look.as_deref().and_then(|l| Path::new(l).parent().map(Path::to_path_buf))];
            let repos: Vec<PathBuf> = dirs.into_iter().flatten().filter_map(|d| repository(&d)).collect();
            let tried: Vec<Result<Vec<File>, String>> = repos.iter().map(|r| way_back(r, b)).collect();
            let found = tried.iter().find_map(|r| r.as_ref().ok()).cloned();
            match (found, tried.into_iter().next()) {
                (Some(f), _) => f,
                (None, Some(Err(e))) => return Checked::Refused(refused(e, Vec::new())),
                _ => return Checked::Refused(refused("the project's folder is in no git repository: there is no commit to go back from", Vec::new())),
            }
        }
        (None, _) => job.files.clone(),
    };
    let (placed, _) = match place_all(at, &files) {
        Ok(x) => x,
        Err((why, places)) => return Checked::Refused(refused(why, places)),
    };
    let read = read_of(&placed);
    let files: Vec<Value> = placed.iter().map(|p| json!({"path": p.real.display().to_string(), "text": p.text})).collect();
    if placed.iter().all(|p| p.touch == Touch::Look) {
        return match look_checks(at, &placed) {
            Ok(()) => Checked::Look(json!({"head": "Look", "files": files, "by": job.by, "read": read})),
            Err((why, places)) => Checked::Refused(refused(why, places)),
        };
    }
    let touches = match touched(&placed) {
        Ok(t) => t,
        Err(e) => return Checked::Refused(refused(e.clone(), places_in(&[e]))),
    };
    // the look's decision is the system's: a project's policy file that rules on it is refused here, at its own
    // change, rather than stopping every look change after it (consult-2, N-875 post 4)
    let look_rulers: Vec<String> = placed.iter().filter(|p| p.touch == Touch::Policy && (p.rel.file_name().is_some_and(|n| n == "look.eflint") || p.text.as_ref().is_some_and(|t| t.contains("look-read")))).map(|p| p.rel.display().to_string()).collect();
    if !look_rulers.is_empty() {
        return Checked::Refused(refused(format!("{}: `look-read`, the look's decision, is the system's, and a project's policy cannot rule on it or stand in its file's place", look_rulers.join(", ")), Vec::new()));
    }
    let scratch = match Scratch::make(at, &placed, &format!("{}-{}", job.t, job.step)) {
        Ok(s) => s,
        Err(e) => return Checked::Refused(refused(e, Vec::new())),
    };
    let own = scratch.declares_own();
    if !own.is_empty() {
        return Checked::Refused(refused(format!("nothing changes: {}", own.join("; ")), places_in(&own)));
    }
    // the copy's policy and duties are the project's in it: PLACES' project is the copy
    let places = crate::policy::Places { base: at.base.clone(), project: Some(scratch.project.clone()), coord: at.coord.clone(), ..Default::default() };
    let (defs, errors, duties, call_files) = crate::ticket::Tickets::read_defs_in(&places, &scratch.dirs, job.first);
    let errors: Vec<String> = errors.iter().map(|e| scratch.real(e)).collect();
    let mut head = json!({"head": "Checks", "files": files, "by": job.by, "read": read, "shadows": shadows(at, &scratch, &defs, &placed), "bindings": [], "handlers": []});
    for k in ["kinds", "calls", "renderings", "views", "policy", "library"] {
        head[k] = touches[k].clone();
    }
    Checked::Read { read: Box::new((defs, errors, duties, call_files)), head }
}

/// The look as it would be: TOML of at most `LOOK_CAP` bytes that the window takes (`coord_types::look::check`, the
/// check the window and its editor run, look.rs `read`), and no project's policy that rules on `look-read`, the
/// system's alone (consult-2, N-875 point 4).
fn look_checks(at: &At, placed: &[Placed]) -> Result<(), (String, Vec<String>)> {
    // each by its name in the project's folder for him to read, its path for the window to open
    let (mut names, mut rulers) = (Vec::new(), Vec::new());
    for sub in ["policy", "policy/agents", "policy/eflint"] {
        for e in std::fs::read_dir(Path::new(&at.project).join(sub)).into_iter().flatten().flatten() {
            // a file that names the decision, or one in the place of agent-base's look.eflint (which would leave the
            // decision with no rule, and so the owner's to answer)
            if e.path().extension().is_some_and(|x| x == "eflint") && (e.file_name() == "look.eflint" || std::fs::read_to_string(e.path()).is_ok_and(|t| t.contains("look-read"))) {
                names.push(format!("{sub}/{}", e.file_name().to_string_lossy()));
                rulers.push(e.path().display().to_string());
            }
        }
    }
    if !rulers.is_empty() {
        return Err((format!("the look's rule (`look-read`) is the system's, and the project's policy names it or stands in its file's place ({}): no look changes while it does", names.join(", ")), rulers));
    }
    for p in placed {
        if let Some(t) = &p.text {
            let v = toml::from_str::<Value>(t).map_err(|e| (format!("{} does not read: {e}", p.real.display()), Vec::new()))?;
            // the window's own check (coord_types::look, here-2's head A): what the window would not take is refused
            // here, in its words, before anything is written (docket-core3, N-412 p1119, 3)
            coord_types::look::check(&v).map_err(|faults| (format!("{} is not a look the window takes: {}", p.real.display(), faults.join("; ")), Vec::new()))?;
        }
    }
    Ok(())
}

/// The old bytes of each file a load writes, to put back.
struct Undo {
    files: Vec<(PathBuf, Option<Vec<u8>>)>,
    made: Vec<PathBuf>,
}

impl Undo {
    fn back(&self) {
        for (p, old) in &self.files {
            match old {
                Some(b) => {
                    let _ = std::fs::write(p, b);
                }
                None => {
                    let _ = std::fs::remove_file(p);
                }
            }
        }
        for d in self.made.iter().rev() {
            let _ = std::fs::remove_dir(d);
        }
    }
}

/// The folders PLACED's writing would make, outermost first.
fn to_make(placed: &[Placed]) -> Vec<PathBuf> {
    let mut made: Vec<PathBuf> = Vec::new();
    for p in placed.iter().filter(|p| p.text.is_some()) {
        let mut missing: Vec<PathBuf> = p.real.ancestors().skip(1).take_while(|a| !a.exists()).map(Path::to_path_buf).collect();
        missing.reverse();
        for m in missing {
            if !made.contains(&m) {
                made.push(m);
            }
        }
    }
    made
}

fn write_all(placed: &[Placed]) -> Result<Undo, (Undo, String)> {
    let mut undo = Undo { files: Vec::new(), made: to_make(placed) };
    for (i, p) in placed.iter().enumerate() {
        if i == 1 && stops_at("half") {
            break;
        }
        undo.files.push((p.real.clone(), std::fs::read(&p.real).ok()));
        let r = match &p.text {
            Some(t) => p.real.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| write_beside(&p.real, t)),
            None => std::fs::remove_file(&p.real),
        };
        if let Err(e) = r {
            return Err((undo, format!("{}: {e}", p.real.display())));
        }
    }
    Ok(undo)
}

/// TEXT written to a new file beside PATH and renamed over it: a reader sees the old file or the new one whole, and a
/// link put at PATH since the check is replaced, never written through (docket-core3, N-412 p1119).
fn write_beside(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.text-change"));
    let _ = std::fs::remove_file(&tmp);
    // create_new: a link planted at the temporary name is not followed either
    let r = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp).and_then(|mut f| f.write_all(text.as_bytes()).and_then(|_| f.sync_all())).and_then(|_| std::fs::rename(&tmp, path));
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

/// git in REPO as coordd's own act: no hook runs, nothing is signed, and the author is `coord`.
fn git_own(repo: &Path, args: &[&str]) -> Result<String, String> {
    let o = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false"])
        .args(args)
        .env("GIT_AUTHOR_NAME", "coord")
        .env("GIT_AUTHOR_EMAIL", "coord@localhost")
        .env("GIT_COMMITTER_NAME", "coord")
        .env("GIT_COMMITTER_EMAIL", "coord@localhost")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if o.status.success() { Ok(String::from_utf8_lossy(&o.stdout).into_owned()) } else { Err(String::from_utf8_lossy(&o.stderr).trim().to_string()) }
}

/// PATHS (from REPO) back in the index as HEAD has them; never the whole index (docket-core3, N-874 remark 2).
fn unstage(repo: &Path, paths: &[String]) {
    let mut r = vec!["reset", "-q", "--"];
    r.extend(paths.iter().map(String::as_str));
    let _ = git_own(repo, &r);
}

fn rel_paths(repo: &Path, placed: &[Placed]) -> Vec<String> {
    placed.iter().map(|p| p.real.strip_prefix(repo).unwrap_or(&p.real).display().to_string()).collect()
}

/// PLACED committed in REPO with MESSAGE, only those paths (`--only`: what another has staged stays staged and out of
/// it): the commit, or why not, their index entries as they were.
fn commit(repo: &Path, placed: &[Placed], message: &str) -> Result<String, String> {
    let paths = rel_paths(repo, placed);
    let mut add = vec!["add", "-A", "--"];
    add.extend(paths.iter().map(String::as_str));
    if let Err(e) = git_own(repo, &add) {
        unstage(repo, &paths);
        return Err(format!("not committed (git add: {e})"));
    }
    let mut c = vec!["commit", "-q", "--only", "--no-verify", "-m", message, "--"];
    c.extend(paths.iter().map(String::as_str));
    if let Err(e) = git_own(repo, &c) {
        unstage(repo, &paths);
        return Err(format!("not committed (git commit: {e})"));
    }
    git_own(repo, &["rev-parse", "HEAD"]).map(|h| h.trim().to_string()).map_err(|e| format!("committed, its hash unread: {e}"))
}

/// Whether REPO's index is held by another git now: then a load refuses before it writes (N-874 remark 3).
fn index_held(repo: &Path) -> bool {
    let Ok(p) = git(repo, &["rev-parse", "--git-path", "index.lock"]) else { return false };
    let p = PathBuf::from(p.trim());
    (if p.is_absolute() { p } else { repo.join(p) }).exists()
}

#[cfg(test)]
thread_local! {
    /// A test's stop of coordd in the middle of a load: after the first of its files is written (`half`), after all
    /// of them (`written`), or after its commit (`committed`).
    pub static STOP_AT: std::cell::Cell<Option<&'static str>> = const { std::cell::Cell::new(None) };
    /// A test's stand-in window program, in the place of the one beside coordd.
    pub static STAND_IN_AT: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// The stand-in window's program beside coordd, when one is built there.
#[cfg(not(test))]
fn stand_in_bin() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(|d| d.join(STAND_IN)).filter(|p| p.is_file())
}

#[cfg(test)]
fn stand_in_bin() -> Option<PathBuf> {
    STAND_IN_AT.with(|s| s.borrow().clone())
}

/// A still to draw (consult-lead N-412 p1203): the stand-in window's program and arguments, the whole of its
/// environment, where its picture goes, the page in words, the agent that looks at it, and whether the stand-in waits
/// for the look on its link.
pub struct Stand {
    pub bin: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub path: PathBuf,
    pub page: String,
    pub by: String,
    pub look: bool,
}

/// The page a change is drawn on ("the page the change touches"): for the look (LOOK), the agent page of BY, where its
/// colours, its words and the context circle's mark are drawn; for the project file's views (VIEWS, the ones it
/// changed that stand after it), the first one's own page (paper-research, N-1446 p1787 2), or the project's first
/// view when it only took views away; else the Kinds page with KINDS, the kinds the change wrote, open. The scene, the
/// page in words, and the scene's settings.
fn still_page(look: bool, views: Option<&[String]>, by: &str, kinds: &[String]) -> (&'static str, String, Vec<(String, String)>) {
    match (look, views, kinds) {
        (true, ..) => ("agent", format!("{by}'s agent page"), vec![("YAAIHH_AGENT".into(), by.into()), ("YAAIHH_STILL_AFTER".into(), "look".into())]),
        (_, Some([id, ..]), _) => ("overview", format!("the view {id}"), vec![("YAAIHH_VIEW".into(), id.clone())]),
        (_, Some([]), _) => ("overview", "the overview, the project's first view".into(), Vec::new()),
        (.., []) => ("kinds", "the Kinds page".into(), Vec::new()),
        _ => ("kinds", format!("the Kinds page, {} open", kinds.join(", ")), vec![("YAAIHH_DOCKET".into(), kinds.iter().map(|k| format!("kind:{k}")).collect::<Vec<_>>().join(","))]),
    }
}

/// The views the change at COMMIT changed in AT's project file (`views_changed` of the file before it and at it) that
/// stand after it, in the file's order: a view it took away has no page to draw.
fn views_standing(at: &At, commit: Option<&str>) -> Vec<String> {
    let Some(c) = commit else { return Vec::new() };
    let file = |rev: String| git(Path::new(&at.project), &["show", &format!("{rev}:./project.toml")]).unwrap_or_default();
    let now = file(c.to_string());
    let changed = views_changed(&file(format!("{c}^")), &now).unwrap_or_default();
    let standing = toml::from_str::<toml::Value>(&now).ok().and_then(|v| v.get("view")?.as_array().cloned()).unwrap_or_default();
    standing.iter().filter_map(|v| v.get("id")?.as_str().map(String::from)).filter(|i| changed.contains(i)).collect()
}

/// The stand-in run, blocking: started with STAND's environment and nothing else; WATCHING run each time it says its
/// link is up (`coord_types::STILL_WATCHING`) when it waits for the look; waited for as long as it lives. `Still` with
/// the picture's path when it ended well and wrote it, else `Unseen` with how it ended and its last words.
pub fn draw(stand: &Stand, mut watching: impl FnMut()) -> Value {
    use std::io::{BufRead, Read};
    let _ = std::fs::remove_file(&stand.path);
    let spawned = std::process::Command::new(&stand.bin)
        .args(&stand.args)
        .env_clear()
        .envs(stand.env.iter().cloned())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => return unseen(format!("the stand-in window {} did not start: {e}", stand.bin.display())),
    };
    // its words read beside its output, so that neither pipe fills and holds it
    let words = child.stderr.take().map(|mut e| {
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = e.read_to_string(&mut s);
            s
        })
    });
    if let Some(out) = child.stdout.take() {
        for l in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            if stand.look && l == coord_types::STILL_WATCHING {
                watching();
            }
        }
    }
    let ended = child.wait();
    let said = words.and_then(|h| h.join().ok()).unwrap_or_default();
    let lines: Vec<&str> = said.lines().filter(|l| !l.trim().is_empty()).collect();
    let last = lines[lines.len().saturating_sub(3)..].join(" / ");
    match ended {
        Ok(s) if s.success() && stand.path.is_file() => json!({"head": "Still", "path": stand.path.display().to_string(), "by": stand.by, "page": stand.page}),
        Ok(s) => unseen(format!("the stand-in window ended ({s}) and drew nothing{}", if last.is_empty() { String::new() } else { format!(": {last}") })),
        Err(e) => unseen(format!("the stand-in window: {e}")),
    }
}

fn stops_at(_at: &str) -> bool {
    #[cfg(test)]
    if STOP_AT.with(|s| s.get()) == Some(_at) {
        return true;
    }
    false
}

impl Core {
    /// Where this coord's change is read and written; None when it has no project.
    pub fn text_at(&self) -> Option<At> {
        let project = self.project.as_ref().map(|p| p.dir.clone()).filter(|d| !d.is_empty())?;
        let folders = crate::project::folders(&self.coord, &self.base, Some(project.as_str()));
        // the flavour: the folder of the `projects` folder the project is in
        let real = canon(&project);
        let look = real.parent().filter(|p| p.file_name().is_some_and(|n| n == "projects")).and_then(Path::parent).map(|f| f.join(LOOK).display().to_string());
        Some(At { coord: self.coord.clone(), base: self.base.clone(), library: (folders.len() == 3).then(|| folders[1].clone()), project, look, home: std::env::var("HOME").unwrap_or_default() })
    }

    /// The flavour's look read into memory (at start, and by a text-change's load): what the window is sent.
    pub fn read_look(&mut self) {
        let Some(path) = self.text_at().and_then(|a| a.look) else { return };
        if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink() || m.len() > LOOK_CAP as u64) {
            eprintln!("coordd: the look {path} is a link or over {} KB: not read", LOOK_CAP / 1024);
            self.look = None;
            return;
        }
        self.look = std::fs::read_to_string(&path).ok().and_then(|t| toml::from_str::<Value>(&t).map_err(|e| eprintln!("coordd: the look {path}: {e}")).ok());
        // the window has it as `Snapshot.look` (`snapshot_with`), and as `Body::Look` once a change loads it
    }

    /// Why ticket T is not one `coord` answers: none when its kind is agent-base's text-change, read from agent-base,
    /// and its program that kind's text as it is (docket-core3, N-874 condition 1).
    fn not_ours(&self, t: u64) -> Option<String> {
        let x = self.tickets.m.tickets.get(&t)?;
        let base = canon(&self.base);
        match self.tickets.m.defs.kinds.get(KIND) {
            _ if x.kind != KIND => Some(format!("coord answers only agent-base's {KIND} tickets, and {} is a {}", self.tickets.m.id(t), x.kind)),
            None => Some(format!("there is no kind {KIND} now")),
            Some(k) if !canon(&k.file).starts_with(&base) => Some(format!("the kind {KIND} is read from {}, not agent-base: coord answers only agent-base's", k.file)),
            Some(k) if k.text != x.program => Some(format!("{}'s program is not agent-base's {KIND} as its file has it now (edited, or opened before the file changed): coord answers only that program", self.tickets.m.id(t))),
            Some(_) => None,
        }
    }

    /// Hand what is due to `coord` (the text-change kind's steps) to its job, which answers them (`Job::Coord`).
    pub fn hand_coord(&mut self) {
        let Some(tx) = self.jobs.clone() else { return };
        let due = self.ticket_due(COORD);
        if due.is_empty() {
            return;
        }
        let handed: Vec<Delivery> = self.ticket_handing(COORD, due).into_iter().map(|(d, _)| d).collect();
        let jobs: Vec<Job> = handed.iter().filter_map(|d| self.text_job(d)).collect();
        if !jobs.is_empty() {
            let _ = tx.send(crate::core::Job::Coord { jobs });
        }
    }

    /// Delivery D to `coord`, given: for a check or a load, what it needs; None for anything else (a notice).
    pub fn text_job(&mut self, d: &Delivery) -> Option<Job> {
        self.ticket_given(COORD, d);
        let docket_machine::What::Step { step, call, args, .. } = &d.what else { return None };
        self.text_job_of(d.t, *step, call, args)
    }

    fn text_job_of(&mut self, t: u64, step: u32, call: &str, args: &Value) -> Option<Job> {
        let args = crate::render::resolved(args);
        let x = self.tickets.m.tickets.get(&t)?;
        let by = self.text_by(t);
        let why = args["why"].as_str().map(String::from).or_else(|| x.form.get("why").and_then(Value::as_str).map(String::from)).unwrap_or_default();
        let job = Job {
            t,
            step,
            ticket: self.tickets.m.id(t),
            call: call.to_string(),
            files: files_of(&args["files"]),
            back: args["back"].as_str().filter(|b| !b.is_empty()).map(String::from),
            commit: args["commit"].as_str().map(String::from),
            read: args["read"].as_str().unwrap_or_default().to_string(),
            why,
            by,
            at: self.text_at(),
            first: self.tickets.m.defs.first,
            not_ours: self.not_ours(t),
        };
        if ![CHECK, LOAD, STILL].contains(&call) {
            self.text_answer(&job, refused(format!("coord answers only {CHECK}, {LOAD} and {STILL}, not {call}"), Vec::new()));
            return None;
        }
        Some(job)
    }

    /// Who opened ticket T, for the decision: its opener, or for a ticket a program opened the nearest opener above it
    /// (as `ticket_facts` gives `opener(ticket, agent)`; docket-core7, N-919); a ticket nobody opened (a binding's) is
    /// `nobody`'s, never the owner's, so that only the owner's own open is his.
    fn text_by(&self, t: u64) -> String {
        let m = &self.tickets.m;
        let mut at = Some(t);
        for _ in 0..=m.tickets.len() {
            let Some(x) = at.and_then(|u| m.tickets.get(&u)) else { break };
            if let Some(o) = &x.opener {
                return o.clone();
            }
            at = x.parent.or(x.under);
        }
        "nobody".into()
    }

    /// The `coord` steps given before this start and not answered (the machine gives a step once): their jobs, to run
    /// again; a check reads again, a load finds its journal's end (`text_recover_line`) or runs again.
    pub fn text_owed(&mut self) -> Vec<Job> {
        let due: Vec<u64> = self.ticket_due(COORD).iter().map(|d| d.t).collect();
        let owed: Vec<(u64, u32, String, Value)> = self
            .tickets
            .m
            .tickets
            .values()
            .filter(|x| !due.contains(&x.t))
            .filter_map(|x| match &x.status {
                docket_machine::Status::Awaits { agent, owed, .. } if agent == COORD => Some((x.t, owed.step, owed.call.clone(), owed.args.clone())),
                _ => None,
            })
            .collect();
        owed.into_iter().filter_map(|(t, step, call, args)| self.text_job_of(t, step, &call, &args)).collect()
    }

    /// [`Core::text_owed`] sent to the job, at start.
    pub fn text_owed_again(&mut self) {
        let jobs = self.text_owed();
        if let (false, Some(tx)) = (jobs.is_empty(), self.jobs.clone()) {
            eprintln!("coordd: tickets: {} text-change steps given before this start, run again", jobs.len());
            let _ = tx.send(crate::core::Job::Coord { jobs });
        }
    }

    pub(crate) fn text_answer(&mut self, job: &Job, head: Value) {
        match self.ticket_answer(COORD, job.t, head, Text::New(String::new())) {
            Ok(r) => eprintln!("coordd: tickets: coord {} on {}: {r}", job.call, job.ticket),
            Err(e) => eprintln!("coordd: tickets: coord {} on {}: {e}", job.call, job.ticket),
        }
    }

    /// The check's answer to JOB, under the lock: what the copy read held to the open tickets as a reload holds it
    /// (`reload_checked`, which changes nothing); the bindings and the owner's handlers it changes read against the
    /// definitions now.
    pub fn text_checked(&mut self, job: &Job, checked: Checked) {
        let head = match checked {
            Checked::Refused(v) | Checked::Look(v) => v,
            Checked::Read { read, mut head } => {
                let (bindings, handlers) = bindings_and_handlers(&self.tickets.m.defs, &read.0);
                head["bindings"] = json!(bindings);
                head["handlers"] = json!(handlers);
                match self.reload_checked(*read) {
                    Ok(_) => head,
                    Err(r) => refused(format!("nothing changes: {}", r.join("; ")), places_in(&r)),
                }
            }
        };
        self.text_answer(job, head);
    }

    /// The load, one step under the lock: the files held to what the check read, written, read again, committed and
    /// taken, or every one as it was and no commit.
    pub fn text_load(&mut self, job: &Job) {
        if let Some(head) = crate::counts::timed_sync("text-change load", || self.text_load_head(job)) {
            self.text_answer(job, head);
        }
    }

    /// The still step, under the lock (consult-lead N-412 p1203): `Own` for the owner's own change, which his window
    /// draws; `Unseen` when no agent opened it or no stand-in window is built beside coordd; else the stand-in to start:
    /// the window's own build, connected to this coordd as his window is (`coordc bridge` on this coord,
    /// `YAAIHH_LOCAL_PIPE`), drawing the page the change touches into the coord's `stills` folder, at a name of the
    /// ticket and step alone. It reads nothing of the owner's: its environment is only what is set here, its HOME and
    /// its config its own folder in the coord's.
    pub fn text_still(&self, job: &Job) -> Result<Stand, Value> {
        if let Some(why) = &job.not_ours {
            return Err(unseen(why.clone()));
        }
        if job.by == "owner" {
            return Err(json!({"head": "Own"}));
        }
        if !self.agents.contains_key(&job.by) {
            return Err(unseen(format!("{} is no agent, so nobody looks at a picture of the change", job.by)));
        }
        let Some(at) = &job.at else { return Err(unseen("this coord has no project")) };
        let Some(bin) = stand_in_bin() else {
            return Err(unseen(format!("no stand-in window ({STAND_IN}) is built beside coordd, so nothing drew the change")));
        };
        // what the change touched, from its files as the check placed them (a file it removed is gone now)
        let paths: Vec<PathBuf> = job.files.iter().map(|f| canon(&f.path)).collect();
        let look = at.look.as_deref().map(canon).is_some_and(|l| !paths.is_empty() && paths.iter().all(|p| *p == l));
        let views = paths.contains(&canon(&at.project).join("project.toml")).then(|| views_standing(at, job.commit.as_deref()));
        let kinds: Vec<String> = self.tickets.m.defs.kinds.iter().filter(|(_, k)| paths.contains(&canon(&k.file))).map(|(n, _)| n.clone()).collect();
        let (scene, page, set) = still_page(look, views.as_deref(), &job.by, &kinds);
        let (own, stills) = (Path::new(&at.coord).join(STAND_IN_DIR), Path::new(&at.coord).join(STILLS));
        let config = own.join("projects.toml");
        [&own, &stills].iter().try_for_each(std::fs::create_dir_all).and_then(|_| std::fs::write(&config, "[[project]]\nname = \"stand-in\"\nid = \"stand-in\"\nboxed = false\n")).map_err(|e| unseen(format!("{}: {e}", own.display())))?;
        let name: String = format!("{}-step{}", job.ticket, job.step).chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
        let path = stills.join(format!("{name}.png"));
        // the coordc beside coordd first: the bridge of coordd's own release, as his window's is
        let beside = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.display().to_string()));
        let mut env: Vec<(String, String)> = vec![
            ("PATH".into(), beside.into_iter().chain(std::env::var("PATH").ok()).collect::<Vec<_>>().join(":")),
            ("HOME".into(), own.display().to_string()),
            ("YAAIHH_CONFIG".into(), config.display().to_string()),
            ("YAAIHH_LOCAL_PIPE".into(), "1".into()),
            ("COORD".into(), at.coord.clone()),
            ("COORDD_SOCK".into(), std::env::var("COORDD_SOCK").unwrap_or_else(|_| coord_types::SOCKET.into())),
            ("AGENT_BASE".into(), at.base.clone()),
        ];
        // what draws on this machine's graphics, and nothing else of coordd's environment
        env.extend(std::env::vars().filter(|(k, _)| ["LD_LIBRARY_PATH", "XDG_RUNTIME_DIR", "WAYLAND_DISPLAY", "DISPLAY"].contains(&k.as_str()) || ["WGPU_", "MESA_", "VK_", "LIBGL_", "GALLIUM_"].iter().any(|p| k.starts_with(p))));
        env.extend(set);
        let args = ["--snap", &path.display().to_string(), "--scene", scene, "--size", "1400x900", "--at", "1.0"].map(String::from).to_vec();
        Ok(Stand { bin, args, env, path, page, by: job.by.clone(), look })
    }

    /// Once the stand-in's link is up: the look sent again to every window that reads it, as the load sent it, so
    /// that the stand-in draws the look as his window took it (`Body::Look` on the link, never the file).
    pub fn text_still_look(&mut self) {
        self.emit(coord_types::Body::Look(self.look.clone().unwrap_or(Value::Null)));
    }

    fn journal_path(&self) -> PathBuf {
        Path::new(&self.coord).join(JOURNAL)
    }

    /// The load's answer; None when a test stops coordd in its middle (`STOP_AT`).
    fn text_load_head(&mut self, job: &Job) -> Option<Value> {
        if let Some(v) = not_built() {
            return Some(v);
        }
        if let Some(why) = &job.not_ours {
            return Some(refused(why.clone(), Vec::new()));
        }
        // its journal's end found at this start: it had committed, and its line is written
        if let Some((_, commit)) = self.text_recovered.clone().filter(|(t, _)| *t == job.t) {
            self.text_recovered = None;
            return Some(json!({"head": "Live", "commit": commit}));
        }
        let Some(at) = self.text_at() else { return Some(refused("this coord has no project", Vec::new())) };
        let (placed, repo) = match place_all(&at, &job.files) {
            Ok(x) => x,
            Err((why, places)) => return Some(refused(why, places)),
        };
        let look = placed.iter().all(|p| p.touch == Touch::Look);
        if look {
            if let Err((why, places)) = look_checks(&at, &placed) {
                return Some(refused(why, places));
            }
        } else if let Err(e) = touched(&placed) {
            return Some(refused(e.clone(), places_in(&[e])));
        }
        if read_of(&placed) != job.read {
            return Some(refused("the files, or what they are to be, are not what the check read: open the change again", Vec::new()));
        }
        if index_held(&repo) {
            return Some(refused(format!("{}'s index is held by another git now (index.lock): nothing changes; open the change again once that git is done", repo.display()), Vec::new()));
        }
        if self.journal_path().exists() {
            return Some(refused("another text-change's load did not end (its journal is in the coord's folder): nothing changes until coordd's next start ends it", Vec::new()));
        }
        // the journal first: what a stop leaves is ended at the next start, back or forward
        let olds: Vec<(String, Option<String>)> = placed.iter().map(|p| (p.real.display().to_string(), std::fs::read_to_string(&p.real).ok())).collect();
        if placed.iter().zip(&olds).any(|(p, (_, o))| o.is_none() && p.real.exists()) {
            return Some(refused("a file it changes is not text: nothing changes", Vec::new()));
        }
        let head = git(&repo, &["rev-parse", "HEAD"]).map(|h| h.trim().to_string()).unwrap_or_default();
        let made: Vec<String> = to_make(&placed).iter().map(|d| d.display().to_string()).collect();
        let journal = Journal { ticket: job.ticket.clone(), t: job.t, repo: repo.display().to_string(), head, files: olds, made, by: job.by.clone(), why: job.why.clone() };
        if let Err(e) = write_journal(&self.journal_path(), &journal) {
            let _ = std::fs::remove_file(self.journal_path());
            return Some(refused(format!("nothing changes: its journal could not be written ({e})"), Vec::new()));
        }
        let end = |core: &Core, undo: &Undo| {
            undo.back();
            let _ = std::fs::remove_file(core.journal_path());
        };
        let undo = match write_all(&placed) {
            Ok(u) => u,
            Err((u, e)) => {
                end(self, &u);
                return Some(refused(format!("nothing changes: {e}"), Vec::new()));
            }
        };
        if stops_at("half") || stops_at("written") {
            return None;
        }
        // the files as written, read as the owner's Reload reads them and held to the open tickets; nothing is taken
        // before the commit
        let reread = if look {
            None
        } else {
            match self.reload_checked(crate::ticket::Tickets::read_defs(&self.places(), self.tickets.m.defs.first)) {
                Ok(r) => Some(r),
                Err(r) => {
                    end(self, &undo);
                    return Some(refused(format!("nothing changes: {}", r.join("; ")), places_in(&r)));
                }
            }
        };
        let message = format!("{}\n\n{} by {}, made live by coordd (text-change)\n\nTicket: {}\nBy: {}\n", job.why.trim(), job.ticket, job.by, job.ticket, job.by);
        let hash = match crate::counts::timed_sync("text-change git commit", || commit(&repo, &placed, &message)) {
            Ok(h) => h,
            Err(e) => {
                end(self, &undo);
                return Some(refused(format!("nothing changes: {e}"), Vec::new()));
            }
        };
        if stops_at("committed") {
            return None;
        }
        let why = format!("{}: {}", job.ticket, job.why);
        let taken = match reread {
            Some(r) => self.reload_take(r, job.by.clone(), why, Some(hash.clone())),
            None => self.ticket_input(docket_machine::Input::Reload { by: job.by.clone(), why: format!("{why} (the flavour's look)"), commit: Some(hash.clone()), kinds: Vec::new(), calls: Vec::new() }).map(|r| json!({"said": r, "tickets": []})),
        };
        match taken {
            Ok(said) => {
                let _ = std::fs::remove_file(self.journal_path());
                eprintln!("coordd: tickets: {} live as {hash}: {said}", job.ticket);
                self.retext_notice(&job.ticket, &said);
            }
            Err(e) => {
                // the record refused the reload's line: the commit just made is taken off again (nothing else has it:
                // coordd never pushes), its paths in the index as they were, the files as they were
                let _ = git(&repo, &["update-ref", "-m", "text-change refused by the record", "HEAD", &format!("{hash}^"), &hash]);
                unstage(&repo, &rel_paths(&repo, &placed));
                end(self, &undo);
                return Some(refused(format!("nothing changes: the record refused the reload ({e})"), Vec::new()));
            }
        }
        if look {
            self.read_look();
            // to every window that reads it (`LOOK_PROTOCOL`, coordc's `for_app`); an older one has it in its next
            // snapshot, which it ignores
            self.emit(coord_types::Body::Look(self.look.clone().unwrap_or(Value::Null)));
        }
        // the project file's views, read again, and a snapshot to every window: a window takes the project file only
        // from a snapshot (live.rs), so one already connected draws the new view now, not at its next connect
        // (docket-edge6's read, N-1446 p1785 4.3)
        if placed.iter().any(|p| p.touch == Touch::Views) {
            match crate::project::Project::load(&self.coord) {
                Ok(Some(p)) => {
                    self.project = Some(p);
                    self.resolve_modes();
                }
                Ok(None) => {}
                Err(e) => eprintln!("coordd: tickets: {}: the project file read again: {e}", job.ticket),
            }
            let snap = self.snapshot();
            self.emit(snap.body);
        }
        Some(json!({"head": "Live", "commit": hash}))
    }

    /// The open tickets of a kind the load edited, said on the change's ticket: which took the new text, which kept
    /// their old one and why.
    fn retext_notice(&mut self, ticket: &str, said: &Value) {
        let all = said["tickets"].as_array().cloned().unwrap_or_default();
        let took: Vec<String> = all.iter().filter(|x| x["took"] == true).filter_map(|x| x["id"].as_str().map(String::from)).collect();
        let kept: Vec<String> = all.iter().filter(|x| x["took"] == false).map(|x| format!("{} kept its old text ({})", x["id"].as_str().unwrap_or_default(), x["why"].as_str().unwrap_or_default())).collect();
        let mut line = Vec::new();
        if !took.is_empty() {
            line.push(format!("{} took the new text for what they had not run", took.join(", ")));
        }
        line.extend(kept);
        if !line.is_empty() {
            self.ticket_notice(ticket, "text-change", Some(line.join("; ")));
        }
    }

    fn text_journal(&self) -> Option<Journal> {
        let text = std::fs::read_to_string(self.journal_path()).ok()?;
        serde_json::from_str(&text).map_err(|e| eprintln!("coordd: tickets: the text-change journal does not read ({e}); left as it is")).ok()
    }

    /// Whether the journal's load had committed: the repository's head moved from where it was onto a commit of its
    /// ticket; that commit.
    fn journal_committed(j: &Journal) -> Option<String> {
        let repo = Path::new(&j.repo);
        let head = git(repo, &["rev-parse", "HEAD"]).ok()?.trim().to_string();
        let parent = git(repo, &["rev-parse", "HEAD^"]).ok()?.trim().to_string();
        let msg = git(repo, &["log", "-1", "--format=%B"]).ok()?;
        (head != j.head && parent == j.head && msg.lines().any(|l| l == format!("Ticket: {}", j.ticket))).then_some(head)
    }

    /// At start, before the definitions are read: a load that stopped before its commit has its files put back as they
    /// were (and their index entries), and its journal ends; its load step runs again (`text_owed_again`).
    pub fn text_recover_files(&mut self) {
        let Some(j) = self.text_journal() else { return };
        if Self::journal_committed(&j).is_some() {
            return;
        }
        for (p, old) in &j.files {
            let _ = match old {
                Some(t) => std::fs::write(p, t),
                None => std::fs::remove_file(p),
            };
        }
        for d in j.made.iter().rev() {
            let _ = std::fs::remove_dir(d);
        }
        let repo = Path::new(&j.repo);
        let paths: Vec<String> = j.files.iter().map(|(p, _)| Path::new(p).strip_prefix(repo).unwrap_or(Path::new(p)).display().to_string()).collect();
        unstage(repo, &paths);
        let _ = std::fs::remove_file(self.journal_path());
        eprintln!("coordd: tickets: {}'s load had stopped before its commit: its files are as they were, and it runs again", j.ticket);
    }

    /// At start, once the record is read: a load that stopped after its commit has its reload line written now (its
    /// text was read from the files at this start), and its load step answers Live with that commit.
    pub fn text_recover_line(&mut self) {
        let Some(j) = self.text_journal() else { return };
        let Some(head) = Self::journal_committed(&j) else { return };
        let why = format!("{}: {} (its line written at coordd's start: coordd had stopped between the commit and the line)", j.ticket, j.why);
        match self.ticket_input(docket_machine::Input::Reload { by: j.by.clone(), why, commit: Some(head.clone()), kinds: Vec::new(), calls: Vec::new() }) {
            Ok(_) => {
                let _ = std::fs::remove_file(self.journal_path());
                eprintln!("coordd: tickets: {}'s load had committed {head} with no line: its line is written", j.ticket);
                self.text_recovered = Some((j.t, head));
            }
            Err(e) => eprintln!("coordd: tickets: {}'s load had committed {head}: its line not written ({e}); the journal stays", j.ticket),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_views_with_no_source_change_in_the_project_file() {
        let old = "[project]\nname = \"p\"\n\n[[view]]\nid = \"runs\"\nkind = \"stages\"\nsource = \"./runs\"\n";
        let board = format!("{old}\n[[view]]\nid = \"board\"\nkind = \"board\"\n");
        assert_eq!(views_changed(old, &board), Ok(vec!["board".to_string()]));
        assert!(views_changed(old, &board.replace("name = \"p\"", "name = \"q\"")).unwrap_err().contains("only its [[view]] entries"));
        let norms = format!("[project]\nname = \"p\"\nnorms = \"clingo\"\n\n{}", &old[old.find("[[view]]").unwrap()..]);
        assert!(views_changed(old, &norms).unwrap_err().contains("only its [[view]] entries"), "a norms line");
        let refusal = views_changed(old, &format!("{old}\n[[view]]\nid = \"x\"\nkind = \"x\"\nsource = \"./x\"\n")).unwrap_err();
        assert!(refusal.contains("a view with a command rides a train"), "{refusal}");
        assert!(views_changed(old, &old.replace("./runs", "./other")).unwrap_err().contains("rides a train"));
        assert!(views_changed(old, &old.replace("kind = \"stages\"", "kind = \"stages\"\ntitle = \"Runs\"")).unwrap_err().contains("rides a train"), "a view with a source changes on a train whole");
        assert!(views_changed(old, &old.replace("source = \"./runs\"\n", "source = \"./runs\"\nolder = \"./o\"\n")).unwrap_err().contains("rides a train"));
        assert!(views_changed(old, "[project]\nname = \"p\"\n").unwrap_err().contains("rides a train"), "a view with a source is not removed either");
    }

    /// The design's example (paper-research N-1446 p1785, 1), as an agent writes it into project.toml.
    const MADE: &str = "\n[[view]]\nid = \"edge6-made\"\nkind = \"lens\"\ntitle = \"What docket-edge6 made this week\"\ndraw = \"rows\"\nnodes = [\"commit\", \"job\", \"build\"]\ngroup = \"kind\"\norder = \"newest\"\nwords = \"{label}\"\nwithin = \"7d\"\n\n[view.ask]\naround = \"agent:docket-edge6\"\nhops = 2\nlayers = [\"work\"]\nlimit = 2000\n";

    /// p1789, 2: a template the window's check refuses is refused here, at the line naming it, its faults in its words.
    #[test]
    fn a_template_changes_only_as_the_windows_check_takes_it() {
        let old = "[project]\nname = \"p\"\n";
        assert_eq!(views_changed(old, &format!("{old}{MADE}")), Ok(vec!["edge6-made".to_string()]));
        for (from, to, says) in [
            ("hops = 2", "hops = 4", "`ask.hops`: a whole number from 0 to 3 is wanted, not 4"),
            ("hops = 2", "hops = 2\nfolder = \"docket-edge6\"", "`ask.folder`: an agent's folder and its sessions are the file view's own"),
            ("hops = 2", "hops = 2\nsession = \"session:5c6f1a2b\"", "`ask.session`"),
            ("words = \"{label}\"", "words = \"{text}\"", "`words`: {text} is not a field"),
        ] {
            let new = format!("{old}{}", MADE.replace(from, to));
            let r = views_changed(old, &new).unwrap_err();
            assert!(r.starts_with("project.toml:5: the view edge6-made is not a template the window takes: ") && r.contains(says), "{to}: {r}");
            assert_eq!(places_in(&[r]), vec!["project.toml:5".to_string()], "{to}: the line naming it");
        }
        assert!(views_changed(old, &format!("{old}{}", MADE.replace("kind = \"lens\"", "kind = \"board\""))).is_ok(), "the check is a template's alone");
    }

    #[test]
    fn a_refusals_places_are_its_files_lines() {
        let r = vec!["/p/kinds/a.kind:3: kind a: no such call".to_string(), "the kind b is gone (`/p/kinds/b.kind:1`)".into(), "N-12:4 is no file".into()];
        assert_eq!(places_in(&r), vec!["/p/kinds/a.kind:3".to_string(), "/p/kinds/b.kind:1".into()]);
        // a kind's refusal in the check's words (docket_machine check.rs): its file, then the line
        let r = vec!["/p/kinds/bad.kind: kind bad refused: line 5: no call `nosuch`".to_string(), "/p/kinds/c.kind: kind c refused: its base is gone".into()];
        assert_eq!(places_in(&r), vec!["/p/kinds/bad.kind:5".to_string()]);
    }

    #[test]
    fn the_owners_handlers_are_read_from_the_text_not_its_lines() {
        let a = "kind k\n  form x: Text\n  pattern\n    return x\n  when owner_post(p) do\n    post p.text\n  when ask(a) do\n    post a.text\n";
        assert_eq!(owner_handlers(a), vec!["when owner_post(p) do\npost p.text\n".to_string()]);
        // a line above moves it; it is the same handler
        assert_eq!(owner_handlers(&a.replace("  pattern\n", "  pattern\n    // a note\n")), owner_handlers(a));
        assert_ne!(owner_handlers(&a.replace("post p.text", "post \"no\"")), owner_handlers(a));
        assert!(owner_handlers("kind k\n  pattern\n    return 1\n").is_empty());
    }
}
