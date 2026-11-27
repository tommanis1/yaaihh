//! A place that is a folder on this machine's own disk: for tests and one host. For several devices, use a git place.

// The one-writer rule here is flock(2) on the folder's lock file, which holds between processes of one host only.
// On NFS, SMB, rclone or another FUSE mount it may lock nothing across hosts, so two hosts writing one folder can
// both pass it: keep such a folder to one writing host.

use crate::place::{Fetched, Head, MOST_PART, Part, PlaceError, RecordPlace, Text, check_order, read_most, text_name};
use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

/// A folder holding `parts/NNNNNNNNNNNN.part`, `texts/NAME`, `open.part` and `head` (its count and token), written behind a lock.
#[derive(Debug, Clone)]
pub struct LocalDirPlace {
    dir: PathBuf,
}

fn io(what: &Path, e: std::io::Error) -> PlaceError {
    PlaceError::Io(format!("{}: {e}", what.display()))
}

/// The folder's lock, held while it lives.
struct Locked(File);

impl Drop for Locked {
    fn drop(&mut self) {
        // SAFETY: the descriptor is the open lock file's
        unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
    }
}

impl LocalDirPlace {
    /// The place in DIR, made when it is not there.
    pub fn open(dir: impl Into<PathBuf>) -> Result<LocalDirPlace, PlaceError> {
        let dir = dir.into();
        std::fs::create_dir_all(dir.join("parts")).and_then(|()| std::fs::create_dir_all(dir.join("texts"))).map_err(|e| io(&dir, e))?;
        Ok(LocalDirPlace { dir })
    }

    fn lock(&self, how: libc::c_int) -> Result<Locked, PlaceError> {
        let p = self.dir.join("lock");
        let f = File::options().create(true).append(true).open(&p).map_err(|e| io(&p, e))?;
        // SAFETY: the descriptor is the open lock file's
        if unsafe { libc::flock(f.as_raw_fd(), how) } != 0 {
            return Err(io(&p, std::io::Error::last_os_error()));
        }
        Ok(Locked(f))
    }

    fn part_file(&self, n: u64) -> PathBuf {
        self.dir.join("parts").join(format!("{n:012}.part"))
    }

    /// BYTES written to FILE whole or not at all, and its name on disk too.
    fn put(&self, file: &Path, bytes: &[u8]) -> Result<(), PlaceError> {
        let tmp = file.with_extension("tmp");
        std::fs::write(&tmp, bytes).map_err(|e| io(&tmp, e))?;
        File::open(&tmp).and_then(|f| f.sync_all()).map_err(|e| io(&tmp, e))?;
        std::fs::rename(&tmp, file).map_err(|e| io(file, e))?;
        let dir = file.parent().unwrap_or(Path::new("."));
        File::open(dir).and_then(|d| d.sync_all()).map_err(|e| io(dir, e))
    }

    fn read_head(&self) -> Result<Head, PlaceError> {
        let p = self.dir.join("head");
        let text = match read_most(&p, 64) {
            Ok(t) => String::from_utf8_lossy(&t).to_string(),
            Err(_) if !p.exists() => return Ok(Head::default()),
            Err(e) => return Err(e),
        };
        let bad = || PlaceError::Io(format!("{}: not a head: {text:?}", p.display()));
        let (closed, rev) = text.trim().split_once(' ').ok_or_else(bad)?;
        Ok(Head { closed: closed.parse().map_err(|_| bad())?, rev: rev.to_string() })
    }
}

impl RecordPlace for LocalDirPlace {
    fn head(&self) -> Result<Head, PlaceError> {
        let _l = self.lock(libc::LOCK_SH)?;
        self.read_head()
    }

    fn take_parts(&mut self, base: &Head, closed: &[Part], open: &Part, texts: &[Text]) -> Result<Head, PlaceError> {
        let _l = self.lock(libc::LOCK_EX)?;
        let now = self.read_head()?;
        if &now != base {
            return Err(PlaceError::Stale { base: base.clone(), now });
        }
        let count = check_order(base, closed, open)?;
        for t in texts {
            text_name(&t.name)?;
            self.put(&self.dir.join("texts").join(&t.name), &t.sealed)?;
        }
        for p in closed {
            self.put(&self.part_file(p.n), &p.sealed)?;
        }
        self.put(&self.dir.join("open.part"), &open.sealed)?;
        let head = Head { closed: count, rev: (base.rev.parse::<u64>().unwrap_or(0) + 1).to_string() };
        self.put(&self.dir.join("head"), format!("{} {}\n", head.closed, head.rev).as_bytes())?;
        Ok(head)
    }

    fn fetch_texts(&self, _: &Head) -> Result<Vec<Text>, PlaceError> {
        let _l = self.lock(libc::LOCK_SH)?;
        let d = self.dir.join("texts");
        let mut out = Vec::new();
        for e in std::fs::read_dir(&d).map_err(|e| io(&d, e))? {
            let p = e.map_err(|e| io(&d, e))?.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            if text_name(&name).is_ok() {
                out.push(Text { sealed: read_most(&p, MOST_PART)?, name });
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn fetch_from(&self, from: u64) -> Result<Fetched, PlaceError> {
        let _l = self.lock(libc::LOCK_SH)?;
        let head = self.read_head()?;
        let read = |p: PathBuf| read_most(&p, MOST_PART);
        let closed = (from..head.closed).map(|n| Ok(Part { n, sealed: read(self.part_file(n))? })).collect::<Result<Vec<_>, PlaceError>>()?;
        let open = if head.rev.is_empty() { None } else { Some(Part { n: head.closed, sealed: read(self.dir.join("open.part"))? }) };
        Ok(Fetched { head, closed, open })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::scratch;

    fn part(n: u64, b: &[u8]) -> Part {
        Part { n, sealed: b.to_vec() }
    }

    #[test]
    fn parts_taken_are_fetched_back_in_order() {
        let mut p = LocalDirPlace::open(scratch("local")).unwrap();
        assert_eq!(p.head().unwrap(), Head::default());
        assert!(p.fetch_from(0).unwrap().open.is_none());
        let h1 = p.take_parts(&Head::default(), &[part(0, b"a"), part(1, b"b")], &part(2, b"o1"), &[]).unwrap();
        let h2 = p.take_parts(&h1, &[part(2, b"c")], &part(3, b"o2"), &[]).unwrap();
        assert_eq!(h2.closed, 3);
        let f = p.fetch_from(1).unwrap();
        assert_eq!(f.head, h2);
        assert_eq!(f.closed, vec![part(1, b"b"), part(2, b"c")]);
        assert_eq!(f.open, Some(part(3, b"o2")));
    }

    #[test]
    fn a_writer_behind_the_head_is_refused_and_nothing_changes() {
        let d = scratch("stale");
        let mut a = LocalDirPlace::open(&d).unwrap();
        let mut b = LocalDirPlace::open(&d).unwrap();
        let base = a.head().unwrap();
        let h = a.take_parts(&base, &[part(0, b"a")], &part(1, b"oa"), &[]).unwrap();
        let refused = b.take_parts(&base, &[part(0, b"b")], &part(1, b"ob"), &[]);
        assert_eq!(refused, Err(PlaceError::Stale { base: base.clone(), now: h.clone() }));
        assert!(matches!(b.refuse_if_stale(&base), Err(PlaceError::Stale { .. })));
        assert_eq!(b.refuse_if_stale(&h), Ok(()));
        assert_eq!(a.fetch_from(0).unwrap().closed, vec![part(0, b"a")]);
    }

    #[test]
    fn texts_go_with_the_parts_and_come_back() {
        let mut p = LocalDirPlace::open(scratch("texts")).unwrap();
        let t = Text { name: "ab".into(), sealed: b"s".to_vec() };
        let h = p.take_parts(&Head::default(), &[], &part(0, b"o"), std::slice::from_ref(&t)).unwrap();
        assert_eq!(p.fetch_texts(&h).unwrap(), [t]);
        let bad = Text { name: "../x".into(), sealed: vec![] };
        assert!(p.take_parts(&h, &[], &part(0, b"o"), &[bad]).is_err());
    }

    #[test]
    fn a_part_larger_than_a_place_keeps_is_refused_unread() {
        let d = scratch("big");
        let mut p = LocalDirPlace::open(&d).unwrap();
        p.take_parts(&Head::default(), &[part(0, b"a")], &part(1, b"o"), &[]).unwrap();
        // 2 GiB that take no disk: the size alone refuses it
        std::fs::File::options().write(true).open(d.join("parts").join(format!("{:012}.part", 0))).unwrap().set_len(2 << 30).unwrap();
        let e = p.fetch_from(0).unwrap_err().to_string();
        assert!(e.contains("more than the"), "{e}");
    }

    #[test]
    fn parts_out_of_order_are_refused() {
        let mut p = LocalDirPlace::open(scratch("order")).unwrap();
        assert!(matches!(p.take_parts(&Head::default(), &[part(1, b"a")], &part(2, b"o"), &[]), Err(PlaceError::Order(_))));
        assert_eq!(p.head().unwrap(), Head::default());
    }
}
