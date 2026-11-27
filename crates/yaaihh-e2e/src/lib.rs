//! A built yaaihh window run headless and live (`--live --realtime`) on a scratch coordd: what a person presses, and
//! what the window then shows, as pictures, element trees and texts.

pub mod look;
pub mod scratch;

pub use look::{Look, Session, Shown, Step, Target, Window};
pub use scratch::{Release, Scratch};

use sha2::{Digest, Sha256};
use std::path::Path;

/// The sha256 of FILE's bytes, in hex.
pub fn digest(file: &Path) -> Result<String, String> {
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(hex(&Sha256::digest(&bytes)))
}

/// The sha256 of TEXT, in hex.
pub fn digest_text(text: &str) -> String {
    hex(&Sha256::digest(text.as_bytes()))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Whether SEEN has every line of WANTED that is not `!TEXT`, and none of those that are; the lines it fails on.
/// WANTED with no line that must be shown holds of a blank screen, so it never holds.
pub fn expect(seen: &str, wanted: &str) -> Result<(), Vec<String>> {
    let lines: Vec<&str> = wanted.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if lines.iter().all(|l| l.starts_with('!')) {
        return Err(vec![format!("{wanted:?} asks for nothing to be shown")]);
    }
    let failed: Vec<String> = lines
        .into_iter()
        .filter(|l| match l.strip_prefix('!') {
            Some(absent) => seen.contains(absent.trim()),
            None => !seen.contains(l),
        })
        .map(String::from)
        .collect();
    if failed.is_empty() { Ok(()) } else { Err(failed) }
}

#[cfg(test)]
mod tests;
