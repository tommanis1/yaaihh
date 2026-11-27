//! The devices on Home: the sampler's one file, `~/agent-coord/machines/machines.json`, by its fixed name only, its text
//! passed on as it is with how old it is. Neither a linked folder nor a linked file is followed; at most [`FILE_AT_MOST`]
//! bytes are read. An old or missing file starts one run of the sampler beside the answer, so nothing samples unasked.

use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::SystemTime;

/// How old the file may be before an ask starts a fresh sample, and how long it speaks for the build machine.
pub const FRESH_SECS: u64 = 30;

/// The most bytes read of the file.
const FILE_AT_MOST: u64 = 2 << 20;

/// The folders whose sampler runs now: a second ask while one lives starts none.
static RUNNING: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

/// `~/agent-coord/machines`, the folder of the file; none when HOME is not an absolute path (never a folder under
/// coordd's own working directory).
pub fn folder() -> Result<PathBuf, String> {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    if !home.is_absolute() {
        return Err("no home to read the machines from".into());
    }
    Ok(home.join("agent-coord").join("machines"))
}

/// The page's answer from FOLDER at NOW, as JSON: `{"text", "age_secs"}` when `machines.json` is there as a plain file;
/// `{"text": null, "why"}` when it is not (not written yet, a link, too big, not UTF-8). The text is not parsed here.
pub fn read(folder: &Path, now: SystemTime) -> Value {
    match text(folder) {
        Ok((text, written)) => {
            let age = written.and_then(|w| now.duration_since(w).ok()).map(|d| d.as_secs());
            json!({"text": text, "age_secs": age})
        }
        Err(why) => json!({"text": null, "why": why}),
    }
}

/// A page asked at NOW: when FOLDER's file is missing or [`FRESH_SECS`] old and no run for FOLDER lives, SAMPLER (an
/// absolute path to a plain file) starts once as `python3 -I SAMPLER`, writing `MACHINES_OUT` = FOLDER's file, and a
/// thread waits on it until it ends. True when a run started. The answer never waits for it.
pub fn refresh(folder: &Path, sampler: &Path, now: SystemTime) -> bool {
    if !sampler.is_absolute() || !sampler.is_file() {
        return false;
    }
    let mut c = Command::new("python3");
    c.arg("-I").arg(sampler);
    start(folder, now, c)
}

/// [`refresh`]'s run, given as a command (a stand-in in the tests).
fn start(folder: &Path, now: SystemTime, mut run: Command) -> bool {
    if !old(folder, now) {
        return false;
    }
    if !RUNNING.lock().unwrap_or_else(|e| e.into_inner()).insert(folder.to_path_buf()) {
        return false;
    }
    run.env("MACHINES_OUT", folder.join("machines.json")).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let ended = folder.to_path_buf();
    match run.spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
                RUNNING.lock().unwrap_or_else(|e| e.into_inner()).remove(&ended);
            });
            true
        }
        Err(_) => {
            RUNNING.lock().unwrap_or_else(|e| e.into_inner()).remove(&ended);
            false
        }
    }
}

/// FOLDER's file wants a fresh sample at NOW: missing, or a plain file written [`FRESH_SECS`] or more ago. A link, or
/// something other than a folder or a plain file by those names, starts nothing: no run writes through it.
fn old(folder: &Path, now: SystemTime) -> bool {
    match std::fs::symlink_metadata(folder) {
        Ok(m) if m.file_type().is_dir() => {}
        Ok(_) => return false,
        Err(_) => return true,
    }
    match std::fs::symlink_metadata(folder.join("machines.json")) {
        Ok(m) if m.file_type().is_file() => match m.modified() {
            Ok(w) => now.duration_since(w).is_ok_and(|d| d.as_secs() >= FRESH_SECS),
            Err(_) => true,
        },
        Ok(_) => false,
        Err(_) => true,
    }
}

fn text(folder: &Path) -> Result<(String, Option<SystemTime>), String> {
    match std::fs::symlink_metadata(folder) {
        Ok(m) if m.file_type().is_dir() => {}
        Ok(_) => return Err("the machines folder is not a folder (a link is not followed)".into()),
        Err(_) => return Err("no numbers yet".into()),
    }
    // opened with O_NOFOLLOW, then its own metadata read: a link put there between a look and the open is refused
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(folder.join("machines.json"))
        .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { "no numbers yet".to_string() } else { "the machines file is not a plain file (a link is not followed)".to_string() })?;
    let m = f.metadata().map_err(|e| e.to_string())?;
    if !m.file_type().is_file() {
        return Err("the machines file is not a plain file".into());
    }
    if m.len() > FILE_AT_MOST {
        return Err(format!("the machines file is larger than {} MB", FILE_AT_MOST >> 20));
    }
    let mut bytes = Vec::new();
    f.by_ref().take(FILE_AT_MOST + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > FILE_AT_MOST {
        return Err(format!("the machines file is larger than {} MB", FILE_AT_MOST >> 20));
    }
    let text = String::from_utf8(bytes).map_err(|_| "the machines file is not text".to_string())?;
    Ok((text, m.modified().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("machine-state-machines-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The file's text goes as it is, with its age; nothing else of the folder is read.
    #[test]
    fn the_file_goes_as_it_is_with_its_age() {
        let d = scratch("plain");
        let text = r#"{"read": "2026-11-19T21:50:05+01:00", "machines": [{"name": "This PC", "on": true}], "new_field": 1}"#;
        std::fs::write(d.join("machines.json"), text).unwrap();
        std::fs::write(d.join("other.json"), "not read").unwrap();
        let written = std::fs::metadata(d.join("machines.json")).unwrap().modified().unwrap();
        let v = read(&d, written + Duration::from_secs(7));
        assert_eq!(v["text"], text);
        assert_eq!(v["age_secs"], 7);
        assert!(v.get("why").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// No file, a link for the file or for the folder, a file too big: no text, and a reason the page shows in one line.
    #[test]
    fn a_link_a_missing_or_a_big_file_gives_no_text() {
        let d = scratch("refused");
        let now = SystemTime::now();
        assert_eq!(read(&d, now)["why"], "no numbers yet");
        assert_eq!(read(&d.join("none"), now)["why"], "no numbers yet");
        assert!(read(&d, now)["text"].is_null());

        std::fs::write(d.join("elsewhere.json"), "{}").unwrap();
        std::os::unix::fs::symlink(d.join("elsewhere.json"), d.join("machines.json")).unwrap();
        let v = read(&d, now);
        assert!(v["text"].is_null(), "a link is not followed: {v}");
        assert!(v["why"].as_str().unwrap().contains("not a plain file"), "{v}");
        std::fs::remove_file(d.join("machines.json")).unwrap();

        let real = scratch("refused-real");
        std::fs::write(real.join("machines.json"), "{}").unwrap();
        let linked = d.join("linked");
        std::os::unix::fs::symlink(&real, &linked).unwrap();
        let v = read(&linked, now);
        assert!(v["text"].is_null(), "a linked folder is not followed: {v}");

        std::fs::create_dir(d.join("machines.json")).unwrap();
        assert!(read(&d, now)["text"].is_null(), "a folder by the file's name is not read");
        std::fs::remove_dir(d.join("machines.json")).unwrap();

        std::fs::write(d.join("machines.json"), vec![b' '; (FILE_AT_MOST + 1) as usize]).unwrap();
        let v = read(&d, now);
        assert!(v["text"].is_null() && v["why"].as_str().unwrap().contains("larger"), "{v}");
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&real);
    }

    fn runs(folder: &Path) -> bool {
        RUNNING.lock().unwrap().contains(folder)
    }

    /// Waits, at most 30 s, for FOLDER's run to end.
    fn ended(folder: &Path) {
        let t0 = Instant::now();
        while runs(folder) {
            assert!(t0.elapsed() < Duration::from_secs(30), "the stand-in sampler did not end");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// No file starts one run, a second ask while it lives starts none, and the run writes the file read; a fresh file
    /// starts none, a 30 s old one starts the next. A link for the folder starts nothing.
    #[test]
    fn an_old_or_missing_file_starts_one_run_and_none_while_it_lives() {
        let d = scratch("refresh");
        let gate = d.join("gate");
        // the stand-in sampler: it waits for the test's word, then writes a sample where it is told
        let run = || {
            let mut c = Command::new("sh");
            c.arg("-c").arg(format!("while [ ! -e '{}' ]; do sleep 0.05; done; printf '{{\"read\": \"now\"}}' > \"$MACHINES_OUT\"", gate.display()));
            c
        };
        let now = SystemTime::now();
        assert!(start(&d, now, run()), "no file: a run starts");
        assert!(!start(&d, now, run()), "a run lives: no second one");
        assert!(read(&d, now)["text"].is_null(), "the answer does not wait for the run");
        std::fs::write(&gate, "").unwrap();
        ended(&d);
        let now = SystemTime::now();
        assert_eq!(read(&d, now)["text"], r#"{"read": "now"}"#);
        assert!(!start(&d, now, run()), "a fresh file: no run");
        assert!(start(&d, now + Duration::from_secs(FRESH_SECS), run()), "a 30 s old file: a run");
        ended(&d);

        assert!(!refresh(&d, &d.join("no-sampler"), now + Duration::from_secs(FRESH_SECS)), "no sampler: nothing runs");
        assert!(!refresh(&d, Path::new("gate"), now + Duration::from_secs(FRESH_SECS)), "a sampler by a relative path: nothing runs");

        let linked = d.join("linked");
        std::os::unix::fs::symlink(&d, &linked).unwrap();
        assert!(!start(&linked, now + Duration::from_secs(FRESH_SECS), run()), "a link for the folder: nothing runs");
        assert!(!runs(&linked));
        let _ = std::fs::remove_dir_all(&d);
    }
}
