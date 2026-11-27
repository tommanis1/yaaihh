//! The window run live on a scratch project: each step pressed after the ones before, from a fresh start, and what it then shows.

use crate::scratch::{Release, Scratch};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// From the window's start to its first look: its pipe up and the first answers in.
pub const BOOT: f64 = 4.0;
/// From a step to the look after it: the answers the step asked for in.
pub const SETTLE: f64 = 2.5;

/// One thing a person does to the window.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// Press what shows this text (`tap Settings`), or the element of this key (`tap @key`).
    Tap(Target),
    Type(String),
    /// A named key (`key Enter`).
    Key(String),
    Wait(f64),
    /// Nothing pressed: only a look.
    Look,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Text(String),
    Key(String),
}

impl std::str::FromStr for Step {
    type Err = String;

    /// `tap TEXT`, `tap @KEY`, `type TEXT`, `key NAME`, `wait SECONDS` or `look`.
    fn from_str(line: &str) -> Result<Step, String> {
        let line = line.trim();
        let (verb, rest) = line.split_once(' ').map_or((line, ""), |(v, r)| (v, r.trim()));
        let need = |what: &str| if rest.is_empty() { Err(format!("`{verb}` needs {what}: {line:?}")) } else { Ok(rest.to_string()) };
        match verb {
            "tap" => match need("what it presses")? {
                k if k == "@" => Err(format!("`tap @` needs a key: {line:?}")),
                k if k.starts_with('@') => Ok(Step::Tap(Target::Key(k[1..].into()))),
                t => Ok(Step::Tap(Target::Text(t))),
            },
            "type" => Ok(Step::Type(need("the text")?)),
            "key" => Ok(Step::Key(need("the key's name")?)),
            "wait" => need("seconds")?.parse::<f64>().ok().filter(|s| s.is_finite() && *s > 0.0 && *s <= 120.0).map(Step::Wait).ok_or(format!("`wait` takes 0 to 120 seconds: {line:?}")),
            "look" if rest.is_empty() => Ok(Step::Look),
            _ => Err(format!("not a step: {line:?} (tap TEXT, tap @KEY, type TEXT, key NAME, wait SECONDS, look)")),
        }
    }
}

/// An element of a look with text, and where it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub text: String,
    pub key: Option<String>,
    pub center: (f64, f64),
}

/// What the window showed after a step: its picture, its element tree and log as files, its texts read.
#[derive(Clone, Debug)]
pub struct Look {
    pub step: String,
    pub png: PathBuf,
    pub tree: PathBuf,
    pub log: PathBuf,
    pub shown: Vec<Shown>,
}

impl Look {
    /// Its texts, one a line, as a person reads the screen.
    pub fn seen(&self) -> String {
        self.shown.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join("\n")
    }
}

/// The texts on screen in TREE (rune's `--tree`): shown, not faded out, inside the viewport.
pub fn shown(tree: &Value) -> Vec<Shown> {
    let (vw, vh) = (tree["viewport"][0].as_f64().unwrap_or(f64::MAX), tree["viewport"][1].as_f64().unwrap_or(f64::MAX));
    let els = tree["elements"].as_array().map(Vec::as_slice).unwrap_or_default();
    els.iter()
        .filter_map(|e| {
            let text = e["text"].as_str()?.trim();
            let r: Vec<f64> = e["rect"].as_array()?.iter().filter_map(Value::as_f64).collect();
            let on = r.len() == 4 && r[2] > 0.0 && r[3] > 0.0 && r[0] < vw && r[1] < vh && r[0] + r[2] > 0.0 && r[1] + r[3] > 0.0;
            (on && !text.is_empty() && e["opacity"].as_f64().unwrap_or(1.0) > 0.01 && e["exiting"].as_bool() != Some(true))
                .then(|| Shown { text: text.into(), key: e["key"].as_str().map(String::from), center: (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0) })
        })
        .collect()
}

/// Where TARGET is in SHOWN: a key's element, else the first text equal to it, else the first that contains it.
pub fn find(shown: &[Shown], target: &Target) -> Option<(f64, f64)> {
    match target {
        Target::Key(k) => shown.iter().find(|s| s.key.as_deref() == Some(k)).map(|s| s.center),
        Target::Text(t) => shown.iter().find(|s| &s.text == t).or_else(|| shown.iter().find(|s| s.text.contains(t.as_str()))).map(|s| s.center),
    }
}

/// The built window and how it is drawn.
#[derive(Clone, Debug)]
pub struct Window {
    pub exe: PathBuf,
    pub size: (u32, u32),
    pub fps: u32,
}

/// One run of steps: the script so far, its clock, the looks taken; each look on a scratch project of its own, so a
/// step done again for a later look acts on nothing a first doing changed.
pub struct Session {
    release: Release,
    pub server_log: String,
    window: Window,
    out: PathBuf,
    script: Vec<Value>,
    t: f64,
    pub looks: Vec<Look>,
}

impl Session {
    pub fn new(release: Release, window: Window, out: &Path) -> Result<Session, String> {
        std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
        Ok(Session { release, server_log: String::new(), window, out: out.into(), script: Vec::new(), t: BOOT, looks: Vec::new() })
    }

    /// STEP pressed after the steps before, its place found on the last look; then a look at the window.
    pub fn step(&mut self, step: &Step, said: &str) -> Result<&Look, String> {
        let at = self.t;
        let mut events = match step {
            Step::Tap(target) => {
                if self.looks.is_empty() {
                    self.look("first look")?;
                }
                let last = self.looks.last().map(|l| l.shown.as_slice()).unwrap_or_default();
                let (x, y) = find(last, target).ok_or_else(|| format!("the window shows no {target:?} to press"))?;
                let at = self.t;
                self.t += 0.2;
                vec![json!({"t": at, "type": "tap", "x": x, "y": y})]
            }
            Step::Type(text) => {
                self.t += 0.035 * text.chars().count() as f64 + 0.1;
                vec![json!({"t": at, "type": "type", "text": text})]
            }
            Step::Key(name) => {
                self.t += 0.1;
                vec![json!({"t": at, "type": "key", "name": name})]
            }
            Step::Wait(s) => {
                self.t += s;
                Vec::new()
            }
            Step::Look => Vec::new(),
        };
        self.script.append(&mut events);
        self.look(said)
    }

    /// A fresh scratch project and window, the whole script played on the wall clock, its picture and tree taken SETTLE
    /// later; the project's coordd log kept beside them, and the project removed.
    fn look(&mut self, said: &str) -> Result<&Look, String> {
        let at = self.t + if self.looks.is_empty() && self.script.is_empty() { 0.0 } else { SETTLE };
        let n = self.looks.len() + 1;
        let scratch = Scratch::up_in_temp(&self.release)?;
        let looked = self.shoot(&scratch, n, at, said);
        self.server_log = scratch.log();
        std::fs::write(self.file(n, "coordd.log"), &self.server_log).map_err(|e| e.to_string())?;
        drop(scratch);
        looked?;
        Ok(self.looks.last().expect("pushed"))
    }

    /// The window run on SCRATCH to AT, look N taken.
    fn shoot(&mut self, scratch: &Scratch, n: usize, at: f64, said: &str) -> Result<(), String> {
        let (png, tree, log, script) = (self.file(n, "png"), self.file(n, "json"), self.file(n, "log"), self.file(n, "script.json"));
        std::fs::write(&script, serde_json::to_string_pretty(&self.script).unwrap_or_default()).map_err(|e| format!("{}: {e}", script.display()))?;
        let w = &self.window;
        let mut cmd = Command::new(&w.exe);
        cmd.args(["--live", "--realtime", "--scale", "1"])
            .args(["--size", &format!("{}x{}", w.size.0, w.size.1), "--fps", &w.fps.to_string(), "--at", &format!("{at:.3}")])
            .arg("--snap").arg(&png).arg("--tree").arg(&tree);
        if !self.script.is_empty() {
            cmd.arg("--script").arg(&script);
        }
        let logf = std::fs::File::create(&log).map_err(|e| format!("{}: {e}", log.display()))?;
        cmd.env_clear().envs(scratch.env()).env("YAAIHH_LOCAL_PIPE", "1").env("YAAIHH_CONFIG", scratch.config());
        cmd.stdin(Stdio::null()).stdout(logf.try_clone().map_err(|e| e.to_string())?).stderr(logf);
        let ran = wait_for(cmd, Duration::from_secs_f64(at + 90.0))?;
        let said_log = std::fs::read_to_string(&log).unwrap_or_default();
        if !ran {
            return Err(format!("the window failed at look {n}: {}", said_log.lines().rev().take(6).collect::<Vec<_>>().join(" / ")));
        }
        let t: Value = std::fs::read_to_string(&tree).ok().and_then(|s| serde_json::from_str(&s).ok()).ok_or(format!("look {n}: no tree at {}", tree.display()))?;
        if !png.is_file() {
            return Err(format!("look {n}: no picture at {}", png.display()));
        }
        self.t = at;
        self.looks.push(Look { step: said.into(), png, tree, log, shown: shown(&t) });
        Ok(())
    }

    fn file(&self, n: usize, ext: &str) -> PathBuf {
        self.out.join(format!("{n:02}.{ext}"))
    }
}

/// CMD run to its end, killed past LIMIT: whether it exited 0.
fn wait_for(mut cmd: Command, limit: Duration) -> Result<bool, String> {
    let mut child = cmd.spawn().map_err(|e| format!("the window: {e}"))?;
    let began = std::time::Instant::now();
    loop {
        if let Some(st) = child.try_wait().map_err(|e| e.to_string())? {
            return Ok(st.success());
        }
        if began.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("the window ran past {} s and was ended", limit.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
