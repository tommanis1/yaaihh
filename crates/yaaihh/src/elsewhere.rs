//! What a window on Windows cannot show while the WSL machine's coordd is older than what it needs (the owner's ask 13;
//! docket-edge N-432.p4): files and a picture named by its path, the kind and call editor and the raw pane are served
//! by coordd (ask 13's ReadFile and ListDir, EditFile, Lsp; `RawPage`, N-802), and with an older one each says so in
//! one quiet line in place of the page; docket-ls's hovers on the program picture are left out without a word, and a
//! run's commit shows its hash alone. The line names no program and no command (the owner: nothing in the window
//! sends him to a terminal).

use rune::{El, col, text};
use rune_kit::Kit;

/// The line in place of a page an older coordd does not serve.
pub const LINE: &str = "Needs a newer engine on WSL.";

thread_local! {
    /// The Windows branch taken on Linux, for a test.
    static FORCED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The Linux branch taken on Windows, for a test whose files are folders on this machine.
    #[cfg(test)]
    static HERE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The WSL machine's files are not read from here: the app runs on Windows.
pub fn away() -> bool {
    (cfg!(windows) && !here()) || FORCED.with(|f| f.get())
}

#[cfg(test)]
fn here() -> bool {
    HERE.with(|h| h.get())
}

#[cfg(not(test))]
fn here() -> bool {
    false
}

/// Read the files from folders on this machine for the rest of this test's thread, on Windows too.
#[cfg(test)]
pub fn files_here() {
    HERE.with(|h| h.set(true));
}

/// Take the Windows branch on Linux for the rest of this run: a still of a page as the Windows window draws it (the
/// Linux build, which `--snap` can run; the Windows build cannot be shot here).
pub fn still_as_windows() {
    FORCED.with(|x| x.set(true));
}

/// A page left out: its one quiet line.
pub fn page(k: &Kit) -> El {
    col().w_full().padding(24.0).child(text(LINE).font(k.ty.small).color(k.c.text_muted).wrap())
}

/// Take the Windows branch on Linux while F runs (tests).
#[cfg(test)]
pub fn as_on_windows<T>(f: impl FnOnce() -> T) -> T {
    FORCED.with(|x| x.set(true));
    let out = f();
    FORCED.with(|x| x.set(false));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Windows branch, taken on Linux: an edit is refused with the line, a hover is none with nothing said, and
    /// a picture named by its path opens as the line.
    #[test]
    fn on_windows_what_reads_the_wsl_machines_files_says_so_or_stays_quiet() {
        let (_held, mut s) = crate::view_cost::test_app();
        as_on_windows(|| {
            crate::edit::start_kind(&mut s, "thread", None);
            assert_eq!(s.tickets.edits.said, Some(Err(LINE.to_string())));
            s.tickets.edits.said = None;
            crate::edit::start_call(&mut s, "post");
            assert_eq!(s.tickets.edits.said, Some(Err(LINE.to_string())));
            let mut docs = crate::docket_ls::Docs::default();
            docs.ask("p", &[], "/nowhere", "kind", "thread", None);
            assert!(matches!(docs.got.values().next(), Some(Ok(None))));
            crate::viewer::open_path("/nowhere/picture.png".into());
            assert!(crate::viewer::drain(&mut s));
            assert!(s.images.values().any(|i| matches!(i, crate::chat::Img::Failed(e) if e == LINE)));
        });
        assert!(!away(), "the branch is taken only while forced");
    }
}
