//! What the host keeps: each kept folder's size and age once a day, and the proofs the capture tool leaves.

use docket_v2::run::value::{Instance, Value};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const SIZED: &str = "std.budgets.Sized";

/// The bytes under `p`, symlinks not followed.
fn bytes(p: &Path) -> u64 {
    let Ok(m) = std::fs::symlink_metadata(p) else { return 0 };
    if !m.is_dir() {
        return m.len();
    }
    std::fs::read_dir(p).map_or(0, |d| d.filter_map(Result::ok).map(|e| bytes(&e.path())).sum())
}

/// `Sized(key, mb, days)` for each kept folder that is there: its megabytes, and the days since it last changed.
pub fn sized(kept: &[PathBuf], now: SystemTime) -> Vec<Instance> {
    let mut out = Vec::new();
    for p in kept {
        let Ok(changed) = std::fs::metadata(p).and_then(|m| m.modified()) else { continue };
        let days = now.duration_since(changed).map_or(0, |d| d.as_secs() / 86_400);
        let mb = bytes(p).div_ceil(1 << 20);
        out.push(Instance { ty: SIZED.into(), args: vec![Value::text(&p.display().to_string()), Value::Int(mb as i64), Value::Int(days as i64)] });
    }
    out
}

/// Each `*.proof` in `dir` (four lines: piece, condition, file, release) as Event `event`, then renamed `*.taken`;
/// one that is not four lines is renamed `*.bad`.
pub fn proofs(dir: &Path, event: &str) -> Vec<Instance> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "proof")).collect();
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        let taken = match lines[..] {
            [target, cond, file, release] => {
                let args = vec![Value::text(target), Value::text(cond), Value::text(file), Value::text(release)];
                out.push(Instance { ty: event.into(), args });
                "taken"
            }
            _ => "bad",
        };
        if let Err(e) = std::fs::rename(&f, f.with_extension(taken)) {
            eprintln!("docket-driver-host: {}: {e}", f.display());
        }
    }
    out
}
