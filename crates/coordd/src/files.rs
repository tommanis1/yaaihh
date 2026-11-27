//! The owner's file requests (N-412 ask 13, paper-research p124): what the app read and wrote on the host's disk
//! itself, served here for an app that cannot (the Windows window). The Files page's roots, folders and files
//! (`Roots`, `ListDir`, `ReadFile`, which the image viewer uses too), a path named in a chat (`Locate`), the kinds and
//! calls the edit check reads (`Sources`) and the edit itself (`EditFile`). Each is the app's own code moved here
//! (yaaihh's files.rs, paths.rs, edit.rs), so the page shows what it showed. Owner only.
//!
//! What is read (p146, p167): inside the roots, agent-base and the library; and a picture the viewer opens wherever
//! it is, so an agent's picture named in a chat opens as it did. Never, wherever it is: the credential places
//! (`never_served`). Every check is on the path with its links followed. Written: only the project's and its
//! library's kinds and calls.

use base64::Engine;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The most one `ReadFile` reply carries, about 1.4 MB of base64 on the bridge's one stdout: the app asks again
/// from where it got to (docket-core3, p135: an 8 MiB line held every event and key echo behind it).
pub const MAX_PIECE: u64 = 1 << 20;

/// What the requests need of the coord, taken under the lock and used off it.
#[derive(Clone, Debug, Default)]
pub struct Places {
    /// The owner's home, as `~/` takes it.
    pub home: String,
    /// The coord's folder.
    pub coord: String,
    /// The project's folder, where its project file is.
    pub project: Option<String>,
    /// The project file's `[project] files`, as written (`~/` for the home).
    pub files: Vec<String>,
    /// The project's repositories (`[project] repos`), each by its name with its folder, in the file's order
    /// (`Project::repos`).
    pub repos: Vec<(String, String)>,
    /// Each agent's working folder.
    pub cwds: Vec<String>,
    /// Where kinds and calls are read from: agent-base, the library, the project's folder (`project::folders`).
    pub folders: Vec<String>,
    /// The flavour's look file (`change::At::look`), when the project is a flavour's: `Sources` serves it beside the
    /// kinds, for the look's editor where kinds are edited (N-866 p6, 2).
    pub look: Option<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Root {
    pub name: String,
    pub path: String,
    pub here: bool,
}

impl Places {
    /// `~/x` as a path under the home.
    pub(crate) fn expand(&self, p: &str) -> PathBuf {
        match p.strip_prefix("~/") {
            Some(r) => Path::new(&self.home).join(r),
            None if p == "~" => PathBuf::from(&self.home),
            None => PathBuf::from(p),
        }
    }

    /// The home with its links followed.
    fn home(&self) -> PathBuf {
        canon(Path::new(&self.home))
    }

    /// Where reading is allowed: the roots, agent-base, the library (and the project's folder, a root already).
    fn allowed(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = roots(self).into_iter().map(|r| PathBuf::from(r.path)).collect();
        out.extend(self.folders.iter().map(|f| canon(Path::new(f))));
        out
    }

    /// Why P (links followed) is never served, whatever the roots: a credential place (paper-research p146, p147),
    /// or a repository's own folder (`.git/config` can hold a remote's token). The kept graph asks git in none of them
    /// either (`ticket::work_repos::place`).
    pub(crate) fn never_served(&self, p: &Path) -> Option<String> {
        never_served(&self.home(), p)
    }

    /// Whether P (links followed) is served by `ReadFile`, or why not; FOLDER: by `ListDir` (no picture rule).
    fn served(&self, p: &Path, folder: bool) -> Result<(), String> {
        if let Some(why) = self.never_served(p) {
            return Err(format!("{} is never served ({why})", p.display()));
        }
        if self.allowed().iter().any(|r| p.starts_with(r)) || (!folder && is_image(p)) {
            Ok(())
        } else {
            Err(format!("{} is not in this project's folders (its Files page's roots, agent-base, the library)", p.display()))
        }
    }

    /// PATH with its links followed, when it is served (FOLDER: as a folder); else why not.
    fn open(&self, path: &str, folder: bool) -> Result<PathBuf, String> {
        let p = std::fs::canonicalize(self.expand(path)).map_err(|e| format!("{path}: {e}"))?;
        self.served(&p, folder)?;
        Ok(p)
    }
}

pub use machine_state::kept::never_served;

fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// A picture the viewer opens (yaaihh viewer.rs `is_image`, files.rs `is_image`).
fn is_image(p: &Path) -> bool {
    matches!(p.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref(), Some("png" | "jpg" | "jpeg" | "gif" | "webp"))
}

fn last_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

/// The Files page's roots, in their order, the later ones inside an earlier one left out (yaaihh files.rs `roots`):
/// `files`, the project's repositories by their names (paper-research N-1446 p3387, 1: his yaaihh project's Files
/// showed none of its four), the agents' folders, the conversations, the project file's folder. A root that is `/`,
/// the home or a folder above it is left out (an agent's cwd can be the home: p135).
pub fn roots(at: &Places) -> Vec<Root> {
    let mut all: Vec<(String, PathBuf)> = Vec::new();
    for f in &at.files {
        let path = at.expand(f);
        all.push((last_name(&path), path));
    }
    for (name, path) in &at.repos {
        all.push((name.clone(), at.expand(path)));
    }
    for c in at.cwds.iter().filter(|c| !c.is_empty()) {
        let path = PathBuf::from(c);
        all.push((last_name(&path), path));
    }
    let conv = Path::new(&at.coord).join("conversations");
    if conv.is_dir() {
        all.push(("conversations".into(), conv));
    }
    if let Some(def) = at.project.as_deref().filter(|d| !d.is_empty()) {
        all.push(("definition".into(), PathBuf::from(def)));
    }
    let home = at.home();
    let mut out: Vec<Root> = Vec::new();
    for (name, path) in all {
        // a path as the host sees it (symlinks followed), so a folder named two ways is one root
        let path = canon(&path);
        if home.starts_with(&path) || out.iter().any(|r| path.starts_with(&r.path)) {
            continue;
        }
        let here = path.is_dir();
        out.push(Root { name, path: path.display().to_string(), here });
    }
    out
}

/// `ListDir`: folder PATH's entries, folders first, then files, by name with case aside; no `.git`. An entry that
/// would not open (a link out of the roots, a credential place) says `"outside": true`. GIT (`FILE_STATES_PROTOCOL`,
/// N-607): each entry's git state, from git asked in that folder ([`git_states`]); git is run only then.
pub fn list_dir(at: &Places, path: &str, git: bool) -> Result<String, String> {
    let dir = at.open(path, true)?;
    let rd = std::fs::read_dir(&dir).map_err(|e| format!("{path}: {e}"))?;
    let mut v: Vec<coord_types::DirEntry> = rd
        .filter_map(Result::ok)
        .map(|e| {
            let p = e.path();
            let dir = p.is_dir();
            let outside = std::fs::canonicalize(&p).map_or(true, |c| at.served(&c, dir).is_err());
            coord_types::DirEntry { name: e.file_name().to_string_lossy().into_owned(), dir, outside, git: None }
        })
        .filter(|e| e.name != ".git")
        .collect();
    if git {
        let states = git_states(&dir);
        for e in &mut v {
            e.git = states.get(&e.name).map(|s| s.to_string());
        }
    }
    v.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())).then_with(|| a.name.cmp(&b.name)));
    Ok(serde_json::to_string(&v).unwrap())
}

/// The git state of each entry of folder DIR (already checked as served, links followed) that git reports whole:
/// `ignored` or `new` (untracked, not ignored); an entry not named is tracked, or a folder git reports
/// neither way as a whole. Two `git ls-files` in DIR, its own entries only (`--directory`: a folder untracked or
/// ignored as a whole is one name, `target/`, not walked), paths relative to DIR; only a name that is one of DIR's own
/// entries is kept (git's paths are matched back, never served). Read-only (`--no-optional-locks`); a repository's own
/// config runs nothing (no system config, fsmonitor off, no hooks). Not a repository, no git, an error: nothing, the
/// folder shows as today.
pub fn git_states(dir: &Path) -> std::collections::HashMap<String, &'static str> {
    let ask = |ignored: bool| -> Option<Vec<u8>> {
        let mut c = std::process::Command::new("git");
        c.current_dir(dir)
            .args(["--no-optional-locks", "-c", "core.fsmonitor=false", "-c", "core.hooksPath=/dev/null", "-c", "core.untrackedCache=false"])
            .args(["ls-files", "-z", "--others", "--exclude-standard", "--directory"])
            .args(if ignored { &["--ignored"][..] } else { &["--no-empty-directory"][..] })
            .args(["--", "."])
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0");
        for v in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_COMMON_DIR", "GIT_NAMESPACE", "GIT_CONFIG", "GIT_CONFIG_PARAMETERS", "GIT_CONFIG_COUNT", "GIT_EXTERNAL_DIFF"] {
            c.env_remove(v);
        }
        let o = c.stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok()?;
        o.status.success().then_some(o.stdout)
    };
    let mut out = std::collections::HashMap::new();
    let names = |b: &[u8]| -> Vec<String> {
        b.split(|c| *c == 0)
            .filter(|p| !p.is_empty())
            .map(|p| String::from_utf8_lossy(p).into_owned())
            // DIR's own entries only: `name` or a whole folder `name/`; a path further down says nothing of its entry
            .filter_map(|p| {
                let p = p.strip_suffix('/').unwrap_or(&p).to_string();
                (!p.is_empty() && !p.contains('/') && p != "." && p != ".." && p != ".git").then_some(p)
            })
            .collect()
    };
    if let Some(b) = ask(false) {
        for n in names(&b) {
            out.insert(n, "new");
        }
    }
    if let Some(b) = ask(true) {
        for n in names(&b) {
            out.insert(n, "ignored");
        }
    }
    out
}

/// `ReadFile`: file PATH's size and mtime (ms since the epoch), and LEN bytes (at most MAX_PIECE) from FROM in
/// base64.
pub fn read_file(at: &Places, path: &str, from: u64, len: Option<u64>) -> Result<String, String> {
    use std::io::{Read, Seek, SeekFrom};
    let file = at.open(path, false)?;
    let meta = std::fs::metadata(&file).map_err(|e| format!("{path}: {e}"))?;
    if meta.is_dir() {
        return Err(format!("{path} is a folder"));
    }
    let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64);
    let len = len.unwrap_or(MAX_PIECE).min(MAX_PIECE);
    let mut data = Vec::new();
    if len > 0 && from < meta.len() {
        let mut f = std::fs::File::open(&file).map_err(|e| format!("{path}: {e}"))?;
        f.seek(SeekFrom::Start(from)).map_err(|e| format!("{path}: {e}"))?;
        f.take(len).read_to_end(&mut data).map_err(|e| format!("{path}: {e}"))?;
    }
    Ok(json!({"size": meta.len(), "modified": modified, "data": base64::engine::general_purpose::STANDARD.encode(&data)}).to_string())
}

/// `PdfPage`: page PAGE (from 1, kept within the file's pages) of the PDF at PATH, served as `ReadFile` serves a
/// file, drawn as a PNG WIDTH pixels wide (default 1200, from 200 to 2400) by poppler's `pdftoppm` at nice 10, its
/// pages counted by `pdfinfo`, each with a minute at most: `{pages, page, data}`, data the PNG in base64 (the owner's
/// walk item 22: a PDF read inside the window, by nothing of an editor's).
pub fn pdf_page(at: &Places, path: &str, page: u32, width: Option<u32>) -> Result<String, String> {
    if !Path::new(path).extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
        return Err(format!("{path} is no PDF"));
    }
    let file = at.open(path, false)?;
    let run = |args: &[&str]| -> Result<Vec<u8>, String> {
        let out = std::process::Command::new("timeout")
            .arg("60")
            .args(["nice", "-n", "10"])
            .args(args)
            .arg("--")
            .arg(&file)
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| format!("{}: {e}", args[0]))?;
        if !out.status.success() {
            let why = String::from_utf8_lossy(&out.stderr);
            return Err(format!("{path}: {} did not draw it ({})", args[0], why.lines().last().unwrap_or("no answer in 60 s").trim()));
        }
        Ok(out.stdout)
    };
    let info = String::from_utf8_lossy(&run(&["pdfinfo"])?).into_owned();
    let pages: u32 = info.lines().find_map(|l| l.strip_prefix("Pages:")).and_then(|n| n.trim().parse().ok()).ok_or(format!("{path}: pdfinfo gives no page count"))?;
    if pages == 0 {
        return Err(format!("{path} has no page"));
    }
    let page = page.clamp(1, pages);
    let (n, w) = (page.to_string(), width.unwrap_or(1200).clamp(200, 2400).to_string());
    let png = run(&["pdftoppm", "-png", "-f", &n, "-l", &n, "-scale-to-x", &w, "-scale-to-y", "-1"])?;
    Ok(json!({"pages": pages, "page": page, "data": base64::engine::general_purpose::STANDARD.encode(&png)}).to_string())
}

/// `Locate`: the file PATH names (yaaihh paths.rs `resolve`): itself when absolute (`~` expanded); else under CWD or
/// one of its parents; else the first file under the SEARCH folders whose path ends in it. `{path, readable}`:
/// readable, whether `ReadFile` serves it, so the app can say why it cannot open one.
pub fn locate(at: &Places, path: &str, cwd: Option<&str>, search: &[String]) -> Result<String, String> {
    let found = |p: PathBuf| {
        let readable = std::fs::canonicalize(&p).is_ok_and(|c| at.served(&c, false).is_ok());
        Ok(json!({"path": p.display().to_string(), "readable": readable}).to_string())
    };
    let not_found = || Err(format!("{path}: no such file here"));
    let full = at.expand(path);
    if full.is_absolute() {
        return if full.exists() { found(full) } else { not_found() };
    }
    if let Some(f) = cwd.filter(|c| !c.is_empty()).and_then(|c| Path::new(c).ancestors().map(|d| d.join(path)).find(|f| f.exists())) {
        return found(f);
    }
    let dirs: Vec<PathBuf> = search.iter().map(|d| at.expand(d)).collect();
    // a folder is a word find reads as one: `-delete` or `-exec` would be an expression (p135)
    if let Some(bad) = dirs.iter().find(|d| !d.is_absolute() || d.to_string_lossy().starts_with('-')) {
        return Err(format!("{}: a search folder is an absolute path", bad.display()));
    }
    if dirs.is_empty() {
        return not_found();
    }
    let pattern = format!("*/{}", path.trim_start_matches("./"));
    // build and VCS folders are not searched
    let skip = ["(", "-name", ".git", "-o", "-name", "target", "-o", "-name", "_build", "-o", "-name", "dist-newstyle", "-o", "-name", "node_modules", ")", "-prune", "-o"];
    let out = std::process::Command::new("find")
        .args(&dirs)
        .args(skip)
        .args(["-path", &pattern, "-print", "-quit"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("find: {e}"))?;
    match String::from_utf8_lossy(&out.stdout).lines().next() {
        Some(first) if !first.is_empty() => found(PathBuf::from(first)),
        _ => not_found(),
    }
}

/// The files with extension EXT in DIR, with their text, by path.
fn texts(dir: &str, ext: &str) -> Vec<Value> {
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == ext))
        .filter_map(|p| std::fs::read_to_string(&p).ok().map(|t| (p.display().to_string(), t)))
        .collect();
    out.sort();
    out.into_iter().map(|(path, text)| json!({"path": path, "text": text})).collect()
}

/// `Sources`: every kinds and calls file of the folders coordd reads them from, in their order; and the flavour's look
/// file, its text none while there is no file yet (none at all for a project that is no flavour's). A look that is a
/// link or over its cap is not read, as `Core::read_look` does not read it.
pub fn sources(at: &Places) -> String {
    let of = |sub: &str, ext: &str| -> Vec<Value> { at.folders.iter().flat_map(|d| texts(&crate::project::place(d, sub), ext)).collect() };
    let look = at.look.as_ref().map(|l| {
        let plain = std::fs::symlink_metadata(l).is_ok_and(|m| m.is_file() && m.len() <= crate::change::LOOK_CAP as u64);
        json!({"path": l, "text": if plain { std::fs::read_to_string(l).ok() } else { None }})
    });
    json!({"kinds": of("kinds", "kind"), "calls": of("calls", "call"), "look": look}).to_string()
}

/// `EditFile` (yaaihh edit.rs `write_commit`): write TEXT to PATH, a kinds or calls file of the project or its library,
/// when it is still OLD (None: it does not exist yet), and commit it where it is with MESSAGE: the commit's short
/// name, or why it was not committed, in `coord_types::save_words`' words (the file by its folder and name, N-802).
pub fn edit_file(at: &Places, path: &str, old: Option<&str>, text: &str, message: &str) -> Result<String, String> {
    use coord_types::save_words as W;
    let file = at.expand(path);
    if !file.extension().is_some_and(|x| x == "kind" || x == "call") {
        return Err(W::not_kinds(path));
    }
    let (Some(dir), Some(name)) = (file.parent(), file.file_name()) else { return Err(W::not_kinds(path)) };
    // its folder as the host sees it, under the project's folder or its library (agent-base's are edited where they
    // are); a project with kinds but no calls folder yet gets one, as the app's write made it
    let Some(there) = dir.ancestors().find(|d| d.exists()) else { return Err(W::no_folder(path)) };
    let there = std::fs::canonicalize(there).map_err(|_| W::no_folder(path))?;
    let mine: Vec<PathBuf> = at.folders.iter().skip(1).map(|f| std::fs::canonicalize(f).unwrap_or_else(|_| PathBuf::from(f))).collect();
    if dir.components().any(|c| matches!(c, std::path::Component::ParentDir)) || !mine.iter().any(|f| there.starts_with(f)) {
        return Err(W::outside(path));
    }
    if let Some(why) = at.never_served(dir).or_else(|| at.never_served(&there)) {
        return Err(W::protected(path, &why));
    }
    std::fs::create_dir_all(dir).map_err(|e| W::unwritten(path, e))?;
    let dir = std::fs::canonicalize(dir).map_err(|e| W::unwritten(path, e))?;
    let file = dir.join(name);
    // written through, a link would put the text wherever it points (p135)
    if std::fs::symlink_metadata(&file).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(W::link(path));
    }
    if let Some(why) = at.never_served(&file) {
        return Err(W::protected(path, &why));
    }
    match (old, std::fs::read_to_string(&file)) {
        (Some(o), Ok(now)) if now == o => {}
        (Some(_), Ok(_)) => return Err(W::changed(path)),
        (Some(_), Err(_)) => return Err(W::changed(path)),
        (None, Ok(_)) => return Err(W::exists(path)),
        (None, Err(_)) => {}
    }
    std::fs::write(&file, text).map_err(|e| W::unwritten(path, e))?;
    commit_where(&dir, &name.to_string_lossy(), message)
}

/// Whether folder DIR is in a git repository's work tree.
pub(crate) fn in_work_tree(dir: &Path) -> bool {
    std::process::Command::new("git").arg("-C").arg(dir).args(["rev-parse", "--is-inside-work-tree"]).stdin(std::process::Stdio::null()).output().is_ok_and(|o| o.status.success())
}

/// File NAME of folder DIR, just written, committed alone where it is with MESSAGE (`EditFile`, and the owner's Add a
/// repository, project.rs `add_repo`): the commit's short name, or that its folder is in no git repository; or why it
/// was not committed, in `coord_types::save_words`' words.
pub(crate) fn commit_where(dir: &Path, name: &str, message: &str) -> Result<String, String> {
    use coord_types::save_words as W;
    let git = |args: &[&str]| std::process::Command::new("git").arg("-C").arg(dir).args(args).stdin(std::process::Stdio::null()).output();
    if !in_work_tree(dir) {
        return Ok("written (its folder is no git repository: not committed)".into());
    }
    let add = git(&["add", "--", name]).map_err(|e| W::uncommitted("add", &e.to_string()))?;
    if !add.status.success() {
        return Err(W::uncommitted("add", &String::from_utf8_lossy(&add.stderr)));
    }
    let c = git(&["commit", "-q", "-m", message, "--", name]).map_err(|e| W::uncommitted("commit", &e.to_string()))?;
    if !c.status.success() {
        return Err(W::uncommitted("commit", &String::from_utf8_lossy(&c.stderr)));
    }
    let h = git(&["rev-parse", "--short", "HEAD"]).map_err(|e| e.to_string())?;
    Ok(format!("commit {}", String::from_utf8_lossy(&h.stdout).trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// A coord with a home (credential places in it, a picture outside the roots), a project folder (kinds, calls),
    /// a library, agent-base, a folder outside, and links out of the roots.
    fn scene(name: &str) -> (PathBuf, Places) {
        let dir = std::env::temp_dir().join(format!("owner-files-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for d in [
            "coord/conversations",
            "base/kinds",
            "lib",
            "proj/kinds",
            "proj/calls",
            "proj/src/deep",
            "proj/src/.git",
            "outside",
            "home/.claude",
            "home/.claude-alt",
            "home/.ssh",
            "home/.config/yaaihh",
            "home/.local/opt/EasyCLIProxyAPI",
            "home/agent-coord/accounts/claude/a/config",
            "home/agent-coord/research/claude-home",
            "home/pics",
            "proj/lanes/base/box/run/ffprof",
        ] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        let w = |p: &str, t: &str| std::fs::write(dir.join(p), t).unwrap();
        w("base/kinds/base.kind", "kind b\n");
        w("lib/lib.kind", "kind l\n");
        w("lib/lib.call", "call l()\n");
        w("proj/kinds/a.kind", "kind a\n");
        w("proj/calls/c.call", "call c()\n");
        w("proj/src/deep/Main.hs", "main = pure ()\n");
        w("proj/src/Zeta.txt", "z");
        w("proj/src/alpha.txt", "0123456789");
        w("proj/src/.git/config", "token");
        w("proj/src/route", "not a key");
        w("proj/lanes/base/box/run/ffprof/cookies", "c");
        w("outside/secret", "no");
        w("outside/target.kind", "kind t\n");
        w("outside/shot.png", "png");
        w("home/.claude/.credentials.json", "k");
        w("home/.claude-alt/settings.json", "k");
        w("home/.ssh/id", "k");
        w("home/.config/yaaihh/a.toml", "k");
        w("home/.local/opt/EasyCLIProxyAPI/config.yaml", "k");
        w("home/agent-coord/accounts/claude/a/config/x.json", "k");
        w("home/agent-coord/research/registry.jsonl", "");
        w("home/agent-coord/research/route", "k");
        w("home/agent-coord/research/.proxy-key", "k");
        w("home/agent-coord/research/telemetry.token", "k");
        w("home/agent-coord/research/claude-home/x", "k");
        w("home/pics/shot.png", "png");
        w("home/pics/notes.txt", "t");
        symlink(dir.join("outside"), dir.join("proj/src/out-link")).unwrap();
        symlink(dir.join("outside/secret"), dir.join("proj/src/secret-link")).unwrap();
        symlink(dir.join("outside/target.kind"), dir.join("proj/kinds/link.kind")).unwrap();
        symlink(dir.join("outside"), dir.join("lib/linked")).unwrap();
        symlink(dir.join("home/.ssh"), dir.join("proj/src/ssh-link")).unwrap();
        let s = |p: &str| dir.join(p).display().to_string();
        let at = Places {
            home: s("home"),
            coord: s("coord"),
            project: Some(s("proj")),
            files: vec![s("proj/src"), s("missing")],
            repos: Vec::new(),
            // an agent working in the home itself, and one at /
            cwds: vec![s("proj/src/deep"), String::new(), s("home"), "/".into()],
            folders: vec![s("base"), s("lib"), s("proj")],
            look: None,
        };
        (dir, at)
    }

    fn p(dir: &Path, rel: &str) -> String {
        dir.join(rel).display().to_string()
    }

    #[test]
    fn the_roots_are_the_files_pages_with_the_nested_the_home_and_slash_left_out() {
        let (dir, at) = scene("roots");
        let r = roots(&at);
        let names: Vec<(&str, bool)> = r.iter().map(|r| (r.name.as_str(), r.here)).collect();
        assert_eq!(names, [("src", true), ("missing", false), ("conversations", true), ("definition", true)]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The project's repositories are roots by their names, in the project file's order, after `files` and before
    /// the agents' folders; one inside an earlier root is left out (paper-research N-1446 p3387, 1).
    #[test]
    fn the_projects_repositories_are_roots_by_their_names() {
        let (dir, at) = scene("repos");
        for d in ["work/main", "lib2"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        let at = Places { repos: vec![("main-repo".into(), p(&dir, "work/main")), ("inner".into(), p(&dir, "proj/src/deep")), ("second".into(), p(&dir, "lib2"))], ..at };
        let names: Vec<String> = roots(&at).into_iter().map(|r| r.name).collect();
        assert_eq!(names, ["src", "missing", "main-repo", "second", "conversations", "definition"]);
        assert!(at.open(&p(&dir, "lib2"), true).is_ok(), "a repository is served");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// N-607: with `git`, each entry git reports whole says so: a file or folder untracked `new`, ignored `ignored`
    /// (a whole ignored folder one entry, not walked); tracked, and a folder with both kinds inside, nothing. Without
    /// `git`, nothing (git not run); outside a repository, nothing; nothing under `.git` is listed.
    #[test]
    fn a_folders_entries_say_what_git_makes_of_them() {
        let dir = std::env::temp_dir().join(format!("docket-git-states-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = dir.join("repo");
        for d in ["home", "plain/sub", "repo/target/debug", "repo/newdir", "repo/mixed", "repo/sub/build"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        for (f, t) in [("repo/.gitignore", "target/\n*.log\nbuild/\n"), ("repo/a.md", "a"), ("repo/mixed/t.rs", "t"), ("repo/sub/s.rs", "s"), ("repo/n.rs", "n"), ("repo/x.log", "x"), ("repo/target/debug/big", "b"), ("repo/newdir/f", "f"), ("repo/mixed/u.rs", "u"), ("repo/sub/build/o", "o"), ("plain/p.txt", "p")] {
            std::fs::write(dir.join(f), t).unwrap();
        }
        let git = |args: &[&str]| assert!(std::process::Command::new("git").current_dir(&repo).args(["-c", "user.name=t", "-c", "user.email=t@x"]).args(args).env("GIT_CONFIG_NOSYSTEM", "1").output().unwrap().status.success(), "{args:?}");
        git(&["init", "-q"]);
        git(&["add", ".gitignore", "a.md", "mixed/t.rs", "sub/s.rs"]);
        git(&["commit", "-qm", "base"]);
        let s = |p: &str| dir.join(p).display().to_string();
        let at = Places { home: s("home"), coord: s("home"), project: None, files: vec![s("repo"), s("plain")], repos: vec![], cwds: vec![], folders: vec![], look: None };
        let list = |path: &str, git: bool| -> Vec<(String, Option<String>)> {
            let v: Vec<coord_types::DirEntry> = serde_json::from_str(&list_dir(&at, &s(path), git).unwrap()).unwrap();
            v.into_iter().map(|e| (e.name, e.git)).collect()
        };
        let want = |v: &[(&str, Option<&str>)]| -> Vec<(String, Option<String>)> { v.iter().map(|(n, g)| (n.to_string(), g.map(String::from))).collect() };
        assert_eq!(
            list("repo", true),
            want(&[("mixed", None), ("newdir", Some("new")), ("sub", None), ("target", Some("ignored")), (".gitignore", None), ("a.md", None), ("n.rs", Some("new")), ("x.log", Some("ignored"))])
        );
        assert_eq!(list("repo/mixed", true), want(&[("t.rs", None), ("u.rs", Some("new"))]), "below the repository's top, names relative to the folder");
        assert_eq!(list("repo/sub", true), want(&[("build", Some("ignored")), ("s.rs", None)]));
        assert!(list("repo", false).iter().all(|(_, g)| g.is_none()), "not asked: git not run");
        assert!(list("plain", true).iter().all(|(_, g)| g.is_none()), "outside a repository");
        assert!(list("repo", true).iter().all(|(n, _)| n != ".git"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `PdfPage` (the owner's walk item 22): a PDF in the roots drawn page by page as a PNG, the page kept within its
    /// pages; a file that is no PDF, or one outside the roots, refused. The drawing needs poppler's tools where the test
    /// runs; without them only the refusals are held.
    #[test]
    fn a_pdfs_page_is_drawn_as_a_picture_and_no_other_file_is() {
        let dir = std::env::temp_dir().join(format!("docket-pdf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("repo")).unwrap();
        std::fs::create_dir_all(dir.join("outside")).unwrap();
        // a made-up two-page PDF, its cross-reference left for the reader to rebuild
        let page = |t: &str| format!("BT /F1 24 Tf 72 700 Td ({t}) Tj ET");
        let (a, b) = (page("A made-up paper"), page("Page two"));
        let pdf = format!(
            "%PDF-1.4\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >> endobj\n\
             3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 5 0 R /Resources << /Font << /F1 7 0 R >> >> >> endobj\n\
             4 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 6 0 R /Resources << /Font << /F1 7 0 R >> >> >> endobj\n\
             5 0 obj << /Length {} >> stream\n{a}\nendstream endobj\n6 0 obj << /Length {} >> stream\n{b}\nendstream endobj\n\
             7 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF\n",
            a.len(),
            b.len()
        );
        for f in ["repo/paper.pdf", "outside/paper.pdf"] {
            std::fs::write(dir.join(f), &pdf).unwrap();
        }
        std::fs::write(dir.join("repo/notes.txt"), "notes").unwrap();
        let s = |p: &str| dir.join(p).display().to_string();
        let at = Places { home: s("repo"), coord: s("repo"), project: None, files: vec![s("repo")], repos: vec![], cwds: vec![], folders: vec![], look: None };
        assert!(pdf_page(&at, &s("repo/notes.txt"), 1, None).is_err_and(|e| e.contains("is no PDF")));
        assert!(pdf_page(&at, &s("outside/paper.pdf"), 1, None).is_err());
        if std::process::Command::new("pdfinfo").arg("-v").output().is_err() {
            eprintln!("no pdfinfo here: the drawing is not held");
            return;
        }
        let v: Value = serde_json::from_str(&pdf_page(&at, &s("repo/paper.pdf"), 9, Some(300)).unwrap()).unwrap();
        assert_eq!((v["pages"].as_u64(), v["page"].as_u64()), (Some(2), Some(2)), "page 9 of 2 is the last");
        let png = base64::engine::general_purpose::STANDARD.decode(v["data"].as_str().unwrap()).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 300, "as wide as asked");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_lists_folders_first_and_marks_what_would_not_open() {
        let (dir, at) = scene("list");
        let l: Value = serde_json::from_str(&list_dir(&at, &p(&dir, "proj/src"), false).unwrap()).unwrap();
        let got: Vec<(String, bool)> = l.as_array().unwrap().iter().map(|e| (e["name"].as_str().unwrap().to_string(), e["outside"] == true)).collect();
        let want = [("deep", false), ("out-link", true), ("ssh-link", true), ("alpha.txt", false), ("route", false), ("secret-link", true), ("Zeta.txt", false)];
        assert_eq!(got, want.map(|(n, o)| (n.to_string(), o)));
        // a link out of the roots, and a way out through .., are the outside
        assert!(list_dir(&at, &p(&dir, "proj/src/out-link"), false).is_err());
        assert!(list_dir(&at, &format!("{}/../../outside", p(&dir, "proj/src")), false).is_err());
        assert!(read_file(&at, &p(&dir, "proj/src/secret-link"), 0, None).is_err_and(|e| e.contains("not in this project's folders")));
        // agent-base and the library are read, for the edit check
        assert!(read_file(&at, &p(&dir, "lib/lib.kind"), 0, None).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_picture_is_served_wherever_it_is_and_nothing_else_outside() {
        let (dir, at) = scene("picture");
        assert!(read_file(&at, &p(&dir, "home/pics/shot.png"), 0, None).is_ok());
        assert!(read_file(&at, &p(&dir, "outside/shot.png"), 0, None).is_ok());
        assert!(read_file(&at, &p(&dir, "home/pics/notes.txt"), 0, None).is_err());
        assert!(list_dir(&at, &p(&dir, "home/pics"), false).is_err(), "a folder is no picture");
        // the extension is the target's, links followed: a picture's name on a key is the key (docket-core3, p177)
        symlink(dir.join("home/.ssh/id"), dir.join("home/pics/x.png")).unwrap();
        symlink(dir.join("outside/secret"), dir.join("proj/src/y.png")).unwrap();
        assert!(read_file(&at, &p(&dir, "home/pics/x.png"), 0, None).is_err_and(|e| e.contains("never served")));
        assert!(read_file(&at, &p(&dir, "proj/src/y.png"), 0, None).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Each credential place, and a name made from one (a writer's own `.proxy-key.new.XXXXXX`), is refused by name.
    #[test]
    fn the_credential_places_are_never_served_wherever_the_roots_are() {
        let (dir, mut at) = scene("never");
        // even with the whole home and the project as files roots, which the home rule would leave out anyway
        at.files.push(p(&dir, "home/agent-coord"));
        at.files.push(p(&dir, "home/.claude"));
        // and what is named from one of them: route.sh's own `.proxy-key.new.XXXXXX` while its command runs, or left
        // by a kill (build 53; proto-lead N-1396 p467). Made-up bytes; a name that only holds one is served
        std::fs::create_dir_all(dir.join("home/agent-coord/research/codex-home.old")).unwrap();
        for f in ["home/agent-coord/research/.proxy-key.new.ab12cd", "home/agent-coord/research/telemetry.token.tmp", "home/agent-coord/research/codex-home.old/config.toml", "proj/src/notes-on-.proxy-key.txt"] {
            std::fs::write(dir.join(f), "made-up").unwrap();
        }
        assert!(read_file(&at, &p(&dir, "proj/src/notes-on-.proxy-key.txt"), 0, None).is_ok());
        assert!(list_dir(&at, &p(&dir, "home/agent-coord/research/codex-home.old"), false).is_err());
        for f in [
            "home/agent-coord/research/.proxy-key.new.ab12cd",
            "home/agent-coord/research/telemetry.token.tmp",
            "home/agent-coord/research/codex-home.old/config.toml",
            "home/.claude/.credentials.json",
            "home/.claude-alt/settings.json",
            "home/.ssh/id",
            "home/.config/yaaihh/a.toml",
            "home/.local/opt/EasyCLIProxyAPI/config.yaml",
            "home/agent-coord/accounts/claude/a/config/x.json",
            "home/agent-coord/research/route",
            "home/agent-coord/research/.proxy-key",
            "home/agent-coord/research/telemetry.token",
            "home/agent-coord/research/claude-home/x",
            "proj/src/.git/config",
            "proj/lanes/base/box/run/ffprof/cookies",
            "proj/src/ssh-link/id",
        ] {
            let r = read_file(&at, &p(&dir, f), 0, None);
            assert!(r.as_ref().is_err_and(|e| e.contains("never served")), "{f}: {r:?}");
        }
        for d in ["home/.claude", "home/agent-coord/accounts", "proj/src/.git"] {
            assert!(list_dir(&at, &p(&dir, d), false).is_err(), "{d}");
        }
        // a coord's folder is listed, its route is marked; a file named route elsewhere is no key
        let l: Value = serde_json::from_str(&list_dir(&at, &p(&dir, "home/agent-coord/research"), false).unwrap()).unwrap();
        let route = l.as_array().unwrap().iter().find(|e| e["name"] == "route").expect("listed");
        assert_eq!(route["outside"], true, "{l}");
        assert!(read_file(&at, &p(&dir, "proj/src/route"), 0, None).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_comes_in_pieces_of_at_most_a_mebibyte_and_len_zero_is_its_stat() {
        let (dir, at) = scene("read");
        let f = p(&dir, "proj/src/alpha.txt");
        let v: Value = serde_json::from_str(&read_file(&at, &f, 3, Some(4)).unwrap()).unwrap();
        assert_eq!(v["size"], 10);
        assert!(v["modified"].as_u64().is_some());
        assert_eq!(base64::engine::general_purpose::STANDARD.decode(v["data"].as_str().unwrap()).unwrap(), b"3456");
        let v: Value = serde_json::from_str(&read_file(&at, &f, 0, Some(0)).unwrap()).unwrap();
        assert_eq!((v["size"].as_u64(), v["data"].as_str()), (Some(10), Some("")));
        let big = p(&dir, "proj/src/big.bin");
        std::fs::write(&big, vec![7u8; MAX_PIECE as usize + 10]).unwrap();
        let piece = |from: u64, len: Option<u64>| -> usize {
            let v: Value = serde_json::from_str(&read_file(&at, &big, from, len).unwrap()).unwrap();
            base64::engine::general_purpose::STANDARD.decode(v["data"].as_str().unwrap()).unwrap().len()
        };
        assert_eq!(piece(0, None), MAX_PIECE as usize);
        assert_eq!(piece(0, Some(u64::MAX)), MAX_PIECE as usize);
        assert_eq!(piece(MAX_PIECE, None), 10);
        assert_eq!(piece(MAX_PIECE + 10, None), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_chats_path_is_found_as_the_app_found_it_and_says_if_it_opens() {
        let (dir, at) = scene("locate");
        let deep = p(&dir, "proj/src/deep");
        let got = |r: Result<String, String>| -> (String, bool) {
            let v: Value = serde_json::from_str(&r.unwrap()).unwrap();
            (v["path"].as_str().unwrap().to_string(), v["readable"] == true)
        };
        // under a parent of the agent's folder
        assert_eq!(got(locate(&at, "alpha.txt", Some(&deep), &[])), (p(&dir, "proj/src/alpha.txt"), true));
        // under a search folder, by its ending
        assert_eq!(got(locate(&at, "deep/Main.hs", None, &[p(&dir, "proj")])), (p(&dir, "proj/src/deep/Main.hs"), true));
        // found, but not served: the app can say why
        assert_eq!(got(locate(&at, &p(&dir, "outside/secret"), None, &[])), (p(&dir, "outside/secret"), false));
        assert_eq!(got(locate(&at, "~/pics/shot.png", None, &[])), (p(&dir, "home/pics/shot.png"), true));
        assert!(locate(&at, "nowhere.rs", Some(&deep), &[p(&dir, "proj")]).is_err());
        // a search folder find would read as an expression, or a relative one, is refused
        assert!(locate(&at, "x", None, &["-delete".into()]).is_err_and(|e| e.contains("absolute")));
        assert!(locate(&at, "x", None, &["proj".into()]).is_err_and(|e| e.contains("absolute")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_sources_are_every_kind_and_call_in_order() {
        let (dir, at) = scene("sources");
        let v: Value = serde_json::from_str(&sources(&at)).unwrap();
        let ends = |k: &str| -> Vec<String> { v[k].as_array().unwrap().iter().map(|e| e["path"].as_str().unwrap().rsplit('/').next().unwrap().to_string()).collect() };
        assert_eq!(ends("kinds"), ["base.kind", "lib.kind", "a.kind", "link.kind"]);
        assert_eq!(ends("calls"), ["lib.call", "c.call"]);
        assert_eq!(v["kinds"][2]["text"], "kind a\n");
        assert_eq!(v["look"], Value::Null, "a project that is no flavour's: no look");
        // the flavour's look beside them, for the look's editor (N-866 p6, 2): its text; none while there is no file
        // yet; a link is not read
        let look = p(&dir, "look.toml");
        let at = Places { look: Some(look.clone()), ..at };
        let look_of = |at: &Places| serde_json::from_str::<Value>(&sources(at)).unwrap()["look"].clone();
        assert_eq!(look_of(&at), json!({"path": look, "text": null}));
        std::fs::write(&look, "# calm\n[colour]\nattention = \"#336699\"\n").unwrap();
        assert_eq!(look_of(&at)["text"], "# calm\n[colour]\nattention = \"#336699\"\n", "as written, its comment too");
        std::fs::remove_file(&look).unwrap();
        std::os::unix::fs::symlink(p(&dir, "proj/kinds/a.kind"), &look).unwrap();
        assert_eq!(look_of(&at)["text"], Value::Null, "a link is not read");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_edit_is_written_only_when_unchanged_and_only_in_the_projects_kinds_and_calls() {
        let (dir, at) = scene("edit");
        let a = p(&dir, "proj/kinds/a.kind");
        // refused in his words: the file by its folder and name, never where it sits on the machine (N-802)
        assert_eq!(edit_file(&at, &a, Some("kind other\n"), "x", "m"), Err("Not saved: kinds/a.kind changed since you opened it. Open the edit again to start from its new text.".into()));
        assert_eq!(edit_file(&at, &p(&dir, "outside/x.kind"), None, "x", "m"), Err("Not saved: outside/x.kind is outside the project's folder and its library.".into()));
        // no git here: written, not committed
        assert_eq!(edit_file(&at, &a, Some("kind a\n"), "kind a2\n", "m").unwrap(), "written (its folder is no git repository: not committed)");
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "kind a2\n");
        // a new call in the project's calls; not over one that exists
        let n = p(&dir, "proj/calls/n.call");
        assert!(edit_file(&at, &n, None, "call n()\n", "m").is_ok());
        assert!(edit_file(&at, &n, None, "call n()\n", "m").is_err());
        // the library has no calls folder: one is made, as the app's write made it
        assert!(edit_file(&at, &p(&dir, "lib/calls/m.call"), None, "call m()\n", "m").is_ok());
        assert!(edit_file(&at, &format!("{}/new/../../outside/y.kind", p(&dir, "lib")), None, "x", "m").is_err());
        assert!(!dir.join("outside/y.kind").exists());
        // agent-base's, another extension, and the outside are refused
        assert!(edit_file(&at, &p(&dir, "base/kinds/base.kind"), Some("kind b\n"), "x", "m").is_err());
        assert!(edit_file(&at, &p(&dir, "proj/src/alpha.txt"), Some("0123456789"), "x", "m").is_err());
        assert!(edit_file(&at, &p(&dir, "outside/x.kind"), None, "x", "m").is_err());
        // a link as the file is not written through; a new file under a linked folder lands where it points: refused
        assert!(edit_file(&at, &p(&dir, "proj/kinds/link.kind"), Some("kind t\n"), "kind evil\n", "m").is_err_and(|e| e.contains("is a link")));
        assert_eq!(std::fs::read_to_string(dir.join("outside/target.kind")).unwrap(), "kind t\n");
        assert!(edit_file(&at, &p(&dir, "lib/linked/new.kind"), None, "x", "m").is_err());
        assert!(!dir.join("outside/new.kind").exists());
        assert_eq!(std::fs::read_to_string(dir.join("base/kinds/base.kind")).unwrap(), "kind b\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_edit_in_a_repository_commits_that_file_alone() {
        let (dir, at) = scene("commit");
        let proj = dir.join("proj");
        let git = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(&proj).args(args).output().unwrap().status.success());
        git(&["init", "-q"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "user.email", "t@t"]);
        git(&["commit", "-q", "--allow-empty", "-m", "start"]);
        // something else staged stays staged, out of the edit's commit
        git(&["add", "--", "kinds/a.kind"]);
        let c = p(&dir, "proj/calls/c.call");
        let r = edit_file(&at, &c, Some("call c()\n"), "call c(x)\n", "call c: takes x").unwrap();
        assert!(r.starts_with("commit "), "{r}");
        let out = |args: &[&str]| String::from_utf8_lossy(&std::process::Command::new("git").arg("-C").arg(&proj).args(args).output().unwrap().stdout).trim().to_string();
        assert_eq!(out(&["log", "-1", "--format=%s"]), "call c: takes x");
        assert_eq!(out(&["show", "--name-only", "--format="]), "calls/c.call");
        assert_eq!(out(&["diff", "--cached", "--name-only"]), "kinds/a.kind");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
