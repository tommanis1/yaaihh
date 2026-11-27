//! yaaihh: the projects, their agents and views (lib.rs), with two built-in extensions, the Harnesses page
//! (harness.rs) and the Accounts page of the native path (accounts.rs). A variant with more extensions (an account proxy's pages, say) is its own small binary calling
//! [`yaaihh::run`] with them.

// a window program on Windows: no terminal opens with it; its lines go to the log folder (log.rs)
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    // on Windows the Harnesses page asks a coordd for what it read on the WSL machine (harness.rs); the Accounts page
    // asks coordd for everything it shows (N-412 ask 12)
    let mut exts = vec![yaaihh::harness::extension(), yaaihh::accounts::extension()];
    exts.extend(yaaihh::v2::extensions());
    yaaihh::run(exts);
}
