//! A text beside the drawing (N-1988): the declaration pressed, with the comment lines over it, or the whole text with
//! it lit; its words coloured as the language reads them; under each line what the check found there and, from the
//! engine, how many of each Fact and Duty declared there hold, a press listing them; and why one holds, or an act is
//! not enabled, as the engine explains it (live.rs), each place in it a press to its line.

use super::check::Level;
use super::live::{self, Said};
use super::{Built, Live};
use crate::App;
use docket_v2::syntax::tree::{DeclKind, Norm};
use rune::{Color, El, Span, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit};
use serde_json::Value as Json;

/// The words the language reads as its own.
const KEYWORDS: &[&str] = &[
    "module", "import", "as", "Fact", "Act", "Event", "Duty", "Effect", "Call", "type", "record", "fn", "on", "where", "do", "return", "perform",
    "Holds", "when", "Creates", "Terminates", "Conditioned", "by", "Derived", "from", "Violated", "Identified", "Needs", "Does", "Returns", "match",
    "if", "then", "else", "par", "in", "Foreach", "When", "Exists", "Forall", "Not", "Extend", "Obfuscates", "Syncs", "with", "Prefer", "let", "Program",
];
/// The most instances listed.
const LISTED: usize = 50;

/// The spans of WORDS as the language reads them: comments, texts, its own words, a module's own facts (`+`).
pub fn spans(words: &str, k: &Kit) -> Vec<Span> {
    let b = words.as_bytes();
    let (mut out, mut i, mut line_start) = (Vec::new(), 0, true);
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'-';
    while i < b.len() {
        let c = b[i];
        if c == b'\n' {
            (i, line_start) = (i + 1, true);
            continue;
        }
        if c == b' ' || c == b'\t' {
            i += 1;
            continue;
        }
        let start = i;
        let mut ink: Option<(Color, bool)> = None;
        if b[i..].starts_with(b"//") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            ink = Some((k.c.text_faint, false));
        } else if b[i..].starts_with(b"\"\"\"") {
            i += 3;
            while i < b.len() && !b[i..].starts_with(b"\"\"\"") {
                i += 1;
            }
            i = (i + 3).min(b.len());
            ink = Some((k.c.success.fg, false));
        } else if c == b'"' {
            i += 1;
            while i < b.len() && b[i] != b'"' && b[i] != b'\n' {
                i += if b[i] == b'\\' { 2 } else { 1 };
            }
            i = (i + 1).min(b.len());
            ink = Some((k.c.success.fg, false));
        } else if c == b'+' && line_start {
            i += 1;
            while i < b.len() && (word(b[i]) || b[i] == b'.') {
                i += 1;
            }
            ink = Some((k.c.info.fg, true));
        } else if word(c) && !c.is_ascii_digit() && c != b'-' {
            while i < b.len() && word(b[i]) {
                i += 1;
            }
            if KEYWORDS.contains(&&words[start..i]) {
                ink = Some((k.c.text_accent, true));
            }
        } else {
            i += 1;
        }
        line_start = false;
        if let Some((color, bold)) = ink {
            let sp = Span::new(start, i).color(color);
            out.push(if bold { sp.bold() } else { sp });
        }
    }
    out
}

/// The lines (from 1, through the last) of the declaration at LINE in the text at PATH: from the comment lines directly
/// over it to the line before the next, its trailing blank and comment lines left out.
pub fn slice(b: &Built, path: &str, line: u32) -> Option<(u32, u32)> {
    let words = b.words(path)?;
    let (_, m) = b.checked.modules.iter().find(|(n, _)| b.path(n) == path)?;
    let mut at: Vec<u32> = m.decls.iter().map(|d| d.at.line).collect();
    at.sort_unstable();
    at.dedup();
    let first = *at.iter().rev().find(|l| **l <= line)?;
    let lines: Vec<&str> = words.lines().collect();
    let next = at.iter().find(|l| **l > first).map_or(lines.len() as u32, |l| l - 1);
    let text_at = |l: u32| lines.get(l as usize - 1).map_or("", |t| t.trim());
    let (mut from, mut to) = (first, next.max(first));
    while from > 1 && text_at(from - 1).starts_with("//") {
        from -= 1;
    }
    while to > first && (text_at(to).is_empty() || text_at(to).starts_with("//")) {
        to -= 1;
    }
    Some((from, to))
}

/// The panel beside the drawing: the text shown, and what is explained; none when neither is picked.
pub fn panel(s: &App, b: &Built, live: Option<&Live>, k: &Kit) -> Option<El> {
    let why = super::picked(s, super::WHY).map(str::to_string);
    let shown = super::shown(s);
    if shown.is_none() && why.is_none() {
        return None;
    }
    let mut c = col().w(520.0).max_h(super::VIEW_H).scroll(rune::ScrollSpec { stick_bottom: false }).gap(8.0).padding(12.0).radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_weak).key("programs-source");
    if let Some((path, line)) = shown {
        c = c.child(text_part(s, b, live, &path, line, k));
    }
    if let (Some(inst), Some(l)) = (why, live) {
        c = c.child(why_part(b, &inst, (l.explain)(&inst), k));
    }
    Some(c)
}

fn text_part(s: &App, b: &Built, live: Option<&Live>, path: &str, line: u32, k: &Kit) -> El {
    let Some(words) = b.words(path) else {
        return text(format!("{path} is not among the texts")).font(k.ty.small).color(k.c.danger.fg);
    };
    let whole = super::picked(s, super::WHOLE).is_some() || line == 0;
    let lines: Vec<&str> = words.lines().collect();
    let decl = (line > 0).then(|| slice(b, path, line)).flatten();
    let (from, to) = match decl {
        Some(r) if !whole => r,
        _ => (1, lines.len().max(1) as u32),
    };
    let lit = decl.filter(|_| whole);
    let place = if line == 0 { b.short(path).to_string() } else { format!("{}:{line}", b.short(path)) };
    let toggle = (line > 0).then(|| {
        Button::new(if whole { "Its declaration" } else { "Whole text" })
            .variant(ButtonVariant::Ghost)
            .on_press(move |s: &mut App| super::set(s, super::WHOLE, (!whole).then(String::new)))
            .build(k)
            .key("programs-whole")
    });
    let close = Button::new("Put away").variant(ButtonVariant::Ghost).on_press(|s: &mut App| super::show(s, None)).build(k).key("programs-source-close");
    let head = row().w_full().gap(8.0).center_y().child(text(place).font(k.ty.mono).color(k.c.text_base).truncate().shrink()).child(rune::spacer()).children(toggle).child(close);
    // what is said under a line: what the check found there, and how many of what it declares hold
    let diags: Vec<_> = b.checked.diags.iter().filter(|d| d.path == path && d.line >= from && d.line <= to).collect();
    let module = b.checked.modules.iter().find(|(n, _)| b.path(n) == path);
    let facts: Vec<(u32, String)> = module
        .filter(|_| live.is_some())
        .map(|(n, m)| {
            let declared = m.decls.iter().filter_map(|d| match &d.what {
                DeclKind::Norm { kind: Norm::Fact | Norm::Duty, name, .. } => Some((d.at.line, format!("{n}.{name}"))),
                _ => None,
            });
            declared.filter(|(l, _)| *l >= from && *l <= to).collect()
        })
        .unwrap_or_default();
    let marked = |l: u32| diags.iter().any(|d| d.line == l) || facts.iter().any(|(f, _)| *f == l);
    let mut body = col().w_full().gap(0.0);
    let mut start = from;
    let num_w = crate::picture::graph::measure(&to.to_string(), k.ty.mono).0 + 8.0;
    for l in from..=to {
        let cut = marked(l) || l == to || lit.is_some_and(|(a, z)| l + 1 == a || l == z);
        if !cut {
            continue;
        }
        let shown: Vec<&str> = (start..=l).map(|i| lines.get(i as usize - 1).copied().unwrap_or("")).collect();
        let code = shown.join("\n");
        let numbers: Vec<String> = (start..=l).map(|i| i.to_string()).collect();
        let mut block = row().w_full().gap(8.0).child(text(numbers.join("\n")).font(k.ty.mono).color(k.c.text_faint).w(num_w)).child(text(code.clone()).font(k.ty.mono).color(k.c.text_base).spans(spans(&code, k)));
        if lit.is_some_and(|(a, z)| start >= a && l <= z) {
            block = block.fill(k.c.layer_03).radius(k.radius.sm);
        }
        body = body.child(block);
        for d in diags.iter().filter(|d| d.line == l) {
            let ink = if d.level == Level::Error { k.c.danger.fg } else { k.c.warning.fg };
            body = body.child(row().w_full().pl(num_w + 8.0).py(2.0).child(text(format!("▲ {}", d.why)).font(k.ty.small).color(ink).wrap().grow(1.0).shrink()));
        }
        for (_, f) in facts.iter().filter(|(fl, _)| *fl == l) {
            body = body.children(held_chip(s, b, live, f, num_w, k));
        }
        start = l + 1;
    }
    col().w_full().gap(6.0).child(head).child(col().w_full().scroll_x().child(body))
}

/// How many of FACT hold, as a press that lists them; the list, when it is open.
fn held_chip(s: &App, b: &Built, live: Option<&Live>, fact: &str, indent: f32, k: &Kit) -> Vec<El> {
    let Some(l) = live else { return Vec::new() };
    let got = (l.holding)(fact);
    let n = match &got {
        Some(Ok(Json::Array(a))) => a.len(),
        _ => return vec![row().pl(indent + 8.0).child(text(if got.is_none() { "asking the engine…".to_string() } else { "the engine did not say".into() }).font(k.ty.small).color(k.c.text_faint))],
    };
    let open = super::picked(s, super::HELD) == Some(fact);
    let f = fact.to_string();
    let chip = k.interactive(row().px(6.0).py(1.0).radius(9.0).fill(if n > 0 { k.c.warning.bg } else { k.c.layer_02 }).child(text(format!("{n} hold")).font(k.ty.small_medium).color(if n > 0 { k.c.warning.fg } else { k.c.text_faint })).key(format!("programs-held-{fact}")))
        .on_press(move |s: &mut App| super::set(s, super::HELD, (!open).then(|| f.clone())));
    let mut out = vec![row().pl(indent + 8.0).py(2.0).child(chip)];
    if !open {
        return out;
    }
    let Some(Ok(Json::Array(all))) = got else { return out };
    for (i, inst) in all.iter().take(LISTED).enumerate() {
        let asked = live::written(inst, &|q| b.in_top(q));
        let mut r = row().w_full().pl(indent + 20.0).gap(8.0).center_y().child(text(live::words(inst)).font(k.ty.small).color(k.c.text_base).truncate().shrink());
        r = r.child(match asked {
            Some(q) => Button::new("Why?").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| super::set(s, super::WHY, Some(q.clone()))).build(k).key(format!("programs-why-{fact}-{i}")),
            None => text("cannot be asked: a value with no written form").font(k.ty.small).color(k.c.text_faint),
        });
        out.push(r);
    }
    if all.len() > LISTED {
        out.push(row().pl(indent + 20.0).child(text(format!("and {} more", all.len() - LISTED)).font(k.ty.small).color(k.c.text_faint)));
    }
    out
}

/// Why INST holds or not, or is not enabled, as the engine explains it now: its lines, each place a press to its line.
fn why_part(b: &Built, inst: &str, got: Option<Result<Json, String>>, k: &Kit) -> El {
    let close = Button::new("Put away").variant(ButtonVariant::Ghost).on_press(|s: &mut App| super::set(s, super::WHY, None)).build(k).key("programs-why-close");
    let head = row().w_full().gap(8.0).center_y().child(text(format!("Why: {inst}")).font(k.ty.small_medium).color(k.c.text_base).truncate().shrink()).child(rune::spacer()).child(close);
    let mut c = col().w_full().gap(2.0).pt(8.0).key("programs-why").child(head);
    let lines = match got {
        None => return c.child(text("asking the engine…").font(k.ty.small).color(k.c.text_faint)),
        Some(Err(e)) => return c.child(text(format!("the engine did not explain it: {e}")).font(k.ty.small).color(k.c.danger.fg).wrap()),
        Some(Ok(j)) => live::said(&j),
    };
    c = c.child(text("As it stands now, at the record's last line.").font(k.ty.small).color(k.c.text_faint));
    for (i, l) in lines.iter().enumerate() {
        c = c.child(said_row(b, l, i, k));
    }
    c
}

fn said_row(b: &Built, l: &Said, i: usize, k: &Kit) -> El {
    let (mark, ink) = match l.holds {
        Some(true) => ("✓", k.c.success.fg),
        Some(false) => ("✗", k.c.danger.fg),
        None => ("·", k.c.text_faint),
    };
    let mut r = row().w_full().pl(l.depth as f32 * 14.0).gap(6.0).child(text(mark).font(k.ty.small_medium).color(ink)).child(text(l.words.clone()).font(k.ty.small).color(k.c.text_base).wrap().grow(1.0).shrink());
    if let Some((path, line)) = l.at.as_deref().and_then(|a| b.checked.place(a)) {
        let at = text(format!("{path}:{line}")).font(k.ty.small).color(k.c.text_accent);
        r = r.child(k.interactive(at.key(format!("programs-said-{i}"))).on_press(move |s: &mut App| super::set(s, super::SOURCE, Some(format!("{path}\t{line}")))));
    }
    r
}

/// The record's latest refusals, newest first: the act, its party and why, and a press for why it is not enabled now.
pub fn refusals(b: &Built, l: &Live, k: &Kit) -> Option<El> {
    if l.tally.refusals.is_empty() {
        return None;
    }
    let mut c = col().w_full().gap(4.0).key("programs-refusals").child(text(format!("Refused lately: the record's last {}", l.tally.refusals.len())).font(k.ty.small_medium).color(k.c.text_base));
    for (at, r) in l.tally.refusals.iter().rev() {
        let act = &r["act"];
        let why = match &r["why"] {
            Json::String(w) if w == "source" => "not its party's to perform".to_string(),
            w => match (w.get("policy").and_then(Json::as_str), w.get("closes").and_then(Json::as_str)) {
                (Some(p), _) => p.to_string(),
                (_, Some(c)) => format!("it would end a duty its party holds ({c})"),
                _ => w.to_string(),
            },
        };
        let party = r["party"].as_str().map_or(String::new(), |p| format!(" by {p}"));
        let mut line = row().w_full().gap(8.0).center_y().child(text(format!("line {at}")).font(k.ty.mono).color(k.c.text_faint)).child(text(format!("{}{party}: {why}", live::words(act))).font(k.ty.small).color(k.c.danger.fg).wrap().grow(1.0).shrink());
        if let Some(q) = live::written(act, &|q| b.in_top(q)) {
            let decl = act["inst"].as_str().and_then(|n| b.decl(n));
            line = line.child(
                Button::new("Why?")
                    .variant(ButtonVariant::Ghost)
                    .on_press(move |s: &mut App| {
                        if let Some((p, l)) = &decl {
                            super::show(s, Some((p.as_str(), *l)));
                        }
                        super::set(s, super::WHY, Some(q.clone()));
                    })
                    .build(k)
                    .key(format!("programs-refused-{at}")),
            );
        }
        c = c.child(line);
    }
    Some(c)
}
