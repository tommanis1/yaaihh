//! A chat as the owner reads it (owner 7 Nov night): what the agent says, prompts, mail and questions stay in view;
//! each run of tool calls between them is one line of work. While it is the agent's latest and the agent works, the
//! line turns a spinner and shows the call it is on (its tool and the first characters of what it runs); otherwise it
//! sums the run up (how many calls, of which tools, over what time). A click opens the run as one line per call; a
//! click on a call shows the call and its result (each a few lines, the rest on another click, chat.rs `entry`); an
//! edit's line counts the lines it added and removed, and its open call shows the diff (owner 8 Nov).
//!
//! A call's result is the next result in the run not yet taken: Claude Code writes parallel calls first and their
//! results after, in the same order.

use crate::App;
use crate::live::Conv;
use coord_types::Entry;
use rune::{El, col, row, text};
use rune_kit::{Kit, Spinner};
use std::collections::HashSet;

/// Characters of a call shown on its one line, and on the working line.
const GIST: usize = 50;

/// One row of a chat.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// An entry in view: text, a prompt, mail, a question's card.
    Entry(usize),
    /// A run of work, entries FROM..TO.
    Work { from: usize, to: usize },
    /// One call of an open run: its entry and its result's.
    Call { call: usize, result: Option<usize> },
    /// A pass of the agent's brain that spoke, by its ticket's number, at the time it spoke: none of the session's own
    /// rows (chat.rs `with_passes`; his N-1104 p625).
    Pass(u64),
    /// His edit of the agent's system prompt, `Conv::restarted`'s MARK, standing before entry AT (or after them all):
    /// "System prompt changed", made from a turn with the old line folded beside it (chat.rs `restarted_row`; path
    /// fix 6 point 3, accounts-lead N-1446 p2524).
    Restarted { mark: usize, at: usize },
}

/// Whether entry E is work (folded into a run) rather than in view. ASKED: whether a result is a question's card.
fn is_work(e: &Entry, asked: impl Fn(&str) -> bool) -> bool {
    match e.kind.as_str() {
        "assistant" | "user" | "mail" => false,
        "result" => !asked(&e.text),
        _ => true,
    }
}

/// The key an open run is kept under in `App::expanded`: its first entry's.
pub fn run_key(c: &Conv, from: usize) -> String {
    format!("w-{}", c.keys[from])
}

/// The key an open call is kept under: its entry's.
pub fn call_key(c: &Conv, call: usize) -> String {
    format!("c-{}", c.keys[call])
}

/// The rows of conversation C, the runs in EXPANDED open, his system-prompt edits where they fell. ASKED: whether a
/// result is a question's card.
pub fn items(c: &Conv, expanded: &HashSet<String>, asked: impl Fn(&str) -> bool) -> Vec<Item> {
    let mut out = Vec::new();
    // his edits, the first last: each before the first entry at or after its byte, ending a run there
    let mut marks = restarted_at(c);
    marks.reverse();
    let mut i = 0;
    loop {
        while let Some(&(at, mark)) = marks.last().filter(|m| m.0 <= i) {
            out.push(Item::Restarted { mark, at });
            marks.pop();
        }
        if i >= c.entries.len() {
            break;
        }
        if !is_work(&c.entries[i], &asked) {
            out.push(Item::Entry(i));
            i += 1;
            continue;
        }
        let from = i;
        let stop = marks.last().map_or(c.entries.len(), |m| m.0);
        while i < stop && is_work(&c.entries[i], &asked) {
            i += 1;
        }
        out.push(Item::Work { from, to: i });
        if expanded.contains(&run_key(c, from)) {
            out.extend(calls(&c.entries[from..i]).into_iter().map(|(call, result)| Item::Call { call: from + call, result: result.map(|r| from + r) }));
        }
    }
    out
}

/// Where each of his system-prompt edits held in C stands among its entries, with its place in `Conv::restarted`, in
/// order: before the first entry at or after its byte, or after them all (path fix 6 point 3). One before what is
/// held, or past it, stands nowhere until that part is read.
fn restarted_at(c: &Conv) -> Vec<(usize, usize)> {
    let held = |o: u64| c.loaded && c.start <= o && o <= c.next;
    let mut at: Vec<(usize, usize)> =
        c.restarted.iter().enumerate().filter(|(_, r)| held(r.offset)).map(|(m, r)| (c.entries.partition_point(|e| e.offset < r.offset), m)).collect();
    at.sort_unstable();
    at
}

/// The calls of RUN, each with its result (by place in RUN); a result with no call before it stands alone.
fn calls(run: &[Entry]) -> Vec<(usize, Option<usize>)> {
    let mut out: Vec<(usize, Option<usize>)> = Vec::new();
    let mut waiting: std::collections::VecDeque<usize> = std::collections::VecDeque::new(); // places in OUT of calls without their result yet
    for (i, e) in run.iter().enumerate() {
        if e.kind == "result" {
            match waiting.pop_front() {
                Some(at) => out[at].1 = Some(i),
                None => out.push((i, None)),
            }
        } else {
            waiting.push_back(out.len());
            out.push((i, None));
        }
    }
    out
}

/// The first line of TEXT, at most N characters.
fn gist(text: &str, n: usize) -> String {
    let line = text.trim().lines().next().unwrap_or("");
    if line.chars().count() > n { format!("{}…", line.chars().take(n).collect::<String>()) } else { line.to_string() }
}


/// A run's line: working (spinner, the call it is on) when LIVE, else its summary; a click opens or closes it.
/// Its times are written as every time is (when.rs).
pub fn run_line(s: &App, c: &Conv, from: usize, to: usize, live: bool, k: &Kit) -> El {
    let run = &c.entries[from..to];
    let tools: Vec<&Entry> = run.iter().filter(|e| e.kind != "result").collect();
    let key = run_key(c, from);
    let open = s.expanded.contains(&key);
    let mut line = row().w_full().gap(8.0).center_y().child(text(if open { "▾" } else { "▸" }).font(k.ty.small).color(k.c.text_faint));
    if live {
        let on = tools.last().map_or(String::new(), |e| format!("{} · {}", e.tool.as_deref().unwrap_or("tool"), gist(&e.text, GIST)));
        line = line
            .child(Spinner::new().size(12.0).build(k))
            .child(text("working").font(k.ty.small_medium).color(k.c.text_muted))
            .child(text(on).font(k.ty.mono).color(k.c.text_faint).truncate().grow(1.0).shrink());
    } else {
        let (n, by, when) = run_words(run);
        line = line
            .child(text(n).font(k.ty.small_medium).color(k.c.text_muted))
            .child(text(by).font(k.ty.small).color(k.c.text_faint).truncate().grow(1.0).shrink())
            .child(text(when).font(k.ty.small).color(k.c.text_faint));
    }
    k.interactive(col().w_full().px(24.0).py(6.0).child(line)).on_press(move |s: &mut App| toggle(s, &key))
}

/// A run summed up in words: how many calls, of which tools (the most used first), over what time.
fn run_words(run: &[Entry]) -> (String, String, String) {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for e in run.iter().filter(|e| e.kind != "result") {
        let t = e.tool.as_deref().unwrap_or("tool");
        match counts.iter_mut().find(|(n, _)| *n == t) {
            Some((_, n)) => *n += 1,
            None => counts.push((t, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    let by = counts.iter().map(|(t, n)| if *n > 1 { format!("{t} ×{n}") } else { t.to_string() }).collect::<Vec<_>>().join(", ");
    let n: usize = counts.iter().map(|(_, n)| n).sum();
    let when = crate::when::range(&run[0].at, &run[run.len() - 1].at);
    (format!("{n} tool call{}", if n == 1 { "" } else { "s" }), by, when)
}

/// A run's line read only: its words ([`run_words`]) and no press, as the old line's runs in a fold (chat.rs
/// `old_rows`; path fix 6 point 3).
pub fn run_sum(run: &[Entry], k: &Kit) -> El {
    let (n, by, when) = run_words(run);
    row()
        .w_full()
        .gap(8.0)
        .center_y()
        .px(24.0)
        .py(4.0)
        .child(text(n).font(k.ty.small_medium).color(k.c.text_muted))
        .child(text(by).font(k.ty.small).color(k.c.text_faint).truncate().grow(1.0).shrink())
        .child(text(when).font(k.ty.small).color(k.c.text_faint))
}

/// A call's line in an open run: its tool and the first characters of what it ran; a click shows it in full.
pub fn call_line(s: &App, c: &Conv, call: usize, result: Option<usize>, k: &Kit) -> El {
    let e = &c.entries[call];
    let key = call_key(c, call);
    let open = s.expanded.contains(&key);
    let (tool, what) = if e.kind == "result" { ("result".to_string(), gist(&e.text, 80)) } else { (e.tool.clone().unwrap_or_else(|| "tool".into()), gist(&e.text, 80)) };
    let lines = result.map(|r| c.entries[r].text.lines().count());
    let mut line = row()
        .w_full()
        .gap(8.0)
        .center_y()
        .child(text(if open { "▾" } else { "▸" }).font(k.ty.small).color(k.c.text_faint))
        .child(text(tool).font(k.ty.small_medium).color(k.c.text_muted))
        .child(text(what).font(k.ty.mono).color(k.c.text_base).truncate().grow(1.0).shrink());
    if let Some((added, removed)) = std::iter::once(call).chain(result).find_map(|i| c.entries[i].patch.as_deref()).map(changed) {
        line = line
            .child(text(format!("+{added}")).font(k.ty.small_medium).color(k.c.success.fg))
            .child(text(format!("−{removed}")).font(k.ty.small_medium).color(k.c.danger.fg));
    } else if let Some(n) = lines {
        line = line.child(text(format!("{n} line{}", if n == 1 { "" } else { "s" })).font(k.ty.small).color(k.c.text_faint));
    }
    k.interactive(col().w_full().pl(44.0).pr(24.0).py(3.0).child(line)).on_press(move |s: &mut App| toggle(s, &key))
}

/// The lines a unified diff adds and removes (file headers not counted).
fn changed(patch: &str) -> (usize, usize) {
    let lines = || patch.lines().filter(|l| !l.starts_with("+++ ") && !l.starts_with("--- "));
    (lines().filter(|l| l.starts_with('+')).count(), lines().filter(|l| l.starts_with('-')).count())
}

pub(crate) fn toggle(s: &mut App, key: &str) {
    if !s.expanded.remove(key) {
        s.expanded.insert(key.to_string());
    }
}

/// The keys to open so entry I of C shows (its run's and its call's), for a seek to reach it.
pub fn reveal(c: &Conv, i: usize, asked: impl Fn(&str) -> bool) -> Vec<String> {
    if i >= c.entries.len() || !is_work(&c.entries[i], &asked) {
        return Vec::new();
    }
    let mut from = i;
    while from > 0 && is_work(&c.entries[from - 1], &asked) {
        from -= 1;
    }
    let mut to = i;
    while to < c.entries.len() && is_work(&c.entries[to], &asked) {
        to += 1;
    }
    let mut keys = vec![run_key(c, from)];
    if let Some((call, _)) = calls(&c.entries[from..to]).into_iter().find(|(a, b)| from + a == i || b.is_some_and(|b| from + b == i)) {
        keys.push(call_key(c, from + call));
    }
    keys
}
