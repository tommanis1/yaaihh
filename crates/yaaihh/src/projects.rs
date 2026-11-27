//! yaaihh's projects (design §3; amendments, 7 Nov evening): the config (`~/.config/yaaihh/projects.toml`, or the file
//! YAAIHH_CONFIG names), where each project is one coordd, in a box or on the host, read through its pipe; and the New
//! project dialog, which starts a project's coordd and adds the project to the config.
//!
//!   [[project]]
//!   name  = "My box"
//!   box   = "my-box"          (or `id`: the box's name, or the host coordd's; its coord folder is ~/agent-coord/ID)
//!   boxed = false             (coordd and the agents on the host as the owner's user, bin/host-coordd; default true)
//!   device = "tower"          (its server's device, one of the `[[device]]`s; none: this PC's, through WSL from Windows)
//!
//!   [[device]]
//!   name = "tower"
//!   ssh  = "t@tower-server"   (where ssh goes, over Tailscale, with his own keys: places.rs `Device`)
//!   agent_base = "/home/t/soft-swarm/agent-base"   (its own; none: the config's `agent_base`)

use crate::places::Device;
use crate::{App, Proj};
use rune::{El, TextEdit, col, row, text};
use rune_kit::{Button, ButtonVariant, Dialog, Kit, TextInput};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, channel};

#[derive(Deserialize, Default)]
pub struct Config {
    pub agent_base: Option<String>,
    /// The WSL distribution the coordds run in (places.rs `server`).
    pub wsl: Option<String>,
    /// The folders a relative file path in a chat is looked for in (`~` allowed), after the chat's own folder.
    #[serde(default)]
    pub search: Vec<String>,
    /// The devices a project's server can run on besides this PC (places.rs `Device`).
    #[serde(default)]
    pub device: Vec<Device>,
    #[serde(default)]
    pub project: Vec<ProjectEntry>,
}

/// A `[[project]]` as written: its own fields, and the device its server runs on (`device`, a `[[device]]`'s name;
/// none: this PC's own).
#[derive(Clone, Deserialize)]
pub struct ProjectEntry {
    #[serde(flatten)]
    pub cfg: ProjectConfig,
    pub device: Option<String>,
}

impl Config {
    /// Each project with its device, in the config's order; one whose device is missing, named twice or not reached by
    /// a plain `USER@HOST` is left out, with why (never run on this PC instead: a coordd of the same id here is another
    /// project's).
    pub fn placed(&self) -> (Vec<(ProjectConfig, Option<Device>)>, Vec<String>) {
        let (mut placed, mut refused) = (Vec::new(), Vec::new());
        for e in &self.project {
            let Some(name) = &e.device else {
                placed.push((e.cfg.clone(), None));
                continue;
            };
            let named: Vec<&Device> = self.device.iter().filter(|d| &d.name == name).collect();
            let why = match named[..] {
                [] => format!("there is no device {name}"),
                [d] if ssh_ok(&d.ssh) => {
                    placed.push((e.cfg.clone(), Some(d.clone())));
                    continue;
                }
                [d] => format!("device {name}'s ssh is not a USER@HOST ({})", d.ssh),
                _ => format!("there are {} devices named {name}", named.len()),
            };
            refused.push(format!("project {} is left out: {why}", e.cfg.name));
        }
        (placed, refused)
    }
}

/// Whether S is a plain `USER@HOST` or host name for ssh: letters, digits and `._-@`, not starting with `-` (never an
/// option).
fn ssh_ok(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('-') && s.chars().all(|c| c.is_ascii_alphanumeric() || "._-@".contains(c))
}

#[derive(Clone, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    /// The box's name, or the host coordd's (its unit is coordd-ID).
    #[serde(alias = "box")]
    pub id: String,
    #[serde(default = "yes")]
    pub boxed: bool,
}

fn yes() -> bool {
    true
}

/// The config file: YAAIHH_CONFIG, else projects.toml in the config folder (~/.config/yaaihh; on Windows
/// %APPDATA%\yaaihh: places.rs).
pub fn config_path() -> String {
    std::env::var("YAAIHH_CONFIG").unwrap_or_else(|_| crate::places::config_dir().join("projects.toml").to_string_lossy().into_owned())
}

/// The pipe to P's coordd on its server, on DEVICE when it has one: `coordc bridge` inside its box (`box.sh sh`), or
/// with its host coordd's environment (`host-coordd sh`). Both put agent-base's bin first on PATH. With
/// YAAIHH_LOCAL_PIPE set, `coordc bridge` with this process's own environment (COORD, COORDD_SOCK): a yaaihh run inside
/// a box, on the box's coordd (the yaaihh-dev box's agents filming a feature).
pub fn pipe(base: &str, p: &ProjectConfig, device: Option<&Device>) -> Vec<String> {
    // this window's id (state.rs `box_id`, N-487): an older bridge takes no `--box` and reads past it
    let id = crate::state::box_id();
    if std::env::var_os("YAAIHH_LOCAL_PIPE").is_some() {
        return vec!["coordc".into(), "bridge".into(), "--box".into(), id];
    }
    // on the coordds' machine: through wsl.exe from Windows, by ssh on another device (places.rs)
    crate::places::on(vec![tool(base, p.boxed, device), "sh".into(), p.id.clone(), "coordc".into(), "bridge".into(), "--box".into(), id], device)
}

/// What runs a command with a project's coordd's environment on its server: its box's `box.sh` when BOXED, else
/// `host-coordd`, in BASE, or in DEVICE's own agent-base when it names one.
fn tool(base: &str, boxed: bool, device: Option<&Device>) -> String {
    let base = device.and_then(|d| d.agent_base.as_deref()).unwrap_or(base);
    if boxed { format!("{base}/box/box.sh") } else { format!("{base}/bin/host-coordd") }
}

/// The Linux side kept up after the window closes (docket-core7, N-433 p32, p36): from Windows the coordds and the
/// agents live in a WSL distribution, which WSL stops a few seconds after its last process from Windows ends, so a
/// window that only had its bridges would take every agent down with it when closed. At its start the window starts
/// `work-hold` there (agent-base bin/work-hold: it waits for the user manager, starts the proxy once more, then sleeps
/// as `yaaihh-holder`; a second one exits at once), with no console window and nothing waiting for it, so it outlives
/// the window. On Linux nothing: the distribution is the machine itself.
pub fn hold(base: &str) {
    let Some(cmd) = hold_cmd(base, cfg!(windows)) else { return };
    let said = cmd.join(" ");
    let mut c = std::process::Command::new(&cmd[0]);
    c.args(&cmd[1..]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW | DETACHED_PROCESS: no console of its own, and none of the window's
        c.creation_flags(0x0800_0000 | 0x0000_0008);
    }
    // the child is dropped, never waited on: it ends when Windows or `wsl --shutdown` ends it
    match c.spawn() {
        Ok(_) => crate::log::line(&format!("hold: {said}")),
        Err(e) => crate::log::line(&format!("hold: {said}: {e}")),
    }
}

/// What [`hold`] runs: `work-hold` of BASE through wsl.exe (places.rs), on Windows only; None without a base.
fn hold_cmd(base: &str, windows: bool) -> Option<Vec<String>> {
    (windows && !base.is_empty()).then(|| crate::places::server_in(vec![format!("{base}/bin/work-hold")], crate::places::WSL.get().and_then(Option::as_deref), windows))
}

/// Bring project I up, in the background: its box started again with its start steps (`box.sh start ID`, which knows
/// the box's folder), or its host coordd (`host-coordd start ID`); both resume the agents that ran when its coordd
/// last went down (`roster resume`). Each step does nothing when already done, so this runs at every start of yaaihh.
pub fn bring_up(s: &mut App, i: usize) {
    if s.projects[i].starting.is_some() {
        return;
    }
    let (base, wake) = (s.base.clone(), s.wake.clone());
    let p = &mut s.projects[i];
    let cmd = vec![tool(&base, p.boxed, p.device.as_ref()), "start".into(), p.id.clone()];
    p.starting = Some(background(cmd, wake, p.device.as_ref()));
}

/// CMD run off the frame on the server, on DEVICE when the project has one, its output's last line or why it failed
/// on the receiver, logged, and the window woken.
fn background(cmd: Vec<String>, wake: Option<rune::Waker>, device: Option<&Device>) -> Receiver<Result<String, String>> {
    // each is a command of the coordds' machine (host-coordd, box.sh, roster): through wsl.exe from Windows, by ssh on
    // another device (places.rs)
    let cmd = crate::places::on(cmd, device);
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let said = cmd.join(" ");
        crate::log::line(&format!("run: {said}"));
        let last = |b: &[u8]| String::from_utf8_lossy(b).lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no message").to_string();
        let r = match crate::places::command(&cmd[0]).args(&cmd[1..]).stdin(std::process::Stdio::null()).output() {
            Ok(o) if o.status.success() => Ok(last(&o.stdout)),
            Ok(o) => Err(format!("{said} failed ({}): {}", o.status, last(&[o.stderr, o.stdout].concat()))),
            Err(e) => Err(format!("{said}: {e}")),
        };
        crate::log::line(&format!("run: {said}: {}", r.as_ref().unwrap_or_else(|e| e)));
        let _ = tx.send(r);
        if let Some(w) = wake {
            w();
        }
    });
    rx
}

/// Spin off agent NAME of the current project (his walk item 19: "with an agent selected he wants a quick way to make a
/// spin-off of it"): `roster fork NAME` through its box or host coordd, a new agent on a copy of NAME's whole
/// conversation, recorded as a copy of NAME, with the owner its parent (agent-base bin/roster; a busy NAME's copy is
/// cut where its calls all have their results, coordd's `owner_roster`). Shown when it is up (`brought_up`), and pinned
/// under NAME in the sidebar. AT (seconds; the Map's Rewind / Branch from here): the copy is cut at the end of NAME's
/// last turn that ended by then, asked of a coordd at [`coord_types::REWIND_PROTOCOL`] or later only.
pub fn fork(s: &mut App, name: &str, at: Option<i64>) {
    let (base, wake) = (s.base.clone(), s.wake.clone());
    let Some(p) = s.projects.get_mut(s.cur) else { return };
    if p.forking.is_some() {
        return;
    }
    if at.is_some() && p.protocol < coord_types::REWIND_PROTOCOL {
        s.notice = Some(format!("{}: needs a newer engine; {name} not forked", p.name));
        return;
    }
    // asked of coordd where its bridge speaks protocol 1, which an app on another machine needs; else through a shell
    let req = coord_types::Request::ForkAgent { from: name.into(), new: None, model: None, dir: None, text: None, at };
    if let Some(rx) = p.ask_roster(&req) {
        p.forking = Some(rx);
        return;
    }
    if at.is_some() {
        s.notice = Some(format!("{}: not connected; {name} was not forked", p.name));
        return;
    }
    let tool = tool(&base, p.boxed, p.device.as_ref());
    p.forking = Some(background(vec![tool, "sh".into(), p.id.clone(), "roster".into(), "fork".into(), name.into()], wake, p.device.as_ref()));
}

/// The owner's end of a spin-off, while coordd does it (`Request::SpinEnd`).
pub struct SpinEnding {
    pub name: String,
    /// The agent it was copied from: where a joined-back or deleted spin-off's page goes.
    pub from: String,
    pub how: coord_types::SpinEnd,
    pub rx: std::sync::mpsc::Receiver<Result<String, String>>,
}

/// End spin-off NAME of the current project, a copy of FROM, as HOW says (his walk item 19: "join it back when he
/// closes it or delete it"): asked of coordd, which answers when it is done. One at a time per project; nothing from a
/// coordd before [`coord_types::SPIN_PROTOCOL`] (the window shows no such press then).
pub fn spin_end(s: &mut App, name: &str, from: &str, how: coord_types::SpinEnd) {
    let Some(p) = s.projects.get_mut(s.cur) else { return };
    if p.spin_ending.is_some() || p.protocol < coord_types::SPIN_PROTOCOL {
        return;
    }
    match p.ask_roster(&coord_types::Request::SpinEnd { name: name.into(), how }) {
        Some(rx) => p.spin_ending = Some(SpinEnding { name: name.into(), from: from.into(), how, rx }),
        None => s.notice = Some(format!("{}: not connected; {name} was not ended", p.name)),
    }
}

/// The bring-ups that ended since the last frame; true when one did.
pub fn brought_up(s: &mut App) -> bool {
    let mut any = false;
    let mut forked = None;
    let mut spun = Vec::new();
    for (i, p) in s.projects.iter_mut().enumerate() {
        if let Some(r) = p.starting.as_ref().and_then(|rx| rx.try_recv().ok()) {
            p.starting = None;
            p.start_error = r.err();
            any = true;
        }
        if let Some(r) = p.forking.as_ref().and_then(|rx| rx.try_recv().ok()) {
            p.forking = None;
            forked = Some((i, r));
            any = true;
        }
        if let Some(r) = p.spin_ending.as_ref().and_then(|e| e.rx.try_recv().ok()) {
            let e = p.spin_ending.take().unwrap();
            spun.push((i, e, r));
            any = true;
        }
    }
    for (i, e, r) in spun {
        match r {
            // joined back or deleted: it is in none of his lists, its pin goes, and its page gives way to the agent it
            // came from; closed, it stays, stopped
            Ok(_) if e.how != coord_types::SpinEnd::Close => {
                let project = s.projects[i].id.clone();
                s.pins.unpin_agent(&project, &e.name);
                if i == s.cur && s.sel == crate::Sel::Agent(e.name.clone()) {
                    s.sel = crate::Sel::Agent(e.from);
                }
            }
            Ok(_) => {}
            Err(why) => s.notice = Some(why),
        }
    }
    if let Some(r) = s.rename.running.as_ref().and_then(|rx| rx.try_recv().ok()) {
        s.rename.running = None;
        any = true;
        match r {
            Ok(_) => {
                let (from, to) = (s.rename.from.clone(), s.rename.name.text.trim().to_string());
                if s.sel == crate::Sel::Agent(from) {
                    s.sel = crate::Sel::Agent(to);
                }
                s.rename.open = false;
            }
            Err(e) => s.rename.error = Some(e),
        }
    }
    // roster fork's last line is "NEW: forked from NAME (session ...)": the new agent is shown, and pinned under NAME
    match forked {
        Some((i, Ok(said))) if i == s.cur => {
            let new = said.split(':').next().unwrap_or_default().to_string();
            if let Some(from) = said.split_once(": forked from ").and_then(|(_, r)| r.split(" (").next()) {
                let project = s.projects[i].id.clone();
                s.pins.spun_off(&project, from, &new);
            }
            s.sel = crate::Sel::Agent(new);
        }
        Some((_, Err(e))) => s.notice = Some(e),
        _ => {}
    }
    any
}

/// A project's coord folder (the box's mount, or the host coordd's COORD).
pub(crate) fn coord_dir(id: &str) -> String {
    format!("{}/agent-coord/{id}", std::env::var("HOME").unwrap_or_default())
}

/// The id a project's name gives: lower-case letters, digits and dashes (a box name and a unit name both take it).
pub fn id_of(name: &str) -> String {
    name.to_lowercase().split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join("-")
}

/// The New project dialog's fields, and the start it ran, while it runs.
#[derive(Default)]
pub struct NewProject {
    pub open: bool,
    pub name: TextEdit,
    /// The project file (views, rights, sources), named in the coord folder's `project`; may be left empty.
    pub file: TextEdit,
    /// "More" pressed: the project file's field and where the project is kept, which the first screen does not show
    /// (paper-research N-1446 p3207, 1). A file typed in keeps them shown.
    pub more: bool,
    pub error: Option<String>,
    /// The start running: its outcome comes here.
    pub running: Option<Receiver<Result<ProjectConfig, String>>>,
}

impl App {
    /// Start the project the dialog describes, off the UI thread: agent-base's `new-project host` on the coordds'
    /// machine (its coord folder, then `host-coordd start ID`), then its entry appended to the config. The dialog makes
    /// host projects only: a Docker box (`box.sh`) is not offered (the owner: the box is a Linux user per agent, no
    /// docker; paper-research N-1446 p2424); a boxed project already in the config is still brought up as one. It asks
    /// for no folder: a project has no repository at first ("none ofc intitally", the owner N-1104 p999), its agents
    /// work in a new empty `work` in its coord folder, and its repositories are added on its Home (home.rs, Add a
    /// repository; accounts-lead N-1446 p2455). Create and Enter in either field ask it; while a start runs, nothing.
    pub fn create_project(&mut self) {
        if self.new_project.running.is_some() {
            return;
        }
        let np = &self.new_project;
        let (name, file) = (np.name.text.trim().to_string(), np.file.text.trim().to_string());
        let id = id_of(&name);
        let refused = if id.is_empty() {
            Some("a name with letters or digits".to_string())
        } else if self.projects.iter().any(|p| p.id == id) {
            Some(format!("there is a project {id} already"))
        } else {
            // the folder and the file are the coordds' machine's (WSL's, from Windows): new-project checks them there
            None
        };
        if refused.is_some() {
            self.new_project.error = refused;
            return;
        }
        let cfg = ProjectConfig { name, id, boxed: false };
        let (base, config, wake) = (self.base.clone(), self.config_path.clone(), self.wake.clone());
        let (tx, rx) = channel();
        self.new_project.error = None;
        self.new_project.running = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(start(&base, &config, cfg, &file));
            if let Some(w) = wake {
                w();
            }
        });
    }

    /// The start's outcome, once it has come: the new project is added and its Home shown, with New agent the next
    /// step (paper-research N-1446 p2424, 0; Home accounts-lead p2446), or the dialog says why not.
    pub fn new_project_done(&mut self) -> bool {
        let Some(r) = self.new_project.running.as_ref().and_then(|rx| rx.try_recv().ok()) else { return false };
        self.new_project.running = None;
        match r {
            Ok(cfg) => {
                // New project starts it on this PC's server
                let mut p = Proj::new(&cfg, pipe(&self.base, &cfg, None));
                if let (Some(w), Some(pty)) = (&self.stream_wake, &self.pty_wake) {
                    p.link = Some(crate::link::Link::start(p.cmd.clone(), w.clone(), pty.clone()));
                }
                self.projects.push(p);
                self.cur = self.projects.len() - 1;
                // it opens on its Home: its repositories (none yet), its agents and New agent (home.rs; the second
                // to land takes this line, accounts-lead N-1446 p2455)
                self.sel = crate::Sel::View(String::new());
                self.new_project = NewProject::default();
            }
            Err(e) => self.new_project.error = Some(e),
        }
        true
    }
}

/// What [`start`] runs: BASE's `new-project` (agent-base/bin/new-project) for CFG and FILE, on the coordds' machine:
/// through wsl.exe from Windows or with WSL set (places.rs), as it is on Linux. Its folder is "": a new empty `work`
/// in the coord folder. From Windows the file and the coord folder are WSL's, and box.sh and host-coordd are WSL
/// programs, so none of it is done here (docket-core7, N-412 post 286).
fn new_project_cmd(base: &str, cfg: &ProjectConfig, file: &str, wsl: Option<&str>, windows: bool) -> Vec<String> {
    let mut cmd = vec![format!("{base}/bin/new-project"), if cfg.boxed { "box" } else { "host" }.to_string(), cfg.id.clone(), String::new()];
    if !file.is_empty() {
        cmd.push(file.to_string());
    }
    crate::places::server_in(cmd, wsl, windows)
}

/// What the dialog says when the coordds' machine's agent-base has no new-project yet (paper-research, N-547.p11).
const NO_NEW_PROJECT: &str = "needs a newer release";

/// Start CFG's coordd and add CFG to the config at CONFIG.
fn start(base: &str, config: &str, cfg: ProjectConfig, file: &str) -> Result<ProjectConfig, String> {
    let (wsl, windows) = (crate::places::WSL.get().and_then(Option::as_deref), cfg!(windows));
    // a base from before new-project (the checkout comes with yaaihh's release, the exe with docket-core's): said so
    let has = crate::places::server_in(vec!["test".into(), "-x".into(), format!("{base}/bin/new-project")], wsl, windows);
    if crate::places::command(&has[0]).args(&has[1..]).stdin(std::process::Stdio::null()).status().is_ok_and(|s| s.code() == Some(1)) {
        return Err(NO_NEW_PROJECT.to_string());
    }
    let cmd = new_project_cmd(base, &cfg, file, wsl, windows);
    let out = crate::places::command(&cmd[0]).args(&cmd[1..]).stdin(std::process::Stdio::null()).output().map_err(|e| format!("{}: {e}", cmd[0]))?;
    if !out.status.success() {
        let said = [out.stderr, out.stdout].concat();
        let said = String::from_utf8_lossy(&said);
        let why = said.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no message").to_string();
        return Err(format!("new-project failed ({}): {why}", out.status));
    }
    let q = |s: &str| serde_json::to_string(s).unwrap(); // a JSON string is a TOML basic string
    let entry = format!("\n[[project]]\nname = {}\nid = {}\nboxed = {}\n", q(&cfg.name), q(&cfg.id), cfg.boxed);
    use std::io::Write;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(config)
        .and_then(|mut f| f.write_all(entry.as_bytes()))
        .map_err(|e| format!("{config}: {e} ({} runs)", cfg.id))?;
    Ok(cfg)
}

/// The agent page's Rename dialog: agent FROM of project PROJECT (an index) as NAME, by `roster rename` (it waits for a
/// running turn to end, stops the agent, renames it in coordd and resumes it under the new name: its session, history,
/// mail and pins stay).
#[derive(Default)]
pub struct Rename {
    pub open: bool,
    pub project: usize,
    pub from: String,
    pub name: TextEdit,
    pub running: Option<Receiver<Result<String, String>>>,
    pub error: Option<String>,
}

/// The agent page's title: agent NAME, a click on it renames it (owner, 10 Nov).
pub fn rename_title(name: &str, k: &Kit) -> El {
    let n = name.to_string();
    row()
        .key("rename-title")
        .shrink()
        .min_w(0.0)
        .cursor(rune::Cursor::Pointer)
        .help("Click to rename")
        .on_press(move |s: &mut App| {
            if s.rename.running.is_none() {
                s.rename = Rename { open: true, project: s.cur, from: n.clone(), name: TextEdit::new(&n), running: None, error: None };
            }
        })
        .child(text(name.to_string()).key("title").font(k.ty.ui_medium).color(k.c.text_base).truncate())
}

/// Whether NAME may be an agent's name: letters, digits, `-` and `_` (it names a terminal, a cgroup, a folder).
pub(crate) fn agent_name_ok(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Start the rename the dialog holds.
fn rename_now(s: &mut App) {
    let to = s.rename.name.text.trim().to_string();
    if s.rename.running.is_some() || !agent_name_ok(&to) || to == s.rename.from {
        return;
    }
    let (base, wake) = (s.base.clone(), s.wake.clone());
    let Some(p) = s.projects.get_mut(s.rename.project) else { return };
    s.rename.error = None;
    if let Some(rx) = p.ask_roster(&coord_types::Request::RenameAgent { name: s.rename.from.clone(), to: to.clone() }) {
        s.rename.running = Some(rx);
        return;
    }
    let tool = tool(&base, p.boxed, p.device.as_ref());
    s.rename.running = Some(background(vec![tool, "sh".into(), p.id.clone(), "roster".into(), "rename".into(), s.rename.from.clone(), to], wake, p.device.as_ref()));
}

/// The Rename dialog, into LAYER while it is open.
pub fn rename_dialog(s: &App, k: &Kit, layer: &mut Vec<El>) {
    let r = &s.rename;
    if !r.open {
        return;
    }
    let to = r.name.text.trim().to_string();
    let running = r.running.is_some();
    let ok = agent_name_ok(&to) && to != r.from;
    let input = TextInput::new(&r.name)
        .placeholder("Agent name")
        .label("Name")
        .width(None)
        .invalid(!to.is_empty() && !agent_name_ok(&to))
        .disabled(running)
        .on_edit(|s: &mut App| &mut s.rename.name)
        .on_submit(|s: &mut App, _| rename_now(s))
        .build(k);
    let note = match (&r.error, running) {
        (_, true) => (format!("Renaming {} to {to}…", r.from), k.c.text_muted),
        (Some(e), _) => (e.clone(), k.c.danger.fg),
        (None, _) if !to.is_empty() && !agent_name_ok(&to) => ("Letters, digits, - and _ only.".to_string(), k.c.danger.fg),
        (None, _) => (format!("{} restarts under the new name.", r.from), k.c.text_faint),
    };
    let go = Button::new(if running { "Renaming…" } else { "Rename" }).variant(ButtonVariant::Contrast).disabled(running || !ok).on_press(|s: &mut App| rename_now(s)).build(k);
    let cancel = Button::new("Cancel").variant(ButtonVariant::Ghost).disabled(running).on_press(|s: &mut App| s.rename.open = false).build(k);
    let body = col()
        .w_full()
        .px(16.0)
        .pb(16.0)
        .gap(12.0)
        .child(input)
        .child(text(note.0).font(k.ty.small).color(note.1).wrap())
        .child(row().w_full().justify_end().gap(8.0).child(cancel).child(go));
    Dialog::new("rename-agent", format!("Rename {}", r.from))
        .open(true)
        .fit()
        .width(420.0)
        .child(body)
        .on_close(|s: &mut App| {
            if s.rename.running.is_none() {
                s.rename.open = false;
            }
        })
        .build(k, layer);
}

/// A dialog's field: LABEL over INPUT (New project's, New agent's).
fn field(label: &str, input: El, k: &Kit) -> El {
    col().w_full().gap(4.0).child(text(label.to_string()).font(k.ty.small_medium).color(k.c.text_muted)).child(input)
}

/// The New project dialog, into LAYER while it is open.
pub fn dialog(s: &App, k: &Kit, layer: &mut Vec<El>) {
    let np = &s.new_project;
    let id = id_of(&np.name.text);
    // Enter in either field is Create, as in New agent, Rename and Add a repository (proto-window's walk, N-1446 p2612)
    let name = TextInput::new(&np.name)
        .placeholder("My project")
        .label("Name")
        .width(None)
        .on_edit(|s: &mut App| &mut s.new_project.name)
        .on_submit(|s: &mut App, _| s.create_project())
        .build(k);
    let file = TextInput::new(&np.file)
        .placeholder("/path/to/project.toml (optional)")
        .label("Project file")
        .width(None)
        .on_edit(|s: &mut App| &mut s.new_project.file)
        .on_submit(|s: &mut App, _| s.create_project())
        .build(k);
    let running = np.running.is_some();
    let more = np.more || !np.file.text.trim().is_empty();
    let note = match (&np.error, running) {
        (_, true) => (format!("Starting {}…", np.name.text.trim()), k.c.text_muted),
        (Some(e), _) => (e.clone(), k.c.danger.fg),
        (None, _) if id.is_empty() || !more => (String::new(), k.c.text_faint),
        // his words, not ours (paper-research N-1446 p2535); a folder only behind More (p3207, 1)
        (None, _) if np.file.text.trim().is_empty() => (format!("Kept in ~/agent-coord/{id}"), k.c.text_faint),
        (None, _) => (format!("Kept in ~/agent-coord/{id}."), k.c.text_faint),
    };
    let create = Button::new(if running { "Starting…" } else { "Create" })
        .variant(ButtonVariant::Contrast)
        .disabled(running || id.is_empty())
        .on_press(|s: &mut App| s.create_project())
        .build(k);
    let cancel = Button::new("Cancel").variant(ButtonVariant::Ghost).disabled(running).on_press(|s: &mut App| s.new_project.open = false).build(k);
    // the first screen is his project's name; a project file and where it is kept are behind More (p3207, 1)
    let toggle = Button::new(if more { "Less" } else { "More" }).variant(ButtonVariant::Ghost).disabled(running || !np.file.text.trim().is_empty()).on_press(|s: &mut App| s.new_project.more = !s.new_project.more).build(k).key("new-project-more");
    // a project is made on the WSL machine (its coord folder, its unit): from Windows too, new-project runs there
    // through wsl.exe (`start`), so the file is the WSL machine's path (N-744)
    let mut body = col().w_full().px(16.0).pb(16.0).gap(12.0).child(field("Name", name, k));
    if more {
        body = body.child(field("Project file", file, k));
    }
    if !note.0.is_empty() {
        body = body.child(text(note.0).font(k.ty.small).color(note.1).wrap());
    }
    let body = body.child(row().w_full().gap(8.0).child(toggle).child(row().grow(1.0)).child(cancel).child(create));
    Dialog::new("new-project", "New project")
        .open(np.open)
        .fit()
        .width(520.0)
        .child(body)
        .on_close(|s: &mut App| {
            if s.new_project.running.is_none() {
                s.new_project.open = false;
            }
        })
        .build(k, layer);
}

/// The Agents page's New agent dialog (owner m875: "in the test version open a new agent, send it a message ... and
/// just go look at the raw logs"): a plain standing agent NAME on MODEL in project PROJECT (an index), started by
/// `roster add` as the agents file's lines are, the owner its parent; policy's refusal (`run-model`, `add-agent`) is
/// shown in its words. MODELS: what coordd says the project may run (`Request::ModelList`, or `Request::Models` from a
/// coordd before it; Haiku first), read when the dialog opens, as it lists them ([`listed_models`]): by account, by
/// plain name, none that runs no agent. GUESSES: what people guess a model is where its maker is not named, in the
/// same answer ([`models_of`]). READING: coordd's answer, read.
#[derive(Default)]
pub struct NewAgent {
    pub open: bool,
    pub project: usize,
    pub name: TextEdit,
    pub models: Vec<String>,
    pub guesses: BTreeMap<String, Guess>,
    pub model: Option<String>,
    pub pick: rune_kit::ComboboxState,
    /// He picked the model himself: the project's default, read after the list, no longer takes its place.
    pub chosen: bool,
    /// The effort it starts at; none, its model's default (new_agent_fields.rs).
    pub effort: Option<String>,
    /// Its first message; empty, it waits for his (new_agent_fields.rs).
    pub first: TextEdit,
    pub reading: Option<Receiver<Result<String, String>>>,
    pub running: Option<Receiver<Result<String, String>>>,
    pub error: Option<String>,
}

/// The words of a model id that draws, reviews or embeds and takes no agent's turn: New agent lists none of them
/// (docket-edge7's walk, N-1446 p3050 5: `gpt-image-2.6-sunburst`, `codex-auto-review`; paper-research p3051).
const NOT_AGENTS: &[&str] = &["image", "review", "embedding", "tts", "whisper", "transcribe", "moderation", "dall"];

/// Whether model ID can run an agent: none of its words is one of [`NOT_AGENTS`].
fn runs_agents(id: &str) -> bool {
    !id.split(['-', '/', '_']).any(|w| NOT_AGENTS.contains(&w))
}

/// The account model ID runs on, as New agent groups its list (paper-research N-1446 p3051): its place in the list
/// and the heading the Accounts page gives that harness (accounts.rs `FAMILIES`); Codex on ChatGPT for OpenAI's, so
/// a search for either finds them.
fn model_account(id: &str) -> (usize, &'static str) {
    let heading = |h: &str| crate::accounts::FAMILIES.iter().find(|f| f.0 == h).map_or("Other", |f| f.2);
    if id.starts_with("agy/") {
        (3, heading("agy"))
    } else if id.starts_with("opencode") {
        (2, heading("opencode"))
    } else if id.starts_with("claude") {
        (0, heading(crate::accounts::CLAUDE))
    } else if id.starts_with("gpt") || id.starts_with("codex") {
        (1, "Codex · ChatGPT")
    } else {
        (4, "Other")
    }
}

/// A model by its plain name, from its id (paper-research N-1446 p3051): its account's prefix and a date left out, its
/// numbers joined by dots, each word capitalised, the family before its number (`claude-opus-4-1-20250805` is
/// "Opus 4.1", `claude-3-5-haiku-20241022` "Haiku 3.5", `gpt-6.2-sol` "GPT-6.2 Sol", `agy/gemini-3.8-flash-high`
/// "Gemini 3.8 Flash High").
pub(crate) fn plain_model(id: &str) -> String {
    let base = id.rsplit('/').next().unwrap_or(id);
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
    let mut parts: Vec<String> = Vec::new();
    for t in base.split('-').filter(|t| !(t.len() == 8 && digits(t))) {
        match parts.last_mut() {
            Some(last) if digits(t) && last.split('.').all(digits) => {
                last.push('.');
                last.push_str(t);
            }
            _ => parts.push(t.to_string()),
        }
    }
    if parts.len() > 1 && parts[0] == "claude" {
        parts.remove(0);
    }
    if parts.len() == 2 && parts[0].split('.').all(digits) {
        parts.swap(0, 1);
    }
    let word = |t: &str| match t {
        "gpt" | "glm" | "oss" => t.to_uppercase(),
        _ => {
            let mut c = t.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        }
    };
    let mut out: Vec<String> = parts.iter().map(|p| word(p)).collect();
    if out.len() > 1 && out[0] == "GPT" {
        let n = out.remove(1);
        out[0] = format!("GPT-{n}");
    }
    out.join(" ")
}

/// The models New agent lists, of what coordd says the project may run (OUT, one id a line, Haiku first): those that
/// run agents, grouped by account in [`model_account`]'s order, each account's in coordd's order.
pub(crate) fn listed_models(out: &str) -> Vec<String> {
    let mut m: Vec<String> = out.lines().map(str::trim).filter(|l| !l.is_empty() && runs_agents(l)).map(String::from).collect();
    // by account, the newest first within one (paper-research N-1446 p3207, 4); of one number, coordd's order (Haiku
    // first, the dialog's first pick)
    m.sort_by_key(|id| (model_account(id).0, std::cmp::Reverse(model_number(id))));
    m
}

/// The number in a model's plain name ([`plain_model`]), its parts: "Opus 4.1" is [4, 1], "GPT-6.2 Sol" [6, 2]; none:
/// empty, which sorts after every number.
fn model_number(id: &str) -> Vec<u32> {
    plain_model(id)
        .split_whitespace()
        .filter_map(|w| {
            let n = w.rsplit('-').next().unwrap_or(w);
            n.split('.').map(|d| d.parse::<u32>().ok()).collect::<Option<Vec<u32>>>()
        })
        .next()
        .unwrap_or_default()
}

/// What people guess a model is where its maker is not named (the owner's walk, item 29): what coordd sends with the
/// model (`Request::ModelList`), read from the flavor's one `model-guesses.toml`; the window holds no guess of its own
/// and reads no file. GUESS is shown as a guess; READ is where it was read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Guess {
    pub guess: String,
    pub read: String,
}

/// The request for P's models: `Request::ModelList` from a coordd that takes it ([`coord_types::MODEL_LIST_PROTOCOL`]),
/// which brings the guesses; else `Request::Models`, with none.
pub(crate) fn models_asked(p: &Proj) -> coord_types::Request {
    if p.protocol >= coord_types::MODEL_LIST_PROTOCOL {
        coord_types::Request::ModelList
    } else {
        coord_types::Request::Models
    }
}

/// OUT, coordd's answer to [`models_asked`], as the models one per line (what [`listed_models`] reads) and the
/// guesses by model id: a ModelList's JSON list, or Models' lines with no guesses.
pub(crate) fn models_of(out: &str) -> (String, BTreeMap<String, Guess>) {
    let Ok(list) = serde_json::from_str::<Vec<coord_types::harness::ModelEntry>>(out) else { return (out.to_string(), BTreeMap::new()) };
    let lines = list.iter().map(|m| m.id.as_str()).collect::<Vec<_>>().join("\n");
    let guesses = list.into_iter().filter_map(|m| Some((m.id, Guess { guess: m.guess?, read: m.read.unwrap_or_default() }))).collect();
    (lines, guesses)
}

/// New agent's options for MODELS ([`listed_models`]): each by its plain name, its account under it, and its id too
/// where two of one account have one name (a dated id beside its alias); after that, a model's guess in GUESSES,
/// marked as a guess, with where it was read; a provider's default in DEFAULTS (line, id) said first.
pub(crate) fn model_options(models: &[String], guesses: &BTreeMap<String, Guess>, defaults: &[(&str, String)]) -> Vec<rune_kit::ComboboxOption> {
    let named: Vec<(String, &str)> = models.iter().map(|m| (plain_model(m), model_account(m).1)).collect();
    models
        .iter()
        .zip(&named)
        .map(|(id, (name, account))| {
            let twice = named.iter().filter(|(n, a)| n == name && a == account).count() > 1;
            let mut o = rune_kit::ComboboxOption::new(id.clone(), name.clone());
            o.detail = if twice { format!("{account} · {id}") } else { account.to_string() };
            let lines: Vec<&str> = defaults.iter().filter(|(_, m)| m == id).map(|(l, _)| *l).collect();
            if !lines.is_empty() {
                o.detail = format!("Default for {} · {}", lines.join(", "), o.detail);
            }
            if let Some(g) = guesses.get(id) {
                o.detail = format!("{} · guess: {}", o.detail, g.guess);
                if !g.read.is_empty() {
                    o.detail = format!("{} (read: {})", o.detail, g.read);
                }
            }
            o
        })
        .collect()
}

/// Open the New agent dialog on the shown project and ask coordd for its models, with their guesses ([`models_asked`]).
pub fn new_agent_open(s: &mut App) {
    if s.new_agent.running.is_some() {
        s.new_agent.open = true;
        return;
    }
    let Some(p) = s.projects.get(s.cur) else { return };
    let (cmd, wake) = (p.cmd.clone(), s.wake.clone());
    let asked = models_asked(p);
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut r = Err("no answer".to_string());
        for m in crate::link::once(&cmd, &[asked]) {
            if let crate::link::Incoming::Reply(_, x) = m {
                r = if x.ok { Ok(x.out) } else { Err(x.out) };
                break;
            }
        }
        let _ = tx.send(r);
        if let Some(w) = wake {
            w();
        }
    });
    s.new_agent = NewAgent { open: true, project: s.cur, reading: Some(rx), ..Default::default() };
    crate::policy::ask(s, s.cur);
}

/// The dialog's reads and starts that ended since the last frame; true when one did.
pub fn new_agent_ended(s: &mut App) -> bool {
    let defaults = s.projects.get(s.new_agent.project).map(|p| crate::policy::default_models(s, p)).unwrap_or_default();
    let na = &mut s.new_agent;
    let mut any = false;
    // the project's default, once its settings are read, whichever came first, until he picks one himself
    if let Some((_, m)) = defaults.iter().find(|(_, m)| na.models.contains(m)).filter(|(_, m)| !na.chosen && na.model.as_ref() != Some(m)) {
        na.model = Some(m.clone());
        any = true;
    }
    if let Some(r) = na.reading.as_ref().and_then(|rx| rx.try_recv().ok()) {
        na.reading = None;
        any = true;
        match r {
            Ok(out) => {
                let (lines, guesses) = models_of(&out);
                na.models = listed_models(&lines);
                na.guesses = guesses;
                na.model = na.models.first().cloned();
                // the project's first provider default it may run, before the list's first
                if let Some((_, m)) = defaults.iter().find(|(_, m)| na.models.contains(m)) {
                    na.model = Some(m.clone());
                }
                if na.models.is_empty() {
                    na.error = Some("No model available".into());
                }
            }
            Err(e) => na.error = Some(format!("No models: {e}")),
        }
    }
    if let Some(r) = na.running.as_ref().and_then(|rx| rx.try_recv().ok()) {
        na.running = None;
        any = true;
        match r {
            Ok(_) => {
                na.open = false;
                let name = na.name.text.trim().to_string();
                if s.cur == s.new_agent.project {
                    // its page opens with its box taking his keys: his first message is the next step (proto-window's
                    // walk, N-1446 p2612); the box's place on an agent's page is its name (message_box.rs `place`)
                    s.composers.entry((s.cur, name.clone())).or_insert_with(|| TextEdit::new("")).focused = true;
                    s.sel = crate::Sel::Agent(name);
                }
            }
            Err(e) => na.error = Some(e),
        }
    }
    any
}

/// The folder most of P's agents run in, if any of them says one.
fn most_agents_dir(p: &Proj) -> Option<String> {
    let mut dirs: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for d in p.live.agents.iter().filter_map(|a| a.cwd.as_deref()) {
        *dirs.entry(d).or_default() += 1;
    }
    dirs.into_iter().max_by_key(|(d, n)| (*n, std::cmp::Reverse(*d))).map(|(d, _)| d.to_string())
}

/// The folder New agent names in P's `AddAgent`: none to a coordd that says where it starts agents (`places.start`),
/// so its one rule decides (`Core::start_dir`, N-1446 p2455); to one that says nothing of it, a coordd from before that
/// rule, the folder most of P's agents run in, as the window named before: such a coordd (74's) starts an agent named
/// no folder in $COORD, its record folder, p2423's complaint (docket-core p2461, 1).
fn add_agent_dir(p: &Proj) -> Option<String> {
    if p.live.places.as_ref().is_some_and(|x| x.start.is_some()) { None } else { most_agents_dir(p) }
}

/// Start the agent the dialog holds: `roster add NAME MODEL PROMPT DIR` in the project's environment, the prompt
/// written to $COORD/prompts/NAME.md there (a box's $COORD is the box's). Through `AddAgent`, the folder
/// [`add_agent_dir`] gives; a coordd from before `AddAgent` (the shell below) is given the folder most of its agents
/// run in, else $COORD, as before.
fn new_agent_now(s: &mut App) {
    let na = &s.new_agent;
    let name = na.name.text.trim().to_string();
    // the effort rides on the model as roster takes it (MODEL:EFFORT), so its first turn runs at it
    let Some(model) = na.model.clone().map(|m| match &na.effort {
        Some(e) => format!("{m}:{e}"),
        None => m,
    }) else {
        return;
    };
    if na.running.is_some() || !agent_name_ok(&name) {
        return;
    }
    let (base, wake) = (s.base.clone(), s.wake.clone());
    let Some(p) = s.projects.get_mut(na.project) else { return };
    if p.live.agents.iter().any(|a| a.name == name) {
        s.new_agent.error = Some(format!("{name} is already an agent of {}.", p.name));
        return;
    }
    s.new_agent.error = None;
    // no first message: none of ours in its place, so it waits at its prompt for his (N-1997)
    let prompt = s.new_agent.first.text.trim().to_string();
    let req = coord_types::Request::AddAgent { name: name.clone(), model: model.clone(), text: prompt.clone(), dir: add_agent_dir(p) };
    if let Some(rx) = p.ask_roster(&req) {
        s.new_agent.running = Some(rx);
        return;
    }
    let dir = most_agents_dir(p).unwrap_or_default();
    let tool = tool(&base, p.boxed, p.device.as_ref());
    let script = r#"set -e; mkdir -p "$COORD/prompts"; f=$COORD/prompts/$1.md; printf '%s\n' "$3" >"$f"; exec roster add "$1" "$2" "$f" "${4:-$COORD}""#;
    let cmd = vec![tool, "sh".into(), p.id.clone(), "bash".into(), "-c".into(), script.into(), "new-agent".into(), name, model, prompt, dir];
    s.new_agent.error = None;
    s.new_agent.running = Some(background(cmd, wake, p.device.as_ref()));
}

/// The New agent dialog, into LAYER while it is open.
pub fn new_agent_dialog(s: &App, k: &Kit, layer: &mut Vec<El>) {
    let na = &s.new_agent;
    if !na.open {
        return;
    }
    let name = na.name.text.trim().to_string();
    let (running, reading) = (na.running.is_some(), na.reading.is_some());
    let taken = s.projects.get(na.project).is_some_and(|p| p.live.agents.iter().any(|a| a.name == name));
    let ok = agent_name_ok(&name) && !taken && na.model.is_some();
    let input = TextInput::new(&na.name)
        .placeholder("Agent name")
        .label("Name")
        .width(None)
        .invalid(!name.is_empty() && (!agent_name_ok(&name) || taken))
        .disabled(running)
        .on_edit(|s: &mut App| &mut s.new_agent.name)
        .on_submit(|s: &mut App, _| new_agent_now(s))
        .build(k);
    let defaults = s.projects.get(na.project).map(|p| crate::policy::default_models(s, p)).unwrap_or_default();
    let options = crate::new_agent_fields::searched(model_options(&na.models, &na.guesses, &defaults), &na.guesses, &na.pick.search().text);
    let mut pops = Vec::new();
    let pick = rune_kit::Combobox::new("new-agent-model", &na.pick, &options, |s: &mut App| &mut s.new_agent.pick)
        .label("Model")
        .current(na.model.as_deref())
        .placeholder(if reading { "Reading the models…" } else { "Model" })
        .search_placeholder("Model")
        .width(388.0)
        .disabled(running || reading || na.models.is_empty())
        .on_choose(|s: &mut App, m, _| {
            s.new_agent.model = Some(m.to_string());
            s.new_agent.chosen = true;
            s.new_agent.effort = None;
        })
        .build(k, &mut pops);
    // the chosen model's guess whole, which its row may cut short
    let guessed = na.model.as_ref().and_then(|m| Some((plain_model(m), na.guesses.get(m)?))).map(|(name, g)| {
        let read = if g.read.is_empty() { String::new() } else { format!(" Where it was read: {}.", g.read) };
        text(format!("{name}: its maker is not named. People's guess: {}.{read}", g.guess)).font(k.ty.small).color(k.c.text_muted).wrap()
    });
    let project = s.projects.get(na.project).map_or("", |p| p.name.as_str());
    let note = match (&na.error, running) {
        (_, true) => (format!("Starting {name}…"), k.c.text_muted),
        (Some(e), _) => (e.clone(), k.c.danger.fg),
        (None, _) if !name.is_empty() && !agent_name_ok(&name) => ("Letters, digits, - and _ only.".to_string(), k.c.danger.fg),
        (None, _) if taken => (format!("{name} is already an agent of {project}."), k.c.danger.fg),
        (None, _) => (format!("A standing agent of {project}"), k.c.text_faint),
    };
    let go = Button::new(if running { "Starting…" } else { "Start" }).variant(ButtonVariant::Contrast).disabled(running || !ok).on_press(|s: &mut App| new_agent_now(s)).build(k);
    let cancel = Button::new("Cancel").variant(ButtonVariant::Ghost).disabled(running).on_press(|s: &mut App| s.new_agent.open = false).build(k);
    let body = col()
        .w_full()
        .px(16.0)
        .pb(16.0)
        .gap(12.0)
        // each field's name over it, as New project's (paper-research N-1446 p3207, 4)
        .child(field("Name", input, k))
        .child(field("Model", pick, k))
        .children(guessed)
        .children(crate::new_agent_fields::effort(s, k).map(|e| field("Effort", e, k)))
        .child(field("First message", crate::new_agent_fields::first_message(s, running, k), k))
        .child(text(note.0).font(k.ty.small).color(note.1).wrap())
        .child(row().w_full().justify_end().gap(8.0).child(cancel).child(go));
    Dialog::new("new-agent", "New agent")
        .open(true)
        .fit()
        .width(420.0)
        .child(body)
        .on_close(|s: &mut App| {
            if s.new_agent.running.is_none() {
                s.new_agent.open = false;
            }
        })
        .build(k, layer);
    layer.extend(pops);
}

/// A project's context menu, at a secondary press on its row in the sidebar (N-412 ask 5). Its one entry so far sets
/// slow over the whole project or lifts it: the owner's act, which the policy never refuses him (the app is his), sent
/// as the request a ticket's switch sends with `project` for its scope. A project whose coordd is not reached offers it
/// disabled; an older coordd, which knows no project scope, refuses it and says so.
pub fn menu(s: &App, k: &Kit, layer: &mut Vec<El>) {
    use rune_kit::{MenuItem, context_menu};
    let Some((i, at)) = s.project_menu else { return };
    let Some(p) = s.projects.get(i) else { return };
    let on = p.live.slow_project();
    let item = MenuItem::new("Slow: the whole project")
        .checked(on)
        .disabled(p.down.is_some())
        .on_select(move |s: &mut App| {
            s.project_menu = None;
            let req = coord_types::Request::Slow { t: "project".into(), on: !on };
            s.request_in(i, req, crate::Pending::TicketDone { clear: crate::threads::Cleared::Quiet });
        });
    // the press's own point is the anchor; the trigger is only the menu's place to give focus back to
    let menu = context_menu(format!("project-menu-{i}"), rune::el(), Some(at)).item(item).on_close(|s: &mut App| s.project_menu = None).build(k, layer);
    layer.push(menu);
}

#[cfg(test)]
mod device_tests {
    use super::{Config, pipe, tool};
    use crate::places::Device;

    /// A project names its server's device: its pipe goes there by ssh, with the device's own agent-base; one with no
    /// device stays on this PC's server, `box` and `boxed`'s default read as before; a missing device, one named twice
    /// or an ssh that is not a plain USER@HOST leaves the project out, with why (docs/specs/devices.md step 2).
    #[test]
    fn a_project_on_another_device_is_reached_by_ssh_and_a_wrong_device_leaves_it_out() {
        let cfg: Config = toml::from_str(
            r#"
            agent_base = "/b/agent-base"
            [[device]]
            name = "tower"
            ssh = "t@tower-server"
            agent_base = "/t/agent-base"
            [[device]]
            name = "odd"
            ssh = "-oProxyCommand=x"
            [[device]]
            name = "twice"
            ssh = "a@b"
            [[device]]
            name = "twice"
            ssh = "c@d"
            [[project]]
            name = "Research"
            id = "research"
            boxed = false
            device = "tower"
            [[project]]
            name = "Here"
            box = "here"
            [[project]]
            name = "Lost"
            id = "lost"
            device = "nowhere"
            [[project]]
            name = "Odd"
            id = "odd"
            device = "odd"
            [[project]]
            name = "Twice"
            id = "twice"
            device = "twice"
            "#,
        )
        .unwrap();
        let (placed, refused) = cfg.placed();
        let got: Vec<_> = placed.iter().map(|(p, d)| (p.id.as_str(), p.boxed, d.as_ref().map(|d| d.name.as_str()))).collect();
        assert_eq!(got, [("research", false, Some("tower")), ("here", true, None)]);
        assert_eq!(
            refused,
            [
                "project Lost is left out: there is no device nowhere",
                "project Odd is left out: device odd's ssh is not a USER@HOST (-oProxyCommand=x)",
                "project Twice is left out: there are 2 devices named twice",
            ]
        );

        let (research, tower) = &placed[0];
        let c = pipe("/b/agent-base", research, tower.as_ref());
        let at = c.iter().position(|w| w == "--").unwrap();
        assert_eq!(c[0], "ssh");
        assert_eq!(c[at + 1..at + 6], ["t@tower-server", "/t/agent-base/bin/host-coordd", "sh", "research", "coordc"]);
        let (here, none) = &placed[1];
        let c = pipe("/b/agent-base", here, none.as_ref());
        assert!(c[0] != "ssh" && c.contains(&"/b/agent-base/box/box.sh".to_string()), "{c:?}");

        let plain = Device { name: "p".into(), ssh: "u@p".into(), agent_base: None };
        assert_eq!(tool("/b/agent-base", false, Some(&plain)), "/b/agent-base/bin/host-coordd", "no agent-base of its own: the config's");
    }
}

#[cfg(test)]
mod new_project_tests {
    use super::{ProjectConfig, new_project_cmd};

    /// New project runs agent-base's new-project on the coordds' machine: through wsl.exe from Windows (the file is
    /// WSL's), as it is on Linux; with no folder, so its agents work in a new empty `work` until a repository is added
    /// on its Home (accounts-lead N-1446 p2455); the project file only when given (docket-core7, N-412 post 286).
    #[test]
    fn new_project_runs_on_the_coordds_machine() {
        let host = ProjectConfig { name: "My project".into(), id: "my-project".into(), boxed: false };
        let boxed = ProjectConfig { boxed: true, ..host.clone() };
        assert_eq!(new_project_cmd("/b/agent-base", &host, "", None, false), ["/b/agent-base/bin/new-project", "host", "my-project", ""]);
        assert_eq!(
            new_project_cmd("/b/agent-base", &boxed, "/p/project.toml", Some("Ubuntu-main"), true),
            ["wsl.exe", "-d", "Ubuntu-main", "-e", "/b/agent-base/bin/new-project", "box", "my-project", "", "/p/project.toml"]
        );
        assert_eq!(new_project_cmd("/b/agent-base", &host, "", None, true)[..3], ["wsl.exe", "-e", "/b/agent-base/bin/new-project"]);
    }

    /// A base without new-project: the dialog says what is missing, and nothing is run or written (on Linux, the
    /// machine the test runs on, a base folder with no bin/new-project).
    #[test]
    #[cfg(not(windows))]
    fn a_base_without_new_project_says_what_is_missing() {
        let base = std::env::temp_dir().join(format!("np-base-{}", std::process::id()));
        std::fs::create_dir_all(base.join("bin")).unwrap();
        let config = base.join("projects.toml");
        let cfg = ProjectConfig { name: "X".into(), id: "x".into(), boxed: false };
        let r = super::start(&base.to_string_lossy(), &config.to_string_lossy(), cfg, "");
        assert_eq!(r.err().as_deref(), Some(super::NO_NEW_PROJECT));
        assert!(!config.exists());
        std::fs::remove_dir_all(&base).unwrap();
    }
}

#[cfg(test)]
mod hold_tests {
    /// From Windows the window starts work-hold in WSL, so closing it leaves the agents up; on Linux nothing is started.
    #[test]
    fn on_windows_the_window_starts_work_hold_through_wsl_and_on_linux_nothing() {
        assert_eq!(super::hold_cmd("/b/agent-base", false), None);
        assert_eq!(super::hold_cmd("", true), None);
        let cmd = super::hold_cmd("/b/agent-base", true).unwrap();
        assert_eq!(cmd.last().map(String::as_str), Some("/b/agent-base/bin/work-hold"));
        assert_eq!(cmd[0], "wsl.exe");
    }

    /// The real start, run by hand from WSL as the Windows test program (no window is opened): `hold` of the
    /// agent-base YAAIHH_HOLD_TRY_BASE names starts its work-hold and returns at once; the holder outlives this
    /// program, which the caller checks after it ends (docket-edge3's try, N-433 p45).
    #[test]
    #[ignore]
    fn hold_starts_work_hold_and_returns_at_once() {
        let base = std::env::var("YAAIHH_HOLD_TRY_BASE").expect("YAAIHH_HOLD_TRY_BASE");
        let at = std::time::Instant::now();
        super::hold(&base);
        assert!(at.elapsed() < std::time::Duration::from_secs(2), "hold waited {:?}", at.elapsed());
    }
}

#[cfg(test)]
mod new_agent_tests {
    /// Where New agent says to start an agent (docket-core N-1446 p2461, 1): to a coordd that sends `places.start`, no
    /// folder, so its one rule decides; to one that sends none (74's, until its restart, or after a way back), the
    /// folder most of the project's agents run in, as the window named before, never nothing, which such a coordd would
    /// fill with its record folder. With no agent there, none either way.
    #[test]
    fn new_agent_names_a_folder_only_to_a_coordd_that_does_not_say_where_it_starts_agents() {
        let mut p = crate::view_cost::project_for_tests();
        let cwd = |p: &mut crate::Proj, i: usize, d: &str| p.live.agents[i].cwd = Some(d.into());
        cwd(&mut p, 0, "/home/u/tovrin");
        cwd(&mut p, 1, "/home/u/papers");
        cwd(&mut p, 2, "/home/u/papers");
        p.live.places = None;
        assert_eq!(super::add_agent_dir(&p).as_deref(), Some("/home/u/papers"), "no snapshot's places yet: as before");
        p.live.places = Some(coord_types::Places { coord: "/home/u/agent-coord/research".into(), ..Default::default() });
        assert_eq!(super::add_agent_dir(&p).as_deref(), Some("/home/u/papers"), "a coordd that sends no start");
        p.live.places.as_mut().unwrap().start = Some("/home/u/agent-coord/research/work".into());
        assert_eq!(super::add_agent_dir(&p), None, "its rule decides");
        p.live.places.as_mut().unwrap().start = None;
        for a in &mut p.live.agents {
            a.cwd = None;
        }
        assert_eq!(super::add_agent_dir(&p), None, "no agent says a folder");
    }

    /// New agent's list (docket-edge7's walk, N-1446 p3050 5; paper-research p3051): of coordd's ids, Haiku first, none
    /// that draws or reviews; Claude's, then ChatGPT's, OpenCode's and Antigravity's, each newest first; each by
    /// its plain name with its account under it, a dated id's beside its alias's of one name.
    #[test]
    fn new_agent_lists_plain_names_by_account_and_no_image_or_review_model() {
        let out = "claude-haiku-5-5\nclaude-3-5-haiku-20241022\nagy/gemini-3.8-flash-high\ncodex-auto-review\nclaude-opus-4-1-20250805\n\
                   gpt-6.2-sol\ngpt-image-2.6-sunburst\nopencode-go/glm-5\nclaude-opus-5-5\nagy/claude-sonnet-4-6\ngpt-6-chat-thinking\n\
                   claude-sonnet-4-20250514\nclaude-sonnet-4\n\n";
        let listed = super::listed_models(out);
        // the newest first within an account, of one number in coordd's order (p3207, 4)
        assert_eq!(
            listed,
            [
                "claude-haiku-5-5", "claude-opus-5-5", "claude-opus-4-1-20250805", "claude-sonnet-4-20250514", "claude-sonnet-4", "claude-3-5-haiku-20241022",
                "gpt-6.2-sol", "gpt-6-chat-thinking", "opencode-go/glm-5", "agy/claude-sonnet-4-6", "agy/gemini-3.8-flash-high",
            ]
        );
        let shown: Vec<(String, String)> = super::model_options(&listed, &Default::default(), &[]).into_iter().map(|o| (o.label, o.detail)).collect();
        let want = [
            ("Haiku 5.5", "Claude"),
            ("Opus 5.5", "Claude"),
            ("Opus 4.1", "Claude"),
            ("Sonnet 4", "Claude · claude-sonnet-4-20250514"),
            ("Sonnet 4", "Claude · claude-sonnet-4"),
            ("Haiku 3.5", "Claude"),
            ("GPT-6.2 Sol", "Codex · ChatGPT"),
            ("GPT-6 Chat Thinking", "Codex · ChatGPT"),
            ("GLM 5", "OpenCode"),
            ("Sonnet 4.6", "Antigravity"),
            ("Gemini 3.8 Flash High", "Antigravity"),
        ];
        assert_eq!(shown, want.map(|(a, b)| (a.to_string(), b.to_string())));
        assert_eq!(super::plain_model("claude-3-7-sonnet-20250219"), "Sonnet 3.7");
        assert_eq!(super::plain_model("claude-fable-5-2"), "Fable 5.2");
        assert_eq!(super::listed_models("gpt-image-1.5\ncodex-auto-review\n"), Vec::<String>::new(), "nothing to run an agent on");
    }

    /// A model whose maker is not named (the owner's walk, item 29): its guess, in coordd's model list
    /// (`Request::ModelList`), on its row after its account, marked as a guess with where it was read; a model with no
    /// guess as it was. A coordd before the list (`Request::Models`' lines) gives the models with no guesses.
    #[test]
    fn a_model_with_no_named_maker_shows_its_guess_and_where_it_was_read() {
        use coord_types::harness::ModelEntry;
        let e = |id: &str, guess: Option<&str>, read: Option<&str>| ModelEntry { id: id.into(), guess: guess.map(str::to_string), read: read.map(str::to_string) };
        let sent = serde_json::to_string(&[
            e("claude-haiku-5-5", None, None),
            e("opencode-go/ann-free", Some("bob's model"), Some("https://example.org/ann")),
            e("opencode-go/cy-free", Some("cy's"), None),
            e("opencode-go/dee-free", None, Some("no guess")),
            e("opencode-go/glm-5", None, None),
        ])
        .unwrap();
        let (lines, guesses) = super::models_of(&sent);
        assert_eq!(guesses.keys().map(String::as_str).collect::<Vec<_>>(), ["opencode-go/ann-free", "opencode-go/cy-free"], "dee's has no guess");
        let listed = super::listed_models(&lines);
        let shown: Vec<(String, String)> = super::model_options(&listed, &guesses, &[]).into_iter().map(|o| (o.label, o.detail)).collect();
        let want = [
            ("Haiku 5.5", "Claude"),
            ("GLM 5", "OpenCode"),
            ("Ann Free", "OpenCode · guess: bob's model (read: https://example.org/ann)"),
            ("Cy Free", "OpenCode · guess: cy's"),
            ("Dee Free", "OpenCode"),
        ];
        assert_eq!(shown, want.map(|(a, b)| (a.to_string(), b.to_string())));
        let (old, none) = super::models_of("claude-haiku-5-5\nopencode-go/ann-free\n");
        assert_eq!((old.as_str(), none.len()), ("claude-haiku-5-5\nopencode-go/ann-free\n", 0), "an older coordd's lines");
    }
}

#[cfg(test)]
mod dialog_focus_tests {
    use crate::view_cost::{press, type_in, window};

    fn app() -> (std::sync::MutexGuard<'static, ()>, crate::App) {
        let (held, mut s) = crate::view_cost::test_app();
        s.projects = vec![crate::view_cost::project_for_tests()];
        s.cur = 0;
        (held, s)
    }

    /// The key that has focus, the field's or the button's (rune keys a text field `input`, a button by its own key).
    fn focused(rt: &rune::runtime::Runtime<crate::App>) -> Option<String> {
        rt.elements().into_iter().find(|e| e.focused).map(|e| e.path)
    }

    /// A dialog opens with its first field focused, so his first key goes into it with no click first (paper-research
    /// N-1446 p2527, 1; docket-edge8 p2505): New project as the sidebar's + opens it, New agent as its press on Home or
    /// the Agents page does (its models asked of a coordd this test has none of), and an agent's Rename. Escape still
    /// closes each, and nothing typed went anywhere else.
    #[test]
    fn a_dialog_opens_with_its_first_field_focused() {
        let (_held, s) = app();
        let mut rt = window(s);
        rt.update(|s, _| s.new_project.open = true);
        rt.frame(0.016);
        type_in(&mut rt, "notes");
        assert_eq!(rt.state.new_project.name.text, "notes", "New project: typed into Name (focus on {:?})", focused(&rt));
        // its file alone: 76 takes the Folder field out (accounts-lead p2568, path-nofolder e670fdba)
        assert_eq!(rt.state.new_project.file.text, "", "nothing typed into its project file");
        press(&mut rt, rune::Key::Escape);
        assert!(!rt.state.new_project.open, "Escape closes New project");
        rt.update(|s, _| crate::projects::new_agent_open(s));
        rt.frame(0.016);
        type_in(&mut rt, "try-2");
        assert_eq!(rt.state.new_agent.name.text, "try-2", "New agent: typed into Name (focus on {:?})", focused(&rt));
        press(&mut rt, rune::Key::Escape);
        assert!(!rt.state.new_agent.open, "Escape closes New agent");
        let from = rt.state.projects[0].live.agents[0].name.clone();
        rt.update(move |s, _| {
            s.rename = super::Rename { open: true, project: 0, from: from.clone(), name: rune::TextEdit::new(""), running: None, error: None };
        });
        rt.frame(0.016);
        type_in(&mut rt, "lead");
        assert_eq!(rt.state.rename.name.text, "lead", "Rename: typed into its field (focus on {:?})", focused(&rt));
    }

    /// New project's first screen is his project's name (paper-research N-1446 p3207, 1; docket-edge8's walk on build 89,
    /// p3205): no project file field and no folder path until More is pressed; then both, and Less folds them again.
    #[test]
    fn new_project_shows_the_file_and_folder_only_behind_more() {
        let (_held, s) = app();
        let mut rt = window(s);
        rt.update(|s, _| s.new_project.open = true);
        rt.frame(0.016);
        type_in(&mut rt, "Scratch");
        rt.frame(0.016);
        let texts = |rt: &rune::runtime::Runtime<crate::App>| rt.elements().into_iter().filter(|e| !e.exiting && e.path.contains("new-project")).filter_map(|e| e.text.or(e.label)).collect::<Vec<_>>();
        let first = texts(&rt);
        assert!(first.iter().any(|t| t == "Name") && first.iter().any(|t| t == "More"), "{first:?}");
        assert!(!first.iter().any(|t| t == "Project file" || t.contains("agent-coord") || t.contains("project.toml")), "the first screen: {first:?}");
        let r = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some("new-project-more")).expect("More").rect;
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        for _ in 0..3 {
            rt.frame(0.016);
        }
        let more = texts(&rt);
        assert!(more.iter().any(|t| t == "Project file") && more.iter().any(|t| t.starts_with("Kept in ~/agent-coord/scratch")) && more.iter().any(|t| t == "Less"), "behind More: {more:?}");
    }

    /// New agent's two fields say what they are, over each, as New project's do (paper-research N-1446 p3207, 4;
    /// docket-edge8's walk on build 89, p3205): "Name" and "Model" drawn as words, not only read out.
    #[test]
    fn new_agent_names_its_two_fields() {
        let (_held, s) = app();
        let mut rt = window(s);
        rt.update(|s, _| {
            s.new_agent = super::NewAgent { open: true, project: 0, name: rune::TextEdit::new("try-1"), models: vec!["claude-haiku-5-5".into()], model: Some("claude-haiku-5-5".into()), ..Default::default() };
        });
        for _ in 0..3 {
            rt.frame(0.016);
        }
        let words: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting && e.path.contains("new-agent")).filter_map(|e| e.text).collect();
        assert!(words.iter().any(|w| w == "Name") && words.iter().any(|w| w == "Model"), "{words:?}");
    }

    /// His new-project path takes his keys (proto-window's walk on 75, N-1446 p2612). Enter in New project's Name asks
    /// the start as Create does, and a second Enter while it runs asks nothing more. The stand-in agent-base's
    /// new-project only counts its runs and refuses, so nothing is made on this machine and the dialog says why.
    /// New agent's start, once roster says yes, opens the agent's page with its box taking his first message's keys.
    #[test]
    #[cfg(unix)]
    fn enter_makes_the_project_and_the_new_agents_page_takes_his_first_words() {
        use std::os::unix::fs::PermissionsExt;
        let (_held, mut s) = app();
        let base = std::env::temp_dir().join(format!("yaaihh-enter-base-{}", std::process::id()));
        std::fs::create_dir_all(base.join("bin")).unwrap();
        let _ = std::fs::remove_file(base.join("asked"));
        let np = base.join("bin/new-project");
        std::fs::write(&np, "#!/bin/sh\necho \"$2\" >> \"$(dirname \"$0\")/../asked\"\necho 'stand-in: nothing made' >&2\nexit 1\n").unwrap();
        std::fs::set_permissions(&np, std::fs::Permissions::from_mode(0o755)).unwrap();
        s.base = base.display().to_string();
        let mut rt = window(s);
        rt.update(|s, _| s.new_project.open = true);
        rt.frame(0.016);
        type_in(&mut rt, "notes");
        press(&mut rt, rune::Key::Enter);
        assert!(rt.state.new_project.running.is_some(), "Enter in Name starts it, as Create does (focus on {:?})", focused(&rt));
        press(&mut rt, rune::Key::Enter);
        let t = std::time::Instant::now();
        while !rt.state.new_project_done() {
            assert!(t.elapsed() < std::time::Duration::from_secs(10), "the stand-in's start ends");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        rt.frame(0.016);
        assert_eq!(std::fs::read_to_string(base.join("asked")).unwrap(), "notes\n", "one start for two Enters");
        assert!(rt.state.new_project.error.as_deref().is_some_and(|e| e.ends_with("stand-in: nothing made")), "{:?}", rt.state.new_project.error);
        press(&mut rt, rune::Key::Escape);
        assert!(!rt.state.new_project.open, "Escape closes New project");

        // New agent's start, and roster's yes: try-2's page, its box taking his keys with no click
        let (tx, rx) = std::sync::mpsc::channel();
        rt.update(move |s, _| {
            let mut a = s.projects[0].live.agents[0].clone();
            a.name = "try-2".into();
            s.projects[0].live.agents.push(a);
            s.new_agent = super::NewAgent { open: true, project: 0, name: rune::TextEdit::new("try-2"), models: vec!["claude-haiku-5-5".into()], model: Some("claude-haiku-5-5".into()), running: Some(rx), ..Default::default() };
        });
        rt.frame(0.016);
        tx.send(Ok("try-2: started".to_string())).unwrap();
        rt.update(|s, _| assert!(super::new_agent_ended(s), "roster's yes is taken"));
        for _ in 0..3 {
            rt.frame(0.016);
        }
        assert_eq!(rt.state.sel, crate::Sel::Agent("try-2".into()), "its page opens");
        type_in(&mut rt, "hello");
        assert_eq!(rt.state.composers.get(&(0, "try-2".to_string())).map(|e| e.text.as_str()), Some("hello"), "its page takes his first words (focus on {:?})", focused(&rt));
    }

    /// The stills (STILL_OUT, a stand-in window: the made-up project, no coordd): New project and New agent just
    /// opened, nothing pressed after, one a process: `STILL_OUT=DIR yaaihh-tests
    /// projects::dialog_focus_tests::still_new_project --ignored --exact` (and `still_new_agent`).
    fn still(name: &str, init: fn() -> crate::App) {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) }).args(args).try_run().unwrap();
    }

    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn still_new_project() {
        fn init() -> crate::App {
            let (_held, mut s) = app();
            // the test's own folder has no projects.toml: none of that in a still
            s.notice = None;
            s.new_project.open = true;
            s
        }
        still("1-new-project-opened.png", init);
    }

    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn still_new_agent() {
        fn init() -> crate::App {
            let (_held, mut s) = app();
            s.notice = None;
            s.new_agent = super::NewAgent { open: true, project: 0, models: vec!["claude-haiku-5-5".into(), "claude-opus-5-5".into()],
                model: Some("claude-haiku-5-5".into()), ..Default::default() };
            s
        }
        still("2-new-agent-opened.png", init);
    }

    /// New agent with the guesses of the file STILL_GUESSES (the flavor's `model-guesses.toml`), its models beside Haiku
    /// and GLM 5, the first of them chosen.
    fn guessed() -> crate::App {
        let (_held, mut s) = app();
        s.notice = None;
        // the file as coordd's model list carries it (coordd's model_list)
        let file = std::env::var("STILL_GUESSES").ok().and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default();
        let table: toml::Table = file.parse().unwrap_or_default();
        let entries: Vec<serde_json::Value> = table.iter().map(|(id, v)| serde_json::json!({"id": id, "guess": v.get("guess").and_then(|g| g.as_str()), "read": v.get("read").and_then(|r| r.as_str())})).collect();
        let (_, guesses) = super::models_of(&serde_json::Value::from(entries).to_string());
        let ids: Vec<String> = guesses.keys().cloned().collect();
        let models = super::listed_models(&format!("claude-haiku-5-5\nopencode-go/glm-5\n{}", ids.join("\n")));
        s.new_agent = super::NewAgent { open: true, project: 0, name: rune::TextEdit::new("try-1"), models, guesses, model: ids.first().cloned(), ..Default::default() };
        s
    }

    /// The Model list open, each guessed model's guess on its row (`STILL_GUESSES=FILE STILL_OUT=DIR`).
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn still_new_agent_guesses_listed() {
        fn init() -> crate::App {
            let mut s = guessed();
            let na = &mut s.new_agent;
            let _ = na.pick.step(rune_kit::ComboboxEvent::Toggle { current: na.model.as_deref() });
            s
        }
        still("4-new-agent-guesses-listed.png", init);
    }

    /// The list closed, the chosen model's guess whole under it.
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn still_new_agent_guess_chosen() {
        still("5-new-agent-guess-chosen.png", guessed);
    }

    /// try-2's page as New agent's Start opens it, roster's yes taken: its box holds the caret (N-1446 p2612, 2).
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn still_new_agents_page() {
        fn init() -> crate::App {
            let (_held, mut s) = app();
            s.notice = None;
            let mut a = s.projects[0].live.agents[0].clone();
            a.name = "try-2".into();
            a.state = "idle".into();
            s.projects[0].live.agents.push(a);
            let (tx, rx) = std::sync::mpsc::channel();
            tx.send(Ok("try-2: started".to_string())).unwrap();
            s.new_agent = super::NewAgent { open: true, project: 0, name: rune::TextEdit::new("try-2"), running: Some(rx), ..Default::default() };
            super::new_agent_ended(&mut s);
            s
        }
        still("3-new-agents-page.png", init);
    }
}
