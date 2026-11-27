//! The project's language servers (the project file's `[[lsp]]`), the files each one serves, and every symbol in
//! them under the name an agent writes: the language's own qualified name (`store::Store::open` in Rust,
//! `Ticket.Store.open` in Haskell), optionally after the file (`src/store.rs:Store::open`).

use crate::lsp::{self, Rng, Server};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// One `[[lsp]]` of the project file.
#[derive(Clone, Debug)]
pub struct ServerConf {
    pub lang: String,
    pub command: Vec<String>,
    /// File name patterns it serves (`*.rs`).
    pub files: Vec<String>,
    /// Folders or files kept out of the server (`skip`, relative to the root or absolute): their files are not
    /// opened or outlined, and a call on one is refused. A server checks what is opened in it, so this is what keeps
    /// it from checking them (paper-research, N-169 post 95: Tovrin's proofs and tests libraries).
    pub skip: Vec<String>,
    /// The Unix socket of a server the project runs for every agent (`socket`; N-101 post 145, item 7): symbols
    /// connects to it in place of starting `command`. A leading `$COORD` is the coord's folder.
    pub socket: Option<String>,
    /// The entry's own settings for its server (`options = { ... }`), given as is at `initialize` under the
    /// language's name (design.md 4.1: Agda's libraries file): symbols knows no language's settings.
    pub options: Option<serde_json::Value>,
    /// The commit's records (`records = { dir, toolchain }`; crate::records): read for the files the view has not
    /// changed, in place of the server.
    pub records: Option<crate::records::Conf>,
}

impl ServerConf {
    /// The socket's path, `$COORD` read from the environment.
    pub fn socket_path(&self) -> Option<PathBuf> {
        let s = self.socket.as_deref()?;
        Some(match s.strip_prefix("$COORD") {
            Some(rest) => PathBuf::from(format!("{}{rest}", std::env::var("COORD").unwrap_or_default())),
            None => PathBuf::from(s),
        })
    }
}

/// The project file: COORD's `project` names it (its first line), as coordd and roster read it; SYMBOLS_PROJECT
/// names one directly.
pub fn project_file() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("SYMBOLS_PROJECT") {
        return Ok(PathBuf::from(p));
    }
    let coord = std::env::var("COORD").context("neither SYMBOLS_PROJECT nor COORD is set: no project file")?;
    let first = std::fs::read_to_string(Path::new(&coord).join("project")).context("reading $COORD/project")?;
    let first = first.lines().next().unwrap_or("").trim().to_string();
    if first.is_empty() {
        bail!("$COORD/project names no project file");
    }
    Ok(PathBuf::from(first))
}

pub fn servers_of(file: &Path) -> Result<Vec<ServerConf>> {
    let text = std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    let v: toml::Value = toml::from_str(&text).with_context(|| format!("parsing {}", file.display()))?;
    let mut out = Vec::new();
    for s in v.get("lsp").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
        let lang = s.get("lang").and_then(|x| x.as_str()).ok_or_else(|| anyhow!("an [[lsp]] without lang"))?.to_string();
        let command: Vec<String> = s.get("command").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        let files: Vec<String> = s.get("files").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        let socket = match s.get("socket") {
            None => None,
            Some(toml::Value::String(p)) if !p.trim().is_empty() => Some(p.trim().to_string()),
            Some(_) => bail!("[[lsp]] {lang}: socket must be a path"),
        };
        if (command.is_empty() && socket.is_none()) || files.is_empty() {
            bail!("[[lsp]] {lang}: files, and a command or a socket, are needed");
        }
        let skip: Vec<String> = match s.get("skip") {
            None => Vec::new(),
            Some(toml::Value::Array(a)) => a.iter().map(|x| x.as_str().map(String::from).ok_or_else(|| anyhow!("[[lsp]] {lang}: skip holds a non-string"))).collect::<Result<_>>()?,
            Some(_) => bail!("[[lsp]] {lang}: skip must be a list of folders or files"),
        };
        let options = match s.get("options") {
            None => None,
            Some(t @ toml::Value::Table(_)) => Some(serde_json::to_value(t).with_context(|| format!("[[lsp]] {lang}: options"))?),
            Some(_) => bail!("[[lsp]] {lang}: options must be a table"),
        };
        let records = match s.get("records") {
            None => None,
            Some(r) => Some(crate::records::conf_of(r, &lang, file.parent().unwrap_or(Path::new(".")))?),
        };
        out.push(ServerConf { lang, command, files, skip, socket, options, records });
    }
    Ok(out)
}

/// A file name matches a pattern: `*` any run of characters, the rest literally.
pub fn matches(pat: &str, name: &str) -> bool {
    match pat.split_once('*') {
        None => pat == name,
        Some((a, b)) => name.len() >= a.len() + b.len() && name.starts_with(a) && matches_rest(b, &name[a.len()..]),
    }
}

fn matches_rest(pat: &str, name: &str) -> bool {
    match pat.split_once('*') {
        None => name.ends_with(pat),
        Some((a, b)) => (0..=name.len()).any(|i| name.is_char_boundary(i) && name[i..].starts_with(a) && matches_rest(b, &name[i + a.len()..])),
    }
}

/// Folders never walked: build outputs and version control.
const SKIP: &[&str] = &[".git", "target", "dist-newstyle", ".stack-work", "_build", "node_modules", ".conversations"];

/// SKIP's entries as paths under ROOT (absolute ones as they are).
pub fn skip_paths(root: &Path, skip: &[String]) -> Vec<PathBuf> {
    skip.iter().map(|s| root.join(s.trim_end_matches('/'))).collect()
}

/// Whether PATH is one of SKIP or lies under one (by whole components, never by name).
pub fn skipped(path: &Path, skip: &[PathBuf]) -> bool {
    skip.iter().any(|s| path.starts_with(s))
}

pub fn files_under(root: &Path, pats: &[String], skip: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if skipped(&p, skip) {
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if !SKIP.contains(&name.as_str()) && !name.starts_with('.') {
                    stack.push(p);
                }
            } else if pats.iter().any(|pat| matches(pat, &name)) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Tell view server S what differs now from REPOS' bases: each changed file whose bytes are not what it was last
/// told, each deleted path not yet told, and, as `same_as_base`, each path told before that no longer differs. FULL
/// gives all of it, replacing what it had (its first update, a reconnect, a base that moved). The paths sent as
/// changed and as gone, or None when there was nothing to send.
pub fn view_sync(s: &Server, repos: &[crate::view::Repo], full: bool) -> Result<Option<(Vec<PathBuf>, Vec<PathBuf>)>> {
    use crate::lsp::Sent;
    let (told, last) = s.view_sent();
    // a base that moved, or a repository more or less: the whole view again
    let full = full || last != repos;
    let mut changed = Vec::new();
    let mut deleted = Vec::new();
    let mut now = std::collections::HashSet::new();
    for r in repos {
        let (c, d) = crate::view::difference(r)?;
        for rel in c {
            let abs = r.root.join(&rel);
            let Ok(bytes) = std::fs::read(&abs) else { continue };
            now.insert(abs.clone());
            let h = lsp::hash_bytes(&bytes);
            if full || told.get(&abs) != Some(&Sent::Text(h)) {
                changed.push((abs, crate::view::entry(r, &rel, bytes), h));
            }
        }
        for rel in d {
            let abs = r.root.join(&rel);
            now.insert(abs.clone());
            if full || told.get(&abs) != Some(&Sent::Deleted) {
                deleted.push((abs, serde_json::json!({"repo": r.name, "path": rel})));
            }
        }
    }
    if !full {
        for p in told.keys().filter(|p| !now.contains(*p)) {
            if let Some((r, rel)) = crate::view::locate(repos, p) {
                deleted.push((p.clone(), serde_json::json!({"repo": r.name, "path": rel, "same_as_base": true})));
            }
        }
    }
    if !full && changed.is_empty() && deleted.is_empty() {
        return Ok(None);
    }
    let paths: (Vec<PathBuf>, Vec<PathBuf>) = (changed.iter().map(|(p, ..)| p.clone()).collect(), deleted.iter().map(|(p, _)| p.clone()).collect());
    let n = s.view_update(full, repos, changed, deleted)?;
    eprintln!("symbols: view/update{}: {} changed, {} gone; the server took {n}", if full { " (full)" } else { "" }, paths.0.len(), paths.1.len());
    Ok(Some(paths))
}

/// One symbol: where it is, its kind, and the segments of its full name.
#[derive(Clone, Debug)]
pub struct Sym {
    pub server: usize,
    pub file: PathBuf,
    pub full: Vec<String>,
    pub kind: u64,
    pub range: Rng,
    pub sel: Rng,
    pub depth: usize,
    /// The server's own word for its kind (agda-ls: `record`, `data`, `postulate`), where it gives one.
    pub kind_name: Option<String>,
    /// agda-ls's `agda.signatureRange`: the signature alone.
    pub signature: Option<Rng>,
    /// agda-ls's `agda.ranges`: the definition's pieces (its signature, then each run of its clauses), each its own.
    pub pieces: Vec<Rng>,
    /// The server's detail of a Haskell symbol: hs-ls gives a function's or field's type as written there.
    pub detail: Option<String>,
    /// Its type as Agda printed it at the commit (a record's; crate::records).
    pub typed: Option<String>,
    /// Read from the commit's record, not from a server's outline: its ranges are UTF-16.
    pub of_record: bool,
}

impl Sym {
    pub fn name(&self, lang: &str) -> String {
        self.full.join(sep(lang))
    }
    pub fn kind(&self) -> String {
        self.kind_name.clone().unwrap_or_else(|| kind_word(self.kind).to_string())
    }
}

pub fn sep(lang: &str) -> &'static str {
    if lang == "rust" { "::" } else { "." }
}

pub fn kind_word(k: u64) -> &'static str {
    match k {
        1 => "file",
        2 => "module",
        3 => "namespace",
        4 => "package",
        5 => "class",
        6 => "method",
        7 => "property",
        8 => "field",
        9 => "constructor",
        10 => "enum",
        11 => "interface",
        12 => "function",
        13 => "variable",
        14 => "constant",
        19 => "object",
        22 => "variant",
        23 => "struct",
        24 => "event",
        25 => "operator",
        26 => "type parameter",
        _ => "symbol",
    }
}

/// The module a file is, as name segments, before its symbols' own: Rust from the path under the crate's `src`
/// (lib.rs and main.rs are the crate's root, `a/mod.rs` and `a.rs` are `a`); Haskell and Agda from the module the
/// server's outline puts at the top (their `module` line).
fn module_of(lang: &str, root: &Path, file: &Path) -> Vec<String> {
    if lang != "rust" {
        return Vec::new();
    }
    // the nearest folder up with a Cargo.toml holds `src`
    let mut d = file.parent();
    while let Some(x) = d {
        if x.join("Cargo.toml").exists() {
            break;
        }
        if x == root {
            d = None;
            break;
        }
        d = x.parent();
    }
    let Some(krate) = d else { return Vec::new() };
    let Ok(rel) = file.strip_prefix(krate.join("src")) else { return Vec::new() };
    let mut segs: Vec<String> = rel.iter().map(|s| s.to_string_lossy().into_owned()).collect();
    if let Some(last) = segs.pop() {
        let stem = last.trim_end_matches(".rs").to_string();
        if !(segs.is_empty() && (stem == "lib" || stem == "main")) && stem != "mod" {
            segs.push(stem);
        }
    }
    segs
}

/// A Rust impl block's outline name as the segment it puts before its items: `impl Store` is `Store`, `impl
/// Display for Store` is `<Store as Display>`; generic arguments go.
fn impl_segment(name: &str) -> Option<String> {
    let rest = name.strip_prefix("impl")?;
    let rest = rest.trim_start();
    // impl<T> ...: drop the generics after impl
    let rest = if rest.starts_with('<') { skip_angles(rest).trim_start() } else { rest };
    // a type or trait as its last path segment, without its generic arguments (`fmt::Display` is `Display`)
    let strip = |s: &str| -> String {
        let s = s.trim();
        let s = match s.find('<') {
            Some(i) => s[..i].trim(),
            None => s.split_whitespace().next().unwrap_or(""),
        };
        s.rsplit("::").next().unwrap_or(s).to_string()
    };
    Some(match rest.split_once(" for ") {
        Some((tr, ty)) => format!("<{} as {}>", strip(ty), strip(tr)),
        None => strip(rest),
    })
}

fn skip_angles(s: &str) -> &str {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return &s[i + 1..];
                }
            }
            _ => {}
        }
    }
    s
}

/// The symbols of one file from the server's outline.
pub fn outline_syms(server: usize, lang: &str, root: &Path, file: &Path, outline: &Value) -> Vec<Sym> {
    let mut out = Vec::new();
    let base = module_of(lang, root, file);
    fn walk(v: &Value, prefix: &[String], depth: usize, server: usize, lang: &str, file: &Path, out: &mut Vec<Sym>) {
        let Some(arr) = v.as_array() else { return };
        for s in arr {
            let name = s.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            let kind = s.get("kind").and_then(Value::as_u64).unwrap_or(0);
            let children = s.get("children").cloned().unwrap_or(Value::Null);
            // a Haskell or Agda module at the top: its dotted name is the prefix of what is in it
            if lang != "rust" && depth == 0 && (kind == 1 || kind == 2) {
                let segs: Vec<String> = name.split('.').map(String::from).collect();
                walk(&children, &segs, 0, server, lang, file, out);
                continue;
            }
            // a Rust impl block names no symbol of its own: its items go under its type
            if lang == "rust" && name.starts_with("impl") {
                if let Some(seg) = impl_segment(&name) {
                    let mut p = prefix.to_vec();
                    p.push(seg);
                    walk(&children, &p, depth + 1, server, lang, file, out);
                    continue;
                }
            }
            // a Haskell data type's constructor of the same name: the type's full name with its kind told apart
            let mut full = prefix.to_vec();
            full.push(name.clone());
            out.push(Sym {
                server,
                file: file.to_path_buf(),
                full: full.clone(),
                kind,
                range: s.get("range").map(Rng::of).unwrap_or_default(),
                sel: s.get("selectionRange").map(Rng::of).unwrap_or_default(),
                depth,
                // agda-ls's `agda.kind`; before it, its detail held the kind word (now the type)
                kind_name: if lang == "agda" { s.pointer("/agda/kind").or_else(|| s.get("detail")).and_then(Value::as_str).map(String::from) } else { None },
                signature: s.pointer("/agda/signatureRange").map(Rng::of),
                pieces: s.pointer("/agda/ranges").and_then(Value::as_array).map(|a| a.iter().map(Rng::of).collect()).unwrap_or_default(),
                detail: if lang == "haskell" { s.get("detail").and_then(Value::as_str).map(String::from) } else { None },
                typed: None,
                of_record: false,
            });
            walk(&children, &full, depth + 1, server, lang, file, out);
        }
    }
    walk(outline, &base, 0, server, lang, file, &mut out);
    out
}

/// The servers in use and all the symbols in their files.
pub struct Index {
    pub root: PathBuf,
    pub confs: Vec<ServerConf>,
    pub servers: Vec<Option<Server>>,
    pub syms: Vec<Sym>,
    /// The files the last call wrote (an edit's act, main.rs `act`).
    pub written: Vec<PathBuf>,
    /// A server whose first load was still going on when a call had to answer (lsp's deadline): kept, its load
    /// waited on again by the next call that needs it.
    pub starting: Vec<Option<(Server, lsp::Mark, Vec<PathBuf>)>>,
    /// The files of a started server not yet outlined: a file is outlined when a call needs it (its name's file, a
    /// reference in it), all of them for a name without its file, and the rest one by one while no call is waiting
    /// (main.rs, `outline_next`).
    pub unread: Vec<Vec<PathBuf>>,
    /// Unread files whose outline failed behind the calls: not tried there again.
    pub not_behind: Vec<PathBuf>,
    /// Servers whose start failed behind the calls: not tried there again.
    pub not_started_behind: Vec<usize>,
    /// An edit whose check was still going on when its call answered: its report comes with the next call.
    pub edit_check: Option<crate::ops::EditCheck>,
    /// The view's repositories and their bases (design.md 4.1), read again before each call (a base may move); a
    /// test sets them. None: not in a git repository, so a server is given its files by didOpen.
    pub repos: Option<Vec<crate::view::Repo>>,
    /// Whether `repos` is set by hand (tests) and not read from the environment.
    pub repos_fixed: bool,
    /// Per server, the names its files declare in Agda's `variable` blocks, read once at the first `definition` that
    /// needs them (`generalised`): a signature using one is generalised over it by Agda, so its type is the server's.
    pub variables: Vec<Option<std::collections::HashSet<String>>>,
    /// Per server, what the commit's records give this view (`[[lsp]] records`; crate::records).
    pub recs: Vec<Option<RecState>>,
}

/// A server's records for the view: the commit read for its bases, and what the view has changed from them.
pub struct RecState {
    pub bases: Vec<crate::view::Repo>,
    /// None: no manifest of this build for these bases (every file is then the server's).
    pub commit: Option<crate::records::Commit>,
    /// The paths that differ from the bases now (changed, added, deleted); None when git could not say, and then no
    /// record is used.
    pub changed: Option<std::collections::HashSet<PathBuf>>,
    /// The files whose symbols are read from their records now.
    pub loaded: std::collections::HashSet<PathBuf>,
    /// The modules that import a changed one: their records are as of the commit.
    pub stale: std::collections::HashSet<String>,
}

impl Index {
    pub fn new(root: PathBuf, confs: Vec<ServerConf>) -> Index {
        let n = confs.len();
        Index { root, confs, servers: (0..n).map(|_| None).collect(), syms: Vec::new(), written: Vec::new(), starting: (0..n).map(|_| None).collect(), unread: vec![Vec::new(); n], not_behind: Vec::new(), not_started_behind: Vec::new(), edit_check: None, repos: None, repos_fixed: false, variables: vec![None; n], recs: (0..n).map(|_| None).collect() }
    }

    /// The view's repositories now (crate::view::repos), kept in `repos`; an error is logged once a call, and the
    /// last known ones kept.
    pub fn view_repos(&mut self) -> Option<Vec<crate::view::Repo>> {
        if !self.repos_fixed {
            match crate::view::repos(&self.root) {
                Ok(r) => self.repos = Some(r),
                Err(e) => eprintln!("symbols: the view's repositories: {e:#}"),
            }
        }
        self.repos.clone()
    }

    /// Tell view server I what differs now from the bases (FULL: all of it, replacing what it had). The paths sent
    /// as changed and as gone, or None when nothing was to be sent.
    pub fn view_sync(&mut self, i: usize, full: bool) -> Result<Option<(Vec<PathBuf>, Vec<PathBuf>)>> {
        let Some(repos) = self.view_repos() else { return Ok(None) };
        let s = match (&self.servers[i], &self.starting[i]) {
            (Some(s), _) => s,
            (None, Some((s, ..))) => s,
            _ => return Ok(None),
        };
        view_sync(s, &repos, full)
    }

    /// The server for files like PATH, started (and its files opened) on first use, and PATH outlined.
    pub fn server_for(&mut self, path: &Path) -> Result<usize> {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let i = self.confs.iter().position(|c| c.files.iter().any(|p| matches(p, &name))).ok_or_else(|| anyhow!("no [[lsp]] in the project file serves {name}"))?;
        let abs = if path.is_absolute() { path.to_path_buf() } else { self.root.join(path) };
        if skipped(&abs, &skip_paths(&self.root, &self.confs[i].skip)) {
            bail!("{} is in a folder the project file keeps out of the {} server ([[lsp]] skip): the symbol tools do not read or change it, so that the server never checks it", self.rel(&abs), self.confs[i].lang);
        }
        if let Some(why) = self.unchecked(&abs) {
            lsp::note(format!("the commit's build did not check {} ({why}): it is the {} server's, which checks it now", self.rel(&abs), self.confs[i].lang));
        }
        self.start(i)?;
        self.outlined(i, &abs)?;
        Ok(i)
    }

    /// FILE outlined, if server I has not outlined it yet.
    pub fn outlined(&mut self, i: usize, file: &Path) -> Result<()> {
        if let Some(k) = self.unread[i].iter().position(|f| f == file) {
            self.reindex(i, file)?;
            self.unread[i].remove(k);
        }
        Ok(())
    }

    /// Each of FILES a started server serves and has not outlined yet, outlined (the references' files: a reference
    /// is said with the symbol it is in).
    pub fn outline_files(&mut self, files: &[&Path]) -> Result<()> {
        for f in files {
            for i in 0..self.confs.len() {
                if self.servers[i].is_some() {
                    self.outlined(i, f)?;
                }
            }
        }
        Ok(())
    }

    /// Every file of server I outlined.
    fn outline_all(&mut self, i: usize) -> Result<()> {
        while let Some(f) = self.unread[i].last().cloned() {
            self.reindex(i, &f)?;
            self.unread[i].pop();
        }
        Ok(())
    }

    /// A file not yet outlined that is to be outlined behind the calls: server I's and the file.
    fn next_behind(&self) -> Option<(usize, PathBuf)> {
        (0..self.confs.len()).filter(|&i| self.servers[i].as_ref().is_some_and(|s| !s.closed())).find_map(|i| self.unread[i].iter().rev().find(|f| !self.not_behind.contains(f)).map(|f| (i, f.clone())))
    }

    /// Whether a started server has files to outline behind the calls.
    pub fn has_unread(&self) -> bool {
        self.next_behind().is_some()
    }

    /// A server not started yet that is to be started behind the calls.
    fn next_start(&self) -> Option<usize> {
        // one the commit's records serve is started only by a call that needs it: a file they do not give
        (0..self.confs.len()).find(|&i| self.servers[i].is_none() && !self.not_started_behind.contains(&i) && !self.recs[i].as_ref().is_some_and(|r| r.commit.is_some()))
    }

    /// Whether there is work to do behind the calls: a server to start, a file to outline.
    pub fn has_behind(&self) -> bool {
        self.next_start().is_some() || self.has_unread()
    }

    /// One server not started yet, started (or its first load waited on again), while no call waits; false when
    /// none was left to start. One whose start fails is logged and not started behind the calls again (a call that
    /// needs it starts it, and says the error); one cut by a cancel (a call came) is kept as it is (`starting`).
    pub fn start_next(&mut self) -> bool {
        for i in 0..self.confs.len() {
            if self.recs[i].is_none() && self.servers[i].is_none() {
                self.refresh_records(i);
            }
        }
        let Some(i) = self.next_start() else { return false };
        match self.start(i) {
            Ok(()) => eprintln!("symbols: the {} server started behind the calls; {} files to outline", self.confs[i].lang, self.unread[i].len()),
            Err(_) if lsp::cancelled() => {}
            Err(e) => {
                eprintln!("symbols: starting the {} server behind the calls: {e:#}", self.confs[i].lang);
                self.not_started_behind.push(i);
            }
        }
        true
    }

    /// One file not yet outlined, outlined (while no call waits: main.rs). A file whose outline fails is logged and
    /// not tried again behind the calls (it stays unread: a call that needs it outlines it, and says the error); one
    /// cut by a cancel (a call came) stays to be outlined.
    pub fn outline_next(&mut self) {
        let Some((i, f)) = self.next_behind() else { return };
        match self.reindex(i, &f) {
            Ok(()) => {
                self.unread[i].retain(|x| x != &f);
                if self.unread[i].is_empty() {
                    eprintln!("symbols: every {} file outlined", self.confs[i].lang);
                }
            }
            Err(_) if lsp::cancelled() => {}
            Err(e) => {
                eprintln!("symbols: outlining {} behind the calls: {e:#}", self.rel(&f));
                self.not_behind.push(f);
            }
        }
    }

    /// Server I started or connected to, its files opened and its first load waited on; its files are outlined
    /// as they are needed (`outlined`), not here: outlining a file is checking it (agda-ls), and a first call for
    /// one file would wait on the whole project's.
    pub fn start(&mut self, i: usize) -> Result<()> {
        self.gone(i);
        if self.servers[i].is_none() {
            self.refresh_records(i);
            let (s, since, files) = match self.starting[i].take() {
                Some(st) => st,
                None => {
                    let c = self.confs[i].clone();
                    let sock = c.socket_path();
                    let how = match &sock {
                        Some(p) => lsp::How::Socket(p),
                        None => lsp::How::Command(&c.command),
                    };
                    let repos = self.view_repos();
                    let mut extra = serde_json::json!({"skip": c.skip});
                    if let Some(r) = &repos {
                        extra["view"] = crate::view::init(r, &c.skip);
                    }
                    if let Some(o) = &c.options {
                        extra[c.lang.as_str()] = o.clone();
                    }
                    let s = Server::begin(&c.lang, &c.lang, how, &self.root, &extra)?;
                    // from its start: its first indexing may begin and end before Server::start returns
                    let since = lsp::Mark::default();
                    let files = files_under(&self.root, &c.files, &skip_paths(&self.root, &c.skip));
                    if s.view {
                        // the view as data: the bases at initialize, the difference now (design.md 4.1)
                        view_sync(&s, repos.as_deref().unwrap_or_default(), true)?;
                    } else {
                        // every file it serves is opened: a server checks (and reports on) the files open in it
                        for f in &files {
                            if let Ok(t) = std::fs::read_to_string(f) {
                                s.open(f, &t)?;
                            }
                        }
                    }
                    (s, since, files)
                }
            };
            // the first load: the server's own indexing. A call cut at the client's limit or interrupted leaves it
            // running, kept here for the next call
            match s.settle(since, Duration::from_millis(1500)) {
                Ok(_) => {}
                // cut by a cancel (the session interrupted, or a call that came while it started behind the calls), the
                // load goes on in the server: kept, as at the deadline
                Err(e) if lsp::still_working(&e).is_some() || lsp::cancelled() => {
                    self.starting[i] = Some((s, since, files));
                    return Err(e);
                }
                Err(e) => return Err(e),
            }
            self.servers[i] = Some(s);
            // a file the commit's records give is read from its record, not outlined by the server
            let files: Vec<PathBuf> = files.into_iter().filter(|f| self.backed(i, f).is_none()).collect();
            self.unread[i] = files.into_iter().rev().collect();
        }
        Ok(())
    }

    /// A server whose output has ended (a shared server restarted, a child that died) is dropped with what came from
    /// it, so that this call starts or connects to it again; the call's notes say so.
    fn gone(&mut self, i: usize) {
        let ended = self.servers[i].as_ref().is_some_and(Server::closed) || self.starting[i].as_ref().is_some_and(|(s, ..)| s.closed());
        if !ended {
            return;
        }
        let lang = self.confs[i].lang.clone();
        let shared = self.servers[i].as_ref().or(self.starting[i].as_ref().map(|(s, ..)| s)).and_then(|s| s.socket().map(|p| p.display().to_string()));
        let what = match shared {
            Some(p) => format!("the shared {lang} server's connection ({p}) had closed (the server was restarted?): connected again, and its files read again"),
            None => format!("the {lang} server had ended: started again, and its files read again"),
        };
        eprintln!("symbols: {what}");
        lsp::note(what);
        self.servers[i] = None;
        self.starting[i] = None;
        self.unread[i].clear();
        self.syms.retain(|x| x.server != i);
        if self.edit_check.as_ref().is_some_and(|c| c.waits_on(i)) {
            // its marks are the old server's: the edit's errors come from the new one's check of the files
            self.edit_check = None;
            lsp::note(format!("the errors of the last edit were not in when the {lang} server went: they come from its new check"));
        }
    }

    /// Every server started and every file outlined, for a name without its file (the languages of those this
    /// started); a server with the commit's records reads the files of the
    /// names in their table that NAME ends from their records, and is started (and outlines) only for the files they
    /// do not give (changed in the view, or not checked at the commit).
    fn start_all_for(&mut self, name: &str) -> Result<Vec<String>> {
        let segs: Vec<String> = if name.contains("::") { name.split("::").map(String::from).collect() } else { split_dots(name) };
        let mut now = Vec::new();
        for i in 0..self.confs.len() {
            let Some(c) = self.recs[i].as_ref().and_then(|r| r.commit.as_ref()) else {
                if self.servers[i].is_none() || self.servers[i].as_ref().is_some_and(Server::closed) {
                    self.start(i)?;
                    now.push(self.confs[i].lang.clone());
                }
                self.outline_all(i)?;
                continue;
            };
            // a name exported from another module (Haskell): the files of the name it stands for as well
            let alias: Vec<String> = c.exports.iter().filter(|(a, _)| ends_with(&split_dots(c.bare(a)), &segs)).map(|(_, o)| c.bare(o).to_string()).collect();
            let wanted = |n: &crate::records::Name| n.instance.is_none() && (ends_with(&split_dots(c.bare(&n.full)), &segs) || alias.iter().any(|o| c.bare(&n.full) == o));
            let files: Vec<PathBuf> = c.names.iter().filter(|n| wanted(n)).filter_map(|n| c.module_file.get(c.module(n)).cloned()).collect();
            for f in files {
                self.load_backed(i, &f)?;
            }
            let c = &self.confs[i];
            let rest: Vec<PathBuf> = files_under(&self.root, &c.files, &skip_paths(&self.root, &c.skip)).into_iter().filter(|f| self.backed(i, f).is_none()).collect();
            let unchecked: Vec<String> = rest.iter().filter_map(|f| self.unchecked(f).map(|why| format!("{} ({why})", self.rel(f)))).collect();
            if !unchecked.is_empty() {
                lsp::note(format!("the commit's build did not check {} file(s), so the {} server outlines them now: {}", unchecked.len(), self.confs[i].lang, unchecked.join("; ")));
            }
            if !rest.is_empty() {
                if self.servers[i].is_none() || self.servers[i].as_ref().is_some_and(Server::closed) {
                    self.start(i)?;
                    now.push(self.confs[i].lang.clone());
                }
                self.outline_all(i)?;
            }
        }
        Ok(now)
    }

    /// The server whose files are like PATH's, started or not.
    pub fn serving(&self, path: &Path) -> Option<usize> {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.confs.iter().position(|c| c.files.iter().any(|p| matches(p, &name)))
    }

    /// Whether server I is started.
    pub fn started(&self, i: usize) -> bool {
        self.servers[i].is_some()
    }

    /// Whether S's ranges count UTF-8 bytes (a server that agreed to them); a record's count UTF-16.
    pub fn enc(&self, s: &Sym) -> bool {
        !s.of_record && self.servers[s.server].as_ref().is_some_and(|x| x.utf8)
    }

    /// Server I's records read again for the view's bases if they moved, and what differs from them now: a file
    /// read from its record and since changed goes back to the server.
    pub fn refresh_records(&mut self, i: usize) {
        let Some(conf) = self.confs[i].records.clone() else { return };
        let Some(repos) = self.view_repos() else { return };
        if !self.recs[i].as_ref().is_some_and(|r| r.bases == repos) {
            // other bases: the records read are another commit's
            if self.recs[i].take().is_some() {
                self.syms.retain(|s| !(s.server == i && s.of_record));
            }
            let main = crate::view::main_repo().unwrap_or_default();
            let commit = match crate::records::open(&conf, &repos, &main, self.confs[i].lang == "haskell") {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("symbols: the commit's records: {e:#}");
                    None
                }
            };
            match &commit {
                Some(c) => eprintln!("symbols: the commit's records ({}): {} files, {} names, read from {}", conf.toolchain, c.files.len(), c.names.len(), c.sets.join(", ")),
                None => eprintln!("symbols: no records of {} for {}: the {} server's, as before", conf.toolchain, crate::records::sets(&repos, &main).join(", "), self.confs[i].lang),
            }
            self.recs[i] = Some(RecState { bases: repos.clone(), commit, changed: None, loaded: Default::default(), stale: Default::default() });
        }
        let st = self.recs[i].as_mut().unwrap();
        let Some(c) = &st.commit else { return };
        let mut changed = std::collections::HashSet::new();
        let mut known = true;
        for r in &repos {
            match crate::view::difference(r) {
                Ok((ch, del)) => changed.extend(ch.iter().chain(del.iter()).map(|x| r.root.join(x))),
                Err(e) => {
                    eprintln!("symbols: what the view changed in {}: {e:#}; no record is used", r.name);
                    known = false;
                }
            }
        }
        let gone: Vec<PathBuf> = st.loaded.iter().filter(|f| !known || changed.contains(*f)).cloned().collect();
        for f in &gone {
            st.loaded.remove(f);
        }
        let modules: std::collections::HashSet<String> = changed.iter().filter_map(|f| c.files.get(f).map(|e| e.module.clone())).collect();
        st.stale = c.importers(&modules);
        st.changed = known.then_some(changed);
        for f in gone {
            self.syms.retain(|s| s.file != f);
            if self.servers[i].is_some() && !self.unread[i].contains(&f) {
                self.unread[i].push(f);
            }
        }
    }

    /// FILE's record key, when server I's records give it: the commit has it checked, its record is of this build,
    /// and the view has not changed it.
    pub fn backed(&self, i: usize, file: &Path) -> Option<String> {
        let st = self.recs.get(i)?.as_ref()?;
        let c = st.commit.as_ref()?;
        if st.changed.as_ref()?.contains(file) {
            return None;
        }
        let k = c.files.get(file)?.record.as_ref().ok()?;
        c.has_record(k).then(|| k.clone())
    }

    /// FILE's symbols from its record, if server I's records give it and they are not read yet; whether they give it.
    pub fn load_backed(&mut self, i: usize, file: &Path) -> Result<bool> {
        if self.recs[i].as_ref().is_some_and(|r| r.loaded.contains(file)) {
            return Ok(true);
        }
        let Some(k) = self.backed(i, file) else { return Ok(false) };
        let st = self.recs[i].as_mut().unwrap();
        let c = st.commit.as_ref().unwrap();
        let rec = c.record(&k)?;
        let syms = crate::records::record_syms(i, file, &rec, c);
        st.loaded.insert(file.to_path_buf());
        self.syms.retain(|s| s.file != file);
        self.syms.extend(syms);
        Ok(true)
    }

    /// FILE's server, when its records give FILE: its symbols read from the record (the records read again for the
    /// view first). None: the server's, as before.
    pub fn from_record(&mut self, file: &Path) -> Result<Option<usize>> {
        let Some(i) = self.serving(file) else { return Ok(None) };
        self.refresh_records(i);
        Ok(self.load_backed(i, file)?.then_some(i))
    }

    /// Why the commit's build did not check FILE, from its manifest; None for a file it checked or does not have.
    pub fn unchecked(&self, file: &Path) -> Option<String> {
        let i = self.serving(file)?;
        let st = self.recs[i].as_ref()?;
        st.commit.as_ref()?.files.get(file)?.record.as_ref().err().cloned()
    }

    /// Each of FILES its server's records give, read from its record.
    pub fn load_files(&mut self, files: &[&Path]) -> Result<()> {
        for f in files {
            if let Some(i) = self.serving(f) {
                self.load_backed(i, f)?;
            }
        }
        Ok(())
    }

    /// Whether S, read from its record, is of a module that imports a file the view has changed: as of the commit.
    pub fn stale(&self, s: &Sym) -> bool {
        let Some(st) = self.recs[s.server].as_ref() else { return false };
        s.of_record && st.commit.as_ref().and_then(|c| c.files.get(&s.file)).is_some_and(|e| st.stale.contains(&e.module))
    }

    /// The uses of S as the commit's uses index and records give them (each place, and the files the view changed,
    /// whose uses are found by name); None when server S's records do not know S's name.
    pub fn record_uses(&mut self, s: &Sym) -> Result<Option<(Vec<(PathBuf, Value)>, Vec<PathBuf>)>> {
        let bare = s.full.join(".");
        let type_ns = is_type_ns(s);
        let Some(st) = self.recs[s.server].as_mut() else { return Ok(None) };
        let Some(changed) = st.changed.clone() else { return Ok(None) };
        let Some(c) = st.commit.as_mut() else { return Ok(None) };
        // Haskell's names are UNIT:Module.name, a constructor's and a field's too (the outline puts them under their
        // type): S's file's module's
        let q = match c.files.get(&s.file).filter(|_| c.haskell) {
            Some(e) => format!("{}.{}", e.module, s.full.last().cloned().unwrap_or_default()),
            None => bare.clone(),
        };
        if !c.names.iter().any(|n| *n.full == *q) {
            // a name new in a changed file: the server's
            return Ok(None);
        }
        let mut out = Vec::new();
        for k in c.users(&q)? {
            let Some(f) = c.file_of_key(&k).cloned() else { continue };
            if changed.contains(&f) {
                continue;
            }
            for (r, d, ty) in c.record(&k)?.sites {
                if d == q && (!c.haskell || ty == type_ns) {
                    let pos = |l: u32, ch: u32| serde_json::json!({"line": l, "character": ch});
                    out.push((f.clone(), serde_json::json!({"start": pos(r.start_line, r.start_char), "end": pos(r.end_line, r.end_char)})));
                }
            }
        }
        out.sort_by_key(|(f, r)| (f.clone(), r.pointer("/start/line").and_then(Value::as_u64), r.pointer("/start/character").and_then(Value::as_u64)));
        let conf = &self.confs[s.server];
        let mut by_name: Vec<PathBuf> = changed.into_iter().filter(|f| f.exists() && f.file_name().is_some_and(|n| conf.files.iter().any(|p| matches(p, &n.to_string_lossy())))).collect();
        by_name.sort();
        Ok(Some((out, by_name)))
    }

    pub fn server(&self, i: usize) -> &Server {
        self.servers[i].as_ref().expect("started")
    }

    /// FILE's symbols afresh from its outline.
    pub fn reindex(&mut self, i: usize, file: &Path) -> Result<()> {
        let s = self.server(i);
        let o = s.request("textDocument/documentSymbol", serde_json::json!({"textDocument": {"uri": lsp::uri(file)}}))?;
        let lang = self.confs[i].lang.clone();
        let syms = outline_syms(i, &lang, &self.root, file, &o);
        self.syms.retain(|x| x.file != file);
        self.syms.extend(syms);
        Ok(())
    }

    pub fn rel(&self, p: &Path) -> String {
        p.strip_prefix(&self.root).unwrap_or(p).to_string_lossy().into_owned()
    }

    /// Whether TYPE, a signature's type as written in server I's language, uses a name its files declare as a
    /// generalised `variable` without binding it: then Agda adds it as an implicit argument, and only the server
    /// gives the type as it is (`{@0 σ : Sizes} → …`). The names are read from the files once.
    pub fn generalised(&mut self, i: usize, ty: &str) -> Vec<String> {
        if self.variables[i].is_none() {
            let c = &self.confs[i];
            let mut names = std::collections::HashSet::new();
            for f in files_under(&self.root, &c.files, &skip_paths(&self.root, &c.skip)) {
                if let Ok(t) = std::fs::read_to_string(&f) {
                    names.extend(variables_in(&t));
                }
            }
            self.variables[i] = Some(names);
        }
        let names = self.variables[i].as_ref().unwrap();
        free_variables(ty, names)
    }

    pub fn lang(&self, s: &Sym) -> &str {
        &self.confs[s.server].lang
    }

    /// The symbol a name stands for (see the module's head). None found and several found are errors that say what
    /// was found: the near names, or each candidate under the full name that picks it out.
    pub fn resolve(&mut self, written: &str) -> Result<Sym> {
        let (file, name) = split_file(written);
        // only the servers the name needs: a file's language's, or (a bare name) all of them, the refusal saying so
        for i in 0..self.confs.len() {
            self.refresh_records(i);
        }
        let note = match file {
            Some(f) => {
                let abs = self.root.join(f);
                match self.serving(&abs).filter(|&i| self.backed(i, &abs).is_some()) {
                    // a file the view has not changed: its record, and no server
                    Some(i) => {
                        self.load_backed(i, &abs)?;
                    }
                    None => {
                        self.server_for(&abs)?;
                    }
                }
                String::new()
            }
            None => {
                let now = self.start_all_for(name)?;
                let all: Vec<&str> = self.confs.iter().map(|c| c.lang.as_str()).collect();
                let now = if now.is_empty() { "none, all were running".to_string() } else { now.join(", ") };
                format!("\n(a name without its file: searched in every server, {}; started for it: {now})", all.join(", "))
            }
        };
        self.find(written, file, name).map_err(|e| anyhow!("{e}{note}"))
    }

    fn find(&self, written: &str, file: Option<&str>, name: &str) -> Result<Sym> {
        let segs: Vec<String> = if name.contains("::") { name.split("::").map(String::from).collect() } else { split_dots(name) };
        let segs: Vec<String> = segs.into_iter().filter(|s| !s.is_empty()).collect();
        if segs.is_empty() {
            bail!("no symbol named: {written:?}");
        }
        let file = file.map(|f| self.root.join(f));
        let cands: Vec<&Sym> = self
            .syms
            .iter()
            .filter(|s| file.as_ref().is_none_or(|f| &s.file == f))
            .filter(|s| s.full.len() >= segs.len() && s.full[s.full.len() - segs.len()..] == segs[..])
            // a Haskell instance's methods (its record's children under `instance HEAD`) only when written with it
            .filter(|s| !(s.of_record && self.lang(s) == "haskell" && s.full[..s.full.len() - segs.len()].iter().any(|x| x.starts_with("instance "))))
            .collect();
        if cands.is_empty() && file.is_none() {
            // a name exported from a module not its own (Haskell): the one it stands for
            for st in self.recs.iter().flatten() {
                let Some(c) = &st.commit else { continue };
                if let Some((a, o)) = c.exports.iter().find(|(a, _)| split_dots(c.bare(a)) == segs) {
                    let o = c.bare(o).to_string();
                    let s = self.find(&o, None, &o)?;
                    lsp::note(format!("{} is exported from {}; it is {} there", c.bare(a), c.bare(a).rsplit_once('.').map_or("", |x| x.0), o));
                    return Ok(s);
                }
            }
        }
        // a name written whole picks its symbol out of those it ends (`Store.Store`, the type, not the constructor
        // `Store.Store.Store`): the picking names a refusal lists are then never refused themselves
        let whole: Vec<&Sym> = cands.iter().copied().filter(|s| s.full == segs).collect();
        let cands = if whole.len() == 1 { whole } else { cands };
        match cands.len() {
            1 => Ok(cands[0].clone()),
            0 => {
                // a name the commit defines in a file the view has changed, no longer in that file's outline
                let mut gone = Vec::new();
                for st in self.recs.iter().flatten() {
                    let (Some(c), Some(ch)) = (&st.commit, &st.changed) else { continue };
                    for n in c.names.iter().filter(|n| n.instance.is_none() && ends_with(&split_dots(c.bare(&n.full)), &segs)) {
                        if let Some(f) = c.module_file.get(c.module(n)).filter(|f| ch.contains(*f)) {
                            gone.push(format!("{} ({}) was defined in {} at the commit (line {}); that file has changed since, and it no longer defines it", c.bare(&n.full), c.kind(n), self.rel(f), n.range.start_line + 1));
                        }
                    }
                }
                if !gone.is_empty() {
                    bail!("no symbol {written}: {}", gone.join("; "));
                }
                let last = segs.last().unwrap();
                let mut near: Vec<(usize, String)> = self.syms.iter().filter_map(|s| Some((closeness(last, s.full.last()?)?, s))).map(|(c, s)| (c, format!("{}  ({}, {})", s.name(self.lang(s)), s.kind(), self.rel(&s.file)))).collect();
                for st in self.recs.iter().flatten() {
                    let Some(c) = &st.commit else { continue };
                    for n in c.names.iter().filter(|n| n.instance.is_none()) {
                        let Some(k) = split_dots(c.bare(&n.full)).last().and_then(|l| closeness(last, l)) else { continue };
                        let at = c.module_file.get(c.module(n)).map(|f| self.rel(f)).unwrap_or_else(|| c.module(n).to_string());
                        near.push((k, format!("{}  ({}, {at})", c.bare(&n.full), c.kind(n))));
                    }
                }
                near.sort();
                near.dedup_by(|a, b| a.1 == b.1);
                let near: Vec<String> = near.into_iter().take(5).map(|(_, n)| n).collect();
                // with its file the name is looked for in that file only: the near names are of the files outlined so far
                let unread: usize = self.unread.iter().map(Vec::len).sum();
                let among = if unread > 0 { format!(" (near names from the files outlined so far; {unread} are not yet: the name without its file searches them all)") } else { String::new() };
                if near.is_empty() {
                    bail!("no symbol {written}")
                } else {
                    bail!("no symbol {written}; near it{among}:\n  {}", near.join("\n  "))
                }
            }
            _ => {
                let mut lines: Vec<String> = cands.iter().map(|s| self.picking_name(s, file.is_some())).collect();
                lines.sort();
                bail!("{written} names {} symbols; nothing was done. Write one of:\n  {}", cands.len(), lines.join("\n  "))
            }
        }
    }

    /// The full name that picks S out, with the file in front when the full name alone does not, or when the name
    /// was written with its file (WITH_FILE: the other files may not be outlined, so the full name alone may not
    /// pick it out).
    fn picking_name(&self, s: &Sym, with_file: bool) -> String {
        let lang = self.lang(s);
        let n = s.name(lang);
        let same = self.syms.iter().filter(|o| o.full == s.full).count();
        if same > 1 || with_file {
            let in_file = self.syms.iter().filter(|o| o.full == s.full && o.file == s.file).count();
            if in_file > 1 {
                return format!("{}:{n}  ({}, line {}; the same name twice in one file: edit it as text)", self.rel(&s.file), s.kind(), line_of(&s.sel) + 1);
            }
            format!("{}:{n}  ({})", self.rel(&s.file), s.kind())
        } else {
            format!("{n}  ({}, {})", s.kind(), self.rel(&s.file))
        }
    }

    /// The innermost symbol whose range holds a position of FILE (for saying where a reference is).
    pub fn enclosing(&self, file: &Path, line: u64) -> Option<&Sym> {
        self.syms.iter().filter(|s| s.file == file && line_of(&s.range) <= line && line <= end_line_of(&s.range)).max_by_key(|s| s.depth)
    }
}

/// The names an Agda file declares in its `variable` blocks (`variable`, `private variable`, a block's lines
/// indented under it: `@0 sc : Scoping`, `x y : Nat`).
pub fn variables_in(text: &str) -> Vec<String> {
    let indent = |l: &str| l.len() - l.trim_start().len();
    let mut out = Vec::new();
    let mut under: Option<usize> = None;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("--") {
            continue;
        }
        if let Some(at) = under {
            if indent(line) > at {
                if let Some((names, _)) = t.split_once(" : ") {
                    out.extend(names.split_whitespace().filter(|n| !n.starts_with('@')).map(|n| n.trim_matches(|c| c == '{' || c == '}').to_string()).filter(|n| !n.is_empty()));
                }
                continue;
            }
            under = None;
        }
        if t.split_whitespace().last() == Some("variable") {
            under = Some(indent(line));
        }
    }
    out
}

/// Whether Agda type TY uses one of NAMES without binding it in TY itself (`{x : A} →`, `(x : A) →`, `∀ x →`).
#[cfg(test)]
pub fn uses_free(ty: &str, names: &std::collections::HashSet<String>) -> bool {
    !free_variables(ty, names).is_empty()
}

/// The NAMES Agda type TY uses without binding them in TY itself, in the order of their first use.
pub fn free_variables(ty: &str, names: &std::collections::HashSet<String>) -> Vec<String> {
    let mut toks = Vec::new();
    let mut cur = String::new();
    for c in ty.chars() {
        if c.is_whitespace() || "(){}⦃⦄;".contains(c) {
            if !cur.is_empty() {
                toks.push(std::mem::take(&mut cur));
            }
            if !c.is_whitespace() {
                toks.push(c.to_string());
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        toks.push(cur);
    }
    let mut bound = std::collections::HashSet::new();
    for (i, t) in toks.iter().enumerate() {
        let opens = matches!(t.as_str(), "(" | "{" | "⦃" | "∀" | "forall");
        if opens {
            let names: Vec<&String> = toks[i + 1..].iter().take_while(|x| !matches!(x.as_str(), ":" | "→" | "->" | "." | ")" | "}" | "⦄")).collect();
            let ends = toks.get(i + 1 + names.len()).map(String::as_str);
            if matches!(t.as_str(), "∀" | "forall") || ends == Some(":") {
                bound.extend(names.into_iter().filter(|n| !n.starts_with('@')).cloned());
            }
        }
    }
    let mut out: Vec<String> = Vec::new();
    for t in &toks {
        if names.contains(t) && !bound.contains(t) && !out.contains(t) {
            out.push(t.clone());
        }
    }
    out
}

/// The type of an Agda signature SIG as written (`name : TYPE`, maybe over several lines), its continuation lines
/// without their indentation.
pub fn written_type(sig: &str) -> Option<String> {
    let at = sig.find(" : ").or_else(|| sig.find(" :\n"))?;
    let ty = sig[at + 2..].trim();
    (!ty.is_empty()).then(|| ty.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n"))
}

/// How close the name N of a symbol is to the name ASKED that no symbol has, for the near names a refusal lists (less
/// is closer), or none if it is not near: the same but for case, then one beginning with the other, then one holding
/// the other, each by how many characters they differ in length; then a few edits apart (a typo). A name of one or
/// two characters, or under half the other's length, is near only by case or a typo: a local `n` or `r` is held in
/// nearly every name, and listed them first (paper-research N-101 p1565: every miss on Tovrin-dev listed
/// `Debug.Along.followStep.n`).
fn closeness(asked: &str, n: &str) -> Option<usize> {
    let (a, b) = (asked.to_lowercase(), n.to_lowercase());
    let apart = a.len().abs_diff(b.len());
    // one held in the other counts only for a name of 3 or more characters and at least half the other's length: in
    // `unconfirmedText` a local `n` is held, and `text` and `Conf` (Tovrin's `productClaim.text`, `Semantics.Conf`)
    let (lo, hi) = (a.chars().count().min(b.chars().count()), a.chars().count().max(b.chars().count()));
    let short = lo < 3 || 2 * lo < hi;
    if a == b {
        return Some(0);
    }
    if !short && (a.starts_with(&b) || b.starts_with(&a)) {
        return Some(1_000 + apart);
    }
    if !short && (a.contains(&b) || b.contains(&a)) {
        return Some(2_000 + apart);
    }
    let d = edits(&a, &b);
    (d <= 2.max(a.chars().count() / 4)).then_some(3_000 + d)
}

/// The edits (a character put in, taken out or changed) from A to B.
fn edits(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for j in 0..b.len() {
            let here = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != b[j])).min(row[j] + 1).min(here + 1);
            prev = here;
        }
    }
    row[b.len()]
}

pub fn line_of<R: crate::lsp::Ends + ?Sized>(r: &R) -> u64 {
    r.start().0
}

pub fn end_line_of<R: crate::lsp::Ends + ?Sized>(r: &R) -> u64 {
    r.end().0
}

/// `path/to/file.ext:Name` into its file and name; a Rust `::` is not a file's colon.
fn split_file(w: &str) -> (Option<&str>, &str) {
    let b = w.as_bytes();
    for (i, c) in w.char_indices() {
        if c == ':' && b.get(i + 1) != Some(&b':') && (i == 0 || b[i - 1] != b':') {
            let f = &w[..i];
            if f.contains('.') && !f.contains(' ') {
                return (Some(f), &w[i + 1..]);
            }
        }
    }
    (None, w)
}

/// Dotted segments; an operator (`Store.<+>`, all symbol characters) stays one segment.
/// Whether S is in Haskell's type namespace (a type, class, type synonym or family), not a value's.
pub fn is_type_ns(s: &Sym) -> bool {
    matches!(s.kind_name.as_deref(), Some("data" | "newtype" | "type" | "class" | "type family" | "data family" | "type synonym")) || (s.kind_name.is_none() && matches!(s.kind, 5 | 10 | 11 | 23 | 26))
}

/// The instances that implement Haskell class method S, from the commit's names table: each `instance HEAD` with
/// its place.
pub fn instances_of(ix: &Index, s: &Sym) -> Vec<String> {
    let Some(c) = ix.recs[s.server].as_ref().and_then(|r| r.commit.as_ref()).filter(|c| c.haskell) else { return Vec::new() };
    let Some(e) = c.files.get(&s.file) else { return Vec::new() };
    let q = format!("{}.{}", e.module, s.full.last().cloned().unwrap_or_default());
    let mut out: Vec<String> = c
        .names
        .iter()
        .filter(|n| n.instance.is_some() && *n.full == *q)
        .map(|n| {
            let at = c.module_file.get(c.module(n)).map(|f| ix.rel(f)).unwrap_or_else(|| c.bare(c.module(n)).to_string());
            format!("{} ({at}:{})", n.instance.as_deref().unwrap_or(""), n.range.start_line + 1)
        })
        .collect();
    out.sort();
    out
}

/// Whether FULL ends with SEGS.
fn ends_with(full: &[String], segs: &[String]) -> bool {
    full.len() >= segs.len() && full[full.len() - segs.len()..] == segs[..]
}

fn split_dots(n: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for (i, c) in n.char_indices() {
        if c == '.' && !cur.is_empty() && cur.chars().all(|x| x.is_alphanumeric() || x == '_' || x == '\'') && n[i + 1..].chars().next().is_some_and(|x| x.is_alphanumeric() || x == '_' || !x.is_ascii()) {
            out.push(std::mem::take(&mut cur));
        } else if c == '.' && !cur.is_empty() && cur.chars().all(|x| x.is_alphanumeric() || x == '_' || x == '\'') {
            // `Module.<+>`: the rest is an operator
            out.push(std::mem::take(&mut cur));
            out.push(n[i + 1..].to_string());
            return out;
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_split_as_written() {
        assert_eq!(split_file("src/store.rs:Store::open"), (Some("src/store.rs"), "Store::open"));
        assert_eq!(split_file("store::Store::open"), (None, "store::Store::open"));
        assert_eq!(split_dots("Ticket.Store.open"), vec!["Ticket", "Store", "open"]);
        assert_eq!(split_dots("Store.<+>"), vec!["Store", "<+>"]);
        assert_eq!(impl_segment("impl Store").as_deref(), Some("Store"));
        assert_eq!(impl_segment("impl Display for Store").as_deref(), Some("<Store as Display>"));
        assert_eq!(impl_segment("impl<T> From<T> for Wrap<T>").as_deref(), Some("<Wrap as From>"));
        assert_eq!(impl_segment("impl fmt::Display for Store").as_deref(), Some("<Store as Display>"));
        assert!(matches("*.rs", "lib.rs") && !matches("*.rs", "lib.hs") && matches("*.lagda.md", "A.lagda.md"));
    }

    /// The near names of a name no symbol has are the closest ones, not the one-letter locals every name holds (the
    /// miss `Interface.Engine.Exported.unconfirmedText` on Tovrin-dev listed `Debug.Along.followStep.n` first).
    #[test]
    fn a_miss_lists_the_closest_names() {
        let conf = ServerConf { lang: "agda".into(), command: vec![], files: vec!["*.agda".into()], skip: vec![], socket: None, options: None, records: None };
        let mut ix = Index::new(PathBuf::from("/v"), vec![conf]);
        let sym = |file: &str, full: &[&str]| Sym { server: 0, file: PathBuf::from("/v").join(file), full: full.iter().map(|s| s.to_string()).collect(), kind: 12, range: Rng::default(), sel: Rng::default(), depth: full.len() - 1, kind_name: None, signature: None, pieces: vec![], detail: None, typed: None, of_record: false };
        ix.syms = vec![
            sym("agda/core/Debug/Along.agda", &["Debug", "Along", "followStep", "n"]),
            sym("agda/core/Debug/Session.agda", &["Debug", "Session", "symRegistered", "r"]),
            sym("agda/protocol/Interface/Core/Derive.agda", &["Interface", "Core", "Derive", "apartSound", "n"]),
            sym("agda/interface/Interface/Engine/Exported.agda", &["Interface", "Engine", "Exported", "confirmedText"]),
            sym("agda/interface/Interface/Engine/Exported.agda", &["Interface", "Engine", "Exported", "unconfirmed"]),
            sym("agda/interface/Interface/Engine/Exported.agda", &["Interface", "Engine", "Exported", "unconfirmedTexts"]),
            sym("agda/core/Top/Class.agda", &["Top", "Class", "undecidedText"]),
            sym("agda/core/Top/Text.agda", &["Top", "Text", "unconfirmedtext"]),
            sym("agda/interface/Interface/Engine/Product/Claim.agda", &["Interface", "Engine", "Product", "Claim", "productClaim", "text"]),
            sym("agda/interface/Interface/System/Semantics.agda", &["Interface", "System", "Semantics", "Conf"]),
        ];
        let e = ix.find("Interface.Engine.Exported.unconfirmedText", None, "Interface.Engine.Exported.unconfirmedText").unwrap_err().to_string();
        let listed: Vec<&str> = e.lines().skip(1).map(|l| l.trim().split("  ").next().unwrap()).collect();
        assert_eq!(listed, ["Top.Text.unconfirmedtext", "Interface.Engine.Exported.unconfirmedTexts", "Interface.Engine.Exported.unconfirmed", "Interface.Engine.Exported.confirmedText"], "{e}");
        assert!(!e.contains("followStep.n") && !e.contains("symRegistered.r") && !e.contains("productClaim.text") && !e.contains("Semantics.Conf"), "{e}");
        // a typo is near; a name nothing like it is not listed at all
        assert!(ix.find("undecidedTxet", None, "undecidedTxet").unwrap_err().to_string().contains("Top.Class.undecidedText"));
        assert_eq!(ix.find("zzqqxx", None, "zzqqxx").unwrap_err().to_string(), "no symbol zzqqxx");
    }

    /// The names of a file's `variable` blocks; a type uses one when it is not bound in the type itself; a written
    /// signature's type over several lines.
    #[test]
    fn a_type_is_generalised_only_over_a_variable_it_does_not_bind() {
        let file = "module M where\n\nprivate variable\n  @0 sc : Scoping\n  @0 σ  : Sizes\n  x y : Nat\n\nf : Nat\nf = 0\n";
        let mut names: Vec<String> = variables_in(file);
        names.sort();
        assert_eq!(names, ["sc", "x", "y", "σ"]);
        let vars: std::collections::HashSet<String> = names.into_iter().collect();
        assert!(uses_free("DeadSteps → View sc (Term Var) → Bool", &vars));
        assert!(uses_free("Evaluable σ → PairSeen", &vars));
        assert!(!uses_free("{t : Type} → View sc' t → List t", &vars), "sc' is another name");
        assert!(!uses_free("{@0 sc : Scoping} → View sc Term → Bool", &vars), "bound in the type");
        assert!(!uses_free("(x : Nat) → ∀ y → Vec x y", &vars));
        assert!(!uses_free("Opening → Nat → Product → Check Bool", &vars));
        assert_eq!(free_variables("Evaluable σ → View sc t → σ", &vars), vec!["σ".to_string(), "sc".to_string()]);
        assert_eq!(written_type("productSinkShown : Opening → Nat →\n    Product → Check Bool").as_deref(), Some("Opening → Nat →\nProduct → Check Bool"));
        assert_eq!(written_type("_+_ : Nat → Nat → Nat").as_deref(), Some("Nat → Nat → Nat"));
        assert_eq!(written_type("no colon here"), None);
    }

    /// A range kept as four numbers gives the same lines and offsets as the server's JSON it was read from.
    #[test]
    fn a_kept_range_spans_what_the_servers_range_does() {
        let v = serde_json::json!({"start": {"line": 1, "character": 2}, "end": {"line": 2, "character": 3}});
        let r = Rng::of(&v);
        assert_eq!(r, Rng { start_line: 1, start_char: 2, end_line: 2, end_char: 3 });
        let text = "ab\ncdéf\nghij\n";
        for utf8 in [false, true] {
            assert_eq!(lsp::span(text, &r, utf8), lsp::span(text, &v, utf8));
        }
        assert_eq!((line_of(&r), end_line_of(&r)), (line_of(&v), end_line_of(&v)));
        assert_eq!(Rng::of(&serde_json::Value::Null), Rng::default());
    }

    /// A skip list keeps its folders' files out (by whole path components: `agda/core/Kernel/Proof` is not under
    /// `agda/proofs`), relative or absolute.
    #[test]
    fn skipped_folders_are_not_listed() {
        let root = std::env::temp_dir().join(format!("symbols-skip-{}", std::process::id()));
        for f in ["agda/proofs/P.agda", "agda/tests/T.agda", "agda/core/Kernel/Proof/B.agda", "agda/core/C.agda", "agda/proofsx/X.agda"] {
            let p = root.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, "").unwrap();
        }
        let skip = super::skip_paths(&root, &["agda/proofs/".to_string(), root.join("agda/tests").display().to_string()]);
        let got: Vec<String> = super::files_under(&root, &["*.agda".to_string()], &skip).iter().map(|p| p.strip_prefix(&root).unwrap().display().to_string()).collect();
        assert_eq!(got, vec!["agda/core/C.agda", "agda/core/Kernel/Proof/B.agda", "agda/proofsx/X.agda"]);
        assert!(super::skipped(&root.join("agda/proofs/P.agda"), &skip));
        assert!(!super::skipped(&root.join("agda/proofsx/X.agda"), &skip));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A shared server (`socket`): symbols connects to it, tells it the skip list, the root and the layer in
    /// `initialize`, opens no skipped file, and after the server is restarted connects again on the next call.
    #[test]
    fn a_shared_server_is_connected_to_and_again_after_its_restart() {
        let root = std::env::temp_dir().join(format!("symbols-sock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for f in ["src/A.agda", "proofs/P.agda"] {
            let p = root.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, "module X where\n").unwrap();
        }
        let sock = root.join("agda.sock");
        let log = root.join("server.log");
        let fake = root.join("fake.py");
        std::fs::write(&fake, r#"
import sys, json, socket, os, threading
path, log = sys.argv[1], sys.argv[2]
def note(m):
    with open(log, "a") as f: f.write(json.dumps(m) + "\n")
def serve(c):
    r = c.makefile("rb")
    def send(m):
        b = json.dumps(m).encode(); c.sendall(b"Content-Length: %d\r\n\r\n" % len(b) + b)
    while True:
        n = None
        while True:
            l = r.readline()
            if not l: c.close(); return
            l = l.strip()
            if not l and n is not None: break
            if l.lower().startswith(b"content-length:"): n = int(l.split(b":")[1])
        m = json.loads(r.read(n)); k = m.get("method")
        if k == "initialize": note({"init": m["params"]["initializationOptions"]}); send({"jsonrpc": "2.0", "id": m["id"], "result": {"capabilities": {}}})
        elif k == "textDocument/didOpen": note({"open": m["params"]["textDocument"]["uri"]})
        elif k == "exit": note({"exit": 1}); c.close(); return
        elif "id" in m: send({"jsonrpc": "2.0", "id": m["id"], "result": [] if k == "textDocument/documentSymbol" else None})
if os.path.exists(path): os.unlink(path)
s = socket.socket(socket.AF_UNIX); s.bind(path); s.listen()
while True:
    c, _ = s.accept(); threading.Thread(target=serve, args=(c,), daemon=True).start()
"#).unwrap();
        let run = || {
            let _ = std::fs::remove_file(&sock);
            let c = std::process::Command::new("python3").arg(&fake).arg(&sock).arg(&log).spawn().unwrap();
            for _ in 0..200 {
                if sock.exists() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            c
        };
        let lines = || -> Vec<Value> { std::fs::read_to_string(&log).unwrap_or_default().lines().map(|l| serde_json::from_str(l).unwrap()).collect() };
        // SAFETY: no other test reads or sets COORD_LAYER or ROSTER_VIEW
        unsafe {
            std::env::set_var("COORD_LAYER", "E-3.l1");
            std::env::set_var("ROSTER_VIEW", "4242");
        }
        // the fake servers are ended even when an assertion fails
        struct Ends(Vec<std::process::Child>);
        impl Drop for Ends {
            fn drop(&mut self) {
                for c in &mut self.0 {
                    let _ = c.kill();
                    let _ = c.wait();
                }
            }
        }
        let mut ends = Ends(Vec::new());
        ends.0.push(run());
        let conf = ServerConf { lang: "agda".into(), command: Vec::new(), files: vec!["*.agda".into()], skip: vec!["proofs".into()], socket: Some(sock.display().to_string()), options: None, records: None };
        let mut ix = Index::new(root.clone(), vec![conf]);
        ix.repos_fixed = true;
        ix.start(0).unwrap();
        let l = lines();
        let inits: Vec<&Value> = l.iter().filter_map(|v| v.get("init")).collect();
        assert_eq!(inits.len(), 1);
        assert_eq!(inits[0]["skip"], serde_json::json!(["proofs"]));
        assert_eq!(inits[0]["layer"], "E-3.l1");
        assert_eq!(inits[0]["viewPid"], 4242);
        assert_eq!(inits[0]["root"], root.display().to_string());
        let opened: Vec<&str> = l.iter().filter_map(|v| v.get("open").and_then(Value::as_str)).collect();
        assert!(opened.len() == 1 && opened[0].ends_with("src/A.agda"), "{opened:?}");

        // the server restarted: the connection closes, and the next start connects again
        ends.0[0].kill().unwrap();
        let _ = ends.0[0].wait();
        for _ in 0..200 {
            if ix.servers[0].as_ref().unwrap().closed() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        ends.0.push(run());
        lsp::begin_call(None);
        ix.start(0).unwrap();
        assert!(lsp::take_notes().iter().any(|n| n.contains("connected again")));
        assert_eq!(lines().iter().filter(|v| v.get("init").is_some()).count(), 2);

        // the end: shutdown and exit, the server itself left running
        drop(ix);
        assert!(lines().iter().any(|v| v.get("exit").is_some()));
        assert!(ends.0[1].try_wait().unwrap().is_none(), "the shared server must not be ended by a client");
        drop(ends);
        let _ = std::fs::remove_dir_all(&root);
    }
}
