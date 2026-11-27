//! yaaihh in the background (owner 7 Nov: yaaihh replaces EasyCLIProxyAPI fully, with its own tray icon). Closing the
//! window hides it; the app keeps running, and with it the proxy backend's usage collector and the links to the
//! boxes. The tray icon opens the window again (a click, or "Open yaaihh"); "Quit yaaihh" ends it.
//!
//! One yaaihh per login: the first listens on `$XDG_RUNTIME_DIR/yaaihh.sock` (Windows: rune's one-process pipe); a
//! second start (the launcher, while the autostarted one sits in the tray) asks it to open its window and exits.
//!
//! Releases (vault designs/yaaihh-2026-11-07.md, rolling updates): `--replace` asks the running yaaihh to quit and
//! starts once it has (a release going live, or a rollback); `--dev` runs beside prod (its own socket and tray icon,
//! never taking the proxy's usage recording), to try a release without interrupting the owner.

#[cfg(unix)]
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(unix)]
use std::path::PathBuf;

const ICON: &[u8] = include_bytes!("../assets/yaaihh.svg");

/// assets/yaaihh.svg as straight RGBA, SIZE pixels square.
fn icon(size: u32) -> rune::TrayIcon {
    let tree = resvg::usvg::Tree::from_data(ICON, &resvg::usvg::Options::default()).expect("assets/yaaihh.svg parses");
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).expect("a nonzero icon size");
    let scale = size as f32 / tree.size().width();
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    // tiny-skia keeps premultiplied alpha; the tray takes it straight
    let rgba = pixmap.pixels().iter().flat_map(|p| { let c = p.demultiply(); [c.red(), c.green(), c.blue(), c.alpha()] }).collect();
    rune::TrayIcon::rgba(size, size, rgba)
}

/// The running yaaihh's socket: named after its config's folder (`~/.config/yaaihh`: `yaaihh.sock`, `yaaihh-dev.sock`
/// with --dev), so a yaaihh on another config (YAAIHH_CONFIG=~/.config/yaaihh-docket/projects.toml: the Docket dev
/// copy's `yaaihh-docket-dev.sock`) is a yaaihh of its own, not a second start of this one.
#[cfg(unix)]
fn socket() -> PathBuf {
    let dir = crate::places::runtime_dir();
    dir.join(socket_name(&crate::projects::config_path(), crate::dev()))
}

#[cfg(unix)]
fn socket_name(config: &str, dev: bool) -> String {
    format!("{}.sock", instance(config, dev))
}

/// Which yaaihh this is, for one per login: its config's folder, `-dev` with --dev. The Unix socket's name, and on
/// Windows the id of rune's one-process pipe.
fn instance(config: &str, dev: bool) -> String {
    let folder = std::path::Path::new(config).parent().and_then(|d| d.file_name()).and_then(|n| n.to_str()).filter(|n| !n.is_empty()).unwrap_or("yaaihh");
    format!("{folder}{}", if dev { "-dev" } else { "" })
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn the_default_config_keeps_its_socket_names() {
        assert_eq!(super::socket_name("/home/t/.config/yaaihh/projects.toml", false), "yaaihh.sock");
        assert_eq!(super::socket_name("/home/t/.config/yaaihh/projects.toml", true), "yaaihh-dev.sock");
        assert_eq!(super::socket_name("/home/t/.config/yaaihh-docket/projects.toml", true), "yaaihh-docket-dev.sock");
    }

    #[test]
    fn a_windows_config_names_its_instance_by_its_folder() {
        // places.rs: %APPDATA%\yaaihh\projects.toml; a Windows path's parts as std reads them on Windows
        let config = std::path::Path::new("C:").join("Users").join("t").join("AppData").join("Roaming").join("yaaihh").join("projects.toml");
        assert_eq!(super::instance(config.to_str().unwrap(), false), "yaaihh");
        assert_eq!(super::instance(config.to_str().unwrap(), true), "yaaihh-dev");
        assert_eq!(super::instance("projects.toml", false), "yaaihh");
    }
}

/// Ask the running yaaihh (of this kind: prod, or dev) to quit, and wait until it has. False when it is still
/// there after a while (it then keeps running and this start should not go on).
#[cfg(unix)]
pub fn replace() -> bool {
    let Ok(mut s) = UnixStream::connect(socket()) else { return true }; // none runs
    if s.write_all(b"quit\n").is_err() {
        return true;
    }
    let _ = s.read(&mut [0u8; 1]);
    // it answers, then quits: its socket stops answering once its process is gone
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(150));
        if UnixStream::connect(socket()).is_err() {
            return true;
        }
    }
    false
}

/// When a yaaihh already runs: ask it to open its window. True when it answered (this start should end).
#[cfg(unix)]
pub fn hand_over() -> bool {
    let Ok(mut s) = UnixStream::connect(socket()) else { return false };
    s.write_all(b"open\n").is_ok() && s.read(&mut [0u8; 1]).is_ok()
}

// One yaaihh per login on Windows is rune's one-process pipe (per user and session): a second start (the Start menu,
// a shortcut) brings the running one's window forward, or opens it again from the tray, and exits. `--replace` has
// nothing to ask: a release there is a new exe the owner starts when he likes (paper-research, N-412 p124).
#[cfg(not(unix))]
pub fn replace() -> bool {
    true
}

/// When a yaaihh already runs: it brought its window forward. True when it did (this start should end).
#[cfg(not(unix))]
pub fn hand_over() -> bool {
    !rune::single_instance(&instance(&crate::projects::config_path(), crate::dev()))
}

/// A window that took over from another (restart.rs) takes the one-window claim once the other is gone. True when it
/// holds it.
#[cfg(not(unix))]
pub fn claim() -> bool {
    rune::single_instance(&instance(&crate::projects::config_path(), crate::dev()))
}

/// On Linux the socket is this window's from its start (`start`).
#[cfg(unix)]
pub fn claim() -> bool {
    true
}

/// The tray menu's "Spelling marks in other programs" was picked (desk_marks.rs takes it at the next update).
pub static MARKS_PICKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The tray menu's "Split my messages by point" was picked (split.rs takes it at the next update).
pub static SPLIT_PICKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

thread_local! {
    /// The tray, on the window's thread, where its menu is shown again when the marks' switch changes.
    static TRAY: std::cell::RefCell<Option<rune::Tray>> = const { std::cell::RefCell::new(None) };
    /// What the menu shows: the marks' switch, and the split's (None: no coordd splits, not shown).
    static SHOWS: std::cell::Cell<(bool, Option<bool>)> = const { std::cell::Cell::new((false, None)) };
}

/// The tray taken down on the window's thread once the app's loop has ended, while the tray's own thread still runs.
/// Left to the thread-local's drop, Windows runs that at the process's exit after it has ended every other thread, and
/// rune's tray waits on its ended thread there: `join` panics inside a thread-local's destructor and the quitting
/// window aborts ("threads should not terminate unexpectedly" in stderr.log, N-585.p39).
pub fn end() {
    let tray = TRAY.with(|t| t.borrow_mut().take());
    drop(tray);
}

fn names() -> (&'static str, &'static str) {
    if crate::dev() { ("yaaihh-dev", "yaaihh dev") } else { ("yaaihh", "yaaihh") }
}

/// The tray's menu: Open, the marks over other programs' boxes where the machine has them (checked while on: the
/// app's, not a box's; paper-research N-487.p49), his messages split by point where a coordd splits (split.rs, beside
/// the spelling setting: paper-research N-589.p22), Quit.
fn menu(marks_on: bool, split: Option<bool>) -> Vec<rune::TrayItem> {
    let name = names().1;
    let mut m = vec![rune::TrayItem::action("open", format!("Open {name}")), rune::TrayItem::Separator];
    if crate::desk_marks::supported() {
        m.push(rune::TrayItem::action("marks", if marks_on { "✓ Spelling marks in other programs" } else { "Spelling marks in other programs" }));
    }
    if let Some(on) = split {
        m.push(rune::TrayItem::action("split", if on { "✓ Split my messages by point" } else { "Split my messages by point" }));
    }
    if crate::desk_marks::supported() || split.is_some() {
        m.push(rune::TrayItem::Separator);
    }
    m.push(rune::TrayItem::action("quit", format!("Quit {name}")));
    m
}

/// Whether the tray icon is there (when not, the marks' switch shows by the composer's Spelling instead).
pub fn shown() -> bool {
    TRAY.with(|t| t.borrow().is_some())
}

/// The marks' switch changed: the menu says so.
pub fn marks_changed(on: bool) {
    let (_, split) = SHOWS.get();
    show(on, split);
}

/// The split's switch changed, or whether a coordd splits: the menu says so (None: not shown).
pub fn split_changed(split: Option<bool>) {
    let (marks, _) = SHOWS.get();
    show(marks, split);
}

fn show(marks: bool, split: Option<bool>) {
    SHOWS.set((marks, split));
    TRAY.with(|t| {
        if let Some(t) = &*t.borrow() {
            t.set_menu(menu(marks, split));
        }
    });
}

/// Start the tray icon and the one-instance socket; both open the window through HANDLE. The tray is kept on this
/// thread (dropping it removes the icon).
pub fn start(handle: rune::AppHandle<crate::App>, marks_on: bool) {
    #[cfg(unix)]
    listen(handle.clone());
    let (id, name) = names();
    SHOWS.set((marks_on, None));
    let spec = rune::TraySpec::new(id, name, icon(64)).tooltip(format!("{name}: projects, agents and the proxy")).menu(menu(marks_on, None));
    let tray = rune::Tray::new(spec, move |e| match e {
        rune::TrayEvent::Activate { token: Some(t) } => handle.activate(t),
        rune::TrayEvent::Activate { token: None } => handle.open(),
        rune::TrayEvent::Menu(id) if id == "open" => handle.open(),
        rune::TrayEvent::Menu(id) if id == "marks" => {
            MARKS_PICKED.store(true, std::sync::atomic::Ordering::SeqCst);
            handle.wake();
        }
        rune::TrayEvent::Menu(id) if id == "split" => {
            SPLIT_PICKED.store(true, std::sync::atomic::Ordering::SeqCst);
            handle.wake();
        }
        rune::TrayEvent::Menu(id) if id == "quit" => handle.quit(),
        _ => {}
    });
    match tray {
        Ok(t) => TRAY.with(|k| *k.borrow_mut() = Some(t)),
        Err(e) => eprintln!("yaaihh: no tray icon ({e}); closing the window keeps yaaihh running all the same"),
    }
}

/// The one-instance socket: a later start asks this yaaihh to open its window, or to quit.
#[cfg(unix)]
fn listen(handle: rune::AppHandle<crate::App>) {
    let path = socket();
    let _ = std::fs::remove_file(&path); // a socket no yaaihh answers (hand_over failed before this)
    if let Ok(listener) = UnixListener::bind(&path) {
        let h = handle.clone();
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                let mut line = [0u8; 5];
                if s.read_exact(&mut line).is_err() {
                    continue;
                }
                match &line {
                    b"open\n" => h.open(),
                    b"quit\n" => {
                        let _ = s.write_all(b"k");
                        h.quit();
                        break; // the listener goes with the process; no more opens
                    }
                    _ => continue,
                }
                let _ = s.write_all(b"k");
            }
        });
    }
}
