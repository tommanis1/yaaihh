//! The composer's spelling mode (the owner, N-363: autocorrect where one word fits, mark the rest, "make that a
//! mode"; paper-research N-101 p1285, p1286, p1517). Local only, no text leaves the machine: on Linux hunspell's
//! dictionary as the machine has it (/usr/share/hunspell), read by spellbook; on Windows, which has no hunspell
//! files, Windows' own checker (`ISpellChecker`, the dictionaries of the languages added in Settings; owner,
//! N-433.p15).
//!
//! - A word he ends (a space or a stop typed after it) that the dictionary does not know, and for which one word
//!   fits, is corrected in place; one undo puts his spelling back, and that word is then left as he wrote it.
//! - A word it does not know and cannot correct is underlined; the pointer resting on it shows the best candidate
//!   under it as a ghost, faint, in the box's own text, no card around it (paper-research N-815.p13, 1); a press on
//!   the ghost puts it in. The ⋯ beside it (or Ctrl+. with the caret in the word) opens its whole card
//!   (N-487.p88): the word struck through and the best candidate, up to four more dim, a click on one puts it in,
//!   `Add to dictionary` leaves the word as it is from then on.
//! - A changed word (the cleaning's, or his choice) shows his own word the same way, under it, when the pointer rests
//!   on it or a click puts the caret in it; a press on that puts his word back (the owner, N-858.p3).
//! - Nothing here stops a send. Agent names, @tags, ticket and question ids, file names and paths, code (between
//!   backticks), words with digits, inner capitals or underscores are left alone.
//! - The mode is one toggle in the composer, on until he turns it off, remembered (`spell.json` beside the pins).

use crate::App;
use rune::{Span, TextEdit};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

/// The checker: hunspell's files on Linux, Windows' own on Windows.
#[cfg(not(windows))]
mod checker {
    use std::sync::OnceLock;

    /// Where the machine keeps hunspell's dictionaries, and the one read.
    pub(super) const DIR: &str = "/usr/share/hunspell";
    const LANG: &str = "en_US";

    static DICT: OnceLock<Option<spellbook::Dictionary>> = OnceLock::new();

    /// The dictionary, read once; none when the machine has no hunspell files (the toggle says so).
    fn dict() -> Option<&'static spellbook::Dictionary> {
        DICT.get_or_init(|| {
            let aff = std::fs::read_to_string(format!("{DIR}/{LANG}.aff")).ok()?;
            let dic = std::fs::read_to_string(format!("{DIR}/{LANG}.dic")).ok()?;
            spellbook::Dictionary::new(&aff, &dic).ok()
        })
        .as_ref()
    }

    pub(super) fn available() -> bool {
        dict().is_some()
    }

    /// The English the checker reads.
    pub(super) fn english() -> Option<String> {
        Some(LANG.to_string())
    }

    pub(super) fn check(w: &str) -> bool {
        dict().is_none_or(|d| d.check(w))
    }

    pub(super) fn suggest(w: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(d) = dict() {
            d.suggest(w, &mut out);
        }
        out
    }
}

#[cfg(windows)]
#[path = "spell_windows.rs"]
mod checker;

/// The English the window's checker reads (`en_US`, `en-GB`, …), none when it names none; Harper takes its dialect
/// from it (`marks::dialect`).
pub fn english() -> Option<String> {
    checker::english()
}

/// The most candidates shown for a marked word.
const MAX_CANDIDATES: usize = 5;

/// Whether the machine has a checker (the toggle says when it has none).
pub fn available() -> bool {
    checker::available()
}

thread_local! {
    /// Each word's check, and each misspelled word's candidates, as the dictionary answered them.
    static KNOWN: RefCell<HashMap<String, bool>> = RefCell::new(HashMap::new());
    static CANDIDATES: RefCell<HashMap<String, Vec<String>>> = RefCell::new(HashMap::new());
}

fn known(w: &str) -> bool {
    KNOWN.with(|k| *k.borrow_mut().entry(w.to_string()).or_insert_with(|| checker::check(w)))
}

/// What the dictionary offers for W, best first.
pub fn candidates(w: &str) -> Vec<String> {
    CANDIDATES.with(|c| c.borrow_mut().entry(w.to_string()).or_insert_with(|| checker::suggest(w)).clone())
}

/// Edits between A and B (letters compared without case; a swap of two neighbours is one).
pub(crate) fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.to_lowercase().chars().collect();
    let b: Vec<char> = b.to_lowercase().chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=b.len() {
        d[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

fn letters(w: &str) -> Vec<char> {
    let mut l: Vec<char> = w.to_lowercase().chars().filter(|c| c.is_alphabetic()).collect();
    l.sort_unstable();
    l
}

/// The one word that fits W, if one does, of the dictionary's candidates (hunspell offers many: teh has thirteen):
/// - split words and words in another case (NAD for nad) are not his word;
/// - his word with an apostrophe put in (dont: don't), when one is;
/// - none for a word under three letters (im: mi is no fit);
/// - else, where a candidate is his word with two neighbours swapped (teh: the), that one when it alone is (wierd:
///   weird and wired, none);
/// - else the only candidate, near enough (isntead: instead; diapepsrs: diapers is too far);
/// - else, for a word of four letters or more, the nearest candidate when it alone is and near enough (seperate:
///   separate; chek: cheek and check, none).
/// Near enough: two edits at most, one in five letters at most.
///
/// On the owner's 40 typos `Rule::SwapAlone` put in no wrong word (this rule two: torugh: trough, lsay: slay), and 16
/// right ones with hunspell, 18 with Windows' checker; not taken, since correcting by itself goes off (paper-research
/// N-487.p1, 1).
fn fits_among(w: &str, cands: &[String]) -> Option<String> {
    fits_by(w, cands, Rule::Today)
}

/// The rules tried for when a word is put in by itself (paper-research N-483.p3, 1: no wrong correction).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum Rule {
    /// As `fits_among` says.
    Today,
    /// Without the scrambled-letters rule: a swap counts as any other one edit.
    NoSwap,
    /// The scrambled-letters rule only when no other candidate is one edit away (torugh: trough, but tough too).
    SwapAlone,
    /// Only one candidate one edit away (a swap is one), none nearer: that one; else none.
    OneNear,
    /// The scrambled-letters rule only for words of five letters or more.
    SwapLong,
}

pub(crate) fn fits_by(w: &str, cands: &[String], rule: Rule) -> Option<String> {
    let lower = !w.chars().any(char::is_uppercase);
    let cands: Vec<&String> = cands.iter().filter(|c| !c.contains([' ', '-']) && (!lower || !c.chars().any(char::is_uppercase))).collect();
    let one = |v: Vec<&String>| (v.len() == 1).then(|| v[0].clone());
    let apostrophe: Vec<&String> = cands.iter().copied().filter(|c| c.replace('\'', "") == w && c.len() == w.len() + 1).collect();
    if !apostrophe.is_empty() {
        return one(apostrophe);
    }
    // near enough to be his word: two edits at most, and no more than one in five letters
    let len = w.chars().count();
    let near = |d: usize| d <= 2 && d * 5 <= len;
    let nearest = |v: &[&String]| -> Option<String> {
        let best = v.iter().map(|c| distance(w, c)).min()?;
        near(best).then_some(())?;
        one(v.iter().copied().filter(|c| distance(w, c) == best).collect())
    };
    if w.chars().count() < 3 {
        return None;
    }
    if rule == Rule::OneNear {
        let best = cands.iter().map(|c| distance(w, c)).min()?;
        return (best == 1).then(|| one(cands.iter().copied().filter(|c| distance(w, c) == 1).collect())).flatten();
    }
    let others_near = |c: &String| cands.iter().any(|o| o != &c && distance(w, o) <= 1);
    let same: Vec<&String> = cands.iter().copied().filter(|c| letters(c) == letters(w)).collect();
    let same_rule = match rule {
        Rule::NoSwap => false,
        Rule::SwapAlone => !same.iter().any(|c| distance(w, c) == 1 && others_near(c)),
        Rule::SwapLong => len >= 5,
        _ => true,
    };
    if !same.is_empty() && same_rule {
        // his letters scrambled: the one a single swap away, else none (htne: then, hent)
        return one(same.into_iter().filter(|c| distance(w, c) == 1).collect());
    }
    if cands.len() == 1 {
        return near(distance(w, cands[0])).then(|| cands[0].clone());
    }
    if w.chars().count() >= 4 { nearest(&cands) } else { None }
}

pub fn fits(w: &str) -> Option<String> {
    fits_among(w, &candidates(w))
}

fn word_char(c: char) -> bool {
    c.is_alphabetic() || c == '\''
}

/// The byte ranges of TEXT that are code: between ``` fences, or between single backticks on a line.
pub(crate) fn code(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut fence: Option<usize> = None;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            match fence.take() {
                Some(f) => out.push(f..at + line.len()),
                None => fence = Some(at),
            }
        } else if fence.is_none() {
            let mut open: Option<usize> = None;
            for (i, c) in line.char_indices() {
                if c == '`' {
                    match open.take() {
                        Some(o) => out.push(at + o..at + i + 1),
                        None => open = Some(i),
                    }
                }
            }
        }
        at += line.len();
    }
    if let Some(f) = fence {
        out.push(f..text.len());
    }
    out
}

/// A run of TEXT between spaces that is not prose: a path or file name, an @tag, an id (N-101, Q-38, p1517), a
/// link, an agent's name (NAMES, lower case), something with digits, underscores or code's marks.
fn not_prose(chunk: &str, names: &HashSet<String>) -> bool {
    let bare = chunk.trim_matches(|c: char| !c.is_alphanumeric() && c != '@' && c != '#' && c != '/' && c != '~');
    if bare.is_empty() {
        return false;
    }
    let name = bare.trim_start_matches('@').trim_end_matches("'s").trim_end_matches("\u{2019}s").to_lowercase();
    if names.contains(&name) {
        return true;
    }
    if bare.chars().any(|c| c.is_ascii_digit() || matches!(c, '/' | '\\' | '_' | '@' | '#' | '~' | '=' | '<' | '>' | '{' | '}' | '[' | ']' | '(' | ')' | '$' | '|' | '*')) {
        return true;
    }
    // a dot or a colon inside the run: a file name (lib.rs), a module path (a::b), a link
    let inner: Vec<char> = bare.chars().collect();
    (1..inner.len().saturating_sub(1)).any(|i| matches!(inner[i], '.' | ':') && inner[i - 1].is_alphanumeric() && inner[i + 1].is_alphanumeric())
}

/// A word written as a name or in code's case: inner capitals (camelCase, HCI) or a lone letter.
fn not_a_word(w: &str) -> bool {
    w.chars().filter(|c| c.is_alphabetic()).count() < 2 || w.chars().skip(1).any(char::is_uppercase)
}

/// The words of TEXT to check: each a byte range, outside code, pills and chunks that are not prose.
fn words(text: &str, skip: &[Range<usize>], names: &HashSet<String>) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut at = 0;
    for chunk in text.split(|c: char| c.is_whitespace()) {
        let start = at;
        at += chunk.len() + text[start + chunk.len()..].chars().next().map_or(0, char::len_utf8);
        if chunk.is_empty() || not_prose(chunk, names) {
            continue;
        }
        let mut w: Option<usize> = None;
        let mut push = |a: usize, b: usize| {
            let r = a..b;
            let word = text[r.clone()].trim_matches('\'');
            let (a, b) = (a + text[r.clone()].len() - text[r.clone()].trim_start_matches('\'').len(), a + text[r.clone()].trim_end_matches('\'').len());
            if !word.is_empty() && !not_a_word(word) && !skip.iter().any(|s| s.start < b && a < s.end) {
                out.push(a..b);
            }
        };
        for (i, c) in chunk.char_indices() {
            match (word_char(c), w) {
                (true, None) => w = Some(start + i),
                (false, Some(a)) => {
                    push(a, start + i);
                    w = None;
                }
                _ => {}
            }
        }
        if let Some(a) = w {
            push(a, start + chunk.len());
        }
    }
    out
}

/// The composer's spelling state: whether the mode is on, the words he kept, the last correction made.
#[derive(Default)]
pub struct Spell {
    pub on: bool,
    /// Whether a word that fits is put in by itself: off (the owner, N-481 p10: "not enforced"; paper-research
    /// N-487.p1, 1). The way stays until he says otherwise.
    pub corrects: bool,
    kept: HashSet<String>,
    /// Whether "Words you added" is open (`dialog`).
    pub words_open: bool,
    /// The last correction in each composer: where, his word and what it became (to see it undone).
    last: HashMap<(usize, String), (usize, String, String)>,
    /// Each composer's text as last seen, to tell a typed stop from any other change.
    seen: HashMap<(usize, String), String>,
    /// Each composer's words put in by themselves, until sent: where, his word and what it became. A correction is
    /// never silent (paper-research N-483.p3, 2): each carries a quiet mark, and his word is one press away.
    fixed: HashMap<(usize, String), Vec<(usize, String, String)>>,
    /// The card open on a marked word (paper-research N-487.p88): which box, the word's place and text.
    pub card: Option<Card>,
    /// Where the pointer is for the card: on the marked word, on the card itself. It goes when on neither.
    pub on_word: bool,
    pub on_card: bool,
    /// Whether the card's word shows its whole card (every candidate and `Add to dictionary`), opened with the ⋯
    /// beside its ghost or with Ctrl+.; else only the ghost under it (paper-research N-815.p13, 1).
    pub whole: bool,
    /// Each box's words he put back with `Undo` on a changed word: not marked nor changed again in that box
    /// (paper-research N-487.p90, 3). In memory only.
    left: HashMap<(usize, String), HashSet<String>>,
}

/// A marked word's card: the box KEY, the word's byte RANGE in it and the WORD as it stood when it opened (a change
/// of the word closes it). KEYS: opened from the keyboard (Ctrl+.), which puts focus in the card.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub key: (usize, String),
    pub range: Range<usize>,
    pub word: String,
    pub keys: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Saved {
    on: bool,
    #[serde(default)]
    kept: Vec<String>,
}

fn path() -> std::path::PathBuf {
    std::path::Path::new(&crate::projects::config_path()).with_file_name("spell.json")
}

impl Spell {
    /// As he left it; on when he never turned it off.
    pub fn load() -> Spell {
        let saved: Option<Saved> = std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok());
        match saved {
            Some(s) => Spell { on: s.on, kept: s.kept.into_iter().collect(), ..Default::default() },
            None => Spell { on: true, ..Default::default() },
        }
    }

    fn save(&self) {
        // tests never write beside the owner's real config
        if cfg!(test) {
            return;
        }
        let mut kept: Vec<String> = self.kept.iter().cloned().collect();
        kept.sort();
        let _ = std::fs::write(path(), serde_json::to_string_pretty(&Saved { on: self.on, kept }).unwrap());
    }

    pub fn toggle(&mut self) {
        self.on = !self.on;
        self.save();
    }

    fn keep(&mut self, w: &str) {
        if self.kept.insert(w.to_lowercase()) {
            self.save();
        }
    }

    /// The words he added, sorted.
    pub fn added(&self) -> Vec<String> {
        let mut v: Vec<String> = self.kept.iter().cloned().collect();
        v.sort();
        v
    }

    /// W out of his words: marked again wherever the dictionaries do not know it.
    fn take_out(&mut self, w: &str) {
        if self.kept.remove(&w.to_lowercase()) {
            self.save();
        }
    }

    fn marked(&self, w: &str) -> bool {
        !self.kept.contains(&w.to_lowercase()) && !known(w)
    }
}

/// The words never marked in another program's box, lower case: the project's words and his added ones
/// (desk_marks.rs hands them to its own thread).
pub(crate) fn desk_words(s: &App) -> HashSet<String> {
    let mut n = names(s, 0);
    n.extend(s.spell.kept.iter().cloned());
    n
}

/// The window's checking for a text that is not a composer's: another program's box (desk_marks.rs), as the box
/// checks: the machine's checker's words and Harper's spelling marks (marks.rs `checked`, with SymSpell's
/// candidates), by byte range, each with Harper's candidates. None in code, a path, an id or a name's run, none of
/// MINE ([`desk_words`]). The words this thread checked stay in its caches until [`forget`].
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn misspelled(text: &str, mine: &HashSet<String>) -> Vec<(Range<usize>, Vec<String>)> {
    let all = words(text, &code(text), mine);
    let ours = |r: &Range<usize>| !mine.contains(&text[r.clone()].to_lowercase());
    let mut out: Vec<(Range<usize>, Vec<String>)> =
        if available() { all.iter().filter(|r| ours(r) && !known(&text[(*r).clone()])).map(|r| (r.clone(), Vec::new())).collect() } else { Vec::new() };
    // Harper's spelling marks, on the same words (so names, paths and code are left as the checker leaves them)
    for m in crate::marks::checked(text).into_iter().filter(|m| m.spelling && all.contains(&m.range) && ours(&m.range)) {
        match out.iter_mut().find(|(r, _)| *r == m.range) {
            Some((_, sug)) => *sug = m.suggestions,
            None => out.push((m.range, m.suggestions)),
        }
    }
    out.sort_by_key(|(r, _)| r.start);
    out
}

/// What is offered for a word W marked by [`misspelled`], in the box's order: MINE near it first, then Harper's and
/// SymSpell's ranked (HARPER), then the machine's checker's; as many as the composer shows.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn offered(w: &str, harper: &[String], mine: &HashSet<String>) -> Vec<String> {
    let mut c: Vec<String> = Vec::new();
    for x in project_near(w, mine).into_iter().chain(harper.iter().cloned()).chain(if known(w) { Vec::new() } else { candidates(w) }) {
        if !x.is_empty() && !c.iter().any(|y| y.eq_ignore_ascii_case(&x)) {
            c.push(x);
        }
    }
    c.truncate(MAX_CANDIDATES);
    c
}

/// This thread's caches dropped: the words it checked and their candidates (desk_marks.rs, when the box changes, so
/// nothing of a box read stays in memory once it is left).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn forget() {
    KNOWN.with(|k| k.borrow_mut().clear());
    CANDIDATES.with(|c| c.borrow_mut().clear());
}

/// The project's words, lower case, never checked and offered first when near (paper-research N-483.p8, 3): what the
/// window knows of every project open in it, the projects' names and boxes, the agents' names, the sidebar's parts,
/// each also in its pieces (docket-edge2: docket, edge2; "yaaihh project/Engine": yaaihh, project, engine). Ticket and
/// question ids are not words (`not_prose`). His own words are the kept ones (`spell.json`).
pub(crate) fn names(s: &App, _pi: usize) -> HashSet<String> {
    let mut whole: Vec<&str> = Vec::new();
    for p in &s.projects {
        whole.push(&p.name);
        whole.push(&p.id);
        whole.extend(p.live.agents.iter().map(|a| a.name.as_str()));
        whole.extend(p.live.agents.iter().map(|a| a.part.as_str()));
        whole.extend(p.live.parts.iter().map(|p| p.0.as_str()));
    }
    let mut n = project_words(&whole);
    n.insert("owner".into());
    n
}

/// NAMES, lower case, each whole and in its pieces of three letters or more (split at "-", "/", "_", ".", spaces).
fn project_words(names: &[&str]) -> HashSet<String> {
    let mut n = HashSet::new();
    for w in names.iter().map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()) {
        for p in w.split(['-', '/', '_', '.', ' ']).filter(|p| p.chars().filter(|c| c.is_alphabetic()).count() >= 3) {
            n.insert(p.to_string());
        }
        n.insert(w);
    }
    n
}

/// The project's words near W (one edit for a short word, two from five letters on), nearest first: yaaiihh: yaaihh.
fn project_near(w: &str, words: &HashSet<String>) -> Vec<String> {
    let most = if w.chars().count() >= 5 { 2 } else { 1 };
    let mut v: Vec<(usize, &String)> = words.iter().filter(|p| !p.contains(' ')).map(|p| (distance(w, p), p)).filter(|(d, _)| *d > 0 && *d <= most).collect();
    v.sort();
    v.into_iter().map(|(_, p)| p.clone()).collect()
}

fn skips(e: &TextEdit) -> Vec<Range<usize>> {
    let mut s = code(&e.text);
    s.extend(e.pills().map(|(r, _)| r));
    s
}

/// The misspelled words of E (for the underline), by byte range.
fn marked_in(sp: &Spell, e: &TextEdit, names: &HashSet<String>) -> Vec<Range<usize>> {
    if !sp.on || !available() {
        return Vec::new();
    }
    words(&e.text, &skips(e), names).into_iter().filter(|r| sp.marked(&e.text[r.clone()])).collect()
}

/// The marked ranges of E: the machine's checker's words, and Harper's marks once its thread has them (marks.rs),
/// none on an agent's name, a kept word, code or a pill.
pub fn marked(s: &App, pi: usize, e: &TextEdit) -> Vec<Range<usize>> {
    let names = names(s, pi);
    let mut out = marked_in(&s.spell, e, &names);
    for m in harper_marks(s, e, &names) {
        if !out.iter().any(|r| r.start < m.range.end && m.range.start < r.end) {
            out.push(m.range.clone());
        }
    }
    out.sort_by_key(|r| r.start);
    out
}

/// Harper's marks on E that the box shows (marks.rs `shown`), when they are ready.
fn harper_marks(s: &App, e: &TextEdit, names: &HashSet<String>) -> Vec<crate::marks::Mark> {
    if !s.spell.on {
        return Vec::new();
    }
    let Some(all) = crate::marks::of(&e.text) else { return Vec::new() };
    let skip = skips(e);
    all.iter()
        .filter(|m| {
            let w = &e.text[m.range.clone()];
            !skip.iter().any(|k| k.start < m.range.end && m.range.start < k.end) && !(m.spelling && (names.contains(&w.to_lowercase()) || s.spell.kept.contains(&w.to_lowercase())))
        })
        .cloned()
        .collect()
}

/// The prose word of E (`words`: outside code and pills, not a name, a path, an id or a lone letter) that byte AT is
/// in or at the end of (suggest.rs).
pub(crate) fn word_at(s: &App, pi: usize, e: &TextEdit, at: usize) -> Option<Range<usize>> {
    words(&e.text, &skips(e), &names(s, pi)).into_iter().find(|r| r.start <= at && at <= r.end)
}

/// Whether the word at R of box KEY's E is marked (`marked`): the machine's checker does not know it or Harper marks
/// it, and he did not put it back in that box (suggest.rs).
pub(crate) fn marked_word(s: &App, key: &(usize, String), e: &TextEdit, r: &Range<usize>) -> bool {
    let Some(w) = e.text.get(r.clone()) else { return false };
    if !s.spell.on || left_in(s, key, w) {
        return false;
    }
    (available() && s.spell.marked(w)) || harper_marks(s, e, &names(s, key.0)).iter().any(|m| m.range == *r)
}

/// The marked word the caret of E is in or at the end of, with its candidates.
pub fn at_caret(s: &App, pi: usize, e: &TextEdit) -> Option<(Range<usize>, Vec<String>)> {
    if e.anchor != e.focus {
        return None;
    }
    let r = marked(s, pi, e).into_iter().find(|r| r.start <= e.focus && e.focus <= r.end)?;
    let c = candidates_at(s, pi, e, &r);
    Some((r, c))
}

/// The candidates for the marked word at R in E, best first, at most MAX_CANDIDATES.
pub fn candidates_at(s: &App, pi: usize, e: &TextEdit, r: &Range<usize>) -> Vec<String> {
    // the project's words and his added ones near it first (N-515.p1), then Harper's and SymSpell's in offered's order
    // (nearest, commoner, splits last: N-487.p83), then the machine's checker's (N-487.p1: both for what he is shown)
    let names = names(s, pi);
    let harper = harper_marks(s, e, &names).into_iter().find(|m| m.range == *r).map(|m| m.suggestions).unwrap_or_default();
    let mut c: Vec<String> = Vec::new();
    let Some(w) = e.text.get(r.clone()) else { return c };
    for w in project_near(w, &names).into_iter().chain(project_near(w, &s.spell.kept)).chain(harper).chain(if known(w) { Vec::new() } else { candidates(w) }) {
        if !w.is_empty() && !c.iter().any(|x| x.eq_ignore_ascii_case(&w)) {
            c.push(w);
        }
    }
    c.truncate(MAX_CANDIDATES);
    c
}

/// The card's rows for a word with candidates CANDS (best first): the best one, beside the word struck through, and
/// up to four more, dim (paper-research N-487.p88). The same in the box and over other programs.
pub fn card_rows(cands: &[String]) -> (Option<&String>, &[String]) {
    match cands.split_first() {
        Some((best, more)) => (Some(best), &more[..more.len().min(4)]),
        None => (None, &[]),
    }
}

/// The highlight key of the marked word at R in box KEY: where its card stands.
pub fn mark_key(key: &(usize, String), r: &Range<usize>) -> String {
    format!("spell-mark-{}-{}-{}", key.0, key.1, r.start)
}

/// The highlight key of the changed word at R in box KEY.
pub fn changed_key(key: &(usize, String), r: &Range<usize>) -> String {
    format!("spell-changed-{}-{}-{}", key.0, key.1, r.start)
}

/// The pointer resting on (INSIDE) or gone from the marked word CARD is for, after the system's hover time.
pub fn word_hover(s: &mut App, card: Card, inside: bool) {
    if inside {
        s.spell.on_word = true;
        if s.spell.card.as_ref() != Some(&card) {
            s.spell.on_card = false;
            s.spell.whole = false;
        }
        s.spell.card = Some(card);
    } else if s.spell.card.as_ref() == Some(&card) {
        s.spell.on_word = false;
        if !s.spell.on_card {
            s.spell.card = None;
        }
    }
}

/// The pointer on (INSIDE) or off the card itself.
pub fn card_hover(s: &mut App, inside: bool) {
    s.spell.on_card = inside;
    if !inside && !s.spell.on_word && !s.spell.card.as_ref().is_some_and(|c| c.keys) {
        s.spell.card = None;
    }
}

/// Close the card (a key, a change of the word, a choice in it).
pub fn close_card(s: &mut App) {
    s.spell.card = None;
    s.spell.on_word = false;
    s.spell.on_card = false;
    s.spell.whole = false;
}

/// A changed word of a box with the caret inside it, when no card is open: a click in the word (or the arrows) shows
/// his own word under it (the owner: "clicking on the changed word … shows the original text", N-858.p3). Only while
/// that box has the keyboard, or the pointer is on what it shows (a press there takes the keyboard from the box).
/// Else the marked word the caret moved into or onto (suggest.rs's `Fix`): its ghost under it, which Tab takes (0.2
/// line 27, the owner: "for the suggestion i need to like click instead of just move my cursor").
pub fn caret_card(s: &App) -> Option<Card> {
    if s.spell.card.is_some() {
        return None;
    }
    s.composers.iter().filter(|(_, e)| (e.focused || s.spell.on_card) && e.anchor == e.focus).find_map(|(key, e)| {
        let at = e.focus;
        let r = match changed_at(s, key, e).into_iter().find(|(r, _)| r.start < at && at < r.end) {
            Some((r, _)) => r,
            None => crate::suggest::ghost(s, key).filter(|g| g.kind == crate::suggest::Kind::Fix)?.range,
        };
        Some(Card { key: key.clone(), word: e.text.get(r.clone())?.to_string(), range: r, keys: false })
    })
}


/// A key in box KEY: Ctrl+. opens the card on the marked word at the caret, focus in it (the keyboard's way to the
/// candidates, N-487.p88); any other key closes a card opened by the pointer and goes on to the box.
pub fn card_key(s: &mut App, key: &(usize, String), ev: &rune::KeyEvent, tx: &mut rune::Tx) -> bool {
    if ev.is_command('.') {
        let Some(e) = s.composers.get(key) else { return false };
        // a marked word, or a changed one (its Undo from the keyboard, N-584.p19, 5)
        let at = e.focus;
        let r = match at_caret(s, key.0, e) {
            Some((r, _)) => r,
            None if e.anchor == e.focus => match changed_at(s, key, e).into_iter().find(|(r, _)| r.start <= at && at <= r.end) {
                Some((r, _)) => r,
                None => return false,
            },
            None => return false,
        };
        let word = e.text[r.clone()].to_string();
        s.spell.card = Some(Card { key: key.clone(), range: r, word, keys: true });
        s.spell.whole = true;
        tx.focus(format!("spell-card-{}-{}", key.0, key.1), true);
        return true;
    }
    if s.spell.card.as_ref().is_some_and(|c| c.key == *key) {
        close_card(s);
    }
    false
}

/// The system's hover time: how long the pointer rests before a card opens (Windows' SPI_GETMOUSEHOVERTIME; 400 ms,
/// its default, elsewhere).
pub fn hover_time() -> std::time::Duration {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{SPI_GETMOUSEHOVERTIME, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW};
        let mut ms: u32 = 0;
        if unsafe { SystemParametersInfoW(SPI_GETMOUSEHOVERTIME, 0, Some(&mut ms as *mut u32 as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)) }.is_ok() && ms > 0 {
            return std::time::Duration::from_millis(ms as u64);
        }
    }
    std::time::Duration::from_millis(400)
}

/// Put WORD in place of range R of composer KEY, as one undo step, the caret after it.
pub fn replace(s: &mut App, key: &(usize, String), r: Range<usize>, word: &str) {
    let Some(e) = s.composers.get_mut(key) else { return };
    if e.text.get(r.clone()).is_none() {
        return;
    }
    let caret = r.start + word.len();
    e.undo_group(|e| {
        e.set_selection(r.start, r.end);
        e.insert(word);
        e.set_caret(caret);
    });
    // a choice in the card: typing goes on in the box (the card never keeps the keyboard)
    e.focused = true;
    s.spell.seen.insert(key.clone(), e.text.clone());
    if let Some(list) = s.spell.fixed.get_mut(key) {
        shift(list, &[(r.clone(), word.len())]);
    }
    close_card(s);
}

/// The changed words in LIST kept where they stand after EDITS (each a range of the old text and its new length,
/// in order): those an edit touched go, those after one move by it.
fn shift(list: &mut Vec<(usize, String, String)>, edits: &[(Range<usize>, usize)]) {
    list.retain_mut(|(at, _, ours)| {
        let end = *at + ours.len();
        let mut delta = 0isize;
        for (r, len) in edits {
            if r.start < end && *at < r.end || (r.start == r.end && r.start > *at && r.start < end) {
                return false;
            }
            if r.end <= *at {
                delta += *len as isize - r.len() as isize;
            }
        }
        *at = (*at as isize + delta) as usize;
        true
    });
}

/// A word chosen in the card of a marked word: put in, and a changed word from then on, with `Undo` (N-487.p90, 4).
pub fn choose(s: &mut App, key: &(usize, String), r: Range<usize>, word: &str) {
    let Some(his) = s.composers.get(key).and_then(|e| e.text.get(r.clone())).map(str::to_string) else { return };
    replace(s, key, r.clone(), word);
    s.spell.fixed.entry(key.clone()).or_default().push((r.start, his, word.to_string()));
}

/// The cleaning's EDITS (ranges of box KEY's text, in order, and what takes their place) put in as one undo step,
/// the caret where he types, each a changed word (changed.rs).
pub fn apply_changes(s: &mut App, key: &(usize, String), edits: &[(Range<usize>, String)]) {
    let Some(e) = s.composers.get_mut(key) else { return };
    let old = e.text.clone();
    if edits.iter().any(|(r, _)| old.get(r.clone()).is_none()) {
        return;
    }
    let caret = e.focus;
    let mut moved = caret as isize;
    for (r, w) in edits {
        if r.end <= caret {
            moved += w.len() as isize - r.len() as isize;
        } else if r.start < caret {
            moved = moved - (caret - r.start) as isize + w.len() as isize;
        }
    }
    e.undo_group(|e| {
        for (r, w) in edits.iter().rev() {
            e.set_selection(r.start, r.end);
            e.insert(w);
        }
        e.set_caret(moved.max(0) as usize);
    });
    s.spell.seen.insert(key.clone(), e.text.clone());
    let list = s.spell.fixed.entry(key.clone()).or_default();
    shift(list, &edits.iter().map(|(r, w)| (r.clone(), w.len())).collect::<Vec<_>>());
    let mut off = 0isize;
    for (r, w) in edits {
        list.push(((r.start as isize + off) as usize, old[r.clone()].to_string(), w.clone()));
        off += w.len() as isize - r.len() as isize;
    }
}

/// The words box KEY's cleaning keeps as they are: his added words and the words he put back in that box (so an
/// `Undo` holds through Send, consult-2 N-584.p18, 2).
pub fn keep_for(s: &App, key: &(usize, String)) -> Vec<String> {
    let mut k = s.spell.added();
    if let Some(l) = s.spell.left.get(key) {
        k.extend(l.iter().filter(|w| !k.contains(w)).cloned().collect::<Vec<_>>());
    }
    k
}

/// Whether WORD was put back by him in box KEY (not changed nor marked again there). WORD may be two (two typos side
/// by side are one edit, "wnat ot"): any of them put back counts (consult-2 N-584.p43, 3).
pub fn left_in(s: &App, key: &(usize, String), word: &str) -> bool {
    s.spell.left.get(key).is_some_and(|l| word.split_whitespace().any(|w| l.contains(&w.to_lowercase())))
}

/// The marked words of box KEY's E, without those he put back there, nor the word the caret is in or ends while he is
/// typing it (suggest.rs `typing`): a line that comes and goes under each half-typed word (`beca`, `becau`, then none
/// at `because`) is noise under his caret. A space or a stop ends the word, a move of the caret leaves it, and the
/// pause marks it as it stands. Nor the word whose completion stands after the caret (suggest.rs `Complete`): it is
/// the start of a word, offered whole, not a word misspelt.
pub fn marked_box(s: &App, key: &(usize, String), e: &TextEdit) -> Vec<Range<usize>> {
    let typing = (e.focused && e.anchor == e.focus && crate::suggest::typing(s, key)).then_some(e.focus);
    let completed = crate::suggest::ghost(s, key).filter(|g| g.kind == crate::suggest::Kind::Complete).map(|g| g.range);
    marked(s, key.0, e)
        .into_iter()
        .filter(|r| !left_in(s, key, &e.text[r.clone()]) && typing.is_none_or(|c| c < r.start || c > r.end) && completed.as_ref() != Some(r))
        .collect()
}

/// The underline spans for the marked words of box KEY's E, in COLOR.
pub fn box_spans(s: &App, key: &(usize, String), e: &TextEdit, color: rune::Color) -> Vec<Span> {
    marked_box(s, key, e).into_iter().map(|r| Span::new(r.start, r.end).underline(color)).collect()
}

/// The changed words of box KEY's E still standing, with the word he typed (the dotted mark, the card's `Undo`).
pub fn changed_at(s: &App, key: &(usize, String), e: &TextEdit) -> Vec<(Range<usize>, String)> {
    corrections(s, key, e).into_iter().map(|(r, his)| (r, his.to_string())).collect()
}

/// `Add to dictionary` on a marked word: left as he wrote it, from now on.
pub fn keep(s: &mut App, w: &str) {
    s.spell.keep(w);
    close_card(s);
}

/// "Words you added", from the top bar's ⋯ menu (paper-research N-515.p1: the way back, behind a click, not on the
/// box): each word `Add to dictionary` or "Put back" kept, with Take out beside it, into LAYER while it is open.
pub fn dialog(s: &App, k: &rune_kit::Kit, layer: &mut Vec<rune::El>) {
    use rune::{ScrollSpec, col, row, text};
    use rune_kit::{Button, ButtonVariant, Dialog};
    if !s.spell.words_open {
        return;
    }
    let words = s.spell.added();
    let mut list = col().w_full().gap(2.0);
    if words.is_empty() {
        list = list.child(text("None yet.").font(k.ty.small).color(k.c.text_faint).wrap());
    }
    for w in words {
        let out = w.clone();
        list = list.child(
            row()
                .key(format!("added-word-{w}"))
                .w_full()
                .justify_between()
                .center_y()
                .child(text(w))
                .child(Button::new("Take out").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| s.spell.take_out(&out)).build(k)),
        );
    }
    let close = Button::new("Close").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.spell.words_open = false).build(k);
    let body = col()
        .w_full()
        .px(16.0)
        .pb(16.0)
        .gap(12.0)
        .child(col().key("added-words-list").w_full().max_h(360.0).scroll(ScrollSpec { stick_bottom: false }).child(list))
        .child(row().w_full().justify_end().child(close));
    Dialog::new("added-words", "Words you added").open(true).fit().width(420.0).child(body).on_close(|s: &mut App| s.spell.words_open = false).build(k, layer);
}

/// After each change of composer KEY (chat.rs `on_change`).
pub fn changed(s: &mut App, key: &(usize, String)) {
    if !s.spell.on {
        return;
    }
    let names = names(s, key.0);
    if let Some(e) = s.composers.get_mut(key) {
        fix(&mut s.spell, e, &names, key);
    }
}

/// When he has just ended a word of E the dictionary does not know and one word fits, it is put in place (one undo
/// step); when he undid such a correction, his word is kept.
fn fix(sp: &mut Spell, e: &mut TextEdit, names: &HashSet<String>, key: &(usize, String)) {
    let text = e.text.clone();
    let before = sp.seen.insert(key.clone(), text.clone()).unwrap_or_default();
    if let Some(list) = sp.fixed.get_mut(key) {
        follow(&before, &text, list);
    }
    // his undo of the last correction: his word is back where it was
    if let Some((at, his, ours)) = sp.last.get(key).cloned()
        && text.get(at..at + his.len()) == Some(his.as_str())
        && text.get(at..at + ours.len()) != Some(ours.as_str())
    {
        sp.last.remove(key);
        sp.keep(&his);
        return;
    }
    if !sp.corrects || !available() || e.anchor != e.focus {
        return;
    }
    // one character typed, a space or a stop, right before the caret
    let caret = e.focus.min(text.len());
    let Some(stop) = text[..caret].chars().next_back() else { return };
    if text.len() != before.len() + stop.len_utf8() || !(stop.is_whitespace() || matches!(stop, '.' | ',' | ';' | ':' | '!' | '?' | ')')) {
        return;
    }
    let end = caret - stop.len_utf8();
    let Some(r) = words(&text, &skips(e), names).into_iter().find(|r| r.end == end) else { return };
    let w = text[r.clone()].to_string();
    if !sp.marked(&w) {
        return;
    }
    let Some(fit) = fits(&w) else { return };
    let delta = fit.len() as isize - w.len() as isize;
    e.undo_group(|e| {
        e.set_selection(r.start, r.end);
        e.insert(&fit);
        e.set_caret((caret as isize + delta) as usize);
    });
    sp.seen.insert(key.clone(), e.text.clone());
    sp.fixed.entry(key.clone()).or_default().push((r.start, w.clone(), fit.clone()));
    sp.last.insert(key.clone(), (r.start, w, fit));
}

/// The corrections in LIST, kept where they stand as BEFORE became AFTER (one edit): moved with the text, gone where
/// the edit touched them or the word is no longer what was put in (his undo, his own change, the box sent).
pub(crate) fn follow(before: &str, after: &str, list: &mut Vec<(usize, String, String)>) {
    let (b, a) = (before.as_bytes(), after.as_bytes());
    let pre = b.iter().zip(a).take_while(|(x, y)| x == y).count();
    let room = b.len().min(a.len()) - pre;
    let suf = b.iter().rev().zip(a.iter().rev()).take(room).take_while(|(x, y)| x == y).count();
    let (cut_end, delta) = (b.len() - suf, a.len() as isize - b.len() as isize);
    list.retain_mut(|(at, _, ours)| {
        let end = *at + ours.len();
        if end > pre && *at < cut_end {
            return false;
        }
        if *at >= cut_end {
            *at = (*at as isize + delta) as usize;
        }
        after.get(*at..*at + ours.len()) == Some(ours.as_str())
    });
}

/// The words of composer KEY put in by themselves and still standing, with the word he typed.
fn corrections<'a>(s: &'a App, key: &(usize, String), e: &TextEdit) -> Vec<(Range<usize>, &'a str)> {
    let Some(list) = s.spell.fixed.get(key) else { return Vec::new() };
    list.iter().filter(|(at, _, ours)| e.text.get(*at..*at + ours.len()) == Some(ours.as_str())).map(|(at, his, ours)| (*at..*at + ours.len(), his.as_str())).collect()
}

/// WORD left as he wrote it in box KEY (not marked nor changed again there) while the window runs; `Add to
/// dictionary` is the lasting way (paper-research N-584.p34).
/// WORD may be two (a pair changed as one, "wnat ot"): each is kept on its own, as `left_in`, `keep` and the marks
/// read single words (consult-2 N-584.p48).
pub fn leave(s: &mut App, key: &(usize, String), word: &str) {
    let left = s.spell.left.entry(key.clone()).or_default();
    left.extend(word.split_whitespace().map(str::to_lowercase));
}

/// Box KEY as if new: nothing put back in it (a test's fresh start).
#[cfg(test)]
pub fn forget_left(s: &mut App, key: &(usize, String)) {
    s.spell.left.remove(key);
}

/// `Undo` on a changed word: his word in its place, left as he wrote it in this box.
pub fn put_back(s: &mut App, key: &(usize, String), r: Range<usize>, his: &str) {
    replace(s, key, r, his);
    // he chose his word: not marked nor changed again in this box (N-487.p90, 3); the dictionary is `Add to dictionary`'s
    leave(s, key, his);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(t: &str) -> Vec<&str> {
        let names: HashSet<String> = ["docket-edge", "paper-research", "owner"].iter().map(|s| s.to_string()).collect();
        words(t, &code(t), &names).into_iter().map(|r| &t[r]).collect()
    }

    #[test]
    fn another_programs_text_is_checked_as_the_composer_checks_it_and_his_kept_words_are_not_marked() {
        if !available() {
            return;
        }
        let t = "I tihnk the box is fine, see lib.rs and `teh`";
        let marked: Vec<&str> = misspelled(t, &HashSet::new()).into_iter().map(|(r, _)| &t[r]).collect();
        assert_eq!(marked, ["tihnk"]);
        let kept: HashSet<String> = ["tihnk".to_string()].into();
        assert!(misspelled(t, &kept).is_empty());
        let o = offered("tihnk", &[], &HashSet::new());
        assert!(!o.is_empty() && o.len() <= MAX_CANDIDATES, "{o:?}");
        // his and the project's words near it come first
        let mine: HashSet<String> = ["tihnks".to_string()].into();
        assert_eq!(offered("tihnk", &[], &mine)[0], "tihnks");
        forget();
        assert!(KNOWN.with(|k| k.borrow().is_empty()) && CANDIDATES.with(|c| c.borrow().is_empty()));
    }

    #[test]
    fn the_projects_words_are_known_and_offered_first() {
        let n = project_words(&["docket-edge2", "yaaihh project/Engine", "tovrin-dev", ""]);
        for w in ["docket-edge2", "docket", "edge2", "yaaihh", "engine", "tovrin", "dev"] {
            assert!(n.contains(w), "{w}: {n:?}");
        }
        assert_eq!(project_near("yaaiihh", &n), ["yaaihh"]);
        assert_eq!(project_near("tovirn", &n), ["tovrin"]);
        assert!(project_near("dog", &n).is_empty(), "a short word only one edit away: dev is two");
    }

    #[test]
    fn names_ids_files_and_code_are_not_checked() {
        assert_eq!(ws("@docket-edge pls chek N-101.p1517 and lib.rs, `fn foo_bar` HCI isn't docket-edge's"), ["pls", "chek", "and", "isn't"]);
        assert_eq!(ws("see ~/src/rune or https://x.org/a ok"), ["see", "or", "ok"]);
        assert_eq!(ws("```\nlet x = teh;\n```\nthe end"), ["the", "end"]);
        assert_eq!(ws("camelCase and Q-38 and a"), ["and", "and"]);
    }

    #[test]
    fn one_word_fits_only_when_it_alone_is_nearest() {
        let c = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // the lists as the machine's en_US gave them
        assert_eq!(fits_among("recieve", &c(&["receive", "relieve", "reverie"])), Some("receive".into()));
        assert_eq!(fits_among("teh", &c(&["the", "eh", "teth", "tech", "tee", "tea", "ten", "ter", "tel", "ted", "meh", "Neh", "t eh"])), Some("the".into()));
        assert_eq!(fits_among("wierd", &c(&["weird", "wired", "wider", "wield"])), None, "two swaps: marked");
        assert_eq!(fits_among("im", &c(&["mi", "m", "i", "aim"])), None, "two letters: marked");
        assert_eq!(fits_among("htne", &c(&["then", "hent", "hone", "ht ne", "ht-ne"])), None, "his letters scrambled two ways: marked");
        assert_eq!(fits_among("diapepsrs", &c(&["diapers"])), None, "the only one, too far");
        assert_eq!(fits_among("isntead", &c(&["instead"])), Some("instead".into()));
        assert_eq!(fits_among("chek", &c(&["cheek", "check", "heck", "chem", "chef", "chew", "chez", "Cheka"])), None, "two at one edit: marked");
        assert_eq!(fits_among("nad", &c(&["NAD", "and", "ad", "nard", "sad"])), Some("and".into()), "NAD is not in his case");
        assert_eq!(fits_among("dont", &c(&["dot", "don", "don't", "dent"])), Some("don't".into()));
        assert_eq!(fits_among("seperate", &c(&["separate", "perseverate", "temperate", "desperate"])), Some("separate".into()));
        assert_eq!(fits_among("untill", &c(&["until", "until l"])), Some("until".into()));
        assert_eq!(fits_among("pls", &c(&["ls", "ps", "pl", "pals", "plus"])), None);
        assert_eq!(distance("teh", "the"), 1);
    }

    /// With the machine's dictionary: a word that one word fits is corrected as he ends it, one undo puts his back
    /// and it is not corrected again; a word nothing fits is marked; nothing is done to a word in progress.
    #[test]
    fn a_word_ended_is_corrected_and_one_undo_puts_his_back() {
        if !available() {
            eprintln!("no spelling checker on this machine: skipped");
            return;
        }
        // the way kept while it is off (N-487.p1, 1): off, nothing is put in
        let mut sp = Spell { on: true, ..Default::default() };
        let names: HashSet<String> = HashSet::new();
        let key = (0usize, "w".to_string());
        let mut e = TextEdit::new("");
        for ch in "I recieve ".chars() {
            e.insert(&ch.to_string());
            fix(&mut sp, &mut e, &names, &key);
        }
        assert_eq!(e.text, "I recieve ", "off: his word stays");
        let mut sp = Spell { on: true, corrects: true, ..Default::default() };
        let mut e = TextEdit::new("");
        let typ = |sp: &mut Spell, e: &mut TextEdit, t: &str| {
            for ch in t.chars() {
                e.insert(&ch.to_string());
                fix(sp, e, &names, &key);
            }
        };
        typ(&mut sp, &mut e, "I recieve");
        assert_eq!(e.text, "I recieve", "a word in progress is left");
        typ(&mut sp, &mut e, " ");
        assert_eq!(e.text, "I receive ");
        e.undo();
        fix(&mut sp, &mut e, &names, &key);
        assert_eq!(e.text, "I recieve ");
        typ(&mut sp, &mut e, "recieve ");
        assert_eq!(e.text, "I recieve recieve ", "his word, once undone, is his");
        assert!(marked_in(&sp, &e, &names).is_empty(), "a kept word is not marked");
        typ(&mut sp, &mut e, "qzxv ");
        let m: Vec<&str> = marked_in(&sp, &e, &names).into_iter().map(|r| &e.text[r]).collect();
        assert_eq!(m, ["qzxv"]);
        // the way back (N-515.p1): listed, and taken out it is marked again
        assert_eq!(sp.added(), ["recieve"]);
        sp.take_out("Recieve");
        assert!(sp.added().is_empty());
        let m: Vec<&str> = marked_in(&sp, &e, &names).into_iter().map(|r| &e.text[r]).collect();
        assert_eq!(m, ["recieve", "recieve", "qzxv"], "taken out, marked again");
    }

    #[test]
    fn a_correction_keeps_its_mark_where_it_stands_until_touched() {
        let mut l = vec![(4, "torugh".to_string(), "trough".to_string())];
        // typed after it, and before it: it moves with the text
        follow("the trough ", "the trough is", &mut l);
        assert_eq!(l[0].0, 4);
        follow("the trough is", "so the trough is", &mut l);
        assert_eq!(l[0].0, 7);
        // an edit inside it: gone (his own change)
        follow("so the trough is", "so the tough is", &mut l);
        assert!(l.is_empty());
        // the box emptied when sent: gone
        let mut l = vec![(0, "hte".to_string(), "the".to_string())];
        follow("the end", "", &mut l);
        assert!(l.is_empty());
    }

    /// Each rule's corrections of the typos in SPELL_TYPOS (TYPO, tab, meant; # comments): right, wrong and none per
    /// rule, then each word's corrections, as TSV (paper-research N-483.p3, 1). `SPELL_TYPOS=FILE cargo test
    /// rules_on_a_list_of_typos -- --ignored --nocapture`, on Linux (hunspell) and Windows (its own checker).
    #[test]
    #[ignore]
    fn rules_on_a_list_of_typos() {
        let Ok(file) = std::env::var("SPELL_TYPOS") else { return };
        let text = std::fs::read_to_string(file).unwrap();
        let rules = [Rule::Today, Rule::NoSwap, Rule::SwapAlone, Rule::OneNear, Rule::SwapLong];
        let mut counts = vec![(0, 0, 0); rules.len()];
        for line in text.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
            let (typo, meant) = line.split_once('\t').unwrap_or((line, ""));
            if checker::check(typo) {
                println!("WORD\t{typo}\t{meant}\tknown");
                continue;
            }
            let cands = checker::suggest(typo);
            let got: Vec<String> = rules.iter().map(|r| fits_by(typo, &cands, *r).unwrap_or_default()).collect();
            for (g, c) in got.iter().zip(counts.iter_mut()) {
                match g.as_str() {
                    "" => c.2 += 1,
                    x if x == meant => c.0 += 1,
                    _ => c.1 += 1,
                }
            }
            println!("WORD\t{typo}\t{meant}\t{}\t{}", got.join("\t"), cands.join(", "));
        }
        for (r, (right, wrong, none)) in rules.iter().zip(counts) {
            println!("RULE\t{r:?}\tright {right}\twrong {wrong}\tunderlined {none}");
        }
    }
}
