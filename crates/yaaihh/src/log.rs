//! yaaihh's own record, so a window that hung or burned a core can be read afterwards (9 Nov: a window busy all night
//! left only its CPU total). Two files in ~/.local/state/yaaihh (yaaihh-dev for `--dev`):
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
    // ~/.local/state, or %LOCALAPPDATA% on Windows (places.rs)
    let base = crate::places::state_dir();
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
    #[cfg(windows)]
    stderr_to(&d);
    *FILE.lock().unwrap() = std::fs::OpenOptions::new().create(true).append(true).open(rotated(d.join("yaaihh.log"))).ok();
    line(&format!("yaaihh {} started", std::process::id()));
}

// The lines given to `line` on this thread, for a test to read back (`written`).
#[cfg(test)]
thread_local! {
    static WRITTEN: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The lines [`line`] was given on this thread since the last call, for a test to read back: a test has no log file.
#[cfg(test)]
pub fn written() -> Vec<String> {
    WRITTEN.with(|w| std::mem::take(&mut *w.borrow_mut()))
}

/// Append one line, with the local time.
pub fn line(text: &str) {
    #[cfg(test)]
    WRITTEN.with(|w| w.borrow_mut().push(text.to_string()));
    if let Some(f) = FILE.lock().unwrap().as_mut() {
        let at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(f, "{at} {text}");
    }
}

/// On Windows the window has no terminal for its stderr: Rune's lines go to `stderr.log` beside the others, and with
/// them the GPU it took (`RUNE_GPU_LOG`: the adapter, D3D12 on the real GPU or WARP in software; N-412 post 120).
/// Rust's stderr asks Windows for the handle on each write, so setting it once moves every later line.
#[cfg(windows)]
fn stderr_to(d: &std::path::Path) {
    use std::os::windows::io::IntoRawHandle;
    if std::env::var_os("RUNE_GPU_LOG").is_none() {
        // SAFETY: called from main before any other thread starts (open)
        unsafe { std::env::set_var("RUNE_GPU_LOG", "1") };
    }
    let Ok(f) = std::fs::OpenOptions::new().create(true).append(true).open(rotated(d.join("stderr.log"))) else { return };
    // the handle stays open for the process's life
    let h = f.into_raw_handle();
    // SAFETY: H is an open file handle that is never closed
    let _ = unsafe { windows::Win32::System::Console::SetStdHandle(windows::Win32::System::Console::STD_ERROR_HANDLE, windows::Win32::Foundation::HANDLE(h)) };
}
