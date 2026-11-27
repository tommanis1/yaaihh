//! The box's changed words (paper-research N-487.p90; docket-core8 N-564.p5).
//!
//! - When the words in the focused box have stood still for a pause (`PAUSE`, not each key), the window asks coordd to
//!   clean them (`Request::CleanText`, from `CLEAN_TEXT_PROTOCOL`), with his added words kept. Nothing is posted.
//!   Only the paragraphs not cleaned yet are sent, from the first such to the last (`to_ask`).
//! - The answer is lined up against what he typed, word by word (`align`); each difference is one edit (a word
//!   replaced, two joined, one split). The edits go into the box as one undo step, so Ctrl+Z takes the batch back,
//!   and the caret stays where he types. Each changed word carries the dotted mark and its card offers `Undo`
//!   (spell.rs, message_box.rs).
//! - He typed on before the answer came: its edits still go in where his words stand as they were sent, lined up
//!   word by word against the box as it is then (not the word at its very end, which he may still be typing, and only
//!   where the words either side of it stand beside it too), and the next pause asks for the rest (`moved`; the
//!   owner: "the being cleaned basically is too late", N-107.p15). An answer for a message he has sent meanwhile goes
//!   nowhere (`sent`).
//! - An edit on the line he is still typing on waits, as its word's ghost, until he stops or leaves that line
//!   (suggest.rs `hold`, 0.2 line 27): an answer 5 s after his pause never moves the word he is typing.
//! - A word he undid (its card's `Undo`, or Ctrl+Z on the batch, then typing on) is not changed again in that box.
//! - What is sent is the box as he saw it; the words ticket's form carries the changed words' pairs and his typing
//!   rebuilt from them (`typed`), which "as typed" shows (words.rs; paper-research N-584.p19, 3).

use std::collections::HashMap;
use std::ops::Range;
use std::time::{Duration, Instant};

use crate::{App, Pending};
use coord_types::Request;

/// How long the box's words stand still before they are cleaned.
pub const PAUSE: Duration = Duration::from_millis(1500);

/// At most this many words are lined up (the lining-up is quadratic); a longer text is left as it is.
const MOST: usize = 600;

/// A focused box as the last poll saw it: its text and since when it has stood, its caret, and whether he typed at
/// that caret (else he moved it there since the text last changed; suggest.rs `typing`).
pub(crate) struct Seen {
    pub(crate) text: String,
    pub(crate) since: Instant,
    pub(crate) caret: usize,
    pub(crate) typed: bool,
}

#[derive(Default)]
pub struct State {
    /// Each focused box as last seen (any box, cleaned or not: suggest.rs reads whether he is typing there).
    pub(crate) seen: HashMap<(usize, String), Seen>,
    /// The text each box asked to have cleaned, the part of it sent, and the ask's number (its answer is taken only
    /// while it is this one), until its answer comes.
    asked: HashMap<(usize, String), (String, Range<usize>, u64)>,
    /// The asks made so far: each ask's number.
    asks: u64,
    /// Each box's paragraphs already cleaned (sent, or as the edits left them): a pause sends only the others
    /// (paper-research N-584.p46, item 6).
    clean: HashMap<(usize, String), std::collections::HashSet<String>>,
    /// The texts of each box not to be asked again: each text sent (so a batch he took back with Ctrl+Z stays taken
    /// back, consult-2 N-584.p18, 1), each text the edits made, and a text that could not be asked.
    done: HashMap<(usize, String), std::collections::HashSet<String>>,
    /// Each box's texts whose edits were put in, with the words he had typed there: when the box reads such a text
    /// again (Ctrl+Z took the batch back) those words are left as he typed them, so typing on does not change them
    /// again (consult-2 N-584.p33, UX 1).
    put: HashMap<(usize, String), HashMap<String, Vec<String>>>,
}

/// Whether box KEY's words are cleaned: the spelling mode is on and the coord speaks CleanText.
fn on(s: &App, pi: usize) -> bool {
    s.spell.on && s.projects.get(pi).is_some_and(|p| p.protocol >= coord_types::CLEAN_TEXT_PROTOCOL)
}

/// Each poll: [`step`], and while a box waits for its pause, a wake: once per tick (`App::wake_every`), and in the
/// pause's last tick for its end itself, the same moment on each poll, so the box is cleaned when its words have stood
/// still for PAUSE. Asked half a second ahead on each poll instead, each frame of the pause asked for one more frame:
/// 42 frames a second while he typed in With you against his 5.5 keys (his frames.jsonl on build 24, N-1272).
pub fn poll(s: &mut App, tx: &mut rune::Tx) {
    match step(s) {
        Some(left) if left <= PAUSE / 3 => tx.wake_after(left),
        Some(_) => s.wake_every(PAUSE / 3, tx),
        None => {}
    }
}

/// The focused box whose words stood still for PAUSE is sent to be cleaned. While a box is still waiting for its
/// pause, how long until the soonest pause ends (the poll wakes again for it).
pub fn step(s: &mut App) -> Option<Duration> {
    let now = Instant::now();
    let boxes: Vec<((usize, String), String, usize)> = s.composers.iter().filter(|(_, e)| e.focused).map(|(k, e)| (k.clone(), e.text.clone(), e.focus)).collect();
    let mut waiting: Option<Duration> = None;
    for (key, text, caret) in boxes {
        let seen = s.changed.seen.entry(key.clone()).or_insert_with(|| Seen { text: text.clone(), since: now, caret, typed: false });
        if seen.text != text {
            *seen = Seen { text: text.clone(), since: now, caret, typed: true };
        } else if seen.caret != caret {
            seen.caret = caret;
            seen.typed = false;
        }
        let since = seen.since;
        if !on(s, key.0) || text.split_whitespace().count() < 2 {
            continue;
        }
        if let Some(his) = s.changed.put.get_mut(&key).and_then(|p| p.remove(&text)) {
            for w in his {
                crate::spell::leave(s, &key, &w);
            }
        }
        if s.changed.asked.contains_key(&key) || s.changed.done.get(&key).is_some_and(|d| d.contains(&text)) {
            continue;
        }
        if let Some(left) = PAUSE.checked_sub(now.duration_since(since)).filter(|l| !l.is_zero()) {
            waiting = Some(waiting.map_or(left, |w| w.min(left)));
            continue;
        }
        let Some(part) = to_ask(s, &key, &text) else {
            s.changed.done.entry(key).or_default().insert(text);
            continue;
        };
        let req = Request::CleanText { text: text[part.clone()].to_string(), keep: crate::spell::keep_for(s, &key) };
        let Some(p) = s.projects.get_mut(key.0) else { continue };
        match p.link.as_mut().and_then(|l| l.ask(&req)) {
            Some(id) => {
                s.changed.asks += 1;
                let n = s.changed.asks;
                p.pending.insert(id, Pending::Cleaned { key: key.clone(), n });
                s.changed.asked.insert(key, (text, part, n));
            }
            // not connected: this text is left as it is (no notice: he did not ask for it)
            None => _ = s.changed.done.entry(key).or_default().insert(text),
        }
    }
    waiting
}

/// The cleaning's answer for box KEY's ask N (`{head, cleaned, ...}`): its edits put into the box where its words
/// still stand as they were sent. He typed on meanwhile (the call takes 4.7 s in the middle, 10 s for one in ten,
/// N-815): the edits on words he left as they were still go in (`moved`), the others wait for the next pause (the
/// owner: "the being cleaned basically is too late", N-107.p15).
pub fn replied(s: &mut App, key: (usize, String), n: u64, ok: bool, out: String) {
    // the ask is no longer waited for: the box was sent meanwhile (`sent`), and what it holds now is another message
    // (consult-5, N-412 post 2029)
    if s.changed.asked.get(&key).is_none_or(|a| a.2 != n) {
        return;
    }
    let Some((sent, part, _)) = s.changed.asked.remove(&key) else { return };
    s.changed.done.entry(key.clone()).or_default().insert(sent.clone());
    if !ok {
        crate::log::line(&format!("cleaning: not cleaned ({})", out.trim()));
        return;
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&out) else { return };
    let Some(now) = s.composers.get(&key).map(|e| e.text.clone()) else { return };
    let edits: Vec<(Range<usize>, String)> = v["cleaned"]
        .as_str()
        .map(|cleaned| {
            align(&sent[part.clone()], cleaned)
                .into_iter()
                .map(|(r, w)| (r.start + part.start..r.end + part.start, w))
                .filter(|(r, _)| !crate::spell::left_in(s, &key, &sent[r.clone()]))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let to = moved(&sent, &now, &edits);
    // what was sent counts as cleaned, also when the answer was Unreadable (asking again would give the same); a
    // paragraph with an edit that could not go in is asked again
    let left: Vec<&Range<usize>> = edits.iter().zip(&to).filter(|(_, t)| t.is_none()).map(|((r, _), _)| r).collect();
    let clean = s.changed.clean.entry(key.clone()).or_default();
    clean.extend(
        paragraphs(&sent)
            .into_iter()
            .filter(|r| r.start >= part.start && r.end <= part.end && !left.iter().any(|e| e.start >= r.start && e.end <= r.end))
            .map(|r| sent[r].to_string()),
    );
    if v["cleaned"].as_str().is_none() {
        return;
    }
    let as_cleaned = with_edits(&sent, &edits);
    let all = edits.len();
    let edits: Vec<(Range<usize>, String)> = edits.into_iter().zip(to).filter_map(|((_, w), t)| Some((t?, w))).collect();
    if now != sent && all > 0 {
        // counts only, never his words
        crate::log::line(&format!("cleaning: he typed on while it was cleaned; {} of {all} changes put in", edits.len()));
    }
    // those on the line he is typing on wait as ghosts until he stops or leaves it (suggest.rs): an answer coming 5 s
    // after he paused never moves the word he is typing (docket-edge5, N-1426 p2, 4)
    let edits = crate::suggest::hold(s, &key, edits);
    if edits.is_empty() {
        return;
    }
    let his: Vec<String> = edits.iter().flat_map(|(r, _)| now[r.clone()].split_whitespace().map(String::from).collect::<Vec<_>>()).collect();
    s.changed.put.entry(key.clone()).or_default().insert(now.clone(), his);
    if now != sent {
        // a batch he takes back with Ctrl+Z stays taken back, as for a text the box still held (consult-2 N-584.p18, 1)
        s.changed.done.entry(key.clone()).or_default().insert(now.clone());
    }
    crate::spell::apply_changes(s, &key, &edits);
    if let Some(e) = s.composers.get(&key) {
        let after = e.text.clone();
        // a paragraph of the box now as the answer made it is cleaned; one he typed in since is asked at the next pause
        let made: std::collections::HashSet<&str> = paragraphs(&as_cleaned).into_iter().map(|r| &as_cleaned[r]).collect();
        let clean = s.changed.clean.entry(key.clone()).or_default();
        clean.extend(paragraphs(&after).into_iter().map(|r| after[r].to_string()).filter(|p| made.contains(p.as_str())));
        if now == sent {
            s.changed.done.entry(key).or_default().insert(after);
        }
    }
}

/// Where each of EDITS (ranges of SENT, in order) stands in NOW, the box's text when the answer came, or none: its
/// words, lined up word by word against NOW, stand there together and unchanged, with the word before and the word
/// after them in SENT (where it has them) beside them in NOW too, and not as the last thing in NOW (a word he may
/// still be typing).
fn moved(sent: &str, now: &str, edits: &[(Range<usize>, String)]) -> Vec<Option<Range<usize>>> {
    if sent == now {
        return edits.iter().map(|(r, _)| Some(r.clone())).collect();
    }
    let (a, b) = (words(sent), words(now));
    if a.len() > MOST || b.len() > MOST {
        return vec![None; edits.len()];
    }
    // each word of SENT that stands in NOW, as the word of NOW it is (the longest run of words common to both)
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if sent[a[i].clone()] == now[b[j].clone()] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let mut to: Vec<Option<usize>> = vec![None; n];
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if sent[a[i].clone()] == now[b[j].clone()] {
            to[i] = Some(j);
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    edits
        .iter()
        .map(|(r, _)| {
            let first = a.iter().position(|w| w.start == r.start)?;
            let last = a.iter().position(|w| w.end == r.end)?;
            let k = to[first]?;
            (first..=last).all(|i| to[i] == Some(k + i - first)).then_some(())?;
            // the words either side stand beside it as sent: a word alone that the lining-up found in a sentence he
            // rewrote around it is not the word the cleaning saw (consult-5, N-412 post 2029)
            (first == 0 || (k > 0 && to[first - 1] == Some(k - 1))).then_some(())?;
            (last + 1 == n || to[last + 1] == Some(k + last - first + 1)).then_some(())?;
            let at = b[k].start..b[k + last - first].end;
            (at.end < now.len()).then_some(at)
        })
        .collect()
}

/// EDITS, ranges of SENT, carried into NOW as `moved` places them; the others are left for the next pause.
#[cfg(test)]
pub(crate) fn carried(sent: &str, now: &str, edits: Vec<(Range<usize>, String)>) -> Vec<(Range<usize>, String)> {
    let to = moved(sent, now, &edits);
    edits.into_iter().zip(to).filter_map(|((_, w), t)| Some((t?, w))).collect()
}

/// T with EDITS (its ranges, in order, apart) put in.
fn with_edits(t: &str, edits: &[(Range<usize>, String)]) -> String {
    let mut out = String::with_capacity(t.len());
    let mut at = 0;
    for (r, w) in edits {
        out.push_str(&t[at..r.start]);
        out.push_str(w);
        at = r.end;
    }
    out.push_str(&t[at..]);
    out
}

/// The part of box KEY's TEXT a pause sends: from its first paragraph not yet cleaned to its last, or nothing when
/// each is (paper-research N-584.p46, item 6).
pub fn to_ask(s: &App, key: &(usize, String), text: &str) -> Option<Range<usize>> {
    let clean = s.changed.clean.get(key);
    let new: Vec<Range<usize>> = paragraphs(text).into_iter().filter(|r| clean.is_none_or(|c| !c.contains(&text[r.clone()]))).collect();
    let part = new.first()?.start..new.last()?.end;
    (text[part.clone()].split_whitespace().count() >= 2).then_some(part)
}

/// The paragraphs of T (runs of lines with text, between blank lines), as byte ranges.
fn paragraphs(t: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let (mut start, mut end): (Option<usize>, usize) = (None, 0);
    let mut at = 0;
    for line in t.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        if body.trim().is_empty() {
            if let Some(a) = start.take() {
                out.push(a..end);
            }
        } else {
            let lead = body.len() - body.trim_start().len();
            start.get_or_insert(at + lead);
            end = at + body.trim_end().len();
        }
        at += line.len();
    }
    if let Some(a) = start {
        out.push(a..end);
    }
    out
}

/// Box KEY sent: the texts whose edits went in, the paragraphs cleaned and the texts asked are forgotten, so typing
/// a sent text again in a later message is not taken for a Ctrl+Z and is cleaned again (consult-2 N-584.p43, 1; p48).
/// An ask still out is no longer waited for: its answer goes nowhere, and the next message is asked at its own pause
/// (consult-5, N-412 post 2029).
pub fn sent(s: &mut App, key: &(usize, String)) {
    s.changed.asked.remove(key);
    s.changed.put.remove(key);
    s.changed.clean.remove(key);
    s.changed.done.remove(key);
    crate::suggest::sent(s, key);
}

/// The words of T (runs of non-space), as byte ranges.
fn words(t: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in t.char_indices() {
        match (c.is_whitespace(), start) {
            (true, Some(a)) => {
                out.push(a..i);
                start = None;
            }
            (false, None) => start = Some(i),
            _ => {}
        }
    }
    if let Some(a) = start {
        out.push(a..t.len());
    }
    out
}

/// CLEANED lined up against TYPED word by word: each difference that is a word replaced, two joined or one split
/// (one or two words against one or two) as the range of TYPED and the words that take its place. Words only added
/// or only dropped, and longer rewrites, are left as he typed them.
pub fn align(typed: &str, cleaned: &str) -> Vec<(Range<usize>, String)> {
    let (a, b) = (words(typed), words(cleaned));
    if a.len() > MOST || b.len() > MOST {
        return Vec::new();
    }
    let (n, m) = (a.len(), b.len());
    // the longest run of words common to both, from the end
    let mut lcs = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if typed[a[i].clone()] == cleaned[b[j].clone()] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    let (mut gi, mut gj) = (0, 0);
    let gap = |gi: usize, i: usize, gj: usize, j: usize, out: &mut Vec<(Range<usize>, String)>| {
        let (k, l) = (i - gi, j - gj);
        if (1..=2).contains(&k) && (1..=2).contains(&l) {
            out.push((a[gi].start..a[i - 1].end, cleaned[b[gj].start..b[j - 1].end].to_string()));
        }
    };
    while i < n && j < m {
        if typed[a[i].clone()] == cleaned[b[j].clone()] {
            gap(gi, i, gj, j, &mut out);
            i += 1;
            j += 1;
            (gi, gj) = (i, j);
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    gap(gi, n, gj, m, &mut out);
    out
}

/// Whether box KEY's text TEXT would be asked again at the next pause.
#[cfg(test)]
pub fn would_ask(s: &App, key: &(usize, String), text: &str) -> bool {
    !s.changed.done.get(key).is_some_and(|d| d.contains(text))
}

/// How many texts of box KEY are remembered as having had edits put in.
#[cfg(test)]
pub fn put_count(s: &App, key: &(usize, String)) -> usize {
    s.changed.put.get(key).map_or(0, |p| p.len())
}

/// Box KEY asks for TEXT as a pause does: the ask's number.
#[cfg(test)]
pub fn asked_for(s: &mut App, key: &(usize, String), text: &str) -> u64 {
    let part = to_ask(s, key, text).unwrap_or(0..text.len());
    s.changed.asks += 1;
    s.changed.asked.insert(key.clone(), (text.to_string(), part, s.changed.asks));
    s.changed.asks
}

/// The answer to box KEY's ask that is out.
#[cfg(test)]
pub fn answer(s: &mut App, key: (usize, String), ok: bool, out: String) {
    let n = s.changed.asked.get(&key).map_or(0, |a| a.2);
    replied(s, key, n, ok, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cleaned_text_lines_up_word_by_word() {
        let t = "I thnik it is teh box";
        let e = align(t, "I think it is the box");
        assert_eq!(e.iter().map(|(r, w)| (&t[r.clone()], w.as_str())).collect::<Vec<_>>(), [("thnik", "think"), ("teh", "the")]);
        // two joined, one split, punctuation with its word
        let t = "every one has alot of it,ok";
        let e = align(t, "everyone has a lot of it, ok");
        assert_eq!(e.iter().map(|(r, w)| (&t[r.clone()], w.as_str())).collect::<Vec<_>>(), [("every one", "everyone"), ("alot", "a lot"), ("it,ok", "it, ok")]);
        // a word only added or only dropped is left
        assert!(align("send it now", "send it right now").is_empty());
        assert!(align("send it right now", "send it now").is_empty());
        // spaces alone do not count
        assert!(align("a  b\nc", "a b c").is_empty());
        // paragraphs: between blank lines, without their edge spaces
        let t = "  one two\nthree\n\n \nfour five \n";
        assert_eq!(paragraphs(t).into_iter().map(|r| &t[r]).collect::<Vec<_>>(), ["one two\nthree", "four five"]);
        // the ranges are his text's, after a word of more than one byte
        let t = "naïve teh";
        assert_eq!(align(t, "naïve the").iter().map(|(r, _)| &t[r.clone()]).collect::<Vec<_>>(), ["teh"]);
    }

    /// He typed on while it was cleaned: each edit goes where its word stands whole as it was sent, the others wait.
    #[test]
    fn an_answer_that_comes_while_he_types_on_goes_where_his_words_stand() {
        let carry = |sent: &str, now: &str, cleaned: &str| -> String { with_edits(now, &carried(sent, now, align(sent, cleaned))) };
        let sent = "I thnik it is teh box";
        // typed on at the end, the usual case
        assert_eq!(carry(sent, "I thnik it is teh box and mroe", "I think it is the box"), "I think it is the box and mroe");
        // a new paragraph after it
        assert_eq!(carry(sent, "I thnik it is teh box\n\nsend it", "I think it is the box"), "I think it is the box\n\nsend it");
        // the last word sent is one he is still typing: left; a space after it ends it
        assert_eq!(carry("I thnik it is teh", "I thnik it is tehe", "I think it is the"), "I think it is tehe");
        assert_eq!(carry("I thnik it is teh", "I thnik it is teh ", "I think it is the"), "I think it is the ");
        // a change before them: the words after it shift to their place
        assert_eq!(carry(sent, "So I thnik it is teh box", "I think it is the box"), "So I think it is the box");
        // the word before one changed: that edit waits for the next pause, the others go in
        assert_eq!(carry(sent, "We thnik it is teh box", "I think it is the box"), "We thnik it is the box");
        // he changed one of the words: that one is his, the rest go in
        assert_eq!(carry(sent, "I thnk it is teh box", "I think it is the box"), "I thnk it is the box");
        // letters typed onto a word's front take it out
        assert_eq!(carry(sent, "I thnik it is ateh box", "I think it is the box"), "I think it is ateh box");
        // changes on both sides: the words between them still stand, and are cleaned
        assert_eq!(carry(sent, "So I thnik it is teh box now", "I think it is the box"), "So I think it is the box now");
        // a word he went back to fix while typing on at the end: his fix stands, the others go in
        assert_eq!(carry("we sohuld go nwo", "we shuold go nwo please", "we should go now"), "we shuold go now please");
        // the word at the very end of his text is one he may still be typing
        assert_eq!(carry(sent, "I thnik it is teh", "I think it is the box"), "I think it is teh");
        // all of it gone: nothing goes in
        assert_eq!(carry(sent, "", "I think it is the box"), "");
        // the text as sent: every edit, as before
        assert_eq!(carry(sent, sent, "I think it is the box"), "I think it is the box");
        // bytes of more than one byte at the edge of his change; the word after one changed: that edit waits
        assert_eq!(carry("naïve teh box", "naïve teh box é", "naïve the box"), "naïve the box é");
        assert_eq!(carry("é so teh box", "è so teh box", "é so the box"), "è so the box");
        assert_eq!(carry("naïve teh box", "naïve teh boxé", "naïve the box"), "naïve teh boxé");
        // a sentence he rewrote around a word the cleaning changed: the word alone, found elsewhere, takes no edit
        // (consult-5, N-412 post 2029)
        assert_eq!(carry("so its fine", "a new idea: its colour", "so it's fine"), "a new idea: its colour");
        assert_eq!(carry("I thnik its fine", "its a new idea, I thnik", "I think it's fine"), "its a new idea, I thnik");
        // two words changed side by side are one edit ("thnik its" for "think it's"): the word after it is gone, so the
        // whole edit waits for the next pause, which asks for "thnik" again
        assert_eq!(carry("I thnik its fine", "I thnik its a new idea", "I think it's fine"), "I thnik its a new idea");
    }

    /// While he types in a box whose words are cleaned, the pause's wake is asked once per tick, not on each poll
    /// (N-1272: asked on each poll, each frame of the pause asked for one more frame).
    #[test]
    fn a_box_waiting_for_its_pause_wakes_the_window_once_per_tick() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::view_cost::project_for_tests();
        p.protocol = coord_types::PROTOCOL;
        s.projects = vec![p];
        s.spell.on = true;
        let key = (0usize, "paper-research".to_string());
        let mut e = rune::TextEdit::new("I thnik");
        e.focused = true;
        s.composers.insert(key.clone(), e);
        let mut tx = rune::Tx::default();
        let began = Instant::now();
        poll(&mut s, &mut tx);
        let tick = *s.ticks.get(&(PAUSE / 3)).expect("waiting for its pause: the pause's tick is set");
        for c in " it is teh".chars() {
            s.composers.get_mut(&key).unwrap().text.push(c);
            poll(&mut s, &mut tx);
        }
        if began.elapsed() >= PAUSE / 3 {
            eprintln!("the polls took a whole tick here: not judged");
            return;
        }
        assert_eq!(s.ticks.get(&(PAUSE / 3)), Some(&tick), "the keys within the tick ask for no further wake");
    }

    /// The cleaning failed (the gateway refused every model the policy allows, paper-research N-1397.p27): his box
    /// keeps what he typed, letter for letter, and the same words are not asked again.
    #[test]
    fn a_cleaning_that_failed_leaves_his_box_as_he_typed_it() {
        let (_held, mut s) = crate::view_cost::test_app();
        let key = (0usize, "paper-research".to_string());
        let typed = "I thnik it is teh box  , as N-487 said";
        s.composers.insert(key.clone(), rune::TextEdit::new(typed));
        s.changed.asked.insert(key.clone(), (typed.to_string(), 0..typed.len(), 7));
        replied(&mut s, key.clone(), 7, false, "no model answered: 400 Bad Request".into());
        assert_eq!(s.composers[&key].text, typed);
        assert!(!s.changed.asked.contains_key(&key), "no longer waited for");
        assert!(s.changed.done[&key].contains(typed), "not asked again");
    }
}
