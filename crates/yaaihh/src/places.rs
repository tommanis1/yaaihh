//! Where yaaihh keeps its own things, and how it reaches the machine its coordds run on (N-412 ask 13: yaaihh on
//! Windows, its coordds in WSL). Two kinds of place, kept apart here:
//! - **the app's own**, on the machine it runs on: its config folder (projects.toml, pins, read marks, the window's
//!   state), its log folder and the folder of its one-per-login socket. On Linux the XDG folders, as before; on
//!   Windows %APPDATA%\yaaihh and %LOCALAPPDATA%\yaaihh.
//! - **the server's**: every command that reaches a coordd (`coordc bridge`, `host-coordd start`, `roster …`) runs on
//!   the machine the coordds run on. On Linux that is this one; on Windows it is a WSL distribution, reached through
//!   `wsl.exe` (projects.toml's `wsl`, the distribution's name; none: WSL's default one). A project whose server is on
//!   another device (projects.toml's `[[device]]`, the project's `device`) is reached by SSH instead ([`on`]).

use std::path::PathBuf;
use std::sync::OnceLock;

/// projects.toml's `wsl`: the WSL distribution the server's commands run in. Set once at start.
pub static WSL: OnceLock<Option<String>> = OnceLock::new();

fn env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn home() -> PathBuf {
    env("HOME").or_else(|| env("USERPROFILE")).unwrap_or_default()
}

/// The folder of the config file, when YAAIHH_CONFIG does not name one: `~/.config/yaaihh` as always (not
/// XDG_CONFIG_HOME, which would move an existing config), on Windows `%APPDATA%\yaaihh`.
pub fn config_dir() -> PathBuf {
    if cfg!(windows) {
        return env("APPDATA").unwrap_or_else(|| home().join("AppData").join("Roaming")).join("yaaihh");
    }
    home().join(".config").join("yaaihh")
}

/// The folder the log folders sit in: XDG_STATE_HOME, else `~/.local/state`; on Windows `%LOCALAPPDATA%`.
pub fn state_dir() -> PathBuf {
    if cfg!(windows) {
        return env("LOCALAPPDATA").unwrap_or_else(|| home().join("AppData").join("Local"));
    }
    env("XDG_STATE_HOME").unwrap_or_else(|| home().join(".local").join("state"))
}

/// The folder of the one-per-login socket: XDG_RUNTIME_DIR, else the temporary folder.
pub fn runtime_dir() -> PathBuf {
    env("XDG_RUNTIME_DIR").unwrap_or_else(std::env::temp_dir)
}

/// CMD as run on the server's machine: as it is on Linux without `wsl`; through `wsl.exe` otherwise (on Windows
/// always, with `wsl` naming the distribution or not). The command's own arguments are passed as they are (`-e`: no
/// shell in between).
pub fn server(cmd: Vec<String>) -> Vec<String> {
    server_in(cmd, WSL.get().and_then(Option::as_deref), cfg!(windows))
}

pub(crate) fn server_in(cmd: Vec<String>, wsl: Option<&str>, windows: bool) -> Vec<String> {
    if wsl.is_none() && !windows {
        return cmd;
    }
    let mut out = vec!["wsl.exe".to_string()];
    if let Some(d) = wsl.filter(|d| !d.is_empty()) {
        out.extend(["-d".to_string(), d.to_string()]);
    }
    out.push("-e".into());
    out.extend(cmd);
    out
}

/// A device a project's server runs on other than this PC (projects.toml's `[[device]]`, named by a project's
/// `device`; the owner's N-1104 p1070, docs/specs/devices.md step 2): reached by SSH, over Tailscale, as his own user
/// with his own keys and known hosts, which ssh reads and nothing here does.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct Device {
    /// The name a project's `device` gives.
    pub name: String,
    /// Where ssh goes: `USER@HOST`, or a Host of his ssh config.
    pub ssh: String,
    /// The agent-base folder there; none: the config's own `agent_base`.
    pub agent_base: Option<String>,
}

/// The SSH client: Windows' own OpenSSH (`ssh.exe`, on its PATH), or Linux's.
pub const SSH: &str = "ssh";

/// CMD as run on DEVICE when the project names one ([`ssh_in`]); else on this PC's server ([`server`]).
pub fn on(cmd: Vec<String>, device: Option<&Device>) -> Vec<String> {
    match device {
        Some(d) => ssh_in(cmd, &d.ssh),
        None => server(cmd),
    }
}

/// CMD run on HOST by ssh, with no prompt: a host it does not know or a key it lacks fails at once, in ssh's words (he
/// makes the first connection himself, in a terminal); a link gone quiet ends within 45 s, so the pipe starts again
/// (link.rs); nothing forwarded and no terminal. The remote shell reads the words ssh joins, so each is [`quote`]d.
pub(crate) fn ssh_in(cmd: Vec<String>, host: &str) -> Vec<String> {
    let opts = ["BatchMode=yes", "ConnectTimeout=10", "ServerAliveInterval=15", "ServerAliveCountMax=3", "ForwardAgent=no", "ForwardX11=no", "ClearAllForwardings=yes"];
    let mut out = vec![SSH.to_string(), "-T".into()];
    out.extend(opts.iter().flat_map(|o| ["-o".to_string(), o.to_string()]));
    out.extend(["--".to_string(), host.to_string()]);
    out.extend(cmd.iter().map(|w| quote(w)));
    out
}

/// WORD for a POSIX shell: as it is when every character is plain (letters, digits and `/._-:@,+%`), else in single
/// quotes, a quote in it written `'\''`; an empty word is `''`.
pub fn quote(w: &str) -> String {
    if !w.is_empty() && w.chars().all(|c| c.is_ascii_alphanumeric() || "/._-:@,+%".contains(c)) {
        return w.to_string();
    }
    format!("'{}'", w.replace('\'', r"'\''"))
}

/// ARGS added to CMD, a command made by [`on`]: quoted when it goes by ssh (the remote shell reads them), as they are
/// otherwise.
pub fn more(cmd: &mut Vec<String>, args: impl IntoIterator<Item = String>) {
    let by_ssh = cmd.first().is_some_and(|c| c == SSH);
    cmd.extend(args.into_iter().map(|a| if by_ssh { quote(&a) } else { a }));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A command reaches the coordds as it is on Linux, and through wsl.exe on Windows or with `wsl` set (N-412 ask 13).
    #[test]
    fn a_server_command_goes_through_wsl_on_windows_or_when_named() {
        let cmd = || vec!["/b/bin/host-coordd".to_string(), "sh".into(), "research".into(), "coordc".into(), "bridge".into()];
        assert_eq!(server_in(cmd(), None, false), cmd());
        assert_eq!(server_in(cmd(), None, true), [vec!["wsl.exe".to_string(), "-e".into()], cmd()].concat());
        assert_eq!(server_in(cmd(), Some("Ubuntu"), true), [vec!["wsl.exe".to_string(), "-d".into(), "Ubuntu".into(), "-e".into()], cmd()].concat());
        assert_eq!(server_in(cmd(), Some("Ubuntu"), false)[..4], ["wsl.exe", "-d", "Ubuntu", "-e"]);
        assert_eq!(server_in(cmd(), Some(""), true)[..2], ["wsl.exe", "-e"]);
    }

    /// A project on another device: its commands go by ssh with no prompt, nothing forwarded and a quiet link ended,
    /// each word quoted for the remote shell, and so are words added later (link.rs's `--from`); with no device, as
    /// before.
    #[test]
    fn a_command_for_another_device_goes_by_ssh_each_word_quoted() {
        let d = Device { name: "tower".into(), ssh: "t@tower-server".into(), agent_base: None };
        let cmd = vec!["/b/bin/host-coordd".to_string(), "sh".into(), "research".into(), "coordc".into(), "bridge".into(), "--box".into(), "win 1".into()];
        let c = on(cmd.clone(), Some(&d));
        assert_eq!(c[..2], ["ssh", "-T"]);
        for o in ["BatchMode=yes", "ConnectTimeout=10", "ServerAliveInterval=15", "ServerAliveCountMax=3", "ForwardAgent=no", "ForwardX11=no", "ClearAllForwardings=yes"] {
            assert!(c.windows(2).any(|w| w == ["-o", o]), "{o}: {c:?}");
        }
        let at = c.iter().position(|w| w == "--").unwrap();
        assert_eq!(c[at + 1], "t@tower-server");
        assert_eq!(c[at + 2..], ["/b/bin/host-coordd", "sh", "research", "coordc", "bridge", "--box", "'win 1'"]);
        assert_eq!(on(cmd.clone(), None), server(cmd.clone()));

        let mut c = c;
        more(&mut c, ["--from".into(), "2026-11-18T10:01:56Z-4242:17".into(), "it's".into()]);
        assert_eq!(c[c.len() - 3..], ["--from", "2026-11-18T10:01:56Z-4242:17", r"'it'\''s'"]);
        let mut here = server(cmd);
        more(&mut here, ["it's".into()]);
        assert_eq!(here.last().unwrap(), "it's", "not by ssh: as it is");
    }

    /// A word is quoted unless every character is plain; a POSIX shell reads each quoted word back as it was.
    #[test]
    fn a_word_quoted_reads_back_as_it_was_in_a_posix_shell() {
        assert_eq!(quote("/b/c-d_e.f:1@x,y+z%"), "/b/c-d_e.f:1@x,y+z%");
        for (w, q) in [("", "''"), ("a b", "'a b'"), ("$HOME", "'$HOME'"), ("~/x", "'~/x'"), ("a;b", "'a;b'"), ("X=1", "'X=1'"), ("it's", r"'it'\''s'")] {
            assert_eq!(quote(w), q);
        }
        #[cfg(unix)]
        {
            let words = ["", "a b", "$HOME", "~/x", "a;b|c&d", "X=1", "it's", "two\nlines", "`id`", "\"q\"", "*"];
            let line = format!("printf '%s\\0' {}", words.iter().map(|w| quote(w)).collect::<Vec<_>>().join(" "));
            let out = std::process::Command::new("sh").arg("-c").arg(&line).current_dir(std::env::temp_dir()).output().unwrap();
            let back: Vec<String> = String::from_utf8(out.stdout).unwrap().split('\0').map(String::from).collect();
            assert_eq!(back[..words.len()], words.map(String::from));
        }
    }
}

/// A command to run PROGRAM. On Windows with no console of its own: the exe is a window program (main.rs), and each
/// console program it starts (`wsl.exe` for every pipe and run) would otherwise open a terminal window of its own.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    #[allow(unused_mut)]
    let mut c = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}
