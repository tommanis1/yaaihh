//! An API-key account's key file (`ROOT/claude/ID/key`, accounts/mod.rs), as its two writers write it: coordc from the
//! owner's terminal (`coord account key ID`) and coordd from the window's key field (`Request::AccountKeyStore`).
//! Nothing here prints, logs or returns a key.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// The key in what was TYPED (a pasted key often ends in a newline or a blank), or why it is none. The words never
/// hold what was typed.
pub fn word(typed: &str) -> Result<&str, &'static str> {
    match typed.trim() {
        "" => Err("nothing typed"),
        key if key.contains(char::is_whitespace) => Err("a key has no spaces: paste it again"),
        key => Ok(key),
    }
}

/// Write KEY as the key file FILE: a new file beside it, the owner's alone (mode 600) from its first byte, moved over
/// it, so there is never a half-written key and never one others can read. An error names the file, not the key.
pub fn write(file: &Path, key: &str) -> std::io::Result<()> {
    let new = file.with_extension("new");
    let _ = std::fs::remove_file(&new);
    let written = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&new).and_then(|mut f| writeln!(f, "{key}")).and_then(|()| std::fs::rename(&new, file));
    if written.is_err() {
        let _ = std::fs::remove_file(&new);
    }
    written
}
