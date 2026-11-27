//! The project file (design §3): a path on the machine, named in `$COORD/project`. coordd reads from it what it needs:
//! the item prefix, rights by level and the view sources (what an agent waits on comes from the tickets). The view kinds, columns and cards are
//! yaaihh's to draw; coordd passes each source's JSON on as it comes.

use serde::Deserialize;
use serde_json::Value;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Project {
    #[serde(default)]
    pub project: Header,
    #[serde(default, rename = "view")]
    pub views: Vec<View>,
    /// The language servers (`[[lsp]]`, symbols' table): coordd reads only `serve` and `socket` in them.
    #[serde(default, rename = "lsp")]
    pub lsps: Vec<Lsp>,
    /// Where the file is: sources run in its folder.
    #[serde(skip)]
    pub dir: String,
    /// The whole file as JSON, for yaaihh.
    #[serde(skip)]
    pub raw: Value,
    /// `[project] repos`' names in the file's own order, which is the order they were added in from Home
    /// ([`repo_order`]; `repos` itself is kept by name): what [`Project::repos`] goes by, so the first added is where
    /// agents start when `repo` names none (paper-research N-1446 p3387, 2).
    #[serde(skip)]
    pub order: Vec<String>,
    /// Limits by agent name (`[limits.runner]`, `memory`, `cpu`, `pids` as the `box_` keys take them): on agent
    /// NAME's own cgroup `agents/NAME`, so on its jobs too (the runner's: one pool for everything CI runs). Written
    /// when the agent starts and when coordd starts; for a boxed agent over `box_memory`, `box_cpu`, `box_pids`.
    #[serde(default)]
    pub limits: std::collections::BTreeMap<String, crate::sandbox::Limits>,
    /// `[read.NAME]` (N-101 post 406; the joint proposal's F): folders agent NAME reads beside `box_read`, read-only
    /// like them; for the agents the owner talks to, which may need more of the machine than a step's agent.
    /// `[read.orchestrator] folders = ["~/Documents/papers/sigcomm/notes"]`.
    #[serde(default)]
    pub read: std::collections::BTreeMap<String, Read>,
}

/// `[read.NAME]`: the folders, `~/` for the owner's home.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Read {
    #[serde(default)]
    pub folders: Vec<String>,
}

/// Where the item notes are read and written (`Project::notes`).
#[derive(Clone, Debug, PartialEq)]
pub enum Notes {
    /// A plain folder, written in place.
    Dir(String),
    /// Folder REL of repository NAME (at REPO), as branch ROOT's head has it.
    Line { name: String, repo: String, root: String, rel: String },
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Header {
    /// The folder of the item notes: `coord item open` writes ID-SLUG.md there (design §4). A folder inside one of
    /// `repos`, in a project with a line (`root`), is read from the line's head and written by a commit handed in
    /// (`Project::notes`).
    #[serde(default)]
    pub notes: Option<String>,
    /// A command printing the project's events, one JSON object per line as they happen, each naming its `event`
    /// (vault designs/supervision-nets-2026-11-10.md, step 1). coordd raises each to the nets; one with
    /// `"notify": "owner"` is also mailed to the owner, once per `fingerprint` an hour.
    #[serde(default)]
    pub events: Option<String>,
    /// How a check step reaches the project's runner (Docket's `check` kind, plan/delta-m613.md): a command run in the
    /// project file's folder as `RUNNER check TICKET --to AGENT REPO:COMMIT`; its verdict comes back as a `check_end`
    /// event naming the ticket.
    #[serde(default)]
    pub runner: Option<String>,
    /// The model proxy's request log folder (one file per request), a folder on the host: `~/` (the app's home) or
    /// absolute. The raw pane's source: the app reads it in place, never copied (yaaihh proxylog.rs, N-40); coordd
    /// only says what is written here.
    #[serde(default)]
    pub proxy_log: Option<String>,
    /// Which engine answers the policy's acts (policy/interp.rs `Mode`): `clingo` (the default), `both` (norms.rs answers,
    /// eflint-rs is asked beside it and differences are journalled) or `eflint`. Read by `interp::Mode::of`.
    #[serde(default)]
    pub norms: Option<String>,
    /// Whether what is due to a host session ($COORD/hosts/NAME) is written to the feed for coord-relay
    /// (`Core::hand_hosts`). Off unless the file says `host_relay = true`: production's relay reads every
    /// ~/agent-coord/*/feed.jsonl, so a dev or scratch coord must never write a relay line (paper-research m1058); the
    /// cut-over switches it on for the project that has host sessions.
    #[serde(default)]
    pub host_relay: bool,
    /// Whether an agent's ask (the ask tool) waits for its answer in the asker's turn, holding back what else is due
    /// meanwhile (`block_ask = true`; plan/design-two-systems.md, the ask). Off: every ask is a question ticket.
    #[serde(default)]
    pub block_ask: bool,
    /// The project's build output folders, relative to its path (`outputs = ["target"]`): in a layer's view each is
    /// bound from a folder outside the layer, so what a build writes is in no layer (plan/layers.md I §6, test 5;
    /// coordd's layer.rs).
    #[serde(default)]
    pub outputs: Vec<String>,
    /// The line's built outputs, a folder as `repo` takes it (`warm = "../runner/last"`): it holds NAME/ for each of
    /// `outputs` and a file `commit` naming the commit they were built from. A layer's outputs start from them
    /// (copy-on-write, coordd's warm.rs) instead of empty. Unset, or nothing there: empty, as before.
    #[serde(default)]
    pub warm: Option<String>,
    /// The project's folder, a git repository: relative to the project file's folder, absolute, or `~/`; with `repos`,
    /// also one of their names (`Project::repo`). A `layer`'s
    /// view is mounted at it; a fresh agent runs in it (main.rs `fresh_dir`). Unset: no layer can be made (NoLayer),
    /// and a fresh agent's folder is guessed as before (N-89.p4).
    #[serde(default)]
    pub repo: Option<String>,
    /// A project of several repositories (N-101): each by NAME, its folder as `repo` takes it (`repos = { tovrin =
    /// "../tovrin", p4 = "../p4" }`). A layer is one workspace over all of them: each repository its own part of the
    /// view at its own path, on the branch `layer/ID` in each. `repo` names the one agents start in; unset, the first
    /// in the file, the first added from Home (`Project::order`; it was the first by name before p3387). A repository inside another is mounted on the outer one's view, and must be one the outer one ignores;
    /// one that is not, or a folder that is no repository's top, refuses the project file at load.
    #[serde(default)]
    pub repos: std::collections::BTreeMap<String, String>,
    /// The agents' root branch (the owner: the one branch marked as the "llm" root): a `layer` without `on` starts at
    /// its head as it is when the layer starts. Unset: such a layer raises NoLayer.
    #[serde(default)]
    pub root: Option<String>,
    /// The light sandbox (coordd's sandbox.rs): `box = true` runs each agent made for a step in a layer as a Linux user
    /// of its own. Unset: `box` in `yaaihh.toml`, else off.
    #[serde(default, rename = "box")]
    pub boxed: bool,
    /// The default Safe · Yolo of the project's Claude Code agents on the host (`agent_mode = "yolo"`): the mode of
    /// an agent that has no mode of its own (`Core::resolve_mode`). Unset: the yaaihh-wide default, else Safe.
    #[serde(default)]
    pub agent_mode: Option<String>,
    /// A boxed agent's memory limit, as its cgroup's `memory.max` takes it (`"4G"`); no swap beside it. Unset: none.
    #[serde(default)]
    pub box_memory: Option<String>,
    /// A boxed agent's process limit (`pids.max`). Unset: none.
    #[serde(default)]
    pub box_pids: Option<u64>,
    /// A boxed agent's CPU share, in CPUs (`cpu.max`). Unset: none.
    #[serde(default)]
    pub box_cpu: Option<f64>,
    /// What an agent is told to do when a process of its box was killed at `box_memory`, after the fact itself
    /// ("a build of that size runs as a step of the runner (`coord check`)"). Unset: the fact alone.
    #[serde(default)]
    pub box_memory_hint: Option<String>,
    /// Folders under the owner's home a boxed agent reads (a toolchain), bound read-only: `~/` or absolute. Nothing
    /// else of the home gets in but the coord's folder, agent-base and Claude Code.
    #[serde(default)]
    pub box_read: Vec<String>,
    /// A script a boxed agent runs in its box, as itself, in its view, each time it starts, before its session
    /// (`box_setup = "./box-setup.sh"`, as `runner` takes a path): what its own home needs that the owner's has (git's
    /// and cabal's configuration, links to the toolchain's data). Copied into the box when coordd builds it
    /// (`sandbox::home`), so it need not lie where the box can read. It fails: the agent does not start.
    #[serde(default)]
    pub box_setup: Option<String>,
    /// Folders put first on every agent's PATH (`path = ["~/.ghcup/bin"]`, `~/` or absolute), before what the
    /// starting shell had: the project's toolchain, whoever starts the agent. coordd writes them to `$COORD/path`;
    /// roster reads that file.
    #[serde(default)]
    pub path: Vec<String>,
    /// The whole project's memory limit, as `memory.max` takes it (`"40G"`): everything coordd runs for it together
    /// (its agents, their jobs, the runner's, the terminal holders, the view sources, coordd itself), as a
    /// container's `--memory` did. Never above coordd's own cap of 90% of the machine. Unset: that cap alone.
    #[serde(default)]
    pub memory: Option<String>,
    /// The whole project's CPU share, in CPUs (`cpu.max`), as a container's `--cpus`. Unset: none.
    #[serde(default)]
    pub cpu: Option<f64>,
    /// The whole project's process limit (`pids.max`). Unset: none.
    #[serde(default)]
    pub pids: Option<u64>,
    /// The library the project stands on (N-101; tickets-patterns plan/library for now, agent-base's later): a folder
    /// of kinds and calls read between agent-base's and the project's, so the project's own replace its names
    /// (`library = "PATH"`, as `repo` takes it). Unset: none.
    #[serde(default)]
    pub library: Option<String>,
}

/// One `[[lsp]]`, as coordd reads it (paper-research, N-101 posts 145 and 156): a server the project runs once for
/// every agent. `serve` is its command (no shell; `$COORD` in a word is the coord's folder), `socket` where it
/// listens, the same path every agent's symbols connects to (`$COORD/lsp/agda.sock`). coordd starts it at its own
/// start, in the project's cgroup, and again when it ends; an entry without `serve` is symbols' alone.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Lsp {
    #[serde(default)]
    pub lang: String,
    #[serde(default)]
    pub serve: Vec<String>,
    #[serde(default)]
    pub socket: Option<String>,
}

impl Lsp {
    /// WORD with `$COORD` as COORD.
    pub fn expand(word: &str, coord: &str) -> String {
        word.replace("$COORD", coord)
    }
}

/// Run shared server L once, in the project file's folder DIR: the socket's stale file removed first and its folder
/// made; the server coordd's child in the project's cgroup, its log in coordd's journal, signalled when coordd ends
/// (PR_SET_PDEATHSIG). Its socket is opened to every user once it is there (an agent may run as a uid of its own,
/// as coordd's socket is). Returns how it ended.
pub async fn run_server(dir: String, coord: String, l: Lsp) -> String {
    let Some(socket) = l.socket.as_deref().map(|s| Lsp::expand(s, &coord)) else {
        return format!("[[lsp]] {}: serve without a socket", l.lang);
    };
    let words: Vec<String> = l.serve.iter().map(|w| Lsp::expand(w, &coord)).collect();
    let Some((prog, args)) = words.split_first() else { return format!("[[lsp]] {}: an empty serve", l.lang) };
    if let Some(d) = std::path::Path::new(&socket).parent() {
        if let Err(e) = std::fs::create_dir_all(d) {
            return format!("[[lsp]] {}: {}: {e}", l.lang, d.display());
        }
    }
    let _ = std::fs::remove_file(&socket);
    let mut command = Command::new(prog);
    command.args(args).current_dir(&dir).env("COORD", &coord).stdin(Stdio::null()).kill_on_drop(true);
    // SAFETY: prctl is async-signal-safe; nothing else runs between fork and exec.
    unsafe {
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => return format!("[[lsp]] {}: starting {prog}: {e}", l.lang),
    };
    let pid = child.id().unwrap_or_default();
    let _ = crate::cgroup::place(pid as i32, "project");
    eprintln!("coordd: the shared {} server started (pid {pid}): {}", l.lang, words.join(" "));
    let (lang, sock) = (l.lang.clone(), socket.clone());
    let opened = tokio::spawn(async move {
        use std::os::unix::fs::PermissionsExt;
        // a cold server may index before it listens: looked for, for as long as it runs
        loop {
            if std::fs::metadata(&sock).is_ok() {
                let _ = std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o666));
                return eprintln!("coordd: the shared {lang} server listens on {sock}");
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });
    let status = child.wait().await.map(|s| s.to_string()).unwrap_or_else(|e| e.to_string());
    opened.abort();
    format!("the shared {} server (pid {pid}) ended: {status}", l.lang)
}

#[derive(Clone, Debug, Deserialize)]
pub struct View {
    pub id: String,
    #[serde(default)]
    pub source: Option<String>,
    /// A command printing the view's part before $BEFORE once (`Request::Older`), for a view that holds only its
    /// recent part.
    #[serde(default)]
    pub older: Option<String>,
}

/// The folders kinds and calls are read from, in order, a later one's name replacing an earlier one's: agent-base
/// (BASE), the project's library when its file in COORD names one, and the project's folder (PROJECT).
pub fn folders(coord: &str, base: &str, project: Option<&str>) -> Vec<String> {
    let mut out = vec![base.to_string()];
    if let Some(dir) = project {
        out.extend(library(coord, dir));
        out.push(dir.to_string());
    }
    out
}

/// Where FOLDER keeps its SUB (`kinds`, `calls`): FOLDER/SUB, or FOLDER itself when it has neither a `kinds` nor a
/// `calls` folder (plan/library and Tovrin's program on it are flat: lib.kind and lib.call side by side).
pub fn place(folder: &str, sub: &str) -> String {
    let has = |s: &str| std::path::Path::new(&format!("{folder}/{s}")).is_dir();
    if has("kinds") || has("calls") { format!("{folder}/{sub}") } else { folder.to_string() }
}

/// The library named by the project file COORD names, when that file is in DIR: as a path on the machine.
fn library(coord: &str, dir: &str) -> Option<String> {
    let path = std::fs::read_to_string(format!("{coord}/project")).ok()?;
    let path = path.trim();
    let p = Project {
        project: toml::from_str::<Project>(&std::fs::read_to_string(path).ok()?).ok()?.project,
        dir: std::path::Path::new(path).parent()?.to_string_lossy().to_string(),
        ..Default::default()
    };
    (p.dir == dir).then(|| p.project.library.as_deref().map(|l| p.path(l))).flatten()
}

impl Project {
    /// The project's folder (`repo`, else the first of `repos`), as a path on the machine. `repo` is one of `repos`
    /// by its name (`repo = "tovrin"`) or a folder as `repo` takes it: a name of `repos` wins (yaaihh N-169 post 129:
    /// the trial's `repo = "tovrin"` was looked for as `files/tovrin`).
    pub fn repo(&self) -> Option<String> {
        match self.project.repo.as_deref() {
            Some(r) if self.project.repos.contains_key(r.trim()) => Some(self.path(&self.project.repos[r.trim()])),
            Some(r) => Some(self.path(r)),
            None => self.repos().into_iter().next().map(|(_, p)| p),
        }
    }

    /// The line's built outputs (`warm`), as a path on the machine.
    pub fn warm(&self) -> Option<String> {
        self.project.warm.as_deref().map(|w| self.path(w))
    }

    /// `repos` in the file's order ([`Project::order`]; any it does not hold after them, by name), as paths on the
    /// machine.
    /// Where the item notes are (N-101 post 406; the joint proposal's J, N-101 post 357): `notes` as a plain folder, or,
    /// in a project with a line (`root`) when the folder lies in one of `repos`, that repository's folder REL as the
    /// line's head has it. There a note is read from the line, and a new one is a commit on the line's head handed in
    /// through the runner like any hand-in, so the runner stays the one way onto the line (items.rs).
    pub fn notes(&self) -> Option<Notes> {
        let dir = self.path(self.project.notes.as_deref()?);
        let line = self.project.root.clone().and_then(|root| {
            let (name, repo) = self.repos().into_iter().find(|(_, r)| dir.strip_prefix(r.as_str()).is_some_and(|rest| rest.starts_with('/')))?;
            let rel = dir[repo.len() + 1..].trim_end_matches('/').to_string();
            Some(Notes::Line { name, repo, root, rel })
        });
        Some(line.unwrap_or(Notes::Dir(dir)))
    }

    pub fn repos(&self) -> Vec<(String, String)> {
        let r = &self.project.repos;
        let named = self.order.iter().filter(|n| r.contains_key(n.as_str()));
        let rest = r.keys().filter(|n| !self.order.contains(n));
        named.chain(rest).map(|n| (n.clone(), self.path(&r[n]))).collect()
    }

    /// The name among `repos` of the project's own folder (`repo`), and the others by name: what a layer is over.
    pub fn layer_repos(&self) -> (String, Vec<(String, String)>) {
        let main = self.repo().unwrap_or_default();
        let (mine, others): (Vec<_>, Vec<_>) = self.repos().into_iter().partition(|(_, p)| *p == main);
        (mine.into_iter().next().map(|(n, _)| n).unwrap_or_default(), others)
    }

    /// `repos` as a layer can be over them, or why not: names that are plain words, each folder a repository's top,
    /// `repo` among them when both are set, and a repository inside another one the outer one ignores.
    pub(crate) fn check_repos(&self) -> Result<(), String> {
        let repos = self.repos();
        if repos.is_empty() {
            return Ok(());
        }
        for (n, p) in &repos {
            if n.is_empty() || !n.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                return Err(format!("repos: `{n}` is no plain name (letters, digits, - and _)"));
            }
            let top = std::process::Command::new("git").args(["-C", p, "rev-parse", "--show-toplevel"]).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
            if top.as_deref() != Some(p.as_str()) {
                return Err(format!("repos: {n} = {p} is not the top of a git repository"));
            }
        }
        if let Some(r) = self.project.repo.as_ref().and_then(|_| self.repo()) {
            if !repos.iter().any(|(_, p)| *p == r) {
                return Err(format!("repo = {r} is not one of repos"));
            }
        }
        for (n, p) in &repos {
            for (m, q) in &repos {
                if let Some(rel) = p.strip_prefix(&format!("{q}/")) {
                    let ignored = std::process::Command::new("git").args(["-C", q, "check-ignore", "-q", &format!("{rel}/")]).status().is_ok_and(|s| s.success());
                    if !ignored {
                        return Err(format!("repos: {n} ({p}) is inside {m} ({q}), which does not ignore it: a layer over both would take {n}'s files into {m}'s"));
                    }
                }
            }
        }
        Ok(())
    }

    /// A path of the project file: `~/`, absolute, or relative to the file's folder, made canonical when it exists.
    pub(crate) fn path(&self, r: &str) -> String {
        let r = r.trim();
        let p = if let Some(rest) = r.strip_prefix("~/") {
            format!("{}/{rest}", std::env::var("HOME").unwrap_or_default())
        } else if r.starts_with('/') {
            r.to_string()
        } else {
            format!("{}/{r}", self.dir)
        };
        std::fs::canonicalize(&p).map(|c| c.display().to_string()).unwrap_or(p)
    }

    /// `box_read` as paths on the machine.
    pub fn box_read(&self) -> Vec<String> {
        let home = std::env::var("HOME").unwrap_or_default();
        self.project.box_read.iter().map(|r| r.strip_prefix("~/").map(|rest| format!("{home}/{rest}")).unwrap_or_else(|| r.clone())).collect()
    }

    /// What boxed agent NAME reads, read-only: `box_read`, then its own `[read.NAME]`.
    pub fn agent_read(&self, name: &str) -> Vec<String> {
        let home = std::env::var("HOME").unwrap_or_default();
        let own = self.read.get(name).map(|r| r.folders.iter().map(|r| r.strip_prefix("~/").map(|rest| format!("{home}/{rest}")).unwrap_or_else(|| r.clone())).collect::<Vec<_>>()).unwrap_or_default();
        let mut all = self.box_read();
        all.extend(own.into_iter().filter(|r| !all.contains(r)).collect::<Vec<_>>());
        all
    }

    /// The limits of a boxed agent.
    pub fn box_limits(&self) -> crate::sandbox::Limits {
        crate::sandbox::Limits { memory: self.project.box_memory.clone(), pids: self.project.box_pids, cpu: self.project.box_cpu }
    }

    /// Agent NAME's limits: its own `[limits.NAME]`, over the box's when BOXED; each only when given.
    pub fn agent_limits(&self, name: &str, boxed: bool) -> crate::sandbox::Limits {
        let b = if boxed { self.box_limits() } else { Default::default() };
        let own = self.limits.get(name).cloned().unwrap_or_default();
        crate::sandbox::Limits { memory: own.memory.or(b.memory), pids: own.pids.or(b.pids), cpu: own.cpu.or(b.cpu) }
    }

    /// The whole project's limits (`memory`, `cpu`, `pids` in the header).
    pub fn whole_limits(&self) -> crate::sandbox::Limits {
        crate::sandbox::Limits { memory: self.project.memory.clone(), pids: self.project.pids, cpu: self.project.cpu }
    }

    /// `path` as paths on the machine, in order.
    pub fn agent_path(&self) -> Vec<String> {
        let home = std::env::var("HOME").unwrap_or_default();
        self.project.path.iter().map(|r| r.strip_prefix("~/").map(|rest| format!("{home}/{rest}")).unwrap_or_else(|| r.clone())).collect()
    }

    /// `box_setup` as a path on the machine.
    pub fn box_setup(&self) -> Option<String> {
        self.project.box_setup.as_deref().map(|s| self.path(s))
    }

    /// The project named in COORD/project, or none.
    pub fn load(coord: &str) -> Result<Option<Project>, String> {
        let Ok(path) = std::fs::read_to_string(format!("{coord}/project")) else { return Ok(None) };
        let path = path.trim();
        let text = std::fs::read_to_string(path).map_err(|e| format!("project file {path}: {e}"))?;
        let mut p: Project = toml::from_str(&text).map_err(|e| format!("project file {path}: {e}"))?;
        p.raw = toml::from_str::<Value>(&text).unwrap_or(Value::Null);
        // no box of its own: the one every project takes
        if p.raw.pointer("/project/box").is_none() {
            p.project.boxed = crate::settings::wide_box(coord).unwrap_or(false);
        }
        p.order = repo_order(&text);
        p.dir = std::path::Path::new(path).parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
        p.check_repos().map_err(|e| format!("project file {path}: {e}"))?;
        if let Some(m) = p.project.agent_mode.as_deref().filter(|m| mode_yolo(m).is_none()) {
            return Err(format!("project file {path}: agent_mode = \"{m}\" is not a mode: safe or yolo"));
        }
        Ok(Some(p))
    }
}

/// A mode's word as a file gives it: `yolo` true, `safe` false, anything else none.
pub fn mode_yolo(word: &str) -> Option<bool> {
    match word {
        "yolo" => Some(true),
        "safe" => Some(false),
        _ => None,
    }
}

/// The yaaihh-wide default Safe · Yolo of agents on the host: `agent_mode` in `yaaihh.toml` beside the coords
/// (`~/agent-coord/yaaihh.toml`), shared by every project. None when the file or the key is not there or not a mode.
pub fn wide_mode(coord: &str) -> Option<bool> {
    let file = std::path::Path::new(coord).parent()?.join("yaaihh.toml");
    let text = std::fs::read_to_string(&file).ok()?;
    let v: Value = toml::from_str(&text).map_err(|e| eprintln!("coordd: {}: {e}", file.display())).ok()?;
    let word = v.get("agent_mode")?.as_str()?;
    mode_yolo(word).or_else(|| {
        eprintln!("coordd: {}: agent_mode = \"{word}\" is not a mode: safe or yolo; Safe is taken", file.display());
        None
    })
}

impl Project {

    /// Run view ID's `older` command once with BEFORE in $BEFORE: its output, or why not.
    pub async fn older(&self, id: &str, before: &str) -> Result<String, String> {
        let cmd = self.views.iter().find(|v| v.id == id).and_then(|v| v.older.clone()).ok_or(format!("coord older: the view {id} has no older source"))?;
        let run = Command::new("sh").arg("-c").arg(&cmd).env("BEFORE", before).current_dir(&self.dir).stdin(Stdio::null()).output();
        match tokio::time::timeout(Duration::from_secs(60), run).await {
            Ok(Ok(out)) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
            Ok(Ok(out)) => Err(format!("{cmd}: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("failed"))),
            Ok(Err(e)) => Err(format!("{cmd}: {e}")),
            Err(_) => Err(format!("{cmd}: no answer in 60 s")),
        }
    }
}

/// `Request::AddRepo` (fix 3, the project's Home; docket-edge8's shape N-1446 p2452, read right p2455 and p2456):
/// repository NAME at PATH (`~/` or a whole path) added to the project file `$COORD/project` names, AT the coord's
/// places: one key of `[project] repos` (`with_repo`), the rest of his text as it was, committed alone where the file
/// is in git inside the coord (files.rs `commit_where`, as `EditFile` commits), else the file as it was kept beside it
/// as `NAME.previous`. A coord with no project file gets one of its own, `$COORD/definition/project.toml`, which
/// `$COORD/project` then names: never in `$COORD` itself, where roster would take its own `claude-settings.json` for
/// the file's and coordd `$COORD/prompts` for the project's (accounts-lead p2455). Refused, nothing written, unless
/// the file is in no git repository outside the coord (the flavor's, p2528), PATH is no place never served (files.rs
/// `never_served`) and no repository of the project already, and the file as it would be loads as `load` loads it
/// (`check_repos`: a plain name, a git repository's top, inside no other repository of the project that does not
/// ignore it). What was written and its way back, in his words.
pub fn add_repo(at: &crate::files::Places, name: &str, path: &str) -> Result<String, String> {
    let name = name.trim();
    let path = path.trim();
    let path = if path.len() > 1 { path.trim_end_matches('/') } else { path };
    if !(path.starts_with("~/") || path.starts_with('/')) {
        return Err(format!("{path}: a folder by its whole path, or from ~/"));
    }
    let folder = at.expand(path);
    if let Some(why) = at.never_served(&std::fs::canonicalize(&folder).unwrap_or(folder)) {
        return Err(format!("{path} is never a repository here ({why})"));
    }
    let change = Change { done: format!("{name} added"), there: "a repository is added there", message: format!("the project's repository {name} = \"{path}\", added by the owner on its Home") };
    edit_project_file(at, &change, |old, dir| {
        let text = with_repo(old, name, path)?;
        // the file as it would be, taken and checked as `load` takes it
        let mut p: Project = toml::from_str(&text).map_err(|e| format!("the project file as it would be: {e}"))?;
        p.order = repo_order(&text);
        p.dir = dir.display().to_string();
        let mine = p.path(path);
        if let Some((other, _)) = p.repos().into_iter().find(|(n, q)| n != name && *q == mine) {
            return Err(format!("{path} is a repository of the project already, as {other}"));
        }
        p.check_repos()?;
        Ok(text)
    })
}

/// A change of the project file: what is said when it is written (`done`), what is changed in a file this coord does
/// not write (`there`), and its commit's message.
pub(crate) struct Change {
    pub done: String,
    pub there: &'static str,
    pub message: String,
}

/// The project file `$COORD/project` names changed by EDIT (the file's text, "" for none, and its folder, to the text
/// as it is to be), as `add_repo` writes it: one change at a time; never through a link, nor into a git repository
/// outside the coord (the flavor's); with a `.previous` copy when the file is in no work tree, else committed alone,
/// his own changes to it refused; a coord with no file gets one in `definition/` that `$COORD/project` names. What
/// was written and its way back, in his words.
pub(crate) fn edit_project_file(at: &crate::files::Places, change: &Change, edit: impl FnOnce(&str, &std::path::Path) -> Result<String, String>) -> Result<String, String> {
    use std::path::{Path, PathBuf};
    // one change at a time, from the file's read to its write: two at once would both read the old text, and the
    // second write drop the first one's change (docket-core's note, N-1446 p2528)
    static ADDING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ADDING.lock().unwrap_or_else(|e| e.into_inner());
    let Change { done, there, message } = change;
    let coord = Path::new(&at.coord);
    let named = std::fs::read_to_string(coord.join("project")).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let (file, old) = match &named {
        Some(f) => (PathBuf::from(f), Some(std::fs::read_to_string(f).map_err(|e| format!("the project file {f}: {e}"))?)),
        None => (coord.join("definition/project.toml"), None),
    };
    if std::fs::symlink_metadata(&file).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(format!("the project file {} is a link: written through, the line would go wherever it points", file.display()));
    }
    if old.is_none() && file.exists() {
        return Err(format!("{} is there already, and {}/project names no file: nothing is written", file.display(), at.coord));
    }
    let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
    // his file in a git repository outside the coord, as research's is in production's flavor checkout, which each
    // install moves to the train's pin (an ff would then refuse, or a checkout drop his line): never committed onto from
    // here (docket-core N-1446 p2528). A file of the coord's own, in `definition/`, is written as before.
    let canon = |q: &Path| std::fs::canonicalize(q).unwrap_or_else(|_| q.to_path_buf());
    if let Some(top) = crate::change::repository(&dir).filter(|_| !canon(&dir).starts_with(canon(coord))) {
        let project = coord.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        return Err(match at.look {
            // a flavour's project (change.rs `text_at`: FLAVOUR/projects/NAME)
            Some(_) => format!("{project}'s project file is in the flavor ({}), which each install moves; {there}, through the flavor", top.display()),
            None => format!("{project}'s project file is in {}, a git repository outside this coord; {there}, through that repository", top.display()),
        });
    }
    let text = edit(old.as_deref().unwrap_or(""), &dir)?;
    let fname = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let unwritten = |e: std::io::Error| format!("the project file {}: {e}", file.display());
    let Some(old) = old else {
        std::fs::create_dir_all(&dir).map_err(unwritten)?;
        std::fs::write(&file, &text).map_err(unwritten)?;
        if let Err(e) = std::fs::write(coord.join("project"), format!("{}\n", file.display())) {
            let _ = std::fs::remove_file(&file);
            return Err(format!("{}/project: {e}", at.coord));
        }
        return Ok(format!("{done}, in a new project file {} that {}/project names; the way back: remove both", file.display(), at.coord));
    };
    if !crate::files::in_work_tree(&dir) {
        let previous = dir.join(format!("{fname}.previous"));
        std::fs::write(&previous, &old).map_err(|e| format!("{}: {e}", previous.display()))?;
        std::fs::write(&file, &text).map_err(unwritten)?;
        return Ok(format!("{done} to {}; the way back: {} is the file as it was", file.display(), previous.display()));
    }
    // his own changes to the file, staged or not, stay his: the commit would carry them, and putting a failed one back
    // would unstage them (docket-core's note, p2528)
    let git = |args: &[&str]| std::process::Command::new("git").arg("-C").arg(&dir).args(args).stdin(Stdio::null()).status();
    if git(&["diff", "--quiet", "HEAD", "--", &fname]).is_ok_and(|s| s.code() == Some(1)) || git(&["diff", "--cached", "--quiet", "--", &fname]).is_ok_and(|s| s.code() == Some(1)) {
        return Err(format!("the project file {} has changes not committed yet: commit them first; nothing is written", file.display()));
    }
    std::fs::write(&file, &text).map_err(unwritten)?;
    match crate::files::commit_where(&dir, &fname, message) {
        Ok(c) => Ok(format!("{done} to {}, {c}; the way back: that commit", file.display())),
        // not committed: the file as it was, and out of the index again, so nothing changes
        Err(why) => {
            let _ = std::fs::write(&file, &old);
            let _ = git(&["reset", "-q", "--", &fname]);
            Err(why)
        }
    }
}

/// The names of TEXT's `[project] repos` (a project file), in the file's order: toml_edit keeps a document's order,
/// where serde's map would put them by name. None for a text that is no TOML or has no `repos`.
pub fn repo_order(text: &str) -> Vec<String> {
    let Ok(doc) = text.parse::<toml_edit::DocumentMut>() else { return Vec::new() };
    let repos = doc.get("project").and_then(|p| p.as_table_like()).and_then(|p| p.get("repos")).and_then(|r| r.as_table_like());
    repos.map(|r| r.iter().map(|(k, _)| k.to_string()).collect()).unwrap_or_default()
}

/// TEXT, a project file, with NAME = PATH among its `[project] repos`: one key added through toml_edit, so his other
/// lines, comments and order stay as they are. `repos` as a table or inline, as the file has it; a file without one
/// gets `repos = { NAME = "PATH" }` in `[project]`, and one without `[project]` gets it too. Refused when the text is
/// no TOML or NAME is a repository already.
fn with_repo(text: &str, name: &str, path: &str) -> Result<String, String> {
    use toml_edit::{DocumentMut, InlineTable, Item, Table, value};
    let mut doc: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| format!("the project file does not read as TOML: {e}"))?;
    let root = doc.as_table_mut();
    if !root.contains_key("project") {
        root.insert("project", Item::Table(Table::new()));
    }
    let project = root.get_mut("project").and_then(Item::as_table_like_mut).ok_or("the project file's `project` is no table")?;
    match project.get_mut("repos") {
        None => {
            let mut repos = InlineTable::new();
            repos.insert(name, path.into());
            project.insert("repos", value(repos));
        }
        Some(r) => {
            let repos = r.as_table_like_mut().ok_or("the project file's `repos` is no table")?;
            if repos.contains_key(name) {
                return Err(format!("the project has a repository named {name} already"));
            }
            repos.insert(name, value(path));
        }
    }
    Ok(doc.to_string())
}

/// Run a view's source: each line it prints is the view's whole data. SEND gets (data, error) for each line, and the
/// error when the source ends. The shell execs the command, so the source itself is coordd's child, and it is
/// signalled when coordd ends (PR_SET_PDEATHSIG): a restarted coordd leaves no sources of the old one running.
pub async fn run_source(dir: String, cmd: String, send: impl Fn(Option<Value>, Option<String>)) {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(format!("exec {cmd}"))
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // SAFETY: prctl is async-signal-safe; nothing else runs between fork and exec.
    unsafe {
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    let child = command.spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return send(None, Some(format!("{cmd}: {e}"))),
    };
    if let Some(pid) = child.id() {
        let _ = crate::cgroup::place(pid as i32, "project");
    }
    let mut lines = BufReader::new(child.stdout.take().expect("piped")).lines();
    let mut last = String::new();
    while let Ok(Some(line)) = lines.next_line().await {
        match serde_json::from_str::<Value>(&line) {
            Ok(v) => send(Some(v), None),
            Err(e) => send(None, Some(format!("not JSON ({e}): {}", line.chars().take(200).collect::<String>()))),
        }
        last = line;
    }
    let status = child.wait().await.map(|s| s.to_string()).unwrap_or_default();
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() {
        use tokio::io::AsyncReadExt;
        let _ = e.read_to_string(&mut err).await;
    }
    let _ = last;
    send(None, Some(format!("{cmd} ended ({status}){}", if err.trim().is_empty() { String::new() } else { format!(": {}", err.trim()) })));
}

/// Hand ticket delivery D (a step owed to the runner) to the project's runner, run in the project file's folder DIR.
/// Blocking: off the lock.
/// - A `check` step (agent-base's `check` kind): `RUNNER check TICKET --to AGENT [--branch B] REF`, REF the step's
///   `commit` (`REPO:COMMIT`, or a bare commit the runner finds); its verdict comes back as a `check_end` event naming
///   TICKET (`Core::project_event`).
/// - Any other call (the runner's contract, plan/tovrin-program/runner-contract.md; N-101): `RUNNER step TICKET --to
///   AGENT CALL 'ARGS-AS-JSON'`, the arguments by the call's parameter names, a text held at a place given as its
///   words; an inline step's CALL is `inline` and its arguments `{"text": ...}`. Its answer comes back as a
///   `step_end` event naming TICKET and CALL, the answer's head and fields beside them, held to the call's answer type
///   as any answer is. PLACE: what coordd adds (`runner::step_json`), and on every such step the project's `root`
///   as `line` (none when the project file names no root).
/// A reminder of a step already handed is not sent again.
/// Why a step did not go to the runner.
#[derive(Debug)]
pub struct NotSent {
    /// What coordd's journal says, with the command.
    pub journal: String,
    /// What the step is told: the runner's own message.
    pub why: String,
    /// Whether the runner's command ran and refused (an answer about the work: the step raises RunnerRefused), or could
    /// not run at all (nothing answered: the step is held back and goes again).
    pub ran: bool,
}

pub fn to_runner(dir: &str, runner: &str, d: &docket_machine::Delivery, place: &crate::runner::Place) -> Result<(), NotSent> {
    let docket_machine::What::Step { args, call, again, template, .. } = &d.what else { return Ok(()) }; // notices: nothing to run
    if *again {
        return Ok(());
    }
    if call != "check" {
        // the contract's JSON (runner.rs): hand_in's commits by repository and its layer branch
        let mut args = if call == docket_machine::syntax::INLINE { serde_json::json!({"text": template.clone().unwrap_or_default()}) } else { crate::runner::step_json(call, &crate::render::resolved(args), place) };
        // the project's root as the line the step is on (N-159 post 25): the runner cuts it and refuses another
        if let (Some(line), Some(o)) = (&place.line, args.as_object_mut()) {
            o.insert("line".into(), serde_json::json!(line));
        }
        // the work it is for, never a par's branch ticket (N-101 p166): the runner records a hand-in as `of`
        if let (Some(of), Some(o)) = (&place.of, args.as_object_mut()) {
            o.insert("of".into(), serde_json::json!(of));
        }
        let cmd = format!("{runner} step {} --to {} {} {}", sh_quote(&d.ticket), sh_quote(&d.to), sh_quote(call), sh_quote(&args.to_string()));
        return run_runner(dir, &cmd);
    }
    let commit = args.get("commit").and_then(Value::as_str).ok_or_else(|| NotSent { journal: format!("{}: step {call} has no commit to check", d.id), why: format!("step {call} has no commit to check"), ran: true })?;
    // a check is of a branch (paper-research m661): the runner is Red when its head does not contain the commit
    let branch = args.get("branch").and_then(Value::as_str).map(|b| format!(" --branch {}", sh_quote(b))).unwrap_or_default();
    let cmd = format!("{runner} check {} --to {}{branch} {}", sh_quote(&d.ticket), sh_quote(&d.to), sh_quote(commit));
    run_runner(dir, &cmd)
}

/// Run the runner's command CMD in DIR. Not sent when it exits otherwise than 0: refused (`ran`), its message what it
/// wrote on stderr, its last line, else how it exited; or not run at all, when the shell could not start, found no
/// runnable command (126, 127), or the command was killed before it could answer (a signal: 128 + N).
fn run_runner(dir: &str, cmd: &str) -> Result<(), NotSent> {
    let out = std::process::Command::new("sh").arg("-c").arg(cmd).current_dir(dir).output().map_err(|e| NotSent { journal: format!("{cmd}: {e}"), why: format!("the runner's command could not be run: {e}"), ran: false })?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let why = err.lines().last().map(str::to_string).filter(|l| !l.is_empty()).unwrap_or_else(|| format!("the runner exited with {}", out.status));
    // ran and refused: it exited itself, 1 to 125; no runnable command (126, 127) or killed by a signal (the shell's
    // 128 + N, or no code at all) gave no answer
    Err(NotSent { journal: format!("{cmd}: {}: {err}", out.status), why, ran: matches!(out.status.code(), Some(1..=125)) })
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
#[path = "tests_library.rs"]
mod tests_library;

#[cfg(test)]
mod tests_serve {
    use super::*;

    /// symbols' `[[lsp]]` entries load as coordd reads them: `serve` and `socket` taken, the rest left to symbols.
    #[test]
    fn lsp_entries_give_serve_and_socket() {
        let p: Project = toml::from_str(
            r#"
[[lsp]]
lang = "agda"
command = ["agda-ls"]
files = ["*.agda"]
skip = ["agda/proofs"]
serve = ["agda-ls", "--serve", "$COORD/lsp/agda.sock", "--allow", "/p"]
socket = "$COORD/lsp/agda.sock"

[[lsp]]
lang = "rust"
command = ["rust-analyzer"]
files = ["*.rs"]
"#,
        )
        .unwrap();
        assert_eq!(p.lsps.len(), 2);
        assert_eq!(p.lsps[0].serve[2], "$COORD/lsp/agda.sock");
        assert_eq!(Lsp::expand(p.lsps[0].socket.as_deref().unwrap(), "/c"), "/c/lsp/agda.sock");
        assert!(p.lsps[1].serve.is_empty() && p.lsps[1].socket.is_none());
    }

    /// A shared server runs with $COORD in its words and its environment, its socket is opened to every user once
    /// it listens, and how it ended is said.
    #[tokio::test]
    async fn a_shared_server_runs_and_its_socket_is_opened() {
        use std::os::unix::fs::PermissionsExt;
        let coord = std::env::temp_dir().join(format!("coordd-serve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let c = coord.display().to_string();
        let l = Lsp {
            lang: "agda".into(),
            serve: ["python3", "-c", "import socket,sys,os,time; s=socket.socket(socket.AF_UNIX); os.umask(0o077); s.bind(sys.argv[1]); s.listen(); time.sleep(2); assert os.environ['COORD']", "$COORD/lsp/agda.sock"].map(String::from).to_vec(),
            socket: Some("$COORD/lsp/agda.sock".into()),
        };
        // a stale socket file from an earlier server is removed first
        std::fs::create_dir_all(coord.join("lsp")).unwrap();
        std::fs::write(coord.join("lsp/agda.sock"), "").unwrap();
        let run = tokio::spawn(run_server(c.clone(), c.clone(), l));
        let sock = coord.join("lsp/agda.sock");
        let mut mode = 0;
        for _ in 0..30 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if let Ok(m) = std::fs::symlink_metadata(&sock) {
                use std::os::unix::fs::FileTypeExt;
                if m.file_type().is_socket() {
                    mode = m.permissions().mode() & 0o777;
                    if mode == 0o666 {
                        break;
                    }
                }
            }
        }
        assert_eq!(mode, 0o666);
        let ended = run.await.unwrap();
        assert!(ended.contains("ended: exit status: 0"), "{ended}");
        let _ = std::fs::remove_dir_all(&coord);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `repo` names one of `repos` or is a folder; a name wins; anything else is refused at load.
    #[test]
    fn repo_is_one_of_repos_by_name_or_by_folder() {
        let t = std::env::temp_dir().join(format!("coordd-project-repo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        for r in ["a", "b"] {
            std::fs::create_dir_all(t.join(r)).unwrap();
            assert!(std::process::Command::new("git").args(["init", "-q"]).current_dir(t.join(r)).status().unwrap().success());
        }
        std::fs::create_dir_all(t.join("files")).unwrap();
        let canon = |r: &str| std::fs::canonicalize(t.join(r)).unwrap().display().to_string();
        let load = |repo: &str| {
            let toml = format!("[project]\nname = \"x\"\nrepos = {{ a = \"{}\", b = \"{}\" }}\n{repo}", canon("a"), canon("b"));
            let p = Project { project: toml::from_str::<Project>(&toml).unwrap().project, dir: canon("files"), ..Default::default() };
            (p.repo(), p.check_repos())
        };
        assert_eq!(load("repo = \"b\""), (Some(canon("b")), Ok(())), "by name");
        assert_eq!(load(&format!("repo = \"{}\"", canon("b"))), (Some(canon("b")), Ok(())), "by folder");
        assert_eq!(load("repo = \"../b\"").0, Some(canon("b")), "by folder, relative to the project file's");
        assert_eq!(load("").0, Some(canon("a")), "unset: the first by name");
        assert!(load("repo = \"c\"").1.unwrap_err().contains("is not one of repos"));
        std::fs::remove_dir_all(&t).unwrap();
    }

    /// The limits a project file gives: the whole project's in its header; an agent's own (`[limits.NAME]`) over the
    /// box's for a boxed one, alone for one that is not; a word the limits do not know refuses the file. `path` and
    /// `box_setup` as paths on the machine.
    #[test]
    fn limits_paths_and_the_box_setup_as_the_file_gives_them() {
        use crate::sandbox::Limits;
        let home = std::env::var("HOME").unwrap();
        let mut p: Project = toml::from_str(
            r#"
[project]
name = "x"
memory = "40G"
cpu = 12
box_memory = "8G"
box_pids = 2048
box_setup = "./box-setup.sh"
path = ["~/.ghcup/bin", "/opt/t/bin"]
box_read = ["~/.ghcup", "/opt/t"]

[read.orch]
folders = ["~/notes", "/opt/t"]

[limits.runner]
memory = "27G"
cpu = 10

[limits.orch]
pids = 512
"#,
        )
        .unwrap();
        p.dir = "/p/files".into();
        assert_eq!(p.whole_limits(), Limits { memory: Some("40G".into()), cpu: Some(12.0), pids: None });
        assert_eq!(p.agent_limits("runner", false), Limits { memory: Some("27G".into()), cpu: Some(10.0), pids: None });
        assert_eq!(p.agent_limits("orch", true), Limits { memory: Some("8G".into()), cpu: None, pids: Some(512) }, "its own over the box's");
        assert_eq!(p.agent_limits("f3-1", true), p.box_limits());
        assert_eq!(p.agent_limits("f3-1", false), Limits::default());
        assert_eq!(p.agent_path(), vec![format!("{home}/.ghcup/bin"), "/opt/t/bin".to_string()]);
        assert_eq!(p.box_setup().as_deref(), Some("/p/files/./box-setup.sh"));
        assert_eq!(p.agent_read("orch"), vec![format!("{home}/.ghcup"), "/opt/t".into(), format!("{home}/notes")], "box_read, then its own, once each");
        assert_eq!(p.agent_read("f3-1"), p.box_read());
        assert!(toml::from_str::<Project>("[limits.runner]\nmemmory = \"1G\"\n").is_err(), "a misspelt limit is refused");
    }

    /// His Add a repository writes one key and leaves the rest of his text as it was: `repos` inline or as a table, as
    /// his file has it; none yet, or no file, `repos = { NAME = "PATH" }` in `[project]` (p2452).
    #[test]
    fn a_repository_is_one_key_added_and_his_text_stays() {
        let inline = "# his project\n[project]\nname = \"walk\" # its name\nrepos = { tovrin = \"../tovrin\" }\n\n[[view]]\nid = \"home\"\n";
        let t = with_repo(inline, "p4", "~/p4").unwrap();
        let others = |t: &str| t.lines().filter(|l| !l.starts_with("repos")).map(String::from).collect::<Vec<_>>();
        assert_eq!(others(&t), others(inline), "his other lines, comments and order");
        let v: Value = toml::from_str(&t).unwrap();
        assert_eq!(v["project"]["repos"], serde_json::json!({"tovrin": "../tovrin", "p4": "~/p4"}));
        let table = "[project]\nname = \"walk\"\n\n[project.repos]\n# the main one\ntovrin = \"../tovrin\"\n\n[[view]]\nid = \"home\"\n";
        assert_eq!(with_repo(table, "p4", "~/p4").unwrap(), table.replace("tovrin = \"../tovrin\"\n", "tovrin = \"../tovrin\"\np4 = \"~/p4\"\n"), "one line under [project.repos]");
        assert_eq!(with_repo("[project]\nname = \"walk\"\n", "p4", "~/p4").unwrap(), "[project]\nname = \"walk\"\nrepos = { p4 = \"~/p4\" }\n");
        assert_eq!(with_repo("", "p4", "~/p4").unwrap(), "[project]\nrepos = { p4 = \"~/p4\" }\n", "a file of its own");
        assert!(with_repo(table, "tovrin", "/x").unwrap_err().contains("named tovrin already"));
        assert!(with_repo("[project\n", "p4", "/x").unwrap_err().contains("does not read as TOML"));
    }

    /// His yaaihh project (paper-research N-1446 p3387, 2): soft-swarm added first, then flavor, proxy and rune, and
    /// fresh started in flavor, the first by name. Agents start in the first added, the file's own order, unless
    /// `repo` names one; Files lists them in that order too (`Project::repos`, `files::roots`).
    #[test]
    fn agents_start_in_the_repository_added_first_unless_the_project_names_one() {
        let d = std::env::temp_dir().join(format!("coordd-repo-order-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let (coord, home) = (d.join("coord"), d.join("home"));
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        let top = |r: &str| {
            std::fs::create_dir_all(d.join(r)).unwrap();
            assert!(std::process::Command::new("git").arg("-C").arg(d.join(r)).args(["init", "-q"]).status().unwrap().success());
            std::fs::canonicalize(d.join(r)).unwrap().display().to_string()
        };
        let (soft, flavor, rune) = (top("soft-swarm"), top("flavor"), top("rune"));
        let at = crate::files::Places { home: home.display().to_string(), coord: coord.display().to_string(), ..Default::default() };
        for (n, path) in [("soft-swarm", &soft), ("flavor", &flavor), ("rune", &rune)] {
            add_repo(&at, n, path).unwrap();
        }
        let load = || Project::load(&coord.display().to_string()).unwrap().unwrap();
        let p = load();
        let names: Vec<String> = p.repos().into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["soft-swarm", "flavor", "rune"], "as added, not by name");
        assert_eq!(p.repo(), Some(soft.clone()), "the first added");
        assert_eq!(p.layer_repos().0, "soft-swarm");
        // the project names one
        let file = coord.join("definition/project.toml");
        let text = std::fs::read_to_string(&file).unwrap().replace("[project]\n", "[project]\nrepo = \"rune\"\n");
        std::fs::write(&file, text).unwrap();
        assert_eq!(load().repo(), Some(rune.clone()));
        assert_eq!(repo_order("[project\n"), Vec::<String>::new(), "no TOML: no order, by name as before");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// `Request::AddRepo`'s write (p2452, p2455): refused with nothing written unless the file as it would be loads;
    /// a coord with no project file gets one of its own in `definition/`, named in the coord's `project`, with nothing
    /// beside it; a file in no git is kept as it was beside it; a file in git gets a commit of its own, his staged
    /// work left out of it.
    #[test]
    fn the_owners_repository_goes_in_his_file_or_in_a_file_of_its_own() {
        let d = std::env::temp_dir().join(format!("coordd-add-repo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let (coord, home) = (d.join("coord"), d.join("home"));
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::create_dir_all(home.join(".ssh")).unwrap();
        let git = |dir: &std::path::Path, args: &[&str]| {
            let o = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
            assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        };
        let top = |r: &str| {
            std::fs::create_dir_all(d.join(r)).unwrap();
            git(&d.join(r), &["init", "-q"]);
            std::fs::canonicalize(d.join(r)).unwrap().display().to_string()
        };
        let (tovrin, p4) = (top("tovrin"), top("p4"));
        let at = crate::files::Places { home: home.display().to_string(), coord: coord.display().to_string(), ..Default::default() };
        let load = || Project::load(&coord.display().to_string()).unwrap().unwrap();

        // refused, nothing written
        assert!(add_repo(&at, "tovrin", "tovrin").unwrap_err().contains("whole path"));
        assert!(add_repo(&at, "bad name", &tovrin).unwrap_err().contains("no plain name"));
        assert!(add_repo(&at, "keys", "~/.ssh").unwrap_err().contains("is never a repository here"));
        std::fs::create_dir_all(d.join("plain")).unwrap();
        assert!(add_repo(&at, "plain", &d.join("plain").display().to_string()).unwrap_err().contains("not the top of a git repository"));
        assert!(!coord.join("project").exists() && !coord.join("definition").exists(), "nothing written");

        // no project file: one of its own, never in the coord's folder itself, and nothing beside it that roster or
        // coordd would take for the project's (its settings, standing and prompts stay agent-base's and the coord's)
        let said = add_repo(&at, "tovrin", &format!("{tovrin}/")).unwrap();
        let file = coord.join("definition/project.toml");
        assert!(said.contains("remove both"), "{said}");
        assert_eq!(std::fs::read_to_string(coord.join("project")).unwrap(), format!("{}\n", file.display()));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), format!("[project]\nrepos = {{ tovrin = \"{tovrin}\" }}\n"));
        let p = load();
        assert_eq!((p.repo(), p.dir.clone()), (Some(tovrin.clone()), coord.join("definition").display().to_string()));
        let beside: Vec<String> = std::fs::read_dir(coord.join("definition")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(beside, ["project.toml"]);

        // a second repository, the file in no git: kept as it was beside it; the same folder or name twice refused
        assert!(add_repo(&at, "again", &tovrin).unwrap_err().contains("already, as tovrin"));
        assert!(add_repo(&at, "tovrin", &p4).unwrap_err().contains("named tovrin already"));
        let before = std::fs::read_to_string(&file).unwrap();
        let said = add_repo(&at, "p4", &p4).unwrap();
        assert!(said.contains("project.toml.previous"), "{said}");
        assert_eq!(std::fs::read_to_string(coord.join("definition/project.toml.previous")).unwrap(), before);
        assert_eq!((load().repos().len(), load().repo()), (2, Some(tovrin.clone())), "no repo set: the first added, not the first by name");

        // his file in a git repository: his comments in it, his notes staged, and the coord's `project` naming it
        let his = |r: &str| {
            let dir = d.join(r);
            top(r);
            git(&dir, &["config", "user.name", "t"]);
            git(&dir, &["config", "user.email", "t@t"]);
            std::fs::write(dir.join("project.toml"), "# his\n[project]\nname = \"walk\"\n").unwrap();
            std::fs::write(dir.join("notes.md"), "his").unwrap();
            git(&dir, &["add", "project.toml"]);
            git(&dir, &["commit", "-q", "-m", "start"]);
            git(&dir, &["add", "notes.md"]);
            std::fs::write(coord.join("project"), format!("{}\n", dir.join("project.toml").display())).unwrap();
            dir
        };

        // outside the coord, as research's is in production's flavor checkout, which each install moves (p2528):
        // refused, nothing written or committed, his staged work as it was
        let flavor = his("flavor");
        let why = add_repo(&at, "tovrin", &tovrin).unwrap_err();
        let outside = std::fs::canonicalize(&flavor).unwrap().display().to_string();
        assert_eq!(why, format!("coord's project file is in {outside}, a git repository outside this coord; a repository is added there, through that repository"));
        let flavoured = crate::files::Places { look: Some(flavor.join("look.toml").display().to_string()), ..at.clone() };
        let why = add_repo(&flavoured, "tovrin", &tovrin).unwrap_err();
        assert_eq!(why, format!("coord's project file is in the flavor ({outside}), which each install moves; a repository is added there, through the flavor"));
        assert_eq!(std::fs::read_to_string(flavor.join("project.toml")).unwrap(), "# his\n[project]\nname = \"walk\"\n", "nothing written");
        assert_eq!(git(&flavor, &["log", "--format=%s"]), "start", "nothing committed");
        assert_eq!(git(&flavor, &["diff", "--cached", "--name-only"]), "notes.md");
        assert!(!flavor.join("project.toml.previous").exists());

        // a file of the coord's own in git: a commit of its own, his staged work and his comments left as they are
        let own = his("coord/own");
        // his own change to the file, unstaged or staged: refused, his change as it was (p2528)
        std::fs::write(own.join("project.toml"), "# his, changed\n[project]\nname = \"walk\"\n").unwrap();
        assert!(add_repo(&at, "tovrin", &tovrin).unwrap_err().contains("has changes not committed yet"));
        git(&own, &["add", "project.toml"]);
        assert!(add_repo(&at, "tovrin", &tovrin).unwrap_err().contains("has changes not committed yet"));
        assert_eq!((git(&own, &["diff", "--cached", "--name-only"]), git(&own, &["log", "--format=%s"])), ("notes.md\nproject.toml".into(), "start".into()), "staged as he left it");
        assert_eq!(std::fs::read_to_string(own.join("project.toml")).unwrap(), "# his, changed\n[project]\nname = \"walk\"\n");
        git(&own, &["reset", "-q", "--", "project.toml"]);
        git(&own, &["checkout", "--", "project.toml"]);
        let said = add_repo(&at, "tovrin", &tovrin).unwrap();
        assert!(said.contains(", commit ") && said.ends_with("the way back: that commit"), "{said}");
        assert_eq!(git(&own, &["log", "-1", "--format=%s"]), format!("the project's repository tovrin = \"{tovrin}\", added by the owner on its Home"));
        assert_eq!(git(&own, &["show", "--name-only", "--format="]), "project.toml");
        assert_eq!(git(&own, &["diff", "--cached", "--name-only"]), "notes.md");
        assert_eq!(std::fs::read_to_string(own.join("project.toml")).unwrap(), format!("# his\n[project]\nname = \"walk\"\nrepos = {{ tovrin = \"{tovrin}\" }}\n"));
        assert_eq!(load().repo(), Some(tovrin.clone()));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Six Adds at once, as two windows or two presses send them, each keep their key: one at a time, from the file's
    /// read to its write (docket-core's note, N-1446 p2528).
    #[test]
    fn adds_at_once_each_keep_their_repository() {
        let d = std::env::temp_dir().join(format!("coordd-add-repos-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let coord = d.join("coord");
        std::fs::create_dir_all(coord.join("definition")).unwrap();
        let file = coord.join("definition/project.toml");
        std::fs::write(&file, "[project]\nname = \"walk\"\n").unwrap();
        std::fs::write(coord.join("project"), format!("{}\n", file.display())).unwrap();
        let repos: Vec<(String, String)> = (1..=6)
            .map(|i| {
                let r = d.join(format!("r{i}"));
                std::fs::create_dir_all(&r).unwrap();
                assert!(std::process::Command::new("git").arg("-C").arg(&r).args(["init", "-q"]).status().unwrap().success());
                (format!("r{i}"), std::fs::canonicalize(&r).unwrap().display().to_string())
            })
            .collect();
        let at = crate::files::Places { home: d.join("home").display().to_string(), coord: coord.display().to_string(), ..Default::default() };
        let start = std::sync::Barrier::new(repos.len());
        std::thread::scope(|t| {
            for (name, path) in &repos {
                let (at, start) = (&at, &start);
                t.spawn(move || {
                    start.wait();
                    add_repo(at, name, path).unwrap();
                });
            }
        });
        let p = Project::load(&coord.display().to_string()).unwrap().unwrap();
        let mut got: Vec<String> = p.repos().into_iter().map(|(n, _)| n).collect();
        got.sort();
        assert_eq!(got, repos.iter().map(|r| r.0.clone()).collect::<Vec<_>>(), "every Add's key");
        let _ = std::fs::remove_dir_all(&d);
    }
}
