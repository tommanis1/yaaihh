//! Typing suggestions in the box (0.2 line 27; paper-research N-1396 post 32; the owner, N-1104 post 489: "show
//! suggestion completions as i type … getting the ui for tab completion right … for the suggestion i need to like click
//! instead of just move my cursor"). This is the source half (docket-edge2). The drawing, the caret and the frame cost
//! are docket-edge5's (N-1426), who draws what [`ghost`] gives and nothing else.
//!
//! - **One ghost at most per box, at the caret.** It is worked out from the box's text and caret each time it is asked
//!   for, so it is never stale against what he typed. It is never part of the text: a send, a copy or an undo carries
//!   only what he typed (consult-3, N-1396 post 33, 2).
//! - **`Complete`**: the caret ends a word of three letters or more, and nothing but spaces follows it to the end of
//!   its line (docket-edge5 draws it over the line). The ghost is the rest of the word he is most likely typing: a word
//!   of the thread he is on or of his own box first, else the commonest word of the local list (en-80k) that starts
//!   so, when it is clearly the likeliest. No ghost when the word he typed is itself commoner than the completion
//!   (`the` on a thread about threads), when a near word is far commoner (`teh` is "the" mistyped, not the start of
//!   "tehran"), or in the first seconds after the window's start, while SymSpell's list is built.
//! - **`Fix`**: the caret is in or at a marked word he is not typing (he moved there, or the pause ended). The ghost
//!   is its best candidate, the one the card shows first.
//! - **Two sources, one anchor.** `Local` is at once. `Call` is the cleaning's answer (B9, changed.rs): an edit on
//!   the line he is typing on is held as a ghost while he types (`hold`), so an answer arriving 5 s later never moves
//!   what he is typing; it goes in when he stops or leaves that line (`poll`). A held edit at the caret replaces the
//!   local ghost on the same word, with no shift. A third source (a model on the tower, roadmap) is one more `Source`.
//! - **Keys** (`key`): Tab takes the ghost as one undo step, the caret after it. Esc drops every ghost on that word
//!   while it stands so (a held edit of the cleaning with it), and its completions while he types it on. Typing on
//!   ignores it. Without a ghost both keys do what they did.
//! - No ghost while text is selected, while the input method composes, in a box without the keyboard, with the
//!   spelling mode off, or on code, a pill, a name, a path or an id (spell.rs `words`).

use crate::App;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;
use std::sync::OnceLock;

/// The fewest letters a word has before it is completed.
pub const MIN_PREFIX: usize = 3;

/// How much of the list's count under a prefix its commonest completion must hold to be offered.
const SHARE: f64 = 0.5;

/// How many times commoner than the completion a word one edit away must be for the prefix to read as that word
/// mistyped.
const MISTYPED: u64 = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The rest of the word the caret ends: drawn after the caret.
    Complete,
    /// The marked word the caret is in: drawn as N-815.p13's ghost under it.
    Fix,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The window's own: the local list, the machine's checker, Harper and SymSpell.
    Local,
    /// The cleaning's answer (changed.rs), held on the line he types on.
    Call,
}

/// What box KEY shows at its caret. `range` and `word` are the anchor: the word as it stands in the box. Tab puts
/// `with` in place of `range`.
#[derive(Clone, Debug, PartialEq)]
pub struct Ghost {
    pub key: (usize, String),
    pub range: Range<usize>,
    pub word: String,
    pub with: String,
    pub kind: Kind,
    pub source: Source,
}

impl Ghost {
    /// What is drawn after the caret: the rest of `with` past what he typed (a `Complete`'s); empty for a `Fix`.
    pub fn rest(&self) -> &str {
        match self.kind {
            Kind::Complete => self.with.get(self.word.len()..).unwrap_or(""),
            Kind::Fix => "",
        }
    }
}

/// An edit of the cleaning's answer held on the line he types on: where it stands, the words it stands for, and what
/// takes their place.
#[derive(Clone, Debug, PartialEq)]
pub struct Held {
    pub range: Range<usize>,
    pub word: String,
    pub with: String,
}

/// One box's last answer, by its anchor: `ghost` is asked on every frame (docket-edge5, N-1426 p2, 3).
struct Memo {
    start: usize,
    word: String,
    /// Not prose (a name, a path, code): no ghost.
    prose: bool,
    complete: Option<String>,
    /// Its best candidate when marked, worked out the first time it is wanted (the checker's candidates are slow).
    fix: Option<Option<String>>,
}

#[derive(Default)]
pub struct State {
    /// The boxes the last poll saw him typing in: when he stops, a frame (a `Fix` may show).
    typing: HashSet<(usize, String)>,
    /// Each box's held edits of the cleaning (`hold`), in the box's order.
    held: HashMap<(usize, String), Vec<Held>>,
    /// The ghost he dropped with Esc in each box: its kind, the start of its word, and the word as it stood. No ghost
    /// shows on that word while it stands so, nor a completion while he types it on.
    dropped: HashMap<(usize, String), (Kind, usize, String)>,
    memo: RefCell<HashMap<(usize, String), Memo>>,
    /// The words of each project's thread (by its number), counted, with the snapshot's `tickets_rev` they were
    /// counted at.
    context: RefCell<HashMap<(usize, u64), (u64, BTreeMap<String, u32>)>>,
}

/// The local list: SymSpell's English list (80,000 words with their counts, marks.rs), by word.
fn list() -> &'static [(&'static str, u64)] {
    static L: OnceLock<Vec<(&'static str, u64)>> = OnceLock::new();
    L.get_or_init(|| {
        let mut v: Vec<(&'static str, u64)> = crate::marks::EN_80K
            .lines()
            .filter_map(|l| {
                let (w, c) = l.split_once(' ')?;
                Some((w, c.trim().parse().ok()?))
            })
            .collect();
        v.sort_unstable_by(|a, b| a.0.cmp(b.0));
        v
    })
}

/// The local list read, at the window's start on the spelling thread (marks.rs), before any completion is asked for.
pub(crate) fn warm() {
    let _ = list();
}

/// The list's count for WORD (lower case), 0 when it is not in it.
fn count(word: &str) -> u64 {
    let l = list();
    l.binary_search_by(|(w, _)| (*w).cmp(word)).map(|i| l[i].1).unwrap_or(0)
}

/// The commonest word starting with lower-case PREFIX and longer than it, from the words he sees (CONTEXT, each with
/// how often) first, else from the list when it holds SHARE of the list's count under PREFIX; none when PREFIX is
/// itself a word of the list commoner than it (`the`, not "thread"), or when a near word of the list is far commoner
/// than it (`teh`). None at all in the first seconds after the window's start, while the spelling thread builds
/// SymSpell's list (`complete_with`).
pub(crate) fn complete(prefix: &str, context: &[&BTreeMap<String, u32>]) -> Option<String> {
    complete_with(prefix, context, crate::marks::ready())
}

/// [`complete`], with BUILT whether SymSpell's list is built: until it is, none (asking it then would hold the frame
/// until the spelling thread has built it, consult-3 N-1396 p73 (a); this list is read just before it, marks.rs).
pub(crate) fn complete_with(prefix: &str, context: &[&BTreeMap<String, u32>], built: bool) -> Option<String> {
    if !built || prefix.chars().count() < MIN_PREFIX {
        return None;
    }
    let longer = |w: &str| w.len() > prefix.len() && w.starts_with(prefix);
    let mut seen: Option<(&str, u32, u64)> = None;
    for c in context {
        for (w, n) in c.range::<str, _>((std::ops::Bound::Excluded(prefix), std::ops::Bound::Unbounded)).take_while(|(w, _)| w.starts_with(prefix)) {
            if !longer(w.as_str()) {
                continue;
            }
            let k = count(w);
            if seen.is_none_or(|(_, sn, sk)| (*n, k) > (sn, sk)) {
                seen = Some((w.as_str(), *n, k));
            }
        }
    }
    // the word he typed is likelier than the thread's longer one: "the" is not the start of "thread", "for" of
    // "format" (consult-3 N-1396 p73 (b); the list's own branch has this from the prefix's count in `total`)
    if let Some((w, _, k)) = seen {
        return (count(prefix) <= k).then(|| w.to_string());
    }
    let l = list();
    let from = l.partition_point(|(w, _)| *w < prefix);
    let (mut total, mut best): (u64, Option<(&str, u64)>) = (0, None);
    for &(w, c) in l[from..].iter().take_while(|(w, _)| w.starts_with(prefix)) {
        total += c;
        if longer(w) && best.is_none_or(|(_, b)| c > b) {
            best = Some((w, c));
        }
    }
    let (w, c) = best?;
    if (c as f64) < SHARE * total as f64 {
        return None;
    }
    // a word he mistyped, not one he is still typing: a word one edit from it is a hundred times commoner than the
    // completion ("teh" is "the", not the start of "tehran"; "thi" may still be "this"). SymSpell gives the rarest of
    // the nearest first, so the commonest is looked for
    if count(prefix) == 0 {
        let near = crate::marks::symspell(prefix).into_iter().filter(|n| !n.starts_with(prefix) && crate::spell::distance(prefix, n) == 1).map(|n| count(&n.to_lowercase())).max();
        if near.is_some_and(|n| n >= MISTYPED * c) {
            return None;
        }
    }
    Some(w.to_string())
}

/// The run of word letters (spell.rs: letters and the apostrophe) of TEXT that byte AT is in or at the end of.
fn word_around(text: &str, at: usize) -> Option<Range<usize>> {
    let is = |c: char| c.is_alphabetic() || c == '\'';
    let start = text[..at].char_indices().rev().take_while(|(_, c)| is(*c)).last().map_or(at, |(i, _)| i);
    let end = at + text[at..].char_indices().find(|(_, c)| !is(*c)).map_or(text.len() - at, |(i, _)| i);
    (start < end).then_some(start..end)
}

/// Whether WORD still stands at R of TEXT, whole: no letter he typed onto either end of it.
fn stands(text: &str, r: &Range<usize>, word: &str) -> bool {
    let is = |c: char| c.is_alphabetic() || c == '\'';
    text.get(r.clone()) == Some(word) && !text[..r.start].chars().next_back().is_some_and(is) && !text[r.end..].chars().next().is_some_and(is)
}

/// Whether nothing but spaces follows byte AT of TEXT up to the end of its line.
fn ends_line(text: &str, at: usize) -> bool {
    text[at..].chars().take_while(|c| *c != '\n').all(char::is_whitespace)
}

/// The byte range of TEXT's line that byte AT is on, without its line break.
fn line_of(text: &str, at: usize) -> Range<usize> {
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
    start..end
}

/// Whether he is typing in box KEY now: its text changed since the last poll, or it changed at the caret the poll
/// saw less than the pause ago (changed.rs's `seen` and `PAUSE`, the record and the pause the cleaning goes by).
pub fn typing(s: &App, key: &(usize, String)) -> bool {
    let Some(e) = s.composers.get(key) else { return false };
    match s.changed.seen.get(key) {
        Some(v) if v.text == e.text => v.caret == e.focus && v.typed && v.since.elapsed() < crate::changed::PAUSE,
        Some(_) => true,
        None => false,
    }
}

/// The words of the thread box KEY is on (its place names it, `thread:N-450`) with how often each stands there: its
/// title and posts as the snapshot has them, counted once for each `tickets_rev`.
fn thread_words<'a>(s: &'a App, key: &(usize, String)) -> Option<std::cell::Ref<'a, BTreeMap<String, u32>>> {
    let at = key.1.find("N-")?;
    let n: u64 = key.1[at + 2..].chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()?;
    let p = s.projects.get(key.0)?;
    let rev = p.live.tickets_rev;
    let fresh = s.suggest.context.borrow().get(&(key.0, n)).is_some_and(|(r, _)| *r == rev);
    if !fresh {
        let v = p.live.tickets.get(&n)?;
        let mut words = BTreeMap::new();
        let mut add = |t: &str| count_words(t, &mut words);
        add(&v.title);
        for post in &v.posts {
            if let docket_machine::Text::New(t) = &post.text {
                add(t);
            }
        }
        s.suggest.context.borrow_mut().insert((key.0, n), (rev, words));
    }
    std::cell::Ref::filter_map(s.suggest.context.borrow(), |c| c.get(&(key.0, n)).map(|(_, w)| w)).ok()
}

/// The words of T (letters only, four or more, lower case) counted into WORDS.
fn count_words(t: &str, words: &mut BTreeMap<String, u32>) {
    for w in t.split(|c: char| !c.is_alphabetic()).filter(|w| w.chars().count() > MIN_PREFIX) {
        *words.entry(w.to_lowercase()).or_default() += 1;
    }
}

/// Box KEY's memo for the word at START (WORD), made when its anchor changed.
fn memo<'a>(s: &'a App, key: &(usize, String), e: &rune::TextEdit, r: &Range<usize>) -> std::cell::RefMut<'a, Memo> {
    let word = &e.text[r.clone()];
    let mut all = s.suggest.memo.borrow_mut();
    if all.get(key).is_none_or(|m| m.start != r.start || m.word != word) {
        let prose = crate::spell::word_at(s, key.0, e, r.end).is_some_and(|w| w == *r);
        let complete = if prose && !word.chars().skip(1).any(char::is_uppercase) && word.to_lowercase().len() == word.len() {
            let lower = word.to_lowercase();
            // the words he sees: his own box's (not the word he is typing) and the thread's
            let mut mine = BTreeMap::new();
            count_words(&e.text[..r.start], &mut mine);
            count_words(&e.text[r.end..], &mut mine);
            let thread = thread_words(s, key);
            let mut ctx: Vec<&BTreeMap<String, u32>> = vec![&mine];
            if let Some(t) = &thread {
                ctx.push(t);
            }
            complete(&lower, &ctx).map(|w| format!("{word}{}", &w[lower.len()..]))
        } else {
            None
        };
        all.insert(key.clone(), Memo { start: r.start, word: word.to_string(), prose, complete, fix: None });
    }
    std::cell::RefMut::map(all, |a| a.get_mut(key).expect("just put in"))
}

/// The ghost box KEY shows at its caret, if any (the module's head says when).
pub fn ghost(s: &App, key: &(usize, String)) -> Option<Ghost> {
    if !s.spell.on {
        return None;
    }
    let e = s.composers.get(key)?;
    if !e.focused || e.anchor != e.focus || e.is_composing() {
        return None;
    }
    let at = e.focus;
    let make = |range: Range<usize>, with: String, kind: Kind, source: Source| {
        let word = e.text[range.clone()].to_string();
        // Esc said no to every ghost on that word while it stands as it was (no fix after the pause for a word whose
        // completion he dropped, docket-edge5 N-1426 p5), and to its completions while he types it on
        let gone = s.suggest.dropped.get(key).is_some_and(|(_, st, w)| *st == range.start && (*w == word || (kind == Kind::Complete && word.starts_with(w.as_str()))));
        (!gone && with != word).then(|| Ghost { key: key.clone(), range, word, with, kind, source })
    };
    // the cleaning's held edit at the caret: the same anchor as the local one, which it replaces
    if let Some(h) = s.suggest.held.get(key).and_then(|l| l.iter().find(|h| h.range.start <= at && at <= h.range.end && stands(&e.text, &h.range, &h.word))) {
        return make(h.range.clone(), h.with.clone(), Kind::Fix, Source::Call);
    }
    let r = word_around(&e.text, at)?;
    let typing = typing(s, key);
    let mut m = memo(s, key, e, &r);
    if !m.prose {
        return None;
    }
    if at == r.end
        && ends_line(&e.text, at)
        && let Some(w) = m.complete.clone()
        && let Some(g) = make(r.clone(), w, Kind::Complete, Source::Local)
    {
        return Some(g);
    }
    if typing {
        return None;
    }
    let fix = m.fix.get_or_insert_with(|| crate::spell::marked_word(s, key, e, &r).then(|| crate::spell::candidates_at(s, key.0, e, &r).into_iter().next()).flatten()).clone()?;
    drop(m);
    make(r, fix, Kind::Fix, Source::Local)
}

/// Tab and Esc in box KEY while it shows a ghost: Tab takes it, Esc drops it. False (the key goes on as before) with
/// no ghost, or with a modifier held.
pub fn key(s: &mut App, key: &(usize, String), ev: &rune::KeyEvent) -> bool {
    if ev.mods != rune::Mods::default() || !matches!(ev.key, rune::Key::Tab | rune::Key::Escape) {
        return false;
    }
    let Some(g) = ghost(s, key) else { return false };
    if ev.key == rune::Key::Escape {
        // a held edit of the cleaning he said no to does not go in when he stops
        if g.source == Source::Call
            && let Some(l) = s.suggest.held.get_mut(key)
        {
            l.retain(|h| h.range != g.range);
        }
        s.suggest.dropped.insert(key.clone(), (g.kind, g.range.start, g.word));
        return true;
    }
    take(s, &g);
    true
}

/// Ghost G put into its box as one undo step, the caret after it: a completion as typed letters; a fix as a changed
/// word whose original his pointer or caret shows (spell.rs).
pub fn take(s: &mut App, g: &Ghost) {
    let Some(e) = s.composers.get(&g.key) else { return };
    if !stands(&e.text, &g.range, &g.word) {
        return;
    }
    match (g.kind, g.source) {
        (Kind::Complete, _) => {
            let Some(e) = s.composers.get_mut(&g.key) else { return };
            let rest = g.rest().to_string();
            e.undo_group(|e| {
                e.set_caret(g.range.end);
                e.insert(&rest);
            });
        }
        (Kind::Fix, Source::Local) => crate::spell::choose(s, &g.key, g.range.clone(), &g.with),
        (Kind::Fix, Source::Call) => {
            if let Some(l) = s.suggest.held.get_mut(&g.key) {
                l.retain(|h| h.range != g.range);
            }
            crate::spell::apply_changes(s, &g.key, &[(g.range.clone(), g.with.clone())]);
            shift_held(s, &g.key, &g.range, g.with.len());
        }
    }
}

/// The held edits of box KEY after an edit at R that became LEN bytes long: those after it move with it.
fn shift_held(s: &mut App, key: &(usize, String), r: &Range<usize>, len: usize) {
    if let Some(l) = s.suggest.held.get_mut(key) {
        let d = len as isize - r.len() as isize;
        for h in l.iter_mut().filter(|h| h.range.start >= r.end) {
            h.range = (h.range.start as isize + d) as usize..(h.range.end as isize + d) as usize;
        }
    }
}

/// The cleaning's EDITS for box KEY (ranges of its text now, in order), split: those on the line he is typing on are
/// held as ghosts until he stops or leaves it (`poll`); the others are given back to go in at once (changed.rs).
pub fn hold(s: &mut App, key: &(usize, String), edits: Vec<(Range<usize>, String)>) -> Vec<(Range<usize>, String)> {
    let Some(e) = s.composers.get(key) else { return edits };
    if !e.focused || !typing(s, key) {
        return edits;
    }
    let line = line_of(&e.text, e.focus);
    let (held, now): (Vec<_>, Vec<_>) = edits.into_iter().partition(|(r, _)| r.start <= line.end && line.start <= r.end);
    if !held.is_empty() {
        // counts only, never his words
        crate::log::line(&format!("cleaning: {} changes held on the line he types on", held.len()));
        let text = e.text.clone();
        let l = s.suggest.held.entry(key.clone()).or_default();
        // a later answer's edit for the same words takes the place of the earlier one
        l.retain(|h| !held.iter().any(|(r, _)| r.start < h.range.end && h.range.start < r.end));
        l.extend(held.into_iter().map(|(r, with)| Held { word: text[r.clone()].to_string(), range: r, with }));
        l.sort_by_key(|h| h.range.start);
    }
    now
}

/// Box KEY was sent or emptied: its ghosts' state goes (changed.rs `sent`).
pub fn sent(s: &mut App, key: &(usize, String)) {
    s.suggest.held.remove(key);
    s.suggest.dropped.remove(key);
    s.suggest.memo.borrow_mut().remove(key);
}

/// Each poll, after changed.rs's (which records each focused box's text and caret): a frame when he stops typing or
/// moves away in a box (woken for the pause's end), and the held edits put in where he stopped typing or left their
/// line, as one undo step each box. True when a frame is wanted.
pub fn poll(s: &mut App, tx: &mut rune::Tx) -> bool {
    let pause = crate::changed::PAUSE;
    let mut changed = false;
    let mut soonest: Option<std::time::Duration> = None;
    let focused: Vec<(usize, String)> = s.composers.iter().filter(|(_, e)| e.focused).map(|(k, _)| k.clone()).collect();
    for key in &focused {
        let typing_now = typing(s, key);
        if typing_now && let Some(v) = s.changed.seen.get(key) {
            let left = pause.saturating_sub(v.since.elapsed());
            soonest = Some(soonest.map_or(left, |w| w.min(left)));
        }
        if typing_now != s.suggest.typing.contains(key) {
            changed = true;
            if typing_now {
                s.suggest.typing.insert(key.clone());
            } else {
                s.suggest.typing.remove(key);
            }
        }
    }
    s.suggest.typing.retain(|k| focused.contains(k));
    match soonest {
        Some(left) if left <= pause / 3 => tx.wake_after(left),
        Some(_) => s.wake_every(pause / 3, tx),
        None => {}
    }
    let keys: Vec<(usize, String)> = s.suggest.held.keys().cloned().collect();
    for key in keys {
        let Some(e) = s.composers.get(&key) else {
            s.suggest.held.remove(&key);
            continue;
        };
        let line = (e.focused && typing(s, &key)).then(|| line_of(&e.text, e.focus));
        let text = e.text.clone();
        let Some(l) = s.suggest.held.get_mut(&key) else { continue };
        // one he typed into or around is the cleaning's no more: the next pause asks for its paragraph again
        l.retain(|h| stands(&text, &h.range, &h.word));
        let (keep, go): (Vec<Held>, Vec<Held>) = std::mem::take(l).into_iter().partition(|h| line.as_ref().is_some_and(|ln| h.range.start <= ln.end && ln.start <= h.range.end));
        *l = keep;
        if l.is_empty() {
            s.suggest.held.remove(&key);
        }
        if !go.is_empty() {
            let edits: Vec<(Range<usize>, String)> = go.into_iter().map(|h| (h.range, h.with)).collect();
            crate::spell::apply_changes(s, &key, &edits);
            // the ones still held, after them on the line he types on, move with them (the rightmost first, so each
            // range is still the text's before it)
            for (r, w) in edits.iter().rev() {
                shift_held(s, &key, r, w.len());
            }
            changed = true;
        }
    }
    changed
}

/// Box KEY with text TEXT and its caret at AT, focused, typed just now, for a test.
#[cfg(test)]
pub(crate) fn boxed(s: &mut App, key: &(usize, String), text: &str, at: usize) {
    let mut e = rune::TextEdit::new(text);
    e.set_caret(at);
    e.focused = true;
    s.composers.insert(key.clone(), e);
    typed_now(s, key);
}

/// He typed in box KEY just now, as the poll would have seen it.
#[cfg(test)]
pub(crate) fn typed_now(s: &mut App, key: &(usize, String)) {
    let e = &s.composers[key];
    s.changed.seen.insert(key.clone(), crate::changed::Seen { text: e.text.clone(), since: std::time::Instant::now(), caret: e.focus, typed: true });
}

/// He stopped typing in box KEY a pause ago.
#[cfg(test)]
pub(crate) fn stopped(s: &mut App, key: &(usize, String)) {
    let e = &s.composers[key];
    let since = std::time::Instant::now().checked_sub(crate::changed::PAUSE * 2).unwrap_or_else(std::time::Instant::now);
    s.changed.seen.insert(key.clone(), crate::changed::Seen { text: e.text.clone(), since, caret: e.focus, typed: false });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none() -> Vec<&'static BTreeMap<String, u32>> {
        Vec::new()
    }

    /// `complete` with SymSpell's list built, as a few seconds after the window's start (asking builds it here).
    fn complete(prefix: &str, context: &[&BTreeMap<String, u32>]) -> Option<String> {
        complete_with(prefix, context, true)
    }

    #[test]
    fn the_list_completes_a_word_only_when_one_is_clearly_likeliest() {
        assert_eq!(complete("messa", &none()).as_deref(), Some("message"));
        assert_eq!(complete("becau", &none()).as_deref(), Some("because"));
        assert_eq!(complete("probab", &none()).as_deref(), Some("probably"));
        assert_eq!(complete("shoul", &none()).as_deref(), Some("should"));
        // too short, or no word holds half of what starts so
        assert_eq!(complete("me", &none()), None);
        assert_eq!(complete("pro", &none()), None);
        assert_eq!(complete("compl", &none()), None);
        // one letter more is a completion too: "this" is most of what starts "thi"
        assert_eq!(complete("thi", &none()).as_deref(), Some("this"));
        // the word itself is the likeliest: "the" is not the start of "they"
        assert_eq!(complete("the", &none()), None);
        // "teh" is "the" mistyped, not the start of "tehran"; "yoru" is "you", not "yoruba"
        assert_eq!(complete("teh", &none()), None);
        assert_eq!(complete("yoru", &none()), None);
        // a near word only somewhat commoner leaves it: "windo" is a window, though "wind" is one edit away
        assert_eq!(complete("windo", &none()).as_deref(), Some("window"));
    }

    #[test]
    fn the_words_he_sees_come_before_the_list() {
        // the list's likeliest for "threa" is "threat"; on a thread about threads it is "thread"
        let mut seen = BTreeMap::new();
        count_words("the thread view, and the thread's posts", &mut seen);
        assert_eq!(complete("threa", &[&seen]).as_deref(), Some("thread"));
        assert_eq!(complete("threa", &none()), None);
    }

    #[test]
    fn a_common_word_he_typed_is_not_completed_from_the_thread() {
        // consult-3, N-1396 p73 (b): in his chat "the" showed "thread", "for" "format", "his" "history"
        let mut thread = BTreeMap::new();
        count_words("the thread, its format, his history and the notes on it", &mut thread);
        for w in ["the", "for", "his", "not", "note", "form"] {
            assert_eq!(complete(w, &[&thread]), None, "{w} is likelier as he typed it");
        }
        assert_eq!(complete("thr", &[&thread]).as_deref(), Some("thread"));
        assert_eq!(complete("forma", &[&thread]).as_deref(), Some("format"));
        assert_eq!(complete("hist", &[&thread]).as_deref(), Some("history"), "a rarer word than the completion");
        // a word of the project the list does not hold
        count_words("yaaihh", &mut thread);
        assert_eq!(complete("yaa", &[&thread]).as_deref(), Some("yaaihh"));
    }

    #[test]
    fn no_completion_until_symspells_list_is_built() {
        // the first seconds after the window's start: SymSpell is not asked, so no frame waits for the spelling thread
        // to build it (consult-3, N-1396 p73 (a)); `complete` passes whether it is built (marks.rs `ready`)
        let mut thread = BTreeMap::new();
        count_words("the thread view", &mut thread);
        let t = std::time::Instant::now();
        assert_eq!(complete_with("thr", &[&thread], false), None);
        assert_eq!(complete_with("messa", &none(), false), None);
        assert_eq!(complete_with("teh", &none(), false), None);
        assert!(t.elapsed() < std::time::Duration::from_millis(50), "at once: {:?}", t.elapsed());
        assert_eq!(complete("thr", &[&thread]).as_deref(), Some("thread"));
    }

    #[test]
    fn a_recount_of_a_thread_of_500_posts_is_timed() {
        // consult-3, N-1396 p73 note 1: `thread_words` counts the whole thread again on the first key after a snapshot
        // changed it. 500 posts of 130 words drawn from the whole list: more different words than a real thread has
        let l = list();
        let mut x: u64 = 1;
        let posts: Vec<String> = (0..500)
            .map(|_| {
                (0..130)
                    .map(|_| {
                        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                        l[(x >> 33) as usize % l.len()].0
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        let t = std::time::Instant::now();
        let mut words = BTreeMap::new();
        for p in &posts {
            count_words(p, &mut words);
        }
        let took = t.elapsed();
        println!("recount: {} posts, {} bytes, {} words kept, {:?}", posts.len(), posts.iter().map(String::len).sum::<usize>(), words.len(), took);
        assert!(!words.is_empty());
    }

    fn on() -> (std::sync::MutexGuard<'static, ()>, App, (usize, String)) {
        // SymSpell's list built, as the spelling thread has it a few seconds after the window's start
        let _ = crate::marks::symspell("teh");
        let (held, mut s) = crate::view_cost::test_app();
        s.spell.on = true;
        (held, s, (0, "thread:N-450".to_string()))
    }

    fn press(k: rune::Key) -> rune::KeyEvent {
        rune::KeyEvent { key: k, mods: Default::default() }
    }

    #[test]
    fn a_completion_shows_at_the_caret_and_is_never_in_the_text() {
        let (_held, mut s, k) = on();
        boxed(&mut s, &k, "please send the messa", 21);
        typed_now(&mut s, &k);
        let g = ghost(&s, &k).expect("a completion at the caret");
        let completion = |s: &App| ghost(s, &k).filter(|g| g.kind == Kind::Complete);
        assert_eq!((g.kind, g.source, g.rest(), g.range.clone(), g.with.as_str()), (Kind::Complete, Source::Local, "ge", 16..21, "message"));
        assert_eq!(s.composers[&k].text, "please send the messa", "the ghost is not in the text");
        // he types on: the ghost follows what he typed, from the same text
        s.composers.get_mut(&k).unwrap().insert("g");
        assert_eq!(ghost(&s, &k).map(|g| g.rest().to_string()).as_deref(), Some("e"));
        // his capital stays his
        boxed(&mut s, &k, "Becau", 5);
        assert_eq!(ghost(&s, &k).map(|g| g.with).as_deref(), Some("Because"));
        // text after the caret on its line: none (docket-edge5 draws over the line); on the next line it is fine
        boxed(&mut s, &k, "the messa here", 9);
        assert_eq!(completion(&s), None);
        boxed(&mut s, &k, "the messa  \nhere", 9);
        assert!(ghost(&s, &k).is_some());
        // a selection, a box without the keyboard, the mode off, a path or a name: none
        s.composers.get_mut(&k).unwrap().set_selection(4, 9);
        assert_eq!(ghost(&s, &k), None, "text selected");
        boxed(&mut s, &k, "the messa", 9);
        s.composers.get_mut(&k).unwrap().focused = false;
        assert_eq!(ghost(&s, &k), None, "not focused");
        boxed(&mut s, &k, "the messa", 9);
        s.spell.on = false;
        assert_eq!(ghost(&s, &k), None, "the mode off");
        s.spell.on = true;
        boxed(&mut s, &k, "see src/messa", 13);
        assert_eq!(ghost(&s, &k), None, "a path");
        boxed(&mut s, &k, "```\nthe messa", 13);
        assert_eq!(ghost(&s, &k), None, "code");
    }

    #[test]
    fn tab_takes_the_ghost_as_one_undo_step_and_esc_drops_only_the_ghost() {
        let (_held, mut s, k) = on();
        boxed(&mut s, &k, "the messa", 9);
        assert!(key(&mut s, &k, &press(rune::Key::Tab)));
        assert_eq!((s.composers[&k].text.as_str(), s.composers[&k].focus), ("the message", 11));
        s.composers.get_mut(&k).unwrap().undo();
        assert_eq!(s.composers[&k].text, "the messa", "one undo takes it back, and what he typed stays");
        // Esc: the ghost goes, his text stays, and it stays gone while he types that word on
        boxed(&mut s, &k, "the messa", 9);
        assert!(key(&mut s, &k, &press(rune::Key::Escape)));
        assert_eq!((ghost(&s, &k), s.composers[&k].text.as_str()), (None, "the messa"));
        s.composers.get_mut(&k).unwrap().insert("g");
        assert_eq!(ghost(&s, &k), None, "dropped for the word he is typing");
        // the next word is offered again
        s.composers.get_mut(&k).unwrap().insert(" becau");
        assert_eq!(ghost(&s, &k).map(|g| g.with).as_deref(), Some("because"));
        // with no ghost, Tab and Esc go on as before (a second Esc does today's thing); a modifier is never ours
        boxed(&mut s, &k, "hi there", 8);
        assert!(!key(&mut s, &k, &press(rune::Key::Tab)));
        assert!(!key(&mut s, &k, &press(rune::Key::Escape)));
        boxed(&mut s, &k, "the becau", 9);
        assert!(key(&mut s, &k, &press(rune::Key::Escape)));
        assert!(!key(&mut s, &k, &press(rune::Key::Escape)), "the second Esc is not ours");
        // he takes the word back and types another there: it is offered
        boxed(&mut s, &k, "the shoul", 9);
        assert_eq!(ghost(&s, &k).map(|g| g.with).as_deref(), Some("should"));
        boxed(&mut s, &k, "the messa", 9);
        let shift_tab = rune::KeyEvent { key: rune::Key::Tab, mods: rune::Mods { shift: true, ..Default::default() } };
        assert!(!key(&mut s, &k, &shift_tab));
        assert_eq!(s.composers[&k].text, "the messa");
    }

    #[test]
    fn a_marked_word_shows_its_fix_once_he_stops_or_moves_there() {
        if !crate::spell::available() {
            return;
        }
        let (_held, mut s, k) = on();
        // the caret in "wrold", which he typed and left
        boxed(&mut s, &k, "hello wrold and more", 8);
        typed_now(&mut s, &k);
        assert_eq!(ghost(&s, &k), None, "not while he types there");
        stopped(&mut s, &k);
        let g = ghost(&s, &k).expect("its fix");
        let first = crate::spell::candidates_at(&s, 0, &s.composers[&k], &(6..11)).into_iter().next();
        assert_eq!((g.kind, g.source, g.range.clone(), Some(g.with.clone())), (Kind::Fix, Source::Local, 6..11, first), "the card's first candidate");
        assert!(key(&mut s, &k, &press(rune::Key::Tab)));
        let e = &s.composers[&k];
        assert_eq!(e.text, format!("hello {} and more", g.with));
        // a changed word: his own is one press away (spell.rs)
        assert_eq!(crate::spell::changed_at(&s, &k, e).into_iter().map(|(_, his)| his).collect::<Vec<_>>(), ["wrold"]);
    }

    #[test]
    fn the_cleanings_answer_on_his_line_waits_while_he_types_and_goes_in_when_he_stops() {
        let (_held, mut s, k) = on();
        boxed(&mut s, &k, "I thnik so", 10);
        crate::changed::asked_for(&mut s, &k, "I thnik so");
        // the answer comes after he typed on, and he is still typing
        s.composers.get_mut(&k).unwrap().insert(" and more");
        typed_now(&mut s, &k);
        crate::changed::answer(&mut s, k.clone(), true, serde_json::json!({"head": "Sure", "cleaned": "I think so"}).to_string());
        assert_eq!((s.composers[&k].text.as_str(), s.composers[&k].focus), ("I thnik so and more", 19), "nothing he typed moves while he types");
        // the caret on the word: the cleaning's fix is its ghost, on the local one's anchor
        s.composers.get_mut(&k).unwrap().set_caret(4);
        typed_now(&mut s, &k);
        let g = ghost(&s, &k).expect("the cleaning's fix");
        assert_eq!((g.kind, g.source, g.range.clone(), g.with.as_str()), (Kind::Fix, Source::Call, 2..7, "think"));
        // he stops: the poll puts it in as a changed word, his caret by the same letters
        s.composers.get_mut(&k).unwrap().set_caret(19);
        stopped(&mut s, &k);
        let mut tx = rune::Tx::default();
        assert!(poll(&mut s, &mut tx));
        assert_eq!((s.composers[&k].text.as_str(), s.composers[&k].focus), ("I think so and more", 19));
        assert_eq!(crate::spell::changed_at(&s, &k, &s.composers[&k]).into_iter().map(|(_, his)| his).collect::<Vec<_>>(), ["thnik"]);
        assert_eq!(ghost(&s, &k), None);
    }

    #[test]
    fn a_held_fix_he_typed_into_is_dropped_and_tab_takes_one() {
        let (_held, mut s, k) = on();
        boxed(&mut s, &k, "I thnik so", 10);
        crate::changed::asked_for(&mut s, &k, "I thnik so");
        s.composers.get_mut(&k).unwrap().insert(" and teh");
        typed_now(&mut s, &k);
        crate::changed::answer(&mut s, k.clone(), true, serde_json::json!({"head": "Sure", "cleaned": "I think so"}).to_string());
        // Tab with the caret on it takes the cleaning's word
        s.composers.get_mut(&k).unwrap().set_caret(5);
        typed_now(&mut s, &k);
        assert!(key(&mut s, &k, &press(rune::Key::Tab)));
        assert_eq!(s.composers[&k].text, "I think so and teh");
        // another answer, and he changes that word himself before he stops: it is his
        boxed(&mut s, &k, "we shoud go", 11);
        crate::changed::asked_for(&mut s, &k, "we shoud go");
        s.composers.get_mut(&k).unwrap().insert(" now");
        typed_now(&mut s, &k);
        crate::changed::answer(&mut s, k.clone(), true, serde_json::json!({"head": "Sure", "cleaned": "we should go"}).to_string());
        assert_eq!(s.composers[&k].text, "we shoud go now");
        let e = s.composers.get_mut(&k).unwrap();
        e.set_caret(8);
        e.insert("l");
        stopped(&mut s, &k);
        let mut tx = rune::Tx::default();
        poll(&mut s, &mut tx);
        assert_eq!(s.composers[&k].text, "we shoudl go now", "his own edit stands");
    }

    #[test]
    fn esc_says_no_to_every_ghost_on_that_word_until_it_changes() {
        let (_held, mut s, k) = on();
        // the cleaning's held fix he says no to does not go in when he stops
        boxed(&mut s, &k, "I thnik so", 10);
        crate::changed::asked_for(&mut s, &k, "I thnik so");
        s.composers.get_mut(&k).unwrap().insert(" and more");
        typed_now(&mut s, &k);
        crate::changed::answer(&mut s, k.clone(), true, serde_json::json!({"head": "Sure", "cleaned": "I think so"}).to_string());
        s.composers.get_mut(&k).unwrap().set_caret(4);
        typed_now(&mut s, &k);
        assert_eq!(ghost(&s, &k).map(|g| g.source), Some(Source::Call));
        assert!(key(&mut s, &k, &press(rune::Key::Escape)));
        stopped(&mut s, &k);
        let mut tx = rune::Tx::default();
        poll(&mut s, &mut tx);
        assert_eq!(s.composers[&k].text, "I thnik so and more", "his word stays as he typed it");
        assert_eq!(ghost(&s, &k), None, "and no local fix is offered on it either");
        if !crate::spell::available() {
            return;
        }
        // a completion he dropped: no fix for the same word after the pause (docket-edge5, N-1426 p5)
        boxed(&mut s, &k, "we wait beca", 12);
        assert_eq!(ghost(&s, &k).map(|g| g.kind), Some(Kind::Complete));
        assert!(key(&mut s, &k, &press(rune::Key::Escape)));
        stopped(&mut s, &k);
        assert_eq!(ghost(&s, &k), None);
        // he typed it on: no completion still, but once he stops the word is not the one he said no to
        s.composers.get_mut(&k).unwrap().insert("u");
        assert_eq!(ghost(&s, &k), None, "its completion stays dropped while he types it on");
        stopped(&mut s, &k);
        assert_eq!(ghost(&s, &k).map(|g| (g.kind, g.word)), Some((Kind::Fix, "becau".to_string())));
    }

    #[test]
    fn a_word_is_the_run_of_letters_at_the_caret() {
        assert_eq!(word_around("say messa", 9), Some(4..9));
        assert_eq!(word_around("say messa", 6), Some(4..9));
        assert_eq!(word_around("say messa ", 10), None);
        assert_eq!(word_around("it's", 4), Some(0..4));
        assert!(ends_line("ab  \nmore", 2));
        assert!(!ends_line("ab cd", 2));
        assert_eq!(line_of("one\ntwo three\nfour", 6), 4..13);
    }
}
