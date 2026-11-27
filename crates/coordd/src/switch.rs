//! A coord-switch's mark: `switching` beside coordd's socket, holding the pid and start time of the process that
//! switches it. release.sh's coord-switch puts it up while it stops coordd and starts it again, and coordd itself while
//! it starts again in place (restart.rs); coordc's hook, finding no coordd, waits while it stands and that process lives
//! (client.rs `hook_ask`), so no hook is lost while coordd is down.

/// Whether a coord-switch runs for the coordd at SOCK: the mark `switching` in SOCK.d stands and the process it names
/// (pid and start time) lives, so a pid reused after a killed switch holds nothing. A boxed agent's SOCK is a link into
/// that folder and a box shares the host's processes, so it sees the same. No time of ours bounds the wait.
// coordc's: coordd puts its mark up and takes it down
#[allow(dead_code)]
pub fn switching(sock: &str) -> bool {
    stands(&switch_mark(sock)) == Some(true)
}

/// When process PID started (field 22 of /proc/PID/stat, after its name in brackets, which may hold spaces); none when
/// it is gone.
pub fn started(pid: &str) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    start_in(&stat).map(String::from)
}

/// The start time in a process's STAT: field 22, the 20th after its name in brackets.
fn start_in(stat: &str) -> Option<&str> {
    stat.rsplit_once(')')?.1.split_whitespace().nth(19)
}

/// The mark's path for SOCK: beside the socket in its folder when SOCK is the folder's `coordd.sock` (or a link to it,
/// a box's), else in SOCK.d. A link is read, not followed to its end: the socket it leads to is gone in a switch.
pub fn switch_mark(sock: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(sock);
    let p = match std::fs::read_link(p) {
        Ok(t) if t.is_relative() => p.parent().unwrap_or(std::path::Path::new("/")).join(t),
        Ok(t) => t,
        Err(_) => p.to_path_buf(),
    };
    if p.file_name().is_some_and(|n| n == "coordd.sock") {
        p.with_file_name("switching")
    } else {
        std::path::PathBuf::from(format!("{}.d/switching", p.display()))
    }
}

/// Where the coordd at SOCK keeps its own mark ready, whole, beside the mark's place (MARK): written at its start
/// ([`stage_mine`]), put up by a rename ([`mark_mine`]).
fn staged(mark: &std::path::Path) -> std::path::PathBuf {
    let mut p = mark.as_os_str().to_owned();
    p.push(".coordd");
    std::path::PathBuf::from(p)
}

/// This process's mark made ready for the coordd at SOCK, once its socket is bound: its pid and start time, written
/// beside the mark's place, so that putting it up takes no file descriptor ([`mark_mine`]) even when none are left.
pub fn stage_mine(sock: &str) -> std::io::Result<()> {
    let path = staged(&switch_mark(sock));
    let me = std::process::id().to_string();
    let start = started(&me).ok_or_else(|| std::io::Error::other("its own start time could not be read"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, format!("{me} {start}\n"))
}

/// Whether the mark at PATH names a living process (the switch's pid and start time): None when that cannot be read
/// (no descriptor to read it with).
fn stands(path: &std::path::Path) -> Option<bool> {
    let gone = |e: &std::io::Error| e.kind() == std::io::ErrorKind::NotFound;
    let mark = match std::fs::read_to_string(path) {
        Ok(m) => m,
        Err(e) => return gone(&e).then_some(false),
    };
    let mut w = mark.split_whitespace();
    let (Some(pid), Some(start)) = (w.next(), w.next()) else { return Some(false) };
    if pid.parse::<u32>().is_err() {
        return Some(false);
    }
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => Some(start_in(&stat) == Some(start)),
        Err(e) => gone(&e).then_some(false),
    }
}

/// The mark put up for this process, the coordd at SOCK starting again in place: the one made ready at its start
/// ([`stage_mine`]) renamed into place, which takes no descriptor, and only where no mark stands. A switch's mark that
/// stands is left as it is (that switch's own start takes it down), and one whose process is gone is written over; one
/// that cannot be read is left, and said. It names this process until the new program answers ([`unmark_mine`]). Its
/// path, and whether this process put it up (not a switch's mark left as it stood).
pub fn mark_mine(sock: &str) -> std::io::Result<(std::path::PathBuf, bool)> {
    use std::os::unix::ffi::OsStrExt;
    let path = switch_mark(sock);
    let ready = staged(&path);
    let c = |p: &std::path::Path| std::ffi::CString::new(p.as_os_str().as_bytes()).map_err(std::io::Error::other);
    let (from, to) = (c(&ready)?, c(&path)?);
    // SAFETY: two paths, NUL-ended, that live across the call; AT_FDCWD for both folders
    let r = unsafe { libc::syscall(libc::SYS_renameat2, libc::AT_FDCWD, from.as_ptr(), libc::AT_FDCWD, to.as_ptr(), libc::RENAME_NOREPLACE) };
    if r == 0 {
        return Ok((path, true));
    }
    let e = std::io::Error::last_os_error();
    // a mark there already, or a file system that takes no RENAME_NOREPLACE: what stands there decides
    if !matches!(e.raw_os_error(), Some(libc::EEXIST) | Some(libc::EINVAL)) {
        return Err(e);
    }
    match stands(&path) {
        Some(true) => Ok((path, false)),
        Some(false) => std::fs::rename(&ready, &path).map(|()| (path, true)),
        None => Err(std::io::Error::other(format!("a mark stands at {} that could not be read, left as it is", path.display()))),
    }
}

/// The mark this process put up for the coordd at SOCK ([`mark_mine`]) taken back to where it was made ready, its start
/// in place having failed: down and ready for the next, in one rename, which takes no file descriptor.
pub fn unmark_back(sock: &str) -> std::io::Result<()> {
    let path = switch_mark(sock);
    std::fs::rename(&path, staged(&path))
}

/// The mark taken down once the coordd at SOCK answers again, when it names this process ([`mark_mine`]); another's
/// stays.
pub fn unmark_mine(sock: &str) {
    let path = switch_mark(sock);
    let me = std::process::id().to_string();
    let Ok(mark) = std::fs::read_to_string(&path) else { return };
    let mut w = mark.split_whitespace();
    if w.next() == Some(me.as_str()) && w.next().is_some_and(|s| started(&me).as_deref() == Some(s)) {
        let _ = std::fs::remove_file(&path);
    }
}
