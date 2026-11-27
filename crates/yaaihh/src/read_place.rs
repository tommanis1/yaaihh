//! Where he was reading, kept across a window swap (W3's named miss, N-412.p523: "The scroll place is still lost";
//! paper-research N-788.p1). For each thread and chat he looked at: the key of the row at the top of its view, or
//! nothing when he was at a chat's end (it sticks to its end by itself). The keys go into ui.json with the rest
//! (state.rs); after a swap, the first time a view is laid out it is scrolled back to that row. That ends when the row
//! is at the top, when the view's rows are in and the row is not among them (gone: a post taken back, a part of a
//! session not held), or when he moves the view himself, which always wins: by the wheel, or by anything else (the
//! scrollbar, a key), seen as the view's offset moving between two polls in which nothing was asked of it
//! (paper-research N-788.p9). It also ends when the row is laid out and putting it at the top moved nothing (the
//! content ends below it, in a taller window). Leaving the view stops it until he returns: a view counts as seen only
//! once its row was reached or he moved in it, so it is brought back then (consult-2, N-586.p43). A thread he left
//! for another place is brought back so too when he comes back to it, on Threads or beside an agent's list: it opens
//! where he last read it, else at its foot (`Places::left`; item 15, paper-research N-1446 p2603). A thread opened at a post (Starred, a
//! reference, a question's post, the outline: `find::open_at`) goes to that post: its kept row is dropped there and
//! nothing is brought back (consult-2, N-586.p37). No time limit: on his bridge a thread's rows can take seconds to
//! come (N-788.p6).
//!
//! A chat's row further back than the page held is read back to (paper-research N-586.p40, p42): the page before what
//! is held is asked (`Proj::read_older`, as scrolling to the top does) until the session held starts at or before the
//! row's offset in its file (the key `e{OFFSET}-{n}`). One page at a time, the next asked only once the last is in
//! and laid out; none once he moves the view (by any means: the row at its top changes, which older rows put in above
//! and the agent's entries growing in at the end do not) or leaves it. At most [`PAGES`] pages: past them the chat stays where it
//! is and says faintly once that the place is further back. A place kept in another session file (the session resumed
//! or compacted since) is dropped: its offset means nothing in the new file.
//!
//! The places are row keys, not pixels: a virtual list measures its rows only as they are drawn, so a pixel offset
//! would land elsewhere in a window of another size or zoom.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How far a view's offset may move between two polls with nothing asked of it before it is his move, in points:
/// what is left of rows being measured after a jump stays under it.
const MOVED: f32 = 2.0;

/// The most pages read back to a chat's kept row: about 2 MB of session back from its end at 512 KB a page, about
/// 6 s of reads at 1.4 s each on his bridge (docket-edge4's proposal, N-586.p41).
pub const PAGES: u32 = 4;

/// The polls a chat's rows must stand unchanged after a page before its offset is his (the rows put in, measured and
/// grown first), and before the next page is asked.
const SETTLE: u32 = 3;

/// The line over a chat whose place is further back than [`PAGES`] pages.
pub const FURTHER: &str = "Scroll up to where you were";

/// A chat being read back to its row.
#[derive(Default)]
struct Reading {
    pages: u32,
    /// What was held at the last poll (entries, start, next), and for how many polls it has stood.
    stamp: Option<(usize, u64, u64)>,
    steady: u32,
    /// The row at the view's top at the last poll ("" at a chat's end): older rows put in above, or the agent's new
    /// entries growing in at the end, leave it as it was; a move of his changes it (paper-research N-586.p48).
    top: Option<String>,
}

/// What is held of a chat's session, for reading back.
#[derive(Clone, Copy)]
pub struct Held<'a> {
    pub session: &'a str,
    pub start: u64,
    pub entries: usize,
    pub next: u64,
    pub loaded: bool,
    pub pending: bool,
}

/// The offset in its session file of the line a chat row's key stands for: `e{OFFSET}-{n}`, a run's `w-…` or a call's
/// `c-…` of it.
fn offset(key: &str) -> Option<u64> {
    let k = key.strip_prefix("w-").or_else(|| key.strip_prefix("c-")).unwrap_or(key);
    k.strip_prefix('e')?.split_once('-')?.0.parse().ok()
}

/// A view being brought back to its row.
pub struct Back {
    pub key: String,
    /// The scope its key is unique in (the list's key): a chat's row keys repeat between agents.
    pub scope: String,
    /// Whether the row was among those laid out at the last report.
    shown: bool,
    asked: u32,
    /// The view's offset as the scroll back last left it (or found it), and whether it asked a scroll at the last
    /// poll (then the next poll only takes the offset that came of it).
    ours: Option<f32>,
    asked_last: bool,
    /// The offset when the last scroll was asked, and whether that scroll put the row at the top.
    before: Option<f32>,
    last_at_top: bool,
}

#[derive(Default)]
pub struct Places {
    /// The row at the top of each view, by view ("thread PROJECT N-12", "chat PROJECT agent"); "" at a chat's end.
    pub kept: HashMap<String, String>,
    /// The row each view last reported at its top.
    top: HashMap<String, String>,
    /// The views seen since the window started: only the first sight of each is brought back (a view left before its
    /// row was back is not counted, nor a thread he left for another place: [`Places::left`]).
    seen: HashSet<String>,
    pub back: HashMap<String, Back>,
    /// Each view's scroll offset as its viewport reports it (`on_scroll_offset`, which has no state to write to).
    pub offsets: Arc<Mutex<HashMap<String, f32>>>,
    /// The session file each chat's place is in.
    pub files: HashMap<String, String>,
    reading: HashMap<String, Reading>,
    /// The chats whose place was further back than [`PAGES`] pages, with the row at their top then: the line stands
    /// until he moves.
    further: HashMap<String, String>,
}

pub fn thread_view(project: &str, id: &str) -> String {
    format!("thread {project} {id}")
}

pub fn chat_view(project: &str, agent: &str) -> String {
    format!("chat {project} {agent}")
}

impl Places {
    /// View VIEW (its list keyed SCOPE, its rows' keys ALL) laid out with the rows SHOWN in sight; AT_END when the
    /// last row is among them and the view sticks to its end; IN when its rows have come (a chat's page read, or
    /// refused). True when a scroll back began (the view runs again).
    pub fn report(&mut self, view: &str, scope: &str, all: &[String], shown: std::ops::Range<usize>, at_end: bool, rows_in: bool) -> bool {
        let Some(first) = all.get(shown.start) else { return false };
        let top = if at_end { String::new() } else { first.clone() };
        self.top.insert(view.into(), top.clone());
        if self.further.get(view).is_some_and(|t| *t != top) {
            self.further.remove(view);
        }
        let sight = all.get(shown).unwrap_or_default();
        if let Some(b) = self.back.get_mut(view) {
            b.shown = sight.contains(&b.key);
            if rows_in && !all.contains(&b.key) {
                // gone: where the view stands is kept from now on
                self.back.remove(view);
                self.kept.insert(view.into(), top);
            }
            return false;
        }
        if self.seen.insert(view.into()) {
            let want = match self.kept.get(view) {
                Some(w) if !w.is_empty() && *w != top && !(rows_in && !all.contains(w)) => Some(w.clone()),
                // a thread he left at its foot opens at its foot (`left`): its scroll may come back at the offset an
                // earlier visit left it at, so its last row is brought into sight
                Some(w) if w.is_empty() && !at_end && view.starts_with("thread ") => all.last().cloned(),
                _ => None,
            };
            if let Some(want) = want {
                self.back.insert(view.into(), Back { shown: sight.contains(&want), key: want, scope: scope.into(), asked: 0, ours: None, asked_last: false, before: None, last_at_top: false });
                return true;
            }
        }
        self.kept.insert(view.into(), top);
        false
    }

    /// VIEW is opened at a post (`find::open_at`): that post wins over the kept row, and nothing is brought back.
    pub fn sought(&mut self, view: &str) {
        self.kept.remove(view);
        self.back.remove(view);
        self.seen.insert(view.into());
    }

    /// He went back or forward to VIEW (history.rs): the next time it is laid out it is brought back to row KEY, as
    /// after a swap ("" at a chat's end: it sticks there by itself).
    pub fn bring(&mut self, view: &str, key: &str) {
        self.kept.insert(view.into(), key.into());
        self.back.remove(view);
        self.reading.remove(view);
        self.further.remove(view);
        self.seen.remove(view);
    }

    /// He left VIEW for another place (history.rs `poll`): a thread is brought back to the row he left at its top the
    /// next time it is laid out, as after a swap, so it opens where he last read it, on Threads or beside an agent's
    /// list, and at its foot when he left it there (item 15, paper-research N-1446 p2603 (a)). Each thread is a scroll
    /// of its own (threads.rs `thread_list`), so nothing else would bring it back. A chat is as before.
    pub fn left(&mut self, view: &str) {
        if view.starts_with("thread ") {
            self.seen.remove(view);
        }
    }

    /// He scrolled VIEW himself: a scroll back stops at once, and where he goes is kept.
    pub fn his_scroll(&mut self, view: &str) {
        self.further.remove(view);
        self.reading.remove(view);
        if self.back.remove(view).is_some() {
            self.kept.insert(view.into(), self.top.get(view).cloned().unwrap_or_default());
        }
    }

    /// The scrolls to ask now, as (scope, key, put at the top): first the row is brought into the laid-out rows, then
    /// its top put at the view's top, every other poll; the poll between takes the offset that came of it. A view
    /// whose row is at its top is done; one whose offset moved with nothing asked was moved by him, and keeps it.
    pub fn asks(&mut self, shown: impl Fn(&str) -> bool) -> Vec<(String, String, bool)> {
        let offsets = self.offsets.lock().map(|o| o.clone()).unwrap_or_default();
        let (back, top, kept, reading, seen) = (&mut self.back, &self.top, &mut self.kept, &self.reading, &mut self.seen);
        let mut out = Vec::new();
        back.retain(|view, b| {
            let now = offsets.get(view).copied();
            if top.get(view) == Some(&b.key) {
                kept.insert(view.clone(), b.key.clone());
                return false;
            }
            // he left it: nothing more is asked, and nothing wakes the window for it; its row stays kept, and it is
            // brought back when he returns (consult-2, N-586.p43)
            if !shown(view) {
                seen.remove(view);
                return false;
            }
            // a chat reading back to its row: nothing to scroll to until it is held
            if reading.contains_key(view) {
                return true;
            }
            if std::mem::take(&mut b.asked_last) {
                b.ours = now;
                // the row laid out, put at the top, and nothing moved: the content ends below it, so it stays here
                if b.last_at_top && b.shown && matches!((b.before, now), (Some(x), Some(y)) if (x - y).abs() <= 0.5) {
                    kept.insert(view.clone(), top.get(view).cloned().unwrap_or_default());
                    return false;
                }
                return true;
            }
            if let (Some(was), Some(now)) = (b.ours, now)
                && (now - was).abs() > MOVED
            {
                kept.insert(view.clone(), top.get(view).cloned().unwrap_or_default());
                return false;
            }
            b.ours = b.ours.or(now);
            let at_top = b.shown && b.asked > 0;
            (b.before, b.last_at_top) = (now, at_top);
            out.push((b.scope.clone(), b.key.clone(), at_top));
            b.asked += 1;
            b.asked_last = true;
            true
        });
        self.reading.retain(|v, _| self.back.contains_key(v));
        out
    }

    /// The chat VIEW, its session file SESSION, laid out with nothing being brought back: its place is in that file.
    pub fn file(&mut self, view: &str, session: &str) {
        if !self.back.contains_key(view) && !session.is_empty() {
            self.files.insert(view.into(), session.into());
        }
    }

    /// Whether the row chat VIEW is brought back to may yet come by reading back (then its rows are not all in): it
    /// is being read back to, or it lies before what is held of the same file.
    pub fn behind(&self, view: &str, held: Held) -> bool {
        if self.reading.contains_key(view) {
            return true;
        }
        let Some(key) = self.back.get(view).map(|b| &b.key).or(self.kept.get(view)) else { return false };
        let file = self.files.get(view).is_none_or(|f| f == held.session);
        file && held.start > 0 && offset(key).is_some_and(|o| o < held.start)
    }

    /// Whether chat VIEW says its place is further back.
    pub fn further(&self, view: &str) -> bool {
        self.further.contains_key(view)
    }

    /// Each poll, for a chat VIEW being brought back, what is held of it now: true when the page before it is to be
    /// asked. Its place in another file is dropped; a row held now ends the reading (the scroll back goes on as for
    /// a thread); his move, or [`PAGES`] pages read, ends it all.
    pub fn read_back(&mut self, view: &str, held: Held) -> bool {
        let Some(b) = self.back.get_mut(view) else {
            self.reading.remove(view);
            return false;
        };
        if !held.loaded {
            return false;
        }
        if self.files.get(view).is_some_and(|f| f != held.session) {
            self.back.remove(view);
            self.reading.remove(view);
            self.kept.remove(view);
            self.files.insert(view.into(), held.session.into());
            return false;
        }
        let Some(at) = offset(&b.key) else { return false };
        if held.start <= at || held.start == 0 {
            if self.reading.remove(view).is_some() {
                // the offset moved with the pages put in: the scroll back takes it afresh
                (b.ours, b.asked_last) = (None, false);
            }
            return false;
        }
        let now = self.top.get(view).cloned();
        let r = self.reading.entry(view.into()).or_default();
        let stamp = (held.entries, held.start, held.next);
        if r.stamp == Some(stamp) {
            r.steady += 1;
        } else {
            (r.stamp, r.steady) = (Some(stamp), 0);
        }
        // the row at the top changed: he moved (nothing of ours scrolls the view while it reads back)
        let moved = r.top.is_some() && r.top != now;
        r.top = now;
        let end = |s: &mut Self, further: bool| {
            s.back.remove(view);
            s.reading.remove(view);
            s.kept.insert(view.into(), s.top.get(view).cloned().unwrap_or_default());
            if further {
                s.further.insert(view.into(), s.top.get(view).cloned().unwrap_or_default());
            }
        };
        if moved {
            end(self, false);
            return false;
        }
        if held.pending || r.steady < SETTLE {
            return false;
        }
        if r.pages >= PAGES {
            end(self, true);
            return false;
        }
        r.pages += 1;
        true
    }

    /// What a view's viewport reports of its offset (`El::on_scroll_offset`).
    pub fn offset_watch(&self, view: String) -> impl Fn(rune::ScrollPos) + 'static {
        let at = self.offsets.clone();
        move |pos: rune::ScrollPos| {
            if let Ok(mut m) = at.lock() {
                m.insert(view.clone(), pos.offset[1]);
            }
        }
    }
}

/// The threads and chats shown now, in any pane: a thread on Threads, or beside an agent's list on its With you
/// (history.rs `thread_shown`), there while its chat shows With you, not its terminal.
fn shown(s: &crate::App) -> HashSet<String> {
    let mut out = HashSet::new();
    for (pi, sel) in s.panes.iter().map(|(i, sel)| (*i, sel)).chain(std::iter::once((s.cur, &s.sel))) {
        let Some(p) = s.projects.get(pi) else { continue };
        let thread = match sel {
            crate::Sel::Agent(a) if !s.term.pages.contains(&(pi, a.clone())) => {
                out.insert(chat_view(&p.id, a));
                s.streams.which.get(&(pi, a.clone())).copied().unwrap_or_default() == crate::streams::Which::You
            }
            crate::Sel::Agent(_) => false,
            sel => sel.shows_threads() || sel.with_you().is_some(),
        };
        if let Some(v) = thread.then(|| crate::history::thread_shown(s, pi, sel)).flatten().and_then(|n| crate::find::ticket(s, p, n)) {
            out.insert(thread_view(&p.id, &v.id));
        }
    }
    out
}

/// Each poll: the views being brought back are scrolled towards their rows.
pub fn poll(s: &mut crate::App, tx: &mut rune::Tx) {
    if s.read_places.back.is_empty() {
        return;
    }
    let shown = shown(s);
    // the chats whose row is before what is held: one page further back each, when the last is in and laid out
    let chats: Vec<String> = s.read_places.back.keys().filter(|v| v.starts_with("chat ") && shown.contains(*v)).cloned().collect();
    for view in chats {
        let Some((pid, agent)) = view.strip_prefix("chat ").and_then(|v| v.split_once(' ')) else { continue };
        let Some(p) = s.projects.iter_mut().find(|p| p.id == pid) else { continue };
        let held = p.convs.get(agent).map(|c| (c.session.clone(), c.start, c.entries.len(), c.next, c.loaded, c.older_pending));
        let Some((session, start, entries, next, loaded, pending)) = held else { continue };
        if s.read_places.read_back(&view, Held { session: &session, start, entries, next, loaded, pending }) {
            p.read_older(agent);
        }
    }
    for (scope, key, at_top) in s.read_places.asks(|v| shown.contains(v)) {
        if at_top {
            tx.scroll_key_to(key, 0.0);
        } else {
            tx.scroll_center_in_instant(scope, key);
        }
    }
    tx.wake_after(Duration::from_millis(100));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(k: &[&str]) -> Vec<String> {
        k.iter().map(|s| s.to_string()).collect()
    }

    const T: &str = "thread a N-12";
    const S: &str = "thread-rows-N-12";

    fn rows() -> Vec<String> {
        keys(&["head", "p1", "p2", "p3", "p4", "p5", "p6", "p7", "p8", "p9", "foot"])
    }

    // before the swap he read N-12 down to p7, and a chat to its end; after it they come back from ui.json
    fn after_swap() -> Places {
        let mut was = Places::default();
        was.report(T, S, &rows(), 3..5, false, true);
        was.report(T, S, &rows(), 7..9, false, true);
        was.report("chat a orch", "rows-0-orch", &keys(&["e10-0", "e20-0"]), 0..2, true, true);
        assert_eq!(was.kept.get(T).map(String::as_str), Some("p7"));
        assert_eq!(was.kept.get("chat a orch").map(String::as_str), Some(""), "at a chat's end: nothing to bring back");
        Places { kept: was.kept, ..Default::default() }
    }

    #[test]
    fn a_swap_brings_back_the_row_he_was_reading() {
        let mut now = after_swap();
        // the thread opens at its head: a scroll back begins, and what it opened at is not kept meanwhile
        assert!(now.report(T, S, &rows(), 0..2, false, true));
        assert!(!now.report("chat a orch", "rows-0-orch", &keys(&["e10-0", "e30-0"]), 1..2, true, true), "the chat sticks to its end itself");
        assert_eq!(now.kept.get(T).map(String::as_str), Some("p7"));
        // first into the laid-out rows, then its top at the view's top; the poll between only takes the offset
        assert_eq!(now.asks(|_| true), [(S.to_string(), "p7".to_string(), false)]);
        now.report(T, S, &rows(), 6..8, false, true);
        assert!(now.asks(|_| true).is_empty());
        assert_eq!(now.asks(|_| true), [(S.to_string(), "p7".to_string(), true)]);
        // end 1: the row at the top
        now.report(T, S, &rows(), 7..9, false, true);
        assert!(now.asks(|_| true).is_empty() && now.back.is_empty());
        // from then on his scrolling is kept, and the view is not brought back a second time
        now.report(T, S, &rows(), 9..11, false, true);
        assert_eq!(now.kept.get(T).map(String::as_str), Some("p9"));
    }

    #[test]
    fn a_scroll_back_ends_when_the_row_is_gone_or_he_scrolls() {
        // the rows not in yet (a chat's page coming): it waits, however long that takes
        let mut slow = after_swap();
        let chat = "chat a dev";
        slow.kept.insert(chat.into(), "e5-0".into());
        assert!(slow.report(chat, "rows-0-dev", &keys(&["e9-0"]), 0..1, false, false));
        assert_eq!(slow.asks(|_| true).len(), 1);
        assert!(slow.asks(|_| true).is_empty());
        assert_eq!(slow.asks(|_| true).len(), 1, "still asked while the rows come");
        // end 2: the rows are in and the row is not among them
        slow.report(chat, "rows-0-dev", &keys(&["e7-0", "e9-0"]), 0..2, false, true);
        assert!(slow.back.is_empty() && slow.asks(|_| true).is_empty());
        assert_eq!(slow.kept.get(chat).map(String::as_str), Some("e7-0"), "where the view stands is kept");
        // ... and a row gone already at the first sight begins nothing
        let mut gone = after_swap();
        gone.kept.insert(T.into(), "p40".into());
        assert!(!gone.report(T, S, &rows(), 0..2, false, true) && gone.back.is_empty());

        // end 3: he scrolls himself, and that wins at once
        let mut his = after_swap();
        assert!(his.report(T, S, &rows(), 0..2, false, true));
        his.report(T, S, &rows(), 2..4, false, true);
        his.his_scroll(T);
        assert!(his.back.is_empty() && his.asks(|_| true).is_empty());
        assert_eq!(his.kept.get(T).map(String::as_str), Some("p2"));

        // ... by the scrollbar or a key too: the offset moves between two polls with nothing asked, while the page is
        // still coming, and when the row then arrives nothing pulls him back (paper-research N-788.p9)
        let mut bar = after_swap();
        let chat = "chat a dev";
        bar.kept.insert(chat.into(), "e5-0".into());
        let off = |p: &Places, y: f32| _ = p.offsets.lock().unwrap().insert(chat.to_string(), y);
        off(&bar, 900.0);
        assert!(bar.report(chat, "rows-0-dev", &keys(&["e9-0"]), 0..1, false, false));
        assert_eq!(bar.asks(|_| true).len(), 1, "asked, the row not held yet");
        assert!(bar.asks(|_| true).is_empty(), "the offset that came of it taken");
        off(&bar, 400.0);
        assert!(bar.asks(|_| true).is_empty() && bar.back.is_empty(), "his move ends it");
        bar.report(chat, "rows-0-dev", &keys(&["e5-0", "e7-0", "e9-0"]), 2..3, false, true);
        assert!(bar.asks(|_| true).is_empty() && bar.back.is_empty(), "the row came: nothing pulls him back");
        assert_eq!(bar.kept.get(chat).map(String::as_str), Some("e9-0"));
    }

    /// A thread he left for another place opens where he left it when he comes back (item 15, paper-research N-1446
    /// p2603 (a)), though its scroll comes back elsewhere: at the row he read to, or at its foot; a chat is as before.
    #[test]
    fn a_thread_left_for_another_place_opens_where_he_left_it() {
        let mut row = Places::default();
        row.report(T, S, &rows(), 7..9, false, true);
        row.left(T);
        assert!(row.report(T, S, &rows(), 0..2, false, true), "brought back");
        assert_eq!(row.asks(|_| true), [(S.to_string(), "p7".to_string(), false)]);
        // left at its foot, its scroll back at an offset an earlier visit left: brought to its last row
        let mut foot = Places::default();
        foot.report(T, S, &rows(), 9..11, true, true);
        assert_eq!(foot.kept.get(T).map(String::as_str), Some(""));
        foot.left(T);
        assert!(foot.report(T, S, &rows(), 3..5, false, true), "brought to its foot");
        assert_eq!(foot.asks(|_| true), [(S.to_string(), "foot".to_string(), false)]);
        // ... at its foot already: nothing to do
        let mut there = Places::default();
        there.report(T, S, &rows(), 9..11, true, true);
        there.left(T);
        assert!(!there.report(T, S, &rows(), 9..11, true, true) && there.back.is_empty());
        // a chat: left and back, nothing brought back
        let (chat, scope, all) = ("chat a dev", "rows-0-dev", keys(&["e1-0", "e2-0", "e3-0"]));
        let mut c = Places::default();
        c.report(chat, scope, &all, 0..1, false, true);
        c.left(chat);
        assert!(!c.report(chat, scope, &all, 2..3, true, true) && c.back.is_empty());
    }

    #[test]
    fn a_post_sought_wins_and_a_row_that_cannot_reach_the_top_ends_it() {
        // consult-2's example: N-412 read down to p590, a swap, then a starred answer opens it at p640
        let rows: Vec<String> = (580..=650).map(|n| format!("p{n}")).collect();
        let view = "thread a N-412";
        let mut star = Places { kept: [(view.to_string(), "p590".to_string())].into(), ..Default::default() };
        star.sought(view);
        assert!(!star.report(view, "thread-rows-N-412", &rows, 60..62, false, true), "no scroll back");
        assert!(star.asks(|_| true).is_empty());
        assert_eq!(star.kept.get(view).map(String::as_str), Some("p640"), "the sought post stays, and is kept");

        // a taller window after the swap: p648 is laid out but the thread ends below it, so it cannot reach the top
        let mut tall = Places { kept: [(view.to_string(), "p648".to_string())].into(), ..Default::default() };
        let off = |p: &Places, y: f32| _ = p.offsets.lock().unwrap().insert(view.to_string(), y);
        off(&tall, 3000.0);
        assert!(tall.report(view, "thread-rows-N-412", &rows, 0..2, false, true));
        assert_eq!(tall.asks(|_| true).len(), 1, "into the laid-out rows");
        tall.report(view, "thread-rows-N-412", &rows, 60..71, false, true);
        off(&tall, 5000.0);
        assert!(tall.asks(|_| true).is_empty());
        assert_eq!(tall.asks(|_| true), [("thread-rows-N-412".to_string(), "p648".to_string(), true)], "its top asked at the view's top");
        // clamped: the offset did not move
        assert!(tall.asks(|_| true).is_empty() && tall.back.is_empty(), "ended: nothing more asked or woken");
        assert_eq!(tall.kept.get(view).map(String::as_str), Some("p640"));

        // he left the view before it was back: it ends, and nothing wakes the window for it ...
        let mut left = Places { kept: [(view.to_string(), "p600".to_string())].into(), ..Default::default() };
        assert!(left.report(view, "thread-rows-N-412", &rows, 0..2, false, true));
        assert!(left.asks(|_| false).is_empty() && left.back.is_empty());
        // ... and when he returns it is brought back, not kept where it opened (consult-2, N-586.p43)
        assert_eq!(left.kept.get(view).map(String::as_str), Some("p600"));
        assert!(left.report(view, "thread-rows-N-412", &rows, 0..2, false, true), "brought back again");

        // a chat kept at e30, left while its page is still coming: when he returns, e30 comes back
        let chat = "chat a dev";
        let mut away = Places { kept: [(chat.to_string(), "e30-0".to_string())].into(), ..Default::default() };
        assert!(away.report(chat, "rows-0-dev", &keys(&["e90-0"]), 0..1, false, false), "waits for the page");
        assert_eq!(away.asks(|_| true).len(), 1);
        assert!(away.asks(|_| false).is_empty() && away.back.is_empty(), "left");
        let rows = keys(&["e10-0", "e20-0", "e30-0", "e40-0", "e90-0"]);
        assert!(away.report(chat, "rows-0-dev", &rows, 3..5, true, true), "he returns, the page in: brought back");
        assert_eq!(away.asks(|_| true), [("rows-0-dev".to_string(), "e30-0".to_string(), false)]);
        away.report(chat, "rows-0-dev", &rows, 1..3, false, true);
        assert!(away.asks(|_| true).is_empty());
        assert_eq!(away.asks(|_| true), [("rows-0-dev".to_string(), "e30-0".to_string(), true)]);
        away.report(chat, "rows-0-dev", &rows, 2..4, false, true);
        assert!(away.asks(|_| true).is_empty() && away.back.is_empty());
        assert_eq!(away.kept.get(chat).map(String::as_str), Some("e30-0"), "e30 at the top");
    }

    /// A chat after a swap, its kept row KEPT bytes into the session file; the session held from 600 to 700, a page
    /// being 100 bytes and one entry every 10. Polls as the window does (read back, then the scroll asks), a page
    /// coming 5 polls after it is asked, until nothing is being brought back or 200 polls. At each poll EV says what
    /// else happens: HIS, he moves the view up (by the scrollbar or a key: no wheel); GROW, the agent's new entry
    /// grows in at the end, the offset following it. The pages asked, and the places.
    fn read_back_to(kept: u64, file: &str, ev: impl Fn(u32) -> Ev) -> (u32, Places) {
        let view = "chat a dev";
        let scope = "rows-0-dev";
        let keys = |start: u64| (start..700).step_by(10).map(|o| format!("e{o}-0")).collect::<Vec<_>>();
        let mut p = Places { kept: [(view.to_string(), format!("e{kept}-0"))].into(), files: [(view.to_string(), file.to_string())].into(), ..Default::default() };
        let (mut start, mut coming, mut asked) = (600u64, None::<u32>, 0);
        let held = |start: u64, pending: bool| Held { session: "s.jsonl", start, entries: ((700 - start) / 10) as usize, next: 700, loaded: true, pending };
        // the chat opens at its end
        let rows_in = !p.behind(view, held(start, false));
        assert!(p.report(view, scope, &keys(start), 8..10, true, rows_in) || file != "s.jsonl" || kept >= 600);
        p.file(view, "s.jsonl");
        p.offsets.lock().unwrap().insert(view.into(), 1000.0);
        for poll in 0..200 {
            match ev(poll) {
                Ev::His => _ = p.report(view, scope, &keys(start), 2..4, false, true),
                Ev::Grow => {
                    let y = 1000.0 + 7.0 * poll as f32;
                    p.offsets.lock().unwrap().insert(view.into(), y);
                    let k = keys(start);
                    p.report(view, scope, &k, k.len() - 2..k.len(), true, false);
                }
                Ev::None => {}
            }
            if coming == Some(poll) {
                // the page is in and laid out: it is put above, his view held where it was by the rows put in
                (start, coming) = (start - 100, None);
                let k = keys(start);
                let rows_in = !p.behind(view, held(start, false));
                p.report(view, scope, &k, k.len() - 2..k.len(), true, rows_in);
                p.file(view, "s.jsonl");
            }
            if p.read_back(view, held(start, coming.is_some())) {
                assert!(coming.is_none(), "never two pages in flight");
                (coming, asked) = (Some(poll + 5), asked + 1);
            }
            for (_, key, at_top) in p.asks(|_| true) {
                // the row laid out and put at the top
                let k = keys(start);
                let i = k.iter().position(|x| *x == key).expect("asked only for a row held");
                if at_top {
                    p.report(view, scope, &k, i..i + 2, false, true);
                } else {
                    p.report(view, scope, &k, i.saturating_sub(1)..i + 1, false, true);
                }
            }
            if p.back.is_empty() && coming.is_none() {
                break;
            }
        }
        (asked, p)
    }

    #[derive(PartialEq)]
    enum Ev {
        None,
        His,
        Grow,
    }

    #[test]
    fn a_chat_reads_back_to_its_row_one_page_at_a_time_and_at_most_four() {
        let view = "chat a dev";
        // 3 pages back: reached in 3 reads, and it lands at the top
        let (asked, p) = read_back_to(310, "s.jsonl", |_| Ev::None);
        assert_eq!(asked, 3);
        assert!(p.back.is_empty() && !p.further(view));
        assert_eq!(p.kept.get(view).map(String::as_str), Some("e310-0"), "at the top");
        // 6 pages back: it stops at 4 and says so
        let (asked, p) = read_back_to(10, "s.jsonl", |_| Ev::None);
        assert_eq!(asked, PAGES);
        assert!(p.back.is_empty() && p.further(view));
        assert_ne!(p.kept.get(view).map(String::as_str), Some("e10-0"), "where it stands is kept");
        // he scrolls during the reads: no further page is asked, and no line
        let (asked, p) = read_back_to(10, "s.jsonl", |poll| if poll == 6 { Ev::His } else { Ev::None });
        assert_eq!(asked, 1, "the page in flight when he moved is the last");
        assert!(p.back.is_empty() && !p.further(view));
        // ... also in the polls just after a page landed (the next is asked 3 polls after one lands, at 8 + 3)
        let (asked, p) = read_back_to(10, "s.jsonl", |poll| if poll == 9 { Ev::His } else { Ev::None });
        assert_eq!(asked, 1, "moved before the next page was asked");
        assert!(p.back.is_empty());
        // the agent writes while a page is coming: its entry grows in at the end for 10 polls, and the reads go on
        let (asked, p) = read_back_to(310, "s.jsonl", |poll| if (5..15).contains(&poll) { Ev::Grow } else { Ev::None });
        assert_eq!(asked, 3);
        assert_eq!(p.kept.get(view).map(String::as_str), Some("e310-0"), "at the top");
        // the faint line stands until he moves
        let (_, mut p) = read_back_to(10, "s.jsonl", |_| Ev::None);
        let k: Vec<String> = (200..700).step_by(10).map(|o| format!("e{o}-0")).collect();
        p.report(view, "rows-0-dev", &k, k.len() - 2..k.len(), true, true);
        assert!(p.further(view), "still at the end");
        p.report(view, "rows-0-dev", &k, 5..7, false, true);
        assert!(!p.further(view), "he moved up, by any means");
        // ... the wheel likewise
        let mut w = Places { kept: [(view.to_string(), "e10-0".to_string())].into(), ..Default::default() };
        let h = Held { session: "s.jsonl", start: 600, entries: 10, next: 700, loaded: true, pending: false };
        assert!(w.report(view, "rows-0-dev", &keys(&["e600-0", "e610-0"]), 0..2, true, !w.behind(view, h)));
        w.his_scroll(view);
        assert!((0..20).all(|_| !w.read_back(view, h)) && w.back.is_empty());
        // a place in another session file is dropped, with no line and nothing read
        let (asked, p) = read_back_to(310, "old.jsonl", |_| Ev::None);
        assert_eq!(asked, 0);
        assert!(p.back.is_empty() && !p.further(view));
        assert_eq!(p.files.get(view).map(String::as_str), Some("s.jsonl"));
    }

    /// The try of the read-back in a window (paper-research N-586.p52), offscreen and on the wall clock:
    /// `STILL_OUT=DIR READ_BACK_BRIDGE=bridge.py READ_BACK_KEPT=OFFSET READ_BACK_AT=SECONDS cargo test read_back_still
    /// -- --ignored`. The bridge is a made-up one (one agent writing a line every 0.4 s, older pages after 1.4 s; no
    /// coordd, no record, no model); ui.json's place says its chat was read at the line at OFFSET, as a swap leaves it.
    /// The still is the chat at SECONDS, and READ_BACK_TREE the element tree then.
    #[test]
    #[ignore]
    fn read_back_still() {
        let (Ok(out), Ok(_)) = (std::env::var("STILL_OUT"), std::env::var("READ_BACK_BRIDGE")) else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn init() -> crate::App {
            let (_held, mut s) = crate::view_cost::test_app();
            let p = crate::Proj::new(&crate::projects::ProjectConfig { name: "standin".into(), id: "standin".into(), boxed: false }, Vec::new());
            s.projects = vec![p];
            s.cur = 0;
            s.sel = crate::Sel::Agent("dev".into());
            s.streams.which.insert((0, "dev".into()), crate::streams::Which::Work);
            let kept: u64 = std::env::var("READ_BACK_KEPT").unwrap().parse().unwrap();
            s.read_places.kept.insert(chat_view("standin", "dev"), format!("e{kept}-0"));
            s.read_places.files.insert(chat_view("standin", "dev"), "made-up.jsonl".into());
            s
        }
        fn setup(s: &mut crate::App, wake: rune::Waker) {
            let cmd = vec!["python3".to_string(), "-I".into(), std::env::var("READ_BACK_BRIDGE").unwrap()];
            s.projects[0].link = Some(crate::link::Link::start(cmd, wake.clone(), wake));
        }
        // the pipe and the scroll back only (no other page's work, nothing of the owner's)
        fn poll(s: &mut crate::App, _: f64, tx: &mut rune::Tx) -> bool {
            let ms: Vec<_> = s.projects[0].link.as_ref().map(|l| l.drain()).unwrap_or_default();
            let changed = !ms.is_empty();
            for m in ms {
                s.take(0, m);
            }
            // the chat opened, as a click does (asked again until the pipe is up)
            s.projects[0].open_conv("dev");
            super::poll(s, tx);
            tx.wake_after(Duration::from_millis(50));
            changed
        }
        let at = std::env::var("READ_BACK_AT").unwrap_or_else(|_| "10".into());
        let name = format!("read-back-{}.png", std::env::var("READ_BACK_KEPT").unwrap());
        let mut args = vec!["--realtime".to_string(), "--snap".into(), out.join(&name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), at];
        if let Ok(t) = std::env::var("READ_BACK_TREE") {
            args.extend(["--tree".to_string(), t]);
        }
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), poll: Some(poll), setup: Some(setup), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) }).args(args).try_run().unwrap();
    }

    /// The still for the hand-over (paper-research N-788.p9): `STILL_OUT=DIR cargo test read_place_still -- --ignored`.
    /// A made-up thread of 40 posts, no coordd: ui.json's place says N-575 was read at p30, as a swap leaves it; after
    /// the first layout and the scroll back, p30 is at the top of the thread, not its head.
    #[test]
    #[ignore]
    fn read_place_still() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn init() -> crate::App {
            let (_held, mut s) = crate::view_cost::test_app();
            let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "standin".into(), id: "standin".into(), boxed: false }, Vec::new());
            p.down = None;
            p.protocol = coord_types::PROTOCOL;
            let at = |m: u32| format!("2026-11-19T01:{m:02}:00Z");
            let posts: Vec<serde_json::Value> = (1..=40)
                .map(|n| {
                    let by = if n % 2 == 1 { "owner" } else { "paper-research" };
                    serde_json::json!({"n": n, "at": at(n), "by": by, "text": format!("p{n}: a made-up post of the stand-in thread, long enough to take a few lines in the window so that forty of them scroll well past one screen."), "told": ["owner"]})
                })
                .collect();
            let v: docket_machine::TicketView = serde_json::from_value(serde_json::json!({"id": "N-575", "n": 575, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
                "title": "Runner rows", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(0), "given": [], "posts": posts}))
            .unwrap();
            p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v) });
            s.projects = vec![p];
            s.cur = 0;
            s.sel = crate::Sel::Tickets;
            s.tickets.shown = Some(575);
            // as ui.json brings it back after a swap
            s.read_places.kept.insert(thread_view("standin", "N-575"), "post-N-575-30".into());
            s
        }
        fn poll(s: &mut crate::App, _: f64, tx: &mut rune::Tx) -> bool {
            super::poll(s, tx);
            false
        }
        let args = vec!["--snap".to_string(), out.join("read-place.png").display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "2.0".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), poll: Some(poll), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) }).args(args).try_run().unwrap();
    }
}
