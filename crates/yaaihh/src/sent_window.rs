//! What a model was sent, in an OS window of its own (the owner, N-1104 p562: "very big stuff like opening a request
//! what the model was send etc should open in a new wienod opup isntaed of inline ... otheriwse it destor ytge chat
//! and i cnanot fidn hte close at the top again"; paper-research N-1446 p2112, 2; the list and the choice, p2130). A
//! reply's turn's requests ("› what the model was sent" under the work behind it, turn.rs) and a call's prompt in a
//! reading ("What CALL was sent", ticket.rs) open beside the window, in a window of their own: titled with what it is,
//! its id and label; its close is the OS window's own, at its top, and Escape closes it too. The window and the popup it
//! was pressed in stay as they were. One opens a request: a second press brings it to the front. Each closes with the
//! window (to the tray), so none outlives it (p2134). Each is drawn in the window's theme, and a change of it reaches
//! those open ([`restyle`], p2166).
//!
//! A window draws and takes presses on a state of its own (rune `open_window`), so this one holds what it shows: a
//! turn's requests copied whole once read (the turn has ended: they no longer change), drawn by raw.rs's own rows,
//! their folds its own ([`raw::Holds`]); a call's prompt as the reading drew it (ticket.rs `boxed`). It offers Comment
//! where the popup it was opened from does (popup.rs `goes_to`: a thread's reading, not a turn's work): a passage
//! selected in it is carried to the window (restart.rs `to_app`), and its box opens there at that popup's foot, for
//! where its comments go (popup.rs `comment_from`).
//!
//! The other big views stay in the popup (popup.rs): each reads the window's `App`, and its presses go there.

use crate::popup::What;
use crate::raw::{self, Holds, Log};
use crate::App;
use coord_types::look::Look;
use rune::{AppSpec, Cx, El, Key, KeyEvent, ScrollSpec, Tx, WindowSpec, col, text};
use rune_kit::Kit;
use std::collections::HashMap;

/// A window of its own: its title, what it shows, the look it is drawn in (the window's, kept as it changes), and the
/// popup it was opened from, by its title, when a comment on it goes somewhere (its Comment).
#[derive(Default)]
pub struct SentWindow {
    pub title: String,
    pub shows: Shows,
    dark: Option<bool>,
    look: Option<Look>,
    from: Option<(String, What)>,
    /// Whether the OS window's title is set (its first frame).
    titled: bool,
    /// Its key and number, for its close to take it out of the window's list ([`closed`]).
    key: String,
    n: u64,
}

/// What a window of its own shows.
pub enum Shows {
    /// The requests of a turn held as NAME in project PI, from SINCE (raw.rs `requests`).
    Turn { pi: usize, name: String, since: u64, log: Log },
    /// A text whole: a call's prompt.
    Text(String),
}

impl Default for Shows {
    fn default() -> Self {
        Shows::Text(String::new())
    }
}

impl Holds for SentWindow {
    fn log_mut(&mut self, _: usize, _: &str) -> Option<&mut Log> {
        match &mut self.shows {
            Shows::Turn { log, .. } => Some(log),
            Shows::Text(_) => None,
        }
    }
}

/// The windows of their own open, by key, one a request; the theme they were given; the turns pressed whose requests
/// are still being read.
#[derive(Default)]
pub struct Sent {
    windows: HashMap<String, Open>,
    /// The window's theme its windows of their own were last given ([`restyle`]).
    theme: (Option<bool>, Option<Look>),
    /// The number the next window opened takes.
    next: u64,
    /// "› what the model was sent" pressed before its turn's requests were read: opened when they are ([`poll`]).
    waiting: Vec<Turn>,
}

/// Agent AGENT's turn behind POST in project PI, from SINCE, whose window was asked for: titled TITLE, from popup FROM
/// (its title and what).
#[derive(Clone, PartialEq)]
struct Turn {
    pi: usize,
    agent: String,
    post: String,
    since: u64,
    title: String,
    from: Option<(String, What)>,
}

/// A window of its own open: its number, so its close takes out its own entry and never that of a later window of the
/// same key ([`closed`]); its OS window, brought to the front and closed; its app, given the window's theme.
struct Open {
    n: u64,
    window: rune::WindowHandle,
    app: rune::AppHandle<SentWindow>,
}

impl Turn {
    fn key(&self) -> String {
        format!("sent-turn-{}-{}-{}", self.pi, self.agent, self.post)
    }
}

/// Open the requests of agent AGENT's turn behind POST in project PI (SINCE to UNTIL, turn.rs) in a window of their own,
/// titled TITLE, pressed in popup FROM (its title and what): at once when they are read, else as they come ([`poll`]);
/// its window brought to the front when it is open.
#[allow(clippy::too_many_arguments)]
pub fn open_turn(s: &mut App, pi: usize, agent: &str, post: &str, since: u64, until: u64, title: String, from: Option<(String, What)>) {
    raw::open_turn(s, pi, agent, post, since, until);
    let t = Turn { pi, agent: agent.into(), post: post.into(), since, title, from };
    if !opened(s, &t) && !s.sent.waiting.contains(&t) {
        s.sent.waiting.push(t);
    }
}

/// Whether agent AGENT's turn behind POST in project PI waits for its requests to open its window ([`open_turn`]).
pub fn waits(s: &App, pi: usize, agent: &str, post: &str) -> bool {
    s.sent.waiting.iter().any(|t| t.pi == pi && t.agent == agent && t.post == post)
}

/// Whether T's window is open now: brought to the front, or opened on its requests as read; false while they are not.
fn opened(s: &mut App, t: &Turn) -> bool {
    let key = t.key();
    if front(s, &key) {
        return true;
    }
    let Some(w) = of_turn(s, t) else { return false };
    open(s, key, w);
    true
}

/// T's window as it opens: its requests as read, copied whole; none while they are not.
fn of_turn(s: &App, t: &Turn) -> Option<SentWindow> {
    let log = raw::turn_log(s, t.pi, &t.agent, &t.post).filter(|l| l.loaded)?;
    let shows = Shows::Turn { pi: t.pi, name: raw::turn_name(&t.agent, &t.post), since: t.since, log: log.clone() };
    Some(window(s, t.title.clone(), shows, t.from.clone()))
}

/// The turns waited for whose requests came: their windows opened. One refused is waited for no more: its line says
/// why (turn.rs), and a press asks again. Whether any is waited for no more.
pub fn poll(s: &mut App) -> bool {
    restyle(s);
    if s.sent.waiting.is_empty() {
        return false;
    }
    let before = s.sent.waiting.len();
    for t in std::mem::take(&mut s.sent.waiting) {
        let refused = raw::turn_log(s, t.pi, &t.agent, &t.post).is_some_and(|l| l.refused.is_some());
        if !refused && !opened(s, &t) {
            s.sent.waiting.push(t);
        }
    }
    s.sent.waiting.len() != before
}

/// Open what call I of reading FILE of ticket T was sent, in a window of its own, pressed in that reading's popup.
pub fn open_call(s: &mut App, t: u64, file: &str, i: usize) {
    let key = format!("sent-call-{t}-{file}-{i}");
    if front(s, &key) {
        return;
    }
    if let Some(w) = of_call(s, t, file, i) {
        open(s, key, w);
    }
}

/// The window of what call I of reading FILE of ticket T (the current project's) was sent, as it opens: its prompt as
/// coordd gave the reading (`Forming`), from that reading's popup; none when the reading is no longer held.
fn of_call(s: &App, t: u64, file: &str, i: usize) -> Option<SentWindow> {
    let p = s.proj()?;
    let v = p.live.tickets.get(&t)?;
    let r = s.tickets.forming.get(&t)?.as_ref().ok()?.iter().find(|r| r.file == file)?;
    let call = r.calls.get(i)?;
    let title = crate::popup::title(&crate::refs::ticket(Some(p), v).label, &format!("{}.s{}", v.id, r.step), &format!("what {} was sent", call.call));
    let from = (crate::ticket::reading_title(p, v, r), What::Reading { t, file: file.into() });
    Some(window(s, title, Shows::Text(call.prompt.clone()), Some(from)))
}

/// Bring the window keyed KEY to the front, when it is open; one still starting comes to the front as it starts (its
/// handle keeps the request until then).
fn front(s: &App, key: &str) -> bool {
    let Some(o) = s.sent.windows.get(key) else { return false };
    o.window.focus();
    true
}

/// A window titled TITLE showing SHOWS, from popup FROM, in the window's look; commented on as FROM is, when FROM is.
fn window(s: &App, title: String, shows: Shows, from: Option<(String, What)>) -> SentWindow {
    let from = from.filter(|(_, w)| crate::popup::goes_to(s, w).is_some());
    SentWindow { title, shows, dark: s.dark, look: crate::look::of(s).cloned(), from, ..Default::default() }
}

/// Open W, keyed KEY, beside the window.
fn open(s: &mut App, key: String, w: SentWindow) {
    let n = s.sent.next;
    s.sent.next += 1;
    let w = SentWindow { key: key.clone(), n, ..w };
    let app = AppSpec { poll: Some(title_once), on_key: Some(on_key), families: ("Inter", "ui-monospace"), ..AppSpec::new("yaaihh", (1100.0, 820.0), SentWindow::default, view) };
    let app = rune::App::new(app).on_close(closed);
    let handle = app.handle();
    let window = rune::open_window(WindowSpec::from_app(key.clone(), app).state(w));
    s.sent.windows.insert(key, Open { n, window, app: handle });
}

/// W closed (its close, Escape, or the window's [`close_all`]): out of the window's list, so a press opens it again.
fn closed(w: &mut SentWindow, _: &mut Tx) {
    let (key, n) = (std::mem::take(&mut w.key), w.n);
    crate::restart::to_app(move |s: &mut App, _| forget(s, &key, n));
}

/// Window N keyed KEY out of the list; a later window of that key stays.
fn forget(s: &mut App, key: &str, n: u64) {
    if s.sent.windows.get(key).is_some_and(|o| o.n == n) {
        s.sent.windows.remove(key);
    }
}

/// Every window of its own closed, and no turn waited for: the window closed (to the tray, lib.rs), and nothing it
/// opened outlives it (paper-research N-1446 p2134).
pub fn close_all(s: &mut App) {
    for o in std::mem::take(&mut s.sent.windows).into_values() {
        o.window.close();
    }
    s.sent.waiting.clear();
}

/// The window's theme (its light or dark, its look), changed since its windows of their own were given it: given to
/// each open (paper-research N-1446 p2166). One opened since was drawn in it already; with none open, nothing is read.
fn restyle(s: &mut App) {
    let look = crate::look::of(s);
    if s.sent.windows.is_empty() || (s.sent.theme.0 == s.dark && s.sent.theme.1.as_ref() == look) {
        return;
    }
    let theme = (s.dark, look.cloned());
    for o in s.sent.windows.values() {
        let (dark, look) = theme.clone();
        o.app.update(move |w: &mut SentWindow, _| (w.dark, w.look) = (dark, look));
    }
    s.sent.theme = theme;
}

/// The OS window's title, on its first frame.
fn title_once(w: &mut SentWindow, _: f64, tx: &mut Tx) -> bool {
    if !w.titled {
        tx.set_title(w.title.clone());
        w.titled = true;
    }
    false
}

/// Escape closes the window, as its close does.
fn on_key(_: &mut SentWindow, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if ev.key != Key::Escape {
        return false;
    }
    tx.close_window();
    true
}

/// QUOTE, selected in W and commented on: carried to the window, as a passage of the popup W was opened from.
fn comment(w: &SentWindow, quote: String) {
    let Some((title, from)) = w.from.clone() else { return };
    let label = w.title.clone();
    crate::restart::to_app(move |s: &mut App, _| crate::popup::comment_from(s, &title, &from, &label, &quote));
    crate::restart::front();
}

/// The window's page: its title over what it shows, which scrolls.
pub fn view(w: &SentWindow, cx: &mut Cx<SentWindow>) -> El {
    let dark = w.dark.unwrap_or_else(|| cx.system_dark());
    let mut k = Kit::neutral(dark);
    if let Some(l) = &w.look {
        crate::look::apply(&mut k, l);
    }
    // as the window's (lib.rs `view`)
    cx.selection_color(k.selection());
    let head = text(w.title.clone()).key("sent-title").font(k.ty.body_medium).color(k.c.text_base).wrap().selectable(true);
    let body = match &w.shows {
        Shows::Turn { pi, name, since, log } => raw::requests::<SentWindow>(Some(log), *pi, name, *since, &k),
        Shows::Text(t) => crate::ticket::boxed(t.clone(), &k),
    };
    let body = match &w.from {
        Some(_) => body.selection_group(true).selection_menu_item("comment", "Comment", |w: &mut SentWindow, sel: &rune::TextSelection, _: &mut Tx| comment(w, sel.text().to_string())),
        None => body,
    };
    let scroll = col().key("sent-body").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).px(16.0).pb(16.0).child(body);
    // a page, as the window's is (lib.rs `view`): its words select as one, the title into the text
    let page = col().w_full().h_full().fill(k.c.page).select_texts(true).child(col().w_full().px(16.0).pt(14.0).pb(10.0).child(head)).child(scroll);
    rune_kit::root(page.instant_changes(), Vec::new())
}

/// Whether a window keyed KEY was opened (a test's: rune opens none without its event loop).
#[cfg(test)]
pub(crate) fn asked(s: &App, key: &str) -> bool {
    s.sent.windows.contains_key(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::popup::tests::{chat_app, ticket_app};
    use rune::runtime::Runtime;

    const AGENT: &str = "paper-research";
    const POST: &str = "N-1104.p21";
    const TITLE: &str = "the owner's chat · N-1104.p21 · what paper-research's model was sent";
    const WORK: &str = "the owner's chat · N-1104.p21 · the work behind paper-research's post";

    fn nanos(t: &str) -> u64 {
        crate::turn::nanos(t).unwrap()
    }

    /// The turn behind `chat_app`'s p21, from his prompt at 19:20, as its line in the work's popup asks for it.
    fn turn() -> Turn {
        let work = What::Work { pi: 0, agent: AGENT.into(), post: POST.into(), at: "2026-11-19T19:21:00Z".into() };
        Turn { pi: 0, agent: AGENT.into(), post: POST.into(), since: nanos("2026-11-19T19:20:00Z"), title: TITLE.into(), from: Some((WORK.into(), work)) }
    }

    fn window_of(w: SentWindow) -> Runtime<SentWindow> {
        let mut rt = Runtime::new(w, view, (1100.0, 820.0), rune::Env::new(false, 1.0));
        settle(&mut rt);
        rt
    }

    fn settle(rt: &mut Runtime<SentWindow>) {
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
    }

    /// The rect of the first element whose text holds WORDS.
    fn find(rt: &Runtime<SentWindow>, words: &str) -> Option<[f32; 4]> {
        rt.elements().into_iter().find(|e| !e.exiting && e.text.as_deref().is_some_and(|t| t.contains(words))).map(|e| e.rect)
    }

    fn press(rt: &mut Runtime<SentWindow>, r: [f32; 4]) {
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle(rt);
    }

    /// paper-research N-1446 p2112, 2: a turn's requests in a window of their own, titled with what they are, the id and
    /// the label; each request folded to its head, as in the popup before, and a press on it opens it there, in that
    /// window's copy: the window's own log is as it was.
    #[test]
    fn a_turns_requests_draw_in_a_window_of_their_own_with_folds_of_their_own() {
        let (_held, s) = chat_app();
        let mut rt = window_of(of_turn(&s, &turn()).expect("the turn's requests, read"));
        assert!(rt.elements().into_iter().any(|e| e.text.as_deref() == Some(TITLE)), "its title at its top");
        let head = find(&rt, "Request · 19:20:31 · claude-opus-5-5 · status 200").expect("the request's head");
        assert!(find(&rt, "You are paper-research.").is_none(), "folded to its head as it comes");
        press(&mut rt, head);
        assert!(find(&rt, "You are paper-research.").is_some() && find(&rt, "Post 20").is_some(), "opened in the window");
        let again = window_of(of_turn(&s, &turn()).unwrap());
        assert!(find(&again, "You are paper-research.").is_none(), "the window's log folded as it was: the press was the copy's");
    }

    /// The line pressed before the turn's requests are read waits for them, and its window opens as they come.
    #[test]
    fn a_turn_not_yet_read_opens_its_window_when_its_requests_come() {
        let (_held, mut s) = chat_app();
        let t = turn();
        let held = s.raw.logs.remove(&(0, raw::turn_name(AGENT, POST))).expect("chat_app holds the turn");
        s.sent.waiting.push(t.clone());
        assert!(waits(&s, 0, AGENT, POST));
        assert!(!poll(&mut s), "nothing read yet: it waits");
        assert!(!asked(&s, &t.key()));
        s.raw.logs.insert((0, raw::turn_name(AGENT, POST)), held);
        assert!(poll(&mut s), "read: its window opens");
        assert!(asked(&s, &t.key()) && !waits(&s, 0, AGENT, POST));
    }

    /// The window offers Comment where the popup it was opened from does: a turn's, opened from its work, none (popup.rs
    /// `goes_to`); a call's prompt, from a thread's reading, yes. A passage of it commented on goes where that popup's
    /// comments go, its box at the popup's foot in the window; with the popup closed since, the popup opens again under
    /// the box.
    #[test]
    fn a_comment_on_the_window_goes_where_its_popups_comments_go() {
        // one made-up window's state at a time: each holds the tests' lock
        {
            let (_held, s) = chat_app();
            assert!(of_turn(&s, &turn()).unwrap().from.is_none(), "a turn's work offers no Comment, nor its requests");
        }
        let (_held, mut s, _) = ticket_app();
        s.projects[0].live.tickets.get_mut(&5).unwrap().kind = "thread".into();
        let w = of_call(&s, 5, "tickets/forming/5.s1.1.json", 0).unwrap();
        let (title, from) = w.from.clone().expect("a thread's reading offers Comment");
        crate::popup::open(&mut s, title.as_str(), from.clone());
        crate::popup::comment_from(&mut s, &title, &from, &w.title, " Read the agent's answer. ");
        let p = s.notes.picked.clone().expect("the comment box");
        assert_eq!((p.target.as_str(), p.source.as_str(), p.label.as_str(), p.quote.as_str()), ("thread:T-5", "popup:reading:5:tickets/forming/5.s1.1.json", w.title.as_str(), "Read the agent's answer."));
        assert_eq!(p.anchor.as_deref(), Some("popup0-body"), "at the popup's foot");
        crate::ask::unwrite(&mut s);
        crate::popup::close(&mut s);
        crate::popup::comment_from(&mut s, &title, &from, &w.title, "It is done.");
        assert_eq!(s.popups.len(), 1, "its popup again");
        assert_eq!(s.popups[0].title, title);
        assert_eq!(s.notes.picked.as_ref().and_then(|p| p.anchor.as_deref()), Some("popup0-body"));
    }

    /// paper-research N-1446 p2134: one window a request, brought to the front on a second press, and none outlives the
    /// window: its close closes them all and drops the turns waited for. A window's close takes out its own entry, never
    /// that of a later window of the same key.
    #[test]
    fn one_window_a_request_closed_with_the_window() {
        let (_held, mut s) = chat_app();
        let t = turn();
        assert!(opened(&mut s, &t) && opened(&mut s, &t), "open, then to the front");
        assert_eq!((s.sent.windows.len(), s.sent.next), (1, 1), "a second press opens no second window");
        let first = s.sent.windows[&t.key()].n;
        forget(&mut s, &t.key(), first);
        assert!(!asked(&s, &t.key()), "closed: a press opens it again");
        assert!(opened(&mut s, &t));
        forget(&mut s, &t.key(), first);
        assert!(asked(&s, &t.key()), "the first window's close leaves the second");
        s.sent.waiting.push(Turn { post: "N-1104.p22".into(), ..turn() });
        close_all(&mut s);
        assert!(s.sent.windows.is_empty() && s.sent.waiting.is_empty(), "the window closed: none of them left");
    }

    /// paper-research N-1446 p2166: a change of the window's theme is given to the windows of their own open, and a
    /// window given it is drawn in it.
    #[test]
    fn the_windows_theme_changed_reaches_its_windows_of_their_own() {
        let (_held, mut s) = chat_app();
        s.dark = Some(false);
        assert!(opened(&mut s, &turn()));
        poll(&mut s);
        assert_eq!(s.sent.theme.0, Some(false), "given the window's theme");
        s.dark = Some(true);
        poll(&mut s);
        assert_eq!(s.sent.theme.0, Some(true), "its change given");
        let page = Kit::neutral(true).c.page;
        let drawn = |w: SentWindow| window_of(w).elements().into_iter().any(|e| e.fill == page);
        let mut w = of_turn(&s, &turn()).unwrap();
        w.dark = Some(false);
        assert!(!drawn(w) && drawn(of_turn(&s, &turn()).unwrap()), "drawn in it");
    }

    /// A reading's "What CALL was sent" opens the call's prompt whole in a window of its own, titled with the step.
    #[test]
    fn what_a_call_was_sent_draws_in_a_window_of_its_own() {
        let (_held, s, _) = ticket_app();
        let w = of_call(&s, 5, "tickets/forming/5.s1.1.json", 0).expect("the reading held");
        assert_eq!(w.title, "task · T-5.s1 · what form-head was sent");
        assert!(w.from.is_none(), "a task's reading offers no Comment, nor its call's prompt");
        let rt = window_of(w);
        assert!(find(&rt, "Read the agent's answer.").is_some() && find(&rt, "It is done.").is_some(), "the prompt whole");
        assert!(of_call(&s, 5, "tickets/forming/5.s1.1.json", 1).is_none(), "no such call");
    }

    /// Stills of the windows of their own, on made-up data: `STILL_OUT=DIR yaaihh-tests
    /// sent_window::tests::sent_window_stills --ignored --exact`: `chat_app`'s p21's requests as they open
    /// (sent-window.png) and its request pressed open (sent-window-open.png); `ticket_app`'s call's prompt
    /// (sent-call.png). The popup it is pressed in: popup.rs `popup_stills`.
    #[test]
    #[ignore]
    fn sent_window_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn folded() -> SentWindow {
            of_turn(&chat_app().1, &turn()).unwrap()
        }
        fn opened() -> SentWindow {
            let (_held, mut s) = chat_app();
            raw::unfold_turn(&mut s, 0, AGENT, POST);
            of_turn(&s, &turn()).unwrap()
        }
        fn call() -> SentWindow {
            of_call(&ticket_app().1, 5, "tickets/forming/5.s1.1.json", 0).unwrap()
        }
        let shots: [(&str, fn() -> SentWindow); 3] = [("sent-window", folded), ("sent-window-open", opened), ("sent-call", call)];
        for (name, setup) in shots {
            let snap = out.join(format!("{name}.png")).display().to_string();
            let args: Vec<String> = ["--snap", &snap, "--size", "1100x820"].map(String::from).to_vec();
            let spec = AppSpec { families: ("Inter", "ui-monospace"), ..AppSpec::new("yaaihh", (1100.0, 820.0), setup, view) };
            rune::App::new(spec).args(args).try_run().unwrap();
        }
    }
}
