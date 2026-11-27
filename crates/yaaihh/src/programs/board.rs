//! A stack drawn (N-1988): on the left what starts each rule (the harness moments, the Events, Acts and Effects, each
//! near its rules), in the middle each text in load order with its rules as rows (a bridge with a line for each harness
//! version it states), on the right what the rules do; an arrow from each start to each rule it starts and from each
//! rule to each thing it does. What the engine's record says is written on them: how often a moment came, how often an
//! act was performed or refused, how many steps wait on an Effect, how many of a Duty hold. A press lights a thing's or
//! a rule's arrows and opens its text beside the drawing.

use super::check::Lets;
use super::read::{Is, Node, Row, Rule};
use super::{Built, Live};
use crate::App;
use crate::picture::graph::{Board, R, Way, Wire};
use rune::{Color, El, row, text};
use rune_kit::Kit;
use serde_json::Value as Json;
use std::collections::BTreeMap;

const SIDE_W: f32 = 200.0;
const GAP: f32 = 80.0;
const MID_W: f32 = 380.0;
const NODE_H: f32 = 24.0;
const ROW_H: f32 = 22.0;
const HEAD_H: f32 = 30.0;
const PAD: f32 = 8.0;
const BOX_GAP: f32 = 12.0;
/// Where the drawing starts under the columns' names.
const TOP: f32 = 28.0;

const MID_X: f32 = SIDE_W + GAP;
const RIGHT_X: f32 = MID_X + MID_W + GAP;

/// The key a rule is picked by.
pub fn row_key(r: &Row) -> String {
    format!("row:{}:{}", r.module, r.line)
}

/// What the record says of thing N, each with its ink: none, without the engine.
fn counts(n: &Node, live: Option<&Live>, k: &Kit) -> Vec<(String, Color)> {
    let Some(l) = live else { return Vec::new() };
    let mut out = Vec::new();
    match n.is {
        Is::Moment => {
            if let Some((c, _)) = n.key.strip_prefix("moment:").and_then(|m| l.tally.reached.get(m)) {
                out.push((format!("came {c}"), k.c.text_accent));
            }
        }
        Is::Event | Is::Act => {
            if let Some((c, _)) = l.tally.performed.get(&n.key) {
                out.push((format!("{c}×"), k.c.text_accent));
            }
            if let Some(c) = l.tally.refused.get(&n.key) {
                out.push((format!("{c} refused"), k.c.danger.fg));
            }
        }
        Is::Effect => {
            let w = l.tally.waiting(&n.key);
            if w > 0 {
                out.push((format!("{w} waiting"), k.c.warning.fg));
            }
        }
        Is::Answer => {}
    }
    out
}

/// How many of Duty R hold now, once the engine has said.
fn holding(r: &Row, live: Option<&Live>) -> Option<usize> {
    match (live?.holding)(&r.name)? {
        Ok(Json::Array(a)) => Some(a.len()),
        _ => None,
    }
}

fn kind_word(is: Is) -> &'static str {
    match is {
        Is::Moment => "moment",
        Is::Event => "Event",
        Is::Act => "Act",
        Is::Effect => "Effect",
        Is::Answer => "answer",
    }
}

/// The counts as small words, each in its ink.
fn chips(cs: &[(String, Color)], k: &Kit) -> Vec<El> {
    cs.iter().map(|(w, c)| text(w.clone()).font(k.ty.small_medium).color(*c)).collect()
}

/// NODES placed in a column at X, each near the middle of the rows it is drawn to (AT), in order, none over another.
fn column(nodes: &[&Node], at: &BTreeMap<&str, Vec<f32>>) -> Vec<R> {
    let mut y = TOP;
    nodes
        .iter()
        .map(|n| {
            let ys = at.get(n.key.as_str()).map_or(&[][..], Vec::as_slice);
            let want = if ys.is_empty() { y } else { ys.iter().sum::<f32>() / ys.len() as f32 - NODE_H / 2.0 };
            let r = R::new(0.0, want.max(y), SIDE_W, NODE_H);
            y = r.bottom() + 6.0;
            r
        })
        .collect()
}

/// Stack B as one element.
pub fn el(s: &App, b: &Built, live: Option<&Live>, k: &Kit) -> El {
    let d = &b.checked.drawing;
    let picked = super::picked(s, super::PICK);
    let mut g = Board::new();
    for (x, w) in [(0.0, "What starts it"), (MID_X, "The rules, text by text"), (RIGHT_X, "What it does")] {
        g.words(x, 0.0, w, k.ty.small_medium, k.c.text_muted);
    }
    // the middle: each text a box, its rules a row each, a bridge's versions a line each
    let lets = Lets::of(&b.checked.stated);
    let mut mids: Vec<f32> = vec![0.0; d.rows.len()];
    let mut y = TOP;
    for (m, _) in &b.checked.modules {
        let rows: Vec<(usize, &Row)> = d.rows.iter().enumerate().filter(|(_, r)| r.module == *m).collect();
        let mut versions: Vec<(String, String, u32)> = Vec::new();
        for st in b.checked.stated.iter().filter(|st| st.module == *m && matches!(st.fact.as_str(), "std.hooks.Reaches" | "std.hooks.Reaches-In")) {
            if let [h, v, ..] = st.args.as_slice() {
                if !versions.iter().any(|(a, c, _)| a == h && c == v) {
                    versions.push((h.clone(), v.clone(), st.line));
                }
            }
        }
        let lines = rows.len() + versions.len();
        let r = R::new(MID_X, y, MID_W, HEAD_H + lines as f32 * ROW_H + PAD);
        g.frame(r, rune::el().radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_weak));
        let what = match (rows.len(), versions.len()) {
            (0, 0) => "its facts and functions; no rule".to_string(),
            (0, v) => format!("{v} harness version{}", if v == 1 { "" } else { "s" }),
            (n, _) => format!("{n} rule{}", if n == 1 { "" } else { "s" }),
        };
        let path = b.path(m);
        let head = row().w_full().h_full().px(PAD).gap(8.0).center_y().child(text(m.clone()).font(k.ty.small_medium).color(k.c.text_base)).child(text(b.short(&path).to_string()).font(k.ty.small).color(k.c.text_faint).truncate().shrink()).child(rune::spacer()).child(text(what).font(k.ty.small).color(k.c.text_muted));
        g.put(R::new(MID_X, y, MID_W, HEAD_H), k.interactive(head.key(format!("programs-module-{m}")).radius(k.radius.md)).on_press(move |s: &mut App| super::show(s, Some((&path, 0)))));
        let mut ly = y + HEAD_H;
        for (i, rw) in rows {
            mids[i] = ly + ROW_H / 2.0;
            let key = row_key(rw);
            let lit = picked == Some(key.as_str());
            let mut line = row().w_full().h_full().px(PAD).gap(8.0).center_y().radius(k.radius.sm);
            line = line.child(text(rw.words.clone()).font(k.ty.small).color(if lit { k.c.text_accent } else { k.c.text_base }).truncate().shrink()).child(text(rw.line.to_string()).font(k.ty.small).color(k.c.text_faint)).child(rune::spacer());
            if rw.rule == Rule::Duty {
                if let Some(n) = holding(rw, live) {
                    line = line.child(text(format!("{n} hold")).font(k.ty.small_medium).color(if n > 0 { k.c.warning.fg } else { k.c.text_faint }));
                    if n > 0 {
                        line = line.fill(k.c.warning.bg);
                    }
                }
            }
            if lit {
                line = line.fill(k.c.layer_03);
            }
            let (path, l) = (b.path(&rw.module), rw.line);
            g.put(R::new(MID_X + 2.0, ly, MID_W - 4.0, ROW_H), k.interactive(line.key(format!("programs-row-{}-{}", rw.module, rw.line))).on_press(move |s: &mut App| super::pick(s, &key, &path, l)));
            ly += ROW_H;
        }
        for (h, v, at) in versions {
            let moments = lets.reaches.get(&(h.clone(), v.clone())).map_or(0, |m| m.len());
            let powers: Vec<&str> = lets.lets.iter().filter(|(a, c, _, _)| *a == h && *c == v).map(|(_, _, _, p)| p.as_str()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
            let lets_words = if powers.is_empty() { "lets nothing".to_string() } else { format!("lets {}", powers.join(", ")) };
            let words = format!("{h} {v} · comes to {moments} moments · {lets_words}");
            let path = b.path(m);
            let line = row().w_full().h_full().px(PAD).center_y().child(text(words).font(k.ty.small).color(k.c.text_muted).truncate().shrink());
            g.put(R::new(MID_X + 2.0, ly, MID_W - 4.0, ROW_H), k.interactive(line.key(format!("programs-version-{h}-{v}")).radius(k.radius.sm)).on_press(move |s: &mut App| super::show(s, Some((&path, at)))));
            ly += ROW_H;
        }
        y = r.bottom() + BOX_GAP;
    }
    // the sides: each thing near the middle of the rows it is drawn to, those drawn to none last
    let near = |side: fn(&Row) -> &Vec<String>| {
        let mut at: BTreeMap<&str, Vec<f32>> = BTreeMap::new();
        for (i, r) in d.rows.iter().enumerate() {
            for key in side(r) {
                at.entry(key.as_str()).or_default().push(mids[i]);
            }
        }
        at
    };
    let (from_at, to_at) = (near(|r| &r.from), near(|r| &r.to));
    let bary = |at: &BTreeMap<&str, Vec<f32>>, n: &Node| at.get(n.key.as_str()).map_or(f32::MAX, |ys| ys.iter().sum::<f32>() / ys.len() as f32);
    let mut left: Vec<&Node> = d.from.iter().collect();
    left.sort_by(|a, c| bary(&from_at, a).total_cmp(&bary(&from_at, c)));
    let mut right: Vec<&Node> = d.to.iter().collect();
    right.sort_by(|a, c| bary(&to_at, a).total_cmp(&bary(&to_at, c)));
    // an Act that starts a Duty and that a reaction does stands on both sides, each its own key and place
    let mut place: [BTreeMap<&str, R>; 2] = Default::default();
    for (side, nodes, x, at) in [(0, &left, 0.0, &from_at), (1, &right, RIGHT_X, &to_at)] {
        for (n, r) in nodes.iter().zip(column(nodes, at)) {
            let r = R::new(x, r.y, r.w, r.h);
            place[side].insert(n.key.as_str(), r);
            g.put(r, node(b, n, live, picked == Some(n.key.as_str()), ["node", "does"][side], k));
        }
    }
    // the arrows, the lit ones last so they lie over the others
    let mut wires: Vec<(bool, Wire)> = Vec::new();
    for (i, r) in d.rows.iter().enumerate() {
        let rk = row_key(r);
        for f in &r.from {
            if let Some(a) = place[0].get(f.as_str()) {
                let lit = picked == Some(rk.as_str()) || picked == Some(f.as_str());
                wires.push((lit, Wire::new(Way::across(a.right_mid(), [MID_X, mids[i]]), if lit { k.c.text_accent } else { k.c.border_base }, if lit { 2.0 } else { 1.0 })));
            }
        }
        for t in &r.to {
            if let Some(a) = place[1].get(t.as_str()) {
                let lit = picked == Some(rk.as_str()) || picked == Some(t.as_str());
                wires.push((lit, Wire::new(Way::across([MID_X + MID_W, mids[i]], a.left_mid()), if lit { k.c.text_accent } else { k.c.border_base }, if lit { 2.0 } else { 1.0 })));
            }
        }
    }
    wires.sort_by_key(|(lit, _)| *lit);
    for (_, w) in wires {
        g.wire(w);
    }
    g.el("programs-board")
}

/// Thing N as a box on SIDE (`node` on the left, `does` on the right): what it is, its name, what the record says of
/// it; lit when picked or when the record says something came or waits. A press lights its arrows and shows its
/// declaration.
fn node(b: &Built, n: &Node, live: Option<&Live>, picked: bool, side: &str, k: &Kit) -> El {
    let cs = counts(n, live, k);
    let border = if picked {
        k.c.border_focus
    } else if !cs.is_empty() {
        k.c.text_accent
    } else {
        k.c.border_weak
    };
    let fill = if n.is == Is::Answer { k.c.info.bg } else { k.c.layer_02 };
    let mut words = n.words.clone();
    if let Some(p) = &n.needs {
        words = format!("{words} · needs {p}");
    }
    let el = row()
        .w_full()
        .h_full()
        .px(PAD)
        .gap(6.0)
        .center_y()
        .radius(k.radius.sm)
        .fill(fill)
        .border(if picked { 2.0 } else { 1.0 }, border)
        .child(text(kind_word(n.is)).font(k.ty.small).color(k.c.text_faint))
        .child(text(words).font(k.ty.small).color(k.c.text_base).truncate().shrink())
        .child(rune::spacer())
        .children(chips(&cs, k))
        .key(format!("programs-{side}-{}", n.key));
    let (key, path, line) = (n.key.clone(), b.path(&n.module), n.line);
    if n.module.is_empty() {
        return el;
    }
    k.interactive(el).on_press(move |s: &mut App| super::pick(s, &key, &path, line))
}
