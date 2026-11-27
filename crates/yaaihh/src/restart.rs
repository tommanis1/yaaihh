//! A release reaches the owner's open window by itself (N-497; the owner, N-510.p4: "you guys can restart. the whole
//! iea of ysaaihh is that its saves my statee where iwas you can push trolling updates").
//!
//! `release.sh live` puts the new exe at `C:\yaaihh\yaaihh.exe` and, last, its release name in `release.txt` beside it
//! (yaaihh's `put_exe`). The window reads that one line every 10 s; when it names another release than this build's
//! (`YAAIHH_RELEASE`), it swaps itself for the exe there:
//! 1. this window writes its state (state.rs) and starts the exe with `--take-over PID --place X,Y,W,H,MAX --for RELEASE`;
//! 2. the new one opens hidden at that place, starts its bridges, draws its first frame and signs "up"; it does not
//!    take the one-window claim (tray.rs) yet;
//! 3. this one writes its state again and signs "go"; the new one takes that state back whole (state.rs `retake`: the
//!    page, the panes, the tabs and every box's text), shows itself over this one, focused, and signs "shown";
//! 4. this one ends its bridges and quits; the new one takes the one-window claim once this process is gone.
//! Each wait lasts as long as the process waited on lives, with no number of ours (paper-research, N-497.p8): this window
//! works as usual meanwhile, its bar saying small "starting RELEASE". A new exe that ends before it shows leaves this
//! window as it was, saying so once; that release is not tried again until release.txt names another. The keys typed between this window's last write
//! and the new one's focus are lost (a few frames); none goes to another program, since the new window is on top
//! before this one goes.
//!
//! Closed to the tray (N-804), the window still reads release.txt and swaps, the new one hidden as it was: whether the
//! new one shows is decided at "go", by whether this one is on screen then (the sign "open"). Opened again from the
//! tray or by a second start, it reads release.txt at once, so a newer exe there takes over at that open and the old
//! window is not what he gets back (N-497: a hidden old process took the next start and showed its old window).
//!
//! "Another release", not "a newer one": a `back` puts the previous exe there, and the window goes to it too. A build
//! without a release (a dev build) and a headless run do nothing.

use crate::App;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How often release.txt is read.
const EVERY: Duration = Duration::from_secs(10);

/// What the window knows of the exe beside it.
#[derive(Default)]
pub struct Restart {
    read_at: Option<Instant>,
    /// A swap under way.
    swap: Option<Swap>,
    /// A release that did not start, and why: said once on the bar, not tried again.
    pub failed: Option<(String, String)>,
    /// The release this window was started for when that is not its own (`--for`): not swapped to again.
    skip: Option<String>,
    /// This window took over from another: "now on RELEASE", until his first key.
    pub now_on: bool,
    /// Whether the window was on screen at the last poll: shown again from the tray, release.txt is read at once.
    shown: bool,
    /// Started to take over, and the old window's state not taken back yet (state.rs `retake`).
    taking: bool,
}

struct Swap {
    /// The new exe, once its start has returned (on a thread of its own: starting a new exe took 1.6 s on the try,
    /// Windows reading the file through, and the window must not stop for it).
    child: Option<Child>,
    spawned: std::sync::mpsc::Receiver<Result<Child, String>>,
    release: String,
    up: Sign,
    go: Sign,
    shown: Sign,
    /// Set with "go" when this window is on screen then: the new one shows; else it stays hidden, as this one was.
    open: Sign,
    began: Instant,
    went: Option<Instant>,
}

/// This build's release, when it has one; its hello names it (link.rs `release_named`).
pub(crate) fn ours() -> Option<&'static str> {
    option_env!("YAAIHH_RELEASE").filter(|r| !r.is_empty())
}

/// The folder the window was started from (its exe may have been renamed since; the folder stays).
fn folder() -> Option<PathBuf> {
    std::env::var_os("YAAIHH_RESTART_DIR").map(PathBuf::from).or_else(|| Some(std::env::current_exe().ok()?.parent()?.to_path_buf()))
}

/// The release release.txt in DIR names, when it is not OURS.
fn other_in(dir: &Path, ours: &str) -> Option<String> {
    let named = std::fs::read_to_string(dir.join("release.txt")).ok()?;
    let named = named.trim();
    (!named.is_empty() && named != ours).then(|| named.to_string())
}

/// The app's window, for its place on the desktop (set by `App::on_window`).
static WINDOW: Mutex<Option<rune::WindowHandle>> = Mutex::new(None);

pub fn window(h: &rune::WindowHandle) {
    *WINDOW.lock().unwrap() = Some(h.clone());
}

/// The app, for whether its window is on screen (a window closed to the tray keeps its place and stays ready), and for
/// what a window of its own hands it ([`to_app`]).
static APP: Mutex<Option<rune::AppHandle<App>>> = Mutex::new(None);

pub fn app(h: rune::AppHandle<App>) {
    *APP.lock().unwrap() = Some(h);
}

/// Run F on the app's state from a window of its own (sent_window.rs: a passage of it commented on, the window closed);
/// nothing without the app (a still, a test).
pub fn to_app(f: impl FnOnce(&mut App, &mut rune::Tx) + Send + 'static) {
    if let Some(app) = APP.lock().unwrap().clone() {
        app.update(f);
    }
}

/// Bring the app's window to the front (a passage of a window of its own commented on: its box is there).
pub fn front() {
    if let Some(w) = WINDOW.lock().unwrap().as_ref() {
        w.focus();
    }
}

/// Whether the window is on screen (minimised counts as on screen); without the app, as if it were.
fn on_screen() -> bool {
    APP.lock().unwrap().as_ref().is_none_or(|a| a.is_visible())
}

/// Whether release.txt is read now rather than at its time: the window was hidden at the last poll and is shown now.
fn read_at_once(was_shown: bool, shown: bool) -> bool {
    shown && !was_shown
}

/// Whether the new exe starts hidden: this window is not on screen (closed to the tray), has no place, or is not ready.
fn starts_hidden(on_screen: bool, placed: bool, ready: bool) -> bool {
    !on_screen || !placed || !ready
}

/// Each poll: release.txt read once every EVERY, on Windows (or with YAAIHH_RESTART_DIR, for a test); a swap under way
/// followed. True when what the bar shows changed.
pub fn poll(s: &mut App, tx: &mut rune::Tx) -> bool {
    let (Some(ours), true) = (ours(), cfg!(windows) || std::env::var_os("YAAIHH_RESTART_DIR").is_some()) else { return false };
    if rune::is_headless() {
        return false;
    }
    wake_by_thread(s);
    let shown = on_screen();
    if read_at_once(std::mem::replace(&mut s.restart.shown, shown), shown) {
        s.restart.read_at = None;
    }
    if s.restart.swap.is_some() {
        tx.wake_after(Duration::from_millis(30));
        return follow(s, tx);
    }
    s.wake_every(EVERY, tx);
    if s.restart.read_at.is_some_and(|t| t.elapsed() < EVERY) {
        return false;
    }
    s.restart.read_at = Some(Instant::now());
    let Some(other) = folder().and_then(|d| other_in(&d, ours)) else {
        return s.restart.failed.take().is_some();
    };
    if s.restart.failed.as_ref().is_some_and(|(r, _)| *r == other) || s.restart.skip.as_deref() == Some(other.as_str()) {
        return false;
    }
    let changed = s.restart.failed.take().is_some();
    match begin(s, &other) {
        Ok(()) => {
            tx.wake_after(Duration::from_millis(30));
            return true; // "starting RELEASE"
        }
        Err(why) => fail(s, other, why),
    }
    changed || s.restart.failed.is_some()
}

/// Set while a swap is under way: the waking thread wakes the window often.
static SWAPPING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static WAKING: std::sync::Once = std::sync::Once::new();

/// A thread that wakes the window every EVERY, and every 30 ms during a swap. A window rune does not draw (minimised,
/// covered, or on a desktop not shown) runs no poll for its own `wake_after`; a wake from outside still polls it (the
/// try on a desktop of its own read release.txt once and never again without this).
fn wake_by_thread(s: &App) {
    let Some(wake) = s.wake.clone() else { return };
    WAKING.call_once(|| {
        std::thread::spawn(move || {
            let mut last = Instant::now();
            loop {
                std::thread::sleep(Duration::from_millis(30));
                if SWAPPING.load(std::sync::atomic::Ordering::Relaxed) || last.elapsed() >= EVERY {
                    last = Instant::now();
                    wake();
                }
            }
        });
    });
}

/// Step 1: the state written, the exe there started to take over.
fn begin(s: &mut App, release: &str) -> Result<(), String> {
    if !crate::state::write_now(s) {
        return Err("the window's state could not be written".into());
    }
    let exe = folder().ok_or("no folder")?.join(if cfg!(windows) { "yaaihh.exe" } else { "yaaihh" });
    let pid = std::process::id();
    let (up, go, shown, open) = (Sign::make(pid, "up")?, Sign::make(pid, "go")?, Sign::make(pid, "shown")?, Sign::make(pid, "open")?);
    let mut cmd = std::process::Command::new(&exe);
    cmd.args(["--take-over", &pid.to_string(), "--for", release]);
    let (place, ready) = WINDOW.lock().unwrap().as_ref().map_or((None, false), |h| (h.placement(), h.is_ready()));
    match place {
        Some(p) if !p.minimized => {
            let (w, h) = p.content_size;
            cmd.args(["--place", &format!("{},{},{w},{h},{}", p.position.0, p.position.1, u8::from(p.maximized))]);
        }
        _ => {}
    }
    if starts_hidden(on_screen(), place.is_some(), ready) {
        cmd.arg("--hidden"); // no window on screen (closed to the tray): the new one starts the same
    }
    crate::log::line(&format!("swap onto {} ({release}): {cmd:?}", exe.display()));
    let (put, spawned) = std::sync::mpsc::channel();
    let wake = s.wake.clone();
    std::thread::spawn(move || {
        let t = Instant::now();
        let child = cmd.spawn().map_err(|e| format!("{}: {e}", exe.display()));
        crate::log::line(&format!("swap: the new exe's start took {} ms", t.elapsed().as_millis()));
        let _ = put.send(child);
        if let Some(w) = wake {
            w();
        }
    });
    SWAPPING.store(true, std::sync::atomic::Ordering::Relaxed);
    s.restart.swap = Some(Swap { child: None, spawned, release: release.into(), up, go, shown, open, began: Instant::now(), went: None });
    Ok(())
}

/// Steps 3 and 4, from this side; a new exe that does not come up, ended.
fn follow(s: &mut App, tx: &mut rune::Tx) -> bool {
    let sw = s.restart.swap.as_mut().expect("a swap");
    if sw.child.is_none() {
        match sw.spawned.try_recv() {
            Ok(Ok(c)) => sw.child = Some(c),
            Ok(Err(why)) => {
                let release = sw.release.clone();
                s.restart.swap = None;
                fail(s, release, why);
                return true;
            }
            Err(_) => return false,
        }
    }
    let child = sw.child.as_mut().expect("started");
    if let Ok(Some(status)) = child.try_wait() {
        let (release, waited) = (sw.release.clone(), sw.began.elapsed());
        s.restart.swap = None;
        fail(s, release, format!("it ended after {:.1} s ({status})", waited.as_secs_f64()));
        crate::state::write_now(s);
        return true;
    }
    match sw.went {
        None if sw.up.is_set() => {
            let up_in = sw.began.elapsed();
            crate::state::write_now(s);
            let sw = s.restart.swap.as_mut().expect("a swap");
            allow_foreground(sw.child.as_ref().map_or(0, |c| c.id()));
            if on_screen() {
                sw.open.set();
            }
            // from "go" the new window is his: this one's hellos name no release, so a bridge of its started again in
            // the gap cannot record it back (docket-edge7, N-1446 p511)
            crate::link::names_release(false);
            sw.go.set();
            sw.went = Some(Instant::now());
            crate::log::line(&format!("swap: {} up in {} ms; go", sw.release, up_in.as_millis()));
            false
        }
        Some(went) if sw.shown.is_set() => {
            let ended = crate::link::end_all();
            crate::log::line(&format!("swap: {} shown {} ms after go; {ended} bridges ended; this window quits", sw.release, went.elapsed().as_millis()));
            // stderr.log has no times of its own: what this window writes there as it goes is placed by this line
            eprintln!("{} yaaihh {}: this window quits (swap onto {})", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), std::process::id(), sw.release);
            tx.quit();
            false
        }
        _ => false,
    }
}

/// The release a restart he asks for goes onto, from DIR (the exe's folder): the one its release.txt names when that is
/// another (a newer exe there is what a restart should bring), else this one again; None with no exe there.
fn restart_onto(dir: &Path, ours: &str) -> Option<String> {
    dir.join(if cfg!(windows) { "yaaihh.exe" } else { "yaaihh" }).is_file().then(|| other_in(dir, ours).unwrap_or_else(|| ours.to_string()))
}

/// Whether the bar's menu offers Restart window: a release build where swaps run (Windows, or a test's
/// YAAIHH_RESTART_DIR), not headless, no swap under way, and an exe beside it to start.
pub fn can_restart(s: &App) -> bool {
    let (Some(ours), true) = (ours(), cfg!(windows) || std::env::var_os("YAAIHH_RESTART_DIR").is_some()) else { return false };
    !rune::is_headless() && s.restart.swap.is_none() && folder().and_then(|d| restart_onto(&d, ours)).is_some()
}

/// His press on Restart window (the owner's N-1793, item 3: "a clear restart button"): the swap a new release starts,
/// onto the exe beside this one ([`restart_onto`]), so the window comes back as it was, its pages, panes, tabs, the
/// threads open, where he was reading and every box's words (state.rs `retake`), the new window over this one before it
/// goes. A start that fails leaves this window as it was and says why on the bar, as a release's does.
pub fn now(s: &mut App, tx: &mut rune::Tx) {
    let Some(ours) = ours().filter(|_| can_restart(s)) else { return };
    let Some(release) = folder().and_then(|d| restart_onto(&d, ours)) else { return };
    s.restart.failed = None;
    crate::log::line(&format!("restart asked: onto {release}"));
    match begin(s, &release) {
        Ok(()) => tx.wake_after(Duration::from_millis(30)),
        Err(why) => fail(s, release, why),
    }
}

fn fail(s: &mut App, release: String, why: String) {
    SWAPPING.store(false, std::sync::atomic::Ordering::Relaxed);
    // this window stays his: its hellos name its release again
    crate::link::names_release(true);
    crate::log::line(&format!("swap onto {release} did not happen: {why}"));
    s.restart.failed = Some((release, why));
}

/// His first key after a swap: "now on" has been said.
pub fn key(s: &mut App) {
    s.restart.now_on = false;
}

/// The bar's line, only for a release that did not start (a swap is felt, not announced: the release's name is the
/// Releases page's, docs/specs/window-rules.md §1).
pub fn line(s: &App, k: &rune_kit::Kit) -> Option<rune::El> {
    let label = match (&s.restart.failed, s.restart.now_on, ours()) {
        (Some((release, why)), ..) => format!("{release} did not start: {why}"),
        _ => return None,
    };
    Some(rune::row().px(8.0).py(3.0).child(rune::text(label).font(k.ty.small).color(k.c.text_muted)).into())
}

/// The take-over this exe was started for (`--take-over PID [--place X,Y,W,H,MAX] [--for RELEASE] [--hidden]`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TakeOver {
    pub pid: u32,
    /// Where the old window stood: its frame's top-left and its content's size in physical pixels, and maximised.
    pub at: Option<(i32, i32, u32, u32, bool)>,
    pub release: Option<String>,
    pub hidden: bool,
}

static TAKING: Mutex<Option<TakeOver>> = Mutex::new(None);

/// The take-over's arguments, taken out of ARGS. Rune reads the whole command line too (its still flags), so none of
/// these may be one of its names: `--at` was, and the try's new exe ended on "not a number".
pub fn take_args(args: &mut Vec<String>) -> Option<TakeOver> {
    let value = |args: &mut Vec<String>, flag: &str| -> Option<Option<String>> {
        let i = args.iter().position(|a| a == flag)?;
        let v = args.get(i + 1).cloned();
        args.drain(i..(i + 2).min(args.len()));
        Some(v)
    };
    let pid = value(args, "--take-over")?.and_then(|p| p.parse().ok());
    let at = value(args, "--place").flatten().and_then(|a| {
        let p: Vec<&str> = a.split(',').collect();
        let [x, y, w, h, m] = p[..] else { return None };
        Some((x.parse().ok()?, y.parse().ok()?, w.parse().ok()?, h.parse().ok()?, m == "1"))
    });
    let release = value(args, "--for").flatten();
    let hidden = args.iter().position(|a| a == "--hidden").map(|i| args.remove(i)).is_some();
    let t = TakeOver { pid: pid?, at, release, hidden };
    // the old window is his until this one has taken over (`ready`): no hello of this one names its release before
    crate::link::names_release(false);
    *TAKING.lock().unwrap() = Some(t.clone());
    Some(t)
}

/// A window started to take over (S just made): not swapped again for the release it was started for when that is not
/// its own (a release.txt that names another build than the exe there).
pub fn init(s: &mut App) {
    if let Some(t) = TAKING.lock().unwrap().as_ref() {
        s.restart.skip = t.release.clone().filter(|r| Some(r.as_str()) != ours());
        s.restart.taking = true;
    }
}

/// Whether this window is between two in a swap: the old one once it said "go" (its state written for the new one),
/// the new one until it read that state. Neither sends what the other may send (words.rs: the messages in cleaning).
pub fn handed_over(s: &App) -> bool {
    s.restart.taking || s.restart.swap.as_ref().is_some_and(|w| w.went.is_some())
}

/// The window's first frame (`App::on_ready`): for a take-over, steps 2 and 3 from this side, on a thread of its own.
pub fn ready(h: &rune::WindowHandle, app: rune::AppHandle<App>) {
    let Some(t) = TAKING.lock().unwrap().take() else { return };
    let h = h.clone();
    std::thread::spawn(move || {
        let t0 = Instant::now();
        if let Some(up) = Sign::open(t.pid, "up") {
            up.set();
        }
        // as long as the old window lives: it says "go" once its state is written
        let went = Sign::open(t.pid, "go").is_some_and(|go| loop {
            if go.wait(Duration::from_millis(100)) {
                break true;
            }
            if !alive(t.pid) {
                break go.is_set();
            }
        });
        let took = t0.elapsed();
        app.update(|s, _| {
            crate::state::retake(s);
            s.restart.now_on = true;
            s.restart.taking = false;
        });
        // shown when the old window was on screen at "go"; an old release makes no such sign: as it started this one
        let show = Sign::open(t.pid, "open").map_or(!t.hidden, |o| o.is_set());
        if show {
            h.show();
            h.focus();
            if t.at.is_some_and(|a| a.4) {
                maximize(&h);
            }
        }
        if let Some(shown) = Sign::open(t.pid, "shown") {
            shown.set();
        }
        after(t.pid);
        let claimed = crate::tray::claim();
        crate::log::line(&format!(
            "took over from {}: {} after {} ms, {}; the one-window claim {}",
            t.pid,
            if went { "go" } else { "no go (it was gone)" },
            took.as_millis(),
            if show { "shown" } else { "hidden, as the old window was" },
            if claimed { "taken" } else { "held by another start" }
        ));
        // his window now: a second hello on every bridge it has names its release (I-1457; proto-lead N-1446 p509),
        // and says since when, its time and not coordd's (link.rs `SINCE`; paper-research p1392)
        crate::link::his_since(chrono::Utc::now());
        crate::link::names_release(true);
        app.update(|s, _| s.projects.iter().filter_map(|p| p.link.as_ref()).for_each(crate::link::Link::hello));
    });
}

/// The old window stood maximised: once this one shows, maximise it too (rune maximises only a window that opens shown).
fn maximize(h: &rune::WindowHandle) {
    #[cfg(windows)]
    {
        let until = Instant::now() + Duration::from_secs(2);
        while !h.is_ready() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = h.with_raw_handle(|w| {
            if let rune::raw_window_handle::RawWindowHandle::Win32(w) = w.as_raw() {
                use windows::Win32::UI::WindowsAndMessaging::{SW_MAXIMIZE, ShowWindow};
                unsafe {
                    let _ = ShowWindow(windows::Win32::Foundation::HWND(w.hwnd.get() as *mut _), SW_MAXIMIZE);
                }
            }
        });
    }
    #[cfg(not(windows))]
    let _ = h;
}

/// The new exe may take the foreground from this one (Windows gives it only to the process the user works in).
fn allow_foreground(pid: u32) {
    #[cfg(windows)]
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(pid);
    }
    #[cfg(not(windows))]
    let _ = pid;
}

/// Wait for process PID to end, as long as it lives.
pub fn after(pid: u32) {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
        if let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            let _ = WaitForSingleObject(h, INFINITE);
            let _ = CloseHandle(h);
        }
    }
    #[cfg(not(windows))]
    while alive(pid) {
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Whether process PID still runs.
fn alive(pid: u32) -> bool {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
        let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) else { return false };
        let running = WaitForSingleObject(h, 0) == WAIT_TIMEOUT;
        let _ = CloseHandle(h);
        running
    }
    #[cfg(not(windows))]
    {
        // a zombie (ended, not yet waited for) has ended
        std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| s.rsplit_once(") ").is_some_and(|(_, r)| !r.starts_with('Z')))
    }
}

/// A one-way sign between the two exes of a swap, named by the old one's pid and a word: a named event of this login's
/// session on Windows, a file in the temp folder elsewhere (the tests). The one who made it removes it.
struct Sign {
    #[cfg(windows)]
    h: isize,
    #[cfg(not(windows))]
    path: PathBuf,
    made: bool,
}

impl Sign {
    fn name(pid: u32, word: &str) -> String {
        format!("yaaihh-swap-{pid}-{word}")
    }

    fn make(pid: u32, word: &str) -> Result<Sign, String> {
        #[cfg(windows)]
        unsafe {
            use windows::Win32::System::Threading::CreateEventW;
            let name: Vec<u16> = format!("Local\\{}", Self::name(pid, word)).encode_utf16().chain(Some(0)).collect();
            let h = CreateEventW(None, true, false, windows::core::PCWSTR(name.as_ptr())).map_err(|e| format!("the sign {word}: {e}"))?;
            Ok(Sign { h: h.0 as isize, made: true })
        }
        #[cfg(not(windows))]
        {
            let path = std::env::temp_dir().join(Self::name(pid, word));
            let _ = std::fs::remove_file(&path);
            Ok(Sign { path, made: true })
        }
    }

    fn open(pid: u32, word: &str) -> Option<Sign> {
        #[cfg(windows)]
        unsafe {
            use windows::Win32::System::Threading::{EVENT_MODIFY_STATE, OpenEventW, SYNCHRONIZATION_SYNCHRONIZE};
            let name: Vec<u16> = format!("Local\\{}", Self::name(pid, word)).encode_utf16().chain(Some(0)).collect();
            let h = OpenEventW(EVENT_MODIFY_STATE | SYNCHRONIZATION_SYNCHRONIZE, false, windows::core::PCWSTR(name.as_ptr())).ok()?;
            Some(Sign { h: h.0 as isize, made: false })
        }
        #[cfg(not(windows))]
        {
            Some(Sign { path: std::env::temp_dir().join(Self::name(pid, word)), made: false })
        }
    }

    fn set(&self) {
        #[cfg(windows)]
        unsafe {
            let _ = windows::Win32::System::Threading::SetEvent(windows::Win32::Foundation::HANDLE(self.h as *mut _));
        }
        #[cfg(not(windows))]
        {
            let _ = std::fs::write(&self.path, b"");
        }
    }

    /// Whether it was set, waiting at most FOR.
    fn wait(&self, r#for: Duration) -> bool {
        #[cfg(windows)]
        unsafe {
            use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
            windows::Win32::System::Threading::WaitForSingleObject(HANDLE(self.h as *mut _), r#for.as_millis() as u32) == WAIT_OBJECT_0
        }
        #[cfg(not(windows))]
        {
            let until = Instant::now() + r#for;
            loop {
                if self.path.exists() {
                    return true;
                }
                if Instant::now() >= until {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    fn is_set(&self) -> bool {
        self.wait(Duration::ZERO)
    }
}

impl Drop for Sign {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(windows::Win32::Foundation::HANDLE(self.h as *mut _));
        }
        #[cfg(not(windows))]
        if self.made {
            let _ = std::fs::remove_file(&self.path);
        }
        let _ = self.made;
    }
}

/// For a test: `handed_over` reads ON, as for the window between two in a swap (the old one after "go", the new one
/// before it read the old one's texts); a real swap's signs are not made.
#[cfg(test)]
pub(crate) fn hand_over(s: &mut App, on: bool) {
    s.restart.taking = on;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_restart_he_asks_for_goes_onto_the_exe_beside_this_release_or_the_newer_one() {
        let d = std::env::temp_dir().join(format!("yaaihh-restart-onto-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(restart_onto(&d, "r1"), None, "no exe there: nothing to start");
        std::fs::write(d.join(if cfg!(windows) { "yaaihh.exe" } else { "yaaihh" }), "").unwrap();
        assert_eq!(restart_onto(&d, "r1").as_deref(), Some("r1"), "no release.txt: this release again");
        std::fs::write(d.join("release.txt"), "r1\n").unwrap();
        assert_eq!(restart_onto(&d, "r1").as_deref(), Some("r1"), "release.txt names this one: again");
        std::fs::write(d.join("release.txt"), "r2\n").unwrap();
        assert_eq!(restart_onto(&d, "r1").as_deref(), Some("r2"), "a newer one there: that one");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn release_txt_names_another_release_and_not_its_own() {
        let d = std::env::temp_dir().join(format!("yaaihh-restart-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(other_in(&d, "20261118-1400-a"), None, "no file: nothing");
        std::fs::write(d.join("release.txt"), "20261118-1400-a\n").unwrap();
        assert_eq!(other_in(&d, "20261118-1400-a"), None, "its own release");
        std::fs::write(d.join("release.txt"), "20261118-1500-b\n").unwrap();
        assert_eq!(other_in(&d, "20261118-1400-a").as_deref(), Some("20261118-1500-b"));
        std::fs::write(d.join("release.txt"), "20261118-1300-z\n").unwrap();
        assert_eq!(other_in(&d, "20261118-1400-a").as_deref(), Some("20261118-1300-z"), "a back is gone to too");
        std::fs::remove_dir_all(&d).ok();
    }

    /// A window closed to the tray swaps onto a hidden new one, and one shown again reads release.txt at once (N-804).
    #[test]
    fn a_window_closed_to_the_tray_swaps_hidden_and_reads_again_when_shown() {
        assert!(starts_hidden(false, true, true), "closed to the tray: its place kept and ready, still hidden");
        assert!(starts_hidden(true, false, true) && starts_hidden(true, true, false));
        assert!(!starts_hidden(true, true, true));
        assert!(read_at_once(false, true), "shown again from the tray");
        assert!(!read_at_once(true, true) && !read_at_once(false, false) && !read_at_once(true, false));
    }

    #[test]
    fn after_returns_once_the_process_has_ended() {
        let mut c = std::process::Command::new("sleep").arg("0.2").spawn().unwrap();
        let pid = c.id();
        let t = Instant::now();
        let waiter = std::thread::spawn(move || after(pid));
        let _ = c.wait();
        waiter.join().unwrap();
        assert!(t.elapsed() < Duration::from_secs(5));
        assert!(!alive(pid) && alive(std::process::id()));
    }

    #[test]
    fn the_take_over_arguments_come_out_of_the_rest() {
        let mut args: Vec<String> = ["--scene", "x", "--take-over", "4242", "--place", "-8,10,1920,1040,1", "--for", "20261118-1600-b", "--hidden"]
            .map(String::from)
            .to_vec();
        let t = take_args(&mut args).unwrap();
        assert_eq!(t, TakeOver { pid: 4242, at: Some((-8, 10, 1920, 1040, true)), release: Some("20261118-1600-b".into()), hidden: true });
        assert_eq!(args, ["--scene", "x"]);
        let mut plain: Vec<String> = vec!["--dev".into()];
        assert_eq!(take_args(&mut plain), None);
        assert_eq!(plain, ["--dev"]);
        TAKING.lock().unwrap().take();
    }

    #[test]
    fn a_sign_made_by_one_side_is_seen_by_the_other_and_gone_with_its_maker() {
        let pid = 4_000_000 + std::process::id() % 1000;
        let made = Sign::make(pid, "up").unwrap();
        assert!(!made.is_set());
        let other = Sign::open(pid, "up").unwrap();
        other.set();
        assert!(made.is_set() && made.wait(Duration::from_millis(1)));
        drop(other);
        assert!(made.is_set(), "the opener's end leaves it set");
        drop(made);
        assert!(!Sign::open(pid, "up").unwrap().is_set(), "its maker removed it");
    }
}
