//! The project file (design §3): a path on the machine, named in `$COORD/project`. coordd reads from it what it needs:
//! the item prefix, rights by level, the waits source and the view sources. The view kinds, columns and cards are
//! yaaihh's to draw; coordd passes each source's JSON on as it comes.

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Project {
    #[serde(default)]
    pub project: Header,
    #[serde(default)]
    pub permissions: HashMap<String, usize>,
    #[serde(default, rename = "view")]
    pub views: Vec<View>,
    /// Where the file is: sources run in its folder.
    #[serde(skip)]
    pub dir: String,
    /// The whole file as JSON, for yaaihh.
    #[serde(skip)]
    pub raw: Value,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Header {
    /// The prefix of the project's item ids (`I-` gives I-143).
    #[serde(default = "default_prefix")]
    pub item_prefix: String,
    /// A command printing {agent: [what it waits on]}, asked at each turn end.
    #[serde(default)]
    pub waits: Option<String>,
    /// The folder of the item notes: `coord item open` writes ID-SLUG.md there (design §4).
    #[serde(default)]
    pub notes: Option<String>,
}

fn default_prefix() -> String {
    "I-".into()
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

impl Project {
    /// The project named in COORD/project, or none.
    pub fn load(coord: &str) -> Result<Option<Project>, String> {
        let Ok(path) = std::fs::read_to_string(format!("{coord}/project")) else { return Ok(None) };
        let path = path.trim();
        let text = std::fs::read_to_string(path).map_err(|e| format!("project file {path}: {e}"))?;
        let mut p: Project = toml::from_str(&text).map_err(|e| format!("project file {path}: {e}"))?;
        p.raw = toml::from_str::<Value>(&text).unwrap_or(Value::Null);
        p.dir = std::path::Path::new(path).parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
        Ok(Some(p))
    }

    /// The level a right is granted to; the owner (0) has every right.
    pub fn level_of(&self, right: &str) -> usize {
        *self.permissions.get(right).unwrap_or(&0)
    }

    /// What the waits source says each agent waits on (an empty map when there is none or it fails).
    pub async fn waits(&self) -> HashMap<String, Vec<String>> {
        let Some(cmd) = &self.project.waits else { return HashMap::new() };
        let run = Command::new("sh").arg("-c").arg(cmd).current_dir(&self.dir).stdin(Stdio::null()).output();
        match tokio::time::timeout(Duration::from_secs(20), run).await {
            Ok(Ok(out)) if out.status.success() => serde_json::from_slice(&out.stdout).unwrap_or_default(),
            _ => HashMap::new(),
        }
    }

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
