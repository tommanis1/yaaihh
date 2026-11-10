//! yaaihh in the background (owner 7 Nov: yaaihh replaces EasyCLIProxyAPI fully, with its own tray icon). Closing the
//! window hides it; the app keeps running, and with it the proxy backend's usage collector and the links to the
//! boxes. The tray icon opens the window again (a click, or "Open yaaihh"); "Quit yaaihh" ends it.
//!
//! One yaaihh per login: the first listens on `$XDG_RUNTIME_DIR/yaaihh.sock`; a second start (the launcher, while
//! the autostarted one sits in the tray) asks it to open its window and exits.
//!
//! Releases (vault designs/yaaihh-2026-11-07.md, rolling updates): `--replace` asks the running yaaihh to quit and
//! starts once it has (a release going live, or a rollback); `--dev` runs beside prod (its own socket and tray icon,
//! never taking the proxy's usage recording), to try a release without interrupting the owner.

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
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

fn socket() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    dir.join(if crate::dev() { "yaaihh-dev.sock" } else { "yaaihh.sock" })
}

/// Ask the running yaaihh (of this kind: prod, or dev) to quit, and wait until it has. False when it is still
/// there after a while (it then keeps running and this start should not go on).
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
pub fn hand_over() -> bool {
    let Ok(mut s) = UnixStream::connect(socket()) else { return false };
    s.write_all(b"open\n").is_ok() && s.read(&mut [0u8; 1]).is_ok()
}

/// Start the tray icon and the one-instance socket; both open the window through HANDLE. The returned tray must
/// stay alive (dropping it removes the icon).
pub fn start(handle: rune::AppHandle<crate::App>) -> Option<rune::Tray> {
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
    let (id, name) = if crate::dev() { ("yaaihh-dev", "yaaihh dev") } else { ("yaaihh", "yaaihh") };
    let menu = vec![rune::TrayItem::action("open", format!("Open {name}")), rune::TrayItem::Separator, rune::TrayItem::action("quit", format!("Quit {name}"))];
    let spec = rune::TraySpec::new(id, name, icon(64)).tooltip(format!("{name}: projects, agents and the proxy")).menu(menu);
    let tray = rune::Tray::new(spec, move |e| match e {
        rune::TrayEvent::Activate { token: Some(t) } => handle.activate(t),
        rune::TrayEvent::Activate { token: None } => handle.open(),
        rune::TrayEvent::Menu(id) if id == "open" => handle.open(),
        rune::TrayEvent::Menu(id) if id == "quit" => handle.quit(),
        _ => {}
    });
    match tray {
        Ok(t) => Some(t),
        Err(e) => {
            eprintln!("yaaihh: no tray icon ({e}); closing the window keeps yaaihh running all the same");
            None
        }
    }
}
