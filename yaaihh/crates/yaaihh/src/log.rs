//! yaaihh's own record, so a window that hung or burned a core can be read afterwards
//! . Two files in ~/.local/state/yaaihh (yaaihh-dev for `--dev`):
//!
//! * `frames.jsonl`: Rune's frame log (`RUNE_FRAME_WHY`), one line per second that drew: frames, commits and why,
//!   what kept it animating, the slowest frame. A window at rest writes nothing.
//! * `yaaihh.log`: the pipes to the boxes (up, down and why, what each brought per minute) and slow updates.
//!
//! Each file is moved to `NAME.1` when it passes 20 MB at start, so the two hold at most 80 MB.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

const ROTATE: u64 = 20 << 20;

static FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);

fn dir() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    base.join(if crate::DEV.load(std::sync::atomic::Ordering::Relaxed) { "yaaihh-dev" } else { "yaaihh" })
}

fn rotated(p: PathBuf) -> PathBuf {
    if std::fs::metadata(&p).is_ok_and(|m| m.len() > ROTATE) {
        let _ = std::fs::rename(&p, p.with_extension(format!("{}.1", p.extension().and_then(|e| e.to_str()).unwrap_or(""))));
    }
    p
}

/// Open the log and turn Rune's frame log on (unless RUNE_FRAME_WHY names another file). Before the window starts,
/// while yaaihh has one thread.
pub fn open() {
    let d = dir();
    if std::fs::create_dir_all(&d).is_err() {
        return;
    }
    if std::env::var_os("RUNE_FRAME_WHY").is_none() {
        // SAFETY: called from main before any other thread starts
        unsafe { std::env::set_var("RUNE_FRAME_WHY", rotated(d.join("frames.jsonl"))) };
    }
    *FILE.lock().unwrap() = std::fs::OpenOptions::new().create(true).append(true).open(rotated(d.join("yaaihh.log"))).ok();
    line(&format!("yaaihh {} started", std::process::id()));
}

/// Append one line, with the local time.
pub fn line(text: &str) {
    if let Some(f) = FILE.lock().unwrap().as_mut() {
        let at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(f, "{at} {text}");
    }
}
