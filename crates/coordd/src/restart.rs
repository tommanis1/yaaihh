//! coordd started again in place when its record stops taking lines (paper-research N-1396: "coordd must not sit
//! latched: on this error it restarts itself the way coord-switch does, and says so in its journal"). A ticket or mail
//! log line that failed to go in (jsonl.rs: the disk full, no descriptor left) latched the record, and coordd took
//! nothing more until it was restarted by hand. Now it runs its own program again in the same process: a coord-switch's
//! mark stands meanwhile (switch.rs), so no hook is lost, and the new program replays the record up to the line that
//! failed, as any start does. The process stays (its unit, its cgroup, its pid and its start time, which the mark
//! names), and every descriptor it held is let go. It says so in its journal (coordd.log) before and after. A start in
//! place that took no record line before its record failed again is not done again: the fault outlasts a start (the
//! disk still full), and coordd stays latched, with its notice, until it is restarted by hand.

use std::ffi::OsString;
use std::sync::OnceLock;

/// The environment's word for why the coordd before this one started it again: its record's "PATH: ERROR".
const FOR: &str = "COORDD_RESTARTED_FOR";

/// What this process was started with: the allocator's arena limit (main takes it out of the environment, so that
/// nothing coordd starts inherits it; a start in place puts it back), and why the coordd before it started it again.
struct Start {
    arena: Option<OsString>,
    after: Option<String>,
}

static START: OnceLock<Start> = OnceLock::new();

/// First thing in main, one thread so far: what this process was started with, taken. A start in place says so in the
/// journal, and the children the coordd before it left are waited for ([`reap_inherited`]).
pub fn at_start() {
    let arena = std::env::var_os("MALLOC_ARENA_MAX");
    let after = std::env::var(FOR).ok();
    if let Some(why) = &after {
        // SAFETY: one thread so far (main); nothing coordd starts is told why it was started
        unsafe { std::env::remove_var(FOR) };
        eprintln!("coordd: started again in place (pid {}): the coordd before it could not write its record ({why})", std::process::id());
        reap_inherited();
    }
    let _ = START.set(Start { arena, after });
}

/// coordd's socket SOCK bound (main.rs run): the mark a start in place put up taken down, now that a hook that asks
/// is answered, and the next one made ready, so that putting it up takes no descriptor (switch.rs `stage_mine`).
pub fn up(sock: &str) {
    crate::switch::unmark_mine(sock);
    if let Err(e) = crate::switch::stage_mine(sock) {
        eprintln!("coordd: the mark for a start in place is not made ready ({e}): such a start would put up none for hooks to wait on");
    }
}

/// coordd on SOCK, its record having failed (WHY, "PATH: ERROR"), started again in place: what the writer thread holds
/// written first, the switch's mark put up (made ready at the start: no descriptor is taken, where the fault may be
/// that none is left), then its program run again in this process, with its own arguments and environment. The caller holds the core's lock, so nothing more is taken meanwhile. Returns only when that is not done,
/// said in the journal: this coordd was itself started in place and its record took no line since, or the program
/// could not be run.
pub fn again(sock: &str, why: &str) {
    let start = START.get();
    if let Some(before) = start.and_then(|s| s.after.as_deref()).filter(|_| !crate::jsonl::took()) {
        eprintln!("coordd: {why}: not started again: this coordd, started in place of one whose record failed ({before}), took no record line since; nothing more is taken until coordd is restarted");
        return;
    }
    crate::jsonl::flush();
    let (marked, ours) = match crate::switch::mark_mine(sock) {
        Ok((p, ours)) => (format!("hooks wait for it ({})", p.display()), ours),
        Err(e) => (format!("no mark for hooks to wait on: {e}"), false),
    };
    eprintln!("coordd: {why}: coordd starts again in place (pid {}), as a coord-switch would; {marked}", std::process::id());
    let e = exec(start.and_then(|s| s.arena.clone()), why);
    // its own mark taken back by a rename, with no descriptor, else as at a start
    if !(ours && crate::switch::unmark_back(sock).is_ok()) {
        up(sock);
    }
    eprintln!("coordd: starting again in place failed: {e}; nothing more is taken until coordd is restarted");
}

/// This program run again in this process: the same file, by its path, so the process keeps its name (through
/// /proc/self/exe when that file is gone or another is in its place); its own arguments; its environment, with why
/// and the arena limit put in. Every descriptor goes at the exec, one opened without close-on-exec too: what ran out may
/// be them. Returns only failing.
fn exec(arena: Option<OsString>, why: &str) -> std::io::Error {
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::process::CommandExt;
    let me = std::path::PathBuf::from("/proc/self/exe");
    let same = |p: &std::path::Path| match (std::fs::metadata(p), std::fs::metadata(&me)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    };
    let prog = std::env::current_exe().ok().filter(|p| same(p)).unwrap_or_else(|| me.clone());
    // a test's exec that fails (tests below)
    #[cfg(test)]
    let prog = if tests::NO_PROGRAM.load(std::sync::atomic::Ordering::SeqCst) { std::path::PathBuf::from("/nonexistent/coordd") } else { prog };
    let mut args = std::env::args_os();
    let mut cmd = std::process::Command::new(prog);
    if let Some(a0) = args.next() {
        cmd.arg0(a0);
    }
    cmd.args(args).env(FOR, why);
    if let Some(a) = arena {
        cmd.env("MALLOC_ARENA_MAX", a);
    }
    // SAFETY: sets close-on-exec on this process's descriptors from 3 up; 0, 1 and 2 (the journal) stay
    if unsafe { libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, libc::CLOSE_RANGE_CLOEXEC) } != 0 {
        eprintln!("coordd: close_range: {}: a descriptor opened without close-on-exec stays across the exec", std::io::Error::last_os_error());
    }
    cmd.exec()
}

/// The children of the coordd before this one (a model's RPC, a view's source, a language server, a build's job): this
/// process's still, but nothing in it waits for them, so each is waited for on a thread of its own and none stays a
/// zombie once it ends. A coordd stopped instead leaves them to the service manager, which does the same. Its own
/// children are not started yet.
fn reap_inherited() {
    let me = std::process::id().to_string();
    let Ok(dir) = std::fs::read_dir("/proc") else { return };
    for pid in dir.flatten().filter_map(|e| e.file_name().to_str()?.parse::<i32>().ok()) {
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { continue };
        // its parent: field 4, the second after its name in brackets, which may hold spaces
        if stat.rsplit_once(')').and_then(|(_, r)| r.split_whitespace().nth(1)) == Some(me.as_str()) {
            let _ = std::thread::Builder::new().name("coordd-reap".into()).spawn(move || {
                // SAFETY: waits for a child of this process that nothing else in it waits for
                unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) };
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    /// The exec runs a program that is not there, so it fails (`exec`).
    pub(super) static NO_PROGRAM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    /// The child of the test below: its mark made ready, every descriptor taken, and a start in place whose exec fails.
    /// It writes whether the mark is up and what the one made ready names.
    fn failing_child(dir: &str) -> ! {
        unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) };
        super::at_start();
        let me = std::process::id().to_string();
        let sock = format!("{dir}/research.sock");
        super::up(&sock);
        NO_PROGRAM.store(true, std::sync::atomic::Ordering::SeqCst);
        let open = std::fs::read_dir("/proc/self/fd").map(|d| d.count()).unwrap() as u64;
        let mut lim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) }, 0);
        lim.rlim_cur = open + 32;
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &lim) }, 0);
        let mut held = Vec::new();
        while let Ok(f) = std::fs::File::open("/dev/null") {
            held.push(f);
            assert!(held.len() < 100, "no limit reached");
        }
        super::again(&sock, "a test's record: no descriptor left");
        drop(held);
        let mark = crate::switch::switch_mark(&sock);
        let ready = std::fs::read_to_string(format!("{}.coordd", mark.display())).unwrap_or_default().trim() == format!("{me} {}", crate::switch::started(&me).unwrap());
        std::fs::write(format!("{dir}/report"), format!("{me} up={} ready={ready}\n", mark.exists())).unwrap();
        std::process::exit(0)
    }

    /// A start in place whose exec fails with no descriptor left takes its mark back where it was made ready, by a
    /// rename: no hook goes on waiting on a coordd that lives on latched, and the next start in place has its mark
    /// (consult-3, N-1396 p405). In a child process with a low limit, as below.
    #[test]
    fn a_start_in_place_that_fails_with_no_descriptor_left_takes_its_mark_back() {
        if let Ok(dir) = std::env::var("COORDD_RESTART_FAIL_CHILD") {
            failing_child(&dir);
        }
        let dir = std::env::temp_dir().join(format!("coordd-restart-fail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let journal = std::fs::File::create(dir.join("journal")).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["restart::tests::a_start_in_place_that_fails_with_no_descriptor_left_takes_its_mark_back", "--exact", "--nocapture", "--test-threads=1"])
            .env("COORDD_RESTART_FAIL_CHILD", &dir)
            .stdout(std::process::Stdio::null())
            .stderr(journal)
            .spawn()
            .unwrap();
        let pid = child.id();
        let t = Instant::now();
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break Some(s);
            }
            if t.elapsed() > Duration::from_secs(60) {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let report = std::fs::read_to_string(dir.join("report")).unwrap_or_default();
        let journal = std::fs::read_to_string(dir.join("journal")).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        let shown = format!("report:\n{report}journal:\n{journal}");
        assert!(status.is_some_and(|s| s.success()), "the child: {status:?}\n{shown}");
        assert!(journal.contains("as a coord-switch would; hooks wait for it (") && journal.contains("starting again in place failed:"), "{shown}");
        assert_eq!(report.trim(), format!("{pid} up=false ready=true"), "{shown}");
    }

    const NAME: &str = "restart::tests::a_failed_record_starts_coordd_again_in_place_and_not_again_while_it_took_nothing";

    /// The child of the test below, this test binary again, one stage per program it runs, told by the lines in DIR's
    /// report so far; each makes its mark ready as main.rs does once its socket is bound. 0: a child of its own left to
    /// end unwaited, a writer thread with nothing queued, every descriptor taken up to its limit, and the record fails
    /// for want of one (EMFILE, as in N-1396). 1 (started in place): the mark named it, comes down as main.rs takes it
    /// down and the next is made ready, the child left is waited for, a record line goes in, and the record fails
    /// again (a folder where the log should be). 2 (started in place again): its record fails at once, and it stays.
    fn child(dir: &str) -> ! {
        // the parent gone (killed, or its runner stopped): the child goes too; kept across an exec
        unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) };
        super::at_start();
        let me = std::process::id();
        let report = format!("{dir}/report");
        let before = std::fs::read_to_string(&report).unwrap_or_default();
        let say = |line: String| {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&report).unwrap();
            writeln!(f, "{line}").unwrap();
        };
        let (bad, good, sock) = (format!("{dir}/bad/log.jsonl"), format!("{dir}/good/log.jsonl"), format!("{dir}/research.sock"));
        // record logs, not held open (core-slowlock's `HeldOpen` dropped): this test makes a record's line fail with
        // no descriptor left, as one not held does
        drop(crate::jsonl::essential(&bad));
        drop(crate::jsonl::essential(&good));
        let s = sock.clone();
        crate::jsonl::on_fault(move |why| super::again(&s, &why));
        let start = super::START.get().unwrap();
        let arena = start.arena.as_ref().and_then(|a| a.to_str()).unwrap_or("-").to_string();
        let after = start.after.clone().unwrap_or_default();
        // as main does
        unsafe { std::env::remove_var("MALLOC_ARENA_MAX") };
        let line = || vec!["{}".to_string()];
        match before.lines().count() {
            0 => {
                super::up(&sock);
                crate::jsonl::start_writer();
                let left = std::process::Command::new("true").spawn().unwrap().id();
                say(format!("{me} first arena={arena} left={left}"));
                // no descriptor left: the soft limit a little above those open, and all of them taken
                let open = std::fs::read_dir("/proc/self/fd").map(|d| d.count()).unwrap() as u64;
                let mut lim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
                assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) }, 0);
                lim.rlim_cur = open + 32;
                assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &lim) }, 0);
                let mut held = Vec::new();
                while let Ok(f) = std::fs::File::open("/dev/null") {
                    held.push(f);
                    assert!(held.len() < 100, "no limit reached");
                }
                let _ = crate::jsonl::record(&good, &line());
                drop(held);
                say(format!("{me} not started again"));
            }
            1 => {
                let mark = crate::switch::switch_mark(&sock);
                let named = std::fs::read_to_string(&mark).unwrap_or_default().trim() == format!("{me} {}", crate::switch::started(&me.to_string()).unwrap());
                super::up(&sock);
                let down = !mark.exists();
                let ready = std::fs::read_to_string(format!("{}.coordd", mark.display())).unwrap_or_default().trim() == format!("{me} {}", crate::switch::started(&me.to_string()).unwrap());
                let left = before.split("left=").nth(1).unwrap().trim().to_string();
                let t = Instant::now();
                while std::path::Path::new(&format!("/proc/{left}")).exists() && t.elapsed() < Duration::from_secs(10) {
                    std::thread::sleep(Duration::from_millis(20));
                }
                let reaped = !std::path::Path::new(&format!("/proc/{left}")).exists();
                let took = crate::jsonl::record(&good, &line()).is_ok() && crate::jsonl::took();
                say(format!("{me} again after=[{after}] arena={arena} named={named} down={down} ready={ready} reaped={reaped} took={took}"));
                let _ = crate::jsonl::record(&bad, &line());
                say(format!("{me} not started again"));
            }
            _ => {
                super::up(&sock);
                say(format!("{me} again after=[{after}] took={}", crate::jsonl::took()));
                let _ = crate::jsonl::record(&bad, &line());
                say(format!("{me} stayed"));
            }
        }
        std::process::exit(0)
    }

    /// A record log that cannot take a line, for want of a descriptor (no other is left to write the mark with:
    /// consult-3, N-1396 p395) or a folder where the file should be, starts coordd again in place: the same process,
    /// told why, its arena limit back, the mark up while it is down and taken down after, the child it left waited for. A start in place whose record took a line is cured, and a later failure starts it again; one that
    /// took none stays latched, and the journal says each. Neither side outlives a minute (paper-research N-27 post
    /// 29): a child that starts itself again without end is killed and the test fails.
    #[test]
    fn a_failed_record_starts_coordd_again_in_place_and_not_again_while_it_took_nothing() {
        if let Ok(dir) = std::env::var("COORDD_RESTART_CHILD") {
            child(&dir);
        }
        let dir = std::env::temp_dir().join(format!("coordd-restart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bad/log.jsonl")).unwrap();
        let journal = std::fs::File::create(dir.join("journal")).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([NAME, "--exact", "--nocapture", "--test-threads=1"])
            .env("COORDD_RESTART_CHILD", &dir)
            .env("MALLOC_ARENA_MAX", "3")
            .stdout(std::process::Stdio::null())
            .stderr(journal)
            .spawn()
            .unwrap();
        let pid = child.id();
        let t = Instant::now();
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break Some(s);
            }
            if t.elapsed() > Duration::from_secs(60) {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let report = std::fs::read_to_string(dir.join("report")).unwrap_or_default();
        let journal = std::fs::read_to_string(dir.join("journal")).unwrap_or_default();
        let bad = dir.join("bad/log.jsonl").display().to_string();
        let good = dir.join("good/log.jsonl").display().to_string();
        let _ = std::fs::remove_dir_all(&dir);
        let shown = format!("report:\n{report}journal:\n{journal}");
        assert!(status.is_some_and(|s| s.success()), "the child: {status:?}\n{shown}");
        let lines: Vec<&str> = report.lines().collect();
        assert_eq!(lines.len(), 4, "{shown}");
        assert!(lines.iter().all(|l| l.starts_with(&format!("{pid} "))), "one process throughout: {shown}");
        assert!(lines[0].starts_with(&format!("{pid} first arena=3 left=")), "{shown}");
        assert!(lines[1].starts_with(&format!("{pid} again after=[{good}: ")) && lines[1].contains("(os error 24)]"), "EMFILE: {shown}");
        assert!(lines[1].ends_with("] arena=3 named=true down=true ready=true reaped=true took=true"), "{shown}");
        assert!(lines[2].starts_with(&format!("{pid} again after=[{bad}: ")) && lines[2].ends_with("] took=false"), "{shown}");
        assert_eq!(lines[3], format!("{pid} stayed"), "{shown}");
        assert_eq!(journal.matches("coordd starts again in place").count(), 2, "{shown}");
        assert_eq!(journal.matches("as a coord-switch would; hooks wait for it (").count(), 2, "the mark up both times, with no descriptor left the first: {shown}");
        assert_eq!(journal.matches("started again in place (pid").count(), 2, "{shown}");
        assert_eq!(journal.matches("not started again: this coordd, started in place").count(), 1, "{shown}");
    }
}
