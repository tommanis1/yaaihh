//! Spelling marks over other programs' text boxes, on Windows (the owner, N-481 p29: "if oyu turn it on it owkrs
//! eveyrhwere"; paper-research N-547; docket-edge8's reading of UI Automation, N-487 p33 and p43).
//!
//! - One switch, "Spelling marks in other programs", in the tray icon's menu (the app's, not a box's; by the
//!   composer's Spelling only when there is no tray icon), off until he turns it on, remembered (`desk-marks.json`
//!   beside the config). While off nothing of this exists: no thread, no UI Automation, no hook, no window. The marks
//!   work while yaaihh runs; on Windows closing its window ends them.
//! - On: a thread of its own follows the box with the focus through UI Automation (an out-of-context WinEvent hook
//!   wakes it on focus, caret, text, move and scroll; it looks 150 ms after, at most, never by polling; the pointer's
//!   moves only say whether it rests on a marked word, and cost nothing while no word is marked). A box with
//!   IsPassword is never asked for its text; one with no TextPattern (a plain Win32 EDIT, VS Code), yaaihh's own
//!   window and the terminals get no marks.
//! - The words of the visible text are checked by the window's own engine as the box checks (spell.rs `misspelled`:
//!   the machine's checker and Harper; the project's words and his added ones never marked) and underlined in one
//!   layered, click-through, topmost window that is never activated: nothing in his box moves. The candidates come in
//!   the box's order (spell.rs `offered`).
//! - With the pointer resting on a marked word (the system's hover time), a card stands right under it (paper-research
//!   N-487.p88): the word struck through and the best candidate, under it, dim, up to four more and "Add to
//!   dictionary". It goes when the pointer leaves the word and the card, when his box's text, caret or selection
//!   changes (a key), and when the word changes. It is not click-through and never activated, so his caret stays in
//!   his box. A candidate clicked is typed in place of the word (selected by UI Automation and read back first; his
//!   program's undo takes it back): the only thing that ever goes into his box, and only on his click. "Add to
//!   dictionary" keeps the word in spell.json, as the composer's.
//! - What is read stays in that thread: the text is dropped after each look, the checker's caches when the box
//!   changes. Nothing of it is logged, kept or sent.

use crate::App;
use std::collections::HashSet;
use std::ops::Range;
use std::sync::{Arc, Mutex};

/// Where the switch is kept.
fn path() -> std::path::PathBuf {
    std::path::Path::new(&crate::projects::config_path()).with_file_name("desk-marks.json")
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Saved {
    on: bool,
}

/// Whether this machine can have the marks (Windows only).
pub fn supported() -> bool {
    cfg!(windows)
}

/// The switch and, while it is on, the thread.
#[derive(Default)]
pub struct DeskMarks {
    pub on: bool,
    /// His added words and the project's, as the thread sees them (spell.rs `desk_words`).
    kept: Arc<Mutex<HashSet<String>>>,
    synced: Option<std::time::Instant>,
    /// The words he added from the popup, for spell.json.
    added: Option<std::sync::mpsc::Receiver<String>>,
    #[cfg(windows)]
    worker: Option<win::Worker>,
}

impl DeskMarks {
    pub fn load() -> DeskMarks {
        let saved: Saved = std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        DeskMarks { on: saved.on && supported(), ..Default::default() }
    }

    fn save(&self) {
        if cfg!(test) {
            return;
        }
        let _ = std::fs::write(path(), serde_json::to_string_pretty(&Saved { on: self.on }).unwrap());
    }
}

/// The window opened: the thread starts when he left the switch on.
pub fn setup(s: &mut App) {
    if s.desk.on {
        start(s);
    }
}

fn start(s: &mut App) {
    *s.desk.kept.lock().unwrap() = crate::spell::desk_words(s);
    s.desk.synced = Some(std::time::Instant::now());
    #[cfg(windows)]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        s.desk.added = Some(rx);
        s.desk.worker = win::Worker::start(s.desk.kept.clone(), tx, s.wake.clone());
    }
}

fn stop(s: &mut App) {
    #[cfg(windows)]
    {
        s.desk.worker = None;
    }
    s.desk.added = None;
}

/// The switch pressed.
pub fn toggle(s: &mut App) {
    s.desk.on = !s.desk.on && supported();
    s.desk.save();
    crate::tray::marks_changed(s.desk.on);
    if s.desk.on {
        start(s);
    } else {
        stop(s);
    }
}

/// Each update: the words he added from the popup go into spell.json, and the thread sees his kept words as the
/// composer's "Keep" left them. True when something changed.
pub fn poll(s: &mut App) -> bool {
    if crate::tray::MARKS_PICKED.swap(false, std::sync::atomic::Ordering::SeqCst) {
        toggle(s);
    }
    let Some(rx) = &s.desk.added else { return false };
    let added: Vec<String> = rx.try_iter().collect();
    for w in &added {
        crate::spell::keep(s, w);
    }
    // his added words and the project's (names, agents, parts), at once after he added one, else every 5 s
    if !added.is_empty() || s.desk.synced.is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(5)) {
        *s.desk.kept.lock().unwrap() = crate::spell::desk_words(s);
        s.desk.synced = Some(std::time::Instant::now());
    }
    !added.is_empty()
}

/// The marked words of TEXT, each as (start, length) in UTF-16 units: what UI Automation counts characters in.
#[cfg_attr(not(windows), allow(dead_code))]
fn marked_units(text: &str, mine: &HashSet<String>, most: usize) -> Vec<(Range<usize>, i32, i32, Vec<String>)> {
    let mut out = Vec::new();
    let (mut at, mut units) = (0usize, 0i32);
    for (r, harper) in crate::spell::misspelled(text, mine).into_iter().take(most) {
        units += text[at..r.start].encode_utf16().count() as i32;
        let len = text[r.clone()].encode_utf16().count() as i32;
        out.push((r.clone(), units, len, harper));
        at = r.start;
    }
    out
}

/// A wavy line from X0 to X1 at Y, SCALE times a 96-dpi size: its points.
#[cfg_attr(not(windows), allow(dead_code))]
fn wavy(x0: i32, x1: i32, y: i32, scale: f32) -> Vec<(i32, i32)> {
    let step = (2.0 * scale).round().max(2.0) as i32;
    let amp = (1.0 * scale).round().max(1.0) as i32;
    let mut out = Vec::new();
    let mut x = x0;
    let mut up = false;
    while x < x1 {
        out.push((x, if up { y - amp } else { y + amp }));
        up = !up;
        x += step;
    }
    out.push((x1, if up { y - amp } else { y + amp }));
    out
}

/// Where a popup of SIZE goes for a word at WORD (x, y, w, h), in the work area AREA (left, top, right, bottom): under
/// the word, else above it when it does not fit below; kept inside the area sideways.
#[cfg_attr(not(windows), allow(dead_code))]
fn popup_at(word: (i32, i32, i32, i32), size: (i32, i32), area: (i32, i32, i32, i32), gap: i32) -> (i32, i32) {
    let (x, y, _, h) = word;
    let below = y + h + gap;
    let top = if below + size.1 <= area.3 { below } else { (y - gap - size.1).max(area.1) };
    let left = x.min(area.2 - size.0).max(area.0);
    (left, top)
}

/// Whether point P is on a word with rectangles RECTS (left, top, right, bottom), or in the GAP between it and the card
/// under it: the pointer goes from the word to the card without the card going.
#[cfg_attr(not(windows), allow(dead_code))]
fn on_word(rects: &[(i32, i32, i32, i32)], gap: i32, p: (i32, i32)) -> bool {
    rects.iter().any(|&(l, t, r, b)| p.0 >= l && p.0 < r && p.1 >= t && p.1 < b + gap)
}

/// The card's row ROW: which of its items (the candidates, then "Add to dictionary"). With a best candidate HEAD, the
/// first row is it (the word struck, the candidate); without one, the first row is the word alone and no item.
#[cfg_attr(not(windows), allow(dead_code))]
fn item_of(row: usize, head: bool, items: usize) -> Option<usize> {
    let i = if head { row } else { row.checked_sub(1)? };
    (i < items).then_some(i)
}

#[cfg(windows)]
mod win {
    //! The thread: UI Automation, the hook, the marks' window and the popup.

    use super::{item_of, marked_units, on_word, popup_at, wavy};
    use std::cell::{Cell, RefCell};
    use std::collections::HashSet;
    use std::ffi::c_void;
    use std::sync::mpsc::Sender;
    use std::sync::{Arc, Mutex};
    use windows::Win32::Foundation::*;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::System::Com::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Ole::*;
    use windows::Win32::Security::*;
    use windows::Win32::System::Threading::*;
    use windows::Win32::UI::Accessibility::*;
    use windows::Win32::UI::HiDpi::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::*;

    /// The most words marked in one look.
    const MOST: usize = 60;
    /// The most of a visible range read.
    const READ: i32 = 20_000;
    /// The wait after an event before the look.
    const AFTER_MS: u32 = 150;
    /// The programs that get no marks: the terminals (what is in them is output, not his writing).
    const NOT: &[&str] = &["windowsterminal.exe", "openconsole.exe", "conhost.exe"];
    const NOT_CLASSES: &[&str] = &["TermControl", "ConsoleWindowClass", "PseudoConsoleWindow"];
    /// The marks' window's see-through colour, and the marks'.
    const KEY: COLORREF = COLORREF(0x00FF00FF);
    const RED: COLORREF = COLORREF(0x003030D8);
    const LOOK_TIMER: usize = 1;
    /// The pointer rests on a word: the card, after the system's hover time.
    const HOVER_TIMER: usize = 2;
    const ADD: &str = "Add to dictionary";
    const UNDO: &str = "Undo";
    /// The most words he replaced, and put back, remembered (each with its box, in memory only).
    const KEEP: usize = 20;
    /// Beside a word with no candidate (not struck: a struck word alone reads as "delete it"); the box's card says the
    /// same (docket-edge2).
    const NONE_NEAR: &str = "no word near it";
    /// WM_MOUSELEAVE (UI.Controls in the windows crate, not taken in for this one number).
    const WM_MOUSELEAVE: u32 = 0x02A3;

    /// The thread, while the switch is on; dropped: it ends.
    pub struct Worker {
        thread: u32,
    }

    impl Worker {
        pub fn start(kept: Arc<Mutex<HashSet<String>>>, added: Sender<String>, wake: Option<rune::Waker>) -> Option<Worker> {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::Builder::new()
                .name("desk-marks".into())
                .spawn(move || {
                    // SAFETY: the thread's own id
                    let _ = tx.send(unsafe { GetCurrentThreadId() });
                    run(kept, added, wake);
                })
                .ok()?;
            rx.recv().ok().map(|thread| Worker { thread })
        }
    }

    impl Drop for Worker {
        fn drop(&mut self) {
            // the thread ends its loop, takes its hook and windows away and drops what it read; not waited for (a
            // program that does not answer UI Automation must not hold the window)
            // SAFETY: a message to a thread of ours
            unsafe {
                let _ = PostThreadMessageW(self.thread, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
    }

    /// A marked word of the box: what it is, its range in the box, where it shows.
    struct Mark {
        word: String,
        /// Harper's and SymSpell's candidates for it.
        harper: Vec<String>,
        range: IUIAutomationTextRange,
        rects: Vec<RECT>,
        /// A word he replaced from the card: what it was (the dotted mark, its card offers Undo).
        was: Option<String>,
    }

    /// A word he replaced from the card, while it stands unchanged in the box it was replaced in.
    struct Changed {
        now: String,
        was: String,
        range: IUIAutomationTextRange,
        el: IUIAutomationElement,
    }

    /// What a look found.
    #[derive(Debug, PartialEq)]
    pub(super) enum Look {
        Marked(usize),
        Password,
        NoText,
        Skipped,
    }

    struct St {
        uia: IUIAutomation,
        me: u32,
        overlay: HWND,
        popup: HWND,
        kept: Arc<Mutex<HashSet<String>>>,
        added: Sender<String>,
        wake: Option<rune::Waker>,
        /// The foreground window of the last look (its root): keys go only while it still is.
        fg: HWND,
        /// The test's stand-in takes its keys as messages to this window (no keys reach a desktop that is not shown).
        keys_to: HWND,
        /// The box looked at last: its element and text pattern.
        el: Option<(IUIAutomationElement, IUIAutomationTextPattern)>,
        marks: Vec<Mark>,
        /// Where the marks' window is, on the screen.
        at: RECT,
        /// The mark the card is for, and what it offers: the candidates (the first is the struck line's, when HEAD),
        /// then "Add to dictionary".
        card: Option<usize>,
        items: Vec<String>,
        head: bool,
        /// The card's row under the pointer.
        hover: Option<usize>,
        /// Where the pointer came to rest on a word: the card at the hover time, if it is still there.
        rest: Option<POINT>,
        row: i32,
        font: HFONT,
        /// The struck word's font.
        strike: HFONT,
        /// The words he replaced, each in its own box, kept across focus changes while they stand unchanged (at most
        /// KEEP); the one just typed (now, was), found at the next look.
        changed: Vec<Changed>,
        just: Option<(String, String)>,
        /// The words he put back with Undo, each with its box: not marked again there (he chose them; at most KEEP).
        chosen: Vec<(IUIAutomationElement, String)>,
        scale: f32,
        pending: bool,
        /// The test's stand-in: a window of this process, on a desktop that has no foreground.
        test: bool,
    }

    thread_local! {
        static ST: RefCell<Option<St>> = const { RefCell::new(None) };
        /// An event came while a look was asking (a call out of this apartment lets messages in): another look after.
        static AGAIN: Cell<bool> = const { Cell::new(false) };
        /// The box changed: the checker's words of the last one go, at the next look (not inside a check).
        static FORGET: Cell<bool> = const { Cell::new(false) };
        /// A key came while a look was asking: the card goes after it.
        static CARD_GOES: Cell<bool> = const { Cell::new(false) };
    }

    fn with<R>(f: impl FnOnce(&mut St) -> R) -> Option<R> {
        ST.with(|s| s.try_borrow_mut().ok()?.as_mut().map(f))
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn class(name: &str, proc_: WNDPROC) -> Vec<u16> {
        let n = wide(name);
        // SAFETY: a class of this module's, its name kept by the caller
        unsafe {
            let wc = WNDCLASSW { lpfnWndProc: proc_, hInstance: GetModuleHandleW(None).unwrap_or_default().into(), lpszClassName: PCWSTR(n.as_ptr()), hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), ..Default::default() };
            RegisterClassW(&wc);
        }
        n
    }

    /// The windows and the automation, made on this thread (also the test's).
    pub(super) fn make(kept: Arc<Mutex<HashSet<String>>>, added: Sender<String>, wake: Option<rune::Waker>) -> Result<()> {
        // SAFETY: this thread's apartment, DPI and windows
        unsafe {
            let _ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            // a program that does not answer holds a look one second at most, not UI Automation's twenty
            if let Ok(u2) = uia.cast::<IUIAutomation2>() {
                let _ = u2.SetConnectionTimeout(1000);
                let _ = u2.SetTransactionTimeout(1000);
            }
            let oc = class("yaaihh-desk-marks", Some(overlay_proc));
            let overlay = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                PCWSTR(oc.as_ptr()),
                w!(""),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                None,
                None,
            )?;
            SetLayeredWindowAttributes(overlay, KEY, 0, LWA_COLORKEY)?;
            let pc = class("yaaihh-desk-marks-popup", Some(popup_proc));
            let popup = CreateWindowExW(WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW, PCWSTR(pc.as_ptr()), w!(""), WS_POPUP | WS_BORDER, 0, 0, 1, 1, None, None, None, None)?;
            let scale = GetDpiForWindow(overlay).max(96) as f32 / 96.0;
            let font = CreateFontW(-(12.0 * scale).round() as i32, 0, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, 0, w!("Segoe UI"));
            let strike = CreateFontW(-(12.0 * scale).round() as i32, 0, 0, 0, 400, 0, 0, 1, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, 0, w!("Segoe UI"));
            let row = (22.0 * scale).round() as i32;
            ST.with(|s| {
                *s.borrow_mut() = Some(St {
                    uia,
                    me: GetCurrentProcessId(),
                    overlay,
                    popup,
                    kept,
                    added,
                    wake,
                    el: None,
                    marks: Vec::new(),
                    at: RECT::default(),
                    card: None,
                    items: Vec::new(),
                    head: false,
                    hover: None,
                    rest: None,
                    row,
                    font,
                    strike,
                    changed: Vec::new(),
                    just: None,
                    chosen: Vec::new(),
                    scale,
                    pending: false,
                    test: false,
                    fg: HWND::default(),
                    keys_to: HWND::default(),
                })
            });
        }
        Ok(())
    }

    /// The windows gone and all that was read dropped.
    pub(super) fn unmake() {
        if let Some(st) = ST.with(|s| s.borrow_mut().take()) {
            // SAFETY: this thread's windows and font
            unsafe {
                let _ = DestroyWindow(st.popup);
                let _ = DestroyWindow(st.overlay);
                let _ = DeleteObject(st.font.into());
                let _ = DeleteObject(st.strike.into());
            }
        }
        crate::spell::forget();
    }

    fn run(kept: Arc<Mutex<HashSet<String>>>, added: Sender<String>, wake: Option<rune::Waker>) {
        if make(kept, added, wake).is_err() {
            unmake();
            return;
        }
        // SAFETY: the hooks and the loop are this thread's; the hook calls back on it (out of context: no code of
        // ours goes into another program)
        unsafe {
            let hooks = [
                SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, Some(on_event), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS),
                SetWinEventHook(EVENT_SYSTEM_SCROLLINGSTART, EVENT_SYSTEM_SCROLLINGEND, None, Some(on_event), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS),
                SetWinEventHook(EVENT_OBJECT_FOCUS, EVENT_OBJECT_FOCUS, None, Some(on_event), 0, 0, WINEVENT_OUTOFCONTEXT),
                SetWinEventHook(EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_VALUECHANGE, None, Some(on_event), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS),
                SetWinEventHook(EVENT_OBJECT_TEXTSELECTIONCHANGED, EVENT_OBJECT_TEXTSELECTIONCHANGED, None, Some(on_event), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS),
            ];
            soon();
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            for h in hooks {
                let _ = UnhookWinEvent(h);
            }
        }
        unmake();
    }

    /// A look 150 ms from the first event since the last one: a burst of typing is looked at as it goes, not after.
    fn soon() {
        with(|st| {
            if !st.pending {
                st.pending = true;
                // SAFETY: a timer on our own window
                unsafe { SetTimer(Some(st.overlay), LOOK_TIMER, AFTER_MS, None) };
            }
        });
    }

    /// The marks hidden at once (what they mark is moving), and a look soon.
    fn hide_and_look() {
        with(|st| {
            st.marks.clear();
            hide_card(st);
            // SAFETY: our own window
            unsafe {
                let _ = ShowWindow(st.overlay, SW_HIDE);
            }
        });
        soon();
    }

    extern "system" fn on_event(_: HWINEVENTHOOK, ev: u32, hwnd: HWND, obj: i32, _: i32, _: u32, _: u32) {
        if obj == OBJID_CURSOR.0 {
            if ev == EVENT_OBJECT_LOCATIONCHANGE {
                pointer_moved();
            }
            return;
        }
        // a key in his box: its text, caret or selection changes (no keyboard hook: his keys are not seen)
        let key = matches!(ev, EVENT_OBJECT_VALUECHANGE | EVENT_OBJECT_TEXTSELECTIONCHANGED) || (ev == EVENT_OBJECT_LOCATIONCHANGE && obj == OBJID_CARET.0);
        let Some(ours) = with(|st| hwnd == st.overlay || hwnd == st.popup) else {
            AGAIN.set(true);
            if ev == EVENT_OBJECT_FOCUS {
                FORGET.set(true);
            }
            if key {
                CARD_GOES.set(true);
            }
            return;
        };
        if ours {
            return;
        }
        if key {
            with(hide_card);
        }
        match ev {
            EVENT_OBJECT_FOCUS => {
                // another box: the words checked for the last one go
                FORGET.set(true);
                // the words he replaced or put back stay with their own boxes (consult-4, N-589.p10)
                with(|st| {
                    st.el = None;
                    st.just = None;
                });
                hide_and_look();
            }
            EVENT_SYSTEM_FOREGROUND | EVENT_SYSTEM_SCROLLINGSTART | EVENT_SYSTEM_SCROLLINGEND => hide_and_look(),
            // a window moved (not the caret): its words with it
            EVENT_OBJECT_LOCATIONCHANGE if obj == OBJID_WINDOW.0 => hide_and_look(),
            _ => soon(),
        }
    }

    /// Reads a SAFEARRAY of doubles, four to a rectangle.
    fn rects(r: &IUIAutomationTextRange) -> Vec<RECT> {
        // SAFETY: the array UI Automation hands over, read and destroyed here
        unsafe {
            let Ok(sa) = r.GetBoundingRectangles() else { return Vec::new() };
            if sa.is_null() {
                return Vec::new();
            }
            let mut out = Vec::new();
            if let Ok(ub) = SafeArrayGetUBound(sa, 1) {
                let mut p: *mut c_void = std::ptr::null_mut();
                if SafeArrayAccessData(sa, &mut p).is_ok() {
                    let v = std::slice::from_raw_parts(p as *const f64, (ub + 1).max(0) as usize);
                    for c in v.chunks(4).filter(|c| c.len() == 4) {
                        let (x, y, w, h) = (c[0] as i32, c[1] as i32, c[2] as i32, c[3] as i32);
                        if w > 0 && h > 0 {
                            out.push(RECT { left: x, top: y, right: x + w, bottom: y + h });
                        }
                    }
                    let _ = SafeArrayUnaccessData(sa);
                }
            }
            let _ = SafeArrayDestroy(sa);
            out
        }
    }

    /// The program's file name, lower case.
    fn program(pid: i32) -> String {
        // SAFETY: a handle opened and closed here
        unsafe {
            let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid as u32) else { return String::new() };
            let mut buf = [0u16; 512];
            let mut n = buf.len() as u32;
            let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n);
            let _ = CloseHandle(h);
            if r.is_err() {
                return String::new();
            }
            String::from_utf16_lossy(&buf[..n as usize]).rsplit('\\').next().unwrap_or("").to_lowercase()
        }
    }

    /// Whether program PID runs elevated, or cannot be asked (taken as elevated).
    fn elevated(pid: i32) -> bool {
        // SAFETY: handles opened and closed here; the token's one fixed-size field read
        unsafe {
            let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid as u32) else { return true };
            let mut tok = HANDLE::default();
            let opened = OpenProcessToken(h, TOKEN_QUERY, &mut tok).is_ok();
            let _ = CloseHandle(h);
            if !opened {
                return true;
            }
            let mut e = TOKEN_ELEVATION::default();
            let mut n = 0u32;
            let r = GetTokenInformation(tok, TokenElevation, Some(&mut e as *mut _ as *mut c_void), std::mem::size_of::<TOKEN_ELEVATION>() as u32, &mut n);
            let _ = CloseHandle(tok);
            r.is_err() || e.TokenIsElevated != 0
        }
    }

    /// Whether the box of the last look is still where his keys go: the same foreground window, the same focused
    /// element.
    fn still_his(st: &St) -> bool {
        if st.test {
            return true;
        }
        let Some((el, _)) = &st.el else { return false };
        // SAFETY: UI Automation on this thread's apartment
        unsafe {
            GetAncestor(GetForegroundWindow(), GA_ROOT) == st.fg
                && !st.fg.is_invalid()
                && st.uia.GetFocusedElement().and_then(|f| st.uia.CompareElements(&f, el)).is_ok_and(|b| b.as_bool())
        }
    }

    /// The look: the box with the focus.
    fn look() {
        AGAIN.set(false);
        if FORGET.replace(false) {
            crate::spell::forget();
        }
        let el = with(|st| {
            st.pending = false;
            // SAFETY: our timer
            unsafe {
                let _ = KillTimer(Some(st.overlay), LOOK_TIMER);
                st.uia.GetFocusedElement().ok()
            }
        })
        .flatten();
        match el {
            Some(el) => {
                let _ = look_at(&el);
            }
            None => clear(),
        }
        if CARD_GOES.replace(false) {
            with(hide_card);
        }
        if AGAIN.get() {
            soon();
        }
    }

    fn hide_card(st: &mut St) {
        st.card = None;
        st.hover = None;
        st.rest = None;
        // SAFETY: our own window and timer
        unsafe {
            let _ = KillTimer(Some(st.overlay), HOVER_TIMER);
            let _ = ShowWindow(st.popup, SW_HIDE);
        }
    }

    fn pointer() -> POINT {
        let mut p = POINT::default();
        // SAFETY: a point of ours
        unsafe {
            let _ = GetCursorPos(&mut p);
        }
        p
    }

    /// The gap between a word and its card: the wave shows in it.
    fn gap(st: &St) -> i32 {
        (3.0 * st.scale).ceil() as i32
    }

    /// The mark under point P.
    fn mark_at(st: &St, p: POINT) -> Option<usize> {
        let g = gap(st);
        st.marks.iter().position(|m| on_word(&m.rects.iter().map(|r| (r.left, r.top, r.right, r.bottom)).collect::<Vec<_>>(), g, (p.x, p.y)))
    }

    fn in_card(st: &St, p: POINT) -> bool {
        let mut r = RECT::default();
        // SAFETY: our own window
        unsafe { st.card.is_some() && GetWindowRect(st.popup, &mut r).is_ok() && p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom }
    }

    /// The pointer moved (the cursor's own event; nothing when no word is marked): the card goes when it left the word
    /// and the card; on a word, the hover time starts again whenever it moved more than the system's hover box.
    fn pointer_moved() {
        with(|st| {
            if st.marks.is_empty() {
                return;
            }
            let p = pointer();
            let at = mark_at(st, p);
            if let Some(i) = st.card {
                if at == Some(i) || in_card(st, p) {
                    return;
                }
                hide_card(st);
            }
            // SAFETY: our own timer; the system's hover sizes read
            unsafe {
                match at {
                    Some(_) => {
                        let (mut w, mut h, mut ms) = (4u32, 4u32, 400u32);
                        let _ = SystemParametersInfoW(SPI_GETMOUSEHOVERWIDTH, 0, Some(&mut w as *mut u32 as *mut c_void), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
                        let _ = SystemParametersInfoW(SPI_GETMOUSEHOVERHEIGHT, 0, Some(&mut h as *mut u32 as *mut c_void), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
                        let _ = SystemParametersInfoW(SPI_GETMOUSEHOVERTIME, 0, Some(&mut ms as *mut u32 as *mut c_void), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
                        let moved = st.rest.is_none_or(|r| (r.x - p.x).unsigned_abs() * 2 > w || (r.y - p.y).unsigned_abs() * 2 > h);
                        if moved {
                            st.rest = Some(p);
                            SetTimer(Some(st.overlay), HOVER_TIMER, ms.max(1), None);
                        }
                    }
                    None => {
                        if st.rest.take().is_some() {
                            let _ = KillTimer(Some(st.overlay), HOVER_TIMER);
                        }
                    }
                }
            }
        });
    }

    /// The hover time passed: the card for the word the pointer rests on.
    fn rested() {
        with(|st| {
            // SAFETY: our own timer
            unsafe {
                let _ = KillTimer(Some(st.overlay), HOVER_TIMER);
            }
            if st.rest.take().is_none() || st.card.is_some() {
                return;
            }
            if let Some(i) = mark_at(st, pointer()) {
                st.card = Some(i);
                show_card(st);
            }
        });
    }

    fn clear() {
        with(|st| {
            st.marks.clear();
            st.el = None;
            hide_card(st);
            // SAFETY: our own window
            unsafe {
                let _ = ShowWindow(st.overlay, SW_HIDE);
            }
        });
    }

    /// The look at box EL: its visible words checked, the marks drawn, the card kept while its word is.
    pub(super) fn look_at(el: &IUIAutomationElement) -> Look {
        let found = with(|st| read(st, el)).unwrap_or(Ok((Look::Skipped, None)));
        match found {
            Ok((look, Some((marks, tp)))) => {
                with(|st| {
                    // the card stays on the same word in the same place (the pointer leaving takes it away by itself)
                    let was = st.card.and_then(|i| st.marks.get(i)).map(|m| (m.word.clone(), m.rects[0]));
                    st.el = Some((el.clone(), tp));
                    st.marks = marks;
                    draw(st, was);
                });
                look
            }
            Ok((look, None)) => {
                clear();
                look
            }
            Err(_) => {
                clear();
                Look::Skipped
            }
        }
    }

    type Found = (Vec<Mark>, IUIAutomationTextPattern);

    fn read(st: &mut St, el: &IUIAutomationElement) -> Result<(Look, Option<Found>)> {
        // SAFETY: UI Automation calls on this thread's apartment
        unsafe {
            // no foreground: a secure desktop shows (a UAC prompt, the lock screen), which this thread cannot reach
            let fg = GetAncestor(GetForegroundWindow(), GA_ROOT);
            if fg.is_invalid() && !st.test {
                return Ok((Look::Skipped, None));
            }
            st.fg = fg;
            let pid = el.CurrentProcessId()?;
            if pid as u32 == st.me && !st.test {
                return Ok((Look::Skipped, None));
            }
            // a program run as administrator: nothing read, nothing typed (its token is asked, never its text)
            if elevated(pid) {
                return Ok((Look::Skipped, None));
            }
            // asked before anything else: a password box is never asked for its text
            if el.CurrentIsPassword()?.as_bool() {
                return Ok((Look::Password, None));
            }
            let class = el.CurrentClassName().map(|c| c.to_string()).unwrap_or_default();
            if NOT_CLASSES.contains(&class.as_str()) || NOT.contains(&program(pid).as_str()) {
                return Ok((Look::Skipped, None));
            }
            let Ok(tp) = el.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) else {
                return Ok((Look::NoText, None));
            };
            let mut kept = st.kept.lock().unwrap().clone();
            kept.extend(st.chosen.iter().filter(|(b, _)| st.uia.CompareElements(b, el).is_ok_and(|x| x.as_bool())).map(|(_, w)| w.clone()));
            let vis = tp.GetVisibleRanges()?;
            let mut marks = Vec::new();
            for i in 0..vis.Length()?.min(4) {
                let v = vis.GetElement(i)?;
                // the text is read, checked and dropped here; only the marked words stay, for the popup
                let text = v.GetText(READ)?.to_string();
                // only endpoints are moved (`Move` on an empty range takes a character in WPF: N-487)
                let mut cursor = v.Clone()?;
                cursor.MoveEndpointByRange(TextPatternRangeEndpoint_End, &cursor, TextPatternRangeEndpoint_Start)?;
                let mut pos = 0;
                for (r, start, len, harper) in marked_units(&text, &kept, super::MOST_MARKS.min(MOST - marks.len())) {
                    let word = cursor.Clone()?;
                    if word.MoveEndpointByUnit(TextPatternRangeEndpoint_End, TextUnit_Character, start - pos + len)? != start - pos + len
                        || word.MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, start - pos)? != start - pos
                    {
                        break;
                    }
                    cursor = word.Clone()?;
                    cursor.MoveEndpointByRange(TextPatternRangeEndpoint_End, &cursor, TextPatternRangeEndpoint_Start)?;
                    pos = start;
                    // the box counts its characters as the text says, or its words are not marked
                    if word.GetText(len + 1)?.to_string() != text[r.clone()] {
                        break;
                    }
                    let rs = rects(&word);
                    if !rs.is_empty() {
                        marks.push(Mark { word: text[r].to_string(), harper, range: word, rects: rs, was: None });
                    }
                }
            }
            // the word he just replaced: it ends at the caret (his keys went there)
            if let Some((now, was)) = st.just.take()
                && let Some(c) = caret_range(el, &tp)
                && let Ok(r) = c.Clone()
            {
                let n = now.encode_utf16().count() as i32;
                if r.MoveEndpointByRange(TextPatternRangeEndpoint_Start, &c, TextPatternRangeEndpoint_Start).is_ok()
                    && r.MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, -n).is_ok_and(|m| m == -n)
                    && r.GetText(n + 1).is_ok_and(|t| t.to_string() == now)
                {
                    st.changed.push(Changed { now, was, range: r, el: el.clone() });
                    if st.changed.len() > KEEP {
                        st.changed.remove(0);
                    }
                }
            }
            // the words he replaced keep the dotted mark while they stand unchanged
            let uia = st.uia.clone();
            let here = |c: &Changed| uia.CompareElements(&c.el, el).is_ok_and(|b| b.as_bool());
            // this box's are read again (a word changed by anyone goes); another box's wait for it
            st.changed.retain(|c| !here(c) || c.range.GetText(c.now.encode_utf16().count() as i32 + 1).is_ok_and(|t| t.to_string() == c.now));
            for c in st.changed.iter().filter(|c| here(c)) {
                let rs = rects(&c.range);
                if !rs.is_empty() && marks.len() < MOST {
                    marks.push(Mark { word: c.now.clone(), harper: Vec::new(), range: c.range.clone(), rects: rs, was: Some(c.was.clone()) });
                }
            }
            let n = marks.len();
            Ok((Look::Marked(n), Some((marks, tp))))
        }
    }

    /// Where the caret is: TextPattern2's caret, else the selection's start when nothing is selected.
    fn caret_range(el: &IUIAutomationElement, tp: &IUIAutomationTextPattern) -> Option<IUIAutomationTextRange> {
        // SAFETY: UI Automation calls on this thread's apartment
        unsafe {
            if let Ok(tp2) = el.GetCurrentPatternAs::<IUIAutomationTextPattern2>(UIA_TextPattern2Id) {
                let mut active = BOOL(0);
                if let Ok(c) = tp2.GetCaretRange(&mut active) {
                    return Some(c);
                }
            }
            let sel = tp.GetSelection().ok()?;
            let c = sel.GetElement(0).ok()?;
            (c.CompareEndpoints(TextPatternRangeEndpoint_Start, &c, TextPatternRangeEndpoint_End).ok()? == 0).then_some(c)
        }
    }


    /// The marks' window over the marks that show (not under another window); the card kept when its word WAS is still
    /// marked where it was.
    fn draw(st: &mut St, was: Option<(String, RECT)>) {
        // SAFETY: our own windows; WindowFromPoint passes over the click-through marks' window
        unsafe {
            let fg = GetAncestor(GetForegroundWindow(), GA_ROOT);
            let test = st.test;
            st.marks.retain_mut(|m| {
                m.rects.retain(|r| {
                    let p = POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 };
                    test || GetAncestor(WindowFromPoint(p), GA_ROOT) == fg
                });
                !m.rects.is_empty()
            });
            if st.marks.is_empty() {
                let _ = ShowWindow(st.overlay, SW_HIDE);
                hide_card(st);
                return;
            }
            let all = st.marks.iter().flat_map(|m| m.rects.iter());
            let pad = (3.0 * st.scale).ceil() as i32;
            let mut at = RECT { left: i32::MAX, top: i32::MAX, right: i32::MIN, bottom: i32::MIN };
            for r in all {
                at.left = at.left.min(r.left);
                at.top = at.top.min(r.top);
                at.right = at.right.max(r.right);
                at.bottom = at.bottom.max(r.bottom + pad);
            }
            st.at = at;
            let _ = SetWindowPos(st.overlay, Some(HWND_TOPMOST), at.left, at.top, at.right - at.left, at.bottom - at.top, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            let _ = InvalidateRect(Some(st.overlay), None, true);
            match was.and_then(|(w, r)| st.marks.iter().position(|m| m.word == w && m.rects[0] == r)) {
                Some(i) => {
                    st.card = Some(i);
                    show_card(st);
                }
                None => hide_card(st),
            }
        }
    }

    /// The card's first line: the word struck, and what stands beside it when it is not the best candidate (a word he
    /// replaced: what it is now).
    fn card_head(st: &St) -> (String, Option<String>) {
        match st.card.and_then(|i| st.marks.get(i)) {
            Some(Mark { word, was: Some(was), .. }) => (was.clone(), Some(word.clone())),
            Some(m) => (m.word.clone(), None),
            None => (String::new(), None),
        }
    }

    /// The card's text widths: WORD struck, a space, and each of TEXTS, on the card's own DC.
    fn widths(st: &St, word: &str, texts: &[String]) -> (i32, i32, Vec<i32>) {
        // SAFETY: our own card's DC, its fonts put back
        unsafe {
            let dc = GetDC(Some(st.popup));
            let measure = |f: HFONT, t: &str| {
                let old = SelectObject(dc, f.into());
                let mut sz = SIZE::default();
                let t: Vec<u16> = t.encode_utf16().collect();
                let _ = GetTextExtentPoint32W(dc, &t, &mut sz);
                SelectObject(dc, old);
                sz.cx
            };
            let w = measure(st.strike, word);
            let space = measure(st.font, "  ");
            let items = texts.iter().map(|t| measure(st.font, t)).collect();
            ReleaseDC(Some(st.popup), dc);
            (w, space, items)
        }
    }

    /// The card for mark ST.card, right under its word (above it at the screen's foot).
    fn show_card(st: &mut St) {
        // SAFETY: our own card
        unsafe {
            let Some(m) = st.card.and_then(|i| st.marks.get(i)) else {
                hide_card(st);
                return;
            };
            let r = m.rects[0];
            let (struck, plain) = card_head(st);
            match &m.was {
                // a word he replaced: what it was struck, what it is, Undo
                Some(_) => {
                    st.items = vec![UNDO.to_string()];
                    st.head = false;
                }
                None => {
                    st.items = crate::spell::offered(&m.word, &m.harper, &st.kept.lock().unwrap());
                    st.head = !st.items.is_empty();
                    st.items.push(ADD.to_string());
                }
            }
            st.hover = None;
            let (ww, space, iw) = widths(st, &struck, &st.items);
            let none = NONE_NEAR.to_string();
            let p = plain.as_ref().or(if st.head { st.items.first() } else { None }).unwrap_or(&none);
            let first = ww + space + widths(st, "", std::slice::from_ref(p)).2[0];
            let w = iw.iter().skip(st.head as usize).copied().fold(first, i32::max);
            let pad = (10.0 * st.scale) as i32;
            let rows = st.items.len() + !st.head as usize;
            let size = (w + 2 * pad, st.row * rows as i32 + 2);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(MonitorFromPoint(POINT { x: r.left, y: r.top }, MONITOR_DEFAULTTONEAREST), &mut mi);
            let a = mi.rcWork;
            let (x, y) = popup_at((r.left, r.top, r.right - r.left, r.bottom - r.top), size, (a.left, a.top, a.right, a.bottom), gap(st));
            let _ = SetWindowPos(st.popup, Some(HWND_TOPMOST), x, y, size.0, size.1, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            let _ = InvalidateRect(Some(st.popup), None, true);
        }
    }

    /// COLOR_WINDOWTEXT taken most of the way to COLOR_WINDOW: the dim rows, still read as text to click.
    fn dim() -> COLORREF {
        // SAFETY: system colours read
        let (t, b) = unsafe { (GetSysColor(COLOR_WINDOWTEXT), GetSysColor(COLOR_WINDOW)) };
        let mix = |s: u32| ((((t >> s) & 0xFF) * 62 + ((b >> s) & 0xFF) * 38) / 100) << s;
        COLORREF(mix(0) | mix(8) | mix(16))
    }

    /// The marks on DC whose top left is screen point AT: the wave under a misspelt word, a dotted line under a word he
    /// replaced (N-487.p90).
    fn paint_marks(st: &St, dc: HDC, at: (i32, i32)) {
        // SAFETY: a pen and a brush of ours on the caller's DC, put back and deleted
        unsafe {
            let th = st.scale.round().max(1.0) as i32;
            let pen = CreatePen(PS_SOLID, th, RED);
            let red = CreateSolidBrush(RED);
            let old = SelectObject(dc, pen.into());
            for m in &st.marks {
                for r in &m.rects {
                    let (x0, x1, y) = (r.left - at.0, r.right - at.0, r.bottom - at.1);
                    if m.was.is_some() {
                        let mut x = x0;
                        while x < x1 {
                            FillRect(dc, &RECT { left: x, top: y, right: (x + th).min(x1), bottom: y + th }, red);
                            x += 3 * th;
                        }
                    } else {
                        let pts: Vec<POINT> = wavy(x0, x1, y, st.scale).into_iter().map(|(x, y)| POINT { x, y }).collect();
                        let _ = Polyline(dc, &pts);
                    }
                }
            }
            SelectObject(dc, old);
            let _ = DeleteObject(pen.into());
            let _ = DeleteObject(red.into());
        }
    }

    extern "system" fn overlay_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        // SAFETY: our own window's messages
        unsafe {
            match m {
                WM_TIMER if w.0 == LOOK_TIMER => {
                    look();
                    LRESULT(0)
                }
                WM_TIMER if w.0 == HOVER_TIMER => {
                    rested();
                    LRESULT(0)
                }
                WM_ERASEBKGND => LRESULT(1),
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    let dc = BeginPaint(h, &mut ps);
                    let key = CreateSolidBrush(KEY);
                    let mut all = RECT::default();
                    let _ = GetClientRect(h, &mut all);
                    FillRect(dc, &all, key);
                    let _ = DeleteObject(key.into());
                    with(|st| paint_marks(st, dc, (st.at.left, st.at.top)));
                    let _ = EndPaint(h, &ps);
                    LRESULT(0)
                }
                _ => DefWindowProcW(h, m, w, l),
            }
        }
    }

    extern "system" fn popup_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        // SAFETY: our own window's messages
        unsafe {
            match m {
                // a click on it never takes the focus from his box
                WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
                WM_MOUSEMOVE => {
                    // told when the pointer leaves the card
                    let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 };
                    let _ = TrackMouseEvent(&mut t);
                    let y = ((l.0 >> 16) & 0xFFFF) as i16 as i32;
                    let changed = with(|st| {
                        let row = (y / st.row.max(1)).max(0) as usize;
                        let new = item_of(row, st.head, st.items.len()).map(|_| row);
                        let c = new != st.hover;
                        st.hover = new;
                        c
                    });
                    if changed == Some(true) {
                        let _ = InvalidateRect(Some(h), None, false);
                    }
                    LRESULT(0)
                }
                WM_MOUSELEAVE => {
                    // off the card: it goes, unless the pointer went back onto its word
                    with(|st| {
                        let p = pointer();
                        if st.card.is_some() && mark_at(st, p) != st.card && !in_card(st, p) {
                            hide_card(st);
                        } else if st.hover.take().is_some() {
                            let _ = InvalidateRect(Some(h), None, false);
                        }
                    });
                    LRESULT(0)
                }
                WM_LBUTTONUP => {
                    let y = ((l.0 >> 16) & 0xFFFF) as i16 as i32;
                    pick(y);
                    LRESULT(0)
                }
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    let dc = BeginPaint(h, &mut ps);
                    with(|st| {
                        let mut all = RECT::default();
                        let _ = GetClientRect(h, &mut all);
                        let bg = CreateSolidBrush(COLORREF(GetSysColor(COLOR_WINDOW)));
                        FillRect(dc, &all, bg);
                        let _ = DeleteObject(bg.into());
                        let old = SelectObject(dc, st.font.into());
                        SetBkMode(dc, TRANSPARENT);
                        let pad = (10.0 * st.scale) as i32;
                        let (word, plain) = card_head(st);
                        let plain = plain.or(if st.head { st.items.first().cloned() } else { None });
                        let (ww, space, _) = widths(st, &word, &[]);
                        let rows = st.items.len() + !st.head as usize;
                        for row in 0..rows {
                            let mut r = RECT { left: 0, top: row as i32 * st.row, right: all.right, bottom: (row as i32 + 1) * st.row };
                            let lit = st.hover == Some(row);
                            if lit {
                                let hb = CreateSolidBrush(COLORREF(GetSysColor(COLOR_HIGHLIGHT)));
                                FillRect(dc, &r, hb);
                                let _ = DeleteObject(hb.into());
                            }
                            let colour = |plain: bool| COLORREF(if lit { GetSysColor(COLOR_HIGHLIGHTTEXT) } else if plain { GetSysColor(COLOR_WINDOWTEXT) } else { dim().0 });
                            r.left += pad;
                            if row == 0 {
                                // the word struck through, then the best candidate, plain; with no candidate the
                                // word as it is and, dim, that no word is near it
                                SelectObject(dc, if plain.is_some() { st.strike } else { st.font }.into());
                                SetTextColor(dc, colour(plain.is_none()));
                                let mut t: Vec<u16> = word.encode_utf16().collect();
                                DrawTextW(dc, &mut t, &mut r, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
                                SelectObject(dc, st.font.into());
                                r.left += ww + space;
                                SetTextColor(dc, colour(plain.is_some()));
                                let mut t: Vec<u16> = plain.as_deref().unwrap_or(NONE_NEAR).encode_utf16().collect();
                                DrawTextW(dc, &mut t, &mut r, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
                                continue;
                            }
                            let Some(i) = item_of(row, st.head, st.items.len()) else { continue };
                            let t = &st.items[i];
                            // "Add to dictionary" under a thin line (paper-research N-487.p58)
                            if t == ADD {
                                let line = CreateSolidBrush(COLORREF(GetSysColor(COLOR_3DSHADOW)));
                                let th = st.scale.round().max(1.0) as i32;
                                FillRect(dc, &RECT { left: pad / 2, top: r.top, right: all.right - pad / 2, bottom: r.top + th }, line);
                                let _ = DeleteObject(line.into());
                            }
                            SetTextColor(dc, colour(false));
                            let mut t: Vec<u16> = t.encode_utf16().collect();
                            DrawTextW(dc, &mut t, &mut r, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
                        }
                        SelectObject(dc, old);
                    });
                    let _ = EndPaint(h, &ps);
                    LRESULT(0)
                }
                _ => DefWindowProcW(h, m, w, l),
            }
        }
    }

    /// His click at Y on the card: a candidate typed in place of the word, or the word kept.
    fn pick(y: i32) {
        let chosen = with(|st| {
            let i = item_of((y / st.row.max(1)).max(0) as usize, st.head, st.items.len())?;
            let item = st.items.get(i)?.clone();
            let m = st.marks.get(st.card?)?;
            Some((item, m.word.clone(), m.was.clone(), m.range.clone(), st.el.as_ref()?.1.clone()))
        })
        .flatten();
        let Some((item, word, was, range, tp)) = chosen else { return };
        // Undo types back what the word was
        let typed = match (&*item, was) {
            (UNDO, Some(w)) => w,
            _ => item.clone(),
        };
        // only a word's letters go in: no Enter, no Tab, nothing of the clipboard
        if item != ADD && !typed.chars().all(|c| c.is_alphabetic() || matches!(c, '\'' | '-' | ' ' | '\u{2019}')) {
            hide_and_look();
            return;
        }
        if item == ADD {
            with(|st| {
                st.kept.lock().unwrap().insert(word.to_lowercase());
                let _ = st.added.send(word.clone());
                if let Some(w) = &st.wake {
                    w();
                }
            });
        } else {
            // SAFETY: UI Automation on this thread's apartment; the keys go to the window with the focus, his box
            unsafe {
                let still = range.GetText(word.encode_utf16().count() as i32 + 1).is_ok_and(|t| t.to_string() == word);
                if !still || with(|st| still_his(st)) != Some(true) || range.Select().is_err() {
                    hide_and_look();
                    return;
                }
                // what is selected is that word, or nothing is typed
                let sel = tp.GetSelection().ok().and_then(|s| s.GetElement(0).ok()).and_then(|r| r.GetText(word.len() as i32 + 1).ok());
                if sel.map(|t| t.to_string()).as_deref() != Some(word.as_str()) {
                    hide_and_look();
                    return;
                }
                // checked again at the last moment: keys never land in another program
                let typed_in = match with(|st| (still_his(st), st.keys_to)) {
                    Some((true, to)) if !to.is_invalid() => {
                        type_to(to, &typed);
                        true
                    }
                    Some((true, _)) => {
                        type_text(&typed);
                        true
                    }
                    _ => false,
                };
                if typed_in {
                    with(|st| {
                        if item == UNDO {
                            // put back: not marked again in this box, and no longer a changed word
                            if let Some((b, _)) = &st.el {
                                st.chosen.push((b.clone(), typed.to_lowercase()));
                                if st.chosen.len() > KEEP {
                                    st.chosen.remove(0);
                                }
                            }
                            st.changed.retain(|c| !(c.now == word && c.range == range));
                        } else {
                            st.just = Some((typed.clone(), word.clone()));
                        }
                    });
                }
            }
        }
        hide_and_look();
    }

    /// The test's typing: TEXT as character messages to window TO, on a desktop that takes no keys.
    fn type_to(to: HWND, text: &str) {
        for u in text.encode_utf16() {
            // SAFETY: a message to the test's own stand-in
            unsafe {
                let _ = SendMessageW(to, WM_CHAR, Some(WPARAM(u as usize)), Some(LPARAM(1)));
            }
        }
    }

    /// TEXT typed as keys of its characters (his program takes them as typing: its own undo).
    fn type_text(text: &str) {
        let mut inputs = Vec::new();
        for u in text.encode_utf16() {
            for up in [false, true] {
                inputs.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: u, dwFlags: if up { KEYEVENTF_UNICODE | KEYEVENTF_KEYUP } else { KEYEVENTF_UNICODE }, time: 0, dwExtraInfo: 0 } },
                });
            }
        }
        // SAFETY: the inputs outlive the call
        unsafe {
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }

    /// How long one look takes, for the cost line (the test).
    #[cfg(test)]
    pub(super) fn timed(el: &IUIAutomationElement) -> (Look, f64) {
        let t = std::time::Instant::now();
        let l = look_at(el);
        (l, t.elapsed().as_secs_f64() * 1e3)
    }

    #[cfg(test)]
    pub(super) fn changed_words() -> Vec<(String, String)> {
        with(|st| st.marks.iter().filter_map(|m| Some((m.word.clone(), m.was.clone()?))).collect()).unwrap_or_default()
    }

    #[cfg(test)]
    pub(super) fn marked_words() -> Vec<String> {
        with(|st| st.marks.iter().map(|m| m.word.clone()).collect()).unwrap_or_default()
    }

    /// The test's look at EL, step by step, lengths and counts only: where its marks were lost.
    #[cfg(test)]
    pub(super) fn why_for_test(el: &IUIAutomationElement) -> String {
        // SAFETY: UI Automation on this thread's apartment
        unsafe {
            let mut o = String::new();
            let pid = el.CurrentProcessId().unwrap_or(-1);
            o += &format!("pid {pid} (me {}), elevated {}, password {:?}, class {:?}; ", GetCurrentProcessId(), elevated(pid), el.CurrentIsPassword().map(|b| b.as_bool()), el.CurrentClassName().map(|c| c.to_string()));
            let Ok(tp) = el.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) else { return o + "no TextPattern" };
            let Ok(vis) = tp.GetVisibleRanges() else { return o + "no visible ranges" };
            let n = vis.Length().unwrap_or(0);
            o += &format!("{n} visible ranges; ");
            for i in 0..n {
                let Ok(v) = vis.GetElement(i) else { continue };
                let text = v.GetText(READ).map(|t| t.to_string()).unwrap_or_default();
                let units = marked_units(&text, &HashSet::new(), 10);
                o += &format!("range {i}: {} chars, {} marked; ", text.chars().count(), units.len());
                let cursor = v.Clone().unwrap();
                let _ = cursor.MoveEndpointByRange(TextPatternRangeEndpoint_End, &cursor, TextPatternRangeEndpoint_Start);
                for (r, start, len, _) in units.into_iter().take(3) {
                    let w = cursor.Clone().unwrap();
                    let moved = w.MoveEndpointByUnit(TextPatternRangeEndpoint_End, TextUnit_Character, start + len);
                    let _ = w.MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, start);
                    let got = w.GetText(len + 1).map(|t| t.to_string()).unwrap_or_default();
                    o += &format!("[{}..{} moved {moved:?}, read back same {}, rects {}] ", start, start + len, got == text[r], rects(&w).len());
                }
            }
            o
        }
    }

    /// The test's click on item ITEM of the card for mark MARK, its keys as messages to TO.
    #[cfg(test)]
    pub(super) fn pick_for_test(mark: usize, item: usize, to: HWND) -> Vec<String> {
        let (row, items, head) = with(|st| {
            st.card = Some(mark);
            st.keys_to = to;
            show_card(st);
            (st.row, st.items.clone(), st.head)
        })
        .unwrap();
        let r = if head { item } else { item + 1 };
        pick(r as i32 * row + row / 2);
        items
    }

    /// The test's still: window TOP as it paints itself, the marks drawn on it as the marks' window draws them, and
    /// the card for mark MARK where it shows; a 24-bit BMP at PATH.
    #[cfg(test)]
    pub(super) fn still_for_test(top: HWND, mark: usize, path: &str) {
        use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
        // SAFETY: bitmaps and DCs of this function's, released here
        unsafe {
            let mut wr = RECT::default();
            let _ = GetWindowRect(top, &mut wr);
            with(|st| {
                st.card = Some(mark);
                show_card(st);
            });
            let mut pr = RECT::default();
            let popup = with(|st| st.popup).unwrap();
            // painted now: a stand-in in this process lets no message through before the shot (one in another process
            // does, while UI Automation waits on it)
            let _ = UpdateWindow(popup);
            let _ = GetWindowRect(popup, &mut pr);
            let all = RECT { left: wr.left.min(pr.left), top: wr.top.min(pr.top), right: wr.right.max(pr.right), bottom: wr.bottom.max(pr.bottom) };
            let (w, h) = (all.right - all.left, all.bottom - all.top);
            let screen = GetDC(None);
            let dc = CreateCompatibleDC(Some(screen));
            let bmp = CreateCompatibleBitmap(screen, w, h);
            let old = SelectObject(dc, bmp.into());
            let white = CreateSolidBrush(COLORREF(0x00FFFFFF));
            FillRect(dc, &RECT { left: 0, top: 0, right: w, bottom: h }, white);
            let _ = DeleteObject(white.into());
            let scale = with(|st| st.scale).unwrap_or(1.0);
            let part = |win: HWND, r: RECT| {
                // a window that is not DPI-aware paints itself at 96 dpi, and the screen shows it scaled up: so here
                let unaware = GetAwarenessFromDpiAwarenessContext(GetWindowDpiAwarenessContext(win)) == DPI_AWARENESS_UNAWARE;
                let k = if unaware { scale } else { 1.0 };
                let (pw, ph) = (((r.right - r.left) as f32 / k).round() as i32, ((r.bottom - r.top) as f32 / k).round() as i32);
                let pdc = CreateCompatibleDC(Some(screen));
                let pb = CreateCompatibleBitmap(screen, pw, ph);
                let po = SelectObject(pdc, pb.into());
                let _ = PrintWindow(win, pdc, PRINT_WINDOW_FLAGS(2));
                SetStretchBltMode(dc, HALFTONE);
                let _ = StretchBlt(dc, r.left - all.left, r.top - all.top, r.right - r.left, r.bottom - r.top, Some(pdc), 0, 0, pw, ph, SRCCOPY);
                SelectObject(pdc, po);
                let _ = DeleteObject(pb.into());
                let _ = DeleteDC(pdc);
            };
            part(top, wr);
            with(|st| paint_marks(st, dc, (all.left, all.top)));
            part(popup, pr);
            // the bitmap's rows, bottom up, as a BMP file
            let stride = ((w * 3 + 3) / 4) * 4;
            let mut bits = vec![0u8; (stride * h) as usize];
            let mut bi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: h, biPlanes: 1, biBitCount: 24, ..Default::default() }, ..Default::default() };
            SelectObject(dc, old);
            GetDIBits(dc, bmp, 0, h as u32, Some(bits.as_mut_ptr() as *mut c_void), &mut bi, DIB_RGB_COLORS);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(dc);
            ReleaseDC(None, screen);
            let mut f = Vec::new();
            let size = 54 + bits.len() as u32;
            f.extend(b"BM");
            f.extend(size.to_le_bytes());
            f.extend([0u8; 4]);
            f.extend(54u32.to_le_bytes());
            f.extend(40u32.to_le_bytes());
            f.extend(w.to_le_bytes());
            f.extend(h.to_le_bytes());
            f.extend(1u16.to_le_bytes());
            f.extend(24u16.to_le_bytes());
            f.extend([0u8; 24]);
            f.extend(bits);
            std::fs::write(path, f).unwrap();
        }
    }

    /// The test's stand-in is a window of this process, on a desktop with no foreground: looked at all the same.
    #[cfg(test)]
    pub(super) fn uia_for_test() -> IUIAutomation {
        with(|st| {
            st.test = true;
            st.uia.clone()
        })
        .unwrap()
    }
}

/// The most words marked in one visible range.
#[cfg(windows)]
const MOST_MARKS: usize = 40;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marked_word_is_counted_in_utf16_units_as_ui_automation_counts() {
        if !crate::spell::available() {
            return;
        }
        let t = "naïve 😀 tihnk";
        let m = marked_units(t, &HashSet::new(), 10);
        let (r, start, len, _) = m.iter().find(|(r, _, _, _)| &t[r.clone()] == "tihnk").cloned().unwrap();
        assert_eq!(&t[r], "tihnk");
        // ï is one unit, 😀 two
        assert_eq!((start, len), (9, 5));
    }

    #[test]
    fn the_popup_goes_under_the_word_and_above_it_at_the_screens_foot() {
        let area = (0, 0, 1920, 1040);
        assert_eq!(popup_at((100, 200, 50, 18), (160, 120), area, 4), (100, 222));
        // no room below: above
        assert_eq!(popup_at((100, 1000, 50, 18), (160, 120), area, 4), (100, 876));
        // kept inside sideways
        assert_eq!(popup_at((1850, 200, 50, 18), (160, 120), area, 4), (1760, 222));
    }

    #[test]
    fn the_card_stays_while_the_pointer_is_on_the_word_or_between_it_and_the_card() {
        let word = [(100, 200, 150, 218)];
        assert!(on_word(&word, 3, (120, 210)));
        // in the gap above the card, under the wave
        assert!(on_word(&word, 3, (120, 220)));
        assert!(!on_word(&word, 3, (120, 221)));
        assert!(!on_word(&word, 3, (150, 210)));
        assert!(!on_word(&word, 3, (99, 210)));
    }

    #[test]
    fn the_cards_first_row_is_the_best_candidate_and_without_one_no_item() {
        // best, two more, Add to dictionary
        assert_eq!(item_of(0, true, 4), Some(0));
        assert_eq!(item_of(3, true, 4), Some(3));
        assert_eq!(item_of(4, true, 4), None);
        // no candidate: the word alone, then Add to dictionary
        assert_eq!(item_of(0, false, 1), None);
        assert_eq!(item_of(1, false, 1), Some(0));
    }

    #[test]
    fn the_wave_spans_the_word() {
        let w = wavy(10, 30, 50, 1.0);
        assert_eq!(w.first().unwrap().0, 10);
        assert_eq!(w.last().unwrap().0, 30);
        assert!(w.iter().all(|(_, y)| (49..=51).contains(y)));
    }

    #[test]
    fn off_until_he_turns_it_on() {
        assert!(!DeskMarks::default().on);
    }

    /// On Windows, on a desktop of its own (nothing on the screen): a RichEdit of this process and a WPF TextBox of
    /// another (PowerShell's) get their misspelled word marked, a candidate clicked goes in in place of the word, the
    /// password boxes are not read; stills of the marks and the popup, and one look's cost, in DIR (DESK_MARKS_TRY,
    /// else C:\yaaihh-try-edge8).
    /// `cargo test --target x86_64-pc-windows-gnu desk_marks -- --ignored --nocapture`
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn on_a_hidden_desktop_a_richedit_and_a_wpf_box_are_marked_and_corrected_and_a_password_box_is_not_read() {
        use windows::Win32::Foundation::*;
        use windows::Win32::System::LibraryLoader::LoadLibraryW;
        use windows::Win32::System::StationsAndDesktops::*;
        use windows::Win32::System::Threading::*;
        use windows::Win32::System::Variant::VARIANT;
        use windows::Win32::UI::Accessibility::*;
        use windows::Win32::UI::WindowsAndMessaging::*;
        use windows::core::*;
        if !crate::spell::available() {
            return;
        }
        let dir = std::env::var("DESK_MARKS_TRY").unwrap_or_else(|_| "C:\\yaaihh-try-edge8".into());
        std::fs::create_dir_all(&dir).unwrap();
        let ready = format!("{dir}\\wpf.ready");
        let _ = std::fs::remove_file(&ready);
        // the WPF stand-in, its own made-up text
        let script = format!(
            "Add-Type -AssemblyName PresentationFramework, PresentationCore\n[System.Windows.Media.RenderOptions]::ProcessRenderMode = 'SoftwareOnly'\n$w = New-Object System.Windows.Window; $w.Title = 'stand-in wpf'; $w.Left = 100; $w.Top = 450; $w.Width = 500; $w.Height = 160\n$sp = New-Object System.Windows.Controls.StackPanel\n$tb = New-Object System.Windows.Controls.TextBox; $tb.Text = 'A wpf box where a wrod is wrong.'; $tb.FontSize = 16\n$pb = New-Object System.Windows.Controls.PasswordBox; $pb.Password = 'secretpassw0rd'\n[void]$sp.Children.Add($tb); [void]$sp.Children.Add($pb); $w.Content = $sp\n$w.Add_ContentRendered({{ [void]$tb.Focus(); $h = (New-Object System.Windows.Interop.WindowInteropHelper $w).Handle; \"$h\" | Set-Content '{ready}' }})\n[void]$w.ShowDialog()\n"
        );
        std::fs::write(format!("{dir}\\wpf-host.ps1"), script).unwrap();
        // SAFETY: a desktop of the test's own, its windows on threads of the test's and in the stand-in it starts
        unsafe {
            let name = "yaaihh-test-desk-marks";
            let d = CreateDesktopW(&HSTRING::from(name), PCWSTR::null(), None, DESKTOP_CONTROL_FLAGS(0), 0x10000000, None).unwrap();
            let dd = d.0 as isize;
            let mut desk: Vec<u16> = format!("WinSta0\\{name}").encode_utf16().chain(Some(0)).collect();
            let mut cmd: Vec<u16> = format!("powershell.exe -NoProfile -ExecutionPolicy Bypass -File {dir}\\wpf-host.ps1").encode_utf16().chain(Some(0)).collect();
            let si = STARTUPINFOW { cb: std::mem::size_of::<STARTUPINFOW>() as u32, lpDesktop: PWSTR(desk.as_mut_ptr()), ..Default::default() };
            let mut pi = PROCESS_INFORMATION::default();
            CreateProcessW(PCWSTR::null(), Some(PWSTR(cmd.as_mut_ptr())), None, None, false, CREATE_NO_WINDOW, None, PCWSTR::null(), &si, &mut pi).unwrap();
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                SetThreadDesktop(HDESK(dd as _)).unwrap();
                LoadLibraryW(w!("msftedit.dll")).unwrap();
                let top = CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!("stand-in"), WS_OVERLAPPEDWINDOW | WS_VISIBLE, 100, 100, 600, 300, None, None, None, None).unwrap();
                let rich = CreateWindowExW(WINDOW_EX_STYLE(0), w!("RICHEDIT50W"), w!("A richedit box with a mispeled word."), WS_CHILD | WS_VISIBLE | WS_BORDER | WINDOW_STYLE(4), 10, 10, 560, 100, Some(top), None, None, None).unwrap();
                let pw = CreateWindowExW(WINDOW_EX_STYLE(0), w!("EDIT"), w!("secretpassw0rd"), WS_CHILD | WS_VISIBLE | WS_BORDER | WINDOW_STYLE(0x20), 10, 150, 300, 24, Some(top), None, None, None).unwrap();
                // another box, looked at between: what he replaced in the first stays with it
                let other = CreateWindowExW(WINDOW_EX_STYLE(0), w!("RICHEDIT50W"), w!("Anothr box."), WS_CHILD | WS_VISIBLE | WS_BORDER | WINDOW_STYLE(4), 330, 150, 240, 24, Some(top), None, None, None).unwrap();
                tx.send((top.0 as isize, rich.0 as isize, pw.0 as isize, other.0 as isize)).unwrap();
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            });
            let (top, rich, pw, other) = rx.recv().unwrap();
            let mut wpf = 0isize;
            for _ in 0..300 {
                if let Some(h) = std::fs::read_to_string(&ready).ok().and_then(|t| t.trim().parse().ok()) {
                    wpf = h;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            assert!(wpf != 0, "the WPF stand-in did not start");
            let (rtx, rrx) = std::sync::mpsc::channel();
            let dir2 = dir.clone();
            std::thread::spawn(move || {
                SetThreadDesktop(HDESK(dd as _)).unwrap();
                let (added, _) = std::sync::mpsc::channel();
                win::make(Default::default(), added, None).unwrap();
                let uia = win::uia_for_test();
                let text = |el: &IUIAutomationElement| el.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId).unwrap().DocumentRange().unwrap().GetText(-1).unwrap().to_string();
                let mut out = Vec::new();
                let mut undo_line = None;
                // the RichEdit
                let el = uia.ElementFromHandle(HWND(rich as _)).unwrap();
                let (look, ms) = win::timed(&el);
                let words = win::marked_words();
                out.push(format!("richedit: {look:?} {words:?} in {ms:.1} ms"));
                if words.is_empty() {
                    // what went wrong: the checker on this thread, and the steps of the look
                    let t = "A richedit box with a mispeled word.";
                    let m: Vec<&str> = crate::spell::misspelled(t, &Default::default()).into_iter().map(|(r, _)| &t[r]).collect();
                    out.push(format!("  checker on this thread: available {} marks {m:?}; {}", crate::spell::available(), win::why_for_test(&el)));
                } else {
                    let i = words.iter().position(|w| w == "mispeled").unwrap_or(0);
                    win::still_for_test(HWND(top as _), i, &format!("{dir2}\\still-richedit.bmp"));
                    let items = win::pick_for_test(i, 0, HWND(rich as _));
                    out.push(format!("richedit: offered {items:?}; after the first: {:?}", text(&el)));
                    // the word he replaced: dotted, its card offers Undo, which puts the word back and leaves it unmarked;
                    // it stays with its box while he looks at another one
                    win::look_at(&el);
                    win::look_at(&uia.ElementFromHandle(HWND(other as _)).unwrap());
                    let elsewhere = win::changed_words();
                    win::look_at(&el);
                    let changed = win::changed_words();
                    let undo = match win::marked_words().iter().position(|w| changed.iter().any(|(now, _)| now == w)) {
                        Some(j) => {
                            win::still_for_test(HWND(top as _), j, &format!("{dir2}\\still-richedit-changed.bmp"));
                            let items = win::pick_for_test(j, 0, HWND(rich as _));
                            let back = text(&el);
                            win::look_at(&el);
                            win::look_at(&uia.ElementFromHandle(HWND(other as _)).unwrap());
                            win::look_at(&el);
                            format!("richedit changed {changed:?} (in the other box {elsewhere:?}): offered {items:?}; after Undo: {back:?}; marked then, after the other box again, {:?}", win::marked_words())
                        }
                        None => format!("richedit changed: none ({changed:?})"),
                    };
                    undo_line = Some(undo);
                }
                // its password box
                out.push(format!("password EDIT: {:?}", win::look_at(&uia.ElementFromHandle(HWND(pw as _)).unwrap())));
                // the WPF TextBox and PasswordBox, in the other process
                let w = uia.ElementFromHandle(HWND(wpf as _)).unwrap();
                let edit = uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_EditControlTypeId.0)).unwrap();
                let tb = w.FindFirst(TreeScope_Descendants, &uia.CreateAndCondition(&edit, &uia.CreatePropertyCondition(UIA_IsPasswordPropertyId, &VARIANT::from(false)).unwrap()).unwrap()).unwrap();
                let pb = w.FindFirst(TreeScope_Descendants, &uia.CreatePropertyCondition(UIA_IsPasswordPropertyId, &VARIANT::from(true)).unwrap()).unwrap();
                let (look, ms) = win::timed(&tb);
                let words = win::marked_words();
                out.push(format!("wpf: {look:?} {words:?} in {ms:.1} ms"));
                if words.is_empty() {
                    out.push(format!("  {}", win::why_for_test(&tb)));
                } else {
                    let i = words.iter().position(|w| w == "wrod").unwrap_or(0);
                    win::still_for_test(HWND(wpf as _), i, &format!("{dir2}\\still-wpf.bmp"));
                    let items = win::pick_for_test(i, 0, HWND(wpf as _));
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    out.push(format!("wpf: offered {items:?}; after the first: {:?}", text(&tb)));
                }
                out.push(format!("wpf PasswordBox: {:?}", win::look_at(&pb)));
                // the cost: twenty looks at the RichEdit
                let t = std::time::Instant::now();
                for _ in 0..20 {
                    win::look_at(&el);
                }
                out.push(format!("one look at the RichEdit: {:.1} ms on average of 20", t.elapsed().as_secs_f64() * 1e3 / 20.0));
                out.extend(undo_line);
                win::unmake();
                rtx.send(out).unwrap();
            });
            let out = rrx.recv().unwrap();
            let _ = TerminateProcess(pi.hProcess, 0);
            std::thread::sleep(std::time::Duration::from_millis(300));
            let _ = CloseDesktop(d);
            let all = out.join("\n");
            println!("{all}");
            std::fs::write(format!("{dir}\\desk-marks-test.txt"), &all).unwrap();
            assert!(out[0].starts_with("richedit: Marked(") && out[0].contains("\"mispeled\""), "{all}");
            assert!(out[1].contains("after the first") && !out[1].contains("mispeled"), "{all}");
            assert_eq!(out[2], "password EDIT: Password", "{all}");
            assert!(out[3].starts_with("wpf: Marked(") && out[3].contains("\"wrod\""), "{all}");
            assert!(out[4].contains("after the first") && !out[4].contains("wrod"), "{all}");
            assert_eq!(out[5], "wpf PasswordBox: Password", "{all}");
            assert!(out.get(7).is_some_and(|l| l.contains("changed [(\"") && l.contains("(in the other box [])") && l.contains("[\"Undo\"]") && l.contains("after Undo: \"A richedit box with a mispeled word.") && !l.rsplit("marked then").next().unwrap().contains("mispeled")), "{all}");
        }
    }
}
