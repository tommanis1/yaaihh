//! yaaihh's projects (design §3; amendments, 7 Nov evening): the config (`~/.config/yaaihh/projects.toml`, or the file
//! YAAIHH_CONFIG names), where each project is one coordd, in a box or on the host, read through its pipe; and the New
//! project dialog, which starts a project's coordd and adds the project to the config.
//!
//!   [[project]]
//!   name  = "My box"
//!   box   = "my-box"          (or `id`: the box's name, or the host coordd's; its coord folder is ~/agent-coord/ID)
//!   boxed = false             (coordd and the agents on the host as the owner's user, bin/host-coordd; default true)

use crate::{App, Proj};
use rune::{El, TextEdit, col, row, text};
use rune_kit::{Button, ButtonVariant, Dialog, Kit, Switch, TextInput};
use serde::Deserialize;
use std::process::Command;
use std::sync::mpsc::{Receiver, channel};

#[derive(Deserialize, Default)]
pub struct Config {
    pub agent_base: Option<String>,
    /// The folders a relative file path in a chat is looked for in (`~` allowed), after the chat's own folder.
    #[serde(default)]
    pub search: Vec<String>,
    #[serde(default)]
    pub project: Vec<ProjectConfig>,
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

/// The config file: YAAIHH_CONFIG, else ~/.config/yaaihh/projects.toml.
pub fn config_path() -> String {
    std::env::var("YAAIHH_CONFIG").unwrap_or_else(|_| format!("{}/.config/yaaihh/projects.toml", std::env::var("HOME").unwrap_or_default()))
}

/// The pipe to P's coordd: `coordc bridge` inside its box (`box.sh sh`), or with its host coordd's environment
/// (`host-coordd sh`). Both put agent-base's bin first on PATH.
pub fn pipe(base: &str, p: &ProjectConfig) -> Vec<String> {
    let tool = if p.boxed { format!("{base}/box/box.sh") } else { format!("{base}/bin/host-coordd") };
    vec![tool, "sh".into(), p.id.clone(), "coordc".into(), "bridge".into()]
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
    let cmd = if p.boxed { vec![format!("{base}/box/box.sh"), "start".into(), p.id.clone()] } else { vec![format!("{base}/bin/host-coordd"), "start".into(), p.id.clone()] };
    p.starting = Some(background(cmd, wake));
}

/// CMD run off the frame, its output's last line or why it failed on the receiver, logged, and the window woken.
fn background(cmd: Vec<String>, wake: Option<rune::Waker>) -> Receiver<Result<String, String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let said = cmd.join(" ");
        crate::log::line(&format!("run: {said}"));
        let last = |b: &[u8]| String::from_utf8_lossy(b).lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no message").to_string();
        let r = match Command::new(&cmd[0]).args(&cmd[1..]).stdin(std::process::Stdio::null()).output() {
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

/// Fork agent NAME of the current project: `roster fork NAME` through its box or host coordd, a new agent on a copy of
/// NAME's conversation with the owner its parent (agent-base bin/roster). Shown when it is up (`brought_up`).
pub fn fork(s: &mut App, name: &str) {
    let (base, wake) = (s.base.clone(), s.wake.clone());
    let Some(p) = s.projects.get_mut(s.cur) else { return };
    if p.forking.is_some() {
        return;
    }
    let tool = if p.boxed { format!("{base}/box/box.sh") } else { format!("{base}/bin/host-coordd") };
    p.forking = Some(background(vec![tool, "sh".into(), p.id.clone(), "roster".into(), "fork".into(), name.into()], wake));
}

/// The bring-ups that ended since the last frame; true when one did.
pub fn brought_up(s: &mut App) -> bool {
    let mut any = false;
    let mut forked = None;
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
    }
    // roster fork's last line is "NEW: forked from NAME (session ...)": the new agent is shown
    match forked {
        Some((i, Ok(said))) if i == s.cur => s.sel = crate::Sel::Agent(said.split(':').next().unwrap_or_default().to_string()),
        Some((_, Err(e))) => s.notice = Some(e),
        _ => {}
    }
    any
}

/// A project's coord folder (the box's mount, or the host coordd's COORD).
fn coord_dir(id: &str) -> String {
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
    /// The repository a box clones, or the folder a host project's agents work in.
    pub dir: TextEdit,
    /// The project file (views, rights, sources), named in the coord folder's `project`; may be left empty.
    pub file: TextEdit,
    pub boxed: bool,
    pub error: Option<String>,
    /// The start running: its outcome comes here.
    pub running: Option<Receiver<Result<ProjectConfig, String>>>,
}

impl NewProject {
    pub fn new() -> NewProject {
        NewProject { boxed: true, ..Default::default() }
    }
}

impl App {
    /// Start the project the dialog describes, off the UI thread: its coord folder with its `project` (and `repo`)
    /// written, then `box.sh up ID REPO` (boxed) or `host-coordd start ID`, then its entry appended to the config.
    pub fn create_project(&mut self) {
        let np = &self.new_project;
        let (name, dir, file, boxed) = (np.name.text.trim().to_string(), np.dir.text.trim().to_string(), np.file.text.trim().to_string(), np.boxed);
        let id = id_of(&name);
        let refused = if id.is_empty() {
            Some("a name with letters or digits".to_string())
        } else if self.projects.iter().any(|p| p.id == id) {
            Some(format!("there is a project {id} already"))
        } else if !std::path::Path::new(&dir).is_dir() {
            Some(format!("no folder {dir}"))
        } else if !file.is_empty() && !std::path::Path::new(&file).is_file() {
            Some(format!("no file {file}"))
        } else {
            None
        };
        if refused.is_some() {
            self.new_project.error = refused;
            return;
        }
        let cfg = ProjectConfig { name, id, boxed };
        let (base, config, wake) = (self.base.clone(), self.config_path.clone(), self.wake.clone());
        let (tx, rx) = channel();
        self.new_project.error = None;
        self.new_project.running = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(start(&base, &config, cfg, &dir, &file));
            if let Some(w) = wake {
                w();
            }
        });
    }

    /// The start's outcome, once it has come: the new project is added and shown, or the dialog says why not.
    pub fn new_project_done(&mut self) -> bool {
        let Some(r) = self.new_project.running.as_ref().and_then(|rx| rx.try_recv().ok()) else { return false };
        self.new_project.running = None;
        match r {
            Ok(cfg) => {
                let mut p = Proj::new(&cfg, pipe(&self.base, &cfg));
                if let Some(w) = &self.wake {
                    p.link = Some(crate::link::Link::start(p.cmd.clone(), w.clone()));
                }
                self.projects.push(p);
                self.cur = self.projects.len() - 1;
                self.new_project = NewProject::new();
            }
            Err(e) => self.new_project.error = Some(e),
        }
        true
    }
}

/// Start CFG's coordd and add CFG to the config at CONFIG.
fn start(base: &str, config: &str, cfg: ProjectConfig, dir: &str, file: &str) -> Result<ProjectConfig, String> {
    let coord = coord_dir(&cfg.id);
    let wrote = (|| {
        std::fs::create_dir_all(&coord)?;
        if !file.is_empty() {
            std::fs::write(format!("{coord}/project"), format!("{file}\n"))?;
        }
        if !cfg.boxed {
            std::fs::write(format!("{coord}/repo"), format!("{dir}\n"))?; // box.sh up writes a box's itself
        }
        std::io::Result::Ok(())
    })();
    wrote.map_err(|e| format!("{coord}: {e}"))?;
    let mut cmd = if cfg.boxed {
        let mut c = Command::new(format!("{base}/box/box.sh"));
        c.args(["up", &cfg.id, dir]);
        c
    } else {
        let mut c = Command::new(format!("{base}/bin/host-coordd"));
        c.args(["start", &cfg.id]);
        c
    };
    let out = cmd.output().map_err(|e| format!("{:?}: {e}", cmd.get_program()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let why = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no message").to_string();
        return Err(format!("{} failed ({}): {why}", if cfg.boxed { "box.sh up" } else { "host-coordd start" }, out.status));
    }
    let q = |s: &str| serde_json::to_string(s).unwrap(); // a JSON string is a TOML basic string
    let entry = format!("\n[[project]]\nname = {}\nid = {}\nboxed = {}\n", q(&cfg.name), q(&cfg.id), cfg.boxed);
    use std::io::Write;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(config)
        .and_then(|mut f| f.write_all(entry.as_bytes()))
        .map_err(|e| format!("{config}: {e} (the coordd of {} runs)", cfg.id))?;
    Ok(cfg)
}

/// The New project dialog, into LAYER while it is open.
pub fn dialog(s: &App, k: &Kit, layer: &mut Vec<El>) {
    let np = &s.new_project;
    let id = id_of(&np.name.text);
    let field = |label: &str, input: El| col().w_full().gap(4.0).child(text(label.to_string()).font(k.ty.small_medium).color(k.c.text_muted)).child(input);
    let name = TextInput::new(&np.name).placeholder("My project").label("Name").width(None).on_edit(|s: &mut App| &mut s.new_project.name).build(k);
    let dir = TextInput::new(&np.dir)
        .placeholder("~/code/my-repo")
        .label(if np.boxed { "Repository" } else { "Folder" })
        .width(None)
        .on_edit(|s: &mut App| &mut s.new_project.dir)
        .build(k);
    let file = TextInput::new(&np.file).placeholder("/path/to/project.toml (optional)").label("Project file").width(None).on_edit(|s: &mut App| &mut s.new_project.file).build(k);
    // the switch carries its label; what it means wraps below it (a long description beside it overflows the dialog)
    let boxed = col()
        .w_full()
        .gap(4.0)
        .child(Switch::new(np.boxed).label("Boxed").on_toggle(|s: &mut App| s.new_project.boxed = !s.new_project.boxed).build(k))
        .child(
            text(if np.boxed {
                "A Docker Sandbox on a private clone of the repository; coordd runs in it as root."
            } else {
                "coordd and the agents run on the host as you (a systemd user unit, coordd-ID)."
            })
            .font(k.ty.small)
            .color(k.c.text_muted)
            .wrap(),
        );
    let running = np.running.is_some();
    let note = match (&np.error, running) {
        (_, true) => (format!("starting {id}…"), k.c.text_muted),
        (Some(e), _) => (e.clone(), k.c.danger.fg),
        (None, _) if id.is_empty() => (String::new(), k.c.text_faint),
        (None, _) => (format!("coord folder ~/agent-coord/{id}"), k.c.text_faint),
    };
    let create = Button::new(if running { "Starting…" } else { "Create" })
        .variant(ButtonVariant::Contrast)
        .disabled(running || id.is_empty() || np.dir.text.trim().is_empty())
        .on_press(|s: &mut App| s.create_project())
        .build(k);
    let cancel = Button::new("Cancel").variant(ButtonVariant::Ghost).disabled(running).on_press(|s: &mut App| s.new_project.open = false).build(k);
    let body = col()
        .w_full()
        .px(16.0)
        .pb(16.0)
        .gap(12.0)
        .child(field("Name", name))
        .child(field(if np.boxed { "Repository" } else { "Folder" }, dir))
        .child(field("Project file", file))
        .child(boxed)
        .child(text(note.0).font(k.ty.small).color(note.1).wrap())
        .child(row().w_full().justify_end().gap(8.0).child(cancel).child(create));
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
