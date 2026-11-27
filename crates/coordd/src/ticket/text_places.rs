//! The places the record gives a text at (paper-research N-1446 p1419, 2): what `Request::Text` may read. A place is
//! read only when a line of the record names it in a field the machine types as a text's place: an opening's or a
//! post's text, an answer's text, a turn's end (its final output), a giving's `where`, a pin's two places. coordd alone
//! writes those: a request carries words as a string, never a place, so no caller can make one up. A formed answer's
//! place is its turn's end (`Effect::Form` carries the `turn_ended` line's text), and a pinned text's copy is the pin's
//! `to`, so both are here as those. A place inside a value (a form, a head, an act's `more`, an event's value) is not
//! taken: an agent or a program writes those, and could name any file there. A line that does not parse gives none.

use docket_machine::{Line, Loc, Text};
use std::collections::{HashMap, HashSet};

/// By file, each place (offset, length) a line of the record gives a text at.
#[derive(Debug, Default)]
pub(crate) struct TextPlaces(HashMap<String, HashSet<(u64, u64)>>);

impl TextPlaces {
    /// The places LINE gives a text at, taken in.
    pub(crate) fn take(&mut self, line: &Line) {
        match line {
            Line::Open { text: Some(t), .. } | Line::Post { text: t, .. } | Line::Answer { text: t, .. } => self.text(t),
            Line::TurnEnded { text, .. } => self.put(text),
            Line::Given { place: Some(p), .. } => self.put(p),
            Line::Pinned { loc, to, .. } => {
                self.put(loc);
                self.put(to);
            }
            _ => {}
        }
    }

    /// The place of each text T holds by reference: its own, a span's, each piece's of a joined one.
    fn text(&mut self, t: &Text) {
        match t {
            Text::Loc { loc } => self.put(loc),
            Text::Span { span } => self.put(&span.loc),
            Text::Joined { joined } => joined.iter().for_each(|t| self.text(t)),
            Text::New(_) | Text::Part { .. } | Text::Post { .. } => {}
        }
    }

    fn put(&mut self, l: &Loc) {
        if !l.file.is_empty() && l.len > 0 {
            self.0.entry(l.file.clone()).or_default().insert((l.offset, l.len));
        }
    }

    /// Whether a line of the record gives a text at L: its file, offset and length all as the line has them.
    pub(crate) fn holds(&self, l: &Loc) -> bool {
        self.0.get(&l.file).is_some_and(|s| s.contains(&(l.offset, l.len)))
    }

    /// The places, in order, to save with a replay stopped at a cut (replay_state.rs): the same places give the same
    /// bytes.
    pub(crate) fn saved(&self) -> std::collections::BTreeMap<String, std::collections::BTreeSet<(u64, u64)>> {
        self.0.iter().map(|(f, s)| (f.clone(), s.iter().copied().collect())).collect()
    }

    /// The places SAVED.
    pub(crate) fn from_saved(saved: std::collections::BTreeMap<String, std::collections::BTreeSet<(u64, u64)>>) -> TextPlaces {
        TextPlaces(saved.into_iter().map(|(f, s)| (f, s.into_iter().collect())).collect())
    }
}
