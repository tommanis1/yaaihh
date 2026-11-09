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
