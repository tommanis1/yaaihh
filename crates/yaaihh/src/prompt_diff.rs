//! The system prompt as it is sent, read against Claude Code's own (the owner, N-1980): one document in its order, a
//! part of Claude Code's left out a collapsed struck pill with what it saves (`−142 tok · Session-specific guidance`)
//! that opens to its text and Restore, and each addition (a part changed or his own, the standing texts) marked with
//! a `+` in its gutter, under a line that adds them up: `Base 14.2k − 1.8k + 0.6k = 13.0k tok`.
//!
//! Tokens are estimated as characters ÷ 4: the record keeps a request's tokens, not a part's.

use crate::App;
use crate::prompt_parts::{Part, Pick};
use crate::prompt_view::{Section, Source, sections};
use coord_types::StandingPart;
use rune::{El, col, row, text};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

/// TEXT's tokens, estimated.
pub fn tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

/// N tokens as the sum line writes them: `640`, `14.2k`.
pub fn short(n: usize) -> String {
    if n < 1000 { n.to_string() } else { format!("{:.1}k", n as f64 / 1000.0) }
}

/// One row of the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    /// Sent as Claude Code sends it.
    Same(Section),
    /// Claude Code's part NAME, left out: its TEXT not sent, TOKENS saved. RESTORABLE by the parts editor; a recipe's
    /// cut is the recipe's to undo.
    Stripped { name: String, text: String, tokens: usize, restorable: bool },
    /// Claude Code's part NAME sent in other words: WAS its own.
    Changed { name: String, was: String, now: Section },
    /// Sent and not Claude Code's: a part of his own, or the standing texts after it.
    Added(Section),
}

/// The document, and the sum: Claude Code's own (BASE), what is left out of it or changed (MINUS), what is sent in
/// their place or after them (PLUS).
#[derive(Debug, Default)]
pub struct Diff {
    pub rows: Vec<Row>,
    pub base: usize,
    pub minus: usize,
    pub plus: usize,
    /// What a recipe could not do, said under the sum.
    pub notes: Vec<String>,
}

impl Diff {
    /// The sum line's words.
    pub fn sum(&self) -> String {
        format!("Base {} − {} + {} = {} tok", short(self.base), short(self.minus), short(self.plus), short(self.base.saturating_add(self.plus).saturating_sub(self.minus)))
    }
}

/// SENT (the parts of the text sent now) read against STOCK (Claude Code's own as it first sent them), in SENT's order,
/// each part of STOCK it leaves out at its place; IDENTITY first, as Claude Code's; the parts that are ours cut at the
/// standing FILES (`prompt_view::sections`), his words among them his where HIS.
pub fn diff(identity: Option<&str>, stock: &[Part], sent: &[Part], files: &[StandingPart], his: bool) -> Diff {
    let mut d = Diff::default();
    let own: Vec<&Part> = stock.iter().filter(|p| !p.ours).collect();
    if let Some(t) = identity.map(str::trim).filter(|t| !t.is_empty()) {
        d.base += tokens(t);
        d.rows.push(Row::Same(Section { source: Source::Harness, text: t.to_string() }));
    }
    d.base += own.iter().map(|p| tokens(&p.text)).sum::<usize>();
    let mut at = 0;
    for p in sent.iter().filter(|p| !p.ours) {
        let Some(j) = own[at..].iter().position(|q| q.name == p.name).map(|j| at + j) else {
            d.plus += tokens(&p.text);
            d.rows.push(Row::Added(Section { source: Source::Override, text: p.text.clone() }));
            continue;
        };
        for q in &own[at..j] {
            d.minus += tokens(&q.text);
            d.rows.push(Row::Stripped { name: q.name.clone(), text: q.text.clone(), tokens: tokens(&q.text), restorable: true });
        }
        let q = own[j];
        if q.text.trim() == p.text.trim() {
            d.rows.push(Row::Same(Section { source: Source::Harness, text: p.text.clone() }));
        } else {
            (d.minus, d.plus) = (d.minus + tokens(&q.text), d.plus + tokens(&p.text));
            d.rows.push(Row::Changed { name: q.name.clone(), was: q.text.clone(), now: Section { source: Source::Override, text: p.text.clone() } });
        }
        at = j + 1;
    }
    for q in &own[at..] {
        d.minus += tokens(&q.text);
        d.rows.push(Row::Stripped { name: q.name.clone(), text: q.text.clone(), tokens: tokens(&q.text), restorable: true });
    }
    let ours: Vec<Part> = sent.iter().filter(|p| p.ours).cloned().collect();
    for s in sections(None, &ours, &[], his, files) {
        d.plus += tokens(&s.text);
        d.rows.push(Row::Added(s));
    }
    d
}

/// The document a prompt recipe says it made (roster's diff file, gap-homes N-1981 p2): its kept segments Claude Code's,
/// its stripped ones pills, its added ones a pack's or the standing text; a segment missing (a cut that matched
/// nothing) said under the sum; its own token estimates.
pub fn of_file(f: &coord_types::PromptDiff) -> Diff {
    let mut d = Diff { notes: f.notes.clone(), ..Diff::default() };
    for g in &f.segments {
        let n = if g.tokens > 0 { g.tokens } else { tokens(&g.text) };
        match g.kind.as_str() {
            "kept" => {
                d.base += n;
                d.rows.push(Row::Same(Section { source: Source::Harness, text: g.text.clone() }));
            }
            "stripped" => {
                (d.base, d.minus) = (d.base + n, d.minus + n);
                d.rows.push(Row::Stripped { name: g.label.clone(), text: g.text.clone(), tokens: n, restorable: false });
            }
            "added" => {
                d.plus += n;
                let source = if g.label.starts_with("pack") { Source::Pack } else { Source::Standing };
                d.rows.push(Row::Added(Section { source, text: g.text.clone() }));
            }
            _ => d.notes.push(format!("{} not found in the harness's text", g.label)),
        }
    }
    d
}

thread_local! {
    /// The document last worked out for each agent's pane, by project and name, with the key of what it was read from.
    static KEPT: std::cell::RefCell<std::collections::HashMap<(usize, String), (u64, std::rc::Rc<Diff>)>> = Default::default();
}

/// A key of X: equal while what a diff is read from is.
pub fn key_of(x: &impl std::hash::Hash) -> u64 {
    use std::hash::{BuildHasher, BuildHasherDefault};
    BuildHasherDefault::<std::collections::hash_map::DefaultHasher>::default().hash_one(x)
}

/// Agent NAME's document in project PI: the one kept while KEY is unchanged, else MAKE's, kept.
pub fn kept(pi: usize, name: &str, key: u64, make: impl FnOnce() -> Diff) -> std::rc::Rc<Diff> {
    let at = (pi, name.to_string());
    if let Some(d) = KEPT.with_borrow(|m| m.get(&at).filter(|(k, _)| *k == key).map(|(_, d)| d.clone())) {
        return d;
    }
    let d = std::rc::Rc::new(make());
    KEPT.with_borrow_mut(|m| m.insert(at, (key, d.clone())));
    d
}

/// [`of_file`] of F, kept for agent NAME of project PI while F is unchanged.
pub fn of_file_kept(pi: usize, name: &str, f: &coord_types::PromptDiff) -> std::rc::Rc<Diff> {
    let key = key_of(&(&f.notes, f.segments.iter().map(|g| (&g.kind, &g.label, &g.text, g.tokens)).collect::<Vec<_>>()));
    kept(pi, name, key, || of_file(f))
}

/// The document D of agent NAME's in project PI drawn: its sum line, then each row, a run of one source under its
/// quiet marker as `prompt_view::drawn` marks it.
pub fn drawn(s: &App, pi: usize, name: &str, d: &Diff, k: &Kit) -> Vec<El> {
    let sum = text(d.sum()).font(k.ty.small_medium).color(k.c.text_muted).single_line().key("prompt-sum").help("Tokens estimated as characters ÷ 4");
    let mut out = vec![row().w_full().pb(4.0).child(sum)];
    out.extend(d.notes.iter().map(|n| text(n.clone()).font(k.ty.small).color(k.c.text_faint).wrap()));
    let mut last: Option<Source> = None;
    for (i, r) in d.rows.iter().enumerate() {
        let source = match r {
            Row::Same(x) | Row::Added(x) | Row::Changed { now: x, .. } => Some(x.source),
            Row::Stripped { .. } => None,
        };
        let marker = (source.is_some() && source != last).then(|| text(source.map_or("", Source::words)).font(k.ty.small).color(k.c.text_faint).single_line().key(format!("prompt-from-{i}")));
        last = source.or(last);
        let el = match r {
            Row::Same(x) => body(&x.text, false, k),
            Row::Added(x) => body(&x.text, true, k),
            Row::Changed { name: part, was, now } => {
                let delta = format!("changed · −{} +{} tok", short(tokens(was)), short(tokens(&now.text)));
                col().w_full().gap(4.0).child(text(format!("{part} · {delta}")).font(k.ty.small).color(k.c.text_faint).key(format!("prompt-changed-{part}"))).child(body(&now.text, true, k))
            }
            Row::Stripped { name: part, text: t, tokens: n, restorable } => pill(s, pi, name, (part, t, *n), *restorable, k),
        };
        out.push(col().key(format!("prompt-row-{i}")).w_full().gap(4.0).pt(if marker.is_some() && i > 0 { 14.0 } else { 6.0 }).children(marker).child(el));
    }
    out
}

/// TEXT as Markdown, a faint rule down its left; an addition's a `+` in its gutter and the rule in the success colour.
fn body(t: &str, added: bool, k: &Kit) -> El {
    let md = rune_kit::markdown::Markdown::new(t.to_string()).selectable(true).build(k);
    let rule = if added { k.c.success.icon } else { k.c.border_weak };
    let gutter = text(if added { "+" } else { "" }).font(k.ty.small).color(k.c.success.icon).w(10.0);
    row().w_full().gap(2.0).child(gutter).child(col().grow(1.0).shrink().min_w(0.0).pl(10.0).border_sides([0.0, 0.0, 0.0, 2.0], rule).child(md))
}

/// Claude Code's part PART (its text T, N tokens), left out of agent NAME's prompt: a translucent struck pill with what
/// it saves and its first line; a press opens it to its whole text, struck, and where RESTORABLE Restore, which drafts
/// it back in the parts editor (Undo while it is drafted so); a recipe's cut says so.
fn pill(s: &App, pi: usize, name: &str, (part, t, n): (&str, &str, usize), restorable: bool, k: &Kit) -> El {
    let first = t.lines().find(|l| !l.trim().is_empty()).unwrap_or_default().trim().to_string();
    let words = format!("−{} tok · {part}", short(n));
    let key = (pi, name.to_string(), part.to_string());
    let open = s.prompt_edit.pills.contains(&key);
    let struck = |line: String, font| {
        let n = line.len();
        text(line).font(font).color(k.c.text_faint).strike(0, n, None)
    };
    let k2 = key.clone();
    let head = row().w_full().gap(8.0).center_y().child(text(words).font(k.ty.small_medium).color(k.c.text_muted).single_line()).child(struck(first, k.ty.small).single_line().truncate().shrink().min_w(0.0));
    let pressed = k.interactive(head).help(if open { "Fold" } else { "Show what is left out" }).on_press(move |s: &mut App| {
        if !s.prompt_edit.pills.remove(&k2) {
            s.prompt_edit.pills.insert(k2.clone());
        }
    });
    let mut c = col().key(format!("prompt-pill-{part}")).w_full().padding(6.0).gap(6.0).radius(k.radius.md).fill(k.c.layer_01).opacity(0.7).child(pressed);
    if open && !restorable {
        c = c.child(struck(t.trim().to_string(), k.ty.small).wrap()).child(text("Cut by the agent's prompt recipe").font(k.ty.small).color(k.c.text_faint));
    } else if open {
        let restored = crate::prompt_edit::draft_of(s, pi, name).and_then(|p| p.get(part)).is_some_and(|(p, _)| *p == Pick::Stock);
        let (n, pa, tx) = (name.to_string(), part.to_string(), t.to_string());
        let act = if restored {
            Button::new("Undo restore").on_press(move |s: &mut App| crate::prompt_edit::reset_part(s, pi, &n, &pa))
        } else {
            Button::new("Restore").on_press(move |s: &mut App| crate::prompt_edit::pick_part(s, pi, &n, &pa, Pick::Stock, &tx, Pick::Off))
        };
        let note = if restored { "Drafted back in: Edit by part sends it" } else { "Not sent now" };
        c = c.child(struck(t.trim().to_string(), k.ty.small).wrap()).child(
            row().gap(8.0).center_y().child(act.variant(ButtonVariant::Ghost).control_size(ControlSize::Small).build(k).key(format!("prompt-restore-{part}"))).child(text(note).font(k.ty.small).color(k.c.text_faint)),
        );
    }
    c
}

#[cfg(test)]
#[path = "prompt_diff_tests.rs"]
mod tests;
