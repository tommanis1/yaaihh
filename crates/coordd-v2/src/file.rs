//! The record's file: one JSON line per record line, appended by one write each and read back by place.

use serde_json::Value as Json;
use std::fs::{File as OsFile, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::FileExt;
use std::path::Path;

/// The open file and where each of its lines ends.
pub struct File {
    f: OsFile,
    ends: Vec<u64>,
}

impl File {
    /// Open the file at `path`, made if missing, and read its lines. A last line with no newline is what a write cut
    /// short left behind; it was never a line and is cut off. A line that is not JSON is an error: the file is not a
    /// record.
    pub fn open(path: &Path) -> io::Result<(File, Vec<Json>)> {
        let mut f = OpenOptions::new().read(true).append(true).create(true).open(path)?;
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes)?;
        let whole = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        if whole < bytes.len() {
            f.set_len(whole as u64)?;
        }
        let (mut ends, mut lines, mut at) = (Vec::new(), Vec::new(), 0usize);
        for text in bytes[..whole].split_inclusive(|&b| b == b'\n') {
            at += text.len();
            let line = serde_json::from_slice(text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("line {}: {e}", lines.len())))?;
            lines.push(line);
            ends.push(at as u64);
        }
        Ok((File { f, ends }, lines))
    }

    /// How many lines the file holds: the place the next one takes.
    pub fn len(&self) -> u64 {
        self.ends.len() as u64
    }

    /// Append `line` in one write and wait for it to be on disk; its place. On a failed write the file is cut back
    /// to its last whole line, so it never holds part of one.
    pub fn append(&mut self, line: &Json) -> io::Result<u64> {
        let mut text = serde_json::to_vec(line)?;
        text.push(b'\n');
        let end = self.ends.last().copied().unwrap_or(0);
        if let Err(e) = self.f.write_all(&text).and_then(|()| self.f.sync_data()) {
            let _ = self.f.set_len(end);
            return Err(e);
        }
        self.ends.push(end + text.len() as u64);
        Ok(self.len() - 1)
    }

    /// The lines at places `from` up to `to`, `to` not included, both cut to what the file holds.
    pub fn read(&self, from: u64, to: u64) -> io::Result<Vec<Json>> {
        let to = to.min(self.len());
        if from >= to {
            return Ok(Vec::new());
        }
        let start = if from == 0 { 0 } else { self.ends[from as usize - 1] };
        let mut bytes = vec![0; (self.ends[to as usize - 1] - start) as usize];
        self.f.read_exact_at(&mut bytes, start)?;
        bytes.split_inclusive(|&b| b == b'\n').map(|t| serde_json::from_slice(t).map_err(io::Error::from)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lines_are_kept_in_order_read_by_place_and_a_cut_write_is_dropped() {
        let dir = std::env::temp_dir().join(format!("frame-file-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("record.jsonl");
        let (mut f, lines) = File::open(&path).unwrap();
        assert!(lines.is_empty() && f.len() == 0);
        for n in 0..5 {
            assert_eq!(f.append(&json!({"n": n})).unwrap(), n);
        }
        assert_eq!(f.read(1, 3).unwrap(), [json!({"n": 1}), json!({"n": 2})]);
        assert_eq!(f.read(3, 99).unwrap(), [json!({"n": 3}), json!({"n": 4})]);
        assert!(f.read(5, 9).unwrap().is_empty());
        drop(f);
        std::fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b"{\"n\": 5").unwrap();
        let (mut f, lines) = File::open(&path).unwrap();
        assert_eq!(lines.len(), 5, "the cut line is no line");
        assert_eq!(f.append(&json!({"n": 5})).unwrap(), 5);
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().last(), Some("{\"n\":5}"));
        std::fs::write(&path, "{\"n\":0}\nnot json\n").unwrap();
        assert!(File::open(&path).is_err(), "a file whose line is not JSON is not a record");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
