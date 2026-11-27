//! Harper and SymSpell, for what the owner is shown as he writes (the owner, N-481 p10: "harper and symspell for user
//! facing suggestion diagnostics … think grammarly not enforced"; paper-research N-487.p1). Both are pure Rust with
//! their English built in, so they are the same on Linux and Windows:
//! - Harper reads the whole message: its spelling marks and its grammar marks ("a" before a vowel, a word twice,
//!   words often confused), each with what would fix it;
//! - SymSpell offers a word's candidates by how common they are (`assets/spell/en-80k.txt`, SymSpell's own list, MIT).
//!
//! Nothing here changes his words: they are marks and suggestions only.

use harper_core::linting::{LintGroup, LintKind, Linter, Suggestion};
use harper_core::spell::{CanonicalWordId, Dictionary, FstDictionary, FuzzyMatchResult};
use harper_core::{Dialect, DictWordMetadata, Document};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use symspell::{SymSpell, UnicodeStringStrategy, Verbosity};

/// The kinds of Harper's marks he is shown and counted: what touches the meaning. His capitals, spacing and
/// punctuation are his style (on his 27 messages of 18 Nov: 115 capitalisation and 32 formatting marks of 359), not
/// errors, and "grammarly not enforced" (N-481 p10) is no lesson in style.
pub fn shown(kind: &LintKind) -> bool {
    matches!(kind, LintKind::Spelling | LintKind::Typo | LintKind::Grammar | LintKind::Repetition | LintKind::WordChoice | LintKind::Agreement | LintKind::Malapropism | LintKind::Eggcorn | LintKind::BoundaryError)
}

/// One of Harper's marks on a text: where (bytes), of what kind, its words, what would fix it.
#[derive(Clone, Debug)]
pub struct Mark {
    pub range: Range<usize>,
    pub kind: String,
    pub spelling: bool,
    pub message: String,
    pub suggestions: Vec<String>,
}

thread_local! {
    /// Harper's linter, built once per thread (its rules and dictionary, about a second the first time).
    static HARPER: RefCell<Option<LintGroup>> = const { RefCell::new(None) };
}

/// Harper's dictionary, searched no further than three edits from a word (paper-research N-483.p8, 2). Harper's
/// spelling rule looks for a misspelt word's suggestions two edits away, then three, then four; a long word with
/// nothing within three ("yaaiihh", "susbtrheads") made it search the whole dictionary four edits wide: 1.8 s of the
/// 1,990 ms on N-446.p1, for suggestions such as "yacht". With this the word is still marked, without that guess.
struct Near(Arc<FstDictionary>);

/// The widest search Harper's spelling rule makes.
const NEAR: u8 = 3;

impl Dictionary for Near {
    fn contains_word(&self, w: &[char]) -> bool {
        self.0.contains_word(w)
    }
    fn contains_word_str(&self, w: &str) -> bool {
        self.0.contains_word_str(w)
    }
    fn contains_exact_word(&self, w: &[char]) -> bool {
        self.0.contains_exact_word(w)
    }
    fn contains_exact_word_str(&self, w: &str) -> bool {
        self.0.contains_exact_word_str(w)
    }
    fn fuzzy_match(&self, w: &[char], max_distance: u8, max_results: usize) -> Vec<FuzzyMatchResult<'_>> {
        if max_distance > NEAR { Vec::new() } else { self.0.fuzzy_match(w, max_distance, max_results) }
    }
    fn fuzzy_match_str(&self, w: &str, max_distance: u8, max_results: usize) -> Vec<FuzzyMatchResult<'_>> {
        if max_distance > NEAR { Vec::new() } else { self.0.fuzzy_match_str(w, max_distance, max_results) }
    }
    fn get_correct_capitalization_of(&self, w: &[char]) -> Option<&'_ [char]> {
        self.0.get_correct_capitalization_of(w)
    }
    fn get_correct_capitalizations_of(&self, w: &[char]) -> Box<dyn Iterator<Item = &[char]> + Send + '_> {
        self.0.get_correct_capitalizations_of(w)
    }
    fn get_word_metadata(&self, w: &[char]) -> Option<Cow<'_, DictWordMetadata>> {
        self.0.get_word_metadata(w)
    }
    fn get_word_metadata_str(&self, w: &str) -> Option<Cow<'_, DictWordMetadata>> {
        self.0.get_word_metadata_str(w)
    }
    fn get_word_metadata_exact(&self, w: &[char]) -> Option<&DictWordMetadata> {
        self.0.get_word_metadata_exact(w)
    }
    fn words_iter(&self) -> Box<dyn Iterator<Item = &'_ [char]> + Send + '_> {
        self.0.words_iter()
    }
    fn word_count(&self) -> usize {
        self.0.word_count()
    }
    fn get_word_from_id(&self, id: &CanonicalWordId) -> Option<&[char]> {
        self.0.get_word_from_id(id)
    }
    fn find_words_with_prefix(&self, p: &[char]) -> Vec<Cow<'_, [char]>> {
        self.0.find_words_with_prefix(p)
    }
    fn find_words_with_common_prefix(&self, w: &[char]) -> Vec<Cow<'_, [char]>> {
        self.0.find_words_with_common_prefix(w)
    }
}

/// Harper's English: the one the window's own checker takes (hunspell's `en_US` on Linux; on Windows the first English
/// among his display languages), else American (paper-research N-487.p51: not British by itself).
pub fn dialect_of(tag: Option<&str>) -> Dialect {
    let region = tag.map(|t| t.replace('_', "-").to_uppercase()).unwrap_or_default();
    match region.split('-').nth(1).unwrap_or("") {
        "GB" | "IE" | "NZ" | "ZA" => Dialect::British,
        "AU" => Dialect::Australian,
        "CA" => Dialect::Canadian,
        "IN" => Dialect::Indian,
        _ => Dialect::American,
    }
}

/// The dialect, decided once.
pub fn dialect() -> Dialect {
    static D: OnceLock<Dialect> = OnceLock::new();
    *D.get_or_init(|| dialect_of(crate::spell::english().as_deref()))
}

/// Harper's marks on TEXT, read as Markdown (code is not checked), in [`dialect`]'s English.
pub fn harper(text: &str) -> Vec<Mark> {
    let doc = Document::new_markdown_default_curated(text);
    let lints = HARPER.with(|h| {
        let mut h = h.borrow_mut();
        let g = h.get_or_insert_with(|| LintGroup::new_curated(Arc::new(Near(FstDictionary::curated())), dialect()));
        g.lint(&doc)
    });
    // Harper counts in chars; the box in bytes
    let bytes: Vec<usize> = text.char_indices().map(|(i, _)| i).chain(std::iter::once(text.len())).collect();
    let at = |c: usize| bytes.get(c).copied().unwrap_or(text.len());
    let mut out: Vec<Mark> = lints
        .into_iter()
        .filter(|l| shown(&l.lint_kind))
        .map(|l| {
            let range = at(l.span.start)..at(l.span.end);
            let suggestions = l
                .suggestions
                .iter()
                .map(|s| match s {
                    Suggestion::ReplaceWith(c) => c.iter().collect(),
                    Suggestion::InsertAfter(c) => format!("{}{}", &text[range.clone()], c.iter().collect::<String>()),
                    Suggestion::Remove => String::new(),
                })
                .collect();
            Mark { range, kind: format!("{:?}", l.lint_kind), spelling: l.lint_kind == LintKind::Spelling, message: l.message, suggestions }
        })
        .collect();
    out.sort_by_key(|m| (m.range.start, m.range.end));
    out
}

/// What the box shows for a text: Harper's shown marks, each spelling mark's candidates ranked by Harper and SymSpell
/// together (`ranked`), and how long it took.
pub type Marks = Arc<Vec<Mark>>;

/// Harper's marks on TEXT with the candidates the box offers: for a spelling mark Harper's and SymSpell's ranked
/// together (on his 40 typos: right first 26 times, Harper alone 25, SymSpell alone 19); for the others Harper's.
pub fn checked(text: &str) -> Vec<Mark> {
    let mut out = harper(text);
    for m in out.iter_mut().filter(|m| m.spelling) {
        let w = &text[m.range.clone()];
        m.suggestions = offered(w, &m.suggestions, &symspell(w));
    }
    out
}

/// The candidates of LISTS (best first in each) by their ranks summed (3 for a first, 2, 1), the first list's order
/// on a tie, then the rest of each list, no word twice: the released order, kept to measure `offered` against.
#[cfg(test)]
fn ranked(lists: &[Vec<String>]) -> Vec<String> {
    let mut all: Vec<String> = Vec::new();
    for l in lists {
        for w in l {
            if !all.iter().any(|a| a.eq_ignore_ascii_case(w)) {
                all.push(w.clone());
            }
        }
    }
    let score = |w: &String| -> usize { lists.iter().map(|l| l.iter().take(3).position(|x| x.eq_ignore_ascii_case(w)).map_or(0, |i| 3 - i)).sum() };
    let mut scored: Vec<(usize, usize, String)> = all.into_iter().enumerate().map(|(i, w)| (score(&w), i, w)).collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, w)| w).collect()
}

/// The candidates the box offers for WORD from Harper's (HARPER) and SymSpell's (SYM), best first (paper-research
/// N-487.p83, measured on his 40 typos and 35 misspelt words of his 27 messages; candidate_orders): the nearest first,
/// a swap of two neighbours one edit (wrod: word), the commoner word (SymSpell's count) first among the equally near,
/// a split ("diction y", "an-gets") after every whole word. First place: 27 of 40 and 32 of 35, against 25 and 28 for
/// the ranked order it replaces; the same on Windows and Linux, as it reads no platform checker.
pub fn offered(word: &str, harper: &[String], sym: &[String]) -> Vec<String> {
    let mut all: Vec<String> = Vec::new();
    for w in harper.iter().chain(sym) {
        if !all.iter().any(|a| a.eq_ignore_ascii_case(w)) {
            all.push(w.clone());
        }
    }
    let lower = word.to_lowercase();
    let split = |w: &str| w.contains(' ') || (w.contains('-') && !word.contains('-'));
    // among equally near words, one that keeps every typed letter in order first (only letters added: "mispeled" →
    // "misspelled" before "misled", which drops two; docket-edge N-412.p388), then the commoner
    // a swap of neighbouring letters keeps them too ("teh" → "the" ties with "tech", and the commoner wins as before;
    // consult-4 N-584.p36)
    let sorted = |w: &str| {
        let mut v: Vec<char> = w.chars().collect();
        v.sort_unstable();
        v
    };
    let keeps = |w: &str| {
        let w = w.to_lowercase();
        let mut it = w.chars().collect::<Vec<_>>().into_iter();
        sorted(&w) == sorted(&lower) || lower.chars().all(|c| it.any(|x| x == c))
    };
    let mut keyed: Vec<(bool, usize, bool, std::cmp::Reverse<i64>, usize, String)> =
        all.into_iter().enumerate().map(|(i, w)| (split(&w), crate::spell::distance(&lower, &w.to_lowercase()), !keeps(&w), std::cmp::Reverse(count(&w)), i, w)).collect();
    keyed.sort();
    keyed.into_iter().map(|k| k.5).collect()
}

/// How common WORD is in SymSpell's list (0 when not in it).
fn count(word: &str) -> i64 {
    sym().lookup(&word.to_lowercase(), Verbosity::Top, 0).first().filter(|s| s.distance == 0).map_or(0, |s| s.count)
}

/// The checker's thread: Harper takes 3 to 30 ms on most of his messages but up to 2 s on one (N-483 post 7), so it
/// never runs in a frame. Each text is checked once; while it works, only the latest text asked for waits.
struct Worker {
    ask: mpsc::Sender<String>,
    done: Arc<Mutex<HashMap<String, Marks>>>,
    /// The texts asked for and not yet checked or passed over; the thread takes each out as it checks or passes it,
    /// so a text whose marks were let go (KEPT) is asked for again, not waited on for ever.
    waiting: Arc<Mutex<HashSet<String>>>,
}

thread_local! {
    static WORKER: RefCell<Option<Worker>> = const { RefCell::new(None) };
}

/// What wakes the window when marks are ready (none headless: a test asks with `wait`).
static WAKE: OnceLock<rune::Waker> = OnceLock::new();

pub fn set_waker(w: rune::Waker) {
    let _ = WAKE.set(w);
}

/// How many texts' marks are kept: the box's last few, the others' as he moves between boxes.
const KEPT: usize = 32;

fn start() -> Worker {
    let (ask, rx) = mpsc::channel::<String>();
    let done: Arc<Mutex<HashMap<String, Marks>>> = Arc::default();
    let out = done.clone();
    let waiting: Arc<Mutex<HashSet<String>>> = Arc::default();
    let pending = waiting.clone();
    let _ = std::thread::Builder::new().name("spelling".into()).spawn(move || {
        // SymSpell's list and Harper's rules, built here before any text comes, never in a frame (paper-research
        // N-487.p51: how long it takes at the window's start, and that the first frame does not wait on it)
        // the completions' list first (suggest.rs; tens of ms), so the first completion's frame does not read it
        crate::suggest::warm();
        let t = std::time::Instant::now();
        let _ = sym();
        let list = t.elapsed().as_millis();
        let _ = harper("warm");
        crate::log::line(&format!("spelling: SymSpell's list read in {list} ms, Harper's rules ready {} ms later, off the window's thread", t.elapsed().as_millis() - list));
        while let Ok(mut text) = rx.recv() {
            // only the latest: what he typed since has replaced it
            while let Ok(t) = rx.try_recv() {
                pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&text);
                text = t;
            }
            let t = std::time::Instant::now();
            let marks = Arc::new(checked(&text));
            let ms = t.elapsed().as_millis();
            if ms > 500 {
                crate::log::line(&format!("spelling: Harper took {ms} ms on {} characters", text.chars().count()));
            }
            let mut d = out.lock().unwrap_or_else(|e| e.into_inner());
            if d.len() >= KEPT {
                d.clear();
            }
            pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&text);
            d.insert(text, marks);
            drop(d);
            if let Some(w) = WAKE.get() {
                w();
            }
        }
    });
    Worker { ask, done, waiting }
}

/// Start the checker's thread at the window's start, so the lists are read before the first text is asked for; the
/// frame does not wait on it.
pub fn warm() {
    WORKER.with(|w| {
        w.borrow_mut().get_or_insert_with(start);
    });
}

/// The marks of TEXT if they are ready; else it is asked for (once) and none yet: the window is woken when they come.
pub fn of(text: &str) -> Option<Marks> {
    if text.trim().is_empty() {
        return Some(Marks::default());
    }
    WORKER.with(|w| {
        let mut w = w.borrow_mut();
        let w = w.get_or_insert_with(start);
        if let Some(m) = w.done.lock().unwrap_or_else(|e| e.into_inner()).get(text) {
            return Some(m.clone());
        }
        if w.waiting.lock().unwrap_or_else(|e| e.into_inner()).insert(text.to_string()) {
            let _ = w.ask.send(text.to_string());
        }
        None
    })
}

/// For tests: the marks of TEXT, waiting for the checker's thread up to 60 s (a debug build loads its lists slowly).
#[cfg(test)]
pub fn wait(text: &str) -> Marks {
    for _ in 0..6000 {
        if let Some(m) = of(text) {
            return m;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("no marks for {text:?} in 60 s")
}

/// SymSpell's English list: 80,000 words, each with its count, one a line (also suggest.rs's completions).
pub(crate) const EN_80K: &str = include_str!("../assets/spell/en-80k.txt");

static SYM: OnceLock<SymSpell<UnicodeStringStrategy>> = OnceLock::new();

/// SymSpell over SymSpell's English list (80,000 words with their counts), built once (about half a second; 2 to 8 s
/// at his window's start, on the spelling thread).
fn sym() -> &'static SymSpell<UnicodeStringStrategy> {
    SYM.get_or_init(read_list)
}

/// Whether SymSpell's list is built: until it is, asking `symspell` waits for the spelling thread to build it.
pub(crate) fn ready() -> bool {
    SYM.get().is_some()
}

fn read_list() -> SymSpell<UnicodeStringStrategy> {
    // a prefix of 10, not SymSpell's 7: with 7 a word past it loses candidates its suffix differs in ("dictiony"
    // found no "dictionary", N-487.p73)
    let mut s: SymSpell<UnicodeStringStrategy> = symspell::SymSpellBuilder::default().max_dictionary_edit_distance(2).prefix_length(10).build().expect("SymSpell's settings");
    for line in EN_80K.lines() {
        s.load_dictionary_line(line, 0, 1, " ");
    }
    s
}

/// SymSpell's candidates for WORD, nearest first and the most common first among the nearest (two edits at most), in
/// WORD's case: none when it is a word of its list.
pub fn symspell(word: &str) -> Vec<String> {
    let lower = word.to_lowercase();
    let found = sym().lookup(&lower, Verbosity::All, 2);
    if found.first().is_some_and(|s| s.distance == 0) {
        return Vec::new();
    }
    let cap = word.chars().next().is_some_and(char::is_uppercase);
    found
        .into_iter()
        .map(|s| {
            if cap {
                let mut c = s.term.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            } else {
                s.term
            }
        })
        .collect()
}

/// How much of a message is wrong, as one number and its parts: Harper's marks per 100 words (the kinds he is shown:
/// spelling and what touches the meaning), with the words and the marks it is made of.
#[derive(Clone, Debug, PartialEq)]
pub struct Bad {
    pub words: usize,
    pub spelling: usize,
    pub grammar: usize,
    /// Marks per 100 words.
    pub per100: f32,
}

pub fn bad(text: &str) -> Bad {
    let words = text.split_whitespace().filter(|w| w.chars().any(char::is_alphabetic)).count();
    let marks = harper(text);
    let spelling = marks.iter().filter(|m| m.spelling).count();
    let grammar = marks.len() - spelling;
    let per100 = if words == 0 { 0.0 } else { 100.0 * marks.len() as f32 / words as f32 };
    Bad { words, spelling, grammar, per100 }
}

/// How sure a word must be before it is put in by itself (paper-research N-483.p8, 1): the rules measured.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub enum Sure {
    /// Harper and SymSpell put the same word first.
    Agree,
    /// And no other word is as near: none of SymSpell's 80,000 within as many edits, none of Harper's, and no other
    /// mark of Harper's on the same words (onlyif: only, or "only if").
    Alone,
    /// And that word is one edit away (a swap is one).
    AloneOne,
    /// Alone, and no word one edit further out is more common (actuayl: actual, but actually; frither: fritter, but
    /// further), and the fix does not take a letter that belongs to the word beside it (fro mpolicy: from policy;
    /// al lthis: all this).
    Common,
}

/// The words of SymSpell's list within two edits of WORD, with how common each is.
#[cfg_attr(not(test), allow(dead_code))]
fn near(word: &str) -> Vec<(String, i64)> {
    sym().lookup(&word.to_lowercase(), Verbosity::All, 2).into_iter().map(|s| (s.term, s.count)).collect()
}

/// Whether WORD is a word of SymSpell's list.
#[cfg_attr(not(test), allow(dead_code))]
fn listed(word: &str) -> bool {
    sym().lookup(&word.to_lowercase(), Verbosity::Top, 0).first().is_some_and(|s| s.distance == 0)
}

/// The word put in for WORD by RULE, if any: HARPER is Harper's suggestions for it, OTHERS whether another of
/// Harper's marks is on the same words, BEFORE and AFTER the words beside it.
#[cfg_attr(not(test), allow(dead_code))]
pub fn certain(word: &str, harper: &[String], others: bool, before: &str, after: &str, rule: Sure) -> Option<String> {
    use crate::spell::distance;
    let lower = word.to_lowercase();
    let near = near(word);
    if near.iter().any(|t| t.0 == lower) {
        return None;
    }
    let h = harper.first()?;
    let y = symspell(word).into_iter().next()?;
    if !h.eq_ignore_ascii_case(&y) {
        return None;
    }
    if rule == Sure::Agree {
        return Some(h.clone());
    }
    if others {
        return None;
    }
    let d = distance(word, h);
    let rivals = |within: usize| {
        near.iter().map(|t| &t.0).chain(harper.iter()).filter(|t| !t.eq_ignore_ascii_case(h) && distance(word, t) <= within).count()
    };
    if rivals(d) > 0 || (rule == Sure::AloneOne && d != 1) {
        return None;
    }
    if rule == Sure::Common {
        let count = near.iter().find(|t| t.0.eq_ignore_ascii_case(h)).map_or(0, |t| t.1);
        if near.iter().any(|t| t.1 > count && distance(word, &t.0) == d + 1) {
            return None;
        }
        let hl = h.to_lowercase();
        let (first, last) = (lower.chars().next()?, lower.chars().last()?);
        if lower[first.len_utf8()..] == hl && !before.is_empty() && listed(&format!("{before}{first}")) {
            return None;
        }
        if lower[..lower.len() - last.len_utf8()] == hl && !after.is_empty() && listed(&format!("{last}{after}")) {
            return None;
        }
    }
    Some(h.clone())
}

/// The box's answer to the words kind's `judge` step (paper-research N-487 p10, p20): TEXT with the certain fixes made
/// (`Sure::Common`: Harper and SymSpell put the same word first, no other word as near, none more common one edit
/// further, no letter taken from the word beside it; on his 40 typos 12 changed, 0 wrong; on his 27 messages of 18 Nov
/// 14 changed, 0 wrong), and Harper's shown marks still on the fixed words per 100 words. LEAVE: the byte ranges not
/// to touch (code, pills); KEEP: words never changed (the project's words and his own, lower case).
#[cfg_attr(not(test), allow(dead_code))]
pub fn judge(text: &str, leave: &[Range<usize>], keep: &std::collections::HashSet<String>) -> (String, i64) {
    let marks = harper(text);
    let mut fixed = String::with_capacity(text.len());
    let mut at = 0;
    for (i, m) in marks.iter().enumerate().filter(|(_, m)| m.spelling) {
        let w = &text[m.range.clone()];
        if m.range.start < at || leave.iter().any(|k| k.start < m.range.end && m.range.start < k.end) || keep.contains(&w.to_lowercase()) {
            continue;
        }
        let others = marks.iter().enumerate().any(|(j, o)| j != i && o.range.start < m.range.end && m.range.start < o.range.end);
        let word = |s: &str| s.chars().filter(|c| c.is_alphabetic() || *c == '\'').collect::<String>();
        let before = word(text[..m.range.start].split_whitespace().last().unwrap_or(""));
        let after = word(text[m.range.end..].split_whitespace().next().unwrap_or(""));
        if let Some(f) = certain(w, &m.suggestions, others, &before, &after, Sure::Common) {
            fixed.push_str(&text[at..m.range.start]);
            fixed.push_str(&f);
            at = m.range.end;
        }
    }
    fixed.push_str(&text[at..]);
    // what is left, his kept words not counted (Harper reads code as code and leaves it)
    let words = fixed.split_whitespace().filter(|w| w.chars().any(char::is_alphabetic)).count();
    let left = harper(&fixed).into_iter().filter(|m| !(m.spelling && keep.contains(&fixed[m.range.clone()].to_lowercase()))).count();
    let per100 = if words == 0 { 0 } else { (100.0 * left as f32 / words as f32).round() as i64 };
    (fixed, per100)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window's start: `warm` only starts the checker's thread, so the frame that calls it does not wait on the
    /// lists; the time to read SymSpell's list and build Harper's rules is printed (paper-research N-487.p51).
    #[test]
    fn warming_does_not_hold_the_frame() {
        let t = std::time::Instant::now();
        warm();
        let call = t.elapsed();
        assert!(call < std::time::Duration::from_millis(50), "warm took {call:?} on the calling thread");
        let t = std::time::Instant::now();
        let list = read_list();
        let read = t.elapsed();
        assert!(!list.lookup("teh", Verbosity::Top, 2).is_empty());
        let t = std::time::Instant::now();
        let _ = LintGroup::new_curated(Arc::new(Near(FstDictionary::curated())), dialect());
        println!("WARM call {} us; SymSpell list {} ms; Harper rules {} ms", call.as_micros(), read.as_millis(), t.elapsed().as_millis());
    }

    #[test]
    fn the_dialect_is_the_checkers_english_else_american() {
        assert_eq!(dialect_of(Some("en_US")), Dialect::American);
        assert_eq!(dialect_of(Some("en-GB")), Dialect::British);
        assert_eq!(dialect_of(Some("en_AU")), Dialect::Australian);
        assert_eq!(dialect_of(Some("en-CA")), Dialect::Canadian);
        assert_eq!(dialect_of(Some("en")), Dialect::American);
        assert_eq!(dialect_of(None), Dialect::American);
        // on Linux the checker reads en_US
        #[cfg(not(windows))]
        assert_eq!(dialect(), Dialect::American);
        // "colour" is his British spelling: not British by itself any more
        assert!(harper("the colour of it").iter().any(|m| m.spelling));
    }

    #[test]
    fn judge_makes_only_the_certain_fixes() {
        let keep: std::collections::HashSet<String> = ["yaaihh".to_string()].into();
        let text = "it does ntohing becuase yaaihh is fro mpolicy `ncie`";
        let code = text.find('`').unwrap()..text.len();
        let (fixed, per100) = judge(text, &[code], &keep);
        // ntohing and becuase are certain; fro mpolicy takes a letter of the word beside it; code and kept words stay
        assert_eq!(fixed, "it does nothing because yaaihh is fro mpolicy `ncie`");
        assert!(per100 > 0, "the marks left are counted: {per100}");
        assert_eq!(judge("all fine here", &[], &keep), ("all fine here".to_string(), 0));
    }

    const RULES: [Sure; 4] = [Sure::Agree, Sure::Alone, Sure::AloneOne, Sure::Common];

    /// Each rule on the typos in SPELL_TYPOS (TYPO, tab, meant): changed, and of those wrong; then on the messages
    /// in SPELL_MESSAGES (ID, tab, text) each change with the words around it, to be judged by hand (paper-research
    /// N-483.p8, 1). `SPELL_TYPOS=F SPELL_MESSAGES=G cargo test certain_fixes -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn certain_fixes_on_typos_and_messages() {
        if let Ok(file) = std::env::var("SPELL_TYPOS") {
            let text = std::fs::read_to_string(file).unwrap();
            let mut c = [(0, 0); RULES.len()];
            for line in text.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
                let (typo, meant) = line.split_once('\t').unwrap_or((line, ""));
                let marks = harper(typo);
                let hm = marks.iter().find(|m| m.spelling && m.range == (0..typo.len())).map(|m| m.suggestions.clone()).unwrap_or_default();
                let others = marks.iter().filter(|m| m.range.start < typo.len()).count() > 1;
                let got: Vec<String> = RULES.iter().map(|r| certain(typo, &hm, others, "", "", *r).unwrap_or_default()).collect();
                for (g, c) in got.iter().zip(c.iter_mut()) {
                    if !g.is_empty() {
                        c.0 += 1;
                        if !g.eq_ignore_ascii_case(meant) {
                            c.1 += 1;
                        }
                    }
                }
                println!("TYPO\t{typo}\t{meant}\t{}", got.join("\t"));
            }
            for (r, (n, wrong)) in RULES.iter().zip(c) {
                println!("TYPOS\t{r:?}\tchanged {n}\twrong {wrong}");
            }
        }
        if let Ok(file) = std::env::var("SPELL_MESSAGES") {
            let text = std::fs::read_to_string(file).unwrap();
            let mut n = [0; RULES.len()];
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let (id, msg) = line.split_once('\t').unwrap_or(("", line));
                let marks = harper(msg);
                for (i, m) in marks.iter().enumerate().filter(|(_, m)| m.spelling) {
                    let others = marks.iter().enumerate().any(|(j, o)| j != i && o.range.start < m.range.end && m.range.start < o.range.end);
                    let w = &msg[m.range.clone()];
                    let word = |s: &str| s.chars().filter(|c| c.is_alphabetic() || *c == '\'').collect::<String>();
                    let before = word(msg[..m.range.start].split_whitespace().last().unwrap_or(""));
                    let after = word(msg[m.range.end..].split_whitespace().next().unwrap_or(""));
                    let got: Vec<String> = RULES.iter().map(|r| certain(w, &m.suggestions, others, &before, &after, *r).unwrap_or_default()).collect();
                    if got[0].is_empty() {
                        continue;
                    }
                    for (g, n) in got.iter().zip(n.iter_mut()) {
                        *n += usize::from(!g.is_empty());
                    }
                    let fl = |mut i: usize| {
                        while !msg.is_char_boundary(i) {
                            i -= 1;
                        }
                        i
                    };
                    let around = &msg[fl(m.range.start.saturating_sub(30))..fl((m.range.end + 30).min(msg.len()))];
                    println!("CHANGE\t{id}\t{w}\t{}\t{around}", got.join("\t"));
                }
            }
            for (r, n) in RULES.iter().zip(n) {
                println!("MESSAGES\t{r:?}\tchanged {n}");
            }
        }
    }

    #[test]
    fn harper_marks_a_misspelt_word_and_a_word_twice() {
        let t = "I tihnk the the box is fine.";
        let m = harper(t);
        assert!(m.iter().any(|m| m.spelling && &t[m.range.clone()] == "tihnk" && m.suggestions.iter().any(|s| s == "think")), "{m:?}");
        assert!(m.iter().any(|m| !m.spelling && t[m.range.clone()].contains("the the")), "{m:?}");
    }

    #[test]
    fn the_checkers_thread_answers_and_the_two_lists_are_ranked_together() {
        let m = wait("I tihnk so");
        assert!(m.iter().any(|m| m.spelling && m.range == (2..7) && m.suggestions.first().map(String::as_str) == Some("think")), "{m:?}");
        // a word both put first wins; one only Harper has comes before one only SymSpell has, on a tie
        let r = ranked(&[vec!["a".into(), "b".into()], vec!["b".into(), "c".into()]]);
        assert_eq!(r, ["b", "a", "c"]);
    }

    #[test]
    fn symspell_offers_the_common_word_first() {
        // nearest first: trough (one swap) before through (two edits); among the nearest, the most common
        assert_eq!(symspell("torugh").first().map(String::as_str), Some("trough"));
        assert!(symspell("through").is_empty());
        assert_eq!(symspell("Seperate").first().map(String::as_str), Some("Separate"));
    }

    /// The orders of candidates tried (paper-research N-487.p83), each from Harper's (H), SymSpell's (Y) and the
    /// machine's checker's (C) for the word alone: for each set (TSV, typo and the word meant) how often the meant word
    /// is first and within the first three. `SPELL_TYPOS=FILE SPELL_MEANT=FILE cargo test candidate_orders -- --ignored
    /// --nocapture`.
    #[test]
    #[ignore]
    fn candidate_orders() {
        type Order = fn(&str, &[String], &[String], &[String]) -> Vec<String>;
        fn union(ls: &[&[String]]) -> Vec<String> {
            let mut all: Vec<String> = Vec::new();
            for l in ls {
                for w in l.iter() {
                    if !all.iter().any(|a| a.eq_ignore_ascii_case(w)) {
                        all.push(w.clone());
                    }
                }
            }
            all
        }
        fn by_distance(word: &str, all: Vec<String>, tie: &dyn Fn(&String) -> usize) -> Vec<String> {
            let lower = word.to_lowercase();
            let mut k: Vec<(bool, usize, usize, std::cmp::Reverse<i64>, usize, String)> = all
                .into_iter()
                .enumerate()
                .map(|(i, w)| (w.contains(' '), crate::spell::distance(&lower, &w.to_lowercase()), tie(&w), std::cmp::Reverse(count(&w)), i, w))
                .collect();
            k.sort();
            k.into_iter().map(|x| x.5).collect()
        }
        let orders: Vec<(&str, Order)> = vec![
            ("ranked (released)", |_, h, y, _| ranked(&[h.to_vec(), y.to_vec()])),
            ("offered (the box: distance, Harper and SymSpell, commoner, splits last)", |w, h, y, _| offered(w, h, y)),
            ("checker first, then ranked", |_, h, y, c| union(&[c, &ranked(&[h.to_vec(), y.to_vec()])])),
            ("distance (swap = 1), commoner, splits last", |w, h, y, c| by_distance(w, union(&[h, y, c]), &|_| 0)),
            ("checker first, then distance, commoner", |w, h, y, c| union(&[c, &by_distance(w, union(&[h, y]), &|_| 0)])),
            ("distance, then checker's place, commoner", |w, h, y, c| {
                let cc = c.to_vec();
                by_distance(w, union(&[h, y, c]), &move |x: &String| cc.iter().position(|z| z.eq_ignore_ascii_case(x)).unwrap_or(99))
            }),
            ("distance, Harper and SymSpell only, commoner", |w, h, y, _| by_distance(w, union(&[h, y]), &|_| 0)),
        ];
        for var in ["SPELL_TYPOS", "SPELL_MEANT"] {
            let Ok(file) = std::env::var(var) else { continue };
            let text = std::fs::read_to_string(file).unwrap();
            let pairs: Vec<(String, String)> = text.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()).filter_map(|l| l.split_once('\t')).map(|(a, b)| (a.to_string(), b.to_string())).collect();
            let lists: Vec<(Vec<String>, Vec<String>, Vec<String>)> = pairs
                .iter()
                .map(|(t, _)| (harper(t).into_iter().find(|m| m.range == (0..t.len())).map(|m| m.suggestions).unwrap_or_default(), symspell(t), crate::spell::candidates(t)))
                .collect();
            for (name, f) in &orders {
                let (mut first, mut three) = (0, 0);
                for ((t, meant), (h, y, c)) in pairs.iter().zip(&lists) {
                    let got = f(t, h, y, c);
                    let at = got.iter().position(|g| g.eq_ignore_ascii_case(meant));
                    first += usize::from(at == Some(0));
                    three += usize::from(at.is_some_and(|a| a < 3));
                    if at != Some(0) {
                        println!("MISS\t{var}\t{name}\t{t}\t{meant}\t{}", got.iter().take(5).cloned().collect::<Vec<_>>().join(", "));
                    }
                }
                println!("ORDER\t{var}\t{}\t{name}\tfirst {first}\tfirst three {three}", pairs.len());
            }
        }
    }

    /// A text whose marks were let go (the kept marks are emptied after KEPT texts, while the box may still show it)
    /// is asked for again: before, it stayed in the list of texts asked for and was never checked again, so its words
    /// kept only the machine's checker's candidates until he typed (seen in the N-515 stills).
    #[test]
    fn marks_let_go_are_asked_for_again() {
        let text = "a text whose marks are let go, dictiony";
        assert!(!wait(text).is_empty());
        WORKER.with(|w| w.borrow().as_ref().unwrap().done.lock().unwrap().clear());
        assert!(wait(text).iter().any(|m| m.suggestions.iter().any(|w| w == "dictionary")));
    }

    /// His own example (N-538): "mispeled" offers "misspelled" first, before "misled" (both two edits away).
    #[test]
    fn mispeled_offers_misspelled_first() {
        let sym = symspell("mispeled");
        let got = offered("mispeled", &["misled".into(), "misspelled".into()], &sym);
        assert_eq!(got.first().map(String::as_str), Some("misspelled"), "{got:?}");
        // his commonest swap still offers the common word first (consult-4 N-584.p36)
        let sym = symspell("teh");
        let got = offered("teh", &["tech".into(), "teth".into(), "the".into()], &sym);
        assert_eq!(got.first().map(String::as_str), Some("the"), "{got:?}");
        let sym = symspell("hte");
        let got = offered("hte", &["hate".into(), "the".into()], &sym);
        assert_eq!(got.first().map(String::as_str), Some("the"), "{got:?}");
    }

    /// His own example (N-487.p73): "dictiony" finds "dictionary"; whole words within two edits before a split, at
    /// most five shown.
    #[test]
    fn dictiony_finds_dictionary_and_splits_come_last() {
        let sym = symspell("dictiony");
        assert!(sym.iter().any(|w| w == "dictionary"), "SymSpell: {sym:?}");
        let got = offered("dictiony", &["diction".into(), "diction y".into(), "dictation".into()], &sym);
        let five: Vec<&str> = got.iter().take(5).map(String::as_str).collect();
        assert!(five.contains(&"dictionary"), "{got:?}");
        let split = got.iter().position(|w| w == "diction y").unwrap();
        assert!(got[..split].iter().all(|w| !w.contains(' ')) && split >= 3, "{got:?}");
    }

    /// Harper's and SymSpell's first candidates for each typo in SPELL_TYPOS (TYPO, tab, meant; # comments), and the
    /// counts (paper-research N-487.p1, 2): `SPELL_TYPOS=FILE cargo test typos_by_harper -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn typos_by_harper_and_symspell() {
        let Ok(file) = std::env::var("SPELL_TYPOS") else { return };
        let text = std::fs::read_to_string(file).unwrap();
        let (mut h, mut y, mut b, mut r) = ([0; 3], [0; 3], [0; 3], [0; 3]);
        let count = |c: &mut [usize; 3], first: Option<&String>, meant: &str| match first {
            None => c[2] += 1,
            Some(f) if f.eq_ignore_ascii_case(meant) => c[0] += 1,
            Some(_) => c[1] += 1,
        };
        for line in text.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
            let (typo, meant) = line.split_once('\t').unwrap_or((line, ""));
            // the word alone, as the box sees it once he has typed it
            let hm: Vec<String> = harper(typo).into_iter().find(|m| m.range == (0..typo.len())).map(|m| m.suggestions).unwrap_or_default();
            let ys = symspell(typo);
            count(&mut h, hm.first(), meant);
            count(&mut y, ys.first(), meant);
            let box_first = offered(typo, &hm, &ys);
            count(&mut b, box_first.first(), meant);
            count(&mut r, ranked(&[hm.clone(), ys.clone()]).first(), meant);
            let three = |v: &[String]| v.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
            println!("TYPO\t{typo}\t{meant}\t{}\t{}", three(&hm), three(&ys));
        }
        println!("COUNT\tHarper\tright {}\twrong {}\tnothing {}", h[0], h[1], h[2]);
        println!("COUNT\tSymSpell\tright {}\twrong {}\tnothing {}", y[0], y[1], y[2]);
        println!("COUNT\tBox\tright {}\twrong {}\tnothing {}", b[0], b[1], b[2]);
        println!("COUNT\tRanked\tright {}\twrong {}\tnothing {}", r[0], r[1], r[2]);
    }

    /// For each of his messages in SPELL_MESSAGES (ID, tab, text): Harper's marks, the words they are on with Harper's and
    /// SymSpell's first candidate, and the number `bad` gives it (paper-research N-487.p1, 2 and 3).
    #[test]
    #[ignore]
    fn messages_by_harper_and_symspell() {
        let Ok(file) = std::env::var("SPELL_MESSAGES") else { return };
        let text = std::fs::read_to_string(file).unwrap();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let (id, msg) = line.split_once('\t').unwrap_or(("", line));
            let _ = harper("warm");
            let t = std::time::Instant::now();
            let b = bad(msg);
            let ms = t.elapsed().as_secs_f64() * 1e3;
            let t = std::time::Instant::now();
            let _: Vec<Vec<String>> = msg.split_whitespace().map(symspell).collect();
            let sym_ms = t.elapsed().as_secs_f64() * 1e3;
            println!("MSG\t{id}\t{:.1}\t{}\t{}\t{}\t{msg}", b.per100, b.words, b.spelling, b.grammar);
            println!("TIME\t{id}\t{}\tharper {ms:.1} ms\tsymspell all words {sym_ms:.1} ms", msg.len());
            for m in harper(msg) {
                let w = &msg[m.range.clone()];
                let y = if m.spelling { symspell(w).into_iter().next().unwrap_or_default() } else { String::new() };
                println!("MARK\t{id}\t{}\t{w}\t{}\t{y}\t{}", m.kind, m.suggestions.first().cloned().unwrap_or_default(), m.message);
            }
        }
    }

    /// Where Harper's time goes on the message SPELL_ID of SPELL_MESSAGES (paper-research N-483.p8, 2: 1,990 ms on
    /// N-446.p1): the document alone, each rule alone over it (the slowest first), and each word alone.
    #[test]
    #[ignore]
    fn harper_time_by_rule() {
        let (Ok(file), Ok(want)) = (std::env::var("SPELL_MESSAGES"), std::env::var("SPELL_ID")) else { return };
        let text = std::fs::read_to_string(file).unwrap();
        let msg = text.lines().find_map(|l| l.split_once('\t').filter(|(id, _)| *id == want).map(|(_, m)| m.to_string())).unwrap();
        let ms = |t: std::time::Instant| t.elapsed().as_secs_f64() * 1e3;
        let _ = harper("warm");
        let t = std::time::Instant::now();
        let doc = Document::new_markdown_default_curated(&msg);
        println!("DOC\t{:.1} ms", ms(t));
        // the box's call, searching no further than NEAR, before anything else has looked at this text
        let t = std::time::Instant::now();
        let bounded = harper(&msg);
        println!("BOUNDED\t{:.1} ms\t{} marks", ms(t), bounded.len());
        let mut g = LintGroup::new_curated(FstDictionary::curated(), dialect());
        let keys: Vec<String> = g.iter_keys().map(str::to_string).collect();
        let mut times: Vec<(f64, String, usize)> = Vec::new();
        for k in &keys {
            g.set_all_rules_to(Some(false));
            g.config.set_rule_enabled(k, true);
            let t = std::time::Instant::now();
            let n = g.lint(&doc).len();
            times.push((ms(t), k.clone(), n));
        }
        times.sort_by(|a, b| b.0.total_cmp(&a.0));
        println!("RULES\t{} rules, summed {:.1} ms", times.len(), times.iter().map(|t| t.0).sum::<f64>());
        for (t, k, n) in times.iter().take(15) {
            println!("RULE\t{t:.1} ms\t{k}\t{n} marks");
        }
        let mut first = LintGroup::new_curated(FstDictionary::curated(), dialect());
        let t = std::time::Instant::now();
        let _ = first.lint(&doc);
        println!("GROUP-FIRST\t{:.1} ms", ms(t));
        let t = std::time::Instant::now();
        let _ = HARPER.with(|h| h.borrow_mut().as_mut().map(|g| g.lint(&doc).len()));
        println!("HARPER-GROUP\t{:.1} ms", ms(t));
        let t = std::time::Instant::now();
        let _ = harper(&msg);
        println!("ALL\t{:.1} ms", ms(t));
        let t = std::time::Instant::now();
        let _ = harper(&format!("{msg} "));
        println!("ALL-AGAIN\t{:.1} ms", ms(t));
        let t = std::time::Instant::now();
        let _ = Document::new_plain_english_curated(&msg);
        println!("DOC-PLAIN\t{:.1} ms", ms(t));
        // the rules together: the whole group, without its spelling rule, and growing a hundred rules at a time
        let mut whole = LintGroup::new_curated(FstDictionary::curated(), dialect());
        let t = std::time::Instant::now();
        let _ = whole.lint(&doc);
        println!("GROUP\t{:.1} ms", ms(t));
        let mut no_spell = LintGroup::new_curated(FstDictionary::curated(), dialect());
        no_spell.config.set_rule_enabled("SpellCheck", false);
        let t = std::time::Instant::now();
        let _ = no_spell.lint(&doc);
        println!("GROUP-NO-SPELLCHECK\t{:.1} ms", ms(t));
        let mut grow = LintGroup::new_curated(FstDictionary::curated(), dialect());
        grow.set_all_rules_to(Some(false));
        for (i, k) in keys.iter().enumerate() {
            grow.config.set_rule_enabled(k, true);
            if i % 100 == 99 || i + 1 == keys.len() {
                let t = std::time::Instant::now();
                let n = grow.lint(&doc).len();
                println!("GROW\t{}\t{:.1} ms\t{n} marks\tlast {k}", i + 1, ms(t));
            }
        }
        for w in msg.split_whitespace() {
            let t = std::time::Instant::now();
            let _ = harper(w);
            let e = ms(t);
            if e > 20.0 {
                println!("WORD\t{e:.1} ms\t{w}");
            }
        }
    }
}
