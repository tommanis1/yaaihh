//! What an agent holds on disk, for its view: its current transcript's size (one stat, nothing read) and its own
//! folder's (`agents/NAME`, a walk that follows no link). A task of its own reads them every [`EVERY`], off the lock
//! and apart from the usage timer (`main::count_footprints`). A stopped agent's folder is walked once after it stops
//! and then only when the folder itself changes (N-1933.p9: 96 agents walked every 30 s, most long stopped).

use std::collections::HashMap;
use std::path::Path;
use std::time::SystemTime;

/// How often the sizes are read again.
pub const EVERY: std::time::Duration = std::time::Duration::from_secs(30);

/// The most entries one folder's walk reads: past it the walk stops and the size is not known.
const MOST: usize = 200_000;

/// What was last read of each agent's folder: its change time then, whether it ran, and the size.
#[derive(Default)]
pub struct Kept(HashMap<String, (Option<SystemTime>, bool, Option<u64>)>);

impl Kept {
    /// The sizes of AGENTS (name, transcript, running) under COORD: each transcript stat'ed; a folder walked while its
    /// agent runs, the first time, the first time after it stopped, or when the folder itself changed, else kept.
    pub fn read(&mut self, coord: &Path, agents: Vec<(String, Option<String>, bool)>) -> Vec<(String, (Option<u64>, Option<u64>))> {
        self.0.retain(|n, _| agents.iter().any(|(a, ..)| a == n));
        agents
            .into_iter()
            .map(|(name, transcript, running)| {
                let dir = coord.join("agents").join(&name);
                let changed = std::fs::metadata(&dir).and_then(|m| m.modified()).ok();
                let folder = match self.0.get(&name) {
                    Some((at, ran, size)) if !running && !ran && *at == changed => *size,
                    _ => folder(&dir, MOST),
                };
                self.0.insert(name.clone(), (changed, running, folder));
                (name, (transcript.and_then(|t| std::fs::metadata(t).ok()).map(|m| m.len()), folder))
            })
            .collect()
    }
}

/// The bytes of the files under DIR, links neither followed nor counted; None when DIR cannot be read or holds more
/// than MOST entries. Walked from a list, not by recursion: an agent may nest folders as deep as it likes.
fn folder(dir: &Path, most: usize) -> Option<u64> {
    let (mut sum, mut seen, mut todo) = (0, 0, vec![std::fs::read_dir(dir).ok()?]);
    while let Some(entries) = todo.pop() {
        for e in entries.flatten() {
            seen += 1;
            if seen > most {
                return None;
            }
            let Ok(t) = e.file_type() else { continue };
            if t.is_dir() {
                todo.extend(std::fs::read_dir(e.path()).ok());
            } else if t.is_file() {
                sum += e.metadata().map_or(0, |m| m.len());
            }
        }
    }
    Some(sum)
}

#[cfg(test)]
mod tests {
    use super::Kept;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("footprint-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// The folder counts its files and its subfolders' files, never what a link points to; the transcript is its size;
    /// a walk past its most entries gives no size.
    #[test]
    fn an_agents_footprint_counts_its_own_files_and_no_link() {
        let d = scratch("links");
        let (home, outside) = (d.join("agents/a"), d.join("big"));
        std::fs::create_dir_all(home.join("home/project/agents")).unwrap();
        std::fs::write(home.join("session.json"), [0u8; 10]).unwrap();
        std::fs::write(home.join("home/notes"), [0u8; 5]).unwrap();
        std::fs::write(&outside, vec![0u8; 4096]).unwrap();
        std::os::unix::fs::symlink(&outside, home.join("home/project/agents/b")).unwrap();
        std::os::unix::fs::symlink(&d, home.join("home/project/up")).unwrap();
        let t = d.join("t.jsonl");
        std::fs::write(&t, [0u8; 7]).unwrap();
        let read = Kept::default().read(&d, vec![("a".into(), t.to_str().map(String::from), true), ("none".into(), None, true)]);
        assert_eq!(read, [("a".to_string(), (Some(7), Some(15))), ("none".to_string(), (None, None))]);
        assert_eq!(super::folder(&home, 3), None, "past its most entries");
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// N-1933.p9: a stopped agent's folder is walked once after it stops, then kept until the folder itself changes;
    /// a running agent's is walked each time.
    #[test]
    fn a_stopped_agents_folder_is_kept_until_it_changes() {
        let d = scratch("kept");
        let home = d.join("agents/a");
        std::fs::create_dir_all(home.join("home/sub")).unwrap();
        std::fs::write(home.join("home/sub/x"), [0u8; 10]).unwrap();
        let mut kept = Kept::default();
        let size = |kept: &mut Kept, running: bool| kept.read(&d, vec![("a".into(), None, running)])[0].1.1;
        assert_eq!(size(&mut kept, true), Some(10));
        std::fs::write(home.join("home/sub/y"), [0u8; 5]).unwrap();
        assert_eq!(size(&mut kept, true), Some(15), "running: walked");
        std::fs::write(home.join("home/sub/z"), [0u8; 1]).unwrap();
        assert_eq!(size(&mut kept, false), Some(16), "the first read after it stopped: walked");
        std::fs::write(home.join("home/sub/w"), [0u8; 100]).unwrap();
        assert_eq!(size(&mut kept, false), Some(16), "stopped, its folder itself unchanged: kept, not walked");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(home.join("top"), [0u8; 4]).unwrap();
        assert_eq!(size(&mut kept, false), Some(120), "its folder changed: walked again");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
