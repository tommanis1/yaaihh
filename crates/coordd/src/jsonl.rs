//! coordd's append-only logs (`mail/log.jsonl`, `tickets/log.jsonl`): one JSON value per line.
//! A line goes in with one write, and a write that fails part way (the disk full: 8 Nov 21:48, a run's record stopped
//! mid-line and the next one went on the same line, under the same id) is cut back off, so the file only ever holds
//! whole lines.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::sync::Arc;

/// Append LINE (one JSON value, no newline) to PATH, creating its folder; the file is left as it was if it fails. A
/// record log held open ([`essential`]) takes it through the descriptor it holds, and asks for none.
pub fn append(path: &str, line: &str) -> std::io::Result<()> {
    let mut buf = Vec::with_capacity(line.len() + 1);
    buf.extend_from_slice(line.as_bytes());
    buf.push(b'\n');
    let held = HELD.lock().unwrap().iter().rev().find(|(_, p, _)| p == path).map(|(id, _, f)| (*id, f.clone()));
    match held {
        Some((_, Some(f))) if same_file(path, &f) => write_whole(&f, &buf),
        // held, but not open yet (no file at the start) or no longer the file at PATH: the one opened now is held
        Some((id, _)) => {
            let f = Arc::new(open(path)?);
            if let Some(e) = HELD.lock().unwrap().iter_mut().find(|(i, ..)| *i == id) {
                e.2 = Some(f.clone());
            }
            write_whole(&f, &buf)
        }
        None => write_whole(&open(path)?, &buf),
    }
}

/// PATH opened to append to, its folder made first.
fn open(path: &str) -> std::io::Result<File> {
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir)?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

/// BUF onto the end of F in one write, or F as it was.
fn write_whole(mut f: &File, buf: &[u8]) -> std::io::Result<()> {
    let len = f.metadata()?.len();
    f.write_all(buf).inspect_err(|_| {
        let _ = f.set_len(len);
    })
}

/// Whether PATH still names the file F (`stat`, which takes no descriptor): not once it was moved away or removed.
fn same_file(path: &str, f: &File) -> bool {
    match (std::fs::metadata(path), f.metadata()) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

/// The record logs held open ([`essential`]), by the id of the [`HeldOpen`] that keeps each: its path, and its file
/// once it is open.
static HELD: std::sync::Mutex<Vec<(u64, String, Option<Arc<File>>)>> = std::sync::Mutex::new(Vec::new());

/// A record log held open from coordd's start ([`essential`]) until this is dropped, with the Core that keeps it.
#[must_use = "the log is held open only while this is kept"]
pub struct HeldOpen(u64);

impl Drop for HeldOpen {
    fn drop(&mut self) {
        HELD.lock().unwrap().retain(|(id, ..)| *id != self.0);
    }
}

/// Whether PATH is held open now (a test's).
#[cfg(test)]
fn held_open(path: &str) -> bool {
    HELD.lock().unwrap().iter().any(|(_, p, f)| p == path && f.is_some())
}

/// The logs that are coordd's record (the ticket log, the mail log): after a line to one of them failed to go in, no
/// later line goes in either, so the log stays a clean prefix of what happened and the next start replays it as it
/// was up to there (paper-research N-8, docket-core's answer 5). What failed, "PATH: ERROR", per log.
static ESSENTIAL: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
static FAULTS: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());
/// Told "PATH: ERROR" once, when a record's log first fails (coordd starts again in place, or raises its notice:
/// main.rs, restart.rs).
static ON_FAULT: std::sync::OnceLock<Box<dyn Fn(String) + Send + Sync>> = std::sync::OnceLock::new();
/// A record log took a line since this process started (restart.rs: a start in place that took none before its record
/// failed again is not done again).
static TOOK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// PATH is one of coordd's record logs ([`ESSENTIAL`]), held open while the answer is kept: opened now, or by its first
/// line when there is no file yet, and from then on its lines go in through that descriptor, never one of their own. So
/// a coordd with no descriptor left still takes its record's lines, and only what asks for a new one is turned away:
/// at 22:48 20 Nov the log's open for each line was what failed, and the record took nothing more until a restart
/// (N-1396; consult-3 p382, 3 and p384, 3). A write that fails (the disk full) still stops the log, as before.
pub fn essential(path: &str) -> HeldOpen {
    {
        let mut e = ESSENTIAL.lock().unwrap();
        if !e.iter().any(|p| p == path) {
            e.push(path.to_string());
        }
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    // no file yet (a new coord): its first line opens it (`append`); the start makes none
    let f = match OpenOptions::new().append(true).open(path) {
        Ok(f) => Some(Arc::new(f)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            eprintln!("coordd: {path}: {e}; not held open: its next line opens it");
            None
        }
    };
    HELD.lock().unwrap().push((id, path.to_string(), f));
    HeldOpen(id)
}

/// What failed on PATH, a record log a line could not be appended to: "PATH: ERROR". None while it takes lines.
pub fn fault(path: &str) -> Option<String> {
    FAULTS.lock().unwrap().iter().find(|(p, _)| p == path).map(|(_, why)| why.clone())
}

/// Forget PATH's failure, as a restart does (a test's restart, in the same process).
#[cfg(test)]
pub fn forget(path: &str) {
    FAULTS.lock().unwrap().retain(|(p, _)| p != path);
}

/// Call F with "PATH: ERROR" when a record log first fails.
pub fn on_fault(f: impl Fn(String) + Send + Sync + 'static) {
    let _ = ON_FAULT.set(Box::new(f));
}

/// Whether a record log took a line since this process started.
pub fn took() -> bool {
    TOOK.load(std::sync::atomic::Ordering::SeqCst)
}

/// Append LINE to PATH, unless PATH is a record log that failed before; a record log's first failure is kept. Err:
/// "PATH: ERROR", the line not in.
fn write_line(path: &str, line: &str) -> Result<(), String> {
    let record = ESSENTIAL.lock().unwrap().iter().any(|p| p == path);
    if record {
        if let Some(why) = fault(path) {
            return Err(why);
        }
    }
    append(path, line).inspect(|()| {
        if record {
            TOOK.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }).map_err(|e| {
        eprintln!("coordd: {path}: {e}");
        let why = format!("{path}: {e}");
        if record {
            FAULTS.lock().unwrap().push((path.to_string(), why.clone()));
            eprintln!("coordd: {path} takes no more lines until coordd is restarted");
            if let Some(f) = ON_FAULT.get() {
                f(why.clone());
            }
        }
        why
    })
}

/// Append LINES (one JSON value each) to PATH now, in one write, and say whether they went in: the ticket log's
/// lines are written before the input they record is answered or its effects go out, as a write-ahead log's are
/// (paper-research N-8 post 16). A failed write leaves the file as it was. Nothing for PATH may be queued on the
/// writer thread ([`later`]).
pub fn record(path: &str, lines: &[String]) -> Result<(), String> {
    if lines.is_empty() {
        return match fault(path) {
            Some(why) => Err(why),
            None => Ok(()),
        };
    }
    write_line(path, &lines.join("\n"))
}

/// What the writer thread does, in the order given ([`start_writer`]).
enum Job {
    Append(String, String),
    Replace(String, Vec<u8>),
    /// Everything handed before it written: told on its sender ([`flush`]).
    Flush(std::sync::mpsc::Sender<()>),
}

static WRITER: std::sync::OnceLock<std::sync::mpsc::Sender<Job>> = std::sync::OnceLock::new();

/// Start the writer thread: from then on [`later`] and [`replace_later`] hand their writes to it, in order, so the
/// core's lock is never held over a disk write (design coordd-core-2026-11-10 §2).
pub fn start_writer() {
    let (tx, rx) = std::sync::mpsc::channel::<Job>();
    if WRITER.set(tx).is_err() {
        return;
    }
    std::thread::Builder::new()
        .name("coordd-writer".into())
        .spawn(move || {
            for job in rx {
                match job {
                    Job::Append(path, line) => {
                        let _ = write_line(&path, &line);
                    }
                    Job::Replace(path, bytes) => {
                        if let Err(e) = replace(&path, &bytes) {
                            eprintln!("coordd: {path}: {e}");
                        }
                    }
                    Job::Flush(done) => {
                        let _ = done.send(());
                    }
                }
            }
        })
        .expect("the writer thread");
}

/// Wait until the writer thread has written all it was handed (coordd starting again in place: restart.rs). At once
/// when there is none, or on the writer thread itself.
pub fn flush() {
    let Some(tx) = WRITER.get() else { return };
    if std::thread::current().name() == Some("coordd-writer") {
        return;
    }
    let (done, wait) = std::sync::mpsc::channel();
    if tx.send(Job::Flush(done)).is_ok() {
        let _ = wait.recv();
    }
}

/// Append LINE to PATH on the writer thread (at once, before it runs: the load and the tests).
pub fn later(path: &str, line: String) {
    match WRITER.get() {
        Some(tx) => {
            let _ = tx.send(Job::Append(path.into(), line));
        }
        None => {
            let _ = write_line(path, &line);
        }
    }
}

/// Replace PATH's content with BYTES (a temporary file renamed over it) on the writer thread.
pub fn replace_later(path: &str, bytes: Vec<u8>) {
    match WRITER.get() {
        Some(tx) => {
            let _ = tx.send(Job::Replace(path.into(), bytes));
        }
        None => {
            if let Err(e) = replace(path, &bytes) {
                eprintln!("coordd: {path}: {e}");
            }
        }
    }
}

fn replace(path: &str, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn appends_whole_lines() {
        let dir = std::env::temp_dir().join(format!("jsonl-{}", std::process::id()));
        let p = dir.join("log.jsonl").to_string_lossy().to_string();
        super::append(&p, r#"{"a":1}"#).unwrap();
        super::append(&p, r#"{"a":2}"#).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{\"a\":1}\n{\"a\":2}\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    const NO_DESCRIPTOR_LEFT: &str = "jsonl::tests::a_record_log_held_open_takes_lines_with_no_descriptor_left";

    /// A record log held open takes its lines while coordd has no descriptor left (N-1396, 22:48 20 Nov), where a line
    /// to a log that opens its file is refused. Run in a child of this test binary, whose descriptors it fills up to a
    /// lowered limit: lowering this process's would starve the tests beside it.
    #[test]
    fn a_record_log_held_open_takes_lines_with_no_descriptor_left() {
        if let Ok(dir) = std::env::var("JSONL_NO_DESCRIPTOR_LEFT") {
            return no_descriptor_left(&dir);
        }
        let dir = std::env::temp_dir().join(format!("jsonl-fds-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([NO_DESCRIPTOR_LEFT, "--exact", "--nocapture", "--test-threads=1"])
            .env("JSONL_NO_DESCRIPTOR_LEFT", &dir)
            .output()
            .unwrap();
        let said = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success() && said.contains("1 passed"), "the child: {said}");
        assert!(said.contains("no descriptor left: EMFILE"), "the child filled its descriptors: {said}");
        assert_eq!(std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap(), "{\"n\":1}\n{\"n\":2}\n", "both lines in, the second with no descriptor left");
        assert!(!dir.join("mail/log.jsonl").exists(), "the line whose log opens its file did not go in");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The child's part: the ticket log held open (no file yet, so its first line opens it), its descriptors filled to a
    /// limit lowered to what it holds and 16 more; then a line to a log not held is refused for want of a descriptor, and
    /// the ticket log's goes in.
    fn no_descriptor_left(dir: &str) {
        let p = format!("{dir}/tickets/log.jsonl");
        let other = format!("{dir}/mail/log.jsonl");
        let _held = super::essential(&p);
        super::record(&p, &[r#"{"n":1}"#.to_string()]).unwrap();
        std::fs::create_dir_all(format!("{dir}/mail")).unwrap();
        let open = std::fs::read_dir("/proc/self/fd").unwrap().count() as u64;
        let mut lim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) }, 0);
        lim.rlim_cur = lim.rlim_cur.min(open + 16);
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &lim) }, 0);
        let mut filled = Vec::new();
        let err = loop {
            match std::fs::File::open("/dev/null") {
                Ok(f) => filled.push(f),
                Err(e) => break e,
            }
        };
        assert_eq!(err.raw_os_error(), Some(libc::EMFILE), "{err}");
        println!("no descriptor left: EMFILE after {} more", filled.len());
        let refused = super::append(&other, r#"{"m":1}"#).expect_err("a log not held needs a descriptor");
        assert_eq!(refused.raw_os_error(), Some(libc::EMFILE), "{refused}");
        super::record(&p, &[r#"{"n":2}"#.to_string()]).expect("the ticket log held open takes its line");
        assert_eq!(super::fault(&p), None);
    }

    /// A record log held open is written at its path: moved away, the file now at its path is opened and held, and the
    /// moved one takes nothing more. Once its holder is dropped (a test's Core), it is open no more, and a line opens it.
    #[test]
    fn a_record_log_held_open_follows_its_path_and_closes_with_its_holder() {
        let dir = std::env::temp_dir().join(format!("jsonl-held-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("log.jsonl").to_string_lossy().to_string();
        let read = |f: &str| std::fs::read_to_string(f).unwrap();
        let line = |n: u32| vec![format!(r#"{{"n":{n}}}"#)];
        std::fs::write(&p, "{\"n\":0}\n").unwrap();
        let held = super::essential(&p);
        assert!(super::held_open(&p), "a log that is there is opened at once");
        super::record(&p, &line(1)).unwrap();
        let moved = dir.join("moved.jsonl").to_string_lossy().to_string();
        std::fs::rename(&p, &moved).unwrap();
        super::record(&p, &line(2)).unwrap();
        super::record(&p, &line(3)).unwrap();
        assert_eq!(read(&moved), "{\"n\":0}\n{\"n\":1}\n", "the moved file takes nothing more");
        assert_eq!(read(&p), "{\"n\":2}\n{\"n\":3}\n", "the file at its path, held from its first line");
        drop(held);
        assert!(!super::held_open(&p), "closed with its holder");
        super::record(&p, &line(4)).unwrap();
        assert_eq!(read(&p), "{\"n\":2}\n{\"n\":3}\n{\"n\":4}\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
