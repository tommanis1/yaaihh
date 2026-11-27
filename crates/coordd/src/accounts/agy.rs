//! An agy account: Google's Antigravity CLI, the third harness, run as its own ACP client by agent-base's agy-rpc on
//! the pinned agy 1.5.18 ([`AGY_VERSION`]), naming no client, with no telemetry, in two kinds:
//!
//! - **A sign-in account** runs on his Google sign-in, made by agy's own `agy acp --login` into the account's profile
//!   ([`profile`], `home/profile`) and nowhere else: coordd hands the window Google's address and agy his pasted code
//!   ([`SignIn`]), and nothing else of the sign-in passes through coordd, which never reads the profile. Its check is
//!   agy's list of models on that sign-in ([`models`]), which sends no prompt. Each of its agents runs on that profile
//!   with a HOME of its own (agy keeps the sign-in in its data folder there), sharing the profile's lock, which a
//!   sign-in or sign-out takes whole.
//! - **A key account** runs on his Gemini API key, which the window's masked field writes to the account's key file
//!   (mode 600; `Store::key_file`). Each of its agents runs on a profile of its own, given the key alone. Its check is
//!   agy's list of models on that key ([`models`]).
//!
//! **An account t3code signed in** ([`legacy`]: t3code's state in its `home/.t3`, no sign-in of agy's own finished
//! on it, [`own_signin`]) is a
//! sign-in account from before agy's own sign-in was used here: its agents run as threads of the account's t3code
//! 0.1.11 server ([`T3_VERSION`], unmodified, on 127.0.0.1 alone), started by coordd as `agy-rpc server HOME`
//! ([`start_server`]) in a cgroup of its own ([`server_group`]), whose cgroup names no agent, so their coord commands
//! declare their names ([`declares`]); its check is what t3code says ([`check`]). Signing it in again (once none of its
//! agents runs: their commands run in that server) ends its server and moves it to agy's own sign-in.
//!
//! What runs here is agent-base's `agy-rpc`, the one program that speaks to t3code and agy; coordd reads its words.
//! Each check and sign-in runs in a cgroup of the account's own ([`probe_group`]), never coordd's. The server's log
//! (`t3.log`) holds t3code's pairing details: coordd writes it and never reads it.
//!
//! No time limit is ours: a run lasts as long as its process lives, and the owner's Cancel ends a sign-in.

use super::limits::{self, Said, Verdict};
use super::signin::{how, plain, Ended};
use super::{Kind, State, Store, Window};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::watch;

/// The t3code agy-rpc runs, as agy-pin fetches it (`COORD/opt/t3/0.1.11/t3`) and agy-rpc checks it before every run.
pub const T3_VERSION: &str = "0.1.11";

/// The sha256 of that release's `t3` (its linux x64 archive matched the release's SHA256SUMS), as agy-rpc's
/// `T3_SHA256` and agy-pin's pin it (tests/agy-pin-test.sh reads all three).
pub const T3_SHA256: &str = "f13dd1f37eb2f719b41f45219b3800412799c80f724b022839b8a133e21aced1";

/// The agy every account runs, as agy-pin fetches it (`COORD/opt/agy/1.5.18/bin/antigravity`).
pub const AGY_VERSION: &str = "1.5.18";

/// The sha256 of that `antigravity` (Google's linux x64 archive matched the sha512 its updater's manifest gives).
pub const AGY_SHA256: &str = "7c96ad8c944c1a8d195ca34d2041a3d74d5166c96662b52183b748a3c455d52f";

/// What an agy model's name begins with (coord-types' `harness::AGY_MODEL_PREFIX`).
pub const MODEL_PREFIX: &str = coord_types::harness::AGY_MODEL_PREFIX;

/// [`Said::from`] for t3code's reading of an account's limits ([`check`]): a reading of every window it has, as Go's
/// usage page's.
pub const T3CODE: &str = "t3code";

/// agent-base's agy-rpc, under BASE.
pub fn rpc(base: &Path) -> PathBuf {
    base.join("bin/agy-rpc")
}

/// The account's `home/`: the HOME of everything run on it, its agy profile ([`profile`]) or, on an account t3code
/// signed in, t3code's state (`home/.t3`). Never read by coordd.
pub fn home(dir: &Path) -> PathBuf {
    dir.join("home")
}

/// The account's agy profile (agy's `--gemini-dir`): a sign-in account's sign-in, its settings and conversations, as
/// agy keeps them; a key account's checks'. Made by agy-rpc, never read by coordd.
pub fn profile(dir: &Path) -> PathBuf {
    home(dir).join("profile")
}

/// The mark agy-rpc's `signin` leaves beside the account's [`profile`] while a sign-in it began there has not finished:
/// made before the profile, taken away with it unless agy took the code, and left by a sign-in killed outright.
fn unfinished(dir: &Path) -> PathBuf {
    home(dir).join("profile.unfinished")
}

/// Whether agy's own sign-in finished on the account whose folder is DIR: its profile there, and no sign-in
/// [`unfinished`] on it. One cancelled, failed or killed is none (the owner, N-1956 p9).
pub fn own_signin(dir: &Path) -> bool {
    profile(dir).is_dir() && !unfinished(dir).exists()
}

/// Account ID's way in to agy's own client: `api_key` when it is a key account whose key file is set,
/// `oauth-personal` when agy's own sign-in finished on it ([`own_signin`]); None, none.
pub fn way_in(store: &Store, id: &str) -> Result<Option<&'static str>, String> {
    match store.kind(super::Harness::Agy, id) {
        Some(Kind::Key) => Ok(store.key_set(super::Harness::Agy, id)?.then_some("api_key")),
        _ => Ok(own_signin(&store.dir(super::Harness::Agy, id)).then_some("oauth-personal")),
    }
}

/// Whether the account whose folder is DIR is one t3code signed in and agy's own sign-in has not finished on since:
/// t3code's state in its home, and no [`own_signin`].
pub fn legacy(dir: &Path) -> bool {
    home(dir).join(".t3").is_dir() && !own_signin(dir)
}

/// The environment an agent on the account whose folder is DIR is given, beside its own (`Store::session_env`): a
/// sign-in account's profile as `AGY_SIGNIN`, or on an account t3code signed in ([`legacy`]) `T3_HOME`, the home whose
/// t3code server holds its thread. A key account's key file is roster's `@key` line instead (mover.rs), so it gives
/// nothing here.
pub fn session_env(dir: &Path, kind: Kind) -> Vec<(String, String)> {
    match kind {
        Kind::Signin if legacy(dir) => vec![("T3_HOME".into(), home(dir).display().to_string())],
        Kind::Signin => vec![("AGY_SIGNIN".into(), profile(dir).display().to_string())],
        Kind::Key => vec![],
    }
}

/// The cgroup (under coordd's root) the t3code server of an account t3code signed in runs in: an account's sign-in's.
/// `cgroup::identify_path` names it no owner and no agent, so whatever agy runs there is answered nothing, except
/// the coord commands of the account's own agents ([`declares`]).
pub fn server_group(id: &str) -> String {
    format!("signin/t3-{id}")
}

/// The cgroup (under coordd's root) the account's checks, sign-ins and sign-outs run in, and the agy each starts:
/// an account's sign-in's, which `cgroup::identify_path` names no owner and no agent, so anything agy runs there is
/// answered nothing; never coordd's own.
pub fn probe_group(id: &str) -> String {
    format!("signin/agy-{id}")
}

/// Whether a caller in cgroup PATH is agy agent NAME of coord COORD, as its request line declares
/// (`coord_types::DECLARED`): the one exception to a caller's cgroup naming it. It holds only when NAME is bound to a
/// sign-in account of agy that t3code signed in ([`legacy`]) and PATH is that account's t3code server's cgroup
/// ([`server_pid`]), read from the server's own process. The threads of one account share that cgroup, so one may
/// declare another's name, of any coord on the account: signing the account in again ends that.
pub fn declares(store: &Store, coord: &str, name: &str, path: &str) -> bool {
    store
        .bound_to(super::Harness::Agy, coord, name)
        .filter(|e| e.account.kind == Kind::Signin && legacy(&store.dir(super::Harness::Agy, &e.id)))
        .and_then(|e| server_pid(&store.dir(super::Harness::Agy, &e.id)))
        .is_some_and(|pid| !path.is_empty() && crate::cgroup::path_of(pid) == path)
}

/// CMD made to put itself into cgroup GROUP before it runs (a write of `0` to the cgroup's `cgroup.procs`, from the
/// child, before its exec), so nothing it starts is ever in coordd's own cgroup. The file is opened here; an Err says
/// why it cannot be, and then nothing is run.
fn grouped(cmd: &mut Command, group: &str) -> Result<std::fs::File, String> {
    use std::os::fd::AsRawFd;
    let dir = crate::cgroup::root().join(group);
    let procs = dir.join("cgroup.procs");
    std::fs::create_dir_all(&dir).map_err(|e| format!("its cgroup {}: {e}", dir.display()))?;
    let f = std::fs::OpenOptions::new().write(true).create(true).truncate(false).open(&procs).map_err(|e| format!("its cgroup {}: {e}", procs.display()))?;
    let fd = f.as_raw_fd();
    // SAFETY: the closure runs in the forked child before its exec and calls write(2) alone, which is
    // async-signal-safe; FD stays open in the parent until the spawn has returned (the File is given back)
    unsafe {
        cmd.pre_exec(move || if libc::write(fd, b"0".as_ptr().cast(), 1) == 1 { Ok(()) } else { Err(std::io::Error::last_os_error()) });
    }
    Ok(f)
}

/// The environment of everything coordd runs on the account: nothing of coordd's but `LANG`, and `COORD`, where
/// agy-rpc finds the pinned t3code and agy.
fn run_env(cmd: &mut Command, coord: &Path) {
    cmd.env_clear().env("PATH", "/usr/local/bin:/usr/bin:/bin").env("LANG", std::env::var_os("LANG").unwrap_or_else(|| "C.UTF-8".into())).env("COORD", coord);
}

/// The pid of the t3code server of the account whose folder is DIR, while one runs: the pid its own
/// `server-runtime.json` names, when that process lives and is a `t3`. None otherwise.
pub fn server_pid(dir: &Path) -> Option<i32> {
    let rt: serde_json::Value = super::read_json(&home(dir).join(".t3/userdata/server-runtime.json"))?;
    let pid = i32::try_from(rt.get("pid")?.as_i64()?).ok()?;
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    (exe.file_name().is_some_and(|n| n == "t3")).then_some(pid)
}

/// Start the t3code server of the account whose folder is DIR: `agy-rpc server HOME` (agent-base under BASE) in
/// cgroup GROUP, its own process group, its output appended to DIR's `t3.log`, and nothing waiting on it but a task
/// that reaps it. Its pid, or why it was not started. One that runs already is left as it is (its pid).
pub fn start_server(base: &Path, coord: &Path, dir: &Path, group: &str) -> Result<i32, String> {
    if let Some(pid) = server_pid(dir) {
        return Ok(pid);
    }
    let log = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("t3.log")).map_err(|e| format!("{}: {e}", dir.join("t3.log").display()))?;
    let mut cmd = Command::new(rpc(base));
    cmd.arg("server").arg(home(dir)).current_dir(dir).stdin(Stdio::null());
    cmd.stdout(log.try_clone().map_err(|e| e.to_string())?).stderr(log);
    run_env(&mut cmd, coord);
    cmd.process_group(0).kill_on_drop(false);
    let procs = grouped(&mut cmd, group)?;
    let mut child = cmd.spawn().map_err(|e| format!("agy-rpc server: {e}"))?;
    drop(procs);
    let pid = child.id().map(|p| p as i32).unwrap_or_default();
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    Ok(pid)
}

/// Wait while the account's server (PID, just started) comes up: until its `server-runtime.json` names a live t3, or
/// the process has ended. No time limit (the owner: none of ours); an Err when it ended first.
pub async fn server_up(dir: &Path, pid: i32) -> Result<(), String> {
    loop {
        if server_pid(dir).is_some() {
            return Ok(());
        }
        // SAFETY: kill(2) with signal 0 sends nothing; it asks whether PID is there
        if unsafe { libc::kill(pid, 0) } != 0 {
            return Err(format!("its t3code server ended before it was up (its log: {})", dir.join("t3.log").display()));
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

/// What t3code says of a sign-in account ([`check`]): its agy runtime, its sign-in, its models and limits.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
pub struct Checked {
    #[serde(default)]
    pub installed: bool,
    #[serde(default)]
    pub version: Option<String>,
    /// t3code's word on the provider (`ready`, `warning`, `error`, `disabled`) and its message.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub auth: Auth,
    /// The models t3code offers, as agy names them (no [`MODEL_PREFIX`]).
    #[serde(default)]
    pub models: Vec<String>,
    /// Its limits as t3code last read them (`usageLimits`), as t3code gives them.
    #[serde(default)]
    pub limits: Option<serde_json::Value>,
    /// t3code's words when it refused the check's refresh, which leaves [`Checked::limits`] as before.
    #[serde(default)]
    pub refresh_failed: Option<String>,
}

/// t3code's word on the sign-in: `authenticated` or another, its kind (`oauth-personal`) and its label.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
pub struct Auth {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
}

impl Checked {
    pub fn signed_in(&self) -> bool {
        self.auth.status.as_deref() == Some("authenticated")
    }
}

/// One `agy-rpc VERB HOME` on the account whose folder is DIR, in cgroup GROUP: its output when it ended 0, else its
/// words.
async fn verb(base: &Path, coord: &Path, dir: &Path, group: &str, verb: &str) -> Result<String, String> {
    let mut cmd = Command::new(rpc(base));
    cmd.arg(verb).arg(home(dir)).current_dir(dir).stdin(Stdio::null());
    run_env(&mut cmd, coord);
    let procs = grouped(&mut cmd, group)?;
    let out = cmd.output().await;
    drop(procs);
    let out = out.map_err(|e| format!("agy-rpc {verb}: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() {
        let said = format!("{} {}", plain(&text).trim(), plain(&String::from_utf8_lossy(&out.stderr)).trim());
        return Err(format!("agy-rpc {verb} {}: {}", how(out.status), said.trim()));
    }
    Ok(text)
}

/// Ask t3code what it says of the account t3code signed in whose folder is DIR: `agy-rpc check HOME`, on its running
/// server, in cgroup GROUP ([`probe_group`]).
pub async fn check(base: &Path, coord: &Path, dir: &Path, group: &str) -> Result<Checked, String> {
    let text = verb(base, coord, dir, group, "check").await?;
    serde_json::from_str(text.trim()).map_err(|e| format!("agy-rpc check: not its answer ({e}): {}", text.trim().chars().take(200).collect::<String>()))
}

/// The sign-out of the account whose folder is DIR, in cgroup GROUP ([`probe_group`]): agy's own of its profile, and
/// t3code's own on its running server when t3code signed it in once.
pub async fn logout(base: &Path, coord: &Path, dir: &Path, group: &str) -> Result<(), String> {
    verb(base, coord, dir, group, "logout").await.map(drop)
}

/// The limits t3code read (`usageLimits.windows`) as a reading of every window under t3code's label; each is a window
/// of some models ([`coord_types::ModelWeek`]). A window not used yet (`resetsAt` null) is kept as
/// [`coord_types::NOT_STARTED`]. A used-up window is no verdict on the account: its agents' turns say which model is out
/// (`limits::apply`, [`super::State::models_out`]). A window whose share used is not a number from 0 up (none, NaN,
/// infinite, below 0), or whose reset is no time, is left out; None when no window is left.
pub fn from_limits(limits: &serde_json::Value) -> Option<Said> {
    let windows = limits.get("windows")?.as_array().filter(|w| !w.is_empty())?;
    let mut said = Said::of(Verdict::In, T3CODE);
    let mut weeks = Vec::new();
    for w in windows {
        let Some(used_percent) = w.get("usedPercent").and_then(|p| p.as_f64()).filter(|p| p.is_finite() && *p >= 0.0) else { continue };
        let resets = match w.get("resetsAt") {
            None | Some(serde_json::Value::Null) => None,
            Some(t) => match t.as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()) {
                Some(t) => Some(t.timestamp()),
                None => continue,
            },
        };
        let label = w.get("label").and_then(|l| l.as_str()).unwrap_or("models").to_string();
        weeks.push(coord_types::ModelWeek { model: label, window: Window { used_percent, resets_at: resets.unwrap_or(coord_types::NOT_STARTED) } });
    }
    if weeks.is_empty() {
        return None;
    }
    said.model_weeks = Some(weeks);
    Some(said)
}

/// When t3code read LIMITS (its `checkedAt`), at most NOW (a time after NOW is its clock's, read this moment); None
/// when it does not say when: a reading of no known time is no reading of now.
fn read_at(limits: &serde_json::Value, now: i64) -> Option<i64> {
    limits["checkedAt"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| t.timestamp().min(now))
}

/// Put what t3code said ([`check`]) into ID's `state.json` at NOW, an account t3code signed in: signed in when t3code
/// says authenticated, its kind as `auth_method`, its models (with [`MODEL_PREFIX`]) while it is signed in, and its
/// limits as t3code read them, seen when t3code read them ([`read_at`]): a refresh it refused, or whose quota read
/// failed, left a reading of before, which this check did not see anew; a reading of no known time is left out.
pub fn record(store: &Store, id: &str, c: &Checked, now: i64) -> Result<State, String> {
    store.update(super::Harness::Agy, id, |st| {
        st.signed_in = c.signed_in();
        st.signed_in_seen = now;
        st.auth_method = c.auth.kind.clone();
        if st.signed_in {
            st.put_back = None;
            st.models = c.models.iter().map(|m| format!("{MODEL_PREFIX}{m}")).collect();
            if let Some((said, at)) = c.limits.as_ref().and_then(|l| Some((from_limits(l)?, read_at(l, now)?))) {
                limits::apply(st, &said, now);
                st.model_weeks_seen = Some(at);
                if !st.model_weeks.is_empty() {
                    st.limits_seen = at;
                }
            }
        } else {
            st.models.clear();
        }
    })
}

/// The models agy itself offers on account DIR, each with [`MODEL_PREFIX`]: `agy-rpc models` with HOME its `home/`
/// and the pinned agy, in cgroup GROUP ([`probe_group`]), on its key file (`Some`), or on its Google sign-in (`None`:
/// AGY_SIGNIN its [`profile`]), and on nothing else. agy opens a session to say them and is sent no prompt.
pub async fn models(base: &Path, coord: &Path, dir: &Path, group: &str, key_file: Option<&Path>) -> Result<Vec<String>, String> {
    let mut cmd = Command::new(rpc(base));
    cmd.arg("models").current_dir(dir).stdin(Stdio::null());
    run_env(&mut cmd, coord);
    cmd.env("HOME", home(dir)).env("AGY_BIN", coord.join(format!("opt/agy/{AGY_VERSION}/bin/antigravity")));
    match key_file {
        Some(k) => cmd.env("AGY_KEY_FILE", k),
        None => cmd.env("AGY_SIGNIN", profile(dir)),
    };
    let procs = grouped(&mut cmd, group)?;
    let out = cmd.output().await;
    drop(procs);
    let out = out.map_err(|e| format!("agy-rpc models: {e}"))?;
    if !out.status.success() {
        return Err(format!("agy-rpc models {}: {}", how(out.status), plain(&String::from_utf8_lossy(&out.stderr)).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).lines().map(str::trim).filter(|l| !l.is_empty() && !l.contains(char::is_whitespace)).map(|m| format!("{MODEL_PREFIX}{m}")).collect())
}

/// Put the check of an account on agy's own client into ID's `state.json` at NOW: signed in while it has a way in
/// (METHOD, [`way_in`]) and agy took it (MODELS Ok:
/// its models, kept); a way agy refused (`authentication_failed`) is not signed in. Its models as agy offers them on it.
pub fn record_own(store: &Store, id: &str, method: Option<&str>, models: &Result<Vec<String>, String>, now: i64) -> Result<State, String> {
    store.update(super::Harness::Agy, id, |st| {
        st.signed_in = method.is_some() && !matches!(models, Err(why) if why.contains("authentication_failed"));
        st.signed_in_seen = now;
        st.auth_method = method.map(str::to_string);
        match models {
            Ok(m) if st.signed_in => st.models = m.clone(),
            _ if !st.signed_in => st.models.clear(),
            _ => {}
        }
    })
}

/// One running `agy-rpc signin HOME`: agy's own Google sign-in of the account into its [`profile`]. It prints `url URL`,
/// Google's address as agy gives it, then reads the code he pasted as one line, then prints `succeeded` or `failed WORDS`.
/// Every method takes `&self`. Dropping it before it ended ends it.
pub struct SignIn {
    pid: i32,
    stdin: tokio::sync::Mutex<Option<ChildStdin>>,
    url: watch::Receiver<Option<Result<String, String>>>,
    end: watch::Receiver<Option<Ended>>,
    /// What it printed on stderr (agy-rpc's own words, never the code), for a failure's words.
    errors: std::sync::Arc<Mutex<String>>,
}

impl SignIn {
    /// Start it for the account whose folder is DIR, in cgroup GROUP.
    pub fn start(base: &Path, coord: &Path, dir: &Path, group: &str) -> Result<SignIn, String> {
        let mut cmd = Command::new(rpc(base));
        cmd.arg("signin").arg(home(dir)).current_dir(dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        run_env(&mut cmd, coord);
        cmd.kill_on_drop(true);
        let procs = grouped(&mut cmd, group)?;
        let mut child: Child = cmd.spawn().map_err(|e| format!("agy-rpc signin: {e}"))?;
        drop(procs);
        let pid = child.id().map(|p| p as i32).unwrap_or_default();
        let (url_tx, url) = watch::channel(None);
        let (end_tx, end) = watch::channel(None);
        let errors = std::sync::Arc::new(Mutex::new(String::new()));
        let (stdout, mut stderr, stdin) = (child.stdout.take(), child.stderr.take(), child.stdin.take());
        let e2 = errors.clone();
        tokio::spawn(async move {
            let mut buf = String::new();
            if let Some(s) = stderr.as_mut() {
                let _ = s.read_to_string(&mut buf).await;
            }
            *e2.lock().unwrap() = buf;
        });
        let e3 = errors.clone();
        tokio::spawn(async move {
            let mut said = None;
            if let Some(out) = stdout {
                let mut lines = BufReader::new(out).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    if let Some(u) = l.strip_prefix("url ") {
                        let _ = url_tx.send(Some(Ok(u.trim().to_string())));
                    } else if l.trim() == "succeeded" {
                        said = Some(Ended::Ok);
                    } else if let Some(w) = l.strip_prefix("failed") {
                        said = Some(Ended::Failed(w.trim().to_string()));
                    }
                }
            }
            let status = child.wait().await;
            let ended = said.unwrap_or_else(|| {
                let how = status.map(how).unwrap_or_else(|e| e.to_string());
                let words = e3.lock().unwrap().trim().chars().rev().take(300).collect::<Vec<_>>().into_iter().rev().collect::<String>();
                Ended::Failed(if words.is_empty() { format!("the sign-in {how}") } else { format!("{words} (the sign-in {how})") })
            });
            if url_tx.borrow().is_none() {
                let _ = url_tx.send(Some(Err(match &ended {
                    Ended::Failed(w) => w.clone(),
                    Ended::Ok => "agy gave no address".into(),
                })));
            }
            let _ = end_tx.send(Some(ended));
        });
        Ok(SignIn { pid, stdin: tokio::sync::Mutex::new(stdin), url, end, errors })
    }

    pub fn pid(&self) -> i32 {
        self.pid
    }

    /// Google's address, as agy gave it; waited for as long as the run lives.
    pub async fn url(&self) -> Result<String, String> {
        let mut rx = self.url.clone();
        let got = rx.wait_for(Option::is_some).await.map_err(|_| "the sign-in ended".to_string())?;
        got.clone().unwrap_or_else(|| Err("the sign-in ended".into()))
    }

    /// CODE, the code Google showed him, to agy's sign-in: once, as one line.
    pub async fn paste(&self, code: &str) -> Result<(), String> {
        let code = code.trim();
        if code.is_empty() || code.contains(['\n', '\r']) {
            return Err("the code is empty or more than one line".into());
        }
        let mut stdin = self.stdin.lock().await;
        let Some(w) = stdin.as_mut() else { return Err("a code was given to this sign-in already".into()) };
        w.write_all(format!("{code}\n").as_bytes()).await.map_err(|e| format!("the sign-in took no code: {e}"))?;
        w.flush().await.map_err(|e| format!("the sign-in took no code: {e}"))?;
        *stdin = None;
        Ok(())
    }

    /// How it ended, waited for as long as it lives.
    pub async fn wait(&self) -> Ended {
        let mut rx = self.end.clone();
        match rx.wait_for(Option::is_some).await {
            Ok(e) => e.clone().unwrap_or(Ended::Failed("ended".into())),
            Err(_) => Ended::Failed("ended".into()),
        }
    }

    pub fn ended(&self) -> Option<Ended> {
        self.end.borrow().clone()
    }

    /// End it: agy-rpc is told to end (SIGTERM), and ends agy's sign-in itself.
    pub async fn cancel(&self) -> Ended {
        if self.ended().is_none() {
            // SAFETY: kill(2) on the child's own pid, which is reaped only by this run's task
            unsafe { libc::kill(self.pid, libc::SIGTERM) };
        }
        self.wait().await
    }

    /// Its own words so far on stderr, for the owner.
    pub fn printed(&self) -> String {
        self.errors.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// t3code's limits read as its windows under t3code's labels, on made-up times: four windows none used, all kept
    /// as not started; a used-up window leaves the account in; a window without its share or with no reset time is left
    /// out; no windows, nothing said.
    #[test]
    fn t3codes_limits_are_read_as_its_windows_a_window_not_used_yet_with_no_reset() {
        let window = |id: &str, kind: &str, label: &str, used: f64, resets: serde_json::Value, mins: i64| json!({"id": id, "kind": kind, "label": label, "usedPercent": used, "resetsAt": resets, "windowDurationMins": mins});
        let his = json!({"checkedAt": "2026-11-24T09:47:55.956Z", "windows": [
            window("3p-5h", "session", "Claude & GPT (shared) · 5-hour", 0.0, json!(null), 300),
            window("gemini-5h", "session", "Gemini · 5-hour", 0.0, json!(null), 300),
            window("3p-weekly", "weekly", "Claude & GPT (shared) · Weekly", 0.0, json!(null), 10080),
            window("gemini-weekly", "weekly", "Gemini · Weekly", 0.0, json!(null), 10080),
        ]});
        let said = from_limits(&his).unwrap();
        let rows = |said: &Said| said.model_weeks.as_ref().unwrap().iter().map(|w| (w.model.clone(), w.window.used_percent, w.window.resets_at)).collect::<Vec<_>>();
        let not_started = coord_types::NOT_STARTED;
        assert_eq!(rows(&said), vec![("Claude & GPT (shared) · 5-hour".to_string(), 0.0, not_started), ("Gemini · 5-hour".to_string(), 0.0, not_started), ("Claude & GPT (shared) · Weekly".to_string(), 0.0, not_started), ("Gemini · Weekly".to_string(), 0.0, not_started)]);
        assert_eq!((said.five_hour.as_ref(), said.seven_day.as_ref(), said.month.as_ref(), &said.verdict), (None, None, None, &Verdict::In), "no window of all models: one 5-hour per family");
        // recorded: the four windows, and when and from what they were read
        let mut st = State::default();
        limits::apply(&mut st, &said, 1_795_513_685);
        assert_eq!((st.model_weeks.len(), st.limits_seen, st.limits_from.as_str(), st.out_until), (4, 1_795_513_685, T3CODE, None));
        // used up, Gemini's two: the account stays in, as it was, its windows read
        let at = |t: &str| chrono::DateTime::parse_from_rfc3339(t).unwrap().timestamp();
        let used = from_limits(&json!({"windows": [
            window("g5", "session", "Gemini · 5-hour", 100.0, json!("2026-11-24T15:00:00Z"), 300),
            window("gw", "weekly", "Gemini · Weekly", 100.0, json!("2026-11-30T00:00:00Z"), 10080),
            window("c5", "session", "Claude & GPT (shared) · 5-hour", 30.0, json!("2026-11-24T14:00:00Z"), 300),
        ]}))
        .unwrap();
        assert_eq!((&used.verdict, used.out_for), (&Verdict::In, None), "no verdict of a used-up window");
        assert_eq!(rows(&used)[2], ("Claude & GPT (shared) · 5-hour".to_string(), 30.0, at("2026-11-24T14:00:00Z")));
        let one = at("2026-11-24T13:00:00Z");
        st.signed_in = true;
        limits::apply(&mut st, &used, one);
        assert!(st.can_take(one) && st.out_until.is_none() && st.models_out.is_empty(), "in: {st:?}");
        assert_eq!(st.model_weeks.iter().filter(|w| w.window.used_percent >= 100.0).count(), 2, "shown used up");
        // left out: no share used said, one below 0, a reset that is no time
        let odd = from_limits(&json!({"windows": [{"id": "a", "label": "A", "resetsAt": null}, window("b", "weekly", "B", 5.0, json!("soon"), 10080),
                                                  window("n", "weekly", "N", -1.0, json!(null), 10080), window("c", "weekly", "C", 5.0, json!(null), 10080)]})).unwrap();
        assert_eq!(rows(&odd), vec![("C".to_string(), 5.0, not_started)]);
        assert!(from_limits(&json!({"checkedAt": "x", "windows": [], "unavailable": "no reading"})).is_none(), "no windows read: nothing said");
        assert!(from_limits(&json!({"windows": [{"id": "a", "label": "A", "resetsAt": null}, window("n", "weekly", "N", -1.0, json!(null), 10080)]})).is_none(), "every window left out: nothing said");
    }

    /// A signed-in check of an account t3code signed in lists t3code's models alone; signed out, none.
    #[test]
    fn a_t3code_accounts_check_lists_t3codes_models() {
        let t = crate::accounts::testing::Tmp::new("agy-t3-models");
        t.store.create_agy("g", "G", 0, Kind::Signin, "x").unwrap();
        let mut c = Checked { auth: Auth { status: Some("authenticated".into()), ..Auth::default() }, models: vec!["gemini-3.8-flash".into()], ..Checked::default() };
        assert_eq!(record(&t.store, "g", &c, 1).unwrap().models, ["agy/gemini-3.8-flash"]);
        c.auth.status = Some("unauthenticated".into());
        assert!(record(&t.store, "g", &c, 2).unwrap().models.is_empty());
    }

    /// The check of an account on agy's own client: signed in with a way in that agy took, its models; a way agy refused
    /// is not signed in and has none; another failure (agy not reached) leaves its models; no way in, not signed in.
    #[test]
    fn an_own_clients_check_is_signed_in_on_the_way_agy_took() {
        let t = crate::accounts::testing::Tmp::new("agy-own-check");
        t.store.create_agy("g", "G", 0, Kind::Signin, "x").unwrap();
        let st = record_own(&t.store, "g", Some("oauth-personal"), &Ok(vec!["agy/gemini-4-neon".into()]), 1).unwrap();
        assert_eq!((st.signed_in, st.auth_method.as_deref(), st.models.clone()), (true, Some("oauth-personal"), vec!["agy/gemini-4-neon".to_string()]));
        let st = record_own(&t.store, "g", Some("oauth-personal"), &Err("agy-rpc models exit 1: agy-rpc: unknown: agy ended".into()), 2).unwrap();
        assert_eq!((st.signed_in, st.models.len()), (true, 1), "not reached: as it was");
        let st = record_own(&t.store, "g", Some("oauth-personal"), &Err("agy-rpc models exit 1: agy-rpc: authentication_failed: Authentication required".into()), 3).unwrap();
        assert_eq!((st.signed_in, st.models.len()), (false, 0), "refused");
        let st = record_own(&t.store, "g", None, &Ok(vec![]), 4).unwrap();
        assert_eq!((st.signed_in, st.auth_method), (false, None), "no way in");
    }

    /// A check's reading is seen at t3code's `checkedAt` (a refused refresh keeps the reading before); one after now is
    /// now, one of no known time is left out. A not-started window stays when a later signal names no weeks.
    #[test]
    fn a_checks_reading_is_seen_when_t3code_read_it() {
        let t = crate::accounts::testing::Tmp::new("agy-read-at");
        t.store.create_agy("g", "G", 0, Kind::Signin, "x").unwrap();
        let now = 1_795_513_685;
        let at = |t: &str| chrono::DateTime::parse_from_rfc3339(t).unwrap().timestamp();
        let checked = |checked_at: serde_json::Value| Checked {
            auth: Auth { status: Some("authenticated".into()), ..Auth::default() },
            limits: Some(json!({"checkedAt": checked_at, "windows": [{"id": "gw", "kind": "weekly", "label": "Gemini · Weekly", "usedPercent": 10.0, "resetsAt": null, "windowDurationMins": 10080}]})),
            refresh_failed: Some("Expected string, got 0".into()),
            ..Checked::default()
        };
        let before = "2026-11-24T09:47:55Z";
        assert!(at(before) < now);
        let st = record(&t.store, "g", &checked(json!(before)), now).unwrap();
        assert_eq!((st.limits_seen, st.model_weeks_seen, st.limits_from.as_str(), st.model_weeks.len()), (at(before), Some(at(before)), T3CODE, 1));
        // a reading of no known time is no reading of now: left out, the one before kept as seen then
        for odd in [json!(null), json!("not a time")] {
            let st = record(&t.store, "g", &checked(odd.clone()), now).unwrap();
            assert_eq!((st.limits_seen, st.model_weeks_seen), (at(before), Some(at(before))), "checkedAt {odd}: left out");
        }
        let st = record(&t.store, "g", &checked(json!("2027-01-01T00:00:00Z")), now).unwrap();
        assert_eq!((st.limits_seen, st.model_weeks_seen), (now, Some(now)), "a checkedAt after now: now");
        // a later signal that names no weeks keeps the not-started one
        let st = t.store.update(crate::accounts::Harness::Agy, "g", |st| limits::apply(st, &Said::of(Verdict::In, "rate_limit_event"), now + 10)).unwrap();
        assert_eq!(st.model_weeks.iter().map(|w| w.window.resets_at).collect::<Vec<_>>(), vec![coord_types::NOT_STARTED]);
    }

    /// A sign-in account's agent is given its profile; on an account t3code signed in, its t3code server's home until
    /// agy's own sign-in made its profile; a key account's, nothing here (its `@key` line). Its checks run in a cgroup of
    /// the account's, apart from its t3code server's.
    #[test]
    fn a_signin_account_gives_its_profile_or_t3codes_home_and_a_key_account_nothing() {
        let t = crate::accounts::testing::Tmp::new("agy-session-env");
        let dir = t.dir.join("g");
        let (h, p) = (home(&dir).display().to_string(), profile(&dir).display().to_string());
        assert_eq!(session_env(&dir, Kind::Signin), vec![("AGY_SIGNIN".to_string(), p.clone())], "a new account");
        std::fs::create_dir_all(home(&dir).join(".t3")).unwrap();
        assert!(legacy(&dir));
        assert_eq!(session_env(&dir, Kind::Signin), vec![("T3_HOME".to_string(), h)], "t3code signed it in");
        std::fs::create_dir_all(profile(&dir)).unwrap();
        assert!(!legacy(&dir));
        assert_eq!(session_env(&dir, Kind::Signin), vec![("AGY_SIGNIN".to_string(), p)], "signed in again, by agy's own sign-in");
        assert!(session_env(&dir, Kind::Key).is_empty());
        assert_eq!((server_group("g"), probe_group("g")), ("signin/t3-g".to_string(), "signin/agy-g".to_string()));
    }

    /// A sign-in of agy's own that did not finish, its profile left with the mark beside it as one killed outright
    /// leaves them (one cancelled or failed takes both away: agy-rpc-test): an account t3code signed in is still
    /// t3code's, and a new one has no way in, so a check that does not reach agy never signs it in (the owner,
    /// N-1956 p9). Once agy took the code the mark goes, and the sign-in is the account's.
    #[test]
    fn a_sign_in_not_finished_leaves_t3codes_account_t3codes_and_a_new_one_signed_out() {
        use crate::accounts::Harness;
        let t = crate::accounts::testing::Tmp::new("agy-unfinished");
        t.store.create_agy("old", "Old", 0, Kind::Signin, "x").unwrap();
        t.store.create_agy("new", "New", 0, Kind::Signin, "x").unwrap();
        let (old, new) = (t.store.dir(Harness::Agy, "old"), t.store.dir(Harness::Agy, "new"));
        std::fs::create_dir_all(home(&old).join(".t3")).unwrap();
        for dir in [&old, &new] {
            std::fs::create_dir_all(profile(dir)).unwrap();
            std::fs::write(unfinished(dir), "").unwrap();
        }
        assert!(legacy(&old) && !own_signin(&old));
        assert_eq!(session_env(&old, Kind::Signin), vec![("T3_HOME".to_string(), home(&old).display().to_string())]);
        assert_eq!(Harness::Agy.signs_out_of(&old), T3CODE);
        let way = way_in(&t.store, "new").unwrap();
        let st = record_own(&t.store, "new", way, &Err("agy-rpc models exit 1: agy-rpc: unknown: agy ended".into()), 1).unwrap();
        assert_eq!((way, st.signed_in), (None, false), "a new account is never signed in");
        for dir in [&old, &new] {
            std::fs::remove_file(unfinished(dir)).unwrap();
        }
        assert_eq!((legacy(&old), way_in(&t.store, "new").unwrap()), (false, Some("oauth-personal")));
    }
}
