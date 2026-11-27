//! "how it runs" (design-two-systems §16, 4; picture 5): a thread's program as rows drawn from `TicketView.run`, the
//! posts between them by time, with the program's text beside it. Nothing here is written for the owner: a row says
//! what the program and its log say (paper-research m1419): a step its worker, its call and what came back; an inline
//! step its worker and its words; an open the kind and the opened ticket's title, that ticket's own run under it (one
//! level open); a match the arm taken, the others in one faint line; the hole "rests here", the minder's program in a
//! box with its time and Undo, or the result it closed with; a post who and its first line.

use crate::threads::Page;
use crate::{App, Pending};
use coord_types::{Request, TicketView};
use docket_machine::view::RunNode;
use rune::{El, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit};
use serde_json::Value;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(v: Value) -> RunNode {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn a_step_says_its_worker_its_call_and_what_came_back() {
        let s = node(json!({"what": "step", "line": 3, "source": 0, "text": "c <- step fixer repair(text)", "who": "fixer", "state": "done", "value": {"head": "Fixed", "commit": "0ae8f2a91b2c3d4e5f60718293a4b5c6d7e8f901"}}));
        assert_eq!(said(&s), ("fixer · repair → Fixed(0ae8f2a91)".to_string(), String::new()));
        let i = node(json!({"what": "step", "line": 1, "source": 1, "text": "r <- step role lead \"Go ahead. The owner said so.\" -> Done | Cannot(why: Text)", "who": "lead", "state": "now"}));
        assert_eq!(said(&i), ("lead".to_string(), "Go ahead. The owner said so.".to_string()));
        let w = node(json!({"what": "step", "line": 1, "source": 0, "text": "step watcher intake(text, to)", "who": "watcher", "state": "done", "value": {"head": "Thread"}}));
        assert_eq!(said(&w).0, "watcher · intake → Thread");
    }

    #[test]
    fn a_match_draws_the_arm_taken_and_names_the_others() {
        let m = node(json!({"what": "match", "line": 1, "source": 0, "text": "match v", "state": "done", "nodes": [
            {"what": "arm", "line": 2, "source": 0, "text": "Place(t)", "state": "not_taken"},
            {"what": "arm", "line": 3, "source": 0, "text": "Thread", "state": "done", "nodes": [{"what": "post", "line": 4, "source": 0, "text": "post text as owner naming to", "who": "lead", "state": "done"}]},
            {"what": "arm", "line": 5, "source": 0, "text": "Unsure(ts)", "state": "not_taken"}]}));
        let mut rows = Vec::new();
        flat(&m, 0, &mut rows, &[]);
        let said: Vec<String> = rows.iter().map(|r| format!("{} {} {}", r.depth, r.head, r.faint.clone().unwrap_or_default())).collect();
        assert_eq!(said, vec!["0 match v → Thread not taken: Place(t) · Unsure(ts)", "1 post text as owner naming to "]);
    }
}

/// One row of the picture.
struct Row {
    depth: usize,
    state: String,
    head: String,
    /// The words under the head (an inline step's, a post's first line).
    under: String,
    faint: Option<String>,
    who: Option<String>,
    at: Option<String>,
    /// The minder's answer N filled the hole this row opens (the box), with its time.
    minder: Option<u32>,
    /// The hole: rests here; or the result the minder closed with.
    hole: bool,
}

/// The step's call (`intake`) or, for an inline step, its words; and its worker.
fn step_parts(t: &str) -> (Option<String>, Option<String>) {
    let t = t.split_once(" <- ").map_or(t, |(_, r)| r);
    let Some(rest) = t.strip_prefix("step ") else { return (None, None) };
    let rest = rest.strip_prefix("role ").unwrap_or(rest);
    let rest = rest.split_once(' ').map_or("", |(_, r)| r).trim_start();
    if let Some(q) = rest.strip_prefix('"') {
        let mut out = String::new();
        let mut esc = false;
        for c in q.chars() {
            match (esc, c) {
                (true, c) => {
                    out.push(if c == 'n' { '\n' } else { c });
                    esc = false;
                }
                (false, '\\') => esc = true,
                (false, '"') => break,
                (false, c) => out.push(c),
            }
        }
        return (None, Some(out));
    }
    (Some(rest.split(['(', ' ']).next().unwrap_or("").to_string()), None)
}

fn back(v: &Option<Value>) -> String {
    match v {
        Some(Value::Object(o)) => {
            let head = o.get("head").and_then(Value::as_str).unwrap_or("");
            let short: Vec<String> = o
                .iter()
                .filter(|(k, _)| *k != "head")
                .filter_map(|(_, x)| x.as_str())
                .filter(|s| !s.is_empty() && s.len() <= 48 && !s.contains(char::is_whitespace))
                .map(|s| if s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit()) { s[..9].to_string() } else { s.to_string() })
                .collect();
            if short.is_empty() { head.to_string() } else { format!("{head}({})", short.join(", ")) }
        }
        Some(Value::String(s)) => s.lines().next().unwrap_or("").to_string(),
        Some(x) if !x.is_null() => x.to_string(),
        _ => String::new(),
    }
}

/// A node's head and the words under it.
fn said(n: &RunNode) -> (String, String) {
    let came = back(&n.value);
    let arrow = if came.is_empty() { String::new() } else { format!(" → {came}") };
    match n.what.as_str() {
        "step" => match step_parts(&n.text) {
            (Some(call), _) => (format!("{} · {call}{arrow}", n.who.clone().unwrap_or_default()), String::new()),
            (None, Some(words)) => (format!("{}{arrow}", n.who.clone().unwrap_or_default()), words),
            _ => (format!("{}{arrow}", n.text), String::new()),
        },
        _ => (format!("{}{arrow}", n.text), String::new()),
    }
}

/// N and what is under it as rows, at DEPTH; OPENED the tickets of the view, for an open's title and run.
fn flat(n: &RunNode, depth: usize, out: &mut Vec<Row>, opened: &[(String, String, Vec<RunNode>)]) {
    let row_of = |head: String, under: String| Row { depth, state: n.state.clone(), head, under, faint: None, who: n.who.clone(), at: n.at.as_ref().map(|a| a.to_string()), minder: None, hole: false };
    match n.what.as_str() {
        "match" => {
            let taken = n.nodes.iter().find(|a| a.state != "not_taken" && a.state != "to_come");
            let others: Vec<&str> = n.nodes.iter().filter(|a| taken.is_none_or(|t| !std::ptr::eq(*a, t))).map(|a| a.text.as_str()).collect();
            let mut r = row_of(match taken {
                Some(a) => format!("{} → {}", n.text, a.text),
                None => n.text.clone(),
            }, String::new());
            r.who = None;
            if !others.is_empty() {
                r.faint = Some(format!("{}{}", if taken.is_some() { "not taken: " } else { "" }, others.join(" · ")));
            }
            out.push(r);
            if let Some(a) = taken {
                for c in &a.nodes {
                    flat(c, depth + 1, out, opened);
                }
            }
        }
        "hole" => {
            let mut r = row_of(String::new(), String::new());
            r.hole = true;
            r.who = None;
            match n.nodes.first() {
                Some(c) if c.source > 0 => {
                    r.minder = Some(c.source);
                    out.push(r);
                    for c in &n.nodes {
                        flat(c, depth + 1, out, opened);
                    }
                }
                _ => {
                    r.head = match &n.value {
                        Some(_) => format!("the minder closed it → {}", back(&n.value)),
                        None if n.state == "now" => "rests here".into(),
                        None => "…".into(),
                    };
                    out.push(r);
                }
            }
        }
        "open" => {
            let (head, _) = said(n);
            let kind = n.text.split_once("open ").map(|(_, r)| r.split(['(', ' ']).next().unwrap_or("")).unwrap_or("");
            let mut nested = Vec::new();
            for id in &n.tickets {
                if let Some((_, title, run)) = opened.iter().find(|(i, _, _)| i == id) {
                    out.push(Row { faint: None, ..row_of(format!("{kind} {id}{}", back(&n.value).is_empty().then(String::new).unwrap_or_else(|| format!(" → {}", back(&n.value)))), title.clone()) });
                    nested.push(run);
                }
            }
            if nested.is_empty() {
                out.push(row_of(head, String::new()));
            }
            // one level open: the opened ticket's own top statements, not what is under them
            for run in nested {
                for c in run {
                    let (h, u) = said(c);
                    out.push(Row { depth: depth + 1, state: c.state.clone(), head: h, under: u, faint: None, who: c.who.clone(), at: c.at.as_ref().map(|a| a.to_string()), minder: None, hole: c.what == "hole" });
                }
            }
        }
        "arm" | "branch" | "handle" => {
            out.push(row_of(n.text.clone(), String::new()));
            for c in &n.nodes {
                flat(c, depth + 1, out, opened);
            }
        }
        _ => {
            let (h, u) = said(n);
            out.push(row_of(h, u));
            for c in &n.nodes {
                flat(c, depth + 1, out, opened);
            }
        }
    }
}

/// Thread V's picture and its program's text.
pub fn picture(pg: &Page, v: &TicketView, all: &[&TicketView], k: &Kit) -> El {
    let opened: Vec<(String, String, Vec<RunNode>)> = all.iter().map(|t| (t.id.clone(), t.title.clone(), t.run.clone())).collect();
    let mut rows = Vec::new();
    for n in &v.run {
        flat(n, 0, &mut rows, &opened);
    }
    // the posts, between the rows by time: before the first row reached after it, else before what is still to come
    let mut drawn: Vec<El> = Vec::new();
    let mut posts = v.posts.iter().filter(|p| p.visible() && p.moved.is_none()).peekable();
    for r in &rows {
        while let Some(p) = posts.peek() {
            let at = p.at.to_string();
            // a hole resting now is where the thread stands: every post so far is above it, though it was reached first
            let before = match &r.at {
                Some(_) if r.hole && r.state == "now" && r.minder.is_none() => true,
                Some(a) => at.as_str() < a.as_str(),
                None => r.state == "to_come" || r.state == "now",
            };
            if !before {
                break;
            }
            let w = crate::texts::words(pg, &serde_json::to_value(&p.text).unwrap_or(Value::Null));
            drawn.push(post_row(&p.by, &crate::when::at(&at), w.lines().find(|l| !l.trim().is_empty()).unwrap_or(""), k));
            posts.next();
        }
        drawn.push(node_row(v, r, k));
    }
    for p in posts {
        let w = crate::texts::words(pg, &serde_json::to_value(&p.text).unwrap_or(Value::Null));
        drawn.push(post_row(&p.by, &crate::when::at(&p.at.to_string()), w.lines().find(|l| !l.trim().is_empty()).unwrap_or(""), k));
    }
    let mut prog = col().w(420.0).shrink().gap(8.0).padding(12.0).radius(k.radius.md).fill(k.c.layer_01).child(text("THE SAME, AS TEXT").font(k.ty.small_medium).color(k.c.text_muted)).child(text(v.program.trim_end().to_string()).font(k.ty.mono).color(k.c.text_base).wrap_anywhere().selectable(true));
    for s in &v.shaped {
        if let Some(t) = s.answer.get("text").and_then(Value::as_str) {
            let when = crate::when::at(&s.at.to_string());
            let note = if s.undone { ", undone by the owner" } else if s.refused.is_some() { ", refused by the check" } else { "" };
            prog = prog.child(text(format!("// written by the minder at the hole, {when}{note}")).font(k.ty.mono).color(k.c.warning.fg).wrap()).child(text(t.to_string()).font(k.ty.mono).color(if s.undone { k.c.text_faint } else { k.c.text_base }).wrap_anywhere().selectable(true));
        }
    }
    let legend = row().gap(6.0).flex_wrap().children(["you", "watcher", "minder", "an agent", "the program itself"].map(|w| chip(w, k)));
    row().w_full().gap(16.0).child(col().grow(1.0).w(0.0).gap(6.0).child(legend).children(drawn)).child(prog)
}

fn who_colour(who: &str, k: &Kit) -> (rune::Color, rune::Color) {
    match who {
        "you" | "owner" => (k.c.info.bg, k.c.info.fg),
        "watcher" => (k.c.layer_03, k.c.text_accent),
        "minder" => (k.c.warning.bg, k.c.warning.fg),
        "the program itself" | "program" => (k.c.layer_02, k.c.text_muted),
        _ => (k.c.success.bg, k.c.success.fg),
    }
}

fn chip(who: &str, k: &Kit) -> El {
    let (bg, fg) = who_colour(who, k);
    row().px(6.0).py(1.0).radius(k.radius.sm).fill(bg).child(text(who.to_string()).font(k.ty.small).color(fg))
}

/// The done / now / to come mark.
fn mark(state: &str, k: &Kit) -> El {
    let (t, c) = match state {
        "done" => ("✓", k.c.success.fg),
        "now" => ("●", k.c.warning.fg),
        _ => ("○", k.c.text_faint),
    };
    row().w(18.0).child(text(t).font(k.ty.body).color(c))
}

fn post_row(by: &str, when: &str, first: &str, k: &Kit) -> El {
    let who = crate::refs::person(by);
    row().w_full().gap(8.0).center_y().child(mark("done", k)).child(
        col().grow(1.0).w(0.0).gap(2.0).px(10.0).py(6.0).radius(k.radius.sm).border(1.0, k.c.border_weak)
            .child(row().w_full().gap(8.0).child(text(format!("{who} posted · {when}")).font(k.ty.small).color(k.c.text_muted).grow(1.0)).child(chip(&who, k)))
            .child(text(format!("\u{201c}{first}\u{201d}")).font(k.ty.small).color(k.c.text_base).truncate()),
    )
}

fn node_row(v: &TicketView, r: &Row, k: &Kit) -> El {
    let faint = r.state == "to_come";
    let colour = if faint { k.c.text_faint } else { k.c.text_base };
    let mut body = col().grow(1.0).w(0.0).gap(2.0).px(10.0).py(6.0).radius(k.radius.sm);
    body = match r.state.as_str() {
        "now" => body.border(1.0, k.c.warning.border).fill(k.c.layer_01),
        "to_come" => body.border(1.0, k.c.border_weak).border_dash(4.0, 3.0),
        _ => body.fill(k.c.layer_01),
    };
    if let Some(n) = r.minder {
        let s = v.shaped.iter().find(|s| s.n == n);
        let when = s.map(|s| crate::when::at(&s.at.to_string())).unwrap_or_default();
        let last = v.shaped.iter().rposition(|x| !x.undone && x.refused.is_none() && x.answer.get("head").and_then(Value::as_str) != Some("Rest")).is_some_and(|i| v.shaped[i].n == n);
        let t = v.id.clone();
        let mut head = row().w_full().gap(8.0).center_y().child(text(format!("the program stopped here (...); the minder wrote what comes next, {when}")).font(k.ty.small).color(k.c.warning.fg).wrap().grow(1.0).shrink().w(0.0));
        if last {
            head = head.child(Button::new("Undo").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
                s.request_in(s.cur, Request::Undo { t: t.clone() }, Pending::TicketDone { clear: crate::threads::Cleared::Quiet });
            }).build(k));
        }
        return row().w_full().pl(18.0 * r.depth as f32).child(col().w_full().px(10.0).py(6.0).radius(k.radius.sm).border(1.0, k.c.warning.border).child(head));
    }
    let mut top = row().w_full().gap(8.0).child(text(r.head.clone()).font(k.ty.ui).color(if r.hole { k.c.warning.fg } else { colour }).wrap().grow(1.0).shrink());
    if let Some(w) = &r.who {
        top = top.child(chip(w, k));
    }
    body = body.child(top);
    if !r.under.is_empty() {
        body = body.child(text(r.under.clone()).font(k.ty.small).color(k.c.text_muted).wrap());
    }
    if let Some(f) = &r.faint {
        body = body.child(text(f.clone()).font(k.ty.small).color(k.c.text_faint).wrap());
    }
    row().w_full().gap(8.0).pl(18.0 * r.depth as f32).child(mark(&r.state, k)).child(body)
}
