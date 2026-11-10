//! coordd's append-only logs (`mail/log.jsonl`, `nets/log.jsonl`, `decisions/log.jsonl`): one JSON value per line.
//! A line goes in with one write, and a write that fails part way (the disk full: 8 Nov 21:48, a run's record stopped
//! mid-line and the next one went on the same line, under the same id) is cut back off, so the file only ever holds
//! whole lines.

use std::fs::OpenOptions;
use std::io::Write;

/// Append LINE (one JSON value, no newline) to PATH, creating its folder; the file is left as it was if it fails.
pub fn append(path: &str, line: &str) -> std::io::Result<()> {
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    let len = f.metadata()?.len();
    let mut buf = Vec::with_capacity(line.len() + 1);
    buf.extend_from_slice(line.as_bytes());
    buf.push(b'\n');
    f.write_all(&buf).inspect_err(|_| {
        let _ = f.set_len(len);
    })
}

/// What the writer thread does, in the order given ([`start_writer`]).
enum Job {
    Append(String, String),
    Replace(String, Vec<u8>),
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
                        if let Err(e) = append(&path, &line) {
                            eprintln!("coordd: {path}: {e}");
                        }
                    }
                    Job::Replace(path, bytes) => {
                        if let Err(e) = replace(&path, &bytes) {
                            eprintln!("coordd: {path}: {e}");
                        }
                    }
                }
            }
        })
        .expect("the writer thread");
}

/// Append LINE to PATH on the writer thread (at once, before it runs: the load and the tests).
pub fn later(path: &str, line: String) {
    match WRITER.get() {
        Some(tx) => {
            let _ = tx.send(Job::Append(path.into(), line));
        }
        None => {
            if let Err(e) = append(path, &line) {
                eprintln!("coordd: {path}: {e}");
            }
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

/// The number of whole lines in PATH (0 when it does not exist): the next record's number is one more.
pub fn lines(path: &str) -> usize {
    std::fs::read(path).map(|b| b.iter().filter(|c| **c == b'\n').count()).unwrap_or(0)
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
        assert_eq!(super::lines(&p), 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
