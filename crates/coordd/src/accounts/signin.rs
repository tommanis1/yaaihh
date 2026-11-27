//! Adding an account: Claude Code's own sign-in, run for him, its URL given to the window.
//!
//! coordd runs `claude auth login` with `CLAUDE_CONFIG_DIR` the account's `config/` folder, its input and output on
//! pipes ([`SignIn::start`]). The command prints a URL ([`SignIn::url`]); the owner opens it and signs in. It completes
//! in one of two ways, both Claude Code's: the browser's redirect reaches the address the command listens on, or the
//! browser shows a code that the owner pastes into the app and coordd writes to the command's input
//! ([`SignIn::paste`]). Then the command ends ([`SignIn::wait`]). Whether the account is signed in is what
//! `claude auth status --json` says under the same folder ([`status`]), never a look at a file: the login is Claude
//! Code's, made, kept and renewed by it alone.
//!
//! The pasted code passes through coordd on its way to Claude Code's prompt, as it would through a terminal. It is held
//! in memory only to keep it out of what is shown ([`SignIn::printed`], the failure's words), and never logged or
//! stored.
//!
//! Nothing here depends on the command's wording: the URL is the first `https://` address it prints, a code is taken
//! whether or not a prompt was printed, and done is its exit code.

use super::{Harness, State, Store};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{ChildStdin, Command};
use tokio::sync::watch;

/// How a sign-in ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Ended {
    /// The command exited 0: Claude Code says the sign-in is done. [`status`] then says what it made.
    Ok,
    /// It did not: the command's own last words (what it printed, the pasted code taken out) and how it ended.
    Failed(String),
}

/// What the command printed, both streams in the order read, and the codes pasted into it (only to take them out).
#[derive(Default)]
struct Printed {
    text: String,
    pasted: Vec<String>,
}

impl Printed {
    /// The printed text as it may be shown: escape sequences removed, and any pasted code replaced.
    fn shown(&self) -> String {
        let mut t = plain(&self.text);
        for code in &self.pasted {
            t = t.replace(code.as_str(), "[the pasted code]");
        }
        t
    }
}

/// One running `claude auth login`. Every method takes `&self`, so the app's requests (show the URL, a pasted code,
/// cancel) can come from anywhere while it runs. There is no time limit of ours: a sign-in lasts as long as its command
/// lives. Dropping the handle before the command ended ends it, so that no sign-in runs on with nobody to finish it.
pub struct SignIn {
    /// The command's process; it leads a process group of its own, which a cancel ends.
    pid: i32,
    stdin: tokio::sync::Mutex<Option<ChildStdin>>,
    printed: Arc<Mutex<Printed>>,
    url: watch::Receiver<Option<String>>,
    end: watch::Receiver<Option<Ended>>,
}

impl SignIn {
    /// Start Claude Code's sign-in: PROGRAM (`claude`) `auth login [--email EMAIL]` under the account's CONFIG folder
    /// ([`Store::config`]). The environment is the command's own and nothing of coordd's (no `ANTHROPIC_*`, no
    /// `CLAUDE_CODE_*`): `CLAUDE_CONFIG_DIR`, `HOME`, `PATH`, and `BROWSER` a program that does nothing, since coordd may
    /// run where the owner's browser is not (he works in a Windows window, coordd in WSL) and the app shows the URL.
    /// Must be called inside a tokio runtime.
    pub fn start(program: &Path, config: &Path, email: Option<&str>) -> Result<SignIn, String> {
        let mut cmd = command(program, config);
        cmd.args(["auth", "login"]);
        if let Some(e) = email.filter(|e| !e.is_empty()) {
            cmd.args(["--email", e]);
        }
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0);
        let mut child = cmd.spawn().map_err(|e| format!("sign-in: {} auth login: {e}", program.display()))?;
        let pid = child.id().map(|p| p as i32).ok_or("sign-in: the command ended before it began")?;
        let printed = Arc::new(Mutex::new(Printed::default()));
        let (url_tx, url) = watch::channel(None);
        let url_tx = Arc::new(url_tx);
        let (end_tx, end) = watch::channel(None);
        let readers = [
            child.stdout.take().map(|o| tokio::spawn(read(o, printed.clone(), url_tx.clone()))),
            child.stderr.take().map(|e| tokio::spawn(read(e, printed.clone(), url_tx.clone()))),
        ];
        drop(url_tx);
        let stdin = tokio::sync::Mutex::new(child.stdin.take());
        let p = printed.clone();
        tokio::spawn(async move {
            let status = child.wait().await;
            // what it printed last is read to the end before its last words are taken
            for r in readers.into_iter().flatten() {
                let _ = r.await;
            }
            let ended = match status {
                Ok(s) if s.success() => Ended::Ok,
                Ok(s) => Ended::Failed(last_words(&p.lock().unwrap().shown(), &how(s))),
                Err(e) => Ended::Failed(last_words(&p.lock().unwrap().shown(), &format!("could not be waited for: {e}"))),
            };
            let _ = end_tx.send(Some(ended));
        });
        Ok(SignIn { pid, stdin, printed, url, end })
    }

    /// The URL the owner opens: the first `https://` address the command printed. Waits until it is printed; an Err
    /// if the command ended without one (its last words).
    pub async fn url(&self) -> Result<String, String> {
        let mut rx = self.url.clone();
        if let Ok(u) = rx.wait_for(Option::is_some).await {
            return Ok(u.clone().unwrap_or_default());
        }
        Err(match self.wait().await {
            Ended::Failed(why) => why,
            Ended::Ok => "the sign-in ended without printing a URL".into(),
        })
    }

    /// Hand in the code the browser showed: written to the command's input as one line, as a terminal would. Whether
    /// it is right is the command's to say (it ends, or prints why). Never logged.
    pub async fn paste(&self, code: &str) -> Result<(), String> {
        let code = code.trim();
        if code.is_empty() || code.contains('\n') {
            return Err("sign-in: a code is one line, not empty".into());
        }
        self.printed.lock().unwrap().pasted.push(code.to_string());
        let mut stdin = self.stdin.lock().await;
        let Some(input) = stdin.as_mut() else {
            return Err("sign-in: it was cancelled".into());
        };
        let wrote = async {
            input.write_all(format!("{code}\n").as_bytes()).await?;
            input.flush().await
        };
        wrote.await.map_err(|_| "sign-in: the command takes no more input (it has ended)".to_string())
    }

    /// Wait for the command's end, however long that is.
    pub async fn wait(&self) -> Ended {
        let mut rx = self.end.clone();
        match rx.wait_for(Option::is_some).await {
            Ok(e) => e.clone().unwrap_or_else(|| Ended::Failed("lost".into())),
            Err(_) => Ended::Failed("sign-in: its watcher ended without a word".into()),
        }
    }

    /// The command's process, the leader of its own process group: for whoever started it to put it in a cgroup of
    /// its own.
    pub fn pid(&self) -> i32 {
        self.pid
    }

    /// How it ended, if it has (without waiting).
    pub fn ended(&self) -> Option<Ended> {
        self.end.borrow().clone()
    }

    /// Everything the command printed so far, as it may be shown (escape sequences and the pasted code taken out).
    pub fn printed(&self) -> String {
        self.printed.lock().unwrap().shown()
    }

    /// End the sign-in: its input is closed and its process group ended. A half-made login is Claude Code's to clean
    /// up: nothing in `config/` is removed. Returns how it ended.
    pub async fn cancel(&self) -> Ended {
        self.stdin.lock().await.take();
        self.kill();
        self.wait().await
    }

    fn kill(&self) {
        if self.end.borrow().is_none() {
            // SAFETY: signals the process group this handle started, whose leader has not been reaped yet (the end
            // is sent only after it is), so the id is still its own
            unsafe { libc::kill(-self.pid, libc::SIGTERM) };
        }
    }
}

impl Drop for SignIn {
    fn drop(&mut self) {
        self.kill();
    }
}

/// What Claude Code says of an account's login (`claude auth status --json`), the parts coordd keeps.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    /// Signed in: it exited 0 and said `loggedIn`.
    pub logged_in: bool,
    /// `authMethod`: "one of `none`, `claude.ai`, `oauth_token`, `api_key`, `api_key_helper`, or `third_party`".
    pub auth_method: Option<String>,
    /// `loginExpiresAt`, seconds since the epoch, if it said one.
    pub expires: Option<i64>,
    /// An e-mail or plan to show beside the account, if it said one: the first of `email`, `emailAddress`,
    /// `planTier`, `subscriptionType` (names seen only in the program's strings, not in a signed-in answer).
    pub shown: Option<String>,
    /// `email`, the address of the person signed in (Claude Code 2.1.331, for a `claude.ai` login), which the gateway's
    /// Move compares with the login it moves.
    pub email: Option<String>,
    /// `subscriptionType`, which Claude Code 2.1.329 says for a `claude.ai` login (its strings: pro, max, team,
    /// enterprise): the plan the placement's weekly ceilings are named by (policy/placement.eflint).
    pub plan: Option<String>,
}

/// Ask Claude Code whether the account whose folder is CONFIG is signed in: PROGRAM `auth status --json`, with the
/// sign-in's environment. Its doc: "Exits with code 0 if logged in, 1 if not". An Err when it could not be asked or
/// answered otherwise.
pub async fn status(program: &Path, config: &Path) -> Result<Status, String> {
    let mut cmd = command(program, config);
    cmd.args(["auth", "status", "--json"]).stdin(Stdio::null());
    let out = cmd.output().await.map_err(|e| format!("{} auth status: {e}", program.display()))?;
    let code = out.status.code();
    let said = || format!("{} {}", String::from_utf8_lossy(&out.stdout).trim(), plain(&String::from_utf8_lossy(&out.stderr)).trim());
    if !matches!(code, Some(0 | 1)) {
        return Err(format!("{} auth status {}: {}", program.display(), how(out.status), said()));
    }
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("{} auth status: not JSON ({e}): {}", program.display(), said()))?;
    let text = |k: &str| v.get(k).and_then(|x| x.as_str()).filter(|s| !s.is_empty()).map(str::to_string);
    Ok(Status {
        logged_in: code == Some(0) && v.get("loggedIn").and_then(|x| x.as_bool()) == Some(true),
        auth_method: text("authMethod"),
        expires: v.get("loginExpiresAt").and_then(seconds),
        shown: ["email", "emailAddress", "planTier", "subscriptionType"].into_iter().find_map(text),
        email: text("email"),
        plan: text("subscriptionType"),
    })
}

/// Claude Code's own sign-out of the account whose folder is CONFIG: PROGRAM `auth logout`, with the sign-in's
/// environment. The login is Claude Code's to end as it was Claude Code's to make; coordd reads and deletes nothing
/// of it. An Err says why it could not be run or what it said when it failed.
pub async fn logout(program: &Path, config: &Path) -> Result<(), String> {
    let mut cmd = command(program, config);
    cmd.args(["auth", "logout"]);
    run(cmd, &format!("{} auth logout", program.display())).await.map(|_| ())
}

/// CMD (one of [`command`]'s) run to its end with nothing on its input: what it printed; or, when it failed, WHAT (the
/// command in words) with how it ended and what it said. Claude Code's sign-out here, and its window opening
/// (opener.rs).
pub(super) async fn run(mut cmd: Command, what: &str) -> Result<String, String> {
    let out = cmd.stdin(Stdio::null()).kill_on_drop(true).output().await.map_err(|e| format!("{what}: {e}"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let said = format!("{} {}", plain(&String::from_utf8_lossy(&out.stdout)).trim(), plain(&String::from_utf8_lossy(&out.stderr)).trim());
    Err(format!("{what} {}: {}", how(out.status), said.trim()))
}

/// Put what [`status`] said into account ID's `state.json`, at NOW (seconds).
pub fn record(store: &Store, h: Harness, id: &str, s: &Status, now: i64) -> Result<State, String> {
    // off the core's lock (a sign-in's end, `coord account check`), as every change of the store
    store.update(h, id, |st| {
        // a new sign-in: whether messages reach its sessions is not known yet, and its next agent's test says
        if s.logged_in && !st.signed_in {
            (st.heard, st.missed, st.deaf) = (None, None, None);
            // and the window opener looks at it at the next round
            st.window_look = None;
        }
        st.signed_in = s.logged_in;
        st.signed_in_seen = now;
        st.login_expires = s.expires;
        st.auth_method = s.auth_method.clone();
        st.plan = s.plan.clone();
        st.signed_in_as = s.email.clone().filter(|_| s.logged_in);
        // signed in again after a put-back: the card's "sign in again" is over for good
        if s.logged_in {
            st.put_back = None;
        }
    })
}

/// PROGRAM run under the account folder CONFIG with nothing of coordd's environment (see [`SignIn::start`]).
pub(super) fn command(program: &Path, config: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.env_clear().envs(env(config)).current_dir(config);
    cmd
}

/// The whole environment of Claude Code run on the account folder CONFIG ([`command`]; a reset's session, resets.rs):
/// that folder, the owner's home and PATH, and a browser that does nothing.
pub(super) fn env(config: &Path) -> [(&'static str, std::ffi::OsString); 4] {
    [
        ("CLAUDE_CONFIG_DIR", config.into()),
        ("HOME", std::env::var_os("HOME").unwrap_or_default()),
        ("PATH", std::env::var_os("PATH").unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin".into())),
        ("BROWSER", quiet().into()),
    ]
}

/// A program that does nothing, for `BROWSER`.
fn quiet() -> PathBuf {
    ["/bin/true", "/usr/bin/true"].into_iter().map(PathBuf::from).find(|p| p.exists()).unwrap_or_else(|| "true".into())
}

/// Read one of the command's streams to its end into PRINTED, and give the first whole URL in it to URL.
async fn read(mut from: impl AsyncRead + Unpin, printed: Arc<Mutex<Printed>>, url: Arc<watch::Sender<Option<String>>>) {
    let mut buf = [0u8; 4096];
    let mut pending: Vec<u8> = Vec::new(); // the end of a character split between two reads
    let mut own = String::new(); // this stream alone, where its URL is looked for
    loop {
        let n = from.read(&mut buf).await.unwrap_or(0);
        let more = n > 0;
        pending.extend_from_slice(&buf[..n]);
        let whole = match std::str::from_utf8(&pending) {
            Err(e) if more && e.error_len().is_none() => e.valid_up_to(),
            _ => pending.len(),
        };
        let piece = String::from_utf8_lossy(&pending[..whole]).into_owned();
        pending.drain(..whole);
        own.push_str(&piece);
        printed.lock().unwrap().text.push_str(&piece);
        if url.borrow().is_none() {
            if let Some(u) = first_url(&plain(&own), !more) {
                let _ = url.send(Some(u));
            }
        }
        if !more {
            return;
        }
    }
}

/// The first `https://` address in TEXT, once it is whole: followed by a space, a quote or a line end, or, at the
/// stream's END, by nothing. A trailing `.`, `,`, `;` or `)` is the sentence's, not the URL's.
fn first_url(text: &str, end: bool) -> Option<String> {
    let at = text.find("https://")?;
    let rest = &text[at..];
    let url = match rest.find(|c: char| c.is_whitespace() || "\"'<>`".contains(c)) {
        Some(i) => &rest[..i],
        None if end => rest,
        None => return None,
    };
    let url = url.trim_end_matches(['.', ',', ';', ')']);
    (url.len() > "https://".len()).then(|| url.to_string())
}

/// TEXT without terminal escape sequences (colours, cursor moves, OSC 8 links, whose visible text stays) and with
/// carriage returns as line ends.
pub(super) fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\u{1b}' => match it.next() {
                // CSI: parameters, then one final character from @ to ~
                Some('[') => {
                    for d in it.by_ref() {
                        if ('@'..='~').contains(&d) {
                            break;
                        }
                    }
                }
                // OSC: up to BEL or ESC \
                Some(']') => {
                    while let Some(d) = it.next() {
                        if d == '\u{7}' || (d == '\u{1b}' && it.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\r' => {
                if it.peek() != Some(&'\n') {
                    out.push('\n');
                }
            }
            c if c.is_control() && c != '\n' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

/// The command's last words: what it printed (its last 2000 characters, which for a sign-in is all of it) and how it
/// ended.
pub(super) fn last_words(shown: &str, how: &str) -> String {
    let t = shown.trim();
    let cut = t.char_indices().rev().nth(1999).map(|(i, _)| &t[i..]).unwrap_or(t);
    if cut.is_empty() { format!("the sign-in {how}, printing nothing") } else { format!("{cut}\n(the sign-in {how})") }
}

/// How a process ended, in words.
pub(super) fn how(s: std::process::ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match (s.code(), s.signal()) {
        (Some(c), _) => format!("exited with {c}"),
        (None, Some(sig)) => format!("was ended by signal {sig}"),
        _ => "ended".into(),
    }
}

/// A time as seconds since the epoch: a number (seconds, or milliseconds when that large) or an RFC 3339 string.
fn seconds(v: &serde_json::Value) -> Option<i64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64().map(|f| if f > 1e11 { (f / 1000.0) as i64 } else { f as i64 }),
        serde_json::Value::String(s) => chrono::DateTime::parse_from_rfc3339(s).ok().map(|t| t.timestamp()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::Tmp;
    use super::*;
    use std::time::Duration;

    /// A stand-in `claude` in the test's folder. `auth login` records its environment's names, prints the lines the
    /// program's strings have (its URL bold and inside an OSC 8 link, a prompt with no line end), then runs LOGIN
    /// (shell); `auth status` says signed in once LOGIN has made `$CLAUDE_CONFIG_DIR/.signed`.
    fn stand_in(t: &Tmp, login: &str) -> PathBuf {
        let script = format!(
            r#"#!/bin/sh
case "$1 $2" in
"auth login")
  env | cut -d= -f1 | sort > "$CLAUDE_CONFIG_DIR/../env-names"
  echo "Opening browser to sign in..."
  printf 'If the browser didn'"'"'t open, visit: \033]8;;https://claude.com/x\033\\\033[1mhttps://claude.com/cai/oauth/authorize?code=true&client_id=c&state=s1\033[0m\033]8;;\033\\\n'
  printf 'Paste code here if prompted > '
  {login}
  ;;
"auth status")
  if [ -f "$CLAUDE_CONFIG_DIR/.signed" ]; then
    echo '{{"loggedIn": true, "authMethod": "claude.ai", "loginExpiresAt": "2026-12-18T12:00:00.000Z", "email": "a@example.com", "subscriptionType": "max"}}'; exit 0
  fi
  echo '{{"loggedIn": false, "authMethod": "none", "loginExpiresAt": null}}'; exit 1
  ;;
"auth logout")
  if [ -f "$CLAUDE_CONFIG_DIR/.offline" ]; then echo "Failed to log out: network error" >&2; exit 1; fi
  rm -f "$CLAUDE_CONFIG_DIR/.signed"; echo "Successfully logged out from your Anthropic account."
  ;;
esac
"#
        );
        executable(t, &script)
    }

    /// SCRIPT as the program `claude` in the test's folder, made by `cp` from text written beside it, so this process never
    /// holds the program open for writing (ETXTBSY in a child started meanwhile).
    fn executable(t: &Tmp, script: &str) -> PathBuf {
        t.program("claude", script)
    }

    /// The login part that takes a code from its input: "good" signs in, anything else is refused, naming the code
    /// (so that the test sees it taken out of what is shown).
    const READS_CODE: &str = r#"read code
  if [ "$code" = good ]; then echo "Login successful."; touch "$CLAUDE_CONFIG_DIR/.signed"; exit 0; fi
  echo "OAuth error: invalid code $code"; exit 1"#;

    /// F, given at most 20 s: a stand-in that hangs fails the test instead of the queue.
    async fn soon<T>(f: impl std::future::Future<Output = T>) -> T {
        tokio::time::timeout(Duration::from_secs(20), f).await.expect("the stand-in took too long")
    }

    fn account(t: &Tmp) -> PathBuf {
        t.store.create(Harness::Claude, "work", "Work", 0, crate::accounts::Kind::Signin, "2026-11-18T12:00:00Z").unwrap();
        t.store.config(Harness::Claude, "work")
    }

    #[tokio::test]
    async fn a_pasted_code_signs_in_and_status_says_so() {
        let t = Tmp::new("signin-paste");
        let (claude, config) = (stand_in(&t, READS_CODE), account(&t));
        assert!(!soon(status(&claude, &config)).await.unwrap().logged_in);
        let s = SignIn::start(&claude, &config, Some("a@example.com")).unwrap();
        assert_eq!(soon(s.url()).await.unwrap(), "https://claude.com/cai/oauth/authorize?code=true&client_id=c&state=s1");
        soon(s.paste("  good \n")).await.unwrap();
        assert_eq!(soon(s.wait()).await, Ended::Ok);
        assert!(s.printed().contains("Login successful"));

        let st = soon(status(&claude, &config)).await.unwrap();
        let want = Status { logged_in: true, auth_method: Some("claude.ai".into()), expires: Some(1_797_595_200), shown: Some("a@example.com".into()), email: Some("a@example.com".into()), plan: Some("max".into()) };
        assert_eq!(st, want);
        let state = record(&t.store, Harness::Claude, "work", &st, 1_000).unwrap();
        assert!(state.signed_in && state.signed_in_seen == 1_000 && state.login_expires == Some(1_797_595_200));
        assert_eq!(state.signed_in_as.as_deref(), Some("a@example.com"), "whom it signed in, kept");
        assert_eq!(state.plan.as_deref(), Some("max"));
        // nothing of coordd's environment reached the command
        let names = std::fs::read_to_string(config.join("../env-names")).unwrap();
        for n in names.lines() {
            assert!(["BROWSER", "CLAUDE_CONFIG_DIR", "HOME", "PATH", "PWD", "SHLVL", "_", "OLDPWD"].contains(&n), "{n} reached the command");
        }
    }

    #[tokio::test]
    async fn claude_code_signs_an_account_out_itself_and_a_failure_says_its_words() {
        let t = Tmp::new("signin-logout");
        let (claude, config) = (stand_in(&t, READS_CODE), account(&t));
        std::fs::write(config.join(".signed"), "").unwrap();
        assert!(soon(status(&claude, &config)).await.unwrap().logged_in);
        // it cannot (no network): said with the command's words, and the login is as it was
        std::fs::write(config.join(".offline"), "").unwrap();
        let why = soon(logout(&claude, &config)).await.unwrap_err();
        assert!(why.contains("auth logout exited with 1") && why.contains("network error"), "{why}");
        assert!(soon(status(&claude, &config)).await.unwrap().logged_in);
        std::fs::remove_file(config.join(".offline")).unwrap();
        soon(logout(&claude, &config)).await.unwrap();
        assert!(!soon(status(&claude, &config)).await.unwrap().logged_in);
    }

    #[tokio::test]
    async fn a_refused_code_fails_with_the_commands_words_and_without_the_code() {
        let t = Tmp::new("signin-refused");
        let (claude, config) = (stand_in(&t, READS_CODE), account(&t));
        let s = SignIn::start(&claude, &config, None).unwrap();
        soon(s.url()).await.unwrap();
        soon(s.paste("wrong-SECRET-1")).await.unwrap();
        let Ended::Failed(why) = soon(s.wait()).await else { panic!("signed in on a wrong code") };
        assert!(why.contains("invalid code [the pasted code]") && why.contains("exited with 1"), "{why}");
        assert!(!why.contains("SECRET") && !why.contains('\u{1b}'), "{why}");
        assert!(soon(s.paste("again")).await.is_err());
        assert!(!soon(status(&claude, &config)).await.unwrap().logged_in);
    }

    #[tokio::test]
    async fn the_redirect_ends_it_with_no_code_and_a_cancel_ends_a_waiting_one() {
        let t = Tmp::new("signin-redirect");
        let config = account(&t);
        // the browser's redirect: the command finishes by itself
        let claude = stand_in(&t, r#"touch "$CLAUDE_CONFIG_DIR/.signed"; echo "Login successful."; exit 0"#);
        let s = SignIn::start(&claude, &config, None).unwrap();
        assert_eq!(soon(s.wait()).await, Ended::Ok);
        assert!(soon(s.url()).await.unwrap().starts_with("https://claude.com/cai/oauth/"));

        // nobody signs in: cancel ends the command, and config/ keeps what is in it
        let claude = stand_in(&t, "sleep 1000");
        let s = SignIn::start(&claude, &config, None).unwrap();
        soon(s.url()).await.unwrap();
        assert!(s.ended().is_none());
        let Ended::Failed(why) = soon(s.cancel()).await else { panic!("a cancelled sign-in succeeded") };
        assert!(why.contains("signal"), "{why}");
        assert!(config.join("projects").exists() && config.join(".signed").exists());
    }

    #[tokio::test]
    async fn a_command_that_ends_without_a_url_says_why() {
        let t = Tmp::new("signin-nourl");
        let config = account(&t);
        let p = executable(&t, "#!/bin/sh\necho 'error: unknown option --email' >&2\nexit 2\n");
        let s = SignIn::start(&p, &config, Some("x@example.com")).unwrap();
        let why = soon(s.url()).await.unwrap_err();
        assert!(why.contains("unknown option") && why.contains("exited with 2"), "{why}");
        assert!(SignIn::start(&t.dir.join("none"), &config, None).is_err());
        assert!(soon(status(&p, &config)).await.unwrap_err().contains("exited with 2"));
    }

    #[test]
    fn a_url_is_taken_whole_and_escape_sequences_are_dropped() {
        assert_eq!(first_url("visit: https://a.b/c?d=e", false), None);
        assert_eq!(first_url("visit: https://a.b/c?d=e", true).as_deref(), Some("https://a.b/c?d=e"));
        assert_eq!(first_url("Sign in at: https://a.b/c.\nmore", false).as_deref(), Some("https://a.b/c"));
        assert_eq!(first_url("no address here\n", true), None);
        assert_eq!(plain("\u{1b}[1mbold\u{1b}[0m \u{1b}]8;;https://x\u{1b}\\link\u{1b}]8;;\u{7} a\r\nb\rc"), "bold link a\nb\nc");
        assert_eq!(seconds(&serde_json::json!(1_797_595_200_000_i64)), Some(1_797_595_200));
        assert_eq!(seconds(&serde_json::json!(null)), None);
    }
}
