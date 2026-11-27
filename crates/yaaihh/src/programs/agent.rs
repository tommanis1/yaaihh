//! An agent's own part of the harness's programs, in its Inspect drawer (N-1988): the bridge its harness runs under in
//! the texts coordd-v2 runs in the release, and for each version that bridge states, each moment: the harness's own
//! hook for it, whether it comes, what may be answered there, and each rule that answers it there or is skipped for want
//! of a power; how his words reach it (its bridge's delivery section, N-1978); and on coordd (v1), a press to its part
//! of the Docket.

use super::check::Lets;
use super::read::{Is, Row, Rule};
use super::Built;
use crate::App;
use coord_types::AgentView;
use rune::{El, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit};

/// The bridge module and harness name of an agent's kind (AgentView's `kind`).
pub fn bridge_of(kind: &str) -> Option<(&'static str, &'static str)> {
    Some(match kind {
        "claude" => ("std.bridges.claude_code", "claude-code"),
        "codex" => ("std.bridges.codex", "codex"),
        "agy" => ("std.bridges.agy", "agy"),
        "opencode" => ("std.bridges.opencode", "opencode"),
        _ => return None,
    })
}

/// What rule R does at a moment of a version that lets LETS there: answers, is skipped for a power it tests for, or is
/// not started for one it does not.
#[derive(Clone, Debug, PartialEq)]
pub enum Fate {
    Answers,
    Skipped(String),
    NotStarted(String),
}

pub fn fate(b: &Built, r: &Row, lets: &[&str]) -> Fate {
    let needs = b.checked.drawing.to.iter().filter(|n| n.is == Is::Answer && r.to.contains(&n.key)).filter_map(|n| n.needs.clone());
    let missing: Vec<String> = needs.filter(|p| !lets.contains(&p.as_str())).collect();
    match missing.iter().find(|p| !r.tests.contains(p)) {
        Some(p) => Fate::NotStarted(p.clone()),
        None => missing.first().map_or(Fate::Answers, |p| Fate::Skipped(p.clone())),
    }
}

/// The comment that LINES open with, as paragraphs: a line indented past the comment's own space is an item of a list
/// (its name and its words apart in the comment, ` — ` between here), one indented deeper goes on with its item, and an
/// empty one ends a paragraph.
fn paragraphs<'w>(lines: impl Iterator<Item = &'w str>) -> Vec<String> {
    let (mut out, mut item, mut open): (Vec<String>, Option<usize>, bool) = (Vec::new(), None, false);
    for l in lines.map_while(|l| l.trim_start().strip_prefix("//")) {
        let at = l.len() - l.trim_start().len();
        let said = l.split_whitespace().collect::<Vec<_>>().join(" ");
        if said.is_empty() {
            (item, open) = (None, false);
            continue;
        }
        let goes_on = open && item.map_or(at < 2, |n| at > n);
        if let Some(last) = out.last_mut().filter(|_| goes_on) {
            last.push(' ');
            last.push_str(&said);
            continue;
        }
        if at >= 2 {
            let line = match l.trim().split_once("  ") {
                Some((name, words)) => format!("· {name} — {}", words.split_whitespace().collect::<Vec<_>>().join(" ")),
                None => format!("· {said}"),
            };
            out.push(line);
            item = Some(at);
        } else {
            out.push(said);
            item = None;
        }
        open = true;
    }
    out
}

/// The bridge's own words about its harness: its first comment after its imports.
fn about(words: &str) -> Vec<String> {
    paragraphs(words.lines().skip_while(|l| !l.starts_with("import")).skip_while(|l| l.starts_with("import") || l.trim().is_empty()))
}

/// The bridge's delivery section (`// ===== Delivery: HOW ===`, N-1978): how what waits reaches its harness, the
/// section's words, and the lines its rules stand on, to the next section.
fn delivery(words: &str) -> Option<(String, Vec<String>, std::ops::Range<u32>)> {
    let lines: Vec<&str> = words.lines().collect();
    let at = lines.iter().position(|l| l.starts_with("// =====") && l.contains("Delivery:"))?;
    let how = lines[at].split("Delivery:").nth(1)?.trim_matches(|c: char| c == '=' || c.is_whitespace()).to_string();
    let end = lines[at + 1..].iter().position(|l| l.starts_with("// =====")).map_or(lines.len(), |n| at + 1 + n);
    Some((how, paragraphs(lines[at + 1..].iter().copied()), (at + 2) as u32..(end + 1) as u32))
}

/// Rule R as a press, KEY its own: the Docket page opened on it, the rule brought to the middle of the drawing.
fn link(b: &Built, r: &Row, key: String, k: &Kit) -> El {
    let (pick, path, line, drawn) = (super::board::row_key(r), b.path(&r.module), r.line, format!("programs-row-{}-{}", r.module, r.line));
    let name = text(format!("{} · {}:{}", r.words, r.module, r.line)).font(k.ty.small).color(k.c.text_accent);
    k.interactive(name.key(key)).on_press_tx(move |s: &mut App, tx: &mut rune::Tx| {
        open_rule(s, Some(&pick), &path, line);
        tx.scroll_center(drawn.clone());
    })
}

/// The Docket tab of agent A's drawer.
pub fn tab(s: &App, a: &AgentView, k: &Kit) -> El {
    let small = |t: String| text(t).font(k.ty.small).color(k.c.text_muted).wrap();
    let mut c = col().w_full().gap(8.0).px(16.0).pt(10.0).key(format!("inspect-docket-{}", a.name));
    let b = super::built_in(s);
    let Some((module, harness)) = bridge_of(&a.kind) else {
        return c.child(small(format!("Its harness ({}) has no bridge among the texts coordd-v2 runs.", a.kind)));
    };
    let path = b.path(module);
    let open = Button::new(path.clone()).variant(ButtonVariant::Ghost).on_press(move |s: &mut App| open_rule(s, None, &path, 0)).build(k).key("inspect-docket-bridge");
    c = c.child(row().w_full().gap(8.0).center_y().child(text(format!("Its harness, {harness}, under coordd-v2")).font(k.ty.small_medium).color(k.c.text_base)).child(rune::spacer()).child(open));
    if let Some(w) = b.words(&b.path(module)) {
        c = c.children(about(w).into_iter().map(&small));
    }
    c = c.child(text("As the release runs it: the texts this window was built with.").font(k.ty.small).color(k.c.text_faint));
    let st = &b.checked.stated;
    let lets = Lets::of(st);
    let moments: Vec<&str> = st.iter().filter(|x| x.fact == super::read::MOMENT).filter_map(|x| x.args.first().map(String::as_str)).collect();
    let hook = |m: &str| st.iter().filter(|x| x.fact == "std.hooks.Native-Hook" && x.args.first().map(String::as_str) == Some(harness) && x.args.get(2).map(String::as_str) == Some(m)).filter_map(|x| x.args.get(1).cloned()).collect::<Vec<_>>();
    let rows: Vec<&Row> = b.checked.drawing.rows.iter().filter(|r| r.rule == Rule::On && r.moment.is_some()).collect();
    let mut pushed = std::collections::BTreeSet::new();
    for ((h, v), reached) in lets.reaches.iter().filter(|((h, _), _)| h == harness) {
        c = c.child(row().pt(8.0).child(text(format!("{h} {v}")).font(k.ty.small_medium).color(k.c.text_base)));
        for m in &moments {
            let modes: Vec<&str> = st.iter().filter(|x| x.fact == "std.hooks.Reaches-In" && matches!(x.args.as_slice(), [a, b2, _, mm] if a == h && b2 == v && mm == m)).map(|x| x.args[2].as_str()).collect();
            let always = st.iter().any(|x| x.fact == "std.hooks.Reaches" && matches!(x.args.as_slice(), [a, b2, mm] if a == h && b2 == v && mm == m));
            let at: Vec<&&Row> = rows.iter().filter(|r| r.moment.as_deref() == Some(*m) || r.moment.as_deref() == Some("*")).collect();
            if !reached.contains(*m) && at.is_empty() {
                continue;
            }
            let here: Vec<&str> = lets.lets.iter().filter(|(a, b2, mm, _)| a == h && b2 == v && mm == m).map(|(_, _, _, p)| p.as_str()).collect();
            let comes = match (always, modes.is_empty()) {
                (true, _) => "comes always".to_string(),
                (false, false) => format!("comes only in mode {}", modes.join(", ")),
                (false, true) => "does not come".into(),
            };
            let natives = hook(m);
            let lets_words = if here.is_empty() { "lets nothing".to_string() } else { format!("lets {}", here.join(", ")) };
            let mut said = vec![comes, lets_words];
            if !natives.is_empty() {
                said.insert(0, format!("its hook {}", natives.join(", ")));
            }
            let ink = if reached.contains(*m) { k.c.text_base } else { k.c.text_faint };
            c = c.child(row().w_full().gap(8.0).pl(8.0).child(text(m.to_string()).font(k.ty.mono).color(ink)).child(text(said.join(" · ")).font(k.ty.small).color(k.c.text_muted).wrap().grow(1.0).shrink()));
            for r in at {
                let f = fate(&b, r, &here);
                let (words, ink) = match (&f, reached.contains(*m)) {
                    (_, false) => ("never runs: the moment does not come".to_string(), k.c.text_faint),
                    (Fate::Answers, true) => ("runs when its condition holds".to_string(), k.c.text_base),
                    (Fate::Skipped(p), true) => (format!("skipped: needs {p}, which it tests for"), k.c.text_faint),
                    (Fate::NotStarted(p), true) => (format!("not started: needs {p} and does not test for it"), k.c.danger.fg),
                };
                if *m == "turn_begins" && r.module == "std.comms" && f == Fate::Answers && reached.contains(*m) {
                    pushed.extend(b.checked.drawing.to.iter().filter(|n| n.is == Is::Answer && r.to.contains(&n.key)).map(|n| super::read::short(&n.key).to_string()));
                }
                let name = link(&b, r, format!("inspect-docket-rule-{v}-{m}-{}-{}", r.module, r.line), k);
                c = c.child(row().w_full().gap(8.0).pl(24.0).child(name).child(text(words).font(k.ty.small).color(ink).wrap().grow(1.0).shrink()));
            }
        }
    }
    // how his words reach it: its bridge's delivery section and its rules, else what std.comms answers at a turn's start
    // what a rule does, what reaches the harness (its Effects) first
    let does = |r: &Row| {
        let mut ns: Vec<&super::read::Node> = b.checked.drawing.to.iter().filter(|n| r.to.contains(&n.key)).collect();
        ns.sort_by_key(|n| n.is != Is::Effect);
        ns.into_iter().map(|n| super::read::short(&n.key).to_string()).collect::<Vec<_>>()
    };
    match b.words(&b.path(module)).and_then(delivery) {
        Some((how, said, lines)) => {
            c = c.child(row().pt(10.0).child(text(format!("How his words reach it: {how}")).font(k.ty.small_medium).color(k.c.text_base)));
            c = c.children(said.into_iter().map(&small));
            for r in b.checked.drawing.rows.iter().filter(|r| r.module == module && r.rule == Rule::On && lines.contains(&r.line)) {
                let name = link(&b, r, format!("inspect-docket-delivery-{}-{}", r.module, r.line), k);
                let what = does(r);
                let what = if what.is_empty() { String::new() } else { format!("does {}", what.join(", ")) };
                c = c.child(row().w_full().gap(8.0).pl(8.0).child(name).child(text(what).font(k.ty.small).color(k.c.text_base).wrap().grow(1.0).shrink()));
            }
        }
        None => {
            let reach = if !pushed.is_empty() {
                format!("His words reach it at a turn's start: std.comms answers turn_begins with {}.", pushed.into_iter().collect::<Vec<_>>().join(" or "))
            } else if b.decl("std.comms.Pull").is_some() {
                "Nothing is pushed at a turn's start: what waits for it is read by its own Pull act (std.comms).".to_string()
            } else {
                "Nothing is pushed at a turn's start.".to_string()
            };
            c = c.child(row().pt(10.0).child(text(reach).font(k.ty.small).color(k.c.text_base).wrap()));
        }
    }
    let name = a.name.clone();
    let v1 = Button::new("Its part of the Docket").variant(ButtonVariant::Outline).on_press(move |s: &mut App| crate::picture::docket::open_agent(s, &name)).build(k).key("inspect-docket-v1");
    c.child(row().pt(10.0).gap(8.0).center_y().child(v1).child(small("On coordd (v1): what its kinds' steps owe, at the Agent scale".into())))
}

/// The Docket page with the release's texts drawn, KEY lit and the text at PATH shown at LINE.
fn open_rule(s: &mut App, key: Option<&str>, path: &str, line: u32) {
    crate::picture::docket::set_scale(s, crate::picture::docket::Scale::Project);
    s.sel = crate::Sel::Kinds;
    super::set(s, super::BUILT, Some(String::new()));
    super::set(s, super::PICK, key.map(str::to_string));
    super::show(s, Some((path, line)));
}
