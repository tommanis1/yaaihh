//! A record kept in extra places: cut into parts at line ends, each sealed before it leaves, one writer at a time.

pub mod chunk;
pub mod git;
pub mod handover;
pub mod local;
pub mod place;
pub mod seal;
pub mod sync;

pub use chunk::{PART_SIZE, cut};
pub use git::GitRepoPlace;
pub use local::LocalDirPlace;
pub use handover::Handover;
pub use place::{Fetched, Head, MOST_PART, Part, PlaceError, RecordPlace, Text};
pub use seal::{Key, Kind, PlaceKey, RecordId, Sealer};
pub use sync::{Restored, Sender, catch_up, restore, take_over};

#[cfg(test)]
pub(crate) mod fixture {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A record of LINES JSON lines, about 300 bytes each.
    pub fn record(lines: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for i in 0..lines {
            let text = "x".repeat(200 + i % 97);
            out.extend_from_slice(format!("{{\"at\":\"2026-11-26T00:00:00Z\",\"line\":{i},\"text\":\"{text}\"}}\n").as_bytes());
        }
        out
    }

    /// A fresh empty folder of this test's own.
    pub fn scratch(name: &str) -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let d = std::env::temp_dir().join(format!("record-place-{}-{}-{name}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// FROM's files copied to TO, folders and all.
    pub fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() { copy_dir(&e.path(), &to.join(e.file_name())) } else { std::fs::copy(e.path(), to.join(e.file_name())).map(|_| ()).unwrap() }
        }
    }

    /// A place's key for the tests.
    pub fn key() -> crate::PlaceKey {
        crate::Key::from_bytes([7; 32]).place("test")
    }
}
